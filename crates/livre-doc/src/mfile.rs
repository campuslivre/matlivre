//! Plain-text `.m` scripts: `%%` starts a section, its title becomes a heading, and the
//! comment lines directly below it become a text block (spec section 2).

use crate::{BlockKind, Document};

pub fn read_m(src: &str) -> Document {
    let mut doc = Document::new();
    let mut code: Vec<&str> = Vec::new();
    let mut lines = src.lines().peekable();
    let flush = |doc: &mut Document, code: &mut Vec<&str>| {
        while code.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
            code.pop();
        }
        while code.first().map(|l| l.trim().is_empty()).unwrap_or(false) {
            code.remove(0);
        }
        if !code.is_empty() {
            doc.push(BlockKind::Code, code.join("\n"));
        }
        code.clear();
    };
    while let Some(line) = lines.next() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("%%") {
            if rest.starts_with('{') || rest.starts_with('}') {
                code.push(line);
                continue;
            }
            flush(&mut doc, &mut code);
            if !doc.blocks.is_empty() {
                doc.push(BlockKind::SectionBreak, "");
            }
            let title = rest.trim();
            if !title.is_empty() {
                let kind = if doc.blocks.is_empty() { BlockKind::Title } else { BlockKind::Heading };
                doc.push(kind, title);
            }
            // Leading comment lines describe the section.
            let mut text: Vec<String> = Vec::new();
            while let Some(next) = lines.peek() {
                let nt = next.trim_start();
                if nt.starts_with('%') && !nt.starts_with("%%") && !nt.starts_with("%{") {
                    text.push(nt[1..].strip_prefix(' ').unwrap_or(&nt[1..]).to_string());
                    lines.next();
                } else {
                    break;
                }
            }
            if !text.is_empty() {
                doc.push(BlockKind::Text, text.join("\n"));
            }
        } else {
            code.push(line);
        }
    }
    flush(&mut doc, &mut code);
    if doc.blocks.is_empty() {
        doc.push(BlockKind::Code, "");
    }
    doc
}

pub fn write_m(doc: &Document) -> String {
    let mut out = String::new();
    let mut pending_break = false;
    for (i, b) in doc.blocks.iter().enumerate() {
        match b.kind {
            BlockKind::SectionBreak => {
                pending_break = true;
                continue;
            }
            BlockKind::Title | BlockKind::Heading | BlockKind::Heading2 => {
                if i > 0 && !out.ends_with("\n\n") {
                    out.push('\n');
                }
                out.push_str(&format!("%% {}\n", b.text.replace('\n', " ")));
            }
            BlockKind::Text => {
                if pending_break {
                    out.push_str("%%\n");
                }
                for l in b.text.lines() {
                    out.push_str(&format!("% {l}\n"));
                }
            }
            BlockKind::Code => {
                if pending_break {
                    out.push_str("\n%%\n");
                }
                out.push_str(&b.text);
                if !b.text.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
        pending_break = false;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_round_trip() {
        let src = "%% Intro\n% Some text\nx = 1;\n\n%% Plot\ny = x + 1;\n";
        let d = read_m(src);
        let kinds: Vec<BlockKind> = d.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            vec![BlockKind::Title, BlockKind::Text, BlockKind::Code, BlockKind::SectionBreak, BlockKind::Heading, BlockKind::Code]
        );
        let again = read_m(&write_m(&d));
        let k2: Vec<BlockKind> = again.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(kinds, k2);
    }
}
