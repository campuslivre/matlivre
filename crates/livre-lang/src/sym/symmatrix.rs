//! `symmatrix` and `symfunmatrix`: matrix-valued symbolic expressions. The value keeps the
//! expanded elements in `data` and the matrix expression's text in `SymArray::label`
//! (`M|A*B` for a symmatrix, `F|f(t)|t,s|rows|cols` for a symfunmatrix). Only `+`, `-`, `*`,
//! the transpose and scalar multiples keep the matrix form; they do not simplify beyond
//! `A + A` and `A*A`.

use super::{BinOp, Expr, SymArray};
use crate::ast::UnOp;
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub fn class_of(s: &SymArray) -> &'static str {
    if s.label.as_deref().map_or(false, |l| l.starts_with("F|")) { "symfunmatrix" } else { "symmatrix" }
}

/// `name = text` for a matrix-valued value (`None` for plain syms).
pub fn shown(s: &SymArray, name: &str) -> Option<String> {
    let l = s.label.as_deref()?;
    if let Some(rest) = l.strip_prefix("F|") {
        let mut p = rest.split('|');
        let (text, args) = (p.next()?, p.next()?);
        let args = args.split(',').collect::<Vec<_>>().join(", ");
        return Some(if name.is_empty() { text.to_string() } else { format!("{name}({args}) = {text}") }).map(|t| if name.is_empty() { format!(" = {t}") } else { t });
    }
    let text = l.strip_prefix("M|")?;
    Some(if name.is_empty() { format!(" = {text}") } else { format!("{name} = {text}") })
}

fn text_of(s: &SymArray) -> String {
    match s.label.as_deref().and_then(|l| l.strip_prefix("M|")) {
        Some(t) => t.to_string(),
        None => s.data.first().map(|e| e.to_string()).unwrap_or_default(),
    }
}

/// Whether `t` has a top-level `+` or `-` (outside parentheses).
fn is_sum(t: &str) -> bool {
    let mut depth = 0;
    let b: Vec<char> = t.chars().collect();
    for i in 0..b.len() {
        match b[i] {
            '(' => depth += 1,
            ')' => depth -= 1,
            ' ' if depth == 0 && i + 2 < b.len() && (b[i + 1] == '+' || b[i + 1] == '-') && b[i + 2] == ' ' => return true,
            '-' if depth == 0 && i == 0 => return true,
            _ => {}
        }
    }
    false
}

fn paren_sum(t: String) -> String {
    if is_sum(&t) { format!("({t})") } else { t }
}

pub fn relabel(op: BinOp, x: &SymArray, y: &SymArray, mut out: SymArray) -> R<SymArray> {
    if x.label.as_deref().map_or(false, |l| l.starts_with("F|")) || y.label.as_deref().map_or(false, |l| l.starts_with("F|")) {
        return rt("Operator is not supported for symfunmatrix inputs.");
    }
    let (a, b) = (text_of(x), text_of(y));
    let (xm, ym) = (x.label.is_some(), y.label.is_some());
    let label = match op {
        BinOp::Add | BinOp::Sub if xm && ym => {
            if x.dims != y.dims {
                return rt("Dimensions do not match.");
            }
            match op {
                BinOp::Add if a == b => format!("2*{}", paren_sum(a)),
                BinOp::Add => format!("{a} + {b}"),
                _ => format!("{a} - {}", paren_sum(b)),
            }
        }
        BinOp::MatMul | BinOp::Mul if xm && ym => {
            if x.dims.get(1) != y.dims.first() && x.data.len() != 1 && y.data.len() != 1 {
                return rt("Dimensions do not match.");
            }
            if a == b && !is_sum(&a) && !a.contains('*') {
                format!("{a}^2")
            } else {
                format!("{}*{}", paren_sum(a), paren_sum(b))
            }
        }
        BinOp::MatMul | BinOp::Mul if xm => format!("{}*{}", paren_sum(b), paren_sum(a)),
        BinOp::MatMul | BinOp::Mul => format!("{}*{}", paren_sum(a), paren_sum(b)),
        BinOp::MatDiv | BinOp::Div if xm && !ym && y.data.len() == 1 => format!("1/{}*{}", paren_sum(b), paren_sum(a)),
        _ => return rt(format!("Operator '{}' is not supported for symmatrix inputs.", op.symbol())),
    };
    out.args = Vec::new();
    out.label = Some(format!("M|{label}"));
    Ok(out)
}

pub fn relabel_unary(op: UnOp, s: &SymArray, mut out: SymArray) -> SymArray {
    if s.label.is_none() {
        return out;
    }
    let t = text_of(s);
    let label = match op {
        UnOp::Neg => format!("-{}", paren_sum(t)),
        UnOp::Plus => t,
        _ => {
            if t.contains('*') && !is_sum(&t) {
                t.split('*').rev().map(|f| format!("{f}.'")).collect::<Vec<_>>().join("*")
            } else {
                format!("{}.'", paren_sum(t))
            }
        }
    };
    out.label = Some(format!("M|{label}"));
    out
}

fn is_ident(n: &str) -> bool {
    n.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_') && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn shape(a: &Value) -> R<(usize, usize)> {
    let bad = |m: &str| rt::<(usize, usize)>(format!("Invalid argument at position 2. {m}"));
    let m = match a {
        Value::Num(m) => m,
        _ => return bad("Value must be numeric."),
    };
    let v: Vec<f64> = (0..m.numel()).map(|i| m.data[i]).collect();
    if v.iter().any(|x| x.fract() != 0.0 || !x.is_finite()) {
        return bad("Value must be integer-valued.");
    }
    if v.iter().any(|&x| x < 0.0) {
        return bad("Value must be nonnegative.");
    }
    match v.as_slice() {
        [n] => Ok((*n as usize, *n as usize)),
        [r, c] => Ok((*r as usize, *c as usize)),
        _ => bad("Value must be a vector with 2 elements."),
    }
}

