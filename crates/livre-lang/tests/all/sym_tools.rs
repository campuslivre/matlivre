//! Symbolic utilities: quorem, in, assumptions, cell2sym/sym2cell, the symType queries, fold,
//! polynomial and linear algebra tools, potentials, pade, vpasum, vpaintegral and poles, against
//! MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind};

fn shown(src: &str) -> Vec<String> {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    it.take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Value { text, .. } => Some(text),
            OutputKind::Text(t) => Some(t),
            OutputKind::Error(e) => Some(format!("Error: {e}")),
            _ => None,
        })
        .collect()
}

fn check(src: &str, expected: &[&str]) {
    assert_eq!(shown(src), expected, "{src}");
}

#[test]
fn quotients_sets_assumptions_and_cells() {
    check(
        concat!(
            "syms x y\n",
            "[Q1, R1] = quorem(x^3+2*x+1, x-1)\n",
            "[Q2, R2] = quorem(x^3+y*x+1, x+y, x)\n",
            "[Q3, R3] = quorem(sym(17), sym(5))\n",
            "[Q4, R4] = quorem(x^4-1, x^2+x)\n",
            "[Q5, R5] = quorem(sym(-17), sym(5))\n",
            "[Q6, R6] = quorem(2*x^2 + 3, 2*x + 1)\n",
            "[Q7, R7] = quorem(x^2*y + y, y + 1, y)\n",
            "I1 = in(x, 'real')\n",
            "I2 = in(sym(3), 'integer')\n",
            "I3 = in(sym(1/2), 'rational')\n",
            "I5 = in(sym(1i), 'real')\n",
            "A0 = assumptions\n",
            "syms x real\n",
            "A1 = assumptions\n",
            "A2 = assumptions(x)\n",
            "A3 = assumptions(y)\n",
            "syms x\n",
            "C1 = cell2sym({1 2; x 'y'})\n",
            "C2 = cell2sym({[1 2] [3]; [x y] sym('z')})\n",
            "C3 = cell2sym({x})\n",
            "S1 = sym2cell([x 2; y^2 sym('z')])\n",
        ),
        &[
            "Q1 = x^2 + x + 3",
            "R1 = 4",
            "Q2 = x^2 - x*y + y^2 + y",
            "R2 = 1 - y*(y^2 + y)",
            "Q3 = 3",
            "R3 = 2",
            "Q4 = x^2 - x + 1",
            "R4 = - x - 1",
            "Q5 = -4",
            "R5 = 3",
            "Q6 = x - 1/2",
            "R6 = 7/2",
            "Q7 = x^2 + 1",
            "R7 = - x^2 - 1",
            "I1 = in(x, 'real')",
            "I2 = in(3, 'integer')",
            "I3 = in(1/2, 'rational')",
            "I5 = in(1i, 'real')",
            "A0 = \nEmpty sym: 1-by-0",
            "A1 = in(x, 'real')",
            "A2 = in(x, 'real')",
            "A3 = \nEmpty sym: 1-by-0",
            "C1 = \n[1, 2]\n[x, y]",
            "C2 = \n[1, 2, 3]\n[x, y, z]",
            "C3 = x",
            "S1 = 2×2 cell\n\n    {[x  ]}    {[2]}\n    {[y^2]}    {[z]}",
        ],
    );
}

