//! Serial date numbers and date vectors (`datestr`, `datevec`, `clock`, `date`, `etime`,
//! `addtodate`, `eomday`, `eomdate`, `weeknum`, `calendar`, `m2xdate`, `x2mdate`, `months`),
//! and datetime helpers (`ymd`, `hms`, `isweekend`, `isbetween`, `exceltime`, `juliandate`,
//! `yyyymmdd`, `convertTo`, `week`, `calendarDuration`, `cputime`).
//!
//! A serial date number counts days from year 0 (`datenum(1970,1,1)` is 719529).

use crate::builtins::{arg, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::time::{civil_from_days, days_from_civil, TimeArray, TimeKind};
use crate::nd::Dims;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "datestr", "datevec", "clock", "date", "etime", "cputime", "addtodate", "eomday", "eomdate", "weeknum", "calendar", "ymd", "hms", "isweekend",
    "isbetween", "exceltime", "juliandate", "yyyymmdd", "convertTo", "m2xdate", "x2mdate", "months", "calendarDuration", "week", "isdst",
    "datenum",
];

const EPOCH: f64 = 719_529.0;
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// Date number from components (months and days may overflow, as in datenum).
fn datenum_of(y: f64, mo: f64, d: f64, h: f64, mi: f64, s: f64) -> f64 {
    let total = y.floor() as i64 * 12 + (mo.round() as i64 - 1);
    let (yy, mm) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    days_from_civil(yy, mm, 1) as f64 + EPOCH + (d - 1.0) + (h * 3600.0 + mi * 60.0 + s) / 86_400.0
}

/// Components (y, mo, d, h, mi, s) of a date number, with the seconds rounded to the
/// millisecond as MATLAB does.
fn parts(dn: f64) -> (i64, i64, i64, i64, i64, f64) {
    let ms = ((dn - EPOCH) * 86_400_000.0).round();
    let days = (ms / 86_400_000.0).floor();
    let rem = ms - days * 86_400_000.0;
    let (y, m, d) = civil_from_days(days as i64);
    let h = (rem / 3_600_000.0).floor();
    let mi = ((rem - h * 3_600_000.0) / 60_000.0).floor();
    let s = (rem - h * 3_600_000.0 - mi * 60_000.0) / 1000.0;
    (y, m, d, h as i64, mi as i64, s)
}

fn weekday_of(dn: f64) -> usize {
    // datenum 1 (01-Jan-0000) was a Saturday.
    ((dn.floor() as i64 + 5).rem_euclid(7)) as usize
}

/// datestr's numeric format codes.
fn format_code(k: i64) -> Option<&'static str> {
    Some(match k {
        0 => "dd-mmm-yyyy HH:MM:SS",
        1 => "dd-mmm-yyyy",
        2 => "mm/dd/yy",
        3 => "mmm",
        4 => "m",
        5 => "mm",
        6 => "mm/dd",
        7 => "dd",
        8 => "ddd",
        9 => "d",
        10 => "yyyy",
        11 => "yy",
        12 => "mmmyy",
        13 => "HH:MM:SS",
        14 => "HH:MM:SS PM",
        15 => "HH:MM",
        16 => "HH:MM PM",
        17 => "QQ-YY",
        18 => "QQ",
        19 => "dd/mm",
        20 => "dd/mm/yy",
        21 => "mmm.dd,yyyy HH:MM:SS",
        22 => "mmm.dd,yyyy",
        23 => "mm/dd/yyyy",
        24 => "dd/mm/yyyy",
        25 => "yy/mm/dd",
        26 => "yyyy/mm/dd",
        27 => "QQ-YYYY",
        28 => "mmmyyyy",
        29 => "yyyy-mm-dd",
        30 => "yyyymmddTHHMMSS",
        31 => "yyyy-mm-dd HH:MM:SS",
        _ => return None,
    })
}

