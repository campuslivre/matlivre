//! 3-D plots, contours, images and colormaps against MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind};

fn run(src: &str) -> Interpreter {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    let errs: Vec<String> = it.take_outputs().into_iter().filter_map(|o| if let OutputKind::Error(e) = o.kind { Some(e) } else { None }).collect();
    assert!(errs.is_empty(), "{errs:?}");
    it
}

fn num(it: &Interpreter, n: &str) -> Vec<f64> {
    it.get_var(n).unwrap().to_matrix().unwrap().data
}

fn close(a: &[f64], b: &[f64], tol: f64) {
    assert_eq!(a.len(), b.len(), "{a:?} vs {b:?}");
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() <= tol, "{a:?} vs {b:?}");
    }
}

#[test]
fn colormaps_match_matlab() {
    let it = run("j = jet(7); h = hot(7); p = parula(3); b = bone(7); hs = hsv(7); l = lines(8); n = size(gray);");
    // Column-major rows of jet(7): r, g, b columns.
    close(&num(&it, "j"), &[0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 0.0, 0.5, 1.0, 1.0, 1.0, 0.5, 0.0, 1.0, 1.0, 1.0, 0.5, 0.0, 0.0, 0.0], 1e-12);
    close(&num(&it, "h")[..7], &[0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0], 1e-12);
    close(&num(&it, "p"), &[0.2422, 0.0704, 0.9769, 0.1504, 0.7457, 0.9839, 0.6603, 0.7258, 0.0805], 1e-4);
    close(&num(&it, "b")[..2], &[0.0, 0.1458333333333333], 1e-12);
    close(&num(&it, "hs")[..2], &[1.0, 1.0], 1e-12);
    close(&num(&it, "l")[7..8], &[0.0], 1e-12);
    assert_eq!(num(&it, "n"), [256.0, 3.0]);
}

#[test]
fn shapes_and_contour_matrix() {
    let it = run(concat!(
        "[xs, ys, zs] = sphere(4); zc1 = zs(:, 1)'; xs2 = xs(2, :); sz = size(xs);\n",
        "[xc, yc, zc] = cylinder(1, 4); xcr = xc(1, :);\n",
        "z = peaks(3); M = contourc([1 2; 3 4], [2.5 2.5]);\n",
    ));
    close(&num(&it, "zc1"), &[-1.0, -0.7071067811865476, 0.0, 0.7071067811865476, 1.0], 1e-12);
    close(&num(&it, "xs2"), &[-0.7071067811865476, 0.0, 0.7071067811865476, 0.0, -0.7071067811865476], 1e-12);
    assert_eq!(num(&it, "sz"), [5.0, 5.0]);
    close(&num(&it, "xcr"), &[1.0, 0.0, -1.0, 0.0, 1.0], 1e-12);
    close(&num(&it, "z"), &[6.67128029e-5, -0.0365, 0.0, -0.245, 0.981, 0.2999, 0.0, 0.0331, 0.0], 1e-4);
    // One line at 2.5 with two points (direction may differ from MATLAB's).
    let m = num(&it, "M");
    assert_eq!(m.len(), 6);
    assert_eq!(&m[..2], &[2.5, 2.0]);
}

#[test]
fn surfaces_images_and_views() {
    let it = run(concat!(
        "s = surf(peaks(5)); c = class(s); fc = s.FaceColor; ec = s.EdgeColor; v = view; zd = size(s.ZData);\n",
        "figure; im = imagesc(magic(3)); ic = class(im); cl = clim; colormap(gca, 'jet'); cm = size(colormap(gca));\n",
        "figure; [M, h] = contour(peaks(5), 3); lv = h.LevelList; hc = class(h);\n",
        "figure; plot(1:3); v2 = view; view(3); v3 = view; [az, el] = view;\n",
        "figure; q = quiver(1:3, 1:3, [1 0 1], [0 1 1]); qc = class(q);\n",
    ));
    let text = |n: &str| it.get_var(n).unwrap().text().unwrap();
    assert_eq!(text("c"), "matlab.graphics.chart.primitive.Surface");
    assert_eq!(text("fc"), "flat");
    assert_eq!(num(&it, "ec"), [0.0, 0.0, 0.0]);
    assert_eq!(num(&it, "v"), [-37.5, 30.0]);
    assert_eq!(num(&it, "zd"), [5.0, 5.0]);
    assert_eq!(text("ic"), "matlab.graphics.primitive.Image");
    assert_eq!(num(&it, "cl"), [1.0, 9.0]);
    assert_eq!(num(&it, "cm"), [256.0, 3.0]);
    close(&num(&it, "lv"), &[-2.26105, 1.15817, 4.5774], 1e-4);
    assert_eq!(text("hc"), "matlab.graphics.chart.primitive.Contour");
    assert_eq!(num(&it, "v2"), [0.0, 90.0]);
    assert_eq!(num(&it, "v3"), [-37.5, 30.0]);
    assert_eq!(num(&it, "el"), [30.0]);
    assert_eq!(text("qc"), "matlab.graphics.chart.primitive.Quiver");
}
