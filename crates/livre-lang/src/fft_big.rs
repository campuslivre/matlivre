//! Multithreaded FFT of one long vector.
//!
//! rustfft runs on one thread, and a transform of a million points spends most of its time
//! waiting for memory. A length `n = n1 * n2` transform is split in the four-step way: `n2`
//! transforms of length `n1` over the columns of an `n1 x n2` array, a twiddle factor per
//! element, then `n1` transforms of length `n2` over its rows. The column transforms gather a
//! few columns at a time into a small buffer that stays in the cache, and the row transforms
//! write their results straight into transposed position, so the data crosses the memory bus
//! twice instead of once per step. All steps run on the rayon pool. A real input of even length
//! is packed into a complex vector of half the length and unpacked afterwards, which halves the
//! work and gives an exactly conjugate-symmetric result. Vectors below [`BIG`] elements, or
//! whose length has no split into two factors of at least 64, are left to the ordinary
//! single-thread path.

use std::cell::RefCell;

use rayon::prelude::*;
use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

/// Shortest transform handled here.
pub const BIG: usize = 1 << 16;
/// Smallest factor of a split.
const MIN_FACTOR: usize = 64;
/// Columns (or rows) handled together by one task.
const GROUP: usize = 8;
/// Entries of the low part of the twiddle table.
const LOW: usize = 1024;

thread_local! {
    /// Work buffers kept from one transform to the next: a fresh allocation of this size comes
    /// back from the OS as untouched pages, and faulting them in costs about as much as the
    /// transform itself.
    static POOL: RefCell<Vec<Vec<Complex64>>> = const { RefCell::new(Vec::new()) };
}

/// Most bytes kept in the pool.
const POOL_BYTES: usize = 160 << 20;

/// A buffer of `n` elements with arbitrary contents.
fn take(n: usize) -> Vec<Complex64> {
    let zero = Complex64::new(0.0, 0.0);
    POOL.with(|p| {
        let mut p = p.borrow_mut();
        match p.iter().position(|v| v.capacity() >= n) {
            Some(i) => {
                let mut v = p.swap_remove(i);
                v.resize(n, zero);
                v.truncate(n);
                v
            }
            None => vec![zero; n],
        }
    })
}

fn give(v: Vec<Complex64>) {
    POOL.with(|p| {
        let mut p = p.borrow_mut();
        let total: usize = p.iter().map(|b| b.capacity() * 16).sum::<usize>() + v.capacity() * 16;
        if total <= POOL_BYTES {
            p.push(v);
        }
    })
}

/// `n = n1 * n2` with both factors at least [`MIN_FACTOR`], as close to a square as possible.
fn split(n: usize) -> Option<(usize, usize)> {
    let mut d = (n as f64).sqrt() as usize;
    while d >= MIN_FACTOR {
        if n.is_multiple_of(d) {
            return Some((d, n / d));
        }
        d -= 1;
    }
    None
}

/// `w^m` for `w = exp(-2 pi i / n)` (conjugated for an inverse transform), as the product of two
/// table entries, each computed directly from its angle.
struct Twiddle {
    lo: Vec<Complex64>,
    hi: Vec<Complex64>,
}

impl Twiddle {
    fn new(n: usize, inverse: bool) -> Twiddle {
        let sign = if inverse { 1.0 } else { -1.0 };
        let w = |m: usize| {
            let (s, c) = (sign * 2.0 * std::f64::consts::PI * (m as f64) / (n as f64)).sin_cos();
            Complex64::new(c, s)
        };
        let lo = (0..LOW.min(n)).into_par_iter().map(&w).collect();
        let hi = (0..n.div_ceil(LOW)).into_par_iter().map(|h| w(h * LOW)).collect();
        Twiddle { lo, hi }
    }

    /// `w^m` for `m < n`.
    #[inline]
    fn at(&self, m: usize) -> Complex64 {
        self.hi[m / LOW] * self.lo[m % LOW]
    }
}

/// A buffer written from several tasks, each to positions no other task writes.
struct Shared {
    ptr: *mut Complex64,
    len: usize,
}

unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

impl Shared {
    fn new(v: &mut [Complex64]) -> Shared {
        Shared { ptr: v.as_mut_ptr(), len: v.len() }
    }

    /// Writes position `i`; every position is written by one task only.
    #[inline]
    fn put(&self, i: usize, v: Complex64) {
        assert!(i < self.len);
        // SAFETY: in bounds, and the callers write each position from exactly one task.
        unsafe { *self.ptr.add(i) = v }
    }
}

/// As [`Shared`], for real arrays.
struct SharedF {
    ptr: *mut f64,
    len: usize,
}

unsafe impl Send for SharedF {}
unsafe impl Sync for SharedF {}

impl SharedF {
    fn new(v: &mut [f64]) -> SharedF {
        SharedF { ptr: v.as_mut_ptr(), len: v.len() }
    }

    #[inline]
    fn put(&self, i: usize, v: f64) {
        assert!(i < self.len);
        // SAFETY: in bounds, and the callers write each position from exactly one task.
        unsafe { *self.ptr.add(i) = v }
    }
}

