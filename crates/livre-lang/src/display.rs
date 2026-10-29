//! Text rendering of values for statement output, `disp`, and the workspace browser.

use crate::value::{CellArray, Kind, Matrix, Struct, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NumFormat {
    #[default]
    Short,
    Long,
}

fn is_int(v: f64) -> bool {
    v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15
}

fn special(v: f64) -> Option<String> {
    if v.is_nan() {
        Some("NaN".into())
    } else if v.is_infinite() {
        Some(if v > 0.0 { "Inf".into() } else { "-Inf".into() })
    } else {
        None
    }
}

/// C-style `%.{prec}e` (`1.2346e+05`).
pub fn fmt_exp(v: f64, prec: usize) -> String {
    if let Some(s) = special(v) {
        return s;
    }
    let s = format!("{:.*e}", prec, v);
    // Rust gives `1.2346e5`; normalize the exponent to a sign and two digits.
    match s.split_once('e') {
        Some((mant, exp)) => {
            let (sign, digits) = match exp.strip_prefix('-') {
                Some(d) => ('-', d),
                None => ('+', exp),
            };
            format!("{mant}e{sign}{:0>2}", digits)
        }
        None => s,
    }
}

/// C-style `%.{prec}g`.
pub fn fmt_g(v: f64, prec: usize) -> String {
    if let Some(s) = special(v) {
        return s;
    }
    if v == 0.0 {
        return "0".into();
    }
    let p = prec.max(1);
    let exp = v.abs().log10().floor() as i32;
    // Rounding can bump the exponent (9.99995 -> 10.000); recompute from the %e form.
    let e_form = fmt_exp(v, p - 1);
    let exp = e_form
        .split_once('e')
        .and_then(|(_, e)| e.parse::<i32>().ok())
        .unwrap_or(exp);
    if exp < -4 || exp >= p as i32 {
        let (mant, e) = e_form.split_once('e').unwrap();
        format!("{}e{}", strip_zeros(mant), e)
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        strip_zeros(&format!("{:.*}", decimals, v))
    }
}

fn strip_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

pub fn fmt_short_scalar(v: f64) -> String {
    if let Some(s) = special(v) {
        return s;
    }
    if is_int(v) && v.abs() < 1e9 {
        return format!("{}", v as i64);
    }
    let a = v.abs();
    if a >= 1e5 || a < 1e-4 {
        fmt_exp(v, 4)
    } else {
        format!("{v:.4}")
    }
}

pub fn fmt_long_scalar(v: f64) -> String {
    if let Some(s) = special(v) {
        return s;
    }
    if is_int(v) && v.abs() < 1e9 {
        return format!("{}", v as i64);
    }
    let a = v.abs();
    if a >= 1e5 || a < 1e-4 {
        fmt_exp(v, 15)
    } else {
        let int_digits = (a.log10().floor() as i32 + 1).max(1) as usize;
        format!("{:.*}", 16usize.saturating_sub(int_digits), v)
    }
}

pub fn fmt_scalar(v: f64, f: NumFormat) -> String {
    match f {
        NumFormat::Short => fmt_short_scalar(v),
        NumFormat::Long => fmt_long_scalar(v),
    }
}

/// `2×3` size text.
pub fn size_text(rows: usize, cols: usize) -> String {
    format!("{rows}×{cols}")
}

/// Rows of a numeric matrix, without header, each line indented as MATLAB does.
fn matrix_body(m: &Matrix, f: NumFormat) -> String {
    let mut out = String::new();
    if m.kind == Kind::Logical {
        for r in 0..m.rows {
            for c in 0..m.cols {
                out.push_str(&format!("{:>4}", m.get(r, c) as i64));
            }
            out.push('\n');
        }
        return out;
    }
    if m.kind == Kind::Char {
        for r in 0..m.rows {
            let row: String = (0..m.cols).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or('?')).collect();
            out.push_str(&format!("    '{row}'\n"));
        }
        return out;
    }
    let finite: Vec<f64> = m.data.iter().copied().filter(|v| v.is_finite()).collect();
    let all_int = m.data.iter().all(|&v| is_int(v) || !v.is_finite());
    let max_abs = finite.iter().fold(0.0f64, |a, &v| a.max(v.abs()));
    let any_neg = m.data.iter().any(|&v| v < 0.0);
    if all_int && max_abs < 1e9 {
        let w = m.data.iter().map(|&v| fmt_short_scalar(v).len()).max().unwrap_or(1).max(3) + 3;
        for r in 0..m.rows {
            for c in 0..m.cols {
                out.push_str(&format!("{:>w$}", fmt_short_scalar(m.get(r, c))));
            }
            out.push('\n');
        }
        return out;
    }
    let decimals = if f == NumFormat::Long { 15 } else { 4 };
    let mut scale = 1.0;
    if max_abs >= 1e5 || (max_abs > 0.0 && max_abs < 1e-3) {
        let e = max_abs.log10().floor();
        scale = 10f64.powf(e);
        out.push_str(&format!("   {} *\n\n", fmt_exp(scale, 1).replace("1.0e", "1.0e")));
    }
    let scaled_max = max_abs / scale;
    let int_digits = if scaled_max >= 1.0 { scaled_max.log10().floor() as usize + 1 } else { 1 };
    let w = int_digits + 1 + decimals + if any_neg { 1 } else { 0 } + 3;
    for r in 0..m.rows {
        for c in 0..m.cols {
            let v = m.get(r, c);
            let s = match special(v) {
                Some(s) => s,
                None => format!("{:.*}", decimals, v / scale),
            };
            out.push_str(&format!("{s:>w$}"));
        }
        out.push('\n');
    }
    out
}

