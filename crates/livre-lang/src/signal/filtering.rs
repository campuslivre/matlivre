//! Filtering and resampling helpers: `filtfilt`, `sosfilt`, `filtic`, `medfilt1`, `downsample`,
//! `upsample`, `upfirdn`, `fftfilt`, `impz`, `stepz`, `bitrevorder`.

use crate::builtins::{m_arg, Ret};
use crate::cplx::{poly_roots, C};
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["filtfilt", "sosfilt", "filtic", "medfilt1", "downsample", "upsample", "upfirdn", "fftfilt", "impz", "stepz", "bitrevorder"];

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "filtfilt" => filtfilt(a),
        "sosfilt" => sosfilt(a),
        "filtic" => filtic(a),
        "medfilt1" => medfilt1(a),
        "upfirdn" => upfirdn(a),
        "fftfilt" => fftfilt(a),
        "impz" | "stepz" => impz(name, a, nargout),
        "bitrevorder" => bitrevorder(a),
        _ => resample_by(name, a),
    }
}

/// Direct form II transposed filter with initial state `zi` (length max(len b, len a) - 1).
/// Returns the output and the final state.
pub(crate) fn filt(b: &[f64], a: &[f64], x: &[f64], zi: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = b.len().max(a.len());
    let a0 = a[0];
    let mut bb = vec![0.0; n];
    let mut aa = vec![0.0; n];
    for (i, &v) in b.iter().enumerate() {
        bb[i] = v / a0;
    }
    for (i, &v) in a.iter().enumerate() {
        aa[i] = v / a0;
    }
    let mut z = vec![0.0; n.saturating_sub(1)];
    for (i, &v) in zi.iter().enumerate().take(z.len()) {
        z[i] = v;
    }
    let mut y = Vec::with_capacity(x.len());
    for &xi in x {
        let yi = if z.is_empty() { bb[0] * xi } else { bb[0] * xi + z[0] };
        for k in 0..z.len() {
            let next = if k + 1 < z.len() { z[k + 1] } else { 0.0 };
            z[k] = next + bb[k + 1] * xi - aa[k + 1] * yi;
        }
        y.push(yi);
    }
    (y, z)
}

/// Steady-state initial state of the filter for a unit step input.
fn steady_state(b: &[f64], a: &[f64]) -> Vec<f64> {
    let n = b.len().max(a.len());
    let m = n - 1;
    if m == 0 {
        return Vec::new();
    }
    let a0 = a[0];
    let at = |i: usize| a.get(i).copied().unwrap_or(0.0) / a0;
    let bt = |i: usize| b.get(i).copied().unwrap_or(0.0) / a0;
    // (I - companion(a)^T) zi = b[1..] - a[1..] b[0].
    let mut mat = vec![vec![0.0; m + 1]; m];
    for i in 0..m {
        mat[i][i] += 1.0;
        mat[i][0] += at(i + 1);
        if i + 1 < m {
            mat[i][i + 1] -= 1.0;
        }
        mat[i][m] = bt(i + 1) - at(i + 1) * bt(0);
    }
    // Gaussian elimination with partial pivoting.
    for col in 0..m {
        let piv = (col..m).max_by(|&p, &q| mat[p][col].abs().total_cmp(&mat[q][col].abs())).unwrap();
        if mat[piv][col].abs() < 1e-300 {
            return vec![0.0; m];
        }
        mat.swap(col, piv);
        for r in 0..m {
            if r != col {
                let f = mat[r][col] / mat[col][col];
                for c in col..=m {
                    let v = mat[col][c];
                    mat[r][c] -= f * v;
                }
            }
        }
    }
    (0..m).map(|i| mat[i][m] / mat[i][i]).collect()
}

fn positive_integer(v: f64, label: &str) -> R<usize> {
    if v <= 0.0 {
        return rt(format!("Expected {label} to be positive."));
    }
    if v.fract() != 0.0 {
        return rt(format!("Expected {label} to be integer-valued."));
    }
    Ok(v as usize)
}

/// A scalar numeric argument with MATLAB's validation wording.
fn scalar_labeled(a: &[Value], i: usize, name: &str, label: &str) -> R<f64> {
    let m = m_arg(a, i, name)?;
    if m.numel() != 1 {
        return rt(format!("Expected {label} to be a scalar."));
    }
    Ok(m.data[0])
}

