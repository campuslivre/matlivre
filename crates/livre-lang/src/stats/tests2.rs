//! `fishertest`, `runstest`, `partialcorr`, `grp2idx`, `confusionmat`, `cholcov`, `jackknife`,
//! `silhouette`, `linhyptest`, `glmval`.

use super::misc::lstsq;
use super::util::*;
use crate::builtins::Ret;
use crate::builtins_specfun::gammaln;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::value::*;

pub const NAMES: &[&str] = &["fishertest", "runstest", "partialcorr", "grp2idx", "confusionmat", "cholcov", "jackknife", "silhouette", "linhyptest", "glmval"];

fn cdf(it: &mut Interpreter, name: &str, x: f64, params: &[f64]) -> Res<f64> {
    let mut args = vec![Value::num(x)];
    args.extend(params.iter().map(|&p| Value::num(p)));
    let r = it.call_function(name, args, 1)?;
    r[0].scalar()
}

fn matches_name(given: &str, full: &str) -> bool {
    !given.is_empty() && full.to_lowercase().starts_with(&given.to_lowercase())
}

fn lbinom(n: f64, k: f64) -> f64 {
    gammaln(n + 1.0) - gammaln(k + 1.0) - gammaln(n - k + 1.0)
}

fn fishertest(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let _ = nargout;
    let Some(Value::Num(x)) = a.first() else { return rt("FISHERTEST requires a 2-by-2 matrix or table X.") };
    if x.dims != [2, 2] {
        return rt("FISHERTEST requires a 2-by-2 matrix or table X.");
    }
    if x.data.iter().any(|&v| v < 0.0 || v.fract() != 0.0 || !v.is_finite()) {
        return rt("FISHERTEST requires the matrix or table X to contain only nonnegative integers.");
    }
    let mut alpha = 0.05;
    let mut tail = 0;
    if (a.len() - 1) % 2 != 0 {
        return rt("Invalid parameter name/value pair.");
    }
    for pair in a[1..].chunks(2) {
        let key = pair[0].text().unwrap_or_default();
        if matches_name(&key, "Alpha") {
            alpha = pair[1].scalar()?;
            if !(alpha > 0.0 && alpha < 1.0) {
                return rt("ALPHA must be a scalar between 0 and 1.");
            }
        } else if matches_name(&key, "Tail") {
            let t = pair[1].text().unwrap_or_default();
            tail = match ["both", "right", "left"].iter().position(|n| matches_name(&t, n)) {
                Some(i) => i,
                None => return rt("'TAIL' must be 'both', 'right' or 'left'."),
            };
        } else {
            return rt(format!("Invalid parameter name: {key}."));
        }
    }
    if x.kind != Kind::Double && x.kind != Kind::Single && x.kind != Kind::Logical {
        return rt("Unrecognized function or variable 'p'.");
    }
    let (aa, bb, cc, dd) = (x.data[0], x.data[2], x.data[1], x.data[3]);
    let (r1, c1, n) = (aa + bb, aa + cc, aa + bb + cc + dd);
    let lo = (r1 + c1 - n).max(0.0);
    let hi = r1.min(c1);
    let lp = |k: f64| lbinom(r1, k) + lbinom(n - r1, c1 - k) - lbinom(n, c1);
    let pobs = lp(aa).exp();
    let mut p = 0.0;
    let mut k = lo;
    while k <= hi {
        let pk = lp(k).exp();
        p += match tail {
            0 => {
                if pk <= pobs * (1.0 + 1e-7) {
                    pk
                } else {
                    0.0
                }
            }
            1 => {
                if k >= aa {
                    pk
                } else {
                    0.0
                }
            }
            _ => {
                if k <= aa {
                    pk
                } else {
                    0.0
                }
            }
        };
        k += 1.0;
    }
    let p = p.min(1.0);
    let or = (aa * dd) / (bb * cc);
    let se = (1.0 / aa + 1.0 / bb + 1.0 / cc + 1.0 / dd).sqrt();
    let z = super::dist::norminv_std(1.0 - alpha / 2.0);
    let ci = if or == 0.0 || !or.is_finite() { [f64::NEG_INFINITY, f64::INFINITY] } else { [(or.ln() - z * se).exp(), (or.ln() + z * se).exp()] };
    let _ = it;
    let mut st = StructArray::scalar();
    st.set("OddsRatio", Value::num(or));
    st.set("ConfidenceInterval", Value::Num(Matrix::from_dims(vec![1, 2], ci.to_vec())));
    Ok(Some(vec![Value::bool(p <= alpha), Value::num(p), Value::Struct(st)]))
}

fn runs_pmf(n1: usize, n0: usize) -> Vec<f64> {
    let n = (n1 + n0) as f64;
    let tot = lbinom(n, n1 as f64);
    let mut pm = vec![0.0; n1.min(n0) * 2 + 2];
    let c = |nn: usize, k: isize| -> f64 { if k < 0 || k as usize > nn { f64::NEG_INFINITY } else { lbinom(nn as f64, k as f64) } };
    for r in 2..pm.len() {
        let k = (r / 2) as isize;
        let v = if r % 2 == 0 {
            2.0 * (c(n1 - 1, k - 1) + c(n0 - 1, k - 1) - tot).exp()
        } else {
            (c(n1 - 1, k) + c(n0 - 1, k - 1) - tot).exp() + (c(n1 - 1, k - 1) + c(n0 - 1, k) - tot).exp()
        };
        pm[r] = if v.is_finite() { v } else { 0.0 };
    }
    pm
}

