//! Expansion, polynomials, rational functions, factoring, simplification and solving.

use super::expr::{Const, Expr, Num};
use crate::cplx::{poly_roots, C};

// ---- expansion ------------------------------------------------------------------------------

fn terms(e: &Expr) -> Vec<Expr> {
    match e {
        Expr::Add(t) => t.clone(),
        other => vec![other.clone()],
    }
}

fn multiply_out(a: &Expr, b: &Expr) -> Expr {
    let ta = terms(a);
    let tb = terms(b);
    let mut out = Vec::with_capacity(ta.len() * tb.len());
    for x in &ta {
        for y in &tb {
            out.push(Expr::mul(vec![x.clone(), y.clone()]));
        }
    }
    Expr::add(out)
}

pub fn expand(e: &Expr) -> Expr {
    match e {
        Expr::Add(ts) => Expr::add(ts.iter().map(expand).collect()),
        Expr::Mul(fs) => {
            let mut acc = Expr::one();
            for f in fs {
                acc = multiply_out(&acc, &expand(f));
            }
            acc
        }
        Expr::Pow(b, n) => {
            let base = expand(b);
            match n.as_num().and_then(|k| k.as_int()) {
                Some(k) if k > 1 && k <= 32 && matches!(base, Expr::Add(_)) => {
                    let mut acc = base.clone();
                    for _ in 1..k {
                        acc = multiply_out(&acc, &base);
                    }
                    acc
                }
                _ => Expr::pow(base, (**n).clone()),
            }
        }
        Expr::Fn(name, args) => Expr::func(name, args.iter().map(expand).collect()),
        Expr::Rel(op, l, r) => Expr::rel(*op, expand(l), expand(r)),
        other => other.clone(),
    }
}

// ---- polynomials ----------------------------------------------------------------------------

/// Degree of a term in `v` and its coefficient, or `None` when the term is not a monomial.
fn term_degree(t: &Expr, v: &str) -> Option<(usize, Expr)> {
    if !t.contains_var(v) {
        return Some((0, t.clone()));
    }
    let factors = match t {
        Expr::Mul(f) => f.clone(),
        other => vec![other.clone()],
    };
    let mut deg = 0usize;
    let mut coeff = Vec::new();
    for f in factors {
        match &f {
            Expr::Var(n) if n == v => deg += 1,
            Expr::Pow(b, n) if matches!(&**b, Expr::Var(x) if x == v) => {
                let k = n.as_num()?.as_int()?;
                if k < 0 {
                    return None;
                }
                deg += k as usize;
            }
            other if !other.contains_var(v) => coeff.push(f.clone()),
            _ => return None,
        }
    }
    Some((deg, Expr::mul(coeff)))
}

/// Coefficients (ascending powers) of `e` as a polynomial in `v`.
pub fn poly_coeffs(e: &Expr, v: &str) -> Option<Vec<Expr>> {
    let ex = expand(e);
    let mut out: Vec<Vec<Expr>> = Vec::new();
    for t in terms(&ex) {
        let (d, c) = term_degree(&t, v)?;
        if out.len() <= d {
            out.resize(d + 1, Vec::new());
        }
        out[d].push(c);
    }
    if out.is_empty() {
        return Some(vec![Expr::zero()]);
    }
    let mut coeffs: Vec<Expr> = out.into_iter().map(Expr::add).collect();
    while coeffs.len() > 1 && coeffs.last().map(|c| c.is_zero()).unwrap_or(false) {
        coeffs.pop();
    }
    Some(coeffs)
}

pub fn from_coeffs(c: &[Expr], v: &str) -> Expr {
    Expr::add(c.iter().enumerate().map(|(k, a)| Expr::mul(vec![a.clone(), Expr::pow(Expr::var(v), Expr::int(k as i128))])).collect())
}

/// Rational-coefficient polynomial (ascending), if every coefficient is an exact number.
fn rat_poly(e: &Expr, v: &str) -> Option<Vec<Num>> {
    let c = poly_coeffs(e, v)?;
    c.iter().map(|x| match x.as_num() {
        Some(n @ Num::Rat(..)) => Some(n),
        _ => None,
    }).collect()
}

fn trim(p: &mut Vec<Num>) {
    while p.len() > 1 && p.last().map(|x| x.is_zero()).unwrap_or(false) {
        p.pop();
    }
}

