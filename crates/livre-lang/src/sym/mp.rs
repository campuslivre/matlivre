//! Variable-precision arithmetic for `vpa`: binary floating values m * 2^e at a fixed working
//! precision, and the elementary functions on them. An expression without variables is
//! evaluated with a margin of guard bits and then rounded to `digits()` decimal digits.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::expr::{digits, Const, Expr, Num};

/// m * 2^e.
#[derive(Clone, Debug)]
struct F {
    m: BigInt,
    e: i64,
}

/// Working precision in bits.
#[derive(Clone, Copy)]
struct Ctx {
    prec: u64,
}

fn shift_round(m: &BigInt, s: u64) -> BigInt {
    if s == 0 {
        return m.clone();
    }
    // Round half away from zero.
    let half = BigInt::one() << (s - 1) as usize;
    if m.is_negative() {
        -((-m + half) >> s as usize)
    } else {
        (m + half) >> s as usize
    }
}

impl Ctx {
    fn more(self, bits: u64) -> Ctx {
        Ctx { prec: self.prec + bits }
    }

    fn norm(&self, m: BigInt, e: i64) -> F {
        let b = m.bits();
        if b > self.prec {
            let s = b - self.prec;
            F { m: shift_round(&m, s), e: e + s as i64 }
        } else {
            F { m, e }
        }
    }

    fn int(&self, n: i64) -> F {
        F { m: BigInt::from(n), e: 0 }
    }

    fn rat(&self, r: &BigRational) -> F {
        let (p, q) = (r.numer(), r.denom());
        if p.is_zero() {
            return self.int(0);
        }
        let k = self.prec as i64 + q.bits() as i64 - p.bits() as i64 + 2;
        let (num, den) = if k >= 0 { (p << k as usize, q.clone()) } else { (p.clone(), q << (-k) as usize) };
        self.norm(num.div_floor(&den), -k)
    }

    fn to_rat(&self, a: &F) -> BigRational {
        if a.e >= 0 {
            BigRational::from_integer(&a.m << a.e as usize)
        } else {
            BigRational::new(a.m.clone(), BigInt::one() << (-a.e) as usize)
        }
    }

    fn add(&self, a: &F, b: &F) -> F {
        if a.m.is_zero() {
            return b.clone();
        }
        if b.m.is_zero() {
            return a.clone();
        }
        let (hi, lo) = if a.e >= b.e { (a, b) } else { (b, a) };
        let gap = (hi.e - lo.e) as u64;
        // A term whose magnitude is far below the precision of the other only rounds.
        let mag = |x: &F| x.m.bits() as i64 + x.e;
        if mag(hi) - mag(lo) > self.prec as i64 + 8 {
            return if mag(hi) >= mag(lo) { hi.clone() } else { lo.clone() };
        }
        self.norm((&hi.m << gap as usize) + &lo.m, lo.e)
    }

    fn sub(&self, a: &F, b: &F) -> F {
        self.add(a, &neg(b))
    }

    fn mul(&self, a: &F, b: &F) -> F {
        self.norm(&a.m * &b.m, a.e + b.e)
    }

    fn div(&self, a: &F, b: &F) -> Option<F> {
        if b.m.is_zero() {
            return None;
        }
        if a.m.is_zero() {
            return Some(self.int(0));
        }
        let k = self.prec as i64 + b.m.bits() as i64 - a.m.bits() as i64 + 2;
        let (num, den) = if k >= 0 { (&a.m << k as usize, b.m.clone()) } else { (a.m.clone(), &b.m << (-k) as usize) };
        Some(self.norm(num.div_floor(&den), a.e - b.e - k))
    }

    fn sqrt(&self, a: &F) -> Option<F> {
        if a.m.is_negative() {
            return None;
        }
        if a.m.is_zero() {
            return Some(self.int(0));
        }
        // Scale to about 2*prec bits with an even exponent.
        let mut s = (2 * self.prec as i64 + 4 - a.m.bits() as i64).max(0);
        if (a.e - s) % 2 != 0 {
            s += 1;
        }
        let r = (&a.m << s as usize).sqrt();
        Some(self.norm(r, (a.e - s) / 2))
    }

