//! The editor's completion list: names that start with the word before the caret, from the
//! workspace, the document, the current folder, the keywords and the built-in functions.
//! It needs no network and answers at once (the Claude suggestions are in `complete.rs`).

use crate::highlight::{spans, Kind};
use crate::i18n::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ItemKind {
    Variable,
    Field,
    LocalFunction,
    File,
    Keyword,
    Function,
}

impl ItemKind {
    pub fn label(self) -> &'static str {
        match self {
            ItemKind::Variable => tr("variable"),
            ItemKind::Field => tr("field"),
            ItemKind::LocalFunction => tr("local function"),
            ItemKind::File => tr("file"),
            ItemKind::Keyword => tr("keyword"),
            ItemKind::Function => tr("function"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub text: String,
    pub kind: ItemKind,
}

/// The list on show in a code block: it replaces characters `start..at` of the block.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggest {
    pub block: u64,
    pub start: usize,
    pub at: usize,
    pub items: Vec<Item>,
    pub sel: usize,
    /// Take item `.0` on the next frame (a click, or Tab with a single match).
    pub accept: Option<usize>,
}

/// The word being typed before the caret: its first character (as a char index into
/// `before`), the word, and the expression before a `.` it follows (`s` in `s.na`).
/// `None` inside a comment or string, or in a number.
pub fn word_before(before: &str) -> Option<(usize, String, Option<String>)> {
    let line_start = before.rfind('\n').map_or(0, |p| p + 1);
    let line = &before[line_start..];
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let wstart = line.char_indices().rev().take_while(|(_, c)| ident(*c)).last().map_or(line.len(), |(i, _)| i);
    let word = &line[wstart..];
    if word.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    // Inside a comment, a string or a command's arguments nothing is offered.
    if let Some((_, k)) = spans(line).into_iter().find(|(r, _)| r.start < line.len().max(1) && r.end >= line.len() && !r.is_empty()) {
        if !matches!(k, Kind::Text | Kind::Keyword) && !(word.is_empty() && k == Kind::Number) {
            return None;
        }
    }
    let head = &line[..wstart];
    let owner = head.strip_suffix('.').and_then(|h| {
        // `a.b.` and `s(2).`: the whole chain names the owner; a number (`1.`) does not.
        let o = h.char_indices().rev().take_while(|(_, c)| ident(*c) || *c == '.').last().map_or(h.len(), |(i, _)| i);
        let o = &h[o..];
        (!o.is_empty() && o.starts_with(|c: char| c.is_ascii_alphabetic())).then(|| o.to_string())
    });
    let start = before[..line_start + wstart].chars().count();
    Some((start, word.to_string(), owner))
}

/// Names a document defines: assigned variables, loop variables, function names and their
/// arguments, and the fields of `owner` (`s.name = ...`).
fn document_names(doc: &str) -> (Vec<String>, Vec<String>) {
    let mut vars = Vec::new();
    let mut funcs = Vec::new();
    for line in doc.lines() {
        let code = line.split('%').next().unwrap_or("");
        let t = code.trim_start();
        if let Some(rest) = t.strip_prefix("function ").or_else(|| t.strip_prefix("function\t")) {
            // function [a, b] = name(x, y)
            let (outs, sig) = match rest.split_once('=') {
                Some((o, s)) => (o, s),
                None => ("", rest),
            };
            let (name, args) = sig.split_once('(').unwrap_or((sig, ""));
            let name = name.trim();
            if !name.is_empty() {
                funcs.push(name.to_string());
            }
            for w in outs.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).chain(args.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))) {
                if w.starts_with(|c: char| c.is_ascii_alphabetic()) {
                    vars.push(w.to_string());
                }
            }
            continue;
        }
        // Statements end at `;` and `,` outside brackets.
        let mut stmts = Vec::new();
        let (mut depth, mut from) = (0i32, 0);
        for (k, c) in code.char_indices() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ';' | ',' if depth <= 0 => {
                    stmts.push(&code[from..k]);
                    from = k + 1;
                }
                _ => {}
            }
        }
        stmts.push(&code[from..]);
        for stmt in stmts {
            let s = stmt.trim_start();
            let s = s.strip_prefix("for ").or_else(|| s.strip_prefix("parfor ")).unwrap_or(s).trim_start();
            // `[a, b] = f(x)` and `x = ...`
            let Some(eq) = s.find('=') else { continue };
            if s[eq + 1..].starts_with('=') || (eq > 0 && matches!(s.as_bytes()[eq - 1], b'<' | b'>' | b'~' | b'=')) {
                continue;
            }
            let lhs = s[..eq].trim();
            let lhs = lhs.strip_prefix('[').and_then(|l| l.strip_suffix(']')).unwrap_or(lhs);
            for part in lhs.split([',', ' ']) {
                let name: String = part.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                if name.starts_with(|c: char| c.is_ascii_alphabetic()) {
                    vars.push(name);
                }
            }
        }
    }
    (vars, funcs)
}

