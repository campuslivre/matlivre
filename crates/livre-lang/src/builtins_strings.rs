//! Regular expressions (`regexp`, `regexpi`, `regexprep`, `regexptranslate`), the remaining
//! character and string functions, and base conversions (`dec2bin`, `hex2dec`, ...).
//!
//! MATLAB patterns are close to PCRE; they are translated for `fancy_regex` (lookaround and
//! backreferences included): `.` matches newlines unless 'dotexceptnewline', `^`/`$` anchor
//! the whole text unless 'lineanchors', and `\<`/`\>` are word starts and ends.

use fancy_regex::{Regex, RegexBuilder};

use crate::builtins::{arg, none, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "regexp", "regexpi", "regexprep", "regexptranslate", "strtok", "splitlines", "sscanf", "strvcat", "strmatch", "findstr", "isStringScalar",
    "eraseBetween", "replaceBetween", "convertContainedStringsToChars", "str2mat", "dec2bin", "bin2dec", "dec2hex", "hex2dec", "dec2base",
    "base2dec", "num2hex", "hex2num", "native2unicode", "unicode2native", "matches", "extract", "strread",
];

// ---- text helpers ------------------------------------------------------------------------

/// Text input as (characters, was it a string scalar).
fn text_of(v: &Value) -> R<(String, bool)> {
    match v {
        Value::Str(s) if s.data.len() == 1 => Ok((s.data[0].clone().unwrap_or_default(), true)),
        other => Ok((other.text()?, false)),
    }
}

fn out_text(s: &str, as_string: bool) -> Value {
    if as_string { Value::string(s) } else { Value::chars(s) }
}

/// A 1×n row of texts, as char cells or a string row.
fn text_row(items: Vec<String>, as_string: bool) -> Value {
    if as_string {
        Value::Str(StrArray::from_dims(vec![1, items.len()], items.into_iter().map(Some).collect()))
    } else {
        Value::Cell(CellArray::row(items.into_iter().map(|s| Value::chars(&s)).collect()))
    }
}

fn empty_char() -> Value {
    Value::Num(Matrix::new(0, 0, Vec::new()).cast_to(Kind::Char))
}

// ---- regular expressions -----------------------------------------------------------------

#[derive(Clone)]
struct Opts {
    once: bool,
    ignore_case: bool,
    line_anchors: bool,
    dot_all: bool,
    free_spacing: bool,
    empty_match: bool,
    outputs: Vec<String>,
}

fn parse_opts(a: &[Value], ignore_case: bool) -> R<Opts> {
    let mut o = Opts { once: false, ignore_case, line_anchors: false, dot_all: true, free_spacing: false, empty_match: false, outputs: Vec::new() };
    for v in a {
        let s = v.text()?;
        match s.to_lowercase().as_str() {
            "once" => o.once = true,
            "ignorecase" => o.ignore_case = true,
            "matchcase" => o.ignore_case = false,
            "lineanchors" => o.line_anchors = true,
            "stringanchors" => o.line_anchors = false,
            "dotall" => o.dot_all = true,
            "dotexceptnewline" => o.dot_all = false,
            "freespacing" => o.free_spacing = true,
            "literalspacing" => o.free_spacing = false,
            "emptymatch" => o.empty_match = true,
            "noemptymatch" => o.empty_match = false,
            "warnings" | "noemptymatch_" => {}
            "match" | "tokens" | "names" | "start" | "end" | "split" | "tokenextents" => o.outputs.push(s.to_lowercase()),
            _ => return rt(format!("Invalid option for regexp: {s}.")),
        }
    }
    Ok(o)
}

/// MATLAB pattern syntax to fancy_regex syntax.
fn translate(pat: &str) -> String {
    let mut out = String::with_capacity(pat.len() + 8);
    let mut chars = pat.chars().peekable();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('<') if !in_class => out.push_str(r"\b(?=\w)"),
                Some('>') if !in_class => out.push_str(r"\b(?<=\w)"),
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push_str("\\\\"),
            },
            '[' if !in_class => {
                in_class = true;
                out.push(c);
                // A ] right after [ or [^ is literal.
                if chars.peek() == Some(&'^') {
                    out.push(chars.next().unwrap());
                }
                if chars.peek() == Some(&']') {
                    chars.next();
                    out.push_str("\\]");
                }
            }
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn compile(pat: &str, o: &Opts) -> R<Regex> {
    let mut flags = String::new();
    if o.ignore_case {
        flags.push('i');
    }
    if o.line_anchors {
        flags.push('m');
    }
    if o.dot_all {
        flags.push('s');
    }
    if o.free_spacing {
        flags.push('x');
    }
    let src = if flags.is_empty() { translate(pat) } else { format!("(?{flags}){}", translate(pat)) };
    RegexBuilder::new(&src).build().map_err(|e| LangError::new(format!("Invalid regular expression: {e}"), 0))
}

