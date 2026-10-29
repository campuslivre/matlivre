//! Native compilation of hot loops and scalar functions with Cranelift.
//!
//! A `for`/`while` statement is compiled when everything it touches has a fixed machine
//! representation: real double/logical scalars, real double 2-D arrays (read, written, grown),
//! cells that are only read (all-scalar) or only written, and scalar fields of scalar structs.
//! User functions reached from such loops are compiled when their parameters and locals are
//! scalars. Operations whose MATLAB result could leave the real line (`sqrt`, `log`,
//! non-integer powers) are never compiled, so compiled code cannot diverge from the
//! interpreter; errors raise the interpreter's messages after writing variables back.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, AbiParam, Block, FuncRef, InstBuilder, MemFlagsData, SigRef, Signature, UserFuncName, Value as CVal};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, FuncId, Linkage, Module};

use crate::ast::*;
use crate::nd::{self, IndexArg};
use crate::value::{CellArray, Kind, Matrix, Value};

// ---- runtime ABI ------------------------------------------------------------------------------

#[repr(C)]
pub struct Ctx {
    pub frame: *mut f64,
    pub slots: *mut Slot,
    pub rt: *mut Runtime,
    pub cancel: *const u8,
    pub depth: i64,
    pub status: i64,
}

const CTX_FRAME: i32 = 0;
const CTX_SLOTS: i32 = 8;
const CTX_CANCEL: i32 = 24;
const CTX_DEPTH: i32 = 32;
const CTX_STATUS: i32 = 40;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Slot {
    pub data: *mut f64,
    pub len: i64,
    pub rows: i64,
    pub cols: i64,
}

const SLOT_SIZE: i64 = 32;
const SLOT_DATA: i32 = 0;
const SLOT_LEN: i32 = 8;
const SLOT_ROWS: i32 = 16;
const SLOT_COLS: i32 = 24;

pub enum Storage {
    Mat(Matrix),
    Cell(CellArray),
}

pub struct Runtime {
    pub store: Vec<Storage>,
    pub error: Option<(String, usize)>,
}

pub const MAX_DEPTH: i64 = 256;

impl Slot {
    fn of(s: &mut Storage) -> Slot {
        match s {
            Storage::Mat(m) => Slot { data: m.data.as_mut_ptr(), len: m.numel() as i64, rows: m.rows() as i64, cols: m.cols() as i64 },
            Storage::Cell(c) => Slot { data: std::ptr::null_mut(), len: c.data.len() as i64, rows: c.rows() as i64, cols: c.cols() as i64 },
        }
    }
}

unsafe fn rt<'a>(ctx: *mut Ctx) -> &'a mut Runtime {
    &mut *(*ctx).rt
}

unsafe fn sync(ctx: *mut Ctx, k: usize) {
    let s = Slot::of(&mut rt(ctx).store[k]);
    *(*ctx).slots.add(k) = s;
}

fn set_err(ctx: *mut Ctx, msg: String, line: i64) -> i32 {
    unsafe {
        let r = rt(ctx);
        if r.error.is_none() {
            r.error = Some((msg, line.max(0) as usize));
        }
    }
    1
}

extern "C" fn h_err(ctx: *mut Ctx, code: i64, a: f64, b: f64, line: i64) -> i32 {
    let msg = match code {
        1 => "Array indices must be positive integers or logical values.".to_string(),
        2 => "Index in position 1 is invalid. Array indices must be positive integers or logical values.".to_string(),
        3 => format!("Index exceeds the number of array elements. Index must not exceed {}.", b as i64),
        4 => format!("Index in position {} exceeds array bounds. Index must not exceed {}.", a as i64, b as i64),
        5 => format!("Maximum recursion limit of {MAX_DEPTH} reached."),
        6 => "Execution stopped by user.".to_string(),
        7 => "NaN's cannot be converted to logicals.".to_string(),
        _ => "Internal error in compiled code.".to_string(),
    };
    set_err(ctx, msg, line)
}

extern "C" fn h_set_line(ctx: *mut Ctx, line: i64) {
    unsafe {
        if let Some((_, l)) = rt(ctx).error.as_mut() {
            if *l == 0 {
                *l = line.max(0) as usize;
            }
        }
    }
}

fn index_arg(x: f64) -> IndexArg {
    IndexArg::scalar(x as usize - 1)
}

extern "C" fn h_arr_set1(ctx: *mut Ctx, k: i64, i: f64, v: f64, line: i64) -> i32 {
    unsafe {
        let r = rt(ctx);
        let Storage::Mat(m) = &mut r.store[k as usize] else { return set_err(ctx, "internal: slot".into(), line) };
        // Appending one element to a vector (x(end+1) = v) is amortized O(1).
        let n = m.data.len();
        if i as usize == n + 1 && m.dims.len() == 2 {
            let row = m.dims[0] == 1 || (n == 0 && m.dims[1] == 0 && m.dims[0] == 0);
            let col = m.dims[1] == 1 && m.dims[0] == n && n > 1;
            if row || col {
                m.data.push(v);
                if row {
                    m.dims[0] = 1;
                    m.dims[1] = n + 1;
                } else {
                    m.dims[0] = n + 1;
                }
                sync(ctx, k as usize);
                return 0;
            }
        }
        let mut dims = m.dims.clone();
        if let Err(e) = nd::assign(&mut dims, &mut m.data, &[index_arg(i)], &[1, 1], vec![v], 0.0) {
            return set_err(ctx, e.message, line);
        }
        m.dims = dims;
        sync(ctx, k as usize);
    }
    0
}

extern "C" fn h_arr_set2(ctx: *mut Ctx, k: i64, i: f64, j: f64, v: f64, line: i64) -> i32 {
    unsafe {
        let r = rt(ctx);
        let Storage::Mat(m) = &mut r.store[k as usize] else { return set_err(ctx, "internal: slot".into(), line) };
        let mut dims = m.dims.clone();
        if let Err(e) = nd::assign(&mut dims, &mut m.data, &[index_arg(i), index_arg(j)], &[1, 1], vec![v], 0.0) {
            return set_err(ctx, e.message, line);
        }
        m.dims = dims;
        sync(ctx, k as usize);
    }
    0
}

extern "C" fn h_cell_set(ctx: *mut Ctx, k: i64, i: f64, v: f64, is_bool: i64, line: i64) -> i32 {
    unsafe {
        let r = rt(ctx);
        let Storage::Cell(c) = &mut r.store[k as usize] else { return set_err(ctx, "internal: slot".into(), line) };
        let val = if is_bool != 0 { Value::bool(v != 0.0) } else { Value::num(v) };
        // In-bounds writes replace the element directly.
        let p = i as usize - 1;
        if p < c.data.len() {
            c.data[p] = val;
            return 0;
        }
        let mut dims = c.dims.clone();
        if let Err(e) = nd::assign(&mut dims, &mut c.data, &[index_arg(i)], &[1, 1], vec![val], Value::empty()) {
            return set_err(ctx, e.message, line);
        }
        c.dims = dims;
        sync(ctx, k as usize);
    }
    0
}

/// Reads a scalar cell element; sets `ctx.status` on error.
extern "C" fn h_cell_get(ctx: *mut Ctx, k: i64, i: f64, line: i64) -> f64 {
    unsafe {
        let r = rt(ctx);
        let Storage::Cell(c) = &r.store[k as usize] else { return 0.0 };
        let p = i as usize - 1;
        if p >= c.data.len() {
            (*ctx).status = h_err(ctx, 3, 0.0, c.data.len() as f64, line) as i64;
            return 0.0;
        }
        match &c.data[p] {
            Value::Num(m) if m.numel() == 1 => m.data[0],
            _ => {
                (*ctx).status = set_err(ctx, "internal: non-scalar cell element in compiled code".into(), line) as i64;
                0.0
            }
        }
    }
}

extern "C" fn h_range_count(a: f64, s: f64, b: f64) -> i64 {
    crate::interp::range_count(a, s, b) as i64
}

extern "C" fn h_sin(x: f64) -> f64 { x.sin() }
extern "C" fn h_cos(x: f64) -> f64 { x.cos() }
extern "C" fn h_tan(x: f64) -> f64 { x.tan() }
extern "C" fn h_exp(x: f64) -> f64 { x.exp() }
extern "C" fn h_atan(x: f64) -> f64 { x.atan() }
extern "C" fn h_sinh(x: f64) -> f64 { x.sinh() }
extern "C" fn h_cosh(x: f64) -> f64 { x.cosh() }
extern "C" fn h_tanh(x: f64) -> f64 { x.tanh() }
extern "C" fn h_round(x: f64) -> f64 { x.round() }
extern "C" fn h_pow(x: f64, y: f64) -> f64 { x.powf(y) }
extern "C" fn h_atan2(x: f64, y: f64) -> f64 { x.atan2(y) }
extern "C" fn h_hypot(x: f64, y: f64) -> f64 { x.hypot(y) }

// ---- compile-time environment -----------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum VarType {
    F64,
    Bool,
    Array,
    Cell { all_scalar: bool },
    Struct(Vec<(String, Option<Kind>)>),
    Other,
}

