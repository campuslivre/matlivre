//! Preferences (`addpref`, `getpref`, `setpref`, `ispref`, `rmpref`), `loadenv`, `clipboard`,
//! `formattedDisplayText`, `winqueryreg`, `perl`, `matlab.io.saveVariablesToScript`,
//! `matfile`, and text import options (`detectImportOptions`, `delimitedTextImportOptions`,
//! `getvaropts`, `setvaropts`, `setvartype`, `preview`, and `readtable`/`readmatrix`/`readcell`
//! with an options object).
//!
//! Preferences persist in `matlivreprefs.mat` in MatLivre's settings folder (or the folder
//! named by `MATLIVRE_PREFDIR`), one structure per group.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::builtins::{arg, none, one, Ret};
use crate::class::ClassDef;
use crate::display::NumFormat;
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::table::Table;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "addpref", "getpref", "setpref", "ispref", "rmpref", "loadenv", "clipboard", "formattedDisplayText", "winqueryreg", "perl",
    "matlab.io.saveVariablesToScript", "matfile", "detectImportOptions", "delimitedTextImportOptions", "getvaropts", "setvaropts", "setvartype", "preview",
];

/// The readers that also accept an import options object.
pub const READERS: &[&str] = &["readtable", "readmatrix", "readcell", "readtimetable"];

const OPTS_CLASS: &str = "matlab.io.text.DelimitedTextImportOptions";
const OPTS_PROPS: &[&str] = &[
    "Delimiter", "Whitespace", "LineEnding", "CommentStyle", "ConsecutiveDelimitersRule", "LeadingDelimitersRule", "TrailingDelimitersRule", "EmptyLineRule",
    "Encoding", "MissingRule", "ImportErrorRule", "ExtraColumnsRule", "VariableNames", "VariableTypes", "SelectedVariableNames", "VariableOptions",
    "VariableNamingRule", "DataLines", "VariableNamesLine", "RowNamesColumn", "VariableUnitsLine", "VariableDescriptionsLine",
];
const NUMERIC_TYPES: &[&str] = &["double", "single", "int8", "int16", "int32", "int64", "uint8", "uint16", "uint32", "uint64"];

// ---- generated classes -------------------------------------------------------------------------

/// A value class with these (public) properties, shown under MATLAB's dotted class name.
fn class_def(name: &str, props: &[&str]) -> Rc<ClassDef> {
    thread_local! {
        static DEFS: RefCell<HashMap<String, Rc<ClassDef>>> = RefCell::new(HashMap::new());
    }
    let key = format!("{name}|{}", props.join(","));
    DEFS.with(|d| {
        if let Some(def) = d.borrow().get(&key) {
            return def.clone();
        }
        let mut src = String::from("classdef MatlivreGenerated\n  properties\n");
        for p in props {
            src.push_str(&format!("    {p}\n"));
        }
        src.push_str("  end\nend\n");
        let ast = crate::class::parse_classdef(&src).expect("built-in class source parses");
        let mut def = ClassDef::build(ast, &[]);
        def.name = name.to_string();
        let def = Rc::new(def);
        d.borrow_mut().insert(key, def.clone());
        def
    })
}

fn object(name: &str, props: &[&str], values: Vec<Value>) -> Value {
    Value::Object(Object { class: class_def(name, props), props: Rc::new(RefCell::new(values)) })
}

fn cellrow(items: &[String]) -> Value {
    Value::Cell(CellArray::row(items.iter().map(|s| Value::chars(s)).collect()))
}

fn texts(v: &Value) -> R<Vec<String>> {
    match v {
        Value::Cell(c) => c.data.iter().map(|x| x.text()).collect(),
        Value::Str(s) => Ok(s.data.iter().map(|x| x.clone().unwrap_or_default()).collect()),
        other => Ok(vec![other.text()?]),
    }
}

// ---- preferences -------------------------------------------------------------------------------

pub(crate) fn pref_path() -> std::path::PathBuf {
    if let Ok(d) = std::env::var("MATLIVRE_PREFDIR") {
        return std::path::PathBuf::from(d).join("matlivreprefs.mat");
    }
    #[cfg(target_os = "macos")]
    if std::env::var_os("APPDATA").is_none() {
        if let Ok(h) = std::env::var("HOME") {
            return std::path::PathBuf::from(h).join("Library").join("Application Support").join("matlivre").join("matlivreprefs.mat");
        }
    }
    let base = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|_| std::env::var("XDG_CONFIG_HOME").map(std::path::PathBuf::from))
        .or_else(|_| std::env::var("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("matlivre").join("matlivreprefs.mat")
}

fn load_prefs() -> R<Vec<(String, StructArray)>> {
    let path = pref_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let vars = crate::builtins_data::read_mat(&path.to_string_lossy())?;
    Ok(vars
        .into_iter()
        .filter_map(|(n, v)| match v {
            Some(Value::Struct(s)) => Some((n, s)),
            _ => None,
        })
        .collect())
}

fn save_prefs(groups: &[(String, StructArray)]) -> R<()> {
    let path = pref_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| LangError::new(format!("Unable to save preferences: {e}"), 0))?;
    }
    if groups.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    let vars: Vec<(String, Value)> = groups.iter().map(|(n, s)| (n.clone(), Value::Struct(s.clone()))).collect();
    crate::builtins_data::write_mat(&path.to_string_lossy(), &vars, false)
}

/// Preference names and values: a name and a value, or cell arrays of each.
fn pref_pairs(names: &Value, values: Option<&Value>) -> R<Vec<(String, Option<Value>)>> {
    let ns = texts(names)?;
    let vs: Vec<Option<Value>> = match values {
        None => vec![None; ns.len()],
        Some(Value::Cell(c)) if matches!(names, Value::Cell(_)) => c.data.iter().cloned().map(Some).collect(),
        Some(v) => vec![Some(v.clone())],
    };
    if vs.len() != ns.len() {
        return rt("PREF and VALUE must be the same size.");
    }
    Ok(ns.into_iter().zip(vs).collect())
}

