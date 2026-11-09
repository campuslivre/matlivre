//! Text functions over char vectors, string arrays and cell arrays of char vectors. Results
//! keep the input's container type, as MATLAB does.

use crate::builtins::{arg, m_arg, none, num2str_scalar, one, Ret};
use crate::display::fmt_g;
use crate::error::{rt, LangError};
use crate::format::{process_escapes, sprintf};
use crate::interp::Interpreter;
use crate::nd::{self, Dims};
use crate::ops::to_str_array;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "sprintf", "fprintf", "num2str", "int2str", "mat2str", "str2double", "str2num", "num2cell", "string", "strings", "char", "cellstr", "strlength",
    "upper", "lower", "strtrim", "strip", "deblank", "strrep", "replace", "strcat", "append", "strcmp", "strcmpi", "strncmp", "strncmpi", "strsplit",
    "split", "strjoin", "join", "contains", "startsWith", "endsWith", "strfind", "extractBefore", "extractAfter", "extractBetween", "ismissing",
    "isspace", "isletter", "isstrprop", "reverse", "pad", "compose", "ischar", "iscellstr", "count", "erase", "insertAfter", "insertBefore", "newline",
    "blanks", "strjust", "convertStringsToChars", "convertCharsToStrings", "missing",
];

/// Text items of a value, with the container they came from.
enum Texts {
    Char(String),
    Strs(StrArray),
    Cells(Dims, Vec<String>),
}

fn texts(v: &Value) -> R<Texts> {
    Ok(match v {
        Value::Str(s) => Texts::Strs(s.clone()),
        Value::Cell(c) => Texts::Cells(c.dims.clone(), c.data.iter().map(|x| x.text()).collect::<R<_>>()?),
        Value::Num(m) if m.kind == Kind::Char || m.is_empty() => Texts::Char(m.to_string_lossy()),
        other => return rt(format!("Expected text input but got a value of class '{}'.", other.class_name())),
    })
}

/// Applies `f` to every text element, keeping the container type.
fn map_text(v: &Value, f: impl Fn(&str) -> String) -> R<Value> {
    Ok(match texts(v)? {
        Texts::Char(s) => Value::chars(&f(&s)),
        Texts::Strs(s) => Value::Str(StrArray::from_dims(s.dims.clone(), s.data.iter().map(|x| x.as_ref().map(|t| f(t))).collect())),
        Texts::Cells(d, items) => Value::Cell(CellArray::from_dims(d, items.iter().map(|t| Value::chars(&f(t))).collect())),
    })
}

/// Applies a predicate to every element; logical array result (scalar for char).
fn test_text(v: &Value, f: impl Fn(&str) -> bool) -> R<Value> {
    Ok(match texts(v)? {
        Texts::Char(s) => Value::bool(f(&s)),
        Texts::Strs(s) => Value::Num(Matrix::from_dims(s.dims.clone(), s.data.iter().map(|x| x.as_deref().map(&f).unwrap_or(false) as u8 as f64).collect()).with_kind(Kind::Logical)),
        Texts::Cells(d, items) => Value::Num(Matrix::from_dims(d, items.iter().map(|t| f(t) as u8 as f64).collect()).with_kind(Kind::Logical)),
    })
}

fn text_list(v: &Value) -> R<Vec<String>> {
    Ok(match texts(v)? {
        Texts::Char(s) => {
            if let Value::Num(m) = v {
                if m.rows() > 1 {
                    return Ok((0..m.rows()).map(|r| (0..m.cols()).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or('?')).collect()).collect());
                }
            }
            vec![s]
        }
        Texts::Strs(s) => s.data.into_iter().map(|x| x.unwrap_or_default()).collect(),
        Texts::Cells(_, items) => items,
    })
}

fn is_string_input(args: &[Value]) -> bool {
    args.iter().any(|a| matches!(a, Value::Str(_)))
}

pub fn sort_text(v: &Value, desc: bool) -> Ret {
    let items = text_list(v)?;
    let mut idx: Vec<usize> = (0..items.len()).collect();
    idx.sort_by(|&a, &b| if desc { items[b].cmp(&items[a]) } else { items[a].cmp(&items[b]) });
    let sorted: Vec<String> = idx.iter().map(|&i| items[i].clone()).collect();
    let d = v.dims();
    let out = match v {
        Value::Str(_) => Value::Str(StrArray::from_dims(d.clone(), sorted.into_iter().map(Some).collect())),
        _ => Value::Cell(CellArray::from_dims(d.clone(), sorted.iter().map(|s| Value::chars(s)).collect())),
    };
    Ok(Some(vec![out, Value::Num(Matrix::from_dims(d, idx.iter().map(|&i| (i + 1) as f64).collect()))]))
}

pub fn unique_text(v: &Value) -> Ret {
    let mut items = text_list(v)?;
    items.sort();
    items.dedup();
    let n = items.len();
    one(match v {
        Value::Str(_) => Value::Str(StrArray::col(items)),
        _ => Value::Cell(CellArray::from_dims(vec![n, 1], items.iter().map(|s| Value::chars(s)).collect())),
    })
}

