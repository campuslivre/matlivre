//! Color schemes set through root defaults (`colordef`, `graymon`) and `refreshdata`.

use crate::builtins::{none, Ret};
use crate::error::rt;
use crate::interp::Interpreter;
use crate::value::*;

pub const NAMES: &[&str] = &["colordef", "graymon", "refreshdata"];

const BLACK_ORDER: &str = "[1 1 0.0667;0.0745 0.6235 1;1 0.4118 0.1608;0.3922 0.8314 0.0745;0.7176 0.2745 1;0.0588 1 1;1 0.0745 0.651]";
const WHITE_ORDER: &str = "[0 0.447 0.741;0.85 0.325 0.098;0.929 0.694 0.125;0.494 0.184 0.556;0.466 0.674 0.188;0.301 0.745 0.933;0.635 0.078 0.184]";

/// The root defaults of `colordef` scheme `s`, recorded from MATLAB R2023b.
fn scheme(s: &str) -> Option<Vec<(&'static str, String)>> {
    let g = |v: f64| format!("[{v} {v} {v}]");
    // (text and ruler color, minor grid color, grid alpha, minor grid alpha, patch face, line,
    //  figure color, axes color, color order, colormap)
    let (fg, minor, ga, ma, face, line, fig, ax, order, map) = match s {
        "black" => (g(0.85), g(0.85), "0.35", "0.5", g(1.0), g(1.0), g(0.15), g(0.0), BLACK_ORDER.to_string(), "parula(256)"),
        "white" => (g(0.15), g(0.1), "0.15", "0.25", g(0.0), g(0.0), g(0.94), g(1.0), WHITE_ORDER.to_string(), "parula(256)"),
        "none" => (g(1.0), g(1.0), "0.15", "0.25", g(1.0), g(1.0), g(0.0), "'none'".to_string(), "[1 1 0;1 0 1;0 1 1;1 0 0;0 1 0;0 0 1]".to_string(), "hsv(64)"),
        _ => return None,
    };
    let text = if s == "white" { g(0.0) } else { fg.clone() };
    // Polar axes follow the white scheme except under black.
    let (pfg, pminor, pga, pma, pax, porder) =
        if s == "black" { (fg.clone(), minor.clone(), ga, ma, ax.clone(), order.clone()) } else { (g(0.15), g(0.1), "0.15", "0.25", g(1.0), WHITE_ORDER.to_string()) };
    let mut v: Vec<(&'static str, String)> = vec![("defaultTextColor", text)];
    for p in ["defaultAxesXColor", "defaultAxesYColor", "defaultAxesZColor", "defaultAxesGridColor"] {
        v.push((p, fg.clone()));
    }
    v.push(("defaultAxesMinorGridColor", minor));
    v.push(("defaultAxesGridAlpha", ga.into()));
    v.push(("defaultAxesMinorGridAlpha", ma.into()));
    v.push(("defaultPatchFaceColor", face));
    v.push(("defaultPatchEdgeColor", g(0.0)));
    v.push(("defaultSurfaceEdgeColor", g(0.0)));
    v.push(("defaultLineColor", line));
    v.push(("defaultFigureColor", fig));
    v.push(("defaultAxesColor", ax));
    v.push(("defaultAxesColorOrder", order));
    v.push(("defaultFigureInvertHardcopy", "'on'".into()));
    v.push(("defaultPolaraxesColor", pax));
    v.push(("defaultPolaraxesColorOrder", porder));
    for p in ["defaultPolaraxesThetaColor", "defaultPolaraxesRColor", "defaultPolaraxesGridColor"] {
        v.push((p, pfg.clone()));
    }
    v.push(("defaultPolaraxesMinorGridColor", pminor));
    v.push(("defaultPolaraxesGridAlpha", pga.into()));
    v.push(("defaultPolaraxesMinorGridAlpha", pma.into()));
    v.push(("defaultFigureColormap", map.into()));
    Some(v)
}

fn set_root(it: &mut Interpreter, items: &[(&str, String)]) -> Ret {
    for (name, value) in items {
        it.eval_text(&format!("set(groot, '{name}', {value});"), 0)?;
    }
    none()
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    match name {
        "colordef" => {
            if a.len() != 1 {
                return rt("colordef(fig, color) is not supported; use colordef(color).");
            }
            let s = a[0].text()?.to_lowercase();
            match scheme(&s) {
                Some(items) => set_root(it, &items),
                None => rt(format!("Invalid color specification: '{s}'.")),
            }
        }
        "graymon" => set_root(it, &[("defaultAxesColorOrder", "[0.75 0.75 0.75;0.5 0.5 0.5;0.25 0.25 0.25]".into())]),
        "refreshdata" => refreshdata(it, a),
        _ => none(),
    }
}

/// Re-evaluates the XDataSource / YDataSource / ZDataSource of `h` and its children.
fn refreshdata(it: &mut Interpreter, a: &[Value]) -> Ret {
    let target = match a.first() {
        Some(v) => v.clone(),
        None => it.eval_text("gcf", 1)?.into_iter().next().unwrap_or_else(Value::empty),
    };
    let Some(ids) = crate::gfx::ids_of(&target) else {
        return rt("Invalid or deleted object.");
    };
    let mut all = Vec::new();
    for id in ids {
        all.push(id);
        crate::gfx::descendants(it, id, &mut all);
    }
    for id in all {
        for (src, data) in [("XDataSource", "XData"), ("YDataSource", "YData"), ("ZDataSource", "ZData")] {
            let expr = match crate::gfx::stored(it, id, src) {
                Some(v) if v.is_text() => v.text()?,
                _ => continue,
            };
            if expr.trim().is_empty() {
                continue;
            }
            let value = it.eval_text(&expr, 1)?.into_iter().next().unwrap_or_else(Value::empty);
            crate::gfx::set_prop(it, id, data, value)?;
        }
    }
    none()
}
