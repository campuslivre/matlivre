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
            OutputKind::Figure(f) => format!("<figure {}>", f.id),
            OutputKind::Warning(w) => format!("Warning: {w}"),
            OutputKind::Error(e) => format!("Error: {e}"),
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

#[test]
fn arithmetic_and_display() {
    let (_, out) = run("x = 5\ny = 1/3\nz = [1 2 3]\n");
    assert_eq!(out[0], "x = 5");
    assert_eq!(out[1], "y = 0.3333");
    assert_eq!(out[2], "z = 1×3\n\n     1     2     3");
}

#[test]
fn matrix_whitespace_rules() {
    let (it, _) = run("a = [1 -2]; b = [1 - 2]; c = [1, 2\n 3, 4]; d = c';");
    assert_eq!(num(&it, "a"), vec![1.0, -2.0]);
    assert_eq!(num(&it, "b"), vec![-1.0]);
    assert_eq!(num(&it, "c"), vec![1.0, 3.0, 2.0, 4.0]);
    assert_eq!(num(&it, "d"), vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn transpose_vs_quote() {
    let (it, _) = run("x = [1 2 3]'; s = ['ab' 'cd']; n = numel(s);");
    assert_eq!(num(&it, "x").len(), 3);
    assert_eq!(it.get_var("s").unwrap().text().unwrap(), "abcd");
    assert_eq!(num(&it, "n"), vec![4.0]);
}

#[test]
fn precedence() {
    let (it, _) = run("a = -2^2; b = 2^-1; c = 1:3+1; d = ~0 == 1;");
    assert_eq!(num(&it, "a"), vec![-4.0]);
    assert_eq!(num(&it, "b"), vec![0.5]);
    assert_eq!(num(&it, "c"), vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(num(&it, "d"), vec![1.0]);
}

#[test]
fn indexing_and_growth() {
    let (it, _) = run("v = []; for k = 1:5, v(k) = k^2; end\nw = v(end-1:end); v(2) = []; M = magic3(); e = M(2, :);\nfunction M = magic3()\nM = [8 1 6; 3 5 7; 4 9 2];\nend");
    assert_eq!(num(&it, "w"), vec![16.0, 25.0]);
    assert_eq!(num(&it, "v"), vec![1.0, 9.0, 16.0, 25.0]);
    assert_eq!(num(&it, "e"), vec![3.0, 5.0, 7.0]);
}

#[test]
fn logical_indexing() {
    let (it, _) = run("x = 1:10; y = x(mod(x, 2) == 0); x(x > 8) = 0;");
    assert_eq!(num(&it, "y"), vec![2.0, 4.0, 6.0, 8.0, 10.0]);
    assert_eq!(num(&it, "x")[9], 0.0);
}

#[test]
fn control_flow() {
    let src = "s = 0;\nfor i = 1:10\n  if mod(i, 2) == 0\n    continue\n  elseif i > 7\n    break\n  end\n  s = s + i;\nend\nk = 0; while k < 3, k = k + 1; end\nswitch 'b'\n case {'a', 'b'}\n  r = 1;\n otherwise\n  r = 2;\nend";
    let (it, _) = run(src);
    assert_eq!(num(&it, "s"), vec![1.0 + 3.0 + 5.0 + 7.0]);
    assert_eq!(num(&it, "k"), vec![3.0]);
    assert_eq!(num(&it, "r"), vec![1.0]);
}

#[test]
fn functions_and_handles() {
    let src = "[m, i] = mx([3 9 2]);\nf = @(x) x.^2 + c0;\nc0 = 100;\ny = f(3);\ng = @sqrt; h = g(16);\nq = arrayfun(@(t) t * 2, [1 2 3]);\nfunction [m, i] = mx(v)\n[m, i] = max(v);\nend";
    let (it, out) = run(&format!("c0 = 1;\n{src}"));
    assert!(out.iter().all(|o| !o.starts_with("Error")), "{out:?}");
    assert_eq!(num(&it, "m"), vec![9.0]);
    assert_eq!(num(&it, "i"), vec![2.0]);
    assert_eq!(num(&it, "y"), vec![10.0], "anonymous functions capture by value");
    assert_eq!(num(&it, "h"), vec![4.0]);
    assert_eq!(num(&it, "q"), vec![2.0, 4.0, 6.0]);
}

#[test]
fn structs_and_cells() {
    let (it, out) = run("s.name = 'Ada'; s.age = 36; s.tags{2} = 'x';\nc = {1, 'two', [3 4]};\nn = numel(c); t = c{2};\ns");
    assert_eq!(num(&it, "n"), vec![3.0]);
    assert_eq!(it.get_var("t").unwrap().text().unwrap(), "two");
    assert!(out.last().unwrap().starts_with("s = struct with fields:"), "{out:?}");
}

#[test]
fn printing() {
    let (_, out) = run("fprintf('%d apples and %.2f pears\\n', 3, 2.5);\ndisp('hello')\nfprintf('%d,', [1 2 3]); fprintf('\\n');\nx = sprintf('%5.1f|', pi)");
    assert_eq!(out[0], "3 apples and 2.50 pears\n");
    assert_eq!(out[1], "hello\n");
    assert_eq!(out[2], "1,2,3,");
    assert_eq!(out[4], "x = '  3.1|'");
}

#[test]
fn command_syntax_and_ans() {
    let (it, out) = run("format long\npi\nformat short\n3 + 4");
    assert_eq!(out[0], "ans = 3.141592653589793");
    assert_eq!(out[1], "ans = 7");
    assert_eq!(num(&it, "ans"), vec![7.0]);
}

#[test]
fn linear_algebra() {
    let (it, _) = run("A = [4 -2; 1 1]; b = [2; 3]; x = A \\ b; d = det(A); B = A * inv(A);");
    let x = num(&it, "x");
    assert!((x[0] - 4.0 / 3.0).abs() < 1e-12 && (x[1] - 5.0 / 3.0).abs() < 1e-12, "{x:?}");
    assert!((num(&it, "d")[0] - 6.0).abs() < 1e-12);
    let b = num(&it, "B");
    assert!((b[0] - 1.0).abs() < 1e-12 && b[1].abs() < 1e-12);
}

#[test]
fn errors_report_line() {
    let mut it = Interpreter::new();
    let err = it.run_source("a = 1;\nb = undefinedThing + 1;\nc = 3;").unwrap_err();
    assert_eq!(err.line, 2);
    assert!(err.message.contains("undefinedThing"));
    assert!(it.get_var("c").is_none());
}

#[test]
fn plotting_records_figure() {
    let (_, out) = run("x = linspace(0, 2*pi, 50);\nplot(x, sin(x), 'r--', 'LineWidth', 2)\nhold on\nplot(x, cos(x))\ntitle('Waves'); legend('sin', 'cos')");
    let figs = out.iter().filter(|o| o.starts_with("<figure")).count();
    assert!(figs >= 1, "{out:?}");
}

#[test]
fn try_catch() {
    let (it, _) = run("try\n  error('my:id', 'bad %d', 7);\ncatch err\n  m = err.message;\nend");
    assert_eq!(it.get_var("m").unwrap().text().unwrap(), "bad 7");
}

#[test]
fn daspect_sets_and_reports_the_data_aspect_ratio() {
    let (it, out) = run(concat!(
        "t = linspace(0, 2*pi, 50);\nplot(cos(t), sin(t))\n",
        "m0 = daspect('mode');\n",
        "daspect([1 2 1])\nr = daspect;\nm1 = daspect('mode');\n",
        "daspect('auto')\nm2 = daspect('mode');\n",
        "axis equal\nr2 = daspect;\nm3 = daspect('mode');\n",
        "axis normal\nm4 = daspect('mode');\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    let text = |n: &str| it.get_var(n).unwrap().text().unwrap();
    assert_eq!([text("m0"), text("m1"), text("m2"), text("m3"), text("m4")], ["auto", "manual", "auto", "manual", "auto"]);
    assert_eq!(num(&it, "r"), [1.0, 2.0, 1.0]);
    assert_eq!(num(&it, "r2"), [1.0, 1.0, 1.0]);

    let (_, out) = run("plot(1:3)\ndaspect([1 0 1])");
    assert!(out.iter().any(|o| o.starts_with("Error")), "{out:?}");
}
