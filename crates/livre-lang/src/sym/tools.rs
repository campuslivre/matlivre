//! Symbolic utilities: `quorem`, `in`, `assumptions`, `cell2sym`/`sym2cell`, the symType queries
//! (`symType`, `isSymType`, `hasSymType`, `findSymType`, `mapSymType`, `symFunType`), `fold`,
//! polynomial tools (`resultant`, `polynomialReduce`, `minpoly`), `colspace`, `bernsteinMatrix`,
//! vector potentials, `pade`, `vpasum`, `vpaintegral` and `poles`.

use std::rc::Rc;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};

use super::expr::{self, Const, Num, Rel};
use super::{algebra, calculus, mp, Expr, SymArray};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "quorem", "in", "cell2sym", "sym2cell", "assumptions", "symType", "isSymType", "hasSymType", "findSymType", "mapSymType", "symFunType", "fold",
    "resultant", "polynomialReduce", "colspace", "bernsteinMatrix", "minpoly", "potential", "vectorPotential", "pade", "vpasum", "vpaintegral", "poles", "subexpr",
];

fn not_enough() -> LangError {
    LangError::new("Not enough input arguments.", 0)
}

fn sym_arg(a: &[Value], i: usize) -> R<SymArray> {
    SymArray::from_value(a.get(i).ok_or_else(not_enough)?)
}

fn scalar_arg(a: &[Value], i: usize) -> R<Expr> {
    let s = sym_arg(a, i)?;
    if s.data.len() != 1 {
        return rt("A scalar symbolic expression is required.");
    }
    Ok(s.data.into_iter().next().unwrap())
}

fn var_of(v: &Value) -> R<String> {
    match SymArray::from_value(v)?.data.as_slice() {
        [Expr::Var(n)] => Ok(n.clone()),
        _ => rt("The variable must be a symbolic variable."),
    }
}

fn vars_of(v: &Value) -> R<Vec<String>> {
    SymArray::from_value(v)?
        .data
        .iter()
        .map(|e| match e {
            Expr::Var(n) => Ok(n.clone()),
            _ => rt("Variables must be symbolic variables."),
        })
        .collect()
}

fn sym(e: Expr) -> Value {
    Value::Sym(SymArray::scalar(e))
}

fn is_zero(e: &Expr) -> bool {
    e.is_zero() || algebra::simplify(e).is_zero()
}

