//! str2sym, isolate, equationsToMatrix, vpasolve, partfrac, laplace/ilaplace, sums and
//! number theory against MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind};

/// The displayed text of each statement.
fn shown(src: &str) -> Vec<String> {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    it.take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Value { text, .. } => Some(text),
            OutputKind::Error(e) => Some(format!("Error: {e}")),
            _ => None,
        })
        .collect()
}

fn check(src: &str, expected: &[&str]) {
    let out = shown(src);
    assert_eq!(out, expected, "{src}");
}

#[test]
fn parsing_isolating_and_matrices() {
    check(
        "syms x y a b\ne1 = str2sym('x^2 + 2*x + 1')\ne2 = str2sym('sin(x) == 1/2')\ni1 = isolate(x^2 + y == 3, y)\ni2 = isolate(a*x + b == 0, x)\ni3 = isolate(exp(x) == 5, x)\ni4 = isolate(2*x + 3 == y, x)\n",
        &["e1 = x^2 + 2*x + 1", "e2 = sin(x) == 1/2", "i1 = y == 3 - x^2", "i2 = x == -b/a", "i3 = x == log(5)", "i4 = x == y/2 - 3/2"],
    );
    check("syms x y\ne3 = str2sym('[x, y; 2*x, 3]')\n[A, B] = equationsToMatrix([x + 2*y == 3, 4*x - y == 1], [x y])\n", &["e3 = \n[  x, y]\n[2*x, 3]", "A = \n[1,  2]\n[4, -1]", "B = \n3\n1"]);
}

#[test]
fn vpasolve_matches_matlab_digits() {
    check(
        "syms x y\nv1 = vpasolve(x^3 - 2*x - 5 == 0, x)\nv2 = vpasolve(x^2 - 2 == 0, x)\nv4 = vpasolve(cos(x) == x, x, 1)\nS = vpasolve([x + y == 3, x - y == 1], [x y])\n",
        &[
            "v1 = \n                                       2.0945514815423265914823865405793\n- 1.0472757407711632957411932702897 - 1.1359398890889281862454926290294i\n- 1.0472757407711632957411932702897 + 1.1359398890889281862454926290294i",
            "v2 = \n-1.4142135623730950488016887242097\n 1.4142135623730950488016887242097",
            "v4 = 0.73908513321516064165531208767387",
            "S = struct with fields:\n\n    x: 2.0\n    y: 1.0",
        ],
    );
}

#[test]
fn partial_fractions_and_transforms() {
    check(
        concat!(
            "syms x s t a\np1 = partfrac((x^2 + 1)/(x^3 - x), x)\np2 = partfrac(1/((x+1)*(x+2)), x)\np3 = partfrac((x^3+1)/(x^2-1), x)\n",
            "l1 = laplace(t^2)\nl2 = laplace(exp(-2*t)*sin(3*t))\nl3 = laplace(cos(a*t))\nl4 = laplace(heaviside(t-1))\nl6 = laplace(t*exp(-t))\n",
            "il1 = ilaplace(1/(s^2 + 4))\nil2 = ilaplace(1/(s*(s+1)))\nil3 = ilaplace((s+1)/(s^2+2*s+5))\nil4 = ilaplace(1/s^3)\nil5 = ilaplace(exp(-2*s)/s)\n",
        ),
        &[
            "p1 = 1/(x - 1) + 1/(x + 1) - 1/x",
            "p2 = 1/(x + 1) - 1/(x + 2)",
            "p3 = x + 1/(x - 1)",
            "l1 = 2/s^3",
            "l2 = 3/((s + 2)^2 + 9)",
            "l3 = s/(a^2 + s^2)",
            "l4 = exp(-s)/s",
            "l6 = 1/(s + 1)^2",
            "il1 = sin(2*t)/2",
            "il2 = 1 - exp(-t)",
            "il3 = cos(2*t)*exp(-t)",
            "il4 = t^2/2",
            "il5 = heaviside(t - 2)",
        ],
    );
}

#[test]
fn sums_matrices_and_number_theory() {
    check(
        concat!(
            "syms x y k n\nd2 = polynomialDegree(x^2*y^3 + x, [x y])\nh1 = hessian(x^2*y + y^3, [x y])\ns2 = symsum(1/2^k, k, 0, inf)\ns3 = symsum(k, k, 1, 10)\n",
            "pr1 = symprod(k, k, 1, n)\nho = horner(x^3 + 2*x^2 + 3*x + 4)\ndv = divisors(sym(12))\nzs = zeta(sym(2))\nnp = [nextprime(100), prevprime(100), nthprime(10)]\n",
            "fb = fibonacci(10)\nhm = harmonic(sym(4))\nep = eulerPhi(36)\ncps = charpoly(sym([1 2; 3 4]), x)\nad = adjoint(sym([1 2; 3 4]))\nfi = finverse(2*x + 3)\n",
        ),
        &[
            "d2 = 5",
            "h1 = \n[2*y, 2*x]\n[2*x, 6*y]",
            "s2 = 2",
            "s3 = 55",
            "pr1 = factorial(n)",
            "ho = x*(x*(x + 2) + 3) + 4",
            "dv = \n[1, 2, 3, 4, 6, 12]",
            "zs = pi^2/6",
            "np = 1×3\n\n   101    97    29",
            "fb = 55",
            "hm = 25/12",
            "ep = 12",
            "cps = x^2 - 5*x - 2",
            "ad = \n[ 4, -2]\n[-3,  1]",
            "fi = x/2 - 3/2",
        ],
    );
}

#[test]
fn numeric_special_functions() {
    let mut it = Interpreter::new();
    it.run_source("z = [zeta(2) zeta(3) zeta(0.5) zeta(-1)]; w = lambertw(1);").unwrap();
    let z = it.get_var("z").unwrap().to_matrix().unwrap().data;
    for (a, b) in z.iter().zip([1.6449340668482264, 1.2020569031595942, -1.4603545088095868, -1.0 / 12.0]) {
        assert!((a - b).abs() < 1e-12, "{z:?}");
    }
    let w = it.get_var("w").unwrap().to_matrix().unwrap().data[0];
    assert!((w - 0.5671432904097838).abs() < 1e-14);
}

#[test]
fn signs_and_imaginary_parts_print_like_matlab() {
    check("syms x\na1 = -x - 1\na2 = -x\na3 = [-x - 1; x]\na4 = sym(2+3i)\na5 = sym(-2-3i)\na8 = -2*x^2 + 1\n", &["a1 = - x - 1", "a2 = -x", "a3 = \n- x - 1\n      x", "a4 = 2 + 3i", "a5 = - 2 - 3i", "a8 = 1 - 2*x^2"]);
}
