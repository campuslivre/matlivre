//! Cepstra: `cceps`, `rceps`, `icceps`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["cceps", "rceps", "icceps"];

const PI: f64 = std::f64::consts::PI;

fn input(a: &[Value], label: &str, all_types: bool) -> R<Matrix> {
    match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) || (all_types && m.kind != Kind::Char && m.kind != Kind::Logical) => Ok(m.clone()),
        _ if all_types => rt(format!(
            "Expected input number 1, {label}, to be one of these types:  double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64"
        )),
        _ => rt(format!("Expected input number 1, {label}, to be one of these types:  single, double")),
    }
}

fn spectrum(re: &[f64], im: &[f64]) -> Value {
    if im.iter().all(|&v| v == 0.0) {
        real_col(re.to_vec())
    } else {
        Value::Num(Matrix::complex(vec![re.len(), 1], re.to_vec(), im.to_vec()))
    }
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let (maxin, maxout) = match name {
        "cceps" => (2, 3),
        "rceps" => (1, 2),
        _ => (2, 1),
    };
    if a.len() > maxin {
        return rt("Too many input arguments.");
    }
    if nargout > maxout {
        return rt("Too many output arguments.");
    }
    let label = match name {
        "rceps" => "x",
        "icceps" => "XHAT",
        _ => "X",
    };
    let x = input(a, label, name != "cceps")?;
    if x.numel() == 0 {
        return rt(format!("Expected input number 1, {label}, to be nonempty."));
    }
    if x.im.is_some() {
        return rt(format!("Expected input number 1, {label}, to be real."));
    }
    let vector = x.dims[0] == 1 || x.dims[1] == 1;
    if name != "rceps" && !vector {
        return rt(format!("Expected input number 1, {label}, to be a vector."));
    }
    let single = x.kind == Kind::Single;
    let cols: Vec<Vec<f64>> = if x.dims[0] == 1 {
        vec![x.data.clone()]
    } else {
        let r = x.dims[0];
        (0..x.numel() / r).map(|c| x.data[c * r..(c + 1) * r].to_vec()).collect()
    };
    let mut cols = cols;
    if name == "cceps" {
        if let Some(Value::Num(m)) = a.get(1) {
            if m.numel() == 1 && m.data[0] >= 1.0 {
                let len = m.data[0] as usize;
                for c in cols.iter_mut() {
                    c.resize(len, 0.0);
                }
            }
        }
    }
    let n = cols[0].len();
    let call1 = |it: &mut Interpreter, f: &str, args: Vec<Value>| -> R<Matrix> { it.call_function(f, args, 1)?.remove(0).to_matrix() };
    let mut results: Vec<Vec<f64>> = vec![];
    let mut aux: Vec<Vec<f64>> = vec![];
    let mut nd_out = 0.0;
    for col in &cols {
        match name {
            "cceps" => {
                let big = call1(it, "fft", vec![real_col(col.clone())])?;
                let logabs: Vec<f64> = (0..n).map(|i| big.data[i].hypot(big.im_at(i)).ln()).collect();
                let ang: Vec<f64> = (0..n).map(|i| big.im_at(i).atan2(big.data[i])).collect();
                let ph = call1(it, "unwrap", vec![real_col(ang)])?.data;
                let h = (n + 1) / 2;
                let nd = if h < n { (ph[h] / PI).round() } else { 0.0 };
                let ph: Vec<f64> = (0..n).map(|k| ph[k] - PI * nd * k as f64 / h.max(1) as f64).collect();
                results.push(call1(it, "ifft", vec![spectrum(&logabs, &ph)])?.data);
                nd_out = nd;
            }
            "rceps" => {
                let big = call1(it, "fft", vec![real_col(col.clone())])?;
                let abs: Vec<f64> = (0..n).map(|i| big.data[i].hypot(big.im_at(i))).collect();
                if abs.iter().any(|&v| v == 0.0) {
                    return rt("The Fourier transform of X contains zeros. Therefore, the real cepstrum of X does not exist.");
                }
                let logabs: Vec<f64> = abs.iter().map(|v| v.ln()).collect();
                let y = call1(it, "ifft", vec![real_col(logabs)])?.data;
                let wy: Vec<f64> = (0..n)
                    .map(|i| {
                        let w = if i == 0 {
                            1.0
                        } else if i < (n + 1) / 2 {
                            2.0
                        } else if i == (n + 1) / 2 && n % 2 == 0 {
                            1.0
                        } else {
                            0.0
                        };
                        w * y[i]
                    })
                    .collect();
                let fy = call1(it, "fft", vec![real_col(wy)])?;
                let re: Vec<f64> = (0..n).map(|i| fy.data[i].exp() * fy.im_at(i).cos()).collect();
                let im: Vec<f64> = (0..n).map(|i| fy.data[i].exp() * fy.im_at(i).sin()).collect();
                aux.push(call1(it, "ifft", vec![spectrum(&re, &im)])?.data);
                results.push(y);
            }
            _ => {
                let nd = match a.get(1) {
                    Some(Value::Num(m)) if m.numel() == 1 && m.data[0].fract() == 0.0 => m.data[0],
                    Some(Value::Num(m)) if m.numel() == 1 => return rt("Expected input number 2, ND, to be integer-valued."),
                    Some(_) => return rt("Expected input number 2, ND, to be a scalar."),
                    None => 0.0,
                };
                let h = ((n + 1) / 2).max(1);
                let fx = call1(it, "fft", vec![real_col(col.clone())])?;
                let re: Vec<f64> = (0..n).map(|k| fx.data[k].exp() * (fx.im_at(k) + PI * nd * k as f64 / h as f64).cos()).collect();
                let im: Vec<f64> = (0..n).map(|k| fx.data[k].exp() * (fx.im_at(k) + PI * nd * k as f64 / h as f64).sin()).collect();
                results.push(call1(it, "ifft", vec![spectrum(&re, &im)])?.data);
            }
        }
    }
    let shape = |res: Vec<Vec<f64>>| -> Value {
        let mut v = if x.dims[0] == 1 {
            Matrix::from_dims(vec![1, n], res[0].clone())
        } else {
            Matrix::from_dims(vec![n, res.len()], res.iter().flatten().copied().collect())
        };
        if single {
            v.kind = Kind::Single;
        }
        Value::Num(v)
    };
    let mut outv = vec![shape(results)];
    if name == "cceps" && nargout > 1 {
        outv.push(scalar_value(nd_out));
    }
    if name == "rceps" && nargout > 1 {
        outv.push(shape(aux));
    }
    out(outv)
}
