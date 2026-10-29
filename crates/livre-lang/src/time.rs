//! `datetime`, `duration` and `calendarDuration` arrays: storage, arithmetic, display,
//! properties and builtins.

use crate::ast::{BinOp, UnOp};
use crate::display::fmt_g;
use crate::error::{rt, LangError};
use crate::nd::{self, Dims, IndexArg};
use crate::value::{Kind, Matrix, StrArray, Value};

type R<T> = Result<T, LangError>;

const MS_DAY: f64 = 86_400_000.0;
const MS_HOUR: f64 = 3_600_000.0;
const MS_MIN: f64 = 60_000.0;
pub const DEFAULT_DT_FORMAT: &str = "dd-MMM-yyyy HH:mm:ss";
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeKind {
    DateTime,
    Duration,
    CalDuration,
}

/// Elements are `[utc_ms, 0, 0]` (datetime), `[ms, 0, 0]` (duration) or
/// `[months, days, ms]` (calendarDuration). NaT / NaN durations use NaN.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeArray {
    pub dims: Dims,
    pub kind: TimeKind,
    pub data: Vec<[f64; 3]>,
    /// datetime: UTC offset in minutes (`None` = unzoned).
    pub tz: Option<f64>,
    pub tz_name: String,
    /// Display format (datetime pattern, or duration `hh:mm:ss`, `h`, `m`, `s`, `d`, `y`).
    pub format: String,
}

// ---- civil calendar -------------------------------------------------------------------------

/// Days since 1970-01-01 for a proleptic Gregorian date (month 1–12).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    days_from_civil(ny, nm, 1) - days_from_civil(y, m, 1)
}

/// Wall-clock milliseconds from components; months/days/hours overflow like MATLAB.
pub fn wall_ms(y: f64, mo: f64, d: f64, h: f64, mi: f64, s: f64) -> f64 {
    if [y, mo, d, h, mi, s].iter().any(|v| v.is_nan()) {
        return f64::NAN;
    }
    let total_months = y.floor() as i64 * 12 + (mo.floor() as i64 - 1);
    let (yy, mm) = (total_months.div_euclid(12), total_months.rem_euclid(12) + 1);
    let days = days_from_civil(yy, mm, 1) as f64 + (d - 1.0);
    days * MS_DAY + h * MS_HOUR + mi * MS_MIN + s * 1000.0
}

/// Components (y, mo, d, h, mi, s) of wall-clock milliseconds.
pub fn components(ms: f64) -> (i64, i64, i64, i64, i64, f64) {
    let days = (ms / MS_DAY).floor();
    let rem = ms - days * MS_DAY;
    let (y, m, d) = civil_from_days(days as i64);
    let h = (rem / MS_HOUR).floor();
    let mi = ((rem - h * MS_HOUR) / MS_MIN).floor();
    let s = (rem - h * MS_HOUR - mi * MS_MIN) / 1000.0;
    (y, m, d, h as i64, mi as i64, s)
}

fn parse_tz(name: &str) -> R<Option<f64>> {
    let n = name.trim();
    if n.is_empty() {
        return Ok(None);
    }
    let up = n.to_ascii_uppercase();
    if up == "UTC" || up == "GMT" || up == "Z" {
        return Ok(Some(0.0));
    }
    if up == "LOCAL" {
        return Ok(Some(0.0));
    }
    let off = up.strip_prefix("UTC").or_else(|| up.strip_prefix("GMT")).unwrap_or(&up);
    let (sign, rest) = match off.chars().next() {
        Some('+') => (1.0, &off[1..]),
        Some('-') => (-1.0, &off[1..]),
        _ => return rt(format!("Unable to use '{name}' as a time zone. Use 'UTC', 'UTC+HH:mm' or a fixed offset in this version.")),
    };
    let (h, m) = match rest.split_once(':') {
        Some((h, m)) => (h.parse::<f64>().unwrap_or(f64::NAN), m.parse::<f64>().unwrap_or(f64::NAN)),
        None if rest.len() == 4 => (rest[..2].parse::<f64>().unwrap_or(f64::NAN), rest[2..].parse::<f64>().unwrap_or(f64::NAN)),
        None => (rest.parse::<f64>().unwrap_or(f64::NAN), 0.0),
    };
    if h.is_nan() || m.is_nan() {
        return rt(format!("Unable to use '{name}' as a time zone."));
    }
    Ok(Some(sign * (h * 60.0 + m)))
}

fn now_utc_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

/// Formats wall-clock ms with a MATLAB/Unicode datetime pattern.
pub fn format_datetime(ms: f64, pattern: &str) -> String {
    if ms.is_nan() {
        return "NaT".into();
    }
    let (y, mo, d, h, mi, s) = components(ms);
    let wd = (days_from_civil(y, mo, d) + 4).rem_euclid(7) as usize; // 1970-01-01 was a Thursday
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                out.push(chars[i]);
                i += 1;
            }
            i += 1;
            continue;
        }
        let mut n = 1;
        while i + n < chars.len() && chars[i + n] == c {
            n += 1;
        }
        let piece = match c {
            'y' | 'u' => if n == 2 { format!("{:02}", y.rem_euclid(100)) } else { format!("{:0w$}", y, w = n.max(4)) },
            'M' | 'L' => match n {
                1 => mo.to_string(),
                2 => format!("{mo:02}"),
                3 => MONTHS[(mo - 1) as usize][..3].to_string(),
                _ => MONTHS[(mo - 1) as usize].to_string(),
            },
            'd' => if n == 1 { d.to_string() } else { format!("{d:02}") },
            'D' => {
                let doy = days_from_civil(y, mo, d) - days_from_civil(y, 1, 1) + 1;
                format!("{:0w$}", doy, w = n)
            }
            'E' => if n <= 3 { WEEKDAYS[wd][..3].to_string() } else { WEEKDAYS[wd].to_string() },
            'H' => if n == 1 { h.to_string() } else { format!("{h:02}") },
            'h' => {
                let h12 = if h % 12 == 0 { 12 } else { h % 12 };
                if n == 1 { h12.to_string() } else { format!("{h12:02}") }
            }
            'm' => if n == 1 { mi.to_string() } else { format!("{mi:02}") },
            's' => if n == 1 { (s.floor() as i64).to_string() } else { format!("{:02}", s.floor() as i64) },
            'S' => {
                let frac = s - s.floor();
                let digits = format!("{:.*}", n, frac);
                digits.trim_start_matches('0').trim_start_matches('.').chars().take(n).collect()
            }
            'a' => (if h < 12 { "AM" } else { "PM" }).to_string(),
            'Q' => format!("Q{}", (mo - 1) / 3 + 1),
            _ => std::iter::repeat(c).take(n).collect(),
        };
        out.push_str(&piece);
        i += n;
    }
    out
}

