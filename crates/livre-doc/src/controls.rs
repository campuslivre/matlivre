//! Live controls: widgets drawn over a literal of a code block, whose current value is that
//! literal (spec section 1.7).

use std::ops::Range;

use crate::{Block, BlockKind, Document};

#[derive(Debug, Clone, PartialEq)]
pub enum ControlKind {
    /// Numeric slider; the literal is the number.
    Slider { min: f64, max: f64, step: f64 },
    /// Drop-down; `items` are the code literals offered (e.g. "'sin'", "'cos'", "3"), `labels` what is shown (same length; default to the items).
    DropDown { items: Vec<String>, labels: Vec<String> },
    /// Check box; the literal is `true` or `false`.
    CheckBox,
    /// Edit field; the literal is whatever the user types (number or quoted text).
    EditField,
    /// Push button that runs its section; replaces nothing (zero-width span).
    Button { label: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveControl {
    pub kind: ControlKind,
    /// 0-based line within the block text and the [start, end) char columns of the literal it stands for.
    pub line: usize,
    pub start: usize,
    pub end: usize,
    /// customXml attributes this crate does not interpret, kept verbatim so a round trip through MatLivre does not lose them.
    /// A control read from `.mlx` also keeps its original `context` JSON here, so unknown fields in it survive.
    pub extra: Vec<(String, String)>,
}

impl LiveControl {
    pub fn is_button(&self) -> bool {
        matches!(self.kind, ControlKind::Button { .. })
    }
}

/// Byte range of the [start, end) char columns of `line` in `text`.
pub(crate) fn span_bytes(text: &str, line: usize, start: usize, end: usize) -> Option<Range<usize>> {
    if start > end {
        return None;
    }
    let ls = if line == 0 { 0 } else { text.match_indices('\n').nth(line - 1)?.0 + 1 };
    let l = &text[ls..text[ls..].find('\n').map_or(text.len(), |i| ls + i)];
    let mut cols = l.char_indices().map(|(i, _)| i).chain(std::iter::once(l.len()));
    let s = cols.nth(start)?;
    let e = if end == start { s } else { cols.nth(end - start - 1)? };
    Some(ls + s..ls + e)
}

/// Characters that would fuse with a literal into one token.
fn glues(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '\'' | '"')
}

impl Block {
    /// The literal a control currently stands for (its span of `text`).
    pub fn control_text(&self, i: usize) -> &str {
        let c = &self.controls[i];
        span_bytes(&self.text, c.line, c.start, c.end).map_or("", |r| &self.text[r])
    }

    /// Replaces control `i`'s literal, updating its span and the spans of controls after it on the same line.
    /// Line breaks are removed from `literal`; an empty literal and buttons are ignored.
    pub fn set_control_value(&mut self, i: usize, literal: &str) {
        let literal: String = literal.chars().filter(|&c| c != '\n' && c != '\r').collect();
        let c = &self.controls[i];
        if c.is_button() || literal.is_empty() {
            return;
        }
        let Some(r) = span_bytes(&self.text, c.line, c.start, c.end) else { return };
        let (line, old_end, old_len, new_len) = (c.line, c.end, c.end - c.start, literal.chars().count());
        self.text.replace_range(r, &literal);
        self.controls[i].end = self.controls[i].start + new_len;
        for (j, o) in self.controls.iter_mut().enumerate() {
            if j != i && o.line == line && o.start >= old_end {
                o.start = o.start - old_len + new_len;
                o.end = o.end - old_len + new_len;
            }
        }
    }

    /// Adds a control after validating it against `text` (code blocks only); see [`Document::add_control`].
    pub fn add_control(&mut self, mut control: LiveControl) -> bool {
        let c = &mut control;
        let width_ok = if c.is_button() { c.start == c.end } else { c.start < c.end };
        if self.kind != BlockKind::Code || !width_ok || span_bytes(&self.text, c.line, c.start, c.end).is_none() {
            return false;
        }
        match &mut c.kind {
            ControlKind::Slider { min, max, step } => {
                if !(min.is_finite() && max.is_finite() && step.is_finite() && min <= max && *step > 0.0) {
                    return false;
                }
            }
            ControlKind::DropDown { items, labels } if labels.len() != items.len() => *labels = items.clone(),
            _ => {}
        }
        let clash = |o: &LiveControl| {
            o.line == c.line && ((o.start < c.end && c.start < o.end) || (o.start == o.end && c.start == c.end && o.start == c.start))
        };
        if self.controls.iter().any(clash) {
            return false;
        }
        let at = self.controls.partition_point(|o| (o.line, o.start) <= (control.line, control.start));
        self.controls.insert(at, control);
        true
    }

