//! Parametric spectrum estimates: `pyulear`, `pburg`, `pcov`, `pmcov`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["pyulear", "pburg", "pcov", "pmcov"];

const PI: f64 = std::f64::consts::PI;

const OPTIONS: &str = "Expected input to match one of these values:  'half', 'onesided', 'whole', 'twosided', 'adapt', 'unity', 'eigen', 'corr', 'ev', 'centered', 'power', 'psd', 'ms', 'reassigned', 'confidencelevel', 'mean', 'maxhold', 'minhold', 'mimo'  The input, '";

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let ar = match name {
        "pyulear" => "aryule",
        "pburg" => "arburg",
        "pcov" => "arcov",
        _ => "armcov",
    };
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        other => {
            return if matches!(name, "pcov" | "pmcov") {
                rt(format!("Input X in {name} must be double/single, instead it was {}.", other.class_name()))
            } else if matches!(other, Value::Num(_)) {
                rt("Expected X to be one of these types:  single, double")
            } else {
                rt(format!("Expected X to be one of these types:  single, double  Instead its type was {}.", other.class_name()))
            };
        }
    };
    if x.numel() == 0 {
        return rt("Expected X to be nonempty.");
    }
    let pm = crate::builtins::m_arg(a, 1, name)?;
    if pm.numel() != 1 {
        return rt("Expected ORDER to be a scalar.");
    }
    let p = pm.data[0];
    if p < 0.0 {
        return rt("Expected ORDER to be nonnegative.");
    }
    if p.fract() != 0.0 {
        return rt("Expected ORDER to be integer-valued.");
    }
    let cols: Vec<Vec<f64>> = if x.dims[0] == 1 {
        vec![x.data.clone()]
    } else {
        let r = x.dims[0];
        (0..x.numel() / r).map(|c| x.data[c * r..(c + 1) * r].to_vec()).collect()
    };
    if name == "pyulear" && (cols[0].len() as f64) < p {
        return rt("When X is a vector, it must have length greater than or equal to the model order.");
    }
    // Frequency specification.
    let mut nfft: usize = 256;
    let mut freqs: Option<Vec<f64>> = None;
    let mut fs: Option<f64> = None;
    let mut sides: Option<String> = None;
    for (i, v) in a.iter().enumerate().skip(2) {
        match v {
            Value::Num(m) if m.kind == Kind::Char => {
                let s = m.to_string_lossy().to_lowercase();
                match s.as_str() {
                    "onesided" | "half" => sides = Some("one".into()),
                    "twosided" | "whole" => sides = Some("two".into()),
                    "centered" => sides = Some("centered".into()),
                    _ => return rt(format!("{OPTIONS}{}', did not match any of the valid values.", m.to_string_lossy())),
                }
            }
            Value::Num(m) => {
                if i == 2 {
                    if m.numel() > 1 {
                        freqs = Some(m.data.clone());
                    } else if m.numel() == 1 {
                        if m.data[0] < 1.0 {
                            return rt("Array indices must be positive integers or logical values.");
                        }
                        nfft = m.data[0] as usize;
                    }
                } else if m.numel() == 1 {
                    if m.data[0] <= 0.0 {
                        return rt("Expected fs to be positive.");
                    }
                    fs = Some(m.data[0]);
                }
            }
            _ => {}
        }
    }
    let complex = x.im.is_some();
    if complex {
        return rt("Complex input is not supported.");
    }
    let sides = sides.unwrap_or_else(|| "one".into());
    let fsv = fs.unwrap_or(2.0 * PI);
    // Models.
    let mut models = vec![];
    for c in &cols {
        let res = it.call_function(ar, vec![real_col(c.clone()), scalar_value(p)], 2)?;
        models.push((res[0].to_matrix()?.data, res[1].to_matrix()?.data[0]));
    }
    // Frequency grid in rad/sample (omega) and in the user's units.
    let (omegas, units, dbl): (Vec<f64>, Vec<f64>, Vec<bool>) = if let Some(f) = &freqs {
        let om: Vec<f64> = f.iter().map(|&v| 2.0 * PI * v / fsv).collect();
        (om, f.clone(), vec![false; f.len()])
    } else {
        let ks: Vec<i64> = match sides.as_str() {
            "two" => (0..nfft as i64).collect(),
            "centered" => {
                let lo = if nfft % 2 == 0 { -(nfft as i64 / 2) + 1 } else { -((nfft as i64 - 1) / 2) };
                (lo..lo + nfft as i64).collect()
            }
            _ => {
                let top = if nfft % 2 == 0 { nfft / 2 } else { (nfft - 1) / 2 };
                (0..=top as i64).collect()
            }
        };
        let om: Vec<f64> = ks.iter().map(|&k| 2.0 * PI * k as f64 / nfft as f64).collect();
        let un: Vec<f64> = ks.iter().map(|&k| fsv * k as f64 / nfft as f64).collect();
        let last = ks.len() - 1;
        let dbl: Vec<bool> = ks
            .iter()
            .enumerate()
            .map(|(i, &k)| sides == "one" && k != 0 && !(nfft % 2 == 0 && i == last))
            .collect();
        (om, un, dbl)
    };
    let mut data = vec![];
    for (poly, e) in &models {
        for (i, &w) in omegas.iter().enumerate() {
            let (mut re, mut im) = (0.0, 0.0);
            for (k, &c) in poly.iter().enumerate() {
                re += c * (w * k as f64).cos();
                im -= c * (w * k as f64).sin();
            }
            let mut v = e / (fsv * (re * re + im * im));
            if dbl[i] {
                v *= 2.0;
            }
            data.push(v);
        }
    }
    let n = omegas.len();
    let pxx = if freqs.is_some() && x.dims[0] == 1 {
        Value::Num(Matrix::from_dims(vec![1, n], data))
    } else {
        Value::Num(Matrix::from_dims(vec![n, models.len()], data))
    };
    let mut outv = vec![pxx];
    if nargout > 1 {
        outv.push(real_col(units));
    }
    out(outv)
}