fn runstest(it: &mut Interpreter, a: &[Value]) -> Ret {
    let Some(Value::Num(x0)) = a.first() else { return rt("Not enough input arguments.") };
    if x0.dims.len() > 2 || (x0.dims[0] > 1 && x0.dims[1] > 1) {
        return rt("Input sample X must be a vector.");
    }
    let mut rest = &a[1..];
    let mut vspec: Option<Value> = None;
    if let Some(v) = rest.first() {
        vspec = Some(v.clone());
        rest = &rest[1..];
    }
    if let Some(v) = &vspec {
        let is_ud = v.is_text() && v.text().map(|t| t.eq_ignore_ascii_case("ud")).unwrap_or(false);
        if !is_ud && !(matches!(v, Value::Num(m) if m.numel() == 1 && m.kind != Kind::Char)) {
            return rt("V must be either 'UD' or a number");
        }
    }
    let mut alpha = 0.05;
    let mut tail = 0;
    let mut method: Option<bool> = None; // true = exact
    for pair in rest.chunks(2) {
        if pair.len() < 2 {
            return rt("Invalid parameter name/value pair.");
        }
        let key = pair[0].text().unwrap_or_default();
        if matches_name(&key, "Alpha") {
            alpha = pair[1].scalar()?;
            if !(alpha > 0.0 && alpha < 1.0) {
                return rt("ALPHA must be a scalar between 0 and 1.");
            }
        } else if matches_name(&key, "Tail") {
            let t = pair[1].text().unwrap_or_default();
            tail = match ["both", "right", "left"].iter().position(|n| matches_name(&t, n)) {
                Some(i) => i,
                None => return rt("'TAIL' must be 'both', 'right' or 'left'."),
            };
        } else if matches_name(&key, "Method") {
            let t = pair[1].text().unwrap_or_default();
            method = match ["exact", "approximate"].iter().position(|n| matches_name(&t, n)) {
                Some(i) => Some(i == 0),
                None => return rt(format!("'{t}' is not a valid value for the 'Method' argument.  Valid values are: 'exact', 'approximate'.")),
            };
        } else {
            return rt(format!("Invalid parameter name: {key}."));
        }
    }
    let single = false;
    let x: Vec<f64> = x0.data.iter().copied().filter(|v| !v.is_nan()).collect();
    let n_all = x.len();
    let ud = matches!(&vspec, Some(v) if v.is_text() && v.text().map(|t| t.eq_ignore_ascii_case("ud")).unwrap_or(false));
    if let Some(v) = &vspec {
        if !ud && !(matches!(v, Value::Num(m) if m.numel() == 1 && m.kind != Kind::Char)) {
            return rt("V must be either 'UD' or a number");
        }
    }
    let mut st = StructArray::scalar();
    let out = |h: bool, p: f64, mut st: StructArray, nruns: f64, n1: f64, n0: f64, z: f64| -> Ret {
        st.set("nruns", Value::num(nruns));
        st.set("n1", Value::num(n1));
        st.set("n0", Value::num(n0));
        st.set("z", Value::num(z));
        Ok(Some(vec![Value::num(if h { 1.0 } else { 0.0 }), Value::num(p), Value::Struct(st)]))
    };
    if ud {
        let d: Vec<f64> = x.windows(2).map(|w| w[1] - w[0]).filter(|&v| v != 0.0).collect();
        let n = n_all as f64;
        if d.is_empty() {
            return out(false, 1.0, st, 0.0, 0.0, 0.0, f64::NAN);
        }
        let nr = 1.0 + d.windows(2).filter(|w| (w[0] > 0.0) != (w[1] > 0.0)).count() as f64;
        let mu = (2.0 * n - 1.0) / 3.0;
        let sd = ((16.0 * n - 29.0) / 90.0).sqrt();
        let cc = if nr > mu { -0.5 } else { 0.5 };
        let z = (nr - mu + cc) / sd;
        let (pl, pr) = (cdf(it, "normcdf", (nr + 0.5 - mu) / sd, &[])?, 1.0 - cdf(it, "normcdf", (nr - 0.5 - mu) / sd, &[])?);
        let p = match tail {
            0 => 2.0 * cdf(it, "normcdf", -z.abs(), &[])?,
            1 => pr,
            _ => pl,
        };
        return out(p <= alpha, p, st, nr, d.iter().filter(|&&v| v > 0.0).count() as f64, d.iter().filter(|&&v| v < 0.0).count() as f64, z);
    }
    let v = match &vspec {
        Some(v) => v.scalar()?,
        None => {
            if x.is_empty() {
                f64::NAN
            } else {
                x.iter().sum::<f64>() / x.len() as f64
            }
        }
    };
    let signs: Vec<bool> = x.iter().filter(|&&t| t != v).map(|&t| t > v).collect();
    let n1 = signs.iter().filter(|&&s| s).count();
    let n0 = signs.len() - n1;
    if n1 == 0 || n0 == 0 {
        return out(false, 1.0, st, if signs.is_empty() { 0.0 } else { 1.0 }, n1 as f64, n0 as f64, f64::NAN);
    }
    let nr = 1 + signs.windows(2).filter(|w| w[0] != w[1]).count();
    let n = (n1 + n0) as f64;
    let (f1, f0) = (n1 as f64, n0 as f64);
    let exact = method.unwrap_or(n1 <= 50 && n0 <= 50 && n1 + n0 <= 100 && false || (n1 + n0) <= 50 + 0 * 1);
    let mu = 2.0 * f1 * f0 / n + 1.0;
    let sd = (2.0 * f1 * f0 * (2.0 * f1 * f0 - n) / (n * n * (n - 1.0))).sqrt();
    let cc = if (nr as f64) > mu { -0.5 } else { 0.5 };
    let z = (nr as f64 - mu + cc) / sd;
    let p = if exact {
        let pm = runs_pmf(n1, n0);
        let pl: f64 = pm.iter().take(nr + 1).sum();
        let pr: f64 = pm.iter().skip(nr).sum();
        match tail {
            0 => (2.0 * pl.min(pr)).min(1.0),
            1 => pr.min(1.0),
            _ => pl.min(1.0),
        }
    } else {
        match tail {
            0 => 2.0 * cdf(it, "normcdf", -z.abs(), &[])?,
            1 => 1.0 - cdf(it, "normcdf", (nr as f64 - 0.5 - mu) / sd, &[])?,
            _ => cdf(it, "normcdf", (nr as f64 + 0.5 - mu) / sd, &[])?,
        }
    };
    st = StructArray::scalar();
    out(p <= alpha, p, st, nr as f64, f1, f0, z)
}

