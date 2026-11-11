//! Transforms and convolution helpers: `dct`, `idct`, `dftmtx`, `goertzel`, `hilbert`,
//! `cconv`, `convmtx`.
//!
//! `dct` and `idct` sum the orthonormal DCT-II definition directly, O(N^2) per column. Complex
//! input is handled by transforming its real and imaginary parts separately (all of these
//! operations are linear).

use crate::builtins::{m_arg, Ret};
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["dct", "idct", "dftmtx", "goertzel", "hilbert", "cconv", "convmtx"];

const PI: f64 = std::f64::consts::PI;

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    match name {
        "dct" | "idct" => dct(name, a),
        "dftmtx" => dftmtx(a),
        "goertzel" => goertzel(a),
        "hilbert" => hilbert(it, a),
        "cconv" => cconv(a),
        _ => convmtx(a),
    }
}

/// The real and (if any) imaginary parts of a numeric argument.
fn parts(a: &[Value], i: usize, name: &str) -> R<(Matrix, Option<Matrix>, Kind)> {
    let m = m_arg(a, i, name)?;
    if m.dims.len() > 2 {
        return rt("N-D arrays are not supported.");
    }
    let im = m.im.as_ref().filter(|im| im.iter().any(|&v| v != 0.0)).map(|im| Matrix::from_dims(m.dims.clone(), im.clone()));
    let kind = m.kind;
    let re = Matrix::from_dims(m.dims.clone(), m.data.clone());
    Ok((re, im, kind))
}

/// Recombine real and imaginary results.
fn combine(re: Matrix, im: Option<Matrix>) -> Matrix {
    match im {
        Some(im) => Matrix::complex(re.dims.clone(), re.data, im.data),
        None => re,
    }
}

fn reject_integers(kind: Kind, name: &str, label: &str, types: &str) -> R<()> {
    if let Kind::Int(c) = kind {
        let _ = c;
        return rt(format!("Input {label} in {name} must be {types}, instead it was {}.", kind.name()));
    }
    Ok(())
}

/// A size argument (`n`) that may be empty (meaning the default).
fn size_arg(a: &[Value], i: usize) -> R<Option<usize>> {
    let Some(v) = a.get(i) else { return Ok(None) };
    if let Ok(s) = v.text() {
        if !s.is_empty() {
            return rt(format!("The argument '{s}' is a string scalar or character vector and does not match any parameter names. It failed validation for the argument 'N'."));
        }
    }
    let m = m_arg(a, i, "dct")?;
    if m.numel() == 0 {
        return Ok(None);
    }
    if m.numel() != 1 {
        return rt("Expected input to be a scalar.");
    }
    if m.data[0] <= 0.0 {
        return rt("Expected input to be positive.");
    }
    if m.data[0].fract() != 0.0 {
        return rt("Expected input to be integer-valued.");
    }
    Ok(Some(m.data[0] as usize))
}

fn dct(name: &str, a: &[Value]) -> Ret {
    if a.len() > 3 {
        if matches!(&a[3], Value::Num(m) if m.kind != Kind::Char) {
            return rt("Expected a string scalar or character vector for the parameter name.");
        }
        return rt("Name-value options are not supported.");
    }
    let (re, im, kind) = parts(a, 0, name)?;
    reject_integers(kind, name, "X", "double/single")?;
    let n_arg = size_arg(a, 1)?;
    let dim = if a.len() == 3 {
        let d = scalar_arg(a, 2, name)?;
        if d < 1.0 || d.fract() != 0.0 {
            return rt("Expected input to be positive.");
        }
        if d > 2.0 {
            return rt("Operating along dimension 3 is not supported.");
        }
        d as usize
    } else {
        first_dim(&re)
    };
    let inverse = name == "idct";
    let transform = |x: &Matrix| -> R<Matrix> {
        along(x, dim, &|s| {
            let n = n_arg.unwrap_or(s.len());
            let mut v = s.to_vec();
            v.resize(n, 0.0);
            // cos(pi (2j + 1) k / (2n)) = table[(2j + 1) k mod 4n].
            let table: Vec<f64> = (0..4 * n).map(|m| (PI * m as f64 / (2 * n) as f64).cos()).collect();
            let weight = |k: usize| if k == 0 { (1.0 / n as f64).sqrt() } else { (2.0 / n as f64).sqrt() };
            Ok((0..n)
                .map(|p| {
                    (0..n)
                        .map(|q| {
                            // Forward: output index p is the frequency k, input q the sample j.
                            // Inverse: output index p is the sample j, input q the frequency k.
                            let (j, k) = if inverse { (p, q) } else { (q, p) };
                            let w = if inverse { weight(k) } else { 1.0 };
                            w * v[q] * table[((2 * j + 1) * k) % (4 * n)]
                        })
                        .sum::<f64>()
                        * if inverse { 1.0 } else { weight(p) }
                })
                .collect())
        })
    };
    let r = transform(&re)?;
    let i = match &im {
        Some(m) => Some(transform(m)?),
        None => None,
    };
    out(vec![Value::Num(combine(r, i))])
}

