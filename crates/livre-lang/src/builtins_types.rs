//! Cell, struct and type conversions: `cell2mat`, `mat2cell`, `cell2struct`, `struct2cell`,
//! `structfun`, `getfield`, `setfield`, `orderfields`, `typecast`, `swapbytes`, `allfinite`,
//! `anynan`, `functions`.

use crate::builtins::{arg, one, Ret};
use crate::error::{rt, LangError};
use crate::indexing;
use crate::interp::Interpreter;
use crate::nd::IndexArg;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "cell2mat", "mat2cell", "cell2struct", "struct2cell", "structfun", "getfield", "setfield", "orderfields", "typecast", "swapbytes", "allfinite",
    "anynan", "functions",
];

/// Zero-based positions as one subscript.
fn positions(ix: Vec<usize>) -> IndexArg {
    let n = ix.len();
    IndexArg::List { ix, dims: vec![1, n], from_mask: false }
}

/// A subscript from a value in a getfield/setfield index cell: ':', a logical mask or 1-based
/// positions.
fn index_from(v: &Value) -> R<IndexArg> {
    if v.is_text() && v.text()? == ":" {
        return Ok(IndexArg::All);
    }
    let m = v.to_matrix()?;
    if m.kind == Kind::Logical {
        return Ok(positions(m.data.iter().enumerate().filter(|(_, &b)| b != 0.0).map(|(i, _)| i).collect()));
    }
    Ok(positions(m.data.iter().map(|&k| k as usize - 1).collect()))
}

fn cell2mat(v: &Value) -> Ret {
    let Value::Cell(c) = v else { return rt("Input must be a cell array.") };
    if c.data.is_empty() {
        return one(Value::empty());
    }
    let first = c.data[0].class_name();
    if c.data.iter().any(|x| x.class_name() != first) {
        return rt("All contents of the input cell array must be of the same data type.");
    }
    if c.data.iter().any(|x| matches!(x, Value::Cell(_))) {
        return rt("CELL2MAT does not support cell arrays containing cell arrays or objects.");
    }
    // Rows of the cell concatenated side by side, then the rows stacked.
    let (r, k) = (c.rows(), c.cols());
    let rows = (0..r).map(|i| indexing::hconcat((0..k).map(|j| c.data[j * r + i].clone()).collect())).collect::<R<Vec<_>>>()?;
    one(indexing::vconcat(rows)?)
}

/// mat2cell(A, r, c): blocks of A with the given row and column sizes.
fn mat2cell(a: &[Value]) -> Ret {
    let x = arg(a, 0, "mat2cell")?;
    let dims = x.dims();
    let sizes: Vec<Vec<usize>> = (0..dims.len().max(2))
        .map(|d| match a.get(d + 1) {
            Some(v) => Ok(v.to_matrix()?.data.iter().map(|&s| s as usize).collect()),
            None => Ok(vec![dims.get(d).copied().unwrap_or(1)]),
        })
        .collect::<R<_>>()?;
    for (d, s) in sizes.iter().enumerate().take(2) {
        let total: usize = s.iter().sum();
        if total != dims.get(d).copied().unwrap_or(1) {
            return rt(format!(
                "Input arguments, D1 through D{}, must sum to each dimension of the input matrix size, [{}].",
                sizes.len(),
                dims.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(" ")
            ));
        }
    }
    let (rs, cs) = (&sizes[0], &sizes[1]);
    let mut out = Vec::with_capacity(rs.len() * cs.len());
    let mut c0 = 0;
    for &cw in cs {
        let mut r0 = 0;
        for &rw in rs {
            let ri = positions((r0..r0 + rw).collect());
            let ci = positions((c0..c0 + cw).collect());
            out.push(indexing::index(x, &[ri, ci])?);
            r0 += rw;
        }
        c0 += cw;
    }
    one(Value::Cell(CellArray::from_dims(vec![rs.len(), cs.len()], out)))
}

fn field_names(v: &Value) -> R<Vec<String>> {
    match v {
        Value::Cell(c) => c.data.iter().map(|x| x.text()).collect(),
        Value::Str(s) => Ok(s.data.iter().map(|x| x.clone().unwrap_or_default()).collect()),
        Value::Num(m) if m.kind == Kind::Char && m.rows() > 1 => {
            let (r, c) = (m.rows(), m.cols());
            Ok((0..r).map(|i| (0..c).map(|j| char::from_u32(m.data[j * r + i] as u32).unwrap_or(' ')).collect::<String>().trim_end().to_string()).collect())
        }
        other => Ok(vec![other.text()?]),
    }
}

