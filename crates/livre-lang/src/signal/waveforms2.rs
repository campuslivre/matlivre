//! `taylorwin`, `pulstran`, `vco` and `modulate`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["taylorwin", "pulstran", "vco", "modulate", "dpss", "marcumq", "rainflow"];

const NUM_TYPES: &str = "double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64";

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "taylorwin" => taylorwin(a, nargout),
        "pulstran" => pulstran(it, a, nargout),
        "vco" => vco(a, nargout),
        "dpss" => dpss(it, a, nargout),
        "marcumq" => marcumq(it, a, nargout),
        "rainflow" => rainflow(it, a, nargout),
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

fn dpss(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let nv = match &a[0] {
        Value::Num(m) if m.numel() == 1 && m.kind != Kind::Char => m.data[0],
        Value::Num(m) if m.numel() > 1 && m.kind != Kind::Char => return rt("The length of the sequence, N, must be a scalar."),
        _ => return rt("The length of the sequence, N, must be a positive integer."),
    };
    if !(nv >= 1.0) {
        return rt("The length of the sequence, N, must be a positive integer.");
    }
    if nv.fract() != 0.0 {
        return rt("Size inputs must be integers.");
    }
    let n = nv as usize;
    let nw = match &a[1] {
        Value::Num(m) if m.numel() == 1 && m.kind != Kind::Char => m.data[0],
        Value::Num(m) if m.numel() > 1 && m.kind != Kind::Char => return rt("The Time-bandwidth product, NW, must be a scalar."),
        _ => return rt("Time-bandwidth product NW must be a positive number."),
    };
    if nw < 0.0 {
        return rt("Time-bandwidth product NW must be a positive number.");
    }
    if nw >= n as f64 / 2.0 {
        return rt("Time-bandwidth product NW must be less than N/2.");
    }
    // optional K and method strings
    let mut k = (1usize, ((2.0 * nw).round() as usize).max(1));
    let mut k_given = false;
    let mut method = "calc".to_string();
    for v in &a[2..] {
        match v {
            Value::Num(m) if m.kind == Kind::Char => {
                let s = v.text().unwrap_or_default().to_lowercase();
                match s.as_str() {
                    "calc" | "spline" | "linear" => method = s,
                    "trace" => {}
                    _ => return rt("Invalid character vector option.  Valid character vectors are 'calc', 'spline' and 'linear' for method, or 'trace' to show which method is being used."),
                }
            }
            Value::Num(m) if !k_given && (m.numel() == 1 || m.numel() == 2) => {
                k_given = true;
                if m.numel() == 1 {
                    let kv = m.data[0];
                    if kv == 0.0 {
                        k = (1, 0);
                    } else if kv.fract() != 0.0 || kv < 1.0 || kv > n as f64 {
                        return rt("K must be a positive integer in the range 1:N");
                    } else {
                        k = (1, kv as usize);
                    }
                } else {
                    let (k1, k2) = (m.data[0], m.data[1]);
                    if k1.fract() != 0.0 || k2.fract() != 0.0 || k1 < 1.0 || k2 > n as f64 {
                        return rt("K must be a positive integer in the range 1:N");
                    }
                    if k2 < k1 {
                        return rt("Requested array exceeds the maximum possible variable size.");
                    }
                    k = (k1 as usize, k2 as usize);
                }
            }
            Value::Num(m) if !k_given && m.numel() > 2 => return rt("K must be either a scalar or a two element vector."),
            Value::Num(_) if !k_given => return rt("K must be a positive integer in the range 1:N"),
            _ if !k_given => return rt("K must be a positive integer in the range 1:N"),
            _ => {}
        }
    }
    if method != "calc" {
        let g = format!("{nw}");
        return rt(format!("There is no DPSS with time-bandwidth product NW = {g} in database."));
    }
    let ncols = if k.1 >= k.0 { k.1 - k.0 + 1 } else { 0 };
    let w = nw / n as f64;
    let c2 = (2.0 * std::f64::consts::PI * w).cos();
    // tridiagonal matrix whose eigenvectors are the sequences
    let mut tm = vec![0.0; n * n];
    for i in 0..n {
        let d = (n as f64 - 1.0 - 2.0 * i as f64) / 2.0;
        tm[i + n * i] = d * d * c2;
        if i + 1 < n {
            let off = (i + 1) as f64 * (n - 1 - i) as f64 / 2.0;
            tm[i + n * (i + 1)] = off;
            tm[i + 1 + n * i] = off;
        }
    }
    let res = it.call_function("eig", vec![Value::Num(Matrix::from_dims(vec![n, n], tm))], 2)?;
    let vecs = res[0].to_matrix()?;
    let dm = res[1].to_matrix()?;
    let mut order: Vec<usize> = (0..n).collect();
    let eigs: Vec<f64> = (0..n).map(|i| dm.data[i + n * i]).collect();
    order.sort_by(|&x, &y| eigs[y].partial_cmp(&eigs[x]).unwrap_or(std::cmp::Ordering::Equal));
    let mut e = vec![0.0; n * ncols];
    let mut lam = vec![0.0; ncols];
    for (cidx, kk) in (k.0..=k.1).enumerate().take(ncols) {
        let src = order[kk - 1];
        let mut col: Vec<f64> = (0..n).map(|i| vecs.data[i + n * src]).collect();
        let nrm = col.iter().map(|v| v * v).sum::<f64>().sqrt();
        col.iter_mut().for_each(|v| *v /= nrm);
        let sign = if (kk - 1) % 2 == 0 {
            col.iter().sum::<f64>()
        } else {
            col[0]
        };
        if sign < 0.0 {
            col.iter_mut().for_each(|v| *v = -*v);
        }
        // concentration in the band |f| <= W
        let mut q = 0.0;
        for i in 0..n {
            for j in 0..n {
                let d = i as f64 - j as f64;
                let aij = if i == j { 2.0 * w } else { (2.0 * std::f64::consts::PI * w * d).sin() / (std::f64::consts::PI * d) };
                q += col[i] * aij * col[j];
            }
        }
        lam[cidx] = q;
        e[n * cidx..n * (cidx + 1)].copy_from_slice(&col);
    }
    let mut outs = vec![Value::Num(Matrix::from_dims(vec![n, ncols], e))];
    if nargout > 1 {
        outs.push(Value::Num(Matrix::from_dims(vec![ncols, 1], lam)));
    }
    out(outs)
}

