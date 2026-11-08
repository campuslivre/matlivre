//! Error-tolerant syntax highlighting for code blocks being edited, following the MATLAB
//! Editor's rules: keywords, comments (with `%{ ... %}` blocks and the text after a `...`
//! continuation), strings and unterminated strings, command-syntax arguments (`hold on`,
//! `format long`), system commands (`!dir`) and section titles.

use std::ops::Range;

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{Color32, FontFamily, FontId};

use crate::theme::Skin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Keyword,
    Comment,
    Section,
    String,
    /// A string missing its closing quote.
    Unterminated,
    /// The words of a command-syntax statement, which MATLAB passes as char arguments.
    Command,
    /// A `!` system command.
    System,
    Number,
}

/// Words the editor shows as keywords wherever they appear as a word.
fn is_keyword(w: &str) -> bool {
    livre_lang::KEYWORDS.contains(&w) || w == "spmd"
}

/// The trimmed line starting at byte `i`.
fn line_at(src: &str, i: usize) -> &str {
    let end = src[i..].find('\n').map_or(src.len(), |k| i + k);
    src[i..end].trim()
}

/// `name word ...` at a statement start is command syntax when what follows the name is a
/// word rather than an operator, assignment, call or index (`hold on`, `close all`).
fn is_command(rest: &str) -> bool {
    let b = rest.as_bytes();
    if !matches!(b.first(), Some(b' ' | b'\t')) {
        return false;
    }
    let mut k = 0;
    while matches!(b.get(k), Some(b' ' | b'\t')) {
        k += 1;
    }
    let Some(&c) = b.get(k) else { return false };
    if !(c.is_ascii_alphanumeric() || c == b'-' || c == b'_') {
        // `format +`, `cd ..` and `cd /tmp`: a lone sign or a path.
        let next = b.get(k + 1).copied().unwrap_or(b'\n');
        return (c == b'+' && matches!(next, b'\n' | b'\r' | b';' | b',' | b' ')) || (c == b'.' && matches!(next, b'.' | b'/' | b'\\') && !rest[k..].starts_with("...")) || c == b'/';
    }
    if c.is_ascii_digit() || (c == b'-' && !b.get(k + 1).is_some_and(|d| d.is_ascii_alphabetic())) {
        return false;
    }
    let mut j = k;
    while b.get(j).is_some_and(|d| d.is_ascii_alphanumeric() || *d == b'_' || *d == b'-') {
        j += 1;
    }
    while matches!(b.get(j), Some(b' ' | b'\t')) {
        j += 1;
    }
    // `a = b` and `a == b` are expressions; `x (1)` too.
    !matches!(b.get(j), Some(b'=' | b'('))
}

