//! Table custom properties (`addprop`, `rmprop`) and `decomposition` objects with
//! `isIllConditioned`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::class::{ClassDef, Object};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["addprop", "rmprop", "matlivre_decomposition_type", "dmperm", "details", "nested", "rowfilter", "matlivre_rowfilter_method"];
pub const CLASS: &str = "decomposition";
/// Methods of the class (for `methods` and the coverage checklist).
pub const METHODS: &[&str] = &["isIllConditioned", "rcond", "mldivide", "mrdivide"];

const TYPES: &[&str] = &["auto", "lu", "qr", "chol", "ldl", "triangular", "permutedTriangular", "banded", "hessenberg", "diagonal", "cod"];

/// The `decomposition` class: solves with the stored matrix through `\` and `/`.
pub fn class_def() -> Rc<ClassDef> {
    thread_local! {
        static DEF: RefCell<Option<Rc<ClassDef>>> = const { RefCell::new(None) };
    }
    DEF.with(|d| {
        d.borrow_mut()
            .get_or_insert_with(|| {
                let src = "classdef decomposition\n  properties\n    MatrixSize\n    Type\n  end\n  properties (Hidden)\n    Matrix\n  end\n  methods\n    \
                    function d = decomposition(A, varargin)\n      d.Type = matlivre_decomposition_type(A, varargin{:});\n      d.Matrix = A;\n      d.MatrixSize = size(A);\n    end\n    \
                    function x = mldivide(d, b)\n      x = d.Matrix \\ b;\n    end\n    \
                    function x = mrdivide(b, d)\n      x = b / d.Matrix;\n    end\n    \
                    function r = rcond(d)\n      r = rcond(d.Matrix);\n    end\n    \
                    function tf = isIllConditioned(d)\n      tf = rcond(d.Matrix) < eps(class(d.Matrix));\n    end\n  end\nend\n";
                debug_assert!(METHODS.iter().all(|m| src.contains(&format!("= {m}("))));
                let ast = crate::class::parse_classdef(src).expect("built-in class source parses");
                let mut def = ClassDef::build(ast, &[]);
                def.name = CLASS.into();
                Rc::new(def)
            })
            .clone()
    })
}

/// The display of a decomposition object.
pub fn object_text(o: &Object) -> Option<String> {
    if let Some(t) = crate::xml_dom::object_text(o) {
        return Some(t);
    }
    if o.class.name == RF_CLASS {
        return Some(rowfilter_text(o));
    }
    if o.class.name != CLASS {
        return None;
    }
    let get = |n: &str| o.class.prop_index(n).map(|i| o.props.borrow()[i].clone());
    let size = get("MatrixSize")?.to_matrix().ok()?;
    let ty = get("Type")?.text().ok()?;
    let dims: Vec<String> = size.data.iter().map(|x| format!("{x}")).collect();
    Some(format!("  decomposition with properties:\n\n    MatrixSize: [{}]\n          Type: '{ty}'\n\n  Show all properties\n", dims.join(" ")))
}

fn is_triangular(m: &Matrix) -> bool {
    let (r, c) = (m.rows(), m.cols());
    if r != c {
        return false;
    }
    let at = |i: usize, j: usize| m.data[j * r + i];
    let upper = (0..r).all(|i| (0..i.min(c)).all(|j| at(i, j) == 0.0));
    let lower = (0..r).all(|i| (i + 1..c).all(|j| at(i, j) == 0.0));
    upper || lower
}

