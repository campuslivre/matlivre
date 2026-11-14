//! Resampling helpers: `decimate`, `interp`, `intfilt`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["decimate", "interp", "intfilt"];

const PI: f64 = std::f64::consts::PI;

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "decimate" => decimate(it, a, nargout),
        "interp" => interp(it, a, nargout),
        _ => intfilt(a, nargout),
    }
}

fn scalar_checks(m: &Matrix, label: &str, positive_only: bool, min1: bool) -> R<f64> {
    if m.numel() != 1 {
        return rt(format!("Expected {label} to be a scalar."));
    }
    let v = m.data[0];
    if min1 && v < 1.0 {
        return rt(format!("Expected {label} to be a scalar with value >= 1."));
    }
    if positive_only && v <= 0.0 {
        return rt(format!("Expected {label} to be positive."));
    }
    if v.fract() != 0.0 {
        return rt(format!("Expected {label} to be integer-valued."));
    }
    Ok(v)
}

fn filter_row(b: &[f64], x: &[f64]) -> Vec<f64> {
    (0..x.len()).map(|i| (0..b.len().min(i + 1)).map(|k| b[k] * x[i - k]).sum()).collect()
}

fn decimate(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt("Expected X to be one of these types:  double, single"),
    };
    if x.numel() == 0 || x.dims.len() > 2 || (x.dims[0] > 1 && x.dims[1] > 1) {
        return rt("Expected X to be a vector.");
    }
    let rm = crate::builtins::m_arg(a, 1, "decimate")?;
    let r = scalar_checks(&rm, "R", true, false)?;
    let mut fir = false;
    let mut order: Option<f64> = None;
    for (i, v) in a.iter().enumerate().skip(2) {
        match v {
            Value::Num(m) if m.kind == Kind::Char && !(i == 2 && a.len() > 3) => {
                match m.to_string_lossy().to_lowercase().as_str() {
                    "fir" => fir = true,
                    "iir" => fir = false,
                    _ => return rt("Filter options are FIR and IIR."),
                }
            }
            Value::Num(m) if m.kind == Kind::Char => return rt("Expected N to be one of these types:  double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64"),
            Value::Num(m) if i == 2 => order = Some(scalar_checks(m, "N", true, false)?),
            _ => return rt("Expected N to be one of these types:  double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64"),
        }
    }
    let r = r as usize;
    let single = x.kind == Kind::Single;
    if x.im.is_some() {
        // Complex data: filter the parts separately.
        let re = Matrix::from_dims(x.dims.clone(), x.data.clone());
        let im = Matrix::from_dims(x.dims.clone(), (0..x.numel()).map(|i| x.im_at(i)).collect());
        let mut args_re = a.to_vec();
        args_re[0] = Value::Num(re);
        let mut args_im = a.to_vec();
        args_im[0] = Value::Num(im);
        let yr = decimate(it, &args_re, 1)?.unwrap().remove(0).to_matrix()?;
        let yi = decimate(it, &args_im, 1)?.unwrap().remove(0).to_matrix()?;
        return out(vec![Value::Num(Matrix::complex(yr.dims.clone(), yr.data.clone(), yi.data.clone()))]);
    }
    if r == 1 {
        return out(vec![a[0].clone()]);
    }
    let xv = x.data.clone();
    let n = xv.len();
    let y: Vec<f64> = if fir {
        let nf = order.unwrap_or(30.0) as usize;
        let nf = nf + nf % 2;
        let bb = it
            .call_function("fir1", vec![scalar_value(nf as f64), scalar_value(1.0 / r as f64)], 1)?
            .remove(0)
            .to_matrix()?
            .data;
        let nd = nf / 2;
        if nf >= n {
            return rt(format!("Index exceeds the number of array elements. Index must not exceed {n}."));
        }
        let mut xp = vec![];
        for k in (1..=nd).rev() {
            xp.push(2.0 * xv[0] - xv[k]);
        }
        xp.extend(&xv);
        for k in 1..=nd {
            xp.push(2.0 * xv[n - 1] - xv[n - 1 - k]);
        }
        let yf = filter_row(&bb, &xp);
        yf[2 * nd..].iter().step_by(r).copied().collect()
    } else {
        let ord = order.unwrap_or(8.0);
        if (n as f64) <= 3.0 * ord {
            return rt("Data length must be larger than 24 samples.");
        }
        let ba = it.call_function("cheby1", vec![scalar_value(ord), scalar_value(0.05), scalar_value(0.8 / r as f64)], 2)?;
        let yf = it.call_function("filtfilt", vec![ba[0].clone(), ba[1].clone(), real_row(xv.clone())], 1)?.remove(0).to_matrix()?.data;
        let start = (n - 1) % r;
        yf.iter().skip(start).step_by(r).copied().collect()
    };
    let mut m = if x.dims[0] == 1 { Matrix::from_dims(vec![1, y.len()], y) } else { Matrix::from_dims(vec![y.len(), 1], y) };
    if single {
        m = crate::signal::cast_single_values(m);
        m.kind = Kind::Single;
    }
    out(vec![Value::Num(m)])
}

