//! Elementary matrices and array manipulation (`toolbox/matlab/elmat`) that the core builtins
//! lack: replication and resizing (`repelem`, `resize`, `paddata`, `trimdata`), rearrangement
//! (`rot90`, `ipermute`, `shiftdim`, `flipdim`, `blkdiag`), index conversion (`sub2ind`,
//! `ind2sub`), `bsxfun`, `isequaln`, `freqspace`, `maxNumCompThreads` and the specialized
//! matrices (`compan`, `hadamard`, `hankel`, `hilb`, `invhilb`, `magic`, `pascal`, `rosser`,
//! `toeplitz`, `vander`, `wilkinson`, `gallery`).

use std::f64::consts::PI;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::ast::BinOp;
use crate::builtins::{arg, default_dim, m_arg, one, values_equal, Ret};
use crate::cplx::C;
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::nd::{self, Dims};
use crate::ops;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "repelem", "freqspace", "isequaln", "isequalwithequalnans", "resize", "paddata", "trimdata", "blkdiag", "rot90", "sub2ind", "ind2sub",
    "bsxfun", "ipermute", "shiftdim", "compan", "gallery", "hadamard", "hankel", "hilb", "invhilb", "magic", "pascal", "rosser", "toeplitz",
    "vander", "wilkinson", "flipdim", "maxNumCompThreads",
];

// ---- element gathering -----------------------------------------------------------------------

/// Linear source positions for an output whose k-th subscript takes source subscript
/// `maps[k][j]` (`None` marks a padded element). `src` is padded with ones to `maps.len()`.
fn plan(src: &[usize], maps: &[Vec<Option<usize>>]) -> (Dims, Vec<Option<usize>>) {
    let mut strides = Vec::with_capacity(maps.len());
    let mut s = 1;
    for k in 0..maps.len() {
        strides.push(s);
        s *= src.get(k).copied().unwrap_or(1);
    }
    let dims: Dims = maps.iter().map(|m| m.len()).collect();
    let total = nd::numel(&dims);
    let mut out = Vec::with_capacity(total);
    let mut sub = vec![0usize; maps.len()];
    for _ in 0..total {
        let mut pos = Some(0);
        for (k, &j) in sub.iter().enumerate() {
            pos = match (pos, maps[k][j]) {
                (Some(p), Some(i)) => Some(p + i * strides[k]),
                _ => None,
            };
        }
        out.push(pos);
        for k in 0..sub.len() {
            sub[k] += 1;
            if sub[k] < dims[k] {
                break;
            }
            sub[k] = 0;
        }
    }
    (nd::normalize(dims), out)
}

/// The elements of `v` at `pos` shaped as `dims`; `None` positions get `fill` (or the class's
/// default: zero, `[]` in cells, `<missing>` in strings).
fn take(name: &str, v: &Value, pos: &[Option<usize>], dims: Dims, fill: Option<&Matrix>) -> R<Value> {
    Ok(match v {
        Value::Num(m) => {
            let (fr, fi) = fill.map(|f| (f.data[0], f.im_at(0))).unwrap_or((0.0, 0.0));
            let (fr, fi) = (m.kind.cast(fr), if m.kind == Kind::Logical { 0.0 } else { m.kind.cast(fi) });
            let data = pos.iter().map(|p| p.map(|i| m.data[i]).unwrap_or(fr)).collect();
            let mut out = Matrix::from_dims(dims, data).with_kind(m.kind);
            if m.is_complex() || fi != 0.0 {
                out.im = Some(pos.iter().map(|p| p.map(|i| m.im_at(i)).unwrap_or(fi)).collect());
                out.narrow();
            }
            Value::Num(out)
        }
        Value::Cell(c) => Value::Cell(CellArray::from_dims(dims, pos.iter().map(|p| p.map(|i| c.data[i].clone()).unwrap_or_else(Value::empty)).collect())),
        Value::Str(s) => Value::Str(StrArray::from_dims(dims, pos.iter().map(|p| p.and_then(|i| s.data[i].clone())).collect())),
        Value::Struct(s) => {
            let elems = pos.iter().map(|p| p.map(|i| s.elems[i].clone()).unwrap_or_else(|| s.empty_elem())).collect();
            Value::Struct(StructArray { dims: nd::normalize(dims), fields: s.fields.clone(), elems })
        }
        other => return rt(format!("Undefined function '{name}' for input arguments of type '{}'.", other.class_name())),
    })
}

fn identity_maps(d: &[usize], n: usize) -> Vec<Vec<Option<usize>>> {
    (0..n).map(|k| (0..d.get(k).copied().unwrap_or(1)).map(Some).collect()).collect()
}

fn is_vector_shape(d: &[usize]) -> bool {
    d.len() == 2 && (d[0] == 1 || d[1] == 1)
}

/// Class of a concatenation of the given classes (integers win, then char, single, double).
fn concat_kind(kinds: &[Kind]) -> Kind {
    if let Some(k) = kinds.iter().find(|k| k.is_int()) {
        return *k;
    }
    if kinds.contains(&Kind::Char) {
        Kind::Char
    } else if kinds.contains(&Kind::Single) {
        Kind::Single
    } else if !kinds.is_empty() && kinds.iter().all(|&k| k == Kind::Logical) {
        Kind::Logical
    } else {
        Kind::Double
    }
}

fn int_scalar(v: &Value) -> Option<f64> {
    match v {
        Value::Num(m) if m.numel() == 1 && !m.is_complex() && m.data[0].fract() == 0.0 => Some(m.data[0]),
        _ => None,
    }
}

/// A real double matrix built from `f(i, j)` (zero-based).
fn build(r: usize, c: usize, f: impl Fn(usize, usize) -> f64) -> Matrix {
    let mut data = Vec::with_capacity(r * c);
    for j in 0..c {
        for i in 0..r {
            data.push(f(i, j));
        }
    }
    Matrix::new(r, c, data)
}

fn build_c(r: usize, c: usize, f: impl Fn(usize, usize) -> C) -> Matrix {
    let (mut re, mut im) = (Vec::with_capacity(r * c), Vec::with_capacity(r * c));
    for j in 0..c {
        for i in 0..r {
            let z = f(i, j);
            re.push(z.re);
            im.push(z.im);
        }
    }
    Matrix::complex(vec![r, c], re, im)
}

fn elems_c(m: &Matrix) -> Vec<C> {
    (0..m.numel()).map(|i| C::new(m.data[i], m.im_at(i))).collect()
}

/// Trailing class-name argument of the matrix constructors (`hilb(3, 'single')`).
fn class_name(a: &[Value], at: usize, allowed: &[&str], msg: &str) -> R<Option<Kind>> {
    match a.get(at) {
        None => Ok(None),
        Some(v) => {
            let t = v.text().ok().filter(|_| v.is_text());
            match t {
                Some(t) if allowed.is_empty() || allowed.contains(&t.as_str()) => match Kind::from_name(&t) {
                    Some(k) => Ok(Some(k)),
                    None => rt(msg),
                },
                _ => rt(msg),
            }
        }
    }
}

/// Order argument `n` of the matrix constructors, which must be a whole number.
fn order(a: &[Value], name: &str) -> R<usize> {
    let v = arg(a, 0, name)?.scalar()?;
    if v.fract() != 0.0 && v.is_finite() {
        return rt("Size inputs must be integers.");
    }
    Ok(if v > 0.0 { v as usize } else { 0 })
}

fn binom(n: usize, k: usize) -> f64 {
    if k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    let mut r = 1.0;
    for i in 0..k {
        r = r * (n - i) as f64 / (i + 1) as f64;
    }
    r.round()
}

