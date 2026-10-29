//! Differentiation, integration, limits and Taylor expansion.

use super::algebra::{expand, numden, poly_coeffs, simplify};
use super::expr::{Const, Expr, Num};

fn x_var(v: &str) -> Expr {
    Expr::var(v)
}

pub fn diff(e: &Expr, v: &str) -> Expr {
    if !e.contains_var(v) {
        return Expr::zero();
    }
    match e {
        Expr::Var(n) => {
            if n == v { Expr::one() } else { Expr::zero() }
        }
        Expr::Add(ts) => Expr::add(ts.iter().map(|t| diff(t, v)).collect()),
        Expr::Mul(fs) => {
            // Product rule.
            let mut terms = Vec::new();
            for i in 0..fs.len() {
                if !fs[i].contains_var(v) {
                    continue;
                }
                let mut f = fs.clone();
                f[i] = diff(&fs[i], v);
                terms.push(Expr::mul(f));
            }
            Expr::add(terms)
        }
        Expr::Pow(b, x) => {
            if !x.contains_var(v) {
                // d(b^n) = n b^(n-1) b'
                Expr::mul(vec![(**x).clone(), Expr::pow((**b).clone(), Expr::sub((**x).clone(), Expr::one())), diff(b, v)])
            } else {
                // d(b^x) = b^x (x' log b + x b'/b)
                Expr::mul(vec![
                    e.clone(),
                    Expr::add(vec![
                        Expr::mul(vec![diff(x, v), Expr::func("log", vec![(**b).clone()])]),
                        Expr::mul(vec![(**x).clone(), diff(b, v), Expr::pow((**b).clone(), Expr::int(-1))]),
                    ]),
                ])
            }
        }
        Expr::Fn(name, args) if args.len() == 1 => {
            let u = &args[0];
            let du = diff(u, v);
            let outer = match name.as_str() {
                "sin" => Expr::func("cos", vec![u.clone()]),
                "cos" => Expr::neg(Expr::func("sin", vec![u.clone()])),
                "tan" => Expr::add(vec![Expr::one(), Expr::pow(Expr::func("tan", vec![u.clone()]), Expr::int(2))]),
                "exp" => e.clone(),
                "log" => Expr::pow(u.clone(), Expr::int(-1)),
                "asin" => Expr::pow(Expr::sub(Expr::one(), Expr::pow(u.clone(), Expr::int(2))), Expr::rat(-1, 2)),
                "acos" => Expr::neg(Expr::pow(Expr::sub(Expr::one(), Expr::pow(u.clone(), Expr::int(2))), Expr::rat(-1, 2))),
                "atan" => Expr::pow(Expr::add(vec![Expr::one(), Expr::pow(u.clone(), Expr::int(2))]), Expr::int(-1)),
                "sinh" => Expr::func("cosh", vec![u.clone()]),
                "cosh" => Expr::func("sinh", vec![u.clone()]),
                "tanh" => Expr::sub(Expr::one(), Expr::pow(Expr::func("tanh", vec![u.clone()]), Expr::int(2))),
                "abs" => Expr::func("sign", vec![u.clone()]),
                "sign" | "floor" | "ceil" | "round" | "fix" => Expr::zero(),
                "heaviside" => Expr::func("dirac", vec![u.clone()]),
                // d/du erf(u) = 2/sqrt(pi) * exp(-u^2); erfc is its negative.
                "erf" | "erfc" => {
                    let g = Expr::mul(vec![Expr::int(2), Expr::pow(Expr::Const(Const::Pi), Expr::rat(-1, 2)), Expr::func("exp", vec![Expr::neg(Expr::pow(u.clone(), Expr::int(2)))])]);
                    if name == "erf" { g } else { Expr::neg(g) }
                }
                _ => return Expr::Fn("diff".into(), vec![e.clone(), x_var(v)]),
            };
            Expr::mul(vec![outer, du])
        }
        _ => Expr::Fn("diff".into(), vec![e.clone(), x_var(v)]),
    }
}

