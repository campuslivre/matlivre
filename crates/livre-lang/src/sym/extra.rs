//! More symbolic functions: `str2sym`, `isolate`, `equationsToMatrix`, `vpasolve`,
//! `partfrac`, `laplace`/`ilaplace`, `symsum`/`symprod`, `hessian`, `horner`,
//! `polynomialDegree`, `finverse`, `charpoly`, `adjoint`, `has`, `formula`/`argnames`, and
//! number theory (`nextprime`, `fibonacci`, `harmonic`, `eulerPhi`, ...) and special
//! functions (`zeta`, `lambertw`, `sinc`, `kroneckerDelta`).

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::expr::{Const, Num, Rel};
use super::{algebra, calculus, mp, Expr, SymArray};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;
type Q = BigRational;

pub const NAMES: &[&str] = &[
    "str2sym", "isolate", "equationsToMatrix", "vpasolve", "partfrac", "laplace", "ilaplace", "symsum", "symprod", "hessian", "horner", "polynomialDegree",
    "finverse", "charpoly", "adjoint", "has", "formula", "argnames", "series", "divisors", "nextprime", "prevprime", "nthprime", "fibonacci", "harmonic",
    "eulerPhi", "kroneckerDelta", "zeta", "lambertw", "sinc", "bernoulli", "simplifyFraction", "symtrue", "symfalse",
];

fn sym_val(e: Expr) -> Value {
    Value::Sym(SymArray::scalar(e))
}

fn one(v: Value) -> R<Option<Vec<Value>>> {
    Ok(Some(vec![v]))
}

fn sym_arg(a: &[Value], i: usize) -> R<SymArray> {
    SymArray::from_value(a.get(i).ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)
}

fn scalar_arg(a: &[Value], i: usize) -> R<Expr> {
    let s = sym_arg(a, i)?;
    if s.data.len() != 1 {
        return rt("A scalar symbolic expression is required.");
    }
    Ok(s.data.into_iter().next().unwrap())
}

fn var_arg(a: &[Value], i: usize, e: &Expr) -> R<String> {
    match a.get(i) {
        Some(v) => match scalar_arg(&[v.clone()], 0)? {
            Expr::Var(n) => Ok(n),
            _ => rt("The variable must be a symbolic variable."),
        },
        None => super::default_var(e).ok_or_else(|| LangError::new("Unable to determine the variable.", 0)),
    }
}

fn vars_arg(v: &Value) -> R<Vec<String>> {
    SymArray::from_value(v)?
        .data
        .into_iter()
        .map(|e| match e {
            Expr::Var(n) => Ok(n),
            _ => rt("Variables must be symbolic variables."),
        })
        .collect()
}

/// `l == r` as `l - r`.
fn as_zero(e: &Expr) -> Expr {
    match e {
        Expr::Rel(Rel::Eq, l, r) => Expr::sub((**l).clone(), (**r).clone()),
        other => other.clone(),
    }
}

fn q_int(n: i64) -> Q {
    Q::from_integer(BigInt::from(n))
}

fn q_expr(q: &Q) -> Expr {
    Expr::Num(Num::from_big(q.clone()))
}

fn expr_q(e: &Expr) -> Option<Q> {
    e.as_num().and_then(|n| n.exact())
}

// ---- rational polynomials (ascending coefficients) --------------------------------------------------

fn trim(mut p: Vec<Q>) -> Vec<Q> {
    while p.len() > 1 && p.last().is_some_and(|c| c.is_zero()) {
        p.pop();
    }
    p
}

fn rat_poly(e: &Expr, v: &str) -> Option<Vec<Q>> {
    let c = algebra::poly_coeffs(&algebra::expand(e), v)?;
    c.iter().map(expr_q).collect::<Option<Vec<_>>>().map(trim)
}

