//! `stmcb`: Steiglitz-McBride iteration for a rational model of an impulse response or of an
//! input/output pair.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::ar::lstsq;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["stmcb"];

fn count_arg(a: &[Value], i: usize, label: &str) -> R<usize> {
    let m = match &a[i] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt(format!("Expected input number {}, {label}, to be numeric.", i + 1)),
    };
    if m.numel() != 1 {
        return rt(format!("Expected input number {}, {label}, to be a scalar.", i + 1));
    }
    let v = m.data[0];
    if v < 0.0 {
        return rt(format!("Expected input number {}, {label}, to be nonnegative.", i + 1));
    }
    if v.fract() != 0.0 {
        return rt(format!("Expected input number {}, {label}, to be integer-valued.", i + 1));
    }
    Ok(v as usize)
}

fn col(m: &Matrix) -> Matrix {
    let mut c = m.clone();
    c.dims = vec![m.numel(), 1];
    c
}

pub fn call(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Index exceeds the number of array elements. Index must not exceed 0.");
    }
    if a.len() == 1 {
        return rt("Index exceeds the number of array elements. Index must not exceed 1.");
    }
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let second_vec = matches!(&a[1], Value::Num(m) if m.numel() > 1);
    let yx = second_vec || a.len() >= 6;
    let (y_label, base) = if yx { ("Y", 2) } else { ("H", 1) };
    let y = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt(format!("Expected input number 1, {y_label}, to be one of these types:  single, double")),
    };
    if yx && a.len() < 4 {
        return rt("Not enough input arguments.");
    }
    let single = y.kind == Kind::Single;
    let y = col(&y);
    let n = y.numel();
    let x = if yx {
        let x = match &a[1] {
            Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => col(m),
            _ => return rt("Expected input number 2, X, to be one of these types:  single, double"),
        };
        x
    } else {
        let mut d = vec![0.0; n];
        if n > 0 {
            d[0] = 1.0;
        }
        Matrix::from_dims(vec![n, 1], d)
    };
    let nb = count_arg(a, base, "NB")?;
    let na = count_arg(a, base + 1, "NA")?;
    let iters = if a.len() > base + 2 { count_arg(a, base + 2, "N")? } else { 5 };
    if yx && x.numel() != n {
        return rt("Input signal X and output signal Y must have the same length.");
    }

    let mut ai: Value = if a.len() > base + 3 {
        match &a[base + 3] {
            Value::Num(m) => Value::Num(m.clone()),
            _ => return rt("Expected input number 6, Ai, to be numeric."),
        }
    } else {
        it.call_function("prony", vec![Value::Num(y.clone()), scalar_value(0.0), scalar_value(na as f64)], 2)?.remove(1)
    };
    let zeros_out = |single: bool| -> Ret {
        let mk = |k: usize| {
            let mut m = Matrix::from_dims(vec![1, k], vec![0.0; k]);
            if single {
                m.kind = Kind::Single;
            }
            Value::Num(m)
        };
        let mut o = vec![mk(nb + 1)];
        if nargout > 1 {
            o.push(mk(na + 1));
        }
        out(o)
    };
    if iters == 0 || matches!(&ai, Value::Num(m) if m.numel() == 0) {
        return zeros_out(single);
    }

    let one = scalar_value(1.0);
    let (mut b_re, mut b_im, mut a_re, mut a_im) = (vec![], vec![], vec![], vec![]);
    let cplx = y.im.is_some() || x.im.is_some();
    for _ in 0..iters {
        let w = it.call_function("filter", vec![one.clone(), ai.clone(), Value::Num(y.clone())], 1)?.remove(0).to_matrix()?;
        let u = it.call_function("filter", vec![one.clone(), ai.clone(), Value::Num(x.clone())], 1)?.remove(0).to_matrix()?;
        let nu = nb + 1 + na;
        let wim = w.im.clone().unwrap_or_else(|| vec![0.0; n]);
        let uim = u.im.clone().unwrap_or_else(|| vec![0.0; n]);
        let mut cols_re: Vec<Vec<f64>> = vec![];
        let mut cols_im: Vec<Vec<f64>> = vec![];
        for k in 0..=nb {
            cols_re.push((0..n).map(|i| if i >= k { u.data[i - k] } else { 0.0 }).collect());
            cols_im.push((0..n).map(|i| if i >= k { uim[i - k] } else { 0.0 }).collect());
        }
        for k in 1..=na {
            cols_re.push((0..n).map(|i| if i >= k { -w.data[i - k] } else { 0.0 }).collect());
            cols_im.push((0..n).map(|i| if i >= k { -wim[i - k] } else { 0.0 }).collect());
        }
        let wcomplex = cplx || w.im.is_some() || u.im.is_some();
        let (th_re, th_im): (Vec<f64>, Vec<f64>) = if !wcomplex {
            (lstsq(&cols_re, &w.data), vec![0.0; nu])
        } else {
            let mut re = Vec::with_capacity(n * nu);
            let mut im = Vec::with_capacity(n * nu);
            for k in 0..nu {
                re.extend_from_slice(&cols_re[k]);
                im.extend_from_slice(&cols_im[k]);
            }
            let am = Value::Num(Matrix::complex(vec![n, nu], re, im));
            let wm = Value::Num(Matrix::complex(vec![n, 1], w.data.clone(), wim.clone()));
            let sol = it.call_function("mldivide", vec![am, wm], 1)?.remove(0).to_matrix()?;
            (sol.data.clone(), sol.im.clone().unwrap_or_else(|| vec![0.0; nu]))
        };
        b_re = th_re[..nb + 1].to_vec();
        b_im = th_im[..nb + 1].to_vec();
        a_re = vec![1.0];
        a_re.extend_from_slice(&th_re[nb + 1..]);
        a_im = vec![0.0];
        a_im.extend_from_slice(&th_im[nb + 1..]);
        ai = if wcomplex {
            Value::Num(Matrix::complex(vec![1, a_re.len()], a_re.clone(), a_im.clone()))
        } else {
            real_row(a_re.clone())
        };
    }
    let build = |re: Vec<f64>, im: Vec<f64>| -> Value {
        let k = re.len();
        let mut m = if cplx { Matrix::complex(vec![1, k], re, im) } else { Matrix::from_dims(vec![1, k], re) };
        if single {
            m.data.iter_mut().for_each(|e| *e = *e as f32 as f64);
            if let Some(i) = m.im.as_mut() {
                i.iter_mut().for_each(|e| *e = *e as f32 as f64);
            }
            m.kind = Kind::Single;
        }
        Value::Num(m)
    };
    let mut outs = vec![build(b_re, b_im)];
    if nargout > 1 {
        outs.push(build(a_re, a_im));
    }
    out(outs)
}
