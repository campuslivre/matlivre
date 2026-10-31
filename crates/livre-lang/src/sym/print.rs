//! Printers: MATLAB `char(sym)` text, LaTeX, and elementwise MATLAB code (`matlabFunction`).

use num_bigint::BigInt;
use num_rational::BigRational;

use super::expr::{Const, Expr, Num, Rel};
use crate::display::fmt_g;

#[derive(Clone, Copy, PartialEq)]
pub enum Style {
    Matlab,
    Code,
}

const P_ADD: u8 = 1;
const P_MUL: u8 = 2;
const P_POW: u8 = 4;
const P_ATOM: u8 = 5;

/// A vpa number as MATLAB prints it: `digits` significant digits with trailing zeros
/// dropped, in full while the decimal exponent is within [-32, 32] (whole numbers end in
/// `.0`), otherwise as `d.ddd...e+NN`.
pub fn vpa_text(r: &BigRational) -> String {
    use num_traits::{Signed, Zero};
    if r.is_zero() {
        return "0.0".into();
    }
    let digits = crate::sym::expr::digits() as i64;
    let e = crate::sym::expr::decimal_exponent(r);
    // r is already rounded to `digits` significant digits: its digit string is exact.
    let scaled = r.abs() * BigRational::from_integer(BigInt::from(10).pow((digits - 1 - e).max(0) as u32))
        / BigRational::from_integer(BigInt::from(10).pow((e - (digits - 1)).max(0) as u32));
    let mut ds = scaled.round().to_integer().to_string();
    while ds.len() > 1 && ds.ends_with('0') {
        ds.pop();
    }
    let sign = if r.is_negative() { "-" } else { "" };
    if !(-32..=32).contains(&e) {
        let frac = if ds.len() > 1 { &ds[1..] } else { "0" };
        return format!("{sign}{}.{frac}e{}{}", &ds[..1], if e < 0 { "-" } else { "+" }, e.abs());
    }
    if e < 0 {
        format!("{sign}0.{}{ds}", "0".repeat((-e - 1) as usize))
    } else {
        let int_len = (e + 1) as usize;
        if ds.len() <= int_len {
            format!("{sign}{ds}{}.0", "0".repeat(int_len - ds.len()))
        } else {
            format!("{sign}{}.{}", &ds[..int_len], &ds[int_len..])
        }
    }
}

fn num_text(n: &Num, style: Style) -> String {
    match n {
        Num::Rat(p, 1) => p.to_string(),
        Num::Rat(p, q) => format!("{p}/{q}"),
        Num::Big(r) if r.is_integer() => r.numer().to_string(),
        Num::Big(r) => format!("{}/{}", r.numer(), r.denom()),
        Num::Vpa(r) => vpa_text(r),
        Num::F(x) => {
            let x = *x;
            if style == Style::Code {
                fmt_g(x, 17)
            } else {
                let s = fmt_g(x, 16);
                if s.contains('.') || s.contains('e') || s.contains("Inf") || s.contains("NaN") { s } else { format!("{s}.0") }
            }
        }
    }
}

/// Degree of `e` in variable `v` for MATLAB's term ordering (non-polynomial parts count 0).
fn degree(e: &Expr, v: &str) -> f64 {
    match e {
        Expr::Var(n) if n == v => 1.0,
        Expr::Pow(b, x) => match x.as_num() {
            Some(n) => degree(b, v) * n.to_f64(),
            None => 0.0,
        },
        Expr::Mul(fs) => fs.iter().map(|f| degree(f, v)).sum(),
        _ => 0.0,
    }
}

/// Orders sum terms like MATLAB: by descending degree in each variable (alphabetically),
/// constants last.
fn ordered_terms(terms: &[Expr]) -> Vec<Expr> {
    let mut vars = Vec::new();
    for t in terms {
        t.free_vars(&mut vars);
    }
    vars.sort();
    let mut v: Vec<Expr> = terms.to_vec();
    v.sort_by(|a, b| {
        let a_const = a.as_num().is_some();
        let b_const = b.as_num().is_some();
        if a_const != b_const {
            return a_const.cmp(&b_const);
        }
        let total = |e: &Expr| vars.iter().map(|x| degree(e, x)).sum::<f64>();
        let o = total(b).partial_cmp(&total(a)).unwrap_or(std::cmp::Ordering::Equal);
        if o != std::cmp::Ordering::Equal {
            return o;
        }
        for x in &vars {
            let o = degree(b, x).partial_cmp(&degree(a, x)).unwrap_or(std::cmp::Ordering::Equal);
            if o != std::cmp::Ordering::Equal {
                return o;
            }
        }
        std::cmp::Ordering::Equal
    });
    v
}