// ---- entry point ----------------------------------------------------------------------------

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match name {
        "repelem" => one(repelem(a)?),
        "freqspace" => freqspace(a, nargout),
        "isequaln" | "isequalwithequalnans" => {
            if a.len() < 2 {
                return rt("Not enough input arguments.");
            }
            one(Value::bool(a[1..].iter().all(|b| equal_nan(&a[0], b))))
        }
        "resize" | "paddata" | "trimdata" => one(resize(name, a)?),
        "blkdiag" => one(blkdiag(a)?),
        "rot90" => one(rot90(a)?),
        "sub2ind" => one(sub2ind(a)?),
        "ind2sub" => Ok(Some(ind2sub(a, nargout)?)),
        "bsxfun" => one(bsxfun(it, a)?),
        "ipermute" => one(ipermute(it, a)?),
        "shiftdim" => shiftdim(it, a, nargout),
        "flipdim" => {
            if a.len() < 2 {
                return rt("Requires two arguments.");
            }
            match int_scalar(&a[1]) {
                Some(d) if d >= 1.0 => crate::builtins::call(it, "flip", a.to_vec(), 1),
                _ => rt("DIM must be a positive integer."),
            }
        }
        "maxNumCompThreads" => one(max_threads(a)?),
        "compan" => one(Value::Num(compan(a)?)),
        "gallery" => gallery(a, nargout),
        "hadamard" => one(Value::Num(hadamard(a)?)),
        "hankel" => one(Value::Num(hankel(it, a)?)),
        "toeplitz" => one(Value::Num(toeplitz(it, a)?)),
        "hilb" | "invhilb" | "wilkinson" if a.len() > 2 => rt("Too many input arguments."),
        "pascal" if a.len() > 3 => rt("Too many input arguments."),
        "hilb" => {
            let n = arg(a, 0, name)?.scalar()?.floor().max(0.0) as usize;
            let kind = class_name(a, 1, &["double", "single"], "CLASSNAME must be 'double' or 'single'.")?;
            one(Value::Num(hilb(n).cast_to(kind.unwrap_or(Kind::Double))))
        }
        "invhilb" => {
            let n = order(a, name)?;
            let kind = class_name(a, 1, &["double", "single"], "CLASSNAME must be 'double' or 'single'.")?;
            one(Value::Num(invhilb(n).cast_to(kind.unwrap_or(Kind::Double))))
        }
        "magic" => {
            if a.len() > 1 {
                return rt("Too many input arguments.");
            }
            let v = arg(a, 0, name)?;
            if v.numel() != 1 {
                return rt("Invalid argument at position 1. Value must be a scalar.");
            }
            let n = v.scalar()?;
            one(Value::Num(magic(if n >= 1.0 { n as usize } else { 0 })))
        }
        "pascal" => {
            let n = order(a, name)?;
            let (k, cls_at) = match a.get(1) {
                Some(v) if !v.is_text() => (v.scalar()?, 2),
                _ => (0.0, 1),
            };
            if k != 0.0 && k != 1.0 && k != 2.0 {
                return rt("Second argument must be 0, 1, or 2.");
            }
            let kind = class_name(a, cls_at, &["double", "single"], "CLASSNAME must be 'double' or 'single'.")?;
            one(Value::Num(pascal(n, k as u8).cast_to(kind.unwrap_or(Kind::Double))))
        }
        "rosser" => {
            if a.len() > 1 {
                return rt("Too many input arguments.");
            }
            let kind = match a.first() {
                Some(v) if !v.is_text() => return rt("Second argument must be either a character row vector or a string scalar."),
                _ => class_name(a, 0, &["double", "single"], "CLASSNAME must be 'double' or 'single'.")?,
            };
            one(Value::Num(Matrix::new(8, 8, ROSSER.to_vec()).cast_to(kind.unwrap_or(Kind::Double))))
        }
        "vander" => {
            if a.len() > 1 {
                return rt("Too many input arguments.");
            }
            one(Value::Num(vander(&m_arg(a, 0, name)?)?))
        }
        "wilkinson" => {
            let n = order(a, name)?;
            let kind = class_name(a, 1, &["double", "single"], "CLASSNAME must be 'double' or 'single'.")?;
            one(Value::Num(wilkinson(n).cast_to(kind.unwrap_or(Kind::Double))))
        }
        _ => unreachable!(),
    }
}

// ---- replication and resizing ---------------------------------------------------------------

fn rep_factors(v: &Value) -> R<Vec<usize>> {
    let m = match v {
        Value::Num(m) if !m.is_complex() => m,
        _ => return rt("Replication factors must be nonnegative integer-valued scalars or vectors."),
    };
    if !(m.is_empty() || is_vector_shape(&m.dims)) || m.data.iter().any(|&x| x < 0.0 || x.fract() != 0.0 || !x.is_finite()) {
        return rt("Replication factors must be nonnegative integer-valued scalars or vectors.");
    }
    Ok(m.data.iter().map(|&x| x as usize).collect())
}

fn repeat_map(len: usize, f: &[usize]) -> Vec<Option<usize>> {
    let mut out = Vec::new();
    for i in 0..len {
        let r = if f.len() == 1 { f[0] } else { f[i] };
        out.extend(std::iter::repeat_n(Some(i), r));
    }
    out
}

fn repelem(a: &[Value]) -> R<Value> {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    let v = &a[0];
    let d = v.dims();
    if a.len() == 2 {
        if !is_vector_shape(&d) {
            return rt("With two inputs, the first argument must be a vector. Use three-input syntax for matrices.");
        }
        let f = rep_factors(&a[1])?;
        let n = nd::numel(&d);
        if f.len() != 1 && f.len() != n {
            return rt("In repelem(v, N), N must be a scalar or a vector of length equal to v.");
        }
        let map = repeat_map(n, &f);
        let len = map.len();
        let dims = if d[1] == 1 && d[0] != 1 { vec![len, 1] } else { vec![1, len] };
        return take("repelem", v, &map, dims, None);
    }
    let k = a.len() - 1;
    let n = k.max(d.len());
    let mut maps = identity_maps(&d, n);
    for j in 0..k {
        let f = rep_factors(&a[j + 1])?;
        let len = d.get(j).copied().unwrap_or(1);
        if f.len() != 1 && f.len() != len {
            return rt("In repelem(A, N1, N2, ..., Nn), Nj must be a scalar or a vector of length size(A, j).");
        }
        maps[j] = repeat_map(len, &f);
    }
    let (dims, pos) = plan(&d, &maps);
    take("repelem", v, &pos, dims, None)
}

#[derive(Clone, Copy, PartialEq)]
enum Pattern {
    Constant,
    Edge,
    Circular,
    Flip,
    Reflect,
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Trailing,
    Leading,
    Both,
}

/// Source subscript of padded position `j` (relative to the first original element).
fn pad_source(j: i64, s: usize, pat: Pattern) -> Option<usize> {
    let s = s as i64;
    if (0..s).contains(&j) {
        return Some(j as usize);
    }
    Some(match pat {
        Pattern::Constant => return None,
        Pattern::Edge => j.clamp(0, s - 1),
        Pattern::Circular => j.rem_euclid(s),
        Pattern::Flip => {
            let r = j.rem_euclid(2 * s);
            if r < s { r } else { 2 * s - 1 - r }
        }
        Pattern::Reflect => {
            if s == 1 {
                0
            } else {
                let r = j.rem_euclid(2 * s - 2);
                if r < s { r } else { 2 * s - 2 - r }
            }
        }
    } as usize)
}

/// `resize`, `paddata` and `trimdata`, with name-value options given as `'Name', value`.
fn resize(name: &str, a: &[Value]) -> R<Value> {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    let v = &a[0];
    let size_err = "Second argument must be a finite nonnegative integer scalar or vector of nonnegative integers.";
    let target: Vec<usize> = match &a[1] {
        Value::Num(m) if !m.is_complex() && !m.is_empty() && is_vector_shape(&m.dims) => {
            if m.data.iter().any(|&x| x < 0.0 || x.fract() != 0.0 || !x.is_finite()) {
                return rt(size_err);
            }
            m.data.iter().map(|&x| x as usize).collect()
        }
        _ => return rt(size_err),
    };
    let (can_pad, can_trim) = match name {
        "paddata" => (true, false),
        "trimdata" => (false, true),
        _ => (true, true),
    };
    let names_msg = if can_pad { "Parameter name must be 'Dimension', 'FillValue', 'Pattern', or 'Side'." } else { "Parameter name must be 'Dimension' or 'Side'." };
    let (mut dims_opt, mut fill, mut pat, mut side) = (None, None, Pattern::Constant, Side::Trailing);
    let mut i = 2;
    while i < a.len() {
        let key = if a[i].is_text() { a[i].text()?.to_ascii_lowercase() } else { return rt(names_msg) };
        let Some(val) = a.get(i + 1) else { return rt(names_msg) };
        match key.as_str() {
            "dimension" => {
                let m = val.to_matrix()?;
                if m.is_empty() || m.data.iter().any(|&x| x < 1.0 || x.fract() != 0.0) {
                    return rt("Dimension must be a positive integer scalar or vector of positive integers.");
                }
                dims_opt = Some(m.data.iter().map(|&x| x as usize).collect::<Vec<_>>());
            }
            "fillvalue" if can_pad => {
                let m = val.to_matrix()?;
                if m.numel() != 1 {
                    return rt("Fill value must be scalar when the first argument is an array.");
                }
                fill = Some(m);
            }
            "pattern" if can_pad => {
                pat = match val.text().map(|t| t.to_ascii_lowercase()).as_deref() {
                    Ok("constant") => Pattern::Constant,
                    Ok("edge") => Pattern::Edge,
                    Ok("circular") => Pattern::Circular,
                    Ok("flip") => Pattern::Flip,
                    Ok("reflect") => Pattern::Reflect,
                    _ => return rt("Pattern must be 'constant', 'edge', 'circular', 'flip', or 'reflect'."),
                }
            }
            "side" => {
                side = match val.text().map(|t| t.to_ascii_lowercase()).as_deref() {
                    Ok("trailing") => Side::Trailing,
                    Ok("leading") => Side::Leading,
                    Ok("both") => Side::Both,
                    _ => return rt("Side must be 'trailing', 'leading', or 'both'."),
                }
            }
            _ => return rt(names_msg),
        }
        i += 2;
    }
    let d = v.dims();
    let dims: Vec<usize> = match dims_opt {
        Some(ds) => {
            if target.len() != 1 && target.len() != ds.len() {
                return rt("Second argument must be a scalar or have the same number of elements as 'Dimension'.");
            }
            ds
        }
        None if target.len() == 1 => {
            // Vectors (and scalars) resize along their length, matrices along rows, N-D arrays
            // along the first non-singleton dimension.
            vec![if is_vector_shape(&d) { if d[1] == 1 && d[0] != 1 { 1 } else { 2 } } else if d.len() == 2 { 1 } else { default_dim(&d) }]
        }
        None => (1..=target.len()).collect(),
    };
    if pat != Pattern::Constant && nd::numel(&d) == 0 {
        return rt("When the first argument is empty, the pattern must be constant.");
    }
    let n = d.len().max(dims.iter().copied().max().unwrap_or(0));
    let mut maps = identity_maps(&d, n);
    for (k, &dim) in dims.iter().enumerate() {
        let t = if target.len() == 1 { target[0] } else { target[k] };
        let s = d.get(dim - 1).copied().unwrap_or(1);
        let split = |c: usize| match side {
            Side::Trailing => 0,
            Side::Leading => c,
            Side::Both => c / 2,
        };
        if t > s && can_pad {
            let lead = split(t - s) as i64;
            maps[dim - 1] = (0..t as i64).map(|j| pad_source(j - lead, s, pat)).collect();
        } else if t < s && can_trim {
            let lead = split(s - t);
            maps[dim - 1] = (lead..lead + t).map(Some).collect();
        }
    }
    let (out_dims, pos) = plan(&d, &maps);
    take(name, v, &pos, out_dims, fill.as_ref())
}