/// Classifies a workspace value for compilation.
pub fn var_type(v: &Value) -> VarType {
    match v {
        Value::Num(m) if m.im.is_none() && m.is_2d() && m.kind == Kind::Double && m.numel() == 1 => VarType::F64,
        Value::Num(m) if m.im.is_none() && m.numel() == 1 && m.kind == Kind::Logical => VarType::Bool,
        Value::Num(m) if m.im.is_none() && m.is_2d() && m.kind == Kind::Double => VarType::Array,
        Value::Cell(c) if c.dims.len() == 2 => VarType::Cell {
            all_scalar: c.data.iter().all(|x| matches!(x, Value::Num(m) if m.numel() == 1 && m.kind == Kind::Double && m.im.is_none())),
        },
        Value::Struct(s) if s.elems.len() == 1 => VarType::Struct(
            s.fields
                .iter()
                .zip(&s.elems[0])
                .map(|(f, x)| {
                    let k = match x {
                        Value::Num(m) if m.im.is_none() && m.numel() == 1 && matches!(m.kind, Kind::Double | Kind::Logical) => Some(m.kind),
                        _ => None,
                    };
                    (f.clone(), k)
                })
                .collect(),
        ),
        _ => VarType::Other,
    }
}

#[derive(Clone)]
pub enum Callee {
    User(Rc<Function>),
    Builtin,
}

pub struct Env {
    pub vars: HashMap<String, VarType>,
    pub funcs: HashMap<String, Callee>,
}

/// Names a statement refers to (variables or functions), including names inside user
/// functions it may call is left to the driver.
pub fn referenced_names(stmts: &[Stmt], out: &mut Vec<String>) {
    fn expr(e: &Expr, out: &mut Vec<String>) {
        match e {
            Expr::Ident(n) => push(out, n),
            Expr::Unary(_, a) | Expr::Field(a, _) => expr(a, out),
            Expr::Binary(_, a, b) => {
                expr(a, out);
                expr(b, out);
            }
            Expr::Range(a, s, b) => {
                expr(a, out);
                if let Some(s) = s {
                    expr(s, out);
                }
                expr(b, out);
            }
            Expr::Index(b, args) | Expr::CellIndex(b, args) => {
                expr(b, out);
                args.iter().for_each(|a| expr(a, out));
            }
            Expr::DynField(a, b) => {
                expr(a, out);
                expr(b, out);
            }
            Expr::Matrix(r) | Expr::Cell(r) => r.iter().flatten().for_each(|x| expr(x, out)),
            _ => {}
        }
    }
    fn push(out: &mut Vec<String>, n: &str) {
        if !out.iter().any(|x| x == n) {
            out.push(n.to_string());
        }
    }
    for s in stmts {
        match &s.kind {
            StmtKind::Expr { expr: e, .. } => expr(e, out),
            StmtKind::Assign { lhs, rhs, .. } => {
                for l in lhs {
                    if let LValue::Target { name, chain } = l {
                        push(out, name);
                        for a in chain {
                            match a {
                                Accessor::Paren(x) | Accessor::Brace(x) => x.iter().for_each(|e| expr(e, out)),
                                Accessor::DynField(e) => expr(e, out),
                                Accessor::Field(_) => {}
                            }
                        }
                    }
                }
                expr(rhs, out);
            }
            StmtKind::If { clauses, otherwise } => {
                for (c, b) in clauses {
                    expr(c, out);
                    referenced_names(b, out);
                }
                if let Some(b) = otherwise {
                    referenced_names(b, out);
                }
            }
            StmtKind::For { var, iter, body } => {
                push(out, var);
                expr(iter, out);
                referenced_names(body, out);
            }
            StmtKind::While { cond, body } => {
                expr(cond, out);
                referenced_names(body, out);
            }
            StmtKind::Switch { subject, cases, otherwise } => {
                expr(subject, out);
                for (c, b) in cases {
                    expr(c, out);
                    referenced_names(b, out);
                }
                if let Some(b) = otherwise {
                    referenced_names(b, out);
                }
            }
            StmtKind::Try { body, catch_body, .. } => {
                referenced_names(body, out);
                referenced_names(catch_body, out);
            }
            StmtKind::Command { name, .. } => push(out, name),
            _ => {}
        }
    }
}

/// Builtins the compiler implements natively.
pub const INTRINSICS: &[&str] = &[
    "abs", "floor", "ceil", "fix", "round", "sin", "cos", "tan", "exp", "atan", "sinh", "cosh", "tanh", "mod", "rem", "min", "max", "atan2", "hypot",
    "sign", "isnan", "isinf", "isfinite", "pi", "Inf", "inf", "NaN", "nan", "eps", "true", "false", "numel", "length", "size",
];

// ---- compiler -------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ty {
    F64,
    Bool,
}

#[derive(Clone, Copy)]
enum V {
    F(CVal),
    B(CVal),
}

/// Compilation failure: the statement runs in the interpreter instead.
#[derive(Debug)]
pub struct Reject(pub String);

type CR<T> = Result<T, Reject>;

fn reject<T>(why: impl Into<String>) -> CR<T> {
    Err(Reject(why.into()))
}

struct ScalarVar {
    var: Variable,
    ty: Ty,
    assigned: Option<Variable>,
    empty: Option<Variable>,
}

/// Where a kernel scalar comes from and goes back to.
#[derive(Debug, Clone)]
pub enum ScalarHome {
    Var(String),
    Field(String, String),
}

#[derive(Debug, Clone)]
pub struct KernelScalar {
    pub home: ScalarHome,
    pub ty_bool: bool,
    pub at_entry: bool,
}

pub struct Kernel {
    pub ptr: extern "C" fn(*mut Ctx) -> i32,
    pub scalars: Vec<KernelScalar>,
    /// Workspace variables passed by slot (arrays and cells), in slot order.
    pub slots: Vec<String>,
}

struct Helpers {
    err: SigRef,
    set_line: SigRef,
    arr_set1: SigRef,
    arr_set2: SigRef,
    cell_set: SigRef,
    cell_get: SigRef,
    range_count: SigRef,
    f1: SigRef,
    f2: SigRef,
}

pub struct Jit {
    module: JITModule,
    fctx: FunctionBuilderContext,
    /// Compiled scalar user functions: (function address, arity) → id.
    funcs: HashMap<(usize, usize), FuncId>,
    next_name: u32,
    retired: Vec<JITModule>,
}

fn make_module() -> JITModule {
    let mut flags = settings::builder();
    flags.set("use_colocated_libcalls", "false").unwrap();
    flags.set("is_pic", "false").unwrap();
    flags.set("opt_level", "speed").unwrap();
    let isa = cranelift_native::builder().expect("host not supported").finish(settings::Flags::new(flags)).expect("isa");
    JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()))
}

impl Default for Jit {
    fn default() -> Self {
        Self::new()
    }
}

struct Gen<'a, 'b> {
    b: FunctionBuilder<'a>,
    module: &'b mut JITModule,
    funcs: &'b HashMap<(usize, usize), FuncId>,
    env_funcs: &'b HashMap<String, Callee>,
    h: Helpers,
    ctx: CVal,
    scalars: HashMap<String, ScalarVar>,
    slots: HashMap<String, (usize, bool /*is cell*/)>,
    fields: HashMap<(String, String), String>,
    defined: HashSet<String>,
    maybe_empty: HashSet<String>,
    loops: Vec<(Block, Block)>,
    exit: Block,
    status: Variable,
    line: i64,
    is_fn: bool,
    order: Vec<String>,
    frame_ptr: Option<CVal>,
    func_refs: HashMap<FuncId, FuncRef>,
    end_ctx: Vec<CVal>,
    /// Interned memory flags (trusted: aligned, non-trapping).
    tf: MemFlagsData,
    plain: MemFlagsData,
}

impl<'a, 'b> Gen<'a, 'b> {
    fn fconst(&mut self, x: f64) -> CVal {
        self.b.ins().f64const(x)
    }

    fn to_f(&mut self, v: V) -> CVal {
        match v {
            V::F(x) => x,
            V::B(x) => {
                let one = self.fconst(1.0);
                let zero = self.fconst(0.0);
                self.b.ins().select(x, one, zero)
            }
        }
    }

    /// `if`/`while` truth (nonzero and not NaN).
    fn truth(&mut self, v: V) -> CVal {
        match v {
            V::B(x) => x,
            V::F(x) => {
                let z = self.fconst(0.0);
                self.b.ins().fcmp(FloatCC::OrderedNotEqual, x, z)
            }
        }
    }

    /// Error exit with a coded message.
    fn fail(&mut self, code: i64, a: CVal, bv: CVal) {
        let codev = self.b.ins().iconst(types::I64, code);
        let line = self.b.ins().iconst(types::I64, self.line);
        let f = self.b.ins().iconst(types::I64, h_err as *const () as i64);
        let call = self.b.ins().call_indirect(self.h.err, f, &[self.ctx, codev, a, bv, line]);
        let st = self.b.inst_results(call)[0];
        self.b.def_var(self.status, st);
        self.b.ins().jump(self.exit, &[]);
    }

    /// Branches to an error exit when `cond` is true.
    fn fail_if(&mut self, cond: CVal, code: i64, a: CVal, bv: CVal) {
        let bad = self.b.create_block();
        let ok = self.b.create_block();
        self.b.ins().brif(cond, bad, &[], ok, &[]);
        self.b.switch_to_block(bad);
        self.b.set_cold_block(bad);
        self.fail(code, a, bv);
        self.b.switch_to_block(ok);
    }

    /// Exit with the status already in ctx/err when `st` (i32) is nonzero.
    fn propagate(&mut self, st: CVal) {
        let bad = self.b.create_block();
        let ok = self.b.create_block();
        self.b.ins().brif(st, bad, &[], ok, &[]);
        self.b.switch_to_block(bad);
        self.b.set_cold_block(bad);
        let line = self.b.ins().iconst(types::I64, self.line);
        let f = self.b.ins().iconst(types::I64, h_set_line as *const () as i64);
        self.b.ins().call_indirect(self.h.set_line, f, &[self.ctx, line]);
        let one = self.b.ins().iconst(types::I32, 1);
        self.b.def_var(self.status, one);
        self.b.ins().jump(self.exit, &[]);
        self.b.switch_to_block(ok);
    }

