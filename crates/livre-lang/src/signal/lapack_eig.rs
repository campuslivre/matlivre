//! Eigenvalues of a real general matrix in the order LAPACK's `dgeev` returns them (the order
//! MATLAB's `eig` and `roots` use): balancing (`dgebal`), Hessenberg reduction (`dgehd2`) and the
//! double-shift QR iteration (`dlahqr`), written from the published algorithm descriptions.
//!
//! The eigenvalue order is the order of the diagonal blocks of the final quasi-triangular matrix,
//! so it depends on the details of the deflation and shift strategy, which is why the general
//! eigenvalue routine of the matrix library cannot be used where the order must match MATLAB.

const RADIX: f64 = 2.0;

struct Mat {
    n: usize,
    a: Vec<f64>,
}

impl Mat {
    fn at(&self, i: usize, j: usize) -> f64 {
        self.a[i + j * self.n]
    }
    fn set(&mut self, i: usize, j: usize, v: f64) {
        self.a[i + j * self.n] = v;
    }
}

fn sign(a: f64, b: f64) -> f64 {
    if b.is_sign_negative() { -a.abs() } else { a.abs() }
}

fn hypot(a: f64, b: f64) -> f64 {
    a.hypot(b)
}

/// Balances the matrix; returns (ilo, ihi) as 0-based inclusive bounds.
fn balance(m: &mut Mat) -> (usize, usize) {
    let n = m.n;
    if n == 0 {
        return (0, 0);
    }
    let (mut k, mut l) = (0usize, n - 1);
    // Rows that isolate an eigenvalue are pushed to the bottom.
    'rows: loop {
        let mut found = None;
        for j in (0..=l).rev() {
            if (0..=l).all(|i| i == j || m.at(j, i) == 0.0) {
                found = Some(j);
                break;
            }
        }
        match found {
            Some(j) => {
                exchange(m, j, l, k, l);
                if l == 0 {
                    return (0, 0);
                }
                l -= 1;
                continue 'rows;
            }
            None => break,
        }
    }
    // Columns that isolate an eigenvalue are pushed to the left.
    'cols: loop {
        let mut found = None;
        for j in k..=l {
            if (k..=l).all(|i| i == j || m.at(i, j) == 0.0) {
                found = Some(j);
                break;
            }
        }
        match found {
            Some(j) => {
                exchange(m, j, k, k, l);
                k += 1;
                continue 'cols;
            }
            None => break,
        }
    }
    // Scale rows and columns of the remaining block by powers of the radix.
    let sfmin1 = f64::MIN_POSITIVE / (f64::EPSILON * 0.5 * 2.0);
    let sfmax1 = 1.0 / sfmin1;
    let sfmin2 = sfmin1 * RADIX;
    let sfmax2 = 1.0 / sfmin2;
    let mut scale = vec![1.0; n];
    loop {
        let mut noconv = false;
        for i in k..=l {
            let mut c = (k..=l).map(|r| m.at(r, i).powi(2)).sum::<f64>().sqrt();
            let mut r = (k..=l).map(|cc| m.at(i, cc).powi(2)).sum::<f64>().sqrt();
            let mut ca = (0..=l).map(|rr| m.at(rr, i).abs()).fold(0.0, f64::max);
            let mut ra = (k..n).map(|cc| m.at(i, cc).abs()).fold(0.0, f64::max);
            if c == 0.0 || r == 0.0 {
                continue;
            }
            let mut g = r / RADIX;
            let mut f: f64 = 1.0;
            let s = c + r;
            while c < g && f.max(c).max(ca) < sfmax2 && r.min(g).min(ra) > sfmin2 {
                f *= RADIX;
                c *= RADIX;
                ca *= RADIX;
                r /= RADIX;
                g /= RADIX;
                ra /= RADIX;
            }
            g = c / RADIX;
            while g >= r && r.max(ra) < sfmax2 && f.min(c).min(g).min(ca) > sfmin2 {
                f /= RADIX;
                c /= RADIX;
                g /= RADIX;
                ca /= RADIX;
                r *= RADIX;
                ra *= RADIX;
            }
            if c + r >= 0.95 * s {
                continue;
            }
            if f < 1.0 && scale[i] < 1.0 && f * scale[i] <= sfmin1 {
                continue;
            }
            if f > 1.0 && scale[i] > 1.0 && scale[i] >= sfmax1 / f {
                continue;
            }
            let gi = 1.0 / f;
            scale[i] *= f;
            noconv = true;
            for cc in k..n {
                let v = m.at(i, cc) * gi;
                m.set(i, cc, v);
            }
            for rr in 0..=l {
                let v = m.at(rr, i) * f;
                m.set(rr, i, v);
            }
        }
        if !noconv {
            break;
        }
    }
    (k, l)
}

