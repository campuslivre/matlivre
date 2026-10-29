//! `A(idx)`, `A(idx) = rhs`, `A(idx) = []` and concatenation for every array type, on top of
//! the shape plans in [`crate::nd`].

use crate::error::{rt, LangError};
use crate::nd::{self, Dims, IndexArg};
use crate::ops::to_str_array;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub fn to_index_arg(v: &Value) -> R<IndexArg> {
    match v {
        Value::Num(m) if m.kind == Kind::Char && m.to_string_lossy() == ":" => Ok(IndexArg::All),
        Value::Num(m) if m.kind == Kind::Logical => {
            let ix: Vec<usize> = m.data.iter().enumerate().filter(|(_, &x)| x != 0.0).map(|(i, _)| i).collect();
            let n = ix.len();
            let dims = if m.is_2d() && m.dims[0] == 1 { vec![1, n] } else { vec![n, 1] };
            Ok(IndexArg::List { ix, dims, from_mask: true })
        }
        Value::Num(m) => {
            if m.is_complex() {
                return rt("Subscript indices must be real positive integers.");
            }
            let mut ix = Vec::with_capacity(m.numel());
            for &x in &m.data {
                if x < 1.0 || x.fract() != 0.0 || !x.is_finite() {
                    if x.fract() != 0.0 {
                        return rt("Array indices must be positive integers or logical values.");
                    }
                    return rt(format!("Index in position 1 is invalid. Array indices must be positive integers or logical values."));
                }
                ix.push(x as usize - 1);
            }
            Ok(IndexArg::List { ix, dims: m.dims.clone(), from_mask: false })
        }
        Value::Sym(s) => {
            let m = s.to_matrix()?;
            to_index_arg(&Value::Num(m))
        }
        _ => rt(format!("Indexing with a value of class '{}' is not supported.", v.class_name())),
    }
}

/// `v(idx)`.
pub fn index(v: &Value, idx: &[IndexArg]) -> R<Value> {
    let (pos, dims) = nd::index_plan(&v.dims(), idx)?;
    Ok(match v {
        Value::Num(m) => Value::Num(m.gather(&pos, dims)),
        Value::Cell(c) => Value::Cell(CellArray::from_dims(dims, pos.iter().map(|&p| c.data[p].clone()).collect())),
        Value::Str(s) => Value::Str(StrArray::from_dims(dims, pos.iter().map(|&p| s.data[p].clone()).collect())),
        Value::Struct(s) => Value::Struct(StructArray { dims: nd::normalize(dims), fields: s.fields.clone(), elems: pos.iter().map(|&p| s.elems[p].clone()).collect() }),
        Value::Time(t) => Value::Time(t.gather(&pos, dims)),
        Value::Sym(s) => Value::Sym(s.gather(&pos, dims)),
        Value::Func(_) | Value::Object(_) => v.clone(),
        Value::Table(_) => return rt("Use T(rows, vars) with two subscripts to index a table."),
    })
}

fn is_delete_rhs(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.dims == [0, 0] && m.kind != Kind::Char)
}

/// Empty container of the same type as `like`, used when assigning into an undefined
/// variable or into `[]`.
fn empty_like(like: &Value) -> Value {
    match like {
        Value::Cell(_) => Value::Cell(CellArray::from_dims(vec![0, 0], Vec::new())),
        Value::Str(_) => Value::Str(StrArray::from_dims(vec![0, 0], Vec::new())),
        Value::Struct(s) => Value::Struct(StructArray::empty_with(s.fields.clone())),
        Value::Time(t) => Value::Time(t.empty_like()),
        Value::Sym(_) => Value::Sym(crate::sym::SymArray::empty()),
        Value::Num(m) => Value::Num(Matrix::from_dims(vec![0, 0], Vec::new()).with_kind(m.kind)),
        other => other.clone(),
    }
}