// ---- filtfilt / sosfilt / filtic ---------------------------------------------------------------

fn filtfilt(a: &[Value]) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    let (bm, am) = (m_arg(a, 0, "filtfilt")?, m_arg(a, 1, "filtfilt")?);
    if (bm.dims[0] != 1 && bm.dims[1] != 1) || (am.dims[0] != 1 && am.dims[1] != 1) {
        return rt("Inputs must be vector or SOS matrix.");
    }
    let b = rvec_arg(a, 0, "filtfilt")?;
    let den = rvec_arg(a, 1, "filtfilt")?;
    if b.is_empty() || den.is_empty() {
        return rt("Inputs must be vector or SOS matrix.");
    }
    if den[0] == 0.0 {
        return rt("First denominator filter coefficient must be nonzero.");
    }
    let x = real_matrix(a, 2, "filtfilt")?;
    let nfact = 3 * (b.len().max(den.len()) - 1);
    let zi = steady_state(&b, &den);
    let m = along(&x, first_dim(&x), &|s| {
        if s.len() <= nfact {
            return rt(format!("Data length must be larger than {nfact} samples."));
        }
        let n = s.len();
        let mut ext = Vec::with_capacity(n + 2 * nfact);
        ext.extend((1..=nfact).rev().map(|i| 2.0 * s[0] - s[i]));
        ext.extend_from_slice(s);
        ext.extend((1..=nfact).map(|i| 2.0 * s[n - 1] - s[n - 1 - i]));
        let z0: Vec<f64> = zi.iter().map(|z| z * ext[0]).collect();
        let (mut y, _) = filt(&b, &den, &ext, &z0);
        y.reverse();
        let z1: Vec<f64> = zi.iter().map(|z| z * y[0]).collect();
        let (mut y, _) = filt(&b, &den, &y, &z1);
        y.reverse();
        Ok(y[nfact..nfact + n].to_vec())
    })?;
    out(vec![Value::Num(m)])
}

fn sosfilt(a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    let sos = real_matrix(a, 0, "sosfilt")?;
    if sos.dims[1] != 6 || sos.dims[0] == 0 {
        return rt("Size of SOS matrix must be Mx6. See \"zp2sos\" or \"ss2sos\" for details.");
    }
    let x = real_matrix(a, 1, "sosfilt")?;
    let dim = if a.len() == 3 && !matches!(&a[2], Value::Num(m) if m.numel() == 0) {
        let d = scalar_arg(a, 2, "sosfilt")?;
        if d < 1.0 || d.fract() != 0.0 {
            return rt("Dimension argument must be a positive integer scalar within indexing range.");
        }
        d as usize
    } else {
        first_dim(&x)
    };
    let k = sos.dims[0];
    let sections: Vec<([f64; 3], [f64; 3])> = (0..k)
        .map(|i| {
            let g = |c: usize| sos.data[c * k + i];
            ([g(0), g(1), g(2)], [g(3), g(4), g(5)])
        })
        .collect();
    let degenerate = sections.iter().any(|(_, den)| den[0] == 0.0);
    let m = along(&x, dim, &|s| {
        if degenerate {
            return Ok(vec![f64::NAN; s.len()]);
        }
        let mut cur = s.to_vec();
        for (b, den) in &sections {
            cur = filt(b, den, &cur, &[]).0;
        }
        Ok(cur)
    })?;
    out(vec![Value::Num(m)])
}

/// Initial state that continues a filter from past outputs `y` and inputs `x` (most recent
/// first); returned as a row.
fn filtic(a: &[Value]) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    let b = rvec_arg(a, 0, "filtic")?;
    let den = rvec_arg(a, 1, "filtic")?;
    if b.is_empty() || den.is_empty() {
        return rt("Expected numerator and denominator to be nonempty.");
    }
    let y = rvec_arg(a, 2, "filtic")?;
    let x = if a.len() == 4 { rvec_arg(a, 3, "filtic")? } else { Vec::new() };
    let n = b.len().max(den.len());
    let a0 = den[0];
    let bb = |i: usize| b.get(i).copied().unwrap_or(0.0) / a0;
    let aa = |i: usize| den.get(i).copied().unwrap_or(0.0) / a0;
    let at = |v: &[f64], i: usize| v.get(i).copied().unwrap_or(0.0);
    let z: Vec<f64> = (0..n - 1).map(|k| (k + 1..n).map(|m| bb(m) * at(&x, m - k - 1) - aa(m) * at(&y, m - k - 1)).sum()).collect();
    out(vec![real_row(z)])
}