fn decomposition_type(a: &[Value]) -> R<Value> {
    let m = a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?.to_matrix()?;
    let mut ty = "auto".to_string();
    let mut i = 1;
    while i < a.len() {
        let t = a[i].text()?;
        if TYPES.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
            ty = TYPES.iter().find(|x| x.eq_ignore_ascii_case(&t)).unwrap().to_string();
            i += 1;
        } else if matches!(t.to_lowercase().as_str(), "checkcondition" | "rankcheck" | "upper" | "lower" | "ranktolerance") {
            i += 2;
        } else {
            return rt(format!("Invalid decomposition type '{t}'."));
        }
    }
    let square = m.rows() == m.cols();
    let ty = match ty.as_str() {
        "auto" if square && is_triangular(&m) => "triangular".to_string(),
        "auto" if square => "lu".to_string(),
        "auto" => "qr".to_string(),
        "diagonal" => "triangular".to_string(),
        "lu" | "chol" | "ldl" | "triangular" | "hessenberg" | "permutedTriangular" | "banded" if !square => {
            return rt(format!("Decomposition type '{ty}' requires a square matrix."));
        }
        "chol" => {
            // A Cholesky factor exists only for a Hermitian positive definite matrix.
            let n = m.rows();
            let at = |i: usize, j: usize| m.data[j * n + i];
            let symmetric = (0..n).all(|i| (0..n).all(|j| at(i, j) == at(j, i)));
            let mut l = vec![0.0; n * n];
            let mut ok = symmetric;
            for j in 0..n {
                if !ok {
                    break;
                }
                let d = at(j, j) - (0..j).map(|k| l[k * n + j] * l[k * n + j]).sum::<f64>();
                if d <= 0.0 {
                    ok = false;
                    break;
                }
                l[j * n + j] = d.sqrt();
                for i in j + 1..n {
                    l[j * n + i] = (at(i, j) - (0..j).map(|k| l[k * n + i] * l[k * n + j]).sum::<f64>()) / l[j * n + j];
                }
            }
            if !ok {
                return rt("Matrix must be symmetric positive definite for a Cholesky decomposition.");
            }
            ty
        }
        _ => ty,
    };
    Ok(Value::chars(&ty))
}

fn names_of(v: &Value) -> R<Vec<String>> {
    Ok(match v {
        Value::Cell(c) => c.data.iter().map(|x| x.text()).collect::<R<_>>()?,
        Value::Str(s) => s.data.iter().map(|x| x.clone().unwrap_or_default()).collect(),
        other => vec![other.text()?],
    })
}

/// Nested dissection numbering of the block rows r0..=r1, columns c0..=c1 of `g`: the block is
/// split across its longer side (columns on a tie) at its middle line, the two parts are
/// numbered first and the separating line last; a block one cell thick is numbered in order.
fn dissect(g: &mut Matrix, r0: usize, r1: usize, c0: usize, c1: usize, next: &mut f64) {
    if r0 > r1 || c0 > c1 {
        return;
    }
    let (h, w) = (r1 - r0 + 1, c1 - c0 + 1);
    if h == 1 || w == 1 {
        for c in c0..=c1 {
            for r in r0..=r1 {
                g.set(r, c, *next);
                *next += 1.0;
            }
        }
        return;
    }
    if w >= h {
        let s = c0 + (w + 1) / 2 - 1;
        if s > c0 {
            dissect(g, r0, r1, c0, s - 1, next);
        }
        dissect(g, r0, r1, s + 1, c1, next);
        for r in r0..=r1 {
            g.set(r, s, *next);
            *next += 1.0;
        }
    } else {
        let s = r0 + (h + 1) / 2 - 1;
        if s > r0 {
            dissect(g, r0, s - 1, c0, c1, next);
        }
        dissect(g, s + 1, r1, c0, c1, next);
        for c in c0..=c1 {
            g.set(s, c, *next);
            *next += 1.0;
        }
    }
}