/// Splits a product into (coefficient, numerator factors, denominator factors).
fn fraction(e: &Expr) -> (Num, Vec<Expr>, Vec<Expr>) {
    let factors: Vec<Expr> = match e {
        Expr::Mul(f) => f.clone(),
        other => vec![other.clone()],
    };
    let mut coeff = Num::int(1);
    let mut num = Vec::new();
    let mut den = Vec::new();
    for f in factors {
        match &f {
            Expr::Num(n) => coeff = coeff.mul(n),
            Expr::Pow(b, x) if x.as_num().map(|n| n.is_neg()).unwrap_or(false) => {
                den.push(Expr::pow((**b).clone(), Expr::Num(x.as_num().unwrap().neg())));
            }
            _ => num.push(f),
        }
    }
    (coeff, num, den)
}

pub fn print(e: &Expr, style: Style) -> String {
    pr(e, style).0
}

fn wrap(s: (String, u8), min: u8) -> String {
    if s.1 < min { format!("({})", s.0) } else { s.0 }
}

fn pr(e: &Expr, style: Style) -> (String, u8) {
    let (mul_op, div_op, pow_op) = if style == Style::Code { (".*", "./", ".^") } else { ("*", "/", "^") };
    match e {
        Expr::Num(n) => {
            let s = num_text(n, style);
            let prec = if n.is_neg() || matches!(n, Num::Rat(_, q) if *q != 1) { P_MUL } else { P_ATOM };
            (s, prec)
        }
        Expr::Var(v) => (v.clone(), P_ATOM),
        Expr::Const(c) => (
            match c {
                Const::Pi => "pi".into(),
                Const::I => "1i".into(),
                Const::Inf => "Inf".into(),
                Const::NaN => "NaN".into(),
            },
            P_ATOM,
        ),
        Expr::Add(terms) => {
            let mut ordered = ordered_terms(terms);
            // As MATLAB prints them, a sum leads with a positive term when it has one (3 - x^2).
            if ordered.first().is_some_and(|t| t.split_coeff().0.is_neg()) {
                if let Some(k) = ordered.iter().position(|t| !t.split_coeff().0.is_neg()) {
                    let t = ordered.remove(k);
                    ordered.insert(0, t);
                }
            }
            // Imaginary parts go last (2 + 3i).
            let (real, imag): (Vec<Expr>, Vec<Expr>) = ordered.into_iter().partition(|t| !is_imag_term(t));
            ordered = real.into_iter().chain(imag).collect();
            let mut out = String::new();
            for (k, t) in ordered.iter().enumerate() {
                let (c, _) = t.split_coeff();
                let negative = c.is_neg();
                let body = if negative { Expr::neg(t.clone()) } else { t.clone() };
                let s = wrap(pr(&body, style), P_ADD + 1);
                if k == 0 {
                    if negative {
                        // MATLAB separates the sign of a leading negative term in a sum: - x - 1.
                        out.push_str(if style == Style::Matlab && ordered.len() > 1 { "- " } else { "-" });
                    }
                    out.push_str(&s);
                } else {
                    out.push_str(if negative { " - " } else { " + " });
                    out.push_str(&s);
                }
            }
            (out, P_ADD)
        }
        Expr::Mul(_) | Expr::Pow(..) => {
            if let (Expr::Mul(fs), Style::Matlab) = (e, style) {
                if let [Expr::Num(c), Expr::Const(Const::I)] | [Expr::Const(Const::I), Expr::Num(c)] = fs.as_slice() {
                    let sign = if c.is_neg() { "-" } else { "" };
                    let mag = if c.is_neg() { c.neg() } else { c.clone() };
                    return (format!("{sign}{}i", num_text(&mag, style)), P_MUL);
                }
            }
            if let Expr::Pow(b, x) = e {
                if !x.as_num().map(|n| n.is_neg()).unwrap_or(false) {
                    if let Some(Num::Rat(1, 2)) = x.as_num() {
                        if style == Style::Code {
                            return (format!("sqrt({})", pr(b, style).0), P_ATOM);
                        }
                    }
                    let base = wrap(pr(b, style), P_POW + 1);
                    let ex = match x.as_num() {
                        Some(Num::Rat(p, 1)) if p >= 0 => p.to_string(),
                        _ => format!("({})", pr(x, style).0),
                    };
                    let ex = if matches!(**x, Expr::Var(_) | Expr::Const(_)) { pr(x, style).0 } else { ex };
                    return (format!("{base}{pow_op}{ex}"), P_POW);
                }
            }
            let (coeff, num, den) = fraction(e);
            let negative = coeff.is_neg();
            let c = if negative { coeff.neg() } else { coeff };
            let (cp, cq) = match c {
                Num::Rat(p, q) => (Num::int(p), Num::int(q)),
                f => (f, Num::int(1)),
            };
            let mut num_parts: Vec<String> = Vec::new();
            if !cp.is_one() || num.is_empty() {
                num_parts.push(num_text(&cp, style));
            }
            for f in &num {
                num_parts.push(wrap(pr(f, style), P_MUL));
            }
            let mut den_parts: Vec<String> = Vec::new();
            if !cq.is_one() {
                den_parts.push(num_text(&cq, style));
            }
            for f in &den {
                den_parts.push(wrap(pr(f, style), P_MUL + 1));
            }
            let mut s = num_parts.join(mul_op);
            if !den_parts.is_empty() {
                if num_parts.len() > 1 {
                    s = format!("({s})");
                }
                let d = den_parts.join(mul_op);
                let d = if den_parts.len() > 1 { format!("({d})") } else { d };
                s = format!("{s}{div_op}{d}");
            }
            if negative {
                s = format!("-{s}");
            }
            (s, P_MUL)
        }
        Expr::Fn(n, args) => {
            let a: Vec<String> = args.iter().map(|x| pr(x, style).0).collect();
            (format!("{n}({})", a.join(", ")), P_ATOM)
        }
        Expr::Rel(op, l, r) => {
            let o = match op {
                Rel::Eq => "==",
                Rel::Ne => "~=",
                Rel::Lt => "<",
                Rel::Le => "<=",
                Rel::Gt => ">",
                Rel::Ge => ">=",
            };
            (format!("{} {o} {}", pr(l, style).0, pr(r, style).0), 0)
        }
    }
}