    fn call1(&mut self, f: extern "C" fn(f64) -> f64, x: CVal) -> CVal {
        let p = self.b.ins().iconst(types::I64, f as *const () as i64);
        let c = self.b.ins().call_indirect(self.h.f1, p, &[x]);
        self.b.inst_results(c)[0]
    }

    fn call2(&mut self, f: extern "C" fn(f64, f64) -> f64, x: CVal, y: CVal) -> CVal {
        let p = self.b.ins().iconst(types::I64, f as *const () as i64);
        let c = self.b.ins().call_indirect(self.h.f2, p, &[x, y]);
        self.b.inst_results(c)[0]
    }

    fn slot_addr(&mut self, k: usize) -> CVal {
        let slots = self.b.ins().load(types::I64, self.tf, self.ctx, CTX_SLOTS);
        self.b.ins().iadd_imm_s(slots, k as i64 * SLOT_SIZE)
    }

    fn slot_field(&mut self, k: usize, off: i32) -> CVal {
        let a = self.slot_addr(k);
        self.b.ins().load(types::I64, self.tf, a, off)
    }

    /// Validates a 1-based index value; returns the zero-based i64.
    fn index0(&mut self, v: CVal) -> CVal {
        let k = self.b.ins().fcvt_to_sint_sat(types::I64, v);
        let kf = self.b.ins().fcvt_from_sint(types::F64, k);
        let nonint = self.b.ins().fcmp(FloatCC::NotEqual, kf, v);
        let zero = self.fconst(0.0);
        self.fail_if(nonint, 1, v, zero);
        let lt1 = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, k, 1);
        self.fail_if(lt1, 2, v, zero);
        self.b.ins().iadd_imm_s(k, -1)
    }

    // ---- expressions ----------------------------------------------------------------------

    fn expr(&mut self, e: &Expr) -> CR<V> {
        match e {
            Expr::Num(n) => Ok(V::F(self.fconst(*n))),
            Expr::Ident(n) => {
                if let Some(s) = self.scalars.get(n) {
                    if !self.defined.contains(n) {
                        return reject(format!("'{n}' may be used before it is assigned"));
                    }
                    if self.maybe_empty.contains(n) {
                        return reject(format!("'{n}' may be empty after a loop"));
                    }
                    let (var, ty) = (s.var, s.ty);
                    let v = self.b.use_var(var);
                    return Ok(if ty == Ty::Bool { V::B(v) } else { V::F(v) });
                }
                if self.slots.contains_key(n) {
                    return reject(format!("array '{n}' used as a whole"));
                }
                self.call(n, &[])
            }
            Expr::End => match self.end_ctx.last() {
                Some(v) => Ok(V::F(*v)),
                None => reject("'end' outside an index"),
            },
            Expr::Field(b, f) => {
                if let Expr::Ident(s) = b.as_ref() {
                    if let Some(name) = self.fields.get(&(s.clone(), f.clone())).cloned() {
                        return self.expr(&Expr::Ident(name));
                    }
                }
                reject("unsupported field access")
            }
            Expr::Unary(op, x) => {
                let v = self.expr(x)?;
                Ok(match op {
                    UnOp::Neg => {
                        let f = self.to_f(v);
                        V::F(self.b.ins().fneg(f))
                    }
                    UnOp::Plus => V::F(self.to_f(v)),
                    UnOp::Not => match v {
                        V::B(b) => {
                            let one = self.b.ins().iconst(types::I8, 1);
                            V::B(self.b.ins().bxor(b, one))
                        }
                        V::F(f) => {
                            let nan = self.b.ins().fcmp(FloatCC::Unordered, f, f);
                            let z = self.fconst(0.0);
                            self.fail_if(nan, 7, z, z);
                            V::B(self.b.ins().fcmp(FloatCC::Equal, f, z))
                        }
                    },
                    UnOp::Transpose | UnOp::CTranspose => v,
                })
            }
            Expr::Binary(op, a, b) => self.binary(*op, a, b),
            Expr::Index(base, args) => {
                let Expr::Ident(name) = base.as_ref() else { return reject("complex index base") };
                if let Some(&(k, false)) = self.slots.get(name) {
                    return self.array_read(k, args);
                }
                if self.scalars.contains_key(name) {
                    return reject(format!("indexing scalar '{name}'"));
                }
                self.call(name, args)
            }
            Expr::CellIndex(base, args) => {
                let Expr::Ident(name) = base.as_ref() else { return reject("complex cell base") };
                let Some(&(k, true)) = self.slots.get(name) else { return reject("brace on non-cell") };
                if args.len() != 1 {
                    return reject("multi-index cell read");
                }
                let len = self.slot_field(k, SLOT_LEN);
                let lenf = self.b.ins().fcvt_from_sint(types::F64, len);
                self.end_ctx.push(lenf);
                let iv = self.expr(&args[0]);
                self.end_ctx.pop();
                let iv = self.to_f(iv?);
                self.index0(iv);
                let kk = self.b.ins().iconst(types::I64, k as i64);
                let line = self.b.ins().iconst(types::I64, self.line);
                let f = self.b.ins().iconst(types::I64, h_cell_get as *const () as i64);
                let c = self.b.ins().call_indirect(self.h.cell_get, f, &[self.ctx, kk, iv, line]);
                let v = self.b.inst_results(c)[0];
                let st = self.b.ins().load(types::I64, self.tf, self.ctx, CTX_STATUS);
                let st32 = self.b.ins().ireduce(types::I32, st);
                self.propagate(st32);
                Ok(V::F(v))
            }
            _ => reject("unsupported expression"),
        }
    }

    fn binary(&mut self, op: BinOp, a: &Expr, b: &Expr) -> CR<V> {
        use BinOp::*;
        if matches!(op, AndAnd | OrOr) {
            // Short circuit; operands use scalar truth (x ~= 0).
            let result = self.b.declare_var(types::I8);
            let av = self.expr(a)?;
            let at = self.sc_truth(av);
            self.b.def_var(result, at);
            let rhs_block = self.b.create_block();
            let done = self.b.create_block();
            if op == AndAnd {
                self.b.ins().brif(at, rhs_block, &[], done, &[]);
            } else {
                self.b.ins().brif(at, done, &[], rhs_block, &[]);
            }
            self.b.switch_to_block(rhs_block);
            let bv = self.expr(b)?;
            let bt = self.sc_truth(bv);
            self.b.def_var(result, bt);
            self.b.ins().jump(done, &[]);
            self.b.switch_to_block(done);
            return Ok(V::B(self.b.use_var(result)));
        }
        // x^2 and integer literal powers.
        if matches!(op, Pow | MatPow) {
            let lit = match b {
                Expr::Num(n) => Some(*n),
                Expr::Unary(UnOp::Neg, x) => match x.as_ref() {
                    Expr::Num(n) => Some(-*n),
                    _ => None,
                },
                _ => None,
            };
            let Some(p) = lit.filter(|p| p.fract() == 0.0) else { return reject("non-integer or variable exponent") };
            let x = self.expr(a)?;
            let x = self.to_f(x);
            if p == 2.0 {
                return Ok(V::F(self.b.ins().fmul(x, x)));
            }
            let pv = self.fconst(p);
            return Ok(V::F(self.call2(h_pow, x, pv)));
        }
        let av = self.expr(a)?;
        let bv = self.expr(b)?;
        match op {
            Eq | Ne | Lt | Le | Gt | Ge => {
                let (x, y) = (self.to_f(av), self.to_f(bv));
                let cc = match op {
                    Eq => FloatCC::Equal,
                    Ne => FloatCC::NotEqual,
                    Lt => FloatCC::LessThan,
                    Le => FloatCC::LessThanOrEqual,
                    Gt => FloatCC::GreaterThan,
                    _ => FloatCC::GreaterThanOrEqual,
                };
                Ok(V::B(self.b.ins().fcmp(cc, x, y)))
            }
            And | Or => {
                let x = self.logical(av);
                let y = self.logical(bv);
                Ok(V::B(if op == And { self.b.ins().band(x, y) } else { self.b.ins().bor(x, y) }))
            }
            _ => {
                let (x, y) = (self.to_f(av), self.to_f(bv));
                Ok(V::F(match op {
                    Add => self.b.ins().fadd(x, y),
                    Sub => self.b.ins().fsub(x, y),
                    Mul | MatMul => self.b.ins().fmul(x, y),
                    Div | MatDiv => self.b.ins().fdiv(x, y),
                    LeftDiv | MatLeftDiv => self.b.ins().fdiv(y, x),
                    _ => return reject("unsupported operator"),
                }))
            }
        }
    }

    /// Truth for `&&`/`||` operands (the interpreter's scalar truth: x ~= 0).
    fn sc_truth(&mut self, v: V) -> CVal {
        match v {
            V::B(b) => b,
            V::F(f) => {
                let z = self.fconst(0.0);
                self.b.ins().fcmp(FloatCC::NotEqual, f, z)
            }
        }
    }

    /// Operand of `&`/`|`: NaN is an error, else x ~= 0.
    fn logical(&mut self, v: V) -> CVal {
        match v {
            V::B(b) => b,
            V::F(f) => {
                let nan = self.b.ins().fcmp(FloatCC::Unordered, f, f);
                let z = self.fconst(0.0);
                self.fail_if(nan, 7, z, z);
                self.b.ins().fcmp(FloatCC::NotEqual, f, z)
            }
        }
    }

    fn array_read(&mut self, k: usize, args: &[Expr]) -> CR<V> {
        match args.len() {
            1 => {
                let len = self.slot_field(k, SLOT_LEN);
                let lenf = self.b.ins().fcvt_from_sint(types::F64, len);
                self.end_ctx.push(lenf);
                let iv = self.expr(&args[0]);
                self.end_ctx.pop();
                let iv = self.to_f(iv?);
                let i0 = self.index0(iv);
                let len = self.slot_field(k, SLOT_LEN);
                let oob = self.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, i0, len);
                let lenf = self.b.ins().fcvt_from_sint(types::F64, len);
                let z = self.fconst(0.0);
                self.fail_if(oob, 3, z, lenf);
                let data = self.slot_field(k, SLOT_DATA);
                let off = self.b.ins().ishl_imm_s(i0, 3);
                let addr = self.b.ins().iadd(data, off);
                Ok(V::F(self.b.ins().load(types::F64, self.tf, addr, 0)))
            }
            2 => {
                let (addr, _, _) = self.addr2(k, args, true)?;
                Ok(V::F(self.b.ins().load(types::F64, self.tf, addr.unwrap(), 0)))
            }
            _ => reject("N-D indexing"),
        }
    }

    /// Address of element (i, j); with `check`, out-of-bounds is an error, otherwise returns
    /// `None` address plus the in-bounds flag and raw subscripts for the slow path.
    fn addr2(&mut self, k: usize, args: &[Expr], check: bool) -> CR<(Option<CVal>, CVal, (CVal, CVal, CVal, CVal))> {
        let rows = self.slot_field(k, SLOT_ROWS);
        let cols = self.slot_field(k, SLOT_COLS);
        let rf = self.b.ins().fcvt_from_sint(types::F64, rows);
        let cf = self.b.ins().fcvt_from_sint(types::F64, cols);
        self.end_ctx.push(rf);
        let iv = self.expr(&args[0]);
        self.end_ctx.pop();
        let iv = self.to_f(iv?);
        self.end_ctx.push(cf);
        let jv = self.expr(&args[1]);
        self.end_ctx.pop();
        let jv = self.to_f(jv?);
        let i0 = self.index0(iv);
        let j0 = self.index0(jv);
        let rows = self.slot_field(k, SLOT_ROWS);
        let cols = self.slot_field(k, SLOT_COLS);
        let ri = self.b.ins().icmp(IntCC::UnsignedLessThan, i0, rows);
        let ci = self.b.ins().icmp(IntCC::UnsignedLessThan, j0, cols);
        let inb = self.b.ins().band(ri, ci);
        if check {
            let one = self.fconst(1.0);
            let two = self.fconst(2.0);
            let rf = self.b.ins().fcvt_from_sint(types::F64, rows);
            let cf = self.b.ins().fcvt_from_sint(types::F64, cols);
            let r_oob = self.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, i0, rows);
            self.fail_if(r_oob, 4, one, rf);
            let c_oob = self.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, j0, cols);
            self.fail_if(c_oob, 4, two, cf);
        }
        let rows = self.slot_field(k, SLOT_ROWS);
        let lin = self.b.ins().imul(j0, rows);
        let lin = self.b.ins().iadd(lin, i0);
        let data = self.slot_field(k, SLOT_DATA);
        let off = self.b.ins().ishl_imm_s(lin, 3);
        let addr = self.b.ins().iadd(data, off);
        Ok((Some(addr), inb, (iv, jv, i0, j0)))
    }

    fn call(&mut self, name: &str, args: &[Expr]) -> CR<V> {
        match self.env_funcs.get(name).cloned() {
            Some(Callee::User(f)) => {
                let Some(&id) = self.funcs.get(&(Rc::as_ptr(&f) as usize, args.len())) else { return reject(format!("function '{name}' not compiled")) };
                let mut argv = vec![self.ctx];
                for a in args {
                    let v = self.expr(a)?;
                    argv.push(self.to_f(v));
                }
                let fref = match self.func_refs.get(&id) {
                    Some(r) => *r,
                    None => {
                        let r = self.module.declare_func_in_func(id, self.b.func);
                        self.func_refs.insert(id, r);
                        r
                    }
                };
                let c = self.b.ins().call(fref, &argv);
                let res = self.b.inst_results(c).to_vec();
                self.propagate(res[0]);
                Ok(V::F(res[1]))
            }
            Some(Callee::Builtin) => self.intrinsic(name, args),
            None => reject(format!("unknown function '{name}'")),
        }
    }

    fn arg_f(&mut self, args: &[Expr], i: usize) -> CR<CVal> {
        let v = self.expr(args.get(i).ok_or_else(|| Reject("missing argument".into()))?)?;
        Ok(self.to_f(v))
    }

    fn intrinsic(&mut self, name: &str, args: &[Expr]) -> CR<V> {
        let n = args.len();
        let want = |k: usize| -> CR<()> { if n == k { Ok(()) } else { reject(format!("{name} with {n} arguments")) } };
        Ok(match name {
            "pi" | "Inf" | "inf" | "NaN" | "nan" | "eps" => {
                want(0)?;
                V::F(self.fconst(match name {
                    "pi" => std::f64::consts::PI,
                    "Inf" | "inf" => f64::INFINITY,
                    "NaN" | "nan" => f64::NAN,
                    _ => f64::EPSILON,
                }))
            }
            "true" | "false" => {
                want(0)?;
                V::B(self.b.ins().iconst(types::I8, (name == "true") as i64))
            }
            "numel" | "length" | "size" => {
                let Some(Expr::Ident(arr)) = args.first() else { return reject("size of expression") };
                let Some(&(k, _)) = self.slots.get(arr) else { return reject("size of scalar") };
                match (name, n) {
                    ("numel", 1) => {
                        let l = self.slot_field(k, SLOT_LEN);
                        V::F(self.b.ins().fcvt_from_sint(types::F64, l))
                    }
                    ("length", 1) => {
                        let l = self.slot_field(k, SLOT_LEN);
                        let r = self.slot_field(k, SLOT_ROWS);
                        let c = self.slot_field(k, SLOT_COLS);
                        let m = self.b.ins().smax(r, c);
                        let z = self.b.ins().iconst(types::I64, 0);
                        let empty = self.b.ins().icmp_imm_s(IntCC::Equal, l, 0);
                        let v = self.b.ins().select(empty, z, m);
                        V::F(self.b.ins().fcvt_from_sint(types::F64, v))
                    }
                    ("size", 2) => {
                        let off = match &args[1] {
                            Expr::Num(d) if *d == 1.0 => SLOT_ROWS,
                            Expr::Num(d) if *d == 2.0 => SLOT_COLS,
                            _ => return reject("size with a non-literal dimension"),
                        };
                        let v = self.slot_field(k, off);
                        V::F(self.b.ins().fcvt_from_sint(types::F64, v))
                    }
                    _ => return reject("unsupported size form"),
                }
            }
            "abs" | "floor" | "ceil" | "fix" | "round" | "sin" | "cos" | "tan" | "exp" | "atan" | "sinh" | "cosh" | "tanh" | "sign" => {
                want(1)?;
                let x = self.arg_f(args, 0)?;
                V::F(match name {
                    "abs" => self.b.ins().fabs(x),
                    "floor" => self.b.ins().floor(x),
                    "ceil" => self.b.ins().ceil(x),
                    "fix" => self.b.ins().trunc(x),
                    "round" => self.call1(h_round, x),
                    "sin" => self.call1(h_sin, x),
                    "cos" => self.call1(h_cos, x),
                    "tan" => self.call1(h_tan, x),
                    "exp" => self.call1(h_exp, x),
                    "atan" => self.call1(h_atan, x),
                    "sinh" => self.call1(h_sinh, x),
                    "cosh" => self.call1(h_cosh, x),
                    "tanh" => self.call1(h_tanh, x),
                    _ => {
                        // sign: 1, -1, 0, NaN
                        let z = self.fconst(0.0);
                        let one = self.fconst(1.0);
                        let m1 = self.fconst(-1.0);
                        let gt = self.b.ins().fcmp(FloatCC::GreaterThan, x, z);
                        let lt = self.b.ins().fcmp(FloatCC::LessThan, x, z);
                        let s = self.b.ins().select(lt, m1, x);
                        let s = self.b.ins().select(gt, one, s);
                        let eq = self.b.ins().fcmp(FloatCC::Equal, x, z);
                        self.b.ins().select(eq, z, s)
                    }
                })
            }
            "isnan" | "isinf" | "isfinite" => {
                want(1)?;
                let x = self.arg_f(args, 0)?;
                V::B(match name {
                    "isnan" => self.b.ins().fcmp(FloatCC::Unordered, x, x),
                    _ => {
                        let a = self.b.ins().fabs(x);
                        let inf = self.fconst(f64::INFINITY);
                        let is_inf = self.b.ins().fcmp(FloatCC::Equal, a, inf);
                        if name == "isinf" {
                            is_inf
                        } else {
                            self.b.ins().fcmp(FloatCC::LessThan, a, inf)
                        }
                    }
                })
            }
            "mod" | "rem" | "atan2" | "hypot" | "min" | "max" => {
                want(2)?;
                let x = self.arg_f(args, 0)?;
                let y = self.arg_f(args, 1)?;
                V::F(match name {
                    "mod" => {
                        // b == 0 ? a : (r = a - floor(a/b)*b; r != 0 && sign(r) != sign(b) ? r + b : r)
                        let z = self.fconst(0.0);
                        let q = self.b.ins().fdiv(x, y);
                        let fq = self.b.ins().floor(q);
                        let p = self.b.ins().fmul(fq, y);
                        let r = self.b.ins().fsub(x, p);
                        let rneg = self.b.ins().fcmp(FloatCC::LessThan, r, z);
                        let yneg = self.b.ins().fcmp(FloatCC::LessThan, y, z);
                        let differ = self.b.ins().bxor(rneg, yneg);
                        let rnz = self.b.ins().fcmp(FloatCC::NotEqual, r, z);
                        let fix = self.b.ins().band(differ, rnz);
                        let rb = self.b.ins().fadd(r, y);
                        let r = self.b.ins().select(fix, rb, r);
                        let y0 = self.b.ins().fcmp(FloatCC::Equal, y, z);
                        self.b.ins().select(y0, x, r)
                    }
                    "rem" => {
                        let z = self.fconst(0.0);
                        let q = self.b.ins().fdiv(x, y);
                        let tq = self.b.ins().trunc(q);
                        let p = self.b.ins().fmul(tq, y);
                        let r = self.b.ins().fsub(x, p);
                        let nan = self.fconst(f64::NAN);
                        let y0 = self.b.ins().fcmp(FloatCC::Equal, y, z);
                        self.b.ins().select(y0, nan, r)
                    }
                    "atan2" => self.call2(h_atan2, x, y),
                    "hypot" => self.call2(h_hypot, x, y),
                    _ => {
                        // NaN-ignoring min/max (MATLAB): NaN operands lose.
                        let cc = if name == "max" { FloatCC::GreaterThan } else { FloatCC::LessThan };
                        let better = self.b.ins().fcmp(cc, y, x);
                        let pick = self.b.ins().select(better, y, x);
                        let xnan = self.b.ins().fcmp(FloatCC::Unordered, x, x);
                        let ynan = self.b.ins().fcmp(FloatCC::Unordered, y, y);
                        let pick = self.b.ins().select(ynan, x, pick);
                        self.b.ins().select(xnan, y, pick)
                    }
                })
            }
            _ => return reject(format!("builtin '{name}' is not compiled")),
        })
    }

    // ---- statements -----------------------------------------------------------------------

    fn assign_scalar(&mut self, name: &str, v: V) -> CR<()> {
        let ty = match v {
            V::F(_) => Ty::F64,
            V::B(_) => Ty::Bool,
        };
        if !self.scalars.contains_key(name) {
            if self.is_fn {
                let var = self.b.declare_var(if ty == Ty::Bool { types::I8 } else { types::F64 });
                self.scalars.insert(name.to_string(), ScalarVar { var, ty, assigned: None, empty: None });
            } else {
                return reject(format!("'{name}' has no kernel slot"));
            }
        }
        let s = &self.scalars[name];
        if s.ty != ty {
            return reject(format!("'{name}' changes type"));
        }
        let (var, assigned, empty) = (s.var, s.assigned, s.empty);
        let val = match v {
            V::F(x) | V::B(x) => x,
        };
        self.b.def_var(var, val);
        if let Some(a) = assigned {
            let one = self.b.ins().iconst(types::I8, 1);
            self.b.def_var(a, one);
        }
        if let Some(e) = empty {
            let zero = self.b.ins().iconst(types::I8, 0);
            self.b.def_var(e, zero);
        }
        self.defined.insert(name.to_string());
        self.maybe_empty.remove(name);
        Ok(())
    }

    fn block(&mut self, stmts: &[Stmt]) -> CR<()> {
        for s in stmts {
            self.stmt(s)?;
        }
        Ok(())
    }

    fn after_jump(&mut self) {
        // Code after break/continue is unreachable but must live in a block.
        let dead = self.b.create_block();
        self.b.switch_to_block(dead);
    }

    fn stmt(&mut self, s: &Stmt) -> CR<()> {
        if !self.is_fn {
            self.line = s.line as i64;
        }
        match &s.kind {
            StmtKind::Assign { lhs, rhs, print } => {
                if *print {
                    return reject("displays a value");
                }
                if lhs.len() != 1 {
                    return reject("multiple assignment");
                }
                let LValue::Target { name, chain } = &lhs[0] else { return reject("~ target") };
                match chain.as_slice() {
                    [] => {
                        let v = self.expr(rhs)?;
                        self.assign_scalar(name, v)
                    }
                    [Accessor::Field(f)] => {
                        let Some(target) = self.fields.get(&(name.clone(), f.clone())).cloned() else { return reject("unknown field") };
                        let v = self.expr(rhs)?;
                        self.assign_scalar(&target, v)
                    }
                    [Accessor::Paren(args)] => {
                        let Some(&(k, false)) = self.slots.get(name) else { return reject("indexed assignment into non-array") };
                        self.array_write(k, args, rhs)
                    }
                    [Accessor::Brace(args)] => {
                        let Some(&(k, true)) = self.slots.get(name) else { return reject("brace assignment into non-cell") };
                        if args.len() != 1 {
                            return reject("multi-index cell write");
                        }
                        let len = self.slot_field(k, SLOT_LEN);
                        let lenf = self.b.ins().fcvt_from_sint(types::F64, len);
                        self.end_ctx.push(lenf);
                        let iv = self.expr(&args[0]);
                        self.end_ctx.pop();
                        let iv = self.to_f(iv?);
                        self.index0(iv);
                        let v = self.expr(rhs)?;
                        let is_bool = matches!(v, V::B(_));
                        let vf = self.to_f(v);
                        let kk = self.b.ins().iconst(types::I64, k as i64);
                        let ib = self.b.ins().iconst(types::I64, is_bool as i64);
                        let line = self.b.ins().iconst(types::I64, self.line);
                        let f = self.b.ins().iconst(types::I64, h_cell_set as *const () as i64);
                        let c = self.b.ins().call_indirect(self.h.cell_set, f, &[self.ctx, kk, iv, vf, ib, line]);
                        let st = self.b.inst_results(c)[0];
                        self.propagate(st);
                        Ok(())
                    }
                    _ => reject("unsupported assignment target"),
                }
            }
            StmtKind::Expr { expr, print } => {
                if *print {
                    return reject("displays a value");
                }
                // Calls of user functions for effect only; discard the value.
                match expr {
                    Expr::Index(..) | Expr::Ident(_) => {
                        self.expr(expr)?;
                        Ok(())
                    }
                    _ => reject("expression statement"),
                }
            }
            StmtKind::If { clauses, otherwise } => {
                let done = self.b.create_block();
                let entry_defined = self.defined.clone();
                let mut branch_sets: Vec<HashSet<String>> = Vec::new();
                for (cond, body) in clauses {
                    self.defined = entry_defined.clone();
                    let c = self.expr(cond)?;
                    let t = self.truth(c);
                    let then_b = self.b.create_block();
                    let else_b = self.b.create_block();
                    self.b.ins().brif(t, then_b, &[], else_b, &[]);
                    self.b.switch_to_block(then_b);
                    self.block(body)?;
                    branch_sets.push(self.defined.clone());
                    self.b.ins().jump(done, &[]);
                    self.b.switch_to_block(else_b);
                }
                self.defined = entry_defined.clone();
                match otherwise {
                    Some(body) => {
                        self.block(body)?;
                        branch_sets.push(self.defined.clone());
                    }
                    None => branch_sets.push(entry_defined.clone()),
                }
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(done);
                let mut inter = branch_sets[0].clone();
                for s in &branch_sets[1..] {
                    inter = inter.intersection(s).cloned().collect();
                }
                self.defined = inter;
                Ok(())
            }
            StmtKind::While { cond, body } => {
                let header = self.b.create_block();
                let body_b = self.b.create_block();
                let exit = self.b.create_block();
                let entry_defined = self.defined.clone();
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(header);
                let c = self.expr(cond)?;
                let t = self.truth(c);
                self.b.ins().brif(t, body_b, &[], exit, &[]);
                self.b.switch_to_block(body_b);
                self.loops.push((header, exit));
                self.block(body)?;
                self.loops.pop();
                self.cancel_check();
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(exit);
                self.defined = entry_defined;
                Ok(())
            }
            StmtKind::For { var, iter, body } => {
                let Expr::Range(a, step, b) = iter else { return reject("for over a non-range") };
                if self.slots.contains_key(var) {
                    return reject("loop variable is an array");
                }
                let av = self.expr(a)?;
                let a = self.to_f(av);
                let st = match step {
                    Some(s) => {
                        let v = self.expr(s)?;
                        self.to_f(v)
                    }
                    None => self.fconst(1.0),
                };
                let bv = self.expr(b)?;
                let b = self.to_f(bv);
                let f = self.b.ins().iconst(types::I64, h_range_count as *const () as i64);
                let c = self.b.ins().call_indirect(self.h.range_count, f, &[a, st, b]);
                let n = self.b.inst_results(c)[0];
                let i = self.b.declare_var(types::I64);
                let zero = self.b.ins().iconst(types::I64, 0);
                self.b.def_var(i, zero);
                let entry_defined = self.defined.clone();
                // Zero iterations leave the loop variable empty (1×0), as in MATLAB.
                let none = self.b.ins().icmp_imm_s(IntCC::Equal, n, 0);
                if let Some(e) = self.scalars.get(var).and_then(|s| s.empty) {
                    let cur = self.b.use_var(e);
                    let v = self.b.ins().select(none, none, cur);
                    self.b.def_var(e, v);
                }
                let header = self.b.create_block();
                let body_b = self.b.create_block();
                let latch = self.b.create_block();
                let exit = self.b.create_block();
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(header);
                let iv = self.b.use_var(i);
                let more = self.b.ins().icmp(IntCC::SignedLessThan, iv, n);
                self.b.ins().brif(more, body_b, &[], exit, &[]);
                self.b.switch_to_block(body_b);
                let iv = self.b.use_var(i);
                let fi = self.b.ins().fcvt_from_sint(types::F64, iv);
                let prod = self.b.ins().fmul(fi, st);
                let x = self.b.ins().fadd(a, prod);
                self.assign_scalar(var, V::F(x))?;
                self.loops.push((latch, exit));
                self.block(body)?;
                self.loops.pop();
                self.b.ins().jump(latch, &[]);
                self.b.switch_to_block(latch);
                let iv = self.b.use_var(i);
                let next = self.b.ins().iadd_imm_s(iv, 1);
                self.b.def_var(i, next);
                self.cancel_check();
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(exit);
                self.defined = entry_defined;
                if !self.defined.contains(var) || self.scalars.get(var).map(|s| s.empty.is_some()).unwrap_or(false) {
                    // The variable may be empty after a zero-iteration loop.
                    self.maybe_empty.insert(var.clone());
                }
                Ok(())
            }
            StmtKind::Break => {
                let Some(&(_, exit)) = self.loops.last() else { return reject("break outside loop") };
                self.b.ins().jump(exit, &[]);
                self.after_jump();
                Ok(())
            }
            StmtKind::Continue => {
                let Some(&(cont, _)) = self.loops.last() else { return reject("continue outside loop") };
                self.b.ins().jump(cont, &[]);
                self.after_jump();
                Ok(())
            }
            _ => reject("unsupported statement"),
        }
    }

    fn array_write(&mut self, k: usize, args: &[Expr], rhs: &Expr) -> CR<()> {
        match args.len() {
            1 => {
                let len = self.slot_field(k, SLOT_LEN);
                let lenf = self.b.ins().fcvt_from_sint(types::F64, len);
                self.end_ctx.push(lenf);
                let iv = self.expr(&args[0]);
                self.end_ctx.pop();
                let iv = self.to_f(iv?);
                let i0 = self.index0(iv);
                let v = self.expr(rhs)?;
                let v = self.to_f(v);
                let len = self.slot_field(k, SLOT_LEN);
                let inb = self.b.ins().icmp(IntCC::UnsignedLessThan, i0, len);
                let fast = self.b.create_block();
                let slow = self.b.create_block();
                let done = self.b.create_block();
                self.b.ins().brif(inb, fast, &[], slow, &[]);
                self.b.switch_to_block(fast);
                let data = self.slot_field(k, SLOT_DATA);
                let off = self.b.ins().ishl_imm_s(i0, 3);
                let addr = self.b.ins().iadd(data, off);
                self.b.ins().store(self.tf, v, addr, 0);
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(slow);
                let kk = self.b.ins().iconst(types::I64, k as i64);
                let line = self.b.ins().iconst(types::I64, self.line);
                let f = self.b.ins().iconst(types::I64, h_arr_set1 as *const () as i64);
                let c = self.b.ins().call_indirect(self.h.arr_set1, f, &[self.ctx, kk, iv, v, line]);
                let st = self.b.inst_results(c)[0];
                self.propagate(st);
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(done);
                Ok(())
            }
            2 => {
                let (addr, inb, (iv, jv, _, _)) = self.addr2(k, args, false)?;
                let v = self.expr(rhs)?;
                let v = self.to_f(v);
                let fast = self.b.create_block();
                let slow = self.b.create_block();
                let done = self.b.create_block();
                self.b.ins().brif(inb, fast, &[], slow, &[]);
                self.b.switch_to_block(fast);
                self.b.ins().store(self.tf, v, addr.unwrap(), 0);
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(slow);
                let kk = self.b.ins().iconst(types::I64, k as i64);
                let line = self.b.ins().iconst(types::I64, self.line);
                let f = self.b.ins().iconst(types::I64, h_arr_set2 as *const () as i64);
                let c = self.b.ins().call_indirect(self.h.arr_set2, f, &[self.ctx, kk, iv, jv, v, line]);
                let st = self.b.inst_results(c)[0];
                self.propagate(st);
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(done);
                Ok(())
            }
            _ => reject("N-D indexed assignment"),
        }
    }

    fn cancel_check(&mut self) {
        let p = self.b.ins().load(types::I64, self.tf, self.ctx, CTX_CANCEL);
        let flag = self.b.ins().load(types::I8, self.plain, p, 0);
        let z = self.fconst(0.0);
        self.fail_if(flag, 6, z, z);
    }
}

