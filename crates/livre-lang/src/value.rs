use std::rc::Rc;

use crate::ast::AnonFn;
use crate::error::{rt, LangError};

/// Element class of a numeric-like matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Double,
    Logical,
    Char,
}

/// A 2-D column-major array of doubles. Logical and char arrays share the representation
/// and differ only in [`Kind`].
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>,
    pub kind: Kind,
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<f64>) -> Self {
        debug_assert_eq!(rows * cols, data.len());
        Self { rows, cols, data, kind: Kind::Double }
    }

    pub fn with_kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }

    pub fn scalar(v: f64) -> Self {
        Self::new(1, 1, vec![v])
    }

    pub fn empty() -> Self {
        Self::new(0, 0, Vec::new())
    }

    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self::new(rows, cols, vec![0.0; rows * cols])
    }

    pub fn filled(rows: usize, cols: usize, v: f64) -> Self {
        Self::new(rows, cols, vec![v; rows * cols])
    }

    pub fn row(data: Vec<f64>) -> Self {
        Self::new(1, data.len(), data)
    }

    pub fn col(data: Vec<f64>) -> Self {
        Self::new(data.len(), 1, data)
    }

    pub fn chars(s: &str) -> Self {
        let data: Vec<f64> = s.chars().map(|c| c as u32 as f64).collect();
        Self::new(if data.is_empty() { 0 } else { 1 }, data.len(), data).with_kind(Kind::Char)
    }

    pub fn logical(rows: usize, cols: usize, data: Vec<f64>) -> Self {
        Self::new(rows, cols, data).with_kind(Kind::Logical)
    }

    pub fn numel(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn is_scalar(&self) -> bool {
        self.data.len() == 1
    }

    pub fn is_vector(&self) -> bool {
        (self.rows == 1 || self.cols == 1) && !self.data.is_empty()
    }

    #[inline]
    pub fn get(&self, r: usize, c: usize) -> f64 {
        self.data[c * self.rows + r]
    }

    #[inline]
    pub fn set(&mut self, r: usize, c: usize, v: f64) {
        self.data[c * self.rows + r] = v;
    }

    pub fn transpose(&self) -> Matrix {
        let mut out = Vec::with_capacity(self.data.len());
        for r in 0..self.rows {
            for c in 0..self.cols {
                out.push(self.get(r, c));
            }
        }
        Matrix { rows: self.cols, cols: self.rows, data: out, kind: self.kind }
    }

    pub fn to_string_lossy(&self) -> String {
        // Char matrices read row by row.
        let mut s = String::new();
        for r in 0..self.rows {
            for c in 0..self.cols {
                s.push(char::from_u32(self.get(r, c) as u32).unwrap_or('?'));
            }
        }
        s
    }

    pub fn map(&self, f: impl Fn(f64) -> f64) -> Matrix {
        Matrix::new(self.rows, self.cols, self.data.iter().map(|&x| f(x)).collect())
    }

    /// Resizes (growing with zeros) preserving existing elements at their (r, c) positions.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        if rows == self.rows && cols == self.cols {
            return;
        }
        let mut data = vec![0.0; rows * cols];
        for c in 0..self.cols.min(cols) {
            for r in 0..self.rows.min(rows) {
                data[c * rows + r] = self.get(r, c);
            }
        }
        self.rows = rows;
        self.cols = cols;
        self.data = data;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CellArray {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<Value>,
}

impl CellArray {
    pub fn new(rows: usize, cols: usize, data: Vec<Value>) -> Self {
        Self { rows, cols, data }
    }

    pub fn empty(rows: usize, cols: usize) -> Self {
        Self { rows, cols, data: vec![Value::empty(); rows * cols] }
    }

    pub fn row(data: Vec<Value>) -> Self {
        Self { rows: if data.is_empty() { 0 } else { 1 }, cols: data.len(), data }
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        if rows == self.rows && cols == self.cols {
            return;
        }
        let mut data = vec![Value::empty(); rows * cols];
        for c in 0..self.cols.min(cols) {
            for r in 0..self.rows.min(rows) {
                data[c * rows + r] = self.data[c * self.rows + r].clone();
            }
        }
        self.rows = rows;
        self.cols = cols;
        self.data = data;
    }
}

/// Scalar struct with fields in insertion order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Struct {
    pub fields: Vec<(String, Value)>,
}