fn rank_avg(v: &[f64]) -> Vec<f64> {
    let n = v.len();
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| v[a].partial_cmp(&v[b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut r = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && v[idx[j + 1]] == v[idx[i]] {
            j += 1;
        }
        let av = (i + j) as f64 / 2.0 + 1.0;
        for t in i..=j {
            r[idx[t]] = av;
        }
        i = j + 1;
    }
    r
}

fn cols_of(m: &Matrix) -> Vec<Vec<f64>> {
    let r = m.dims[0];
    (0..m.dims[1]).map(|j| m.data[j * r..(j + 1) * r].to_vec()).collect()
}

fn residual(col: &[f64], z: &[Vec<f64>]) -> Vec<f64> {
    let n = col.len();
    let p = z.len() + 1;
    let design: Vec<Vec<f64>> = (0..n).map(|i| std::iter::once(1.0).chain(z.iter().map(|c| c[i])).collect()).collect();
    let b = lstsq(&design, col, p);
    (0..n).map(|i| col[i] - (0..p).map(|j| design[i][j] * b[j]).sum::<f64>()).collect()
}

fn partialcorr(it: &mut Interpreter, a: &[Value]) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let mut mats: Vec<&Matrix> = Vec::new();
    let mut k = 0;
    while k < a.len() && matches!(&a[k], Value::Num(m) if m.kind != Kind::Char) && mats.len() < 3 {
        if let Value::Num(m) = &a[k] {
            mats.push(m);
        }
        k += 1;
    }
    let mut spearman = false;
    let mut rows_complete = false;
    let mut tail = 0;
    if (a.len() - k) % 2 != 0 {
        return rt("Invalid parameter name/value pair.");
    }
    for pair in a[k..].chunks(2) {
        let key = pair[0].text().unwrap_or_default();
        let val = pair[1].text().unwrap_or_default();
        if matches_name(&key, "Type") {
            match ["Pearson", "Kendall", "Spearman"].iter().position(|n| matches_name(&val, n)) {
                Some(0) => {}
                Some(1) => return rt("Cannot compute Kendall's partial rank correlation."),
                Some(_) => spearman = true,
                None => return rt(format!("Invalid value for parameter Type: {val}.")),
            }
        } else if matches_name(&key, "Rows") {
            rows_complete = !matches_name(&val, "all");
        } else if matches_name(&key, "Tail") {
            tail = match ["both", "right", "left"].iter().position(|n| matches_name(&val, n)) {
                Some(i) => i,
                None => return rt("'TAIL' must be 'both', 'right' or 'left'."),
            };
        } else {
            return rt(format!("Invalid parameter name: {key}."));
        }
    }
    let n = mats[0].dims[0];
    if mats.iter().any(|m| m.dims[0] != n) {
        return rt("All input arrays must have the same number of rows.");
    }
    let single = mats.iter().any(|m| is_single(m));
    let xs = cols_of(mats[0]);
    let (ys, zs, xonly): (Vec<Vec<f64>>, Vec<Vec<f64>>, bool) = match mats.len() {
        1 => (xs.clone(), vec![], true),
        2 => (xs.clone(), cols_of(mats[1]), false),
        _ => (cols_of(mats[1]), cols_of(mats[2]), false),
    };
    let (p, q) = (xs.len(), ys.len());
    let mut rho = vec![f64::NAN; p * q];
    let mut pv = vec![f64::NAN; p * q];
    for i in 0..p {
        for j in 0..q {
            let zz: Vec<&Vec<f64>> = if xonly { (0..p).filter(|&t| t != i && t != j).map(|t| &xs[t]).collect() } else { zs.iter().collect() };
            let keep: Vec<usize> = (0..n).filter(|&r| !xs[i][r].is_nan() && !ys[j][r].is_nan() && zz.iter().all(|c| !c[r].is_nan())).collect();
            if keep.len() != n && !rows_complete {
                continue;
            }
            let pick = |c: &Vec<f64>| -> Vec<f64> { keep.iter().map(|&r| c[r]).collect() };
            let (mut cx, mut cy) = (pick(&xs[i]), pick(&ys[j]));
            let mut cz: Vec<Vec<f64>> = zz.iter().map(|c| pick(c)).collect();
            if spearman {
                cx = rank_avg(&cx);
                cy = rank_avg(&cy);
                cz = cz.iter().map(|c| rank_avg(c)).collect();
            }
            let m = keep.len();
            let kz = cz.len();
            let dof = m as f64 - 2.0 - kz as f64;
            if dof <= 0.0 && !(xonly && i == j) {
                continue;
            }
            if xonly && i == j {
                if m > p {
                    rho[j * p + i] = 1.0;
                    pv[j * p + i] = 0.0;
                }
                continue;
            }
            let (rx, ry) = (residual(&cx, &cz), residual(&cy, &cz));
            let scale = |v: &[f64], r: &[f64]| -> bool {
                let mv = v.iter().sum::<f64>() / v.len() as f64;
                let tot: f64 = v.iter().map(|a| (a - mv).powi(2)).sum();
                r.iter().map(|a| a * a).sum::<f64>() <= tot * if single { 1e-10 } else { 1e-20 }
            };
            if scale(&cx, &rx) || scale(&cy, &ry) {
                continue;
            }
            let (mx, my) = (rx.iter().sum::<f64>() / m as f64, ry.iter().sum::<f64>() / m as f64);
            let sxy: f64 = rx.iter().zip(&ry).map(|(a, b)| (a - mx) * (b - my)).sum();
            let sxx: f64 = rx.iter().map(|a| (a - mx).powi(2)).sum();
            let syy: f64 = ry.iter().map(|b| (b - my).powi(2)).sum();
            let r = (sxy / (sxx * syy).sqrt()).clamp(-1.0, 1.0);
            let t = r * (dof / (1.0 - r * r)).sqrt();
            let pval = match tail {
                0 => 2.0 * cdf(it, "tcdf", -t.abs(), &[dof])?,
                1 => cdf(it, "tcdf", -t, &[dof])?,
                _ => cdf(it, "tcdf", t, &[dof])?,
            };
            rho[j * p + i] = r;
            pv[j * p + i] = pval;
        }
    }
    let mk = |d: Vec<f64>| Value::Num(finish(Matrix::from_dims(vec![p, q], d), single));
    Ok(Some(vec![mk(rho), mk(pv)]))
}