/// Parses text using a datetime pattern; returns wall-clock ms.
fn parse_with_format(text: &str, pattern: &str) -> Option<f64> {
    let t: Vec<char> = text.trim().chars().collect();
    let p: Vec<char> = pattern.chars().collect();
    let (mut y, mut mo, mut d, mut h, mut mi, mut s) = (1970.0, 1.0, 1.0, 0.0, 0.0, 0.0);
    let mut pm: Option<bool> = None;
    let (mut i, mut j) = (0, 0);
    let num = |t: &[char], j: &mut usize, max: usize| -> Option<f64> {
        let start = *j;
        while *j < t.len() && *j - start < max && (t[*j].is_ascii_digit() || (t[*j] == '.' && *j > start)) {
            *j += 1;
        }
        if *j == start {
            return None;
        }
        t[start..*j].iter().collect::<String>().parse().ok()
    };
    while i < p.len() {
        let c = p[i];
        let mut n = 1;
        while i + n < p.len() && p[i + n] == c {
            n += 1;
        }
        match c {
            'y' | 'u' => {
                let v = num(&t, &mut j, if n == 2 { 2 } else { 4 })?;
                y = if n == 2 { 2000.0 + v } else { v };
            }
            'M' if n >= 3 => {
                let word: String = t[j..].iter().take_while(|c| c.is_alphabetic()).collect();
                let k = MONTHS.iter().position(|m| m.to_lowercase().starts_with(&word.to_lowercase()) && word.len() >= 3)?;
                mo = (k + 1) as f64;
                j += word.chars().count();
            }
            'M' => mo = num(&t, &mut j, 2)?,
            'd' => d = num(&t, &mut j, 2)?,
            'H' | 'h' => h = num(&t, &mut j, 2)?,
            'm' => mi = num(&t, &mut j, 2)?,
            's' => s = num(&t, &mut j, 2)?,
            'S' => {
                let start = j;
                while j < t.len() && t[j].is_ascii_digit() {
                    j += 1;
                }
                let digits: String = t[start..j].iter().collect();
                s += format!("0.{digits}").parse::<f64>().unwrap_or(0.0);
            }
            'a' => {
                let word: String = t[j..].iter().take(2).collect();
                pm = Some(word.eq_ignore_ascii_case("PM"));
                j += 2;
            }
            'E' => {
                while j < t.len() && t[j].is_alphabetic() {
                    j += 1;
                }
            }
            '\'' => {
                i += 1;
                while i < p.len() && p[i] != '\'' {
                    if t.get(j) != Some(&p[i]) {
                        return None;
                    }
                    i += 1;
                    j += 1;
                }
                i += 1;
                continue;
            }
            lit => {
                for _ in 0..n {
                    if t.get(j) != Some(&lit) {
                        return None;
                    }
                    j += 1;
                }
            }
        }
        i += n;
    }
    if j != t.len() {
        return None;
    }
    if let Some(pm) = pm {
        if pm && h < 12.0 {
            h += 12.0;
        } else if !pm && h == 12.0 {
            h = 0.0;
        }
    }
    Some(wall_ms(y, mo, d, h, mi, s))
}

const AUTO_FORMATS: [&str; 12] = [
    "yyyy-MM-dd HH:mm:ss.SSS",
    "yyyy-MM-dd HH:mm:ss",
    "yyyy-MM-dd'T'HH:mm:ss",
    "yyyy-MM-dd HH:mm",
    "yyyy-MM-dd",
    "dd-MMM-yyyy HH:mm:ss",
    "dd-MMM-yyyy HH:mm",
    "dd-MMM-yyyy",
    "MM/dd/yyyy HH:mm:ss",
    "MM/dd/yyyy",
    "yyyy/MM/dd",
    "dd.MM.yyyy",
];

fn parse_auto(text: &str) -> Option<f64> {
    let t = text.trim();
    let t = t.strip_suffix('Z').unwrap_or(t);
    AUTO_FORMATS.iter().find_map(|f| parse_with_format(t, f))
}

// ---- array model ---------------------------------------------------------------------------

impl TimeArray {
    pub fn datetimes(dims: Dims, utc: Vec<f64>, tz: Option<f64>, tz_name: String, format: String) -> Self {
        Self { dims: nd::normalize(dims), kind: TimeKind::DateTime, data: utc.into_iter().map(|x| [x, 0.0, 0.0]).collect(), tz, tz_name, format }
    }

    pub fn durations(dims: Dims, ms: Vec<f64>, format: &str) -> Self {
        Self { dims: nd::normalize(dims), kind: TimeKind::Duration, data: ms.into_iter().map(|x| [x, 0.0, 0.0]).collect(), tz: None, tz_name: String::new(), format: format.into() }
    }

    pub fn calendar(dims: Dims, parts: Vec<[f64; 3]>) -> Self {
        Self { dims: nd::normalize(dims), kind: TimeKind::CalDuration, data: parts, tz: None, tz_name: String::new(), format: String::new() }
    }

