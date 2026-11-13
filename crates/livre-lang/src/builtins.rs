//! Built-in functions. `call` returns `Ok(None)` when the name is not a builtin. Type-specific
//! families live in `builtins_text`, `builtins_plot`, `time`, `table` and `sym`.

use crate::cplx::C;
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
    "unique", "var", "vertcat", "warning", "zeros", "histcounts", "pwd", "drawnow", "clc", "isspace", "fprintf",
    "sprintf", "newline", "blanks", "accumarray", "cumtrapz", "validateattributes", "inputname", "nargchk", "narginchk", "isdiag", "istril",
    "istriu", "issorted", "triu", "tril", "floor", "ctranspose", "uminus", "uplus", "plus", "minus", "times", "mtimes", "rdivide", "ldivide",
    "mrdivide", "mldivide", "mpower", "eq", "ne", "lt", "le", "gt", "ge", "and", "or", "not", "xor", "colon", "isobject", "isstring",
];

pub fn exists(name: &str) -> bool {
    all_names().contains(name)
}

/// The functions a user can call by name (for the editor's completion list), sorted.
pub fn function_names() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = all_names()
        .iter()
        .copied()
        .filter(|n| n.bytes().next().is_some_and(|c| c.is_ascii_alphabetic()) && n.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') && !n.starts_with("matlivre_"))
        .collect();
    v.sort_unstable();
    v
}

fn all_names() -> &'static std::collections::HashSet<&'static str> {
    static ALL: std::sync::OnceLock<std::collections::HashSet<&'static str>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        [NAMES, crate::builtins_elfun::NAMES, crate::builtins_datafun::NAMES, crate::builtins_lang::NAMES, crate::builtins_strings::NAMES, crate::builtins_types::NAMES, crate::builtins_dates::NAMES, crate::builtins_files::NAMES, crate::builtins_textio::NAMES, crate::builtins_data::NAMES, crate::builtins_media::NAMES, crate::builtins_archive::NAMES, crate::builtins_structio::NAMES, crate::gfx::NAMES, crate::builtins_plotfmt::NAMES, crate::builtins_graph3d::NAMES, crate::builtins_funfun::NAMES, crate::builtins_matfun::NAMES, crate::builtins_elmat::NAMES, crate::builtins_ops::NAMES, crate::builtins_specfun::NAMES, crate::polyshape::NAMES, crate::builtins_polyfun::NAMES, crate::builtins_geom::NAMES, crate::polyfun_classes::NAMES, crate::builtins_text::NAMES, crate::builtins_plot::NAMES, crate::time::NAMES, crate::table::NAMES, crate::sym::NAMES, crate::sym::extra::NAMES, crate::sym::special::NAMES, crate::sym::tools::NAMES, crate::sym::units::NAMES, crate::sym::transforms::NAMES, crate::sym::forms::NAMES, crate::sym::ode::NAMES, crate::sym::formula::NAMES, crate::sym::dae::NAMES, crate::containers::NAMES, crate::table_ops::NAMES, crate::builtins_plotmore::NAMES, crate::patterns::NAMES, crate::builtins_validate::NAMES, crate::graphs::NAMES, crate::graphs::FUNCS, crate::builtins_excel::NAMES, crate::builtins_sparse::NAMES, crate::builtins_ode::NAMES, crate::timetable::NAMES, crate::io_more::NAMES, crate::lang_more::NAMES, crate::plot_extra::NAMES, crate::volume::NAMES, crate::timeseries::NAMES, crate::charts::NAMES, crate::images::NAMES, crate::math_more::NAMES, crate::func_objects::NAMES, crate::files_more::NAMES, crate::random::NAMES, crate::timers::NAMES, crate::session_more::NAMES, crate::settings::NAMES, crate::sysobj::NAMES, crate::tiff::NAMES, crate::decomp::NAMES, crate::messages::NAMES, crate::ode_objects::NAMES, crate::gfx_util::NAMES, crate::figfile::NAMES, crate::xml_dom::NAMES, crate::gfx_defaults::NAMES, crate::signal::windows::NAMES, crate::signal::waveforms::NAMES, crate::signal::db::NAMES, crate::signal::analog::NAMES, crate::signal::iir::NAMES, crate::signal::fir::NAMES, crate::signal::response::NAMES, crate::signal::filtering::NAMES, crate::signal::conversions::NAMES, crate::signal::measures::NAMES, crate::signal::spectral::NAMES, crate::signal::transforms::NAMES, crate::signal::peaks::NAMES, crate::signal::smoothing::NAMES, crate::signal::lpc::NAMES, crate::signal::sos::NAMES, crate::signal::rational::NAMES, crate::signal::elliptic::NAMES, crate::signal::order::NAMES, crate::signal::firdes::NAMES, crate::signal::remez::NAMES, crate::signal::fanalysis::NAMES, crate::signal::measure2::NAMES, crate::signal::lpc2::NAMES, crate::signal::ar::NAMES, crate::signal::transforms2::NAMES, crate::signal::cepstra::NAMES, crate::signal::pspec::NAMES, crate::signal::change::NAMES, crate::signal::resamp::NAMES, crate::signal::bandwidth::NAMES, crate::signal::modeling::NAMES, crate::signal::pulses::NAMES, crate::signal::zcrate::NAMES, crate::signal::ssconv::NAMES]
            .iter()
            .flat_map(|n| n.iter().copied())
            .collect()
    })
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
        Value::Sparse(c) => c.to_dense(),
        Value::Sym(s) => s.to_matrix(),
        other => rt(format!("Undefined function '{fname}' for input arguments of type '{}'.", other.class_name())),
    }
}

/// Like [`m_arg`], but borrows a numeric argument instead of copying it (large arrays).
pub(crate) fn m_cow<'a>(args: &'a [Value], i: usize, fname: &str) -> R<std::borrow::Cow<'a, Matrix>> {
    match arg(args, i, fname)? {
        Value::Num(m) => Ok(std::borrow::Cow::Borrowed(m)),
        Value::Sparse(c) => Ok(std::borrow::Cow::Owned(c.to_dense()?)),
        Value::Sym(s) => Ok(std::borrow::Cow::Owned(s.to_matrix()?)),
        other => rt(format!("Undefined function '{fname}' for input arguments of type '{}'.", other.class_name())),
    }
}

fn bool_mat(m: &Matrix, f: impl Fn(f64) -> bool) -> Matrix {
    Matrix::from_dims(m.dims.clone(), m.data.iter().map(|&x| if f(x) { 1.0 } else { 0.0 }).collect()).with_kind(Kind::Logical)
}

/// Trailing class name in constructor calls: `zeros(2, 'int8')`, `ones(3, 'like', x)`.
pub(crate) fn class_option(args: &[Value]) -> R<(Vec<Value>, Kind)> {
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

/// The values of a slice, without the NaNs when `omit` is set (no copy otherwise).
fn filt_of(v: &[f64], omit: bool) -> std::borrow::Cow<'_, [f64]> {
    if omit { v.iter().copied().filter(|x| !x.is_nan()).collect::<Vec<f64>>().into() } else { v.into() }
}

/// Geometry of the slices along `dim` (1-based): the result dims, the distance between
/// consecutive elements of a slice, the slice length and the number of outer blocks. Slice
/// `idx` (in result order) starts at `(idx / stride) * stride * len + idx % stride`.
pub(crate) fn slice_geom(d: &[usize], dim: usize) -> (Dims, usize, usize, usize) {
    let k = dim - 1;
    if k >= d.len() {
        return (d.to_vec(), nd::numel(d), 1, 1);
    }
    let mut out_dims = d.to_vec();
    out_dims[k] = 1;
    (nd::normalize(out_dims), d[..k].iter().product(), d[k], d[k + 1..].iter().product())
}

/// Applies `f` to each slice along `dim`. Contiguous slices are passed without copying, and
/// large reductions run on several threads.
fn reduce(m: &Matrix, dim: usize, f: impl Fn(&[f64]) -> f64 + Sync) -> Matrix {
    use rayon::prelude::*;
    if m.dims == [0, 0] {
        return Matrix::scalar(f(&[]));
    }
    let (dims, stride, len, outer) = slice_geom(&m.dims, dim);
    let total = stride * outer;
    let one = |idx: usize, buf: &mut Vec<f64>| -> f64 {
        let base = (idx / stride) * stride * len + idx % stride;
        if stride == 1 {
            f(&m.data[base..base + len])
        } else {
            buf.clear();
            buf.extend((0..len).map(|j| m.data[base + j * stride]));
            f(buf)
        }
    };
    let data: Vec<f64> = if total >= 2 && total * len >= ops::PAR_MIN {
        (0..total).into_par_iter().map_init(Vec::new, |buf, idx| one(idx, buf)).collect()
    } else {
        let mut buf = Vec::new();
        (0..total).map(|idx| one(idx, &mut buf)).collect()
    };
    Matrix::from_dims(dims, data)
}

/// `sum` or `prod` along a dimension other than the first, accumulating whole rows at a time
/// (contiguous reads and writes) instead of gathering each strided slice. The order of the
/// additions in each slice is unchanged. Returns `None` when the generic path should be used.
fn fold_strided(m: &Matrix, dim: usize, is_sum: bool) -> Option<Matrix> {
    use rayon::prelude::*;
    let (dims, stride, len, outer) = slice_geom(&m.dims, dim);
    if stride <= 1 || len == 0 || m.dims == [0, 0] {
        return None;
    }
    let mut out = vec![0.0; stride * outer];
    let fill = |o: usize, i0: usize, oc: &mut [f64]| {
        let base = o * stride * len + i0;
        oc.copy_from_slice(&m.data[base..base + oc.len()]);
        for j in 1..len {
            let row = &m.data[base + j * stride..base + j * stride + oc.len()];
            if is_sum {
                oc.iter_mut().zip(row).for_each(|(a, b)| *a += b);
            } else {
                oc.iter_mut().zip(row).for_each(|(a, b)| *a *= b);
            }
        }
    };
    const CH: usize = 4096;
    if stride * outer * len >= ops::PAR_MIN {
        out.par_chunks_mut(stride).enumerate().for_each(|(o, out_o)| {
            out_o.par_chunks_mut(CH).enumerate().for_each(|(c, oc)| fill(o, c * CH, oc));
        });
    } else {
        for (o, out_o) in out.chunks_mut(stride).enumerate() {
            fill(o, 0, out_o);
        }
    }
    Some(Matrix::from_dims(dims, out))
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
    let (_, stride, len, outer) = slice_geom(&m.dims, dim);
    if len < 2 || stride == 0 {
        return out;
    }
    let kind = out.kind;
    let plain = kind == Kind::Double;
    for o in 0..outer {
        let base = o * stride * len;
        for j in 1..len {
            let at = base + j * stride;
            if plain {
                for i in at..at + stride {
                    out.data[i] = f(out.data[i - stride], m.data[i]);
                }
            } else {
                for i in at..at + stride {
                    out.data[i] = kind.cast(f(out.data[i - stride], m.data[i]));
                }
            }
        }
    }
    out
}