pub fn ismember_text(x: &Value, set: &Value) -> Ret {
    let pool = text_list(set)?;
    let items = text_list(x)?;
    let d = if x.is_char() { vec![1, 1] } else { x.dims() };
    let loc: Vec<f64> = items.iter().map(|s| pool.iter().position(|p| p == s).map(|k| (k + 1) as f64).unwrap_or(0.0)).collect();
    Ok(Some(vec![
        Value::Num(Matrix::from_dims(d.clone(), loc.iter().map(|&l| (l > 0.0) as u8 as f64).collect()).with_kind(Kind::Logical)),
        Value::Num(Matrix::from_dims(d, loc)),
    ]))
}

fn char_matrix(items: &[String]) -> Value {
    let w = items.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let mut m = Matrix::zeros(items.len(), w).with_kind(Kind::Char);
    for (r, s) in items.iter().enumerate() {
        let chars: Vec<char> = s.chars().collect();
        for c in 0..w {
            m.set(r, c, *chars.get(c).unwrap_or(&' ') as u32 as f64);
        }
    }
    Value::Num(m)
}

fn pattern_list(v: &Value) -> R<Vec<String>> {
    text_list(v)
}

pub fn call(it: &mut Interpreter, name: &str, args: &[Value], nargout: usize) -> Ret {
    let a = args;
    match name {
        "sprintf" => {
            let f = arg(a, 0, name)?.text()?;
            let s = sprintf(&f, &a[1..])?;
            one(if matches!(a[0], Value::Str(_)) { Value::string(s) } else { Value::chars(&s) })
        }
        "fprintf" => {
            // fprintf(fid, format, ...) writes to a file; 1 and 2 are the output.
            let mut rest: &[Value] = a;
            let mut fid = 1;
            if let Some(Value::Num(m)) = rest.first() {
                if m.kind != Kind::Char && m.is_scalar() && rest.len() >= 2 {
                    fid = m.data[0] as i32;
                    rest = &rest[1..];
                }
            }
            let f = arg(rest, 0, name)?.text()?;
            let s = sprintf(&f, &rest[1..])?;
            let n = s.len();
            crate::builtins_files::write_text(it, fid, &s)?;
            if nargout > 0 { one(Value::num(n as f64)) } else { none() }
        }
        "num2str" => {
            let v = arg(a, 0, name)?;
            if v.is_text() {
                return one(Value::chars(&v.text()?));
            }
            let m = m_arg(a, 0, name)?;
            if let Some(f) = a.get(1) {
                if f.is_text() {
                    return one(Value::chars(&sprintf(&f.text()?, &a[..1])?));
                }
                let p = f.as_usize()?;
                let parts: Vec<String> = m.data.iter().map(|&x| fmt_g(x, p)).collect();
                return one(Value::chars(&parts.join("  ")));
            }
            let fmt1 = |i: usize| -> String {
                if m.is_complex() {
                    let im = m.im_at(i);
                    format!("{}{}{}i", num2str_scalar(m.data[i]), if im < 0.0 { "-" } else { "+" }, num2str_scalar(im.abs()))
                } else if m.kind == Kind::Logical || m.kind.is_int() {
                    format!("{}", m.data[i] as i64)
                } else {
                    num2str_scalar(m.data[i])
                }
            };
            if m.is_scalar() {
                return one(Value::chars(&fmt1(0)));
            }
            let cells: Vec<String> = (0..m.numel()).map(fmt1).collect();
            let w = cells.iter().map(|s| s.len()).max().unwrap_or(0);
            let (rows, cols) = (m.rows(), m.cols());
            let lines: Vec<String> = (0..rows).map(|r| (0..cols).map(|c| format!("{:>w$}", cells[c * rows + r])).collect::<Vec<_>>().join("  ")).collect();
            one(if lines.len() == 1 { Value::chars(&lines[0]) } else { char_matrix(&lines) })
        }
        "int2str" => {
            let m = m_arg(a, 0, name)?.map(f64::round);
            call(it, "num2str", &[Value::Num(m)], 1)
        }
        "mat2str" => {
            let v = arg(a, 0, name)?;
            if v.is_char() {
                return one(Value::chars(&format!("'{}'", v.text()?)));
            }
            let m = m_arg(a, 0, name)?;
            let prec = a.get(1).map(|p| p.as_usize()).transpose()?.unwrap_or(15);
            let f = |i: usize| -> String {
                if m.kind == Kind::Logical {
                    return (if m.data[i] != 0.0 { "true" } else { "false" }).into();
                }
                let re = fmt_g(m.data[i], prec);
                if m.is_complex() {
                    let im = m.im_at(i);
                    format!("{re}{}{}i", if im < 0.0 { "-" } else { "+" }, fmt_g(im.abs(), prec))
                } else {
                    re
                }
            };
            if m.is_scalar() {
                return one(Value::chars(&f(0)));
            }
            let (rows, cols) = (m.rows(), m.cols());
            let body: Vec<String> = (0..rows).map(|r| (0..cols).map(|c| f(c * rows + r)).collect::<Vec<_>>().join(" ")).collect();
            let s = format!("[{}]", body.join(";"));
            one(Value::chars(&if m.kind.is_int() || m.kind == Kind::Single { format!("{}({s})", m.kind.name()) } else { s }))
        }
        "str2double" => {
            let real = |t: &str| -> Option<f64> {
                if let Ok(v) = t.parse::<f64>() {
                    return Some(v);
                }
                match t.to_lowercase().as_str() {
                    "inf" | "+inf" => Some(f64::INFINITY),
                    "-inf" => Some(f64::NEG_INFINITY),
                    "nan" | "+nan" | "-nan" => Some(f64::NAN),
                    _ => None,
                }
            };
            // Real numbers, and complex ones written a+bi, a-bj, bi or i.
            let parse = |s: &str| -> (f64, f64) {
                let t: String = s.trim().replace(',', "").chars().filter(|c| !c.is_whitespace()).collect();
                if let Some(v) = real(&t) {
                    return (v, 0.0);
                }
                let Some(body) = t.strip_suffix(['i', 'j']) else { return (f64::NAN, 0.0) };
                let b = body.as_bytes();
                let split = (1..b.len()).rev().find(|&k| (b[k] == b'+' || b[k] == b'-') && !matches!(b[k - 1], b'e' | b'E'));
                let (re_t, im_t) = match split {
                    Some(k) => (&body[..k], &body[k..]),
                    None => ("", body),
                };
                let re = if re_t.is_empty() { Some(0.0) } else { real(re_t) };
                let im_t = im_t.strip_suffix('*').unwrap_or(im_t);
                let im = match im_t {
                    "" | "+" => Some(1.0),
                    "-" => Some(-1.0),
                    x => real(x),
                };
                match (re, im) {
                    (Some(r), Some(m)) => (r, m),
                    _ => (f64::NAN, 0.0),
                }
            };
            let build = |dims: Vec<usize>, vals: Vec<(f64, f64)>| -> Value {
                let re: Vec<f64> = vals.iter().map(|v| v.0).collect();
                if vals.iter().any(|v| v.1 != 0.0) {
                    Value::Num(Matrix::complex(dims, re, vals.iter().map(|v| v.1).collect()))
                } else {
                    Value::Num(Matrix::from_dims(dims, re))
                }
            };
            match arg(a, 0, name)? {
                Value::Cell(c) => one(build(c.dims.clone(), c.data.iter().map(|v| v.text().map(|s| parse(&s)).unwrap_or((f64::NAN, 0.0))).collect())),
                Value::Str(s) => one(build(s.dims.clone(), s.data.iter().map(|x| x.as_deref().map(parse).unwrap_or((f64::NAN, 0.0))).collect())),
                v if v.is_text() => one(build(vec![1, 1], vec![parse(&v.text()?)])),
                _ => one(Value::num(f64::NAN)),
            }
        }
        "str2num" => {
            let s = arg(a, 0, name)?.text()?;
            let e = crate::parser::parse_expr(&format!("[{s}]"));
            match e {
                Ok(e) => match it.eval(&e) {
                    Ok(v) => Ok(Some(vec![v, Value::bool(true)])),
                    Err(_) => Ok(Some(vec![Value::empty(), Value::bool(false)])),
                },
                Err(_) => Ok(Some(vec![Value::empty(), Value::bool(false)])),
            }
        }
        "num2cell" => {
            let v = arg(a, 0, name)?;
            let n = v.numel();
            let items = (0..n).map(|k| crate::indexing::index(v, &[crate::nd::IndexArg::scalar(k)])).collect::<R<Vec<_>>>()?;
            one(Value::Cell(CellArray::from_dims(v.dims(), items)))
        }
        "string" => {
            let v = arg(a, 0, name)?;
            if let Value::Num(m) = v {
                if m.kind != Kind::Char && !m.is_complex() && m.kind != Kind::Logical {
                    // Numbers become their shortest text (num2str-like) per element.
                    return one(Value::Str(StrArray::from_dims(m.dims.clone(), m.data.iter().map(|&x| if x.is_nan() { None } else { Some(fmt_g(x, if m.kind.is_int() { 20 } else { 5 })) }).collect())));
                }
            }
            if let Value::Time(t) = v {
                return one(Value::Str(crate::time::to_strings(t)));
            }
            one(Value::Str(to_str_array(v)?))
        }
        "convertCharsToStrings" => match arg(a, 0, name)? {
            v if v.is_char() || matches!(v, Value::Cell(_)) => one(Value::Str(to_str_array(v)?)),
            v => one(v.clone()),
        },
        "convertStringsToChars" => match arg(a, 0, name)? {
            Value::Str(s) if s.is_scalar() => one(Value::chars(s.data[0].as_deref().unwrap_or(""))),
            Value::Str(s) => one(Value::Cell(CellArray::from_dims(s.dims.clone(), s.data.iter().map(|x| Value::chars(x.as_deref().unwrap_or(""))).collect()))),
            v => one(v.clone()),
        },
        "strings" => {
            let d = crate::builtins::size_args(a)?;
            let n = nd::numel(&d);
            one(Value::Str(StrArray::from_dims(d, vec![Some(String::new()); n])))
        }
        "missing" => one(Value::Str(StrArray::from_dims(vec![1, 1], vec![None]))),
        // char(A, B, ...): the rows of every argument, padded with blanks to one width.
        "char" if a.len() > 1 => {
            let mut items: Vec<String> = Vec::new();
            for v in a {
                match v {
                    Value::Num(m) => {
                        let (r, c) = (m.rows(), m.cols());
                        if m.numel() == 0 {
                            items.push(String::new());
                        }
                        for i in 0..r {
                            items.push((0..c).map(|j| char::from_u32(m.data[j * r + i] as u32).unwrap_or('\0')).collect());
                        }
                    }
                    Value::Str(_) | Value::Cell(_) => items.extend(text_list(v)?),
                    _ => return rt("Conversion to char is not possible."),
                }
            }
            one(char_matrix(&items))
        }
        "char" => match arg(a, 0, name)? {
            Value::Num(m) => one(Value::Num(Matrix::from_dims(m.dims.clone(), m.data.clone()).with_kind(Kind::Char))),
            Value::Str(s) if s.is_scalar() => one(Value::chars(s.data[0].as_deref().unwrap_or(""))),
            v @ (Value::Str(_) | Value::Cell(_)) => {
                let mut items = text_list(v)?;
                for extra in &a[1..] {
                    items.extend(text_list(extra)?);
                }
                one(char_matrix(&items))
            }
            Value::Time(t) => {
                let items: Vec<String> = (0..t.data.len()).map(|i| t.format_elem(i)).collect();
                one(char_matrix(&items))
            }
            Value::Object(o) if crate::class::on_off_text(o).is_some() => one(Value::chars(&crate::class::on_off_text(o).unwrap_or_default())),
            _ => rt("Conversion to char is not possible."),
        },
        "cellstr" => {
            let v = arg(a, 0, name)?;
            match v {
                Value::Cell(_) => one(v.clone()),
                _ => {
                    let items = text_list(v)?;
                    let n = items.len();
                    let d = if let Value::Str(s) = v { s.dims.clone() } else { vec![n, 1] };
                    one(Value::Cell(CellArray::from_dims(d, items.iter().map(|s| Value::chars(s.trim_end())).collect())))
                }
            }
        }
        "strlength" => {
            let v = arg(a, 0, name)?;
            Ok(Some(vec![match texts(v)? {
                Texts::Char(s) => Value::num(s.chars().count() as f64),
                Texts::Strs(s) => Value::Num(Matrix::from_dims(s.dims.clone(), s.data.iter().map(|x| x.as_ref().map(|t| t.chars().count() as f64).unwrap_or(f64::NAN)).collect())),
                Texts::Cells(d, items) => Value::Num(Matrix::from_dims(d, items.iter().map(|t| t.chars().count() as f64).collect())),
            }]))
        }
        "upper" | "lower" => one(map_text(arg(a, 0, name)?, |s| if name == "upper" { s.to_uppercase() } else { s.to_lowercase() })?),
        "strtrim" => one(map_text(arg(a, 0, name)?, |s| s.trim_matches(|c: char| c.is_whitespace() || c == '\0').to_string())?),
        "deblank" => one(map_text(arg(a, 0, name)?, |s| s.trim_end_matches(|c: char| c.is_whitespace() || c == '\0').to_string())?),
        "strip" => {
            let side = a.get(1).map(|v| v.text()).transpose()?.unwrap_or_else(|| "both".into());
            let ch = a.get(2).map(|v| v.text()).transpose()?;
            one(map_text(arg(a, 0, name)?, |s| {
                let pred = |c: char| match &ch {
                    Some(t) => t.contains(c),
                    None => c.is_whitespace(),
                };
                match side.as_str() {
                    "left" => s.trim_start_matches(pred).to_string(),
                    "right" => s.trim_end_matches(pred).to_string(),
                    _ => s.trim_matches(pred).to_string(),
                }
            })?)
        }
        "strrep" | "replace" => {
            let froms = pattern_list(arg(a, 1, name)?)?;
            let tos = pattern_list(arg(a, 2, name)?)?;
            one(map_text(arg(a, 0, name)?, |s| {
                let mut out = s.to_string();
                for (k, f) in froms.iter().enumerate() {
                    if f.is_empty() {
                        continue;
                    }
                    let to = tos.get(k).or(tos.first()).cloned().unwrap_or_default();
                    out = out.replace(f.as_str(), &to);
                }
                out
            })?)
        }
        "erase" => {
            let pats = pattern_list(arg(a, 1, name)?)?;
            one(map_text(arg(a, 0, name)?, |s| pats.iter().fold(s.to_string(), |acc, p| if p.is_empty() { acc } else { acc.replace(p.as_str(), "") }))?)
        }
        "reverse" => one(map_text(arg(a, 0, name)?, |s| s.chars().rev().collect())?),
        "pad" => {
            let width = a.get(1).filter(|v| !v.is_text()).map(|v| v.as_usize()).transpose()?;
            let items = text_list(arg(a, 0, name)?)?;
            let w = width.unwrap_or_else(|| items.iter().map(|s| s.chars().count()).max().unwrap_or(0));
            let side = a.iter().skip(1).find(|v| v.is_text()).map(|v| v.text()).transpose()?.unwrap_or_else(|| "right".into());
            one(map_text(arg(a, 0, name)?, |s| {
                let n = s.chars().count();
                if n >= w {
                    return s.to_string();
                }
                let fill = w - n;
                match side.as_str() {
                    "left" => format!("{}{s}", " ".repeat(fill)),
                    "both" => format!("{}{s}{}", " ".repeat(fill / 2), " ".repeat(fill - fill / 2)),
                    _ => format!("{s}{}", " ".repeat(fill)),
                }
            })?)
        }
        // A numeric position counts characters: insertAfter(s, 2, t) puts t after the 2nd.
        "insertAfter" | "insertBefore" if matches!(a.get(1), Some(v) if !v.is_text() && !matches!(v, Value::Str(_) | Value::Cell(_))) => {
            let k = arg(a, 1, name)?.as_usize()?;
            let ins = arg(a, 2, name)?.text()?;
            let at = if name == "insertAfter" { k } else { k.saturating_sub(1) };
            one(map_text(arg(a, 0, name)?, |s| {
                let chars: Vec<char> = s.chars().collect();
                let at = at.min(chars.len());
                format!("{}{ins}{}", chars[..at].iter().collect::<String>(), chars[at..].iter().collect::<String>())
            })?)
        }
        "insertAfter" | "insertBefore" => {
            let pat = arg(a, 1, name)?.text()?;
            let ins = arg(a, 2, name)?.text()?;
            one(map_text(arg(a, 0, name)?, |s| match s.find(&pat) {
                Some(k) => {
                    let at = if name == "insertAfter" { k + pat.len() } else { k };
                    format!("{}{ins}{}", &s[..at], &s[at..])
                }
                None => s.to_string(),
            })?)
        }
        "extractBefore" | "extractAfter" => {
            let p = arg(a, 1, name)?;
            let pos = if p.is_text() { None } else { Some(p.as_usize()?) };
            let pat = if p.is_text() { p.text()? } else { String::new() };
            one(map_text(arg(a, 0, name)?, |s| {
                let chars: Vec<char> = s.chars().collect();
                let (before, after) = match pos {
                    Some(k) => (chars[..(k.saturating_sub(1)).min(chars.len())].iter().collect::<String>(), chars[k.min(chars.len())..].iter().collect::<String>()),
                    None => match s.find(&pat) {
                        Some(i) => (s[..i].to_string(), s[i + pat.len()..].to_string()),
                        None => (String::new(), String::new()),
                    },
                };
                if name == "extractBefore" { before } else { after }
            })?)
        }
        // Numeric start and end positions, inclusive; char input gives a cell.
        "extractBetween" if matches!(a.get(1), Some(v) if !v.is_text() && !matches!(v, Value::Str(_) | Value::Cell(_))) => {
            let (i, j) = (arg(a, 1, name)?.as_usize()?, arg(a, 2, name)?.as_usize()?);
            let v = arg(a, 0, name)?;
            let out = map_text(v, |s| {
                let chars: Vec<char> = s.chars().collect();
                let (lo, hi) = (i.max(1) - 1, j.min(chars.len()));
                if lo < hi { chars[lo..hi].iter().collect() } else { String::new() }
            })?;
            one(if matches!(v, Value::Num(_)) { Value::Cell(CellArray::new(1, 1, vec![out])) } else { out })
        }
        "extractBetween" => {
            let p1 = arg(a, 1, name)?.text()?;
            let p2 = arg(a, 2, name)?.text()?;
            one(map_text(arg(a, 0, name)?, |s| match s.find(&p1) {
                Some(i) => {
                    let rest = &s[i + p1.len()..];
                    match rest.find(&p2) {
                        Some(j) => rest[..j].to_string(),
                        None => String::new(),
                    }
                }
                None => String::new(),
            })?)
        }
        "strcat" => {
            if is_string_input(a) || a.iter().any(|v| matches!(v, Value::Cell(_))) {
                // Elementwise with implicit expansion; strings keep whitespace.
                let mut acc = to_str_array(arg(a, 0, name)?)?;
                for v in &a[1..] {
                    let r = crate::ops::binary(crate::ast::BinOp::Add, &Value::Str(acc), &Value::Str(to_str_array(v)?))?;
                    let Value::Str(s) = r else { unreachable!() };
                    acc = s;
                }
                if a.iter().any(|v| matches!(v, Value::Cell(_))) && !is_string_input(a) {
                    return one(Value::Cell(CellArray::from_dims(acc.dims.clone(), acc.data.iter().map(|x| Value::chars(x.as_deref().unwrap_or(""))).collect())));
                }
                return one(Value::Str(acc));
            }
            let mut s = String::new();
            for v in a {
                // strcat removes trailing whitespace of char inputs.
                s.push_str(v.text()?.trim_end());
            }
            one(Value::chars(&s))
        }
        "append" => {
            let mut acc = arg(a, 0, name)?.clone();
            for v in &a[1..] {
                acc = if matches!(acc, Value::Str(_)) || matches!(v, Value::Str(_)) {
                    crate::ops::binary(crate::ast::BinOp::Add, &Value::Str(to_str_array(&acc)?), &Value::Str(to_str_array(v)?))?
                } else {
                    Value::chars(&(acc.text()? + &v.text()?))
                };
            }
            one(acc)
        }
        "strcmp" | "strcmpi" | "strncmp" | "strncmpi" => {
            let x = arg(a, 0, name)?;
            let y = arg(a, 1, name)?;
            let n = if name.starts_with("strn") { Some(arg(a, 2, name)?.as_usize()?) } else { None };
            let ci = name.ends_with('i');
            let eq = |p: &str, q: &str| -> bool {
                let (p, q) = match n {
                    Some(k) => (p.chars().take(k).collect::<String>(), q.chars().take(k).collect::<String>()),
                    None => (p.to_string(), q.to_string()),
                };
                if n.is_some() && (p.chars().count() < n.unwrap() || q.chars().count() < n.unwrap()) {
                    return false;
                }
                if ci { p.to_lowercase() == q.to_lowercase() } else { p == q }
            };
            let textual = |v: &Value| v.is_text() || matches!(v, Value::Str(_) | Value::Cell(_)) || v.is_char();
            if !textual(x) || !textual(y) {
                return one(Value::bool(false));
            }
            let multi = |v: &Value| matches!(v, Value::Cell(_)) || matches!(v, Value::Str(s) if !s.is_scalar());
            if !multi(x) && !multi(y) {
                let (p, q) = (text_list(x)?, text_list(y)?);
                return one(Value::bool(p.len() == 1 && q.len() == 1 && eq(&p[0], &q[0])));
            }
            let (big, other) = if multi(x) { (x, y) } else { (y, x) };
            let items = text_list(big)?;
            let d = big.dims();
            if multi(other) {
                let o = text_list(other)?;
                if o.len() != items.len() {
                    return rt("Inputs must be the same size or either one can be a scalar.");
                }
                return one(Value::Num(Matrix::from_dims(d, items.iter().zip(&o).map(|(p, q)| eq(p, q) as u8 as f64).collect()).with_kind(Kind::Logical)));
            }
            let s = text_list(other)?.into_iter().next().unwrap_or_default();
            one(Value::Num(Matrix::from_dims(d, items.iter().map(|p| eq(p, &s) as u8 as f64).collect()).with_kind(Kind::Logical)))
        }
        "strsplit" | "split" => {
            let v = arg(a, 0, name)?;
            let has_delims = a.get(1).is_some_and(|d| !(name == "strsplit" && d.text().is_ok_and(|t| t.eq_ignore_ascii_case("CollapseDelimiters")) && a.len() == 3));
            let mut delims: Vec<String> = match a.get(1) {
                Some(d) if has_delims => text_list(d)?.into_iter().map(|t| process_escapes(&t)).collect(),
                _ => vec![" ".into(), "\x0c".into(), "\n".into(), "\r".into(), "\t".into(), "\x0b".into()],
            };
            // Longer delimiters are tried first, so "ab" wins over "a".
            delims.sort_by_key(|d| std::cmp::Reverse(d.chars().count()));
            let mut collapse = name == "strsplit";
            let opts = if has_delims { 2 } else { 1 };
            let mut k = opts;
            while k + 1 < a.len() {
                if a[k].text()?.eq_ignore_ascii_case("CollapseDelimiters") {
                    collapse = a[k + 1].is_true()?;
                }
                k += 2;
            }
            if name == "split" && !has_delims {
                collapse = true;
            }
            let split_one = |s: &str| -> (Vec<String>, Vec<String>) {
                let mut out = Vec::new();
                let mut found: Vec<String> = Vec::new();
                let mut cur = String::new();
                let mut rest = s;
                let mut after_delim = false;
                'outer: while !rest.is_empty() {
                    for d in &delims {
                        if !d.is_empty() && rest.starts_with(d.as_str()) {
                            if collapse && after_delim {
                                found.last_mut().unwrap().push_str(d);
                            } else {
                                out.push(std::mem::take(&mut cur));
                                found.push(d.clone());
                            }
                            after_delim = true;
                            rest = &rest[d.len()..];
                            continue 'outer;
                        }
                    }
                    after_delim = false;
                    let c = rest.chars().next().unwrap();
                    cur.push(c);
                    rest = &rest[c.len_utf8()..];
                }
                out.push(cur);
                (out, found)
            };
            let s = text_list(v)?.into_iter().next().unwrap_or_default();
            let (parts, found) = split_one(&s);
            if name == "split" {
                // split returns a column string array (or cell for char input).
                return one(if matches!(v, Value::Str(_)) { Value::Str(StrArray::col(parts)) } else { Value::Cell(CellArray::col(parts.iter().map(|p| Value::chars(p)).collect())) });
            }
            let is_str = matches!(v, Value::Str(_));
            let row = |items: Vec<String>| {
                if is_str {
                    let n = items.len();
                    Value::Str(StrArray::from_dims(vec![1, n], items.into_iter().map(Some).collect()))
                } else if items.is_empty() {
                    Value::Cell(CellArray::new(1, 0, vec![]))
                } else {
                    Value::Cell(CellArray::row(items.iter().map(|p| Value::chars(p)).collect()))
                }
            };
            Ok(Some(vec![row(parts), row(found)]))
        }
        "join" if matches!(a.first(), Some(Value::Table(_))) => crate::table_ops::join_tables(a),
        "strjoin" | "join" => {
            let v = arg(a, 0, name)?;
            let items = text_list(v)?;
            let d = match a.get(1) {
                Some(d) => process_escapes(&d.text()?),
                None => " ".into(),
            };
            let joined = items.join(&d);
            one(if matches!(v, Value::Str(_)) || name == "join" && !matches!(v, Value::Cell(_)) { Value::string(joined) } else if name == "join" { Value::chars(&joined) } else { Value::chars(&joined) })
        }
        "contains" | "startsWith" | "endsWith" => {
            let pats = pattern_list(arg(a, 1, name)?)?;
            let ci = a.iter().skip(2).any(|v| v.text().map(|t| t == "IgnoreCase").unwrap_or(false)) && a.last().map(|v| v.is_true().unwrap_or(false)).unwrap_or(false);
            one(test_text(arg(a, 0, name)?, |s| {
                let s = if ci { s.to_lowercase() } else { s.to_string() };
                pats.iter().any(|p| {
                    let p = if ci { p.to_lowercase() } else { p.clone() };
                    match name {
                        "contains" => s.contains(p.as_str()),
                        "startsWith" => s.starts_with(p.as_str()),
                        _ => s.ends_with(p.as_str()),
                    }
                })
            })?)
        }
        "count" => {
            let pats = pattern_list(arg(a, 1, name)?)?;
            let v = arg(a, 0, name)?;
            let f = |s: &str| pats.iter().map(|p| if p.is_empty() { 0 } else { s.matches(p.as_str()).count() }).sum::<usize>() as f64;
            Ok(Some(vec![match texts(v)? {
                Texts::Char(s) => Value::num(f(&s)),
                Texts::Strs(s) => Value::Num(Matrix::from_dims(s.dims.clone(), s.data.iter().map(|x| x.as_deref().map(f).unwrap_or(f64::NAN)).collect())),
                Texts::Cells(d, items) => Value::Num(Matrix::from_dims(d, items.iter().map(|t| f(t)).collect())),
            }]))
        }
        "strfind" => {
            let p = arg(a, 1, name)?.text()?;
            let pc: Vec<char> = p.chars().collect();
            let force_cell = a.len() >= 4 && a[2].text().is_ok_and(|t| t.eq_ignore_ascii_case("ForceCellOutput")) && a[3].is_true()?;
            let find = |s: &str| {
                let chars: Vec<char> = s.chars().collect();
                let mut hits = Vec::new();
                if !pc.is_empty() && pc.len() <= chars.len() {
                    for i in 0..=chars.len() - pc.len() {
                        if chars[i..i + pc.len()] == pc[..] {
                            hits.push((i + 1) as f64);
                        }
                    }
                }
                Value::Num(if hits.is_empty() { Matrix::new(1, 0, vec![]) } else { Matrix::row(hits) })
            };
            match arg(a, 0, name)? {
                Value::Cell(c) => {
                    let items = c.data.iter().map(|v| v.text().map(|t| find(&t))).collect::<R<Vec<_>>>()?;
                    one(Value::Cell(CellArray::from_dims(c.dims.clone(), items)))
                }
                Value::Str(sa) if sa.data.len() != 1 => {
                    let items = sa.data.iter().map(|t| find(t.as_deref().unwrap_or(""))).collect();
                    one(Value::Cell(CellArray::from_dims(sa.dims.clone(), items)))
                }
                v => {
                    let r = find(&v.text()?);
                    one(if force_cell { Value::Cell(CellArray::new(1, 1, vec![r])) } else { r })
                }
            }
        }
        "ismissing" => {
            let v = arg(a, 0, name)?;
            one(match v {
                Value::Str(s) => Value::Num(Matrix::from_dims(s.dims.clone(), s.data.iter().map(|x| x.is_none() as u8 as f64).collect()).with_kind(Kind::Logical)),
                Value::Num(m) => Value::Num(Matrix::from_dims(m.dims.clone(), m.data.iter().map(|x| x.is_nan() as u8 as f64).collect()).with_kind(Kind::Logical)),
                Value::Time(t) => Value::Num(Matrix::from_dims(t.dims.clone(), t.data.iter().map(|p| p[0].is_nan() as u8 as f64).collect()).with_kind(Kind::Logical)),
                Value::Cell(c) => Value::Num(Matrix::from_dims(c.dims.clone(), c.data.iter().map(|x| x.text().map(|t| t.is_empty()).unwrap_or(false) as u8 as f64).collect()).with_kind(Kind::Logical)),
                other => Value::Num(Matrix::filled_dims(other.dims(), 0.0).with_kind(Kind::Logical)),
            })
        }
        "isspace" | "isletter" | "isstrprop" => {
            let v0 = arg(a, 0, name)?;
            if name != "isletter" {
                let mut sub = |t: &str| -> R<Value> {
                    let mut b = a.to_vec();
                    b[0] = Value::chars(t);
                    Ok(call(it, name, &b, 1)?.and_then(|v| v.into_iter().next()).unwrap_or(Value::empty()))
                };
                match v0 {
                    Value::Str(sa) if sa.data.len() == 1 => return one(sub(sa.data[0].as_deref().unwrap_or(""))?),
                    Value::Cell(c) if name == "isstrprop" => {
                        let items = c.data.iter().map(|v| sub(&v.text()?)).collect::<R<Vec<_>>>()?;
                        return one(Value::Cell(CellArray::from_dims(c.dims.clone(), items)));
                    }
                    _ => {}
                }
            }
            let m = m_arg(a, 0, name)?;
            let prop = if name == "isstrprop" { arg(a, 1, name)?.text()? } else { name.trim_start_matches("is").to_string() };
            let f = |c: char| match prop.as_str() {
                "space" | "wspace" => c.is_whitespace(),
                "letter" | "alpha" => c.is_alphabetic(),
                "digit" => c.is_ascii_digit(),
                "alphanum" => c.is_alphanumeric(),
                "upper" => c.is_uppercase(),
                "lower" => c.is_lowercase(),
                "punct" => c.is_ascii_punctuation(),
                _ => false,
            };
            one(Value::Num(Matrix::from_dims(m.dims.clone(), m.data.iter().map(|&x| char::from_u32(x as u32).map(f).unwrap_or(false) as u8 as f64).collect()).with_kind(Kind::Logical)))
        }
        "ischar" => one(Value::bool(arg(a, 0, name)?.is_char())),
        "iscellstr" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Cell(c) if c.data.iter().all(|v| v.is_char())))),
        "compose" => {
            let fmt = arg(a, 0, name)?.text()?;
            let n = a[1..].iter().map(|v| v.numel()).max().unwrap_or(1).max(1);
            let mut out = Vec::with_capacity(n);
            for k in 0..n {
                let row: Vec<Value> = a[1..].iter().map(|v| crate::indexing::index(v, &[crate::nd::IndexArg::scalar(if v.numel() == 1 { 0 } else { k })])).collect::<R<_>>()?;
                out.push(sprintf(&fmt, &row)?);
            }
            one(if out.len() == 1 { Value::string(out.remove(0)) } else { Value::Str(StrArray::col(out)) })
        }
        "newline" => one(Value::chars("\n")),
        "blanks" => one(Value::chars(&" ".repeat(arg(a, 0, name)?.as_usize()?))),
        "strjust" => strjust(arg(a, 0, name)?, a.get(1)),
        _ => {
            let _ = nargout;
            Ok(None)
        }
    }
}

