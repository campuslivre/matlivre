//! `buffer`, `shiftdata`, `unshiftdata`, `uencode` and `udecode`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["buffer", "shiftdata", "unshiftdata", "uencode", "udecode"];

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "buffer" => buffer(a, nargout),
        "shiftdata" => shiftdata(it, a, nargout),
        "unshiftdata" => unshiftdata(it, a),
        "uencode" => uencode(a, nargout),
        _ => udecode(a, nargout),
    }
}

fn scalar_of(v: &Value) -> Option<f64> {
    match v {
        Value::Num(m) if m.numel() == 1 && m.kind != Kind::Char => Some(m.data[0]),
        _ => None,
    }
}

fn is_empty(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.numel() == 0)
}

// ---------------------------------------------------------------- buffer

fn buffer(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) => m.clone(),
        _ => return rt("Input must be a vector."),
    };
    if x.dims.len() > 2 || x.numel() == 0 || (x.dims[0] != 1 && x.dims[1] != 1) {
        return rt("Input must be a vector.");
    }
    let row = x.dims[0] == 1 && x.dims[1] != 1;
    let nv = scalar_of(&a[1]);
    let n = match nv {
        Some(v) if v >= 0.0 && v.fract() == 0.0 => v as usize,
        _ => return rt("Buffer size N must be a positive integer-valued scalar."),
    };
    let p = match a.get(2) {
        None => 0i64,
        Some(v) => match scalar_of(v) {
            Some(p) if p.fract() == 0.0 => p as i64,
            _ => return rt("Buffer overlap P must be an integer-valued scalar."),
        },
    };
    if p >= n as i64 {
        return rt("Frame overlap P must be less than the buffer size N.");
    }
    let len = x.numel();
    let opt = a.get(3);
    let mut nodelay = false;
    let mut ic: Vec<(f64, f64)> = vec![];
    let mut offset = 0usize;
    if let Some(o) = opt {
        if p == 0 {
            if !is_empty(o) {
                return rt("Options argument OPT must be empty if P is 0.");
            }
        } else if p > 0 {
            match o {
                Value::Num(m) if m.kind == Kind::Char => {
                    let s = o.text().unwrap_or_default();
                    if !s.is_empty() && "nodelay".starts_with(&s) {
                        nodelay = true;
                    } else {
                        return rt("OPT must be 'nodelay' or a vector of initial conditions for overlap buffering.");
                    }
                }
                Value::Num(m) if m.numel() == 0 => {}
                Value::Num(m) => {
                    if m.numel() != p as usize {
                        return rt("Initial conditions must be specified as a length-P vector.");
                    }
                    let im = m.im.clone().unwrap_or_else(|| vec![0.0; m.numel()]);
                    ic = (0..m.numel()).map(|k| (m.data[k], im[k])).collect();
                }
                _ => return rt("OPT must be 'nodelay' or a vector of initial conditions for overlap buffering."),
            }
        } else if !is_empty(o) {
            match scalar_of(o) {
                Some(v) if v.fract() == 0.0 && v >= 0.0 && v <= (-p) as f64 => offset = v as usize,
                _ => return rt("Sample skip offset for case of underlap P must be integer-valued and in the range [0,P]."),
            }
        }
    }
    let cplx = x.im.is_some() || a.get(3).map_or(false, |o| matches!(o, Value::Num(m) if m.im.is_some()));
    let xim = x.im.clone().unwrap_or_else(|| vec![0.0; len]);
    let samples: Vec<(f64, f64)> = (0..len).map(|k| (x.data[k], xim[k])).collect();
    let want_z = nargout >= 2;
    let step = (n as i64 - p) as usize;

    let mut frames: Vec<Vec<(f64, f64)>> = vec![];
    let mut z: Vec<(f64, f64)> = vec![];
    let mut optv: Vec<(f64, f64)> = vec![];
    let mut skip_out = 0usize;
    let pad = |v: &[(f64, f64)], nn: usize| -> Vec<(f64, f64)> {
        let mut f = v.to_vec();
        f.resize(nn, (0.0, 0.0));
        f
    };
    if p >= 0 {
        let pu = p as usize;
        let s: Vec<(f64, f64)> = if nodelay {
            samples.clone()
        } else {
            let mut s = if ic.is_empty() { vec![(0.0, 0.0); pu] } else { ic.clone() };
            s.extend_from_slice(&samples);
            s
        };
        let ls = s.len();
        if want_z {
            let f = if ls >= n { (ls - n) / step + 1 } else { 0 };
            for k in 0..f {
                frames.push(s[k * step..k * step + n].to_vec());
            }
            if pu > 0 {
                if f == 0 {
                    z = s[..ls.saturating_sub(pu).min(ls)].to_vec();
                    optv = s[..pu.min(ls)].to_vec();
                } else {
                    let consumed = (f - 1) * step + n;
                    z = s[consumed..].to_vec();
                    optv = s[consumed - pu..consumed].to_vec();
                }
            } else {
                z = s[f * n..].to_vec();
            }
        } else {
            let total = if nodelay { if len > pu { (len - pu + step - 1) / step } else { 0 } } else { (len + step - 1) / step };
            for k in 0..total {
                let lo = (k * step).min(ls);
                let hi = (k * step + n).min(ls);
                frames.push(pad(&s[lo..hi], n));
            }
        }
    } else {
        let gap = (-p) as usize;
        let stride = n + gap;
        if want_z {
            let mut start = offset;
            while start + n <= len {
                frames.push(samples[start..start + n].to_vec());
                start += stride;
            }
            // samples after the last complete frame: first the gap, then a partial frame
            let last_end = if frames.is_empty() { offset } else { offset + (frames.len() - 1) * stride + n };
            if frames.is_empty() {
                z = samples[offset.min(len)..].to_vec();
                skip_out = 0;
            } else {
                let rest = len - last_end.min(len);
                if rest >= gap {
                    z = samples[last_end + gap..].to_vec();
                    skip_out = 0;
                } else {
                    z = vec![];
                    skip_out = gap - rest;
                }
            }
        } else {
            let avail = len.saturating_sub(offset);
            let total = (avail + stride - 1) / stride;
            for k in 0..total {
                let lo = (offset + k * stride).min(len);
                let hi = (offset + k * stride + n).min(len);
                frames.push(pad(&samples[lo..hi], n));
            }
        }
    }

    let mk = |dims: Vec<usize>, v: &[(f64, f64)]| -> Value {
        let re: Vec<f64> = v.iter().map(|e| e.0).collect();
        let mut m = if cplx { Matrix::complex(dims, re, v.iter().map(|e| e.1).collect()) } else { Matrix::from_dims(dims, re) };
        m.kind = x.kind;
        Value::Num(m)
    };
    let nf = frames.len();
    let flat: Vec<(f64, f64)> = frames.into_iter().flatten().collect();
    let mut outs = vec![mk(vec![n, nf], &flat)];
    if nargout > 1 {
        let zd = if p < 0 {
            vec![1, z.len()]
        } else if row {
            vec![1, z.len()]
        } else {
            vec![z.len(), 1]
        };
        outs.push(mk(zd, &z));
    }
    if nargout > 2 {
        if p > 0 {
            let mut m = mk(vec![optv.len(), 1], &optv);
            if let Value::Num(mm) = &mut m {
                mm.kind = Kind::Double;
                if x.kind != Kind::Double && !cplx {
                    mm.kind = x.kind;
                }
            }
            outs.push(m);
        } else if p < 0 {
            outs.push(scalar_value(skip_out as f64));
        } else {
            outs.push(Value::Num(Matrix::from_dims(vec![0, 0], vec![])));
        }
    }
    out(outs)
}

