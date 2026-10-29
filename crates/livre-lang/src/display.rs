//! Text rendering of values for statement output, `disp`, and the workspace browser,
//! following MATLAB's Live Editor display.

use crate::nd;
use crate::value::{CellArray, Kind, Matrix, StrArray, StructArray, Value};

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
    let e_form = fmt_exp(v, p - 1);
    let exp = e_form.split_once('e').and_then(|(_, e)| e.parse::<i32>().ok()).unwrap_or(0);
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
    // MATLAB format short: exponent form from 1000 up and at or below 0.001.
    if a >= 1e3 || a <= 1e-3 {
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
    // MATLAB format long: exponent form from 100 up and at or below 0.001.
    if a >= 1e2 || a <= 1e-3 {
        fmt_exp(v, 15)
    } else {
        // Always 15 decimals, as MATLAB shows 9.500000000000000 and 99.500000000000000.
        format!("{v:.15}")
    }
}

pub fn fmt_scalar(v: f64, f: NumFormat) -> String {
    match f {
        NumFormat::Short => fmt_short_scalar(v),
        NumFormat::Long => fmt_long_scalar(v),
    }
}

/// One element of a class-aware numeric array.
pub fn fmt_elem(m: &Matrix, i: usize, f: NumFormat) -> String {
    match m.kind {
        Kind::Int(_) | Kind::Logical => format!("{}", m.data[i] as i64),
        Kind::Char => char::from_u32(m.data[i] as u32).map(String::from).unwrap_or_default(),
        _ if m.is_complex() => fmt_complex(m.data[i], m.im_at(i), f),
        Kind::Single if f == NumFormat::Long => {
            if is_int(m.data[i]) { format!("{}", m.data[i] as i64) } else { format!("{:.7}", m.data[i]) }
        }
        _ => fmt_scalar(m.data[i], f),
    }
}

/// `3.0000 + 4.0000i`
pub fn fmt_complex(re: f64, im: f64, f: NumFormat) -> String {
    let d = if f == NumFormat::Long { 15 } else { 4 };
    let part = |x: f64| match special(x) {
        Some(s) => s,
        None if x.abs() >= 1e5 || (x != 0.0 && x.abs() < 1e-4) => fmt_exp(x, d),
        None => format!("{:.*}", d, x),
    };
    let sign = if im < 0.0 || (im == 0.0 && im.is_sign_negative()) { '-' } else { '+' };
    format!("{} {sign} {}i", part(re), part(im.abs()))
}

pub fn size_text(rows: usize, cols: usize) -> String {
    format!("{rows}×{cols}")
}

/// Rows of one 2-D page of a numeric matrix (`page` is the page's data offset).
fn page_body(m: &Matrix, page: usize, rows: usize, cols: usize, f: NumFormat) -> String {
    let mut out = String::new();
    let at = |r: usize, c: usize| page + c * rows + r;
    match m.kind {
        Kind::Logical => {
            for r in 0..rows {
                for c in 0..cols {
                    out.push_str(&format!("{:>4}", m.data[at(r, c)] as i64));
                }
                out.push('\n');
            }
            return out;
        }
        Kind::Char => {
            for r in 0..rows {
                let row: String = (0..cols).map(|c| char::from_u32(m.data[at(r, c)] as u32).unwrap_or('?')).collect();
                out.push_str(&format!("    '{row}'\n"));
            }
            return out;
        }
        Kind::Int(_) => {
            let w = (0..m.numel()).map(|i| format!("{}", m.data[i] as i64).len()).max().unwrap_or(1) + 3;
            for r in 0..rows {
                for c in 0..cols {
                    out.push_str(&format!("{:>w$}", m.data[at(r, c)] as i64));
                }
                out.push('\n');
            }
            return out;
        }
        _ => {}
    }
    if m.is_complex() {
        let cells: Vec<String> = (0..rows * cols).map(|k| fmt_complex(m.data[page + k], m.im_at(page + k), f)).collect();
        let w = cells.iter().map(|s| s.len()).max().unwrap_or(1) + 3;
        for r in 0..rows {
            for c in 0..cols {
                out.push_str(&format!("{:>w$}", cells[c * rows + r]));
            }
            out.push('\n');
        }
        return out;
    }
    let finite: Vec<f64> = m.data.iter().copied().filter(|v| v.is_finite()).collect();
    let all_int = m.data.iter().all(|&v| is_int(v) || !v.is_finite());
    let max_abs = finite.iter().fold(0.0f64, |a, &v| a.max(v.abs()));
    let any_neg = m.data.iter().any(|&v| v < 0.0);
    if all_int && max_abs < 1e9 {
        let w = m.data.iter().map(|&v| fmt_short_scalar(v).len()).max().unwrap_or(1).max(3) + 3;
        for r in 0..rows {
            for c in 0..cols {
                out.push_str(&format!("{:>w$}", fmt_short_scalar(m.data[at(r, c)])));
            }
            out.push('\n');
        }
        return out;
    }
    let decimals = if f == NumFormat::Long { if m.kind == Kind::Single { 7 } else { 15 } } else { 4 };
    let mut scale = 1.0;
    // A common scale factor, as MATLAB uses from 1000 up (format short) or below 0.001.
    let big = if f == NumFormat::Long { 1e5 } else { 1e3 };
    if max_abs >= big || (max_abs > 0.0 && max_abs < 1e-3) {
        // The exponent rounds down for large values and up for small ones (1.0e-03 for 0.0009).
        let e = max_abs.log10();
        scale = 10f64.powf(if max_abs >= 1.0 { e.floor() } else { e.ceil() });
        out.push_str(&format!("   {} *\n\n", fmt_exp(scale, 1)));
    }
    let scaled_max = max_abs / scale;
    let int_digits = if scaled_max >= 1.0 { scaled_max.log10().floor() as usize + 1 } else { 1 };
    let w = int_digits + 1 + decimals + usize::from(any_neg) + 3;
    for r in 0..rows {
        for c in 0..cols {
            let v = m.data[at(r, c)];
            // Exact zeros show as a bare 0, as in MATLAB.
            let s = special(v).unwrap_or_else(|| if v == 0.0 { "0".into() } else { format!("{:.*}", decimals, v / scale) });
            out.push_str(&format!("{s:>w$}"));
        }
        out.push('\n');
    }
    out
}