fn dftmtx(a: &[Value]) -> Ret {
    if a.len() > 1 {
        return rt("Too many input arguments.");
    }
    let m = m_arg(a, 0, "dftmtx")?;
    if m.numel() != 1 {
        return rt("Expected input number 1, n, to be a scalar.");
    }
    if m.data[0] < 0.0 {
        return rt("Expected input number 1, n, to be nonnegative.");
    }
    if m.data[0].fract() != 0.0 {
        return rt("Expected input number 1, n, to be integer-valued.");
    }
    let n = m.data[0] as usize;
    if n == 0 {
        return out(vec![Value::Num(Matrix::zeros(0, 0))]);
    }
    let (mut re, mut im) = (vec![0.0; n * n], vec![0.0; n * n]);
    for k in 0..n {
        for j in 0..n {
            let ang = 2.0 * PI * ((j * k) % n) as f64 / n as f64;
            re[k * n + j] = ang.cos();
            im[k * n + j] = -ang.sin();
        }
    }
    out(vec![Value::Num(Matrix::complex(vec![n, n], re, im))])
}

/// DFT coefficients X(k) for the requested (1-based, possibly fractional) indices.
fn goertzel(a: &[Value]) -> Ret {
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    let (re, im, kind) = parts(a, 0, "goertzel")?;
    if let Kind::Int(_) = kind {
        return rt(format!("Expected X to be one of these types:\n\nsingle, double\n\nInstead its type was {}.", kind.name()));
    }
    let dim = if a.len() == 3 && !matches!(&a[2], Value::Num(m) if m.numel() == 0) {
        let d = scalar_arg(a, 2, "goertzel")?;
        if d < 1.0 || d.fract() != 0.0 || d > 2.0 {
            return rt("Dimension specified exceeds the dimensions of X.");
        }
        d as usize
    } else {
        first_dim(&re)
    };
    let len = re.dims[dim - 1];
    let (rows, cols) = (re.dims[0], re.dims[1]);
    // Requested indices, and whether the output should follow their orientation.
    let (ind, ind_col): (Vec<f64>, Option<bool>) = match a.get(1) {
        Some(Value::Num(m)) if m.numel() > 0 => {
            if m.kind == Kind::Char {
                return rt("Expected INDVEC to be one of these types:\n\ndouble, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64");
            }
            (m.data.clone(), Some(m.dims[1] == 1 && m.dims[0] > 1))
        }
        _ => ((1..=len).map(|k| k as f64).collect(), None),
    };
    if ind.iter().any(|&k| k < 1.0) {
        return rt("Index must be greater than zero.");
    }
    if ind.iter().any(|&k| k > len as f64) {
        return rt("Maximum Index exceeds the dimensions of X.");
    }
    let dft = |sre: &[f64], sim: &[f64]| -> (Vec<f64>, Vec<f64>) {
        ind.iter()
            .map(|&k1| {
                let k = k1 - 1.0;
                (0..len).fold((0.0, 0.0), |(r, i), j| {
                    let ang = 2.0 * PI * ((j as f64 * k) % len as f64) / len as f64;
                    let (c, s) = (ang.cos(), ang.sin());
                    (r + sre[j] * c + sim[j] * s, i - sre[j] * s + sim[j] * c)
                })
            })
            .unzip()
    };
    let zero_im = vec![0.0; re.numel()];
    let imd = im.as_ref().map_or(&zero_im, |m| &m.data);
    let take = |idx: &[usize]| -> (Vec<f64>, Vec<f64>) { (idx.iter().map(|&i| re.data[i]).collect(), idx.iter().map(|&i| imd[i]).collect()) };
    let slices: Vec<Vec<usize>> = if dim == 1 { (0..cols).map(|c| (c * rows..(c + 1) * rows).collect()).collect() } else { (0..rows).map(|r| (0..cols).map(|c| c * rows + r).collect()).collect() };
    let results: Vec<(Vec<f64>, Vec<f64>)> = slices.iter().map(|s| {
        let (sr, si) = take(s);
        dft(&sr, &si)
    }).collect();
    let n_ind = ind.len();
    let vector_input = rows == 1 || cols == 1;
    let (dims, rev, imv) = if vector_input {
        let column = ind_col.unwrap_or(rows > 1);
        (if column { vec![n_ind, 1] } else { vec![1, n_ind] }, results[0].0.clone(), results[0].1.clone())
    } else if dim == 1 {
        (vec![n_ind, cols], results.iter().flat_map(|r| r.0.clone()).collect(), results.iter().flat_map(|r| r.1.clone()).collect())
    } else {
        let (mut r_out, mut i_out) = (vec![0.0; rows * n_ind], vec![0.0; rows * n_ind]);
        for (r, res) in results.iter().enumerate() {
            for c in 0..n_ind {
                r_out[c * rows + r] = res.0[c];
                i_out[c * rows + r] = res.1[c];
            }
        }
        (vec![rows, n_ind], r_out, i_out)
    };
    out(vec![Value::Num(Matrix::complex(dims, rev, imv))])
}