fn prefs(name: &str, a: &[Value], nargout: usize) -> Ret {
    let mut groups = load_prefs()?;
    let group = arg(a, 0, name)?.text()?;
    let gi = groups.iter().position(|(n, _)| *n == group);
    match name {
        "addpref" | "setpref" => {
            let pairs = pref_pairs(arg(a, 1, name)?, Some(arg(a, 2, name)?))?;
            let k = match gi {
                Some(k) => k,
                None => {
                    groups.push((group.clone(), StructArray::scalar()));
                    groups.len() - 1
                }
            };
            for (p, v) in pairs {
                if name == "addpref" && groups[k].1.get(&p).is_some() {
                    return rt("A preference with that GROUP and NAME already exists.");
                }
                groups[k].1.set(&p, v.unwrap());
            }
            save_prefs(&groups)?;
            none()
        }
        "getpref" => {
            let Some(k) = gi else {
                if let (Some(p), Some(d)) = (a.get(1), a.get(2)) {
                    // A default creates the preference.
                    let mut s = StructArray::scalar();
                    for (n, v) in pref_pairs(p, Some(d))? {
                        s.set(&n, v.unwrap());
                    }
                    groups.push((group, s));
                    save_prefs(&groups)?;
                    return one(d.clone());
                }
                return rt(format!("group {group} does not exist"));
            };
            let Some(p) = a.get(1) else { return one(Value::Struct(groups[k].1.clone())) };
            let pairs = pref_pairs(p, a.get(2))?;
            let mut out = Vec::new();
            let mut changed = false;
            for (n, d) in &pairs {
                match (groups[k].1.get(n).cloned(), d) {
                    (Some(v), _) => out.push(v),
                    (None, Some(d)) => {
                        groups[k].1.set(n, d.clone());
                        changed = true;
                        out.push(d.clone());
                    }
                    (None, None) => return rt(format!("preference {n} does not exist in group {group}")),
                }
            }
            if changed {
                save_prefs(&groups)?;
            }
            let _ = nargout;
            one(if matches!(p, Value::Cell(_)) { Value::Cell(CellArray::row(out)) } else { out.remove(0) })
        }
        "ispref" => match a.get(1) {
            None => one(Value::bool(gi.is_some())),
            Some(p) => {
                let ns = texts(p)?;
                let tf: Vec<f64> = ns.iter().map(|n| if gi.is_some_and(|k| groups[k].1.get(n).is_some()) { 1.0 } else { 0.0 }).collect();
                if matches!(p, Value::Cell(_)) {
                    one(Value::Num(Matrix::row(tf).cast_to(Kind::Logical)))
                } else {
                    one(Value::bool(tf[0] != 0.0))
                }
            }
        },
        "rmpref" => {
            let Some(k) = gi else { return rt("GROUP and PREF must exist") };
            match a.get(1) {
                None => {
                    groups.remove(k);
                }
                Some(p) => {
                    let ns = texts(p)?;
                    if ns.iter().any(|n| groups[k].1.get(n).is_none()) {
                        return rt("GROUP and PREF must exist");
                    }
                    let s = &groups[k].1;
                    let keep: Vec<usize> = (0..s.fields.len()).filter(|&i| !ns.contains(&s.fields[i])).collect();
                    let mut ns2 = StructArray::scalar();
                    for i in keep {
                        ns2.set(&s.fields[i], s.elems[0][i].clone());
                    }
                    groups[k].1 = ns2;
                }
            }
            save_prefs(&groups)?;
            none()
        }
        _ => Ok(None),
    }
}

// ---- environment, clipboard, registry, perl ------------------------------------------------

/// KEY=VALUE lines of a .env file (comments and blank lines skipped, quotes removed).
fn parse_env(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((k, v)) = line.split_once('=') else { continue };
        let mut v = v.trim().to_string();
        if v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\''))) {
            v = v[1..v.len() - 1].to_string();
        } else if let Some(i) = v.find(" #") {
            v.truncate(i);
            v = v.trim().to_string();
        }
        out.push((k.trim().to_string(), v));
    }
    out
}

fn loadenv(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let path = arg(a, 0, "loadenv")?.text()?;
    let text = std::fs::read_to_string(&path).map_err(|_| LangError::new(format!("Unable to find file '{path}'."), 0))?;
    let pairs = parse_env(&text);
    if nargout == 0 {
        for (k, v) in &pairs {
            std::env::set_var(k, v);
        }
        return none();
    }
    let n = pairs.len();
    let keys = Value::Str(StrArray::from_dims(vec![n, 1], pairs.iter().map(|p| Some(p.0.clone())).collect()));
    let vals = Value::Str(StrArray::from_dims(vec![n, 1], pairs.iter().map(|p| Some(p.1.clone())).collect()));
    one(it.call_function("dictionary", vec![keys, vals], 1)?.remove(0))
}

fn clipboard(it: &mut Interpreter, a: &[Value]) -> Ret {
    let cmd = arg(a, 0, "clipboard")?.text()?.to_lowercase();
    let mut cb = arboard::Clipboard::new().map_err(|e| LangError::new(format!("The clipboard is not available: {e}"), 0))?;
    match cmd.as_str() {
        "copy" => {
            let v = arg(a, 1, "clipboard")?;
            let text = if v.is_text() { v.text()? } else { it.call_function("num2str", vec![v.clone()], 1)?.remove(0).text()? };
            cb.set_text(text).map_err(|e| LangError::new(format!("Unable to copy to the clipboard: {e}"), 0))?;
            none()
        }
        "paste" | "pastespecial" => one(Value::chars(&cb.get_text().unwrap_or_default())),
        other => rt(format!("'{other}' is not a valid clipboard command; use 'copy' or 'paste'.")),
    }
}

fn reg_root(root: &str) -> R<&'static str> {
    Ok(match root.to_uppercase().as_str() {
        "HKEY_CLASSES_ROOT" | "HKCR" => "HKCR",
        "HKEY_CURRENT_USER" | "HKCU" => "HKCU",
        "HKEY_LOCAL_MACHINE" | "HKLM" => "HKLM",
        "HKEY_USERS" | "HKU" => "HKU",
        "HKEY_CURRENT_CONFIG" | "HKCC" => "HKCC",
        _ => return rt(format!("'{root}' is not a valid registry root key.")),
    })
}

