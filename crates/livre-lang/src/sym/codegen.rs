//! `ccode` and `fortran`: symbolic expressions as C or Fortran 77 assignments, in the
//! layout MATLAB R2023b produces (recorded from its output).

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};

use super::expr::{Const, Expr, Num};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    C,
    Fortran,
}

/// A whole number: `2.0` below ten, `1.23E+2` (C) or `1.23D+2` (Fortran) from ten up.
fn int_text(n: &BigInt, lang: Lang) -> String {
    let neg = n.is_negative();
    let digits = n.abs().to_string();
    let body = if digits.len() == 1 {
        format!("{digits}.0{}", if lang == Lang::Fortran { "D0" } else { "" })
    } else {
        let tail = digits[1..].trim_end_matches('0');
        let mant = if tail.is_empty() { format!("{}.0", &digits[..1]) } else { format!("{}.{tail}", &digits[..1]) };
        format!("{mant}{}+{}", if lang == Lang::Fortran { "D" } else { "E" }, digits.len() - 1)
    };
    if neg { format!("-{body}") } else { body }
}

/// A double, with the 16 significant digits MATLAB writes.
fn float_text(x: f64, lang: Lang) -> String {
    let mut s = format!("{x}");
    if !s.contains('.') && !s.contains('e') {
        s.push_str(".0");
    }
    if lang == Lang::Fortran { format!("{s}D0") } else { s }
}

/// A number standing alone: `2.0`, `1.0/3.0`, `-2.0`.
fn num_text(n: &Num, lang: Lang) -> String {
    match n {
        Num::Vpa(_) | Num::F(_) => float_text(n.to_f64(), lang),
        _ => {
            let r = n.exact().unwrap_or_else(|| BigRational::from_integer(BigInt::zero()));
            if r.is_integer() {
                int_text(r.numer(), lang)
            } else {
                let p = int_text(r.numer(), lang);
                format!("{p}/{}", int_text(r.denom(), lang))
            }
        }
    }
}

/// How code generation orders the terms of a sum: by degree, the constant last; a function
/// counts between degree one and two (`x+sin(y)`, `sin(y)+x*x`).
fn degree(e: &Expr) -> f64 {
    match e {
        Expr::Num(_) | Expr::Const(_) => 0.0,
        Expr::Var(_) => 1.0,
        Expr::Pow(b, x) => match x.as_num() {
            Some(n) => degree(b) * n.to_f64().abs(),
            None => degree(b) + 1.0,
        },
        Expr::Mul(f) => f.iter().map(degree).sum(),
        Expr::Add(t) => t.iter().map(degree).fold(0.0, f64::max),
        Expr::Fn(..) => 1.5,
        Expr::Rel(..) => 0.0,
    }
}

/// A term as its variables (or functions) and their powers, by name: `x*y^2` is
/// [(x, 1), (y, 2)]. Terms of one degree compare by it (`x*y + x^2 + y^2`, `-x+y`).
fn monomial(e: &Expr) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    let mut add = |name: String, p: f64| match out.iter_mut().find(|(n, _)| *n == name) {
        Some(e) => e.1 += p,
        None => out.push((name, p)),
    };
    fn walk(e: &Expr, scale: f64, add: &mut dyn FnMut(String, f64)) {
        match e {
            Expr::Var(v) => add(v.clone(), scale),
            Expr::Fn(f, _) => add(f.clone(), scale),
            Expr::Pow(b, x) => match x.as_num() {
                Some(n) => walk(b, scale * n.to_f64().abs(), add),
                None => walk(b, scale, add),
            },
            Expr::Mul(f) => f.iter().for_each(|x| walk(x, scale, add)),
            Expr::Add(t) => t.iter().take(1).for_each(|x| walk(x, scale, add)),
            _ => {}
        }
    }
    walk(e, 1.0, &mut add);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn cmp_monomial(a: &Expr, b: &Expr) -> std::cmp::Ordering {
    let (ma, mb) = (monomial(a), monomial(b));
    for (x, y) in ma.iter().zip(&mb) {
        let o = x.0.cmp(&y.0).then(x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
        if o != std::cmp::Ordering::Equal {
            return o;
        }
    }
    ma.len().cmp(&mb.len())
}

/// The terms of a sum in MATLAB's internal order (the order of `char` and generated code):
/// by degree with functions between degree one and two, ties by their variables and powers,
/// numbers last.
pub(crate) fn internal_order(terms: &[Expr]) -> Vec<Expr> {
    let mut ts: Vec<Expr> = terms.to_vec();
    ts.sort_by(|a, b| {
        let ka = (is_numeric(a), degree(a));
        let kb = (is_numeric(b), degree(b));
        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal).then_with(|| cmp_monomial(a, b))
    });
    ts
}