    pub fn class_name(&self) -> &'static str {
        match self.kind {
            TimeKind::DateTime => "datetime",
            TimeKind::Duration => "duration",
            TimeKind::CalDuration => "calendarDuration",
        }
    }

    fn offset_ms(&self) -> f64 {
        self.tz.unwrap_or(0.0) * MS_MIN
    }

    /// Wall-clock ms of element `i` (datetime).
    pub fn wall(&self, i: usize) -> f64 {
        self.data[i][0] + self.offset_ms()
    }

    pub fn ms(&self) -> Vec<f64> {
        self.data.iter().map(|x| x[0]).collect()
    }

    pub fn with_ms(&self, dims: Dims, ms: Vec<f64>) -> Self {
        Self { dims: nd::normalize(dims), data: ms.into_iter().map(|x| [x, 0.0, 0.0]).collect(), ..self.clone() }
    }

    pub fn empty_like(&self) -> Self {
        Self { dims: vec![0, 0], data: Vec::new(), ..self.clone() }
    }

    pub fn gather(&self, pos: &[usize], dims: Dims) -> Self {
        Self { dims: nd::normalize(dims), data: pos.iter().map(|&p| self.data[p]).collect(), ..self.clone() }
    }

    fn display_pattern(&self) -> String {
        if self.format == DEFAULT_DT_FORMAT {
            // The default format omits the time when every element is at midnight.
            let all_midnight = (0..self.data.len()).all(|i| {
                let w = self.wall(i);
                w.is_nan() || (w.rem_euclid(MS_DAY)).abs() < 0.5
            });
            if all_midnight && !self.data.is_empty() {
                return "dd-MMM-yyyy".into();
            }
        }
        self.format.clone()
    }

    pub fn format_elem(&self, i: usize) -> String {
        match self.kind {
            TimeKind::DateTime => format_datetime(self.wall(i), &self.display_pattern()),
            TimeKind::Duration => format_duration(self.data[i][0], &self.format),
            TimeKind::CalDuration => format_caldur(self.data[i]),
        }
    }

    pub fn summary(&self) -> String {
        if self.data.len() == 1 {
            self.format_elem(0)
        } else {
            format!("{} {}", nd::size_text(&self.dims), self.class_name())
        }
    }

    fn grid(&self) -> String {
        let rows = self.dims[0];
        let cols: usize = self.dims[1..].iter().product();
        let items: Vec<String> = (0..self.data.len()).map(|i| self.format_elem(i)).collect();
        let w = items.iter().map(|s| s.chars().count()).max().unwrap_or(0);
        let mut out = String::new();
        for r in 0..rows {
            for c in 0..cols {
                out.push_str(&format!("   {:>w$}", items[c * rows + r]));
            }
            out.push('\n');
        }
        out.trim_end_matches('\n').to_string()
    }

    pub fn display(&self, name: &str) -> String {
        if self.data.len() == 1 {
            return format!("{name} = {}\n   {}", self.class_name(), self.format_elem(0));
        }
        if self.data.is_empty() {
            return format!("{name} = \n  {} empty {} array", nd::size_text(&self.dims), self.class_name());
        }
        format!("{name} = {} {} array\n\n{}", nd::size_text(&self.dims), self.class_name(), self.grid())
    }

    pub fn disp(&self) -> String {
        if self.data.len() == 1 {
            return format!("   {}", self.format_elem(0));
        }
        self.grid()
    }

    pub fn assign(mut self, idx: &[IndexArg], rhs: &Value) -> R<Self> {
        let r = coerce_like(&self, rhs)?;
        let fill = [f64::NAN, 0.0, 0.0];
        let rd = r.dims.clone();
        nd::assign(&mut self.dims, &mut self.data, idx, &rd, r.data, fill)?;
        Ok(self)
    }

    pub fn delete(mut self, idx: &[IndexArg]) -> R<Self> {
        nd::delete(&mut self.dims, &mut self.data, idx)?;
        Ok(self)
    }

    fn component(&self, f: impl Fn(f64) -> f64) -> Value {
        Value::Num(Matrix::from_dims(self.dims.clone(), (0..self.data.len()).map(|i| f(self.wall(i))).collect()))
    }

    pub fn get_prop(&self, name: &str) -> R<Value> {
        match (self.kind, name) {
            (TimeKind::DateTime, "Year") => Ok(self.component(|w| if w.is_nan() { f64::NAN } else { components(w).0 as f64 })),
            (TimeKind::DateTime, "Month") => Ok(self.component(|w| if w.is_nan() { f64::NAN } else { components(w).1 as f64 })),
            (TimeKind::DateTime, "Day") => Ok(self.component(|w| if w.is_nan() { f64::NAN } else { components(w).2 as f64 })),
            (TimeKind::DateTime, "Hour") => Ok(self.component(|w| if w.is_nan() { f64::NAN } else { components(w).3 as f64 })),
            (TimeKind::DateTime, "Minute") => Ok(self.component(|w| if w.is_nan() { f64::NAN } else { components(w).4 as f64 })),
            (TimeKind::DateTime, "Second") => Ok(self.component(|w| if w.is_nan() { f64::NAN } else { components(w).5 })),
            (TimeKind::DateTime, "TimeZone") => Ok(Value::chars(&self.tz_name)),
            (_, "Format") => Ok(Value::chars(&self.format)),
            _ => rt(format!("Unrecognized property '{name}' for class '{}'.", self.class_name())),
        }
    }

    pub fn set_prop(mut self, name: &str, v: &Value) -> R<Self> {
        match name {
            "Format" => self.format = v.text()?,
            "TimeZone" if self.kind == TimeKind::DateTime => {
                let tz_name = v.text()?;
                let new_tz = parse_tz(&tz_name)?;
                if self.tz.is_none() {
                    // Unzoned values keep their wall-clock time when a zone is assigned.
                    let off = new_tz.unwrap_or(0.0) * MS_MIN;
                    for e in self.data.iter_mut() {
                        e[0] -= off;
                    }
                }
                self.tz = new_tz;
                self.tz_name = tz_name;
            }
            "Year" | "Month" | "Day" | "Hour" | "Minute" | "Second" if self.kind == TimeKind::DateTime => {
                let vals = v.to_matrix()?;
                for i in 0..self.data.len() {
                    let w = self.wall(i);
                    let (y, mo, d, h, mi, s) = components(w);
                    let x = if vals.numel() == 1 { vals.data[0] } else { vals.data[i] };
                    let (mut c, off) = ([y as f64, mo as f64, d as f64, h as f64, mi as f64, s], self.offset_ms());
                    let k = ["Year", "Month", "Day", "Hour", "Minute", "Second"].iter().position(|n| *n == name).unwrap();
                    c[k] = x;
                    self.data[i][0] = wall_ms(c[0], c[1], c[2], c[3], c[4], c[5]) - off;
                }
            }
            _ => return rt(format!("Unable to set the '{name}' property of class '{}'.", self.class_name())),
        }
        Ok(self)
    }
}

