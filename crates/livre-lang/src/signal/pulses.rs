//! Measurements on bilevel waveforms built on the state-level and transition detection of
//! `modeling.rs`: pulse metrics, slew rate, overshoot and settling time.

use crate::builtins::Ret;
use crate::error::rt;
use crate::signal::modeling::{crossing, find_transitions, parse_transitions, Transitions};
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["pulsewidth", "pulseperiod", "dutycycle", "pulsesep", "slewrate", "overshoot", "undershoot", "settlingtime"];

pub fn call(name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "pulsewidth" | "pulseperiod" | "dutycycle" | "pulsesep" => pulse(name, a, nargout),
        "slewrate" => slewrate(a, nargout),
        "overshoot" | "undershoot" => shoot(name, a, nargout),
        _ => settlingtime(a, nargout),
    }
}

fn is_text(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.kind == Kind::Char)
}

/// Pulls the named options in `names` (matched by case-insensitive prefix) out of `a`, returning
/// the remaining arguments and the (canonical name, value) pairs.
fn split_opts(a: &[Value], names: &[&'static str]) -> R<(Vec<Value>, Vec<(&'static str, Value)>)> {
    let mut rest = vec![];
    let mut opts = vec![];
    let mut i = 0;
    while i < a.len() {
        if i >= 1 && is_text(&a[i]) {
            let key = a[i].text().unwrap_or_default().to_lowercase();
            if let Some(c) = names.iter().find(|c| !key.is_empty() && c.starts_with(&key)) {
                if i + 1 >= a.len() {
                    return rt(format!("Parameter {c} requires a value."));
                }
                opts.push((*c, a[i + 1].clone()));
                i += 2;
                continue;
            }
        }
        rest.push(a[i].clone());
        i += 1;
    }
    Ok((rest, opts))
}

fn empty_or(v: Vec<f64>, row: bool) -> Value {
    if v.is_empty() {
        Value::Num(Matrix::from_dims(if row { vec![1, 0] } else { vec![0, 1] }, vec![]))
    } else if row {
        real_row(v)
    } else {
        real_col(v)
    }
}

fn mid_level(tr: &Transitions) -> f64 {
    let (lo, hi) = tr.levels;
    lo + tr.mid_pct / 100.0 * (hi - lo)
}

/// Mid-reference crossings of every transition as (time, rising).
fn mid_crossings(tr: &Transitions) -> Vec<(f64, bool)> {
    let mid = mid_level(tr);
    find_transitions(tr).into_iter().filter_map(|(s, e, r)| crossing(tr, s, e, r, mid).map(|t| (t, r))).collect()
}

type Pulse = (f64, f64, Option<f64>);

fn pulse(name: &str, a: &[Value], nargout: usize) -> Ret {
    // dutycycle(tau, prf)
    if name == "dutycycle" && a.len() == 2 {
        if let (Value::Num(t), Value::Num(p)) = (&a[0], &a[1]) {
            if t.numel() == 1 && t.kind != Kind::Char && p.kind != Kind::Char {
                if nargout > 1 {
                    return rt("Too many outputs specified when TAU is a scalar.");
                }
                if t.im.is_some() || !(t.data[0] > 0.0) {
                    return rt("Expected TAU to be positive.");
                }
                if p.im.is_some() || p.data.iter().any(|v| !(*v > 0.0)) {
                    return rt("Expected PRF to be positive.");
                }
                if p.data.iter().any(|v| v * t.data[0] > 1.0) {
                    return rt("The product of TAU and PRF should be less than or equal to 1.");
                }
                let d: Vec<f64> = p.data.iter().map(|v| v * t.data[0]).collect();
                return out(vec![Value::Num(Matrix::from_dims(p.dims.clone(), d))]);
            }
        }
    }
    let max_out = if name == "pulsewidth" { 4 } else { 5 };
    if nargout > max_out {
        return rt("Too many output arguments.");
    }
    let (rest, opts) = split_opts(a, &["polarity"])?;
    let mut positive = true;
    for (_, v) in &opts {
        let s = v.text().unwrap_or_default().to_lowercase();
        match s.as_str() {
            "positive" => positive = true,
            "negative" => positive = false,
            _ => return rt("POLARITY must be either 'Positive' or 'Negative'."),
        }
    }
    let tr = parse_transitions(&rest, &["tolerance", "midpercentreferencelevel", "statelevels"])?;
    let c = mid_crossings(&tr);
    // (initial, final, next) of every pulse; `next` is the following transition of the initial kind
    let mut pulses: Vec<Pulse> = vec![];
    let mut k = 0;
    while k + 1 < c.len() {
        if c[k].1 == positive && c[k + 1].1 != positive {
            let next = c.get(k + 2).map(|n| n.0);
            pulses.push((c[k].0, c[k + 1].0, next));
            k += 2;
        } else {
            k += 1;
        }
    }
    let row = tr.row;
    let cyc: Vec<Pulse> = pulses.iter().filter(|p| p.2.is_some()).cloned().collect();
    let col = |v: &[Pulse], f: &dyn Fn(&Pulse) -> f64| empty_or(v.iter().map(f).collect(), row);
    let mid = scalar_value(mid_level(&tr));
    let mut outs = match name {
        "pulsewidth" => vec![col(&pulses, &|p| p.1 - p.0), col(&pulses, &|p| p.0), col(&pulses, &|p| p.1), mid],
        "pulseperiod" => vec![col(&cyc, &|p| p.2.unwrap() - p.0), col(&cyc, &|p| p.0), col(&cyc, &|p| p.1), col(&cyc, &|p| p.2.unwrap()), mid],
        "dutycycle" => vec![col(&cyc, &|p| (p.1 - p.0) / (p.2.unwrap() - p.0)), col(&cyc, &|p| p.0), col(&cyc, &|p| p.1), col(&cyc, &|p| p.2.unwrap()), mid],
        _ => vec![col(&cyc, &|p| p.2.unwrap() - p.1), col(&cyc, &|p| p.0), col(&cyc, &|p| p.1), col(&cyc, &|p| p.2.unwrap()), mid],
    };
    outs.truncate(nargout.max(1));
    out(outs)
}

