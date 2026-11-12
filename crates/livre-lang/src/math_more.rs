//! Elementary and matrix extras (`sinpi`, `cospi`, `rms`, `residue`, `matchpairs`, `pageinv`,
//! `pagenorm`, `expmv`, `combinations`, `interp1q`, `padecoef`), the state-space helpers
//! `ss2tf` and `ltitr`, kernel density estimates (`kde`), `svdappend`, `superiorfloat`,
//! `inner2outer`, `svdsketch`, small helpers (`getcolumn`, `setstr`, `vectorize`, `is2D`), and platform
//! queries (`ismac`, `ispc`, `isunix`, `isstudent`, `usejava`, `javachk`, `version`).

use crate::builtins::{arg, m_arg, Ret};
use crate::cplx::{poly_roots, C};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::nd::IndexArg;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "sinpi", "cospi", "rms", "residue", "matchpairs", "pageinv", "pagenorm", "expmv", "combinations", "interp1q", "padecoef", "ss2tf", "ltitr",
    "ismac", "ispc", "isunix", "isstudent", "usejava", "javachk", "version", "kde", "svdappend", "superiorfloat", "inner2outer", "getcolumn", "setstr",
    "vectorize", "is2D", "svdsketch",
];

const VERSION: &str = "23.2.0.2409890 (R2023b) Update 3";
const VERSION_DATE: &str = "October 4, 2023";

fn call1(it: &mut Interpreter, f: &str, args: Vec<Value>) -> R<Value> {
    it.call_function(f, args, 1)?.into_iter().next().ok_or_else(|| LangError::new(format!("{f} returned no value."), 0))
}

fn cvec(m: &Matrix) -> Vec<C> {
    (0..m.numel()).map(|i| C::new(m.data[i], m.im_at(i))).collect()
}

fn cvalue(dims: Vec<usize>, z: &[C]) -> Value {
    Value::Num(Matrix::complex(dims, z.iter().map(|c| c.re).collect(), z.iter().map(|c| c.im).collect()))
}

fn col_value(z: &[C]) -> Value {
    if z.is_empty() {
        return Value::empty();
    }
    cvalue(vec![z.len(), 1], z)
}

fn row_value(z: &[C]) -> Value {
    if z.is_empty() {
        return Value::empty();
    }
    cvalue(vec![1, z.len()], z)
}

// ---- sinpi, cospi, rms ---------------------------------------------------------------------

fn sinpi(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    let r = x % 2.0;
    if r.fract() == 0.0 {
        return 0.0f64.copysign(x);
    }
    match r {
        0.5 | -1.5 => 1.0,
        1.5 | -0.5 => -1.0,
        _ => (std::f64::consts::PI * r).sin(),
    }
}

fn cospi(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    match x.abs() % 2.0 {
        0.0 => 1.0,
        1.0 => -1.0,
        0.5 | 1.5 => 0.0,
        r => (std::f64::consts::PI * r).cos(),
    }
}

fn trig_pi(it: &mut Interpreter, name: &str, a: &[Value]) -> R<Value> {
    let m = m_arg(a, 0, name)?;
    if !m.kind.is_float() {
        return rt(format!("Undefined function '{name}' for input arguments of type '{}'.", arg(a, 0, name)?.class_name()));
    }
    if m.is_complex() {
        let z: Vec<C> = cvec(&m).iter().map(|c| C::new(c.re * std::f64::consts::PI, c.im * std::f64::consts::PI)).collect();
        let v = cvalue(m.dims.clone(), &z);
        let out = call1(it, if name == "sinpi" { "sin" } else { "cos" }, vec![v])?;
        return Ok(match out {
            Value::Num(o) => Value::Num(o.cast_to(m.kind)),
            o => o,
        });
    }
    let f = if name == "sinpi" { sinpi } else { cospi };
    let data = m.data.iter().map(|&x| f(x)).collect();
    Ok(Value::Num(Matrix::from_dims(m.dims.clone(), data).cast_to(m.kind)))
}

fn rms(it: &mut Interpreter, a: &[Value]) -> R<Value> {
    let x = arg(a, 0, "rms")?;
    let Value::Num(m) = x else {
        return rt(format!("Undefined function 'real' for input arguments of type '{}'.", x.class_name()));
    };
    let kind = if m.kind == Kind::Single { Kind::Single } else { Kind::Double };
    let sq: Vec<f64> = (0..m.numel()).map(|i| m.data[i] * m.data[i] + m.im_at(i) * m.im_at(i)).collect();
    let mut args = vec![Value::Num(Matrix::from_dims(m.dims.clone(), sq).cast_to(kind))];
    args.extend_from_slice(&a[1..]);
    match call1(it, "mean", args)? {
        Value::Num(mut r) => {
            r.data.iter_mut().for_each(|v| *v = v.sqrt());
            Ok(Value::Num(r.cast_to(kind)))
        }
        o => Ok(o),
    }
}

// ---- residue -------------------------------------------------------------------------------

fn strip_leading(p: &[C]) -> Vec<C> {
    let k = p.iter().position(|c| c.re != 0.0 || c.im != 0.0).unwrap_or(p.len());
    p[k..].to_vec()
}

