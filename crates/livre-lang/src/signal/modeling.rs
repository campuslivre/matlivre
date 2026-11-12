//! Rational models fitted to impulse responses: `prony`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["prony", "lpc"];

fn order_arg(a: &[Value], i: usize, label: &str) -> R<usize> {
    let m = match &a[i] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt(format!("Expected input number {}, {label}, to be numeric.", i + 1)),
    };
    if m.numel() != 1 {
        return rt(format!("Expected input number {}, {label}, to be a scalar.", i + 1));
    }
    let v = m.data[0];
    if v < 0.0 {
        return rt(format!("Expected input number {}, {label}, to be nonnegative.", i + 1));
    }
    if v.fract() != 0.0 {
        return rt(format!("Expected input number {}, {label}, to be integer-valued.", i + 1));
    }
    Ok(v as usize)
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if name == "lpc" {
        return lpc(it, a, nargout);
    }
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let h = match &a[0] {
        Value::Num(m) if m.kind != Kind::Char => m.clone(),
        _ => return rt("Expected input number 1, H, to be numeric."),
    };
    if h.dims.len() > 2 || (h.dims[0] > 1 && h.dims[1] > 1) {
        return rt("Expected input number 1, H, to be a vector.");
    }
    let m = order_arg(a, 1, "NB")?;
    let n = order_arg(a, 2, "NA")?;
    let single = h.kind == Kind::Single;
    if h.numel() == 0 {
        let nan_row = |k: usize| Value::Num(Matrix::from_dims(vec![1, k], vec![f64::NAN; k]));
        return out(vec![nan_row(m + 1), nan_row(n + 1)]);
    }
    let mut re = h.data.clone();
    let mut im: Vec<f64> = (0..h.numel()).map(|i| h.im_at(i)).collect();
    let mut k = re.len() - 1;
    if k <= m.max(n) {
        let len = m.max(n) + 2;
        re.resize(len, 0.0);
        im.resize(len, 0.0);
        k = len - 1;
    }
    let (c_re, c_im) = if re[0] == 0.0 && im[0] == 0.0 { (1.0, 0.0) } else { (re[0], im[0]) };
    let cd = c_re * c_re + c_im * c_im;
    let sre: Vec<f64> = (0..re.len()).map(|i| (re[i] * c_re + im[i] * c_im) / cd).collect();
    let sim: Vec<f64> = (0..re.len()).map(|i| (im[i] * c_re - re[i] * c_im) / cd).collect();
    // H(i, j) = h(i - j) for the first K+1 rows of the convolution matrix.
    let at = |i: usize, j: usize| -> (f64, f64) { if i >= j { (sre[i - j], sim[i - j]) } else { (0.0, 0.0) } };
    let rows = k + 1 - (m + 1);
    let mut a_rest = vec![(0.0, 0.0); n];
    if n > 0 {
        let mut dre = vec![0.0; rows * n];
        let mut dim = vec![0.0; rows * n];
        let mut rre = vec![0.0; rows];
        let mut rim = vec![0.0; rows];
        for r in 0..rows {
            let i = m + 1 + r;
            let (x, y) = at(i, 0);
            rre[r] = -x;
            rim[r] = -y;
            for j in 0..n {
                let (x, y) = at(i, j + 1);
                dre[j * rows + r] = x;
                dim[j * rows + r] = y;
            }
        }
        let cplx = im.iter().any(|&v| v != 0.0);
        let mk = |r: usize, c: usize, d: Vec<f64>, i: Vec<f64>| {
            Value::Num(if cplx { Matrix::complex(vec![r, c], d, i) } else { Matrix::from_dims(vec![r, c], d) })
        };
        if !cplx {
            let cols: Vec<Vec<f64>> = (0..n).map(|j| dre[j * rows..(j + 1) * rows].to_vec()).collect();
            let x = crate::signal::ar::lstsq(&cols, &rre);
            for j in 0..n {
                a_rest[j] = (x[j], 0.0);
            }
        } else {
        let sol = it.call_function("mldivide", vec![mk(rows, n, dre, dim), mk(rows, 1, rre, rim)], 1)?.remove(0).to_matrix()?;
        for j in 0..n {
            a_rest[j] = (sol.data[j], sol.im_at(j));
        }
        }
    }
    let mut den: Vec<(f64, f64)> = vec![(1.0, 0.0)];
    den.extend(a_rest);
    let mut num = vec![(0.0, 0.0); m + 1];
    for (i, slot) in num.iter_mut().enumerate() {
        let (mut sr, mut si) = (0.0, 0.0);
        for (j, &(dr, di)) in den.iter().enumerate() {
            let (x, y) = at(i, j);
            sr += x * dr - y * di;
            si += x * di + y * dr;
        }
        *slot = (sr * c_re - si * c_im, sr * c_im + si * c_re);
    }
    let build = |v: &[(f64, f64)]| {
        let mut mat = if im.iter().any(|&x| x != 0.0) {
            Matrix::complex(vec![1, v.len()], v.iter().map(|p| p.0).collect(), v.iter().map(|p| p.1).collect())
        } else {
            Matrix::from_dims(vec![1, v.len()], v.iter().map(|p| p.0).collect())
        };
        if single {
            mat.kind = Kind::Single;
        }
        Value::Num(mat)
    };
    out(vec![build(&num), build(&den)])
}

