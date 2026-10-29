//! Symbolic expressions in canonical form. Constructors (`add`, `mul`, `pow`, `func`) keep
//! expressions normalized: nested sums/products flattened, like terms and powers combined,
//! exact rational arithmetic folded, and numeric coefficients distributed over sums.

use std::cmp::Ordering;

use crate::cplx::C;

use std::borrow::Borrow;
use std::cell::Cell;
use std::rc::Rc;

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

thread_local! {
    /// Significant decimal digits of `vpa` results (MATLAB's `digits`, 32 by default).
    static DIGITS: Cell<u32> = const { Cell::new(32) };
}

pub fn digits() -> u32 {
    DIGITS.with(|d| d.get())
}

pub fn set_digits(n: u32) {
    DIGITS.with(|d| d.set(n));
}

/// A symbolic number: an exact rational (`Rat` while it fits in i128, `Big` beyond), a
/// variable-precision value from `vpa` (a decimal rounded to `digits()` significant digits),
/// or a plain double.
#[derive(Debug, Clone, PartialEq)]
pub enum Num {
    Rat(i128, i128),
    Big(Rc<BigRational>),
    Vpa(Rc<BigRational>),
    F(f64),
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// Bounds of the "modest" integers MATLAB's sym() recognises in p/q, p*pi/q and sqrt(p).
const MODEST: f64 = 100_000.0;

/// The fraction p/q with |p|, q <= 100000 that `x` is within 100*eps (relative) of, as
/// sym(x) recognises it: sym(0.1 + 0.2) is 3/10, sym(123456.789) is not 123456789/1000.
pub fn modest_fraction(x: f64) -> Option<(i128, i128)> {
    if !x.is_finite() || x == 0.0 {
        return None;
    }
    let tol = 100.0 * f64::EPSILON * x.abs();
    let (mut h0, mut h1, mut k0, mut k1) = (0f64, 1f64, 1f64, 0f64);
    let mut v = x.abs();
    for _ in 0..64 {
        let a = v.floor();
        let (h2, k2) = (a * h1 + h0, a * k1 + k0);
        if h2 > MODEST || k2 > MODEST {
            return None;
        }
        (h0, h1, k0, k1) = (h1, h2, k1, k2);
        if (h1 / k1 - x.abs()).abs() <= tol {
            let p = h1 as i128;
            return Some((if x < 0.0 { -p } else { p }, k1 as i128));
        }
        let f = v - a;
        if f == 0.0 {
            return None;
        }
        v = 1.0 / f;
    }
    None
}

/// `r` rounded to `digits` significant decimal digits (half away from zero).
pub fn round_sig(r: &BigRational, digits: u32) -> BigRational {
    if r.is_zero() {
        return r.clone();
    }
    let e = decimal_exponent(r);
    let shift = digits as i64 - 1 - e;
    let ten = BigInt::from(10);
    let scaled = if shift >= 0 { r * BigRational::from_integer(ten.pow(shift as u32)) } else { r / BigRational::from_integer(ten.pow((-shift) as u32)) };
    let n = round_half_away(&scaled);
    if shift >= 0 {
        BigRational::new(n, ten.pow(shift as u32))
    } else {
        BigRational::from_integer(n * ten.pow((-shift) as u32))
    }
}

fn round_half_away(r: &BigRational) -> BigInt {
    let two = BigInt::from(2);
    let (n, d) = (r.numer(), r.denom());
    // round(n/d) = floor((2n + d) / 2d) for n >= 0, mirrored for n < 0.
    if n.is_negative() {
        -((-n * &two + d).div_floor(&(d * &two)))
    } else {
        (n * &two + d).div_floor(&(d * &two))
    }
}

/// floor(log10(|r|)) for r != 0.
pub fn decimal_exponent(r: &BigRational) -> i64 {
    let a = r.abs();
    let bits = a.numer().bits() as i64 - a.denom().bits() as i64;
    let mut e = ((bits as f64) * std::f64::consts::LOG10_2).floor() as i64;
    let ten = BigRational::from_integer(BigInt::from(10));
    let pow = |k: i64| if k >= 0 { ten.pow(k as i32) } else { BigRational::one() / ten.pow((-k) as i32) };
    while pow(e) > a {
        e -= 1;
    }
    while pow(e + 1) <= a {
        e += 1;
    }
    e
}

/// A rational converted to the nearest double.
pub fn rational_to_f64(r: &BigRational) -> f64 {
    if r.is_zero() {
        return 0.0;
    }
    let neg = r.is_negative();
    let (n, d) = (r.numer().abs(), r.denom().clone());
    // Scale so the integer quotient has 54..55 significant bits, then round once.
    let shift = 54 + d.bits() as i64 - n.bits() as i64;
    let (num, den) = if shift >= 0 { (n << shift as usize, d) } else { (n, d << (-shift) as usize) };
    let (q, rem) = num.div_rem(&den);
    // q has 54 or 55 bits; keep 53 with round-half-even, using the remainder as sticky bit.
    let extra = q.bits() as i64 - 53;
    let mask = (BigInt::one() << extra as usize) - 1;
    let low = &q & &mask;
    let mut m = (&q >> extra as usize).to_u64().unwrap_or(0);
    let half = BigInt::one() << (extra - 1) as usize;
    if low > half || (low == half && (!rem.is_zero() || m & 1 == 1)) {
        m += 1;
    }
    let x = (m as f64) * 2f64.powi((extra - shift) as i32);
    if neg { -x } else { x }
}

impl Num {
    pub fn int(n: i128) -> Num {
        Num::Rat(n, 1)
    }