fn helpers(b: &mut FunctionBuilder, module: &JITModule) -> Helpers {
    let ptr = types::I64;
    let mut sig = |params: &[types::Type], rets: &[types::Type]| -> SigRef {
        let mut s = module.make_signature();
        for p in params {
            s.params.push(AbiParam::new(*p));
        }
        for r in rets {
            s.returns.push(AbiParam::new(*r));
        }
        b.import_signature(s)
    };
    let (f, i, i32t) = (types::F64, types::I64, types::I32);
    Helpers {
        err: sig(&[ptr, i, f, f, i], &[i32t]),
        set_line: sig(&[ptr, i], &[]),
        arr_set1: sig(&[ptr, i, f, f, i], &[i32t]),
        arr_set2: sig(&[ptr, i, f, f, f, i], &[i32t]),
        cell_set: sig(&[ptr, i, f, f, i, i], &[i32t]),
        cell_get: sig(&[ptr, i, f, i], &[f]),
        range_count: sig(&[f, f, f], &[i]),
        f1: sig(&[f], &[f]),
        f2: sig(&[f, f], &[f]),
    }
}

/// Definite-assignment pre-check for function bodies: the output must be assigned on every
/// path and `return` is not supported.
fn assigns_output(body: &[Stmt], out: &str) -> bool {
    fn block(b: &[Stmt], out: &str) -> Option<bool> {
        let mut assigned = false;
        for s in b {
            match &s.kind {
                StmtKind::Return => return None,
                StmtKind::Assign { lhs, .. } => {
                    if lhs.iter().any(|l| matches!(l, LValue::Target { name, chain } if name == out && chain.is_empty())) {
                        assigned = true;
                    }
                }
                StmtKind::If { clauses, otherwise } => {
                    let mut all = otherwise.is_some();
                    for (_, body) in clauses {
                        all &= block(body, out)?;
                    }
                    if let Some(o) = otherwise {
                        all &= block(o, out)?;
                    }
                    assigned |= all;
                }
                StmtKind::For { body, .. } | StmtKind::While { body, .. } => {
                    block(body, out)?;
                }
                _ => {}
            }
        }
        Some(assigned)
    }
    block(body, out).unwrap_or(false)
}

