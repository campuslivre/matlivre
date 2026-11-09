//! Differential tests. Every case under `tests/matlab/cases` runs in a fresh interpreter through
//! `tools/difftest/difftest_run.m`; the snapshot it writes (command-window log, error, lastwarn,
//! workspace variables with their display text, figures) is compared field by field with the one
//! MATLAB recorded, kept in `tests/matlab/expected/<release>/`. Record with
//! `tools/difftest/record.sh`.
//!
//! Any difference from MATLAB fails the test. There is no way to mark a case as expected to
//! differ: a case that fails is a behaviour to implement.
//!
//! * The interpreter runs in command-window display style, like `matlab -batch`.
//! * `MATLIVRE_TARGET_RELEASE` picks the release to compare against (default `R2023b`).
//! * A case without a recorded snapshot for that release is listed, not failed.

use livre_lang::{Interpreter, OutputKind};
use serde_json::Value as J;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    PathBuf::from(p.display().to_string().trim_start_matches(r"\\?\"))
}

fn slash(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

fn cases_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            cases_under(&p, out);
        } else if p.extension().is_some_and(|e| e == "m") {
            out.push(p);
        }
    }
}

/// Removes `<a href="...">` and `</a>`: `diary` records MATLAB's hyperlinks as markup, which a
/// command window shows as plain (bold) text.
fn strip_links(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("<a href=") {
        out.push_str(&rest[..i]);
        match rest[i..].find('>') {
            Some(j) => rest = &rest[i + j + 1..],
            None => {
                rest = &rest[i..];
                break;
            }
        }
    }
    out.push_str(rest);
    out.replace("</a>", "")
}

/// Command-window text without what differs between the two programs for reasons that are not
/// behaviour: line endings, hyperlink markup, the bold markers MATLAB wraps warnings in,
/// call-stack lines, and trailing blanks.
fn normalize_text(s: &str) -> String {
    let s = s.replace("\r\n", "\n").replace("[\u{8}", "").replace("]\u{8}", "").replace('\u{8}', "");
    let s = strip_links(&s);
    let lines: Vec<&str> = s
        .split('\n')
        .filter(|l| !(l.starts_with("> In ") || l.starts_with("In ")))
        .map(|l| l.trim_end())
        .collect();
    lines.join("\n")
}

fn normalize(v: &mut J, key: &str) {
    match v {
        J::String(s) if key == "log" || key == "display" => *s = normalize_text(s),
        J::Object(m) => {
            for (k, x) in m.iter_mut() {
                normalize(x, k);
            }
        }
        J::Array(a) => {
            for x in a.iter_mut() {
                normalize(x, key);
            }
        }
        _ => {}
    }
}

fn short(v: &J) -> String {
    let s = v.to_string();
    if s.chars().count() > 140 { format!("{}...", s.chars().take(140).collect::<String>()) } else { s }
}

fn diff(path: &str, ours: &J, matlab: &J, out: &mut Vec<String>) {
    match (ours, matlab) {
        (J::Object(a), J::Object(b)) => {
            for (k, x) in b {
                match a.get(k) {
                    Some(y) => diff(&format!("{path}.{k}"), y, x, out),
                    None => out.push(format!("{path}.{k}: missing in MatLivre (MATLAB {})", short(x))),
                }
            }
            for k in a.keys().filter(|k| !b.contains_key(*k)) {
                out.push(format!("{path}.{k}: only in MatLivre"));
            }
        }
        (J::Array(a), J::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{path}: {} items in MatLivre, {} in MATLAB", a.len(), b.len()));
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                diff(&format!("{path}[{i}]"), x, y, out);
            }
        }
        _ => {
            if ours != matlab {
                out.push(format!("{path}: MatLivre {} / MATLAB {}", short(ours), short(matlab)));
            }
        }
    }
}

fn run_case(root: &Path, case: &Path, out_json: &Path) -> Result<J, String> {
    let _ = std::fs::remove_file(out_json);
    let mut it = Interpreter::new();
    it.command_window = true;
    it.set_search_path(vec![root.join("tools/difftest")]);
    let _ = it.run_source(&format!("difftest_run('{}', '{}')", slash(case), slash(out_json)));
    let errors: Vec<String> = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| if let OutputKind::Error(e) = o.kind { Some(e) } else { None })
        .collect();
    if !errors.is_empty() {
        return Err(format!("the harness failed: {errors:?}"));
    }
    let text = std::fs::read_to_string(out_json).map_err(|e| format!("no snapshot written: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("snapshot is not JSON: {e}"))
}

#[test]
fn cases_match_matlab_snapshots() {
    let root = root();
    let release = std::env::var("MATLIVRE_TARGET_RELEASE").unwrap_or_else(|_| "R2023b".to_string());
    let cases_dir = root.join("tests/matlab/cases");
    let expected_dir = root.join("tests/matlab/expected").join(&release);
    let tmp = std::env::temp_dir().join(format!("matlivre-difftest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let mut cases = Vec::new();
    cases_under(&cases_dir, &mut cases);
    let (mut failed, mut unrecorded) = (Vec::new(), Vec::new());
    let mut passed = 0;
    for case in &cases {
        let rel = slash(case.strip_prefix(&cases_dir).unwrap());
        let rel = rel.trim_end_matches(".m").to_string();
        let expected_file = expected_dir.join(format!("{rel}.json"));
        let Ok(expected_text) = std::fs::read_to_string(&expected_file) else {
            unrecorded.push(rel);
            continue;
        };
        let mut matlab: J = serde_json::from_str(&expected_text).expect("recorded snapshot is JSON");
        let recorded = matlab["header"]["release"].as_str().unwrap_or("?").to_string();
        if format!("R{recorded}") != release {
            failed.push(format!("{rel}: snapshot says MATLAB {recorded} but sits under {release}"));
            continue;
        }
        let mut diffs = Vec::new();
        match run_case(&root, case, &tmp.join("out.json")) {
            Err(e) => diffs.push(e),
            Ok(mut ours) => {
                normalize(&mut ours, "");
                normalize(&mut matlab, "");
                for j in [&mut ours, &mut matlab] {
                    if let Some(o) = j.as_object_mut() {
                        o.remove("header");
                    }
                }
                diff("", &ours, &matlab, &mut diffs);
            }
        }
        if diffs.is_empty() {
            passed += 1;
        } else {
            let more = diffs.len().saturating_sub(12);
            diffs.truncate(12);
            let mut msg = format!("{rel}:\n  {}", diffs.join("\n  "));
            if more > 0 {
                msg.push_str(&format!("\n  ... and {more} more"));
            }
            failed.push(msg);
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    eprintln!("difftest against MATLAB {release}: {passed} match, {} not recorded, {} differ", unrecorded.len(), failed.len());
    if !unrecorded.is_empty() {
        eprintln!("not recorded for {release}: {}", unrecorded.join(", "));
    }
    assert!(failed.is_empty(), "cases differ from MATLAB {release}:\n{}", failed.join("\n"));
}
