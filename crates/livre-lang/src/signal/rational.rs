//! Rational-function and lattice conversions: `residuez`, `latc2tf`, `tf2latc`, `sos2cell`,
//! `cell2sos`.

use crate::builtins::Ret;
use crate::cplx::C;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::math_more::{group_poles, poly_div, refine_pole, taylor};
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["residuez", "latc2tf", "tf2latc", "sos2cell", "cell2sos", "impinvar"];

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "residuez" => residuez(a, nargout),
        "latc2tf" => latc2tf(a, nargout),
        "tf2latc" => tf2latc(a, nargout),
        "sos2cell" => sos2cell(a, nargout),
        "impinvar" => impinvar(it, a, nargout),
        _ => cell2sos(a, nargout),
    }
}

const ZERO: C = C { re: 0.0, im: 0.0 };

/// Product that stays real when both factors are, so infinities and NaNs do not leak into the
/// imaginary part.
fn cm(a: C, b: C) -> C {
    if a.im == 0.0 && b.im == 0.0 { C::new(a.re * b.re, 0.0) } else { a.mul(b) }
}

fn cd(a: C, b: C) -> C {
    if a.im == 0.0 && b.im == 0.0 { C::new(a.re / b.re, 0.0) } else { a.div(b) }
}
const ONE: C = C { re: 1.0, im: 0.0 };

fn row_c(v: &[C]) -> Value {
    shaped_c(vec![1, v.len()], v)
}

fn col_c(v: &[C]) -> Value {
    shaped_c(vec![v.len(), 1], v)
}

fn shaped_c(dims: Vec<usize>, v: &[C]) -> Value {
    let re: Vec<f64> = v.iter().map(|c| c.re).collect();
    if v.iter().all(|c| c.im == 0.0) {
        Value::Num(Matrix::from_dims(dims, re))
    } else {
        Value::Num(Matrix::complex(dims, re, v.iter().map(|c| c.im).collect()))
    }
}

fn empty00() -> Value {
    Value::Num(Matrix::from_dims(vec![0, 0], vec![]))
}

/// The argument as a float matrix, with MATLAB's wording for other classes.
fn float_matrix(a: &[Value], i: usize, label: &str, fname: &str) -> R<Matrix> {
    match &a[i] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => Ok(m.clone()),
        other => rt(format!("Input {label} in {fname} must be double/single, instead it was {}.", other.class_name())),
    }
}

fn elements(m: &Matrix) -> Vec<C> {
    (0..m.numel()).map(|k| C::new(m.data[k], m.im_at(k))).collect()
}

fn poly_mul(a: &[C], b: &[C]) -> Vec<C> {
    let mut out = vec![ZERO; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            out[i + j] = out[i + j].add(x.mul(*y));
        }
    }
    out
}

// ---- residuez ------------------------------------------------------------------------------

