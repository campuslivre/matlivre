//! Headless runner: `matlivre-cli script.m` runs a script in a fresh workspace and prints its
//! output (like `matlab -batch` / `octave-cli`). The script's folder is on the search path.

use livre_lang::{Interpreter, OutputKind};

fn run(path: std::path::PathBuf, src: String) -> i32 {
    let mut it = Interpreter::new();
    if let Some(dir) = path.canonicalize().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        it.set_search_path(vec![dir]);
    }
    it.set_output_sink(|outs| {
        use std::io::Write;
        let mut so = std::io::stdout().lock();
        for o in outs {
            match o.kind {
                OutputKind::Text(t) => {
                    let _ = write!(so, "{t}");
                }
                OutputKind::Value { text, .. } => {
                    let _ = writeln!(so, "{text}");
                }
                OutputKind::Warning(w) => eprintln!("Warning: {w}"),
                OutputKind::Error(e) => eprintln!("Error (line {}): {e}", o.line),
                OutputKind::Figure(_) => {}
            }
        }
        let _ = so.flush();
    });
    if it.run_source(&src).is_ok() { 0 } else { 1 }
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: matlivre-cli <script.m>");
        std::process::exit(2);
    };
    let path = std::path::PathBuf::from(path);
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {e}", path.display());
            std::process::exit(2);
        }
    };
    // The interpreter lives on a large-stack thread (deep recursion in user code).
    let code = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run(path, src))
        .expect("thread")
        .join()
        .unwrap_or(1);
    std::process::exit(code);
}