pub fn format_duration(ms: f64, fmt: &str) -> String {
    if ms.is_nan() {
        return "NaN".into();
    }
    let unit = |v: f64, one: &str, many: &str| format!("{} {}", fmt_g(v, 5), if v == 1.0 { one } else { many });
    match fmt {
        "y" => unit(ms / (365.2425 * MS_DAY), "yr", "yrs"),
        "d" => unit(ms / MS_DAY, "day", "days"),
        "h" => format!("{} hr", fmt_g(ms / MS_HOUR, 5)),
        "m" => format!("{} min", fmt_g(ms / MS_MIN, 5)),
        "s" => format!("{} sec", fmt_g(ms / 1000.0, 5)),
        f => {
            let neg = ms < 0.0;
            let a = ms.abs();
            let h = (a / MS_HOUR).floor();
            let m = ((a - h * MS_HOUR) / MS_MIN).floor();
            let s = (a - h * MS_HOUR - m * MS_MIN) / 1000.0;
            let sign = if neg { "-" } else { "" };
            if f.starts_with("dd:") {
                let d = (h / 24.0).floor();
                return format!("{sign}{:02}:{:02}:{:02}:{:02}", d as i64, (h - d * 24.0) as i64, m as i64, s.round() as i64);
            }
            if f.ends_with(".SSS") {
                return format!("{sign}{:02}:{:02}:{:06.3}", h as i64, m as i64, s);
            }
            if f == "hh:mm" {
                return format!("{sign}{:02}:{:02}", h as i64, (m + (s / 60.0).round()) as i64);
            }
            format!("{sign}{:02}:{:02}:{:02}", h as i64, m as i64, s.round() as i64)
        }
    }
}

fn format_caldur(p: [f64; 3]) -> String {
    let [months, days, ms] = p;
    if months.is_nan() || days.is_nan() || ms.is_nan() {
        return "NaN".into();
    }
    let mut parts = Vec::new();
    let y = (months / 12.0).trunc();
    let mo = months - y * 12.0;
    if y != 0.0 {
        parts.push(format!("{}y", y as i64));
    }
    if mo != 0.0 {
        parts.push(format!("{}mo", mo as i64));
    }
    if days != 0.0 {
        parts.push(format!("{}d", fmt_g(days, 5)));
    }
    if ms != 0.0 {
        // The time part shows as `1h 2m 3s`.
        let neg = ms < 0.0;
        let a = ms.abs();
        let h = (a / MS_HOUR).floor();
        let m = ((a - h * MS_HOUR) / MS_MIN).floor();
        let s = (a - h * MS_HOUR - m * MS_MIN) / 1000.0;
        let sign = if neg { "-" } else { "" };
        parts.push(format!("{sign}{}h {}m {}s", h as i64, m as i64, fmt_g(s, 5)));
    }
    if parts.is_empty() {
        "0d".into()
    } else {
        parts.join(" ")
    }
}

/// Adds calendar months/days (clamping to month end) and exact ms to wall-clock ms.
fn add_calendar(wall: f64, p: [f64; 3]) -> f64 {
    if wall.is_nan() {
        return f64::NAN;
    }
    let (y, mo, d, h, mi, s) = components(wall);
    let total = y * 12 + (mo - 1) + p[0] as i64;
    let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    let nd = d.min(days_in_month(ny, nm));
    wall_ms(ny as f64, nm as f64, nd as f64, h as f64, mi as f64, s) + p[1] * MS_DAY + p[2]
}

/// Converts `v` to the same kind of time array as `like` (text → datetime, numbers → days).
fn coerce_like(like: &TimeArray, v: &Value) -> R<TimeArray> {
    match v {
        Value::Time(t) if t.kind == like.kind => Ok(t.clone()),
        Value::Time(t) => rt(format!("Unable to combine a {} with a {}.", like.class_name(), t.class_name())),
        Value::Num(m) if m.is_empty() => Ok(like.empty_like()),
        _ if like.kind == TimeKind::DateTime && (v.is_text() || matches!(v, Value::Str(_) | Value::Cell(_))) => {
            let texts = crate::ops::to_str_array(v)?;
            let mut ms = Vec::with_capacity(texts.data.len());
            for t in &texts.data {
                let w = t.as_deref().and_then(parse_auto).ok_or_else(|| LangError::new(format!("Could not recognize the date/time format of '{}'.", t.clone().unwrap_or_default()), 0))?;
                ms.push(w - like.offset_ms());
            }
            Ok(like.with_ms(texts.dims, ms))
        }
        Value::Num(m) if like.kind == TimeKind::Duration => Ok(like.with_ms(m.dims.clone(), m.data.iter().map(|d| d * MS_DAY).collect())),
        _ => rt(format!("Conversion to {} from {} is not possible.", like.class_name(), v.class_name())),
    }
}

fn pair_ms(a: &TimeArray, b: &TimeArray, f: impl Fn([f64; 3], [f64; 3]) -> [f64; 3]) -> R<(Dims, Vec<[f64; 3]>)> {
    let (dims, pairs) = nd::broadcast_pairs(&a.dims, &b.dims)?;
    Ok((dims, pairs.into_iter().map(|(i, j)| f(a.data[i], b.data[j])).collect()))
}

fn logical(dims: Dims, v: Vec<bool>) -> Value {
    Value::Num(Matrix::from_dims(dims, v.into_iter().map(|b| if b { 1.0 } else { 0.0 }).collect()).with_kind(Kind::Logical))
}

fn compare(op: BinOp, x: f64, y: f64) -> bool {
    match op {
        BinOp::Eq => x == y,
        BinOp::Ne => x != y,
        BinOp::Lt => x < y,
        BinOp::Le => x <= y,
        BinOp::Gt => x > y,
        _ => x >= y,
    }
}

fn numeric_days(v: &Value) -> Option<TimeArray> {
    match v {
        Value::Num(m) if m.kind != Kind::Char => Some(TimeArray::durations(m.dims.clone(), m.data.iter().map(|d| d * MS_DAY).collect(), "hh:mm:ss")),
        _ => None,
    }
}