/// The highlighted runs of `src`, covering it from start to end.
pub fn spans(src: &str) -> Vec<(Range<usize>, Kind)> {
    let b = src.as_bytes();
    let mut out: Vec<(Range<usize>, Kind)> = Vec::new();
    let mut push = |r: Range<usize>, k: Kind| {
        if r.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((last, lk)) if *lk == k && last.end == r.start => last.end = r.end,
            _ => out.push((r, k)),
        }
    };
    let mut i = 0;
    // A value just ended (so `'` is a transpose), the line so far is blank, a statement can
    // start here, and how deep the brackets are.
    let mut prev_value = false;
    let mut line_start = true;
    let mut stmt_start = true;
    let mut depth = 0usize;
    let mut after_dot = false;
    while i < b.len() {
        let c = b[i];
        let start = i;
        if c == b'\n' {
            i += 1;
            push(start..i, Kind::Text);
            line_start = true;
            prev_value = false;
            after_dot = false;
            // A newline inside brackets continues the matrix, not the statement.
            stmt_start = depth == 0;
            continue;
        }
        if c == b' ' || c == b'\t' || c == b'\r' {
            i += 1;
            push(start..i, Kind::Text);
            continue;
        }
        // `%{` alone on a line opens a block comment that ends with a line holding `%}`.
        if line_start && line_at(src, i) == "%{" {
            let mut level = 0;
            let mut k = i;
            loop {
                let end = src[k..].find('\n').map_or(src.len(), |e| k + e);
                match src[k..end].trim() {
                    "%{" => level += 1,
                    "%}" => level -= 1,
                    _ => {}
                }
                k = end;
                if level == 0 || k >= src.len() {
                    break;
                }
                k += 1;
            }
            i = k;
            push(start..i, Kind::Comment);
            continue;
        }
        if c == b'%' {
            let end = src[i..].find('\n').map_or(src.len(), |e| i + e);
            let after = b.get(i + 2).copied();
            let section = line_start && src[i..].starts_with("%%") && matches!(after, None | Some(b' ' | b'\t' | b'\r' | b'\n'));
            i = end;
            push(start..i, if section { Kind::Section } else { Kind::Comment });
            continue;
        }
        if src[i..].starts_with("...") {
            // A continuation: the rest of the line is a comment.
            let end = src[i..].find('\n').map_or(src.len(), |e| i + e);
            i = end;
            push(start..i, Kind::Comment);
            // The statement goes on with the next line.
            if i < b.len() {
                i += 1;
                push(end..i, Kind::Text);
            }
            line_start = true;
            continue;
        }
        if c == b'!' && stmt_start && depth == 0 {
            let end = src[i..].find('\n').map_or(src.len(), |e| i + e);
            i = end;
            push(start..i, Kind::System);
            continue;
        }
        line_start = false;
        if c == b'"' || (c == b'\'' && !prev_value) {
            i += 1;
            let mut closed = false;
            while i < b.len() && b[i] != b'\n' {
                if b[i] == c {
                    if b.get(i + 1) == Some(&c) {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    closed = true;
                    break;
                }
                i += src[i..].chars().next().map_or(1, char::len_utf8);
            }
            push(start..i, if closed { Kind::String } else { Kind::Unterminated });
            prev_value = true;
            stmt_start = false;
            after_dot = false;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &src[start..i];
            if after_dot {
                // A field or method name, even one spelled like a keyword.
                push(start..i, Kind::Text);
                prev_value = true;
            } else if is_keyword(word) || (word == "arguments" && stmt_start && depth == 0) {
                push(start..i, Kind::Keyword);
                prev_value = word == "end";
                // `if x`, `for k = 1:3`: the condition follows, not a new statement.
                stmt_start = matches!(word, "else" | "end" | "try" | "otherwise" | "break" | "continue" | "return");
                after_dot = false;
                continue;
            } else if stmt_start && depth == 0 && is_command(&src[i..]) {
                push(start..i, Kind::Text);
                // The arguments run to the end of the statement.
                let mut k = i;
                while k < b.len() && !matches!(b[k], b'\n' | b';' | b',' | b'%') {
                    k += 1;
                }
                let lead = i + (src[i..k].len() - src[i..k].trim_start().len());
                let end_ws = src[i..k].trim_end().len() + i;
                push(i..lead, Kind::Text);
                push(lead..end_ws.max(lead), Kind::Command);
                push(end_ws..k, Kind::Text);
                i = k;
                prev_value = false;
                stmt_start = false;
                continue;
            } else {
                push(start..i, Kind::Text);
                prev_value = true;
            }
            stmt_start = false;
            after_dot = false;
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.') {
                // `1:n.^2` style: stop before an operator that starts with a dot.
                if b[i] == b'.' && b.get(i + 1).is_some_and(|d| matches!(d, b'*' | b'/' | b'\\' | b'^' | b'\'')) {
                    break;
                }
                i += 1;
            }
            push(start..i, Kind::Number);
            prev_value = true;
            stmt_start = false;
            after_dot = false;
            continue;
        }
        // Any other character; UTF-8 safe.
        let ch = src[i..].chars().next().unwrap();
        i += ch.len_utf8();
        push(start..i, Kind::Text);
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        // `.'` is a transpose; a lone `.` starts a field name.
        after_dot = c == b'.' && b.get(i) != Some(&b'\'');
        if c == b'.' && b.get(i) == Some(&b'\'') {
            i += 1;
            push(i - 1..i, Kind::Text);
        }
        prev_value = matches!(c, b')' | b']' | b'}') || (c == b'.' && b.get(i - 1) == Some(&b'\'')) || (c == b'\'' && prev_value);
        stmt_start = depth == 0 && matches!(c, b';' | b',');
    }
    out
}

