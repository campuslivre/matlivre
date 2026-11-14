//! Filter property tests and small designs: `firtype`, `isminphase`, `ismaxphase`, `islinphase`,
//! `isallpass`, `filtord`, `filternorm`, `impzlength`, `gaussdesign`, `rcosdesign`.

use crate::builtins::Ret;
use crate::cplx::C;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::filtering::automatic_length_tol;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &[
    "firtype", "isminphase", "ismaxphase", "islinphase", "isallpass", "filtord", "filternorm", "impzlength", "gaussdesign", "rcosdesign",
];

const PI: f64 = std::f64::consts::PI;

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    match name {
        "firtype" => firtype(a),
        "gaussdesign" => gaussdesign(a),
        "rcosdesign" => rcosdesign(a),
        _ => with_ba(it, name, a),
    }
}

fn logical(v: bool) -> Value {
    let mut m = Matrix::scalar(if v { 1.0 } else { 0.0 });
    m.kind = Kind::Logical;
    Value::Num(m)
}

fn strip_trailing(mut v: Vec<f64>) -> Vec<f64> {
    while v.len() > 1 && v.last() == Some(&0.0) {
        v.pop();
    }
    v
}

fn strip_leading(v: &[f64]) -> &[f64] {
    let k = v.iter().take_while(|&&x| x == 0.0).count();
    if k == v.len() { &v[..1.min(v.len())] } else { &v[k..] }
}

fn all_zero(v: &[f64]) -> bool {
    v.iter().all(|&x| x == 0.0)
}

fn firtype(a: &[Value]) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 1 {
        return rt("Too many input arguments.");
    }
    let m = match &a[0] {
        Value::Num(m) if m.kind != Kind::Char && m.im.is_none() => m.clone(),
        _ => return rt("The filter must have real coefficients."),
    };
    if m.dims.len() > 2 || (m.dims[0] > 1 && m.dims[1] > 1) {
        return rt("The FIR filter coefficients must be stored in a vector.");
    }
    let b = strip_trailing(strip_leading(&m.data).to_vec());
    let n = b.len();
    let tol = 1e-12 * b.iter().fold(0.0_f64, |s, v| s.max(v.abs()));
    let sym = (0..n).all(|i| (b[i] - b[n - 1 - i]).abs() <= tol);
    let anti = (0..n).all(|i| (b[i] + b[n - 1 - i]).abs() <= tol);
    let t = if sym {
        if n % 2 == 1 { 1.0 } else { 2.0 }
    } else if anti {
        if n % 2 == 1 { 3.0 } else { 4.0 }
    } else {
        return rt("The filter must have linear phase.");
    };
    out(vec![scalar_value(t)])
}

