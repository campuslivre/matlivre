//! sinpi, cospi, rms, residue, matchpairs, pageinv, pagenorm, combinations, ss2tf, ltitr,
//! interp1q, padecoef, expmv, kde, svdappend, superiorfloat, inner2outer, small helpers, platform
//! queries, Name=Value arguments, and the display and norm fixes found with them, against MATLAB
//! R2023b.

use livre_lang::{Interpreter, OutputKind};

fn shown(src: &str) -> Vec<String> {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    it.take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Value { text, .. } => Some(text),
            OutputKind::Text(t) => Some(t),
            OutputKind::Warning(w) => Some(format!("Warning: {w}")),
            OutputKind::Error(e) => Some(format!("Error: {e}")),
            _ => None,
        })
        .collect()
}

#[test]
fn trig_pi_and_rms() {
    assert_eq!(
        shown(concat!(
            "s = sinpi([0 0.5 1 1.5 2 -1 0.25 1/3])\nc = cospi([0 0.5 1 1.5 2 -1 0.25 1/3])\ns2 = sinpi(single(0.5))\ncz = cospi(Inf)\n",
            "r = rms([1 2 3 4])\nr2 = rms([1 2; 3 4])\nr3 = rms([1 2; 3 4], 2)\nr4 = rms([1 NaN 3], 'omitnan')\nr5 = rms(magic(3), 'all')\n",
            "r6 = rms([3+4i 1])\nr7 = rms([])\nr8 = rms(int8([1 2 3]))\n",
            "try, sinpi('a'), catch e, disp(e.message), end\ntry, rms({1}), catch e, disp(e.message), end\n",
        )),
        [
            "s = 1×8\n\n         0    1.0000         0   -1.0000         0         0    0.7071    0.8660",
            "c = 1×8\n\n    1.0000         0   -1.0000         0    1.0000   -1.0000    0.7071    0.5000",
            "s2 = single\n   1",
            "cz = NaN",
            "r = 2.7386",
            "r2 = 1×2\n\n    2.2361    3.1623",
            "r3 = 2×1\n\n    1.5811\n    3.5355",
            "r4 = 2.2361",
            "r5 = 5.6273",
            "r6 = 3.6056",
            "r7 = NaN",
            "r8 = 2.1602",
            "Undefined function 'sinpi' for input arguments of type 'char'.\n",
            "Undefined function 'real' for input arguments of type 'cell'.\n",
        ]
    );
}

#[test]
fn residue_both_directions() {
    assert_eq!(
        shown(concat!(
            "[rr,pp,kk] = residue([5 3 -2 7],[-4 0 8 3])\n[rr2,pp2,kk2] = residue([1 2],[1 3 2])\n[rr3,pp3,kk3] = residue(1,[1 2 1])\n",
            "[bb,aa] = residue([-2 1],[-2 -1],[])\n",
        )),
        [
            "rr = 3×1\n\n   -1.4167\n   -0.6653\n    1.3320",
            "pp = 3×1\n\n    1.5737\n   -1.1644\n   -0.4093",
            "kk = -1.2500",
            "rr2 = 2×1\n\n     0\n     1",
            "pp2 = 2×1\n\n    -2\n    -1",
            "kk2 = []",
            "rr3 = 2×1\n\n     0\n     1",
            "pp3 = 2×1\n\n    -1\n    -1",
            "kk3 = []",
            "bb = 1×2\n\n    -1     0",
            "aa = 1×3\n\n     1     3     2",
        ]
    );
}