    pub fn rat(p: i128, q: i128) -> Num {
        if q == 0 {
            return Num::F(if p == 0 { f64::NAN } else if p > 0 { f64::INFINITY } else { f64::NEG_INFINITY });
        }
        let g = gcd(p, q).max(1);
        let (p, q) = (p / g, q / g);
        if q < 0 { Num::Rat(-p, -q) } else { Num::Rat(p, q) }
    }

    /// An exact rational, kept in `Rat` when it fits.
    pub fn from_big(r: BigRational) -> Num {
        match (r.numer().to_i128(), r.denom().to_i128()) {
            (Some(p), Some(q)) => Num::Rat(p, q),
            _ => Num::Big(Rc::new(r)),
        }
    }

    /// A vpa value: `r` rounded to the current `digits()`.
    pub fn vpa(r: &BigRational) -> Num {
        Num::Vpa(Rc::new(round_sig(r, digits())))
    }

    /// The exact value of a rational or vpa number (None for a non-finite double).
    pub fn exact(&self) -> Option<BigRational> {
        match self {
            Num::Rat(p, q) => Some(BigRational::new(BigInt::from(*p), BigInt::from(*q))),
            Num::Big(r) | Num::Vpa(r) => Some((**r).clone()),
            Num::F(x) => BigRational::from_float(*x),
        }
    }

    /// sym(x) for a finite double, as MATLAB converts it: integers exactly, modest fractions
    /// p/q, and anything else as its exact binary value (sym(0.3333333) is
    /// 6004798902680711/18014398509481984).
    pub fn from_f64(x: f64) -> Num {
        if !x.is_finite() {
            return Num::F(x);
        }
        if x.fract() == 0.0 {
            return Num::from_big(BigRational::from_float(x).unwrap());
        }
        if let Some((p, q)) = modest_fraction(x) {
            return Num::rat(p, q);
        }
        Num::from_big(BigRational::from_float(x).unwrap())
    }

    pub fn to_f64(&self) -> f64 {
        match self {
            Num::Rat(p, q) => {
                if p.unsigned_abs() < 1 << 53 && *q < 1 << 53 {
                    *p as f64 / *q as f64
                } else {
                    rational_to_f64(&self.exact().unwrap())
                }
            }
            Num::Big(r) | Num::Vpa(r) => rational_to_f64(r),
            Num::F(x) => *x,
        }
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Num::Rat(p, _) => *p == 0,
            Num::Big(r) | Num::Vpa(r) => r.is_zero(),
            Num::F(x) => *x == 0.0,
        }
    }

    pub fn is_one(&self) -> bool {
        match self {
            Num::Rat(p, q) => *p == 1 && *q == 1,
            Num::Big(_) => false,
            Num::Vpa(r) => r.is_one(),
            Num::F(x) => *x == 1.0,
        }
    }

    pub fn is_neg(&self) -> bool {
        match self {
            Num::Rat(p, _) => *p < 0,
            Num::Big(r) | Num::Vpa(r) => r.is_negative(),
            Num::F(x) => *x < 0.0,
        }
    }

    pub fn is_int(&self) -> bool {
        match self {
            Num::Rat(_, q) => *q == 1,
            Num::Big(r) | Num::Vpa(r) => r.is_integer(),
            Num::F(x) => x.fract() == 0.0,
        }
    }

    /// An exact fraction that is not an integer (prints as p/q).
    pub fn is_fraction(&self) -> bool {
        matches!(self, Num::Rat(_, q) if *q != 1) || matches!(self, Num::Big(r) if !r.is_integer())
    }

    pub fn as_int(&self) -> Option<i128> {
        match self {
            Num::Rat(p, 1) => Some(*p),
            _ => None,
        }
    }

    fn is_float(&self) -> bool {
        matches!(self, Num::F(_))
    }

    fn is_vpa(&self) -> bool {
        matches!(self, Num::Vpa(_))
    }

    /// Combines two numbers: doubles stay doubles, vpa stays vpa, exact stays exact.
    fn combine(&self, o: &Num, f: fn(f64, f64) -> f64, g: impl Fn(BigRational, BigRational) -> BigRational) -> Num {
        if self.is_float() || o.is_float() {
            return Num::F(f(self.to_f64(), o.to_f64()));
        }
        let r = g(self.exact().unwrap(), o.exact().unwrap());
        if self.is_vpa() || o.is_vpa() { Num::vpa(&r) } else { Num::from_big(r) }
    }

