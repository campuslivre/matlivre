//! eval and workspace functions, and cell display, against MATLAB R2023b.

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

fn text(it: &Interpreter, name: &str) -> String {
    it.get_var(name).unwrap().text().unwrap()
}

#[test]
fn eval_runs_in_the_current_workspace() {
    let (it, out) = run(concat!(
        "x = 3;\neval('y = x + 1;');\nz = eval('x * 2');\n[a, b] = eval('size(ones(2,3))');\n",
        "eval('error(''boom'')', 'c = 1;');\n",
        "function_free = 1;\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    assert_eq!(num(&it, "y"), [4.0]);
    assert_eq!(num(&it, "z"), [6.0]);
    assert_eq!((num(&it, "a"), num(&it, "b")), (vec![2.0], vec![3.0]));
    assert_eq!(num(&it, "c"), [1.0]);

    let (_, out) = run("w = eval('v = 1');");
    assert_eq!(out, ["Error: Incorrect use of '=' operator. Assign a value to a variable using '=' and compare values for equality using '=='."]);
}

#[test]
fn eval_inside_a_function_uses_its_workspace_and_evalin_reaches_out() {
    let (it, out) = run(concat!(
        "r = f(5);\n",
        "function out = f(k)\n",
        "  eval('t = k * 10;');\n",
        "  assignin('base', 'fromf', t);\n",
        "  out = evalin('base', 'basevar');\n",
        "end\n",
    ));
    // basevar does not exist: the error comes from the base workspace lookup.
    assert!(out.iter().any(|o| o.contains("basevar")), "{out:?}");
    assert_eq!(num(&it, "fromf"), [50.0]);

    let (it, out) = run("basevar = 7;\nr = g();\nfunction out = g()\n  out = evalin('base', 'basevar') + 1;\nend\n");
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    assert_eq!(num(&it, "r"), [8.0]);

    let (_, out) = run("assignin('base', '1x', 1)");
    assert_eq!(out, ["Error: Invalid variable name \"1x\" in ASSIGNIN."]);
}

#[test]
fn evalc_captures_displayed_text() {
    let (it, _) = run("t = evalc('disp(1:3)'); u = evalc('q = 5;');");
    assert_eq!(text(&it, "t"), "     1     2     3\n");
    assert_eq!(text(&it, "u"), "");
    assert_eq!(num(&it, "q"), [5.0]);
}

#[test]
fn names_keywords_and_validation_match_matlab() {
    let (it, out) = run(concat!(
        "k = iskeyword('for'); k2 = iskeyword('fore'); kw = iskeyword();\n",
        "v = [isvarname('abc') isvarname('1abc') isvarname('for') isvarname(repmat('a',1,64))];\n",
        "g1 = genvarname('Hello World'); g2 = genvarname('x', {'x','x1'}); g3 = genvarname('1abc'); g4 = genvarname('for');\n",
        "n = namelengthmax; s1 = validatestring('app', {'apple','banana'});\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    assert_eq!(num(&it, "k"), [1.0]);
    assert_eq!(num(&it, "k2"), [0.0]);
    assert_eq!(it.get_var("kw").unwrap().numel(), 20);
    assert_eq!(num(&it, "v"), [1.0, 0.0, 0.0, 0.0]);
    assert_eq!([text(&it, "g1"), text(&it, "g2"), text(&it, "g3"), text(&it, "g4")], ["HelloWorld", "x2", "x1abc", "xFor"]);
    assert_eq!(num(&it, "n"), [63.0]);
    assert_eq!(text(&it, "s1"), "apple");
    let (_, out) = run("validatestring('x', {'apple','banana'})");
    assert_eq!(out, ["Error: Expected input to match one of these values:\n\n'apple', 'banana'\n\nThe input, 'x', did not match any of the valid values."]);
    let (_, out) = run("validatestring('a', {'apple','apricot'})");
    assert!(out[0].ends_with("The input, 'a', matched more than one valid value."), "{out:?}");
}

#[test]
fn who_whos_and_release_queries() {
    let (it, out) = run(concat!(
        "x = 3; aa = 7;\nwho\nw = who;\nS = whos('x');\n",
        "tf = [isMATLABReleaseOlderThan('R2024a') isMATLABReleaseOlderThan('R2020a')];\n",
        "vl = [verLessThan('matlab', '9.14') verLessThan('matlab', '24.1')];\nb = builtin('sum', [1 2 3]);\n",
    ));
    assert!(out.iter().any(|o| o == "\nYour variables are:\n\naa  x   \n\n"), "{out:?}");
    assert_eq!(it.get_var("w").unwrap().numel(), 2);
    assert_eq!(num(&it, "tf"), [1.0, 0.0]);
    assert_eq!(num(&it, "vl"), [0.0, 1.0]);
    assert_eq!(num(&it, "b"), [6.0]);
    match it.get_var("S") {
        Some(Value::Struct(s)) => assert_eq!(s.fields, ["name", "size", "bytes", "class", "global", "sparse", "complex", "nesting", "persistent"]),
        other => panic!("{other:?}"),
    }
}

#[test]
fn cell_columns_pad_inside_the_braces() {
    let (_, out) = run("c = {1; 'abc'; [1 2 3]; -2.5}\nd = {'x', 22; 'long text', [1 2]}\ne = {true; \"str\"; []}");
    assert!(out[0].ends_with("    {[      1]}\n    {'abc'    }\n    {[  1 2 3]}\n    {[-2.5000]}"), "{:?}", out[0]);
    assert!(out[1].ends_with("    {'x'        }    {[ 22]}\n    {'long text'}    {[1 2]}"), "{:?}", out[1]);
    assert!(out[2].ends_with("    {[       1]}\n    {[\"str\"   ]}\n    {0×0 double}"), "{:?}", out[2]);
}
