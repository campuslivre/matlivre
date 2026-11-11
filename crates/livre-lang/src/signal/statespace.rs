//! Single-input single-output state-space models for the IIR designs. MATLAB designs `butter`,
//! `cheby1` and `cheby2` through state space (a cascade realisation of the analog prototype, a
//! frequency transformation, and the bilinear transform), and the order of the poles it returns
//! is the order of the eigenvalues of the final state matrix, so the same path is followed here.

use crate::cplx::C;

/// x' = A x + B u, y = C x + D u with A stored column-major.
#[derive(Clone, Debug)]
pub struct Ss {
    pub n: usize,
    pub a: Vec<f64>,
    pub b: Vec<f64>,
    pub c: Vec<f64>,
    pub d: f64,
}

impl Ss {
    fn at(&self, i: usize, j: usize) -> f64 {
        self.a[i + j * self.n]
    }
}

/// Solves M X = R for X (M n x n, R n x m, both column-major) by LU with partial pivoting.
fn solve(n: usize, m: &[f64], r: &[f64], cols: usize) -> Vec<f64> {
    solve_numeric(n, m, r, cols)
}

fn solve_numeric(n: usize, m: &[f64], r: &[f64], cols: usize) -> Vec<f64> {
    // Triangular systems are solved by substitution, as MATLAB does, which keeps exact zeros.
    let lower = (0..n).all(|j| (0..j).all(|i| m[i + j * n] == 0.0));
    let upper = (0..n).all(|j| (j + 1..n).all(|i| m[i + j * n] == 0.0));
    if lower || upper {
        let mut x = r.to_vec();
        for c in 0..cols {
            if lower {
                for i in 0..n {
                    let mut s = x[i + c * n];
                    for j in 0..i {
                        s -= m[i + j * n] * x[j + c * n];
                    }
                    x[i + c * n] = s / m[i + i * n];
                }
            } else {
                for i in (0..n).rev() {
                    let mut s = x[i + c * n];
                    for j in i + 1..n {
                        s -= m[i + j * n] * x[j + c * n];
                    }
                    x[i + c * n] = s / m[i + i * n];
                }
            }
        }
        return x;
    }
    let mut a = m.to_vec();
    let mut x = r.to_vec();
    for k in 0..n {
        // The first row holding the largest magnitude, as LAPACK's idamax picks it.
        let mut p = k;
        for i in k + 1..n {
            if a[i + k * n].abs() > a[p + k * n].abs() {
                p = i;
            }
        }
        if p != k {
            for c in 0..n {
                a.swap(k + c * n, p + c * n);
            }
            for c in 0..cols {
                x.swap(k + c * n, p + c * n);
            }
        }
        let piv = a[k + k * n];
        for i in k + 1..n {
            let f = a[i + k * n] / piv;
            a[i + k * n] = f;
            for c in k + 1..n {
                a[i + c * n] -= f * a[k + c * n];
            }
            for c in 0..cols {
                x[i + c * n] -= f * x[k + c * n];
            }
        }
    }
    for c in 0..cols {
        for i in (0..n).rev() {
            let mut s = x[i + c * n];
            for j in i + 1..n {
                s -= a[i + j * n] * x[j + c * n];
            }
            x[i + c * n] = s / a[i + i * n];
        }
    }
    x
}

fn identity(n: usize) -> Vec<f64> {
    let mut v = vec![0.0; n * n];
    for i in 0..n {
        v[i + i * n] = 1.0;
    }
    v
}

/// Row vector times matrix: y = c M.
fn row_times(n: usize, c: &[f64], m: &[f64]) -> Vec<f64> {
    (0..n).map(|j| (0..n).map(|i| c[i] * m[i + j * n]).sum()).collect()
}