impl Jit {
    pub fn new() -> Self {
        Self { module: make_module(), fctx: FunctionBuilderContext::new(), funcs: HashMap::new(), next_name: 0, retired: Vec::new() }
    }

    /// Forgets compiled user functions (their ASTs may be freed between runs). Code memory
    /// is kept, so kernels already handed out stay valid.
    pub fn reset_functions(&mut self) {
        self.funcs.clear();
    }

    /// Replaces the module after a failed compilation left a declared-but-undefined
    /// function in it. The old module is retired, not freed, so its code stays valid.
    fn fresh_module(&mut self) {
        let old = std::mem::replace(&mut self.module, make_module());
        self.retired.push(old);
        self.funcs.clear();
        // An abandoned build leaves its state in the builder context.
        self.fctx = FunctionBuilderContext::new();
    }

    fn fn_sig(&self, arity: usize) -> Signature {
        let mut s = self.module.make_signature();
        s.params.push(AbiParam::new(types::I64));
        for _ in 0..arity {
            s.params.push(AbiParam::new(types::F64));
        }
        s.returns.push(AbiParam::new(types::I32));
        s.returns.push(AbiParam::new(types::F64));
        s
    }

    fn fresh_name(&mut self, prefix: &str) -> String {
        self.next_name += 1;
        format!("{prefix}{}", self.next_name)
    }