pub fn binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    use BinOp::*;
    use TimeKind::*;
    let is_cmp = matches!(op, Eq | Ne | Lt | Le | Gt | Ge);
    match (a, b) {
        (Value::Time(x), Value::Time(y)) => match (x.kind, y.kind, op) {
            (DateTime, DateTime, Sub) => {
                let (d, v) = pair_ms(x, y, |p, q| [p[0] - q[0], 0.0, 0.0])?;
                Ok(Value::Time(TimeArray { dims: d, kind: Duration, data: v, tz: None, tz_name: String::new(), format: "hh:mm:ss".into() }))
            }
            (DateTime, Duration, Add | Sub) | (Duration, DateTime, Add) => {
                let (dt, du, sign) = if x.kind == DateTime { (x, y, if op == Sub { -1.0 } else { 1.0 }) } else { (y, x, 1.0) };
                let (d, v) = pair_ms(dt, du, |p, q| [p[0] + sign * q[0], 0.0, 0.0])?;
                Ok(Value::Time(TimeArray { dims: d, data: v, ..dt.clone() }))
            }
            (DateTime, CalDuration, Add | Sub) | (CalDuration, DateTime, Add) => {
                let (dt, cd, sign) = if x.kind == DateTime { (x, y, if op == Sub { -1.0 } else { 1.0 }) } else { (y, x, 1.0) };
                let off = dt.offset_ms();
                let (d, v) = pair_ms(dt, cd, |p, q| [add_calendar(p[0] + off, [sign * q[0], sign * q[1], sign * q[2]]) - off, 0.0, 0.0])?;
                Ok(Value::Time(TimeArray { dims: d, data: v, ..dt.clone() }))
            }
            (Duration, Duration, Add | Sub) => {
                let s = if op == Sub { -1.0 } else { 1.0 };
                let (d, v) = pair_ms(x, y, |p, q| [p[0] + s * q[0], 0.0, 0.0])?;
                Ok(Value::Time(TimeArray { dims: d, data: v, ..x.clone() }))
            }
            (Duration, Duration, Div | MatDiv) => {
                let (d, v) = pair_ms(x, y, |p, q| [p[0] / q[0], 0.0, 0.0])?;
                Ok(Value::Num(Matrix::from_dims(d, v.into_iter().map(|e| e[0]).collect())))
            }
            (CalDuration, CalDuration, Add | Sub) => {
                let s = if op == Sub { -1.0 } else { 1.0 };
                let (d, v) = pair_ms(x, y, |p, q| [p[0] + s * q[0], p[1] + s * q[1], p[2] + s * q[2]])?;
                Ok(Value::Time(TimeArray::calendar(d, v)))
            }
            (k1, k2, _) if k1 == k2 && is_cmp && k1 != CalDuration => {
                let (d, pairs) = nd::broadcast_pairs(&x.dims, &y.dims)?;
                Ok(logical(d, pairs.into_iter().map(|(i, j)| compare(op, x.data[i][0], y.data[j][0])).collect()))
            }
            _ => rt(format!("Operator '{}' is not supported for operands of type '{}' and '{}'.", op.symbol(), x.class_name(), y.class_name())),
        },
        (Value::Time(t), other) | (other, Value::Time(t)) => {
            let t_left = matches!(a, Value::Time(_));
            // Comparisons with text compare as datetimes; numbers act as days.
            if t.kind == DateTime && (other.is_text() || matches!(other, Value::Str(_))) {
                let o = Value::Time(coerce_like(t, other)?);
                return if t_left { binary(op, a, &o) } else { binary(op, &o, b) };
            }
            match (t.kind, op) {
                (Duration | CalDuration, Mul | MatMul) | (Duration | CalDuration, Div | MatDiv) if !(op == Div || op == MatDiv) || t_left => {
                    let m = other.to_matrix()?;
                    let (d, pairs) = nd::broadcast_pairs(&t.dims, &m.dims)?;
                    let f = |p: [f64; 3], k: f64| if matches!(op, Mul | MatMul) { [p[0] * k, p[1] * k, p[2] * k] } else { [p[0] / k, p[1] / k, p[2] / k] };
                    let data = pairs.into_iter().map(|(i, j)| f(t.data[i], m.data[j])).collect();
                    Ok(Value::Time(TimeArray { dims: d, data, ..t.clone() }))
                }
                (DateTime | Duration, Add | Sub) if t.kind == Duration || t_left || op == Add => {
                    let days = numeric_days(other).ok_or_else(|| LangError::new(format!("Operator '{}' is not supported for '{}' and '{}'.", op.symbol(), t.class_name(), other.class_name()), 0))?;
                    let dv = Value::Time(days);
                    if t_left { binary(op, a, &dv) } else { binary(op, &dv, b) }
                }
                (Duration, _) if is_cmp => {
                    let days = numeric_days(other).ok_or_else(|| LangError::new("Comparison is not supported.", 0))?;
                    let dv = Value::Time(days);
                    if t_left { binary(op, a, &dv) } else { binary(op, &dv, b) }
                }
                _ => rt(format!("Operator '{}' is not supported for operands of type '{}' and '{}'.", op.symbol(), a.class_name(), b.class_name())),
            }
        }
        _ => unreachable!(),
    }
}

pub fn unary(op: UnOp, t: &TimeArray) -> R<Value> {
    match op {
        UnOp::Neg if t.kind != TimeKind::DateTime => Ok(Value::Time(TimeArray { data: t.data.iter().map(|p| [-p[0], -p[1], -p[2]]).collect(), ..t.clone() })),
        UnOp::Plus => Ok(Value::Time(t.clone())),
        UnOp::Transpose | UnOp::CTranspose => {
            let (d, data) = nd::transpose(&t.dims, &t.data)?;
            Ok(Value::Time(TimeArray { dims: d, data, ..t.clone() }))
        }
        _ => rt(format!("Unary operator is not supported for {}.", t.class_name())),
    }
}

pub fn concat(items: Vec<Value>, dim: usize) -> R<Value> {
    let like = items.iter().find_map(|v| if let Value::Time(t) = v { Some(t.clone()) } else { None }).unwrap();
    let mut parts = Vec::new();
    for v in &items {
        let t = coerce_like(&like, v)?;
        parts.push((t.dims, t.data));
    }
    let (d, data) = nd::concat(parts, dim)?;
    Ok(Value::Time(TimeArray { dims: d, data, ..like }))
}