pub fn latex(e: &Expr) -> String {
    lx(e).0
}

fn lwrap(s: (String, u8), min: u8) -> String {
    if s.1 < min { format!("\\left({}\\right)", s.0) } else { s.0 }
}

fn lx(e: &Expr) -> (String, u8) {
    match e {
        Expr::Num(Num::Rat(p, 1)) => (p.to_string(), if *p < 0 { P_MUL } else { P_ATOM }),
        Expr::Num(Num::Rat(p, q)) => (format!("{}\\frac{{{}}}{{{q}}}", if *p < 0 { "-" } else { "" }, p.abs()), P_MUL),
        Expr::Num(Num::F(x)) => (fmt_g(*x, 16), P_ATOM),
        Expr::Num(Num::Big(r)) if r.is_integer() => (r.numer().to_string(), P_ATOM),
        Expr::Num(Num::Big(r)) => (format!("{}\\frac{{{}}}{{{}}}", if r.numer().sign() == num_bigint::Sign::Minus { "-" } else { "" }, r.numer().magnitude(), r.denom()), P_MUL),
        Expr::Num(Num::Vpa(r)) => (vpa_text(r), P_ATOM),
        Expr::Var(v) => (v.clone(), P_ATOM),
        Expr::Const(Const::Pi) => ("\\pi ".into(), P_ATOM),
        Expr::Const(Const::I) => ("\\mathrm{i}".into(), P_ATOM),
        Expr::Const(Const::Inf) => ("\\infty ".into(), P_ATOM),
        Expr::Const(Const::NaN) => ("\\mathrm{NaN}".into(), P_ATOM),
        Expr::Add(terms) => {
            let mut out = String::new();
            for (k, t) in ordered_terms(terms).iter().enumerate() {
                let negative = t.split_coeff().0.is_neg();
                let body = if negative { Expr::neg(t.clone()) } else { t.clone() };
                let s = lwrap(lx(&body), P_ADD + 1);
                if k > 0 || negative {
                    out.push_str(if negative { "-" } else { "+" });
                }
                out.push_str(&s);
            }
            (out, P_ADD)
        }
        Expr::Pow(b, x) if !x.as_num().map(|n| n.is_neg()).unwrap_or(false) => {
            if let Some(Num::Rat(1, 2)) = x.as_num() {
                return (format!("\\sqrt{{{}}}", lx(b).0), P_ATOM);
            }
            (format!("{{{}}}^{{{}}}", lwrap(lx(b), P_POW + 1), lx(x).0), P_POW)
        }
        Expr::Mul(_) | Expr::Pow(..) => {
            let (coeff, num, den) = fraction(e);
            let negative = coeff.is_neg();
            let c = if negative { coeff.neg() } else { coeff };
            let (cp, cq) = match c {
                Num::Rat(p, q) => (p.to_string(), q.to_string()),
                Num::Big(r) => (r.numer().to_string(), r.denom().to_string()),
                Num::Vpa(r) => (vpa_text(&r), "1".to_string()),
                Num::F(f) => (fmt_g(f, 16), "1".to_string()),
            };
            let mut ns: Vec<String> = Vec::new();
            if cp != "1" || num.is_empty() {
                ns.push(cp);
            }
            ns.extend(num.iter().map(|f| lwrap(lx(f), P_MUL)));
            let n = ns.join("\\,");
            let mut ds: Vec<String> = Vec::new();
            if cq != "1" {
                ds.push(cq);
            }
            ds.extend(den.iter().map(|f| lwrap(lx(f), P_MUL)));
            let s = if ds.is_empty() { n } else { format!("\\frac{{{n}}}{{{}}}", ds.join("\\,")) };
            (if negative { format!("-{s}") } else { s }, P_MUL)
        }
        Expr::Fn(n, args) => {
            let a: Vec<String> = args.iter().map(|x| lx(x).0).collect();
            let name = match n.as_str() {
                "sin" | "cos" | "tan" | "exp" | "log" | "sinh" | "cosh" | "tanh" => format!("\\{n}"),
                "asin" => "\\arcsin".into(),
                "acos" => "\\arccos".into(),
                "atan" => "\\arctan".into(),
                "abs" => return (format!("\\left|{}\\right|", a.join(",")), P_ATOM),
                other => format!("\\mathrm{{{other}}}"),
            };
            (format!("{name}\\left({}\\right)", a.join(",")), P_ATOM)
        }
        Expr::Rel(op, l, r) => {
            let o = match op {
                Rel::Eq => "=",
                Rel::Ne => "\\neq ",
                Rel::Lt => "<",
                Rel::Le => "\\le ",
                Rel::Gt => ">",
                Rel::Ge => "\\ge ",
            };
            (format!("{}{o}{}", lx(l).0, lx(r).0), 0)
        }
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", print(self, Style::Matlab))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x() -> Expr {
        Expr::var("x")
    }

    #[test]
    fn matlab_ordering_and_fractions() {
        let p = Expr::add(vec![Expr::one(), Expr::mul(vec![Expr::int(2), x()]), Expr::pow(x(), Expr::int(2))]);
        assert_eq!(p.to_string(), "x^2 + 2*x + 1");
        let q = Expr::sub(Expr::pow(x(), Expr::int(2)), Expr::mul(vec![Expr::int(2), x()]));
        assert_eq!(q.to_string(), "x^2 - 2*x");
        assert_eq!(Expr::div(x(), Expr::int(2)).to_string(), "x/2");
        assert_eq!(Expr::mul(vec![Expr::rat(3, 2), x()]).to_string(), "(3*x)/2");
        assert_eq!(Expr::div(Expr::one(), x()).to_string(), "1/x");
        assert_eq!(Expr::func("sqrt", vec![x()]).to_string(), "x^(1/2)");
    }
}

/// A numeric multiple of i (printed as `3i`).
fn is_imag_term(t: &Expr) -> bool {
    match t {
        Expr::Const(Const::I) => true,
        Expr::Mul(fs) => fs.len() == 2 && fs.iter().any(|f| matches!(f, Expr::Const(Const::I))) && fs.iter().any(|f| matches!(f, Expr::Num(_))),
        _ => false,
    }
}