    /// Declares and compiles every user function reachable from `env.funcs` that is not
    /// compiled yet (all scalar double parameters, one scalar output).
    fn compile_functions(&mut self, env: &Env, arities: &HashMap<String, Vec<usize>>) -> CR<()> {
        let mut todo: Vec<(Rc<Function>, usize)> = Vec::new();
        for (name, callee) in &env.funcs {
            if let Callee::User(f) = callee {
                for &n in arities.get(name).into_iter().flatten() {
                    if !self.funcs.contains_key(&(Rc::as_ptr(f) as usize, n)) {
                        todo.push((f.clone(), n));
                    }
                }
            }
        }
        if todo.is_empty() {
            return Ok(());
        }
        // Declare first so functions can call each other (and themselves).
        let mut declared = Vec::new();
        for (f, n) in &todo {
            if *n != f.params.len() || f.outputs.len() != 1 || f.params.iter().any(|p| p == "varargin" || p == "~") {
                return reject(format!("function '{}' signature", f.name));
            }
            if !assigns_output(&f.body, &f.outputs[0]) {
                return reject(format!("function '{}' may not assign its output", f.name));
            }
            let sig = self.fn_sig(*n);
            let name = self.fresh_name("f");
            let id = self.module.declare_function(&name, Linkage::Local, &sig).map_err(|e| Reject(e.to_string()))?;
            self.funcs.insert((Rc::as_ptr(f) as usize, *n), id);
            declared.push((f.clone(), *n, id));
        }
        for (f, n, id) in declared {
            if let Err(e) = self.define_function(&f, n, id, env) {
                // A declared but undefined function poisons the module; start a fresh one.
                self.fresh_module();
                return Err(e);
            }
        }
        self.module.finalize_definitions().map_err(|e| Reject(e.to_string()))?;
        Ok(())
    }

