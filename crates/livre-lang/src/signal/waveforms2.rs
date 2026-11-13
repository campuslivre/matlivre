//! `taylorwin`, `pulstran`, `vco` and `modulate`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["taylorwin", "pulstran", "vco", "modulate"];

const NUM_TYPES: &str = "double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64";

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "taylorwin" => taylorwin(a, nargout),
        "pulstran" => pulstran(it, a, nargout),
        "vco" => vco(a, nargout),
        _ => modulate(it, a, nargout),
    }
}

fn real_num(v: &Value, pos: usize, label: &str) -> R<Matrix> {
    match v {
        Value::Num(m) if m.kind != Kind::Char && m.kind != Kind::Logical => Ok(m.clone()),
        _ => rt(format!("Expected input number {pos}, {label}, to be one of these types:  {NUM_TYPES}")),
    }
}

fn taylorwin(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let nm = real_num(&a[0], 1, "n")?;
    if nm.numel() != 1 {
        return rt("Expected input number 1, n, to be a scalar.");
    }
    let n = nm.data[0].round();
    if n < 0.0 {
        return rt("Expected input number 1, n, to be nonnegative.");
    }
    let nbar = match a.get(1) {
        Some(v) if !matches!(v, Value::Num(m) if m.numel() == 0) => {
            let m = real_num(v, 2, "nbar")?;
            if m.numel() != 1 || m.data[0] < 1.0 || m.data[0].fract() != 0.0 {
                return rt("The number of nearly constant-level sidelobes NBAR must be a positive integer greater than 0.");
            }
            m.data[0] as usize
        }
        _ => 4,
    };
    let sll = match a.get(2) {
        Some(v) if !matches!(v, Value::Num(m) if m.numel() == 0) => {
            let m = real_num(v, 3, "sll")?;
            if m.numel() != 1 || m.data[0] > 0.0 {
                return rt("The sidelobe level SLL must be a negative number.");
            }
            m.data[0]
        }
        _ => -30.0,
    };
    let n = n as usize;
    let b = 10f64.powf(-sll / 20.0);
    let acosh = (b + (b * b - 1.0).max(0.0).sqrt()).ln();
    let aa = acosh / std::f64::consts::PI;
    let s2 = (nbar * nbar) as f64 / (aa * aa + (nbar as f64 - 0.5).powi(2));
    let nb = nbar as i64;
    let fm: Vec<f64> = (1..nb)
        .map(|m| {
            let mf = m as f64;
            let num: f64 = (1..nb).map(|k| 1.0 - mf * mf / (s2 * (aa * aa + (k as f64 - 0.5).powi(2)))).product();
            let den: f64 = (1..nb).filter(|&k| k != m).map(|k| 1.0 - mf * mf / ((k * k) as f64)).product();
            (if (m + 1) % 2 == 0 { 1.0 } else { -1.0 }) * num / (2.0 * den)
        })
        .collect();
    let w: Vec<f64> = (0..n)
        .map(|i| {
            let x = i as f64 - n as f64 / 2.0 + 0.5;
            1.0 + 2.0 * fm.iter().enumerate().map(|(k, f)| f * (2.0 * std::f64::consts::PI * (k + 1) as f64 * x / n as f64).cos()).sum::<f64>()
        })
        .collect();
    out(vec![Value::Num(Matrix::from_dims(vec![n, 1], w))])
}