/// `winqueryreg(root, key, name)` and `winqueryreg('name', root, key)`, through `reg query`.
fn winqueryreg(a: &[Value]) -> Ret {
    if !cfg!(windows) {
        return rt("winqueryreg is only available on Windows.");
    }
    let first = arg(a, 0, "winqueryreg")?.text()?;
    let list = first.eq_ignore_ascii_case("name");
    let (root, key, value) = if list { (arg(a, 1, "winqueryreg")?.text()?, arg(a, 2, "winqueryreg")?.text()?, None) } else { (first, arg(a, 1, "winqueryreg")?.text()?, a.get(2).map(|v| v.text()).transpose()?) };
    let full = format!("{}\\{key}", reg_root(&root)?);
    let mut cmd = std::process::Command::new("reg");
    cmd.arg("query").arg(&full);
    match &value {
        Some(v) if !v.is_empty() => {
            cmd.arg("/v").arg(v);
        }
        Some(_) | None if !list => {
            cmd.arg("/ve");
        }
        _ => {}
    }
    let out = cmd.output().map_err(|e| LangError::new(format!("Unable to query the registry: {e}"), 0))?;
    if !out.status.success() {
        return rt("Specified key or value does not exist in the registry.");
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    // Value lines: four spaces, name, type, data (separated by runs of four spaces).
    let rows: Vec<(String, String, String)> = text
        .lines()
        .filter(|l| l.starts_with("    "))
        .filter_map(|l| {
            let parts: Vec<&str> = l.trim_start().splitn(3, "    ").collect();
            (parts.len() >= 2).then(|| (parts[0].to_string(), parts[1].to_string(), parts.get(2).map(|s| s.to_string()).unwrap_or_default()))
        })
        .collect();
    if list {
        return one(Value::Cell(CellArray::col(rows.iter().map(|r| Value::chars(&r.0)).collect())));
    }
    let Some((_, ty, data)) = rows.into_iter().next() else { return rt("Specified key or value does not exist in the registry.") };
    one(match ty.as_str() {
        "REG_DWORD" | "REG_QWORD" => {
            let n = i64::from_str_radix(data.trim_start_matches("0x"), 16).unwrap_or(0);
            Value::Num(Matrix::scalar(n as f64).cast_to(Kind::Int(IntClass::I32)))
        }
        _ => Value::chars(&data),
    })
}

fn perl(a: &[Value], nargout: usize) -> Ret {
    let file = arg(a, 0, "perl")?.text()?;
    if !std::path::Path::new(&file).exists() {
        return rt(format!("Unable to find Perl file: {file}"));
    }
    let args: Vec<String> = a[1..].iter().map(|v| v.text()).collect::<R<_>>()?;
    let out = std::process::Command::new("perl").arg(&file).args(&args).output().map_err(|_| LangError::new("Perl is not available on this system.", 0))?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    let status = out.status.code().unwrap_or(1) as f64;
    if nargout < 2 && status != 0.0 {
        return rt(format!("System error: {text}Command executed: perl {file}"));
    }
    Ok(Some(vec![Value::chars(&text), Value::num(status)]))
}

// ---- display text and scripts ----------------------------------------------------------------

fn formatted_display_text(it: &mut Interpreter, a: &[Value]) -> Ret {
    let v = arg(a, 0, "formattedDisplayText")?;
    let mut fmt = it.format;
    let mut true_false = false;
    let mut compact = false;
    for p in a[1..].chunks(2) {
        let k = p[0].text()?.to_lowercase();
        let val = p.get(1).ok_or_else(|| LangError::new("Name-value arguments must come in pairs.", 0))?;
        match k.as_str() {
            "numericformat" => fmt = if val.text()?.to_lowercase().starts_with("long") { NumFormat::Long } else { NumFormat::Short },
            "usetruefalseforlogical" => true_false = val.scalar()? != 0.0,
            "linespacing" => compact = val.text()?.eq_ignore_ascii_case("compact"),
            "suppressmarkup" => {}
            other => return rt(format!("'{other}' is not a recognized parameter.")),
        }
    }
    let _ = compact;
    let text = match v {
        Value::Num(m) if m.kind == Kind::Logical && true_false && m.is_scalar() => format!("    {}\n", if m.data[0] != 0.0 { "true" } else { "false" }),
        _ if v.is_text() => crate::display::disp_value(v, fmt),
        _ => format!("{}\n", crate::display::disp_value(v, fmt).trim_end_matches('\n')),
    };
    one(Value::Str(StrArray::scalar(text)))
}

/// MATLAB source for a value, or None when it cannot be written as a script.
fn script_for(name: &str, v: &Value, out: &mut String) -> bool {
    let num = |x: f64| -> String {
        if x.is_nan() {
            "NaN".into()
        } else if x.is_infinite() {
            if x > 0.0 { "Inf".into() } else { "-Inf".into() }
        } else {
            format!("{x}")
        }
    };
    match v {
        Value::Num(m) if m.kind == Kind::Char && m.rows() <= 1 => {
            let s = m.to_string_lossy();
            if s.chars().count() <= 64 {
                out.push_str(&format!("{name} = '{}';\n", s.replace('\'', "''")));
            } else {
                // Long text in 64-character pieces.
                let chars: Vec<char> = s.chars().collect();
                let pieces: Vec<String> = chars.chunks(64).map(|c| format!("'{}'", c.iter().collect::<String>().replace('\'', "''"))).collect();
                let indent = " ".repeat(name.len() + 4);
                out.push_str(&format!("{name} = [{}];\n", pieces.join(&format!(" ...\n{indent}"))));
            }
            true
        }
        Value::Num(m) if m.is_2d() && !m.is_complex() && m.kind != Kind::Char => {
            let body = if m.is_empty() {
                "[]".to_string()
            } else {
                let rows: Vec<String> = (0..m.rows()).map(|r| (0..m.cols()).map(|c| num(m.get(r, c))).collect::<Vec<_>>().join(" ")).collect();
                if m.numel() == 1 { rows[0].clone() } else { format!("[{}]", rows.join(";")) }
            };
            let wrapped = match m.kind {
                Kind::Double => body,
                Kind::Logical if m.numel() == 1 => (if m.data[0] != 0.0 { "true" } else { "false" }).to_string(),
                k => format!("{}({body})", k.name()),
            };
            out.push_str(&format!("{name} = {wrapped};\n"));
            true
        }
        Value::Str(s) if s.data.len() == 1 && s.data[0].is_some() => {
            let t = s.data[0].clone().unwrap();
            let parts: Vec<String> = t.split('\n').map(|p| format!("'{}'", p.replace('\'', "''"))).collect();
            if parts.len() == 1 {
                out.push_str(&format!("{name} = {};\n", parts[0]));
            } else {
                out.push_str(&format!("{name} = [{}];\n", parts.join(", newline, ")));
            }
            out.push_str(&format!("{name} = convertCharsToStrings({name});\n"));
            true
        }
        Value::Cell(c) if c.dims.len() == 2 && (c.dims[0] == 1 || c.dims[1] == 1) => {
            let mut body = format!("{name} = cell({}, {});\n", c.dims[0], c.dims[1]);
            for (i, x) in c.data.iter().enumerate() {
                if !script_for(&format!("{name}{{{}}}", i + 1), x, &mut body) {
                    return false;
                }
            }
            out.push_str(&body);
            true
        }
        Value::Struct(s) if s.elems.len() == 1 => {
            let mut body = format!("{name} = struct;\n");
            for (k, f) in s.fields.iter().enumerate() {
                if !script_for(&format!("{name}.{f}"), &s.elems[0][k], &mut body) {
                    return false;
                }
            }
            out.push_str(&body);
            true
        }
        _ => false,
    }
}

fn save_to_script(it: &mut Interpreter, a: &[Value]) -> Ret {
    let file = arg(a, 0, "matlab.io.saveVariablesToScript")?.text()?;
    let file = if file.ends_with(".m") { file } else { format!("{file}.m") };
    let names: Vec<String> = match a.get(1) {
        Some(v) if !(v.is_text() && v.text().is_ok_and(|t| t.eq_ignore_ascii_case("SaveMode") || t.eq_ignore_ascii_case("MATFileName"))) => texts(v)?,
        _ => {
            let w = it.call_function("who", Vec::new(), 1)?.remove(0);
            texts(&w)?
        }
    };
    let mut names = names;
    names.sort();
    let mat = file.trim_end_matches(".m").to_string() + ".mat";
    let mat_name = std::path::Path::new(&mat).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| mat.clone());
    let mut body = String::new();
    let mut leftovers = Vec::new();
    for n in &names {
        let Some(v) = it.get_var(n).cloned() else { continue };
        let mut text = String::new();
        let shape = format!("<{} {}>", crate::nd::size_text(&v.dims()).replace('×', "x"), v.class_name());
        if script_for(n, &v, &mut text) {
            body.push_str(&text);
        } else if crate::builtins_data::write_mat(&std::env::temp_dir().join("matlivre-probe.mat").to_string_lossy(), &[(n.clone(), v.clone())], false).is_ok() {
            body.push_str(&format!("{n} = saveVarsMat.{n}; % {shape} unsupported class\n"));
            leftovers.push((n.clone(), v));
        } else {
            // Neither a script nor this version's MAT-file writer can hold it.
            body.push_str(&format!("% {n}: {shape} could not be saved\n"));
        }
        body.push('\n');
    }
    let stamp = crate::time::format_datetime(crate::time::now_utc_ms() + crate::time::local_offset_ms(), "d-MMM-yyyy HH:mm:ss");
    let mut script = format!("% -------------------------------------------------------------------\n%  Generated by MatLivre on {stamp}\n% -------------------------------------------------------------------\n");
    if !leftovers.is_empty() {
        crate::builtins_data::write_mat(&mat, &leftovers, false)?;
        script.push_str(&format!("saveVarsMat = load('{mat_name}');\n\n"));
        it.emit(OutputKind::Warning(format!("The variables that could not be saved into MATLAB script are saved into MAT-file '{mat_name}'.")));
    }
    script.push_str(&body);
    if !leftovers.is_empty() {
        script.push_str("clear saveVarsMat;\n");
    }
    std::fs::write(&file, script).map_err(|e| LangError::new(format!("Unable to write file '{file}': {e}"), 0))?;
    none()
}