fn sorted_vars(es: &[Expr]) -> Vec<String> {
    let mut vs = Vec::new();
    for e in es {
        e.free_vars(&mut vs);
    }
    vs.sort();
    vs.dedup();
    vs
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> R<Option<Vec<Value>>> {
    let one = |v: Value| Ok(Some(vec![v]));
    match name {
        "quorem" => {
            let (q, r) = quorem(a)?;
            Ok(Some(vec![sym(q), sym(r)]))
        }
        "in" => {
            let s = sym_arg(a, 0)?;
            let set = match a.get(1) {
                Some(v) => v.text()?,
                None => return Err(not_enough()),
            };
            if !matches!(set.as_str(), "real" | "integer" | "rational" | "positive") {
                return rt("Invalid set. Specify 'integer', 'rational', 'real', or 'positive'.");
            }
            one(Value::Sym(s.map(|e| Expr::Fn("in".into(), vec![e.clone(), Expr::var(&format!("'{set}'"))]))))
        }
        "assumptions" => {
            let mut vars = super::real_vars();
            if let Some(v) = a.first() {
                let wanted = sorted_vars(&SymArray::from_value(v)?.data);
                vars.retain(|x| wanted.contains(x));
            }
            vars.sort();
            let data: Vec<Expr> = vars.iter().map(|v| Expr::Fn("in".into(), vec![Expr::var(v), Expr::var("'real'")])).collect();
            let n = data.len();
            one(Value::Sym(SymArray::from_dims(vec![1, n], data)))
        }
        "cell2sym" => one(Value::Sym(cell2sym(a.first().ok_or_else(not_enough)?)?)),
        "sym2cell" => {
            let s = sym_arg(a, 0)?;
            one(Value::Cell(CellArray::from_dims(s.dims.clone(), s.data.iter().map(|e| sym(e.clone())).collect())))
        }
        "symType" => {
            let s = sym_arg(a, 0)?;
            one(Value::Str(StrArray::from_dims(s.dims.clone(), s.data.iter().map(|e| Some(classify(e).to_string())).collect())))
        }
        "isSymType" | "hasSymType" => {
            let s = sym_arg(a, 0)?;
            let ty = a.get(1).ok_or_else(not_enough)?.text()?;
            let vals: Vec<f64> = s.data.iter().map(|e| if name == "isSymType" { is_type(e, &ty) } else { has_type(e, &ty) } as i32 as f64).collect();
            let mut m = Matrix::from_dims(s.dims.clone(), vals);
            m.kind = Kind::Logical;
            one(Value::Num(m))
        }
        "findSymType" => {
            let s = sym_arg(a, 0)?;
            let ty = a.get(1).ok_or_else(not_enough)?.text()?;
            let mut found = Vec::new();
            for e in &s.data {
                find_type(e, &ty, &mut found);
            }
            found.sort_by(sort_key);
            found.dedup();
            let n = found.len();
            one(Value::Sym(SymArray::from_dims(vec![1, n], found)))
        }
        "mapSymType" => {
            let s = sym_arg(a, 0)?;
            let ty = a.get(1).ok_or_else(not_enough)?.text()?;
            let Some(Value::Func(f)) = a.get(2) else { return rt("The third argument must be a function handle.") };
            let f = f.clone();
            let mut out = Vec::with_capacity(s.data.len());
            for e in &s.data {
                out.push(map_type(it, e, &ty, &f)?);
            }
            one(Value::Sym(SymArray::from_dims(s.dims.clone(), out)))
        }
        "symFunType" => {
            let s = sym_arg(a, 0)?;
            let e = s.data.first().cloned().unwrap_or_else(Expr::zero);
            one(Value::Str(StrArray::from_dims(vec![1, 1], vec![Some(op_name(&e))])))
        }
        "fold" => {
            let Some(Value::Func(f)) = a.first() else { return rt("The first argument must be a function handle.") };
            let f = f.clone();
            let items = elements(a.get(1).ok_or_else(not_enough)?)?;
            let mut it_items = items.into_iter();
            let mut acc = match it_items.next() {
                Some(v) => v,
                None => return match a.get(2) {
                    Some(d) => one(d.clone()),
                    None => rt("Input vector must not be empty when no default value is specified."),
                },
            };
            for v in it_items {
                acc = it.call_value(&f, vec![acc, v], 1)?.into_iter().next().ok_or_else(not_enough)?;
            }
            one(acc)
        }
        "resultant" => {
            let p = scalar_arg(a, 0)?;
            let q = scalar_arg(a, 1)?;
            let v = match a.get(2) {
                Some(v) => var_of(v)?,
                None => super::default_var(&p).or_else(|| super::default_var(&q)).unwrap_or_else(|| "x".into()),
            };
            one(sym(resultant(&p, &q, &v)?))
        }
        "polynomialReduce" => {
            let p = scalar_arg(a, 0)?;
            let d = sym_arg(a, 1)?;
            let mut all = vec![p.clone()];
            all.extend(d.data.iter().cloned());
            let vars = match a.get(2) {
                Some(v) if !v.is_text() => vars_of(v)?,
                _ => sorted_vars(&all),
            };
            let (r, qs) = poly_reduce(&p, &d.data, &vars)?;
            let q = SymArray::from_dims(d.dims.clone(), qs);
            Ok(Some(if nargout > 1 { vec![sym(r), Value::Sym(q)] } else { vec![sym(r)] }))
        }
        "colspace" => {
            let s = sym_arg(a, 0)?;
            let (r, c) = (s.dims[0], s.dims[1]);
            // Rows of the reduced echelon form of A.' are a basis of the column space.
            let mut rows: Vec<Vec<Expr>> = (0..c).map(|j| (0..r).map(|i| s.data[j * r + i].clone()).collect()).collect();
            let piv = rref(&mut rows);
            let k = piv.len();
            if k == 0 {
                return one(Value::Sym(SymArray::empty()));
            }
            let mut data = Vec::with_capacity(r * k);
            for row in rows.iter().take(k) {
                data.extend(row.iter().cloned());
            }
            one(Value::Sym(SymArray::from_dims(vec![r, k], data)))
        }
        "bernsteinMatrix" => bernstein_matrix(a).map(|v| Some(vec![v])),
        "subexpr" => {
            let s = sym_arg(a, 0)?;
            let name = match a.get(1) {
                None => "sigma".to_string(),
                Some(v) => {
                    let bad = || rt::<String>("Second argument must be a symbolic variable or a char array or string naming a symbolic variable.");
                    let n = match v {
                        Value::Sym(sv) => match sv.data.as_slice() {
                            [Expr::Var(n)] => n.clone(),
                            _ => return bad().map(|_| None),
                        },
                        v if v.is_text() => v.text()?,
                        _ => return bad().map(|_| None),
                    };
                    let ok = n.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                    if !ok {
                        return bad().map(|_| None);
                    }
                    n
                }
            };
            let mut vs = Vec::new();
            for e in &s.data {
                e.free_vars(&mut vs);
            }
            if vs.contains(&name) {
                return rt(&format!("Expression already contains '{name}'."));
            }
            let (data, sub) = super::subexpr::subexpr(&s.data, &name);
            let r = Value::Sym(SymArray::from_dims(s.dims.clone(), data));
            let sv = match sub {
                Some(e) => SymArray::scalar(e),
                None => SymArray::from_dims(vec![0, 0], vec![]),
            };
            Ok(Some(vec![r, Value::Sym(sv)]))
        }
        "minpoly" => {
            let numeric = !matches!(a.first(), Some(Value::Sym(_)));
            let s = sym_arg(a, 0)?;
            let c = minpoly(&s)?;
            if let Some(v) = a.get(1) {
                let v = var_of(v)?;
                let asc: Vec<Expr> = c.iter().rev().cloned().collect();
                return one(sym(algebra::from_coeffs(&asc, &v)));
            }
            let n = c.len();
            let out = SymArray::from_dims(vec![1, n], c);
            if numeric {
                return one(Value::Num(out.to_matrix()?));
            }
            one(Value::Sym(out))
        }
        "potential" => one(sym(potential(a)?)),
        "vectorPotential" => {
            let v = vector_potential(a)?;
            one(Value::Sym(SymArray::from_dims(vec![3, 1], v)))
        }
        "pade" => one(sym(pade(a)?)),
        "vpasum" => one(sym(vpasum(a)?)),
        "vpaintegral" => one(sym(vpaintegral(a)?)),
        "poles" => {
            let (p, ord) = poles(a)?;
            let n = p.len();
            let dims = vec![n, if n == 0 { 0 } else { 1 }];
            let dims = if n == 0 { vec![0, 1] } else { dims };
            let mut out = vec![Value::Sym(SymArray::from_dims(dims.clone(), p))];
            if nargout > 1 {
                out.push(Value::Sym(SymArray::from_dims(dims, ord.into_iter().map(|k| Expr::int(k as i128)).collect())));
            }
            Ok(Some(out))
        }
        _ => Ok(None),
    }
}

// ---- quorem -----------------------------------------------------------------------------------

fn quorem(a: &[Value]) -> R<(Expr, Expr)> {
    let p = scalar_arg(a, 0)?;
    let d = scalar_arg(a, 1)?;
    // Integers: floored division, so the remainder has the sign of the divisor.
    if let (Some(x), Some(y)) = (p.as_num().and_then(|n| n.as_int()), d.as_num().and_then(|n| n.as_int())) {
        if a.len() < 3 {
            if y == 0 {
                return rt("Division by zero.");
            }
            let q = x.div_euclid(y);
            return Ok((Expr::int(q), Expr::int(x - q * y)));
        }
    }
    let v = match a.get(2) {
        Some(v) => var_of(v)?,
        None => super::default_var(&p).or_else(|| super::default_var(&d)).unwrap_or_else(|| "x".into()),
    };
    let pc = algebra::poly_coeffs(&algebra::expand(&p), &v).ok_or_else(|| LangError::new("Arguments must be polynomials.", 0))?;
    let mut dc = algebra::poly_coeffs(&algebra::expand(&d), &v).ok_or_else(|| LangError::new("Arguments must be polynomials.", 0))?;
    while dc.len() > 1 && is_zero(dc.last().unwrap()) {
        dc.pop();
    }
    let db = dc.len() - 1;
    let lc = dc[db].clone();
    if is_zero(&lc) {
        return rt("Division by zero.");
    }
    let mut r = pc;
    if r.len() <= db {
        return Ok((Expr::zero(), p));
    }
    let mut q = vec![Expr::zero(); r.len() - db];
    for i in (db..r.len()).rev() {
        let c = if lc.is_one() { r[i].clone() } else { Expr::div(r[i].clone(), lc.clone()) };
        q[i - db] = c.clone();
        r[i] = Expr::zero();
        if c.is_zero() {
            continue;
        }
        for j in 0..db {
            let k = i - db + j;
            r[k] = Expr::sub(r[k].clone(), Expr::mul(vec![c.clone(), dc[j].clone()]));
        }
    }
    r.truncate(db.max(1));
    Ok((algebra::from_coeffs(&q, &v), algebra::from_coeffs(&r, &v)))
}

// ---- cell2sym ---------------------------------------------------------------------------------

fn cell2sym(v: &Value) -> R<SymArray> {
    let Value::Cell(c) = v else { return SymArray::from_value(v) };
    let (rows, cols) = (c.dims[0], c.dims.get(1).copied().unwrap_or(1));
    let blocks: Vec<SymArray> = c.data.iter().map(SymArray::from_value).collect::<R<_>>()?;
    let mut out_rows: Vec<Vec<Expr>> = Vec::new();
    for i in 0..rows {
        let row_blocks: Vec<&SymArray> = (0..cols).map(|j| &blocks[j * rows + i]).collect();
        let h = row_blocks.first().map(|b| b.dims[0]).unwrap_or(0);
        if row_blocks.iter().any(|b| b.dims[0] != h) {
            return rt("Dimensions of arrays being concatenated are not consistent.");
        }
        for r in 0..h {
            let mut line = Vec::new();
            for b in &row_blocks {
                for cc in 0..b.dims[1] {
                    line.push(b.data[cc * h + r].clone());
                }
            }
            out_rows.push(line);
        }
    }
    let w = out_rows.first().map(|r| r.len()).unwrap_or(0);
    if out_rows.iter().any(|r| r.len() != w) {
        return rt("Dimensions of arrays being concatenated are not consistent.");
    }
    let n = out_rows.len();
    let mut data = Vec::with_capacity(n * w);
    for j in 0..w {
        for r in &out_rows {
            data.push(r[j].clone());
        }
    }
    Ok(SymArray::from_dims(vec![n, w], data))
}

// ---- symType queries --------------------------------------------------------------------------

fn is_number_only(e: &Expr) -> bool {
    match e {
        Expr::Num(_) | Expr::Const(Const::I) => true,
        Expr::Add(xs) | Expr::Mul(xs) => xs.iter().all(is_number_only),
        Expr::Pow(b, x) => is_number_only(b) && is_number_only(x) && matches!(**x, Expr::Num(_)),
        _ => false,
    }
}

fn contains_i(e: &Expr) -> bool {
    match e {
        Expr::Const(Const::I) => true,
        Expr::Add(xs) | Expr::Mul(xs) | Expr::Fn(_, xs) => xs.iter().any(contains_i),
        Expr::Pow(a, b) | Expr::Rel(_, a, b) => contains_i(a) || contains_i(b),
        _ => false,
    }
}

fn classify(e: &Expr) -> &'static str {
    match e {
        Expr::Var(_) => "variable",
        Expr::Num(Num::F(_)) | Expr::Num(Num::Vpa(_)) => "vpareal",
        Expr::Num(n) if n.is_int() => "integer",
        Expr::Num(_) => "rational",
        Expr::Const(Const::I) => "complex",
        Expr::Const(_) => "constant",
        Expr::Fn(n, a) if a.is_empty() && (n == "symtrue" || n == "symfalse") => "logicalconstant",
        Expr::Rel(_, l, r) => {
            let mut vs = Vec::new();
            l.free_vars(&mut vs);
            r.free_vars(&mut vs);
            if vs.is_empty() { "logicalconstant" } else { "equation" }
        }
        other if is_number_only(other) && contains_i(other) => "complex",
        _ => "expression",
    }
}