/// One match: character ranges (0-based, end exclusive) of the whole match and of each group.
struct Hit {
    range: (usize, usize),
    groups: Vec<Option<(usize, usize)>>,
}

fn find_all(re: &Regex, text: &str, o: &Opts) -> R<Vec<Hit>> {
    // Byte offsets to character indices.
    let mut char_at = vec![0usize; text.len() + 1];
    let mut k = 0;
    for (b, _) in text.char_indices() {
        char_at[b] = k;
        k += 1;
    }
    char_at[text.len()] = k;
    let mut hits = Vec::new();
    for caps in re.captures_iter(text) {
        let caps = caps.map_err(|e| LangError::new(format!("Regular expression failed: {e}"), 0))?;
        let m = caps.get(0).unwrap();
        if m.start() == m.end() && !o.empty_match {
            continue;
        }
        let groups = (1..caps.len()).map(|g| caps.get(g).map(|x| (char_at[x.start()], char_at[x.end()]))).collect();
        hits.push(Hit { range: (char_at[m.start()], char_at[m.end()]), groups });
        if o.once {
            break;
        }
    }
    Ok(hits)
}

fn slice(chars: &[char], (a, b): (usize, usize)) -> String {
    chars[a..b].iter().collect()
}

/// The outputs of regexp for one text.
fn regexp_one(text: &str, as_string: bool, pat: &str, o: &Opts) -> R<Vec<(String, Value)>> {
    let re = compile(pat, o)?;
    let chars: Vec<char> = text.chars().collect();
    let hits = find_all(&re, text, o)?;
    let names: Vec<String> = re.capture_names().flatten().map(str::to_string).collect();
    let group_names: Vec<Option<String>> = re.capture_names().skip(1).map(|n| n.map(str::to_string)).collect();
    let n = hits.len();
    let num_row = |v: Vec<f64>| if v.is_empty() { Value::Num(Matrix::new(0, 0, Vec::new())) } else { Value::Num(Matrix::row(v)) };
    let empty_cell = || Value::Cell(CellArray::from_dims(vec![0, 0], Vec::new()));
    let ngroups = group_names.len();
    let token_cell = |h: &Hit| -> Value {
        let toks: Vec<Value> = h.groups.iter().map(|g| out_text(&g.map(|r| slice(&chars, r)).unwrap_or_default(), as_string)).collect();
        if as_string {
            Value::Str(StrArray::from_dims(vec![1, toks.len()], toks.iter().map(|t| t.text().ok()).collect()))
        } else {
            Value::Cell(CellArray::from_dims(vec![1, toks.len()], toks))
        }
    };
    let extents = |h: &Hit| -> Value {
        let rows: Vec<(f64, f64)> = h.groups.iter().map(|g| g.map_or((h.range.0 as f64 + 1.0, h.range.0 as f64), |(a, b)| (a as f64 + 1.0, b as f64))).collect();
        if rows.is_empty() {
            return Value::Num(Matrix::new(0, 0, Vec::new()));
        }
        let mut data: Vec<f64> = rows.iter().map(|r| r.0).collect();
        data.extend(rows.iter().map(|r| r.1));
        Value::Num(Matrix::new(rows.len(), 2, data))
    };
    let names_struct = |hs: &[&Hit]| -> Value {
        let named: Vec<(usize, &String)> = group_names.iter().enumerate().filter_map(|(i, n)| n.as_ref().map(|n| (i, n))).collect();
        let mut s = StructArray::empty_with(named.iter().map(|(_, n)| (*n).clone()).collect());
        for h in hs {
            s.elems.push(named.iter().map(|(i, _)| out_text(&h.groups[*i].map(|r| slice(&chars, r)).unwrap_or_default(), as_string)).collect());
        }
        s.dims = if hs.is_empty() { vec![0, 0] } else { vec![1, hs.len()] };
        Value::Struct(s)
    };
    let _ = (&names, ngroups);
    let mut outs = Vec::new();
    let wanted: Vec<String> = if o.outputs.is_empty() {
        ["start", "end", "tokenextents", "match", "tokens", "names", "split"].iter().map(|s| s.to_string()).collect()
    } else {
        o.outputs.clone()
    };
    for w in wanted {
        let v = match w.as_str() {
            "start" if o.once => hits.first().map_or(Value::Num(Matrix::new(0, 0, Vec::new())), |h| Value::num(h.range.0 as f64 + 1.0)),
            "start" => num_row(hits.iter().map(|h| h.range.0 as f64 + 1.0).collect()),
            "end" if o.once => hits.first().map_or(Value::Num(Matrix::new(0, 0, Vec::new())), |h| Value::num(h.range.1 as f64)),
            "end" => num_row(hits.iter().map(|h| h.range.1 as f64).collect()),
            "match" if o.once => hits.first().map_or_else(|| if as_string { Value::Str(StrArray::from_dims(vec![0, 0], Vec::new())) } else { empty_char() }, |h| out_text(&slice(&chars, h.range), as_string)),
            "match" if n == 0 => if as_string { Value::Str(StrArray::from_dims(vec![0, 0], Vec::new())) } else { empty_cell() },
            "match" => text_row(hits.iter().map(|h| slice(&chars, h.range)).collect(), as_string),
            "tokens" if o.once => hits.first().map_or_else(|| Value::Cell(CellArray::from_dims(vec![1, 0], Vec::new())), token_cell),
            "tokens" if n == 0 => empty_cell(),
            "tokens" => Value::Cell(CellArray::row(hits.iter().map(token_cell).collect())),
            "tokenextents" if o.once => hits.first().map_or(Value::Num(Matrix::new(0, 0, Vec::new())), extents),
            "tokenextents" if n == 0 => empty_cell(),
            "tokenextents" => Value::Cell(CellArray::row(hits.iter().map(extents).collect())),
            "names" if o.once => match hits.first() {
                Some(h) => {
                    let Value::Struct(mut s) = names_struct(&[h]) else { unreachable!() };
                    s.dims = vec![1, 1];
                    Value::Struct(s)
                }
                None => names_struct(&[]),
            },
            "names" => names_struct(&hits.iter().collect::<Vec<_>>()),
            "split" => {
                let mut parts = Vec::new();
                let mut at = 0;
                for h in &hits {
                    parts.push(slice(&chars, (at, h.range.0)));
                    at = h.range.1;
                }
                parts.push(slice(&chars, (at, chars.len())));
                if as_string {
                    text_row(parts, true)
                } else {
                    Value::Cell(CellArray::row(parts.iter().map(|p| if p.is_empty() { empty_char() } else { Value::chars(p) }).collect()))
                }
            }
            _ => unreachable!(),
        };
        outs.push((w, v));
    }
    Ok(outs)
}