/// The one-sided spectrum weights that turn an FFT into the analytic signal's FFT.
fn analytic_mask(n: usize) -> Vec<f64> {
    let mut h = vec![0.0; n];
    h[0] = 1.0;
    if n % 2 == 0 {
        h[n / 2] = 1.0;
        for v in h.iter_mut().take(n / 2).skip(1) {
            *v = 2.0;
        }
    } else {
        for v in h.iter_mut().take(n.div_ceil(2)).skip(1) {
            *v = 2.0;
        }
    }
    h
}

fn hilbert(it: &mut Interpreter, a: &[Value]) -> Ret {
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let (re, im, kind) = parts(a, 0, "hilbert")?;
    if im.is_some() {
        it.emit(OutputKind::Warning("Imaginary parts of complex X argument ignored.".into()));
    }
    if re.numel() == 0 {
        return out(vec![Value::Num(re)]);
    }
    let _ = kind;
    let dim = first_dim(&re);
    let n = match a.get(1) {
        Some(Value::Num(m)) if m.numel() > 0 => {
            let v = m.data.first().copied().unwrap_or(f64::NAN);
            if m.numel() != 1 || v < 0.0 || v.fract() != 0.0 {
                return rt("FFT length must be a nonnegative integer scalar.");
            }
            if v == 0.0 {
                return rt("Index in position 2 exceeds array bounds.");
            }
            v as usize
        }
        _ => re.dims[dim - 1],
    };
    let args = |v: Matrix| vec![Value::Num(v), Value::Num(Matrix::scalar(n as f64)), Value::Num(Matrix::scalar(dim as f64))];
    let first = |it: &mut Interpreter, name: &str, v: Matrix| -> R<Matrix> {
        match it.call_function(name, args(v), 1)?.into_iter().next() {
            Some(Value::Num(m)) => Ok(m),
            _ => Err(LangError::new(format!("{name} returned an unexpected value."), 0)),
        }
    };
    let mut spec = first(it, "fft", re)?;
    let mask = analytic_mask(n);
    let rows = spec.dims[0];
    let imv = spec.im.get_or_insert_with(|| vec![0.0; spec.data.len()]);
    for (idx, (r, i)) in spec.data.iter_mut().zip(imv.iter_mut()).enumerate() {
        let pos = if dim == 1 { idx % rows } else { idx / rows };
        *r *= mask[pos];
        *i *= mask[pos];
    }
    let result = first(it, "ifft", spec)?;
    out(vec![Value::Num(result)])
}