    pub fn add(&self, o: impl Borrow<Num>) -> Num {
        let o = o.borrow();
        if let (Num::Rat(a, b), Num::Rat(c, d)) = (self, o) {
            if let (Some(x), Some(y), Some(z)) = (a.checked_mul(*d), c.checked_mul(*b), b.checked_mul(*d)) {
                if let Some(s) = x.checked_add(y) {
                    return Num::rat(s, z);
                }
            }
        }
        self.combine(o, |a, b| a + b, |a, b| a + b)
    }

    pub fn mul(&self, o: impl Borrow<Num>) -> Num {
        let o = o.borrow();
        if let (Num::Rat(a, b), Num::Rat(c, d)) = (self, o) {
            let g1 = gcd(*a, *d).max(1);
            let g2 = gcd(*c, *b).max(1);
            if let (Some(p), Some(q)) = ((a / g1).checked_mul(c / g2), (b / g2).checked_mul(d / g1)) {
                return Num::rat(p, q);
            }
        }
        self.combine(o, |a, b| a * b, |a, b| a * b)
    }

    pub fn neg(&self) -> Num {
        match self {
            Num::Rat(p, q) => match p.checked_neg() {
                Some(n) => Num::Rat(n, *q),
                None => Num::from_big(-self.exact().unwrap()),
            },
            Num::Big(r) => Num::Big(Rc::new(-(**r).clone())),
            Num::Vpa(r) => Num::Vpa(Rc::new(-(**r).clone())),
            Num::F(x) => Num::F(-x),
        }
    }

    pub fn recip(&self) -> Num {
        if self.is_zero() && !self.is_float() {
            return Num::F(f64::INFINITY);
        }
        match self {
            Num::Rat(p, q) => Num::rat(*q, *p),
            Num::Big(r) => Num::from_big(r.recip()),
            Num::Vpa(r) => Num::vpa(&r.recip()),
            Num::F(x) => Num::F(1.0 / x),
        }
    }

    pub fn pow_int(&self, n: i128) -> Num {
        if n < 0 {
            return self.recip().pow_int(-n);
        }
        let mut acc = Num::int(1);
        let mut base = self.clone();
        let mut k = n;
        while k > 0 {
            if k & 1 == 1 {
                acc = acc.mul(&base);
            }
            k >>= 1;
            if k > 0 {
                base = base.mul(&base);
            }
        }
        acc
    }

