//! Sparse matrices: construction (`sparse`, `speye`, `spdiags`, `sprand`, ...), queries
//! (`nonzeros`, `nzmax`, `sprank`), test matrices (`numgrid`, `delsq`), plots (`spy`,
//! `gplot`, `treeplot`), iterative solvers (`pcg`, `gmres`, `bicgstab`, `minres`, ...),
//! incomplete factorizations (`ichol`, `ilu`) and elimination trees (`etree`, `symbfact`).
//!
//! A sparse matrix is a [`Matrix`] with the `sparse` attribute: values are stored densely, so
//! very large sparse matrices are refused rather than allocated.

use crate::builtins::{arg, none, num2str_scalar, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "sparse", "full", "issparse", "speye", "spones", "spfun", "nonzeros", "nzmax", "spalloc", "spconvert", "spdiags", "sprand", "sprandn", "sprandsym",
    "spaugment", "sprank", "spparms", "spy", "gplot", "delsq", "numgrid", "pcg", "gmres", "bicg", "bicgstab", "bicgstabl", "cgs", "minres", "lsqr", "qmr",
    "tfqmr", "symmlq", "ichol", "ilu", "etree", "etreeplot", "symbfact", "colperm", "symrcm", "treelayout", "treeplot", "unmesh",
];

/// Elementwise functions and reshapes whose result is sparse for a sparse argument.
const KEEP_SPARSE: &[&str] = &[
    "abs", "sqrt", "round", "floor", "ceil", "fix", "sign", "conj", "real", "imag", "sin", "tan", "asin", "atan", "sinh", "tanh", "asinh", "atanh", "expm1",
    "log1p", "tril", "triu", "diag", "reshape", "kron", "repmat", "sum", "transpose", "ctranspose", "uminus", "times", "spones", "double", "logical",
];

/// The largest sparse matrix (in elements) MatLivre will hold, since it stores them densely.
const MAX_ELEMENTS: usize = 200_000_000;

fn check_size(m: usize, n: usize) -> R<()> {
    if m.saturating_mul(n) > MAX_ELEMENTS {
        return rt(format!(
            "Sparse matrices with more than {MAX_ELEMENTS} elements ({m}-by-{n}) are not supported in this version: MatLivre stores sparse matrices densely."
        ));
    }
    Ok(())
}

pub fn is_sparse(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.sparse)
}

fn sp(mut m: Matrix) -> Value {
    m.sparse = true;
    Value::Num(m)
}

/// Marks a builtin's result sparse when MATLAB keeps a sparse argument sparse.
pub fn propagate(name: &str, args: &[Value], out: &mut [Value]) {
    if KEEP_SPARSE.contains(&name) && args.first().is_some_and(is_sparse) {
        if let Some(Value::Num(m)) = out.first_mut() {
            m.sparse = true;
        }
    }
}

/// The sparse attribute of `a op b` (MATLAB's rules for the result being sparse).
pub fn binary_sparse(op: crate::ast::BinOp, a: &Value, b: &Value) -> bool {
    use crate::ast::BinOp::*;
    let (sa, sb) = (is_sparse(a), is_sparse(b));
    if !sa && !sb {
        return false;
    }
    let scalar = |v: &Value| v.numel() == 1;
    match op {
        Add | Sub => sa && sb,
        Mul | And => sa || sb,
        MatMul => (sa && sb) || (sa && scalar(b)) || (scalar(a) && sb),
        Div | MatDiv => sa && (scalar(b) || !sb),
        LeftDiv | MatLeftDiv => sb && scalar(a),
        Pow => sa && scalar(b),
        MatPow => sa,
        Eq | Ne | Lt | Le | Gt | Ge | Or => sa || sb,
        AndAnd | OrOr => false,
    }
}

/// The display of a sparse matrix: `(row,col)  value` for each nonzero, by columns.
pub fn display(name: &str, m: &Matrix, f: crate::display::NumFormat) -> String {
    let body = body(m, f);
    if m.kind == Kind::Logical {
        return format!("{name} = {}x{} sparse logical array\n\n{body}", m.rows(), m.cols());
    }
    format!("{name} = \n\n{body}")
}