    fn approx(&self, a: &F) -> f64 {
        let b = a.m.bits() as i64;
        let s = (b - 60).max(0);
        let m = (&a.m >> s as usize).to_f64().unwrap_or(0.0);
        m * 2f64.powf((a.e + s) as f64)
    }

    /// Smallest magnitude worth adding: 2^-(prec + guard) relative to 1.
    fn tiny(&self, a: &F) -> bool {
        a.m.is_zero() || (a.m.bits() as i64 + a.e) < -(self.prec as i64) - 8
    }

    fn pi(&self) -> F {
        // Machin: pi = 16*atan(1/5) - 4*atan(1/239).
        let c = self.more(16);
        let a = c.atan_inv(5);
        let b = c.atan_inv(239);
        let p = c.sub(&c.mul(&c.int(16), &a), &c.mul(&c.int(4), &b));
        self.norm(p.m, p.e)
    }

    /// atan(1/n) for an integer n > 1.
    fn atan_inv(&self, n: i64) -> F {
        let x = self.div(&self.int(1), &self.int(n)).unwrap();
        let x2 = self.mul(&x, &x);
        let mut term = x.clone();
        let mut sum = x;
        let mut k = 1i64;
        loop {
            term = self.mul(&term, &x2);
            let t = self.div(&term, &self.int(2 * k + 1)).unwrap();
            if self.tiny(&t) {
                break;
            }
            sum = if k % 2 == 1 { self.sub(&sum, &t) } else { self.add(&sum, &t) };
            k += 1;
        }
        sum
    }

    fn ln2(&self) -> F {
        // ln 2 = sum 1/(k 2^k).
        let c = self.more(16);
        let mut sum = c.int(0);
        let mut k = 1i64;
        loop {
            let inv = c.div(&c.int(1), &c.int(k)).unwrap();
            let t = F { m: inv.m, e: inv.e - k };
            if c.tiny(&t) {
                break;
            }
            sum = c.add(&sum, &t);
            k += 1;
        }
        self.norm(sum.m, sum.e)
    }

    fn exp(&self, x: &F) -> Option<F> {
        if x.m.is_zero() {
            return Some(self.int(1));
        }
        let xf = self.approx(x);
        if xf.abs() > 1e7 {
            return None;
        }
        let c = self.more(32 + (xf.abs().log2().max(0.0) as u64));
        let ln2 = c.ln2();
        let n = (xf / std::f64::consts::LN_2).round() as i64;
        let r = c.sub(x, &c.mul(&c.int(n), &ln2));
        // exp(r) = exp(r / 2^8)^(2^8).
        let h = F { m: r.m.clone(), e: r.e - 8 };
        let mut term = c.int(1);
        let mut sum = c.int(1);
        let mut k = 1i64;
        loop {
            term = c.div(&c.mul(&term, &h), &c.int(k)).unwrap();
            if c.tiny(&term) {
                break;
            }
            sum = c.add(&sum, &term);
            k += 1;
        }
        for _ in 0..8 {
            sum = c.mul(&sum, &sum);
        }
        Some(self.norm(sum.m, sum.e + n))
    }

    fn ln(&self, x: &F) -> Option<F> {
        if !x.m.is_positive() {
            return None;
        }
        let c = self.more(32);
        // x = y * 2^k with y in [0.5, 1).
        let k = x.m.bits() as i64 + x.e;
        let y = F { m: x.m.clone(), e: x.e - k };
        let one = c.int(1);
        let z = c.div(&c.sub(&y, &one), &c.add(&y, &one)).unwrap();
        let z2 = c.mul(&z, &z);
        let mut term = z.clone();
        let mut sum = z;
        let mut j = 1i64;
        loop {
            term = c.mul(&term, &z2);
            let t = c.div(&term, &c.int(2 * j + 1)).unwrap();
            if c.tiny(&t) {
                break;
            }
            sum = c.add(&sum, &t);
            j += 1;
        }
        let ln_y = F { m: sum.m, e: sum.e + 1 };
        let r = c.add(&ln_y, &c.mul(&c.int(k), &c.ln2()));
        Some(self.norm(r.m, r.e))
    }

