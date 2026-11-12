//! Sequence comparison and change detection: `seqperiod`, `cusum`, `dtw`, `edr`, `alignsignals`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["seqperiod", "cusum", "dtw", "edr", "alignsignals"];

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "seqperiod" => seqperiod(a, nargout),
        "cusum" => cusum(a, nargout),
        "dtw" => dtw_edr(name, a, nargout),
        "edr" => dtw_edr(name, a, nargout),
        _ => alignsignals(it, a, nargout),
    }
}

fn columns(m: &Matrix) -> Vec<Vec<f64>> {
    if m.dims[0] == 1 {
        return vec![m.data.clone()];
    }
    let r = m.dims[0];
    (0..m.numel() / r).map(|c| m.data[c * r..(c + 1) * r].to_vec()).collect()
}

fn seqperiod(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() || a.len() > 2 {
        return rt("Incorrect number of input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let m = match &a[0] {
        Value::Num(m) if m.kind == Kind::Double && m.im.is_none() => m.clone(),
        _ => return rt("Input sequence must be real, double-precision numbers."),
    };
    let tol = match a.get(1) {
        Some(Value::Num(t)) if t.numel() == 1 => t.data[0],
        _ => 0.0,
    };
    if m.numel() == 0 {
        let e = Value::Num(Matrix::from_dims(vec![1, 0], vec![]));
        return out(vec![e.clone(), e]);
    }
    let cols = columns(&m);
    let (mut ps, mut ns) = (vec![], vec![]);
    for c in &cols {
        let n = c.len();
        let p = (1..=n).find(|&p| (0..n - p).all(|i| (c[i] - c[i + p]).abs() <= tol)).unwrap_or(n);
        ps.push(p as f64);
        ns.push(n as f64 / p as f64);
    }
    let mk = |v: Vec<f64>| if v.len() == 1 { scalar_value(v[0]) } else { real_row(v) };
    out(vec![mk(ps), mk(ns)])
}

fn cusum(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 4 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt("Expected input number 1, X, to be one of these types:  single, double"),
    };
    if x.numel() == 0 || x.dims.len() > 2 || (x.dims[0] > 1 && x.dims[1] > 1) {
        return rt("Expected input number 1, X, to be a vector.");
    }
    let mut all = false;
    let mut nums: Vec<Option<f64>> = vec![];
    for (i, v) in a.iter().enumerate().skip(1) {
        match v {
            Value::Num(m) if m.kind == Kind::Char => {
                if m.to_string_lossy().to_lowercase() == "all" {
                    all = true;
                } else {
                    return rt(format!("Unrecognized option '{}'.", m.to_string_lossy()));
                }
            }
            Value::Num(m) if m.numel() == 0 && i == 1 => nums.push(None),
            Value::Num(m) => {
                if m.numel() != 1 {
                    let name = match i { 1 => "CLIMIT", 2 => "MSHIFT", 3 => "TMEAN", _ => "TDEV" };
                    return rt(format!("Expected input number {}, {name}, to be a scalar.", i + 1));
                }
                nums.push(Some(m.data[0]));
            }
            _ => nums.push(None),
        }
    }
    let climit = nums.first().copied().flatten().unwrap_or(5.0);
    let mshift = nums.get(1).copied().flatten().unwrap_or(1.0);
    if climit <= 0.0 {
        return rt("Expected input number 2, CLIMIT, to be positive.");
    }
    if mshift <= 0.0 {
        return rt("Expected input number 3, MSHIFT, to be positive.");
    }
    let n = x.numel();
    let head = &x.data[..n.min(25)];
    let hm = head.iter().sum::<f64>() / head.len() as f64;
    let hs = if head.len() > 1 { (head.iter().map(|v| (v - hm) * (v - hm)).sum::<f64>() / (head.len() - 1) as f64).sqrt() } else { 0.0 };
    let tmean = nums.get(2).copied().flatten().unwrap_or(hm);
    let tdev = nums.get(3).copied().flatten().unwrap_or(hs);
    let k = mshift / 2.0 * tdev;
    let h = climit * tdev;
    let mut up = vec![0.0; n];
    let mut lo = vec![0.0; n];
    let (mut iu, mut il) = (vec![], vec![]);
    for i in 1..n {
        let d = x.data[i] - tmean;
        up[i] = (up[i - 1] + d - k).max(0.0);
        lo[i] = (lo[i - 1] + d + k).min(0.0);
        if up[i] > h {
            iu.push((i + 1) as f64);
            if all {
                up[i] = 0.0;
            }
        }
        if -lo[i] > h {
            il.push((i + 1) as f64);
            if all {
                lo[i] = 0.0;
            }
        }
    }
    if !all {
        iu.truncate(1);
        il.truncate(1);
    }
    let empty_dims = if n == 1 { vec![0, 0] } else if x.dims[0] == 1 { vec![1, 0] } else { vec![0, 1] };
    let idx = |v: Vec<f64>| -> Value {
        if v.is_empty() { Value::Num(Matrix::from_dims(empty_dims.clone(), vec![])) } else if v.len() == 1 { scalar_value(v[0]) } else { real_row(v) }
    };
    let shape = |v: Vec<f64>| -> Value {
        let mut m = if x.dims[0] == 1 { Matrix::from_dims(vec![1, v.len()], v) } else { Matrix::from_dims(vec![v.len(), 1], v) };
        if x.kind == Kind::Single {
            m.kind = Kind::Single;
        }
        Value::Num(crate::signal::cast_single_values(m))
    };
    let mut outv = vec![idx(iu), idx(il), shape(up), shape(lo)];
    outv.truncate(nargout.max(1));
    out(outv)
}