// ---- matfile -----------------------------------------------------------------------------------

fn matfile(a: &[Value]) -> Ret {
    let file = arg(a, 0, "matfile")?.text()?;
    let path = if std::path::Path::new(&file).extension().is_none() { format!("{file}.mat") } else { file };
    let mut writable = false;
    for p in a[1..].chunks(2) {
        if p[0].text()?.eq_ignore_ascii_case("Writable") {
            writable = p.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(0.0) != 0.0;
        }
    }
    let full = std::fs::canonicalize(&path).map(|p| p.to_string_lossy().trim_start_matches(r"\\?\").to_string()).unwrap_or_else(|_| path.clone());
    let vars: Vec<(String, Value)> = if std::path::Path::new(&path).exists() {
        crate::builtins_data::read_mat(&path)?.into_iter().filter_map(|(n, v)| v.map(|v| (n, v))).collect()
    } else if writable {
        Vec::new()
    } else {
        return rt(format!("Unable to find file or directory '{path}'."));
    };
    let mut props = StructArray::scalar();
    props.set("Source", Value::chars(&full));
    props.set("Writable", Value::bool(writable));
    props.set("ProtectedLoading", Value::bool(false));
    let mut names: Vec<&str> = vec!["Properties"];
    names.extend(vars.iter().map(|(n, _)| n.as_str()));
    let mut values = vec![Value::Struct(props)];
    values.extend(vars.iter().map(|(_, v)| v.clone()));
    one(object("matlab.io.MatFile", &names, values))
}

/// `m.var = value` on a MatFile object: writes the variable to the file when the object is
/// writable, and returns the object with the file's variables.
pub(crate) fn matfile_assign(o: &Object, var: &str, value: Value) -> R<Value> {
    let props = o.get("Properties").unwrap_or_else(Value::empty);
    let Value::Struct(p) = &props else { return rt("Invalid MatFile object.") };
    let writable = p.get("Writable").map(|v| v.is_true()).transpose()?.unwrap_or(false);
    if !writable {
        return rt(format!("Cannot change '{var}' because Properties.Writable is false.  To modify '{var}', set Properties.Writable to true."));
    }
    let path = p.get("Source").map(|v| v.text()).transpose()?.unwrap_or_default();
    let mut vars: Vec<(String, Value)> = if std::path::Path::new(&path).exists() {
        crate::builtins_data::read_mat(&path)?.into_iter().filter_map(|(n, v)| v.map(|v| (n, v))).collect()
    } else {
        Vec::new()
    };
    match vars.iter_mut().find(|(n, _)| n == var) {
        Some(slot) => slot.1 = value,
        None => vars.push((var.to_string(), value)),
    }
    crate::builtins_data::write_mat(&path, &vars, false)?;
    let r = matfile(&[Value::chars(&path), Value::chars("Writable"), Value::bool(true)])?;
    Ok(r.and_then(|v| v.into_iter().next()).unwrap_or_else(Value::empty))
}

// ---- import options ------------------------------------------------------------------------

fn opt_get(o: &Object, name: &str) -> Value {
    o.get(name).unwrap_or_else(Value::empty)
}

fn opts_object(values: HashMap<&str, Value>) -> Value {
    object(OPTS_CLASS, OPTS_PROPS, OPTS_PROPS.iter().map(|p| values.get(p).cloned().unwrap_or_else(Value::empty)).collect())
}

/// Per-variable options as structures (Name, Type, FillValue, TreatAsMissing, ...).
fn var_options(names: &[String], types: &[String]) -> Value {
    Value::Cell(CellArray::row(
        names
            .iter()
            .zip(types)
            .map(|(n, t)| {
                let mut s = StructArray::scalar();
                s.set("Name", Value::chars(n));
                s.set("Type", Value::chars(t));
                s.set("FillValue", if NUMERIC_TYPES.contains(&t.as_str()) { Value::num(f64::NAN) } else { Value::chars("") });
                s.set("TreatAsMissing", Value::Cell(CellArray::from_dims(vec![0, 0], Vec::new())));
                Value::Struct(s)
            })
            .collect(),
    ))
}

fn default_options(names: Vec<String>, types: Vec<String>, delimiter: &str, data_start: f64, names_line: f64, encoding: &str, trailing: &str) -> Value {
    let mut m: HashMap<&str, Value> = HashMap::new();
    let c = |s: &[&str]| Value::Cell(CellArray::row(s.iter().map(|x| Value::chars(x)).collect()));
    m.insert("Delimiter", c(&[delimiter]));
    m.insert("Whitespace", Value::chars("\\b\\t "));
    m.insert("LineEnding", c(&["\\n", "\\r", "\\r\\n"]));
    m.insert("CommentStyle", Value::Cell(CellArray::from_dims(vec![0, 0], Vec::new())));
    m.insert("ConsecutiveDelimitersRule", Value::chars("split"));
    m.insert("LeadingDelimitersRule", Value::chars("keep"));
    m.insert("TrailingDelimitersRule", Value::chars(trailing));
    m.insert("EmptyLineRule", Value::chars("skip"));
    m.insert("Encoding", Value::chars(encoding));
    m.insert("MissingRule", Value::chars("fill"));
    m.insert("ImportErrorRule", Value::chars("fill"));
    m.insert("ExtraColumnsRule", Value::chars("addvars"));
    m.insert("VariableOptions", var_options(&names, &types));
    m.insert("VariableNames", cellrow(&names));
    m.insert("VariableTypes", cellrow(&types));
    m.insert("SelectedVariableNames", cellrow(&names));
    m.insert("VariableNamingRule", Value::chars("modify"));
    m.insert("DataLines", Value::Num(Matrix::row(vec![data_start, f64::INFINITY])));
    m.insert("VariableNamesLine", Value::num(names_line));
    m.insert("RowNamesColumn", Value::num(0.0));
    m.insert("VariableUnitsLine", Value::num(0.0));
    m.insert("VariableDescriptionsLine", Value::num(0.0));
    opts_object(m)
}

/// Splits a line on the delimiters, honoring double quotes (removed).
fn split_fields(line: &str, delims: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            if in_q && chars.get(i + 1) == Some(&'"') {
                cur.push('"');
                i += 2;
                continue;
            }
            in_q = !in_q;
            i += 1;
            continue;
        }
        if !in_q {
            let rest: String = chars[i..].iter().collect();
            if let Some(d) = delims.iter().find(|d| !d.is_empty() && rest.starts_with(d.as_str())) {
                out.push(std::mem::take(&mut cur).trim().to_string());
                i += d.chars().count();
                continue;
            }
        }
        cur.push(c);
        i += 1;
    }
    out.push(cur.trim().to_string());
    out
}

