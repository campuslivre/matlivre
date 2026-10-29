//! `table`: named column variables of equal height, MATLAB indexing forms, display, and
//! table builtins (including CSV `readtable` / `writetable`).

use crate::display::{fmt_elem, NumFormat};
use crate::error::{rt, LangError};
use crate::indexing;
use crate::nd::IndexArg;
use crate::value::*;

type R<T> = Result<T, LangError>;

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub names: Vec<String>,
    pub vars: Vec<Value>,
    pub nrows: usize,
    pub row_names: Option<Vec<String>>,
    pub description: String,
}

fn rows_of(v: &Value) -> usize {
    v.dims()[0]
}

/// Rows `rows` of a variable (all columns).
fn take_rows(v: &Value, rows: &IndexArg) -> R<Value> {
    indexing::index(v, &[rows.clone(), IndexArg::All])
}

fn cellstr_names(v: &Value) -> R<Vec<String>> {
    match v {
        Value::Cell(c) => c.data.iter().map(|x| x.text()).collect(),
        Value::Str(s) => Ok(s.data.iter().map(|x| x.clone().unwrap_or_default()).collect()),
        other => Ok(vec![other.text()?]),
    }
}

impl Table {
    pub fn new(names: Vec<String>, vars: Vec<Value>) -> R<Table> {
        let nrows = vars.first().map(rows_of).unwrap_or(0);
        for (n, v) in names.iter().zip(&vars) {
            if rows_of(v) != nrows {
                return rt(format!("All table variables must have the same number of rows (variable '{n}' has {}, expected {nrows}).", rows_of(v)));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for n in &names {
            if !seen.insert(n) {
                return rt(format!("Duplicate table variable name: '{n}'."));
            }
        }
        Ok(Table { names, vars, nrows, row_names: None, description: String::new() })
    }

    pub fn var(&self, name: &str) -> Option<&Value> {
        self.names.iter().position(|n| n == name).map(|i| &self.vars[i])
    }

    pub fn set_var(&mut self, name: &str, v: Value) -> R<()> {
        let v = if self.vars.is_empty() || rows_of(&v) == self.nrows {
            v
        } else if v.numel() == 1 && self.nrows > 0 {
            // A scalar expands to fill the height (MATLAB does this for new variables).
            indexing::index(&v, &[IndexArg::List { ix: vec![0; self.nrows], dims: vec![self.nrows, 1], from_mask: false }])?
        } else {
            return rt(format!("To assign to or create a variable in a table, the number of rows must match the height of the table ({}).", self.nrows));
        };
        if self.vars.is_empty() {
            self.nrows = rows_of(&v);
        }
        match self.names.iter().position(|n| n == name) {
            Some(i) => self.vars[i] = v,
            None => {
                self.names.push(name.to_string());
                self.vars.push(v);
            }
        }
        Ok(())
    }

    pub fn properties(&self) -> Value {
        let mut s = StructArray::scalar();
        s.set("Description", Value::chars(&self.description));
        s.set("VariableNames", Value::Cell(CellArray::row(self.names.iter().map(|n| Value::chars(n)).collect())));
        s.set("RowNames", Value::Cell(match &self.row_names {
            Some(r) => CellArray::col(r.iter().map(|n| Value::chars(n)).collect()),
            None => CellArray::from_dims(vec![0, 0], Vec::new()),
        }));
        s.set("DimensionNames", Value::Cell(CellArray::row(vec![Value::chars("Row"), Value::chars("Variables")])));
        Value::Struct(s)
    }

    pub fn set_properties(&mut self, v: &Value) -> R<()> {
        let Value::Struct(s) = v else { return rt("Table properties must be a struct.") };
        if let Some(n) = s.get("VariableNames") {
            let names = cellstr_names(n)?;
            if names.len() != self.vars.len() {
                return rt(format!("The VariableNames property must contain one name for each variable in the table ({}).", self.vars.len()));
            }
            self.names = names;
        }
        if let Some(r) = s.get("RowNames") {
            let names = cellstr_names(r)?;
            self.row_names = if names.is_empty() { None } else if names.len() == self.nrows { Some(names) } else { return rt("The number of row names must match the number of rows.") };
        }
        if let Some(d) = s.get("Description") {
            self.description = d.text()?;
        }
        Ok(())
    }

    /// Row subscript: numbers, logical mask, ':', or row names.
    fn row_arg(&self, v: &Value) -> R<IndexArg> {
        if v.is_text() || matches!(v, Value::Cell(_) | Value::Str(_)) {
            if v.text().map(|t| t == ":").unwrap_or(false) {
                return Ok(IndexArg::All);
            }
            let names = cellstr_names(v)?;
            let rn = self.row_names.as_ref().ok_or_else(|| LangError::new("This table has no row names.", 0))?;
            let ix = names.iter().map(|n| rn.iter().position(|r| r == n).ok_or_else(|| LangError::new(format!("Unrecognized row name '{n}'."), 0))).collect::<R<Vec<_>>>()?;
            let n = ix.len();
            return Ok(IndexArg::List { ix, dims: vec![n, 1], from_mask: false });
        }
        indexing::to_index_arg(v)
    }

    /// Variable subscript: numbers, logical mask, ':', or names.
    fn var_positions(&self, v: &Value) -> R<Vec<usize>> {
        if v.is_text() || matches!(v, Value::Cell(_) | Value::Str(_)) {
            if v.text().map(|t| t == ":").unwrap_or(false) {
                return Ok((0..self.vars.len()).collect());
            }
            return cellstr_names(v)?
                .iter()
                .map(|n| self.names.iter().position(|m| m == n).ok_or_else(|| LangError::new(format!("Unrecognized table variable name '{n}'."), 0)))
                .collect();
        }
        match indexing::to_index_arg(v)? {
            IndexArg::All => Ok((0..self.vars.len()).collect()),
            IndexArg::List { ix, .. } => {
                if let Some(&bad) = ix.iter().find(|&&i| i >= self.vars.len()) {
                    return rt(format!("Variable index exceeds table dimensions ({} > {}).", bad + 1, self.vars.len()));
                }
                Ok(ix)
            }
        }
    }

    fn subscripts(&self, args: &[Value]) -> R<(IndexArg, Vec<usize>)> {
        if args.len() != 2 {
            return rt("Subscripting a table using linear indexing (one subscript) or multidimensional indexing (three or more subscripts) is not supported. Use a row subscript and a variable subscript.");
        }
        Ok((self.row_arg(&args[0])?, self.var_positions(&args[1])?))
    }

    /// `T(rows, vars)`.
    pub fn paren(&self, args: &[Value]) -> R<Table> {
        let (rows, vars) = self.subscripts(args)?;
        let mut out = Table { names: Vec::new(), vars: Vec::new(), nrows: 0, row_names: None, description: self.description.clone() };
        for &k in &vars {
            out.names.push(self.names[k].clone());
            out.vars.push(take_rows(&self.vars[k], &rows)?);
        }
        out.nrows = match &rows {
            IndexArg::All => self.nrows,
            IndexArg::List { ix, .. } => {
                if let Some(&bad) = ix.iter().find(|&&i| i >= self.nrows) {
                    return rt(format!("Row index exceeds table dimensions ({} > {}).", bad + 1, self.nrows));
                }
                ix.len()
            }
        };
        if let Some(rn) = &self.row_names {
            out.row_names = Some(match &rows {
                IndexArg::All => rn.clone(),
                IndexArg::List { ix, .. } => ix.iter().map(|&i| rn[i].clone()).collect(),
            });
        }
        Ok(out)
    }

    /// `T{rows, vars}`: the selected contents concatenated horizontally.
    pub fn brace(&self, args: &[Value]) -> R<Value> {
        let sub = self.paren(args)?;
        if sub.vars.is_empty() {
            return Ok(Value::empty());
        }
        indexing::hconcat(sub.vars)
    }

    /// `T(rows, vars) = rhs` (a table with matching variables, or a cell row).
    pub fn assign_paren(mut self, args: Vec<Value>, rhs: Value) -> R<Table> {
        let (rows, vars) = self.subscripts(&args)?;
        match rhs {
            Value::Table(r) => {
                if r.vars.len() != vars.len() {
                    return rt("The right-hand side table must have the same number of variables as the assignment target.");
                }
                for (j, &k) in vars.iter().enumerate() {
                    let new = indexing::assign(Some(self.vars[k].clone()), &[rows.clone(), IndexArg::All], r.vars[j].clone())?;
                    self.vars[k] = new;
                }
            }
            Value::Cell(c) => {
                if c.data.len() != vars.len() {
                    return rt("The number of cells must match the number of table variables being assigned.");
                }
                for (j, &k) in vars.iter().enumerate() {
                    let item = match (&self.vars[k], &c.data[j]) {
                        (Value::Cell(_), v) => Value::Cell(CellArray::new(1, 1, vec![v.clone()])),
                        (_, v) => v.clone(),
                    };
                    let new = indexing::assign(Some(self.vars[k].clone()), &[rows.clone(), IndexArg::All], item)?;
                    self.vars[k] = new;
                }
            }
            Value::Num(m) if m.dims == [0, 0] => {
                // T(rows, :) = [] deletes rows.
                for k in 0..self.vars.len() {
                    self.vars[k] = indexing::delete(self.vars[k].clone(), &[rows.clone(), IndexArg::All])?;
                }
            }
            other => {
                for &k in &vars {
                    let new = indexing::assign(Some(self.vars[k].clone()), &[rows.clone(), IndexArg::All], other.clone())?;
                    self.vars[k] = new;
                }
            }
        }
        self.nrows = self.vars.first().map(rows_of).unwrap_or(0);
        if self.vars.iter().any(|v| rows_of(v) != self.nrows) {
            return rt("Table variables would have different heights after this assignment.");
        }
        Ok(self)
    }

    /// `T{rows, var} = value`.
    pub fn assign_brace(mut self, args: Vec<Value>, rhs: Value) -> R<Table> {
        let (rows, vars) = self.subscripts(&args)?;
        if vars.len() != 1 {
            return rt("Brace assignment into a table supports one variable at a time in this version.");
        }
        let k = vars[0];
        let item = match &self.vars[k] {
            Value::Cell(_) => Value::Cell(CellArray::new(1, 1, vec![rhs])),
            _ => rhs,
        };
        self.vars[k] = indexing::assign(Some(self.vars[k].clone()), &[rows, IndexArg::All], item)?;
        self.nrows = self.vars.iter().map(rows_of).max().unwrap_or(0);
        Ok(self)
    }

    fn cell_text(v: &Value, r: usize, f: NumFormat) -> String {
        match v {
            Value::Num(m) => {
                let cols = m.cols();
                let rows = m.rows();
                if m.kind == Kind::Char {
                    return format!("'{}'", (0..cols).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or('?')).collect::<String>());
                }
                (0..cols)
                    .map(|c| {
                        let i = c * rows + r;
                        if m.kind == Kind::Logical {
                            (if m.data[i] != 0.0 { "true" } else { "false" }).to_string()
                        } else {
                            fmt_elem(m, i, f)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("    ")
            }
            Value::Cell(c) => {
                let x = &c.data[r];
                match x {
                    Value::Num(m) if m.kind == Kind::Char => format!("{{'{}'}}", m.to_string_lossy()),
                    other => format!("{{{}}}", crate::display::summary(other, f)),
                }
            }
            Value::Str(s) => s.data[r].as_ref().map(|t| format!("\"{t}\"")).unwrap_or_else(|| "<missing>".into()),
            Value::Time(t) => t.format_elem(r),
            Value::Sym(s) => s.data[r].to_string(),
            other => crate::display::summary(other, f),
        }
    }

    /// The rows under the header, as MATLAB prints them.
    pub fn body(&self, f: NumFormat) -> String {
        let mut cols: Vec<(String, Vec<String>, bool)> = Vec::new();
        if let Some(rn) = &self.row_names {
            let w = rn.iter().map(|s| s.chars().count()).max().unwrap_or(0);
            cols.push((String::new(), rn.iter().map(|s| format!("{s:<w$}")).collect(), false));
        }
        for (n, v) in self.names.iter().zip(&self.vars) {
            let numeric = matches!(v, Value::Num(m) if m.kind != Kind::Char);
            cols.push((n.clone(), (0..self.nrows).map(|r| Table::cell_text(v, r, f)).collect(), numeric));
        }
        let widths: Vec<usize> = cols.iter().map(|(n, cells, _)| cells.iter().map(|c| c.chars().count()).max().unwrap_or(0).max(n.chars().count())).collect();
        let mut out = String::new();
        let center = |s: &str, w: usize| {
            let pad = w - s.chars().count();
            format!("{}{}{}", " ".repeat(pad / 2), s, " ".repeat(pad - pad / 2))
        };
        let line = |out: &mut String, items: Vec<String>| {
            out.push_str("    ");
            out.push_str(&items.join("    "));
            out.push('\n');
        };
        line(&mut out, cols.iter().zip(&widths).map(|((n, _, _), &w)| center(n, w)).collect());
        line(&mut out, cols.iter().zip(&widths).map(|((n, _, _), &w)| if n.is_empty() { " ".repeat(w) } else { "_".repeat(w) }).collect());
        out.push('\n');
        for r in 0..self.nrows {
            line(
                &mut out,
                cols.iter()
                    .zip(&widths)
                    .map(|((_, cells, numeric), &w)| if *numeric { format!("{:>w$}", cells[r]) } else { format!("{:<w$}", cells[r]) })
                    .collect(),
            );
        }
        out.trim_end_matches('\n').to_string()
    }

    pub fn display(&self, name: &str, f: NumFormat) -> String {
        if self.vars.is_empty() {
            return format!("{name} =\n\n  {}×0 empty table", self.nrows);
        }
        format!("{name} = {}×{} table\n\n{}", self.nrows, self.vars.len(), self.body(f))
    }
}

pub fn concat(items: Vec<Value>, dim: usize) -> R<Value> {
    let tables: Vec<Table> = items
        .into_iter()
        .filter(|v| !matches!(v, Value::Num(m) if m.dims == [0, 0]))
        .map(|v| match v {
            Value::Table(t) => Ok(*t),
            other => rt(format!("Concatenating a table with a {} is not supported.", other.class_name())),
        })
        .collect::<R<_>>()?;
    let mut it = tables.into_iter();
    let mut out = it.next().unwrap();
    for t in it {
        if dim == 0 {
            if t.names.len() != out.names.len() {
                return rt("All tables being vertically concatenated must have the same number of variables.");
            }
            for (k, n) in out.names.clone().iter().enumerate() {
                let j = t.names.iter().position(|m| m == n).ok_or_else(|| LangError::new(format!("All tables being vertically concatenated must have the same variable names (missing '{n}')."), 0))?;
                out.vars[k] = indexing::vconcat(vec![out.vars[k].clone(), t.vars[j].clone()])?;
            }
            out.nrows += t.nrows;
            out.row_names = None;
        } else {
            if t.nrows != out.nrows {
                return rt("All tables being horizontally concatenated must have the same number of rows.");
            }
            for (n, v) in t.names.into_iter().zip(t.vars) {
                if out.names.contains(&n) {
                    return rt(format!("Duplicate table variable name: '{n}'."));
                }
                out.names.push(n);
                out.vars.push(v);
            }
        }
    }
    Ok(Value::Table(Box::new(out)))
}

// ---- builtins ------------------------------------------------------------------------------

pub const NAMES: &[&str] = &[
    "table", "array2table", "cell2table", "struct2table", "table2array", "table2cell", "table2struct", "height", "width", "head", "tail", "sortrows",
    "readtable", "writetable", "removevars", "renamevars", "istable", "summary",
];

fn one(v: Value) -> R<Option<Vec<Value>>> {
    Ok(Some(vec![v]))
}

fn table_arg<'a>(args: &'a [Value], name: &str) -> R<&'a Table> {
    match args.first() {
        Some(Value::Table(t)) => Ok(t),
        _ => rt(format!("The first input to '{name}' must be a table.")),
    }
}

/// Splits `args` into data arguments and name-value options.
fn split_opts(args: &[Value], names: &[&str]) -> (Vec<Value>, Vec<(String, Value)>) {
    let mut data = Vec::new();
    let mut opts = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i].is_text() {
            if let Ok(t) = args[i].text() {
                if let Some(n) = names.iter().find(|n| n.eq_ignore_ascii_case(&t)) {
                    if let Some(v) = args.get(i + 1) {
                        opts.push((n.to_string(), v.clone()));
                        i += 2;
                        continue;
                    }
                }
            }
        }
        data.push(args[i].clone());
        i += 1;
    }
    (data, opts)
}