    fn define_function(&mut self, f: &Function, arity: usize, id: FuncId, env: &Env) -> CR<()> {
        let cfg = self.module.target_config();
        let mut ctx = self.module.make_context();
        ctx.func.signature = self.fn_sig(arity);
        ctx.func.name = UserFuncName::user(0, id.as_u32());
        {
            let mut b = FunctionBuilder::new(&mut ctx.func, &mut self.fctx);
            let tf = MemFlagsData::trusted();
            let plain = MemFlagsData::new();
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let params = b.block_params(entry).to_vec();
            let h = helpers(&mut b, &self.module);
            let exit = b.create_block();
            let status = b.declare_var(types::I32);
            let z32 = b.ins().iconst(types::I32, 0);
            b.def_var(status, z32);
            let mut g = Gen {
                b,
                module: &mut self.module,
                funcs: &self.funcs,
                env_funcs: &env.funcs,
                h,
                ctx: params[0],
                scalars: HashMap::new(),
                slots: HashMap::new(),
                fields: HashMap::new(),
                defined: HashSet::new(),
                maybe_empty: HashSet::new(),
                loops: Vec::new(),
                exit,
                status,
                line: 0,
                is_fn: true,
                order: Vec::new(),
                frame_ptr: None,
                func_refs: HashMap::new(),
                end_ctx: Vec::new(),
                tf,
                plain,
            };
            // Recursion depth.
            let d = g.b.ins().load(types::I64, g.tf, g.ctx, CTX_DEPTH);
            let d1 = g.b.ins().iadd_imm_s(d, 1);
            g.b.ins().store(g.tf, d1, g.ctx, CTX_DEPTH);
            let too_deep = g.b.ins().icmp_imm_s(IntCC::SignedGreaterThan, d1, MAX_DEPTH);
            let z = g.fconst(0.0);
            g.fail_if(too_deep, 5, z, z);
            for (k, p) in f.params.iter().enumerate() {
                g.assign_scalar(p, V::F(params[k + 1]))?;
            }
            // nargin/nargout are constants for this specialization.
            let na = g.fconst(arity as f64);
            g.assign_scalar("nargin", V::F(na))?;
            let no = g.fconst(1.0);
            g.assign_scalar("nargout", V::F(no))?;
            g.block(&f.body)?;
            g.b.ins().jump(exit, &[]);
            g.b.switch_to_block(exit);
            let d = g.b.ins().load(types::I64, g.tf, g.ctx, CTX_DEPTH);
            let d0 = g.b.ins().iadd_imm_s(d, -1);
            g.b.ins().store(g.tf, d0, g.ctx, CTX_DEPTH);
            let st = g.b.use_var(status);
            let out = match g.scalars.get(&f.outputs[0]) {
                Some(s) if s.ty == Ty::F64 => g.b.use_var(s.var),
                Some(_) => return reject("function output is logical"),
                None => return reject("function output never assigned"),
            };
            g.b.ins().return_(&[st, out]);
            g.b.seal_all_blocks();
            g.b.finalize(cfg);
        }
        self.module.define_function(id, &mut ctx).map_err(|e| Reject(format!("{e:?}")))?;
        self.module.clear_context(&mut ctx);
        Ok(())
    }

    /// Compiles `f` (all-scalar double arguments) and an entry trampoline callable from Rust:
    /// `entry(ctx, args, out) -> status`.
    pub fn compile_entry(&mut self, f: &Rc<Function>, arity: usize, env: &Env) -> CR<extern "C" fn(*mut Ctx, *const f64, *mut f64) -> i32> {
        let mut env_funcs = env.funcs.clone();
        env_funcs.insert(f.name.clone(), Callee::User(f.clone()));
        let env = Env { vars: HashMap::new(), funcs: env_funcs };
        let mut arities: HashMap<String, Vec<usize>> = HashMap::new();
        arities.entry(f.name.clone()).or_default().push(arity);
        for callee in env.funcs.values() {
            if let Callee::User(g) = callee {
                let mut inner = HashMap::new();
                collect_uses(&g.body, &mut inner);
                for (n, u) in inner {
                    if env.funcs.contains_key(&n) && !g.params.contains(&n) && !u.written {
                        arities.entry(n).or_default().extend(u.call_arities.iter().copied());
                    }
                }
            }
        }
        self.compile_functions(&env, &arities)?;
        let target = *self.funcs.get(&(Rc::as_ptr(f) as usize, arity)).ok_or_else(|| Reject("function not compiled".into()))?;
        let mut sig = self.module.make_signature();
        for _ in 0..3 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I32));
        let name = self.fresh_name("e");
        let id = self.module.declare_function(&name, Linkage::Local, &sig).map_err(|e| Reject(e.to_string()))?;
        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;
        ctx.func.name = UserFuncName::user(0, id.as_u32());
        let cfg = self.module.target_config();
        {
            let mut b = FunctionBuilder::new(&mut ctx.func, &mut self.fctx);
            let tf = MemFlagsData::trusted();
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let p = b.block_params(entry).to_vec();
            let mut args = vec![p[0]];
            for k in 0..arity {
                args.push(b.ins().load(types::F64, tf, p[1], (k * 8) as i32));
            }
            let fref = self.module.declare_func_in_func(target, b.func);
            let call = b.ins().call(fref, &args);
            let res = b.inst_results(call).to_vec();
            b.ins().store(tf, res[1], p[2], 0);
            b.ins().return_(&[res[0]]);
            b.seal_all_blocks();
            b.finalize(cfg);
        }
        self.module.define_function(id, &mut ctx).map_err(|e| Reject(format!("{e:?}")))?;
        self.module.clear_context(&mut ctx);
        self.module.finalize_definitions().map_err(|e| Reject(e.to_string()))?;
        let code = self.module.get_finalized_function(id);
        Ok(unsafe { std::mem::transmute::<*const u8, extern "C" fn(*mut Ctx, *const f64, *mut f64) -> i32>(code) })
    }

    /// Compiles a loop statement for the given variable environment.
    pub fn compile_kernel(&mut self, stmt: &Stmt, env: &Env) -> CR<Kernel> {
        // Classify the variables the statement touches.
        let mut names = Vec::new();
        referenced_names(std::slice::from_ref(stmt), &mut names);
        let mut uses: HashMap<String, Uses> = HashMap::new();
        collect_uses(std::slice::from_ref(stmt), &mut uses);
        let mut scalars: Vec<KernelScalar> = Vec::new();
        let mut slot_names: Vec<String> = Vec::new();
        let mut slot_is_cell: Vec<bool> = Vec::new();
        let mut fields: HashMap<(String, String), String> = HashMap::new();
        let mut arities: HashMap<String, Vec<usize>> = HashMap::new();
        for n in &names {
            let u = uses.get(n).cloned().unwrap_or_default();
            match env.vars.get(n) {
                Some(VarType::F64) | Some(VarType::Bool) => {
                    if u.indexed || u.braced || !u.fields.is_empty() || u.sized {
                        return reject(format!("scalar '{n}' used as an array"));
                    }
                    scalars.push(KernelScalar { home: ScalarHome::Var(n.clone()), ty_bool: env.vars[n] == VarType::Bool, at_entry: true });
                }
                Some(VarType::Array) => {
                    if u.plain || u.braced || !u.fields.is_empty() {
                        return reject(format!("array '{n}' used as a whole"));
                    }
                    slot_names.push(n.clone());
                    slot_is_cell.push(false);
                }
                Some(VarType::Cell { all_scalar }) => {
                    if u.plain || u.indexed || !u.fields.is_empty() {
                        return reject(format!("cell '{n}' used as a whole"));
                    }
                    if u.brace_read && (u.brace_write || !all_scalar) {
                        return reject(format!("cell '{n}' read with non-scalar or written elements"));
                    }
                    slot_names.push(n.clone());
                    slot_is_cell.push(true);
                }
                Some(VarType::Struct(fs)) => {
                    if u.plain || u.indexed || u.braced || u.sized {
                        return reject(format!("struct '{n}' used as a whole"));
                    }
                    for f in &u.fields {
                        let Some((_, Some(kind))) = fs.iter().find(|(x, _)| x == f) else { return reject(format!("field '{n}.{f}' is not a scalar")) };
                        let key = format!("{n}.{f}");
                        fields.insert((n.clone(), f.clone()), key);
                        scalars.push(KernelScalar { home: ScalarHome::Field(n.clone(), f.clone()), ty_bool: *kind == Kind::Logical, at_entry: true });
                    }
                }
                Some(VarType::Other) => return reject(format!("'{n}' has an unsupported type")),
                None => {
                    if u.written {
                        if u.indexed || u.braced || !u.fields.is_empty() {
                            return reject(format!("'{n}' is created by indexed assignment"));
                        }
                        // New scalar; its type is fixed by its first assignment below.
                        scalars.push(KernelScalar { home: ScalarHome::Var(n.clone()), ty_bool: u.assigned_bool, at_entry: false });
                    } else {
                        match env.funcs.get(n) {
                            Some(_) => {
                                arities.entry(n.clone()).or_default().extend(u.call_arities.iter().copied());
                            }
                            None => return reject(format!("'{n}' is undefined")),
                        }
                    }
                }
            }
        }
        // Functions called from user functions.
        for (name, callee) in &env.funcs {
            if let Callee::User(f) = callee {
                let mut inner = HashMap::new();
                collect_uses(&f.body, &mut inner);
                for (n, u) in inner {
                    if env.funcs.contains_key(&n) && !f.params.contains(&n) && !u.written {
                        arities.entry(n).or_default().extend(u.call_arities.iter().copied());
                    }
                }
                let _ = name;
            }
        }
        self.compile_functions(env, &arities)?;

        let mut ctx = self.module.make_context();
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I32));
        let name = self.fresh_name("k");
        let id = self.module.declare_function(&name, Linkage::Local, &sig).map_err(|e| Reject(e.to_string()))?;
        ctx.func.signature = sig;
        ctx.func.name = UserFuncName::user(0, id.as_u32());
        let n = scalars.len();
        let order: Vec<String> = scalars
            .iter()
            .map(|s| match &s.home {
                ScalarHome::Var(v) => v.clone(),
                ScalarHome::Field(a, f) => format!("{a}.{f}"),
            })
            .collect();
        let cfg = self.module.target_config();
        let result = (|| -> CR<()> {
            let mut b = FunctionBuilder::new(&mut ctx.func, &mut self.fctx);
            let tf = MemFlagsData::trusted();
            let plain = MemFlagsData::new();
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let ctx_v = b.block_params(entry)[0];
            let h = helpers(&mut b, &self.module);
            let exit = b.create_block();
            let status = b.declare_var(types::I32);
            let z32 = b.ins().iconst(types::I32, 0);
            b.def_var(status, z32);
            let frame = b.ins().load(types::I64, tf, ctx_v, CTX_FRAME);
            let mut sv = HashMap::new();
            let mut defined = HashSet::new();
            for (i, s) in scalars.iter().enumerate() {
                let ty = if s.ty_bool { Ty::Bool } else { Ty::F64 };
                let var = b.declare_var(if s.ty_bool { types::I8 } else { types::F64 });
                let assigned = b.declare_var(types::I8);
                let empty = b.declare_var(types::I8);
                let z8 = b.ins().iconst(types::I8, 0);
                b.def_var(assigned, z8);
                b.def_var(empty, z8);
                let v = b.ins().load(types::F64, tf, frame, (i * 8) as i32);
                let v = if s.ty_bool {
                    let z = b.ins().f64const(0.0);
                    b.ins().fcmp(FloatCC::NotEqual, v, z)
                } else {
                    v
                };
                b.def_var(var, v);
                if s.at_entry {
                    defined.insert(order[i].clone());
                }
                sv.insert(order[i].clone(), ScalarVar { var, ty, assigned: Some(assigned), empty: Some(empty) });
            }
            let mut slots = HashMap::new();
            for (k, (s, cell)) in slot_names.iter().zip(&slot_is_cell).enumerate() {
                slots.insert(s.clone(), (k, *cell));
            }
            let mut g = Gen {
                b,
                module: &mut self.module,
                funcs: &self.funcs,
                env_funcs: &env.funcs,
                h,
                ctx: ctx_v,
                scalars: sv,
                slots,
                fields: fields.clone(),
                defined,
                maybe_empty: HashSet::new(),
                loops: Vec::new(),
                exit,
                status,
                line: stmt.line as i64,
                is_fn: false,
                order: order.clone(),
                frame_ptr: Some(frame),
                func_refs: HashMap::new(),
                end_ctx: Vec::new(),
                tf,
                plain,
            };
            // New scalars get their declared type from their first assignment: re-check by
            // compiling; a mismatch rejects.
            g.stmt(stmt)?;
            g.b.ins().jump(exit, &[]);
            g.b.switch_to_block(exit);
            // Write every scalar back with its assigned/empty flags.
            let frame = g.frame_ptr.unwrap();
            for (i, name) in g.order.clone().iter().enumerate() {
                let s = &g.scalars[name];
                let (var, ty, assigned, empty) = (s.var, s.ty, s.assigned.unwrap(), s.empty.unwrap());
                let v = g.b.use_var(var);
                let v = if ty == Ty::Bool { g.to_f(V::B(v)) } else { v };
                g.b.ins().store(g.tf, v, frame, (i * 8) as i32);
                let a = g.b.use_var(assigned);
                let af = g.to_f(V::B(a));
                g.b.ins().store(g.tf, af, frame, ((n + i) * 8) as i32);
                let e = g.b.use_var(empty);
                let ef = g.to_f(V::B(e));
                g.b.ins().store(g.tf, ef, frame, ((2 * n + i) * 8) as i32);
            }
            let st = g.b.use_var(status);
            g.b.ins().return_(&[st]);
            g.b.seal_all_blocks();
            g.b.finalize(cfg);
            Ok(())
        })();
        if let Err(e) = result {
            self.module.clear_context(&mut ctx);
            // The kernel was declared but not defined: start a fresh module.
            self.fresh_module();
            return Err(e);
        }
        self.module.define_function(id, &mut ctx).map_err(|e| Reject(format!("{e:?}")))?;
        self.module.clear_context(&mut ctx);
        self.module.finalize_definitions().map_err(|e| Reject(e.to_string()))?;
        let code = self.module.get_finalized_function(id);
        let ptr: extern "C" fn(*mut Ctx) -> i32 = unsafe { std::mem::transmute(code) };
        Ok(Kernel { ptr, scalars, slots: slot_names })
    }
}