/// `base(idx) = rhs` (including deletion with `[]`).
pub fn assign(base: Option<Value>, idx: &[IndexArg], rhs: Value) -> R<Value> {
    let base = match base {
        Some(b) => b,
        None => {
            if is_delete_rhs(&rhs) {
                return rt("Unrecognized variable for deletion.");
            }
            empty_like(&rhs)
        }
    };
    if is_delete_rhs(&rhs) {
        return delete(base, idx);
    }
    // An empty double `[]` takes the type of what is assigned into it.
    let base = match &base {
        Value::Num(m) if m.is_empty() && m.kind == Kind::Double && !matches!(rhs, Value::Num(_)) => empty_like(&rhs),
        _ => base,
    };
    let rd = rhs.dims();
    match base {
        Value::Num(mut m) => match rhs {
            Value::Num(r) => {
                // The left side keeps its class, except that logical and empty arrays take the
                // right side's class (MATLAB subscripted assignment).
                let kind = if m.is_empty() && m.kind == Kind::Double { r.kind } else if m.kind == Kind::Logical && r.kind != Kind::Logical { r.kind } else { m.kind };
                let complex = m.is_complex() || r.is_complex();
                let r_re: Vec<f64> = r.data.iter().map(|&x| kind.cast(x)).collect();
                let r_im = r.im.clone();
                if complex && m.im.is_none() {
                    m.im = Some(vec![0.0; m.numel()]);
                }
                let mut dims = m.dims.clone();
                nd::assign(&mut dims, &mut m.data, idx, &rd, r_re, 0.0)?;
                if complex {
                    let mut d2 = m.dims.clone();
                    let im_r = r_im.unwrap_or_else(|| vec![0.0; r.data.len()]);
                    let mut im = m.im.take().unwrap();
                    nd::assign(&mut d2, &mut im, idx, &rd, im_r, 0.0)?;
                    m.im = Some(im);
                }
                m.dims = dims;
                m.kind = kind;
                if kind != m.kind {
                    m = m.cast_to(kind);
                }
                m.narrow();
                Ok(Value::Num(m))
            }
            Value::Sym(_) => assign(Some(Value::Sym(crate::sym::SymArray::from_value(&Value::Num(m))?)), idx, rhs),
            other => rt(format!("Conversion to {} from {} is not possible.", m.kind.name(), other.class_name())),
        },
        Value::Cell(mut c) => match rhs {
            Value::Cell(r) => {
                nd::assign(&mut c.dims, &mut c.data, idx, &rd, r.data, Value::empty())?;
                Ok(Value::Cell(c))
            }
            other => rt(format!("Conversion to cell from {} is not possible.", other.class_name())),
        },
        Value::Str(mut s) => {
            let r = to_str_array(&rhs)?;
            nd::assign(&mut s.dims, &mut s.data, idx, &r.dims.clone(), r.data, None)?;
            Ok(Value::Str(s))
        }
        Value::Struct(mut s) => match rhs {
            Value::Struct(r) => {
                let r = align_fields(&mut s, r)?;
                let fill = s.empty_elem();
                nd::assign(&mut s.dims, &mut s.elems, idx, &rd, r.elems, fill)?;
                Ok(Value::Struct(s))
            }
            other => rt(format!("Conversion to struct from {} is not possible.", other.class_name())),
        },
        Value::Time(t) => Ok(Value::Time(t.assign(idx, &rhs)?)),
        Value::Sym(mut s) => {
            let r = crate::sym::SymArray::from_value(&rhs)?;
            let zero = crate::sym::Expr::zero();
            nd::assign(&mut s.dims, &mut s.data, idx, &r.dims.clone(), r.data, zero)?;
            Ok(Value::Sym(s))
        }
        b @ (Value::Func(_) | Value::Object(_)) => {
            let (pos, _) = nd::assign_plan(&[1, 1], idx, &rd)?;
            if pos.iter().all(|&p| p == 0) && nd::numel(&rd) == 1 {
                let _ = b;
                Ok(rhs)
            } else {
                rt("Arrays of objects or function handles are not supported in this version.")
            }
        }
        Value::Table(_) => rt("Use T(rows, vars) = ... or T.Var(rows) = ... to assign into a table."),
    }
}

/// Makes `r`'s fields match `s`'s (MATLAB requires identical field sets; an empty struct
/// array adopts the right side's fields).
fn align_fields(s: &mut StructArray, r: StructArray) -> R<StructArray> {
    if s.elems.is_empty() && s.fields.is_empty() {
        s.fields = r.fields.clone();
        return Ok(r);
    }
    let same_set = s.fields.len() == r.fields.len() && r.fields.iter().all(|f| s.fields.contains(f));
    if !same_set {
        return rt("Subscripted assignment between dissimilar structures.");
    }
    let order: Vec<usize> = s.fields.iter().map(|f| r.field_index(f).unwrap()).collect();
    let elems = r.elems.into_iter().map(|e| order.iter().map(|&k| e[k].clone()).collect()).collect();
    Ok(StructArray { dims: r.dims, fields: s.fields.clone(), elems })
}

pub fn delete(base: Value, idx: &[IndexArg]) -> R<Value> {
    Ok(match base {
        Value::Num(mut m) => {
            let (keep, shape) = nd::delete_plan(&m.dims, idx)?;
            let mut out = m.gather(&keep, shape);
            out.kind = m.kind;
            m = out;
            Value::Num(m)
        }
        Value::Cell(mut c) => {
            nd::delete(&mut c.dims, &mut c.data, idx)?;
            Value::Cell(c)
        }
        Value::Str(mut s) => {
            nd::delete(&mut s.dims, &mut s.data, idx)?;
            Value::Str(s)
        }
        Value::Struct(mut s) => {
            nd::delete(&mut s.dims, &mut s.elems, idx)?;
            Value::Struct(s)
        }
        Value::Time(t) => Value::Time(t.delete(idx)?),
        Value::Sym(mut s) => {
            nd::delete(&mut s.dims, &mut s.data, idx)?;
            Value::Sym(s)
        }
        other => return rt(format!("Deletion is not supported for values of class '{}'.", other.class_name())),
    })
}

// ---- concatenation -----------------------------------------------------------------------

pub fn hconcat(items: Vec<Value>) -> R<Value> {
    concat(items, 1)
}

pub fn vconcat(items: Vec<Value>) -> R<Value> {
    concat(items, 0)
}

