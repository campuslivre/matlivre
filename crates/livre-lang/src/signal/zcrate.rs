//! `zerocrossrate`: zero-crossing rate, count and crossing indices of each channel and window.

use crate::builtins::Ret;
use crate::error::rt;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["zerocrossrate"];

const PARAMS: &[&str] = &["Method", "WindowLength", "OverlapLength", "Level", "Threshold", "TransitionEdge", "ZeroPositive", "InitialState"];
const NUM_TYPES: &str = "double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64";

fn numeric(v: &Value, label: &str, allow_logical: bool) -> R<Matrix> {
    match v {
        Value::Num(m) if m.kind != Kind::Char && (allow_logical || m.kind != Kind::Logical) => Ok(m.clone()),
        _ => {
            let types = if allow_logical { format!("logical, {NUM_TYPES}") } else { NUM_TYPES.to_string() };
            rt(format!("Expected {label} to be one of these types:  {types}"))
        }
    }
}

fn scalar(v: &Value, label: &str) -> R<f64> {
    let m = numeric(v, label, false)?;
    if m.numel() != 1 {
        return rt(format!("Expected {label} to be a scalar."));
    }
    Ok(m.data[0])
}

fn choice(v: &Value, label: &str, options: &[&str]) -> R<usize> {
    let list = options.iter().map(|o| format!("'{o}'")).collect::<Vec<_>>().join(", ");
    let Some(s) = v.text().ok() else {
        return rt(format!("Expected {label} to match one of these values:  {list}  The input did not match any of the valid values."));
    };
    let l = s.to_lowercase();
    match options.iter().position(|o| !l.is_empty() && o.starts_with(&l)) {
        Some(i) => Ok(i),
        None => rt(format!("Expected {label} to match one of these values:  {list}  The input, '{s}', did not match any of the valid values.")),
    }
}