/// `a*x + b` → (a, b) when `e` is linear in `v`.
fn linear(e: &Expr, v: &str) -> Option<(Expr, Expr)> {
    let c = poly_coeffs(e, v)?;
    if c.len() == 2 && !c[1].is_zero() {
        Some((c[1].clone(), c[0].clone()))
    } else {
        None
    }
}

/// Antiderivative, or `None` when no rule applies.
pub fn integrate(e: &Expr, v: &str) -> Option<Expr> {
    let x = x_var(v);
    if !e.contains_var(v) {
        return Some(Expr::mul(vec![e.clone(), x]));
    }
    match e {
        Expr::Var(_) => return Some(Expr::div(Expr::pow(x, Expr::int(2)), Expr::int(2))),
        Expr::Add(ts) => {
            let mut parts = Vec::new();
            for t in ts {
                parts.push(integrate(t, v)?);
            }
            return Some(Expr::add(parts));
        }
        _ => {}
    }
    // Constant factors out.
    if let Expr::Mul(fs) = e {
        let (consts, rest): (Vec<Expr>, Vec<Expr>) = fs.iter().cloned().partition(|f| !f.contains_var(v));
        if !consts.is_empty() {
            return Some(Expr::mul(vec![Expr::mul(consts), integrate(&Expr::mul(rest), v)?]));
        }
    }
    if let Some(r) = integrate_simple(e, v) {
        return Some(r);
    }
    // Products with a sum: expand and integrate term by term.
    let ex = expand(e);
    if ex != *e && matches!(ex, Expr::Add(_)) {
        return integrate(&ex, v);
    }
    // Integration by parts for x^n * f(a x + b) with f in {exp, sin, cos} and x^n*log(x).
    if let Expr::Mul(fs) = e {
        let poly: Vec<&Expr> = fs.iter().filter(|f| poly_coeffs(f, v).is_some()).collect();
        let other: Vec<&Expr> = fs.iter().filter(|f| poly_coeffs(f, v).is_none()).collect();
        if !poly.is_empty() && other.len() == 1 {
            let p = Expr::mul(poly.into_iter().cloned().collect());
            let g = other[0].clone();
            if let Expr::Fn(name, args) = &g {
                if matches!(name.as_str(), "exp" | "sin" | "cos") && linear(&args[0], v).is_some() {
                    // ∫ p g = p G - ∫ p' G
                    let big_g = integrate_simple(&g, v)?;
                    let rest = integrate(&Expr::mul(vec![diff(&p, v), big_g.clone()]), v)?;
                    return Some(Expr::sub(Expr::mul(vec![p, big_g]), rest));
                }
                if name == "log" && args[0] == Expr::var(v) {
                    // ∫ p log x = P log x - ∫ P / x
                    let big_p = integrate(&p, v)?;
                    let rest = integrate(&expand(&Expr::div(big_p.clone(), Expr::var(v))), v)?;
                    return Some(Expr::sub(Expr::mul(vec![big_p, g.clone()]), rest));
                }
            }
        }
    }
    None
}