// ---- rearrangement --------------------------------------------------------------------------

fn rot90(a: &[Value]) -> R<Value> {
    let v = arg(a, 0, "rot90")?;
    let k = match a.get(1) {
        None => 1,
        Some(x) => {
            let m = x.to_matrix()?;
            if m.numel() != 1 {
                return rt("k must be a scalar.");
            }
            if m.data[0].fract() != 0.0 {
                return rt("k must be an integer.");
            }
            (m.data[0] as i64).rem_euclid(4)
        }
    };
    let d = v.dims();
    let (r, c) = (d[0], d[1]);
    let page = r * c;
    let pages = nd::numel(&d[2..]);
    let (or, oc) = if k % 2 == 1 { (c, r) } else { (r, c) };
    let mut pos = Vec::with_capacity(page * pages);
    for p in 0..pages {
        for j in 0..oc {
            for i in 0..or {
                let (si, sj) = match k {
                    0 => (i, j),
                    1 => (j, c - 1 - i),
                    2 => (r - 1 - i, c - 1 - j),
                    _ => (r - 1 - j, i),
                };
                pos.push(Some(p * page + sj * r + si));
            }
        }
    }
    let mut dims = d.clone();
    dims[0] = or;
    dims[1] = oc;
    take("rot90", v, &pos, dims, None)
}

fn ipermute(it: &mut Interpreter, a: &[Value]) -> R<Value> {
    let v = arg(a, 0, "ipermute")?.clone();
    let order = m_arg(a, 1, "ipermute")?;
    if order.data.iter().any(|&x| x < 1.0 || x.fract() != 0.0) {
        return rt("ORDER contains an invalid permutation index.");
    }
    let len = order.data.iter().fold(0.0f64, |m, &x| m.max(x)) as usize;
    let mut inv = vec![0usize; len];
    for (k, &o) in order.data.iter().enumerate() {
        inv[o as usize - 1] = k + 1;
    }
    if inv.len() < v.dims().len() {
        return rt("ORDER must have at least N elements for an N-D array.");
    }
    if inv.contains(&0) {
        return rt("ORDER contains an invalid permutation index.");
    }
    let inv = Matrix::row(inv.iter().map(|&x| x as f64).collect());
    permute(it, v, inv)
}

fn permute(it: &mut Interpreter, v: Value, order: Matrix) -> R<Value> {
    let out = crate::builtins::call(it, "permute", vec![v, Value::Num(order)], 1)?;
    Ok(out.and_then(|o| o.into_iter().next()).unwrap_or_else(Value::empty))
}

fn shiftdim(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let v = arg(a, 0, "shiftdim")?.clone();
    let d = v.dims();
    let shift = match a.get(1) {
        Some(s) => match int_scalar(s) {
            Some(n) => n as i64,
            None => return rt("ORDER must have at least N elements for an N-D array."),
        },
        // Leading singleton dimensions, unless every dimension is a singleton.
        None => {
            let lead = d.iter().take_while(|&&x| x == 1).count();
            if lead == d.len() { 0 } else { lead as i64 }
        }
    };
    let out = if shift > 0 {
        let n = d.len();
        let s = shift as usize % n;
        let order: Vec<f64> = (s..n).chain(0..s).map(|k| (k + 1) as f64).collect();
        permute(it, v, Matrix::row(order))?
    } else if shift < 0 {
        let mut dims = vec![1; (-shift) as usize];
        dims.extend(d);
        reshape(v, dims)?
    } else {
        v
    };
    let mut r = vec![out];
    if nargout > 1 {
        r.push(Value::num(shift as f64));
    }
    Ok(Some(r))
}

fn reshape(v: Value, dims: Dims) -> R<Value> {
    let dims = nd::normalize(dims);
    Ok(match v {
        Value::Num(m) => Value::Num(m.reshaped(dims)),
        Value::Cell(c) => Value::Cell(CellArray::from_dims(dims, c.data)),
        Value::Str(s) => Value::Str(StrArray::from_dims(dims, s.data)),
        Value::Struct(s) => Value::Struct(StructArray { dims, ..s }),
        other => return rt(format!("Undefined function 'shiftdim' for input arguments of type '{}'.", other.class_name())),
    })
}

fn blkdiag(a: &[Value]) -> R<Value> {
    let mut ms = Vec::with_capacity(a.len());
    for v in a {
        match v {
            Value::Num(m) => {
                if m.dims.len() > 2 {
                    return rt("Input arrays must be 2-dimensional.");
                }
                ms.push(m);
            }
            other => {
                return rt(format!(
                    "Inconsistent concatenation dimensions because a 1-by-2 'double' array was converted to a 1-by-1 '{}' array. Consider creating arrays of the same type before concatenating.",
                    other.class_name()
                ))
            }
        }
    }
    let kind = concat_kind(&ms.iter().map(|m| m.kind).collect::<Vec<_>>());
    let rows: usize = ms.iter().map(|m| m.rows()).sum();
    let cols: usize = ms.iter().map(|m| m.cols()).sum();
    let mut out = Matrix::zeros(rows, cols);
    let complex = ms.iter().any(|m| m.is_complex());
    let mut im = vec![0.0; rows * cols];
    let (mut r0, mut c0) = (0, 0);
    for m in &ms {
        for j in 0..m.cols() {
            for i in 0..m.rows() {
                let p = (c0 + j) * rows + r0 + i;
                out.data[p] = m.get(i, j);
                im[p] = m.im_at(j * m.rows() + i);
            }
        }
        r0 += m.rows();
        c0 += m.cols();
    }
    if complex {
        out.im = Some(im);
    }
    Ok(Value::Num(out.cast_to(kind)))
}

// ---- indices --------------------------------------------------------------------------------

fn size_vector(v: &Value) -> R<Vec<f64>> {
    Ok(v.to_matrix()?.data)
}

fn sub2ind(a: &[Value]) -> R<Value> {
    let mut siz = size_vector(arg(a, 0, "sub2ind")?)?;
    if siz.len() < 2 {
        return rt("Size vector must have at least 2 elements.");
    }
    let subs = &a[1..];
    if subs.is_empty() {
        return rt("Array indices must be positive integers or logical values.");
    }
    let k = subs.len();
    if k < siz.len() {
        let folded: f64 = siz[k - 1..].iter().product();
        siz.truncate(k - 1);
        siz.push(folded);
    }
    siz.resize(k.max(siz.len()), 1.0);
    let ms = subs.iter().map(|s| s.to_matrix()).collect::<R<Vec<_>>>()?;
    let dims = ms[0].dims.clone();
    if ms.iter().any(|m| m.dims != dims) {
        return rt("The subscript vectors must all be of the same size.");
    }
    for (m, &s) in ms.iter().zip(&siz) {
        if m.data.iter().any(|&x| x < 1.0 || x > s) {
            return rt("Out of range subscript.");
        }
    }
    let mut out = ms[0].data.clone();
    let mut stride = 1.0;
    // ndx = s1 + (s2-1)*siz(1) + (s3-1)*siz(1)*siz(2) + ...
    for j in 1..k {
        stride *= siz[j - 1];
        for (o, &s) in out.iter_mut().zip(&ms[j].data) {
            *o += (s - 1.0) * stride;
        }
    }
    Ok(Value::Num(Matrix::from_dims(dims, out)))
}

fn ind2sub(a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let mut siz = size_vector(arg(a, 0, "ind2sub")?)?;
    let ndx = m_arg(a, 1, "ind2sub")?;
    let ndx = Matrix::from_dims(ndx.dims.clone(), ndx.data.clone());
    let nout = nargout.max(1);
    if nout == 1 {
        return Ok(vec![Value::Num(ndx)]);
    }
    if nout < siz.len() {
        let folded: f64 = siz[nout - 1..].iter().product();
        siz.truncate(nout - 1);
        siz.push(folded);
    }
    siz.resize(nout, 1.0);
    let mut cum = vec![1.0; nout];
    for k in 1..nout {
        cum[k] = cum[k - 1] * siz[k - 1];
    }
    let mut rest = ndx.data.clone();
    let mut outs = vec![Vec::new(); nout];
    for k in (1..nout).rev() {
        let mut vj = Vec::with_capacity(rest.len());
        for x in rest.iter_mut() {
            let vi = (*x - 1.0) % cum[k] + 1.0;
            vj.push((*x - vi) / cum[k] + 1.0);
            *x = vi;
        }
        outs[k] = vj;
    }
    outs[0] = rest;
    Ok(outs.into_iter().map(|o| Value::Num(Matrix::from_dims(ndx.dims.clone(), o))).collect())
}

