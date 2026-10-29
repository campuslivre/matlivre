//! MATLAB's linear algebra (`toolbox/matlab/matfun`): factorizations (`lu`, `qr`, `chol`,
//! `ldl`, `svd`, `hess`, `schur`, `qz`), their updates and reorderings, matrix functions
//! (`expm`, `logm`, `sqrtm`, `funm`), subspaces, condition estimates and structure tests.
//!
//! Everything runs on a small dense complex matrix type; real inputs stay real because the
//! arithmetic on zero imaginary parts is exact, and outputs drop the imaginary part when the
//! input was real. Householder reflectors and pivoting follow the LAPACK conventions MATLAB
//! uses, so `lu`, `qr`, `chol`, `ldl` and `hess` agree with it to rounding; the SVD and the
//! eigensolvers come from `faer`, whose singular and eigenvector signs are not MATLAB's.

use faer::c64 as Z;

use crate::builtins::{default_dim, m_arg, one, Ret};
use crate::cplx::C;
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::value::*;

type R<T> = Result<T, LangError>;

const EPS: f64 = f64::EPSILON;
const ZERO: Z = Z::new(0.0, 0.0);
const ONE: Z = Z::new(1.0, 0.0);
const SVD_2D: &str = "Input must be 2-D. Use PAGESVD to compute the SVD of each page of an N-D array.";

pub const NAMES: &[&str] = &[
    "bandwidth", "isbanded", "ishermitian", "issymmetric", "vecnorm", "normest", "normest1", "rank", "null", "orth", "rref", "subspace", "linsolve",
    "rcond", "cond", "condest", "chol", "ldl", "lu", "qr", "pinv", "lsqminnorm", "lscov", "sylvester", "svd", "svds", "eigs", "gsvd", "pageeig",
    "pagesvd", "poly", "polyeig", "condeig", "hess", "schur", "qz", "ordschur", "ordqz", "ordeig", "expm", "logm", "sqrtm", "funm", "qrdelete",
    "qrinsert", "rsf2csf", "cdf2rdf", "balance", "planerot", "cholupdate", "qrupdate",
];

// ---- complex scalars ---------------------------------------------------------------------

fn re(x: f64) -> Z {
    Z::new(x, 0.0)
}

/// Division without the rounding of `a * conj(b) / |b|^2` (exact for real divisors).
fn dv(a: Z, b: Z) -> Z {
    let r = C::new(a.re, a.im).div(C::new(b.re, b.im));
    Z::new(r.re, r.im)
}

fn abs(a: Z) -> f64 {
    a.re.hypot(a.im)
}

/// LAPACK's cheap magnitude |re| + |im|, used for pivot choices.
fn cabs1(a: Z) -> f64 {
    a.re.abs() + a.im.abs()
}

fn csqrt(a: Z) -> Z {
    let r = C::new(a.re, a.im).sqrt();
    Z::new(r.re, r.im)
}

fn cexp(a: Z) -> Z {
    let r = C::new(a.re, a.im).exp();
    Z::new(r.re, r.im)
}

fn cln(a: Z) -> Z {
    let r = C::new(a.re, a.im).ln();
    Z::new(r.re, r.im)
}

// ---- dense complex matrices --------------------------------------------------------------

/// A column-major dense complex matrix.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct M {
    r: usize,
    c: usize,
    d: Vec<Z>,
}

impl M {
    fn zeros(r: usize, c: usize) -> M {
        M { r, c, d: vec![ZERO; r * c] }
    }

    fn eye(n: usize) -> M {
        M::eye_rc(n, n)
    }

    fn eye_rc(r: usize, c: usize) -> M {
        let mut m = M::zeros(r, c);
        for i in 0..r.min(c) {
            m.set(i, i, ONE);
        }
        m
    }

    fn from_fn(r: usize, c: usize, f: impl Fn(usize, usize) -> Z) -> M {
        let mut d = Vec::with_capacity(r * c);
        for j in 0..c {
            for i in 0..r {
                d.push(f(i, j));
            }
        }
        M { r, c, d }
    }

    fn col_vec(v: &[Z]) -> M {
        M { r: v.len(), c: 1, d: v.to_vec() }
    }

    fn diag(v: &[Z]) -> M {
        let mut m = M::zeros(v.len(), v.len());
        for (i, &x) in v.iter().enumerate() {
            m.set(i, i, x);
        }
        m
    }

    fn of(m: &Matrix) -> M {
        let (r, c) = (m.rows(), m.cols());
        M::from_fn(r, c, |i, j| {
            let k = j * r + i;
            Z::new(m.data[k], m.im_at(k))
        })
    }

    #[inline]
    fn at(&self, i: usize, j: usize) -> Z {
        self.d[j * self.r + i]
    }

    #[inline]
    fn set(&mut self, i: usize, j: usize, v: Z) {
        self.d[j * self.r + i] = v;
    }

    #[inline]
    fn rf(&mut self, i: usize, j: usize) -> &mut Z {
        &mut self.d[j * self.r + i]
    }

    fn is_square(&self) -> bool {
        self.r == self.c
    }

    fn is_real(&self) -> bool {
        self.d.iter().all(|z| z.im == 0.0)
    }

    fn mul(&self, o: &M) -> M {
        let mut out = M::zeros(self.r, o.c);
        for j in 0..o.c {
            for k in 0..self.c {
                let b = o.at(k, j);
                if b == ZERO {
                    continue;
                }
                for i in 0..self.r {
                    out.d[j * self.r + i] += self.d[k * self.r + i] * b;
                }
            }
        }
        out
    }

    /// Conjugate transpose.
    fn ct(&self) -> M {
        M::from_fn(self.c, self.r, |i, j| self.at(j, i).conj())
    }

    fn tr(&self) -> M {
        M::from_fn(self.c, self.r, |i, j| self.at(j, i))
    }

    fn add(&self, o: &M) -> M {
        M { r: self.r, c: self.c, d: self.d.iter().zip(&o.d).map(|(a, b)| a + b).collect() }
    }

    fn sub(&self, o: &M) -> M {
        M { r: self.r, c: self.c, d: self.d.iter().zip(&o.d).map(|(a, b)| a - b).collect() }
    }

    fn scale(&self, s: Z) -> M {
        M { r: self.r, c: self.c, d: self.d.iter().map(|a| a * s).collect() }
    }

    /// Rows `r0..r1`, columns `c0..c1`.
    fn block(&self, r0: usize, r1: usize, c0: usize, c1: usize) -> M {
        M::from_fn(r1 - r0, c1 - c0, |i, j| self.at(r0 + i, c0 + j))
    }

    fn put(&mut self, r0: usize, c0: usize, b: &M) {
        for j in 0..b.c {
            for i in 0..b.r {
                self.set(r0 + i, c0 + j, b.at(i, j));
            }
        }
    }

    fn cols(&self, idx: &[usize]) -> M {
        M::from_fn(self.r, idx.len(), |i, j| self.at(i, idx[j]))
    }

    fn rows(&self, idx: &[usize]) -> M {
        M::from_fn(idx.len(), self.c, |i, j| self.at(idx[i], j))
    }

    fn col(&self, j: usize) -> Vec<Z> {
        self.d[j * self.r..(j + 1) * self.r].to_vec()
    }

    fn triu(&self, k: isize) -> M {
        M::from_fn(self.r, self.c, |i, j| if j as isize - i as isize >= k { self.at(i, j) } else { ZERO })
    }

    fn tril(&self, k: isize) -> M {
        M::from_fn(self.r, self.c, |i, j| if j as isize - i as isize <= k { self.at(i, j) } else { ZERO })
    }

    fn norm1(&self) -> f64 {
        (0..self.c).map(|j| (0..self.r).map(|i| abs(self.at(i, j))).sum::<f64>()).fold(0.0, nanmax)
    }

    fn norm_inf(&self) -> f64 {
        (0..self.r).map(|i| (0..self.c).map(|j| abs(self.at(i, j))).sum::<f64>()).fold(0.0, nanmax)
    }

    fn norm_fro(&self) -> f64 {
        vnorm(&self.d)
    }

    fn has_nonfinite(&self) -> bool {
        self.d.iter().any(|z| !z.re.is_finite() || !z.im.is_finite())
    }

    fn faer(&self) -> faer::Mat<Z> {
        faer::Mat::from_fn(self.r, self.c, |i, j| self.at(i, j))
    }

    fn faer_re(&self) -> faer::Mat<f64> {
        faer::Mat::from_fn(self.r, self.c, |i, j| self.at(i, j).re)
    }

    fn of_faer(m: faer::MatRef<'_, Z>) -> M {
        M::from_fn(m.nrows(), m.ncols(), |i, j| m[(i, j)])
    }

    fn of_faer_re(m: faer::MatRef<'_, f64>) -> M {
        M::from_fn(m.nrows(), m.ncols(), |i, j| re(m[(i, j)]))
    }

    /// The MATLAB value: complex when `cplx`, otherwise the real parts.
    fn val(&self, cplx: bool) -> Value {
        let dims = vec![self.r, self.c];
        let rp: Vec<f64> = self.d.iter().map(|z| z.re).collect();
        if cplx {
            Value::Num(Matrix::complex(dims, rp, self.d.iter().map(|z| z.im).collect()))
        } else {
            Value::Num(Matrix::from_dims(dims, rp))
        }
    }

    /// Complex output when any imaginary part is nonzero.
    fn auto(&self) -> Value {
        self.val(!self.is_real())
    }
}

fn nanmax(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

fn vnorm(v: &[Z]) -> f64 {
    // Scaled sum of squares, as dnrm2, so huge and tiny entries do not overflow.
    let mut scale = 0.0f64;
    let mut ssq = 1.0f64;
    for z in v {
        for x in [z.re, z.im] {
            if x != 0.0 {
                let a = x.abs();
                if a.is_nan() {
                    return f64::NAN;
                }
                if scale < a {
                    ssq = 1.0 + ssq * (scale / a) * (scale / a);
                    scale = a;
                } else {
                    ssq += (a / scale) * (a / scale);
                }
            }
        }
    }
    scale * ssq.sqrt()
}

fn row_vec(v: Vec<f64>) -> Value {
    Value::Num(Matrix::row(v))
}

fn col_vec(v: Vec<f64>) -> Value {
    Value::Num(Matrix::col(v))
}

fn zcol(v: &[Z], cplx: bool) -> Value {
    M::col_vec(v).val(cplx)
}

/// A permutation vector (0-based) as MATLAB's 1-based row vector.
fn perm_row(p: &[usize]) -> Value {
    row_vec(p.iter().map(|&i| (i + 1) as f64).collect())
}

fn perm_col(p: &[usize]) -> Value {
    col_vec(p.iter().map(|&i| (i + 1) as f64).collect())
}

/// The permutation matrix P with P(i, p(i)) = 1 (so P*A = A(p,:)).
fn perm_rows_matrix(p: &[usize]) -> M {
    let mut m = M::zeros(p.len(), p.len());
    for (i, &k) in p.iter().enumerate() {
        m.set(i, k, ONE);
    }
    m
}

/// The permutation matrix E with E(p(j), j) = 1 (so A*E = A(:,p)).
fn perm_cols_matrix(p: &[usize]) -> M {
    let mut m = M::zeros(p.len(), p.len());
    for (j, &k) in p.iter().enumerate() {
        m.set(k, j, ONE);
    }
    m
}

// ---- argument helpers --------------------------------------------------------------------

/// A 2-D numeric argument as (matrix, is complex).
fn arg2d(a: &[Value], i: usize, name: &str, msg: &str) -> R<(M, bool)> {
    let m = m_arg(a, i, name)?;
    if !m.is_2d() {
        return rt(msg);
    }
    let cplx = m.is_complex();
    Ok((M::of(&m), cplx))
}

fn square(a: &[Value], i: usize, name: &str, msg: &str) -> R<(M, bool)> {
    let (m, c) = arg2d(a, i, name, msg)?;
    if !m.is_square() {
        return rt(msg);
    }
    Ok((m, c))
}

fn opt_text(v: Option<&Value>) -> Option<String> {
    match v {
        Some(v) if v.is_text() && !matches!(v, Value::Num(m) if m.kind != Kind::Char) => v.text().ok().map(|s| s.to_lowercase()),
        _ => None,
    }
}

fn texts(a: &[Value]) -> Vec<String> {
    a.iter().filter_map(|v| opt_text(Some(v))).collect()
}

fn is_num_zero(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Num(m)) if m.kind != Kind::Char && m.numel() == 1 && m.data[0] == 0.0)
}

fn scalar_at(a: &[Value], i: usize) -> Option<f64> {
    match a.get(i) {
        Some(Value::Num(m)) if m.kind != Kind::Char && m.numel() == 1 => Some(m.data[0]),
        _ => None,
    }
}

fn warn(it: &mut Interpreter, msg: impl Into<String>) {
    it.emit(OutputKind::Warning(msg.into()));
}

fn vals(v: Vec<Value>) -> Ret {
    Ok(Some(v))
}

/// `eps(x)` for a nonnegative double.
fn eps_of(x: f64) -> f64 {
    if x == 0.0 {
        f64::from_bits(1)
    } else if !x.is_finite() {
        f64::NAN
    } else {
        2f64.powf(x.abs().log2().floor() - 52.0)
    }
}

// ---- Householder reflectors and rotations ---------------------------------------------------

/// LAPACK `zlarfg`: for x = [alpha; rest], returns (beta, tau, v) with v(1) = 1 and
/// (I - conj(tau) v v') x = [beta; 0]; tau = 0 means the identity.
fn house(x: &[Z]) -> (Z, Z, Vec<Z>) {
    let alpha = x[0];
    let xnorm = vnorm(&x[1..]);
    let mut v = vec![ONE; x.len()];
    if xnorm == 0.0 && alpha.im == 0.0 {
        v[1..].fill(ZERO);
        return (alpha, ZERO, v);
    }
    let norm = alpha.re.hypot(alpha.im).hypot(xnorm);
    let beta = if alpha.re >= 0.0 { -norm } else { norm };
    let tau = Z::new((beta - alpha.re) / beta, -alpha.im / beta);
    let s = dv(ONE, alpha - beta);
    for k in 1..x.len() {
        v[k] = x[k] * s;
    }
    (re(beta), tau, v)
}

/// A := (I - t v v') A on rows r0.., columns c0..c1.
fn apply_left(a: &mut M, v: &[Z], t: Z, r0: usize, c0: usize, c1: usize) {
    if t == ZERO {
        return;
    }
    for j in c0..c1 {
        let mut s = ZERO;
        for (k, vk) in v.iter().enumerate() {
            s += vk.conj() * a.at(r0 + k, j);
        }
        let s = s * t;
        for (k, vk) in v.iter().enumerate() {
            *a.rf(r0 + k, j) -= vk * s;
        }
    }
}

/// A := A (I - t v v') on columns c0.., rows r0..r1.
fn apply_right(a: &mut M, v: &[Z], t: Z, c0: usize, r0: usize, r1: usize) {
    if t == ZERO {
        return;
    }
    for i in r0..r1 {
        let mut s = ZERO;
        for (k, vk) in v.iter().enumerate() {
            s += a.at(i, c0 + k) * vk;
        }
        let s = s * t;
        for (k, vk) in v.iter().enumerate() {
            *a.rf(i, c0 + k) -= s * vk.conj();
        }
    }
}

/// A plane rotation G = [c s; -conj(s) c] (c real) with G*[x; y] = [r; 0].
fn givens(x: Z, y: Z) -> (f64, Z) {
    if y == ZERO {
        return (1.0, ZERO);
    }
    if x == ZERO {
        return (0.0, dv(y.conj(), re(abs(y))));
    }
    let (ax, ay) = (abs(x), abs(y));
    let r = ax.hypot(ay);
    let c = ax / r;
    let s = dv(x, re(ax)) * y.conj() / r;
    (c, s)
}

/// MATLAB's `planerot` rotation G = [x1' x2'; -x2 x1]/norm(x), so G*x = [norm(x); 0].
fn planerot(x1: Z, x2: Z) -> [Z; 4] {
    if x2 == ZERO {
        return [ONE, ZERO, ZERO, ONE];
    }
    let r = vnorm(&[x1, x2]);
    [x1.conj() / r, x2.conj() / r, -x2 / r, x1 / r]
}

/// The rotation `qrdelete`/`qrinsert` use, G*[f; g] = [r; 0]: LAPACK's classic `lartg` for
/// real data (r takes the sign of f only when |f| > |g|), the phase of f for complex data.
fn rot_classic(f: Z, g: Z) -> [Z; 4] {
    if f.im != 0.0 || g.im != 0.0 {
        let (c, s) = givens(f, g);
        return [re(c), s, -s.conj(), re(c)];
    }
    let (f, g) = (f.re, g.re);
    if g == 0.0 {
        return [ONE, ZERO, ZERO, ONE];
    }
    if f == 0.0 {
        return [ZERO, ONE, -ONE, ZERO];
    }
    let r = f.hypot(g);
    let (mut c, mut sn) = (f / r, g / r);
    if f.abs() > g.abs() && c < 0.0 {
        c = -c;
        sn = -sn;
    }
    [re(c), re(sn), re(-sn), re(c)]
}

/// Rows k, k+1 of A (columns c0..c1) := G * rows, for G = [g0 g1; g2 g3].
fn g_rows(a: &mut M, k: usize, g: &[Z; 4], c0: usize, c1: usize) {
    for j in c0..c1 {
        let (x, y) = (a.at(k, j), a.at(k + 1, j));
        a.set(k, j, g[0] * x + g[1] * y);
        a.set(k + 1, j, g[2] * x + g[3] * y);
    }
}

/// Columns k, k+1 of A (rows r0..r1) := cols * G'.
fn g_cols(a: &mut M, k: usize, g: &[Z; 4], r0: usize, r1: usize) {
    for i in r0..r1 {
        let (x, y) = (a.at(i, k), a.at(i, k + 1));
        a.set(i, k, x * g[0].conj() + y * g[1].conj());
        a.set(i, k + 1, x * g[2].conj() + y * g[3].conj());
    }
}

/// Rows i, k of A (columns c0..c1) := G * rows.
fn rot_rows(a: &mut M, i: usize, k: usize, c: f64, s: Z, c0: usize, c1: usize) {
    for j in c0..c1 {
        let (x, y) = (a.at(i, j), a.at(k, j));
        a.set(i, j, x * c + s * y);
        a.set(k, j, -s.conj() * x + y * c);
    }
}

/// Columns i, k of A (rows r0..r1) := cols * G'.
fn rot_cols(a: &mut M, i: usize, k: usize, c: f64, s: Z, r0: usize, r1: usize) {
    for r in r0..r1 {
        let (x, y) = (a.at(r, i), a.at(r, k));
        a.set(r, i, x * c + y * s.conj());
        a.set(r, k, -s * x + y * c);
    }
}

// ---- LU ----------------------------------------------------------------------------------