pub fn call(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        _ => return rt("Expected X to be one of these types:  single, double"),
    };
    if x.numel() == 0 {
        return rt("Expected X to be nonempty.");
    }
    if x.im.is_some() {
        return rt("Expected X to be real.");
    }
    if x.dims.len() > 2 {
        return rt("Expected X to be two-dimensional.");
    }
    let (len, ch) = if x.dims[0] == 1 { (x.dims[1], 1) } else { (x.dims[0], x.dims[1]) };
    let single = x.kind == Kind::Single;

    // name-value pairs
    let mut vals: Vec<Option<Value>> = vec![None; PARAMS.len()];
    let mut i = 1;
    while i < a.len() {
        let Some(name) = a[i].text().ok() else {
            return rt("Expected a string scalar or character vector for the parameter name.");
        };
        let l = name.to_lowercase();
        let hits: Vec<usize> = (0..PARAMS.len()).filter(|&k| PARAMS[k].to_lowercase() == l).collect();
        let hits = if hits.is_empty() && !l.is_empty() { (0..PARAMS.len()).filter(|&k| PARAMS[k].to_lowercase().starts_with(&l)).collect() } else { hits };
        if hits.len() != 1 {
            return rt(format!("'{name}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function."));
        }
        if i + 1 >= a.len() {
            return rt(format!("No value was given for '{name}'. Name-value pair arguments require a name followed by a value."));
        }
        vals[hits[0]] = Some(a[i + 1].clone());
        i += 2;
    }
    let get = |k: usize| vals[k].as_ref();

    let comparison = match get(0) {
        Some(v) => choice(v, "method", &["difference", "comparison"])? == 1,
        None => false,
    };
    let edge = match get(5) {
        Some(v) => choice(v, "transitionEdge", &["rising", "falling", "both"])?,
        None => 2,
    };
    let level = match get(3) {
        Some(v) => scalar(v, "level")?,
        None => 0.0,
    };
    let thr = match get(4) {
        Some(v) => scalar(v, "threshold")?,
        None => 0.0,
    };
    let zero_pos = match get(6) {
        Some(v) => {
            let m = numeric(v, "zeroPositive", true)?;
            if m.numel() != 1 {
                return rt("Expected zeroPositive to be a scalar.");
            }
            if m.data[0] != 0.0 && m.data[0] != 1.0 {
                return rt("Expected zeroPositive to be binary.");
            }
            m.data[0] == 1.0
        }
        None => false,
    };
    let init: Vec<f64> = match get(7) {
        Some(v) => {
            let m = numeric(v, "initialState", false)?;
            if m.numel() != ch {
                return rt(format!("Expected initialState to be an array with number of elements equal to {ch}."));
            }
            m.data.clone()
        }
        None => vec![0.0; ch],
    };
    let win = match get(1) {
        Some(v) => {
            let w = scalar(v, "windowLength")?;
            if !(w > 0.0) {
                return rt("Expected windowLength to be positive.");
            }
            if w.fract() != 0.0 {
                return rt("Expected windowLength to be integer-valued.");
            }
            if w > len as f64 {
                return rt(format!("Expected windowLength to be a scalar with value <= {len}."));
            }
            w as usize
        }
        None => len,
    };
    let ov = match get(2) {
        Some(v) => {
            let o = scalar(v, "overlapLength")?;
            if o < 0.0 {
                return rt("Expected overlapLength to be nonnegative.");
            }
            if o.fract() != 0.0 {
                return rt("Expected overlapLength to be integer-valued.");
            }
            if o >= win as f64 {
                return rt(format!("Expected overlapLength to be a scalar with value < {win}."));
            }
            o as usize
        }
        None => 0,
    };
    let step = win - ov;
    let nw = (len - ov) / step;

    let sgn = |v: f64| -> i32 {
        let mut v = v - level;
        if v.abs() <= thr {
            v = 0.0;
        }
        if v > 0.0 {
            1
        } else if v < 0.0 {
            -1
        } else if zero_pos {
            1
        } else {
            0
        }
    };
    let mut rate = vec![0.0; nw * ch];
    let mut count = vec![0.0; nw * ch];
    let mut idx = vec![0.0; nw * win * ch];
    for c in 0..ch {
        let col: Vec<f64> = (0..len).map(|k| if x.dims[0] == 1 { x.data[k] } else { x.data[c * len + k] }).collect();
        let s: Vec<i32> = col.iter().map(|&v| sgn(v)).collect();
        let s0 = sgn(init[c]);
        let mut mark = vec![false; len];
        let mut amt = vec![0.0; len];
        let mut prev = s0;
        let mut last = s0;
        for k in 0..len {
            if comparison {
                if s[k] != 0 {
                    if last != 0 && s[k] != last && (edge == 2 || (edge == 0) == (s[k] > 0)) {
                        mark[k] = true;
                        amt[k] = 1.0;
                    }
                    last = s[k];
                }
            } else {
                let d = s[k] - prev;
                if d != 0 && (edge == 2 || (edge == 0) == (d > 0)) {
                    mark[k] = true;
                    amt[k] = d.abs() as f64 / 2.0;
                }
                prev = s[k];
            }
        }
        for w in 0..nw {
            let start = w * step;
            let mut total = 0.0;
            for k in 0..win {
                total += amt[start + k];
                idx[w + nw * (k + win * c)] = if mark[start + k] { 1.0 } else { 0.0 };
            }
            count[w + nw * c] = total;
            rate[w + nw * c] = total / win as f64;
        }
    }
    let cast = |v: Vec<f64>| -> Value {
        let mut m = Matrix::from_dims(vec![nw, ch], v);
        if single {
            m.data.iter_mut().for_each(|e| *e = *e as f32 as f64);
            m.kind = Kind::Single;
        }
        Value::Num(m)
    };
    let mut outs = vec![cast(rate)];
    if nargout > 1 {
        outs.push(cast(count));
    }
    if nargout > 2 {
        let dims = if ch == 1 { vec![nw, win] } else { vec![nw, win, ch] };
        let mut m = Matrix::from_dims(dims, idx);
        m.kind = Kind::Logical;
        outs.push(Value::Num(m));
    }
    out(outs)
}
