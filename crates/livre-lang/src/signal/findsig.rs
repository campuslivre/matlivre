//! `findsignal`: locate a signal inside a longer record by a sliding distance (fixed alignment).

use crate::builtins::Ret;
use crate::error::rt;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["findsignal"];

const PARAMS: &[&str] = &["Normalization", "NormalizationLength", "MaxDistance", "MaxNumSegments", "TimeAlignment", "EDRTolerance", "Metric", "Annotate"];
const NUM_TYPES: &str = "double, single, uint8, uint16, uint32, uint64, int8, int16, int32, int64";

fn choice(v: &Value, label: &str, options: &[&str]) -> R<usize> {
    let list = options.iter().map(|o| format!("'{o}'")).collect::<Vec<_>>().join(", ");
    let is_text = matches!(v, Value::Num(m) if m.kind == Kind::Char) || matches!(v, Value::Str(_));
    if !is_text {
        return rt(format!("Expected {label} to match one of these values:  {list}  The input did not match any of the valid values."));
    }
    let s = v.text().unwrap_or_default();
    let l = s.to_lowercase();
    match options.iter().position(|o| !l.is_empty() && o.starts_with(&l)) {
        Some(i) => Ok(i),
        None => rt(format!("Expected {label} to match one of these values:  {list}  The input, '{s}', did not match any of the valid values.")),
    }
}

fn mean(v: &[(f64, f64)]) -> (f64, f64) {
    let n = v.len() as f64;
    (v.iter().map(|c| c.0).sum::<f64>() / n, v.iter().map(|c| c.1).sum::<f64>() / n)
}

/// Matrix of complex samples: `rows` channels by `cols` time samples, row-major per channel.
struct Mat {
    rows: usize,
    cols: usize,
    d: Vec<(f64, f64)>,
}

impl Mat {
    fn at(&self, r: usize, c: usize) -> (f64, f64) {
        self.d[r * self.cols + c]
    }
}

fn to_mat(m: &Matrix) -> Mat {
    let (r0, c0) = (m.dims[0], m.dims[1]);
    let im = m.im.clone().unwrap_or_else(|| vec![0.0; m.numel()]);
    if r0 == 1 || c0 == 1 {
        let n = m.numel();
        Mat { rows: 1, cols: n, d: (0..n).map(|k| (m.data[k], im[k])).collect() }
    } else {
        let mut d = Vec::with_capacity(m.numel());
        for r in 0..r0 {
            for c in 0..c0 {
                d.push((m.data[r + r0 * c], im[r + r0 * c]));
            }
        }
        Mat { rows: r0, cols: c0, d }
    }
}