fn regexp(a: &[Value], nargout: usize, ignore_case: bool, fname: &str) -> Ret {
    let input = arg(a, 0, fname)?;
    let pat = arg(a, 1, fname)?;
    let o = parse_opts(&a[2..], ignore_case)?;
    let k = nargout.max(1);
    // Cell inputs give cell outputs, element by element.
    let cells = |v: &Value| -> Option<(crate::nd::Dims, Vec<Value>)> {
        match v {
            Value::Cell(c) => Some((c.dims.clone(), c.data.clone())),
            Value::Str(s) if s.data.len() != 1 => Some((s.dims.clone(), s.data.iter().map(|t| Value::string(t.clone().unwrap_or_default())).collect())),
            _ => None,
        }
    };
    match (cells(input), cells(pat)) {
        (None, None) => {
            let (text, as_string) = text_of(input)?;
            let p = pat.text()?;
            Ok(Some(regexp_one(&text, as_string, &p, &o)?.into_iter().take(k).map(|(_, v)| v).collect()))
        }
        (ci, cp) => {
            let (dims, items) = ci.clone().or(cp.clone()).unwrap();
            let n = items.len();
            let mut per_output: Vec<Vec<Value>> = vec![Vec::with_capacity(n); k];
            for i in 0..n {
                let (text, as_string) = match &ci {
                    Some((_, xs)) => text_of(&xs[i])?,
                    None => text_of(input)?,
                };
                let p = match &cp {
                    Some((_, ps)) => ps[i].text()?,
                    None => pat.text()?,
                };
                for (j, (_, v)) in regexp_one(&text, as_string, &p, &o)?.into_iter().take(k).enumerate() {
                    per_output[j].push(v);
                }
            }
            Ok(Some(per_output.into_iter().map(|vs| Value::Cell(CellArray::from_dims(dims.clone(), vs))).collect()))
        }
    }
}