impl Struct {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        self.fields.iter_mut().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    pub fn set(&mut self, name: &str, v: Value) {
        match self.get_mut(name) {
            Some(slot) => *slot = v,
            None => self.fields.push((name.to_string(), v)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FuncValue {
    Named(String),
    Anon { f: Rc<AnonFn>, captured: Rc<Vec<(String, Value)>> },
}

impl PartialEq for FuncValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (FuncValue::Named(a), FuncValue::Named(b)) => a == b,
            (FuncValue::Anon { f: a, .. }, FuncValue::Anon { f: b, .. }) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(Matrix),
    /// A `"string"` scalar.
    Str(String),
    Cell(CellArray),
    Struct(Struct),
    Func(FuncValue),
}

impl Value {
    pub fn num(v: f64) -> Value {
        Value::Num(Matrix::scalar(v))
    }

    pub fn bool(b: bool) -> Value {
        Value::Num(Matrix::logical(1, 1, vec![if b { 1.0 } else { 0.0 }]))
    }

    pub fn empty() -> Value {
        Value::Num(Matrix::empty())
    }

    pub fn chars(s: &str) -> Value {
        Value::Num(Matrix::chars(s))
    }

    pub fn class_name(&self) -> &'static str {
        match self {
            Value::Num(m) => match m.kind {
                Kind::Double => "double",
                Kind::Logical => "logical",
                Kind::Char => "char",
            },
            Value::Str(_) => "string",
            Value::Cell(_) => "cell",
            Value::Struct(_) => "struct",
            Value::Func(_) => "function_handle",
        }
    }

    pub fn dims(&self) -> (usize, usize) {
        match self {
            Value::Num(m) => (m.rows, m.cols),
            Value::Cell(c) => (c.rows, c.cols),
            _ => (1, 1),
        }
    }

    pub fn numel(&self) -> usize {
        let (r, c) = self.dims();
        r * c
    }

    pub fn is_char(&self) -> bool {
        matches!(self, Value::Num(m) if m.kind == Kind::Char)
    }

    pub fn is_text(&self) -> bool {
        self.is_char() || matches!(self, Value::Str(_))
    }

    /// Converts to a numeric matrix (strings are an error).
    pub fn to_matrix(&self) -> Result<Matrix, LangError> {
        match self {
            Value::Num(m) => Ok(m.clone()),
            Value::Str(_) => rt("Expected a numeric value but got a string."),
            Value::Cell(_) => rt("Expected a numeric value but got a cell array."),
            Value::Struct(_) => rt("Expected a numeric value but got a struct."),
            Value::Func(_) => rt("Expected a numeric value but got a function handle."),
        }
    }

    pub fn as_matrix(&self) -> Result<&Matrix, LangError> {
        match self {
            Value::Num(m) => Ok(m),
            _ => rt(format!("Expected a numeric value but got a value of class '{}'.", self.class_name())),
        }
    }

    pub fn scalar(&self) -> Result<f64, LangError> {
        match self {
            Value::Num(m) if m.numel() >= 1 => Ok(m.data[0]),
            Value::Num(_) => rt("Expected a scalar but got an empty array."),
            _ => rt(format!("Expected a numeric scalar but got a value of class '{}'.", self.class_name())),
        }
    }

    pub fn as_usize(&self) -> Result<usize, LangError> {
        let v = self.scalar()?;
        if v < 0.0 || v.fract() != 0.0 || !v.is_finite() {
            return rt("Size arguments must be nonnegative integers.");
        }
        Ok(v as usize)
    }

    /// Text content of a char row vector or string scalar.
    pub fn text(&self) -> Result<String, LangError> {
        match self {
            Value::Num(m) if m.kind == Kind::Char || m.is_empty() => Ok(m.to_string_lossy()),
            Value::Str(s) => Ok(s.clone()),
            _ => rt(format!("Expected text but got a value of class '{}'.", self.class_name())),
        }
    }

    /// MATLAB truth value: nonempty and all elements nonzero.
    pub fn is_true(&self) -> Result<bool, LangError> {
        match self {
            Value::Num(m) => Ok(!m.is_empty() && m.data.iter().all(|&x| x != 0.0 && !x.is_nan())),
            Value::Str(s) => Ok(!s.is_empty()),
            _ => rt(format!("Conversion to logical from {} is not possible.", self.class_name())),
        }
    }
}
