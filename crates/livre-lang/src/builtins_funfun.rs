//! Function functions: root finding and minimization (`fzero`, `fminbnd`, `fminsearch`,
//! `lsqnonneg`, `optimset`, `optimget`), quadrature (`integral`, `integral2`, `integral3`,
//! `quadgk`, `quad`, `quadl`, `quadv`, `dblquad`, `triplequad`, `quad2d`) and ODE solvers
//! (`ode45`, `ode23`, `odeset`, `odeget`, `deval`). The algorithms follow MATLAB's: Brent's
//! method with MATLAB's bracket search, Lagarias's Nelder-Mead, adaptive Simpson and Lobatto,
//! Gauss-Kronrod (7,15) with MATLAB's endpoint transformations, and the Dormand-Prince and
//! Bogacki-Shampine pairs with MATLAB's step-size control and interpolants.

#![allow(clippy::excessive_precision)] // published quadrature nodes and weights

use crate::ast::BinOp;
use crate::builtins::{arg, one, Ret};
use crate::error::{rt, LangError};
use crate::format::sprintf;
use crate::interp::{Interpreter, OutputKind};
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "fzero", "fminbnd", "fminsearch", "lsqnonneg", "optimset", "optimget", "integral", "integral2", "integral3", "quadgk", "quad", "quadl",
    "quadv", "dblquad", "triplequad", "quad2d", "ode45", "ode23", "odeset", "odeget", "deval",
];

// ---- shared helpers ----------------------------------------------------------------------

/// First output of `f(args…)`.
fn call1(it: &mut Interpreter, f: &Value, args: Vec<Value>) -> R<Value> {
    it.call_func_value(f, args, 1)?.into_iter().next().ok_or_else(|| LangError::new("Function returned no output.", 0))
}

fn fcall_scalar(it: &mut Interpreter, f: &Value, x: Value, extra: &[Value]) -> R<f64> {
    let mut args = vec![x];
    args.extend_from_slice(extra);
    let v = call1(it, f, args)?;
    let m = v.to_matrix()?;
    if m.numel() != 1 {
        return rt("FUN must return a scalar value.");
    }
    if m.im_at(0) != 0.0 {
        return Ok(f64::NAN);
    }
    Ok(m.data[0])
}

fn func_text(it: &mut Interpreter, f: &Value) -> String {
    if f.is_text() {
        return f.text().unwrap_or_default();
    }
    it.call_function("func2str", vec![f.clone()], 1)
        .ok()
        .and_then(|r| r.into_iter().next())
        .and_then(|v| v.text().ok())
        .unwrap_or_default()
}

fn is_func(v: &Value) -> bool {
    matches!(v, Value::Func(_)) || v.is_text()
}

fn col(v: Vec<f64>) -> Value {
    Value::Num(Matrix::col(v))
}

fn row(v: Vec<f64>) -> Value {
    Value::Num(Matrix::row(v))
}

/// Matrix with `rows` rows from row vectors.
fn rows_to_matrix(rows: &[Vec<f64>], ncols: usize) -> Value {
    let n = rows.len();
    let mut data = vec![0.0; n * ncols];
    for (i, r) in rows.iter().enumerate() {
        for (j, &v) in r.iter().enumerate() {
            data[j * n + i] = v;
        }
    }
    Value::Num(Matrix::new(n, ncols, data))
}

/// Matrix whose columns are the given vectors.
fn cols_to_matrix(cols: &[Vec<f64>], nrows: usize) -> Value {
    let data: Vec<f64> = cols.iter().flat_map(|c| c.iter().copied()).collect();
    Value::Num(Matrix::new(nrows, cols.len(), data))
}

fn warn(it: &mut Interpreter, msg: impl Into<String>) {
    it.emit(OutputKind::Warning(msg.into()));
}

fn fmt(f: &str, vals: &[f64]) -> String {
    sprintf(f, &vals.iter().map(|&v| Value::num(v)).collect::<Vec<_>>()).unwrap_or_default()
}

/// Name-value pairs from `a[from..]`.
fn pairs(a: &[Value], from: usize) -> R<Vec<(String, Value)>> {
    let rest = &a[from.min(a.len())..];
    if !rest.len().is_multiple_of(2) {
        return rt("Name-value arguments must come in pairs.");
    }
    rest.chunks(2).map(|p| Ok((p[0].text()?.to_lowercase(), p[1].clone()))).collect()
}

// ---- option structures -------------------------------------------------------------------

const OPTIM_FIELDS: &[&str] = &["Display", "FunValCheck", "MaxFunEvals", "MaxIter", "OutputFcn", "PlotFcns", "TolFun", "TolX"];

const ODE_FIELDS: &[&str] = &[
    "AbsTol", "BDF", "Events", "InitialStep", "Jacobian", "JConstant", "JPattern", "Mass", "MassSingular", "MaxOrder", "MaxStep", "NonNegative",
    "NormControl", "OutputFcn", "OutputSel", "Refine", "RelTol", "Stats", "Vectorized", "MStateDependence", "MvPattern", "InitialSlope",
];