fn pmul(a: &[Q], b: &[Q]) -> Vec<Q> {
    let mut out = vec![Q::zero(); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    trim(out)
}

/// Quotient and remainder of a / b.
fn pdivmod(a: &[Q], b: &[Q]) -> (Vec<Q>, Vec<Q>) {
    let mut r = a.to_vec();
    let db = b.len() - 1;
    if r.len() <= db {
        return (vec![Q::zero()], trim(r));
    }
    let mut q = vec![Q::zero(); r.len() - db];
    let lead = b[db].clone();
    for k in (0..q.len()).rev() {
        let c = &r[k + db] / &lead;
        for (j, bj) in b.iter().enumerate() {
            r[k + j] -= &c * bj;
        }
        q[k] = c;
    }
    r.truncate(db.max(1));
    (trim(q), trim(r))
}

fn peval(p: &[Q], x: &Q) -> Q {
    p.iter().rev().fold(Q::zero(), |acc, c| acc * x + c)
}

fn ppow(p: &[Q], n: usize) -> Vec<Q> {
    (0..n).fold(vec![Q::one()], |acc, _| pmul(&acc, p))
}

fn poly_expr(p: &[Q], v: &str) -> Expr {
    algebra::from_coeffs(&p.iter().map(q_expr).collect::<Vec<_>>(), v)
}

/// Rational roots of an integer-scaled polynomial (rational root theorem, small coefficients).
fn rational_roots(p: &[Q]) -> Vec<Q> {
    // Scale to integer coefficients.
    let lcm = p.iter().fold(BigInt::one(), |l, c| l.lcm(c.denom()));
    let ints: Vec<BigInt> = p.iter().map(|c| (c * Q::from_integer(lcm.clone())).to_integer()).collect();
    let mut lo = 0;
    while lo < ints.len() && ints[lo].is_zero() {
        lo += 1;
    }
    let mut out = Vec::new();
    if lo > 0 {
        out.push(Q::zero());
    }
    let ints = &ints[lo..];
    if ints.len() < 2 {
        return out;
    }
    let (a0, an) = (ints[0].abs().to_i64(), ints[ints.len() - 1].abs().to_i64());
    let (Some(a0), Some(an)) = (a0, an) else { return out };
    if a0 > 1_000_000 || an > 1_000_000 {
        return out;
    }
    let divs = |n: i64| (1..=n).filter(|d| n % d == 0).collect::<Vec<_>>();
    let poly: Vec<Q> = ints.iter().map(|c| Q::from_integer(c.clone())).collect();
    for p in divs(a0) {
        for q in divs(an) {
            for s in [1, -1] {
                let r = Q::new(BigInt::from(s * p), BigInt::from(q));
                if !out.contains(&r) && peval(&poly, &r).is_zero() {
                    out.push(r);
                }
            }
        }
    }
    out
}

/// Factors of a monic rational polynomial: (factor, multiplicity), linear factors first,
/// the irreducible remainder last.
fn factor_q(p: &[Q]) -> Vec<(Vec<Q>, usize)> {
    let mut rest = p.to_vec();
    let mut out = Vec::new();
    for r in rational_roots(p) {
        let f = vec![-r.clone(), Q::one()];
        let mut m = 0;
        loop {
            let (q, rem) = pdivmod(&rest, &f);
            if rem.iter().all(|c| c.is_zero()) && rest.len() > 1 {
                rest = q;
                m += 1;
            } else {
                break;
            }
        }
        if m > 0 {
            out.push((f, m));
        }
    }
    if rest.len() > 1 {
        let lead = rest.last().unwrap().clone();
        out.push((rest.iter().map(|c| c / &lead).collect(), 1));
    }
    out
}

/// Gaussian elimination over the rationals; `None` when singular.
fn solve_q(mut a: Vec<Vec<Q>>, mut b: Vec<Q>) -> Option<Vec<Q>> {
    let n = b.len();
    for col in 0..n {
        let piv = (col..n).find(|&r| !a[r][col].is_zero())?;
        a.swap(col, piv);
        b.swap(col, piv);
        for r in 0..n {
            if r != col && !a[r][col].is_zero() {
                let f = &a[r][col] / &a[col][col];
                for c in col..n {
                    let v = &f * &a[col][c];
                    a[r][c] -= v;
                }
                let v = &f * &b[col];
                b[r] -= v;
            }
        }
    }
    Some((0..n).map(|i| &b[i] / &a[i][i]).collect())
}

/// Partial fractions of num/den in v: the polynomial part and (numerator, factor, power) terms.
#[allow(clippy::type_complexity)]
fn partial_fractions(num: &[Q], den: &[Q]) -> Option<(Vec<Q>, Vec<(Vec<Q>, Vec<Q>, usize)>)> {
    let lead = den.last()?.clone();
    if lead.is_zero() {
        return None;
    }
    let den: Vec<Q> = den.iter().map(|c| c / &lead).collect();
    let num: Vec<Q> = num.iter().map(|c| c / &lead).collect();
    let (poly, rem) = pdivmod(&num, &den);
    let factors = factor_q(&den);
    // Unknowns: for each factor f^m and power j <= m, deg(f) numerator coefficients.
    let mut cols: Vec<(usize, usize, usize, Vec<Q>)> = Vec::new();
    for (fi, (f, m)) in factors.iter().enumerate() {
        for j in 1..=*m {
            let (cof, _) = pdivmod(&den, &ppow(f, j));
            for k in 0..f.len() - 1 {
                let mut basis = vec![Q::zero(); k + 1];
                basis[k] = Q::one();
                cols.push((fi, j, k, pmul(&basis, &cof)));
            }
        }
    }
    let n = den.len() - 1;
    if cols.len() != n {
        return None;
    }
    let a: Vec<Vec<Q>> = (0..n).map(|r| cols.iter().map(|c| c.3.get(r).cloned().unwrap_or_else(Q::zero)).collect()).collect();
    let b: Vec<Q> = (0..n).map(|r| rem.get(r).cloned().unwrap_or_else(Q::zero)).collect();
    let sol = solve_q(a, b)?;
    let mut terms = Vec::new();
    for (fi, (f, m)) in factors.iter().enumerate() {
        for j in 1..=*m {
            let coeffs: Vec<Q> = cols.iter().zip(&sol).filter(|(c, _)| c.0 == fi && c.1 == j).map(|(_, s)| s.clone()).collect();
            let coeffs = trim(coeffs);
            if !(coeffs.len() == 1 && coeffs[0].is_zero()) {
                terms.push((coeffs, f.clone(), j));
            }
        }
    }
    Some((poly, terms))
}

fn rational_parts(e: &Expr, v: &str) -> Option<(Vec<Q>, Vec<Q>)> {
    let t = algebra::together(e);
    let (n, d) = algebra::numden(&t);
    Some((rat_poly(&n, v)?, rat_poly(&d, v)?))
}

fn partfrac(e: &Expr, v: &str) -> Expr {
    let Some((n, d)) = rational_parts(e, v) else { return e.clone() };
    if d.len() < 2 {
        return e.clone();
    }
    let Some((poly, terms)) = partial_fractions(&n, &d) else { return e.clone() };
    let mut out = vec![poly_expr(&poly, v)];
    for (c, f, j) in terms {
        let base = poly_expr(&f, v);
        let den = if j == 1 { base } else { Expr::pow(base, Expr::int(j as i128)) };
        out.push(Expr::div(poly_expr(&c, v), den));
    }
    Expr::add(out)
}

// ---- Laplace transforms ---------------------------------------------------------------------------

fn factorial(n: u64) -> Q {
    (1..=n).fold(Q::one(), |acc, k| acc * q_int(k as i64))
}

/// Coefficients (a, b) of an argument a*v + b, if it is linear in v.
fn linear_in(e: &Expr, v: &str) -> Option<(Expr, Expr)> {
    let c = algebra::poly_coeffs(e, v)?;
    match c.len() {
        1 => Some((Expr::zero(), c[0].clone())),
        2 => Some((c[1].clone(), c[0].clone())),
        _ => None,
    }
}

fn factors(e: &Expr) -> Vec<Expr> {
    match e {
        Expr::Mul(f) => f.clone(),
        other => vec![other.clone()],
    }
}

fn laplace(f: &Expr, t: &str, s: &str) -> Option<Expr> {
    if !f.contains_var(t) {
        return Some(Expr::div(f.clone(), Expr::var(s)));
    }
    if let Expr::Add(terms) = f {
        return terms.iter().map(|x| laplace(x, t, s)).collect::<Option<Vec<_>>>().map(Expr::add);
    }
    let fs = factors(f);
    let (consts, rest): (Vec<Expr>, Vec<Expr>) = fs.into_iter().partition(|x| !x.contains_var(t));
    if !consts.is_empty() {
        return laplace(&Expr::mul(rest), t, s).map(|l| Expr::mul(consts.into_iter().chain([l]).collect()));
    }
    // exp(a t + b) * g(t): shift s by a.
    if let Some(k) = rest.iter().position(|x| matches!(x, Expr::Fn(n, _) if n == "exp")) {
        let Expr::Fn(_, args) = &rest[k] else { unreachable!() };
        let (a, b) = linear_in(&args[0], t)?;
        let mut others = rest.clone();
        others.remove(k);
        let g = laplace(&Expr::mul(others), t, s)?;
        let shifted = g.subs(s, &Expr::sub(Expr::var(s), a));
        return Some(if b.is_zero() { shifted } else { Expr::mul(vec![Expr::func("exp", vec![b]), shifted]) });
    }
    // heaviside(t - c) * g(t) = exp(-c s) L{g(t + c)}.
    if let Some(k) = rest.iter().position(|x| matches!(x, Expr::Fn(n, _) if n == "heaviside")) {
        let Expr::Fn(_, args) = &rest[k] else { unreachable!() };
        let (a, b) = linear_in(&args[0], t)?;
        if !a.is_one() {
            return None;
        }
        let c = Expr::neg(b);
        let mut others = rest.clone();
        others.remove(k);
        let g = Expr::mul(others).subs(t, &Expr::add(vec![Expr::var(t), c.clone()]));
        let lg = laplace(&algebra::expand(&g), t, s)?;
        let shift = Expr::func("exp", vec![Expr::neg(Expr::mul(vec![c.clone(), Expr::var(s)]))]);
        return Some(if c.is_zero() { lg } else { Expr::mul(vec![shift, lg]) });
    }
    if rest.len() == 1 {
        if let Expr::Fn(name, args) = &rest[0] {
            if name == "dirac" {
                let (a, b) = linear_in(&args[0], t)?;
                if a.is_one() {
                    return Some(Expr::func("exp", vec![Expr::mul(vec![b, Expr::var(s)])]));
                }
            }
        }
    }
    // t^n * g(t) = (-1)^n d^n/ds^n L{g}.
    let mut n_pow: i128 = 0;
    let mut others = Vec::new();
    for x in rest {
        match &x {
            Expr::Var(v) if v == t => n_pow += 1,
            Expr::Pow(b, e) if matches!(&**b, Expr::Var(v) if v == t) => match e.as_num().and_then(|n| n.as_int()) {
                Some(k) if k >= 0 => n_pow += k,
                _ => return None,
            },
            _ => others.push(x),
        }
    }
    if others.is_empty() {
        let n = n_pow as u64;
        return Some(Expr::div(q_expr(&factorial(n)), Expr::pow(Expr::var(s), Expr::int(n_pow + 1))));
    }
    if others.len() != 1 {
        return None;
    }
    let base = match &others[0] {
        Expr::Fn(name, args) if matches!(name.as_str(), "sin" | "cos" | "sinh" | "cosh") => {
            let (b, c) = linear_in(&args[0], t)?;
            if !c.is_zero() {
                return None;
            }
            let sv = Expr::var(s);
            let b2 = Expr::pow(b.clone(), Expr::int(2));
            let s2 = Expr::pow(sv.clone(), Expr::int(2));
            match name.as_str() {
                "sin" => Expr::div(b, Expr::add(vec![b2, s2])),
                "cos" => Expr::div(sv, Expr::add(vec![b2, s2])),
                "sinh" => Expr::div(b, Expr::sub(s2, b2)),
                _ => Expr::div(sv, Expr::sub(s2, b2)),
            }
        }
        _ => return None,
    };
    let mut out = base;
    for _ in 0..n_pow {
        out = Expr::neg(calculus::diff(&out, s));
    }
    Some(if n_pow > 0 { algebra::simplify(&out) } else { out })
}

/// The inverse transform of c/(s - a)^n, or (A s + B)/(s^2 + p s + q) terms.
fn ilaplace_rational(f: &Expr, s: &str, t: &str) -> Option<Expr> {
    let (n, d) = rational_parts(f, s)?;
    let (poly, terms) = partial_fractions(&n, &d)?;
    if poly.iter().any(|c| !c.is_zero()) {
        return None;
    }
    let tv = Expr::var(t);
    let mut out = Vec::new();
    for (c, fac, j) in terms {
        match fac.len() {
            2 => {
                // c / (s - a)^j -> c t^(j-1) e^(a t) / (j-1)!
                let a = -fac[0].clone();
                let coef = &c[0] / factorial(j as u64 - 1);
                let mut parts = vec![q_expr(&coef)];
                if j > 1 {
                    parts.push(Expr::pow(tv.clone(), Expr::int(j as i128 - 1)));
                }
                if !a.is_zero() {
                    parts.push(Expr::func("exp", vec![Expr::mul(vec![q_expr(&a), tv.clone()])]));
                }
                out.push(Expr::mul(parts));
            }
            3 if j == 1 => {
                // (A s + B)/((s - p)^2 + w^2) -> e^(p t) (A cos(w t) + (B + A p)/w sin(w t))
                let a_coef = c.get(1).cloned().unwrap_or_else(Q::zero);
                let b_coef = c[0].clone();
                let p = -&fac[1] / q_int(2);
                let w2 = &fac[0] - &p * &p;
                if !w2.is_positive() {
                    return None;
                }
                let w = Expr::pow(q_expr(&w2), Expr::rat(1, 2));
                let wt = Expr::mul(vec![w.clone(), tv.clone()]);
                let cos_part = Expr::mul(vec![q_expr(&a_coef), Expr::func("cos", vec![wt.clone()])]);
                let sin_coef = Expr::div(q_expr(&(&b_coef + &a_coef * &p)), w);
                let sin_part = Expr::mul(vec![sin_coef, Expr::func("sin", vec![wt])]);
                let osc = Expr::add(vec![cos_part, sin_part]);
                out.push(if p.is_zero() { osc } else { Expr::mul(vec![osc, Expr::func("exp", vec![Expr::mul(vec![q_expr(&p), tv.clone()])])]) });
            }
            _ => return None,
        }
    }
    Some(Expr::add(out))
}

fn ilaplace(f: &Expr, s: &str, t: &str) -> Option<Expr> {
    if let Expr::Add(terms) = f {
        if let Some(parts) = terms.iter().map(|x| ilaplace_term(x, s, t)).collect::<Option<Vec<_>>>() {
            return Some(Expr::add(parts));
        }
    }
    ilaplace_term(f, s, t).or_else(|| ilaplace_rational(f, s, t))
}

fn ilaplace_term(f: &Expr, s: &str, t: &str) -> Option<Expr> {
    if !f.contains_var(s) {
        return Some(Expr::mul(vec![f.clone(), Expr::func("dirac", vec![Expr::var(t)])]));
    }
    // exp(-c s) * G(s) -> g(t - c) heaviside(t - c).
    let fs = factors(f);
    if let Some(k) = fs.iter().position(|x| matches!(x, Expr::Fn(n, _) if n == "exp")) {
        let Expr::Fn(_, args) = &fs[k] else { unreachable!() };
        let (a, b) = linear_in(&args[0], s)?;
        if !b.is_zero() {
            return None;
        }
        let c = Expr::neg(a);
        let mut rest = fs.clone();
        rest.remove(k);
        let g = ilaplace(&Expr::mul(rest), s, t)?;
        let shifted = Expr::sub(Expr::var(t), c.clone());
        let g = g.subs(t, &shifted);
        let h = Expr::func("heaviside", vec![shifted]);
        return Some(if g.is_one() { h } else { Expr::mul(vec![g, h]) });
    }
    // Symbolic parameters: c/(s^2 + w^2) and c*s/(s^2 + w^2).
    let (n, d) = algebra::numden(&algebra::together(f));
    if let Some(dc) = algebra::poly_coeffs(&d, s) {
        if dc.len() == 3 && dc[1].is_zero() && dc[2].is_one() && dc[0].contains_var(s) == false && expr_q(&dc[0]).is_none() {
            let w = Expr::pow(dc[0].clone(), Expr::rat(1, 2));
            let wt = Expr::mul(vec![w.clone(), Expr::var(t)]);
            if let Some(nc) = algebra::poly_coeffs(&n, s) {
                let b = nc.first().cloned().unwrap_or_else(Expr::zero);
                let a = nc.get(1).cloned().unwrap_or_else(Expr::zero);
                if nc.len() <= 2 {
                    return Some(Expr::add(vec![Expr::mul(vec![a, Expr::func("cos", vec![wt.clone()])]), Expr::mul(vec![Expr::div(b, w), Expr::func("sin", vec![wt])])]));
                }
            }
        }
    }
    ilaplace_rational(f, s, t)
}

// ---- vpasolve -------------------------------------------------------------------------------------

/// Complex rational numbers for polishing polynomial roots.
#[derive(Clone)]
struct Cq(Q, Q);

impl Cq {
    fn add(&self, o: &Cq) -> Cq {
        Cq(&self.0 + &o.0, &self.1 + &o.1)
    }
    fn mul(&self, o: &Cq) -> Cq {
        Cq(&self.0 * &o.0 - &self.1 * &o.1, &self.0 * &o.1 + &self.1 * &o.0)
    }
    fn div(&self, o: &Cq) -> Cq {
        let d = &o.0 * &o.0 + &o.1 * &o.1;
        Cq((&self.0 * &o.0 + &self.1 * &o.1) / &d, (&self.1 * &o.0 - &self.0 * &o.1) / &d)
    }
    fn round(&self, digits: u32) -> Cq {
        Cq(super::expr::round_sig(&self.0, digits), super::expr::round_sig(&self.1, digits))
    }
}

fn q_from_f64(x: f64) -> Q {
    Q::from_float(x).unwrap_or_else(Q::zero)
}

/// All roots of a polynomial (Durand-Kerner in f64, then Newton in exact arithmetic).
fn poly_roots(p: &[Q], digits: u32) -> Vec<Cq> {
    let n = p.len() - 1;
    let lead = p[n].to_f64().unwrap_or(1.0);
    let pf: Vec<(f64, f64)> = p.iter().map(|c| (c.to_f64().unwrap_or(0.0) / lead, 0.0)).collect();
    let cm = |a: (f64, f64), b: (f64, f64)| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
    let cd = |a: (f64, f64), b: (f64, f64)| {
        let d = b.0 * b.0 + b.1 * b.1;
        ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
    };
    let eval = |x: (f64, f64)| pf.iter().rev().fold((0.0, 0.0), |acc, c| {
        let m = cm(acc, x);
        (m.0 + c.0, m.1 + c.1)
    });
    let mut z: Vec<(f64, f64)> = (0..n).map(|k| {
        let a = 0.4f64 + 0.9;
        let ang = 2.0 * std::f64::consts::PI * k as f64 / n as f64 + 0.4;
        (a * ang.cos(), a * ang.sin())
    }).collect();
    for _ in 0..500 {
        for i in 0..n {
            let mut den = (1.0, 0.0);
            for j in 0..n {
                if i != j {
                    den = cm(den, (z[i].0 - z[j].0, z[i].1 - z[j].1));
                }
            }
            let step = cd(eval(z[i]), den);
            z[i] = (z[i].0 - step.0, z[i].1 - step.1);
        }
    }
    let dp: Vec<Q> = (1..p.len()).map(|k| &p[k] * q_int(k as i64)).collect();
    let ev = |poly: &[Q], x: &Cq| poly.iter().rev().fold(Cq(Q::zero(), Q::zero()), |acc, c| acc.mul(x).add(&Cq(c.clone(), Q::zero())));
    let mut out: Vec<Cq> = z
        .into_iter()
        .map(|(re, im)| {
            let im = if im.abs() < 1e-12 * (1.0 + re.abs()) { 0.0 } else { im };
            let mut x = Cq(q_from_f64(re), q_from_f64(im));
            for _ in 0..8 {
                let d = ev(&dp, &x);
                if d.0.is_zero() && d.1.is_zero() {
                    break;
                }
                let step = ev(p, &x).div(&d);
                x = Cq(&x.0 - &step.0, &x.1 - &step.1).round(digits + 10);
            }
            x
        })
        .collect();
    // MATLAB lists real roots first (ascending), then complex ones by real part.
    out.sort_by(|a, b| {
        let (ra, rb) = (a.1.is_zero(), b.1.is_zero());
        rb.cmp(&ra).then(a.0.partial_cmp(&b.0).unwrap()).then(a.1.partial_cmp(&b.1).unwrap())
    });
    out
}

fn vpa_q(q: &Q, digits: u32) -> Expr {
    Expr::Num(Num::vpa(&super::expr::round_sig(q, digits)))
}

fn vpa_c(c: &Cq, digits: u32) -> Expr {
    let tiny = |q: &Q| q.is_zero() || q.abs() < Q::new(BigInt::one(), BigInt::from(10).pow(digits + 5));
    if tiny(&c.1) {
        return vpa_q(&c.0, digits);
    }
    let im = Expr::mul(vec![vpa_q(&c.1, digits), Expr::Const(Const::I)]);
    if tiny(&c.0) {
        return im;
    }
    Expr::add(vec![vpa_q(&c.0, digits), im])
}

/// A real root of f near x0 by Newton's method, refined in exact arithmetic.
fn newton_real(f: &Expr, v: &str, x0: f64, digits: u32) -> Option<Q> {
    let df = calculus::diff(f, v);
    let fe = |x: f64| f.subs(v, &Expr::from_f64(x)).to_f64();
    let dfe = |x: f64| df.subs(v, &Expr::from_f64(x)).to_f64();
    let mut x = x0;
    for _ in 0..200 {
        let (fx, dx) = (fe(x)?, dfe(x)?);
        if dx == 0.0 || !dx.is_finite() {
            return None;
        }
        let nx = x - fx / dx;
        if (nx - x).abs() < 1e-15 * (1.0 + x.abs()) {
            x = nx;
            break;
        }
        x = nx;
    }
    if !fe(x)?.is_finite() || fe(x)?.abs() > 1e-8 * (1.0 + x.abs()) {
        return None;
    }
    // Polish with high-precision evaluation.
    let mut q = q_from_f64(x);
    for _ in 0..6 {
        let at = Expr::Num(Num::from_big(q.clone()));
        let fx = mp::eval_digits(&f.subs(v, &at))?;
        let dx = mp::eval_digits(&df.subs(v, &at))?;
        if dx.is_zero() {
            break;
        }
        q = super::expr::round_sig(&(&q - fx / dx), digits + 10);
    }
    Some(q)
}

fn vpasolve(it: &mut Interpreter, a: &[Value], nargout: usize) -> R<Option<Vec<Value>>> {
    let digits = super::expr::digits();
    let eqs: Vec<Expr> = sym_arg(a, 0)?.data.iter().map(as_zero).collect();
    let vars: Vec<String> = match a.get(1) {
        Some(v) => vars_arg(v)?,
        None => {
            let mut vs = Vec::new();
            for e in &eqs {
                e.free_vars(&mut vs);
            }
            vs.sort();
            vs.dedup();
            vs
        }
    };
    let init = a.get(2).map(|v| v.to_matrix()).transpose()?;
    if eqs.len() == 1 && vars.len() == 1 {
        let (f, v) = (&eqs[0], &vars[0]);
        if init.is_none() {
            if let Some((n, d)) = rational_parts(f, v) {
                if d.len() == 1 && n.len() > 1 {
                    let roots = poly_roots(&n, digits);
                    let vals: Vec<Expr> = roots.iter().map(|r| vpa_c(r, digits)).collect();
                    let k = vals.len();
                    return one(Value::Sym(SymArray::from_dims(vec![k, 1], vals)));
                }
            }
        }
        let starts: Vec<f64> = match &init {
            Some(m) if m.numel() == 1 => vec![m.data[0]],
            Some(m) if m.numel() == 2 => vec![(m.data[0] + m.data[1]) / 2.0, m.data[0], m.data[1]],
            _ => {
                let mut s = vec![0.0, 1.0, -1.0];
                s.extend((1..=40).flat_map(|k| [k as f64 * 0.5, -(k as f64) * 0.5]));
                s
            }
        };
        for x0 in starts {
            if let Some(q) = newton_real(f, v, x0, digits) {
                if let Some(m) = init.as_ref().filter(|m| m.numel() == 2) {
                    let x = q.to_f64().unwrap_or(f64::NAN);
                    if x < m.data[0] || x > m.data[1] {
                        continue;
                    }
                }
                return one(sym_val(vpa_q(&q, digits)));
            }
        }
        return one(Value::Sym(SymArray::from_dims(vec![0, 1], Vec::new())));
    }
    // Systems: linear ones exactly, others by Newton's method in floating point.
    let sol: Vec<Expr> = match algebra::solve_linear(&eqs, &vars) {
        Some(s) => s.iter().map(|e| mp::vpa(e)).collect(),
        None => {
            let jac: Vec<Vec<Expr>> = eqs.iter().map(|e| vars.iter().map(|v| calculus::diff(e, v)).collect()).collect();
            let mut x: Vec<f64> = match &init {
                Some(m) if m.numel() == vars.len() => m.data.clone(),
                _ => vec![0.5; vars.len()],
            };
            let at = |e: &Expr, x: &[f64]| -> Option<f64> {
                let mut e = e.clone();
                for (v, xv) in vars.iter().zip(x) {
                    e = e.subs(v, &Expr::from_f64(*xv));
                }
                e.to_f64()
            };
            for _ in 0..100 {
                let fx: Option<Vec<f64>> = eqs.iter().map(|e| at(e, &x)).collect();
                let jx: Option<Vec<Vec<f64>>> = jac.iter().map(|r| r.iter().map(|e| at(e, &x)).collect()).collect();
                let (Some(fx), Some(jx)) = (fx, jx) else { break };
                let a: Vec<Vec<Q>> = jx.iter().map(|r| r.iter().map(|&v| q_from_f64(v)).collect()).collect();
                let b: Vec<Q> = fx.iter().map(|&v| q_from_f64(v)).collect();
                let Some(step) = solve_q(a, b) else { break };
                let mut done = true;
                for (xi, si) in x.iter_mut().zip(&step) {
                    let s = si.to_f64().unwrap_or(0.0);
                    *xi -= s;
                    if s.abs() > 1e-15 * (1.0 + xi.abs()) {
                        done = false;
                    }
                }
                if done {
                    break;
                }
            }
            x.iter().map(|&v| vpa_q(&q_from_f64(v), digits)).collect()
        }
    };
    if nargout >= vars.len() && vars.len() > 1 {
        return Ok(Some(sol.into_iter().map(sym_val).collect()));
    }
    if vars.len() == 1 {
        return one(sym_val(sol.into_iter().next().unwrap()));
    }
    let _ = it;
    let mut s = StructArray::scalar();
    for (v, e) in vars.iter().zip(sol) {
        s.set(v, sym_val(e));
    }
    one(Value::Struct(s))
}

// ---- sums, products, matrices ------------------------------------------------------------------

/// Sum of f(k) for k = a..b with numeric bounds.
fn finite_sum(f: &Expr, k: &str, a: i128, b: i128, product: bool) -> Expr {
    let items: Vec<Expr> = (a..=b).map(|i| f.subs(k, &Expr::int(i))).collect();
    let r = if product { Expr::mul(items) } else { Expr::add(items) };
    algebra::simplify(&r)
}

fn symsum(f: &Expr, k: &str, a: &Expr, b: &Expr) -> Expr {
    let (ai, bi) = (a.as_num().and_then(|n| n.as_int()), b.as_num().and_then(|n| n.as_int()));
    if let (Some(ai), Some(bi)) = (ai, bi) {
        return finite_sum(f, k, ai, bi, false);
    }
    // A polynomial in k summed to a symbolic bound: interpolate the closed form.
    if let (Some(ai), Expr::Var(n)) = (ai, b) {
        if let Some(c) = rat_poly(f, k) {
            let d = c.len();
            let pts: Vec<(Q, Q)> = (0..=d as i128)
                .map(|j| {
                    let upper = ai + j;
                    let s = (ai..=upper).fold(Q::zero(), |acc, i| acc + peval(&c, &Q::from_integer(BigInt::from(i))));
                    (Q::from_integer(BigInt::from(upper)), s)
                })
                .collect();
            // Lagrange interpolation in n.
            let mut poly = vec![Q::zero()];
            for (i, (xi, yi)) in pts.iter().enumerate() {
                let mut term = vec![yi.clone()];
                for (j, (xj, _)) in pts.iter().enumerate() {
                    if i != j {
                        let den = xi - xj;
                        term = pmul(&term, &[-xj / &den, Q::one() / &den]);
                    }
                }
                let len = poly.len().max(term.len());
                poly.resize(len, Q::zero());
                for (p, t) in poly.iter_mut().zip(term.iter().chain(std::iter::repeat(&Q::zero()))) {
                    *p += t;
                }
            }
            return algebra::factored(&poly_expr(&trim(poly), n));
        }
    }
    // Geometric series c*r^k to infinity.
    if calculus::is_inf(b) {
        if let Some(ai) = ai {
            let first = f.subs(k, &Expr::int(ai));
            let next = f.subs(k, &Expr::int(ai + 1));
            let ratio = algebra::simplify(&Expr::div(next.clone(), first.clone()));
            if !ratio.contains_var(k) {
                if let Some(r) = ratio.to_f64() {
                    if r.abs() < 1.0 {
                        return algebra::simplify(&Expr::div(first, Expr::sub(Expr::one(), ratio)));
                    }
                }
            }
        }
    }
    Expr::func("symsum", vec![f.clone(), Expr::var(k), a.clone(), b.clone()])
}

fn symprod(f: &Expr, k: &str, a: &Expr, b: &Expr) -> Expr {
    let (ai, bi) = (a.as_num().and_then(|n| n.as_int()), b.as_num().and_then(|n| n.as_int()));
    if let (Some(ai), Some(bi)) = (ai, bi) {
        return finite_sum(f, k, ai, bi, true);
    }
    if *f == Expr::var(k) && ai == Some(1) {
        return Expr::func("factorial", vec![b.clone()]);
    }
    Expr::func("symprod", vec![f.clone(), Expr::var(k), a.clone(), b.clone()])
}

fn horner(e: &Expr, v: &str) -> Expr {
    let Some(c) = algebra::poly_coeffs(e, v) else { return e.clone() };
    let x = Expr::var(v);
    let mut acc = c.last().cloned().unwrap_or_else(Expr::zero);
    for k in (0..c.len() - 1).rev() {
        acc = Expr::add(vec![Expr::Mul(vec![x.clone(), acc]), c[k].clone()]);
        if let Expr::Add(ref ts) = acc {
            if ts.len() == 1 {
                acc = ts[0].clone();
            }
        }
    }
    acc
}

fn total_degree(e: &Expr, vars: &[String]) -> Option<i128> {
    let ex = algebra::expand(e);
    let terms: Vec<Expr> = match ex {
        Expr::Add(t) => t,
        other => vec![other],
    };
    let mut best: Option<i128> = None;
    for t in terms {
        let mut d = 0i128;
        for f in factors(&t) {
            match &f {
                Expr::Var(v) if vars.contains(v) => d += 1,
                Expr::Pow(b, e) => match (&**b, e.as_num().and_then(|n| n.as_int())) {
                    (Expr::Var(v), Some(k)) if vars.contains(v) && k >= 0 => d += k,
                    _ if vars.iter().any(|v| f.contains_var(v)) => return None,
                    _ => {}
                },
                other if vars.iter().any(|v| other.contains_var(v)) => return None,
                _ => {}
            }
        }
        best = Some(best.map_or(d, |b| b.max(d)));
    }
    best
}

/// Characteristic polynomial coefficients (Faddeev-LeVerrier), highest power first.
fn charpoly_coeffs(a: &[Vec<Expr>]) -> Vec<Expr> {
    let n = a.len();
    let mat_mul = |x: &[Vec<Expr>], y: &[Vec<Expr>]| -> Vec<Vec<Expr>> {
        (0..n).map(|i| (0..n).map(|j| algebra::simplify(&Expr::add((0..n).map(|k| Expr::mul(vec![x[i][k].clone(), y[k][j].clone()])).collect()))).collect()).collect()
    };
    let mut coeffs = vec![Expr::one()];
    let mut m: Vec<Vec<Expr>> = (0..n).map(|i| (0..n).map(|j| if i == j { Expr::zero() } else { Expr::zero() }).collect()).collect();
    for k in 1..=n {
        // M_k = A M_{k-1} + c_{n-k+1} I
        let mut next = mat_mul(a, &m);
        let c_prev = coeffs.last().unwrap().clone();
        for (i, row) in next.iter_mut().enumerate() {
            row[i] = algebra::simplify(&Expr::add(vec![row[i].clone(), c_prev.clone()]));
        }
        m = next;
        let am = mat_mul(a, &m);
        let tr = Expr::add((0..n).map(|i| am[i][i].clone()).collect());
        coeffs.push(algebra::simplify(&Expr::mul(vec![Expr::rat(-1, k as i128), tr])));
    }
    coeffs
}

fn rows_of(s: &SymArray) -> R<Vec<Vec<Expr>>> {
    if s.dims.len() != 2 || s.dims[0] != s.dims[1] {
        return rt("Matrix must be square.");
    }
    let n = s.dims[0];
    Ok((0..n).map(|i| (0..n).map(|j| s.data[j * n + i].clone()).collect()).collect())
}

fn has(e: &Expr, sub: &Expr) -> bool {
    if e == sub {
        return true;
    }
    match e {
        Expr::Add(v) | Expr::Mul(v) | Expr::Fn(_, v) => v.iter().any(|x| has(x, sub)),
        Expr::Pow(a, b) | Expr::Rel(_, a, b) => has(a, sub) || has(b, sub),
        _ => false,
    }
}

// ---- number theory and special functions ----------------------------------------------------------

fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n % 2 == 0 {
        return n == 2;
    }
    let mut d = 3;
    while d * d <= n {
        if n % d == 0 {
            return false;
        }
        d += 2;
    }
    true
}