/// `num / den` for polynomials in descending powers: quotient and remainder.
pub(crate) fn poly_div(num: &[C], den: &[C]) -> (Vec<C>, Vec<C>) {
    let (n, d) = (num.len(), den.len());
    if n < d {
        return (Vec::new(), num.to_vec());
    }
    let mut r = num.to_vec();
    let mut q = vec![C::new(0.0, 0.0); n - d + 1];
    for i in 0..q.len() {
        let c = r[i].div(den[0]);
        q[i] = c;
        for j in 0..d {
            r[i + j] = r[i + j].sub(c.mul(den[j]));
        }
    }
    (q, r[n - d + 1..].to_vec())
}

fn poly_mul(a: &[C], b: &[C]) -> Vec<C> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut out = vec![C::new(0.0, 0.0); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            out[i + j] = out[i + j].add(x.mul(*y));
        }
    }
    out
}

fn poly_from_roots(r: &[C]) -> Vec<C> {
    r.iter().fold(vec![C::new(1.0, 0.0)], |acc, z| poly_mul(&acc, &[C::new(1.0, 0.0), C::new(-z.re, -z.im)]))
}

/// Taylor coefficients of a polynomial about `x0`, constant term first.
pub(crate) fn taylor(p: &[C], x0: C, count: usize) -> Vec<C> {
    let mut cur = p.to_vec();
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if cur.is_empty() {
            out.push(C::new(0.0, 0.0));
            continue;
        }
        // Synthetic division by (s - x0): the remainder is the value at x0.
        let mut q = Vec::with_capacity(cur.len().saturating_sub(1));
        let mut acc = C::new(0.0, 0.0);
        for (i, c) in cur.iter().enumerate() {
            acc = acc.mul(x0).add(*c);
            if i + 1 < cur.len() {
                q.push(acc);
            }
        }
        out.push(acc);
        cur = q;
    }
    out
}

/// Poles grouped by multiplicity, as `mpoles` does: largest magnitude first, poles within a
/// relative tolerance of 0.001 merged into their mean.
pub(crate) fn group_poles(p: &[C]) -> Vec<(C, usize)> {
    let mut order: Vec<usize> = (0..p.len()).collect();
    order.sort_by(|&i, &j| p[j].abs().partial_cmp(&p[i].abs()).unwrap_or(std::cmp::Ordering::Equal));
    let mut used = vec![false; p.len()];
    let mut groups = Vec::new();
    for &i in &order {
        if used[i] {
            continue;
        }
        let tol = 0.001 * p[i].abs().max(1.0);
        let members: Vec<usize> = order.iter().copied().filter(|&j| !used[j] && p[j].sub(p[i]).abs() < tol).collect();
        let mut sum = C::new(0.0, 0.0);
        for &j in &members {
            used[j] = true;
            sum = sum.add(p[j]);
        }
        let n = members.len() as f64;
        groups.push((C::new(sum.re / n, sum.im / n), members.len()));
    }
    groups
}

fn poly_deriv(p: &[C]) -> Vec<C> {
    let n = p.len().saturating_sub(1);
    p[..n].iter().enumerate().map(|(i, c)| C::new(c.re * (n - i) as f64, c.im * (n - i) as f64)).collect()
}

fn poly_eval(p: &[C], x: C) -> C {
    p.iter().fold(C::new(0.0, 0.0), |acc, c| acc.mul(x).add(*c))
}

/// A pole of multiplicity `m` is a simple root of the (m-1)-th derivative, which pins it down
/// far better than the mean of the clustered roots.
pub(crate) fn refine_pole(den: &[C], p0: C, m: usize) -> C {
    let mut d = den.to_vec();
    for _ in 1..m {
        d = poly_deriv(&d);
    }
    let dd = poly_deriv(&d);
    let mut p = p0;
    for _ in 0..50 {
        let (f, g) = (poly_eval(&d, p), poly_eval(&dd, p));
        if g.abs() == 0.0 {
            break;
        }
        let next = p.sub(f.div(g));
        if next == p {
            break;
        }
        p = next;
    }
    if p.sub(p0).abs() < 0.001 * p0.abs().max(1.0) { p } else { p0 }
}

fn residue(a: &[Value], nargout: usize) -> R<Vec<Value>> {
    if a.len() >= 3 {
        return residue_inverse(a);
    }
    let b = strip_leading(&cvec(&m_arg(a, 0, "residue")?));
    let den = strip_leading(&cvec(&m_arg(a, 1, "residue")?));
    if den.is_empty() {
        return rt("Denominator must have a nonzero coefficient.");
    }
    let lead = den[0];
    let den: Vec<C> = den.iter().map(|c| c.div(lead)).collect();
    let b: Vec<C> = b.iter().map(|c| c.div(lead)).collect();
    let (k, rem) = poly_div(&b, &den);
    let groups: Vec<(C, usize)> = group_poles(&poly_roots(&den)).into_iter().map(|(p, m)| (if m > 1 { refine_pole(&den, p, m) } else { p }, m)).collect();
    let (mut r, mut p) = (Vec::new(), Vec::new());
    for (gi, &(pole, m)) in groups.iter().enumerate() {
        let others: Vec<C> = groups.iter().enumerate().filter(|(j, _)| *j != gi).flat_map(|(_, &(q, n))| std::iter::repeat(q).take(n)).collect();
        let q = poly_from_roots(&others);
        let f = taylor(&rem, pole, m);
        let g = taylor(&q, pole, m);
        // Series of rem/q about the pole.
        let mut c: Vec<C> = Vec::with_capacity(m);
        for n in 0..m {
            let mut s = f[n];
            for i in 1..=n {
                s = s.sub(g[i].mul(c[n - i]));
            }
            c.push(s.div(g[0]));
        }
        for j in 1..=m {
            r.push(c[m - j]);
            p.push(pole);
        }
    }
    let mut out = vec![col_value(&r), col_value(&p), row_value(&k)];
    out.truncate(nargout.max(1));
    Ok(out)
}

