//! Signal Processing Toolbox functions against MATLAB R2023b with Signal Processing Toolbox 23.2.
//!
//! `data/signal_cases.m` runs about 900 calls (valid use and error cases) and prints every result
//! with `%.17g`; `data/signal_matlab.txt` is MATLAB's output for it. The same script runs here and
//! the two outputs are compared: classes, sizes and error messages exactly, numbers to a relative
//! 1e-9 (1e-6 for single precision). `data/signal_known_deviations.tsv` lists the calls where
//! MatLivre still differs and why; the test fails if one of them starts to match, so the list only
//! shrinks.

use livre_lang::{Interpreter, OutputKind};
use std::collections::{BTreeMap, HashMap};

const SCRIPT: &str = include_str!("data/signal_cases.m");
const MATLAB: &str = include_str!("data/signal_matlab.txt");
const KNOWN: &str = include_str!("data/signal_known_deviations.tsv");

#[derive(Debug, Clone)]
enum Item {
    Error(String),
    Value { class: String, size: Vec<usize>, re: Vec<f64>, im: Vec<f64> },
    Other,
}

fn number(s: &str) -> f64 {
    match s.to_ascii_lowercase().as_str() {
        "nan" | "-nan" => f64::NAN,
        "inf" | "+inf" => f64::INFINITY,
        "-inf" => f64::NEG_INFINITY,
        other => other.parse().unwrap_or_else(|_| panic!("bad number {s:?}")),
    }
}

/// Result lines of a printed run, grouped by case number.
fn parse(text: &str) -> BTreeMap<usize, Vec<(String, Item)>> {
    let mut cases: BTreeMap<usize, Vec<(String, Item)>> = BTreeMap::new();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once('|') else { continue };
        let case = match key.split('.').next().and_then(|c| c.parse::<usize>().ok()) {
            Some(c) if key.chars().all(|ch| ch.is_ascii_digit() || ch == '.') => c,
            _ => continue,
        };
        let item = if let Some(msg) = rest.strip_prefix("ERR|") {
            Item::Error(msg.to_string())
        } else {
            let parts: Vec<&str> = rest.split('|').collect();
            if parts.len() >= 4 {
                Item::Value {
                    class: parts[0].to_string(),
                    size: parts[1].split_whitespace().map(|v| v.parse().unwrap()).collect(),
                    re: parts[2].split_whitespace().map(number).collect(),
                    im: parts[3].split_whitespace().map(number).collect(),
                }
            } else {
                Item::Other
            }
        };
        cases.entry(case).or_default().push((key.to_string(), item));
    }
    cases
}

/// The text of every case in the script, in order.
fn case_texts() -> Vec<String> {
    let start = SCRIPT.find("cases = {").expect("cases list");
    let end = SCRIPT[start..].find("\n};").expect("end of cases") + start;
    SCRIPT[start..end]
        .lines()
        .skip(1)
        .filter(|l| l.starts_with('\''))
        .map(|l| l[1..l.len() - 1].replace("''", "'"))
        .collect()
}

fn close(a: &[f64], b: &[f64], tol: f64) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            (x.is_nan() && y.is_nan()) || x == y || (x - y).abs() <= tol * x.abs().max(y.abs()).max(1.0)
        })
}