/// LAPACK-style LU with partial pivoting: the packed factors (multipliers below the diagonal)
/// and the row order, so A(p,:) = L*U. Zero pivot columns are skipped as in `getrf`.
fn lu_packed(a: &M) -> (M, Vec<usize>) {
    let (m, n) = (a.r, a.c);
    let mut f = a.clone();
    let mut p: Vec<usize> = (0..m).collect();
    for k in 0..m.min(n) {
        let mut piv = k;
        let mut best = cabs1(f.at(k, k));
        for i in k + 1..m {
            let v = cabs1(f.at(i, k));
            if v > best {
                best = v;
                piv = i;
            }
        }
        if f.at(piv, k) == ZERO {
            continue;
        }
        if piv != k {
            for j in 0..n {
                let t = f.at(k, j);
                f.set(k, j, f.at(piv, j));
                f.set(piv, j, t);
            }
            p.swap(k, piv);
        }
        let pivot = f.at(k, k);
        if abs(pivot) >= f64::MIN_POSITIVE {
            let r = dv(ONE, pivot);
            for i in k + 1..m {
                *f.rf(i, k) *= r;
            }
        } else {
            for i in k + 1..m {
                let v = dv(f.at(i, k), pivot);
                f.set(i, k, v);
            }
        }
        for j in k + 1..n {
            let u = f.at(k, j);
            if u == ZERO {
                continue;
            }
            for i in k + 1..m {
                let l = f.at(i, k);
                *f.rf(i, j) -= l * u;
            }
        }
    }
    (f, p)
}

fn lu_split(f: &M) -> (M, M) {
    let k = f.r.min(f.c);
    let l = M::from_fn(f.r, k, |i, j| if i == j { ONE } else if i > j { f.at(i, j) } else { ZERO });
    let u = M::from_fn(k, f.c, |i, j| if j >= i { f.at(i, j) } else { ZERO });
    (l, u)
}

/// Solves A X = B for square A through LU; `None` when A is exactly singular.
fn lu_solve(a: &M, b: &M) -> Option<M> {
    let n = a.r;
    let (f, p) = lu_packed(a);
    if (0..n).any(|i| f.at(i, i) == ZERO) {
        return None;
    }
    let mut x = b.rows(&p);
    for j in 0..x.c {
        for i in 0..n {
            let mut s = x.at(i, j);
            for k in 0..i {
                s -= f.at(i, k) * x.at(k, j);
            }
            x.set(i, j, s);
        }
        for i in (0..n).rev() {
            let mut s = x.at(i, j);
            for k in i + 1..n {
                s -= f.at(i, k) * x.at(k, j);
            }
            x.set(i, j, dv(s, f.at(i, i)));
        }
    }
    Some(x)
}

/// Reciprocal 1-norm condition number from an explicit inverse (0 when singular).
fn rcond_of(a: &M) -> f64 {
    if a.r == 0 {
        return f64::INFINITY;
    }
    if a.has_nonfinite() {
        return f64::NAN;
    }
    let an = a.norm1();
    if an == 0.0 {
        return 0.0;
    }
    let (f, p) = lu_packed(a);
    if (0..a.r).any(|i| f.at(i, i) == ZERO) {
        return 0.0;
    }
    let est = inv_norm1_est(&f, &p, a.is_real());
    if est == 0.0 { 0.0 } else { (1.0 / est) / an }
}

/// 1/(norm(A,1)*norm(inv(A),1)) from an explicit inverse (0 when singular).
fn rcond_exact(a: &M) -> f64 {
    if a.r == 0 {
        return f64::INFINITY;
    }
    let an = a.norm1();
    match lu_solve(a, &M::eye(a.r)) {
        Some(inv) if an > 0.0 && !inv.has_nonfinite() => 1.0 / (an * inv.norm1()),
        _ => 0.0,
    }
}

/// Solves A x = b (or A' x = b) with the packed LU factors of A.
fn lu_apply(f: &M, p: &[usize], b: &[Z], trans: bool) -> Vec<Z> {
    let n = f.r;
    if !trans {
        let mut x: Vec<Z> = p.iter().map(|&i| b[i]).collect();
        for i in 0..n {
            for k in 0..i {
                let t = f.at(i, k) * x[k];
                x[i] -= t;
            }
        }
        for i in (0..n).rev() {
            for k in i + 1..n {
                let t = f.at(i, k) * x[k];
                x[i] -= t;
            }
            x[i] = dv(x[i], f.at(i, i));
        }
        x
    } else {
        let mut y = b.to_vec();
        for i in 0..n {
            for k in 0..i {
                let t = f.at(k, i).conj() * y[k];
                y[i] -= t;
            }
            y[i] = dv(y[i], f.at(i, i).conj());
        }
        for i in (0..n).rev() {
            for k in i + 1..n {
                let t = f.at(k, i).conj() * y[k];
                y[i] -= t;
            }
        }
        let mut x = vec![ZERO; n];
        for (i, &pi) in p.iter().enumerate() {
            x[pi] = y[i];
        }
        x
    }
}

/// Hager-Higham estimate of norm(inv(A), 1) from the LU factors, as LAPACK's `lacn2`
/// (the real variant also stops when a sign vector repeats).
fn inv_norm1_est(f: &M, p: &[usize], real: bool) -> f64 {
    let n = f.r;
    let sum = |v: &[Z]| v.iter().map(|z| abs(*z)).sum::<f64>();
    let sgn = |z: Z| {
        if real {
            re(if z.re >= 0.0 { 1.0 } else { -1.0 })
        } else {
            let a = abs(z);
            if a > f64::MIN_POSITIVE { z / a } else { ONE }
        }
    };
    let argmax = |v: &[Z]| (0..v.len()).fold(0, |b, i| if abs(v[i]) > abs(v[b]) { i } else { b });
    let x = vec![re(1.0 / n as f64); n];
    let y = lu_apply(f, p, &x, false);
    if n == 1 {
        return abs(y[0]);
    }
    let mut est = sum(&y);
    let mut xi: Vec<Z> = y.iter().map(|&z| sgn(z)).collect();
    let mut z = lu_apply(f, p, &xi, true);
    let mut j = argmax(&z);
    let mut iter = 2;
    loop {
        let mut e = vec![ZERO; n];
        e[j] = ONE;
        let y = lu_apply(f, p, &e, false);
        let estold = est;
        est = sum(&y);
        if real && y.iter().zip(&xi).all(|(a, s)| sgn(*a) == *s) {
            break;
        }
        if est <= estold {
            break;
        }
        xi = y.iter().map(|&z| sgn(z)).collect();
        z = lu_apply(f, p, &xi, true);
        let jlast = j;
        j = argmax(&z);
        let moved = if real { z[jlast].re != z[j].re.abs() } else { abs(z[jlast]) != abs(z[j]) };
        if moved && iter < 5 {
            iter += 1;
            continue;
        }
        break;
    }
    let alt: Vec<Z> = (0..n).map(|i| re(if i % 2 == 0 { 1.0 } else { -1.0 } * (1.0 + i as f64 / (n - 1) as f64))).collect();
    let y = lu_apply(f, p, &alt, false);
    let temp = 2.0 * (sum(&y) / (3.0 * n as f64));
    est.max(temp)
}

/// `A\B` for square A with MATLAB's singularity warnings.
fn solve_square(it: &mut Interpreter, a: &M, b: &M) -> M {
    let rc = rcond_of(a);
    match lu_solve(a, b) {
        Some(x) => {
            if rc < EPS {
                warn(it, format!("Matrix is close to singular or badly scaled. Results may be inaccurate. RCOND = {}.", fmt_e(rc)));
            }
            x
        }
        None => {
            warn(it, "Matrix is singular to working precision.");
            singular_solution(a, b)
        }
    }
}

/// What LAPACK's triangular solve produces for an exactly singular LU: divisions by zero.
fn singular_solution(a: &M, b: &M) -> M {
    let n = a.r;
    let (f, p) = lu_packed(a);
    let mut x = b.rows(&p);
    for j in 0..x.c {
        for i in 0..n {
            let mut s = x.at(i, j);
            for k in 0..i {
                s -= f.at(i, k) * x.at(k, j);
            }
            x.set(i, j, s);
        }
        for i in (0..n).rev() {
            let mut s = x.at(i, j);
            for k in i + 1..n {
                s -= f.at(i, k) * x.at(k, j);
            }
            let d = f.at(i, i);
            x.set(i, j, if d == ZERO { Z::new(s.re / 0.0, if s.im == 0.0 { 0.0 } else { s.im / 0.0 }) } else { dv(s, d) });
        }
    }
    x
}

/// `%e` formatting as MATLAB prints it in warnings (two-digit exponent minimum).
fn fmt_e(x: f64) -> String {
    let s = format!("{x:e}");
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let m: f64 = mant.parse().unwrap_or(0.0);
    let e: i32 = exp.parse().unwrap_or(0);
    format!("{m:.6}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
}

// ---- QR ----------------------------------------------------------------------------------

struct Qr {
    /// R in the upper triangle (m×n), reflector tails below it.
    f: M,
    taus: Vec<Z>,
    /// Column order (identity without pivoting).
    perm: Vec<usize>,
}

/// Householder QR as `geqrf` (or `geqp3` with `pivot`): R(k,k) = -sign(re(a))*norm.
fn qr_fact(a: &M, pivot: bool) -> Qr {
    let (m, n) = (a.r, a.c);
    let mut f = a.clone();
    let mut perm: Vec<usize> = (0..n).collect();
    let mut taus = Vec::new();
    for k in 0..m.min(n) {
        if pivot {
            let norms: Vec<f64> = (k..n).map(|j| vnorm(&f.d[j * m + k..(j + 1) * m])).collect();
            let mut best = k;
            for (o, &v) in norms.iter().enumerate() {
                if v > norms[best - k] {
                    best = k + o;
                }
            }
            if best != k {
                for i in 0..m {
                    let t = f.at(i, k);
                    f.set(i, k, f.at(i, best));
                    f.set(i, best, t);
                }
                perm.swap(k, best);
            }
        }
        let x: Vec<Z> = (k..m).map(|i| f.at(i, k)).collect();
        let (beta, tau, v) = house(&x);
        apply_left(&mut f, &v, tau.conj(), k, k + 1, n);
        f.set(k, k, beta);
        for i in k + 1..m {
            f.set(i, k, v[i - k]);
        }
        taus.push(tau);
    }
    Qr { f, taus, perm }
}

impl Qr {
    /// The first `cols` columns of Q.
    fn q(&self, cols: usize) -> M {
        let m = self.f.r;
        let mut q = M::eye_rc(m, cols);
        for k in (0..self.taus.len()).rev() {
            let v: Vec<Z> = (k..m).map(|i| if i == k { ONE } else { self.f.at(i, k) }).collect();
            apply_left(&mut q, &v, self.taus[k], k, 0, cols);
        }
        q
    }

    /// R with `rows` rows.
    fn r(&self, rows: usize) -> M {
        M::from_fn(rows, self.f.c, |i, j| if j >= i && i < self.f.r { self.f.at(i, j) } else { ZERO })
    }

    /// Q' * B.
    fn qt_mul(&self, b: &M) -> M {
        let m = self.f.r;
        let mut x = b.clone();
        for k in 0..self.taus.len() {
            let v: Vec<Z> = (k..m).map(|i| if i == k { ONE } else { self.f.at(i, k) }).collect();
            let nc = x.c;
            apply_left(&mut x, &v, self.taus[k].conj(), k, 0, nc);
        }
        x
    }
}

/// Solves the upper triangular system R(0..k,0..k) X = B(0..k,:).
fn back_sub(r: &M, b: &M, k: usize) -> M {
    let mut x = M::zeros(k, b.c);
    for j in 0..b.c {
        for i in (0..k).rev() {
            let mut s = b.at(i, j);
            for l in i + 1..k {
                s -= r.at(i, l) * x.at(l, j);
            }
            x.set(i, j, dv(s, r.at(i, i)));
        }
    }
    x
}

/// Least squares through column-pivoted QR, the basic solution MATLAB's `\` returns, with
/// its rank-deficiency warning.
fn lstsq(it: Option<&mut Interpreter>, a: &M, b: &M) -> (M, usize) {
    let (m, n) = (a.r, a.c);
    let qr = qr_fact(a, true);
    let k = m.min(n);
    let tol = (m.max(n) as f64) * EPS * if k > 0 { abs(qr.f.at(0, 0)) } else { 0.0 };
    let rank = (0..k).take_while(|&i| abs(qr.f.at(i, i)) > tol).count();
    if rank < k {
        if let Some(it) = it {
            warn(it, format!("Rank deficient, rank = {rank}, tol = {}.", fmt_e(tol)));
        }
    }
    let c = qr.qt_mul(b);
    let y = back_sub(&qr.f, &c, rank);
    let mut x = M::zeros(n, b.c);
    for i in 0..rank {
        for j in 0..b.c {
            x.set(qr.perm[i], j, y.at(i, j));
        }
    }
    (x, rank)
}

// ---- Cholesky ----------------------------------------------------------------------------

/// Upper Cholesky factor from the upper triangle; Err(k) (1-based) at the first nonpositive
/// pivot, with the factor of the leading (k-1)×(k-1) block.
fn chol_upper(a: &M) -> Result<M, (usize, M)> {
    let n = a.r;
    let mut r = M::zeros(n, n);
    for j in 0..n {
        let mut s = a.at(j, j).re;
        for k in 0..j {
            s -= r.at(k, j).norm_sqr();
        }
        if s.is_nan() || s <= 0.0 {
            return Err((j + 1, r.block(0, j, 0, j)));
        }
        let d = s.sqrt();
        r.set(j, j, re(d));
        for i in j + 1..n {
            let mut t = a.at(j, i);
            for k in 0..j {
                t -= r.at(k, j).conj() * r.at(k, i);
            }
            r.set(j, i, t / d);
        }
    }
    Ok(r)
}

// ---- LDL' (Bunch-Kaufman) ----------------------------------------------------------------

/// L*D*L' of a Hermitian matrix with bounded Bunch-Kaufman ("rook") pivoting, as LAPACK's
/// `sytrf_rook`/`hetrf_rook` with lower storage: (L unit lower, D block diagonal, p) with
/// A(p,p) = L*D*L'. The matrix is read from its upper triangle when `from_upper`.
fn ldl_rook(a0: &M, from_upper: bool) -> (M, M, Vec<usize>) {
    let n = a0.r;
    let mut a = M::from_fn(n, n, |i, j| {
        if i == j {
            re(a0.at(i, i).re)
        } else if (i < j) == from_upper {
            a0.at(i, j)
        } else {
            a0.at(j, i).conj()
        }
    });
    let real = a.is_real();
    let alpha = (1.0 + 17f64.sqrt()) / 8.0;
    let mut l = M::eye(n);
    let mut d = M::zeros(n, n);
    let mut p: Vec<usize> = (0..n).collect();
    let swap = |a: &mut M, l: &mut M, p: &mut Vec<usize>, k: usize, i: usize, j: usize| {
        if i == j {
            return;
        }
        for c in 0..n {
            let t = a.at(i, c);
            a.set(i, c, a.at(j, c));
            a.set(j, c, t);
        }
        for r in 0..n {
            let t = a.at(r, i);
            a.set(r, i, a.at(r, j));
            a.set(r, j, t);
        }
        for c in 0..k {
            let t = l.at(i, c);
            l.set(i, c, l.at(j, c));
            l.set(j, c, t);
        }
        p.swap(i, j);
    };
    // Mirrors the updated lower triangle of the trailing block into the upper one.
    let mirror = |a: &mut M, from: usize| {
        for j in from..n {
            for i in j + 1..n {
                let v = a.at(i, j).conj();
                a.set(j, i, v);
            }
        }
    };
    // First index of the largest value, as `idamax`.
    let first_max = |v: &mut dyn Iterator<Item = (usize, f64)>| v.fold((usize::MAX, -1.0f64), |b, (i, x)| if x > b.1 { (i, x) } else { b });
    let mut k = 0;
    while k < n {
        let absakk = a.at(k, k).re.abs();
        let (mut imax, mut colmax) = if k + 1 < n { first_max(&mut (k + 1..n).map(|i| (i, cabs1(a.at(i, k))))) } else { (k, 0.0) };
        let (mut kstep, mut kp, mut pp) = (1, k, k);
        if absakk.max(colmax) != 0.0 && absakk < alpha * colmax {
            loop {
                let (mut jmax, mut rowmax) = if imax != k { first_max(&mut (k..imax).map(|j| (j, cabs1(a.at(imax, j))))) } else { (k, 0.0) };
                if imax + 1 < n {
                    let (it, dt) = first_max(&mut (imax + 1..n).map(|i| (i, cabs1(a.at(i, imax)))));
                    if dt > rowmax {
                        rowmax = dt;
                        jmax = it;
                    }
                }
                if a.at(imax, imax).re.abs() >= alpha * rowmax {
                    kp = imax;
                    break;
                } else if pp == jmax || rowmax <= colmax {
                    kp = imax;
                    kstep = 2;
                    break;
                } else {
                    pp = imax;
                    colmax = rowmax;
                    imax = jmax;
                }
            }
        }
        let kk = k + kstep - 1;
        if kstep == 2 && pp != k {
            swap(&mut a, &mut l, &mut p, k, k, pp);
        }
        swap(&mut a, &mut l, &mut p, k, kk, kp);
        if kstep == 1 {
            let akk = a.at(k, k).re;
            d.set(k, k, re(akk));
            if akk != 0.0 {
                let d11 = 1.0 / akk;
                let x: Vec<Z> = (0..n).map(|i| if i > k { a.at(i, k) } else { ZERO }).collect();
                for j in k + 1..n {
                    if x[j] == ZERO {
                        continue;
                    }
                    let temp = if real { re(-d11 * x[j].re) } else { x[j].conj() * -d11 };
                    let djj = a.at(j, j) + x[j] * temp;
                    a.set(j, j, if real { djj } else { re(djj.re) });
                    for (i, xi) in x.iter().enumerate().skip(j + 1) {
                        *a.rf(i, j) += xi * temp;
                    }
                }
                for (i, xi) in x.iter().enumerate().skip(k + 1) {
                    l.set(i, k, xi * d11);
                }
                mirror(&mut a, k + 1);
            }
        } else {
            let (d11o, d21o, d22o) = (a.at(k, k), a.at(k + 1, k), a.at(k + 1, k + 1));
            d.set(k, k, d11o);
            d.set(k + 1, k, d21o);
            d.set(k, k + 1, d21o.conj());
            d.set(k + 1, k + 1, d22o);
            let xk: Vec<Z> = (0..n).map(|i| a.at(i, k)).collect();
            let xk1: Vec<Z> = (0..n).map(|i| a.at(i, k + 1)).collect();
            if real {
                let d21 = d21o.re;
                let d11 = d22o.re / d21;
                let d22 = d11o.re / d21;
                let t = 1.0 / (d11 * d22 - 1.0);
                for j in k + 2..n {
                    let wk = t * (d11 * xk[j].re - xk1[j].re);
                    let wkp1 = t * (d22 * xk1[j].re - xk[j].re);
                    for i in j..n {
                        let v = a.at(i, j).re - (xk[i].re / d21) * wk - (xk1[i].re / d21) * wkp1;
                        a.set(i, j, re(v));
                    }
                    l.set(j, k, re(wk / d21));
                    l.set(j, k + 1, re(wkp1 / d21));
                }
            } else {
                let dd = abs(d21o);
                let d11 = d22o.re / dd;
                let d22 = d11o.re / dd;
                let d21 = d21o / dd;
                let tt = 1.0 / (d11 * d22 - 1.0);
                for j in k + 2..n {
                    let wk = (xk[j] * d11 - d21 * xk1[j]) * tt;
                    let wkp1 = (xk1[j] * d22 - d21.conj() * xk[j]) * tt;
                    for i in j..n {
                        let v = a.at(i, j) - (xk[i] / dd) * wk.conj() - (xk1[i] / dd) * wkp1.conj();
                        a.set(i, j, v);
                    }
                    let djj = a.at(j, j).re;
                    a.set(j, j, re(djj));
                    l.set(j, k, wk / dd);
                    l.set(j, k + 1, wkp1 / dd);
                }
            }
            mirror(&mut a, k + 2);
        }
        k += kstep;
    }
    (l, d, p)
}

// ---- SVD and eigenvalues through faer ------------------------------------------------------

/// (U, singular values, V); `thin` gives the economy-size factors.
fn svd_of(a: &M, real: bool, thin: bool) -> R<(M, Vec<f64>, M)> {
    if a.has_nonfinite() {
        return rt("SVD did not converge.");
    }
    let k = a.r.min(a.c);
    if a.r == 0 || a.c == 0 {
        let (ur, vr) = if thin { (k, k) } else { (a.r, a.c) };
        return Ok((M::eye_rc(a.r, ur), Vec::new(), M::eye_rc(a.c, vr)));
    }
    let fail = |_| LangError::new("SVD did not converge.", 0);
    if real {
        let f = a.faer_re();
        let s = if thin { f.thin_svd() } else { f.svd() }.map_err(fail)?;
        let sv: Vec<f64> = (0..k).map(|i| s.S().column_vector()[i]).collect();
        Ok((M::of_faer_re(s.U()), sv, M::of_faer_re(s.V())))
    } else {
        let f = a.faer();
        let s = if thin { f.thin_svd() } else { f.svd() }.map_err(fail)?;
        let sv: Vec<f64> = (0..k).map(|i| s.S().column_vector()[i].re).collect();
        Ok((M::of_faer(s.U()), sv, M::of_faer(s.V())))
    }
}

fn singular_values(a: &M) -> R<Vec<f64>> {
    Ok(svd_of(a, a.is_real(), true)?.1)
}

/// Eigenvalues and unit-norm eigenvectors of a general square matrix.
fn eig_general(a: &M) -> R<(Vec<Z>, M)> {
    if a.r == 0 {
        return Ok((Vec::new(), M::zeros(0, 0)));
    }
    if a.has_nonfinite() {
        return rt("Input to EIG must not contain NaN or Inf.");
    }
    let fail = |_| LangError::new("EIG did not converge.", 0);
    let e = if a.is_real() { a.faer_re().eigen().map_err(fail)? } else { a.faer().eigen().map_err(fail)? };
    let n = a.r;
    let mut vals: Vec<Z> = (0..n).map(|i| e.S().column_vector()[i]).collect();
    let mut v = M::of_faer(e.U());
    // Conjugate pairs of a real matrix come positive imaginary part first, as from LAPACK.
    if a.is_real() {
        let mut j = 0;
        while j + 1 < n {
            if vals[j].im < 0.0 && vals[j + 1] == vals[j].conj() {
                vals.swap(j, j + 1);
                for i in 0..n {
                    let t = v.at(i, j);
                    v.set(i, j, v.at(i, j + 1));
                    v.set(i, j + 1, t);
                }
                j += 2;
            } else {
                j += 1;
            }
        }
    }
    for j in 0..n {
        let nrm = vnorm(&v.col(j));
        if nrm > 0.0 {
            for i in 0..n {
                *v.rf(i, j) /= nrm;
            }
        }
    }
    Ok((vals, v))
}

/// Ascending eigenvalues and orthonormal eigenvectors of a Hermitian matrix.
fn eig_herm(a: &M) -> R<(Vec<f64>, M)> {
    if a.r == 0 {
        return Ok((Vec::new(), M::zeros(0, 0)));
    }
    let fail = |_| LangError::new("EIG did not converge.", 0);
    if a.is_real() {
        let e = a.faer_re().self_adjoint_eigen(faer::Side::Lower).map_err(fail)?;
        Ok(((0..a.r).map(|i| e.S().column_vector()[i]).collect(), M::of_faer_re(e.U())))
    } else {
        let e = a.faer().self_adjoint_eigen(faer::Side::Lower).map_err(fail)?;
        Ok(((0..a.r).map(|i| e.S().column_vector()[i].re).collect(), M::of_faer(e.U())))
    }
}

fn is_herm(a: &M) -> bool {
    a.is_square() && (0..a.r).all(|j| (j..a.r).all(|i| a.at(i, j) == a.at(j, i).conj()))
}

/// Eigenvalues of a square matrix.
fn eigvals(a: &M) -> R<Vec<Z>> {
    if is_herm(a) {
        Ok(eig_herm(a)?.0.into_iter().map(re).collect())
    } else {
        Ok(eig_general(a)?.0)
    }
}

// ---- Hessenberg and Schur ----------------------------------------------------------------

/// Householder reduction as `gehrd`: returns (P, H) with A = P*H*P'.
fn hessenberg(a: &M) -> (M, M) {
    let n = a.r;
    let mut h = a.clone();
    let mut p = M::eye(n);
    for k in 0..n.saturating_sub(1) {
        let x: Vec<Z> = (k + 1..n).map(|i| h.at(i, k)).collect();
        let (beta, tau, v) = house(&x);
        apply_left(&mut h, &v, tau.conj(), k + 1, k + 1, n);
        apply_right(&mut h, &v, tau, k + 1, 0, n);
        apply_right(&mut p, &v, tau, k + 1, 0, n);
        h.set(k + 1, k, beta);
        for i in k + 2..n {
            h.set(i, k, ZERO);
        }
    }
    (p, h)
}

/// Complex Schur form A = U*T*U' by shifted QR on the Hessenberg form.
fn schur_complex(a: &M) -> R<(M, M)> {
    let n = a.r;
    let (mut u, mut t) = hessenberg(a);
    if n < 2 {
        return Ok((u, t));
    }
    let mut hi = n - 1;
    let mut iter = 0;
    let mut total = 0;
    while hi > 0 {
        let mut l = hi;
        while l > 0 {
            let s = cabs1(t.at(l, l)) + cabs1(t.at(l - 1, l - 1));
            if cabs1(t.at(l, l - 1)) <= EPS * s || t.at(l, l - 1) == ZERO {
                t.set(l, l - 1, ZERO);
                break;
            }
            l -= 1;
        }
        if l == hi {
            hi -= 1;
            iter = 0;
            continue;
        }
        iter += 1;
        total += 1;
        if total > 100 * n {
            return rt("Schur decomposition did not converge.");
        }
        let shift = if iter % 10 == 0 {
            t.at(hi, hi) + re(t.at(hi, hi - 1).re.abs() * 0.75)
        } else {
            let (a11, a12, a21, a22) = (t.at(hi - 1, hi - 1), t.at(hi - 1, hi), t.at(hi, hi - 1), t.at(hi, hi));
            let half = (a11 - a22) * 0.5;
            let disc = csqrt(half * half + a12 * a21);
            let m = (a11 + a22) * 0.5;
            let (e1, e2) = (m + disc, m - disc);
            if abs(e1 - a22) <= abs(e2 - a22) { e1 } else { e2 }
        };
        let mut x = t.at(l, l) - shift;
        let mut y = t.at(l + 1, l);
        for k in l..hi {
            if k > l {
                x = t.at(k, k - 1);
                y = t.at(k + 1, k - 1);
            }
            let (c, s) = givens(x, y);
            let c0 = if k > l { k - 1 } else { l };
            rot_rows(&mut t, k, k + 1, c, s, c0, n);
            rot_cols(&mut t, k, k + 1, c, s, 0, (k + 3).min(hi + 1));
            rot_cols(&mut u, k, k + 1, c, s, 0, n);
            if k > l {
                t.set(k + 1, k - 1, ZERO);
            }
        }
    }
    for j in 0..n {
        for i in j + 1..n {
            t.set(i, j, ZERO);
        }
    }
    Ok((u, t))
}

/// LAPACK `lanv2`: the standardized Schur factorization of a real 2×2 block,
/// [a b; c d] = [cs -sn; sn cs] * [aa bb; cc dd] * [cs sn; -sn cs].
fn lanv2(mut a: f64, mut b: f64, mut c: f64, mut d: f64) -> ([f64; 4], f64, f64) {
    let sign = |x: f64, y: f64| if y >= 0.0 { x.abs() } else { -x.abs() };
    let (mut cs, mut sn);
    if c == 0.0 {
        cs = 1.0;
        sn = 0.0;
    } else if b == 0.0 {
        cs = 0.0;
        sn = 1.0;
        std::mem::swap(&mut a, &mut d);
        b = -c;
        c = 0.0;
    } else if a - d == 0.0 && b.signum() != c.signum() {
        cs = 1.0;
        sn = 0.0;
    } else {
        let temp = a - d;
        let mut p = 0.5 * temp;
        let bcmax = b.abs().max(c.abs());
        let bcmis = b.abs().min(c.abs()) * sign(1.0, b) * sign(1.0, c);
        let scale = p.abs().max(bcmax);
        let mut z = p / scale * p + bcmax / scale * bcmis;
        if z >= 4.0 * EPS {
            z = p + sign(scale.sqrt() * z.sqrt(), p);
            a = d + z;
            d -= bcmax / z * bcmis;
            let tau = c.hypot(z);
            cs = z / tau;
            sn = c / tau;
            b -= c;
            c = 0.0;
        } else {
            let sigma = b + c;
            let tau = sigma.hypot(temp);
            cs = (0.5 * (1.0 + sigma.abs() / tau)).sqrt();
            sn = -(p / (tau * cs)) * sign(1.0, sigma);
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
                    if b.signum() == c.signum() {
                        let sab = b.abs().sqrt();
                        let sac = c.abs().sqrt();
                        p = sign(sab * sac, c);
                        let tau = 1.0 / (b + c).abs().sqrt();
                        a = temp + p;
                        d = temp - p;
                        b -= c;
                        c = 0.0;
                        let cs1 = sab * tau;
                        let sn1 = sac * tau;
                        let t2 = cs * cs1 - sn * sn1;
                        sn = cs * sn1 + sn * cs1;
                        cs = t2;
                    }
                } else {
                    b = -c;
                    c = 0.0;
                    let t2 = cs;
                    cs = -sn;
                    sn = t2;
                }
            }
        }
    }
    ([a, b, c, d], cs, sn)
}

