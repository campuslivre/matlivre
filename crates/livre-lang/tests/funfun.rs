//! Root finding, minimization, quadrature and ODE solvers against MATLAB R2023b's documented
//! results and analytic values.

use livre_lang::{Interpreter, OutputKind, Value};

fn run(src: &str) -> (Interpreter, Vec<String>) {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    let out = it
        .take_outputs()
        .into_iter()
        .map(|o| match o.kind {
            OutputKind::Text(t) => t,
            OutputKind::Value { text, .. } => text,
            OutputKind::Error(e) => format!("Error: {e}"),
            OutputKind::Warning(w) => format!("Warning: {w}"),
            OutputKind::Figure(_) => String::new(),
        })
        .collect();
    (it, out)
}

fn num(it: &Interpreter, name: &str) -> Vec<f64> {
    match it.get_var(name) {
        Some(Value::Num(m)) => m.data.clone(),
        other => panic!("{name} = {other:?}"),
    }
}

fn dims(it: &Interpreter, name: &str) -> Vec<usize> {
    it.get_var(name).unwrap().dims()
}

fn close(it: &Interpreter, name: &str, want: &[f64], tol: f64) {
    let got = num(it, name);
    assert_eq!(got.len(), want.len(), "{name}: {got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() <= tol, "{name}: got {got:?}, want {want:?}");
    }
}

fn no_errors(out: &[String]) {
    assert!(!out.iter().any(|o| o.starts_with("Error") || o.starts_with("Warning")), "{out:?}");
}

#[test]
fn fzero_fminbnd_fminsearch_match_matlab() {
    let (it, out) = run(concat!(
        "x1 = fzero(@cos, 1); x2 = fzero(@(x) x.^3 - 2*x - 5, 2); x3 = fzero('sin', 3);\n",
        "[x4, fv, ef, o] = fzero(@sin, [3 4]);\n",
        "[x5, fv5, ef5, o5] = fzero(@(x) x.^2 - 2, 1);\n",
        "x6 = fzero(@(x, c) x - c, 0, [], 7);\n",
        "[xm, fm, efm, om] = fminbnd(@(x) (x - 2).^2 + 1, 0, 5); xc = fminbnd(@cos, 3, 4);\n",
        "[xs, fs, efs, os] = fminsearch(@(x) 100*(x(2) - x(1)^2)^2 + (1 - x(1))^2, [-1.2 1]);\n",
        "xs2 = fminsearch(@(x) sum((x - [1 2; 3 4]).^2, 'all'), zeros(2));\n",
        "xs3 = fminsearch(@(x, a) (x - a)^2, 0, optimset('TolX', 1e-10, 'TolFun', 1e-10), 3);\n",
    ));
    no_errors(&out);
    close(&it, "x1", &[std::f64::consts::FRAC_PI_2], 1e-15);
    close(&it, "x2", &[2.0945514815423265], 1e-14);
    close(&it, "x3", &[std::f64::consts::PI], 1e-15);
    close(&it, "x4", &[std::f64::consts::PI], 1e-15);
    close(&it, "fv", &[1.2246467991473532e-16], 1e-30);
    close(&it, "ef", &[1.0], 0.0);
    let o = it.get_var("o").unwrap().clone();
    let Value::Struct(o) = o else { panic!() };
    assert_eq!(o.fields, ["intervaliterations", "iterations", "funcCount", "algorithm", "message"]);
    assert_eq!(o.get("message").unwrap().text().unwrap(), "Zero found in the interval [3, 4]");
    assert_eq!(o.get("algorithm").unwrap().text().unwrap(), "bisection, interpolation");
    close(&it, "x5", &[2f64.sqrt()], 1e-15);
    close(&it, "x6", &[7.0], 1e-15);
    close(&it, "xm", &[2.0], 1e-6);
    close(&it, "fm", &[1.0], 1e-10);
    close(&it, "efm", &[1.0], 0.0);
    close(&it, "xc", &[std::f64::consts::PI], 1e-4);
    close(&it, "xs", &[1.0, 1.0], 1e-4);
    close(&it, "efs", &[1.0], 0.0);
    assert_eq!(dims(&it, "xs"), [1, 2]);
    assert_eq!(dims(&it, "xs2"), [2, 2]);
    close(&it, "xs2", &[1.0, 3.0, 2.0, 4.0], 1e-3);
    close(&it, "xs3", &[3.0], 1e-8);
    let Some(Value::Struct(os)) = it.get_var("os") else { panic!() };
    assert_eq!(os.get("algorithm").unwrap().text().unwrap(), "Nelder-Mead simplex direct search");
}

