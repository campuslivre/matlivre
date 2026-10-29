use std::rc::Rc;

use crate::ast::AnonFn;
use crate::error::{rt, LangError};
use crate::nd::{self, Dims};

pub use crate::class::Object;
pub use crate::sym::SymArray;
pub use crate::table::Table;
pub use crate::time::TimeArray;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntClass {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

impl IntClass {
    pub const ALL: [IntClass; 8] = [IntClass::I8, IntClass::I16, IntClass::I32, IntClass::I64, IntClass::U8, IntClass::U16, IntClass::U32, IntClass::U64];

    pub fn name(self) -> &'static str {
        match self {
            IntClass::I8 => "int8",
            IntClass::I16 => "int16",
            IntClass::I32 => "int32",
            IntClass::I64 => "int64",
            IntClass::U8 => "uint8",
            IntClass::U16 => "uint16",
            IntClass::U32 => "uint32",
            IntClass::U64 => "uint64",
        }
    }

    pub fn from_name(s: &str) -> Option<IntClass> {
        IntClass::ALL.into_iter().find(|c| c.name() == s)
    }

    pub fn range(self) -> (f64, f64) {
        match self {
            IntClass::I8 => (i8::MIN as f64, i8::MAX as f64),
            IntClass::I16 => (i16::MIN as f64, i16::MAX as f64),
            IntClass::I32 => (i32::MIN as f64, i32::MAX as f64),
            IntClass::I64 => (i64::MIN as f64, i64::MAX as f64),
            IntClass::U8 => (0.0, u8::MAX as f64),
            IntClass::U16 => (0.0, u16::MAX as f64),
            IntClass::U32 => (0.0, u32::MAX as f64),
            IntClass::U64 => (0.0, u64::MAX as f64),
        }
    }

    pub fn bytes(self) -> usize {
        match self {
            IntClass::I8 | IntClass::U8 => 1,
            IntClass::I16 | IntClass::U16 => 2,
            IntClass::I32 | IntClass::U32 => 4,
            IntClass::I64 | IntClass::U64 => 8,
        }
    }
}

/// Element class of a numeric-like array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Double,
    Single,
    Logical,
    Char,
    Int(IntClass),
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Double => "double",
            Kind::Single => "single",
            Kind::Logical => "logical",
            Kind::Char => "char",
            Kind::Int(c) => c.name(),
        }
    }

    pub fn from_name(s: &str) -> Option<Kind> {
        Some(match s {
            "double" => Kind::Double,
            "single" => Kind::Single,
            "logical" => Kind::Logical,
            "char" => Kind::Char,
            other => Kind::Int(IntClass::from_name(other)?),
        })
    }

    pub fn is_int(self) -> bool {
        matches!(self, Kind::Int(_))
    }

    pub fn is_float(self) -> bool {
        matches!(self, Kind::Double | Kind::Single)
    }

    pub fn bytes(self) -> usize {
        match self {
            Kind::Double => 8,
            Kind::Single => 4,
            Kind::Logical => 1,
            Kind::Char => 2,
            Kind::Int(c) => c.bytes(),
        }
    }

    /// Converts a double to this class's value set (MATLAB conversion rules: integers round
    /// half away from zero and saturate, NaN becomes 0; single rounds to f32).
    #[inline]
    pub fn cast(self, v: f64) -> f64 {
        match self {
            Kind::Double | Kind::Char => v,
            Kind::Single => v as f32 as f64,
            Kind::Logical => {
                if v != 0.0 { 1.0 } else { 0.0 }
            }
            Kind::Int(c) => {
                if v.is_nan() {
                    return 0.0;
                }
                let (lo, hi) = c.range();
                v.round().clamp(lo, hi)
            }
        }
    }
}

