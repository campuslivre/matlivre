//! JSON (`jsondecode`, `jsonencode`) and MAT-files (`load`, `save`).
//!
//! MAT-files use the documented Level 5 format (also used by -v6 and -v7 files): numeric,
//! logical, char, cell and struct variables, compressed (miCOMPRESSED) or not. Sparse arrays
//! load as full matrices; objects such as strings, datetimes and tables saved by MATLAB are
//! skipped with a warning. `save` writes compressed Level 5 files, which MATLAB reads.

use std::io::{Read, Write};

use crate::builtins::{arg, none, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["jsondecode", "jsonencode", "load", "save"];

// ---- JSON --------------------------------------------------------------------------------

/// matlab.lang.makeValidName: whitespace removed with the next letter capitalised, other
/// invalid characters replaced by '_', and an 'x' prefix when needed.
pub fn make_valid_name(s: &str) -> String {
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
    if !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out = format!("x{out}");
    }
    out.truncate(63);
    out
}

fn json_to_value(j: &serde_json::Value) -> Value {
    use serde_json::Value as J;
    match j {
        J::Null => Value::empty(),
        J::Bool(b) => Value::bool(*b),
        J::Number(n) => Value::num(n.as_f64().unwrap_or(f64::NAN)),
        J::String(s) => Value::chars(s),
        J::Object(map) => {
            let mut s = StructArray::scalar();
            for (k, v) in map {
                s.set(&make_valid_name(k), json_to_value(v));
            }
            Value::Struct(s)
        }
        J::Array(items) => {
            if items.is_empty() {
                return Value::empty();
            }
            // Numbers (and nulls among numbers) become a column.
            if items.iter().all(|x| x.is_number() || x.is_null()) && items.iter().any(|x| x.is_number()) {
                return Value::Num(Matrix::col(items.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()));
            }
            if items.iter().all(|x| x.is_boolean()) {
                return Value::Num(Matrix::col(items.iter().map(|x| if x.as_bool() == Some(true) { 1.0 } else { 0.0 }).collect()).cast_to(Kind::Logical));
            }
            // Arrays of equal-length number arrays become a matrix, one row each.
            if let Some(J::Array(first)) = items.first() {
                let w = first.len();
                if w > 0 && items.iter().all(|x| matches!(x, J::Array(r) if r.len() == w && r.iter().all(|v| v.is_number()))) {
                    let n = items.len();
                    let mut data = vec![0.0; n * w];
                    for (i, row) in items.iter().enumerate() {
                        if let J::Array(r) = row {
                            for (k, v) in r.iter().enumerate() {
                                data[k * n + i] = v.as_f64().unwrap_or(f64::NAN);
                            }
                        }
                    }
                    return Value::Num(Matrix::new(n, w, data));
                }
            }
            // Objects with the same field names become a struct array.
            if let Some(J::Object(first)) = items.first() {
                let keys: Vec<&String> = first.keys().collect();
                if items.iter().all(|x| matches!(x, J::Object(o) if o.keys().collect::<Vec<_>>() == keys)) {
                    let fields: Vec<String> = keys.iter().map(|k| make_valid_name(k)).collect();
                    let mut s = StructArray::empty_with(fields);
                    for x in items {
                        if let J::Object(o) = x {
                            s.elems.push(o.values().map(json_to_value).collect());
                        }
                    }
                    s.dims = vec![items.len(), 1];
                    return Value::Struct(s);
                }
            }
            Value::Cell(CellArray::col(items.iter().map(json_to_value).collect()))
        }
    }
}

fn json_number(x: f64) -> serde_json::Value {
    if !x.is_finite() {
        return serde_json::Value::Null;
    }
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return serde_json::Value::from(x as i64);
    }
    // Up to 15 significant digits, as MATLAB writes them (3.14159, not 3.1415899999999999).
    let s = crate::display::fmt_g(x, 15);
    serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)
}