/// `strjust(s, side)`: moves each row's (or string's) text to the right (default), left or
/// center of its width; centering puts the smaller half of the spaces on the left.
fn strjust(v: &Value, side: Option<&Value>) -> Ret {
    let side = match side {
        Some(s) => s.text()?.to_lowercase(),
        None => "right".into(),
    };
    if !matches!(side.as_str(), "left" | "right" | "center") {
        return rt("Justification must be 'left', 'right', or 'center'.");
    }
    let justify = |row: &str| -> String {
        let chars: Vec<char> = row.chars().collect();
        let w = chars.len();
        let start = chars.iter().position(|&c| c != ' ').unwrap_or(w);
        let end = chars.iter().rposition(|&c| c != ' ').map(|p| p + 1).unwrap_or(start);
        let text: String = chars[start..end].iter().collect();
        let pad = w - (end - start);
        let left = match side.as_str() {
            "left" => 0,
            "right" => pad,
            _ => pad / 2,
        };
        format!("{}{}{}", " ".repeat(left), text, " ".repeat(pad - left))
    };
    one(match v {
        Value::Num(m) if m.kind == Kind::Char => {
            let (r, c) = (m.rows(), m.cols());
            let mut data = m.data.clone();
            for i in 0..r {
                let row: String = (0..c).map(|j| char::from_u32(m.data[j * r + i] as u32).unwrap_or(' ')).collect();
                for (j, ch) in justify(&row).chars().enumerate() {
                    data[j * r + i] = ch as u32 as f64;
                }
            }
            Value::Num(Matrix::from_dims(m.dims.clone(), data).with_kind(Kind::Char))
        }
        Value::Str(s) => Value::Str(StrArray::from_dims(s.dims.clone(), s.data.iter().map(|x| x.as_deref().map(justify)).collect())),
        Value::Cell(c) => Value::Cell(CellArray::from_dims(c.dims.clone(), c.data.iter().map(|x| x.text().map(|t| Value::chars(&justify(&t)))).collect::<R<_>>()?)),
        other => return rt(format!("Input must be a character array, string array or cell array of character vectors, not '{}'.", other.class_name())),
    })
}