/// Transform of the `n` values `src(0..n)`, unscaled, as a pooled buffer. `None` when `n` has no
/// usable split.
fn complex_fft(n: usize, src: &(dyn Fn(usize) -> Complex64 + Sync), inverse: bool) -> Option<Vec<Complex64>> {
    let (n1, n2) = split(n)?;
    let mut planner = FftPlanner::<f64>::new();
    let (p1, p2) = if inverse { (planner.plan_fft_inverse(n1), planner.plan_fft_inverse(n2)) } else { (planner.plan_fft_forward(n1), planner.plan_fft_forward(n2)) };
    let tw = Twiddle::new(n, inverse);
    let zero = Complex64::new(0.0, 0.0);
    // The input is n1 rows of n2: element (j1, j2) is src(j1 * n2 + j2). Step 1: a transform over
    // j1 for every column j2, times the twiddle factor, kept in a row-major n1 x n2 array.
    let mut w = take(n);
    {
        let shared = Shared::new(&mut w);
        let s1 = p1.get_inplace_scratch_len();
        (0..n2.div_ceil(GROUP)).into_par_iter().for_each_init(
            || (vec![zero; n1 * GROUP], vec![zero; s1]),
            |(buf, sc), gi| {
                let g0 = gi * GROUP;
                let gc = GROUP.min(n2 - g0);
                for r in 0..n1 {
                    for c in 0..gc {
                        buf[c * n1 + r] = src(r * n2 + g0 + c);
                    }
                }
                for c in 0..gc {
                    let col = &mut buf[c * n1..(c + 1) * n1];
                    p1.process_with_scratch(col, sc);
                    for (k1, v) in col.iter_mut().enumerate() {
                        *v *= tw.at((g0 + c) * k1);
                    }
                }
                for k1 in 0..n1 {
                    for c in 0..gc {
                        shared.put(k1 * n2 + g0 + c, buf[c * n1 + k1]);
                    }
                }
            },
        );
    }
    // Step 2: a transform over j2 for every row k1, written to position k2 * n1 + k1.
    let mut out = take(n);
    {
        let shared = Shared::new(&mut out);
        let s2 = p2.get_inplace_scratch_len();
        w.par_chunks_mut(GROUP * n2).enumerate().for_each_init(
            || vec![zero; s2],
            |sc, (gi, rows)| {
                let k0 = gi * GROUP;
                let rc = rows.len() / n2;
                for row in rows.chunks_mut(n2) {
                    p2.process_with_scratch(row, sc);
                }
                for k2 in 0..n2 {
                    for kk in 0..rc {
                        shared.put(k2 * n1 + k0 + kk, rows[kk * n2 + k2]);
                    }
                }
            },
        );
    }
    give(w);
    Some(out)
}

/// FFT (`inverse`: scaled inverse) of the vector `re + i im` zero-padded or cut to `n` points.
/// Returns the real and imaginary parts of the result, or `None` when the length is not handled
/// here.
pub fn transform(re: &[f64], im: Option<&[f64]>, n: usize, inverse: bool) -> Option<(Vec<f64>, Vec<f64>)> {
    if n < BIG {
        return None;
    }
    let at = |i: usize| -> f64 { re.get(i).copied().unwrap_or(0.0) };
    // Real input, forward, even length: transform half-length complex data.
    if im.is_none() && !inverse && n.is_multiple_of(2) {
        let h = n / 2;
        split(h)?;
        let big_z = complex_fft(h, &|m| Complex64::new(at(2 * m), at(2 * m + 1)), false)?;
        let tw = Twiddle::new(n, false);
        let mut ore = vec![0.0; n];
        let mut oim = vec![0.0; n];
        {
            // Coefficients 0..=h from the half-length transform; the rest are their conjugate
            // mirror images. Every position is written once.
            let (pr, pi) = (SharedF::new(&mut ore), SharedF::new(&mut oim));
            const CHUNK: usize = 8192;
            (0..(h + 1).div_ceil(CHUNK)).into_par_iter().for_each(|ci| {
                for k in ci * CHUNK..((ci + 1) * CHUNK).min(h + 1) {
                    let zk = big_z[if k == h { 0 } else { k }];
                    let zc = big_z[if k == 0 { 0 } else { h - k }].conj();
                    let e = (zk + zc) * 0.5;
                    let o = (zk - zc) * Complex64::new(0.0, -0.5);
                    let mut v = e + tw.at(k) * o;
                    // The first and the Nyquist coefficients of a real signal are real.
                    if k == 0 || k == h {
                        v.im = 0.0;
                    }
                    pr.put(k, v.re);
                    pi.put(k, v.im);
                    if k > 0 && k < h {
                        pr.put(n - k, v.re);
                        pi.put(n - k, -v.im);
                    }
                }
            });
        }
        give(big_z);
        return Some((ore, oim));
    }
    split(n)?;
    let y = complex_fft(n, &|i| Complex64::new(at(i), im.and_then(|v| v.get(i).copied()).unwrap_or(0.0)), inverse)?;
    let scale = if inverse { 1.0 / n as f64 } else { 1.0 };
    let mut ore = vec![0.0; n];
    let mut oim = vec![0.0; n];
    ore.par_iter_mut().zip(oim.par_iter_mut()).zip(y.par_iter()).for_each(|((r, i), v)| {
        *r = v.re * scale;
        *i = v.im * scale;
    });
    give(y);
    Some((ore, oim))
}