fn bsxfun(it: &mut Interpreter, a: &[Value]) -> R<Value> {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if !matches!(a[0], Value::Func(_)) {
        return rt("First Argument must be a function handle.");
    }
    let (x, y) = match (&a[1], &a[2]) {
        (Value::Num(x), Value::Num(y)) => (x, y),
        _ => return rt("Operands must be numeric arrays."),
    };
    let n = x.dims.len().max(y.dims.len());
    let (mut dx, mut dy) = (x.dims.clone(), y.dims.clone());
    dx.resize(n, 1);
    dy.resize(n, 1);
    let mut target = Vec::with_capacity(n);
    for k in 0..n {
        target.push(match (dx[k], dy[k]) {
            (p, q) if p == q => p,
            (1, q) => q,
            (p, 1) => p,
            _ => return rt("Non-singleton dimensions of the two input arrays must match each other."),
        });
    }
    let expand = |v: &Value, d: &[usize]| -> R<Value> {
        if d == target.as_slice() {
            return Ok(v.clone());
        }
        let maps: Vec<Vec<Option<usize>>> = (0..n).map(|k| (0..target[k]).map(|j| Some(if d[k] == 1 { 0 } else { j })).collect()).collect();
        let (dims, pos) = plan(d, &maps);
        take("bsxfun", v, &pos, dims, None)
    };
    let (ex, ey) = (expand(&a[1], &dx)?, expand(&a[2], &dy)?);
    let out = it.call_func_value(&a[0], vec![ex, ey], 1)?;
    let Some(r) = out.into_iter().next() else { return rt("Specified function handle produces invalid output dimensions. The function handle must be a binary elementwise function.") };
    if r.dims() != nd::normalize(target) {
        return rt("Specified function handle produces invalid output dimensions. The function handle must be a binary elementwise function.");
    }
    Ok(r)
}

// ---- comparison, frequency spacing, threads -------------------------------------------------

/// `isequal` with NaN equal to NaN (and missing strings equal to each other).
fn equal_nan(a: &Value, b: &Value) -> bool {
    let same = |x: f64, y: f64| x == y || (x.is_nan() && y.is_nan());
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x.dims == y.dims && (0..x.numel()).all(|i| same(x.data[i], y.data[i]) && same(x.im_at(i), y.im_at(i))),
        (Value::Cell(x), Value::Cell(y)) => x.dims == y.dims && x.data.iter().zip(&y.data).all(|(p, q)| equal_nan(p, q)),
        (Value::Struct(x), Value::Struct(y)) => {
            x.dims == y.dims
                && x.fields.len() == y.fields.len()
                && x.fields.iter().all(|f| y.field_index(f).is_some())
                && (0..x.elems.len()).all(|i| x.fields.iter().all(|f| equal_nan(x.get_at(i, f).unwrap(), y.get_at(i, f).unwrap())))
        }
        (Value::Str(x), Value::Str(y)) => x.dims == y.dims && x.data == y.data,
        _ => values_equal(a, b),
    }
}

fn freqspace(a: &[Value], nargout: usize) -> Ret {
    let n = m_arg(a, 0, "freqspace")?;
    let flag = a.len() > 1;
    // Points of a centered 2-D grid: ((0:n-1) - floor(n/2)) * (2/n).
    let centered = |n: f64| -> Matrix {
        let len = n.max(0.0).floor() as usize;
        let mid = if len % 2 == 1 { (n - 1.0) / 2.0 } else { n / 2.0 };
        Matrix::row((0..len).map(|k| (k as f64 - mid) * (2.0 / n)).collect())
    };
    if nargout <= 1 {
        if n.numel() != 1 {
            return rt("Matrix dimensions must agree.");
        }
        let n = n.data[0];
        let step = 2.0 / n;
        let stop = if flag { 2.0 * (n - 1.0) / n } else { 1.0 };
        let count = if n <= 0.0 || !step.is_finite() { 0 } else { ((stop / step) * (1.0 + 1e-15)).floor() as usize + 1 };
        return one(Value::Num(Matrix::row((0..count).map(|k| k as f64 * step).collect())));
    }
    let (rows, cols) = if n.numel() >= 2 { (n.data[0], n.data[1]) } else { (n.data[0], n.data[0]) };
    let (f1, f2) = (centered(cols), centered(rows));
    if flag {
        let (nx, ny) = (f1.numel(), f2.numel());
        let x = build(ny, nx, |_, j| f1.data[j]);
        let y = build(ny, nx, |i, _| f2.data[i]);
        return Ok(Some(vec![Value::Num(x), Value::Num(y)]));
    }
    Ok(Some(vec![Value::Num(f1), Value::Num(f2)]))
}

/// 0 means "automatic" (the machine's core count).
static THREADS: AtomicUsize = AtomicUsize::new(0);

fn max_threads(a: &[Value]) -> R<Value> {
    if a.len() > 1 {
        return rt("Too many input arguments.");
    }
    let auto = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let cur = match THREADS.load(Ordering::Relaxed) {
        0 => auto,
        n => n,
    };
    if let Some(v) = a.first() {
        let bad = "Input to maxNumCompThreads must be a positive numeric integer-valued scalar or 'automatic'.";
        if v.is_text() {
            if !v.text()?.eq_ignore_ascii_case("automatic") {
                return rt(bad);
            }
            THREADS.store(0, Ordering::Relaxed);
        } else {
            match int_scalar(v) {
                Some(n) if n >= 1.0 => THREADS.store(n as usize, Ordering::Relaxed),
                _ => return rt(bad),
            }
        }
    }
    Ok(Value::num(cur as f64))
}

// ---- specialized matrices -------------------------------------------------------------------

fn compan(a: &[Value]) -> R<Matrix> {
    let u = m_arg(a, 0, "compan")?;
    if u.is_empty() || !is_vector_shape(&u.dims) {
        return rt("Input argument must be a vector.");
    }
    let n = u.numel();
    if n <= 1 {
        return Ok(Matrix::empty().with_kind(u.kind));
    }
    let tail = Matrix { dims: vec![1, n - 1], data: u.data[1..].iter().map(|x| -x).collect(), im: u.im.as_ref().map(|im| im[1..].iter().map(|x| -x).collect()), kind: u.kind };
    let head = Matrix { dims: vec![1, 1], data: vec![u.data[0]], im: u.im.as_ref().map(|im| vec![im[0]]), kind: u.kind };
    let row = ops::num_binary(BinOp::Div, &tail, &head)?;
    let m = n - 1;
    let mut out = Matrix::zeros(m, m);
    let mut im = vec![0.0; m * m];
    // MATLAB's complex quotients carry +0 where this division leaves -0.
    let z = if row.is_complex() { 0.0 } else { -0.0 };
    for j in 0..m {
        out.data[j * m] = row.data[j] + z;
        im[j * m] = row.im_at(j) + 0.0;
    }
    for i in 1..m {
        out.data[(i - 1) * m + i] = 1.0;
    }
    if row.is_complex() {
        out.im = Some(im);
    }
    Ok(out.with_kind(row.kind))
}

fn hadamard(a: &[Value]) -> R<Matrix> {
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let n = arg(a, 0, "hadamard")?.scalar()?;
    let kind = class_name(a, 1, &[], "Invalid class name.")?.unwrap_or(Kind::Double);
    let bad = "n must be an integer and n, n/12 or n/20 must be a power of 2.";
    if n.is_nan() || n < 1.0 || n.fract() != 0.0 {
        return rt(bad);
    }
    let pow2 = |x: f64| x >= 1.0 && x.fract() == 0.0 && (x as u64).is_power_of_two();
    let mut h = if pow2(n) {
        Matrix::scalar(1.0)
    } else if pow2(n / 12.0) {
        let c = [-1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0];
        let r = [-1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0];
        build(12, 12, |i, j| if i == 0 || j == 0 { 1.0 } else if i >= j { c[i - j] } else { r[j - i] })
    } else if pow2(n / 20.0) {
        // The core is a Hankel matrix: first column, then the rest of its last row.
        let s = [
            -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0,
            -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0,
        ];
        build(20, 20, |i, j| if i == 0 || j == 0 { 1.0 } else { s[i + j - 2] })
    } else {
        return rt(bad);
    };
    while (h.rows() as f64) < n {
        let k = h.rows();
        let prev = h.clone();
        h = build(2 * k, 2 * k, |i, j| {
            let v = prev.get(i % k, j % k);
            if i >= k && j >= k { -v } else { v }
        });
    }
    Ok(h.cast_to(kind))
}

/// Arguments of hankel/toeplitz as flattened column/row vectors and the result class.
type ColRow = (Vec<C>, Option<Vec<C>>, Kind, bool);

fn col_row(a: &[Value], name: &str) -> R<ColRow> {
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let c = m_arg(a, 0, name)?;
    let r = if a.len() > 1 { Some(m_arg(a, 1, name)?) } else { None };
    let mut kinds = vec![c.kind];
    let mut complex = c.is_complex();
    if let Some(r) = &r {
        kinds.push(r.kind);
        complex |= r.is_complex();
    }
    Ok((elems_c(&c), r.as_ref().map(elems_c), concat_kind(&kinds), complex))
}

fn finish(m: Matrix, kind: Kind, complex: bool) -> Matrix {
    let mut m = m.cast_to(kind);
    if !complex {
        m.im = None;
    }
    m
}