/// Replacement text with $N, $0 and $<name> tokens and \n-style escapes.
fn expand_replacement(rep: &str, chars: &[char], h: &Hit, names: &[Option<String>]) -> String {
    let mut out = String::new();
    let r: Vec<char> = rep.chars().collect();
    let mut i = 0;
    while i < r.len() {
        match r[i] {
            '$' if i + 1 < r.len() && r[i + 1].is_ascii_digit() => {
                let mut j = i + 1;
                while j < r.len() && r[j].is_ascii_digit() {
                    j += 1;
                }
                let g: usize = r[i + 1..j].iter().collect::<String>().parse().unwrap_or(0);
                if g == 0 {
                    out.push_str(&slice(chars, h.range));
                } else if let Some(Some(rg)) = h.groups.get(g - 1) {
                    out.push_str(&slice(chars, *rg));
                }
                i = j;
            }
            '$' if i + 1 < r.len() && r[i + 1] == '<' => {
                let end = r[i + 2..].iter().position(|&c| c == '>').map(|p| p + i + 2);
                match end {
                    Some(e) => {
                        let name: String = r[i + 2..e].iter().collect();
                        if let Some(g) = names.iter().position(|n| n.as_deref() == Some(name.as_str())) {
                            if let Some(rg) = h.groups[g] {
                                out.push_str(&slice(chars, rg));
                            }
                        }
                        i = e + 1;
                    }
                    None => {
                        out.push('$');
                        i += 1;
                    }
                }
            }
            '\\' if i + 1 < r.len() => {
                out.push(match r[i + 1] {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    'a' => '\x07',
                    'b' => '\x08',
                    'f' => '\x0c',
                    'v' => '\x0b',
                    c => c,
                });
                i += 2;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn regexprep_one(text: &str, pat: &str, rep: &str, o: &Opts, nth: Option<usize>) -> R<String> {
    let re = compile(pat, o)?;
    let chars: Vec<char> = text.chars().collect();
    let names: Vec<Option<String>> = re.capture_names().skip(1).map(|n| n.map(str::to_string)).collect();
    let all = Opts { once: false, ..o.clone() };
    let hits = find_all(&re, text, &all)?;
    let mut out = String::new();
    let mut at = 0;
    for (k, h) in hits.iter().enumerate() {
        let selected = match nth {
            Some(n) => k + 1 == n,
            None => !o.once || k == 0,
        };
        if !selected {
            continue;
        }
        out.push_str(&slice(&chars, (at, h.range.0)));
        out.push_str(&expand_replacement(rep, &chars, h, &names));
        at = h.range.1;
    }
    out.push_str(&slice(&chars, (at, chars.len())));
    Ok(out)
}

fn regexprep(a: &[Value]) -> Ret {
    let input = arg(a, 0, "regexprep")?;
    let pats = cell_texts(arg(a, 1, "regexprep")?)?;
    let reps = cell_texts(arg(a, 2, "regexprep")?)?;
    let mut rest = Vec::new();
    let mut nth = None;
    for v in &a[3..] {
        if v.is_text() {
            rest.push(v.clone());
        } else {
            nth = Some(v.scalar()? as usize);
        }
    }
    let o = parse_opts(&rest, false)?;
    // Several patterns apply in turn; one replacement may serve them all.
    let apply = |text: &str| -> R<String> {
        let mut t = text.to_string();
        for (i, p) in pats.iter().enumerate() {
            let r = if reps.len() == 1 { &reps[0] } else { reps.get(i).ok_or_else(|| LangError::new("The number of replacement texts must match the number of expressions.", 0))? };
            t = regexprep_one(&t, p, r, &o, nth)?;
        }
        Ok(t)
    };
    match input {
        Value::Cell(c) => {
            let out = c.data.iter().map(|v| { let (t, s) = text_of(v)?; Ok(out_text(&apply(&t)?, s)) }).collect::<R<Vec<_>>>()?;
            one(Value::Cell(CellArray::from_dims(c.dims.clone(), out)))
        }
        Value::Str(s) => {
            let out = s.data.iter().map(|t| t.as_ref().map(|t| apply(t)).transpose()).collect::<R<Vec<_>>>()?;
            one(Value::Str(StrArray::from_dims(s.dims.clone(), out)))
        }
        v => one(Value::chars(&apply(&v.text()?)?)),
    }
}

fn cell_texts(v: &Value) -> R<Vec<String>> {
    match v {
        Value::Cell(c) => c.data.iter().map(|x| x.text()).collect(),
        Value::Str(s) => Ok(s.data.iter().map(|x| x.clone().unwrap_or_default()).collect()),
        other => Ok(vec![other.text()?]),
    }
}

fn regexptranslate(a: &[Value]) -> Ret {
    let op = arg(a, 0, "regexptranslate")?.text()?.to_lowercase();
    let (s, as_string) = text_of(arg(a, 1, "regexptranslate")?)?;
    let special = |c: char| "\\^$.|?*+()[]{}".contains(c);
    let out: String = match op.as_str() {
        "escape" => s.chars().flat_map(|c| if special(c) { vec!['\\', c] } else { vec![c] }).collect(),
        "wildcard" => s
            .chars()
            .flat_map(|c| match c {
                '*' => vec!['.', '*'],
                '?' => vec!['.'],
                '.' => vec!['\\', '.'],
                c if special(c) => vec!['\\', c],
                c => vec![c],
            })
            .collect(),
        "flexible" => s.split_whitespace().collect::<Vec<_>>().join(r"\s+"),
        _ => return rt(format!("Invalid translation type '{op}'.")),
    };
    one(out_text(&out, as_string))
}

// ---- other text functions ----------------------------------------------------------------

fn strtok(a: &[Value], nargout: usize) -> Ret {
    let (s, as_string) = text_of(arg(a, 0, "strtok")?)?;
    let delims: Vec<char> = match a.get(1) {
        Some(d) => d.text()?.chars().collect(),
        None => vec![' ', '\t', '\n', '\x0b', '\x0c', '\r', '\0'],
    };
    let chars: Vec<char> = s.chars().collect();
    let start = chars.iter().position(|c| !delims.contains(c)).unwrap_or(chars.len());
    let end = chars[start..].iter().position(|c| delims.contains(c)).map_or(chars.len(), |p| start + p);
    let tok: String = chars[start..end].iter().collect();
    let rem: String = chars[end..].iter().collect();
    let mut out = vec![out_text(&tok, as_string)];
    if nargout > 1 {
        out.push(out_text(&rem, as_string));
    }
    Ok(Some(out))
}

fn splitlines(a: &[Value]) -> Ret {
    let (s, as_string) = text_of(arg(a, 0, "splitlines")?)?;
    let parts: Vec<String> = s.replace("\r\n", "\n").replace('\r', "\n").split('\n').map(str::to_string).collect();
    // A column: strings for string input, a cell of char vectors for char input.
    if as_string {
        one(Value::Str(StrArray::from_dims(vec![parts.len(), 1], parts.into_iter().map(Some).collect())))
    } else {
        one(Value::Cell(CellArray::col(parts.iter().map(|p| Value::chars(p)).collect())))
    }
}

/// Base conversions: digits of a non-negative integer.
fn to_base(mut n: u128, base: u32, min_len: usize) -> String {
    const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut d = Vec::new();
    loop {
        d.push(DIGITS[(n % base as u128) as usize] as char);
        n /= base as u128;
        if n == 0 {
            break;
        }
    }
    while d.len() < min_len {
        d.push('0');
    }
    d.iter().rev().collect()
}

/// A char matrix with one right-aligned, zero-padded row per value.
fn base_rows(vals: &[f64], base: u32, min_len: usize, fname: &str) -> Ret {
    let mut rows = Vec::new();
    for &x in vals {
        if x < 0.0 || x.fract() != 0.0 || !x.is_finite() {
            return rt(format!("D must be a nonnegative integer smaller than flintmax for {fname}."));
        }
        rows.push(to_base(x as u128, base, min_len));
    }
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(min_len);
    let padded: Vec<String> = rows.iter().map(|r| format!("{}{r}", "0".repeat(w - r.len()))).collect();
    one(char_matrix(&padded))
}

fn char_matrix(rows: &[String]) -> Value {
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

/// Text rows of a char matrix, cellstr or string array.
fn rows_of(v: &Value) -> R<Vec<String>> {
    match v {
        Value::Num(m) if m.kind == Kind::Char && m.is_2d() => {
            let (r, c) = (m.rows(), m.cols());
            Ok((0..r).map(|i| (0..c).map(|j| char::from_u32(m.data[j * r + i] as u32).unwrap_or(' ')).collect()).collect())
        }
        other => cell_texts(other),
    }
}

fn from_base(rows: &[String], base: u32, fname: &str) -> Ret {
    let mut out = Vec::new();
    for r in rows {
        let t = r.trim();
        let mut n: f64 = 0.0;
        for c in t.chars() {
            if c == ' ' {
                continue;
            }
            let d = c.to_digit(base).ok_or_else(|| LangError::new(format!("Input to {fname} must contain only valid digits for base {base}."), 0))?;
            n = n * base as f64 + d as f64;
        }
        out.push(n);
    }
    one(Value::Num(Matrix::col(out)))
}

fn numeric_values(v: &Value) -> R<Vec<f64>> {
    Ok(v.to_matrix()?.data)
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match name {
        "regexp" => regexp(a, nargout, false, name),
        "regexpi" => regexp(a, nargout, true, name),
        "regexprep" => regexprep(a),
        "regexptranslate" => regexptranslate(a),
        "strtok" => strtok(a, nargout),
        "splitlines" => splitlines(a),
        "isStringScalar" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Str(s) if s.data.len() == 1))),
        "str2mat" | "strvcat" => {
            let mut rows = Vec::new();
            for v in a {
                // An empty text is one empty row.
                let rs = rows_of(v)?;
                let rs = if rs.is_empty() && v.is_text() { vec![String::new()] } else { rs };
                for r in rs {
                    // strvcat skips empty texts; str2mat keeps them.
                    if name == "str2mat" || !r.is_empty() {
                        rows.push(r);
                    }
                }
            }
            one(char_matrix(&rows))
        }
        "strmatch" => {
            let s = arg(a, 0, name)?.text()?;
            let rows = rows_of(arg(a, 1, name)?)?;
            let exact = matches!(a.get(2), Some(v) if v.text().map(|t| t == "exact").unwrap_or(false));
            let hits: Vec<f64> = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| if exact { r.trim_end_matches([' ', '\0']) == s } else { r.starts_with(&s) })
                .map(|(i, _)| i as f64 + 1.0)
                .collect();
            one(if hits.is_empty() { Value::Num(Matrix::new(0, 1, Vec::new())) } else { Value::Num(Matrix::col(hits)) })
        }
        "findstr" => {
            let (x, y) = (arg(a, 0, name)?.text()?, arg(a, 1, name)?.text()?);
            let (long, short) = if y.chars().count() > x.chars().count() { (y, x) } else { (x, y) };
            let l: Vec<char> = long.chars().collect();
            let s: Vec<char> = short.chars().collect();
            let hits: Vec<f64> = if s.is_empty() { Vec::new() } else { (0..=l.len().saturating_sub(s.len())).filter(|&i| l[i..i + s.len()] == s[..]).map(|i| i as f64 + 1.0).collect() };
            one(if hits.is_empty() { Value::Num(Matrix::new(1, 0, Vec::new())) } else { Value::Num(Matrix::row(hits)) })
        }
        "sscanf" => sscanf(a, nargout),
        "eraseBetween" | "replaceBetween" => {
            let (s, as_string) = text_of(arg(a, 0, name)?)?;
            let chars: Vec<char> = s.chars().collect();
            let (start, end) = (arg(a, 1, name)?, arg(a, 2, name)?);
            let rep = if name == "replaceBetween" { arg(a, 3, name)?.text()? } else { String::new() };
            let inclusive = !matches!(a.get(if name == "replaceBetween" { 5 } else { 4 }), Some(v) if v.text().map(|t| t.to_lowercase() == "exclusive").unwrap_or(false));
            let locate = |v: &Value, is_start: bool| -> R<usize> {
                if v.is_text() || matches!(v, Value::Str(_)) {
                    let t: Vec<char> = v.text()?.chars().collect();
                    let p = (0..=chars.len().saturating_sub(t.len())).find(|&i| chars[i..i + t.len()] == t[..]).ok_or_else(|| LangError::new("Boundary text not found.", 0))?;
                    // Text boundaries are exclusive by default.
                    Ok(if is_start { p + t.len() } else { p })
                } else {
                    let k = v.scalar()? as usize;
                    Ok(if is_start { if inclusive { k - 1 } else { k } } else if inclusive { k } else { k - 1 })
                }
            };
            let (i, j) = (locate(start, true)?, locate(end, false)?);
            let out: String = chars[..i.min(chars.len())].iter().chain(rep.chars().collect::<Vec<_>>().iter()).chain(chars[j.min(chars.len())..].iter()).collect();
            one(out_text(&out, as_string))
        }
        "convertContainedStringsToChars" => {
            let conv = |v: &Value| -> Value {
                match v {
                    Value::Str(s) if s.data.len() == 1 => Value::chars(&s.data[0].clone().unwrap_or_default()),
                    Value::Str(s) => Value::Cell(CellArray::from_dims(s.dims.clone(), s.data.iter().map(|t| Value::chars(&t.clone().unwrap_or_default())).collect())),
                    Value::Cell(c) => Value::Cell(CellArray::from_dims(c.dims.clone(), c.data.iter().map(|x| if let Value::Str(s) = x { if s.data.len() == 1 { Value::chars(&s.data[0].clone().unwrap_or_default()) } else { x.clone() } } else { x.clone() }).collect())),
                    other => other.clone(),
                }
            };
            Ok(Some(a.iter().take(nargout.max(1)).map(conv).collect()))
        }
        "dec2bin" | "dec2hex" => {
            let vals = numeric_values(arg(a, 0, name)?)?;
            let min = a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(0.0) as usize;
            base_rows(&vals, if name == "dec2bin" { 2 } else { 16 }, min.max(1), name)
        }
        "dec2base" => {
            let vals = numeric_values(arg(a, 0, name)?)?;
            let base = arg(a, 1, name)?.scalar()? as u32;
            if !(2..=36).contains(&base) {
                return rt("Base must be an integer between 2 and 36.");
            }
            let min = a.get(2).map(|v| v.scalar()).transpose()?.unwrap_or(0.0) as usize;
            base_rows(&vals, base, min.max(1), name)
        }
        "bin2dec" => from_base(&rows_of(arg(a, 0, name)?)?, 2, name),
        "hex2dec" => from_base(&rows_of(arg(a, 0, name)?)?, 16, name),
        "base2dec" => {
            let base = arg(a, 1, name)?.scalar()? as u32;
            from_base(&rows_of(arg(a, 0, name)?)?, base, name)
        }
        "num2hex" => {
            let m = arg(a, 0, name)?.to_matrix()?;
            let rows: Vec<String> = m.data.iter().map(|&x| if m.kind == Kind::Single { format!("{:08x}", (x as f32).to_bits()) } else { format!("{:016x}", x.to_bits()) }).collect();
            one(char_matrix(&rows))
        }
        "hex2num" => {
            let rows = rows_of(arg(a, 0, name)?)?;
            let vals = rows.iter().map(|r| {
                let t = r.trim();
                let padded = format!("{t:0<16}");
                u64::from_str_radix(&padded, 16).map(f64::from_bits).map_err(|_| LangError::new("Input to hex2num must contain only hexadecimal digits.", 0))
            }).collect::<R<Vec<_>>>()?;
            one(Value::Num(Matrix::col(vals)))
        }
        "native2unicode" => {
            let bytes: Vec<u8> = numeric_values(arg(a, 0, name)?)?.iter().map(|&b| b as u8).collect();
            one(Value::chars(&String::from_utf8_lossy(&bytes)))
        }
        "unicode2native" => {
            let s = arg(a, 0, name)?.text()?;
            one(Value::Num(Matrix::row(s.bytes().map(|b| b as f64).collect()).cast_to(Kind::Int(IntClass::U8))))
        }
        "matches" => {
            let pats = cell_texts(arg(a, 1, name)?)?;
            let ignore = a.len() > 3 && a[2].text().map(|t| t.eq_ignore_ascii_case("IgnoreCase")).unwrap_or(false) && a[3].is_true().unwrap_or(false);
            let test = |t: &str| pats.iter().any(|p| if ignore { p.to_lowercase() == t.to_lowercase() } else { p == t });
            match arg(a, 0, name)? {
                Value::Cell(c) => one(Value::Num(Matrix::from_dims(c.dims.clone(), c.data.iter().map(|v| v.text().map(|t| if test(&t) { 1.0 } else { 0.0 })).collect::<R<Vec<_>>>()?).cast_to(Kind::Logical))),
                Value::Str(s) => one(Value::Num(Matrix::from_dims(s.dims.clone(), s.data.iter().map(|t| if t.as_deref().map(test).unwrap_or(false) { 1.0 } else { 0.0 }).collect()).cast_to(Kind::Logical))),
                v => one(Value::bool(test(&v.text()?))),
            }
        }
        "extract" => {
            // extract(str, pos) and extract(str, text): the matched texts as a string column.
            let (s, _) = text_of(arg(a, 0, name)?)?;
            let what = arg(a, 1, name)?;
            if what.is_text() || matches!(what, Value::Str(_)) {
                let t = what.text()?;
                let n = if t.is_empty() { 0 } else { s.matches(&t).count() };
                return one(Value::Str(StrArray::from_dims(vec![n, 1], vec![Some(t); n])));
            }
            let k = what.scalar()? as usize;
            let c: String = s.chars().nth(k - 1).map(String::from).unwrap_or_default();
            one(Value::string(c))
        }
        "strread" => rt("strread has been removed; use textscan or strsplit."),
        _ => Ok(None),
    }
    .or_else(|e| if e.message.is_empty() { none() } else { Err(e) })
}

/// sscanf(str, format[, size]): values read with a C-like format, as a column.
fn sscanf(a: &[Value], nargout: usize) -> Ret {
    let s = arg(a, 0, "sscanf")?.text()?;
    let fmt = arg(a, 1, "sscanf")?.text()?;
    let fmt = fmt.replace("\\n", "\n").replace("\\t", "\t");
    let chars: Vec<char> = s.chars().collect();
    let f: Vec<char> = fmt.chars().collect();
    let mut vals: Vec<f64> = Vec::new();
    let mut is_char = true;
    let mut pos = 0usize;
    let mut errmsg = String::new();
    let skip_ws = |p: &mut usize| while *p < chars.len() && chars[*p].is_whitespace() { *p += 1 };
    'outer: loop {
        let before = pos;
        let mut i = 0;
        while i < f.len() {
            let c = f[i];
            if c.is_whitespace() {
                skip_ws(&mut pos);
                i += 1;
                continue;
            }
            if c != '%' {
                if pos < chars.len() && chars[pos] == c {
                    pos += 1;
                    i += 1;
                    continue;
                }
                break 'outer;
            }
            // %[*][width][lhl]conversion
            i += 1;
            let skip = f.get(i) == Some(&'*');
            if skip {
                i += 1;
            }
            let mut width = String::new();
            while i < f.len() && f[i].is_ascii_digit() {
                width.push(f[i]);
                i += 1;
            }
            let width: usize = width.parse().unwrap_or(usize::MAX);
            while i < f.len() && matches!(f[i], 'l' | 'h') {
                i += 1;
            }
            let conv = match f.get(i) { Some(c) => *c, None => break 'outer };
            i += 1;
            match conv {
                'd' | 'i' | 'u' | 'f' | 'g' | 'e' | 'x' | 'o' => {
                    skip_ws(&mut pos);
                    let start = pos;
                    let ok = |c: char, k: usize, prev: char| -> bool {
                        match conv {
                            'd' | 'i' | 'u' => c.is_ascii_digit() || (k == 0 && (c == '-' || c == '+')),
                            'x' => c.is_ascii_hexdigit(),
                            'o' => ('0'..='7').contains(&c),
                            _ => c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E') || ((c == '-' || c == '+') && (k == 0 || matches!(prev, 'e' | 'E'))),
                        }
                    };
                    let mut k = 0;
                    while pos < chars.len() && k < width && ok(chars[pos], k, if pos > 0 { chars[pos - 1] } else { ' ' }) {
                        pos += 1;
                        k += 1;
                    }
                    let t: String = chars[start..pos].iter().collect();
                    let v = match conv {
                        'x' => i64::from_str_radix(&t, 16).ok().map(|v| v as f64),
                        'o' => i64::from_str_radix(&t, 8).ok().map(|v| v as f64),
                        _ => t.parse::<f64>().ok().or_else(|| match t.to_lowercase().as_str() { "inf" => Some(f64::INFINITY), "nan" => Some(f64::NAN), _ => None }),
                    };
                    match v {
                        Some(v) => {
                            if !skip {
                                vals.push(v);
                                is_char = false;
                            }
                        }
                        None => {
                            pos = start;
                            if pos < chars.len() {
                                errmsg = "Matching failure in format.".into();
                            }
                            break 'outer;
                        }
                    }
                }
                's' => {
                    skip_ws(&mut pos);
                    let start = pos;
                    while pos < chars.len() && !chars[pos].is_whitespace() && pos - start < width {
                        pos += 1;
                    }
                    if pos == start {
                        break 'outer;
                    }
                    if !skip {
                        vals.extend(chars[start..pos].iter().map(|&c| c as u32 as f64));
                    }
                }
                'c' => {
                    let w = if width == usize::MAX { 1 } else { width };
                    if pos + w > chars.len() {
                        break 'outer;
                    }
                    if !skip {
                        vals.extend(chars[pos..pos + w].iter().map(|&c| c as u32 as f64));
                    }
                    pos += w;
                }
                _ => break 'outer,
            }
        }
        // The format repeats while it consumes input.
        if pos >= chars.len() || pos == before {
            break;
        }
    }
    let size = a.get(2).map(|v| v.to_matrix()).transpose()?;
    let all_text = is_char && !vals.is_empty();
    let mut m = match size {
        Some(sz) if sz.numel() == 2 => {
            let r = sz.data[0] as usize;
            let c = if sz.data[1].is_infinite() { vals.len().div_ceil(r.max(1)) } else { sz.data[1] as usize };
            vals.resize(r * c, 0.0);
            Matrix::new(r, c, vals.clone())
        }
        Some(sz) if sz.numel() == 1 && sz.data[0].is_finite() => {
            vals.truncate(sz.data[0] as usize);
            Matrix::col(vals.clone())
        }
        _ => if vals.is_empty() { Matrix::new(0, 0, Vec::new()) } else { Matrix::col(vals.clone()) },
    };
    if all_text {
        m = Matrix::row(m.data.clone()).cast_to(Kind::Char);
    }
    let count = if all_text { 1.0 } else { m.numel() as f64 };
    let mut out = vec![Value::Num(m)];
    if nargout > 1 {
        out.push(Value::num(count));
    }
    if nargout > 2 {
        out.push(Value::chars(&errmsg));
    }
    if nargout > 3 {
        out.push(Value::num(pos as f64 + 1.0));
    }
    Ok(Some(out))
}