/// Cascade realisation of k * prod(s - z) / prod(s - p) for conjugate-closed `z` and `p`.
///
/// The sections follow MATLAB's `zp2ss`: a single real pole (the one nearest the origin) when
/// the number of real poles is odd, the complex pairs with the most negative real part first,
/// then pairs of real poles from the largest magnitude down. A second-order section with
/// denominator s^2 + a s + w^2 is [-a -w; w 0] with B = [1; 0]; its numerator is set through C
/// and the feedthrough D. The zeros go to the second-order sections in order, a pair each,
/// largest magnitude first; a section without zeros has C = [0 1/w].
pub fn zp2ss(z: &[C], p: &[C], k: f64) -> Ss {
    let tiny = |r: &C| r.im.abs() <= 1e-9 * r.abs().max(1.0);
    let mut cp: Vec<C> = p.iter().filter(|r| !tiny(r) && r.im > 0.0).copied().collect();
    cp.sort_by(|a, b| a.re.partial_cmp(&b.re).unwrap_or(std::cmp::Ordering::Equal));
    let mut rp: Vec<f64> = p.iter().filter(|r| tiny(r)).map(|r| r.re).collect();
    rp.sort_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap_or(std::cmp::Ordering::Equal));
    enum Sec {
        One(f64),
        Two(f64, f64),
    }
    let mut secs = Vec::new();
    if rp.len() % 2 == 1 {
        secs.push(Sec::One(rp.remove(0)));
    }
    for q in &cp {
        secs.push(Sec::Two(-2.0 * q.re, q.re * q.re + q.im * q.im));
    }
    rp.sort_by(|a, b| b.abs().partial_cmp(&a.abs()).unwrap_or(std::cmp::Ordering::Equal));
    for pair in rp.chunks(2) {
        secs.push(Sec::Two(-(pair[0] + pair[1]), pair[0] * pair[1]));
    }
    // Zeros as numerator factors (s^2 + n1 s + n0) or (s + n0), largest magnitude first.
    let mut zc: Vec<C> = z.iter().filter(|r| !tiny(r) && r.im > 0.0).copied().collect();
    zc.sort_by(|a, b| b.abs().partial_cmp(&a.abs()).unwrap_or(std::cmp::Ordering::Equal));
    let mut zr: Vec<f64> = z.iter().filter(|r| tiny(r)).map(|r| r.re).collect();
    zr.sort_by(|a, b| b.abs().partial_cmp(&a.abs()).unwrap_or(std::cmp::Ordering::Equal));
    let mut groups: Vec<(usize, f64, f64)> = zc.iter().map(|q| (2, -2.0 * q.re, q.re * q.re + q.im * q.im)).collect();
    for pair in zr.chunks(2) {
        if pair.len() == 2 {
            groups.push((2, -(pair[0] + pair[1]), pair[0] * pair[1]));
        } else {
            groups.push((1, 0.0, -pair[0]));
        }
    }
    let mut groups = groups.into_iter();
    let n: usize = secs.iter().map(|s| if matches!(s, Sec::One(_)) { 1 } else { 2 }).sum();
    let mut a = vec![0.0; n * n];
    let mut b = vec![0.0; n];
    let mut c = vec![0.0; n];
    let mut pos = 0;
    // Row vector of the cascade output over the states so far, and the product of feedthroughs.
    let mut y: Vec<f64> = Vec::new();
    let mut dprod = 1.0;
    let mut sections: Vec<(usize, Vec<f64>, Vec<f64>, f64)> = Vec::new();
    for sec in &secs {
        match *sec {
            Sec::One(r) => sections.push((1, vec![r], vec![1.0], 0.0)),
            Sec::Two(aa, cc) => {
                let w = cc.sqrt();
                let (c1, c2, d) = match groups.next() {
                    Some((2, n1, n0)) => (n1 - aa, (n0 - cc) / w, 1.0),
                    Some((_, _, n0)) => (1.0, n0 / w, 0.0),
                    None => (0.0, 1.0 / w, 0.0),
                };
                sections.push((2, vec![-aa, w, -w, 0.0], vec![c1, c2], d));
            }
        }
    }
    // Zeros left over (more groups than second-order sections) go to the first-order sections.
    for (idx, sec) in secs.iter().enumerate() {
        if let (Sec::One(r), Some((_, _, n0))) = (sec, groups.next()) {
            sections[idx] = (1, vec![*r], vec![r + n0], 1.0);
        }
    }
    for (m, am, cs, d) in sections {
        let bs: Vec<f64> = if m == 1 { vec![1.0] } else { vec![1.0, 0.0] };
        for i in 0..m {
            for j in 0..m {
                a[(pos + i) + (pos + j) * n] = am[i + j * m];
            }
        }
        if y.is_empty() {
            for i in 0..m {
                b[pos + i] = bs[i] * dprod;
            }
        } else {
            for i in 0..m {
                for (j, yj) in y.iter().enumerate() {
                    a[(pos + i) + j * n] = bs[i] * yj;
                }
                b[pos + i] = bs[i] * dprod;
            }
        }
        // y_new = d * y_old + C_s x_s
        let mut ynew: Vec<f64> = y.iter().map(|v| v * d).collect();
        ynew.extend(&cs);
        y = ynew;
        dprod *= d;
        pos += m;
    }
    for (j, yj) in y.iter().enumerate() {
        c[j] = yj * k;
    }
    Ss { n, a, b, c, d: dprod * k }
}

