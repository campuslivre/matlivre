//! Rational models fitted to impulse responses: `prony`.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["prony"];

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
    let _ = name;
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