pub(crate) fn is_numeric(e: &Expr) -> bool {
    match e {
        Expr::Num(_) | Expr::Const(_) => true,
        Expr::Pow(b, x) => is_numeric(b) && is_numeric(x),
        Expr::Mul(f) | Expr::Add(f) => f.iter().all(is_numeric),
        Expr::Fn(_, a) => a.iter().all(is_numeric),
        _ => false,
    }
}

/// A factor of a product split into numerator or denominator: `x^-2` is `x^2` below.
fn split(e: &Expr) -> (bool, Expr) {
    if let Expr::Pow(b, x) = e {
        if let Some(n) = x.as_num() {
            if n.is_neg() {
                let m = n.neg();
                return (true, if m.is_one() { (**b).clone() } else { Expr::Pow(b.clone(), Box::new(Expr::Num(m))) });
            }
        }
    }
    (false, e.clone())
}

struct Gen {
    lang: Lang,
}

impl Gen {
    fn pi(&self) -> String {
        float_text(std::f64::consts::PI, self.lang)
    }

    /// A power written out as a product (`x*x*x`), which needs brackets inside a product.
    fn is_repeated(&self, e: &Expr) -> bool {
        if self.lang == Lang::Fortran {
            return false;
        }
        match e {
            Expr::Pow(b, x) => matches!(**b, Expr::Var(_) | Expr::Const(Const::Pi)) && x.as_num().and_then(|n| n.as_int()).is_some_and(|k| (2..=9).contains(&k)),
            _ => false,
        }
    }

    fn atom(&self, e: &Expr) -> String {
        match e {
            Expr::Var(v) => v.clone(),
            Expr::Const(Const::Pi) => self.pi(),
            Expr::Const(Const::I) => "I".into(),
            Expr::Const(Const::Inf) => if self.lang == Lang::C { "INFINITY".into() } else { "Inf".into() },
            Expr::Const(Const::NaN) => "NaN".into(),
            _ => self.expr(e),
        }
    }

    /// `e` where it is one factor of a product or a power's base.
    fn factor(&self, e: &Expr, in_product: bool) -> String {
        match e {
            Expr::Add(_) => format!("({})", self.expr(e)),
            Expr::Mul(_) => format!("({})", self.expr(e)),
            Expr::Num(n) if n.is_neg() || n.is_fraction() => format!("({})", num_text(n, self.lang)),
            _ if in_product && self.is_repeated(e) => format!("({})", self.expr(e)),
            _ => self.expr(e),
        }
    }

    fn pow(&self, b: &Expr, x: &Expr) -> String {
        let n = x.as_num();
        if let Some(n) = &n {
            if matches!(n, Num::Rat(1, 2)) {
                return format!("sqrt({})", self.expr(b));
            }
        }
        match self.lang {
            Lang::C => {
                if self.is_repeated(&Expr::Pow(Box::new(b.clone()), Box::new(x.clone()))) {
                    let k = n.and_then(|n| n.as_int()).unwrap_or(1);
                    return vec![self.atom(b); k as usize].join("*");
                }
                let exp = match &n {
                    Some(n) => num_text(n, self.lang),
                    None => self.expr(x),
                };
                format!("pow({},{exp})", self.expr(b))
            }
            Lang::Fortran => {
                let base = match b {
                    Expr::Fn(..) | Expr::Var(_) | Expr::Const(_) => self.atom(b),
                    Expr::Num(m) if !m.is_neg() && !m.is_fraction() => num_text(m, self.lang),
                    _ => format!("({})", self.expr(b)),
                };
                let exp = match &n {
                    Some(n) if n.is_int() => n.as_int().map_or_else(|| num_text(n, self.lang), |k| k.to_string()),
                    Some(n) => format!("({})", num_text(n, self.lang)),
                    None => self.factor(x, false),
                };
                format!("{base}**{exp}")
            }
        }
    }

