//! zip/unzip, tar/untar and gzip/gunzip against MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind, Value};

fn cell_texts(it: &Interpreter, name: &str) -> Vec<String> {
    match it.get_var(name) {
        Some(Value::Cell(c)) => {
            assert_eq!(c.dims[0], 1);
            c.data.iter().map(|v| v.text().unwrap()).collect()
        }
        other => panic!("{name}: {other:?}"),
    }
}

#[test]
fn archives_round_trip_like_matlab() {
    let d = std::env::temp_dir().join(format!("matlivre-arc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("src").join("sub")).unwrap();
    std::fs::write(d.join("src").join("a.txt"), "hello").unwrap();
    std::fs::write(d.join("src").join("b.txt"), "world").unwrap();
    std::fs::write(d.join("src").join("sub").join("c.txt"), "deep").unwrap();
    let mut it = Interpreter::new();
    let _ = it.run_source(&format!("cd('{}')", d.display().to_string().replace('\'', "''")));
    let _ = it.run_source(concat!(
        "e1 = zip('out', 'src'); e2 = zip('o2.zip', {'a.txt','sub'}, 'src'); e3 = zip('o3', 'src/*.txt');\n",
        "u = unzip('out.zip', 'un'); u2 = unzip('o2', 'un2');\n",
        "g = gzip({'src/a.txt','src/b.txt'}, 'gz'); gu = gunzip('gz/a.txt.gz', 'gun'); back = fileread('gun/a.txt');\n",
        "t2 = tar('t2.tgz', {'a.txt','sub'}, 'src'); ut2 = untar('t2.tgz', 'ut2'); deep = fileread('ut2/sub/c.txt');\n",
        "try, unzip('nope.zip'), catch e, msg = e.message; end\n",
    ));
    let errs: Vec<String> = it.take_outputs().into_iter().filter_map(|o| if let OutputKind::Error(e) = o.kind { Some(e) } else { None }).collect();
    assert!(errs.is_empty(), "{errs:?}");
    let sep = std::path::MAIN_SEPARATOR;
    let native = |s: &str| s.replace('/', &sep.to_string());
    assert_eq!(cell_texts(&it, "e1"), ["src/a.txt", "src/b.txt", "src/sub/c.txt"]);
    assert_eq!(cell_texts(&it, "e2"), ["a.txt", "sub/c.txt"]);
    assert_eq!(cell_texts(&it, "e3"), ["src/a.txt", "src/b.txt"]);
    assert_eq!(cell_texts(&it, "u"), ["un/src/a.txt", "un/src/b.txt", "un/src/sub/c.txt"].map(native));
    assert_eq!(cell_texts(&it, "u2"), ["un2/a.txt", "un2/sub/c.txt"].map(native));
    assert_eq!(cell_texts(&it, "g"), ["gz/a.txt.gz", "gz/b.txt.gz"].map(native));
    assert_eq!(cell_texts(&it, "gu"), [native("gun/a.txt")]);
    assert_eq!(it.get_var("back").unwrap().text().unwrap(), "hello");
    assert_eq!(cell_texts(&it, "t2"), ["a.txt", "sub/c.txt"]);
    assert_eq!(cell_texts(&it, "ut2"), ["ut2/a.txt", "ut2/sub/c.txt"].map(native));
    assert_eq!(it.get_var("deep").unwrap().text().unwrap(), "deep");
    assert_eq!(it.get_var("msg").unwrap().text().unwrap(), "Function UNZIP was unable to find file ''nope.zip''.");
    let _ = it.run_source("cd(tempdir)");
    let _ = std::fs::remove_dir_all(&d);
}
