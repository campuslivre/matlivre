//! More plots: the `ez*` function plotters, `fimplicit`, animated lines (`animatedline`,
//! `addpoints`, `getpoints`, `clearpoints`), `histogram2`, `pie3`, `plotmatrix`, `surfl`,
//! vector field derivatives (`curl`, `divergence`) and movie frames (`getframe`, `frame2im`,
//! `im2frame`, `movie`), surface normals (`surfnorm`), ruler secondary labels
//! (`xsecondarylabel`, ...) and ruler value conversion (`ruler2num`, `num2ruler`).

use crate::builtins::{arg, none, one, Ret};
use crate::builtins_plot::{apply_props, handles_out};
use crate::error::{rt, LangError};
use crate::gfx::{self, Target};
use crate::graphics::{ExportFormat, LineStyle, Series, SeriesKind, XAxisKind};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "ezplot", "ezplot3", "ezsurf", "ezsurfc", "ezmesh", "ezmeshc", "ezcontour", "ezcontourf", "fimplicit", "fimplicit3", "animatedline", "addpoints", "getpoints",
    "clearpoints", "histogram2", "pie3", "plotmatrix", "surfl", "curl", "divergence", "getframe", "frame2im", "im2frame", "movie",
    "surfnorm", "xsecondarylabel", "ysecondarylabel", "zsecondarylabel", "ruler2num", "num2ruler",
];

/// Unit normals of a surface: edges are extended by cubic extrapolation, then the normal at each
/// point is the cross product of the central differences along the columns and the rows.
fn surface_normals(x: &Matrix, y: &Matrix, z: &Matrix) -> R<[Matrix; 3]> {
    let (m, n) = (z.rows(), z.cols());
    if m < 3 || n < 3 {
        return rt("Z must be at least 3-by-3.");
    }
    let ext = |a: &Matrix| -> Vec<Vec<f64>> {
        let at = |i: usize, j: usize| a.data[i + m * j];
        let mut g = vec![vec![0.0; n + 2]; m + 2];
        for i in 0..m {
            for j in 0..n {
                g[i + 1][j + 1] = at(i, j);
            }
            g[i + 1][0] = 3.0 * at(i, 0) - 3.0 * at(i, 1) + at(i, 2);
            g[i + 1][n + 1] = 3.0 * at(i, n - 1) - 3.0 * at(i, n - 2) + at(i, n - 3);
        }
        for j in 0..n + 2 {
            g[0][j] = 3.0 * g[1][j] - 3.0 * g[2][j] + g[3][j];
            g[m + 1][j] = 3.0 * g[m][j] - 3.0 * g[m - 1][j] + g[m - 2][j];
        }
        g
    };
    let (gx, gy, gz) = (ext(x), ext(y), ext(z));
    let mut out = [vec![0.0; m * n], vec![0.0; m * n], vec![0.0; m * n]];
    for i in 1..=m {
        for j in 1..=n {
            let u = [gx[i][j + 1] - gx[i][j - 1], gy[i][j + 1] - gy[i][j - 1], gz[i][j + 1] - gz[i][j - 1]];
            let v = [gx[i + 1][j] - gx[i - 1][j], gy[i + 1][j] - gy[i - 1][j], gz[i + 1][j] - gz[i - 1][j]];
            let c = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            let len = if len == 0.0 { 1.0 } else { len };
            for k in 0..3 {
                out[k][(i - 1) + m * (j - 1)] = c[k] / len;
            }
        }
    }
    let [a, b, c] = out;
    Ok([Matrix::new(m, n, a), Matrix::new(m, n, b), Matrix::new(m, n, c)])
}

fn surfnorm(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = crate::builtins_plot::select_axes(it, a)?;
    let (x, y, z) = match a.len() {
        0 => return rt("Not enough input arguments."),
        1 | 2 => {
            let z = a[0].to_matrix()?;
            let (m, n) = (z.rows(), z.cols());
            let x = Matrix::new(m, n, (0..m * n).map(|k| (k / m + 1) as f64).collect());
            let y = Matrix::new(m, n, (0..m * n).map(|k| (k % m + 1) as f64).collect());
            (x, y, z)
        }
        _ => {
            let (x, y, z) = (a[0].to_matrix()?, a[1].to_matrix()?, a[2].to_matrix()?);
            let (m, n) = (z.rows(), z.cols());
            // Vector x and y expand to the grid.
            let x = if x.numel() == n && m != 1 { Matrix::new(m, n, (0..m * n).map(|k| x.data[k / m]).collect()) } else { x };
            let y = if y.numel() == m && n != 1 { Matrix::new(m, n, (0..m * n).map(|k| y.data[k % m]).collect()) } else { y };
            (x, y, z)
        }
    };
    let [nx, ny, nz] = surface_normals(&x, &y, &z)?;
    if nargout > 0 {
        return Ok(Some(vec![Value::Num(nx), Value::Num(ny), Value::Num(nz)].into_iter().take(nargout).collect()));
    }
    // No outputs: draw the surface with its normals.
    let held = it.current_figure().axes_mut().hold;
    let (xv, yv, zv) = (Value::Num(x), Value::Num(y), Value::Num(z));
    it.call_function("surf", vec![xv.clone(), yv.clone(), zv.clone()], 0)?;
    it.current_figure().axes_mut().hold = true;
    it.call_function("quiver3", vec![xv, yv, zv, Value::Num(nx), Value::Num(ny), Value::Num(nz), Value::num(0.0)], 0)?;
    it.current_figure().axes_mut().hold = held;
    none()
}

fn secondary_label(it: &mut Interpreter, name: &str, a: &[Value]) -> Ret {
    let a = crate::builtins_plot::select_axes(it, a)?;
    let text = match a.first() {
        Some(v) => crate::builtins_plot::text_of(v)?,
        None => return rt("Not enough input arguments."),
    };
    let k = match name {
        "xsecondarylabel" => 0,
        "ysecondarylabel" => 1,
        _ => 2,
    };
    it.current_figure().axes_mut().secondary[k] = text;
    let part = [gfx::Part::XSecondary, gfx::Part::YSecondary, gfx::Part::ZSecondary][k];
    let label = gfx::ids_of(&it.current_axes_handle()).unwrap()[0] + part as u64;
    if a.len() > 1 {
        let mut sa = vec![gfx::handle(it, label)];
        sa.extend(a[1..].iter().cloned());
        it.call_function("set", sa, 0)?;
    }
    none()
}

