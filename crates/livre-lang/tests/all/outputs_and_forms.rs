//! Missing outputs, options and call forms reported while documenting the wiki, against
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
            OutputKind::Warning(w) => Some(format!("Warning: {w}")),
            OutputKind::Error(e) => Some(format!("Error: {e}")),
            _ => None,
        })
        .collect()
}

fn check(src: &str, expected: &[&str]) {
    assert_eq!(shown(src), expected, "{src}");
}

#[test]
fn extra_outputs() {
    check(
        concat!(
            "[c, m] = strsplit('a,b;c', {',', ';'})\n",
            "[c, m] = strsplit('a,,b', ',')\n",
            "c = strsplit('a,,b', ',', 'CollapseDelimiters', false)\n",
            "c = strsplit(\"x  y\")\n",
            "[n, s] = weekday(738000)\n",
            "[n, s] = weekday([738000 738001], 'long')\n",
            "[g, c, d] = gcd([12 7 0], [18 -3 5])\n",
            "[f, e] = log2([8 10 0.3 -6 0])\n",
            "[p, S, mu] = polyfit(1:5, [2 4 7 9 12], 1);\n",
            "p, S.df, mu\n",
            "[u, k] = matlab.lang.makeUniqueStrings({'a', 'a', 'b'})\n",
            "v = matlab.lang.makeValidName('1a', 'Prefix', 'p_')\n",
            "v = matlab.lang.makeValidName('a-b c', 'ReplacementStyle', 'delete')\n",
            "v = matlab.lang.makeValidName('a-b', 'ReplacementStyle', 'hex')\n",
            "v = matlab.lang.makeValidName('for')\n",
            "r = cellfun(@(x) error('my:id', 'bad'), {1, 2}, 'ErrorHandler', @(s, x) s.index * 10)\n",
            "[xb, yb] = stairs([1 2 4], [5 3 6])\n",
        ),
        &[
            "c = 1×3 cell\n\n    {'a'}    {'b'}    {'c'}",
            "m = 1×2 cell\n\n    {','}    {';'}",
            "c = 1×2 cell\n\n    {'a'}    {'b'}",
            "m = 1×1 cell\n\n    {',,'}",
            "c = 1×3 cell\n\n    {'a'}    {0×0 char}    {'b'}",
            "c = 1×2 string array\n\n    \"x\"    \"y\"",
            "n = 3",
            "s = 'Tue'",
            "n = 1×2\n\n     3     4",
            "s = 2×9 char array\n\n    'Tuesday  '\n    'Wednesday'",
            "g = 1×3\n\n     6     1     5",
            "c = 1×3\n\n    -1     1     0",
            "d = 1×3\n\n     1     2     1",
            "f = 1×5\n\n    0.5000    0.6250    0.6000   -0.7500         0",
            "e = 1×5\n\n     4     4    -1     3     0",
            "p = 1×2\n\n    3.9528    6.8000",
            "ans = 3",
            "mu = 2×1\n\n    3.0000\n    1.5811",
            "u = 1×3 cell\n\n    {'a'}    {'a_1'}    {'b'}",
            "k = 1×3 logical array\n\n   0   1   0",
            "v = 'p_1a'",
            "v = 'abC'",
            "v = 'a0x2Db'",
            "v = 'xFor'",
            "r = 1×2\n\n    10    20",
            "xb = 5×1\n\n     1\n     2\n     2\n     4\n     4",
            "yb = 5×1\n\n     5\n     5\n     3\n     3\n     6",
        ],
    );
}

#[test]
fn matrix_operators() {
    check(
        concat!(
            "[V, D] = eig([1 2; 3 4])\n",
            "[V, D] = eig([0 1; -1 0])\n",
            "A = [4 1; 2 3]^0.5\n",
            "A = [1 2; 3 4]^0.5\n",
            "B = 2^[1 2; 3 4]\n",
            "x = [1 2; 2 4] \\ [1; 2]\n",
            "y = [1 2; 2 4] \\ [1; 3]\n",
            "z = [1 3] / [1 2; 2 4]\n",
        ),
        &[
            "V = 2×2\n\n   -0.8246   -0.4160\n    0.5658   -0.9094",
            "D = 2×2\n\n   -0.3723         0\n         0    5.3723",
            "V = 2×2 complex\n\n   0.7071 + 0.0000i   0.7071 + 0.0000i\n   0.0000 + 0.7071i   0.0000 - 0.7071i",
            "D = 2×2 complex\n\n   0.0000 + 1.0000i   0.0000 + 0.0000i\n   0.0000 + 0.0000i   0.0000 - 1.0000i",
            "A = 2×2\n\n    1.9621    0.2740\n    0.5479    1.6882",
            "A = 2×2 complex\n\n   0.5537 + 0.4644i   0.8070 - 0.2124i\n   1.2104 - 0.3186i   1.7641 + 0.1458i",
            "B = 2×2\n\n   10.4827   14.1519\n   21.2278   31.7106",
            "Warning: Matrix is singular to working precision.",
            "x = 2×1\n\n   NaN\n   NaN",
            "Warning: Matrix is singular to working precision.",
            "y = 2×1\n\n   Inf\n  -Inf",
            "Warning: Matrix is singular to working precision.",
            "z = 1×2\n\n   Inf  -Inf",
        ],
    );
}