/// How a name is used inside a statement.
#[derive(Default, Clone)]
struct Uses {
    plain: bool,
    written: bool,
    indexed: bool,
    braced: bool,
    brace_read: bool,
    brace_write: bool,
    sized: bool,
    fields: Vec<String>,
    call_arities: Vec<usize>,
    assigned_bool: bool,
}

fn collect_uses(stmts: &[Stmt], u: &mut HashMap<String, Uses>) {
    fn expr(e: &Expr, u: &mut HashMap<String, Uses>) {
        match e {
            Expr::Ident(n) => {
                let x = u.entry(n.clone()).or_default();
                x.plain = true;
                x.call_arities.push(0);
            }
            Expr::Index(b, args) => {
                if let Expr::Ident(n) = b.as_ref() {
                    let x = u.entry(n.clone()).or_default();
                    x.indexed = true;
                    x.call_arities.push(args.len());
                    // numel(x)/length(x)/size(x, d) do not use x as a whole.
                    if matches!(n.as_str(), "numel" | "length" | "size") {
                        if let Some(Expr::Ident(a)) = args.first() {
                            u.entry(a.clone()).or_default().sized = true;
                            args.iter().skip(1).for_each(|a| expr(a, u));
                            return;
                        }
                    }
                } else {
                    expr(b, u);
                }
                args.iter().for_each(|a| expr(a, u));
            }
            Expr::CellIndex(b, args) => {
                if let Expr::Ident(n) = b.as_ref() {
                    let x = u.entry(n.clone()).or_default();
                    x.braced = true;
                    x.brace_read = true;
                } else {
                    expr(b, u);
                }
                args.iter().for_each(|a| expr(a, u));
            }
            Expr::Field(b, f) => {
                if let Expr::Ident(n) = b.as_ref() {
                    let x = u.entry(n.clone()).or_default();
                    if !x.fields.contains(f) {
                        x.fields.push(f.clone());
                    }
                } else {
                    expr(b, u);
                }
            }
            Expr::Unary(_, a) => expr(a, u),
            Expr::Binary(_, a, b) => {
                expr(a, u);
                expr(b, u);
            }
            Expr::Range(a, s, b) => {
                expr(a, u);
                if let Some(s) = s {
                    expr(s, u);
                }
                expr(b, u);
            }
            Expr::DynField(a, b) => {
                expr(a, u);
                expr(b, u);
            }
            Expr::Matrix(r) | Expr::Cell(r) => r.iter().flatten().for_each(|x| expr(x, u)),
            _ => {}
        }
    }
    fn is_bool_expr(e: &Expr) -> bool {
        match e {
            Expr::Binary(op, ..) => matches!(op, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::And | BinOp::Or | BinOp::AndAnd | BinOp::OrOr),
            Expr::Unary(UnOp::Not, _) => true,
            Expr::Ident(n) => n == "true" || n == "false",
            Expr::Index(f, _) => matches!(f.as_ref(), Expr::Ident(n) if matches!(n.as_str(), "isnan" | "isinf" | "isfinite")),
            _ => false,
        }
    }
    for s in stmts {
        match &s.kind {
            StmtKind::Expr { expr: e, .. } => expr(e, u),
            StmtKind::Assign { lhs, rhs, .. } => {
                for l in lhs {
                    if let LValue::Target { name, chain } = l {
                        let x = u.entry(name.clone()).or_default();
                        x.written = true;
                        match chain.first() {
                            None => x.assigned_bool |= is_bool_expr(rhs),
                            Some(Accessor::Paren(_)) => x.indexed = true,
                            Some(Accessor::Brace(_)) => {
                                x.braced = true;
                                x.brace_write = true;
                            }
                            Some(Accessor::Field(f)) => {
                                if !x.fields.contains(f) {
                                    x.fields.push(f.clone());
                                }
                            }
                            Some(Accessor::DynField(_)) => x.plain = true,
                        }
                        for a in chain {
                            match a {
                                Accessor::Paren(xs) | Accessor::Brace(xs) => xs.iter().for_each(|e| expr(e, u)),
                                Accessor::DynField(e) => expr(e, u),
                                Accessor::Field(_) => {}
                            }
                        }
                    }
                }
                expr(rhs, u);
            }
            StmtKind::If { clauses, otherwise } => {
                for (c, b) in clauses {
                    expr(c, u);
                    collect_uses(b, u);
                }
                if let Some(b) = otherwise {
                    collect_uses(b, u);
                }
            }
            StmtKind::For { var, iter, body } => {
                u.entry(var.clone()).or_default().written = true;
                expr(iter, u);
                collect_uses(body, u);
            }
            StmtKind::While { cond, body } => {
                expr(cond, u);
                collect_uses(body, u);
            }
            StmtKind::Switch { subject, cases, otherwise } => {
                expr(subject, u);
                for (c, b) in cases {
                    expr(c, u);
                    collect_uses(b, u);
                }
                if let Some(b) = otherwise {
                    collect_uses(b, u);
                }
            }
            StmtKind::Try { body, catch_body, .. } => {
                collect_uses(body, u);
                collect_uses(catch_body, u);
            }
            StmtKind::Command { name, .. } => {
                u.entry(name.clone()).or_default().plain = true;
            }
            _ => {}
        }
    }
}