fn residuez(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    if a.len() == 3 {
        return residuez_inverse(a);
    }
    let b = elements(&float_matrix(a, 0, "B", "residuez")?);
    let den = elements(&float_matrix(a, 1, "A", "residuez")?);
    if den.is_empty() || (den[0].re == 0.0 && den[0].im == 0.0) {
        return rt("First coefficient in A vector must be non-zero.");
    }
    let (nb, na) = (b.len(), den.len());
    let poles = {
        let mut p = roots_matlab(&den)?;
        // Roots at the origin are placed with the other roots by size, as the grouping does.
        // Sizes equal to rounding keep their order of appearance.
        p.sort_by(|x, y| {
            let (u, v) = (x.abs(), y.abs());
            if (u - v).abs() <= 1e-12 * u.max(v) { std::cmp::Ordering::Equal } else { v.partial_cmp(&u).unwrap_or(std::cmp::Ordering::Equal) }
        });
        p
    };
    let bd: Vec<C> = b.iter().rev().copied().collect();
    let ad: Vec<C> = den.iter().rev().copied().collect();
    let (q, rem) = if nb >= na { poly_div(&bd, &ad) } else { (vec![], bd.clone()) };
    let k: Vec<C> = q.iter().rev().copied().collect();
    let groups: Vec<(C, usize)> = group_poles(&poles)
        .into_iter()
        .map(|(p, m)| (if m > 1 && p.abs() > 0.0 { refine_pole(&den, p, m) } else { p }, m))
        .collect();
    let (mut r, mut p) = (vec![], vec![]);
    for (gi, &(pole, m)) in groups.iter().enumerate() {
        if pole.re == 0.0 && pole.im == 0.0 {
            for _ in 0..m {
                r.push(C::new(f64::NAN, 0.0));
                p.push(pole);
            }
            continue;
        }
        // a(x) / (1 - pole x)^m, as a polynomial in x with descending coefficients.
        let mut asc = vec![den[0]];
        for (gj, &(q, n)) in groups.iter().enumerate() {
            if gj != gi {
                for _ in 0..n {
                    asc = poly_mul(&asc, &[ONE, C::new(-q.re, -q.im)]);
                }
            }
        }
        let qd: Vec<C> = asc.iter().rev().copied().collect();
        let x0 = ONE.div(pole);
        let f = taylor(&rem, x0, m);
        let g = taylor(&qd, x0, m);
        let mut c: Vec<C> = Vec::with_capacity(m);
        for n in 0..m {
            let mut s = f[n];
            for i in 1..=n {
                s = s.sub(g[i].mul(c[n - i]));
            }
            c.push(s.div(g[0]));
        }
        let step = ONE.div(C::new(-pole.re, -pole.im));
        for j in 1..=m {
            let n = m - j;
            let mut s = c[n];
            for _ in 0..n {
                s = s.mul(step);
            }
            r.push(s);
            p.push(pole);
        }
    }
    let kv = if nb >= na { row_c(&k) } else { empty00() };
    let mut outv = vec![col_c(&r), col_c(&p), kv];
    outv.truncate(nargout.max(1));
    out(outv)
}

fn residuez_inverse(a: &[Value]) -> Ret {
    let r = elements(&float_matrix(a, 0, "R", "residuez")?);
    let p = elements(&float_matrix(a, 1, "P", "residuez")?);
    let k = elements(&float_matrix(a, 2, "K", "residuez")?);
    if r.len() != p.len() {
        return rt("Length of R and P vectors must be the same.");
    }
    let n = p.len();
    // Denominator in powers of z^-1.
    let mut den = vec![ONE];
    for q in &p {
        den = poly_mul(&den, &[ONE, C::new(-q.re, -q.im)]);
    }
    let mut num = vec![ZERO; n.max(1)];
    let mut i = 0;
    while i < n {
        let mut m = 1;
        while i + m < n && p[i + m].sub(p[i]).abs() < 0.001 * p[i].abs().max(1.0) {
            m += 1;
        }
        for j in 1..=m {
            let mut term = vec![ONE];
            for (t, q) in p.iter().enumerate() {
                if t < i || t >= i + m {
                    term = poly_mul(&term, &[ONE, C::new(-q.re, -q.im)]);
                }
            }
            for _ in 0..(m - j) {
                term = poly_mul(&term, &[ONE, C::new(-p[i].re, -p[i].im)]);
            }
            for (t, c) in term.iter().enumerate() {
                num[t] = num[t].add(c.mul(r[i + j - 1]));
            }
        }
        i += m;
    }
    num.truncate(n.max(1));
    if n == 0 {
        num = vec![];
    }
    if !k.is_empty() {
        let kd = poly_mul(&k, &den);
        let len = kd.len().max(num.len());
        let mut sum = vec![ZERO; len];
        for (t, c) in kd.iter().enumerate() {
            sum[t] = sum[t].add(*c);
        }
        for (t, c) in num.iter().enumerate() {
            sum[t] = sum[t].add(*c);
        }
        num = sum;
    }
    let real = r.iter().chain(&p).chain(&k).all(|c| c.im == 0.0)
        || num.iter().chain(&den).all(|c| c.im.abs() <= 1e-10 * c.abs().max(1.0));
    let fix = |v: Vec<C>| -> Vec<C> { if real { v.into_iter().map(|c| C::new(c.re, 0.0)).collect() } else { v } };
    out(vec![row_c(&fix(num)), row_c(&fix(den))])
}

// ---- lattice -------------------------------------------------------------------------------

