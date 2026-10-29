//! MATLAB-compatibility tests for complex numbers, numeric classes, N-D arrays, string and
//! struct arrays, classdef classes, date/time, tables, and Symbolic Math.

use livre_lang::{Interpreter, OutputKind, Value};

fn run_in(it: &mut Interpreter, src: &str) -> Vec<String> {
    let _ = it.run_source(src);
    it.take_outputs()
        .into_iter()
        .map(|o| match o.kind {
            OutputKind::Text(t) => t,
            OutputKind::Value { text, .. } => text,
            OutputKind::Figure(f) => format!("<figure {}>", f.id),
            OutputKind::Warning(w) => format!("Warning: {w}"),
            OutputKind::Error(e) => format!("Error: {e}"),
        })
        .collect()
}

fn run(src: &str) -> (Interpreter, Vec<String>) {
    let mut it = Interpreter::new();
    let out = run_in(&mut it, src);
    (it, out)
}

fn no_errors(out: &[String]) {
    assert!(out.iter().all(|o| !o.starts_with("Error")), "{out:#?}");
}

fn num(it: &Interpreter, name: &str) -> Vec<f64> {
    match it.get_var(name) {
        Some(Value::Num(m)) => m.data.clone(),
        other => panic!("{name} = {other:?}"),
    }
}

fn imag(it: &Interpreter, name: &str) -> Vec<f64> {
    match it.get_var(name) {
        Some(Value::Num(m)) => m.im.clone().unwrap_or_else(|| vec![0.0; m.numel()]),
        other => panic!("{name} = {other:?}"),
    }
}

fn class(it: &Interpreter, name: &str) -> String {
    it.get_var(name).unwrap().class_name()
}

fn dims(it: &Interpreter, name: &str) -> Vec<usize> {
    it.get_var(name).unwrap().dims()
}

fn text(it: &Interpreter, name: &str) -> String {
    match it.get_var(name).unwrap() {
        Value::Sym(s) => s.disp(),
        v => v.text().unwrap(),
    }
}

// ---- complex --------------------------------------------------------------------------------

#[test]
fn complex_arithmetic_and_display() {
    let (it, out) = run("z = 3 + 4i\nm = abs(z);\nw = (1+2i)*(3-1i);\nr = sqrt(-4);\nc = z';\nn = (1+2i) - 2i;\nok = isreal(n);");
    no_errors(&out);
    assert_eq!(out[0], "z = 3.0000 + 4.0000i");
    assert_eq!(num(&it, "m"), vec![5.0]);
    assert_eq!((num(&it, "w"), imag(&it, "w")), (vec![5.0], vec![5.0]));
    assert_eq!((num(&it, "r"), imag(&it, "r")), (vec![0.0], vec![2.0]));
    assert_eq!(imag(&it, "c"), vec![-4.0]);
    assert_eq!(num(&it, "ok"), vec![1.0], "all-zero imaginary parts narrow to real");
}

#[test]
fn complex_roots_and_functions() {
    let (it, out) = run("r = roots([1 0 1]);\nq = (-8)^(1/3);\nl = log(-1);");
    no_errors(&out);
    let mut im = imag(&it, "r");
    im.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!((im[0] + 1.0).abs() < 1e-9 && (im[1] - 1.0).abs() < 1e-9, "{im:?}");
    assert!((num(&it, "q")[0] - 1.0).abs() < 1e-12 && (imag(&it, "q")[0] - 3f64.sqrt()).abs() < 1e-12);
    assert!((imag(&it, "l")[0] - std::f64::consts::PI).abs() < 1e-12);
}

// ---- numeric classes ------------------------------------------------------------------------

#[test]
fn integer_saturation_and_rounding() {
    let (it, out) = run("a = int8(100) + int8(100);\nb = int8(-7) / int8(2);\nc = uint8(5) - 10;\nd = int16(5) * 2.6;\ne = int8(1) + 1;\nm = intmax('int32');\nx = int8(5)");
    no_errors(&out);
    assert_eq!(num(&it, "a"), vec![127.0]);
    assert_eq!(num(&it, "b"), vec![-4.0], "-3.5 rounds half away from zero");
    assert_eq!(num(&it, "c"), vec![0.0]);
    assert_eq!(num(&it, "d"), vec![13.0]);
    assert_eq!(class(&it, "e"), "int8");
    assert_eq!(num(&it, "m"), vec![2147483647.0]);
    assert_eq!(out.last().unwrap(), "x = int8\n   5");
}