fn op_name(e: &Expr) -> String {
    match e {
        Expr::Add(_) => "plus".into(),
        Expr::Mul(_) => "times".into(),
        Expr::Pow(..) => "power".into(),
        Expr::Fn(n, _) => n.clone(),
        Expr::Rel(op, ..) => rel_name(*op).into(),
        other => classify(other).into(),
    }
}

fn rel_name(op: Rel) -> &'static str {
    match op {
        Rel::Eq => "eq",
        Rel::Ne => "ne",
        Rel::Lt => "lt",
        Rel::Le => "le",
        Rel::Gt => "gt",
        Rel::Ge => "ge",
    }
}

fn is_type(e: &Expr, ty: &str) -> bool {
    let c = classify(e);
    match ty {
        "variable" | "integer" | "vpareal" | "complex" | "constant" | "equation" | "logicalconstant" | "expression" => c == ty,
        "rational" => matches!(c, "integer" | "rational"),
        "real" => matches!(c, "integer" | "rational" | "vpareal"),
        "number" => matches!(c, "integer" | "rational" | "vpareal" | "complex"),
        "plus" => matches!(e, Expr::Add(_)),
        "times" => matches!(e, Expr::Mul(_)),
        "power" => matches!(e, Expr::Pow(..)),
        "eq" | "ne" | "lt" | "le" | "gt" | "ge" => matches!(e, Expr::Rel(op, ..) if rel_name(*op) == ty),
        "scalar" | "units" | "unit" | "symfun" | "symfunOf" | "logicalexpression" => false,
        name => matches!(e, Expr::Fn(n, _) if n == name),
    }
}