/// Direct-form polynomials A_0 .. A_M (ascending powers of z^-1) of the lattice with reflection
/// coefficients `k`.
fn step_up(k: &[C]) -> Vec<Vec<C>> {
    let mut stages = vec![vec![ONE]];
    for (m, km) in k.iter().enumerate() {
        let prev = &stages[m];
        let mut next = vec![ZERO; m + 2];
        next[0] = ONE;
        for i in 1..=m {
            next[i] = prev[i].add(cm(*km, prev[m + 1 - i].conj()));
        }
        next[m + 1] = *km;
        stages.push(next);
    }
    stages
}

/// Reflection coefficients and all stage polynomials of `a` (a(0) = 1), or None if some
/// coefficient has magnitude one.
fn step_down(a: &[C]) -> Option<(Vec<C>, Vec<Vec<C>>)> {
    let n = a.len() - 1;
    let mut stages = vec![vec![]; n + 1];
    let mut k = vec![ZERO; n];
    let mut cur = a.to_vec();
    for m in (1..=n).rev() {
        stages[m] = cur.clone();
        let km = cur[m];
        k[m - 1] = km;
        if m >= 2 && km.abs() == 1.0 {
            return None;
        }
        let den = 1.0 - km.abs() * km.abs();
        let den = if m == 1 { 1.0 } else { den };
        let mut next = vec![ZERO; m];
        next[0] = ONE;
        for i in 1..m {
            let d = cur[i].sub(cm(km, cur[m - i].conj()));
            next[i] = C::new(d.re / den, if d.im == 0.0 { 0.0 } else { d.im / den });
        }
        cur = next;
    }
    stages[0] = vec![ONE];
    Some((k, stages))
}

/// Backward polynomial of a stage: reversed and conjugated.
fn backward(a: &[C]) -> Vec<C> {
    a.iter().rev().map(|c| c.conj()).collect()
}

fn latc2tf(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let km = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        Value::Num(m) if m.kind != Kind::Char => {
            return rt(format!("Input K(reflection coefficients) in rc2poly must be double/single, instead it was {}.", m.kind.name()));
        }
        _ => return rt("CLASSNAME argument must be a class that supports ZEROS, such as 'double' or 'single'."),
    };
    if km.dims.len() > 2 || (km.dims[0] > 1 && km.dims[1] > 1) {
        return rt("The reflection coefficients must be stored in a vector.");
    }
    let mut k = elements(&km);
    let mut kind = "fir".to_string();
    let mut v: Option<Vec<C>> = None;
    if a.len() == 2 {
        match &a[1] {
            Value::Num(m) if m.kind == Kind::Char => {
                kind = m.to_string_lossy().to_lowercase();
                if !matches!(kind.as_str(), "fir" | "allpole" | "allpass" | "iir") {
                    return rt("Lattice type must be 'fir', 'allpole' or 'allpass'.");
                }
            }
            _ => v = Some(elements(&float_matrix(a, 1, "V", "latc2tf")?)),
        }
    }
    if let Some(v) = &v {
        let m = k.len().max(v.len().saturating_sub(1));
        k.resize(m, ZERO);
        let mut v = v.clone();
        v.resize(m + 1, ZERO);
        let stages = step_up(&k);
        let mut b = vec![ZERO; m + 1];
        for (mm, vm) in v.iter().enumerate() {
            let back = backward(&stages[mm]);
            for (i, c) in back.iter().enumerate() {
                b[i] = b[i].add(cm(*c, *vm));
            }
        }
        let den = stages[m].clone();
        let mut outv = vec![row_c(&b), row_c(&den)];
        outv.truncate(nargout.max(1));
        return out(outv);
    }
    let stages = step_up(&k);
    let poly = stages[k.len()].clone();
    let one = vec![ONE];
    let (b, den) = match kind.as_str() {
        "fir" => (poly, one),
        "allpass" => (backward(&poly), poly),
        _ => {
            let mut b = vec![ZERO; poly.len()];
            b[0] = ONE;
            (b, poly)
        }
    };
    let mut outv = vec![row_c(&b), row_c(&den)];
    outv.truncate(nargout.max(1));
    out(outv)
}

fn strip_trailing_zeros(mut v: Vec<C>) -> Vec<C> {
    while v.last().is_some_and(|c| c.re == 0.0 && c.im == 0.0) {
        v.pop();
    }
    v
}

const ONE_ERR: &str = "At least one of the reflection coefficients is equal to one. The algorithm fails for this case.";