/// Pages of an N-D array: calls `page(k, offset)` for each 2-D page with its `(:,:,…)` label.
fn pages(dims: &[usize], mut page: impl FnMut(String, usize) -> String) -> String {
    let rc = dims[0] * dims[1];
    let outer: usize = dims[2..].iter().product();
    let mut out = String::new();
    for p in 0..outer {
        let mut rem = p;
        let mut subs = Vec::new();
        for &d in &dims[2..] {
            subs.push(rem % d + 1);
            rem /= d;
        }
        let label = format!("(:,:,{})", subs.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(","));
        out.push_str(&page(label, p * rc));
    }
    out
}

fn matrix_body(m: &Matrix, f: NumFormat, name: &str) -> String {
    if m.is_2d() {
        return page_body(m, 0, m.rows(), m.cols(), f);
    }
    let (r, c) = (m.dims[0], m.dims[1]);
    pages(&m.dims, |label, off| format!("{name}{label} =\n\n{}\n", page_body(m, off, r, c, f)))
}

fn shape_word(dims: &[usize]) -> &'static str {
    if dims.len() > 2 {
        "array"
    } else if dims[0] == 1 {
        "row vector"
    } else if dims[1] == 1 {
        "column vector"
    } else {
        "matrix"
    }
}

/// One-line summary used inside cells, struct fields, and the workspace browser.
pub fn summary(v: &Value, f: NumFormat) -> String {
    match v {
        Value::Num(m) => {
            if m.kind == Kind::Char && m.is_2d() && m.rows() <= 1 {
                return format!("'{}'", m.to_string_lossy());
            }
            if m.is_empty() {
                return "[]".into();
            }
            if m.is_scalar() {
                return fmt_elem(m, 0, f);
            }
            if m.is_2d() && m.rows() == 1 && m.cols() <= 10 && !m.is_complex() {
                let items: Vec<String> = (0..m.numel()).map(|i| fmt_elem(m, i, f)).collect();
                return format!("[{}]", items.join(" "));
            }
            let cls = if m.is_complex() { format!("complex {}", v.class_name()) } else { v.class_name() };
            format!("{} {}", nd::size_text(&m.dims), cls)
        }
        Value::Str(s) => {
            if s.data.len() == 1 {
                match &s.data[0] {
                    Some(t) => format!("\"{t}\""),
                    None => "<missing>".into(),
                }
            } else {
                format!("{} string", nd::size_text(&s.dims))
            }
        }
        Value::Cell(c) => format!("{} cell", nd::size_text(&c.dims)),
        Value::Struct(s) => format!("{} struct", nd::size_text(&s.dims)),
        Value::Func(fv) => func_text(fv),
        Value::Object(o) => format!("1×1 {}", o.class.name),
        Value::Time(t) => t.summary(),
        Value::Table(t) => format!("{}×{} table", t.nrows, t.vars.len()),
        Value::Sym(s) => s.summary(),
    }
}

pub fn func_text(fv: &crate::value::FuncValue) -> String {
    match fv {
        crate::value::FuncValue::Named(n) => format!("@{n}"),
        crate::value::FuncValue::Anon { f, .. } => f.text.clone(),
    }
}

