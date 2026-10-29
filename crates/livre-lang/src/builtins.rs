//! Built-in functions. `call` returns `Ok(None)` when the name is not a builtin. Type-specific
//! families live in `builtins_text`, `builtins_plot`, `time`, `table` and `sym`.

use crate::cplx::{poly_roots, C};
use crate::display::{disp_value, fmt_g, NumFormat};
use crate::error::{rt, LangError};
use crate::format::sprintf;
use crate::indexing::{self, hconcat, vconcat};
use crate::interp::{range_count, Interpreter, OutputKind};
use crate::nd::{self, Dims, IndexArg};
use crate::ops::{self, broadcast, map_c};
use crate::value::*;

type R<T> = Result<T, LangError>;
pub(crate) type Ret = R<Option<Vec<Value>>>;

const NAMES: &[&str] = &[
    "abs", "acos", "acosd", "acosh", "all", "angle", "any", "arrayfun", "asin", "asind", "asinh", "assert", "atan", "atan2", "atand", "atanh", "cast",
    "cat", "ceil", "cell", "cellfun", "circshift", "class", "clear", "complex", "conj", "cos", "cosd", "cosh", "cross", "cummax", "cummin", "cumprod",
    "cumsum", "deal", "deg2rad", "det", "diag", "diff", "disp", "display", "dot", "double", "eig", "eps", "error", "exist", "exp", "expm1", "eye",
    "factor", "factorial", "false", "feval", "fieldnames", "find", "fix", "flintmax", "flip", "fliplr", "flipud", "floor", "format", "func2str",
    "gamma", "gcd", "horzcat", "hypot", "i", "idivide", "imag", "Inf", "inf", "int16", "int32", "int64", "int8", "interp1", "intmax", "intmin", "inv",
    "isa", "iscell", "iscolumn", "iscomplex", "isempty", "isequal", "isfield", "isfinite", "isfloat", "isinf", "isinteger", "islogical", "ismatrix",
    "ismember", "ismethod", "isnan", "isnumeric", "isobject", "isprime", "isprop", "isreal", "isrow", "isscalar", "isstruct", "isvector", "j", "kron",
    "lcm", "length", "linspace", "log", "log10", "log1p", "log2", "logical", "logspace", "max", "mean", "median", "meshgrid", "methods", "min", "mod",
    "mode", "NaN", "nan", "nchoosek", "ndgrid", "ndims", "nnz", "norm", "nthroot", "num2cell", "numel", "ones", "pause", "permute", "pi", "polyfit",
    "polyval", "power", "primes", "prod", "properties", "rad2deg", "rand", "randi", "randn", "randperm", "real", "realmax", "realmin", "rem", "repmat",
    "reshape", "rmfield", "rng", "roots", "round", "sign", "sin", "sind", "single", "sinh", "size", "sort", "sortrows", "sqrt", "squeeze", "std",
    "str2func", "struct", "sum", "tan", "tand", "tanh", "tic", "toc", "trace", "transpose", "trapz", "true", "uint16", "uint32", "uint64", "uint8",
    "unique", "var", "vertcat", "warning", "zeros", "histcounts", "addpath", "rmpath", "path", "pwd", "drawnow", "clc", "isspace", "fprintf",
    "sprintf", "newline", "blanks", "accumarray", "cumtrapz", "validateattributes", "inputname", "nargchk", "narginchk", "isdiag", "istril",
    "istriu", "issorted", "triu", "tril", "floor", "ctranspose", "uminus", "uplus", "plus", "minus", "times", "mtimes", "rdivide", "ldivide",
    "mrdivide", "mldivide", "mpower", "eq", "ne", "lt", "le", "gt", "ge", "and", "or", "not", "xor", "colon", "isobject", "isstring",
];

pub fn exists(name: &str) -> bool {
    static ALL: std::sync::OnceLock<std::collections::HashSet<&'static str>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        [NAMES, crate::builtins_elfun::NAMES, crate::builtins_ops::NAMES, crate::builtins_text::NAMES, crate::builtins_plot::NAMES, crate::time::NAMES, crate::table::NAMES, crate::sym::NAMES]
            .iter()
            .flat_map(|n| n.iter().copied())
            .collect()
    })
    .contains(name)
}

pub(crate) fn one(v: Value) -> Ret {
    Ok(Some(vec![v]))
}

pub(crate) fn none() -> Ret {
    Ok(Some(Vec::new()))
}

fn num(v: f64) -> Ret {
    one(Value::num(v))
}

fn mat(m: Matrix) -> Ret {
    one(Value::Num(m))
}

pub(crate) fn arg<'a>(args: &'a [Value], i: usize, fname: &str) -> R<&'a Value> {
    args.get(i).ok_or_else(|| LangError::new(format!("Not enough input arguments for '{fname}'."), 0))
}

pub(crate) fn m_arg(args: &[Value], i: usize, fname: &str) -> R<Matrix> {
    match arg(args, i, fname)? {
        Value::Num(m) => Ok(m.clone()),
        Value::Sym(s) => s.to_matrix(),
        other => rt(format!("Undefined function '{fname}' for input arguments of type '{}'.", other.class_name())),
    }
}

fn bool_mat(m: &Matrix, f: impl Fn(f64) -> bool) -> Matrix {
    Matrix::from_dims(m.dims.clone(), m.data.iter().map(|&x| if f(x) { 1.0 } else { 0.0 }).collect()).with_kind(Kind::Logical)
}

/// Trailing class name in constructor calls: `zeros(2, 'int8')`, `ones(3, 'like', x)`.
fn class_option(args: &[Value]) -> R<(Vec<Value>, Kind)> {
    let mut kind = Kind::Double;
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i].is_text() {
            let t = args[i].text()?;
            if t.eq_ignore_ascii_case("like") {
                if let Some(Value::Num(m)) = args.get(i + 1) {
                    kind = m.kind;
                }
                i += 2;
                continue;
            }
            if let Some(k) = Kind::from_name(&t) {
                kind = k;
            }
            i += 1;
            continue;
        }
        rest.push(args[i].clone());
        i += 1;
    }
    Ok((rest, kind))
}

/// Parses size arguments: `()`, `(n)`, `(r, c, …)`, `([r c …])`.
pub(crate) fn size_args(args: &[Value]) -> R<Dims> {
    let nums: Vec<&Value> = args.iter().filter(|a| !a.is_text()).collect();
    let dim = |v: f64| -> R<usize> {
        if v.is_nan() {
            return rt("Size inputs must be integers.");
        }
        Ok(if v < 0.0 { 0 } else { v as usize })
    };
    match nums.len() {
        0 => Ok(vec![1, 1]),
        1 => {
            let m = nums[0].as_matrix()?;
            if m.numel() >= 2 {
                Ok(nd::normalize(m.data.iter().map(|&x| dim(x)).collect::<R<_>>()?))
            } else {
                let n = dim(nums[0].scalar()?)?;
                Ok(vec![n, n])
            }
        }
        _ => Ok(nd::normalize(nums.iter().map(|v| dim(v.scalar()?)).collect::<R<_>>()?)),
    }
}

/// First non-singleton dimension (1-based).
pub(crate) fn default_dim(d: &[usize]) -> usize {
    d.iter().position(|&x| x != 1).map(|p| p + 1).unwrap_or(1)
}

pub(crate) fn dim_arg(args: &[Value], i: usize, m: &Matrix) -> R<usize> {
    match args.get(i) {
        Some(v) if !v.is_text() && !matches!(v, Value::Num(x) if x.is_empty()) => {
            let d = v.scalar()?;
            if d < 1.0 || d.fract() != 0.0 {
                return rt("Dimension argument must be a positive integer scalar within indexing range.");
            }
            Ok(d as usize)
        }
        _ => Ok(default_dim(&m.dims)),
    }
}

/// Positions of the slices along dimension `dim` (1-based): one Vec per output element.
pub(crate) fn slices(d: &[usize], dim: usize) -> (Dims, Vec<Vec<usize>>) {
    let k = dim - 1;
    if k >= d.len() {
        let n = nd::numel(d);
        return (d.to_vec(), (0..n).map(|i| vec![i]).collect());
    }
    let stride: usize = d[..k].iter().product();
    let len = d[k];
    let outer: usize = d[k + 1..].iter().product();
    let mut out_dims = d.to_vec();
    out_dims[k] = 1;
    let mut out = Vec::with_capacity(stride * outer);
    for o in 0..outer {
        for i in 0..stride {
            out.push((0..len).map(|j| o * stride * len + j * stride + i).collect());
        }
    }
    (nd::normalize(out_dims), out)
}

/// Applies `f` to each slice along `dim`.
fn reduce(m: &Matrix, dim: usize, f: impl Fn(&[f64]) -> f64) -> Matrix {
    if m.dims == [0, 0] {
        return Matrix::scalar(f(&[]));
    }
    let (dims, sl) = slices(&m.dims, dim);
    let mut buf = Vec::new();
    let data = sl
        .iter()
        .map(|s| {
            buf.clear();
            buf.extend(s.iter().map(|&p| m.data[p]));
            f(&buf)
        })
        .collect();
    Matrix::from_dims(dims, data)
}

fn reduce_c(m: &Matrix, dim: usize, f: impl Fn(&[C]) -> C) -> Matrix {
    let (dims, sl) = slices(&m.dims, dim);
    let mut re = Vec::with_capacity(sl.len());
    let mut im = Vec::with_capacity(sl.len());
    for s in &sl {
        let v: Vec<C> = s.iter().map(|&p| C::new(m.data[p], m.im_at(p))).collect();
        let z = f(&v);
        re.push(z.re);
        im.push(z.im);
    }
    Matrix::complex(dims, re, im)
}

fn cumulate(m: &Matrix, dim: usize, f: impl Fn(f64, f64) -> f64) -> Matrix {
    let mut out = m.clone();
    out.kind = if m.kind.is_int() || m.kind == Kind::Single { m.kind } else { Kind::Double };
    let (_, sl) = slices(&m.dims, dim);
    for s in sl {
        for w in 1..s.len() {
            out.data[s[w]] = out.kind.cast(f(out.data[s[w - 1]], m.data[s[w]]));
        }
    }
    out
}

fn median_of(v: &[f64]) -> f64 {
    if v.is_empty() || v.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = s.len();
    if n % 2 == 1 { s[n / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 }
}

fn mode_of(v: &[f64]) -> f64 {
    let mut s: Vec<f64> = v.iter().copied().filter(|x| !x.is_nan()).collect();
    if s.is_empty() {
        return f64::NAN;
    }
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (mut best, mut best_n, mut i) = (s[0], 0, 0);
    while i < s.len() {
        let mut j = i;
        while j < s.len() && s[j] == s[i] {
            j += 1;
        }
        if j - i > best_n {
            best_n = j - i;
            best = s[i];
        }
        i = j;
    }
    best
}

fn variance(v: &[f64], w: f64) -> f64 {
    let n = v.len();
    if n == 0 {
        return f64::NAN;
    }
    if n == 1 {
        return 0.0;
    }
    let mean = v.iter().sum::<f64>() / n as f64;
    v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / if w == 1.0 { n as f64 } else { (n - 1) as f64 }
}

fn gcd(a: f64, b: f64) -> f64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b > 0.5 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

fn is_prime(n: f64) -> bool {
    if n < 2.0 || n.fract() != 0.0 {
        return false;
    }
    let n = n as u64;
    if n < 4 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }
    let mut i = 3;
    while i * i <= n {
        if n % i == 0 {
            return false;
        }
        i += 2;
    }
    true
}