#[test]
fn sym_type_queries_and_fold() {
    check(
        concat!(
            "syms x y z\n",
            "T1 = symType(x)\n",
            "T2 = symType(sym(2))\n",
            "T3 = symType(x+1)\n",
            "T4 = symType(x == 1)\n",
            "T5 = symType([x sin(x); 2 x*y])\n",
            "T6 = symType(sym(pi))\n",
            "T7 = symType(sin(x))\n",
            "T8 = symType(symtrue)\n",
            "T9 = symType(x < 2)\n",
            "T10 = symType(vpa(2.5))\n",
            "T11 = symType(sym(2+3i))\n",
            "T12 = symType(sym(1/3))\n",
            "T13 = symType(sym(Inf))\n",
            "T14 = symType(-x)\n",
            "IS1 = isSymType(x+1, 'expression')\n",
            "IS2 = isSymType([x 2 sin(x)], 'variable')\n",
            "IS3 = isSymType(sym(3), 'integer')\n",
            "IS4 = isSymType(sym(3), 'number')\n",
            "IS5 = isSymType(sin(x), 'sin')\n",
            "IS6 = isSymType(sym(1/2), 'rational')\n",
            "IS7 = isSymType(x^2, 'power')\n",
            "IS8 = isSymType(x*y, 'times')\n",
            "IS9 = isSymType(x+y, 'plus')\n",
            "IS10 = isSymType(sym(pi), 'constant')\n",
            "IS11 = isSymType(vpa(2.5), 'real')\n",
            "IS12 = isSymType(sym(2+3i), 'complex')\n",
            "IS13 = isSymType(sym(2), 'real')\n",
            "IS14 = isSymType(x == 1, 'eq')\n",
            "IS15 = isSymType(x < 1, 'lt')\n",
            "IS16 = isSymType(x, 'scalar')\n",
            "H1 = hasSymType(sin(x)+cos(y), 'sin')\n",
            "H2 = hasSymType(x^2+1, 'integer')\n",
            "H3 = hasSymType(x^2+1, 'power')\n",
            "H4 = hasSymType([x 1], 'variable')\n",
            "F1 = findSymType(sin(x)+cos(y)+sin(2*z), 'sin')\n",
            "F2 = findSymType(x^2 + y*z + 3, 'variable')\n",
            "F3 = findSymType(x^2 + y*z + 3, 'integer')\n",
            "F4 = findSymType(x^2 + y*z + 3, 'power')\n",
            "F5 = findSymType(x^2+1, 'sin')\n",
            "F6 = findSymType(sin(sin(x)), 'sin')\n",
            "F7 = findSymType(2*x + y/3, 'number')\n",
            "M2 = mapSymType(x+y+2, 'variable', @(u) 2*u)\n",
            "M3 = mapSymType(x^2 + 3, 'integer', @(u) u+1)\n",
            "g(x,y) = x+y;\n",
            "FT2 = symFunType(g)\n",
            "FT3 = symFunType(sin(x))\n",
            "FD1 = fold(@plus, [x y z])\n",
            "FD2 = fold(@times, [x y z])\n",
            "FD3 = fold(@(a,b) a^b, [x y z])\n",
            "FD4 = fold(@plus, [x y z], 3)\n",
            "FD5 = fold(@plus, sym([]), 7)\n",
            "FD6 = fold(@max, [1 5 3])\n",
            "FD7 = fold(@(a,b) a - b, [x y z])\n",
        ),
        &[
            "T1 = \"variable\"",
            "T2 = \"integer\"",
            "T3 = \"expression\"",
            "T4 = \"equation\"",
            "T5 = 2×2 string array\n\n    \"variable\"    \"expression\"\n    \"integer\"     \"expression\"",
            "T6 = \"constant\"",
            "T7 = \"expression\"",
            "T8 = \"logicalconstant\"",
            "T9 = \"equation\"",
            "T10 = \"vpareal\"",
            "T11 = \"complex\"",
            "T12 = \"rational\"",
            "T13 = \"constant\"",
            "T14 = \"expression\"",
            "IS1 = logical\n   1",
            "IS2 = 1×3 logical array\n\n   1   0   0",
            "IS3 = logical\n   1",
            "IS4 = logical\n   1",
            "IS5 = logical\n   1",
            "IS6 = logical\n   1",
            "IS7 = logical\n   1",
            "IS8 = logical\n   1",
            "IS9 = logical\n   1",
            "IS10 = logical\n   1",
            "IS11 = logical\n   1",
            "IS12 = logical\n   1",
            "IS13 = logical\n   1",
            "IS14 = logical\n   1",
            "IS15 = logical\n   1",
            "IS16 = logical\n   0",
            "H1 = logical\n   1",
            "H2 = logical\n   1",
            "H3 = logical\n   1",
            "H4 = 1×2 logical array\n\n   1   0",
            "F1 = \n[sin(2*z), sin(x)]",
            "F2 = \n[x, y, z]",
            "F3 = \n[2, 3]",
            "F4 = x^2",
            "F5 = \nEmpty sym: 1-by-0",
            "F6 = sin(sin(x))",
            "F7 = \n[1/3, 2]",
            "M2 = 2*x + 2*y + 2",
            "M3 = x^3 + 4",
            "FT2 = \"plus\"",
            "FT3 = \"sin\"",
            "FD1 = x + y + z",
            "FD2 = x*y*z",
            "FD3 = (x^y)^z",
            "FD4 = x + y + z",
            "FD5 = 7",
            "FD6 = 5",
            "FD7 = x - y - z",
        ],
    );
}

