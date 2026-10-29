//! matlivre language engine: a clean-room implementation of a MATLAB-compatible language
//! (see `docs/SPEC-live-script.md`, section 5).

pub mod ast;
pub mod builtins;
pub mod builtins_elfun;
pub mod builtins_plot;
pub mod builtins_interpn;
pub mod builtins_polyfun;
pub mod builtins_text;
pub mod class;
pub mod cplx;
pub mod display;
pub mod error;
pub mod format;
pub mod graphics;
pub mod indexing;
pub mod interp;
pub mod jit;
pub mod lexer;
pub mod nd;
pub mod ops;
pub mod parser;
pub mod sym;
pub mod table;
pub mod time;
pub mod value;

pub use display::NumFormat;
pub use error::LangError;
pub use graphics::{Axes, Figure, LineStyle, Marker, Series, SeriesKind, XAxisKind};
pub use interp::{Interpreter, Output, OutputKind, VarInfo};
pub use value::Value;

/// Names that the editor highlights as keywords.
pub const KEYWORDS: &[&str] = &[
    "if", "elseif", "else", "end", "for", "parfor", "while", "break", "continue", "return", "function", "switch", "case",
    "otherwise", "try", "catch", "global", "persistent", "classdef", "properties", "methods", "events", "enumeration",
];