fn vector_parts(a: &[Value], i: usize, name: &str, message: &str) -> R<(Vec<f64>, Option<Vec<f64>>, bool, Kind)> {
    let (re, im, kind) = parts(a, i, name)?;
    if re.dims[0] != 1 && re.dims[1] != 1 {
        return rt(message);
    }
    Ok((re.data.clone(), im.map(|m| m.data), re.dims[1] == 1 && re.dims[0] > 1, kind))
}

fn conv_circular(x: &[f64], y: &[f64], n: usize) -> Vec<f64> {
    let mut r = vec![0.0; n];
    for (i, &p) in x.iter().enumerate() {
        for (j, &q) in y.iter().enumerate() {
            r[(i + j) % n] += p * q;
        }
    }
    r
}

fn cconv(a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    let first = m_arg(a, 0, "cconv")?;
    if first.numel() == 0 {
        return rt("Input must be a vector.");
    }
    let (xr, xi, col, _) = vector_parts(a, 0, "cconv", "A and B must be vectors.")?;
    let second = m_arg(a, 1, "cconv")?;
    let (yr, yi) = if second.numel() == 0 {
        (Vec::new(), None)
    } else {
        let (r, i, _, _) = vector_parts(a, 1, "cconv", "A and B must be vectors.")?;
        (r, i)
    };
    let n = if a.len() == 3 {
        let m = m_arg(a, 2, "cconv")?;
        if m.numel() != 1 || m.data[0] < 0.0 || m.data[0].fract() != 0.0 {
            return rt("Buffer size N must be a positive integer-valued scalar.");
        }
        m.data[0] as usize
    } else {
        (xr.len() + yr.len()).saturating_sub(1)
    };
    if n == 0 {
        return rt("Frame overlap P must be less than the buffer size N.");
    }
    let zx = vec![0.0; xr.len()];
    let zy = vec![0.0; yr.len()];
    let (xi, yi) = (xi.unwrap_or(zx), yi.unwrap_or(zy));
    let complex = xi.iter().any(|&v| v != 0.0) || yi.iter().any(|&v| v != 0.0);
    let re: Vec<f64> = {
        let (p, q) = (conv_circular(&xr, &yr, n), conv_circular(&xi, &yi, n));
        p.iter().zip(&q).map(|(a, b)| a - b).collect()
    };
    let dims = if col { vec![n, 1] } else { vec![1, n] };
    if complex {
        let (p, q) = (conv_circular(&xr, &yi, n), conv_circular(&xi, &yr, n));
        let im: Vec<f64> = p.iter().zip(&q).map(|(a, b)| a + b).collect();
        out(vec![Value::Num(Matrix::complex(dims, re, im))])
    } else {
        out(vec![Value::Num(Matrix::from_dims(dims, re))])
    }
}

fn convmtx(a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let (hr, hi, col, _) = vector_parts(a, 0, "convmtx", "Expected input number 1, H, to be a vector.")?;
    if hr.is_empty() {
        return rt("Expected input number 1, H, to be a vector.");
    }
    let nm = m_arg(a, 1, "convmtx")?;
    if nm.numel() != 1 {
        return rt("Expected input number 2, N, to be a scalar.");
    }
    if nm.data[0] <= 0.0 {
        return rt("Expected input number 2, N, to be positive.");
    }
    if nm.data[0].fract() != 0.0 {
        return rt("Expected input number 2, N, to be integer-valued.");
    }
    let n = nm.data[0] as usize;
    let m = hr.len();
    let len = m + n - 1;
    // Column j holds h shifted down by j (column vector h); a row vector h gives the transpose.
    let build = |h: &[f64]| -> Matrix {
        let mut g = vec![0.0; len * n];
        for j in 0..n {
            for (i, &v) in h.iter().enumerate() {
                g[j * len + i + j] = v;
            }
        }
        if col {
            Matrix::new(len, n, g)
        } else {
            let mut t = vec![0.0; len * n];
            for j in 0..n {
                for i in 0..len {
                    t[i * n + j] = g[j * len + i];
                }
            }
            Matrix::new(n, len, t)
        }
    };
    let re = build(&hr);
    let result = match hi {
        Some(hi) => Matrix::complex(re.dims.clone(), re.data.clone(), build(&hi).data),
        None => re,
    };
    out(vec![Value::Num(result)])
}