#[test]
fn polynomials_and_linear_algebra() {
    check(
        concat!(
            "syms x y a b t\n",
            "RS1 = resultant(x^2-1, x^2-4, x)\n",
            "RS2 = resultant(x^2+y, x-y, x)\n",
            "RS3 = resultant(x^2+a*x+b, 2*x+a, x)\n",
            "RS4 = resultant(x^3-2*x+1, x^2-1)\n",
            "RS5 = resultant(x^2 + y^2 - 1, x - y, y)\n",
            "PR1 = polynomialReduce(x^3 - x*y^2 + 1, x+y)\n",
            "[PR2a, PR2b] = polynomialReduce(x^4+1, x^2+1)\n",
            "PR3 = polynomialReduce(x^2*y + x*y^2 + y^2, [x*y-1, y^2-1])\n",
            "CS1 = colspace(sym([1 2; 2 4]))\n",
            "CS2 = colspace(sym([1 0 1; 0 1 1; 1 1 2]))\n",
            "CS3 = colspace(sym([2 4 6; 1 3 5]))\n",
            "CS4 = colspace(sym([0 0; 0 0]))\n",
            "BM1 = bernsteinMatrix(2, [0 1/2 1])\n",
            "BM2 = bernsteinMatrix(3, sym([0 1/3 1]))\n",
            "BM3 = bernsteinMatrix(2, t)\n",
            "BM4 = bernsteinMatrix(1, [0.25 0.75])\n",
            "MP1 = minpoly(sym([1 1; 0 1]))\n",
            "MP2 = minpoly([2 0; 0 2])\n",
            "MP3 = minpoly(sym([1 2; 3 4]), x)\n",
            "MP4 = minpoly(sym(eye(3)))\n",
        ),
        &[
            "RS1 = 9",
            "RS2 = y^2 + y",
            "RS3 = - a^2 + 4*b",
            "RS4 = 0",
            "RS5 = 2*x^2 - 1",
            "PR1 = 1",
            "PR2a = 2",
            "PR2b = x^2 - 1",
            "PR3 = x + y + 1",
            "CS1 = \n1\n2",
            "CS2 = \n[1, 0]\n[0, 1]\n[1, 1]",
            "CS3 = \n[1, 0]\n[0, 1]",
            "CS4 = \n[ empty sym ]",
            "BM1 = 3×3\n\n    1.0000         0         0\n    0.2500    0.5000    0.2500\n         0         0    1.0000",
            "BM2 = \n[   1,   0,   0,    0]\n[8/27, 4/9, 2/9, 1/27]\n[   0,   0,   0,    1]",
            "BM3 = \n[(t - 1)^2, -2*t*(t - 1), t^2]",
            "BM4 = 2×2\n\n    0.7500    0.2500\n    0.2500    0.7500",
            "MP1 = \n[1, -2, 1]",
            "MP2 = 1×2\n\n     1    -2",
            "MP3 = x^2 - 5*x - 2",
            "MP4 = \n[1, -1]",
        ],
    );
}