#[test]
fn sort_options() {
    check(
        concat!(
            "s = sort([3 NaN 1], 'MissingPlacement', 'first')\n",
            "s = sort([3 NaN 1], 'descend', 'MissingPlacement', 'last')\n",
            "[s, i] = sort([-3 2 1 -2], 'ComparisonMethod', 'abs')\n",
            "t = [issorted([3 2 1], 'descend'), issorted([1 2 NaN]), issorted([NaN 1 2]), issorted([1 1 2], 'strictascend')]\n",
            "t = [issorted({'a', 'b', 'c'}), issorted([\"b\" \"a\"]), issorted([3 1; 4 2]), issorted([3 1; 4 2], 2)]\n",
            "t = issorted([1 3 2 0], 'monotonic')\n",
            "[r, i] = sortrows({'b'; 'a'; 'c'})\n",
            "r = sortrows([\"b\" \"x\"; \"a\" \"z\"; \"b\" \"a\"])\n",
            "r = sortrows({'b'; 'a'; 'c'}, 'descend')\n",
        ),
        &[
            "s = 1×3\n\n   NaN     1     3",
            "s = 1×3\n\n     3     1   NaN",
            "s = 1×4\n\n     1     2    -2    -3",
            "i = 1×4\n\n     3     2     4     1",
            "t = 1×4 logical array\n\n   1   1   0   0",
            "t = 1×4 logical array\n\n   1   0   1   0",
            "t = logical\n   0",
            "r = 3×1 cell\n\n    {'a'}\n    {'b'}\n    {'c'}",
            "i = 3×1\n\n     2\n     1\n     3",
            "r = 3×2 string array\n\n    \"a\"    \"z\"\n    \"b\"    \"a\"\n    \"b\"    \"x\"",
            "r = 3×1 cell\n\n    {'c'}\n    {'b'}\n    {'a'}",
        ],
    );
}

#[test]
fn text_forms() {
    check(
        concat!(
            "k = strfind({'abcab', 'xab'}, 'ab')\n",
            "k = strfind([\"aa\" \"aba\"], 'a')\n",
            "k = strfind('abc', 'b', 'ForceCellOutput', true)\n",
            "t = isspace(\"a b\")\n",
            "t = isstrprop(\"a1\", 'digit')\n",
            "t = isstrprop({'a1', 'b'}, 'alpha')\n",
            "t = iskeyword({'for', 'x'})\n",
            "h = height(ones(3, 4)), w = width(ones(3, 4))\n",
            "c = cast(int8(5), 'like', single(1)); class(c)\n",
            "try, anymissing([1 NaN], 1), catch e, disp(e.message), end\n",
        ),
        &[
            "k = 1×2 cell\n\n    {[1 4]}    {[2]}",
            "k = 1×2 cell\n\n    {[1 2]}    {[1 3]}",
            "k = 1×1 cell\n\n    {[2]}",
            "t = 1×3 logical array\n\n   0   1   0",
            "t = 1×2 logical array\n\n   0   1",
            "t = 1×2 cell\n\n    {[1 0]}    {[1]}",
            "t = logical\n   0",
            "h = 3",
            "w = 4",
            "ans = 'single'",
            "Too many input arguments.\n",
        ],
    );
}

#[test]
fn arguments_blocks() {
    check(
        concat!(
            "r = f1(3)\n",
            "r = f1(3, 'scale', 2)\n",
            "try, f1(-1), catch e, disp(e.message), end\n",
            "try, f1([1 2]), catch e, disp(e.message), end\n",
            "try, f1(3, 'Bogus', 1), catch e, disp(e.message), end\n",
            "try, f1(3, 'Scale'), catch e, disp(e.message), end\n",
            "try, f1(), catch e, disp(e.message), end\n",
            "try, f1(3, 'Scale', -2), catch e, disp(e.message), end\n",
            "v = f2(5)\n",
            "try, f2([1 2]), catch e, disp(e.message), end\n",
            "s = f3(1, 'B', 5)\n",
            "m = f4()\n",
            "try, f4('quad'), catch e, disp(e.message), end\n",
            "try, f5(1, 2), catch e, disp(e.message), end\n",
            "try, f6({1}), catch e, disp(e.message), end\n",
            "w = f6(uint8(7)); class(w)\n",
            "try, f7(ones(2, 3)), catch e, disp(e.message), end\n",
            "try, f8([1; 2]), catch e, disp(e.message), end\n",
            "try, f9([1 2]), catch e, disp(e.message), end\n",
            "try, f10([1 2]), catch e, disp(e.message), end\n",
            "function r = f1(x, opts)\n",
            "arguments\n",
            "    x (1,1) double {mustBePositive}\n",
            "    opts.Scale (1,1) double {mustBePositive} = 1\n",
            "end\n",
            "r = x * opts.Scale;\n",
            "end\n",
            "function v = f2(v)\n",
            "arguments\n",
            "    v (1,3) double\n",
            "end\n",
            "end\n",
            "function s = f3(a, opts)\n",
            "arguments\n",
            "    a\n",
            "    opts.A = 10\n",
            "    opts.B string\n",
            "end\n",
            "s = opts;\n",
            "end\n",
            "function m = f4(method)\n",
            "arguments\n",
            "    method (1,:) char {mustBeMember(method, {'linear', 'cubic'})} = 'linear'\n",
            "end\n",
            "m = method;\n",
            "end\n",
            "function z = f5(x, y)\n",
            "arguments\n",
            "    x double\n",
            "    y double {mustBeLessThan(y, x)}\n",
            "end\n",
            "z = x + y;\n",
            "end\n",
            "function w = f6(w)\n",
            "arguments\n",
            "    w double\n",
            "end\n",
            "end\n",
            "function q = f7(q)\n",
            "arguments\n",
            "    q (:,2)\n",
            "end\n",
            "end\n",
            "function q = f8(q)\n",
            "arguments\n",
            "    q (1,:)\n",
            "end\n",
            "end\n",
            "function q = f9(q)\n",
            "arguments\n",
            "    q (2,:)\n",
            "end\n",
            "end\n",
            "function q = f10(q)\n",
            "arguments\n",
            "    q (2,3)\n",
            "end\n",
            "end\n",
        ),
        &[
            "r = 3",
            "r = 6",
            "Invalid argument at position 1. Value must be positive.\n",
            "Invalid argument at position 1. Value must be a scalar.\n",
            "Invalid argument name 'Bogus'. Use 'Scale'.\n",
            "Invalid argument at position 2. Make sure name-value arguments include both the name and the value.\n",
            "Invalid argument list. Function requires 1 more input(s).\n",
            "Invalid value for 'Scale' argument. Value must be positive.\n",
            "v = 1×3\n\n     5     5     5",
            "Invalid argument at position 1. Value must be a vector with 3 elements.\n",
            "s = struct with fields:\n\n    A: 10\n    B: \"5\"",
            "m = 'linear'",
            "Invalid argument at position 1. Value must be a member of this set:\n    'linear'\n    'cubic'\n\n",
            "Invalid argument at position 2. Value must be less than 1.\n",
            "Invalid argument at position 1. Value must be of type double or be convertible to double.\n",
            "ans = 'double'",
            "Invalid argument at position 1. Value must be a matrix with 2 columns.\n",
            "ans = 1×2\n\n     1     2",
            "Invalid argument at position 1. Value must be a matrix with 2 rows.\n",
            "Invalid argument at position 1. Value must be a matrix of size 2-by-3.\n",
        ],
    );
}