pub fn lp2lp(s: &Ss, wo: f64) -> Ss {
    Ss { n: s.n, a: s.a.iter().map(|v| v * wo).collect(), b: s.b.iter().map(|v| v * wo).collect(), c: s.c.clone(), d: s.d }
}

pub fn lp2hp(s: &Ss, wo: f64) -> Ss {
    let n = s.n;
    // at = wo inv(a); bt = -wo (a\b); ct = c/a; dt = d - c/a b
    let inv = right_inverse(n, &s.a);
    let ab = solve(n, &s.a, &s.b, 1);
    // c / a = c inv(a)
    let ca = row_times(n, &s.c, &inv);
    let dt = s.d - ca.iter().zip(&s.b).map(|(x, y)| x * y).sum::<f64>();
    Ss { n, a: inv.iter().map(|v| v * wo).collect(), b: ab.iter().map(|v| -wo * v).collect(), c: ca, d: dt }
}

pub fn lp2bp(s: &Ss, wo: f64, bw: f64) -> Ss {
    let n = s.n;
    let q = wo / bw;
    let m = 2 * n;
    let mut a = vec![0.0; m * m];
    for i in 0..n {
        for j in 0..n {
            a[i + j * m] = wo * s.at(i, j) / q;
        }
        a[i + (n + i) * m] = wo;
        a[(n + i) + i * m] = -wo;
    }
    let mut b = vec![0.0; m];
    let mut c = vec![0.0; m];
    for i in 0..n {
        b[i] = wo * s.b[i] / q;
        c[i] = s.c[i];
    }
    Ss { n: m, a, b, c, d: s.d }
}

pub fn lp2bs(s: &Ss, wo: f64, bw: f64) -> Ss {
    let n = s.n;
    let q = wo / bw;
    let m = 2 * n;
    let inv = right_inverse(n, &s.a);
    let ab = solve(n, &s.a, &s.b, 1);
    let ca = row_times(n, &s.c, &inv);
    let mut a = vec![0.0; m * m];
    for i in 0..n {
        for j in 0..n {
            a[i + j * m] = wo / q * inv[i + j * n];
        }
        a[i + (n + i) * m] = wo;
        a[(n + i) + i * m] = -wo;
    }
    let mut b = vec![0.0; m];
    let mut c = vec![0.0; m];
    for i in 0..n {
        b[i] = -(wo / q) * ab[i];
        c[i] = ca[i];
    }
    let d = s.d - ca.iter().zip(&s.b).map(|(x, y)| x * y).sum::<f64>();
    Ss { n: m, a, b, c, d }
}

