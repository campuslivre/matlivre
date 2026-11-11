//! A variable's contents for the app's variable viewer, and the preview the workspace
//! browser shows for arrays whose one-line summary is only their size.

use crate::display::{self, NumFormat};
use crate::nd;
use crate::value::{Kind, Matrix, Value};

/// The most rows, columns and elements of an array a view carries; larger arrays show their
/// top-left corner.
const MAX_ROWS: usize = 100_000;
const MAX_COLS: usize = 10_000;
const MAX_CELLS: usize = 4_000_000;
/// The most elements of a value shown as `disp` text.
const MAX_TEXT_ELEMS: usize = 20_000;
/// The rows and columns of a preview.
const PREVIEW_ROWS: usize = 8;
const PREVIEW_COLS: usize = 6;

/// A snapshot of one variable.
#[derive(Debug, Clone)]
pub struct VarView {
    pub name: String,
    pub class: String,
    /// The full size, as the workspace browser words it (`3×3`).
    pub size: String,
    /// The array is larger than the view carries: `body` is its top-left corner.
    pub truncated: bool,
    pub body: VarBody,
}

#[derive(Debug, Clone)]
pub enum VarBody {
    /// A 2-D numeric or logical array, or its top-left corner, with the session's format.
    Numbers { m: Matrix, format: NumFormat },
    /// One text per element (column-major): string and cell arrays, the rows of a char array,
    /// and the fields of a scalar struct (`row_names` then holds the field names).
    Cells { rows: usize, cols: usize, text: Vec<String>, row_names: Option<Vec<String>> },
    /// Anything else, as `disp` shows it.
    Text(String),
}

impl VarBody {
    /// Rows and columns of the grid; `None` for text.
    pub fn shape(&self) -> Option<(usize, usize)> {
        match self {
            VarBody::Numbers { m, .. } => Some((m.rows(), m.cols())),
            VarBody::Cells { rows, cols, .. } => Some((*rows, *cols)),
            VarBody::Text(_) => None,
        }
    }

    /// The text of one grid cell (0-based); empty outside the grid.
    pub fn cell(&self, r: usize, c: usize) -> String {
        match self {
            VarBody::Numbers { m, format } if r < m.rows() && c < m.cols() => display::fmt_elem(m, c * m.rows() + r, *format),
            VarBody::Cells { rows, cols, text, .. } if r < *rows && c < *cols => text[c * rows + r].clone(),
            _ => String::new(),
        }
    }

    /// Whether the cells hold numbers (which a grid aligns to the right).
    pub fn is_numeric(&self) -> bool {
        matches!(self, VarBody::Numbers { .. })
    }
}

/// The rows and columns of an array a view keeps.
fn shown(rows: usize, cols: usize) -> (usize, usize) {
    let c = cols.min(MAX_COLS);
    let r = rows.min(MAX_ROWS).min(MAX_CELLS / c.max(1));
    (r, c)
}

/// The top-left `rows`-by-`cols` corner of a 2-D array, as a full (not sparse) array.
fn corner(m: &Matrix, rows: usize, cols: usize) -> Matrix {
    let full = m.rows();
    let take = |src: &[f64]| -> Vec<f64> {
        if rows == full {
            src[..rows * cols].to_vec()
        } else {
            (0..cols).flat_map(|c| src[c * full..c * full + rows].iter().copied()).collect()
        }
    };
    let mut out = Matrix::new(rows, cols, take(&m.data));
    out.im = m.im.as_deref().map(take);
    out.kind = m.kind;
    out
}

fn grid_of(rows: usize, cols: usize, mut text_at: impl FnMut(usize) -> String) -> VarBody {
    let (r, c) = shown(rows, cols);
    let text = (0..c).flat_map(|j| (0..r).map(move |i| j * rows + i)).map(&mut text_at).collect();
    VarBody::Cells { rows: r, cols: c, text, row_names: None }
}

pub fn view(name: &str, v: &Value, f: NumFormat) -> VarView {
    let mut truncated = false;
    let body = match v {
        Value::Num(m) if m.is_2d() && m.kind == Kind::Char => {
            let rows = m.rows().min(MAX_ROWS);
            truncated = rows < m.rows();
            let text = (0..rows).map(|r| (0..m.cols()).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or('?')).collect()).collect();
            VarBody::Cells { rows, cols: usize::from(rows > 0), text, row_names: None }
        }
        Value::Num(m) if m.is_2d() => {
            let (r, c) = shown(m.rows(), m.cols());
            truncated = (r, c) != (m.rows(), m.cols());
            VarBody::Numbers { m: corner(m, r, c), format: f }
        }
        Value::Str(s) if s.dims.len() == 2 => {
            truncated = shown(s.dims[0], s.dims[1]) != (s.dims[0], s.dims[1]);
            grid_of(s.dims[0], s.dims[1], |i| match &s.data[i] {
                Some(t) => format!("\"{t}\""),
                None => "<missing>".into(),
            })
        }
        Value::Cell(c) if c.dims.len() == 2 => {
            truncated = shown(c.dims[0], c.dims[1]) != (c.dims[0], c.dims[1]);
            grid_of(c.dims[0], c.dims[1], |i| display::summary(&c.data[i], f))
        }
        Value::Struct(s) if s.numel() == 1 && !s.fields.is_empty() => {
            let text = s.elems[0].iter().map(|x| display::summary(x, f)).collect();
            VarBody::Cells { rows: s.fields.len(), cols: 1, text, row_names: Some(s.fields.clone()) }
        }
        Value::Sparse(c) if c.nnz() > MAX_TEXT_ELEMS => VarBody::Text(display::summary(v, f)),
        Value::Sparse(_) => VarBody::Text(display::disp_value(v, f)),
        _ if v.numel() > MAX_TEXT_ELEMS => VarBody::Text(display::summary(v, f)),
        _ => VarBody::Text(display::disp_value(v, f)),
    };
    VarView { name: name.to_string(), class: v.class_name(), size: nd::size_text(&v.dims()), truncated, body }
}

/// The top-left corner of a numeric matrix as `disp` shows it, for arrays whose summary is
/// only their size; `…` marks the rows and columns left out.
pub fn preview(v: &Value, f: NumFormat) -> Option<String> {
    let Value::Num(m) = v else {
        return None;
    };
    let in_summary = m.is_scalar() || (m.rows() == 1 && m.cols() <= 10 && !m.is_complex());
    if !m.is_2d() || m.kind == Kind::Char || m.is_empty() || in_summary {
        return None;
    }
    let (r, c) = (m.rows().min(PREVIEW_ROWS), m.cols().min(PREVIEW_COLS));
    let mut text = display::disp_value(&Value::Num(corner(m, r, c)), f);
    if c < m.cols() {
        // Every row of numbers, not the blank lines or a `1.0e+03 *` scale factor.
        text = text.lines().map(|l| if l.trim().is_empty() || l.ends_with('*') { l.to_string() } else { format!("{l}   …") }).collect::<Vec<_>>().join("\n");
    }
    if r < m.rows() {
        text.push_str("\n     …");
    }
    Some(text)
}
