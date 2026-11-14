//! Rational models fitted to impulse responses: `prony`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["prony", "lpc", "statelevels", "midcross", "risetime", "falltime"];

fn order_arg(a: &[Value], i: usize, label: &str) -> R<usize> {
    let m = match &a[i] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt(format!("Expected input number {}, {label}, to be numeric.", i + 1)),
    };
    if m.numel() != 1 {
        return rt(format!("Expected input number {}, {label}, to be a scalar.", i + 1));
    }
    let v = m.data[0];
    if v < 0.0 {
        return rt(format!("Expected input number {}, {label}, to be nonnegative.", i + 1));
    }
    if v.fract() != 0.0 {
        return rt(format!("Expected input number {}, {label}, to be integer-valued.", i + 1));
    }
    Ok(v as usize)
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if name == "lpc" {
        return lpc(it, a, nargout);
    }
    if name == "statelevels" {
        return statelevels(a, nargout);
    }
    if name == "midcross" {
        return midcross(a, nargout);
    }
    if name == "risetime" || name == "falltime" {
        return edge_time(name, a, nargout);
    }
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let h = match &a[0] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt("Expected input number 1, H, to be numeric."),
    };
    if h.dims.len() > 2 || (h.dims[0] > 1 && h.dims[1] > 1) {
        return rt("Expected input number 1, H, to be a vector.");
    }
    let m = order_arg(a, 1, "NB")?;
    let n = order_arg(a, 2, "NA")?;
    let single = h.kind == Kind::Single;
    if h.numel() == 0 {
        let nan_row = |k: usize| Value::Num(Matrix::from_dims(vec![1, k], vec![f64::NAN; k]));
        return out(vec![nan_row(m + 1), nan_row(n + 1)]);
    }
    let mut re = h.data.clone();
    let mut im: Vec<f64> = (0..h.numel()).map(|i| h.im_at(i)).collect();
    let mut k = re.len() - 1;
    if k <= m.max(n) {
        let len = m.max(n) + 2;
        re.resize(len, 0.0);
        im.resize(len, 0.0);
        k = len - 1;
    }
    let (c_re, c_im) = if re[0] == 0.0 && im[0] == 0.0 { (1.0, 0.0) } else { (re[0], im[0]) };
    let cd = c_re * c_re + c_im * c_im;
    let sre: Vec<f64> = (0..re.len()).map(|i| (re[i] * c_re + im[i] * c_im) / cd).collect();
    let sim: Vec<f64> = (0..re.len()).map(|i| (im[i] * c_re - re[i] * c_im) / cd).collect();
    // H(i, j) = h(i - j) for the first K+1 rows of the convolution matrix.
    let at = |i: usize, j: usize| -> (f64, f64) { if i >= j { (sre[i - j], sim[i - j]) } else { (0.0, 0.0) } };
    let rows = k + 1 - (m + 1);
    let mut a_rest = vec![(0.0, 0.0); n];
    if n > 0 {
        let mut dre = vec![0.0; rows * n];
        let mut dim = vec![0.0; rows * n];
        let mut rre = vec![0.0; rows];
        let mut rim = vec![0.0; rows];
        for r in 0..rows {
            let i = m + 1 + r;
            let (x, y) = at(i, 0);
            rre[r] = -x;
            rim[r] = -y;
            for j in 0..n {
                let (x, y) = at(i, j + 1);
                dre[j * rows + r] = x;
                dim[j * rows + r] = y;
            }
        }
        let cplx = im.iter().any(|&v| v != 0.0);
        let mk = |r: usize, c: usize, d: Vec<f64>, i: Vec<f64>| {
            Value::Num(if cplx { Matrix::complex(vec![r, c], d, i) } else { Matrix::from_dims(vec![r, c], d) })
        };
        if !cplx {
            let cols: Vec<Vec<f64>> = (0..n).map(|j| dre[j * rows..(j + 1) * rows].to_vec()).collect();
            let x = crate::signal::ar::lstsq(&cols, &rre);
            for j in 0..n {
                a_rest[j] = (x[j], 0.0);
            }
        } else {
        let sol = it.call_function("mldivide", vec![mk(rows, n, dre, dim), mk(rows, 1, rre, rim)], 1)?.remove(0).to_matrix()?;
        for j in 0..n {
            a_rest[j] = (sol.data[j], sol.im_at(j));
        }
        }
    }
    let mut den: Vec<(f64, f64)> = vec![(1.0, 0.0)];
    den.extend(a_rest);
    let mut num = vec![(0.0, 0.0); m + 1];
    for (i, slot) in num.iter_mut().enumerate() {
        let (mut sr, mut si) = (0.0, 0.0);
        for (j, &(dr, di)) in den.iter().enumerate() {
            let (x, y) = at(i, j);
            sr += x * dr - y * di;
            si += x * di + y * dr;
        }
        *slot = (sr * c_re - si * c_im, sr * c_im + si * c_re);
    }
    let build = |v: &[(f64, f64)]| {
        let mut mat = if im.iter().any(|&x| x != 0.0) {
            Matrix::complex(vec![1, v.len()], v.iter().map(|p| p.0).collect(), v.iter().map(|p| p.1).collect())
        } else {
            Matrix::from_dims(vec![1, v.len()], v.iter().map(|p| p.0).collect())
        };
        if single {
            mat.kind = Kind::Single;
        }
        Value::Num(mat)
    };
    out(vec![build(&num), build(&den)])
}