/// One date number in a datestr format.
fn datestr_one(dn: f64, fmt: &str) -> String {
    let (y, mo, d, h, mi, s) = parts(dn);
    let pm = fmt.contains("AM") || fmt.contains("PM");
    let f: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let run = |i: usize, c: char| f[i..].iter().take_while(|&&x| x == c).count();
    // `mm` is the month unless it follows hours (HH:MM uses MM).
    while i < f.len() {
        let c = f[i];
        let n = run(i, c);
        match c {
            'y' if n >= 4 => out.push_str(&format!("{y:04}")),
            'y' => out.push_str(&format!("{:02}", y.rem_euclid(100))),
            'Y' if n >= 4 => out.push_str(&format!("{y:04}")),
            'Y' => out.push_str(&format!("{:02}", y.rem_euclid(100))),
            'm' if n >= 4 => out.push_str(MONTHS[mo as usize - 1]),
            'm' if n == 3 => out.push_str(&MONTHS[mo as usize - 1][..3]),
            'm' if n == 2 => out.push_str(&format!("{mo:02}")),
            'm' => out.push_str(&MONTHS[mo as usize - 1][..1]),
            'd' if n >= 4 => out.push_str(WEEKDAYS[weekday_of(dn)]),
            'd' if n == 3 => out.push_str(&WEEKDAYS[weekday_of(dn)][..3]),
            'd' if n == 2 => out.push_str(&format!("{d:02}")),
            'd' => out.push_str(&WEEKDAYS[weekday_of(dn)][..1]),
            'H' => {
                if pm {
                    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
                    out.push_str(&format!("{h12:2}"));
                } else {
                    out.push_str(&format!("{h:02}"));
                }
            }
            'M' => out.push_str(&format!("{mi:02}")),
            'S' => out.push_str(&format!("{:02}", s.floor() as i64)),
            'F' => out.push_str(&format!("{:03}", ((s - s.floor()) * 1000.0).round() as i64)),
            'Q' => out.push_str(&format!("Q{}", (mo - 1) / 3 + 1)),
            'A' | 'P' if f.get(i + 1) == Some(&'M') => {
                out.push_str(if h >= 12 { "PM" } else { "AM" });
                i += 2;
                continue;
            }
            other => {
                for _ in 0..n {
                    out.push(other);
                }
            }
        }
        // Q is one symbol per 'QQ'; the others consume their whole run.
        i += if c == 'Q' { 2.min(n) } else { n };
    }
    out
}

/// Date numbers of the first argument of datestr/datevec: date numbers, a date vector (or
/// rows of them), datetimes, or text.
fn datenums(v: &Value) -> R<(Vec<f64>, Dims)> {
    match v {
        Value::Time(t) if t.kind == TimeKind::DateTime => Ok(((0..t.data.len()).map(|i| t.wall(i) / 86_400_000.0 + EPOCH).collect(), t.dims.clone())),
        v if v.is_text() || matches!(v, Value::Str(_) | Value::Cell(_)) => {
            let texts = crate::ops::to_str_array(v)?;
            let vals = texts.data.iter().map(|t| parse_date_text(t.as_deref().unwrap_or(""), None)).collect::<R<Vec<_>>>()?;
            let n = vals.len();
            Ok((vals, vec![n, 1]))
        }
        v => {
            let m = v.to_matrix()?;
            // Rows of 3 or 6 columns are date vectors (when they look like dates).
            if m.is_2d() && (m.cols() == 6 || m.cols() == 3) && m.rows() >= 1 && (0..m.rows()).all(|r| m.get(r, 0) > 31.0 || m.cols() == 6) {
                let r = m.rows();
                let row = |i: usize, j: usize| if j < m.cols() { m.get(i, j) } else { 0.0 };
                Ok(((0..r).map(|i| datenum_of(row(i, 0), row(i, 1), row(i, 2), row(i, 3), row(i, 4), row(i, 5))).collect(), vec![r, 1]))
            } else {
                Ok((m.data.clone(), m.dims.clone()))
            }
        }
    }
}