/// Text labels of a grouping value (`None` = missing), or numeric values.
pub(crate) enum Labels {
    Num(Vec<f64>, Kind),
    Text(Vec<Option<String>>, bool),
}

pub(crate) fn labels_of(v: &Value) -> Res<Labels> {
    let bad = || rt::<Labels>("Grouping variable must be a vector, character array, string array, or cell array of character vectors.");
    match v {
        Value::Num(m) if m.kind == Kind::Char => {
            if m.dims.len() == 2 && m.dims[0] == 1 {
                Ok(Labels::Text(vec![Some(m.to_string_lossy())], false))
            } else if m.dims.len() == 2 && m.dims[0] > 1 {
                // The rows of a character matrix are the labels.
                let (r, c) = (m.dims[0], m.dims[1]);
                Ok(Labels::Text((0..r).map(|i| Some((0..c).map(|j| char::from_u32(m.data[j * r + i] as u32).unwrap_or(' ')).collect::<String>())).collect(), false))
            } else {
                bad()
            }
        }
        Value::Num(m) => {
            if m.dims.len() > 2 || (m.dims[0] != 1 && m.dims[1] != 1) {
                return bad();
            }
            Ok(Labels::Num(m.data.clone(), m.kind))
        }
        Value::Cell(c) => {
            if c.dims.len() > 2 || (c.dims[0] != 1 && c.dims[1] != 1) {
                return bad();
            }
            let mut out = Vec::new();
            for e in &c.data {
                match e {
                    Value::Num(m) if m.kind == Kind::Char => out.push(if m.is_empty() { None } else { Some(m.to_string_lossy()) }),
                    _ => return rt("Grouping variable must be a categorical, numeric, logical, datetime or duration vector, character array, string array, or cell vector of character vectors."),
                }
            }
            Ok(Labels::Text(out, true))
        }
        Value::Str(s) => Ok(Labels::Text(s.data.iter().map(|t| t.clone().filter(|x| !x.is_empty() || true)).collect(), false)),
        _ => bad(),
    }
}

pub(crate) fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

fn cell_col(items: Vec<String>) -> Value {
    let n = items.len();
    Value::Cell(CellArray::from_dims(vec![n, 1], items.iter().map(|s| Value::chars(s)).collect()))
}