/// An N-D numeric array (column-major), optionally complex. Logical and char arrays share
/// the representation and differ only in [`Kind`].
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    pub dims: Dims,
    pub data: Vec<f64>,
    /// Imaginary parts; `None` for real arrays.
    pub im: Option<Vec<f64>>,
    pub kind: Kind,
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<f64>) -> Self {
        debug_assert_eq!(rows * cols, data.len());
        Self { dims: vec![rows, cols], data, im: None, kind: Kind::Double }
    }

    pub fn from_dims(dims: Dims, data: Vec<f64>) -> Self {
        let dims = nd::normalize(dims);
        debug_assert_eq!(nd::numel(&dims), data.len());
        Self { dims, data, im: None, kind: Kind::Double }
    }

    pub fn complex(dims: Dims, re: Vec<f64>, im: Vec<f64>) -> Self {
        let mut m = Self::from_dims(dims, re);
        m.im = Some(im);
        m.narrow();
        m
    }

    pub fn with_kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }

    /// Converts every element to `kind`'s value set.
    pub fn cast_to(mut self, kind: Kind) -> Self {
        if kind != Kind::Double && kind != Kind::Char {
            for v in self.data.iter_mut() {
                *v = kind.cast(*v);
            }
            if let Some(im) = self.im.as_mut() {
                for v in im.iter_mut() {
                    *v = kind.cast(*v);
                }
            }
        }
        if kind == Kind::Logical {
            self.im = None;
        }
        self.kind = kind;
        self
    }

    pub fn scalar(v: f64) -> Self {
        Self::new(1, 1, vec![v])
    }

    pub fn complex_scalar(re: f64, im: f64) -> Self {
        Self::complex(vec![1, 1], vec![re], vec![im])
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

    pub fn filled_dims(dims: Dims, v: f64) -> Self {
        let n = nd::numel(&dims);
        Self::from_dims(dims, vec![v; n])
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

    pub fn rows(&self) -> usize {
        self.dims[0]
    }

    /// Columns of the 2-D unfolding (trailing dimensions folded in).
    pub fn cols(&self) -> usize {
        self.dims[1..].iter().product()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn is_scalar(&self) -> bool {
        self.data.len() == 1
    }

    pub fn is_2d(&self) -> bool {
        self.dims.len() == 2
    }

    pub fn is_vector(&self) -> bool {
        self.is_2d() && (self.dims[0] == 1 || self.dims[1] == 1) && !self.data.is_empty()
    }

    pub fn is_complex(&self) -> bool {
        self.im.is_some()
    }

    /// Drops an all-zero imaginary part (MATLAB narrows such results to real).
    pub fn narrow(&mut self) {
        if let Some(im) = &self.im {
            if im.iter().all(|&x| x == 0.0) {
                self.im = None;
            }
        }
    }

    pub fn im_at(&self, i: usize) -> f64 {
        self.im.as_ref().map(|v| v[i]).unwrap_or(0.0)
    }

    #[inline]
    pub fn get(&self, r: usize, c: usize) -> f64 {
        self.data[c * self.dims[0] + r]
    }

    #[inline]
    pub fn set(&mut self, r: usize, c: usize, v: f64) {
        let rows = self.dims[0];
        self.data[c * rows + r] = v;
    }

    /// `.'` (non-conjugate) transpose of a 2-D array.
    pub fn transpose(&self) -> Matrix {
        let (rows, cols) = (self.rows(), self.cols());
        let mut out = Vec::with_capacity(self.data.len());
        let mut out_im = self.im.as_ref().map(|_| Vec::with_capacity(self.data.len()));
        for r in 0..rows {
            for c in 0..cols {
                out.push(self.data[c * rows + r]);
                if let (Some(o), Some(im)) = (out_im.as_mut(), self.im.as_ref()) {
                    o.push(im[c * rows + r]);
                }
            }
        }
        Matrix { dims: vec![cols, rows], data: out, im: out_im, kind: self.kind }
    }

    pub fn to_string_lossy(&self) -> String {
        let (rows, cols) = (self.rows(), self.cols());
        let mut s = String::new();
        for r in 0..rows {
            for c in 0..cols {
                s.push(char::from_u32(self.data[c * rows + r] as u32).unwrap_or('?'));
            }
        }
        s
    }

    /// Applies `f` to the real parts, producing a real double array of the same shape.
    pub fn map(&self, f: impl Fn(f64) -> f64 + Sync + Send) -> Matrix {
        use rayon::prelude::*;
        let data = if self.data.len() >= crate::ops::PAR_MIN {
            self.data.par_iter().map(|&x| f(x)).collect()
        } else {
            self.data.iter().map(|&x| f(x)).collect()
        };
        Matrix::from_dims(self.dims.clone(), data)
    }

    pub fn reshaped(mut self, dims: Dims) -> Matrix {
        self.dims = nd::normalize(dims);
        self
    }

    /// Gathers the elements at `pos` into a new array of shape `dims`.
    pub fn gather(&self, pos: &[usize], dims: Dims) -> Matrix {
        let data = pos.iter().map(|&p| self.data[p]).collect();
        let im = self.im.as_ref().map(|im| pos.iter().map(|&p| im[p]).collect());
        let mut m = Matrix { dims: nd::normalize(dims), data, im, kind: self.kind };
        m.narrow();
        m
    }
}

/// N-D cell array.
#[derive(Debug, Clone, PartialEq)]
pub struct CellArray {
    pub dims: Dims,
    pub data: Vec<Value>,
}

impl CellArray {
    pub fn new(rows: usize, cols: usize, data: Vec<Value>) -> Self {
        Self { dims: vec![rows, cols], data }
    }

    pub fn from_dims(dims: Dims, data: Vec<Value>) -> Self {
        Self { dims: nd::normalize(dims), data }
    }

    pub fn empty(rows: usize, cols: usize) -> Self {
        Self { dims: vec![rows, cols], data: vec![Value::empty(); rows * cols] }
    }

    pub fn row(data: Vec<Value>) -> Self {
        Self { dims: vec![if data.is_empty() { 0 } else { 1 }, data.len()], data }
    }

    pub fn col(data: Vec<Value>) -> Self {
        Self { dims: vec![data.len(), if data.is_empty() { 0 } else { 1 }], data }
    }

    pub fn rows(&self) -> usize {
        self.dims[0]
    }

    pub fn cols(&self) -> usize {
        self.dims[1..].iter().product()
    }
}

/// N-D string array; `None` is `<missing>`.
#[derive(Debug, Clone, PartialEq)]
pub struct StrArray {
    pub dims: Dims,
    pub data: Vec<Option<String>>,
}

impl StrArray {
    pub fn scalar(s: impl Into<String>) -> Self {
        Self { dims: vec![1, 1], data: vec![Some(s.into())] }
    }

    pub fn from_dims(dims: Dims, data: Vec<Option<String>>) -> Self {
        Self { dims: nd::normalize(dims), data }
    }

    pub fn row(items: Vec<String>) -> Self {
        let n = items.len();
        Self { dims: vec![if n == 0 { 0 } else { 1 }, n], data: items.into_iter().map(Some).collect() }
    }

    pub fn col(items: Vec<String>) -> Self {
        let n = items.len();
        Self { dims: vec![n, if n == 0 { 0 } else { 1 }], data: items.into_iter().map(Some).collect() }
    }

    pub fn is_scalar(&self) -> bool {
        self.data.len() == 1
    }
}

/// N-D struct array; every element has the same fields (in `fields` order).
#[derive(Debug, Clone, PartialEq)]
pub struct StructArray {
    pub dims: Dims,
    pub fields: Vec<String>,
    pub elems: Vec<Vec<Value>>,
}

impl Default for StructArray {
    fn default() -> Self {
        Self::scalar()
    }
}

impl StructArray {
    /// A 1×1 struct with no fields.
    pub fn scalar() -> Self {
        Self { dims: vec![1, 1], fields: Vec::new(), elems: vec![Vec::new()] }
    }

    pub fn empty_with(fields: Vec<String>) -> Self {
        Self { dims: vec![0, 0], fields, elems: Vec::new() }
    }

    pub fn numel(&self) -> usize {
        self.elems.len()
    }

    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|f| f == name)
    }

    /// Field of element `i`.
    pub fn get_at(&self, i: usize, name: &str) -> Option<&Value> {
        self.field_index(name).and_then(|k| self.elems.get(i).map(|e| &e[k]))
    }

    /// Field of a scalar struct.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.get_at(0, name)
    }

    pub fn add_field(&mut self, name: &str) -> usize {
        match self.field_index(name) {
            Some(k) => k,
            None => {
                self.fields.push(name.to_string());
                for e in self.elems.iter_mut() {
                    e.push(Value::empty());
                }
                self.fields.len() - 1
            }
        }
    }

    pub fn set_at(&mut self, i: usize, name: &str, v: Value) {
        let k = self.add_field(name);
        self.elems[i][k] = v;
    }

    /// Sets a field of a scalar struct.
    pub fn set(&mut self, name: &str, v: Value) {
        self.set_at(0, name, v);
    }

    /// Element `i` as a scalar struct.
    pub fn element(&self, i: usize) -> StructArray {
        StructArray { dims: vec![1, 1], fields: self.fields.clone(), elems: vec![self.elems[i].clone()] }
    }

    pub fn empty_elem(&self) -> Vec<Value> {
        vec![Value::empty(); self.fields.len()]
    }

    pub fn remove_field(&mut self, name: &str) -> bool {
        match self.field_index(name) {
            Some(k) => {
                self.fields.remove(k);
                for e in self.elems.iter_mut() {
                    e.remove(k);
                }
                true
            }
            None => false,
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
    Str(StrArray),
    Cell(CellArray),
    Struct(StructArray),
    Func(FuncValue),
    Object(Object),
    Time(TimeArray),
    Table(Box<Table>),
    Sym(SymArray),
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

    pub fn string(s: impl Into<String>) -> Value {
        Value::Str(StrArray::scalar(s))
    }

    pub fn class_name(&self) -> String {
        match self {
            Value::Num(m) => m.kind.name().to_string(),
            Value::Str(_) => "string".into(),
            Value::Cell(_) => "cell".into(),
            Value::Struct(_) => "struct".into(),
            Value::Func(_) => "function_handle".into(),
            Value::Object(o) => o.class.name.clone(),
            Value::Time(t) => t.class_name().into(),
            Value::Table(_) => "table".into(),
            Value::Sym(s) if !s.args.is_empty() => "symfun".into(),
            Value::Sym(_) => "sym".into(),
        }
    }

    pub fn dims(&self) -> Dims {
        match self {
            Value::Num(m) => m.dims.clone(),
            Value::Str(s) => s.dims.clone(),
            Value::Cell(c) => c.dims.clone(),
            Value::Struct(s) => s.dims.clone(),
            Value::Time(t) => t.dims.clone(),
            Value::Table(t) => vec![t.nrows, t.vars.len()],
            Value::Sym(s) => s.dims.clone(),
            Value::Func(_) | Value::Object(_) => vec![1, 1],
        }
    }

    /// (rows, columns of the 2-D unfolding).
    pub fn dims2(&self) -> (usize, usize) {
        let d = self.dims();
        (d[0], d[1..].iter().product())
    }

    pub fn numel(&self) -> usize {
        nd::numel(&self.dims())
    }

    pub fn is_char(&self) -> bool {
        matches!(self, Value::Num(m) if m.kind == Kind::Char)
    }

    /// A char row vector or a string scalar.
    pub fn is_text(&self) -> bool {
        match self {
            Value::Num(m) => m.kind == Kind::Char && m.dims.len() == 2 && m.dims[0] <= 1,
            Value::Str(s) => s.is_scalar(),
            _ => false,
        }
    }

    /// Converts to a numeric matrix (strings are an error).
    pub fn to_matrix(&self) -> Result<Matrix, LangError> {
        match self {
            Value::Num(m) => Ok(m.clone()),
            other => rt(format!("Expected a numeric value but got a value of class '{}'.", other.class_name())),
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
            Value::Sym(s) if s.data.len() == 1 => s.data[0].to_f64().ok_or_else(|| LangError::new("Unable to convert expression containing symbolic variables into double array.", 0)),
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

    /// Text content of a char array (row by row) or a string scalar.
    pub fn text(&self) -> Result<String, LangError> {
        match self {
            Value::Num(m) if m.kind == Kind::Char || m.is_empty() => Ok(m.to_string_lossy()),
            Value::Str(s) if s.data.len() == 1 => match &s.data[0] {
                Some(t) => Ok(t.clone()),
                None => rt("Missing string values are not supported here."),
            },
            _ => rt(format!("Expected text but got a value of class '{}'.", self.class_name())),
        }
    }

    /// MATLAB truth value: nonempty and all elements nonzero (complex: real or imag nonzero).
    pub fn is_true(&self) -> Result<bool, LangError> {
        match self {
            Value::Num(m) => Ok(!m.is_empty() && (0..m.numel()).all(|i| (m.data[i] != 0.0 && !m.data[i].is_nan()) || m.im_at(i) != 0.0)),
            Value::Sym(s) => s.truth(),
            _ => rt(format!("Conversion to logical from {} is not possible.", self.class_name())),
        }
    }
}