fn marcumq(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let mut ms: Vec<Matrix> = vec![];
    for v in a {
        match v {
            Value::Num(m) if m.kind == Kind::Double => ms.push(m.clone()),
            _ => return rt("Input arguments must be 'double'."),
        }
    }
    if ms.iter().any(|m| m.im.is_some()) {
        return rt("Arguments must be real.");
    }
    if ms[0].data.iter().chain(ms[1].data.iter()).any(|v| *v < 0.0) {
        return rt("The first two arguments must be nonnegative.");
    }
    let mdef = Matrix::from_dims(vec![1, 1], vec![1.0]);
    let m3 = ms.get(2).cloned().unwrap_or(mdef);
    if m3.data.iter().any(|v| !(*v >= 1.0) || v.fract() != 0.0) {
        return rt("Third argument must be a positive integer.");
    }
    let all = [&ms[0], &ms[1], &m3];
    let shape = all.iter().find(|m| m.numel() != 1).map(|m| m.dims.clone()).unwrap_or_else(|| vec![1, 1]);
    for m in all {
        if m.numel() != 1 && m.dims != shape {
            return rt("Dimensions of arguments do not match.");
        }
    }
    let n: usize = shape.iter().product();
    let pick = |m: &Matrix, i: usize| if m.numel() == 1 { m.data[0] } else { m.data[i] };
    let mut y = vec![0.0; n];
    for i in 0..n {
        let (aa, bb, mm) = (pick(&ms[0], i), pick(&ms[1], i), pick(&m3, i));
        let aa = if aa.is_nan() { 0.0 } else { aa };
        y[i] = if bb.is_nan() {
            f64::NAN
        } else if bb == 0.0 {
            1.0
        } else if bb.is_infinite() {
            0.0
        } else if aa.is_infinite() {
            1.0
        } else {
            let lam = aa * aa / 2.0;
            let x = bb * bb / 2.0;
            let kmax = (lam + 14.0 * lam.sqrt() + 60.0).ceil() as usize;
            let mut total = 0.0;
            let mut lnfact = 0.0;
            for k in 0..=kmax {
                if k > 0 {
                    lnfact += (k as f64).ln();
                }
                let pk = if lam == 0.0 { if k == 0 { 1.0 } else { 0.0 } } else { (-lam + k as f64 * lam.ln() - lnfact).exp() };
                if pk == 0.0 {
                    continue;
                }
                let q = it
                    .call_function("gammainc", vec![scalar_value(x), scalar_value(mm + k as f64), Value::chars("upper")], 1)?
                    .remove(0)
                    .to_matrix()?
                    .data[0];
                total += pk * q;
            }
            total
        };
    }
    out(vec![Value::Num(Matrix::from_dims(shape, y))])
}

