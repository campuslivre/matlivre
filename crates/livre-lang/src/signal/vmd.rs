//! `vmd`: variational mode decomposition by ADMM on the one-sided spectrum of the signal mirrored
//! by half its length on each side (Dragomiretskiy and Zosso, 2014).

use crate::builtins::Ret;
use crate::error::rt;
use crate::interp::Interpreter;
use crate::signal::poly::*;
use crate::value::*;

pub const NAMES: &[&str] = &["vmd"];

fn opt_matrix(v: &Value) -> Matrix {
    v.to_matrix().unwrap_or_else(|_| Matrix::from_dims(vec![0, 0], vec![]))
}

pub fn call(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    if a.is_empty() {
        return rt("Not enough input arguments.");
    }
    if nargout > 3 {
        return rt("Too many output arguments.");
    }
    let x = match &a[0] {
        Value::Num(m) if matches!(m.kind, Kind::Double | Kind::Single) => m.clone(),
        Value::Num(m) if m.kind == Kind::Char || m.kind == Kind::Logical => return rt("Expected X to be one of these types:  single, double, timetable"),
        Value::Num(_) => return rt(format!("Expected X to be one of these types:  single, double, timetable  Instead its type was {}.", a[0].class_name())),
        _ => return rt("Unsupported data conversion from 'double' to 'cell'. Second argument must be the name of a data type or a class that supports CAST, such as 'double' or 'single'."),
    };
    if !(x.dims.len() == 2 && (x.dims[0] == 1 || x.dims[1] == 1)) {
        return rt("Expected X to be a vector.");
    }
    if x.im.is_some() {
        return rt("Expected X to be real.");
    }
    let n = x.numel();
    let mut k_imf = 5.0;
    let mut abs_tol = 5e-6;
    let mut rel_tol: Option<f64> = None;
    let mut max_it = 500.0;
    let mut alpha = 1000.0;
    let mut tau = 0.01;
    let mut method: Option<String> = None;
    let mut cf: Option<Matrix> = None;
    let mut init_imfs: Option<Matrix> = None;
    let mut init_lm: Option<Matrix> = None;
    let mut i = 1;
    while i < a.len() {
        let s = a[i].text().unwrap_or_default();
        let low = s.to_lowercase();
        let known = ["numimfs", "absolutetolerance", "relativetolerance", "maxiterations", "penaltyfactor", "lmupdaterate", "initializemethod", "centralfrequencies", "initialimfs", "initiallm", "display"];
        if !known.contains(&low.as_str()) {
            if i + 1 >= a.len() {
                return rt("Incorrect number of input arguments. Each parameter name must be followed by a corresponding value.");
            }
            return rt(format!("'{s}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function."));
        }
        let Some(v) = a.get(i + 1) else { return rt("Incorrect number of input arguments. Each parameter name must be followed by a corresponding value.") };
        let num = |name: &str| -> R<f64> {
            let m = opt_matrix(v);
            if m.numel() != 1 {
                return rt(format!("Expected {name} to be a scalar."));
            }
            Ok(m.data[0])
        };
        match low.as_str() {
            "numimfs" => {
                k_imf = num("NumIMFs")?;
                if !(k_imf > 0.0) {
                    return rt("Expected NumIMFs to be positive.");
                }
                if k_imf.fract() != 0.0 {
                    return rt("Expected NumIMFs to be integer-valued.");
                }
            }
            "absolutetolerance" => {
                abs_tol = num("AbsoluteTolerance")?;
                if !(abs_tol > 0.0) {
                    return rt("Expected AbsoluteTolerance to be positive.");
                }
            }
            "relativetolerance" => {
                let r = num("RelativeTolerance")?;
                if !(r > 0.0) {
                    return rt("Expected RelativeTolerance to be positive.");
                }
                rel_tol = Some(r);
            }
            "maxiterations" => {
                max_it = num("MaxIterations")?;
                if !(max_it > 0.0) {
                    return rt("Expected MaxIterations to be positive.");
                }
                if max_it.fract() != 0.0 {
                    return rt("Expected MaxIterations to be integer-valued.");
                }
            }
            "penaltyfactor" => {
                alpha = num("PenaltyFactor")?;
                if !(alpha > 0.0) {
                    return rt("Expected PenaltyFactor to be positive.");
                }
            }
            "lmupdaterate" => {
                tau = num("LMUpdateRate")?;
                if tau < 0.0 {
                    return rt("Expected LMUpdateRate to be nonnegative.");
                }
            }
            "initializemethod" => {
                let t = v.text().unwrap_or_default();
                match t.to_lowercase().as_str() {
                    "random" | "grid" | "peaks" => method = Some(t.to_lowercase()),
                    _ => return rt(format!("Expected InitializeMethod to match one of these values:  'random', 'grid', 'peaks'  The input, '{t}', did not match any of the valid values.")),
                }
            }
            "centralfrequencies" => cf = Some(opt_matrix(v)),
            "initialimfs" => init_imfs = Some(opt_matrix(v)),
            "initiallm" => init_lm = Some(opt_matrix(v)),
            _ => {}
        }
        i += 2;
    }
    if n < 2 {
        return rt("Input signal must have at least two samples.");
    }
    let k = k_imf as usize;
    if cf.is_some() && method.is_some() {
        return rt("Specify only one of 'CentralFrequencies' or 'InitializeMethod'.");
    }
    if let Some(c) = &cf {
        if c.numel() != k {
            return rt(format!("Central frequencies vector length must equal the number of IMFs, {k}."));
        }
        if c.data.iter().any(|v| *v > 0.5) {
            return rt("Expected CentralFrequencies to be an array with all of the values <= 0.5.");
        }
        if c.data.iter().any(|v| *v < 0.0) {
            return rt("Expected CentralFrequencies to be nonnegative.");
        }
    }
    let t_len = 2 * n;
    let nh = t_len / 2 + 1;
    if let Some(m) = &init_imfs {
        if m.dims[0] != n {
            return rt(format!("The number of rows in initial IMFs must equal the input signal length, {n}."));
        }
        if m.data.iter().any(|v| *v != 0.0) {
            return rt("vmd: nonzero InitialIMFs are not implemented.");
        }
    }
    if let Some(m) = &init_lm {
        if m.dims[0] != 1 && m.dims[1] != 1 {
            return rt("Expected InitialLM to be a vector.");
        }
        if m.numel() != nh {
            return rt(format!("The initial Lagrange multiplier length must equal {nh}. See documentation."));
        }
    }
    let rel = rel_tol.unwrap_or(abs_tol * 1e3);
    // Mirror-extended signal and its one-sided spectrum.
    let h = n / 2;
    let mut xe: Vec<f64> = x.data[..h].iter().rev().cloned().collect();
    xe.extend_from_slice(&x.data);
    xe.extend(x.data[h..].iter().rev());
    let spec = it.call_function("fft", vec![real_col(xe)], 1)?.remove(0).to_matrix()?;
    let sim = spec.im.clone().unwrap_or_else(|| vec![0.0; t_len]);
    let xp: Vec<(f64, f64)> = (0..nh).map(|j| (spec.data[j], sim[j])).collect();
    let freq: Vec<f64> = (0..nh).map(|j| j as f64 / t_len as f64).collect();
    // Initial central frequencies.
    let mut w: Vec<f64> = match (&cf, method.as_deref()) {
        (Some(c), _) => c.data.clone(),
        (None, Some("grid")) => {
            if k == 1 {
                vec![0.0]
            } else {
                (0..k).map(|j| 0.5 * j as f64 / (k as f64 - 1.0)).collect()
            }
        }
        (None, Some("random")) => return rt("vmd: InitializeMethod 'random' is not implemented."),
        _ => {
            // The k largest peaks of the magnitude spectrum of the signal, highest frequency first.
            let ox = it.call_function("fft", vec![real_col(x.data.clone())], 1)?.remove(0).to_matrix()?;
            let oi = ox.im.clone().unwrap_or_else(|| vec![0.0; n]);
            let mag: Vec<f64> = (0..n / 2 + 1).map(|j| ox.data[j].hypot(oi[j])).collect();
            let mut pk: Vec<usize> = (1..mag.len().saturating_sub(1)).filter(|&j| mag[j] > mag[j - 1] && mag[j] > mag[j + 1]).collect();
            pk.sort_by(|&p, &q| mag[q].partial_cmp(&mag[p]).unwrap_or(std::cmp::Ordering::Equal));
            pk.truncate(k);
            let mut f: Vec<f64> = pk.iter().map(|&j| j as f64 / n as f64).collect();
            f.sort_by(|p, q| q.partial_cmp(p).unwrap_or(std::cmp::Ordering::Equal));
            if f.len() < k {
                return rt("vmd: fewer spectral peaks than IMFs (random initial frequencies are not implemented).");
            }
            f
        }
    };
    let mut u = vec![vec![(0.0, 0.0); nh]; k];
    let mut lam: Vec<(f64, f64)> = match &init_lm {
        Some(m) => {
            let im = m.im.clone().unwrap_or_else(|| vec![0.0; nh]);
            (0..nh).map(|j| (m.data[j], im[j])).collect()
        }
        None => vec![(0.0, 0.0); nh],
    };
    let mut iters = 0usize;
    let (mut abs_imp, mut rel_imp) = (f64::INFINITY, f64::INFINITY);
    let mut converged = false;
    for it_n in 1..=(max_it as usize) {
        iters = it_n;
        let old = u.clone();
        for m in 0..k {
            let mut sum_p = 0.0;
            let mut fsum = 0.0;
            for j in 0..nh {
                let mut so = (0.0, 0.0);
                for q in 0..k {
                    if q != m {
                        so.0 += u[q][j].0;
                        so.1 += u[q][j].1;
                    }
                }
                let den = 1.0 + alpha * (freq[j] - w[m]) * (freq[j] - w[m]);
                let v = ((xp[j].0 - so.0 + lam[j].0 / 2.0) / den, (xp[j].1 - so.1 + lam[j].1 / 2.0) / den);
                u[m][j] = v;
                let p = v.0 * v.0 + v.1 * v.1;
                sum_p += p;
                fsum += freq[j] * p;
            }
            w[m] = fsum / sum_p;
        }
        for j in 0..nh {
            let mut s = (0.0, 0.0);
            for m in 0..k {
                s.0 += u[m][j].0;
                s.1 += u[m][j].1;
            }
            lam[j].0 += tau * (xp[j].0 - s.0);
            lam[j].1 += tau * (xp[j].1 - s.1);
        }
        let mut total = 0.0;
        let mut relsum = 0.0;
        for m in 0..k {
            let num: f64 = (0..nh).map(|j| (u[m][j].0 - old[m][j].0).powi(2) + (u[m][j].1 - old[m][j].1).powi(2)).sum();
            let den: f64 = (0..nh).map(|j| old[m][j].0.powi(2) + old[m][j].1.powi(2)).sum();
            total += num;
            relsum += num / den;
        }
        abs_imp = total / nh as f64;
        rel_imp = relsum;
        if abs_imp < abs_tol && rel_imp < rel {
            converged = true;
            break;
        }
    }
    // Modes back in time, ordered by decreasing central frequency.
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|&p, &q| w[q].partial_cmp(&w[p]).unwrap_or(std::cmp::Ordering::Equal));
    let mut imf = vec![0.0; n * k];
    for (col, &m) in order.iter().enumerate() {
        // Hermitian completion of the one-sided spectrum (DC and Nyquist appear once).
        let mut re = vec![0.0; t_len];
        let mut im = vec![0.0; t_len];
        for j in 0..nh {
            re[j] = u[m][j].0;
            im[j] = u[m][j].1;
            if j > 0 && j < nh - 1 {
                re[t_len - j] = u[m][j].0;
                im[t_len - j] = -u[m][j].1;
            }
        }
        let full = Matrix::complex(vec![t_len, 1], re, im);
        let td = it.call_function("ifft", vec![Value::Num(full)], 1)?.remove(0).to_matrix()?;
        for s in 0..n {
            imf[col * n + s] = td.data[h + s];
        }
    }
    let mut resid = x.data.clone();
    for col in 0..k {
        for s in 0..n {
            resid[s] -= imf[col * n + s];
        }
    }
    let single = x.kind == Kind::Single;
    let mut imf_m = Matrix::from_dims(vec![n, k], imf);
    let mut res_m = Matrix::from_dims(vec![n, 1], resid);
    if single {
        imf_m.kind = Kind::Single;
        res_m.kind = Kind::Single;
    }
    let cfs: Vec<f64> = order.iter().map(|&m| w[m]).collect();
    let info = StructArray {
        dims: vec![1, 1],
        fields: ["ExitFlag", "CentralFrequencies", "NumIterations", "AbsoluteImprovement", "RelativeImprovement", "LagrangeMultiplier"].iter().map(|s| s.to_string()).collect(),
        elems: vec![vec![
            scalar_value(if converged { 1.0 } else { 0.0 }),
            real_col(cfs),
            scalar_value(iters as f64),
            scalar_value(abs_imp),
            scalar_value(rel_imp),
            Value::Num(Matrix::complex(vec![nh, 1], lam.iter().map(|c| c.0).collect(), lam.iter().map(|c| c.1).collect())),
        ]],
    };
    out(vec![Value::Num(imf_m), Value::Num(res_m), Value::Struct(info)].into_iter().take(nargout.max(1)).collect())
}