    pub fn cmp_val(&self, o: impl Borrow<Num>) -> Ordering {
        let o = o.borrow();
        match (self.exact(), o.exact()) {
            (Some(a), Some(b)) if !self.is_float() && !o.is_float() => a.cmp(&b),
            _ => self.to_f64().partial_cmp(&o.to_f64()).unwrap_or(Ordering::Equal),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Const {
    Pi,
    /// Imaginary unit.
    I,
    Inf,
    NaN,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rel {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(Num),
    Var(String),
    Const(Const),
    Add(Vec<Expr>),
    Mul(Vec<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    /// Function application (`sin`, `exp`, `log`, …; also unevaluated `int`/`limit`).
    Fn(String, Vec<Expr>),
    Rel(Rel, Box<Expr>, Box<Expr>),
}

fn rank(e: &Expr) -> u8 {
    match e {
        Expr::Num(_) => 0,
        Expr::Const(_) => 1,
        Expr::Var(_) => 2,
        Expr::Pow(..) => 3,
        Expr::Mul(_) => 4,
        Expr::Add(_) => 5,
        Expr::Fn(..) => 6,
        Expr::Rel(..) => 7,
    }
}

/// Total order used to canonicalize the argument order of sums and products.
pub fn cmp(a: &Expr, b: &Expr) -> Ordering {
    // Compare by the "base" of powers so x and x^2 sort together.
    let (ka, kb) = (sort_base(a), sort_base(b));
    if !std::ptr::eq(ka, a) || !std::ptr::eq(kb, b) {
        let o = cmp_raw(ka, kb);
        if o != Ordering::Equal {
            return o;
        }
    }
    cmp_raw(a, b)
}

fn sort_base(e: &Expr) -> &Expr {
    match e {
        Expr::Pow(b, _) => b,
        other => other,
    }
}

fn cmp_list(a: &[Expr], b: &[Expr]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let o = cmp(x, y);
        if o != Ordering::Equal {
            return o;
        }
    }
    a.len().cmp(&b.len())
}

fn cmp_raw(a: &Expr, b: &Expr) -> Ordering {
    let r = rank(a).cmp(&rank(b));
    if r != Ordering::Equal {
        return r;
    }
    match (a, b) {
        (Expr::Num(x), Expr::Num(y)) => x.cmp_val(y),
        (Expr::Const(x), Expr::Const(y)) => x.cmp(y),
        (Expr::Var(x), Expr::Var(y)) => x.cmp(y),
        (Expr::Pow(b1, e1), Expr::Pow(b2, e2)) => cmp(b1, b2).then_with(|| cmp(e1, e2)),
        (Expr::Mul(x), Expr::Mul(y)) | (Expr::Add(x), Expr::Add(y)) => cmp_list(x, y),
        (Expr::Fn(n1, a1), Expr::Fn(n2, a2)) => n1.cmp(n2).then_with(|| cmp_list(a1, a2)),
        (Expr::Rel(_, l1, r1), Expr::Rel(_, l2, r2)) => cmp(l1, l2).then_with(|| cmp(r1, r2)),
        _ => Ordering::Equal,
    }
}

impl Expr {
    pub fn int(n: i128) -> Expr {
        Expr::Num(Num::int(n))
    }

    pub fn rat(p: i128, q: i128) -> Expr {
        Expr::Num(Num::rat(p, q))
    }

    pub fn zero() -> Expr {
        Expr::int(0)
    }

    pub fn one() -> Expr {
        Expr::int(1)
    }

    pub fn var(s: &str) -> Expr {
        Expr::Var(s.to_string())
    }

    pub fn from_f64(x: f64) -> Expr {
        if x.is_nan() {
            return Expr::Const(Const::NaN);
        }
        if x.is_infinite() {
            return if x > 0.0 { Expr::Const(Const::Inf) } else { Expr::mul(vec![Expr::int(-1), Expr::Const(Const::Inf)]) };
        }
        // sym(x) recognises, in order: powers of ten (sym(1e23) is 10^23, not the double's
        // binary value), integers and modest fractions, p*pi/q, and sqrt(p) for modest p, q.
        if x != 0.0 && x.abs().log10().fract().abs() < 1e-9 {
            let q = x.abs().log10().round() as i32;
            if format!("1e{q}").parse::<f64>() == Ok(x.abs()) {
                let ten = Expr::Num(Num::from_big(BigRational::from_integer(BigInt::from(10)).pow(q)));
                return if x < 0.0 { Expr::neg(ten) } else { ten };
            }
        }
        if x.fract() == 0.0 || modest_fraction(x).is_some() {
            return Expr::Num(Num::from_f64(x));
        }
        let tol = 100.0 * f64::EPSILON * x.abs();
        if let Some((p, q)) = modest_fraction(x / std::f64::consts::PI) {
            if (p as f64 * std::f64::consts::PI / q as f64 - x).abs() <= tol {
                return Expr::mul(vec![Expr::rat(p, q), Expr::Const(Const::Pi)]);
            }
        }
        let s = (x * x).round();
        if s >= 2.0 && s <= 100_000.0 && (s.sqrt() - x.abs()).abs() <= tol {
            let root = Expr::pow(Expr::int(s as i128), Expr::rat(1, 2));
            return if x < 0.0 { Expr::neg(root) } else { root };
        }
        Expr::Num(Num::from_f64(x))
    }

    pub fn as_num(&self) -> Option<Num> {
        if let Expr::Num(n) = self { Some(n.clone()) } else { None }
    }

    pub fn is_zero(&self) -> bool {
        matches!(self, Expr::Num(n) if n.is_zero())
    }

    pub fn is_one(&self) -> bool {
        matches!(self, Expr::Num(n) if n.is_one())
    }

    pub fn neg(e: Expr) -> Expr {
        Expr::mul(vec![Expr::int(-1), e])
    }

    pub fn sub(a: Expr, b: Expr) -> Expr {
        Expr::add(vec![a, Expr::neg(b)])
    }

    pub fn div(a: Expr, b: Expr) -> Expr {
        Expr::mul(vec![a, Expr::pow(b, Expr::int(-1))])
    }

    /// (numeric coefficient, remaining factor) of a term.
    pub fn split_coeff(&self) -> (Num, Expr) {
        match self {
            Expr::Num(n) => (n.clone(), Expr::one()),
            Expr::Mul(f) => match f.first() {
                Some(Expr::Num(n)) => {
                    let rest: Vec<Expr> = f[1..].to_vec();
                    (n.clone(), if rest.len() == 1 { rest.into_iter().next().unwrap() } else { Expr::Mul(rest) })
                }
                _ => (Num::int(1), self.clone()),
            },
            _ => (Num::int(1), self.clone()),
        }
    }

    pub fn add(terms: Vec<Expr>) -> Expr {
        let mut flat = Vec::new();
        for t in terms {
            match t {
                Expr::Add(inner) => flat.extend(inner),
                other => flat.push(other),
            }
        }
        if flat.iter().any(|t| matches!(t, Expr::Const(Const::NaN))) {
            return Expr::Const(Const::NaN);
        }
        let mut num = Num::int(0);
        let mut collected: Vec<(Expr, Num)> = Vec::new();
        for t in flat {
            if let Expr::Num(n) = t {
                num = num.add(n);
                continue;
            }
            let (c, rest) = t.split_coeff();
            match collected.iter_mut().find(|(r, _)| *r == rest) {
                Some((_, acc)) => *acc = acc.add(c),
                None => collected.push((rest, c)),
            }
        }
        let mut out: Vec<Expr> = collected.into_iter().filter(|(_, c)| !c.is_zero()).map(|(r, c)| Expr::scale(c, r)).collect();
        // Inf + finite stays Inf.
        if out.iter().any(|t| matches!(t, Expr::Const(Const::Inf))) && !num.is_zero() {
            num = Num::int(0);
        }
        out.sort_by(cmp);
        if !num.is_zero() {
            out.insert(0, Expr::Num(num));
        }
        match out.len() {
            0 => Expr::zero(),
            1 => out.pop().unwrap(),
            _ => Expr::Add(out),
        }
    }

    /// `c * e` without re-running full product canonicalization when `e` is already canonical.
    fn scale(c: Num, e: Expr) -> Expr {
        if c.is_one() {
            return e;
        }
        if e.is_one() {
            return Expr::Num(c);
        }
        match e {
            Expr::Mul(mut f) => {
                f.insert(0, Expr::Num(c));
                Expr::Mul(f)
            }
            other => Expr::Mul(vec![Expr::Num(c), other]),
        }
    }

    pub fn mul(factors: Vec<Expr>) -> Expr {
        let mut flat = Vec::new();
        for f in factors {
            match f {
                Expr::Mul(inner) => flat.extend(inner),
                other => flat.push(other),
            }
        }
        if flat.iter().any(|t| matches!(t, Expr::Const(Const::NaN))) {
            return Expr::Const(Const::NaN);
        }
        // 0 * Inf is NaN.
        if flat.iter().any(|t| matches!(t, Expr::Const(Const::Inf))) && flat.iter().any(|t| t.is_zero()) {
            return Expr::Const(Const::NaN);
        }
        let mut coeff = Num::int(1);
        let mut powers: Vec<(Expr, Expr)> = Vec::new();
        for f in flat {
            if let Expr::Num(n) = f {
                coeff = coeff.mul(n);
                continue;
            }
            let (b, e) = match f {
                Expr::Pow(b, e) => (*b, *e),
                other => (other, Expr::one()),
            };
            match powers.iter_mut().find(|(pb, _)| *pb == b) {
                Some((_, pe)) => *pe = Expr::add(vec![pe.clone(), e]),
                None => powers.push((b, e)),
            }
        }
        if coeff.is_zero() {
            return Expr::zero();
        }
        let mut out = Vec::new();
        for (b, e) in powers {
            match Expr::pow(b, e) {
                Expr::Num(n) => coeff = coeff.mul(n),
                Expr::Mul(inner) => {
                    for x in inner {
                        if let Expr::Num(n) = x {
                            coeff = coeff.mul(n);
                        } else {
                            out.push(x);
                        }
                    }
                }
                other => out.push(other),
            }
        }
        // Merge factors that became equal after power folding (e.g. sqrt(2)*sqrt(2)).
        if out.len() > 1 {
            let mut changed = false;
            for i in 0..out.len() {
                for j in i + 1..out.len() {
                    if sort_base(&out[i]) == sort_base(&out[j]) {
                        changed = true;
                    }
                }
            }
            if changed {
                let mut v = vec![Expr::Num(coeff)];
                v.extend(out);
                return Expr::mul(v);
            }
        }
        out.sort_by(cmp);
        if coeff.is_zero() {
            return Expr::zero();
        }
        // A numeric coefficient distributes over a single sum: 2*(x + 1) -> 2*x + 2.
        if out.len() == 1 && !coeff.is_one() {
            if let Expr::Add(terms) = &out[0] {
                return Expr::add(terms.iter().map(|t| Expr::mul(vec![Expr::Num(coeff.clone()), t.clone()])).collect());
            }
        }
        if !coeff.is_one() {
            out.insert(0, Expr::Num(coeff));
        }
        match out.len() {
            0 => Expr::one(),
            1 => out.pop().unwrap(),
            _ => Expr::Mul(out),
        }
    }

    pub fn pow(b: Expr, e: Expr) -> Expr {
        if e.is_zero() {
            return Expr::one();
        }
        if e.is_one() {
            return b;
        }
        if b.is_one() {
            return Expr::one();
        }
        match (&b, &e) {
            (Expr::Num(x), Expr::Num(n)) => {
                if let Some(k) = n.as_int() {
                    if x.is_zero() && k < 0 {
                        return Expr::Const(Const::Inf);
                    }
                    if k.abs() <= 4096 {
                        return Expr::Num(x.pow_int(k));
                    }
                }
                if let (Num::F(_), _) | (_, Num::F(_)) = (x, n) {
                    let v = x.to_f64().powf(n.to_f64());
                    if v.is_finite() {
                        return Expr::Num(Num::F(v));
                    }
                }
                if x.is_zero() && !n.is_neg() {
                    return Expr::zero();
                }
                // Rational roots of perfect powers: 4^(1/2) = 2, 8^(1/3) = 2, (1/4)^(1/2) = 1/2.
                if let (Num::Rat(p, q), Num::Rat(a, r)) = (x.clone(), n.clone()) {
                    if p > 0 {
                        if let (Some(pr), Some(qr)) = (int_root(p, r), int_root(q, r)) {
                            return Expr::pow(Expr::rat(pr, qr), Expr::int(a));
                        }
                        // Extract square factors: sqrt(8) = 2*sqrt(2).
                        if r == 2 && a == 1 && q == 1 {
                            let (out, inside) = extract_square(p);
                            if out > 1 {
                                return Expr::mul(vec![Expr::int(out), Expr::Pow(Box::new(Expr::int(inside)), Box::new(Expr::rat(1, 2)))]);
                            }
                        }
                        if r == 2 && a == -1 && q == 1 {
                            // 1/sqrt(p) stays as p^(-1/2).
                        }
                    }
                }
                Expr::Pow(Box::new(b), Box::new(e))
            }
            (Expr::Const(Const::I), Expr::Num(n)) if n.is_int() => match n.as_int().unwrap_or(0).rem_euclid(4) {
                0 => Expr::one(),
                1 => Expr::Const(Const::I),
                2 => Expr::int(-1),
                _ => Expr::neg(Expr::Const(Const::I)),
            },
            (Expr::Pow(bb, be), Expr::Num(n)) if n.is_int() => Expr::pow((**bb).clone(), Expr::mul(vec![(**be).clone(), e.clone()])),
            (Expr::Mul(fs), Expr::Num(n)) if n.is_int() => Expr::mul(fs.iter().map(|f| Expr::pow(f.clone(), e.clone())).collect()),
            (Expr::Fn(name, args), _) if name == "exp" && args.len() == 1 => Expr::func("exp", vec![Expr::mul(vec![args[0].clone(), e.clone()])]),
            _ => Expr::Pow(Box::new(b), Box::new(e)),
        }
    }

    pub fn func(name: &str, args: Vec<Expr>) -> Expr {
        // mod(a, m) and rem(a, m) fold once both are numbers; otherwise stay unevaluated.
        if matches!(name, "mod" | "rem") && args.len() == 2 {
            if let (Some(x), Some(m)) = (args[0].as_num(), args[1].as_num()) {
                if m.is_zero() {
                    return if name == "mod" { args[0].clone() } else { Expr::Const(Const::NaN) };
                }
                let t = x.mul(m.recip()).to_f64();
                let k = if name == "mod" { t.floor() } else { t.trunc() };
                return Expr::Num(x.add(Num::from_f64(k).mul(m).neg()));
            }
            return Expr::Fn(name.into(), args);
        }
        let a = args.first().cloned().unwrap_or_else(Expr::zero);
        let pi = Expr::Const(Const::Pi);
        // Exact values at multiples of pi/2.
        let pi_multiple = |e: &Expr| -> Option<Num> {
            if e.is_zero() {
                return Some(Num::int(0));
            }
            if *e == pi {
                return Some(Num::int(1));
            }
            if let Expr::Mul(f) = e {
                if f.len() == 2 && f[1] == pi {
                    return f[0].as_num();
                }
            }
            None
        };
        match name {
            "sin" | "cos" | "tan" => {
                if let Some(k) = pi_multiple(&a) {
                    let twice = k.mul(Num::int(2));
                    if let Some(h) = twice.as_int() {
                        // k*pi with k a multiple of 1/2.
                        let hm = h.rem_euclid(4);
                        let (s, c) = match hm {
                            0 => (0, 1),
                            1 => (1, 0),
                            2 => (0, -1),
                            _ => (-1, 0),
                        };
                        return match name {
                            "sin" => Expr::int(s),
                            "cos" => Expr::int(c),
                            _ => {
                                if c == 0 {
                                    Expr::Const(Const::Inf)
                                } else {
                                    Expr::int(s / c)
                                }
                            }
                        };
                    }
                }
                if let (Some(n), true) = (a.as_num(), matches!(a.as_num(), Some(Num::F(_)))) {
                    let x = n.to_f64();
                    return Expr::Num(Num::F(match name {
                        "sin" => x.sin(),
                        "cos" => x.cos(),
                        _ => x.tan(),
                    }));
                }
                // Odd/even symmetry: sin(-x) = -sin(x), cos(-x) = cos(x).
                let (c, _) = a.split_coeff();
                if c.is_neg() {
                    let pos = Expr::neg(a.clone());
                    return match name {
                        "cos" => Expr::Fn(name.into(), vec![pos]),
                        _ => Expr::neg(Expr::Fn(name.into(), vec![pos])),
                    };
                }
            }
            "exp" => {
                if a.is_zero() {
                    return Expr::one();
                }
                if let Expr::Fn(n, inner) = &a {
                    if n == "log" {
                        return inner[0].clone();
                    }
                }
                if let Some(Num::F(x)) = a.as_num() {
                    return Expr::Num(Num::F(x.exp()));
                }
            }
            "log" => {
                if a.is_one() {
                    return Expr::zero();
                }
                if let Expr::Fn(n, inner) = &a {
                    if n == "exp" {
                        return inner[0].clone();
                    }
                }
                if let Some(Num::F(x)) = a.as_num() {
                    if x > 0.0 {
                        return Expr::Num(Num::F(x.ln()));
                    }
                }
            }
            "sqrt" => return Expr::pow(a, Expr::rat(1, 2)),
            "abs" => {
                if let Some(n) = a.as_num() {
                    return Expr::Num(if n.is_neg() { n.neg() } else { n });
                }
            }
            "sign" => {
                if let Some(n) = a.as_num() {
                    return Expr::int(if n.is_zero() { 0 } else if n.is_neg() { -1 } else { 1 });
                }
            }
            "asin" | "atan" | "sinh" | "tanh" | "asinh" | "atanh" | "erf" if a.is_zero() => return Expr::zero(),
            "erfc" if a.is_zero() => return Expr::one(),
            "cosh" if a.is_zero() => return Expr::one(),
            "acos" if a.is_one() => return Expr::zero(),
            "floor" | "ceil" | "round" | "fix" => {
                if let Some(n) = a.as_num() {
                    let x = n.to_f64();
                    return Expr::from_f64(match name {
                        "floor" => x.floor(),
                        "ceil" => x.ceil(),
                        "round" => x.round(),
                        _ => x.trunc(),
                    });
                }
            }
            _ => {}
        }
        // Remaining numeric (float) arguments evaluate.
        if args.len() == 1 {
            if let Some(Num::F(x)) = a.as_num() {
                if let Some(v) = eval_fn(name, C::new(x, 0.0)) {
                    if v.im == 0.0 {
                        return Expr::Num(Num::F(v.re));
                    }
                }
            }
        }
        Expr::Fn(name.to_string(), args)
    }

    pub fn rel(op: Rel, l: Expr, r: Expr) -> Expr {
        Expr::Rel(op, Box::new(l), Box::new(r))
    }

    pub fn contains_var(&self, v: &str) -> bool {
        match self {
            Expr::Var(n) => n == v,
            Expr::Num(_) | Expr::Const(_) => false,
            Expr::Add(xs) | Expr::Mul(xs) | Expr::Fn(_, xs) => xs.iter().any(|x| x.contains_var(v)),
            Expr::Pow(a, b) | Expr::Rel(_, a, b) => a.contains_var(v) || b.contains_var(v),
        }
    }

    pub fn free_vars(&self, out: &mut Vec<String>) {
        match self {
            Expr::Var(n) => {
                if !out.contains(n) {
                    out.push(n.clone());
                }
            }
            Expr::Num(_) | Expr::Const(_) => {}
            Expr::Add(xs) | Expr::Mul(xs) | Expr::Fn(_, xs) => xs.iter().for_each(|x| x.free_vars(out)),
            Expr::Pow(a, b) | Expr::Rel(_, a, b) => {
                a.free_vars(out);
                b.free_vars(out);
            }
        }
    }

    /// Rebuilds the expression bottom-up through the canonical constructors.
    pub fn map(&self, f: &dyn Fn(&Expr) -> Option<Expr>) -> Expr {
        if let Some(r) = f(self) {
            return r;
        }
        match self {
            Expr::Add(xs) => Expr::add(xs.iter().map(|x| x.map(f)).collect()),
            Expr::Mul(xs) => Expr::mul(xs.iter().map(|x| x.map(f)).collect()),
            Expr::Pow(a, b) => Expr::pow(a.map(f), b.map(f)),
            Expr::Fn(n, xs) => Expr::func(n, xs.iter().map(|x| x.map(f)).collect()),
            Expr::Rel(op, a, b) => Expr::rel(*op, a.map(f), b.map(f)),
            other => other.clone(),
        }
    }

    pub fn subs(&self, var: &str, val: &Expr) -> Expr {
        self.map(&|e| match e {
            Expr::Var(n) if n == var => Some(val.clone()),
            _ => None,
        })
    }

    /// Replaces exact numbers and constants with floating values (`vpa`).
    pub fn to_float(&self) -> Expr {
        self.map(&|e| match e {
            Expr::Num(n) => Some(Expr::Num(Num::F(n.to_f64()))),
            Expr::Const(Const::Pi) => Some(Expr::Num(Num::F(std::f64::consts::PI))),
            _ => None,
        })
    }

    /// Numeric value with variables looked up in `env`.
    pub fn eval_c(&self, env: &dyn Fn(&str) -> Option<C>) -> Option<C> {
        Some(match self {
            Expr::Num(n) => C::new(n.to_f64(), 0.0),
            Expr::Const(Const::Pi) => C::new(std::f64::consts::PI, 0.0),
            Expr::Const(Const::I) => C::new(0.0, 1.0),
            Expr::Const(Const::Inf) => C::new(f64::INFINITY, 0.0),
            Expr::Const(Const::NaN) => C::new(f64::NAN, 0.0),
            Expr::Var(n) => env(n)?,
            Expr::Add(xs) => {
                let mut acc = C::new(0.0, 0.0);
                for x in xs {
                    acc = acc.add(x.eval_c(env)?);
                }
                acc
            }
            Expr::Mul(xs) => {
                let mut acc = C::new(1.0, 0.0);
                for x in xs {
                    acc = acc.mul(x.eval_c(env)?);
                }
                acc
            }
            Expr::Pow(a, b) => {
                let (x, y) = (a.eval_c(env)?, b.eval_c(env)?);
                if x.im == 0.0 && y.im == 0.0 && (x.re >= 0.0 || y.re.fract() == 0.0) {
                    C::new(x.re.powf(y.re), 0.0)
                } else {
                    x.powc(y)
                }
            }
            Expr::Fn(n, xs) => {
                if xs.len() == 1 {
                    eval_fn(n, xs[0].eval_c(env)?)?
                } else if n == "atan2" && xs.len() == 2 {
                    C::new(xs[0].eval_c(env)?.re.atan2(xs[1].eval_c(env)?.re), 0.0)
                } else if (n == "mod" || n == "rem") && xs.len() == 2 {
                    let (x, m) = (xs[0].eval_c(env)?.re, xs[1].eval_c(env)?.re);
                    let r = if m == 0.0 {
                        if n == "mod" { x } else { f64::NAN }
                    } else {
                        x - if n == "mod" { (x / m).floor() } else { (x / m).trunc() } * m
                    };
                    C::new(r, 0.0)
                } else {
                    return None;
                }
            }
            Expr::Rel(..) => return None,
        })
    }

    pub fn to_f64(&self) -> Option<f64> {
        let z = self.eval_c(&|_| None)?;
        if z.im.abs() > 1e-12 * z.re.abs().max(1.0) {
            return None;
        }
        Some(z.re)
    }
}

pub fn eval_fn(name: &str, z: C) -> Option<C> {
    let real = |f: fn(f64) -> f64| if z.im == 0.0 { Some(C::new(f(z.re), 0.0)) } else { None };
    Some(match name {
        "sin" => z.sin(),
        "cos" => z.cos(),
        "tan" => z.tan(),
        "exp" => z.exp(),
        "log" => z.ln(),
        "sqrt" => z.sqrt(),
        "sinh" => z.sinh(),
        "cosh" => z.cosh(),
        "tanh" => z.tanh(),
        "asin" => z.asin(),
        "acos" => z.acos(),
        "atan" => z.atan(),
        "abs" => C::new(z.abs(), 0.0),
        "sign" => return real(|x| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 }),
        "floor" => return real(f64::floor),
        "ceil" => return real(f64::ceil),
        "round" => return real(f64::round),
        "fix" => return real(f64::trunc),
        "asinh" => return real(f64::asinh),
        "acosh" => return real(f64::acosh),
        "atanh" => return real(f64::atanh),
        "heaviside" => return real(|x| if x > 0.0 { 1.0 } else if x < 0.0 { 0.0 } else { 0.5 }),
        "erf" => return real(libm::erf),
        "erfc" => return real(libm::erfc),
        _ => return None,
    })
}

fn int_root(n: i128, r: i128) -> Option<i128> {
    if r <= 0 || n < 0 {
        return None;
    }
    if n <= 1 {
        return Some(n);
    }
    let guess = (n as f64).powf(1.0 / r as f64).round() as i128;
    for c in [guess - 1, guess, guess + 1] {
        if c >= 0 {
            let mut p: i128 = 1;
            let mut ok = true;
            for _ in 0..r {
                match p.checked_mul(c) {
                    Some(v) => p = v,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && p == n {
                return Some(c);
            }
        }
    }
    None
}

/// n = out² · inside with `inside` square-free.
fn extract_square(n: i128) -> (i128, i128) {
    let mut out = 1;
    let mut inside = n;
    let mut f = 2;
    while f * f <= inside {
        while inside % (f * f) == 0 {
            inside /= f * f;
            out *= f;
        }
        f += 1;
    }
    (out, inside)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x() -> Expr {
        Expr::var("x")
    }

    #[test]
    fn like_terms_and_powers() {
        let e = Expr::add(vec![x(), Expr::mul(vec![Expr::int(2), x()]), Expr::int(3), Expr::int(-3)]);
        assert_eq!(e, Expr::mul(vec![Expr::int(3), x()]));
        let p = Expr::mul(vec![x(), Expr::pow(x(), Expr::int(2))]);
        assert_eq!(p, Expr::pow(x(), Expr::int(3)));
        assert_eq!(Expr::sub(x(), x()), Expr::zero());
    }

    #[test]
    fn exact_numbers() {
        assert_eq!(Expr::add(vec![Expr::rat(1, 3), Expr::rat(1, 6)]), Expr::rat(1, 2));
        assert_eq!(Expr::pow(Expr::int(4), Expr::rat(1, 2)), Expr::int(2));
        assert_eq!(Expr::func("sin", vec![Expr::Const(Const::Pi)]), Expr::zero());
        assert_eq!(Num::from_f64(0.75), Num::rat(3, 4));
    }
}
