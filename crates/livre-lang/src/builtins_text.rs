//! Text functions over char vectors, string arrays and cell arrays of char vectors. Results
//! keep the input's container type, as MATLAB does.

use crate::builtins::{arg, m_arg, none, num2str_scalar, one, Ret};
use crate::display::fmt_g;
use crate::error::{rt, LangError};
use crate::format::{process_escapes, sprintf};
use crate::interp::{Interpreter, OutputKind};
use crate::nd::{self, Dims};
use crate::ops::to_str_array;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "sprintf", "fprintf", "num2str", "int2str", "mat2str", "str2double", "str2num", "num2cell", "string", "strings", "char", "cellstr", "strlength",
    "upper", "lower", "strtrim", "strip", "deblank", "strrep", "replace", "strcat", "append", "strcmp", "strcmpi", "strncmp", "strncmpi", "strsplit",
    "split", "strjoin", "join", "contains", "startsWith", "endsWith", "strfind", "extractBefore", "extractAfter", "extractBetween", "ismissing",
    "isspace", "isletter", "isstrprop", "reverse", "pad", "compose", "ischar", "iscellstr", "count", "erase", "insertAfter", "insertBefore", "newline",
    "blanks", "lettersPattern", "strjust", "convertStringsToChars", "convertCharsToStrings", "missing",
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
            let mut rest: &[Value] = a;
            if let Some(Value::Num(m)) = rest.first() {
                if m.kind != Kind::Char && m.is_scalar() && rest.len() >= 2 {
                    rest = &rest[1..];
                }
            }
            let f = arg(rest, 0, name)?.text()?;
            let s = sprintf(&f, &rest[1..])?;
            it.emit(OutputKind::Text(s));
            none()
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
            let parse = |s: &str| -> f64 {
                let t = s.trim().replace(',', "");
                if let Ok(v) = t.parse::<f64>() {
                    return v;
                }
                match t.to_lowercase().as_str() {
                    "inf" | "+inf" => f64::INFINITY,
                    "-inf" => f64::NEG_INFINITY,
                    _ => f64::NAN,
                }
            };
            match arg(a, 0, name)? {
                Value::Cell(c) => one(Value::Num(Matrix::from_dims(c.dims.clone(), c.data.iter().map(|v| v.text().map(|s| parse(&s)).unwrap_or(f64::NAN)).collect()))),
                Value::Str(s) => one(Value::Num(Matrix::from_dims(s.dims.clone(), s.data.iter().map(|x| x.as_deref().map(parse).unwrap_or(f64::NAN)).collect()))),
                v if v.is_text() => one(Value::num(parse(&v.text()?))),
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
            let delims: Vec<String> = match a.get(1) {
                Some(d) => text_list(d)?.into_iter().map(|t| process_escapes(&t)).collect(),
                None => vec![" ".into(), "\t".into(), "\n".into()],
            };
            let split_one = |s: &str| -> Vec<String> {
                let mut out = Vec::new();
                let mut cur = String::new();
                let mut rest = s;
                'outer: while !rest.is_empty() {
                    for d in &delims {
                        if !d.is_empty() && rest.starts_with(d.as_str()) {
                            out.push(std::mem::take(&mut cur));
                            rest = &rest[d.len()..];
                            continue 'outer;
                        }
                    }
                    let c = rest.chars().next().unwrap();
                    cur.push(c);
                    rest = &rest[c.len_utf8()..];
                }
                out.push(cur);
                if a.get(1).is_none() {
                    // Default whitespace splitting collapses runs (strsplit behavior).
                    out.retain(|x| !x.is_empty());
                }
                out
            };
            let s = text_list(v)?.into_iter().next().unwrap_or_default();
            let parts = split_one(&s);
            if name == "split" {
                // split returns a column string array (or cell for char input).
                return one(if matches!(v, Value::Str(_)) { Value::Str(StrArray::col(parts)) } else { Value::Cell(CellArray::col(parts.iter().map(|p| Value::chars(p)).collect())) });
            }
            one(Value::Cell(CellArray::row(parts.iter().map(|p| Value::chars(p)).collect())))
        }
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
            let s = arg(a, 0, name)?.text()?;
            let p = arg(a, 1, name)?.text()?;
            let chars: Vec<char> = s.chars().collect();
            let pc: Vec<char> = p.chars().collect();
            let mut hits = Vec::new();
            if !pc.is_empty() && pc.len() <= chars.len() {
                for i in 0..=chars.len() - pc.len() {
                    if chars[i..i + pc.len()] == pc[..] {
                        hits.push((i + 1) as f64);
                    }
                }
            }
            one(Value::Num(if hits.is_empty() { Matrix::new(1, 0, vec![]) } else { Matrix::row(hits) }))
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
        "strjust" | "lettersPattern" => rt(format!("'{name}' is not supported in this version.")),
        _ => {
            let _ = nargout;
            Ok(None)
        }
    }
}
