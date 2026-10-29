//! Error-tolerant syntax highlighting for code blocks being edited.

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{Color32, FontFamily, FontId};

use crate::theme::Skin;

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
    let b = src.as_bytes();
    let mut i = 0;
    let mut prev_value = false;
    let mut line_start = true;
    let mut section_line = false;
    while i < b.len() {
        let c = b[i];
        let start = i;
        let color;
        if line_start && src[start..].starts_with("%%") {
            section_line = true;
        }
        if c == b'%' {
            // Comment to end of line; `%%` at line start is a section title.
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            color = if line_start && src[start..].starts_with("%%") { skin.syn_section } else { skin.syn_comment };
            prev_value = false;
        } else if c == b'"' || (c == b'\'' && !prev_value) {
            i += 1;
            while i < b.len() && b[i] != b'\n' {
                if b[i] == c {
                    if i + 1 < b.len() && b[i + 1] == c {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            color = skin.syn_string;
            prev_value = true;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &src[start..i];
            if livre_lang::KEYWORDS.contains(&word) {
                color = skin.syn_keyword;
                prev_value = word == "end";
            } else {
                color = skin.syn_text;
                prev_value = true;
            }
        } else if c.is_ascii_digit() || (c == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.') {
                i += 1;
            }
            color = skin.syn_number;
            prev_value = true;
        } else {
            // Run of other characters; UTF-8 safe advance.
            let ch = src[i..].chars().next().unwrap();
            i += ch.len_utf8();
            color = skin.syn_text;
            prev_value = matches!(c, b')' | b']' | b'}' | b'\'');
        }
        line_start = c == b'\n' || (line_start && (c == b' ' || c == b'\t'));
        job.append(&src[start..i], 0.0, fmt(color, section_line));
        if c == b'\n' {
            prev_value = false;
            section_line = false;
        }
    }
    if src.is_empty() {
        job.append("", 0.0, fmt(skin.syn_text, false));
    }
    job
}