fn lpc(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if m.kind == Kind::Double => m.clone(),
        _ => return rt("Input arguments must be 'double'."),
    };
    if x.numel() == 0 {
        return rt("Input vector X should not be empty");
    }
    let rows = if x.dims[0] == 1 { x.numel() } else { x.dims[0] };
    let ncols = x.numel() / rows;
    let p = if a.len() > 1 {
        let m = crate::builtins::m_arg(a, 1, "lpc")?;
        if m.numel() != 1 {
            return rt("Expected input number 2, N, to be a scalar.");
        }
        if m.data[0] < 0.0 || m.data[0].is_nan() {
            return rt("Order of the predictor should be a positive integer.");
        }
        m.data[0].floor() as usize
    } else {
        rows - 1
    };
    if rows == 1 {
        return out(vec![scalar_value(1.0), scalar_value(x.data[0] * x.data[0])]);
    }
    if p > rows {
        return rt("X must be a vector with length greater or equal to the prediction order. If X is a matrix, the length of each column must be greater or equal to the prediction order.");
    }
    let cplx = x.im.is_some();
    let mut arows: Vec<Vec<(f64, f64)>> = vec![];
    let mut gs: Vec<(f64, f64)> = vec![];
    for c in 0..ncols {
        let col: Vec<(f64, f64)> = (0..rows).map(|i| (x.data[c * rows + i], x.im_at(c * rows + i))).collect();
        let mut rr = vec![0.0; p + 1];
        let mut ri = vec![0.0; p + 1];
        for k in 0..=p {
            for n in 0..rows - k {
                let (ar, ai) = col[n + k];
                let (br, bi) = col[n];
                rr[k] += (ar * br + ai * bi) / rows as f64;
                ri[k] += (ai * br - ar * bi) / rows as f64;
            }
        }
        if cplx {
            let cm = |x: (f64, f64), y: (f64, f64)| (x.0 * y.0 - x.1 * y.1, x.0 * y.1 + x.1 * y.0);
            let r: Vec<(f64, f64)> = (0..=p).map(|k| (rr[k], ri[k])).collect();
            let mut av = vec![(1.0, 0.0)];
            let mut e = r[0].0;
            for m in 1..=p {
                let mut acc = (0.0, 0.0);
                for i in 0..m {
                    let t = cm(av[i], r[m - i]);
                    acc = (acc.0 + t.0, acc.1 + t.1);
                }
                let k = (-acc.0 / e, -acc.1 / e);
                let prev = av.clone();
                av.push((0.0, 0.0));
                for i in 1..=m {
                    let c = (prev.get(m - i).map_or(0.0, |v| v.0), -prev.get(m - i).map_or(0.0, |v| v.1));
                    let t = cm(k, c);
                    let base = if i < prev.len() { prev[i] } else { (0.0, 0.0) };
                    av[i] = (base.0 + t.0, base.1 + t.1);
                }
                e *= 1.0 - (k.0 * k.0 + k.1 * k.1);
            }
            arows.push(av);
            gs.push((e, 0.0));
            continue;
        }
        let rv = if cplx { Value::Num(Matrix::complex(vec![p + 1, 1], rr, ri)) } else { real_col(rr) };
        let res = it.call_function("levinson", vec![rv, scalar_value(p as f64)], 2)?;
        let am = res[0].to_matrix()?;
        arows.push((0..am.numel()).map(|i| (am.data[i], am.im_at(i))).collect());
        let e = res[1].to_matrix()?;
        gs.push((e.data[0], e.im_at(0)));
    }
    let width = p + 1;
    let mut re = vec![0.0; ncols * width];
    let mut im = vec![0.0; ncols * width];
    for (c, row) in arows.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            re[j * ncols + c] = v.0;
            im[j * ncols + c] = v.1;
        }
    }
    let mk = |d: Vec<f64>, i: Vec<f64>, r: usize, cc: usize| {
        Value::Num(if cplx { Matrix::complex(vec![r, cc], d, i) } else { Matrix::from_dims(vec![r, cc], d) })
    };
    let g = mk(gs.iter().map(|v| v.0).collect(), gs.iter().map(|v| v.1).collect(), ncols, 1);
    out(vec![mk(re, im, ncols, width), g])
}

