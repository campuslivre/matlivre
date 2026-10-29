//! Figure data model. The interpreter only records what was plotted; the UI renders it so
//! the chart can follow the application skin.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
    DashDot,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Circle,
    Square,
    Diamond,
    Plus,
    Cross,
    Star,
    Point,
    TriangleUp,
    TriangleDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesKind {
    Line,
    Scatter,
    Bar,
    Stairs,
    Area,
    Stem,
}

/// How x values are labeled: plain numbers, wall-clock milliseconds (datetime) or
/// milliseconds (duration).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XAxisKind {
    #[default]
    Numeric,
    DateTime,
    Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub kind: SeriesKind,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    /// `None` picks the next color of the skin's palette.
    pub color: Option<[u8; 3]>,
    pub line_style: LineStyle,
    pub marker: Option<Marker>,
    pub line_width: f32,
    pub marker_size: f32,
    pub name: Option<String>,
    /// Bar width in x units (histograms); `None` = 0.8 of the spacing.
    pub bar_width: Option<f64>,
}

impl Series {
    pub fn new(kind: SeriesKind, x: Vec<f64>, y: Vec<f64>) -> Self {
        Self {
            kind,
            x,
            y,
            color: None,
            line_style: if kind == SeriesKind::Scatter { LineStyle::None } else { LineStyle::Solid },
            marker: match kind {
                SeriesKind::Scatter | SeriesKind::Stem => Some(Marker::Circle),
                _ => None,
            },
            line_width: 1.5,
            marker_size: 6.0,
            name: None,
            bar_width: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Axes {
    pub title: String,
    pub xlabel: String,
    pub ylabel: String,
    pub grid: bool,
    pub hold: bool,
    pub legend: Option<Vec<String>>,
    pub series: Vec<Series>,
    pub xlim: Option<(f64, f64)>,
    pub ylim: Option<(f64, f64)>,
    /// `DataAspectRatio` when its mode is manual (`daspect`, `axis equal`); None for auto.
    pub data_aspect: Option<[f64; 3]>,
    pub x_kind: XAxisKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Figure {
    pub id: u32,
    pub name: String,
    /// Subplot grid (rows, cols) and the axes, row-major.
    pub grid: (usize, usize),
    pub axes: Vec<Axes>,
    pub current_axes: usize,
}

impl Figure {
    pub fn new(id: u32) -> Self {
        Self { id, name: String::new(), grid: (1, 1), axes: vec![Axes::default()], current_axes: 0 }
    }

    pub fn axes_mut(&mut self) -> &mut Axes {
        &mut self.axes[self.current_axes]
    }
}

/// Parses a MATLAB line spec such as `'r--o'`. Returns `None` when `s` is not a line spec.
pub fn parse_line_spec(s: &str) -> Option<(Option<[u8; 3]>, Option<LineStyle>, Option<Marker>)> {
    let mut color = None;
    let mut style = None;
    let mut marker = None;
    let b = s.as_bytes();
    let mut i = 0;
    if s.is_empty() {
        return None;
    }
    while i < b.len() {
        let rest = &s[i..];
        if rest.starts_with("--") {
            style = Some(LineStyle::Dashed);
            i += 2;
            continue;
        }
        if rest.starts_with("-.") {
            style = Some(LineStyle::DashDot);
            i += 2;
            continue;
        }
        match b[i] {
            b'-' => style = Some(LineStyle::Solid),
            b':' => style = Some(LineStyle::Dotted),
            b'r' => color = Some([217, 48, 37]),
            b'g' => color = Some([16, 124, 16]),
            b'b' => color = Some([0, 95, 184]),
            b'c' => color = Some([0, 183, 195]),
            b'm' => color = Some([194, 57, 179]),
            b'y' => color = Some([234, 180, 0]),
            b'k' => color = Some([29, 29, 29]),
            b'w' => color = Some([255, 255, 255]),
            b'o' => marker = Some(Marker::Circle),
            b's' => marker = Some(Marker::Square),
            b'd' => marker = Some(Marker::Diamond),
            b'+' => marker = Some(Marker::Plus),
            b'x' => marker = Some(Marker::Cross),
            b'*' => marker = Some(Marker::Star),
            b'.' => marker = Some(Marker::Point),
            b'^' => marker = Some(Marker::TriangleUp),
            b'v' => marker = Some(Marker::TriangleDown),
            _ => return None,
        }
        i += 1;
    }
    Some((color, style, marker))
}

pub fn named_color(s: &str) -> Option<[u8; 3]> {
    Some(match s.to_ascii_lowercase().as_str() {
        "red" | "r" => [217, 48, 37],
        "green" | "g" => [16, 124, 16],
        "blue" | "b" => [0, 95, 184],
        "cyan" | "c" => [0, 183, 195],
        "magenta" | "m" => [194, 57, 179],
        "yellow" | "y" => [234, 180, 0],
        "black" | "k" => [29, 29, 29],
        "white" | "w" => [255, 255, 255],
        _ => return None,
    })
}