pub fn body(m: &Matrix, f: crate::display::NumFormat) -> String {
    let (r, c) = (m.rows(), m.cols());
    let nz: Vec<usize> = (0..m.data.len()).filter(|&k| m.data[k] != 0.0 || m.im.as_ref().is_some_and(|im| im[k] != 0.0)).collect();
    if nz.is_empty() {
        return format!("   All zero sparse: {r}x{c}");
    }
    let iw = format!("({r},{c})").len().max(6);
    let vals: Vec<f64> = nz.iter().map(|&k| m.data[k]).collect();
    let (texts, vf) = if m.kind == Kind::Logical {
        (vals.iter().map(|_| "1".to_string()).collect::<Vec<_>>(), 7)
    } else {
        // Values formatted as one column, the line 9 wider than that column.
        let col = Matrix::col(vals.clone());
        let lines: Vec<String> = crate::display::disp_value(&Value::Num(col), f).lines().map(|l| l.to_string()).collect();
        let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1);
        let texts = if lines.len() == vals.len() { lines.iter().map(|l| l.trim().to_string()).collect() } else { vals.iter().map(|v| num2str_scalar(*v)).collect() };
        (texts, 9 + width - iw)
    };
    nz.iter()
        .zip(texts)
        .map(|(&k, t)| {
            let idx = format!("({},{})", k % r + 1, k / r + 1);
            format!("  {idx:>iw$}{t:>vf$}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn call_handle(it: &mut Interpreter, f: &Value, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    match f {
        Value::Func(fv) => it.call_value(fv, args, nargout),
        _ => rt("Expected a function handle."),
    }
}

/// The sparse-matrix parameters, in MATLAB's order, with their defaults.
const SPPARMS: [(&str, f64); 19] = [
    ("spumoni", 0.0), ("thr_rel", 1.1), ("thr_abs", 1.0), ("exact_d", 0.0), ("supernd", 3.0), ("rreduce", 3.0), ("wh_frac", 0.5), ("autommd", 1.0),
    ("autoamd", 1.0), ("piv_tol", 0.1), ("bandden", 0.5), ("umfpack", 1.0), ("sym_tol", 0.001), ("ldl_tol", 0.01), ("usema57", 1.0), ("spqrtol", -2.0),
    ("sp_ctor", 0.0), ("reorder", 0.0), ("no_redo", 0.0),
];

thread_local! {
    static SPPARM_VALUES: std::cell::RefCell<Vec<f64>> = std::cell::RefCell::new(SPPARMS.iter().map(|p| p.1).collect());
}

/// spparms: get and set the sparse-matrix parameters (they are kept, but MatLivre's
/// solvers do not read them).
fn spparms(a: &[Value], nargout: usize) -> Ret {
    let all = || SPPARM_VALUES.with(|v| v.borrow().clone());
    match a.first() {
        None => {
            if nargout == 0 {
                return none();
            }
            let keys: Vec<String> = SPPARMS.iter().map(|p| p.0.to_string()).collect();
            let w = 7;
            let mut km = Matrix::zeros(keys.len(), w).with_kind(Kind::Char);
            for (r, k) in keys.iter().enumerate() {
                for (c, ch) in format!("{k:<w$}").chars().enumerate() {
                    km.set(r, c, ch as u32 as f64);
                }
            }
            let vals = Value::Num(Matrix::col(all()));
            Ok(Some(if nargout >= 2 { vec![Value::Num(km), vals] } else { vec![vals] }))
        }
        Some(v) if v.is_text() => {
            let key = v.text()?.to_lowercase();
            if key == "default" {
                SPPARM_VALUES.with(|v| *v.borrow_mut() = SPPARMS.iter().map(|p| p.1).collect());
                return none();
            }
            let Some(k) = SPPARMS.iter().position(|p| p.0 == key) else { return rt(format!("Unknown parameter '{key}'.")) };
            match a.get(1) {
                Some(x) => {
                    let x = x.scalar()?;
                    SPPARM_VALUES.with(|v| v.borrow_mut()[k] = x);
                    none()
                }
                None => one(Value::num(all()[k])),
            }
        }
        // spparms(values): all of them at once.
        Some(v) => {
            let m = v.to_matrix()?;
            if m.numel() != SPPARMS.len() {
                return rt(format!("The value vector must have {} elements.", SPPARMS.len()));
            }
            SPPARM_VALUES.with(|vv| *vv.borrow_mut() = m.data.clone());
            none()
        }
    }
}

fn mat(v: &Value) -> R<Matrix> {
    v.to_matrix()
}

fn nnz_of(m: &Matrix) -> usize {
    m.data.iter().filter(|&&x| x != 0.0).count()
}

// ---- construction --------------------------------------------------------------------------

fn sparse_fn(a: &[Value]) -> Ret {
    match a.len() {
        0 => rt("Not enough input arguments."),
        1 => {
            let mut m = mat(&a[0])?;
            if m.kind == Kind::Char || matches!(m.kind, Kind::Int(_)) {
                return rt("Function 'sparse' is not supported for this data type.");
            }
            if m.kind == Kind::Single {
                m = m.cast_to(Kind::Double);
            }
            one(sp(m))
        }
        2 => {
            let (r, c) = (a[0].as_usize()?, a[1].as_usize()?);
            check_size(r, c)?;
            one(sp(Matrix::zeros(r, c)))
        }
        _ => {
            let (i, j, v) = (mat(&a[0])?, mat(&a[1])?, mat(&a[2])?);
            let n = i.numel().max(j.numel()).max(v.numel());
            let pick = |m: &Matrix, k: usize| m.data[if m.numel() == 1 { 0 } else { k }];
            for m in [&i, &j, &v] {
                if m.numel() != 1 && m.numel() != n {
                    return rt("Vectors must be the same length.");
                }
            }
            let rows = match a.get(3) {
                Some(x) => x.as_usize()?,
                None => (0..n).map(|k| pick(&i, k) as usize).max().unwrap_or(0),
            };
            let cols = match a.get(4) {
                Some(x) => x.as_usize()?,
                None => (0..n).map(|k| pick(&j, k) as usize).max().unwrap_or(0),
            };
            check_size(rows, cols)?;
            let mut out = Matrix::zeros(rows, cols);
            for k in 0..n {
                let (ri, ci) = (pick(&i, k), pick(&j, k));
                if ri < 1.0 || ci < 1.0 || ri.fract() != 0.0 || ci.fract() != 0.0 {
                    return rt("Index into matrix must be positive.");
                }
                let (ri, ci) = (ri as usize - 1, ci as usize - 1);
                if ri >= rows || ci >= cols {
                    return rt("Index exceeds matrix dimensions.");
                }
                // Repeated subscripts add up (logical values combine with or).
                if v.kind == Kind::Logical {
                    if pick(&v, k) != 0.0 {
                        out.data[ci * rows + ri] = 1.0;
                    }
                } else {
                    out.data[ci * rows + ri] += pick(&v, k);
                }
            }
            if v.kind == Kind::Logical {
                out.kind = Kind::Logical;
            }
            one(sp(out))
        }
    }
}

fn speye(a: &[Value]) -> Ret {
    let (r, c) = match a {
        [] => (1, 1),
        [n] => {
            let m = mat(n)?;
            if m.numel() == 2 { (m.data[0] as usize, m.data[1] as usize) } else { (n.as_usize()?, n.as_usize()?) }
        }
        [r, c, ..] => (r.as_usize()?, c.as_usize()?),
    };
    check_size(r, c)?;
    let mut m = Matrix::zeros(r, c);
    for k in 0..r.min(c) {
        m.data[k * r + k] = 1.0;
    }
    one(sp(m))
}

/// `spdiags`: build from diagonals (B, d, m, n), replace diagonals of A (B, d, A), or extract
/// them (A) / (A, d).
fn spdiags(a: &[Value]) -> Ret {
    let place = |m: usize, n: usize, j: usize, i: usize, k: usize, b: &Matrix| -> f64 {
        // For m >= n the B column is indexed by the column of A, otherwise by the row.
        let idx = if m >= n { j } else { i };
        if idx < b.rows() { b.data[k * b.rows() + idx] } else { 0.0 }
    };
    match a.len() {
        4 | 3 if a.len() == 4 || matches!(a.get(2), Some(Value::Num(x)) if x.numel() > 1) => {
            let b = mat(&a[0])?;
            let d: Vec<i64> = mat(&a[1])?.data.iter().map(|&x| x as i64).collect();
            let mut target = if a.len() == 4 {
                let (m, n) = (a[2].as_usize()?, a[3].as_usize()?);
                check_size(m, n)?;
                Matrix::zeros(m, n)
            } else {
                let mut t = mat(&a[2])?;
                t.sparse = false;
                t
            };
            let (m, n) = (target.rows(), target.cols());
            for (k, &dk) in d.iter().enumerate() {
                for j in 0..n {
                    let i = j as i64 - dk;
                    if i < 0 || i as usize >= m {
                        continue;
                    }
                    let i = i as usize;
                    target.data[j * m + i] = if b.cols() > k { place(m, n, j, i, k, &b) } else { 0.0 };
                }
            }
            one(sp(target))
        }
        1 | 2 => {
            let src = mat(&a[0])?;
            let (m, n) = (src.rows(), src.cols());
            let d: Vec<i64> = match a.get(1) {
                Some(v) => mat(v)?.data.iter().map(|&x| x as i64).collect(),
                None => (-(m as i64 - 1)..n as i64).filter(|&dk| (0..n).any(|j| { let i = j as i64 - dk; i >= 0 && (i as usize) < m && src.data[j * m + i as usize] != 0.0 })).collect(),
            };
            let len = m.min(n);
            let mut b = vec![0.0; len * d.len()];
            for (k, &dk) in d.iter().enumerate() {
                for j in 0..n {
                    let i = j as i64 - dk;
                    if i < 0 || i as usize >= m {
                        continue;
                    }
                    let i = i as usize;
                    let idx = if m >= n { j } else { i };
                    if idx < len {
                        b[k * len + idx] = src.data[j * m + i];
                    }
                }
            }
            let bm = Matrix::new(len, d.len(), b);
            Ok(Some(vec![Value::Num(bm), Value::Num(Matrix::col(d.iter().map(|&x| x as f64).collect()))]))
        }
        _ => rt("Invalid spdiags arguments."),
    }
}

fn sprand(it: &mut Interpreter, name: &str, a: &[Value]) -> Ret {
    // sprand(S): random values in the pattern of S.
    if a.len() == 1 {
        let s = mat(&a[0])?;
        let n = s.numel();
        let rnd = it.call_function(if name == "sprandn" { "randn" } else { "rand" }, vec![Value::num(n as f64), Value::num(1.0)], 1)?.remove(0).to_matrix()?;
        let mut out = Matrix::zeros(s.rows(), s.cols());
        for k in 0..n {
            if s.data[k] != 0.0 {
                out.data[k] = rnd.data[k];
            }
        }
        return one(sp(out));
    }
    let (m, n) = if name == "sprandsym" { (a[0].as_usize()?, a[0].as_usize()?) } else { (a[0].as_usize()?, arg(a, 1, name)?.as_usize()?) };
    let density = if name == "sprandsym" { arg(a, 1, name)?.scalar()? } else { arg(a, 2, name)?.scalar()? };
    check_size(m, n)?;
    let total = m * n;
    let want = ((density.clamp(0.0, 1.0) * total as f64).round() as usize).min(total);
    let pos = it.call_function("randperm", vec![Value::num(total as f64), Value::num(want as f64)], 1)?.remove(0).to_matrix()?;
    let vals = it.call_function(if name == "sprand" { "rand" } else { "randn" }, vec![Value::num(want as f64), Value::num(1.0)], 1)?.remove(0).to_matrix()?;
    let mut out = Matrix::zeros(m, n);
    for (k, &p) in pos.data.iter().enumerate() {
        out.data[p as usize - 1] = vals.data[k];
    }
    if name == "sprandsym" {
        for j in 0..n {
            for i in 0..j {
                let v = out.data[j * m + i];
                out.data[i * m + j] = v;
            }
        }
    }
    one(sp(out))
}

/// Structural rank: a maximum matching between rows and columns of the nonzero pattern.
fn sprank(m: &Matrix) -> usize {
    let (r, c) = (m.rows(), m.cols());
    let mut row_of = vec![usize::MAX; c];
    let adj: Vec<Vec<usize>> = (0..r).map(|i| (0..c).filter(|&j| m.data[j * r + i] != 0.0).collect()).collect();
    fn augment(i: usize, adj: &[Vec<usize>], seen: &mut [bool], row_of: &mut [usize]) -> bool {
        for &j in &adj[i] {
            if !seen[j] {
                seen[j] = true;
                if row_of[j] == usize::MAX || augment(row_of[j], adj, seen, row_of) {
                    row_of[j] = i;
                    return true;
                }
            }
        }
        false
    }
    (0..r).filter(|&i| augment(i, &adj, &mut vec![false; c], &mut row_of)).count()
}

/// `numgrid(region, n)`: interior points of a region on an n-by-n grid over [-1, 1], numbered
/// by columns.
fn numgrid(a: &[Value]) -> Ret {
    let region = arg(a, 0, "numgrid")?.text()?;
    let n = arg(a, 1, "numgrid")?.as_usize()?;
    let h = if n > 1 { 2.0 / (n - 1) as f64 } else { 0.0 };
    let at = |k: usize| -1.0 + k as f64 * h;
    let inside = |i: usize, j: usize| -> R<bool> {
        let (x, y) = (at(j), at(i));
        let interior = i > 0 && j > 0 && i + 1 < n && j + 1 < n;
        Ok(interior
            && match region.as_str() {
                "S" => true,
                "L" => !(x < 0.0 && y > 0.0),
                "C" => x * x + y * y < 1.0 && !(x < 0.0 && y > 0.0),
                "D" => x * x + y * y < 1.0,
                "A" => {
                    let r2 = x * x + y * y;
                    r2 < 1.0 && r2 > 1.0 / 3.0
                }
                other => return rt(format!("Region '{other}' is not supported in this version; use 'S', 'L', 'C', 'D' or 'A'.")),
            })
    };
    let mut g = vec![0.0; n * n];
    let mut k = 0.0;
    for j in 0..n {
        for i in 0..n {
            if inside(i, j)? {
                k += 1.0;
                g[j * n + i] = k;
            }
        }
    }
    one(Value::Num(Matrix::new(n, n, g)))
}

/// `delsq(G)`: the five-point Laplacian on the points numbered in G.
fn delsq(a: &[Value]) -> Ret {
    let g = mat(arg(a, 0, "delsq")?)?;
    let (r, c) = (g.rows(), g.cols());
    let n = g.data.iter().fold(0.0f64, |m, &x| m.max(x)) as usize;
    check_size(n, n)?;
    let mut out = Matrix::zeros(n, n);
    for j in 0..c {
        for i in 0..r {
            let p = g.data[j * r + i] as usize;
            if p == 0 {
                continue;
            }
            out.data[(p - 1) * n + p - 1] = 4.0;
            let nb = [(i.wrapping_sub(1), j), (i + 1, j), (i, j.wrapping_sub(1)), (i, j + 1)];
            for (ni, nj) in nb {
                if ni < r && nj < c {
                    let q = g.data[nj * r + ni] as usize;
                    if q > 0 {
                        out.data[(q - 1) * n + p - 1] = -1.0;
                    }
                }
            }
        }
    }
    one(sp(out))
}

// ---- iterative solvers ---------------------------------------------------------------------

/// The operator of a solver: a matrix or a function handle computing A*x (and A'*x).
enum Op {
    M(Matrix),
    /// A matrix with few nonzeros, in compressed sparse column form (rows sorted in each column).
    S(std::sync::Arc<crate::value::Csc>),
    F(Value),
}

impl Op {
    /// The operator of a matrix: compressed sparse columns when fewer than a fifth of the
    /// elements are nonzero (every product then costs the nonzeros, not rows x columns).
    fn from_matrix(m: Matrix) -> Op {
        Op::from_ref(&m)
    }

    /// As `from_matrix`, without copying the matrix when it becomes compressed columns.
    fn from_ref(m: &Matrix) -> Op {
        match crate::ops::csc_of(m) {
            Some(c) => Op::S(c),
            None => Op::M(m.clone()),
        }
    }

    fn apply(&self, it: &mut Interpreter, x: &[f64], transpose: bool) -> R<Vec<f64>> {
        match self {
            Op::S(c) => Ok(if transpose {
                let (cols, ptr, idx, val) = (c.cols, &c.ptr, &c.idx, &c.val);
                (0..cols).map(|j| (ptr[j]..ptr[j + 1]).map(|e| val[e] * x[idx[e] as usize]).sum()).collect()
            } else {
                let (rows, cols, ptr, idx, val) = (c.rows, c.cols, &c.ptr, &c.idx, &c.val);
                let mut y = vec![0.0; rows];
                for j in 0..cols {
                    let xj = x[j];
                    if xj != 0.0 {
                        for e in ptr[j]..ptr[j + 1] {
                            y[idx[e] as usize] += val[e] * xj;
                        }
                    }
                }
                y
            }),
            Op::M(m) => {
                let (r, c) = (m.rows(), m.cols());
                Ok(if transpose {
                    (0..c).map(|j| (0..r).map(|i| m.data[j * r + i] * x[i]).sum()).collect()
                } else {
                    let mut y = vec![0.0; r];
                    for j in 0..c {
                        if x[j] != 0.0 {
                            for i in 0..r {
                                y[i] += m.data[j * r + i] * x[j];
                            }
                        }
                    }
                    y
                })
            }
            Op::F(f) => {
                let mut args = vec![Value::Num(Matrix::col(x.to_vec()))];
                if transpose {
                    args.push(Value::chars("transp"));
                }
                Ok(call_handle(it, f, args, 1)?.remove(0).to_matrix()?.data)
            }
        }
    }
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm(a: &[f64]) -> f64 {
    dot(a, a).sqrt()
}

fn axpy(y: &mut [f64], a: f64, x: &[f64]) {
    for (yi, xi) in y.iter_mut().zip(x) {
        *yi += a * xi;
    }
}

/// A preconditioner M = M1*M2 (or a function): solves M z = r.
struct Precond(Vec<Matrix>, Option<Value>);

impl Precond {
    fn solve(&self, it: &mut Interpreter, r: &[f64]) -> R<Vec<f64>> {
        if let Some(f) = &self.1 {
            return Ok(call_handle(it, f, vec![Value::Num(Matrix::col(r.to_vec()))], 1)?.remove(0).to_matrix()?.data);
        }
        let mut z = r.to_vec();
        for m in &self.0 {
            let x = crate::builtins::call(it, "mldivide", vec![Value::Num(m.clone()), Value::Num(Matrix::col(z.clone()))], 1)?
                .and_then(|mut v| v.pop())
                .ok_or_else(|| LangError::new("Preconditioner solve failed.", 0))?;
            z = x.to_matrix()?.data;
        }
        Ok(z)
    }
}

struct Solve {
    x: Vec<f64>,
    flag: u8,
    relres: f64,
    iter: f64,
    resvec: Vec<f64>,
}

fn report(it: &mut Interpreter, name: &str, s: &Solve, tol: f64, maxit: usize, nargout: usize) {
    if nargout >= 2 {
        return;
    }
    let iter = crate::display::fmt_g(s.iter, 5);
    let rr = crate::display::fmt_g(s.relres, 2);
    let msg = if s.flag == 0 {
        format!("{name} converged at iteration {iter} to a solution with relative residual {rr}.\n")
    } else {
        format!(
            "{name} stopped at iteration {maxit} without converging to the desired tolerance {}\nbecause the maximum number of iterations was reached.\nThe iterate returned (number {iter}) has relative residual {rr}.\n",
            crate::display::fmt_g(tol, 2)
        )
    };
    it.emit(OutputKind::Text(msg));
}

fn solver(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    let op = match arg(a, 0, name)? {
        f @ Value::Func(_) => Op::F(f.clone()),
        Value::Num(m) => Op::from_ref(m),
        v => Op::from_matrix(mat(v)?),
    };
    let b = mat(arg(a, 1, name)?)?.data;
    let n = b.len();
    // gmres(A, b, restart, tol, maxit, ...); the others (A, b, tol, maxit, M1, M2, x0).
    let shift = if name == "gmres" { 1 } else { 0 };
    let restart = if name == "gmres" { a.get(2).filter(|v| v.numel() > 0).map(|v| v.as_usize()).transpose()?.unwrap_or(n) } else { n };
    let tol = a.get(2 + shift).filter(|v| v.numel() > 0).map(|v| v.scalar()).transpose()?.unwrap_or(1e-6);
    let maxit = a.get(3 + shift).filter(|v| v.numel() > 0).map(|v| v.as_usize()).transpose()?.unwrap_or(if name == "gmres" { n.div_ceil(restart).min(10).max(1) } else { n.min(20) });
    let mut pm = Vec::new();
    let mut pf = None;
    for k in [4 + shift, 5 + shift] {
        match a.get(k) {
            Some(f @ Value::Func(_)) => pf = Some(f.clone()),
            Some(v) if v.numel() > 0 => pm.push(mat(v)?),
            _ => {}
        }
    }
    let pre = Precond(pm, pf);
    let x0 = a.get(6 + shift).filter(|v| v.numel() > 0).map(mat).transpose()?.map(|m| m.data).unwrap_or_else(|| vec![0.0; n]);
    let nb = norm(&b);
    let nb = if nb == 0.0 { 1.0 } else { nb };
    let s = match name {
        "pcg" | "cgs" | "bicg" | "bicgstab" | "bicgstabl" | "qmr" | "tfqmr" | "minres" | "symmlq" => krylov(it, name, &op, &b, &pre, x0, tol, maxit, nb)?,
        "gmres" => gmres(it, &op, &b, &pre, x0, tol, restart, maxit, nb)?,
        _ => lsqr(it, &op, &b, x0, tol, maxit, nb)?,
    };
    report(it, name, &s, tol, if name == "gmres" { maxit * restart } else { maxit }, nargout);
    let iter = if name == "gmres" { Value::Num(Matrix::row(vec![((s.iter - 1.0) / restart as f64).floor() + 1.0, ((s.iter - 1.0) % restart as f64) + 1.0])) } else { Value::num(s.iter) };
    Ok(Some(vec![Value::Num(Matrix::col(s.x)), Value::num(s.flag as f64), Value::num(s.relres), iter, Value::Num(Matrix::col(s.resvec))]))
}

#[allow(clippy::too_many_arguments)]
fn krylov(it: &mut Interpreter, name: &str, op: &Op, b: &[f64], pre: &Precond, mut x: Vec<f64>, tol: f64, maxit: usize, nb: f64) -> R<Solve> {
    let n = b.len();
    let ax = op.apply(it, &x, false)?;
    let mut r: Vec<f64> = b.iter().zip(&ax).map(|(bi, ai)| bi - ai).collect();
    let mut resvec = vec![norm(&r)];
    if norm(&r) / nb <= tol {
        return Ok(Solve { relres: norm(&r) / nb, x, flag: 0, iter: 0.0, resvec });
    }
    let symmetric = matches!(name, "pcg" | "minres" | "symmlq");
    let rt0 = r.clone();
    let (mut p, mut v, mut rho_old, mut alpha, mut omega) = (vec![0.0; n], vec![0.0; n], 1.0, 1.0, 1.0);
    let mut z_old = vec![0.0; n];
    let mut best = (f64::INFINITY, x.clone(), 0.0);
    for k in 1..=maxit {
        if symmetric {
            // Preconditioned conjugate gradients (also used for minres and symmlq on SPD systems).
            let z = pre.solve(it, &r)?;
            let rho = dot(&r, &z);
            if k == 1 {
                p = z.clone();
            } else {
                let beta = rho / rho_old;
                p = z.iter().zip(&p).map(|(zi, pi)| zi + beta * pi).collect();
            }
            let q = op.apply(it, &p, false)?;
            let pq = dot(&p, &q);
            if pq == 0.0 {
                break;
            }
            let a = rho / pq;
            axpy(&mut x, a, &p);
            axpy(&mut r, -a, &q);
            rho_old = rho;
            z_old = z;
        } else {
            // BiCGSTAB-style step (used for the nonsymmetric methods).
            let rho = dot(&rt0, &r);
            if rho == 0.0 {
                break;
            }
            if k == 1 {
                p = r.clone();
            } else {
                let beta = (rho / rho_old) * (alpha / omega);
                p = r.iter().zip(p.iter().zip(&v)).map(|(ri, (pi, vi))| ri + beta * (pi - omega * vi)).collect();
            }
            let ph = pre.solve(it, &p)?;
            v = op.apply(it, &ph, false)?;
            alpha = rho / dot(&rt0, &v);
            let s: Vec<f64> = r.iter().zip(&v).map(|(ri, vi)| ri - alpha * vi).collect();
            let sh = pre.solve(it, &s)?;
            let t = op.apply(it, &sh, false)?;
            let tt = dot(&t, &t);
            omega = if tt == 0.0 { 0.0 } else { dot(&t, &s) / tt };
            axpy(&mut x, alpha, &ph);
            axpy(&mut x, omega, &sh);
            r = s.iter().zip(&t).map(|(si, ti)| si - omega * ti).collect();
            rho_old = rho;
        }
        // The true residual decides convergence.
        let ax = op.apply(it, &x, false)?;
        let res: Vec<f64> = b.iter().zip(&ax).map(|(bi, ai)| bi - ai).collect();
        let rel = norm(&res) / nb;
        resvec.push(norm(&res));
        if rel < best.0 {
            best = (rel, x.clone(), k as f64);
        }
        if rel <= tol {
            let iter = if name == "bicgstab" { k as f64 - 0.5 } else { k as f64 };
            return Ok(Solve { x, flag: 0, relres: rel, iter, resvec });
        }
    }
    let _ = z_old;
    Ok(Solve { relres: best.0, x: best.1, flag: 1, iter: best.2, resvec })
}

#[allow(clippy::too_many_arguments)]
fn gmres(it: &mut Interpreter, op: &Op, b: &[f64], pre: &Precond, mut x: Vec<f64>, tol: f64, restart: usize, maxit: usize, nb: f64) -> R<Solve> {
    let n = b.len();
    let mut resvec = Vec::new();
    let mut total = 0usize;
    for _outer in 0..maxit {
        let ax = op.apply(it, &x, false)?;
        let r0: Vec<f64> = b.iter().zip(&ax).map(|(bi, ai)| bi - ai).collect();
        let r = pre.solve(it, &r0)?;
        let beta = norm(&r);
        resvec.push(norm(&r0));
        if norm(&r0) / nb <= tol {
            return Ok(Solve { x, flag: 0, relres: norm(&r0) / nb, iter: total as f64, resvec });
        }
        let m = restart.min(n);
        let mut v: Vec<Vec<f64>> = vec![r.iter().map(|x| x / beta).collect()];
        let mut h = vec![vec![0.0; m]; m + 1];
        let (mut cs, mut sn) = (vec![0.0; m], vec![0.0; m]);
        let mut g = vec![0.0; m + 1];
        g[0] = beta;
        let mut kdone = 0;
        for j in 0..m {
            total += 1;
            let av = op.apply(it, &v[j], false)?;
            let mut w = pre.solve(it, &av)?;
            for i in 0..=j {
                h[i][j] = dot(&w, &v[i]);
                let hij = h[i][j];
                axpy(&mut w, -hij, &v[i]);
            }
            h[j + 1][j] = norm(&w);
            for i in 0..j {
                let t = cs[i] * h[i][j] + sn[i] * h[i + 1][j];
                h[i + 1][j] = -sn[i] * h[i][j] + cs[i] * h[i + 1][j];
                h[i][j] = t;
            }
            let d = h[j][j].hypot(h[j + 1][j]);
            cs[j] = if d == 0.0 { 1.0 } else { h[j][j] / d };
            sn[j] = if d == 0.0 { 0.0 } else { h[j + 1][j] / d };
            h[j][j] = d;
            h[j + 1][j] = 0.0;
            g[j + 1] = -sn[j] * g[j];
            g[j] *= cs[j];
            kdone = j + 1;
            let nw = norm(&w);
            v.push(if nw > 0.0 { w.iter().map(|x| x / nw).collect() } else { w });
            // Solve the small system and check the true residual.
            let mut y = vec![0.0; kdone];
            for i in (0..kdone).rev() {
                let s: f64 = (i + 1..kdone).map(|l| h[i][l] * y[l]).sum();
                y[i] = (g[i] - s) / h[i][i];
            }
            let mut xt = x.clone();
            for (i, yi) in y.iter().enumerate() {
                axpy(&mut xt, *yi, &v[i]);
            }
            let axt = op.apply(it, &xt, false)?;
            let res = norm(&b.iter().zip(&axt).map(|(bi, ai)| bi - ai).collect::<Vec<_>>());
            resvec.push(res);
            if res / nb <= tol || g[j + 1].abs() < 1e-300 {
                return Ok(Solve { x: xt, flag: 0, relres: res / nb, iter: total as f64, resvec });
            }
        }
        let mut y = vec![0.0; kdone];
        for i in (0..kdone).rev() {
            let s: f64 = (i + 1..kdone).map(|l| h[i][l] * y[l]).sum();
            y[i] = (g[i] - s) / h[i][i];
        }
        for (i, yi) in y.iter().enumerate() {
            axpy(&mut x, *yi, &v[i]);
        }
    }
    let ax = op.apply(it, &x, false)?;
    let rel = norm(&b.iter().zip(&ax).map(|(bi, ai)| bi - ai).collect::<Vec<_>>()) / nb;
    Ok(Solve { x, flag: 1, relres: rel, iter: total as f64, resvec })
}

/// LSQR (Paige and Saunders) for least squares.
fn lsqr(it: &mut Interpreter, op: &Op, b: &[f64], mut x: Vec<f64>, tol: f64, maxit: usize, nb: f64) -> R<Solve> {
    let ax = op.apply(it, &x, false)?;
    let mut u: Vec<f64> = b.iter().zip(&ax).map(|(bi, ai)| bi - ai).collect();
    let mut beta = norm(&u);
    let mut resvec = vec![beta];
    if beta / nb <= tol {
        return Ok(Solve { x, flag: 0, relres: beta / nb, iter: 0.0, resvec });
    }
    u.iter_mut().for_each(|v| *v /= beta);
    let mut v = op.apply(it, &u, true)?;
    let mut alpha = norm(&v);
    v.iter_mut().for_each(|e| *e /= alpha.max(1e-300));
    let mut w = v.clone();
    let (mut phibar, mut rhobar) = (beta, alpha);
    for k in 1..=maxit {
        let av = op.apply(it, &v, false)?;
        u = av.iter().zip(&u).map(|(a, ui)| a - alpha * ui).collect();
        beta = norm(&u);
        if beta > 0.0 {
            u.iter_mut().for_each(|e| *e /= beta);
        }
        let atu = op.apply(it, &u, true)?;
        v = atu.iter().zip(&v).map(|(a, vi)| a - beta * vi).collect();
        alpha = norm(&v);
        if alpha > 0.0 {
            v.iter_mut().for_each(|e| *e /= alpha);
        }
        let rho = rhobar.hypot(beta);
        let (c, s) = (rhobar / rho, beta / rho);
        let theta = s * alpha;
        rhobar = -c * alpha;
        let phi = c * phibar;
        phibar *= s;
        axpy(&mut x, phi / rho, &w);
        w = v.iter().zip(&w).map(|(vi, wi)| vi - (theta / rho) * wi).collect();
        let ax = op.apply(it, &x, false)?;
        let res = norm(&b.iter().zip(&ax).map(|(bi, ai)| bi - ai).collect::<Vec<_>>());
        resvec.push(res);
        if res / nb <= tol {
            return Ok(Solve { x, flag: 0, relres: res / nb, iter: k as f64, resvec });
        }
    }
    let rel = *resvec.last().unwrap() / nb;
    Ok(Solve { x, flag: 1, relres: rel, iter: maxit as f64, resvec })
}

// ---- factorizations and trees --------------------------------------------------------------

/// `ichol(A)`: incomplete Cholesky with no fill (the lower triangle's pattern).
fn ichol(a: &Matrix) -> R<Matrix> {
    let n = a.rows();
    let mut l = Matrix::zeros(n, n);
    for j in 0..n {
        for i in j..n {
            l.data[j * n + i] = a.data[j * n + i];
        }
    }
    for k in 0..n {
        let d = l.data[k * n + k];
        if d <= 0.0 {
            return rt("Encountered nonpositive pivot.");
        }
        let d = d.sqrt();
        l.data[k * n + k] = d;
        for i in k + 1..n {
            if l.data[k * n + i] != 0.0 {
                l.data[k * n + i] /= d;
            }
        }
        for j in k + 1..n {
            let ljk = l.data[k * n + j];
            if ljk == 0.0 {
                continue;
            }
            for i in j..n {
                if a.data[j * n + i] != 0.0 {
                    l.data[j * n + i] -= l.data[k * n + i] * ljk;
                }
            }
        }
    }
    Ok(l)
}

/// `ilu(A)`: incomplete LU with no fill; L unit lower triangular, U upper.
fn ilu(a: &Matrix) -> (Matrix, Matrix) {
    let n = a.rows();
    let mut w = a.data.clone();
    let nz = |i: usize, j: usize| a.data[j * n + i] != 0.0;
    for i in 1..n {
        for k in 0..i {
            if !nz(i, k) {
                continue;
            }
            w[k * n + i] /= w[k * n + k];
            let lik = w[k * n + i];
            for j in k + 1..n {
                if nz(i, j) {
                    w[j * n + i] -= lik * w[j * n + k];
                }
            }
        }
    }
    let mut l = Matrix::zeros(n, n);
    let mut u = Matrix::zeros(n, n);
    for j in 0..n {
        for i in 0..n {
            if i > j {
                l.data[j * n + i] = w[j * n + i];
            } else {
                u.data[j * n + i] = w[j * n + i];
            }
        }
        l.data[j * n + j] = 1.0;
    }
    (l, u)
}

/// The elimination tree of a symmetric matrix (parent of each column, 0 for roots).
fn etree(a: &Matrix) -> Vec<usize> {
    let n = a.cols();
    let r = a.rows();
    let mut parent = vec![0usize; n];
    let mut ancestor = vec![0usize; n];
    for j in 0..n {
        for i in 0..j.min(r) {
            if a.data[j * r + i] == 0.0 {
                continue;
            }
            let mut k = i + 1;
            while ancestor[k - 1] != 0 && ancestor[k - 1] != j + 1 {
                let next = ancestor[k - 1];
                ancestor[k - 1] = j + 1;
                k = next;
            }
            if ancestor[k - 1] == 0 {
                ancestor[k - 1] = j + 1;
                parent[k - 1] = j + 1;
            }
        }
    }
    parent
}

/// Postorder of an elimination tree (`parent` 1-based, 0 at roots): children in increasing
/// order, roots in increasing order.
fn postorder(parent: &[usize]) -> Vec<usize> {
    let n = parent.len();
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (j, &p) in parent.iter().enumerate() {
        if p != 0 {
            kids[p - 1].push(j);
        }
    }
    let mut out = Vec::with_capacity(n);
    fn go(u: usize, kids: &[Vec<usize>], out: &mut Vec<usize>) {
        for &c in &kids[u] {
            go(c, kids, out);
        }
        out.push(u);
    }
    for j in 0..n {
        if parent[j] == 0 {
            go(j, &kids, &mut out);
        }
    }
    out
}

/// Column counts and row patterns of the Cholesky factor's columns.
fn symbfact_pattern(a: &Matrix) -> (Vec<f64>, Vec<std::collections::BTreeSet<usize>>) {
    let n = a.cols();
    let r = a.rows();
    let mut pattern: Vec<std::collections::BTreeSet<usize>> = (0..n).map(|j| (j..n).filter(|&i| i == j || a.data[j * r + i] != 0.0 || a.data[i * r + j] != 0.0).collect()).collect();
    for j in 0..n {
        let below: Vec<usize> = pattern[j].iter().copied().filter(|&i| i > j).collect();
        if let Some(&p) = below.first() {
            for &i in &below {
                pattern[p].insert(i);
            }
        }
    }
    (pattern.iter().map(|s| s.len() as f64).collect(), pattern)
}

/// Reverse Cuthill-McKee ordering from a pseudo-peripheral node of each component.
fn symrcm(a: &Matrix) -> Vec<usize> {
    let n = a.cols();
    let r = a.rows();
    let adj: Vec<Vec<usize>> = (0..n).map(|i| (0..n).filter(|&j| j != i && (a.data[j * r + i] != 0.0 || a.data[i * r + j] != 0.0)).collect()).collect();
    let deg: Vec<usize> = adj.iter().map(|l| l.len()).collect();
    let mut placed = vec![false; n];
    let mut order = Vec::with_capacity(n);
    let bfs_levels = |s: usize, placed: &[bool]| -> Vec<Vec<usize>> {
        let mut seen = placed.to_vec();
        seen[s] = true;
        let mut levels = vec![vec![s]];
        loop {
            let mut next = Vec::new();
            for &u in levels.last().unwrap() {
                for &v in &adj[u] {
                    if !seen[v] {
                        seen[v] = true;
                        next.push(v);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            levels.push(next);
        }
        levels
    };
    while let Some(start) = (0..n).filter(|&v| !placed[v]).min_by_key(|&v| (deg[v], v)) {
        // Pseudo-peripheral node: repeat BFS from a minimum-degree node of the last level.
        let mut s = start;
        let mut height = bfs_levels(s, &placed).len();
        loop {
            let levels = bfs_levels(s, &placed);
            let cand = *levels.last().unwrap().iter().min_by_key(|&&v| (deg[v], v)).unwrap();
            let h = bfs_levels(cand, &placed).len();
            s = cand;
            if h > height {
                height = h;
            } else {
                break;
            }
        }
        let mut queue = std::collections::VecDeque::from([s]);
        placed[s] = true;
        while let Some(u) = queue.pop_front() {
            order.push(u);
            let mut next: Vec<usize> = adj[u].iter().copied().filter(|&v| !placed[v]).collect();
            next.sort_by_key(|&v| (deg[v], v));
            for v in next {
                placed[v] = true;
                queue.push_back(v);
            }
        }
    }
    order.reverse();
    order
}

/// `treelayout(parent)`: x by leaf order (children in index order), y by depth.
fn treelayout(parent: &[usize]) -> (Vec<f64>, Vec<f64>, usize) {
    let n = parent.len();
    let mut children = vec![Vec::new(); n];
    let mut roots = Vec::new();
    for (k, &p) in parent.iter().enumerate() {
        if p == 0 { roots.push(k) } else { children[p - 1].push(k) }
    }
    let mut depth = vec![0usize; n];
    let mut x = vec![0.0; n];
    let mut leaf = 0.0;
    fn place(u: usize, d: usize, children: &[Vec<usize>], depth: &mut [usize], x: &mut [f64], leaf: &mut f64) {
        depth[u] = d;
        if children[u].is_empty() {
            *leaf += 1.0;
            x[u] = *leaf;
            return;
        }
        let first = *leaf;
        for &c in &children[u] {
            place(c, d + 1, children, depth, x, leaf);
        }
        // An inner node sits over the middle of its leaves.
        x[u] = (first + 1.0 + *leaf) / 2.0;
    }
    for &r in &roots {
        place(r, 0, &children, &mut depth, &mut x, &mut leaf);
    }
    let height = depth.iter().max().copied().unwrap_or(0);
    let leaves = leaf;
    let xs = x.iter().map(|v| v / (leaves + 1.0)).collect();
    // Leaves sit on the bottom level, inner nodes by their depth.
    let ys = (0..n).map(|k| if children[k].is_empty() { 1.0 } else { (height + 1 - depth[k]) as f64 } / (height as f64 + 2.0)).collect();
    (xs, ys, height)
}

fn tree_lines(it: &mut Interpreter, parent: &[usize], nargout: usize) -> Ret {
    let (x, y, _) = treelayout(parent);
    let (mut lx, mut ly) = (Vec::new(), Vec::new());
    for (k, &p) in parent.iter().enumerate() {
        if p > 0 {
            lx.extend([x[k], x[p - 1], f64::NAN]);
            ly.extend([y[k], y[p - 1], f64::NAN]);
        }
    }
    let nodes = Value::Num(Matrix::row(x));
    let ys = Value::Num(Matrix::row(y));
    it.call_function("plot", vec![nodes, ys, Value::chars("ro"), Value::Num(Matrix::row(lx)), Value::Num(Matrix::row(ly)), Value::chars("b-")], nargout)?;
    it.call_function("xlabel", vec![Value::chars(&format!("height = {}", parent.len()))], 0)?;
    Ok(Some(Vec::new()))
}

fn spy(it: &mut Interpreter, a: &[Value]) -> Ret {
    let m = mat(arg(a, 0, "spy")?)?;
    let r = m.rows();
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for k in 0..m.data.len() {
        if m.data[k] != 0.0 {
            xs.push((k / r + 1) as f64);
            ys.push((k % r + 1) as f64);
        }
    }
    let spec = a.iter().skip(1).find(|v| v.is_text()).cloned().unwrap_or_else(|| Value::chars("."));
    it.call_function("plot", vec![Value::Num(Matrix::row(xs)), Value::Num(Matrix::row(ys)), spec], 0)?;
    it.call_function("axis", vec![Value::chars("ij")], 0)?;
    it.call_function("xlim", vec![Value::Num(Matrix::row(vec![0.0, m.cols() as f64 + 1.0]))], 0)?;
    it.call_function("ylim", vec![Value::Num(Matrix::row(vec![0.0, r as f64 + 1.0]))], 0)?;
    it.call_function("xlabel", vec![Value::chars(&format!("nz = {}", nnz_of(&m)))], 0)?;
    none()
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "sparse" => sparse_fn(a),
        "full" => {
            let v = arg(a, 0, name)?.clone();
            one(match v {
                Value::Num(mut m) => {
                    m.sparse = false;
                    Value::Num(m)
                }
                other => other,
            })
        }
        "issparse" => one(Value::bool(a.first().is_some_and(is_sparse))),
        "speye" => speye(a),
        "spones" => {
            let m = mat(arg(a, 0, name)?)?;
            let data = m.data.iter().map(|&x| if x != 0.0 { 1.0 } else { 0.0 }).collect();
            one(sp(Matrix::from_dims(m.dims.clone(), data)))
        }
        "spfun" => {
            let f = arg(a, 0, name)?.clone();
            let m = mat(arg(a, 1, name)?)?;
            let idx: Vec<usize> = (0..m.data.len()).filter(|&k| m.data[k] != 0.0).collect();
            let vals = Matrix::col(idx.iter().map(|&k| m.data[k]).collect());
            let out = call_handle(it, &f, vec![Value::Num(vals)], 1)?.remove(0).to_matrix()?;
            let mut res = Matrix::from_dims(m.dims.clone(), vec![0.0; m.data.len()]);
            for (i, &k) in idx.iter().enumerate() {
                res.data[k] = out.data[i];
            }
            one(sp(res))
        }
        "nonzeros" => {
            let m = mat(arg(a, 0, name)?)?;
            one(Value::Num(Matrix::col(m.data.iter().copied().filter(|&x| x != 0.0).collect())))
        }
        "nzmax" => {
            let m = mat(arg(a, 0, name)?)?;
            let n = nnz_of(&m);
            one(Value::num(if m.sparse { n.max(1) } else { m.numel() } as f64))
        }
        "spalloc" => {
            let (r, c) = (arg(a, 0, name)?.as_usize()?, arg(a, 1, name)?.as_usize()?);
            check_size(r, c)?;
            one(sp(Matrix::zeros(r, c)))
        }
        "spconvert" => {
            let d = mat(arg(a, 0, name)?)?;
            let r = d.rows();
            if d.cols() < 3 {
                return rt("Input must have three or four columns.");
            }
            let col = |k: usize| Value::Num(Matrix::col(d.data[k * r..(k + 1) * r].to_vec()));
            sparse_fn(&[col(0), col(1), col(2)])
        }
        "spdiags" => spdiags(a),
        "sprand" | "sprandn" | "sprandsym" => sprand(it, name, a),
        "spaugment" => {
            let m = mat(arg(a, 0, name)?)?;
            let c = a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(1.0);
            let (r, k) = (m.rows(), m.cols());
            let n = r + k;
            let mut out = Matrix::zeros(n, n);
            for i in 0..r {
                out.data[i * n + i] = c;
            }
            for j in 0..k {
                for i in 0..r {
                    let v = m.data[j * r + i];
                    out.data[(r + j) * n + i] = v;
                    out.data[i * n + r + j] = v;
                }
            }
            one(sp(out))
        }
        "sprank" => one(Value::num(sprank(&mat(arg(a, 0, name)?)?) as f64)),
        "spparms" => spparms(a, nargout),
        "spy" => spy(it, a),
        "gplot" => {
            let m = mat(arg(a, 0, name)?)?;
            let xy = mat(arg(a, 1, name)?)?;
            let (n, r) = (m.cols(), xy.rows());
            let (mut lx, mut ly) = (Vec::new(), Vec::new());
            for j in 0..n {
                for i in 0..m.rows() {
                    if m.data[j * m.rows() + i] != 0.0 {
                        lx.extend([xy.data[i], xy.data[j], f64::NAN]);
                        ly.extend([xy.data[r + i], xy.data[r + j], f64::NAN]);
                    }
                }
            }
            // [X,Y] = gplot(...) returns the segments (NaN-separated) without drawing.
            if nargout >= 1 {
                return Ok(Some(vec![Value::Num(Matrix::col(lx)), Value::Num(Matrix::col(ly))]));
            }
            let mut args = vec![Value::Num(Matrix::row(lx)), Value::Num(Matrix::row(ly))];
            if let Some(s) = a.get(2) {
                args.push(s.clone());
            }
            it.call_function("plot", args, nargout)?;
            none()
        }
        "delsq" => delsq(a),
        "numgrid" => numgrid(a),
        "pcg" | "gmres" | "bicg" | "bicgstab" | "bicgstabl" | "cgs" | "minres" | "lsqr" | "qmr" | "tfqmr" | "symmlq" => solver(it, name, a, nargout),
        "ichol" => one(sp(ichol(&mat(arg(a, 0, name)?)?)?)),
        "ilu" => {
            let (l, u) = ilu(&mat(arg(a, 0, name)?)?);
            if nargout <= 1 {
                // A single output holds L+U-I.
                let n = l.rows();
                let mut w = u.clone();
                for j in 0..n {
                    for i in j + 1..n {
                        w.data[j * n + i] = l.data[j * n + i];
                    }
                }
                return one(sp(w));
            }
            // No pivoting (nofill), so P is the identity.
            let n = l.rows();
            let mut p = Matrix::zeros(n, n);
            for i in 0..n {
                p.set(i, i, 1.0);
            }
            Ok(Some(vec![sp(l), sp(u), sp(p)]))
        }
        "etree" => {
            let parent = etree(&mat(arg(a, 0, name)?)?);
            let post = postorder(&parent);
            Ok(Some(vec![Value::Num(Matrix::row(parent.iter().map(|&p| p as f64).collect())), Value::Num(Matrix::row(post.iter().map(|&k| (k + 1) as f64).collect()))]))
        }
        "symbfact" => {
            let m = mat(arg(a, 0, name)?)?;
            let n = m.cols();
            let (counts, pattern) = symbfact_pattern(&m);
            if nargout <= 1 {
                return one(Value::Num(Matrix::row(counts)));
            }
            let parent = etree(&m);
            let post = postorder(&parent);
            // Height: the most nodes on a path from a leaf to a root.
            let mut depth = vec![0usize; n];
            let mut h = 0;
            for j in (0..n).rev() {
                depth[j] = if parent[j] == 0 { 1 } else { depth[parent[j] - 1] + 1 };
                h = h.max(depth[j]);
            }
            let mut r = Matrix::zeros(n, n);
            for (j, rows) in pattern.iter().enumerate() {
                for &i in rows {
                    r.set(j, i, 1.0);
                }
            }
            Ok(Some(vec![
                Value::Num(Matrix::row(counts)),
                Value::num(h as f64),
                Value::Num(Matrix::row(parent.iter().map(|&p| p as f64).collect())),
                Value::Num(Matrix::row(post.iter().map(|&k| (k + 1) as f64).collect())),
                sp(r),
            ]))
        }
        "colperm" => {
            let m = mat(arg(a, 0, name)?)?;
            let r = m.rows();
            let mut idx: Vec<usize> = (0..m.cols()).collect();
            idx.sort_by_key(|&j| (0..r).filter(|&i| m.data[j * r + i] != 0.0).count());
            one(Value::Num(Matrix::row(idx.iter().map(|&j| (j + 1) as f64).collect())))
        }
        "symrcm" => one(Value::Num(Matrix::row(symrcm(&mat(arg(a, 0, name)?)?).iter().map(|&j| (j + 1) as f64).collect()))),
        "treelayout" => {
            let p: Vec<usize> = mat(arg(a, 0, name)?)?.data.iter().map(|&x| x as usize).collect();
            let (x, y, h) = treelayout(&p);
            Ok(Some(vec![Value::Num(Matrix::row(x)), Value::Num(Matrix::row(y)), Value::num(h as f64), Value::num(0.0)]))
        }
        "treeplot" => {
            let p: Vec<usize> = mat(arg(a, 0, name)?)?.data.iter().map(|&x| x as usize).collect();
            tree_lines(it, &p, nargout)
        }
        "etreeplot" => {
            let p = etree(&mat(arg(a, 0, name)?)?);
            tree_lines(it, &p, nargout)
        }
        "unmesh" => {
            // unmesh(E): an edge list [x1 y1 x2 y2] to an adjacency matrix and node coordinates.
            let e = mat(arg(a, 0, name)?)?;
            if e.cols() != 4 {
                return rt("Mesh must have rows of the form [x1 y1 x2 y2].");
            }
            let r = e.rows();
            let mut nodes: Vec<(f64, f64)> = Vec::new();
            let mut idx = |x: f64, y: f64| -> usize {
                match nodes.iter().position(|&p| p == (x, y)) {
                    Some(k) => k,
                    None => {
                        nodes.push((x, y));
                        nodes.len() - 1
                    }
                }
            };
            let mut pairs = Vec::new();
            for i in 0..r {
                let p = idx(e.data[i], e.data[r + i]);
                let q = idx(e.data[2 * r + i], e.data[3 * r + i]);
                pairs.push((p, q));
            }
            // Nodes in sorted (x, y) order, as unique gives them.
            let mut order: Vec<usize> = (0..nodes.len()).collect();
            order.sort_by(|&i, &j| nodes[i].0.total_cmp(&nodes[j].0).then(nodes[i].1.total_cmp(&nodes[j].1)));
            let mut rank = vec![0; nodes.len()];
            for (r, &i) in order.iter().enumerate() {
                rank[i] = r;
            }
            let pairs: Vec<(usize, usize)> = pairs.into_iter().map(|(p, q)| (rank[p], rank[q])).collect();
            let nodes: Vec<(f64, f64)> = order.iter().map(|&i| nodes[i]).collect();
            let n = nodes.len();
            // The Laplacian of the mesh: node degrees on the diagonal, -1 for each edge.
            let mut adj = Matrix::zeros(n, n);
            for (p, q) in pairs {
                if p != q && adj.data[q * n + p] == 0.0 {
                    adj.data[q * n + p] = -1.0;
                    adj.data[p * n + q] = -1.0;
                    adj.data[p * n + p] += 1.0;
                    adj.data[q * n + q] += 1.0;
                }
            }
            let mut xy = vec![0.0; 2 * n];
            for (k, &(x, y)) in nodes.iter().enumerate() {
                xy[k] = x;
                xy[n + k] = y;
            }
            Ok(Some(vec![sp(adj), Value::Num(Matrix::new(n, 2, xy))]))
        }
        _ => Ok(None),
    }
}