#[test]
fn graph_outputs() {
    check(
        concat!(
            "G = graph([1 1 2 3 4], [2 3 4 4 5], [1 2 3 1 5]);\n",
            "D = digraph([1 1 2 3 4], [2 3 4 4 5], [1 2 3 1 5]);\n",
            "[p, d, e] = shortestpath(G, 1, 5)\n",
            "[TR, dist] = shortestpathtree(G, 1); dist\n",
            "[mf, GF, cs, ct] = maxflow(D, 1, 5); mf, GF.Edges.EndNodes', cs', ct'\n",
            "[P, E] = allpaths(D, 1, 5)\n",
            "[c, ec] = allcycles(digraph([1 2 3], [2 3 1]))\n",
            "[cy, ecy] = cyclebasis(G)\n",
            "[T, pred] = minspantree(G); pred\n",
            "[T, pred] = minspantree(G, 'Root', 3); pred\n",
            "[H, idx] = reordernodes(G, [5 4 3 2 1]); idx'\n",
            "[H, idx] = reordernodes(G, [2 1 3 5 4]); idx', H.Edges.EndNodes'\n",
            "[n, H] = toposort(digraph([3 1 2], [1 2 4])); n, H.Edges.EndNodes'\n",
        ),
        &[
            "p = 1×4\n\n     1     3     4     5",
            "d = 8",
            "e = 1×3\n\n     2     4     5",
            "dist = 1×5\n\n     0     1     2     3     8",
            "mf = 2",
            "ans = 2×5\n\n     1     1     2     3     4\n     2     3     4     4     5",
            "ans = 1×2\n\n     1     3",
            "ans = 1×3\n\n     2     4     5",
            "P = 2×1 cell\n\n    {[1 2 4 5]}\n    {[1 3 4 5]}",
            "E = 2×1 cell\n\n    {[1 3 5]}\n    {[2 4 5]}",
            "c = 1×1 cell\n\n    {[1 2 3]}",
            "ec = 1×1 cell\n\n    {[1 2 3]}",
            "cy = 1×1 cell\n\n    {[1 2 4 3]}",
            "ecy = 1×1 cell\n\n    {[1 3 4 2]}",
            "pred = 1×5\n\n     0     1     1     3     4",
            "pred = 1×5\n\n     3     1     0     3     4",
            "ans = 1×5\n\n     5     4     3     2     1",
            "ans = 1×5\n\n     1     3     2     4     5",
            "ans = 2×5\n\n     1     1     2     3     4\n     2     5     3     5     5",
            "n = 1×4\n\n     3     1     2     4",
            "ans = 2×3\n\n     1     2     3\n     2     3     4",
        ],
    );
}

#[test]
fn sparse_and_shapes() {
    check(
        concat!(
            "A = sparse([4 1 0; 1 4 1; 0 1 4]);\n",
            "[pe, q] = etree(A)\n",
            "[pe, q] = etree(sparse([4 0 1 0; 0 4 0 1; 1 0 4 0; 0 1 0 4]))\n",
            "[cnt, h, parent, post, R] = symbfact(A); cnt', h, parent, post, full(R)\n",
            "[L, U, P] = ilu(A); full(P)\n",
            "[x, y] = gplot([0 1; 1 0], [0 0; 1 1])\n",
            "ps = polyshape([0 2 2 0], [0 0 2 2]);\n",
            "[in, on] = isinterior(ps, [1 2 3 0], [1 1 1 2])\n",
        ),
        &[
            "pe = 1×3\n\n     2     3     0",
            "q = 1×3\n\n     1     2     3",
            "pe = 1×4\n\n     3     4     0     0",
            "q = 1×4\n\n     1     3     2     4",
            "ans = 3×1\n\n     2\n     2\n     1",
            "h = 3",
            "parent = 1×3\n\n     2     3     0",
            "post = 1×3\n\n     1     2     3",
            "ans = 3×3\n\n     1     1     0\n     0     1     1\n     0     0     1",
            "ans = 3×3\n\n     1     0     0\n     0     1     0\n     0     0     1",
            "x = 6×1\n\n     1\n     0\n   NaN\n     0\n     1\n   NaN",
            "y = 6×1\n\n     1\n     0\n   NaN\n     0\n     1\n   NaN",
            "in = 4×1 logical array\n\n   1\n   1\n   0\n   1",
            "on = 4×1 logical array\n\n   0\n   1\n   0\n   1",
        ],
    );
}