#[test]
fn matchpairs_minimizes_cost() {
    assert_eq!(
        shown(concat!(
            "[M,uR,uC] = matchpairs([4 1 3; 2 0 5; 3 2 2], 10)\n",
            "M3 = matchpairs([1 2 3 4; 5 6 7 8], 3, 'max')\n",
            "[M4,uR4,uC4] = matchpairs([1 100; 100 1; 50 50], 20)\n",
            "[M5,uR5,uC5] = matchpairs([1 100; 100 1; 50 50], 0.4)\n",
            "M6 = matchpairs([Inf 2; 3 Inf], 100)\n",
            "M8 = matchpairs(magic(5), 100)\n",
            "try, matchpairs([1 NaN], 1), catch e, disp(e.message), end\n",
        )),
        [
            "M = 3×2\n\n     2     1\n     1     2\n     3     3",
            "uR = 0×1 empty double column vector",
            "uC = 0×1 empty double column vector",
            "M3 = 1×2\n\n     2     4",
            "M4 = 2×2\n\n     1     1\n     2     2",
            "uR4 = 3",
            "uC4 = 0×1 empty double column vector",
            "M5 = 0×2 empty double matrix",
            "uR5 = 3×1\n\n     1\n     2\n     3",
            "uC5 = 2×1\n\n     1\n     2",
            "M6 = 2×2\n\n     2     1\n     1     2",
            "M8 = 5×2\n\n     3     1\n     2     2\n     1     3\n     5     4\n     4     5",
            "Cost must not contain NaN.\n",
        ]
    );
}

#[test]
fn pages_and_tables() {
    assert_eq!(
        shown(concat!(
            "P = pageinv(cat(3,[2 0;0 4],[1 2;3 4]))\n",
            "n3 = pagenorm(cat(3,[1 2;3 4],[0 1;1 0]),1)\nn2 = pagenorm(cat(3,[1 2;3 4],[0 1;1 0]),'fro')\n",
            "w = pageinv(cat(3,[1 1;1 1],eye(2)))\n",
            "try, pageinv(ones(2,3)), catch e, disp(e.message), end\n",
            "T = combinations([1 2 3],[\"a\" \"b\"])\nT4 = combinations(1:2, categorical([\"lo\" \"hi\"]))\n",
            "try, combinations(), catch e, disp(e.message), end\n",
        )),
        [
            "P = 2×2×2\n\nP(:,:,1) =\n\n    0.5000         0\n         0    0.2500\n\nP(:,:,2) =\n\n   -2.0000    1.0000\n    1.5000   -0.5000",
            "n3 = 1×1×2\n\nn3(:,:,1) =\n\n     6\n\nn3(:,:,2) =\n\n     1",
            "n2 = 1×1×2\n\nn2(:,:,1) =\n\n    5.4772\n\nn2(:,:,2) =\n\n    1.4142",
            "w = 2×2×2\n\nw(:,:,1) =\n\n   Inf   Inf\n   Inf   Inf\n\nw(:,:,2) =\n\n     1     0\n     0     1",
            "Pages of the input array must be square.\n",
            "T = 6×2 table\n\n    Var1    Var2\n    ____    ____\n\n     1      \"a\" \n     1      \"b\" \n     2      \"a\" \n     2      \"b\" \n     3      \"a\" \n     3      \"b\" ",
            "T4 = 4×2 table\n\n    Var1    Var2\n    ____    ____\n\n     1       lo \n     1       hi \n     2       lo \n     2       hi ",
            "Not enough input arguments.\n",
        ]
    );
}

#[test]
fn systems_interpolation_and_pade() {
    assert_eq!(
        shown(concat!(
            "[b2,a2] = ss2tf([-1 0;0 -2],[1 0;0 1],[1 1],[0 0],2)\n",
            "x = ltitr([0.5 0;0 0.2],[1;1],[1;2;3])\nx2 = ltitr([0.5 0;0 0.2],[1;1],[1;2;3],[1;1])\n",
            "yi = interp1q([1;2;3],[10;20;30],[1.5;2.5;4])\nyi2 = interp1q([1;2;3],[10 1;20 2;30 3],[1.5;0])\n",
            "[num,den] = padecoef(0.1,2)\n[num1,den1] = padecoef(1)\n",
            "e1 = expmv([0 1;0 0],[1;1])\ne2 = expmv([1 0;0 2],[1;1],0.5)\n",
        )),
        [
            "b2 = 1×3\n\n     0     1     1",
            "a2 = 1×3\n\n     1     3     2",
            "x = 3×2\n\n         0         0\n    1.0000    1.0000\n    2.5000    2.2000",
            "x2 = 3×2\n\n    1.0000    1.0000\n    1.5000    1.2000\n    2.7500    2.2400",
            "yi = 3×1\n\n    15\n    25\n   NaN",
            "yi2 = 2×2\n\n   15.0000    1.5000\n       NaN       NaN",
            "num = 1×3\n\n           1         -60        1200",
            "den = 1×3\n\n           1          60        1200",
            "num1 = 1×2\n\n    -1     2",
            "den1 = 1×2\n\n     1     2",
            "e1 = 2×1\n\n     2\n     1",
            "e2 = 2×1\n\n    1.6487\n    2.7183",
        ]
    );
}