/// How a cell element is aligned within its column: text and descriptions pad on the right
/// inside the braces; numbers are right-aligned inside their brackets; strings are
/// left-aligned inside theirs.
enum CellAlign {
    Left,
    NumBrackets,
    StrBrackets,
}

/// The text between the braces of a cell element.
fn cell_item(v: &Value, f: NumFormat) -> (String, CellAlign) {
    match v {
        // Text up to its first line break (MATLAB marks the rest with ...); empty text by size.
        Value::Num(m) if m.kind == Kind::Char && m.is_2d() && m.rows() <= 1 && !m.is_empty() => {
            let t = m.to_string_lossy();
            let shown = match t.find('\n') {
                Some(p) => format!("'{}...'", &t[..p]),
                None => format!("'{t}'"),
            };
            (shown, CellAlign::Left)
        }
        Value::Num(m) if m.is_empty() => (format!("{} {}", nd::size_text(&m.dims), m.kind.name()), CellAlign::Left),
        Value::Num(m) if m.is_scalar() || (m.is_2d() && m.rows() == 1 && m.cols() <= 10 && !m.is_complex()) => {
            let s = summary(v, f);
            let inner = s.strip_prefix('[').and_then(|t| t.strip_suffix(']')).unwrap_or(&s).to_string();
            (inner, CellAlign::NumBrackets)
        }
        Value::Str(s) if s.data.len() == 1 => (summary(v, f), CellAlign::StrBrackets),
        other => (summary(other, f), CellAlign::Left),
    }
}

fn grid(rows: usize, cols: usize, items: &[String], sep: &str) -> String {
    let mut widths = vec![0usize; cols];
    for c in 0..cols {
        for r in 0..rows {
            widths[c] = widths[c].max(items[c * rows + r].chars().count());
        }
    }
    let mut out = String::new();
    for r in 0..rows {
        for c in 0..cols {
            let s = &items[c * rows + r];
            out.push_str(sep);
            out.push_str(s);
            out.push_str(&" ".repeat(widths[c] - s.chars().count()));
        }
        out.push('\n');
    }
    out
}

fn cell_body(c: &CellArray, f: NumFormat) -> String {
    let (rows, cols) = (c.rows(), c.cols());
    let raw: Vec<(String, CellAlign)> = c.data.iter().map(|v| cell_item(v, f)).collect();
    // Width between the braces, brackets included.
    let len = |s: &str, a: &CellAlign| s.chars().count() + if matches!(a, CellAlign::Left) { 0 } else { 2 };
    // Each column is as wide as its widest element.
    let widths: Vec<usize> = (0..cols).map(|j| (0..rows).map(|i| { let (s, a) = &raw[j * rows + i]; len(s, a) }).max().unwrap_or(0)).collect();
    let items: Vec<String> = raw
        .iter()
        .enumerate()
        .map(|(k, (s, a))| {
            let w = widths[k / rows.max(1)];
            let pad = " ".repeat(w.saturating_sub(len(s, a)));
            match a {
                CellAlign::Left => format!("{{{s}{pad}}}"),
                CellAlign::NumBrackets => format!("{{[{pad}{s}]}}"),
                CellAlign::StrBrackets => format!("{{[{s}{pad}]}}"),
            }
        })
        .collect();
    grid(rows, cols, &items, "    ")
}


fn str_body(s: &StrArray) -> String {
    let items: Vec<String> = s.data.iter().map(|x| match x {
        Some(t) => format!("\"{t}\""),
        None => "<missing>".into(),
    }).collect();
    grid(s.dims[0], s.dims[1..].iter().product(), &items, "    ")
}

fn struct_fields_body(s: &StructArray, f: NumFormat) -> String {
    let w = s.fields.iter().map(|n| n.len()).max().unwrap_or(0);
    let mut out = String::new();
    for (k, n) in s.fields.iter().enumerate() {
        // A 1×1 cell holding a simple value shows its content inline, as {[1]} or {'hi'}.
        let v = &s.elems[0][k];
        let shown = match v {
            Value::Cell(c) if c.data.len() == 1 && !matches!(c.data[0], Value::Cell(_) | Value::Struct(_)) => {
                let (t, a) = cell_item(&c.data[0], f);
                match a {
                    CellAlign::Left => format!("{{{t}}}"),
                    _ => format!("{{[{t}]}}"),
                }
            }
            other => summary(other, f),
        };
        out.push_str(&format!("    {n:>w$}: {shown}\n"));
    }
    out
}

fn struct_names_body(s: &StructArray) -> String {
    s.fields.iter().map(|n| format!("    {n}\n")).collect()
}