fn grp2idx(a: &[Value]) -> Ret {
    let Some(v) = a.first() else { return rt("Not enough input arguments.") };
    let lab = labels_of(v)?;
    match lab {
        Labels::Num(vals, kind) => {
            let n = vals.len();
            if n == 0 {
                return rt("Grouping variable must be a vector, character array, string array, or cell array of character vectors.");
            }
            let mut u: Vec<f64> = vals.iter().copied().filter(|x| !x.is_nan()).collect();
            u.sort_by(|a, b| a.partial_cmp(b).unwrap());
            u.dedup();
            let g: Vec<f64> = vals.iter().map(|x| if x.is_nan() { f64::NAN } else { u.iter().position(|y| y == x).unwrap() as f64 + 1.0 }).collect();
            let names: Vec<String> = u.iter().map(|&x| if kind == Kind::Logical { (if x != 0.0 { "true" } else { "false" }).to_string() } else { fmt_num(x) }).collect();
            let mut gl = Matrix::from_dims(vec![u.len(), 1], u.clone());
            gl.kind = kind;
            Ok(Some(vec![Value::Num(Matrix::from_dims(vec![n, 1], g)), cell_col(names), Value::Num(gl)]))
        }
        Labels::Text(items, is_cell) => {
            let n = items.len();
            let mut u: Vec<String> = Vec::new();
            let mut g = Vec::new();
            for it in &items {
                match it {
                    Some(s) => {
                        let p = match u.iter().position(|x| x == s) {
                            Some(p) => p,
                            None => {
                                u.push(s.clone());
                                u.len() - 1
                            }
                        };
                        g.push(p as f64 + 1.0);
                    }
                    None => g.push(f64::NAN),
                }
            }
            let gl = if is_cell {
                cell_col(u.clone())
            } else if matches!(v, Value::Str(_)) {
                Value::Str(StrArray::col(u.clone()))
            } else if matches!(v, Value::Num(m) if m.dims[0] > 1) {
                // The unique rows as a character matrix.
                let (r, w) = (u.len(), u.iter().map(|s| s.chars().count()).max().unwrap_or(0));
                let mut data = vec![32.0; r * w];
                for (i, s) in u.iter().enumerate() {
                    for (j, ch) in s.chars().enumerate() {
                        data[j * r + i] = ch as u32 as f64;
                    }
                }
                let mut m = Matrix::from_dims(vec![r, w], data);
                m.kind = Kind::Char;
                Value::Num(m)
            } else {
                Value::chars(&u[0])
            };
            Ok(Some(vec![Value::Num(Matrix::from_dims(vec![n, 1], g)), cell_col(u), gl]))
        }
    }
}

fn confusionmat(a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    let (l1, l2) = (labels_of(&a[0])?, labels_of(&a[1])?);
    let mut order: Option<&Value> = None;
    if a.len() > 2 {
        if (a.len() - 2) % 2 != 0 {
            return rt("Invalid parameter name/value pair.");
        }
        for pair in a[2..].chunks(2) {
            let key = pair[0].text().unwrap_or_default();
            if matches_name(&key, "Order") {
                order = Some(&pair[1]);
            } else {
                return rt(format!("Expected input to match one of these values:\n\n'Order'\n\nThe input, '{key}', did not match any of the valid values."));
            }
        }
    }
    match (l1, l2) {
        (Labels::Num(g, k1), Labels::Num(h, k2)) => {
            if k1 != k2 {
                return rt("G and GHAT need to be the same type.");
            }
            if g.len() != h.len() {
                return rt("G and GHAT need to have same number of rows");
            }
            let pairs: Vec<(f64, f64)> = g.iter().copied().zip(h.iter().copied()).filter(|(a, b)| !a.is_nan() && !b.is_nan()).collect();
            let mut u: Vec<f64> = pairs.iter().flat_map(|p| [p.0, p.1]).collect();
            u.sort_by(|a, b| a.partial_cmp(b).unwrap());
            u.dedup();
            let ord: Vec<f64> = match order {
                Some(Value::Num(o)) => {
                    if u.iter().any(|x| !o.data.contains(x)) {
                        return rt("ORDER should contain all the labels appearing in G or GHAT.");
                    }
                    o.data.clone()
                }
                Some(_) => return rt("ORDER should contain all the labels appearing in G or GHAT."),
                None => u,
            };
            let k = ord.len();
            let mut c = vec![0.0; k * k];
            for (x, y) in pairs {
                if let (Some(i), Some(j)) = (ord.iter().position(|&v| v == x), ord.iter().position(|&v| v == y)) {
                    c[j * k + i] += 1.0;
                }
            }
            let mut om = Matrix::from_dims(vec![k, 1], ord);
            om.kind = k1;
            Ok(Some(vec![Value::Num(Matrix::from_dims(vec![k, k], c)), Value::Num(om)]))
        }
        (Labels::Text(g, c1), Labels::Text(h, c2)) => {
            if c1 != c2 {
                return rt("G and GHAT need to be the same type.");
            }
            if g.len() != h.len() {
                return rt("G and GHAT need to have same number of rows");
            }
            let pairs: Vec<(String, String)> = g.iter().zip(h.iter()).filter_map(|(a, b)| Some((a.clone()?, b.clone()?))).collect();
            let mut u: Vec<String> = pairs.iter().flat_map(|p| [p.0.clone(), p.1.clone()]).collect();
            u.sort();
            u.dedup();
            let ord: Vec<String> = match order {
                Some(Value::Cell(o)) => {
                    let os: Vec<String> = o.data.iter().filter_map(|v| v.text().ok()).collect();
                    if u.iter().any(|x| !os.contains(x)) {
                        return rt("ORDER should contain all the labels appearing in G or GHAT.");
                    }
                    os
                }
                Some(_) => return rt("ORDER should contain all the labels appearing in G or GHAT."),
                None => u,
            };
            let k = ord.len();
            let mut c = vec![0.0; k * k];
            for (x, y) in pairs {
                if let (Some(i), Some(j)) = (ord.iter().position(|v| *v == x), ord.iter().position(|v| *v == y)) {
                    c[j * k + i] += 1.0;
                }
            }
            Ok(Some(vec![Value::Num(Matrix::from_dims(vec![k, k], c)), cell_col(ord)]))
        }
        _ => rt("G and GHAT need to be the same type."),
    }
}