/// `t1:step:t2` with a duration step (or 1 day).
pub fn range(start: &Value, step: Option<&Value>, stop: &Value) -> R<Value> {
    let Value::Time(s) = start else { return rt("Colon operands must be datetime or duration when the step is a duration.") };
    let stop = coerce_like(s, stop)?;
    let step_ms = match step {
        Some(Value::Time(st)) if st.kind == TimeKind::Duration => st.data[0][0],
        Some(Value::Time(st)) if st.kind == TimeKind::CalDuration => {
            let mut out = Vec::new();
            let off = s.offset_ms();
            let mut k = 0.0;
            loop {
                let w = add_calendar(s.data[0][0] + off, [st.data[0][0] * k, st.data[0][1] * k, st.data[0][2] * k]) - off;
                if w > stop.data[0][0] || out.len() > 1_000_000 {
                    break;
                }
                out.push(w);
                k += 1.0;
            }
            let n = out.len();
            return Ok(Value::Time(s.with_ms(vec![1, n], out)));
        }
        Some(v) => v.scalar()? * MS_DAY,
        None => MS_DAY,
    };
    let n = crate::interp::range_count(s.data[0][0], step_ms, stop.data[0][0]);
    Ok(Value::Time(s.with_ms(vec![1, n], (0..n).map(|i| s.data[0][0] + i as f64 * step_ms).collect())))
}

// ---- builtins ------------------------------------------------------------------------------

pub const NAMES: &[&str] = &[
    "datetime", "NaT", "duration", "years", "days", "hours", "minutes", "seconds", "milliseconds", "calyears", "calquarters", "calmonths", "calweeks",
    "caldays", "between", "year", "month", "day", "hour", "minute", "second", "weekday", "quarter", "isdatetime", "isduration", "iscalendarduration",
    "isnat", "dateshift", "timeofday", "posixtime", "datenum", "now", "today",
];

fn opt<'a>(args: &'a [Value], name: &str) -> Option<&'a Value> {
    let mut i = 0;
    while i + 1 < args.len() {
        if args[i].is_text() && args[i].text().map(|t| t.eq_ignore_ascii_case(name)).unwrap_or(false) {
            return Some(&args[i + 1]);
        }
        i += 1;
    }
    None
}

/// Positional (non name-value) arguments.
fn positional(args: &[Value]) -> &[Value] {
    let names = ["TimeZone", "Format", "InputFormat", "ConvertFrom", "Locale", "PivotYear", "Epoch", "TicksPerSecond", "OutputFormat"];
    let n = args
        .iter()
        .position(|a| a.is_text() && a.text().map(|t| names.iter().any(|n| n.eq_ignore_ascii_case(&t))).unwrap_or(false))
        .unwrap_or(args.len());
    &args[..n]
}

fn one(v: Value) -> R<Option<Vec<Value>>> {
    Ok(Some(vec![v]))
}

fn time_arg<'a>(args: &'a [Value], name: &str) -> R<&'a TimeArray> {
    match args.first() {
        Some(Value::Time(t)) => Ok(t),
        _ => rt(format!("Expected a datetime, duration or calendarDuration input to '{name}'.")),
    }
}