fn kids(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Add(xs) | Expr::Mul(xs) | Expr::Fn(_, xs) => xs.iter().collect(),
        Expr::Pow(a, b) | Expr::Rel(_, a, b) => vec![a, b],
        _ => Vec::new(),
    }
}

fn has_type(e: &Expr, ty: &str) -> bool {
    is_type(e, ty) || kids(e).into_iter().any(|k| has_type(k, ty))
}

fn find_type(e: &Expr, ty: &str, out: &mut Vec<Expr>) {
    if is_type(e, ty) {
        out.push(e.clone());
        return;
    }
    for k in kids(e) {
        find_type(k, ty, out);
    }
}

/// The order of `sort` on sym: numbers by value first, then by their printed form.
fn sort_key(a: &Expr, b: &Expr) -> std::cmp::Ordering {
    match (a.to_f64(), b.to_f64()) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.to_string().cmp(&b.to_string()),
    }
}

fn rebuild(e: &Expr, ks: Vec<Expr>) -> Expr {
    match e {
        Expr::Add(_) => Expr::add(ks),
        Expr::Mul(_) => Expr::mul(ks),
        Expr::Fn(n, _) => Expr::func(n, ks),
        Expr::Pow(..) => {
            let mut k = ks.into_iter();
            Expr::pow(k.next().unwrap(), k.next().unwrap())
        }
        Expr::Rel(op, ..) => {
            let mut k = ks.into_iter();
            Expr::rel(*op, k.next().unwrap(), k.next().unwrap())
        }
        other => other.clone(),
    }
}

fn map_type(it: &mut Interpreter, e: &Expr, ty: &str, f: &FuncValue) -> R<Expr> {
    if is_type(e, ty) {
        let r = it.call_value(f, vec![sym(e.clone())], 1)?;
        let s = SymArray::from_value(r.first().ok_or_else(not_enough)?)?;
        return s.data.into_iter().next().ok_or_else(not_enough);
    }
    let ks = kids(e);
    if ks.is_empty() {
        return Ok(e.clone());
    }
    let mut out = Vec::with_capacity(ks.len());
    for k in ks {
        out.push(map_type(it, k, ty, f)?);
    }
    Ok(rebuild(e, out))
}

fn elements(v: &Value) -> R<Vec<Value>> {
    Ok(match v {
        Value::Sym(s) => s.data.iter().map(|e| sym(e.clone())).collect(),
        Value::Cell(c) => c.data.clone(),
        Value::Num(m) => {
            let m2 = m.clone();
            (0..m.numel())
                .map(|i| {
                    let mut x = Matrix::from_dims(vec![1, 1], vec![m2.data[i]]);
                    x.kind = m2.kind;
                    Value::Num(x)
                })
                .collect()
        }
        other => vec![other.clone()],
    })
}

// ---- polynomials ------------------------------------------------------------------------------

fn resultant(p: &Expr, q: &Expr, v: &str) -> R<Expr> {
    let pc = algebra::poly_coeffs(&algebra::expand(p), v).ok_or_else(|| LangError::new("Arguments must be polynomials.", 0))?;
    let qc = algebra::poly_coeffs(&algebra::expand(q), v).ok_or_else(|| LangError::new("Arguments must be polynomials.", 0))?;
    let trim = |mut c: Vec<Expr>| {
        while c.len() > 1 && is_zero(c.last().unwrap()) {
            c.pop();
        }
        c
    };
    let (pc, qc) = (trim(pc), trim(qc));
    let (m, n) = (pc.len() - 1, qc.len() - 1);
    if m == 0 && n == 0 {
        return Ok(Expr::one());
    }
    let size = m + n;
    let mut data = vec![Expr::zero(); size * size];
    // Sylvester matrix: n shifted rows of p, then m shifted rows of q (highest degree first).
    for i in 0..n {
        for (k, c) in pc.iter().rev().enumerate() {
            data[(i + k) * size + i] = c.clone();
        }
    }
    for i in 0..m {
        for (k, c) in qc.iter().rev().enumerate() {
            data[(i + k) * size + n + i] = c.clone();
        }
    }
    let det = super::sym_det(&SymArray::from_dims(vec![size, size], data))?;
    Ok(algebra::expand(&det))
}

type Poly = Vec<(Vec<u32>, Expr)>;

pub(crate) fn to_poly(e: &Expr, vars: &[String]) -> Option<Poly> {
    let ex = algebra::expand(e);
    let terms: Vec<Expr> = match ex {
        Expr::Add(ts) => ts,
        other => vec![other],
    };
    let mut out: Poly = Vec::new();
    for t in terms {
        if t.is_zero() {
            continue;
        }
        let factors = match t {
            Expr::Mul(fs) => fs,
            other => vec![other],
        };
        let mut exps = vec![0u32; vars.len()];
        let mut coeff = Vec::new();
        for f in factors {
            let (base, k) = match &f {
                Expr::Pow(b, x) => match (&**b, x.as_num().and_then(|n| n.as_int())) {
                    (Expr::Var(_), Some(k)) if k > 0 => ((**b).clone(), k as u32),
                    _ => (f.clone(), 1),
                },
                _ => (f.clone(), 1),
            };
            match &base {
                Expr::Var(n) if vars.contains(n) => exps[vars.iter().position(|x| x == n).unwrap()] += k,
                _ => {
                    if vars.iter().any(|v| f.contains_var(v)) {
                        return None;
                    }
                    coeff.push(f.clone());
                }
            }
        }
        let c = Expr::mul(coeff);
        match out.iter_mut().find(|(m, _)| *m == exps) {
            Some((_, old)) => *old = algebra::simplify(&Expr::add(vec![old.clone(), c])),
            None => out.push((exps, c)),
        }
    }
    out.retain(|(_, c)| !is_zero(c));
    Some(out)
}

fn from_poly(p: &Poly, vars: &[String]) -> Expr {
    Expr::add(
        p.iter()
            .map(|(m, c)| {
                let mut fs = vec![c.clone()];
                for (v, &k) in vars.iter().zip(m) {
                    if k > 0 {
                        fs.push(Expr::pow(Expr::var(v), Expr::int(k as i128)));
                    }
                }
                Expr::mul(fs)
            })
            .collect(),
    )
}