/// `ruler2num` / `num2ruler`: numeric rulers pass values through; datetime rulers count days from
/// the start of the day of the earliest plotted time.
fn ruler_convert(it: &mut Interpreter, name: &str, a: &[Value]) -> Ret {
    let v = arg(a, 0, name)?;
    let ruler = arg(a, 1, name)?;
    let Some(Target::Part(f, ax, part)) = gfx::ids_of(ruler).and_then(|ids| ids.first().copied()).and_then(|id| gfx::resolve(it, id)) else {
        return rt("The second argument must be a ruler object.");
    };
    if !matches!(part, gfx::Part::XAxis | gfx::Part::YAxis | gfx::Part::ZAxis | gfx::Part::YAxisRight) {
        return rt("The second argument must be a ruler object.");
    }
    let axes = &it.figures[f].axes[ax];
    if !(part == gfx::Part::XAxis && axes.x_kind == XAxisKind::DateTime) {
        return match (name, v) {
            ("ruler2num", Value::Time(_)) => rt("Values must be numeric for a numeric ruler."),
            _ => one(Value::Num(v.to_matrix()?)),
        };
    }
    const MS_DAY: f64 = 86_400_000.0;
    let first = axes.series.iter().flat_map(|s| s.x.iter().copied()).filter(|x| x.is_finite()).fold(f64::INFINITY, f64::min);
    let origin = if first.is_finite() { (first / MS_DAY).floor() * MS_DAY } else { 0.0 };
    match name {
        "ruler2num" => {
            let Value::Time(t) = v else { return rt("Values must be datetime values for a datetime ruler.") };
            let (m, _) = crate::builtins_plot::x_numeric(&Value::Time(t.clone()))?;
            one(Value::Num(Matrix::from_dims(m.dims.clone(), m.data.iter().map(|x| (x - origin) / MS_DAY).collect())))
        }
        _ => {
            let m = v.to_matrix()?;
            let ms = m.data.iter().map(|d| origin + d * MS_DAY).collect();
            one(Value::Time(crate::time::TimeArray::datetimes(m.dims.clone(), ms, None, String::new(), crate::time::DEFAULT_DT_FORMAT.into())))
        }
    }
}

const TWO_PI: f64 = 2.0 * std::f64::consts::PI;

fn is_textlike(v: &Value) -> bool {
    v.is_text() || matches!(v, Value::Str(s) if s.data.len() == 1)
}

fn num(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.kind != Kind::Char)
}

/// Positional arguments up to the first text, then name-value pairs.
fn split_props(a: &[Value]) -> R<(&[Value], Vec<(String, Value)>)> {
    let i = a.iter().position(is_textlike).unwrap_or(a.len());
    let (head, tail) = a.split_at(i);
    if tail.len() % 2 != 0 {
        return rt(format!("Invalid parameter/value pair arguments ('{}').", tail.last().map(|v| v.text().unwrap_or_default()).unwrap_or_default()));
    }
    Ok((head, tail.chunks(2).map(|p| Ok((p[0].text()?, p[1].clone()))).collect::<R<_>>()?))
}

fn first(it: &mut Interpreter, name: &str, args: Vec<Value>) -> R<Value> {
    it.call_function(name, args, 1)?.into_iter().next().ok_or_else(|| LangError::new(format!("{name} returned no value."), 0))
}

fn grid_row(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..n).map(|k| lo + (hi - lo) * k as f64 / (n - 1) as f64).collect()
}

/// A handle array of the given shape.
fn handle_matrix(it: &Interpreter, ids: &[u64], rows: usize, cols: usize) -> Value {
    let v = gfx::handles(it, ids);
    if let Value::Object(o) = &v {
        let data: Vec<f64> = ids.iter().map(|&i| i as f64).collect();
        o.props.borrow_mut()[0] = Value::Num(Matrix::new(rows, cols, data));
    }
    v
}

fn ids(v: &Value) -> Vec<u64> {
    gfx::ids_of(v).unwrap_or_default()
}

// ---- functions given as text or handles ------------------------------------------------------

/// A plotted function: the handle to evaluate, its variable names and its title text.
struct Fun {
    f: Value,
    vars: Vec<String>,
    text: String,
}

const KNOWN: &[&str] = &["pi", "exp", "log", "sqrt", "abs", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "sec", "csc", "cot", "log10", "log2", "e", "i", "j", "inf", "Inf", "NaN", "real", "imag", "sign", "floor", "ceil", "round", "mod", "rem", "max", "min", "besselj", "gamma", "erf", "atan2"];

/// Text in MATLAB's vectorized form (`.*`, `./`, `.^`).
fn vectorize(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if matches!(c, '*' | '/' | '^') && (i == 0 || chars[i - 1] != '.') {
            out.push('.');
        }
        out.push(c);
    }
    out
}

fn fun(it: &mut Interpreter, v: &Value) -> R<Fun> {
    if let Value::Func(_) = v {
        let text = first(it, "func2str", vec![v.clone()])?.text()?;
        let (vars, body) = match text.strip_prefix("@(").and_then(|r| r.split_once(')')) {
            Some((p, b)) => (p.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(), b.trim().to_string()),
            None => (vec!["x".into()], text.trim_start_matches('@').to_string()),
        };
        return Ok(Fun { f: v.clone(), vars, text: body });
    }
    let text = if let Value::Sym(_) = v { first(it, "char", vec![v.clone()])?.text()? } else { v.text()? };
    // Variables: identifiers that are not known functions or constants, in alphabetical order.
    let mut vars: Vec<String> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_alphabetic() {
            let s = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let id: String = chars[s..i].iter().collect();
            let call = chars.get(i) == Some(&'(');
            if !call && !KNOWN.contains(&id.as_str()) && !vars.contains(&id) {
                vars.push(id);
            }
        } else {
            i += 1;
        }
    }
    vars.sort();
    if vars.is_empty() {
        vars.push("x".into());
    }
    let src = format!("@({}) {}", vars.join(","), vectorize(&text));
    let f = first(it, "str2func", vec![Value::chars(&src)])?;
    Ok(Fun { f, vars, text })
}

/// f evaluated elementwise on equal-sized arguments (vectorized when the function allows).
fn eval(it: &mut Interpreter, f: &Value, args: &[Matrix]) -> R<Matrix> {
    let n = args[0].numel();
    let vals: Vec<Value> = args.iter().map(|m| Value::Num(m.clone())).collect();
    if let Ok(mut r) = it.call_func_value(f, vals, 1) {
        if let Some(Value::Num(m)) = r.pop() {
            if m.numel() == n {
                return Ok(Matrix::from_dims(args[0].dims.clone(), m.data));
            }
            if m.numel() == 1 {
                return Ok(Matrix::from_dims(args[0].dims.clone(), vec![m.data[0]; n]));
            }
        }
    }
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let point: Vec<Value> = args.iter().map(|m| Value::num(m.data[k])).collect();
        out.push(it.call_func_value(f, point, 1)?.pop().map(|v| v.scalar()).transpose()?.unwrap_or(f64::NAN));
    }
    Ok(Matrix::from_dims(args[0].dims.clone(), out))
}