pub fn call(name: &str, args: &[Value], nargout: usize) -> R<Option<Vec<Value>>> {
    match name {
        "datetime" => {
            let pos = positional(args);
            let tz_name = opt(args, "TimeZone").map(|v| v.text()).transpose()?.unwrap_or_default();
            let tz = parse_tz(&tz_name)?;
            let format = opt(args, "Format").map(|v| v.text()).transpose()?.unwrap_or_else(|| DEFAULT_DT_FORMAT.into());
            let off = tz.unwrap_or(0.0) * MS_MIN;
            let mk = |dims: Dims, wall: Vec<f64>| Value::Time(TimeArray::datetimes(dims, wall.into_iter().map(|w| w - off).collect(), tz, tz_name.clone(), format.clone()));
            if pos.is_empty() {
                return one(Value::Time(TimeArray::datetimes(vec![1, 1], vec![now_utc_ms()], tz, tz_name.clone(), format.clone())));
            }
            if let Some(cf) = opt(args, "ConvertFrom") {
                let m = pos[0].to_matrix()?;
                let kind = cf.text()?.to_lowercase();
                let utc: Vec<f64> = m
                    .data
                    .iter()
                    .map(|&x| match kind.as_str() {
                        "posixtime" => x * 1000.0,
                        "datenum" => (x - 719_529.0) * MS_DAY,
                        "excel" => (x - 25_569.0) * MS_DAY,
                        "juliandate" => (x - 2_440_587.5) * MS_DAY,
                        _ => f64::NAN,
                    })
                    .collect();
                return one(Value::Time(TimeArray::datetimes(m.dims.clone(), utc, tz, tz_name.clone(), format.clone())));
            }
            if pos[0].is_text() || matches!(pos[0], Value::Str(_) | Value::Cell(_)) {
                let texts = crate::ops::to_str_array(&pos[0])?;
                let input_fmt = opt(args, "InputFormat").map(|v| v.text()).transpose()?;
                let now_wall = now_utc_ms() + off;
                let mut walls = Vec::with_capacity(texts.data.len());
                for t in &texts.data {
                    let t = t.clone().unwrap_or_default();
                    let today = (now_wall / MS_DAY).floor() * MS_DAY;
                    let w = match t.to_lowercase().as_str() {
                        "now" => now_wall,
                        "today" => today,
                        "tomorrow" => today + MS_DAY,
                        "yesterday" => today - MS_DAY,
                        _ => match &input_fmt {
                            Some(f) => parse_with_format(&t, f).ok_or_else(|| LangError::new(format!("Unable to convert '{t}' to datetime using the format '{f}'."), 0))?,
                            None => parse_auto(&t).ok_or_else(|| LangError::new(format!("Could not recognize the date/time format of '{t}'. Specify a format using the 'InputFormat' parameter."), 0))?,
                        },
                    };
                    walls.push(w);
                }
                let fmt_given = opt(args, "Format").is_some();
                let mut v = mk(texts.dims.clone(), walls);
                if let (Value::Time(t), false, Some(f)) = (&mut v, fmt_given, input_fmt) {
                    t.format = f;
                }
                return one(v);
            }
            // datetime(Y,M,D[,H,MI,S[,MS]]) or datetime(DateVectors)
            let comps: Vec<Matrix> = if pos.len() == 1 {
                let m = pos[0].to_matrix()?;
                let (r, c) = (m.rows(), m.cols());
                if c != 3 && c != 6 {
                    return rt("Numeric input to datetime must be a date vector matrix with 3 or 6 columns.");
                }
                (0..c).map(|j| Matrix::col((0..r).map(|i| m.get(i, j)).collect())).collect()
            } else {
                pos.iter().map(|v| v.to_matrix()).collect::<R<_>>()?
            };
            if comps.len() < 3 {
                return rt("Not enough input arguments for datetime.");
            }
            let n = comps.iter().map(|m| m.numel()).max().unwrap_or(1);
            let dims = comps.iter().find(|m| m.numel() == n).map(|m| m.dims.clone()).unwrap();
            let at = |k: usize, i: usize| -> f64 {
                match comps.get(k) {
                    Some(m) if m.numel() == 1 => m.data[0],
                    Some(m) => m.data[i],
                    None => 0.0,
                }
            };
            let walls = (0..n).map(|i| wall_ms(at(0, i), at(1, i), at(2, i), at(3, i), at(4, i), at(5, i) + at(6, i) / 1000.0)).collect();
            one(mk(dims, walls))
        }
        "NaT" => {
            let (r, c) = match args.len() {
                0 => (1, 1),
                1 => {
                    let n = args[0].as_usize()?;
                    (n, n)
                }
                _ => (args[0].as_usize()?, args[1].as_usize()?),
            };
            one(Value::Time(TimeArray::datetimes(vec![r, c], vec![f64::NAN; r * c], None, String::new(), DEFAULT_DT_FORMAT.into())))
        }
        "now" | "today" => {
            // MATLAB's legacy serial date numbers.
            let ms = now_utc_ms();
            let dn = ms / MS_DAY + 719_529.0;
            Ok(Some(vec![Value::num(if name == "today" { dn.floor() } else { dn })]))
        }
        "duration" => {
            let pos = positional(args);
            let format = opt(args, "Format").map(|v| v.text()).transpose()?.unwrap_or_else(|| "hh:mm:ss".into());
            if pos.is_empty() {
                return one(Value::Time(TimeArray::durations(vec![1, 1], vec![0.0], &format)));
            }
            let comps: Vec<Matrix> = if pos.len() == 1 {
                let m = pos[0].to_matrix()?;
                (0..m.cols()).map(|j| Matrix::col((0..m.rows()).map(|i| m.get(i, j)).collect())).collect()
            } else {
                pos.iter().map(|v| v.to_matrix()).collect::<R<_>>()?
            };
            let n = comps.iter().map(|m| m.numel()).max().unwrap_or(1);
            let dims = comps.iter().find(|m| m.numel() == n).map(|m| m.dims.clone()).unwrap();
            let at = |k: usize, i: usize| comps.get(k).map(|m| if m.numel() == 1 { m.data[0] } else { m.data[i] }).unwrap_or(0.0);
            let ms = (0..n).map(|i| at(0, i) * MS_HOUR + at(1, i) * MS_MIN + at(2, i) * 1000.0 + at(3, i)).collect();
            one(Value::Time(TimeArray::durations(dims, ms, &format)))
        }
        "years" | "days" | "hours" | "minutes" | "seconds" | "milliseconds" => {
            let (scale, fmt) = match name {
                "years" => (365.2425 * MS_DAY, "y"),
                "days" => (MS_DAY, "d"),
                "hours" => (MS_HOUR, "h"),
                "minutes" => (MS_MIN, "m"),
                "seconds" => (1000.0, "s"),
                _ => (1.0, "s"),
            };
            match args.first() {
                Some(Value::Time(t)) if t.kind == TimeKind::Duration => one(Value::Num(Matrix::from_dims(t.dims.clone(), t.data.iter().map(|p| p[0] / scale).collect()))),
                Some(v) => {
                    let m = v.to_matrix()?;
                    one(Value::Time(TimeArray::durations(m.dims.clone(), m.data.iter().map(|x| x * scale).collect(), fmt)))
                }
                None => one(Value::Time(TimeArray::durations(vec![1, 1], vec![scale], fmt))),
            }
        }
        "calyears" | "calquarters" | "calmonths" | "calweeks" | "caldays" => {
            match args.first() {
                Some(Value::Time(t)) if t.kind == TimeKind::CalDuration => {
                    // Splitting: the component expressed in this unit.
                    let f = |p: [f64; 3]| match name {
                        "calyears" => (p[0] / 12.0).trunc(),
                        "calquarters" => (p[0] / 3.0).trunc(),
                        "calmonths" => p[0],
                        "calweeks" => (p[1] / 7.0).trunc(),
                        _ => p[1],
                    };
                    one(Value::Num(Matrix::from_dims(t.dims.clone(), t.data.iter().map(|&p| f(p)).collect())))
                }
                Some(v) => {
                    let m = v.to_matrix()?;
                    let parts = m
                        .data
                        .iter()
                        .map(|&x| match name {
                            "calyears" => [x * 12.0, 0.0, 0.0],
                            "calquarters" => [x * 3.0, 0.0, 0.0],
                            "calmonths" => [x, 0.0, 0.0],
                            "calweeks" => [0.0, x * 7.0, 0.0],
                            _ => [0.0, x, 0.0],
                        })
                        .collect();
                    one(Value::Time(TimeArray::calendar(m.dims.clone(), parts)))
                }
                None => rt(format!("Not enough input arguments for '{name}'.")),
            }
        }
        "between" => {
            let (Some(Value::Time(a)), Some(b)) = (args.first(), args.get(1)) else { return rt("between requires two datetime inputs.") };
            let b = coerce_like(a, b)?;
            let units = args.get(2).map(|v| v.text()).transpose()?.unwrap_or_else(|| "years,months,days,time".into()).to_lowercase();
            let only_days = units == "day" || units == "days";
            let (dims, pairs) = nd::broadcast_pairs(&a.dims, &b.dims)?;
            let parts = pairs
                .into_iter()
                .map(|(i, j)| {
                    let (w1, w2) = (a.wall(i), b.wall(j));
                    if only_days {
                        let d = ((w2 - w1) / MS_DAY).trunc();
                        return [0.0, d, 0.0];
                    }
                    // Whole months, then whole days, then the remaining time.
                    let (y1, m1, ..) = components(w1);
                    let (y2, m2, ..) = components(w2);
                    let mut months = (y2 - y1) * 12 + (m2 - m1);
                    let sign = if w2 >= w1 { 1 } else { -1 };
                    while months != 0 && (add_calendar(w1, [months as f64, 0.0, 0.0]) - w2) * sign as f64 > 0.0 {
                        months -= sign;
                    }
                    let mid = add_calendar(w1, [months as f64, 0.0, 0.0]);
                    let days = ((w2 - mid) / MS_DAY).trunc();
                    let ms = w2 - mid - days * MS_DAY;
                    [months as f64, days, if units.contains("time") { ms } else { 0.0 }]
                })
                .collect();
            one(Value::Time(TimeArray::calendar(dims, parts)))
        }
        "year" | "month" | "day" | "hour" | "minute" | "second" | "weekday" | "quarter" => {
            let t = time_arg(args, name)?;
            if t.kind != TimeKind::DateTime {
                return rt(format!("'{name}' requires a datetime input."));
            }
            let v = match name {
                "weekday" => t.component(|w| if w.is_nan() { f64::NAN } else { ((w / MS_DAY).floor() as i64 + 4).rem_euclid(7) as f64 + 1.0 }),
                "quarter" => t.component(|w| if w.is_nan() { f64::NAN } else { ((components(w).1 - 1) / 3 + 1) as f64 }),
                _ => t.get_prop(&format!("{}{}", name[..1].to_uppercase(), &name[1..]))?,
            };
            one(v)
        }
        "isdatetime" | "isduration" | "iscalendarduration" => {
            let want = match name {
                "isdatetime" => TimeKind::DateTime,
                "isduration" => TimeKind::Duration,
                _ => TimeKind::CalDuration,
            };
            one(Value::bool(matches!(args.first(), Some(Value::Time(t)) if t.kind == want)))
        }
        "isnat" => {
            let t = time_arg(args, name)?;
            one(logical(t.dims.clone(), t.data.iter().map(|p| p[0].is_nan()).collect()))
        }
        "dateshift" => {
            let t = time_arg(args, name)?;
            let rule = args.get(1).map(|v| v.text()).transpose()?.unwrap_or_default().to_lowercase();
            let unit = args.get(2).map(|v| v.text()).transpose()?.unwrap_or_default().to_lowercase();
            let off = t.offset_ms();
            let shift = |w: f64| -> f64 {
                let (y, mo, d, h, mi, _) = components(w);
                let start = match unit.as_str() {
                    "year" => wall_ms(y as f64, 1.0, 1.0, 0.0, 0.0, 0.0),
                    "quarter" => wall_ms(y as f64, (((mo - 1) / 3) * 3 + 1) as f64, 1.0, 0.0, 0.0, 0.0),
                    "month" => wall_ms(y as f64, mo as f64, 1.0, 0.0, 0.0, 0.0),
                    "week" => (w / MS_DAY).floor() * MS_DAY - (((w / MS_DAY).floor() as i64 + 4).rem_euclid(7)) as f64 * MS_DAY,
                    "hour" => wall_ms(y as f64, mo as f64, d as f64, h as f64, 0.0, 0.0),
                    "minute" => wall_ms(y as f64, mo as f64, d as f64, h as f64, mi as f64, 0.0),
                    "second" => w.floor() - (w.floor() % 1000.0),
                    _ => (w / MS_DAY).floor() * MS_DAY,
                };
                if rule == "end" {
                    let next = match unit.as_str() {
                        "year" => add_calendar(start, [12.0, 0.0, 0.0]),
                        "quarter" => add_calendar(start, [3.0, 0.0, 0.0]),
                        "month" => add_calendar(start, [1.0, 0.0, 0.0]),
                        "week" => start + 7.0 * MS_DAY,
                        "hour" => start + MS_HOUR,
                        "minute" => start + MS_MIN,
                        "second" => start + 1000.0,
                        _ => start + MS_DAY,
                    };
                    // MATLAB's 'end' is the start of the last day/hour… of the unit.
                    match unit.as_str() {
                        "year" | "quarter" | "month" | "week" => next - MS_DAY,
                        _ => next - 1.0,
                    }
                } else {
                    start
                }
            };
            let ms = (0..t.data.len()).map(|i| shift(t.wall(i)) - off).collect();
            one(Value::Time(t.with_ms(t.dims.clone(), ms)))
        }
        "timeofday" => {
            let t = time_arg(args, name)?;
            let ms = (0..t.data.len()).map(|i| t.wall(i).rem_euclid(MS_DAY)).collect();
            one(Value::Time(TimeArray::durations(t.dims.clone(), ms, "hh:mm:ss")))
        }
        "posixtime" | "datenum" => {
            let t = time_arg(args, name)?;
            let f = |i: usize| if name == "posixtime" { t.data[i][0] / 1000.0 } else { t.wall(i) / MS_DAY + 719_529.0 };
            one(Value::Num(Matrix::from_dims(t.dims.clone(), (0..t.data.len()).map(f).collect())))
        }
        _ => {
            let _ = nargout;
            Ok(None)
        }
    }
}

