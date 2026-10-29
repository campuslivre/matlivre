//! Tree-walking interpreter with a MATLAB-style base workspace, local functions, and
//! streamed outputs (text, values, figures, errors) anchored to source lines.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::ast::*;
use crate::display::{display_value, summary, NumFormat};
use crate::error::{rt, LangError};
use crate::graphics::Figure;
use crate::ops;
use crate::value::*;

type R<T> = Result<T, LangError>;

#[derive(Debug, Clone, PartialEq)]
pub enum OutputKind {
    /// Text printed by `disp`, `fprintf`, …
    Text(String),
    /// Display of an unsuppressed statement (`x = 5`).
    Value { name: String, text: String },
    Figure(Figure),
    Warning(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    /// 1-based line within the code block of the top-level statement that produced it.
    pub line: usize,
    pub kind: OutputKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarInfo {
    pub name: String,
    pub class: String,
    pub size: String,
    pub value: String,
    pub bytes: usize,
}

enum Flow {
    Normal,
    Break,
    Continue,
    Return,
}

struct Frame {
    vars: HashMap<String, Value>,
    globals: HashSet<String>,
}

/// One index argument after evaluation, as zero-based positions.
#[derive(Debug, Clone)]
pub enum IndexArg {
    All,
    List { ix: Vec<usize>, rows: usize, cols: usize, from_mask: bool },
}

enum Resolved {
    Paren(Vec<IndexArg>),
    Brace(Vec<IndexArg>),
    Field(String),
}

pub struct Interpreter {
    base: Frame,
    frames: Vec<Frame>,
    global_vars: HashMap<String, Value>,
    functions: HashMap<String, Rc<Function>>,
    pub format: NumFormat,
    pub(crate) figures: Vec<Figure>,
    pub(crate) current_figure: Option<usize>,
    pub(crate) next_figure_id: u32,
    pub(crate) dirty_figures: Vec<u32>,
    pending: Vec<Output>,
    top_line: usize,
    flush: Option<Box<dyn FnMut(Vec<Output>)>>,
    cancel: Option<Arc<AtomicBool>>,
    pub(crate) tic: Instant,
    pub(crate) rng_state: u64,
    end_stack: Vec<f64>,
    depth: usize,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            base: Frame { vars: HashMap::new(), globals: HashSet::new() },
            frames: Vec::new(),
            global_vars: HashMap::new(),
            functions: HashMap::new(),
            format: NumFormat::Short,
            figures: Vec::new(),
            current_figure: None,
            next_figure_id: 1,
            dirty_figures: Vec::new(),
            pending: Vec::new(),
            top_line: 0,
            flush: None,
            cancel: None,
            tic: Instant::now(),
            rng_state: 0x2545_F491_4F6C_DD1D,
            end_stack: Vec::new(),
            depth: 0,
        }
    }

    /// Receives outputs as they are produced (after each top-level statement and loop pass).
    pub fn set_output_sink(&mut self, f: impl FnMut(Vec<Output>) + 'static) {
        self.flush = Some(Box::new(f));
    }

    pub fn set_cancel_flag(&mut self, flag: Arc<AtomicBool>) {
        self.cancel = Some(flag);
    }

    /// Registers local functions (replacing earlier definitions with the same name).
    pub fn define_functions(&mut self, prog: &Program) {
        for f in &prog.functions {
            self.functions.insert(f.name.clone(), f.clone());
        }
    }

    pub fn clear_functions(&mut self) {
        self.functions.clear();
    }

    pub fn clear_workspace(&mut self) {
        self.base.vars.clear();
    }

    pub fn close_all_figures(&mut self) {
        self.figures.clear();
        self.current_figure = None;
        self.dirty_figures.clear();
    }

    /// Parses and runs a code block in the base workspace. Errors are also emitted as an
    /// [`OutputKind::Error`] output.
    pub fn run_source(&mut self, src: &str) -> R<()> {
        let prog = match crate::parser::parse(src) {
            Ok(p) => p,
            Err(e) => {
                self.pending.push(Output { line: e.line.max(1), kind: OutputKind::Error(e.message.clone()) });
                self.flush_outputs();
                return Err(e);
            }
        };
        self.define_functions(&prog);
        self.run(&prog.body)
    }

