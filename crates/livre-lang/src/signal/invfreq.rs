//! `invfreqz` and `invfreqs`: equation-error least-squares fits of a rational transfer function
//! to frequency response data.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::ar::lstsq;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["invfreqz", "invfreqs"];

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    let z = name == "invfreqz";
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    if a.len() < 4 {
        return rt("Not enough input arguments.");
    }
    let num = |v: &Value, label: &str| -> R<Matrix> {
        match v {
            Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => Ok(m.clone()),
            other => rt(format!("Input {label} in {name} must be double/single, instead it was {}.", other.class_name())),
        }
    };
    let h = num(&a[0], "H")?;
    let w = num(&a[1], "W")?;
    let single = h.kind == Kind::Single || w.kind == Kind::Single;
    let mut i = 2;
    let mut complex = false;
    if let Value::Num(m) = &a[2] {
        if m.kind == Kind::Char {
            let s = a[2].text().unwrap_or_default().to_lowercase();
            if !s.is_empty() && "complex".starts_with(&s) {
                complex = true;
                i = 3;
            }
        }
    }
    if a.len() < i + 2 {
        return rt("Not enough input arguments.");
    }
    let dot = if z { "." } else { "" };
    if h.numel() != w.numel() {
        return rt(format!("The lengths of H and W must coincide{dot}"));
    }
    let scalar = |v: &Value| -> R<f64> { Ok(crate::builtins::m_arg(&[v.clone()], 0, name)?.data.first().copied().unwrap_or(0.0)) };
    let nb = scalar(&a[i])?;
    let na = scalar(&a[i + 1])?;
    if nb.fract() != 0.0 {
        return rt(if z { "Size inputs must be integers.".to_string() } else { "Index in position 1 is invalid. Array indices must be positive integers or logical values.".to_string() });
    }
    if na < 0.0 || nb < -1.0 {
        return rt("Arrays have incompatible sizes for this operation.");
    }
    if na.fract() != 0.0 {
        return rt(if z { "Size inputs must be integers.".to_string() } else { "Index in position 1 is invalid. Array indices must be positive integers or logical values.".to_string() });
    }
    let (nb, na) = ((nb + 1.0) as usize, na as usize); // nb is now the number of numerator coefficients
    let m = w.numel();
    let mut wt = vec![1.0; m];
    if let Some(v) = a.get(i + 2) {
        let wm = crate::builtins::m_arg(&[v.clone()], 0, name)?;
        if wm.numel() > 0 {
            if wm.numel() != m {
                return rt(format!("The lengths of Wt and W must coincide{dot}"));
            }
            wt = wm.data.clone();
        }
    }
    if let Some(v) = a.get(i + 3) {
        let it_count = scalar(v)?;
        if it_count > 0.0 {
            return rt(format!("{name}: the iterative fit (ITER > 0) is not implemented."));
        }
    }
    let hre = &h.data;
    let him: Vec<f64> = h.im.clone().unwrap_or_else(|| vec![0.0; m]);
    let wv = &w.data;
    // basis[k] = value of the k-th unknown's column at each frequency, and the right-hand side
    let basis = |k: usize, f: usize| -> (f64, f64) {
        // returns the monomial for the numerator (k < nb) or denominator coefficient
        let wk = wv[f];
        let (hr, hi) = (hre[f], him[f]);
        let mono = |p: i64| -> (f64, f64) {
            if z {
                let ang = -wk * p as f64;
                (ang.cos(), ang.sin())
            } else {
                // (jw)^p
                let mag = wk.powi(p as i32);
                match p.rem_euclid(4) {
                    0 => (mag, 0.0),
                    1 => (0.0, mag),
                    2 => (-mag, 0.0),
                    _ => (0.0, -mag),
                }
            }
        };
        if k < nb {
            mono(if z { k as i64 } else { (nb - 1 - k) as i64 })
        } else {
            let j = k - nb; // 0-based denominator index: a_{j+1}
            let (mr, mi) = mono(if z { (j + 1) as i64 } else { (na - 1 - j) as i64 });
            (-(hr * mr - hi * mi), -(hr * mi + hi * mr))
        }
    };
    let rhs = |f: usize| -> (f64, f64) {
        if z {
            (hre[f], him[f])
        } else {
            let (hr, hi) = (hre[f], him[f]);
            let p = na as i64;
            let mag = wv[f].powi(p as i32);
            let (mr, mi) = match p.rem_euclid(4) {
                0 => (mag, 0.0),
                1 => (0.0, mag),
                2 => (-mag, 0.0),
                _ => (0.0, -mag),
            };
            (hr * mr - hi * mi, hr * mi + hi * mr)
        }
    };
    let nu = nb + na;
    let sw: Vec<f64> = wt.iter().map(|v| v.sqrt()).collect();
    let (theta_re, theta_im): (Vec<f64>, Option<Vec<f64>>) = if !complex {
        let cols: Vec<Vec<f64>> = (0..nu)
            .map(|k| {
                let mut c = Vec::with_capacity(2 * m);
                for f in 0..m {
                    c.push(basis(k, f).0 * sw[f]);
                }
                for f in 0..m {
                    c.push(basis(k, f).1 * sw[f]);
                }
                c
            })
            .collect();
        let mut y = Vec::with_capacity(2 * m);
        for f in 0..m {
            y.push(rhs(f).0 * sw[f]);
        }
        for f in 0..m {
            y.push(rhs(f).1 * sw[f]);
        }
        (lstsq(&cols, &y), None)
    } else {
        let mut re = vec![0.0; m * nu];
        let mut im = vec![0.0; m * nu];
        for k in 0..nu {
            for f in 0..m {
                let (r, ii) = basis(k, f);
                re[f + m * k] = r * sw[f];
                im[f + m * k] = ii * sw[f];
            }
        }
        let (yr, yi): (Vec<f64>, Vec<f64>) = (0..m).map(|f| (rhs(f).0 * sw[f], rhs(f).1 * sw[f])).unzip();
        let am = Value::Num(Matrix::complex(vec![m, nu], re, im));
        let ym = Value::Num(Matrix::complex(vec![m, 1], yr, yi));
        let sol = it.call_function("mldivide", vec![am, ym], 1)?.remove(0).to_matrix()?;
        (sol.data.clone(), Some(sol.im.clone().unwrap_or_else(|| vec![0.0; nu])))
    };
    let build = |re: Vec<f64>, im: Option<Vec<f64>>| -> Value {
        let n = re.len();
        let mut mx = match im {
            Some(im) => Matrix::complex(vec![1, n], re, im),
            None => Matrix::from_dims(vec![1, n], re),
        };
        if single {
            mx.data.iter_mut().for_each(|e| *e = *e as f32 as f64);
            if let Some(im) = mx.im.as_mut() {
                im.iter_mut().for_each(|e| *e = *e as f32 as f64);
            }
            mx.kind = Kind::Single;
        }
        Value::Num(mx)
    };
    let b = build(theta_re[..nb].to_vec(), theta_im.as_ref().map(|t| t[..nb].to_vec()));
    let mut are = vec![1.0];
    are.extend_from_slice(&theta_re[nb..]);
    let aim = theta_im.as_ref().map(|t| {
        let mut v = vec![0.0];
        v.extend_from_slice(&t[nb..]);
        v
    });
    let av = build(are, aim);
    let mut outs = vec![b];
    if nargout > 1 {
        outs.push(av);
    }
    out(outs)
}