fn title(it: &mut Interpreter, text: &str) -> R<()> {
    let t = first(it, "texlabel", vec![Value::chars(text)])?;
    it.call_function("title", vec![t], 0)?;
    Ok(())
}

fn domain(v: Option<&Value>, default: [f64; 4]) -> R<[f64; 4]> {
    match v.filter(|v| num(v)) {
        None => Ok(default),
        Some(v) => {
            let d = v.to_matrix()?.data;
            match d.len() {
                2 => Ok([d[0], d[1], d[0], d[1]]),
                4 => Ok([d[0], d[1], d[2], d[3]]),
                _ => rt("The domain must be [min max] or [xmin xmax ymin ymax]."),
            }
        }
    }
}

fn meshgrid(d: [f64; 4], n: usize) -> (Matrix, Matrix) {
    let (xs, ys) = (grid_row(d[0], d[1], n), grid_row(d[2], d[3], n));
    let mut x = vec![0.0; n * n];
    let mut y = vec![0.0; n * n];
    for j in 0..n {
        for i in 0..n {
            x[j * n + i] = xs[j];
            y[j * n + i] = ys[i];
        }
    }
    (Matrix::new(n, n, x), Matrix::new(n, n, y))
}

fn set_limits(it: &mut Interpreter, d: [f64; 4]) -> R<()> {
    it.call_function("xlim", vec![Value::Num(Matrix::row(vec![d[0], d[1]]))], 0)?;
    it.call_function("ylim", vec![Value::Num(Matrix::row(vec![d[2], d[3]]))], 0)?;
    Ok(())
}

fn ezplot(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let f = fun(it, arg(a, 0, "ezplot")?)?;
    // ezplot(x(t), y(t)): a parametric curve.
    if let Some(g) = a.get(1).filter(|v| !num(v)) {
        let g = fun(it, g)?;
        let d = domain(a.get(2), [0.0, TWO_PI, 0.0, TWO_PI])?;
        let t = Matrix::row(grid_row(d[0], d[1], 300));
        let (x, y) = (eval(it, &f.f, &[t.clone()])?, eval(it, &g.f, &[t])?);
        let h = first(it, "plot", vec![Value::Num(x), Value::Num(y)])?;
        title(it, &format!("x = {}, y = {}", f.text, g.text))?;
        return handles_out(it, &ids(&h), nargout);
    }
    let d = domain(a.get(1), [-TWO_PI, TWO_PI, -TWO_PI, TWO_PI])?;
    if f.vars.len() >= 2 {
        // f(x, y) = 0, drawn as the zero contour.
        let (x, y) = meshgrid(d, 250);
        let z = eval(it, &f.f, &[x.clone(), y.clone()])?;
        let h = it.call_function("contour", vec![Value::Num(x), Value::Num(y), Value::Num(z), Value::Num(Matrix::row(vec![0.0, 0.0]))], 2)?.pop().unwrap_or_else(Value::empty);
        set_limits(it, d)?;
        title(it, &format!("{} = 0", f.text))?;
        return handles_out(it, &ids(&h), nargout);
    }
    let x = Matrix::row(grid_row(d[0], d[1], 500));
    let y = eval(it, &f.f, &[x.clone()])?;
    let h = first(it, "plot", vec![Value::Num(x), Value::Num(y)])?;
    it.call_function("xlim", vec![Value::Num(Matrix::row(vec![d[0], d[1]]))], 0)?;
    title(it, &f.text)?;
    handles_out(it, &ids(&h), nargout)
}

fn ezplot3(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let fs: Vec<Fun> = (0..3).map(|k| fun(it, arg(a, k, "ezplot3")?)).collect::<R<_>>()?;
    let d = domain(a.get(3), [0.0, TWO_PI, 0.0, TWO_PI])?;
    let t = Matrix::row(grid_row(d[0], d[1], 300));
    let pts: Vec<Value> = fs.iter().map(|f| eval(it, &f.f, &[t.clone()]).map(Value::Num)).collect::<R<_>>()?;
    let h = first(it, "plot3", pts)?;
    title(it, &format!("x = {}, y = {}, z = {}", fs[0].text, fs[1].text, fs[2].text))?;
    handles_out(it, &ids(&h), nargout)
}

/// ezsurf, ezmesh, ezcontour and their combined forms: z = f(x, y) on a grid.
fn ez_grid(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    let f = fun(it, arg(a, 0, name)?)?;
    let d = domain(a.get(1), [-TWO_PI, TWO_PI, -TWO_PI, TWO_PI])?;
    let n = a.iter().skip(1).filter(|v| num(v) && v.numel() == 1).map(|v| v.scalar()).next().transpose()?.unwrap_or(60.0) as usize;
    let (x, y) = meshgrid(d, n.max(2));
    let z = eval(it, &f.f, &[x.clone(), y.clone()])?;
    let plotter = match name {
        "ezsurf" => "surf",
        "ezsurfc" => "surfc",
        "ezmesh" => "mesh",
        "ezmeshc" => "meshc",
        "ezcontour" => "contour",
        _ => "contourf",
    };
    // contour and contourf return the contour matrix first and the handle second.
    let contour = plotter.starts_with("contour");
    let r = it.call_function(plotter, vec![Value::Num(x), Value::Num(y), Value::Num(z)], if contour { 2 } else { 1 })?;
    set_limits(it, d)?;
    title(it, &f.text)?;
    let h = (if contour { r.into_iter().nth(1) } else { r.into_iter().next() }).unwrap_or_else(Value::empty);
    handles_out(it, &ids(&h), nargout)
}

// ---- fimplicit3 ----------------------------------------------------------------------------

