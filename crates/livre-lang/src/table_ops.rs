//! Table reshaping and joining: `addvars`, `movevars`, `mergevars`, `splitvars`,
//! `convertvars`, `rows2vars`, `varfun`, `rowfun`, `innerjoin`, `outerjoin`, `stack`,
//! `unstack`, plus small table predicates and `celldisp`.

use std::cmp::Ordering;

use crate::builtins::{one, Ret};
use crate::builtins_datafun::{row_keys, Key};
use crate::error::{rt, LangError};
use crate::indexing;
use crate::interp::{Interpreter, OutputKind};
use crate::nd::IndexArg;
use crate::table::Table;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "addvars", "movevars", "mergevars", "splitvars", "convertvars", "rows2vars", "varfun", "rowfun", "innerjoin", "outerjoin", "stack", "unstack",
    "anymissing", "istabular", "isstr", "celldisp", "issparse", "isenum", "isjava", "home", "commandwindow",
];

/// Functions whose default output names come from the names of their input variables.
pub fn wants_arg_names(name: &str) -> bool {
    matches!(name, "addvars" | "outerjoin")
}

fn table_at<'a>(a: &'a [Value], i: usize, fname: &str) -> R<&'a Table> {
    match a.get(i) {
        Some(Value::Table(t)) => Ok(t),
        _ => rt(format!("Undefined function '{fname}' for input arguments of type '{}'.", a.get(i).map(|v| v.class_name()).unwrap_or_else(|| "double".into()))),
    }
}

fn texts(v: &Value) -> R<Vec<String>> {
    match v {
        Value::Cell(c) => c.data.iter().map(|x| x.text()).collect(),
        Value::Str(s) => Ok(s.data.iter().map(|x| x.clone().unwrap_or_default()).collect()),
        other => Ok(vec![other.text()?]),
    }
}

/// Variable positions named by `v`: names, numeric indices or a logical mask.
fn var_indices(t: &Table, v: &Value) -> R<Vec<usize>> {
    match v {
        Value::Num(m) if m.kind == Kind::Logical => Ok(m.data.iter().enumerate().filter(|(_, &x)| x != 0.0).map(|(i, _)| i).collect()),
        Value::Num(m) if m.kind != Kind::Char => m
            .data
            .iter()
            .map(|&x| {
                let i = x as usize;
                if x < 1.0 || i > t.vars.len() {
                    rt("Variable index exceeds table dimensions.")
                } else {
                    Ok(i - 1)
                }
            })
            .collect(),
        _ => texts(v)?
            .iter()
            .map(|n| t.names.iter().position(|x| x == n).ok_or_else(|| LangError::new(format!("Unrecognized table variable name '{n}'."), 0)))
            .collect(),
    }
}

/// Name-value options (case-insensitive) after the positional arguments.
fn split_opts<'a>(a: &'a [Value], known: &[&str]) -> (Vec<&'a Value>, Vec<(String, &'a Value)>) {
    let mut pos = Vec::new();
    let mut opts = Vec::new();
    let mut i = 0;
    while i < a.len() {
        if let Ok(t) = a[i].text() {
            if let Some(k) = known.iter().find(|k| k.eq_ignore_ascii_case(&t)) {
                if let Some(v) = a.get(i + 1) {
                    opts.push((k.to_string(), v));
                    i += 2;
                    continue;
                }
            }
        }
        pos.push(&a[i]);
        i += 1;
    }
    (pos, opts)
}

fn opt<'a>(opts: &[(String, &'a Value)], k: &str) -> Option<&'a Value> {
    opts.iter().rev().find(|(n, _)| n == k).map(|(_, v)| *v)
}

fn rows(v: &Value, ix: &[usize]) -> R<Value> {
    indexing::index(v, &[IndexArg::List { ix: ix.to_vec(), dims: vec![ix.len(), 1], from_mask: false }, IndexArg::All])
}

