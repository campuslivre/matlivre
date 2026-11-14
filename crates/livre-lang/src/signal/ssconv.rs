//! State-space conversions: `tf2ss`, `zp2ss`, `ss2zp`, `sos2ss`, `ss2sos`.

use crate::builtins::{m_arg, Ret};
use crate::cplx::C;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::signal::statespace;
use crate::value::*;

pub const NAMES: &[&str] = &["tf2ss", "zp2ss", "ss2zp", "sos2ss", "ss2sos"];

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "tf2ss" => tf2ss(a, nargout),
        "zp2ss" => zp2ss(a, nargout),
        "ss2zp" => ss2zp(it, a, nargout),
        "sos2ss" => sos2ss(it, a, nargout),
        _ => ss2sos(it, a, nargout),
    }
}

fn real_matrix_value(rows: usize, cols: usize, data: Vec<f64>) -> Value {
    Value::Num(Matrix::from_dims(vec![rows, cols], data))
}

/// A, B, C, D of the controllable canonical form of num(s)/den(s), one output per numerator row.
fn canonical(num: &Matrix, den: &[f64]) -> Result<Vec<Value>, crate::error::LangError> {
    let lead = den.iter().take_while(|&&v| v == 0.0).count();
    if lead == den.len() {
        return rt("Denominator cannot be zero.");
    }
    let den = &den[lead..];
    let n = den.len() - 1;
    let rows = num.dims[0];
    let cols = num.dims[1];
    if cols > den.len() {
        return rt("Order of denominator must be greater than or equal to order of numerator.");
    }
    let d0 = den[0];
    let a: Vec<f64> = den.iter().map(|v| v / d0).collect();
    // Numerator rows padded on the left to the length of the denominator.
    let at = |r: usize, k: usize| -> f64 {
        let off = den.len() - cols;
        if k < off { 0.0 } else { num.data[(k - off) * rows + r] / d0 }
    };
    let mut am = vec![0.0; n * n];
    if n > 0 {
        for j in 0..n {
            am[j * n] = -a[j + 1];
        }
        for i in 1..n {
            am[(i - 1) * n + i] = 1.0;
        }
    }
    let mut bm = vec![0.0; n];
    if n > 0 {
        bm[0] = 1.0;
    }
    let mut cm = vec![0.0; rows * n];
    let mut dm = vec![0.0; rows];
    for r in 0..rows {
        dm[r] = at(r, 0);
        for k in 0..n {
            cm[k * rows + r] = at(r, k + 1) - at(r, 0) * a[k + 1];
        }
    }
    Ok(vec![real_matrix_value(n, n, am), real_matrix_value(n, if n == 0 { 0 } else { 1 }, bm), real_matrix_value(if n == 0 { 0 } else { rows }, n, cm), real_matrix_value(rows, 1, dm)])
}

fn tf2ss(a: &[Value], nargout: usize) -> Ret {
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if nargout > 4 {
        return rt("Too many output arguments.");
    }
    let num = m_arg(a, 0, "tf2ss")?;
    let den = m_arg(a, 1, "tf2ss")?;
    if (den.dims[0] > 1 && den.dims[1] > 1) || den.dims.len() > 2 || den.numel() == 0 {
        return rt("Denominator must be a row vector.");
    }
    out(canonical(&num, &den.data)?)
}

fn zp2ss(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 4 {
        return rt("Too many output arguments.");
    }
    let z = cvec_arg(a, 0, "zp2ss")?;
    let p = cvec_arg(a, 1, "zp2ss")?;
    let k = scalar_arg(a, 2, "zp2ss")?;
    if z.len() > p.len() {
        return rt("Must be a proper system.");
    }
    let s = statespace::zp2ss(&z, &p, k);
    out(vec![
        real_matrix_value(s.n, s.n, s.a),
        real_matrix_value(s.n, 1, s.b),
        real_matrix_value(1, s.n, s.c),
        real_matrix_value(1, 1, vec![s.d]),
    ])
}

/// Eigenvalues of a real n x n matrix in LAPACK order.
fn eig_vals(n: usize, a: &[f64]) -> R<Vec<C>> {
    if n == 0 {
        return Ok(vec![]);
    }
    if let Some(ev) = crate::signal::lapack_eig::eigenvalues(n, a) {
        return Ok(ev.into_iter().map(|(r, i)| C::new(r, i)).collect());
    }
    let m = Matrix::from_dims(vec![n, n], a.to_vec());
    let Value::Num(v) = crate::builtins_matfun::eig_values(&m)? else { return Ok(vec![]) };
    Ok((0..v.numel()).map(|k| C::new(v.data[k], v.im_at(k))).collect())
}

/// Characteristic polynomial coefficients (leading 1) of a real matrix.
fn charpoly(n: usize, a: &[f64]) -> R<Vec<f64>> {
    Ok(poly_of(&eig_vals(n, a)?).into_iter().map(|c| c.re).collect())
}

