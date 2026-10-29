//! Language and workspace functions: `eval` and friends, workspace queries (`who`, `whos`,
//! `which`, `clearvars`), name utilities, release and version queries, and `run`/`cd`/`type`.

use crate::builtins::{arg, none, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "eval", "evalc", "evalin", "assignin", "run", "mfilename", "iskeyword", "isvarname", "genvarname", "namelengthmax", "nargoutchk",
    "validatestring", "builtin", "who", "whos", "clearvars", "which", "matlabRelease", "isMATLABReleaseOlderThan", "verLessThan", "ver",
    "cd", "genpath", "type", "import", "input",
];

/// MATLAB's reserved words, in `iskeyword()` order.
const KEYWORDS: &[&str] = &[
    "break", "case", "catch", "classdef", "continue", "else", "elseif", "end", "for", "function", "global", "if", "otherwise", "parfor",
    "persistent", "return", "spmd", "switch", "try", "while",
];

/// The release matlivre follows.
const RELEASE: &str = "R2023b";
const VERSION: &str = "23.2.0";

fn text_arg(a: &[Value], i: usize, name: &str) -> R<String> {
    arg(a, i, name)?.text()
}

pub fn is_varname(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(f) if f.is_ascii_alphabetic())
        && s.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && s.len() <= 63
        && !KEYWORDS.contains(&s)
}

/// `genvarname`: whitespace removed (the next letter capitalized), other invalid characters
/// replaced by underscores, an `x` prefix for names not starting with a letter or that are
/// keywords, and a numeric suffix to avoid the exclusions.
fn genvarname(s: &str, exclude: &[String]) -> String {
    let mut out = String::new();
    let mut cap = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            cap = !out.is_empty();
            continue;
        }
        let c = if ch.is_ascii_alphanumeric() || ch == '_' { ch } else { '_' };
        if cap {
            out.extend(c.to_uppercase());
            cap = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        out = "x".into();
    } else if KEYWORDS.contains(&out.as_str()) {
        let mut ch = out.chars();
        let first = ch.next().unwrap().to_uppercase().collect::<String>();
        out = format!("x{first}{}", ch.as_str());
    } else if !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out = format!("x{out}");
    }
    out.truncate(63);
    if !exclude.contains(&out) {
        return out;
    }
    (1..).map(|k| format!("{out}{k}")).find(|n| !exclude.contains(n)).unwrap()
}

/// Compares dotted version numbers: true if `a` < `b`.
fn version_less(a: &str, b: &str) -> bool {
    let parse = |s: &str| s.split('.').map(|p| p.trim().parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (x, y) = (parse(a), parse(b));
    for i in 0..x.len().max(y.len()) {
        let (p, q) = (x.get(i).copied().unwrap_or(0), y.get(i).copied().unwrap_or(0));
        if p != q {
            return p < q;
        }
    }
    false
}

/// "R2023b" as a sortable (year, half) pair.
fn release_key(r: &str) -> R<(u32, u8)> {
    let r = r.trim();
    let bad = || LangError::new(format!("Invalid release '{r}'. Specify a release as a string or character vector of the form 'R2023b'."), 0);
    let rest = r.strip_prefix('R').or_else(|| r.strip_prefix('r')).ok_or_else(bad)?;
    if rest.len() != 5 {
        return Err(bad());
    }
    let year: u32 = rest[..4].parse().map_err(|_| bad())?;
    let half = match &rest[4..] {
        "a" | "A" => 0,
        "b" | "B" => 1,
        _ => return Err(bad()),
    };
    Ok((year, half))
}

fn class_and_bytes(v: &Value) -> (String, usize) {
    let bytes = match v {
        Value::Num(m) => m.numel() * m.kind.bytes() * if m.is_complex() { 2 } else { 1 },
        _ => 0,
    };
    (v.class_name(), bytes)
}

fn size_text(v: &Value) -> String {
    v.dims().iter().map(|d| d.to_string()).collect::<Vec<_>>().join("x")
}

/// Variable names of the current workspace, sorted, optionally limited to `filters`
/// (names or `*` wildcards).
fn workspace_names(it: &Interpreter, filters: &[String]) -> Vec<String> {
    let mut names = it.var_names();
    names.sort();
    if filters.is_empty() {
        return names;
    }
    names.into_iter().filter(|n| filters.iter().any(|f| wildcard(f, n))).collect()
}

fn wildcard(pat: &str, s: &str) -> bool {
    fn go(p: &[char], s: &[char]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], s) || (!s.is_empty() && go(p, &s[1..])),
            (Some(a), Some(b)) if a == b => go(&p[1..], &s[1..]),
            _ => false,
        }
    }
    go(&pat.chars().collect::<Vec<_>>(), &s.chars().collect::<Vec<_>>())
}