fn median_of(v: &[f64]) -> f64 {
    if v.is_empty() || v.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    let n = s.len();
    if n >= 64 {
        // Only the middle element(s) are needed: selection instead of a full sort.
        let (lower, mid, _) = s.select_nth_unstable_by(n / 2, |a, b| a.partial_cmp(b).unwrap());
        let mid = *mid;
        if n % 2 == 1 {
            return mid;
        }
        let below = lower.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        return (below + mid) / 2.0;
    }
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
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

pub(crate) fn is_prime(n: f64) -> bool {
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

/// sind and cosd together: the angle reduced exactly to [-45, 45] degrees and a quadrant,
/// converted with x*(pi/180), and exact at multiples of 30 and 45 degrees. They agree with
/// MATLAB exactly there and within one ulp elsewhere.
pub(crate) fn sind_cosd(x: f64) -> (f64, f64) {
    if !x.is_finite() {
        return (f64::NAN, f64::NAN);
    }
    let r = x % 360.0;
    let k = (r / 90.0).round();
    let r = r - k * 90.0;
    let (s, c) = if r == 0.0 {
        (0.0, 1.0)
    } else if r.abs() == 30.0 {
        (0.5f64.copysign(r), 0.75f64.sqrt())
    } else if r.abs() == 45.0 {
        (0.5f64.sqrt().copysign(r), 0.5f64.sqrt())
    } else {
        let a = r * (std::f64::consts::PI / 180.0);
        (libm::sin(a), libm::cos(a))
    };
    match (k as i64).rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
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
        "sind" => Some(|x: f64| if (x % 180.0) == 0.0 { 0.0f64.copysign(x) } else { sind_cosd(x).0 }),
        "cosd" => Some(|x: f64| if ((x - 90.0) % 180.0) == 0.0 { 0.0 } else { sind_cosd(x).1 }),
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
        "factorial" => Some(|x: f64| if x.fract() == 0.0 && (0.0..=170.0).contains(&x) { (1..=x as u32).fold(1.0, |p, k| p * k as f64) } else { gamma(x + 1.0).round() }),
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
        // sign(z) = z/abs(z) for complex z.
        if m.is_complex() && name == "sign" {
            let im = m.im.as_ref().unwrap();
            let mut out = m.clone();
            let mut oi = vec![0.0; m.data.len()];
            for (k, (&re, &iv)) in m.data.iter().zip(im).enumerate() {
                let r = re.hypot(iv);
                let (a, b) = if r == 0.0 { (0.0, 0.0) } else if r.is_nan() { (f64::NAN, f64::NAN) } else { (re / r, iv / r) };
                out.data[k] = a;
                oi[k] = b;
            }
            out.im = Some(oi);
            return Some(Ok(keep(out)));
        }
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
/// `accumarray(subs, val, sz, fun, fillval)`: values gathered by subscript and reduced by `fun`.
fn accumarray(it: &mut Interpreter, a: &[Value]) -> R<Value> {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    // Subscripts: an m-by-n matrix, or a cell array of m-element index vectors.
    let (m, cols): (usize, Vec<Vec<f64>>) = match &a[0] {
        Value::Cell(c) => {
            let cols: Vec<Vec<f64>> = c.data.iter().map(|v| v.to_matrix().map(|m| m.data)).collect::<R<_>>()?;
            (cols.first().map(|c| c.len()).unwrap_or(0), cols)
        }
        v => {
            let s = v.to_matrix()?;
            let (r, c) = (s.rows(), s.cols());
            let (r, c) = if r == 1 && c > 1 { (c, 1) } else { (r, c) };
            (r, (0..c).map(|j| s.data[j * r..(j + 1) * r].to_vec()).collect())
        }
    };
    if cols.iter().any(|c| c.len() != m) || cols.iter().flatten().any(|&x| x < 1.0 || x.fract() != 0.0) {
        return rt("First input SUBS must contain positive integer subscripts.");
    }
    let vals = a[1].to_matrix()?;
    if vals.numel() != 1 && vals.numel() != m {
        return rt("Second input VAL must be a vector with one element for each row in SUBS, or a scalar.");
    }
    let n = cols.len().max(1);
    let maxes: Vec<usize> = cols.iter().map(|c| c.iter().fold(0.0, |x: f64, &y| x.max(y)) as usize).collect();
    let mut dims: Vec<usize> = if n == 1 { vec![maxes.first().copied().unwrap_or(0), 1] } else { maxes.clone() };
    if let Some(sz) = a.get(2).filter(|v| v.numel() > 0) {
        let sz: Vec<usize> = sz.to_matrix()?.data.iter().map(|&x| x as usize).collect();
        let ok = if n == 1 { sz.len() == 2 && (sz[1] == 1 || sz[0] == 1) } else { sz.len() >= n };
        if !ok || sz.iter().zip(if n == 1 { vec![maxes[0]] } else { maxes.clone() }).any(|(&s, mx)| s < mx) && n > 1 {
            return rt("Third input SZ must be a full row vector with one element for each column of SUBS.");
        }
        if n == 1 && sz.iter().product::<usize>() < maxes[0] {
            return rt("Third input SZ must be a full row vector with one element for each column of SUBS.");
        }
        dims = sz;
    }
    let total: usize = dims.iter().product();
    // Linear index of each row of subscripts.
    let linear = |i: usize| -> usize {
        if n == 1 {
            return cols[0][i] as usize - 1;
        }
        let mut k = 0;
        let mut stride = 1;
        for (j, c) in cols.iter().enumerate() {
            k += (c[i] as usize - 1) * stride;
            stride *= dims[j];
        }
        k
    };
    let val = |i: usize| if vals.numel() == 1 { vals.data[0] } else { vals.data[i] };
    let fun = a.get(3).filter(|v| v.numel() > 0 || matches!(v, Value::Func(_))).cloned();
    let fill = match a.get(4).filter(|v| v.numel() > 0) {
        Some(v) => Some(v.clone()),
        None => None,
    };
    let Some(fun) = fun else {
        let mut out = vec![0.0; total];
        let mut hit = vec![false; total];
        for i in 0..m {
            let k = linear(i);
            out[k] += val(i);
            hit[k] = true;
        }
        if let Some(f) = &fill {
            let fv = f.scalar()?;
            for (o, h) in out.iter_mut().zip(&hit) {
                if !h {
                    *o = fv;
                }
            }
        }
        return Ok(Value::Num(Matrix::from_dims(dims, out)));
    };
    // Groups in subscript order, each with its values in the order they appear.
    let mut groups: Vec<Vec<f64>> = vec![Vec::new(); total];
    for i in 0..m {
        groups[linear(i)].push(val(i));
    }
    let mut results: Vec<Option<Value>> = vec![None; total];
    for (k, g) in groups.iter().enumerate() {
        if !g.is_empty() {
            let out = it.call_func_value(&fun, vec![Value::Num(Matrix::col(g.clone()))], 1)?;
            results[k] = out.into_iter().next();
        }
    }
    if results.iter().flatten().any(|v| matches!(v, Value::Cell(_))) {
        let data: Vec<Value> = results
            .into_iter()
            .map(|r| match r {
                Some(Value::Cell(c)) if c.data.len() == 1 => c.data[0].clone(),
                Some(v) => v,
                None => fill.clone().unwrap_or_else(Value::empty),
            })
            .collect();
        return Ok(Value::Cell(CellArray::from_dims(dims, data)));
    }
    let fv = match &fill {
        Some(f) => f.scalar()?,
        None => 0.0,
    };
    let mut out = Vec::with_capacity(total);
    for r in results {
        out.push(match r {
            Some(v) => {
                let m = v.to_matrix()?;
                if m.numel() != 1 {
                    return rt("The function FUN must return a scalar.");
                }
                m.data[0]
            }
            None => fv,
        });
    }
    Ok(Value::Num(Matrix::from_dims(dims, out)))
}

/// A column cell of names (0-by-1 when there are none, as fieldnames gives).
fn name_col(items: Vec<Value>) -> Value {
    let n = items.len();
    Value::Cell(CellArray::from_dims(vec![n, 1], items))
}

/// The instant `t = tic` time stamps count from.
fn tic_epoch() -> std::time::Instant {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *START.get_or_init(std::time::Instant::now)
}

fn element(v: &Value, k: usize, unwrap_cells: bool) -> R<Value> {
    match v {
        Value::Cell(c) if unwrap_cells => Ok(c.data[k].clone()),
        Value::Func(_) | Value::Object(_) => Ok(v.clone()),
        other => indexing::index(other, &[IndexArg::scalar(k)]),
    }
}

pub fn call(it: &mut Interpreter, name: &str, args: Vec<Value>, nargout: usize) -> Ret {
    call_ref(it, name, &args, nargout)
}

/// As [`call`], reading the arguments in place (the caller keeps them).
pub fn call_ref(it: &mut Interpreter, name: &str, args: &[Value], nargout: usize) -> Ret {
    // faer's factorizations (QR, SVD, eigenvalues, Cholesky) default to the global thread pool,
    // where they ran up to 100 times slower than single-threaded on matrices of a few hundred
    // rows; products and solves pass their own parallelism explicitly.
    static FAER_INIT: std::sync::Once = std::sync::Once::new();
    FAER_INIT.call_once(|| faer::set_global_parallelism(faer::Par::Seq));
    // Sparse matrices held as compressed columns: native builders and operations first, then the
    // expanded array for everything else (the result is compressed again if it is sparse).
    if matches!(name, "sparse" | "speye" | "spdiags" | "kron") {
        if let Some(r) = crate::sparse_ops::build(name, args)? {
            return Ok(Some(r));
        }
    }
    if args.iter().any(Value::is_compressed) {
        if let Some(r) = crate::sparse_ops::call(it, name, args, nargout)? {
            return Ok(Some(r));
        }
        if crate::builtins_sparse::native_solver(name) {
            return call_dense(it, name, args, nargout);
        }
        let expanded = args.iter().map(|a| a.clone().densify()).collect::<R<Vec<Value>>>()?;
        let r = call_ref(it, name, &expanded, nargout)?;
        return Ok(r.map(|v| v.into_iter().map(crate::builtins_sparse::compress).collect()));
    }
    if args.first().is_some_and(crate::builtins_sparse::is_sparse) {
        let mut r = call_dense(it, name, args, nargout)?;
        if let Some(out) = r.as_mut() {
            crate::builtins_sparse::propagate(name, &args[..1], out);
        }
        return Ok(r);
    }
    if crate::gfx::defaults_active() {
        // Objects this call makes take the root defaults (set(groot, 'defaultAxesColor', ...)).
        let before = crate::gfx::snapshot_ids(it);
        let r = call_dense(it, name, args, nargout);
        crate::gfx::apply_new_defaults(it, &before, args);
        return r;
    }
    call_dense(it, name, args, nargout)
}

fn call_dense(it: &mut Interpreter, name: &str, args: &[Value], nargout: usize) -> Ret {
    // patch with faces and vertices.
    if name == "patch" {
        if let Some(r) = crate::volume::patch_fv(it, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    // Text files read with an import options object.
    if crate::io_more::READERS.contains(&name) {
        if let Some(r) = crate::io_more::read_call(it, name, &args)? {
            return Ok(Some(r));
        }
    }
    // Spreadsheet files for the table and matrix readers and writers.
    if crate::builtins_excel::FILE_FUNCS.contains(&name) {
        let at = if name.starts_with("read") { 0 } else { 1 };
        if args.get(at).and_then(|v| v.text().ok()).is_some_and(|p| crate::builtins_excel::is_sheet_path(&p)) {
            return crate::builtins_excel::file_call(it, name, &args, nargout);
        }
    }
    if crate::builtins_excel::NAMES.contains(&name) {
        if let Some(r) = crate::builtins_excel::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    // Type families first.
    if crate::patterns::TEXT_FUNCS.contains(&name) && args.iter().skip(1).any(crate::patterns::is_pattern) {
        return crate::patterns::text_call(name, &args);
    }
    if crate::graphs::NAMES.contains(&name) {
        if let Some(r) = crate::graphs::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::builtins_validate::NAMES.contains(&name) {
        if let Some(r) = crate::builtins_validate::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::patterns::NAMES.contains(&name) {
        if let Some(r) = crate::patterns::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if args.iter().any(|a| matches!(a, Value::Sym(_))) || crate::sym::NAMES.contains(&name) || crate::sym::extra::NAMES.contains(&name) || crate::sym::special::NAMES.contains(&name) || crate::sym::tools::NAMES.contains(&name) || crate::sym::units::NAMES.contains(&name) || crate::sym::transforms::NAMES.contains(&name) || crate::sym::forms::NAMES.contains(&name) || crate::sym::ode::NAMES.contains(&name) || crate::sym::formula::NAMES.contains(&name) || crate::sym::dae::NAMES.contains(&name) {
        if let Some(r) = crate::sym::call(it, name, args.to_vec(), nargout)? {
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
    if crate::builtins_plotmore::NAMES.contains(&name) {
        if let Some(r) = crate::builtins_plotmore::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::charts::NAMES.contains(&name) {
        if let Some(r) = crate::charts::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::images::NAMES.contains(&name) {
        if let Some(r) = crate::images::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::math_more::NAMES.contains(&name) {
        if let Some(r) = crate::math_more::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::func_objects::NAMES.contains(&name) {
        if let Some(r) = crate::func_objects::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::files_more::NAMES.contains(&name) {
        if let Some(r) = crate::files_more::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::random::NAMES.contains(&name) {
        if let Some(r) = crate::random::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::timers::NAMES.contains(&name) {
        if let Some(r) = crate::timers::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::figfile::NAMES.contains(&name) {
        if let Some(r) = crate::figfile::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::xml_dom::NAMES.contains(&name) {
        if let Some(r) = crate::xml_dom::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if let Some(r) = crate::signal::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if crate::gfx_defaults::NAMES.contains(&name) {
        if let Some(r) = crate::gfx_defaults::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::gfx_util::NAMES.contains(&name) {
        if let Some(r) = crate::gfx_util::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::ode_objects::NAMES.contains(&name) {
        if let Some(r) = crate::ode_objects::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::decomp::NAMES.contains(&name) {
        if let Some(r) = crate::decomp::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::messages::NAMES.contains(&name) {
        if let Some(r) = crate::messages::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::tiff::NAMES.contains(&name) {
        if let Some(r) = crate::tiff::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::sysobj::NAMES.contains(&name) {
        if let Some(r) = crate::sysobj::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::settings::NAMES.contains(&name) {
        if let Some(r) = crate::settings::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::session_more::NAMES.contains(&name) {
        if let Some(r) = crate::session_more::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::timeseries::NAMES.contains(&name) {
        if let Some(r) = crate::timeseries::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::volume::NAMES.contains(&name) {
        if let Some(r) = crate::volume::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::plot_extra::NAMES.contains(&name) {
        if let Some(r) = crate::plot_extra::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::lang_more::NAMES.contains(&name) {
        if let Some(r) = crate::lang_more::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if matches!(name, "char" | "string" | "cellstr") {
        if let Some(v) = crate::lang_more::enum_text(name, &args) {
            return Ok(Some(vec![v]));
        }
    }
    if name == "delete" && args.len() == 1 && crate::lang_more::delete_listener(it, &args[0]) {
        return Ok(Some(Vec::new()));
    }
    if crate::io_more::NAMES.contains(&name) {
        if let Some(r) = crate::io_more::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::timetable::NAMES.contains(&name) {
        if let Some(r) = crate::timetable::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::builtins_ode::NAMES.contains(&name) {
        if let Some(r) = crate::builtins_ode::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::builtins_sparse::NAMES.contains(&name) {
        if let Some(r) = crate::builtins_sparse::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::table_ops::NAMES.contains(&name) {
        if let Some(r) = crate::table_ops::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if crate::containers::NAMES.contains(&name) {
        if let Some(r) = crate::containers::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if name == "delete" {
        if let Some(r) = crate::gfx::delete(it, &args) {
            return r;
        }
    }
    if crate::gfx::NAMES.contains(&name) {
        if let Some(r) = crate::gfx::call(it, name, &args, nargout)? {
            return Ok(Some(r));
        }
    }
    if let Some(r) = crate::builtins_plotfmt::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_graph3d::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_datafun::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_lang::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_strings::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_types::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_dates::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_files::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_textio::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_data::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_media::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_archive::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_structio::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_funfun::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_elfun::call(name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_elmat::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_ops::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_specfun::call(name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::polyshape::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_polyfun::call(it, name, &args, nargout)? {
        return Ok(Some(r));
    }
    if let Some(r) = crate::builtins_matfun::call(it, name, &args, nargout)? {
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

    // [F,E] = log2(X): mantissa in [0.5, 1) and exponent, X = F .* 2.^E.
    if name == "log2" && nargout >= 2 && a.len() == 1 {
        let m = m_arg(a, 0, name)?;
        let (f, e): (Vec<f64>, Vec<f64>) = m
            .data
            .iter()
            .map(|&x| {
                if x == 0.0 || !x.is_finite() {
                    return (x, 0.0);
                }
                let mut e = x.abs().log2().floor() + 1.0;
                let mut f = x / 2f64.powf(e);
                // Correct the rounding of log2 near powers of two and in the subnormal range.
                while f.abs() >= 1.0 {
                    f /= 2.0;
                    e += 1.0;
                }
                while f.abs() < 0.5 {
                    f *= 2.0;
                    e -= 1.0;
                }
                (f, e)
            })
            .unzip();
        return Ok(Some(vec![Value::Num(Matrix::from_dims(m.dims.clone(), f)), Value::Num(Matrix::from_dims(m.dims.clone(), e))]));
    }

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
        "pi" if !a.is_empty() => rt("Too many input arguments."),
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
            let n = match a.get(2) {
                Some(v) => v.scalar()?.floor().max(0.0) as usize,
                None => 100,
            };
            let (lm, hm) = (m_arg(a, 0, name)?, m_arg(a, 1, name)?);
            if lm.numel() != 1 || hm.numel() != 1 {
                return rt("Inputs must be scalars.");
            }
            let part = |lo: f64, hi: f64| -> Vec<f64> { if n == 1 { vec![hi] } else { (0..n).map(|i| lo + (hi - lo) * i as f64 / (n - 1) as f64).collect() } };
            let mut out = Matrix::row(part(lm.data[0], hm.data[0]));
            if lm.is_complex() || hm.is_complex() {
                let im = |m: &Matrix| m.im.as_ref().map(|v| v[0]).unwrap_or(0.0);
                out.im = Some(part(im(&lm), im(&hm)));
            }
            mat(out)
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
        "meshgrid" | "ndgrid" if a.len() == 3 || (nargout >= 3 && a.len() == 1) => {
            // 3-D grids: meshgrid is [ny nx nz] with x along columns; ndgrid is [nx ny nz].
            let x = m_arg(a, 0, name)?;
            let y = if a.len() > 1 { m_arg(a, 1, name)? } else { x.clone() };
            let z = if a.len() > 2 { m_arg(a, 2, name)? } else { x.clone() };
            let (n0, n1, n2) = if name == "meshgrid" { (y.numel(), x.numel(), z.numel()) } else { (x.numel(), y.numel(), z.numel()) };
            let total = n0 * n1 * n2;
            let (mut gx, mut gy, mut gz) = (vec![0.0; total], vec![0.0; total], vec![0.0; total]);
            for k in 0..n2 {
                for j in 0..n1 {
                    for i in 0..n0 {
                        let p = k * n0 * n1 + j * n0 + i;
                        let (xi, yi) = if name == "meshgrid" { (j, i) } else { (i, j) };
                        gx[p] = x.data[xi];
                        gy[p] = y.data[yi];
                        gz[p] = z.data[k];
                    }
                }
            }
            let dims = vec![n0, n1, n2];
            Ok(Some(vec![Value::Num(Matrix::from_dims(dims.clone(), gx)), Value::Num(Matrix::from_dims(dims.clone(), gy)), Value::Num(Matrix::from_dims(dims, gz))]))
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
                // X varies down the rows (along the first vector), Y across the columns.
                return Ok(Some(vec![Value::Num(yy), Value::Num(xx)]));
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
            // struct([]): a 0-by-0 struct array with no fields; struct(s) is s.
            if a.len() == 1 {
                match &a[0] {
                    Value::Num(m) if m.numel() == 0 && m.kind != Kind::Char => return one(Value::Struct(StructArray::empty_with(Vec::new()))),
                    Value::Struct(_) => return one(a[0].clone()),
                    _ => {}
                }
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
                let mut im = m.im.as_ref().map(|_| vec![0.0; n * n]);
                for i in 0..m.numel() {
                    let (r, c) = if k >= 0 { (i, i + k as usize) } else { (i + (-k) as usize, i) };
                    out.set(r, c, m.data[i]);
                    if let (Some(o), Some(src)) = (im.as_mut(), m.im.as_ref()) {
                        o[c * n + r] = src[i];
                    }
                }
                out.im = im;
                mat(out)
            } else {
                let mut d = Vec::new();
                let mut di = Vec::new();
                for i in 0..m.rows() {
                    let c = i as i64 + k;
                    if c >= 0 && (c as usize) < m.cols() {
                        d.push(m.get(i, c as usize));
                        if let Some(src) = m.im.as_ref() {
                            di.push(src[c as usize * m.rows() + i]);
                        }
                    }
                }
                let mut out = Matrix::col(d).with_kind(m.kind);
                if m.im.is_some() {
                    out.im = Some(di);
                }
                mat(out)
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
        "horzcat" => one(hconcat(args.to_vec())?),
        "vertcat" => one(vconcat(args.to_vec())?),
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
                            return rt("NaN values cannot be converted to logicals.");
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
            // cast(A, 'like', p) takes p's class (and complexity).
            let cls = if a.len() >= 3 && arg(a, 1, name)?.text().is_ok_and(|t| t.eq_ignore_ascii_case("like")) {
                arg(a, 2, name)?.class_name()
            } else {
                arg(a, 1, name)?.text()?
            };
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
            // An empty array of graphics objects (gobjects(0), gco with nothing selected).
            v @ Value::Object(_) if crate::gfx::is_gfx(v) => crate::gfx::ids_of(v).is_some_and(|ids| ids.is_empty()),
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
            use std::cmp::Ordering;
            // Elements as comparable keys; missing values (NaN, <missing>) sort after all others
            // in ascending order and before them in descending order, as sort places them.
            enum Key {
                Num(f64),
                Text(Option<String>),
            }
            let v = arg(a, 0, name)?;
            let (dims, keys): (Vec<usize>, Vec<Key>) = match v {
                Value::Cell(c) => (c.dims.clone(), c.data.iter().map(|x| x.text().map(|t| Key::Text(Some(t)))).collect::<R<_>>().map_err(|_| LangError::new("Input must be a cell array of character vectors.", 0))?),
                Value::Str(sa) => (sa.dims.clone(), sa.data.iter().map(|t| Key::Text(t.clone())).collect()),
                _ => {
                    let m = m_arg(a, 0, name)?;
                    (m.dims.clone(), m.data.iter().map(|&x| Key::Num(x)).collect())
                }
            };
            let mut dim = None;
            let mut dir = "ascend".to_string();
            let mut missing = "auto".to_string();
            let mut k = 1;
            while k < a.len() {
                match &a[k] {
                    Value::Num(m) if !a[k].is_text() => dim = Some(m.data.first().copied().unwrap_or(1.0) as usize),
                    x => {
                        let t = x.text()?.to_lowercase();
                        if t == "missingplacement" {
                            missing = arg(a, k + 1, name)?.text()?.to_lowercase();
                            k += 1;
                        } else if t == "comparisonmethod" {
                            k += 1;
                        } else {
                            dir = t;
                        }
                    }
                }
                k += 1;
            }
            let is_missing = |x: &Key| matches!(x, Key::Num(f) if f.is_nan()) || matches!(x, Key::Text(None));
            let cmp = |x: &Key, y: &Key| match (x, y) {
                (Key::Num(p), Key::Num(q)) => p.partial_cmp(q).unwrap_or(Ordering::Equal),
                (Key::Text(Some(p)), Key::Text(Some(q))) => p.as_bytes().cmp(q.as_bytes()),
                _ => Ordering::Equal,
            };
            // Whether `seq` is ordered in direction `desc` (strictly when `strict`).
            let ordered = |seq: &[&Key], desc: bool, strict: bool| {
                let last = match missing.as_str() {
                    "first" => false,
                    "last" => true,
                    _ => !desc,
                };
                // Missing values must form one run at the right end.
                let flags: Vec<bool> = seq.iter().map(|x| is_missing(x)).collect();
                let ok_missing = if last { flags.windows(2).all(|w| !w[0] || w[1]) } else { flags.windows(2).all(|w| w[0] || !w[1]) };
                let present: Vec<&&Key> = seq.iter().filter(|x| !is_missing(x)).collect();
                ok_missing
                    && present.windows(2).all(|w| {
                        let o = cmp(w[0], w[1]);
                        let o = if desc { o.reverse() } else { o };
                        if strict { o == Ordering::Less } else { o != Ordering::Greater }
                    })
            };
            let check = |seq: &[&Key]| match dir.as_str() {
                "descend" => ordered(seq, true, false),
                "monotonic" => ordered(seq, false, false) || ordered(seq, true, false),
                "strictascend" => ordered(seq, false, true),
                "strictdescend" => ordered(seq, true, true),
                "strictmonotonic" => ordered(seq, false, true) || ordered(seq, true, true),
                _ => ordered(seq, false, false),
            };
            let (r, c) = (dims[0], keys.len() / dims[0].max(1));
            let d = dim.unwrap_or(if r == 1 { 2 } else { 1 });
            let ok = if d == 1 {
                (0..c).all(|j| check(&(0..r).map(|i| &keys[j * r + i]).collect::<Vec<_>>()))
            } else {
                (0..r).all(|i| check(&(0..c).map(|j| &keys[j * r + i]).collect::<Vec<_>>()))
            };
            one(Value::bool(ok))
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
            let kind = a.get(1).map(|v| v.text()).transpose()?.unwrap_or_default();
            let path = std::path::Path::new(&n);
            // Built-in classes as MATLAB reports them: dynamicprops is a file, containers.Map a class.
            if kind.is_empty() && it.get_var(&n).is_none() {
                match n.as_str() {
                    "dynamicprops" | "function_handle" => return num(2.0),
                    "containers.Map" | "matlab.graphics.Graphics" | "matlab.graphics.GraphicsPlaceholder" => return num(8.0),
                    _ => {}
                }
            }
            // 1 variable, 5 built-in, 2 function or file, 7 folder.
            let found = match kind.as_str() {
                "var" => if it.get_var(&n).is_some() { 1.0 } else { 0.0 },
                "builtin" => if exists(&n) { 5.0 } else { 0.0 },
                "dir" => if path.is_dir() { 7.0 } else { 0.0 },
                "file" => if it.has_function(&n) || path.is_file() { 2.0 } else if path.is_dir() { 7.0 } else { 0.0 },
                _ => if it.get_var(&n).is_some() { 1.0 } else if exists(&n) { 5.0 } else if it.has_function(&n) || path.is_file() { 2.0 } else if path.is_dir() { 7.0 } else { 0.0 },
            };
            num(found)
        }
        // properties with no output prints the list, as MATLAB does.
        "properties" if nargout == 0 && matches!(arg(a, 0, name)?, Value::Object(_)) || nargout == 0 && arg(a, 0, name)?.is_text() => {
            let (cls, names): (String, Vec<String>) = match arg(a, 0, name)? {
                Value::Object(o) => (
                    o.class.name.clone(),
                    o.class.props.iter().filter(|p| !p.hidden).map(|p| p.name.clone()).chain(o.dyn_props().unwrap_or_default().fields).collect(),
                ),
                v => {
                    let cls = v.text()?;
                    match it.find_class(&cls) {
                        Some(c) => (cls, c.props.iter().filter(|p| !p.hidden).map(|p| p.name.clone()).collect()),
                        None if crate::lang_more::BUILTIN_CLASSES.contains(&cls.as_str()) => (cls, Vec::new()),
                        None => return rt(format!("No class '{cls}'.")),
                    }
                }
            };
            let text = if names.is_empty() {
                format!("\nNo properties for class {cls}.\n\n")
            } else {
                format!("\nProperties for class {cls}:\n\n{}\n", names.iter().map(|n| format!("    {n}\n")).collect::<String>())
            };
            it.emit(OutputKind::Text(text));
            none()
        }
        "fieldnames" | "properties" => match arg(a, 0, name)? {
            Value::Struct(s) => one(name_col(s.fields.iter().map(|n| Value::chars(n)).collect())),
            Value::Object(o) => one(name_col(
                o.class.props.iter().filter(|p| !p.hidden).map(|p| p.name.clone()).chain(o.dyn_props().unwrap_or_default().fields).map(|n: String| Value::chars(&n)).collect(),
            )),
            Value::Table(t) => one(name_col(t.names.iter().map(|n| Value::chars(n)).collect())),
            v if v.is_text() => {
                let cls = v.text()?;
                match it.find_class(&cls) {
                    Some(c) => one(name_col(c.props.iter().filter(|p| !p.hidden).map(|p| Value::chars(&p.name)).collect())),
                    // Built-in classes have no public properties to list.
                    None if crate::lang_more::BUILTIN_CLASSES.contains(&cls.as_str()) => one(name_col(Vec::new())),
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
        // ismethod('ClassName', name) looks the class up by name.
        "ismethod" if arg(a, 0, name)?.is_text() => {
            let cls = arg(a, 0, name)?.text()?;
            let n = arg(a, 1, name)?.text()?;
            one(Value::bool(it.find_class(&cls).is_some_and(|c| c.method(&n).is_some())))
        }
        "isprop" | "ismethod" => {
            let Value::Object(o) = arg(a, 0, name)? else { return one(Value::bool(false)) };
            let n = arg(a, 1, name)?.text()?;
            one(Value::bool(if name == "isprop" { o.has_prop(&n) } else { o.class.method(&n).is_some() }))
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
            let m = m_cow(a, 0, name)?;
            if name == "nnz" {
                return num((0..m.numel()).filter(|&i| m.data[i] != 0.0 || m.im_at(i) != 0.0).count() as f64);
            }
            let dim_index = if matches!(name, "std" | "var") { 2 } else { 1 };
            let all_dims = a.get(dim_index).map(|v| v.text().map(|t| t == "all").unwrap_or(false)).unwrap_or(false);
            let m = if all_dims { { let n = m.numel(); std::borrow::Cow::Owned(m.into_owned().reshaped(vec![n, 1])) } } else { m };
            let dim = if all_dims { 1 } else { dim_arg(a, dim_index, &m)? };
            let omit = a.iter().any(|v| v.text().map(|t| t == "omitnan").unwrap_or(false));
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
            // [M, F, C] = mode(...): the frequency of the mode and every value that ties with it.
            if name == "mode" && nargout >= 2 {
                let tally = |v: &[f64]| -> (f64, usize, Vec<f64>) {
                    let mut s: Vec<f64> = v.iter().copied().filter(|x| !x.is_nan()).collect();
                    if s.is_empty() {
                        return (f64::NAN, 0, vec![f64::NAN]);
                    }
                    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    let mut runs: Vec<(f64, usize)> = Vec::new();
                    for x in s {
                        match runs.last_mut() {
                            Some((y, n)) if *y == x => *n += 1,
                            _ => runs.push((x, 1)),
                        }
                    }
                    let best = runs.iter().map(|r| r.1).max().unwrap_or(0);
                    let ties: Vec<f64> = runs.iter().filter(|r| r.1 == best).map(|r| r.0).collect();
                    (ties[0], best, ties)
                };
                let (dims, sl) = slices(&m.dims, dim);
                let mut mv = Vec::with_capacity(sl.len());
                let mut fv = Vec::with_capacity(sl.len());
                let mut cv = Vec::with_capacity(sl.len());
                for sl1 in &sl {
                    let vals: Vec<f64> = sl1.iter().map(|&p| m.data[p]).collect();
                    let (mo, f, ties) = tally(&vals);
                    mv.push(mo);
                    fv.push(f as f64);
                    let mut col = Matrix::col(ties);
                    if m.kind.is_int() || m.kind == Kind::Single {
                        col = col.cast_to(m.kind);
                    }
                    cv.push(Value::Num(col));
                }
                let mut mm = Matrix::from_dims(dims.clone(), mv);
                if m.kind.is_int() || m.kind == Kind::Single {
                    mm = mm.cast_to(m.kind);
                }
                return Ok(Some(vec![Value::Num(mm), Value::Num(Matrix::from_dims(dims.clone(), fv)), Value::Cell(CellArray::from_dims(dims, cv))]));
            }
            let strided = if matches!(name, "sum" | "prod") && !omit { fold_strided(&m, dim, name == "sum") } else { None };
            let r = match name {
                _ if strided.is_some() => strided.unwrap(),
                "sum" => reduce(&m, dim, |v| filt_of(v, omit).iter().sum()),
                "prod" => reduce(&m, dim, |v| filt_of(v, omit).iter().product()),
                "mean" => reduce(&m, dim, |v| {
                    let v = filt_of(v, omit);
                    if v.is_empty() { f64::NAN } else { v.iter().sum::<f64>() / v.len() as f64 }
                }),
                "median" => reduce(&m, dim, |v| median_of(&filt_of(v, omit))),
                "mode" => reduce(&m, dim, mode_of),
                "std" => reduce(&m, dim, |v| variance(&filt_of(v, omit), w).sqrt()),
                "var" => reduce(&m, dim, |v| variance(&filt_of(v, omit), w)),
                "any" => reduce(&m, dim, |v| v.iter().any(|&x| x != 0.0 && !x.is_nan()) as u8 as f64).with_kind(Kind::Logical),
                _ => reduce(&m, dim, |v| v.iter().all(|&x| x != 0.0) as u8 as f64).with_kind(Kind::Logical),
            };
            // sum/prod of integers stay in the integer class; single stays single.
            let r = match (name, m.kind) {
                ("sum" | "prod", k @ (Kind::Int(_) | Kind::Single)) => r.cast_to(k),
                ("mean" | "median" | "mode" | "std" | "var", Kind::Single) => r.cast_to(Kind::Single),
                ("mode" | "median", k @ Kind::Int(_)) => r.cast_to(k),
                _ => r,
            };
            mat(r)
        }
        "cumsum" | "cumprod" | "cummax" | "cummin" => {
            let m = m_cow(a, 0, name)?;
            // Options after the data: a dimension, 'reverse'/'forward' and 'omitnan'/'includenan'.
            let mut dim_given: Option<Value> = None;
            let mut reverse = false;
            let mut omit = matches!(name, "cummax" | "cummin");
            for v in &a[1..] {
                if v.is_text() || matches!(v, Value::Str(_)) {
                    match v.text()?.to_lowercase().as_str() {
                        "reverse" => reverse = true,
                        "forward" => reverse = false,
                        "omitnan" | "omitmissing" => omit = true,
                        "includenan" | "includemissing" => omit = false,
                        o => return rt(format!("Invalid option '{o}'.")),
                    }
                } else {
                    dim_given = Some(v.clone());
                }
            }
            let dim = match dim_given {
                Some(v) => dim_arg(&[Value::empty(), v], 1, &m)?,
                None => dim_arg(&a[..1], 1, &m)?,
            };
            let replaced;
            let src: &Matrix = if omit && matches!(name, "cumsum" | "cumprod") {
                let neutral = if name == "cumsum" { 0.0 } else { 1.0 };
                replaced = m.map(|x| if x.is_nan() { neutral } else { x });
                &replaced
            } else {
                &m
            };
            let run = |m: &Matrix| match name {
                "cumsum" => cumulate(m, dim, |p, x| p + x),
                "cumprod" => cumulate(m, dim, |p, x| p * x),
                "cummax" if omit => cumulate(m, dim, f64::max),
                "cummin" if omit => cumulate(m, dim, f64::min),
                "cummax" => cumulate(m, dim, |p, x| if p.is_nan() || x.is_nan() { f64::NAN } else { p.max(x) }),
                _ => cumulate(m, dim, |p, x| if p.is_nan() || x.is_nan() { f64::NAN } else { p.min(x) }),
            };
            if !reverse {
                return mat(run(src));
            }
            // 'reverse': accumulate from the end of each slice.
            let flip = |m: &Matrix| {
                let mut out = m.clone();
                let (_, sl) = slices(&m.dims, dim);
                for s in sl {
                    for (j, &p) in s.iter().enumerate() {
                        out.data[p] = m.data[s[s.len() - 1 - j]];
                    }
                }
                out
            };
            mat(flip(&run(&flip(src))))
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
            let m = m_cow(a, 0, name)?;
            if m.is_empty() {
                return Ok(Some(vec![Value::empty(), Value::empty()]));
            }
            let all_dims = a.get(2).map(|v| v.text().map(|t| t == "all").unwrap_or(false)).unwrap_or(false);
            let m = if all_dims { { let n = m.numel(); std::borrow::Cow::Owned(m.into_owned().reshaped(vec![n, 1])) } } else { m };
            let dim = if all_dims { 1 } else { dim_arg(a, 2, &m)? };
            // Complex values compare by magnitude.
            use rayon::prelude::*;
            let cplx = m.is_complex();
            let keyat = |p: usize| -> f64 { if cplx { C::new(m.data[p], m.im_at(p)).abs() } else { m.data[p] } };
            let (dims, stride, len, outer) = slice_geom(&m.dims, dim);
            let total = stride * outer;
            let scan = |idx: usize| -> (usize, usize) {
                let base = (idx / stride) * stride * len + idx % stride;
                let mut bi = 0;
                let mut bv = f64::NAN;
                for k in 0..len {
                    let x = keyat(base + k * stride);
                    if x.is_nan() {
                        continue;
                    }
                    if bv.is_nan() || better(x, bv) {
                        bv = x;
                        bi = k;
                    }
                }
                (base + bi * stride, bi)
            };
            let found: Vec<(usize, usize)> = if total == 1 && !cplx && len >= 4 * ops::PAR_MIN {
                // One long vector: best (value, position) of each chunk, then the best of those,
                // the earliest winning a tie as in the sequential scan.
                const CH: usize = 1 << 16;
                let parts: Vec<(f64, usize)> = m
                    .data
                    .par_chunks(CH)
                    .enumerate()
                    .map(|(c, ch)| {
                        let (mut bv, mut bi) = (f64::NAN, 0);
                        for (k, &x) in ch.iter().enumerate() {
                            if !x.is_nan() && (bv.is_nan() || better(x, bv)) {
                                bv = x;
                                bi = c * CH + k;
                            }
                        }
                        (bv, bi)
                    })
                    .collect();
                let (mut bv, mut bi) = (f64::NAN, 0);
                for (v, i) in parts {
                    if !v.is_nan() && (bv.is_nan() || better(v, bv)) {
                        bv = v;
                        bi = i;
                    }
                }
                vec![(bi, bi)]
            } else if total >= 2 && total * len >= ops::PAR_MIN {
                (0..total).into_par_iter().map(scan).collect()
            } else {
                (0..total).map(scan).collect()
            };
            let pos: Vec<usize> = found.iter().map(|f| f.0).collect();
            let idx: Vec<f64> = found.iter().map(|f| (f.1 + 1) as f64).collect();
            let mut vals = m.gather(&pos, dims.clone());
            // Characters compare as their codes and come back as double.
            if vals.kind == Kind::Char {
                vals = vals.with_kind(Kind::Double);
            }
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
            let opt = |n: &str| -> R<Option<String>> {
                match a.iter().skip(1).position(|v| v.text().is_ok_and(|t| t.eq_ignore_ascii_case(n))) {
                    Some(k) => Ok(Some(arg(a, k + 2, name)?.text()?.to_lowercase())),
                    None => Ok(None),
                }
            };
            let missing = opt("MissingPlacement")?.unwrap_or_else(|| "auto".into());
            let method = opt("ComparisonMethod")?.unwrap_or_else(|| "auto".into());
            // NaN goes last in ascending order and first in descending order unless placed.
            let nan_last = match missing.as_str() {
                "first" => false,
                "last" => true,
                _ => !desc,
            };
            let by_abs = method == "abs" || method == "auto" && m.is_complex();
            // Values only, of a real vector: an unstable parallel sort gives the same result
            // unless both zeros are present (only a stable sort keeps their order).
            let along = m.dims.get(dim - 1).copied().unwrap_or(1);
            if nargout <= 1 && !m.is_complex() && missing == "auto" && method != "abs" && along == m.numel() && !m.data.iter().any(|x| *x == 0.0 && x.is_sign_negative()) {
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
            // Sort keys: (abs, angle) by magnitude, (real, imag) otherwise.
            let key: Vec<(f64, f64)> = (0..m.numel())
                .map(|i| {
                    let (x, y) = (m.data[i], m.im_at(i));
                    if x.is_nan() || y.is_nan() {
                        (f64::NAN, 0.0)
                    } else if by_abs {
                        (x.hypot(y), y.atan2(x))
                    } else {
                        (x, y)
                    }
                })
                .collect();
            let (_, sl) = slices(&m.dims, dim);
            let mut order = vec![0usize; m.numel()];
            let mut idx = Matrix::filled_dims(m.dims.clone(), 0.0);
            for s in &sl {
                let mut v: Vec<((f64, f64), usize)> = s.iter().enumerate().map(|(k, &p)| (key[p], k)).collect();
                let by = |p: &((f64, f64), usize), q: &((f64, f64), usize)| {
                    use std::cmp::Ordering::*;
                    match (p.0 .0.is_nan(), q.0 .0.is_nan()) {
                        (true, true) => Equal,
                        (true, false) => if nan_last { Greater } else { Less },
                        (false, true) => if nan_last { Less } else { Greater },
                        _ => {
                            let o = p.0 .0.partial_cmp(&q.0 .0).unwrap().then(p.0 .1.partial_cmp(&q.0 .1).unwrap_or(Equal));
                            if desc { o.reverse() } else { o }
                        }
                    }
                };
                if v.len() >= ops::PAR_MIN {
                    use rayon::slice::ParallelSliceMut;
                    v.par_sort_by(by);
                } else {
                    v.sort_by(by);
                }
                for (j, (_, k)) in v.into_iter().enumerate() {
                    order[s[j]] = s[k];
                    idx.data[s[j]] = (k + 1) as f64;
                }
            }
            let mut sorted = m.gather(&order, m.dims.clone());
            sorted.kind = m.kind;
            Ok(Some(vec![Value::Num(sorted), Value::Num(idx)]))
        }
        "sortrows" if matches!(a.first(), Some(Value::Cell(_) | Value::Str(_))) => {
            // Rows of text, compared column by column in character-code order.
            let v = &a[0];
            let (dims, keys): (Vec<usize>, Vec<Option<String>>) = match v {
                Value::Cell(c) => (c.dims.clone(), c.data.iter().map(|x| x.text().map(Some)).collect::<R<_>>().map_err(|_| LangError::new("Only cell arrays of character vectors are supported by SORTROWS.", 0))?),
                Value::Str(sa) => (sa.dims.clone(), sa.data.clone()),
                _ => unreachable!(),
            };
            let (r, c) = (dims[0], if dims[0] == 0 { 0 } else { keys.len() / dims[0] });
            let cols: Vec<(usize, bool)> = match a.get(1) {
                Some(x) if !x.is_text() => x.to_matrix()?.data.iter().map(|&k| ((k.abs() as usize).saturating_sub(1), k < 0.0)).collect(),
                _ => (0..c).map(|k| (k, false)).collect(),
            };
            let desc_all = a.iter().skip(1).any(|x| x.text().is_ok_and(|t| t.eq_ignore_ascii_case("descend")));
            let mut order: Vec<usize> = (0..r).collect();
            order.sort_by(|&x, &y| {
                for &(k, d) in &cols {
                    let (p, q) = (&keys[k * r + x], &keys[k * r + y]);
                    // <missing> sorts last.
                    let o = match (p, q) {
                        (Some(p), Some(q)) => p.as_bytes().cmp(q.as_bytes()),
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (Some(_), None) => std::cmp::Ordering::Less,
                        _ => std::cmp::Ordering::Equal,
                    };
                    let o = if d || desc_all { o.reverse() } else { o };
                    if o != std::cmp::Ordering::Equal {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            });
            let rows = IndexArg::List { ix: order.clone(), dims: vec![order.len(), 1], from_mask: false };
            let sorted = indexing::index(v, &[rows, IndexArg::All])?;
            Ok(Some(vec![sorted, Value::Num(Matrix::col(order.iter().map(|&i| (i + 1) as f64).collect()))]))
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
            let flag = |f: &str| a[1..].iter().any(|v| v.text().map(|t| t.eq_ignore_ascii_case(f)).unwrap_or(false));
            let (stable, rows, last) = (flag("stable"), flag("rows"), flag("last"));
            // Items: the elements, or the rows with 'rows'.
            let (r, c) = (m.rows(), m.cols());
            let items: Vec<Vec<f64>> = if rows { (0..r).map(|i| (0..c).map(|j| m.data[j * r + i]).collect()).collect() } else { Vec::new() };
            let n_items = if rows { items.len() } else { m.data.len() };
            // Element `i` as a slice (a single value unless rows are compared).
            let item = |i: usize| -> &[f64] { if rows { &items[i] } else { std::slice::from_ref(&m.data[i]) } };
            // Sort the items (NaN last), then group equal neighbours; NaN is never equal to
            // anything, so each NaN is its own item.
            let cmp = |p: &[f64], q: &[f64]| {
                for (x, y) in p.iter().zip(q) {
                    let o = match (x.is_nan(), y.is_nan()) {
                        (true, true) => std::cmp::Ordering::Equal,
                        (true, false) => std::cmp::Ordering::Greater,
                        (false, true) => std::cmp::Ordering::Less,
                        _ => x.partial_cmp(y).unwrap(),
                    };
                    if o != std::cmp::Ordering::Equal {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            };
            let same = |p: &[f64], q: &[f64]| p.iter().zip(q).all(|(x, y)| x == y);
            let mut sorted: Vec<usize> = (0..n_items).collect();
            if n_items >= ops::PAR_MIN {
                use rayon::slice::ParallelSliceMut;
                sorted.par_sort_by(|&i, &j| cmp(item(i), item(j)));
            } else {
                sorted.sort_by(|&i, &j| cmp(item(i), item(j)));
            }
            let mut groups: Vec<(usize, usize)> = Vec::new(); // (first, last) occurrence, in sorted order
            let mut ic_raw = vec![0usize; n_items];
            for (k, &i) in sorted.iter().enumerate() {
                if k > 0 && same(item(sorted[k - 1]), item(i)) {
                    let g = groups.len() - 1;
                    groups[g].0 = groups[g].0.min(i);
                    groups[g].1 = groups[g].1.max(i);
                    ic_raw[i] = g;
                } else {
                    ic_raw[i] = groups.len();
                    groups.push((i, i));
                }
            }
            let mut order: Vec<usize> = (0..groups.len()).collect();
            if stable {
                order.sort_by_key(|&g| groups[g].0);
            }
            let mut rank = vec![0usize; groups.len()];
            for (k, &g) in order.iter().enumerate() {
                rank[g] = k;
            }
            let ia: Vec<f64> = order.iter().map(|&g| (if last { groups[g].1 } else { groups[g].0 } + 1) as f64).collect();
            let ic: Vec<f64> = ic_raw.iter().map(|&g| (rank[g] + 1) as f64).collect();
            let k = order.len();
            let out = if rows {
                let mut d = vec![0.0; k * c];
                for (ri, &g) in order.iter().enumerate() {
                    for j in 0..c {
                        d[j * k + ri] = item(groups[g].0)[j];
                    }
                }
                Matrix::new(k, c, d)
            } else {
                let vals: Vec<f64> = order.iter().map(|&g| m.data[groups[g].0]).collect();
                if m.dims.len() == 2 && m.dims[0] == 1 { Matrix::row(vals) } else { Matrix::col(vals) }
            };
            Ok(Some(vec![Value::Num(out.with_kind(m.kind)), Value::Num(Matrix::col(ia)), Value::Num(Matrix::col(ic))]))
        }
        "ismember" => {
            let x = arg(a, 0, name)?;
            let set = arg(a, 1, name)?;
            if x.is_text() || matches!(x, Value::Cell(_) | Value::Str(_)) {
                return crate::builtins_text::ismember_text(x, set);
            }
            let xm = x.to_matrix()?;
            let sm = set.to_matrix()?;
            if a[2..].iter().any(|v| v.text().map(|t| t.eq_ignore_ascii_case("rows")).unwrap_or(false)) {
                if xm.cols() != sm.cols() {
                    return rt("Inputs A and S must have the same number of columns in the 'rows' case.");
                }
                let (xr, sr, c) = (xm.rows(), sm.rows(), xm.cols());
                // Rows as keys of the bit patterns (-0 as 0); a row holding NaN matches nothing.
                let key = |m: &Matrix, rows: usize, i: usize| -> Option<Vec<u64>> {
                    (0..c).map(|j| {
                        let v = m.data[j * rows + i];
                        if v.is_nan() { None } else { Some((if v == 0.0 { 0.0f64 } else { v }).to_bits()) }
                    }).collect()
                };
                let mut first: std::collections::HashMap<Vec<u64>, usize> = std::collections::HashMap::with_capacity(sr);
                for k in 0..sr {
                    if let Some(kk) = key(&sm, sr, k) {
                        first.entry(kk).or_insert(k);
                    }
                }
                let loc: Vec<f64> = (0..xr).map(|i| key(&xm, xr, i).and_then(|kk| first.get(&kk).copied()).map(|k| (k + 1) as f64).unwrap_or(0.0)).collect();
                return Ok(Some(vec![
                    Value::Num(Matrix::col(loc.iter().map(|&l| (l > 0.0) as u8 as f64).collect()).with_kind(Kind::Logical)),
                    Value::Num(Matrix::col(loc)),
                ]));
            }
            // First position of each value of the set (-0 and 0 are equal, NaN equals nothing).
            let norm = |v: f64| if v == 0.0 { 0.0f64 } else { v };
            let mut first: std::collections::HashMap<u64, usize> = std::collections::HashMap::with_capacity(sm.data.len());
            for (p, &s) in sm.data.iter().enumerate() {
                if !s.is_nan() {
                    first.entry(norm(s).to_bits()).or_insert(p);
                }
            }
            let loc: Vec<f64> = xm.data.iter().map(|&v| if v.is_nan() { 0.0 } else { first.get(&norm(v).to_bits()).map(|&p| (p + 1) as f64).unwrap_or(0.0) }).collect();
            Ok(Some(vec![
                Value::Num(Matrix::from_dims(xm.dims.clone(), loc.iter().map(|&l| (l > 0.0) as u8 as f64).collect()).with_kind(Kind::Logical)),
                Value::Num(Matrix::from_dims(xm.dims.clone(), loc)),
            ]))
        }
        "find" => {
            let m = m_cow(a, 0, name)?;
            let limit = match a.get(1) {
                Some(v) => v.as_usize()?,
                None => usize::MAX,
            };
            let last = a.get(2).map(|v| v.text().map(|t| t == "last").unwrap_or(false)).unwrap_or(false);
            if limit == usize::MAX && !last && nargout < 2 && !m.is_complex() {
                use rayon::prelude::*;
                const CH: usize = 1 << 16;
                let pick = |c: usize, ch: &[f64]| -> Vec<f64> { ch.iter().enumerate().filter(|(_, v)| **v != 0.0).map(|(i, _)| (c * CH + i + 1) as f64).collect() };
                let data: Vec<f64> = if m.numel() >= ops::PAR_MIN {
                    let parts: Vec<Vec<f64>> = m.data.par_chunks(CH).enumerate().map(|(c, ch)| pick(c, ch)).collect();
                    let mut out = Vec::with_capacity(parts.iter().map(Vec::len).sum());
                    parts.into_iter().for_each(|p| out.extend(p));
                    out
                } else {
                    m.data.chunks(CH).enumerate().flat_map(|(c, ch)| pick(c, ch)).collect()
                };
                return mat(if m.dims.len() == 2 && m.dims[0] == 1 { Matrix::row(data) } else { Matrix::col(data) });
            }
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
            if x.numel() != y.numel() || (!(x.is_vector() && y.is_vector()) && x.dims != y.dims) {
                return rt("A and B must be the same size.");
            }
            let term = |i: usize| C::new(x.data[i], -x.im_at(i)).mul(C::new(y.data[i], y.im_at(i)));
            let dim = match a.get(2) {
                Some(v) => Some(v.as_usize()?),
                None if x.is_vector() && y.is_vector() => None,
                None => Some(if x.rows() != 1 { 1 } else { 2 }),
            };
            let Some(dim) = dim else {
                let s = (0..x.numel()).fold(C::new(0.0, 0.0), |acc, i| acc.add(term(i)));
                return mat(Matrix::complex(vec![1, 1], vec![s.re], vec![s.im]));
            };
            // Matrices: one dot product per column (dim 1) or per row (dim 2).
            let (r, c) = (x.rows(), x.cols());
            let (n, len) = if dim == 1 { (c, r) } else { (r, c) };
            let mut re = Vec::with_capacity(n);
            let mut im = Vec::with_capacity(n);
            for k in 0..n {
                let s = (0..len).fold(C::new(0.0, 0.0), |acc, t| acc.add(term(if dim == 1 { k * r + t } else { t * r + k })));
                re.push(s.re);
                im.push(s.im);
            }
            let dims = if dim == 1 { vec![1, n] } else { vec![n, 1] };
            mat(Matrix::complex(dims, re, im))
        }
        "cross" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            let three = |p: &[f64], q: &[f64]| [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]];
            if x.numel() == 3 && y.numel() == 3 && a.len() < 3 {
                let d = three(&x.data, &y.data).to_vec();
                return mat(if x.dims[0] == 1 { Matrix::row(d) } else { Matrix::col(d) });
            }
            if x.dims != y.dims {
                return rt("A and B must be of length 3 in the dimension in which the cross product is taken.");
            }
            let (r, c) = (x.rows(), x.cols());
            let dim = match a.get(2) {
                Some(v) => v.as_usize()?,
                None => if r == 3 { 1 } else if c == 3 { 2 } else { 0 },
            };
            if (dim == 1 && r != 3) || (dim == 2 && c != 3) || dim == 0 || dim > 2 {
                return rt("A and B must be of length 3 in the dimension in which the cross product is taken.");
            }
            // One cross product per column (dim 1) or per row (dim 2).
            let mut out = vec![0.0; r * c];
            let n = if dim == 1 { c } else { r };
            for k in 0..n {
                let at = |t: usize| if dim == 1 { k * r + t } else { t * r + k };
                let p: Vec<f64> = (0..3).map(|t| x.data[at(t)]).collect();
                let q: Vec<f64> = (0..3).map(|t| y.data[at(t)]).collect();
                for (t, v) in three(&p, &q).into_iter().enumerate() {
                    out[at(t)] = v;
                }
            }
            mat(Matrix::new(r, c, out))
        }
        "norm" => {
            let m = m_cow(a, 0, name)?;
            // Real vector 2-norm: a plain sum of squares (rescaled when it could overflow or
            // underflow) instead of a power function per element.
            if a.len() == 1 && !m.is_complex() && m.is_vector() && m.numel() > 0 {
                let big = m.data.iter().fold(0.0f64, |acc, x| acc.max(x.abs()));
                if big.is_nan() || m.data.iter().any(|x| x.is_nan()) {
                    return num(f64::NAN);
                }
                if big == 0.0 || big.is_infinite() {
                    return num(big);
                }
                if big > 1e-150 && big < 1e150 {
                    return num(m.data.iter().map(|x| x * x).sum::<f64>().sqrt());
                }
                return num(big * m.data.iter().map(|x| (x / big) * (x / big)).sum::<f64>().sqrt());
            }
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
            if m.dims.len() > 2 {
                return rt("Input must be 2-D.");
            }
            let pv = match p {
                Some(v) if v.is_text() => match v.text()?.to_lowercase().as_str() {
                    "inf" => f64::INFINITY,
                    _ => return rt("The only matrix norms available are 1, 2, inf, and 'fro'."),
                },
                Some(v) => v.scalar()?,
                None => 2.0,
            };
            let (r, c) = (m.rows(), m.cols());
            if pv == 1.0 {
                return num((0..c).map(|j| mag[j * r..(j + 1) * r].iter().sum::<f64>()).fold(0.0, f64::max));
            }
            if pv == f64::INFINITY {
                return num((0..r).map(|i| (0..c).map(|j| mag[j * r + i]).sum::<f64>()).fold(0.0, f64::max));
            }
            if pv != 2.0 {
                return rt("The only matrix norms available are 1, 2, inf, and 'fro'.");
            }
            if m.is_complex() {
                let s = it.call_function("svd", vec![Value::Num(m.clone().into_owned())], 1)?.remove(0);
                return num(s.to_matrix()?.data.iter().copied().fold(0.0, f64::max));
            }
            num(crate::builtins_matfun::two_norm(&m)?)
        }
        "det" => num(ops::det(&m_arg(a, 0, name)?)?),
        "inv" => {
            let m = m_arg(a, 0, name)?;
            match ops::inv_any(&m) {
                Ok(x) => mat(x),
                // A singular matrix gives a warning and an Inf result, as in MATLAB.
                Err(e) if e.message.contains("singular") => {
                    it.emit(crate::interp::OutputKind::Warning("Matrix is singular to working precision.".into()));
                    let n = m.rows();
                    mat(Matrix::from_dims(vec![n, n], vec![f64::INFINITY; n * n]))
                }
                Err(e) => Err(e),
            }
        }
        "trace" => {
            let m = m_arg(a, 0, name)?;
            let k = m.rows().min(m.cols());
            if m.is_complex() {
                let (re, im): (f64, f64) = (0..k).fold((0.0, 0.0), |(r, i), d| (r + m.get(d, d), i + m.im_at(d * m.rows() + d)));
                let mut out = Matrix::complex(vec![1, 1], vec![re], vec![im]);
                out.narrow();
                return one(Value::Num(out));
            }
            num((0..k).map(|i| m.get(i, i)).sum())
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
                let (vals, vecs) = crate::builtins_matfun::eig_symmetric(&m, nargout >= 2)?;
                if let Some(vecs) = vecs {
                    let mut d = Matrix::zeros(n, n);
                    for i in 0..n {
                        d.set(i, i, vals[i]);
                    }
                    return Ok(Some(vec![Value::Num(vecs), Value::Num(d)]));
                }
                return mat(Matrix::col(vals));
            }
            if nargout >= 2 {
                let (v, d) = crate::builtins_matfun::eig_vd(&m)?;
                return Ok(Some(vec![v, d]));
            }
            Ok(Some(vec![crate::builtins_matfun::eig_values(&m)?]))
        }
        "roots" => {
            let p = m_arg(a, 0, name)?;
            let c: Vec<C> = (0..p.numel()).map(|i| C::new(p.data[i], p.im_at(i))).collect();
            let r = crate::signal::poly::roots_matlab(&c)?;
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
            let ks: Vec<i64> = m_arg(a, 1, name)?.data.iter().map(|&x| x as i64).collect();
            // A vector of shifts moves dimension i by ks[i]; a scalar shifts one dimension.
            let shifts: Vec<(usize, i64)> = match a.get(2) {
                Some(d) => vec![(d.as_usize()?, ks.first().copied().unwrap_or(0))],
                None if ks.len() == 1 => vec![(default_dim(&m.dims), ks[0])],
                None => ks.iter().enumerate().map(|(i, &k)| (i + 1, k)).collect(),
            };
            let mut out = m.clone();
            for (dim, k) in shifts {
                if k == 0 || dim > out.dims.len() {
                    continue;
                }
                let src_m = out.clone();
                let (_, sl) = slices(&src_m.dims, dim);
                for s in sl {
                    let n = s.len() as i64;
                    for (j, &p) in s.iter().enumerate() {
                        let src = (((j as i64 - k) % n + n) % n) as usize;
                        out.data[p] = src_m.data[s[src]];
                        if let (Some(o), Some(i)) = (out.im.as_mut(), src_m.im.as_ref()) {
                            o[p] = i[s[src]];
                        }
                    }
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
        // [G,C,D] = gcd(A,B): Bezout coefficients with A.*C + B.*D = G.
        "gcd" if nargout >= 2 => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            let kind = ops::arith_kind(x.kind, y.kind).unwrap_or(Kind::Double);
            let ext = |p: f64, q: f64| -> (f64, f64, f64) {
                let (mut r0, mut r1) = (p.abs(), q.abs());
                let (mut s0, mut s1) = (1.0, 0.0);
                let (mut t0, mut t1) = (0.0, 1.0);
                while r1 != 0.0 {
                    let k = (r0 / r1).floor();
                    (r0, r1) = (r1, r0 - k * r1);
                    (s0, s1) = (s1, s0 - k * s1);
                    (t0, t1) = (t1, t0 - k * t1);
                }
                (r0, if p < 0.0 { -s0 } else { s0 }, if q < 0.0 { -t0 } else { t0 })
            };
            let g = broadcast(&x, &y, |p, q| ext(p, q).0)?;
            let c = broadcast(&x, &y, |p, q| ext(p, q).1)?;
            let d = broadcast(&x, &y, |p, q| ext(p, q).2)?;
            Ok(Some(vec![Value::Num(g.cast_to(kind)), Value::Num(c.cast_to(kind)), Value::Num(d.cast_to(kind))]))
        }
        "mod" | "rem" | "atan2" | "hypot" | "power" | "gcd" | "lcm" | "nchoosek" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            if name == "power" {
                return mat(ops::num_binary(crate::ast::BinOp::Pow, &x, &y)?);
            }
            // nchoosek(v, k) with a vector v: every combination of k of its elements, one per row.
            if name == "nchoosek" && x.numel() > 1 {
                let k = y.data.first().copied().unwrap_or(0.0) as usize;
                let n = x.numel();
                if k > n {
                    return mat(Matrix::new(0, k, Vec::new()).with_kind(x.kind));
                }
                let mut rows: Vec<Vec<usize>> = Vec::new();
                let mut idx: Vec<usize> = (0..k).collect();
                loop {
                    rows.push(idx.clone());
                    let Some(i) = (0..k).rev().find(|&i| idx[i] != i + n - k) else { break };
                    idx[i] += 1;
                    for j in i + 1..k {
                        idx[j] = idx[j - 1] + 1;
                    }
                    if k == 0 {
                        break;
                    }
                }
                let r = rows.len();
                let mut data = vec![0.0; r * k];
                for (ri, row) in rows.iter().enumerate() {
                    for (ci, &e) in row.iter().enumerate() {
                        data[ci * r + ri] = x.data[e];
                    }
                }
                return mat(Matrix::new(r, k, data).with_kind(x.kind));
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
            // With a third output, x is centred and scaled: mu = [mean(x); std(x)].
            let mu = if nargout >= 3 {
                let mean = x.data.iter().sum::<f64>() / n as f64;
                let sd = (x.data.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0)).sqrt();
                Some((mean, sd))
            } else {
                None
            };
            let xs: Vec<f64> = x.data.iter().map(|&v| match mu { Some((m, s)) => (v - m) / s, None => v }).collect();
            let cols = deg + 1;
            let mut v = vec![0.0; n * cols];
            for i in 0..n {
                for j in 0..cols {
                    v[j * n + i] = xs[i].powi((deg - j) as i32);
                }
            }
            // Householder QR of the Vandermonde matrix (R's diagonal gets the sign LAPACK gives).
            let mut qr = v.clone();
            let mut qy = y.data.clone();
            for k in 0..cols.min(n) {
                let norm = (k..n).map(|i| qr[k * n + i].powi(2)).sum::<f64>().sqrt();
                if norm == 0.0 {
                    continue;
                }
                let alpha = qr[k * n + k];
                let beta = if alpha >= 0.0 { -norm } else { norm };
                let mut h: Vec<f64> = (k..n).map(|i| qr[k * n + i]).collect();
                h[0] -= beta;
                let hh: f64 = h.iter().map(|t| t * t).sum();
                if hh == 0.0 {
                    continue;
                }
                for j in k..cols {
                    let dot: f64 = (k..n).map(|i| h[i - k] * qr[j * n + i]).sum();
                    for i in k..n {
                        qr[j * n + i] -= 2.0 * dot / hh * h[i - k];
                    }
                }
                let dot: f64 = (k..n).map(|i| h[i - k] * qy[i]).sum();
                for i in k..n {
                    qy[i] -= 2.0 * dot / hh * h[i - k];
                }
            }
            let mut p = vec![0.0; cols];
            for k in (0..cols.min(n)).rev() {
                let mut t = qy[k];
                for j in k + 1..cols {
                    t -= qr[j * n + k] * p[j];
                }
                p[k] = t / qr[k * n + k];
            }
            let mut out = vec![Value::Num(Matrix::row(p.clone()))];
            if nargout >= 2 {
                let mut r = Matrix::zeros(cols, cols);
                for j in 0..cols {
                    for i in 0..=j.min(n.saturating_sub(1)) {
                        r.set(i, j, qr[j * n + i]);
                    }
                }
                let resid: f64 = (0..n).map(|i| (y.data[i] - (0..cols).map(|j| v[j * n + i] * p[j]).sum::<f64>()).powi(2)).sum::<f64>().sqrt();
                let df = (n as f64 - cols as f64).max(0.0);
                let st = StructArray { dims: vec![1, 1], fields: vec!["R".into(), "df".into(), "normr".into()], elems: vec![vec![Value::Num(r), Value::num(df), Value::num(resid)]] };
                out.push(Value::Struct(st));
            }
            if let Some((m, s)) = mu {
                out.push(Value::Num(Matrix::col(vec![m, s])));
            }
            Ok(Some(out))
        }
        "polyval" => {
            let p = m_arg(a, 0, name)?;
            let x = m_arg(a, 1, name)?;
            if !p.is_complex() && !x.is_complex() {
                return mat(x.map(|xv| p.data.iter().fold(0.0, |acc, &c| acc * xv + c)));
            }

            let coeffs: Vec<C> = (0..p.numel()).map(|k| C::new(p.data[k], p.im_at(k))).collect();
            let (re, im): (Vec<f64>, Vec<f64>) = (0..x.numel())
                .map(|k| {
                    let xv = C::new(x.data[k], x.im_at(k));
                    let v = coeffs.iter().fold(C::new(0.0, 0.0), |acc, &c| acc.mul(xv).add(c));
                    (v.re, v.im)
                })
                .unzip();
            mat(Matrix::complex(x.dims.clone(), re, im))
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
        "histcounts" => return crate::builtins_datafun::histcounts(a, nargout),
        "accumarray" => return accumarray(it, a).map(|v| Some(vec![v])),

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
            Ok(Some(args.to_vec()))
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
            let Some(first) = inputs.first() else { return rt("Not enough input arguments.") };
            let dims = first.dims();
            let n = nd::numel(&dims);
            let nout = nargout.max(1);
            let mut results: Vec<Vec<Value>> = vec![Vec::with_capacity(n); nout];
            for k in 0..n {
                let call_args = inputs
                    .iter()
                    .map(|v| match crate::gfx::ids_of(v) {
                        // Arrays of graphics objects pass one object at a time.
                        Some(ids) if ids.len() > 1 => Ok(crate::gfx::handle(it, ids[k])),
                        _ => element(v, k, name == "cellfun"),
                    })
                    .collect::<R<Vec<_>>>()?;
                let out = match it.call_func_value(&f, call_args.clone(), nout) {
                    Ok(out) => out,
                    // The handler gets S (identifier, message, index) and the same inputs.
                    Err(e) if error_handler.is_some() => {
                        let s = StructArray {
                            dims: vec![1, 1],
                            fields: vec!["identifier".into(), "message".into(), "index".into()],
                            elems: vec![vec![Value::chars(&e.identifier), Value::chars(&e.message), Value::num((k + 1) as f64)]],
                        };
                        let mut hargs = vec![Value::Struct(s)];
                        hargs.extend(call_args);
                        it.call_func_value(error_handler.as_ref().unwrap(), hargs, nout)?
                    }
                    Err(e) => return Err(e),
                };
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
                // Scalar strings join into a string array.
                if uniform && !res.is_empty() && res.iter().all(|v| matches!(v, Value::Str(s) if s.data.len() == 1)) {
                    let data = res.into_iter().map(|v| if let Value::Str(s) = v { s.data.into_iter().next().unwrap() } else { unreachable!() }).collect();
                    outs.push(Value::Str(StrArray::from_dims(dims.clone(), data)));
                    continue;
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
            // t = tic: a time stamp in microseconds for toc(t); the toc timer is left alone.
            if nargout >= 1 {
                let us = tic_epoch().elapsed().as_micros() as f64 + 1.0;
                return mat(Matrix::scalar(us).with_kind(Kind::Int(IntClass::U64)));
            }
            it.tic = std::time::Instant::now();
            none()
        }
        "toc" => {
            let t = match a.first() {
                Some(v) => {
                    let m = v.to_matrix()?;
                    if m.kind != Kind::Int(IntClass::U64) || m.numel() != 1 {
                        return rt("Input argument must be of type uint64.");
                    }
                    (tic_epoch().elapsed().as_micros() as f64 + 1.0 - m.data[0]).max(0.0) / 1e6
                }
                None => it.tic.elapsed().as_secs_f64(),
            };
            if nargout >= 1 {
                return num(t);
            }
            it.emit(OutputKind::Text(format!("Elapsed time is {t:.6} seconds.\n")));
            none()
        }
        // Running timers fire while pause waits and when drawnow flushes.
        "pause" if crate::timers::any_running(it) => {
            if let Some(Ok(n)) = a.first().filter(|v| !v.is_text()).map(|v| v.scalar()) {
                let n = if n.is_finite() && n > 0.0 { n } else { 0.0 };
                crate::timers::service(it, Some(std::time::Instant::now() + std::time::Duration::from_secs_f64(n)), None);
            }
            none()
        }
        "drawnow" if crate::timers::any_running(it) => {
            crate::timers::service(it, None, None);
            none()
        }
        "pause" | "drawnow" | "clc" | "validateattributes" | "narginchk" | "nargchk" => none(),
        "pwd" => one(Value::chars(&std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default())),
        "clear" => {
            if a.is_empty() || a.iter().any(|v| v.text().map(|s| s == "all" || s == "variables").unwrap_or(false)) {
                it.clear_current_scope();
            } else {
                // Names, wildcards (`a*`), or regular expressions after `-regexp`.
                let mut regexp = false;
                for v in a.iter() {
                    let pat = v.text()?;
                    if pat == "-regexp" {
                        regexp = true;
                        continue;
                    }
                    if regexp {
                        let re = fancy_regex::Regex::new(&pat).map_err(|e| LangError::new(e.to_string(), 0))?;
                        for n in it.var_names() {
                            if re.is_match(&n).unwrap_or(false) {
                                it.remove_var(&n);
                            }
                        }
                    } else if pat.contains('*') {
                        let re = format!("^{}$", pat.split('*').map(fancy_regex::escape).collect::<Vec<_>>().join(".*"));
                        let re = fancy_regex::Regex::new(&re).map_err(|e| LangError::new(e.to_string(), 0))?;
                        for n in it.var_names() {
                            if re.is_match(&n).unwrap_or(false) {
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
            let words = a.iter().map(|v| v.text()).collect::<R<Vec<_>>>()?.join(" ");
            match NumFormat::parse(&words) {
                Some(Some(f)) => it.format = f,
                Some(None) => {}
                None => return rt(format!("Unknown format: {words}")),
            }
            none()
        }
        "disp" => {
            let v = arg(a, 0, name)?;
            if let Some(text) = crate::containers::cat_disp(v) {
                it.emit(OutputKind::Text(text + "\n"));
                return none();
            }
            if let Value::Object(o) = v {
                if let Some(f) = o.class.method("disp").cloned() {
                    let class = o.class.clone();
                    it.call_method(&f, &class, vec![v.clone()], 0)?;
                    return none();
                }
                if let Some(t) = crate::decomp::object_text(o).or_else(|| crate::tiff::object_text(o)).or_else(|| crate::ode_objects::object_text(o)) {
                    it.emit(OutputKind::Text(t));
                    return none();
                }
                if let Some(t) = crate::settings::node_text(it, o) {
                    it.emit(OutputKind::Text(t + "\n"));
                    return none();
                }
                if let Some(t) = crate::func_objects::object_text("", o, Some(crate::random::global_id(it))) {
                    it.emit(OutputKind::Text(t + "\n"));
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
            let mut id = String::new();
            if parts.len() >= 2 {
                if let Ok(t) = parts[0].text() {
                    if t.contains(':') && !t.contains(' ') && parts[1].is_text() {
                        id = t;
                        parts = &parts[1..];
                    }
                }
            }
            if let Some(Value::Object(o)) = parts.first() {
                if let Some(p) = crate::messages::parts(o) {
                    let (msg, id) = p?;
                    return Err(LangError::new(msg, 0).with_id(id).raised());
                }
            }
            let msg = match parts.first() {
                Some(v @ (Value::Struct(_) | Value::Object(_))) => {
                    let (m, i) = crate::builtins_validate::error_parts(v).unwrap_or_default();
                    id = i;
                    m
                }
                Some(f) => {
                    if parts.len() > 1 { sprintf(&f.text()?, &parts[1..])? } else { f.text()? }
                }
                None => return none(),
            };
            if msg.is_empty() {
                return none();
            }
            Err(LangError::new(msg, 0).with_id(id).raised())
        }
        "warning" => {
            let mut parts: &[Value] = a;
            let mut id = String::new();
            if parts.len() >= 2 {
                if let Ok(t) = parts[0].text() {
                    if t.contains(':') && !t.contains(' ') {
                        id = t;
                        parts = &parts[1..];
                    }
                }
            }
            if let Some(Value::Object(o)) = parts.first() {
                if let Some(p) = crate::messages::parts(o) {
                    let (msg, id) = p?;
                    it.last_warning = (msg.clone(), id);
                    it.emit(OutputKind::Warning(msg));
                    return none();
                }
            }
            if let Some(f) = parts.first() {
                let t = f.text()?;
                if t == "off" || t == "on" || t == "query" {
                    return none();
                }
                let msg = if parts.len() > 1 { sprintf(&t, &parts[1..])? } else { t };
                it.last_warning = (msg.clone(), id);
                it.emit(OutputKind::Warning(msg));
            }
            none()
        }
        "assert" => {
            if !arg(a, 0, name)?.is_true()? {
                // assert(cond, id, msg, ...): a second argument shaped like an identifier.
                let is_id = |t: &str| t.contains(':') && !t.contains(char::is_whitespace) && t.split(':').all(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
                let (id, msg) = match a.get(1).map(|v| v.text()).transpose()? {
                    Some(t) if is_id(&t) => match a.get(2) {
                        Some(f) => (t, sprintf(&f.text()?, &a[3..])?),
                        None => {
                            return Err(LangError::new("If you specify a message identifier argument, you must specify the message text argument.", 0)
                                .with_id("MATLAB:error:missingMessageArgument"))
                        }
                    },
                    Some(f) => (String::new(), sprintf(&f, &a[2..])?),
                    None => (String::new(), "Assertion failed.".into()),
                };
                return Err(LangError::new(msg, 0).with_id(id).raised());
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
            if a.len() > 2 {
                return rt("Too many input arguments.");
            }
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

pub(crate) fn gamma_fn(x: f64) -> f64 {
    gamma(x)
}

/// Lanczos approximation of the gamma function.
fn gamma(x: f64) -> f64 {
    if x.fract() == 0.0 && x <= 0.0 {
        return f64::INFINITY;
    }
    // Whole numbers: the exact factorial while it is finite.
    if x.fract() == 0.0 && x <= 171.0 {
        return (1..x as u32).fold(1.0, |p, k| p * k as f64);
    }
    if x > 171.62 {
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
    // t^(x+1/2) e^-t in two halves, so that it does not overflow before the product does.
    let half = t.powf((x + 0.5) / 2.0);
    (2.0 * std::f64::consts::PI).sqrt() * half * (-t).exp() * half * a
}
