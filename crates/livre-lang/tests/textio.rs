//! Delimited text files and textscan against MATLAB R2023b.

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

fn same(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x == y || (x.is_nan() && y.is_nan()))
}

#[test]
fn reading_and_writing_delimited_files_matches_matlab() {
    let d = std::env::temp_dir().join(format!("matlivre-textio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let src = format!(
        concat!(
            "d = '{d}'; mkdir(d); f = fullfile(d, 'a.csv');\n",
            "fid = fopen(f, 'w'); fprintf(fid, 'x,y,name\\n1,2.5,ab\\n3,,cd\\n5,6,ef\\n'); fclose(fid);\n",
            "m = readmatrix(f); c = readcell(f);\n",
            "f2 = fullfile(d, 'n.csv'); writematrix([1 2.5; 3 4], f2); r2 = fileread(f2); m2 = readmatrix(f2);\n",
            "cw = fullfile(d, 'c.csv'); writecell({{'a', 1; 'b', 2.5}}, cw); r3 = fileread(cw); cr = csvread(f2);\n",
            "f4 = fullfile(d, 'd.txt'); dlmwrite(f4, [1 2; 3 4], ' '); r4 = fileread(f4); dr = dlmread(f4);\n",
            "C2 = textscan(sprintf('a 1\\nb 2'), '%s %d');\n",
            "fid = fopen(f); C3 = textscan(fid, '%f %f %s', 'Delimiter', ',', 'HeaderLines', 1); fclose(fid);\n",
            "rmdir(d, 's');\n",
        ),
        d = d.display().to_string().replace('\'', "''")
    );
    let it = run(&src);
    assert!(same(&num(&it, "m"), &[1.0, 3.0, 5.0, 2.5, f64::NAN, 6.0, f64::NAN, f64::NAN, f64::NAN]));
    match it.get_var("c") {
        Some(Value::Cell(c)) => {
            assert_eq!(c.dims, [4, 3]);
            assert_eq!(c.data[0].text().unwrap(), "x");
            assert!(matches!(&c.data[6], Value::Str(s) if s.data[0].is_none()));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(it.get_var("r2").unwrap().text().unwrap(), "1,2.5\n3,4\n");
    assert_eq!(num(&it, "m2"), [1.0, 3.0, 2.5, 4.0]);
    assert_eq!(it.get_var("r3").unwrap().text().unwrap(), "a,1\nb,2.5\n");
    assert_eq!(num(&it, "cr"), [1.0, 3.0, 2.5, 4.0]);
    assert_eq!(it.get_var("r4").unwrap().text().unwrap(), "1 2\n3 4\n");
    assert_eq!(num(&it, "dr"), [1.0, 3.0, 2.0, 4.0]);
    match it.get_var("C2") {
        Some(Value::Cell(c)) => {
            assert_eq!(c.data[1].class_name(), "int32");
            assert_eq!(c.data[1].to_matrix().unwrap().data, [1.0, 2.0]);
        }
        other => panic!("{other:?}"),
    }
    match it.get_var("C3") {
        Some(Value::Cell(c)) => {
            assert_eq!(c.data[0].to_matrix().unwrap().data, [1.0, 3.0, 5.0]);
            assert!(same(&c.data[1].to_matrix().unwrap().data, &[2.5, f64::NAN, 6.0]));
            assert_eq!(c.data[2].dims(), [3, 1]);
        }
        other => panic!("{other:?}"),
    }
}