/// Graded reverse lexicographic order (MATLAB's 'degreeInverseLexicographic').
fn grevlex(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    let (da, db): (u32, u32) = (a.iter().sum(), b.iter().sum());
    if da != db {
        return da.cmp(&db);
    }
    for (x, y) in a.iter().zip(b).rev() {
        if x != y {
            return y.cmp(x);
        }
    }
    std::cmp::Ordering::Equal
}

fn leading(p: &Poly) -> Option<&(Vec<u32>, Expr)> {
    p.iter().max_by(|a, b| grevlex(&a.0, &b.0))
}

fn poly_sub_scaled(p: &mut Poly, d: &Poly, shift: &[u32], c: &Expr) {
    for (m, dc) in d {
        let mm: Vec<u32> = m.iter().zip(shift).map(|(a, b)| a + b).collect();
        let term = Expr::mul(vec![c.clone(), dc.clone()]);
        match p.iter_mut().find(|(k, _)| *k == mm) {
            Some((_, old)) => *old = algebra::simplify(&Expr::sub(old.clone(), term)),
            None => p.push((mm, algebra::simplify(&Expr::neg(term)))),
        }
    }
    p.retain(|(_, c)| !is_zero(c));
}

fn poly_reduce(p: &Expr, ds: &[Expr], vars: &[String]) -> R<(Expr, Vec<Expr>)> {
    let bad = || LangError::new("Arguments must be polynomials.", 0);
    let mut rest = to_poly(p, vars).ok_or_else(bad)?;
    let divs: Vec<Poly> = ds.iter().map(|d| to_poly(d, vars).ok_or_else(bad)).collect::<R<_>>()?;
    let mut quots: Vec<Poly> = vec![Vec::new(); divs.len()];
    let mut rem: Poly = Vec::new();
    let mut guard = 0;
    while let Some((lm, lc)) = leading(&rest).cloned() {
        guard += 1;
        if guard > 10000 {
            return rt("Polynomial reduction did not terminate.");
        }
        let mut divided = false;
        for (i, d) in divs.iter().enumerate() {
            let Some((dm, dc)) = leading(d) else { continue };
            if dm.iter().zip(&lm).all(|(a, b)| a <= b) {
                let shift: Vec<u32> = lm.iter().zip(dm).map(|(a, b)| a - b).collect();
                let c = algebra::simplify(&Expr::div(lc.clone(), dc.clone()));
                quots[i].push((shift.clone(), c.clone()));
                poly_sub_scaled(&mut rest, d, &shift, &c);
                divided = true;
                break;
            }
        }
        if !divided {
            rest.retain(|(m, _)| *m != lm);
            rem.push((lm, lc));
        }
    }
    Ok((from_poly(&rem, vars), quots.iter().map(|q| from_poly(q, vars)).collect()))
}

// ---- linear algebra ---------------------------------------------------------------------------

/// Reduced row echelon form in place; returns the pivot columns.
pub(crate) fn rref(rows: &mut [Vec<Expr>]) -> Vec<usize> {
    let (m, n) = (rows.len(), rows.first().map(|r| r.len()).unwrap_or(0));
    let mut piv = Vec::new();
    let mut r = 0;
    for c in 0..n {
        if r >= m {
            break;
        }
        let Some(p) = (r..m).find(|&i| !is_zero(&rows[i][c])) else { continue };
        rows.swap(r, p);
        let pv = rows[r][c].clone();
        for j in 0..n {
            rows[r][j] = algebra::simplify(&Expr::div(rows[r][j].clone(), pv.clone()));
        }
        for i in 0..m {
            if i != r && !is_zero(&rows[i][c]) {
                let f = rows[i][c].clone();
                for j in 0..n {
                    let t = Expr::mul(vec![f.clone(), rows[r][j].clone()]);
                    rows[i][j] = algebra::simplify(&Expr::sub(rows[i][j].clone(), t));
                }
            }
        }
        piv.push(c);
        r += 1;
    }
    piv
}

fn matmul(a: &[Expr], b: &[Expr], n: usize) -> Vec<Expr> {
    let mut out = vec![Expr::zero(); n * n];
    for j in 0..n {
        for i in 0..n {
            out[j * n + i] = algebra::simplify(&Expr::add((0..n).map(|k| Expr::mul(vec![a[k * n + i].clone(), b[j * n + k].clone()])).collect()));
        }
    }
    out
}

/// Coefficients of the minimal polynomial, highest degree first.
fn minpoly(s: &SymArray) -> R<Vec<Expr>> {
    if s.dims.len() != 2 || s.dims[0] != s.dims[1] {
        return rt("Matrix must be square.");
    }
    let n = s.dims[0];
    let mut eye = vec![Expr::zero(); n * n];
    for i in 0..n {
        eye[i * n + i] = Expr::one();
    }
    let mut powers = vec![eye];
    for k in 1..=n.max(1) {
        let next = matmul(powers.last().unwrap(), &s.data, n);
        powers.push(next);
        // Solve sum_j c_j A^j = A^k for j < k: rows are the matrix entries.
        let mut rows: Vec<Vec<Expr>> = (0..n * n).map(|e| (0..=k).map(|j| powers[j][e].clone()).collect()).collect();
        let piv = rref(&mut rows);
        if piv.contains(&k) {
            continue;
        }
        let mut c = vec![Expr::zero(); k];
        for (r, &pc) in piv.iter().enumerate() {
            c[pc] = rows[r][k].clone();
        }
        let mut out = vec![Expr::one()];
        out.extend(c.iter().rev().map(|x| algebra::simplify(&Expr::neg(x.clone()))));
        return Ok(out);
    }
    rt("Unable to compute the minimal polynomial.")
}

fn binom(n: i64, k: i64) -> i128 {
    let mut r: i128 = 1;
    for i in 0..k {
        r = r * (n - i) as i128 / (i + 1) as i128;
    }
    r
}

