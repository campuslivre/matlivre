//! `findchangepts`: optimal partitioning of a signal into segments with a constant mean, rms,
//! standard deviation or linear trend.

use crate::builtins::Ret;
use crate::error::rt;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["findchangepts"];

const PARAMS: &[&str] = &["MaxNumChanges", "Statistic", "MinDistance", "MinThreshold"];
const NUM_TYPES: &str = "double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64";

#[derive(Clone, Copy, PartialEq)]
enum Stat {
    Mean,
    Rms,
    Std,
    Linear,
}

/// Prefix sums of every channel so that the cost of any segment is O(channels).
struct Costs {
    stat: Stat,
    n: usize,
    s1: Vec<Vec<f64>>,
    s2: Vec<Vec<f64>>,
    sxt: Vec<Vec<f64>>,
    x: Vec<Vec<f64>>,
}

impl Costs {
    fn new(stat: Stat, x: Vec<Vec<f64>>) -> Costs {
        let n = x.first().map_or(0, |c| c.len());
        let mut s1 = vec![];
        let mut s2 = vec![];
        let mut sxt = vec![];
        for ch in &x {
            let (mut a, mut b, mut c) = (vec![0.0; n + 1], vec![0.0; n + 1], vec![0.0; n + 1]);
            for k in 0..n {
                a[k + 1] = a[k] + ch[k];
                b[k + 1] = b[k] + ch[k] * ch[k];
                c[k + 1] = c[k] + ch[k] * k as f64;
            }
            s1.push(a);
            s2.push(b);
            sxt.push(c);
        }
        Costs { stat, n, s1, s2, sxt, x }
    }

    /// Cost of the segment `i..j` (0-based, `j` exclusive) from the prefix sums.
    fn cost(&self, i: usize, j: usize) -> f64 {
        let m = (j - i) as f64;
        let mut total = 0.0;
        for c in 0..self.x.len() {
            let a = self.s1[c][j] - self.s1[c][i];
            let b = self.s2[c][j] - self.s2[c][i];
            total += match self.stat {
                Stat::Mean => b - a * a / m,
                Stat::Rms => m * (b / m).ln(),
                Stat::Std => m * (b / m - (a / m) * (a / m)).max(0.0).ln(),
                Stat::Linear => {
                    let xt = self.sxt[c][j] - self.sxt[c][i];
                    let st = (i..j).map(|k| k as f64).sum::<f64>();
                    let stt = (i..j).map(|k| (k * k) as f64).sum::<f64>();
                    let sxx = stt - st * st / m;
                    let sxy = xt - a * st / m;
                    let base = b - a * a / m;
                    if sxx > 0.0 { base - sxy * sxy / sxx } else { base }
                }
            };
        }
        total
    }

    /// The same cost computed directly from the samples (two passes).
    fn exact(&self, i: usize, j: usize) -> f64 {
        let m = (j - i) as f64;
        let mut total = 0.0;
        for ch in &self.x {
            let seg = &ch[i..j];
            let mean = seg.iter().sum::<f64>() / m;
            total += match self.stat {
                Stat::Mean => seg.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>(),
                Stat::Rms => m * (seg.iter().map(|v| v * v).sum::<f64>() / m).ln(),
                Stat::Std => m * (seg.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / m).ln(),
                Stat::Linear => {
                    let tm = (m - 1.0) / 2.0;
                    let (mut sxy, mut sxx) = (0.0, 0.0);
                    for (k, v) in seg.iter().enumerate() {
                        let dt = k as f64 - tm;
                        sxy += dt * (v - mean);
                        sxx += dt * dt;
                    }
                    let base: f64 = seg.iter().map(|v| (v - mean) * (v - mean)).sum();
                    if sxx > 0.0 { base - sxy * sxy / sxx } else { base }
                }
            };
        }
        total
    }
}