#[test]
fn potentials_pade_numerics_and_poles() {
    check(
        concat!(
            "syms x y z k\n",
            "P2 = potential([x y z], [x y z], [0 0 0])\n",
            "P3 = potential([y, -x], [x y])\n",
            "P4 = potential([2*x, 2*y], [x y])\n",
            "P5 = potential([y*z, x*z, x*y], [x y z])\n",
            "VP1 = vectorPotential([x^2*y, -1/2*y^2*x, -x*y*z], [x y z])\n",
            "VP2 = vectorPotential([x y -2*z], [x y z])\n",
            "PD1 = pade(sin(x))\n",
            "PD2 = pade(exp(x), x, 'Order', [2 2])\n",
            "PD3 = pade(cos(x), 'Order', 4)\n",
            "PD4 = pade(1/(1-x))\n",
            "PD6 = pade(exp(x), 'Order', [1 1])\n",
            "PD7 = pade(log(1+x), 'Order', [2 1])\n",
            "VS1 = vpasum(1/k^2, k, 1, Inf)\n",
            "VS2 = vpasum(1/k, k, 1, 10)\n",
            "VS3 = vpasum(1/2^k, k, 0, Inf)\n",
            "VS4 = vpasum(k^2, k, 1, 5)\n",
            "VI1 = vpaintegral(exp(-x^2), x, 0, 1)\n",
            "VI2 = vpaintegral(sin(x)/x, 1, 2)\n",
            "VI3 = vpaintegral(x^2, x, 0, 3)\n",
            "VI4 = vpaintegral(1/x, x, 1, exp(sym(1)))\n",
            "VI5 = vpaintegral(exp(-x), x, 0, Inf)\n",
            "PO1 = poles(1/(x^2-1))\n",
            "PO2 = poles(1/((x-2)^2*(x+3)), x)\n",
            "PO5 = poles(x/(x^2+1))\n",
            "PO6 = poles(1/(x-1)^3)\n",
            "PO7 = poles(sin(x))\n",
        ),
        &[
            "P2 = x^2/2 + y^2/2 + z^2/2",
            "P3 = NaN",
            "P4 = x^2 + y^2",
            "P5 = x*y*z",
            "VP1 = \n-(x*y^2*z)/2\n    -x^2*y*z\n           0",
            "VP2 = \n y*z\n-x*z\n   0",
            "PD1 = -(x*(7*x^2 - 60))/(3*(x^2 + 20))",
            "PD2 = (x^2 + 6*x + 12)/(x^2 - 6*x + 12)",
            "PD3 = (313*x^4 - 6900*x^2 + 15120)/(13*x^4 + 660*x^2 + 15120)",
            "PD4 = -1/(x - 1)",
            "PD6 = -(x + 2)/(x - 2)",
            "PD7 = (x*(x + 6))/(2*(2*x + 3))",
            "VS1 = 1.644934066848226436472415166646",
            "VS2 = 2.9289682539682539682539682539683",
            "VS3 = 2.0",
            "VS4 = 55.0",
            "VI1 = 0.746824",
            "VI2 = 0.65933",
            "VI3 = 9.0",
            "VI4 = 1.0",
            "VI5 = 1.0",
            "PO1 = \n-1\n 1",
            "PO2 = \n-3\n 2",
            "PO5 = \n-1i\n 1i",
            "PO6 = 1",
            "PO7 = \nEmpty sym: 0-by-1",
        ],
    );
}

#[test]
fn subexpr_picks_the_most_profitable_common_subexpression() {
    // Expected subexpressions recorded from MATLAB R2023b; the rewritten expression is
    // checked by substituting the subexpression back.
    let cases: &[(&str, &str)] = &[
        ("sin(x^2+1)^2 + cos(x^2+1)^2*exp(x^2+1) + (x^2+1)*y", "x^2 + 1"),
        ("(x^2+1)*(y+x)+ (x^2+1)*(y+x)*b + sin((x^2+1)*(y+x))", "(x^2 + 1)*(x + y)"),
        ("(a+b)^2 + (a+b)^2*x + (a+b)*y", "(a + b)^2"),
        ("x*y + x*y*a + x*y*b", "x*y"),
        ("(x+y)^2*(a+b) + (x+y)^2*(a+b)^2 + exp(x+y)*(a+b)", "(x + y)^2"),
        ("sin(x+y)+cos(x+y)+exp(a+b)+log(a+b)", "x + y"),
        ("sin(a+b)+cos(a+b)+exp(x+y)+log(x+y)", "x + y"),
        ("sin(x+y)*cos(x+y)*a+ sin(x+y)*cos(x+y)*b + sin(x+y)*c", "x + y"),
        ("x+y+a + sin(x+y+a)*b", "a + x + y"),
        ("(x+y+a)*c+ (x+y+a+b)*d", "a + x + y"),
        ("(x+y)^2+(x+y)^3", "x + y"),
        ("x^2*y + x^2*a + y*a", "x^2"),
        ("sin(x*y)+cos(x*y)*a", "x*y"),
    ];
    for (f, s) in cases {
        let src = format!(
            "syms x y a b c d\nf = {f};\n[r, s] = subexpr(f);\ndisp(char(s))\nsyms sigma\ndisp(double(isAlways(expand(subs(r, sigma, s)) == expand(f))))\n"
        );
        assert_eq!(shown(&src), vec![s.to_string() + "\n", "     1\n".to_string()], "{f}");
    }
    check(
        "syms x y a\n[r,s] = subexpr(x*y*a+x*y*(a+1));\ndisp(size(s))\n",
        &["     0     0\n"],
    );
    check(
        "syms x y a b\n[r, s] = subexpr(sin(x+y)+cos(x+y), 'q');\ndisp(char(r))\n",
        &["cos(q) + sin(q)\n"],
    );
    check(
        "syms x y\ntry, subexpr(sin(x+y)+cos(x+y), 'x y'), catch e, disp(e.message), end\ntry, subexpr(x+y, 'x'), catch e, disp(e.message), end\n",
        &[
            "Second argument must be a symbolic variable or a char array or string naming a symbolic variable.\n",
            "Expression already contains 'x'.\n",
        ],
    );
}