fn pulstran(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let t = match &a[0] {
        Value::Num(m) if m.kind == Kind::Double => m.clone(),
        other => return rt(format!("Expected input number 1, t, to be one of these types:  double  Instead its type was {}.", other.class_name())),
    };
    let d = real_num(&a[1], 2, "D")?;
    let (delays, gains): (Vec<f64>, Vec<f64>) = if d.numel() == 0 {
        return rt(format!("Expected input number 2, D, to be of size Mx2, but it is of size {}x{}.", d.dims[0], d.dims[1]));
    } else if d.dims.len() == 2 && (d.dims[0] == 1 || d.dims[1] == 1) {
        (d.data.clone(), vec![1.0; d.numel()])
    } else if d.dims.len() == 2 && d.dims[1] == 2 {
        let m = d.dims[0];
        (d.data[..m].to_vec(), d.data[m..].to_vec())
    } else {
        return rt("Delay, D, must either be a vector, or an Mx2 matrix.");
    };
    let n = t.numel();
    let mut y = vec![0.0; n];
    let proto = match &a[2] {
        Value::Num(p) if p.kind != Kind::Char => Some(p.clone()),
        _ => None,
    };
    for (k, &dl) in delays.iter().enumerate() {
        let tt_data: Vec<f64> = t.data.iter().map(|v| v - dl).collect();
        let tt = Matrix::from_dims(t.dims.clone(), tt_data.clone());
        let r = if let Some(p) = &proto {
            let fs = match a.get(3) {
                Some(v) => crate::builtins::m_arg(&[v.clone()], 0, "pulstran")?.data.first().copied().unwrap_or(1.0),
                None => 1.0,
            };
            let method = match a.get(4) {
                Some(v) => v.clone(),
                None => Value::chars("linear"),
            };
            let tp = real_row((0..=p.numel()).map(|i| i as f64 / fs).collect());
            let mut pd = p.data.clone();
            pd.push(0.0);
            let pr = Value::Num(Matrix::from_dims(vec![1, pd.len()], pd));
            let res = it.call_function("interp1", vec![tp, pr, Value::Num(tt), method], 1)?.remove(0).to_matrix()?;
            let spline = method_is_spline(a.get(4));
            let span = p.numel() as f64 / fs;
            res.data
                .iter()
                .zip(tt_data.iter())
                .map(|(v, x)| if v.is_nan() || (!spline && (*x < 0.0 || *x > span)) { 0.0 } else { *v })
                .collect::<Vec<f64>>()
        } else {
            let mut args = vec![Value::Num(tt)];
            args.extend_from_slice(&a[3..]);
            let res = match &a[2] {
                Value::Func(f) => it.call_value(f, args, 1)?,
                other => {
                    let name = other.text().unwrap_or_default();
                    match it.call_function(&name, args, 1) {
                        Ok(v) => v,
                        Err(e) if e.to_string().contains(&format!("Unrecognized function or variable '{name}'")) => {
                            return rt(format!("Undefined function '{name}' for input arguments of type 'double'."));
                        }
                        Err(e) => return Err(e),
                    }
                }
            };
            match res.into_iter().next() {
                Some(v) => v.to_matrix()?.data,
                None => return rt("Function returned no value."),
            }
        };
        for i in 0..n {
            y[i] += gains[k] * r[i];
        }
    }
    out(vec![Value::Num(Matrix::from_dims(t.dims.clone(), y))])
}

fn method_is_spline(v: Option<&Value>) -> bool {
    v.and_then(|v| v.text().ok()).map_or(false, |s| s.to_lowercase() == "spline")
}

fn cast_like(m: Matrix, single: bool) -> Value {
    let mut m = m;
    if single {
        m.data.iter_mut().for_each(|e| *e = *e as f32 as f64);
        m.kind = Kind::Single;
    }
    Value::Num(m)
}

fn vco(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if m.im.is_none() => m.clone(),
        _ => return rt("X outside of range [-1,1]"),
    };
    let single = x.kind == Kind::Single;
    let fs = match a.get(2) {
        None => 1.0,
        Some(v) => match v {
            Value::Num(m) if m.numel() == 1 => m.data[0],
            _ => return rt("Matrix dimensions must agree."),
        },
    };
    let (fmin, fmax) = match a.get(1) {
        None => {
            let fc = fs / 4.0;
            (0.0, 2.0 * fc)
        }
        Some(Value::Num(m)) if m.numel() == 1 => (0.0, 2.0 * m.data[0]),
        Some(Value::Num(m)) if m.numel() == 2 => (m.data[0], m.data[1]),
        Some(Value::Num(m)) if m.numel() == 0 => return rt("Expected input number 2, FC, to be a scalar."),
        Some(_) => return rt("vco: unsupported carrier frequency vector."),
    };
    let limit = match a.get(1) {
        Some(Value::Num(m)) if m.numel() == 1 => m.data[0],
        Some(_) => fmax.max(fmin),
        None => fs / 4.0,
    };
    if limit >= fs / 2.0 {
        return rt("The carrier frequency must be less than half the sampling frequency.");
    }
    if x.data.iter().any(|v| v.abs() > 1.0) || x.kind == Kind::Char {
        return rt("X outside of range [-1,1]");
    }
    if x.numel() == 0 || x.dims.len() > 2 {
        return rt("Operands to the logical AND (&&) and OR (||) operators must be convertible to logical scalar values. Use the ANY or ALL functions to reduce operands to logical scalar values.");
    }
    let fc = (fmin + fmax) / 2.0;
    let kf = (fmax - fmin) / 2.0;
    let (rows, cols) = (x.dims[0], x.dims[1]);
    let (len, nch) = if rows == 1 { (cols, 1) } else { (rows, cols) };
    let mut y = vec![0.0; x.numel()];
    for c in 0..nch {
        let mut acc = 0.0;
        for k in 0..len {
            acc += x.data[c * len + k];
            let ph = 2.0 * std::f64::consts::PI * (fc * k as f64 / fs + kf * acc / fs);
            y[c * len + k] = ph.cos();
        }
    }
    out(vec![cast_like(Matrix::from_dims(x.dims.clone(), y), single)])
}

