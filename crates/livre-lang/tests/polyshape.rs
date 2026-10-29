//! polyshape and nsidedpoly against values recorded from MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind, Value};

fn run(src: &str) -> (Interpreter, Vec<String>) {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    let errors = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Error(e) => Some(e),
            _ => None,
        })
        .collect();
    (it, errors)
}

fn num(it: &Interpreter, name: &str) -> Vec<f64> {
    match it.get_var(name) {
        Some(Value::Num(m)) => m.data.clone(),
        other => panic!("{name} = {other:?}"),
    }
}

fn close(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 4.0 * f64::EPSILON * y.abs().max(1.0))
}

#[test]
fn nsidedpoly_places_vertices_like_matlab() {
    let (it, errors) = run(concat!(
        "p = nsidedpoly(4); a = p.Vertices;\n",
        "p = nsidedpoly(6, 'SideLength', 2); b = p.Vertices;\n",
        "p = nsidedpoly(5, 'Center', [1 2], 'Radius', 3); c = p.Vertices;\n",
        "p = nsidedpoly(32, 'Center', [0 0], 'Radius', 5); d = size(p.Vertices); e = p.Vertices(1, :);\n",
        "k = class(p); r = p.NumRegions; h = p.NumHoles;\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    let s = 0.70710678118654757;
    // Column-major: x column then y column.
    assert_eq!(num(&it, "a"), [-s, -s, s, s, -s, s, s, -s]);
    let t = 1.7320508075688772;
    assert_eq!(num(&it, "b"), [-1.0, -2.0, -1.0, 1.0, 2.0, 1.0, -t, 0.0, t, t, 0.0, -t]);
    assert!(close(
        &num(&it, "c"),
        &[-0.76335575687741919, -1.8531695488854609, 1.0, 3.8531695488854609, 2.7633557568774192, -0.42705098312484235, 2.9270509831248424, 5.0, 2.9270509831248424, -0.42705098312484235]
    ));
    assert_eq!(num(&it, "d"), [32.0, 2.0]);
    assert!(close(&num(&it, "e"), &[-0.49008570164780302, -4.9759236333609849]));
    assert_eq!(it.get_var("k").unwrap().text().unwrap(), "polyshape");
    assert_eq!(num(&it, "r"), [1.0]);
    assert_eq!(num(&it, "h"), [0.0]);
    let (_, errors) = run("nsidedpoly(2)");
    assert_eq!(errors, ["Number of sides must be a positive integer larger than or equal to 3."]);
}

#[test]
fn polyshape_methods_match_matlab() {
    let (it, errors) = run(concat!(
        "p = nsidedpoly(3); ar = area(p); pe = perimeter(p); [cx, cy] = centroid(p);\n",
        "q = polyshape([0 1 1 0], [0 0 1 1]); qv = q.Vertices; n = numsides(q);\n",
        "r = polyshape([0 0 1 1], [0 1 1 0]); rv = r.Vertices;\n",
        "t = translate(q, 2, 3); tv = t.Vertices;\n",
        "s = scale(q, 2); sv = s.Vertices;\n",
        "u = rotate(q, 90); uv = u.Vertices;\n",
        "in = isinterior(q, [0.5 2], [0.5 0.5]);\n",
        "[xl, yl] = boundingbox(q);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    assert!(close(&num(&it, "ar"), &[1.299038105676658]));
    assert!(close(&num(&it, "pe"), &[5.196152422706632]));
    assert!(num(&it, "cx")[0].abs() < 1e-15 && num(&it, "cy")[0].abs() < 1e-15);
    // Counterclockwise input is stored clockwise, keeping its first vertex.
    assert_eq!(num(&it, "qv"), [0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0]);
    assert_eq!(num(&it, "rv"), [0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0]);
    assert_eq!(num(&it, "n"), [4.0]);
    assert_eq!(num(&it, "tv"), [2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 3.0]);
    assert_eq!(num(&it, "sv"), [0.0, 0.0, 2.0, 2.0, 0.0, 2.0, 2.0, 0.0]);
    assert_eq!(num(&it, "uv"), [0.0, -1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
    assert_eq!(num(&it, "in"), [1.0, 0.0]);
    assert_eq!(num(&it, "xl"), [0.0, 1.0]);
    assert_eq!(num(&it, "yl"), [0.0, 1.0]);
}

#[test]
fn sind_and_cosd_are_exact_where_matlab_is() {
    let (it, errors) = run("a = [sind(30) cosd(60) cosd(240) sind(45) cosd(45) sind(150) cosd(-120)];");
    assert!(errors.is_empty(), "{errors:?}");
    let s = 0.70710678118654757;
    assert_eq!(num(&it, "a"), [0.5, 0.5, -0.5, s, s, 0.5, -0.5]);
}