/// Rows of a variable where `None` gives a missing value (NaN, '', <missing>).
fn rows_opt(v: &Value, ix: &[Option<usize>]) -> R<Value> {
    Ok(match v {
        Value::Num(m) if m.kind != Kind::Char && m.cols() == 1 => {
            let kind = if m.kind == Kind::Double || m.kind == Kind::Single { m.kind } else { Kind::Double };
            Value::Num(Matrix::col(ix.iter().map(|i| i.map(|i| m.data[i]).unwrap_or(f64::NAN)).collect()).with_kind(kind))
        }
        Value::Cell(c) => Value::Cell(CellArray::col(ix.iter().map(|i| i.map(|i| c.data[i].clone()).unwrap_or_else(|| Value::chars(""))).collect())),
        Value::Str(s) => Value::Str(StrArray::from_dims(vec![ix.len(), 1], ix.iter().map(|i| i.and_then(|i| s.data[i].clone())).collect())),
        other if ix.iter().all(|i| i.is_some()) => rows(other, &ix.iter().map(|i| i.unwrap()).collect::<Vec<_>>())?,
        other => return rt(format!("Unable to fill missing rows of a variable of class '{}'.", other.class_name())),
    })
}

/// Where new variables go: `Before`/`After` a variable (name or index), else the end.
fn location(t: &Table, opts: &[(String, &Value)]) -> R<usize> {
    for (k, after) in [("Before", false), ("After", true)] {
        if let Some(v) = opt(opts, k) {
            let i = *var_indices(t, v)?.first().ok_or_else(|| LangError::new("Invalid location.", 0))?;
            return Ok(if after { i + 1 } else { i });
        }
    }
    Ok(t.vars.len())
}

fn rebuild(t: &Table, names: Vec<String>, vars: Vec<Value>) -> R<Value> {
    let mut nt = Table::new(names, vars)?;
    nt.row_names = t.row_names.clone();
    nt.description = t.description.clone();
    if nt.vars.is_empty() {
        nt.nrows = t.nrows;
    }
    Ok(Value::Table(Box::new(nt)))
}

fn addvars(it: &mut Interpreter, a: &[Value], arg_names: &[Option<String>]) -> Ret {
    let t = table_at(a, 0, "addvars")?;
    let (pos, opts) = split_opts(&a[1..], &["Before", "After", "NewVariableNames"]);
    let at = location(t, &opts)?;
    let given = match opt(&opts, "NewVariableNames") {
        Some(v) => Some(texts(v)?),
        None => None,
    };
    if let Some(g) = &given {
        if g.len() != pos.len() {
            return rt("The number of new variable names must match the number of variables to add.");
        }
    }
    let mut names = t.names.clone();
    let mut vars = t.vars.clone();
    for (k, v) in pos.iter().enumerate() {
        let name = match &given {
            Some(g) => g[k].clone(),
            None => arg_names.get(k + 1).cloned().flatten().unwrap_or_else(|| format!("Var{}", at + k + 1)),
        };
        names.insert(at + k, name);
        vars.insert(at + k, (*v).clone());
    }
    let _ = it;
    one(rebuild(t, names, vars)?)
}

fn movevars(a: &[Value]) -> Ret {
    let t = table_at(a, 0, "movevars")?;
    let (pos, opts) = split_opts(&a[1..], &["Before", "After"]);
    let moving = var_indices(t, pos.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)?;
    let mut order: Vec<usize> = (0..t.vars.len()).filter(|i| !moving.contains(i)).collect();
    // The location is resolved among the variables that stay.
    let at = match (opt(&opts, "Before"), opt(&opts, "After")) {
        (Some(v), _) => {
            let i = var_indices(t, v)?[0];
            order.iter().position(|&x| x == i).unwrap_or(order.len())
        }
        (_, Some(v)) => {
            let i = var_indices(t, v)?[0];
            order.iter().position(|&x| x == i).map(|p| p + 1).unwrap_or(order.len())
        }
        _ => order.len(),
    };
    for (k, &m) in moving.iter().enumerate() {
        order.insert(at + k, m);
    }
    one(rebuild(t, order.iter().map(|&i| t.names[i].clone()).collect(), order.iter().map(|&i| t.vars[i].clone()).collect())?)
}

fn mergevars(a: &[Value]) -> Ret {
    let t = table_at(a, 0, "mergevars")?;
    let (pos, opts) = split_opts(&a[1..], &["NewVariableName", "MergeAsTable"]);
    let ix = var_indices(t, pos.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)?;
    let first = *ix.iter().min().ok_or_else(|| LangError::new("No variables to merge.", 0))?;
    let as_table = opt(&opts, "MergeAsTable").map(|v| v.is_true()).transpose()?.unwrap_or(false);
    let merged = if as_table {
        Value::Table(Box::new(Table::new(ix.iter().map(|&i| t.names[i].clone()).collect(), ix.iter().map(|&i| t.vars[i].clone()).collect())?))
    } else {
        indexing::hconcat(ix.iter().map(|&i| t.vars[i].clone()).collect())?
    };
    let mut names = Vec::new();
    let mut vars = Vec::new();
    for i in 0..t.vars.len() {
        if i == first {
            names.push(match opt(&opts, "NewVariableName") {
                Some(v) => v.text()?,
                None => format!("Var{}", names.len() + 1),
            });
            vars.push(merged.clone());
        } else if !ix.contains(&i) {
            names.push(t.names[i].clone());
            vars.push(t.vars[i].clone());
        }
    }
    one(rebuild(t, names, vars)?)
}

