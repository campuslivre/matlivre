//! Converters between linear-prediction parameter sets: `ac2poly`, `poly2ac`, `rc2is`, `is2rc`,
//! `rc2lar`, `lar2rc`, `poly2lsf`, `lsf2poly`.

use crate::builtins::Ret;
use crate::cplx::C;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["ac2poly", "poly2ac", "rc2is", "is2rc", "rc2lar", "lar2rc", "poly2lsf", "lsf2poly"];

const PI: f64 = std::f64::consts::PI;

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "ac2poly" => ac2poly(it, a, nargout),
        "poly2ac" => poly2ac(it, a, nargout),
        "poly2lsf" => poly2lsf(a, nargout),
        "lsf2poly" => lsf2poly(a, nargout),
        _ => elementwise(name, a, nargout),
    }
}

fn arity(a: &[Value], min: usize, max: usize, nargout: usize, max_out: usize) -> R<()> {
    if a.len() < min {
        return rt("Not enough input arguments.");
    }
    if a.len() > max {
        return rt("Too many input arguments.");
    }
    if nargout > max_out {
        return rt("Too many output arguments.");
    }
    Ok(())
}

fn elementwise(name: &str, a: &[Value], nargout: usize) -> Ret {
    arity(a, 1, 1, nargout, 1)?;
    let m = match &a[0] {
        Value::Num(m) => m.clone(),
        other => return rt(format!("Undefined function '{name}' for input arguments of type '{}'.", other.class_name())),
    };
    let check = matches!(name, "rc2is" | "rc2lar");
    if check && m.data.iter().any(|&k| k.abs() >= 1.0) {
        return rt("All reflection coefficients should have magnitude less than unity.");
    }
    let data: Vec<f64> = m
        .data
        .iter()
        .map(|&v| match name {
            "rc2is" => 2.0 / PI * v.asin(),
            "is2rc" => (PI * v / 2.0).sin(),
            "rc2lar" => ((1.0 + v) / (1.0 - v)).ln(),
            _ => (v / 2.0).tanh(),
        })
        .collect();
    let mut r = Matrix::from_dims(m.dims.clone(), data);
    if m.kind == Kind::Single {
        r.kind = Kind::Single;
    }
    out(vec![Value::Num(r)])
}

fn ac2poly(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    arity(a, 1, 1, nargout, 2)?;
    if let Value::Num(m) = &a[0] {
        if m.numel() == 0 {
            return rt("Expected input number 1, R, to be nonempty.");
        }
        if m.kind != Kind::Double {
            return rt("Input arguments must be 'double'.");
        }
    } else {
        return rt("Input arguments must be 'double'.");
    }
    let r = it.call_function("levinson", vec![a[0].clone()], nargout.max(1).min(2))?;
    Ok(Some(r))
}

fn poly2ac(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    arity(a, 2, 2, nargout, 1)?;
    let m = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        other => {
            return rt(format!(
                "Input A(Prediction Polynomial) in poly2ac must be double/single, instead it was {}.",
                other.class_name()
            ))
        }
    };
    if m.dims.len() > 2 || (m.dims[0] > 1 && m.dims[1] > 1) {
        return rt("The prediction polynomial must be stored in a vector.");
    }
    if m.numel() < 2 {
        return rt("Polynomial should have at least two coefficients");
    }
    let e = match &a[1] {
        Value::Num(e) if e.numel() == 1 => e.clone(),
        Value::Num(_) => return rt("Unable to perform assignment because the left and right sides have a different number of elements."),
        _ => return rt("Not enough input arguments."),
    };
    if m.data[0] == 0.0 {
        return out(vec![Value::Num(Matrix::from_dims(vec![m.numel(), 1], vec![f64::NAN; m.numel()]))]);
    }
    let k = it.call_function("poly2rc", vec![a[0].clone(), Value::Num(e)], 2)?;
    let r = it.call_function("rc2ac", vec![k[0].clone(), k[1].clone()], 1)?;
    Ok(Some(r))
}

fn float_vec(a: &[Value], label: &str, func: &str) -> R<Matrix> {
    match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => Ok(m.clone()),
        other => rt(format!("Input {label} in {func} must be double/single, instead it was {}.", other.class_name())),
    }
}

fn poly_from(roots: &[C]) -> Vec<C> {
    poly_of(roots)
}