#[test]
fn plot_outputs() {
    check(
        concat!(
            "set(0, 'DefaultFigureVisible', 'off');\n",
            "[h, ax] = pareto([3 1 2]); numel(h), class(ax)\n",
            "h = fplot(@sin); isgraphics(h)\n",
            "h = fplot(@cos, @sin, [0 2*pi]); x = h.XData; [x(1) x(end)]\n",
            "y = h.YData; round([y(1) y(end)], 4)\n",
        ),
        &[
            "ans = 2",
            "ans = 'matlab.graphics.axis.Axes'",
            "ans = logical\n   1",
            "ans = 1×2\n\n     1     1",
            "ans = 1×2\n\n     0     0",
        ],
    );
}

#[test]
fn ignored_options() {
    check(
        concat!(
            "c = addcats(categorical({'b', 'a'}), {'y', 'z'}, 'Before', 'b'); categories(c)', double(c)\n",
            "c = addcats(categorical({'b', 'a'}), 'm', 'After', 'a'); categories(c)'\n",
            "try, polybuffer([0 0; 1 0], 'lines', 0.5, 'NumPoints', 8), catch e, disp(e.message), end\n",
            "d = distances(graph([1 2], [2 3], [5 7]), 'Method', 'unweighted')\n",
            "T = table([1; 1; 2], [3; 4; 5], 'VariableNames', {'g', 'x'});\n",
            "varfun(@mean, T, 'GroupingVariables', 'g')\n",
            "rowfun(@(x) x * 2, T, 'GroupingVariables', 'g', 'InputVariables', 'x')\n",
            "s = table2struct(T, 'ToScalar', true)\n",
            "F = fillmissing2([1 NaN 3; 4 5 NaN; 7 8 9], 'linear')\n",
            "c = textscan('1,,3', '%f %f %f', 'Delimiter', ',', 'EmptyValue', -1); [c{:}]\n",
            "abc1 = 1; abd2 = 2; xyz = 3; clear -regexp ^ab\n",
            "exist('abc1'), exist('xyz')\n",
            "q1 = 1; q22 = 2; clear q*2; exist('q1'), exist('q22')\n",
            "T2 = table([1; 2], [7; 8], 'VariableNames', {'g', 'y'});\n",
            "J = join(T, T2)\n",
            "s = extractBetween('abcdef', 2, 4)\n",
            "s = extractBetween(\"abcdef\", 2, 4)\n",
            "s = insertAfter('abcdef', 2, 'XX')\n",
            "s = insertBefore(\"abcdef\", 3, \"--\")\n",
            "[hh, mm, ss] = hms(duration(1, 2, 3.5))\n",
            "t = issortedrows(table([1; 2; 2], [3; 1; 2]))\n",
            "f = [tempname '.csv']; writematrix([\"a\" \"b\"; \"c\" \"d\"], f); disp(fileread(f)); delete(f)\n",
            "T3 = convertvars(T, @(v) all(v > 2), 'string'); class(T3.x), class(T3.g)\n",
        ),
        &[
            "ans = 1×4 cell\n\n    {'a'}    {'y'}    {'z'}    {'b'}",
            "ans = 1×2\n\n     4     1",
            "ans = 1×3 cell\n\n    {'a'}    {'m'}    {'b'}",
            "Parameter name must be 'JointType' or 'MiterLimit'.\n",
            "d = 3×3\n\n     0     1     2\n     1     0     1\n     2     1     0",
            "ans = 2×3 table\n\n    g    GroupCount    mean_x\n    _    __________    ______\n\n    1        2          3.5  \n    2        1            5  ",
            "ans = 3×3 table\n\n    g    GroupCount    Var3\n    _    __________    ____\n\n    1        2           6 \n    1        2           8 \n    2        1          10 ",
            "s = struct with fields:\n\n    g: [3×1 double]\n    x: [3×1 double]",
            "F = 3×3\n\n     1     2     3\n     4     5     6\n     7     8     9",
            "ans = 1×3\n\n     1    -1     3",
            "ans = 0",
            "ans = 1",
            "ans = 1",
            "ans = 0",
            "J = 3×3 table\n\n    g    x    y\n    _    _    _\n\n    1    3    7\n    1    4    7\n    2    5    8",
            "s = 1×1 cell\n\n    {'bcd'}",
            "s = \"bcd\"",
            "s = 'abXXcdef'",
            "s = \"ab--cdef\"",
            "hh = 1",
            "mm = 2",
            "ss = 3.5000",
            "t = logical\n   1",
            "a,b\nc,d\n\n",
            "ans = 'string'",
            "ans = 'double'",
        ],
    );
}

