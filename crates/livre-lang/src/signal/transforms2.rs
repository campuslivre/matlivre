//! Fast and chirp transforms and data matrices: `fwht`, `ifwht`, `digitrevorder`, `corrmtx`, `czt`.

use crate::builtins::Ret;
use crate::cplx::C;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["fwht", "ifwht", "digitrevorder", "corrmtx", "czt"];

const PI: f64 = std::f64::consts::PI;

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "fwht" | "ifwht" => walsh(name, a, nargout),
        "digitrevorder" => digitrev(a, nargout),
        "corrmtx" => corrmtx(a, nargout),
        _ => czt(a, nargout),
    }
}

fn cvec(m: &Matrix) -> Vec<C> {
    (0..m.numel()).map(|i| C::new(m.data[i], m.im_at(i))).collect()
}

fn mat_c(dims: Vec<usize>, v: &[C]) -> Value {
    let re: Vec<f64> = v.iter().map(|c| c.re).collect();
    if v.iter().all(|c| c.im == 0.0) {
        Value::Num(Matrix::from_dims(dims, re))
    } else {
        Value::Num(Matrix::complex(dims, re, v.iter().map(|c| c.im).collect()))
    }
}

fn columns_c(m: &Matrix) -> Vec<Vec<C>> {
    let all = cvec(m);
    if m.dims[0] == 1 {
        return vec![all];
    }
    let r = m.dims[0];
    (0..all.len() / r).map(|c| all[c * r..(c + 1) * r].to_vec()).collect()
}

fn walsh(name: &str, a: &[Value], nargout: usize) -> Ret {
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
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt("Expected input number 1, x, to be one of these types:  double, single"),
    };
    let mut order = "sequency".to_string();
    if let Some(o) = a.get(2) {
        order = o.text().unwrap_or_default().to_lowercase();
        if !matches!(order.as_str(), "sequency" | "hadamard" | "dyadic") {
            return rt(format!(
                "Expected input to match one of these values:\n\n'sequency', 'hadamard', 'dyadic'\n\nThe input, '{}', did not match any of the valid values.",
                o.text().unwrap_or_default()
            ));
        }
    }
    if x.numel() == 0 {
        return out(vec![Value::Num(Matrix::from_dims(vec![0, 0], vec![]))]);
    }
    let rows = if x.dims[0] == 1 { x.dims[1] } else { x.dims[0] };
    let n = match a.get(1) {
        Some(Value::Num(m)) if m.numel() > 0 => {
            if m.numel() != 1 {
                return rt("Expected input number 2, N, to be a scalar.");
            }
            let v = m.data[0];
            if v <= 0.0 {
                return rt("Expected input number 2, N, to be positive.");
            }
            if v.fract() != 0.0 {
                return rt("Expected input number 2, N, to be integer-valued.");
            }
            let v = v as usize;
            if !v.is_power_of_two() {
                return rt("Transform length must be a non-negative integer that is a power of 2");
            }
            v
        }
        _ => rows.next_power_of_two(),
    };
    // Row `k` of the transform matrix in the requested ordering.
    let bits = n.trailing_zeros();
    let natural = |r: usize, c: usize| -> f64 { if (r & c).count_ones() % 2 == 0 { 1.0 } else { -1.0 } };
    let rev = |v: usize| -> usize {
        let mut o = 0;
        for b in 0..bits {
            if v >> b & 1 == 1 {
                o |= 1 << (bits - 1 - b);
            }
        }
        o
    };
    let row_index: Vec<usize> = match order.as_str() {
        "hadamard" => (0..n).collect(),
        "dyadic" => (0..n).map(rev).collect(),
        _ => {
            // Sequency: sorted by the number of sign changes of the natural rows.
            let mut idx: Vec<usize> = (0..n).collect();
            let changes = |r: usize| (1..n).filter(|&c| natural(r, c) != natural(r, c - 1)).count();
            idx.sort_by_key(|&r| changes(r));
            idx
        }
    };
    let scale = if name == "fwht" { 1.0 / n as f64 } else { 1.0 };
    let cols = columns_c(&x);
    let mut res = vec![];
    for col in &cols {
        let mut padded = col.clone();
        padded.resize(n, C::new(0.0, 0.0));
        for &r in &row_index {
            let mut s = C::new(0.0, 0.0);
            for (c, v) in padded.iter().enumerate() {
                let w = natural(r, c);
                s = s.add(C::new(v.re * w, v.im * w));
            }
            res.push(C::new(s.re * scale, s.im * scale));
        }
    }
    let dims = if x.dims[0] == 1 { vec![1, n] } else { vec![n, cols.len()] };
    out(vec![mat_c(dims, &res)])
}