/// A maximum matching of columns to rows of the nonzero pattern (dmperm's one output):
/// each column in turn takes the first free row, or one freed along an augmenting path.
fn max_matching(m: &Matrix) -> Vec<Option<usize>> {
    let (r, c) = (m.rows(), m.cols());
    let rows_of: Vec<Vec<usize>> = (0..c).map(|j| (0..r).filter(|&i| m.data[j * r + i] != 0.0).collect()).collect();
    let mut col_of_row: Vec<Option<usize>> = vec![None; r];
    let mut row_of_col: Vec<Option<usize>> = vec![None; c];
    fn augment(j: usize, rows_of: &[Vec<usize>], col_of_row: &mut [Option<usize>], row_of_col: &mut [Option<usize>], seen: &mut [bool]) -> bool {
        for &i in &rows_of[j] {
            if seen[i] {
                continue;
            }
            seen[i] = true;
            let free = match col_of_row[i] {
                None => true,
                Some(k) => augment(k, rows_of, col_of_row, row_of_col, seen),
            };
            if free {
                col_of_row[i] = Some(j);
                row_of_col[j] = Some(i);
                return true;
            }
        }
        false
    }
    for j in 0..c {
        // A free row first (the cheap assignment), then an augmenting path.
        if let Some(&i) = rows_of[j].iter().find(|&&i| col_of_row[i].is_none()) {
            col_of_row[i] = Some(j);
            row_of_col[j] = Some(i);
            continue;
        }
        let mut seen = vec![false; r];
        augment(j, &rows_of, &mut col_of_row, &mut row_of_col, &mut seen);
    }
    row_of_col
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> R<Option<Vec<Value>>> {
    match name {
        "matlivre_decomposition_type" => Ok(Some(vec![decomposition_type(a)?])),
        // details(obj): the object's display without the "name =" line.
        "details" => {
            let v = a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?;
            if _nargout > 0 {
                return rt("Too many output arguments.");
            }
            let text = match v {
                Value::Object(o) => {
                    let t = crate::class::display_object("x", o, _it.format);
                    t.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or(t)
                }
                other => crate::display::disp_value(other, _it.format),
            };
            _it.emit(crate::interp::OutputKind::Text(format!("{text}\n")));
            Ok(Some(Vec::new()))
        }
        "rowfilter" => Ok(Some(vec![rowfilter(a)?])),
        "matlivre_rowfilter_method" => Ok(Some(vec![Value::Cell(CellArray::row(vec![rowfilter_method(a)?]))])),
        "nested" => {
            let n = a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?.scalar()?.max(0.0) as usize;
            let mut g = Matrix::zeros(n, n);
            if n > 2 {
                let mut next = 1.0;
                dissect(&mut g, 1, n - 2, 1, n - 2, &mut next);
            }
            Ok(Some(vec![Value::Num(g)]))
        }
        "dmperm" => {
            let m = a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?.to_matrix()?;
            if _nargout > 1 {
                return rt("The block triangular form of dmperm (more than one output) is not supported in this version.");
            }
            Ok(Some(vec![Value::Num(Matrix::row(max_matching(&m).into_iter().map(|r| r.map_or(0.0, |r| (r + 1) as f64)).collect()))]))
        }
        // addprop(obj, name) on a dynamicprops object: the property, and its meta.DynamicProperty.
        "addprop" if matches!(a.first(), Some(Value::Object(o)) if o.class.is_a("dynamicprops")) => {
            let Some(Value::Object(o)) = a.first() else { unreachable!() };
            let n = a.get(1).ok_or_else(|| LangError::new("Not enough input arguments.", 0))?.text()?;
            if o.has_prop(&n) {
                return rt(format!("'{n}' is already defined as a property."));
            }
            if !n.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) || !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return rt(format!("'{n}' is not a valid property name."));
            }
            let mut s = o.dyn_props().unwrap_or_default();
            s.set(&n, Value::empty());
            o.set_dyn_props(s);
            Ok(Some(vec![dynamic_property(&n, &a[0])]))
        }
        "addprop" | "rmprop" => {
            let Some(Value::Table(t)) = a.first() else { return Ok(None) };
            let mut t = t.clone();
            let names = names_of(a.get(1).ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)?;
            if name == "rmprop" {
                t.custom.retain(|(n, _, _)| !names.contains(n));
                return Ok(Some(vec![Value::Table(t)]));
            }
            let kinds = names_of(a.get(2).ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)?;
            let bad = || LangError::new("For each custom property, specify 'variable' or 'table' to indicate whether the property applies to the variables, or to the table as a whole. Specify these settings in a string array or cell array of character vectors.", 0);
            if kinds.len() != names.len() {
                return Err(bad());
            }
            for (n, k) in names.iter().zip(&kinds) {
                let per_var = match k.as_str() {
                    "variable" => true,
                    "table" => false,
                    _ => return Err(bad()),
                };
                if let Some((_, pv, _)) = t.custom.iter().find(|(x, _, _)| x == n) {
                    return rt(format!("New custom property name '{n}' conflicts with existing '{}' custom property '{n}'.", if *pv { "variable" } else { "table" }));
                }
                if t.names.iter().any(|x| x == n) || matches!(n.as_str(), "Description" | "UserData" | "DimensionNames" | "VariableNames" | "RowNames") {
                    return rt(format!("Custom property name '{n}' conflicts with a table property or variable name."));
                }
                t.custom.push((n.clone(), per_var, Value::empty()));
            }
            Ok(Some(vec![Value::Table(t)]))
        }
        _ => Ok(None),
    }
}

