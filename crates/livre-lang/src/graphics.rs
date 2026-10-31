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
    /// Horizontal bars: `x` holds the bar positions (drawn on the y axis), `y` the lengths.
    BarH,
    /// Vertical lines and markers ±`err`.
    ErrorBar,
    /// A filled polygon through the points.
    Patch,
    /// `label` at (x[0], y[0]).
    Text,
    /// `xline` (vertical, at x[0]) or `yline` (`horizontal`, at y[0]), labelled with `label`.
    ConstantLine,
    /// `xregion` (between x[0] and x[1]) or `yregion` (`horizontal`, between y[0] and y[1]).
    Region,
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
    /// Graphics handle number (0 until the object is created through the interpreter).
    pub id: u64,
    /// Text of a `Text` object or the label of a constant line.
    pub label: String,
    /// Error bar lengths below and above each point.
    pub err: Option<(Vec<f64>, Vec<f64>)>,
    /// Orientation of constant lines and regions.
    pub horizontal: bool,
    /// Fill opacity of patches and regions.
    pub face_alpha: f32,
    /// Edge color of patches (`None` = no edge).
    pub edge_color: Option<[u8; 3]>,
    pub font_size: Option<f32>,
    pub visible: bool,
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
            line_width: 0.5,
            marker_size: 6.0,
            name: None,
            bar_width: None,
            id: 0,
            label: String::new(),
            err: None,
            horizontal: false,
            face_alpha: 1.0,
            edge_color: None,
            font_size: None,
            visible: true,
        }
    }
}

/// Tick labels: positions, custom labels, a format and the label rotation in degrees.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ticks {
    pub values: Option<Vec<f64>>,
    pub labels: Option<Vec<String>>,
    /// `xtickformat` format: a printf format such as `'%.2f'`, or `'usd'`, `'percentage'`, ...
    pub format: Option<String>,
    pub angle: f32,
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
    /// Graphics handle number (assigned when a handle to the axes is first made).
    pub id: u64,
    pub zlabel: String,
    pub zlim: Option<(f64, f64)>,
    pub xticks: Ticks,
    pub yticks: Ticks,
    pub xlog: bool,
    pub ylog: bool,
    /// Font size in points (`None` = the skin's default).
    pub font_size: Option<f32>,
    pub colorbar: bool,
    pub visible_axes: Option<bool>,
}

/// One unit of the default tick count: MATLAB-like automatic limits, rounded out to ticks.
pub fn nice_step(range: f64, target: f64) -> f64 {
    let raw = (range / target).max(1e-300);
    let mag = 10f64.powf(raw.log10().floor());
    let f = raw / mag;
    let n = if f < 1.5 { 1.0 } else if f < 3.0 { 2.0 } else if f < 7.0 { 5.0 } else { 10.0 };
    n * mag
}

impl Axes {
    /// Data extent of the plotted objects ((x0, x1), (y0, y1)), widened for bars and so on;
    /// limits set with `xlim`/`ylim` win.
    pub fn bounds(&self) -> ((f64, f64), (f64, f64)) {
        let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        let mut grow_x = |v: f64| {
            if v.is_finite() {
                x0 = x0.min(v);
                x1 = x1.max(v);
            }
        };
        let mut xs: Vec<f64> = Vec::new();
        let mut ys: Vec<f64> = Vec::new();
        for s in self.series.iter().filter(|s| s.visible) {
            match s.kind {
                // Constant lines and regions do not affect the limits.
                SeriesKind::ConstantLine | SeriesKind::Region | SeriesKind::Text => continue,
                SeriesKind::BarH => {
                    ys.extend(&s.x);
                    xs.extend(&s.y);
                    xs.push(0.0);
                    let dy = bar_spacing(&s.x);
                    if let (Some(lo), Some(hi)) = (s.x.iter().cloned().reduce(f64::min), s.x.iter().cloned().reduce(f64::max)) {
                        ys.extend([lo - dy * 0.6, hi + dy * 0.6]);
                    }
                    continue;
                }
                _ => {}
            }
            xs.extend(&s.x);
            ys.extend(&s.y);
            if let Some((lo, hi)) = &s.err {
                ys.extend(s.y.iter().zip(lo).map(|(y, e)| y - e));
                ys.extend(s.y.iter().zip(hi).map(|(y, e)| y + e));
            }
            if matches!(s.kind, SeriesKind::Bar | SeriesKind::Area | SeriesKind::Stem) {
                ys.push(0.0);
            }
            if s.kind == SeriesKind::Bar && !s.x.is_empty() {
                let dx = s.bar_width.map(|w| w / 0.8).unwrap_or_else(|| bar_spacing(&s.x));
                if let (Some(lo), Some(hi)) = (s.x.iter().cloned().reduce(f64::min), s.x.iter().cloned().reduce(f64::max)) {
                    xs.extend([lo - dx * 0.6, hi + dx * 0.6]);
                }
            }
        }
        xs.iter().for_each(|&v| grow_x(v));
        for &v in &ys {
            if v.is_finite() {
                y0 = y0.min(v);
                y1 = y1.max(v);
            }
        }
        if !x0.is_finite() {
            (x0, x1) = (0.0, 1.0);
        }
        if !y0.is_finite() {
            (y0, y1) = (0.0, 1.0);
        }
        if x0 == x1 {
            x0 -= 1.0;
            x1 += 1.0;
        }
        if y0 == y1 {
            let d = if y0 == 0.0 { 1.0 } else { y0.abs() * 0.1 };
            y0 -= d;
            y1 += d;
        }
        (self.xlim.unwrap_or((x0, x1)), self.ylim.unwrap_or((y0, y1)))
    }

    /// The limits MATLAB reports for `XLim`/`YLim`: set limits, or the data range rounded
    /// out to ticks.
    pub fn limits(&self) -> ((f64, f64), (f64, f64)) {
        let ((x0, x1), (y0, y1)) = self.bounds();
        let round = |lo: f64, hi: f64, set: bool| {
            if set {
                return (lo, hi);
            }
            let step = nice_step(hi - lo, 5.0);
            ((lo / step + 1e-9).floor() * step, (hi / step - 1e-9).ceil() * step)
        };
        (round(x0, x1, self.xlim.is_some()), round(y0, y1, self.ylim.is_some()))
    }
}

/// Smallest positive spacing between successive x values (1 when there is none).
pub fn bar_spacing(x: &[f64]) -> f64 {
    let mut d = f64::INFINITY;
    for w in x.windows(2) {
        let dx = (w[1] - w[0]).abs();
        if dx > 0.0 {
            d = d.min(dx);
        }
    }
    if d.is_finite() { d } else { 1.0 }
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

/// What a figure exporter renders: PNG bytes or SVG text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Svg,
}