fn lpc(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if m.kind == Kind::Double => m.clone(),
        _ => return rt("Input arguments must be 'double'."),
    };
    if x.numel() == 0 {
        return rt("Input vector X should not be empty");
    }
    let rows = if x.dims[0] == 1 { x.numel() } else { x.dims[0] };
    let ncols = x.numel() / rows;
    let p = if a.len() > 1 {
        let m = crate::builtins::m_arg(a, 1, "lpc")?;
        if m.numel() != 1 {
            return rt("Expected input number 2, N, to be a scalar.");
        }
        if m.data[0] < 0.0 || m.data[0].is_nan() {
            return rt("Order of the predictor should be a positive integer.");
        }
        m.data[0].floor() as usize
    } else {
        rows - 1
    };
    if rows == 1 {
        return out(vec![scalar_value(1.0), scalar_value(x.data[0] * x.data[0])]);
    }
    if p > rows {
        return rt("X must be a vector with length greater or equal to the prediction order. If X is a matrix, the length of each column must be greater or equal to the prediction order.");
    }
    let cplx = x.im.is_some();
    let mut arows: Vec<Vec<(f64, f64)>> = vec![];
    let mut gs: Vec<(f64, f64)> = vec![];
    for c in 0..ncols {
        let col: Vec<(f64, f64)> = (0..rows).map(|i| (x.data[c * rows + i], x.im_at(c * rows + i))).collect();
        let mut rr = vec![0.0; p + 1];
        let mut ri = vec![0.0; p + 1];
        for k in 0..=p {
            for n in 0..rows - k {
                let (ar, ai) = col[n + k];
                let (br, bi) = col[n];
                rr[k] += (ar * br + ai * bi) / rows as f64;
                ri[k] += (ai * br - ar * bi) / rows as f64;
            }
        }
        if cplx {
            let cm = |x: (f64, f64), y: (f64, f64)| (x.0 * y.0 - x.1 * y.1, x.0 * y.1 + x.1 * y.0);
            let r: Vec<(f64, f64)> = (0..=p).map(|k| (rr[k], ri[k])).collect();
            let mut av = vec![(1.0, 0.0)];
            let mut e = r[0].0;
            for m in 1..=p {
                let mut acc = (0.0, 0.0);
                for i in 0..m {
                    let t = cm(av[i], r[m - i]);
                    acc = (acc.0 + t.0, acc.1 + t.1);
                }
                let k = (-acc.0 / e, -acc.1 / e);
                let prev = av.clone();
                av.push((0.0, 0.0));
                for i in 1..=m {
                    let c = (prev.get(m - i).map_or(0.0, |v| v.0), -prev.get(m - i).map_or(0.0, |v| v.1));
                    let t = cm(k, c);
                    let base = if i < prev.len() { prev[i] } else { (0.0, 0.0) };
                    av[i] = (base.0 + t.0, base.1 + t.1);
                }
                e *= 1.0 - (k.0 * k.0 + k.1 * k.1);
            }
            arows.push(av);
            gs.push((e, 0.0));
            continue;
        }
        let rv = if cplx { Value::Num(Matrix::complex(vec![p + 1, 1], rr, ri)) } else { real_col(rr) };
        let res = it.call_function("levinson", vec![rv, scalar_value(p as f64)], 2)?;
        let am = res[0].to_matrix()?;
        arows.push((0..am.numel()).map(|i| (am.data[i], am.im_at(i))).collect());
        let e = res[1].to_matrix()?;
        gs.push((e.data[0], e.im_at(0)));
    }
    let width = p + 1;
    let mut re = vec![0.0; ncols * width];
    let mut im = vec![0.0; ncols * width];
    for (c, row) in arows.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            re[j * ncols + c] = v.0;
            im[j * ncols + c] = v.1;
        }
    }
    let mk = |d: Vec<f64>, i: Vec<f64>, r: usize, cc: usize| {
        Value::Num(if cplx { Matrix::complex(vec![r, cc], d, i) } else { Matrix::from_dims(vec![r, cc], d) })
    };
    let g = mk(gs.iter().map(|v| v.0).collect(), gs.iter().map(|v| v.1).collect(), ncols, 1);
    out(vec![mk(re, im, ncols, width), g])
}