#[test]
fn platform_and_java_queries() {
    // MatLivre has no Java, so it answers as MATLAB started with -nojvm.
    assert_eq!(
        shown(concat!(
            "m = ismac, p = ispc, st = isstudent\nuj = usejava('jvm')\nv = version\nvr = version('-release')\nvj = version('-java')\n",
            "jc = javachk('jvm'); jc.message\njc.identifier\njc2 = javachk('swing', 'myfun'); jc2.identifier\n",
            "try, version('-foo'), catch e, disp(e.message), end\n",
        )),
        [
            &format!("m = logical\n   {}", if cfg!(target_os = "macos") { 1 } else { 0 }),
            &format!("p = logical\n   {}", if cfg!(windows) { 1 } else { 0 }),
            "st = logical\n   0",
            "uj = logical\n   0",
            "v = '23.2.0.2409890 (R2023b) Update 3'",
            "vr = '2023b'",
            "vj = 'Java is not enabled'",
            "ans = \n    'This feature is not supported because:\n     Java is not currently available.'",
            "ans = 'MATLAB:javachk:thisFeatureNotAvailable'",
            "ans = 'MATLAB:javachk:featureNotAvailable'",
            "Invalid option passed to 'version' command\n",
        ]
    );
}

#[test]
fn norms_singular_inverse_and_empty_display() {
    assert_eq!(
        shown(concat!(
            "n1 = norm([1 2;3 4],1)\nni = norm([1 2;3 4],Inf)\n",
            "x = inv([1 1;1 1])\n",
            "y = cat(3,[1.5 2],[1 2])\n",
            "a = zeros(0,1)\nb = zeros(1,0)\nd = zeros(0,0,'single')\ne = zeros(0,3,'int8')\nf = ''\nh = false(0,1)\nk = zeros(0,2,2)\n",
        )),
        [
            "n1 = 6",
            "ni = 7",
            "Warning: Matrix is singular to working precision.",
            "x = 2×2\n\n   Inf   Inf\n   Inf   Inf",
            "y = 1×2×2\n\ny(:,:,1) =\n\n    1.5000    2.0000\n\ny(:,:,2) =\n\n     1     2",
            "a = 0×1 empty double column vector",
            "b = 1×0 empty double row vector",
            "d = 0×0 empty single matrix",
            "e = 0×3 empty int8 matrix",
            "f = 0×0 empty char array",
            "h = 0×1 empty logical array",
            "k = 0×2×2 empty double array",
        ]
    );
}

