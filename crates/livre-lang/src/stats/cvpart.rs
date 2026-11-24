//! `cvpartition`: random partitions of data for cross validation (K-fold, hold-out, leave-one-out
//! and resubstitution, stratified when given group labels), with `training`, `test` and
//! `repartition`. The random draws are `rand` calls, so under `rng(seed)` they match MATLAB's.

use super::util::*;
use crate::builtins::Ret;
use crate::class::{ClassDef, Object};
use crate::error::{rt, rt_id};
use crate::interp::Interpreter;
use crate::value::*;
use std::cell::RefCell;
use std::rc::Rc;

pub const NAMES: &[&str] = &["cvpartition", "matlivre_cv_method"];

const METHODS: &[&str] = &["training", "test", "repartition", "gather"];
const VISIBLE: &[&str] = &["Type", "NumTestSets", "TrainSize", "TestSize", "NumObservations", "IsCustom"];
const HIDDEN: &[&str] = &["N", "Indices", "Group", "HoldoutT"];

fn class_def() -> Rc<ClassDef> {
    thread_local! {
        static DEF: RefCell<Option<Rc<ClassDef>>> = const { RefCell::new(None) };
    }
    DEF.with(|d| {
        if let Some(c) = d.borrow().as_ref() {
            return c.clone();
        }
        let mut src = String::from("classdef cvpartition\n  properties\n");
        for p in VISIBLE {
            src += &format!("    {p}\n");
        }
        src += "  end\n  properties (Hidden)\n";
        for p in HIDDEN {
            src += &format!("    {p}\n");
        }
        src += "  end\n  methods\n";
        for m in METHODS {
            src += &format!("    function varargout = {m}(obj, varargin)\n      varargout = matlivre_cv_method('{m}', nargout, obj, varargin{{:}});\n    end\n");
        }
        src += "  end\nend\n";
        let ast = crate::class::parse_classdef(&src).expect("built-in class source parses");
        let def = Rc::new(ClassDef::build(ast, &[]));
        *d.borrow_mut() = Some(def.clone());
        def
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Kind4 {
    KFold,
    HoldOut,
    LeaveOut,
    Resub,
}

impl Kind4 {
    fn name(self) -> &'static str {
        match self {
            Kind4::KFold => "kfold",
            Kind4::HoldOut => "holdout",
            Kind4::LeaveOut => "leaveout",
            Kind4::Resub => "resubstitution",
        }
    }
}

/// The state of a partition.
#[derive(Clone)]
struct Cv {
    kind: Kind4,
    /// Observations, those with a missing group included.
    n: usize,
    k: usize,
    /// Group number of each observation (NaN when missing); `None` without groups.
    group: Option<Vec<f64>>,
    /// The fold (or hold-out set, 1 train / 2 test) of each observation; NaN for a missing group.
    indices: Vec<f64>,
    holdout_t: f64,
    train: Vec<f64>,
    test: Vec<f64>,
}

fn err_id<T>(id: &str, msg: &str) -> Res<T> {
    rt_id(&format!("stats:cvpartition:{id}"), msg)
}

fn warn(it: &mut Interpreter, msg: &str) {
    crate::builtins_funfun::warn(it, msg);
}

/// 1-based positions that sort `v` ascending (stable).
fn sort_idx(v: &[f64]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|&a, &b| v[a].partial_cmp(&v[b]).unwrap_or(std::cmp::Ordering::Equal));
    idx
}

/// `sort(rand(1, n))`: the permutation MATLAB's `rand`-based partitions use.
fn rand_perm(it: &mut Interpreter, n: usize) -> Res<Vec<usize>> {
    if n == 0 {
        return Ok(vec![]);
    }
    let r = it.call_function("rand", vec![Value::num(n as f64), Value::num(1.0)], 1)?;
    let Value::Num(m) = &r[0] else { return rt("rand failed.") };
    Ok(sort_idx(&m.data))
}

fn rand_vec(it: &mut Interpreter, n: usize) -> Res<Vec<f64>> {
    let r = it.call_function("rand", vec![Value::num(n as f64), Value::num(1.0)], 1)?;
    let Value::Num(m) = &r[0] else { return rt("rand failed.") };
    Ok(m.data.clone())
}

fn accum_ones(idx: &[f64], k: usize) -> Vec<f64> {
    let mut c = vec![0.0; k.max(idx.iter().filter(|v| v.is_finite()).fold(0.0f64, |a, &b| a.max(b)) as usize)];
    for &v in idx {
        if v.is_finite() {
            c[v as usize - 1] += 1.0;
        }
    }
    c
}

/// Draws the partition (MATLAB's `rerandom`); `cv.group` holds the groups, missing ones as NaN.
fn rerandom(it: &mut Interpreter, cv: &mut Cv) -> Res<()> {
    let present: Vec<usize> = match &cv.group {
        Some(g) => (0..g.len()).filter(|&i| !g[i].is_nan()).collect(),
        None => (0..cv.n).collect(),
    };
    let m = present.len();
    let grouped = cv.group.is_some();
    let gvals: Vec<f64> = match &cv.group {
        Some(g) => present.iter().map(|&i| g[i]).collect(),
        None => vec![],
    };
    let mut idx: Vec<f64>;
    match cv.kind {
        Kind4::KFold => {
            if cv.k == m {
                let p = rand_perm(it, m)?;
                idx = p.iter().map(|&i| (i + 1) as f64).collect();
            } else if !grouped {
                let base: Vec<f64> = (1..=m).map(|i| (1 + i % cv.k) as f64).collect();
                let p = rand_perm(it, m)?;
                idx = p.iter().map(|&i| base[i]).collect();
            } else {
                let ng = gvals.iter().fold(0.0f64, |a, &b| a.max(b)) as usize;
                let mut sizes = vec![0usize; ng];
                for &g in &gvals {
                    sizes[g as usize - 1] += 1;
                }
                if sizes.iter().any(|&s| s < cv.k && s > 0) {
                    warn(it, "One or more of the unique class values in GROUP is not present in one or more folds. For classification problems, either remove this class from the data or use N instead of GROUP to obtain nonstratified partitions. For regression problems with continuous response, use N.");
                }
                let base: Vec<f64> = (1..=m).map(|i| (1 + i % cv.k) as f64).collect();
                let r = rand_vec(it, m)?;
                let key: Vec<f64> = gvals.iter().zip(&r).map(|(g, r)| g + r).collect();
                let order = sort_idx(&key);
                idx = vec![0.0; m];
                for (j, &o) in order.iter().enumerate() {
                    idx[o] = base[j];
                }
            }
            cv.test = accum_ones(&idx, 0);
            cv.train = cv.test.iter().map(|t| m as f64 - t).collect();
        }
        Kind4::HoldOut => {
            let t = cv.holdout_t;
            if !grouped {
                let ntest = if t >= 1.0 { t as usize } else { (cv.n as f64 * t).floor() as usize };
                let mut v = vec![2.0; m];
                v[..m - ntest].iter_mut().for_each(|x| *x = 1.0);
                let p = rand_perm(it, m)?;
                idx = p.iter().map(|&i| v[i]).collect();
                cv.test = vec![ntest as f64];
                cv.train = vec![(cv.n - ntest) as f64];
            } else {
                let ratio = if t >= 1.0 { t / m as f64 } else { t };
                idx = stra_holdout(it, &gvals, ratio)?;
                cv.test = vec![idx.iter().filter(|&&v| v == 2.0).count() as f64];
                cv.train = vec![idx.iter().filter(|&&v| v == 1.0).count() as f64];
            }
        }
        Kind4::LeaveOut => {
            let p = rand_perm(it, cv.k)?;
            idx = p.iter().map(|&i| (i + 1) as f64).collect();
        }
        Kind4::Resub => {
            idx = (1..=m).map(|i| i as f64).collect();
        }
    }
    // Missing groups take NaN back.
    cv.indices = vec![f64::NAN; cv.n];
    for (j, &i) in present.iter().enumerate() {
        cv.indices[i] = idx[j];
    }
    Ok(())
}

/// Hold-out sets that keep the proportion of each group.
fn stra_holdout(it: &mut Interpreter, group: &[f64], ratio: f64) -> Res<Vec<f64>> {
    let n = group.len();
    let ng = group.iter().fold(0.0f64, |a, &b| a.max(b)) as usize;
    let mut sizes = vec![0usize; ng];
    for &g in group {
        sizes[g as usize - 1] += 1;
    }
    let mut num_test: Vec<usize> = sizes.iter().map(|&s| (s as f64 * ratio).floor() as usize).collect();
    let mut test_diff = (n as f64 * ratio).floor() as i64 - num_test.iter().sum::<usize>() as i64;
    if num_test.iter().any(|&v| v == 0) {
        let mut seen = 0i64;
        let mut added = 0i64;
        for v in num_test.iter_mut() {
            if *v == 0 {
                seen += 1;
                if seen <= test_diff {
                    *v += 1;
                    added += 1;
                }
            }
        }
        test_diff -= added;
    }
    if test_diff > 0 {
        let wasfull: Vec<bool> = num_test.iter().zip(&sizes).map(|(a, b)| a == b).collect();
        let full_len = wasfull.iter().filter(|&&f| f).count();
        let free = ng - full_len;
        let mut add: Vec<usize> = vec![1; (test_diff as usize).min(free)];
        add.resize(free, 0);
        let p = rand_perm(it, free)?;
        let add: Vec<usize> = p.iter().map(|&i| add[i]).collect();
        let mut at = 0;
        for (g, nt) in num_test.iter_mut().enumerate() {
            if !wasfull[g] {
                *nt += add[at];
                at += 1;
            }
        }
    }
    if num_test.iter().any(|&v| v == 0) {
        warn(it, "One or more of the unique class values in GROUP is not present in the test set. For classification problems, either remove this class from the data or use N instead of GROUP to obtain nonstratified partitions. For regression problems with continuous response, use N.");
    }
    if num_test.iter().zip(&sizes).any(|(a, b)| a == b) {
        warn(it, "One or more of the unique class values in GROUP is not present in the training set. For classification problems, either remove this class from the data or use N instead of GROUP to obtain nonstratified partitions. For regression problems with continuous response, use N.");
    }
    let mut idx = vec![2.0; n];
    for g in 1..=ng {
        let at: Vec<usize> = (0..n).filter(|&i| group[i] == g as f64).collect();
        let train = sizes[g - 1] - num_test[g - 1];
        let mut v = idx.clone();
        for &i in at.iter().take(train) {
            v[i] = 1.0;
        }
        let p = rand_perm(it, sizes[g - 1])?;
        for (j, &i) in at.iter().enumerate() {
            idx[i] = v[at[p[j]]];
        }
    }
    Ok(idx)
}

/// `T(1) ~= 1`: the first element of `T` is not one (a character counts by its code).
fn first_is_not_one(t: &Value) -> bool {
    match t {
        Value::Num(m) => m.data.first().is_none_or(|&x| x != 1.0),
        _ => true,
    }
}

fn is_numeric(v: &Value) -> bool {
    matches!(v, Value::Num(m) if !matches!(m.kind, Kind::Char | Kind::Logical))
}

fn matches_prefix(name: &str, full: &str) -> bool {
    name.len() <= full.len() && full.as_bytes()[..name.len()].eq_ignore_ascii_case(name.as_bytes())
}

fn char_row_text(v: &Value) -> Option<String> {
    match v {
        Value::Num(m) if m.kind == Kind::Char && m.dims.len() == 2 && m.rows() == 1 => v.text().ok(),
        Value::Str(s) if s.data.len() == 1 => v.text().ok(),
        _ => None,
    }
}

fn build(it: &mut Interpreter, a: &[Value]) -> Ret {
    // Optional trailing arguments: 'Stratify', value.
    let mut stratify: Option<bool> = None;
    if a.len() > 3 {
        // Resubstitution and leave-out take no optional arguments (checked before they are read).
        if let Some(mt) = a.get(1).and_then(char_row_text).filter(|t| !t.is_empty()) {
            if matches_prefix(&mt, "resubstitution") {
                return err_id("UnsupportedCV", "Only two inputs are accepted when the second input is 'Resubstitution'.");
            }
            if matches_prefix(&mt, "leaveout") && first_is_not_one(&a[2]) {
                return err_id("UnsupportedLeaveout", "Currently the only supported value of leave-out is 1.");
            }
        }
        let extra = &a[3..];
        match extra.len() {
            1 => return err_id("cvpartBadArg", "CVPARTITION optional argument must be a RandStream object"),
            2 => {
                let nm = extra[0].text().unwrap_or_default();
                if nm.is_empty() || !matches_prefix(&nm, "stratify") {
                    return err_id("UnknownThirdInput", "The third input must be 'Stratify' or an optional RandStream object, the value for 'Stratify' must be logical.");
                }
                match &extra[1] {
                    Value::Num(m) if m.kind == Kind::Logical && m.numel() >= 1 => stratify = Some(m.data[0] != 0.0),
                    _ => return err_id("InvalidStratifyType", "The value for 'Stratify' must be logical."),
                }
            }
            3 => return err_id("UnknownThirdInput", "The third input must be 'Stratify' or an optional RandStream object, the value for 'Stratify' must be logical."),
            _ => return err_id("cvpartTooManyArg", "CVPARTITION can have at most two optional arguments"),
        }
    }
    if a.len() < 2 {
        return err_id("TooFewInputs", "At least two arguments are needed.");
    }
    let method = &a[1];
    let names = ["kfold", "holdout", "leaveout", "resubstitution"];
    let Some(mname) = char_row_text(method) else {
        return err_id("InvalidType", "Second argument must be a character vector or string scalar.");
    };
    let hits: Vec<usize> = (0..4).filter(|&i| matches_prefix(&mname, names[i])).collect();
    if hits.len() > 1 {
        return err_id("AmbiguousMethod", &format!("The second input has an ambiguous value:  {mname}."));
    }
    let Some(&j) = hits.first() else {
        return err_id("UnknownMethod", "The second input must be either 'KFold', 'Holdout','LeaveOut' or 'Resubstitution'.");
    };
    let kind = [Kind4::KFold, Kind4::HoldOut, Kind4::LeaveOut, Kind4::Resub][j];
    let first = &a[0];
    let scalar_first = first.numel() == 1;
    if let Some(st) = stratify {
        match kind {
            Kind4::LeaveOut => return err_id("LeaveOutStratify", "STRATIFY is not supported with 'LeaveOut'."),
            Kind4::Resub => return err_id("ResubstitutionStratify", "STRATIFY is not supported with 'Resubstitution'."),
            _ => {
                if scalar_first && st {
                    return err_id("ScalarStratify", "STRATIFY can not be 'true' when first input is a scalar.");
                }
            }
        }
    }
    if kind == Kind4::LeaveOut && a.len() > 2 && first_is_not_one(&a[2]) {
        return err_id("UnsupportedLeaveout", "Currently the only supported value of leave-out is 1.");
    }
    let t_arg: Option<&Value> = if a.len() > 2 { Some(&a[2]) } else { None };
    let mut cv = Cv { kind, n: 0, k: 1, group: None, indices: vec![], holdout_t: 0.1, train: vec![], test: vec![] };
    // The observations: a count, or the group of each one.
    let non_stratified = stratify == Some(false) && !scalar_first;
    if scalar_first {
        let ok = is_numeric(first) && first.scalar().is_ok_and(|n| n > 1.0 && n == n.round() && n.is_finite());
        if !ok {
            return err_id("BadNX", "The number of observations must be a positive integer greater than one.");
        }
        cv.n = first.scalar()? as usize;
    } else {
        // The group of each observation; a categorical array gives its category codes.
        let is_cat = matches!(first, Value::Object(o) if o.class.name == "categorical");
        let g = it.call_function(if is_cat { "double" } else { "grp2idx" }, vec![first.clone()], 1)?;
        let Value::Num(gm) = &g[0] else { return rt("grp2idx failed.") };
        let g: Vec<f64> = if non_stratified {
            // No stratification: every observation in one group, those with a missing label apart.
            gm.data.iter().map(|&v| if v.is_nan() { f64::NAN } else { 1.0 }).collect()
        } else {
            gm.data.clone()
        };
        cv.n = g.len();
        let missing = g.iter().filter(|v| v.is_nan()).count();
        if missing > 0 {
            warn(it, "Ignoring rows in GROUP with missing values.");
            if g.len() - missing <= 1 {
                return err_id("BadNGrp", "The number of rows in GROUP after removing missing values must be greater than one.");
            }
        }
        cv.group = Some(g);
    }
    let m = match &cv.group {
        Some(g) => g.iter().filter(|v| !v.is_nan()).count(),
        None => cv.n,
    };
    match kind {
        Kind4::KFold => {
            let mut k = 10usize;
            if let Some(t) = t_arg.filter(|t| !matches!(t, Value::Num(x) if x.is_empty())) {
                let ok = is_numeric(t) && t.numel() == 1 && t.scalar().is_ok_and(|v| v > 1.0 && v == v.round() && v.is_finite());
                if !ok {
                    return err_id("BadK", "For K-fold, the number of folds must be an integer greater than one.");
                }
                k = t.scalar()? as usize;
            }
            if k > m {
                if cv.group.is_none() {
                    warn(it, "The number of folds K is greater than the number of observations N. K will be set to the value of N.");
                } else {
                    warn(it, "The number of folds K is greater than the number of observations with non-missing GROUP values. K will be set to the number of observations with non-missing GROUP values.");
                }
                k = m;
            }
            cv.k = k;
            rerandom(it, &mut cv)?;
        }
        Kind4::LeaveOut => {
            cv.k = m;
            rerandom(it, &mut cv)?;
            cv.train = vec![(m - 1) as f64; m];
            cv.test = vec![1.0; m];
        }
        Kind4::Resub => {
            if a.len() > 2 {
                return err_id("UnsupportedCV", "Only two inputs are accepted when the second input is 'Resubstitution'.");
            }
            cv.k = 1;
            rerandom(it, &mut cv)?;
            cv.train = vec![m as f64];
            cv.test = vec![m as f64];
        }
        Kind4::HoldOut => {
            let mut t = 0.1;
            if let Some(tv) = t_arg.filter(|t| !matches!(t, Value::Num(x) if x.is_empty())) {
                let ok = is_numeric(tv) && tv.numel() == 1 && tv.scalar().is_ok_and(|v| v > 0.0 && v.is_finite());
                if !ok {
                    return err_id("BadP", "For holdout, the third input must be either a scalar in the range of (0,1) or a positive integer.");
                }
                t = tv.scalar()?;
            }
            if t >= 1.0 {
                if t != t.round() {
                    return err_id("BadP", "For holdout, the third input must be either a scalar in the range of (0,1) or a positive integer.");
                }
                if t >= m as f64 {
                    return if cv.group.is_none() {
                        err_id("PNotLTN", "The number of observations for testing (holdout) should be smaller than the number of observations.")
                    } else {
                        err_id("PNotLTGN", "The number of observations for testing (holdout) should be smaller than the number of observations with non-missing GROUP values.")
                    };
                }
            } else if (m as f64 * t).floor() == 0.0 {
                return err_id("PTooSmall", "P is too small to have a non-empty test set.");
            }
            cv.holdout_t = t;
            rerandom(it, &mut cv)?;
            cv.k = 1;
        }
    }
    Ok(Some(vec![make_object(&cv)]))
}

fn row_of(v: &[f64]) -> Value {
    Value::Num(Matrix::row(v.to_vec()))
}

fn make_object(cv: &Cv) -> Value {
    let class = class_def();
    let mut props: Vec<Value> = Vec::new();
    for p in class.props.iter().map(|p| p.name.clone()).collect::<Vec<_>>() {
        let v = match p.as_str() {
            "Type" => Value::chars(cv.kind.name()),
            "NumTestSets" => Value::num(cv.k as f64),
            "TrainSize" => {
                if cv.train.len() == 1 {
                    Value::num(cv.train[0])
                } else {
                    row_of(&cv.train)
                }
            }
            "TestSize" => {
                if cv.test.len() == 1 {
                    Value::num(cv.test[0])
                } else {
                    row_of(&cv.test)
                }
            }
            "NumObservations" | "N" => Value::num(cv.n as f64),
            "IsCustom" => Value::bool(false),
            "Indices" => Value::Num(Matrix::col(cv.indices.clone())),
            "Group" => match &cv.group {
                Some(g) => Value::Num(Matrix::col(g.clone())),
                None => Value::empty(),
            },
            "HoldoutT" => Value::num(cv.holdout_t),
            _ => Value::empty(),
        };
        props.push(v);
    }
    Value::Object(Object { class, props: Rc::new(RefCell::new(props)) })
}

fn prop(o: &Object, name: &str) -> Value {
    let idx = o.class.props.iter().position(|p| p.name == name);
    idx.map(|i| o.props.borrow()[i].clone()).unwrap_or_else(Value::empty)
}

fn nums(v: &Value) -> Vec<f64> {
    match v {
        Value::Num(m) => m.data.clone(),
        _ => vec![],
    }
}

fn from_object(o: &Object) -> Option<Cv> {
    if o.class.name != "cvpartition" {
        return None;
    }
    let kind = match prop(o, "Type").text().ok()?.as_str() {
        "kfold" => Kind4::KFold,
        "holdout" => Kind4::HoldOut,
        "leaveout" => Kind4::LeaveOut,
        _ => Kind4::Resub,
    };
    let group = match prop(o, "Group") {
        Value::Num(m) if !m.is_empty() => Some(m.data.clone()),
        _ => None,
    };
    Some(Cv {
        kind,
        n: prop(o, "N").scalar().ok()? as usize,
        k: prop(o, "NumTestSets").scalar().ok()? as usize,
        group,
        indices: nums(&prop(o, "Indices")),
        holdout_t: prop(o, "HoldoutT").scalar().ok()?,
        train: nums(&prop(o, "TrainSize")),
        test: nums(&prop(o, "TestSize")),
    })
}

/// What `crossval` needs of a partition: the number of observations, the number of test sets, its
/// `Type` and the size of each test set; `None` when the value is not a partition.
pub fn describe(v: &Value) -> Option<(usize, usize, String, Vec<f64>)> {
    let Value::Object(o) = v else { return None };
    let cv = from_object(o)?;
    Some((cv.n, cv.k, cv.kind.name().to_string(), cv.test))
}

/// `num2str` of a vector of integers: each in a column of width digits + 2, trimmed on the left.
fn num2str_ints(v: &[f64]) -> String {
    let digits = v.iter().map(|x| format!("{}", *x as i64).len()).max().unwrap_or(1);
    v.iter().map(|x| format!("{:>w$}", *x as i64, w = digits + 2)).collect::<String>().trim_start().to_string()
}

/// The `disp` text of a partition.
pub fn object_text(o: &Object) -> Option<String> {
    let cv = from_object(o)?;
    let head = match cv.kind {
        Kind4::KFold => "K-fold cross validation partition",
        Kind4::HoldOut => "Hold-out cross validation partition",
        Kind4::LeaveOut => "Leave-one-out cross validation partition",
        Kind4::Resub => "Resubstitution (no partition of data)",
    };
    let mut s = format!("{head}\n   NumObservations: {}\n       NumTestSets: {}\n", cv.n, cv.k);
    if cv.k <= 10 {
        s += &format!("         TrainSize: {}\n          TestSize: {}\n", num2str_ints(&cv.train), num2str_ints(&cv.test));
    } else {
        s += &format!("     TrainSize: {} ...\n      TestSize: {} ...\n", num2str_ints(&cv.train[..10]), num2str_ints(&cv.test[..10]));
    }
    s += "          IsCustom: 0\n";
    Some(s)
}

/// Which sets an index argument names (`'all'` or a vector of distinct integers in 1..k).
fn set_indices(v: &Value, k: usize) -> Res<Vec<usize>> {
    let bad = || err_id("InvalidKFIndex", "Indices must be specified as \"all\" or a positive integer vector with values less than or equal to the number of test sets.");
    if v.is_text() {
        let t = v.text().unwrap_or_default();
        if !t.is_empty() && matches_prefix(&t, "all") {
            return Ok((1..=k).collect());
        }
        return bad();
    }
    let Value::Num(m) = v else { return bad() };
    let vector = m.dims.len() == 2 && (m.rows() == 1 || m.cols() == 1);
    if matches!(m.kind, Kind::Char | Kind::Logical) || !vector || m.is_empty() || m.data.iter().any(|&x| x != x.round() || x < 1.0 || x > k as f64) {
        return bad();
    }
    let ids: Vec<usize> = m.data.iter().map(|&x| x as usize).collect();
    let mut seen = std::collections::HashSet::new();
    if !ids.iter().all(|i| seen.insert(*i)) {
        return err_id("UniqueIndex", "Indices must be a vector of unique positive integers.");
    }
    Ok(ids)
}

/// `isequal(i, 1)` after `'all'` has become `1:NumTestSets` (one set here).
fn all_is_one(v: &Value) -> bool {
    if v.is_text() {
        let t = v.text().unwrap_or_default();
        return !t.is_empty() && matches_prefix(&t, "all");
    }
    matches!(v, Value::Num(m) if m.numel() == 1 && m.data[0] == 1.0 && m.kind != Kind::Char)
}

fn flags(rows: usize, cols: usize, data: Vec<bool>) -> Value {
    Value::Num(Matrix::logical(rows, cols, data.into_iter().map(|b| if b { 1.0 } else { 0.0 }).collect()))
}

fn method(it: &mut Interpreter, a: &[Value]) -> Ret {
    let name = a[0].text().unwrap_or_default();
    let Value::Object(o) = &a[2] else { return rt("Invalid cvpartition object.") };
    let Some(cv) = from_object(o) else { return rt("Invalid cvpartition object.") };
    let rest = &a[3..];
    if rest.len() > 1 && name != "repartition" {
        return rt_id("MATLAB:TooManyInputs", "Too many input arguments.");
    }
    match name.as_str() {
        "gather" => Ok(Some(vec![a[2].clone()])),
        "training" | "test" => {
            let train = name == "training";
            let given = !rest.is_empty();
            let sets: Vec<usize> = match cv.kind {
                Kind4::KFold | Kind4::LeaveOut => {
                    if !given {
                        return err_id("WrongNumInputs", "Requires two input arguments.");
                    }
                    set_indices(&rest[0], cv.k)?
                }
                Kind4::HoldOut => {
                    if given {
                        let ok = all_is_one(&rest[0]);
                        if !ok {
                            return err_id("InvalidHOIndex", "I must be 1 for Holdout validation.");
                        }
                    }
                    vec![1]
                }
                Kind4::Resub => {
                    if given {
                        let ok = all_is_one(&rest[0]);
                        if !ok {
                            return err_id("InvalidResubIndex", "I must be 1 if cv.Type is 'resubstitution'.");
                        }
                    }
                    vec![1]
                }
            };
            let n = cv.n;
            let mut out: Vec<bool> = Vec::with_capacity(n * sets.len());
            for &s in &sets {
                for i in 0..n {
                    let v = cv.indices[i];
                    out.push(match cv.kind {
                        Kind4::KFold | Kind4::LeaveOut => {
                            if train {
                                v != s as f64 && !v.is_nan()
                            } else {
                                v == s as f64
                            }
                        }
                        Kind4::HoldOut => v == if train { 1.0 } else { 2.0 },
                        Kind4::Resub => !v.is_nan(),
                    });
                }
            }
            Ok(Some(vec![flags(n, sets.len(), out)]))
        }
        "repartition" => {
            if rest.len() > 1 {
                return err_id("RepartTooManyArg", "REPARTITION can only have at most one optional argument");
            }
            if !rest.is_empty() {
                return err_id("RepartBadArg", "REPARTITION optional argument must be a RandStream object");
            }
            if cv.kind == Kind4::Resub {
                warn(it, "C keeps the same test and training sets as before if the 'Type' is 'Resubstitution'.");
                return Ok(Some(vec![a[2].clone()]));
            }
            let mut cv = cv;
            rerandom(it, &mut cv)?;
            Ok(Some(vec![make_object(&cv)]))
        }
        _ => rt(format!("Undefined function '{name}'.")),
    }
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    match name {
        "cvpartition" => build(it, a),
        _ => {
            let outs = method(it, a)?.unwrap_or_default();
            Ok(Some(vec![Value::Cell(CellArray::row(outs))]))
        }
    }
}