/// Table integrals with linear inner arguments.
fn integrate_simple(e: &Expr, v: &str) -> Option<Expr> {
    // 1/(1 + x^2) → atan(x); 1/sqrt(1 - x^2) → asin(x)
    if let Expr::Pow(b, n) = e {
        if let Some(c) = poly_coeffs(b, v) {
            if c.len() == 3 && c[1].is_zero() && c[0].is_one() {
                if **n == Expr::int(-1) && c[2].is_one() {
                    return Some(Expr::func("atan", vec![Expr::var(v)]));
                }
                if **n == Expr::rat(-1, 2) && c[2] == Expr::int(-1) {
                    return Some(Expr::func("asin", vec![Expr::var(v)]));
                }
            }
        }
    }
    match e {
        Expr::Pow(b, n) if !n.contains_var(v) => {
            let (a, _) = linear(b, v)?;
            if **n == Expr::int(-1) {
                return Some(Expr::div(Expr::func("log", vec![(**b).clone()]), a));
            }
            let n1 = Expr::add(vec![(**n).clone(), Expr::one()]);
            // ∫ 1/(1 + x^2) handled below; here (a x + b)^n.
            Some(Expr::div(Expr::pow((**b).clone(), n1.clone()), Expr::mul(vec![a, n1])))
        }
        Expr::Pow(b, x) if !b.contains_var(v) => {
            // a^(c x + d)
            let (c, _) = linear(x, v)?;
            Some(Expr::div(e.clone(), Expr::mul(vec![c, Expr::func("log", vec![(**b).clone()])])))
        }
        Expr::Fn(name, args) if args.len() == 1 => {
            let u = &args[0];
            let (a, _) = linear(u, v)?;
            let f = match name.as_str() {
                "exp" => e.clone(),
                "sin" => Expr::neg(Expr::func("cos", vec![u.clone()])),
                "cos" => Expr::func("sin", vec![u.clone()]),
                "tan" => Expr::neg(Expr::func("log", vec![Expr::func("cos", vec![u.clone()])])),
                "sinh" => Expr::func("cosh", vec![u.clone()]),
                "cosh" => Expr::func("sinh", vec![u.clone()]),
                "log" => Expr::sub(Expr::mul(vec![u.clone(), Expr::func("log", vec![u.clone()])]), u.clone()),
                _ => return None,
            };
            Some(Expr::div(f, a))
        }
        _ => None,
    }
}

/// Normal form of a calculus result: polynomials are expanded (as MATLAB returns them);
/// anything else keeps its canonical form rather than being simplified.
pub fn tidy(e: &Expr) -> Expr {
    let mut vars = Vec::new();
    e.free_vars(&mut vars);
    if vars.iter().all(|v| poly_coeffs(e, v).is_some()) {
        expand(e)
    } else {
        e.clone()
    }
}

/// `int(f, x)` (unevaluated when no rule applies, as MATLAB does).
pub fn int_indef(e: &Expr, v: &str) -> Expr {
    match integrate(e, v) {
        Some(r) => tidy(&r),
        None => Expr::Fn("int".into(), vec![e.clone(), Expr::var(v)]),
    }
}

pub fn int_def(e: &Expr, v: &str, a: &Expr, b: &Expr) -> Expr {
    let f = int_indef(e, v);
    if matches!(&f, Expr::Fn(n, _) if n == "int") {
        return Expr::Fn("int".into(), vec![e.clone(), Expr::var(v), a.clone(), b.clone()]);
    }
    let at = |p: &Expr| -> Expr {
        if is_inf(p) || is_neg_inf(p) {
            limit(&f, v, p, 0)
        } else {
            f.subs(v, p)
        }
    };
    simplify(&Expr::sub(at(b), at(a)))
}

pub fn is_inf(e: &Expr) -> bool {
    matches!(e, Expr::Const(Const::Inf))
}

pub fn is_neg_inf(e: &Expr) -> bool {
    matches!(e, Expr::Mul(f) if f.len() == 2 && f[0] == Expr::int(-1) && is_inf(&f[1]))
}

fn finite_value(e: &Expr) -> bool {
    match e.eval_c(&|_| None) {
        Some(z) => z.re.is_finite() && z.im.is_finite(),
        None => !contains_const(e, Const::Inf) && !contains_const(e, Const::NaN),
    }
}

fn contains_const(e: &Expr, c: Const) -> bool {
    match e {
        Expr::Const(k) => *k == c,
        Expr::Add(xs) | Expr::Mul(xs) | Expr::Fn(_, xs) => xs.iter().any(|x| contains_const(x, c)),
        Expr::Pow(a, b) | Expr::Rel(_, a, b) => contains_const(a, c) || contains_const(b, c),
        _ => false,
    }
}