fn hankel(it: &mut Interpreter, a: &[Value]) -> R<Matrix> {
    if a.len() == 1 && a[0].is_char() {
        return rt("Argument following 'like' must be a variable with a data type that supports ZEROS, such as a double or single variable.");
    }
    let (c, r, kind, complex) = col_row(a, "hankel")?;
    let nc = c.len();
    let r = match r {
        Some(r) => {
            if let (Some(last), Some(first)) = (c.last(), r.first()) {
                if last != first {
                    it.emit(OutputKind::Warning("Last element of input column does not match first element of input row. \n         Column wins anti-diagonal conflict.".into()));
                }
            }
            r
        }
        None => vec![C::new(0.0, 0.0); nc],
    };
    let mut x = c.clone();
    x.extend(r.iter().skip(1));
    let m = build_c(nc, r.len(), |i, j| x[i + j]);
    Ok(finish(m, kind, complex))
}

fn toeplitz(it: &mut Interpreter, a: &[Value]) -> R<Matrix> {
    for v in a {
        if let Value::Num(m) = v {
            if m.kind == Kind::Char || m.kind == Kind::Logical {
                return rt("Inputs must be numeric.");
            }
        }
    }
    let (c, r, kind, complex) = col_row(a, "toeplitz")?;
    let (c, r) = match r {
        Some(r) => {
            if let (Some(c0), Some(r0)) = (c.first(), r.first()) {
                if c0 != r0 {
                    it.emit(OutputKind::Warning("First element of input column does not match first element of input row. \n         Column wins diagonal conflict.".into()));
                }
            }
            (c, r)
        }
        None => {
            // toeplitz(r) is Hermitian: the column is the conjugated row.
            if c.is_empty() {
                return rt("Index exceeds the number of array elements. Index must not exceed 0.");
            }
            let mut col: Vec<C> = c.iter().map(|z| C::new(z.re, -z.im + 0.0)).collect();
            col[0] = c[0];
            (col, c)
        }
    };
    let m = build_c(c.len(), r.len(), |i, j| if i >= j { c[i - j] } else { r[j - i] });
    Ok(finish(m, kind, complex))
}

fn hilb(n: usize) -> Matrix {
    build(n, n, |i, j| 1.0 / (i + j + 1) as f64)
}

fn invhilb(n: usize) -> Matrix {
    let mut h = Matrix::zeros(n, n);
    let nf = n as f64;
    let mut p = nf;
    for i in 1..=n {
        let fi = i as f64;
        let mut r = p * p;
        h.set(i - 1, i - 1, r / (2.0 * fi - 1.0));
        for j in i + 1..=n {
            let fj = j as f64;
            r = -((nf - fj + 1.0) * r * (nf + fj - 1.0)) / ((fj - 1.0) * (fj - 1.0));
            h.set(i - 1, j - 1, r / (fi + fj - 1.0));
            h.set(j - 1, i - 1, r / (fi + fj - 1.0));
        }
        p = ((nf - fi) * p * (nf + fi)) / (fi * fi);
    }
    h
}

fn magic(n: usize) -> Matrix {
    if n == 0 {
        return Matrix::empty();
    }
    if n % 2 == 1 {
        let nf = n as f64;
        return build(n, n, |i, j| {
            let (fi, fj) = ((i + 1) as f64, (j + 1) as f64);
            let a = (fi + fj - (nf + 3.0) / 2.0).rem_euclid(nf);
            let b = (fi + 2.0 * fj - 2.0).rem_euclid(nf);
            nf * a + b + 1.0
        });
    }
    if n.is_multiple_of(4) {
        let nn = (n * n) as f64;
        return build(n, n, |i, j| {
            let v = (i * n + j + 1) as f64;
            if ((i + 1) % 4) / 2 == ((j + 1) % 4) / 2 { nn + 1.0 - v } else { v }
        });
    }
    let p = n / 2;
    let q = magic(p);
    let pp = (p * p) as f64;
    let mut m = build(n, n, |i, j| {
        let v = q.get(i % p, j % p);
        v + pp * match (i >= p, j >= p) {
            (false, false) => 0.0,
            (false, true) => 2.0,
            (true, false) => 3.0,
            (true, true) => 1.0,
        }
    });
    if n == 2 {
        return m;
    }
    let k = (n - 2) / 4;
    let swap = |m: &mut Matrix, i: usize, j: usize| {
        let (a, b) = (m.get(i, j), m.get(i + p, j));
        m.set(i, j, b);
        m.set(i + p, j, a);
    };
    let cols: Vec<usize> = (0..k).chain(n - k + 1..n).collect();
    for &j in &cols {
        for i in 0..p {
            swap(&mut m, i, j);
        }
    }
    for j in [0, k] {
        swap(&mut m, k, j);
    }
    m
}

fn pascal(n: usize, k: u8) -> Matrix {
    if k == 0 {
        let mut p = Matrix::filled(n, n, 1.0);
        for i in 1..n {
            for j in 1..n {
                let v = p.get(i - 1, j) + p.get(i, j - 1);
                p.set(i, j, v);
            }
        }
        return p;
    }
    let mut l = Matrix::zeros(n, n);
    for i in 0..n {
        l.set(i, 0, 1.0);
        for j in 1..=i {
            let v = if i == 0 { 0.0 } else { l.get(i - 1, j) - l.get(i - 1, j - 1) };
            l.set(i, j, v);
        }
    }
    if k == 1 {
        return l;
    }
    // rot90(L, 3), negated for even n (which leaves -0 in place of the zeros).
    let neg = n.is_multiple_of(2);
    build(n, n, |i, j| {
        let v = l.get(n - 1 - j, i);
        if neg { -v } else { v }
    })
}

fn vander(v: &Matrix) -> R<Matrix> {
    let n = v.numel();
    let col = Matrix { dims: vec![n, 1], data: v.data.clone(), im: v.im.clone(), kind: v.kind };
    let mut cols = vec![Matrix::filled(n, 1, 1.0).with_kind(v.kind); n];
    for j in (0..n.saturating_sub(1)).rev() {
        cols[j] = ops::num_binary(BinOp::Mul, &col, &cols[j + 1])?;
    }
    let complex = cols.iter().any(|c| c.is_complex());
    let mut out = Matrix::zeros(n, n);
    let mut im = vec![0.0; n * n];
    for (j, c) in cols.iter().enumerate() {
        for i in 0..n {
            out.data[j * n + i] = c.data[i];
            im[j * n + i] = c.im_at(i);
        }
    }
    if complex {
        out.im = Some(im);
    }
    Ok(out.with_kind(v.kind))
}

fn wilkinson(n: usize) -> Matrix {
    let m = (n as f64 - 1.0) / 2.0;
    build(n, n, |i, j| {
        if i == j {
            (i as f64 - m).abs()
        } else if i.abs_diff(j) == 1 {
            1.0
        } else {
            0.0
        }
    })
}

/// Right-hand side of gallery('wilk', 4) (0.7853 is data, not pi/4).
#[allow(clippy::approx_constant)]
const WILK4_B: [f64; 4] = [0.6524, 0.3127, 0.4186, 0.7853];

#[rustfmt::skip]
const ROSSER: [f64; 64] = [
    611.0, 196.0, -192.0, 407.0, -8.0, -52.0, -49.0, 29.0,
    196.0, 899.0, 113.0, -192.0, -71.0, -43.0, -8.0, -44.0,
    -192.0, 113.0, 899.0, 196.0, 61.0, 49.0, 8.0, 52.0,
    407.0, -192.0, 196.0, 611.0, 8.0, 44.0, 59.0, -23.0,
    -8.0, -71.0, 61.0, 8.0, 411.0, -599.0, 208.0, 208.0,
    -52.0, -43.0, 49.0, 44.0, -599.0, 411.0, 208.0, 208.0,
    -49.0, -8.0, 8.0, 59.0, 208.0, 208.0, 99.0, -911.0,
    29.0, -44.0, 52.0, -23.0, 208.0, 208.0, -911.0, 99.0,
];

// ---- gallery --------------------------------------------------------------------------------

/// sin(pi*r) and cos(pi*r) for |r| <= 1/4, with pi*r carried in double-double.
fn sincos_pi_small(r: f64) -> (f64, f64) {
    const PI_LO: f64 = 1.2246467991473532e-16;
    let hi = PI * r;
    let lo = PI.mul_add(r, -hi) + PI_LO * r;
    let (s, c) = hi.sin_cos();
    (s + lo * c, c - lo * s)
}

/// `sin(pi*x)`, exact at multiples of 1/2 and accurate elsewhere (MATLAB's `sinpi`).
fn sinpi(x: f64) -> f64 {
    let r = x - 2.0 * (x / 2.0).round();
    let (sign, r) = if r < 0.0 { (-1.0, -r) } else { (1.0, r) };
    let r = if r > 0.5 { 1.0 - r } else { r };
    let v = if r <= 0.25 { sincos_pi_small(r).0 } else { sincos_pi_small(0.5 - r).1 };
    // Zeros at whole x take the sign of x.
    if v == 0.0 { 0.0f64.copysign(x) } else { sign * v }
}

/// `cos(pi*x)` (MATLAB's `cospi`).
fn cospi(x: f64) -> f64 {
    let r = (x - 2.0 * (x / 2.0).round()).abs();
    let (sign, r) = if r > 0.5 { (-1.0, 1.0 - r) } else { (1.0, r) };
    sign * if r <= 0.25 { sincos_pi_small(r).1 } else { sincos_pi_small(0.5 - r).0 }
}

/// Parameters of a gallery family: numeric scalars and vectors after the name.
struct Params<'a>(&'a [Value]);

