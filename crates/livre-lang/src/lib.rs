//! matlivre language engine: a clean-room implementation of a MATLAB-compatible language
//! subset (see `docs/SPEC-live-script.md`, section 5).

pub mod ast;
pub mod builtins;
pub mod display;
pub mod error;
pub mod format;
pub mod graphics;
pub mod interp;
pub mod lexer;
pub mod ops;
pub mod parser;
pub mod value;

pub use display::NumFormat;
pub use error::LangError;
pub use graphics::{Axes, Figure, LineStyle, Marker, Series, SeriesKind};
pub use interp::{Interpreter, Output, OutputKind, VarInfo};
pub use value::Value;

/// Names that the editor highlights as keywords.
pub const KEYWORDS: &[&str] = &[
    "if", "elseif", "else", "end", "for", "parfor", "while", "break", "continue", "return", "function", "switch", "case",
    "otherwise", "try", "catch", "global", "persistent",
];