fn delimiters(v: &Value) -> R<Vec<String>> {
    Ok(texts(v)?.into_iter().map(|d| d.replace("\\t", "\t")).collect())
}

fn detect(path: &str, a: &[Value]) -> R<Value> {
    let text = std::fs::read_to_string(path).map_err(|_| LangError::new(format!("Unable to find or open '{path}'. Check the path and filename or file permissions."), 0))?;
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let header = lines.first().copied().unwrap_or("");
    let mut delim = [",", "\t", ";", "|"].into_iter().max_by_key(|d| header.matches(d).count()).filter(|d| header.contains(d)).unwrap_or(",").to_string();
    for p in a.chunks(2) {
        if p[0].text().is_ok_and(|t| t.eq_ignore_ascii_case("Delimiter")) {
            if let Some(v) = p.get(1) {
                delim = texts(v)?.into_iter().next().unwrap_or(delim);
            }
        }
    }
    let delims = vec![delim.replace("\\t", "\t")];
    let rows: Vec<Vec<String>> = lines.iter().map(|l| split_fields(l, &delims)).collect();
    let is_num = |s: &str| s.is_empty() || s.parse::<f64>().is_ok() || matches!(s.to_lowercase().as_str(), "nan" | "inf" | "-inf");
    let first_numeric = rows.first().is_some_and(|r| r.iter().all(|f| !f.is_empty() && is_num(f)));
    let has_header = !first_numeric && rows.len() > 1;
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let body: &[Vec<String>] = if has_header { &rows[1..] } else { &rows[..] };
    let names: Vec<String> = (0..ncols)
        .map(|j| match rows.first().and_then(|r| r.get(j)) {
            Some(n) if has_header && !n.is_empty() => crate::builtins_data::make_valid_name(n),
            _ => format!("Var{}", j + 1),
        })
        .collect();
    let types: Vec<String> = (0..ncols)
        .map(|j| {
            let col: Vec<&str> = body.iter().filter_map(|r| r.get(j).map(|s| s.as_str())).collect();
            if col.iter().any(|s| !s.is_empty()) && col.iter().all(|s| is_num(s)) { "double".to_string() } else { "char".to_string() }
        })
        .collect();
    let delim_shown = if delim == "\t" { "\\t".to_string() } else { delim };
    Ok(default_options(names, types, &delim_shown, if has_header { 2.0 } else { 1.0 }, if has_header { 1.0 } else { 0.0 }, "UTF-8", "ignore"))
}

fn options_of(v: &Value) -> Option<&Object> {
    match v {
        Value::Object(o) if o.class.name == OPTS_CLASS => Some(o),
        _ => None,
    }
}

/// Variable positions selected by name(s), index(es) or all.
fn selection(o: &Object, v: Option<&Value>) -> R<Vec<usize>> {
    let names = texts(&opt_get(o, "VariableNames"))?;
    match v {
        None => Ok((0..names.len()).collect()),
        Some(v) if v.is_text() || matches!(v, Value::Cell(_) | Value::Str(_)) => texts(v)?
            .iter()
            .map(|n| names.iter().position(|m| m == n).ok_or_else(|| LangError::new(format!("Unknown variable name: '{n}'."), 0)))
            .collect(),
        Some(v) => Ok(v.to_matrix()?.data.iter().map(|&k| k as usize - 1).collect()),
    }
}

