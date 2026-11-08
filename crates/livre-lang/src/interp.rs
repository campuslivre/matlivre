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
use crate::graphics::{ExportFormat, Figure};
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
    /// The user function running in this frame (for `mfilename`).
    func: Option<String>,
    /// The line of the statement this frame is running (for error stacks).
    line: usize,
}

/// Where a function's code comes from: the file's name (without extension), its path, and
/// its lines (for error reports).
#[derive(Clone)]
struct CodeSource {
    stem: String,
    path: String,
    lines: Rc<Vec<String>>,
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
    /// The search path the host set, which `restoredefaultpath` returns to.
    default_path: Vec<PathBuf>,
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
    /// Next graphics handle number for axes and plotted objects (see `gfx`).
    pub(crate) next_handle: u64,
    /// Graphics properties set by name that the figure model does not hold, per handle.
    pub(crate) gfx_store: HashMap<u64, Vec<(String, Value)>>,
    /// Renders a figure for `saveas`/`print`: PNG bytes at a scale, or SVG text.
    pub(crate) exporter: Option<Box<dyn FnMut(&Figure, ExportFormat, f32) -> Result<Vec<u8>, String>>>,
    pending: Vec<Output>,
    top_line: usize,
    /// The script being run (for error stacks), its path and lines.
    script_name: String,
    script_path: String,
    script_lines: Rc<Vec<String>>,
    /// The source of each user function, by name.
    code_sources: HashMap<String, CodeSource>,
    flush: Option<Box<dyn FnMut(Vec<Output>)>>,
    cancel: Option<Arc<AtomicBool>>,
    pub(crate) tic: Instant,
    pub(crate) rng: crate::random::Streams,
    pub(crate) timers: crate::timers::Timers,
    pub(crate) settings: crate::settings::Tree,
    /// Property set methods running now (class, property): their own assignment is direct.
    pub(crate) active_setters: Vec<(String, String)>,
    /// `diary`: whether output is being copied to the diary file, and the file.
    pub(crate) diary: (bool, String),
    /// The functions of each loaded file, in file order: (the first is the file's main
    /// function, names) — for `localfunctions`.
    pub(crate) local_groups: Vec<(bool, Vec<String>)>,
    end_stack: Vec<f64>,
    depth: usize,
    /// Source names of the arguments of the builtin being called (`table` uses them).
    pub(crate) arg_names: Vec<Option<String>>,
    /// The last error and warning (message, identifier), for `lasterr` and `lastwarn`.
    pub(crate) last_error: (String, String),
    pub(crate) last_warning: (String, String),
    /// Event listeners of handle objects (addlistener, listener).
    pub(crate) listeners: Vec<crate::lang_more::Listener>,
    jit: Option<crate::jit::Jit>,
    jit_enabled: bool,
    /// Compiled kernels by (statement address, variable types); `None` = not compilable.
    jit_cache: HashMap<(usize, String), Option<Rc<crate::jit::Kernel>>>,
    jit_names: HashMap<usize, Vec<String>>,
    /// Native entries for user functions called with scalar doubles, by (function, arity).
    jit_entries: HashMap<(usize, usize), Option<extern "C" fn(*mut crate::jit::Ctx, *const f64, *mut f64) -> i32>>,
    jit_runs: usize,
}

