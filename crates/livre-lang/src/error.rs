use std::fmt;

/// A parse or runtime error, with the 1-based line (within the code block) it occurred on.
#[derive(Debug, Clone, PartialEq)]
pub struct LangError {
    pub message: String,
    pub line: usize,
}

impl LangError {
    pub fn new(message: impl Into<String>, line: usize) -> Self {
        Self { message: message.into(), line }
    }
}

impl fmt::Display for LangError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LangError {}

/// Shorthand for building a runtime error before the line is known; the interpreter fills in
/// the line of the statement being executed.
pub fn rt<T>(message: impl Into<String>) -> Result<T, LangError> {
    Err(LangError::new(message, 0))
}
