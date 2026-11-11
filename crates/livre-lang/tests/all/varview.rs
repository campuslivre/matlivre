//! The variable viewer's snapshots and the workspace browser's matrix preview.

use livre_lang::{Interpreter, VarBody};

fn session(src: &str) -> Interpreter {
    let mut it = Interpreter::new();
    it.run_source(src).unwrap();
    it
}

fn body(it: &Interpreter, name: &str) -> VarBody {
    it.inspect(name).unwrap().body
}

fn preview(it: &Interpreter, name: &str) -> Option<String> {
    it.workspace().into_iter().find(|v| v.name == name).unwrap().preview
}

#[test]
fn a_matrix_is_a_grid_of_its_elements() {
    let it = session("A = [1 2.5; -3 1234.5; 0 NaN];");
    let v = it.inspect("A").unwrap();
    assert_eq!((v.name.as_str(), v.class.as_str(), v.size.as_str()), ("A", "double", "3×2"));
    assert_eq!(v.body.shape(), Some((3, 2)));
    assert!(v.body.is_numeric());
    let cells: Vec<String> = (0..3).flat_map(|r| (0..2).map(move |c| (r, c))).map(|(r, c)| v.body.cell(r, c)).collect();
    assert_eq!(cells, ["1", "2.5000", "-3", "1.2345e+03", "0", "NaN"]);
    assert_eq!(v.body.cell(3, 0), "");
}

#[test]
fn cells_follow_the_class_and_the_format() {
    let it = session("format long\nx = [pi; 2];\nz = [1+2i 3-4i; 0 1i];\nk = int8([1 2; 3 4]);\nb = [true false; false true];");
    assert_eq!(body(&it, "x").cell(0, 0), "3.141592653589793");
    assert_eq!(body(&it, "k").cell(1, 0), "3");
    assert_eq!(body(&it, "b").cell(0, 1), "0");
    let it = session("z = [1+2i 3-4i; 0 1i];");
    assert_eq!(body(&it, "z").cell(0, 1), "3.0000 - 4.0000i");
    assert_eq!(it.inspect("z").unwrap().class, "double");
}

#[test]
fn text_cell_and_struct_values_are_grids_of_text() {
    let it = session("s = [\"a\" \"bc\"; \"d\" missing];\nc = {1, 'two'; [1 2 3], {4}};\nt = ['ab'; 'cd'];\np.name = 'x';\np.data = magic(3);");
    let s = body(&it, "s");
    assert_eq!(s.shape(), Some((2, 2)));
    assert!(!s.is_numeric());
    assert_eq!([s.cell(0, 1), s.cell(1, 0), s.cell(1, 1)], ["\"bc\"", "\"d\"", "<missing>"]);
    let c = body(&it, "c");
    assert_eq!([c.cell(0, 0), c.cell(0, 1), c.cell(1, 0), c.cell(1, 1)], ["1", "'two'", "[1 2 3]", "1×1 cell"]);
    let t = body(&it, "t");
    assert_eq!((t.shape(), t.cell(1, 0)), (Some((2, 1)), "cd".to_string()));
    match body(&it, "p") {
        VarBody::Cells { rows: 2, cols: 1, text, row_names: Some(names) } => {
            assert_eq!(names, ["name", "data"]);
            assert_eq!(text, ["'x'", "3×3 double"]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn other_values_are_shown_as_disp_text() {
    let it = session("n = ones(2, 2, 2);\nf = @(x) x + 1;\ns = struct('a', {1, 2});");
    for name in ["n", "f", "s"] {
        assert!(matches!(body(&it, name), VarBody::Text(t) if !t.is_empty()), "{name}");
    }
    assert!(matches!(body(&it, "f"), VarBody::Text(t) if t.contains("@(x)")));
    assert!(it.inspect("missing_variable").is_none());
}

#[test]
fn a_large_array_keeps_its_top_left_corner() {
    let it = session("x = reshape(1:3e5, 3, 1e5);\ny = (1:2e5)';
f = magic(4);");
    let x = it.inspect("x").unwrap();
    assert_eq!(x.size, "3×100000");
    assert!(x.truncated && !it.inspect("f").is_some_and(|v| v.truncated));
    assert_eq!(x.body.shape(), Some((3, 10_000)));
    assert_eq!(x.body.cell(2, 9_999), "30000");
    let y = body(&it, "y");
    assert_eq!(y.shape(), Some((100_000, 1)));
    assert_eq!(y.cell(99_999, 0), "100000");
}

#[test]
fn the_workspace_previews_matrices_whose_value_is_only_a_size() {
    let it = session("a = 3;\nr = 1:10;\nA = [1 2; 3 4];\nB = [0.5 1; 2 3];\ns = \"text\";\nc = {1, 2; 3, 4};\ne = [];");
    for name in ["a", "r", "s", "c", "e"] {
        assert_eq!(preview(&it, name), None, "{name}");
    }
    assert_eq!(preview(&it, "A").unwrap(), "     1     2\n     3     4");
    assert_eq!(preview(&it, "B").unwrap(), "    0.5000    1.0000\n    2.0000    3.0000");
}

#[test]
fn a_preview_marks_the_rows_and_columns_it_leaves_out() {
    let it = session("x = 1:200;\nM = reshape(1:90, 9, 10);\nv = (1:9)';");
    assert_eq!(preview(&it, "x").unwrap(), "     1     2     3     4     5     6   …");
    let m = preview(&it, "M").unwrap();
    let lines: Vec<&str> = m.lines().collect();
    assert_eq!(lines.len(), 9);
    assert_eq!(lines[0], "     1    10    19    28    37    46   …");
    assert_eq!(lines[7], "     8    17    26    35    44    53   …");
    assert_eq!(lines[8], "     …");
    assert_eq!(preview(&it, "v").unwrap().lines().last(), Some("     …"));
}