/// The text shown for an unsuppressed statement: `name = value`.
pub fn display_value(name: &str, v: &Value, f: NumFormat) -> String {
    match v {
        Value::Num(m) => {
            if m.kind == Kind::Char && m.is_2d() && m.rows() <= 1 {
                return format!("{name} = '{}'", m.to_string_lossy());
            }
            if m.is_empty() {
                if m.dims == [0, 0] && m.kind == Kind::Double {
                    return format!("{name} = []");
                }
                return format!("{name} = {} empty {} array", nd::size_text(&m.dims), m.kind.name());
            }
            if m.is_scalar() {
                return match m.kind {
                    Kind::Double => format!("{name} = {}", fmt_elem(m, 0, f)),
                    k => format!("{name} = {}\n   {}", if m.is_complex() { format!("{} complex", k.name()) } else { k.name().into() }, fmt_elem(m, 0, f)),
                };
            }
            let header = match m.kind {
                Kind::Double if m.is_complex() => format!("{} complex", nd::size_text(&m.dims)),
                Kind::Double => nd::size_text(&m.dims),
                Kind::Logical | Kind::Char => format!("{} {} array", nd::size_text(&m.dims), m.kind.name()),
                k => format!("{} {}{} {}", nd::size_text(&m.dims), if m.is_complex() { "complex " } else { "" }, k.name(), shape_word(&m.dims)),
            };
            format!("{name} = {header}\n\n{}", matrix_body(m, f, name).trim_end_matches('\n'))
        }
        Value::Str(s) => {
            if s.data.len() == 1 {
                return format!("{name} = {}", summary(v, f));
            }
            if s.data.is_empty() {
                return format!("{name} = \n  {} empty string array", nd::size_text(&s.dims));
            }
            format!("{name} = {} string array\n\n{}", nd::size_text(&s.dims), str_body(s).trim_end_matches('\n'))
        }
        Value::Cell(c) => {
            if c.data.is_empty() {
                return format!("{name} =\n\n  {} empty cell array", nd::size_text(&c.dims));
            }
            if c.dims.len() > 2 {
                return format!("{name} = {} cell array", nd::size_text(&c.dims));
            }
            format!("{name} = {} cell\n\n{}", nd::size_text(&c.dims), cell_body(c, f).trim_end_matches('\n'))
        }
        Value::Struct(s) => {
            if s.elems.len() == 1 {
                if s.fields.is_empty() {
                    return format!("{name} = struct with no fields.");
                }
                return format!("{name} = struct with fields:\n\n{}", struct_fields_body(s, f).trim_end_matches('\n'));
            }
            let what = if s.elems.is_empty() { format!("{} empty struct array", nd::size_text(&s.dims)) } else { format!("{} struct array", nd::size_text(&s.dims)) };
            if s.fields.is_empty() {
                return format!("{name} = {what} with no fields.");
            }
            format!("{name} = {what} with fields:\n\n{}", struct_names_body(s).trim_end_matches('\n'))
        }
        Value::Func(fv) => format!("{name} = function_handle with value:\n\n    {}", func_text(fv)),
        Value::Object(o) => crate::class::display_object(name, o, f),
        Value::Time(t) => t.display(name),
        Value::Table(t) => t.display(name, f),
        Value::Sym(s) => s.display(name),
    }
}

/// Text produced by `disp(v)`.
pub fn disp_value(v: &Value, f: NumFormat) -> String {
    match v {
        Value::Num(m) => {
            if m.kind == Kind::Char {
                return (0..m.rows()).map(|r| (0..m.cols()).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or('?')).collect::<String>()).collect::<Vec<_>>().join("\n");
            }
            if m.is_empty() {
                return String::new();
            }
            if m.is_scalar() {
                return if m.kind == Kind::Logical || m.kind.is_int() { format!("   {}", fmt_elem(m, 0, f)) } else { fmt_elem(m, 0, f) };
            }
            matrix_body(m, f, "ans").trim_end_matches('\n').to_string()
        }
        Value::Str(s) => {
            if s.data.len() == 1 {
                return s.data[0].clone().unwrap_or_else(|| "<missing>".into());
            }
            str_body(s).trim_end_matches('\n').to_string()
        }
        Value::Cell(c) => cell_body(c, f).trim_end_matches('\n').to_string(),
        Value::Struct(s) => {
            if s.elems.len() == 1 {
                struct_fields_body(s, f).trim_end_matches('\n').to_string()
            } else {
                format!("  {} struct array with fields:\n\n{}", nd::size_text(&s.dims), struct_names_body(s).trim_end_matches('\n'))
            }
        }
        Value::Func(fv) => func_text(fv),
        Value::Object(o) => crate::class::display_object("", o, f).trim_start_matches(" = ").to_string(),
        Value::Time(t) => t.disp(),
        Value::Table(t) => t.body(f),
        Value::Sym(s) => s.disp(),
    }
}