/// `limit(f, x, a)`; `dir` is -1 / 0 / +1 for left / two-sided / right.
pub fn limit(e: &Expr, v: &str, a: &Expr, dir: i32) -> Expr {
    if is_inf(a) || is_neg_inf(a) {
        // Rational functions: compare leading terms.
        let (n, d) = numden(e);
        if let (Some(pn), Some(pd)) = (poly_coeffs(&n, v), poly_coeffs(&d, v)) {
            let (dn, dd) = (pn.len() as i64 - 1, pd.len() as i64 - 1);
            let lead = simplify(&Expr::div(pn.last().unwrap().clone(), pd.last().unwrap().clone()));
            if dn < dd {
                return Expr::zero();
            }
            if dn == dd {
                return lead;
            }
            let sign = lead.to_f64().unwrap_or(1.0).signum() * if is_neg_inf(a) && (dn - dd) % 2 == 1 { -1.0 } else { 1.0 };
            return if sign >= 0.0 { Expr::Const(Const::Inf) } else { Expr::neg(Expr::Const(Const::Inf)) };
        }
        // Substitute x = 1/t (t → 0 from the right).
        let t = format!("{v}__t");
        let sub = e.subs(v, &Expr::pow(Expr::var(&t), Expr::int(-1)));
        let sub = if is_neg_inf(a) { e.subs(v, &Expr::neg(Expr::pow(Expr::var(&t), Expr::int(-1)))) } else { sub };
        return limit(&simplify(&sub), &t, &Expr::zero(), 1);
    }
    let direct = simplify(&e.subs(v, a));
    if finite_value(&direct) && !contains_const(&direct, Const::NaN) {
        return direct;
    }
    // L'Hôpital on quotients of forms 0/0 or ∞/∞.
    let (mut n, mut d) = numden(e);
    for _ in 0..8 {
        let nv = simplify(&n.subs(v, a));
        let dv = simplify(&d.subs(v, a));
        let n_zero = nv.is_zero();
        let d_zero = dv.is_zero();
        if !d_zero && finite_value(&dv) {
            return simplify(&Expr::div(nv, dv));
        }
        if d_zero && !n_zero && finite_value(&nv) {
            // c/0: one-sided infinity.
            let s = nv.to_f64().unwrap_or(1.0).signum();
            let approach = d.subs(v, &Expr::add(vec![a.clone(), Expr::Num(Num::F(if dir < 0 { -1e-9 } else { 1e-9 }))])).to_f64().unwrap_or(1.0).signum();
            if dir == 0 {
                let other = d.subs(v, &Expr::add(vec![a.clone(), Expr::Num(Num::F(-1e-9))])).to_f64().unwrap_or(1.0).signum();
                if other != approach {
                    return Expr::Const(Const::NaN);
                }
            }
            return if s * approach > 0.0 { Expr::Const(Const::Inf) } else { Expr::neg(Expr::Const(Const::Inf)) };
        }
        n = simplify(&diff(&n, v));
        d = simplify(&diff(&d, v));
    }
    Expr::Fn("limit".into(), vec![e.clone(), Expr::var(v), a.clone()])
}

pub fn taylor(e: &Expr, v: &str, a: &Expr, order: usize) -> Expr {
    let mut terms = Vec::new();
    let mut d = e.clone();
    let mut fact = Num::int(1);
    let dx = if a.is_zero() { Expr::var(v) } else { Expr::sub(Expr::var(v), a.clone()) };
    for k in 0..order {
        if k > 0 {
            fact = fact.mul(Num::int(k as i128));
            d = simplify(&diff(&d, v));
        }
        let c = simplify(&d.subs(v, a));
        if !c.is_zero() {
            terms.push(Expr::mul(vec![c, Expr::Num(fact.recip()), Expr::pow(dx.clone(), Expr::int(k as i128))]));
        }
    }
    Expr::add(terms)
}