/// cell2struct(C, fields, dim): one struct element per slice of C across `dim`.
fn cell2struct(a: &[Value]) -> Ret {
    let Value::Cell(c) = arg(a, 0, "cell2struct")? else { return rt("Input must be a cell array.") };
    let fields = field_names(arg(a, 1, "cell2struct")?)?;
    let dim = a.get(2).map(|v| v.scalar()).transpose()?.unwrap_or(1.0) as usize;
    let (r, k) = (c.rows(), c.cols());
    let (nf, ne) = if dim == 1 { (r, k) } else { (k, r) };
    if nf != fields.len() {
        return rt("Number of field names must match number of fields in new structure.");
    }
    let mut s = StructArray::empty_with(fields);
    for e in 0..ne {
        s.elems.push((0..nf).map(|f| if dim == 1 { c.data[e * r + f].clone() } else { c.data[f * r + e].clone() }).collect());
    }
    s.dims = vec![ne, 1];
    one(Value::Struct(s))
}

fn struct2cell(v: &Value) -> Ret {
    let Value::Struct(s) = v else { return rt("Input must be a structure.") };
    let nf = s.fields.len();
    let n = s.elems.len();
    // Fields along the first dimension, then the struct's own dimensions.
    let mut data = Vec::with_capacity(nf * n);
    for e in &s.elems {
        data.extend(e.iter().cloned());
    }
    let mut dims = vec![nf];
    dims.extend(if s.dims == [1, 1] { vec![1] } else { s.dims.clone() });
    one(Value::Cell(CellArray::from_dims(dims, data)))
}

fn structfun(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let f = arg(a, 0, "structfun")?.clone();
    let Value::Struct(s) = arg(a, 1, "structfun")? else { return rt("Input must be a scalar structure.") };
    if s.elems.len() != 1 {
        return rt("Input must be a scalar structure.");
    }
    let mut uniform = true;
    let mut handler = None;
    let mut i = 2;
    while i + 1 < a.len() {
        match a[i].text()?.to_lowercase().as_str() {
            "uniformoutput" => uniform = a[i + 1].is_true()?,
            "errorhandler" => handler = Some(a[i + 1].clone()),
            other => return rt(format!("Unrecognized parameter name '{other}'.")),
        }
        i += 2;
    }
    let k = nargout.max(1);
    let mut cols: Vec<Vec<Value>> = vec![Vec::new(); k];
    for (fi, v) in s.elems[0].iter().enumerate() {
        let r = match it.call_func_value(&f, vec![v.clone()], k) {
            Ok(r) => r,
            Err(e) => match &handler {
                Some(h) => {
                    let mut info = StructArray::scalar();
                    info.set("message", Value::chars(&e.message));
                    info.set("identifier", Value::chars(""));
                    info.set("index", Value::num(fi as f64 + 1.0));
                    it.call_func_value(h, vec![Value::Struct(info), v.clone()], k)?
                }
                None => return Err(e),
            },
        };
        for (j, out) in r.into_iter().take(k).enumerate() {
            if uniform && out.numel() != 1 {
                return rt("Non-scalar in Uniform output, at index 1, output 1.\nSet 'UniformOutput' to false.");
            }
            cols[j].push(out);
        }
    }
    let n = s.fields.len();
    let outs = cols
        .into_iter()
        .map(|vals| {
            if uniform {
                if vals.is_empty() {
                    return Ok(Value::Num(Matrix::new(0, 1, Vec::new())));
                }
                indexing::vconcat(vals)
            } else {
                let mut st = StructArray::scalar();
                for (name, v) in s.fields.iter().zip(vals) {
                    st.set(name, v);
                }
                Ok(Value::Struct(st))
            }
        })
        .collect::<R<Vec<_>>>()?;
    let _ = n;
    Ok(Some(outs))
}

fn getfield(a: &[Value]) -> Ret {
    let mut v = arg(a, 0, "getfield")?.clone();
    for f in &a[1..] {
        match f {
            Value::Cell(c) => {
                let idx = c.data.iter().map(|x| index_from(x)).collect::<R<Vec<_>>>()?;
                v = indexing::index(&v, &idx)?;
            }
            name => {
                let n = name.text()?;
                v = match &v {
                    Value::Struct(s) if s.elems.len() == 1 => s.get(&n).cloned().ok_or_else(|| LangError::new(format!("Unrecognized field name \"{n}\"."), 0))?,
                    Value::Struct(_) => return rt("Dot indexing is not supported for structure arrays with more than one element here."),
                    _ => return rt("Dot indexing is not supported for variables of this type."),
                };
            }
        }
    }
    one(v)
}

