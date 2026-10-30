//! Delimited text files: `readmatrix`, `writematrix`, `readcell`, `writecell`, `readvars`,
//! `csvread`, `csvwrite`, `dlmread`, `dlmwrite`, `importdata`, and `textscan`/`textread`.
//!
//! The delimiter is detected (comma, tab, semicolon, bar or spaces) unless given, and leading
//! lines without numbers are taken as headers, as MATLAB's import detection does for simple
//! files. Numbers are written with up to 15 significant digits.

use crate::builtins::{arg, none, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "readmatrix", "writematrix", "readcell", "writecell", "readvars", "csvread", "csvwrite", "dlmread", "dlmwrite", "importdata", "textscan", "textread",
];

fn read_file(path: &str) -> R<String> {
    let bytes = std::fs::read(path).map_err(|_| LangError::new(format!("Unable to find or open '{path}'. Check the path and filename or file permissions."), 0))?;
    Ok(String::from_utf8_lossy(&bytes).replace("\r\n", "\n"))
}

/// Name-value options (case-insensitive names) after `start`.
fn option(a: &[Value], start: usize, name: &str) -> Option<Value> {
    let mut i = start;
    while i + 1 < a.len() {
        if a[i].text().map(|t| t.eq_ignore_ascii_case(name)).unwrap_or(false) {
            return Some(a[i + 1].clone());
        }
        i += 2;
    }
    None
}

/// '\t' and similar escape names for a delimiter.
fn delimiter_text(s: &str) -> String {
    match s {
        "\\t" | "tab" => "\t".into(),
        "comma" => ",".into(),
        "space" => " ".into(),
        "semi" => ";".into(),
        "bar" => "|".into(),
        other => other.to_string(),
    }
}

fn detect_delimiter(lines: &[&str]) -> String {
    for d in [",", "\t", ";", "|"] {
        if lines.iter().take(10).any(|l| l.contains(d)) {
            return d.into();
        }
    }
    " ".into()
}

/// Fields of one line; quoted fields keep embedded delimiters. Space-delimited lines treat
/// runs of spaces as one delimiter.
fn split_line(line: &str, delim: &str) -> Vec<String> {
    if delim == " " {
        return line.split_whitespace().map(str::to_string).collect();
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let d: Vec<char> = delim.chars().collect();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            if quoted && chars.get(i + 1) == Some(&'"') {
                cur.push('"');
                i += 2;
                continue;
            }
            quoted = !quoted;
        } else if !quoted && d.contains(&c) {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
        i += 1;
    }
    out.push(cur.trim().to_string());
    out
}

fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    match t.to_lowercase().as_str() {
        "nan" => Some(f64::NAN),
        "inf" | "+inf" => Some(f64::INFINITY),
        "-inf" => Some(f64::NEG_INFINITY),
        _ => t.parse::<f64>().ok(),
    }
}

struct Grid {
    rows: Vec<Vec<String>>,
    headers: usize,
}

fn grid(text: &str, a: &[Value], start: usize) -> R<Grid> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let delim = match option(a, start, "Delimiter") {
        Some(v) => delimiter_text(&v.text()?),
        None => detect_delimiter(&lines),
    };
    let rows: Vec<Vec<String>> = lines.iter().map(|l| split_line(l, &delim)).collect();
    let headers = match option(a, start, "NumHeaderLines").or_else(|| option(a, start, "HeaderLines")) {
        Some(v) => v.scalar()? as usize,
        None => rows.iter().take_while(|r| r.iter().all(|f| parse_number(f).is_none())).count().min(rows.len().saturating_sub(1)),
    };
    Ok(Grid { rows, headers })
}

fn matrix_of(rows: &[Vec<String>]) -> Matrix {
    let r = rows.len();
    let c = rows.iter().map(|x| x.len()).max().unwrap_or(0);
    let mut data = vec![f64::NAN; r * c];
    for (i, row) in rows.iter().enumerate() {
        for (j, f) in row.iter().enumerate() {
            data[j * r + i] = parse_number(f).unwrap_or(f64::NAN);
        }
    }
    Matrix::new(r, c, data)
}

/// A number as MATLAB writes it to text: up to 15 significant digits.
fn num_text(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf".into() } else { "-Inf".into() };
    }
    crate::display::fmt_g(x, 15)
}

