//! `crossval`: cross-validated loss of a function of the training and test rows ('mse', 'mcr' or
//! a function handle), over K-fold, hold-out, leave-one-out or given `cvpartition`s, with Monte
//! Carlo repetitions. The partitions are drawn by `cvpartition` and `repartition`, so under
//! `rng(seed)` every fold matches MATLAB's.

use super::cvpart;
use super::tests2::fmt_num;
use super::util::*;
use crate::builtins::Ret;
use crate::error::{rt, rt_id};
use crate::indexing;
use crate::interp::Interpreter;
use crate::nd::IndexArg;
use crate::value::*;

pub const NAMES: &[&str] = &["crossval"];

pub fn call(it: &mut Interpreter, _name: &str, a: &[Value], _nargout: usize) -> Ret {
    crossval(it, a)
}

fn err_id<T>(id: &str, msg: &str) -> Res<T> {
    rt_id(&format!("stats:crossval:{id}"), msg)
}

fn warn(it: &mut Interpreter, msg: &str) {
    crate::builtins_funfun::warn(it, msg);
}

fn is_empty(v: &Value) -> bool {
    v.numel() == 0
}

fn is_char_row(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.kind == Kind::Char && m.dims.len() == 2 && m.dims[0] == 1)
}

fn is_numeric(v: &Value) -> bool {
    matches!(v, Value::Num(m) if !matches!(m.kind, Kind::Char | Kind::Logical))
}

/// `isvector`: 1-by-N or N-by-1.
fn is_vector(v: &Value) -> bool {
    let d = v.dims();
    d.len() == 2 && (d[0] == 1 || d[1] == 1)
}

/// `func2str` of a function handle.
fn func_name(f: &Value) -> String {
    match f {
        Value::Func(fv) => match fv {
            FuncValue::Named(n) => n.clone(),
            _ => crate::display::func_text(fv),
        },
        _ => String::new(),
    }
}

/// `fun(args{:})` with the errors of `evalFun`.
fn eval_fun(it: &mut Interpreter, f: &Value, args: Vec<Value>) -> Res<Value> {
    let name = func_name(f);
    let Value::Func(fv) = f else { return rt("Predfun must be a function handle.") };
    let wrap = |msg: &str| -> Res<Value> {
        if msg.starts_with("Unrecognized function or variable") && msg.contains(&name) {
            err_id("FunNotFound", &format!("The function '{name}' was not found."))
        } else {
            err_id("FunError", &format!("The function '{name}' generated the following error:\n{msg}"))
        }
    };
    // `error` returns nothing, so asking an anonymous function that is just a call to it for a value fails first.
    if let FuncValue::Anon { .. } = fv {
        let text = crate::display::func_text(fv);
        if text.find(')').is_some_and(|p| text[p + 1..].starts_with("error(")) {
            return wrap("Too many output arguments.");
        }
    }
    match it.call_value(fv, args, 1) {
        Err(e) => wrap(&e.message),
        Ok(v) => match v.into_iter().next() {
            Some(x) => Ok(x),
            None => wrap("Too many output arguments."),
        },
    }
}

/// `v(mask,:)`.
fn rows_of(v: &Value, mask: &Value) -> Res<Value> {
    let arg = indexing::to_index_arg(mask)?;
    indexing::index(v, &[arg, IndexArg::All])
}

fn mask_bits(v: &Value) -> Vec<bool> {
    match v {
        Value::Num(m) => m.data.iter().map(|&x| x != 0.0).collect(),
        _ => vec![],
    }
}

