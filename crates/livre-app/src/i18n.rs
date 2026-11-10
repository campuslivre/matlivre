//! The interface language. Every piece of the app's own text goes through `tr` (or `trf` when
//! it has `{}` holes), keyed by its English wording; each other language has a table in its
//! own module. MATLAB output, interpreter errors and code are never translated.

mod de;
mod es;
mod fr;
mod it;
mod ja;
mod pt;
mod ru;
mod zh;

use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::icons::Ic;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Pt,
    Es,
    Fr,
    De,
    It,
    Ru,
    Zh,
    Ja,
}

impl Lang {
    /// In the order the selector lists them.
    pub const ALL: [Lang; 9] = [Lang::En, Lang::Pt, Lang::Es, Lang::Fr, Lang::De, Lang::It, Lang::Ru, Lang::Zh, Lang::Ja];

    /// The value saved as `language=` in the settings, and the system locale's prefix.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Pt => "pt",
            Lang::Es => "es",
            Lang::Fr => "fr",
            Lang::De => "de",
            Lang::It => "it",
            Lang::Ru => "ru",
            Lang::Zh => "zh",
            Lang::Ja => "ja",
        }
    }

    pub fn from_code(s: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code() == s)
    }

    /// The language's own name, as the selector lists it.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Pt => "Português",
            Lang::Es => "Español",
            Lang::Fr => "Français",
            Lang::De => "Deutsch",
            Lang::It => "Italiano",
            Lang::Ru => "Русский",
            Lang::Zh => "中文",
            Lang::Ja => "日本語",
        }
    }

    pub fn flag(self) -> Ic {
        match self {
            Lang::En => Ic::UsFlag,
            Lang::Pt => Ic::BrazilFlag,
            Lang::Es => Ic::SpainFlag,
            Lang::Fr => Ic::FranceFlag,
            Lang::De => Ic::GermanyFlag,
            Lang::It => Ic::ItalyFlag,
            Lang::Ru => Ic::RussiaFlag,
            Lang::Zh => Ic::ChinaFlag,
            Lang::Ja => Ic::JapanFlag,
        }
    }

    /// Whether the language needs the CJK fonts.
    pub fn cjk(self) -> bool {
        matches!(self, Lang::Zh | Lang::Ja)
    }

    /// The system's interface language when the app has it, else English.
    pub fn system() -> Lang {
        crate::os::ui_language().and_then(|tag| from_tag(&tag)).unwrap_or(Lang::En)
    }

    fn table(self, en: &str) -> Option<&'static str> {
        match self {
            Lang::En => None,
            Lang::Pt => pt::get(en),
            Lang::Es => es::get(en),
            Lang::Fr => fr::get(en),
            Lang::De => de::get(en),
            Lang::It => it::get(en),
            Lang::Ru => ru::get(en),
            Lang::Zh => zh::get(en),
            Lang::Ja => ja::get(en),
        }
    }

    /// Whether numbers take a decimal comma.
    fn decimal_comma(self) -> bool {
        !matches!(self, Lang::En | Lang::Zh | Lang::Ja)
    }
}