fn texts(a: &[Value]) -> R<Vec<String>> {
    a.iter().map(|v| v.text()).collect()
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match name {
        "eval" => {
            let code = text_arg(a, 0, name)?;
            match a.get(1) {
                // eval(try, catch): run the second text if the first fails.
                Some(c) => {
                    let catch = c.text()?;
                    match it.eval_text(&code, nargout) {
                        Ok(v) => Ok(Some(v)),
                        Err(_) => Ok(Some(it.eval_text(&catch, nargout)?)),
                    }
                }
                None => Ok(Some(eval_checked(it, &code, nargout)?)),
            }
        }
        "evalc" => {
            let code = text_arg(a, 0, name)?;
            let extra = nargout.saturating_sub(1);
            let (vals, text) = it.capture_text(|it| eval_checked(it, &code, extra))?;
            let mut out = vec![Value::chars(&text)];
            out.extend(vals);
            Ok(Some(out))
        }
        "evalin" => {
            let ws = text_arg(a, 0, name)?;
            let code = text_arg(a, 1, name)?;
            let catch = a.get(2).map(|v| v.text()).transpose()?;
            let vals = it.in_workspace(&ws, |it| match it.eval_text(&code, nargout) {
                Ok(v) => Ok(v),
                Err(e) => match &catch {
                    Some(c) => it.eval_text(c, nargout),
                    None => Err(e),
                },
            })?;
            Ok(Some(vals))
        }
        "assignin" => {
            let ws = text_arg(a, 0, name)?;
            let var = text_arg(a, 1, name)?;
            if !is_varname(&var) {
                return rt(format!("Invalid variable name \"{var}\" in ASSIGNIN."));
            }
            let value = arg(a, 2, name)?.clone();
            it.in_workspace(&ws, |it| {
                it.set_var(&var, value);
                Ok(())
            })?;
            none()
        }
        "run" => {
            let script = text_arg(a, 0, name)?;
            it.run_script(&script)?;
            none()
        }
        "mfilename" => {
            let n = it.current_function().unwrap_or_default();
            one(Value::chars(&n))
        }
        "iskeyword" => match a.first() {
            None => one(Value::Cell(CellArray::col(KEYWORDS.iter().map(|k| Value::chars(k)).collect()))),
            Some(v) => one(Value::bool(KEYWORDS.contains(&v.text()?.as_str()))),
        },
        "isvarname" => {
            let ok = match arg(a, 0, name)? {
                v if v.is_text() && v.numel() > 0 || matches!(v, Value::Str(_)) => is_varname(&v.text()?),
                _ => false,
            };
            one(Value::bool(ok))
        }
        "genvarname" => {
            let exclude = match a.get(1) {
                Some(v) => cell_texts(v)?,
                None => Vec::new(),
            };
            match arg(a, 0, name)? {
                Value::Cell(c) => {
                    let mut taken = exclude;
                    let mut out = Vec::new();
                    for v in &c.data {
                        let n = genvarname(&v.text()?, &taken);
                        taken.push(n.clone());
                        out.push(Value::chars(&n));
                    }
                    one(Value::Cell(CellArray::from_dims(c.dims.clone(), out)))
                }
                v => one(Value::chars(&genvarname(&v.text()?, &exclude))),
            }
        }
        "namelengthmax" => one(Value::num(63.0)),
        "nargoutchk" => {
            let lo = arg(a, 0, name)?.scalar()?;
            let hi = arg(a, 1, name)?.scalar()?;
            let n = it.get_var("nargout").map(|v| v.scalar()).transpose()?.unwrap_or(0.0);
            if n < lo {
                return rt("Not enough output arguments.");
            }
            if n > hi {
                return rt("Too many output arguments.");
            }
            none()
        }
        "validatestring" => {
            let s = text_arg(a, 0, name)?;
            let valid = cell_texts(arg(a, 1, name)?)?;
            let lower = s.to_lowercase();
            let hits: Vec<&String> = valid.iter().filter(|v| v.to_lowercase().starts_with(&lower) && !s.is_empty()).collect();
            // An exact match wins over longer ones that share the prefix.
            if let Some(exact) = hits.iter().find(|v| v.to_lowercase() == lower) {
                return one(Value::chars(exact));
            }
            let list = valid.iter().map(|v| format!("'{v}'")).collect::<Vec<_>>().join(", ");
            match hits.as_slice() {
                [one_hit] => one(Value::chars(one_hit)),
                [] => rt(format!("Expected input to match one of these values:\n\n{list}\n\nThe input, '{s}', did not match any of the valid values.")),
                _ => {
                    // Several matches are fine when each is a prefix of the next (shortest wins).
                    let shortest = hits.iter().min_by_key(|v| v.len()).unwrap();
                    if hits.iter().all(|v| v.to_lowercase().starts_with(&shortest.to_lowercase())) {
                        one(Value::chars(shortest))
                    } else {
                        rt(format!("Expected input to match one of these values:\n\n{list}\n\nThe input, '{s}', matched more than one valid value."))
                    }
                }
            }
        }
        "builtin" => {
            let f = text_arg(a, 0, name)?;
            match crate::builtins::call(it, &f, a[1..].to_vec(), nargout)? {
                Some(v) => Ok(Some(v)),
                None => rt(format!("Cannot find builtin function '{f}'.")),
            }
        }
        "who" | "whos" => {
            let filters = texts(a)?;
            let names = workspace_names(it, &filters);
            if name == "who" {
                if nargout > 0 {
                    return one(Value::Cell(CellArray::col(names.iter().map(|n| Value::chars(n)).collect())));
                }
                if !names.is_empty() {
                    let w = names.iter().map(|n| n.len()).max().unwrap_or(0) + 2;
                    let row: String = names.iter().map(|n| format!("{n:<w$}")).collect();
                    it.emit(OutputKind::Text(format!("\nYour variables are:\n\n{row}\n\n")));
                }
                return none();
            }
            let rows: Vec<(String, String, usize, String, bool)> = names
                .iter()
                .map(|n| {
                    let v = it.get_var(n).cloned().unwrap_or_else(Value::empty);
                    let (class, bytes) = class_and_bytes(&v);
                    let complex = matches!(&v, Value::Num(m) if m.is_complex());
                    (n.clone(), size_text(&v), bytes, class, complex)
                })
                .collect();
            if nargout > 0 {
                let mut s = StructArray::empty_with(
                    ["name", "size", "bytes", "class", "global", "sparse", "complex", "nesting", "persistent"].iter().map(|f| f.to_string()).collect(),
                );
                for (n, _, bytes, class, complex) in &rows {
                    let v = it.get_var(n).cloned().unwrap_or_else(Value::empty);
                    let mut e = StructArray::scalar();
                    e.set("name", Value::chars(n));
                    e.set("size", Value::Num(Matrix::row(v.dims().iter().map(|d| *d as f64).collect())));
                    e.set("bytes", Value::num(*bytes as f64));
                    e.set("class", Value::chars(class));
                    e.set("global", Value::bool(false));
                    e.set("sparse", Value::bool(false));
                    e.set("complex", Value::bool(*complex));
                    let mut nest = StructArray::scalar();
                    nest.set("function", Value::chars(&it.current_function().unwrap_or_default()));
                    nest.set("level", Value::num(1.0));
                    e.set("nesting", Value::Struct(nest));
                    e.set("persistent", Value::bool(false));
                    let row: Vec<Value> = s.fields.iter().map(|f| e.get(f).cloned().unwrap_or_else(Value::empty)).collect();
                    s.elems.push(row);
                }
                s.dims = vec![rows.len(), 1];
                return one(Value::Struct(s));
            }
            if rows.is_empty() {
                return none();
            }
            let mut t = String::from("  Name      Size            Bytes  Class     Attributes\n\n");
            for (n, size, bytes, class, complex) in &rows {
                let attr = if *complex { "complex" } else { "" };
                t.push_str(&format!("  {n:<10}{size:<15}{bytes:>6}  {class:<10}{attr}\n"));
            }
            t.push('\n');
            it.emit(OutputKind::Text(t));
            none()
        }
        "clearvars" => {
            let args = texts(a)?;
            let (keep_pos, regexp) = (args.iter().position(|s| s == "-except"), args.iter().any(|s| s == "-regexp"));
            let (targets, keep): (Vec<String>, Vec<String>) = match keep_pos {
                Some(p) => (args[..p].iter().filter(|s| *s != "-regexp").cloned().collect(), args[p + 1..].iter().filter(|s| *s != "-regexp").cloned().collect()),
                None => (args.iter().filter(|s| *s != "-regexp").cloned().collect(), Vec::new()),
            };
            let matches = |pat: &str, n: &str| if regexp { regex_lite_match(pat, n) } else { wildcard(pat, n) };
            for n in it.var_names() {
                let targeted = targets.is_empty() || targets.iter().any(|t| matches(t, &n));
                let kept = keep.iter().any(|k| matches(k, &n));
                if targeted && !kept {
                    it.remove_var(&n);
                }
            }
            none()
        }
        "which" => {
            let n = text_arg(a, 0, name)?;
            let text = if it.get_var(&n).is_some() {
                format!("{n} is a variable.")
            } else if crate::builtins::exists(&n) {
                format!("built-in (matlivre)  % {n}")
            } else if let Some(p) = it.function_file(&n) {
                p
            } else {
                format!("'{n}' not found.")
            };
            if nargout > 0 {
                let out = if text.ends_with("not found.") { String::new() } else if text.ends_with("is a variable.") { "variable".into() } else { text.split("  %").next().unwrap_or("").to_string() };
                return one(Value::chars(&out));
            }
            it.emit(OutputKind::Text(format!("{text}\n")));
            none()
        }
        "matlabRelease" => {
            let mut s = StructArray::scalar();
            s.set("Release", Value::string(RELEASE));
            s.set("Stage", Value::string("release"));
            s.set("Update", Value::num(3.0));
            s.set("Date", Value::string("04-Oct-2023"));
            one(Value::Struct(s))
        }
        "isMATLABReleaseOlderThan" => {
            let r = text_arg(a, 0, name)?;
            one(Value::bool(release_key(RELEASE)? < release_key(&r)?))
        }
        "verLessThan" => {
            let product = text_arg(a, 0, name)?;
            let v = text_arg(a, 1, name)?;
            if product.to_lowercase() != "matlab" && product.to_lowercase() != "symbolic" {
                return rt(format!("Unknown product: {product}."));
            }
            one(Value::bool(version_less(VERSION, &v)))
        }
        "ver" => {
            if nargout > 0 {
                let mut s = StructArray::scalar();
                s.set("Name", Value::chars("MATLAB"));
                s.set("Version", Value::chars("23.2"));
                s.set("Release", Value::chars(&format!("({RELEASE})")));
                s.set("Date", Value::chars("04-Oct-2023"));
                return one(Value::Struct(s));
            }
            let bar = "-".repeat(100);
            it.emit(OutputKind::Text(format!(
                "{bar}\nmatlivre, compatible with MATLAB Version {VERSION} ({RELEASE})\n{bar}\nMATLAB                                                Version 23.2        ({RELEASE})\nSymbolic Math Toolbox                                 Version 23.2        ({RELEASE})\n"
            )));
            none()
        }
        "cd" => {
            let old = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default();
            match a.first() {
                None => {
                    if nargout > 0 {
                        return one(Value::chars(&old));
                    }
                    it.emit(OutputKind::Text(format!("{old}\n")));
                    none()
                }
                Some(d) => {
                    let dir = d.text()?;
                    std::env::set_current_dir(&dir).map_err(|_| LangError::new(format!("Unable to change current folder to '{dir}' (Name is nonexistent or not a folder)."), 0))?;
                    if nargout > 0 { one(Value::chars(&old)) } else { none() }
                }
            }
        }
        "genpath" => {
            let root = text_arg(a, 0, name)?;
            let mut out = Vec::new();
            collect_dirs(std::path::Path::new(&root), &mut out);
            let sep = if cfg!(windows) { ";" } else { ":" };
            let mut s = out.join(sep);
            if !s.is_empty() {
                s.push_str(sep);
            }
            one(Value::chars(&s))
        }
        "type" => {
            for f in texts(a)? {
                let src = it.function_source(&f).or_else(|| std::fs::read_to_string(&f).ok());
                match src {
                    Some(s) => it.emit(OutputKind::Text(format!("\n{s}\n"))),
                    None => return rt(format!("File '{f}' not found.")),
                }
            }
            none()
        }
        // No packages: `import pkg.*` has nothing to bring into scope.
        "import" => none(),
        "input" => rt("input is not supported: matlivre runs scripts without an interactive command window."),
        _ => Ok(None),
    }
}