fn rainflow(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 5 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        Value::Num(m) if m.kind == Kind::Char => return rt("Expected X to be one of these types:  single, double"),
        other => return rt(format!("Expected X to be one of these types:  single, double  Instead its type was {}.", other.class_name())),
    };
    if x.im.is_some() {
        return rt("Expected X to be real.");
    }
    if x.dims.len() > 2 || x.numel() == 0 || (x.dims[0] != 1 && x.dims[1] != 1) {
        return rt("Expected X to be a vector.");
    }
    if x.data.iter().any(|v| !v.is_finite()) {
        return rt("Expected X to be finite.");
    }
    let n = x.numel();
    if n < 3 {
        return rt("Input time series must have at least 3 samples.");
    }
    let single = x.kind == Kind::Single;
    let mut ext = false;
    let mut times: Vec<f64> = (1..=n).map(|i| i as f64).collect();
    for (k, v) in a.iter().enumerate().skip(1) {
        match v {
            Value::Num(m) if m.kind == Kind::Char => {
                if v.text().unwrap_or_default().to_lowercase() == "ext" {
                    ext = true;
                } else {
                    return rt("Invalid option.");
                }
            }
            Value::Num(m) if k == 1 => {
                if m.numel() == 1 {
                    let fs = m.data[0];
                    if !fs.is_finite() || fs == 0.0 {
                        return rt("Expected T to be finite.");
                    }
                    if fs < 0.0 {
                        return rt("Expected T to be strictly increasing.");
                    }
                    times = (0..n).map(|i| i as f64 / fs).collect();
                } else {
                    if m.numel() != n {
                        return rt(format!("Expected T to be an array with number of elements equal to {n}."));
                    }
                    if m.data.windows(2).any(|w| !(w[1] > w[0])) {
                        return rt("Expected T to be strictly increasing.");
                    }
                    times = m.data.clone();
                }
            }
            _ => return rt("Invalid option."),
        }
    }
    // reversals
    let mut rev: Vec<usize> = vec![];
    if ext {
        rev = (0..n).collect();
    } else {
        rev.push(0);
        // compress plateaus
        let mut runs: Vec<(usize, f64)> = vec![]; // first index of each distinct run
        for i in 0..n {
            if runs.last().map_or(true, |r| r.1 != x.data[i]) {
                runs.push((i, x.data[i]));
            }
        }
        for k in 1..runs.len().saturating_sub(1) {
            let (p, c, q) = (runs[k - 1].1, runs[k].1, runs[k + 1].1);
            if (c > p && c > q) || (c < p && c < q) {
                rev.push(runs[k].0);
            }
        }
        rev.push(n - 1);
    }
    // ASTM E1049 rainflow counting
    let mut rows: Vec<[f64; 5]> = vec![];
    let mut stack: Vec<usize> = vec![];
    let val = |i: usize| x.data[i];
    let mut emit = |count: f64, i: usize, j: usize, rows: &mut Vec<[f64; 5]>| {
        if val(i) != val(j) {
            rows.push([count, (val(i) - val(j)).abs(), (val(i) + val(j)) / 2.0, times[i], times[j]]);
        }
    };
    for &p in &rev {
        stack.push(p);
        while stack.len() >= 3 {
            let l = stack.len();
            let xr = (val(stack[l - 1]) - val(stack[l - 2])).abs();
            let yr = (val(stack[l - 2]) - val(stack[l - 3])).abs();
            if xr < yr {
                break;
            }
            if l == 3 {
                emit(0.5, stack[0], stack[1], &mut rows);
                stack.remove(0);
            } else {
                emit(1.0, stack[l - 3], stack[l - 2], &mut rows);
                stack.remove(l - 2);
                stack.remove(l - 3);
            }
        }
    }
    for w in stack.windows(2) {
        emit(0.5, w[0], w[1], &mut rows);
    }
    let nr = rows.len();
    let mut c = vec![0.0; nr * 5];
    for (i, r) in rows.iter().enumerate() {
        for j in 0..5 {
            c[i + nr * j] = r[j];
        }
    }
    let cast = |m: Matrix| -> Value {
        let mut m = m;
        if single {
            m.data.iter_mut().for_each(|e| *e = *e as f32 as f64);
            m.kind = Kind::Single;
        }
        Value::Num(m)
    };
    let mut outs = vec![cast(Matrix::from_dims(vec![nr, 5], c))];
    if nargout > 1 {
        let (rm, rmr, rmm) = if nr == 0 {
            (Matrix::from_dims(vec![1, 1], vec![0.0]), vec![0.0, 1.0], vec![0.0, 1.0])
        } else {
            let mut rr = vec![];
            let mut mm = vec![];
            for r in &rows {
                for _ in 0..((2.0 * r[0]).round() as usize) {
                    rr.push(r[1]);
                    mm.push(r[2]);
                }
            }
            let edges = |it: &mut Interpreter, v: Vec<f64>| -> R<Vec<f64>> {
                let res = it.call_function("histcounts", vec![real_col(v)], 2)?;
                Ok(res[1].to_matrix()?.data.clone())
            };
            let er = edges(it, rr)?;
            let em = edges(it, mm)?;
            let bin = |e: &[f64], v: f64| -> Option<usize> {
                let nb = e.len() - 1;
                (0..nb).find(|&k| v >= e[k] && (v < e[k + 1] || (k == nb - 1 && v <= e[k + 1])))
            };
            let (nrb, nmb) = (er.len() - 1, em.len() - 1);
            let mut h = vec![0.0; nrb * nmb];
            for r in &rows {
                if let (Some(i), Some(j)) = (bin(&er, r[1]), bin(&em, r[2])) {
                    h[i + nrb * j] += r[0];
                }
            }
            (Matrix::from_dims(vec![nrb, nmb], h), er, em)
        };
        outs.push(cast(rm));
        if nargout > 2 {
            outs.push(cast(Matrix::from_dims(vec![rmr.len(), 1], rmr)));
        }
        if nargout > 3 {
            outs.push(cast(Matrix::from_dims(vec![rmm.len(), 1], rmm)));
        }
        if nargout > 4 {
            outs.push(cast(Matrix::from_dims(vec![rev.len(), 1], rev.iter().map(|&i| (i + 1) as f64).collect())));
        }
    }
    out(outs)
}