fn digitrev(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt("Input X must be numeric or logical."),
    };
    let r = match &a[1] {
        Value::Num(m) if m.numel() == 1 && m.data[0].fract() == 0.0 && m.data[0] >= 2.0 && m.data[0] <= 36.0 && m.im.is_none() => m.data[0] as usize,
        _ => return rt("The specified radix must be a scalar, real, integer between 2 and 36 inclusive."),
    };
    if x.numel() == 0 {
        return out(vec![Value::Num(Matrix::from_dims(vec![0, 0], vec![]))]);
    }
    let n = if x.dims[0] == 1 { x.dims[1] } else { x.dims[0] };
    let mut digits = 0;
    let mut p = 1usize;
    while p < n {
        p *= r;
        digits += 1;
    }
    if p != n {
        return rt(format!("The length of the input X must be an integer power of {r}."));
    }
    let perm: Vec<usize> = (0..n)
        .map(|i| {
            let mut v = i;
            let mut o = 0;
            for _ in 0..digits {
                o = o * r + v % r;
                v /= r;
            }
            o
        })
        .collect();
    let rows = if x.dims[0] == 1 { 1 } else { n };
    let ncols = x.numel() / n;
    let mut re = vec![0.0; x.numel()];
    let mut im = vec![0.0; x.numel()];
    for c in 0..ncols {
        for (i, &pi) in perm.iter().enumerate() {
            let src = if rows == 1 { pi } else { c * n + pi };
            let dst = if rows == 1 { i } else { c * n + i };
            re[dst] = x.data[src];
            im[dst] = x.im_at(src);
        }
    }
    let mut y = if x.im.is_some() { Matrix::complex(x.dims.clone(), re, im) } else { Matrix::from_dims(x.dims.clone(), re) };
    y.kind = x.kind;
    let mut outv = vec![Value::Num(y)];
    if nargout > 1 {
        outv.push(real_col(perm.iter().map(|&p| (p + 1) as f64).collect()));
    }
    out(outv)
}

fn corrmtx(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let mm = crate::builtins::m_arg(a, 1, "corrmtx")?;
    let xm = match &a[0] {
        Value::Num(m) if m.numel() > 0 => m.clone(),
        Value::Num(m) if m.numel() == 0 => return rt("Input signal must be a vector."),
        _ => return rt("Input signal must be a vector."),
    };
    if mm.numel() != 1 {
        return rt("Expected M to be a scalar.");
    }
    let m = mm.data[0];
    if m < 0.0 {
        return rt("Expected M to be nonnegative.");
    }
    if m.fract() != 0.0 {
        return rt("Expected M to be integer-valued.");
    }
    let n = xm.numel();
    if xm.dims.len() > 2 || (xm.dims[0] > 1 && xm.dims[1] > 1) {
        return rt("Input signal must be a vector.");
    }
    if m >= n as f64 {
        return rt(format!("Expected M to be a scalar with value < {n}."));
    }
    let m = m as usize;
    let method = match a.get(2) {
        None => "autocorrelation".to_string(),
        Some(v) => {
            let s = v.text().unwrap_or_default().to_lowercase();
            let opts = ["autocorrelation", "prewindowed", "postwindowed", "covariance", "modified"];
            let hits: Vec<&&str> = opts.iter().filter(|o| !s.is_empty() && o.starts_with(&s)).collect();
            if hits.len() != 1 {
                return rt("Ambiguous or invalid method specified.");
            }
            hits[0].to_string()
        }
    };
    let x = cvec(&xm);
    let at = |i: i64| -> C { if i >= 0 && (i as usize) < n { x[i as usize] } else { C::new(0.0, 0.0) } };
    // Rows of the full Toeplitz data matrix: row r holds x(r), x(r-1), ..., x(r-m).
    let row = |r: i64| -> Vec<C> { (0..=m as i64).map(|j| at(r - j)).collect() };
    let rows: Vec<Vec<C>> = match method.as_str() {
        "autocorrelation" => (0..(n + m) as i64).map(row).collect(),
        "prewindowed" => (0..n as i64).map(row).collect(),
        "postwindowed" => (m as i64..(n + m) as i64).map(row).collect(),
        "covariance" => (m as i64..n as i64).map(row).collect(),
        _ => {
            let cov: Vec<Vec<C>> = (m as i64..n as i64).map(row).collect();
            let mut all = cov;
            for i in 0..(n - m) as i64 {
                all.push((0..=m as i64).map(|j| at(i + j).conj()).collect());
            }
            all
        }
    };
    let nr = rows.len();
    let sc = 1.0 / (if method == "autocorrelation" { n } else { nr } as f64).sqrt();
    let mut data = vec![C::new(0.0, 0.0); nr * (m + 1)];
    for (i, r) in rows.iter().enumerate() {
        for (j, v) in r.iter().enumerate() {
            data[j * nr + i] = C::new(v.re * sc, v.im * sc);
        }
    }
    let mut outv = vec![mat_c(vec![nr, m + 1], &data)];
    if nargout > 1 {
        let mut rm = vec![C::new(0.0, 0.0); (m + 1) * (m + 1)];
        for p in 0..=m {
            for q in 0..=m {
                let mut s = C::new(0.0, 0.0);
                for i in 0..nr {
                    s = s.add(data[p * nr + i].conj().mul(data[q * nr + i]));
                }
                rm[q * (m + 1) + p] = s;
            }
        }
        outv.push(mat_c(vec![m + 1, m + 1], &rm));
    }
    out(outv)
}