fn cholcov(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let Some(Value::Num(s)) = a.first() else { return rt("Not enough input arguments.") };
    let single = is_single(s);
    let flag = match a.get(1) {
        Some(v) => v.scalar()? != 0.0,
        None => true,
    };
    let n = s.dims[0];
    let sym = s.dims.len() == 2 && s.dims[0] == s.dims[1] && (0..n).all(|i| (0..n).all(|j| s.data[j * n + i] == s.data[i * n + j]));
    let empty = Value::Num(Matrix::from_dims(vec![0, 0], vec![]));
    if !sym {
        if nargout < 2 && flag {
            return rt("SIGMA must be a square, symmetric matrix.");
        }
        return Ok(Some(vec![empty, Value::num(f64::NAN)]));
    }
    // positive definite: Cholesky
    let rows: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| s.data[j * n + i]).collect()).collect();
    if let Some(r) = super::dist_matrix::chol_upper(&rows) {
        let mut d = vec![0.0; n * n];
        for i in 0..n {
            for j in 0..n {
                d[j * n + i] = r[i][j];
            }
        }
        return Ok(Some(vec![Value::Num(finish(Matrix::from_dims(vec![n, n], d), single)), Value::num(0.0)]));
    }
    let res = it.call_function("eig", vec![Value::Num(Matrix::from_dims(vec![n, n], s.data.clone()))], 2)?;
    let (Value::Num(v), Value::Num(dm)) = (&res[0], &res[1]) else { return rt("eig failed") };
    let lam: Vec<f64> = (0..n).map(|i| dm.data[i * n + i]).collect();
    let maxd = (0..n).map(|i| s.data[i * n + i].abs()).fold(0.0, f64::max);
    let tol = crate::stats::util::eps_of(maxd) * n as f64;
    let neg = lam.iter().filter(|&&l| l < -tol).count();
    if neg > 0 {
        if nargout < 2 && flag {
            return rt("SIGMA must be a symmetric positive semi-definite matrix.");
        }
        return Ok(Some(vec![empty, Value::num(neg as f64)]));
    }
    let keep: Vec<usize> = (0..n).filter(|&i| lam[i] > tol).collect();
    let m = keep.len();
    let mut d = vec![0.0; m * n];
    for (r, &i) in keep.iter().enumerate() {
        for j in 0..n {
            d[j * m + r] = lam[i].sqrt() * v.data[i * n + j];
        }
    }
    Ok(Some(vec![Value::Num(finish(Matrix::from_dims(vec![m, n], d), single)), Value::num(0.0)]))
}

fn jackknife(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let _ = nargout;
    if a.len() < 2 {
        return rt("JACKKNIFE requires a function handle and at least one data argument.");
    }
    let f = match &a[0] {
        Value::Func(f) => f.clone(),
        v if v.is_text() => {
            let name = v.text()?;
            match it.call_function("str2func", vec![Value::chars(&name)], 1)?.into_iter().next() {
                Some(Value::Func(f)) => f,
                _ => return rt("JACKKNIFE requires a function handle and at least one data argument."),
            }
        }
        _ => return rt("JACKKNIFE requires a function handle and at least one data argument."),
    };
    let mut datas: Vec<Matrix> = Vec::new();
    for v in &a[1..] {
        let Value::Num(m) = v else { return rt("Data arguments must be numeric.") };
        let m = if m.dims.len() == 2 && m.dims[0] == 1 && m.dims[1] > 1 { let mut t = Matrix::from_dims(vec![m.dims[1], 1], m.data.clone()); t.kind = m.kind; t } else { m.clone() };
        datas.push(m);
    }
    let n = datas[0].dims[0];
    let single = datas.iter().any(|m| is_single(m));
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for i in 0..n {
        let mut args = Vec::new();
        for m in &datas {
            let (r, c) = (m.dims[0], m.dims[1]);
            let mut d = Vec::with_capacity((r - 1) * c);
            for j in 0..c {
                for t in 0..r {
                    if t != i {
                        d.push(m.data[j * r + t]);
                    }
                }
            }
            let mut mm = Matrix::from_dims(vec![r - 1, c], d);
            mm.kind = m.kind;
            args.push(Value::Num(mm));
        }
        let res = it.call_value(&f, args, 1)?;
        let Some(Value::Num(o)) = res.first() else { return rt("The function must return a numeric value.") };
        rows.push(o.data.clone());
    }
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut d = vec![0.0; n * w];
    for (i, r) in rows.iter().enumerate() {
        for j in 0..w {
            d[j * n + i] = r.get(j).copied().unwrap_or(f64::NAN);
        }
    }
    Ok(Some(vec![Value::Num(finish(Matrix::from_dims(vec![n, w], d), single))]))
}