fn bernstein_matrix(a: &[Value]) -> R<Value> {
    let n = a.first().ok_or_else(not_enough)?.scalar()?;
    if n < 0.0 || n.fract() != 0.0 {
        return rt("Nonnegative integer expected.");
    }
    let n = n as i64;
    let tv = a.get(1).ok_or_else(not_enough)?;
    let t = SymArray::from_value(tv)?;
    let m = t.data.len();
    let cols = n as usize + 1;
    let mut data = vec![Expr::zero(); m * cols];
    for (i, tt) in t.data.iter().enumerate() {
        let numeric = tt.as_num().is_some();
        for k in 0..=n {
            let p = n - k;
            let e = if numeric {
                Expr::mul(vec![Expr::int(binom(n, k)), Expr::pow(tt.clone(), Expr::int(k as i128)), Expr::pow(Expr::sub(Expr::one(), tt.clone()), Expr::int(p as i128))])
            } else {
                // (1 - t)^p written as ±(t - 1)^p, as MATLAB prints it.
                let sign = if p % 2 == 0 { 1 } else { -1 };
                Expr::mul(vec![Expr::int(sign * binom(n, k)), Expr::pow(tt.clone(), Expr::int(k as i128)), Expr::pow(Expr::sub(tt.clone(), Expr::one()), Expr::int(p as i128))])
            };
            data[k as usize * m + i] = e;
        }
    }
    let out = SymArray::from_dims(vec![m, cols], data);
    if !matches!(tv, Value::Sym(_)) {
        return Ok(Value::Num(out.to_matrix()?));
    }
    Ok(Value::Sym(out))
}

// ---- vector calculus --------------------------------------------------------------------------

fn nan() -> Expr {
    Expr::Const(Const::NaN)
}

fn potential(a: &[Value]) -> R<Expr> {
    let v = sym_arg(a, 0)?.data;
    let x = vars_of(a.get(1).ok_or_else(not_enough)?)?;
    if v.len() != x.len() {
        return rt("The vector field and the vector of variables must have the same length.");
    }
    let n = v.len();
    for i in 0..n {
        for j in i + 1..n {
            if !is_zero(&Expr::sub(calculus::diff(&v[i], &x[j]), calculus::diff(&v[j], &x[i]))) {
                return Ok(nan());
            }
        }
    }
    let mut phi = calculus::int_indef(&v[0], &x[0]);
    for i in 1..n {
        let g = algebra::simplify(&Expr::sub(v[i].clone(), calculus::diff(&phi, &x[i])));
        if !g.is_zero() {
            phi = Expr::add(vec![phi, calculus::int_indef(&g, &x[i])]);
        }
    }
    if let Some(y) = a.get(2) {
        let y = SymArray::from_value(y)?.data;
        let mut at = phi.clone();
        for (xi, yi) in x.iter().zip(&y) {
            at = at.subs(xi, yi);
        }
        phi = Expr::sub(phi, at);
    }
    Ok(algebra::expand(&phi))
}

fn vector_potential(a: &[Value]) -> R<Vec<Expr>> {
    let v = sym_arg(a, 0)?.data;
    let x = match a.get(1) {
        Some(x) => vars_of(x)?,
        None => vec!["x".into(), "y".into(), "z".into()],
    };
    if v.len() != 3 || x.len() != 3 {
        return rt("Vector potentials exist only for three-dimensional vector fields.");
    }
    let div = Expr::add((0..3).map(|i| calculus::diff(&v[i], &x[i])).collect());
    if !is_zero(&div) {
        return Ok(vec![nan(), nan(), nan()]);
    }
    // A = [∫0^z V2 dz, -∫0^z V1 dz + ∫0^x V3(x, y, 0) dx, 0].
    let from0 = |e: &Expr, var: &str| {
        let f = calculus::int_indef(e, var);
        algebra::simplify(&Expr::sub(f.clone(), f.subs(var, &Expr::zero())))
    };
    let a1 = from0(&v[1], &x[2]);
    let a2 = algebra::simplify(&Expr::add(vec![Expr::neg(from0(&v[0], &x[2])), from0(&v[2].subs(&x[2], &Expr::zero()), &x[0])]));
    Ok(vec![a1, a2, Expr::zero()])
}

// ---- pade -------------------------------------------------------------------------------------

fn pade(a: &[Value]) -> R<Expr> {
    let f = scalar_arg(a, 0)?;
    let mut v = super::default_var(&f).unwrap_or_else(|| "x".into());
    let (mut m, mut n) = (3usize, 3usize);
    let mut point = Expr::zero();
    let mut i = 1;
    while i < a.len() {
        match a[i].text().ok().as_deref() {
            Some("Order") => {
                let o = a.get(i + 1).ok_or_else(not_enough)?.to_matrix()?;
                let vals: Vec<usize> = o.data.iter().map(|&x| x as usize).collect();
                (m, n) = if vals.len() >= 2 { (vals[0], vals[1]) } else { (vals[0], vals[0]) };
                i += 2;
            }
            Some("ExpansionPoint") => {
                point = scalar_arg(a, i + 1)?;
                i += 2;
            }
            Some("OrderMode") => i += 2,
            _ => {
                v = var_of(&a[i])?;
                i += 1;
            }
        }
    }
    let shifted = if point.is_zero() { f.clone() } else { f.subs(&v, &Expr::add(vec![Expr::var(&v), point.clone()])) };
    let t = calculus::taylor(&shifted, &v, &Expr::zero(), m + n + 1);
    let mut c = algebra::poly_coeffs(&algebra::expand(&t), &v).ok_or_else(|| LangError::new("Unable to compute the Taylor expansion.", 0))?;
    c.resize(m + n + 1, Expr::zero());
    let ck = |k: i64| if k < 0 { Expr::zero() } else { c[k as usize].clone() };
    // Denominator q_1..q_n from sum_j q_j c_{k-j} = -c_k, k = m+1..m+n (free unknowns zero).
    let mut rows: Vec<Vec<Expr>> = (m + 1..=m + n)
        .map(|k| {
            let mut row: Vec<Expr> = (1..=n).map(|j| ck(k as i64 - j as i64)).collect();
            row.push(Expr::neg(ck(k as i64)));
            row
        })
        .collect();
    let piv = rref(&mut rows);
    if piv.contains(&n) {
        return rt("Unable to compute the Pade approximant.");
    }
    let mut q = vec![Expr::one()];
    q.extend(std::iter::repeat(Expr::zero()).take(n));
    for (r, &pc) in piv.iter().enumerate() {
        q[pc + 1] = rows[r][n].clone();
    }
    let p: Vec<Expr> = (0..=m).map(|k| algebra::simplify(&Expr::add((0..=n.min(k)).map(|j| Expr::mul(vec![q[j].clone(), ck((k - j) as i64)])).collect()))).collect();
    let result = rational_form(&p, &q, &v).unwrap_or_else(|| Expr::div(algebra::from_coeffs(&p, &v), algebra::from_coeffs(&q, &v)));
    Ok(if point.is_zero() { result } else { result.subs(&v, &Expr::sub(Expr::var(&v), point)) })
}

