//! MatLivre language engine: a clean-room implementation of a MATLAB-compatible language
//! (see `docs/SPEC-live-script.md`, section 5).

// mimalloc keeps freed memory mapped. With the system allocator every large array comes back from
// the OS as untouched pages, and faulting them in cost a third of a 300 x 300 product and about
// half of a whole-array operation on a few million elements.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod ast;
pub mod sparse_lu;
pub mod sparse_ops;
pub mod fft_big;
pub mod fuse;
pub mod builtins;
pub mod containers;
pub mod builtins_data;
pub mod builtins_datafun;
pub mod builtins_dates;
pub mod builtins_elfun;
pub mod builtins_elmat;
pub mod builtins_excel;
pub mod builtins_files;
pub mod builtins_funfun;
pub mod builtins_matfun;
pub mod builtins_media;
pub mod builtins_archive;
pub mod builtins_structio;
pub mod builtins_ops;
pub mod builtins_plot;
pub mod builtins_plotmore;
pub mod builtins_plotfmt;
pub mod builtins_graph3d;
pub mod builtins_geom;
pub mod builtins_interpn;
pub mod builtins_lang;
pub mod builtins_polyfun;
pub mod builtins_specfun;
pub mod builtins_strings;
pub mod builtins_text;
pub mod builtins_validate;
pub mod builtins_textio;
pub mod builtins_types;
pub mod class;
pub mod cplx;
pub mod decomp;
pub mod figfile;
pub mod xml_dom;
pub mod gfx_defaults;
pub mod messages;
pub mod display;
pub mod error;
pub mod format;
pub mod gfx;
pub mod gfx_util;
pub mod graphics;
pub mod graphs;
pub mod indexing;
pub mod interp;
pub mod jit;
pub mod lexer;
pub mod nd;
pub mod ode_objects;
pub mod ops;
pub mod parser;
pub mod patterns;
pub mod polyfun_classes;
pub mod polyshape;
pub mod sym;
pub mod sysobj;
pub mod table;
pub mod tiff;
pub mod table_ops;
pub mod builtins_sparse;
pub mod builtins_ode;
pub mod timetable;
pub mod io_more;
pub mod lang_more;
pub mod plot_extra;
pub mod volume;
pub mod timeseries;
pub mod charts;
pub mod images;
pub mod math_more;
pub mod func_objects;
pub mod files_more;
pub mod random;
pub mod session_more;
pub mod settings;
pub mod timers;
pub mod zones;
pub mod time;
pub mod value;

pub use display::NumFormat;
pub use error::LangError;
pub use graphics::{Axes, ExportFormat, Figure, LineStyle, Marker, Series, SeriesKind, Ticks, XAxisKind};
pub use interp::{Interpreter, Output, OutputKind, VarInfo};
pub use value::Value;

/// Names that the editor highlights as keywords.
pub const KEYWORDS: &[&str] = &[
    "if", "elseif", "else", "end", "for", "parfor", "while", "break", "continue", "return", "function", "switch", "case",
    "otherwise", "try", "catch", "global", "persistent", "classdef", "properties", "methods", "events", "enumeration",
];
