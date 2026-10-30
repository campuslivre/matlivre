//! `readstruct` and `writestruct`: structs to and from XML and JSON files.
//!
//! XML: the root element is the struct; child elements become fields (repeated elements a
//! row of values or a struct array), attributes become `<name>Attribute` fields and the text
//! of an element that also has attributes or children goes in `Text`. Text that reads as a
//! number becomes a double, anything else a string. JSON maps objects to structs, arrays of
//! numbers to row vectors and arrays of objects to struct arrays.

use crate::builtins::{arg, none, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["readstruct", "writestruct"];

fn err(e: impl std::fmt::Display) -> LangError {
    LangError::new(e.to_string(), 0)
}

fn file_type(path: &str, a: &[Value], from: usize) -> R<String> {
    let mut i = from;
    while i + 1 < a.len() {
        if a[i].text()?.eq_ignore_ascii_case("FileType") {
            return Ok(a[i + 1].text()?.to_lowercase());
        }
        i += 2;
    }
    let ext = std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "json" => Ok("json".into()),
        "xml" => Ok("xml".into()),
        _ => rt(format!("Unable to determine the file type of \"{path}\". Specify the 'FileType' name-value argument.")),
    }
}

fn option(a: &[Value], from: usize, name: &str) -> R<Option<String>> {
    let mut i = from;
    while i + 1 < a.len() {
        if a[i].text()?.eq_ignore_ascii_case(name) {
            return Ok(Some(a[i + 1].text()?));
        }
        i += 2;
    }
    Ok(None)
}

/// A text value: a double when it parses as a number, otherwise a string.
fn scalar_text(s: &str) -> Value {
    let t = s.trim();
    match t.parse::<f64>() {
        Ok(x) if !t.is_empty() && !t.eq_ignore_ascii_case("nan") && !t.eq_ignore_ascii_case("inf") && !t.eq_ignore_ascii_case("infinity") => Value::num(x),
        _ => Value::string(t),
    }
}

/// Values gathered for one field: a row vector of numbers, a string row, or a struct row.
fn combine(vals: Vec<Value>) -> Value {
    if vals.len() == 1 {
        return vals.into_iter().next().unwrap();
    }
    if vals.iter().all(|v| matches!(v, Value::Num(m) if m.numel() == 1)) {
        let data = vals.iter().map(|v| v.to_matrix().unwrap().data[0]).collect::<Vec<_>>();
        return Value::Num(Matrix::new(1, data.len(), data));
    }
    if vals.iter().all(|v| matches!(v, Value::Str(s) if s.data.len() == 1)) {
        let data = vals.iter().map(|v| if let Value::Str(s) = v { s.data[0].clone() } else { None }).collect::<Vec<_>>();
        return Value::Str(StrArray::from_dims(vec![1, data.len()], data));
    }
    // Mixed or structured: one struct per value (plain values in Text).
    let structs: Vec<StructArray> = vals
        .into_iter()
        .map(|v| match v {
            Value::Struct(s) if s.numel() == 1 => s,
            other => {
                let mut s = StructArray::scalar();
                s.set("Text", other);
                s
            }
        })
        .collect();
    let mut fields: Vec<String> = Vec::new();
    for s in &structs {
        for f in &s.fields {
            if !fields.contains(f) {
                fields.push(f.clone());
            }
        }
    }
    let elems = structs
        .iter()
        .map(|s| fields.iter().map(|f| s.get(f).cloned().unwrap_or_else(|| Value::Str(StrArray::from_dims(vec![1, 1], vec![None])))).collect())
        .collect::<Vec<_>>();
    Value::Struct(StructArray { dims: vec![1, elems.len()], fields, elems })
}

fn xml_node(n: roxmltree::Node) -> Value {
    let kids: Vec<_> = n.children().filter(|c| c.is_element()).collect();
    let text: String = n.children().filter(|c| c.is_text()).map(|c| c.text().unwrap_or("")).collect::<String>();
    if kids.is_empty() && n.attributes().len() == 0 {
        return scalar_text(&text);
    }
    let mut s = StructArray::scalar();
    for at in n.attributes() {
        s.set(&format!("{}Attribute", at.name()), scalar_text(at.value()));
    }
    let mut order: Vec<&str> = Vec::new();
    for k in &kids {
        if !order.contains(&k.tag_name().name()) {
            order.push(k.tag_name().name());
        }
    }
    for name in order {
        let vals = kids.iter().filter(|k| k.tag_name().name() == name).map(|k| xml_node(*k)).collect();
        s.set(name, combine(vals));
    }
    if !text.trim().is_empty() {
        s.set("Text", scalar_text(&text));
    }
    Value::Struct(s)
}

