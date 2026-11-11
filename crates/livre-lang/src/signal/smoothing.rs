//! Savitzky-Golay smoothing: `sgolay` and `sgolayfilt`.
//!
//! The smoothing matrix is B = H (H'WH)^-1 H'W, where the columns of H are the powers 0..order
//! of the frame offsets -k..k and W is the diagonal matrix of the weights (the identity by
//! default); the differentiation filters are G = H (H'WH)^-1.

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["sgolay", "sgolayfilt"];

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if name == "sgolay" { sgolay(a, nargout) } else { sgolayfilt(a) }
}

fn check_design(order: f64, frame: f64) -> R<(i64, usize)> {
    if order.fract() != 0.0 {
        return rt("Polynomial degree must be an integer.");
    }
    if frame.fract() != 0.0 || frame % 2.0 == 0.0 {
        return rt("Frame length must be odd.");
    }
    if order >= frame {
        return rt("The degree must be less than the frame length.");
    }
    if frame < 1.0 {
        return rt("The degree must be less than the frame length.");
    }
    Ok((order as i64, frame as usize))
}

/// Solve the square system `m x = rhs` (columns of `rhs`) by Gaussian elimination.
fn solve(mut m: Vec<Vec<f64>>, mut rhs: Vec<Vec<f64>>) -> Vec<Vec<f64>> {
    let n = m.len();
    for col in 0..n {
        let piv = (col..n).max_by(|&p, &q| m[p][col].abs().total_cmp(&m[q][col].abs())).unwrap();
        m.swap(col, piv);
        rhs.swap(col, piv);
        for r in 0..n {
            if r != col {
                let f = m[r][col] / m[col][col];
                for c in col..n {
                    let v = m[col][c];
                    m[r][c] -= f * v;
                }
                for c in 0..rhs[r].len() {
                    let v = rhs[col][c];
                    rhs[r][c] -= f * v;
                }
            }
        }
    }
    (0..n).map(|r| rhs[r].iter().map(|v| v / m[r][r]).collect()).collect()
}

/// (B, G) with B the fl-by-fl smoothing matrix (row-major) and G the fl-by-(order+1) matrix.
/// A negative order gives all zeros.
fn design(order: i64, frame: usize, weights: &[f64]) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    if order < 0 {
        return (vec![vec![0.0; frame]; frame], vec![Vec::new(); frame]);
    }
    let order = order as usize;
    let k = (frame / 2) as f64;
    let h: Vec<Vec<f64>> = (0..frame).map(|i| (0..=order).map(|p| (i as f64 - k).powi(p as i32)).collect()).collect();
    let w = |i: usize| weights.get(i).copied().unwrap_or(1.0);
    let gram: Vec<Vec<f64>> = (0..=order).map(|p| (0..=order).map(|q| (0..frame).map(|i| w(i) * h[i][p] * h[i][q]).sum()).collect()).collect();
    let identity: Vec<Vec<f64>> = (0..=order).map(|p| (0..=order).map(|q| if p == q { 1.0 } else { 0.0 }).collect()).collect();
    let inv = solve(gram, identity);
    let g: Vec<Vec<f64>> = (0..frame).map(|i| (0..=order).map(|q| (0..=order).map(|p| h[i][p] * inv[p][q]).sum()).collect()).collect();
    let b: Vec<Vec<f64>> = (0..frame).map(|i| (0..frame).map(|j| w(j) * (0..=order).map(|p| g[i][p] * h[j][p]).sum::<f64>()).collect()).collect();
    (b, g)
}

fn weight_arg(a: &[Value], i: usize, frame: usize) -> R<Vec<f64>> {
    match a.get(i) {
        Some(v) if !matches!(v, Value::Num(m) if m.numel() == 0) => {
            let w = rvec_arg(a, i, "sgolay")?;
            if w.len() != frame {
                return rt("The weight vector must be of the same length as the frame length.");
            }
            Ok(w)
        }
        _ => Ok(Vec::new()),
    }
}

fn sgolay(a: &[Value], nargout: usize) -> Ret {
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    let (order, frame) = check_design(scalar_arg(a, 0, "sgolay")?, scalar_arg(a, 1, "sgolay")?)?;
    let weights = weight_arg(a, 2, frame)?;
    let (b, g) = design(order, frame, &weights);
    let cols = g.first().map_or(0, |r| r.len());
    let flat = |m: &Vec<Vec<f64>>, rows: usize, cols: usize| -> Matrix { Matrix::new(rows, cols, (0..cols).flat_map(|c| (0..rows).map(move |r| (r, c))).map(|(r, c)| m[r][c]).collect()) };
    let mut outs = vec![Value::Num(flat(&b, frame, frame))];
    if nargout >= 2 {
        outs.push(Value::Num(flat(&g, frame, cols)));
    }
    out(outs)
}

fn sgolayfilt(a: &[Value]) -> Ret {
    if a.len() < 3 {
        return rt("Not enough input arguments.");
    }
    if a.len() > 5 {
        return rt("Too many input arguments.");
    }
    let x = real_matrix(a, 0, "sgolayfilt")?;
    let (order, frame) = check_design(scalar_arg(a, 1, "sgolayfilt")?, scalar_arg(a, 2, "sgolayfilt")?)?;
    let weights = weight_arg(a, 3, frame)?;
    let dim = if a.len() == 5 && !matches!(&a[4], Value::Num(m) if m.numel() == 0) {
        let d = scalar_arg(a, 4, "sgolayfilt")?;
        if d < 1.0 || d.fract() != 0.0 || d > 2.0 {
            return rt("Dimension specified exceeds the dimensions of X.");
        }
        d as usize
    } else {
        first_dim(&x)
    };
    if x.numel() == 0 {
        return rt("The length of the input must be >= frame length.");
    }
    let (b, _) = design(order, frame, &weights);
    let m = frame / 2;
    let result = along(&x, dim, &|s| {
        let len = s.len();
        if len < frame {
            return rt("The length of the input must be >= frame length.");
        }
        let mut y = vec![0.0; len];
        // Steady state: the centre row (symmetric for equal weights).
        for (i, v) in y.iter_mut().enumerate().take(len - m).skip(m) {
            *v = (0..frame).map(|j| b[m][j] * s[i + j - m]).sum();
        }
        // Start-up and terminal transients from the first and last frames.
        for i in 0..m {
            y[i] = (0..frame).map(|j| b[i][j] * s[j]).sum();
            y[len - m + i] = (0..frame).map(|j| b[frame - m + i][j] * s[len - frame + j]).sum();
        }
        Ok(y)
    })?;
    out(vec![Value::Num(result)])
}