// ------------------------------------------------------------ shiftdata

fn dims_row(d: &[usize]) -> Value {
    real_row(d.iter().map(|&v| v as f64).collect())
}

fn shiftdata(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let x = a[0].clone();
    let dims = x.dims();
    let dim = a.get(1).filter(|v| !is_empty(v));
    let empty = || Value::Num(Matrix::from_dims(vec![0, 0], vec![]));
    match dim {
        Some(Value::Num(m)) => {
            if m.numel() != 1 {
                return rt("ORDER cannot contain repeated permutation indices.");
            }
            let d = m.data[0];
            if d.fract() != 0.0 {
                return rt("ORDER must have at least N elements for an N-D array.");
            }
            if d < 1.0 {
                return rt("ORDER contains an invalid permutation index.");
            }
            let d = d as usize;
            let nd = dims.len().max(d);
            let mut perm = vec![d];
            perm.extend((1..=nd).filter(|&k| k != d));
            let y = it.call_function("permute", vec![x, dims_row(&perm)], 1)?.remove(0);
            out(vec![y, dims_row(&perm), empty()])
        }
        Some(_) => rt("ORDER contains an invalid permutation index."),
        None => {
            let n = dims.iter().position(|&d| d != 1).unwrap_or(0);
            let n = if dims.iter().all(|&d| d == 1) { 0 } else { n };
            let mut nd: Vec<usize> = dims[n..].to_vec();
            if nd.len() < 2 {
                nd.push(1);
            }
            let y = if n == 0 { x } else { it.call_function("reshape", vec![x, dims_row(&nd)], 1)?.remove(0) };
            out(vec![y, empty(), scalar_value(n as f64)])
        }
    }
}