/// The group label of each element (`None` when missing), for `nominal`.
fn label_strings(v: &Value) -> Res<Vec<Option<String>>> {
    Ok(match v {
        Value::Num(m) if m.kind == Kind::Char => {
            if m.dims.len() != 2 {
                return rt("Invalid group labels.");
            }
            let (r, c) = (m.dims[0], m.dims[1]);
            (0..r).map(|i| Some((0..c).map(|j| char::from_u32(m.data[j * r + i] as u32).unwrap_or(' ')).collect::<String>().trim_end().to_string())).collect()
        }
        Value::Num(m) if m.kind == Kind::Logical => m.data.iter().map(|&x| Some(if x != 0.0 { "true" } else { "false" }.to_string())).collect(),
        Value::Num(m) => m.data.iter().map(|&x| if x.is_nan() { None } else { Some(fmt_num(x)) }).collect(),
        Value::Cell(c) => {
            let mut out = Vec::with_capacity(c.data.len());
            for e in &c.data {
                match e {
                    Value::Num(m) if m.kind == Kind::Char => out.push(if m.is_empty() { None } else { Some(m.to_string_lossy()) }),
                    _ => return rt("Invalid group labels."),
                }
            }
            out
        }
        Value::Str(s) => s.data.clone(),
        v if crate::containers::is_categorical(v) => crate::containers::cat_strings(v).unwrap_or_default(),
        _ => return rt("Invalid group labels."),
    })
}

/// The class of the result array when `new` is assigned into an array of class `cur`: a numeric
/// array keeps its class, a logical or character one takes the class of what is stored in it.
fn merge_kind(cur: Kind, new: Kind) -> Kind {
    match cur {
        Kind::Logical | Kind::Char => new,
        _ => cur,
    }
}

struct Opts {
    kfold: Value,
    holdout: Value,
    leaveout: Value,
    mcreps: Value,
    stratify: Value,
    partition: Value,
    predfun: Value,
    options: Value,
}

/// `internal.stats.parseArgs` for the options of `crossval`.
fn parse_opts(rest: &[Value]) -> Res<Opts> {
    const PNAMES: [&str; 8] = ["kfold", "holdout", "leaveout", "mcreps", "stratify", "partition", "predfun", "options"];
    let mut slots: Vec<Value> = vec![Value::empty(), Value::empty(), Value::empty(), Value::num(1.0), Value::empty(), Value::empty(), Value::empty(), Value::empty()];
    if rest.len() % 2 != 0 {
        return rt_id("stats:internal:parseArgs:WrongNumberArgs", "Wrong number of arguments.");
    }
    for pair in rest.chunks(2) {
        let name_v = &pair[0];
        let is_char = matches!(name_v, Value::Num(m) if m.kind == Kind::Char);
        if !is_char {
            return rt_id("stats:internal:parseArgs:IllegalParamName", "Parameter name must be text.");
        }
        let pname = name_v.text().unwrap_or_default();
        let lower = pname.to_lowercase();
        let hits: Vec<usize> = (0..PNAMES.len()).filter(|&i| PNAMES[i].starts_with(&lower)).collect();
        let slot = match hits.len() {
            0 => return rt_id("stats:internal:parseArgs:BadParamName", format!("Invalid parameter name: {pname}.")),
            1 => hits[0],
            _ => match PNAMES.iter().position(|n| *n == lower) {
                Some(i) => i,
                None => return rt_id("stats:internal:parseArgs:AmbiguousParamName", format!("Ambiguous parameter name: {pname}.")),
            },
        };
        slots[slot] = pair[1].clone();
    }
    let mut it = slots.into_iter();
    let mut next = || it.next().unwrap_or_else(Value::empty);
    Ok(Opts { kfold: next(), holdout: next(), leaveout: next(), mcreps: next(), stratify: next(), partition: next(), predfun: next(), options: next() })
}

/// `processParallelAndStreamOptions`: the options must be a `statset` structure.
fn check_options(v: &Value) -> Res<()> {
    if !is_empty(v) && !matches!(v, Value::Struct(_)) {
        return rt("First argument must be an options structure created with STATSET.");
    }
    Ok(())
}