fn czt(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt("Expected input number 1, X, to be one of these types:  double, single"),
    };
    if x.numel() == 0 {
        return rt("Index in position 1 is invalid. Array indices must be positive integers or logical values.");
    }
    let scalar = |i: usize, label: &str| -> R<Option<C>> {
        match a.get(i) {
            None => Ok(None),
            Some(Value::Num(m)) if m.numel() == 1 => Ok(Some(C::new(m.data[0], m.im_at(0)))),
            Some(Value::Num(m)) if m.numel() == 0 => Ok(None),
            Some(_) => rt(format!("Expected input number {}, {label}, to be a scalar.", i + 1)),
        }
    };
    let n_in = if x.dims[0] == 1 { x.dims[1] } else { x.dims[0] };
    let m_raw = match scalar(1, "M")? {
        Some(c) => c.re,
        None => n_in as f64,
    };
    let w = match scalar(2, "W")? {
        Some(c) => c,
        None => C::new((2.0 * PI / m_raw).cos(), -(2.0 * PI / m_raw).sin()),
    };
    let av = scalar(3, "A")?.unwrap_or(C::new(1.0, 0.0));
    let m = if m_raw >= 1.0 { m_raw.floor() as usize } else { 0 };
    let degenerate = (w.re == 0.0 && w.im == 0.0) || (av.re == 0.0 && av.im == 0.0);
    let cols = columns_c(&x);
    let mut res = vec![];
    for col in &cols {
        for k in 0..m {
            let mut s = C::new(0.0, 0.0);
            for (i, v) in col.iter().enumerate() {
                // x(n) * a^-n * w^(n k)
                let term = if degenerate { C::new(f64::NAN, f64::NAN) } else { cpow(w, (i * k) as f64).div(cpow(av, i as f64)) };
                s = s.add(v.mul(term));
            }
            res.push(s);
        }
    }
    let dims = if x.dims[0] == 1 { vec![1, m] } else { vec![m, cols.len()] };
    // The result is complex in MATLAB even when the imaginary parts vanish; keep zeros explicit.
    let re: Vec<f64> = res.iter().map(|c| c.re).collect();
    let im: Vec<f64> = res.iter().map(|c| c.im).collect();
    if im.iter().all(|&v| v == 0.0) {
        out(vec![Value::Num(Matrix::from_dims(dims, re))])
    } else {
        out(vec![Value::Num(Matrix::complex(dims, re, im))])
    }
}

fn cpow(b: C, e: f64) -> C {
    if e == 0.0 {
        return C::new(1.0, 0.0);
    }
    let r = b.abs();
    if r == 0.0 {
        return C::new(0.0, 0.0);
    }
    let th = b.im.atan2(b.re);
    let (mag, ang) = (r.powf(e), th * e);
    C::new(mag * ang.cos(), mag * ang.sin())
}