fn normalize(m: &mut Mat, mode: usize) {
    if mode == 3 {
        return;
    }
    match mode {
        0 | 1 => {
            // center / zscore: subtract each row's mean
            let orig_all: Vec<(f64, f64)> = m.d.clone();
            for r in 0..m.rows {
                let row: Vec<(f64, f64)> = (0..m.cols).map(|c| m.at(r, c)).collect();
                let mu = mean(&row);
                for c in 0..m.cols {
                    m.d[r * m.cols + c].0 -= mu.0;
                    m.d[r * m.cols + c].1 -= mu.1;
                }
            }
            if mode == 1 {
                // standard deviation of all elements (of the uncentered data for matrices)
                let basis = if m.rows == 1 { m.d.clone() } else { orig_all };
                let mu = mean(&basis);
                let n = basis.len() as f64;
                let var = basis.iter().map(|c| (c.0 - mu.0).powi(2) + (c.1 - mu.1).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
                let sd = var.sqrt();
                for c in m.d.iter_mut() {
                    c.0 /= sd;
                    c.1 /= sd;
                }
            }
        }
        _ => {
            // power: divide by the mean of all elements
            let mu = mean(&m.d);
            let den = mu.0 * mu.0 + mu.1 * mu.1;
            for c in m.d.iter_mut() {
                let (a, b) = (c.0, c.1);
                *c = ((a * mu.0 + b * mu.1) / den, (b * mu.0 - a * mu.1) / den);
            }
        }
    }
}

pub fn call(a: &[Value], nargout: usize) -> Ret {
    if a.len() < 2 {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let get = |v: &Value, pos: usize, label: &str| -> R<Matrix> {
        match v {
            Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => Ok(m.clone()),
            _ => rt(format!("Expected input number {pos}, {label}, to be one of these types:  single, double")),
        }
    };
    let data = get(&a[0], 1, "DATA")?;
    let sig = get(&a[1], 2, "SIGNAL")?;
    let single = data.kind == Kind::Single || sig.kind == Kind::Single;
    let mut vals: Vec<Option<Value>> = vec![None; PARAMS.len()];
    let mut i = 2;
    while i < a.len() {
        let name = match &a[i] {
            Value::Num(m) if m.kind == Kind::Char => a[i].text().unwrap_or_default(),
            Value::Str(_) => a[i].text().unwrap_or_default(),
            _ => return rt("Expected a string scalar or character vector for the parameter name."),
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
    let norm = match &vals[0] {
        Some(v) => choice(v, "Normalization", &["center", "zscore", "power", "none"])?,
        None => 3,
    };
    let metric = match &vals[6] {
        Some(v) => choice(v, "Metric", &["absolute", "euclidean", "squared", "symmkl"])?,
        None => 2,
    };
    let align = match &vals[4] {
        Some(v) => choice(v, "MaxDistance", &["fixed", "dtw", "edr"])?,
        None => 0,
    };
    if let Some(v) = &vals[7] {
        choice(v, "input", &["data", "signal", "all"])?;
    }
    let numeric = |v: &Value, label: &str| -> R<Matrix> {
        match v {
            Value::Num(m) if m.kind != Kind::Char && m.kind != Kind::Logical => Ok(m.clone()),
            _ => rt(format!("Expected {label} to be one of these types:  {NUM_TYPES}")),
        }
    };
    let maxdist = match &vals[2] {
        Some(v) => {
            let m = numeric(v, "MaxDistance")?;
            if m.numel() != 1 {
                return rt("Expected MaxDistance to be a scalar.");
            }
            if m.data[0] < 0.0 {
                return rt("Expected MaxDistance to be nonnegative.");
            }
            Some(m.data[0])
        }
        None => None,
    };
    let maxseg = match &vals[3] {
        Some(v) => {
            let m = numeric(v, "MaxNumSegments")?;
            if m.numel() != 1 {
                return rt("Expected MaxNumSegments to be a scalar.");
            }
            if !(m.data[0] > 0.0) {
                return rt("Expected MaxNumSegments to be positive.");
            }
            if m.data[0].is_finite() && m.data[0].fract() != 0.0 {
                return rt("Expected MaxNumSegments to be integer-valued.");
            }
            m.data[0]
        }
        None => {
            if maxdist.is_some() { f64::INFINITY } else { 1.0 }
        }
    };
    let mut nlen_full = true;
    if let Some(v) = &vals[1] {
        let m = numeric(v, "NormalizationLength")?;
        if m.numel() != 1 || !(m.data[0] > 1.0) {
            return rt("Expected NormalizationLength to be a scalar with value > 1.");
        }
        if m.data[0] < data.numel().max(sig.numel()) as f64 {
            nlen_full = false;
        }
    }
    if !nlen_full {
        return rt("findsignal: windowed normalization (NormalizationLength) is not implemented.");
    }
    if align != 0 {
        return rt("findsignal: only the 'fixed' time alignment is implemented.");
    }
    let col_data = data.dims[1] == 1 && data.dims[0] > 1;
    let mut d = to_mat(&data);
    let mut s = to_mat(&sig);
    let data_is_matrix = data.dims[0] > 1 && data.dims[1] > 1;
    let sig_is_matrix = sig.dims[0] > 1 && sig.dims[1] > 1;
    if (data_is_matrix || sig_is_matrix) && d.rows != s.rows {
        return rt("The number of rows between SIGNAL and DATA must be equal when either SIGNAL or DATA are matrices");
    }
    if metric == 3 {
        let bad = d.d.iter().chain(s.d.iter()).any(|c| c.0 < 0.0 || c.1 != 0.0);
        if bad {
            return rt("DATA must be real and non-negative when using the 'symmkl' distance metric");
        }
    }
    let shape = |v: Vec<f64>| -> Value {
        if v.is_empty() {
            let dims = if data.numel() == 0 { vec![1, 0] } else if col_data || data.dims[1] == 1 { vec![0, 1] } else { vec![1, 0] };
            return Value::Num(Matrix::from_dims(dims, vec![]));
        }
        if col_data { real_col(v) } else { real_row(v) }
    };
    let (n, m) = (d.cols, s.cols);
    if m == 0 || n < m || n == 0 {
        let e = shape(vec![]);
        return out(vec![e.clone(), e.clone(), e].into_iter().take(nargout.max(1)).collect());
    }
    normalize(&mut d, norm);
    normalize(&mut s, norm);
    let nd = n - m + 1;
    let mut dist = vec![0.0; nd];
    for (i0, di) in dist.iter_mut().enumerate() {
        let mut acc = 0.0;
        match metric {
            0 => {
                for r in 0..d.rows {
                    for j in 0..m {
                        let (x, y) = (d.at(r, i0 + j), s.at(r, j));
                        acc += ((x.0 - y.0).powi(2) + (x.1 - y.1).powi(2)).sqrt();
                    }
                }
            }
            1 => {
                for j in 0..m {
                    let mut col = 0.0;
                    for r in 0..d.rows {
                        let (x, y) = (d.at(r, i0 + j), s.at(r, j));
                        col += (x.0 - y.0).powi(2) + (x.1 - y.1).powi(2);
                    }
                    acc += col.sqrt();
                }
            }
            2 => {
                for r in 0..d.rows {
                    for j in 0..m {
                        let (x, y) = (d.at(r, i0 + j), s.at(r, j));
                        acc += (x.0 - y.0).powi(2) + (x.1 - y.1).powi(2);
                    }
                }
            }
            _ => {
                for r in 0..d.rows {
                    for j in 0..m {
                        let (x, y) = (d.at(r, i0 + j).0, s.at(r, j).0);
                        acc += (x - y) * (x / y).ln();
                    }
                }
            }
        }
        *di = acc;
    }
    // local minima (plateaus count once, at their first index; edges have no neighbour)
    let mut minima: Vec<usize> = vec![];
    let mut i0 = 0;
    while i0 < nd {
        let mut j = i0;
        while j + 1 < nd && dist[j + 1] == dist[i0] {
            j += 1;
        }
        let left_ok = i0 == 0 || dist[i0 - 1] > dist[i0];
        let right_ok = j + 1 >= nd || dist[j + 1] > dist[i0];
        if left_ok && right_ok {
            minima.push(i0);
        }
        i0 = j + 1;
    }
    if let Some(md) = maxdist {
        minima.retain(|&k| dist[k] < md);
    }
    minima.sort_by(|&x, &y| dist[x].partial_cmp(&dist[y]).unwrap_or(std::cmp::Ordering::Equal).then(x.cmp(&y)));
    if maxseg.is_finite() {
        minima.truncate(maxseg as usize);
    }
    let starts: Vec<f64> = minima.iter().map(|&k| (k + 1) as f64).collect();
    let stops: Vec<f64> = minima.iter().map(|&k| (k + m) as f64).collect();
    let dists: Vec<f64> = minima.iter().map(|&k| dist[k]).collect();
    let mut outs = vec![shape(starts), shape(stops)];
    let mut dv = shape(dists);
    if single {
        if let Value::Num(mm) = &mut dv {
            mm.data.iter_mut().for_each(|e| *e = *e as f32 as f64);
            mm.kind = Kind::Single;
        }
    }
    outs.push(dv);
    outs.truncate(nargout.max(1));
    out(outs)
}
