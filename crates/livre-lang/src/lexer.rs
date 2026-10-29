//! Tokenizer. MATLAB lexing depends on context: `'` is a transpose after a value but a
//! quote elsewhere, whitespace separates elements inside `[]` / `{}`, and a statement such
//! as `hold on` is "command syntax" whose words are passed as char arguments.

use crate::error::LangError;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(f64),
    /// Imaginary literal such as `3i`.
    Imag(f64),
    /// Single-quoted char array.
    Chars(String),
    /// Double-quoted string scalar.
    Str(String),
    Ident(String),
    Kw(Kw),
    /// Argument of a command-syntax statement.
    CmdArg(String),
    Op(&'static str),
    /// Statement separator; `true` when it was `;` (output suppressed).
    Sep(bool),
    Newline,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kw {
    If,
    Elseif,
    Else,
    End,
    For,
    Parfor,
    While,
    Break,
    Continue,
    Return,
    Function,
    Switch,
    Case,
    Otherwise,
    Try,
    Catch,
    Global,
    Persistent,
}

fn keyword(s: &str) -> Option<Kw> {
    Some(match s {
        "if" => Kw::If,
        "elseif" => Kw::Elseif,
        "else" => Kw::Else,
        "end" => Kw::End,
        "for" => Kw::For,
        "parfor" => Kw::Parfor,
        "while" => Kw::While,
        "break" => Kw::Break,
        "continue" => Kw::Continue,
        "return" => Kw::Return,
        "function" => Kw::Function,
        "switch" => Kw::Switch,
        "case" => Kw::Case,
        "otherwise" => Kw::Otherwise,
        "try" => Kw::Try,
        "catch" => Kw::Catch,
        "global" => Kw::Global,
        "persistent" => Kw::Persistent,
        _ => return None,
    })
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    /// Whitespace immediately preceded this token.
    pub space_before: bool,
}

const OPS: [&str; 30] = [
    "...", ".^", ".*", "./", ".\\", ".'", "==", "~=", "!=", "<=", ">=", "&&", "||", "+", "-", "*",
    "/", "\\", "^", "'", "<", ">", "&", "|", "~", "!", "=", ":", ".", "@",
];

pub fn tokenize(src: &str) -> Result<Vec<Token>, LangError> {
    Lexer { s: src.as_bytes(), src, i: 0, line: 1, out: Vec::new(), brackets: Vec::new() }.run()
}

struct Lexer<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
    line: usize,
    out: Vec<Token>,
    /// Open bracket stack: b'(', b'[', b'{'.
    brackets: Vec<u8>,
}

impl<'a> Lexer<'a> {
    fn peek(&self, k: usize) -> u8 {
        *self.s.get(self.i + k).unwrap_or(&0)
    }

    fn push(&mut self, tok: Tok, space_before: bool) {
        self.out.push(Token { tok, line: self.line, space_before });
    }

    fn in_matrix(&self) -> bool {
        matches!(self.brackets.last(), Some(b'[') | Some(b'{'))
    }

    /// The previous token ends a value, so `'` is a transpose and whitespace before a new
    /// value inside brackets separates elements.
    fn prev_is_value(&self) -> bool {
        match self.out.last().map(|t| &t.tok) {
            Some(Tok::Num(_)) | Some(Tok::Imag(_)) | Some(Tok::Chars(_)) | Some(Tok::Str(_)) | Some(Tok::Ident(_)) => true,
            Some(Tok::Kw(Kw::End)) => !self.brackets.is_empty(),
            Some(Tok::Op(op)) => matches!(*op, ")" | "]" | "}" | "'" | ".'"),
            _ => false,
        }
    }

    fn at_statement_start(&self) -> bool {
        match self.out.last().map(|t| &t.tok) {
            None | Some(Tok::Newline) | Some(Tok::Sep(_)) => true,
            _ => false,
        }
    }

    fn err(&self, msg: impl Into<String>) -> LangError {
        LangError::new(msg, self.line)
    }