/// fimplicit3(f, [xmin xmax ymin ymax zmin zmax]): the surface f(x, y, z) = 0, as an isosurface
/// of f sampled on a MeshDensity-point grid (35 by default) over the ranges ([-5 5] each).
fn fimplicit3(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let f = fun(it, arg(a, 0, "fimplicit3")?)?;
    let given = a.get(1).filter(|v| num(v));
    let r = match given.map(|v| v.to_matrix()).transpose()?.map(|m| m.data) {
        None => [-5.0, 5.0, -5.0, 5.0, -5.0, 5.0],
        Some(v) if v.len() == 2 => [v[0], v[1], v[0], v[1], v[0], v[1]],
        Some(v) if v.len() == 4 => [v[0], v[1], v[2], v[3], -5.0, 5.0],
        Some(v) if v.len() == 6 => [v[0], v[1], v[2], v[3], v[4], v[5]],
        _ => return rt("Specify the interval as a vector of the form [min max], [xmin xmax ymin ymax] or [xmin xmax ymin ymax zmin zmax]."),
    };
    let rest = &a[if given.is_some() { 2 } else { 1 }..];
    let props = split_props(rest)?.1;
    let density = props.iter().find(|(k, _)| k.eq_ignore_ascii_case("MeshDensity")).map(|(_, v)| v.scalar()).transpose()?.unwrap_or(35.0) as usize;
    let n = density.max(2);
    let axis = |lo: f64, hi: f64| Value::Num(Matrix::row(grid_row(lo, hi, n)));
    let g = it.call_function("meshgrid", vec![axis(r[0], r[1]), axis(r[2], r[3]), axis(r[4], r[5])], 3)?;
    let (x, y, z) = (g[0].to_matrix()?, g[1].to_matrix()?, g[2].to_matrix()?);
    let v = eval(it, &f.f, &[x.clone(), y.clone(), z.clone()])?;
    it.call_function("isosurface", vec![Value::Num(x), Value::Num(y), Value::Num(z), Value::Num(v), Value::num(0.0)], 0)?;
    let fi = it.current_figure_index();
    let Some(s) = it.figures[fi].axes_mut().series.last_mut() else { return none() };
    s.class_hint = Some("matlab.graphics.function.ImplicitFunctionSurface");
    let id = s.id;
    it.gfx_store.entry(id).or_default().extend([
        ("Function".to_string(), f.f.clone()),
        ("XRange".to_string(), Value::Num(Matrix::row(r[0..2].to_vec()))),
        ("YRange".to_string(), Value::Num(Matrix::row(r[2..4].to_vec()))),
        ("ZRange".to_string(), Value::Num(Matrix::row(r[4..6].to_vec()))),
        ("MeshDensity".to_string(), Value::num(n as f64)),
        ("EdgeColor".to_string(), Value::Num(Matrix::row(vec![0.0, 0.0, 0.0]))),
        ("FaceAlpha".to_string(), Value::num(1.0)),
    ]);
    apply_props(it, &[id], &props.into_iter().filter(|(k, _)| !k.eq_ignore_ascii_case("MeshDensity")).collect::<Vec<_>>())?;
    handles_out(it, &[id], nargout)
}

// ---- fimplicit -----------------------------------------------------------------------------

fn fimplicit(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let f = fun(it, arg(a, 0, "fimplicit")?)?;
    let given = a.get(1).filter(|v| num(v));
    let d = domain(given, [-5.0, 5.0, -5.0, 5.0])?;
    let rest = &a[if given.is_some() { 2 } else { 1 }..];
    let (spec, props) = match rest.first() {
        Some(v) if is_textlike(v) && rest.len() % 2 == 1 => (Some(v.clone()), split_props(&rest[1..])?.1),
        _ => (None, split_props(rest)?.1),
    };
    let density = props.iter().find(|(k, _)| k.eq_ignore_ascii_case("MeshDensity")).map(|(_, v)| v.scalar()).transpose()?.unwrap_or(151.0) as usize;
    let (x, y) = meshgrid(d, density.max(2));
    let z = eval(it, &f.f, &[x, y])?;
    // The zero level as NaN-separated segments (contourc's matrix: [level n; x y ...]).
    let xs = Value::Num(Matrix::row(grid_row(d[0], d[1], density.max(2))));
    let ys = Value::Num(Matrix::row(grid_row(d[2], d[3], density.max(2))));
    let cm = first(it, "contourc", vec![xs, ys, Value::Num(z), Value::Num(Matrix::row(vec![0.0, 0.0]))])?.to_matrix()?;
    let (mut px, mut py) = (Vec::new(), Vec::new());
    let mut k = 0;
    let cols = cm.cols();
    while k < cols {
        let n = cm.data[k * 2 + 1] as usize;
        if !px.is_empty() {
            px.push(f64::NAN);
            py.push(f64::NAN);
        }
        for j in k + 1..=k + n {
            px.push(cm.data[j * 2]);
            py.push(cm.data[j * 2 + 1]);
        }
        k += n + 1;
    }
    let mut s = Series::new(SeriesKind::Line, px.clone(), py.clone());
    s.class_hint = Some("matlab.graphics.function.ImplicitFunctionLine");
    if let Some(spec) = spec {
        let st = spec.text()?;
        if let Some((color, style, marker)) = crate::graphics::parse_line_spec(&st) {
            s.color = color;
            if let Some(ls) = style {
                s.line_style = ls;
            }
            s.marker = marker;
        }
    }
    let ids_ = it.add_series(vec![s], XAxisKind::Numeric);
    // Without an interval the ranges follow the curve, rounded out to whole numbers.
    let fin = |v: &[f64], lo: bool| v.iter().copied().filter(|x| x.is_finite()).fold(if lo { f64::INFINITY } else { f64::NEG_INFINITY }, if lo { f64::min } else { f64::max });
    let (xr, yr) = if given.is_some() || px.is_empty() {
        ([d[0], d[1]], [d[2], d[3]])
    } else {
        // Within 1% of a whole number counts as that number (the traced curve overshoots a little).
        let out = |v: f64, lo: bool| if (v - v.round()).abs() < 0.01 { v.round() } else if lo { v.floor() } else { v.ceil() };
        ([out(fin(&px, true), true), out(fin(&px, false), false)], [out(fin(&py, true), true), out(fin(&py, false), false)])
    };
    if let Some(&id) = ids_.first() {
        it.gfx_store.entry(id).or_default().extend([
            ("Function".to_string(), f.f.clone()),
            ("XRange".to_string(), Value::Num(Matrix::row(xr.to_vec()))),
            ("YRange".to_string(), Value::Num(Matrix::row(yr.to_vec()))),
            ("MeshDensity".to_string(), Value::num(density as f64)),
        ]);
    }
    it.call_function("xlim", vec![Value::Num(Matrix::row(xr.to_vec()))], 0)?;
    it.call_function("ylim", vec![Value::Num(Matrix::row(yr.to_vec()))], 0)?;
    let props: Vec<(String, Value)> = props.into_iter().filter(|(k, _)| !k.eq_ignore_ascii_case("MeshDensity")).collect();
    apply_props(it, &ids_, &props)?;
    handles_out(it, &ids_, nargout)
}

// ---- animated lines --------------------------------------------------------------------------

const ANIMATED: &str = "matlab.graphics.animation.AnimatedLine";