fn bernoulli_q(n: usize) -> Q {
    // Akiyama-Tanigawa.
    let mut a: Vec<Q> = Vec::with_capacity(n + 1);
    for m in 0..=n {
        a.push(Q::new(BigInt::one(), BigInt::from(m as i64 + 1)));
        for j in (1..=m).rev() {
            a[j - 1] = (&a[j - 1] - &a[j]) * q_int(j as i64);
        }
    }
    // This gives B1 = +1/2; MATLAB uses -1/2.
    if n == 1 { Q::new(BigInt::from(-1), BigInt::from(2)) } else { a[0].clone() }
}

/// Riemann zeta for real s (Euler-Maclaurin / alternating series).
pub fn zeta_f64(s: f64) -> f64 {
    if s == 1.0 {
        return f64::INFINITY;
    }
    if s < 0.5 {
        // Functional equation.
        let g = libm::tgamma(1.0 - s);
        return 2f64.powf(s) * std::f64::consts::PI.powf(s - 1.0) * (std::f64::consts::PI * s / 2.0).sin() * g * zeta_f64(1.0 - s);
    }
    // Euler-Maclaurin: the first N terms, the integral tail and Bernoulli corrections.
    let n = 12.0f64;
    let mut sum: f64 = (1..12).map(|k| (k as f64).powf(-s)).sum();
    sum += n.powf(1.0 - s) / (s - 1.0) + 0.5 * n.powf(-s);
    let b = [1.0 / 6.0, -1.0 / 30.0, 1.0 / 42.0, -1.0 / 30.0, 5.0 / 66.0, -691.0 / 2730.0, 7.0 / 6.0];
    let mut rising = s; // s (s+1) ... (s + 2j - 2)
    let mut fact = 2.0; // (2j)!
    for (j, bj) in b.iter().enumerate() {
        let j = j as f64 + 1.0;
        sum += bj / fact * rising * n.powf(-s - 2.0 * j + 1.0);
        rising *= (s + 2.0 * j - 1.0) * (s + 2.0 * j);
        fact *= (2.0 * j + 1.0) * (2.0 * j + 2.0);
    }
    sum
}