pub fn num2str_scalar(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 {
        return format!("{}", v as i64);
    }
    if !v.is_finite() {
        return fmt_g(v, 5);
    }
    let digits = (v.abs().log10().floor().max(0.0) as usize) + 5;
    fmt_g(v, digits)
}

pub(crate) fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x.dims == y.dims && x.data == y.data && (0..x.numel()).all(|i| x.im_at(i) == y.im_at(i)),
        (Value::Cell(x), Value::Cell(y)) => x.dims == y.dims && x.data.iter().zip(&y.data).all(|(p, q)| values_equal(p, q)),
        (Value::Struct(x), Value::Struct(y)) => {
            x.dims == y.dims
                && x.fields.len() == y.fields.len()
                && x.fields.iter().all(|f| y.field_index(f).is_some())
                && (0..x.elems.len()).all(|i| x.fields.iter().all(|f| values_equal(x.get_at(i, f).unwrap(), y.get_at(i, f).unwrap())))
        }
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Str(x), y) | (y, Value::Str(x)) if y.is_char() && x.is_scalar() => y.text().map(|t| x.data[0].as_deref() == Some(t.as_str())).unwrap_or(false),
        (Value::Table(x), Value::Table(y)) => x.names == y.names && x.vars.iter().zip(&y.vars).all(|(p, q)| values_equal(p, q)),
        _ => a == b,
    }
}

/// Elementwise math with MATLAB's complex extensions of the real domain.
pub(crate) fn unary_math(name: &str, m: &Matrix) -> Option<R<Matrix>> {
    let real: Option<fn(f64) -> f64> = match name {
        "floor" => Some(f64::floor),
        "ceil" => Some(f64::ceil),
        "fix" => Some(f64::trunc),
        "sign" => Some(|x: f64| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else if x == 0.0 { 0.0 } else { f64::NAN }),
        // Exact zeros and infinities at multiples of 90 degrees, signed as MATLAB signs them
        // (sind(-180) is -0, tand(180) is -0, tand(270) is -Inf).
        "sind" => Some(|x: f64| if (x % 180.0) == 0.0 { 0.0f64.copysign(x) } else { x.to_radians().sin() }),
        "cosd" => Some(|x: f64| if ((x - 90.0) % 180.0) == 0.0 { 0.0 } else { x.to_radians().cos() }),
        "tand" => Some(|x: f64| match x.rem_euclid(360.0) {
            _ if x == 0.0 => x,
            r if r == 0.0 => 0.0,
            r if r == 180.0 => -0.0,
            r if r == 90.0 => f64::INFINITY,
            r if r == 270.0 => f64::NEG_INFINITY,
            _ => x.to_radians().tan(),
        }),
        "asind" => Some(|x: f64| x.asin().to_degrees()),
        "acosd" => Some(|x: f64| x.acos().to_degrees()),
        "atand" => Some(|x: f64| x.atan().to_degrees()),
        "deg2rad" => Some(f64::to_radians),
        "rad2deg" => Some(f64::to_degrees),
        "gamma" => Some(gamma),
        "factorial" => Some(|x: f64| gamma(x + 1.0).round()),
        "expm1" => Some(f64::exp_m1),
        "log1p" => Some(f64::ln_1p),
        _ => None,
    };
    const COMPLEX_MATH: &[&str] = &[
        "sin", "cos", "tan", "sinh", "cosh", "tanh", "exp", "log", "sqrt", "asin", "acos", "atan", "log10", "log2", "asinh", "acosh", "atanh", "abs",
        "round",
    ];
    if real.is_none() && !COMPLEX_MATH.contains(&name) {
        return None;
    }
    let int_ok = matches!(name, "abs" | "sign" | "floor" | "ceil" | "fix" | "round");
    if m.kind.is_int() && !int_ok {
        return Some(rt(format!("Undefined function '{name}' for input arguments of type '{}'.", m.kind.name())));
    }
    let keep = |out: Matrix| -> Matrix {
        if m.kind.is_int() || m.kind == Kind::Single { out.cast_to(m.kind) } else { out }
    };
    if let Some(f) = real {
        if m.is_complex() && matches!(name, "floor" | "ceil" | "fix") {
            let mut out = m.map(f);
            out.im = m.im.as_ref().map(|im| im.iter().map(|&x| f(x)).collect());
            return Some(Ok(keep(out)));
        }
        return Some(Ok(keep(m.map(f))));
    }
    let cf: fn(C) -> C = match name {
        "sin" => C::sin,
        "cos" => C::cos,
        "tan" => C::tan,
        "sinh" => C::sinh,
        "cosh" => C::cosh,
        "tanh" => C::tanh,
        "exp" => C::exp,
        "log" => C::ln,
        "sqrt" => C::sqrt,
        "asin" => C::asin,
        "acos" => C::acos,
        "atan" => C::atan,
        "log10" => |z: C| {
            let l = z.ln();
            C::new(l.re / std::f64::consts::LN_10, l.im / std::f64::consts::LN_10)
        },
        "log2" => |z: C| {
            let l = z.ln();
            C::new(l.re / std::f64::consts::LN_2, l.im / std::f64::consts::LN_2)
        },
        "asinh" => |z: C| z.add(z.mul(z).add(C::new(1.0, 0.0)).sqrt()).ln(),
        "acosh" => |z: C| z.add(z.add(C::new(1.0, 0.0)).sqrt().mul(z.sub(C::new(1.0, 0.0)).sqrt())).ln(),
        "atanh" => |z: C| {
            let a = C::new(1.0, 0.0).add(z).ln().sub(C::new(1.0, 0.0).sub(z).ln());
            C::new(a.re / 2.0, a.im / 2.0)
        },
        "abs" => {
            if !m.is_complex() {
                return Some(Ok(keep(m.map(f64::abs))));
            }
            return Some(Ok(Matrix::from_dims(m.dims.clone(), (0..m.numel()).map(|i| C::new(m.data[i], m.im_at(i)).abs()).collect())));
        }
        "round" => {
            let mut out = m.map(f64::round);
            out.im = m.im.as_ref().map(|im| im.iter().map(|x| x.round()).collect());
            return Some(Ok(keep(out)));
        }
        _ => return None,
    };
    // Real inputs stay on the real fast path unless the result leaves the real line.
    if !m.is_complex() {
        let outside = |x: f64| match name {
            "sqrt" | "log" | "log10" | "log2" => x < 0.0,
            "asin" | "acos" | "atanh" => x.abs() > 1.0,
            "acosh" => x < 1.0,
            _ => false,
        };
        let leaves = m.data.iter().any(|&x| outside(x));
        {
            let f: fn(f64) -> f64 = match name {
                "sin" => f64::sin,
                "cos" => f64::cos,
                "tan" => f64::tan,
                "sinh" => f64::sinh,
                "cosh" => f64::cosh,
                "tanh" => f64::tanh,
                "exp" => f64::exp,
                "log" => f64::ln,
                "sqrt" => f64::sqrt,
                "asin" => f64::asin,
                "acos" => f64::acos,
                "atan" => f64::atan,
                "log10" => f64::log10,
                "log2" => f64::log2,
                "asinh" => f64::asinh,
                "acosh" => f64::acosh,
                _ => f64::atanh,
            };
            if !leaves {
                return Some(Ok(keep(m.map(f))));
            }
            // A complex result, but elements inside the real domain stay exact, and those
            // outside take MATLAB's closed form on the branch cut.
            let (re, im): (Vec<f64>, Vec<f64>) =
                m.data.iter().map(|&x| if outside(x) { real_branch(name, x) } else { (f(x), 0.0) }).unzip();
            return Some(Ok(keep(Matrix::complex(m.dims.clone(), re, im))));
        }
    }
    Some(Ok(keep(map_c(m, cf))))
}

/// The complex value of an inverse or logarithmic function at a real `x` outside its real
/// domain, in MATLAB's branch (real inputs approach the cuts from above).
fn real_branch(name: &str, x: f64) -> (f64, f64) {
    use std::f64::consts::{FRAC_PI_2, LN_10, LN_2, PI};
    match name {
        "sqrt" => (0.0, (-x).sqrt()),
        "log" => ((-x).ln(), PI),
        "log10" => ((-x).log10(), PI / LN_10),
        "log2" => ((-x).log2(), PI / LN_2),
        "asin" if x > 0.0 => (FRAC_PI_2, -x.acosh()),
        "asin" => (-FRAC_PI_2, (-x).acosh()),
        "acos" if x > 0.0 => (0.0, x.acosh()),
        "acos" => (PI, -(-x).acosh()),
        "acosh" if x >= -1.0 => (0.0, x.acos()),
        "acosh" => ((-x).acosh(), PI),
        // atanh, |x| > 1.
        _ if x.is_infinite() => (0.0f64.copysign(x), FRAC_PI_2.copysign(x)),
        // atanh(1/x) by Kahan's form, accurate also for large |x|.
        _ => (0.25 * (4.0 * x / ((x - 1.0) * (x - 1.0))).ln_1p(), FRAC_PI_2.copysign(x)),
    }
}