    /// Runs top-level statements in the base workspace.
    pub fn run(&mut self, body: &[Stmt]) -> R<()> {
        self.frames.clear();
        for s in body {
            let r = self.exec(s);
            match r {
                Ok(Flow::Return) => {
                    self.flush_outputs();
                    break;
                }
                Ok(_) => self.flush_outputs(),
                Err(mut e) => {
                    self.frames.clear();
                    self.depth = 0;
                    self.end_stack.clear();
                    if e.line == 0 {
                        e.line = self.top_line;
                    }
                    let line = self.top_line.max(1);
                    self.pending.push(Output { line, kind: OutputKind::Error(e.message.clone()) });
                    self.flush_outputs();
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn emit(&mut self, kind: OutputKind) {
        self.pending.push(Output { line: self.top_line, kind });
    }

    pub(crate) fn mark_figure_dirty(&mut self, id: u32) {
        if !self.dirty_figures.contains(&id) {
            self.dirty_figures.push(id);
        }
    }

    fn flush_outputs(&mut self) {
        let dirty = std::mem::take(&mut self.dirty_figures);
        for id in dirty {
            if let Some(f) = self.figures.iter().find(|f| f.id == id) {
                let fig = f.clone();
                self.pending.push(Output { line: self.top_line, kind: OutputKind::Figure(fig) });
            }
        }
        if self.pending.is_empty() {
            return;
        }
        let out = std::mem::take(&mut self.pending);
        if let Some(f) = self.flush.as_mut() {
            f(out);
        } else {
            // No sink: keep them for `take_outputs`.
            self.pending = out;
        }
    }

    /// Outputs collected when no sink is installed.
    pub fn take_outputs(&mut self) -> Vec<Output> {
        std::mem::take(&mut self.pending)
    }

    fn check_cancel(&self) -> R<()> {
        if let Some(c) = &self.cancel {
            if c.load(Ordering::Relaxed) {
                return rt("Execution stopped by user.");
            }
        }
        Ok(())
    }

    // ---- variables ---------------------------------------------------------------------

    fn frame(&self) -> &Frame {
        self.frames.last().unwrap_or(&self.base)
    }

    fn frame_mut(&mut self) -> &mut Frame {
        match self.frames.last_mut() {
            Some(f) => f,
            None => &mut self.base,
        }
    }

    pub fn get_var(&self, name: &str) -> Option<&Value> {
        let f = self.frame();
        if f.globals.contains(name) {
            self.global_vars.get(name)
        } else {
            f.vars.get(name)
        }
    }

    pub fn set_var(&mut self, name: &str, v: Value) {
        if self.frame().globals.contains(name) {
            self.global_vars.insert(name.to_string(), v);
        } else {
            self.frame_mut().vars.insert(name.to_string(), v);
        }
    }

    fn take_var(&mut self, name: &str) -> Option<Value> {
        if self.frame().globals.contains(name) {
            self.global_vars.remove(name)
        } else {
            self.frame_mut().vars.remove(name)
        }
    }

    pub(crate) fn remove_var(&mut self, name: &str) {
        self.take_var(name);
    }

    pub(crate) fn var_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.frame().vars.keys().cloned().collect();
        v.sort();
        v
    }

    pub(crate) fn clear_current_scope(&mut self) {
        self.frame_mut().vars.clear();
    }

    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name) || crate::builtins::exists(name)
    }

    /// Snapshot of the base workspace for the variable browser.
    pub fn workspace(&self) -> Vec<VarInfo> {
        let mut names: Vec<&String> = self.base.vars.keys().collect();
        names.sort_by_key(|n| n.to_lowercase());
        names
            .into_iter()
            .map(|n| {
                let v = &self.base.vars[n];
                let (r, c) = v.dims();
                VarInfo {
                    name: n.clone(),
                    class: v.class_name().to_string(),
                    size: crate::display::size_text(r, c),
                    value: summary(v, self.format),
                    bytes: approx_bytes(v),
                }
            })
            .collect()
    }

    // ---- statements --------------------------------------------------------------------

    fn exec_block(&mut self, body: &[Stmt]) -> R<Flow> {
        for s in body {
            match self.exec(s)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec(&mut self, s: &Stmt) -> R<Flow> {
        if self.frames.is_empty() {
            self.top_line = s.line;
        }
        self.check_cancel()?;
        let r = self.exec_inner(s);
        r.map_err(|mut e| {
            if e.line == 0 && self.frames.is_empty() {
                e.line = s.line;
            }
            e
        })
    }

    fn exec_inner(&mut self, s: &Stmt) -> R<Flow> {
        match &s.kind {
            StmtKind::Expr { expr, print } => {
                if let Expr::Ident(name) = expr {
                    if let Some(v) = self.get_var(name) {
                        if *print {
                            let text = display_value(name, v, self.format);
                            self.emit(OutputKind::Value { name: name.clone(), text });
                        }
                        return Ok(Flow::Normal);
                    }
                }
                let vals = self.eval_multi(expr, 0)?;
                if let Some(v) = vals.into_iter().next() {
                    if *print {
                        let text = display_value("ans", &v, self.format);
                        self.emit(OutputKind::Value { name: "ans".into(), text });
                    }
                    self.set_var("ans", v);
                }
            }
            StmtKind::Assign { lhs, rhs, print } => {
                if lhs.len() == 1 {
                    let v = self.eval(rhs)?;
                    self.assign(&lhs[0], v)?;
                } else {
                    let vals = self.eval_multi(rhs, lhs.len())?;
                    if vals.len() < lhs.len() {
                        // Only an error if a requested output is actually missing.
                        let needed = lhs.iter().rposition(|l| !matches!(l, LValue::Tilde)).map_or(0, |i| i + 1);
                        if vals.len() < needed {
                            return rt("Too many output arguments.");
                        }
                    }
                    for (l, v) in lhs.iter().zip(vals) {
                        self.assign(l, v)?;
                    }
                }
                if *print {
                    for l in lhs {
                        if let LValue::Target { name, .. } = l {
                            if let Some(v) = self.get_var(name) {
                                let text = display_value(name, v, self.format);
                                self.emit(OutputKind::Value { name: name.clone(), text });
                            }
                        }
                    }
                }
            }
            StmtKind::Command { name, args, print } => {
                let argv: Vec<Value> = args.iter().map(|a| Value::chars(a)).collect();
                let vals = self.call_function(name, argv, 0)?;
                if let Some(v) = vals.into_iter().next() {
                    if *print {
                        let text = display_value("ans", &v, self.format);
                        self.emit(OutputKind::Value { name: "ans".into(), text });
                    }
                    self.set_var("ans", v);
                }
            }
            StmtKind::If { clauses, otherwise } => {
                for (cond, body) in clauses {
                    if self.eval(cond)?.is_true()? {
                        return self.exec_block(body);
                    }
                }
                if let Some(body) = otherwise {
                    return self.exec_block(body);
                }
            }
            StmtKind::For { var, iter, body } => return self.exec_for(var, iter, body),
            StmtKind::While { cond, body } => {
                while self.eval(cond)?.is_true()? {
                    self.check_cancel()?;
                    match self.exec_block(body)? {
                        Flow::Break => break,
                        Flow::Return => return Ok(Flow::Return),
                        _ => {}
                    }
                    self.loop_pass_flush();
                }
            }
            StmtKind::Switch { subject, cases, otherwise } => {
                let v = self.eval(subject)?;
                for (case, body) in cases {
                    let cv = self.eval(case)?;
                    let hit = match &cv {
                        Value::Cell(c) => c.data.iter().any(|x| switch_match(&v, x)),
                        other => switch_match(&v, other),
                    };
                    if hit {
                        return self.exec_block(body);
                    }
                }
                if let Some(body) = otherwise {
                    return self.exec_block(body);
                }
            }
            StmtKind::Try { body, catch_var, catch_body } => match self.exec_block(body) {
                Ok(flow) => return Ok(flow),
                Err(e) => {
                    if e.message == "Execution stopped by user." {
                        return Err(e);
                    }
                    if let Some(name) = catch_var {
                        let mut st = Struct::default();
                        st.set("message", Value::chars(&e.message));
                        st.set("identifier", Value::chars(""));
                        self.set_var(name, Value::Struct(st));
                    }
                    return self.exec_block(catch_body);
                }
            },
            StmtKind::Global(names) => {
                for n in names {
                    let local = self.frame_mut().vars.remove(n);
                    self.frame_mut().globals.insert(n.clone());
                    let slot = self.global_vars.entry(n.clone()).or_insert_with(Value::empty);
                    if let Some(v) = local {
                        if matches!(slot, Value::Num(m) if m.is_empty()) {
                            *slot = v;
                        }
                    }
                }
            }
            StmtKind::Break => return Ok(Flow::Break),
            StmtKind::Continue => return Ok(Flow::Continue),
            StmtKind::Return => return Ok(Flow::Return),
        }
        Ok(Flow::Normal)
    }

    /// Streams outputs produced inside long-running top-level loops.
    fn loop_pass_flush(&mut self) {
        if self.frames.is_empty() && !self.pending.is_empty() {
            let out = std::mem::take(&mut self.pending);
            if let Some(f) = self.flush.as_mut() {
                f(out);
            } else {
                self.pending = out;
            }
        }
    }

    fn exec_for(&mut self, var: &str, iter: &Expr, body: &[Stmt]) -> R<Flow> {
        // Numeric ranges iterate without materializing the vector.
        if let Expr::Range(a, step, b) = iter {
            let start = self.eval(a)?.scalar()?;
            let step = match step {
                Some(s) => self.eval(s)?.scalar()?,
                None => 1.0,
            };
            let stop = self.eval(b)?.scalar()?;
            let n = range_count(start, step, stop);
            for i in 0..n {
                self.set_var(var, Value::num(start + i as f64 * step));
                self.check_cancel()?;
                match self.exec_block(body)? {
                    Flow::Break => break,
                    Flow::Return => return Ok(Flow::Return),
                    _ => {}
                }
                self.loop_pass_flush();
            }
            return Ok(Flow::Normal);
        }
        let v = self.eval(iter)?;
        let (rows, cols) = v.dims();
        let items: Vec<Value> = match &v {
            Value::Num(m) => (0..cols)
                .map(|c| {
                    let col: Vec<f64> = (0..rows).map(|r| m.get(r, c)).collect();
                    Value::Num(Matrix::new(rows, 1, col).with_kind(m.kind))
                })
                .collect(),
            Value::Cell(cell) => (0..cols)
                .map(|c| {
                    let col: Vec<Value> = (0..rows).map(|r| cell.data[c * rows + r].clone()).collect();
                    Value::Cell(CellArray::new(rows, 1, col))
                })
                .collect(),
            other => vec![other.clone()],
        };
        for item in items {
            self.set_var(var, item);
            self.check_cancel()?;
            match self.exec_block(body)? {
                Flow::Break => break,
                Flow::Return => return Ok(Flow::Return),
                _ => {}
            }
            self.loop_pass_flush();
        }
        Ok(Flow::Normal)
    }

    // ---- assignment --------------------------------------------------------------------

    fn assign(&mut self, lv: &LValue, rhs: Value) -> R<()> {
        let LValue::Target { name, chain } = lv else {
            return Ok(());
        };
        if chain.is_empty() {
            self.set_var(name, rhs);
            return Ok(());
        }
        // Pass 1: evaluate index arguments against the current sub-value dims (for `end`).
        let mut resolved = Vec::with_capacity(chain.len());
        for (k, acc) in chain.iter().enumerate() {
            let r = match acc {
                Accessor::Field(f) => Resolved::Field(f.clone()),
                Accessor::DynField(e) => Resolved::Field(self.eval(e)?.text()?),
                Accessor::Paren(args) | Accessor::Brace(args) => {
                    let dims = self.dims_at(name, &resolved[..k]);
                    let idx = self.eval_index_args(args, dims)?;
                    if matches!(acc, Accessor::Paren(_)) { Resolved::Paren(idx) } else { Resolved::Brace(idx) }
                }
            };
            resolved.push(r);
        }
        // Pass 2: take the value out, mutate it, and put it back.
        // Hot path: `x(i) = v` on a numeric array mutates in place (validation happens
        // before any write, so an error leaves the variable untouched).
        if let ([Resolved::Paren(idx)], Value::Num(r)) = (resolved.as_slice(), &rhs) {
            if !is_delete_rhs(&rhs) {
                let in_place = !self.frame().globals.contains(name.as_str());
                if in_place {
                    if let Some(Value::Num(m)) = self.frame_mut().vars.get_mut(name.as_str()) {
                        let kind = if m.is_empty() { r.kind } else if m.kind == r.kind { m.kind } else { Kind::Double };
                        assign_generic(&mut m.rows, &mut m.cols, &mut m.data, idx, (r.rows, r.cols, r.data.clone()), 0.0)?;
                        m.kind = kind;
                        return Ok(());
                    }
                }
            }
        }
        // General path: on error the variable is restored from this snapshot.
        let cur = self.take_var(name);
        let snapshot = cur.clone();
        match apply_assign(cur, &resolved, rhs) {
            Ok(v) => {
                self.set_var(name, v);
                Ok(())
            }
            Err(e) => {
                if let Some(v) = snapshot {
                    self.set_var(name, v);
                }
                Err(e)
            }
        }
    }

    /// Dims of the sub-value reached by `path` from variable `name` (0×0 if absent).
    fn dims_at(&self, name: &str, path: &[Resolved]) -> (usize, usize) {
        let mut cur = match self.get_var(name) {
            Some(v) => v,
            None => return (0, 0),
        };
        for p in path {
            match (p, cur) {
                (Resolved::Field(f), Value::Struct(s)) => match s.get(f) {
                    Some(v) => cur = v,
                    None => return (0, 0),
                },
                (Resolved::Brace(idx), Value::Cell(c)) => match linear_positions(c.rows, c.cols, idx) {
                    Ok(pos) if pos.len() == 1 && pos[0] < c.data.len() => cur = &c.data[pos[0]],
                    _ => return (0, 0),
                },
                (Resolved::Paren(_), v @ Value::Struct(_)) => cur = v,
                _ => return (0, 0),
            }
        }
        cur.dims()
    }

    // ---- expressions -------------------------------------------------------------------

    pub fn eval(&mut self, e: &Expr) -> R<Value> {
        match e {
            Expr::Num(n) => Ok(Value::num(*n)),
            Expr::Chars(s) => Ok(Value::chars(s)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Binary(op, a, b) => {
                match op {
                    BinOp::AndAnd => {
                        let l = self.eval(a)?;
                        if !scalar_truth(&l, "&&")? {
                            return Ok(Value::bool(false));
                        }
                        let r = self.eval(b)?;
                        return Ok(Value::bool(scalar_truth(&r, "&&")?));
                    }
                    BinOp::OrOr => {
                        let l = self.eval(a)?;
                        if scalar_truth(&l, "||")? {
                            return Ok(Value::bool(true));
                        }
                        let r = self.eval(b)?;
                        return Ok(Value::bool(scalar_truth(&r, "||")?));
                    }
                    _ => {}
                }
                let l = self.eval(a)?;
                let r = self.eval(b)?;
                ops::binary(*op, &l, &r)
            }
            Expr::Unary(op, x) => {
                let v = self.eval(x)?;
                ops::unary(*op, &v)
            }
            Expr::Range(a, step, b) => {
                let start = self.eval(a)?.scalar()?;
                let step = match step {
                    Some(s) => self.eval(s)?.scalar()?,
                    None => 1.0,
                };
                let stop = self.eval(b)?.scalar()?;
                let n = range_count(start, step, stop);
                Ok(Value::Num(Matrix::new(if n == 0 { 1 } else { 1 }, n, (0..n).map(|i| start + i as f64 * step).collect())))
            }
            Expr::End => match self.end_stack.last() {
                Some(v) => Ok(Value::num(*v)),
                None => rt("'end' is not valid here."),
            },
            Expr::Colon => Ok(Value::chars(":")),
            Expr::Tilde => rt("'~' is only valid on the left side of an assignment."),
            Expr::Matrix(rows) => self.eval_matrix(rows),
            Expr::Cell(rows) => self.eval_cell(rows),
            Expr::AnonFn(f) => {
                // Capture referenced variables by value at creation time.
                let mut names = Vec::new();
                collect_idents(&f.body, &mut names);
                let mut captured = Vec::new();
                for n in names {
                    if f.params.contains(&n) {
                        continue;
                    }
                    if let Some(v) = self.get_var(&n) {
                        if !captured.iter().any(|(m, _): &(String, Value)| *m == n) {
                            captured.push((n, v.clone()));
                        }
                    }
                }
                Ok(Value::Func(FuncValue::Anon { f: f.clone(), captured: Rc::new(captured) }))
            }
            Expr::FuncHandle(n) => Ok(Value::Func(FuncValue::Named(n.clone()))),
            _ => {
                let mut v = self.eval_multi(e, 1)?;
                if v.is_empty() {
                    return rt("Value expected, but the expression produced no output (it may be a function that returns nothing).");
                }
                Ok(v.swap_remove(0))
            }
        }
    }

    /// Evaluates an expression that may yield several values (function outputs, `c{:}`).
    pub fn eval_multi(&mut self, e: &Expr, nargout: usize) -> R<Vec<Value>> {
        match e {
            Expr::Ident(name) => {
                if let Some(v) = self.get_var(name) {
                    return Ok(vec![v.clone()]);
                }
                self.call_function(name, Vec::new(), nargout)
            }
            Expr::Index(base, args) => {
                if let Expr::Ident(name) = base.as_ref() {
                    if self.get_var(name).is_none() {
                        let argv = self.eval_args(args)?;
                        return self.call_function(name, argv, nargout);
                    }
                }
                let bv = self.eval(base)?;
                if let Value::Func(f) = &bv {
                    let argv = self.eval_args(args)?;
                    return self.call_value(f, argv, nargout);
                }
                let idx = self.eval_index_args(args, bv.dims())?;
                Ok(vec![index_value(&bv, &idx)?])
            }
            Expr::CellIndex(base, args) => {
                let bv = self.eval(base)?;
                let Value::Cell(c) = &bv else {
                    return rt(format!("Brace indexing is not supported for variables of this type ({}).", bv.class_name()));
                };
                let idx = self.eval_index_args(args, (c.rows, c.cols))?;
                let pos = linear_positions(c.rows, c.cols, &idx)?;
                let mut out = Vec::with_capacity(pos.len());
                for p in pos {
                    match c.data.get(p) {
                        Some(v) => out.push(v.clone()),
                        None => return rt(format!("Index exceeds the number of array elements. Index must not exceed {}.", c.data.len())),
                    }
                }
                Ok(out)
            }
            Expr::Field(base, f) => {
                let bv = self.eval(base)?;
                Ok(vec![get_field(&bv, f)?])
            }
            Expr::DynField(base, f) => {
                let bv = self.eval(base)?;
                let name = self.eval(f)?.text()?;
                Ok(vec![get_field(&bv, &name)?])
            }
            other => Ok(vec![self.eval(other)?]),
        }
    }

    /// Function-call arguments with comma-list expansion of `c{:}`.
    fn eval_args(&mut self, args: &[Expr]) -> R<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Expr::CellIndex(..) => out.extend(self.eval_multi(a, 1)?),
                Expr::Colon => out.push(Value::chars(":")),
                _ => out.push(self.eval(a)?),
            }
        }
        Ok(out)
    }

    fn eval_index_args(&mut self, args: &[Expr], dims: (usize, usize)) -> R<Vec<IndexArg>> {
        let n = args.len();
        let mut out = Vec::with_capacity(n);
        for (k, a) in args.iter().enumerate() {
            if matches!(a, Expr::Colon) {
                out.push(IndexArg::All);
                continue;
            }
            let end = match (n, k) {
                (1, _) => (dims.0 * dims.1) as f64,
                (_, 0) => dims.0 as f64,
                (_, 1) => dims.1 as f64,
                _ => 1.0,
            };
            self.end_stack.push(end);
            let v = self.eval(a);
            self.end_stack.pop();
            out.push(to_index_arg(&v?)?);
        }
        Ok(out)
    }

    fn eval_matrix(&mut self, rows: &[Vec<Expr>]) -> R<Value> {
        let mut row_vals = Vec::with_capacity(rows.len());
        for row in rows {
            let mut items = Vec::with_capacity(row.len());
            for e in row {
                match e {
                    Expr::CellIndex(..) => items.extend(self.eval_multi(e, 1)?),
                    _ => items.push(self.eval(e)?),
                }
            }
            row_vals.push(hconcat(items)?);
        }
        vconcat(row_vals)
    }

    fn eval_cell(&mut self, rows: &[Vec<Expr>]) -> R<Value> {
        let mut grid: Vec<Vec<Value>> = Vec::new();
        for row in rows {
            let mut items = Vec::new();
            for e in row {
                match e {
                    Expr::CellIndex(..) => items.extend(self.eval_multi(e, 1)?),
                    _ => items.push(self.eval(e)?),
                }
            }
            grid.push(items);
        }
        let nrows = grid.len();
        if nrows == 0 {
            return Ok(Value::Cell(CellArray::new(0, 0, Vec::new())));
        }
        let ncols = grid[0].len();
        if grid.iter().any(|r| r.len() != ncols) {
            return rt("Dimensions of arrays being concatenated are not consistent.");
        }
        let mut data = Vec::with_capacity(nrows * ncols);
        for c in 0..ncols {
            for r in 0..nrows {
                data.push(grid[r][c].clone());
            }
        }
        Ok(Value::Cell(CellArray::new(nrows, ncols, data)))
    }

    // ---- calls -------------------------------------------------------------------------

    pub fn call_function(&mut self, name: &str, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        if let Some(f) = self.functions.get(name).cloned() {
            return self.call_user(&f, args, nargout);
        }
        if let Some(r) = crate::builtins::call(self, name, args, nargout)? {
            return Ok(r);
        }
        rt(format!("Unrecognized function or variable '{name}'."))
    }

    pub fn call_value(&mut self, f: &FuncValue, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        match f {
            FuncValue::Named(n) => self.call_function(n, args, nargout),
            FuncValue::Anon { f, captured } => {
                if args.len() > f.params.len() {
                    return rt("Too many input arguments.");
                }
                self.enter()?;
                let mut vars: HashMap<String, Value> = captured.iter().cloned().collect();
                for (p, a) in f.params.iter().zip(args) {
                    vars.insert(p.clone(), a);
                }
                self.frames.push(Frame { vars, globals: HashSet::new() });
                let r = self.eval_multi(&f.body, nargout.max(1));
                self.frames.pop();
                self.depth -= 1;
                r
            }
        }
    }

    fn enter(&mut self) -> R<()> {
        self.depth += 1;
        if self.depth > 256 {
            self.depth -= 1;
            return rt("Maximum recursion limit of 256 reached.");
        }
        Ok(())
    }

    fn call_user(&mut self, f: &Rc<Function>, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        let has_varargin = f.params.last().map(|p| p == "varargin").unwrap_or(false);
        let fixed = if has_varargin { f.params.len() - 1 } else { f.params.len() };
        if args.len() > fixed && !has_varargin {
            return rt(format!("Too many input arguments for function '{}'.", f.name));
        }
        if nargout > f.outputs.len() && !f.outputs.last().map(|o| o == "varargout").unwrap_or(false) {
            return rt(format!("Too many output arguments for function '{}'.", f.name));
        }
        self.enter()?;
        let nargin = args.len();
        let mut vars = HashMap::new();
        let mut it = args.into_iter();
        for p in f.params.iter().take(fixed) {
            match it.next() {
                Some(a) => {
                    vars.insert(p.clone(), a);
                }
                None => break,
            }
        }
        if has_varargin {
            let rest: Vec<Value> = it.collect();
            vars.insert("varargin".into(), Value::Cell(CellArray::row(rest)));
        }
        vars.insert("nargin".into(), Value::num(nargin as f64));
        vars.insert("nargout".into(), Value::num(nargout as f64));
        self.frames.push(Frame { vars, globals: HashSet::new() });
        let r = self.exec_block(&f.body);
        let frame = self.frames.pop().unwrap();
        self.depth -= 1;
        r?;
        let mut out = Vec::new();
        let want = nargout.max(1);
        for (i, o) in f.outputs.iter().enumerate() {
            if out.len() >= want {
                break;
            }
            if o == "varargout" {
                if let Some(Value::Cell(c)) = frame.vars.get(o) {
                    out.extend(c.data.iter().cloned());
                }
                break;
            }
            match frame.vars.get(o) {
                Some(v) => out.push(v.clone()),
                None => {
                    if i == 0 && nargout == 0 {
                        break;
                    }
                    return rt(format!("Output argument \"{o}\" (and possibly others) not assigned a value in the execution with \"{}\" function.", f.name));
                }
            }
        }
        out.truncate(want);
        Ok(out)
    }
}

fn scalar_truth(v: &Value, op: &str) -> R<bool> {
    match v {
        Value::Num(m) if m.numel() == 1 => Ok(m.data[0] != 0.0),
        _ => rt(format!("Operands to the {op} operator must be convertible to logical scalar values.")),
    }
}

fn switch_match(subject: &Value, case: &Value) -> bool {
    if subject.is_text() && case.is_text() {
        return subject.text().ok() == case.text().ok();
    }
    match (subject, case) {
        (Value::Num(a), Value::Num(b)) if a.is_scalar() && b.is_scalar() => a.data[0] == b.data[0],
        _ => false,
    }
}

pub(crate) fn range_count(start: f64, step: f64, stop: f64) -> usize {
    if step == 0.0 || !start.is_finite() || !stop.is_finite() || (step > 0.0 && start > stop) || (step < 0.0 && start < stop) {
        return 0;
    }
    let n = ((stop - start) / step + 1e-10).floor() + 1.0;
    if n < 0.0 { 0 } else { n as usize }
}

fn collect_idents(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Ident(n) => out.push(n.clone()),
        Expr::Unary(_, x) => collect_idents(x, out),
        Expr::Binary(_, a, b) => {
            collect_idents(a, out);
            collect_idents(b, out);
        }
        Expr::Range(a, s, b) => {
            collect_idents(a, out);
            if let Some(s) = s {
                collect_idents(s, out);
            }
            collect_idents(b, out);
        }
        Expr::Index(b, args) | Expr::CellIndex(b, args) => {
            collect_idents(b, out);
            args.iter().for_each(|a| collect_idents(a, out));
        }
        Expr::Field(b, _) => collect_idents(b, out),
        Expr::DynField(b, f) => {
            collect_idents(b, out);
            collect_idents(f, out);
        }
        Expr::Matrix(rows) | Expr::Cell(rows) => rows.iter().flatten().for_each(|x| collect_idents(x, out)),
        Expr::AnonFn(f) => collect_idents(&f.body, out),
        _ => {}
    }
}