#[test]
fn kernel_density_estimates() {
    assert_eq!(
        shown(concat!(
            "[f,xf] = kde([1 2 2 3 7]); sz = size(f), xf([1 50 100])\n",
            "[f2,xf2,bw] = kde([1 2 2 3 7], EvaluationPoints=[0 2 4]); f2, bw\n",
            "f4 = kde([1 2 2 3 7], EvaluationPoints=[0 2 4], ProbabilityFcn=\"cdf\", Bandwidth=1)\n",
            "f5 = kde([1 2 2 3 7], EvaluationPoints=[0 2 4], Kernel=\"epanechnikov\", Bandwidth=2)\n",
            "[f6,~,bw6] = kde([1;5;9], EvaluationPoints=5, Weight=[1 2 1])\n",
            "f9 = kde([1 2 2 3 7], EvaluationPoints=[0 2 4], Kernel=\"box\", Bandwidth=1)\n",
            "f10 = kde([1 2 2 3 7], EvaluationPoints=[0 2 4], Kernel=\"triangle\", Bandwidth=1)\n",
            "f11 = kde([1 2 NaN 3], EvaluationPoints=2, Bandwidth=1)\n",
            "[f8,x8] = kde([1 2 2 3 7]', NumPoints=2); size(x8)\n",
        )),
        [
            "sz = 1×2\n\n     1   100",
            "ans = 1×3\n\n   -2.4145    3.9352   10.4145",
            "f2 = 1×3\n\n    0.0798    0.2355    0.0819",
            "bw = 1.1382",
            "f4 = 1×3\n\n    0.0411    0.4000    0.7592",
            "f5 = 1×3\n\n    0.1040    0.1308    0.1224",
            "f6 = 0.0684",
            "bw6 = 5.0424",
            "f9 = 1×3\n\n    0.0577    0.2309    0.0577",
            "f10 = 1×3\n\n    0.0783    0.2599    0.0783",
            "f11 = 0.2943",
            "ans = 1×2\n\n     2     1",
        ]
    );
}

#[test]
fn svdappend_superiorfloat_and_helpers() {
    assert_eq!(
        shown(concat!(
            "[U,S,V] = svdappend([1 0;0 2], [1 0;0 1], [1 0;0 1], [3;4]); S\nA = round(U*diag(S)*V', 10)\n",
            "[U2,S2,V2] = svdappend([1 0;0 1;0 0], [2 0;0 1], [1 0;0 1], [0 5], 'Shape', 'rows'); size(U2)\n",
            "c1 = superiorfloat(1, single(2))\nc2 = superiorfloat(1, 2)\ntry, superiorfloat(int8(1), 2), catch e, disp(e.message), end\n",
            "A = table([1;2],[3;4],'VariableNames',{'x','y'}); B = table([5;6],[7;8],'VariableNames',{'x','y'});\n",
            "T2 = inner2outer(table(A,B,'VariableNames',{'a','b'})); T2.Properties.VariableNames\nT2.x\n",
            "q2 = getcolumn([1 2 3;4 5 6], [1 3])\nst = setstr([72 105])\nvz = vectorize('x^2*y/z')\nvz2 = vectorize(@(x) x^2)\n",
            "figure('Visible','off'); ax = axes; i2 = is2D(ax)\nview(3); i3 = is2D(ax)\n",
        )),
        [
            "S = 2×1\n\n    5.2915\n    1.4142",
            "A = 2×3\n\n     1     0     3\n     0     2     4",
            "ans = 1×2\n\n     4     2",
            "c1 = 'single'",
            "c2 = 'double'",
            "Inputs must be floats, namely single or double.\n",
            "ans = 1×2 cell\n\n    {'x'}    {'y'}",
            "ans = 2×2 table\n\n    a    b\n    _    _\n\n    1    5\n    2    6",
            "q2 = 2×2\n\n     1     3\n     4     6",
            "st = 'Hi'",
            "vz = 'x.^2.*y./z'",
            "vz2 = '@(x)x.^2'",
            "i2 = logical\n   1",
            "i3 = logical\n   0",
        ]
    );
}

#[test]
fn name_value_arguments_and_anonymous_varargin() {
    assert_eq!(
        shown("f = @(varargin) varargin;\nc = f(1, Alpha=2, beta=\"x\")\nclass(c{2})\nc2 = f(A=1==1)\ns.k = @(varargin) numel(varargin);\nn = s.k(X=1)\n"),
        [
            "c = 1×5 cell\n\n    {[1]}    {[\"Alpha\"]}    {[2]}    {[\"beta\"]}    {[\"x\"]}",
            "ans = 'string'",
            "c2 = 1×2 cell\n\n    {[\"A\"]}    {[1]}",
            "n = 2",
        ]
    );
}
