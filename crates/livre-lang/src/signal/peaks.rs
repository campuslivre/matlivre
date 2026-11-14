//! Peak finding and outlier removal: `findpeaks`, `hampel`.
//!
//! `findpeaks` follows the documented rules: a peak is a sample larger than both neighbours (or
//! `Inf`), a flat peak is reported at its lowest index, and the filters run in the order height,
//! threshold, minimum distance, prominence, width, then sorting and `NPeaks`.

use crate::builtins::{m_arg, Ret};
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["findpeaks", "hampel"];

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if name == "hampel" { hampel(a) } else { findpeaks(a, nargout) }
}

// ---- findpeaks --------------------------------------------------------------------------------

struct Options {
    min_height: f64,
    min_distance: f64,
    threshold: f64,
    npeaks: Option<usize>,
    sort: i8,
    min_prom: f64,
    min_width: f64,
    max_width: f64,
    half_height: bool,
}

const OPTION_NAMES: &[&str] =
    &["NPeaks", "SortStr", "MinPeakHeight", "MinPeakProminence", "Threshold", "MinPeakDistance", "WidthReference", "MinPeakWidth", "MaxPeakWidth", "Annotate"];

/// Resolve a (possibly abbreviated, any case) option name to its canonical spelling.
fn resolve_option(given: &str) -> R<&'static str> {
    let low = given.to_lowercase();
    if let Some(exact) = OPTION_NAMES.iter().find(|n| n.to_lowercase() == low) {
        return Ok(exact);
    }
    let matches: Vec<&&str> = OPTION_NAMES.iter().filter(|n| !low.is_empty() && n.to_lowercase().starts_with(&low)).collect();
    match matches.len() {
        1 => Ok(matches[0]),
        0 => rt(format!("'{given}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function.")),
        _ => {
            let mut names: Vec<&str> = matches.iter().map(|n| **n).collect();
            names.sort();
            let list = names.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ");
            rt(format!("'{given}' matches multiple parameter names: {list}. To avoid ambiguity, specify the complete name of the parameter."))
        }
    }
}

fn numeric_option(a: &[Value], i: usize, key: &str) -> R<f64> {
    if i >= a.len() {
        return rt(format!("No value was given for '{key}'. Name-value pair arguments require a name followed by a value."));
    }
    let m = m_arg(a, i, key)?;
    if matches!(&a[i], Value::Num(mm) if mm.kind == Kind::Char) {
        return rt(format!(
            "Expected {key} to be one of these types:\n\ndouble, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64"
        ));
    }
    if m.numel() != 1 {
        return rt(format!("Expected {key} to be a scalar."));
    }
    Ok(m.data[0])
}

fn parse_options(a: &[Value], mut i: usize) -> R<Options> {
    let mut o = Options {
        min_height: f64::NEG_INFINITY,
        min_distance: 0.0,
        threshold: 0.0,
        npeaks: None,
        sort: 0,
        min_prom: 0.0,
        min_width: 0.0,
        max_width: f64::INFINITY,
        half_height: false,
    };
    while i < a.len() {
        let Ok(given) = a[i].text() else { return rt("Expected a string scalar or character vector for the parameter name.") };
        let key = resolve_option(&given)?;
        i += 1;
        match key {
            "MinPeakHeight" => o.min_height = numeric_option(a, i, key)?,
            "MinPeakDistance" => {
                o.min_distance = numeric_option(a, i, key)?;
                if o.min_distance < 0.0 {
                    return rt("Expected MinPeakDistance to be nonnegative.");
                }
            }
            "Threshold" => {
                o.threshold = numeric_option(a, i, key)?;
                if o.threshold < 0.0 {
                    return rt("Expected Threshold to be nonnegative.");
                }
            }
            "NPeaks" => {
                let n = numeric_option(a, i, key)?;
                if n <= 0.0 {
                    return rt("Expected NPeaks to be positive.");
                }
                if n.fract() != 0.0 {
                    return rt("Expected NPeaks to be integer-valued.");
                }
                o.npeaks = Some(n as usize);
            }
            "MinPeakProminence" => o.min_prom = numeric_option(a, i, key)?,
            "MinPeakWidth" => o.min_width = numeric_option(a, i, key)?,
            "MaxPeakWidth" => o.max_width = numeric_option(a, i, key)?,
            _ => {
                if i >= a.len() {
                    return rt(format!("No value was given for '{key}'. Name-value pair arguments require a name followed by a value."));
                }
                let v = a[i].text().unwrap_or_default();
                let low = v.to_lowercase();
                let (valid, list): (&[&str], &str) = match key {
                    "SortStr" => (&["ascend", "none", "descend"], "'ascend', 'none', 'descend'"),
                    "WidthReference" => (&["halfprom", "halfheight"], "'halfprom', 'halfheight'"),
                    _ => (&["peaks", "extents"], "'peaks', 'extents'"),
                };
                if !valid.contains(&low.as_str()) {
                    return rt(format!("Expected {key} to match one of these values:\n\n{list}\n\nThe input, '{v}', did not match any of the valid values."));
                }
                match (key, low.as_str()) {
                    ("SortStr", "ascend") => o.sort = 1,
                    ("SortStr", "descend") => o.sort = -1,
                    ("WidthReference", "halfheight") => o.half_height = true,
                    _ => {}
                }
            }
        }
        i += 1;
    }
    Ok(o)
}