/// Numerator and denominator, with the tolerance argument if any, for the tests that take
/// `(b, a, tol)` or an SOS matrix.
fn with_ba(it: &mut Interpreter, name: &str, a: &[Value]) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if name == "filternorm" && a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if name == "impzlength" && a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if a.len() > 4 || (name != "filternorm" && name != "impzlength" && a.len() > 3) {
        return rt("Too many input arguments.");
    }
    let m = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        other => {
            return match name {
                "isminphase" => rt("Undefined function 'eq' for input arguments of type 'cell'."),
                "ismaxphase" | "isallpass" => rt("Undefined function 'ne' for input arguments of type 'cell'."),
                "islinphase" => out(vec![logical(true)]),
                "filtord" => rt(format!("Expected input to be one of these types:  double, single  Instead its type was {}.", other.class_name())),
                _ => rt("Not enough input arguments."),
            };
        }
    };
    let vector = m.dims.len() == 2 && (m.dims[0] == 1 || m.dims[1] == 1);
    let (b, den, rest): (Vec<f64>, Vec<f64>, &[Value]) = if vector {
        match a.get(1) {
            Some(Value::Num(x)) if x.kind != Kind::Char => (m.data.clone(), x.data.clone(), &a[2..]),
            _ => (m.data.clone(), vec![1.0], &a[1..]),
        }
    } else if m.dims.len() == 2 && m.dims[1] == 6 {
        let tf = it.call_function("sos2tf", vec![a[0].clone()], 2)?;
        (tf[0].to_matrix()?.data, tf[1].to_matrix()?.data, &a[1..])
    } else {
        if name == "filternorm" {
            (m.data.clone(), vec![1.0], &a[1..])
        } else {
        return rt("When first input is a matrix, it must have exactly 6 columns to be a valid SOS matrix.");
        }
    };
    if m.im.is_some() {
        return rt("The filter must have real coefficients.");
    }
    let tol_arg = |def: f64| -> f64 {
        rest.first().and_then(|v| if let Value::Num(x) = v { x.data.first().copied() } else { None }).unwrap_or(def)
    };
    let bt = strip_trailing(b.clone());
    let at = strip_trailing(den.clone());
    let to_c = |v: &[f64]| v.iter().map(|&x| C::new(x, 0.0)).collect::<Vec<_>>();
    match name {
        "filtord" => out(vec![scalar_value((bt.len().max(at.len()) - 1) as f64)]),
        "impzlength" => {
            let tol = if a.len() >= 3 { tol_arg(5e-5) } else { 5e-5 };
            if tol <= 0.0 {
                return rt("Expected TOL to be positive.");
            }
            if all_zero(&b) {
                return out(vec![scalar_value(1.0)]);
            }
            let n = if at.len() == 1 { bt.len() } else { automatic_length_tol(&bt, &at, tol) };
            out(vec![scalar_value(n as f64)])
        }
        "islinphase" => {
            let tol = tol_arg(1e-12);
            if at.len() > 1 {
                return out(vec![logical(false)]);
            }
            let core = strip_trailing(strip_leading(&b).to_vec());
            if all_zero(&core) {
                return out(vec![logical(true)]);
            }
            let n = core.len();
            let scale = core.iter().fold(0.0_f64, |s, v| s.max(v.abs()));
            let sym = (0..n).all(|i| (core[i] - core[n - 1 - i]).abs() <= tol * scale.max(1e-300) + 1e-300);
            let anti = (0..n).all(|i| (core[i] + core[n - 1 - i]).abs() <= tol * scale.max(1e-300) + 1e-300);
            out(vec![logical(sym || anti)])
        }
        "isallpass" => {
            let tol = tol_arg(1e-12);
            let ok = !all_zero(&b) && b.len() == den.len() && {
                let (bs, ds) = (strip_leading(&b).to_vec(), strip_leading(&den).to_vec());
                bs.len() == ds.len() && {
                    let c = bs[0] / ds[ds.len() - 1];
                    let scale = bs.iter().fold(0.0_f64, |s, v| s.max(v.abs()));
                    c.is_finite() && (0..bs.len()).all(|i| (bs[i] - c * ds[ds.len() - 1 - i]).abs() <= tol * scale.max(1e-300) + 1e-300)
                }
            };
            out(vec![logical(ok)])
        }
        "isminphase" | "ismaxphase" => {
            let tol = tol_arg(1e-11);
            if all_zero(&b) {
                return rt("Polynomial cannot contain only zeros.");
            }
            let zeros = roots_matlab(&to_c(&b))?;
            if name == "isminphase" {
                let poles = roots_matlab(&to_c(&den))?;
                let ok = zeros.iter().chain(&poles).all(|r| r.abs() <= 1.0 + tol);
                out(vec![logical(ok)])
            } else {
                let zb = roots_matlab(&to_c(&bt))?;
                let extra = den.len() > bt.len();
                out(vec![logical(!extra && zb.iter().all(|r| r.abs() >= 1.0 - tol))])
            }
        }
        _ => {
            // filternorm(b, a, pnorm, tol)
            let p = match rest.first() {
                None => 2.0,
                Some(Value::Num(x)) if x.kind != Kind::Char => x.data.first().copied().unwrap_or(2.0),
                Some(_) => return rt("Expected input number 3, PNORM, to be one of these types:  double"),
            };
            if !(p == 2.0 || p == f64::INFINITY) {
                return rt("Invalid input. Expected values for pnorm are Inf and 2.");
            }
            let tol = rest.get(1).and_then(|v| if let Value::Num(x) = v { x.data.first().copied() } else { None }).unwrap_or(1e-8);
            if den.is_empty() || den[0] == 0.0 {
                return rt("The filter must be stable.");
            }
            if at.len() > 1 {
                let poles = roots_matlab(&to_c(&at))?;
                if poles.iter().any(|r| r.abs() >= 1.0) {
                    return rt("The filter must be stable.");
                }
            }
            if p == 2.0 {
                let len = if at.len() == 1 { bt.len() } else { automatic_length_tol(&bt, &at, tol) };
                let h = impulse(&b, &den, len);
                out(vec![scalar_value(h.iter().map(|v| v * v).sum::<f64>().sqrt())])
            } else {
                out(vec![scalar_value(inf_norm(&b, &den))])
            }
        }
    }
}

fn impulse(b: &[f64], a: &[f64], n: usize) -> Vec<f64> {
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut v = if i < b.len() { b[i] } else { 0.0 };
        for k in 1..a.len().min(i + 1) {
            v -= a[k] * y[i - k];
        }
        y[i] = v / a[0];
    }
    y
}

fn mag(b: &[f64], a: &[f64], w: f64) -> f64 {
    let ev = |p: &[f64]| {
        let (mut re, mut im) = (0.0, 0.0);
        for (k, &c) in p.iter().enumerate() {
            re += c * (w * k as f64).cos();
            im -= c * (w * k as f64).sin();
        }
        re.hypot(im)
    };
    ev(b) / ev(a)
}