fn residue_inverse(a: &[Value]) -> R<Vec<Value>> {
    let r = cvec(&m_arg(a, 0, "residue")?);
    let p = cvec(&m_arg(a, 1, "residue")?);
    let k = cvec(&m_arg(a, 2, "residue")?);
    if r.len() != p.len() {
        return rt("Residues and poles must have the same length.");
    }
    let den = poly_from_roots(&p);
    let n = p.len();
    let mut num = vec![C::new(0.0, 0.0); n];
    // Residues follow their poles in order; repeated poles take increasing powers.
    let mut i = 0;
    while i < n {
        let mut m = 1;
        while i + m < n && p[i + m].sub(p[i]).abs() < 0.001 * p[i].abs().max(1.0) {
            m += 1;
        }
        for j in 1..=m {
            let mut rest: Vec<C> = p[..i].to_vec();
            rest.extend(std::iter::repeat(p[i]).take(m - j));
            rest.extend_from_slice(&p[i + m..]);
            let term = poly_from_roots(&rest);
            let off = n - term.len();
            for (t, c) in term.iter().enumerate() {
                num[off + t] = num[off + t].add(c.mul(r[i + j - 1]));
            }
        }
        i += m;
    }
    if !k.is_empty() {
        let mut kd = poly_mul(&k, &den);
        let off = kd.len() - n;
        for (t, c) in num.iter().enumerate() {
            kd[off + t] = kd[off + t].add(*c);
        }
        num = kd;
    }
    let real = r.iter().chain(&p).chain(&k).all(|c| c.im == 0.0) || num.iter().chain(&den).all(|c| c.im.abs() <= 1e-10 * c.abs().max(1.0));
    let fix = |v: Vec<C>| if real { v.into_iter().map(|c| C::new(c.re, 0.0)).collect() } else { v };
    Ok(vec![row_value(&fix(num)), row_value(&fix(den))])
}

// ---- matchpairs ----------------------------------------------------------------------------