fn varopts_list(o: &Object) -> Vec<StructArray> {
    let names = texts(&opt_get(o, "VariableNames")).unwrap_or_default();
    let types = texts(&opt_get(o, "VariableTypes")).unwrap_or_default();
    let stored: Vec<StructArray> = match opt_get(o, "VariableOptions") {
        Value::Cell(c) => c.data.iter().filter_map(|v| if let Value::Struct(s) = v { Some(s.clone()) } else { None }).collect(),
        _ => Vec::new(),
    };
    // Kept in step with VariableNames and VariableTypes.
    names
        .iter()
        .enumerate()
        .map(|(k, n)| {
            let ty = types.get(k).cloned().unwrap_or_else(|| "char".into());
            let mut s = stored.get(k).cloned().unwrap_or_else(|| match var_options(std::slice::from_ref(n), std::slice::from_ref(&ty)) {
                Value::Cell(c) => match &c.data[0] {
                    Value::Struct(s) => s.clone(),
                    _ => unreachable!(),
                },
                _ => unreachable!(),
            });
            let old_ty = s.get("Type").and_then(|t| t.text().ok()).unwrap_or_default();
            if old_ty != ty {
                s.set("FillValue", if NUMERIC_TYPES.contains(&ty.as_str()) { Value::num(f64::NAN) } else { Value::chars("") });
            }
            s.set("Name", Value::chars(n));
            s.set("Type", Value::chars(&ty));
            s
        })
        .collect()
}

fn with_props(o: &Object, changes: &[(&str, Value)]) -> Value {
    let values: Vec<Value> = OPTS_PROPS.iter().map(|p| changes.iter().find(|(k, _)| k == p).map(|(_, v)| v.clone()).unwrap_or_else(|| opt_get(o, p))).collect();
    object(OPTS_CLASS, OPTS_PROPS, values)
}

fn varopts_object(s: &StructArray) -> Value {
    let get = |n: &str| s.get(n).cloned().unwrap_or_else(Value::empty);
    let ty = get("Type").text().unwrap_or_default();
    let empty_cell = || Value::Cell(CellArray::from_dims(vec![0, 0], Vec::new()));
    let base = [("Name", get("Name")), ("Type", get("Type")), ("FillValue", get("FillValue")), ("TreatAsMissing", get("TreatAsMissing")), ("QuoteRule", Value::chars("remove")), ("Prefixes", empty_cell()), ("Suffixes", empty_cell()), ("EmptyFieldRule", Value::chars("missing"))];
    let (class, extra): (&str, Vec<(&str, Value)>) = if NUMERIC_TYPES.contains(&ty.as_str()) {
        (
            "matlab.io.NumericVariableImportOptions",
            vec![
                ("ExponentCharacter", Value::chars("eEdD")),
                ("DecimalSeparator", Value::chars(".")),
                ("ThousandsSeparator", Value::chars("")),
                ("TrimNonNumeric", Value::bool(false)),
                ("NumberSystem", Value::chars("decimal")),
            ],
        )
    } else {
        ("matlab.io.TextVariableImportOptions", vec![("WhitespaceRule", Value::chars("trim"))])
    };
    let all: Vec<(&str, Value)> = base.into_iter().chain(extra).collect();
    let names: Vec<&str> = all.iter().map(|(k, _)| *k).collect();
    object(class, &names, all.into_iter().map(|(_, v)| v).collect())
}

/// One column of text fields as a variable of the given type.
fn convert(it: &mut Interpreter, fields: &[String], ty: &str, fill: &Value) -> R<Value> {
    let n = fields.len();
    if NUMERIC_TYPES.contains(&ty) || ty == "logical" {
        let f = fill.scalar().unwrap_or(f64::NAN);
        let data: Vec<f64> = fields
            .iter()
            .map(|s| match s.to_lowercase().as_str() {
                "" => f,
                "true" if ty == "logical" => 1.0,
                "false" if ty == "logical" => 0.0,
                t => t.parse::<f64>().unwrap_or(f),
            })
            .collect();
        let m = Matrix::col(data);
        return Ok(Value::Num(match ty {
            "double" => m,
            "logical" => m.cast_to(Kind::Logical),
            other => m.cast_to(Kind::from_name(other).unwrap_or(Kind::Double)),
        }));
    }
    let cells = || Value::Cell(CellArray::col(fields.iter().map(|s| Value::chars(s)).collect()));
    Ok(match ty {
        "string" => Value::Str(StrArray::from_dims(vec![n, 1], fields.iter().map(|s| if s.is_empty() { None } else { Some(s.clone()) }).collect())),
        "categorical" | "datetime" | "duration" => it.call_function(ty, vec![cells()], 1)?.remove(0),
        _ => cells(),
    })
}

/// The selected variables of a text file read with an options object.
fn read_with(it: &mut Interpreter, path: &str, o: &Object, max_rows: Option<usize>) -> R<(Vec<String>, Vec<Value>, Vec<StructArray>)> {
    let text = std::fs::read_to_string(path).map_err(|_| LangError::new(format!("Unable to find or open '{path}'. Check the path and filename or file permissions."), 0))?;
    let delims = delimiters(&opt_get(o, "Delimiter"))?;
    let lines: Vec<&str> = text.lines().collect();
    let dl = opt_get(o, "DataLines").to_matrix()?.data;
    let (start, stop) = (dl.first().copied().unwrap_or(1.0).max(1.0) as usize, dl.get(1).copied().unwrap_or(f64::INFINITY));
    let names = texts(&opt_get(o, "VariableNames"))?;
    let selected = texts(&opt_get(o, "SelectedVariableNames"))?;
    let vopts = varopts_list(o);
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (k, line) in lines.iter().enumerate() {
        let ln = k + 1;
        if ln < start || (ln as f64) > stop || line.trim().is_empty() {
            continue;
        }
        let mut f = split_fields(line, &delims);
        f.resize(names.len(), String::new());
        rows.push(f);
        if max_rows.is_some_and(|m| rows.len() >= m) {
            break;
        }
    }
    let mut vars = Vec::new();
    let mut used = Vec::new();
    for s in &selected {
        let j = names.iter().position(|n| n == s).ok_or_else(|| LangError::new(format!("Unknown variable name: '{s}'."), 0))?;
        let col: Vec<String> = rows.iter().map(|r| r[j].clone()).collect();
        let ty = vopts[j].get("Type").and_then(|t| t.text().ok()).unwrap_or_else(|| "char".into());
        let fill = vopts[j].get("FillValue").cloned().unwrap_or_else(Value::empty);
        vars.push(convert(it, &col, &ty, &fill)?);
        used.push(vopts[j].clone());
    }
    Ok((selected, vars, used))
}

