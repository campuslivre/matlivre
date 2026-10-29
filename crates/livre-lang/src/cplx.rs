//! Complex scalar arithmetic and elementary functions (principal branches, as MATLAB).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C {
    pub re: f64,
    pub im: f64,
}

impl C {
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    pub fn add(self, o: C) -> C {
        C::new(self.re + o.re, self.im + o.im)
    }

    pub fn sub(self, o: C) -> C {
        C::new(self.re - o.re, self.im - o.im)
    }

    pub fn mul(self, o: C) -> C {
        C::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }

    pub fn div(self, o: C) -> C {
        if o.im == 0.0 {
            return C::new(self.re / o.re, self.im / o.re);
        }
        // Smith's algorithm for robustness.
        if o.re.abs() >= o.im.abs() {
            let r = o.im / o.re;
            let d = o.re + o.im * r;
            C::new((self.re + self.im * r) / d, (self.im - self.re * r) / d)
        } else {
            let r = o.re / o.im;
            let d = o.re * r + o.im;
            C::new((self.re * r + self.im) / d, (self.im * r - self.re) / d)
        }
    }

    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }

    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }

    pub fn conj(self) -> C {
        C::new(self.re, -self.im)
    }

    pub fn exp(self) -> C {
        let e = self.re.exp();
        if self.im == 0.0 {
            return C::new(e, 0.0);
        }
        C::new(e * self.im.cos(), e * self.im.sin())
    }

    pub fn ln(self) -> C {
        C::new(self.abs().ln(), self.arg())
    }

    pub fn sqrt(self) -> C {
        if self.im == 0.0 {
            return if self.re >= 0.0 { C::new(self.re.sqrt(), 0.0) } else { C::new(0.0, (-self.re).sqrt()) };
        }
        let r = self.abs();
        let re = ((r + self.re) / 2.0).sqrt();
        let im = ((r - self.re) / 2.0).sqrt().copysign(self.im);
        C::new(re, im)
    }

    pub fn powc(self, e: C) -> C {
        if self.re == 0.0 && self.im == 0.0 {
            return if e.re == 0.0 && e.im == 0.0 { C::new(1.0, 0.0) } else { C::new(0.0, 0.0) };
        }
        // Exact integer powers keep real/integer results clean.
        if e.im == 0.0 && e.re.fract() == 0.0 && e.re.abs() <= 64.0 {
            let mut n = e.re.abs() as u32;
            let mut base = self;
            let mut acc = C::new(1.0, 0.0);
            while n > 0 {
                if n & 1 == 1 {
                    acc = acc.mul(base);
                }
                base = base.mul(base);
                n >>= 1;
            }
            return if e.re < 0.0 { C::new(1.0, 0.0).div(acc) } else { acc };
        }
        e.mul(self.ln()).exp()
    }

    pub fn sin(self) -> C {
        C::new(self.re.sin() * self.im.cosh(), self.re.cos() * self.im.sinh())
    }

    pub fn cos(self) -> C {
        C::new(self.re.cos() * self.im.cosh(), -self.re.sin() * self.im.sinh())
    }

    pub fn tan(self) -> C {
        self.sin().div(self.cos())
    }

    pub fn sinh(self) -> C {
        C::new(self.re.sinh() * self.im.cos(), self.re.cosh() * self.im.sin())
    }

    pub fn cosh(self) -> C {
        C::new(self.re.cosh() * self.im.cos(), self.re.sinh() * self.im.sin())
    }

    pub fn tanh(self) -> C {
        self.sinh().div(self.cosh())
    }

    /// asin(z) = -i ln(iz + sqrt(1 - z²))
    pub fn asin(self) -> C {
        let i = C::new(0.0, 1.0);
        let w = i.mul(self).add(C::new(1.0, 0.0).sub(self.mul(self)).sqrt()).ln();
        C::new(w.im, -w.re)
    }

    /// acos(z) = π/2 - asin(z)
    pub fn acos(self) -> C {
        let a = self.asin();
        C::new(std::f64::consts::FRAC_PI_2 - a.re, -a.im)
    }

    /// atan(z) = i/2 (ln(1 - iz) - ln(1 + iz))
    pub fn atan(self) -> C {
        let i = C::new(0.0, 1.0);
        let iz = i.mul(self);
        let d = C::new(1.0, 0.0).sub(iz).ln().sub(C::new(1.0, 0.0).add(iz).ln());
        C::new(-d.im / 2.0, d.re / 2.0)
    }
}

/// Roots of a polynomial with complex coefficients (highest degree first) by the
/// Durand–Kerner iteration, polished with Newton steps.
pub fn poly_roots(coeffs: &[C]) -> Vec<C> {
    let mut c: Vec<C> = coeffs.iter().copied().skip_while(|z| z.re == 0.0 && z.im == 0.0).collect();
    // Trailing zeros are roots at the origin.
    let mut zeros = 0;
    while c.len() > 1 && c.last().map(|z| z.re == 0.0 && z.im == 0.0).unwrap_or(false) {
        c.pop();
        zeros += 1;
    }
    let n = c.len().saturating_sub(1);
    let mut roots = Vec::new();
    if n >= 1 {
        let lead = c[0];
        let monic: Vec<C> = c.iter().map(|z| z.div(lead)).collect();
        let eval = |z: C| monic.iter().fold(C::new(0.0, 0.0), |acc, &a| acc.mul(z).add(a));
        let bound = 1.0 + monic[1..].iter().map(|z| z.abs()).fold(0.0, f64::max);
        let mut r: Vec<C> = (0..n)
            .map(|k| {
                let ang = 2.0 * std::f64::consts::PI * k as f64 / n as f64 + 0.4;
                C::new(bound * 0.9 * ang.cos(), bound * 0.9 * ang.sin())
            })
            .collect();
        for _ in 0..2000 {
            let mut change: f64 = 0.0;
            for i in 0..n {
                let mut den = C::new(1.0, 0.0);
                for j in 0..n {
                    if i != j {
                        den = den.mul(r[i].sub(r[j]));
                    }
                }
                if den.abs() == 0.0 {
                    continue;
                }
                let delta = eval(r[i]).div(den);
                r[i] = r[i].sub(delta);
                change = change.max(delta.abs());
            }
            if change < 1e-15 {
                break;
            }
        }
        // Clean tiny imaginary/real parts relative to magnitude.
        for z in r.iter_mut() {
            let m = z.abs().max(1.0);
            if z.im.abs() < 1e-10 * m {
                z.im = 0.0;
            }
            if z.re.abs() < 1e-12 * m {
                z.re = 0.0;
            }
        }
        roots = r;
    }
    roots.extend(std::iter::repeat(C::new(0.0, 0.0)).take(zeros));
    // MATLAB returns roots in descending order of magnitude-ish; sort by real desc then imag desc.
    roots.sort_by(|a, b| b.re.partial_cmp(&a.re).unwrap_or(std::cmp::Ordering::Equal).then(b.im.partial_cmp(&a.im).unwrap_or(std::cmp::Ordering::Equal)));
    roots
}