fn approx_bytes(v: &Value) -> usize {
    match v {
        Value::Num(m) => m.numel() * if m.kind == Kind::Double { 8 } else if m.kind == Kind::Char { 2 } else { 1 },
        Value::Str(s) => s.len() * 2 + 16,
        Value::Cell(c) => c.data.iter().map(approx_bytes).sum::<usize>() + c.data.len() * 8,
        Value::Struct(s) => s.fields.iter().map(|(_, v)| approx_bytes(v) + 64).sum(),
        Value::Func(_) => 32,
    }
}

fn get_field(v: &Value, f: &str) -> R<Value> {
    match v {
        Value::Struct(s) => match s.get(f) {
            Some(x) => Ok(x.clone()),
            None => rt(format!("Unrecognized field name \"{f}\".")),
        },
        _ => rt(format!("Dot indexing is not supported for variables of this type ({}).", v.class_name())),
    }
}

// ---- indexing ------------------------------------------------------------------------------

fn to_index_arg(v: &Value) -> R<IndexArg> {
    match v {
        Value::Num(m) if m.kind == Kind::Char && m.to_string_lossy() == ":" => Ok(IndexArg::All),
        Value::Num(m) if m.kind == Kind::Logical => {
            let ix: Vec<usize> = m.data.iter().enumerate().filter(|(_, &x)| x != 0.0).map(|(i, _)| i).collect();
            let n = ix.len();
            let (rows, cols) = if m.rows == 1 { (1, n) } else { (n, 1) };
            Ok(IndexArg::List { ix, rows, cols, from_mask: true })
        }
        Value::Num(m) => {
            let mut ix = Vec::with_capacity(m.numel());
            for &x in &m.data {
                if x < 1.0 || x.fract() != 0.0 || !x.is_finite() {
                    if x.fract() != 0.0 {
                        return rt("Array indices must be positive integers or logical values.");
                    }
                    return rt(format!("Index in position {} is invalid. Array indices must be positive integers or logical values.", x));
                }
                ix.push(x as usize - 1);
            }
            Ok(IndexArg::List { ix, rows: m.rows, cols: m.cols, from_mask: false })
        }
        _ => rt(format!("Indexing with a value of class '{}' is not supported.", v.class_name())),
    }
}