fn setfield(a: &[Value]) -> Ret {
    let s = arg(a, 0, "setfield")?.clone();
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    let value = a[a.len() - 1].clone();
    one(set_path(s, &a[1..a.len() - 1], value)?)
}

/// Assigns `value` at a getfield-style path of field names and index cells.
fn set_path(base: Value, path: &[Value], value: Value) -> R<Value> {
    let Some((first, rest)) = path.split_first() else { return Ok(value) };
    match first {
        Value::Cell(c) => {
            let idx = c.data.iter().map(|x| index_from(x)).collect::<R<Vec<_>>>()?;
            let inner = if rest.is_empty() { value } else { set_path(indexing::index(&base, &idx).unwrap_or_else(|_| Value::empty()), rest, value)? };
            indexing::assign(Some(base), &idx, inner)
        }
        name => {
            let n = name.text()?;
            let mut s = match base {
                Value::Struct(s) => s,
                Value::Num(m) if m.is_empty() => StructArray::scalar(),
                _ => return rt("Field assignment to a non-structure array object."),
            };
            let cur = s.get(&n).cloned().unwrap_or_else(Value::empty);
            let new = set_path(cur, rest, value)?;
            s.set(&n, new);
            Ok(Value::Struct(s))
        }
    }
}

fn orderfields(a: &[Value], nargout: usize) -> Ret {
    let Value::Struct(s) = arg(a, 0, "orderfields")? else { return rt("Input must be a structure.") };
    let n = s.fields.len();
    let perm: Vec<usize> = match a.get(1) {
        None => {
            let mut p: Vec<usize> = (0..n).collect();
            p.sort_by(|&x, &y| s.fields[x].cmp(&s.fields[y]));
            p
        }
        Some(Value::Struct(o)) => o.fields.iter().map(|f| s.field_index(f).ok_or_else(|| LangError::new("Structures must have the same field names.", 0))).collect::<R<_>>()?,
        Some(v @ (Value::Cell(_) | Value::Str(_))) => field_names(v)?.iter().map(|f| s.field_index(f).ok_or_else(|| LangError::new(format!("Invalid field name '{f}'."), 0))).collect::<R<_>>()?,
        Some(v) => v.to_matrix()?.data.iter().map(|&k| k as usize - 1).collect(),
    };
    if perm.len() != n {
        return rt("The permutation must list every field once.");
    }
    let mut out = s.clone();
    out.fields = perm.iter().map(|&k| s.fields[k].clone()).collect();
    out.elems = s.elems.iter().map(|e| perm.iter().map(|&k| e[k].clone()).collect()).collect();
    let mut res = vec![Value::Struct(out)];
    if nargout > 1 {
        res.push(Value::Num(Matrix::col(perm.iter().map(|&k| k as f64 + 1.0).collect())));
    }
    Ok(Some(res))
}

fn kind_bytes(k: Kind) -> usize {
    match k {
        Kind::Double => 8,
        Kind::Single => 4,
        Kind::Logical | Kind::Char => 1,
        Kind::Int(i) => i.bytes(),
    }
}

/// Little-endian bytes of each element.
fn to_bytes(m: &Matrix) -> Vec<u8> {
    let mut out = Vec::new();
    for &x in &m.data {
        match m.kind {
            Kind::Double => out.extend(x.to_le_bytes()),
            Kind::Single => out.extend((x as f32).to_le_bytes()),
            Kind::Int(IntClass::I8) => out.extend((x as i8).to_le_bytes()),
            Kind::Int(IntClass::U8) | Kind::Logical | Kind::Char => out.push(x as u8),
            Kind::Int(IntClass::I16) => out.extend((x as i16).to_le_bytes()),
            Kind::Int(IntClass::U16) => out.extend((x as u16).to_le_bytes()),
            Kind::Int(IntClass::I32) => out.extend((x as i32).to_le_bytes()),
            Kind::Int(IntClass::U32) => out.extend((x as u32).to_le_bytes()),
            Kind::Int(IntClass::I64) => out.extend((x as i64).to_le_bytes()),
            Kind::Int(IntClass::U64) => out.extend((x as u64).to_le_bytes()),
        }
    }
    out
}