fn silhouette(it: &mut Interpreter, a: &[Value]) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    let Value::Num(x) = &a[0] else { return rt("X must be numeric.") };
    let single = is_single(x);
    let (n, p) = (x.dims[0], x.dims[1]);
    let clust: Vec<Option<usize>> = match labels_of(&a[1])? {
        Labels::Num(v, _) => {
            let mut u: Vec<f64> = v.iter().copied().filter(|t| !t.is_nan()).collect();
            u.sort_by(|a, b| a.partial_cmp(b).unwrap());
            u.dedup();
            v.iter().map(|t| if t.is_nan() { None } else { u.iter().position(|y| y == t) }).collect()
        }
        Labels::Text(items, _) => {
            let mut u: Vec<String> = Vec::new();
            items
                .iter()
                .map(|t| {
                    t.as_ref().map(|s| match u.iter().position(|y| y == s) {
                        Some(i) => i,
                        None => {
                            u.push(s.clone());
                            u.len() - 1
                        }
                    })
                })
                .collect()
        }
    };
    if clust.len() != n {
        return rt("The number of rows in X must match the length of CLUST.");
    }
    let rows: Vec<Vec<f64>> = (0..n).map(|i| (0..p).map(|j| x.data[j * n + i]).collect()).collect();
    let mut dist: Box<dyn FnMut(&mut Interpreter, usize, usize) -> Res<f64>> = match a.get(2) {
        None => Box::new({
            let rows = rows.clone();
            move |_, i, j| Ok(rows[i].iter().zip(&rows[j]).map(|(a, b)| (a - b).powi(2)).sum())
        }),
        Some(Value::Func(f)) => {
            let f = f.clone();
            let rows = rows.clone();
            let extra: Vec<Value> = a[3..].to_vec();
            Box::new(move |it, i, j| {
                let mut args = vec![Value::Num(Matrix::from_dims(vec![1, p], rows[i].clone())), Value::Num(Matrix::from_dims(vec![1, p], rows[j].clone()))];
                args.extend(extra.iter().cloned());
                let r = it.call_value(&f, args, 1)?;
                r[0].scalar()
            })
        }
        Some(v) => {
            let name = v.text().unwrap_or_default().to_lowercase();
            let rows = rows.clone();
            let f: fn(&[f64], &[f64]) -> f64 = match name.as_str() {
                "euclidean" => |a, b| a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt(),
                "sqeuclidean" => |a, b| a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f64>(),
                "cityblock" => |a, b| a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f64>(),
                "hamming" => |a, b| a.iter().zip(b).filter(|(x, y)| x != y).count() as f64 / a.len() as f64,
                "jaccard" => |a, b| {
                    let nz = a.iter().zip(b).filter(|(x, y)| **x != 0.0 || **y != 0.0).count() as f64;
                    if nz == 0.0 { 0.0 } else { a.iter().zip(b).filter(|(x, y)| (**x != 0.0 || **y != 0.0) && x != y).count() as f64 / nz }
                },
                "cosine" => |a, b| {
                    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
                    1.0 - dot / (a.iter().map(|x| x * x).sum::<f64>().sqrt() * b.iter().map(|x| x * x).sum::<f64>().sqrt())
                },
                "correlation" => |a, b| {
                    let (ma, mb) = (a.iter().sum::<f64>() / a.len() as f64, b.iter().sum::<f64>() / b.len() as f64);
                    let dot: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
                    1.0 - dot / (a.iter().map(|x| (x - ma).powi(2)).sum::<f64>().sqrt() * b.iter().map(|y| (y - mb).powi(2)).sum::<f64>().sqrt())
                },
                _ => return rt(format!("The distance function '{}' was not found.", v.text().unwrap_or_default())),
            };
            Box::new(move |_, i, j| Ok(f(&rows[i], &rows[j])))
        }
    };
    let valid: Vec<usize> = (0..n).filter(|&i| clust[i].is_some()).collect();
    let k = valid.iter().map(|&i| clust[i].unwrap()).max().map(|m| m + 1).unwrap_or(0);
    let mut s = vec![f64::NAN; n];
    for &i in &valid {
        let ci = clust[i].unwrap();
        let (mut sum, mut cnt) = (vec![0.0; k], vec![0usize; k]);
        for &j in &valid {
            if j == i {
                continue;
            }
            let cj = clust[j].unwrap();
            sum[cj] += dist(it, i, j)?;
            cnt[cj] += 1;
        }
        let own = cnt[ci];
        if own == 0 {
            s[i] = 1.0;
            continue;
        }
        let av = sum[ci] / own as f64;
        let mut b = f64::INFINITY;
        for c in 0..k {
            if c != ci && cnt[c] > 0 {
                b = b.min(sum[c] / cnt[c] as f64);
            }
        }
        s[i] = if b.is_infinite() { f64::NAN } else { (b - av) / av.max(b) };
    }
    Ok(Some(vec![Value::Num(finish(Matrix::from_dims(vec![n, 1], s), single))]))
}

fn linhyptest(it: &mut Interpreter, a: &[Value]) -> Ret {
    if a.len() < 4 {
        return rt("At least four arguments are required.");
    }
    let (Value::Num(b), Value::Num(cov), Value::Num(c), Value::Num(h)) = (&a[0], &a[1], &a[2], &a[3]) else { return rt("Inputs must be numeric.") };
    let dfe = match a.get(4) {
        None => f64::INFINITY,
        Some(v) => {
            let d = v.scalar()?;
            if !(d > 0.0) {
                return rt("DFE must be a positive scalar.");
            }
            d
        }
    };
    let (pp, rr) = (b.numel(), c.numel());
    if h.dims.len() != 2 || h.dims[0] != rr || h.dims[1] != pp {
        return rt(format!("H must be a {rr}-by-{pp} numeric matrix."));
    }
    let hm: Vec<Vec<f64>> = (0..rr).map(|i| (0..pp).map(|j| h.data[j * rr + i]).collect()).collect();
    let cv: Vec<Vec<f64>> = (0..pp).map(|i| (0..pp).map(|j| cov.data[j * cov.dims[0] + i]).collect()).collect();
    let diff: Vec<f64> = (0..rr).map(|i| (0..pp).map(|j| hm[i][j] * b.data[j]).sum::<f64>() - c.data[i]).collect();
    // M = H*COV*H'
    let hc: Vec<Vec<f64>> = (0..rr).map(|i| (0..pp).map(|j| (0..pp).map(|k| hm[i][k] * cv[k][j]).sum()).collect()).collect();
    let md: Vec<f64> = (0..rr * rr).map(|idx| { let (i, j) = (idx % rr, idx / rr); (0..pp).map(|k| hc[i][k] * hm[j][k]).sum() }).collect();
    let sol = it.call_function("mldivide", vec![Value::Num(Matrix::from_dims(vec![rr, rr], md)), Value::Num(Matrix::from_dims(vec![rr, 1], diff.clone()))], 1)?;
    let Value::Num(sv) = &sol[0] else { return rt("solve failed") };
    let w: f64 = diff.iter().zip(&sv.data).map(|(a, b)| a * b).sum();
    let rk = it.call_function("rank", vec![Value::Num(Matrix::from_dims(vec![rr, pp], h.data.clone()))], 1)?[0].scalar()?;
    let (t, p) = if dfe.is_infinite() {
        (w, 1.0 - cdf(it, "chi2cdf", w, &[rk])?)
    } else {
        let f = w / rk;
        (f, 1.0 - cdf(it, "fcdf", f, &[rk, dfe])?)
    };
    Ok(Some(vec![Value::num(p), Value::num(t), Value::num(rk)]))
}