/// Linear positions selected by `idx` (no bounds check against growth).
pub(crate) fn linear_positions(rows: usize, cols: usize, idx: &[IndexArg]) -> R<Vec<usize>> {
    match idx.len() {
        0 => Ok((0..rows * cols).collect()),
        1 => Ok(match &idx[0] {
            IndexArg::All => (0..rows * cols).collect(),
            IndexArg::List { ix, .. } => ix.clone(),
        }),
        _ => {
            let rs = arg_list(&idx[0], rows);
            let cs = arg_list(&idx[1], cols);
            for extra in &idx[2..] {
                if let IndexArg::List { ix, .. } = extra {
                    if ix.iter().any(|&i| i != 0) {
                        return rt("Only 2-D arrays are supported.");
                    }
                }
            }
            let mut out = Vec::with_capacity(rs.len() * cs.len());
            for &c in &cs {
                for &r in &rs {
                    out.push(c * rows + r);
                }
            }
            Ok(out)
        }
    }
}

fn arg_list(a: &IndexArg, n: usize) -> Vec<usize> {
    match a {
        IndexArg::All => (0..n).collect(),
        IndexArg::List { ix, .. } => ix.clone(),
    }
}

/// Generic `A(idx)` for any element type.
pub(crate) fn index_generic<T: Clone>(rows: usize, cols: usize, data: &[T], idx: &[IndexArg]) -> R<(usize, usize, Vec<T>)> {
    let n = data.len();
    match idx.len() {
        0 => Ok((rows, cols, data.to_vec())),
        1 => match &idx[0] {
            IndexArg::All => Ok((n, if n == 0 { 0 } else { 1 }, data.to_vec())),
            IndexArg::List { ix, rows: ir, cols: ic, from_mask } => {
                let mut out = Vec::with_capacity(ix.len());
                for &i in ix {
                    if i >= n {
                        return rt(format!("Index exceeds the number of array elements. Index must not exceed {n}."));
                    }
                    out.push(data[i].clone());
                }
                let k = out.len();
                let source_vector = rows == 1 || cols == 1;
                let index_vector = *ir == 1 || *ic == 1;
                let (r, c) = if (source_vector && index_vector) || *from_mask {
                    if rows == 1 && !(cols == 1 && *from_mask && *ic == 1) { (1, k) } else { (k, 1) }
                } else {
                    (*ir, *ic)
                };
                Ok((r, c, out))
            }
        },
        _ => {
            let rs = arg_list(&idx[0], rows);
            let cs = arg_list(&idx[1], cols);
            if rs.iter().any(|&r| r >= rows) {
                return rt(format!("Index in position 1 exceeds array bounds. Index must not exceed {rows}."));
            }
            if cs.iter().any(|&c| c >= cols) {
                return rt(format!("Index in position 2 exceeds array bounds. Index must not exceed {cols}."));
            }
            let mut out = Vec::with_capacity(rs.len() * cs.len());
            for &c in &cs {
                for &r in &rs {
                    out.push(data[c * rows + r].clone());
                }
            }
            Ok((rs.len(), cs.len(), out))
        }
    }
}