fn value_to_json(v: &Value) -> R<serde_json::Value> {
    use serde_json::Value as J;
    Ok(match v {
        Value::Num(m) if m.kind == Kind::Char => J::String(m.to_string_lossy()),
        Value::Num(m) => {
            let elem = |x: f64| if m.kind == Kind::Logical { J::Bool(x != 0.0) } else { json_number(x) };
            if m.is_empty() {
                J::Array(Vec::new())
            } else if m.numel() == 1 {
                elem(m.data[0])
            } else if m.is_vector() {
                J::Array(m.data.iter().map(|&x| elem(x)).collect())
            } else {
                let (r, c) = (m.rows(), m.cols());
                J::Array((0..r).map(|i| J::Array((0..c).map(|j| elem(m.data[j * r + i])).collect())).collect())
            }
        }
        Value::Str(s) if s.data.len() == 1 => s.data[0].clone().map(J::String).unwrap_or(J::Null),
        Value::Str(s) => J::Array(s.data.iter().map(|t| t.clone().map(J::String).unwrap_or(J::Null)).collect()),
        Value::Cell(c) => J::Array(c.data.iter().map(value_to_json).collect::<R<_>>()?),
        Value::Struct(s) => {
            let obj = |i: usize| -> R<J> {
                let mut map = serde_json::Map::new();
                for (k, f) in s.fields.iter().enumerate() {
                    map.insert(f.clone(), value_to_json(&s.elems[i][k])?);
                }
                Ok(J::Object(map))
            };
            if s.elems.len() == 1 { obj(0)? } else { J::Array((0..s.elems.len()).map(obj).collect::<R<_>>()?) }
        }
        Value::Time(_) | Value::Table(_) => J::String(crate::display::summary(v, crate::display::NumFormat::Short)),
        other => return rt(format!("Unable to encode values of class '{}' as JSON.", other.class_name())),
    })
}

// ---- MAT-files ---------------------------------------------------------------------------

const MI_INT8: u32 = 1;
const MI_UINT8: u32 = 2;
const MI_INT16: u32 = 3;
const MI_UINT16: u32 = 4;
const MI_INT32: u32 = 5;
const MI_UINT32: u32 = 6;
const MI_SINGLE: u32 = 7;
const MI_DOUBLE: u32 = 9;
const MI_INT64: u32 = 12;
const MI_UINT64: u32 = 13;
const MI_MATRIX: u32 = 14;
const MI_COMPRESSED: u32 = 15;
const MI_UTF8: u32 = 16;
const MI_UTF16: u32 = 17;

const MX_CELL: u8 = 1;
const MX_STRUCT: u8 = 2;
const MX_CHAR: u8 = 4;
const MX_SPARSE: u8 = 5;
const MX_DOUBLE: u8 = 6;

fn bad(path: &str) -> LangError {
    LangError::new(format!("Unable to read MAT-file {path}. Not a binary MAT-file. Try load -ASCII to read as text."), 0)
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u32(&mut self) -> Option<u32> {
        let v = u32::from_le_bytes(self.b.get(self.pos..self.pos + 4)?.try_into().ok()?);
        self.pos += 4;
        Some(v)
    }

    /// One data element: (type, payload).
    fn element(&mut self) -> Option<(u32, &'a [u8])> {
        let tag = self.u32()?;
        // Small element: size in the upper 16 bits, data in the next 4 bytes.
        if tag >> 16 != 0 {
            let (ty, n) = (tag & 0xffff, (tag >> 16) as usize);
            let data = self.b.get(self.pos..self.pos + n)?;
            self.pos += 4;
            return Some((ty, data));
        }
        let n = self.u32()? as usize;
        let data = self.b.get(self.pos..self.pos + n)?;
        self.pos += n;
        if self.pos % 8 != 0 && tag != MI_COMPRESSED {
            self.pos += 8 - self.pos % 8;
        }
        Some((tag, data))
    }
}

