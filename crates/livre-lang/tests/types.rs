//! Cell, struct and type conversions against MATLAB R2023b.

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

fn class(it: &Interpreter, name: &str) -> String {
    it.get_var(name).unwrap().class_name()
}

#[test]
fn cell_and_struct_conversions_match_matlab() {
    let (it, out) = run(concat!(
        "a = cell2mat({1 2; 3 4}); b = cell2mat({[1 2], [3]; [4 5], [6]}); c = cell2mat({'ab', 'cd'}); e = cell2mat({});\n",
        "d = mat2cell(magic(4), [1 3], [2 2]); d2 = mat2cell((1:6)', [2 4]);\n",
        "s = cell2struct({1; 'x'}, {'f1'; 'f2'}, 1); s2 = cell2struct({1, 2; 3, 4}, {'a', 'b'}, 2);\n",
        "st.p = 1; st.q = 'hi'; sc = struct2cell(st); sf = structfun(@(x) numel(x), st);\n",
        "g = getfield(st, 'q'); st2 = setfield(st, 'r', 5); [o2, perm] = orderfields(struct('b', 1, 'a', 2));\n",
        "n.inner.v = [1 2 3]; gv = getfield(n, 'inner', 'v', {2}); n2 = setfield(n, 'inner', 'v', {3}, 9);\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    assert_eq!(num(&it, "a"), [1.0, 3.0, 2.0, 4.0]);
    assert_eq!(num(&it, "b"), [1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
    assert_eq!(it.get_var("c").unwrap().text().unwrap(), "abcd");
    assert_eq!(it.get_var("e").unwrap().dims(), [0, 0]);
    assert_eq!(it.get_var("d").unwrap().dims(), [2, 2]);
    assert_eq!(it.get_var("d2").unwrap().dims(), [2, 1]);
    match it.get_var("s2") {
        Some(Value::Struct(s)) => assert_eq!((s.dims.clone(), s.fields.clone()), (vec![2, 1], vec!["a".to_string(), "b".to_string()])),
        other => panic!("{other:?}"),
    }
    assert_eq!(it.get_var("sc").unwrap().dims(), [2, 1]);
    assert_eq!(num(&it, "sf"), [1.0, 2.0]);
    assert_eq!(it.get_var("g").unwrap().text().unwrap(), "hi");
    match it.get_var("o2") {
        Some(Value::Struct(s)) => assert_eq!(s.fields, ["a", "b"]),
        other => panic!("{other:?}"),
    }
    assert_eq!(num(&it, "perm"), [2.0, 1.0]);
    assert_eq!(num(&it, "gv"), [2.0]);
    match it.get_var("n2") {
        Some(Value::Struct(s)) => match s.get("inner") {
            Some(Value::Struct(i)) => assert_eq!(i.get("v").unwrap().to_matrix().unwrap().data, [1.0, 2.0, 9.0]),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }

    let (_, out) = run("cell2mat({1, 'a'})");
    assert_eq!(out, ["Error: All contents of the input cell array must be of the same data type."]);
    let (_, out) = run("s.a = 1; getfield(s, 'zz')");
    assert_eq!(out, ["Error: Unrecognized field name \"zz\"."]);
    let (_, out) = run("st.p = 1; st.q = 'hi'; sf2 = structfun(@(x) {x}, st, 'UniformOutput', false)");
    assert!(out[0].ends_with("    p: {[1]}\n    q: {'hi'}"), "{:?}", out[0]);
}

#[test]
fn byte_level_conversions_match_matlab() {
    let (it, out) = run(concat!(
        "tc = typecast(single(1), 'uint32'); tc2 = typecast(uint8([1 0 0 0]), 'uint32'); tc3 = typecast(int16(-1), 'uint16');\n",
        "sb = swapbytes(uint16(1)); af = [allfinite([1 2 3]) allfinite([1 Inf]) anynan([1 NaN]) anynan([1 2])];\n",
        "fh = functions(@sin);\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    assert_eq!((num(&it, "tc"), class(&it, "tc")), (vec![1065353216.0], "uint32".into()));
    assert_eq!(num(&it, "tc2"), [1.0]);
    assert_eq!((num(&it, "tc3"), class(&it, "tc3")), (vec![65535.0], "uint16".into()));
    assert_eq!(num(&it, "sb"), [256.0]);
    assert_eq!(num(&it, "af"), [1.0, 0.0, 1.0, 0.0]);
    match it.get_var("fh") {
        Some(Value::Struct(s)) => assert_eq!(s.get("type").unwrap().text().unwrap(), "simple"),
        other => panic!("{other:?}"),
    }
}