fn tf2latc(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let is_text = |v: &Value| matches!(v, Value::Num(m) if m.kind == Kind::Char) || matches!(v, Value::Str(_));
    if a[0].class_name() == "cell" {
        return rt("Undefined function 'abs' for input arguments of type 'cell'.");
    }
    if a.iter().skip(1).any(is_text) {
        return rt("Unrecognized character vector specified.");
    }
    if a.len() == 3 {
        return rt("Too many input arguments.");
    }
    let num = elements(&float_matrix(a, 0, "NUM", "tf2latc")?);
    let den = if a.len() == 2 { Some(elements(&float_matrix(a, 1, "DEN", "tf2latc")?)) } else { None };
    let num = strip_trailing_zeros(num);
    if num.is_empty() {
        return rt("Numerator cannot be zero.");
    }
    let den = den.map(strip_trailing_zeros).unwrap_or_default();
    if den.len() <= 1 {
        // No denominator: an FIR lattice (one output) or the ladder of a numerator alone.
        let scale = den.first().copied().unwrap_or(ONE);
        if nargout >= 2 {
            let v: Vec<C> = num.iter().map(|c| cd(*c, scale)).collect();
            let k = vec![ZERO; num.len() - 1];
            return out(vec![col_c_shaped(&k), col_c(&v)]);
        }
        if num[0].re == 0.0 && num[0].im == 0.0 {
            return rt("Leading coefficient cannot be zero.");
        }
        if num.len() == 1 {
            return out(vec![empty00()]);
        }
        let p: Vec<C> = num.iter().map(|c| cd(*c, num[0])).collect();
        let Some((k, _)) = step_down(&p) else { return rt(ONE_ERR) };
        return out(vec![col_c(&k)]);
    }
    let len = num.len().max(den.len());
    let mut aa: Vec<C> = den.iter().map(|c| cd(*c, den[0])).collect();
    aa.resize(len, ZERO);
    let mut bb: Vec<C> = num.iter().map(|c| cd(*c, den[0])).collect();
    bb.resize(len, ZERO);
    let Some((k, stages)) = step_down(&aa) else { return rt(ONE_ERR) };
    if nargout < 2 {
        return out(vec![col_c_shaped(&k)]);
    }
    // Ladder coefficients: bb = sum_m v_m * backward(A_m), solved from the highest stage down.
    let m = len - 1;
    let mut v = vec![ZERO; len];
    let mut resid = bb;
    for mm in (0..=m).rev() {
        let back = backward(&stages[mm]);
        v[mm] = cd(resid[mm], back[mm]);
        for (i, c) in back.iter().enumerate() {
            resid[i] = resid[i].sub(cm(*c, v[mm]));
        }
    }
    out(vec![col_c_shaped(&k), col_c(&v)])
}

/// Column vector that is 0-by-1 when empty.
fn col_c_shaped(v: &[C]) -> Value {
    col_c(v)
}

// ---- cell arrays of sections ---------------------------------------------------------------

fn trim_trailing_zeros(m: &Matrix, range: std::ops::Range<usize>, rows: usize, row: usize) -> Value {
    let idx: Vec<usize> = range.map(|c| c * rows + row).collect();
    let mut n = idx.len();
    while n > 0 && m.data[idx[n - 1]] == 0.0 && m.im_at(idx[n - 1]) == 0.0 {
        n -= 1;
    }
    if n == 0 {
        return Value::Num(Matrix::from_dims(vec![0, 0], vec![]).cast_to(m.kind));
    }
    let re: Vec<f64> = idx[..n].iter().map(|&i| m.data[i]).collect();
    let mat = if m.im.is_some() && idx[..n].iter().any(|&i| m.im_at(i) != 0.0) {
        Matrix::complex(vec![1, n], re, idx[..n].iter().map(|&i| m.im_at(i)).collect())
    } else {
        Matrix::from_dims(vec![1, n], re)
    };
    Value::Num(mat.cast_to(m.kind))
}