/// Histogram of `x` over `[lb, ub]` in `nb` bins: counts and bin centers.
fn state_histogram(x: &[f64], nb: usize, lb: f64, ub: f64) -> (Vec<f64>, Vec<f64>) {
    let w = (ub - lb) / nb as f64;
    let mut h = vec![0.0; nb];
    for &v in x {
        if v >= lb && v <= ub && w > 0.0 {
            let k = ((((v - lb) / (ub - lb)) * nb as f64).ceil() as i64 - 1).clamp(0, nb as i64 - 1) as usize;
            h[k] += 1.0;
        }
    }
    let c = (0..nb).map(|k| lb + w * (k as f64 + 0.5)).collect();
    (h, c)
}

/// Lower and upper state levels of a bilevel waveform from its histogram.
pub(crate) fn levels_from_histogram(h: &[f64], c: &[f64], mean: bool) -> (f64, f64) {
    let nz: Vec<usize> = (0..h.len()).filter(|&i| h[i] > 0.0).collect();
    if nz.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    let (i0, i1) = (nz[0], *nz.last().unwrap());
    let m = i0 + (i1 - i0) / 2;
    let level = |lo: usize, hi: usize| -> f64 {
        if mean {
            let (sw, sc) = (lo..=hi).fold((0.0, 0.0), |acc, i| (acc.0 + h[i], acc.1 + h[i] * c[i]));
            sc / sw
        } else {
            let mut best = lo;
            for i in lo..=hi {
                if h[i] > h[best] {
                    best = i;
                }
            }
            c[best]
        }
    };
    (level(i0, m), level(m, i1))
}

fn statelevels(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if m.kind == Kind::Double => m.clone(),
        other => return rt(format!("Expected X to be one of these types:  double  Instead its type was {}.", other.class_name())),
    };
    if x.numel() == 1 {
        return rt("X must be a vector with more than one element.");
    }
    if x.numel() == 0 || x.dims.len() > 2 || (x.dims[0] > 1 && x.dims[1] > 1) {
        return rt("Expected X to be a vector.");
    }
    if x.im.is_some() {
        return rt("Expected X to be real.");
    }
    if x.data.iter().any(|v| !v.is_finite()) {
        return rt("Expected X to be finite.");
    }
    let mut nb = 100usize;
    if a.len() > 1 {
        match &a[1] {
            Value::Num(m) if m.kind == Kind::Double => {
                if m.numel() != 1 {
                    return rt("Expected NBINS to be a scalar.");
                }
                if m.data[0] <= 1.0 || m.data[0].is_nan() {
                    return rt("Expected NBINS to be a scalar with value > 1.");
                }
                if m.data[0].fract() != 0.0 {
                    return rt("Expected NBINS to be integer-valued.");
                }
                nb = m.data[0] as usize;
            }
            _ => return rt("Expected NBINS to be one of these types:  double"),
        }
    }
    let mut mean = false;
    if a.len() > 2 {
        let t = a[2].text().unwrap_or_default();
        match t.to_lowercase().as_str() {
            "mean" => mean = true,
            "mode" => {}
            _ => return rt(format!("Expected METHOD to match one of these values:  'mean', 'mode'  The input, '{t}', did not match any of the valid values.")),
        }
    }
    let (mut lb, mut ub) = (x.data.iter().cloned().fold(f64::INFINITY, f64::min), x.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    if a.len() > 3 {
        let m = crate::builtins::m_arg(a, 3, "statelevels")?;
        if m.numel() != 2 || m.dims[0] != 1 {
            return rt(format!("Expected BOUNDS to be of size 1x2, but it is of size {}x{}.", m.dims[0], m.dims[1]));
        }
        if m.data[1] <= m.data[0] {
            return rt("Expected BOUNDS to be strictly increasing.");
        }
        lb = m.data[0];
        ub = m.data[1];
    }
    let (h, c) = if ub > lb {
        state_histogram(&x.data, nb, lb, ub)
    } else {
        let mut h = vec![0.0; nb];
        h[0] = x.numel() as f64;
        (h, vec![lb; nb])
    };
    let (lo, hi) = if ub > lb { levels_from_histogram(&h, &c, mean) } else { (lb, lb) };
    let mut outs = vec![real_row(vec![lo, hi])];
    if nargout > 1 {
        outs.push(real_col(h));
    }
    if nargout > 2 {
        outs.push(real_col(c));
    }
    out(outs)
}