pub(crate) fn index_value(v: &Value, idx: &[IndexArg]) -> R<Value> {
    match v {
        Value::Num(m) => {
            let (r, c, d) = index_generic(m.rows, m.cols, &m.data, idx)?;
            Ok(Value::Num(Matrix::new(r, c, d).with_kind(m.kind)))
        }
        Value::Cell(cell) => {
            let (r, c, d) = index_generic(cell.rows, cell.cols, &cell.data, idx)?;
            Ok(Value::Cell(CellArray::new(r, c, d)))
        }
        other => {
            let pos = linear_positions(1, 1, idx)?;
            if pos.iter().all(|&p| p == 0) && pos.len() == 1 {
                Ok(other.clone())
            } else {
                rt("Index exceeds the number of array elements. Index must not exceed 1.")
            }
        }
    }
}

/// `A(idx) = rhs` for any element type, growing as needed. `rhs` is (rows, cols, data).
fn assign_generic<T: Clone>(
    rows: &mut usize,
    cols: &mut usize,
    data: &mut Vec<T>,
    idx: &[IndexArg],
    rhs: (usize, usize, Vec<T>),
    fill: T,
) -> R<()> {
    let (rr, rc, rd) = rhs;
    let rn = rd.len();
    match idx.len() {
        1 => {
            let ix: Vec<usize> = match &idx[0] {
                IndexArg::All => (0..*rows * *cols).collect(),
                IndexArg::List { ix, .. } => ix.clone(),
            };
            if rn != 1 && rn != ix.len() {
                return rt(format!("Unable to perform assignment because the left and right sides have a different number of elements ({} and {rn}).", ix.len()));
            }
            let max = ix.iter().copied().max().map(|m| m + 1).unwrap_or(0);
            if max > data.len() {
                if *rows == 0 && *cols == 0 {
                    // New vector: a row unless the rhs is a column being assigned with ':'.
                    if rr > 1 && rc == 1 { *rows = max; *cols = 1; } else { *rows = 1; *cols = max; }
                    data.resize(max, fill);
                } else if *rows == 1 {
                    *cols = max;
                    data.resize(max, fill);
                } else if *cols == 1 {
                    *rows = max;
                    data.resize(max, fill);
                } else {
                    return rt("Attempt to grow array along ambiguous dimension.");
                }
            }
            for (k, &i) in ix.iter().enumerate() {
                data[i] = if rn == 1 { rd[0].clone() } else { rd[k].clone() };
            }
            Ok(())
        }
        2 => {
            let rs: Vec<usize> = match &idx[0] {
                IndexArg::All => (0..if *rows == 0 { rr } else { *rows }).collect(),
                IndexArg::List { ix, .. } => ix.clone(),
            };
            let cs: Vec<usize> = match &idx[1] {
                IndexArg::All => (0..if *cols == 0 { rc } else { *cols }).collect(),
                IndexArg::List { ix, .. } => ix.clone(),
            };
            let count = rs.len() * cs.len();
            if rn != 1 && rn != count {
                return rt(format!(
                    "Unable to perform assignment because the size of the left side is {}-by-{} and the size of the right side is {rr}-by-{rc}.",
                    rs.len(),
                    cs.len()
                ));
            }
            let need_r = rs.iter().copied().max().map(|m| m + 1).unwrap_or(0).max(*rows);
            let need_c = cs.iter().copied().max().map(|m| m + 1).unwrap_or(0).max(*cols);
            if need_r != *rows || need_c != *cols {
                let mut nd = vec![fill.clone(); need_r * need_c];
                for c in 0..*cols {
                    for r in 0..*rows {
                        nd[c * need_r + r] = data[c * *rows + r].clone();
                    }
                }
                *data = nd;
                *rows = need_r;
                *cols = need_c;
            }
            let mut k = 0;
            for &c in &cs {
                for &r in &rs {
                    data[c * *rows + r] = if rn == 1 { rd[0].clone() } else { rd[k].clone() };
                    k += 1;
                }
            }
            Ok(())
        }
        _ => rt("Only 1-D and 2-D indexing is supported."),
    }
}