    fn run(mut self) -> Result<Vec<Token>, LangError> {
        let mut at_line_start = true;
        loop {
            let mut space = false;
            while matches!(self.peek(0), b' ' | b'\t' | b'\r') {
                self.i += 1;
                space = true;
            }
            let c = self.peek(0);
            if c == 0 {
                break;
            }
            // Block comment: a line holding only `%{` ... a line holding only `%}`.
            if at_line_start && c == b'%' && self.peek(1) == b'{' && self.rest_of_line_blank(2) {
                self.skip_block_comment();
                continue;
            }
            at_line_start = false;
            if c == b'%' {
                while !matches!(self.peek(0), b'\n' | 0) {
                    self.i += 1;
                }
                continue;
            }
            if c == b'.' && self.peek(1) == b'.' && self.peek(2) == b'.' {
                // Continuation: ignore the rest of the line and the newline.
                while !matches!(self.peek(0), b'\n' | 0) {
                    self.i += 1;
                }
                if self.peek(0) == b'\n' {
                    self.i += 1;
                    self.line += 1;
                }
                continue;
            }
            if c == b'\n' {
                self.i += 1;
                if self.in_matrix() {
                    // Newline inside brackets is a row separator.
                    if !matches!(self.out.last().map(|t| &t.tok), Some(Tok::Sep(true)) | Some(Tok::Op("[")) | Some(Tok::Op("{"))) {
                        self.push(Tok::Sep(true), space);
                    }
                } else if self.brackets.is_empty() {
                    self.push(Tok::Newline, space);
                }
                self.line += 1;
                at_line_start = true;
                continue;
            }

            // Inside a matrix, whitespace between two values separates elements.
            if self.in_matrix() && space && self.prev_is_value() && self.starts_value(c) {
                self.push(Tok::Sep(false), true);
            }

            if c.is_ascii_digit() || (c == b'.' && self.peek(1).is_ascii_digit()) {
                let n = self.number()?;
                // `3i` / `2.5j` are imaginary literals.
                if matches!(self.peek(0), b'i' | b'j') && !(self.peek(1).is_ascii_alphanumeric() || self.peek(1) == b'_') {
                    self.i += 1;
                    self.push(Tok::Imag(n), space);
                } else {
                    self.push(Tok::Num(n), space);
                }
                continue;
            }
            if c.is_ascii_alphabetic() || c == b'_' {
                let start = self.i;
                while self.peek(0).is_ascii_alphanumeric() || self.peek(0) == b'_' {
                    self.i += 1;
                }
                let word = &self.src[start..self.i];
                if let Some(kw) = keyword(word) {
                    self.push(Tok::Kw(kw), space);
                } else {
                    let stmt_start = self.at_statement_start() && self.brackets.is_empty();
                    self.push(Tok::Ident(word.to_string()), space);
                    if stmt_start && self.looks_like_command() {
                        self.command_args();
                    }
                }
                continue;
            }
            if c == b'"' {
                let s = self.quoted(b'"')?;
                self.push(Tok::Str(s), space);
                continue;
            }
            if c == b'\'' {
                let transpose = self.prev_is_value() && !(self.in_matrix() && space);
                if !transpose {
                    let s = self.quoted(b'\'')?;
                    self.push(Tok::Chars(s), space);
                    continue;
                }
            }
            match c {
                b'(' | b'[' | b'{' => {
                    self.brackets.push(c);
                    self.i += 1;
                    let op = match c { b'(' => "(", b'[' => "[", _ => "{" };
                    self.push(Tok::Op(op), space);
                    continue;
                }
                b')' | b']' | b'}' => {
                    self.brackets.pop();
                    // Drop a trailing row separator such as `[1 2\n]`.
                    if c != b')' && matches!(self.out.last().map(|t| &t.tok), Some(Tok::Sep(_))) {
                        self.out.pop();
                    }
                    self.i += 1;
                    let op = match c { b')' => ")", b']' => "]", _ => "}" };
                    self.push(Tok::Op(op), space);
                    continue;
                }
                b',' => {
                    self.i += 1;
                    self.push(Tok::Sep(false), space);
                    continue;
                }
                b';' => {
                    self.i += 1;
                    self.push(Tok::Sep(true), space);
                    continue;
                }
                _ => {}
            }
            let mut matched = None;
            for op in OPS {
                if self.s[self.i..].starts_with(op.as_bytes()) {
                    matched = Some(op);
                    break;
                }
            }
            let Some(op) = matched else {
                return Err(self.err(format!("Invalid character '{}'.", c as char)));
            };
            self.i += op.len();
            let op = match op {
                "!=" => "~=",
                "!" => "~",
                o => o,
            };
            self.push(Tok::Op(op), space);
        }
        self.push(Tok::Newline, false);
        self.push(Tok::Eof, false);
        Ok(self.out)
    }

    /// Whether `c` (with what follows) begins a new element after whitespace in a matrix.
    fn starts_value(&self, c: u8) -> bool {
        match c {
            b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'"' | b'\'' | b'(' | b'[' | b'{' | b'@' => true,
            b'.' => self.peek(1).is_ascii_digit(),
            // `[1 -2]` is two elements, `[1 - 2]` is one.
            b'+' | b'-' => !matches!(self.peek(1), b' ' | b'\t' | b'=' ),
            b'~' | b'!' => self.peek(1) != b'=',
            _ => false,
        }
    }