fn from_bytes(b: &[u8], k: Kind) -> f64 {
    let a = |n: usize| -> [u8; 8] {
        let mut x = [0u8; 8];
        x[..n].copy_from_slice(&b[..n]);
        x
    };
    match k {
        Kind::Double => f64::from_le_bytes(a(8)),
        Kind::Single => f32::from_le_bytes(a(4)[..4].try_into().unwrap()) as f64,
        Kind::Int(IntClass::I8) => b[0] as i8 as f64,
        Kind::Int(IntClass::U8) | Kind::Logical | Kind::Char => b[0] as f64,
        Kind::Int(IntClass::I16) => i16::from_le_bytes([b[0], b[1]]) as f64,
        Kind::Int(IntClass::U16) => u16::from_le_bytes([b[0], b[1]]) as f64,
        Kind::Int(IntClass::I32) => i32::from_le_bytes(a(4)[..4].try_into().unwrap()) as f64,
        Kind::Int(IntClass::U32) => u32::from_le_bytes(a(4)[..4].try_into().unwrap()) as f64,
        Kind::Int(IntClass::I64) => i64::from_le_bytes(a(8)) as f64,
        Kind::Int(IntClass::U64) => u64::from_le_bytes(a(8)) as f64,
    }
}

fn typecast(a: &[Value]) -> Ret {
    let m = arg(a, 0, "typecast")?.to_matrix()?;
    let target = arg(a, 1, "typecast")?.text()?;
    let k = match Kind::from_name(&target) {
        Some(k @ (Kind::Double | Kind::Single | Kind::Int(_))) => k,
        _ => return rt("The second input argument must be a numeric class name, such as 'uint8' or 'double'."),
    };
    if !(m.is_vector() || m.is_scalar() || m.is_empty()) {
        return rt("The first input argument must be a full, non-complex numeric scalar or vector.");
    }
    let bytes = to_bytes(&m);
    let w = kind_bytes(k);
    if bytes.len() % w != 0 {
        return rt("Too few input values to make output type.");
    }
    let vals: Vec<f64> = bytes.chunks(w).map(|c| from_bytes(c, k)).collect();
    let n = vals.len();
    let out = if m.rows() > 1 { Matrix::new(n, 1, vals) } else { Matrix::new(1, n, vals) };
    one(Value::Num(out.with_kind(k)))
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match name {
        "cell2mat" => cell2mat(arg(a, 0, name)?),
        "mat2cell" => mat2cell(a),
        "cell2struct" => cell2struct(a),
        "struct2cell" => struct2cell(arg(a, 0, name)?),
        "structfun" => structfun(it, a, nargout),
        "getfield" => getfield(a),
        "setfield" => setfield(a),
        "orderfields" => orderfields(a, nargout),
        "typecast" => typecast(a),
        "swapbytes" => {
            let m = arg(a, 0, name)?.to_matrix()?;
            let w = kind_bytes(m.kind);
            let bytes = to_bytes(&m);
            let vals: Vec<f64> = bytes.chunks(w).map(|c| {
                let r: Vec<u8> = c.iter().rev().copied().collect();
                from_bytes(&r, m.kind)
            }).collect();
            one(Value::Num(Matrix::from_dims(m.dims.clone(), vals).with_kind(m.kind)))
        }
        "allfinite" => {
            let m = arg(a, 0, name)?.to_matrix()?;
            one(Value::bool(m.data.iter().all(|x| x.is_finite()) && m.im.as_ref().is_none_or(|im| im.iter().all(|x| x.is_finite()))))
        }
        "anynan" => {
            let m = arg(a, 0, name)?.to_matrix()?;
            one(Value::bool(m.data.iter().any(|x| x.is_nan()) || m.im.as_ref().is_some_and(|im| im.iter().any(|x| x.is_nan()))))
        }
        "functions" => {
            let Value::Func(f) = arg(a, 0, name)? else { return rt("Input must be a function handle.") };
            let mut s = StructArray::scalar();
            match f {
                FuncValue::Named(n) => {
                    s.set("function", Value::chars(n));
                    s.set("type", Value::chars("simple"));
                    s.set("file", Value::chars(""));
                }
                FuncValue::Anon { captured, .. } => {
                    s.set("function", Value::chars(&crate::display::func_text(f)));
                    s.set("type", Value::chars("anonymous"));
                    s.set("file", Value::chars(""));
                    let mut ws = StructArray::scalar();
                    for (n, v) in captured.iter() {
                        ws.set(n, v.clone());
                    }
                    s.set("workspace", Value::Cell(CellArray::row(vec![Value::Struct(ws)])));
                }
            }
            one(Value::Struct(s))
        }
        _ => Ok(None),
    }
}