/// Local maxima as (first index, last index) of the flat top: strictly above both neighbours,
/// or any `Inf` sample.
fn local_maxima(x: &[f64]) -> Vec<(usize, usize)> {
    let n = x.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if x[i] == f64::INFINITY {
            out.push((i, i));
            i += 1;
            continue;
        }
        let mut j = i;
        while j + 1 < n && x[j + 1] == x[i] {
            j += 1;
        }
        if i > 0 && j + 1 < n && x[i] > x[i - 1] && x[i] > x[j + 1] {
            out.push((i, j));
        }
        i = j + 1;
    }
    out
}

/// Prominence of the peak at `i`; the indices where the left and right scans stopped; and the
/// positions of the lowest samples of the two intervals (the "bases").
struct Prominence {
    value: f64,
    left: usize,
    right: usize,
    lbase: usize,
    rbase: usize,
}

fn prominence(x: &[f64], i: usize) -> Prominence {
    let (mut lmin, mut lbase, mut left) = (x[i], i, i);
    while left > 0 {
        left -= 1;
        if x[left] > x[i] {
            break;
        }
        if x[left] < lmin {
            lmin = x[left];
            lbase = left;
        }
    }
    let (mut rmin, mut rbase, mut right) = (x[i], i, i);
    while right + 1 < x.len() {
        right += 1;
        if x[right] > x[i] {
            break;
        }
        if x[right] < rmin {
            rmin = x[right];
            rbase = right;
        }
    }
    Prominence { value: x[i] - lmin.max(rmin), left, right, lbase, rbase }
}

/// Width at the reference line, in the units of `pos`, with linear interpolation; where the line
/// lies below the interval's lowest sample the base position is used.
fn width(x: &[f64], pos: &[f64], i: usize, level: f64, p: &Prominence) -> f64 {
    let mut xl = pos[p.lbase];
    for j in (p.left..i).rev() {
        if x[j] <= level {
            xl = pos[j] + (pos[j + 1] - pos[j]) * (level - x[j]) / (x[j + 1] - x[j]);
            break;
        }
    }
    let mut xr = pos[p.rbase];
    for j in i + 1..=p.right {
        if x[j] <= level {
            xr = pos[j - 1] + (pos[j] - pos[j - 1]) * (x[j - 1] - level) / (x[j - 1] - x[j]);
            break;
        }
    }
    xr - xl
}

