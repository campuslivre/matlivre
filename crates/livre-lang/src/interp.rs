//! Tree-walking interpreter with a MATLAB-style base workspace, local and path functions,
//! classdef classes, and streamed outputs (text, values, figures, errors) anchored to lines.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::ast::*;
use crate::class::{ClassDef, Object};
use crate::display::{display_value, summary, NumFormat};
use crate::error::{rt, LangError};
use crate::graphics::Figure;
use crate::indexing::{self, to_index_arg};
use crate::nd::{self, Dims, IndexArg};
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

pub(crate) enum Flow {
    Normal,
    Break,
    Continue,
    Return,
}

struct Frame {
    vars: HashMap<String, Value>,
    globals: HashSet<String>,
}

enum Resolved {
    Paren(Vec<IndexArg>),
    Brace(Vec<IndexArg>),
    Field(String),
    /// Table subscripts (row/variable names allowed).
    TableParen(Vec<Value>),
    TableBrace(Vec<Value>),
}

pub struct Interpreter {
    base: Frame,
    frames: Vec<Frame>,
    global_vars: HashMap<String, Value>,
    functions: HashMap<String, Rc<Function>>,
    search_path: Vec<PathBuf>,
    path_functions: HashMap<String, Rc<Function>>,
    path_scripts: HashMap<String, Rc<Program>>,
    classes: HashMap<String, Rc<ClassDef>>,
    not_on_path: HashSet<String>,
    /// Class whose method is executing (for `obj@Super` and `method@Super`).
    class_stack: Vec<Rc<ClassDef>>,
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
    /// Source names of the arguments of the builtin being called (`table` uses them).
    pub(crate) arg_names: Vec<Option<String>>,
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
            search_path: Vec::new(),
            path_functions: HashMap::new(),
            path_scripts: HashMap::new(),
            classes: HashMap::new(),
            not_on_path: HashSet::new(),
            class_stack: Vec::new(),
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
            arg_names: Vec::new(),
        }
    }

    /// Receives outputs as they are produced (after each top-level statement and loop pass).
    pub fn set_output_sink(&mut self, f: impl FnMut(Vec<Output>) + 'static) {
        self.flush = Some(Box::new(f));
    }

    pub fn set_cancel_flag(&mut self, flag: Arc<AtomicBool>) {
        self.cancel = Some(flag);
    }

    /// Folders searched for function and classdef files (`name.m`). Clears cached lookups so
    /// edited files are picked up.
    pub fn set_search_path(&mut self, dirs: Vec<PathBuf>) {
        self.search_path = dirs;
        self.reset_path_cache();
    }

    pub fn add_path(&mut self, dir: PathBuf) {
        if !self.search_path.contains(&dir) {
            self.search_path.insert(0, dir);
        }
        self.not_on_path.clear();
    }

    pub fn reset_path_cache(&mut self) {
        self.path_functions.clear();
        self.path_scripts.clear();
        self.classes.clear();
        self.not_on_path.clear();
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
        self.class_stack.clear();
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
                    self.class_stack.clear();
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

    pub fn has_function(&mut self, name: &str) -> bool {
        self.functions.contains_key(name) || crate::builtins::exists(name) || self.find_class(name).is_some() || self.find_path_function(name).is_some()
    }

    /// Snapshot of the base workspace for the variable browser.
    pub fn workspace(&self) -> Vec<VarInfo> {
        let mut names: Vec<&String> = self.base.vars.keys().collect();
        names.sort_by_key(|n| n.to_lowercase());
        names
            .into_iter()
            .map(|n| {
                let v = &self.base.vars[n];
                VarInfo { name: n.clone(), class: v.class_name(), size: nd::size_text(&v.dims()), value: summary(v, self.format), bytes: approx_bytes(v) }
            })
            .collect()
    }

    // ---- search path -------------------------------------------------------------------

    fn read_path_file(&self, name: &str) -> Option<(PathBuf, String)> {
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }
        for dir in &self.search_path {
            let p = dir.join(format!("{name}.m"));
            if let Ok(src) = std::fs::read_to_string(&p) {
                return Some((p, src));
            }
            // Class folders: @Name/Name.m
            let p = dir.join(format!("@{name}")).join(format!("{name}.m"));
            if let Ok(src) = std::fs::read_to_string(&p) {
                return Some((p, src));
            }
        }
        None
    }

    /// Loads (once) a function or script file from the search path.
    fn find_path_function(&mut self, name: &str) -> Option<Rc<Function>> {
        if let Some(f) = self.path_functions.get(name) {
            return Some(f.clone());
        }
        if self.not_on_path.contains(name) || self.classes.contains_key(name) {
            return None;
        }
        let Some((_, src)) = self.read_path_file(name) else {
            self.not_on_path.insert(name.to_string());
            return None;
        };
        if crate::class::is_classdef(&src) {
            return None;
        }
        let prog = crate::parser::parse(&src).ok()?;
        if prog.functions.is_empty() {
            self.path_scripts.insert(name.to_string(), Rc::new(prog));
            return None;
        }
        let main = prog.functions[0].clone();
        // Subfunctions are visible to the file's main function (registered globally here).
        for f in &prog.functions[1..] {
            self.path_functions.entry(f.name.clone()).or_insert_with(|| f.clone());
        }
        self.path_functions.insert(name.to_string(), main.clone());
        Some(main)
    }

    /// Loads (once) a classdef class from the search path, with its superclasses.
    pub(crate) fn find_class(&mut self, name: &str) -> Option<Rc<ClassDef>> {
        if let Some(c) = self.classes.get(name) {
            return Some(c.clone());
        }
        if self.not_on_path.contains(name) || self.path_functions.contains_key(name) {
            return None;
        }
        let Some((_, src)) = self.read_path_file(name) else {
            self.not_on_path.insert(name.to_string());
            return None;
        };
        if !crate::class::is_classdef(&src) {
            return None;
        }
        let ast = match crate::class::parse_classdef(&src) {
            Ok(a) => a,
            Err(e) => {
                self.emit(OutputKind::Warning(format!("Could not load class '{name}': {}", e.message)));
                self.not_on_path.insert(name.to_string());
                return None;
            }
        };
        let mut supers = Vec::new();
        for s in &ast.supers {
            if s == "handle" {
                continue;
            }
            match self.find_class(s) {
                Some(c) => supers.push(c),
                None => {
                    self.emit(OutputKind::Warning(format!("Superclass '{s}' of '{name}' was not found.")));
                }
            }
        }
        let def = Rc::new(ClassDef::build(ast, &supers));
        self.classes.insert(name.to_string(), def.clone());
        Some(def)
    }

    // ---- statements --------------------------------------------------------------------

    pub(crate) fn exec_block(&mut self, body: &[Stmt]) -> R<Flow> {
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

    /// Emits the display of `name = v`, honoring `disp`/`display` overloads of objects.
    fn show(&mut self, name: &str, v: &Value) -> R<()> {
        if let Value::Object(o) = v {
            if let Some(f) = o.class.method("display").cloned() {
                self.call_method(&f, &o.class.clone(), vec![v.clone()], 0)?;
                return Ok(());
            }
            if let Some(f) = o.class.method("disp").cloned() {
                self.emit(OutputKind::Text(format!("{name} = \n")));
                self.call_method(&f, &o.class.clone(), vec![v.clone()], 0)?;
                return Ok(());
            }
            let values = self.object_values(o)?;
            let text = crate::class::format_object(name, o, &values, self.format);
            self.emit(OutputKind::Value { name: name.to_string(), text });
            return Ok(());
        }
        let text = display_value(name, v, self.format);
        self.emit(OutputKind::Value { name: name.to_string(), text });
        Ok(())
    }

    /// Property values with Dependent ones computed through their `get.` methods.
    pub(crate) fn object_values(&mut self, o: &Object) -> R<Vec<Value>> {
        let mut values = o.props.borrow().clone();
        for (i, p) in o.class.props.iter().enumerate() {
            if p.dependent {
                if let Some(g) = o.class.method(&format!("get.{}", p.name)).cloned() {
                    values[i] = self.call_method(&g, &o.class.clone(), vec![Value::Object(o.clone())], 1)?.into_iter().next().unwrap_or_else(Value::empty);
                }
            }
        }
        Ok(values)
    }

    fn exec_inner(&mut self, s: &Stmt) -> R<Flow> {
        match &s.kind {
            StmtKind::Expr { expr, print } => {
                if let Expr::Ident(name) = expr {
                    if let Some(v) = self.get_var(name).cloned() {
                        if *print {
                            self.show(name, &v)?;
                        }
                        return Ok(Flow::Normal);
                    }
                }
                let vals = self.eval_multi(expr, 0)?;
                if let Some(v) = vals.into_iter().next() {
                    if *print {
                        self.show("ans", &v)?;
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
                    let needed = lhs.iter().rposition(|l| !matches!(l, LValue::Tilde)).map_or(0, |i| i + 1);
                    if vals.len() < needed {
                        return rt("Too many output arguments.");
                    }
                    for (l, v) in lhs.iter().zip(vals) {
                        self.assign(l, v)?;
                    }
                }
                if *print {
                    for l in lhs {
                        if let LValue::Target { name, .. } = l {
                            if let Some(v) = self.get_var(name).cloned() {
                                self.show(name, &v)?;
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
                        self.show("ans", &v)?;
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
                        let mut st = StructArray::scalar();
                        st.set("message", Value::chars(&e.message));
                        st.set("identifier", Value::chars(""));
                        st.set("stack", Value::Struct(StructArray::empty_with(vec!["file".into(), "name".into(), "line".into()])));
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
            let start_v = self.eval(a)?;
            if matches!(start_v, Value::Num(_)) {
                let start = start_v.scalar()?;
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
        }
        let v = self.eval(iter)?;
        let cols = match &v {
            Value::Func(_) | Value::Object(_) | Value::Table(_) => 1,
            other => {
                let (r, c) = other.dims2();
                if r == 0 { 0 } else { c }
            }
        };
        for c in 0..cols {
            let item = match &v {
                Value::Func(_) | Value::Object(_) | Value::Table(_) => v.clone(),
                other => indexing::index(other, &[IndexArg::All, IndexArg::scalar(c)])?,
            };
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
        // Pass 1: evaluate subscripts against the current sub-value (for `end`).
        let mut resolved = Vec::with_capacity(chain.len());
        for (k, acc) in chain.iter().enumerate() {
            let r = match acc {
                Accessor::Field(f) => Resolved::Field(f.clone()),
                Accessor::DynField(e) => Resolved::Field(self.eval(e)?.text()?),
                Accessor::Paren(args) | Accessor::Brace(args) => {
                    let (dims, is_table) = self.sub_info(name, &resolved[..k]);
                    let paren = matches!(acc, Accessor::Paren(_));
                    if is_table {
                        let vals = self.eval_raw_args(args, &dims)?;
                        if paren { Resolved::TableParen(vals) } else { Resolved::TableBrace(vals) }
                    } else {
                        let idx = self.eval_index_args(args, &dims)?;
                        if paren { Resolved::Paren(idx) } else { Resolved::Brace(idx) }
                    }
                }
            };
            resolved.push(r);
        }
        // Hot paths mutate the variable in place (no copy of large arrays per assignment).
        let rhs = if self.frame().globals.contains(name.as_str()) {
            rhs
        } else {
            match self.frame_mut().vars.get_mut(name.as_str()) {
                Some(target) => match assign_in_place(target, &resolved, rhs)? {
                    None => return Ok(()),
                    Some(rhs) => rhs,
                },
                None => rhs,
            }
        };
        // General path: on error the variable is restored from this snapshot.
        let cur = self.take_var(name);
        let snapshot = cur.clone();
        match self.apply_assign(cur, &resolved, rhs) {
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

    /// Dims of the sub-value reached by `path` from variable `name`, and whether it is a table.
    fn sub_info(&self, name: &str, path: &[Resolved]) -> (Dims, bool) {
        /// Walks by reference; only object properties (behind a RefCell) are copied.
        fn walk(v: &Value, path: &[Resolved]) -> (Dims, bool) {
            let Some((p, rest)) = path.split_first() else {
                return (v.dims(), matches!(v, Value::Table(_)));
            };
            match (p, v) {
                (Resolved::Field(f), Value::Struct(s)) if s.elems.len() == 1 => match s.get(f) {
                    Some(x) => walk(x, rest),
                    None => (vec![0, 0], false),
                },
                (Resolved::Field(f), Value::Object(o)) => match o.get(f) {
                    Some(x) => walk(&x, rest),
                    None => (vec![0, 0], false),
                },
                (Resolved::Field(f), Value::Table(t)) => match t.var(f) {
                    Some(x) => walk(x, rest),
                    None => (vec![0, 0], false),
                },
                (Resolved::Brace(idx), Value::Cell(c)) => match nd::index_plan(&c.dims, idx) {
                    Ok((pos, _)) if pos.len() == 1 => walk(&c.data[pos[0]], rest),
                    _ => (vec![0, 0], false),
                },
                (Resolved::Paren(idx), Value::Struct(s)) => match nd::index_plan(&s.dims, idx) {
                    Ok((pos, _)) if pos.len() == 1 => walk(&Value::Struct(s.element(pos[0])), rest),
                    _ => (vec![0, 0], false),
                },
                _ => (vec![0, 0], false),
            }
        }
        match self.get_var(name) {
            Some(v) => walk(v, path),
            None => (vec![0, 0], false),
        }
    }

    fn apply_assign(&mut self, cur: Option<Value>, chain: &[Resolved], rhs: Value) -> R<Value> {
        let Some((first, rest)) = chain.split_first() else {
            return Ok(rhs);
        };
        match first {
            Resolved::Field(f) => match cur {
                None => {
                    let mut s = StructArray::scalar();
                    let new = self.apply_assign(None, rest, rhs)?;
                    s.set(f, new);
                    Ok(Value::Struct(s))
                }
                Some(Value::Num(m)) if m.is_empty() => self.apply_assign(None, chain, rhs),
                Some(Value::Struct(mut s)) => {
                    if s.elems.len() != 1 {
                        return rt("Scalar structure required for this assignment.");
                    }
                    let sub = s.get(f).cloned();
                    let new = self.apply_assign(sub, rest, rhs)?;
                    s.set(f, new);
                    Ok(Value::Struct(s))
                }
                Some(Value::Object(o)) => {
                    if o.class.prop_index(f).is_none() {
                        return rt(format!("Unrecognized property '{f}' for class '{}'.", o.class.name));
                    }
                    let sub = o.get(f);
                    let new = self.apply_assign(sub, rest, rhs)?;
                    Ok(Value::Object(o.set(f, new)?))
                }
                Some(Value::Table(mut t)) => {
                    if f == "Properties" {
                        let sub = Some(t.properties());
                        let new = self.apply_assign(sub, rest, rhs)?;
                        t.set_properties(&new)?;
                    } else {
                        let sub = t.var(f).cloned();
                        let new = self.apply_assign(sub, rest, rhs)?;
                        t.set_var(f, new)?;
                    }
                    Ok(Value::Table(t))
                }
                Some(Value::Time(t)) => {
                    let sub = Some(t.get_prop(f)?);
                    let new = self.apply_assign(sub, rest, rhs)?;
                    Ok(Value::Time(t.set_prop(f, &new)?))
                }
                Some(other) => rt(format!("Unable to perform assignment because dot indexing is not supported for variables of this type ({}).", other.class_name())),
            },
            Resolved::Brace(idx) => {
                let cell = match cur {
                    None => Value::Cell(CellArray::from_dims(vec![0, 0], Vec::new())),
                    Some(Value::Num(m)) if m.is_empty() => Value::Cell(CellArray::from_dims(vec![0, 0], Vec::new())),
                    Some(c @ Value::Cell(_)) => c,
                    Some(other) => return rt(format!("Brace indexing is not supported for variables of this type ({}).", other.class_name())),
                };
                let new = if rest.is_empty() {
                    rhs
                } else {
                    let sub = match &cell {
                        Value::Cell(c) => match nd::index_plan(&c.dims, idx) {
                            Ok((pos, _)) if pos.len() == 1 => Some(c.data[pos[0]].clone()),
                            _ => None,
                        },
                        _ => None,
                    };
                    self.apply_assign(sub, rest, rhs)?
                };
                indexing::assign(Some(cell), idx, Value::Cell(CellArray::new(1, 1, vec![new])))
            }
            Resolved::Paren(idx) => {
                if rest.is_empty() {
                    return indexing::assign(cur, idx, rhs);
                }
                match cur {
                    Some(v @ (Value::Object(_) | Value::Cell(_))) => {
                        if nd::index_plan(&[1, 1], idx).map(|(p, _)| p == vec![0]).unwrap_or(false) {
                            self.apply_assign(Some(v), rest, rhs)
                        } else {
                            rt("Indexing into object arrays is not supported in this version.")
                        }
                    }
                    Some(Value::Num(m)) if !m.is_empty() => rt(format!("Unable to perform assignment into a value of class '{}'.", m.kind.name())),
                    Some(other @ (Value::Str(_) | Value::Func(_) | Value::Time(_) | Value::Table(_) | Value::Sym(_))) => {
                        rt(format!("Unable to perform assignment into a value of class '{}'.", other.class_name()))
                    }
                    // s(k).f = v on a struct array, growing it as needed.
                    cur => {
                        let mut s = match cur {
                            Some(Value::Struct(s)) => s,
                            _ => StructArray::empty_with(Vec::new()),
                        };
                        let existing = match nd::index_plan(&s.dims, idx) {
                            Ok((pos, _)) if pos.len() == 1 => Some(Value::Struct(s.element(pos[0]))),
                            _ => None,
                        };
                        let template = existing.unwrap_or_else(|| {
                            let mut e = StructArray::scalar();
                            for f in &s.fields {
                                e.set(f, Value::empty());
                            }
                            Value::Struct(e)
                        });
                        let new = self.apply_assign(Some(template), rest, rhs)?;
                        let Value::Struct(ns) = new else {
                            return rt("Unable to perform assignment because the indexed value is not a struct.");
                        };
                        for f in &ns.fields {
                            s.add_field(f);
                        }
                        indexing::assign(Some(Value::Struct(s)), idx, Value::Struct(ns))
                    }
                }
            }
            Resolved::TableParen(args) => {
                let Some(Value::Table(t)) = cur else { unreachable!() };
                if !rest.is_empty() {
                    return rt("Use T.Var(rows) to assign into part of a table variable.");
                }
                Ok(Value::Table(Box::new(t.assign_paren(args.clone(), rhs)?)))
            }
            Resolved::TableBrace(args) => {
                let Some(Value::Table(t)) = cur else { unreachable!() };
                if !rest.is_empty() {
                    return rt("Nested indexing after T{...} is not supported in assignments.");
                }
                Ok(Value::Table(Box::new(t.assign_brace(args.clone(), rhs)?)))
            }
        }
    }

    // ---- expressions -------------------------------------------------------------------

    pub fn eval(&mut self, e: &Expr) -> R<Value> {
        match e {
            Expr::Num(n) => Ok(Value::num(*n)),
            Expr::Imag(n) => Ok(Value::Num(Matrix::complex_scalar(0.0, *n))),
            Expr::Chars(s) => Ok(Value::chars(s)),
            Expr::Str(s) => Ok(Value::string(s.clone())),
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
                self.binary(*op, l, r)
            }
            Expr::Unary(op, x) => {
                let v = self.eval(x)?;
                if let Value::Object(o) = &v {
                    let name = match op {
                        UnOp::Neg => "uminus",
                        UnOp::Plus => "uplus",
                        UnOp::Not => "not",
                        UnOp::CTranspose => "ctranspose",
                        UnOp::Transpose => "transpose",
                    };
                    if let Some(f) = o.class.method(name).cloned() {
                        let class = o.class.clone();
                        return first(self.call_method(&f, &class, vec![v.clone()], 1)?, name);
                    }
                    if matches!(op, UnOp::CTranspose | UnOp::Transpose) {
                        return Ok(v);
                    }
                    return rt(format!("Undefined function '{name}' for input arguments of type '{}'.", o.class.name));
                }
                ops::unary(*op, &v)
            }
            Expr::Range(a, step, b) => {
                let start = self.eval(a)?;
                let step_v = match step {
                    Some(s) => Some(self.eval(s)?),
                    None => None,
                };
                let stop = self.eval(b)?;
                if matches!(start, Value::Time(_)) || matches!(step_v, Some(Value::Time(_))) {
                    return crate::time::range(&start, step_v.as_ref(), &stop);
                }
                if matches!(start, Value::Sym(_)) || matches!(stop, Value::Sym(_)) {
                    return rt("Colon operands must be real numeric scalars.");
                }
                let kind = match &start {
                    Value::Num(m) if m.kind.is_int() || m.kind == Kind::Single => m.kind,
                    _ => Kind::Double,
                };
                let start = start.scalar()?;
                let step = match step_v {
                    Some(s) => s.scalar()?,
                    None => 1.0,
                };
                let stop = stop.scalar()?;
                let n = range_count(start, step, stop);
                Ok(Value::Num(Matrix::new(1, n, (0..n).map(|i| start + i as f64 * step).collect()).cast_to(kind)))
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

    /// Binary operator with object overloading.
    pub(crate) fn binary(&mut self, op: BinOp, l: Value, r: Value) -> R<Value> {
        if matches!(l, Value::Object(_)) || matches!(r, Value::Object(_)) {
            let name = match op {
                BinOp::Add => "plus",
                BinOp::Sub => "minus",
                BinOp::Mul => "times",
                BinOp::MatMul => "mtimes",
                BinOp::Div => "rdivide",
                BinOp::MatDiv => "mrdivide",
                BinOp::LeftDiv => "ldivide",
                BinOp::MatLeftDiv => "mldivide",
                BinOp::Pow => "power",
                BinOp::MatPow => "mpower",
                BinOp::Eq => "eq",
                BinOp::Ne => "ne",
                BinOp::Lt => "lt",
                BinOp::Le => "le",
                BinOp::Gt => "gt",
                BinOp::Ge => "ge",
                BinOp::And | BinOp::AndAnd => "and",
                BinOp::Or | BinOp::OrOr => "or",
            };
            for side in [&l, &r] {
                if let Value::Object(o) = side {
                    if let Some(f) = o.class.method(name).cloned() {
                        let class = o.class.clone();
                        return first(self.call_method(&f, &class, vec![l.clone(), r.clone()], 1)?, name);
                    }
                }
            }
            if let (Value::Object(a), Value::Object(b)) = (&l, &r) {
                if a.class.is_handle && matches!(op, BinOp::Eq | BinOp::Ne) {
                    return Ok(Value::bool((Rc::ptr_eq(&a.props, &b.props)) == (op == BinOp::Eq)));
                }
            }
            let cls = if let Value::Object(o) = &l { o.class.name.clone() } else if let Value::Object(o) = &r { o.class.name.clone() } else { String::new() };
            return rt(format!("Undefined function '{name}' for input arguments of type '{cls}'."));
        }
        ops::binary(op, &l, &r)
    }

    /// Evaluates an expression that may yield several values (function outputs, `c{:}`,
    /// `s.f` on a struct array).
    pub fn eval_multi(&mut self, e: &Expr, nargout: usize) -> R<Vec<Value>> {
        match e {
            Expr::Ident(name) => {
                if let Some(v) = self.get_var(name) {
                    return Ok(vec![v.clone()]);
                }
                self.call_function(name, Vec::new(), nargout)
            }
            Expr::SuperRef { name, class } => self.super_call(name, class, Vec::new(), nargout),
            Expr::Index(base, args) => {
                // Fast path: index a variable (or a scalar-struct field) by reference.
                if let Some((name, field)) = var_ref(base) {
                    let dims = match self.var_at(name, field) {
                        Some(v) if !matches!(v, Value::Func(_) | Value::Table(_) | Value::Object(_)) => Some(v.dims()),
                        _ => None,
                    };
                    if let Some(dims) = dims {
                        let idx = self.eval_index_args(args, &dims)?;
                        if let Some(v) = self.var_at(name, field) {
                            return Ok(vec![indexing::index(v, &idx)?]);
                        }
                    }
                }
                match base.as_ref() {
                    Expr::Ident(name) if self.get_var(name).is_none() => {
                        if name == "table" {
                            self.arg_names = args.iter().map(|a| if let Expr::Ident(n) = a { Some(n.clone()) } else { None }).collect();
                        }
                        let argv = self.eval_args(args)?;
                        return self.call_function(name, argv, nargout);
                    }
                    Expr::SuperRef { name, class } => {
                        let argv = self.eval_args(args)?;
                        return self.super_call(name, class, argv, nargout);
                    }
                    // obj.method(args), Class.staticMethod(args), pkg-like calls.
                    Expr::Field(obj_expr, member) => {
                        if let Expr::Ident(cls) = obj_expr.as_ref() {
                            if self.get_var(cls).is_none() {
                                if let Some(c) = self.find_class(cls) {
                                    let argv = self.eval_args(args)?;
                                    return self.static_call(&c, member, argv, nargout);
                                }
                            }
                        }
                        let target = self.eval(obj_expr)?;
                        if let Value::Object(o) = &target {
                            if o.class.prop_index(member).is_none() {
                                if let Some(f) = o.class.method(member).cloned() {
                                    let mut argv = vec![target.clone()];
                                    argv.extend(self.eval_args(args)?);
                                    let class = o.class.clone();
                                    return self.call_method(&f, &class, argv, nargout);
                                }
                            }
                        }
                        let bv = self.member(&target, member)?;
                        return self.index_into(bv, args, nargout);
                    }
                    _ => {}
                }
                let bv = self.eval(base)?;
                self.index_into(bv, args, nargout)
            }
            Expr::CellIndex(base, args) => {
                if let Some((name, field)) = var_ref(base) {
                    let dims = match self.var_at(name, field) {
                        Some(Value::Cell(c)) => Some(c.dims.clone()),
                        _ => None,
                    };
                    if let Some(dims) = dims {
                        let idx = self.eval_index_args(args, &dims)?;
                        if let Some(Value::Cell(c)) = self.var_at(name, field) {
                            let (pos, _) = nd::index_plan(&c.dims, &idx)?;
                            return Ok(pos.into_iter().map(|p| c.data[p].clone()).collect());
                        }
                    }
                }
                let bv = self.eval(base)?;
                if let Value::Table(t) = &bv {
                    let vals = self.eval_raw_args(args, &bv.dims())?;
                    return Ok(vec![t.brace(&vals)?]);
                }
                let Value::Cell(c) = &bv else {
                    return rt(format!("Brace indexing is not supported for variables of this type ({}).", bv.class_name()));
                };
                let idx = self.eval_index_args(args, &c.dims)?;
                let (pos, _) = nd::index_plan(&c.dims, &idx)?;
                Ok(pos.into_iter().map(|p| c.data[p].clone()).collect())
            }
            Expr::Field(base, f) => {
                if let Expr::Ident(cls) = base.as_ref() {
                    if self.get_var(cls).is_none() {
                        if let Some(c) = self.find_class(cls) {
                            return self.static_call(&c, f, Vec::new(), nargout);
                        }
                    }
                }
                if let Expr::Ident(name) = base.as_ref() {
                    if let Some(Value::Struct(s)) = self.get_var(name) {
                        if s.elems.len() == 1 {
                            if let Some(v) = s.get(f) {
                                return Ok(vec![v.clone()]);
                            }
                        }
                    }
                }
                let bv = self.eval(base)?;
                if let Value::Struct(s) = &bv {
                    if s.elems.len() != 1 {
                        let k = s.field_index(f).ok_or_else(|| LangError::new(format!("Unrecognized field name \"{f}\"."), 0))?;
                        return Ok(s.elems.iter().map(|e| e[k].clone()).collect());
                    }
                }
                if let Value::Object(o) = &bv {
                    if o.class.prop_index(f).is_none() {
                        if let Some(m) = o.class.method(f).cloned() {
                            let class = o.class.clone();
                            return self.call_method(&m, &class, vec![bv.clone()], nargout);
                        }
                    }
                }
                Ok(vec![self.member(&bv, f)?])
            }
            Expr::DynField(base, f) => {
                let bv = self.eval(base)?;
                let name = self.eval(f)?.text()?;
                Ok(vec![self.member(&bv, &name)?])
            }
            other => Ok(vec![self.eval(other)?]),
        }
    }

    /// `v(args)`: indexing, or calling a function handle.
    fn index_into(&mut self, bv: Value, args: &[Expr], nargout: usize) -> R<Vec<Value>> {
        match &bv {
            Value::Func(f) => {
                let argv = self.eval_args(args)?;
                self.call_value(f, argv, nargout)
            }
            Value::Table(t) => {
                let vals = self.eval_raw_args(args, &bv.dims())?;
                Ok(vec![Value::Table(Box::new(t.paren(&vals)?))])
            }
            Value::Object(o) => {
                if let Some(f) = o.class.method("subsref").cloned() {
                    let _ = f;
                    return rt("subsref overloading is not supported in this version.");
                }
                let idx = self.eval_index_args(args, &[1, 1])?;
                Ok(vec![indexing::index(&bv, &idx)?])
            }
            _ => {
                let idx = self.eval_index_args(args, &bv.dims())?;
                Ok(vec![indexing::index(&bv, &idx)?])
            }
        }
    }

    /// `v.name` for structs, objects, tables and datetimes.
    fn member(&mut self, v: &Value, name: &str) -> R<Value> {
        match v {
            Value::Struct(s) => {
                if s.elems.is_empty() {
                    return rt("Dot indexing into an empty struct array is not supported.");
                }
                s.get(name).cloned().ok_or_else(|| LangError::new(format!("Unrecognized field name \"{name}\"."), 0))
            }
            Value::Object(o) => {
                let Some(i) = o.class.prop_index(name) else {
                    return rt(format!("Unrecognized method, property, or field '{name}' for class '{}'.", o.class.name));
                };
                let p = &o.class.props[i];
                if p.dependent {
                    if let Some(g) = o.class.method(&format!("get.{name}")).cloned() {
                        let class = o.class.clone();
                        return first(self.call_method(&g, &class, vec![v.clone()], 1)?, name);
                    }
                }
                if p.constant {
                    if let Some(d) = p.default.clone() {
                        return self.eval_in_class_scope(&d);
                    }
                }
                Ok(o.props.borrow()[i].clone())
            }
            Value::Table(t) => {
                if name == "Properties" {
                    return Ok(t.properties());
                }
                t.var(name).cloned().ok_or_else(|| LangError::new(format!("Unrecognized table variable name '{name}'."), 0))
            }
            Value::Time(t) => t.get_prop(name),
            other => rt(format!("Dot indexing is not supported for variables of this type ({}).", other.class_name())),
        }
    }

    fn eval_in_class_scope(&mut self, e: &Expr) -> R<Value> {
        self.frames.push(Frame { vars: HashMap::new(), globals: HashSet::new() });
        let r = self.eval(e);
        self.frames.pop();
        r
    }

    /// Function-call arguments with comma-list expansion of `c{:}` and `s.f`.
    pub(crate) fn eval_args(&mut self, args: &[Expr]) -> R<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Expr::CellIndex(..) | Expr::Field(..) => out.extend(self.eval_multi(a, 1)?),
                Expr::Colon => out.push(Value::chars(":")),
                _ => out.push(self.eval(a)?),
            }
        }
        Ok(out)
    }

    fn eval_index_args(&mut self, args: &[Expr], dims: &[usize]) -> R<Vec<IndexArg>> {
        let n = args.len();
        let mut out = Vec::with_capacity(n);
        for (k, a) in args.iter().enumerate() {
            if matches!(a, Expr::Colon) {
                out.push(IndexArg::All);
                continue;
            }
            self.end_stack.push(nd::end_value(dims, k, n) as f64);
            let v = self.eval(a);
            self.end_stack.pop();
            out.push(to_index_arg(&v?)?);
        }
        Ok(out)
    }

    /// Subscripts as values (tables accept names), with `end` support.
    fn eval_raw_args(&mut self, args: &[Expr], dims: &[usize]) -> R<Vec<Value>> {
        let n = args.len();
        let mut out = Vec::with_capacity(n);
        for (k, a) in args.iter().enumerate() {
            if matches!(a, Expr::Colon) {
                out.push(Value::chars(":"));
                continue;
            }
            self.end_stack.push(nd::end_value(dims, k, n) as f64);
            let v = self.eval(a);
            self.end_stack.pop();
            out.push(v?);
        }
        Ok(out)
    }

    fn eval_list(&mut self, row: &[Expr]) -> R<Vec<Value>> {
        let mut items = Vec::with_capacity(row.len());
        for e in row {
            match e {
                Expr::CellIndex(..) | Expr::Field(..) => items.extend(self.eval_multi(e, 1)?),
                _ => items.push(self.eval(e)?),
            }
        }
        Ok(items)
    }

    fn eval_matrix(&mut self, rows: &[Vec<Expr>]) -> R<Value> {
        let mut row_vals = Vec::with_capacity(rows.len());
        for row in rows {
            let items = self.eval_list(row)?;
            row_vals.push(indexing::hconcat(items)?);
        }
        indexing::vconcat(row_vals)
    }

    fn eval_cell(&mut self, rows: &[Vec<Expr>]) -> R<Value> {
        let mut row_vals = Vec::new();
        for row in rows {
            let items = self.eval_list(row)?;
            // Inside braces each element becomes one cell (cells nest rather than merge).
            let cells: Vec<Value> = items.into_iter().map(|v| Value::Cell(CellArray::new(1, 1, vec![v]))).collect();
            row_vals.push(indexing::hconcat(cells)?);
        }
        if row_vals.is_empty() {
            return Ok(Value::Cell(CellArray::from_dims(vec![0, 0], Vec::new())));
        }
        indexing::vconcat(row_vals)
    }

    // ---- calls -------------------------------------------------------------------------

    pub fn call_function(&mut self, name: &str, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        if let Some(f) = self.functions.get(name).cloned() {
            return self.call_user(&f, args, nargout);
        }
        // Methods of an object argument take precedence over other functions.
        for a in &args {
            if let Value::Object(o) = a {
                if let Some(f) = o.class.method(name).cloned() {
                    let class = o.class.clone();
                    return self.call_method(&f, &class, args, nargout);
                }
            }
        }
        if !crate::builtins::exists(name) {
            if let Some(c) = self.find_class(name) {
                return Ok(vec![self.construct(&c, args)?]);
            }
            if let Some(f) = self.find_path_function(name) {
                return self.call_user(&f, args, nargout);
            }
            if let Some(p) = self.path_scripts.get(name).cloned() {
                if !args.is_empty() {
                    return rt(format!("Attempt to execute SCRIPT {name} as a function."));
                }
                self.define_functions(&p);
                match self.exec_block(&p.body)? {
                    _ => return Ok(Vec::new()),
                }
            }
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

    pub(crate) fn call_method(&mut self, f: &Rc<Function>, class: &Rc<ClassDef>, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        self.class_stack.push(class.clone());
        let r = self.call_user(f, args, nargout);
        self.class_stack.pop();
        r
    }

    fn static_call(&mut self, c: &Rc<ClassDef>, member: &str, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        if let Some(i) = c.prop_index(member) {
            if c.props[i].constant {
                if let Some(d) = c.props[i].default.clone() {
                    return Ok(vec![self.eval_in_class_scope(&d)?]);
                }
                return Ok(vec![Value::empty()]);
            }
        }
        match c.method(member).cloned() {
            Some(f) => self.call_method(&f, c, args, nargout),
            None => rt(format!("The class {} has no Constant property or Static method named '{member}'.", c.name)),
        }
    }

    /// Creates an object with property defaults and runs the constructor chain.
    pub(crate) fn construct(&mut self, c: &Rc<ClassDef>, args: Vec<Value>) -> R<Value> {
        let mut props = Vec::with_capacity(c.props.len());
        for p in &c.props {
            props.push(match &p.default {
                Some(d) if !p.dependent => self.eval_in_class_scope(d)?,
                _ => Value::empty(),
            });
        }
        let obj = Value::Object(Object { class: c.clone(), props: Rc::new(RefCell::new(props)) });
        self.run_ctor(c, obj, args)
    }

    /// Runs `c`'s constructor on `obj` (or the default constructor, which forwards to the
    /// superclass constructors).
    fn run_ctor(&mut self, c: &Rc<ClassDef>, obj: Value, args: Vec<Value>) -> R<Value> {
        let Some(ctor) = c.ctor.clone() else {
            let mut obj = obj;
            for s in c.supers.clone() {
                if s == "handle" {
                    continue;
                }
                if let Some(sc) = self.find_class(&s) {
                    obj = self.run_ctor(&sc, obj, args.clone())?;
                }
            }
            return Ok(obj);
        };
        // Superclass constructors run implicitly (with no arguments) unless called explicitly.
        let mut obj = obj;
        if !body_calls_super(&ctor.body) {
            for s in c.supers.clone() {
                if s == "handle" {
                    continue;
                }
                if let Some(sc) = self.find_class(&s) {
                    obj = self.run_ctor(&sc, obj, Vec::new())?;
                }
            }
        }
        let out_name = ctor.outputs.first().cloned().unwrap_or_else(|| "obj".into());
        self.class_stack.push(c.clone());
        let r = self.call_user_with(&ctor, args, 1, Some((out_name, obj)));
        self.class_stack.pop();
        let mut out = r?;
        if out.is_empty() {
            return rt(format!("The constructor of class '{}' must return the object.", c.name));
        }
        Ok(out.swap_remove(0))
    }

    /// `obj@Super(args)` in a constructor, or `method@Super(args)` in a method.
    fn super_call(&mut self, name: &str, class: &str, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        let Some(sc) = self.find_class(class) else {
            return rt(format!("Unable to find superclass '{class}'."));
        };
        if let Some(Value::Object(_)) = self.get_var(name) {
            let obj = self.get_var(name).cloned().unwrap();
            let new = self.run_ctor(&sc, obj, args)?;
            self.set_var(name, new);
            return Ok(Vec::new());
        }
        match sc.method(name).cloned() {
            Some(f) => self.call_method(&f, &sc, args, nargout),
            None => rt(format!("Superclass '{class}' has no method '{name}'.")),
        }
    }

    fn call_user(&mut self, f: &Rc<Function>, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        self.call_user_with(f, args, nargout, None)
    }

    fn call_user_with(&mut self, f: &Rc<Function>, args: Vec<Value>, nargout: usize, preset: Option<(String, Value)>) -> R<Vec<Value>> {
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
        if let Some((n, v)) = preset {
            vars.insert(n, v);
        }
        let mut it = args.into_iter();
        for p in f.params.iter().take(fixed) {
            match it.next() {
                Some(a) => {
                    if p != "~" {
                        vars.insert(p.clone(), a);
                    }
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

/// Applies common assignment forms directly to the stored value: `x(i) = v` on numeric
/// arrays, `c{i} = v`, `s.f = v` on scalar structs, and nested paths through them. Returns
/// the right-hand side back (`Some`) when the form is not handled here. Every case validates
/// before writing, so an error leaves `target` unchanged.
fn assign_in_place(target: &mut Value, chain: &[Resolved], rhs: Value) -> R<Option<Value>> {
    let Some((first, rest)) = chain.split_first() else {
        return Ok(Some(rhs));
    };
    match (first, target) {
        (Resolved::Paren(idx), Value::Num(m)) if rest.is_empty() => {
            let Value::Num(r) = &rhs else { return Ok(Some(rhs)) };
            let deleting = r.dims == [0, 0] && r.kind != Kind::Char;
            if deleting || r.is_complex() || m.is_complex() || !(m.kind == r.kind || (m.kind != Kind::Logical && !m.is_empty())) {
                return Ok(Some(rhs));
            }
            let kind = m.kind;
            let data: Vec<f64> = if kind == r.kind || kind == Kind::Double { r.data.clone() } else { r.data.iter().map(|&x| kind.cast(x)).collect() };
            let mut dims = m.dims.clone();
            nd::assign(&mut dims, &mut m.data, idx, &r.dims, data, 0.0)?;
            m.dims = dims;
            Ok(None)
        }
        (Resolved::Brace(idx), Value::Cell(c)) => {
            if rest.is_empty() {
                let mut dims = c.dims.clone();
                nd::assign(&mut dims, &mut c.data, idx, &[1, 1], vec![rhs], Value::empty())?;
                c.dims = dims;
                return Ok(None);
            }
            match nd::index_plan(&c.dims, idx) {
                Ok((pos, _)) if pos.len() == 1 => assign_in_place(&mut c.data[pos[0]], rest, rhs),
                _ => Ok(Some(rhs)),
            }
        }
        (Resolved::Field(f), Value::Struct(s)) if s.elems.len() == 1 => {
            if rest.is_empty() {
                s.set(f, rhs);
                return Ok(None);
            }
            match s.field_index(f) {
                Some(k) => assign_in_place(&mut s.elems[0][k], rest, rhs),
                None => Ok(Some(rhs)),
            }
        }
        _ => Ok(Some(rhs)),
    }
}

/// `x` or `s.f` as (variable, optional field) for by-reference reads.
fn var_ref(e: &Expr) -> Option<(&str, Option<&str>)> {
    match e {
        Expr::Ident(n) => Some((n.as_str(), None)),
        Expr::Field(b, f) => match b.as_ref() {
            Expr::Ident(n) => Some((n.as_str(), Some(f.as_str()))),
            _ => None,
        },
        _ => None,
    }
}

impl Interpreter {
    /// Variable `name`, or field `field` of it when it is a scalar struct.
    fn var_at(&self, name: &str, field: Option<&str>) -> Option<&Value> {
        let v = self.get_var(name)?;
        match field {
            None => Some(v),
            Some(f) => match v {
                Value::Struct(s) if s.elems.len() == 1 => s.get(f),
                _ => None,
            },
        }
    }
}

fn first(mut v: Vec<Value>, what: &str) -> R<Value> {
    if v.is_empty() {
        return rt(format!("'{what}' did not return a value."));
    }
    Ok(v.swap_remove(0))
}

fn body_calls_super(body: &[Stmt]) -> bool {
    fn expr(e: &Expr) -> bool {
        match e {
            Expr::SuperRef { .. } => true,
            Expr::Index(b, a) | Expr::CellIndex(b, a) => expr(b) || a.iter().any(expr),
            Expr::Binary(_, a, b) => expr(a) || expr(b),
            Expr::Unary(_, a) | Expr::Field(a, _) => expr(a),
            _ => false,
        }
    }
    body.iter().any(|s| match &s.kind {
        StmtKind::Expr { expr: e, .. } => expr(e),
        StmtKind::Assign { rhs, .. } => expr(rhs),
        StmtKind::If { clauses, otherwise } => clauses.iter().any(|(c, b)| expr(c) || body_calls_super(b)) || otherwise.as_ref().map(|b| body_calls_super(b)).unwrap_or(false),
        StmtKind::Try { body, catch_body, .. } => body_calls_super(body) || body_calls_super(catch_body),
        _ => false,
    })
}

fn scalar_truth(v: &Value, op: &str) -> R<bool> {
    match v {
        Value::Num(m) if m.numel() == 1 => Ok(m.data[0] != 0.0 || m.im_at(0) != 0.0),
        Value::Sym(_) if v.numel() == 1 => v.is_true(),
        _ => rt(format!("Operands to the {op} operator must be convertible to logical scalar values.")),
    }
}

fn switch_match(subject: &Value, case: &Value) -> bool {
    if subject.is_text() && case.is_text() {
        return subject.text().ok() == case.text().ok();
    }
    match (subject, case) {
        (Value::Num(a), Value::Num(b)) if a.is_scalar() && b.is_scalar() => a.data[0] == b.data[0],
        (Value::Object(_), Value::Object(_)) => subject == case,
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
        Value::Num(m) => m.numel() * m.kind.bytes() * if m.is_complex() { 2 } else { 1 },
        Value::Str(s) => s.data.iter().map(|x| x.as_ref().map(|t| t.len() * 2).unwrap_or(0) + 16).sum(),
        Value::Cell(c) => c.data.iter().map(approx_bytes).sum::<usize>() + c.data.len() * 8,
        Value::Struct(s) => s.elems.iter().flatten().map(|v| approx_bytes(v) + 64).sum(),
        Value::Func(_) => 32,
        Value::Object(o) => o.props.borrow().iter().map(approx_bytes).sum::<usize>() + 8,
        Value::Time(t) => t.data.len() * 8,
        Value::Table(t) => t.vars.iter().map(approx_bytes).sum(),
        Value::Sym(s) => s.data.len() * 8,
    }
}