fn quote_if_needed(s: &str, delim: &str) -> String {
    if s.contains(delim) || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn write_rows(path: &str, rows: &[Vec<String>], delim: &str, append: bool) -> R<()> {
    let body: String = rows.iter().map(|r| format!("{}\n", r.join(delim))).collect();
    if append {
        use std::io::Write;
        std::fs::OpenOptions::new().create(true).append(true).open(path).and_then(|mut f| f.write_all(body.as_bytes()))
    } else {
        std::fs::write(path, body)
    }
    .map_err(|e| LangError::new(format!("Unable to write '{path}': {e}"), 0))
}

fn default_delim(path: &str, a: &[Value], start: usize) -> R<String> {
    Ok(match option(a, start, "Delimiter") {
        Some(v) => delimiter_text(&v.text()?),
        None => if path.to_lowercase().ends_with(".csv") { ",".into() } else if path.to_lowercase().ends_with(".txt") || path.to_lowercase().ends_with(".dat") { ",".into() } else { ",".into() },
    })
}

fn rows_of_matrix(m: &Matrix) -> Vec<Vec<String>> {
    (0..m.rows()).map(|i| (0..m.cols()).map(|j| num_text(m.get(i, j))).collect()).collect()
}

fn cell_value_text(v: &Value, delim: &str) -> R<String> {
    Ok(match v {
        Value::Num(m) if m.kind == Kind::Char => quote_if_needed(&m.to_string_lossy(), delim),
        Value::Num(m) if m.numel() == 1 => num_text(m.data[0]),
        Value::Num(m) if m.is_empty() => String::new(),
        Value::Str(s) if s.data.len() == 1 => s.data[0].as_deref().map(|t| quote_if_needed(t, delim)).unwrap_or_default(),
        other => quote_if_needed(&other.text()?, delim),
    })
}

/// textscan on text: one cell per format field.
fn textscan(text: &str, fmt: &str, repeat: Option<usize>, a: &[Value], start: usize) -> R<(Vec<Value>, usize)> {
    let delims: Vec<char> = match option(a, start, "Delimiter") {
        Some(v) => delimiter_text(&v.text()?).chars().collect(),
        None => Vec::new(),
    };
    let header = option(a, start, "HeaderLines").map(|v| v.scalar()).transpose()?.unwrap_or(0.0) as usize;
    let comment = option(a, start, "CommentStyle").map(|v| v.text()).transpose()?;
    let mut body: String = text.split_inclusive('\n').skip(header).collect();
    if let Some(c) = &comment {
        body = body.lines().filter(|l| !l.trim_start().starts_with(c.as_str())).map(|l| format!("{l}\n")).collect();
    }
    // Format items: (kind, skip, width); literals are matched between fields.
    #[derive(Clone)]
    enum Item {
        Field { conv: String, skip: bool },
        Lit(String),
    }
    let mut items = Vec::new();
    let f: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < f.len() {
        if f[i] == '%' {
            let mut j = i + 1;
            let skip = f.get(j) == Some(&'*');
            if skip {
                j += 1;
            }
            while j < f.len() && f[j].is_ascii_digit() {
                j += 1;
            }
            let start_conv = j;
            while j < f.len() && f[j].is_ascii_alphabetic() {
                j += 1;
                // %d8, %u16, %f32 carry a size.
                while j < f.len() && f[j].is_ascii_digit() {
                    j += 1;
                }
                if f[start_conv] != 'd' && f[start_conv] != 'u' && f[start_conv] != 'f' {
                    break;
                }
                break;
            }
            items.push(Item::Field { conv: f[start_conv..j].iter().collect(), skip });
            i = j;
        } else if f[i].is_whitespace() {
            i += 1;
        } else {
            let mut lit = String::new();
            while i < f.len() && f[i] != '%' && !f[i].is_whitespace() {
                lit.push(f[i]);
                i += 1;
            }
            let lit = lit.replace("\\n", "\n").replace("\\t", "\t");
            items.push(Item::Lit(lit));
        }
    }
    let fields: Vec<String> = items.iter().filter_map(|it| match it { Item::Field { conv, skip: false } => Some(conv.clone()), _ => None }).collect();
    let mut cols: Vec<Vec<Value>> = vec![Vec::new(); fields.len()];
    let chars: Vec<char> = body.chars().collect();
    let mut pos = 0;
    let is_delim = |c: char| delims.contains(&c);
    let skip_space = |pos: &mut usize| while *pos < chars.len() && (chars[*pos].is_whitespace() || is_delim(chars[*pos])) && chars[*pos] != '\0' { *pos += 1 };
    let mut rounds = 0;
    'outer: while pos < chars.len() && repeat.is_none_or(|n| rounds < n) {
        let round_start = pos;
        let mut fi = 0;
        for item in &items {
            match item {
                Item::Lit(l) => {
                    while pos < chars.len() && chars[pos] == ' ' {
                        pos += 1;
                    }
                    let lc: Vec<char> = l.chars().collect();
                    if chars.get(pos..pos + lc.len()) == Some(&lc[..]) {
                        pos += lc.len();
                    } else {
                        break 'outer;
                    }
                }
                Item::Field { conv, skip } => {
                    // Fields are separated by delimiters or whitespace.
                    while pos < chars.len() && (chars[pos].is_whitespace() || (is_delim(chars[pos]) && fi > 0 && false)) {
                        pos += 1;
                    }
                    let st = pos;
                    let numeric = conv.starts_with('f') || conv.starts_with('d') || conv.starts_with('u') || conv.starts_with('n');
                    if numeric {
                        while pos < chars.len() && (chars[pos].is_ascii_digit() || matches!(chars[pos], '.' | '-' | '+' | 'e' | 'E' | 'I' | 'n' | 'f' | 'N' | 'a')) && !is_delim(chars[pos]) {
                            pos += 1;
                        }
                    } else if conv.starts_with('q') && chars.get(pos) == Some(&'"') {
                        pos += 1;
                        while pos < chars.len() && chars[pos] != '"' {
                            pos += 1;
                        }
                        pos = (pos + 1).min(chars.len());
                    } else {
                        while pos < chars.len() && !chars[pos].is_whitespace() && !is_delim(chars[pos]) {
                            pos += 1;
                        }
                    }
                    let tok: String = chars[st..pos].iter().collect();
                    let v = if numeric {
                        match parse_number(&tok) {
                            Some(x) => Value::num(x),
                            None if tok.is_empty() && pos < chars.len() && is_delim(chars[pos]) => Value::num(f64::NAN),
                            None => {
                                pos = st;
                                break 'outer;
                            }
                        }
                    } else {
                        if tok.is_empty() && pos >= chars.len() {
                            break 'outer;
                        }
                        Value::chars(tok.trim_matches('"'))
                    };
                    // Step over one delimiter after the field.
                    while pos < chars.len() && chars[pos] == ' ' {
                        pos += 1;
                    }
                    if pos < chars.len() && is_delim(chars[pos]) {
                        pos += 1;
                    }
                    if !skip {
                        cols[fi].push(v);
                        fi += 1;
                    }
                }
            }
        }
        rounds += 1;
        skip_space(&mut pos);
        if pos == round_start {
            break;
        }
    }
    let out = fields
        .iter()
        .zip(cols)
        .map(|(conv, vals)| {
            let n = vals.len();
            if conv.starts_with('s') || conv.starts_with('q') || conv.starts_with('c') {
                Value::Cell(CellArray::from_dims(vec![n, 1], vals))
            } else {
                let data: Vec<f64> = vals.iter().map(|v| v.scalar().unwrap_or(f64::NAN)).collect();
                let kind = match conv.as_str() {
                    "d" | "d32" => Kind::Int(IntClass::I32),
                    "d8" => Kind::Int(IntClass::I8),
                    "d16" => Kind::Int(IntClass::I16),
                    "d64" => Kind::Int(IntClass::I64),
                    "u" | "u32" => Kind::Int(IntClass::U32),
                    "u8" => Kind::Int(IntClass::U8),
                    "u16" => Kind::Int(IntClass::U16),
                    "u64" => Kind::Int(IntClass::U64),
                    "f32" => Kind::Single,
                    _ => Kind::Double,
                };
                Value::Num(Matrix::new(n, 1, data).cast_to(kind))
            }
        })
        .collect();
    let consumed: usize = chars[..pos].iter().collect::<String>().len();
    let header_bytes: usize = text.split_inclusive('\n').take(header).map(str::len).sum();
    Ok((out, header_bytes + consumed))
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match name {
        "readmatrix" => {
            let path = arg(a, 0, name)?.text()?;
            let g = grid(&read_file(&path)?, a, 1)?;
            one(Value::Num(matrix_of(&g.rows[g.headers..])))
        }
        "readcell" => {
            let path = arg(a, 0, name)?.text()?;
            let g = grid(&read_file(&path)?, a, 1)?;
            let r = g.rows.len();
            let c = g.rows.iter().map(|x| x.len()).max().unwrap_or(0);
            let mut data = vec![Value::Str(StrArray::from_dims(vec![1, 1], vec![None])); r * c];
            for (i, row) in g.rows.iter().enumerate() {
                for (j, f) in row.iter().enumerate() {
                    data[j * r + i] = match parse_number(f) {
                        Some(x) => Value::num(x),
                        // Empty fields are <missing>, as in MATLAB.
                        None if f.is_empty() => Value::Str(StrArray::from_dims(vec![1, 1], vec![None])),
                        None => Value::chars(f),
                    };
                }
            }
            one(Value::Cell(CellArray::from_dims(vec![r, c], data)))
        }
        "readvars" => {
            let path = arg(a, 0, name)?.text()?;
            let g = grid(&read_file(&path)?, a, 1)?;
            let m = matrix_of(&g.rows[g.headers..]);
            let r = m.rows();
            Ok(Some((0..m.cols().min(nargout.max(1))).map(|j| Value::Num(Matrix::col(m.data[j * r..(j + 1) * r].to_vec()))).collect()))
        }
        "writematrix" => {
            let m = arg(a, 0, name)?.to_matrix()?;
            let path = match a.get(1) {
                Some(p) if !p.text().map(|t| t.eq_ignore_ascii_case("Delimiter") || t.eq_ignore_ascii_case("WriteMode")).unwrap_or(false) => p.text()?,
                _ => "matrix.txt".into(),
            };
            let delim = default_delim(&path, a, 2)?;
            let append = option(a, 2, "WriteMode").map(|v| v.text().map(|t| t.eq_ignore_ascii_case("append"))).transpose()?.unwrap_or(false);
            write_rows(&path, &rows_of_matrix(&m), &delim, append)?;
            none()
        }
        "writecell" => {
            let Value::Cell(c) = arg(a, 0, name)? else { return rt("Input must be a cell array.") };
            let path = a.get(1).map(|v| v.text()).transpose()?.unwrap_or_else(|| "cell.txt".into());
            let delim = default_delim(&path, a, 2)?;
            let (r, k) = (c.rows(), c.cols());
            let rows = (0..r).map(|i| (0..k).map(|j| cell_value_text(&c.data[j * r + i], &delim)).collect::<R<Vec<_>>>()).collect::<R<Vec<_>>>()?;
            write_rows(&path, &rows, &delim, false)?;
            none()
        }
        "csvread" | "dlmread" => {
            let path = arg(a, 0, name)?.text()?;
            let text = read_file(&path)?;
            // csvread(f, r, c) / dlmread(f, delim, r, c): zero-based offsets.
            let (delim, off) = if name == "csvread" { (",".to_string(), 1) } else {
                match a.get(1) {
                    Some(d) if d.is_text() => (delimiter_text(&d.text()?), 2),
                    _ => (detect_delimiter(&text.lines().collect::<Vec<_>>()), 1),
                }
            };
            let r0 = a.get(off).map(|v| v.scalar()).transpose()?.unwrap_or(0.0) as usize;
            let c0 = a.get(off + 1).map(|v| v.scalar()).transpose()?.unwrap_or(0.0) as usize;
            let rows: Vec<Vec<String>> = text.lines().filter(|l| !l.trim().is_empty()).skip(r0).map(|l| split_line(l, &delim).into_iter().skip(c0).collect()).collect();
            // Empty fields read as 0 in csvread and dlmread.
            let mut m = matrix_of(&rows);
            for (i, row) in rows.iter().enumerate() {
                for j in 0..m.cols() {
                    if row.get(j).is_none_or(|f| f.is_empty()) {
                        m.set(i, j, 0.0);
                    }
                }
            }
            one(Value::Num(m))
        }
        "csvwrite" | "dlmwrite" => {
            let path = arg(a, 0, name)?.text()?;
            let m = arg(a, 1, name)?.to_matrix()?;
            let mut delim = ",".to_string();
            let mut append = false;
            let mut precision: Option<String> = None;
            let mut i = 2;
            while i < a.len() {
                let t = a[i].text().unwrap_or_default();
                match t.to_lowercase().as_str() {
                    "-append" => {
                        append = true;
                        i += 1;
                    }
                    "delimiter" => {
                        delim = delimiter_text(&a.get(i + 1).map(|v| v.text()).transpose()?.unwrap_or_default());
                        i += 2;
                    }
                    "precision" => {
                        precision = Some(match &a[i + 1] {
                            v if v.is_text() => v.text()?,
                            v => format!("%.{}g", v.scalar()? as usize),
                        });
                        i += 2;
                    }
                    "newline" | "roffset" | "coffset" => i += 2,
                    _ if name == "dlmwrite" && i == 2 => {
                        delim = delimiter_text(&t);
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
            let rows: Vec<Vec<String>> = match &precision {
                Some(p) => (0..m.rows()).map(|r| (0..m.cols()).map(|c| crate::format::sprintf(p, &[Value::num(m.get(r, c))])).collect::<R<Vec<_>>>()).collect::<R<_>>()?,
                // dlmwrite/csvwrite use 5 significant digits by default.
                None => (0..m.rows()).map(|r| (0..m.cols()).map(|c| crate::display::fmt_g(m.get(r, c), 5)).collect()).collect(),
            };
            write_rows(&path, &rows, &delim, append)?;
            none()
        }
        "importdata" => {
            let path = arg(a, 0, name)?.text()?;
            let g = grid(&read_file(&path)?, a, 1)?;
            let data = matrix_of(&g.rows[g.headers..]);
            if g.headers == 0 {
                return one(Value::Num(data));
            }
            let mut s = StructArray::scalar();
            s.set("data", Value::Num(data));
            let header_lines: Vec<Value> = g.rows[..g.headers].iter().map(|r| Value::chars(&r.join(","))).collect();
            s.set("textdata", Value::Cell(CellArray::col(header_lines)));
            s.set("colheaders", Value::Cell(CellArray::row(g.rows[g.headers - 1].iter().map(|f| Value::chars(f)).collect())));
            one(Value::Struct(s))
        }
        "textscan" => {
            let src = arg(a, 0, name)?;
            let fmt = arg(a, 1, name)?.text()?;
            let (repeat, opt_start) = match a.get(2) {
                Some(v) if !v.is_text() && !matches!(v, Value::Str(_)) => (Some(v.scalar()?).filter(|n| n.is_finite()).map(|n| n as usize), 3),
                _ => (None, 2),
            };
            if src.is_text() || matches!(src, Value::Str(_)) {
                let (cols, _) = textscan(&src.text()?, &fmt, repeat, a, opt_start)?;
                return one(Value::Cell(CellArray::row(cols)));
            }
            // From a file identifier: read from the current position and advance.
            let fid = src.scalar()? as i32;
            let mut args = vec![Value::num(fid as f64)];
            args.push(Value::Num(Matrix::scalar(f64::INFINITY)));
            args.push(Value::chars("*char"));
            let rest = crate::builtins_files::call(it, "fread", &args, 1)?.unwrap_or_default();
            let text = rest.first().map(|v| v.text()).transpose()?.unwrap_or_default();
            let (cols, used) = textscan(&text, &fmt, repeat, a, opt_start)?;
            // Put back what textscan did not consume.
            let back = text.len() - used.min(text.len());
            crate::builtins_files::call(it, "fseek", &[Value::num(fid as f64), Value::num(-(back as f64)), Value::chars("cof")], 1)?;
            one(Value::Cell(CellArray::row(cols)))
        }
        "textread" => {
            let path = arg(a, 0, name)?.text()?;
            let fmt = arg(a, 1, name)?.text()?;
            let (cols, _) = textscan(&read_file(&path)?, &fmt, None, a, 2)?;
            Ok(Some(cols.into_iter().take(nargout.max(1)).collect()))
        }
        _ => Ok(None),
    }
}
