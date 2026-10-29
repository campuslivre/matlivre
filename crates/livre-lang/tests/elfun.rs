//! Elementary math against values recorded from MATLAB R2023b (printed with %.17g).

use livre_lang::{Interpreter, OutputKind, Value};

fn run(src: &str) -> (Interpreter, Vec<String>) {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    let errors = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Error(e) => Some(e),
            _ => None,
        })
        .collect();
    (it, errors)
}

/// (real, imaginary) parts of a variable.
fn parts(it: &Interpreter, name: &str) -> Vec<(f64, f64)> {
    match it.get_var(name) {
        Some(Value::Num(m)) => (0..m.numel()).map(|i| (m.data[i], m.im_at(i))).collect(),
        other => panic!("{name} = {other:?}"),
    }
}

fn same(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan()) || (a - b).abs() <= 4.0 * f64::EPSILON * a.abs().max(b.abs())
}

fn check(it: &Interpreter, name: &str, want: &[(f64, f64)]) {
    let got = parts(it, name);
    assert_eq!(got.len(), want.len(), "{name}: {got:?}");
    for (k, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(same(g.0, w.0) && same(g.1, w.1), "{name}({}) = {g:?}, MATLAB {w:?}", k + 1);
    }
}

fn real(v: &[f64]) -> Vec<(f64, f64)> {
    v.iter().map(|&x| (x, 0.0)).collect()
}

const INF: f64 = f64::INFINITY;