fn splitvars(a: &[Value]) -> Ret {
    let t = table_at(a, 0, "splitvars")?;
    let (pos, opts) = split_opts(&a[1..], &["NewVariableNames"]);
    let which: Vec<usize> = match pos.first() {
        Some(v) => var_indices(t, v)?,
        None => (0..t.vars.len()).collect(),
    };
    let given = match opt(&opts, "NewVariableNames") {
        Some(v) => Some(texts(v)?),
        None => None,
    };
    let mut names = Vec::new();
    let mut vars = Vec::new();
    for (i, (n, v)) in t.names.iter().zip(&t.vars).enumerate() {
        if !which.contains(&i) {
            names.push(n.clone());
            vars.push(v.clone());
            continue;
        }
        match v {
            Value::Table(inner) => {
                names.extend(inner.names.iter().cloned());
                vars.extend(inner.vars.iter().cloned());
            }
            _ if v.dims2().1 > 1 => {
                let cols = v.dims2().1;
                for c in 0..cols {
                    names.push(match &given {
                        Some(g) => g.get(c).cloned().unwrap_or_default(),
                        None => format!("{n}_{}", c + 1),
                    });
                    vars.push(indexing::index(v, &[IndexArg::All, IndexArg::scalar(c)])?);
                }
            }
            _ => {
                names.push(n.clone());
                vars.push(v.clone());
            }
        }
    }
    one(rebuild(t, names, vars)?)
}

fn convertvars(it: &mut Interpreter, a: &[Value]) -> Ret {
    let t = table_at(a, 0, "convertvars")?;
    let ix = var_indices(t, crate::builtins::arg(a, 1, "convertvars")?)?;
    let ty = crate::builtins::arg(a, 2, "convertvars")?.clone();
    let mut vars = t.vars.clone();
    for i in ix {
        let v = vars[i].clone();
        let out = match &ty {
            Value::Func(_) => it.call_func_value(&ty, vec![v], 1)?,
            other => match other.text()?.as_str() {
                "cell" => it.call_function("num2cell", vec![v], 1)?,
                name => it.call_function(name, vec![v], 1)?,
            },
        };
        vars[i] = out.into_iter().next().ok_or_else(|| LangError::new("The conversion function returned no output.", 0))?;
    }
    one(rebuild(t, t.names.clone(), vars)?)
}

fn rows2vars(a: &[Value]) -> Ret {
    let t = table_at(a, 0, "rows2vars")?;
    let mut names = vec!["OriginalVariableNames".to_string()];
    let mut vars = vec![Value::Cell(CellArray::col(t.names.iter().map(|n| Value::chars(n)).collect()))];
    for r in 0..t.nrows {
        names.push(match &t.row_names {
            Some(rn) => rn[r].clone(),
            None => format!("Var{}", r + 1),
        });
        let cells: Vec<Value> = t.vars.iter().map(|v| rows(v, &[r])).collect::<R<_>>()?;
        vars.push(indexing::vconcat(cells)?);
    }
    one(Value::Table(Box::new(Table::new(names, vars)?)))
}

/// The prefix of output variable names for a function handle (`Fun` for anonymous ones).
fn fun_label(f: &Value) -> String {
    match f {
        Value::Func(FuncValue::Named(n)) => n.clone(),
        _ => "Fun".into(),
    }
}