/// Applies the `lanv2` standardization to the 2×2 block at k of a real quasi-triangular T.
fn standardize_block(t: &mut M, u: &mut M, k: usize) {
    let n = t.r;
    let (blk, cs, sn) = lanv2(t.at(k, k).re, t.at(k, k + 1).re, t.at(k + 1, k).re, t.at(k + 1, k + 1).re);
    // Rotation G = [cs sn; -sn cs] applied as T := G*T*G', U := U*G'.
    rot_rows(t, k, k + 1, cs, re(sn), k + 2, n);
    rot_cols(t, k, k + 1, cs, re(sn), 0, k);
    rot_cols(u, k, k + 1, cs, re(sn), 0, n);
    t.set(k, k, re(blk[0]));
    t.set(k, k + 1, re(blk[1]));
    t.set(k + 1, k, re(blk[2]));
    t.set(k + 1, k + 1, re(blk[3]));
}

/// Real Schur form (quasi-triangular, standardized 2×2 blocks) by Francis double-shift QR.
fn schur_real(a: &M) -> R<(M, M)> {
    let n = a.r;
    let (mut u, mut t) = hessenberg(a);
    let g = |t: &M, i: usize, j: usize| t.at(i, j).re;
    let mut hi = n as isize - 1;
    let mut iter = 0;
    let mut total = 0;
    while hi >= 1 {
        let h = hi as usize;
        let mut l = h;
        while l > 0 {
            let s = g(&t, l - 1, l - 1).abs() + g(&t, l, l).abs();
            if g(&t, l, l - 1).abs() <= EPS * s || g(&t, l, l - 1) == 0.0 {
                t.set(l, l - 1, ZERO);
                break;
            }
            l -= 1;
        }
        if l == h {
            hi -= 1;
            iter = 0;
            continue;
        }
        if l + 1 == h {
            standardize_block(&mut t, &mut u, l);
            hi -= 2;
            iter = 0;
            continue;
        }
        iter += 1;
        total += 1;
        if total > 100 * n {
            return rt("Schur decomposition did not converge.");
        }
        let (s, tt) = if iter % 10 == 0 {
            let w = g(&t, h, h - 1).abs() + g(&t, h - 1, h - 2).abs();
            (1.5 * w, w * w)
        } else {
            (g(&t, h - 1, h - 1) + g(&t, h, h), g(&t, h - 1, h - 1) * g(&t, h, h) - g(&t, h - 1, h) * g(&t, h, h - 1))
        };
        let mut x = g(&t, l, l) * g(&t, l, l) + g(&t, l, l + 1) * g(&t, l + 1, l) - s * g(&t, l, l) + tt;
        let mut y = g(&t, l + 1, l) * (g(&t, l, l) + g(&t, l + 1, l + 1) - s);
        let mut zz = g(&t, l + 1, l) * g(&t, l + 2, l + 1);
        for k in l..h - 1 {
            let (_, tau, v) = house(&[re(x), re(y), re(zz)]);
            let c0 = if k > l { k - 1 } else { l };
            apply_left(&mut t, &v, tau.conj(), k, c0, n);
            let r1 = (k + 4).min(h + 1);
            apply_right(&mut t, &v, tau, k, 0, r1);
            apply_right(&mut u, &v, tau, k, 0, n);
            if k > l {
                t.set(k + 1, k - 1, ZERO);
                t.set(k + 2, k - 1, ZERO);
            }
            x = g(&t, k + 1, k);
            y = g(&t, k + 2, k);
            if k + 3 <= h {
                zz = g(&t, k + 3, k);
            }
        }
        let (c, sn) = givens(re(x), re(y));
        let c0 = if h - 1 > l { h - 2 } else { l };
        rot_rows(&mut t, h - 1, h, c, sn, c0, n);
        rot_cols(&mut t, h - 1, h, c, sn, 0, h + 1);
        rot_cols(&mut u, h - 1, h, c, sn, 0, n);
        if h - 1 > l {
            t.set(h, h - 2, ZERO);
        }
    }
    for j in 0..n {
        for i in j + 2..n {
            t.set(i, j, ZERO);
        }
    }
    for z in t.d.iter_mut().chain(u.d.iter_mut()) {
        z.im = 0.0;
    }
    Ok((u, t))
}

/// Schur form, real (quasi-triangular) for real input unless `complex`.
fn schur_of(a: &M, real_in: bool, complex: bool) -> R<(M, M)> {
    if a.has_nonfinite() {
        return rt("Input to SCHUR must not contain NaN or Inf.");
    }
    if real_in && !complex {
        schur_real(a)
    } else if real_in {
        let (u, t) = schur_real(a)?;
        Ok(rsf2csf(&u, &t))
    } else {
        schur_complex(a)
    }
}

/// Diagonal blocks (start, size) of a quasi-triangular matrix.
fn blocks_of(t: &M) -> Vec<(usize, usize)> {
    let n = t.r;
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if i + 1 < n && t.at(i + 1, i) != ZERO {
            out.push((i, 2));
            i += 2;
        } else {
            out.push((i, 1));
            i += 1;
        }
    }
    out
}

/// Eigenvalues along the diagonal of a quasi-triangular matrix (2×2 blocks give the pair
/// with positive imaginary part first).
fn ordeig_std(t: &M) -> Vec<Z> {
    let mut out = Vec::new();
    for (k, s) in blocks_of(t) {
        if s == 1 {
            out.push(t.at(k, k));
        } else {
            let (a, b, c, d) = (t.at(k, k), t.at(k, k + 1), t.at(k + 1, k), t.at(k + 1, k + 1));
            let m = (a + d) * 0.5;
            let disc = csqrt((a - d) * (a - d) * 0.25 + b * c);
            let (e1, e2) = (m + disc, m - disc);
            if e1.im >= e2.im {
                out.extend([e1, e2]);
            } else {
                out.extend([e2, e1]);
            }
        }
    }
    out
}

/// Real Schur to complex Schur: a rotation per 2×2 block.
fn rsf2csf(u0: &M, t0: &M) -> (M, M) {
    let n = t0.r;
    let mut u = u0.clone();
    let mut t = t0.clone();
    for m in (1..n).rev() {
        if t.at(m, m - 1) != ZERO {
            let (a, b, c, d) = (t.at(m - 1, m - 1), t.at(m - 1, m), t.at(m, m - 1), t.at(m, m));
            let mid = (a + d) * 0.5;
            let disc = csqrt((a - d) * (a - d) * 0.25 + b * c);
            let mu = mid + disc - d;
            let r = abs(mu).hypot(abs(c));
            let cc = mu / r;
            let s = c / r;
            // G = [cc' s; -s cc]
            let g = [cc.conj(), s, -s, cc];
            for j in m - 1..n {
                let (x, y) = (t.at(m - 1, j), t.at(m, j));
                t.set(m - 1, j, g[0] * x + g[1] * y);
                t.set(m, j, g[2] * x + g[3] * y);
            }
            for i in 0..=m {
                let (x, y) = (t.at(i, m - 1), t.at(i, m));
                t.set(i, m - 1, x * g[0].conj() + y * g[1].conj());
                t.set(i, m, x * g[2].conj() + y * g[3].conj());
            }
            for i in 0..n {
                let (x, y) = (u.at(i, m - 1), u.at(i, m));
                u.set(i, m - 1, x * g[0].conj() + y * g[1].conj());
                u.set(i, m, x * g[2].conj() + y * g[3].conj());
            }
            t.set(m, m - 1, ZERO);
        }
    }
    (u, t)
}

// ---- reordering --------------------------------------------------------------------------

/// Solves the small dense system M x = b by Gaussian elimination with partial pivoting.
fn dense_solve(m: &M, b: &[Z]) -> Vec<Z> {
    match lu_solve(m, &M::col_vec(b)) {
        Some(x) => x.d,
        None => vec![ZERO; b.len()],
    }
}

/// Q (square) from the QR factorization of a tall matrix.
fn q_full(a: &M) -> M {
    qr_fact(a, false).q(a.r)
}