/// The meta.DynamicProperty object addprop returns (it keeps its owner, for delete).
fn dynamic_property(name: &str, owner: &Value) -> Value {
    thread_local! {
        static DEF: std::cell::RefCell<Option<std::rc::Rc<crate::class::ClassDef>>> = const { std::cell::RefCell::new(None) };
    }
    let def = DEF.with(|d| {
        d.borrow_mut()
            .get_or_insert_with(|| {
                let src = "classdef DynamicProperty < handle\n  properties\n    Name\n    Description\n  end\n  properties (Hidden)\n    Owner\n  end\nend\n";
                let ast = crate::class::parse_classdef(src).expect("built-in class source parses");
                let mut def = crate::class::ClassDef::build(ast, &[]);
                def.name = "meta.DynamicProperty".into();
                std::rc::Rc::new(def)
            })
            .clone()
    });
    Value::Object(crate::class::Object { class: def, props: std::rc::Rc::new(std::cell::RefCell::new(vec![Value::chars(name), Value::chars(""), owner.clone()])) })
}

/// delete(p) of a meta.DynamicProperty: removes the property from its object.
pub fn delete_dynamic_property(p: &crate::class::Object) {
    let (Some(Value::Object(owner)), Some(name)) = (p.get("Owner"), p.get("Name").and_then(|v| v.text().ok())) else { return };
    if let Some(mut s) = owner.dyn_props() {
        s.remove_field(&name);
        owner.set_dyn_props(s);
    }
}

// ---- rowfilter ------------------------------------------------------------------------------

/// The class of rowfilter objects.
pub const RF_CLASS: &str = "matlab.io.RowFilter";

const RF_METHODS: &[&str] = &["gt", "lt", "ge", "le", "eq", "ne", "and", "or", "not"];

fn rf_class() -> Rc<ClassDef> {
    thread_local! {
        static DEF: RefCell<Option<Rc<ClassDef>>> = const { RefCell::new(None) };
    }
    DEF.with(|d| {
        d.borrow_mut()
            .get_or_insert_with(|| {
                let mut src = String::from("classdef RowFilter\n  properties (Hidden)\n    Names\n    Tree\n  end\n  methods\n");
                for m in RF_METHODS {
                    src.push_str(&format!("    function varargout = {m}(varargin)\n      varargout = matlivre_rowfilter_method('{m}', varargin{{:}});\n    end\n"));
                }
                src.push_str("  end\nend\n");
                let ast = crate::class::parse_classdef(&src).expect("built-in class source parses");
                let mut def = ClassDef::build(ast, &[]);
                def.name = RF_CLASS.into();
                Rc::new(def)
            })
            .clone()
    })
}

/// A rowfilter over `names` with constraint tree `tree`: {} unconstrained, {'var', name}
/// one selected variable, {'cmp', name, op, value}, {'and'|'or', t1, t2} or {'not', t}.
fn rf_value(names: &[String], tree: Value) -> Value {
    let names = Value::Cell(CellArray::row(names.iter().map(|n| Value::chars(n)).collect()));
    Value::Object(Object { class: rf_class(), props: Rc::new(RefCell::new(vec![names, tree])) })
}