fn poly_divmod(a: &[Num], b: &[Num]) -> (Vec<Num>, Vec<Num>) {
    let mut r = a.to_vec();
    trim(&mut r);
    let mut b = b.to_vec();
    trim(&mut b);
    if b.len() > r.len() {
        return (vec![Num::int(0)], r);
    }
    let lead = *b.last().unwrap();
    let mut q = vec![Num::int(0); r.len() - b.len() + 1];
    while r.len() >= b.len() && !(r.len() == 1 && r[0].is_zero()) {
        let k = r.len() - b.len();
        let f = r.last().unwrap().mul(lead.recip());
        q[k] = f;
        for (i, &bi) in b.iter().enumerate() {
            r[i + k] = r[i + k].add(f.mul(bi).neg());
        }
        r.pop();
        if r.is_empty() {
            r.push(Num::int(0));
        }
        trim(&mut r);
        if r.len() < b.len() {
            break;
        }
    }
    (q, r)
}

fn poly_gcd(a: &[Num], b: &[Num]) -> Vec<Num> {
    let mut x = a.to_vec();
    let mut y = b.to_vec();
    trim(&mut x);
    trim(&mut y);
    while !(y.len() == 1 && y[0].is_zero()) {
        let (_, r) = poly_divmod(&x, &y);
        x = y;
        y = r;
    }
    let lead = *x.last().unwrap();
    if lead.is_zero() {
        return vec![Num::int(1)];
    }
    x.iter().map(|c| c.mul(lead.recip())).collect()
}

fn num_poly_expr(p: &[Num], v: &str) -> Expr {
    from_coeffs(&p.iter().map(|n| Expr::Num(*n)).collect::<Vec<_>>(), v)
}

// ---- rational functions ---------------------------------------------------------------------

/// Numerator and denominator over a common denominator.
pub fn numden(e: &Expr) -> (Expr, Expr) {
    match e {
        Expr::Num(Num::Rat(p, q)) => (Expr::int(*p), Expr::int(*q)),
        Expr::Pow(b, n) if n.as_num().map(|k| k.is_neg()).unwrap_or(false) => (Expr::one(), Expr::pow((**b).clone(), Expr::Num(n.as_num().unwrap().neg()))),
        Expr::Mul(fs) => {
            let mut ns = Vec::new();
            let mut ds = Vec::new();
            for f in fs {
                let (n, d) = numden(f);
                ns.push(n);
                ds.push(d);
            }
            (Expr::mul(ns), Expr::mul(ds))
        }
        Expr::Add(ts) => {
            let mut n = Expr::zero();
            let mut d = Expr::one();
            for t in ts {
                let (tn, td) = numden(t);
                if td == d {
                    n = Expr::add(vec![n, tn]);
                } else if td.is_one() {
                    n = Expr::add(vec![n, Expr::mul(vec![tn, d.clone()])]);
                } else {
                    n = Expr::add(vec![Expr::mul(vec![n, td.clone()]), Expr::mul(vec![tn, d.clone()])]);
                    d = Expr::mul(vec![d, td]);
                }
            }
            (expand(&n), d)
        }
        other => (other.clone(), Expr::one()),
    }
}

/// Combines over a common denominator and cancels common univariate polynomial factors.
pub fn together(e: &Expr) -> Expr {
    let (n, d) = numden(e);
    if d.is_one() {
        return n;
    }
    let mut vars = Vec::new();
    n.free_vars(&mut vars);
    d.free_vars(&mut vars);
    if vars.len() == 1 {
        let v = &vars[0];
        if let (Some(pn), Some(pd)) = (rat_poly(&n, v), rat_poly(&expand(&d), v)) {
            let g = poly_gcd(&pn, &pd);
            if g.len() > 1 {
                let (qn, _) = poly_divmod(&pn, &g);
                let (qd, _) = poly_divmod(&pd, &g);
                let den = num_poly_expr(&qd, v);
                let num = num_poly_expr(&qn, v);
                return Expr::div(num, den);
            }
        }
    }
    Expr::div(n, d)
}

// ---- factoring ------------------------------------------------------------------------------