fn unshiftdata(it: &mut Interpreter, a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    let x = a[0].clone();
    if !is_empty(&a[1]) {
        let y = it.call_function("ipermute", vec![x, a[1].clone()], 1)?.remove(0);
        return out(vec![y]);
    }
    let n = a.get(2).and_then(scalar_of).unwrap_or(0.0).max(0.0) as usize;
    if n == 0 {
        return out(vec![x]);
    }
    let mut d = vec![1usize; n];
    d.extend(x.dims());
    let y = it.call_function("reshape", vec![x, dims_row(&d)], 1)?.remove(0);
    out(vec![y])
}

// ------------------------------------------------------ uencode / udecode

fn bits_arg(v: &Value) -> R<u32> {
    match scalar_of(v) {
        Some(b) if b.fract() == 0.0 && (2.0..=32.0).contains(&b) => Ok(b as u32),
        _ => rt("Number of output bits must be an integer from 2 to 32."),
    }
}

fn peak_arg(a: &[Value]) -> R<f64> {
    match a.get(2) {
        None => Ok(1.0),
        Some(v) if is_empty(v) => Ok(1.0),
        Some(Value::Num(m)) if m.numel() == 1 && m.im.is_none() && m.data[0] > 0.0 && m.kind != Kind::Char => Ok(m.data[0]),
        Some(_) => rt("Peak input value must be a positive real scalar."),
    }
}

fn uencode(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let u = match &a[0] {
        Value::Num(m) if m.kind == Kind::Double => m.clone(),
        _ => return rt("Input value must be a double."),
    };
    let nbits = bits_arg(&a[1])?;
    let v = peak_arg(a)?;
    let mut signed = false;
    if let Some(s) = a.get(3) {
        let t = s.text().unwrap_or_default().to_lowercase();
        if !t.is_empty() && "signed".starts_with(&t) {
            signed = true;
        } else if !t.is_empty() && "unsigned".starts_with(&t) {
            signed = false;
        } else {
            return rt("Sign character vector must be either 'unsigned' or 'signed'.");
        }
    }
    if u.im.is_some() {
        return rt("uencode: complex input is not supported.");
    }
    let levels = (2.0f64).powi(nbits as i32);
    let delta = 2.0 * v / levels;
    let class = match (signed, nbits) {
        (false, 2..=8) => IntClass::U8,
        (false, 9..=16) => IntClass::U16,
        (false, _) => IntClass::U32,
        (true, 2..=8) => IntClass::I8,
        (true, 9..=16) => IntClass::I16,
        (true, _) => IntClass::I32,
    };
    let data: Vec<f64> = u
        .data
        .iter()
        .map(|&x| {
            let k = if x.is_nan() { 0.0 } else { ((x + v) / delta).floor().clamp(0.0, levels - 1.0) };
            if signed { k - levels / 2.0 } else { k }
        })
        .collect();
    let mut m = Matrix::from_dims(u.dims.clone(), data);
    m.kind = Kind::Int(class);
    out(vec![Value::Num(m)])
}

fn udecode(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let u = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Int(IntClass::I8 | IntClass::I16 | IntClass::I32 | IntClass::U8 | IntClass::U16 | IntClass::U32)) => m.clone(),
        _ => return rt("Input value must be a signed or unsigned integer."),
    };
    let nbits = bits_arg(&a[1])?;
    let v = peak_arg(a)?;
    let mut wrap = false;
    if let Some(s) = a.get(3) {
        let t = s.text().unwrap_or_default().to_lowercase();
        if !t.is_empty() && "saturate".starts_with(&t) {
            wrap = false;
        } else if !t.is_empty() && "wrap".starts_with(&t) {
            wrap = true;
        } else {
            return rt("Overflow mode character vector must be either 'saturate' or 'wrap'.");
        }
    }
    let half = (2.0f64).powi(nbits as i32 - 1);
    let signed = matches!(u.kind, Kind::Int(IntClass::I8 | IntClass::I16 | IntClass::I32));
    let (lo, hi) = if signed { (-half, half - 1.0) } else { (0.0, 2.0 * half - 1.0) };
    let data: Vec<f64> = u
        .data
        .iter()
        .map(|&x| {
            let k = if wrap { lo + (x - lo).rem_euclid(hi - lo + 1.0) } else { x.clamp(lo, hi) };
            let centered = if signed { k } else { k - half };
            v * centered / half
        })
        .collect();
    out(vec![Value::Num(Matrix::from_dims(u.dims.clone(), data))])
}