/// Swaps index `j` with `m`: columns over rows 0..=l and rows over columns k..n.
fn exchange(mat: &mut Mat, j: usize, m: usize, k: usize, l: usize) {
    if j == m {
        return;
    }
    let n = mat.n;
    for r in 0..=l {
        let t = mat.at(r, j);
        mat.set(r, j, mat.at(r, m));
        mat.set(r, m, t);
    }
    for c in k..n {
        let t = mat.at(j, c);
        mat.set(j, c, mat.at(m, c));
        mat.set(m, c, t);
    }
}

/// Householder vector for (alpha, x): returns (beta, tau) and overwrites x with v(2:).
fn larfg(alpha: f64, x: &mut [f64]) -> (f64, f64) {
    let xnorm = x.iter().map(|v| v * v).sum::<f64>().sqrt();
    if x.is_empty() || xnorm == 0.0 {
        return (alpha, 0.0);
    }
    let beta = -sign(hypot(alpha, xnorm), alpha);
    let tau = (beta - alpha) / beta;
    let scal = 1.0 / (alpha - beta);
    for v in x.iter_mut() {
        *v *= scal;
    }
    (beta, tau)
}

/// Reduces rows/columns ilo..=ihi to upper Hessenberg form.
fn hessenberg(m: &mut Mat, ilo: usize, ihi: usize) {
    let n = m.n;
    if ihi < ilo + 2 {
        return;
    }
    for i in ilo..ihi - 1 {
        let alpha = m.at(i + 1, i);
        let mut x: Vec<f64> = (i + 2..=ihi).map(|r| m.at(r, i)).collect();
        let (beta, tau) = larfg(alpha, &mut x);
        let mut v = vec![1.0];
        v.extend(&x);
        m.set(i + 1, i, 1.0);
        for (t, r) in (i + 2..=ihi).enumerate() {
            m.set(r, i, x[t]);
        }
        // H applied from the right to A(0..=ihi, i+1..=ihi).
        for r in 0..=ihi {
            let s: f64 = (0..v.len()).map(|t| m.at(r, i + 1 + t) * v[t]).sum();
            for t in 0..v.len() {
                let val = m.at(r, i + 1 + t) - tau * s * v[t];
                m.set(r, i + 1 + t, val);
            }
        }
        // H applied from the left to A(i+1..=ihi, i+1..n).
        for c in i + 1..n {
            let s: f64 = (0..v.len()).map(|t| m.at(i + 1 + t, c) * v[t]).sum();
            for t in 0..v.len() {
                let val = m.at(i + 1 + t, c) - tau * s * v[t];
                m.set(i + 1 + t, c, val);
            }
        }
        m.set(i + 1, i, beta);
        for r in i + 2..=ihi {
            m.set(r, i, 0.0);
        }
    }
}