/// Bandlimited interpolation filter of length 2 l n - 1.
fn intfilt_coefficients(l: usize, n: usize, alpha: f64) -> Vec<f64> {
    let len = 2 * l * n - 1;
    let centre = l * n - 1;
    let mut b = vec![0.0; len];
    b[centre] = 1.0;
    let taps = 2 * n;
    let g = |d: f64| -> f64 { if d == 0.0 { alpha * PI } else { (alpha * PI * d).sin() / d } };
    for p in 1..l {
        let delta = p as f64 / l as f64;
        // m = -n+1 ..= n
        let ms: Vec<f64> = (0..taps).map(|k| k as f64 - n as f64 + 1.0).collect();
        let mut mat = vec![vec![0.0; taps]; taps];
        let mut rhs = vec![0.0; taps];
        for i in 0..taps {
            for j in 0..taps {
                mat[i][j] = g(ms[i] - ms[j]);
            }
            rhs[i] = g(ms[i] - delta);
        }
        let h = solve(mat, rhs);
        for (k, &m) in ms.iter().enumerate() {
            let j = p as i64 - l as i64 * m as i64;
            let idx = centre as i64 + j;
            if idx >= 0 && (idx as usize) < len {
                b[idx as usize] = h[k];
            }
        }
    }
    b
}