#[test]
fn format_modes() {
    check(
        concat!(
            "format short e\n",
            "a = pi, f = [pi -2.5 1e5], e = [1 2 3], disp([1.5 -2.25])\n",
            "format long e\n",
            "a = pi, f = [pi -2.5 1e5]\n",
            "format short g\n",
            "a = pi, c = -0.000123456, g = [0.001 0.5; -7 1234.5678]\n",
            "format long g\n",
            "b = 314159.26535, f = [pi -2.5 1e5]\n",
            "format bank\n",
            "c = -0.000123456, n = [NaN Inf -Inf 0], f = [pi -2.5 1e5]\n",
            "format rat\n",
            "a = pi, f = [pi -2.5 1e5], g = [0.001 0.5; -7 1234.5678]\n",
            "format hex\n",
            "a = pi, n = [NaN Inf -Inf 0], s = single(pi)\n",
            "format +\n",
            "e = [1 -2 0], disp([NaN -Inf 3])\n",
            "format compact\n",
            "x = 0.5\n",
            "format\n",
            "x = 0.5\n",
            "format longG\n",
            "x = 1/3\n",
            "format\n",
        ),
        &[
            "a = 3.1416e+00",
            "f = 1×3\n\n   3.1416e+00  -2.5000e+00   1.0000e+05",
            "e = 1×3\n\n     1     2     3",
            "   1.5000e+00  -2.2500e+00\n",
            "a = 3.141592653589793e+00",
            "f = 1×3\n\n     3.141592653589793e+00    -2.500000000000000e+00     1.000000000000000e+05",
            "a = 3.1416",
            "c = -0.00012346",
            "g = 2×2\n\n        0.001          0.5\n           -7       1234.6",
            "b = 314159.26535",
            "f = 1×3\n\n          3.14159265358979                      -2.5                    100000",
            "c = -0.00",
            "n = 1×4\n\n           NaN           Inf          -Inf             0",
            "f = 1×3\n\n          3.14         -2.50     100000.00",
            "a = 355/113",
            "f = 1×3\n\n     355/113         -5/2       100000       ",
            "g = 2×2\n\n       1/1000         1/2     \n      -7          45679/37    ",
            "a = 400921fb54442d18",
            "n = 1×4\n\n   fff8000000000000   7ff0000000000000   fff0000000000000   0000000000000000",
            "s = single\n   40490fdb",
            "e = 1×3\n\n+- ",
            "+-+\n",
            "x = +",
            "x = 0.5000",
            "x = 0.333333333333333",
        ],
    );
}

#[test]
fn missing_functions() {
    check(
        concat!(
            "t = [license('test', 'MATLAB'), license('test', 'Symbolic_Toolbox')]\n",
            "syms x y\n",
            "f = symfun(x^2 + y, [x y]);\n",
            "f(2, 3)\n",
            "argnames(f)\n",
            "mc = ?containers.Map; mc.Name\n",
            "mc = meta.class.fromName('double'); mc.Name\n",
            "isempty(meta.class.fromName('NoSuchClass'))\n",
            "p = [powermod(3, -1, 7), powermod(2, -1, 4), powermod(5, -2, 13)]\n",
            "G = graph(table([1 2; 2 3], [5; 6], 'VariableNames', {'EndNodes', 'Weight'})); G.Edges\n",
            "G = graph([0 1 1; 1 0 0; 1 0 0], 'upper'); G.Edges\n",
            "G = graph([0 2 0; 0 0 3; 4 0 0], 'lower'); G.Edges\n",
            "I = dither([0.1 0.6 0.3; 0.8 0.5 0.2; 0.4 0.9 0.7])\n",
            "I = dither(uint8([10 200; 130 60]))\n",
            "syms a b\n",
            "has([a + b, b], a)\n",
            "c = children([a + b, a * b]); c{1}\n",
            "subs(a + b, [a b], {[1 2], [3 4]})\n",
            "set(0, 'DefaultFigureVisible', 'off');\n",
            "figure; h = findall(0, 'Type', 'figure'); numel(h) >= 1\n",
            "h = trimesh([1 2 3], [0 1 0], [0 0 1]); class(h)\n",
            "h = errorbar(1:3, [2 3 4], [0.5 0.5 0.5], 'o-'); [h.LineStyle h.Marker]\n",
            "h = scatter3([1 2], [3 4], [5 6], 'filled'); h.MarkerFaceColor\n",
        ),
        &[
            "t = 1×2\n\n     1     1",
            "ans = 7",
            "ans = \n[x, y]",
            "ans = 'containers.Map'",
            "ans = 'double'",
            "ans = logical\n   1",
            "p = 1×3\n\n     5   NaN    12",
            "ans = 2×2 table\n\n    EndNodes    Weight\n    ________    ______\n\n     1    2       5   \n     2    3       6   ",
            "ans = 2×2 table\n\n    EndNodes    Weight\n    ________    ______\n\n     1    2       1   \n     1    3       1   ",
            "ans = 1×2 table\n\n    EndNodes    Weight\n    ________    ______\n\n     1    3       4   ",
            "I = 3×3 logical array\n\n   0   1   0\n   1   0   0\n   0   1   1",
            "I = 2×2 logical array\n\n   0   1\n   1   0",
            "ans = 1×2 logical array\n\n   1   0",
            "ans = 1×2 cell\n\n    {[a]}    {[b]}",
            "ans = \n[4, 6]",
            "ans = logical\n   1",
            "ans = 'matlab.graphics.chart.primitive.Line'",
            "ans = '-o'",
            "ans = 'flat'",
        ],
    );
}