// ---- medfilt1 ---------------------------------------------------------------------------------

fn median_of(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|p, q| p.total_cmp(q));
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

fn medfilt1(a: &[Value]) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let x = real_matrix_named(a, 0, "medfilt1", "X")?;
    let (mut n, mut dim, mut truncate, mut omit) = (3usize, None::<usize>, false, false);
    for i in 1..a.len() {
        if let Some(s) = text_arg(a, i) {
            match s.as_str() {
                "zeropad" => truncate = false,
                "truncate" => truncate = true,
                "omitnan" => omit = true,
                "includenan" => omit = false,
                _ => return rt(format!("The '{}' option is not recognized.", a[i].text().unwrap_or_default())),
            }
            continue;
        }
        let m = m_arg(a, i, "medfilt1")?;
        if m.numel() == 0 {
            continue;
        }
        if m.numel() != 1 {
            return rt(format!("Expected input number {}, {}, to be a scalar.", i + 1, if i == 1 { "N" } else { "DIM" }));
        }
        let v = m.data[0];
        if v < 0.0 {
            return rt(format!("Expected input number {}, {}, to be nonnegative.", i + 1, if i == 1 { "N" } else { "DIM" }));
        }
        if v.fract() != 0.0 {
            return rt(format!("Expected input number {}, {}, to be integer-valued.", i + 1, if i == 1 { "N" } else { "DIM" }));
        }
        // The first numeric argument is the order, a later one the dimension.
        if i == 1 {
            n = v as usize;
        } else {
            if v < 1.0 {
                return rt("Expected input number 4, DIM, to be positive.");
            }
            dim = Some(v as usize);
        }
    }
    let dim = dim.unwrap_or_else(|| first_dim(&x));
    let (left, right) = (n / 2, (n as isize - 1 - (n / 2) as isize).max(0) as usize);
    let m = along(&x, dim, &|s| {
        let len = s.len() as isize;
        if n == 0 {
            return Ok(vec![f64::NAN; s.len()]);
        }
        Ok((0..len)
            .map(|i| {
                let mut win: Vec<f64> = Vec::with_capacity(n);
                for j in i - left as isize..=i + right as isize {
                    if (0..len).contains(&j) {
                        win.push(s[j as usize]);
                    } else if !truncate {
                        win.push(0.0);
                    }
                }
                if omit {
                    win.retain(|v| !v.is_nan());
                } else if win.iter().any(|v| v.is_nan()) {
                    return f64::NAN;
                }
                median_of(&mut win)
            })
            .collect())
    })?;
    out(vec![Value::Num(m)])
}

// ---- downsample / upsample --------------------------------------------------------------------

fn resample_by(name: &str, a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    let x = real_matrix_named(a, 0, name, "X")?;
    if x.numel() == 0 {
        return rt("Expected X to be nonempty.");
    }
    let n = positive_integer(scalar_labeled(a, 1, name, "N")?, "N")?;
    let phase = if a.len() == 3 { scalar_labeled(a, 2, name, "PHASE")? } else { 0.0 };
    if phase < 0.0 {
        return rt("Expected PHASE to be nonnegative.");
    }
    if phase.fract() != 0.0 {
        return rt("Expected PHASE to be integer-valued.");
    }
    if phase > (n - 1) as f64 {
        return rt(format!("Expected PHASE to be a scalar with value <= {}.", n - 1));
    }
    let phase = phase as usize;
    let down = name == "downsample";
    let m = along(&x, first_dim(&x), &|s| {
        if down {
            Ok(s.iter().skip(phase).step_by(n).copied().collect())
        } else {
            let mut up = vec![0.0; s.len() * n];
            for (i, &v) in s.iter().enumerate() {
                up[phase + n * i] = v;
            }
            Ok(up)
        }
    })?;
    out(vec![Value::Num(m)])
}

// ---- upfirdn / fftfilt ------------------------------------------------------------------------

/// The columns of a matrix, or the single vector, and whether it was a row.
fn columns_of(m: &Matrix) -> (Vec<Vec<f64>>, bool) {
    if m.dims[0] == 1 {
        (vec![m.data.clone()], true)
    } else {
        ((0..m.dims[1]).map(|c| m.data[c * m.dims[0]..(c + 1) * m.dims[0]].to_vec()).collect(), false)
    }
}

