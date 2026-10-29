//! Arithmetic, relational and logical operators with MATLAB implicit expansion.

use crate::ast::{BinOp, UnOp};
use crate::display::fmt_g;
use crate::error::{rt, LangError};
use crate::value::{Kind, Matrix, Value};

type R<T> = Result<T, LangError>;

/// Elementwise `f(a, b)` with scalar and implicit (row/column) expansion.
pub fn broadcast(a: &Matrix, b: &Matrix, f: impl Fn(f64, f64) -> f64) -> R<Matrix> {
    if a.rows == b.rows && a.cols == b.cols {
        let data = a.data.iter().zip(&b.data).map(|(&x, &y)| f(x, y)).collect();
        return Ok(Matrix::new(a.rows, a.cols, data));
    }
    if a.is_scalar() {
        let x = a.data[0];
        return Ok(Matrix::new(b.rows, b.cols, b.data.iter().map(|&y| f(x, y)).collect()));
    }
    if b.is_scalar() {
        let y = b.data[0];
        return Ok(Matrix::new(a.rows, a.cols, a.data.iter().map(|&x| f(x, y)).collect()));
    }
    let rows = expand_dim(a.rows, b.rows)?;
    let cols = expand_dim(a.cols, b.cols)?;
    let mut data = Vec::with_capacity(rows * cols);
    for c in 0..cols {
        let ca = if a.cols == 1 { 0 } else { c };
        let cb = if b.cols == 1 { 0 } else { c };
        for r in 0..rows {
            let ra = if a.rows == 1 { 0 } else { r };
            let rb = if b.rows == 1 { 0 } else { r };
            data.push(f(a.get(ra, ca), b.get(rb, cb)));
        }
    }
    Ok(Matrix::new(rows, cols, data))
}

fn expand_dim(x: usize, y: usize) -> R<usize> {
    if x == y {
        Ok(x)
    } else if x == 1 {
        Ok(y)
    } else if y == 1 {
        Ok(x)
    } else {
        rt("Arrays have incompatible sizes for this operation.")
    }
}

fn b2f(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

/// Text of a value when concatenated with a string (`"n = " + 5`).
fn as_string_operand(v: &Value) -> R<String> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        Value::Num(m) if m.kind == Kind::Char => Ok(m.to_string_lossy()),
        Value::Num(m) if m.is_scalar() => Ok(if m.kind == Kind::Logical {
            if m.data[0] != 0.0 { "true".into() } else { "false".into() }
        } else {
            fmt_g(m.data[0], 5)
        }),
        _ => rt("String concatenation requires scalar operands."),
    }
}

pub fn binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    if matches!(a, Value::Str(_)) || matches!(b, Value::Str(_)) {
        let (x, y) = (as_string_operand(a)?, as_string_operand(b)?);
        return match op {
            BinOp::Add => Ok(Value::Str(x + &y)),
            BinOp::Eq => Ok(Value::bool(x == y)),
            BinOp::Ne => Ok(Value::bool(x != y)),
            BinOp::Lt => Ok(Value::bool(x < y)),
            BinOp::Gt => Ok(Value::bool(x > y)),
            BinOp::Le => Ok(Value::bool(x <= y)),
            BinOp::Ge => Ok(Value::bool(x >= y)),
            _ => rt(format!("Operator '{}' is not supported for strings.", op.symbol())),
        };
    }
    let (Value::Num(x), Value::Num(y)) = (a, b) else {
        let bad = if matches!(a, Value::Num(_)) { b } else { a };
        return rt(format!("Operator '{}' is not supported for operands of type '{}'.", op.symbol(), bad.class_name()));
    };
    num_binary(op, x, y).map(Value::Num)
}