/// `eval` with MATLAB's message when a statement is used where a value is expected.
fn eval_checked(it: &mut Interpreter, code: &str, nargout: usize) -> R<Vec<Value>> {
    it.eval_text(code, nargout).map_err(|e| {
        if nargout > 0 && e.message.starts_with("Unable to return a value from a statement") && code.contains('=') {
            LangError::new("Incorrect use of '=' operator. Assign a value to a variable using '=' and compare values for equality using '=='.", e.line)
        } else {
            e
        }
    })
}

fn cell_texts(v: &Value) -> R<Vec<String>> {
    match v {
        Value::Cell(c) => c.data.iter().map(|x| x.text()).collect(),
        Value::Str(s) => Ok(s.data.iter().map(|x| x.clone().unwrap_or_default()).collect()),
        other => Ok(vec![other.text()?]),
    }
}

/// Folders below `dir` in genpath's order, skipping private, class (@), package (+) and
/// resources folders.
fn collect_dirs(dir: &std::path::Path, out: &mut Vec<String>) {
    if !dir.is_dir() {
        return;
    }
    out.push(dir.display().to_string());
    let mut subs: Vec<_> = std::fs::read_dir(dir).map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
    subs.sort();
    for s in subs {
        let n = s.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if n == "private" || n == "resources" || n.starts_with('@') || n.starts_with('+') {
            continue;
        }
        collect_dirs(&s, out);
    }
}

/// A small regular-expression matcher for `clearvars -regexp`: the pattern must match
/// somewhere in the name (`^`, `$`, `.`, `*`, `+`, `?` and literal characters).
fn regex_lite_match(pat: &str, s: &str) -> bool {
    fancy_regex::Regex::new(pat).ok().and_then(|r| r.is_match(s).ok()).unwrap_or(false)
}