/// `readtable`, `readmatrix`, `readcell` and `readtimetable` with an options object.
pub fn read_call(it: &mut Interpreter, name: &str, a: &[Value]) -> Ret {
    let Some(o) = a.get(1).and_then(options_of) else { return Ok(None) };
    let path = arg(a, 0, name)?.text()?;
    match name {
        "readtable" | "readtimetable" => {
            let (names, vars, _) = read_with(it, &path, o, None)?;
            let t = Value::Table(Box::new(Table::new(names, vars)?));
            if name == "readtimetable" {
                return Ok(Some(it.call_function("table2timetable", vec![t], 1)?));
            }
            one(t)
        }
        "readmatrix" => {
            // Every selected variable is read with the first one's options.
            let vopts = varopts_list(o);
            let selected = texts(&opt_get(o, "SelectedVariableNames"))?;
            let names = texts(&opt_get(o, "VariableNames"))?;
            let pos: Vec<usize> = selected.iter().filter_map(|s| names.iter().position(|n| n == s)).collect();
            let first = vopts[pos[0]].clone();
            let same = |s: &StructArray| s.get("FillValue") == first.get("FillValue") && s.get("Type") == first.get("Type");
            if pos.iter().any(|&p| !same(&vopts[p])) {
                it.emit(OutputKind::Warning("Different 'VariableOptions' detected for the selected variables. Importing data using options defined in the first selected variable. \n To import the selected variables using different variable options, use readtable".into()));
            }
            let mut forced = Vec::new();
            for &p in &pos {
                let mut s = first.clone();
                s.set("Name", Value::chars(&names[p]));
                forced.push(Value::Struct(s));
            }
            let mut all = varopts_list(o);
            for (i, &p) in pos.iter().enumerate() {
                if let Value::Struct(s) = &forced[i] {
                    all[p] = s.clone();
                }
            }
            let types: Vec<String> = all.iter().map(|s| s.get("Type").and_then(|t| t.text().ok()).unwrap_or_default()).collect();
            let o2 = with_props(o, &[("VariableOptions", Value::Cell(CellArray::row(all.into_iter().map(Value::Struct).collect()))), ("VariableTypes", cellrow(&types))]);
            let Value::Object(o2) = o2 else { unreachable!() };
            let (_, vars, _) = read_with(it, &path, &o2, None)?;
            one(crate::indexing::hconcat(vars)?)
        }
        _ => {
            let (_, vars, _) = read_with(it, &path, o, None)?;
            let n = vars.first().map(|v| v.dims()[0]).unwrap_or(0);
            let mut data = Vec::with_capacity(n * vars.len());
            for v in &vars {
                for r in 0..n {
                    let x = crate::indexing::index(v, &[crate::nd::IndexArg::scalar(r)])?;
                    data.push(match x {
                        Value::Cell(c) => c.data[0].clone(),
                        other => other,
                    });
                }
            }
            one(Value::Cell(CellArray::new(n, vars.len(), data)))
        }
    }
}

/// The display of an import options object or a variable's options.
pub fn display(name: &str, v: &Value) -> Option<String> {
    let Value::Object(o) = v else { return None };
    let cls = o.class.name.as_str();
    if cls == "matlab.io.MatFile" {
        let Some(Value::Struct(p)) = o.get("Properties") else { return None };
        let get = |k: &str| p.get(k).cloned().unwrap_or_else(Value::empty);
        let mut rows: Vec<(String, String)> = vec![
            ("Properties.Source".into(), format!("'{}'", get("Source").text().unwrap_or_default())),
            ("Properties.Writable".into(), if get("Writable").scalar().unwrap_or(0.0) != 0.0 { "true".into() } else { "false".into() }),
            ("Properties.ProtectedLoading".into(), "false".into()),
        ];
        for prop in o.class.props.iter().skip(1) {
            let val = o.get(&prop.name).unwrap_or_else(Value::empty);
            rows.push((prop.name.clone(), format!("[{} {}]", crate::nd::size_text(&val.dims()).replace('×', "x"), val.class_name())));
        }
        let w = rows.iter().map(|r| r.0.len()).max().unwrap_or(0) + 4;
        let body: Vec<String> = rows.iter().map(|(k, v)| format!("{k:>w$}: {v}")).collect();
        return Some(format!("{name} = \n  matlab.io.MatFile\n\n  Properties:\n{}", body.join("\n")));
    }
    let show = |v: &Value, commas: bool| -> String {
        match v {
            Value::Cell(c) if c.data.is_empty() => "{}".into(),
            Value::Cell(c) => {
                let items: Vec<String> = c.data.iter().map(|x| format!("'{}'", x.text().unwrap_or_default())).collect();
                format!("{{{}}}", items.join(if commas { ", " } else { "  " }))
            }
            Value::Num(m) if m.kind == Kind::Char => format!("'{}'", m.to_string_lossy()),
            Value::Num(m) if m.kind == Kind::Logical => format!("{}", m.data.first().copied().unwrap_or(0.0)),
            Value::Num(m) if m.numel() == 1 => crate::display::fmt_g(m.data[0], 5).replace("nan", "NaN"),
            Value::Num(m) => format!("[{}]", m.data.iter().map(|x| if x.is_infinite() { "Inf".to_string() } else { crate::display::fmt_g(*x, 5) }).collect::<Vec<_>>().join(" ")),
            _ => String::new(),
        }
    };
    let field = |out: &mut String, k: &str, w: usize| {
        let val = opt_get(o, k);
        let commas = matches!(k, "VariableNames" | "VariableTypes" | "SelectedVariableNames");
        out.push_str(&format!("{:>w$}: {}\n", k, show(&val, commas)));
    };
    if cls == OPTS_CLASS {
        let n = texts(&opt_get(o, "VariableNames")).map(|v| v.len()).unwrap_or(0);
        let mut s = format!("{name} = \n  DelimitedTextImportOptions with properties:\n\n   Format Properties:\n");
        for k in ["Delimiter", "Whitespace", "LineEnding", "CommentStyle", "ConsecutiveDelimitersRule", "LeadingDelimitersRule", "TrailingDelimitersRule", "EmptyLineRule", "Encoding"] {
            field(&mut s, k, 29);
        }
        s.push_str("\n   Replacement Properties:\n");
        for k in ["MissingRule", "ImportErrorRule", "ExtraColumnsRule"] {
            field(&mut s, k, 29);
        }
        s.push_str("\n   Variable Import Properties: Set types by name using setvartype\n");
        for k in ["VariableNames", "VariableTypes", "SelectedVariableNames"] {
            field(&mut s, k, 29);
        }
        s.push_str(&format!("{:>29}: [1-by-{n} matlab.io.VariableImportOptions] \n\tAccess VariableOptions sub-properties using setvaropts/getvaropts\n", "VariableOptions"));
        field(&mut s, "VariableNamingRule", 29);
        s.push_str("\n   Location Properties:\n");
        for k in ["DataLines", "VariableNamesLine", "RowNamesColumn", "VariableUnitsLine"] {
            field(&mut s, k, 29);
        }
        s.push_str(&format!("{:>29}: {} \n\tTo display a preview of the table, use preview", "VariableDescriptionsLine", show(&opt_get(o, "VariableDescriptionsLine"), false)));
        return Some(s);
    }
    let short = cls.strip_prefix("matlab.io.")?;
    if !short.ends_with("VariableImportOptions") {
        return None;
    }
    let numeric = short.starts_with("Numeric");
    let w = if numeric { 22 } else { 18 };
    let mut s = format!("{name} = \n  {short} with properties:\n\n   Variable Properties:\n");
    for k in ["Name", "Type", "FillValue", "TreatAsMissing", "QuoteRule", "Prefixes", "Suffixes", "EmptyFieldRule"] {
        field(&mut s, k, w);
    }
    if numeric {
        s.push_str("\n   Numeric Options:\n");
        for k in ["ExponentCharacter", "DecimalSeparator", "ThousandsSeparator", "TrimNonNumeric", "NumberSystem"] {
            field(&mut s, k, w);
        }
    } else {
        s.push_str("\n   String Options:\n");
        field(&mut s, "WhitespaceRule", w);
    }
    Some(s.trim_end_matches('\n').to_string())
}