fn elements(name: &str, r: usize, c: usize) -> Vec<Expr> {
    let mut v = Vec::with_capacity(r * c);
    for j in 1..=c {
        for i in 1..=r {
            v.push(if r == 1 && c == 1 {
                Expr::var(name)
            } else if c == 1 {
                Expr::var(&format!("{name}{i}"))
            } else {
                Expr::var(&format!("{name}{i}_{j}"))
            });
        }
    }
    v
}

fn labelled(dims: Vec<usize>, data: Vec<Expr>, label: String) -> Value {
    let mut s = SymArray::from_dims(dims, data);
    s.label = Some(label);
    Value::Sym(s)
}

fn cell_vars(v: &Value) -> Option<Vec<String>> {
    let one = |v: &Value| match v {
        Value::Sym(s) if s.data.len() == 1 => match &s.data[0] {
            Expr::Var(n) => Some(n.clone()),
            _ => None,
        },
        _ => None,
    };
    let vars: Vec<String> = match v {
        Value::Cell(c) => c.data.iter().map(one).collect::<Option<_>>()?,
        other => vec![one(other)?],
    };
    let mut d = vars.clone();
    d.sort();
    d.dedup();
    if vars.is_empty() || d.len() != vars.len() { None } else { Some(vars) }
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value]) -> R<Option<Vec<Value>>> {
    let out = |v: Value| Ok(Some(vec![v]));
    let max = if name == "symfunmatrix" { 3 } else if name == "symmatrix" { 2 } else { 1 };
    if a.len() > max {
        return rt("Too many input arguments.");
    }
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    match name {
        "symmatrix" => {
            if a[0].is_text() {
                let n = a[0].text()?;
                let (r, c) = if a.len() > 1 { shape(&a[1])? } else { (1, 1) };
                let label = if is_ident(&n) { n.clone() } else { format!("`{n}`") };
                return out(labelled(vec![r, c], elements(&n, r, c), format!("M|{label}")));
            }
            let s = SymArray::from_value(&a[0])?;
            if s.label.is_some() {
                return out(Value::Sym(s));
            }
            if s.dims.len() > 2 {
                return rt("Input must be a matrix.");
            }
            let label = if s.data.len() == 1 {
                format!("{}*symmatrix(eye(1))", s.data[0])
            } else {
                let (r, c) = (s.dims[0], s.dims[1]);
                let rows: Vec<String> = (0..r).map(|i| (0..c).map(|j| s.data[i + j * r].to_string()).collect::<Vec<_>>().join(",")).collect();
                format!("symmatrix([{}])", rows.join(";"))
            };
            out(labelled(s.dims.clone(), s.data.clone(), format!("M|{label}")))
        }
        "symmatrix2sym" => match &a[0] {
            Value::Sym(s) if class_of(s) == "symmatrix" && s.label.is_some() => out(Value::Sym(SymArray::from_dims(s.dims.clone(), s.data.clone()))),
            _ => rt("Incorrect number or types of inputs or outputs for function symmatrix2sym."),
        },
        "symfunmatrix" => {
            let spec = a[0].text()?;
            let (f, rest) = spec.split_once('(').ok_or_else(|| LangError::new("First argument must be a character vector or string scalar of the form 'f(x)'.", 0))?;
            let f = f.trim().to_string();
            let _ = rest;
            let vars = match a.get(1).and_then(cell_vars) {
                Some(v) => v,
                None => return rt("Invalid argument at position 2. Second argument must be a symbolic variable or a nonempty cell array of distinct symbolic variables."),
            };
            let (r, c) = match a.get(2) {
                Some(v) => shape(v).map_err(|e| LangError::new(e.message.replace("position 2", "position 3"), 0))?,
                None => (1, 1),
            };
            let body = Expr::Fn(f, vars.iter().map(|v| Expr::var(v)).collect());
            out(labelled(vec![1, 1], vec![body], format!("F|{spec}|{}|{r}|{c}", vars.join(","))))
        }
        _ => match &a[0] {
            Value::Sym(s) if s.label.as_deref().map_or(false, |l| l.starts_with("F|")) => {
                let l = s.label.clone().unwrap();
                let p: Vec<&str> = l.split('|').collect();
                let vars: Vec<String> = p[2].split(',').map(String::from).collect();
                let (r, c): (usize, usize) = (p[3].parse().unwrap_or(1), p[4].parse().unwrap_or(1));
                let f = match &s.data[0] {
                    Expr::Fn(n, _) => n.clone(),
                    _ => String::new(),
                };
                let mut data = Vec::new();
                for j in 1..=c {
                    for i in 1..=r {
                        let n = if r == 1 && c == 1 { f.clone() } else { format!("{f}{i}_{j}") };
                        data.push(Expr::Fn(n, vars.iter().map(|v| Expr::var(v)).collect()));
                    }
                }
                let mut o = SymArray::from_dims(vec![r, c], data);
                o.args = vars;
                out(Value::Sym(o))
            }
            other => rt(format!("Undefined function 'symfunmatrix2symfun' for input arguments of type '{}'.", other.class_name())),
        },
    }
}