/// Swaps the adjacent diagonal blocks (k, n1) and (k+n1, n2) of the (quasi-)triangular
/// matrix S (and of T for a pencil), updating the left and right transforms.
#[allow(clippy::too_many_arguments)]
fn swap_blocks(s: &mut M, mut t: Option<&mut M>, q: &mut M, z: &mut M, k: usize, n1: usize, n2: usize, real: bool) {
    let n = s.r;
    let m = n1 + n2;
    let blk = |x: &M, r0: usize, r1: usize, c0: usize, c1: usize| x.block(k + r0, k + r1, k + c0, k + c1);
    let (a11, a12, a22) = (blk(s, 0, n1, 0, n1), blk(s, 0, n1, n1, m), blk(s, n1, m, n1, m));
    let nn = n1 * n2;
    let (rmat, lmat) = match &t {
        None => {
            // A11 X - X A22 = -A12
            let mut sys = M::zeros(nn, nn);
            for c in 0..n2 {
                for r in 0..n1 {
                    let row = c * n1 + r;
                    for kk in 0..n1 {
                        *sys.rf(row, c * n1 + kk) += a11.at(r, kk);
                    }
                    for kk in 0..n2 {
                        *sys.rf(row, kk * n1 + r) -= a22.at(kk, c);
                    }
                }
            }
            let rhs: Vec<Z> = a12.d.iter().map(|x| -x).collect();
            let x = M { r: n1, c: n2, d: dense_solve(&sys, &rhs) };
            (x.clone(), x)
        }
        Some(t) => {
            let (b11, b12, b22) = (blk(t, 0, n1, 0, n1), blk(t, 0, n1, n1, m), blk(t, n1, m, n1, m));
            // A11 R - L A22 = -A12, B11 R - L B22 = -B12; unknowns [vec R; vec L].
            let mut sys = M::zeros(2 * nn, 2 * nn);
            for c in 0..n2 {
                for r in 0..n1 {
                    let row = c * n1 + r;
                    for kk in 0..n1 {
                        *sys.rf(row, c * n1 + kk) += a11.at(r, kk);
                        *sys.rf(nn + row, c * n1 + kk) += b11.at(r, kk);
                    }
                    for kk in 0..n2 {
                        *sys.rf(row, nn + kk * n1 + r) -= a22.at(kk, c);
                        *sys.rf(nn + row, nn + kk * n1 + r) -= b22.at(kk, c);
                    }
                }
            }
            let mut rhs: Vec<Z> = a12.d.iter().map(|x| -x).collect();
            rhs.extend(b12.d.iter().map(|x| -x));
            let sol = dense_solve(&sys, &rhs);
            (M { r: n1, c: n2, d: sol[..nn].to_vec() }, M { r: n1, c: n2, d: sol[nn..].to_vec() })
        }
    };
    let stack = |x: &M| {
        let mut w = M::zeros(m, n2);
        w.put(0, 0, x);
        w.put(n1, 0, &M::eye(n2));
        w
    };
    let zr = q_full(&stack(&rmat));
    let ql = if t.is_some() { q_full(&stack(&lmat)) } else { zr.clone() };
    // S := Ql' S Zr on the affected rows and columns.
    let apply = |x: &mut M| {
        let rows = ql.ct().mul(&x.block(k, k + m, 0, n));
        x.put(k, 0, &rows);
        let cols = x.block(0, n, k, k + m).mul(&zr);
        x.put(0, k, &cols);
    };
    apply(s);
    if let Some(t) = t.as_deref_mut() {
        apply(t);
    }
    let qn = ql.ct().mul(&q.block(k, k + m, 0, q.c));
    q.put(k, 0, &qn);
    let zn = z.block(0, z.r, k, k + m).mul(&zr);
    z.put(0, k, &zn);
    // Clean the now-zero lower-left block and restore the block shapes.
    for j in k..k + n2 {
        for i in k + n2..k + m {
            s.set(i, j, ZERO);
            if let Some(t) = t.as_deref_mut() {
                t.set(i, j, ZERO);
            }
        }
    }
    if let Some(t) = t.as_deref_mut() {
        for i in k..k + m {
            for j in k..i {
                t.set(i, j, ZERO);
            }
        }
    }
    let clean = |s: &mut M, t: Option<&mut M>, q: &mut M, z: &mut M, start: usize, size: usize| {
        if size == 1 {
            return;
        }
        if real && t.is_none() {
            standardize_block(s, z, start);
            *q = z.ct();
        } else if !real {
            // complex blocks are 1×1 already
        } else if let Some(t) = t {
            // Keep the 2×2 block of T upper triangular.
            let (c, sn) = givens(t.at(start + 1, start + 1), t.at(start + 1, start));
            let sn = sn.conj();
            rot_cols(t, start + 1, start, c, sn, 0, n);
            rot_cols(s, start + 1, start, c, sn, 0, n);
            rot_cols(z, start + 1, start, c, sn, 0, z.r);
            t.set(start + 1, start, ZERO);
        }
    };
    clean(s, t.as_deref_mut(), q, z, k, n2);
    clean(s, t, q, z, k + n2, n1);
}

/// Stable reordering of the diagonal blocks so that higher cluster numbers come first.
fn reorder(s: &mut M, mut t: Option<&mut M>, q: &mut M, z: &mut M, ids: &[f64], real: bool) {
    let mut blocks: Vec<(usize, f64)> = Vec::new();
    for (start, size) in blocks_of(s) {
        let id = (start..start + size).map(|i| ids.get(i).copied().unwrap_or(0.0)).fold(f64::NEG_INFINITY, f64::max);
        blocks.push((size, id));
    }
    loop {
        let mut swapped = false;
        let mut pos = 0;
        for b in 0..blocks.len().saturating_sub(1) {
            let (n1, id1) = blocks[b];
            let (n2, id2) = blocks[b + 1];
            if id2 > id1 {
                swap_blocks(s, t.as_deref_mut(), q, z, pos, n1, n2, real);
                blocks.swap(b, b + 1);
                swapped = true;
                pos += n2;
            } else {
                pos += n1;
            }
        }
        if !swapped {
            break;
        }
    }
}

// ---- QZ through faer -----------------------------------------------------------------------

/// Generalized Schur form: (AA, BB, Q, Z) with Q*A*Z = AA, Q*B*Z = BB.
fn qz_of(a: &M, b: &M, real: bool) -> R<(M, M, M, M)> {
    use faer::dyn_stack::{MemBuffer, MemStack};
    use faer::linalg::gevd;
    let n = a.r;
    if a.has_nonfinite() || b.has_nonfinite() {
        return rt("Input to QZ must not contain NaN or Inf.");
    }
    if n == 0 {
        return Ok((a.clone(), b.clone(), M::eye(0), M::eye(0)));
    }
    // B = Q1 R first, as the Hessenberg-triangular reduction expects triangular B.
    let qr = qr_fact(b, false);
    let a1 = qr.qt_mul(a);
    let b1 = qr.r(n);
    let q1 = qr.q(n);
    let par = faer::Par::Seq;
    if real {
        let mut fa = a1.faer_re();
        let mut fb = b1.faer_re();
        let mut fq = q1.faer_re();
        let mut fz = faer::Mat::<f64>::identity(n, n);
        let hp = <gevd::gen_hessenberg::GeneralizedHessenbergParams as faer::Auto<f64>>::auto();
        let mut buf = MemBuffer::new(gevd::gen_hessenberg::generalized_hessenberg_scratch::<f64>(n, hp));
        gevd::gen_hessenberg::generalized_hessenberg(fa.as_mut(), fb.as_mut(), Some(fq.as_mut()), Some(fz.as_mut()), par, MemStack::new(&mut buf), hp);
        let sp = <gevd::GeneralizedSchurParams as faer::Auto<f64>>::auto();
        let mut buf = MemBuffer::new(gevd::qz_real::hessenberg_to_qz_scratch::<f64>(n, par, sp));
        let (mut ar, mut ai, mut be) = (faer::Col::<f64>::zeros(n), faer::Col::<f64>::zeros(n), faer::Col::<f64>::zeros(n));
        gevd::qz_real::hessenberg_to_qz(
            fa.as_mut(),
            fb.as_mut(),
            Some(fq.as_mut()),
            Some(fz.as_mut()),
            ar.as_mut(),
            ai.as_mut(),
            be.as_mut(),
            faer::linalg::evd::ComputeEigenvectors::Yes,
            par,
            sp,
            MemStack::new(&mut buf),
        );
        let (mut aa, mut bb) = (M::of_faer_re(fa.as_ref()), M::of_faer_re(fb.as_ref()));
        for j in 0..n {
            for i in j + 1..n {
                bb.set(i, j, ZERO);
                if i > j + 1 {
                    aa.set(i, j, ZERO);
                }
            }
        }
        Ok((aa, bb, M::of_faer_re(fq.as_ref()).ct(), M::of_faer_re(fz.as_ref())))
    } else {
        let mut fa = a1.faer();
        let mut fb = b1.faer();
        let mut fq = q1.faer();
        let mut fz = faer::Mat::<Z>::identity(n, n);
        let hp = <gevd::gen_hessenberg::GeneralizedHessenbergParams as faer::Auto<Z>>::auto();
        let mut buf = MemBuffer::new(gevd::gen_hessenberg::generalized_hessenberg_scratch::<Z>(n, hp));
        gevd::gen_hessenberg::generalized_hessenberg(fa.as_mut(), fb.as_mut(), Some(fq.as_mut()), Some(fz.as_mut()), par, MemStack::new(&mut buf), hp);
        let sp = <gevd::GeneralizedSchurParams as faer::Auto<Z>>::auto();
        let mut buf = MemBuffer::new(gevd::qz_cplx::hessenberg_to_qz_scratch::<Z>(n, par, sp));
        let (mut al, mut be) = (faer::Col::<Z>::zeros(n), faer::Col::<Z>::zeros(n));
        gevd::qz_cplx::hessenberg_to_qz(
            fa.as_mut(),
            fb.as_mut(),
            Some(fq.as_mut()),
            Some(fz.as_mut()),
            al.as_mut(),
            be.as_mut(),
            faer::linalg::evd::ComputeEigenvectors::Yes,
            par,
            sp,
            MemStack::new(&mut buf),
        );
        let (mut aa, mut bb) = (M::of_faer(fa.as_ref()), M::of_faer(fb.as_ref()));
        for j in 0..n {
            for i in j + 1..n {
                aa.set(i, j, ZERO);
                bb.set(i, j, ZERO);
            }
        }
        Ok((aa, bb, M::of_faer(fq.as_ref()).ct(), M::of_faer(fz.as_ref())))
    }
}

/// Generalized eigenvalues along the diagonal of a (quasi-)triangular pencil.
fn ordeig_pencil(aa: &M, bb: &M) -> Vec<Z> {
    let mut out = Vec::new();
    for (k, s) in blocks_of(aa) {
        if s == 1 {
            let (a, b) = (aa.at(k, k), bb.at(k, k));
            out.push(if b == ZERO { Z::new(if a == ZERO { f64::NAN } else { f64::INFINITY }, 0.0) } else { dv(a, b) });
        } else {
            // eigenvalues of B^-1 A for the 2×2 block
            let a2 = aa.block(k, k + 2, k, k + 2);
            let b2 = bb.block(k, k + 2, k, k + 2);
            let m = lu_solve(&b2, &a2).unwrap_or_else(|| M::zeros(2, 2));
            out.extend(ordeig_std(&m));
        }
    }
    out
}

// ---- matrix functions --------------------------------------------------------------------

/// Scaling and squaring with Padé approximants (Higham 2005).
fn expm_of(a: &M) -> M {
    const THETA: [(usize, f64); 4] = [(3, 1.495585217958292e-2), (5, 2.53939833006323e-1), (7, 9.504178996162932e-1), (9, 2.097847961257068)];
    let nrm = a.norm1();
    for (m, th) in THETA {
        if nrm <= th {
            return pade_exp(a, m);
        }
    }
    let s = ((nrm / 5.371920351148152).log2().ceil()).max(0.0) as i32;
    let a = a.scale(re(2f64.powi(-s)));
    let mut x = pade_exp(&a, 13);
    for _ in 0..s {
        x = x.mul(&x);
    }
    x
}

fn pade_exp(a: &M, m: usize) -> M {
    let n = a.r;
    let b: &[f64] = match m {
        3 => &[120.0, 60.0, 12.0, 1.0],
        5 => &[30240.0, 15120.0, 3360.0, 420.0, 30.0, 1.0],
        7 => &[17297280.0, 8648640.0, 1995840.0, 277200.0, 25200.0, 1512.0, 56.0, 1.0],
        9 => &[17643225600.0, 8821612800.0, 2075673600.0, 302702400.0, 30270240.0, 2162160.0, 110880.0, 3960.0, 90.0, 1.0],
        _ => &[
            64764752532480000.0,
            32382376266240000.0,
            7771770303897600.0,
            1187353796428800.0,
            129060195264000.0,
            10559470521600.0,
            670442572800.0,
            33522128640.0,
            1323241920.0,
            40840800.0,
            960960.0,
            16380.0,
            182.0,
            1.0,
        ],
    };
    let id = M::eye(n);
    let a2 = a.mul(a);
    let (u, v);
    if m == 13 {
        let a4 = a2.mul(&a2);
        let a6 = a4.mul(&a2);
        let lin = |c: [f64; 3]| a6.scale(re(c[0])).add(&a4.scale(re(c[1]))).add(&a2.scale(re(c[2])));
        let u_in = a6.mul(&lin([b[13], b[11], b[9]])).add(&a6.scale(re(b[7]))).add(&a4.scale(re(b[5]))).add(&a2.scale(re(b[3]))).add(&id.scale(re(b[1])));
        u = a.mul(&u_in);
        v = a6.mul(&lin([b[12], b[10], b[8]])).add(&a6.scale(re(b[6]))).add(&a4.scale(re(b[4]))).add(&a2.scale(re(b[2]))).add(&id.scale(re(b[0])));
    } else {
        let mut powers = vec![id.clone(), a2.clone()];
        while powers.len() <= m / 2 {
            let next = powers.last().unwrap().mul(&a2);
            powers.push(next);
        }
        let mut uu = M::zeros(n, n);
        let mut vv = M::zeros(n, n);
        for (k, pw) in powers.iter().enumerate().take(m / 2 + 1) {
            uu = uu.add(&pw.scale(re(b[2 * k + 1])));
            vv = vv.add(&pw.scale(re(b[2 * k])));
        }
        u = a.mul(&uu);
        v = vv;
    }
    lu_solve(&v.sub(&u), &v.add(&u)).unwrap_or_else(|| M::from_fn(n, n, |_, _| re(f64::NAN)))
}

/// Quotient that gives MATLAB's Inf/NaN for a zero divisor.
fn div_or_inf(num: Z, d: Z) -> Z {
    if d != ZERO {
        return dv(num, d);
    }
    if num == ZERO {
        return re(f64::NAN);
    }
    Z::new(num.re / 0.0, if num.im == 0.0 { 0.0 } else { num.im / 0.0 })
}

/// Upper triangular square root (Björck-Hammarling).
fn sqrtm_tri(t: &M) -> M {
    let n = t.r;
    let mut r = M::zeros(n, n);
    for j in 0..n {
        r.set(j, j, csqrt(t.at(j, j)));
        for i in (0..j).rev() {
            let mut s = ZERO;
            for k in i + 1..j {
                s += r.at(i, k) * r.at(k, j);
            }
            let v = div_or_inf(t.at(i, j) - s, r.at(i, i) + r.at(j, j));
            r.set(i, j, v);
        }
    }
    r
}

/// Gauss-Legendre nodes and weights on [0, 1] (8 points).
fn gauss_legendre8() -> [(f64, f64); 8] {
    let x = [0.1834346424956498, 0.525532409916329, 0.7966664774136267, 0.9602898564975363];
    let w = [0.362683783378362, 0.3137066458778873, 0.2223810344533745, 0.1012285362903763];
    let mut out = [(0.0, 0.0); 8];
    for k in 0..4 {
        out[2 * k] = ((1.0 - x[k]) / 2.0, w[k] / 2.0);
        out[2 * k + 1] = ((1.0 + x[k]) / 2.0, w[k] / 2.0);
    }
    out
}

/// Logarithm of an upper triangular matrix by inverse scaling and squaring; the flag says
/// whether the square root limit was hit.
fn logm_tri(t0: &M) -> (M, bool) {
    let n = t0.r;
    let id = M::eye(n);
    let mut t = t0.clone();
    let mut k = 0;
    while t.sub(&id).norm1() > 0.25 && k < 100 {
        t = sqrtm_tri(&t);
        k += 1;
    }
    let x = t.sub(&id);
    let mut l = M::zeros(n, n);
    for (node, w) in gauss_legendre8() {
        let den = id.add(&x.scale(re(node)));
        let term = lu_solve(&den.tr(), &x.tr()).map(|m| m.tr()).unwrap_or_else(|| M::from_fn(n, n, |_, _| re(f64::NAN)));
        l = l.add(&term.scale(re(w)));
    }
    (l.scale(re(2f64.powi(k))), k >= 100)
}

/// Complex Schur form for the matrix functions (real input goes through the real form).
fn cschur(a: &M) -> R<(M, M)> {
    schur_of(a, a.is_real(), true)
}

fn has_nonpos_real_eig(t: &M) -> bool {
    (0..t.r).any(|i| t.at(i, i).im == 0.0 && t.at(i, i).re <= 0.0)
}

/// The k-th derivative of a built-in function at x, for `funm`'s named handles.
fn named_deriv(name: &str, x: Z, k: usize) -> Option<Z> {
    let cx = C::new(x.re, x.im);
    let z = |c: C| Z::new(c.re, c.im);
    Some(match name {
        "exp" => cexp(x),
        "log" => {
            if k == 0 {
                cln(x)
            } else {
                let fact: f64 = (1..k).map(|i| i as f64).product();
                let sign = if k % 2 == 1 { 1.0 } else { -1.0 };
                let mut p = ONE;
                for _ in 0..k {
                    p *= x;
                }
                dv(re(sign * fact), p)
            }
        }
        "sin" | "cos" => {
            let (s, c) = (z(cx.sin()), z(cx.cos()));
            let shift = if name == "cos" { k + 1 } else { k };
            match shift % 4 {
                0 => s,
                1 => c,
                2 => -s,
                _ => -c,
            }
        }
        "sinh" | "cosh" => {
            let (s, c) = (z(cx.sinh()), z(cx.cosh()));
            if (k + usize::from(name == "cosh")).is_multiple_of(2) { s } else { c }
        }
        _ => return None,
    })
}

/// Evaluates fun(x, k) for `funm`.
fn fun_deriv(it: &mut Interpreter, f: &Value, x: Z, k: usize) -> R<Z> {
    if let Value::Func(FuncValue::Named(name)) = f {
        if let Some(v) = named_deriv(name, x, k) {
            return Ok(v);
        }
    }
    let xv = if x.im == 0.0 { Value::num(x.re) } else { Value::Num(Matrix::complex_scalar(x.re, x.im)) };
    let out = it.call_func_value(f, vec![xv, Value::num(k as f64)], 1)?;
    match out.first() {
        Some(Value::Num(m)) if m.numel() >= 1 => Ok(Z::new(m.data[0], m.im_at(0))),
        _ => rt("FUN must return a numeric value."),
    }
}

