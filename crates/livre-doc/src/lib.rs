//! Live script document model and interchange formats (see `docs/SPEC-live-script.md`
//! sections 1–2).

mod mfile;
mod mlx;

use std::ops::Range;
use std::path::Path;

pub use mfile::{read_m, write_m};
pub use mlx::{read_mlx, read_mlx_bytes, write_mlx, write_mlx_bytes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Title,
    Heading,
    Heading2,
    Text,
    Code,
    SectionBreak,
}

impl BlockKind {
    pub fn is_text_like(self) -> bool {
        matches!(self, BlockKind::Title | BlockKind::Heading | BlockKind::Heading2 | BlockKind::Text)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Stable identity used to anchor outputs while the document is edited.
    pub id: u64,
    pub kind: BlockKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub blocks: Vec<Block>,
    next_id: u64,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum DocError {
    Io(std::io::Error),
    Format(String),
}

impl std::fmt::Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocError::Io(e) => write!(f, "{e}"),
            DocError::Format(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for DocError {}

impl From<std::io::Error> for DocError {
    fn from(e: std::io::Error) -> Self {
        DocError::Io(e)
    }
}

impl Document {
    pub fn new() -> Self {
        Self { blocks: Vec::new(), next_id: 1 }
    }

    /// A new document with one empty code block, as a fresh live script starts.
    pub fn new_script() -> Self {
        let mut d = Self::new();
        d.push(BlockKind::Code, "");
        d
    }

    pub fn push(&mut self, kind: BlockKind, text: impl Into<String>) -> u64 {
        let id = self.alloc_id();
        self.blocks.push(Block { id, kind, text: text.into() });
        id
    }

    pub fn insert(&mut self, at: usize, kind: BlockKind, text: impl Into<String>) -> u64 {
        let id = self.alloc_id();
        self.blocks.insert(at.min(self.blocks.len()), Block { id, kind, text: text.into() });
        id
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.blocks.iter().position(|b| b.id == id)
    }

    /// Block index ranges of the sections (split at section breaks, breaks excluded).
    pub fn sections(&self) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        let mut start = 0;
        for (i, b) in self.blocks.iter().enumerate() {
            if b.kind == BlockKind::SectionBreak {
                out.push(start..i);
                start = i + 1;
            }
        }
        out.push(start..self.blocks.len());
        out
    }

    /// Index of the section containing block `i`.
    pub fn section_of(&self, i: usize) -> usize {
        self.blocks[..i.min(self.blocks.len())].iter().filter(|b| b.kind == BlockKind::SectionBreak).count()
    }

    pub fn load(path: &Path) -> Result<Document, DocError> {
        match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
            Some("mlx") => read_mlx(path),
            _ => Ok(read_m(&std::fs::read_to_string(path)?)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), DocError> {
        match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
            Some("mlx") => write_mlx(self, path),
            _ => Ok(std::fs::write(path, write_m(self))?),
        }
    }
}