impl Interpreter {
    fn next_rand(&mut self) -> f64 {
        let mut x = self.rng_state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng_state = x;
        let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        ((r >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    fn next_randn(&mut self) -> f64 {
        let u1 = self.next_rand();
        let u2 = self.next_rand();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }

    pub(crate) fn call_func_value(&mut self, f: &Value, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        match f {
            Value::Func(fv) => self.call_value(fv, args, nargout),
            v if v.is_text() => {
                let name = v.text()?;
                self.call_function(&name, args, nargout)
            }
            _ => rt("Expected a function handle."),
        }
    }
}

/// Element `k` of any array as its own 1×1 value (for arrayfun/cellfun).
fn element(v: &Value, k: usize, unwrap_cells: bool) -> R<Value> {
    match v {
        Value::Cell(c) if unwrap_cells => Ok(c.data[k].clone()),
        Value::Func(_) | Value::Object(_) => Ok(v.clone()),
        other => indexing::index(other, &[IndexArg::scalar(k)]),
    }
}

pub fn call(it: &mut Interpreter, name: &str, args: Vec<Value>, nargout: usize) -> Ret {
    // Type families first.
    if args.iter().any(|a| matches!(a, Value::Sym(_))) || crate::sym::NAMES.contains(&name) {
        if let Some(r) = crate::sym::call(it, name, args.clone(), nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::time::NAMES.contains(&name) {
        if let Some(r) = crate::time::call(name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::table::NAMES.contains(&name) {
        let names = std::mem::take(&mut it.arg_names);
        if let Some(r) = crate::table::call(&names, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if let Some(r) = crate::builtins_elfun::call(name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_ops::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_text::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_plot::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(Value::Time(t)) = args.first() {
        if let Some(r) = time_reduction(name, t, &args)? {
            return Ok(Some(r));
        }
    }
    let a = &args;

    // Elementwise math.
    if let Some(Value::Num(m)) = a.first() {
        if a.len() == 1 || matches!(name, "round") && a.len() == 1 {
            if let Some(r) = unary_math(name, m) {
                return mat(r?);
            }
        }
    }

    match name {
        // ---- constants ---------------------------------------------------------------
        "pi" => num(std::f64::consts::PI),
        "eps" => match a.first() {
            Some(Value::Num(m)) if m.kind != Kind::Char => {
                let x = m.data.first().copied().unwrap_or(1.0).abs();
                num(if x == 0.0 { f64::from_bits(1) } else { 2f64.powf(x.log2().floor() - 52.0) })
            }
            _ => num(f64::EPSILON),
        },
        "i" | "j" => one(Value::Num(Matrix::complex_scalar(0.0, 1.0))),
        "Inf" | "inf" | "NaN" | "nan" => {
            let (rest, kind) = class_option(a)?;
            let d = size_args(&rest)?;
            let v = if name.eq_ignore_ascii_case("inf") { f64::INFINITY } else { f64::NAN };
            mat(Matrix::filled_dims(d, v).cast_to(kind))
        }
        "true" | "false" => {
            let d = size_args(a)?;
            mat(Matrix::filled_dims(d, if name == "true" { 1.0 } else { 0.0 }).with_kind(Kind::Logical))
        }
        "intmax" | "intmin" => {
            let cls = a.first().map(|v| v.text()).transpose()?.unwrap_or_else(|| "int32".into());
            let Some(c) = IntClass::from_name(&cls) else { return rt(format!("Invalid class name '{cls}'.")) };
            let (lo, hi) = c.range();
            mat(Matrix::scalar(if name == "intmax" { hi } else { lo }).with_kind(Kind::Int(c)))
        }
        "realmax" | "realmin" | "flintmax" => {
            let single = a.first().map(|v| v.text().map(|t| t == "single").unwrap_or(false)).unwrap_or(false);
            let v = match (name, single) {
                ("realmax", false) => f64::MAX,
                ("realmax", true) => f32::MAX as f64,
                ("realmin", false) => f64::MIN_POSITIVE,
                ("realmin", true) => f32::MIN_POSITIVE as f64,
                (_, false) => 9_007_199_254_740_992.0,
                _ => 16_777_216.0,
            };
            mat(Matrix::scalar(v).cast_to(if single { Kind::Single } else { Kind::Double }))
        }
        "newline" => one(Value::chars("\n")),

        // ---- construction ------------------------------------------------------------
        "zeros" | "ones" | "eye" => {
            let (rest, kind) = class_option(a)?;
            let d = size_args(&rest)?;
            let mut m = Matrix::filled_dims(d.clone(), if name == "ones" { 1.0 } else { 0.0 });
            if name == "eye" {
                let (r, c) = (d[0], d[1..].iter().product::<usize>());
                for i in 0..r.min(c) {
                    m.set(i, i, 1.0);
                }
            }
            mat(m.cast_to(kind))
        }
        "rand" | "randn" => {
            let (rest, kind) = class_option(a)?;
            let d = size_args(&rest)?;
            let n = nd::numel(&d);
            let data = (0..n).map(|_| if name == "rand" { it.next_rand() } else { it.next_randn() }).collect();
            mat(Matrix::from_dims(d, data).cast_to(kind))
        }
        "randi" => {
            let lim = m_arg(a, 0, name)?;
            let (lo, hi) = if lim.numel() >= 2 { (lim.data[0], lim.data[1]) } else { (1.0, lim.data[0]) };
            let (rest, kind) = class_option(&a[1..])?;
            let d = size_args(&rest)?;
            let n = nd::numel(&d);
            let data = (0..n).map(|_| (lo + (it.next_rand() * (hi - lo + 1.0)).floor()).min(hi)).collect();
            mat(Matrix::from_dims(d, data).cast_to(kind))
        }
        "randperm" => {
            let n = arg(a, 0, name)?.as_usize()?;
            let mut v: Vec<f64> = (1..=n).map(|x| x as f64).collect();
            for i in (1..n).rev() {
                let j = (it.next_rand() * (i + 1) as f64) as usize;
                v.swap(i, j.min(i));
            }
            if let Some(k) = a.get(1) {
                v.truncate(k.as_usize()?);
            }
            mat(Matrix::row(v))
        }
        "rng" => {
            if let Some(v) = a.first() {
                if !v.is_text() {
                    it.rng_state = (v.scalar()? as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
                } else if v.text()? == "default" {
                    it.rng_state = 0x2545_F491_4F6C_DD1D;
                }
            }
            none()
        }
        "linspace" => {
            if let Some(Value::Time(t)) = a.first() {
                let Value::Time(t2) = arg(a, 1, name)? else { return rt("linspace endpoints must both be datetimes.") };
                let n = match a.get(2) {
                    Some(v) => v.as_usize()?,
                    None => 100,
                };
                let (x0, x1) = (t.data[0][0], t2.data[0][0]);
                let ms = (0..n).map(|i| if n == 1 { x1 } else { x0 + (x1 - x0) * i as f64 / (n - 1) as f64 }).collect();
                return one(Value::Time(t.with_ms(vec![1, n], ms)));
            }
            let lo = arg(a, 0, name)?.scalar()?;
            let hi = arg(a, 1, name)?.scalar()?;
            let n = match a.get(2) {
                Some(v) => v.scalar()?.floor().max(0.0) as usize,
                None => 100,
            };
            let data = if n == 1 { vec![hi] } else { (0..n).map(|i| lo + (hi - lo) * i as f64 / (n - 1) as f64).collect() };
            mat(Matrix::row(data))
        }
        "logspace" => {
            let lo = arg(a, 0, name)?.scalar()?;
            let hi = arg(a, 1, name)?.scalar()?;
            let n = match a.get(2) {
                Some(v) => v.as_usize()?,
                None => 50,
            };
            mat(Matrix::row((0..n).map(|i| 10f64.powf(lo + (hi - lo) * i as f64 / (n.max(2) - 1) as f64)).collect()))
        }
        "colon" => {
            let lo = arg(a, 0, name)?.scalar()?;
            let (step, hi) = if a.len() >= 3 { (a[1].scalar()?, a[2].scalar()?) } else { (1.0, arg(a, 1, name)?.scalar()?) };
            let n = range_count(lo, step, hi);
            mat(Matrix::row((0..n).map(|i| lo + i as f64 * step).collect()))
        }
        "meshgrid" | "ndgrid" => {
            let x = m_arg(a, 0, name)?;
            let y = if a.len() > 1 { m_arg(a, 1, name)? } else { x.clone() };
            let (gx, gy) = if name == "meshgrid" { (x, y) } else { (y, x) };
            let (nr, nc) = (gy.numel(), gx.numel());
            let mut xx = Matrix::zeros(nr, nc);
            let mut yy = Matrix::zeros(nr, nc);
            for c in 0..nc {
                for r in 0..nr {
                    xx.set(r, c, gx.data[c]);
                    yy.set(r, c, gy.data[r]);
                }
            }
            if name == "ndgrid" {
                return Ok(Some(vec![Value::Num(yy.transpose()), Value::Num(xx.transpose())]));
            }
            Ok(Some(vec![Value::Num(xx), Value::Num(yy)]))
        }
        "repmat" => {
            let v = arg(a, 0, name)?;
            let reps = size_args(&a[1..])?;
            let src = v.dims();
            let n = src.len().max(reps.len());
            let pad = |d: &[usize]| {
                let mut x = d.to_vec();
                x.resize(n, 1);
                x
            };
            let (sd, rd) = (pad(&src), pad(&reps));
            // Build by concatenating copies along each dimension.
            let mut cur = v.clone();
            for k in 0..n {
                if rd[k] != 1 {
                    let copies = vec![cur.clone(); rd[k]];
                    cur = if rd[k] == 0 {
                        let mut d = cur.dims();
                        d.resize(n, 1);
                        d[k] = 0;
                        indexing::index(&cur, &[IndexArg::List { ix: vec![], dims: vec![0, 0], from_mask: false }])?
                    } else {
                        indexing::concat(copies, k)?
                    };
                }
            }
            let _ = sd;
            one(cur)
        }
        "reshape" => {
            let v = arg(a, 0, name)?;
            let n = v.numel();
            let mut dims: Vec<Option<usize>> = Vec::new();
            if a.len() == 2 {
                for &d in &m_arg(a, 1, name)?.data {
                    dims.push(Some(d as usize));
                }
            } else {
                for x in &a[1..] {
                    match x {
                        Value::Num(m) if m.is_empty() => dims.push(None),
                        other => dims.push(Some(other.as_usize()?)),
                    }
                }
            }
            let known: usize = dims.iter().flatten().product();
            let dims: Dims = dims.iter().map(|d| d.unwrap_or(if known == 0 { 0 } else { n / known })).collect();
            if nd::numel(&dims) != n {
                return rt("Number of elements must not change. Use [] as one of the size inputs to automatically calculate the appropriate size for that dimension.");
            }
            let dims = nd::normalize(dims);
            one(match v {
                Value::Num(m) => Value::Num(m.clone().reshaped(dims)),
                Value::Cell(c) => Value::Cell(CellArray::from_dims(dims, c.data.clone())),
                Value::Str(s) => Value::Str(StrArray::from_dims(dims, s.data.clone())),
                Value::Struct(s) => Value::Struct(StructArray { dims, ..s.clone() }),
                Value::Sym(s) => Value::Sym(crate::sym::SymArray::from_dims(dims, s.data.clone())),
                Value::Time(t) => Value::Time(TimeArray { dims, ..t.clone() }),
                other => return rt(format!("reshape is not supported for class '{}'.", other.class_name())),
            })
        }
        "permute" => {
            let v = arg(a, 0, name)?;
            let order: Vec<usize> = m_arg(a, 1, name)?.data.iter().map(|&x| (x as usize).saturating_sub(1)).collect();
            one(match v {
                Value::Num(m) => {
                    let (d, re) = nd::permute(&m.dims, &m.data, &order)?;
                    let mut out = Matrix::from_dims(d.clone(), re).with_kind(m.kind);
                    if let Some(im) = &m.im {
                        out.im = Some(nd::permute(&m.dims, im, &order)?.1);
                    }
                    Value::Num(out)
                }
                Value::Cell(c) => {
                    let (d, data) = nd::permute(&c.dims, &c.data, &order)?;
                    Value::Cell(CellArray::from_dims(d, data))
                }
                other => return rt(format!("permute is not supported for class '{}'.", other.class_name())),
            })
        }
        "squeeze" => {
            let v = arg(a, 0, name)?;
            let d = v.dims();
            if d.len() <= 2 {
                return one(v.clone());
            }
            let nd2: Dims = d.into_iter().filter(|&x| x != 1).collect();
            let call_args = vec![v.clone(), Value::Num(Matrix::row(nd::normalize(nd2).iter().map(|&x| x as f64).collect()))];
            call(it, "reshape", call_args, 1)
        }
        "cell" => {
            let d = size_args(a)?;
            let n = nd::numel(&d);
            one(Value::Cell(CellArray::from_dims(d, vec![Value::empty(); n])))
        }
        "struct" => {
            if a.is_empty() {
                return one(Value::Struct(StructArray::scalar()));
            }
            // Cell values create a struct array of the cells' size (MATLAB semantics).
            let mut fields = Vec::new();
            let mut values = Vec::new();
            for pair in a.chunks(2) {
                fields.push(pair[0].text()?);
                values.push(pair.get(1).cloned().unwrap_or_else(Value::empty));
            }
            let mut dims: Option<Dims> = None;
            for v in &values {
                if let Value::Cell(c) = v {
                    if c.data.len() != 1 {
                        match &dims {
                            Some(d) if *d != c.dims => return rt("Array dimensions of input cell arrays must match."),
                            _ => dims = Some(c.dims.clone()),
                        }
                    }
                }
            }
            let dims = dims.unwrap_or_else(|| vec![1, 1]);
            let n = nd::numel(&dims);
            let elems = (0..n)
                .map(|k| {
                    values
                        .iter()
                        .map(|v| match v {
                            Value::Cell(c) if c.data.len() == 1 => c.data[0].clone(),
                            Value::Cell(c) => c.data[k].clone(),
                            other => other.clone(),
                        })
                        .collect()
                })
                .collect();
            one(Value::Struct(StructArray { dims, fields, elems }))
        }
        "blanks" => one(Value::chars(&" ".repeat(arg(a, 0, name)?.as_usize()?))),
        "diag" => {
            let m = m_arg(a, 0, name)?;
            let k = match a.get(1) {
                Some(v) => v.scalar()? as i64,
                None => 0,
            };
            if m.is_vector() {
                let n = m.numel() + k.unsigned_abs() as usize;
                let mut out = Matrix::zeros(n, n).with_kind(m.kind);
                for i in 0..m.numel() {
                    let (r, c) = if k >= 0 { (i, i + k as usize) } else { (i + (-k) as usize, i) };
                    out.set(r, c, m.data[i]);
                }
                mat(out)
            } else {
                let mut d = Vec::new();
                for i in 0..m.rows() {
                    let c = i as i64 + k;
                    if c >= 0 && (c as usize) < m.cols() {
                        d.push(m.get(i, c as usize));
                    }
                }
                mat(Matrix::col(d).with_kind(m.kind))
            }
        }
        "triu" | "tril" => {
            let m = m_arg(a, 0, name)?;
            let k = match a.get(1) {
                Some(v) => v.scalar()? as i64,
                None => 0,
            };
            let mut out = m.clone();
            for c in 0..m.cols() {
                for r in 0..m.rows() {
                    let keep = if name == "triu" { c as i64 - r as i64 >= k } else { c as i64 - r as i64 <= k };
                    if !keep {
                        out.set(r, c, 0.0);
                    }
                }
            }
            mat(out)
        }
        "kron" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            let (r1, c1, r2, c2) = (x.rows(), x.cols(), y.rows(), y.cols());
            let mut out = Matrix::zeros(r1 * r2, c1 * c2);
            for i in 0..r1 {
                for j in 0..c1 {
                    for k in 0..r2 {
                        for l in 0..c2 {
                            out.set(i * r2 + k, j * c2 + l, x.get(i, j) * y.get(k, l));
                        }
                    }
                }
            }
            mat(out)
        }
        "horzcat" => one(hconcat(args)?),
        "vertcat" => one(vconcat(args)?),
        "cat" => {
            let d = arg(a, 0, name)?.as_usize()?;
            if d == 0 {
                return rt("Dimension must be a positive integer.");
            }
            one(indexing::concat(args[1..].to_vec(), d - 1)?)
        }
        "complex" => {
            let re = m_arg(a, 0, name)?;
            let im = match a.get(1) {
                Some(_) => m_arg(a, 1, name)?,
                None => Matrix::filled_dims(re.dims.clone(), 0.0),
            };
            let r = broadcast(&re, &im, |x, _| x)?;
            let i = broadcast(&re, &im, |_, y| y)?;
            // complex() keeps an all-zero imaginary part (unlike arithmetic results).
            let mut m = Matrix::from_dims(r.dims, r.data);
            m.im = Some(i.data);
            mat(m)
        }

        // ---- complex parts -------------------------------------------------------------
        "real" => {
            let m = m_arg(a, 0, name)?;
            mat(Matrix::from_dims(m.dims.clone(), m.data.clone()).with_kind(if m.kind == Kind::Logical || m.kind == Kind::Char { Kind::Double } else { m.kind }))
        }
        "imag" => {
            let m = m_arg(a, 0, name)?;
            mat(Matrix::from_dims(m.dims.clone(), m.im.clone().unwrap_or_else(|| vec![0.0; m.numel()])).with_kind(if m.kind.is_int() || m.kind == Kind::Single { m.kind } else { Kind::Double }))
        }
        "conj" => {
            let mut m = m_arg(a, 0, name)?;
            if let Some(im) = m.im.as_mut() {
                im.iter_mut().for_each(|x| *x = -*x);
            }
            mat(m)
        }
        "angle" => {
            let m = m_arg(a, 0, name)?;
            mat(Matrix::from_dims(m.dims.clone(), (0..m.numel()).map(|i| m.im_at(i).atan2(m.data[i])).collect()))
        }
        "isreal" => one(Value::bool(!matches!(arg(a, 0, name)?, Value::Num(m) if m.is_complex()))),
        "iscomplex" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Num(m) if m.is_complex()))),

        // ---- numeric classes -----------------------------------------------------------
        "double" | "single" | "int8" | "int16" | "int32" | "int64" | "uint8" | "uint16" | "uint32" | "uint64" | "logical" => {
            let kind = Kind::from_name(name).unwrap();
            match arg(a, 0, name)? {
                Value::Num(m) => {
                    if kind == Kind::Logical {
                        if m.data.iter().any(|x| x.is_nan()) {
                            return rt("NaN's cannot be converted to logicals.");
                        }
                        if m.is_complex() {
                            return rt("Conversion to logical from complex is not possible.");
                        }
                    }
                    mat(m.clone().cast_to(kind))
                }
                Value::Str(s) if kind == Kind::Double => mat(Matrix::from_dims(s.dims.clone(), s.data.iter().map(|x| x.as_deref().and_then(|t| t.trim().parse().ok()).unwrap_or(f64::NAN)).collect())),
                Value::Time(t) if kind == Kind::Double => mat(crate::time::to_numeric(t)),
                other => rt(format!("Conversion to {name} from {} is not possible.", other.class_name())),
            }
        }
        "cast" => {
            let cls = arg(a, 1, name)?.text()?;
            call(it, &cls, vec![arg(a, 0, name)?.clone()], 1)
        }
        "class" => one(Value::chars(&arg(a, 0, name)?.class_name())),
        "isa" => {
            let cls = arg(a, 1, name)?.text()?;
            let v = arg(a, 0, name)?;
            let hit = match v {
                Value::Object(o) => o.class.is_a(&cls),
                Value::Num(m) => {
                    m.kind.name() == cls
                        || (cls == "numeric" && !matches!(m.kind, Kind::Logical | Kind::Char))
                        || (cls == "float" && m.kind.is_float())
                        || (cls == "integer" && m.kind.is_int())
                }
                other => other.class_name() == cls,
            };
            one(Value::bool(hit))
        }
        "isnumeric" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Num(m) if !matches!(m.kind, Kind::Logical | Kind::Char)))),
        "isfloat" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Num(m) if m.kind.is_float()))),
        "isinteger" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Num(m) if m.kind.is_int()))),
        "islogical" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Num(m) if m.kind == Kind::Logical))),
        "iscell" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Cell(_)))),
        "isstruct" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Struct(_)))),
        "isobject" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Object(_)))),
        "isstring" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Str(_)))),
        "idivide" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            let kind = ops::arith_kind(x.kind, y.kind)?;
            if !kind.is_int() {
                return rt("At least one argument must belong to an integer class.");
            }
            let mode = a.get(2).map(|v| v.text()).transpose()?.unwrap_or_else(|| "fix".into());
            let f = match mode.as_str() {
                "floor" => f64::floor,
                "ceil" => f64::ceil,
                "round" => f64::round,
                _ => f64::trunc,
            };
            mat(broadcast(&x, &y, |p, q| f(p / q))?.cast_to(kind))
        }

        // ---- size & shape --------------------------------------------------------------
        "size" => {
            let v = arg(a, 0, name)?;
            let d = v.dims();
            if let Some(k) = a.get(1) {
                let ks = k.to_matrix()?;
                let vals: Vec<f64> = ks.data.iter().map(|&x| *d.get(x as usize - 1).unwrap_or(&1) as f64).collect();
                return if vals.len() == 1 { num(vals[0]) } else { mat(Matrix::row(vals)) };
            }
            if nargout >= 2 {
                let mut out: Vec<Value> = (0..nargout).map(|k| Value::num(*d.get(k).unwrap_or(&1) as f64)).collect();
                // The last output folds the remaining dimensions.
                if nargout < d.len() {
                    out[nargout - 1] = Value::num(d[nargout - 1..].iter().product::<usize>() as f64);
                }
                return Ok(Some(out));
            }
            mat(Matrix::row(d.iter().map(|&x| x as f64).collect()))
        }
        "numel" => num(arg(a, 0, name)?.numel() as f64),
        "length" => {
            let d = arg(a, 0, name)?.dims();
            num(if d.iter().any(|&x| x == 0) { 0.0 } else { *d.iter().max().unwrap() as f64 })
        }
        "ndims" => num(arg(a, 0, name)?.dims().len() as f64),
        "isempty" => one(Value::bool(match arg(a, 0, name)? {
            Value::Func(_) | Value::Object(_) => false,
            Value::Table(t) => t.nrows == 0 || t.vars.is_empty(),
            v => v.numel() == 0,
        })),
        "isscalar" => one(Value::bool(arg(a, 0, name)?.numel() == 1)),
        "isvector" => {
            let d = arg(a, 0, name)?.dims();
            one(Value::bool(d.len() == 2 && (d[0] == 1 || d[1] == 1) && d[0] * d[1] >= 1))
        }
        "isrow" => {
            let d = arg(a, 0, name)?.dims();
            one(Value::bool(d.len() == 2 && d[0] == 1))
        }
        "iscolumn" => {
            let d = arg(a, 0, name)?.dims();
            one(Value::bool(d.len() == 2 && d[1] == 1))
        }
        "ismatrix" => one(Value::bool(arg(a, 0, name)?.dims().len() == 2)),
        "isdiag" | "istriu" | "istril" => {
            let m = m_arg(a, 0, name)?;
            let ok = (0..m.cols()).all(|c| (0..m.rows()).all(|r| {
                let v = m.get(r, c);
                match name {
                    "isdiag" => r == c || v == 0.0,
                    "istriu" => r <= c || v == 0.0,
                    _ => r >= c || v == 0.0,
                }
            }));
            one(Value::bool(ok))
        }
        "issorted" => {
            let m = m_arg(a, 0, name)?;
            one(Value::bool(m.data.windows(2).all(|w| w[0] <= w[1])))
        }
        "isfield" => {
            let Value::Struct(s) = arg(a, 0, name)? else { return one(Value::bool(false)) };
            match arg(a, 1, name)? {
                Value::Cell(c) => mat(Matrix::from_dims(c.dims.clone(), c.data.iter().map(|f| f.text().map(|t| s.field_index(&t).is_some()).unwrap_or(false) as u8 as f64).collect()).with_kind(Kind::Logical)),
                f => one(Value::bool(s.field_index(&f.text()?).is_some())),
            }
        }
        "isnan" => mat(bool_mat(&m_arg(a, 0, name)?, f64::is_nan)),
        "isinf" => mat(bool_mat(&m_arg(a, 0, name)?, f64::is_infinite)),
        "isfinite" => {
            let m = m_arg(a, 0, name)?;
            mat(Matrix::from_dims(m.dims.clone(), (0..m.numel()).map(|i| (m.data[i].is_finite() && m.im_at(i).is_finite()) as u8 as f64).collect()).with_kind(Kind::Logical))
        }
        "isprime" => mat(bool_mat(&m_arg(a, 0, name)?, is_prime)),
        "isequal" => {
            let first = arg(a, 0, name)?;
            one(Value::bool(a[1..].iter().all(|b| values_equal(first, b))))
        }
        "exist" => {
            let n = arg(a, 0, name)?.text()?;
            num(if it.get_var(&n).is_some() { 1.0 } else if exists(&n) { 5.0 } else if it.has_function(&n) { 2.0 } else { 0.0 })
        }
        "fieldnames" | "properties" => match arg(a, 0, name)? {
            Value::Struct(s) => one(Value::Cell(CellArray::col(s.fields.iter().map(|n| Value::chars(n)).collect()))),
            Value::Object(o) => one(Value::Cell(CellArray::col(o.class.props.iter().filter(|p| !p.hidden).map(|p| Value::chars(&p.name)).collect()))),
            Value::Table(t) => one(Value::Cell(CellArray::col(t.names.iter().map(|n| Value::chars(n)).collect()))),
            v if v.is_text() => {
                let cls = v.text()?;
                match it.find_class(&cls) {
                    Some(c) => one(Value::Cell(CellArray::col(c.props.iter().filter(|p| !p.hidden).map(|p| Value::chars(&p.name)).collect()))),
                    None => rt(format!("No class '{cls}'.")),
                }
            }
            _ => rt("Invalid input argument. Input must be a structure or an object."),
        },
        "methods" => {
            let class = match arg(a, 0, name)? {
                Value::Object(o) => o.class.clone(),
                v => {
                    let cls = v.text()?;
                    it.find_class(&cls).ok_or_else(|| LangError::new(format!("No class '{cls}'."), 0))?
                }
            };
            let mut ms: Vec<String> = class.methods.keys().filter(|m| !m.contains('.')).cloned().collect();
            ms.push(class.name.clone());
            ms.sort();
            ms.dedup();
            one(Value::Cell(CellArray::col(ms.iter().map(|m| Value::chars(m)).collect())))
        }
        "isprop" | "ismethod" => {
            let Value::Object(o) = arg(a, 0, name)? else { return one(Value::bool(false)) };
            let n = arg(a, 1, name)?.text()?;
            one(Value::bool(if name == "isprop" { o.class.prop_index(&n).is_some() } else { o.class.method(&n).is_some() }))
        }
        "rmfield" => match arg(a, 0, name)? {
            Value::Struct(s) => {
                let mut s = s.clone();
                let names: Vec<String> = match arg(a, 1, name)? {
                    Value::Cell(c) => c.data.iter().map(|v| v.text()).collect::<R<_>>()?,
                    v => vec![v.text()?],
                };
                for f in names {
                    if !s.remove_field(&f) {
                        return rt(format!("A field named '{f}' doesn't exist."));
                    }
                }
                one(Value::Struct(s))
            }
            _ => rt("rmfield requires a struct."),
        },

        // ---- reductions --------------------------------------------------------------------
        "sum" | "prod" | "mean" | "median" | "mode" | "std" | "var" | "any" | "all" | "nnz" => {
            let m = m_arg(a, 0, name)?;
            if name == "nnz" {
                return num((0..m.numel()).filter(|&i| m.data[i] != 0.0 || m.im_at(i) != 0.0).count() as f64);
            }
            let dim_index = if matches!(name, "std" | "var") { 2 } else { 1 };
            let all_dims = a.get(dim_index).map(|v| v.text().map(|t| t == "all").unwrap_or(false)).unwrap_or(false);
            let m = if all_dims { m.clone().reshaped(vec![m.numel(), 1]) } else { m };
            let dim = if all_dims { 1 } else { dim_arg(a, dim_index, &m)? };
            let omit = a.iter().any(|v| v.text().map(|t| t == "omitnan").unwrap_or(false));
            let filt = |v: &[f64]| -> Vec<f64> { if omit { v.iter().copied().filter(|x| !x.is_nan()).collect() } else { v.to_vec() } };
            let w = if matches!(name, "std" | "var") { a.get(1).and_then(|v| v.scalar().ok()).unwrap_or(0.0) } else { 0.0 };
            if m.is_complex() && matches!(name, "sum" | "mean" | "prod") {
                let out = reduce_c(&m, dim, |v| match name {
                    "sum" => v.iter().fold(C::new(0.0, 0.0), |acc, z| acc.add(*z)),
                    "prod" => v.iter().fold(C::new(1.0, 0.0), |acc, z| acc.mul(*z)),
                    _ => {
                        let s = v.iter().fold(C::new(0.0, 0.0), |acc, z| acc.add(*z));
                        C::new(s.re / v.len() as f64, s.im / v.len() as f64)
                    }
                });
                return mat(out);
            }
            let r = match name {
                "sum" => reduce(&m, dim, |v| filt(v).iter().sum()),
                "prod" => reduce(&m, dim, |v| filt(v).iter().product()),
                "mean" => reduce(&m, dim, |v| {
                    let v = filt(v);
                    if v.is_empty() { f64::NAN } else { v.iter().sum::<f64>() / v.len() as f64 }
                }),
                "median" => reduce(&m, dim, |v| median_of(&filt(v))),
                "mode" => reduce(&m, dim, mode_of),
                "std" => reduce(&m, dim, |v| variance(&filt(v), w).sqrt()),
                "var" => reduce(&m, dim, |v| variance(&filt(v), w)),
                "any" => reduce(&m, dim, |v| v.iter().any(|&x| x != 0.0 && !x.is_nan()) as u8 as f64).with_kind(Kind::Logical),
                _ => reduce(&m, dim, |v| v.iter().all(|&x| x != 0.0) as u8 as f64).with_kind(Kind::Logical),
            };
            // sum/prod of integers stay in the integer class; single stays single.
            let r = match (name, m.kind) {
                ("sum" | "prod", k @ (Kind::Int(_) | Kind::Single)) => r.cast_to(k),
                ("mean" | "median" | "mode" | "std" | "var", Kind::Single) => r.cast_to(Kind::Single),
                ("mode", k @ Kind::Int(_)) => r.cast_to(k),
                _ => r,
            };
            mat(r)
        }
        "cumsum" | "cumprod" | "cummax" | "cummin" => {
            let m = m_arg(a, 0, name)?;
            let dim = dim_arg(a, 1, &m)?;
            mat(match name {
                "cumsum" => cumulate(&m, dim, |p, x| p + x),
                "cumprod" => cumulate(&m, dim, |p, x| p * x),
                "cummax" => cumulate(&m, dim, f64::max),
                _ => cumulate(&m, dim, f64::min),
            })
        }
        "max" | "min" => {
            let is_max = name == "max";
            let better = |x: f64, best: f64| if is_max { x > best } else { x < best };
            if a.len() >= 2 && !matches!(&a[1], Value::Num(m) if m.is_empty()) {
                let x = m_arg(a, 0, name)?;
                let y = m_arg(a, 1, name)?;
                let kind = ops::arith_kind(x.kind, y.kind).unwrap_or(Kind::Double);
                return mat(broadcast(&x, &y, |p, q| if p.is_nan() { q } else if q.is_nan() { p } else if better(q, p) { q } else { p })?.cast_to(kind));
            }
            let m = m_arg(a, 0, name)?;
            if m.is_empty() {
                return Ok(Some(vec![Value::empty(), Value::empty()]));
            }
            let all_dims = a.get(2).map(|v| v.text().map(|t| t == "all").unwrap_or(false)).unwrap_or(false);
            let m = if all_dims { m.clone().reshaped(vec![m.numel(), 1]) } else { m };
            let dim = if all_dims { 1 } else { dim_arg(a, 2, &m)? };
            // Complex values compare by magnitude.
            let key: Vec<f64> = (0..m.numel()).map(|i| if m.is_complex() { C::new(m.data[i], m.im_at(i)).abs() } else { m.data[i] }).collect();
            let (dims, sl) = slices(&m.dims, dim);
            let mut pos = Vec::with_capacity(sl.len());
            let mut idx = Vec::with_capacity(sl.len());
            for s in &sl {
                let mut bi = 0;
                let mut bv = f64::NAN;
                for (k, &p) in s.iter().enumerate() {
                    let x = key[p];
                    if x.is_nan() {
                        continue;
                    }
                    if bv.is_nan() || better(x, bv) {
                        bv = x;
                        bi = k;
                    }
                }
                pos.push(s[bi]);
                idx.push((bi + 1) as f64);
            }
            let vals = m.gather(&pos, dims.clone());
            Ok(Some(vec![Value::Num(vals), Value::Num(Matrix::from_dims(dims, idx))]))
        }
        "sort" => {
            let v = arg(a, 0, name)?;
            let desc = a.iter().skip(1).any(|v| v.text().map(|s| s.eq_ignore_ascii_case("descend")).unwrap_or(false));
            if let Value::Cell(_) | Value::Str(_) = v {
                return crate::builtins_text::sort_text(v, desc);
            }
            let m = m_arg(a, 0, name)?;
            let dim = match a.get(1) {
                Some(d) if !d.is_text() => d.as_usize()?,
                _ => default_dim(&m.dims),
            };
            // Values only, of a real vector: an unstable parallel sort gives the same result
            // unless both zeros are present (only a stable sort keeps their order).
            let along = m.dims.get(dim - 1).copied().unwrap_or(1);
            if nargout <= 1 && !m.is_complex() && along == m.numel() && !m.data.iter().any(|x| *x == 0.0 && x.is_sign_negative()) {
                use rayon::slice::ParallelSliceMut;
                let mut sorted = m;
                let cmp = |p: &f64, q: &f64| match (p.is_nan(), q.is_nan()) {
                    (true, true) => std::cmp::Ordering::Equal,
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    _ => p.partial_cmp(q).unwrap(),
                };
                if desc {
                    sorted.data.par_sort_unstable_by(|p, q| cmp(q, p));
                } else {
                    sorted.data.par_sort_unstable_by(cmp);
                }
                return Ok(Some(vec![Value::Num(sorted)]));
            }
            let key: Vec<f64> = (0..m.numel()).map(|i| if m.is_complex() { C::new(m.data[i], m.im_at(i)).abs() } else { m.data[i] }).collect();
            let (_, sl) = slices(&m.dims, dim);
            let mut order = vec![0usize; m.numel()];
            let mut idx = Matrix::filled_dims(m.dims.clone(), 0.0);
            for s in &sl {
                let mut v: Vec<(f64, usize)> = s.iter().enumerate().map(|(k, &p)| (key[p], k)).collect();
                v.sort_by(|p, q| {
                    let o = match (p.0.is_nan(), q.0.is_nan()) {
                        (true, true) => std::cmp::Ordering::Equal,
                        (true, false) => std::cmp::Ordering::Greater,
                        (false, true) => std::cmp::Ordering::Less,
                        _ => p.0.partial_cmp(&q.0).unwrap(),
                    };
                    // Descending order puts NaN first, as MATLAB does.
                    if desc { o.reverse() } else { o }
                });
                for (j, (_, k)) in v.into_iter().enumerate() {
                    order[s[j]] = s[k];
                    idx.data[s[j]] = (k + 1) as f64;
                }
            }
            let mut sorted = m.gather(&order, m.dims.clone());
            sorted.kind = m.kind;
            Ok(Some(vec![Value::Num(sorted), Value::Num(idx)]))
        }
        "sortrows" => {
            let m = m_arg(a, 0, name)?;
            let cols: Vec<(usize, bool)> = match a.get(1) {
                Some(v) if !v.is_text() => v.to_matrix()?.data.iter().map(|&c| ((c.abs() as usize).saturating_sub(1), c < 0.0)).collect(),
                _ => (0..m.cols()).map(|c| (c, false)).collect(),
            };
            let desc_all = a.iter().any(|v| v.text().map(|t| t == "descend").unwrap_or(false));
            let mut order: Vec<usize> = (0..m.rows()).collect();
            order.sort_by(|&x, &y| {
                for &(c, d) in &cols {
                    let o = m.get(x, c).partial_cmp(&m.get(y, c)).unwrap_or(std::cmp::Ordering::Equal);
                    let o = if d || desc_all { o.reverse() } else { o };
                    if o != std::cmp::Ordering::Equal {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            });
            let rows = IndexArg::List { ix: order.clone(), dims: vec![order.len(), 1], from_mask: false };
            let sorted = indexing::index(&Value::Num(m), &[rows, IndexArg::All])?;
            Ok(Some(vec![sorted, Value::Num(Matrix::col(order.iter().map(|&i| (i + 1) as f64).collect()))]))
        }
        "unique" => {
            let v = arg(a, 0, name)?;
            if let Value::Cell(_) | Value::Str(_) = v {
                return crate::builtins_text::unique_text(v);
            }
            let m = m_arg(a, 0, name)?;
            let stable = a.iter().any(|v| v.text().map(|t| t == "stable").unwrap_or(false));
            let mut seen: Vec<f64> = Vec::new();
            let mut first_idx = Vec::new();
            for (i, &x) in m.data.iter().enumerate() {
                if !seen.iter().any(|&y| y == x || (x.is_nan() && false)) {
                    seen.push(x);
                    first_idx.push((i + 1) as f64);
                }
            }
            if !stable {
                let mut pairs: Vec<(f64, f64)> = seen.into_iter().zip(first_idx).collect();
                pairs.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(std::cmp::Ordering::Equal));
                seen = pairs.iter().map(|p| p.0).collect();
                first_idx = pairs.iter().map(|p| p.1).collect();
            }
            let ic: Vec<f64> = m.data.iter().map(|x| (seen.iter().position(|y| y == x).unwrap_or(0) + 1) as f64).collect();
            let out = if m.dims.len() == 2 && m.dims[0] == 1 { Matrix::row(seen) } else { Matrix::col(seen) };
            Ok(Some(vec![Value::Num(out.with_kind(m.kind)), Value::Num(Matrix::col(first_idx)), Value::Num(Matrix::col(ic))]))
        }
        "ismember" => {
            let x = arg(a, 0, name)?;
            let set = arg(a, 1, name)?;
            if x.is_text() || matches!(x, Value::Cell(_) | Value::Str(_)) {
                return crate::builtins_text::ismember_text(x, set);
            }
            let xm = x.to_matrix()?;
            let sm = set.to_matrix()?;
            let loc: Vec<f64> = xm.data.iter().map(|v| sm.data.iter().position(|s| s == v).map(|p| (p + 1) as f64).unwrap_or(0.0)).collect();
            Ok(Some(vec![
                Value::Num(Matrix::from_dims(xm.dims.clone(), loc.iter().map(|&l| (l > 0.0) as u8 as f64).collect()).with_kind(Kind::Logical)),
                Value::Num(Matrix::from_dims(xm.dims.clone(), loc)),
            ]))
        }
        "find" => {
            let m = m_arg(a, 0, name)?;
            let limit = match a.get(1) {
                Some(v) => v.as_usize()?,
                None => usize::MAX,
            };
            let last = a.get(2).map(|v| v.text().map(|t| t == "last").unwrap_or(false)).unwrap_or(false);
            let mut ix: Vec<usize> = (0..m.numel()).filter(|&i| m.data[i] != 0.0 || m.im_at(i) != 0.0).collect();
            if last {
                let skip = ix.len().saturating_sub(limit);
                ix = ix[skip..].to_vec();
            } else {
                ix.truncate(limit);
            }
            if nargout >= 2 {
                let rows = m.rows().max(1);
                let r: Vec<f64> = ix.iter().map(|&i| (i % rows + 1) as f64).collect();
                let c: Vec<f64> = ix.iter().map(|&i| (i / rows + 1) as f64).collect();
                let vals = m.gather(&ix, vec![ix.len(), 1]);
                return Ok(Some(vec![Value::Num(Matrix::col(r)), Value::Num(Matrix::col(c)), Value::Num(vals)]));
            }
            let data: Vec<f64> = ix.iter().map(|&i| (i + 1) as f64).collect();
            mat(if m.dims.len() == 2 && m.dims[0] == 1 { Matrix::row(data) } else { Matrix::col(data) })
        }
        "diff" => {
            let m = m_arg(a, 0, name)?;
            let order = match a.get(1) {
                Some(v) => v.as_usize()?,
                None => 1,
            };
            let mut cur = m;
            for _ in 0..order {
                let dim = default_dim(&cur.dims);
                let (_, sl) = slices(&cur.dims, dim);
                let mut d = cur.dims.clone();
                d[dim - 1] = d[dim - 1].saturating_sub(1);
                let mut out = vec![0.0; nd::numel(&d)];
                let (_, out_sl) = slices(&d, dim);
                for (s, o) in sl.iter().zip(out_sl.iter()) {
                    for k in 1..s.len() {
                        out[o[k - 1]] = cur.data[s[k]] - cur.data[s[k - 1]];
                    }
                }
                cur = Matrix::from_dims(d, out);
            }
            mat(cur)
        }
        "trapz" | "cumtrapz" => {
            let (x, y) = if a.len() >= 2 { (Some(m_arg(a, 0, name)?), m_arg(a, 1, name)?) } else { (None, m_arg(a, 0, name)?) };
            let mut acc = 0.0;
            let mut out = vec![0.0];
            for i in 1..y.numel() {
                let dx = x.as_ref().map(|x| x.data[i] - x.data[i - 1]).unwrap_or(1.0);
                acc += dx * (y.data[i] + y.data[i - 1]) / 2.0;
                out.push(acc);
            }
            if name == "trapz" { num(acc) } else { mat(Matrix::from_dims(y.dims.clone(), out)) }
        }
        "dot" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            if x.numel() != y.numel() {
                return rt("A and B must be the same size.");
            }
            let s = (0..x.numel()).fold(C::new(0.0, 0.0), |acc, i| acc.add(C::new(x.data[i], -x.im_at(i)).mul(C::new(y.data[i], y.im_at(i)))));
            mat(Matrix::complex(vec![1, 1], vec![s.re], vec![s.im]))
        }
        "cross" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            if x.numel() != 3 || y.numel() != 3 {
                return rt("A and B must be of length 3 for cross.");
            }
            let (p, q) = (&x.data, &y.data);
            let d = vec![p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]];
            mat(if x.dims[0] == 1 { Matrix::row(d) } else { Matrix::col(d) })
        }
        "norm" => {
            let m = m_arg(a, 0, name)?;
            let mag: Vec<f64> = (0..m.numel()).map(|i| C::new(m.data[i], m.im_at(i)).abs()).collect();
            let p = a.get(1);
            if m.is_vector() || m.is_empty() {
                let pv = match p {
                    Some(v) if v.is_text() => if v.text()?.eq_ignore_ascii_case("inf") { f64::INFINITY } else { 2.0 },
                    Some(v) => v.scalar()?,
                    None => 2.0,
                };
                let n = if pv.is_infinite() {
                    mag.iter().fold(0.0f64, |acc, x| acc.max(*x))
                } else if pv == 1.0 {
                    mag.iter().sum()
                } else {
                    mag.iter().map(|x| x.powf(pv)).sum::<f64>().powf(1.0 / pv)
                };
                return num(n);
            }
            if matches!(p, Some(v) if v.text().map(|s| s == "fro").unwrap_or(false)) {
                return num(mag.iter().map(|x| x * x).sum::<f64>().sqrt());
            }
            let real = Matrix::from_dims(m.dims.clone(), m.data.clone());
            let ata = ops::matmul(&real.transpose(), &real)?;
            num(sym_eigs(&ata).into_iter().fold(0.0f64, f64::max).max(0.0).sqrt())
        }
        "det" => num(ops::det(&m_arg(a, 0, name)?)?),
        "inv" => mat(ops::inv_any(&m_arg(a, 0, name)?)?),
        "trace" => {
            let m = m_arg(a, 0, name)?;
            num((0..m.rows().min(m.cols())).map(|i| m.get(i, i)).sum())
        }
        "transpose" => one(ops::unary(crate::ast::UnOp::Transpose, arg(a, 0, name)?)?),
        "ctranspose" => one(ops::unary(crate::ast::UnOp::CTranspose, arg(a, 0, name)?)?),
        "eig" => {
            let m = m_arg(a, 0, name)?;
            if m.rows() != m.cols() {
                return rt("Input matrix must be square.");
            }
            let n = m.rows();
            let symmetric = !m.is_complex() && (0..n).all(|i| (0..n).all(|j| (m.get(i, j) - m.get(j, i)).abs() <= 1e-12 * (m.get(i, j).abs() + 1.0)));
            if symmetric {
                let (vals, vecs) = jacobi_eig(&m);
                if nargout >= 2 {
                    let mut d = Matrix::zeros(n, n);
                    for i in 0..n {
                        d.set(i, i, vals[i]);
                    }
                    return Ok(Some(vec![Value::Num(vecs), Value::Num(d)]));
                }
                return mat(Matrix::col(vals));
            }
            if nargout >= 2 {
                return rt("Eigenvectors of nonsymmetric matrices are not supported in this version.");
            }
            // Characteristic polynomial (Faddeev–LeVerrier), then its roots.
            let coeffs = char_poly(&Matrix::from_dims(m.dims.clone(), m.data.clone()))?;
            let mut roots = poly_roots(&coeffs.iter().map(|&c| C::new(c, 0.0)).collect::<Vec<_>>());
            roots.sort_by(|p, q| p.re.partial_cmp(&q.re).unwrap_or(std::cmp::Ordering::Equal).then(p.im.partial_cmp(&q.im).unwrap_or(std::cmp::Ordering::Equal)));
            mat(Matrix::complex(vec![n, 1], roots.iter().map(|z| z.re).collect(), roots.iter().map(|z| z.im).collect()))
        }
        "roots" => {
            let p = m_arg(a, 0, name)?;
            let c: Vec<C> = (0..p.numel()).map(|i| C::new(p.data[i], p.im_at(i))).collect();
            let r = poly_roots(&c);
            let n = r.len();
            mat(Matrix::complex(vec![n, if n == 0 { 0 } else { 1 }], r.iter().map(|z| z.re).collect(), r.iter().map(|z| z.im).collect()))
        }
        "fliplr" | "flipud" | "flip" => {
            let v = arg(a, 0, name)?;
            let d = v.dims();
            let dim = match (name, a.get(1)) {
                ("fliplr", _) => 2,
                ("flipud", _) => 1,
                (_, Some(k)) => k.as_usize()?,
                _ => default_dim(&d),
            };
            let (_, sl) = slices(&d, dim);
            let mut order = vec![0usize; nd::numel(&d)];
            for s in &sl {
                for (k, &p) in s.iter().enumerate() {
                    order[p] = s[s.len() - 1 - k];
                }
            }
            let ix = IndexArg::List { ix: order, dims: vec![1, nd::numel(&d)], from_mask: false };
            let flat = indexing::index(v, &[ix])?;
            one(match flat {
                Value::Num(m) => Value::Num(m.reshaped(d)),
                Value::Cell(c) => Value::Cell(CellArray::from_dims(d, c.data)),
                Value::Str(s) => Value::Str(StrArray::from_dims(d, s.data)),
                other => other,
            })
        }
        "circshift" => {
            let m = m_arg(a, 0, name)?;
            let k = arg(a, 1, name)?.scalar()? as i64;
            let dim = match a.get(2) {
                Some(d) => d.as_usize()?,
                None => default_dim(&m.dims),
            };
            let (_, sl) = slices(&m.dims, dim);
            let mut out = m.clone();
            for s in sl {
                let n = s.len() as i64;
                for (j, &p) in s.iter().enumerate() {
                    let src = (((j as i64 - k) % n + n) % n) as usize;
                    out.data[p] = m.data[s[src]];
                }
            }
            mat(out)
        }
        "round" => {
            let m = m_arg(a, 0, name)?;
            let digits = match a.get(1) {
                Some(d) => d.scalar()?,
                None => 0.0,
            };
            let f = 10f64.powf(digits);
            mat(m.map(|x| (x * f).round() / f))
        }
        "nthroot" => {
            let x = m_arg(a, 0, name)?;
            let n = m_arg(a, 1, name)?;
            mat(broadcast(&x, &n, |v, k| if v < 0.0 && k % 2.0 == 1.0 { -(-v).powf(1.0 / k) } else { v.powf(1.0 / k) })?)
        }
        "mod" | "rem" | "atan2" | "hypot" | "power" | "gcd" | "lcm" | "nchoosek" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            if name == "power" {
                return mat(ops::num_binary(crate::ast::BinOp::Pow, &x, &y)?);
            }
            let kind = ops::arith_kind(x.kind, y.kind).unwrap_or(Kind::Double);
            let f: fn(f64, f64) -> f64 = match name {
                "mod" => |p, q| if q == 0.0 { p } else { let r = p - (p / q).floor() * q; if r != 0.0 && (r < 0.0) != (q < 0.0) { r + q } else { r } },
                "rem" => |p, q| if q == 0.0 { f64::NAN } else { p - (p / q).trunc() * q },
                "atan2" => f64::atan2,
                "hypot" => f64::hypot,
                "gcd" => gcd,
                "lcm" => |p, q| if p == 0.0 || q == 0.0 { 0.0 } else { (p * q).abs() / gcd(p, q) },
                _ => |n, k| {
                    let mut r = 1.0;
                    for i in 0..(k as i64) {
                        r = r * (n - i as f64) / (i as f64 + 1.0);
                    }
                    r.round()
                },
            };
            mat(broadcast(&x, &y, f)?.cast_to(kind))
        }
        "factor" => {
            let n = arg(a, 0, name)?.scalar()?;
            if n < 0.0 || n.fract() != 0.0 {
                return rt("Input must be a non-negative integer.");
            }
            let fs = crate::sym::algebra::prime_factors(n as i128);
            mat(Matrix::row(fs.into_iter().map(|f| f as f64).collect()))
        }
        "primes" => {
            let n = arg(a, 0, name)?.scalar()?;
            mat(Matrix::row((2..=(n.max(0.0) as u64)).map(|x| x as f64).filter(|&x| is_prime(x)).collect()))
        }
        "polyfit" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            let deg = arg(a, 2, name)?.as_usize()?;
            let n = x.numel();
            let mut v = Matrix::zeros(n, deg + 1);
            for i in 0..n {
                for j in 0..=deg {
                    v.set(i, j, x.data[i].powi((deg - j) as i32));
                }
            }
            let p = ops::solve(&v, &Matrix::col(y.data.clone()))?;
            mat(Matrix::row(p.data))
        }
        "polyval" => {
            let p = m_arg(a, 0, name)?;
            let x = m_arg(a, 1, name)?;
            mat(x.map(|xv| p.data.iter().fold(0.0, |acc, &c| acc * xv + c)))
        }
        "interp1" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            let xi = m_arg(a, 2, name)?;
            let method = a.get(3).map(|v| v.text()).transpose()?.unwrap_or_else(|| "linear".into());
            let n = x.numel();
            mat(xi.map(|q| {
                if n < 2 || q < x.data[0] || q > x.data[n - 1] || q.is_nan() {
                    return f64::NAN;
                }
                let k = x.data.partition_point(|&v| v <= q).clamp(1, n - 1);
                let (x0, x1, y0, y1) = (x.data[k - 1], x.data[k], y.data[k - 1], y.data[k]);
                match method.as_str() {
                    "nearest" => if q - x0 < x1 - q { y0 } else { y1 },
                    "previous" => if q == x1 { y1 } else { y0 },
                    "next" => if q == x0 { y0 } else { y1 },
                    _ => if x1 == x0 { y0 } else { y0 + (y1 - y0) * (q - x0) / (x1 - x0) },
                }
            }))
        }
        "histcounts" => {
            let m = m_arg(a, 0, name)?;
            let edges = match a.get(1) {
                Some(e) if e.numel() > 1 => e.to_matrix()?,
                other => {
                    let nbins = other.map(|v| v.as_usize()).transpose()?.unwrap_or(10).max(1);
                    let lo = m.data.iter().cloned().fold(f64::INFINITY, f64::min);
                    let hi = m.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    Matrix::row((0..=nbins).map(|k| lo + (hi - lo) * k as f64 / nbins as f64).collect())
                }
            };
            let ne = edges.numel();
            let mut counts = vec![0.0; ne.saturating_sub(1)];
            for &v in &m.data {
                for k in 0..ne.saturating_sub(1) {
                    let last = k == ne - 2;
                    if v >= edges.data[k] && (v < edges.data[k + 1] || (last && v == edges.data[k + 1])) {
                        counts[k] += 1.0;
                        break;
                    }
                }
            }
            Ok(Some(vec![Value::Num(Matrix::row(counts)), Value::Num(edges)]))
        }
        "accumarray" => {
            let subs = m_arg(a, 0, name)?;
            let vals = m_arg(a, 1, name)?;
            let n = subs.data.iter().cloned().fold(0.0, f64::max) as usize;
            let mut out = vec![0.0; n];
            for (i, &s) in subs.data.iter().enumerate() {
                let v = if vals.numel() == 1 { vals.data[0] } else { vals.data[i] };
                out[s as usize - 1] += v;
            }
            mat(Matrix::col(out))
        }

        // ---- functional ------------------------------------------------------------------
        "feval" => {
            let f = arg(a, 0, name)?.clone();
            Ok(Some(it.call_func_value(&f, args[1..].to_vec(), nargout)?))
        }
        "deal" => {
            if a.len() == 1 {
                return Ok(Some(vec![a[0].clone(); nargout.max(1)]));
            }
            if nargout > a.len() {
                return rt("The number of outputs should match the number of inputs.");
            }
            Ok(Some(args.clone()))
        }
        "func2str" => match arg(a, 0, name)? {
            // A named handle gives its bare name (func2str(@sin) is 'sin'), an anonymous one its text.
            Value::Func(crate::value::FuncValue::Named(n)) => one(Value::chars(n)),
            Value::Func(f) => one(Value::chars(&crate::display::func_text(f))),
            _ => rt("Input must be a function handle."),
        },
        "str2func" => {
            let s = arg(a, 0, name)?.text()?;
            if s.starts_with('@') {
                let e = crate::parser::parse_expr(&s)?;
                one(it.eval(&e)?)
            } else {
                one(Value::Func(FuncValue::Named(s)))
            }
        }
        "arrayfun" | "cellfun" => {
            let f = arg(a, 0, name)?.clone();
            let mut uniform = true;
            let mut error_handler = None;
            let mut inputs: Vec<Value> = Vec::new();
            let mut i = 1;
            while i < a.len() {
                if a[i].is_text() && !matches!(a[i], Value::Cell(_)) {
                    let t = a[i].text()?;
                    if t.eq_ignore_ascii_case("UniformOutput") {
                        uniform = a.get(i + 1).map(|v| v.is_true()).transpose()?.unwrap_or(true);
                        i += 2;
                        continue;
                    }
                    if t.eq_ignore_ascii_case("ErrorHandler") {
                        error_handler = a.get(i + 1).cloned();
                        i += 2;
                        continue;
                    }
                }
                inputs.push(a[i].clone());
                i += 1;
            }
            let _ = error_handler;
            let Some(first) = inputs.first() else { return rt("Not enough input arguments.") };
            let dims = first.dims();
            let n = nd::numel(&dims);
            let nout = nargout.max(1);
            let mut results: Vec<Vec<Value>> = vec![Vec::with_capacity(n); nout];
            for k in 0..n {
                let call_args = inputs.iter().map(|v| element(v, k, name == "cellfun")).collect::<R<Vec<_>>>()?;
                let out = it.call_func_value(&f, call_args, nout)?;
                if out.len() < nout && (uniform || nargout > 0) {
                    if out.is_empty() && nargout == 0 && !uniform {
                        results[0].push(Value::empty());
                        continue;
                    }
                    if out.is_empty() && nargout == 0 {
                        continue;
                    }
                    return rt("The function must return a value when UniformOutput is true.");
                }
                for (j, v) in out.into_iter().take(nout).enumerate() {
                    results[j].push(v);
                }
            }
            let mut outs = Vec::new();
            for res in results {
                if res.is_empty() && nargout == 0 && n > 0 {
                    return none();
                }
                if uniform {
                    let mut data = Vec::with_capacity(n);
                    let mut kind = None;
                    for (k, v) in res.iter().enumerate() {
                        match v {
                            Value::Num(m) if m.numel() == 1 && !m.is_complex() => {
                                kind = Some(match kind {
                                    None => m.kind,
                                    Some(prev) if prev == m.kind => prev,
                                    _ => Kind::Double,
                                });
                                data.push(m.data[0]);
                            }
                            _ => return rt(format!("Non-scalar in Uniform output, at index {}, output 1. Set 'UniformOutput' to false.", k + 1)),
                        }
                    }
                    outs.push(Value::Num(Matrix::from_dims(dims.clone(), data).with_kind(kind.unwrap_or(Kind::Double))));
                } else {
                    outs.push(Value::Cell(CellArray::from_dims(dims.clone(), res)));
                }
            }
            Ok(Some(outs))
        }

        // ---- session & errors --------------------------------------------------------------
        "tic" => {
            it.tic = std::time::Instant::now();
            none()
        }
        "toc" => {
            let t = it.tic.elapsed().as_secs_f64();
            if nargout >= 1 {
                return num(t);
            }
            it.emit(OutputKind::Text(format!("Elapsed time is {t:.6} seconds.\n")));
            none()
        }
        "pause" | "drawnow" | "clc" | "validateattributes" | "narginchk" | "nargchk" => none(),
        "addpath" => {
            for v in a {
                if let Ok(p) = v.text() {
                    for part in p.split(';').filter(|s| !s.is_empty()) {
                        it.add_path(std::path::PathBuf::from(part));
                    }
                }
            }
            none()
        }
        "rmpath" | "path" => none(),
        "pwd" => one(Value::chars(&std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default())),
        "clear" => {
            if a.is_empty() || a.iter().any(|v| v.text().map(|s| s == "all" || s == "variables").unwrap_or(false)) {
                it.clear_current_scope();
            } else {
                for v in a {
                    let pat = v.text()?;
                    if let Some(prefix) = pat.strip_suffix('*') {
                        for n in it.var_names() {
                            if n.starts_with(prefix) {
                                it.remove_var(&n);
                            }
                        }
                    } else {
                        it.remove_var(&pat);
                    }
                }
            }
            none()
        }
        "format" => {
            let mode = a.first().map(|v| v.text()).transpose()?.unwrap_or_default();
            it.format = if mode.eq_ignore_ascii_case("long") { NumFormat::Long } else { NumFormat::Short };
            none()
        }
        "disp" => {
            let v = arg(a, 0, name)?;
            if let Value::Object(o) = v {
                if let Some(f) = o.class.method("disp").cloned() {
                    let class = o.class.clone();
                    it.call_method(&f, &class, vec![v.clone()], 0)?;
                    return none();
                }
                let values = it.object_values(o)?;
                let text = crate::class::format_object("", o, &values, it.format);
                it.emit(OutputKind::Text(text.trim_start_matches(" = \n").to_string() + "\n"));
                return none();
            }
            let mut s = disp_value(v, it.format);
            if !(v.numel() == 0 && matches!(v, Value::Num(_))) {
                s.push('\n');
            }
            it.emit(OutputKind::Text(s));
            none()
        }
        "display" => {
            let v = arg(a, 0, name)?;
            let text = crate::display::display_value("ans", v, it.format);
            it.emit(OutputKind::Value { name: "ans".into(), text });
            none()
        }
        "error" => {
            let mut parts: &[Value] = a;
            if parts.len() >= 2 {
                if let Ok(id) = parts[0].text() {
                    if id.contains(':') && !id.contains(' ') && parts[1].is_text() {
                        parts = &parts[1..];
                    }
                }
            }
            let msg = match parts.first() {
                Some(Value::Struct(s)) => s.get("message").map(|m| m.text()).transpose()?.unwrap_or_default(),
                Some(f) => {
                    if parts.len() > 1 { sprintf(&f.text()?, &parts[1..])? } else { f.text()? }
                }
                None => return none(),
            };
            rt(msg)
        }
        "warning" => {
            let mut parts: &[Value] = a;
            if parts.len() >= 2 {
                if let Ok(id) = parts[0].text() {
                    if id.contains(':') && !id.contains(' ') {
                        parts = &parts[1..];
                    }
                }
            }
            if let Some(f) = parts.first() {
                let t = f.text()?;
                if t == "off" || t == "on" || t == "query" {
                    return none();
                }
                let msg = if parts.len() > 1 { sprintf(&t, &parts[1..])? } else { t };
                it.emit(OutputKind::Warning(msg));
            }
            none()
        }
        "assert" => {
            if !arg(a, 0, name)?.is_true()? {
                let msg = match a.get(1) {
                    Some(f) => sprintf(&f.text()?, &a[2..])?,
                    None => "Assertion failed.".into(),
                };
                return rt(msg);
            }
            none()
        }
        "inputname" => one(Value::chars("")),

        // ---- operator function forms -------------------------------------------------------
        "plus" | "minus" | "times" | "mtimes" | "rdivide" | "ldivide" | "mrdivide" | "mldivide" | "mpower" | "eq" | "ne" | "lt" | "le" | "gt"
        | "ge" | "and" | "or" => {
            use crate::ast::BinOp::*;
            let op = match name {
                "plus" => Add,
                "minus" => Sub,
                "times" => Mul,
                "mtimes" => MatMul,
                "rdivide" => Div,
                "ldivide" => LeftDiv,
                "mrdivide" => MatDiv,
                "mldivide" => MatLeftDiv,
                "mpower" => MatPow,
                "eq" => Eq,
                "ne" => Ne,
                "lt" => Lt,
                "le" => Le,
                "gt" => Gt,
                "ge" => Ge,
                "and" => And,
                _ => Or,
            };
            one(it.binary(op, arg(a, 0, name)?.clone(), arg(a, 1, name)?.clone())?)
        }
        "xor" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            mat(broadcast(&x, &y, |p, q| ((p != 0.0) != (q != 0.0)) as u8 as f64)?.with_kind(Kind::Logical))
        }
        "not" => one(ops::unary(crate::ast::UnOp::Not, arg(a, 0, name)?)?),
        "uminus" => one(ops::unary(crate::ast::UnOp::Neg, arg(a, 0, name)?)?),
        "uplus" => one(arg(a, 0, name)?.clone()),
        _ => Ok(None),
    }
}