fn findpeaks(a: &[Value], nargout: usize) -> Ret {
    let xm = m_arg(a, 0, "findpeaks")?;
    if xm.im.as_ref().is_some_and(|im| im.iter().any(|&v| v != 0.0)) {
        return rt("Expected Y to be real.");
    }
    if xm.numel() == 0 {
        return rt("Expected Y to be nonempty.");
    }
    if xm.dims.len() > 2 || (xm.dims[0] != 1 && xm.dims[1] != 1) {
        return rt("Expected Y to be a vector.");
    }
    let x = xm.data.clone();
    let row = xm.dims[0] == 1;
    let n = x.len();
    if n < 3 {
        return rt("Data set must contain at least 3 samples.");
    }
    // The second argument may be a sample rate or a vector of locations.
    let mut next = 1;
    let mut pos: Vec<f64> = (1..=n).map(|i| i as f64).collect();
    if a.len() > 1 && a[1].text().is_err() {
        let v = rvec_arg(a, 1, "findpeaks")?;
        if v.len() == 1 {
            if v[0] <= 0.0 {
                return rt("Expected Fs to be positive.");
            }
            pos = (0..n).map(|i| i as f64 / v[0]).collect();
        } else if v.len() == n {
            if v.windows(2).any(|p| p[1] <= p[0]) {
                return rt("Expected X to be strictly increasing.");
            }
            pos = v;
        } else {
            return rt("input vectors X and Y should have the same size.");
        }
        next = 2;
    }
    let o = parse_options(a, next)?;

    let mut peaks: Vec<(usize, usize)> = local_maxima(&x);
    peaks.retain(|&(i, j)| {
        let left = if i > 0 { x[i] - x[i - 1] } else { f64::INFINITY };
        let right = if j + 1 < n { x[i] - x[j + 1] } else { f64::INFINITY };
        x[i] == f64::INFINITY || (x[i] >= o.min_height && left >= o.threshold && right >= o.threshold)
    });
    if o.min_distance > 0.0 && !peaks.is_empty() {
        let mut order: Vec<usize> = (0..peaks.len()).collect();
        order.sort_by(|&p, &q| x[peaks[q].0].total_cmp(&x[peaks[p].0]));
        let mut removed = vec![false; peaks.len()];
        for &p in &order {
            if removed[p] {
                continue;
            }
            for q in 0..peaks.len() {
                if q != p && (pos[peaks[q].0] - pos[peaks[p].0]).abs() <= o.min_distance {
                    removed[q] = true;
                }
            }
        }
        let mut k = 0;
        peaks.retain(|_| {
            k += 1;
            !removed[k - 1]
        });
    }
    // Prominences and widths for the remaining peaks.
    let mut proms = Vec::new();
    let mut widths = Vec::new();
    let mut keep = Vec::new();
    for &(i, _) in &peaks {
        let p = prominence(&x, i);
        if p.value < o.min_prom {
            continue;
        }
        let level = if o.half_height { x[i] / 2.0 } else { x[i] - p.value / 2.0 };
        let w = width(&x, &pos, i, level, &p);
        if w < o.min_width || w > o.max_width {
            continue;
        }
        keep.push(i);
        proms.push(p.value);
        widths.push(w);
    }
    let mut order: Vec<usize> = (0..keep.len()).collect();
    match o.sort {
        1 => order.sort_by(|&p, &q| x[keep[p]].total_cmp(&x[keep[q]])),
        -1 => order.sort_by(|&p, &q| x[keep[q]].total_cmp(&x[keep[p]])),
        _ => {}
    }
    if let Some(m) = o.npeaks {
        order.truncate(m);
    }
    let shape = |v: Vec<f64>| if row { real_row(v) } else { real_col(v) };
    let mut outs = vec![shape(order.iter().map(|&k| x[keep[k]]).collect())];
    if nargout >= 2 {
        outs.push(shape(order.iter().map(|&k| pos[keep[k]]).collect()));
    }
    if nargout >= 3 {
        outs.push(shape(order.iter().map(|&k| widths[k]).collect()));
    }
    if nargout >= 4 {
        outs.push(shape(order.iter().map(|&k| proms[k]).collect()));
    }
    out(outs)
}

// ---- hampel -----------------------------------------------------------------------------------

/// 1 / (sqrt(2) erfcinv(3/2)): scales the median absolute deviation to a standard deviation.
const KAPPA: f64 = 1.4826022185056018;

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|p, q| p.total_cmp(q));
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

fn hampel(a: &[Value]) -> Ret {
    if a.len() > 3 {
        return rt("Too many input arguments.");
    }
    let x = {
        let m = m_arg(a, 0, "hampel")?;
        if m.im.as_ref().is_some_and(|im| im.iter().any(|&v| v != 0.0)) {
            return rt("Expected input to be real.");
        }
        m
    };
    let k = if a.len() >= 2 { scalar_arg(a, 1, "hampel")? } else { 3.0 };
    if k <= 0.0 {
        return rt("Expected input to be positive.");
    }
    if k.fract() != 0.0 {
        return rt("Expected input to be integer-valued.");
    }
    let k = k as usize;
    let nsigma = if a.len() == 3 { scalar_arg(a, 2, "hampel")? } else { 3.0 };
    if nsigma < 0.0 {
        return rt("Expected input to be nonnegative.");
    }
    let (rows, cols) = (x.dims[0], x.dims[1]);
    // Index lists of the vectors to filter: the columns, or the single row.
    let slices: Vec<Vec<usize>> = if rows == 1 { vec![(0..cols).collect()] } else { (0..cols).map(|c| (c * rows..(c + 1) * rows).collect()).collect() };
    let total = x.numel();
    let (mut y, mut flag, mut med, mut sig) = (x.data.clone(), vec![0.0; total], vec![0.0; total], vec![0.0; total]);
    for idx in &slices {
        let len = idx.len();
        for p in 0..len {
            let (lo, hi) = (p.saturating_sub(k), (p + k).min(len - 1));
            let mut win: Vec<f64> = (lo..=hi).map(|q| x.data[idx[q]]).filter(|v| !v.is_nan()).collect();
            let m = median(&mut win);
            let mut dev: Vec<f64> = win.iter().map(|v| (v - m).abs()).collect();
            let s = KAPPA * median(&mut dev);
            let at = idx[p];
            med[at] = m;
            sig[at] = s;
            if x.data[at].is_nan() || (x.data[at] - m).abs() > nsigma * s {
                flag[at] = 1.0;
                y[at] = m;
            }
        }
    }
    out(vec![
        Value::Num(Matrix::from_dims(x.dims.clone(), y)),
        Value::Num(Matrix::logical(rows, cols, flag)),
        Value::Num(Matrix::from_dims(x.dims.clone(), med)),
        Value::Num(Matrix::from_dims(x.dims.clone(), sig)),
    ])
}