fn exact(e: &Expr) -> Option<BigRational> {
    e.as_num().and_then(|n| if matches!(n, Num::F(_)) { None } else { n.exact() })
}

fn gcd_big(a: &BigInt, b: &BigInt) -> BigInt {
    let (mut a, mut b) = (a.abs(), b.abs());
    while !b.is_zero() {
        let t = &a % &b;
        a = b;
        b = t;
    }
    a
}

fn lcm_big(a: &BigInt, b: &BigInt) -> BigInt {
    if a.is_zero() || b.is_zero() {
        return BigInt::from(1);
    }
    (a * b).abs() / gcd_big(a, b)
}

fn big_expr(r: BigRational) -> Expr {
    Expr::Num(Num::from_big(r))
}

/// p/q with rational coefficients as MATLAB shows it: integer coefficients, the content and
/// the lowest power of the variable factored out, and each polynomial with a positive leading
/// coefficient.
fn rational_form(p: &[Expr], q: &[Expr], v: &str) -> Option<Expr> {
    let pr: Vec<BigRational> = p.iter().map(exact).collect::<Option<_>>()?;
    let qr: Vec<BigRational> = q.iter().map(exact).collect::<Option<_>>()?;
    let mut l = BigInt::from(1);
    for r in pr.iter().chain(&qr) {
        l = lcm_big(&l, r.denom());
    }
    let ints = |c: &[BigRational]| -> Vec<BigInt> { c.iter().map(|r| (r * BigRational::from_integer(l.clone())).to_integer()).collect() };
    let (pi, qi) = (ints(&pr), ints(&qr));
    // (constant, lowest power, primitive polynomial)
    let split = |c: &[BigInt]| -> Option<(BigInt, usize, Expr)> {
        let low = c.iter().position(|x| !x.is_zero())?;
        let high = c.iter().rposition(|x| !x.is_zero())?;
        let mut g = BigInt::zero();
        for x in &c[low..=high] {
            g = gcd_big(&g, x);
        }
        if c[high].is_negative() {
            g = -g;
        }
        let prim: Vec<Expr> = c[low..=high].iter().map(|x| big_expr(BigRational::from_integer(x / &g))).collect();
        Some((g, low, algebra::from_coeffs(&prim, v)))
    };
    let (gp, lp, pp) = split(&pi)?;
    let (gq, lq, qp) = split(&qi)?;
    let k = BigRational::new(gp, gq);
    let power = lp as i128 - lq as i128;
    let mut fs = vec![big_expr(k), pp, Expr::pow(qp, Expr::int(-1))];
    if power != 0 {
        fs.push(Expr::pow(Expr::var(v), Expr::int(power)));
    }
    Some(Expr::mul(fs))
}

// ---- numeric sums and integrals ---------------------------------------------------------------

fn vpasum(a: &[Value]) -> R<Expr> {
    let f = scalar_arg(a, 0)?;
    let (k, lo, hi) = match a.len() {
        3 => (super::default_var(&f).unwrap_or_else(|| "k".into()), scalar_arg(a, 1)?, scalar_arg(a, 2)?),
        4 => (var_of(&a[1])?, scalar_arg(a, 2)?, scalar_arg(a, 3)?),
        _ => return rt("Invalid number of input arguments."),
    };
    let s = super::extra::symsum_pub(&f, &k, &lo, &hi);
    let s = if matches!(&s, Expr::Fn(n, _) if n == "symsum") { p_series(&f, &k, &lo, &hi).unwrap_or(s) } else { s };
    let out = mp::vpa(&s);
    if out.contains_var(&k) || matches!(&out, Expr::Fn(..)) {
        return rt("Unable to compute the sum numerically.");
    }
    Ok(out)
}

/// c/k^p summed from an integer lower bound to infinity, through zeta(p).
fn p_series(f: &Expr, k: &str, lo: &Expr, hi: &Expr) -> Option<Expr> {
    if !calculus::is_inf(hi) {
        return None;
    }
    let a = lo.as_num()?.as_int()?;
    if a < 1 {
        return None;
    }
    for p in 2..=40i128 {
        let c = algebra::simplify(&Expr::mul(vec![f.clone(), Expr::pow(Expr::var(k), Expr::int(p))]));
        if !c.contains_var(k) {
            let head = Expr::add((1..a).map(|j| Expr::pow(Expr::int(j), Expr::int(-p))).collect());
            return Some(Expr::mul(vec![c, Expr::sub(super::extra::zeta_sym(&Expr::int(p)), head)]));
        }
    }
    None
}

fn vpaintegral(a: &[Value]) -> R<Expr> {
    let f = scalar_arg(a, 0)?;
    let (v, lo, hi) = match a.len() {
        3 => (super::default_var(&f).unwrap_or_else(|| "x".into()), scalar_arg(a, 1)?, scalar_arg(a, 2)?),
        n if n >= 4 && !a[1].is_text() => (var_of(&a[1])?, scalar_arg(a, 2)?, scalar_arg(a, 3)?),
        _ => return rt("Invalid number of input arguments."),
    };
    let eval = |e: &Expr| e.eval_c(&|_| None).map(|z| z.re).ok_or_else(|| LangError::new("Integration limits must be numeric.", 0));
    let (lo, hi) = (eval(&lo)?, eval(&hi)?);
    let g = |x: f64| -> f64 {
        f.eval_c(&|n| if n == v { Some(crate::cplx::C::new(x, 0.0)) } else { None }).map(|z| z.re).unwrap_or(f64::NAN)
    };
    let value = integrate(&g, lo, hi);
    if !value.is_finite() {
        return rt("Unable to compute the integral numerically.");
    }
    let r = BigRational::from_float(value).ok_or_else(|| LangError::new("Unable to compute the integral numerically.", 0))?;
    Ok(Expr::Num(Num::Vpa(Rc::new(expr::round_sig(&r, 6)))))
}

