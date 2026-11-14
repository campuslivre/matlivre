//! Amplitude measurements: `peak2peak`, `peak2rms`, `rssq`.

use crate::builtins::{m_arg, Ret};
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["peak2peak", "peak2rms", "rssq"];

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 2 {
        return rt("Too many input arguments.");
    }
    let x = m_arg(a, 0, name)?;
    let complex = x.im.as_ref().is_some_and(|im| im.iter().any(|&v| v != 0.0));
    if complex && name != "rssq" {
        return rt("Expected input to be real.");
    }
    if x.dims.len() > 2 {
        return rt("N-D arrays are not supported.");
    }
    let mut all_dims = false;
    let dim = if a.len() == 2 {
        let dm = m_arg(a, 1, name)?;
        if dm.numel() > 1 && dm.data.iter().all(|&d| d >= 1.0 && d.fract() == 0.0) {
            all_dims = true;
            1
        } else {
            match scalar_arg(a, 1, name).ok() {
                Some(d) if d >= 1.0 && d.fract() == 0.0 => d as usize,
                _ => return rt("Dimension argument must be a positive integer scalar within indexing range."),
            }
        }
    } else {
        first_dim(&x)
    };
    if x.numel() == 0 {
        return crate::signal::poly::out(vec![Value::Num(if name == "rssq" { Matrix::scalar(0.0) } else { Matrix::zeros(0, 0) })]);
    }
    // Squared magnitudes for rssq, so complex input contributes |x|^2.
    let sq = |i: usize| x.data[i] * x.data[i] + x.im_at(i) * x.im_at(i);
    let reduce = |idx: &[usize]| -> f64 {
        match name {
            "peak2peak" => idx.iter().map(|&i| x.data[i]).fold(f64::NEG_INFINITY, f64::max) - idx.iter().map(|&i| x.data[i]).fold(f64::INFINITY, f64::min),
            "peak2rms" => {
                let rms = (idx.iter().map(|&i| x.data[i] * x.data[i]).sum::<f64>() / idx.len() as f64).sqrt();
                idx.iter().fold(0.0f64, |m, &i| m.max(x.data[i].abs())) / rms
            }
            _ => idx.iter().map(|&i| sq(i)).sum::<f64>().sqrt(),
        }
    };
    let (rows, cols) = (x.dims[0], x.dims[1]);
    let mut result = match dim {
        _ if all_dims => Matrix::scalar(reduce(&(0..x.numel()).collect::<Vec<_>>())),
        1 => Matrix::new(1, cols, (0..cols).map(|c| reduce(&(c * rows..(c + 1) * rows).collect::<Vec<_>>())).collect()),
        2 => Matrix::new(rows, 1, (0..rows).map(|r| reduce(&(0..cols).map(|c| c * rows + r).collect::<Vec<_>>())).collect()),
        _ => Matrix::new(rows, cols, (0..x.numel()).map(|i| reduce(&[i])).collect()),
    };
    if matches!(x.kind, Kind::Single | Kind::Int(_)) {
        result.kind = x.kind;
        if x.kind == Kind::Single {
            for v in result.data.iter_mut() {
                *v = *v as f32 as f64;
            }
        }
    }
    crate::signal::poly::out(vec![Value::Num(result)])
}
