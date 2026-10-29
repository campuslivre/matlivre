//! Error-tolerant syntax highlighting for code blocks being edited.

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{Color32, FontId};

use crate::theme::Skin;

pub fn highlight(src: &str, skin: &Skin, font: FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    let fmt = |c: Color32| TextFormat { font_id: font.clone(), color: c, ..Default::default() };
    let b = src.as_bytes();
    let mut i = 0;
    let mut prev_value = false;
    let mut line_start = true;
    while i < b.len() {
        let c = b[i];
        let start = i;
        let color;
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
                color = skin.text;
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
            color = skin.text;
            prev_value = matches!(c, b')' | b']' | b'}' | b'\'');
        }
        line_start = c == b'\n' || (line_start && (c == b' ' || c == b'\t'));
        if c == b'\n' {
            prev_value = false;
        }
        job.append(&src[start..i], 0.0, fmt(color));
    }
    if src.is_empty() {
        job.append("", 0.0, fmt(skin.text));
    }
    job
}