fn import_options(it: &mut Interpreter, name: &str, a: &[Value]) -> Ret {
    match name {
        "detectImportOptions" => {
            let path = arg(a, 0, name)?.text()?;
            one(detect(&path, &a[1..])?)
        }
        "delimitedTextImportOptions" => {
            let mut n = 0usize;
            let mut names: Option<Vec<String>> = None;
            let mut types: Option<Vec<String>> = None;
            let mut delim = ",".to_string();
            for p in a.chunks(2) {
                let k = p[0].text()?.to_lowercase();
                let v = p.get(1).ok_or_else(|| LangError::new("Name-value arguments must come in pairs.", 0))?;
                match k.as_str() {
                    "numvariables" => n = v.as_usize()?,
                    "variablenames" => names = Some(texts(v)?),
                    "variabletypes" => types = Some(texts(v)?),
                    "delimiter" => delim = texts(v)?.into_iter().next().unwrap_or(delim),
                    _ => {}
                }
            }
            let n = names.as_ref().map(|v| v.len()).unwrap_or(n);
            let names = names.unwrap_or_else(|| (1..=n).map(|i| format!("Var{i}")).collect());
            let types = types.unwrap_or_else(|| vec!["char".to_string(); n]);
            one(default_options(names, types, &delim, 1.0, 0.0, "", "keep"))
        }
        "getvaropts" => {
            let o = options_of(arg(a, 0, name)?).ok_or_else(|| LangError::new("The first input must be an import options object.", 0))?;
            let list = varopts_list(o);
            let sel = selection(o, a.get(1))?;
            let objs: Vec<Value> = sel.iter().map(|&k| varopts_object(&list[k])).collect();
            one(if objs.len() == 1 { objs.into_iter().next().unwrap() } else { Value::Cell(CellArray::row(objs)) })
        }
        "setvaropts" | "setvartype" => {
            let o = options_of(arg(a, 0, name)?).ok_or_else(|| LangError::new("The first input must be an import options object.", 0))?;
            // setvartype(opts, [sel], type); setvaropts(opts, [sel], Name, Value, ...).
            let rest = &a[1..];
            let has_sel = if name == "setvartype" { rest.len() == 2 } else { rest.len() % 2 == 1 };
            let sel = selection(o, if has_sel { rest.first() } else { None })?;
            let pairs = &rest[usize::from(has_sel)..];
            let mut list = varopts_list(o);
            let mut types = texts(&opt_get(o, "VariableTypes"))?;
            if name == "setvartype" {
                let ty = arg(pairs, 0, name)?.text()?;
                for &k in &sel {
                    types[k] = ty.clone();
                    list[k].set("Type", Value::chars(&ty));
                    list[k].set("FillValue", if NUMERIC_TYPES.contains(&ty.as_str()) { Value::num(f64::NAN) } else { Value::chars("") });
                }
            } else {
                for p in pairs.chunks(2) {
                    let key = p[0].text()?;
                    let v = p.get(1).ok_or_else(|| LangError::new("Name-value arguments must come in pairs.", 0))?;
                    for &k in &sel {
                        if key.eq_ignore_ascii_case("Type") {
                            types[k] = v.text()?;
                        }
                        list[k].set(&key, v.clone());
                    }
                }
            }
            one(with_props(o, &[("VariableOptions", Value::Cell(CellArray::row(list.into_iter().map(Value::Struct).collect()))), ("VariableTypes", cellrow(&types))]))
        }
        "preview" => {
            let path = arg(a, 0, name)?.text()?;
            let o = options_of(arg(a, 1, name)?).ok_or_else(|| LangError::new("The second input must be an import options object.", 0))?;
            let (names, vars, _) = read_with(it, &path, o, Some(8))?;
            one(Value::Table(Box::new(Table::new(names, vars)?)))
        }
        _ => Ok(None),
    }
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "addpref" | "getpref" | "setpref" | "ispref" | "rmpref" => prefs(name, a, nargout),
        "loadenv" => loadenv(it, a, nargout),
        "clipboard" => clipboard(it, a),
        "formattedDisplayText" => formatted_display_text(it, a),
        "winqueryreg" => winqueryreg(a),
        "perl" => perl(a, nargout),
        "matlab.io.saveVariablesToScript" => save_to_script(it, a),
        "matfile" => matfile(a),
        "detectImportOptions" | "delimitedTextImportOptions" | "getvaropts" | "setvaropts" | "setvartype" | "preview" => import_options(it, name, a),
        _ => Ok(None),
    }
}