#[test]
fn solver_errors_and_limits() {
    let (_, out) = run("fzero(@(x) x.^2 + 1, [0 1])");
    assert_eq!(out, ["Error: The function values at the interval endpoints must differ in sign."]);
    let (it, out) = run("[x, fv, ef] = fminsearch(@(x) x^2, 5, optimset('MaxFunEvals', 10));");
    assert!(out[0].starts_with("Exiting: Maximum number of function evaluations has been exceeded"), "{out:?}");
    close(&it, "ef", &[0.0], 0.0);
    let (it, out) = run("[x, fv, ef] = fminsearch(@(x) x^2, 5, optimset('MaxFunEvals', 10, 'Display', 'off'));");
    assert!(out.is_empty(), "{out:?}");
    close(&it, "ef", &[0.0], 0.0);
}

#[test]
fn option_structures() {
    let (it, out) = run(concat!(
        "o = optimset('tolx', 1e-8, 'Disp', 'off'); t = optimget(o, 'TolX'); d = optimget(o, 'MaxIter', 42);\n",
        "o2 = optimset(o, 'TolX', 1e-3); t2 = o2.TolX; f = fieldnames(optimset);\n",
        "z = optimset('fzero'); zt = z.TolX; s = odeset('RelTol', 1e-4, 'abstol', [1e-6 1e-7]);\n",
        "r = odeget(s, 'RelTol'); m = odeget(s, 'MaxStep', 0.5); nf = numel(fieldnames(odeset));\n",
    ));
    no_errors(&out);
    close(&it, "t", &[1e-8], 0.0);
    close(&it, "d", &[42.0], 0.0);
    close(&it, "t2", &[1e-3], 0.0);
    close(&it, "zt", &[f64::EPSILON], 0.0);
    close(&it, "r", &[1e-4], 0.0);
    close(&it, "m", &[0.5], 0.0);
    close(&it, "nf", &[22.0], 0.0);
    let Some(Value::Cell(f)) = it.get_var("f") else { panic!() };
    let names: Vec<String> = f.data.iter().map(|v| v.text().unwrap()).collect();
    assert_eq!(names, ["Display", "FunValCheck", "MaxFunEvals", "MaxIter", "OutputFcn", "PlotFcns", "TolFun", "TolX"]);
    let (_, out) = run("odeset('M', 1)");
    assert_eq!(out, ["Error: Ambiguous property name 'M' (Mass, MassSingular, MaxOrder, MaxStep, MStateDependence, MvPattern)."]);
    let (_, out) = run("optimset('Foo', 1)");
    assert_eq!(out, ["Error: Unrecognized parameter name 'Foo'."]);
}