fn crossval(it: &mut Interpreter, a: &[Value]) -> Ret {
    if a.len() < 2 {
        return err_id("TooFewInputs", "At least two inputs are needed.");
    }
    // convertStringsToChars on every input
    let mut args: Vec<Value> = Vec::with_capacity(a.len());
    for v in a {
        args.push(if matches!(v, Value::Str(_)) { it.call_function("convertStringsToChars", vec![v.clone()], 1)?.into_iter().next().unwrap_or_else(Value::empty) } else { v.clone() });
    }
    let first = args[0].clone();
    // The first input: 'mse', 'mcr' or a function handle.
    let mut is_loss = false;
    let mut ismse = false;
    match &first {
        Value::Num(m) if m.kind == Kind::Char => {
            let t = if m.dims.len() == 2 && m.dims[0] == 1 { first.text().unwrap_or_default().to_lowercase() } else { String::new() };
            match t.as_str() {
                "mse" => {
                    is_loss = true;
                    ismse = true;
                }
                "mcr" => is_loss = true,
                _ => return err_id("BadFun", "The first input must be 'mse' or 'mcr' or a function handle."),
            }
        }
        Value::Func(_) => {}
        _ => return err_id("BadFun", "The first input must be 'mse' or 'mcr' or a function handle."),
    }
    let var = &args[1..];
    let n = var[0].dims().first().copied().unwrap_or(1);
    if n < 2 {
        return err_id("TooFewDataRows", "The data arguments X must have at least two rows");
    }
    let mut ndata = var.len();
    for (i, v) in var.iter().enumerate().skip(1) {
        if v.dims().first().copied().unwrap_or(1) != n {
            if is_char_row(v) {
                ndata = i;
                break;
            }
            return err_id("MismatchedDataRows", "Data arguments X,Y,... must have the same number of rows.");
        }
    }
    if is_loss && ndata < 2 {
        return err_id("NotEnoughVars", "At least two data variables required if the first input is 'mse' or 'mcr'.");
    }
    let data: Vec<Value> = var[..ndata].to_vec();
    let mut o = parse_opts(&var[ndata..])?;
    // The labels of the response, for 'mcr'.
    let mut ylab: Vec<Option<String>> = vec![];
    if is_loss {
        if is_empty(&o.predfun) {
            return err_id("MissingPredfun", "'Predfun' needs to be provided if the first input is 'mse' or 'mcr'.");
        }
        let y = &data[ndata - 1];
        if !ismse {
            let ok = match y {
                Value::Num(m) if m.kind == Kind::Char => m.dims.len() == 2,
                Value::Num(_) | Value::Cell(_) => is_vector(y) && (matches!(y, Value::Num(_)) || matches!(y, Value::Cell(c) if c.data.iter().all(|e| matches!(e, Value::Num(m) if m.kind == Kind::Char)))),
                v if crate::containers::is_categorical(v) => is_vector(v),
                _ => false,
            };
            if !ok {
                return err_id("MCRBadGroup", "When the first argument is 'mcr', the last data variable must be a categorical, numeric, or logical vector, a string array, a cell vector of character vectors, or a character matrix.");
            }
            ylab = label_strings(y)?;
        } else {
            let isfloat = matches!(y, Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single));
            if !isfloat || !is_vector(y) {
                return err_id("MSEBadY", "When the first input is 'mse', the last data variable has to be a column vector of floating-point numbers.");
            }
        }
    } else if !is_empty(&o.predfun) {
        warn(it, "'Predfun' will be ignored since the first input is a function handle.");
    }
    if !is_empty(&o.leaveout) {
        if o.leaveout.numel() != 1 {
            return rt("Operands to the logical AND (&&) and OR (||) operators must be convertible to logical scalar values. Use the ANY or ALL functions to reduce operands to logical scalar values.");
        }
        if !matches!(&o.leaveout, Value::Num(m) if m.data[0] == 1.0) {
            return err_id("UnsupportedLeaveout", "'Leaveout' must be 1. Only leave-one-out is supported.");
        }
    }
    let ok = is_numeric(&o.mcreps) && o.mcreps.numel() == 1 && o.mcreps.scalar().is_ok_and(|v| v == v.round() && v >= 1.0 && v.is_finite());
    if !ok {
        return err_id("BadMcreps", "'Mcreps' must be a positive integer.");
    }
    let mut mcreps = o.mcreps.scalar()? as usize;
    let cvopts = [!is_empty(&o.holdout), !is_empty(&o.kfold), !is_empty(&o.leaveout)].iter().filter(|&&b| b).count();
    if cvopts > 1 {
        return err_id("InconsistentOpts", "Specify only one of the following: 'Kfold','Holdout' and 'Leaveout'.");
    }
    let mut cvp: Value = if !is_empty(&o.partition) {
        if cvopts > 0 || !is_empty(&o.stratify) {
            return err_id("ConflictOpts", "'Partition' cannot be used with 'Kfold','Holdout', 'Leaveout' or 'Stratify'.");
        }
        let Some((pn, _, ptype, _)) = cvpart::describe(&o.partition) else {
            return err_id("Badcvp", "'Partition' must be a CVPARTITION object.");
        };
        if pn != n {
            return err_id("MissizedCVPN", "The 'N' property of 'PARTITION' must be equal the number of rows in X.");
        }
        if mcreps > 1 && (ptype == "resubstitution" || ptype == "leaveout") {
            mcreps = 1;
            warn(it, &format!("'Mcreps' is set to 1 for '{ptype}'."));
        }
        check_options(&o.options)?;
        std::mem::replace(&mut o.partition, Value::empty())
    } else {
        let mut groups: Option<Value> = None;
        if !is_empty(&o.stratify) {
            let g = it.call_function("grp2idx", vec![o.stratify.clone()], 1)?.into_iter().next().unwrap_or_else(Value::empty);
            if g.dims().first().copied().unwrap_or(0) != n {
                return err_id("MissizedStratify", "'Stratify' must have the same number of rows as X.");
            }
            groups = Some(g);
        }
        let (choice, cvarg) = if cvopts == 0 {
            ("kfold", Value::num(10.0))
        } else if !is_empty(&o.kfold) {
            ("kfold", o.kfold.clone())
        } else if !is_empty(&o.holdout) {
            ("holdout", o.holdout.clone())
        } else {
            if mcreps > 1 {
                mcreps = 1;
                warn(it, "'Mcreps' is set to 1 for 'leaveout'.");
            }
            ("leaveout", Value::num(1.0))
        };
        check_options(&o.options)?;
        let src = groups.unwrap_or_else(|| Value::num(n as f64));
        it.call_function("cvpartition", vec![src, Value::chars(choice), cvarg], 1)?.into_iter().next().unwrap_or_else(Value::empty)
    };
    let (_, numtests, _, _) = cvpart::describe(&cvp).ok_or_else(|| crate::error::LangError::new("Invalid partition.", 0))?;
    let numiter = numtests * mcreps;
    let masks = |it: &mut Interpreter, cvp: &Value, i: usize| -> Res<(Value, Value)> {
        let train = it.call_function("training", vec![cvp.clone(), Value::num(i as f64)], 1)?.into_iter().next().unwrap_or_else(Value::empty);
        let test = it.call_function("test", vec![cvp.clone(), Value::num(i as f64)], 1)?.into_iter().next().unwrap_or_else(Value::empty);
        Ok((train, test))
    };
    if !is_loss {
        // A function of the training and test rows: one row of results per fold.
        let mut size0: Option<Vec<usize>> = None;
        let mut kind = Kind::Double;
        let mut rows: Vec<Vec<f64>> = Vec::with_capacity(numiter);
        let mut cell_rows: Vec<Vec<Value>> = Vec::new();
        let mut is_cell = false;
        for mr in 0..mcreps {
            if mr > 0 {
                cvp = it.call_function("repartition", vec![cvp.clone()], 1)?.into_iter().next().unwrap_or_else(Value::empty);
            }
            for i in 1..=numtests {
                let (train, test) = masks(it, &cvp, i)?;
                let mut fargs: Vec<Value> = Vec::with_capacity(2 * ndata);
                for d in &data {
                    fargs.push(rows_of(d, &train)?);
                }
                for d in &data {
                    fargs.push(rows_of(d, &test)?);
                }
                let res = eval_fun(it, &first, fargs)?;
                if let Some(s) = &size0 {
                    if &res.dims() != s {
                        let name = func_name(&first);
                        return err_id("FunOutSizeMismatched", &format!("The size of outputs of function '{name}' should be the same."));
                    }
                }
                if let Value::Cell(c) = &res {
                    if size0.is_none() {
                        size0 = Some(res.dims());
                        is_cell = true;
                    }
                    cell_rows.push(c.data.clone());
                    continue;
                }
                let Value::Num(m) = &res else { return rt(format!("Unsupported result of class '{}' of the function.", res.class_name())) };
                if size0.is_none() {
                    size0 = Some(res.dims());
                    kind = m.kind;
                } else {
                    kind = merge_kind(kind, m.kind);
                }
                rows.push(m.data.clone());
            }
        }
        if is_cell {
            let q = cell_rows.first().map_or(0, |r| r.len());
            let mut out = vec![Value::empty(); numiter * q];
            for (r, row) in cell_rows.iter().enumerate() {
                for (j, x) in row.iter().enumerate() {
                    out[j * numiter + r] = x.clone();
                }
            }
            return Ok(Some(vec![Value::Cell(CellArray::from_dims(vec![numiter, q], out))]));
        }
        let q = rows.first().map_or(0, |r| r.len());
        let mut out = vec![0.0; numiter * q];
        for (r, row) in rows.iter().enumerate() {
            for (j, &x) in row.iter().enumerate() {
                out[j * numiter + r] = x;
            }
        }
        let mut m = Matrix::from_dims(vec![numiter, q], out);
        m.kind = kind;
        return Ok(Some(vec![Value::Num(m)]));
    }
    // A loss measure: the predictions of the function of the training rows and the test rows.
    let predfun = o.predfun.clone();
    let mut total = 0.0;
    for mr in 0..mcreps {
        if mr > 0 {
            cvp = it.call_function("repartition", vec![cvp.clone()], 1)?.into_iter().next().unwrap_or_else(Value::empty);
        }
        for i in 1..=numtests {
            let (train, test) = masks(it, &cvp, i)?;
            let mut fargs: Vec<Value> = Vec::with_capacity(2 * ndata - 1);
            for d in &data {
                fargs.push(rows_of(d, &train)?);
            }
            for d in &data[..ndata - 1] {
                fargs.push(rows_of(d, &test)?);
            }
            let pred = eval_fun(it, &predfun, fargs)?;
            let tbits = mask_bits(&test);
            let ntest = tbits.iter().filter(|&&b| b).count();
            if pred.dims() != vec![ntest, 1] {
                return err_id("badFunResultSize", "The output of 'Predfun' must be a column vector with the number of rows as the number of samples in the corresponding test set.");
            }
            if ismse {
                let yt = rows_of(&data[ndata - 1], &test)?;
                let (Value::Num(ym), Value::Num(pm)) = (&yt, &pred) else {
                    return rt(format!("Operator '-' is not supported for operands of type '{}' and '{}'.", yt.class_name(), pred.class_name()));
                };
                if matches!(ym.kind, Kind::Single) || matches!(pm.kind, Kind::Single) {
                    // single arithmetic when either operand is single
                    total += ym.data.iter().zip(&pm.data).map(|(y, p)| { let d = *y as f32 - *p as f32; d * d }).sum::<f32>() as f64;
                } else {
                    total += ym.data.iter().zip(&pm.data).map(|(y, p)| (y - p) * (y - p)).sum::<f64>();
                }
            } else {
                let plab = label_strings(&pred)?;
                let tl: Vec<&Option<String>> = ylab.iter().zip(&tbits).filter(|(_, &b)| b).map(|(l, _)| l).collect();
                total += tl.iter().zip(&plab).filter(|(y, p)| match (y, p) {
                    (Some(y), Some(p)) => y != p,
                    _ => true,
                }).count() as f64;
            }
        }
    }
    let (_, _, _, tsize) = cvpart::describe(&cvp).ok_or_else(|| crate::error::LangError::new("Invalid partition.", 0))?;
    let loss = total / (mcreps as f64 * tsize.iter().sum::<f64>());
    Ok(Some(vec![Value::num(loss)]))
}