fn delete_generic<T: Clone>(rows: &mut usize, cols: &mut usize, data: &mut Vec<T>, idx: &[IndexArg]) -> R<()> {
    match idx.len() {
        1 => {
            let mut kill = vec![false; data.len()];
            match &idx[0] {
                IndexArg::All => kill.iter_mut().for_each(|k| *k = true),
                IndexArg::List { ix, .. } => {
                    for &i in ix {
                        if i >= data.len() {
                            return rt("Index exceeds the number of array elements.");
                        }
                        kill[i] = true;
                    }
                }
            }
            let kept: Vec<T> = data.iter().zip(&kill).filter(|(_, &k)| !k).map(|(v, _)| v.clone()).collect();
            let n = kept.len();
            if *cols == 1 && *rows != 1 {
                *rows = n;
            } else {
                *rows = if n == 0 && *rows == 0 { 0 } else { 1 };
                *cols = n;
            }
            *data = kept;
            Ok(())
        }
        2 => {
            let all_r = matches!(idx[0], IndexArg::All) || matches!(&idx[0], IndexArg::List { ix, .. } if { let mut v = ix.clone(); v.sort(); v.dedup(); v.len() == *rows && *rows > 0 && v.iter().enumerate().all(|(i, &x)| i == x) });
            let all_c = matches!(idx[1], IndexArg::All);
            if all_c {
                let kill: HashSet<usize> = arg_list(&idx[0], *rows).into_iter().collect();
                let keep_rows: Vec<usize> = (0..*rows).filter(|r| !kill.contains(r)).collect();
                let mut nd = Vec::with_capacity(keep_rows.len() * *cols);
                for c in 0..*cols {
                    for &r in &keep_rows {
                        nd.push(data[c * *rows + r].clone());
                    }
                }
                *rows = keep_rows.len();
                *data = nd;
                Ok(())
            } else if all_r {
                let kill: HashSet<usize> = arg_list(&idx[1], *cols).into_iter().collect();
                let keep_cols: Vec<usize> = (0..*cols).filter(|c| !kill.contains(c)).collect();
                let mut nd = Vec::with_capacity(keep_cols.len() * *rows);
                for &c in &keep_cols {
                    for r in 0..*rows {
                        nd.push(data[c * *rows + r].clone());
                    }
                }
                *cols = keep_cols.len();
                *data = nd;
                Ok(())
            } else {
                rt("A null assignment can have only one non-colon index.")
            }
        }
        _ => rt("Only 1-D and 2-D indexing is supported."),
    }
}