fn varfun(it: &mut Interpreter, a: &[Value]) -> Ret {
    let f = crate::builtins::arg(a, 0, "varfun")?.clone();
    let t = table_at(a, 1, "varfun")?;
    let (_, opts) = split_opts(&a[2..], &["InputVariables", "OutputFormat", "GroupingVariables"]);
    let ix: Vec<usize> = match opt(&opts, "InputVariables") {
        Some(v) => var_indices(t, v)?,
        None => (0..t.vars.len()).collect(),
    };
    let format = opt(&opts, "OutputFormat").map(|v| v.text()).transpose()?.unwrap_or_else(|| "table".into());
    let label = fun_label(&f);
    let mut names = Vec::new();
    let mut vars = Vec::new();
    for i in ix {
        let out = it.call_func_value(&f, vec![t.vars[i].clone()], 1)?;
        names.push(format!("{label}_{}", t.names[i]));
        vars.push(out.into_iter().next().unwrap_or_else(Value::empty));
    }
    match format.to_lowercase().as_str() {
        "uniform" => one(indexing::hconcat(vars)?),
        "cell" => one(Value::Cell(CellArray::row(vars))),
        _ => one(Value::Table(Box::new(Table::new(names, vars)?))),
    }
}

fn rowfun(it: &mut Interpreter, a: &[Value]) -> Ret {
    let f = crate::builtins::arg(a, 0, "rowfun")?.clone();
    let t = table_at(a, 1, "rowfun")?;
    let (_, opts) = split_opts(&a[2..], &["InputVariables", "OutputVariableNames", "NumOutputs", "OutputFormat", "GroupingVariables", "SeparateInputs"]);
    let ix: Vec<usize> = match opt(&opts, "InputVariables") {
        Some(v) => var_indices(t, v)?,
        None => (0..t.vars.len()).collect(),
    };
    let out_names = match opt(&opts, "OutputVariableNames") {
        Some(v) => Some(texts(v)?),
        None => None,
    };
    let nout = match (opt(&opts, "NumOutputs"), &out_names) {
        (Some(v), _) => v.scalar()? as usize,
        (None, Some(n)) => n.len(),
        _ => 1,
    };
    let format = opt(&opts, "OutputFormat").map(|v| v.text()).transpose()?.unwrap_or_else(|| "table".into());
    let mut cols: Vec<Vec<Value>> = vec![Vec::new(); nout];
    for r in 0..t.nrows {
        let args: Vec<Value> = ix.iter().map(|&i| rows(&t.vars[i], &[r])).collect::<R<_>>()?;
        let out = it.call_func_value(&f, args, nout)?;
        if out.len() < nout {
            return rt("The function returned fewer outputs than requested.");
        }
        for (k, v) in out.into_iter().take(nout).enumerate() {
            cols[k].push(v);
        }
    }
    let vars: Vec<Value> = cols.into_iter().map(indexing::vconcat).collect::<R<_>>()?;
    match format.to_lowercase().as_str() {
        "uniform" => Ok(Some(vars)),
        "cell" => one(Value::Cell(CellArray::from_dims(vec![t.nrows, nout], vars.iter().flat_map(|v| (0..t.nrows).map(move |r| rows(v, &[r]).unwrap_or_else(|_| Value::empty()))).collect()))),
        _ => {
            let names = out_names.unwrap_or_else(|| (1..=nout).map(|k| format!("Var{k}")).collect());
            one(Value::Table(Box::new(Table::new(names, vars)?)))
        }
    }
}

// ---- joins ---------------------------------------------------------------------------------

/// Per-row key tuples of `vars` in `t`.
fn key_rows(t: &Table, ix: &[usize]) -> R<Vec<Vec<Option<Key>>>> {
    let cols: Vec<Vec<Option<Key>>> = ix.iter().map(|&i| row_keys(&t.vars[i])).collect::<R<_>>()?;
    Ok((0..t.nrows).map(|r| cols.iter().map(|c| c[r].clone()).collect()).collect())
}