#[test]
fn graphics_forms() {
    check(
        concat!(
            "set(0, 'DefaultFigureVisible', 'off');\n",
            "figure; h = bar([1 2; 3 4], 'stacked'); h(1).BarLayout, h(2).YEndPoints\n",
            "figure; h = bar([1 2 3; 4 5 6]); h(1).BarLayout, round([h(1).XEndPoints; h(2).XEndPoints; h(3).XEndPoints], 4)\n",
            "figure; h = bar([1 2 3]); h.BarLayout, h.XEndPoints\n",
            "figure; h = bubblechart([1 2], [3 4], [5 6], 'r', 'MarkerFaceAlpha', 0.5); h.MarkerFaceAlpha\n",
            "figure; h = fsurf(@(u, v) cos(u), @(u, v) sin(u), @(u, v) v); isgraphics(h)\n",
            "h = gobjects(1, 3); size(h)\n",
            "h(2) = plot(1:3); class(h), isgraphics(h)\n",
            "g = gobjects(2); size(g)\n",
            "try, stlread('no_such_file.stl'), catch e, disp(e.message), end\n",
            "try, stlwrite('w.stl', [1 2 3], [0 0 0; 1 0 0; 0 1 0]), catch e, disp(e.message), end\n",
        ),
        &[
            "ans = 'stacked'",
            "ans = 1×2\n\n     3     7",
            "ans = 'grouped'",
            "ans = 3×2\n\n    0.7778    1.7778\n    1.0000    2.0000\n    1.2222    2.2222",
            "ans = 'grouped'",
            "ans = 1×3\n\n     1     2     3",
            "ans = 0.5000",
            "ans = logical\n   1",
            "ans = 1×2\n\n     1     3",
            "ans = 'matlab.graphics.Graphics'",
            "ans = 1×3 logical array\n\n   0   1   0",
            "ans = 1×2\n\n     2     2",
            "Unable to open file 'no_such_file.stl'.\n",
            "Input argument must be a triangulation object.\n",
        ],
    );
}

#[test]
fn former_stubs() {
    check(
        concat!(
            "s = spparms('spumoni')\n",
            "spparms('spumoni', 1); s = spparms('spumoni')\n",
            "spparms('default'); [k, v] = spparms; size(k), v(2)\n",
            "set(0, 'DefaultFigureVisible', 'off');\n",
            "figure; plot3(1:3, 1:3, [0 5 10]); z = zticks, za = ztickangle, zf = ztickformat, zs = zscale\n",
            "zticks([1 2 3]); zticklabels({'a', 'b', 'c'}); zl = zticklabels\n",
            "syms x\n",
            "sympref('FloatingPointOutput', true); y = [sym(1/3) + x, sym(2)/7, sym(5)], p = sym(pi)\n",
            "sympref('default');\n",
            "tc = tscollection({timeseries([1; 2], [0; 1], 'Name', 'x'), timeseries([3; 4], [0; 1], 'Name', 'm')});\n",
            "tc = settimeseriesnames(tc, 'x', 'a'); gettimeseriesnames(tc)\n",
            "c = matlab.lang.correction.ReplaceIdentifierCorrection('a', 'b')\n",
            "ME = addCorrection(MException('a:b', 'Main'), c); class(ME.Correction)\n",
        ),
        &[
            "s = 0",
            "s = 1",
            "ans = 1×2\n\n    19     7",
            "ans = 1.1000",
            "z = 1×6\n\n     0     2     4     6     8    10",
            "za = 0",
            "zf = '%g'",
            "zs = 'linear'",
            "zl = 3×1 cell\n\n    {'a'}\n    {'b'}\n    {'c'}",
            "y = \n[x + 0.3333, 0.2857, 5]",
            "p = 3.1416",
            "ans = 1×2 cell\n\n    {'m'}    {'a'}",
            "c = \n  ReplaceIdentifierCorrection with properties:\n\n     Identifier: \"a\"\n    Replacement: \"b\"",
            "ans = 'matlab.lang.correction.ReplaceIdentifierCorrection'",
        ],
    );
}

#[test]
fn files_and_classes() {
    check(
        concat!(
            "d = tempname; mkdir(d);\n",
            "w = fullfile(d, 'a8.wav'); y = [0 0.5 -0.5 0.99]'; audiowrite(w, y, 8000, 'BitsPerSample', 8);\n",
            "[z, fs] = audioread(w); z', fs\n",
            "zn = audioread(w, 'native'); class(zn), zn'\n",
            "p = fullfile(d, 'ix.png'); imwrite(uint8([0 1; 2 3]), [0 0 0; 1 0 0; 0 1 0; 0 0 1], p);\n",
            "[X, map] = imread(p); X, map\n",
            "g = fullfile(d, 'g.gif'); imwrite(uint8([0 128; 200 255]), g);\n",
            "[X, map] = imread(g); X, size(map), map(201, :)\n",
            "x = fullfile(d, 'x.xlsx'); writematrix([1 2], x); [st, sh, fmt] = xlsfinfo(x)\n",
            "m = fullfile(d, 'm.mat'); v = 5; save(m, 'v');\n",
            "mf = matfile(m); try, mf.v = 7; catch e, disp(e.message), end\n",
            "mf = matfile(m, 'Writable', true); mf.v = 7; mf.u = 3; clear v u; load(m); [v u]\n",
            "events('containers.Map')\n",
            "p = properties('double')\n",
            "rmdir(d, 's');\n",
        ),
        &[
            "ans = 1×4\n\n         0    0.5000   -0.5000    0.9844",
            "fs = 8000",
            "ans = 'uint8'",
            "ans = 1×4 uint8 row vector\n\n   128   192    64   254",
            "X = 2×2 uint8 matrix\n\n   0   1\n   2   3",
            "map = 4×3\n\n     0     0     0\n     1     0     0\n     0     1     0\n     0     0     1",
            "X = 2×2 uint8 matrix\n\n     0   128\n   200   255",
            "ans = 1×2\n\n   256     3",
            "ans = 1×3\n\n    0.7843    0.7843    0.7843",
            "st = 'Microsoft Excel Spreadsheet'",
            "sh = 1×1 cell\n\n    {'Sheet1'}",
            "fmt = 'xlOpenXMLWorkbook'",
            "Cannot change 'v' because Properties.Writable is false.  To modify 'v', set Properties.Writable to true.\n",
            "ans = 1×2\n\n     7     3",
            "Events for class containers.Map:\n\n    ObjectBeingDestroyed\n\n",
            "p =\n\n  0×1 empty cell array",
        ],
    );
}