/// Schur-Parlett evaluation of f(A) (Davies-Higham): eigenvalue clusters are evaluated by
/// Taylor series, the coupling by block Parlett recurrences.
fn funm_of(it: &mut Interpreter, a: &M, f: &Value) -> R<M> {
    let n = a.r;
    let (mut u, mut t) = cschur(a)?;
    // Cluster eigenvalues closer than delta (transitively).
    let delta = 0.1;
    let mut cluster: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in 0..i {
            if abs(t.at(i, i) - t.at(j, j)) <= delta {
                let (ci, cj) = (cluster[i], cluster[j]);
                if ci != cj {
                    for c in cluster.iter_mut() {
                        if *c == ci {
                            *c = cj;
                        }
                    }
                }
            }
        }
    }
    // Rank clusters by first appearance; reorder so that each is contiguous.
    let mut order: Vec<usize> = Vec::new();
    for &c in &cluster {
        if !order.contains(&c) {
            order.push(c);
        }
    }
    let ids: Vec<f64> = cluster.iter().map(|c| (order.len() - order.iter().position(|o| o == c).unwrap_or(0)) as f64).collect();
    let mut q = u.ct();
    reorder(&mut t, None, &mut q, &mut u, &ids, false);
    let mut sorted_ids = ids.clone();
    sorted_ids.sort_by(|x, y| y.total_cmp(x));
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    let mut s = 0;
    while s < n {
        let mut e = s + 1;
        while e < n && sorted_ids[e] == sorted_ids[s] {
            e += 1;
        }
        blocks.push((s, e));
        s = e;
    }
    let mut fm = M::zeros(n, n);
    for &(s, e) in &blocks {
        let m = e - s;
        let tb = t.block(s, e, s, e);
        if m == 1 {
            fm.set(s, s, fun_deriv(it, f, tb.at(0, 0), 0)?);
            continue;
        }
        let sigma = (0..m).map(|i| tb.at(i, i)).fold(ZERO, |acc, x| acc + x) / m as f64;
        let nmat = tb.sub(&M::eye(m).scale(sigma));
        let mut term = M::eye(m);
        let mut fb = M::eye(m).scale(fun_deriv(it, f, sigma, 0)?);
        let mut fact = 1.0;
        for k in 1..=300 {
            term = term.mul(&nmat);
            fact *= k as f64;
            let dk = fun_deriv(it, f, sigma, k)?;
            let add = term.scale(dk / fact);
            fb = fb.add(&add);
            if add.norm1() <= EPS * fb.norm1() && k > m {
                break;
            }
        }
        fm.put(s, s, &fb);
    }
    for jb in 0..blocks.len() {
        for ib in (0..jb).rev() {
            let (is, ie) = blocks[ib];
            let (js, je) = blocks[jb];
            let (tii, tjj, tij) = (t.block(is, ie, is, ie), t.block(js, je, js, je), t.block(is, ie, js, je));
            let (fii, fjj) = (fm.block(is, ie, is, ie), fm.block(js, je, js, je));
            let mut rhs = fii.mul(&tij).sub(&tij.mul(&fjj));
            for &(ks, ke) in &blocks[ib + 1..jb] {
                let fik = fm.block(is, ie, ks, ke);
                let tkj = t.block(ks, ke, js, je);
                let tik = t.block(is, ie, ks, ke);
                let fkj = fm.block(ks, ke, js, je);
                rhs = rhs.add(&fik.mul(&tkj)).sub(&tik.mul(&fkj));
            }
            let x = sylvester_solve(&tii, &tjj.scale(re(-1.0)), &rhs);
            fm.put(is, js, &x);
        }
    }
    Ok(u.mul(&fm).mul(&u.ct()))
}

/// Solves A X + X B = C by the Kronecker form (small systems).
fn sylvester_solve(a: &M, b: &M, c: &M) -> M {
    let (m, n) = (a.r, b.r);
    let mut sys = M::zeros(m * n, m * n);
    for col in 0..n {
        for r in 0..m {
            let row = col * m + r;
            for k in 0..m {
                *sys.rf(row, col * m + k) += a.at(r, k);
            }
            for k in 0..n {
                *sys.rf(row, k * m + r) += b.at(k, col);
            }
        }
    }
    M { r: m, c: n, d: dense_solve(&sys, &c.d) }
}

/// Solves A X + X B = C through the Schur forms of A and B (Bartels-Stewart).
fn sylvester_of(a: &M, b: &M, c: &M) -> R<M> {
    let (ua, ta) = cschur(a)?;
    let (ub, tb) = cschur(b)?;
    let f = ua.ct().mul(c).mul(&ub);
    let (m, n) = (a.r, b.r);
    let mut y = M::zeros(m, n);
    for k in 0..n {
        // (TA + tb(k,k) I) y_k = f_k - sum_{j<k} y_j tb(j,k)
        let mut rhs = f.col(k);
        for j in 0..k {
            let t = tb.at(j, k);
            for (i, r) in rhs.iter_mut().enumerate() {
                *r -= y.at(i, j) * t;
            }
        }
        let d = tb.at(k, k);
        for i in (0..m).rev() {
            let mut s = rhs[i];
            for l in i + 1..m {
                s -= ta.at(i, l) * y.at(l, k);
            }
            y.set(i, k, div_or_inf(s, ta.at(i, i) + d));
        }
    }
    Ok(ua.mul(&y).mul(&ub.ct()))
}

// ---- helpers for the dispatcher ------------------------------------------------------------

fn rank_tol(s: &[f64], m: usize, n: usize, tol: Option<f64>) -> usize {
    let tol = tol.unwrap_or_else(|| (m.max(n) as f64) * eps_of(s.first().copied().unwrap_or(0.0)));
    s.iter().filter(|&&x| x > tol).count()
}

/// Reduced row echelon form and the pivot columns.
fn rref_of(a: &M, tol: Option<f64>) -> (M, Vec<usize>) {
    let (m, n) = (a.r, a.c);
    let mut x = a.clone();
    let tol = tol.unwrap_or_else(|| (m.max(n) as f64) * EPS * a.norm_inf());
    let mut jb = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < m && j < n {
        let mut k = i;
        let mut p = abs(x.at(i, j));
        for r in i + 1..m {
            let v = abs(x.at(r, j));
            if v > p {
                p = v;
                k = r;
            }
        }
        if p <= tol {
            for r in i..m {
                x.set(r, j, ZERO);
            }
            j += 1;
            continue;
        }
        jb.push(j);
        for c in j..n {
            let t = x.at(i, c);
            x.set(i, c, x.at(k, c));
            x.set(k, c, t);
        }
        let piv = x.at(i, j);
        for c in j..n {
            let v = dv(x.at(i, c), piv);
            x.set(i, c, v);
        }
        for r in 0..m {
            if r == i {
                continue;
            }
            let f = x.at(r, j);
            for c in j..n {
                let v = x.at(r, c) - f * x.at(i, c);
                x.set(r, c, v);
            }
        }
        i += 1;
        j += 1;
    }
    (x, jb)
}

fn orth_of(a: &M, tol: Option<f64>) -> R<M> {
    let (u, s, _) = svd_of(a, a.is_real(), true)?;
    let r = rank_tol(&s, a.r, a.c, tol);
    Ok(u.block(0, u.r, 0, r))
}

fn null_of(a: &M, tol: Option<f64>) -> R<M> {
    let (_, s, v) = svd_of(a, a.is_real(), false)?;
    let r = rank_tol(&s, a.r, a.c, tol);
    Ok(v.block(0, v.r, r, v.c))
}

fn pinv_of(a: &M, tol: Option<f64>) -> R<M> {
    let (u, s, v) = svd_of(a, a.is_real(), true)?;
    let r = rank_tol(&s, a.r, a.c, tol);
    let mut vs = v.block(0, v.r, 0, r);
    for (j, sj) in s.iter().enumerate().take(r) {
        for i in 0..vs.r {
            *vs.rf(i, j) /= *sj;
        }
    }
    Ok(vs.mul(&u.block(0, u.r, 0, r).ct()))
}

/// Pages of an N-D array: (rows, cols, page count, trailing dims).
fn pages(m: &Matrix) -> (usize, usize, usize, Vec<usize>) {
    let r = m.dims[0];
    let c = m.dims.get(1).copied().unwrap_or(1);
    let rest: Vec<usize> = m.dims.iter().skip(2).copied().collect();
    (r, c, rest.iter().product(), rest)
}

fn page(m: &Matrix, r: usize, c: usize, p: usize) -> M {
    let off = p * r * c;
    M::from_fn(r, c, |i, j| {
        let k = off + j * r + i;
        Z::new(m.data[k], m.im_at(k))
    })
}

/// Stacks equally sized pages into an N-D value.
fn stack_pages(ps: &[M], rest: &[usize], cplx: bool) -> Value {
    let (r, c) = ps.first().map(|p| (p.r, p.c)).unwrap_or((0, 0));
    let mut dims = vec![r, c];
    dims.extend_from_slice(rest);
    let rp: Vec<f64> = ps.iter().flat_map(|p| p.d.iter().map(|z| z.re)).collect();
    if cplx {
        let im: Vec<f64> = ps.iter().flat_map(|p| p.d.iter().map(|z| z.im)).collect();
        Value::Num(Matrix::complex(dims, rp, im))
    } else {
        Value::Num(Matrix::from_dims(dims, rp))
    }
}

/// Cluster labels for `ordschur`/`ordqz` from a keyword, logical selection or labels.
fn selection(v: &Value, eigs: &[Z], name: &str) -> R<Vec<f64>> {
    if let Some(k) = opt_text(Some(v)) {
        let f: fn(Z) -> bool = match k.as_str() {
            "lhp" => |z| z.re < 0.0,
            "rhp" => |z| z.re > 0.0,
            "udi" => |z| abs(z) < 1.0,
            "udo" => |z| abs(z) > 1.0,
            _ => return rt(format!("Invalid keyword for {}. Valid keywords are 'lhp', 'rhp', 'udi', and 'udo'.", name.to_uppercase())),
        };
        return Ok(eigs.iter().map(|&z| if f(z) { 1.0 } else { 0.0 }).collect());
    }
    let m = v.to_matrix()?;
    if m.numel() != eigs.len() {
        return rt("Cluster or selection vector must have length equal to the number of eigenvalues.");
    }
    Ok(m.data.clone())
}

fn struct_flag(s: &Value, field: &str) -> bool {
    if let Value::Struct(st) = s {
        if let Some(k) = st.fields.iter().position(|f| f == field) {
            if let Some(v) = st.elems.first().and_then(|e| e.get(k)) {
                return v.is_true().unwrap_or(false);
            }
        }
    }
    false
}

/// Triangular solve using only one triangle of A.
fn tri_solve(a: &M, b: &M, upper: bool) -> M {
    let n = a.r;
    let mut x = b.clone();
    for j in 0..b.c {
        if upper {
            for i in (0..n).rev() {
                let mut s = x.at(i, j);
                for k in i + 1..n {
                    s -= a.at(i, k) * x.at(k, j);
                }
                x.set(i, j, div_or_inf(s, a.at(i, i)));
            }
        } else {
            for i in 0..n {
                let mut s = x.at(i, j);
                for k in 0..i {
                    s -= a.at(i, k) * x.at(k, j);
                }
                x.set(i, j, div_or_inf(s, a.at(i, i)));
            }
        }
    }
    x
}

fn tri_rcond(a: &M, upper: bool) -> f64 {
    let n = a.r;
    if n == 0 {
        return f64::INFINITY;
    }
    let t = if upper { a.triu(0) } else { a.tril(0) };
    if (0..n).any(|i| t.at(i, i) == ZERO) {
        return 0.0;
    }
    let inv = tri_solve(&t, &M::eye(n), upper);
    1.0 / (t.norm1() * inv.norm1())
}

/// Generalized SVD through the QR factorization of [A; B] and the SVD of its lower block.
fn gsvd_of(a: &M, b: &M) -> R<(M, M, M, M, M)> {
    let (m, n, p) = (a.r, b.r, a.c);
    if m + n < p {
        return rt("GSVD requires the stacked matrix [A;B] to have at least as many rows as columns.");
    }
    let mut st = M::zeros(m + n, p);
    st.put(0, 0, a);
    st.put(m, 0, b);
    let qr = qr_fact(&st, false);
    let q = qr.q(p);
    let r = qr.r(p);
    let q1 = q.block(0, m, 0, p);
    let q2 = q.block(m, m + n, 0, p);
    let real = a.is_real() && b.is_real();
    let (v, s, w) = svd_of(&q2, real, false)?;
    let y = q1.mul(&w);
    let svals: Vec<f64> = (0..p).map(|i| s.get(i).copied().unwrap_or(0.0)).collect();
    let cvals: Vec<f64> = (0..p).map(|i| vnorm(&y.col(i))).collect();
    let off = p.saturating_sub(m);
    let mut u = M::zeros(m, m);
    let mut c = M::zeros(m, p);
    let mut known = Vec::new();
    for i in 0..m.min(p) {
        let j = i + off;
        c.set(i, j, re(cvals[j]));
        if cvals[j] > EPS {
            for k in 0..m {
                u.set(k, i, y.at(k, j) / cvals[j]);
            }
            known.push(i);
        }
    }
    // Orthonormal completion of U.
    if known.len() < m {
        let mut basis = M::zeros(m, known.len() + m);
        basis.put(0, 0, &u.cols(&known));
        basis.put(0, known.len(), &M::eye(m));
        let full = q_full(&basis);
        let mut extra = known.len();
        for j in 0..m {
            if !known.contains(&j) {
                for i in 0..m {
                    u.set(i, j, full.at(i, extra));
                }
                extra += 1;
            }
        }
    }
    let mut sm = M::zeros(n, p);
    for (i, sv) in svals.iter().enumerate().take(n.min(p)) {
        sm.set(i, i, re(*sv));
    }
    let x = r.ct().mul(&w);
    Ok((u, v, x, c, sm))
}

/// Unit eigenvectors of the pencil (A, B) for the given eigenvalues, from the null space of
/// A - lambda*B (B for infinite eigenvalues); `left` gives left eigenvectors.
fn pencil_vectors(a: &M, b: &M, lam: &[Z], left: bool) -> R<M> {
    let n = a.r;
    let mut v = M::zeros(n, lam.len());
    for (k, &l) in lam.iter().enumerate() {
        let m = if l.re.is_infinite() { b.clone() } else { a.sub(&b.scale(l)) };
        let m = if left { m.ct() } else { m };
        let (_, _, vv) = svd_of(&m, m.is_real(), false)?;
        for i in 0..n {
            v.set(i, k, vv.at(i, n - 1));
        }
    }
    Ok(v)
}

fn sigma_key(name: &str) -> R<&'static str> {
    Ok(match name {
        "largestabs" | "lm" => "la",
        "smallestabs" | "sm" => "sa",
        "largestreal" | "lr" | "la" => "lr",
        "smallestreal" | "sr" | "sa" => "sr",
        "bothendsreal" | "be" => "be",
        "largestimag" | "li" => "li",
        "smallestimag" | "si" => "si",
        "bothendsimag" => "bi",
        _ => return rt("Invalid value for the sigma input argument."),
    })
}

/// Indices of the k eigenvalues `eigs` chooses for sigma, in its output order.
fn pick(vals: &[Z], k: usize, sigma: Option<Z>, key: &str) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..vals.len()).collect();
    let cmp = |a: f64, b: f64| a.total_cmp(&b);
    match (sigma, key) {
        (Some(s), _) => idx.sort_by(|&i, &j| cmp(abs(vals[i] - s), abs(vals[j] - s))),
        (_, "la") => idx.sort_by(|&i, &j| cmp(abs(vals[j]), abs(vals[i]))),
        (_, "sa") => idx.sort_by(|&i, &j| cmp(abs(vals[i]), abs(vals[j]))),
        (_, "lr") => idx.sort_by(|&i, &j| cmp(vals[j].re, vals[i].re)),
        (_, "sr") => idx.sort_by(|&i, &j| cmp(vals[i].re, vals[j].re)),
        (_, "li") => idx.sort_by(|&i, &j| cmp(vals[j].im.abs(), vals[i].im.abs())),
        (_, "si") => idx.sort_by(|&i, &j| cmp(vals[i].im.abs(), vals[j].im.abs())),
        _ => {
            let im = key == "bi";
            // One more from the high end when k is odd; ascending output.
            idx.sort_by(|&i, &j| if im { cmp(vals[i].im, vals[j].im) } else { cmp(vals[i].re, vals[j].re) });
            let k = k.min(idx.len());
            let low = k / 2;
            let mut out: Vec<usize> = idx[..low].to_vec();
            out.extend_from_slice(&idx[idx.len() - (k - low)..]);
            return out;
        }
    }
    idx.truncate(k.min(vals.len()));
    idx
}