fn sos2cell(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 1 {
        return rt("Too many output arguments.");
    }
    let six = "SOS matrix must be specified as a matrix with 6 columns.";
    let m = match &a[0] {
        Value::Num(m) if m.dims.len() == 2 && m.dims[1] == 6 => m.clone(),
        _ => return rt(six),
    };
    let rows = m.dims[0];
    let mut cells = vec![];
    if let Some(g) = a.get(1) {
        let nonempty = g.numel() != 0;
        if nonempty {
            if g.numel() != 1 {
                return rt("Operands to the logical AND (&&) and OR (||) operators must be convertible to logical scalar values. Use the ANY or ALL functions to reduce operands to logical scalar values.");
            }
            let is_one = matches!(g, Value::Num(x) if x.data[0] == 1.0 && x.im_at(0) == 0.0);
            if !is_one {
                cells.push(Value::Cell(CellArray::row(vec![g.clone(), Value::Num(Matrix::scalar(1.0))])));
            }
        }
    }
    for r in 0..rows {
        let b = trim_trailing_zeros(&m, 0..3, rows, r);
        let d = trim_trailing_zeros(&m, 3..6, rows, r);
        cells.push(Value::Cell(CellArray::row(vec![b, d])));
    }
    out(vec![Value::Cell(CellArray::from_dims(vec![1, cells.len()], cells))])
}

fn cell2sos(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 1 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let brace = "Brace indexing is not supported for variables of this type.";
    let Value::Cell(c) = &a[0] else { return rt(brace) };
    let mut items: Vec<&Value> = c.data.iter().collect();
    let index_err = |n: usize| rt::<()>(format!("Index exceeds the number of array elements. Index must not exceed {n}.")).unwrap_err();
    let section = |v: &Value| -> R<(Value, Value)> {
        let Value::Cell(s) = v else { return rt(brace) };
        if s.data.len() < 2 {
            return Err(index_err(s.data.len()));
        }
        Ok((s.data[0].clone(), s.data[1].clone()))
    };
    let mut gain = Value::Num(Matrix::scalar(1.0));
    if nargout >= 2 {
        if items.is_empty() {
            return Err(index_err(0));
        }
        let (b, d) = section(items[0])?;
        if b.numel() == 1 && d.numel() == 1 {
            let (Value::Num(bm), Value::Num(dm)) = (&b, &d) else { return rt("Input arguments must be 'double'.") };
            if bm.kind != Kind::Double || dm.kind != Kind::Double {
                return rt("Input arguments must be 'double'.");
            }
            {
                let q = cd(C::new(bm.data[0], bm.im_at(0)), C::new(dm.data[0], dm.im_at(0)));
                gain = shaped_c(vec![1, 1], &[q]);
                items.remove(0);
            }
        }
    }
    let l = items.len();
    let mut re = vec![0.0; l * 6];
    let mut im = vec![0.0; l * 6];
    let mut complex = false;
    for (r, it) in items.iter().enumerate() {
        let (b, d) = section(it)?;
        let (Value::Num(bm), Value::Num(dm)) = (&b, &d) else {
            return rt("Input arguments must be 'double'.");
        };
        if bm.kind != Kind::Double || dm.kind != Kind::Double {
            return rt("Input arguments must be 'double'.");
        }
        let mut row: Vec<C> = elements(bm);
        row.resize(row.len().max(3), ZERO);
        let mut dd = elements(dm);
        dd.resize(dd.len().max(3), ZERO);
        row.extend(dd);
        if row.len() != 6 {
            return rt(format!("Unable to perform assignment because the size of the left side is 1-by-6 and the size of the right side is 1-by-{}.", row.len()));
        }
        for (j, v) in row.iter().enumerate() {
            re[j * l + r] = v.re;
            im[j * l + r] = v.im;
            complex |= v.im != 0.0;
        }
    }
    let s = if complex { Value::Num(Matrix::complex(vec![l, 6], re, im)) } else { Value::Num(Matrix::from_dims(vec![l, 6], re)) };
    let mut outv = vec![s, gain];
    outv.truncate(nargout.max(1));
    out(outv)
}

// ---- impinvar ------------------------------------------------------------------------------