/// `line_height` sets the row pitch (20 px in the editors); with `bold_sections`, `%%` lines
/// are drawn semibold as section headers (the plain-script view).
pub fn highlight(src: &str, skin: &Skin, font: FontId, line_height: Option<f32>, bold_sections: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    let bold = FontId::new(font.size, FontFamily::Name("mono-bold".into()));
    let fmt = |c: Color32, section: bool| TextFormat {
        font_id: if section && bold_sections { bold.clone() } else { font.clone() },
        color: c,
        line_height,
        ..Default::default()
    };
    // Section lines are bold from start to end, whatever their runs.
    let mut section_lines: Vec<Range<usize>> = Vec::new();
    let runs = spans(src);
    for (r, k) in &runs {
        if *k == Kind::Section {
            let start = src[..r.start].rfind('\n').map_or(0, |p| p + 1);
            section_lines.push(start..r.end);
        }
    }
    for (r, k) in runs {
        let color = match k {
            Kind::Text => skin.syn_text,
            Kind::Keyword => skin.syn_keyword,
            Kind::Comment => skin.syn_comment,
            Kind::Section => skin.syn_section,
            Kind::String | Kind::Command => skin.syn_string,
            Kind::Unterminated => skin.syn_unterminated,
            Kind::System => skin.syn_system,
            Kind::Number => skin.syn_number,
        };
        let bold = section_lines.iter().any(|s| s.start <= r.start && r.end <= s.end);
        job.append(&src[r], 0.0, fmt(color, bold));
    }
    if src.is_empty() {
        job.append("", 0.0, fmt(skin.syn_text, false));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The non-blank runs of `src` that are not plain text, as (text, kind).
    fn marked(src: &str) -> Vec<(&str, Kind)> {
        spans(src).into_iter().filter(|(_, k)| *k != Kind::Text).map(|(r, k)| (&src[r], k)).collect()
    }

    #[test]
    fn keywords_strings_and_transposes() {
        assert_eq!(marked("if x', y = 'a''b'; end"), vec![("if", Kind::Keyword), ("'a''b'", Kind::String), ("end", Kind::Keyword)]);
        assert_eq!(marked("z = a.' + b(end)'"), vec![("end", Kind::Keyword)]);
        assert_eq!(marked("s.end = \"x\""), vec![("\"x\"", Kind::String)]);
        assert_eq!(marked("x = 'abc"), vec![("'abc", Kind::Unterminated)]);
    }

    #[test]
    fn comments_sections_and_continuations() {
        assert_eq!(marked("%% Title\nx = 1 % note"), vec![("%% Title", Kind::Section), ("1", Kind::Number), ("% note", Kind::Comment)]);
        assert_eq!(marked("%%x\n"), vec![("%%x", Kind::Comment)]);
        assert_eq!(marked("a = [1 ... first\n 2];"), vec![("1", Kind::Number), ("... first", Kind::Comment), ("2", Kind::Number)]);
        let src = "%{\nnot code 'x\n%}\ny = 2";
        assert_eq!(marked(src), vec![("%{\nnot code 'x\n%}", Kind::Comment), ("2", Kind::Number)]);
        // `%{` with text after it is an ordinary comment.
        assert_eq!(marked("%{ x\ny"), vec![("%{ x", Kind::Comment)]);
    }

    #[test]
    fn command_syntax_and_system_commands() {
        assert_eq!(marked("hold on"), vec![("on", Kind::Command)]);
        assert_eq!(marked("format long g; x = 1"), vec![("long g", Kind::Command), ("1", Kind::Number)]);
        assert_eq!(marked("cd ..\nclose all % done"), vec![("..", Kind::Command), ("all", Kind::Command), ("% done", Kind::Comment)]);
        assert_eq!(marked("!dir /b"), vec![("!dir /b", Kind::System)]);
        // Expressions are not commands.
        assert_eq!(marked("a = b"), vec![]);
        assert_eq!(marked("a == b"), vec![]);
        assert_eq!(marked("x (1)"), vec![("1", Kind::Number)]);
        assert_eq!(marked("a - b"), vec![]);
        assert_eq!(marked("disp(x); disp x"), vec![("x", Kind::Command)]);
        // Not at a statement start.
        assert_eq!(marked("if a b, end"), vec![("if", Kind::Keyword), ("end", Kind::Keyword)]);
        assert_eq!(marked("[a b]"), vec![]);
    }
}

/// How far text sits below the top of a `row_h` row once centred in it.
pub fn row_offset(font_height: f32, row_h: f32) -> f32 {
    ((row_h - font_height) / 2.0).max(0.0).round()
}

/// Moves the text of each row down to the middle of its `row_h` rows. egui 0.29 puts text
/// at the top of a row taller than the font, so the caret and selection, which span the
/// row, hung below it.
pub fn center_rows(galley: std::sync::Arc<eframe::egui::Galley>, row_h: f32) -> std::sync::Arc<eframe::egui::Galley> {
    let Some(fh) = galley.rows.iter().flat_map(|r| r.glyphs.iter()).map(|g| g.font_height).next() else { return galley };
    let d = row_offset(fh, row_h);
    if d == 0.0 {
        return galley;
    }
    let mut g = (*galley).clone();
    let shift = eframe::egui::vec2(0.0, d);
    for row in &mut g.rows {
        for glyph in &mut row.glyphs {
            glyph.pos.y += d;
        }
        row.visuals.mesh.translate(shift);
        row.visuals.mesh_bounds = row.visuals.mesh_bounds.translate(shift);
    }
    g.mesh_bounds = g.mesh_bounds.translate(shift);
    std::sync::Arc::new(g)
}