#[test]
fn mixed_integer_classes_error() {
    let (_, out) = run("x = int8(1) + int16(1);");
    assert!(out[0].starts_with("Error: Integers can only be combined"), "{out:?}");
}

#[test]
fn single_precision() {
    let (it, out) = run("s = single(1) / 3;\nk = class(s + 1);");
    no_errors(&out);
    assert_eq!(text(&it, "k"), "single");
    assert_eq!(num(&it, "s")[0], (1.0f32 / 3.0) as f64);
}

// ---- N-D arrays -----------------------------------------------------------------------------

#[test]
fn nd_arrays() {
    let (it, out) = run("A = zeros(2,3,4);\nA(:,:,2) = ones(2,3);\nt = sum(A(:));\nB = cat(3, [1 2], [3 4]);\ns = sum(B, 3);\nn = ndims(A);\nq = squeeze(ones(1,1,3));\nP = permute(reshape(1:6, 2, 3), [2 1]);\nC = ones(2,2,2) + reshape([1 2], 1, 1, 2);\nlast = A(end);");
    no_errors(&out);
    assert_eq!(dims(&it, "A"), vec![2, 3, 4]);
    assert_eq!(num(&it, "t"), vec![6.0]);
    assert_eq!(dims(&it, "B"), vec![1, 2, 2]);
    assert_eq!(num(&it, "s"), vec![4.0, 6.0]);
    assert_eq!(num(&it, "n"), vec![3.0]);
    assert_eq!(dims(&it, "q"), vec![3, 1]);
    assert_eq!(num(&it, "P"), vec![1.0, 3.0, 5.0, 2.0, 4.0, 6.0]);
    assert_eq!(num(&it, "C"), vec![2.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0, 3.0], "implicit expansion along dimension 3");
    assert_eq!(num(&it, "last"), vec![0.0]);
}

#[test]
fn nd_display_pages() {
    let (_, out) = run("B = cat(3, [1 2], [3 4])");
    assert!(out[0].starts_with("B = 1×2×2"), "{out:?}");
    assert!(out[0].contains("B(:,:,1) =") && out[0].contains("B(:,:,2) ="), "{out:?}");
}

// ---- string arrays --------------------------------------------------------------------------

#[test]
fn string_arrays() {
    let (it, out) = run("s = [\"a\" \"bb\" \"ccc\"];\nL = strlength(s);\nt = s + \"x\";\nm = s == \"bb\";\nj = join(s, \"-\");\np = split(\"a,b\", \",\");\nn = \"n = \" + 5;\nu = upper(s(2));\ne = strings(2,1);");
    no_errors(&out);
    assert_eq!(num(&it, "L"), vec![1.0, 2.0, 3.0]);
    match it.get_var("t").unwrap() {
        Value::Str(a) => assert_eq!(a.data, vec![Some("ax".to_string()), Some("bbx".into()), Some("cccx".into())]),
        other => panic!("{other:?}"),
    }
    assert_eq!(num(&it, "m"), vec![0.0, 1.0, 0.0]);
    assert_eq!(text(&it, "j"), "a-bb-ccc");
    assert_eq!(dims(&it, "p"), vec![2, 1]);
    assert_eq!(text(&it, "n"), "n = 5");
    assert_eq!(text(&it, "u"), "BB");
    assert_eq!(dims(&it, "e"), vec![2, 1]);
}

#[test]
fn string_array_display() {
    let (_, out) = run("s = [\"a\" \"bb\"]");
    assert_eq!(out[0], "s = 1×2 string array\n\n    \"a\"    \"bb\"");
}

// ---- struct arrays --------------------------------------------------------------------------