/// Options shared by the transition measurements.
pub(crate) struct Transitions {
    pub x: Vec<f64>,
    pub t: Vec<f64>,
    pub row: bool,
    pub tol: f64,
    pub mid_pct: f64,
    pub levels: (f64, f64),
    pub prl: (f64, f64),
}

/// Parses `(x [, Fs | T] [, name, value]...)` for midcross; the name-value names are listed in `known`.
pub(crate) fn parse_transitions(a: &[Value], allowed: &[&str]) -> R<Transitions> {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if m.kind == Kind::Double => m.clone(),
        other => return rt(format!("Expected X to be one of these types:  double  Instead its type was {}.", other.class_name())),
    };
    if x.numel() <= 1 && x.numel() > 0 && x.dims.len() == 2 {
        return rt("X must be a vector with more than one element.");
    }
    if x.numel() == 0 || x.dims.len() > 2 || (x.dims[0] > 1 && x.dims[1] > 1) {
        return rt("Expected X to be a vector.");
    }
    if x.im.is_some() {
        return rt("Expected X to be real.");
    }
    let n = x.numel();
    let mut t: Vec<f64> = (1..=n).map(|i| i as f64).collect();
    let mut i = 1;
    if let Some(Value::Num(m)) = a.get(1) {
        if m.kind != Kind::Char {
            if m.numel() == 1 {
                if m.data[0] <= 0.0 {
                    return rt("Expected FS to be positive.");
                }
                t = (0..n).map(|k| k as f64 / m.data[0]).collect();
            } else {
                if m.numel() != n {
                    return rt("X and T must be of the same length.");
                }
                t = m.data.clone();
            }
            i = 2;
        }
    }
    let (mut tol, mut mid_pct, mut levels) = (2.0, 50.0, None::<(f64, f64)>);
    let mut prl = (10.0, 90.0);
    while i < a.len() {
        let key = a[i].text().unwrap_or_default();
        let lk = key.to_lowercase();
        let canon = ["tolerance", "midpercentreferencelevel", "statelevels", "lowerpercentreferencelevel", "upperpercentreferencelevel", "percentreferencelevels"]
            .iter()
            .find(|c| c.starts_with(&lk) && !lk.is_empty() && allowed.contains(c))
            .copied();
        let Some(canon) = canon else {
            return rt(format!("Unexpected option: '{key}'."));
        };
        if i + 1 >= a.len() {
            return rt(format!("Parameter {} requires a value.", canon.to_lowercase()));
        }
        let m = crate::builtins::m_arg(a, i + 1, "midcross")?;
        match canon {
            "tolerance" => {
                if m.numel() != 1 {
                    return rt("Expected TOL to be a scalar.");
                }
                if m.data[0] <= 0.0 {
                    return rt("Expected TOL to be positive.");
                }
                tol = m.data[0];
            }
            "percentreferencelevels" => {
                if m.numel() != 2 || m.dims[0] != 1 {
                    return rt(format!("Expected PCTREFLEVELS to be of size 1x2, but it is of size {}x{}.", m.dims[0], m.dims[1]));
                }
                if m.data[1] <= m.data[0] {
                    return rt("The percent reference levels must be in increasing order.");
                }
                prl = (m.data[0], m.data[1]);
            }
            "midpercentreferencelevel" => {
                if m.numel() != 1 {
                    return rt("Expected MIDREFERENCELEVEL to be a scalar.");
                }
                mid_pct = m.data[0];
            }
            _ => {
                if m.numel() != 2 || m.dims[0] != 1 {
                    return rt(format!("Expected STATELEVELS to be of size 1x2, but it is of size {}x{}.", m.dims[0], m.dims[1]));
                }
                if m.data[1] <= m.data[0] {
                    return rt("The state levels must be in increasing order.");
                }
                levels = Some((m.data[0], m.data[1]));
            }
        }
        i += 2;
    }
    let levels = match levels {
        Some(l) => l,
        None => {
            let (lb, ub) = (x.data.iter().cloned().fold(f64::INFINITY, f64::min), x.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
            if ub > lb && x.data.iter().all(|v| v.is_finite()) {
                let (h, c) = state_histogram(&x.data, 100, lb, ub);
                levels_from_histogram(&h, &c, false)
            } else {
                (lb, lb)
            }
        }
    };
    Ok(Transitions { x: x.data.clone(), t, row: x.dims[0] == 1, tol, mid_pct, levels, prl })
}