fn dtw_edr(name: &str, a: &[Value], nargout: usize) -> Ret {
    let min_args = if name == "edr" { 3 } else { 2 };
    if a.len() < min_args {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let get = |v: &Value, label: &str| -> R<Matrix> {
        match v {
            Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => Ok(m.clone()),
            _ => rt(format!("Expected input number {}, {label}, to be one of these types:  single, double", if label == "X" { 1 } else { 2 })),
        }
    };
    let x = get(&a[0], "X")?;
    let y = get(&a[1], "Y")?;
    let mut metric = "euclidean".to_string();
    let mut maxsamp: Option<f64> = None;
    let mut tol = 0.0;
    let mut pos = 2;
    if name == "edr" {
        tol = match &a[2] {
            Value::Num(m) if m.numel() == 1 => m.data[0],
            _ => return rt("Expected input number 3, TOL, to be a scalar."),
        };
        if tol <= 0.0 {
            return rt("Expected input number 3, TOL, to be positive.");
        }
        pos = 3;
    }
    for v in &a[pos..] {
        match v {
            Value::Num(m) if m.kind == Kind::Char => {
                metric = m.to_string_lossy().to_lowercase();
                if !matches!(metric.as_str(), "absolute" | "euclidean" | "squared" | "symmkl") {
                    return rt(format!(
                        "Expected input to match one of these values:\n\n'absolute', 'euclidean', 'squared', 'symmkl'\n\nThe input, '{}', did not match any of the valid values.",
                        m.to_string_lossy()
                    ));
                }
            }
            Value::Num(m) if m.numel() == 1 => {
                if m.data[0] <= 0.0 {
                    return rt(format!("Expected input number {}, MAXSAMP, to be positive.", pos + 1));
                }
                maxsamp = Some(m.data[0]);
            }
            _ => {}
        }
    }
    // Samples: vectors are scalar sequences; matrices have one sample per column.
    let is_vec = |m: &Matrix| m.dims[0] == 1 || m.dims[1] == 1;
    let samples = |m: &Matrix| -> Vec<Vec<(f64, f64)>> {
        if is_vec(m) {
            (0..m.numel()).map(|i| vec![(m.data[i], m.im_at(i))]).collect()
        } else {
            let r = m.dims[0];
            (0..m.numel() / r).map(|c| (0..r).map(|i| (m.data[c * r + i], m.im_at(c * r + i))).collect()).collect()
        }
    };
    if !is_vec(&x) || !is_vec(&y) {
        if x.dims[0] != y.dims[0] {
            return rt("The number of rows between X and Y must be equal when X and Y are matrices");
        }
    }
    if y.numel() == 0 || x.numel() == 0 {
        return rt("The number of rows between X and Y must be equal when X and Y are matrices");
    }
    let (xs, ys) = (samples(&x), samples(&y));
    let (n, m) = (xs.len(), ys.len());
    let cost = |i: usize, j: usize| -> f64 {
        let (p, q) = (&xs[i], &ys[j]);
        if name == "edr" {
            let ok = p.iter().zip(q).all(|(u, v)| ((u.0 - v.0).hypot(u.1 - v.1)) <= tol);
            return if ok { 0.0 } else { 1.0 };
        }
        match metric.as_str() {
            "absolute" => p.iter().zip(q).map(|(u, v)| (u.0 - v.0).hypot(u.1 - v.1)).sum(),
            "squared" => p.iter().zip(q).map(|(u, v)| (u.0 - v.0).powi(2) + (u.1 - v.1).powi(2)).sum(),
            "symmkl" => p.iter().zip(q).map(|(u, v)| (u.0 - v.0) * (u.0.ln() - v.0.ln())).sum(),
            _ => p.iter().zip(q).map(|(u, v)| (u.0 - v.0).powi(2) + (u.1 - v.1).powi(2)).sum::<f64>().sqrt(),
        }
    };
    let window = maxsamp.map(|w| w.max((n as f64 - m as f64).abs()));
    let inf = f64::INFINITY;
    let mut d = vec![vec![inf; m + 1]; n + 1];
    d[0][0] = 0.0;
    if name == "edr" {
        for i in 1..=n {
            d[i][0] = i as f64;
        }
        for j in 1..=m {
            d[0][j] = j as f64;
        }
    }
    for i in 1..=n {
        for j in 1..=m {
            if let Some(w) = window {
                if ((i as f64) - (j as f64)).abs() > w {
                    continue;
                }
            }
            let c = cost(i - 1, j - 1);
            d[i][j] = if name == "edr" {
                (d[i - 1][j - 1] + c).min(d[i - 1][j] + 1.0).min(d[i][j - 1] + 1.0)
            } else {
                c + d[i - 1][j - 1].min(d[i - 1][j]).min(d[i][j - 1])
            };
        }
    }
    let mut dist = Matrix::scalar(d[n][m]);
    if x.kind == Kind::Single || y.kind == Kind::Single {
        dist = crate::signal::cast_single_values(dist);
        dist.kind = Kind::Single;
    }
    let mut outv = vec![Value::Num(dist)];
    if nargout > 1 {
        let (mut i, mut j) = (n, m);
        let (mut pi, mut pj) = (vec![i], vec![j]);
        while i > 1 || j > 1 {
            let diag = if i > 1 && j > 1 { d[i - 1][j - 1] } else { inf };
            let up = if i > 1 { d[i - 1][j] } else { inf };
            let left = if j > 1 { d[i][j - 1] } else { inf };
            if diag <= up && diag <= left {
                i -= 1;
                j -= 1;
            } else if up <= left {
                i -= 1;
            } else {
                j -= 1;
            }
            pi.push(i);
            pj.push(j);
        }
        pi.reverse();
        pj.reverse();
        let col_in = x.dims[1] == 1 && x.dims[0] > 1 && y.dims[1] == 1 && y.dims[0] > 1;
        let mk = |v: Vec<usize>| -> Value {
            let d: Vec<f64> = v.iter().map(|&t| t as f64).collect();
            if col_in { real_row(d) } else { real_col(d) }
        };
        outv.push(mk(pi));
        outv.push(mk(pj));
    }
    out(outv)
}

fn alignsignals(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let types = "double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64";
    let mut ms = vec![];
    for (v, l) in a.iter().take(2).zip(["X", "Y"]) {
        match v {
            Value::Num(m) if m.kind != Kind::Char && m.kind != Kind::Logical => ms.push(m.clone()),
            _ => return rt(format!("Expected {l} to be one of these types:  {types}")),
        }
    }
    for (m, l) in ms.iter().zip(["X", "Y"]) {
        if m.numel() == 0 || m.dims.len() > 2 || (m.dims[0] > 1 && m.dims[1] > 1) {
            return rt(format!("Expected {l} to be a vector."));
        }
    }
    let d = it.call_function("finddelay", a.to_vec(), 1)?.remove(0).to_matrix()?.data[0];
    let (lx, ly) = (ms[0].numel(), ms[1].numel());
    let mut xv = ms[0].data.clone();
    let mut yv = ms[1].data.clone();
    if d > 0.0 {
        let mut p = vec![0.0; d as usize];
        p.extend(xv);
        xv = p;
    } else if d < 0.0 {
        let mut p = vec![0.0; (-d) as usize];
        p.extend(yv);
        yv = p;
    }
    let _ = (lx, ly);
    let shape = |v: Vec<f64>, m: &Matrix, kind: Kind| -> Value {
        let mut r = if m.dims[0] == 1 { Matrix::from_dims(vec![1, v.len()], v) } else { Matrix::from_dims(vec![v.len(), 1], v) };
        r.kind = kind;
        Value::Num(r)
    };
    let mut outv = vec![shape(xv, &ms[0], ms[0].kind), shape(yv, &ms[1], ms[1].kind), scalar_value(d)];
    outv.truncate(nargout.max(1));
    out(outv)
}
