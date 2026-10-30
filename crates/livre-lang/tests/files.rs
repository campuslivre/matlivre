//! File and folder functions against MATLAB R2023b, in a scratch folder.

use livre_lang::{Interpreter, OutputKind, Value};

fn run(src: &str) -> Interpreter {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    let errors: Vec<String> = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Error(e) => Some(e),
            _ => None,
        })
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    it
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

/// A fresh scratch folder for one test, as a MATLAB char literal.
fn scratch(tag: &str) -> String {
    let d = std::env::temp_dir().join(format!("matlivre-files-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d.display().to_string().replace('\'', "''")
}

#[test]
fn names_and_parts_match_matlab() {
    let it = run(concat!(
        "[p, n, e] = fileparts('C:\\a\\b\\file.tar.gz'); [p3, n3, e3] = fileparts('noext');\n",
        "f2 = fullfile('a\\', '\\b', 'c.m'); f3 = fullfile({'a', 'b'}, 'x.m'); fs = filesep; ps = pathsep;\n",
    ));
    if cfg!(windows) {
        assert_eq!((text(&it, "p"), text(&it, "n"), text(&it, "e")), ("C:\\a\\b".into(), "file.tar".into(), ".gz".into()));
        assert_eq!(text(&it, "f2"), "a\\b\\c.m");
        assert_eq!(text(&it, "fs"), "\\");
        assert_eq!(text(&it, "ps"), ";");
    }
    assert_eq!((text(&it, "p3"), text(&it, "n3"), text(&it, "e3")), ("".into(), "noext".into(), "".into()));
    assert_eq!(it.get_var("f3").unwrap().dims(), [1, 2]);
}

#[test]
fn low_level_io_matches_matlab() {
    let d = scratch("io");
    let src = format!(
        concat!(
            "d = '{d}'; mkdir(d); fn = fullfile(d, 't.txt');\n",
            "fid = fopen(fn, 'w'); c = fprintf(fid, 'line %d\\n', 1:3); st = fclose(fid);\n",
            "fid = fopen(fn, 'r'); l1 = fgetl(fid); g2 = fgets(fid); fe = feof(fid); l3 = fgetl(fid); l4 = fgetl(fid); fe2 = feof(fid);\n",
            "frewind(fid); pos0 = ftell(fid); st3 = fseek(fid, 5, 'bof'); r = fread(fid, 3); r2 = fread(fid, 2, '*char'); fclose(fid);\n",
            "fid = fopen(fn); a = fscanf(fid, 'line %d\\n'); fclose(fid);\n",
            "t = fileread(fn); L = readlines(fn); [fidx, emsg] = fopen(fullfile(d, 'nope.txt'));\n",
            "bn = fullfile(d, 'b.bin'); fid = fopen(bn, 'w'); cnt = fwrite(fid, [1 2 300], 'uint16'); fclose(fid);\n",
            "fid = fopen(bn); u = fread(fid, Inf, 'uint16=>uint16'); fclose(fid); fid = fopen(bn); dd = fread(fid, [2 2], 'uint8'); fclose(fid);\n",
            "rmdir(d, 's');\n",
        ),
        d = d
    );
    let it = run(&src);
    assert_eq!(num(&it, "c"), [21.0]);
    assert_eq!(num(&it, "st"), [0.0]);
    assert_eq!(text(&it, "l1"), "line 1");
    assert_eq!(text(&it, "g2"), "line 2\n");
    assert_eq!(num(&it, "fe"), [0.0]);
    assert_eq!(text(&it, "l3"), "line 3");
    assert_eq!(num(&it, "l4"), [-1.0]);
    assert_eq!(num(&it, "fe2"), [1.0]);
    assert_eq!((num(&it, "pos0"), num(&it, "st3")), (vec![0.0], vec![0.0]));
    assert_eq!(num(&it, "r"), [49.0, 10.0, 108.0]);
    assert_eq!(text(&it, "r2"), "in");
    assert_eq!(num(&it, "a"), [1.0, 2.0, 3.0]);
    assert_eq!(text(&it, "t"), "line 1\nline 2\nline 3\n");
    assert_eq!(it.get_var("L").unwrap().dims(), [4, 1]);
    assert_eq!(num(&it, "fidx"), [-1.0]);
    assert_eq!(text(&it, "emsg"), "No such file or directory");
    assert_eq!(num(&it, "cnt"), [3.0]);
    assert_eq!((num(&it, "u"), it.get_var("u").unwrap().class_name()), (vec![1.0, 2.0, 300.0], "uint16".into()));
    assert_eq!(num(&it, "dd"), [1.0, 0.0, 2.0, 0.0]);
}

#[test]
fn folders_and_environment_match_matlab() {
    let d = scratch("dir");
    let src = format!(
        concat!(
            "d = '{d}'; [st, msg] = mkdir(d); [st2, msg2] = mkdir(d); fn = fullfile(d, 't.txt'); writelines([\"x\"; \"y\"], fn);\n",
            "s = dir(d); names = {{s.name}}; isd = [s.isdir]; e1 = [exist(fn) exist(d) exist(fn, 'file') exist(d, 'dir') exist('sin', 'builtin')];\n",
            "i1 = [isfile(fn) isfolder(d) isfile(d)]; copyfile(fn, fullfile(d, 'c.txt')); movefile(fullfile(d, 'c.txt'), fullfile(d, 'm.txt'));\n",
            "s2 = dir(fullfile(d, '*.txt')); n2 = {{s2.name}}; delete(fullfile(d, 'm.txt')); s3 = dir(fullfile(d, '*.txt')); n3 = {{s3.name}};\n",
            "w = fileread(fn); setenv('MATLIVRE_TEST_X', 'hello'); ge = getenv('MATLIVRE_TEST_X'); ie = isenv('MATLIVRE_TEST_X');\n",
            "[stt, out] = system('echo hi'); rmdir(d, 's'); gone = isfolder(d);\n",
        ),
        d = d
    );
    let it = run(&src);
    assert_eq!(text(&it, "msg2"), "Directory already exists.");
    let names = |n: &str| match it.get_var(n) {
        Some(Value::Cell(c)) => c.data.iter().map(|v| v.text().unwrap()).collect::<Vec<_>>(),
        other => panic!("{other:?}"),
    };
    assert_eq!(names("names"), [".", "..", "t.txt"]);
    assert_eq!(num(&it, "isd"), [1.0, 1.0, 0.0]);
    assert_eq!(num(&it, "e1"), [2.0, 7.0, 2.0, 7.0, 5.0]);
    assert_eq!(num(&it, "i1"), [1.0, 1.0, 0.0]);
    assert_eq!(names("n2"), ["m.txt", "t.txt"]);
    assert_eq!(names("n3"), ["t.txt"]);
    assert_eq!(text(&it, "w"), "x\ny\n");
    assert_eq!(text(&it, "ge"), "hello");
    assert_eq!(num(&it, "ie"), [1.0]);
    assert_eq!(num(&it, "stt"), [0.0]);
    assert_eq!(text(&it, "out").trim(), "hi");
    assert_eq!(num(&it, "gone"), [0.0]);
}