/// Coefficients e_l with n^(j-1)/(j-1)! = sum_l e_l C(n+l-1, l-1), l = 1..j.
fn binomial_basis(j: usize) -> Vec<f64> {
    let binom = |n: usize, k: usize| -> f64 {
        let mut v = 1.0;
        for t in 0..k {
            v = v * (n + k - t) as f64 / (t + 1) as f64;
        }
        v
    };
    // Triangular system from n = 0..j-1: C(n+l-1, l-1) is zero for l-1 > ... never, so solve by
    // Gaussian elimination on the small dense system.
    let mut m = vec![vec![0.0; j + 1]; j];
    let fact: f64 = (1..j).map(|x| x as f64).product();
    for n in 0..j {
        for l in 1..=j {
            m[n][l - 1] = binom(n, l - 1);
        }
        m[n][j] = (n as f64).powi(j as i32 - 1) / fact;
    }
    for c in 0..j {
        let piv = (c..j).max_by(|&x, &y| m[x][c].abs().partial_cmp(&m[y][c].abs()).unwrap()).unwrap();
        m.swap(c, piv);
        for r in 0..j {
            if r != c {
                let f = m[r][c] / m[c][c];
                for k in c..=j {
                    m[r][k] -= f * m[c][k];
                }
            }
        }
    }
    (0..j).map(|c| m[c][j] / m[c][c]).collect()
}

fn impinvar(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 4 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let bm = float_matrix(a, 0, "B", "impinvar")?;
    let am = float_matrix(a, 1, "A", "impinvar")?;
    let single = bm.kind == Kind::Single || am.kind == Kind::Single;
    let fs = match a.get(2) {
        None => 1.0,
        Some(Value::Num(m)) if m.kind == Kind::Char => {
            return rt("Input Fs in impinvar must be numeric, instead it was char.");
        }
        Some(Value::Num(m)) if m.numel() == 1 => m.data[0],
        Some(Value::Num(_)) => return rt("Matrix dimensions must agree."),
        Some(other) => return rt(format!("Input Fs in impinvar must be numeric, instead it was {}.", other.class_name())),
    };
    let vec_ok = |m: &Matrix| m.dims.len() == 2 && (m.dims[0] == 1 || m.dims[1] == 1) && m.numel() >= 1;
    if !(vec_ok(&am) && am.numel() >= 2) {
        return rt("'A' must be a vector.");
    }
    if bm.numel() > 0 && !vec_ok(&bm) {
        return rt("'B' must be a vector.");
    }
    let b = elements(&bm);
    let den = elements(&am);
    if b.len() > den.len() {
        return rt("Numerator B(s) degree must be no more than denominator A(s) degree.");
    }
    if den.iter().all(|c| c.re == 0.0 && c.im == 0.0) {
        return rt("Unrecognized function or variable 'poleavg'.");
    }
    let rpk = it.call_function("residue", vec![row_c(&b), row_c(&den)], 3)?;
    let r = elements(&rpk[0].to_matrix()?);
    let p = elements(&rpk[1].to_matrix()?);
    let k = elements(&rpk[2].to_matrix()?);
    let t = 1.0 / fs;
    let mut rz = vec![];
    let mut pz = vec![];
    let mut i = 0;
    while i < p.len() {
        let mut m = 1;
        while i + m < p.len() && p[i + m].sub(p[i]).abs() < 0.001 * p[i].abs().max(1.0) {
            m += 1;
        }
        let e = C::new((p[i].re * t).exp() * (p[i].im * t).cos(), (p[i].re * t).exp() * (p[i].im * t).sin());
        let e = if p[i].im == 0.0 { C::new((p[i].re * t).exp(), 0.0) } else { e };
        let mut acc = vec![ZERO; m];
        for j in 1..=m {
            let basis = binomial_basis(j);
            let scale = t.powi(j as i32);
            for (l, bl) in basis.iter().enumerate() {
                acc[l] = acc[l].add(C::new(r[i + j - 1].re * scale * bl, r[i + j - 1].im * scale * bl));
            }
        }
        for l in 0..m {
            rz.push(acc[l]);
            pz.push(e);
        }
        i += m;
    }
    let kz: Vec<C> = k.iter().map(|c| C::new(c.re * t, c.im * t)).collect();
    let res = it.call_function("residuez", vec![col_c(&rz), col_c(&pz), row_c(&kz)], 2)?;
    // MATLAB keeps the leading coefficient of A(s) in both outputs.
    let lead = den[0];
    let mut outv: Vec<Value> = res
        .into_iter()
        .take(nargout.max(1))
        .map(|v| match v.to_matrix() {
            Ok(m) => row_c(&elements(&m).iter().map(|c| cm(*c, lead)).collect::<Vec<_>>()),
            Err(_) => v,
        })
        .collect();
    if single {
        outv = outv.into_iter().map(crate::signal::to_single).collect();
    }
    out(outv)
}