impl Params<'_> {
    fn get(&self, i: usize) -> Option<&Value> {
        self.0.get(i).filter(|v| !matches!(v, Value::Num(m) if m.is_empty()))
    }

    fn mat(&self, i: usize) -> R<Matrix> {
        match self.0.get(i) {
            Some(v) => v.to_matrix(),
            None => rt("Not enough input arguments."),
        }
    }

    fn scalar(&self, i: usize, default: f64) -> R<f64> {
        match self.get(i) {
            Some(v) => v.scalar(),
            None => Ok(default),
        }
    }

    fn n(&self, i: usize) -> R<usize> {
        let v = self.mat(i)?;
        if v.numel() != 1 {
            return rt("Size inputs must be scalar.");
        }
        Ok(v.data[0].max(0.0) as usize)
    }

    /// A vector argument, where a scalar n stands for 1:n.
    fn points(&self, i: usize) -> R<Vec<f64>> {
        let v = self.mat(i)?;
        Ok(if v.numel() == 1 { (1..=v.data[0].max(0.0) as usize).map(|k| k as f64).collect() } else { v.data })
    }

    /// `n` or `[rows cols]`.
    fn shape(&self, i: usize) -> R<(usize, usize)> {
        let v = self.mat(i)?;
        Ok(match v.numel() {
            1 => (v.data[0] as usize, v.data[0] as usize),
            _ => (v.data[0] as usize, v.data[1] as usize),
        })
    }
}

fn triw(r: usize, c: usize, alpha: f64, k: usize) -> Matrix {
    build(r, c, |i, j| if i == j { 1.0 } else if j > i && j - i <= k { alpha } else { 0.0 })
}

fn tridiag_m(n: usize, sub: &[f64], diag: &[f64], sup: &[f64]) -> Matrix {
    build(n, n, |i, j| {
        if i == j {
            diag[i]
        } else if i == j + 1 {
            sub[j]
        } else if j == i + 1 {
            sup[i]
        } else {
            0.0
        }
    })
}

fn kron_sum(t: &Matrix, n: usize) -> Matrix {
    // kron(I, T) + kron(T, I) for an n-by-n T.
    let nn = n * n;
    build(nn, nn, |p, q| {
        let (pi, pj) = (p % n, p / n);
        let (qi, qj) = (q % n, q / n);
        let a = if pj == qj { t.get(pi, qi) } else { 0.0 };
        let b = if pi == qi { t.get(pj, qj) } else { 0.0 };
        a + b
    })
}

