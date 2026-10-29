//! Built-in functions. `call` returns `Ok(None)` when the name is not a builtin.

use crate::display::{disp_value, fmt_g, NumFormat};
use crate::error::{rt, LangError};
use crate::format::sprintf;
use crate::graphics::{self, Axes, Figure, LineStyle, Series, SeriesKind};
use crate::interp::{hconcat, range_count, vconcat, Interpreter, OutputKind};
use crate::ops;
use crate::value::*;

type R<T> = Result<T, LangError>;
type Ret = R<Option<Vec<Value>>>;

const NAMES: &[&str] = &[
    "abs", "acos", "acosd", "all", "any", "area", "arrayfun", "asin", "asind", "assert", "atan", "atan2", "atand", "axis", "bar",
    "blanks", "cat", "ceil", "cell", "cellfun", "char", "circshift", "class", "clc", "clear", "clf", "close", "colon", "contains",
    "cos", "cosd", "cosh", "cross", "cumprod", "cumsum", "deal", "deg2rad", "det", "diag", "diff", "disp", "display", "dot",
    "double", "drawnow", "endsWith", "eps", "error", "exist", "exp", "eye", "factorial", "false", "feval", "fieldnames",
    "figure", "find", "fix", "flip", "fliplr", "flipud", "floor", "format", "fprintf", "func2str", "gamma", "gcd", "grid",
    "histcounts", "hold", "horzcat", "hypot", "idivide", "Inf", "inf", "int2str", "interp1", "inv", "isa", "iscell", "iscellstr",
    "ischar", "iscolumn", "isempty", "isequal", "isfield", "isfinite", "isinf", "islogical", "ismatrix", "ismember", "isnan",
    "isnumeric", "isprime", "isreal", "isrow", "isscalar", "isspace", "isstring", "isstruct", "isvector", "lcm", "legend",
    "length", "linspace", "log", "log10", "log1p", "log2", "logical", "logspace", "lower", "mat2str", "max", "mean", "median",
    "meshgrid", "min", "mod", "NaN", "nan", "nchoosek", "ndims", "newline", "nnz", "norm", "num2cell", "num2str", "numel",
    "ones", "pause", "pi", "plot", "polyfit", "polyval", "power", "primes", "prod", "rad2deg", "rand", "randi", "randn",
    "randperm", "rem", "repmat", "reshape", "rmfield", "rng", "round", "scatter", "sign", "sin", "sind", "sinh", "size", "sort",
    "sprintf", "sqrt", "stairs", "startsWith", "std", "str2double", "str2func", "str2num", "strcat", "strcmp", "strcmpi",
    "string", "strjoin", "strlength", "strrep", "strsplit", "strtrim", "struct", "subplot", "sum", "tan", "tand", "tanh",
    "tic", "title", "toc", "trace", "transpose", "trapz", "true", "unique", "upper", "var", "vertcat", "xlabel", "xlim",
    "ylabel", "ylim", "zeros",
];

pub fn exists(name: &str) -> bool {
    NAMES.contains(&name)
}

fn one(v: Value) -> Ret {
    Ok(Some(vec![v]))
}

fn none() -> Ret {
    Ok(Some(Vec::new()))
}

fn num(v: f64) -> Ret {
    one(Value::num(v))
}

fn mat(m: Matrix) -> Ret {
    one(Value::Num(m))
}

fn arg<'a>(args: &'a [Value], i: usize, fname: &str) -> R<&'a Value> {
    args.get(i).ok_or_else(|| LangError::new(format!("Not enough input arguments for '{fname}'."), 0))
}

fn m_arg(args: &[Value], i: usize, fname: &str) -> R<Matrix> {
    arg(args, i, fname)?.to_matrix()
}

fn bool_mat(m: &Matrix, f: impl Fn(f64) -> bool) -> Matrix {
    Matrix::logical(m.rows, m.cols, m.data.iter().map(|&x| if f(x) { 1.0 } else { 0.0 }).collect())
}

/// Parses size arguments: `()`, `(n)`, `(r, c)`, `([r c])`; trailing text options are ignored.
fn size_args(args: &[Value]) -> R<(usize, usize)> {
    let nums: Vec<&Value> = args.iter().filter(|a| !a.is_text()).collect();
    match nums.len() {
        0 => Ok((1, 1)),
        1 => {
            let m = nums[0].as_matrix()?;
            if m.numel() >= 2 {
                Ok((dim_of(m.data[0])?, dim_of(m.data[1])?))
            } else {
                let n = dim_of(nums[0].scalar()?)?;
                Ok((n, n))
            }
        }
        _ => Ok((dim_of(nums[0].scalar()?)?, dim_of(nums[1].scalar()?)?)),
    }
}

fn dim_of(v: f64) -> R<usize> {
    if v.is_nan() {
        return rt("Size inputs must be integers.");
    }
    Ok(if v < 0.0 { 0 } else { v as usize })
}

/// First non-singleton dimension (1-based).
fn default_dim(m: &Matrix) -> usize {
    if m.rows != 1 { 1 } else { 2 }
}

fn dim_arg(args: &[Value], i: usize, m: &Matrix) -> R<usize> {
    match args.get(i) {
        Some(v) if !v.is_text() && !matches!(v, Value::Num(x) if x.is_empty()) => {
            let d = v.scalar()? as usize;
            if d < 1 {
                return rt("Dimension argument must be a positive integer.");
            }
            Ok(d)
        }
        _ => Ok(default_dim(m)),
    }
}

/// Applies `f` to each column (dim 1) or row (dim 2) slice.
fn reduce(m: &Matrix, dim: usize, f: impl Fn(&[f64]) -> f64) -> Matrix {
    if m.rows == 0 && m.cols == 0 {
        return Matrix::scalar(f(&[]));
    }
    match dim {
        1 => {
            let data = (0..m.cols).map(|c| f(&m.data[c * m.rows..(c + 1) * m.rows])).collect();
            Matrix::new(1, m.cols, data)
        }
        2 => {
            let mut buf = vec![0.0; m.cols];
            let data = (0..m.rows)
                .map(|r| {
                    for c in 0..m.cols {
                        buf[c] = m.get(r, c);
                    }
                    f(&buf)
                })
                .collect();
            Matrix::new(m.rows, 1, data)
        }
        _ => m.map(|x| f(&[x])),
    }
}

/// Cumulative op along a dimension.
fn cumulate(m: &Matrix, dim: usize, f: impl Fn(f64, f64) -> f64) -> Matrix {
    let mut out = m.clone();
    out.kind = Kind::Double;
    if dim == 1 {
        for c in 0..m.cols {
            for r in 1..m.rows {
                let v = f(out.get(r - 1, c), m.get(r, c));
                out.set(r, c, v);
            }
        }
    } else {
        for r in 0..m.rows {
            for c in 1..m.cols {
                let v = f(out.get(r, c - 1), m.get(r, c));
                out.set(r, c, v);
            }
        }
    }
    out
}