    /// sin (or cos) by reducing modulo 2*pi and summing the series.
    fn sin_cos(&self, x: &F, cos: bool) -> Option<F> {
        let xf = self.approx(x);
        if xf.abs() > 1e15 {
            return None;
        }
        let c = self.more(32 + (xf.abs().log2().max(0.0) as u64));
        let two_pi = F { m: c.pi().m, e: c.pi().e + 1 };
        let k = (xf / (2.0 * std::f64::consts::PI)).round() as i64;
        let r = c.sub(x, &c.mul(&c.int(k), &two_pi));
        let r2 = c.mul(&r, &r);
        let (mut term, mut n) = if cos { (c.int(1), 0i64) } else { (r.clone(), 1i64) };
        let mut sum = term.clone();
        loop {
            term = c.div(&c.mul(&term, &r2), &c.int((n + 1) * (n + 2))).unwrap();
            term = neg(&term);
            n += 2;
            if c.tiny(&term) {
                break;
            }
            sum = c.add(&sum, &term);
        }
        Some(self.norm(sum.m, sum.e))
    }

    fn atan(&self, x: &F) -> F {
        if x.m.is_zero() {
            return self.int(0);
        }
        let c = self.more(32);
        let neg_in = x.m.is_negative();
        let a = abs(x);
        let one = c.int(1);
        let big = c.approx(&a) > 1.0;
        let mut y = if big { c.div(&one, &a).unwrap() } else { a };
        // Two halvings: atan(y) = 2*atan(y / (1 + sqrt(1 + y^2))).
        for _ in 0..2 {
            let s = c.sqrt(&c.add(&one, &c.mul(&y, &y))).unwrap();
            y = c.div(&y, &c.add(&one, &s)).unwrap();
        }
        let y2 = c.mul(&y, &y);
        let mut term = y.clone();
        let mut sum = y;
        let mut k = 1i64;
        loop {
            term = c.mul(&term, &y2);
            let t = c.div(&term, &c.int(2 * k + 1)).unwrap();
            if c.tiny(&t) {
                break;
            }
            sum = if k % 2 == 1 { c.sub(&sum, &t) } else { c.add(&sum, &t) };
            k += 1;
        }
        let mut r = F { m: sum.m, e: sum.e + 2 };
        if big {
            let half_pi = F { m: c.pi().m, e: c.pi().e - 1 };
            r = c.sub(&half_pi, &r);
        }
        if neg_in {
            r = neg(&r);
        }
        self.norm(r.m, r.e)
    }

    /// erf(x), with erfc(x) = 1 - erf(x) evaluated at enough extra precision to keep its
    /// relative accuracy.
    fn erf(&self, x: &F, complement: bool) -> Option<F> {
        let xf = self.approx(x).abs();
        let extra = (xf * xf * std::f64::consts::LOG2_E).ceil() as u64 + 32;
        if complement && xf > 40.0 {
            return None;
        }
        let one = self.int(1);
        if !complement && xf * xf * std::f64::consts::LOG2_E > self.prec as f64 + 16.0 {
            let r = if x.m.is_negative() { neg(&one) } else { one };
            return Some(r);
        }
        let c = self.more(extra);
        let a = abs(x);
        let a2 = c.mul(&a, &a);
        let two_a2 = F { m: a2.m.clone(), e: a2.e + 1 };
        // erf(a) = 2/sqrt(pi) * exp(-a^2) * sum a^(2n+1) 2^n / (1*3*...*(2n+1)).
        let damp = c.exp(&neg(&a2))?;
        let mut term = a.clone();
        let mut sum = a;
        let mut n = 1i64;
        loop {
            term = c.div(&c.mul(&term, &two_a2), &c.int(2 * n + 1)).unwrap();
            // Terms rise and then fall; stop once they no longer matter after damping.
            if n > 2 && c.approx(&term) <= c.approx(&sum) && c.tiny(&c.mul(&term, &damp)) {
                break;
            }
            sum = c.add(&sum, &term);
            n += 1;
        }
        let factor = c.div(&F { m: damp.m.clone(), e: damp.e + 1 }, &c.sqrt(&c.pi())?)?;
        let mut erf = c.mul(&factor, &sum);
        if x.m.is_negative() {
            erf = neg(&erf);
        }
        let r = if complement { c.sub(&c.int(1), &erf) } else { erf };
        Some(self.norm(r.m, r.e))
    }