    /// Call after `text` was edited (old text given): shifts spans by the edit, and drops controls whose span the edit touched or that no longer fit.
    /// The edit is taken as the single change between the common prefix and suffix; an edit right next to a literal
    /// that would fuse with it (e.g. a digit typed after a number) also counts as touching it.
    pub fn reanchor_controls(&mut self, old_text: &str) {
        if self.controls.is_empty() || old_text == self.text {
            return;
        }
        let new = self.text.as_str();
        let a = old_text.char_indices().zip(new.chars()).find(|((_, x), y)| x != y).map_or(old_text.len().min(new.len()), |((i, _), _)| i);
        let suffix: usize = old_text[a..].chars().rev().zip(new[a..].chars().rev()).take_while(|(x, y)| x == y).map(|(x, _)| x.len_utf8()).sum();
        let (old_b, new_b) = (old_text.len() - suffix, new.len() - suffix);
        let line_shift = new[a..new_b].matches('\n').count() as isize - old_text[a..old_b].matches('\n').count() as isize;
        self.controls.retain_mut(|c| {
            let Some(r) = span_bytes(old_text, c.line, c.start, c.end) else { return false };
            let lit = &old_text[r.clone()];
            let empty = r.is_empty();
            let before = if empty {
                r.start <= a
            } else {
                r.end < a || (r.end == a && !(lit.chars().next_back().is_some_and(glues) && new[a..].chars().next().is_some_and(glues)))
            };
            let after = if empty {
                r.start >= old_b
            } else {
                r.start > old_b || (r.start == old_b && !(lit.chars().next().is_some_and(glues) && new[..new_b].chars().next_back().is_some_and(glues)))
            };
            if !before {
                if !after {
                    return false;
                }
                let s = r.start - old_b + new_b;
                if !old_text[old_b..r.start].contains('\n') {
                    let ls = new[..s].rfind('\n').map_or(0, |i| i + 1);
                    let col = new[ls..s].chars().count();
                    c.end = col + (c.end - c.start);
                    c.start = col;
                }
                let Some(line) = c.line.checked_add_signed(line_shift) else { return false };
                c.line = line;
            }
            span_bytes(new, c.line, c.start, c.end).is_some_and(|nr| &new[nr] == lit)
        });
    }
}

impl Document {
    /// Adds a control over [start, end) of `line` in block `index` (a code block); for a Button, start == end and nothing is replaced. Returns false if the span is invalid or overlaps another control.
    pub fn add_control(&mut self, index: usize, line: usize, start: usize, end: usize, kind: ControlKind) -> bool {
        self.blocks.get_mut(index).is_some_and(|b| b.add_control(LiveControl { kind, line, start, end, extra: Vec::new() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slider() -> ControlKind {
        ControlKind::Slider { min: 0.0, max: 10.0, step: 1.0 }
    }

    fn block(text: &str) -> (Document, usize) {
        let mut d = Document::new();
        d.push(BlockKind::Text, "intro");
        d.push(BlockKind::Code, text);
        (d, 1)
    }

    fn edit(b: &mut Block, new: &str) {
        let old = std::mem::replace(&mut b.text, new.to_string());
        b.reanchor_controls(&old);
    }

    fn spans(b: &Block) -> Vec<(usize, usize, usize, &str)> {
        (0..b.controls.len()).map(|i| (b.controls[i].line, b.controls[i].start, b.controls[i].end, b.control_text(i))).collect()
    }

    #[test]
    fn add_validates() {
        let (mut d, i) = block("x = 3; s = 'é';\ny = 2;");
        assert!(!d.add_control(0, 0, 0, 1, ControlKind::CheckBox), "text block");
        assert!(d.add_control(i, 0, 4, 5, slider()));
        assert!(!d.add_control(i, 0, 4, 6, ControlKind::EditField), "overlap");
        assert!(!d.add_control(i, 0, 3, 3, ControlKind::EditField), "empty span");
        assert!(!d.add_control(i, 0, 5, 5, slider()), "zero-width slider");
        assert!(!d.add_control(i, 2, 0, 1, ControlKind::EditField), "no such line");
        assert!(!d.add_control(i, 1, 0, 7, ControlKind::EditField), "past the line end");
        assert!(d.add_control(i, 0, 11, 14, ControlKind::DropDown { items: vec!["'é'".into(), "'a'".into()], labels: vec![] }));
        assert!(d.add_control(i, 1, 6, 6, ControlKind::Button { label: "Run".into() }));
        assert!(!d.add_control(i, 1, 6, 6, ControlKind::Button { label: "Again".into() }));
        let b = &d.blocks[i];
        assert_eq!(spans(b), vec![(0, 4, 5, "3"), (0, 11, 14, "'é'"), (1, 6, 6, "")]);
        assert_eq!(b.controls[1].kind, ControlKind::DropDown { items: vec!["'é'".into(), "'a'".into()], labels: vec!["'é'".into(), "'a'".into()] });
    }

    #[test]
    fn set_value_shifts_later_controls_on_the_line() {
        let (mut d, i) = block("f(3, 'sin'); g = 1;\nh = 2;");
        assert!(d.add_control(i, 0, 5, 10, ControlKind::DropDown { items: vec!["'sin'".into(), "'cos'".into()], labels: vec![] }));
        assert!(d.add_control(i, 0, 2, 3, slider()));
        assert!(d.add_control(i, 1, 4, 5, slider()));
        let b = &mut d.blocks[i];
        b.set_control_value(0, "10");
        assert_eq!(b.text, "f(10, 'sin'); g = 1;\nh = 2;");
        assert_eq!(spans(b), vec![(0, 2, 4, "10"), (0, 6, 11, "'sin'"), (1, 4, 5, "2")]);
        b.set_control_value(1, "'cos'");
        b.set_control_value(0, "7");
        assert_eq!(b.text, "f(7, 'cos'); g = 1;\nh = 2;");
        assert_eq!(spans(b), vec![(0, 2, 3, "7"), (0, 5, 10, "'cos'"), (1, 4, 5, "2")]);
        b.set_control_value(0, "");
        assert_eq!(b.control_text(0), "7");
    }

    #[test]
    fn reanchor_edits_before_after_and_inside() {
        let (mut d, i) = block("a = 1;\nfreq = 3; amp = 2;");
        assert!(d.add_control(i, 1, 7, 8, slider()));
        assert!(d.add_control(i, 1, 16, 17, slider()));
        let b = &mut d.blocks[i];
        // Before the span on the same line.
        edit(b, "a = 1;\nfrequency = 3; amp = 2;");
        assert_eq!(spans(b), vec![(1, 12, 13, "3"), (1, 21, 22, "2")]);
        // After the span.
        edit(b, "a = 1;\nfrequency = 3; amp = 2; % gain");
        assert_eq!(spans(b), vec![(1, 12, 13, "3"), (1, 21, 22, "2")]);
        // Deleting the semicolon right after a literal keeps it.
        edit(b, "a = 1;\nfrequency = 3 amp = 2; % gain");
        assert_eq!(spans(b), vec![(1, 12, 13, "3"), (1, 20, 21, "2")]);
        edit(b, "a = 1;\nfrequency = 3; amp = 2; % gain");
        // Inside the span: only that control goes.
        edit(b, "a = 1;\nfrequency = 3; amp = 9; % gain");
        assert_eq!(spans(b), vec![(1, 12, 13, "3")]);
        // A digit typed right after the literal fuses with it.
        edit(b, "a = 1;\nfrequency = 35; amp = 9; % gain");
        assert!(b.controls.is_empty());
    }

    #[test]
    fn reanchor_line_insertions_and_deletions() {
        let (mut d, i) = block("a = 1;\nb = 2;\nx = 3; % é");
        assert!(d.add_control(i, 2, 4, 5, slider()));
        assert!(d.add_control(i, 2, 10, 10, ControlKind::Button { label: "Run".into() }));
        let b = &mut d.blocks[i];
        edit(b, "a = 1;\nnew = 0;\n\nb = 2;\nx = 3; % é");
        assert_eq!(spans(b), vec![(4, 4, 5, "3"), (4, 10, 10, "")]);
        edit(b, "a = 1;\nx = 3; % é");
        assert_eq!(spans(b), vec![(1, 4, 5, "3"), (1, 10, 10, "")]);
        // Joining the control's line onto the previous one moves its columns.
        edit(b, "a = 1;x = 3; % é");
        assert_eq!(spans(b), vec![(0, 10, 11, "3"), (0, 16, 16, "")]);
        edit(b, "a = 1;x = 3; % é\nz = 0;");
        assert_eq!(spans(b), vec![(0, 10, 11, "3"), (0, 16, 16, "")]);
        // Deleting the line with the controls drops them.
        edit(b, "z = 0;");
        assert!(b.controls.is_empty());
    }

    #[test]
    fn reanchor_drops_spans_that_no_longer_fit() {
        let (mut d, i) = block("x = 3;");
        assert!(d.add_control(i, 0, 4, 5, slider()));
        let b = &mut d.blocks[i];
        b.controls[0].line = 3;
        edit(b, "x = 3; ");
        assert!(b.controls.is_empty());
    }
}