/// Assemble columns into a matrix (a row vector when `row`).
fn from_columns(cols: Vec<Vec<f64>>, row: bool) -> Matrix {
    if row && cols.len() == 1 {
        return Matrix::row(cols.into_iter().next().unwrap());
    }
    let len = cols.first().map_or(0, |c| c.len());
    let n = cols.len();
    Matrix::new(len, n, cols.into_iter().flatten().collect())
}

/// Pair up the signal columns and filter columns: one of them may be a single vector.
fn paired<'a>(xs: &'a [Vec<f64>], hs: &'a [Vec<f64>]) -> R<Vec<(&'a Vec<f64>, &'a Vec<f64>)>> {
    if xs.len() == hs.len() {
        Ok(xs.iter().zip(hs).collect())
    } else if hs.len() == 1 {
        Ok(xs.iter().map(|x| (x, &hs[0])).collect())
    } else if xs.len() == 1 {
        Ok(hs.iter().map(|h| (&xs[0], h)).collect())
    } else {
        rt("The number of columns of the signal and the filter must match.")
    }
}

/// Upsample by p, filter with h, downsample by q (computed directly).
fn upfirdn(a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    let x = real_matrix_named(a, 0, "upfirdn", "X")?;
    if x.numel() == 0 {
        return rt("Expected X to be nonempty.");
    }
    let h = real_matrix_named(a, 1, "upfirdn", "H")?;
    if h.numel() == 0 {
        return rt("Expected H to be nonempty.");
    }
    let factor = |i: usize, label: &str| -> R<usize> {
        if a.len() <= i {
            return Ok(1);
        }
        positive_integer(scalar_labeled(a, i, "upfirdn", label)?, label)
    };
    let (p, q) = (factor(2, "P")?, factor(3, "Q")?);
    let (xs, xrow) = columns_of(&x);
    let (hs, _) = columns_of(&h);
    let results: Vec<Vec<f64>> = paired(&xs, &hs)?
        .into_iter()
        .map(|(s, hv)| {
            let full = (s.len() - 1) * p + hv.len();
            let mut y = vec![0.0; full];
            for (i, &v) in s.iter().enumerate() {
                for (j, &c) in hv.iter().enumerate() {
                    y[i * p + j] += v * c;
                }
            }
            y.into_iter().step_by(q).collect()
        })
        .collect();
    out(vec![Value::Num(from_columns(results, xrow && hs.len() == 1))])
}

/// FIR filtering of x by b. The reference computes it with overlap-add FFTs; the result is the
/// same up to rounding, so it is computed directly here.
fn fftfilt(a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    let b = real_matrix_named(a, 0, "fftfilt", "B")?;
    let x = real_matrix_named(a, 1, "fftfilt", "X")?;
    if b.numel() == 0 {
        return out(vec![Value::Num(Matrix::from_dims(x.dims.clone(), vec![0.0; x.numel()]))]);
    }
    let (bs, _) = columns_of(&b);
    let (xs, xrow) = columns_of(&x);
    let results: Vec<Vec<f64>> = paired(&xs, &bs)?
        .into_iter()
        .map(|(s, bv)| if bv.is_empty() { vec![0.0; s.len()] } else { filt(bv, &[1.0], s, &[]).0 })
        .collect();
    out(vec![Value::Num(from_columns(results, xrow && bs.len() == 1))])
}

// ---- impz / stepz -----------------------------------------------------------------------------

/// Response length chosen from the poles: until the slowest decaying (or fastest growing) term
/// reaches 5e-5 (1e6) of its starting size, or five periods of the slowest oscillation for
/// poles on the unit circle; numerator delays are added.
fn automatic_length(b: &[f64], den: &[f64]) -> usize {
    automatic_length_tol(b, den, 5e-5)
}