fn int_divisors(n: i128) -> Vec<i128> {
    let n = n.abs();
    if n == 0 {
        return vec![1];
    }
    let mut out = Vec::new();
    let mut i = 1;
    while i * i <= n && i < 100_000 {
        if n % i == 0 {
            out.push(i);
            if i != n / i {
                out.push(n / i);
            }
        }
        i += 1;
    }
    out
}

fn eval_poly(p: &[Num], x: Num) -> Num {
    p.iter().rev().fold(Num::int(0), |acc, &c| acc.mul(x).add(c))
}

pub fn prime_factors(mut n: i128) -> Vec<i128> {
    let mut out = Vec::new();
    if n < 0 {
        out.push(-1);
        n = -n;
    }
    let mut f = 2;
    while f * f <= n {
        while n % f == 0 {
            out.push(f);
            n /= f;
        }
        f += 1;
    }
    if n > 1 {
        out.push(n);
    }
    out
}

/// Factors as MATLAB's `factor` returns them (constant first, then irreducible factors).
pub fn factor_list(e: &Expr) -> Vec<Expr> {
    if let Some(Num::Rat(p, 1)) = e.as_num() {
        return prime_factors(p).into_iter().map(Expr::int).collect();
    }
    let (n, d) = numden(e);
    let mut out = factor_poly(&n);
    if !d.is_one() {
        for f in factor_poly(&expand(&d)) {
            out.push(Expr::pow(f, Expr::int(-1)));
        }
    }
    out
}

fn factor_poly(e: &Expr) -> Vec<Expr> {
    let e = expand(e);
    let mut vars = Vec::new();
    e.free_vars(&mut vars);
    vars.sort();
    // Common monomial factors (e.g. x*y + x = x*(y + 1)).
    let mut out = Vec::new();
    let mut rest = e.clone();
    for v in &vars {
        if let Some(c) = poly_coeffs(&rest, v) {
            let k = c.iter().position(|x| !x.is_zero()).unwrap_or(0);
            if k > 0 && c.len() > 1 {
                for _ in 0..k {
                    out.push(Expr::var(v));
                }
                rest = expand(&from_coeffs(&c[k..], v));
            }
        }
    }
    if vars.len() == 1 {
        if let Some(mut p) = rat_poly(&rest, &vars[0]) {
            let v = &vars[0];
            // Clear denominators and pull out the integer content.
            let lcm = p.iter().fold(1i128, |acc, c| match c {
                Num::Rat(_, q) => acc / gcd_i(acc, *q) * *q,
                _ => acc,
            });
            let mut ip: Vec<i128> = p.iter().map(|c| c.mul(Num::int(lcm)).as_int().unwrap_or(0)).collect();
            let content = ip.iter().fold(0i128, |g, &c| gcd_i(g, c)).max(1) * if *ip.last().unwrap() < 0 { -1 } else { 1 };
            ip.iter_mut().for_each(|c| *c /= content);
            let constant = Num::rat(content, lcm);
            if !constant.is_one() {
                out.insert(0, Expr::Num(constant));
            }
            p = ip.iter().map(|&c| Num::int(c)).collect();
            // Rational roots p/q: p | a0, q | an.
            loop {
                if p.len() <= 2 {
                    break;
                }
                let a0 = p[0].as_int().unwrap_or(0);
                let an = p.last().unwrap().as_int().unwrap_or(1);
                let mut found = None;
                'search: for q in int_divisors(an) {
                    for r in int_divisors(a0) {
                        for s in [r, -r] {
                            let root = Num::rat(s, q);
                            if eval_poly(&p, root).is_zero() {
                                found = Some((s, q));
                                break 'search;
                            }
                        }
                    }
                }
                let Some((s, q)) = found else { break };
                // Divide by (q x - s).
                let lin = vec![Num::int(-s), Num::int(q)];
                let (quot, _) = poly_divmod(&p, &lin);
                out.push(num_poly_expr(&lin, v));
                p = quot;
            }
            if p.len() == 2 {
                out.push(num_poly_expr(&p, v));
            } else if p.len() > 2 || (p.len() == 1 && !p[0].is_one()) {
                out.push(num_poly_expr(&p, v));
            }
            if out.is_empty() {
                out.push(Expr::one());
            }
            // Keep a leading constant first.
            if let Some(i) = out.iter().position(|f| f.as_num().is_some()) {
                let c = out.remove(i);
                out.insert(0, c);
            }
            return out;
        }
    }
    if !rest.is_one() {
        out.push(rest);
    }
    if out.is_empty() {
        out.push(Expr::one());
    }
    out
}