/// min/max/sort/mean/diff/sum on datetime and duration arrays.
fn time_reduction(name: &str, t: &TimeArray, args: &[Value]) -> Ret {
    use crate::time::TimeKind;
    if t.kind == TimeKind::CalDuration {
        return Ok(None);
    }
    let m = crate::time::to_numeric(t);
    let wrap = |mm: Matrix| -> Value { Value::Time(t.with_ms(mm.dims.clone(), mm.data)) };
    match name {
        "min" | "max" | "sort" | "mean" | "median" if args.len() == 1 || name == "sort" => {
            let mut rest = vec![Value::Num(m)];
            rest.extend(args[1..].iter().cloned());
            let mut it = Interpreter::new();
            let r = call(&mut it, name, rest, 2)?.unwrap_or_default();
            let mut out: Vec<Value> = Vec::new();
            for (k, v) in r.into_iter().enumerate() {
                out.push(if k == 0 { wrap(v.to_matrix()?) } else { v });
            }
            Ok(Some(out))
        }
        "diff" => {
            let d: Vec<f64> = m.data.windows(2).map(|w| w[1] - w[0]).collect();
            let n = d.len();
            let dims = if m.dims[0] == 1 { vec![1, n] } else { vec![n, 1] };
            Ok(Some(vec![Value::Time(crate::time::TimeArray::durations(dims, d, "hh:mm:ss"))]))
        }
        "sum" if t.kind == TimeKind::Duration => Ok(Some(vec![wrap(Matrix::scalar(m.data.iter().sum()))])),
        "abs" if t.kind == TimeKind::Duration => Ok(Some(vec![wrap(m.map(f64::abs))])),
        _ => Ok(None),
    }
}

