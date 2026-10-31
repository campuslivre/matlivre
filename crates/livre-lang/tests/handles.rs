//! Graphics handles, set/get and axes formatting against MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind};

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
        .filter(|s| !s.is_empty())
        .collect();
    (it, out)
}

fn num(it: &Interpreter, n: &str) -> Vec<f64> {
    it.get_var(n).unwrap().to_matrix().unwrap().data
}

fn text(it: &Interpreter, n: &str) -> String {
    it.get_var(n).unwrap().text().unwrap()
}

#[test]
fn handles_get_set_and_dot_access() {
    let (it, out) = run(concat!(
        "f = figure; h = plot(1:3, [4 5 6]); hold on\n",
        "c = class(h); lw0 = get(h, 'LineWidth'); set(h, 'LineWidth', 2); lw = h.LineWidth;\n",
        "h.Color = 'r'; col = h.Color; hs = plot([1 2; 3 4]); sz = size(hs); set(hs, 'LineStyle', '--'); ls2 = hs(2).LineStyle;\n",
        "lsall = get(hs, 'LineStyle'); ax = gca; xl = get(ax, 'XLim'); set(gca, 'YScale', 'log'); ys = ax.YScale;\n",
        "same = gcf == f; g = isgraphics(h); n = isgraphics(5); nf = numel(fieldnames(get(h)));\n",
        "ax.XLabel.String = 'abc'; xls = ax.XLabel.String; t = title('Hi'); ts = t.String; tc = class(t);\n",
        "try, set(h, 'Bogus', 1), catch e, m1 = e.message; end\n",
        "try, h.linewidth, catch e, m2 = e.message; end\n",
        "delete(h); v = isvalid(h);\n",
        "try, h.Color, catch e, m3 = e.message; end\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    assert_eq!(text(&it, "c"), "matlab.graphics.chart.primitive.Line");
    assert_eq!(num(&it, "lw0"), [0.5]);
    assert_eq!(num(&it, "lw"), [2.0]);
    assert_eq!(num(&it, "col"), [1.0, 0.0, 0.0]);
    assert_eq!(num(&it, "sz"), [2.0, 1.0]);
    assert_eq!(text(&it, "ls2"), "--");
    assert_eq!(num(&it, "xl"), [1.0, 3.0]);
    assert_eq!(text(&it, "ys"), "log");
    assert_eq!(num(&it, "same"), [1.0]);
    assert_eq!(num(&it, "g"), [1.0]);
    assert_eq!(num(&it, "n"), [0.0]);
    assert_eq!(num(&it, "nf"), [50.0]);
    assert_eq!(text(&it, "xls"), "abc");
    assert_eq!(text(&it, "ts"), "Hi");
    assert_eq!(text(&it, "tc"), "matlab.graphics.primitive.Text");
    assert_eq!(text(&it, "m1"), "Unrecognized property Bogus for class Line.");
    assert_eq!(text(&it, "m2"), "Unrecognized method, property, or field 'linewidth' for class 'matlab.graphics.chart.primitive.Line'.");
    assert_eq!(num(&it, "v"), [0.0]);
    assert_eq!(text(&it, "m3"), "Invalid or deleted object.");
}

#[test]
fn handle_display_matches_matlab() {
    let (_, out) = run("h = plot(1:3, [4 5 6])\nx = xline(2)\n");
    assert_eq!(
        out[0],
        "h = \n  Line with properties:\n\n              Color: [0 0.4470 0.7410]\n          LineStyle: '-'\n          LineWidth: 0.5000\n             Marker: 'none'\n         MarkerSize: 6\n    MarkerFaceColor: 'none'\n              XData: [1 2 3]\n              YData: [4 5 6]\n\n  Use GET to show all properties"
    );
    assert!(out[1].contains("ConstantLine with properties:\n\n    InterceptAxis: 'x'\n            Value: 2\n            Color: [0.1500 0.1500 0.1500]"), "{}", out[1]);
}

#[test]
fn axes_formatting_functions() {
    let (it, out) = run(concat!(
        "plot(1:10, (1:10).^2); hold on\n",
        "xticks([0 5 10]); xt = xticks; xticklabels({'zero','five','ten'}); xtl = xticklabels;\n",
        "ytickformat('%.1f'); fmt = ytickformat; ax = gca; rf = ax.YAxis.TickLabelFormat;\n",
        "xtickangle(45); ang = ax.XTickLabelRotation; xlim([0 20]); lim = xlim;\n",
        "t = text(2, 50, 'note'); ts = t.String; l = xline(3, '--r', 'limit'); ll = l.Label; lc = l.Color;\n",
        "e = errorbar(1:3, [2 3 4], [0.5 0.5 0.5]); ec = class(e); p = fill([0 1 1], [0 0 1], 'g'); pc = p.FaceColor;\n",
        "b = barh([3 1 2]); bh = b.Horizontal; semilogy(1:3, [10 100 1000]); sc = get(gca, 'YScale');\n",
        "try, saveas(gcf, 'x.png'), catch err, msg = err.message; end\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    assert_eq!(num(&it, "xt"), [0.0, 5.0, 10.0]);
    assert!(matches!(it.get_var("xtl"), Some(livre_lang::Value::Cell(c)) if c.dims == [3, 1]));
    assert_eq!(text(&it, "fmt"), "%.1f");
    assert_eq!(text(&it, "rf"), "%.1f");
    assert_eq!(num(&it, "ang"), [45.0]);
    assert_eq!(num(&it, "lim"), [0.0, 20.0]);
    assert_eq!(text(&it, "ts"), "note");
    assert_eq!(text(&it, "ll"), "limit");
    assert_eq!(num(&it, "lc"), [1.0, 0.0, 0.0]);
    assert_eq!(text(&it, "ec"), "matlab.graphics.chart.primitive.ErrorBar");
    assert_eq!(num(&it, "pc"), [0.0, 1.0, 0.0]);
    assert_eq!(text(&it, "bh"), "on");
    assert_eq!(text(&it, "sc"), "log");
    // Without the editor's renderer, saving explains why it cannot.
    assert!(text(&it, "msg").contains("renderer"));
}
