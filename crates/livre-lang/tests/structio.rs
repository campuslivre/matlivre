//! readstruct/writestruct against MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind, Value};

fn run(src: &str) -> Interpreter {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    let errs: Vec<String> = it.take_outputs().into_iter().filter_map(|o| if let OutputKind::Error(e) = o.kind { Some(e) } else { None }).collect();
    assert!(errs.is_empty(), "{errs:?}");
    it
}

fn text(it: &Interpreter, n: &str) -> String {
    it.get_var(n).unwrap().text().unwrap()
}

#[test]
fn xml_and_json_match_matlab() {
    let d = std::env::temp_dir().join(format!("matlivre-sio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let p = |n: &str| d.join(n).display().to_string().replace('\'', "''");
    std::fs::write(
        d.join("in.xml"),
        "<?xml version=\"1.0\"?>\n<car make=\"VW\" year=\"2010\">\n  <wheel size=\"17\">front</wheel>\n  <wheel size=\"18\">rear</wheel>\n  <note>old car</note>\n  <empty/>\n</car>\n",
    )
    .unwrap();
    let it = run(&format!(
        concat!(
            "s.Name = 'Rover'; s.Count = 3; s.Ratio = 2.5; s.Vec = [1 2 3]; s.Flag = true; s.Sub.A = 'x'; s.Sub.B = 7; s.Str = \"hi\";\n",
            "writestruct(s, '{x}'); xt = fileread('{x}'); r = readstruct('{x}'); flag = class(r.Flag); vec = r.Vec;\n",
            "t.item = struct('id', {{1, 2}}, 'tag', {{'a', 'b'}}); writestruct(t, '{a}', 'StructNodeName', 'root'); q = readstruct('{a}'); tag2 = q.item(2).tag;\n",
            "c = readstruct('{i}'); w2 = c.wheel(2); year = c.yearAttribute; make = c.makeAttribute; em = c.empty;\n",
            "writestruct(s, '{j}'); jt = fileread('{j}'); js = readstruct('{j}'); jflag = class(js.Flag);\n",
        ),
        x = p("o.xml"),
        a = p("arr.xml"),
        i = p("in.xml"),
        j = p("o.json"),
    ));
    assert_eq!(
        text(&it, "xt"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<struct>\n    <Name>Rover</Name>\n    <Count>3</Count>\n    <Ratio>2.5</Ratio>\n    <Vec>1</Vec>\n    <Vec>2</Vec>\n    <Vec>3</Vec>\n    <Flag>true</Flag>\n    <Sub>\n        <A>x</A>\n        <B>7</B>\n    </Sub>\n    <Str>hi</Str>\n</struct>"
    );
    assert_eq!(text(&it, "flag"), "string");
    assert_eq!(it.get_var("vec").unwrap().dims(), [1, 3]);
    assert_eq!(text(&it, "tag2"), "b");
    match it.get_var("w2") {
        Some(Value::Struct(s)) => {
            assert_eq!(s.fields, ["sizeAttribute", "Text"]);
            assert_eq!(s.get("Text").unwrap().text().unwrap(), "rear");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(it.get_var("year").unwrap().to_matrix().unwrap().data, [2010.0]);
    assert_eq!(text(&it, "make"), "VW");
    assert_eq!(text(&it, "em"), "");
    assert!(text(&it, "jt").contains("\"Count\": 3.0,\n    \"Ratio\": 2.5,\n    \"Vec\": [\n        1.0,"));
    assert_eq!(text(&it, "jflag"), "logical");
    let _ = std::fs::remove_dir_all(&d);
}