/// Standardises a real 2x2 block; returns the new (a, b, c, d) and the eigenvalues
/// (rt1r, rt1i, rt2r, rt2i).
fn lanv2(mut a: f64, mut b: f64, mut c: f64, mut d: f64) -> (f64, f64, f64, f64, f64, f64, f64, f64) {
    let eps = f64::EPSILON * 0.5 * 2.0;
    let multpl = 4.0;
    if c == 0.0 {
    } else if b == 0.0 {
        std::mem::swap(&mut a, &mut d);
        b = -c;
        c = 0.0;
    } else if a - d == 0.0 && sign(1.0, b) != sign(1.0, c) {
    } else {
        let temp = a - d;
        let mut p = 0.5 * temp;
        let bcmax = b.abs().max(c.abs());
        let bcmis = b.abs().min(c.abs()) * sign(1.0, b) * sign(1.0, c);
        let scale = p.abs().max(bcmax);
        let mut z = p / scale * p + bcmax / scale * bcmis;
        if z >= multpl * eps {
            z = p + sign(scale.sqrt() * z.sqrt(), p);
            a = d + z;
            d -= bcmax / z * bcmis;
            b -= c;
            c = 0.0;
        } else {
            let sigma = b + c;
            let tau = hypot(sigma, temp);
            let cs = (0.5 * (1.0 + sigma.abs() / tau)).sqrt();
            let sn = -(p / (tau * cs)) * sign(1.0, sigma);
            let aa = a * cs + b * sn;
            let bb = -a * sn + b * cs;
            let cc = c * cs + d * sn;
            let dd = -c * sn + d * cs;
            a = aa * cs + cc * sn;
            b = bb * cs + dd * sn;
            c = -aa * sn + cc * cs;
            d = -bb * sn + dd * cs;
            let temp = 0.5 * (a + d);
            a = temp;
            d = temp;
            if c != 0.0 {
                if b != 0.0 {
                    if sign(1.0, b) == sign(1.0, c) {
                        let sab = b.abs().sqrt();
                        let sac = c.abs().sqrt();
                        p = sign(sab * sac, c);
                        a = temp + p;
                        d = temp - p;
                        b -= c;
                        c = 0.0;
                    }
                } else {
                    b = -c;
                    c = 0.0;
                }
            }
        }
    }
    let (rt1r, rt2r) = (a, d);
    let (rt1i, rt2i) = if c == 0.0 { (0.0, 0.0) } else { let w = b.abs().sqrt() * c.abs().sqrt(); (w, -w) };
    (a, b, c, d, rt1r, rt1i, rt2r, rt2i)
}