/// Text to a date number, with a datestr-style format or one of the formats datevec knows.
fn parse_date_text(t: &str, fmt: Option<&str>) -> R<f64> {
    let t = t.trim();
    let formats: Vec<String> = match fmt {
        Some(f) => vec![f.to_string()],
        None => [0, 1, 2, 13, 15, 23, 24, 26, 29, 30, 31, 21, 22].iter().filter_map(|&k| format_code(k)).map(str::to_string).chain(["mm/dd/yyyy HH:MM:SS", "dd-mmm-yyyy HH:MM"].iter().map(|s| s.to_string())).collect(),
    };
    for f in &formats {
        if let Some(dn) = parse_with(t, f) {
            return Ok(dn);
        }
    }
    rt(format!("DATEVEC failed to convert date string '{t}'. Failed to match the format."))
}

fn parse_with(t: &str, fmt: &str) -> Option<f64> {
    let s: Vec<char> = t.chars().collect();
    let f: Vec<char> = fmt.chars().collect();
    let (mut y, mut mo, mut d, mut h, mut mi, mut sec) = (None, None, Some(1.0), 0.0, 0.0, 0.0);
    let mut pm: Option<bool> = None;
    let (mut i, mut j) = (0, 0);
    let digits = |s: &[char], j: &mut usize, max: usize| -> Option<f64> {
        let st = *j;
        while *j < s.len() && *j - st < max && s[*j].is_ascii_digit() {
            *j += 1;
        }
        if *j == st { None } else { s[st..*j].iter().collect::<String>().parse().ok() }
    };
    while i < f.len() {
        let c = f[i];
        let n = f[i..].iter().take_while(|&&x| x == c).count();
        match c {
            'y' => {
                let v = digits(&s, &mut j, n.max(2))?;
                y = Some(if n == 2 { if v < 50.0 { 2000.0 + v } else { 1900.0 + v } } else { v });
            }
            'm' if n >= 3 => {
                let rest: String = s[j..].iter().collect::<String>().to_lowercase();
                let k = MONTHS.iter().position(|m| rest.starts_with(&m.to_lowercase()[..3]))?;
                mo = Some(k as f64 + 1.0);
                j += if n >= 4 && rest.starts_with(&MONTHS[k].to_lowercase()) { MONTHS[k].len() } else { 3 };
            }
            'm' => mo = Some(digits(&s, &mut j, 2)?),
            'd' if n >= 3 => {
                let rest: String = s[j..].iter().collect::<String>().to_lowercase();
                let k = WEEKDAYS.iter().position(|w| rest.starts_with(&w.to_lowercase()[..3]))?;
                j += if n >= 4 { WEEKDAYS[k].len() } else { 3 };
            }
            'd' => d = Some(digits(&s, &mut j, 2)?),
            'H' => {
                while j < s.len() && s[j] == ' ' {
                    j += 1;
                }
                h = digits(&s, &mut j, 2)?;
            }
            'M' => mi = digits(&s, &mut j, 2)?,
            'S' => sec = digits(&s, &mut j, 2)?,
            'F' => sec += digits(&s, &mut j, 3)? / 1000.0,
            'A' | 'P' if f.get(i + 1) == Some(&'M') => {
                let rest: String = s[j..].iter().take(2).collect::<String>().to_uppercase();
                pm = Some(rest == "PM");
                j += 2;
                i += 2;
                continue;
            }
            lit => {
                for _ in 0..n {
                    if j < s.len() && s[j].to_ascii_lowercase() == lit.to_ascii_lowercase() {
                        j += 1;
                    } else {
                        return None;
                    }
                }
            }
        }
        i += n;
    }
    if j != s.len() {
        return None;
    }
    if let Some(p) = pm {
        h = (h % 12.0) + if p { 12.0 } else { 0.0 };
    }
    // A time with no date is on 1 January of the current year, as in MATLAB.
    let year = y.unwrap_or_else(|| parts(now_datenum()).0 as f64);
    Some(datenum_of(year, mo.unwrap_or(1.0), d.unwrap_or(1.0), h, mi, sec))
}

fn now_datenum() -> f64 {
    let ms = crate::time::now_utc_ms() + crate::time::local_offset_ms();
    ms / 86_400_000.0 + EPOCH
}