fn animatedline(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = crate::builtins_plot::select_axes(it, a)?;
    let (pos, mut props) = split_props(a)?;
    let coords: Vec<Vec<f64>> = pos.iter().map(|v| v.to_matrix().map(|m| m.data)).collect::<R<_>>()?;
    let mut s = Series::new(SeriesKind::Line, coords.first().cloned().unwrap_or_default(), coords.get(1).cloned().unwrap_or_default());
    s.z = coords.get(2).cloned().unwrap_or_default();
    s.class_hint = Some(ANIMATED);
    s.color = Some([0, 0, 0]);
    let max = match props.iter().position(|(k, _)| k.eq_ignore_ascii_case("MaximumNumPoints")) {
        Some(i) => props.remove(i).1.scalar()?,
        None => 1_000_000.0,
    };
    let ids_ = it.add_series(vec![s], XAxisKind::Numeric);
    if let Some(&id) = ids_.first() {
        it.gfx_store.entry(id).or_default().push(("MaximumNumPoints".to_string(), Value::num(max)));
    }
    apply_props(it, &ids_, &props)?;
    trim_points(it, ids_[0]);
    handles_out(it, &ids_, nargout)
}

fn animated_series<'a>(it: &'a mut Interpreter, v: &Value) -> R<(u64, &'a mut Series)> {
    let id = ids(v).first().copied().ok_or_else(|| LangError::new("The first input must be an animated line.", 0))?;
    match gfx::resolve(it, id) {
        Some(Target::Series(f, ax, si)) => {
            let s = &mut it.figures[f].axes[ax].series[si];
            if s.class_hint != Some(ANIMATED) {
                return rt("The first input must be an animated line.");
            }
            Ok((id, s))
        }
        _ => rt("Invalid or deleted object."),
    }
}

/// Drops the oldest points beyond MaximumNumPoints.
fn trim_points(it: &mut Interpreter, id: u64) {
    let max = it.gfx_store.get(&id).and_then(|m| m.iter().find(|(n, _)| n == "MaximumNumPoints")).and_then(|(_, v)| v.scalar().ok()).unwrap_or(1e6) as usize;
    if let Some(Target::Series(f, ax, si)) = gfx::resolve(it, id) {
        let s = &mut it.figures[f].axes[ax].series[si];
        let n = s.x.len();
        if n > max {
            let cut = n - max;
            s.x.drain(..cut);
            s.y.drain(..cut.min(s.y.len()));
            if !s.z.is_empty() {
                s.z.drain(..cut.min(s.z.len()));
            }
        }
    }
}

fn points(it: &mut Interpreter, name: &str, a: &[Value]) -> Ret {
    let h = arg(a, 0, name)?.clone();
    match name {
        "addpoints" => {
            let xs = arg(a, 1, name)?.to_matrix()?.data;
            let ys = arg(a, 2, name)?.to_matrix()?.data;
            let zs = a.get(3).map(|v| v.to_matrix()).transpose()?.map(|m| m.data);
            if xs.len() != ys.len() {
                return rt("The x and y data must be the same length.");
            }
            let (id, s) = animated_series(it, &h)?;
            s.x.extend(xs);
            s.y.extend(ys);
            if let Some(z) = zs {
                s.z.extend(z);
            }
            trim_points(it, id);
            if let Some(Target::Series(f, _, _)) = gfx::resolve(it, id) {
                let fid = it.figures[f].id;
                it.mark_figure_dirty(fid);
            }
            none()
        }
        "clearpoints" => {
            let (_, s) = animated_series(it, &h)?;
            s.x.clear();
            s.y.clear();
            s.z.clear();
            none()
        }
        _ => {
            let (_, s) = animated_series(it, &h)?;
            let (x, y, z) = (s.x.clone(), s.y.clone(), s.z.clone());
            let r = |v: Vec<f64>| Value::Num(if v.is_empty() { Matrix::new(1, 0, Vec::new()) } else { Matrix::row(v) });
            Ok(Some(vec![r(x), r(y), r(z)]))
        }
    }
}

// ---- histogram2 ------------------------------------------------------------------------------

fn histogram2(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let a = crate::builtins_plot::select_axes(it, a)?;
    let (pos, mut props) = split_props(a)?;
    let x = arg(pos, 0, "histogram2")?.to_matrix()?;
    let y = arg(pos, 1, "histogram2")?.to_matrix()?;
    if x.numel() != y.numel() {
        return rt("X and Y must have the same number of elements.");
    }
    // Counts and edges as histcounts2 computes them (MATLAB's automatic 2-D binning).
    let mut hargs = vec![Value::Num(Matrix::col(x.data.clone())), Value::Num(Matrix::col(y.data.clone()))];
    hargs.extend(pos[2..].iter().cloned());
    let r = it.call_function("histcounts2", hargs, 3)?;
    let counts_m = r[0].to_matrix()?;
    let (ex, ey) = (r[1].to_matrix()?.data, r[2].to_matrix()?.data);
    let (nx, ny) = (ex.len() - 1, ey.len() - 1);
    let counts = counts_m.data;
    let style = match props.iter().position(|(k, _)| k.eq_ignore_ascii_case("DisplayStyle")) {
        Some(i) => props.remove(i).1.text()?.to_lowercase(),
        None => "bar3".into(),
    };
    let mut f = (Vec::new(), Vec::new(), Vec::new());
    for j in 0..ny {
        for i in 0..nx {
            let c = counts[j * nx + i];
            if c > 0.0 {
                let z = if style == "tile" { (0.0, 0.0) } else { (0.0, c) };
                crate::builtins_plotmore::box_faces((ex[i], ex[i + 1]), (ey[j], ey[j + 1]), z, &mut f);
            }
        }
    }
    let mut s = Series::new(SeriesKind::Patch, f.0, f.1);
    s.z = f.2;
    s.line_style = LineStyle::Solid;
    s.edge_color = Some([0, 0, 0]);
    s.class_hint = Some("matlab.graphics.chart.primitive.Histogram2");
    let r = crate::builtins_graph3d::add_3d(it, vec![s], &[], 1)?;
    let id = r.as_ref().and_then(|v| v.first()).and_then(gfx::ids_of).and_then(|v| v.first().copied()).unwrap_or(0);
    let values = Value::Num(Matrix::new(nx, ny, counts));
    let data: Vec<f64> = x.data.iter().chain(&y.data).copied().collect();
    let widths = |e: &[f64]| e.windows(2).map(|w| w[1] - w[0]).fold(f64::NAN, |acc: f64, w| if acc.is_nan() || (acc - w).abs() < 1e-12 { w } else { f64::NAN });
    it.gfx_store.entry(id).or_default().extend([
        ("Data".to_string(), Value::Num(Matrix::new(x.numel(), 2, data))),
        ("Values".to_string(), values.clone()),
        ("BinCounts".to_string(), values),
        ("XBinEdges".to_string(), Value::Num(Matrix::row(ex.clone()))),
        ("YBinEdges".to_string(), Value::Num(Matrix::row(ey.clone()))),
        ("NumBins".to_string(), Value::Num(Matrix::row(vec![nx as f64, ny as f64]))),
        ("BinWidth".to_string(), Value::Num(Matrix::row(vec![widths(&ex), widths(&ey)]))),
        ("DisplayStyle".to_string(), Value::chars(&style)),
        ("Normalization".to_string(), Value::chars("count")),
    ]);
    if style == "tile" {
        it.call_function("view", vec![Value::num(2.0)], 0)?;
    }
    apply_props(it, &[id], &props)?;
    handles_out(it, &[id], nargout)
}