/// Optimal partition minimizing the total cost plus `lambda` per change; ties prefer fewer
/// changes. Returns the change points (0-based start of each new segment) or None if infeasible.
fn solve(c: &Costs, lmin: usize, lambda: f64) -> Option<Vec<usize>> {
    let n = c.n;
    let mut f = vec![(f64::INFINITY, 0usize); n + 1];
    let mut prev = vec![0usize; n + 1];
    f[0] = (0.0, 0);
    for j in lmin..=n {
        let mut best = (f64::INFINITY, usize::MAX);
        let mut arg = 0;
        for i in 0..=(j - lmin) {
            if !f[i].0.is_finite() && !(f[i].0 == f64::NEG_INFINITY) {
                continue;
            }
            let pen = if i > 0 { lambda } else { 0.0 };
            let v = f[i].0 + c.cost(i, j) + pen;
            let cnt = f[i].1 + usize::from(i > 0);
            let tol = 1e-12 * (1.0 + best.0.abs().min(1e300));
            if !best.0.is_finite() || v < best.0 - tol || ((v - best.0).abs() <= tol && cnt < best.1) {
                best = (v, cnt);
                arg = i;
            }
        }
        if best.0.is_finite() || best.0 == f64::NEG_INFINITY {
            f[j] = best;
            prev[j] = arg;
        }
    }
    if !f[n].0.is_finite() && f[n].0 != f64::NEG_INFINITY {
        return None;
    }
    let mut cps = vec![];
    let mut j = n;
    while j > 0 {
        let i = prev[j];
        if i > 0 {
            cps.push(i);
        }
        j = i;
    }
    cps.reverse();
    Some(cps)
}

fn residual(c: &Costs, cps: &[usize]) -> f64 {
    let mut b = vec![0usize];
    b.extend_from_slice(cps);
    b.push(c.n);
    b.windows(2).map(|w| c.exact(w[0], w[1])).sum()
}

fn choice(s: &str, label: &str, options: &[&str]) -> R<usize> {
    let list = options.iter().map(|o| format!("'{o}'")).collect::<Vec<_>>().join(", ");
    let l = s.to_lowercase();
    match options.iter().position(|o| !l.is_empty() && o.starts_with(&l)) {
        Some(i) => Ok(i),
        None => rt(format!("Expected {label} to match one of these values:  {list}  The input, '{s}', did not match any of the valid values.")),
    }
}

fn numeric_arg(v: &Value, label: &str) -> R<Matrix> {
    match v {
        Value::Num(m) if m.kind != Kind::Char && m.kind != Kind::Logical => Ok(m.clone()),
        _ => rt(format!("Expected {label} to be one of these types:  {NUM_TYPES}")),
    }
}