fn is_delete_rhs(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.rows == 0 && m.cols == 0 && m.kind != Kind::Char)
}

fn apply_assign(cur: Option<Value>, chain: &[Resolved], rhs: Value) -> R<Value> {
    let Some((first, rest)) = chain.split_first() else {
        return Ok(rhs);
    };
    match first {
        Resolved::Field(f) => {
            let mut s = match cur {
                None => Struct::default(),
                Some(Value::Num(m)) if m.is_empty() => Struct::default(),
                Some(Value::Struct(s)) => s,
                Some(other) => {
                    return rt(format!("Unable to perform assignment because dot indexing is not supported for variables of this type ({}).", other.class_name()))
                }
            };
            let sub = s.fields.iter().position(|(n, _)| n == f).map(|i| s.fields[i].1.clone());
            let new = apply_assign(sub, rest, rhs)?;
            s.set(f, new);
            Ok(Value::Struct(s))
        }
        Resolved::Brace(idx) => {
            let mut c = match cur {
                None => CellArray::new(0, 0, Vec::new()),
                Some(Value::Num(m)) if m.is_empty() => CellArray::new(0, 0, Vec::new()),
                Some(Value::Cell(c)) => c,
                Some(other) => return rt(format!("Brace indexing is not supported for variables of this type ({}).", other.class_name())),
            };
            let new = if rest.is_empty() {
                rhs
            } else {
                let pos = linear_positions(c.rows, c.cols, idx)?;
                let sub = pos.first().and_then(|&p| c.data.get(p).cloned());
                apply_assign(sub, rest, rhs)?
            };
            assign_generic(&mut c.rows, &mut c.cols, &mut c.data, idx, (1, 1, vec![new]), Value::empty())?;
            Ok(Value::Cell(c))
        }
        Resolved::Paren(idx) => {
            if !rest.is_empty() {
                // s(1).f = v on a scalar struct.
                return match cur {
                    Some(v @ Value::Struct(_)) | Some(v @ Value::Cell(_)) if linear_positions(1, 1, idx)?.iter().all(|&p| p == 0) => {
                        apply_assign(Some(v), rest, rhs)
                    }
                    None => apply_assign(None, rest, rhs),
                    _ => rt("Struct arrays are not supported in this version."),
                };
            }
            let cur = cur.unwrap_or_else(|| match &rhs {
                Value::Cell(_) => Value::Cell(CellArray::new(0, 0, Vec::new())),
                _ => Value::empty(),
            });
            match (cur, rhs) {
                (Value::Num(mut m), rhs) if is_delete_rhs(&rhs) => {
                    delete_generic(&mut m.rows, &mut m.cols, &mut m.data, idx)?;
                    Ok(Value::Num(m))
                }
                (Value::Cell(mut c), rhs) if is_delete_rhs(&rhs) => {
                    delete_generic(&mut c.rows, &mut c.cols, &mut c.data, idx)?;
                    Ok(Value::Cell(c))
                }
                (Value::Num(mut m), Value::Num(r)) => {
                    let kind = if m.is_empty() { r.kind } else if m.kind == r.kind { m.kind } else if m.kind == Kind::Char && r.kind != Kind::Char { Kind::Double } else if m.kind == Kind::Logical { Kind::Double } else { m.kind };
                    assign_generic(&mut m.rows, &mut m.cols, &mut m.data, idx, (r.rows, r.cols, r.data), 0.0)?;
                    m.kind = kind;
                    Ok(Value::Num(m))
                }
                (Value::Num(m), Value::Cell(r)) if m.is_empty() => {
                    let mut c = CellArray::new(0, 0, Vec::new());
                    assign_generic(&mut c.rows, &mut c.cols, &mut c.data, idx, (r.rows, r.cols, r.data), Value::empty())?;
                    Ok(Value::Cell(c))
                }
                (Value::Cell(mut c), Value::Cell(r)) => {
                    assign_generic(&mut c.rows, &mut c.cols, &mut c.data, idx, (r.rows, r.cols, r.data), Value::empty())?;
                    Ok(Value::Cell(c))
                }
                (Value::Cell(_), other) => rt(format!("Conversion to cell from {} is not possible.", other.class_name())),
                (Value::Num(m), other @ (Value::Struct(_) | Value::Str(_) | Value::Func(_))) if m.is_empty() => {
                    if linear_positions(1, 1, idx)?.iter().all(|&p| p == 0) {
                        Ok(other)
                    } else {
                        rt("Arrays of this type are not supported in this version.")
                    }
                }
                (cur @ (Value::Struct(_) | Value::Str(_) | Value::Func(_)), other) => {
                    if linear_positions(1, 1, idx)?.iter().all(|&p| p == 0) {
                        Ok(other)
                    } else {
                        let _ = cur;
                        rt("Arrays of this type are not supported in this version.")
                    }
                }
                (_, other) => rt(format!("Unable to perform assignment with a value of class '{}'.", other.class_name())),
            }
        }
    }
}