fn glmval(it: &mut Interpreter, a: &[Value]) -> Ret {
    if a.len() < 3 {
        return rt("At least three arguments are required.");
    }
    let (Value::Num(beta), Value::Num(x)) = (&a[0], &a[1]) else { return rt("Inputs must be numeric.") };
    let single = is_single(beta) || is_single(x);
    let link = &a[2];
    let mut k = 3;
    let mut stats: Option<&Value> = None;
    if let Some(Value::Struct(_)) = a.get(k) {
        stats = a.get(k);
        k += 1;
        if matches!(a.get(k), Some(Value::Num(m)) if m.kind != Kind::Char) {
            k += 1;
        }
    }
    let mut constant = true;
    let mut size: Option<f64> = None;
    let mut offset: Option<Matrix> = None;
    for pair in a[k..].chunks(2) {
        if pair.len() < 2 {
            return rt("Invalid parameter name/value pair.");
        }
        let key = pair[0].text().unwrap_or_default();
        if matches_name(&key, "constant") {
            constant = pair[1].text().unwrap_or_default().eq_ignore_ascii_case("on");
        } else if matches_name(&key, "size") {
            size = Some(pair[1].scalar()?);
        } else if matches_name(&key, "offset") {
            offset = Some(pair[1].as_matrix()?.clone());
        } else {
            return rt(format!("Invalid parameter name: {key}."));
        }
    }
    let (n, p) = (x.dims[0], x.dims[1]);
    let q = p + constant as usize;
    if beta.numel() != q || beta.dims.iter().filter(|&&d| d > 1).count() > 1 {
        return rt("Incorrect dimensions for matrix multiplication. Check that the number of columns in the first matrix matches the number of rows in the second matrix. To operate on each element of the matrix individually, use TIMES (.*) for elementwise multiplication.");
    }
    let mut eta: Vec<f64> = (0..n).map(|i| (0..p).map(|j| x.data[j * n + i] * beta.data[j + constant as usize]).sum::<f64>() + if constant { beta.data[0] } else { 0.0 }).collect();
    if let Some(o) = &offset {
        for (e, ov) in eta.iter_mut().zip(&o.data) {
            *e += ov;
        }
    }
    let inv: Box<dyn Fn(f64) -> f64> = match link {
        v if v.is_text() => {
            let name = v.text()?.to_lowercase();
            match name.as_str() {
                "identity" => Box::new(|e| e),
                "log" => Box::new(|e| e.exp()),
                "logit" => Box::new(|e| 1.0 / (1.0 + (-e).exp())),
                "probit" => Box::new(|e| super::dist::normcdf_std(e)),
                "comploglog" => Box::new(|e| 1.0 - (-e.exp()).exp()),
                "loglog" => Box::new(|e| (-e.exp()).exp()),
                "reciprocal" => Box::new(|e| 1.0 / e),
                _ => return rt("Unrecognized link function name."),
            }
        }
        Value::Cell(c) => {
            if c.data.len() != 3 {
                return rt("LINK cell array must have three components.");
            }
            let Value::Func(f) = &c.data[1] else { return rt("LINK cell array must have three components.") };
            let f = f.clone();
            let mut vals = Vec::with_capacity(n);
            for &e in &eta {
                let r = it.call_value(&f, vec![Value::num(e)], 1)?;
                vals.push(r[0].scalar()?);
            }
            let mut m = vals;
            if let Some(s) = size {
                m.iter_mut().for_each(|v| *v *= s);
            }
            return Ok(Some(vec![Value::Num(finish(Matrix::from_dims(vec![n, 1], m), single))]));
        }
        _ => return rt("Unrecognized link function name."),
    };
    if let Some(Value::Struct(s)) = stats {
        if s.get("se").is_none() {
            return rt("Unrecognized field name \"se\".");
        }
    }
    let mut mu: Vec<f64> = eta.iter().map(|&e| inv(e)).collect();
    if let Some(s) = size {
        mu.iter_mut().for_each(|v| *v *= s);
    }
    Ok(Some(vec![Value::Num(finish(Matrix::from_dims(vec![n, 1], mu), single))]))
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "fishertest" => fishertest(it, a, nargout),
        "runstest" => runstest(it, a),
        "partialcorr" => partialcorr(it, a),
        "grp2idx" => grp2idx(a),
        "confusionmat" => confusionmat(a),
        "cholcov" => cholcov(it, a, nargout),
        "jackknife" => jackknife(it, a, nargout),
        "silhouette" => silhouette(it, a),
        "linhyptest" => linhyptest(it, a),
        _ => glmval(it, a),
    }
}
