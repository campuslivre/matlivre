//! JSON and MAT-files against MATLAB R2023b.

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

fn text(it: &Interpreter, name: &str) -> String {
    it.get_var(name).unwrap().text().unwrap()
}

#[test]
fn jsondecode_and_jsonencode_match_matlab() {
    let (it, out) = run(concat!(
        "a = jsondecode('{\"x\": 1, \"y\": [1, 2, 3], \"s\": \"hi\", \"b\": true, \"n\": null}');\n",
        "c = jsondecode('[[1, 2], [3, 4]]'); d = jsondecode('[{\"a\": 1}, {\"a\": 2}]'); e = jsondecode('[{\"a\": 1}, {\"b\": 2}]');\n",
        "g = jsondecode('{\"1bad name\": 3}'); k = jsondecode('[true, false]');\n",
        "e1 = jsonencode(struct('x', 1, 'y', [1 2 3], 's', 'hi')); e2 = jsonencode([1 2; 3 4]); e3 = jsonencode({1, 'a', [1 2]});\n",
        "e4 = jsonencode(true); e5 = jsonencode([]); e6 = jsonencode(NaN); e7 = jsonencode(struct('a', {1, 2})); e8 = jsonencode(3.14159);\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    match it.get_var("a") {
        Some(Value::Struct(s)) => {
            assert_eq!(s.fields, ["x", "y", "s", "b", "n"]);
            assert_eq!(s.get("y").unwrap().dims(), [3, 1]);
            assert_eq!(s.get("b").unwrap().class_name(), "logical");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(it.get_var("c").unwrap().to_matrix().unwrap().data, [1.0, 3.0, 2.0, 4.0]);
    assert!(matches!(it.get_var("d"), Some(Value::Struct(s)) if s.dims == [2, 1]));
    assert!(matches!(it.get_var("e"), Some(Value::Cell(_))));
    assert!(matches!(it.get_var("g"), Some(Value::Struct(s)) if s.fields == ["x1badName"]));
    assert_eq!(it.get_var("k").unwrap().class_name(), "logical");
    let enc = ["e1", "e2", "e3", "e4", "e5", "e6", "e7", "e8"].map(|n| text(&it, n));
    assert_eq!(
        enc,
        ["{\"x\":1,\"y\":[1,2,3],\"s\":\"hi\"}", "[[1,2],[3,4]]", "[1,\"a\",[1,2]]", "true", "[]", "null", "[{\"a\":1},{\"a\":2}]", "3.14159"]
    );
}

#[test]
fn save_and_load_round_trip_and_ascii() {
    let d = std::env::temp_dir().join(format!("matlivre-mat-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let p = |n: &str| d.join(n).display().to_string().replace('\'', "''");
    let src = format!(
        concat!(
            "x = 1:3; y = 'text'; z = struct('p', 2); w = {{1, 'a'}}; q = int16([1 -2]); l = true; cc = 1+2i;\n",
            "save('{mat}', 'x', 'y', 'z', 'w', 'q', 'l', 'cc');\nclear x y\n",
            "S = load('{mat}');\nload('{mat}', 'x');\n",
            "M = magic(3); save('{txt}', 'M', '-ascii'); t = fileread('{txt}'); L = load('{txt}');\n",
        ),
        mat = p("r.mat"),
        txt = p("m.txt")
    );
    let (it, out) = run(&src);
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    match it.get_var("S") {
        Some(Value::Struct(s)) => {
            assert_eq!(s.fields, ["cc", "l", "q", "w", "x", "y", "z"]);
            assert_eq!(s.get("q").unwrap().class_name(), "int16");
            assert_eq!(s.get("l").unwrap().class_name(), "logical");
            assert_eq!(s.get("y").unwrap().text().unwrap(), "text");
            assert!(s.get("cc").unwrap().to_matrix().unwrap().is_complex());
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(it.get_var("x").unwrap().to_matrix().unwrap().data, [1.0, 2.0, 3.0]);
    assert_eq!(text(&it, "t"), "   8.0000000e+00   1.0000000e+00   6.0000000e+00\n   3.0000000e+00   5.0000000e+00   7.0000000e+00\n   4.0000000e+00   9.0000000e+00   2.0000000e+00\n");
    assert_eq!(it.get_var("L").unwrap().to_matrix().unwrap().data, [8.0, 3.0, 4.0, 1.0, 5.0, 9.0, 6.0, 7.0, 2.0]);
    let _ = std::fs::remove_dir_all(&d);
}