const EDGE_OPTS: &[&str] = &["tolerance", "percentreferencelevels", "statelevels"];

/// (start, end, rising, lower-crossing time, upper-crossing time) of each transition.
fn edges(tr: &Transitions) -> Vec<(usize, usize, bool, f64, f64)> {
    let (lo, hi) = tr.levels;
    let ll = lo + tr.prl.0 / 100.0 * (hi - lo);
    let ul = lo + tr.prl.1 / 100.0 * (hi - lo);
    find_transitions(tr)
        .into_iter()
        .filter_map(|(s, e, r)| {
            let (tl, tu) = (crossing(tr, s, e, r, ll)?, crossing(tr, s, e, r, ul)?);
            Some((s, e, r, tl, tu))
        })
        .collect()
}

fn check_tol(tr: &Transitions) -> R<()> {
    if tr.tol >= tr.prl.0 {
        return rt("The percent state level tolerance must be less than the lower percent reference level.");
    }
    Ok(())
}

fn slewrate(a: &[Value], nargout: usize) -> Ret {
    if nargout > 5 {
        return rt("Too many output arguments.");
    }
    let tr = parse_transitions(a, EDGE_OPTS)?;
    check_tol(&tr)?;
    let (lo, hi) = tr.levels;
    let ll = lo + tr.prl.0 / 100.0 * (hi - lo);
    let ul = lo + tr.prl.1 / 100.0 * (hi - lo);
    let (mut s, mut lt, mut ut) = (vec![], vec![], vec![]);
    for (_, _, _, tl, tu) in edges(&tr) {
        s.push((ul - ll) / (tu - tl));
        lt.push(tl);
        ut.push(tu);
    }
    let row = tr.row;
    let mut outs = vec![empty_or(s, row), empty_or(lt, row), empty_or(ut, row), scalar_value(ll), scalar_value(ul)];
    outs.truncate(nargout.max(1));
    out(outs)
}