/// The language of a locale tag: `pt`, `pt-BR`, `zh_CN.UTF-8`, `ja-JP`…
fn from_tag(tag: &str) -> Option<Lang> {
    let t = tag.trim().to_ascii_lowercase();
    let primary = t.split(['-', '_', '.', '@']).next()?;
    Lang::from_code(primary)
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn lang() -> Lang {
    Lang::ALL[LANG.load(Ordering::Relaxed) as usize % Lang::ALL.len()]
}

pub fn set(l: Lang) {
    LANG.store(Lang::ALL.iter().position(|x| *x == l).unwrap_or(0) as u8, Ordering::Relaxed);
}

/// `en` in the interface language.
pub fn tr(en: &'static str) -> &'static str {
    tr_in(lang(), en)
}

fn tr_in(l: Lang, en: &'static str) -> &'static str {
    l.table(en).unwrap_or(en)
}

/// `tr(en)` with each `{}` replaced by the next of `args`.
pub fn trf(en: &'static str, args: &[&dyn Display]) -> String {
    trf_in(lang(), en, args)
}

fn trf_in(l: Lang, en: &'static str, args: &[&dyn Display]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut parts = tr_in(l, en).split("{}").peekable();
    while let Some(p) = parts.next() {
        out.push_str(p);
        if parts.peek().is_some() {
            if let Some(a) = args.next() {
                out.push_str(&a.to_string());
            }
        }
    }
    out
}

/// A number with two decimals, with a decimal comma where the language uses one.
pub fn decimal2(x: f64) -> String {
    decimal2_in(lang(), x)
}

fn decimal2_in(l: Lang, x: f64) -> String {
    let s = format!("{x:.2}");
    if l.decimal_comma() {
        s.replace('.', ",")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The string literals passed to `tr(` and `trf(` in the app's sources.
    fn keys_in(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for call in ["tr(\"", "trf(\""] {
            let mut from = 0;
            while let Some(k) = src[from..].find(call) {
                let at = from + k;
                from = at + call.len();
                let before = src[..at].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let mut s = String::new();
                let mut chars = src[from..].chars();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some(e) => s.push(e),
                            None => break,
                        },
                        c => s.push(c),
                    }
                }
                out.push(s);
            }
        }
        out
    }

    /// Every string the sources translate, with the file it is in.
    fn all_keys() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut keys = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") && path.file_name().is_some_and(|n| n != "i18n.rs") {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                keys.extend(keys_in(&std::fs::read_to_string(&path).unwrap()).into_iter().map(|k| (name.clone(), k)));
            }
        }
        keys
    }

    #[test]
    fn every_string_has_a_translation() {
        let keys = all_keys();
        assert!(keys.len() > 100, "found only {} translated strings", keys.len());
        let mut missing = Vec::new();
        for l in Lang::ALL.into_iter().filter(|l| *l != Lang::En) {
            for (file, key) in &keys {
                match l.table(key) {
                    None => missing.push(format!("{l:?} {file}: {key:?}")),
                    Some(t) if t.matches("{}").count() != key.matches("{}").count() => missing.push(format!("{l:?} {file}: {key:?} (holes differ)")),
                    _ => {}
                }
            }
        }
        assert!(missing.is_empty(), "missing translations:\n{}", missing.join("\n"));
    }

    #[test]
    fn formats_and_detects() {
        assert_eq!(trf_in(Lang::Pt, "Section {} of {}", &[&2, &5]), "Seção 2 de 5");
        assert_eq!(decimal2_in(Lang::Pt, 1.5), "1,50");
        assert_eq!(decimal2_in(Lang::Ja, 1.5), "1.50");
        assert_eq!(trf_in(Lang::En, "Section {} of {}", &[&2, &5]), "Section 2 of 5");
        assert_eq!(decimal2_in(Lang::En, 1.5), "1.50");
        for (t, l) in [("pt", Lang::Pt), ("pt-BR", Lang::Pt), ("pt_PT.UTF-8", Lang::Pt), ("PT-br", Lang::Pt), ("zh-Hans-CN", Lang::Zh), ("ja_JP.UTF-8", Lang::Ja), ("de-AT", Lang::De), ("ru", Lang::Ru)] {
            assert_eq!(from_tag(t), Some(l), "{t}");
        }
        for t in ["en-US", "C", "", "ptx", "ko-KR"] {
            assert!(matches!(from_tag(t), None | Some(Lang::En)), "{t}");
        }
        assert_eq!(Lang::from_code("pt"), Some(Lang::Pt));
        assert_eq!(Lang::from_code("xx"), None);
        for l in Lang::ALL {
            set(l);
            assert_eq!(lang(), l);
        }
        set(Lang::En);
    }
}