/// The fields `owner.field` names in the document.
fn fields_of(doc: &str, owner: &str) -> Vec<String> {
    let pat = format!("{owner}.");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(k) = doc[from..].find(&pat) {
        let at = from + k;
        from = at + pat.len();
        // The owner must be a whole name (`xs.` is not `s.`).
        if doc[..at].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
            continue;
        }
        let f: String = doc[from..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if f.starts_with(|c: char| c.is_ascii_alphabetic()) {
            out.push(f);
        }
    }
    out
}

/// What may complete `word`, best first. `doc` is the document's code without the word being
/// typed, `vars` the workspace and `files` the functions and scripts of the current folder.
pub fn candidates(word: &str, owner: Option<&str>, doc: &str, vars: &[String], files: &[String]) -> Vec<Item> {
    let mut pool: Vec<Item> = Vec::new();
    let mut add = |names: &mut dyn Iterator<Item = String>, kind: ItemKind| {
        for text in names {
            pool.push(Item { text, kind });
        }
    };
    if let Some(o) = owner {
        add(&mut fields_of(doc, o).into_iter(), ItemKind::Field);
    } else {
        let (doc_vars, doc_funcs) = document_names(doc);
        add(&mut vars.iter().cloned(), ItemKind::Variable);
        add(&mut doc_vars.into_iter(), ItemKind::Variable);
        add(&mut doc_funcs.into_iter(), ItemKind::LocalFunction);
        add(&mut files.iter().cloned(), ItemKind::File);
        add(&mut livre_lang::KEYWORDS.iter().map(|s| s.to_string()), ItemKind::Keyword);
        add(&mut livre_lang::builtins::function_names().into_iter().map(String::from), ItemKind::Function);
    }
    let lower = word.to_ascii_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Item> = pool.into_iter().filter(|i| i.text.to_ascii_lowercase().starts_with(&lower) && seen.insert(i.text.clone())).collect();
    // Names with the word's own case first, then by kind and length.
    out.sort_by(|a, b| {
        let ca = !a.text.starts_with(word);
        let cb = !b.text.starts_with(word);
        (ca, a.kind, a.text.len(), &a.text).cmp(&(cb, b.kind, b.text.len(), &b.text))
    });
    out.truncate(200);
    out
}

/// The `.m` files of `dir`, by name.
pub fn folder_files(dir: Option<&std::path::Path>) -> Vec<String> {
    let Some(dir) = dir else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            let ext = p.extension()?.to_str()?.to_ascii_lowercase();
            (ext == "m" || ext == "mlx").then(|| p.file_stem()?.to_str().map(String::from)).flatten()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[Item]) -> Vec<&str> {
        items.iter().map(|i| i.text.as_str()).collect()
    }

    #[test]
    fn the_word_before_the_caret() {
        assert_eq!(word_before("x = lin"), Some((4, "lin".into(), None)));
        assert_eq!(word_before("a = 1;\nresult.va"), Some((14, "va".into(), Some("result".into()))));
        assert_eq!(word_before("s.a.b"), Some((4, "b".into(), Some("s.a".into()))));
        assert_eq!(word_before("y = 1.5"), None);
        assert_eq!(word_before("x = 3e"), None);
        assert_eq!(word_before("% plot"), None);
        assert_eq!(word_before("disp('hel"), None);
        assert_eq!(word_before("hold o"), None);
    }

    #[test]
    fn names_from_the_document_and_workspace() {
        let doc = "data = rand(3);\n[total, count] = deal(1, 2);\nfor idx = 1:3, end\nfunction out = helper(arg1)\nend";
        let c = candidates("", None, doc, &["width".into()], &["myscript".into()]);
        let kinds: Vec<(&str, ItemKind)> = c.iter().take(8).map(|i| (i.text.as_str(), i.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                ("idx", ItemKind::Variable),
                ("out", ItemKind::Variable),
                ("arg1", ItemKind::Variable),
                ("data", ItemKind::Variable),
                ("count", ItemKind::Variable),
                ("total", ItemKind::Variable),
                ("width", ItemKind::Variable),
                ("helper", ItemKind::LocalFunction)
            ]
        );
        assert_eq!(names(&candidates("he", None, doc, &[], &[]))[0], "helper");
        assert!(names(&candidates("lins", None, "", &[], &[])).contains(&"linspace"));
        // Matching ignores case, but the word's own case ranks first.
        let c = candidates("Da", None, "Data = 1; data = 2;", &[], &[]);
        assert_eq!(names(&c)[..2], ["Data", "data"]);
        // `==` is not an assignment.
        assert!(!names(&candidates("q", None, "if q == 1, end", &[], &[])).contains(&"q"));
    }

    #[test]
    fn fields_after_a_dot() {
        let doc = "s.name = 'a';\ns.size = 2;\nxs.other = 1;\nt.s.inner = 3;";
        assert_eq!(names(&candidates("", Some("s"), doc, &[], &[])), ["name", "size"]);
        assert_eq!(names(&candidates("n", Some("s"), doc, &[], &[])), ["name"]);
        assert_eq!(names(&candidates("", Some("t.s"), doc, &[], &[])), ["inner"]);
    }
}