    /// A product (or a single term) with coefficient `c`, `c` written as given.
    fn product(&self, c: &Num, rest: &[Expr]) -> String {
        let mut num: Vec<Expr> = Vec::new();
        let mut den: Vec<Expr> = Vec::new();
        for f in rest {
            let (below, g) = split(f);
            if below { den.push(g) } else { num.push(g) }
        }
        // Constants such as pi go after the variables, like the coefficient.
        num.sort_by_key(is_numeric);
        let r = c.exact().filter(|_| !matches!(c, Num::F(_) | Num::Vpa(_)));
        let mut parts: Vec<String> = Vec::new();
        let many = num.len() + den.len() > 1 || (!c.is_one() && !num.is_empty());
        for f in &num {
            parts.push(self.factor(f, many || !den.is_empty()));
        }
        let mut den_num: Option<String> = None;
        match &r {
            Some(r) if !r.is_integer() && r.numer() == &BigInt::from(1) && !num.is_empty() => den_num = Some(int_text(r.denom(), self.lang)),
            _ if c.is_one() => {}
            _ if num.is_empty() => parts.push(num_text(c, self.lang)),
            Some(r) if r.is_integer() && r.is_negative() && self.lang == Lang::C => parts.push(num_text(c, self.lang)),
            _ => parts.push(self.factor(&Expr::Num(c.clone()), true)),
        }
        let mut s = if parts.is_empty() { num_text(&Num::int(1), self.lang) } else { parts.join("*") };
        if let Some(d) = den_num {
            den.insert(0, Expr::Var(d));
        }
        if !den.is_empty() {
            let d: Vec<String> = den.iter().map(|f| self.factor(f, den.len() > 1)).collect();
            let single = den.len() == 1 && !self.is_repeated(&den[0]);
            s = if single { format!("{s}/{}", d[0]) } else { format!("{s}/({})", d.join("*")) };
        }
        s
    }

    /// A term of a sum, as its sign and text; the first term keeps a negative coefficient
    /// in place (`x*-2.0+...`), later ones become a subtraction.
    fn term(&self, t: &Expr, first: bool) -> (bool, String) {
        let (c, rest) = t.split_coeff();
        let rest: Vec<Expr> = match rest {
            Expr::Mul(f) => f,
            e if e.is_one() => Vec::new(),
            e => vec![e],
        };
        if rest.is_empty() {
            return if c.is_neg() && !first { (true, num_text(&c.neg(), self.lang)) } else { (false, num_text(&c, self.lang)) };
        }
        if c.is_neg() && (!first || c.neg().is_one()) {
            return (true, self.product(&c.neg(), &rest));
        }
        (false, self.product(&c, &rest))
    }

    fn expr(&self, e: &Expr) -> String {
        match e {
            Expr::Num(n) => num_text(n, self.lang),
            Expr::Var(_) | Expr::Const(_) => self.atom(e),
            Expr::Add(terms) => {
                let ts = internal_order(terms);
                let mut s = String::new();
                for (k, t) in ts.iter().enumerate() {
                    let (neg, body) = self.term(t, k == 0);
                    if neg {
                        s.push('-');
                    } else if k > 0 {
                        s.push('+');
                    }
                    s.push_str(&body);
                }
                s
            }
            Expr::Pow(b, x) if !x.as_num().is_some_and(|n| n.is_neg()) => self.pow(b, x),
            Expr::Mul(_) | Expr::Pow(..) => {
                let (neg, body) = self.term(e, true);
                if neg { format!("-{body}") } else { body }
            }
            Expr::Fn(name, args) => self.func(name, args),
            Expr::Rel(..) => super::print::print(e, super::print::Style::Code),
        }
    }

    fn func(&self, name: &str, args: &[Expr]) -> String {
        let abs = if self.lang == Lang::C { "fabs" } else { "abs" };
        match (name, args) {
            ("sign", [a]) => format!("({}/{abs}({}))", self.factor(a, false), self.expr(a)),
            ("abs", [a]) => format!("{abs}({})", self.expr(a)),
            _ => format!("{name}({})", args.iter().map(|a| self.expr(a)).collect::<Vec<_>>().join(",")),
        }
    }
}

/// The text of `ccode(e)` or `fortran(e)` for a scalar or matrix (column-major `data`).
pub fn generate(data: &[Expr], rows: usize, cols: usize, lang: Lang) -> String {
    let g = Gen { lang };
    let lead = if lang == Lang::C { "  " } else { "      " };
    let line = |lhs: String, e: &Expr| -> String {
        let text = format!("{lead}{lhs} = {}{}", g.expr(e), if lang == Lang::C { ";" } else { "" });
        if lang == Lang::Fortran { wrap_fortran(&text) } else { text }
    };
    if data.len() == 1 {
        return line("t0".into(), &data[0]);
    }
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let e = &data[c * rows + r];
            if e.is_zero() {
                continue;
            }
            let lhs = if lang == Lang::C { format!("T[{r}][{c}]") } else { format!("T({},{})", r + 1, c + 1) };
            out.push(line(lhs, e));
        }
    }
    out.join("\n")
}

/// Fortran 77 lines end at column 72; the rest goes on `     &` continuation lines.
fn wrap_fortran(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= 72 {
        return s.to_string();
    }
    let mut out: Vec<String> = vec![chars[..72].iter().collect()];
    for chunk in chars[72..].chunks(66) {
        out.push(format!("     &{}", chunk.iter().collect::<String>()));
    }
    out.join("\n")
}