/// Numeric view for reductions and plotting: ms (datetime/duration).
pub fn to_numeric(t: &TimeArray) -> Matrix {
    Matrix::from_dims(t.dims.clone(), t.data.iter().map(|p| p[0]).collect())
}

/// Text conversion used by `string`, `char`, `cellstr`.
pub fn to_strings(t: &TimeArray) -> StrArray {
    StrArray::from_dims(t.dims.clone(), (0..t.data.len()).map(|i| if t.data[i][0].is_nan() { None } else { Some(t.format_elem(i)) }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        for &(y, m, d) in &[(1970, 1, 1), (2000, 2, 29), (2018, 6, 1), (1900, 3, 1)] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
    }

    #[test]
    fn formats() {
        let w = wall_ms(2018.0, 6.0, 1.0, 12.0, 5.0, 9.0);
        assert_eq!(format_datetime(w, DEFAULT_DT_FORMAT), "01-Jun-2018 12:05:09");
        assert_eq!(format_datetime(w, "yyyy-MM-dd'T'HH:mm"), "2018-06-01T12:05");
        assert_eq!(parse_auto("2018-06-01 12:05:09"), Some(w));
        assert_eq!(format_duration(1.5 * MS_HOUR, "h"), "1.5 hr");
        assert_eq!(format_duration(5400_000.0, "hh:mm:ss"), "01:30:00");
        assert_eq!(format_caldur([14.0, 3.0, 0.0]), "1y 2mo 3d");
    }

    #[test]
    fn calendar_month_end_clamps() {
        let jan31 = wall_ms(2021.0, 1.0, 31.0, 0.0, 0.0, 0.0);
        assert_eq!(format_datetime(add_calendar(jan31, [1.0, 0.0, 0.0]), "yyyy-MM-dd"), "2021-02-28");
    }
}