    fn pow(&self, a: &F, b: &Expr) -> Option<F> {
        if let Some(Num::Rat(n, 1)) = b.as_num() {
            if n.unsigned_abs() <= 1 << 20 {
                let mut acc = self.int(1);
                let mut base = a.clone();
                let mut k = n.unsigned_abs();
                while k > 0 {
                    if k & 1 == 1 {
                        acc = self.mul(&acc, &base);
                    }
                    k >>= 1;
                    if k > 0 {
                        base = self.mul(&base, &base);
                    }
                }
                return if n < 0 { self.div(&self.int(1), &acc) } else { Some(acc) };
            }
        }
        let y = self.eval(b)?;
        if a.m.is_zero() {
            return if y.m.is_positive() { Some(self.int(0)) } else { None };
        }
        if let Some(Num::Rat(1, 2)) = b.as_num() {
            return self.sqrt(a);
        }
        if a.m.is_negative() {
            return None;
        }
        let c = self.more(32);
        let r = c.exp(&c.mul(&y, &c.ln(a)?))?;
        Some(self.norm(r.m, r.e))
    }

    fn eval(&self, e: &Expr) -> Option<F> {
        Some(match e {
            Expr::Num(Num::F(x)) => self.rat(&BigRational::from_float(*x)?),
            Expr::Num(n) => self.rat(&n.exact()?),
            Expr::Const(Const::Pi) => self.pi(),
            Expr::Const(_) | Expr::Var(_) | Expr::Rel(..) => return None,
            Expr::Add(xs) => {
                let c = self.more(64);
                let mut s = c.int(0);
                for x in xs {
                    s = c.add(&s, &c.eval(x)?);
                }
                self.norm(s.m, s.e)
            }
            Expr::Mul(xs) => {
                let mut p = self.int(1);
                for x in xs {
                    p = self.mul(&p, &self.eval(x)?);
                }
                p
            }
            Expr::Pow(a, b) => {
                let base = self.eval(a)?;
                self.pow(&base, b)?
            }
            Expr::Fn(name, args) => {
                let x = self.eval(args.first()?)?;
                let one = self.int(1);
                let half_pi = || F { m: self.pi().m, e: self.pi().e - 1 };
                match name.as_str() {
                    "sin" => self.sin_cos(&x, false)?,
                    "cos" => self.sin_cos(&x, true)?,
                    "tan" => self.div(&self.sin_cos(&x, false)?, &self.sin_cos(&x, true)?)?,
                    "exp" => self.exp(&x)?,
                    "log" => self.ln(&x)?,
                    "sqrt" => self.sqrt(&x)?,
                    "atan" => self.atan(&x),
                    "asin" | "acos" => {
                        let s = self.sub(&one, &self.mul(&x, &x));
                        if s.m.is_negative() {
                            return None;
                        }
                        let asin = if s.m.is_zero() {
                            if x.m.is_negative() { neg(&half_pi()) } else { half_pi() }
                        } else {
                            self.atan(&self.div(&x, &self.sqrt(&s)?)?)
                        };
                        if name == "asin" { asin } else { self.sub(&half_pi(), &asin) }
                    }
                    "sinh" | "cosh" | "tanh" => {
                        let c = self.more(64);
                        let ep = c.exp(&x)?;
                        let em = c.div(&c.int(1), &ep)?;
                        let r = match name.as_str() {
                            "sinh" => c.sub(&ep, &em),
                            "cosh" => c.add(&ep, &em),
                            _ => return self.div(&c.sub(&ep, &em), &c.add(&ep, &em)),
                        };
                        F { m: r.m, e: r.e - 1 }
                    }
                    "asinh" => {
                        let a = abs(&x);
                        let r = self.ln(&self.add(&a, &self.sqrt(&self.add(&self.mul(&a, &a), &one))?))?;
                        if x.m.is_negative() { neg(&r) } else { r }
                    }
                    "acosh" => {
                        let s = self.sub(&self.mul(&x, &x), &one);
                        if s.m.is_negative() || x.m.is_negative() {
                            return None;
                        }
                        self.ln(&self.add(&x, &self.sqrt(&s)?))?
                    }
                    "atanh" => {
                        let r = self.ln(&self.div(&self.add(&one, &x), &self.sub(&one, &x))?)?;
                        F { m: r.m, e: r.e - 1 }
                    }
                    "erf" => self.erf(&x, false)?,
                    "erfc" => self.erf(&x, true)?,
                    "abs" => abs(&x),
                    "sign" => self.int(x.m.signum().to_i64().unwrap_or(0)),
                    "floor" | "ceil" | "round" | "fix" => {
                        let r = self.to_rat(&x);
                        let n = match name.as_str() {
                            "floor" => r.floor(),
                            "ceil" => r.ceil(),
                            "round" => r.round(),
                            _ => r.trunc(),
                        };
                        self.rat(&n)
                    }
                    "mod" | "rem" => {
                        let m = self.eval(args.get(1)?)?;
                        if m.m.is_zero() {
                            return if name == "mod" { Some(x) } else { None };
                        }
                        let q = self.to_rat(&x) / self.to_rat(&m);
                        let k = if name == "mod" { q.floor() } else { q.trunc() };
                        self.rat(&(self.to_rat(&x) - k * self.to_rat(&m)))
                    }
                    "heaviside" => match x.m.signum().to_i64() {
                        Some(1) => one,
                        Some(-1) => self.int(0),
                        _ => F { m: BigInt::one(), e: -1 },
                    },
                    _ => return None,
                }
            }
        })
    }
}