fn inf_norm(b: &[f64], a: &[f64]) -> f64 {
    let n = 8192;
    let vals: Vec<f64> = (0..=n).map(|i| mag(b, a, PI * i as f64 / n as f64)).collect();
    let mut best = vals.iter().cloned().fold(0.0_f64, f64::max);
    for i in 1..n {
        if vals[i] >= vals[i - 1] && vals[i] >= vals[i + 1] {
            let (mut lo, mut hi) = (PI * (i - 1) as f64 / n as f64, PI * (i + 1) as f64 / n as f64);
            let g = (5f64.sqrt() - 1.0) / 2.0;
            for _ in 0..80 {
                let (x1, x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
                if mag(b, a, x1) > mag(b, a, x2) { hi = x2 } else { lo = x1 }
            }
            best = best.max(mag(b, a, (lo + hi) / 2.0));
        }
    }
    best
}

fn positive_int(m: &Matrix, label: &str, idx: usize, integer: bool) -> R<f64> {
    if m.numel() != 1 {
        return rt(format!("Expected input number {idx}, {label}, to be a scalar."));
    }
    let v = m.data[0];
    if v <= 0.0 {
        return rt(format!("Expected input number {idx}, {label}, to be positive."));
    }
    if integer && v.fract() != 0.0 {
        return rt(format!("Expected input number {idx}, {label}, to be integer-valued."));
    }
    Ok(v)
}

fn gaussdesign(a: &[Value]) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    let bt = positive_int(&crate::builtins::m_arg(a, 0, "gaussdesign")?, "BT", 1, false)?;
    let span = if a.len() > 1 { positive_int(&crate::builtins::m_arg(a, 1, "gaussdesign")?, "SPAN", 2, true)? } else { 3.0 };
    let sps = if a.len() > 2 { positive_int(&crate::builtins::m_arg(a, 2, "gaussdesign")?, "SPS", 3, true)? } else { 2.0 };
    if (span * sps) % 2.0 != 0.0 {
        return rt("Product of SPS and SPAN must be even.");
    }
    let n = (span * sps) as i64;
    let sigma2 = 2f64.ln() / (2.0 * PI * bt).powi(2);
    let h: Vec<f64> = (-n / 2..=n / 2)
        .map(|k| {
            let t = k as f64 / sps;
            (-t * t / (2.0 * sigma2)).exp()
        })
        .collect();
    let s: f64 = h.iter().sum();
    out(vec![real_row(h.iter().map(|v| v / s).collect())])
}

fn rcosdesign(a: &[Value]) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    let bm = crate::builtins::m_arg(a, 0, "rcosdesign")?;
    if bm.numel() != 1 {
        return rt("Expected input number 1, BETA, to be a scalar.");
    }
    let beta = bm.data[0];
    if beta < 0.0 {
        return rt("Expected input number 1, BETA, to be nonnegative.");
    }
    if beta > 1.0 {
        return rt("Expected input number 1, BETA, to be a scalar with value <= 1.");
    }
    let span = positive_int(&crate::builtins::m_arg(a, 1, "rcosdesign")?, "SPAN", 2, false)?;
    let sps = positive_int(&crate::builtins::m_arg(a, 2, "rcosdesign")?, "SPS", 3, true)?;
    let mut sqrt_shape = true;
    if a.len() == 4 {
        let s = a[3].text().unwrap_or_default().to_lowercase();
        match s.as_str() {
            "sqrt" => sqrt_shape = true,
            "normal" => sqrt_shape = false,
            _ => {
                let shown = a[3].text().unwrap_or_default();
                return rt(format!("Expected input number 4, SHAPE, to match one of these values:  'sqrt', 'normal'  The input, '{shown}', did not match any of the valid values."));
            }
        }
    }
    let n = (span * sps).round() as i64;
    let h: Vec<f64> = (-n / 2..=n / 2)
        .map(|k| {
            let t = k as f64 / sps;
            if sqrt_shape {
                if t == 0.0 {
                    1.0 - beta + 4.0 * beta / PI
                } else if beta > 0.0 && (t.abs() - 1.0 / (4.0 * beta)).abs() < 1e-12 {
                    beta / 2f64.sqrt() * ((1.0 + 2.0 / PI) * (PI / (4.0 * beta)).sin() + (1.0 - 2.0 / PI) * (PI / (4.0 * beta)).cos())
                } else {
                    ((PI * t * (1.0 - beta)).sin() + 4.0 * beta * t * (PI * t * (1.0 + beta)).cos()) / (PI * t * (1.0 - (4.0 * beta * t).powi(2)))
                }
            } else {
                let sinc = if t == 0.0 { 1.0 } else { (PI * t).sin() / (PI * t) };
                let den = 1.0 - (2.0 * beta * t).powi(2);
                if beta > 0.0 && den.abs() < 1e-12 {
                    PI / 4.0 * sinc
                } else {
                    sinc * (PI * beta * t).cos() / den
                }
            }
        })
        .collect();
    let e: f64 = h.iter().map(|v| v * v).sum::<f64>().sqrt();
    out(vec![real_row(h.iter().map(|v| v / e).collect())])
}