// ---- concatenation -------------------------------------------------------------------------

pub(crate) fn hconcat(items: Vec<Value>) -> R<Value> {
    concat(items, true)
}

pub(crate) fn vconcat(items: Vec<Value>) -> R<Value> {
    concat(items, false)
}

fn concat(items: Vec<Value>, horizontal: bool) -> R<Value> {
    if items.len() == 1 {
        return Ok(items.into_iter().next().unwrap());
    }
    if items.is_empty() {
        return Ok(Value::empty());
    }
    if items.iter().any(|v| matches!(v, Value::Cell(_))) {
        let cells: Vec<CellArray> = items
            .into_iter()
            .filter(|v| !matches!(v, Value::Num(m) if m.is_empty()))
            .map(|v| match v {
                Value::Cell(c) => c,
                other => CellArray::new(1, 1, vec![other]),
            })
            .collect();
        let (r, c, d) = concat_generic(cells.into_iter().map(|c| (c.rows, c.cols, c.data)).collect(), horizontal)?;
        return Ok(Value::Cell(CellArray::new(r, c, d)));
    }
    if items.iter().any(|v| matches!(v, Value::Str(_))) {
        return rt("String arrays are not supported in this version; use char arrays ('...') or strcat.");
    }
    if items.iter().any(|v| !matches!(v, Value::Num(_))) {
        return rt("Concatenation of structs or function handles is not supported.");
    }
    let mats: Vec<Matrix> = items.into_iter().map(|v| if let Value::Num(m) = v { m } else { unreachable!() }).collect();
    let any_char = mats.iter().any(|m| m.kind == Kind::Char);
    let all_logical = mats.iter().filter(|m| !m.is_empty()).all(|m| m.kind == Kind::Logical);
    let kind = if any_char { Kind::Char } else if all_logical && mats.iter().any(|m| !m.is_empty()) { Kind::Logical } else { Kind::Double };
    let parts: Vec<(usize, usize, Vec<f64>)> = mats.into_iter().filter(|m| !(m.rows == 0 && m.cols == 0) && !(m.is_empty() && kind == Kind::Char)).map(|m| (m.rows, m.cols, m.data)).collect();
    if parts.is_empty() {
        return Ok(Value::Num(Matrix::empty().with_kind(if any_char { Kind::Char } else { Kind::Double })));
    }
    let (r, c, d) = concat_generic(parts, horizontal)?;
    Ok(Value::Num(Matrix::new(r, c, d).with_kind(kind)))
}

fn concat_generic<T: Clone>(parts: Vec<(usize, usize, Vec<T>)>, horizontal: bool) -> R<(usize, usize, Vec<T>)> {
    if parts.is_empty() {
        return Ok((0, 0, Vec::new()));
    }
    if horizontal {
        let rows = parts[0].0;
        if parts.iter().any(|p| p.0 != rows) {
            return rt("Dimensions of arrays being concatenated are not consistent.");
        }
        let cols = parts.iter().map(|p| p.1).sum();
        let mut data = Vec::with_capacity(rows * cols);
        for p in parts {
            data.extend(p.2);
        }
        Ok((rows, cols, data))
    } else {
        let cols = parts[0].1;
        if parts.iter().any(|p| p.1 != cols) {
            return rt("Dimensions of arrays being concatenated are not consistent.");
        }
        let rows: usize = parts.iter().map(|p| p.0).sum();
        let mut data = Vec::with_capacity(rows * cols);
        for c in 0..cols {
            for p in &parts {
                data.extend_from_slice(&p.2[c * p.0..(c + 1) * p.0]);
            }
        }
        Ok((rows, cols, data))
    }
}