fn cmp_keys(a: &[Option<Key>], b: &[Option<Key>]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let o = match (x, y) {
            (Some(p), Some(q)) => p.partial_cmp(q).unwrap_or(Ordering::Equal),
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (None, None) => Ordering::Equal,
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

fn join_keys(l: &Table, r: &Table, opts: &[(String, &Value)]) -> R<(Vec<usize>, Vec<usize>)> {
    let lk = opt(opts, "LeftKeys").or_else(|| opt(opts, "Keys"));
    let rk = opt(opts, "RightKeys").or_else(|| opt(opts, "Keys"));
    match (lk, rk) {
        (Some(a), Some(b)) => Ok((var_indices(l, a)?, var_indices(r, b)?)),
        _ => {
            let common: Vec<String> = l.names.iter().filter(|n| r.names.contains(n)).cloned().collect();
            if common.is_empty() {
                return rt("Cannot find a common table variable to use as a key variable.");
            }
            let c = Value::Cell(CellArray::row(common.iter().map(|n| Value::chars(n)).collect()));
            Ok((var_indices(l, &c)?, var_indices(r, &c)?))
        }
    }
}

const JOIN_OPTS: &[&str] = &["Keys", "LeftKeys", "RightKeys", "MergeKeys", "Type", "LeftVariables", "RightVariables"];

fn innerjoin(a: &[Value]) -> Ret {
    let l = table_at(a, 0, "innerjoin")?;
    let r = table_at(a, 1, "innerjoin")?;
    let (_, opts) = split_opts(&a[2..], JOIN_OPTS);
    let (lk, rk) = join_keys(l, r, &opts)?;
    let (lrows, rrows) = (key_rows(l, &lk)?, key_rows(r, &rk)?);
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (i, x) in lrows.iter().enumerate() {
        for (j, y) in rrows.iter().enumerate() {
            if x.iter().all(|k| k.is_some()) && x == y {
                pairs.push((i, j));
            }
        }
    }
    pairs.sort_by(|p, q| cmp_keys(&lrows[p.0], &lrows[q.0]).then(p.0.cmp(&q.0)).then(p.1.cmp(&q.1)));
    let li: Vec<usize> = pairs.iter().map(|p| p.0).collect();
    let ri: Vec<usize> = pairs.iter().map(|p| p.1).collect();
    let mut names = Vec::new();
    let mut vars = Vec::new();
    for &k in &lk {
        names.push(l.names[k].clone());
        vars.push(rows(&l.vars[k], &li)?);
    }
    for (i, (n, v)) in l.names.iter().zip(&l.vars).enumerate() {
        if !lk.contains(&i) {
            names.push(n.clone());
            vars.push(rows(v, &li)?);
        }
    }
    for (j, (n, v)) in r.names.iter().zip(&r.vars).enumerate() {
        if !rk.contains(&j) {
            names.push(if names.contains(n) { format!("{n}_right") } else { n.clone() });
            vars.push(rows(v, &ri)?);
        }
    }
    one(Value::Table(Box::new(Table::new(names, vars)?)))
}

fn outerjoin(a: &[Value], arg_names: &[Option<String>]) -> Ret {
    let l = table_at(a, 0, "outerjoin")?;
    let r = table_at(a, 1, "outerjoin")?;
    let (_, opts) = split_opts(&a[2..], JOIN_OPTS);
    let (lk, rk) = join_keys(l, r, &opts)?;
    let merge = opt(&opts, "MergeKeys").map(|v| v.is_true()).transpose()?.unwrap_or(false);
    let kind = opt(&opts, "Type").map(|v| v.text()).transpose()?.unwrap_or_else(|| "full".into()).to_lowercase();
    let (lrows, rrows) = (key_rows(l, &lk)?, key_rows(r, &rk)?);
    let mut pairs: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let mut r_used = vec![false; r.nrows];
    for (i, x) in lrows.iter().enumerate() {
        let mut hit = false;
        for (j, y) in rrows.iter().enumerate() {
            if x.iter().all(|k| k.is_some()) && x == y {
                pairs.push((Some(i), Some(j)));
                r_used[j] = true;
                hit = true;
            }
        }
        if !hit && kind != "right" {
            pairs.push((Some(i), None));
        }
    }
    if kind != "left" {
        for j in (0..r.nrows).filter(|&j| !r_used[j]) {
            pairs.push((None, Some(j)));
        }
    }
    if kind == "left" {
        pairs.retain(|p| p.0.is_some());
    }
    let key_of = |p: &(Option<usize>, Option<usize>)| match p.0 {
        Some(i) => lrows[i].clone(),
        None => rrows[p.1.unwrap()].clone(),
    };
    pairs.sort_by(|p, q| cmp_keys(&key_of(p), &key_of(q)));
    let li: Vec<Option<usize>> = pairs.iter().map(|p| p.0).collect();
    let ri: Vec<Option<usize>> = pairs.iter().map(|p| p.1).collect();
    let lsuffix = arg_names.first().cloned().flatten().unwrap_or_else(|| "left".into());
    let rsuffix = arg_names.get(1).cloned().flatten().unwrap_or_else(|| "right".into());
    let mut names = Vec::new();
    let mut vars = Vec::new();
    for (i, (n, v)) in l.names.iter().zip(&l.vars).enumerate() {
        if merge && lk.contains(&i) {
            // The key from whichever side has the row.
            let j = rk[lk.iter().position(|&k| k == i).unwrap()];
            let lv = rows_opt(v, &li)?;
            let rv = rows_opt(&r.vars[j], &ri)?;
            let picked: Vec<Value> = pairs.iter().enumerate().map(|(k, p)| rows(if p.0.is_some() { &lv } else { &rv }, &[k])).collect::<R<_>>()?;
            names.push(n.clone());
            vars.push(indexing::vconcat(picked)?);
            continue;
        }
        let clash = r.names.contains(n) && !(merge && lk.contains(&i));
        names.push(if clash { format!("{n}_{lsuffix}") } else { n.clone() });
        vars.push(rows_opt(v, &li)?);
    }
    for (j, (n, v)) in r.names.iter().zip(&r.vars).enumerate() {
        if merge && rk.contains(&j) {
            continue;
        }
        let clash = l.names.contains(n);
        names.push(if clash { format!("{n}_{rsuffix}") } else { n.clone() });
        vars.push(rows_opt(v, &ri)?);
    }
    one(Value::Table(Box::new(Table::new(names, vars)?)))
}

// ---- stack / unstack -----------------------------------------------------------------------

fn stack(it: &mut Interpreter, a: &[Value]) -> Ret {
    let t = table_at(a, 0, "stack")?;
    let (pos, opts) = split_opts(&a[1..], &["NewDataVariableName", "IndexVariableName", "ConstantVariables"]);
    let ix = var_indices(t, pos.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)?;
    let joined = ix.iter().map(|&i| t.names[i].clone()).collect::<Vec<_>>().join("_");
    let data_name = opt(&opts, "NewDataVariableName").map(|v| v.text()).transpose()?.unwrap_or_else(|| joined.clone());
    let index_name = opt(&opts, "IndexVariableName").map(|v| v.text()).transpose()?.unwrap_or_else(|| format!("{joined}_Indicator"));
    let keep: Vec<usize> = (0..t.vars.len()).filter(|i| !ix.contains(i)).collect();
    let m = ix.len();
    let src_rows: Vec<usize> = (0..t.nrows).flat_map(|r| std::iter::repeat(r).take(m)).collect();
    let mut names = Vec::new();
    let mut vars = Vec::new();
    for &k in &keep {
        names.push(t.names[k].clone());
        vars.push(rows(&t.vars[k], &src_rows)?);
    }
    let labels: Vec<Value> = (0..t.nrows).flat_map(|_| ix.iter().map(|&i| Value::chars(&t.names[i]))).collect();
    let cats: Vec<Value> = ix.iter().map(|&i| Value::chars(&t.names[i])).collect();
    let indicator = it.call_function("categorical", vec![Value::Cell(CellArray::col(labels)), Value::Cell(CellArray::row(cats))], 1)?.remove(0);
    names.push(index_name);
    vars.push(indicator);
    let mut data = Vec::new();
    for r in 0..t.nrows {
        for &i in &ix {
            data.push(rows(&t.vars[i], &[r])?);
        }
    }
    names.push(data_name);
    vars.push(indexing::vconcat(data)?);
    one(Value::Table(Box::new(Table::new(names, vars)?)))
}

/// Category names (or text) of an indicator variable, per row.
fn label_texts(v: &Value) -> R<Vec<String>> {
    if crate::containers::is_categorical(v) {
        let s = match crate::containers::cat_strings(v) {
            Some(s) => s,
            None => return rt("Invalid indicator variable."),
        };
        return Ok(s.into_iter().map(|x| x.unwrap_or_default()).collect());
    }
    texts(v)
}

fn unstack(a: &[Value]) -> Ret {
    let t = table_at(a, 0, "unstack")?;
    let data = var_indices(t, crate::builtins::arg(a, 1, "unstack")?)?[0];
    let ind = var_indices(t, crate::builtins::arg(a, 2, "unstack")?)?[0];
    let labels = label_texts(&t.vars[ind])?;
    let mut new_names: Vec<String> = Vec::new();
    if crate::containers::is_categorical(&t.vars[ind]) {
        new_names = crate::containers::cat_categories(&t.vars[ind]).unwrap_or_default();
        new_names.retain(|n| labels.contains(n));
    } else {
        for l in &labels {
            if !new_names.contains(l) {
                new_names.push(l.clone());
            }
        }
        new_names.sort();
    }
    let group: Vec<usize> = (0..t.vars.len()).filter(|&i| i != data && i != ind).collect();
    let (g, first) = if group.is_empty() {
        (vec![Some(0); t.nrows], vec![0])
    } else {
        crate::builtins_datafun::groups_of(&group.iter().map(|&i| t.vars[i].clone()).collect::<Vec<_>>())?
    };
    let ng = first.len();
    let mut names = Vec::new();
    let mut vars = Vec::new();
    for &i in &group {
        names.push(t.names[i].clone());
        vars.push(rows(&t.vars[i], &first)?);
    }
    let d = t.vars[data].as_matrix()?;
    for nm in &new_names {
        let mut col = vec![f64::NAN; ng];
        for r in 0..t.nrows {
            if let Some(k) = g[r] {
                if &labels[r] == nm {
                    col[k] = if col[k].is_nan() { d.data[r] } else { col[k] + d.data[r] };
                }
            }
        }
        names.push(nm.clone());
        vars.push(Value::Num(Matrix::col(col)));
    }
    one(Value::Table(Box::new(Table::new(names, vars)?)))
}

// ---- small helpers -------------------------------------------------------------------------

fn celldisp(it: &mut Interpreter, a: &[Value]) -> Ret {
    let Some(Value::Cell(c)) = a.first() else { return rt("Input must be a cell array.") };
    let name = match a.get(1) {
        Some(v) => v.text()?,
        None => "ans".into(),
    };
    let vector = c.dims.len() == 2 && (c.dims[0] == 1 || c.dims[1] == 1);
    let mut out = String::new();
    for (k, v) in c.data.iter().enumerate() {
        let sub = if vector {
            format!("{}", k + 1)
        } else {
            let d = &c.dims;
            let mut rem = k;
            let mut parts = Vec::new();
            for &n in d {
                parts.push((rem % n + 1).to_string());
                rem /= n;
            }
            parts.join(",")
        };
        let body = match v {
            Value::Num(m) if m.kind != Kind::Char && m.numel() == 1 => format!("     {}\n", crate::display::fmt_elem(m, 0, it.format)),
            Value::Num(m) if m.kind != Kind::Char => format!("{}\n", crate::display::disp_value(v, it.format)),
            _ => crate::display::disp_value(v, it.format),
        };
        out.push_str(&format!(" \n{name}{{{sub}}} =\n \n{body}\n \n"));
    }
    it.emit(OutputKind::Text(out));
    Ok(Some(Vec::new()))
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    let arg_names = if wants_arg_names(name) { std::mem::take(&mut it.arg_names) } else { Vec::new() };
    match name {
        "addvars" => addvars(it, a, &arg_names),
        "movevars" => movevars(a),
        "mergevars" => mergevars(a),
        "splitvars" => splitvars(a),
        "convertvars" => convertvars(it, a),
        "rows2vars" => rows2vars(a),
        "varfun" => varfun(it, a),
        "rowfun" => rowfun(it, a),
        "innerjoin" => innerjoin(a),
        "outerjoin" => outerjoin(a, &arg_names),
        "stack" => stack(it, a),
        "unstack" => unstack(a),
        "anymissing" => {
            let v = crate::builtins::arg(a, 0, "anymissing")?.clone();
            let m = match &v {
                Value::Table(t) => t.vars.iter().map(|x| it.call_function("ismissing", vec![x.clone()], 1)).collect::<R<Vec<_>>>()?.into_iter().flatten().collect::<Vec<_>>(),
                _ => it.call_function("ismissing", vec![v], 1)?,
            };
            let any = m.iter().any(|x| x.as_matrix().map(|m| m.data.iter().any(|&d| d != 0.0)).unwrap_or(false));
            one(Value::bool(any))
        }
        "istabular" => one(Value::bool(matches!(a.first(), Some(Value::Table(_))))),
        "isstr" => one(Value::bool(matches!(a.first(), Some(Value::Num(m)) if m.kind == Kind::Char))),
        "issparse" | "isenum" | "isjava" => {
            crate::builtins::arg(a, 0, name)?;
            one(Value::bool(false))
        }
        "home" | "commandwindow" => Ok(Some(Vec::new())),
        "celldisp" => celldisp(it, a),
        _ => Ok(None),
    }
}