fn columns_of(m: &Value) -> R<Vec<Value>> {
    let (_, cols) = m.dims2();
    (0..cols).map(|c| indexing::index(m, &[IndexArg::All, IndexArg::scalar(c)])).collect()
}

pub fn call(it_arg_names: &[Option<String>], name: &str, args: &[Value], _nargout: usize) -> R<Option<Vec<Value>>> {
    match name {
        "table" => {
            let (data, opts) = split_opts(args, &["VariableNames", "RowNames", "Size", "VariableTypes"]);
            let mut names: Vec<String> = (0..data.len())
                .map(|i| it_arg_names.get(i).cloned().flatten().unwrap_or_else(|| format!("Var{}", i + 1)))
                .collect();
            let mut row_names = None;
            for (k, v) in &opts {
                match k.as_str() {
                    "VariableNames" => names = cellstr_names(v)?,
                    "RowNames" => row_names = Some(cellstr_names(v)?),
                    _ => {}
                }
            }
            if names.len() != data.len() {
                return rt("The VariableNames property must contain one name for each variable in the table.");
            }
            let mut t = Table::new(names, data)?;
            if let Some(rn) = row_names {
                if t.vars.is_empty() {
                    t.nrows = rn.len();
                }
                if rn.len() != t.nrows {
                    return rt("The number of row names must match the number of rows.");
                }
                t.row_names = Some(rn);
            }
            one(Value::Table(Box::new(t)))
        }
        "array2table" | "cell2table" => {
            let (data, opts) = split_opts(args, &["VariableNames", "RowNames"]);
            let src = data.first().ok_or_else(|| LangError::new(format!("Not enough input arguments for '{name}'."), 0))?;
            let mut cols = columns_of(src)?;
            if name == "cell2table" {
                // Columns of numeric scalars become numeric variables.
                cols = cols
                    .into_iter()
                    .map(|c| match &c {
                        Value::Cell(cc) if cc.data.iter().all(|v| matches!(v, Value::Num(m) if m.is_scalar() && m.kind != Kind::Char)) => {
                            indexing::vconcat(cc.data.clone()).unwrap_or(c)
                        }
                        _ => c,
                    })
                    .collect();
            }
            let base = it_arg_names.first().cloned().flatten().unwrap_or_else(|| "Var".into());
            let _ = base;
            let mut names: Vec<String> = (1..=cols.len()).map(|i| format!("Var{i}")).collect();
            let mut t_rn = None;
            for (k, v) in &opts {
                match k.as_str() {
                    "VariableNames" => names = cellstr_names(v)?,
                    "RowNames" => t_rn = Some(cellstr_names(v)?),
                    _ => {}
                }
            }
            let mut t = Table::new(names, cols)?;
            t.row_names = t_rn;
            one(Value::Table(Box::new(t)))
        }
        "struct2table" => {
            let Some(Value::Struct(s)) = args.first() else { return rt("Input must be a struct.") };
            let mut vars = Vec::new();
            for (k, _) in s.fields.iter().enumerate() {
                let items: Vec<Value> = s.elems.iter().map(|e| e[k].clone()).collect();
                let all_scalar = items.iter().all(|v| matches!(v, Value::Num(m) if m.is_scalar() && m.kind != Kind::Char));
                vars.push(if all_scalar { indexing::vconcat(items)? } else { Value::Cell(CellArray::col(items)) });
            }
            one(Value::Table(Box::new(Table::new(s.fields.clone(), vars)?)))
        }
        "table2array" => {
            let t = table_arg(args, name)?;
            if t.vars.is_empty() {
                return one(Value::empty());
            }
            one(indexing::hconcat(t.vars.clone())?)
        }
        "table2cell" => {
            let t = table_arg(args, name)?;
            let mut data = Vec::with_capacity(t.nrows * t.vars.len());
            for v in &t.vars {
                for r in 0..t.nrows {
                    let x = indexing::index(v, &[IndexArg::scalar(r), IndexArg::All])?;
                    data.push(match (v, x) {
                        (Value::Cell(_), Value::Cell(c)) if c.data.len() == 1 => c.data[0].clone(),
                        (_, x) => x,
                    });
                }
            }
            one(Value::Cell(CellArray::new(t.nrows, t.vars.len(), data)))
        }
        "table2struct" => {
            let t = table_arg(args, name)?;
            let mut elems = Vec::with_capacity(t.nrows);
            for r in 0..t.nrows {
                let mut e = Vec::new();
                for v in &t.vars {
                    let x = indexing::index(v, &[IndexArg::scalar(r), IndexArg::All])?;
                    e.push(match (v, x) {
                        (Value::Cell(_), Value::Cell(c)) if c.data.len() == 1 => c.data[0].clone(),
                        (_, x) => x,
                    });
                }
                elems.push(e);
            }
            one(Value::Struct(StructArray { dims: vec![t.nrows, 1], fields: t.names.clone(), elems }))
        }
        "height" => one(Value::num(table_arg(args, name)?.nrows as f64)),
        "width" => one(Value::num(table_arg(args, name)?.vars.len() as f64)),
        "istable" => one(Value::bool(matches!(args.first(), Some(Value::Table(_))))),
        "head" | "tail" => {
            let k = match args.get(1) {
                Some(v) => v.as_usize()?,
                None => 8,
            };
            match args.first() {
                Some(Value::Table(t)) => {
                    let n = k.min(t.nrows);
                    let start = if name == "head" { 0 } else { t.nrows - n };
                    let rows = Value::Num(Matrix::row((start..start + n).map(|i| (i + 1) as f64).collect()));
                    one(Value::Table(Box::new(t.paren(&[rows, Value::chars(":")])?)))
                }
                Some(v) => {
                    let r = v.dims()[0];
                    let n = k.min(r);
                    let start = if name == "head" { 0 } else { r - n };
                    let ix = IndexArg::List { ix: (start..start + n).collect(), dims: vec![1, n], from_mask: false };
                    one(indexing::index(v, &[ix, IndexArg::All])?)
                }
                None => rt("Not enough input arguments."),
            }
        }
        "sortrows" => {
            let Some(Value::Table(t)) = args.first() else { return Ok(None) };
            let mut keys: Vec<(usize, bool)> = Vec::new();
            let descend_all = args.iter().skip(1).any(|v| v.text().map(|s| s.eq_ignore_ascii_case("descend")).unwrap_or(false));
            match args.get(1) {
                Some(v) if !(v.text().map(|s| s.eq_ignore_ascii_case("descend") || s.eq_ignore_ascii_case("ascend")).unwrap_or(false)) => {
                    if let Ok(m) = v.to_matrix() {
                        if m.kind != Kind::Char {
                            for &x in &m.data {
                                keys.push(((x.abs() as usize).saturating_sub(1), x < 0.0 || descend_all));
                            }
                        }
                    }
                    if keys.is_empty() {
                        for n in cellstr_names(v)? {
                            let k = t.names.iter().position(|m| *m == n).ok_or_else(|| LangError::new(format!("Unrecognized table variable name '{n}'."), 0))?;
                            keys.push((k, descend_all));
                        }
                    }
                }
                _ => keys = (0..t.vars.len()).map(|k| (k, descend_all)).collect(),
            }
            let key_vals: Vec<Vec<SortKey>> = keys.iter().map(|&(k, _)| sort_keys(&t.vars[k], t.nrows)).collect::<R<_>>()?;
            let mut order: Vec<usize> = (0..t.nrows).collect();
            order.sort_by(|&a, &b| {
                for (j, &(_, desc)) in keys.iter().enumerate() {
                    let o = key_vals[j][a].cmp_with(&key_vals[j][b]);
                    let o = if desc { o.reverse() } else { o };
                    if o != std::cmp::Ordering::Equal {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            });
            let rows = Value::Num(Matrix::col(order.iter().map(|&i| (i + 1) as f64).collect()));
            let sorted = t.paren(&[rows.clone(), Value::chars(":")])?;
            Ok(Some(vec![Value::Table(Box::new(sorted)), rows]))
        }
        "removevars" => {
            let t = table_arg(args, name)?;
            let drop = t.var_positions(args.get(1).ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)?;
            let mut out = t.clone();
            let keep: Vec<usize> = (0..t.vars.len()).filter(|k| !drop.contains(k)).collect();
            out.names = keep.iter().map(|&k| t.names[k].clone()).collect();
            out.vars = keep.iter().map(|&k| t.vars[k].clone()).collect();
            one(Value::Table(Box::new(out)))
        }
        "renamevars" => {
            let t = table_arg(args, name)?;
            let olds = t.var_positions(args.get(1).ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)?;
            let news = cellstr_names(args.get(2).ok_or_else(|| LangError::new("Not enough input arguments.", 0))?)?;
            let mut out = t.clone();
            for (k, n) in olds.into_iter().zip(news) {
                out.names[k] = n;
            }
            one(Value::Table(Box::new(out)))
        }
        "summary" => {
            let t = table_arg(args, name)?;
            let mut lines = vec!["Variables:".to_string(), String::new()];
            for (n, v) in t.names.iter().zip(&t.vars) {
                lines.push(format!("    {n}: {} {}", crate::nd::size_text(&v.dims()), v.class_name()));
                if let Value::Num(m) = v {
                    if m.kind != Kind::Char && !m.is_empty() {
                        let finite: Vec<f64> = m.data.iter().copied().filter(|x| !x.is_nan()).collect();
                        if !finite.is_empty() {
                            let min = finite.iter().cloned().fold(f64::INFINITY, f64::min);
                            let max = finite.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                            let mut s = finite.clone();
                            s.sort_by(|a, b| a.partial_cmp(b).unwrap());
                            let med = if s.len() % 2 == 1 { s[s.len() / 2] } else { (s[s.len() / 2 - 1] + s[s.len() / 2]) / 2.0 };
                            lines.push(format!("        Min: {}   Median: {}   Max: {}", crate::display::fmt_g(min, 5), crate::display::fmt_g(med, 5), crate::display::fmt_g(max, 5)));
                        }
                    }
                }
            }
            Ok(Some(vec![Value::chars(&(lines.join("\n") + "\n"))]))
        }
        "readtable" => {
            let path = args.first().ok_or_else(|| LangError::new("Not enough input arguments.", 0))?.text()?;
            let text = std::fs::read_to_string(&path).map_err(|e| LangError::new(format!("Unable to find or open '{path}': {e}"), 0))?;
            one(Value::Table(Box::new(read_csv(&text)?)))
        }
        "writetable" => {
            let t = table_arg(args, name)?;
            let path = args.get(1).map(|v| v.text()).transpose()?.unwrap_or_else(|| "table.csv".into());
            std::fs::write(&path, write_csv(t)?).map_err(|e| LangError::new(format!("Unable to write '{path}': {e}"), 0))?;
            Ok(Some(Vec::new()))
        }
        _ => Ok(None),
    }
}

#[derive(Clone)]
enum SortKey {
    Num(f64),
    Text(String),
}

impl SortKey {
    fn cmp_with(&self, o: &SortKey) -> std::cmp::Ordering {
        match (self, o) {
            (SortKey::Num(a), SortKey::Num(b)) => a.partial_cmp(b).unwrap_or_else(|| a.is_nan().cmp(&b.is_nan())),
            (SortKey::Text(a), SortKey::Text(b)) => a.cmp(b),
            (SortKey::Num(_), SortKey::Text(_)) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Greater,
        }
    }
}

fn sort_keys(v: &Value, n: usize) -> R<Vec<SortKey>> {
    Ok(match v {
        Value::Num(m) if m.kind == Kind::Char => (0..n).map(|r| SortKey::Text((0..m.cols()).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or(' ')).collect())).collect(),
        Value::Num(m) => (0..n).map(|r| SortKey::Num(m.data[r])).collect(),
        Value::Cell(c) => c.data.iter().take(n).map(|x| x.text().map(SortKey::Text).unwrap_or(SortKey::Num(f64::NAN))).collect(),
        Value::Str(s) => s.data.iter().take(n).map(|x| SortKey::Text(x.clone().unwrap_or_default())).collect(),
        Value::Time(t) => t.data.iter().take(n).map(|p| SortKey::Num(p[0])).collect(),
        other => return rt(format!("Sorting by a variable of class '{}' is not supported.", other.class_name())),
    })
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_q && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => in_q = !in_q,
            ',' if !in_q => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

fn valid_name(s: &str, i: usize) -> String {
    let mut n: String = s.trim().chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if n.is_empty() || !n.chars().next().unwrap().is_alphabetic() {
        n = if n.is_empty() { format!("Var{}", i + 1) } else { format!("x{n}") };
    }
    n
}

pub fn read_csv(text: &str) -> R<Table> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().ok_or_else(|| LangError::new("The file is empty.", 0))?;
    let names: Vec<String> = split_csv_line(header).iter().enumerate().map(|(i, s)| valid_name(s, i)).collect();
    let rows: Vec<Vec<String>> = lines.map(split_csv_line).collect();
    let mut vars = Vec::new();
    for k in 0..names.len() {
        let cells: Vec<String> = rows.iter().map(|r| r.get(k).cloned().unwrap_or_default().trim().to_string()).collect();
        let numeric = cells.iter().all(|c| c.is_empty() || c.parse::<f64>().is_ok() || c.eq_ignore_ascii_case("nan"));
        vars.push(if numeric {
            Value::Num(Matrix::col(cells.iter().map(|c| c.parse::<f64>().unwrap_or(f64::NAN)).collect()))
        } else {
            Value::Cell(CellArray::col(cells.iter().map(|c| Value::chars(c)).collect()))
        });
    }
    Table::new(names, vars)
}

pub fn write_csv(t: &Table) -> R<String> {
    let quote = |s: &str| if s.contains(',') || s.contains('"') { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() };
    let mut out = t.names.join(",") + "\n";
    for r in 0..t.nrows {
        let mut cells = Vec::new();
        for v in &t.vars {
            let x = indexing::index(v, &[IndexArg::scalar(r), IndexArg::All])?;
            let s = match &x {
                Value::Cell(c) if c.data.len() == 1 => c.data[0].text().unwrap_or_default(),
                Value::Num(m) if m.kind == Kind::Char => m.to_string_lossy(),
                Value::Num(m) => m.data.iter().map(|d| crate::display::fmt_g(*d, 15)).collect::<Vec<_>>().join(","),
                Value::Str(s) => s.data[0].clone().unwrap_or_default(),
                Value::Time(tt) => tt.format_elem(0),
                other => crate::display::summary(other, NumFormat::Short),
            };
            cells.push(quote(&s));
        }
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    Ok(out)
}