fn rf_parts(o: &Object) -> (Vec<String>, Vec<Value>) {
    let names = match o.get("Names") {
        Some(Value::Cell(c)) => c.data.iter().filter_map(|v| v.text().ok()).collect(),
        _ => Vec::new(),
    };
    let tree = match o.get("Tree") {
        Some(Value::Cell(c)) => c.data,
        _ => Vec::new(),
    };
    (names, tree)
}

fn node(items: Vec<Value>) -> Value {
    Value::Cell(CellArray::row(items))
}

fn word(t: &[Value], k: usize) -> String {
    t.get(k).and_then(|v| v.text().ok()).unwrap_or_default()
}

/// `rf.Name`: the filter on that variable.
pub fn rowfilter_member(o: &Object, name: &str) -> R<Value> {
    let (names, _) = rf_parts(o);
    if !names.iter().any(|n| n == name) {
        return rt(format!("Unrecognized variable name for filtering. Select a variable for filtering using one of the following variable names: {}.", names.join(", ")));
    }
    Ok(rf_value(&names, node(vec![Value::chars("var"), Value::chars(name)])))
}

/// rowfilter(T), rowfilter(varnames).
fn rowfilter(a: &[Value]) -> R<Value> {
    let names: Vec<String> = match a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))? {
        Value::Table(t) => t.names.clone(),
        Value::Cell(c) => c.data.iter().map(|v| v.text()).collect::<R<_>>()?,
        Value::Str(s) => s.data.iter().map(|v| v.clone().unwrap_or_default()).collect(),
        v => vec![v.text()?],
    };
    Ok(rf_value(&names, node(Vec::new())))
}

/// The operator methods of rowfilter objects.
fn rowfilter_method(a: &[Value]) -> R<Value> {
    let op = a.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?.text()?;
    let args = &a[1..];
    let rf = |v: &Value| match v {
        Value::Object(o) if o.class.name == RF_CLASS => Some(rf_parts(o)),
        _ => None,
    };
    let out = match op.as_str() {
        "not" => {
            let Some((names, t)) = args.first().and_then(rf) else { return rt("Expected a row filter.") };
            rf_value(&names, node(vec![Value::chars("not"), node(t)]))
        }
        "and" | "or" => {
            let (Some((names, l)), Some((_, r))) = (args.first().and_then(rf), args.get(1).and_then(rf)) else {
                return rt("Row filters combine only with other row filters.");
            };
            rf_value(&names, node(vec![Value::chars(&op), node(l), node(r)]))
        }
        _ => {
            // One side is a filter on a variable, the other a value; `2 < rf.X` is X > 2.
            let symbol = |o: &str| match o {
                "gt" => ">",
                "lt" => "<",
                "ge" => ">=",
                "le" => "<=",
                "eq" => "==",
                _ => "~=",
            };
            let flip = |o: &str| match o {
                "gt" => "lt",
                "lt" => "gt",
                "ge" => "le",
                "le" => "ge",
                other => other,
            }
            .to_string();
            let (filter, value, o) = match (args.first().and_then(rf), args.get(1).and_then(rf)) {
                (Some(f), None) => (f, args.get(1).cloned().unwrap_or_else(Value::empty), op.clone()),
                (None, Some(f)) => (f, args[0].clone(), flip(&op)),
                _ => return rt("Compare a row filter variable with a value."),
            };
            let (names, t) = filter;
            if word(&t, 0) != "var" {
                return rt("Compare a row filter variable with a value.");
            }
            rf_value(&names, node(vec![Value::chars("cmp"), t[1].clone(), Value::chars(symbol(&o)), value]))
        }
    };
    Ok(out)
}