/// Eigen-decomposition of a real symmetric matrix by cyclic Jacobi rotations.
fn jacobi_eig(m: &Matrix) -> (Vec<f64>, Matrix) {
    let n = m.rows();
    let mut a = Matrix::from_dims(m.dims.clone(), m.data.clone());
    let mut v = ops::identity(n);
    for _ in 0..100 {
        let off: f64 = (0..n).flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j))).map(|(i, j)| a.get(i, j).powi(2)).sum();
        if off < 1e-22 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = a.get(p, q);
                if apq.abs() < 1e-300 {
                    continue;
                }
                let theta = (a.get(q, q) - a.get(p, p)) / (2.0 * apq);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (akp, akq) = (a.get(k, p), a.get(k, q));
                    a.set(k, p, c * akp - s * akq);
                    a.set(k, q, s * akp + c * akq);
                }
                for k in 0..n {
                    let (apk, aqk) = (a.get(p, k), a.get(q, k));
                    a.set(p, k, c * apk - s * aqk);
                    a.set(q, k, s * apk + c * aqk);
                }
                for k in 0..n {
                    let (vkp, vkq) = (v.get(k, p), v.get(k, q));
                    v.set(k, p, c * vkp - s * vkq);
                    v.set(k, q, s * vkp + c * vkq);
                }
            }
        }
    }
    // Ascending eigenvalues with matching vectors (MATLAB's order for symmetric input).
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| a.get(i, i).partial_cmp(&a.get(j, j)).unwrap_or(std::cmp::Ordering::Equal));
    let vals = order.iter().map(|&i| a.get(i, i)).collect();
    let mut vecs = Matrix::zeros(n, n);
    for (c, &i) in order.iter().enumerate() {
        // Normalize sign so the largest component is positive.
        let col: Vec<f64> = (0..n).map(|r| v.get(r, i)).collect();
        let big = col.iter().cloned().fold(0.0f64, |acc, x| if x.abs() > acc.abs() { x } else { acc });
        let s = if big < 0.0 { -1.0 } else { 1.0 };
        for r in 0..n {
            vecs.set(r, c, s * col[r]);
        }
    }
    (vals, vecs)
}