/// Principal branch of the Lambert W function for real x >= -1/e.
fn lambertw_f64(x: f64) -> f64 {
    if x < -(-1.0f64).exp() {
        return f64::NAN;
    }
    let mut w = if x < 1.0 { 0.0 } else { x.ln() - x.ln().ln().max(0.0) };
    for _ in 0..100 {
        let e = w.exp();
        let f = w * e - x;
        let next = w - f / (e * (w + 1.0) - (w + 2.0) * f / (2.0 * w + 2.0));
        if (next - w).abs() <= 1e-16 * (1.0 + w.abs()) {
            return next;
        }
        w = next;
    }
    w
}

fn int_arg(v: &Value) -> R<Vec<i128>> {
    let s = SymArray::from_value(v)?;
    s.data.iter().map(|e| e.as_num().and_then(|n| n.as_int()).ok_or_else(|| LangError::new("Input must be integers.", 0))).collect()
}

/// Result of an elementwise integer function: sym for sym input, double otherwise.
fn int_out(input: &Value, dims: Vec<usize>, vals: Vec<Expr>) -> Value {
    if matches!(input, Value::Sym(_)) {
        Value::Sym(SymArray::from_dims(dims, vals))
    } else {
        Value::Num(Matrix::from_dims(dims, vals.iter().map(|e| e.to_f64().unwrap_or(f64::NAN)).collect()))
    }
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> R<Option<Vec<Value>>> {
    match name {
        "str2sym" => {
            let text = a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?;
            let texts: Vec<String> = match text {
                Value::Cell(c) => c.data.iter().map(|v| v.text()).collect::<R<_>>()?,
                Value::Str(s) => s.data.iter().map(|x| x.clone().unwrap_or_default()).collect(),
                v => vec![v.text()?],
            };
            let mut out = Vec::new();
            let mut dims = vec![1, 1];
            for t in &texts {
                let ast = crate::parser::parse_expr(t.trim())?;
                if let crate::ast::Expr::Matrix(rows) = &ast {
                    let r = rows.len();
                    let c = rows.first().map(|x| x.len()).unwrap_or(0);
                    let mut data = vec![Expr::zero(); r * c];
                    for (i, row) in rows.iter().enumerate() {
                        for (j, e) in row.iter().enumerate() {
                            data[j * r + i] = super::ast_to_sym(e)?;
                        }
                    }
                    return one(Value::Sym(SymArray::from_dims(vec![r, c], data)));
                }
                out.push(super::ast_to_sym(&ast)?);
            }
            if out.len() > 1 {
                dims = text.dims();
            }
            one(Value::Sym(SymArray::from_dims(dims, out)))
        }
        "isolate" => {
            let eq = scalar_arg(a, 0)?;
            let v = var_arg(a, 1, &eq)?;
            let z = as_zero(&eq);
            let roots = algebra::solve_one(&z, &v).ok_or_else(|| LangError::new(format!("Unable to isolate '{v}' because the equation has no solution."), 0))?;
            let r = roots.into_iter().next().ok_or_else(|| LangError::new(format!("Unable to isolate '{v}' because the equation has no solution."), 0))?;
            one(sym_val(Expr::rel(Rel::Eq, Expr::var(&v), r)))
        }
        "equationsToMatrix" => {
            let eqs: Vec<Expr> = sym_arg(a, 0)?.data.iter().map(as_zero).collect();
            let vars: Vec<String> = match a.get(1) {
                Some(v) => {
                    let mut out = Vec::new();
                    for x in &a[1..] {
                        out.extend(vars_arg(x)?);
                    }
                    let _ = v;
                    out
                }
                None => {
                    let mut vs = Vec::new();
                    for e in &eqs {
                        e.free_vars(&mut vs);
                    }
                    vs.sort();
                    vs.dedup();
                    vs
                }
            };
            let (m, n) = (eqs.len(), vars.len());
            let mut am = vec![Expr::zero(); m * n];
            let mut bm = vec![Expr::zero(); m];
            for (i, e) in eqs.iter().enumerate() {
                let mut rest = e.clone();
                for (j, v) in vars.iter().enumerate() {
                    let c = algebra::simplify(&calculus::diff(e, v));
                    if vars.iter().any(|w| c.contains_var(w)) {
                        return rt("Cannot convert to matrix form because the system does not seem to be linear.");
                    }
                    am[j * m + i] = c;
                    rest = rest.subs(v, &Expr::zero());
                }
                bm[i] = algebra::simplify(&Expr::neg(rest));
            }
            let a_val = Value::Sym(SymArray::from_dims(vec![m, n], am));
            let b_val = Value::Sym(SymArray::from_dims(vec![m, 1], bm));
            Ok(Some(if nargout >= 2 { vec![a_val, b_val] } else { vec![a_val] }))
        }
        "vpasolve" => vpasolve(it, a, nargout),
        "partfrac" => {
            let s = sym_arg(a, 0)?;
            let v = match a.get(1) {
                Some(_) => var_arg(a, 1, &s.data[0])?,
                None => super::default_var(&s.data[0]).unwrap_or_else(|| "x".into()),
            };
            one(Value::Sym(s.map(|e| partfrac(e, &v))))
        }
        "laplace" | "ilaplace" => {
            let s = sym_arg(a, 0)?;
            let first = s.data.first().cloned().unwrap_or_else(Expr::zero);
            let (from_default, to_default) = if name == "laplace" { ("t", "s") } else { ("s", "t") };
            let mut fv = Vec::new();
            first.free_vars(&mut fv);
            let from = match a.get(1) {
                Some(_) if a.len() >= 3 => var_arg(a, 1, &first)?,
                _ if fv.iter().any(|v| v == from_default) => from_default.to_string(),
                _ => super::default_var(&first).unwrap_or_else(|| from_default.to_string()),
            };
            let to = match a.len() {
                2 => var_arg(a, 1, &first)?,
                n if n >= 3 => var_arg(a, 2, &first)?,
                // When the default target letter is the variable itself, MATLAB picks z / x.
                _ if from == to_default => (if name == "laplace" { "z" } else { "x" }).to_string(),
                _ => to_default.to_string(),
            };
            one(Value::Sym(s.map(|e| {
                let r = if name == "laplace" { laplace(e, &from, &to) } else { ilaplace(e, &from, &to) };
                match r {
                    Some(r) => algebra::simplify(&r).pipe_if_bigger(r),
                    None => Expr::func(name, vec![e.clone(), Expr::var(&from), Expr::var(&to)]),
                }
            })))
        }
        "symsum" | "symprod" => {
            let f = sym_arg(a, 0)?;
            let k = var_arg(a, 1, &f.data[0])?;
            let lo = scalar_arg(a, 2)?;
            let hi = scalar_arg(a, 3)?;
            one(Value::Sym(f.map(|e| if name == "symsum" { symsum(e, &k, &lo, &hi) } else { symprod(e, &k, &lo, &hi) })))
        }
        "hessian" => {
            let f = scalar_arg(a, 0)?;
            let vars = match a.get(1) {
                Some(v) => vars_arg(v)?,
                None => {
                    let mut vs = Vec::new();
                    f.free_vars(&mut vs);
                    vs.sort();
                    vs
                }
            };
            let n = vars.len();
            let mut data = vec![Expr::zero(); n * n];
            for (i, vi) in vars.iter().enumerate() {
                let di = calculus::diff(&f, vi);
                for (j, vj) in vars.iter().enumerate() {
                    data[j * n + i] = algebra::simplify(&calculus::diff(&di, vj));
                }
            }
            one(Value::Sym(SymArray::from_dims(vec![n, n], data)))
        }
        "horner" => {
            let s = sym_arg(a, 0)?;
            let v = match a.get(1) {
                Some(_) => var_arg(a, 1, &s.data[0])?,
                None => super::default_var(&s.data[0]).unwrap_or_else(|| "x".into()),
            };
            one(Value::Sym(s.map(|e| horner(e, &v))))
        }
        "polynomialDegree" => {
            let e = scalar_arg(a, 0)?;
            let vars = match a.get(1) {
                Some(v) => vars_arg(v)?,
                None => {
                    let mut vs = Vec::new();
                    e.free_vars(&mut vs);
                    vs
                }
            };
            let d = total_degree(&e, &vars).ok_or_else(|| LangError::new("Input must be a polynomial.", 0))?;
            one(Value::num(d as f64))
        }
        "finverse" => {
            let f = scalar_arg(a, 0)?;
            let v = var_arg(a, 1, &f)?;
            let tmp = "matlivre_finverse_y";
            let roots = algebra::solve_one(&Expr::sub(f.clone(), Expr::var(tmp)), &v).unwrap_or_default();
            match roots.into_iter().next() {
                Some(r) => one(sym_val(algebra::simplify(&r.subs(tmp, &Expr::var(&v))))),
                None => rt("Unable to find the functional inverse."),
            }
        }
        "charpoly" => {
            let sym_in = matches!(a.first(), Some(Value::Sym(_)));
            let m = sym_arg(a, 0)?;
            let coeffs = charpoly_coeffs(&rows_of(&m)?);
            match a.get(1) {
                Some(_) => {
                    let v = var_arg(a, 1, &Expr::zero())?;
                    let asc: Vec<Expr> = coeffs.iter().rev().cloned().collect();
                    one(sym_val(algebra::from_coeffs(&asc, &v)))
                }
                None if sym_in => one(Value::Sym(SymArray::from_dims(vec![1, coeffs.len()], coeffs))),
                None => one(Value::Num(Matrix::row(coeffs.iter().map(|e| e.to_f64().unwrap_or(f64::NAN)).collect()))),
            }
        }
        "adjoint" => {
            let m = sym_arg(a, 0)?;
            let rows = rows_of(&m)?;
            let n = rows.len();
            let mut data = vec![Expr::zero(); n * n];
            for i in 0..n {
                for j in 0..n {
                    // Cofactor C(i, j) goes to position (j, i).
                    let minor: Vec<Expr> = (0..n - 1).flat_map(|c| (0..n - 1).map(move |r| (r, c))).map(|(r, c)| {
                        let rr = if r < i { r } else { r + 1 };
                        let cc = if c < j { c } else { c + 1 };
                        rows[rr][cc].clone()
                    }).collect();
                    let det = if n == 1 { Expr::one() } else { super::sym_det(&SymArray::from_dims(vec![n - 1, n - 1], minor))? };
                    let sign = if (i + j) % 2 == 0 { Expr::one() } else { Expr::int(-1) };
                    data[i * n + j] = algebra::simplify(&Expr::mul(vec![sign, det]));
                }
            }
            one(Value::Sym(SymArray::from_dims(vec![n, n], data)))
        }
        "has" => {
            let e = sym_arg(a, 0)?;
            let sub = scalar_arg(a, 1)?;
            one(Value::bool(e.data.iter().any(|x| has(x, &sub))))
        }
        "formula" => {
            let mut s = sym_arg(a, 0)?;
            s.args.clear();
            one(Value::Sym(s))
        }
        "argnames" => {
            let s = sym_arg(a, 0)?;
            let n = s.args.len();
            one(Value::Sym(SymArray::from_dims(vec![if n == 0 { 0 } else { 1 }, n], s.args.iter().map(|v| Expr::var(v)).collect())))
        }
        "series" => {
            let mut args = a.to_vec();
            if args.len() == 1 {
                if let Ok(e) = scalar_arg(a, 0) {
                    if let Some(v) = super::default_var(&e) {
                        args.push(sym_val(Expr::var(&v)));
                    }
                }
            }
            super::call(it, "taylor", args, nargout)
        }
        "simplifyFraction" => one(Value::Sym(sym_arg(a, 0)?.map(algebra::simplify))),
        "symtrue" => one(sym_val(Expr::rel(Rel::Eq, Expr::zero(), Expr::zero()))),
        "symfalse" => one(sym_val(Expr::rel(Rel::Eq, Expr::zero(), Expr::one()))),
        "divisors" => {
            let n = int_arg(&a[0])?.first().copied().unwrap_or(0).unsigned_abs();
            let d: Vec<Expr> = (1..=n).filter(|k| n % k == 0).map(|k| Expr::int(k as i128)).collect();
            let k = d.len();
            one(int_out(&a[0], vec![1, k], d))
        }
        "nextprime" | "prevprime" | "nthprime" | "fibonacci" | "harmonic" | "eulerPhi" | "bernoulli" => {
            let input = arg0(a)?;
            let ns = int_arg(input)?;
            let vals: Vec<Expr> = ns
                .iter()
                .map(|&n| -> R<Expr> {
                    Ok(match name {
                        "nextprime" => {
                            let mut k = (n.max(2)) as u64;
                            while !is_prime(k) {
                                k += 1;
                            }
                            Expr::int(k as i128)
                        }
                        "prevprime" => {
                            let mut k = n as u64;
                            while k >= 2 && !is_prime(k) {
                                k -= 1;
                            }
                            if k < 2 {
                                return rt("There is no prime number less than or equal to the input.");
                            }
                            Expr::int(k as i128)
                        }
                        "nthprime" => {
                            let mut count = 0;
                            let mut k = 1u64;
                            while count < n {
                                k += 1;
                                if is_prime(k) {
                                    count += 1;
                                }
                            }
                            Expr::int(k as i128)
                        }
                        "fibonacci" => {
                            let (mut x, mut y) = (BigInt::zero(), BigInt::one());
                            for _ in 0..n {
                                let z = &x + &y;
                                x = y;
                                y = z;
                            }
                            Expr::Num(Num::from_big(Q::from_integer(x)))
                        }
                        "harmonic" => q_expr(&(1..=n).fold(Q::zero(), |acc, k| acc + Q::new(BigInt::one(), BigInt::from(k)))),
                        "eulerPhi" => {
                            let mut m = n.unsigned_abs();
                            let mut phi = m;
                            let mut p = 2;
                            while p * p <= m {
                                if m % p == 0 {
                                    while m % p == 0 {
                                        m /= p;
                                    }
                                    phi -= phi / p;
                                }
                                p += 1;
                            }
                            if m > 1 {
                                phi -= phi / m;
                            }
                            Expr::int(phi as i128)
                        }
                        _ => q_expr(&bernoulli_q(n as usize)),
                    })
                })
                .collect::<R<_>>()?;
            one(int_out(input, input.dims(), vals))
        }
        "kroneckerDelta" => {
            let m = scalar_arg(a, 0)?;
            let n = if a.len() > 1 { scalar_arg(a, 1)? } else { Expr::zero() };
            let d = algebra::simplify(&Expr::sub(m.clone(), n.clone()));
            let r = match d.as_num() {
                Some(x) => Expr::int(if x.is_zero() { 1 } else { 0 }),
                None => Expr::func("kroneckerDelta", vec![m, n]),
            };
            one(sym_val(r))
        }
        "zeta" => {
            let input = arg0(a)?;
            if let Value::Sym(s) = input {
                return one(Value::Sym(s.map(|e| match e.as_num().and_then(|n| n.as_int()) {
                    // zeta(2k) = (-1)^(k+1) B_2k (2 pi)^(2k) / (2 (2k)!)
                    Some(n) if n > 0 && n % 2 == 0 => {
                        let b = bernoulli_q(n as usize);
                        let c = (if (n / 2) % 2 == 1 { b } else { -b }) * Q::from_integer(BigInt::from(2).pow(n as u32)) / (q_int(2) * factorial(n as u64));
                        Expr::mul(vec![q_expr(&c), Expr::pow(Expr::Const(Const::Pi), Expr::int(n))])
                    }
                    Some(0) => Expr::rat(-1, 2),
                    _ => Expr::func("zeta", vec![e.clone()]),
                })));
            }
            let m = input.to_matrix()?;
            one(Value::Num(Matrix::from_dims(m.dims.clone(), m.data.iter().map(|&s| zeta_f64(s)).collect())))
        }
        "lambertw" => {
            let input = a.last().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?;
            if let Value::Sym(s) = input {
                let branch = if a.len() > 1 { scalar_arg(a, 0)? } else { Expr::zero() };
                return one(Value::Sym(s.map(|e| if e.is_zero() { Expr::zero() } else { Expr::func("lambertw", vec![branch.clone(), e.clone()]) })));
            }
            let m = input.to_matrix()?;
            one(Value::Num(Matrix::from_dims(m.dims.clone(), m.data.iter().map(|&x| lambertw_f64(x)).collect())))
        }
        "sinc" => {
            let s = sym_arg(a, 0)?;
            one(Value::Sym(s.map(|e| {
                if e.is_zero() {
                    return Expr::one();
                }
                let px = Expr::mul(vec![Expr::Const(Const::Pi), e.clone()]);
                algebra::simplify(&Expr::div(Expr::func("sin", vec![px.clone()]), px))
            })))
        }
        _ => Ok(None),
    }
}

fn arg0(a: &[Value]) -> R<&Value> {
    a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))
}

trait PipeIfBigger {
    fn pipe_if_bigger(self, original: Expr) -> Expr;
}

impl PipeIfBigger for Expr {
    /// Keeps the transform's own form unless simplification makes it shorter.
    fn pipe_if_bigger(self, original: Expr) -> Expr {
        if self.to_string().len() < original.to_string().len() { self } else { original }
    }
}