#[test]
fn struct_arrays() {
    let (it, out) = run("s(1).name = 'A'; s(2).name = 'B'; s(2).val = 7;\nn = numel(s);\nnames = {s.name};\nt = struct('a', {1, 2, 3});\nv = [t.a];\nd = arrayfun(@(e) e.a * 2, t);\nf = isfield(s, 'val');\ns");
    no_errors(&out);
    assert_eq!(num(&it, "n"), vec![2.0]);
    assert_eq!(dims(&it, "names"), vec![1, 2]);
    assert_eq!(dims(&it, "t"), vec![1, 3]);
    assert_eq!(num(&it, "v"), vec![1.0, 2.0, 3.0]);
    assert_eq!(num(&it, "d"), vec![2.0, 4.0, 6.0]);
    assert_eq!(num(&it, "f"), vec![1.0]);
    assert!(out.last().unwrap().starts_with("s = 1×2 struct array with fields:"), "{out:?}");
}

// ---- classes --------------------------------------------------------------------------------

fn class_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("matlivre-classes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("Point.m"),
        "classdef Point\n    properties\n        X = 0\n        Y = 0\n    end\n    properties (Dependent)\n        R\n    end\n    methods\n        function p = Point(x, y)\n            if nargin > 0\n                p.X = x;\n                p.Y = y;\n            end\n        end\n        function r = get.R(p)\n            r = sqrt(p.X^2 + p.Y^2);\n        end\n        function p = plus(a, b)\n            p = Point(a.X + b.X, a.Y + b.Y);\n        end\n        function p = scale(p, k)\n            p.X = p.X * k;\n            p.Y = p.Y * k;\n        end\n    end\nend\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Counter.m"),
        "classdef Counter < handle\n    properties\n        Count = 0\n    end\n    methods\n        function increment(obj)\n            obj.Count = obj.Count + 1;\n        end\n    end\n    methods (Static)\n        function c = make()\n            c = Counter();\n        end\n    end\nend\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Shape.m"),
        "classdef Shape\n    properties\n        Name = 'shape'\n    end\n    methods\n        function s = Shape(name)\n            if nargin > 0\n                s.Name = name;\n            end\n        end\n        function d = describe(s)\n            d = sprintf('%s with area %.2f', s.Name, s.area());\n        end\n        function a = area(~)\n            a = 0;\n        end\n    end\nend\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Circle.m"),
        "classdef Circle < Shape\n    properties\n        Radius = 1\n    end\n    methods\n        function c = Circle(r)\n            c@Shape('circle');\n            c.Radius = r;\n        end\n        function a = area(c)\n            a = pi * c.Radius^2;\n        end\n    end\nend\n",
    )
    .unwrap();
    std::fs::write(dir.join("addtwo.m"), "function y = addtwo(x)\n    y = x + 2;\nend\n").unwrap();
    dir
}

#[test]
fn value_and_handle_classes() {
    let mut it = Interpreter::new();
    it.set_search_path(vec![class_dir()]);
    let out = run_in(
        &mut it,
        "p = Point(3, 4);\nr = p.R;\nq = p + Point(1, 1);\nqx = q.X;\np2 = p.scale(2);\npx = p.X;\np2x = p2.X;\nc = Counter();\nd = c;\nc.increment();\nincrement(d);\nn = c.Count;\nm = Counter.make();\ncls = class(p);\nh = isa(c, 'handle');\nz = addtwo(5);",
    );
    no_errors(&out);
    assert_eq!(num(&it, "r"), vec![5.0], "Dependent property via get.R");
    assert_eq!(num(&it, "qx"), vec![4.0], "operator overloading via plus");
    assert_eq!(num(&it, "px"), vec![3.0], "value semantics: p unchanged");
    assert_eq!(num(&it, "p2x"), vec![6.0]);
    assert_eq!(num(&it, "n"), vec![2.0], "handle semantics: both references see the change");
    assert_eq!(text(&it, "cls"), "Point");
    assert_eq!(num(&it, "h"), vec![1.0]);
    assert_eq!(num(&it, "z"), vec![7.0], "function files on the path");
    assert!(matches!(it.get_var("m"), Some(Value::Object(_))));
}

#[test]
fn inheritance_and_display() {
    let mut it = Interpreter::new();
    it.set_search_path(vec![class_dir()]);
    let out = run_in(&mut it, "c = Circle(2);\nd = c.describe();\nok = isa(c, 'Shape');\nc");
    no_errors(&out);
    assert_eq!(text(&it, "d"), "circle with area 12.57");
    assert_eq!(num(&it, "ok"), vec![1.0]);
    let shown = out.last().unwrap();
    assert!(shown.contains("Circle with properties:") && shown.contains("Radius: 2") && shown.contains("Name: 'circle'"), "{shown}");
}