/// One-line summary used inside cells, struct fields, and the workspace browser.
pub fn summary(v: &Value, f: NumFormat) -> String {
    match v {
        Value::Num(m) => {
            if m.kind == Kind::Char && m.rows <= 1 {
                return format!("'{}'", m.to_string_lossy());
            }
            if m.is_empty() {
                return "[]".into();
            }
            if m.is_scalar() {
                return if m.kind == Kind::Logical {
                    (if m.data[0] != 0.0 { "1" } else { "0" }).into()
                } else {
                    fmt_scalar(m.data[0], f)
                };
            }
            if m.rows == 1 && m.cols <= 10 {
                let items: Vec<String> = m.data.iter().map(|&x| fmt_scalar(x, f)).collect();
                return format!("[{}]", items.join(" "));
            }
            format!("{} {}", size_text(m.rows, m.cols), v.class_name())
        }
        Value::Str(s) => format!("\"{s}\""),
        Value::Cell(c) => format!("{} cell", size_text(c.rows, c.cols)),
        Value::Struct(_) => "1×1 struct".into(),
        Value::Func(fv) => func_text(fv),
    }
}

pub fn func_text(fv: &crate::value::FuncValue) -> String {
    match fv {
        crate::value::FuncValue::Named(n) => format!("@{n}"),
        crate::value::FuncValue::Anon { f, .. } => f.text.clone(),
    }
}

fn cell_item(v: &Value, f: NumFormat) -> String {
    match v {
        Value::Num(m) if m.kind == Kind::Char && m.rows <= 1 => format!("{{'{}'}}", m.to_string_lossy()),
        Value::Num(m) if m.is_scalar() || (m.rows == 1 && m.cols <= 10) || m.is_empty() => {
            let s = summary(v, f);
            if s.starts_with('[') { format!("{{{s}}}") } else { format!("{{[{s}]}}") }
        }
        Value::Str(s) => format!("{{[\"{s}\"]}}"),
        other => format!("{{{}}}", summary(other, f)),
    }
}

fn cell_body(c: &CellArray, f: NumFormat) -> String {
    let items: Vec<String> = c.data.iter().map(|v| cell_item(v, f)).collect();
    let mut widths = vec![0usize; c.cols];
    for col in 0..c.cols {
        for r in 0..c.rows {
            widths[col] = widths[col].max(items[col * c.rows + r].chars().count());
        }
    }
    let mut out = String::new();
    for r in 0..c.rows {
        for col in 0..c.cols {
            let s = &items[col * c.rows + r];
            out.push_str("    ");
            out.push_str(s);
            out.push_str(&" ".repeat(widths[col] - s.chars().count()));
        }
        out.push('\n');
    }
    out
}

fn struct_body(s: &Struct, f: NumFormat) -> String {
    let w = s.fields.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    let mut out = String::new();
    for (n, v) in &s.fields {
        out.push_str(&format!("    {n:>w$}: {}\n", summary(v, f)));
    }
    out
}

/// The text shown for an unsuppressed statement: `name = value`.
pub fn display_value(name: &str, v: &Value, f: NumFormat) -> String {
    match v {
        Value::Num(m) => {
            if m.kind == Kind::Char && m.rows <= 1 {
                return format!("{name} = '{}'", m.to_string_lossy());
            }
            if m.is_empty() {
                if m.rows == 0 && m.cols == 0 {
                    return format!("{name} = []");
                }
                return format!("{name} = {} empty {} array", size_text(m.rows, m.cols), v.class_name());
            }
            if m.is_scalar() {
                if m.kind == Kind::Logical {
                    return format!("{name} = logical\n   {}", m.data[0] as i64);
                }
                return format!("{name} = {}", fmt_scalar(m.data[0], f));
            }
            let kind = match m.kind {
                Kind::Double => "",
                Kind::Logical => " logical array",
                Kind::Char => " char array",
            };
            format!("{name} = {}{kind}\n\n{}", size_text(m.rows, m.cols), matrix_body(m, f).trim_end_matches('\n'))
        }
        Value::Str(s) => format!("{name} = \"{s}\""),
        Value::Cell(c) => {
            if c.data.is_empty() {
                return format!("{name} =\n\n  {} empty cell array", size_text(c.rows, c.cols));
            }
            format!("{name} = {} cell\n\n{}", size_text(c.rows, c.cols), cell_body(c, f).trim_end_matches('\n'))
        }
        Value::Struct(s) => format!("{name} = struct with fields:\n\n{}", struct_body(s, f).trim_end_matches('\n')),
        Value::Func(fv) => format!("{name} = function_handle with value:\n\n    {}", func_text(fv)),
    }
}

/// Text produced by `disp(v)`.
pub fn disp_value(v: &Value, f: NumFormat) -> String {
    match v {
        Value::Num(m) => {
            if m.kind == Kind::Char {
                return (0..m.rows)
                    .map(|r| (0..m.cols).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or('?')).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n");
            }
            if m.is_empty() {
                return String::new();
            }
            if m.is_scalar() {
                return if m.kind == Kind::Logical {
                    format!("   {}", m.data[0] as i64)
                } else {
                    fmt_scalar(m.data[0], f)
                };
            }
            matrix_body(m, f).trim_end_matches('\n').to_string()
        }
        Value::Str(s) => s.clone(),
        Value::Cell(c) => cell_body(c, f).trim_end_matches('\n').to_string(),
        Value::Struct(s) => struct_body(s, f).trim_end_matches('\n').to_string(),
        Value::Func(fv) => func_text(fv),
    }
}