fn char_rows(rows: &[String]) -> Value {
    let n = rows.len();
    let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let mut data = vec![32.0; n * w];
    for (i, r) in rows.iter().enumerate() {
        for (j, c) in r.chars().enumerate() {
            data[j * n + i] = c as u32 as f64;
        }
    }
    Value::Num(Matrix::new(n, w, data).cast_to(Kind::Char))
}

fn datevec_rows(dns: &[f64]) -> Matrix {
    let n = dns.len();
    let mut data = vec![0.0; n * 6];
    for (i, &dn) in dns.iter().enumerate() {
        let (y, mo, d, h, mi, s) = parts(dn);
        for (j, v) in [y as f64, mo as f64, d as f64, h as f64, mi as f64, s].into_iter().enumerate() {
            data[j * n + i] = v;
        }
    }
    Matrix::new(n, 6, data)
}

fn time_arg(a: &[Value], i: usize, name: &str) -> R<TimeArray> {
    match arg(a, i, name)? {
        Value::Time(t) if t.kind == TimeKind::DateTime => Ok(t.clone()),
        v => match crate::time::call("datetime", std::slice::from_ref(v), 1)? {
            Some(mut out) => match out.remove(0) {
                Value::Time(t) => Ok(t),
                _ => rt(format!("{name}: input must be a datetime array.")),
            },
            None => rt(format!("{name}: input must be a datetime array.")),
        },
    }
}