fn sym_eigs(m: &Matrix) -> Vec<f64> {
    jacobi_eig(m).0
}

/// Characteristic polynomial coefficients (highest degree first).
fn char_poly(a: &Matrix) -> R<Vec<f64>> {
    let n = a.rows();
    let mut coeffs = vec![1.0];
    let mut mk = Matrix::zeros(n, n);
    let id = ops::identity(n);
    let mut c = 1.0;
    for k in 1..=n {
        // M_k = A M_{k-1} + c_{k-1} I ; c_k = -tr(A M_k)/k
        let am = ops::matmul(a, &mk)?;
        mk = broadcast(&am, &id, |x, y| x + c * y)?;
        let amk = ops::matmul(a, &mk)?;
        c = -(0..n).map(|i| amk.get(i, i)).sum::<f64>() / k as f64;
        coeffs.push(c);
    }
    Ok(coeffs)
}

/// Lanczos approximation of the gamma function.
fn gamma(x: f64) -> f64 {
    if x.fract() == 0.0 && x <= 0.0 {
        return f64::INFINITY;
    }
    if x < 0.5 {
        return std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma(1.0 - x));
    }
    const G: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    let x = x - 1.0;
    let mut a = G[0];
    let t = x + 7.5;
    for (i, &g) in G.iter().enumerate().skip(1) {
        a += g / (x + i as f64);
    }
    (2.0 * std::f64::consts::PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * a
}