fn median_of(v: &[f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    if n % 2 == 1 { s[n / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 }
}

fn variance(v: &[f64]) -> f64 {
    let n = v.len();
    if n <= 1 {
        return 0.0;
    }
    let mean = v.iter().sum::<f64>() / n as f64;
    v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64
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

fn text_list(v: &Value) -> R<Vec<String>> {
    match v {
        Value::Cell(c) => c.data.iter().map(|x| x.text()).collect(),
        Value::Num(m) if m.kind == Kind::Char && m.rows > 1 => Ok((0..m.rows)
            .map(|r| (0..m.cols).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or('?')).collect::<String>())
            .collect()),
        other => Ok(vec![other.text()?]),
    }
}

/// Wraps strings in the same container shape as `like` (cell in, cell out).
fn text_result(like: &Value, items: Vec<String>) -> Value {
    match like {
        Value::Cell(c) => Value::Cell(CellArray::new(c.rows, c.cols, items.into_iter().map(|s| Value::chars(&s)).collect())),
        Value::Str(_) => Value::Str(items.into_iter().next().unwrap_or_default()),
        _ => Value::chars(&items.into_iter().next().unwrap_or_default()),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x.rows == y.rows && x.cols == y.cols && x.data == y.data,
        (Value::Cell(x), Value::Cell(y)) => x.rows == y.rows && x.cols == y.cols && x.data.iter().zip(&y.data).all(|(p, q)| values_equal(p, q)),
        (Value::Struct(x), Value::Struct(y)) => {
            x.fields.len() == y.fields.len() && x.fields.iter().all(|(n, v)| y.get(n).map(|w| values_equal(v, w)).unwrap_or(false))
        }
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Str(x), y) | (y, Value::Str(x)) if y.is_char() => y.text().map(|t| &t == x).unwrap_or(false),
        _ => a == b,
    }
}

impl Interpreter {
    fn next_rand(&mut self) -> f64 {
        // xorshift64*
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

    fn current_figure(&mut self) -> &mut Figure {
        if self.current_figure.is_none() {
            let id = self.next_figure_id;
            self.next_figure_id += 1;
            self.figures.push(Figure::new(id));
            self.current_figure = Some(self.figures.len() - 1);
        }
        let i = self.current_figure.unwrap();
        let id = self.figures[i].id;
        self.mark_figure_dirty(id);
        &mut self.figures[i]
    }

    fn add_series(&mut self, series: Vec<Series>) {
        let ax = self.current_figure().axes_mut();
        if !ax.hold {
            let hold = ax.hold;
            let grid = ax.grid;
            *ax = Axes { hold, grid, ..Axes::default() };
        }
        ax.series.extend(series);
    }

    fn call_func_value(&mut self, f: &Value, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
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

fn plot_series(args: &[Value], kind: SeriesKind) -> R<Vec<Series>> {
    let mut out: Vec<Series> = Vec::new();
    let mut i = 0;
    let mut props: Vec<(String, Value)> = Vec::new();
    while i < args.len() {
        if args[i].is_text() {
            // Name-value pairs end the data groups.
            let name = args[i].text()?;
            if graphics::parse_line_spec(&name).is_some() && !out.is_empty() && props.is_empty() {
                i += 1;
                continue;
            }
            match args.get(i + 1) {
                Some(v) => props.push((name.to_lowercase(), v.clone())),
                None => return rt(format!("Invalid parameter/value pair arguments ('{name}').")),
            }
            i += 2;
            continue;
        }
        let a = args[i].to_matrix()?;
        let (x, y) = match args.get(i + 1) {
            Some(b) if !b.is_text() => {
                i += 2;
                (Some(a), b.to_matrix()?)
            }
            _ => {
                i += 1;
                (None, a)
            }
        };
        let mut spec = None;
        if let Some(s) = args.get(i) {
            if s.is_text() {
                if let Some(ls) = graphics::parse_line_spec(&s.text()?) {
                    spec = Some(ls);
                    i += 1;
                }
            }
        }
        // Columns of a matrix Y are separate series (rows if Y's rows don't match X).
        let mut ys: Vec<Vec<f64>> = Vec::new();
        if y.rows == 1 || y.cols == 1 {
            ys.push(y.data.clone());
        } else {
            let use_rows = matches!(&x, Some(xm) if xm.is_vector() && xm.numel() == y.cols && xm.numel() != y.rows);
            if use_rows {
                for r in 0..y.rows {
                    ys.push((0..y.cols).map(|c| y.get(r, c)).collect());
                }
            } else {
                for c in 0..y.cols {
                    ys.push(y.data[c * y.rows..(c + 1) * y.rows].to_vec());
                }
            }
        }
        for yv in ys {
            let xv = match &x {
                Some(xm) => xm.data.clone(),
                None => (1..=yv.len()).map(|k| k as f64).collect(),
            };
            if xv.len() != yv.len() {
                return rt("Vectors must be the same length.");
            }
            let mut s = Series::new(kind, xv, yv);
            if let Some((c, st, mk)) = spec {
                if c.is_some() {
                    s.color = c;
                }
                if let Some(st) = st {
                    s.line_style = st;
                } else if mk.is_some() && kind == SeriesKind::Line {
                    s.line_style = LineStyle::None;
                }
                if mk.is_some() {
                    s.marker = mk;
                }
            }
            out.push(s);
        }
    }
    for (name, v) in props {
        for s in out.iter_mut() {
            match name.as_str() {
                "linewidth" => s.line_width = v.scalar()? as f32,
                "markersize" => s.marker_size = v.scalar()? as f32,
                "displayname" => s.name = Some(v.text()?),
                "color" | "markerfacecolor" | "facecolor" => {
                    if v.is_text() {
                        s.color = graphics::named_color(&v.text()?).or(s.color);
                    } else {
                        let m = v.to_matrix()?;
                        if m.numel() >= 3 {
                            let k = if m.data.iter().take(3).all(|&c| c <= 1.0) { 255.0 } else { 1.0 };
                            s.color = Some([(m.data[0] * k) as u8, (m.data[1] * k) as u8, (m.data[2] * k) as u8]);
                        }
                    }
                }
                "linestyle" => {
                    if let Some((_, Some(st), _)) = graphics::parse_line_spec(&v.text()?) {
                        s.line_style = st;
                    } else if v.text()? == "none" {
                        s.line_style = LineStyle::None;
                    }
                }
                "marker" => {
                    if let Some((_, _, mk)) = graphics::parse_line_spec(&v.text()?) {
                        s.marker = mk;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(out)
}

pub fn call(it: &mut Interpreter, name: &str, args: Vec<Value>, nargout: usize) -> Ret {
    let a = &args;
    // Elementwise unary math.
    let unary: Option<fn(f64) -> f64> = match name {
        "sin" => Some(f64::sin),
        "cos" => Some(f64::cos),
        "tan" => Some(f64::tan),
        "asin" => Some(f64::asin),
        "acos" => Some(f64::acos),
        "atan" => Some(f64::atan),
        "sinh" => Some(f64::sinh),
        "cosh" => Some(f64::cosh),
        "tanh" => Some(f64::tanh),
        "sind" => Some(|x: f64| {
            if (x % 180.0) == 0.0 { 0.0 } else { x.to_radians().sin() }
        }),
        "cosd" => Some(|x: f64| {
            if ((x - 90.0) % 180.0) == 0.0 { 0.0 } else { x.to_radians().cos() }
        }),
        "tand" => Some(|x: f64| x.to_radians().tan()),
        "asind" => Some(|x: f64| x.asin().to_degrees()),
        "acosd" => Some(|x: f64| x.acos().to_degrees()),
        "atand" => Some(|x: f64| x.atan().to_degrees()),
        "deg2rad" => Some(f64::to_radians),
        "rad2deg" => Some(f64::to_degrees),
        "exp" => Some(f64::exp),
        "log" => Some(f64::ln),
        "log2" => Some(f64::log2),
        "log10" => Some(f64::log10),
        "log1p" => Some(f64::ln_1p),
        "sqrt" => Some(f64::sqrt),
        "abs" => Some(f64::abs),
        "floor" => Some(f64::floor),
        "ceil" => Some(f64::ceil),
        "fix" => Some(f64::trunc),
        "sign" => Some(|x: f64| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 }),
        "gamma" => Some(gamma),
        "factorial" => Some(|x: f64| gamma(x + 1.0).round()),
        _ => None,
    };
    if let Some(f) = unary {
        let m = m_arg(a, 0, name)?;
        return mat(m.map(f));
    }

    match name {
        // ---- constants ---------------------------------------------------------------
        "pi" => num(std::f64::consts::PI),
        "eps" => num(f64::EPSILON),
        "Inf" | "inf" => {
            let (r, c) = size_args(a)?;
            mat(Matrix::filled(r, c, f64::INFINITY))
        }
        "NaN" | "nan" => {
            let (r, c) = size_args(a)?;
            mat(Matrix::filled(r, c, f64::NAN))
        }
        "true" | "false" => {
            let (r, c) = size_args(a)?;
            mat(Matrix::logical(r, c, vec![if name == "true" { 1.0 } else { 0.0 }; r * c]))
        }
        "newline" => one(Value::chars("\n")),

        // ---- construction ------------------------------------------------------------
        "zeros" | "ones" => {
            let (r, c) = size_args(a)?;
            mat(Matrix::filled(r, c, if name == "ones" { 1.0 } else { 0.0 }))
        }
        "eye" => {
            let (r, c) = size_args(a)?;
            let mut m = Matrix::zeros(r, c);
            for i in 0..r.min(c) {
                m.set(i, i, 1.0);
            }
            mat(m)
        }
        "rand" | "randn" => {
            let (r, c) = size_args(a)?;
            let data = (0..r * c).map(|_| if name == "rand" { it.next_rand() } else { it.next_randn() }).collect();
            mat(Matrix::new(r, c, data))
        }
        "randi" => {
            let lim = m_arg(a, 0, name)?;
            let (lo, hi) = if lim.numel() >= 2 { (lim.data[0], lim.data[1]) } else { (1.0, lim.data[0]) };
            let (r, c) = size_args(&a[1..])?;
            let data = (0..r * c).map(|_| (lo + (it.next_rand() * (hi - lo + 1.0)).floor()).min(hi)).collect();
            mat(Matrix::new(r, c, data))
        }
        "randperm" => {
            let n = arg(a, 0, name)?.as_usize()?;
            let mut v: Vec<f64> = (1..=n).map(|x| x as f64).collect();
            for i in (1..n).rev() {
                let j = (it.next_rand() * (i + 1) as f64) as usize;
                v.swap(i, j.min(i));
            }
            mat(Matrix::row(v))
        }
        "rng" => {
            if let Some(v) = a.first() {
                if !v.is_text() {
                    it.rng_state = (v.scalar()? as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
                }
            }
            none()
        }
        "linspace" => {
            let lo = arg(a, 0, name)?.scalar()?;
            let hi = arg(a, 1, name)?.scalar()?;
            let n = match a.get(2) {
                Some(v) => v.scalar()?.floor().max(0.0) as usize,
                None => 100,
            };
            let data = if n == 1 {
                vec![hi]
            } else {
                (0..n).map(|i| lo + (hi - lo) * i as f64 / (n - 1) as f64).collect()
            };
            mat(Matrix::row(data))
        }
        "logspace" => {
            let lo = arg(a, 0, name)?.scalar()?;
            let hi = arg(a, 1, name)?.scalar()?;
            let n = match a.get(2) {
                Some(v) => v.as_usize()?,
                None => 50,
            };
            let data = (0..n).map(|i| 10f64.powf(lo + (hi - lo) * i as f64 / (n.max(2) - 1) as f64)).collect();
            mat(Matrix::row(data))
        }
        "colon" => {
            let lo = arg(a, 0, name)?.scalar()?;
            let (step, hi) = if a.len() >= 3 { (a[1].scalar()?, a[2].scalar()?) } else { (1.0, arg(a, 1, name)?.scalar()?) };
            let n = range_count(lo, step, hi);
            mat(Matrix::row((0..n).map(|i| lo + i as f64 * step).collect()))
        }
        "meshgrid" => {
            let x = m_arg(a, 0, name)?;
            let y = if a.len() > 1 { m_arg(a, 1, name)? } else { x.clone() };
            let (nr, nc) = (y.numel(), x.numel());
            let mut xx = Matrix::zeros(nr, nc);
            let mut yy = Matrix::zeros(nr, nc);
            for c in 0..nc {
                for r in 0..nr {
                    xx.set(r, c, x.data[c]);
                    yy.set(r, c, y.data[r]);
                }
            }
            Ok(Some(vec![Value::Num(xx), Value::Num(yy)]))
        }
        "repmat" => {
            let v = arg(a, 0, name)?;
            let (rm, cm) = size_args(&a[1..])?;
            let (r, c) = v.dims();
            let get = |i: usize| -> Value {
                match v {
                    Value::Num(m) => Value::Num(Matrix::scalar(m.data[i]).with_kind(m.kind)),
                    Value::Cell(cell) => cell.data[i].clone(),
                    other => other.clone(),
                }
            };
            let _ = get;
            match v {
                Value::Num(m) => {
                    let mut out = Matrix::zeros(r * rm, c * cm).with_kind(m.kind);
                    for bc in 0..cm {
                        for br in 0..rm {
                            for cc in 0..c {
                                for rr in 0..r {
                                    out.set(br * r + rr, bc * c + cc, m.get(rr, cc));
                                }
                            }
                        }
                    }
                    mat(out)
                }
                Value::Cell(cell) => {
                    let mut out = CellArray::empty(r * rm, c * cm);
                    let rows = r * rm;
                    for bc in 0..cm {
                        for br in 0..rm {
                            for cc in 0..c {
                                for rr in 0..r {
                                    out.data[(bc * c + cc) * rows + br * r + rr] = cell.data[cc * r + rr].clone();
                                }
                            }
                        }
                    }
                    one(Value::Cell(out))
                }
                Value::Str(s) => {
                    let s = s.repeat(rm.max(1) * cm.max(1));
                    one(Value::Str(s))
                }
                _ => rt("repmat is not supported for this type."),
            }
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
            let dims: Vec<usize> = dims.iter().map(|d| d.unwrap_or(if known == 0 { 0 } else { n / known })).collect();
            if dims.len() < 2 || dims[0] * dims[1] != n || dims[2..].iter().any(|&d| d != 1) {
                return rt("Number of elements must not change. Use [] as one of the size inputs to automatically calculate the appropriate size for that dimension.");
            }
            match v {
                Value::Num(m) => mat(Matrix::new(dims[0], dims[1], m.data.clone()).with_kind(m.kind)),
                Value::Cell(c) => one(Value::Cell(CellArray::new(dims[0], dims[1], c.data.clone()))),
                _ => rt("reshape is not supported for this type."),
            }
        }
        "cell" => {
            let (r, c) = size_args(a)?;
            one(Value::Cell(CellArray::empty(r, c)))
        }
        "struct" => {
            let mut s = Struct::default();
            for pair in a.chunks(2) {
                let k = pair[0].text()?;
                let v = pair.get(1).cloned().unwrap_or_else(Value::empty);
                // struct('a', {x}) stores x (scalar struct only).
                let v = match v {
                    Value::Cell(c) if c.data.len() == 1 => c.data[0].clone(),
                    other => other,
                };
                s.set(&k, v);
            }
            one(Value::Struct(s))
        }
        "blanks" => one(Value::chars(&" ".repeat(arg(a, 0, name)?.as_usize()?))),
        "diag" => {
            let m = m_arg(a, 0, name)?;
            if m.is_vector() {
                let n = m.numel();
                let mut out = Matrix::zeros(n, n);
                for i in 0..n {
                    out.set(i, i, m.data[i]);
                }
                mat(out)
            } else {
                let n = m.rows.min(m.cols);
                mat(Matrix::col((0..n).map(|i| m.get(i, i)).collect()))
            }
        }
        "horzcat" => one(hconcat(args)?),
        "vertcat" => one(vconcat(args)?),
        "cat" => {
            let d = arg(a, 0, name)?.scalar()?;
            let rest = args[1..].to_vec();
            one(if d == 1.0 { vconcat(rest)? } else { hconcat(rest)? })
        }

        // ---- size & type queries ------------------------------------------------------
        "size" => {
            let v = arg(a, 0, name)?;
            let (r, c) = v.dims();
            if let Some(d) = a.get(1) {
                let d = d.scalar()?;
                return num(if d == 1.0 { r as f64 } else if d == 2.0 { c as f64 } else { 1.0 });
            }
            if nargout >= 2 {
                let mut out = vec![Value::num(r as f64), Value::num(c as f64)];
                for _ in 2..nargout {
                    out.push(Value::num(1.0));
                }
                return Ok(Some(out));
            }
            mat(Matrix::row(vec![r as f64, c as f64]))
        }
        "numel" => num(arg(a, 0, name)?.numel() as f64),
        "length" => {
            let (r, c) = arg(a, 0, name)?.dims();
            num(if r == 0 || c == 0 { 0.0 } else { r.max(c) as f64 })
        }
        "ndims" => num(2.0),
        "isempty" => one(Value::bool(arg(a, 0, name)?.numel() == 0 && !matches!(a[0], Value::Str(_) | Value::Struct(_) | Value::Func(_)))),
        "isscalar" => one(Value::bool(arg(a, 0, name)?.numel() == 1)),
        "isvector" => {
            let (r, c) = arg(a, 0, name)?.dims();
            one(Value::bool((r == 1 || c == 1) && r * c >= 1))
        }
        "isrow" => one(Value::bool(arg(a, 0, name)?.dims().0 == 1)),
        "iscolumn" => one(Value::bool(arg(a, 0, name)?.dims().1 == 1)),
        "ismatrix" => one(Value::bool(true)),
        "class" => one(Value::chars(arg(a, 0, name)?.class_name())),
        "isa" => {
            let cls = arg(a, 1, name)?.text()?;
            let v = arg(a, 0, name)?;
            let hit = v.class_name() == cls || (cls == "numeric" && matches!(v, Value::Num(m) if m.kind == Kind::Double)) || (cls == "float" && v.class_name() == "double");
            one(Value::bool(hit))
        }
        "isnumeric" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Num(m) if m.kind == Kind::Double))),
        "ischar" => one(Value::bool(arg(a, 0, name)?.is_char())),
        "isstring" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Str(_)))),
        "iscell" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Cell(_)))),
        "iscellstr" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Cell(c) if c.data.iter().all(|v| v.is_char())))),
        "isstruct" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Struct(_)))),
        "islogical" => one(Value::bool(matches!(arg(a, 0, name)?, Value::Num(m) if m.kind == Kind::Logical))),
        "isreal" => one(Value::bool(true)),
        "isfield" => {
            let ok = match arg(a, 0, name)? {
                Value::Struct(s) => s.get(&arg(a, 1, name)?.text()?).is_some(),
                _ => false,
            };
            one(Value::bool(ok))
        }
        "isnan" => mat(bool_mat(&m_arg(a, 0, name)?, f64::is_nan)),
        "isinf" => mat(bool_mat(&m_arg(a, 0, name)?, f64::is_infinite)),
        "isfinite" => mat(bool_mat(&m_arg(a, 0, name)?, f64::is_finite)),
        "isprime" => mat(bool_mat(&m_arg(a, 0, name)?, is_prime)),
        "isspace" => mat(bool_mat(&m_arg(a, 0, name)?, |x| char::from_u32(x as u32).map(|c| c.is_whitespace()).unwrap_or(false))),
        "isequal" => {
            let first = arg(a, 0, name)?;
            one(Value::bool(a[1..].iter().all(|b| values_equal(first, b))))
        }
        "exist" => {
            let n = arg(a, 0, name)?.text()?;
            num(if it.get_var(&n).is_some() { 1.0 } else if exists(&n) { 5.0 } else if it.has_function(&n) { 2.0 } else { 0.0 })
        }
        "fieldnames" => match arg(a, 0, name)? {
            Value::Struct(s) => {
                let names: Vec<Value> = s.fields.iter().map(|(n, _)| Value::chars(n)).collect();
                let n = names.len();
                one(Value::Cell(CellArray::new(n, if n == 0 { 0 } else { 1 }, names)))
            }
            _ => rt("Invalid input argument of type 'double'. Input must be a structure or a Java or COM object."),
        },
        "rmfield" => match arg(a, 0, name)? {
            Value::Struct(s) => {
                let f = arg(a, 1, name)?.text()?;
                let mut s = s.clone();
                s.fields.retain(|(n, _)| *n != f);
                one(Value::Struct(s))
            }
            _ => rt("rmfield requires a struct."),
        },

        // ---- conversion ----------------------------------------------------------------
        "double" => match arg(a, 0, name)? {
            Value::Num(m) => mat(Matrix::new(m.rows, m.cols, m.data.clone())),
            Value::Str(s) => num(s.trim().parse().unwrap_or(f64::NAN)),
            _ => rt("Conversion to double is not possible."),
        },
        "logical" => {
            let m = m_arg(a, 0, name)?;
            if m.data.iter().any(|x| x.is_nan()) {
                return rt("NaN's cannot be converted to logicals.");
            }
            mat(bool_mat(&m, |x| x != 0.0))
        }
        "char" => match arg(a, 0, name)? {
            Value::Num(m) => mat(Matrix::new(m.rows, m.cols, m.data.clone()).with_kind(Kind::Char)),
            Value::Str(s) => one(Value::chars(s)),
            Value::Cell(c) => {
                let items: Vec<String> = c.data.iter().map(|v| v.text()).collect::<R<_>>()?;
                let w = items.iter().map(|s| s.chars().count()).max().unwrap_or(0);
                let mut m = Matrix::zeros(items.len(), w).with_kind(Kind::Char);
                for (r, s) in items.iter().enumerate() {
                    let chars: Vec<char> = s.chars().collect();
                    for col in 0..w {
                        m.set(r, col, *chars.get(col).unwrap_or(&' ') as u32 as f64);
                    }
                }
                mat(m)
            }
            _ => rt("Conversion to char is not possible."),
        },
        "string" => match arg(a, 0, name)? {
            Value::Num(m) if m.kind == Kind::Char => one(Value::Str(m.to_string_lossy())),
            Value::Num(m) if m.is_scalar() => one(Value::Str(if m.kind == Kind::Logical {
                (if m.data[0] != 0.0 { "true" } else { "false" }).to_string()
            } else {
                num2str_scalar(m.data[0])
            })),
            Value::Str(s) => one(Value::Str(s.clone())),
            _ => rt("Only scalars and text can be converted to string in this version."),
        },
        "num2str" => {
            let v = arg(a, 0, name)?;
            if v.is_text() {
                return one(Value::chars(&v.text()?));
            }
            let m = v.to_matrix()?;
            if let Some(f) = a.get(1) {
                if f.is_text() {
                    return one(Value::chars(&sprintf(&f.text()?, &a[..1])?));
                }
                let p = f.as_usize()?;
                let parts: Vec<String> = m.data.iter().map(|&x| fmt_g(x, p)).collect();
                return one(Value::chars(&parts.join("  ")));
            }
            if m.is_scalar() {
                return one(Value::chars(&num2str_scalar(m.data[0])));
            }
            // Rows joined by two spaces, right-aligned per column width.
            let cells: Vec<String> = m.data.iter().map(|&x| num2str_scalar(x)).collect();
            let w = cells.iter().map(|s| s.len()).max().unwrap_or(0);
            let lines: Vec<String> = (0..m.rows)
                .map(|r| (0..m.cols).map(|c| format!("{:>w$}", cells[c * m.rows + r])).collect::<Vec<_>>().join("  "))
                .collect();
            if lines.len() == 1 {
                one(Value::chars(&lines[0]))
            } else {
                let cell = Value::Cell(CellArray::new(lines.len(), 1, lines.iter().map(|l| Value::chars(l)).collect()));
                call(it, "char", vec![cell], 1)
            }
        }
        "int2str" => {
            let m = m_arg(a, 0, name)?.map(f64::round);
            call(it, "num2str", vec![Value::Num(m)], 1)
        }
        "mat2str" => {
            let v = arg(a, 0, name)?;
            if v.is_char() {
                return one(Value::chars(&format!("'{}'", v.text()?)));
            }
            let m = v.to_matrix()?;
            let f = |x: f64| -> String {
                if m.kind == Kind::Logical {
                    (if x != 0.0 { "true" } else { "false" }).into()
                } else {
                    fmt_g(x, 15)
                }
            };
            if m.is_scalar() {
                return one(Value::chars(&f(m.data[0])));
            }
            let rows: Vec<String> = (0..m.rows).map(|r| (0..m.cols).map(|c| f(m.get(r, c))).collect::<Vec<_>>().join(" ")).collect();
            one(Value::chars(&format!("[{}]", rows.join(";"))))
        }
        "str2double" => {
            let parse = |s: &str| s.trim().parse::<f64>().unwrap_or(f64::NAN);
            match arg(a, 0, name)? {
                Value::Cell(c) => mat(Matrix::new(c.rows, c.cols, c.data.iter().map(|v| v.text().map(|s| parse(&s)).unwrap_or(f64::NAN)).collect())),
                v if v.is_text() => num(parse(&v.text()?)),
                _ => num(f64::NAN),
            }
        }
        "str2num" => {
            let s = arg(a, 0, name)?.text()?;
            let prog = crate::parser::parse(&format!("__str2num__ = [{s}];"));
            let mut sub = Interpreter::new();
            match prog {
                Ok(p) if sub.run(&p.body).is_ok() => one(sub.get_var("__str2num__").cloned().unwrap_or_else(Value::empty)),
                _ => one(Value::empty()),
            }
        }
        "num2cell" => {
            let m = m_arg(a, 0, name)?;
            let data = m.data.iter().map(|&x| Value::Num(Matrix::scalar(x).with_kind(m.kind))).collect();
            one(Value::Cell(CellArray::new(m.rows, m.cols, data)))
        }
        "func2str" => match arg(a, 0, name)? {
            Value::Func(f) => one(Value::chars(&crate::display::func_text(f))),
            _ => rt("Input must be a function handle."),
        },
        "str2func" => {
            let s = arg(a, 0, name)?.text()?;
            if s.starts_with('@') {
                let prog = crate::parser::parse(&format!("__f__ = {s};")).map_err(|e| LangError::new(e.message, 0))?;
                let mut sub = Interpreter::new();
                sub.run(&prog.body)?;
                one(sub.get_var("__f__").cloned().unwrap_or_else(Value::empty))
            } else {
                one(Value::Func(FuncValue::Named(s)))
            }
        }

        // ---- strings -------------------------------------------------------------------
        "sprintf" => {
            let f = arg(a, 0, name)?.text()?;
            let s = sprintf(&f, &a[1..])?;
            one(if matches!(a[0], Value::Str(_)) { Value::Str(s) } else { Value::chars(&s) })
        }
        "fprintf" => {
            let mut rest: &[Value] = a;
            if let Some(Value::Num(m)) = rest.first() {
                if m.kind != Kind::Char && m.is_scalar() && rest.len() >= 2 {
                    rest = &rest[1..];
                }
            }
            let f = arg(rest, 0, name)?.text()?;
            let s = sprintf(&f, &rest[1..])?;
            it.emit(OutputKind::Text(s));
            none()
        }
        "disp" => {
            let v = arg(a, 0, name)?;
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
        "upper" | "lower" => {
            let v = arg(a, 0, name)?;
            let items = text_list(v)?.into_iter().map(|s| if name == "upper" { s.to_uppercase() } else { s.to_lowercase() }).collect();
            one(text_result(v, items))
        }
        "strtrim" => {
            let v = arg(a, 0, name)?;
            let items = text_list(v)?.into_iter().map(|s| s.trim().to_string()).collect();
            one(text_result(v, items))
        }
        "strrep" => {
            let v = arg(a, 0, name)?;
            let from = arg(a, 1, name)?.text()?;
            let to = arg(a, 2, name)?.text()?;
            let items = text_list(v)?.into_iter().map(|s| s.replace(&from, &to)).collect();
            one(text_result(v, items))
        }
        "strcat" => {
            if a.iter().any(|v| matches!(v, Value::Str(_))) {
                let mut s = String::new();
                for v in a {
                    s.push_str(&v.text()?);
                }
                return one(Value::Str(s));
            }
            let mut s = String::new();
            for v in a {
                // strcat removes trailing whitespace of char inputs.
                s.push_str(v.text()?.trim_end());
            }
            one(Value::chars(&s))
        }
        "strcmp" | "strcmpi" => {
            let x = arg(a, 0, name)?;
            let y = arg(a, 1, name)?;
            let eq = |p: &Value, q: &Value| -> bool {
                match (p.text(), q.text()) {
                    (Ok(s), Ok(t)) if p.is_text() && q.is_text() => {
                        if name == "strcmpi" { s.to_lowercase() == t.to_lowercase() } else { s == t }
                    }
                    _ => false,
                }
            };
            match (x, y) {
                (Value::Cell(c), other) | (other, Value::Cell(c)) if !matches!(other, Value::Cell(_)) => {
                    mat(Matrix::logical(c.rows, c.cols, c.data.iter().map(|v| if eq(v, other) { 1.0 } else { 0.0 }).collect()))
                }
                (Value::Cell(c), Value::Cell(d)) => mat(Matrix::logical(
                    c.rows,
                    c.cols,
                    c.data.iter().zip(&d.data).map(|(p, q)| if eq(p, q) { 1.0 } else { 0.0 }).collect(),
                )),
                _ => one(Value::bool(eq(x, y))),
            }
        }
        "strsplit" => {
            let s = arg(a, 0, name)?.text()?;
            let parts: Vec<Value> = match a.get(1) {
                Some(d) => {
                    let d = crate::format::process_escapes(&d.text()?);
                    s.split(d.as_str()).map(Value::chars).collect()
                }
                None => s.split_whitespace().map(Value::chars).collect(),
            };
            one(Value::Cell(CellArray::row(parts)))
        }
        "strjoin" => {
            let items = text_list(arg(a, 0, name)?)?;
            let d = match a.get(1) {
                Some(d) => crate::format::process_escapes(&d.text()?),
                None => " ".into(),
            };
            one(Value::chars(&items.join(&d)))
        }
        "contains" | "startsWith" | "endsWith" => {
            let v = arg(a, 0, name)?;
            let pats = text_list(arg(a, 1, name)?)?;
            let test = |s: &str| -> bool {
                pats.iter().any(|p| match name {
                    "contains" => s.contains(p.as_str()),
                    "startsWith" => s.starts_with(p.as_str()),
                    _ => s.ends_with(p.as_str()),
                })
            };
            match v {
                Value::Cell(c) => mat(Matrix::logical(c.rows, c.cols, c.data.iter().map(|x| if x.text().map(|s| test(&s)).unwrap_or(false) { 1.0 } else { 0.0 }).collect())),
                other => one(Value::bool(test(&other.text()?))),
            }
        }
        "strlength" => num(arg(a, 0, name)?.text()?.chars().count() as f64),

        // ---- reductions & array ops ----------------------------------------------------
        "sum" | "prod" | "mean" | "median" | "std" | "var" | "any" | "all" | "nnz" => {
            let m = m_arg(a, 0, name)?;
            if name == "nnz" {
                return num(m.data.iter().filter(|&&x| x != 0.0).count() as f64);
            }
            let dim_index = if matches!(name, "std" | "var") { 2 } else { 1 };
            let dim = dim_arg(a, dim_index, &m)?;
            let r = match name {
                "sum" => reduce(&m, dim, |v| v.iter().sum()),
                "prod" => reduce(&m, dim, |v| v.iter().product()),
                "mean" => reduce(&m, dim, |v| if v.is_empty() { f64::NAN } else { v.iter().sum::<f64>() / v.len() as f64 }),
                "median" => reduce(&m, dim, median_of),
                "std" => reduce(&m, dim, |v| variance(v).sqrt()),
                "var" => reduce(&m, dim, variance),
                "any" => reduce(&m, dim, |v| if v.iter().any(|&x| x != 0.0 && !x.is_nan()) { 1.0 } else { 0.0 }).with_kind(Kind::Logical),
                _ => reduce(&m, dim, |v| if v.iter().all(|&x| x != 0.0) { 1.0 } else { 0.0 }).with_kind(Kind::Logical),
            };
            mat(r)
        }
        "cumsum" | "cumprod" => {
            let m = m_arg(a, 0, name)?;
            let dim = dim_arg(a, 1, &m)?;
            mat(if name == "cumsum" { cumulate(&m, dim, |p, x| p + x) } else { cumulate(&m, dim, |p, x| p * x) })
        }
        "max" | "min" => {
            let is_max = name == "max";
            let better = |x: f64, best: f64| if is_max { x > best } else { x < best };
            if a.len() >= 2 && !matches!(&a[1], Value::Num(m) if m.is_empty()) {
                let x = m_arg(a, 0, name)?;
                let y = m_arg(a, 1, name)?;
                return mat(ops::broadcast(&x, &y, |p, q| {
                    if p.is_nan() { q } else if q.is_nan() { p } else if better(q, p) { q } else { p }
                })?);
            }
            let m = m_arg(a, 0, name)?;
            if m.is_empty() {
                return Ok(Some(vec![Value::empty(), Value::empty()]));
            }
            let dim = dim_arg(a, 2, &m)?;
            let pick = |v: &[f64]| -> (f64, f64) {
                let mut bi = 0;
                let mut bv = f64::NAN;
                for (i, &x) in v.iter().enumerate() {
                    if x.is_nan() {
                        continue;
                    }
                    if bv.is_nan() || better(x, bv) {
                        bv = x;
                        bi = i;
                    }
                }
                (bv, (bi + 1) as f64)
            };
            let vals = reduce(&m, dim, |v| pick(v).0);
            let idx = reduce(&m, dim, |v| pick(v).1);
            Ok(Some(vec![Value::Num(vals), Value::Num(idx)]))
        }
        "sort" => {
            let m = m_arg(a, 0, name)?;
            let desc = a.iter().skip(1).any(|v| v.text().map(|s| s.eq_ignore_ascii_case("descend")).unwrap_or(false));
            let dim = if m.rows == 1 { 2 } else { 1 };
            let mut vals = m.clone();
            let mut idx = Matrix::zeros(m.rows, m.cols);
            let sort_slice = |v: &mut Vec<(f64, usize)>| {
                v.sort_by(|p, q| {
                    let o = p.0.partial_cmp(&q.0).unwrap_or_else(|| p.0.is_nan().cmp(&q.0.is_nan()));
                    if desc { o.reverse() } else { o }
                });
            };
            if dim == 2 {
                let mut v: Vec<(f64, usize)> = m.data.iter().copied().zip(0..).collect();
                sort_slice(&mut v);
                for (k, (x, i)) in v.into_iter().enumerate() {
                    vals.data[k] = x;
                    idx.data[k] = (i + 1) as f64;
                }
            } else {
                for c in 0..m.cols {
                    let mut v: Vec<(f64, usize)> = (0..m.rows).map(|r| (m.get(r, c), r)).collect();
                    sort_slice(&mut v);
                    for (r, (x, i)) in v.into_iter().enumerate() {
                        vals.set(r, c, x);
                        idx.set(r, c, (i + 1) as f64);
                    }
                }
            }
            Ok(Some(vec![Value::Num(vals), Value::Num(idx)]))
        }
        "unique" => {
            let v = arg(a, 0, name)?;
            if let Value::Cell(c) = v {
                let mut items: Vec<String> = c.data.iter().map(|x| x.text()).collect::<R<_>>()?;
                items.sort();
                items.dedup();
                let n = items.len();
                return one(Value::Cell(CellArray::new(n, 1, items.iter().map(|s| Value::chars(s)).collect())));
            }
            let m = v.to_matrix()?;
            let mut d = m.data.clone();
            d.sort_by(|p, q| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal));
            d.dedup();
            let out = if m.rows == 1 { Matrix::row(d) } else { Matrix::col(d) };
            mat(out.with_kind(m.kind))
        }
        "ismember" => {
            let x = arg(a, 0, name)?;
            let set = arg(a, 1, name)?;
            if x.is_text() || matches!(x, Value::Cell(_)) {
                let pool = text_list(set)?;
                return match x {
                    Value::Cell(c) => mat(Matrix::logical(c.rows, c.cols, c.data.iter().map(|v| if v.text().map(|s| pool.contains(&s)).unwrap_or(false) { 1.0 } else { 0.0 }).collect())),
                    _ => one(Value::bool(pool.contains(&x.text()?))),
                };
            }
            let xm = x.to_matrix()?;
            let sm = set.to_matrix()?;
            mat(bool_mat(&xm, |v| sm.data.contains(&v)))
        }
        "find" => {
            let m = m_arg(a, 0, name)?;
            let limit = match a.get(1) {
                Some(v) => v.as_usize()?,
                None => usize::MAX,
            };
            let ix: Vec<usize> = m.data.iter().enumerate().filter(|(_, &x)| x != 0.0).map(|(i, _)| i).take(limit).collect();
            if nargout >= 2 {
                let rows: Vec<f64> = ix.iter().map(|&i| (i % m.rows.max(1) + 1) as f64).collect();
                let cols: Vec<f64> = ix.iter().map(|&i| (i / m.rows.max(1) + 1) as f64).collect();
                return Ok(Some(vec![Value::Num(Matrix::col(rows)), Value::Num(Matrix::col(cols))]));
            }
            let data: Vec<f64> = ix.iter().map(|&i| (i + 1) as f64).collect();
            mat(if m.rows == 1 { Matrix::row(data) } else { Matrix::col(data) })
        }
        "diff" => {
            let m = m_arg(a, 0, name)?;
            if m.rows == 1 || m.cols == 1 {
                let d: Vec<f64> = m.data.windows(2).map(|w| w[1] - w[0]).collect();
                return mat(if m.rows == 1 { Matrix::row(d) } else { Matrix::col(d) });
            }
            let mut out = Matrix::zeros(m.rows.saturating_sub(1), m.cols);
            for c in 0..m.cols {
                for r in 1..m.rows {
                    out.set(r - 1, c, m.get(r, c) - m.get(r - 1, c));
                }
            }
            mat(out)
        }
        "trapz" => {
            let (x, y) = if a.len() >= 2 { (Some(m_arg(a, 0, name)?), m_arg(a, 1, name)?) } else { (None, m_arg(a, 0, name)?) };
            let mut s = 0.0;
            for i in 1..y.numel() {
                let dx = x.as_ref().map(|x| x.data[i] - x.data[i - 1]).unwrap_or(1.0);
                s += dx * (y.data[i] + y.data[i - 1]) / 2.0;
            }
            num(s)
        }
        "dot" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            if x.numel() != y.numel() {
                return rt("Vectors must be the same length.");
            }
            num(x.data.iter().zip(&y.data).map(|(p, q)| p * q).sum())
        }
        "cross" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            if x.numel() != 3 || y.numel() != 3 {
                return rt("A and B must be of length 3 for cross.");
            }
            let (p, q) = (&x.data, &y.data);
            let d = vec![p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]];
            mat(if x.rows == 1 { Matrix::row(d) } else { Matrix::col(d) })
        }
        "norm" => {
            let m = m_arg(a, 0, name)?;
            let p = a.get(1);
            if m.is_vector() || m.is_empty() {
                let pv = match p {
                    Some(v) if v.is_text() => {
                        if v.text()?.eq_ignore_ascii_case("inf") { f64::INFINITY } else { 2.0 }
                    }
                    Some(v) => v.scalar()?,
                    None => 2.0,
                };
                let n = if pv.is_infinite() {
                    m.data.iter().fold(0.0f64, |acc, x| acc.max(x.abs()))
                } else if pv == 1.0 {
                    m.data.iter().map(|x| x.abs()).sum()
                } else {
                    m.data.iter().map(|x| x.abs().powf(pv)).sum::<f64>().powf(1.0 / pv)
                };
                return num(n);
            }
            if matches!(p, Some(v) if v.text().map(|s| s == "fro").unwrap_or(false)) {
                return num(m.data.iter().map(|x| x * x).sum::<f64>().sqrt());
            }
            // 2-norm: sqrt of the largest eigenvalue of A'A by power iteration.
            let ata = ops::matmul(&m.transpose(), &m)?;
            let mut v = Matrix::col(vec![1.0; ata.cols]);
            let mut lambda = 0.0;
            for _ in 0..500 {
                let w = ops::matmul(&ata, &v)?;
                let nw = w.data.iter().map(|x| x * x).sum::<f64>().sqrt();
                if nw == 0.0 {
                    break;
                }
                let next = nw;
                v = w.map(|x| x / nw);
                if (next - lambda).abs() <= 1e-14 * next {
                    lambda = next;
                    break;
                }
                lambda = next;
            }
            num(lambda.sqrt())
        }
        "det" => num(ops::det(&m_arg(a, 0, name)?)?),
        "inv" => mat(ops::inv(&m_arg(a, 0, name)?)?),
        "trace" => {
            let m = m_arg(a, 0, name)?;
            num((0..m.rows.min(m.cols)).map(|i| m.get(i, i)).sum())
        }
        "transpose" => mat(m_arg(a, 0, name)?.transpose()),
        "fliplr" | "flipud" | "flip" => {
            let v = arg(a, 0, name)?;
            let (r, c) = v.dims();
            let lr = name == "fliplr" || (name == "flip" && r == 1);
            let map = |i: usize| -> usize {
                let (rr, cc) = (i % r.max(1), i / r.max(1));
                if lr { (c - 1 - cc) * r + rr } else { cc * r + (r - 1 - rr) }
            };
            match v {
                Value::Num(m) => mat(Matrix::new(r, c, (0..m.numel()).map(|i| m.data[map(i)]).collect()).with_kind(m.kind)),
                Value::Cell(cell) => one(Value::Cell(CellArray::new(r, c, (0..cell.data.len()).map(|i| cell.data[map(i)].clone()).collect()))),
                other => one(other.clone()),
            }
        }
        "circshift" => {
            let m = m_arg(a, 0, name)?;
            let k = arg(a, 1, name)?.scalar()? as i64;
            let n = m.numel() as i64;
            if n == 0 {
                return mat(m);
            }
            let data = (0..n).map(|i| m.data[(((i - k) % n + n) % n) as usize]).collect();
            mat(Matrix::new(m.rows, m.cols, data).with_kind(m.kind))
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
        "mod" | "rem" | "atan2" | "hypot" | "power" | "idivide" | "gcd" | "lcm" | "nchoosek" => {
            let x = m_arg(a, 0, name)?;
            let y = m_arg(a, 1, name)?;
            let f: fn(f64, f64) -> f64 = match name {
                "mod" => |p, q| if q == 0.0 { p } else { p - (p / q).floor() * q },
                "rem" => |p, q| if q == 0.0 { p } else { p - (p / q).trunc() * q },
                "atan2" => f64::atan2,
                "hypot" => f64::hypot,
                "power" => f64::powf,
                "idivide" => |p, q| (p / q).trunc(),
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
            mat(ops::broadcast(&x, &y, f)?)
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
            let n = x.numel();
            mat(xi.map(|q| {
                if n < 2 || q < x.data[0] || q > x.data[n - 1] || q.is_nan() {
                    return f64::NAN;
                }
                let k = x.data.partition_point(|&v| v <= q).clamp(1, n - 1);
                let (x0, x1, y0, y1) = (x.data[k - 1], x.data[k], y.data[k - 1], y.data[k]);
                if x1 == x0 { y0 } else { y0 + (y1 - y0) * (q - x0) / (x1 - x0) }
            }))
        }
        "histcounts" => {
            let m = m_arg(a, 0, name)?;
            let edges = m_arg(a, 1, name)?;
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
            mat(Matrix::row(counts))
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
            Ok(Some(args.clone()))
        }
        "arrayfun" | "cellfun" => {
            let f = arg(a, 0, name)?.clone();
            let mut uniform = true;
            let mut inputs: Vec<Value> = Vec::new();
            let mut i = 1;
            while i < a.len() {
                if a[i].is_text() && a[i].text()?.eq_ignore_ascii_case("UniformOutput") {
                    uniform = a.get(i + 1).map(|v| v.is_true()).transpose()?.unwrap_or(true);
                    i += 2;
                    continue;
                }
                inputs.push(a[i].clone());
                i += 1;
            }
            let Some(first) = inputs.first() else { return rt("Not enough input arguments.") };
            let (r, c) = first.dims();
            let n = r * c;
            let mut results: Vec<Value> = Vec::with_capacity(n);
            for k in 0..n {
                let call_args: Vec<Value> = inputs
                    .iter()
                    .map(|v| match v {
                        Value::Cell(cell) if name == "cellfun" => cell.data[k].clone(),
                        Value::Num(m) => Value::Num(Matrix::scalar(m.data[k]).with_kind(m.kind)),
                        Value::Cell(cell) => Value::Cell(CellArray::new(1, 1, vec![cell.data[k].clone()])),
                        other => other.clone(),
                    })
                    .collect();
                // cellfun('isempty', c) style names.
                let out = it.call_func_value(&f, call_args, 1)?;
                match out.into_iter().next() {
                    Some(v) => results.push(v),
                    None if !uniform => results.push(Value::empty()),
                    None => return rt("The function must return a value when UniformOutput is true."),
                }
            }
            if uniform {
                let mut data = Vec::with_capacity(n);
                let mut kind = Kind::Double;
                for (k, v) in results.iter().enumerate() {
                    match v {
                        Value::Num(m) if m.numel() == 1 => {
                            if k == 0 {
                                kind = m.kind;
                            } else if kind != m.kind {
                                kind = Kind::Double;
                            }
                            data.push(m.data[0]);
                        }
                        _ => return rt("Non-scalar in Uniform output, at index ".to_string() + &(k + 1).to_string() + ", output 1. Set 'UniformOutput' to false."),
                    }
                }
                mat(Matrix::new(r, c, data).with_kind(kind))
            } else {
                one(Value::Cell(CellArray::new(r, c, results)))
            }
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
        "pause" | "drawnow" | "clc" => none(),
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
        "error" => {
            let mut parts: &[Value] = a;
            // error('pkg:id', 'message %d', x)
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
                if t == "off" || t == "on" {
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

        // ---- graphics ----------------------------------------------------------------------
        "figure" => {
            let id = match a.first() {
                Some(v) if !v.is_text() => Some(v.scalar()? as u32),
                _ => None,
            };
            let idx = match id.and_then(|id| it.figures.iter().position(|f| f.id == id)) {
                Some(i) => i,
                None => {
                    let id = id.unwrap_or(it.next_figure_id);
                    it.next_figure_id = it.next_figure_id.max(id + 1);
                    it.figures.push(Figure::new(id));
                    it.figures.len() - 1
                }
            };
            it.current_figure = Some(idx);
            let id = it.figures[idx].id;
            if nargout >= 1 {
                return num(id as f64);
            }
            none()
        }
        "close" => {
            let all = a.iter().any(|v| v.text().map(|s| s == "all").unwrap_or(false));
            if all {
                it.close_all_figures();
            } else if let Some(i) = it.current_figure {
                it.figures.remove(i);
                it.current_figure = if it.figures.is_empty() { None } else { Some(it.figures.len() - 1) };
            }
            none()
        }
        "clf" => {
            let f = it.current_figure();
            *f = Figure::new(f.id);
            none()
        }
        "subplot" => {
            let (m, n, p) = if a.len() == 1 {
                let v = a[0].scalar()? as usize;
                (v / 100, (v / 10) % 10, v % 10)
            } else {
                (arg(a, 0, name)?.as_usize()?, arg(a, 1, name)?.as_usize()?, arg(a, 2, name)?.as_usize()?)
            };
            if m == 0 || n == 0 || p == 0 || p > m * n {
                return rt("Index exceeds number of subplots.");
            }
            let f = it.current_figure();
            if f.grid != (m, n) {
                f.grid = (m, n);
                f.axes = vec![Axes::default(); m * n];
            }
            f.current_axes = p - 1;
            none()
        }
        "hold" => {
            let mode = a.first().map(|v| v.text()).transpose()?;
            let ax = it.current_figure().axes_mut();
            ax.hold = match mode.as_deref() {
                Some("on") | Some("all") => true,
                Some("off") => false,
                _ => !ax.hold,
            };
            none()
        }
        "grid" => {
            let mode = a.first().map(|v| v.text()).transpose()?;
            let ax = it.current_figure().axes_mut();
            ax.grid = match mode.as_deref() {
                Some("on") | Some("minor") => true,
                Some("off") => false,
                _ => !ax.grid,
            };
            none()
        }
        "title" | "xlabel" | "ylabel" => {
            let text = match a.first() {
                Some(Value::Cell(c)) => text_list(&Value::Cell(c.clone()))?.join("\n"),
                Some(v) => v.text()?,
                None => String::new(),
            };
            let ax = it.current_figure().axes_mut();
            match name {
                "title" => ax.title = text,
                "xlabel" => ax.xlabel = text,
                _ => ax.ylabel = text,
            }
            none()
        }
        "legend" => {
            let mut names: Vec<String> = Vec::new();
            let mut show = true;
            for v in a {
                match v {
                    Value::Cell(_) => names.extend(text_list(v)?),
                    other => {
                        let t = other.text()?;
                        match t.as_str() {
                            "off" => show = false,
                            "show" | "boxoff" | "boxon" => {}
                            "Location" | "location" | "Orientation" => break,
                            _ => names.push(t),
                        }
                    }
                }
            }
            let ax = it.current_figure().axes_mut();
            if !show {
                ax.legend = None;
            } else if names.is_empty() {
                ax.legend = Some(ax.series.iter().enumerate().map(|(i, s)| s.name.clone().unwrap_or_else(|| format!("data{}", i + 1))).collect());
            } else {
                ax.legend = Some(names);
            }
            none()
        }
        "axis" | "xlim" | "ylim" => {
            let Some(v) = a.first() else { return none() };
            if v.is_text() {
                let ax = it.current_figure().axes_mut();
                if v.text()? == "auto" {
                    ax.xlim = None;
                    ax.ylim = None;
                }
                return none();
            }
            let m = v.to_matrix()?;
            let ax = it.current_figure().axes_mut();
            match name {
                "xlim" if m.numel() >= 2 => ax.xlim = Some((m.data[0], m.data[1])),
                "ylim" if m.numel() >= 2 => ax.ylim = Some((m.data[0], m.data[1])),
                "axis" if m.numel() >= 4 => {
                    ax.xlim = Some((m.data[0], m.data[1]));
                    ax.ylim = Some((m.data[2], m.data[3]));
                }
                _ => {}
            }
            none()
        }
        "plot" | "scatter" | "bar" | "stairs" | "area" => {
            let kind = match name {
                "plot" => SeriesKind::Line,
                "scatter" => SeriesKind::Scatter,
                "bar" => SeriesKind::Bar,
                "stairs" => SeriesKind::Stairs,
                _ => SeriesKind::Area,
            };
            // scatter(x, y, sz) takes a numeric marker size third.
            let mut args2 = args.clone();
            let mut size = None;
            if kind == SeriesKind::Scatter && args2.len() >= 3 && !args2[2].is_text() {
                size = Some(args2.remove(2).scalar()?);
            }
            if kind == SeriesKind::Scatter && args2.len() >= 3 && !args2[2].is_text() {
                args2.remove(2);
            }
            let mut series = plot_series(&args2, kind)?;
            if args2.iter().any(|v| v.text().map(|s| s == "filled").unwrap_or(false)) {
                series.retain(|_| true);
            }
            if let Some(sz) = size {
                for s in series.iter_mut() {
                    s.marker_size = (sz.sqrt() as f32).max(2.0);
                }
            }
            it.add_series(series);
            none()
        }
        _ => Ok(None),
    }
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