/// A constraint as MATLAB writes it (`X <= 1 | Name == "d"`).
fn tree_text(t: &[Value]) -> String {
    let child = |v: &Value| match v {
        Value::Cell(c) => c.data.clone(),
        _ => Vec::new(),
    };
    match word(t, 0).as_str() {
        "cmp" => {
            let value = match t.get(3) {
                Some(Value::Str(s)) if s.data.len() == 1 => format!("\"{}\"", s.data[0].clone().unwrap_or_default()),
                Some(v) if v.is_text() => format!("'{}'", v.text().unwrap_or_default()),
                Some(v) => v.scalar().map(|x| format!("{x}")).unwrap_or_else(|_| crate::display::summary(v, crate::display::NumFormat::Short)),
                None => String::new(),
            };
            format!("{} {} {value}", word(t, 1), word(t, 2))
        }
        "not" => format!("~({})", tree_text(&child(&t[1]))),
        op @ ("and" | "or") => {
            let side = |v: &Value| {
                let c = child(v);
                let s = tree_text(&c);
                // & binds tighter than |: an or inside an and keeps its parentheses.
                if op == "and" && word(&c, 0) == "or" { format!("({s})") } else { s }
            };
            format!("{} {} {}", side(&t[1]), if op == "and" { "&" } else { "|" }, side(&t[2]))
        }
        _ => String::new(),
    }
}

fn rowfilter_text(o: &Object) -> String {
    let (names, t) = rf_parts(o);
    let vars = format!("  VariableNames: {}\n", names.join(", "));
    match word(&t, 0).as_str() {
        "" => format!("  RowFilter with no constraints and no selected variables\n\n    <unconstrained>\n\n{vars}"),
        "var" => format!("  RowFilter with no constraints on variable: {0}\n\n    {0} <unconstrained>\n\n{vars}", word(&t, 1)),
        k => {
            let plural = if k == "and" || k == "or" { "constraints" } else { "constraint" };
            format!("  RowFilter with {plural}:\n\n    {}\n\n{vars}", tree_text(&t))
        }
    }
}

/// The rows of `t` a rowfilter keeps, as a logical column.
pub fn rowfilter_mask(t: &crate::table::Table, o: &Object) -> R<Value> {
    fn eval(t: &crate::table::Table, tree: &[Value]) -> R<Value> {
        let child = |v: &Value| match v {
            Value::Cell(c) => c.data.clone(),
            _ => Vec::new(),
        };
        use crate::ast::BinOp;
        match word(tree, 0).as_str() {
            "cmp" => {
                let name = word(tree, 1);
                let k = t.names.iter().position(|n| *n == name).ok_or_else(|| LangError::new(format!("Unrecognized table variable name '{name}'."), 0))?;
                let mut col = t.vars[k].clone();
                let mut value = tree.get(3).cloned().unwrap_or_else(Value::empty);
                // Text compares as strings.
                if matches!(col, Value::Cell(_)) || value.is_text() {
                    col = Value::Str(crate::ops::to_str_array(&col)?);
                    value = Value::Str(crate::ops::to_str_array(&value)?);
                }
                let op = match word(tree, 2).as_str() {
                    ">" => BinOp::Gt,
                    "<" => BinOp::Lt,
                    ">=" => BinOp::Ge,
                    "<=" => BinOp::Le,
                    "==" => BinOp::Eq,
                    _ => BinOp::Ne,
                };
                crate::ops::binary(op, &col, &value)
            }
            "not" => crate::ops::unary(crate::ast::UnOp::Not, &eval(t, &child(&tree[1]))?),
            op @ ("and" | "or") => {
                let (l, r) = (eval(t, &child(&tree[1]))?, eval(t, &child(&tree[2]))?);
                crate::ops::binary(if op == "and" { BinOp::And } else { BinOp::Or }, &l, &r)
            }
            // No constraint keeps every row.
            _ => Ok(Value::Num(Matrix::from_dims(vec![t.nrows, 1], vec![1.0; t.nrows]).with_kind(Kind::Logical))),
        }
    }
    let (_, tree) = rf_parts(o);
    eval(t, &tree)
}