/// A numeric array with the shape of a datetime array.
fn per_elem(t: &TimeArray, f: impl Fn(f64) -> f64) -> Value {
    Value::Num(Matrix::from_dims(t.dims.clone(), (0..t.data.len()).map(|i| f(t.wall(i) / 86_400_000.0 + EPOCH)).collect()))
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match name {
        "datestr" => {
            let (dns, _) = datenums(arg(a, 0, name)?)?;
            let fmt = match a.get(1) {
                Some(v) if v.is_text() || matches!(v, Value::Str(_)) => v.text()?,
                Some(v) => format_code(v.scalar()? as i64).ok_or_else(|| LangError::new("Invalid date format number.", 0))?.to_string(),
                // Format 1 when every date is at midnight, otherwise format 0.
                None => if dns.iter().all(|d| (d - d.floor()).abs() < 1e-9) { format_code(1).unwrap().into() } else { format_code(0).unwrap().into() },
            };
            let rows: Vec<String> = dns.iter().map(|&d| datestr_one(d, &fmt)).collect();
            one(char_rows(&rows))
        }
        "datevec" => {
            let dns = match (arg(a, 0, name)?, a.get(1)) {
                (v, Some(f)) if v.is_text() || matches!(v, Value::Str(_) | Value::Cell(_)) => {
                    let fmt = f.text()?;
                    crate::ops::to_str_array(v)?.data.iter().map(|t| parse_date_text(t.as_deref().unwrap_or(""), Some(&fmt))).collect::<R<Vec<_>>>()?
                }
                (v, _) => datenums(v)?.0,
            };
            let m = datevec_rows(&dns);
            if nargout <= 1 {
                return one(Value::Num(m));
            }
            let n = m.rows();
            Ok(Some((0..nargout.min(6)).map(|j| Value::Num(Matrix::col(m.data[j * n..(j + 1) * n].to_vec()))).collect()))
        }
        "datenum" => {
            match a.len() {
                0 => rt("Not enough input arguments."),
                // datenum(text[, format]) or datenum(datevec / date numbers).
                1 | 2 if a[0].is_text() || matches!(a[0], Value::Str(_) | Value::Cell(_)) => {
                    let fmt = a.get(1).filter(|v| v.is_text()).map(|v| v.text()).transpose()?;
                    let texts = crate::ops::to_str_array(&a[0])?;
                    let vals = texts.data.iter().map(|t| parse_date_text(t.as_deref().unwrap_or(""), fmt.as_deref())).collect::<R<Vec<_>>>()?;
                    let n = vals.len();
                    one(Value::Num(Matrix::new(n, 1, vals)))
                }
                1 => {
                    let (dns, dims) = datenums(&a[0])?;
                    one(Value::Num(Matrix::from_dims(dims, dns)))
                }
                _ => {
                    // datenum(Y, M, D[, H, MI, S]) with scalar expansion.
                    let ms = a.iter().take(6).map(|v| v.to_matrix()).collect::<R<Vec<_>>>()?;
                    let n = ms.iter().map(|m| m.numel()).max().unwrap_or(1);
                    let dims = ms.iter().find(|m| m.numel() == n).map(|m| m.dims.clone()).unwrap_or(vec![1, 1]);
                    let get = |k: usize, i: usize| ms.get(k).map(|m| if m.numel() == 1 { m.data[0] } else { m.data[i] }).unwrap_or(0.0);
                    let vals = (0..n).map(|i| datenum_of(get(0, i), get(1, i), get(2, i), get(3, i), get(4, i), get(5, i))).collect();
                    one(Value::Num(Matrix::from_dims(dims, vals)))
                }
            }
        }
        "clock" => one(Value::Num(datevec_rows(&[now_datenum()]))),
        "date" => one(Value::chars(&datestr_one(now_datenum(), "dd-mmm-yyyy"))),
        "cputime" => {
            let t = crate::time::process_cpu_seconds();
            one(Value::num(t))
        }
        "etime" => {
            let (t1, _) = datenums(arg(a, 0, name)?)?;
            let (t0, _) = datenums(arg(a, 1, name)?)?;
            one(Value::Num(Matrix::col(t1.iter().zip(&t0).map(|(a, b)| ((a - b) * 86_400.0 * 1000.0).round() / 1000.0).collect())))
        }
        "addtodate" => {
            let dn = arg(a, 0, name)?.to_matrix()?;
            let q = arg(a, 1, name)?.scalar()?;
            let field = arg(a, 2, name)?.text()?.to_lowercase();
            let out: Vec<f64> = dn
                .data
                .iter()
                .map(|&d| {
                    let (y, mo, dd, h, mi, s) = parts(d);
                    let frac = (h as f64 * 3600.0 + mi as f64 * 60.0 + s) / 86_400.0;
                    let date_only = |yy: f64, mm: f64| {
                        // Month arithmetic keeps the day, clamped to the month's length.
                        let base = datenum_of(yy, mm, 1.0, 0.0, 0.0, 0.0);
                        let (ny, nm, _, _, _, _) = parts(base);
                        let last = days_in_month(ny, nm);
                        base + (dd.min(last) - 1) as f64 + frac
                    };
                    match field.as_str() {
                        "year" => Ok(date_only(y as f64 + q, mo as f64)),
                        "month" => Ok(date_only(y as f64, mo as f64 + q)),
                        "week" => Ok(d + 7.0 * q),
                        "day" => Ok(d + q),
                        "hour" => Ok(d + q / 24.0),
                        "minute" => Ok(d + q / 1440.0),
                        "second" => Ok(d + q / 86_400.0),
                        "millisecond" => Ok(d + q / 86_400_000.0),
                        other => rt(format!("Invalid date field '{other}'.")),
                    }
                })
                .collect::<R<_>>()?;
            one(Value::Num(Matrix::from_dims(dn.dims.clone(), out)))
        }
        "eomday" => {
            let y = arg(a, 0, name)?.to_matrix()?;
            let m = arg(a, 1, name)?.to_matrix()?;
            one(Value::Num(crate::ops::broadcast(&y, &m, |yy, mm| days_in_month(yy as i64, mm as i64) as f64)?))
        }
        "eomdate" => {
            let (y, m) = match a.len() {
                1 => {
                    let (dns, _) = datenums(&a[0])?;
                    let (yy, mm, ..) = parts(dns[0]);
                    (yy as f64, mm as f64)
                }
                _ => (arg(a, 0, name)?.scalar()?, arg(a, 1, name)?.scalar()?),
            };
            one(Value::num(datenum_of(y, m, days_in_month(y as i64, m as i64) as f64, 0.0, 0.0, 0.0)))
        }
        "weeknum" => {
            let (dns, dims) = datenums(arg(a, 0, name)?)?;
            let first_day = a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(1.0) as i64;
            let out: Vec<f64> = dns
                .iter()
                .map(|&d| {
                    let (y, ..) = parts(d);
                    let jan1 = datenum_of(y as f64, 1.0, 1.0, 0.0, 0.0, 0.0);
                    // Weeks start on `first_day` (1 = Sunday); week 1 contains 1 January.
                    let offset = (weekday_of(jan1) as i64 - (first_day - 1)).rem_euclid(7);
                    ((d.floor() - jan1 + offset as f64) / 7.0).floor() + 1.0
                })
                .collect();
            one(Value::Num(Matrix::from_dims(dims, out)))
        }
        "week" => {
            let t = time_arg(a, 0, name)?;
            one(per_elem(&t, |d| {
                let (y, ..) = parts(d);
                let jan1 = datenum_of(y as f64, 1.0, 1.0, 0.0, 0.0, 0.0);
                ((d.floor() - jan1 + weekday_of(jan1) as f64) / 7.0).floor() + 1.0
            }))
        }
        "calendar" => {
            let (y, m) = match a.len() {
                0 => {
                    let (y, m, ..) = parts(now_datenum());
                    (y, m)
                }
                1 => {
                    let (dns, _) = datenums(&a[0])?;
                    let (y, m, ..) = parts(dns[0]);
                    (y, m)
                }
                _ => (arg(a, 0, name)?.scalar()? as i64, arg(a, 1, name)?.scalar()? as i64),
            };
            let first = datenum_of(y as f64, m as f64, 1.0, 0.0, 0.0, 0.0);
            let start = weekday_of(first);
            let n = days_in_month(y, m) as usize;
            let mut data = vec![0.0; 42];
            for k in 0..n {
                let pos = start + k;
                let (r, c) = (pos / 7, pos % 7);
                data[c * 6 + r] = k as f64 + 1.0;
            }
            one(Value::Num(Matrix::new(6, 7, data)))
        }
        "ymd" | "hms" => {
            let t = time_arg(a, 0, name)?;
            let comps: Vec<(i64, i64, i64, i64, i64, f64)> = (0..t.data.len()).map(|i| parts(t.wall(i) / 86_400_000.0 + EPOCH)).collect();
            let col = |f: &dyn Fn(&(i64, i64, i64, i64, i64, f64)) -> f64| Value::Num(Matrix::from_dims(t.dims.clone(), comps.iter().map(f).collect()));
            let outs = if name == "ymd" {
                vec![col(&|c| c.0 as f64), col(&|c| c.1 as f64), col(&|c| c.2 as f64)]
            } else {
                vec![col(&|c| c.3 as f64), col(&|c| c.4 as f64), col(&|c| c.5)]
            };
            Ok(Some(outs.into_iter().take(nargout.max(1)).collect()))
        }
        "isweekend" => {
            let t = time_arg(a, 0, name)?;
            let v = per_elem(&t, |d| if matches!(weekday_of(d), 0 | 6) { 1.0 } else { 0.0 });
            one(Value::Num(v.to_matrix()?.cast_to(Kind::Logical)))
        }
        "isdst" => {
            let t = time_arg(a, 0, name)?;
            one(Value::Num(Matrix::from_dims(t.dims.clone(), vec![0.0; t.data.len()]).cast_to(Kind::Logical)))
        }
        "isbetween" => {
            let t = time_arg(a, 0, name)?;
            let lo = time_arg(a, 1, name)?;
            let hi = time_arg(a, 2, name)?;
            let pick = |x: &TimeArray, i: usize| x.data[if x.data.len() == 1 { 0 } else { i }][0];
            let v: Vec<f64> = (0..t.data.len()).map(|i| { let x = t.data[i][0]; if x >= pick(&lo, i) && x <= pick(&hi, i) { 1.0 } else { 0.0 } }).collect();
            one(Value::Num(Matrix::from_dims(t.dims.clone(), v).cast_to(Kind::Logical)))
        }
        "exceltime" => {
            let t = time_arg(a, 0, name)?;
            one(per_elem(&t, |d| d - 693_960.0))
        }
        "juliandate" => {
            let t = time_arg(a, 0, name)?;
            // Julian dates count from noon UTC.
            one(Value::Num(Matrix::from_dims(t.dims.clone(), t.data.iter().map(|e| e[0] / 86_400_000.0 + 2_440_587.5).collect())))
        }
        "yyyymmdd" => {
            let t = time_arg(a, 0, name)?;
            one(per_elem(&t, |d| {
                let (y, m, dd, ..) = parts(d);
                (y * 10_000 + m * 100 + dd) as f64
            }))
        }
        "convertTo" => {
            let t = time_arg(a, 0, name)?;
            let kind = arg(a, 1, name)?.text()?.to_lowercase();
            match kind.as_str() {
                "posixtime" => one(Value::Num(Matrix::from_dims(t.dims.clone(), t.data.iter().map(|e| e[0] / 1000.0).collect()))),
                "datenum" => one(per_elem(&t, |d| d)),
                "excel" => one(per_elem(&t, |d| d - 693_960.0)),
                "excel1904" => one(per_elem(&t, |d| d - 695_422.0)),
                "juliandate" => one(Value::Num(Matrix::from_dims(t.dims.clone(), t.data.iter().map(|e| e[0] / 86_400_000.0 + 2_440_587.5).collect()))),
                "modifiedjuliandate" => one(Value::Num(Matrix::from_dims(t.dims.clone(), t.data.iter().map(|e| e[0] / 86_400_000.0 + 40_587.0).collect()))),
                "yyyymmdd" => one(per_elem(&t, |d| {
                    let (y, m, dd, ..) = parts(d);
                    (y * 10_000 + m * 100 + dd) as f64
                })),
                other => rt(format!("Unknown conversion type '{other}'.")),
            }
        }
        "m2xdate" => {
            let m = arg(a, 0, name)?.to_matrix()?;
            let base = if a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(0.0) == 1.0 { 695_422.0 } else { 693_960.0 };
            one(Value::Num(m.map(move |d| d - base)))
        }
        "x2mdate" => {
            let m = arg(a, 0, name)?.to_matrix()?;
            let base = if a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(0.0) == 1.0 { 695_422.0 } else { 693_960.0 };
            one(Value::Num(m.map(move |d| d + base)))
        }
        "months" => {
            let (d1, _) = datenums(arg(a, 0, name)?)?;
            let (d2, _) = datenums(arg(a, 1, name)?)?;
            let out: Vec<f64> = d1
                .iter()
                .zip(&d2)
                .map(|(&x, &y)| {
                    let (y1, m1, dd1, ..) = parts(x);
                    let (y2, m2, dd2, ..) = parts(y);
                    let mut n = (y2 - y1) * 12 + (m2 - m1);
                    if dd2 < dd1 {
                        n -= 1;
                    }
                    n as f64
                })
                .collect();
            one(Value::Num(Matrix::col(out)))
        }
        "calendarDuration" => {
            let nums = a.iter().map(|v| v.scalar()).collect::<R<Vec<f64>>>()?;
            if nums.len() < 3 {
                return rt("calendarDuration requires years, months and days.");
            }
            let g = |i: usize| nums.get(i).copied().unwrap_or(0.0);
            let months = g(0) * 12.0 + g(1);
            let ms = (g(3) * 3600.0 + g(4) * 60.0 + g(5)) * 1000.0;
            let t = TimeArray { dims: vec![1, 1], kind: TimeKind::CalDuration, data: vec![[months, g(2), ms]], tz: None, tz_name: String::new(), format: String::new() };
            one(Value::Time(t))
        }
        _ => Ok(None),
    }
}

fn days_in_month(y: i64, m: i64) -> i64 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    days_from_civil(ny, nm, 1) - days_from_civil(y, m, 1)
}