fn gallery(a: &[Value], nargout: usize) -> Ret {
    let first = arg(a, 0, "gallery")?;
    if !first.is_text() {
        let n = first.scalar()?;
        return one(Value::Num(match n {
            3.0 => Matrix::new(3, 3, vec![-149.0, 537.0, -27.0, -50.0, 180.0, -9.0, -154.0, 546.0, -25.0]),
            5.0 => Matrix::new(
                5,
                5,
                vec![
                    -9.0, 70.0, -575.0, 3891.0, 1024.0, 11.0, -69.0, 575.0, -3891.0, -1024.0, -21.0, 141.0, -1149.0, 7782.0, 2048.0, 63.0, -421.0,
                    3451.0, -23345.0, -6144.0, -252.0, 1684.0, -13801.0, 93365.0, 24572.0,
                ],
            ),
            _ => return rt("Invalid N in GALLERY(N)."),
        }));
    }
    let fam = first.text()?.to_ascii_lowercase();
    let mut rest = &a[1..];
    let mut kind = None;
    if let Some(last) = rest.last() {
        if last.is_text() {
            kind = match last.text()?.as_str() {
                "single" => Some(Kind::Single),
                "double" => Some(Kind::Double),
                _ => return rt("Invalid class name."),
            };
            rest = &rest[..rest.len() - 1];
        }
    }
    let kind = kind.unwrap_or(if rest.iter().any(|v| matches!(v, Value::Num(m) if m.kind == Kind::Single)) { Kind::Single } else { Kind::Double });
    let p = Params(rest);
    let outs: Vec<Matrix> = match fam.as_str() {
        "cauchy" => {
            let x = p.points(0)?;
            let y = if p.get(1).is_some() { p.mat(1)?.data } else { x.clone() };
            vec![build(x.len(), y.len(), |i, j| 1.0 / (x[i] + y[j]))]
        }
        "chebspec" => {
            let (n, k) = (p.n(0)?, p.scalar(1, 0.0)?);
            let m = if k == 1.0 { n + 1 } else { n };
            let d = chebspec(m);
            vec![if k == 1.0 { build(n, n, |i, j| d.get(i + 1, j + 1)) } else { d }]
        }
        "chebvand" => {
            let (m, pts) = if p.get(1).is_some() { (Some(p.n(0)?), p.mat(1)?) } else { (None, p.mat(0)?) };
            let pts = if pts.numel() == 1 {
                let k = pts.data[0] as usize;
                (0..k).map(|i| if k == 1 { 1.0 } else { i as f64 / (k - 1) as f64 }).collect::<Vec<_>>()
            } else {
                pts.data
            };
            let n = pts.len();
            let m = m.unwrap_or(n);
            let mut c = Matrix::zeros(m, n);
            for (j, &x) in pts.iter().enumerate() {
                let (mut t0, mut t1) = (1.0, x);
                for i in 0..m {
                    let v = match i {
                        0 => 1.0,
                        1 => x,
                        _ => {
                            let t = 2.0 * x * t1 - t0;
                            t0 = t1;
                            t1 = t;
                            t
                        }
                    };
                    c.set(i, j, v);
                }
            }
            vec![c]
        }
        "chow" => {
            let (n, alpha, delta) = (p.n(0)?, p.scalar(1, 1.0)?, p.scalar(2, 0.0)?);
            vec![build(n, n, |i, j| {
                let h = if i + 1 >= j { alpha.powf((i + 1 - j) as f64) } else { 0.0 };
                if i == j { h + delta } else { h }
            })]
        }
        "circul" => {
            let v = p.points(0)?;
            let n = v.len();
            vec![build(n, n, |i, j| v[(j + n - i) % n])]
        }
        "clement" => {
            let (n, k) = (p.n(0)?, p.scalar(1, 0.0)?);
            let z: Vec<f64> = (1..n).map(|i| i as f64).collect();
            let y: Vec<f64> = (1..n).rev().map(|i| i as f64).collect();
            let (sub, sup) = if k == 0.0 { (y, z) } else {
                let s: Vec<f64> = z.iter().zip(&y).map(|(a, b)| (a * b).sqrt()).collect();
                (s.clone(), s)
            };
            vec![tridiag_m(n, &sub, &vec![0.0; n], &sup)]
        }
        "compar" => {
            let m = p.mat(0)?;
            let k = p.scalar(1, 0.0)?;
            let (r, c) = (m.rows(), m.cols());
            let abs = |i: usize, j: usize| C::new(m.get(i, j), m.im_at(j * r + i)).abs();
            let out = if k == 0.0 {
                build(r, c, |i, j| if i == j { abs(i, j) } else { -abs(i, j) })
            } else {
                let upper = (0..r).all(|i| (0..c.min(i)).all(|j| abs(i, j) == 0.0));
                let lower = (0..r).all(|i| (i + 1..c).all(|j| abs(i, j) == 0.0));
                let rowmax: Vec<f64> = (0..r).map(|i| (0..c).filter(|&j| j != i).map(|j| abs(i, j)).fold(0.0, f64::max)).collect();
                build(r, c, |i, j| {
                    if i == j {
                        abs(i, j)
                    } else if (upper && i > j) || (lower && j > i) {
                        0.0
                    } else {
                        -rowmax[i]
                    }
                })
            };
            vec![out]
        }
        "dramadah" => {
            let (n, k) = (p.n(0)?, p.scalar(1, 1.0)?);
            let (c, r): (Vec<f64>, Vec<f64>) = match k as i64 {
                1 => (
                    (0..n).map(|i| if i % 4 == 1 || i % 4 == 2 { 0.0 } else { 1.0 }).collect(),
                    (0..n).map(|j| if j < 2 || j == 3 { 1.0 } else { 0.0 }).collect(),
                ),
                2 => ((0..n).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect(), (0..n).map(|j| if j < 2 || (j >= 3 && j % 2 == 1) { 1.0 } else { 0.0 }).collect()),
                3 => ((0..n).map(|i| if i % 2 == 0 { 1.0 } else { 0.0 }).collect(), (0..n).map(|j| if j < 2 { 1.0 } else { 0.0 }).collect()),
                _ => return rt("Illegal value for K."),
            };
            vec![build(n, n, |i, j| if i >= j { c[i - j] } else { r[j - i] })]
        }
        "fiedler" => {
            let c = p.points(0)?;
            vec![build(c.len(), c.len(), |i, j| (c[i] - c[j]).abs())]
        }
        "forsythe" => {
            let (n, alpha, lambda) = (p.n(0)?, p.scalar(1, f64::EPSILON.sqrt())?, p.scalar(2, 0.0)?);
            let mut m = build(n, n, |i, j| if i == j { lambda } else if j == i + 1 { 1.0 } else { 0.0 });
            if n > 0 {
                m.set(n - 1, 0, alpha);
            }
            vec![m]
        }
        "frank" => {
            let (n, k) = (p.n(0)?, p.scalar(1, 0.0)?);
            let f = |i: usize, j: usize| if j + 1 >= i { (n - i.max(j)) as f64 } else { 0.0 };
            vec![if k == 0.0 { build(n, n, f) } else { build(n, n, |i, j| f(n - 1 - j, n - 1 - i)) }]
        }
        "gcdmat" => {
            let n = p.n(0)?;
            let gcd = |mut a: usize, mut b: usize| {
                while b != 0 {
                    (a, b) = (b, a % b);
                }
                a as f64
            };
            vec![build(n, n, |i, j| gcd(i + 1, j + 1))]
        }
        "gearmat" => {
            let n = p.n(0)?;
            let (ii, jj) = (p.scalar(1, n as f64)?, p.scalar(2, -(n as f64))?);
            let mut m = build(n, n, |i, j| if i.abs_diff(j) == 1 { 1.0 } else { 0.0 });
            if n > 0 {
                m.set(0, ii.abs() as usize - 1, ii.signum());
                m.set(n - 1, n - jj.abs() as usize, jj.signum());
            }
            vec![m]
        }
        "grcar" => {
            let (n, k) = (p.n(0)?, p.scalar(1, 3.0)? as usize);
            vec![build(n, n, |i, j| if i == j + 1 { -1.0 } else if j >= i && j - i <= k { 1.0 } else { 0.0 })]
        }
        "hanowa" => {
            let (n, d) = (p.n(0)?, p.scalar(1, -1.0)?);
            if n % 2 != 0 {
                return rt("N must be even.");
            }
            let m = n / 2;
            vec![build(n, n, |i, j| {
                let (bi, bj) = (i / m, j / m);
                let (r, c) = (i % m, j % m);
                let diag = if r == c { (r + 1) as f64 } else { 0.0 };
                match (bi, bj) {
                    (0, 1) => -diag,
                    (1, 0) => diag,
                    _ => d * if r == c { 1.0 } else { 0.0 },
                }
            })]
        }
        "invhess" => {
            let x = p.points(0)?;
            let n = x.len();
            let y = if p.get(1).is_some() { p.mat(1)?.data } else { x[..n.saturating_sub(1)].iter().map(|v| -v).collect() };
            vec![build(n, n, |i, j| if i >= j { x[j] } else { y[i] })]
        }
        "invol" => {
            let n = p.n(0)?;
            let mut m = hilb(n);
            let nf = n as f64;
            let mut d = -nf;
            for i in 0..n {
                m.set(i, 0, d * m.get(i, 0));
            }
            for i in 1..n {
                let fi = i as f64;
                d = -(nf + fi) * (nf - fi) * d / (fi * fi);
                for j in 0..n {
                    m.set(i, j, d * m.get(i, j));
                }
            }
            vec![m]
        }
        "ipjfact" => {
            let (n, k) = (p.n(0)?, p.scalar(1, 0.0)?);
            if n < 2 {
                return rt("Array indices must be positive integers or logical values.");
            }
            let fact = |m: usize| (1..=m).fold(1.0, |acc, v| acc * v as f64);
            let a = build(n, n, |i, j| {
                let f = fact(i + j + 2);
                if k == 0.0 { f } else { 1.0 / f }
            });
            let d = if k == 0.0 {
                let mut d = 1.0;
                for i in 1..n {
                    d = d * fact(i + 1) * fact(n - i);
                }
                d * fact(n + 1)
            } else {
                let mut d = 1.0;
                for i in 0..n {
                    d = d * fact(i) / fact(n + 1 + i);
                }
                if (n * (n - 1) / 2) % 2 == 1 { -d } else { d }
            };
            vec![a, Matrix::scalar(d)]
        }
        "jordbloc" => {
            let (n, lambda) = (p.n(0)?, p.scalar(1, 1.0)?);
            vec![build(n, n, |i, j| if i == j { lambda } else if j == i + 1 { 1.0 } else { 0.0 })]
        }
        "kahan" => {
            let (r, c) = p.shape(0)?;
            let (theta, pert) = (p.scalar(1, 1.2)?, p.scalar(2, 1e3)?);
            let (s, co) = (theta.sin(), theta.cos());
            vec![build(r, c, |i, j| {
                let base = if j == i { 1.0 } else if j > i { -co } else { 0.0 };
                let v = s.powf(i as f64) * base;
                if i == j { v + pert * f64::EPSILON * (c - i) as f64 } else { v }
            })]
        }
        "kms" => {
            let n = p.n(0)?;
            let rho = match p.get(1) {
                Some(v) => {
                    let m = v.to_matrix()?;
                    C::new(m.data[0], m.im_at(0))
                }
                None => C::new(0.5, 0.0),
            };
            if rho.im == 0.0 {
                vec![build(n, n, |i, j| rho.re.powf(i.abs_diff(j) as f64))]
            } else {
                let pw = |k: usize| (0..k).fold(C::new(1.0, 0.0), |acc, _| acc.mul(rho));
                let below = |k: usize| {
                    let z = pw(k).conj();
                    C::new(z.re, z.im + 0.0)
                };
                vec![build_c(n, n, |i, j| if i > j { below(i - j) } else { pw(j - i) })]
            }
        }
        "krylov" => {
            let m = p.mat(0)?;
            if m.numel() == 1 {
                return rt("gallery('krylov', N) needs random matrices; pass the matrix A instead.");
            }
            let n = m.rows();
            let x = if p.get(1).is_some() { p.mat(1)?.reshaped(vec![n, 1]) } else { Matrix::filled(n, 1, 1.0) };
            let jj = p.scalar(2, n as f64)? as usize;
            let mut cols = vec![x];
            for _ in 1..jj {
                let next = ops::matmul(&m, cols.last().unwrap())?;
                cols.push(next);
            }
            vec![build(n, jj, |i, j| cols[j].data[i])]
        }
        "lauchli" => {
            let (n, mu) = (p.n(0)?, p.scalar(1, f64::EPSILON.sqrt())?);
            vec![build(n + 1, n, |i, j| if i == 0 { 1.0 } else if i - 1 == j { mu } else { 0.0 })]
        }
        "lehmer" => {
            let n = p.n(0)?;
            vec![build(n, n, |i, j| (i.min(j) + 1) as f64 / (i.max(j) + 1) as f64)]
        }
        "leslie" => {
            let (aa, bb) = if p.get(1).is_some() {
                (p.mat(0)?.data, p.mat(1)?.data)
            } else {
                let n = p.n(0)?;
                (vec![1.0; n], vec![1.0; n.saturating_sub(1)])
            };
            let n = aa.len();
            vec![build(n, n, |i, j| if i == 0 { aa[j] } else if i == j + 1 { bb[j] } else { 0.0 })]
        }
        "lesp" => {
            let n = p.n(0)?;
            vec![build(n, n, |i, j| {
                if i == j {
                    -(2.0 * (i + 1) as f64 + 3.0)
                } else if j == i + 1 {
                    (i + 2) as f64
                } else if i == j + 1 {
                    1.0 / (j + 2) as f64
                } else {
                    0.0
                }
            })]
        }
        "lotkin" => {
            let n = p.n(0)?;
            vec![build(n, n, |i, j| if i == 0 { 1.0 } else { 1.0 / (i + j + 1) as f64 })]
        }
        "minij" => {
            let n = p.n(0)?;
            vec![build(n, n, |i, j| (i.min(j) + 1) as f64)]
        }
        "moler" => {
            let (n, alpha) = (p.n(0)?, p.scalar(1, -1.0)?);
            let u = triw(n, n, alpha, n.saturating_sub(1));
            vec![ops::matmul(&u.transpose(), &u)?]
        }
        "orthog" => {
            let (n, k) = (p.n(0)?, p.scalar(1, 1.0)?);
            vec![orthog(n, k as i64)?]
        }
        "parter" => {
            let n = p.n(0)?;
            vec![build(n, n, |i, j| 1.0 / (i as f64 - j as f64 + 0.5))]
        }
        "pei" => {
            let (n, alpha) = (p.n(0)?, p.scalar(1, 1.0)?);
            vec![build(n, n, |i, j| if i == j { alpha + 1.0 } else { 1.0 })]
        }
        "prolate" => {
            let (n, w) = (p.n(0)?, p.scalar(1, 0.25)?);
            let t: Vec<f64> = (0..n).map(|k| if k == 0 { 2.0 * w } else { sinpi(2.0 * w * k as f64) / (PI * k as f64) }).collect();
            vec![build(n, n, |i, j| t[i.abs_diff(j)])]
        }
        "redheff" => {
            let n = p.n(0)?;
            vec![build(n, n, |i, j| if j == 0 || (j + 1) % (i + 1) == 0 { 1.0 } else { 0.0 })]
        }
        "riemann" => {
            let n = p.n(0)?;
            vec![build(n, n, |i, j| if (j + 2) % (i + 2) == 0 { (i + 1) as f64 } else { -1.0 })]
        }
        "ris" => {
            let n = p.n(0)?;
            vec![build(n, n, |i, j| 0.5 / (n as f64 - (i + j + 2) as f64 + 1.5))]
        }
        "smoke" => {
            let (n, k) = (p.n(0)?, p.scalar(1, 0.0)?);
            vec![build_c(n, n, |i, j| {
                if i == j {
                    let t = 2.0 * (i + 1) as f64 / n as f64;
                    C::new(cospi(t), sinpi(t))
                } else if j == i + 1 || (k == 0.0 && i == n - 1 && j == 0) {
                    C::new(1.0, 0.0)
                } else {
                    C::new(0.0, 0.0)
                }
            })]
        }
        "toeppen" => {
            let n = p.n(0)?;
            let c = [p.scalar(1, 1.0)?, p.scalar(2, -10.0)?, p.scalar(3, 0.0)?, p.scalar(4, 10.0)?, p.scalar(5, 1.0)?];
            vec![build(n, n, |i, j| {
                let d = j as i64 - i as i64;
                if d.abs() <= 2 { c[(d + 2) as usize] } else { 0.0 }
            })]
        }
        "tridiag" => {
            let m0 = p.mat(0)?;
            if p.get(1).is_some() && m0.numel() > 1 || rest.len() == 3 && m0.numel() > 1 {
                let (x, y, z) = (m0.data, p.mat(1)?.data, p.mat(2)?.data);
                vec![tridiag_m(y.len(), &x, &y, &z)]
            } else {
                let n = p.n(0)?;
                let (c, d, e) = (p.scalar(1, -1.0)?, p.scalar(2, 2.0)?, p.scalar(3, -1.0)?);
                vec![tridiag_m(n, &vec![c; n], &vec![d; n], &vec![e; n])]
            }
        }
        "triw" => {
            let (r, c) = p.shape(0)?;
            let alpha = p.scalar(1, -1.0)?;
            let k = p.scalar(2, r.max(c).saturating_sub(1) as f64)? as usize;
            vec![triw(r, c, alpha, k)]
        }
        "wilk" => {
            let n = p.n(0)?;
            match n {
                3 => vec![
                    Matrix::new(3, 3, vec![1e-10, 0.0, 0.0, 0.9, 0.9, 0.0, -0.4, -0.4, 1e-10]),
                    Matrix::col(vec![0.0, 0.0, 1.0]),
                ],
                4 => vec![
                    Matrix::new(
                        4,
                        4,
                        vec![9.143e-5, 0.8762, 0.7943, 0.8017, 0.0, 7.156e-5, 0.8143, 0.6123, 0.0, 0.0, 9.504e-5, 0.7165, 0.0, 0.0, 0.0, 7.123e-5],
                    ),
                    Matrix::col(WILK4_B.to_vec()),
                ],
                5 | 21 => {
                    if nargout > 1 {
                        return rt(format!("GALLERY('WILK',{n}) supports only one output."));
                    }
                    vec![if n == 5 { build(5, 5, |i, j| 1.0 / (i + j + 2) as f64 * 1.8144) } else { wilkinson(21) }]
                }
                _ => return rt("Sorry, that value of N is not available."),
            }
        }
        "binomial" => {
            let n = p.n(0)?;
            let m = n.saturating_sub(1);
            vec![build(n, n, |i, j| (0..=i.min(j)).map(|k| if k % 2 == 1 { -1.0 } else { 1.0 } * binom(i, k) * binom(m - i, j - k)).sum())]
        }
        "sampling" => {
            let x = p.points(0)?;
            let n = x.len();
            let mut m = build(n, n, |i, j| if i == j { 0.0 } else { x[i] / (x[i] - x[j]) });
            for j in 0..n {
                let s: f64 = (0..n).filter(|&i| i != j).map(|i| m.get(i, j)).sum();
                m.set(j, j, s);
            }
            vec![m]
        }
        "house" => {
            let x = p.mat(0)?;
            let k = p.scalar(1, 0.0)?;
            return Ok(Some(house(&x, k)?.into_iter().map(|m| Value::Num(m.cast_to(kind))).collect()));
        }
        "poisson" => {
            let n = p.n(0)?;
            let t = tridiag_m(n, &vec![-1.0; n], &vec![2.0; n], &vec![-1.0; n]);
            vec![kron_sum(&t, n)]
        }
        "neumann" => {
            let nn = p.n(0)?;
            let n = (nn as f64).sqrt().round() as usize;
            if n * n != nn {
                return rt("N must be a perfect square.");
            }
            let mut t = tridiag_m(n, &vec![-1.0; n], &vec![2.0; n], &vec![-1.0; n]);
            if n > 1 {
                t.set(0, 1, -2.0);
                t.set(n - 1, n - 2, -2.0);
            }
            vec![kron_sum(&t, n)]
        }
        "condex" | "cycol" | "dorr" | "integerdata" | "normaldata" | "qmult" | "randcolu" | "randcorr" | "randhess" | "randjorth" | "rando"
        | "randsvd" | "toeppd" | "uniformdata" | "wathen" => return rt(format!("gallery('{fam}') is not supported.")),
        _ => return rt("Invalid matrix name."),
    };
    Ok(Some(outs.into_iter().map(|m| Value::Num(m.cast_to(kind))).collect()))
}