fn modulate(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 5 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt("Expected input number 1, X, to be one of these types:  double, single"),
    };
    if x.im.is_some() {
        return rt("modulate: complex input is not supported.");
    }
    let single = x.kind == Kind::Single;
    let fc = match &a[1] {
        Value::Num(m) if m.numel() == 1 => m.data[0],
        _ => return rt("Expected input number 2, FC, to be a scalar."),
    };
    let fs = match &a[2] {
        Value::Num(m) if m.numel() == 1 => m.data[0],
        _ => return rt("Expected input number 3, FS, to be a scalar."),
    };
    if fc >= fs / 2.0 {
        return rt("The carrier frequency must be less than half the sampling frequency.");
    }
    let method = match a.get(3) {
        Some(v) => v.text().unwrap_or_default().to_lowercase(),
        None => "am".to_string(),
    };
    let known = ["am", "amdsb-sc", "amdsb-tc", "amssb", "fm", "pm", "pwm", "ppm", "qam"];
    if !known.contains(&method.as_str()) {
        return rt("The method provided should be one of 'am', 'amdsb-tc', 'amdsd-tc','fm', 'pm', 'pwm',and 'qam'");
    }
    let opt = a.get(4);
    let (rows, cols) = (x.dims[0], x.dims[1]);
    let (len, nch) = if rows == 1 { (cols, 1) } else { (rows, cols) };
    let tvec: Vec<f64> = (0..len).map(|k| k as f64 / fs).collect();
    let tpi = 2.0 * std::f64::consts::PI;
    let maxabs = x.data.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let scalar_opt = |label_pos: usize| -> R<Option<f64>> {
        match opt {
            None => Ok(None),
            Some(Value::Num(m)) if m.numel() == 1 => Ok(Some(m.data[0])),
            Some(Value::Num(m)) if m.numel() == 0 => Ok(None),
            Some(_) => rt(format!("Expected input number {label_pos}, OPT, to be a scalar.")),
        }
    };
    let mut y = vec![0.0; x.numel()];
    match method.as_str() {
        "am" | "amdsb-sc" => {
            for c in 0..nch {
                for k in 0..len {
                    y[c * len + k] = x.data[c * len + k] * (tpi * fc * tvec[k]).cos();
                }
            }
        }
        "amdsb-tc" => {
            let off = scalar_opt(5)?.unwrap_or_else(|| x.data.iter().cloned().fold(f64::INFINITY, f64::min));
            for c in 0..nch {
                for k in 0..len {
                    y[c * len + k] = (x.data[c * len + k] - off) * (tpi * fc * tvec[k]).cos();
                }
            }
        }
        "amssb" => {
            for c in 0..nch {
                let col = Value::Num(Matrix::from_dims(vec![len, 1], x.data[c * len..(c + 1) * len].to_vec()));
                let h = it.call_function("hilbert", vec![col], 1)?.remove(0).to_matrix()?;
                let hi = h.im.clone().unwrap_or_else(|| vec![0.0; len]);
                for k in 0..len {
                    y[c * len + k] = x.data[c * len + k] * (tpi * fc * tvec[k]).cos() + hi[k] * (tpi * fc * tvec[k]).sin();
                }
            }
        }
        "fm" => {
            let kf = scalar_opt(5)?.unwrap_or(fc / fs * tpi / maxabs);
            for c in 0..nch {
                let mut acc = 0.0;
                for k in 0..len {
                    acc += x.data[c * len + k];
                    y[c * len + k] = (tpi * fc * tvec[k] + kf * acc).cos();
                }
            }
        }
        "pm" => {
            let kp = scalar_opt(5)?.unwrap_or(std::f64::consts::PI / maxabs);
            for c in 0..nch {
                for k in 0..len {
                    y[c * len + k] = (tpi * fc * tvec[k] + kp * x.data[c * len + k]).cos();
                }
            }
        }
        "qam" => {
            let q = match opt {
                None => return rt("For 'qam', a 5th input parameter is required."),
                Some(Value::Num(m)) => m.clone(),
                Some(_) => return rt("For 'qam', a 5th input parameter is required."),
            };
            if q.dims != x.dims {
                return rt("For 'qam', input signals must be the same size");
            }
            for c in 0..nch {
                for k in 0..len {
                    y[c * len + k] = x.data[c * len + k] * (tpi * fc * tvec[k]).cos() + q.data[c * len + k] * (tpi * fc * tvec[k]).sin();
                }
            }
        }
        _ => {
            if x.data.iter().any(|v| *v < 0.0 || *v > 1.0) {
                return rt("'X' is out of range [0,1]");
            }
            return rt(format!("modulate: the '{method}' method is not implemented."));
        }
    }
    let mut outs = vec![cast_like(Matrix::from_dims(x.dims.clone(), y), single)];
    if nargout > 1 {
        let tm = Matrix::from_dims(if rows == 1 { vec![1, len] } else { vec![len, 1] }, tvec);
        outs.push(cast_like(tm, single));
    }
    out(outs)
}