/// Numbers of one data element, whatever its storage type.
fn numbers(ty: u32, d: &[u8]) -> Vec<f64> {
    let w = match ty {
        MI_INT8 | MI_UINT8 | MI_UTF8 => 1,
        MI_INT16 | MI_UINT16 | MI_UTF16 => 2,
        MI_INT32 | MI_UINT32 | MI_SINGLE => 4,
        _ => 8,
    };
    d.chunks_exact(w)
        .map(|c| match ty {
            MI_INT8 => c[0] as i8 as f64,
            MI_UINT8 | MI_UTF8 => c[0] as f64,
            MI_INT16 => i16::from_le_bytes([c[0], c[1]]) as f64,
            MI_UINT16 | MI_UTF16 => u16::from_le_bytes([c[0], c[1]]) as f64,
            MI_INT32 => i32::from_le_bytes(c.try_into().unwrap()) as f64,
            MI_UINT32 => u32::from_le_bytes(c.try_into().unwrap()) as f64,
            MI_SINGLE => f32::from_le_bytes(c.try_into().unwrap()) as f64,
            MI_INT64 => i64::from_le_bytes(c.try_into().unwrap()) as f64,
            MI_UINT64 => u64::from_le_bytes(c.try_into().unwrap()) as f64,
            _ => f64::from_le_bytes(c.try_into().unwrap()),
        })
        .collect()
}

fn class_kind(c: u8) -> Option<Kind> {
    Some(match c {
        6 => Kind::Double,
        7 => Kind::Single,
        8 => Kind::Int(IntClass::I8),
        9 => Kind::Int(IntClass::U8),
        10 => Kind::Int(IntClass::I16),
        11 => Kind::Int(IntClass::U16),
        12 => Kind::Int(IntClass::I32),
        13 => Kind::Int(IntClass::U32),
        14 => Kind::Int(IntClass::I64),
        15 => Kind::Int(IntClass::U64),
        _ => return None,
    })
}

/// A miMATRIX element: (name, value), or None for a class this reader does not support.
fn read_matrix(d: &[u8]) -> Option<(String, Option<Value>)> {
    let mut r = Reader { b: d, pos: 0 };
    let (_, flags) = r.element()?;
    let class = flags[0];
    let complex = flags[1] & 0x08 != 0;
    let logical = flags[1] & 0x02 != 0;
    let (_, dims_raw) = r.element()?;
    let dims: Vec<usize> = numbers(MI_INT32, dims_raw).iter().map(|&x| x as usize).collect();
    let (_, name_raw) = r.element()?;
    let name = String::from_utf8_lossy(name_raw).into_owned();
    let n: usize = dims.iter().product();
    let value = match class {
        MX_CELL => {
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                let (_, sub) = r.element()?;
                items.push(read_matrix(sub).and_then(|x| x.1).unwrap_or_else(Value::empty));
            }
            Some(Value::Cell(CellArray::from_dims(dims, items)))
        }
        MX_STRUCT => {
            let (_, len_raw) = r.element()?;
            let flen = numbers(MI_INT32, len_raw).first().copied().unwrap_or(0.0) as usize;
            let (_, names_raw) = r.element()?;
            let fields: Vec<String> = if flen == 0 { Vec::new() } else { names_raw.chunks(flen).map(|c| String::from_utf8_lossy(c).trim_end_matches('\0').to_string()).collect() };
            let mut s = StructArray::empty_with(fields.clone());
            for _ in 0..n {
                let mut row = Vec::with_capacity(fields.len());
                for _ in &fields {
                    let (_, sub) = r.element()?;
                    row.push(read_matrix(sub).and_then(|x| x.1).unwrap_or_else(Value::empty));
                }
                s.elems.push(row);
            }
            s.dims = dims;
            Some(Value::Struct(s))
        }
        MX_CHAR => {
            let (ty, data) = r.element()?;
            let codes = if ty == MI_UTF8 { String::from_utf8_lossy(data).chars().map(|c| c as u32 as f64).collect() } else { numbers(ty, data) };
            Some(Value::Num(Matrix::from_dims(dims, codes).cast_to(Kind::Char)))
        }
        MX_SPARSE => {
            let (ty_ir, ir) = r.element()?;
            let (ty_jc, jc) = r.element()?;
            let (ty_pr, pr) = r.element()?;
            let (ir, jc, pr) = (numbers(ty_ir, ir), numbers(ty_jc, jc), numbers(ty_pr, pr));
            let rows = dims.first().copied().unwrap_or(0);
            let cols = dims.get(1).copied().unwrap_or(0);
            let mut data = vec![0.0; rows * cols];
            for c in 0..cols {
                for k in jc[c] as usize..jc.get(c + 1).copied().unwrap_or(0.0) as usize {
                    data[c * rows + ir[k] as usize] = pr.get(k).copied().unwrap_or(0.0);
                }
            }
            let m = Matrix::new(rows, cols, data);
            Some(Value::Num(if logical { m.cast_to(Kind::Logical) } else { m }))
        }
        c => match class_kind(c) {
            Some(kind) => {
                let (ty, re) = r.element()?;
                let mut m = Matrix::from_dims(dims.clone(), numbers(ty, re));
                if complex {
                    let (ty, im) = r.element()?;
                    m.im = Some(numbers(ty, im));
                }
                let kind = if logical { Kind::Logical } else { kind };
                Some(Value::Num(m.cast_to(kind)))
            }
            None => None,
        },
    };
    Some((name, value))
}