// ---- pie3, plotmatrix, surfl -----------------------------------------------------------------

fn pie3(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let x = arg(a, 0, "pie3")?.to_matrix()?;
    let explode: Vec<f64> = a.get(1).filter(|v| num(v)).map(|v| v.to_matrix()).transpose()?.map(|m| m.data).unwrap_or_default();
    let labels: Option<Vec<String>> = a.iter().skip(1).find(|v| matches!(v, Value::Cell(_) | Value::Str(_))).map(|v| match v {
        Value::Cell(c) => c.data.iter().map(|x| x.text().unwrap_or_default()).collect(),
        Value::Str(s) => s.data.iter().map(|x| x.clone().unwrap_or_default()).collect(),
        _ => Vec::new(),
    });
    let total: f64 = x.data.iter().sum();
    if !(total > 0.0) {
        return rt("Data must contain at least one positive value.");
    }
    let frac: Vec<f64> = x.data.iter().map(|&v| if total < 1.0 { v } else { v / total }).collect();
    let h = 0.35;
    let mut series = Vec::new();
    let mut t = std::f64::consts::FRAC_PI_2;
    for (k, &f) in frac.iter().enumerate() {
        let t1 = t + f * TWO_PI;
        let mid = (t + t1) / 2.0;
        let off = if explode.get(k).copied().unwrap_or(0.0) != 0.0 { 0.15 } else { 0.0 };
        let (cx, cy) = (off * mid.cos(), off * mid.sin());
        let n = ((f * 100.0).ceil() as usize).max(2);
        let arc: Vec<(f64, f64)> = (0..=n).map(|j| t + (t1 - t) * j as f64 / n as f64).map(|a| (cx + a.cos(), cy + a.sin())).collect();
        let face = |z: f64| {
            let mut s = Series::new(SeriesKind::Patch, std::iter::once(cx).chain(arc.iter().map(|p| p.0)).collect(), std::iter::once(cy).chain(arc.iter().map(|p| p.1)).collect());
            s.z = vec![z; arc.len() + 1];
            s.line_style = LineStyle::Solid;
            s.edge_color = Some([0, 0, 0]);
            s.c = vec![k as f64 + 1.0];
            s.face_mapped = true;
            s
        };
        series.push(face(0.0));
        // The curved wall: a 2-by-(n+1) surface.
        let m = arc.len();
        let mut wall = Series::new(SeriesKind::Surface, arc.iter().flat_map(|p| [p.0, p.0]).collect(), arc.iter().flat_map(|p| [p.1, p.1]).collect());
        wall.z = (0..m).flat_map(|_| [0.0, h]).collect();
        wall.grid = (2, m);
        wall.class_hint = Some("matlab.graphics.primitive.Surface");
        wall.c = vec![k as f64 + 1.0; 2 * m];
        wall.face_mapped = true;
        wall.edge_color = Some([0, 0, 0]);
        series.push(wall);
        series.push(face(h));
        let label = match &labels {
            Some(l) => l.get(k).cloned().unwrap_or_default(),
            None => {
                let pct = f * 100.0;
                if pct > 0.0 && pct < 1.0 { "< 1%".to_string() } else { format!("{}%", pct.round()) }
            }
        };
        let mut tx = Series::new(SeriesKind::Text, vec![cx + 1.2 * mid.cos()], vec![cy + 1.2 * mid.sin()]);
        tx.z = vec![h];
        tx.label = label;
        series.push(tx);
        t = t1;
    }
    let r = crate::builtins_graph3d::add_3d(it, series, &[], 1)?;
    let ids_: Vec<u64> = r.as_ref().and_then(|v| v.first()).and_then(gfx::ids_of).unwrap_or_default();
    it.call_function("view", vec![Value::num(3.0)], 0)?;
    it.call_function("axis", vec![Value::chars("equal")], 0)?;
    it.call_function("axis", vec![Value::chars("off")], 0)?;
    handles_out(it, &ids_, nargout)
}

fn plotmatrix(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let spec = a.iter().find(|v| is_textlike(v)).cloned().unwrap_or_else(|| Value::chars("."));
    let nums: Vec<Matrix> = a.iter().filter(|v| num(v)).map(|v| v.to_matrix()).collect::<R<_>>()?;
    let (x, y, square) = match nums.as_slice() {
        [x] => (x.clone(), x.clone(), true),
        [x, y, ..] => (x.clone(), y.clone(), false),
        _ => return rt("Not enough input arguments."),
    };
    let col = |m: &Matrix, j: usize| Value::Num(Matrix::col(m.data[j * m.rows()..(j + 1) * m.rows()].to_vec()));
    let (rows, cols) = (y.cols(), x.cols());
    let (mut s_ids, mut ax_ids, mut h_ids) = (vec![0u64; rows * cols], vec![0u64; rows * cols], Vec::new());
    for i in 0..rows {
        for j in 0..cols {
            let k = i * cols + j + 1;
            it.call_function("subplot", vec![Value::num(rows as f64), Value::num(cols as f64), Value::num(k as f64)], 0)?;
            let ax = it.current_axes_handle();
            let line = first(it, "plot", vec![col(&x, j), col(&y, i), spec.clone()])?;
            s_ids[j * rows + i] = ids(&line)[0];
            ax_ids[j * rows + i] = ids(&ax)[0];
            if square && i == j {
                // The diagonal shows the column's histogram over a hidden scatter line.
                it.call_function("set", vec![line, Value::chars("Visible"), Value::chars("off")], 0)?;
                it.call_function("hold", vec![Value::chars("on")], 0)?;
                let hh = first(it, "histogram", vec![col(&x, i)])?;
                h_ids.push(ids(&hh)[0]);
                it.call_function("hold", vec![Value::chars("off")], 0)?;
            }
        }
    }
    // The enclosing BigAx is not modelled (axes cannot be placed freely): an empty handle.
    let big = gfx::handles(it, &[]);
    let hax: Vec<u64> = if square { (0..rows).map(|i| ax_ids[i * rows + i]).collect() } else { Vec::new() };
    let outs = vec![
        handle_matrix(it, &s_ids, rows, cols),
        handle_matrix(it, &ax_ids, rows, cols),
        big,
        handle_matrix(it, &h_ids, 1, h_ids.len()),
        handle_matrix(it, &hax, 1, hax.len()),
    ];
    if nargout == 0 {
        return none();
    }
    Ok(Some(outs.into_iter().take(nargout).collect()))
}