pub fn num_binary(op: BinOp, x: &Matrix, y: &Matrix) -> R<Matrix> {
    use BinOp::*;
    let logical = |m: Matrix| m.with_kind(Kind::Logical);
    Ok(match op {
        Add => broadcast(x, y, |a, b| a + b)?,
        Sub => broadcast(x, y, |a, b| a - b)?,
        Mul => broadcast(x, y, |a, b| a * b)?,
        Div => broadcast(x, y, |a, b| a / b)?,
        LeftDiv => broadcast(x, y, |a, b| b / a)?,
        Pow => broadcast(x, y, f64::powf)?,
        MatMul => {
            if x.is_scalar() || y.is_scalar() {
                broadcast(x, y, |a, b| a * b)?
            } else {
                matmul(x, y)?
            }
        }
        MatDiv => {
            if y.is_scalar() {
                broadcast(x, y, |a, b| a / b)?
            } else {
                // x / y = (y' \ x')'
                solve(&y.transpose(), &x.transpose())?.transpose()
            }
        }
        MatLeftDiv => {
            if x.is_scalar() {
                broadcast(x, y, |a, b| b / a)?
            } else {
                solve(x, y)?
            }
        }
        MatPow => {
            if x.is_scalar() && y.is_scalar() {
                Matrix::scalar(x.data[0].powf(y.data[0]))
            } else if y.is_scalar() && x.rows == x.cols {
                let p = y.data[0];
                if p.fract() != 0.0 {
                    return rt("Matrix power with non-integer exponents is not supported.");
                }
                let base = if p < 0.0 { inv(x)? } else { x.clone() };
                let mut n = p.abs() as u64;
                let mut result = identity(x.rows);
                let mut sq = base;
                while n > 0 {
                    if n & 1 == 1 {
                        result = matmul(&result, &sq)?;
                    }
                    sq = matmul(&sq, &sq)?;
                    n >>= 1;
                }
                result
            } else {
                return rt("Incorrect dimensions for raising a matrix to a power. Check that the matrix is square and the power is a scalar. To operate on each element of the matrix individually, use POWER (.^).");
            }
        }
        Eq => logical(broadcast(x, y, |a, b| b2f(a == b))?),
        Ne => logical(broadcast(x, y, |a, b| b2f(a != b))?),
        Lt => logical(broadcast(x, y, |a, b| b2f(a < b))?),
        Le => logical(broadcast(x, y, |a, b| b2f(a <= b))?),
        Gt => logical(broadcast(x, y, |a, b| b2f(a > b))?),
        Ge => logical(broadcast(x, y, |a, b| b2f(a >= b))?),
        And | AndAnd => logical(broadcast(x, y, |a, b| b2f(a != 0.0 && b != 0.0))?),
        Or | OrOr => logical(broadcast(x, y, |a, b| b2f(a != 0.0 || b != 0.0))?),
    })
}

pub fn unary(op: UnOp, v: &Value) -> R<Value> {
    match (op, v) {
        (UnOp::CTranspose | UnOp::Transpose, Value::Num(m)) => Ok(Value::Num(m.transpose())),
        (UnOp::CTranspose | UnOp::Transpose, Value::Cell(c)) => {
            let mut data = Vec::with_capacity(c.data.len());
            for r in 0..c.rows {
                for col in 0..c.cols {
                    data.push(c.data[col * c.rows + r].clone());
                }
            }
            Ok(Value::Cell(crate::value::CellArray::new(c.cols, c.rows, data)))
        }
        (UnOp::CTranspose | UnOp::Transpose, Value::Str(_) | Value::Struct(_) | Value::Func(_)) => Ok(v.clone()),
        (UnOp::Neg, Value::Num(m)) => Ok(Value::Num(m.map(|x| -x))),
        (UnOp::Plus, Value::Num(m)) => Ok(Value::Num(m.map(|x| x))),
        (UnOp::Not, Value::Num(m)) => Ok(Value::Num(m.map(|x| b2f(x == 0.0)).with_kind(Kind::Logical))),
        _ => rt(format!("Unary operator is not supported for values of class '{}'.", v.class_name())),
    }
}

pub fn identity(n: usize) -> Matrix {
    let mut m = Matrix::zeros(n, n);
    for i in 0..n {
        m.set(i, i, 1.0);
    }
    m
}