static NO_CANCEL: u8 = 0;

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            base: Frame { vars: HashMap::new(), globals: HashSet::new(), func: None, line: 0 },
            frames: Vec::new(),
            global_vars: HashMap::new(),
            functions: HashMap::new(),
            search_path: Vec::new(),
            default_path: Vec::new(),
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
            next_handle: crate::gfx::OBJ_BASE,
            gfx_store: HashMap::new(),
            exporter: None,
            pending: Vec::new(),
            top_line: 0,
            script_name: String::new(),
            script_path: String::new(),
            script_lines: Rc::new(Vec::new()),
            code_sources: HashMap::new(),
            flush: None,
            cancel: None,
            tic: Instant::now(),
            rng: crate::random::Streams::new(),
            timers: Default::default(),
            settings: Default::default(),
            active_setters: Vec::new(),
            diary: (false, "diary".into()),
            local_groups: Vec::new(),
            end_stack: Vec::new(),
            depth: 0,
            arg_names: Vec::new(),
            last_error: (String::new(), String::new()),
            last_warning: (String::new(), String::new()),
            listeners: Vec::new(),
            jit: None,
            jit_enabled: std::env::var_os("MATLIVRE_NOJIT").is_none(),
            jit_cache: HashMap::new(),
            jit_names: HashMap::new(),
            jit_entries: HashMap::new(),
            jit_runs: 0,
        }
    }

    /// Enables or disables native compilation of loops (enabled by default; the
    /// `MATLIVRE_NOJIT` environment variable disables it).
    pub fn set_jit(&mut self, on: bool) {
        self.jit_enabled = on;
    }

    /// Number of loop executions that ran as native code.
    pub fn jit_runs(&self) -> usize {
        self.jit_runs
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
        self.default_path = dirs.clone();
        self.search_path = dirs;
        self.reset_path_cache();
    }

    pub fn search_path(&self) -> &[PathBuf] {
        &self.search_path
    }

    /// Replaces the search path from `path`, `rmpath` or `restoredefaultpath`, keeping the
    /// host's default.
    pub fn replace_search_path(&mut self, dirs: Vec<PathBuf>) {
        self.search_path = dirs;
        self.reset_path_cache();
    }

    pub fn default_search_path(&self) -> Vec<PathBuf> {
        self.default_path.clone()
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
        self.jit_entries.clear();
        for f in &prog.functions {
            self.functions.insert(f.name.clone(), f.clone());
        }
        if !prog.functions.is_empty() {
            self.local_groups.push((false, prog.functions.iter().map(|f| f.name.clone()).collect()));
        }
    }

    pub fn clear_functions(&mut self) {
        self.functions.clear();
    }

    pub fn clear_workspace(&mut self) {
        self.base.vars.clear();
    }

    /// A copy of the open figures, oldest first.
    pub fn figures_snapshot(&self) -> Vec<Figure> {
        self.figures.clone()
    }

    /// Installs the renderer `saveas`, `print` and `exportgraphics` use.
    pub fn set_figure_exporter(&mut self, f: impl FnMut(&Figure, ExportFormat, f32) -> Result<Vec<u8>, String> + 'static) {
        self.exporter = Some(Box::new(f));
    }

    pub fn close_all_figures(&mut self) {
        self.figures.clear();
        self.gfx_store.clear();
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
        // The script's lines, and its local functions' source, for error reports.
        let lines: Rc<Vec<String>> = Rc::new(src.lines().map(str::to_string).collect());
        self.script_lines = lines.clone();
        for f in &prog.functions {
            self.code_sources.insert(f.name.clone(), CodeSource { stem: self.script_name.clone(), path: self.script_path.clone(), lines: lines.clone() });
        }
        self.run(&prog.body)
    }

    /// Runs top-level statements in the base workspace.
    pub fn run(&mut self, body: &[Stmt]) -> R<()> {
        self.frames.clear();
        self.class_stack.clear();
        // Kernels are keyed by AST addresses, which are only stable within one run.
        self.jit_cache.clear();
        self.jit_names.clear();
        self.jit_entries.clear();
        if let Some(j) = self.jit.as_mut() {
            j.reset_functions();
        }
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
                    self.last_error = (e.message.clone(), e.identifier.clone());
                    let line = self.top_line.max(1);
                    let kind = OutputKind::Error(e.message.clone());
                    self.diary_write(&kind);
                    self.pending.push(Output { line, kind });
                    self.flush_outputs();
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn emit(&mut self, kind: OutputKind) {
        self.diary_write(&kind);
        self.pending.push(Output { line: self.top_line, kind });
    }

    /// Appends output to the diary file while `diary` is on.
    fn diary_write(&mut self, kind: &OutputKind) {
        if !self.diary.0 {
            return;
        }
        let text = match kind {
            OutputKind::Text(t) => t.clone(),
            OutputKind::Value { text, .. } => format!("{text}
"),
            OutputKind::Warning(w) => format!("Warning: {w}
"),
            OutputKind::Error(e) => format!("{e}
"),
            OutputKind::Figure(_) => return,
        };
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.diary.1) {
            let _ = f.write_all(text.as_bytes());
        }
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

    /// True when a user function (not a builtin) with this name is defined.
    pub(crate) fn user_function_defined(&self, name: &str) -> bool {
        self.functions.contains_key(name)
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

    fn get_var_mut(&mut self, name: &str) -> Option<&mut Value> {
        let v = if self.frame().globals.contains(name) {
            self.global_vars.get_mut(name)
        } else {
            self.frame_mut().vars.get_mut(name)
        };
        if let Some(Value::Num(m)) = v {
            m.csc.clear();
        }
        v
    }

    fn take_var(&mut self, name: &str) -> Option<Value> {
        let mut v = if self.frame().globals.contains(name) {
            self.global_vars.remove(name)
        } else {
            self.frame_mut().vars.remove(name)
        };
        if let Some(Value::Num(m)) = &mut v {
            m.csc.clear();
        }
        v
    }

    pub(crate) fn remove_var(&mut self, name: &str) {
        if let Some(v) = self.take_var(name) {
            crate::builtins_validate::run_cleanup(self, &v);
        }
    }

    pub(crate) fn var_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.frame().vars.keys().cloned().collect();
        v.sort();
        v
    }

    pub(crate) fn clear_current_scope(&mut self) {
        let vars: Vec<Value> = self.frame_mut().vars.drain().map(|(_, v)| v).collect();
        for v in &vars {
            crate::builtins_validate::run_cleanup(self, v);
        }
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

    /// Where a function or script file on the search path is (for `which`).
    pub(crate) fn function_file(&self, name: &str) -> Option<String> {
        self.read_path_file(name).map(|(p, _)| p.display().to_string())
    }

    /// The source of a function or script file on the search path (for `type`).
    pub(crate) fn function_source(&self, name: &str) -> Option<String> {
        self.read_path_file(name).map(|(_, s)| s)
    }

    /// Loads (once) a function or script file from the search path.
    fn find_path_function(&mut self, name: &str) -> Option<Rc<Function>> {
        if let Some(f) = self.path_functions.get(name) {
            return Some(f.clone());
        }
        if self.not_on_path.contains(name) || self.classes.contains_key(name) {
            return None;
        }
        let Some((file, src)) = self.read_path_file(name) else {
            self.not_on_path.insert(name.to_string());
            return None;
        };
        let code = CodeSource {
            stem: file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
            path: file.display().to_string(),
            lines: Rc::new(src.lines().map(str::to_string).collect()),
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
        for f in &prog.functions {
            self.code_sources.entry(f.name.clone()).or_insert_with(|| code.clone());
        }
        self.local_groups.push((true, prog.functions.iter().map(|f| f.name.clone()).collect()));
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
        if let Some(c) = crate::lang_more::builtin_class(name) {
            self.classes.insert(name.to_string(), c.clone());
            return Some(c);
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
            // Built-in base classes have nothing to load.
            if s == "handle" || s == "dynamicprops" {
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

    // ---- eval --------------------------------------------------------------------------

    /// Runs `src` in the current workspace, as `eval` does. With `nargout > 0` the text
    /// must be a single expression, whose values are returned.
    pub(crate) fn eval_text(&mut self, src: &str, nargout: usize) -> R<Vec<Value>> {
        let prog = crate::parser::parse(src)?;
        // Statements inside the text must not move the line errors are reported on.
        let line = self.top_line;
        let r = self.eval_program(&prog, nargout);
        self.top_line = line;
        r
    }

    fn eval_program(&mut self, prog: &crate::ast::Program, nargout: usize) -> R<Vec<Value>> {
        if nargout > 0 {
            return match prog.body.as_slice() {
                [Stmt { kind: StmtKind::Expr { expr, .. }, .. }] => self.eval_multi(expr, nargout),
                _ => rt("Unable to return a value from a statement; the text must be an expression."),
            };
        }
        for s in &prog.body {
            let saved = self.frames.is_empty().then_some(self.top_line);
            let flow = self.exec_inner(s);
            if let Some(l) = saved {
                self.top_line = l;
            }
            match flow? {
                Flow::Normal => {}
                _ => break,
            }
        }
        Ok(Vec::new())
    }

    /// Runs `f` with the base workspace (`"base"`) or the caller's (`"caller"`) as the
    /// current one, for `evalin` and `assignin`.
    pub(crate) fn in_workspace<T>(&mut self, which: &str, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        match which {
            "base" => {
                let saved = std::mem::take(&mut self.frames);
                let r = f(self);
                self.frames = saved;
                r
            }
            "caller" => {
                let top = self.frames.pop();
                let r = f(self);
                if let Some(t) = top {
                    self.frames.push(t);
                }
                r
            }
            other => rt(format!("Invalid workspace '{other}'. The workspace must be 'base' or 'caller'.")),
        }
    }

    /// Runs `f` and returns the text it would have displayed (for `evalc`); figures are
    /// still shown.
    pub(crate) fn capture_text<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<(T, String)> {
        let flush = self.flush.take();
        let start = self.pending.len();
        let r = f(self);
        let produced: Vec<Output> = self.pending.drain(start..).collect();
        self.flush = flush;
        let mut text = String::new();
        for o in produced {
            match o.kind {
                OutputKind::Text(t) => text.push_str(&t),
                OutputKind::Value { text: t, .. } => {
                    text.push_str(&t);
                    text.push('\n');
                }
                OutputKind::Warning(w) => text.push_str(&format!("Warning: {w}\n")),
                other => self.pending.push(Output { line: o.line, kind: other }),
            }
        }
        Ok((r?, text))
    }

    /// The function running in the innermost function frame, for `mfilename` (None in a
    /// script or the command line).
    pub(crate) fn current_function(&self) -> Option<String> {
        self.frames.iter().rev().find_map(|f| f.func.clone())
    }

    /// Whether a class method or constructor is running.
    pub(crate) fn in_method(&self) -> bool {
        !self.class_stack.is_empty()
    }

    /// The user functions and classes loaded so far (`inmem`), sorted.
    pub(crate) fn loaded_names(&self) -> (Vec<String>, Vec<String>) {
        let mut f: Vec<String> = self.functions.keys().chain(self.path_functions.keys()).cloned().collect();
        f.sort();
        f.dedup();
        let mut c: Vec<String> = self.classes.keys().cloned().collect();
        c.sort();
        (f, c)
    }

    /// The local functions of the file whose code is running (`localfunctions`).
    pub(crate) fn local_function_names(&self) -> Vec<String> {
        match self.current_function() {
            Some(cur) => match self.local_groups.iter().rev().find(|(_, names)| names.contains(&cur)) {
                Some((true, names)) => names[1..].to_vec(),
                Some((false, names)) => names.clone(),
                None => Vec::new(),
            },
            None => self.local_groups.iter().rev().find(|(main, _)| !main).map(|(_, n)| n.clone()).unwrap_or_default(),
        }
    }

    /// `run(name)`: a script found on the search path (or at a path) run in the current
    /// workspace.
    pub(crate) fn run_script(&mut self, name: &str) -> R<()> {
        let src = match self.read_path_file(name) {
            Some((_, src)) => src,
            None => {
                let p = if name.ends_with(".m") { name.to_string() } else { format!("{name}.m") };
                std::fs::read_to_string(&p).map_err(|_| LangError::new(format!("RUN cannot execute the file '{name}'. RUN requires a valid MATLAB script"), 0))?
            }
        };
        self.eval_text(&src, 0).map(|_| ())
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
        match self.frames.last_mut() {
            Some(f) => f.line = s.line,
            None => self.top_line = s.line,
        }
        self.check_cancel()?;
        let r = self.exec_inner(s);
        r.map_err(|mut e| {
            if e.line == 0 && self.frames.is_empty() {
                e.line = s.line;
            }
            if !e.captured {
                e.captured = true;
                e.stack = self.error_stack();
                let drop = e.drop_frames.min(e.stack.len());
                e.stack.drain(..drop);
            }
            e
        })
    }

    /// The call stack, innermost first: each function frame with its line, then the script.
    fn error_stack(&self) -> Vec<crate::error::ErrFrame> {
        let mut out = Vec::new();
        for f in self.frames.iter().rev() {
            // The wrappers of MException's own methods (throw, rethrow, ...) are not frames.
            if f.func.as_deref().is_some_and(|n| !self.code_sources.contains_key(n) && crate::builtins_validate::ME_METHODS.contains(&n)) {
                continue;
            }
            if let Some(name) = &f.func {
                let file = self.code_sources.get(name).map(|c| c.path.clone()).unwrap_or_default();
                out.push(crate::error::ErrFrame { name: name.clone(), file, line: f.line });
            }
        }
        if !self.script_name.is_empty() {
            out.push(crate::error::ErrFrame { name: self.script_name.clone(), file: self.script_path.clone(), line: self.top_line });
        }
        out
    }

    /// How a frame is named in a report: `file>function` for a local function, else the name.
    fn frame_label(&self, f: &crate::error::ErrFrame) -> String {
        match self.code_sources.get(&f.name) {
            Some(c) if !c.stem.is_empty() && c.stem != f.name => format!("{}>{}", c.stem, f.name),
            _ => f.name.clone(),
        }
    }

    /// The source text of a frame's line.
    fn frame_source(&self, f: &crate::error::ErrFrame) -> Option<String> {
        let lines = if f.name == self.script_name && !self.script_name.is_empty() {
            self.script_lines.clone()
        } else {
            self.code_sources.get(&f.name)?.lines.clone()
        };
        lines.get(f.line.checked_sub(1)?).map(|l| l.trim_end().to_string())
    }

    /// `getReport` of an error: "Error using f" for one raised by `error`/`throw`, the
    /// message, then an "Error in f (line n)" block with the source line for each caller.
    pub(crate) fn error_report(&self, message: &str, stack: &[crate::error::ErrFrame], raised: bool) -> String {
        let mut out = String::new();
        let mut frames = stack;
        if raised {
            if let Some(first) = stack.first() {
                out.push_str(&format!("Error using {}\n", self.frame_label(first)));
                frames = &stack[1..];
            }
        }
        out.push_str(message);
        for f in frames {
            out.push_str(&format!("\n\nError in {} (line {})", self.frame_label(f), f.line));
            if let Some(src) = self.frame_source(f) {
                out.push('\n');
                out.push_str(&src);
            }
        }
        out
    }

    /// Saves settings' personal values (and the groups and settings added) between sessions,
    /// next to the preferences file, as MATLAB does.
    pub fn enable_persistent_settings(&mut self) {
        let file = crate::io_more::pref_path().with_file_name("matlivresettings.mat");
        self.settings.persist_to(file);
    }

    /// Persists settings to `file` instead of the default next to the preferences.
    pub fn set_settings_file(&mut self, file: std::path::PathBuf) {
        self.settings.persist_to(file);
    }

    /// Names the script being run, for error stacks (`Error in name (line n)`).
    pub fn set_script(&mut self, name: &str, path: &str) {
        self.script_name = name.to_string();
        self.script_path = path.to_string();
    }

    /// Emits the display of `name = v`, honoring `disp`/`display` overloads of objects.
    pub(crate) fn show(&mut self, name: &str, v: &Value) -> R<()> {
        if crate::gfx::is_gfx(v) {
            let text = crate::gfx::display(self, name, v)?;
            self.emit(OutputKind::Value { name: name.to_string(), text });
            return Ok(());
        }
        if let Some(text) = crate::containers::display(name, v, self.format) {
            self.emit(OutputKind::Value { name: name.to_string(), text });
            return Ok(());
        }
        if let Value::Object(o) = v {
            if let Some(t) = crate::settings::node_text(self, o) {
                self.emit(OutputKind::Value { name: name.to_string(), text: format!("{name} = \n{t}") });
                return Ok(());
            }
            if let Some(t) = crate::decomp::object_text(o).or_else(|| crate::ode_objects::object_text(o)) {
                self.emit(OutputKind::Value { name: name.to_string(), text: format!("{name} = 
{t}") });
                return Ok(());
            }
            if let Some(t) = crate::tiff::object_text(o) {
                self.emit(OutputKind::Value { name: name.to_string(), text: format!("{name} = 

{t}") });
                return Ok(());
            }
            if let Some(t) = crate::func_objects::object_text(name, o, Some(crate::random::global_id(self))) {
                self.emit(OutputKind::Value { name: name.to_string(), text: format!("{name} = \n{t}") });
                return Ok(());
            }
            if let Some(f) = o.class.method("display").cloned() {
                self.call_method(&f, &o.class.clone(), vec![v.clone()], 0)?;
                return Ok(());
            }
            if let Some(f) = o.class.method("disp").cloned() {
                self.emit(OutputKind::Text(format!("{name} = \n")));
                self.call_method(&f, &o.class.clone(), vec![v.clone()], 0)?;
                return Ok(());
            }
            if let Some(m) = crate::class::enum_name(o) {
                let short = o.class.name.rsplit('.').next().unwrap_or(&o.class.name);
                let text = format!("{name} = 
  {short} enumeration

    {m}");
                self.emit(OutputKind::Value { name: name.to_string(), text });
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
                // A comma-separated list (c{:}, s.f of a struct array) shows every element as ans;
                // a function call shows only its first output.
                let list = matches!(expr, Expr::CellIndex(..) | Expr::Field(..) | Expr::DynField(..));
                let take = if list { vals.len() } else { 1 };
                for v in vals.into_iter().take(take) {
                    if *print {
                        self.show("ans", &v)?;
                    }
                    self.set_var("ans", v);
                }
            }
            StmtKind::Assign { lhs, rhs, print } => {
                if lhs.len() == 1 && self.try_append_in_place(&lhs[0], rhs)? {
                    // x = [x ...] grew x without copying it.
                } else if lhs.len() == 1 {
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
            StmtKind::For { var, iter, body } => {
                if self.try_jit(s)? {
                    return Ok(Flow::Normal);
                }
                return self.exec_for(var, iter, body);
            }
            StmtKind::While { cond, body } => {
                if self.try_jit(s)? {
                    return Ok(Flow::Normal);
                }
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
                    let id = if e.identifier.is_empty() { crate::builtins_validate::builtin_identifier(&e.message).to_string() } else { e.identifier.clone() };
                    self.last_error = (e.message.clone(), id.clone());
                    if let Some(name) = catch_var {
                        let report = self.error_report(&e.message, &e.stack, e.raised);
                        let me = crate::builtins_validate::mexception_caught(&e.message, &id, &e.stack, e.raised, &report);
                        self.set_var(name, me);
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
                if n == 0 {
                    // MATLAB leaves the loop variable empty (1×0) when the loop never runs.
                    self.set_var(var, Value::Num(Matrix::new(1, 0, Vec::new())));
                }
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
        // f(x, y) = expr with symbolic variables as subscripts defines a symbolic function.
        if let [Accessor::Paren(args)] = chain.as_slice() {
            let vars: Option<Vec<String>> = args
                .iter()
                .map(|a| match a {
                    Expr::Ident(n) => self.get_var(n).and_then(crate::sym::bare_var),
                    _ => None,
                })
                .collect();
            if let Some(vars) = vars.filter(|v| !v.is_empty()) {
                let f = crate::sym::define_symfun(vars, &rhs)?;
                self.set_var(name, f);
                return Ok(());
            }
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
                Some(target) => match {
                    if let Value::Num(m) = target {
                        m.csc.clear();
                    }
                    assign_in_place(target, &resolved, rhs)?
                } {
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
                return (v.dims(), matches!(v, Value::Table(_)) || crate::containers::is_keyed(v));
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
                Some(h @ Value::Object(_)) if crate::gfx::is_gfx(&h) => {
                    let sub = if rest.is_empty() { None } else { Some(crate::gfx::member(self, &h, f)?) };
                    let new = self.apply_assign(sub, rest, rhs)?;
                    crate::gfx::set_member(self, &h, f, new)?;
                    Ok(h)
                }
                Some(h @ Value::Object(_)) if matches!(&h, Value::Object(o) if crate::settings::is_node(&o.class.name)) => {
                    let sub = if rest.is_empty() { None } else { Some(crate::settings::member(self, &h, f)?) };
                    let new = self.apply_assign(sub, rest, rhs)?;
                    crate::settings::set_member(self, &h, f, new)?;
                    Ok(h)
                }
                Some(Value::Object(o)) => {
                    // A MatFile takes new variables as well as its existing ones.
                    if o.class.name == "matlab.io.MatFile" && f != "Properties" && o.class.prop_index(f).is_none() {
                        let new = self.apply_assign(None, rest, rhs)?;
                        return crate::io_more::matfile_assign(&o, f, new);
                    }
                    if !o.has_prop(f) {
                        return rt(format!("Unrecognized property '{f}' for class '{}'.", o.class.name));
                    }
                    let sub = o.get(f);
                    let new = self.apply_assign(sub, rest, rhs)?;
                    let new = if o.class.name == "timer" { crate::timers::check_prop(self, f, new)? } else { new };
                    // A set.Prop method validates or converts the value (not inside itself).
                    let key = (o.class.name.clone(), f.to_string());
                    if let Some(setter) = o.class.method(&format!("set.{f}")).cloned() {
                        if !self.active_setters.contains(&key) {
                            let class = o.class.clone();
                            self.active_setters.push(key);
                            let r = self.call_method(&setter, &class, vec![Value::Object(o.clone()), new], 1);
                            self.active_setters.pop();
                            let out = r?;
                            return Ok(if class.is_handle { Value::Object(o) } else { out.into_iter().next().unwrap_or(Value::Object(o)) });
                        }
                    }
                    if o.class.name == "matlab.io.MatFile" && f != "Properties" {
                        return crate::io_more::matfile_assign(&o, f, new);
                    }
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
                    if cur.as_ref().map(crate::containers::is_categorical).unwrap_or(false) {
                        return crate::containers::cat_assign(cur.unwrap(), idx, rhs);
                    }
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
            Resolved::TableParen(args) if cur.as_ref().map(crate::containers::is_keyed).unwrap_or(false) => {
                if !rest.is_empty() {
                    return rt("Only one level of indexing is supported in assignments to a map.");
                }
                crate::containers::keyed_assign(cur.unwrap(), args, rhs)
            }
            Resolved::TableBrace(_) if cur.as_ref().map(crate::containers::is_keyed).unwrap_or(false) => {
                rt("Brace indexing is not supported for variables of this type.")
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
        // Chains of elementwise operators on large arrays are evaluated in cache-sized blocks.
        if matches!(e, Expr::Binary(..) | Expr::Unary(..) | Expr::Index(..)) {
            if let Some(v) = crate::fuse::try_eval(self, e) {
                return Ok(v);
            }
        }
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
                if let Some(v) = self.binary_by_ref(*op, a, b)? {
                    return Ok(v);
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
    /// `a op b` where an operand is a large numeric variable: reads it in place instead of
    /// copying it. Reading a variable has no side effects, so evaluating the other operand
    /// first is unobservable.
    fn binary_by_ref(&mut self, op: BinOp, a: &Expr, b: &Expr) -> R<Option<Value>> {
        let big = |s: &Self, e: &Expr| match e {
            Expr::Ident(n) => matches!(s.get_var(n), Some(Value::Num(m)) if m.numel() >= 64),
            _ => false,
        };
        let (ba, bb) = (big(self, a), big(self, b));
        if !ba && !bb {
            return Ok(None);
        }
        let name = |e: &Expr| if let Expr::Ident(n) = e { n.clone() } else { unreachable!() };
        let (l, r) = match (ba, bb) {
            (true, true) => (None, None),
            (true, false) => (None, Some(self.eval(b)?)),
            _ => (Some(self.eval(a)?), None),
        };
        if matches!(l, Some(Value::Object(_))) || matches!(r, Some(Value::Object(_))) {
            let l = match l { Some(v) => v, None => self.eval(a)? };
            let r = match r { Some(v) => v, None => self.eval(b)? };
            return self.binary(op, l, r).map(Some);
        }
        let (na, nb) = (if ba { Some(name(a)) } else { None }, if bb { Some(name(b)) } else { None });
        let lv = match (&l, &na) {
            (Some(v), _) => v,
            (None, Some(n)) => match self.get_var(n) { Some(v) => v, None => return rt(format!("Unrecognized function or variable '{n}'.")) },
            _ => unreachable!(),
        };
        let rv = match (&r, &nb) {
            (Some(v), _) => v,
            (None, Some(n)) => match self.get_var(n) { Some(v) => v, None => return rt(format!("Unrecognized function or variable '{n}'.")) },
            _ => unreachable!(),
        };
        let out = ops::binary(op, lv, rv);
        self.flush_op_warnings(op);
        out.map(Some)
    }

    /// Prints the warnings an operator raised (only the matrix divisions raise any).
    fn flush_op_warnings(&mut self, op: BinOp) {
        if matches!(op, BinOp::MatLeftDiv | BinOp::MatDiv | BinOp::MatPow) {
            for w in ops::take_warnings() {
                crate::builtins_funfun::warn(self, w);
            }
        }
    }

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
            if matches!(op, BinOp::Eq | BinOp::Ne) {
                if let Some(v) = crate::gfx::compare(op == BinOp::Eq, &l, &r) {
                    return Ok(v);
                }
                if let Some(v) = crate::lang_more::enum_compare(op, &l, &r)? {
                    return Ok(v);
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
        let out = ops::binary(op, &l, &r);
        self.flush_op_warnings(op);
        out
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
                // Calling a symbolic function, f(3) or f(a, b).
                if let Expr::Ident(name) = base.as_ref() {
                    if matches!(self.get_var(name), Some(Value::Sym(s)) if !s.args.is_empty()) {
                        let argv = self.eval_args(args)?;
                        let Some(Value::Sym(f)) = self.get_var(name) else { unreachable!() };
                        let f = f.clone();
                        return Ok(vec![crate::sym::call_symfun(&f, argv)?]);
                    }
                }
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
                        if name == "table" || crate::table_ops::wants_arg_names(name) {
                            self.arg_names = args.iter().map(|a| if let Expr::Ident(n) = a { Some(n.clone()) } else { None }).collect();
                        }
                        if let Some(r) = self.call_lending(name, args, nargout)? {
                            return Ok(r);
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
                        if let Some(full) = self.package_function(base) {
                            let argv = self.eval_args(args)?;
                            return self.call_function(&full, argv, nargout);
                        }
                        // pkg.Class(args) constructs; pkg.Class.method(args) calls a static method.
                        if let Some(c) = self.dotted_class(base) {
                            let argv = self.eval_args(args)?;
                            return Ok(vec![self.construct(&c, argv)?]);
                        }
                        if let Some(c) = self.dotted_class(obj_expr) {
                            let argv = self.eval_args(args)?;
                            return self.static_call(&c, member, argv, nargout);
                        }
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
                if let Some(full) = self.package_function(e) {
                    return self.call_function(&full, Vec::new(), nargout);
                }
                if let Some(c) = self.dotted_class(base) {
                    return self.static_call(&c, f, Vec::new(), nargout);
                }
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

    /// `pkg.name` naming a builtin (`containers.Map`) when `pkg` is not a variable or class.
    /// A class named with its package (`matlab.ode.SolverID`).
    fn dotted_class(&mut self, e: &Expr) -> Option<Rc<ClassDef>> {
        fn parts(e: &Expr, out: &mut Vec<String>) -> bool {
            match e {
                Expr::Ident(n) => {
                    out.push(n.clone());
                    true
                }
                Expr::Field(b, f) => {
                    let ok = parts(b, out);
                    out.push(f.clone());
                    ok
                }
                _ => false,
            }
        }
        let mut p = Vec::new();
        if !parts(e, &mut p) || p.len() < 2 || self.get_var(&p[0]).is_some() {
            return None;
        }
        let full = p.join(".");
        crate::lang_more::builtin_class(&full).or_else(|| self.classes.get(&full).cloned())
    }

    fn package_function(&self, e: &Expr) -> Option<String> {
        fn parts(e: &Expr, out: &mut Vec<String>) -> bool {
            match e {
                Expr::Ident(n) => {
                    out.push(n.clone());
                    true
                }
                Expr::Field(b, f) => {
                    let ok = parts(b, out);
                    out.push(f.clone());
                    ok
                }
                _ => false,
            }
        }
        let mut p = Vec::new();
        if !parts(e, &mut p) || p.len() < 2 || self.get_var(&p[0]).is_some() {
            return None;
        }
        let full = p.join(".");
        crate::builtins::exists(&full).then_some(full)
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
            Value::Object(o) if crate::gfx::is_gfx_class(&o.class.name) => {
                let idx = self.eval_index_args(args, &bv.dims())?;
                Ok(vec![crate::gfx::index(self, &bv, &idx)?])
            }
            Value::Object(o) if o.class.name == "categorical" => {
                let idx = self.eval_index_args(args, &bv.dims())?;
                Ok(vec![crate::containers::cat_index(&bv, &idx)?])
            }
            Value::Object(o) if crate::containers::is_container(&o.class.name) => {
                let vals = self.eval_raw_args(args, &[1, 1])?;
                Ok(vec![crate::containers::lookup(&bv, &vals)?])
            }
            // Calling a System object runs step: obj(x).
            Value::Object(o) if o.class.is_a(crate::sysobj::CLASS) && o.class.method("subsref").is_none() => {
                let vals = self.eval_args(args)?;
                crate::sysobj::step(self, &bv, &vals, nargout)
            }
            Value::Object(o) => {
                // An overloaded subsref receives S = struct('type', '()', 'subs', {args}).
                if let Some(f) = o.class.method("subsref").cloned() {
                    let mut s = StructArray::scalar();
                    s.set("type", Value::chars("()"));
                    s.set("subs", Value::Cell(CellArray::row(self.eval_args(args)?)));
                    let class = o.class.clone();
                    return self.call_method(&f, &class, vec![bv.clone(), Value::Struct(s)], nargout);
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
            Value::Object(o) if crate::gfx::is_gfx_class(&o.class.name) => crate::gfx::member(self, v, name),
            Value::Object(o) if crate::settings::is_node(&o.class.name) => crate::settings::member(self, v, name),
            Value::Object(o) if o.class.name == crate::decomp::RF_CLASS => crate::decomp::rowfilter_member(o, name),
            Value::Object(o) if crate::sym::units::is_symunit(&o.class.name) => crate::sym::units::member(name),
            Value::Object(o) => {
                let Some(i) = o.class.prop_index(name) else {
                    if let Some(v) = o.get(name) {
                        return Ok(v);
                    }
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
        self.frames.push(Frame { vars: HashMap::new(), globals: HashSet::new(), func: None, line: 0 });
        let r = self.eval(e);
        self.frames.pop();
        r
    }

    /// Function-call arguments with comma-list expansion of `c{:}` and `s.f`.
    /// Calls a pure numeric builtin with large local variables passed by reference: the variables
    /// are moved into the argument list for the call and put back afterwards, so `nnz(A)` or
    /// `pcg(A, b)` does not copy `A`. Returns `None` (nothing evaluated, nothing moved) when the
    /// call does not qualify.
    fn call_lending(&mut self, name: &str, args: &[Expr], nargout: usize) -> R<Option<Vec<Value>>> {
        /// Builtins that only read their arguments and never look variables up by name.
        const LEND: &[&str] = &[
            "issparse", "nnz", "size", "numel", "length", "ndims", "isempty", "isreal", "isnumeric", "class", "norm", "pcg", "gmres", "bicg", "bicgstab", "bicgstabl",
            "cgs", "minres", "lsqr", "qmr", "tfqmr", "symmlq", "sum", "prod", "max", "min", "mean", "cumsum", "find", "any", "all", "abs", "full", "sparse", "nonzeros",
            "diag", "trace", "det", "inv", "rank", "eig", "svd", "chol", "lu", "qr", "sort", "unique", "diff", "std", "var", "median", "dot", "kron", "transpose", "numnz",
            "spones", "tril", "triu", "isfinite", "isnan", "floor", "round", "sqrt", "exp", "log",
        ];
        if !LEND.contains(&name) || self.functions.contains_key(name) || !crate::builtins::exists(name) {
            return Ok(None);
        }
        let mut lent: Vec<(usize, &str)> = Vec::new();
        for (k, a) in args.iter().enumerate() {
            match a {
                Expr::CellIndex(..) | Expr::Field(..) | Expr::Colon => return Ok(None),
                Expr::Ident(n) if !self.frame().globals.contains(n.as_str()) => {
                    if matches!(self.frame().vars.get(n.as_str()), Some(Value::Num(m)) if m.numel() >= 4096) {
                        if lent.iter().any(|(_, o)| o == n) {
                            return Ok(None);
                        }
                        lent.push((k, n.as_str()));
                    }
                }
                _ => {}
            }
        }
        if lent.is_empty() {
            return Ok(None);
        }
        // Evaluate the other arguments first (they may read the lent variables), in order.
        let mut slots: Vec<Option<Value>> = Vec::with_capacity(args.len());
        for (k, a) in args.iter().enumerate() {
            slots.push(if lent.iter().any(|(p, _)| *p == k) { None } else { Some(self.eval(a)?) });
        }
        if slots.iter().flatten().any(|v| matches!(v, Value::Object(_))) {
            // An object argument may dispatch to a method: take the ordinary path with copies.
            let mut argv = Vec::with_capacity(slots.len());
            for (k, s) in slots.into_iter().enumerate() {
                argv.push(match s {
                    Some(v) => v,
                    None => match lent.iter().find(|(p, _)| *p == k) {
                        Some((_, n)) => self.get_var(n).cloned().unwrap_or_else(Value::empty),
                        None => Value::empty(),
                    },
                });
            }
            return self.call_function(name, argv, nargout).map(Some);
        }
        for (k, n) in &lent {
            slots[*k] = self.frame_mut().vars.remove(*n);
        }
        let argv: Vec<Value> = slots.into_iter().map(|s| s.unwrap_or_else(Value::empty)).collect();
        let res = crate::builtins::call_ref(self, name, &argv, nargout);
        let mut argv = argv;
        for (k, n) in &lent {
            let v = std::mem::replace(&mut argv[*k], Value::empty());
            self.frame_mut().vars.insert((*n).to_string(), v);
        }
        match res? {
            Some(r) => Ok(Some(r)),
            None => rt(format!("Unrecognized function or variable '{name}'.")),
        }
    }

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

    /// `x = [x a b ...]` with `x` a real row vector (or 0x0) and every appended item a real
    /// row of the same class: extends `x` in place. Returns false when the general path
    /// must run (nothing has been evaluated then, or only side-effect-free items).
    fn try_append_in_place(&mut self, lhs: &LValue, rhs: &Expr) -> R<bool> {
        let LValue::Target { name, chain } = lhs else { return Ok(false) };
        if !chain.is_empty() {
            return Ok(false);
        }
        let Expr::Matrix(rows) = rhs else { return Ok(false) };
        if rows.len() != 1 || rows[0].len() < 2 || !matches!(&rows[0][0], Expr::Ident(n) if n == name) {
            return Ok(false);
        }
        let kind = match self.get_var(name) {
            Some(Value::Num(m)) if m.im.is_none() && m.dims.len() == 2 && (m.dims[0] == 1 || m.numel() == 0 && m.dims == [0, 0]) => m.kind,
            _ => return Ok(false),
        };
        let items = self.eval_list(&rows[0][1..])?;
        let ok = items.iter().all(|v| matches!(v, Value::Num(m) if m.kind == kind && m.im.is_none() && m.dims.len() == 2 && m.dims[0] == 1));
        let Some(Value::Num(x)) = self.get_var_mut(name) else { return rt(format!("Unrecognized function or variable '{name}'.")) };
        if !ok || x.kind != kind {
            // Fall back on the general concatenation with the values already evaluated.
            let mut all = vec![Value::Num(x.clone())];
            all.extend(items);
            let v = indexing::hconcat(all)?;
            self.assign(lhs, v)?;
            return Ok(true);
        }
        for v in &items {
            if let Value::Num(m) = v {
                x.data.extend_from_slice(&m.data);
            }
        }
        x.dims = vec![1, x.data.len()];
        Ok(true)
    }

    fn eval_matrix(&mut self, rows: &[Vec<Expr>]) -> R<Value> {
        let mut row_vals = Vec::with_capacity(rows.len());
        for row in rows {
            let items = self.eval_list(row)?;
            if items.iter().any(crate::containers::is_categorical) {
                row_vals.push(crate::containers::concat(items, 2)?);
            } else {
                row_vals.push(indexing::hconcat(items)?);
            }
        }
        if row_vals.iter().any(crate::containers::is_categorical) {
            return crate::containers::concat(row_vals, 1);
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
                let has_varargin = f.params.last().is_some_and(|p| p == "varargin");
                let fixed = f.params.len() - usize::from(has_varargin);
                if args.len() > fixed && !has_varargin {
                    return rt("Too many input arguments.");
                }
                self.enter()?;
                let mut vars: HashMap<String, Value> = captured.iter().cloned().collect();
                let mut args = args;
                let rest = if args.len() > fixed { args.split_off(fixed) } else { Vec::new() };
                for (p, a) in f.params.iter().zip(args) {
                    vars.insert(p.clone(), a);
                }
                if has_varargin {
                    vars.insert("varargin".into(), Value::Cell(CellArray::row(rest)));
                }
                self.frames.push(Frame { vars, globals: HashSet::new(), func: None, line: 0 });
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

    /// An enumeration member: the constructor run with the member's arguments.
    pub(crate) fn enum_member(&mut self, c: &Rc<ClassDef>, member: &str) -> R<Value> {
        let Some((_, args)) = c.enums.iter().find(|(m, _)| m == member).cloned() else {
            return rt(format!("'{member}' is not a member of enumeration class '{}'.", c.name));
        };
        let argv = match args {
            Some(e) => match self.eval_in_class_scope(&e)? {
                Value::Cell(cc) => cc.data,
                v => vec![v],
            },
            None => Vec::new(),
        };
        let obj = self.construct(c, argv)?;
        let Value::Object(o) = obj else { return rt("Enumeration constructor did not return an object.") };
        Ok(Value::Object(o.set(crate::class::ENUM_PROP, Value::chars(member))?))
    }

    fn static_call(&mut self, c: &Rc<ClassDef>, member: &str, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        if c.enums.iter().any(|(m, _)| m == member) {
            return Ok(vec![self.enum_member(c, member)?]);
        }
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
    /// An object of class `c` with its default property values, without running a
    /// constructor (as `load` creates objects).
    pub(crate) fn default_object(&mut self, c: &Rc<ClassDef>) -> R<Object> {
        let mut props = Vec::with_capacity(c.props.len());
        for p in &c.props {
            props.push(match &p.default {
                Some(d) if !p.dependent => self.eval_in_class_scope(d)?,
                _ => Value::empty(),
            });
        }
        Ok(Object { class: c.clone(), props: Rc::new(RefCell::new(props)) })
    }

    /// Calls a static method of a class.
    pub(crate) fn call_static(&mut self, c: &Rc<ClassDef>, member: &str, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        self.static_call(c, member, args, nargout)
    }

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
        if nargout <= 1 && f.args.is_empty() {
            if let Some(r) = self.try_jit_call(f, &args)? {
                return Ok(vec![r]);
            }
        }
        self.call_user_with(f, args, nargout, None)
    }

    fn call_user_with(&mut self, f: &Rc<Function>, args: Vec<Value>, nargout: usize, preset: Option<(String, Value)>) -> R<Vec<Value>> {
        let mut args = args;
        // With an arguments block, the name-value structure's parameter takes the trailing
        // Name,Value pairs, which are bound after the positional arguments.
        let nv_param = f.params.last().filter(|p| f.args.iter().any(|d| &d.name == *p && d.field.is_some())).cloned();
        let mut nv_args = Vec::new();
        if nv_param.is_some() {
            let npos = f.params.len() - 1;
            if args.len() > npos {
                nv_args = args.split_off(npos);
            }
        }
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
        let nargin = nargin + nv_args.len();
        vars.insert("nargin".into(), Value::num(nargin as f64));
        vars.insert("nargout".into(), Value::num(nargout as f64));
        self.frames.push(Frame { vars, globals: HashSet::new(), func: Some(f.name.clone()), line: 0 });
        let r = if f.args.is_empty() { Ok(()) } else { self.check_arguments(f, nv_param.as_deref(), nargin - nv_args.len(), nv_args) };
        let r = match r {
            Ok(()) => self.exec_block(&f.body),
            Err(e) => Err(e),
        };
        let frame = self.frames.pop().unwrap();
        self.depth -= 1;
        if let Err(e) = r {
            for v in frame.vars.values() {
                crate::builtins_validate::run_cleanup(self, v);
            }
            return Err(e);
        }
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
        // onCleanup objects that were not returned run their task as the workspace ends.
        for v in frame.vars.values() {
            crate::builtins_validate::run_cleanup(self, v);
        }
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

// ---- native compilation ------------------------------------------------------------------------

impl Interpreter {
    /// How a call of `name` resolves, for the compiler (mirrors `call_function`).
    fn jit_callee(&mut self, name: &str) -> Option<crate::jit::Callee> {
        use crate::jit::{Callee, INTRINSICS};
        if let Some(f) = self.functions.get(name) {
            return Some(Callee::User(f.clone()));
        }
        if crate::builtins::exists(name) {
            return if INTRINSICS.contains(&name) { Some(Callee::Builtin) } else { None };
        }
        if self.find_class(name).is_some() {
            return None;
        }
        self.find_path_function(name).map(Callee::User)
    }

    /// Environment for compiling code that references `names`: variable types (when
    /// `with_vars`), and how other names resolve as functions, following user functions
    /// transitively. The string identifies the environment for caching.
    fn jit_env(&mut self, names: &[String], with_vars: bool) -> (crate::jit::Env, String) {
        use crate::jit::{self as j, Callee, Env};
        let mut env = Env { vars: HashMap::new(), funcs: HashMap::new() };
        let mut sig = String::new();
        let mut queue: Vec<String> = Vec::new();
        for n in names {
            match self.get_var(n).filter(|_| with_vars) {
                Some(v) => {
                    let t = j::var_type(v);
                    sig.push_str(&format!("{n}:{t:?};"));
                    env.vars.insert(n.clone(), t);
                }
                None => queue.push(n.clone()),
            }
        }
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(n) = queue.pop() {
            if !seen.insert(n.clone()) {
                continue;
            }
            if let Some(c) = self.jit_callee(&n) {
                if let Callee::User(f) = &c {
                    sig.push_str(&format!("{n}@{:p};", Rc::as_ptr(f)));
                    let mut inner = Vec::new();
                    j::referenced_names(&f.body, &mut inner);
                    for m in inner {
                        if !f.params.contains(&m) && !f.outputs.contains(&m) {
                            queue.push(m);
                        }
                    }
                }
                env.funcs.insert(n, c);
            }
        }
        (env, sig)
    }

    /// Calls user function `f` as native code when every argument is a real double scalar
    /// and it compiles. Returns None to interpret.
    /// Applies a function's `arguments` declarations in its fresh workspace: defaults of
    /// missing inputs, the name-value structure, class conversion, size and validators.
    fn check_arguments(&mut self, f: &Function, nv_param: Option<&str>, npos: usize, nv: Vec<Value>) -> R<()> {
        let positional: Vec<&String> = f.params.iter().filter(|p| Some(p.as_str()) != nv_param && p.as_str() != "varargin").collect();
        let decl = |p: &str| f.args.iter().find(|d| d.name == p && d.field.is_none());
        let required = positional.iter().rposition(|p| decl(p).is_some_and(|d| d.default.is_none())).map_or(0, |i| i + 1);
        if npos < required {
            return rt(format!("Invalid argument list. Function requires {} more input(s).", required - npos));
        }
        for (i, p) in positional.iter().enumerate() {
            let Some(d) = decl(p) else { continue };
            let v = if i < npos {
                self.get_var(p).cloned().unwrap_or_else(Value::empty)
            } else {
                match &d.default {
                    Some(e) => self.eval(e)?,
                    None => continue,
                }
            };
            let v = self.validate_arg(d, v, &format!("Invalid argument at position {}. ", i + 1))?;
            self.set_var(p, v);
        }
        let Some(nvp) = nv_param else { return Ok(()) };
        let fields: Vec<&crate::ast::ArgDecl> = f.args.iter().filter(|d| d.name == nvp && d.field.is_some()).collect();
        if nv.len() % 2 == 1 {
            return rt(format!("Invalid argument at position {}. Make sure name-value arguments include both the name and the value.", npos + nv.len()));
        }
        let field_name = |d: &crate::ast::ArgDecl| d.field.clone().unwrap_or_default();
        let mut given: Vec<Option<Value>> = vec![None; fields.len()];
        for (k, pair) in nv.chunks(2).enumerate() {
            let Ok(name) = pair[0].text() else {
                return rt(format!("Invalid argument at position {}. Function requires {} positional input(s).", npos + 2 * k + 1, npos));
            };
            let hit = fields.iter().position(|d| field_name(d).eq_ignore_ascii_case(&name)).or_else(|| {
                let low = name.to_lowercase();
                let partial: Vec<usize> = fields.iter().enumerate().filter(|(_, d)| !low.is_empty() && field_name(d).to_lowercase().starts_with(&low)).map(|(i, _)| i).collect();
                if partial.len() == 1 { Some(partial[0]) } else { None }
            });
            let Some(j) = hit else {
                let names: Vec<String> = fields.iter().map(|d| format!("'{}'", field_name(d))).collect();
                let hint = if names.len() == 1 { format!("Use {}.", names[0]) } else { format!("Use one of these names:\n    {}", names.join("\n    ")) };
                return rt(format!("Invalid argument name '{name}'. {hint}"));
            };
            given[j] = Some(pair[1].clone());
        }
        let mut s = StructArray::scalar();
        for (d, g) in fields.iter().zip(given) {
            let field = field_name(d);
            let v = match g {
                Some(v) => v,
                None => match &d.default {
                    Some(e) => self.eval(e)?,
                    None => continue,
                },
            };
            let v = self.validate_arg(d, v, &format!("Invalid value for '{field}' argument. "))?;
            s.set(&field, v);
        }
        self.set_var(nvp, Value::Struct(s));
        Ok(())
    }

    /// One declared argument's class conversion, size check (with scalar expansion) and
    /// validators; errors start with `prefix`.
    fn validate_arg(&mut self, d: &crate::ast::ArgDecl, v: Value, prefix: &str) -> R<Value> {
        let mut v = v;
        if let Some(cls) = &d.class {
            let is = match &v {
                Value::Object(o) => o.class.is_a(cls),
                _ => &v.class_name() == cls,
            };
            if !is {
                let convertible = !matches!(cls.as_str(), "cell" | "struct" | "function_handle" | "table");
                let converted = if convertible { self.call_function(cls, vec![v.clone()], 1).ok().and_then(|r| r.into_iter().next()) } else { None };
                match converted {
                    Some(c) => v = c,
                    None if convertible => return rt(format!("{prefix}Value must be of type {cls} or be convertible to {cls}.")),
                    None => return rt(format!("{prefix}Value must be of type {cls}.")),
                }
            }
        }
        if let Some(spec) = &d.dims {
            let mut vd = v.dims();
            while vd.len() < spec.len() {
                vd.push(1);
            }
            let fits = vd.len() == spec.len() && spec.iter().zip(&vd).all(|(s, &n)| s.is_none_or(|s| s == n));
            // A vector takes the declared orientation: a column for `(1,:)` becomes a row.
            let n: usize = vd.iter().product();
            let is_vector = vd.iter().filter(|&&k| k != 1).count() <= 1;
            let vec_spec = spec.len() == 2 && (spec[0] == Some(1) || spec[1] == Some(1));
            if !fits && n > 1 && is_vector && vec_spec {
                let along = if spec[0] == Some(1) { spec[1] } else { spec[0] };
                if along.is_none_or(|k| k == n) {
                    let shape = if spec[0] == Some(1) { [1.0, n as f64] } else { [n as f64, 1.0] };
                    let a = vec![v.clone(), Value::num(shape[0]), Value::num(shape[1])];
                    v = self.call_function("reshape", a, 1)?.remove(0);
                    vd = v.dims();
                }
            }
            let fits = vd.len() == spec.len() && spec.iter().zip(&vd).all(|(s, &n)| s.is_none_or(|s| s == n));
            if !fits {
                if vd.iter().product::<usize>() == 1 {
                    // A scalar expands to the declared size (`:` taken as 1).
                    let mut a = vec![v.clone()];
                    a.extend(spec.iter().map(|s| Value::num(s.unwrap_or(1) as f64)));
                    v = self.call_function("repmat", a, 1)?.remove(0);
                } else {
                    return rt(format!("{prefix}{}", size_message(spec)));
                }
            }
        }
        for e in &d.validators {
            let r = match e {
                Expr::Ident(n) if self.get_var(n).is_none() => self.call_function(n, vec![v.clone()], 0).map(|_| ()),
                _ => {
                    // The validator sees the converted value under the argument's name.
                    if d.field.is_none() {
                        self.set_var(&d.name, v.clone());
                    }
                    self.eval_multi(e, 0).map(|_| ())
                }
            };
            if let Err(err) = r {
                return Err(LangError { message: format!("{prefix}{}", err.message), ..err });
            }
        }
        Ok(v)
    }

    fn try_jit_call(&mut self, f: &Rc<Function>, args: &[Value]) -> R<Option<Value>> {
        use crate::jit::{self as j, Ctx, Runtime, VarType};
        if !self.jit_enabled || args.len() != f.params.len() || f.outputs.len() != 1 {
            return Ok(None);
        }
        let mut xs = Vec::with_capacity(args.len());
        for a in args {
            if j::var_type(a) != VarType::F64 {
                return Ok(None);
            }
            xs.push(a.scalar().unwrap_or(0.0));
        }
        let key = (Rc::as_ptr(f) as usize, args.len());
        let entry = match self.jit_entries.get(&key) {
            Some(e) => *e,
            None => {
                let mut names = Vec::new();
                j::referenced_names(&f.body, &mut names);
                names.retain(|m| !f.params.contains(m) && !f.outputs.contains(m));
                let (env, _) = self.jit_env(&names, false);
                let jit = self.jit.get_or_insert_with(j::Jit::new);
                let e = jit.compile_entry(f, args.len(), &env).ok();
                self.jit_entries.insert(key, e);
                e
            }
        };
        let Some(entry) = entry else { return Ok(None) };
        let rng = crate::random::global_stream(&mut self.rng);
        let mut rt = Runtime { store: Vec::new(), error: None, rng };
        let cancel = match &self.cancel {
            Some(c) => c.as_ptr() as *const u8,
            None => &NO_CANCEL as *const u8,
        };
        let mut ctx = Ctx { frame: std::ptr::null_mut(), slots: std::ptr::null_mut(), rt: &mut rt, cancel, depth: self.depth as i64, status: 0 };
        let mut out = 0.0f64;
        let status = entry(&mut ctx, xs.as_ptr(), &mut out);
        self.jit_runs += 1;
        if status != 0 {
            let (msg, line) = rt.error.take().unwrap_or_else(|| ("Internal error in compiled code.".into(), 0));
            // A value native code cannot represent: interpret the call (it has no side effects yet).
            if msg == j::FALLBACK {
                self.jit_entries.insert(key, None);
                return Ok(None);
            }
            return Err(LangError::new(msg, line));
        }
        Ok(Some(Value::num(out)))
    }

    /// Runs a loop statement as native code when it compiles. Returns false to interpret.
    fn try_jit(&mut self, s: &Stmt) -> R<bool> {
        use crate::jit::{self as j, Ctx, Runtime, ScalarHome, Slot, Storage};
        if !self.jit_enabled {
            return Ok(false);
        }
        let key = s as *const Stmt as usize;
        let names = match self.jit_names.get(&key) {
            Some(n) => n.clone(),
            None => {
                let mut n = Vec::new();
                j::referenced_names(std::slice::from_ref(s), &mut n);
                self.jit_names.insert(key, n.clone());
                n
            }
        };
        let (env, sig) = self.jit_env(&names, true);
        let cache_key = (key, sig);
        let kernel = match self.jit_cache.get(&cache_key) {
            Some(k) => k.clone(),
            None => {
                let jit = self.jit.get_or_insert_with(j::Jit::new);
                let k = jit.compile_kernel(s, &env).ok().map(Rc::new);
                self.jit_cache.insert(cache_key.clone(), k.clone());
                k
            }
        };
        let Some(k) = kernel else { return Ok(false) };
        // A kernel that can hand the statement back keeps the arrays it changes, to restore them.
        let may_fall_back = k.falls_back;
        let snapshot: Vec<(String, Value)> = if may_fall_back { k.slots.iter().filter_map(|n| self.get_var(n).map(|v| (n.clone(), v.clone()))).collect() } else { Vec::new() };
        let rng_state = (may_fall_back && names.iter().any(|n| n == "rand")).then(|| crate::random::save_global(&mut self.rng));

        // Marshal state in.
        let n = k.scalars.len();
        let mut frame = vec![0.0f64; 3 * n];
        for (i, ks) in k.scalars.iter().enumerate() {
            if !ks.at_entry {
                continue;
            }
            let v = match &ks.home {
                ScalarHome::Var(name) => self.get_var(name),
                ScalarHome::Field(st, f) => match self.get_var(st) {
                    Some(Value::Struct(sa)) => sa.get(f),
                    _ => None,
                },
            };
            frame[i] = v.and_then(|v| v.scalar().ok()).unwrap_or(0.0);
        }
        let mut store = Vec::with_capacity(k.slots.len());
        for name in &k.slots {
            store.push(match self.take_var(name) {
                Some(Value::Num(m)) => Storage::Mat(m),
                Some(Value::Cell(c)) => Storage::Cell(c),
                _ => unreachable!("kernel slot types are checked at compile time"),
            });
        }
        let rng = crate::random::global_stream(&mut self.rng);
        let mut rt = Runtime { store, error: None, rng };
        let mut slots: Vec<Slot> = rt.store.iter_mut().map(|s| match s {
            Storage::Mat(m) => Slot { data: m.data.as_mut_ptr(), len: m.numel() as i64, rows: m.rows() as i64, cols: m.cols() as i64 },
            Storage::Cell(c) => Slot { data: std::ptr::null_mut(), len: c.data.len() as i64, rows: c.rows() as i64, cols: c.cols() as i64 },
        }).collect();
        let cancel = match &self.cancel {
            Some(c) => c.as_ptr() as *const u8,
            None => &NO_CANCEL as *const u8,
        };
        let mut ctx = Ctx { frame: frame.as_mut_ptr(), slots: slots.as_mut_ptr(), rt: &mut rt, cancel, depth: 0, status: 0 };
        let status = (k.ptr)(&mut ctx);
        self.jit_runs += 1;
        if status != 0 && rt.error.as_ref().is_some_and(|(m, _)| m == j::FALLBACK) {
            // Put the workspace back as it was and let the interpreter run the statement.
            drop(std::mem::take(&mut rt.store));
            for (name, v) in snapshot {
                self.set_var(&name, v);
            }
            if let Some(st) = rng_state {
                crate::random::restore_global(&mut self.rng, st);
            }
            self.jit_cache.insert(cache_key, None);
            return Ok(false);
        }

        // Marshal state out (also after an error, as the interpreter would leave it).
        for (name, st) in k.slots.iter().zip(std::mem::take(&mut rt.store)) {
            self.set_var(name, match st {
                Storage::Mat(m) => Value::Num(m),
                Storage::Cell(c) => Value::Cell(c),
            });
        }
        for (i, ks) in k.scalars.iter().enumerate() {
            let assigned = frame[n + i] != 0.0;
            let empty = frame[2 * n + i] != 0.0;
            let v = if empty {
                Value::Num(Matrix::new(1, 0, Vec::new()))
            } else if ks.at_entry || assigned {
                if ks.ty_bool { Value::bool(frame[i] != 0.0) } else { Value::num(frame[i]) }
            } else {
                continue;
            };
            match &ks.home {
                ScalarHome::Var(name) => self.set_var(name, v),
                ScalarHome::Field(st, f) => {
                    if let Some(Value::Struct(sa)) = self.frame_mut().vars.get_mut(st.as_str()) {
                        sa.set(f, v);
                    }
                }
            }
        }
        if status != 0 {
            let (msg, line) = rt.error.take().unwrap_or_else(|| ("Internal error in compiled code.".into(), 0));
            if line > 0 && self.frames.is_empty() {
                self.top_line = line;
            }
            return Err(LangError::new(msg, line));
        }
        Ok(true)
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

/// MATLAB's message for a value whose size does not match a declared `(dims)`.
fn size_message(spec: &[Option<usize>]) -> String {
    match spec {
        [Some(1), Some(1)] => "Value must be a scalar.".into(),
        [Some(1), Some(n)] | [Some(n), Some(1)] => format!("Value must be a vector with {n} elements."),
        [Some(1), None] => "Value must be a row vector.".into(),
        [None, Some(1)] => "Value must be a column vector.".into(),
        [None, Some(n)] => format!("Value must be a matrix with {n} columns."),
        [Some(n), None] => format!("Value must be a matrix with {n} rows."),
        _ => format!("Value must be a matrix of size {}.", spec.iter().map(|s| s.map_or(":".to_string(), |n| n.to_string())).collect::<Vec<_>>().join("-by-")),
    }
}