pub fn call(a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 2 {
        return rt("Too many output arguments.");
    }
    let m = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        Value::Num(m) if m.kind == Kind::Char => return rt("Expected input number 1, X, to be one of these types:  single, double"),
        other => return rt(format!("Expected input number 1, X, to be one of these types:  single, double  Instead its type was {}.", other.class_name())),
    };
    if m.im.is_some() {
        return rt("Expected input number 1, X, to be real.");
    }
    if m.dims.len() > 2 {
        return rt("Expected input number 1, X, to be two-dimensional.");
    }

    let mut vals: Vec<Option<Value>> = vec![None; PARAMS.len()];
    let mut i = 1;
    while i < a.len() {
        let Some(name) = (match &a[i] {
            Value::Num(v) if v.kind == Kind::Char => a[i].text().ok(),
            _ => None,
        }) else {
            return rt("Expected a parameter name.");
        };
        let l = name.to_lowercase();
        let mut hits: Vec<usize> = (0..PARAMS.len()).filter(|&k| PARAMS[k].to_lowercase() == l).collect();
        if hits.is_empty() && !l.is_empty() {
            hits = (0..PARAMS.len()).filter(|&k| PARAMS[k].to_lowercase().starts_with(&l)).collect();
        }
        if hits.len() != 1 {
            return rt(format!("'{name}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function."));
        }
        if i + 1 >= a.len() {
            return rt(format!("No value was given for '{name}'. Name-value pair arguments require a name followed by a value."));
        }
        vals[hits[0]] = Some(a[i + 1].clone());
        i += 2;
    }

    let stat = match &vals[1] {
        Some(v) => {
            let s = match v {
                Value::Num(x) if x.kind == Kind::Char => v.text().unwrap_or_default(),
                _ => return rt("Expected Statistic to match one of these values:  'mean', 'rms', 'std', 'linear'  The input did not match any of the valid values."),
            };
            [Stat::Mean, Stat::Rms, Stat::Std, Stat::Linear][choice(&s, "Statistic", &["mean", "rms", "std", "linear"])?]
        }
        None => Stat::Mean,
    };
    let kmax = match &vals[0] {
        Some(v) => {
            let mm = numeric_arg(v, "MaxNumChanges")?;
            if mm.numel() == 0 {
                None
            } else {
                if mm.numel() != 1 {
                    return rt("Expected MaxNumChanges to be a scalar.");
                }
                if mm.data[0] < 0.0 {
                    return rt("Expected MaxNumChanges to be nonnegative.");
                }
                if mm.data[0].fract() != 0.0 {
                    return rt("Expected MaxNumChanges to be integer-valued.");
                }
                Some(mm.data[0] as usize)
            }
        }
        None => None,
    };
    let lmin_given = match &vals[2] {
        Some(v) => {
            let mm = numeric_arg(v, "MinDistance")?;
            if mm.numel() != 1 {
                return rt("Expected MinDistance to be a scalar.");
            }
            if !(mm.data[0] > 0.0) {
                return rt("Expected MinDistance to be positive.");
            }
            if mm.data[0].fract() != 0.0 {
                return rt("Expected MinDistance to be integer-valued.");
            }
            Some(mm.data[0] as usize)
        }
        None => None,
    };
    let beta = match &vals[3] {
        Some(v) => {
            let mm = numeric_arg(v, "MinThreshold")?;
            if mm.numel() != 1 {
                return rt("Expected MinThreshold to be a scalar.");
            }
            if mm.data[0] < 0.0 {
                return rt("Expected MinThreshold to be nonnegative.");
            }
            Some(mm.data[0])
        }
        None => None,
    };
    if vals[0].is_some() && vals[3].is_some() {
        return rt("The 'MaxNumChanges' parameter cannot be specified with the 'MinThreshold' parameter.");
    }
    if let Some(l) = lmin_given {
        if stat != Stat::Mean && l < 2 {
            let nm = match stat {
                Stat::Rms => "rms",
                Stat::Std => "std",
                _ => "linear",
            };
            return rt(format!("The value of LMIN ({l}) must be at least 2 when using '{nm}'."));
        }
    }
    let lmin = lmin_given.unwrap_or(if stat == Stat::Mean { 1 } else { 2 });

    // channels are rows; a column vector is one channel
    let (rows, cols) = if m.dims[0] == 0 || m.dims[1] == 0 { (0, 0) } else { (m.dims[0], m.dims[1]) };
    let column = cols == 1 && rows >= 1;
    let (nch, n) = if column { (1, rows) } else { (rows, cols) };
    let x: Vec<Vec<f64>> = (0..nch)
        .map(|c| {
            (0..n)
                .map(|k| if column { m.data[k] } else { m.data[c + rows * k] })
                .collect()
        })
        .collect();
    let costs = Costs::new(stat, x);

    let empty = |kind: u8| -> Value {
        let dims = match kind {
            0 => vec![0, 0],
            _ if column => vec![0, 1],
            _ => vec![1, 0],
        };
        Value::Num(Matrix::from_dims(dims, vec![]))
    };
    let shape = |cps: &[usize]| -> Value {
        let v: Vec<f64> = cps.iter().map(|&p| (p + 1) as f64).collect();
        if column { real_col(v) } else { real_row(v) }
    };
    let total = |cps: &[usize]| if n == 0 { 0.0 } else { residual(&costs, cps) };

    let (cps, kind): (Vec<usize>, u8) = if let Some(b) = beta {
        (solve(&costs, lmin, b).unwrap_or_default(), 0)
    } else if let Some(k) = kmax {
        if k == 0 || n < 2 * lmin {
            (vec![], 1)
        } else {
            let at = |l: f64| solve(&costs, lmin, l).unwrap_or_default();
            let base = at(0.0);
            if base.len() <= k {
                (base, 1)
            } else {
                let mut hi = 1.0;
                while at(hi).len() > k && hi < 1e300 {
                    hi *= 4.0;
                }
                let mut lo = 0.0;
                for _ in 0..200 {
                    let mid = 0.5 * (lo + hi);
                    if at(mid).len() <= k {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                    if hi - lo <= 1e-15 * hi {
                        break;
                    }
                }
                (at(hi), 1)
            }
        }
    } else if n < 2 * lmin {
        (vec![], 1)
    } else {
        let mut best = (f64::INFINITY, 0usize);
        for i in lmin..=(n - lmin) {
            let v = costs.cost(0, i) + costs.cost(i, n);
            if v < best.0 - 1e-12 * (1.0 + best.0.abs().min(1e300)) {
                best = (v, i);
            }
        }
        (vec![best.1], 1)
    };
    let mut outs = vec![if cps.is_empty() { empty(kind) } else { shape(&cps) }];
    if nargout > 1 {
        outs.push(scalar_value(total(&cps)));
    }
    out(outs)
}