/// Why two result lists differ, or `None` when they agree.
fn difference(want: &[(String, Item)], got: &[(String, Item)]) -> Option<String> {
    if want.len() != got.len() {
        return Some(format!("{} outputs from MATLAB, {} here", want.len(), got.len()));
    }
    for ((wk, w), (gk, g)) in want.iter().zip(got) {
        if wk != gk {
            return Some(format!("output {wk} against {gk}"));
        }
        match (w, g) {
            (Item::Error(a), Item::Error(b)) => {
                if a != b {
                    return Some(format!("error message differs:\n      MATLAB:    {a}\n      MatLivre: {b}"));
                }
            }
            (
                Item::Value { class: c1, size: s1, re: r1, im: i1 },
                Item::Value { class: c2, size: s2, re: r2, im: i2 },
            ) => {
                if c1 != c2 {
                    return Some(format!("{wk}: class {c1} against {c2}"));
                }
                if s1 != s2 {
                    return Some(format!("{wk}: size {s1:?} against {s2:?}"));
                }
                let tol = if c1 == "single" { 1e-6 } else { 1e-9 };
                if !close(r1, r2, tol) || !close(i1, i2, tol) {
                    let first = |v: &[f64]| v.iter().take(4).map(|x| format!("{x:.6e}")).collect::<Vec<_>>().join(" ");
                    return Some(format!("{wk}: values differ\n      MATLAB:   {}\n      MatLivre: {}", first(r1), first(r2)));
                }
            }
            (Item::Other, Item::Other) => {}
            (a, b) => return Some(format!("{wk}: {a:?} against {b:?}")),
        }
    }
    None
}

/// Cases that take long to compute (high-order `cfirpm` designs). The regular test skips them; the
/// ignored `signal_heavy_cases_match_matlab` runs only them (`cargo test -- --ignored`).
fn is_heavy(text: &str) -> bool {
    text.split_once("cfirpm(")
        .and_then(|(_, rest)| rest.split(',').next()?.trim().parse::<u32>().ok())
        .map_or(false, |n| n >= 16)
}

/// The script with the statements of the cases that are not wanted replaced by a no-op.
fn script_for(heavy: bool) -> String {
    let start = SCRIPT.find("cases = {").expect("cases list");
    let mut out = String::with_capacity(SCRIPT.len());
    out.push_str(&SCRIPT[..start]);
    let rest = &SCRIPT[start..];
    let end = rest.find("\n};").expect("end of cases");
    for (k, line) in rest[..end].lines().enumerate() {
        if k > 0 && line.starts_with('\'') && is_heavy(&line[1..line.len() - 1]) != heavy {
            out.push_str("'r = 0;',\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str(&rest[end + 1..]);
    out
}

#[test]
fn signal_functions_match_matlab() {
    check(false);
}

#[test]
#[ignore = "slow (high-order cfirpm designs); run with --ignored"]
fn signal_heavy_cases_match_matlab() {
    check(true);
}

fn check(heavy: bool) {
    let mut it = Interpreter::new();
    let _ = it.run_source(&script_for(heavy));
    let printed: String = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Text(t) => Some(t),
            OutputKind::Error(e) => Some(format!("Error: {e}\n")),
            _ => None,
        })
        .collect();
    let got = parse(&printed);
    let want = parse(MATLAB);
    let texts = case_texts();

    let known: HashMap<&str, &str> = KNOWN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.split_once('\t'))
        .collect();

    let mut failures = Vec::new();
    let mut resolved = Vec::new();
    let mut matching = 0;
    for (i, text) in texts.iter().enumerate() {
        let case = i + 1;
        if is_heavy(text) != heavy {
            continue;
        }
        let w = want.get(&case).map(Vec::as_slice).unwrap_or(&[]);
        let g = got.get(&case).map(Vec::as_slice).unwrap_or(&[]);
        let diff = difference(w, g);
        match (diff, known.get(text.as_str())) {
            (None, None) => matching += 1,
            (None, Some(_)) => resolved.push(format!("[{case}] {text}")),
            (Some(_), Some(_)) => {}
            (Some(d), None) => failures.push(format!("[{case}] {text}\n      {d}")),
        }
    }
    assert!(heavy || texts.len() > 800, "only {} cases parsed", texts.len());
    assert!(resolved.is_empty(), "these known deviations now match MATLAB; remove them from signal_known_deviations.tsv:\n{}", resolved.join("\n"));
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from MATLAB ({} match):\n{}",
        failures.len(),
        texts.len(),
        matching,
        failures.iter().take(200).cloned().collect::<Vec<_>>().join("\n")
    );
}