fn json_node(j: &serde_json::Value) -> Value {
    match j {
        serde_json::Value::Null => Value::Num(Matrix::new(0, 0, Vec::new())),
        serde_json::Value::Bool(b) => Value::bool(*b),
        serde_json::Value::Number(n) => Value::num(n.as_f64().unwrap_or(f64::NAN)),
        serde_json::Value::String(s) => Value::string(s.as_str()),
        serde_json::Value::Array(items) if items.iter().all(|i| i.is_boolean()) && !items.is_empty() => {
            let data = items.iter().map(|i| if i.as_bool() == Some(true) { 1.0 } else { 0.0 }).collect::<Vec<_>>();
            Value::Num(Matrix::new(1, data.len(), data).cast_to(Kind::Logical))
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                return Value::Num(Matrix::new(0, 0, Vec::new()));
            }
            combine(items.iter().map(json_node).collect())
        }
        serde_json::Value::Object(map) => {
            let mut s = StructArray::scalar();
            for (k, v) in map {
                s.set(&crate::builtins_data::make_valid_name(k), json_node(v));
            }
            Value::Struct(s)
        }
    }
}

fn readstruct(a: &[Value]) -> Ret {
    let path = arg(a, 0, "readstruct")?.text()?;
    let text = std::fs::read_to_string(&path).map_err(|_| LangError::new(format!("Unable to find or open '{path}'. Check the path and filename or file permissions."), 0))?;
    match file_type(&path, a, 1)?.as_str() {
        "json" => {
            let j: serde_json::Value = serde_json::from_str(&text).map_err(err)?;
            one(json_node(&j))
        }
        _ => {
            let doc = roxmltree::Document::parse(&text).map_err(err)?;
            let v = xml_node(doc.root_element());
            one(match v {
                Value::Struct(_) => v,
                other => {
                    let mut s = StructArray::scalar();
                    s.set("Text", other);
                    Value::Struct(s)
                }
            })
        }
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Text of each element of a non-struct value.
fn leaf_texts(v: &Value) -> R<Vec<String>> {
    Ok(match v {
        Value::Str(s) => s.data.iter().map(|x| x.clone().unwrap_or_default()).collect(),
        Value::Num(m) if m.kind == Kind::Char => vec![v.text()?],
        Value::Num(m) if m.kind == Kind::Logical => m.data.iter().map(|&x| (if x != 0.0 { "true" } else { "false" }).to_string()).collect(),
        Value::Num(m) if m.is_complex() => return rt("Complex values cannot be written to a structure file."),
        Value::Num(m) => {
            // Row-major order, as the values read back as a row.
            let (r, c) = (m.rows(), m.cols());
            (0..r).flat_map(|i| (0..c).map(move |j| (i, j))).map(|(i, j)| format!("{}", m.data[j * r + i])).collect()
        }
        Value::Cell(c) => {
            let mut out = Vec::new();
            for x in &c.data {
                out.extend(leaf_texts(x)?);
            }
            out
        }
        _ => return rt("Unsupported value type for writestruct."),
    })
}

fn xml_struct(out: &mut String, name: &str, s: &StructArray, i: usize, depth: usize) -> R<()> {
    let pad = "    ".repeat(depth);
    let mut attrs = String::new();
    let mut body = Vec::new();
    let mut text = None;
    for f in &s.fields {
        let v = s.get_at(i, f).unwrap();
        if let Some(a) = f.strip_suffix("Attribute").filter(|a| !a.is_empty() && !matches!(v, Value::Struct(_))) {
            attrs.push_str(&format!(" {a}=\"{}\"", escape(&leaf_texts(v)?.join(" "))));
        } else if f == "Text" && !matches!(v, Value::Struct(_)) {
            text = Some(leaf_texts(v)?.join(" "));
        } else {
            body.push((f, v));
        }
    }
    if body.is_empty() {
        match text {
            Some(t) => out.push_str(&format!("{pad}<{name}{attrs}>{}</{name}>\n", escape(&t))),
            None => out.push_str(&format!("{pad}<{name}{attrs}/>\n")),
        }
        return Ok(());
    }
    out.push_str(&format!("{pad}<{name}{attrs}>\n"));
    if let Some(t) = text {
        out.push_str(&format!("{pad}    {}\n", escape(&t)));
    }
    for (f, v) in body {
        xml_value(out, f, v, depth + 1)?;
    }
    out.push_str(&format!("{pad}</{name}>\n"));
    Ok(())
}

fn xml_value(out: &mut String, name: &str, v: &Value, depth: usize) -> R<()> {
    match v {
        Value::Struct(s) => {
            for i in 0..s.numel() {
                xml_struct(out, name, s, i, depth)?;
            }
        }
        other => {
            let pad = "    ".repeat(depth);
            let texts = leaf_texts(other)?;
            if texts.is_empty() {
                out.push_str(&format!("{pad}<{name}/>\n"));
            }
            for t in texts {
                out.push_str(&format!("{pad}<{name}>{}</{name}>\n", escape(&t)));
            }
        }
    }
    Ok(())
}

fn json_value(v: &Value) -> R<serde_json::Value> {
    use serde_json::Value as J;
    let num = |x: f64| serde_json::Number::from_f64(x).map(J::Number).unwrap_or(J::Null);
    Ok(match v {
        Value::Struct(s) => {
            let obj = |i: usize| -> R<J> {
                let mut m = serde_json::Map::new();
                for f in &s.fields {
                    m.insert(f.clone(), json_value(s.get_at(i, f).unwrap())?);
                }
                Ok(J::Object(m))
            };
            if s.numel() == 1 { obj(0)? } else { J::Array((0..s.numel()).map(obj).collect::<R<_>>()?) }
        }
        Value::Str(s) if s.data.len() == 1 => s.data[0].clone().map(J::String).unwrap_or(J::Null),
        Value::Str(s) => J::Array(s.data.iter().map(|x| x.clone().map(J::String).unwrap_or(J::Null)).collect()),
        Value::Num(m) if m.kind == Kind::Char => J::String(v.text()?),
        Value::Num(m) if m.is_complex() => return rt("Complex values cannot be written to a structure file."),
        Value::Num(m) => {
            let item = |x: f64| if m.kind == Kind::Logical { J::Bool(x != 0.0) } else { num(x) };
            let (r, c) = (m.rows(), m.cols());
            if m.numel() == 1 {
                item(m.data[0])
            } else if r <= 1 || c <= 1 {
                J::Array(m.data.iter().map(|&x| item(x)).collect())
            } else {
                J::Array((0..r).map(|i| J::Array((0..c).map(|j| item(m.data[j * r + i])).collect())).collect())
            }
        }
        Value::Cell(c) => J::Array(c.data.iter().map(json_value).collect::<R<_>>()?),
        _ => return rt("Unsupported value type for writestruct."),
    })
}

fn writestruct(a: &[Value]) -> Ret {
    let s = match arg(a, 0, "writestruct")? {
        Value::Struct(s) => s.clone(),
        _ => return rt("Input must be a scalar structure."),
    };
    let path = arg(a, 1, "writestruct")?.text()?;
    let text = match file_type(&path, a, 2)?.as_str() {
        "json" => {
            let j = json_value(&Value::Struct(s))?;
            let mut buf = Vec::new();
            let fmt = serde_json::ser::PrettyFormatter::with_indent(b"    ");
            let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
            serde::Serialize::serialize(&j, &mut ser).map_err(err)?;
            String::from_utf8(buf).map_err(err)?
        }
        _ => {
            let root = option(a, 2, "StructNodeName")?.unwrap_or_else(|| "struct".into());
            let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
            xml_struct(&mut out, &root, &s, 0, 0)?;
            out.pop();
            out
        }
    };
    std::fs::write(&path, text).map_err(err)?;
    none()
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    match name {
        "readstruct" => readstruct(a),
        "writestruct" => writestruct(a),
        _ => Ok(None),
    }
}