/// Bilinear transform with sample rate `fs`, scaled symmetrically as MATLAB does.
pub fn bilinear(s: &Ss, fs: f64) -> Ss {
    let n = s.n;
    let t = 1.0 / fs;
    let r = t.sqrt();
    let t1: Vec<f64> = (0..n * n).map(|i| (if i % (n + 1) == 0 { 1.0 } else { 0.0 }) + s.a[i] * t / 2.0).collect();
    let t2: Vec<f64> = (0..n * n).map(|i| (if i % (n + 1) == 0 { 1.0 } else { 0.0 }) - s.a[i] * t / 2.0).collect();
    let ad = solve(n, &t2, &t1, n);
    let bd: Vec<f64> = solve(n, &t2, &s.b, 1).iter().map(|v| t / r * v).collect();
    // c / t2 = c inv(t2)
    let inv2 = solve(n, &t2, &identity(n), n);
    let ct = row_times(n, &s.c, &inv2);
    let cd: Vec<f64> = ct.iter().map(|v| r * v).collect();
    let dd = ct.iter().zip(&s.b).map(|(x, y)| x * y).sum::<f64>() * t / 2.0 + s.d;
    Ss { n, a: ad, b: bd, c: cd, d: dd }
}

/// Matrix inverse the way LAPACK's `dgetri` forms it: LU with partial pivoting, the inverse of U,
/// then the solution of X L = inv(U). The rounding pattern (which entries come out exactly zero)
/// is that of MATLAB's `inv`.
pub fn inverse(n: usize, m: &[f64]) -> Vec<f64> {
    let mut a = m.to_vec();
    let mut piv = vec![0usize; n];
    for k in 0..n {
        let mut p = k;
        for i in k + 1..n {
            if a[i + k * n].abs() > a[p + k * n].abs() {
                p = i;
            }
        }
        piv[k] = p;
        if p != k {
            for c in 0..n {
                a.swap(k + c * n, p + c * n);
            }
        }
        let d = a[k + k * n];
        for i in k + 1..n {
            a[i + k * n] /= d;
        }
        for c in k + 1..n {
            let f = a[k + c * n];
            for i in k + 1..n {
                a[i + c * n] -= a[i + k * n] * f;
            }
        }
    }
    // inv(U), in place in the upper triangle (dtrti2).
    for j in 0..n {
        a[j + j * n] = 1.0 / a[j + j * n];
        let ajj = -a[j + j * n];
        // x = T(0..j, 0..j) * a(0..j, j) with the already inverted leading block (dtrmv upper, no-trans)
        let col: Vec<f64> = (0..j).map(|i| a[i + j * n]).collect();
        let mut x = vec![0.0; j];
        for i in 0..j {
            let mut s = 0.0;
            for t in i..j {
                s += a[i + t * n] * col[t];
            }
            x[i] = s;
        }
        for i in 0..j {
            a[i + j * n] = x[i] * ajj;
        }
    }
    // Solve inv(A) L = inv(U), column by column from the right (dgetri).
    let mut work = vec![0.0; n];
    for j in (0..n).rev() {
        for i in j + 1..n {
            work[i] = a[i + j * n];
            a[i + j * n] = 0.0;
        }
        if j + 1 < n {
            for r in 0..n {
                let mut s = 0.0;
                for t in j + 1..n {
                    s += a[r + t * n] * work[t];
                }
                a[r + j * n] -= s;
            }
        }
    }
    for j in (0..n).rev() {
        let p = piv[j];
        if p != j {
            for r in 0..n {
                a.swap(r + j * n, r + p * n);
            }
        }
    }
    a
}

/// I / A, computed as (A' \ I)' (the way a right division is solved), which keeps the zero
/// pattern of a block lower-triangular A.
fn right_inverse(n: usize, a: &[f64]) -> Vec<f64> {
    let t: Vec<f64> = (0..n * n).map(|k| a[(k / n) * n + k % n]).collect();
    let y = solve(n, &t, &identity(n), n);
    (0..n * n).map(|k| y[(k / n) * n + k % n]).collect()
}