fn gcd_i(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// Product form of the factorization with repeated factors as powers: (x + 1)^2.
pub fn factored(e: &Expr) -> Expr {
    let fs = factor_list(e);
    let mut grouped: Vec<(Expr, i128)> = Vec::new();
    for f in fs {
        match grouped.iter_mut().find(|(g, _)| *g == f) {
            Some((_, k)) => *k += 1,
            None => grouped.push((f, 1)),
        }
    }
    // Build without letting a numeric coefficient distribute over a single sum.
    let parts: Vec<Expr> = grouped.into_iter().map(|(f, k)| if k == 1 { f } else { Expr::Pow(Box::new(f), Box::new(Expr::int(k))) }).collect();
    if parts.len() == 1 {
        return parts.into_iter().next().unwrap();
    }
    Expr::Mul(parts)
}

// ---- simplification -------------------------------------------------------------------------

/// sin(u)^2 + cos(u)^2 → 1 (with matching coefficients).
fn pythagorean(e: &Expr) -> Expr {
    let Expr::Add(ts) = e else { return e.clone() };
    let mut ts = ts.clone();
    let sq = |t: &Expr, f: &str| -> Option<(Num, Expr)> {
        let (c, r) = t.split_coeff();
        if let Expr::Pow(b, n) = &r {
            if **n == Expr::int(2) {
                if let Expr::Fn(name, a) = &**b {
                    if name == f {
                        return Some((c, a[0].clone()));
                    }
                }
            }
        }
        None
    };
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for i in 0..ts.len() {
            if let Some((c1, u1)) = sq(&ts[i], "sin") {
                for j in 0..ts.len() {
                    if let Some((c2, u2)) = sq(&ts[j], "cos") {
                        if u1 == u2 && c1 == c2 {
                            let (a, b) = (i.max(j), i.min(j));
                            ts.remove(a);
                            ts.remove(b);
                            ts.push(Expr::Num(c1));
                            changed = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
    Expr::add(ts)
}

fn size(e: &Expr) -> usize {
    e.to_string().len()
}

pub fn simplify(e: &Expr) -> Expr {
    match e {
        Expr::Num(_) | Expr::Var(_) | Expr::Const(_) => return e.clone(),
        Expr::Rel(op, l, r) => {
            let d = simplify(&Expr::sub((**l).clone(), (**r).clone()));
            if d.is_zero() {
                return Expr::rel(*op, simplify(l), simplify(r));
            }
            return Expr::rel(*op, simplify(l), simplify(r));
        }
        _ => {}
    }
    // Simplify inside function arguments first.
    let inner = e.map(&|x| match x {
        Expr::Fn(n, a) => Some(Expr::func(n, a.iter().map(simplify).collect())),
        _ => None,
    });
    let mut best = inner.clone();
    let mut consider = |c: Expr| {
        if size(&c) < size(&best) {
            best = c;
        }
    };
    let ex = pythagorean(&expand(&inner));
    consider(ex.clone());
    consider(pythagorean(&inner));
    if size(&inner) < 400 {
        let t = together(&ex);
        consider(t.clone());
        let mut vars = Vec::new();
        inner.free_vars(&mut vars);
        if vars.len() <= 2 {
            consider(factored(&t));
        }
    }
    best
}

pub fn collect(e: &Expr, v: &str) -> Expr {
    match poly_coeffs(e, v) {
        Some(c) => {
            let parts: Vec<Expr> = c
                .iter()
                .enumerate()
                .filter(|(_, a)| !a.is_zero())
                .map(|(k, a)| {
                    let a = simplify(a);
                    let xk = Expr::pow(Expr::var(v), Expr::int(k as i128));
                    // Keep a symbolic coefficient grouped: (y + 1)*x^2.
                    if matches!(a, Expr::Add(_)) { Expr::Mul(vec![a, xk]) } else { Expr::mul(vec![a, xk]) }
                })
                .collect();
            Expr::add(parts)
        }
        None => e.clone(),
    }
}

// ---- solving --------------------------------------------------------------------------------

fn complex_expr(z: C) -> Expr {
    let re = if z.re.abs() < 1e-14 { Expr::zero() } else { Expr::Num(Num::F(z.re)) };
    if z.im.abs() < 1e-14 {
        return re;
    }
    Expr::add(vec![re, Expr::mul(vec![Expr::Num(Num::F(z.im)), Expr::Const(Const::I)])])
}

/// Roots of `e == 0` in `v`.
pub fn solve_one(e: &Expr, v: &str) -> Option<Vec<Expr>> {
    let (n, d) = numden(&expand(e));
    let roots = solve_poly(&n, v).or_else(|| invert(&n, v).map(|r| vec![r]))?;
    // Drop roots that make the denominator vanish.
    Some(roots.into_iter().filter(|r| !simplify(&d.subs(v, r)).is_zero()).collect())
}

fn solve_poly(e: &Expr, v: &str) -> Option<Vec<Expr>> {
    let c = poly_coeffs(e, v)?;
    if c.iter().any(|x| x.contains_var(v)) {
        return None;
    }
    match c.len() {
        0 | 1 => Some(Vec::new()),
        2 => Some(vec![simplify(&Expr::neg(Expr::div(c[0].clone(), c[1].clone())))]),
        3 => {
            let (a, b, cc) = (c[2].clone(), c[1].clone(), c[0].clone());
            let disc = simplify(&Expr::sub(Expr::pow(b.clone(), Expr::int(2)), Expr::mul(vec![Expr::int(4), a.clone(), cc])));
            let s = Expr::func("sqrt", vec![disc]);
            let two_a = Expr::mul(vec![Expr::int(2), a]);
            let r1 = simplify(&Expr::div(Expr::sub(Expr::neg(b.clone()), s.clone()), two_a.clone()));
            let r2 = simplify(&Expr::div(Expr::add(vec![Expr::neg(b), s]), two_a));
            let mut roots = vec![r1, r2];
            if let (Some(x), Some(y)) = (roots[0].to_f64(), roots[1].to_f64()) {
                if x > y {
                    roots.swap(0, 1);
                }
            }
            Some(roots)
        }
        _ => {
            // Exact linear factors first, then the remaining factor.
            let fs = factor_list(e);
            if fs.len() > 1 {
                let mut out = Vec::new();
                for f in fs {
                    if f.contains_var(v) {
                        out.extend(solve_poly(&f, v)?);
                    }
                }
                return Some(out);
            }
            let nums: Option<Vec<f64>> = c.iter().map(|x| x.to_f64()).collect();
            let nums = nums?;
            let roots = poly_roots(&nums.iter().rev().map(|&x| C::new(x, 0.0)).collect::<Vec<_>>());
            let mut out: Vec<Expr> = roots.into_iter().map(complex_expr).collect();
            out.reverse();
            Some(out)
        }
    }
}

/// Isolates `v` when it appears once inside invertible functions.
fn invert(e: &Expr, v: &str) -> Option<Expr> {
    // e = lhs(v) + rest, solve lhs(v) = -rest.
    let (with_v, without): (Vec<Expr>, Vec<Expr>) = match e {
        Expr::Add(ts) => ts.iter().cloned().partition(|t| t.contains_var(v)),
        other => (vec![other.clone()], Vec::new()),
    };
    if with_v.len() != 1 {
        return None;
    }
    let mut lhs = with_v.into_iter().next().unwrap();
    let mut rhs = Expr::neg(Expr::add(without));
    for _ in 0..16 {
        match lhs {
            Expr::Var(ref n) if n == v => return Some(simplify(&rhs)),
            Expr::Mul(fs) => {
                let (consts, rest): (Vec<Expr>, Vec<Expr>) = fs.into_iter().partition(|f| !f.contains_var(v));
                if rest.len() != 1 {
                    return None;
                }
                rhs = Expr::div(rhs, Expr::mul(consts));
                lhs = rest.into_iter().next().unwrap();
            }
            Expr::Pow(b, n) if !n.contains_var(v) => {
                rhs = Expr::pow(rhs, Expr::pow((*n).clone(), Expr::int(-1)));
                lhs = *b;
            }
            Expr::Pow(b, n) if !b.contains_var(v) => {
                rhs = Expr::div(Expr::func("log", vec![rhs]), Expr::func("log", vec![(*b).clone()]));
                lhs = *n;
            }
            Expr::Fn(name, args) if args.len() == 1 => {
                let inv = match name.as_str() {
                    "exp" => "log",
                    "log" => "exp",
                    "sin" => "asin",
                    "cos" => "acos",
                    "tan" => "atan",
                    "sinh" => "asinh",
                    "tanh" => "atanh",
                    _ => return None,
                };
                rhs = Expr::func(inv, vec![rhs]);
                lhs = args.into_iter().next().unwrap();
            }
            Expr::Add(_) => {
                return invert(&Expr::sub(lhs, rhs), v);
            }
            _ => return None,
        }
    }
    None
}

/// Solves a linear system `eqs == 0` for `vars` by Gaussian elimination.
pub fn solve_linear(eqs: &[Expr], vars: &[String]) -> Option<Vec<Expr>> {
    let n = vars.len();
    if eqs.len() < n {
        return None;
    }
    let mut a: Vec<Vec<Expr>> = Vec::new();
    let mut b: Vec<Expr> = Vec::new();
    for e in eqs {
        let e = expand(e);
        let mut row = Vec::new();
        let mut zeroed = e.clone();
        for v in vars {
            let c = poly_coeffs(&e, v)?;
            // Linear: degree ≤ 1 and a coefficient free of every unknown.
            if c.len() > 2 || c.get(1).map(|x| vars.iter().any(|w| x.contains_var(w))).unwrap_or(false) {
                return None;
            }
            row.push(c.get(1).cloned().unwrap_or_else(Expr::zero));
            zeroed = zeroed.subs(v, &Expr::zero());
        }
        a.push(row);
        b.push(Expr::neg(zeroed));
    }
    let m = a.len();
    let mut r = 0;
    let mut pivots = Vec::new();
    for col in 0..n {
        let Some(p) = (r..m).find(|&i| !simplify(&a[i][col]).is_zero()) else { continue };
        a.swap(r, p);
        b.swap(r, p);
        let piv = a[r][col].clone();
        for k in col..n {
            a[r][k] = simplify(&Expr::div(a[r][k].clone(), piv.clone()));
        }
        b[r] = simplify(&Expr::div(b[r].clone(), piv));
        for i in 0..m {
            if i != r {
                let f = a[i][col].clone();
                if f.is_zero() {
                    continue;
                }
                for k in col..n {
                    a[i][k] = simplify(&Expr::sub(a[i][k].clone(), Expr::mul(vec![f.clone(), a[r][k].clone()])));
                }
                b[i] = simplify(&Expr::sub(b[i].clone(), Expr::mul(vec![f, b[r].clone()])));
            }
        }
        pivots.push(col);
        r += 1;
    }
    if pivots.len() < n {
        return None;
    }
    Some(b.into_iter().take(n).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x() -> Expr {
        Expr::var("x")
    }

    #[test]
    fn expand_and_factor() {
        let e = Expr::pow(Expr::add(vec![x(), Expr::one()]), Expr::int(2));
        assert_eq!(expand(&e).to_string(), "x^2 + 2*x + 1");
        let p = Expr::sub(Expr::pow(x(), Expr::int(2)), Expr::one());
        let fs: Vec<String> = factor_list(&p).iter().map(|f| f.to_string()).collect();
        assert_eq!(fs, vec!["x - 1", "x + 1"]);
        assert_eq!(simplify(&expand(&e)).to_string(), "(x + 1)^2");
    }

    #[test]
    fn cancels_rational_functions() {
        let e = Expr::div(Expr::sub(Expr::pow(x(), Expr::int(2)), Expr::one()), Expr::sub(x(), Expr::one()));
        assert_eq!(simplify(&e).to_string(), "x + 1");
    }

    #[test]
    fn solves_quadratics() {
        let e = Expr::add(vec![Expr::pow(x(), Expr::int(2)), Expr::mul(vec![Expr::int(-5), x()]), Expr::int(6)]);
        let r: Vec<String> = solve_one(&e, "x").unwrap().iter().map(|r| r.to_string()).collect();
        assert_eq!(r, vec!["2", "3"]);
    }
}