/// The canonical field a (case-insensitive, possibly abbreviated) option name refers to.
fn match_field(fields: &[&'static str], name: &str, what: &str) -> R<&'static str> {
    let lower = name.to_lowercase();
    if let Some(f) = fields.iter().find(|f| f.to_lowercase() == lower) {
        return Ok(f);
    }
    let hits: Vec<&&str> = fields.iter().filter(|f| f.to_lowercase().starts_with(&lower)).collect();
    match hits.len() {
        1 => Ok(hits[0]),
        0 => rt(format!("Unrecognized {what} name '{name}'.")),
        _ => rt(format!("Ambiguous {what} name '{name}' ({}).", hits.iter().map(|s| **s).collect::<Vec<_>>().join(", "))),
    }
}

fn blank_options(fields: &[&str]) -> StructArray {
    let mut s = StructArray::scalar();
    for f in fields {
        s.set(f, Value::empty());
    }
    s
}

/// optimset/odeset: merge old structures and name-value pairs into a full options structure.
fn make_options(a: &[Value], fields: &[&'static str], what: &str) -> R<StructArray> {
    let mut out = blank_options(fields);
    let mut i = 0;
    let absorb = |out: &mut StructArray, s: &StructArray| -> R<()> {
        for (k, name) in s.fields.iter().enumerate() {
            let v = &s.elems[0][k];
            if let Ok(f) = match_field(fields, name, what) {
                if !v.is_empty_value() {
                    out.set(f, v.clone());
                }
            }
        }
        Ok(())
    };
    while i < a.len() {
        match &a[i] {
            Value::Struct(s) => {
                absorb(&mut out, s)?;
                i += 1;
            }
            v if v.is_text() => {
                let name = v.text()?;
                let Some(val) = a.get(i + 1) else { return rt(format!("Expected argument {} to be a value for '{name}'.", i + 2)) };
                let f = match_field(fields, &name, what)?;
                out.set(f, val.clone());
                i += 2;
            }
            v if v.is_empty_value() => i += 1,
            _ => return rt(format!("Expected argument {} to be a string {what} name or an options structure.", i + 1)),
        }
    }
    Ok(out)
}

trait EmptyCheck {
    fn is_empty_value(&self) -> bool;
}

impl EmptyCheck for Value {
    fn is_empty_value(&self) -> bool {
        match self {
            Value::Num(m) => m.is_empty(),
            Value::Cell(c) => c.data.is_empty(),
            Value::Struct(s) => s.elems.is_empty(),
            _ => false,
        }
    }
}

/// A non-empty option field, by case-insensitive name.
fn opt<'a>(o: Option<&'a StructArray>, name: &str) -> Option<&'a Value> {
    let s = o?;
    let k = s.fields.iter().position(|f| f.eq_ignore_ascii_case(name))?;
    let v = &s.elems.first()?[k];
    (!v.is_empty_value()).then_some(v)
}

fn opt_num(o: Option<&StructArray>, name: &str, default: f64) -> R<f64> {
    match opt(o, name) {
        Some(v) if v.is_text() => Ok(default),
        Some(v) => v.scalar(),
        None => Ok(default),
    }
}

fn opt_text(o: Option<&StructArray>, name: &str, default: &str) -> R<String> {
    match opt(o, name) {
        Some(v) => Ok(v.text()?.to_lowercase()),
        None => Ok(default.into()),
    }
}

fn options_arg(v: Option<&Value>) -> R<Option<&StructArray>> {
    match v {
        None => Ok(None),
        Some(Value::Struct(s)) => Ok(Some(s)),
        Some(v) if v.is_empty_value() => Ok(None),
        Some(_) => rt("OPTIONS must be a structure created with OPTIMSET or ODESET."),
    }
}

fn optimset(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() && nargout == 0 {
        it.emit(OutputKind::Text(
            [
                "                Display: [ off | iter | iter-detailed | notify | notify-detailed | final | final-detailed ]",
                "            FunValCheck: [ on | {off} ]",
                "            MaxFunEvals: [ positive scalar ]",
                "                MaxIter: [ positive scalar ]",
                "              OutputFcn: [ function | {[]} ]",
                "               PlotFcns: [ function | {[]} ]",
                "                 TolFun: [ positive scalar ]",
                "                   TolX: [ positive scalar ]",
                "",
            ]
            .join("\n"),
        ));
        return Ok(Some(Vec::new()));
    }
    // optimset('fzero'): the defaults of a solver.
    if a.len() == 1 && a[0].is_text() || matches!(a.first(), Some(Value::Func(_))) && a.len() == 1 {
        let name = match &a[0] {
            Value::Func(FuncValue::Named(n)) => n.clone(),
            v => v.text()?,
        };
        let mut s = blank_options(OPTIM_FIELDS);
        let notify = || Value::chars("notify");
        let off = || Value::chars("off");
        match name.as_str() {
            "fzero" => {
                s.set("Display", notify());
                s.set("FunValCheck", off());
                s.set("TolX", Value::num(f64::EPSILON));
            }
            "fminbnd" => {
                s.set("Display", notify());
                s.set("FunValCheck", off());
                s.set("MaxFunEvals", Value::num(500.0));
                s.set("MaxIter", Value::num(500.0));
                s.set("TolX", Value::num(1e-4));
            }
            "fminsearch" => {
                s.set("Display", notify());
                s.set("FunValCheck", off());
                s.set("MaxFunEvals", Value::chars("200*numberofvariables"));
                s.set("MaxIter", Value::chars("200*numberofvariables"));
                s.set("TolFun", Value::num(1e-4));
                s.set("TolX", Value::num(1e-4));
            }
            "lsqnonneg" => {
                s.set("Display", notify());
                s.set("TolX", Value::chars("10*eps*norm(c,1)*length(c)"));
            }
            other => {
                if !a[0].is_text() || !NAMES.contains(&other) {
                    return match_field(OPTIM_FIELDS, other, "parameter").and_then(|f| rt(format!("Expected argument 2 to be a value for '{f}'.")));
                }
            }
        }
        return one(Value::Struct(s));
    }
    one(Value::Struct(make_options(a, OPTIM_FIELDS, "parameter")?))
}

fn optget(a: &[Value], fields: &[&'static str], what: &str) -> Ret {
    let name = arg(a, 1, "optimget")?.text()?;
    let default = a.get(2).cloned().unwrap_or_else(Value::empty);
    let f = match_field(fields, &name, what)?;
    let o = options_arg(a.first())?;
    one(opt(o, f).cloned().unwrap_or(default))
}

// ---- fzero, fminbnd, fminsearch, lsqnonneg -----------------------------------------------

fn info_struct(pairs: Vec<(&str, Value)>) -> Value {
    let mut s = StructArray::scalar();
    for (k, v) in pairs {
        s.set(k, v);
    }
    Value::Struct(s)
}

fn fzero(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let f = arg(a, 0, "fzero")?.clone();
    let x0 = arg(a, 1, "fzero")?.to_matrix()?;
    let o = options_arg(a.get(2))?;
    let extra = a.get(3..).unwrap_or(&[]).to_vec();
    let tol = opt_num(o, "TolX", f64::EPSILON)?;
    let display = opt_text(o, "Display", "notify")?;
    let quiet = display == "off" || display == "none";
    if x0.data.iter().any(|v| !v.is_finite()) {
        return rt("Second argument must be finite.");
    }
    let mut fcount: usize;
    let mut iter = 0usize;
    let mut intervaliter = 0usize;
    let finish = |x: f64, fx: f64, flag: f64, iter: usize, intervaliter: usize, fcount: usize, msg: &str| -> Vec<Value> {
        let out = info_struct(vec![
            ("intervaliterations", Value::num(intervaliter as f64)),
            ("iterations", Value::num(iter as f64)),
            ("funcCount", Value::num(fcount as f64)),
            ("algorithm", Value::chars("bisection, interpolation")),
            ("message", Value::chars(msg)),
        ]);
        vec![Value::num(x), Value::num(fx), Value::num(flag), out].into_iter().take(nargout.max(1)).collect()
    };
    let (mut a_, mut fa, mut b, mut fb, savefa, savefb, savea, saveb);
    match x0.numel() {
        2 => {
            a_ = x0.data[0];
            b = x0.data[1];
            fa = fcall_scalar(it, &f, Value::num(a_), &extra)?;
            fb = fcall_scalar(it, &f, Value::num(b), &extra)?;
            fcount = 2;
            if !fa.is_finite() || !fb.is_finite() {
                return rt("Function values at the interval endpoints must be finite and real.");
            }
            if fa == 0.0 {
                return Ok(Some(finish(a_, fa, 1.0, 0, 0, fcount, "Zero found in the interval [a, b]")));
            }
            if fb == 0.0 {
                return Ok(Some(finish(b, fb, 1.0, 0, 0, fcount, "Zero found in the interval [a, b]")));
            }
            if (fa > 0.0) == (fb > 0.0) {
                return rt("The function values at the interval endpoints must differ in sign.");
            }
            savefa = fa;
            savefb = fb;
            savea = a_;
            saveb = b;
        }
        1 => {
            let x = x0.data[0];
            let fx = fcall_scalar(it, &f, Value::num(x), &extra)?;
            fcount = 1;
            if fx == 0.0 {
                return Ok(Some(finish(x, fx, 1.0, 0, 0, fcount, "Zero found at the initial point")));
            }
            if !fx.is_finite() {
                return rt("Function value at starting guess must be finite and real.");
            }
            let mut dx = if x != 0.0 { x / 50.0 } else { 1.0 / 50.0 };
            let twosqrt = 2f64.sqrt();
            a_ = x;
            fa = fx;
            b = x;
            fb = fx;
            // Widen an interval around x until f changes sign.
            while (fa > 0.0) == (fb > 0.0) {
                intervaliter += 1;
                dx *= twosqrt;
                a_ = x - dx;
                fa = fcall_scalar(it, &f, Value::num(a_), &extra)?;
                fcount += 1;
                if !fa.is_finite() || !a_.is_finite() {
                    let msg = "Exiting fzero: aborting search for an interval containing a sign change\n    because NaN or Inf function value encountered during search.";
                    if !quiet {
                        it.emit(OutputKind::Text(format!("{msg}\n{}\nCheck function or try again with a different starting value.\n", fmt("(Function value at %g is %g.)", &[a_, fa]))));
                    }
                    return Ok(Some(finish(f64::NAN, f64::NAN, -3.0, 0, intervaliter, fcount, msg)));
                }
                if (fa > 0.0) != (fb > 0.0) {
                    break;
                }
                b = x + dx;
                fb = fcall_scalar(it, &f, Value::num(b), &extra)?;
                fcount += 1;
                if !fb.is_finite() || !b.is_finite() {
                    let msg = "Exiting fzero: aborting search for an interval containing a sign change\n    because NaN or Inf function value encountered during search.";
                    if !quiet {
                        it.emit(OutputKind::Text(format!("{msg}\nCheck function or try again with a different starting value.\n")));
                    }
                    return Ok(Some(finish(f64::NAN, f64::NAN, -3.0, 0, intervaliter, fcount, msg)));
                }
            }
            savefa = fa;
            savefb = fb;
            savea = a_;
            saveb = b;
        }
        _ => return rt("Second argument must be a scalar or vector of length 2."),
    }
    // Brent's method.
    let mut fc = fb;
    let (mut c, mut d, mut e) = (a_, b - a_, b - a_);
    while fb != 0.0 && a_ != b {
        if (fb > 0.0) == (fc > 0.0) {
            c = a_;
            fc = fa;
            d = b - a_;
            e = d;
        }
        if fc.abs() < fb.abs() {
            a_ = b;
            b = c;
            c = a_;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let m = 0.5 * (c - b);
        let toler = 2.0 * tol * b.abs().max(1.0);
        if m.abs() <= toler || fb == 0.0 {
            break;
        }
        if e.abs() < toler || fa.abs() <= fb.abs() {
            d = m;
            e = m;
        } else {
            let s = fb / fa;
            let (mut p, mut q);
            if a_ == c {
                p = 2.0 * m * s;
                q = 1.0 - s;
            } else {
                let q0 = fa / fc;
                let r = fb / fc;
                p = s * (2.0 * m * q0 * (q0 - r) - (b - a_) * (r - 1.0));
                q = (q0 - 1.0) * (r - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            } else {
                p = -p;
            }
            if 2.0 * p < 3.0 * m * q - (toler * q).abs() && p < (0.5 * e * q).abs() {
                e = d;
                d = p / q;
            } else {
                d = m;
                e = m;
            }
        }
        a_ = b;
        fa = fb;
        if d.abs() > toler {
            b += d;
        } else if b > c {
            b -= toler;
        } else {
            b += toler;
        }
        fb = fcall_scalar(it, &f, Value::num(b), &extra)?;
        fcount += 1;
        iter += 1;
    }
    if fb.abs() <= savefa.abs().max(savefb.abs()) {
        let msg = fmt("Zero found in the interval [%g, %g]", &[savea, saveb]);
        Ok(Some(finish(b, fb, 1.0, iter, intervaliter, fcount, &msg)))
    } else {
        let msg = "Current point x may be near a singular point. The interval [a, b] may be too small.";
        if !quiet {
            it.emit(OutputKind::Text(format!("{msg}\n")));
        }
        Ok(Some(finish(b, fb, -5.0, iter, intervaliter, fcount, msg)))
    }
}

fn exceeded_message(what: &str, fval: f64) -> String {
    let (limit, opt) = if what == "evals" { ("function evaluations", "MaxFunEvals") } else { ("iterations", "MaxIter") };
    format!("Exiting: Maximum number of {limit} has been exceeded\n         - increase {opt} option.\n         Current function value: {fval:.6} \n")
}

fn fminbnd(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let f = arg(a, 0, "fminbnd")?.clone();
    let ax = arg(a, 1, "fminbnd")?.scalar()?;
    let bx = arg(a, 2, "fminbnd")?.scalar()?;
    let o = options_arg(a.get(3))?;
    let extra = a.get(4..).unwrap_or(&[]).to_vec();
    let tol = opt_num(o, "TolX", 1e-4)?;
    let maxfun = opt_num(o, "MaxFunEvals", 500.0)?;
    let maxiter = opt_num(o, "MaxIter", 500.0)?;
    let quiet = matches!(opt_text(o, "Display", "notify")?.as_str(), "off" | "none");
    let pack = |x: f64, fx: f64, flag: f64, iter: usize, num: usize, msg: &str| -> Ret {
        let out = info_struct(vec![
            ("iterations", Value::num(iter as f64)),
            ("funcCount", Value::num(num as f64)),
            ("algorithm", Value::chars("golden section search, parabolic interpolation")),
            ("message", Value::chars(msg)),
        ]);
        Ok(Some(vec![Value::num(x), Value::num(fx), Value::num(flag), out].into_iter().take(nargout.max(1)).collect()))
    };
    if ax > bx {
        let msg = "Exiting: The bound ax must be less than or equal to bx.";
        if !quiet {
            it.emit(OutputKind::Text(format!("{msg}\n")));
        }
        return pack(f64::NAN, f64::NAN, -2.0, 0, 0, msg);
    }
    let seps = f64::EPSILON.sqrt();
    let c = 0.5 * (3.0 - 5f64.sqrt());
    let (mut a_, mut b) = (ax, bx);
    let mut v = a_ + c * (b - a_);
    let mut w = v;
    let mut xf = v;
    let (mut d, mut e) = (0.0f64, 0.0f64);
    let mut fx = fcall_scalar(it, &f, Value::num(xf), &extra)?;
    let mut num = 1usize;
    let mut iter = 0usize;
    let (mut fv, mut fw) = (fx, fx);
    let mut xm = 0.5 * (a_ + b);
    let mut tol1 = seps * xf.abs() + tol / 3.0;
    let mut tol2 = 2.0 * tol1;
    let mut flag = 1.0;
    while (xf - xm).abs() > tol2 - 0.5 * (b - a_) {
        let mut gs = true;
        if e.abs() > tol1 {
            gs = false;
            let mut r = (xf - w) * (fx - fv);
            let mut q = (xf - v) * (fx - fw);
            let mut p = (xf - v) * q - (xf - w) * r;
            q = 2.0 * (q - r);
            if q > 0.0 {
                p = -p;
            }
            q = q.abs();
            r = e;
            e = d;
            if p.abs() < (0.5 * q * r).abs() && p > q * (a_ - xf) && p < q * (b - xf) {
                d = p / q;
                let x = xf + d;
                if (x - a_) < tol2 || (b - x) < tol2 {
                    let si = (xm - xf).signum_or_one();
                    d = tol1 * si;
                }
            } else {
                gs = true;
            }
        }
        if gs {
            e = if xf >= xm { a_ - xf } else { b - xf };
            d = c * e;
        }
        let si = d.signum_or_one();
        let x = xf + si * d.abs().max(tol1);
        let fu = fcall_scalar(it, &f, Value::num(x), &extra)?;
        num += 1;
        iter += 1;
        if fu <= fx {
            if x >= xf {
                a_ = xf;
            } else {
                b = xf;
            }
            v = w;
            fv = fw;
            w = xf;
            fw = fx;
            xf = x;
            fx = fu;
        } else {
            if x < xf {
                a_ = x;
            } else {
                b = x;
            }
            if fu <= fw || w == xf {
                v = w;
                fv = fw;
                w = x;
                fw = fu;
            } else if fu <= fv || v == xf || v == w {
                v = x;
                fv = fu;
            }
        }
        xm = 0.5 * (a_ + b);
        tol1 = seps * xf.abs() + tol / 3.0;
        tol2 = 2.0 * tol1;
        if num as f64 >= maxfun || iter as f64 >= maxiter {
            flag = 0.0;
            let msg = exceeded_message(if num as f64 >= maxfun { "evals" } else { "iter" }, fx);
            if !quiet {
                it.emit(OutputKind::Text(msg.clone() + "\n"));
            }
            return pack(xf, fx, flag, iter, num, &msg);
        }
    }
    let msg = fmt("Optimization terminated:\n the current x satisfies the termination criteria using OPTIONS.TolX of %e \n", &[tol]);
    pack(xf, fx, flag, iter, num, &msg)
}

trait SignOrOne {
    fn signum_or_one(self) -> f64;
}

impl SignOrOne for f64 {
    /// sign(x) + (x == 0): MATLAB's way of never stepping by zero.
    fn signum_or_one(self) -> f64 {
        if self > 0.0 {
            1.0
        } else if self < 0.0 {
            -1.0
        } else {
            1.0
        }
    }
}

fn fminsearch(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let f = arg(a, 0, "fminsearch")?.clone();
    let x0 = arg(a, 1, "fminsearch")?.to_matrix()?;
    let o = options_arg(a.get(2))?;
    let extra = a.get(3..).unwrap_or(&[]).to_vec();
    let n = x0.numel();
    let tolx = opt_num(o, "TolX", 1e-4)?;
    let tolf = opt_num(o, "TolFun", 1e-4)?;
    let maxfun = opt_num(o, "MaxFunEvals", 200.0 * n as f64)?;
    let maxiter = opt_num(o, "MaxIter", 200.0 * n as f64)?;
    let quiet = matches!(opt_text(o, "Display", "notify")?.as_str(), "off" | "none");
    let shaped = |x: &[f64]| Value::Num(Matrix::from_dims(x0.dims.clone(), x.to_vec()));
    let eval = |it: &mut Interpreter, x: &[f64]| fcall_scalar(it, &f, shaped(x), &extra);
    let (rho, chi, psi, sigma) = (1.0, 2.0, 0.5, 0.5);
    let xin = x0.data.clone();
    let mut v: Vec<Vec<f64>> = vec![xin.clone()];
    let mut fv = vec![eval(it, &xin)?];
    for j in 0..n {
        let mut y = xin.clone();
        y[j] = if y[j] != 0.0 { 1.05 * y[j] } else { 0.00025 };
        fv.push(eval(it, &y)?);
        v.push(y);
    }
    let sort = |v: &mut Vec<Vec<f64>>, fv: &mut Vec<f64>| {
        let mut idx: Vec<usize> = (0..fv.len()).collect();
        idx.sort_by(|&i, &j| fv[i].partial_cmp(&fv[j]).unwrap_or(std::cmp::Ordering::Equal));
        *v = idx.iter().map(|&i| v[i].clone()).collect();
        *fv = idx.iter().map(|&i| fv[i]).collect();
    };
    sort(&mut v, &mut fv);
    let mut itercount = 1usize;
    let mut func_evals = n + 1;
    let eps_of = |x: f64| {
        let x = x.abs();
        if x == 0.0 { f64::from_bits(1) } else { 2f64.powf(x.log2().floor() - 52.0) }
    };
    while (func_evals as f64) < maxfun && (itercount as f64) < maxiter {
        let fspread = (1..=n).map(|j| (fv[0] - fv[j]).abs()).fold(0.0, f64::max);
        let xspread = (1..=n).flat_map(|j| (0..n).map(move |i| (j, i))).map(|(j, i)| (v[j][i] - v[0][i]).abs()).fold(0.0, f64::max);
        let vmax = v[0].iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if fspread <= tolf.max(10.0 * eps_of(fv[0])) && xspread <= tolx.max(10.0 * eps_of(vmax)) {
            break;
        }
        let xbar: Vec<f64> = (0..n).map(|i| (0..n).map(|j| v[j][i]).sum::<f64>() / n as f64).collect();
        let worst = v[n].clone();
        let comb = |s: f64| -> Vec<f64> { (0..n).map(|i| (1.0 + s) * xbar[i] - s * worst[i]).collect() };
        let xr = comb(rho);
        let fxr = eval(it, &xr)?;
        func_evals += 1;
        if fxr < fv[0] {
            let xe = comb(rho * chi);
            let fxe = eval(it, &xe)?;
            func_evals += 1;
            if fxe < fxr {
                v[n] = xe;
                fv[n] = fxe;
            } else {
                v[n] = xr;
                fv[n] = fxr;
            }
        } else if fxr < fv[n - 1] {
            v[n] = xr;
            fv[n] = fxr;
        } else {
            let mut shrink = false;
            if fxr < fv[n] {
                let xc = comb(psi * rho);
                let fxc = eval(it, &xc)?;
                func_evals += 1;
                if fxc <= fxr {
                    v[n] = xc;
                    fv[n] = fxc;
                } else {
                    shrink = true;
                }
            } else {
                let xcc: Vec<f64> = (0..n).map(|i| (1.0 - psi) * xbar[i] + psi * worst[i]).collect();
                let fxcc = eval(it, &xcc)?;
                func_evals += 1;
                if fxcc < fv[n] {
                    v[n] = xcc;
                    fv[n] = fxcc;
                } else {
                    shrink = true;
                }
            }
            if shrink {
                for j in 1..=n {
                    let p: Vec<f64> = (0..n).map(|i| v[0][i] + sigma * (v[j][i] - v[0][i])).collect();
                    fv[j] = eval(it, &p)?;
                    v[j] = p;
                }
                func_evals += n;
            }
        }
        sort(&mut v, &mut fv);
        itercount += 1;
    }
    let (flag, msg) = if func_evals as f64 >= maxfun {
        (0.0, exceeded_message("evals", fv[0]))
    } else if itercount as f64 >= maxiter {
        (0.0, exceeded_message("iter", fv[0]))
    } else {
        (
            1.0,
            fmt(
                "Optimization terminated:\n the current x satisfies the termination criteria using OPTIONS.TolX of %e \n and F(X) satisfies the convergence criteria using OPTIONS.TolFun of %e \n",
                &[tolx, tolf],
            ),
        )
    };
    if flag == 0.0 && !quiet {
        it.emit(OutputKind::Text(msg.clone() + "\n"));
    }
    let out = info_struct(vec![
        ("iterations", Value::num(itercount as f64)),
        ("funcCount", Value::num(func_evals as f64)),
        ("algorithm", Value::chars("Nelder-Mead simplex direct search")),
        ("message", Value::chars(&msg)),
    ]);
    Ok(Some(vec![shaped(&v[0]), Value::num(fv[0]), Value::num(flag), out].into_iter().take(nargout.max(1)).collect()))
}

fn lsqnonneg(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let c = arg(a, 0, "lsqnonneg")?.to_matrix()?;
    let d = arg(a, 1, "lsqnonneg")?.to_matrix()?;
    if c.is_complex() || d.is_complex() {
        return rt("C and d must be real.");
    }
    let o = options_arg(a.get(2))?;
    let (m, n) = (c.rows(), c.cols());
    if d.numel() != m {
        return rt("The number of rows in C must equal the length of d.");
    }
    let norm1 = (0..n).map(|j| (0..m).map(|i| c.get(i, j).abs()).sum::<f64>()).fold(0.0, f64::max);
    let tol = opt_num(o, "TolX", 10.0 * f64::EPSILON * norm1 * m.max(n) as f64)?;
    let mul = |x: &[f64]| -> Vec<f64> { (0..m).map(|i| (0..n).map(|j| c.get(i, j) * x[j]).sum()).collect() };
    let ct_mul = |r: &[f64]| -> Vec<f64> { (0..n).map(|j| (0..m).map(|i| c.get(i, j) * r[i]).sum()).collect() };
    // Least-squares solution on the passive set P.
    let solve_p = |it: &mut Interpreter, p: &[bool]| -> R<Vec<f64>> {
        let cols: Vec<usize> = (0..n).filter(|&j| p[j]).collect();
        let sub = Matrix::new(m, cols.len(), cols.iter().flat_map(|&j| (0..m).map(move |i| (i, j))).map(|(i, j)| c.get(i, j)).collect());
        let zp = it.binary(BinOp::MatLeftDiv, Value::Num(sub), Value::Num(Matrix::col(d.data.clone())))?.to_matrix()?;
        let mut z = vec![0.0; n];
        for (k, &j) in cols.iter().enumerate() {
            z[j] = zp.data[k];
        }
        Ok(z)
    };
    let mut p = vec![false; n];
    let mut zset = vec![true; n];
    let mut x = vec![0.0; n];
    let mut resid: Vec<f64> = d.data.clone();
    let mut w = ct_mul(&resid);
    let (mut outer, mut iter, itmax) = (0usize, 0usize, 3 * n);
    let mut flag = 1.0;
    'outer: while zset.iter().any(|&z| z) && (0..n).any(|j| zset[j] && w[j] > tol) {
        outer += 1;
        let t = (0..n).filter(|&j| !p[j]).fold(None, |best: Option<usize>, j| match best {
            Some(b) if w[b] >= w[j] => Some(b),
            _ => Some(j),
        });
        let Some(t) = t else { break };
        p[t] = true;
        zset[t] = false;
        let mut z = solve_p(it, &p)?;
        while (0..n).any(|j| p[j] && z[j] <= 0.0) {
            iter += 1;
            if iter > itmax {
                flag = 0.0;
                x = z;
                let msg = "Exiting: Iteration count is exceeded, exiting LSQNONNEG.\n                 Try raising the tolerance (OPTIONS.TolX).";
                if !matches!(opt_text(o, "Display", "notify")?.as_str(), "off" | "none") {
                    it.emit(OutputKind::Text(format!("{msg}\n")));
                }
                resid = d.data.iter().zip(mul(&x)).map(|(a, b)| a - b).collect();
                w = ct_mul(&resid);
                break 'outer;
            }
            let alpha = (0..n).filter(|&j| p[j] && z[j] <= 0.0).map(|j| x[j] / (x[j] - z[j])).fold(f64::INFINITY, f64::min);
            for j in 0..n {
                x[j] += alpha * (z[j] - x[j]);
            }
            for j in 0..n {
                zset[j] = (x[j].abs() < tol && p[j]) || zset[j];
                p[j] = !zset[j];
            }
            z = solve_p(it, &p)?;
        }
        x = z;
        resid = d.data.iter().zip(mul(&x)).map(|(a, b)| a - b).collect();
        w = ct_mul(&resid);
    }
    let resnorm: f64 = resid.iter().map(|r| r * r).sum();
    let out = info_struct(vec![
        ("iterations", Value::num(outer as f64)),
        ("algorithm", Value::chars("active-set")),
        ("message", Value::chars(if flag == 1.0 { "Optimization terminated." } else { "Iteration count is exceeded." })),
    ]);
    Ok(Some(vec![col(x), Value::num(resnorm), col(resid), Value::num(flag), out, col(w)].into_iter().take(nargout.max(1)).collect()))
}

// ---- Gauss-Kronrod quadrature (integral, quadgk, integral2/3, quad2d) ---------------------

const XGK: [f64; 8] = [
    0.991455371120812639206854697526329, 0.949107912342758524526189684047851, 0.864864423359769072789712788640926,
    0.741531185599394439863864773280788, 0.586087235467691130294144845693013, 0.405845151377397166906606412076961,
    0.207784955007898467600689403773245, 0.0,
];
const WGK: [f64; 8] = [
    0.022935322010529224963732008058970, 0.063092092629978553290700663189204, 0.104790010322250183839876322541518,
    0.140653259715525918745189590510238, 0.169004726639267902826583426598550, 0.190350578064785409913256402421014,
    0.204432940075298892414161999234649, 0.209482141084727828012999174891714,
];
/// Gauss weights at XGK[1], XGK[3], XGK[5], XGK[7].
const WG: [f64; 4] = [0.129484966168869693270611432679082, 0.279705391489276667901467771423780, 0.381830050505118944950369775488975, 0.417959183673469387755102040816327];

/// Integrand values: real and imaginary parts interleaved, one pair per output element.
type Vals = Vec<f64>;

/// An integrand evaluated at several points at once.
type Integrand<'a> = dyn FnMut(&mut Interpreter, &[f64]) -> R<Vec<Vals>> + 'a;

#[derive(Clone, Copy)]
enum Map {
    Finite(f64, f64),
    Upper(f64),
    Lower(f64),
    Whole,
}

impl Map {
    /// (x, dx/dt) at t.
    fn at(self, t: f64) -> (f64, f64) {
        match self {
            Map::Finite(a, b) => ((b - a) / 4.0 * t * (3.0 - t * t) + (b + a) / 2.0, 3.0 * (b - a) / 4.0 * (1.0 - t * t)),
            // The square weakens a singularity at the finite end.
            Map::Upper(a) => {
                let tt = t / (1.0 - t);
                (a + tt * tt, 2.0 * tt / ((1.0 - t) * (1.0 - t)))
            }
            Map::Lower(b) => {
                let tt = t / (1.0 + t);
                (b - tt * tt, -2.0 * tt / ((1.0 + t) * (1.0 + t)))
            }
            Map::Whole => {
                let u = 1.0 - t * t;
                (t / u, (1.0 + t * t) / (u * u))
            }
        }
    }

    fn range(self) -> (f64, f64) {
        match self {
            Map::Finite(..) | Map::Whole => (-1.0, 1.0),
            Map::Upper(_) => (0.0, 1.0),
            Map::Lower(_) => (-1.0, 0.0),
        }
    }

    /// t at x (for waypoints).
    fn inverse(self, x: f64) -> f64 {
        match self {
            Map::Finite(..) => {
                let (mut lo, mut hi) = (-1.0, 1.0);
                for _ in 0..200 {
                    let mid = 0.5 * (lo + hi);
                    if self.at(mid).0 < x {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                0.5 * (lo + hi)
            }
            Map::Upper(a) => {
                let tt = (x - a).sqrt();
                tt / (1.0 + tt)
            }
            Map::Lower(b) => {
                let tt = -(b - x).sqrt();
                tt / (1.0 - tt)
            }
            Map::Whole => {
                if x == 0.0 {
                    0.0
                } else {
                    (-1.0 + (1.0 + 4.0 * x * x).sqrt()) / (2.0 * x)
                }
            }
        }
    }
}

struct GkOpts {
    abs_tol: f64,
    rel_tol: f64,
    max_intervals: usize,
}

fn vnorm(v: &[f64]) -> f64 {
    v.chunks(2).map(|c| c[0].hypot(c[1])).fold(0.0, |m, x| if x.is_nan() || m.is_nan() { f64::NAN } else { m.max(x) })
}

fn vadd(acc: &mut Vals, v: &[f64], s: f64) {
    if acc.len() < v.len() {
        acc.resize(v.len(), 0.0);
    }
    for (a, b) in acc.iter_mut().zip(v) {
        *a += s * b;
    }
}

/// ∫ f from a to b. Returns the integral and an error bound.
fn gk_integrate(it: &mut Interpreter, f: &mut Integrand, a: f64, b: f64, waypoints: &[f64], o: &GkOpts) -> R<(Vals, f64)> {
    if a == b {
        let v = f(it, &[a])?;
        return Ok((vec![0.0; v.first().map(|v| v.len()).unwrap_or(2)], 0.0));
    }
    if a.is_nan() || b.is_nan() {
        let v = f(it, &[0.0])?;
        return Ok((vec![f64::NAN; v.first().map(|v| v.len()).unwrap_or(2)], f64::NAN));
    }
    if a > b {
        let (q, e) = gk_integrate(it, f, b, a, waypoints, o)?;
        return Ok((q.iter().map(|x| -x).collect(), e));
    }
    let map = match (a.is_finite(), b.is_finite()) {
        (true, true) => Map::Finite(a, b),
        (true, false) => Map::Upper(a),
        (false, true) => Map::Lower(b),
        (false, false) => Map::Whole,
    };
    let (t0, t1) = map.range();
    let mut breaks: Vec<f64> = vec![t0];
    let mut wp: Vec<f64> = waypoints.iter().copied().filter(|&w| w > a && w < b).collect();
    wp.sort_by(|x, y| x.partial_cmp(y).unwrap());
    breaks.extend(wp.iter().map(|&w| map.inverse(w)));
    breaks.push(t1);
    let pieces = breaks.len() - 1;
    let per = 10usize.div_ceil(pieces).max(1);
    let mut subs: Vec<(f64, f64)> = Vec::new();
    for k in 0..pieces {
        let (l, r) = (breaks[k], breaks[k + 1]);
        for j in 0..per {
            subs.push((l + (r - l) * j as f64 / per as f64, l + (r - l) * (j + 1) as f64 / per as f64));
        }
    }
    let pathlen = t1 - t0;
    let mut q_ok: Vals = Vec::new();
    let mut err_ok: Vals = Vec::new();
    let mut q_total: Vals;
    let mut errbnd;
    loop {
        // One call evaluates every node of every subinterval.
        let mut xs = Vec::with_capacity(subs.len() * 15);
        let mut ws = Vec::with_capacity(subs.len() * 15);
        for &(l, r) in &subs {
            let (c, h) = ((l + r) / 2.0, (r - l) / 2.0);
            for k in 0..15 {
                let node = if k < 7 { -XGK[k] } else if k == 7 { 0.0 } else { XGK[14 - k] };
                let (x, w) = map.at(c + h * node);
                xs.push(x);
                ws.push(w * h);
            }
        }
        let fx = f(it, &xs)?;
        let mut qs: Vec<Vals> = Vec::with_capacity(subs.len());
        let mut es: Vec<Vals> = Vec::with_capacity(subs.len());
        for s in 0..subs.len() {
            let mut qk: Vals = Vec::new();
            let mut qg: Vals = Vec::new();
            for k in 0..15 {
                let j = if k < 7 { k } else if k == 7 { 7 } else { 14 - k };
                let i = s * 15 + k;
                vadd(&mut qk, &fx[i], WGK[j] * ws[i]);
                if j % 2 == 1 {
                    vadd(&mut qg, &fx[i], WG[j / 2] * ws[i]);
                }
            }
            es.push(qk.iter().zip(&qg).map(|(x, y)| x - y).collect());
            qs.push(qk);
        }
        q_total = q_ok.clone();
        for q in &qs {
            vadd(&mut q_total, q, 1.0);
        }
        if q_total.iter().any(|v| !v.is_finite()) {
            warn(it, "Inf or NaN value encountered.");
            errbnd = f64::NAN;
            break;
        }
        let tol = o.abs_tol.max(o.rel_tol * vnorm(&q_total));
        let mut next = Vec::new();
        let mut err_not = 0.0;
        for (s, &(l, r)) in subs.iter().enumerate() {
            if vnorm(&es[s]) <= tol * (r - l) / pathlen {
                vadd(&mut q_ok, &qs[s], 1.0);
                vadd(&mut err_ok, &es[s], 1.0);
            } else {
                err_not += vnorm(&es[s]);
                next.push((l, r));
            }
        }
        errbnd = vnorm(&err_ok) + err_not;
        if next.is_empty() || errbnd <= tol {
            break;
        }
        let mut split = Vec::with_capacity(next.len() * 2);
        let mut tiny = None;
        for (l, r) in next {
            let m = (l + r) / 2.0;
            if m <= l || m >= r {
                tiny = Some(map.at(m).0);
            }
            split.push((l, m));
            split.push((m, r));
        }
        if let Some(x) = tiny {
            warn(it, fmt("Minimum step size reached near x = %g. There may be a singularity, or the tolerances may be too tight for this problem.", &[x]));
            break;
        }
        if split.len() > o.max_intervals {
            warn(it, fmt("Reached the limit on the maximum number of intervals in use. Approximate bound on error is %9.1e. The integral may not exist, or it may be difficult to approximate numerically to the requested accuracy.", &[errbnd]));
            break;
        }
        subs = split;
    }
    Ok((q_total, errbnd))
}

/// Interleaved re/im values of a numeric result.
fn vals_of(m: &Matrix) -> Vals {
    let mut v = Vec::with_capacity(2 * m.numel());
    for i in 0..m.numel() {
        v.push(m.data[i]);
        v.push(m.im_at(i));
    }
    v
}

fn vals_value(v: &[f64], dims: &[usize]) -> Value {
    let re: Vec<f64> = v.iter().step_by(2).copied().collect();
    let im: Vec<f64> = v.iter().skip(1).step_by(2).copied().collect();
    if im.iter().all(|&x| x == 0.0) {
        Value::Num(Matrix::from_dims(dims.to_vec(), re))
    } else {
        Value::Num(Matrix::complex(dims.to_vec(), re, im))
    }
}

const SIZE_MISMATCH: &str =
    "Output of the function must be the same size as the input. If FUN is an array-valued integrand, set the 'ArrayValued' option to true.";

/// A vectorized scalar integrand: f called once with every point as a row vector.
fn vector_integrand<'a>(f: &'a Value, extra: &'a [Value]) -> impl FnMut(&mut Interpreter, &[f64]) -> R<Vec<Vals>> + 'a {
    move |it, xs| {
        let mut args = vec![row(xs.to_vec())];
        args.extend_from_slice(extra);
        let m = call1(it, f, args)?.to_matrix()?;
        if m.numel() != xs.len() {
            return rt(SIZE_MISMATCH);
        }
        Ok((0..xs.len()).map(|i| vec![m.data[i], m.im_at(i)]).collect())
    }
}

fn gk_options(a: &[Value], from: usize, abs_tol: f64, rel_tol: f64) -> R<(GkOpts, bool, Vec<f64>)> {
    let mut o = GkOpts { abs_tol, rel_tol, max_intervals: 650 };
    let mut array_valued = false;
    let mut waypoints = Vec::new();
    for (k, v) in pairs(a, from)? {
        match k.as_str() {
            "abstol" => o.abs_tol = v.scalar()?,
            "reltol" => o.rel_tol = v.scalar()?,
            "arrayvalued" => array_valued = v.is_true()?,
            "waypoints" => waypoints = v.to_matrix()?.data,
            "maxintervalcount" => o.max_intervals = v.scalar()? as usize,
            other => return rt(format!("Unrecognized option '{other}'.")),
        }
    }
    Ok((o, array_valued, waypoints))
}

fn integral(it: &mut Interpreter, a: &[Value], quadgk: bool) -> Ret {
    let name = if quadgk { "quadgk" } else { "integral" };
    let f = arg(a, 0, name)?.clone();
    let lo = arg(a, 1, name)?.scalar()?;
    let hi = arg(a, 2, name)?.scalar()?;
    let (o, array_valued, waypoints) = gk_options(a, 3, 1e-10, 1e-6)?;
    if array_valued {
        let mut dims: Vec<usize> = vec![1, 1];
        let mut g = |it: &mut Interpreter, xs: &[f64]| -> R<Vec<Vals>> {
            xs.iter()
                .map(|&x| {
                    let m = call1(it, &f, vec![Value::num(x)])?.to_matrix()?;
                    dims = m.dims.clone();
                    Ok(vals_of(&m))
                })
                .collect()
        };
        let (q, e) = gk_integrate(it, &mut g, lo, hi, &waypoints, &o)?;
        return Ok(Some(vec![vals_value(&q, &dims), Value::num(e)]));
    }
    let mut g = vector_integrand(&f, &[]);
    let (q, e) = gk_integrate(it, &mut g, lo, hi, &waypoints, &o)?;
    Ok(Some(vec![vals_value(&q, &[1, 1]), Value::num(e)]))
}

/// A limit of integration: a number or a function of the outer variables.
fn limit(it: &mut Interpreter, v: &Value, outer: &[f64]) -> R<f64> {
    if is_func(v) && !matches!(v, Value::Num(_)) {
        return call1(it, v, outer.iter().map(|&x| Value::num(x)).collect())?.scalar();
    }
    v.scalar()
}

/// integral2/integral3/quad2d by iterated Gauss-Kronrod: the innermost integrand is called with
/// the outer variables expanded to the size of the inner points.
fn iterated(it: &mut Interpreter, f: &Value, limits: &[(Value, Value)], outer: &[f64], o: &GkOpts) -> R<(f64, f64)> {
    let depth = outer.len();
    let lo = limit(it, &limits[depth].0, outer)?;
    let hi = limit(it, &limits[depth].1, outer)?;
    let last = depth + 1 == limits.len();
    let inner_o = GkOpts { abs_tol: o.abs_tol / 4.0, rel_tol: o.rel_tol / 4.0, max_intervals: o.max_intervals };
    let mut g = |it: &mut Interpreter, xs: &[f64]| -> R<Vec<Vals>> {
        if last {
            let mut args: Vec<Value> = outer.iter().map(|&v| row(vec![v; xs.len()])).collect();
            args.push(row(xs.to_vec()));
            let m = call1(it, f, args)?.to_matrix()?;
            if m.numel() != xs.len() {
                return rt("Integrand output size does not match the input size.");
            }
            return Ok((0..xs.len()).map(|i| vec![m.data[i], m.im_at(i)]).collect());
        }
        let mut out = Vec::with_capacity(xs.len());
        for &x in xs {
            let mut o2 = outer.to_vec();
            o2.push(x);
            let (q, _) = iterated(it, f, limits, &o2, &inner_o)?;
            out.push(vec![q, 0.0]);
        }
        Ok(out)
    };
    let (q, e) = gk_integrate(it, &mut g, lo, hi, &[], o)?;
    Ok((q[0], e))
}

fn integral_nd(it: &mut Interpreter, a: &[Value], dims: usize, name: &str, abs_tol: f64, rel_tol: f64) -> Ret {
    let f = arg(a, 0, name)?.clone();
    let mut limits = Vec::new();
    for d in 0..dims {
        limits.push((arg(a, 1 + 2 * d, name)?.clone(), arg(a, 2 + 2 * d, name)?.clone()));
    }
    let mut o = GkOpts { abs_tol, rel_tol, max_intervals: 650 };
    for (k, v) in pairs(a, 1 + 2 * dims)? {
        match k.as_str() {
            "abstol" => o.abs_tol = v.scalar()?,
            "reltol" => o.rel_tol = v.scalar()?,
            "method" | "maxfunevals" | "singular" | "fail" => {}
            other => return rt(format!("Unrecognized option '{other}'.")),
        }
    }
    let (q, e) = iterated(it, &f, &limits, &[], &o)?;
    Ok(Some(vec![Value::num(q), Value::num(e)]))
}

// ---- adaptive Simpson and Lobatto (quad, quadv, quadl, dblquad, triplequad) ---------------

struct Quad<'a, 'b> {
    f: &'a mut Integrand<'b>,
    tol: f64,
    hmin: f64,
    fcnt: usize,
}

fn quadstep(it: &mut Interpreter, q: &mut Quad, a: f64, b: f64, fa: &[f64], fc: &[f64], fb: &[f64]) -> R<(Vals, u8)> {
    let h = b - a;
    let c = (a + b) / 2.0;
    let y = (q.f)(it, &[(a + c) / 2.0, (c + b) / 2.0])?;
    q.fcnt += 2;
    let (fd, fe) = (&y[0], &y[1]);
    let n = fa.len();
    let q1: Vals = (0..n).map(|i| h / 6.0 * (fa[i] + 4.0 * fc[i] + fb[i])).collect();
    let q2: Vals = (0..n).map(|i| h / 12.0 * (fa[i] + 4.0 * fd[i] + 2.0 * fc[i] + 4.0 * fe[i] + fb[i])).collect();
    let qq: Vals = (0..n).map(|i| q2[i] + (q2[i] - q1[i]) / 15.0).collect();
    if qq.iter().any(|v| !v.is_finite()) {
        return Ok((qq, 3));
    }
    if q.fcnt > 10000 {
        return Ok((q2, 2));
    }
    if h.abs() < q.hmin || c == a || c == b {
        return Ok((qq, 1));
    }
    let d: Vals = (0..n).map(|i| q2[i] - q1[i]).collect();
    if vnorm(&d) <= q.tol {
        return Ok((qq, 0));
    }
    let (qac, wa) = quadstep(it, q, a, c, fa, fd, fc)?;
    let (qcb, wb) = quadstep(it, q, c, b, fc, fe, fb)?;
    Ok((qac.iter().zip(&qcb).map(|(x, y)| x + y).collect(), wa.max(wb)))
}

fn eps_of(x: f64) -> f64 {
    let x = x.abs();
    if x == 0.0 { f64::from_bits(1) } else { 2f64.powf(x.log2().floor() - 52.0) }
}

fn quad_warning(it: &mut Interpreter, w: u8) {
    match w {
        1 => warn(it, "Minimum step size reached; singularity possible."),
        2 => warn(it, "Maximum function count exceeded; singularity likely."),
        3 => warn(it, "Infinite or Not-a-Number function value encountered."),
        _ => {}
    }
}

/// MATLAB's quad: adaptive Simpson started on three unequal subintervals.
fn simpson(it: &mut Interpreter, f: &mut Integrand, a: f64, b: f64, tol: f64) -> R<(Vals, usize)> {
    let h = 0.13579 * (b - a);
    let x = [a, a + h, a + 2.0 * h, (a + b) / 2.0, b - 2.0 * h, b - h, b];
    let mut y = f(it, &x)?;
    let mut fcnt = 7;
    if y[0].iter().any(|v| !v.is_finite()) {
        y[0] = f(it, &[a + eps_of(a.abs().max(b.abs())) * (b - a)])?.remove(0);
        fcnt += 1;
    }
    if y[6].iter().any(|v| !v.is_finite()) {
        y[6] = f(it, &[b - eps_of(a.abs().max(b.abs())) * (b - a)])?.remove(0);
        fcnt += 1;
    }
    let mut q = Quad { f, tol, hmin: eps_of(b - a) / 1024.0, fcnt };
    let mut total: Vals = Vec::new();
    let mut worst = 0;
    for k in 0..3 {
        let (qk, w) = quadstep(it, &mut q, x[2 * k], x[2 * k + 2], &y[2 * k], &y[2 * k + 1], &y[2 * k + 2])?;
        vadd(&mut total, &qk, 1.0);
        worst = worst.max(w);
    }
    let fcnt = q.fcnt;
    quad_warning(it, worst);
    Ok((total, fcnt))
}

fn lobatto_step(it: &mut Interpreter, q: &mut Quad, a: f64, b: f64, fa: &[f64], fb: &[f64]) -> R<(Vals, u8)> {
    let h = (b - a) / 2.0;
    let n = fa.len();
    if h.abs() < q.hmin || b == a + h {
        return Ok(((0..n).map(|i| h * (fa[i] + fb[i])).collect(), 1));
    }
    let m = (a + b) / 2.0;
    let (alpha, beta) = ((2.0f64 / 3.0).sqrt(), 1.0 / 5f64.sqrt());
    let mid = [m - alpha * h, m - beta * h, m, m + beta * h, m + alpha * h];
    let ym = (q.f)(it, &mid)?;
    q.fcnt += 5;
    if q.fcnt > 10000 {
        return Ok(((0..n).map(|i| h * (fa[i] + fb[i])).collect(), 2));
    }
    let x = [a, mid[0], mid[1], mid[2], mid[3], mid[4], b];
    let y: Vec<&[f64]> = std::iter::once(fa).chain(ym.iter().map(|v| v.as_slice())).chain(std::iter::once(fb)).collect();
    let q1: Vals = (0..n).map(|i| h / 6.0 * (y[0][i] + 5.0 * y[2][i] + 5.0 * y[4][i] + y[6][i])).collect();
    let kw = [77.0, 432.0, 625.0, 672.0, 625.0, 432.0, 77.0];
    let qq: Vals = (0..n).map(|i| h / 1470.0 * (0..7).map(|k| kw[k] * y[k][i]).sum::<f64>()).collect();
    if qq.iter().any(|v| !v.is_finite()) {
        return Ok((qq, 3));
    }
    let d: Vals = (0..n).map(|i| qq[i] - q1[i]).collect();
    if vnorm(&d) <= q.tol {
        return Ok((qq, 0));
    }
    let ys: Vec<Vals> = y.iter().map(|v| v.to_vec()).collect();
    let mut total: Vals = vec![0.0; n];
    let mut worst = 0;
    for k in 0..6 {
        let (qk, w) = lobatto_step(it, q, x[k], x[k + 1], &ys[k], &ys[k + 1])?;
        vadd(&mut total, &qk, 1.0);
        worst = worst.max(w);
    }
    Ok((total, worst))
}

/// MATLAB's quadl: adaptive Gauss-Lobatto with a Kronrod extension.
fn lobatto(it: &mut Interpreter, f: &mut Integrand, a: f64, b: f64, tol: f64) -> R<(Vals, usize)> {
    let m = (a + b) / 2.0;
    let h = (b - a) / 2.0;
    let (alpha, beta) = ((2.0f64 / 3.0).sqrt(), 1.0 / 5f64.sqrt());
    let (x1, x2, x3) = (0.942882415695480, 0.641853342345781, 0.236383199662150);
    let x = [a, m - x1 * h, m - alpha * h, m - x2 * h, m - beta * h, m - x3 * h, m, m + x3 * h, m + beta * h, m + x2 * h, m + alpha * h, m + x1 * h, b];
    let mut y = f(it, &x)?;
    let mut fcnt = 13;
    if y[0].iter().any(|v| !v.is_finite()) {
        y[0] = f(it, &[a + f64::EPSILON * (b - a)])?.remove(0);
        fcnt += 1;
    }
    if y[12].iter().any(|v| !v.is_finite()) {
        y[12] = f(it, &[b - f64::EPSILON * (b - a)])?.remove(0);
        fcnt += 1;
    }
    let n = y[0].len();
    let q1: Vals = (0..n).map(|i| h / 6.0 * (y[0][i] + 5.0 * y[4][i] + 5.0 * y[8][i] + y[12][i])).collect();
    let kw = [77.0, 432.0, 625.0, 672.0, 625.0, 432.0, 77.0];
    let q2: Vals = (0..n).map(|i| h / 1470.0 * (0..7).map(|k| kw[k] * y[2 * k][i]).sum::<f64>()).collect();
    let s0 = [0.0158271919734802, 0.094273840218850, 0.155071987336585, 0.188821573960182, 0.199773405226859, 0.224926465333340, 0.242611071901408];
    let s: Vec<f64> = (0..13).map(|k| s0[if k < 7 { k } else { 12 - k }]).collect();
    let q0: Vals = (0..n).map(|i| h * (0..13).map(|k| s[k] * y[k][i]).sum::<f64>()).collect();
    let d20: Vals = (0..n).map(|i| q2[i] - q0[i]).collect();
    let d10: Vals = (0..n).map(|i| q1[i] - q0[i]).collect();
    let r = vnorm(&d20) / (vnorm(&d10) + f64::MIN_POSITIVE);
    let tol = if r > 0.0 && r < 1.0 { tol / r } else { tol };
    let mut q = Quad { f, tol, hmin: eps_of(b - a) / 1024.0, fcnt };
    let (total, w) = lobatto_step(it, &mut q, a, b, &y[0], &y[12])?;
    let fcnt = q.fcnt;
    quad_warning(it, w);
    Ok((total, fcnt))
}

/// quad, quadl or quadv: (Q, fcnt).
fn quad_family(it: &mut Interpreter, a: &[Value], name: &str) -> Ret {
    let f = arg(a, 0, name)?.clone();
    let lo = arg(a, 1, name)?.scalar()?;
    let hi = arg(a, 2, name)?.scalar()?;
    let tol = match a.get(3) {
        Some(v) if !v.is_empty_value() => v.scalar()?,
        _ => 1e-6,
    };
    let extra: Vec<Value> = a.get(5..).unwrap_or(&[]).to_vec();
    if !lo.is_finite() || !hi.is_finite() {
        warn(it, "The integration limits must be finite.");
        return Ok(Some(vec![Value::num(f64::NAN), Value::num(0.0)]));
    }
    if name == "quadv" {
        let mut dims = vec![1, 1];
        let mut g = |it: &mut Interpreter, xs: &[f64]| -> R<Vec<Vals>> {
            xs.iter()
                .map(|&x| {
                    let mut args = vec![Value::num(x)];
                    args.extend_from_slice(&extra);
                    let m = call1(it, &f, args)?.to_matrix()?;
                    dims = m.dims.clone();
                    Ok(vals_of(&m))
                })
                .collect()
        };
        let (q, n) = simpson(it, &mut g, lo, hi, tol)?;
        return Ok(Some(vec![vals_value(&q, &dims), Value::num(n as f64)]));
    }
    let mut g = vector_integrand(&f, &extra);
    let (q, n) = if name == "quadl" { lobatto(it, &mut g, lo, hi, tol)? } else { simpson(it, &mut g, lo, hi, tol)? };
    Ok(Some(vec![vals_value(&q, &[1, 1]), Value::num(n as f64)]))
}

/// dblquad and triplequad: nested quad (or quadl) calls with the innermost variable vectorized.
fn multi_quad(it: &mut Interpreter, a: &[Value], dims: usize, name: &str) -> Ret {
    let f = arg(a, 0, name)?.clone();
    let mut lims = Vec::new();
    for d in 0..dims {
        lims.push((arg(a, 1 + 2 * d, name)?.scalar()?, arg(a, 2 + 2 * d, name)?.scalar()?));
    }
    let tol = match a.get(1 + 2 * dims) {
        Some(v) if !v.is_empty_value() => v.scalar()?,
        _ => 1e-6,
    };
    let lobatto_method = match a.get(2 + 2 * dims) {
        Some(Value::Func(FuncValue::Named(n))) => n == "quadl",
        Some(v) if v.is_text() => v.text()? == "quadl",
        _ => false,
    };
    let extra: Vec<Value> = a.get(3 + 2 * dims..).unwrap_or(&[]).to_vec();
    fn level(it: &mut Interpreter, f: &Value, lims: &[(f64, f64)], fixed: &[f64], tol: f64, lob: bool, extra: &[Value]) -> R<f64> {
        // `fixed` holds the outer variables, outermost last.
        let d = lims.len() - 1 - fixed.len();
        let (lo, hi) = lims[d];
        let mut g = |it: &mut Interpreter, xs: &[f64]| -> R<Vec<Vals>> {
            if d == 0 {
                let mut args = vec![row(xs.to_vec())];
                args.extend(fixed.iter().rev().map(|&v| Value::num(v)));
                args.extend_from_slice(extra);
                let m = call1(it, f, args)?.to_matrix()?;
                if m.numel() != xs.len() {
                    return rt(SIZE_MISMATCH);
                }
                return Ok((0..xs.len()).map(|i| vec![m.data[i], m.im_at(i)]).collect());
            }
            xs.iter()
                .map(|&x| {
                    let mut fx = fixed.to_vec();
                    fx.push(x);
                    Ok(vec![level(it, f, lims, &fx, tol, lob, extra)?, 0.0])
                })
                .collect()
        };
        let (q, _) = if lob { lobatto(it, &mut g, lo, hi, tol)? } else { simpson(it, &mut g, lo, hi, tol)? };
        Ok(q[0])
    }
    one(Value::num(level(it, &f, &lims, &[], tol, lobatto_method, &extra)?))
}

// ---- ODE solvers ------------------------------------------------------------------------

struct Method {
    name: &'static str,
    /// Stage abscissae.
    c: &'static [f64],
    /// Stage coefficients: row s gives the weights of f1..f(s+1) for stage s+2; the last row
    /// is the solution.
    b: &'static [&'static [f64]],
    /// Error estimate weights over all stages (including f(tnew, ynew)).
    e: &'static [f64],
    /// Interpolation: row j gives f_j's coefficients of s, s², ….
    bi: &'static [&'static [f64]],
    pow: f64,
    shrink_min: f64,
    refine: f64,
}

const ODE45: Method = Method {
    name: "ode45",
    c: &[1.0 / 5.0, 3.0 / 10.0, 4.0 / 5.0, 8.0 / 9.0, 1.0, 1.0],
    b: &[
        &[1.0 / 5.0],
        &[3.0 / 40.0, 9.0 / 40.0],
        &[44.0 / 45.0, -56.0 / 15.0, 32.0 / 9.0],
        &[19372.0 / 6561.0, -25360.0 / 2187.0, 64448.0 / 6561.0, -212.0 / 729.0],
        &[9017.0 / 3168.0, -355.0 / 33.0, 46732.0 / 5247.0, 49.0 / 176.0, -5103.0 / 18656.0],
        &[35.0 / 384.0, 0.0, 500.0 / 1113.0, 125.0 / 192.0, -2187.0 / 6784.0, 11.0 / 84.0],
    ],
    e: &[71.0 / 57600.0, 0.0, -71.0 / 16695.0, 71.0 / 1920.0, -17253.0 / 339200.0, 22.0 / 525.0, -1.0 / 40.0],
    bi: &[
        &[1.0, -183.0 / 64.0, 37.0 / 12.0, -145.0 / 128.0],
        &[0.0, 0.0, 0.0, 0.0],
        &[0.0, 1500.0 / 371.0, -1000.0 / 159.0, 1000.0 / 371.0],
        &[0.0, -125.0 / 32.0, 125.0 / 12.0, -375.0 / 64.0],
        &[0.0, 9477.0 / 3392.0, -729.0 / 106.0, 25515.0 / 6784.0],
        &[0.0, -11.0 / 7.0, 11.0 / 3.0, -55.0 / 28.0],
        &[0.0, 3.0 / 2.0, -4.0, 5.0 / 2.0],
    ],
    pow: 1.0 / 5.0,
    shrink_min: 0.1,
    refine: 4.0,
};

const ODE23: Method = Method {
    name: "ode23",
    c: &[1.0 / 2.0, 3.0 / 4.0, 1.0],
    b: &[&[1.0 / 2.0], &[0.0, 3.0 / 4.0], &[2.0 / 9.0, 1.0 / 3.0, 4.0 / 9.0]],
    e: &[-5.0 / 72.0, 1.0 / 12.0, 1.0 / 9.0, -1.0 / 8.0],
    bi: &[&[1.0, -4.0 / 3.0, 5.0 / 9.0], &[0.0, 1.0, -2.0 / 3.0], &[0.0, 4.0 / 3.0, -8.0 / 9.0], &[0.0, -1.0, 1.0]],
    pow: 1.0 / 3.0,
    shrink_min: 0.5,
    refine: 1.0,
};

fn method(name: &str) -> Option<&'static Method> {
    match name {
        "ode45" => Some(&ODE45),
        "ode23" => Some(&ODE23),
        _ => None,
    }
}

/// The continuous extension of one step at `tq`, and its derivative.
fn interpolate(m: &Method, tq: f64, t: f64, y: &[f64], h: f64, f: &[Vec<f64>]) -> (Vec<f64>, Vec<f64>) {
    let s = (tq - t) / h;
    let neq = y.len();
    let mut yi = y.to_vec();
    let mut ypi = vec![0.0; neq];
    for (j, coef) in m.bi.iter().enumerate() {
        let (mut p, mut dp, mut sk) = (0.0, 0.0, 1.0);
        for (k, &c) in coef.iter().enumerate() {
            dp += c * (k + 1) as f64 * sk;
            sk *= s;
            p += c * sk;
        }
        if p == 0.0 && dp == 0.0 {
            continue;
        }
        for i in 0..neq {
            yi[i] += h * f[j][i] * p;
            ypi[i] += f[j][i] * dp;
        }
    }
    (yi, ypi)
}

struct OdeFn {
    f: Value,
    extra: Vec<Value>,
    neq: usize,
    text: String,
    nfevals: usize,
}

impl OdeFn {
    fn eval(&mut self, it: &mut Interpreter, t: f64, y: &[f64]) -> R<Vec<f64>> {
        let mut args = vec![Value::num(t), col(y.to_vec())];
        args.extend_from_slice(&self.extra);
        let m = call1(it, &self.f, args)?.to_matrix()?;
        self.nfevals += 1;
        if m.numel() != self.neq {
            return rt(format!(
                "{} returns a vector of length {}, but the length of initial conditions vector is {}. The vector returned by {} and the initial conditions vector must have the same number of elements.",
                self.text,
                m.numel(),
                self.neq,
                self.text
            ));
        }
        if m.dims.len() == 2 && m.dims[1] != 1 && self.neq > 1 {
            return rt(format!("{} must return a column vector.", self.text));
        }
        Ok(m.data)
    }
}

struct Events {
    f: Value,
    extra: Vec<Value>,
}

impl Events {
    fn eval(&self, it: &mut Interpreter, t: f64, y: &[f64]) -> R<(Vec<f64>, Vec<f64>, Vec<f64>)> {
        let mut args = vec![Value::num(t), col(y.to_vec())];
        args.extend_from_slice(&self.extra);
        let r = it.call_func_value(&self.f, args, 3)?;
        let get = |i: usize| -> R<Vec<f64>> { Ok(r.get(i).map(|v| v.to_matrix()).transpose()?.map(|m| m.data).unwrap_or_default()) };
        let v = get(0)?;
        let mut term = get(1)?;
        let mut dir = get(2)?;
        term.resize(v.len(), 0.0);
        dir.resize(v.len(), 0.0);
        Ok((v, term, dir))
    }
}

/// An event: its time, the state there and the (zero-based) event function index.
type EventHit = (f64, Vec<f64>, usize);

/// Events in (t, tnew]: (time, state, index) sorted by time, cut after the first terminal one.
#[allow(clippy::too_many_arguments)]
fn locate_events(
    it: &mut Interpreter,
    ev: &Events,
    m: &Method,
    vl: &[f64],
    vr: &[f64],
    term: &[f64],
    dir: &[f64],
    t: f64,
    y: &[f64],
    h: f64,
    tnew: f64,
    ynew: &[f64],
    f: &[Vec<f64>],
) -> R<(Vec<EventHit>, bool)> {
    let tol = (128.0 * eps_of(t).max(eps_of(tnew))).min((tnew - t).abs());
    let mut found = Vec::new();
    for i in 0..vl.len() {
        let (a, b) = (vl[i], vr[i]);
        if a == 0.0 || a.signum() == b.signum() && b != 0.0 || dir[i] * (b - a) < 0.0 {
            continue;
        }
        // Illinois regula falsi on the interpolant.
        let (mut tl, mut fl, mut tr, mut fr) = (t, a, tnew, b);
        let (mut yr, mut side) = (ynew.to_vec(), 0i32);
        while (tr - tl).abs() > tol && fr != 0.0 {
            let mut tm = tr - fr * (tr - tl) / (fr - fl);
            if !(tm > tl.min(tr) && tm < tl.max(tr)) || (tm - tl).abs() < tol / 2.0 || (tr - tm).abs() < tol / 2.0 {
                tm = 0.5 * (tl + tr);
            }
            let (ym, _) = interpolate(m, tm, t, y, h, f);
            let fm = ev.eval(it, tm, &ym)?.0[i];
            if fm.signum() == fl.signum() && fm != 0.0 {
                tl = tm;
                fl = fm;
                if side == -1 {
                    fr /= 2.0;
                }
                side = -1;
            } else {
                tr = tm;
                fr = fm;
                yr = ym;
                if side == 1 {
                    fl /= 2.0;
                }
                side = 1;
            }
        }
        found.push((tr, yr, i, term[i] != 0.0));
    }
    let sign = if tnew > t { 1.0 } else { -1.0 };
    found.sort_by(|x, y| (sign * x.0).partial_cmp(&(sign * y.0)).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Vec::new();
    let mut stop = false;
    for (te, ye, i, terminal) in found {
        if stop && out.last().is_some_and(|l: &EventHit| l.0 != te) {
            break;
        }
        out.push((te, ye, i));
        stop |= terminal;
    }
    Ok((out, stop))
}

fn ode_solve(it: &mut Interpreter, m: &'static Method, a: &[Value], nargout: usize) -> Ret {
    let f = arg(a, 0, m.name)?.clone();
    let tspan = arg(a, 1, m.name)?.to_matrix()?.data;
    let y0 = arg(a, 2, m.name)?.to_matrix()?.data;
    let o = options_arg(a.get(3))?;
    let extra: Vec<Value> = a.get(4..).unwrap_or(&[]).to_vec();
    if tspan.len() < 2 {
        return rt("The input argument tspan must have two or more elements.");
    }
    let (t0, tfinal) = (tspan[0], tspan[tspan.len() - 1]);
    if t0 == tfinal {
        return rt("The last entry in tspan must be different from the first entry.");
    }
    let tdir = (tfinal - t0).signum();
    if tspan.windows(2).any(|w| tdir * (w[1] - w[0]) <= 0.0) {
        return rt("The entries in tspan must strictly increase or decrease.");
    }
    let neq = y0.len();
    let text = func_text(it, &f);
    let mut fun = OdeFn { f: f.clone(), extra: extra.clone(), neq, text, nfevals: 0 };
    let mut rtol = opt_num(o, "RelTol", 1e-3)?;
    if rtol < 100.0 * f64::EPSILON {
        rtol = 100.0 * f64::EPSILON;
        warn(it, fmt("RelTol has been increased to %g.", &[rtol]));
    }
    let atol: Vec<f64> = match opt(o, "AbsTol") {
        Some(v) => v.to_matrix()?.data,
        None => vec![1e-6],
    };
    if atol.len() != 1 && atol.len() != neq {
        return rt(format!("Solving {} requires a scalar AbsTol, or a vector AbsTol of length {neq}", func_text(it, &f)));
    }
    let threshold: Vec<f64> = (0..neq).map(|i| atol[if atol.len() == 1 { 0 } else { i }] / rtol).collect();
    let normcontrol = matches!(opt_text(o, "NormControl", "off")?.as_str(), "on");
    let thr_scalar = threshold.first().copied().unwrap_or(0.0);
    let hmax = (tfinal - t0).abs().min(opt_num(o, "MaxStep", 0.1 * (tfinal - t0))?.abs());
    let htry = opt(o, "InitialStep").map(|v| v.scalar()).transpose()?.map(f64::abs);
    let refine = opt_num(o, "Refine", m.refine)?.max(1.0) as usize;
    let stats = opt_text(o, "Stats", "off")? == "on";
    let events = opt(o, "Events").filter(|v| is_func(v)).map(|v| Events { f: v.clone(), extra: extra.clone() });
    let ntspan = tspan.len();
    let want_sol = nargout == 1;
    let norm2 = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>().sqrt();

    let mut t = t0;
    let mut y = y0.clone();
    let mut f0 = fun.eval(it, t, &y)?;
    let mut normy = if normcontrol { norm2(&y) } else { 0.0 };
    let pow = m.pow;
    let mut hmin = 16.0 * eps_of(t);
    let htspan = (tspan[1] - tspan[0]).abs();
    let mut absh = match htry {
        Some(h) => hmax.min(hmin.max(h)),
        None => {
            let mut absh = hmax.min(htspan);
            let rh = if normcontrol {
                (norm2(&f0) / normy.max(thr_scalar)) / (0.8 * rtol.powf(pow))
            } else {
                (0..neq).map(|i| (f0[i] / y[i].abs().max(threshold[i])).abs()).fold(0.0, f64::max) / (0.8 * rtol.powf(pow))
            };
            if absh * rh > 1.0 {
                absh = 1.0 / rh;
            }
            absh.max(hmin)
        }
    };

    let mut tout = vec![t0];
    let mut yout = vec![y0.clone()];
    let mut f3d: Vec<Vec<Vec<f64>>> = Vec::new();
    let mut next = 1usize;
    let (mut nsteps, mut nfailed) = (0usize, 0usize);
    let (mut te, mut ye, mut ie): (Vec<f64>, Vec<Vec<f64>>, Vec<f64>) = (Vec::new(), Vec::new(), Vec::new());
    let mut vleft = match &events {
        Some(ev) => Some(ev.eval(it, t, &y)?),
        None => None,
    };
    let stages = m.e.len();
    let mut done = false;
    while !done {
        hmin = 16.0 * eps_of(t);
        absh = hmax.min(hmin.max(absh));
        let mut h = tdir * absh;
        if 1.1 * absh >= (tfinal - t).abs() {
            h = tfinal - t;
            absh = h.abs();
            done = true;
        }
        let mut nofailed = true;
        let (tnew, ynew, fs, err) = loop {
            let combo = |coef: &[f64], fk: &[Vec<f64>], h: f64| -> Vec<f64> {
                (0..neq).map(|i| y[i] + h * coef.iter().enumerate().map(|(j, c)| c * fk[j][i]).sum::<f64>()).collect()
            };
            let mut fk: Vec<Vec<f64>> = vec![f0.clone()];
            let last = m.b.len() - 1;
            for s in 0..last {
                let ys = combo(m.b[s], &fk, h);
                fk.push(fun.eval(it, t + h * m.c[s], &ys)?);
            }
            let tn = if done { tfinal } else { t + h * m.c[last] };
            // The solution uses the step as attempted; the interpolant uses the purified one.
            let yn = combo(m.b[last], &fk, h);
            h = tn - t;
            fk.push(fun.eval(it, tn, &yn)?);
            let ek: Vec<f64> = (0..neq).map(|i| (0..stages).map(|j| m.e[j] * fk[j][i]).sum()).collect();
            let normynew = if normcontrol { norm2(&yn) } else { 0.0 };
            let e = if normcontrol {
                absh * (norm2(&ek) / normy.max(normynew).max(thr_scalar))
            } else {
                absh * (0..neq).map(|i| (ek[i] / y[i].abs().max(yn[i].abs()).max(threshold[i])).abs()).fold(0.0, |a: f64, b| if b.is_nan() { f64::NAN } else { a.max(b) })
            };
            if e <= rtol {
                if normcontrol {
                    normy = normynew;
                }
                break (tn, yn, fk, e);
            }
            nfailed += 1;
            if absh <= hmin {
                warn(
                    it,
                    format!(
                        "Failure at t={}.  Unable to meet integration tolerances without reducing the step size below the smallest value allowed ({}) at time t.",
                        fmt("%e", &[t]),
                        fmt("%e", &[hmin])
                    ),
                );
                return ode_outputs(it, m, &fun, nargout, &tout, &yout, &f3d, (&te, &ye, &ie), (nsteps, nfailed), want_sol, a);
            }
            if nofailed && !e.is_nan() {
                nofailed = false;
                absh = hmin.max(absh * m.shrink_min.max(0.8 * (rtol / e).powf(pow)));
            } else {
                nofailed = false;
                absh = hmin.max(0.5 * absh);
            }
            h = tdir * absh;
            done = false;
        };
        nsteps += 1;
        let mut tnew = tnew;
        let mut ynew = ynew;
        let mut stop = false;
        if let (Some(ev), Some((vl, term, dir))) = (&events, &vleft) {
            let (vr, _, _) = ev.eval(it, tnew, &ynew)?;
            let (hits, terminal) = locate_events(it, ev, m, vl, &vr, term, dir, t, &y, h, tnew, &ynew, &fs)?;
            for (et, ey, ei) in hits {
                te.push(et);
                ye.push(ey);
                ie.push(ei as f64 + 1.0);
            }
            if terminal {
                stop = true;
                tnew = *te.last().unwrap();
                ynew = ye.last().unwrap().clone();
            }
            vleft = Some((vr, term.clone(), dir.clone()));
        }
        if want_sol {
            f3d.push(fs.clone());
            tout.push(tnew);
            yout.push(ynew.clone());
        } else if ntspan > 2 {
            while next < ntspan {
                let tq = tspan[next];
                if tdir * (tnew - tq) < 0.0 {
                    break;
                }
                tout.push(tq);
                yout.push(if tq == tnew { ynew.clone() } else { interpolate(m, tq, t, &y, h, &fs).0 });
                next += 1;
            }
            if stop && tout.last() != Some(&tnew) {
                tout.push(tnew);
                yout.push(ynew.clone());
            }
        } else {
            for k in 1..refine {
                let tq = t + h * k as f64 / refine as f64;
                if stop && tdir * (tq - tnew) >= 0.0 {
                    break;
                }
                tout.push(tq);
                yout.push(interpolate(m, tq, t, &y, h, &fs).0);
            }
            tout.push(tnew);
            yout.push(ynew.clone());
        }
        if done || stop {
            break;
        }
        if nofailed {
            let temp = 1.25 * (err / rtol).powf(pow);
            absh = if temp > 0.2 { absh / temp } else { 5.0 * absh };
        }
        t = tnew;
        y = ynew;
        f0 = fs[stages - 1].clone();
    }
    if stats {
        it.emit(OutputKind::Text(format!("{nsteps} successful steps\n{nfailed} failed attempts\n{} function evaluations\n", fun.nfevals)));
    }
    ode_outputs(it, m, &fun, nargout, &tout, &yout, &f3d, (&te, &ye, &ie), (nsteps, nfailed), want_sol, a)
}

#[allow(clippy::too_many_arguments)]
fn ode_outputs(
    it: &mut Interpreter,
    m: &Method,
    fun: &OdeFn,
    nargout: usize,
    tout: &[f64],
    yout: &[Vec<f64>],
    f3d: &[Vec<Vec<f64>>],
    ev: (&[f64], &[Vec<f64>], &[f64]),
    counts: (usize, usize),
    want_sol: bool,
    a: &[Value],
) -> Ret {
    let neq = fun.neq;
    if want_sol {
        let mut s = StructArray::scalar();
        s.set("solver", Value::chars(m.name));
        let mut ext = StructArray::scalar();
        ext.set("odefun", fun.f.clone());
        ext.set("options", a.get(3).cloned().unwrap_or_else(|| Value::Struct(blank_options(ODE_FIELDS))));
        ext.set("varargin", Value::Cell(CellArray::row(fun.extra.clone())));
        s.set("extdata", Value::Struct(ext));
        s.set("x", row(tout.to_vec()));
        s.set("y", cols_to_matrix(yout, neq));
        if a.get(3).and_then(|o| if let Value::Struct(st) = o { opt(Some(st), "Events") } else { None }).is_some() {
            s.set("xe", row(ev.0.to_vec()));
            s.set("ye", cols_to_matrix(ev.1, neq));
            s.set("ie", row(ev.2.to_vec()));
        }
        let mut st = StructArray::scalar();
        st.set("nsteps", Value::num(counts.0 as f64));
        st.set("nfailed", Value::num(counts.1 as f64));
        st.set("nfevals", Value::num(fun.nfevals as f64));
        s.set("stats", Value::Struct(st));
        let stages = m.e.len();
        let mut data = Vec::with_capacity(neq * stages * f3d.len());
        for step in f3d {
            for stage in step {
                data.extend_from_slice(stage);
            }
        }
        let mut idata = StructArray::scalar();
        idata.set("f3d", Value::Num(Matrix::from_dims(vec![neq, stages, f3d.len()], data)));
        idata.set("idxNonNegative", Value::empty());
        s.set("idata", Value::Struct(idata));
        return one(Value::Struct(s));
    }
    let t = col(tout.to_vec());
    let y = rows_to_matrix(yout, neq);
    if nargout == 0 {
        it.call_function("plot", vec![t, y], 0)?;
        return Ok(Some(Vec::new()));
    }
    let te = col(ev.0.to_vec());
    let ye = rows_to_matrix(ev.1, neq);
    let ie = col(ev.2.to_vec());
    Ok(Some(vec![t, y, te, ye, ie].into_iter().take(nargout).collect()))
}

fn deval(a: &[Value], nargout: usize) -> Ret {
    // deval(sol, xint, idx) or deval(xint, sol, idx).
    let (sol, xint) = match (arg(a, 0, "deval")?, arg(a, 1, "deval")?) {
        (Value::Struct(s), x) | (x, Value::Struct(s)) => (s, x.to_matrix()?.data),
        _ => return rt("DEVAL requires a solution structure."),
    };
    let solver = sol.get("solver").map(|v| v.text()).transpose()?.unwrap_or_default();
    let Some(m) = method(&solver) else { return rt(format!("Solution structure from {solver} is not supported.")) };
    let x = sol.get("x").ok_or_else(|| LangError::new("Invalid solution structure.", 0))?.to_matrix()?.data;
    let ym = sol.get("y").ok_or_else(|| LangError::new("Invalid solution structure.", 0))?.to_matrix()?;
    let f3d = match sol.get("idata") {
        Some(Value::Struct(id)) => id.get("f3d").map(|v| v.to_matrix()).transpose()?,
        _ => None,
    }
    .ok_or_else(|| LangError::new("Invalid solution structure.", 0))?;
    let neq = ym.rows();
    let stages = m.e.len();
    let idx: Vec<usize> = match a.get(2) {
        Some(v) => v.to_matrix()?.data.iter().map(|&k| k as usize - 1).collect(),
        None => (0..neq).collect(),
    };
    if idx.iter().any(|&k| k >= neq) {
        return rt("Index exceeds the number of solution components.");
    }
    let n = x.len();
    let (lo, hi) = (x[0].min(x[n - 1]), x[0].max(x[n - 1]));
    let dir = if x[n - 1] >= x[0] { 1.0 } else { -1.0 };
    let mut out = Vec::with_capacity(idx.len() * xint.len());
    let mut dout = Vec::with_capacity(idx.len() * xint.len());
    for &xi in &xint {
        if xi < lo || xi > hi {
            return rt(format!("Attempting to evaluate the solution outside the interval {} where it is defined.", fmt("[%e, %e]", &[x[0], x[n - 1]])));
        }
        let k = (0..n - 1).find(|&k| dir * (xi - x[k + 1]) <= 0.0).unwrap_or(n - 2);
        let y: Vec<f64> = (0..neq).map(|i| ym.get(i, k)).collect();
        let fs: Vec<Vec<f64>> = (0..stages).map(|j| (0..neq).map(|i| f3d.data[k * neq * stages + j * neq + i]).collect()).collect();
        let h = x[k + 1] - x[k];
        let (yi, ypi) = if xi == x[k + 1] {
            ((0..neq).map(|i| ym.get(i, k + 1)).collect(), interpolate(m, xi, x[k], &y, h, &fs).1)
        } else {
            interpolate(m, xi, x[k], &y, h, &fs)
        };
        out.extend(idx.iter().map(|&i| yi[i]));
        dout.extend(idx.iter().map(|&i| ypi[i]));
    }
    let r = idx.len();
    Ok(Some(
        vec![Value::Num(Matrix::new(r, xint.len(), out)), Value::Num(Matrix::new(r, xint.len(), dout))].into_iter().take(nargout.max(1)).collect(),
    ))
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    let r = match name {
        "optimset" => return optimset(it, a, nargout),
        "optimget" => optget(a, OPTIM_FIELDS, "parameter"),
        "odeset" => {
            if a.is_empty() && nargout == 0 {
                let lines: Vec<String> = ODE_FIELDS.iter().map(|f| format!("{f:>16}: ")).collect();
                it.emit(OutputKind::Text(lines.join("\n") + "\n\n"));
                return Ok(Some(Vec::new()));
            }
            one(Value::Struct(make_options(a, ODE_FIELDS, "property")?))
        }
        "odeget" => optget(a, ODE_FIELDS, "property"),
        "fzero" => fzero(it, a, nargout),
        "fminbnd" => fminbnd(it, a, nargout),
        "fminsearch" => fminsearch(it, a, nargout),
        "lsqnonneg" => lsqnonneg(it, a, nargout),
        "integral" => integral(it, a, false),
        "quadgk" => integral(it, a, true),
        "integral2" => integral_nd(it, a, 2, name, 1e-10, 1e-6),
        "integral3" => integral_nd(it, a, 3, name, 1e-10, 1e-6),
        "quad2d" => integral_nd(it, a, 2, name, 1e-5, 100.0 * f64::EPSILON),
        "quad" | "quadl" | "quadv" => quad_family(it, a, name),
        "dblquad" => multi_quad(it, a, 2, name),
        "triplequad" => multi_quad(it, a, 3, name),
        "ode45" | "ode23" => ode_solve(it, method(name).unwrap(), a, nargout),
        "deval" => deval(a, nargout),
        _ => return Ok(None),
    }?;
    // Functions with one natural output return only that unless more are asked for.
    Ok(r.map(|mut v| {
        if matches!(name, "integral" | "integral2" | "integral3" | "quad" | "quadl" | "quadv" | "quadgk" | "quad2d") {
            v.truncate(nargout.max(1));
        }
        v
    }))
}