/// LAPACK `gebal`: permutation to isolate eigenvalues, then power-of-2 scaling. Returns
/// (T, B) with B = T\A*T.
#[allow(clippy::needless_range_loop)]
fn balance_of(a: &M, permute: bool) -> (M, M) {
    let n = a.r;
    let mut b = a.clone();
    let mut t = M::eye(n);
    if n == 0 {
        return (t, b);
    }
    let nz = |z: Z| z != ZERO;
    let exchange = |b: &mut M, t: &mut M, j: usize, m: usize, k: usize, l: usize| {
        if j == m {
            return;
        }
        for i in 0..=l {
            let x = b.at(i, j);
            b.set(i, j, b.at(i, m));
            b.set(i, m, x);
        }
        for c in k..n {
            let x = b.at(j, c);
            b.set(j, c, b.at(m, c));
            b.set(m, c, x);
        }
        for i in 0..n {
            let x = t.at(i, j);
            t.set(i, j, t.at(i, m));
            t.set(i, m, x);
        }
    };
    let (mut k, mut l) = (0usize, n - 1);
    if permute {
        // Rows isolating an eigenvalue go to the bottom.
        while let Some(j) = (0..=l).rev().find(|&j| (0..=l).all(|i| i == j || !nz(b.at(j, i)))) {
            exchange(&mut b, &mut t, j, l, k, l);
            if l == 0 {
                return (t, b);
            }
            l -= 1;
        }
        // Columns isolating an eigenvalue go to the left.
        while let Some(j) = (k..=l).find(|&j| (k..=l).all(|i| i == j || !nz(b.at(i, j)))) {
            exchange(&mut b, &mut t, j, k, k, l);
            k += 1;
        }
    }
    let sfmin1 = f64::MIN_POSITIVE / (EPS * 0.5);
    let sfmax1 = 1.0 / sfmin1;
    let sfmin2 = sfmin1 * 2.0;
    let sfmax2 = 1.0 / sfmin2;
    let mut scale = vec![1.0f64; n];
    loop {
        let mut noconv = false;
        for i in k..=l {
            let mut c = vnorm(&(k..=l).map(|r| b.at(r, i)).collect::<Vec<_>>());
            let mut r = vnorm(&(k..=l).map(|cc| b.at(i, cc)).collect::<Vec<_>>());
            let mut ca = (0..=l).map(|rr| abs(b.at(rr, i))).fold(0.0, f64::max);
            let mut ra = (k..n).map(|cc| abs(b.at(i, cc))).fold(0.0, f64::max);
            if c == 0.0 || r == 0.0 {
                continue;
            }
            let mut g = r / 2.0;
            let mut f: f64 = 1.0;
            let s = c + r;
            while c < g && f.max(c).max(ca) < sfmax2 && r.min(g).min(ra) > sfmin2 {
                f *= 2.0;
                c *= 2.0;
                ca *= 2.0;
                r /= 2.0;
                g /= 2.0;
                ra /= 2.0;
            }
            g = c / 2.0;
            while g >= r && r.max(ra) < sfmax2 && f.min(c).min(g).min(ca) > sfmin2 {
                f /= 2.0;
                c /= 2.0;
                g /= 2.0;
                ca /= 2.0;
                r *= 2.0;
                ra *= 2.0;
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
            scale[i] *= f;
            noconv = true;
            for cc in k..n {
                *b.rf(i, cc) /= f;
            }
            for rr in 0..=l {
                *b.rf(rr, i) *= f;
            }
            for rr in 0..n {
                *t.rf(rr, i) *= f;
            }
        }
        if !noconv {
            break;
        }
    }
    (t, b)
}

/// Givens-based rank-one update of a full QR factorization: Q1*R1 = Q*R + u*v'.
fn qr_update(q: &M, r: &M, u: &[Z], v: &[Z]) -> (M, M) {
    let (m, n) = (r.r, r.c);
    let mut q = q.clone();
    let mut r = r.clone();
    let mut w = q.ct().mul(&M::col_vec(u)).d;
    for k in (0..m.saturating_sub(1)).rev() {
        let (c, s) = givens(w[k], w[k + 1]);
        let (x, y) = (w[k], w[k + 1]);
        w[k] = x * c + s * y;
        w[k + 1] = ZERO;
        rot_rows(&mut r, k, k + 1, c, s, 0, n);
        rot_cols(&mut q, k, k + 1, c, s, 0, m);
    }
    if m > 0 {
        for (j, vj) in v.iter().enumerate().take(n) {
            *r.rf(0, j) += w[0] * vj.conj();
        }
    }
    for k in 0..n.min(m.saturating_sub(1)) {
        let (c, s) = givens(r.at(k, k), r.at(k + 1, k));
        rot_rows(&mut r, k, k + 1, c, s, 0, n);
        rot_cols(&mut q, k, k + 1, c, s, 0, m);
        r.set(k + 1, k, ZERO);
    }
    (q, r)
}

// ---- the builtins --------------------------------------------------------------------------

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match name {
        "bandwidth" | "isbanded" => structure_band(name, a, nargout),
        "issymmetric" | "ishermitian" => {
            let Some(Value::Num(x)) = a.first() else {
                return if a.is_empty() { rt("Not enough input arguments.") } else { one(Value::bool(false)) };
            };
            let skew = match opt_text(a.get(1)).as_deref() {
                None | Some("nonskew") => false,
                Some("skew") => true,
                Some(_) => return rt("Option must be 'skew' or 'nonskew'."),
            };
            if !x.is_2d() || x.rows() != x.cols() {
                return one(Value::bool(false));
            }
            let m = M::of(x);
            let herm = name == "ishermitian";
            let ok = (0..m.r).all(|j| {
                (0..m.r).all(|i| {
                    let t = if herm { m.at(j, i).conj() } else { m.at(j, i) };
                    m.at(i, j) == if skew { -t } else { t }
                })
            });
            one(Value::bool(ok))
        }
        "vecnorm" => vecnorm(a),
        "normest" => {
            let (s, _) = arg2d(a, 0, name, "Input must be 2-D.")?;
            let tol = scalar_at(a, 1).unwrap_or(1e-6);
            let mut x: Vec<Z> = (0..s.c).map(|j| re((0..s.r).map(|i| abs(s.at(i, j))).sum())).collect();
            let mut e = vnorm(&x);
            let mut cnt = 0.0;
            if e > 0.0 {
                x.iter_mut().for_each(|v| *v /= e);
                let mut e0 = 0.0;
                while (e - e0).abs() > tol * e {
                    e0 = e;
                    let mut sx = s.mul(&M::col_vec(&x));
                    if sx.d.iter().all(|v| *v == ZERO) {
                        sx.d.iter_mut().for_each(|v| *v = ONE);
                    }
                    let y = s.ct().mul(&sx);
                    let normx = vnorm(&y.d);
                    e = normx / vnorm(&sx.d);
                    x = y.d.iter().map(|v| v / normx).collect();
                    cnt += 1.0;
                    if cnt > 100.0 {
                        warn(it, format!("NORMEST did not converge for 100 iterations with tolerance {}", fmt_g(tol)));
                        break;
                    }
                }
            }
            vals(vec![Value::num(e), Value::num(cnt)])
        }
        "normest1" | "condest" => {
            let (m, _) = square(a, 0, name, "Matrix must be square.")?;
            let n = m.r;
            let target = if name == "condest" {
                match lu_solve(&m, &M::eye(n)) {
                    Some(inv) => inv,
                    None => {
                        let v = Value::Num(Matrix::col({
                            let mut v = vec![0.0; n];
                            if n > 0 {
                                v[0] = 1.0;
                            }
                            v
                        }));
                        return vals(vec![Value::num(f64::INFINITY), v]);
                    }
                }
            } else {
                m.clone()
            };
            let sums: Vec<f64> = (0..n).map(|j| (0..n).map(|i| abs(target.at(i, j))).sum()).collect();
            let j = (0..n).fold(0, |b, k| if sums[k] > sums[b] { k } else { b });
            let est = sums.get(j).copied().unwrap_or(0.0);
            if name == "condest" {
                let c = m.norm1() * est;
                let v: Vec<Z> = target.col(j).iter().map(|z| z / est).collect();
                return vals(vec![Value::num(c), zcol(&v, !v.iter().all(|z| z.im == 0.0))]);
            }
            let mut v = vec![0.0; n];
            if n > 0 {
                v[j] = 1.0;
            }
            let w = target.col(j);
            vals(vec![Value::num(est), col_vec(v), zcol(&w, !m.is_real()), col_vec(vec![1.0, n as f64])])
        }
        "rank" => {
            let (m, _) = arg2d(a, 0, name, SVD_2D)?;
            let s = singular_values(&m)?;
            one(Value::num(rank_tol(&s, m.r, m.c, scalar_at(a, 1)) as f64))
        }
        "null" => {
            let (m, c) = arg2d(a, 0, name, SVD_2D)?;
            if matches!(opt_text(a.get(1)).as_deref(), Some("r")) {
                let (r, piv) = rref_of(&m, None);
                let n = m.c;
                let nopiv: Vec<usize> = (0..n).filter(|j| !piv.contains(j)).collect();
                let mut z = M::zeros(n, nopiv.len());
                for (k, &j) in nopiv.iter().enumerate() {
                    z.set(j, k, ONE);
                    for (i, &pc) in piv.iter().enumerate() {
                        z.set(pc, k, -r.at(i, j));
                    }
                }
                return one(z.val(c));
            }
            one(null_of(&m, scalar_at(a, 1))?.val(c))
        }
        "orth" => {
            let (m, c) = arg2d(a, 0, name, SVD_2D)?;
            one(orth_of(&m, scalar_at(a, 1))?.val(c))
        }
        "rref" => {
            let (m, c) = arg2d(a, 0, name, "Input must be 2-D.")?;
            let (r, jb) = rref_of(&m, scalar_at(a, 1));
            vals(vec![r.val(c), perm_row(&jb)])
        }
        "subspace" => {
            let (x, _) = arg2d(a, 0, name, "Input must be 2-D.")?;
            let (y, _) = arg2d(a, 1, name, "Input must be 2-D.")?;
            if x.r != y.r {
                return rt("The number of rows in A and B must be the same.");
            }
            let (mut p, mut q) = (orth_of(&x, None)?, orth_of(&y, None)?);
            if p.c < q.c {
                std::mem::swap(&mut p, &mut q);
            }
            let q = q.sub(&p.mul(&p.ct().mul(&q)));
            let nrm = singular_values(&q)?.first().copied().unwrap_or(0.0);
            one(Value::num(nrm.min(1.0).asin()))
        }
        "linsolve" => linsolve(it, a, nargout),
        "rcond" => {
            let (m, _) = square(a, 0, name, "Input must be a square matrix.")?;
            one(Value::num(rcond_of(&m)))
        }
        "cond" => {
            let (m, _) = arg2d(a, 0, name, "Input must be 2-D.")?;
            let p = match a.get(1) {
                None => "2".to_string(),
                Some(v) if v.is_text() => v.text()?.to_lowercase(),
                Some(v) => {
                    let p = v.scalar()?;
                    if p == 1.0 {
                        "1".into()
                    } else if p == 2.0 {
                        "2".into()
                    } else if p.is_infinite() && p > 0.0 {
                        "inf".into()
                    } else {
                        return rt("The only matrix norms available are 1, 2, inf, and 'fro'.");
                    }
                }
            };
            if p == "2" {
                if m.r == 0 || m.c == 0 {
                    return one(Value::num(0.0));
                }
                let s = singular_values(&m)?;
                let last = *s.last().unwrap_or(&0.0);
                return one(Value::num(if last == 0.0 { f64::INFINITY } else { s[0] / last }));
            }
            if !m.is_square() {
                return rt("A is rectangular.  Use the 2 norm.");
            }
            let inv = match lu_solve(&m, &M::eye(m.r)) {
                Some(inv) => inv,
                None => {
                    warn(it, "Matrix is singular to working precision.");
                    return one(Value::num(f64::INFINITY));
                }
            };
            let nrm = |x: &M| match p.as_str() {
                "1" => Ok(x.norm1()),
                "inf" => Ok(x.norm_inf()),
                "fro" => Ok(x.norm_fro()),
                _ => rt("The only matrix norms available are 1, 2, inf, and 'fro'."),
            };
            one(Value::num(nrm(&m)? * nrm(&inv)?))
        }
        "chol" => {
            let (m, c) = arg2d(a, 0, name, "Input array has too many dimensions. The array must be 2-dimensional.")?;
            if !m.is_square() {
                return rt("Matrix must be square.");
            }
            let lower = match opt_text(a.get(1)).as_deref() {
                None | Some("upper") => false,
                Some("lower") => true,
                Some(_) => return rt("Option must be 'upper' or 'lower'."),
            };
            let src = if lower { m.tril(0).ct() } else { m.clone() };
            let (r, p) = match chol_upper(&src) {
                Ok(r) => (r, 0),
                Err((k, r)) => {
                    if nargout < 2 {
                        return rt("Matrix must be positive definite.");
                    }
                    (r, k)
                }
            };
            let r = if lower { r.ct() } else { r };
            vals(vec![r.val(c), Value::num(p as f64)])
        }
        "ldl" => {
            let (m, c) = arg2d(a, 0, name, "Matrix must be 2-D.")?;
            if !m.is_square() {
                return rt("Matrix must be square.");
            }
            let opts = texts(&a[1..]);
            let vector = opts.iter().any(|o| o == "vector");
            let upper = opts.iter().any(|o| o == "upper");
            let (l, d, p) = ldl_rook(&m, upper);
            if nargout <= 2 {
                let mut pl = M::zeros(l.r, l.c);
                for (k, &pk) in p.iter().enumerate() {
                    for j in 0..l.c {
                        pl.set(pk, j, l.at(k, j));
                    }
                }
                let pl = if upper { pl.ct() } else { pl };
                return vals(vec![pl.val(c), d.val(c)]);
            }
            let l = if upper { l.ct() } else { l };
            let pv = if vector { perm_row(&p) } else { perm_cols_matrix(&p).val(false) };
            vals(vec![l.val(c), d.val(c), pv])
        }
        "lu" => {
            let (m, c) = arg2d(a, 0, name, "The array must be 2-dimensional.")?;
            let vector = texts(&a[1..]).iter().any(|o| o == "vector");
            let (f, p) = lu_packed(&m);
            if nargout <= 1 {
                return one(f.val(c));
            }
            let (l, u) = lu_split(&f);
            if nargout == 2 {
                let mut pl = M::zeros(l.r, l.c);
                for (k, &pk) in p.iter().enumerate() {
                    for j in 0..l.c {
                        pl.set(pk, j, l.at(k, j));
                    }
                }
                return vals(vec![pl.val(c), u.val(c)]);
            }
            let pv = if vector { perm_row(&p) } else { perm_rows_matrix(&p).val(false) };
            vals(vec![l.val(c), u.val(c), pv])
        }
        "qr" => {
            let (m, c) = arg2d(a, 0, name, "Input array must be 2-dimensional.")?;
            let zero = is_num_zero(a.get(1));
            let opts = texts(&a[1..]);
            let econ = zero || opts.iter().any(|o| o == "econ");
            let vector = zero || opts.iter().any(|o| o == "vector");
            let k = m.r.min(m.c);
            let rows = if econ { k } else { m.r };
            let f = qr_fact(&m, nargout >= 3);
            let r = f.r(rows);
            if nargout <= 1 {
                return one(r.val(c));
            }
            let q = f.q(rows);
            if nargout == 2 {
                return vals(vec![q.val(c), r.val(c)]);
            }
            let pv = if vector { perm_row(&f.perm) } else { perm_cols_matrix(&f.perm).val(false) };
            vals(vec![q.val(c), r.val(c), pv])
        }
        "pinv" => {
            let (m, c) = arg2d(a, 0, name, SVD_2D)?;
            one(pinv_of(&m, scalar_at(a, 1))?.val(c))
        }
        "lsqminnorm" => {
            let (m, c1) = arg2d(a, 0, name, "Input must be 2-D.")?;
            let (b, c2) = arg2d(a, 1, name, "Input must be 2-D.")?;
            if m.r != b.r {
                return rt("Matrix dimensions must agree.");
            }
            let warn_on = texts(&a[2..]).iter().any(|o| o == "warn");
            let qr = qr_fact(&m, true);
            let k = m.r.min(m.c);
            let r00 = if k > 0 { abs(qr.f.at(0, 0)) } else { 0.0 };
            let tol = scalar_at(a, 2).unwrap_or_else(|| (m.r.max(m.c) as f64) * EPS * r00);
            let rank = (0..k).take_while(|&i| abs(qr.f.at(i, i)) > tol).count();
            if warn_on && rank < k {
                warn(it, format!("Rank deficient, rank = {rank}, tol = {}.", fmt_e(tol)));
            }
            let qb = qr.qt_mul(&b);
            let r1 = qr.r(rank);
            // Minimum-norm solution of R1 y = (Q'b)(1:rank) through the QR of R1'.
            let qr2 = qr_fact(&r1.ct(), false);
            let t = qr2.r(rank);
            let rhs = qb.block(0, rank, 0, b.c);
            let y1 = tri_solve(&t.ct(), &rhs, false);
            let mut full = M::zeros(m.c, b.c);
            full.put(0, 0, &y1);
            let mut zq = qr2.q(m.c);
            zq = zq.block(0, m.c, 0, m.c);
            let yperm = zq.mul(&full);
            let mut x = M::zeros(m.c, b.c);
            for (i, &pi) in qr.perm.iter().enumerate() {
                for j in 0..b.c {
                    x.set(pi, j, yperm.at(i, j));
                }
            }
            one(x.val(c1 || c2))
        }
        "lscov" => lscov(it, a, nargout),
        "sylvester" => {
            let (x, c1) = square(a, 0, name, "First and second inputs must be square.")?;
            let (y, c2) = square(a, 1, name, "First and second inputs must be square.")?;
            let (z, c3) = arg2d(a, 2, name, "C must be 2-D.")?;
            if z.r != x.r || z.c != y.r {
                return rt("C must be of size [size(A,1), size(B,1)].");
            }
            one(sylvester_of(&x, &y, &z)?.val(c1 || c2 || c3))
        }
        "svd" => {
            let (m, c) = arg2d(a, 0, name, "Input must be 2-D. Use PAGESVD to compute the SVD of each page of an N-D array.")?;
            if m.has_nonfinite() {
                return rt("Input to SVD must not contain NaN or Inf.");
            }
            let zero = is_num_zero(a.get(1));
            let opts = texts(&a[1..]);
            let econ = (zero && m.r > m.c) || opts.iter().any(|o| o == "econ");
            let vector = if opts.iter().any(|o| o == "vector") { true } else if opts.iter().any(|o| o == "matrix") { false } else { nargout <= 1 };
            let (u, s, v) = svd_of(&m, !c, econ || nargout <= 1)?;
            let sv = if vector {
                col_vec(s.clone())
            } else {
                let (r, cc) = if econ { (s.len(), s.len()) } else { (m.r, m.c) };
                let mut sm = M::zeros(r, cc);
                for (i, &x) in s.iter().enumerate() {
                    sm.set(i, i, re(x));
                }
                sm.val(false)
            };
            if nargout <= 1 {
                return one(sv);
            }
            let (u, v) = if econ { (u.block(0, u.r, 0, s.len()), v.block(0, v.r, 0, s.len())) } else { (u, v) };
            vals(vec![u.val(c), sv, v.val(c)])
        }
        "svds" => {
            let (m, c) = arg2d(a, 0, name, "Input must be 2-D.")?;
            let k = scalar_at(a, 1).map(|x| x as usize).unwrap_or(6).min(m.r.min(m.c));
            let (u, s, v) = svd_of(&m, !c, true)?;
            let sig = a.get(2);
            let idx: Vec<usize> = match (opt_text(sig).as_deref(), sig) {
                (Some("largest") | Some("l"), _) | (None, None) => (0..k).collect(),
                (Some("smallest") | Some("s"), _) => (s.len() - k..s.len()).collect(),
                (Some("smallestnz"), _) => {
                    let nz: Vec<usize> = (0..s.len()).filter(|&i| s[i] > (m.r.max(m.c) as f64) * eps_of(s[0])).collect();
                    nz[nz.len().saturating_sub(k)..].to_vec()
                }
                (Some(_), _) => return rt("Invalid value for sigma."),
                (None, Some(v)) => {
                    let t = v.scalar()?;
                    let mut idx: Vec<usize> = (0..s.len()).collect();
                    idx.sort_by(|&i, &j| (s[i] - t).abs().total_cmp(&(s[j] - t).abs()));
                    idx.truncate(k);
                    idx.sort();
                    idx
                }
            };
            let sv: Vec<f64> = idx.iter().map(|&i| s[i]).collect();
            if nargout <= 1 {
                return one(col_vec(sv));
            }
            let sd = M::diag(&sv.iter().map(|&x| re(x)).collect::<Vec<_>>());
            vals(vec![u.cols(&idx).val(c), sd.val(false), v.cols(&idx).val(c), Value::num(0.0)])
        }
        "eigs" => eigs(it, a, nargout),
        "gsvd" => {
            let (x, c1) = arg2d(a, 0, name, "Input must be 2-D.")?;
            let (y, c2) = arg2d(a, 1, name, "Input must be 2-D.")?;
            if x.c != y.c {
                return rt("Matrices must have the same number of columns.");
            }
            let (u, v, xm, cm, sm) = gsvd_of(&x, &y)?;
            let c = c1 || c2;
            if nargout <= 1 {
                let q = cm.c;
                let sig: Vec<f64> = (0..q)
                    .map(|j| {
                        let cc = vnorm(&cm.col(j));
                        let ss = vnorm(&sm.col(j));
                        cc / ss
                    })
                    .collect();
                return one(col_vec(sig));
            }
            if is_num_zero(a.get(2)) {
                let (ku, kv) = (x.r.min(x.c), y.r.min(y.c));
                return vals(vec![u.block(0, u.r, 0, ku).val(c), v.block(0, v.r, 0, kv).val(c), xm.val(c), cm.block(0, ku, 0, cm.c).val(false), sm.block(0, kv, 0, sm.c).val(false)]);
            }
            vals(vec![u.val(c), v.val(c), xm.val(c), cm.val(false), sm.val(false)])
        }
        "pageeig" => {
            let x = m_arg(a, 0, name)?;
            let (r, cc, np, rest) = pages(&x);
            if r != cc {
                return rt("Input must be square in the first two dimensions.");
            }
            let vector = match opt_text(a.get(1)).as_deref() {
                Some("vector") => true,
                Some("matrix") => false,
                _ => nargout <= 1,
            };
            let mut vs = Vec::new();
            let mut ds = Vec::new();
            let mut cplx = x.is_complex();
            for p in 0..np {
                let m = page(&x, r, r, p);
                let (e, v) = if is_herm(&m) {
                    let (e, v) = eig_herm(&m)?;
                    (e.into_iter().map(re).collect::<Vec<_>>(), v)
                } else {
                    let (e, v) = eig_general(&m)?;
                    cplx |= e.iter().any(|z| z.im != 0.0) || !v.is_real();
                    (e, v)
                };
                ds.push(if vector { M::col_vec(&e) } else { M::diag(&e) });
                vs.push(v);
            }
            if nargout <= 1 {
                return one(stack_pages(&ds, &rest, cplx));
            }
            vals(vec![stack_pages(&vs, &rest, cplx), stack_pages(&ds, &rest, cplx)])
        }
        "pagesvd" => {
            let x = m_arg(a, 0, name)?;
            let (r, cc, np, rest) = pages(&x);
            let opts = texts(&a[1..]);
            let econ = opts.iter().any(|o| o == "econ");
            let vector = if opts.iter().any(|o| o == "vector") { true } else if opts.iter().any(|o| o == "matrix") { false } else { nargout <= 1 };
            let c = x.is_complex();
            let (mut us, mut ss, mut vs) = (Vec::new(), Vec::new(), Vec::new());
            for p in 0..np {
                let m = page(&x, r, cc, p);
                let (u, s, v) = svd_of(&m, !c, econ || nargout <= 1)?;
                let k = s.len();
                ss.push(if vector {
                    M::col_vec(&s.iter().map(|&x| re(x)).collect::<Vec<_>>())
                } else {
                    let (rr, c2) = if econ { (k, k) } else { (r, cc) };
                    let mut sm = M::zeros(rr, c2);
                    for (i, &x) in s.iter().enumerate() {
                        sm.set(i, i, re(x));
                    }
                    sm
                });
                us.push(if econ { u.block(0, u.r, 0, k) } else { u });
                vs.push(if econ { v.block(0, v.r, 0, k) } else { v });
            }
            if nargout <= 1 {
                return one(stack_pages(&ss, &rest, false));
            }
            vals(vec![stack_pages(&us, &rest, c), stack_pages(&ss, &rest, false), stack_pages(&vs, &rest, c)])
        }
        "poly" => {
            let x = m_arg(a, 0, name)?;
            let m = M::of(&x);
            let real_matrix;
            let e: Vec<Z> = if x.is_empty() {
                real_matrix = true;
                Vec::new()
            } else if x.is_vector() {
                real_matrix = false;
                m.d.clone()
            } else if m.is_square() {
                real_matrix = m.is_real();
                eigvals(&m)?
            } else {
                return rt("Argument must be a vector or a square matrix.");
            };
            let n = e.len();
            let mut c = vec![ZERO; n + 1];
            c[0] = ONE;
            for (j, &ej) in e.iter().enumerate() {
                for i in (1..=j + 1).rev() {
                    let v = c[i] - ej * c[i - 1];
                    c[i] = v;
                }
            }
            let mut pos: Vec<Z> = e.iter().filter(|z| z.im > 0.0).copied().collect();
            let mut neg: Vec<Z> = e.iter().filter(|z| z.im < 0.0).map(|z| z.conj()).collect();
            let key = |a: &Z, b: &Z| a.re.total_cmp(&b.re).then(a.im.total_cmp(&b.im));
            pos.sort_by(key);
            neg.sort_by(key);
            let real = real_matrix || pos == neg;
            one(M { r: 1, c: n + 1, d: c }.val(!real))
        }
        "polyeig" => polyeig(a, nargout),
        "condeig" => {
            let (m, _) = square(a, 0, name, "Input matrix must be square.")?;
            let (e, v) = eig_general(&m)?;
            let w = lu_solve(&v, &M::eye(m.r));
            let n = m.r;
            let s: Vec<f64> = (0..n)
                .map(|i| match &w {
                    Some(w) => vnorm(&(0..n).map(|j| w.at(i, j)).collect::<Vec<_>>()) * vnorm(&v.col(i)),
                    None => f64::INFINITY,
                })
                .collect();
            if nargout <= 1 {
                return one(col_vec(s));
            }
            let cplx = !v.is_real() || e.iter().any(|z| z.im != 0.0);
            vals(vec![v.val(cplx), M::diag(&e).val(cplx), col_vec(s)])
        }
        "hess" => {
            if a.len() >= 2 {
                let (x, c1) = square(a, 0, name, "Inputs must be dense, square matrices of type double or single.")?;
                let (y, c2) = square(a, 1, name, "Inputs must be dense, square matrices of type double or single.")?;
                if x.r != y.r {
                    return rt("Matrix dimensions must agree.");
                }
                let (aa, bb, q, z) = gen_hess(&x, &y, c1 || c2);
                let c = c1 || c2;
                return vals(vec![aa.val(c), bb.val(c), q.val(c), z.val(c)]);
            }
            let (m, c) = square(a, 0, name, "Inputs must be dense, square matrices of type double or single.")?;
            let (p, h) = hessenberg(&m);
            if nargout <= 1 {
                return one(h.val(c));
            }
            vals(vec![p.val(c), h.val(c)])
        }
        "schur" => {
            let (m, c) = square(a, 0, name, "First input must be square.")?;
            let complex = match opt_text(a.get(1)).as_deref() {
                None | Some("real") => false,
                Some("complex") => true,
                Some(_) => return rt("Second input must be 'real' or 'complex'."),
            };
            let (u, t) = schur_of(&m, !c, complex)?;
            let oc = c || complex;
            if nargout <= 1 {
                return one(t.val(oc));
            }
            vals(vec![u.val(oc), t.val(oc)])
        }
        "qz" => {
            let (x, c1) = square(a, 0, name, "Matrix must be square.")?;
            let (y, c2) = square(a, 1, name, "Matrix must be square.")?;
            if x.r != y.r {
                return rt("Matrix dimensions must agree.");
            }
            let real = match opt_text(a.get(2)).as_deref() {
                None | Some("complex") => false,
                Some("real") => true,
                Some(_) => return rt("Flag must be 'real' or 'complex'."),
            };
            let real = real && !c1 && !c2;
            let (aa, bb, q, z) = qz_of(&x, &y, real)?;
            let c = !real;
            let mut out = vec![aa.val(c), bb.val(c), q.val(c), z.val(c)];
            if nargout > 4 {
                let lam = ordeig_pencil(&aa, &bb);
                let v = pencil_vectors(&x, &y, &lam, false)?;
                let w = pencil_vectors(&x, &y, &lam, true)?;
                let cv = !v.is_real() || !w.is_real();
                out.push(v.val(cv));
                out.push(w.val(cv));
            }
            vals(out)
        }
        "ordschur" => {
            let (u, c1) = square(a, 0, name, "U must be square.")?;
            let (t, c2) = square(a, 1, name, "T must be square.")?;
            let sel = arg_or(a, 2, name)?;
            let real = !c1 && !c2;
            let ids = selection(sel, &ordeig_std(&t), name)?;
            let (mut uu, mut tt) = (u.clone(), t.clone());
            let mut q = uu.ct();
            reorder(&mut tt, None, &mut q, &mut uu, &ids, real);
            vals(vec![uu.val(!real), tt.val(!real)])
        }
        "ordqz" => {
            let mut ms = Vec::new();
            let mut cplx = false;
            for k in 0..4 {
                let (m, c) = square(a, k, name, "Inputs must be square matrices.")?;
                cplx |= c;
                ms.push(m);
            }
            let sel = arg_or(a, 4, name)?;
            let ids = selection(sel, &ordeig_pencil(&ms[0], &ms[1]), name)?;
            let (mut aa, mut bb, mut q, mut z) = (ms[0].clone(), ms[1].clone(), ms[2].clone(), ms[3].clone());
            reorder(&mut aa, Some(&mut bb), &mut q, &mut z, &ids, !cplx);
            vals(vec![aa.val(cplx), bb.val(cplx), q.val(cplx), z.val(cplx)])
        }
        "ordeig" => {
            let (t, _) = square(a, 0, name, "Input must be square.")?;
            let e = if a.len() >= 2 {
                let (b, _) = square(a, 1, name, "Input must be square.")?;
                ordeig_pencil(&t, &b)
            } else {
                ordeig_std(&t)
            };
            one(zcol(&e, e.iter().any(|z| z.im != 0.0)))
        }
        "expm" => {
            let (m, c) = square(a, 0, name, "Input must be square.")?;
            one(expm_of(&m).val(c))
        }
        "sqrtm" => {
            let (m, c) = square(a, 0, name, "Input matrix must be square.")?;
            let (u, t) = cschur(&m)?;
            if (0..t.r).any(|i| t.at(i, i) == ZERO) {
                warn(it, "Matrix is singular and may not have a square root.");
            }
            let x = u.mul(&sqrtm_tri(&t)).mul(&u.ct());
            let neg = (0..t.r).any(|i| t.at(i, i).im == 0.0 && t.at(i, i).re < 0.0);
            vals(vec![if !c && !neg { x.val(false) } else { x.val(true) }, Value::num(0.0)])
        }
        "logm" => {
            let (m, c) = square(a, 0, name, "Input matrix must be square.")?;
            let (u, t) = cschur(&m)?;
            let nonpos = has_nonpos_real_eig(&t);
            if nonpos {
                warn(it, "Principal matrix logarithm is not defined for A with nonpositive real eigenvalues. A non-principal matrix logarithm is returned.");
            }
            let (l, capped) = logm_tri(&t);
            if capped {
                warn(it, "Maximum number of matrix square roots exceeded. Results may be inaccurate.");
            }
            let x = u.mul(&l).mul(&u.ct());
            vals(vec![if !c && !nonpos { x.val(false) } else { x.val(true) }, Value::num(0.0)])
        }
        "funm" => {
            let (m, c) = square(a, 0, name, "Input must be square.")?;
            let f = arg_or(a, 1, name)?.clone();
            if !matches!(f, Value::Func(_)) && !f.is_text() {
                return rt("FUN must be a function handle.");
            }
            let x = funm_of(it, &m, &f)?;
            let real = !c && {
                let imn: f64 = (0..x.c).map(|j| (0..x.r).map(|i| x.at(i, j).im.abs()).sum::<f64>()).fold(0.0, f64::max);
                imn <= 10.0 * (x.r as f64) * EPS * x.norm1()
            };
            vals(vec![x.val(!real), Value::num(0.0)])
        }
        "qrdelete" | "qrinsert" => qr_modify(name, a),
        "qrupdate" => {
            let (q, c1) = arg2d(a, 0, name, "Q must be 2-D.")?;
            let (r, c2) = arg2d(a, 1, name, "R must be 2-D.")?;
            let (u, c3) = arg2d(a, 2, name, "U must be 2-D.")?;
            let (v, c4) = arg2d(a, 3, name, "V must be 2-D.")?;
            if !q.is_square() || q.c != r.r || u.d.len() != q.r || v.d.len() != r.c {
                return rt("Inputs must have compatible dimensions.");
            }
            let (q1, r1) = qr_update(&q, &r, &u.d, &v.d);
            let c = c1 || c2 || c3 || c4;
            vals(vec![q1.val(c), r1.val(c)])
        }
        "cholupdate" => cholupdate(a, nargout),
        "rsf2csf" => {
            let (u, _) = square(a, 0, name, "U must be square.")?;
            let (t, _) = square(a, 1, name, "T must be square.")?;
            let (u2, t2) = rsf2csf(&u, &t);
            vals(vec![u2.val(true), t2.val(true)])
        }
        "cdf2rdf" => {
            let (v, _) = arg2d(a, 0, name, "Input must be 2-D.")?;
            let (d, _) = square(a, 1, name, "D must be square.")?;
            let n = d.r;
            let (mut vr, mut dr) = (v.clone(), d.clone());
            let mut i = 0;
            while i < n {
                let lam = d.at(i, i);
                if lam.im != 0.0 {
                    if i + 1 >= n || d.at(i + 1, i + 1) != lam.conj() {
                        return rt("The diagonal of D must be a collection of real eigenvalues and complex conjugate pairs (like the output of EIG(X) when X is a real matrix).");
                    }
                    dr.set(i, i, re(lam.re));
                    dr.set(i, i + 1, re(lam.im));
                    dr.set(i + 1, i, re(-lam.im));
                    dr.set(i + 1, i + 1, re(lam.re));
                    let h = std::f64::consts::FRAC_1_SQRT_2;
                    for r in 0..v.r {
                        let (x, y) = (v.at(r, i), v.at(r, i + 1));
                        vr.set(r, i, (x + y) * h);
                        vr.set(r, i + 1, dv(x - y, Z::new(0.0, 1.0)) * h);
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            vals(vec![vr.auto(), dr.val(false)])
        }
        "balance" => {
            let (m, c) = square(a, 0, name, "Matrix must be square.")?;
            let noperm = matches!(opt_text(a.get(1)).as_deref(), Some("noperm"));
            let (t, b) = balance_of(&m, !noperm);
            match nargout {
                0 | 1 => one(b.val(c)),
                2 => vals(vec![t.val(false), b.val(c)]),
                _ => {
                    let n = m.r;
                    let mut s = vec![0.0; n];
                    let mut p = vec![0usize; n];
                    for k in 0..n {
                        for j in 0..n {
                            if t.at(k, j) != ZERO {
                                p[k] = j;
                                s[k] = t.at(k, j).re;
                            }
                        }
                    }
                    vals(vec![col_vec(s), perm_col(&p), b.val(c)])
                }
            }
        }
        "planerot" => {
            let (x, c) = arg2d(a, 0, name, "Input must be 2-D.")?;
            if x.r != 2 || x.c != 1 {
                return rt("First input must be a column vector of length 2.");
            }
            let r = planerot(x.at(0, 0), x.at(1, 0));
            let g = M { r: 2, c: 2, d: vec![r[0], r[2], r[1], r[3]] };
            let y = g.mul(&x);
            vals(vec![g.val(c), y.val(c)])
        }
        _ => Ok(None),
    }
}

fn arg_or<'a>(a: &'a [Value], i: usize, name: &str) -> R<&'a Value> {
    a.get(i).ok_or_else(|| LangError::new(format!("Not enough input arguments for '{name}'."), 0))
}

fn fmt_g(x: f64) -> String {
    crate::display::fmt_g(x, 5)
}

fn structure_band(name: &str, a: &[Value], nargout: usize) -> Ret {
    let x = m_arg(a, 0, name)?;
    if !x.is_2d() {
        return rt("Input must be a 2-D array.");
    }
    let m = M::of(&x);
    let (mut lo, mut up) = (0usize, 0usize);
    for j in 0..m.c {
        for i in 0..m.r {
            if m.at(i, j) != ZERO {
                if i > j {
                    lo = lo.max(i - j);
                } else {
                    up = up.max(j - i);
                }
            }
        }
    }
    if name == "isbanded" {
        let bound = |k: usize| -> R<f64> {
            match scalar_at(a, k) {
                Some(v) if v >= 0.0 && v.fract() == 0.0 => Ok(v),
                _ => rt("Second and third inputs must be nonnegative integer scalars."),
            }
        };
        let (l, u) = (bound(1)?, bound(2)?);
        return one(Value::bool(lo as f64 <= l && up as f64 <= u));
    }
    match opt_text(a.get(1)).as_deref() {
        Some("lower") => one(Value::num(lo as f64)),
        Some("upper") => one(Value::num(up as f64)),
        Some(_) => rt("Option must be 'lower' or 'upper'."),
        None => {
            let _ = nargout;
            vals(vec![Value::num(lo as f64), Value::num(up as f64)])
        }
    }
}

fn vecnorm(a: &[Value]) -> Ret {
    let x = m_arg(a, 0, "vecnorm")?;
    let p = match a.get(1) {
        None => 2.0,
        Some(Value::Num(m)) if m.is_empty() => 2.0,
        Some(v) => v.scalar()?,
    };
    if p.is_nan() || p <= 0.0 {
        return rt("Second argument must be a positive real value or Inf.");
    }
    let dim = match a.get(2) {
        Some(v) => {
            let d = v.scalar()?;
            if d < 1.0 || d.fract() != 0.0 {
                return rt("Dimension argument must be a positive integer scalar within indexing range.");
            }
            d as usize
        }
        None => default_dim(&x.dims),
    };
    let (_, sl) = crate::builtins::slices(&x.dims, dim);
    let mut dims = x.dims.clone();
    if dim <= dims.len() {
        dims[dim - 1] = 1;
    }
    let out: Vec<f64> = sl
        .iter()
        .map(|idx| {
            let mags = idx.iter().map(|&i| x.data[i].hypot(x.im_at(i)));
            if p.is_infinite() {
                mags.fold(0.0, nanmax)
            } else if p == 1.0 {
                mags.sum()
            } else if p == 2.0 {
                mags.map(|v| v * v).sum::<f64>().sqrt()
            } else {
                mags.map(|v| v.powf(p)).sum::<f64>().powf(1.0 / p)
            }
        })
        .collect();
    one(Value::Num(Matrix::from_dims(dims, out)))
}

fn linsolve(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let (mut m, c1) = arg2d(a, 0, "linsolve", "Input must be 2-D.")?;
    let (b, c2) = arg2d(a, 1, "linsolve", "Input must be 2-D.")?;
    let opts = a.get(2);
    if let Some(o) = opts {
        if !matches!(o, Value::Struct(_)) {
            return rt("Third argument must be a structure array.");
        }
    }
    let flag = |f: &str| opts.is_some_and(|o| struct_flag(o, f));
    if flag("TRANSA") {
        m = m.ct();
    }
    if m.r != b.r {
        return rt("Matrix dimensions must agree.");
    }
    let c = c1 || c2;
    let (lt, ut) = (flag("LT"), flag("UT"));
    if lt || ut {
        if !m.is_square() {
            return rt("Matrix must be square for triangular solves.");
        }
        // With TRANSA the stored triangle flips.
        let upper = if flag("TRANSA") { lt } else { ut };
        let x = tri_solve(&m, &b, upper);
        let rc = tri_rcond(&m, upper);
        if nargout < 2 {
            if rc == 0.0 {
                warn(it, "Matrix is singular to working precision.");
            } else if rc < EPS {
                warn(it, format!("Matrix is close to singular or badly scaled. Results may be inaccurate. RCOND = {}.", fmt_e(rc)));
            }
        }
        return vals(vec![x.val(c), Value::num(rc)]);
    }
    if m.is_square() {
        let rc = rcond_exact(&m);
        let x = if nargout >= 2 { lu_solve(&m, &b).unwrap_or_else(|| singular_solution(&m, &b)) } else { solve_square(it, &m, &b) };
        return vals(vec![x.val(c), Value::num(rc)]);
    }
    let (x, rank) = if nargout >= 2 { lstsq(None, &m, &b) } else { lstsq(Some(it), &m, &b) };
    vals(vec![x.val(c), Value::num(rank as f64)])
}

fn lscov(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let (m, c1) = arg2d(a, 0, "lscov", "Input must be 2-D.")?;
    let (b, c2) = arg2d(a, 1, "lscov", "Input must be 2-D.")?;
    if m.r != b.r {
        return rt("B must have the same number of rows as A.");
    }
    let (rows, cols) = (m.r, m.c);
    let (aw, bw) = match a.get(2) {
        None => (m.clone(), b.clone()),
        Some(v) => {
            let w = M::of(&v.to_matrix()?);
            if w.d.len() == rows && (w.r == 1 || w.c == 1) {
                if w.d.iter().any(|x| x.re < 0.0 || x.im != 0.0) {
                    return rt("W must be a vector of nonnegative weights.");
                }
                let s: Vec<f64> = w.d.iter().map(|x| x.re.sqrt()).collect();
                (M::from_fn(rows, cols, |i, j| m.at(i, j) * s[i]), M::from_fn(rows, b.c, |i, j| b.at(i, j) * s[i]))
            } else if w.r == rows && w.c == rows {
                let t = match chol_upper(&w) {
                    Ok(r) => r.ct(),
                    Err(_) => return rt("V must be a symmetric positive definite matrix."),
                };
                (tri_solve(&t, &m, false), tri_solve(&t, &b, false))
            } else {
                return rt("The third input must be a weight vector or a covariance matrix.");
            }
        }
    };
    let (x, _) = lstsq(Some(it), &aw, &bw);
    let c = c1 || c2;
    if nargout <= 1 {
        return one(x.val(c));
    }
    let res = bw.sub(&aw.mul(&x));
    let dof = rows.saturating_sub(cols) as f64;
    let mse: Vec<f64> = (0..b.c).map(|j| res.col(j).iter().map(|z| z.norm_sqr()).sum::<f64>() / dof).collect();
    let qr = qr_fact(&aw, false);
    let r = qr.r(cols);
    let rinv = tri_solve(&r, &M::eye(cols), true);
    let cov = rinv.mul(&rinv.ct());
    let stdx = M::from_fn(cols, b.c, |i, j| re((cov.at(i, i).re * mse[j]).sqrt()));
    let s = cov.scale(re(mse.first().copied().unwrap_or(0.0)));
    vals(vec![x.val(c), stdx.val(false), row_vec(mse), s.val(c)])
}

fn eigs(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let _ = it;
    let (m, c) = square(a, 0, "eigs", "First argument must be a square matrix or a function handle.")?;
    let mut rest = &a[1..];
    let mut bmat: Option<M> = None;
    if let Some(Value::Num(x)) = rest.first() {
        if x.kind != Kind::Char && x.numel() > 1 && x.rows() == m.r && x.cols() == m.r {
            bmat = Some(M::of(x));
            rest = &rest[1..];
        }
    }
    let k = match rest.first() {
        Some(Value::Num(x)) if x.kind != Kind::Char && x.numel() == 1 => x.data[0] as usize,
        Some(Value::Num(x)) if x.kind != Kind::Char && x.is_empty() => 6,
        _ => 6,
    }
    .min(m.r);
    if !rest.is_empty() && matches!(rest.first(), Some(Value::Num(x)) if x.kind != Kind::Char) {
        rest = &rest[1..];
    }
    let (sigma, key) = match rest.first() {
        Some(v) if v.is_text() => (None, sigma_key(&v.text()?.to_lowercase())?),
        Some(Value::Num(x)) if x.numel() == 1 => (Some(Z::new(x.data[0], x.im_at(0))), "la"),
        _ => (None, "la"),
    };
    let (lam, vecs) = match &bmat {
        None if is_herm(&m) => {
            let (e, v) = eig_herm(&m)?;
            (e.into_iter().map(re).collect::<Vec<_>>(), v)
        }
        None => eig_general(&m)?,
        Some(b) => {
            let bm = lu_solve(b, &m).ok_or_else(|| LangError::new("B must be nonsingular.", 0))?;
            eig_general(&bm)?
        }
    };
    let idx = pick(&lam, k, sigma, key);
    let d: Vec<Z> = idx.iter().map(|&i| lam[i]).collect();
    let cplx = c || d.iter().any(|z| z.im != 0.0);
    if nargout <= 1 {
        return one(zcol(&d, cplx));
    }
    let v = vecs.cols(&idx);
    let cv = cplx || !v.is_real();
    vals(vec![v.val(cv), M::diag(&d).val(cplx), Value::num(0.0)])
}

#[allow(clippy::needless_range_loop)]
fn polyeig(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    let mut cs = Vec::new();
    let mut cplx = false;
    for k in 0..a.len() {
        let (m, c) = square(a, k, "polyeig", "Inputs must be square matrices of the same size.")?;
        cplx |= c;
        cs.push(m);
    }
    let n = cs[0].r;
    if cs.iter().any(|m| m.r != n) {
        return rt("Inputs must be square matrices of the same size.");
    }
    let p = cs.len() - 1;
    if p == 0 {
        let (e, v) = eig_general(&cs[0])?;
        let ce = cplx || e.iter().any(|z| z.im != 0.0);
        if nargout <= 1 {
            return one(zcol(&e, ce));
        }
        return vals(vec![v.val(ce || !v.is_real()), zcol(&e, ce), col_vec(vec![1.0; n])]);
    }
    let np = n * p;
    let mut cm = M::zeros(np, np);
    let mut dm = M::zeros(np, np);
    cm.put(0, 0, &cs[0]);
    for (k, ck) in cs.iter().enumerate().skip(1) {
        dm.put(0, (k - 1) * n, &ck.scale(re(-1.0)));
    }
    for k in 1..p {
        cm.put(k * n, k * n, &M::eye(n));
        dm.put(k * n, (k - 1) * n, &M::eye(n));
    }
    let fail = |_| LangError::new("QZ did not converge.", 0);
    let ge = cm.faer().generalized_eigen(dm.faer()).map_err(fail)?;
    let mut e = Vec::with_capacity(np);
    for i in 0..np {
        let (al, be) = (ge.S_a().column_vector()[i], ge.S_b().column_vector()[i]);
        e.push(if be == ZERO { if al == ZERO { re(f64::NAN) } else { re(f64::INFINITY) } } else { dv(al, be) });
    }
    let mut z = M::of_faer(ge.U());
    let real_coeffs = !cplx;
    // Conjugate pairs of a real problem come positive imaginary part first, as from LAPACK.
    if real_coeffs {
        let mut j = 0;
        while j + 1 < np {
            if e[j].im < 0.0 && abs(e[j + 1] - e[j].conj()) <= 1e-10 * abs(e[j]) {
                e.swap(j, j + 1);
                for i in 0..np {
                    let t = z.at(i, j);
                    z.set(i, j, z.at(i, j + 1));
                    z.set(i, j + 1, t);
                }
                j += 2;
            } else {
                j += 1;
            }
        }
    }
    let ce = !real_coeffs || e.iter().any(|z| z.im.abs() > 0.0);
    if nargout <= 1 {
        return one(zcol(&e, ce));
    }
    let mut x = M::zeros(n, np);
    for j in 0..np {
        let lam = e[j];
        let blk = if abs(lam) > 1.0 { p - 1 } else { 0 };
        let mut col: Vec<Z> = (0..n).map(|i| z.at(blk * n + i, j)).collect();
        let nrm = vnorm(&col);
        if nrm > 0.0 {
            col.iter_mut().for_each(|v| *v /= nrm);
        }
        // Real eigenvectors for real eigenvalues of real problems.
        if real_coeffs && lam.im == 0.0 {
            let k = (0..n).fold(0, |b, i| if abs(col[i]) > abs(col[b]) { i } else { b });
            if abs(col[k]) > 0.0 {
                let ph = dv(re(abs(col[k])), col[k]);
                col.iter_mut().for_each(|v| *v *= ph);
            }
        }
        for i in 0..n {
            x.set(i, j, col[i]);
        }
    }
    let cx = ce || !x.is_real();
    let mut out = vec![x.val(cx), zcol(&e, ce)];
    if nargout >= 3 {
        // Homogeneous (alpha, beta) form with Frobenius norms (Dedieu-Tisseur).
        let norms: Vec<f64> = cs.iter().map(|m| m.norm_fro()).collect();
        let pw = |z: Z, k: usize| (0..k).fold(ONE, |acc, _| acc * z);
        let mut s = Vec::with_capacity(np);
        for j in 0..np {
            let lam = e[j];
            let (al, be) = if lam.re.is_finite() && lam.im.is_finite() {
                let h = (1.0 + lam.norm_sqr()).sqrt();
                (lam / h, re(1.0 / h))
            } else {
                (ONE, ZERO)
            };
            let mut pm = M::zeros(n, n);
            let mut da = M::zeros(n, n);
            let mut db = M::zeros(n, n);
            let mut num = 0.0;
            for (i, ci) in cs.iter().enumerate() {
                pm = pm.add(&ci.scale(pw(al, i) * pw(be, p - i)));
                if i >= 1 {
                    da = da.add(&ci.scale(pw(al, i - 1) * pw(be, p - i) * i as f64));
                }
                if i < p {
                    db = db.add(&ci.scale(pw(al, i) * pw(be, p - i - 1) * (p - i) as f64));
                }
                num += abs(al).powi(2 * i as i32) * abs(be).powi(2 * (p - i) as i32) * norms[i] * norms[i];
            }
            let (u, _, _) = svd_of(&pm, false, false)?;
            let y = u.col(n - 1);
            let xc = x.col(j);
            let dm = da.scale(be.conj()).sub(&db.scale(al.conj()));
            let dx = dm.mul(&M::col_vec(&xc));
            let yd = (0..n).fold(ZERO, |acc, i| acc + y[i].conj() * dx.at(i, 0));
            s.push(num.sqrt() / abs(yd));
        }
        out.push(col_vec(s));
    }
    vals(out)
}

/// Hessenberg-triangular reduction: (AA, BB, Q, Z) with Q*A*Z = AA, Q*B*Z = BB.
fn gen_hess(a: &M, b: &M, cplx: bool) -> (M, M, M, M) {
    use faer::dyn_stack::{MemBuffer, MemStack};
    use faer::linalg::gevd::gen_hessenberg as gh;
    let n = a.r;
    let qr = qr_fact(b, false);
    let a1 = qr.qt_mul(a);
    let b1 = qr.r(n);
    let q1 = qr.q(n);
    let par = faer::Par::Seq;
    let (mut aa, mut bb, q, z);
    if cplx {
        let hp = <gh::GeneralizedHessenbergParams as faer::Auto<Z>>::auto();
        let (mut fa, mut fb, mut fq) = (a1.faer(), b1.faer(), q1.faer());
        let mut fz = faer::Mat::<Z>::identity(n, n);
        let mut buf = MemBuffer::new(gh::generalized_hessenberg_scratch::<Z>(n, hp));
        gh::generalized_hessenberg(fa.as_mut(), fb.as_mut(), Some(fq.as_mut()), Some(fz.as_mut()), par, MemStack::new(&mut buf), hp);
        aa = M::of_faer(fa.as_ref());
        bb = M::of_faer(fb.as_ref());
        q = M::of_faer(fq.as_ref()).ct();
        z = M::of_faer(fz.as_ref());
    } else {
        let (mut fa, mut fb, mut fq) = (a1.faer_re(), b1.faer_re(), q1.faer_re());
        let hp = <gh::GeneralizedHessenbergParams as faer::Auto<f64>>::auto();
        let mut fz = faer::Mat::<f64>::identity(n, n);
        let mut buf = MemBuffer::new(gh::generalized_hessenberg_scratch::<f64>(n, hp));
        gh::generalized_hessenberg(fa.as_mut(), fb.as_mut(), Some(fq.as_mut()), Some(fz.as_mut()), par, MemStack::new(&mut buf), hp);
        aa = M::of_faer_re(fa.as_ref());
        bb = M::of_faer_re(fb.as_ref());
        q = M::of_faer_re(fq.as_ref()).ct();
        z = M::of_faer_re(fz.as_ref());
    }
    for j in 0..n {
        for i in j + 1..n {
            bb.set(i, j, ZERO);
            if i > j + 1 {
                aa.set(i, j, ZERO);
            }
        }
    }
    (aa, bb, q, z)
}

fn qr_modify(name: &str, a: &[Value]) -> Ret {
    let (mut q, c1) = arg2d(a, 0, name, "Q must be 2-D.")?;
    let (mut r, c2) = arg2d(a, 1, name, "R must be 2-D.")?;
    let j = arg_or(a, 2, name)?.scalar()?;
    let insert = name == "qrinsert";
    let (x, c3) = if insert { arg2d(a, 3, name, "X must be 2-D.")? } else { (M::zeros(0, 0), false) };
    let orient = opt_text(a.get(if insert { 4 } else { 3 })).unwrap_or_else(|| "col".into());
    if !q.is_square() || q.c != r.r {
        return rt("Q must be square and its size must match the number of rows of R.");
    }
    let (m, n) = (r.r, r.c);
    if j < 1.0 || j.fract() != 0.0 {
        return rt("J must be a positive integer.");
    }
    let j = j as usize - 1;
    let c = c1 || c2 || c3;
    match (insert, orient.as_str()) {
        (false, "col") => {
            if j >= n {
                return rt("Deletion index exceeds matrix dimensions.");
            }
            let keep: Vec<usize> = (0..n).filter(|&k| k != j).collect();
            r = r.cols(&keep);
            for k in j..(n - 1).min(m.saturating_sub(1)) {
                let g = rot_classic(r.at(k, k), r.at(k + 1, k));
                g_rows(&mut r, k, &g, 0, n - 1);
                g_cols(&mut q, k, &g, 0, m);
                r.set(k + 1, k, ZERO);
            }
        }
        (false, "row") => {
            if j >= m {
                return rt("Deletion index exceeds matrix dimensions.");
            }
            for k in (0..m - 1).rev() {
                let g = rot_classic(q.at(j, k).conj(), q.at(j, k + 1).conj());
                g_rows(&mut r, k, &g, 0, n);
                g_cols(&mut q, k, &g, 0, m);
            }
            let rows: Vec<usize> = (0..m).filter(|&k| k != j).collect();
            q = q.rows(&rows).block(0, m - 1, 1, m);
            r = r.block(1, m, 0, n);
        }
        (true, "col") => {
            if j > n {
                return rt("J must be an integer between 1 and the number of columns of R plus 1.");
            }
            if x.d.len() != m {
                return rt("X must have as many elements as R has rows.");
            }
            let w = q.ct().mul(&M::col_vec(&x.d));
            let mut r1 = M::zeros(m, n + 1);
            r1.put(0, 0, &r.block(0, m, 0, j));
            r1.put(0, j, &w);
            r1.put(0, j + 1, &r.block(0, m, j, n));
            for k in (j..m.saturating_sub(1)).rev() {
                let g = rot_classic(r1.at(k, j), r1.at(k + 1, j));
                g_rows(&mut r1, k, &g, j, n + 1);
                g_cols(&mut q, k, &g, 0, m);
                r1.set(k + 1, j, ZERO);
            }
            r = r1;
        }
        (true, "row") => {
            if j > m {
                return rt("J must be an integer between 1 and the number of rows of R plus 1.");
            }
            if x.d.len() != n {
                return rt("X must have as many elements as R has columns.");
            }
            let mut r1 = M::zeros(m + 1, n);
            for (k, v) in x.d.iter().enumerate() {
                r1.set(0, k, *v);
            }
            r1.put(1, 0, &r);
            let mut q1 = M::zeros(m + 1, m + 1);
            q1.set(0, 0, ONE);
            q1.put(1, 1, &q);
            let order: Vec<usize> = (1..=j).chain(std::iter::once(0)).chain(j + 1..=m).collect();
            let mut q1 = q1.rows(&order);
            for k in 0..n.min(m) {
                let g = rot_classic(r1.at(k, k), r1.at(k + 1, k));
                g_rows(&mut r1, k, &g, 0, n);
                g_cols(&mut q1, k, &g, 0, m + 1);
                r1.set(k + 1, k, ZERO);
            }
            q = q1;
            r = r1;
        }
        _ => return rt("ORIENT must be 'col' or 'row'."),
    }
    vals(vec![q.val(c), r.val(c)])
}

#[allow(clippy::needless_range_loop)]
fn cholupdate(a: &[Value], nargout: usize) -> Ret {
    let (r0, c1) = square(a, 0, "cholupdate", "R must be square.")?;
    let (x0, c2) = arg2d(a, 1, "cholupdate", "X must be 2-D.")?;
    let n = r0.r;
    if x0.d.len() != n || (x0.r != 1 && x0.c != 1) {
        return rt("X must be a vector of length size(R,1).");
    }
    let minus = match opt_text(a.get(2)).as_deref() {
        None | Some("+") => false,
        Some("-") => true,
        Some(_) => return rt("OP must be '+' or '-'."),
    };
    let c = c1 || c2;
    let mut r = r0.triu(0);
    let x = x0.d.clone();
    if !minus {
        let mut w: Vec<Z> = x.iter().map(|v| v.conj()).collect();
        for k in 0..n {
            let (cc, s) = givens(r.at(k, k), w[k]);
            for j in k..n {
                let (p, q) = (r.at(k, j), w[j]);
                r.set(k, j, p * cc + s * q);
                w[j] = -s.conj() * p + q * cc;
            }
        }
        return vals(vec![r.val(c), Value::num(0.0)]);
    }
    // Downdate (LINPACK chdd): R1'R1 = R'R - x x'.
    if (0..n).any(|i| r.at(i, i) == ZERO) {
        if nargout >= 2 {
            return vals(vec![r0.val(c), Value::num(2.0)]);
        }
        return rt("R must be nonsingular.");
    }
    let av = tri_solve(&r.ct(), &M::col_vec(&x), false).d;
    let nrm = vnorm(&av);
    if nrm >= 1.0 {
        if nargout >= 2 {
            return vals(vec![r0.val(c), Value::num(1.0)]);
        }
        return rt("Downdated matrix must be positive definite.");
    }
    let mut alpha = (1.0 - nrm * nrm).sqrt();
    let mut cs = vec![0.0; n];
    let mut sn = vec![ZERO; n];
    for i in (0..n).rev() {
        let scale = alpha + abs(av[i]);
        let aa = alpha / scale;
        let b = av[i] / scale;
        let norm = (aa * aa + b.norm_sqr()).sqrt();
        cs[i] = aa / norm;
        sn[i] = b.conj() / norm;
        alpha = scale * norm;
    }
    for j in 0..n {
        let mut xx = ZERO;
        for i in (0..=j).rev() {
            let t = xx * cs[i] + sn[i] * r.at(i, j);
            let v = r.at(i, j) * cs[i] - sn[i].conj() * xx;
            r.set(i, j, v);
            xx = t;
        }
    }
    vals(vec![r.val(c), Value::num(0.0)])
}
