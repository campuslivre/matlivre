//! `sprintf` / `fprintf` formatting: C-style conversions, escape sequences, and MATLAB's
//! recycling of the format over all supplied data.

use crate::display::{fmt_exp, fmt_g};
use crate::error::{rt, LangError};
use crate::value::{Kind, Value};

enum Atom {
    Num(f64),
    Str(String),
}

struct Spec {
    flags: String,
    width: Option<usize>,
    precision: Option<usize>,
    conv: char,
}

enum Seg {
    Lit(String),
    Spec(Spec),
}

pub fn process_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('a') => out.push('\u{7}'),
                Some('b') => out.push('\u{8}'),
                Some('f') => out.push('\u{c}'),
                Some('v') => out.push('\u{b}'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_format(fmt: &str) -> Vec<Seg> {
    let chars: Vec<char> = fmt.chars().collect();
    let mut segs = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            lit.push(c);
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'%') {
            lit.push('%');
            i += 2;
            continue;
        }
        let start = i;
        i += 1;
        let mut flags = String::new();
        while let Some(&f) = chars.get(i) {
            if "-+ 0#".contains(f) {
                flags.push(f);
                i += 1;
            } else {
                break;
            }
        }
        let mut width = None;
        let mut w = String::new();
        while let Some(&d) = chars.get(i) {
            if d.is_ascii_digit() {
                w.push(d);
                i += 1;
            } else {
                break;
            }
        }
        if !w.is_empty() {
            width = w.parse().ok();
        }
        let mut precision = None;
        if chars.get(i) == Some(&'.') {
            i += 1;
            let mut p = String::new();
            while let Some(&d) = chars.get(i) {
                if d.is_ascii_digit() {
                    p.push(d);
                    i += 1;
                } else {
                    break;
                }
            }
            precision = Some(p.parse().unwrap_or(0));
        }
        // Length modifiers are accepted and ignored.
        while matches!(chars.get(i), Some('l') | Some('h')) {
            i += 1;
        }
        match chars.get(i) {
            Some(&conv) if "dicuoxXfeEgGs".contains(conv) => {
                i += 1;
                if !lit.is_empty() {
                    segs.push(Seg::Lit(std::mem::take(&mut lit)));
                }
                segs.push(Seg::Spec(Spec { flags, width, precision, conv }));
            }
            _ => {
                // Not a valid conversion: keep the text literally.
                lit.extend(&chars[start..i]);
            }
        }
    }
    if !lit.is_empty() {
        segs.push(Seg::Lit(lit));
    }
    segs
}

fn pad(s: String, spec: &Spec, numeric: bool) -> String {
    let Some(w) = spec.width else { return s };
    let len = s.chars().count();
    if len >= w {
        return s;
    }
    let fill = w - len;
    if spec.flags.contains('-') {
        format!("{s}{}", " ".repeat(fill))
    } else if spec.flags.contains('0') && numeric {
        let (sign, rest) = if s.starts_with('-') || s.starts_with('+') { s.split_at(1) } else { ("", s.as_str()) };
        format!("{sign}{}{rest}", "0".repeat(fill))
    } else {
        format!("{}{s}", " ".repeat(fill))
    }
}

fn sign_prefix(v: f64, body: String, spec: &Spec) -> String {
    if v >= 0.0 && !body.starts_with('-') {
        if spec.flags.contains('+') {
            return format!("+{body}");
        }
        if spec.flags.contains(' ') {
            return format!(" {body}");
        }
    }
    body
}

fn format_num(v: f64, spec: &Spec) -> String {
    let conv = spec.conv;
    let body = match conv {
        'd' | 'i' | 'u' => {
            if v.is_finite() && v.fract() == 0.0 {
                format!("{}", v as i64)
            } else if !v.is_finite() {
                fmt_g(v, 6)
            } else {
                fmt_exp(v, spec.precision.unwrap_or(6))
            }
        }
        'c' => char::from_u32(v as u32).map(String::from).unwrap_or_default(),
        'o' => format!("{:o}", v as i64),
        'x' => format!("{:x}", v as i64),
        'X' => format!("{:X}", v as i64),
        'f' => {
            if v.is_finite() {
                format!("{:.*}", spec.precision.unwrap_or(6), v)
            } else {
                fmt_g(v, 6)
            }
        }
        'e' => fmt_exp(v, spec.precision.unwrap_or(6)),
        'E' => fmt_exp(v, spec.precision.unwrap_or(6)).to_uppercase(),
        'g' => fmt_g(v, spec.precision.unwrap_or(6)),
        'G' => fmt_g(v, spec.precision.unwrap_or(6)).to_uppercase(),
        's' => {
            if v.is_finite() && v.fract() == 0.0 && v >= 0.0 && v < 1_114_112.0 {
                char::from_u32(v as u32).map(String::from).unwrap_or_default()
            } else {
                fmt_g(v, 5)
            }
        }
        _ => String::new(),
    };
    let body = if matches!(conv, 'c' | 's') { body } else { sign_prefix(v, body, spec) };
    pad(body, spec, !matches!(conv, 'c' | 's'))
}

fn format_str(s: &str, spec: &Spec) -> String {
    let s = match (spec.conv, spec.precision) {
        ('s', Some(p)) => s.chars().take(p).collect(),
        _ => s.to_string(),
    };
    pad(s, spec, false)
}

fn atoms(args: &[Value]) -> Result<Vec<Atom>, LangError> {
    let mut out = Vec::new();
    for a in args {
        match a {
            Value::Num(m) if m.kind == Kind::Char => out.push(Atom::Str(m.to_string_lossy())),
            Value::Num(m) => out.extend(m.data.iter().map(|&x| Atom::Num(x))),
            Value::Str(s) => out.push(Atom::Str(s.clone())),
            Value::Cell(_) => return rt("Function is not defined for 'cell' inputs."),
            Value::Struct(_) => return rt("Function is not defined for 'struct' inputs."),
            Value::Func(_) => return rt("Function is not defined for 'function_handle' inputs."),
        }
    }
    Ok(out)
}

pub fn sprintf(fmt: &str, args: &[Value]) -> Result<String, LangError> {
    let fmt = process_escapes(fmt);
    let segs = parse_format(&fmt);
    let data = atoms(args)?;
    let nspecs = segs.iter().filter(|s| matches!(s, Seg::Spec(_))).count();
    let mut out = String::new();
    if nspecs == 0 || data.is_empty() {
        for s in &segs {
            if let Seg::Lit(l) = s {
                out.push_str(l);
            }
        }
        return Ok(out);
    }
    let mut k = 0;
    loop {
        for s in &segs {
            match s {
                Seg::Lit(l) => out.push_str(l),
                Seg::Spec(spec) => {
                    if k >= data.len() {
                        // Out of data: stop at the first conversion without a value (but
                        // always emit the first pass's leading text when there is no data).
                        return Ok(out);
                    }
                    let piece = match &data[k] {
                        Atom::Num(v) => format_num(*v, spec),
                        Atom::Str(st) => {
                            if matches!(spec.conv, 's' | 'c') {
                                format_str(st, spec)
                            } else {
                                // A char argument to a numeric conversion prints as text.
                                format_str(st, &Spec { conv: 's', precision: None, flags: spec.flags.clone(), width: spec.width })
                            }
                        }
                    };
                    out.push_str(&piece);
                    k += 1;
                }
            }
        }
        if k >= data.len() {
            return Ok(out);
        }
    }
}