#[test]
fn wiki_regressions() {
    check(
        concat!(
            "a = [2 4 6]; b = [1 2 3];\n",
            "a ./ b\n",
            "x = 4;\n",
            "x ./ 2\n",
            "a ...\n",
            "    + 1\n",
            "d = tempname; mkdir(d); f = fullfile(d, 'semi.txt');\n",
            "fid = fopen(f, 'w'); fprintf(fid, 'a;b\\n1;2\\n3;4\\n'); fclose(fid);\n",
            "T = readtable(f); size(T), T.b'\n",
            "g = fullfile(d, 'tab.txt'); fid = fopen(g, 'w'); fprintf(fid, 'a\\tb\\n1\\t2\\n'); fclose(fid);\n",
            "T = readtable(g); size(T)\n",
            "T = readtable(f, 'Delimiter', ';'); size(T)\n",
            "rmdir(d, 's');\n",
            "properties('double')\n",
            "try, assert(false, 'my:id', 'msg %d', 3), catch e, disp(e.identifier), disp(e.message), end\n",
            "try, assert(false, 'plain'), catch e, disp(['[' e.identifier ']']), disp(e.message), end\n",
            "try, assert(false, 'a:b'), catch e, disp(['[' e.identifier ']']), disp(e.message), end\n",
        ),
        &[
            "ans = 1×3\n\n     2     2     2",
            "ans = 2",
            "ans = 1×3\n\n     3     5     7",
            "ans = 1×2\n\n     2     2",
            "ans = 1×2\n\n     2     4",
            "ans = 1×2\n\n     1     2",
            "ans = 1×2\n\n     2     2",
            "\nNo properties for class double.\n\n",
            "my:id\n",
            "msg 3\n",
            "[]\n",
            "plain\n",
            "[MATLAB:error:missingMessageArgument]\n",
            "If you specify a message identifier argument, you must specify the message text argument.\n",
        ],
    );
}

#[test]
fn matching_and_contours() {
    check(
        concat!(
            "p = dmperm(sparse([1 0 1; 0 0 1; 1 1 0]))\n",
            "p = dmperm(sparse([0 1; 1 0]))\n",
            "p = dmperm(sparse([1 1 0; 1 0 0; 0 1 1]))\n",
            "p = dmperm(sparse([1 1 1; 1 0 0; 1 0 0]))\n",
            "p = dmperm(sparse([0 0; 1 1; 0 1]))\n",
            "p = dmperm(magic(4) > 8)\n",
            "C = contours(peaks(5), [0 0]); size(C), C(:, 1)'\n",
        ),
        &[
            "p = 1×3\n\n     1     3     2",
            "p = 1×2\n\n     2     1",
            "p = 1×3\n\n     2     1     3",
            "p = 1×3\n\n     2     1     0",
            "p = 1×2\n\n     2     3",
            "p = 1×4\n\n     1     2     4     3",
            "ans = 1×2\n\n     2    15",
            "ans = 1×2\n\n     0     5",
        ],
    );
}

#[test]
fn implicit_surfaces_and_classes() {
    check(
        concat!(
            "set(0, 'DefaultFigureVisible', 'off');\n",
            "h = fimplicit3(@(x, y, z) x.^2 + y.^2 + z.^2 - 1, [-2 2]); class(h), h.XRange, h.ZRange, h.MeshDensity, h.EdgeColor, h.FaceAlpha\n",
            "h2 = fimplicit3(@(x, y, z) x + y - z, [-1 1 -2 2 -3 3], 'MeshDensity', 20); h2.YRange, h2.ZRange, h2.MeshDensity\n",
            "e = [exist('dynamicprops') exist('function_handle') exist('containers.Map') exist('matlab.graphics.Graphics') exist('matlab.graphics.GraphicsPlaceholder')]\n",
        ),
        &[
            "ans = 'matlab.graphics.function.ImplicitFunctionSurface'",
            "ans = 1×2\n\n    -2     2",
            "ans = 1×2\n\n    -2     2",
            "ans = 35",
            "ans = 1×3\n\n     0     0     0",
            "ans = 1",
            "ans = 1×2\n\n    -2     2",
            "ans = 1×2\n\n    -3     3",
            "ans = 20",
            "e = 1×5\n\n     2     2     8     8     8",
        ],
    );
}