// ---- datetime / duration --------------------------------------------------------------------

#[test]
fn datetime_arithmetic() {
    let (it, out) = run("t = datetime(2018,6,1,12,0,0);\nh = t.Hour;\nu = t + hours(3);\nd = datetime(2020,1,10) - datetime(2020,1,1);\nnh = hours(d);\nk = caldays(between(datetime(2016,1,1), datetime(2018,6,1), 'Day'));\nf = datetime(2021,1,31) + calmonths(1);\ny = year(t);\nr = datetime(2020,1,1):days(1):datetime(2020,1,5);\nn = numel(r);\nt");
    no_errors(&out);
    assert_eq!(num(&it, "h"), vec![12.0]);
    assert_eq!(num(&it, "nh"), vec![216.0]);
    assert_eq!(num(&it, "k"), vec![882.0]);
    assert_eq!(num(&it, "y"), vec![2018.0]);
    assert_eq!(num(&it, "n"), vec![5.0]);
    match it.get_var("u").unwrap() {
        Value::Time(tt) => assert_eq!(tt.format_elem(0), "01-Jun-2018 15:00:00"),
        other => panic!("{other:?}"),
    }
    match it.get_var("f").unwrap() {
        Value::Time(tt) => assert_eq!(tt.format_elem(0), "28-Feb-2021"),
        other => panic!("{other:?}"),
    }
    assert_eq!(out.last().unwrap(), "t = datetime\n   01-Jun-2018 12:00:00");
}

#[test]
fn durations_display() {
    let (_, out) = run("a = hours(1.5)\nb = minutes(90) + seconds(30)");
    assert_eq!(out[0], "a = duration\n   1.5 hr");
    assert_eq!(out[1], "b = duration\n   90.5 min");
}

#[test]
fn solar_panel_time_code() {
    // The date/time part of the SolarPanelEstimator live script shipped with MATLAB.
    let src = "TZ = 'UTC-5';\njanuary1  = datetime(2016,1,1,'TimeZone',TZ);\nlocalTime = datetime(2018,6,1,12,0,0,'TimeZone',TZ);\nd = caldays(between(january1,localTime,'Day'));\nsolarTime = localTime + minutes(-3.2);\nhr = solarTime.Hour; mn = solarTime.Minute;";
    let (it, out) = run(src);
    no_errors(&out);
    assert_eq!(num(&it, "d"), vec![882.0]);
    assert_eq!(num(&it, "hr"), vec![11.0]);
    assert_eq!(num(&it, "mn"), vec![56.0]);
}

// ---- tables ---------------------------------------------------------------------------------

#[test]
fn tables() {
    let (it, out) = run("x = [3;1;2]; name = {'c';'a';'b'};\nT = table(x, name);\nh = height(T);\nv = T.x(2);\nT.y = T.x * 2;\nw = width(T);\nS = sortrows(T, 'x');\nfirst = S.name{1};\nR = T(2, :);\nrh = height(R);\nT{2, 'x'} = 10;\nb = T.x(2);\nvn = T.Properties.VariableNames;\nsub = T(T.x > 2, :);\nhs = height(sub);\nT");
    no_errors(&out);
    assert_eq!(num(&it, "h"), vec![3.0]);
    assert_eq!(num(&it, "v"), vec![1.0]);
    assert_eq!(num(&it, "w"), vec![3.0]);
    assert_eq!(text(&it, "first"), "a");
    assert_eq!(num(&it, "rh"), vec![1.0]);
    assert_eq!(num(&it, "b"), vec![10.0]);
    assert_eq!(dims(&it, "vn"), vec![1, 3]);
    assert_eq!(num(&it, "hs"), vec![2.0]);
    let shown = out.last().unwrap();
    assert!(shown.starts_with("T = 3×3 table") && shown.contains("name") && shown.contains("{'c'}"), "{shown}");
}