fn poly2lsf(a: &[Value], nargout: usize) -> Ret {
    arity(a, 1, 1, nargout, 1)?;
    let m = float_vec(a, "A(prediction polynomial)", "poly2lsf")?;
    if m.numel() == 0 || m.dims.len() > 2 || (m.dims[0] > 1 && m.dims[1] > 1) {
        return rt("The prediction polynomial must be stored in a vector.");
    }
    let mut av = m.data.clone();
    if av.iter().any(|v| v.is_nan() || v.is_infinite()) || av[0] == 0.0 {
        return rt("Input to ROOTS must not contain NaN or Inf.");
    }
    let a0 = av[0];
    for v in av.iter_mut() {
        *v /= a0;
    }
    let p = av.len() - 1;
    if p == 0 {
        return out(vec![Value::Num(Matrix::from_dims(vec![0, 1], vec![]))]);
    }
    let roots = roots_matlab(&av.iter().map(|&v| C::new(v, 0.0)).collect::<Vec<_>>())?;
    if roots.iter().any(|r| r.abs() >= 1.0) {
        return rt("The polynomial must have all roots inside of the unit circle.");
    }
    let mut pp = av.clone();
    pp.push(0.0);
    let mut qq = pp.clone();
    for i in 0..=p {
        pp[i + 1] += av[p - i];
        qq[i + 1] -= av[p - i];
    }
    // Remove the trivial roots at z = -1 and/or z = 1.
    let divide = |poly: Vec<f64>, c: f64| -> Vec<f64> {
        // Divide by (1 + c z^-1).
        let mut q = vec![0.0; poly.len() - 1];
        let mut prev = 0.0;
        for i in 0..q.len() {
            q[i] = poly[i] - c * prev;
            prev = q[i];
        }
        q
    };
    let (pp, qq) = if p % 2 == 0 {
        (divide(pp, 1.0), divide(qq, -1.0))
    } else {
        (pp, divide(divide(qq, 1.0), -1.0))
    };
    let mut ang = vec![];
    for poly in [pp, qq] {
        if poly.len() > 1 {
            let r = roots_matlab(&poly.iter().map(|&v| C::new(v, 0.0)).collect::<Vec<_>>())?;
            for z in r {
                if z.im > 0.0 {
                    ang.push(z.im.atan2(z.re));
                }
            }
        }
    }
    ang.sort_by(|x, y| x.partial_cmp(y).unwrap());
    out(vec![real_col(ang)])
}

fn lsf2poly(a: &[Value], nargout: usize) -> Ret {
    arity(a, 1, 1, nargout, 1)?;
    let m = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        other => {
            return rt(format!(
                "Input LSF(Line spectral frequencies) in poly2lsf must be double/single, instead it was {}.",
                other.class_name()
            ))
        }
    };
    if m.numel() == 0 {
        return out(vec![Value::Num(Matrix::from_dims(vec![0, 1], vec![]))]);
    }
    let vector = m.dims[0] == 1 || m.dims[1] == 1;
    if vector && m.numel() == 1 {
        return rt("Argument must be a vector or a square matrix.");
    }
    if m.data.iter().any(|&w| w < 0.0 || w > PI) {
        return rt("Line spectral frequencies must be between 0 and pi.");
    }
    let (rows, cols) = if vector { (m.numel(), 1) } else { (m.dims[0], m.numel() / m.dims[0]) };
    let mut data = vec![];
    for c in 0..cols {
        let w: Vec<f64> = if vector { m.data.clone() } else { m.data[c * rows..(c + 1) * rows].to_vec() };
        data.extend(one_lsf(&w));
    }
    let n = rows + 1;
    let mat = if vector {
        Matrix::from_dims(vec![1, n], data)
    } else {
        let mut t = vec![0.0; n * cols];
        for c in 0..cols {
            for j in 0..n {
                t[j * cols + c] = data[c * n + j];
            }
        }
        Matrix::from_dims(vec![cols, n], t)
    };
    let mut mat = mat;
    if m.kind == Kind::Single {
        mat.kind = Kind::Single;
    }
    out(vec![Value::Num(mat)])
}

fn conv(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut o = vec![0.0; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            o[i + j] += x * y;
        }
    }
    o
}

fn one_lsf(w: &[f64]) -> Vec<f64> {
    let p = w.len();
    let q = |t: f64| vec![1.0, -2.0 * t.cos(), 1.0];
    let (mut pp, mut qq) = (vec![1.0], vec![1.0]);
    for (i, &t) in w.iter().enumerate() {
        if i % 2 == 0 { pp = conv(&pp, &q(t)) } else { qq = conv(&qq, &q(t)) }
    }
    if p % 2 == 0 {
        pp = conv(&pp, &[1.0, 1.0]);
        qq = conv(&qq, &[1.0, -1.0]);
    } else {
        qq = conv(&qq, &[1.0, 0.0, -1.0]);
    }
    let len = pp.len().max(qq.len());
    (0..=p).map(|i| 0.5 * (pp.get(i).copied().unwrap_or(0.0) + qq.get(i).copied().unwrap_or(0.0))).take(len.min(p + 1)).collect()
}