fn ss2zp(_it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 4 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 5 {
        return rt("Too many input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let mut ms = vec![];
    for i in 0..4 {
        let m = m_arg(a, i, "ss2zp")?;
        if !matches!(m.kind, Kind::Double | Kind::Single) {
            return rt("Invalid data type. Input matrix must be double or single.");
        }
        ms.push(m);
    }
    let (am, bm, cm, dm) = (&ms[0], &ms[1], &ms[2], &ms[3]);
    let n = am.dims[0];
    if am.dims[1] != n {
        return rt("The A matrix must be square.");
    }
    if bm.dims[0] != n {
        return rt("The A and B matrices must have the same number of rows.");
    }
    if cm.dims[1] != n {
        return rt("The A and C matrices must have the same number of columns.");
    }
    if dm.dims[1] != bm.dims[1] {
        return rt("The B and D matrices must have the same number of columns.");
    }
    if dm.dims[0] != cm.dims[0] {
        return rt("The C and D matrices must have the same number of rows.");
    }
    let iu = if a.len() == 5 {
        let v = scalar_arg(a, 4, "ss2zp")?;
        if v <= 0.0 {
            return rt("Expected input number 5, IU, to be positive.");
        }
        if v.fract() != 0.0 {
            return rt("Expected input number 5, IU, to be integer-valued.");
        }
        v as usize
    } else if bm.dims[1] > 1 {
        return rt("IU must be specified for systems with more than one input.");
    } else {
        1
    };
    if iu > bm.dims[1] {
        return rt("IU cannot exceed the number of system inputs.");
    }
    let ny = cm.dims[0];
    let b: Vec<f64> = bm.data[(iu - 1) * n..iu * n].to_vec();
    let p = eig_vals(n, &am.data)?;
    let den = charpoly(n, &am.data)?;
    let mut zs: Vec<Vec<C>> = vec![];
    let mut ks = vec![];
    for i in 0..ny {
        let ci: Vec<f64> = (0..n).map(|k| cm.data[k * ny + i]).collect();
        let d = dm.data[(iu - 1) * dm.dims[0] + i];
        if d != 0.0 {
            // Zeros of c(sI-A)^-1 b + d are the eigenvalues of A - b c / d.
            let mut m = am.data.clone();
            for r in 0..n {
                for k in 0..n {
                    m[k * n + r] -= b[r] * ci[k] / d;
                }
            }
            zs.push(eig_vals(n, &m)?);
            ks.push(d);
        } else {
            let mut m = am.data.clone();
            for r in 0..n {
                for k in 0..n {
                    m[k * n + r] -= b[r] * ci[k];
                }
            }
            let q = charpoly(n, &m)?;
            let scale = q.iter().chain(den.iter()).fold(0.0_f64, |s, v| s.max(v.abs()));
            let num: Vec<f64> = q.iter().zip(&den).map(|(x, y)| {
                let v = x - y;
                if v.abs() <= 1e-13 * scale { 0.0 } else { v }
            }).collect();
            let lead = num.iter().take_while(|&&v| v == 0.0).count();
            if lead == num.len() {
                zs.push(vec![]);
                ks.push(0.0);
            } else {
                let coefs: Vec<C> = num[lead..].iter().map(|&v| C::new(v, 0.0)).collect();
                zs.push(roots_matlab(&coefs)?);
                ks.push(num[lead]);
            }
        }
    }
    let nz = zs.iter().map(|z| z.len()).max().unwrap_or(0);
    let mut zall = vec![];
    for z in &zs {
        let mut z = z.clone();
        z.resize(nz, C::new(f64::INFINITY, 0.0));
        zall.extend(z);
    }
    let zre: Vec<f64> = zall.iter().map(|c| c.re).collect();
    let zim: Vec<f64> = zall.iter().map(|c| c.im).collect();
    let zv = if zim.iter().all(|&v| v == 0.0) {
        Value::Num(Matrix::from_dims(vec![nz, ny], zre))
    } else {
        Value::Num(Matrix::complex(vec![nz, ny], zre, zim))
    };
    out(vec![zv, col_value(&p), Value::Num(Matrix::from_dims(vec![ny, 1], ks))])
}

fn sos2ss(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 4 {
        return rt("Too many output arguments.");
    }
    if let Value::Num(m) = &a[0] {
        if m.dims.len() > 2 || m.dims[1] != 6 {
            return rt("SOS matrix must have six columns.");
        }
    }
    let tf = it.call_function("sos2tf", a.to_vec(), 2)?;
    let mut num = tf[0].to_matrix()?.data;
    let mut den = tf[1].to_matrix()?.data;
    // Poles and zeros at the origin common to both are dropped.
    while den.len() > 1 && num.last() == Some(&0.0) && den.last() == Some(&0.0) {
        num.pop();
        den.pop();
    }
    out(canonical(&Matrix::row(num), &den)?)
}

fn ss2sos(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 4 {
        return rt("Not enough input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    // ss2sos(A,B,C,D,iu,dir,scale): the input index may be left out.
    let mut head = a[..4].to_vec();
    let mut rest = &a[4..];
    if let Some(first) = rest.first() {
        if !matches!(first, Value::Num(m) if m.kind == Kind::Char) && first.text().is_err() {
            head.push(first.clone());
            rest = &rest[1..];
        }
    }
    if head.len() == 4 {
        head.push(Value::Num(Matrix::scalar(1.0)));
    }
    let zpk = it.call_function("ss2zp", head, 3)?;
    let mut args = vec![zpk[0].clone(), zpk[1].clone(), zpk[2].clone()];
    args.extend(rest.iter().cloned());
    it.call_function("zp2sos", args, nargout.max(1)).map(Some)
}
