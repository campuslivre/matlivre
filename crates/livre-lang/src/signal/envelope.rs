//! `envelope`: analytic, rms and peak envelopes of a signal.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["envelope"];

const NUM_TYPES: &str = "double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64";

fn col_value(v: Vec<f64>) -> Value {
    real_col(v)
}

fn peak_envelope(it: &mut Interpreter, x: &[f64], n: usize, upper: bool) -> R<Vec<f64>> {
    let m = x.len();
    let sgn = if upper { 1.0 } else { -1.0 };
    let y: Vec<f64> = x.iter().map(|v| sgn * v).collect();
    let mut knots: Vec<(f64, f64)> = vec![];
    if (n as f64) < (m as f64 - 1.0) {
        let res = it.call_function("findpeaks", vec![col_value(y.clone()), Value::chars("MinPeakDistance"), scalar_value(n as f64)], 2)?;
        let pk = res[0].to_matrix()?;
        let loc = res[1].to_matrix()?;
        for k in 0..loc.numel() {
            knots.push((loc.data[k], pk.data[k]));
        }
    }
    if knots.len() < 2 {
        knots.insert(0, (1.0, y[0]));
        knots.push((m as f64, y[m - 1]));
    }
    let xs = real_row(knots.iter().map(|k| k.0).collect());
    let ys = real_row(knots.iter().map(|k| k.1).collect());
    let grid = real_row((1..=m).map(|i| i as f64).collect());
    let r = it.call_function("spline", vec![xs, ys, grid], 1)?.remove(0).to_matrix()?;
    Ok(r.data.iter().map(|v| sgn * v).collect())
}

pub fn call(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        Value::Num(m) if m.kind == Kind::Char => return rt("Expected input to be one of these types:  single, double"),
        other => return rt(format!("Expected input to be one of these types:  single, double  Instead its type was {}.", other.class_name())),
    };
    if x.im.is_some() {
        return rt("Expected input to be real.");
    }
    if x.dims.len() > 2 {
        return rt("Expected input to be two-dimensional.");
    }
    let mut n = 0usize;
    if a.len() >= 2 {
        let m = match &a[1] {
            Value::Num(m) if m.kind != Kind::Char && m.kind != Kind::Logical => m.clone(),
            Value::Num(m) if m.kind == Kind::Char => return rt(format!("Expected input number 2, N, to be one of these types:  {NUM_TYPES}")),
            other => return rt(format!("Expected input number 2, N, to be one of these types:  {NUM_TYPES}  Instead its type was {}.", other.class_name())),
        };
        if m.numel() != 1 {
            return rt("Expected input number 2, N, to be a scalar.");
        }
        if !(m.data[0] > 0.0) {
            return rt("Expected input number 2, N, to be positive.");
        }
        if m.data[0].fract() != 0.0 {
            return rt("Expected input number 2, N, to be integer-valued.");
        }
        n = m.data[0] as usize;
    }
    let mut kind = 0; // analytic, rms, peak
    if a.len() == 3 {
        let s = a[2].text().unwrap_or_default();
        let l = s.to_lowercase();
        let opts = ["analytic", "rms", "peaks"];
        match opts.iter().position(|o| !l.is_empty() && o.starts_with(&l)) {
            Some(i) => kind = i,
            None => return rt(format!("Expected input to match one of these values:  'analytic', 'rms', 'peaks'  The input, '{s}', did not match any of the valid values.")),
        }
    }
    if kind == 0 && a.len() >= 2 {
        return rt("envelope: the N-tap Hilbert filter option is not implemented.");
    }

    let single = x.kind == Kind::Single;
    let (rows, cols) = (x.dims[0], x.dims[1]);
    let vector = rows == 1 || cols == 1;
    let (len, nch) = if vector { (rows * cols, 1) } else { (rows, cols) };
    let mut up = vec![0.0; rows * cols];
    let mut lo = vec![0.0; rows * cols];
    if len > 0 {
        for c in 0..nch {
            let col: Vec<f64> = x.data[c * len..(c + 1) * len].to_vec();
            let (u, l) = if len == 1 {
                (col.clone(), col.clone())
            } else {
                match kind {
                    0 => {
                        let mean = col.iter().sum::<f64>() / len as f64;
                        let centered: Vec<f64> = col.iter().map(|v| v - mean).collect();
                        let h = it.call_function("hilbert", vec![col_value(centered)], 1)?.remove(0).to_matrix()?;
                        let mag: Vec<f64> = (0..len).map(|k| h.data[k].hypot(h.im.as_ref().map_or(0.0, |im| im[k]))).collect();
                        (mag.iter().map(|v| mean + v).collect(), mag.iter().map(|v| mean - v).collect())
                    }
                    1 => {
                        let mean = col.iter().sum::<f64>() / len as f64;
                        let sq: Vec<f64> = col.iter().map(|v| (v - mean) * (v - mean)).collect();
                        let (nb, na) = if n % 2 == 1 { ((n - 1) / 2, (n - 1) / 2) } else { (n / 2 - 1, n / 2) };
                        let mm = it.call_function("movmean", vec![col_value(sq), real_row(vec![nb as f64, na as f64])], 1)?.remove(0).to_matrix()?;
                        (mm.data.iter().map(|v| mean + v.sqrt()).collect(), mm.data.iter().map(|v| mean - v.sqrt()).collect())
                    }
                    _ => (peak_envelope(it, &col, n, true)?, peak_envelope(it, &col, n, false)?),
                }
            };
            up[c * len..(c + 1) * len].copy_from_slice(&u);
            lo[c * len..(c + 1) * len].copy_from_slice(&l);
        }
    }
    let mk = |v: Vec<f64>| {
        let mut m = Matrix::from_dims(x.dims.clone(), v);
        if single {
            m.data.iter_mut().for_each(|e| *e = *e as f32 as f64);
            m.kind = Kind::Single;
        }
        Value::Num(m)
    };
    let mut outs = vec![mk(up)];
    if nargout > 1 {
        outs.push(mk(lo));
    }
    out(outs)
}