/// Concatenates along 0-based `dim` with MATLAB's class precedence.
pub fn concat(items: Vec<Value>, dim: usize) -> R<Value> {
    if items.len() == 1 && dim < 2 {
        return Ok(items.into_iter().next().unwrap());
    }
    if items.is_empty() {
        return Ok(Value::empty());
    }
    if items.iter().any(|v| matches!(v, Value::Table(_))) {
        return crate::table::concat(items, dim);
    }
    if items.iter().any(|v| matches!(v, Value::Sym(_))) {
        let parts = items.iter().filter(|v| !is_delete_rhs(v)).map(|v| crate::sym::SymArray::from_value(v).map(|s| (s.dims, s.data))).collect::<R<Vec<_>>>()?;
        let (d, data) = nd::concat(parts, dim)?;
        return Ok(Value::Sym(crate::sym::SymArray { dims: d, data }));
    }
    let items: Vec<Value> = {
        let non_empty: Vec<Value> = items.iter().filter(|v| !is_delete_rhs(v)).cloned().collect();
        if non_empty.is_empty() {
            return Ok(Value::empty());
        }
        non_empty
    };
    if items.len() == 1 {
        return Ok(items.into_iter().next().unwrap());
    }
    if items.iter().any(|v| matches!(v, Value::Time(_))) {
        return crate::time::concat(items, dim);
    }
    if items.iter().any(|v| matches!(v, Value::Str(_))) {
        let parts = items.iter().map(|v| to_str_array(v).map(|s| (s.dims, s.data))).collect::<R<Vec<_>>>()?;
        let (d, data) = nd::concat(parts, dim)?;
        return Ok(Value::Str(StrArray::from_dims(d, data)));
    }
    if items.iter().any(|v| matches!(v, Value::Cell(_))) {
        let parts: Vec<(Dims, Vec<Value>)> = items
            .into_iter()
            .map(|v| match v {
                Value::Cell(c) => (c.dims, c.data),
                other => (vec![1, 1], vec![other]),
            })
            .collect();
        let (d, data) = nd::concat(parts, dim)?;
        return Ok(Value::Cell(CellArray::from_dims(d, data)));
    }
    if items.iter().any(|v| matches!(v, Value::Struct(_))) {
        let mut structs = Vec::new();
        for v in items {
            match v {
                Value::Struct(s) => structs.push(s),
                other => return rt(format!("Concatenation of struct with {} is not possible.", other.class_name())),
            }
        }
        let mut first = structs.remove(0);
        let mut parts = vec![(first.dims.clone(), std::mem::take(&mut first.elems))];
        for s in structs {
            let s = align_fields(&mut first, s).map_err(|_| LangError::new("Concatenation of structures with different field names is not possible.", 0))?;
            parts.push((s.dims, s.elems));
        }
        let (d, elems) = nd::concat(parts, dim)?;
        return Ok(Value::Struct(StructArray { dims: d, fields: first.fields, elems }));
    }
    if items.iter().any(|v| matches!(v, Value::Object(_) | Value::Func(_))) {
        return rt("Arrays of objects or function handles are not supported in this version; use a cell array.");
    }
    let mats: Vec<Matrix> = items.into_iter().map(|v| if let Value::Num(m) = v { m } else { unreachable!() }).collect();
    let mut kind = Kind::Double;
    let mut int: Option<IntClass> = None;
    for m in &mats {
        if let Kind::Int(c) = m.kind {
            match int {
                None => int = Some(c),
                Some(prev) if prev != c => {} // MATLAB uses the leftmost integer class.
                _ => {}
            }
        }
    }
    if let Some(c) = int {
        kind = Kind::Int(c);
    } else if mats.iter().any(|m| m.kind == Kind::Char) {
        kind = Kind::Char;
    } else if mats.iter().any(|m| m.kind == Kind::Single) {
        kind = Kind::Single;
    } else if mats.iter().all(|m| m.kind == Kind::Logical) {
        kind = Kind::Logical;
    }
    // 0×0 empties of any class (e.g. '') take part in the class choice but not the shape.
    let mats: Vec<Matrix> = {
        let kept: Vec<Matrix> = mats.iter().filter(|m| m.dims != [0, 0]).cloned().collect();
        if kept.is_empty() {
            return Ok(Value::Num(Matrix::empty().with_kind(kind)));
        }
        kept
    };
    let complex = mats.iter().any(|m| m.is_complex());
    let re_parts: Vec<(Dims, Vec<f64>)> = mats.iter().map(|m| (m.dims.clone(), m.data.clone())).collect();
    let (d, data) = nd::concat(re_parts, dim)?;
    let mut out = Matrix::from_dims(d.clone(), data);
    if complex {
        let im_parts: Vec<(Dims, Vec<f64>)> = mats.iter().map(|m| (m.dims.clone(), m.im.clone().unwrap_or_else(|| vec![0.0; m.numel()]))).collect();
        let (_, im) = nd::concat(im_parts, dim)?;
        out.im = Some(im);
    }
    Ok(Value::Num(out.cast_to(kind)))
}