#[test]
fn rowfilter_and_nested() {
    check(
        concat!(
            "G = nested(4)\n",
            "G = nested(8)\n",
            "T = table([1; 2; 3; 4], {'a'; 'b'; 'c'; 'd'}, [0.5; 1.5; 2.5; 3.5], 'VariableNames', {'X', 'Name', 'Y'});\n",
            "rf = rowfilter(T);\n",
            "f1 = rf.X <= 1 | rf.Name == \"d\"\n",
            "f2 = (rf.X > 1) & (rf.Y < 3)\n",
            "f3 = ~(rf.X == 2)\n",
            "f4 = rf.Name ~= 'a'\n",
            "f5 = 2 < rf.X\n",
            "v = rf.X\n",
            "T(f2, :)\n",
            "T(f3, :)\n",
            "T(f5, 'Y')\n",
            "try, rf.Z, catch e, disp(e.message), end\n",
        ),
        &[
            "G = 4×4\n\n     0     0     0     0\n     0     3     1     0\n     0     4     2     0\n     0     0     0     0",
            "G = 8×8\n\n     0     0     0     0     0     0     0     0\n     0     3     1    31    13    17    15     0\n     0     4     2    32    14    18    16     0\n     0    11    12    33    28    29    30     0\n     0     5     6    34    19    25    22     0\n     0     9    10    35    20    26    23     0\n     0     7     8    36    21    27    24     0\n     0     0     0     0     0     0     0     0",
            "f1 = \n  RowFilter with constraints:\n\n    X <= 1 | Name == \"d\"\n\n  VariableNames: X, Name, Y\n",
            "f2 = \n  RowFilter with constraints:\n\n    X > 1 & Y < 3\n\n  VariableNames: X, Name, Y\n",
            "f3 = \n  RowFilter with constraint:\n\n    ~(X == 2)\n\n  VariableNames: X, Name, Y\n",
            "f4 = \n  RowFilter with constraint:\n\n    Name ~= 'a'\n\n  VariableNames: X, Name, Y\n",
            "f5 = \n  RowFilter with constraint:\n\n    X > 2\n\n  VariableNames: X, Name, Y\n",
            "v = \n  RowFilter with no constraints on variable: X\n\n    X <unconstrained>\n\n  VariableNames: X, Name, Y\n",
            "ans = 2×3 table\n\n    X    Name      Y \n    _    _____    ___\n\n    2    {'b'}    1.5\n    3    {'c'}    2.5",
            "ans = 3×3 table\n\n    X    Name      Y \n    _    _____    ___\n\n    1    {'a'}    0.5\n    3    {'c'}    2.5\n    4    {'d'}    3.5",
            "ans = 2×1 table\n\n     Y \n    ___\n\n    2.5\n    3.5",
            "Unrecognized variable name for filtering. Select a variable for filtering using one of the following variable names: X, Name, Y.\n",
        ],
    );
}

#[test]
fn xml_dom() {
    check(
        concat!(
            "f = [tempname '.xml']; fid = fopen(f, 'w');\n",
            "fprintf(fid, '<?xml version=\"1.0\" encoding=\"utf-8\"?>\\n<library name=\"city\">\\n  <book id=\"1\">\\n    <title>Dune</title>\\n    <year>1965</year>\\n  </book>\\n  <book id=\"2\">\\n    <title>Emma</title>\\n    <year>1815</year>\\n  </book>\\n</library>\\n');\n",
            "fclose(fid);\n",
            "doc = xmlread(f); delete(f);\n",
            "class(doc)\n",
            "root = doc.getDocumentElement(); char(root.getNodeName()), char(root.getAttribute('name'))\n",
            "books = doc.getElementsByTagName('book'); n = books.getLength()\n",
            "b = books.item(0); char(b.getAttribute('id'))\n",
            "s = char(b.getElementsByTagName('title').item(0).getTextContent())\n",
            "y = str2double(b.getElementsByTagName('year').item(0).getTextContent())\n",
            "kids = root.getChildNodes(); kids.getLength(), char(kids.item(1).getNodeName()), kids.item(1).getNodeType(), char(kids.item(0).getNodeName())\n",
            "[b.hasAttribute('id'), b.hasAttribute('zz')]\n",
            "a = root.getAttributes(); a.getLength(), char(a.item(0).getName()), char(a.item(0).getValue())\n",
            "char(root.getFirstChild().getNextSibling().getNodeName())\n",
            "char(root.getElementsByTagName('title').item(1).getFirstChild().getNodeValue())\n",
            "char(books.item(0).getParentNode().getNodeName())\n",
            "root.getElementsByTagName('nothing').getLength()\n",
            "d = com.mathworks.xml.XMLUtils.createDocument('config');\n",
            "r = d.getDocumentElement(); r.setAttribute('version', '2');\n",
            "p = d.createElement('param'); p.setAttribute('name', 'alpha'); p.appendChild(d.createTextNode('0.5')); r.appendChild(p);\n",
            "q = d.createElement('group'); it = d.createElement('item'); it.setTextContent('x & y'); q.appendChild(it); r.appendChild(q);\n",
            "r.appendChild(d.createComment('end')); r.appendChild(d.createElement('empty'));\n",
            "txt = xmlwrite(d)\n",
        ),
        &[
            "ans = 'org.apache.xerces.dom.DeferredDocumentImpl'",
            "ans = 'library'",
            "ans = 'city'",
            "n = 2",
            "ans = '1'",
            "s = 'Dune'",
            "y = 1965",
            "ans = 5",
            "ans = 'book'",
            "ans = 1",
            "ans = '#text'",
            "ans = 1×2 logical array\n\n   1   0",
            "ans = 1",
            "ans = 'name'",
            "ans = 'city'",
            "ans = 'book'",
            "ans = 'Emma'",
            "ans = 'library'",
            "ans = 0",
            "txt = \n    '<?xml version=\"1.0\" encoding=\"utf-8\"?>\n     <config version=\"2\">\n        <param name=\"alpha\">0.5</param>\n        <group>\n           <item>x &amp; y</item>\n        </group>\n        <!--end--><empty/>\n     </config>'",
        ],
    );
}