fn gradient2(it: &mut Interpreter, f: &Matrix, x: &[f64], y: &[f64]) -> R<(Matrix, Matrix)> {
    let r = it.call_function("gradient", vec![Value::Num(f.clone()), Value::Num(Matrix::row(x.to_vec())), Value::Num(Matrix::row(y.to_vec()))], 2)?;
    Ok((r[0].to_matrix()?, r[1].to_matrix()?))
}

fn surfl(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let nums: Vec<Matrix> = a.iter().filter(|v| num(v)).map(|v| v.to_matrix()).collect::<R<_>>()?;
    let (x, y, z, light) = match nums.as_slice() {
        [z] => (None, None, z.clone(), None),
        [z, s] if s.numel() <= 3 => (None, None, z.clone(), Some(s.clone())),
        [x, y, z] => (Some(x.clone()), Some(y.clone()), z.clone(), None),
        [x, y, z, s, ..] => (Some(x.clone()), Some(y.clone()), z.clone(), Some(s.clone())),
        _ => return rt("Not enough input arguments."),
    };
    let (m, n) = (z.rows(), z.cols());
    let x = x.unwrap_or_else(|| Matrix::new(m, n, (0..m * n).map(|k| (k / m + 1) as f64).collect()));
    let y = y.unwrap_or_else(|| Matrix::new(m, n, (0..m * n).map(|k| (k % m + 1) as f64).collect()));
    let idx_x: Vec<f64> = (1..=n).map(|v| v as f64).collect();
    let idx_y: Vec<f64> = (1..=m).map(|v| v as f64).collect();
    let (xu, xv) = gradient2(it, &x, &idx_x, &idx_y)?;
    let (yu, yv) = gradient2(it, &y, &idx_x, &idx_y)?;
    let (zu, zv) = gradient2(it, &z, &idx_x, &idx_y)?;
    // Light 45 degrees counterclockwise from the view direction (azimuth -37.5, elevation 30).
    let (az, el) = match &light {
        Some(s) if s.numel() == 2 => (s.data[0], s.data[1]),
        _ => (-37.5 - 45.0, 30.0),
    };
    let (azr, elr) = (az.to_radians(), el.to_radians());
    let l = match &light {
        Some(s) if s.numel() == 3 => {
            let nrm = (s.data[0].powi(2) + s.data[1].powi(2) + s.data[2].powi(2)).sqrt();
            [s.data[0] / nrm, s.data[1] / nrm, s.data[2] / nrm]
        }
        _ => [elr.cos() * azr.sin(), -elr.cos() * azr.cos(), elr.sin()],
    };
    let (ka, kd, ks, spread) = (0.55, 0.6, 0.4, 10.0);
    let refl: Vec<f64> = (0..m * n)
        .map(|k| {
            let nv = [yu.data[k] * zv.data[k] - zu.data[k] * yv.data[k], zu.data[k] * xv.data[k] - xu.data[k] * zv.data[k], xu.data[k] * yv.data[k] - yu.data[k] * xv.data[k]];
            let len = (nv[0] * nv[0] + nv[1] * nv[1] + nv[2] * nv[2]).sqrt().max(f64::MIN_POSITIVE);
            let nn = [-nv[0] / len, -nv[1] / len, -nv[2] / len];
            let d = (nn[0] * l[0] + nn[1] * l[1] + nn[2] * l[2]).max(0.0);
            // Specular term toward the viewer.
            let v = [l[0], l[1], l[2]];
            let r = [2.0 * d * nn[0] - l[0], 2.0 * d * nn[1] - l[1], 2.0 * d * nn[2] - l[2]];
            let sp = (r[0] * v[0] + r[1] * v[1] + r[2] * v[2]).max(0.0).powf(spread);
            ka + kd * d + ks * sp
        })
        .collect();
    let h = first(it, "surf", vec![Value::Num(x), Value::Num(y), Value::Num(z), Value::Num(Matrix::new(m, n, refl))])?;
    handles_out(it, &ids(&h), nargout)
}

// ---- curl and divergence ---------------------------------------------------------------------

/// Field arguments: (X, Y, [Z,] U, V, [W]) or (U, V, [W]) with unit spacing.
fn field(a: &[Value], name: &str) -> R<(Vec<Matrix>, Vec<Vec<f64>>)> {
    let ms: Vec<Matrix> = a.iter().map(|v| v.to_matrix()).collect::<R<_>>()?;
    let (coords, comps): (Vec<Matrix>, Vec<Matrix>) = match ms.len() {
        2 | 3 => (Vec::new(), ms),
        4 => (ms[..2].to_vec(), ms[2..].to_vec()),
        6 => (ms[..3].to_vec(), ms[3..].to_vec()),
        _ => return rt(format!("Wrong number of input arguments for {name}.")),
    };
    let u = &comps[0];
    let dims = u.dims.clone();
    let (r, c, p) = (dims[0], dims.get(1).copied().unwrap_or(1), dims.get(2).copied().unwrap_or(1));
    let spacing: Vec<Vec<f64>> = if coords.is_empty() {
        vec![(1..=c).map(|v| v as f64).collect(), (1..=r).map(|v| v as f64).collect(), (1..=p).map(|v| v as f64).collect()]
    } else {
        let x = &coords[0];
        let y = &coords[1];
        let mut s = vec![(0..c).map(|j| x.data[j * r]).collect(), (0..r).map(|i| y.data[i]).collect()];
        if let Some(z) = coords.get(2) {
            s.push((0..p).map(|k| z.data[k * r * c]).collect());
        }
        s
    };
    Ok((comps, spacing))
}

/// Partial derivatives of f along x, y (and z for 3-D fields).
fn partials(it: &mut Interpreter, f: &Matrix, sp: &[Vec<f64>], three: bool) -> R<Vec<Matrix>> {
    let mut args = vec![Value::Num(f.clone()), Value::Num(Matrix::row(sp[0].clone())), Value::Num(Matrix::row(sp[1].clone()))];
    if three {
        args.push(Value::Num(Matrix::row(sp[2].clone())));
    }
    it.call_function("gradient", args, if three { 3 } else { 2 })?.iter().map(|v| v.to_matrix()).collect()
}

fn zip3(a: &Matrix, f: impl Fn(usize) -> f64) -> Matrix {
    Matrix::from_dims(a.dims.clone(), (0..a.numel()).map(f).collect())
}