fn solve(mut m: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for c in 0..n {
        let piv = (c..n).max_by(|&x, &y| m[x][c].abs().partial_cmp(&m[y][c].abs()).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
        m.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..n {
            let f = m[r][c] / m[c][c];
            for k in c..n {
                m[r][k] -= f * m[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for k in r + 1..n {
            s -= m[r][k] * x[k];
        }
        x[r] = s / m[r][r];
    }
    x
}

fn intfilt(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let num = |i: usize| -> R<Matrix> {
        match &a[i] {
            Value::Num(m) if m.kind != Kind::Char => Ok(m.clone()),
            _ => rt("Not enough input arguments."),
        }
    };
    let (lm, nm, am) = (num(0)?, num(1)?, num(2)?);
    if lm.numel() != 1 || nm.numel() != 1 || lm.data[0] < 1.0 || nm.data[0] < 1.0 || lm.data[0].fract() != 0.0 || nm.data[0].fract() != 0.0 {
        return rt("Not enough input arguments.");
    }
    if am.numel() != 1 {
        return rt("Frequencies in F must be in range [0,1].");
    }
    let alpha = am.data[0];
    if alpha > 1.0 {
        return rt("Frequencies in F must be non decreasing.");
    }
    if alpha < 0.0 {
        return rt("Frequencies in F must be in range [0,1].");
    }
    let mut b = intfilt_coefficients(lm.data[0] as usize, nm.data[0] as usize, alpha);
    if alpha == 0.0 {
        b = vec![f64::NAN; b.len()];
    }
    let mut outv = vec![real_row(b)];
    if nargout > 1 {
        outv.push(scalar_value(1.0));
    }
    out(outv)
}

fn interp(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt("Expected x to be one of these types:  single, double"),
    };
    if x.numel() == 0 || x.dims.len() > 2 || (x.dims[0] > 1 && x.dims[1] > 1) {
        return rt("Expected x to be a vector.");
    }
    let lm = crate::builtins::m_arg(a, 1, "interp")?;
    let l = scalar_checks(&lm, "r", false, true)? as usize;
    let n = if a.len() > 2 {
        let m = crate::builtins::m_arg(a, 2, "interp")?;
        scalar_checks(&m, "n", false, true)? as usize
    } else {
        4
    };
    let alpha = if a.len() > 3 {
        match &a[3] {
            Value::Num(m) if m.kind != Kind::Char => {
                if m.numel() != 1 {
                    return rt("Expected alpha to be a scalar.");
                }
                if m.data[0] > 1.0 {
                    return rt("Expected alpha to be a scalar with value <= 1.");
                }
                if m.data[0] <= 0.0 {
                    return rt("Expected alpha to be a scalar with value > 0.");
                }
                m.data[0]
            }
            _ => return rt("Expected alpha to be one of these types:  double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64"),
        }
    } else {
        0.5
    };
    let nx = x.numel();
    if nx < 2 * n + 1 {
        return rt(format!("Length of data sequence must be at least {}.  You either need more data or a shorter filter (L).", 2 * n + 1));
    }
    if x.im.is_some() {
        let re = Matrix::from_dims(x.dims.clone(), x.data.clone());
        let im = Matrix::from_dims(x.dims.clone(), (0..x.numel()).map(|i| x.im_at(i)).collect());
        let mut ar = a.to_vec();
        ar[0] = Value::Num(re);
        let mut ai = a.to_vec();
        ai[0] = Value::Num(im);
        let yr = interp(it, &ar, 2)?.unwrap();
        let yi = interp(it, &ai, 1)?.unwrap();
        let (r, i) = (yr[0].to_matrix()?, yi[0].to_matrix()?);
        let mut outv = vec![Value::Num(Matrix::complex(r.dims.clone(), r.data.clone(), i.data.clone()))];
        if nargout > 1 {
            outv.push(yr[1].clone());
        }
        return out(outv);
    }
    let b = intfilt_coefficients(l, n, alpha);
    let xv = x.data.clone();
    let centre = l * n - 1;
    // Upsample, filter and align so that the original samples are kept.
    let total = l * nx;
    let mut y = vec![0.0; total];
    let at = |k: i64| -> f64 {
        let last = nx as i64 - 1;
        if k < 0 {
            2.0 * xv[0] - xv[(-k).min(last) as usize]
        } else if k > last {
            2.0 * xv[nx - 1] - xv[(2 * last - k).max(0) as usize]
        } else {
            xv[k as usize]
        }
    };
    for (i, yi) in y.iter_mut().enumerate() {
        let mut s = 0.0;
        for (j, &bj) in b.iter().enumerate() {
            let pos = i as i64 + centre as i64 - j as i64;
            if pos.rem_euclid(l as i64) == 0 {
                s += bj * at(pos.div_euclid(l as i64));
            }
        }
        *yi = s;
    }
    let mut m = if x.dims[0] == 1 { Matrix::from_dims(vec![1, total], y) } else { Matrix::from_dims(vec![total, 1], y) };
    if x.kind == Kind::Single {
        m = crate::signal::cast_single_values(m);
        m.kind = Kind::Single;
    }
    let mut bfull = vec![0.0];
    bfull.extend(&b);
    bfull.push(0.0);
    let mut bm = Matrix::from_dims(vec![bfull.len(), 1], bfull);
    if x.kind == Kind::Single {
        bm = crate::signal::cast_single_values(bm);
        bm.kind = Kind::Single;
    }
    let mut outv = vec![Value::Num(m), Value::Num(bm)];
    outv.truncate(nargout.max(1));
    out(outv)
}