/// All (name, value) pairs of a MAT-file; unsupported variables have no value.
fn read_mat(path: &str) -> R<Vec<(String, Option<Value>)>> {
    let bytes = std::fs::read(path).map_err(|_| LangError::new(format!("Unable to find file or directory '{path}'."), 0))?;
    if bytes.len() < 128 || &bytes[126..128] != b"IM" {
        return Err(bad(path));
    }
    let mut out = Vec::new();
    let mut r = Reader { b: &bytes, pos: 128 };
    while r.pos + 8 <= bytes.len() {
        let Some((ty, data)) = r.element() else { break };
        match ty {
            MI_COMPRESSED => {
                let mut raw = Vec::new();
                flate2::read::ZlibDecoder::new(data).read_to_end(&mut raw).map_err(|_| bad(path))?;
                let mut inner = Reader { b: &raw, pos: 0 };
                if let Some((MI_MATRIX, m)) = inner.element() {
                    if let Some(v) = read_matrix(m) {
                        out.push(v);
                    }
                }
            }
            MI_MATRIX => {
                if let Some(v) = read_matrix(data) {
                    out.push(v);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

struct Writer {
    b: Vec<u8>,
}

impl Writer {
    fn element(&mut self, ty: u32, data: &[u8]) {
        self.b.extend(ty.to_le_bytes());
        self.b.extend((data.len() as u32).to_le_bytes());
        self.b.extend(data);
        while self.b.len() % 8 != 0 {
            self.b.push(0);
        }
    }
}

/// One variable as a miMATRIX element (without its tag).
fn write_matrix(name: &str, v: &Value) -> R<Vec<u8>> {
    let mut w = Writer { b: Vec::new() };
    let dims = v.dims();
    let mut flags = [0u8; 8];
    let header = |w: &mut Writer, flags: [u8; 8]| {
        w.element(MI_UINT32, &flags);
        let d: Vec<u8> = dims.iter().flat_map(|&x| (x as i32).to_le_bytes()).collect();
        w.element(MI_INT32, &d);
        w.element(MI_INT8, name.as_bytes());
    };
    match v {
        Value::Num(m) => {
            let (class, ty, width) = match m.kind {
                Kind::Char => (MX_CHAR, MI_UINT16, 2),
                Kind::Logical => {
                    flags[1] |= 0x02;
                    (9, MI_UINT8, 1)
                }
                Kind::Double => (MX_DOUBLE, MI_DOUBLE, 8),
                Kind::Single => (7, MI_SINGLE, 4),
                Kind::Int(IntClass::I8) => (8, MI_INT8, 1),
                Kind::Int(IntClass::U8) => (9, MI_UINT8, 1),
                Kind::Int(IntClass::I16) => (10, MI_INT16, 2),
                Kind::Int(IntClass::U16) => (11, MI_UINT16, 2),
                Kind::Int(IntClass::I32) => (12, MI_INT32, 4),
                Kind::Int(IntClass::U32) => (13, MI_UINT32, 4),
                Kind::Int(IntClass::I64) => (14, MI_INT64, 8),
                Kind::Int(IntClass::U64) => (15, MI_UINT64, 8),
            };
            flags[0] = class;
            if m.is_complex() {
                flags[1] |= 0x08;
            }
            header(&mut w, flags);
            let enc = |xs: &[f64]| -> Vec<u8> {
                xs.iter()
                    .flat_map(|&x| match (ty, width) {
                        (MI_DOUBLE, _) => x.to_le_bytes().to_vec(),
                        (MI_SINGLE, _) => (x as f32).to_le_bytes().to_vec(),
                        (MI_INT8, _) => (x as i8).to_le_bytes().to_vec(),
                        (MI_UINT8, _) => (x as u8).to_le_bytes().to_vec(),
                        (MI_INT16, _) => (x as i16).to_le_bytes().to_vec(),
                        (MI_UINT16, _) => (x as u16).to_le_bytes().to_vec(),
                        (MI_INT32, _) => (x as i32).to_le_bytes().to_vec(),
                        (MI_UINT32, _) => (x as u32).to_le_bytes().to_vec(),
                        (MI_INT64, _) => (x as i64).to_le_bytes().to_vec(),
                        _ => (x as u64).to_le_bytes().to_vec(),
                    })
                    .collect()
            };
            w.element(ty, &enc(&m.data));
            if let Some(im) = &m.im {
                w.element(ty, &enc(im));
            }
        }
        Value::Cell(c) => {
            flags[0] = MX_CELL;
            header(&mut w, flags);
            for item in &c.data {
                let sub = write_matrix("", item)?;
                w.element(MI_MATRIX, &sub);
            }
        }
        Value::Struct(s) => {
            flags[0] = MX_STRUCT;
            header(&mut w, flags);
            let flen = s.fields.iter().map(|f| f.len() + 1).max().unwrap_or(1).max(1);
            w.element(MI_INT32, &(flen as i32).to_le_bytes());
            let mut names = Vec::new();
            for f in &s.fields {
                let mut b = f.as_bytes().to_vec();
                b.resize(flen, 0);
                names.extend(b);
            }
            w.element(MI_INT8, &names);
            for e in &s.elems {
                for item in e {
                    let sub = write_matrix("", item)?;
                    w.element(MI_MATRIX, &sub);
                }
            }
        }
        Value::Str(s) if s.data.len() == 1 => return write_matrix(name, &Value::chars(&s.data[0].clone().unwrap_or_default())),
        other => return rt(format!("Saving values of class '{}' is not supported yet.", other.class_name())),
    }
    Ok(w.b)
}

fn write_mat(path: &str, vars: &[(String, Value)], append: bool) -> R<()> {
    let mut body = if append { std::fs::read(path).ok().filter(|b| b.len() >= 128 && &b[126..128] == b"IM").unwrap_or_default() } else { Vec::new() };
    if body.is_empty() {
        let text = format!("MATLAB 5.0 MAT-file, Platform: PCWIN64, Created on: {} by matlivre", crate::time::format_datetime(crate::time::now_utc_ms() + crate::time::local_offset_ms(), "EEE MMM dd HH:mm:ss yyyy"));
        let mut header = text.into_bytes();
        header.resize(116, b' ');
        header.extend([0u8; 8]);
        header.extend(0x0100u16.to_le_bytes());
        header.extend(b"IM");
        body = header;
    }
    for (name, v) in vars {
        let m = write_matrix(name, v)?;
        let mut raw = Vec::new();
        raw.extend(MI_MATRIX.to_le_bytes());
        raw.extend((m.len() as u32).to_le_bytes());
        raw.extend(&m);
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&raw).map_err(|e| LangError::new(e.to_string(), 0))?;
        let packed = z.finish().map_err(|e| LangError::new(e.to_string(), 0))?;
        body.extend(MI_COMPRESSED.to_le_bytes());
        body.extend((packed.len() as u32).to_le_bytes());
        body.extend(packed);
    }
    std::fs::write(path, body).map_err(|e| LangError::new(format!("Unable to write file {path}: {e}"), 0))
}

fn is_mat_file(path: &str) -> bool {
    std::fs::File::open(path)
        .ok()
        .and_then(|mut f| {
            let mut h = [0u8; 128];
            f.read_exact(&mut h).ok().map(|_| &h[126..128] == b"IM")
        })
        .unwrap_or(false)
}

/// A text file of numbers (load -ascii).
fn read_ascii(path: &str) -> R<Matrix> {
    let text = std::fs::read_to_string(path).map_err(|_| LangError::new(format!("Unable to find file or directory '{path}'."), 0))?;
    let rows: Vec<Vec<f64>> = text
        .lines()
        .map(|l| l.split('%').next().unwrap_or(""))
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|t| !t.is_empty()).map(|t| t.parse::<f64>()).collect::<Result<Vec<_>, _>>())
        .collect::<Result<_, _>>()
        .map_err(|_| LangError::new(format!("Unable to read file '{path}'. Input must be a MAT-file or an ASCII file containing numeric data with same number of columns in each row."), 0))?;
    let c = rows.first().map(|r| r.len()).unwrap_or(0);
    if rows.iter().any(|r| r.len() != c) {
        return rt(format!("Unable to read file '{path}'. Input must be a MAT-file or an ASCII file containing numeric data with same number of columns in each row."));
    }
    let n = rows.len();
    let mut data = vec![0.0; n * c];
    for (i, r) in rows.iter().enumerate() {
        for (j, &x) in r.iter().enumerate() {
            data[j * n + i] = x;
        }
    }
    Ok(Matrix::new(n, c, data))
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

fn with_mat_ext(p: &str) -> String {
    if std::path::Path::new(p).extension().is_none() { format!("{p}.mat") } else { p.to_string() }
}

fn load(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let args: Vec<String> = a.iter().map(|v| v.text()).collect::<R<_>>()?;
    let ascii = args.iter().any(|s| s.eq_ignore_ascii_case("-ascii"));
    let force_mat = args.iter().any(|s| s.eq_ignore_ascii_case("-mat"));
    let mut rest: Vec<&String> = args.iter().filter(|s| !s.starts_with('-')).collect();
    let file = if rest.is_empty() { "matlab.mat".to_string() } else { rest.remove(0).clone() };
    let path = if std::path::Path::new(&file).exists() { file.clone() } else { with_mat_ext(&file) };
    let is_mat = !ascii && (force_mat || is_mat_file(&path));
    let vars: Vec<(String, Value)> = if is_mat {
        let mut all = Vec::new();
        for (name, v) in read_mat(&path)? {
            if !rest.is_empty() && !rest.iter().any(|p| wildcard(p, &name)) {
                continue;
            }
            match v {
                Some(v) => all.push((name, v)),
                None => it.emit(OutputKind::Warning(format!("Variable '{name}' has a class matlivre cannot load yet and was skipped."))),
            }
        }
        for p in &rest {
            if !p.contains('*') && !all.iter().any(|(n, _)| n == *p) {
                it.emit(OutputKind::Warning(format!("Variable '{p}' not found.")));
            }
        }
        all
    } else {
        let m = read_ascii(&path)?;
        if nargout > 0 {
            return one(Value::Num(m));
        }
        // An ASCII file loads into a variable named after the file.
        let stem = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "data".into());
        vec![(crate::builtins_data::make_valid_name(&stem), Value::Num(m))]
    };
    if nargout > 0 {
        let mut sorted = vars;
        sorted.sort_by(|x, y| x.0.cmp(&y.0));
        let mut s = StructArray::scalar();
        for (n, v) in sorted {
            s.set(&n, v);
        }
        return one(Value::Struct(s));
    }
    for (n, v) in vars {
        it.set_var(&n, v);
    }
    none()
}

fn save(it: &mut Interpreter, a: &[Value]) -> R<()> {
    let args: Vec<String> = a.iter().map(|v| v.text()).collect::<R<_>>()?;
    let ascii = args.iter().any(|s| s.eq_ignore_ascii_case("-ascii"));
    let append = args.iter().any(|s| s.eq_ignore_ascii_case("-append"));
    let as_struct = args.iter().position(|s| s.eq_ignore_ascii_case("-struct"));
    let mut rest: Vec<String> = args.iter().filter(|s| !s.starts_with('-')).cloned().collect();
    let file = if rest.is_empty() { "matlab.mat".to_string() } else { rest.remove(0) };
    let mut vars: Vec<(String, Value)> = Vec::new();
    if let Some(p) = as_struct {
        // save(file, '-struct', 's', fields...): the fields become variables.
        let sname = args.get(p + 1).cloned().unwrap_or_default();
        let Some(Value::Struct(s)) = it.get_var(&sname).cloned() else { return rt(format!("The argument to -STRUCT must be the name of a scalar structure variable.")) };
        let wanted: Vec<&String> = rest.iter().filter(|n| **n != sname).collect();
        for (k, f) in s.fields.iter().enumerate() {
            if wanted.is_empty() || wanted.iter().any(|w| wildcard(w, f)) {
                vars.push((f.clone(), s.elems[0][k].clone()));
            }
        }
    } else {
        let names = it.var_names();
        let wanted: Vec<String> = if rest.is_empty() { names.clone() } else { names.iter().filter(|n| rest.iter().any(|p| wildcard(p, n))).cloned().collect() };
        for p in &rest {
            if !p.contains('*') && !names.contains(p) {
                return rt(format!("Variable '{p}' not found."));
            }
        }
        for n in wanted {
            if let Some(v) = it.get_var(&n) {
                vars.push((n.clone(), v.clone()));
            }
        }
    }
    if ascii {
        // Each numeric variable as rows of %16.7e values (-double for 16 digits).
        let double = args.iter().any(|s| s.eq_ignore_ascii_case("-double"));
        let tabs = args.iter().any(|s| s.eq_ignore_ascii_case("-tabs"));
        let mut text = String::new();
        for (_, v) in &vars {
            let m = v.to_matrix()?;
            for r in 0..m.rows() {
                let cells: Vec<String> = (0..m.cols()).map(|c| {
                    let x = m.get(r, c);
                    if double { format!("{:>24}", fmt_e(x, 16)) } else { format!("{:>16}", fmt_e(x, 7)) }
                }).collect();
                text.push_str(&cells.join(if tabs { "\t" } else { "" }));
                text.push('\n');
            }
        }
        let mut f = std::fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).open(&file).map_err(|e| LangError::new(e.to_string(), 0))?;
        f.write_all(text.as_bytes()).map_err(|e| LangError::new(e.to_string(), 0))?;
        return Ok(());
    }
    write_mat(&with_mat_ext(&file), &vars, append)
}

/// `%.{p}e` with a two-digit exponent, as MATLAB's text output uses.
fn fmt_e(x: f64, p: usize) -> String {
    let s = format!("{x:.p$e}");
    match s.split_once('e') {
        Some((m, e)) => {
            let (sign, digits) = if let Some(d) = e.strip_prefix('-') { ('-', d) } else { ('+', e) };
            format!("{m}e{sign}{digits:0>2}")
        }
        None => s,
    }
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match name {
        "jsondecode" => {
            let text = arg(a, 0, name)?.text()?;
            let j: serde_json::Value = serde_json::from_str(&text).map_err(|e| LangError::new(format!("Unable to parse JSON text: {e}"), 0))?;
            one(json_to_value(&j))
        }
        "jsonencode" => {
            let v = arg(a, 0, name)?;
            let pretty = a.len() >= 3 && a[1].text().map(|t| t.eq_ignore_ascii_case("PrettyPrint")).unwrap_or(false) && a[2].is_true()?;
            let j = value_to_json(v)?;
            let s = if pretty { serde_json::to_string_pretty(&j) } else { serde_json::to_string(&j) }.map_err(|e| LangError::new(e.to_string(), 0))?;
            one(Value::chars(&s))
        }
        "load" => load(it, a, nargout),
        "save" => {
            save(it, a)?;
            none()
        }
        _ => Ok(None),
    }
}