#[test]
fn reciprocal_functions_and_degree_variants() {
    let (it, errors) = run(concat!(
        "a1 = tand([90 -90 270 180 45]);\n",
        "a2 = cotd([0 90 180 -90 45 -0]);\n",
        "a3 = secd([90 270 0 60]);\n",
        "a4 = cscd([0 180 -180 90 30]);\n",
        "a5 = cot([0 -0]); a6 = csch([0 -0]); a7 = coth(0);\n",
        "a8 = sec(1+2i);\n",
        "a9 = atan2d([1 -1 0], [0 -1 -1]);\n",
        "a10 = acot([0 -0 1 Inf]);\n",
        "a11 = acsch([0 1 -2]);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check(&it, "a1", &real(&[INF, -INF, -INF, -0.0, 0.99999999999999989]));
    check(&it, "a2", &real(&[INF, 0.0, -INF, -0.0, 1.0000000000000002, -INF]));
    check(&it, "a3", &real(&[INF, INF, 1.0, 1.9999999999999996]));
    check(&it, "a4", &real(&[INF, INF, -INF, 1.0, 2.0000000000000004]));
    check(&it, "a5", &real(&[INF, -INF]));
    check(&it, "a6", &real(&[INF, -INF]));
    check(&it, "a7", &real(&[INF]));
    // MATLAB printed this one to 15 digits: 0.151176298265577 + 0.226973675393722i.
    check(&it, "a8", &[(0.15117629826557727, 0.22697367539372162)]);
    check(&it, "a9", &real(&[90.0, -135.0, 180.0]));
    check(&it, "a10", &real(&[1.5707963267948966, -1.5707963267948966, 0.78539816339744828, 0.0]));
    check(&it, "a11", &real(&[INF, 0.88137358701954305, -0.48121182505960347]));
    // The sign of sind at -180 shows through cscd.
    let (it, _) = run("z = 1 / sind(-180);");
    check(&it, "z", &real(&[-INF]));
}

#[test]
fn inverse_reciprocal_functions_take_matlab_branches() {
    let (it, errors) = run(concat!(
        "b1 = asec([0 0.5 2 -2 1]);\n",
        "b2 = acsc([0 0.5 2 -1]);\n",
        "b3 = asech([0 2 -1 0.5 1]);\n",
        "b4 = acoth([0 0.5 1 2 -1 1e-20]);\n",
        "b5 = asecd([0.5 2]);\n",
        "b6 = acotd([0 1]);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check(&it, "b1", &[(0.0, INF), (0.0, 1.3169578969248166), (1.0471975511965979, 0.0), (2.0943951023931957, 0.0), (0.0, 0.0)]);
    check(&it, "b2", &[(1.5707963267948966, -INF), (1.5707963267948966, -1.3169578969248166), (0.52359877559829893, 0.0), (-1.5707963267948966, 0.0)]);
    check(&it, "b3", &[(INF, 0.0), (0.0, 1.0471975511965979), (0.0, 3.1415926535897931), (1.3169578969248166, 0.0), (0.0, 0.0)]);
    check(
        &it,
        "b4",
        &[(0.0, 1.5707963267948966), (0.54930614433405489, 1.5707963267948966), (INF, 0.0), (0.54930614433405478, 0.0), (-INF, 0.0), (1e-20, 1.5707963267948966)],
    );
    check(&it, "b5", &[(0.0, 75.456129290216893), (60.000000000000007, 0.0)]);
    check(&it, "b6", &real(&[90.0, 45.0]));
}

#[test]
fn base_functions_are_exact_inside_their_real_domain() {
    // One element outside the domain makes the result complex; the others stay exact.
    let (it, _) = run("c1 = acos([0.5 2]); c2 = atanh([2 45 -2]); c3 = acosh([0.5 -2]); c4 = log([-1 1]);");
    check(&it, "c1", &[(1.0471975511965979, 0.0), (0.0, 1.3169578969248166)]);
    check(&it, "c2", &[(0.54930614433405489, 1.5707963267948966), (0.022225881285416919, 1.5707963267948966), (-0.54930614433405489, -1.5707963267948966)]);
    check(&it, "c3", &[(0.0, 1.0471975511965979), (1.3169578969248166, 3.1415926535897931)]);
    check(&it, "c4", &[(0.0, 3.1415926535897931), (0.0, 0.0)]);
}

#[test]
fn powers_logs_and_phase() {
    let (it, errors) = run(concat!(
        "d1 = nextpow2([0 1 3 -5 1024 Inf 0.3 NaN]);\n",
        "d2 = pow2([1 2], [3 4]); d3 = pow2([0.5 3]);\n",
        "d4 = realpow([2 3], 2);\n",
        "d5 = unwrap([0 3 6.5 -3 0]);\n",
        "d6 = unwrap([0 1 2; 4 5 9]);\n",
        "d7 = unwrap([0 4 8], 5);\n",
        "d8 = unwrap([-2 -1 -0.5 0 0.5 1 2 3 -Inf Inf 1e-20 0.3 45 60 90]);\n",
        "d9 = cplxpair([1+2i 3 1-2i -1]);\n",
        "c = class(nextpow2(int8(5))); n = nextpow2(int8(5));\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check(&it, "d1", &real(&[0.0, 0.0, 2.0, 3.0, 10.0, INF, -1.0, f64::NAN]));
    check(&it, "d2", &real(&[8.0, 32.0]));
    check(&it, "d3", &real(&[1.4142135623730951, 8.0]));
    check(&it, "d4", &real(&[4.0, 9.0]));
    check(&it, "d5", &real(&[0.0, 3.0, 0.21681469282041377, 3.2831853071795862, 6.2831853071795862]));
    check(&it, "d6", &real(&[0.0, -2.2831853071795862, 1.0, -1.2831853071795862, 2.0, 2.7168146928204138]));
    check(&it, "d7", &real(&[0.0, 4.0, 8.0]));
    check(&it, "d8", &real(&[-2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, 3.0, -INF, INF, 1e-20, 0.3, 1.0177028497428964, 3.4513322353837239, 2.0354056994857928]));
    check(&it, "d9", &[(1.0, -2.0), (1.0, 2.0), (-1.0, 0.0), (3.0, 0.0)]);
    assert_eq!(it.get_var("c").unwrap().text().unwrap(), "int8");
    check(&it, "n", &real(&[3.0]));
}

#[test]
fn errors_match_matlab() {
    for (src, msg) in [
        ("realsqrt(-1)", "Realsqrt produced complex result."),
        ("reallog(-1)", "Reallog produced complex result."),
        ("realpow(-8, 1/3)", "Realpow produced complex result."),
        ("cplxpair([1+2i 1-3i])", "Complex numbers can't be paired."),
        ("cot(int8(1))", "Undefined function 'tan' for input arguments of type 'int8'."),
    ] {
        let (_, errors) = run(src);
        assert_eq!(errors, [msg], "{src}");
    }
}

#[test]
fn one_argument_builtins_accept_integers_and_func2str_is_bare() {
    let (it, errors) = run("c = class(int8(5)); n = numel(uint16([1 2 3])); s = func2str(@sin); t = func2str(@(x) x + 1);");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(it.get_var("c").unwrap().text().unwrap(), "int8");
    check(&it, "n", &real(&[3.0]));
    assert_eq!(it.get_var("s").unwrap().text().unwrap(), "sin");
    assert_eq!(it.get_var("t").unwrap().text().unwrap(), "@(x)x+1");
}