fn curl(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let (c, sp) = field(a, "curl")?;
    if c.len() == 2 {
        let du = partials(it, &c[0], &sp, false)?;
        let dv = partials(it, &c[1], &sp, false)?;
        let cz = zip3(&c[0], |k| dv[0].data[k] - du[1].data[k]);
        let cav = zip3(&c[0], |k| 0.5 * cz.data[k]);
        return Ok(Some(vec![Value::Num(cz), Value::Num(cav)]));
    }
    let (du, dv, dw) = (partials(it, &c[0], &sp, true)?, partials(it, &c[1], &sp, true)?, partials(it, &c[2], &sp, true)?);
    let cx = zip3(&c[0], |k| dw[1].data[k] - dv[2].data[k]);
    let cy = zip3(&c[0], |k| du[2].data[k] - dw[0].data[k]);
    let cz = zip3(&c[0], |k| dv[0].data[k] - du[1].data[k]);
    let cav = zip3(&c[0], |k| {
        let (u, v, w) = (c[0].data[k], c[1].data[k], c[2].data[k]);
        0.5 * (cx.data[k] * u + cy.data[k] * v + cz.data[k] * w) / (u * u + v * v + w * w).sqrt()
    });
    if nargout <= 1 {
        return one(Value::Num(cav));
    }
    Ok(Some(vec![Value::Num(cx), Value::Num(cy), Value::Num(cz), Value::Num(cav)]))
}

fn divergence(it: &mut Interpreter, a: &[Value]) -> Ret {
    let (c, sp) = field(a, "divergence")?;
    let three = c.len() == 3;
    let mut total = zip3(&c[0], |_| 0.0);
    for (k, comp) in c.iter().enumerate() {
        let d = partials(it, comp, &sp, three)?;
        for (t, v) in total.data.iter_mut().zip(&d[k].data) {
            *t += v;
        }
    }
    one(Value::Num(total))
}

// ---- frames ------------------------------------------------------------------------------------

fn frame(cdata: Value, colormap: Value) -> Value {
    let mut s = StructArray::scalar();
    s.set("cdata", cdata);
    s.set("colormap", colormap);
    Value::Struct(s)
}

fn getframe(it: &mut Interpreter, a: &[Value]) -> Ret {
    let f = match a.first().and_then(gfx::ids_of).and_then(|i| i.first().copied()).and_then(|id| gfx::resolve(it, id)) {
        Some(Target::Figure(f) | Target::Axes(f, _)) => f,
        _ => it.current_figure_index(),
    };
    let fig = it.figures[f].clone();
    let Some(exporter) = it.exporter.as_mut() else {
        return rt("getframe needs the matlivre editor's renderer, which is not available here.");
    };
    let png = exporter(&fig, ExportFormat::Png, 1.0).map_err(|e| LangError::new(e, 0))?;
    let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png).map_err(|e| LangError::new(e.to_string(), 0))?.to_rgb8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut data = vec![0.0; w * h * 3];
    for (px, py, p) in img.enumerate_pixels() {
        for c in 0..3 {
            data[c * w * h + px as usize * h + py as usize] = p.0[c] as f64;
        }
    }
    let cdata = Matrix::from_dims(vec![h, w, 3], data).cast_to(Kind::Int(IntClass::U8));
    one(frame(Value::Num(cdata), Value::empty()))
}

fn im2frame(a: &[Value]) -> Ret {
    let x = arg(a, 0, "im2frame")?.to_matrix()?;
    match a.get(1) {
        Some(map) => {
            // Indexed images: 1-based double indices become 0-based uint8.
            let idx = if x.kind == Kind::Double { Matrix::from_dims(x.dims.clone(), x.data.iter().map(|v| v - 1.0).collect()) } else { x.clone() };
            one(frame(Value::Num(idx.cast_to(Kind::Int(IntClass::U8))), map.clone()))
        }
        None => {
            if x.dims.get(2).copied().unwrap_or(1) != 3 {
                return rt("Truecolor images must be m-by-n-by-3; give a colormap for indexed images.");
            }
            let c = if x.kind == Kind::Double { Matrix::from_dims(x.dims.clone(), x.data.iter().map(|v| (v * 255.0).round()).collect()) } else { x.clone() };
            one(frame(Value::Num(c.cast_to(Kind::Int(IntClass::U8))), Value::empty()))
        }
    }
}

fn frame_parts(v: &Value) -> R<(Value, Value)> {
    let Value::Struct(s) = v else { return rt("Input must be a movie frame structure.") };
    let get = |n: &str| s.elems.first().and_then(|e| s.fields.iter().position(|f| f == n).map(|i| e[i].clone()));
    Ok((get("cdata").ok_or_else(|| LangError::new("Input must be a movie frame structure.", 0))?, get("colormap").unwrap_or_else(Value::empty)))
}

fn movie(it: &mut Interpreter, a: &[Value]) -> Ret {
    let a = crate::builtins_plot::select_axes(it, a)?;
    let Value::Struct(m) = arg(a, 0, "movie")? else { return rt("Input must be a movie frame structure.") };
    if m.elems.is_empty() {
        return none();
    }
    // Frames cannot animate in a live script: the last frame stays on screen.
    let last = Value::Struct(m.element(m.elems.len() - 1));
    let (cdata, map) = frame_parts(&last)?;
    it.call_function("image", vec![cdata], 0)?;
    if map.numel() > 0 {
        it.call_function("colormap", vec![map], 0)?;
    }
    it.call_function("axis", vec![Value::chars("off")], 0)?;
    none()
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "surfnorm" => surfnorm(it, a, nargout),
        "xsecondarylabel" | "ysecondarylabel" | "zsecondarylabel" => secondary_label(it, name, a),
        "ruler2num" | "num2ruler" => ruler_convert(it, name, a),
        "ezplot" => ezplot(it, a, nargout),
        "ezplot3" => ezplot3(it, a, nargout),
        "ezsurf" | "ezsurfc" | "ezmesh" | "ezmeshc" | "ezcontour" | "ezcontourf" => ez_grid(it, name, a, nargout),
        "fimplicit" => fimplicit(it, a, nargout),
        "fimplicit3" => fimplicit3(it, a, nargout),
        "animatedline" => animatedline(it, a, nargout),
        "addpoints" | "getpoints" | "clearpoints" => points(it, name, a),
        "histogram2" => histogram2(it, a, nargout),
        "pie3" => pie3(it, a, nargout),
        "plotmatrix" => plotmatrix(it, a, nargout),
        "surfl" => surfl(it, a, nargout),
        "curl" => curl(it, a, nargout),
        "divergence" => divergence(it, a),
        "getframe" => getframe(it, a),
        "frame2im" => {
            let (c, map) = frame_parts(arg(a, 0, name)?)?;
            Ok(Some(vec![c, map]))
        }
        "im2frame" => im2frame(a),
        "movie" => movie(it, a),
        _ => Ok(None),
    }
}