#[test]
fn lsqnonneg_matches_matlab() {
    let (it, out) = run(concat!(
        "C = [0.0372 0.2869; 0.6861 0.7071; 0.6233 0.6245; 0.6344 0.6170]; d = [0.8587; 0.1781; 0.0747; 0.8405];\n",
        "[x, resnorm, residual, ef] = lsqnonneg(C, d); rn = resnorm - sum((d - C*x).^2); r = residual - (d - C*x);\n",
        "x\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    close(&it, "x", &[0.0, 0.692934397130293], 1e-14);
    close(&it, "resnorm", &[0.831455951263312], 1e-14);
    close(&it, "rn", &[0.0], 1e-15);
    close(&it, "r", &[0.0; 4], 1e-15);
    close(&it, "ef", &[1.0], 0.0);
    assert_eq!(out.last().unwrap(), "x = 2×1\n\n        0\n   0.6929");
}

#[test]
fn integrals_match_matlab() {
    let (it, out) = run(concat!(
        "q1 = integral(@(x) exp(-x.^2), -Inf, Inf); q2 = integral(@(x) 1./((x+1).*sqrt(x)), 0, Inf);\n",
        "q3 = integral(@(x) log(x), 0, 1); q3b = integral(@log, 0, 1, 'RelTol', 0, 'AbsTol', 1e-12); q3c = integral(@(x) exp(-x.^2).*log(x).^2, 0, Inf);
",
        "q4 = integral(@(x) exp(x), -Inf, 0); q5 = integral(@sin, pi, 0);\n",
        "q6 = integral(@(x) abs(x - 1/3), 0, 1, 'Waypoints', 1/3); q7 = integral(@(x) [x; x.^2], 0, 1, 'ArrayValued', true);\n",
        "q8 = integral(@(x) exp(1i*x), 0, pi, 'RelTol', 1e-12); [q9, e9] = quadgk(@(x) exp(x).*log(x), 0, 1);\n",
        "q10 = quad(@(x) 1./(x.^3 - 2*x - 5), 0, 2); [q11, n11] = quadl(@(x) 1./(x.^3 - 2*x - 5), 0, 2);\n",
        "q12 = quadv(@(x) [x x.^2], 0, 1); q13 = quad(@(x, k) x.^k, 0, 1, [], [], 3);\n",
        "q14 = integral2(@(x, y) x.*y, 0, 1, 0, @(x) x); q15 = integral2(@(x, y) exp(-x.^2 - y.^2), -Inf, Inf, -Inf, Inf);\n",
        "q16 = integral3(@(x, y, z) x + y + z, 0, 1, 0, 1, 0, 1); q17 = quad2d(@(x, y) x.^2 + y, 0, 1, 0, 2);\n",
        "q18 = dblquad(@(x, y) y.*sin(x) + x.*cos(y), pi, 2*pi, 0, pi); q19 = triplequad(@(x, y, z) y.*sin(x) + z.*cos(x), 0, pi, 0, 1, -1, 1);\n",
    ));
    no_errors(&out);
    let pi = std::f64::consts::PI;
    close(&it, "q1", &[pi.sqrt()], 1e-9);
    close(&it, "q2", &[pi], 1e-9);
    // The values MATLAB's documentation shows for these examples.
    close(&it, "q3", &[-1.000000010959678], 1e-15);
    close(&it, "q3b", &[-1.000000000000010], 1e-15);
    close(&it, "q3c", &[1.947522220295560], 1e-4);
    close(&it, "q4", &[1.0], 1e-10);
    close(&it, "q5", &[-2.0], 1e-12);
    close(&it, "q6", &[5.0 / 18.0], 1e-14);
    close(&it, "q7", &[0.5, 1.0 / 3.0], 1e-12);
    assert_eq!(dims(&it, "q7"), [2, 1]);
    let Some(Value::Num(q8)) = it.get_var("q8") else { panic!() };
    assert!(q8.data[0].abs() < 1e-12 && (q8.im.as_ref().unwrap()[0] - 2.0).abs() < 1e-12, "{q8:?}");
    close(&it, "q9", &[-1.3179021514544038], 1e-6);
    close(&it, "q10", &[-0.4605015338467329], 1e-6);
    close(&it, "q11", &[-0.4605015338467329], 1e-6);
    close(&it, "q12", &[0.5, 1.0 / 3.0], 1e-6);
    close(&it, "q13", &[0.25], 1e-6);
    close(&it, "q14", &[0.125], 1e-10);
    close(&it, "q15", &[pi], 1e-6);
    close(&it, "q16", &[1.5], 1e-10);
    close(&it, "q17", &[2.0 / 3.0 + 2.0], 1e-6);
    close(&it, "q18", &[-pi * pi], 1e-5);
    close(&it, "q19", &[2.0], 1e-5);
    let (_, out) = run("integral(@(x) 5, 0, 1)");
    assert_eq!(out, ["Error: Output of the function must be the same size as the input. If FUN is an array-valued integrand, set the 'ArrayValued' option to true."]);
}

#[test]
fn ode45_and_ode23_match_matlab() {
    let (it, out) = run(concat!(
        "[t, y] = ode45(@(t, y) -y, [0 1], 1); n45 = size(t); e45 = y(end) - exp(-1);\n",
        "[t2, y2] = ode23(@(t, y) -y, [0 1], 1); e23 = y2(end) - exp(-1);\n",
        "[t3, y3] = ode45(@(t, y) [y(2); -y(1)], [0 pi/2 pi], [0; 1]);\n",
        "opts = odeset('RelTol', 1e-8, 'AbsTol', 1e-10);\n",
        "[t4, y4] = ode45(@(t, y) [y(2); -y(1)], [0 10], [0; 1], opts); e4 = y4(end, 1) - sin(10);\n",
        "[t5, y5] = ode45(@(t, y, k) -k*y, [1 0], 1, [], 2); e5 = y5(end) - exp(2);\n",
        "sol = ode45(@(t, y) -y, [0 2], 1); [s, sp] = deval(sol, [0.5 1 1.5]); s2 = deval([0 2], sol);\n",
        "sol23 = ode23(@(t, y) [y(2); -y(1)], [0 1], [0; 1]); s3 = deval(sol23, 0.5, 1);\n",
    ));
    no_errors(&out);
    close(&it, "n45", &[41.0, 1.0], 0.0);
    close(&it, "e45", &[0.0], 1e-6);
    close(&it, "e23", &[0.0], 1e-4);
    close(&it, "t3", &[0.0, std::f64::consts::FRAC_PI_2, std::f64::consts::PI], 1e-15);
    close(&it, "y3", &[0.0, 1.0, 0.0, 1.0, 0.0, -1.0], 1e-4);
    close(&it, "e4", &[0.0], 1e-7);
    close(&it, "e5", &[0.0], 1e-4);
    let want: Vec<f64> = [0.5f64, 1.0, 1.5].iter().map(|t| (-t).exp()).collect();
    close(&it, "s", &want, 1e-5);
    close(&it, "sp", &want.iter().map(|v| -v).collect::<Vec<_>>(), 1e-4);
    close(&it, "s2", &[1.0, (-2f64).exp()], 1e-5);
    close(&it, "s3", &[0.5f64.sin()], 1e-3);
    let Some(Value::Struct(sol)) = it.get_var("sol") else { panic!() };
    assert_eq!(sol.fields, ["solver", "extdata", "x", "y", "stats", "idata"]);
}

#[test]
fn ode_events_stop_the_integration() {
    // A ball dropped from 10 m hits the ground at sqrt(2*10/9.81).
    let (it, out) = run(concat!(
        "function [v, term, dir] = ground(t, y)\n  v = y(1); term = 1; dir = -1;\nend\n",
        "opts = odeset('Events', @ground, 'RelTol', 1e-8, 'AbsTol', 1e-10);\n",
        "[t, y, te, ye, ie] = ode45(@(t, y) [y(2); -9.81], [0 5], [10; 0], opts);\n",
        "tend = t(end);\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    let hit = (20.0f64 / 9.81).sqrt();
    close(&it, "te", &[hit], 1e-8);
    close(&it, "tend", &[hit], 1e-8);
    close(&it, "ie", &[1.0], 0.0);
    close(&it, "ye", &[0.0, -9.81 * hit], 1e-6);
}