fn shoot(name: &str, a: &[Value], nargout: usize) -> Ret {
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let (rest, opts) = split_opts(a, &["region", "seekfactor"])?;
    let (mut pre, mut seek) = (false, 3.0);
    for (k, v) in &opts {
        if *k == "region" {
            match v.text().unwrap_or_default().to_lowercase().as_str() {
                "preshoot" => pre = true,
                "postshoot" => pre = false,
                _ => return rt("REGION must be either 'Preshoot' or 'Postshoot'."),
            }
        } else {
            let m = crate::builtins::m_arg(&[v.clone()], 0, name)?;
            if m.numel() != 1 {
                return rt("Expected FACTOR to be a scalar.");
            }
            if !(m.data[0] > 0.0) {
                return rt("Expected FACTOR to be positive.");
            }
            seek = m.data[0];
        }
    }
    let tr = parse_transitions(&rest, EDGE_OPTS)?;
    check_tol(&tr)?;
    let (lo, hi) = tr.levels;
    let span = hi - lo;
    let n = tr.x.len();
    let ed = edges(&tr);
    let (mut v, mut lev, mut inst) = (vec![], vec![], vec![]);
    for (k, &(s, e, r, tl, tu)) in ed.iter().enumerate() {
        let dur = (tu - tl).abs();
        let (first, last, reference) = if pre {
            let limit = if k > 0 { tr.t[ed[k - 1].1] } else { tr.t[0] };
            let from = (tr.t[s] - seek * dur).max(limit);
            let first = (0..=s).find(|&i| tr.t[i] >= from).unwrap_or(s);
            (first, s, if r { lo } else { hi })
        } else {
            let limit = if k + 1 < ed.len() { tr.t[ed[k + 1].0] } else { tr.t[n - 1] };
            let to = (tr.t[e] + seek * dur).min(limit);
            let last = (e..n).take_while(|&i| tr.t[i] <= to).last().unwrap_or(e);
            (e, last, if r { hi } else { lo })
        };
        let mut best = first;
        for i in first..=last {
            let better = if name == "overshoot" { tr.x[i] > tr.x[best] } else { tr.x[i] < tr.x[best] };
            if better {
                best = i;
            }
        }
        let dev = if name == "overshoot" { tr.x[best] - reference } else { reference - tr.x[best] };
        v.push(dev / span * 100.0);
        lev.push(tr.x[best]);
        inst.push(tr.t[best]);
    }
    let row = tr.row;
    let mut outs = vec![empty_or(v, row), empty_or(lev, row), empty_or(inst, row)];
    outs.truncate(nargout.max(1));
    out(outs)
}

fn settlingtime(a: &[Value], nargout: usize) -> Ret {
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let nnum = a.iter().skip(1).take_while(|v| !is_text(v)).count();
    if nnum == 0 {
        return rt("Delay parameter D is not supplied.");
    }
    if nnum > 2 {
        return rt("Too many leading numeric arguments (at most 3 expected).");
    }
    let dval = match &a[nnum] {
        Value::Num(m) => m.clone(),
        _ => return rt("Delay parameter D is not supplied."),
    };
    if dval.numel() != 1 {
        return rt("Expected D to be a scalar.");
    }
    if !(dval.data[0] > 0.0) {
        return rt("Expected D to be positive.");
    }
    let d = dval.data[0];
    let mut rest: Vec<Value> = a[..nnum].to_vec();
    rest.extend_from_slice(&a[nnum + 1..]);
    let tr = parse_transitions(&rest, &["tolerance", "midpercentreferencelevel", "statelevels"])?;
    let (lo, hi) = tr.levels;
    let band = tr.tol / 100.0 * (hi - lo);
    let mid = mid_level(&tr);
    let n = tr.x.len();
    let trans = find_transitions(&tr);
    let (mut sv, mut lv, mut iv) = (vec![], vec![], vec![]);
    for (k, &(s, e, r)) in trans.iter().enumerate() {
        let Some(m) = crossing(&tr, s, e, r, mid) else { continue };
        let target = if r { hi } else { lo };
        let next_s = if k + 1 < trans.len() { trans[k + 1].0 } else { n - 1 };
        let outside = |i: usize| (tr.x[i] - target).abs() > band;
        let kk = (s..=next_s).rev().find(|&i| outside(i));
        let end = m + d;
        let ok = match kk {
            Some(kk) if kk < n - 1 => end >= tr.t[kk.max(e)] && end <= tr.t[next_s],
            _ => false,
        };
        if !ok {
            sv.push(f64::NAN);
            lv.push(f64::NAN);
            iv.push(f64::NAN);
            continue;
        }
        let kk = kk.unwrap();
        let bound = if tr.x[kk] > target { target + band } else { target - band };
        let (x0, x1) = (tr.x[kk], tr.x[kk + 1]);
        let t = tr.t[kk] + (bound - x0) / (x1 - x0) * (tr.t[kk + 1] - tr.t[kk]);
        sv.push(t - m);
        lv.push(bound);
        iv.push(t);
    }
    let row = tr.row;
    let mut outs = vec![empty_or(sv, row), empty_or(lv, row), empty_or(iv, row)];
    outs.truncate(nargout.max(1));
    out(outs)
}