pub fn matmul(a: &Matrix, b: &Matrix) -> R<Matrix> {
    if a.cols != b.rows {
        return rt("Incorrect dimensions for matrix multiplication. Check that the number of columns in the first matrix matches the number of rows in the second matrix. To operate on each element of the matrix individually, use TIMES (.*) for elementwise multiplication.");
    }
    let (n, m, p) = (a.rows, a.cols, b.cols);
    let mut out = vec![0.0; n * p];
    // Column-major j-k-i loop order keeps the inner loop contiguous.
    for j in 0..p {
        let oc = &mut out[j * n..(j + 1) * n];
        for k in 0..m {
            let bkj = b.data[j * m + k];
            if bkj == 0.0 {
                continue;
            }
            let ac = &a.data[k * n..(k + 1) * n];
            for i in 0..n {
                oc[i] += ac[i] * bkj;
            }
        }
    }
    Ok(Matrix::new(n, p, out))
}

/// LU decomposition with partial pivoting. Returns (lu, perm, sign) or an error if singular.
fn lu(a: &Matrix) -> R<(Matrix, Vec<usize>, f64)> {
    let n = a.rows;
    let mut lu = a.clone();
    lu.kind = Kind::Double;
    let mut perm: Vec<usize> = (0..n).collect();
    let mut sign = 1.0;
    for k in 0..n {
        let mut p = k;
        let mut best = lu.get(k, k).abs();
        for i in k + 1..n {
            let v = lu.get(i, k).abs();
            if v > best {
                best = v;
                p = i;
            }
        }
        if best < 1e-300 {
            return rt("Matrix is singular to working precision.");
        }
        if p != k {
            for c in 0..n {
                let t = lu.get(k, c);
                lu.set(k, c, lu.get(p, c));
                lu.set(p, c, t);
            }
            perm.swap(k, p);
            sign = -sign;
        }
        let pivot = lu.get(k, k);
        for i in k + 1..n {
            let f = lu.get(i, k) / pivot;
            lu.set(i, k, f);
            if f != 0.0 {
                for c in k + 1..n {
                    let v = lu.get(i, c) - f * lu.get(k, c);
                    lu.set(i, c, v);
                }
            }
        }
    }
    Ok((lu, perm, sign))
}

fn lu_solve(lu: &Matrix, perm: &[usize], b: &Matrix) -> Matrix {
    let n = lu.rows;
    let mut x = Matrix::zeros(n, b.cols);
    for col in 0..b.cols {
        let mut y: Vec<f64> = perm.iter().map(|&p| b.get(p, col)).collect();
        for i in 0..n {
            for k in 0..i {
                y[i] -= lu.get(i, k) * y[k];
            }
        }
        for i in (0..n).rev() {
            for k in i + 1..n {
                y[i] -= lu.get(i, k) * y[k];
            }
            y[i] /= lu.get(i, i);
        }
        for i in 0..n {
            x.set(i, col, y[i]);
        }
    }
    x
}

/// `a \ b`: exact solve for square systems, least squares otherwise.
pub fn solve(a: &Matrix, b: &Matrix) -> R<Matrix> {
    if a.rows != b.rows {
        return rt("Matrix dimensions must agree.");
    }
    if a.rows == a.cols {
        let (lu, perm, _) = lu(a)?;
        Ok(lu_solve(&lu, &perm, b))
    } else {
        let at = a.transpose();
        let ata = matmul(&at, a)?;
        let atb = matmul(&at, b)?;
        let (lu, perm, _) = lu(&ata)?;
        Ok(lu_solve(&lu, &perm, &atb))
    }
}

pub fn inv(a: &Matrix) -> R<Matrix> {
    if a.rows != a.cols {
        return rt("Matrix must be square.");
    }
    let (lu, perm, _) = lu(a)?;
    Ok(lu_solve(&lu, &perm, &identity(a.rows)))
}

pub fn det(a: &Matrix) -> R<f64> {
    if a.rows != a.cols {
        return rt("Matrix must be square.");
    }
    if a.rows == 0 {
        return Ok(1.0);
    }
    match lu(a) {
        Ok((lu, _, sign)) => Ok((0..a.rows).map(|i| lu.get(i, i)).product::<f64>() * sign),
        Err(_) => Ok(0.0),
    }
}