/// Adaptive Gauss–Kronrod (7, 15) quadrature; infinite limits are mapped to a finite interval.
fn integrate(f: &dyn Fn(f64) -> f64, a: f64, b: f64) -> f64 {
    if a == b {
        return 0.0;
    }
    if a > b {
        return -integrate(f, b, a);
    }
    if a.is_infinite() || b.is_infinite() {
        // x = t/(1 - t^2) maps (-1, 1) to the real line; one-sided limits use x = a + t/(1 - t).
        return match (a.is_infinite(), b.is_infinite()) {
            (true, true) => gk_adapt(&|t: f64| { let d = 1.0 - t * t; f(t / d) * (1.0 + t * t) / (d * d) }, -1.0, 1.0, 1e-10, 0),
            (false, true) => gk_adapt(&|t: f64| { let d = 1.0 - t; f(a + t / d) / (d * d) }, 0.0, 1.0, 1e-10, 0),
            _ => gk_adapt(&|t: f64| { let d = 1.0 - t; f(b - t / d) / (d * d) }, 0.0, 1.0, 1e-10, 0),
        };
    }
    gk_adapt(f, a, b, 1e-10, 0)
}

fn gk15(f: &dyn Fn(f64) -> f64, a: f64, b: f64) -> (f64, f64) {
    const XK: [f64; 8] = [
        0.991455371120812639206854697526329, 0.949107912342758524526189684047851, 0.864864423359769072789712788640926, 0.741531185599394439863864773280788,
        0.586087235467691130294144845693013, 0.405845151377397166906606412076961, 0.207784955007898467600689403773245, 0.0,
    ];
    const WK: [f64; 8] = [
        0.022935322010529224963732008058970, 0.063092092629978553290700663189204, 0.104790010322250183839876322541518, 0.140653259715525918745189590510238,
        0.169004726639267902826583426598550, 0.190350578064785409913256402421014, 0.204432940075298892414161999234649, 0.209482141084727828012999174891714,
    ];
    const WG: [f64; 4] = [0.129484966168869693270611432679082, 0.279705391489276667901467771423780, 0.381830050505118944950369775488975, 0.417959183673469387755102040816327];
    let (c, h) = ((a + b) / 2.0, (b - a) / 2.0);
    let fc = f(c);
    let (mut k, mut g) = (fc * WK[7], fc * WG[3]);
    for j in 0..7 {
        let x = h * XK[j];
        let s = f(c - x) + f(c + x);
        k += WK[j] * s;
        if j % 2 == 1 {
            g += WG[j / 2] * s;
        }
    }
    (k * h, ((k - g) * h).abs())
}

fn gk_adapt(f: &dyn Fn(f64) -> f64, a: f64, b: f64, tol: f64, depth: u32) -> f64 {
    let (v, err) = gk15(f, a, b);
    if depth > 40 || err <= tol.max(1e-14 * v.abs()) {
        return v;
    }
    let m = (a + b) / 2.0;
    gk_adapt(f, a, m, tol / 2.0, depth + 1) + gk_adapt(f, m, b, tol / 2.0, depth + 1)
}

// ---- poles ------------------------------------------------------------------------------------

fn poles(a: &[Value]) -> R<(Vec<Expr>, Vec<usize>)> {
    let f = scalar_arg(a, 0)?;
    let mut rest = &a[1..];
    let v = match rest.first() {
        Some(x) if matches!(SymArray::from_value(x).ok().as_ref().and_then(|s| s.data.first()), Some(Expr::Var(_))) && rest.len() != 2 => {
            rest = &rest[1..];
            var_of(x)?
        }
        _ => super::default_var(&f).unwrap_or_else(|| "x".into()),
    };
    let bounds = if rest.len() >= 2 {
        let e = |x: &Value| SymArray::from_value(x).ok().and_then(|s| s.data.first().and_then(|e| e.to_f64()));
        Some((e(&rest[0]).unwrap_or(f64::NEG_INFINITY), e(&rest[1]).unwrap_or(f64::INFINITY)))
    } else {
        None
    };
    let (num, den) = algebra::numden(&algebra::together(&f));
    if !den.contains_var(&v) {
        return Ok((Vec::new(), Vec::new()));
    }
    let Some(roots) = algebra::solve_one(&den, &v) else { return Ok((Vec::new(), Vec::new())) };
    let order_at = |e: &Expr, r: &Expr| -> usize {
        let mut d = e.clone();
        for k in 0..64 {
            if !is_zero(&d.subs(&v, r)) {
                return k;
            }
            d = calculus::diff(&d, &v);
        }
        64
    };
    let mut out: Vec<(Expr, usize)> = Vec::new();
    for r in roots {
        let r = algebra::simplify(&r);
        if out.iter().any(|(x, _)| is_zero(&Expr::sub(x.clone(), r.clone()))) {
            continue;
        }
        let ord = order_at(&den, &r) as i64 - order_at(&num, &r) as i64;
        if ord <= 0 {
            continue;
        }
        if let Some((lo, hi)) = bounds {
            match r.eval_c(&|_| None) {
                Some(z) if z.im == 0.0 && z.re >= lo && z.re <= hi => {}
                _ => continue,
            }
        }
        out.push((r, ord as usize));
    }
    let key = |e: &Expr| e.eval_c(&|_| None).map(|z| (z.re, z.im)).unwrap_or((f64::INFINITY, 0.0));
    out.sort_by(|a, b| key(&a.0).partial_cmp(&key(&b.0)).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out.into_iter().unzip())
}