/// Chebyshev spectral differentiation matrix on the n points sin(pi*(n-1-2j)/(2(n-1))).
fn chebspec(n: usize) -> Matrix {
    if n == 1 {
        return Matrix::scalar(1.0 / 6.0);
    }
    let nm = (n - 1) as f64;
    let x: Vec<f64> = (0..n).map(|j| sinpi((nm - 2.0 * j as f64) / (2.0 * nm))).collect();
    let c: Vec<f64> = (0..n).map(|j| if j == 0 || j == n - 1 { 2.0 } else { 1.0 } * if j % 2 == 1 { -1.0 } else { 1.0 }).collect();
    let corner = (2.0 * nm * nm + 1.0) / 6.0;
    build(n, n, |i, j| {
        if i != j {
            (c[i] / c[j]) / (x[i] - x[j])
        } else if i == 0 {
            corner
        } else if i == n - 1 {
            -corner
        } else {
            -x[i] / (2.0 * (1.0 - x[i] * x[i]))
        }
    })
}

fn orthog(n: usize, k: i64) -> R<Matrix> {
    let nf = n as f64;
    Ok(match k {
        1 => build(n, n, |i, j| (2.0 / (nf + 1.0)).sqrt() * sinpi(((i + 1) * (j + 1)) as f64 / (nf + 1.0))),
        2 => build(n, n, |i, j| 2.0 / (2.0 * nf + 1.0).sqrt() * sinpi(2.0 * ((i + 1) * (j + 1)) as f64 / (2.0 * nf + 1.0))),
        3 => {
            let s = nf.sqrt();
            build_c(n, n, |i, j| {
                let t = 2.0 * (i * j) as f64 / nf;
                C::new(cospi(t) / s, sinpi(t) / s)
            })
        }
        4 => build(n, n, |i, j| {
            if i == 0 {
                1.0 / nf.sqrt()
            } else {
                let d = ((i + 1) * i) as f64;
                if j < i {
                    1.0 / d.sqrt()
                } else if j == i {
                    -(i as f64) / d.sqrt()
                } else {
                    0.0
                }
            }
        }),
        5 => build(n, n, |i, j| {
            let t = 2.0 * (i * j) as f64 / nf;
            (sinpi(t) + cospi(t)) / nf.sqrt()
        }),
        6 => build(n, n, |i, j| (2.0 / nf).sqrt() * cospi((i as f64 + 0.5) * (j as f64 + 0.5) / nf)),
        7 => {
            // I - 2*v*v'/(v'*v) with v = ones(n,1) - sqrt(n)*e_1, sign-flipped so sum(Q) = [sqrt(n) 0 ...].
            let mut v = vec![1.0; n];
            if n > 0 {
                v[0] = 1.0 - nf.sqrt();
            }
            let vv: f64 = v.iter().map(|x| x * x).sum();
            build(n, n, |i, j| {
                let h = if i == j { 1.0 } else { 0.0 } - 2.0 * v[i] * v[j] / vv;
                if vv == 0.0 { if i == j { 1.0 } else { 0.0 } } else { h }
            })
        }
        -1 if n == 1 => Matrix::scalar(1.0),
        -1 => build(n, n, |i, j| cospi((i * j) as f64 / (nf - 1.0))),
        -2 => build(n, n, |i, j| cospi(i as f64 * (j as f64 + 0.5) / nf)),
        _ => return rt("Illegal value for K."),
    })
}

/// [v, beta, s] with (I - beta*v*v')*x = s*e_1.
fn house(x: &Matrix, k: f64) -> R<Vec<Matrix>> {
    let z = elems_c(x);
    let n = z.len();
    let norm = z.iter().map(|c| c.abs() * c.abs()).sum::<f64>().sqrt();
    let x1 = z.first().copied().unwrap_or(C::new(0.0, 0.0));
    let sg = if x1.abs() == 0.0 { C::new(1.0, 0.0) } else { C::new(x1.re / x1.abs(), x1.im / x1.abs()) };
    let s = match k as i64 {
        0 => C::new(-sg.re * norm, -sg.im * norm),
        1 => C::new(sg.re * norm, sg.im * norm),
        _ => C::new(norm, 0.0),
    };
    let tail_zero = z.iter().skip(1).all(|c| c.abs() == 0.0);
    let trivial = norm == 0.0 || (tail_zero && (k == 1.0 || (k == 2.0 && x1.im == 0.0 && x1.re >= 0.0)));
    let as_mat = |v: Vec<C>| {
        let len = v.len();
        Matrix::complex(vec![len, 1], v.iter().map(|c| c.re).collect(), v.iter().map(|c| c.im).collect())
    };
    let scal = |c: C| Matrix::complex(vec![1, 1], vec![c.re], vec![c.im]);
    if trivial {
        return Ok(vec![Matrix::zeros(n, 1), Matrix::scalar(1.0), scal(x1)]);
    }
    let mut v = z.clone();
    v[0] = x1.sub(s);
    let beta = -1.0 / s.conj().mul(v[0]).re;
    Ok(vec![as_mat(v), Matrix::scalar(beta), scal(s)])
}