/// Minimum-cost perfect assignment of a square matrix (shortest augmenting paths). Returns the
/// column assigned to each row.
fn assign(cost: &[Vec<f64>]) -> Vec<usize> {
    let n = cost.len();
    let (mut u, mut v) = (vec![0.0; n + 1], vec![0.0; n + 1]);
    let (mut p, mut way) = (vec![0usize; n + 1], vec![0usize; n + 1]);
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![f64::INFINITY; n + 1];
        let mut used = vec![false; n + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let (mut delta, mut j1) = (f64::INFINITY, 0);
            for j in 1..=n {
                if !used[j] {
                    let cur = cost[i0 - 1][j - 1] - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=n {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut row_to_col = vec![0; n];
    for j in 1..=n {
        if p[j] > 0 {
            row_to_col[p[j] - 1] = j - 1;
        }
    }
    row_to_col
}

fn matchpairs(a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let c = m_arg(a, 0, "matchpairs")?;
    let unmatched = arg(a, 1, "matchpairs")?.scalar()?;
    if c.dims.len() > 2 {
        return rt("Cost must be a 2-D matrix.");
    }
    if c.data.iter().any(|x| x.is_nan()) {
        return rt("Cost must not contain NaN.");
    }
    let maximize = match a.get(2) {
        Some(v) => match v.text()?.to_lowercase().as_str() {
            "max" => true,
            "min" => false,
            _ => return rt("Goal must be 'min' or 'max'."),
        },
        None => false,
    };
    let (m, n) = (c.rows(), c.cols());
    let sign = if maximize { -1.0 } else { 1.0 };
    let unmatched = sign * unmatched;
    let finite: f64 = c.data.iter().filter(|x| x.is_finite()).map(|x| x.abs()).sum::<f64>() + unmatched.abs();
    let big = (finite + 1.0) * (m + n + 1) as f64;
    let at = |i: usize, j: usize| {
        let x = sign * c.data[j * m + i];
        if x == f64::INFINITY { big } else if x == f64::NEG_INFINITY { -big } else { x }
    };
    // Rows and columns can each go unmatched through a dummy partner costing `unmatched`.
    let size = m + n;
    let mut cost = vec![vec![big; size]; size];
    for i in 0..m {
        for j in 0..n {
            cost[i][j] = at(i, j);
        }
        cost[i][n + i] = unmatched;
    }
    for j in 0..n {
        cost[m + j][j] = unmatched;
        for i in 0..m {
            cost[m + j][n + i] = 0.0;
        }
    }
    let rc = assign(&cost);
    let mut pairs: Vec<(usize, usize)> = (0..m).filter(|&i| rc[i] < n && cost[i][rc[i]] < big).map(|i| (i, rc[i])).collect();
    pairs.sort_by_key(|&(i, j)| (j, i));
    let mut row_used = vec![false; m];
    let mut col_used = vec![false; n];
    for &(i, j) in &pairs {
        row_used[i] = true;
        col_used[j] = true;
    }
    let k = pairs.len();
    let mut md = vec![0.0; 2 * k];
    for (t, &(i, j)) in pairs.iter().enumerate() {
        md[t] = (i + 1) as f64;
        md[k + t] = (j + 1) as f64;
    }
    let free = |used: &[bool]| {
        let v: Vec<f64> = used.iter().enumerate().filter(|(_, u)| !**u).map(|(i, _)| (i + 1) as f64).collect();
        Value::Num(Matrix::from_dims(vec![v.len(), 1], v))
    };
    let mut out = vec![Value::Num(Matrix::from_dims(vec![k, 2], md)), free(&row_used), free(&col_used)];
    out.truncate(nargout.max(1));
    Ok(out)
}

// ---- pages ---------------------------------------------------------------------------------

fn page_of(m: &Matrix, p: usize) -> Matrix {
    let (r, c) = (m.dims[0], m.dims.get(1).copied().unwrap_or(1));
    let off = p * r * c;
    let re = m.data[off..off + r * c].to_vec();
    let mut out = Matrix::new(r, c, re);
    if let Some(im) = &m.im {
        out.im = Some(im[off..off + r * c].to_vec());
    }
    out.kind = m.kind;
    out
}

fn stack(pages: Vec<Matrix>, rest: &[usize], kind: Kind) -> Value {
    let (r, c) = pages.first().map(|p| (p.rows(), p.cols())).unwrap_or((0, 0));
    let mut dims = vec![r, c];
    dims.extend_from_slice(rest);
    let re: Vec<f64> = pages.iter().flat_map(|p| p.data.iter().copied()).collect();
    let out = if pages.iter().any(|p| p.is_complex()) {
        let im: Vec<f64> = pages.iter().flat_map(|p| (0..p.numel()).map(|i| p.im_at(i)).collect::<Vec<_>>()).collect();
        Matrix::complex(dims, re, im)
    } else {
        Matrix::from_dims(dims, re)
    };
    Value::Num(out.cast_to(kind))
}

fn page_apply(it: &mut Interpreter, name: &str, a: &[Value]) -> R<Value> {
    let x = m_arg(a, 0, name)?;
    let rest: Vec<usize> = x.dims.iter().skip(2).copied().collect();
    let np: usize = rest.iter().product();
    let mut out = Vec::with_capacity(np);
    if name == "pageinv" && x.dims[0] != x.dims.get(1).copied().unwrap_or(1) {
        return rt("Pages of the input array must be square.");
    }
    for p in 0..np {
        let page = page_of(&x, p);
        let r = if name == "pageinv" {
            // Singular pages come back as Inf, without the warning inv gives.
            crate::ops::inv_any(&page).unwrap_or_else(|_| {
                let n = page.rows();
                Matrix::from_dims(vec![n, n], vec![f64::INFINITY; n * n])
            })
        } else {
            let mut args = vec![Value::Num(page)];
            args.extend_from_slice(&a[1..]);
            call1(it, "norm", args)?.to_matrix()?
        };
        out.push(r);
    }
    Ok(stack(out, &rest, if x.kind == Kind::Single { Kind::Single } else { Kind::Double }))
}

// ---- combinations, interp1q, padecoef, ss2tf, ltitr ----------------------------------------

fn combinations(a: &[Value]) -> R<Value> {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let sizes: Vec<usize> = a.iter().map(|v| v.numel()).collect();
    let total: usize = sizes.iter().product();
    let mut vars = Vec::with_capacity(a.len());
    // The first input varies slowest.
    let mut inner = total;
    for (v, &n) in a.iter().zip(&sizes) {
        inner /= n.max(1);
        let ix: Vec<usize> = (0..total).map(|r| if n == 0 { 0 } else { (r / inner.max(1)) % n }).collect();
        let column = crate::indexing::index(v, &[IndexArg::All])?;
        vars.push(crate::indexing::index(&column, &[IndexArg::List { ix, dims: vec![total, 1], from_mask: false }])?);
    }
    let names = (1..=a.len()).map(|i| format!("Var{i}")).collect();
    Ok(Value::Table(Box::new(crate::table::Table::new(names, vars)?)))
}

fn interp1q(a: &[Value]) -> R<Value> {
    let x = m_arg(a, 0, "interp1q")?;
    let y = m_arg(a, 1, "interp1q")?;
    let xi = m_arg(a, 2, "interp1q")?;
    let n = x.numel();
    let ny = if y.rows() == n { y.cols() } else { 1 };
    if y.numel() != n * ny {
        return rt("Y must have the same number of rows as X.");
    }
    let ni = xi.numel();
    let mut out = vec![f64::NAN; ni * ny];
    for (k, &q) in xi.data.iter().enumerate() {
        if n == 0 || q.is_nan() || q < x.data[0] || q > x.data[n - 1] {
            continue;
        }
        let j = x.data.partition_point(|&v| v <= q).clamp(1, n.max(2) - 1) - 1;
        for c in 0..ny {
            out[c * ni + k] = if n == 1 {
                y.data[c * n]
            } else if q == x.data[n - 1] {
                y.data[c * n + n - 1]
            } else {
                let t = (q - x.data[j]) / (x.data[j + 1] - x.data[j]);
                y.data[c * n + j] + t * (y.data[c * n + j + 1] - y.data[c * n + j])
            };
        }
    }
    Ok(Value::Num(Matrix::from_dims(vec![ni, ny], out)))
}

fn padecoef(a: &[Value]) -> R<Vec<Value>> {
    let t = arg(a, 0, "padecoef")?.scalar()?;
    let n = match a.get(1) {
        Some(v) => v.scalar()?,
        None => 1.0,
    };
    if n < 0.0 || n.fract() != 0.0 {
        return rt("N must be a nonnegative integer.");
    }
    let n = n as usize;
    // c_k = (2N-k)! N! / ((2N)! k! (N-k)!), built by its ratio c_k / c_N.
    let fact = |k: usize| (1..=k).map(|x| x as f64).product::<f64>();
    let c = |k: usize| fact(2 * n - k) * fact(n) / (fact(2 * n) * fact(k) * fact(n - k));
    let (mut num, mut den) = (Vec::with_capacity(n + 1), Vec::with_capacity(n + 1));
    for k in (0..=n).rev() {
        let mut x = c(k) / c(n);
        for _ in k..n {
            x /= t;
        }
        den.push(x);
        num.push(if k % 2 == 1 { -x } else { x });
    }
    Ok(vec![Value::Num(Matrix::row(num)), Value::Num(Matrix::row(den))])
}

fn ss2tf(it: &mut Interpreter, a: &[Value]) -> R<Vec<Value>> {
    let am = m_arg(a, 0, "ss2tf")?;
    let b = m_arg(a, 1, "ss2tf")?;
    let c = m_arg(a, 2, "ss2tf")?;
    let d = m_arg(a, 3, "ss2tf")?;
    let iu = match a.get(4) {
        Some(v) => v.scalar()? as usize,
        None => 1,
    };
    let n = am.rows();
    if am.cols() != n || b.rows() != n || c.cols() != n {
        return rt("The A, B, C and D matrices must have consistent dimensions.");
    }
    if iu < 1 || iu > b.cols() {
        return rt("The input index IU must be between 1 and the number of inputs.");
    }
    let den = call1(it, "poly", vec![Value::Num(am.clone())])?.to_matrix()?;
    let ny = c.rows();
    let mut num = vec![0.0; ny * (n + 1)];
    for i in 0..ny {
        // num_i = poly(A - B(:,iu) C(i,:)) + (D(i,iu) - 1) poly(A).
        let mut m = am.clone();
        for r in 0..n {
            for k in 0..n {
                m.data[k * n + r] -= b.data[(iu - 1) * n + r] * c.data[k * ny + i];
            }
        }
        let p = call1(it, "poly", vec![Value::Num(m)])?.to_matrix()?;
        let dd = if d.numel() == 0 { 0.0 } else { d.data[(iu - 1) * d.rows() + i] };
        for k in 0..=n {
            num[k * ny + i] = p.data[k] + (dd - 1.0) * den.data[k];
        }
    }
    Ok(vec![Value::Num(Matrix::from_dims(vec![ny, n + 1], num)), Value::Num(den)])
}

fn ltitr(a: &[Value]) -> R<Value> {
    let am = m_arg(a, 0, "ltitr")?;
    let b = m_arg(a, 1, "ltitr")?;
    let u = m_arg(a, 2, "ltitr")?;
    let n = am.rows();
    let (steps, nu) = (u.rows(), u.cols());
    if am.cols() != n || b.rows() != n || b.cols() != nu {
        return rt("The A, B and U matrices must have consistent dimensions.");
    }
    let mut x = match a.get(3) {
        Some(v) => {
            let x0 = v.to_matrix()?;
            if x0.numel() != n {
                return rt("X0 must have one element per state.");
            }
            x0.data
        }
        None => vec![0.0; n],
    };
    let mut out = vec![0.0; steps * n];
    for k in 0..steps {
        for s in 0..n {
            out[s * steps + k] = x[s];
        }
        let next: Vec<f64> = (0..n)
            .map(|r| (0..n).map(|j| am.data[j * n + r] * x[j]).sum::<f64>() + (0..nu).map(|j| b.data[j * n + r] * u.data[j * steps + k]).sum::<f64>())
            .collect();
        x = next;
    }
    Ok(Value::Num(Matrix::from_dims(vec![steps, n], out)))
}

// ---- kde, svdappend, superiorfloat, inner2outer ---------------------------------------

/// Name-value options after the positional arguments, names lowercased.
fn options<'a>(a: &'a [Value], name: &str) -> R<Vec<(String, &'a Value)>> {
    if a.len() % 2 != 0 {
        return rt(format!("Name-value arguments of {name} must come in pairs."));
    }
    a.chunks(2).map(|p| Ok((p[0].text()?.to_lowercase(), &p[1]))).collect()
}

fn median_of(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let m = v.len();
    if m == 0 {
        f64::NAN
    } else if m % 2 == 1 {
        v[m / 2]
    } else {
        (v[m / 2 - 1] + v[m / 2]) / 2.0
    }
}

fn kde(a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let x = m_arg(a, 0, "kde")?;
    let column = x.cols() == 1 && x.rows() > 1;
    let (mut weight, mut points, mut num_points, mut bw, mut kernel, mut cdf) = (None, None, 100usize, None, "normal".to_string(), false);
    for (k, v) in options(&a[1..], "kde")? {
        match k.as_str() {
            "evaluationpoints" => points = Some(v.to_matrix()?),
            "numpoints" => num_points = v.scalar()? as usize,
            "bandwidth" => bw = Some(v.scalar()?),
            "weight" => weight = Some(v.to_matrix()?.data),
            "kernel" => {
                kernel = v.text()?.to_lowercase();
                if !["normal", "box", "triangle", "epanechnikov"].contains(&kernel.as_str()) {
                    return rt(format!("Expected Kernel to match one of these values:\n'normal', 'box', 'triangle', 'epanechnikov'\nThe input, '{}', did not match any of the valid values.", v.text()?));
                }
            }
            "probabilityfcn" => match v.text()?.to_lowercase().as_str() {
                "pdf" => cdf = false,
                "cdf" => cdf = true,
                _ => return rt(format!("Expected ProbabilityFcn to match one of these values:\n'pdf', 'cdf'\nThe input, '{}', did not match any of the valid values.", v.text()?)),
            },
            _ => return rt(format!("'{k}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function.")),
        }
    }
    let w_all = weight.unwrap_or_else(|| vec![1.0; x.numel()]);
    if w_all.len() != x.numel() {
        return rt("The weights must have one element per data value.");
    }
    let (data, w): (Vec<f64>, Vec<f64>) = x.data.iter().zip(&w_all).filter(|(v, w)| !v.is_nan() && !w.is_nan()).map(|(v, w)| (*v, *w)).unzip();
    let n = data.len();
    // The normal reference rule on the median absolute deviation; weights do not enter it.
    let bw = bw.unwrap_or_else(|| {
        let med = median_of(data.clone());
        let mad = median_of(data.iter().map(|v| (v - med).abs()).collect());
        mad / 0.6745 * (4.0 / (3.0 * n as f64)).powf(0.2)
    });
    let (lo, hi) = data.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &v| (l.min(v), h.max(v)));
    let xs = match points {
        Some(p) => p,
        None => {
            // From three bandwidths below the data to three above.
            let (a0, b0, k) = (lo - 3.0 * bw, hi + 3.0 * bw, num_points);
            let v: Vec<f64> = (0..k).map(|i| if i + 1 == k { b0 } else { a0 + (b0 - a0) * i as f64 / (k - 1) as f64 }).collect();
            Matrix::from_dims(if column { vec![k, 1] } else { vec![1, k] }, v)
        }
    };
    let (s3, s5, s6) = (3f64.sqrt(), 5f64.sqrt(), 6f64.sqrt());
    // Kernels with unit variance, as MATLAB scales them.
    let pdf = |u: f64| match kernel.as_str() {
        "box" => if u.abs() <= s3 { 0.5 / s3 } else { 0.0 },
        "triangle" => if u.abs() < s6 { (1.0 - u.abs() / s6) / s6 } else { 0.0 },
        "epanechnikov" => if u.abs() < s5 { 0.75 * (1.0 - u * u / 5.0) / s5 } else { 0.0 },
        _ => (-0.5 * u * u).exp() / (2.0 * std::f64::consts::PI).sqrt(),
    };
    let cdf_of = |u: f64| match kernel.as_str() {
        "box" => ((u + s3) / (2.0 * s3)).clamp(0.0, 1.0),
        "triangle" => {
            let t = (u / s6).clamp(-1.0, 1.0);
            if t <= 0.0 { 0.5 * (1.0 + t) * (1.0 + t) } else { 1.0 - 0.5 * (1.0 - t) * (1.0 - t) }
        }
        "epanechnikov" => {
            let t = (u / s5).clamp(-1.0, 1.0);
            0.25 * (2.0 + 3.0 * t - t * t * t)
        }
        _ => 0.5 * crate::builtins_specfun::erfc(-u / std::f64::consts::SQRT_2),
    };
    let total: f64 = w.iter().sum();
    let f: Vec<f64> = xs
        .data
        .iter()
        .map(|&q| data.iter().zip(&w).map(|(xi, wi)| wi * if cdf { cdf_of((q - xi) / bw) } else { pdf((q - xi) / bw) / bw }).sum::<f64>() / total)
        .collect();
    let mut out = vec![Value::Num(Matrix::from_dims(xs.dims.clone(), f)), Value::Num(xs), Value::num(bw)];
    out.truncate(nargout.max(1));
    Ok(out)
}

/// `svdappend`: the economy SVD of `U*diag(S)*V'` with columns (or rows) `D` appended.
fn svdappend(it: &mut Interpreter, a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let u = arg(a, 0, "svdappend")?.clone();
    let s = m_arg(a, 1, "svdappend")?;
    let v = arg(a, 2, "svdappend")?.clone();
    let d = arg(a, 3, "svdappend")?.clone();
    let mut rows = false;
    for (k, val) in options(&a[4..], "svdappend")? {
        match k.as_str() {
            "shape" => rows = val.text()?.to_lowercase() == "rows",
            _ => return rt(format!("'{k}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function.")),
        }
    }
    let sm = if s.is_vector() || s.is_scalar() { call1(it, "diag", vec![Value::Num(s)])? } else { Value::Num(s) };
    let vt = call1(it, "ctranspose", vec![v.clone()])?;
    let us = call1(it, "mtimes", vec![u, sm])?;
    let base = call1(it, "mtimes", vec![us, vt])?;
    let full = if rows {
        if d.dims().get(1).copied().unwrap_or(1) != v.dims()[0] {
            return rt("When \"rows\" is specified, the fourth argument must have as many columns as the number of rows in the third argument.");
        }
        crate::indexing::vconcat(vec![base, d])?
    } else {
        if d.dims()[0] != base.dims()[0] {
            return rt("When \"columns\" is specified, the fourth argument must have as many rows as the first argument.");
        }
        crate::indexing::hconcat(vec![base, d])?
    };
    let mut r = it.call_function("svd", vec![full, Value::chars("econ")], 3)?;
    let sv = call1(it, "diag", vec![r.remove(1)])?;
    let mut out = vec![r.remove(0), sv, r.remove(0)];
    if nargout <= 1 {
        return Ok(vec![out.remove(1)]);
    }
    out.truncate(nargout);
    Ok(out)
}

fn superiorfloat(a: &[Value]) -> R<Value> {
    let mut single = false;
    for v in a {
        match v {
            Value::Num(m) if m.kind == Kind::Single => single = true,
            Value::Num(m) if !matches!(m.kind, Kind::Int(_)) => {}
            _ => return rt("Inputs must be floats, namely single or double."),
        }
    }
    Ok(Value::chars(if single { "single" } else { "double" }))
}

/// `inner2outer`: a table of tables regrouped by the inner variable names.
fn inner2outer(a: &[Value]) -> R<Value> {
    let Value::Table(t) = arg(a, 0, "inner2outer")? else {
        return rt("Incorrect number or types of inputs or outputs for function inner2outer.");
    };
    let mut inner: Vec<(String, Vec<String>, Vec<Value>)> = Vec::new();
    for (outer, v) in t.names.iter().zip(&t.vars) {
        let Value::Table(sub) = v else {
            return rt("All variables of the input table must be tables or timetables.");
        };
        for (n, col) in sub.names.iter().zip(&sub.vars) {
            match inner.iter_mut().find(|e| &e.0 == n) {
                Some(e) => {
                    e.1.push(outer.clone());
                    e.2.push(col.clone());
                }
                None => inner.push((n.clone(), vec![outer.clone()], vec![col.clone()])),
            }
        }
    }
    let (mut names, mut vars) = (Vec::new(), Vec::new());
    for (n, outers, cols) in inner {
        names.push(n);
        vars.push(Value::Table(Box::new(crate::table::Table::new(outers, cols)?)));
    }
    let mut out = (**t).clone();
    out.names = names;
    out.vars = vars;
    Ok(Value::Table(Box::new(out)))
}

/// `vectorize`: the elementwise forms of `*`, `/`, `\` and `^`.
pub(crate) fn vectorize(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev = '\0';
    for c in s.chars() {
        if matches!(c, '*' | '/' | '\\' | '^') && prev != '.' {
            out.push('.');
        }
        out.push(c);
        prev = c;
    }
    out
}

// ---- platform ------------------------------------------------------------------------------

fn javachk(a: &[Value]) -> R<Value> {
    arg(a, 0, "javachk")?.text()?;
    let (subject, id) = match a.get(1) {
        Some(v) => (v.text()?, "MATLAB:javachk:featureNotAvailable"),
        None => ("This feature".to_string(), "MATLAB:javachk:thisFeatureNotAvailable"),
    };
    let mut s = StructArray::scalar();
    s.fields = vec!["message".into(), "identifier".into()];
    s.elems = vec![vec![Value::chars(&format!("{subject} is not supported because:\nJava is not currently available.")), Value::chars(id)]];
    Ok(Value::Struct(s))
}

fn version(a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let text = match a.first() {
        None => VERSION.to_string(),
        Some(v) => match v.text()?.to_lowercase().as_str() {
            "-date" => VERSION_DATE.into(),
            "-description" => "Update 3".into(),
            "-release" => "2023b".into(),
            "-java" => "Java is not enabled".into(),
            _ => return rt("Invalid option passed to 'version' command"),
        },
    };
    let mut out = vec![Value::chars(&text), Value::chars(VERSION_DATE)];
    out.truncate(nargout.max(1));
    Ok(out)
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    let outs = match name {
        "sinpi" | "cospi" => vec![trig_pi(it, name, a)?],
        "rms" => vec![rms(it, a)?],
        "residue" => residue(a, nargout)?,
        "matchpairs" => matchpairs(a, nargout)?,
        "pageinv" | "pagenorm" => vec![page_apply(it, name, a)?],
        "expmv" => {
            let am = arg(a, 0, name)?.clone();
            let b = arg(a, 1, name)?.clone();
            let scaled = match a.get(2) {
                Some(t) => call1(it, "times", vec![t.clone(), am])?,
                None => am,
            };
            let e = call1(it, "expm", vec![scaled])?;
            vec![call1(it, "mtimes", vec![e, b])?]
        }
        "combinations" => vec![combinations(a)?],
        "interp1q" => vec![interp1q(a)?],
        "padecoef" => padecoef(a)?,
        "ss2tf" => ss2tf(it, a)?,
        "ltitr" => vec![ltitr(a)?],
        "ismac" => vec![Value::bool(cfg!(target_os = "macos"))],
        "ispc" => vec![Value::bool(cfg!(windows))],
        "isunix" => vec![Value::bool(cfg!(unix))],
        "isstudent" => vec![Value::bool(false)],
        "usejava" => {
            arg(a, 0, name)?.text()?;
            vec![Value::bool(false)]
        }
        "javachk" => vec![javachk(a)?],
        "version" => version(a, nargout)?,
        "kde" => kde(a, nargout)?,
        "svdappend" => svdappend(it, a, nargout)?,
        "svdsketch" => svdsketch(it, a, nargout)?,
        "superiorfloat" => vec![superiorfloat(a)?],
        "inner2outer" => vec![inner2outer(a)?],
        "getcolumn" => {
            let data = arg(a, 0, name)?;
            let cols = crate::indexing::to_index_arg(arg(a, 1, name)?)?;
            vec![crate::indexing::index(data, &[IndexArg::All, cols])?]
        }
        "setstr" => vec![call1(it, "char", a.to_vec())?],
        "vectorize" => {
            let v = arg(a, 0, name)?;
            let text = match v {
                Value::Func(_) => call1(it, "func2str", vec![v.clone()])?.text()?,
                _ => v.text()?,
            };
            vec![Value::chars(&vectorize(&text))]
        }
        "is2D" => {
            let view = call1(it, "get", vec![arg(a, 0, name)?.clone(), Value::chars("View")])?.to_matrix()?;
            let (az, el) = (view.data.first().copied().unwrap_or(0.0), view.data.get(1).copied().unwrap_or(90.0));
            vec![Value::bool(az % 360.0 == 0.0 && el.abs() == 90.0)]
        }
        _ => return Ok(None),
    };
    Ok(Some(outs))
}

/// `[U, S, V, apxErr] = svdsketch(A, tol, Name, Value)`: a low-rank SVD grown a block of
/// columns at a time until the relative Frobenius error ||A - U*S*V'||/||A|| is at most tol
/// (default eps^(1/4)). MATLAB grows it from random sketches; this takes the blocks from the
/// exact SVD, so each step's error is the optimal one for its rank. apxErr holds the error
/// after each block, and singular values that are zero to working precision are dropped.
fn svdsketch(it: &mut Interpreter, a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let am = m_arg(a, 0, "svdsketch")?;
    let single = am.kind == Kind::Single;
    let eps = if single { f32::EPSILON as f64 } else { f64::EPSILON };
    let (m, n) = (am.rows(), am.cols());
    let mut rest = &a[1..];
    let mut tol = eps.powf(0.25);
    if let Some(t) = rest.first().filter(|v| !v.is_text()) {
        let t = t.to_matrix()?;
        if t.numel() != 1 || t.is_complex() || !(t.data[0] >= eps.sqrt() && t.data[0] < 1.0) {
            return rt("The tolerance must be a real numeric scalar in the range sqrt(eps(class(A))) <= tolerance < 1.");
        }
        tol = t.data[0];
        rest = &rest[1..];
    }
    let small = m.min(n);
    let mut block = (0.1 * m.max(n) as f64).floor().max(5.0) as usize;
    let mut max_dim = small;
    let mut max_iter: Option<usize> = None;
    let positive = |v: &Value, what: &str| -> R<usize> {
        let x = v.scalar()?;
        if x < 1.0 || x.fract() != 0.0 {
            return rt(format!("The {what} must be a positive integer."));
        }
        Ok(x as usize)
    };
    for (k, val) in options(rest, "svdsketch")? {
        match k.to_lowercase().as_str() {
            "blocksize" => block = positive(val, "block size")?,
            "maxsubspacedimension" => max_dim = positive(val, "maximum subspace dimension")?.min(small),
            "maxiterations" => max_iter = Some(positive(val, "maximum number of iterations")?),
            "numpoweriterations" => {
                let x = val.scalar()?;
                if x < 0.0 || x.fract() != 0.0 {
                    return rt("The number of power iterations must be a nonnegative integer.");
                }
            }
            _ => return rt(format!("'{k}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function.")),
        }
    }
    let block = block.min(max_dim.max(1));
    let max_iter = max_iter.unwrap_or_else(|| max_dim.div_ceil(block.max(1)));
    let mut r = it.call_function("svd", vec![Value::Num(am.clone()), Value::chars("econ")], 3)?;
    let (u, s, v) = (r.remove(0).to_matrix()?, r.remove(0).to_matrix()?, r.remove(0).to_matrix()?);
    let sv: Vec<f64> = (0..s.rows().min(s.cols())).map(|i| s.get(i, i)).collect();
    let total: f64 = sv.iter().map(|x| x * x).sum::<f64>();
    let rel_err = |k: usize| if total == 0.0 { 0.0 } else { (sv[k.min(sv.len())..].iter().map(|x| x * x).sum::<f64>() / total).sqrt() };
    let mut k = 0;
    let mut errs = Vec::new();
    for _ in 0..max_iter {
        k = (k + block).min(max_dim);
        let e = rel_err(k);
        errs.push(e);
        if e <= tol || k >= max_dim {
            break;
        }
    }
    // Drop singular values that are zero to working precision.
    let cut = m.max(n) as f64 * eps * sv.first().copied().unwrap_or(0.0);
    let rank = sv.iter().take(k).take_while(|&&x| x > cut).count();
    let cols = |x: &Matrix, c: usize| Matrix::new(x.rows(), c, x.data[..x.rows() * c].to_vec());
    let mut sm = Matrix::new(rank, rank, vec![0.0; rank * rank]);
    for i in 0..rank {
        sm.data[i * rank + i] = sv[i];
    }
    let cast = |x: Matrix| -> R<Value> { Ok(Value::Num(if single { x.cast_to(Kind::Single) } else { x })) };
    let outs = vec![cast(cols(&u, rank))?, cast(sm)?, cast(cols(&v, rank))?, cast(Matrix::col(errs))?];
    Ok(outs.into_iter().take(nargout.max(1)).collect())
}