/// [`automatic_length`] for a decay target `dtol` (relative size of the slowest term).
pub(crate) fn automatic_length_tol(b: &[f64], den: &[f64], dtol: f64) -> usize {
    if den.iter().skip(1).all(|&v| v == 0.0) {
        return b.len();
    }
    let dc: Vec<C> = den.iter().map(|&v| C::new(v, 0.0)).collect();
    let poles = poly_roots(&dc);
    let mags: Vec<f64> = poles.iter().map(|p| p.abs()).collect();
    let rmax = mags.iter().cloned().fold(0.0, f64::max);
    let tol = 1e-5;
    let on_circle = |m: f64| (m - 1.0).abs() < tol;
    let delay = b.iter().take_while(|&&v| v == 0.0).count();
    let oscillation = {
        let angles: Vec<f64> = poles.iter().filter(|p| on_circle(p.abs())).map(|p| p.arg().abs()).filter(|&t| t > 1e-4).collect();
        angles.iter().cloned().fold(f64::INFINITY, f64::min)
    };
    let n = if rmax > 1.0 + tol {
        (1e6f64.ln() / rmax.ln()).floor()
    } else if on_circle(rmax) {
        let all_on_circle = mags.iter().all(|&m| on_circle(m));
        let osc = if oscillation.is_finite() { (5.0 * 2.0 * std::f64::consts::PI / oscillation).floor() } else { 10.0 };
        if all_on_circle {
            osc
        } else {
            let inner = mags.iter().cloned().filter(|&m| !on_circle(m)).fold(0.0, f64::max);
            let decay = if inner > 0.0 { (dtol.ln() / inner.ln()).floor() } else { 0.0 };
            osc.max(decay) - 1.0
        }
    } else if rmax == 0.0 {
        1.0
    } else {
        (dtol.ln() / rmax.ln()).floor()
    };
    let n = n.max(1.0) as usize + delay;
    n.max(b.len() + 1)
}

/// Impulse (`impz`) or step (`stepz`) response; the length (or a vector of sample indices) may be
/// given, otherwise it is chosen automatically.
fn impz(name: &str, a: &[Value], nargout: usize) -> Ret {
    if nargout == 0 {
        return rt(format!("{name} without output arguments plots the response, which is not supported; request [h, t]."));
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    let b = rvec_arg(a, 0, name)?;
    let den = if a.len() >= 2 && !matches!(&a[1], Value::Num(m) if m.numel() == 0) { rvec_arg(a, 1, name)? } else { vec![1.0] };
    if b.is_empty() || den.is_empty() || den[0] == 0.0 {
        return rt("Expected numerator and denominator to be nonempty with a nonzero first denominator coefficient.");
    }
    let idx: Vec<usize> = match a.get(2) {
        Some(Value::Num(m)) if m.numel() > 0 => {
            if m.numel() == 1 {
                (0..positive_integer(m.data[0], "N")?).collect()
            } else {
                if m.data.iter().any(|&v| v < 0.0 || v.fract() != 0.0) {
                    return rt("Expected the sample indices to be nonnegative integers.");
                }
                m.data.iter().map(|&v| v as usize).collect()
            }
        }
        _ => (0..automatic_length(&b, &den)).collect(),
    };
    let fs = if a.len() == 4 { Some(scalar_arg(a, 3, name)?) } else { None };
    if fs.is_some_and(|f| f <= 0.0) {
        return rt("Expected Fs to be positive.");
    }
    let len = idx.iter().copied().max().map_or(0, |m| m + 1);
    let input: Vec<f64> = if name == "impz" { (0..len).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect() } else { vec![1.0; len] };
    let (resp, _) = filt(&b, &den, &input, &[]);
    let h: Vec<f64> = idx.iter().map(|&i| resp[i]).collect();
    let t: Vec<f64> = idx.iter().map(|&i| i as f64 / fs.unwrap_or(1.0)).collect();
    out(vec![real_col(h), real_col(t)])
}

// ---- bitrevorder ------------------------------------------------------------------------------

/// Permute a vector or the columns of a matrix into bit-reversed order (length a power of 2).
fn bitrevorder(a: &[Value]) -> Ret {
    if a.len() != 1 {
        return rt(if a.is_empty() { "Not enough input arguments." } else { "Too many input arguments." });
    }
    let x = real_matrix_named(a, 0, "bitrevorder", "X")?;
    if x.numel() == 0 {
        return out(vec![Value::Num(x)]);
    }
    let n = x.dims[first_dim(&x) - 1];
    if !n.is_power_of_two() {
        return rt("The length of the input X must be an integer power of 2.");
    }
    let bits = n.trailing_zeros();
    let m = along(&x, first_dim(&x), &|s| Ok((0..n).map(|i| s[if bits == 0 { 0 } else { i.reverse_bits() >> (usize::BITS - bits) }]).collect()))?;
    out(vec![Value::Num(m)])
}
