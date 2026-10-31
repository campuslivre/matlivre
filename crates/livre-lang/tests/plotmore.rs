//! Layouts, annotations, lights, camera and chart helpers against MATLAB R2023b values.

use livre_lang::Interpreter;

fn run(src: &str) -> Interpreter {
    let mut it = Interpreter::new();
    it.run_source(src).unwrap_or_else(|e| panic!("{src}: {e}"));
    it
}

fn nums(it: &Interpreter, v: &str) -> Vec<f64> {
    it.get_var(v).unwrap_or_else(|| panic!("no {v}")).to_matrix().unwrap().data
}

fn text(it: &Interpreter, v: &str) -> String {
    it.get_var(v).unwrap_or_else(|| panic!("no {v}")).text().unwrap()
}

fn close(a: &[f64], b: &[f64]) {
    assert_eq!(a.len(), b.len(), "{a:?} vs {b:?}");
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() < 1e-9, "{a:?} vs {b:?}");
    }
}

#[test]
fn transforms_match_matlab() {
    let it = run("V = viewmtx(-37.5, 30); T = makehgtform('translate', [1 2 3]); S = makehgtform('scale', [2 3 4], 'zrotate', pi/4); A = makehgtform('axisrotate', [1 1 0], pi/3);");
    // Column-major.
    close(&nums(&it, "V")[..3], &[0.793353340291235, 0.30438071450436, -0.527202862365669]);
    close(&nums(&it, "V")[4..7], &[-0.608761429008721, 0.396676670145617, -0.68706414686945]);
    close(&nums(&it, "T")[12..15], &[1.0, 2.0, 3.0]);
    close(&nums(&it, "S")[..2], &[1.4142135623731, 2.12132034355964]);
    close(&nums(&it, "S")[4..6], &[-1.41421356237309, 2.12132034355964]);
    close(&nums(&it, "A")[8..11], &[0.612372435695794, -0.612372435695794, 0.5]);
}

#[test]
fn hist_brighten_texlabel_palettes() {
    let it = run(concat!(
        "[n, x] = hist([1 2 2 3 3 3 4 5 9]); [n3, x3] = hist([1 2 2 3 3 3 4 5 9], 3);\n",
        "b = brighten(gray(4), 0.5); t1 = texlabel('alpha*sin(x)^2'); t2 = texlabel('x_1 + y^(n+1)'); t3 = texlabel('lambda12');\n",
        "s = orderedcolors('sail'); a = alphamap; g = orderedcolors('gem12');\n",
    ));
    close(&nums(&it, "n"), &[1.0, 2.0, 3.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    close(&nums(&it, "x"), &[1.4, 2.2, 3.0, 3.8, 4.6, 5.4, 6.2, 7.0, 7.8, 8.6]);
    close(&nums(&it, "n3"), &[6.0, 2.0, 1.0]);
    close(&nums(&it, "x3"), &[2.33333333333333, 5.0, 7.66666666666667]);
    close(&nums(&it, "b")[..4], &[0.0, 0.577350269189626, 0.816496580927726, 1.0]);
    assert_eq!(text(&it, "t1"), r"{\alpha} {sin}({x})^{2}");
    assert_eq!(text(&it, "t2"), "{x}_{1} + {y}^{{n}+{1}}");
    assert_eq!(text(&it, "t3"), r"{\lambda_{12}}");
    close(&nums(&it, "s")[..5], &[0.062, 0.329, 1.0, 0.564, 0.066]);
    let a = nums(&it, "a");
    assert_eq!(a.len(), 64);
    assert!((a[1] - 1.0 / 63.0).abs() < 1e-12);
    assert_eq!(nums(&it, "g").len(), 36);
}

#[test]
fn layouts_annotations_lights_and_classes() {
    let it = run(concat!(
        "t = tiledlayout(2, 3); c1 = class(t); k = tilenum(t, 2, 1); [r, c] = tilerowcol(t, 5); ax = nexttile; ca = class(ax); g = t.GridSize;\n",
        "figure; p = pie([1 2 3]); cp = class(p); np = numel(p); l1 = p(2).String; l3 = p(6).String;\n",
        "figure; p = pie([1 1000]); s1 = p(2).String; s2 = p(4).String;\n",
        "figure; h = rectangle('Position', [0 0 2 1]); cr = class(h); pr = h.Position;\n",
        "figure; surf(peaks(5)); h = light; cl = class(h); lp = h.Position; ls = h.Style; h2 = camlight; cs = h2.Style;\n",
        "va = camva; camzoom(2); va2 = camva; cj = camproj; camorbit(20, 10); vw = get(gca, 'View');\n",
        "an = annotation('arrow', [0.1 0.5], [0.1 0.5]); ca1 = class(an); ax1 = an.X; tb = annotation('textbox', [0.2 0.2 0.3 0.1], 'String', 'hi'); ca2 = class(tb);\n",
        "figure; co = colororder; colororder('meadow'); co2 = colororder;\n",
    ));
    assert_eq!(text(&it, "c1"), "matlab.graphics.layout.TiledChartLayout");
    close(&nums(&it, "k"), &[4.0]);
    close(&[nums(&it, "r")[0], nums(&it, "c")[0]], &[2.0, 2.0]);
    assert_eq!(text(&it, "ca"), "matlab.graphics.axis.Axes");
    close(&nums(&it, "g"), &[2.0, 3.0]);
    assert_eq!(text(&it, "cp"), "matlab.graphics.primitive.Data");
    close(&nums(&it, "np"), &[6.0]);
    assert_eq!(text(&it, "l1"), "17%");
    assert_eq!(text(&it, "l3"), "50%");
    assert_eq!(text(&it, "s1"), "< 1%");
    assert_eq!(text(&it, "s2"), "100%");
    assert_eq!(text(&it, "cr"), "matlab.graphics.primitive.Rectangle");
    close(&nums(&it, "pr"), &[0.0, 0.0, 2.0, 1.0]);
    assert_eq!(text(&it, "cl"), "matlab.graphics.primitive.Light");
    close(&nums(&it, "lp"), &[1.0, 0.0, 1.0]);
    assert_eq!(text(&it, "ls"), "infinite");
    assert_eq!(text(&it, "cs"), "local");
    close(&nums(&it, "va"), &[10.339584907202]);
    // 2*atan(tan(va/2)/2): MATLAB gives 5.1978 after a camorbit changed the angle first.
    close(&nums(&it, "va2"), &[5.180336284509495]);
    assert_eq!(text(&it, "cj"), "orthographic");
    close(&nums(&it, "vw"), &[-17.5, 40.0]);
    assert_eq!(text(&it, "ca1"), "matlab.graphics.shape.Arrow");
    close(&nums(&it, "ax1"), &[0.1, 0.5]);
    assert_eq!(text(&it, "ca2"), "matlab.graphics.shape.TextBox");
    close(&nums(&it, "co")[..3], &[0.0, 0.85, 0.929]);
    close(&nums(&it, "co2")[..3], &[0.007, 0.227, 1.0]);
}