/// Settled-state transitions: (start index, end index, rising) between samples within the
/// tolerance band of each state level.
pub(crate) fn find_transitions(tr: &Transitions) -> Vec<(usize, usize, bool)> {
    let (lo, hi) = tr.levels;
    let band = tr.tol / 100.0 * (hi - lo);
    let state = |v: f64| -> i32 {
        if v <= lo + band {
            0
        } else if v >= hi - band {
            1
        } else {
            -1
        }
    };
    let mut res = vec![];
    let mut last: Option<(usize, i32)> = None;
    for (i, &v) in tr.x.iter().enumerate() {
        let s = state(v);
        if s < 0 {
            continue;
        }
        if let Some((j, ls)) = last {
            if ls != s {
                res.push((j, i, s == 1));
            }
        }
        last = Some((i, s));
    }
    res
}

/// Time of the first crossing of `level` within `start..=end`.
pub(crate) fn crossing(tr: &Transitions, start: usize, end: usize, rising: bool, level: f64) -> Option<f64> {
    for i in start..end {
        let (a, b) = (tr.x[i], tr.x[i + 1]);
        let hit = if rising { a < level && b >= level } else { a > level && b <= level };
        if hit {
            return Some(tr.t[i] + (level - a) / (b - a) * (tr.t[i + 1] - tr.t[i]));
        }
    }
    None
}

fn shaped(v: Vec<f64>, row: bool) -> Value {
    if row { real_row(v) } else { real_col(v) }
}

fn midcross(a: &[Value], nargout: usize) -> Ret {
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let tr = parse_transitions(a, &["tolerance", "midpercentreferencelevel", "statelevels"])?;
    let (lo, hi) = tr.levels;
    let mid = lo + tr.mid_pct / 100.0 * (hi - lo);
    let c: Vec<f64> = find_transitions(&tr).into_iter().filter_map(|(s, e, r)| crossing(&tr, s, e, r, mid)).collect();
    let mut outs = vec![if c.is_empty() { Value::Num(Matrix::from_dims(if tr.row { vec![1, 0] } else { vec![0, 1] }, vec![])) } else { shaped(c, tr.row) }];
    if nargout > 1 {
        outs.push(scalar_value(mid));
    }
    out(outs)
}

fn edge_time(name: &str, a: &[Value], nargout: usize) -> Ret {
    if nargout > 5 {
        return rt("Too many output arguments.");
    }
    let tr = parse_transitions(a, &["tolerance", "percentreferencelevels", "statelevels"])?;
    if tr.tol >= tr.prl.0 {
        return rt("The percent state level tolerance must be less than the lower percent reference level.");
    }
    let rising = name == "risetime";
    let (lo, hi) = tr.levels;
    let ll = lo + tr.prl.0 / 100.0 * (hi - lo);
    let ul = lo + tr.prl.1 / 100.0 * (hi - lo);
    let (mut d, mut lt, mut ut) = (vec![], vec![], vec![]);
    for (s, e, r) in find_transitions(&tr) {
        if r != rising {
            continue;
        }
        let (Some(tl), Some(tu)) = (crossing(&tr, s, e, r, ll), crossing(&tr, s, e, r, ul)) else { continue };
        lt.push(tl);
        ut.push(tu);
        d.push(if rising { tu - tl } else { tl - tu });
    }
    let empty = || Value::Num(Matrix::from_dims(if tr.row { vec![1, 0] } else { vec![0, 1] }, vec![]));
    let mk = |v: Vec<f64>| if v.is_empty() { empty() } else { shaped(v, tr.row) };
    let mut outs = vec![mk(d), mk(lt), mk(ut), scalar_value(ll), scalar_value(ul)];
    outs.truncate(nargout.max(1));
    out(outs)
}