#[test]
fn table_csv_round_trip() {
    let path = std::env::temp_dir().join(format!("matlivre-{}.csv", std::process::id()));
    let p = path.display().to_string().replace('\\', "/");
    let (it, out) = run(&format!("T = table([1;2], {{'x';'y'}}, 'VariableNames', {{'n','s'}});\nwritetable(T, '{p}');\nU = readtable('{p}');\nok = isequal(T.n, U.n);"));
    no_errors(&out);
    assert_eq!(num(&it, "ok"), vec![1.0]);
}

// ---- symbolic -------------------------------------------------------------------------------

#[test]
fn symbolic_basics() {
    let (it, out) = run("syms x\nf = x^2 + 2*x + 1\ndf = diff(f);\nF = int(f);\nI = int(x, 0, 1);\nfs = factor(x^2 - 1);\ns = simplify((x^2 - 1)/(x - 1));\nr = solve(x^2 - 5*x + 6 == 0);\nv = subs(f, x, 2);\nd = double(v);\nL = limit(sin(x)/x, x, 0);\nT = taylor(exp(x), x, 'Order', 4);\nE = expand((x + 1)^3);\nsf = simplify(f);");
    no_errors(&out);
    assert_eq!(out[0], "f = x^2 + 2*x + 1");
    assert_eq!(text(&it, "df"), "2*x + 2");
    assert_eq!(text(&it, "F"), "x^3/3 + x^2 + x");
    assert_eq!(text(&it, "I"), "1/2");
    assert_eq!(text(&it, "fs"), "[x - 1, x + 1]");
    assert_eq!(text(&it, "s"), "x + 1");
    assert_eq!(text(&it, "r"), "2\n3");
    assert_eq!(num(&it, "d"), vec![9.0]);
    assert_eq!(text(&it, "L"), "1");
    assert_eq!(text(&it, "T"), "x^3/6 + x^2/2 + x + 1");
    assert_eq!(text(&it, "E"), "x^3 + 3*x^2 + 3*x + 1");
    assert_eq!(text(&it, "sf"), "(x + 1)^2");
}

#[test]
fn symbolic_calculus_rules() {
    let (it2, out2) = run("syms x\nP = int(x^3 - 6*x^2 + 11*x - 6);\nt = datetime(2026,10,28,9,30,0);\nspan = between(t, t + hours(36))");
    no_errors(&out2);
    assert_eq!(text(&it2, "P"), "x^4/4 - 2*x^3 + (11*x^2)/2 - 6*x", "int returns the expanded polynomial");
    assert_eq!(out2.last().unwrap(), "span = calendarDuration\n   1d 12h 0m 0s");
    let (it, out) = run("syms x\na = diff(sin(x)^2);\nb = int(sin(x));\nc = int(exp(2*x));\nd = int(1/(1 + x^2));\ne = int(x*exp(x));\nf = diff(log(x^2 + 1));\ng = limit((1 + 1/x)^x, x, inf);");
    no_errors(&out);
    assert_eq!(text(&it, "a"), "2*cos(x)*sin(x)");
    assert_eq!(text(&it, "b"), "-cos(x)");
    assert_eq!(text(&it, "c"), "exp(2*x)/2");
    assert_eq!(text(&it, "d"), "atan(x)");
    assert!(matches!(text(&it, "e").as_str(), "exp(x)*(x - 1)" | "x*exp(x) - exp(x)"), "{}", text(&it, "e"));
    assert_eq!(text(&it, "f"), "(2*x)/(x^2 + 1)");
}

#[test]
fn symbolic_systems_and_functions() {
    let (it, out) = run("syms a b x\n[sa, sb] = solve([a + b == 3, a - b == 1], [a, b]);\nS = solve([a + b == 3, a - b == 1], [a, b]);\nsx = S.a;\ng = matlabFunction(x^2 + 1);\nv = g(3);\nM = [a 1; 0 b];\nD = det(M);\nc = class(x);");
    no_errors(&out);
    assert_eq!(text(&it, "sa"), "2");
    assert_eq!(text(&it, "sb"), "1");
    assert_eq!(text(&it, "sx"), "2");
    assert_eq!(num(&it, "v"), vec![10.0]);
    assert_eq!(text(&it, "D"), "a*b");
    assert_eq!(text(&it, "c"), "sym");
}
