//! matlivre language engine: a clean-room implementation of a MATLAB-compatible language
//! (see `docs/SPEC-live-script.md`, section 5).

pub mod ast;
pub mod builtins;
pub mod builtins_data;
pub mod builtins_datafun;
pub mod builtins_dates;
pub mod builtins_elfun;
pub mod builtins_elmat;
pub mod builtins_files;
pub mod builtins_funfun;
pub mod builtins_matfun;
pub mod builtins_ops;
pub mod builtins_plot;
pub mod builtins_geom;
pub mod builtins_interpn;
pub mod builtins_lang;
pub mod builtins_polyfun;
pub mod builtins_specfun;
pub mod builtins_strings;
pub mod builtins_text;
pub mod builtins_textio;
pub mod builtins_types;
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
pub mod polyfun_classes;
pub mod polyshape;
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