fn neg(a: &F) -> F {
    F { m: -&a.m, e: a.e }
}

fn abs(a: &F) -> F {
    F { m: a.m.abs(), e: a.e }
}

/// The value of a variable-free real expression to `digits()` significant digits, or None
/// when it is complex, non-finite or uses a function this evaluator does not cover.
pub fn eval_digits(e: &Expr) -> Option<BigRational> {
    let bits = (digits() as f64 * std::f64::consts::LOG2_10).ceil() as u64 + 64;
    let c = Ctx { prec: bits };
    let v = c.eval(e)?;
    Some(c.to_rat(&v))
}

/// `vpa(e)`: numbers and variable-free subexpressions become vpa values; symbolic
/// variables and the structure around them are kept (vpa(x/3) is 0.333...*x).
pub fn vpa(e: &Expr) -> Expr {
    let mut vars = Vec::new();
    e.free_vars(&mut vars);
    if vars.is_empty() {
        if let Some(r) = eval_digits(e) {
            return Expr::Num(Num::vpa(&r));
        }
    }
    match e {
        Expr::Num(n) => match n.exact() {
            Some(r) => Expr::Num(Num::vpa(&r)),
            None => e.clone(),
        },
        Expr::Add(xs) => Expr::add(xs.iter().map(vpa).collect()),
        Expr::Mul(xs) => Expr::mul(xs.iter().map(vpa).collect()),
        // Integer and rational exponents stay exact, as MATLAB keeps x^2.
        Expr::Pow(a, b) => Expr::pow(vpa(a), if b.as_num().is_some() { (**b).clone() } else { vpa(b) }),
        Expr::Fn(name, xs) => Expr::func(name, xs.iter().map(vpa).collect()),
        Expr::Rel(r, a, b) => Expr::Rel(*r, Box::new(vpa(a)), Box::new(vpa(b))),
        Expr::Const(Const::Pi) => e.to_float(),
        other => other.clone(),
    }
}

/// A variable-free real expression as the double nearest its exact value (None when it is
/// complex, non-finite or not covered here).
pub fn to_f64(e: &Expr) -> Option<f64> {
    // Plain numbers convert directly.
    if let Expr::Num(n) = e {
        return Some(n.to_f64());
    }
    let c = Ctx { prec: 128 };
    let v = c.eval(e)?;
    Some(super::expr::rational_to_f64(&c.to_rat(&v)))
}