/// Double-shift QR on rows ilo..=ihi; fills wr/wi. Returns false when it fails to converge.
fn lahqr(m: &mut Mat, ilo: usize, ihi: usize, wr: &mut [f64], wi: &mut [f64]) -> bool {
    if ilo == ihi {
        wr[ilo] = m.at(ilo, ilo);
        wi[ilo] = 0.0;
        return true;
    }
    for j in ilo..ihi.saturating_sub(2) {
        m.set(j + 2, j, 0.0);
        if j + 3 <= ihi {
            m.set(j + 3, j, 0.0);
        }
    }
    if ihi >= ilo + 2 {
        m.set(ihi, ihi - 2, 0.0);
    }
    let nh = (ihi - ilo + 1) as f64;
    let safmin = f64::MIN_POSITIVE;
    let ulp = f64::EPSILON; // dlamch('P') = eps * base
    let smlnum = safmin * (nh / ulp);
    let itmax = 30 * 10usize.max(ihi - ilo + 1);
    let mut kdefl = 0usize;
    let mut i = ihi as isize;
    while i >= ilo as isize {
        let iu = i as usize;
        let mut l = ilo;
        let mut converged = false;
        for _its in 0..=itmax {
            // Look for a single small subdiagonal element.
            let mut k = iu;
            while k > l {
                if m.at(k, k - 1).abs() <= smlnum {
                    break;
                }
                let mut tst = m.at(k - 1, k - 1).abs() + m.at(k, k).abs();
                if tst == 0.0 {
                    if k >= ilo + 2 {
                        tst += m.at(k - 1, k - 2).abs();
                    }
                    if k < ihi {
                        tst += m.at(k + 1, k).abs();
                    }
                }
                if m.at(k, k - 1).abs() <= ulp * tst {
                    let ab = m.at(k, k - 1).abs().max(m.at(k - 1, k).abs());
                    let ba = m.at(k, k - 1).abs().min(m.at(k - 1, k).abs());
                    let aa = m.at(k, k).abs().max((m.at(k - 1, k - 1) - m.at(k, k)).abs());
                    let bb = m.at(k, k).abs().min((m.at(k - 1, k - 1) - m.at(k, k)).abs());
                    let s = aa + ab;
                    if ba * (ab / s) <= smlnum.max(ulp * (bb * (aa / s))) {
                        break;
                    }
                }
                k -= 1;
            }
            l = k;
            if l > ilo {
                m.set(l, l - 1, 0.0);
            }
            if l + 1 >= iu {
                converged = true;
                break;
            }
            kdefl += 1;
            let (i1, i2) = (l, iu);
            let (h11, h21, h12, h22);
            if kdefl % 20 == 0 {
                let s = m.at(iu, iu - 1).abs() + m.at(iu - 1, iu - 2).abs();
                h11 = 0.75 * s + m.at(iu, iu);
                h12 = -0.4375 * s;
                h21 = s;
                h22 = h11;
            } else if kdefl % 10 == 0 {
                let s = m.at(l + 1, l).abs() + m.at(l + 2, l + 1).abs();
                h11 = 0.75 * s + m.at(l, l);
                h12 = -0.4375 * s;
                h21 = s;
                h22 = h11;
            } else {
                h11 = m.at(iu - 1, iu - 1);
                h21 = m.at(iu, iu - 1);
                h12 = m.at(iu - 1, iu);
                h22 = m.at(iu, iu);
            }
            let s = h11.abs() + h12.abs() + h21.abs() + h22.abs();
            let (rt1r, rt1i, rt2r, rt2i);
            if s == 0.0 {
                rt1r = 0.0;
                rt1i = 0.0;
                rt2r = 0.0;
                rt2i = 0.0;
            } else {
                let (h11, h21, h12, h22) = (h11 / s, h21 / s, h12 / s, h22 / s);
                let tr = (h11 + h22) / 2.0;
                let det = (h11 - tr) * (h22 - tr) - h12 * h21;
                let rtdisc = det.abs().sqrt();
                if det >= 0.0 {
                    rt1r = tr * s;
                    rt2r = rt1r;
                    rt1i = rtdisc * s;
                    rt2i = -rt1i;
                } else {
                    let a1 = tr + rtdisc;
                    let a2 = tr - rtdisc;
                    let v = if (a1 - h22).abs() <= (a2 - h22).abs() { a1 * s } else { a2 * s };
                    rt1r = v;
                    rt2r = v;
                    rt1i = 0.0;
                    rt2i = 0.0;
                }
            }
            // Look for two consecutive small subdiagonal elements.
            let mut mm = iu - 2;
            let mut v = [0.0f64; 3];
            loop {
                let h21s0 = m.at(mm + 1, mm);
                let mut s = (m.at(mm, mm) - rt2r).abs() + rt2i.abs() + h21s0.abs();
                let h21s = h21s0 / s;
                v[0] = h21s * m.at(mm, mm + 1) + (m.at(mm, mm) - rt1r) * ((m.at(mm, mm) - rt2r) / s) - rt1i * (rt2i / s);
                v[1] = h21s * (m.at(mm, mm) + m.at(mm + 1, mm + 1) - rt1r - rt2r);
                v[2] = h21s * m.at(mm + 2, mm + 1);
                s = v[0].abs() + v[1].abs() + v[2].abs();
                for t in v.iter_mut() {
                    *t /= s;
                }
                if mm == l {
                    break;
                }
                let h00 = m.at(mm - 1, mm - 1);
                let h10 = m.at(mm, mm - 1);
                let h11 = m.at(mm, mm);
                let h22 = m.at(mm + 1, mm + 1);
                if h10.abs() * (v[1].abs() + v[2].abs()) <= ulp * v[0].abs() * (h00.abs() + h11.abs() + h22.abs()) {
                    break;
                }
                mm -= 1;
            }
            // Double-shift QR step.
            for k in mm..iu {
                let nr = 3.min(iu - k + 1);
                if k > mm {
                    for t in 0..nr {
                        v[t] = m.at(k + t, k - 1);
                    }
                }
                let mut tail: Vec<f64> = v[1..nr].to_vec();
                let (beta, t1) = larfg(v[0], &mut tail);
                v[0] = beta;
                for t in 1..nr {
                    v[t] = tail[t - 1];
                }
                if k > mm {
                    m.set(k, k - 1, v[0]);
                    m.set(k + 1, k - 1, 0.0);
                    if k + 1 < iu {
                        m.set(k + 2, k - 1, 0.0);
                    }
                } else if mm > l {
                    let val = m.at(k, k - 1) * (1.0 - t1);
                    m.set(k, k - 1, val);
                }
                let v2 = v[1];
                let t2 = t1 * v2;
                if nr == 3 {
                    let v3 = v[2];
                    let t3 = t1 * v3;
                    for j in k..=i2 {
                        let sum = m.at(k, j) + v2 * m.at(k + 1, j) + v3 * m.at(k + 2, j);
                        m.set(k, j, m.at(k, j) - sum * t1);
                        m.set(k + 1, j, m.at(k + 1, j) - sum * t2);
                        m.set(k + 2, j, m.at(k + 2, j) - sum * t3);
                    }
                    for j in i1..=(k + 3).min(iu) {
                        let sum = m.at(j, k) + v2 * m.at(j, k + 1) + v3 * m.at(j, k + 2);
                        m.set(j, k, m.at(j, k) - sum * t1);
                        m.set(j, k + 1, m.at(j, k + 1) - sum * t2);
                        m.set(j, k + 2, m.at(j, k + 2) - sum * t3);
                    }
                } else if nr == 2 {
                    for j in k..=i2 {
                        let sum = m.at(k, j) + v2 * m.at(k + 1, j);
                        m.set(k, j, m.at(k, j) - sum * t1);
                        m.set(k + 1, j, m.at(k + 1, j) - sum * t2);
                    }
                    for j in i1..=iu {
                        let sum = m.at(j, k) + v2 * m.at(j, k + 1);
                        m.set(j, k, m.at(j, k) - sum * t1);
                        m.set(j, k + 1, m.at(j, k + 1) - sum * t2);
                    }
                }
            }
        }
        if !converged {
            return false;
        }
        if l == iu {
            wr[iu] = m.at(iu, iu);
            wi[iu] = 0.0;
        } else {
            let (a, b, c, d, r1r, r1i, r2r, r2i) = lanv2(m.at(iu - 1, iu - 1), m.at(iu - 1, iu), m.at(iu, iu - 1), m.at(iu, iu));
            m.set(iu - 1, iu - 1, a);
            m.set(iu - 1, iu, b);
            m.set(iu, iu - 1, c);
            m.set(iu, iu, d);
            wr[iu - 1] = r1r;
            wi[iu - 1] = r1i;
            wr[iu] = r2r;
            wi[iu] = r2i;
        }
        kdefl = 0;
        i = l as isize - 1;
    }
    true
}

/// Eigenvalues (re, im) of the n x n column-major real matrix, in LAPACK order; `None` when the
/// iteration fails to converge.
pub fn eigenvalues(n: usize, a: &[f64]) -> Option<Vec<(f64, f64)>> {
    let mut m = Mat { n, a: a.to_vec() };
    if n == 0 {
        return Some(Vec::new());
    }
    let (ilo, ihi) = balance(&mut m);
    hessenberg(&mut m, ilo, ihi);
    let mut wr = vec![0.0; n];
    let mut wi = vec![0.0; n];
    for i in 0..n {
        if i < ilo || i > ihi {
            wr[i] = m.at(i, i);
        }
    }
    // Entries below the subdiagonal hold reflector data; they are zero for the iteration.
    for j in 0..n {
        for i in j + 2..n {
            m.set(i, j, 0.0);
        }
    }
    if !lahqr(&mut m, ilo, ihi, &mut wr, &mut wi) {
        return None;
    }
    Some(wr.into_iter().zip(wi).collect())
}