    fn rest_of_line_blank(&self, from: usize) -> bool {
        let mut k = from;
        loop {
            match self.peek(k) {
                b' ' | b'\t' | b'\r' => k += 1,
                b'\n' | 0 => return true,
                _ => return false,
            }
        }
    }

    fn skip_block_comment(&mut self) {
        loop {
            // Advance to the next line.
            while !matches!(self.peek(0), b'\n' | 0) {
                self.i += 1;
            }
            if self.peek(0) == 0 {
                return;
            }
            self.i += 1;
            self.line += 1;
            let mut k = 0;
            while matches!(self.peek(k), b' ' | b'\t') {
                k += 1;
            }
            if self.peek(k) == b'%' && self.peek(k + 1) == b'}' && self.rest_of_line_blank(k + 2) {
                while !matches!(self.peek(0), b'\n' | 0) {
                    self.i += 1;
                }
                return;
            }
        }
    }

    fn number(&mut self) -> Result<f64, LangError> {
        let start = self.i;
        while self.peek(0).is_ascii_digit() {
            self.i += 1;
        }
        // A '.' followed by an operator char is an elementwise operator, not a decimal point.
        if self.peek(0) == b'.' && !matches!(self.peek(1), b'*' | b'/' | b'\\' | b'^' | b'\'') {
            self.i += 1;
            while self.peek(0).is_ascii_digit() {
                self.i += 1;
            }
        }
        if matches!(self.peek(0), b'e' | b'E')
            && (self.peek(1).is_ascii_digit() || (matches!(self.peek(1), b'+' | b'-') && self.peek(2).is_ascii_digit()))
        {
            self.i += 2;
            while self.peek(0).is_ascii_digit() {
                self.i += 1;
            }
        }
        self.src[start..self.i].parse::<f64>().map_err(|_| self.err("Invalid number."))
    }

    fn quoted(&mut self, q: u8) -> Result<String, LangError> {
        self.i += 1;
        let mut out = String::new();
        let start_line = self.line;
        loop {
            let c = self.peek(0);
            if c == 0 || c == b'\n' {
                return Err(LangError::new("String is not terminated properly.", start_line));
            }
            if c == q {
                if self.peek(1) == q {
                    out.push(q as char);
                    self.i += 2;
                    continue;
                }
                self.i += 1;
                return Ok(out);
            }
            // Copy one UTF-8 character.
            let ch = self.src[self.i..].chars().next().unwrap();
            out.push(ch);
            self.i += ch.len_utf8();
        }
    }

    /// `name word ...` at the start of a statement is command syntax when the word is not
    /// an operator or assignment (`hold on`, `format long`, `close all`).
    fn looks_like_command(&self) -> bool {
        let c0 = self.peek(0);
        if c0 != b' ' && c0 != b'\t' {
            return false;
        }
        let mut k = 0;
        while matches!(self.peek(k), b' ' | b'\t') {
            k += 1;
        }
        let c = self.peek(k);
        if !(c.is_ascii_alphabetic() || c == b'-') {
            return false;
        }
        if c == b'-' && !self.peek(k + 1).is_ascii_alphabetic() {
            return false;
        }
        // `x and` is unusual; `a = ` or `a == ` never gets here. Reject `name word(`/`name word =`
        // patterns that are clearly expressions such as `x ...`.
        let mut j = k;
        while self.peek(j).is_ascii_alphanumeric() || self.peek(j) == b'_' || self.peek(j) == b'-' {
            j += 1;
        }
        while matches!(self.peek(j), b' ' | b'\t') {
            j += 1;
        }
        !(self.peek(j) == b'=' && self.peek(j + 1) != b'=') && self.peek(j) != b'('
    }

    fn command_args(&mut self) {
        loop {
            while matches!(self.peek(0), b' ' | b'\t' | b'\r') {
                self.i += 1;
            }
            let c = self.peek(0);
            if matches!(c, b'\n' | 0 | b';' | b',' | b'%') {
                return;
            }
            let start = self.i;
            if c == b'\'' {
                if let Ok(s) = self.quoted(b'\'') {
                    self.push(Tok::CmdArg(s), true);
                }
                continue;
            }
            while !matches!(self.peek(0), b' ' | b'\t' | b'\r' | b'\n' | 0 | b';' | b',') {
                self.i += 1;
            }
            let word = self.src[start..self.i].to_string();
            self.push(Tok::CmdArg(word), true);
        }
    }
}
