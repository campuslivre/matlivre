//! Decibel conversions: `mag2db`, `db2mag`, `pow2db`, `db2pow`.
//!
//! Real input stays real; complex input goes through the complex logarithm / power. A negative
//! real magnitude gives NaN, and a negative power is an error, as in MATLAB.

use crate::builtins::{m_arg, one, Ret};
use crate::cplx::C;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::value::*;

pub const NAMES: &[&str] = &["mag2db", "db2mag", "pow2db", "db2pow"];

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if a.len() > 1 {
        return rt("Too many input arguments.");
    }
    let x = m_arg(a, 0, name)?;
    if matches!(name, "mag2db" | "pow2db") && matches!(x.kind, Kind::Char | Kind::Int(_)) {
        return rt(format!("Undefined function 'log10' for input arguments of type '{}'.", x.kind.name()));
    }
    let complex = x.im.as_ref().is_some_and(|im| im.iter().any(|&v| v != 0.0));
    if name == "pow2db" && !complex && x.data.iter().any(|&v| v < 0.0) {
        return rt("The power value must be non-negative");
    }
    let scale = if matches!(name, "mag2db" | "db2mag") { 20.0 } else { 10.0 };
    let to_db = matches!(name, "mag2db" | "pow2db");
    let mut result = if complex {
        let (mut re, mut im) = (Vec::new(), Vec::new());
        for i in 0..x.numel() {
            let z = C::new(x.data[i], x.im_at(i));
            let y = if to_db {
                let l = z.ln();
                C::new(scale * l.re / std::f64::consts::LN_10, scale * l.im / std::f64::consts::LN_10)
            } else {
                // 10^(z / scale)
                C::new(10.0, 0.0).powc(C::new(z.re / scale, z.im / scale))
            };
            re.push(y.re);
            im.push(y.im);
        }
        Matrix::complex(x.dims.clone(), re, im)
    } else {
        let data = x
            .data
            .iter()
            .map(|&v| match name {
                "mag2db" => {
                    if v < 0.0 { f64::NAN } else { 20.0 * v.log10() }
                }
                "pow2db" => 10.0 * v.log10(),
                "db2mag" => 10f64.powf(v / 20.0),
                _ => 10f64.powf(v / 10.0),
            })
            .collect();
        Matrix::from_dims(x.dims.clone(), data)
    };
    if x.kind == Kind::Single {
        result.kind = Kind::Single;
        for v in result.data.iter_mut() {
            *v = *v as f32 as f64;
        }
        if let Some(im) = result.im.as_mut() {
            for v in im.iter_mut() {
                *v = *v as f32 as f64;
            }
        }
    }
    one(Value::Num(result))
}
