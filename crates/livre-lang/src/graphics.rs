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
    /// A grid of `grid.0` × `grid.1` points (x, y, z column-major), faces colored from `c`
    /// through the colormap (`surf`, `mesh`, `pcolor`).
    Surface,
    /// An image: `c` holds `grid.0` × `grid.1` × `channels` values; x and y are the centres
    /// of the first and last pixels.
    Image,
    /// Contour lines: NaN-separated polylines (x, y, and z for `contour3`) with the level of
    /// each point in `c`. Filled contours (`contourf`) keep their band polygons in u, v (x, y,
    /// NaN-separated) and w (the band value of each point).
    Contour,
    /// Arrows from (x, y, z) along (u, v, w).
    Quiver,
    /// `rectangle`: drawn as a patch through (x, y); `u` holds [x y w h] and `v` the curvature.
    Rectangle,
    /// A light (not drawn): position (x[0], y[0], z[0]); `label` is the style ('infinite' or
    /// 'local'). It shades the surfaces and patches of its axes.
    Light,
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
    /// `SizeData` of a scatter as given (marker area, points squared); `marker_size` is its root.
    pub size_data: Option<f64>,
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
    /// z coordinates (empty for 2-D objects).
    pub z: Vec<f64>,
    /// Color data mapped through the colormap (surfaces, images, contours).
    pub c: Vec<f64>,
    /// Rows and columns of a surface or image.
    pub grid: (usize, usize),
    /// Values per image pixel: 1 (colormapped) or 3 (RGB, 0..1).
    pub channels: usize,
    /// Image values index the colormap directly (`image`) instead of being scaled (`imagesc`).
    pub direct: bool,
    /// Surface faces and edges: colored through the colormap, a fixed color (`color` /
    /// `edge_color`), or not drawn.
    pub face_mapped: bool,
    pub face_none: bool,
    pub edge_mapped: bool,
    /// Quiver directions.
    pub u: Vec<f64>,
    pub v: Vec<f64>,
    pub w: Vec<f64>,
    /// The MATLAB class of the object when it differs from its kind's (`polarhistogram`).
    pub class_hint: Option<&'static str>,
    /// Marker diameter of each point in points (bubble charts); empty = `marker_size`.
    pub sizes: Vec<f32>,
    /// Offsets added to x and y where the points are drawn (`swarmchart` jitter).
    pub dx: Vec<f64>,
    pub dy: Vec<f64>,
    /// Plotted against the right y axis of `yyaxis` axes.
    pub right: bool,
    /// Text centered on its position (chart labels) rather than starting there.
    pub centered: bool,
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
            size_data: None,
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
            z: Vec::new(),
            c: Vec::new(),
            grid: (0, 0),
            channels: 1,
            direct: false,
            face_mapped: false,
            face_none: false,
            edge_mapped: false,
            u: Vec::new(),
            v: Vec::new(),
            w: Vec::new(),
            class_hint: None,
            sizes: Vec::new(),
            dx: Vec::new(),
            dy: Vec::new(),
            right: false,
            centered: false,
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
    /// The labels were set as a char matrix (`datetick`) and read back as one.
    pub as_char: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Axes {
    pub title: String,
    pub xlabel: String,
    pub ylabel: String,
    pub grid: bool,
    pub hold: bool,
    pub legend: Option<Vec<String>>,
    /// `Box`. In command-window mode a plotting command that creates the axes turns it on (the
    /// Live Editor's axes start with it off, as do axes made by `hold`, `gca` or `axes`).
    pub box_on: bool,
    pub series: Vec<Series>,
    pub xlim: Option<(f64, f64)>,
    pub ylim: Option<(f64, f64)>,
    /// `DataAspectRatio` when its mode is manual (`daspect`, `axis equal`); None for auto.
    pub data_aspect: Option<[f64; 3]>,
    pub x_kind: XAxisKind,
    /// Graphics handle number (assigned when a handle to the axes is first made).
    pub id: u64,
    pub zlabel: String,
    /// `SecondaryLabel` strings of the x, y and z rulers (`xsecondarylabel`, ...).
    pub secondary: [String; 3],
    pub zlim: Option<(f64, f64)>,
    pub xticks: Ticks,
    pub yticks: Ticks,
    pub xlog: bool,
    pub ylog: bool,
    /// Font size in points (`None` = the skin's default).
    pub font_size: Option<f32>,
    pub colorbar: bool,
    pub visible_axes: Option<bool>,
    /// View (azimuth, elevation) in degrees; `None` is the 2-D view (0, 90).
    pub view: Option<(f64, f64)>,
    /// Color limits (`clim`); `None` = the range of the color data.
    pub clim: Option<(f64, f64)>,
    /// The axes colormap (`colormap(ax, ...)`); `None` uses the figure's.
    pub colormap: Option<Vec<[f64; 3]>>,
    /// y increases downwards (images).
    pub y_reverse: bool,
    /// `colororder` colors for series without their own; `None` = the skin's palette.
    pub color_order: Option<Vec<[f64; 3]>>,
    /// `lighting none`: lights in the axes have no effect.
    pub lighting_none: bool,
    /// Polar axes (`polarplot`): series hold (theta in radians, r) in x and y; `xlim` and
    /// `xticks` are the theta limits and ticks in degrees, `ylim` and `yticks` those of r.
    pub polar: bool,
    /// Cartesian axes drawn with a polar grid (`compass`, `rose`, `polar`).
    pub polar_grid: bool,
    /// Theta increases clockwise (`ThetaDir`).
    pub theta_cw: bool,
    /// Where theta = 0 points, in degrees counter-clockwise from the right (`ThetaZeroLocation`).
    pub theta_zero: f64,
    /// Angle of the r tick labels in degrees (`RAxisLocation`); `None` is 80.
    pub r_axis_angle: Option<f64>,
    /// Two y axes (`yyaxis`). `ylim`, `yticks`, `ylabel` and `ylog` belong to the active side.
    pub yy: Option<Box<Yy>>,
    /// `plotyy`: the id of the axes these are drawn over (sharing its box, with the y axis
    /// on the right); 0 for ordinary axes.
    pub overlay_of: u64,
}

/// The y axis of one side of `yyaxis` axes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct YySide {
    pub ylim: Option<(f64, f64)>,
    pub yticks: Ticks,
    pub ylabel: String,
    pub ylog: bool,
}

/// `yyaxis` state: the active side, the inactive side's axis and the colors of both sides.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Yy {
    pub right_active: bool,
    pub other: YySide,
    pub colors: [[f64; 3]; 2],
}

/// Figure-level objects: a tiled layout or annotations (in normalized figure units, origin at
/// the bottom left).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FigObjKind {
    Layout,
    Line,
    Arrow,
    DoubleArrow,
    TextArrow,
    TextBox,
    Rectangle,
    Ellipse,
}

impl FigObjKind {
    pub fn class(self) -> &'static str {
        match self {
            FigObjKind::Layout => "matlab.graphics.layout.TiledChartLayout",
            FigObjKind::Line => "matlab.graphics.shape.Line",
            FigObjKind::Arrow => "matlab.graphics.shape.Arrow",
            FigObjKind::DoubleArrow => "matlab.graphics.shape.DoubleEndArrow",
            FigObjKind::TextArrow => "matlab.graphics.shape.TextArrow",
            FigObjKind::TextBox => "matlab.graphics.shape.TextBox",
            FigObjKind::Rectangle => "matlab.graphics.shape.Rectangle",
            FigObjKind::Ellipse => "matlab.graphics.shape.Ellipse",
        }
    }

    /// Annotations given by two points (X, Y) rather than a Position box.
    pub fn is_line(self) -> bool {
        matches!(self, FigObjKind::Line | FigObjKind::Arrow | FigObjKind::DoubleArrow | FigObjKind::TextArrow)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FigObject {
    pub id: u64,
    pub kind: FigObjKind,
    /// End points of line-like annotations.
    pub x: [f64; 2],
    pub y: [f64; 2],
    /// [left bottom width height] of box-like annotations.
    pub pos: [f64; 4],
    /// The text of text boxes and text arrows, or a layout's title.
    pub text: String,
    pub color: [u8; 3],
    pub face: Option<[u8; 3]>,
    pub edge: Option<[u8; 3]>,
    pub line_width: f32,
    pub font_size: f32,
    pub visible: bool,
}

impl FigObject {
    pub fn new(id: u64, kind: FigObjKind) -> Self {
        let line = kind.is_line() || kind == FigObjKind::Rectangle || kind == FigObjKind::Ellipse;
        FigObject {
            id,
            kind,
            x: [0.3, 0.4],
            y: [0.3, 0.4],
            pos: [0.3, 0.3, 0.1, 0.1],
            text: String::new(),
            color: [0, 0, 0],
            face: None,
            edge: if line || kind == FigObjKind::TextBox { Some([0, 0, 0]) } else { None },
            line_width: 0.5,
            font_size: 10.0,
            visible: true,
        }
    }
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
            // With two y axes only the active side's objects set the y limits.
            if self.yy.as_ref().is_some_and(|yy| yy.right_active != s.right) {
                xs.extend(&s.x);
                continue;
            }
            match s.kind {
                // Constant lines and regions do not affect the limits.
                SeriesKind::ConstantLine | SeriesKind::Region | SeriesKind::Text | SeriesKind::Light => continue,
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
            if s.kind == SeriesKind::Image {
                let (dx, dy) = image_pixel(s);
                if let (Some(&x0), Some(&x1), Some(&y0), Some(&y1)) = (s.x.first(), s.x.last(), s.y.first(), s.y.last()) {
                    xs.extend([x0.min(x1) - dx / 2.0, x0.max(x1) + dx / 2.0]);
                    ys.extend([y0.min(y1) - dy / 2.0, y0.max(y1) + dy / 2.0]);
                }
                continue;
            }
            if s.kind == SeriesKind::Quiver {
                xs.extend(s.x.iter().zip(&s.u).map(|(x, u)| x + u));
                ys.extend(s.y.iter().zip(&s.v).map(|(y, v)| y + v));
            }
            xs.extend(&s.x);
            ys.extend(&s.y);
            // Stacked bars reach y + dy.
            if s.kind == SeriesKind::Bar && !s.dy.is_empty() {
                ys.extend(s.y.iter().zip(&s.dy).map(|(y, d)| y + d));
            }
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
        // Bubbles (markers sized per point) get room around them.
        if self.series.iter().any(|s| s.visible && !s.sizes.is_empty()) {
            let (dx, dy) = ((x1 - x0) * 0.1, (y1 - y0) * 0.1);
            (x0, x1, y0, y1) = (x0 - dx, x1 + dx, y0 - dy, y1 + dy);
        }
        (self.xlim.unwrap_or((x0, x1)), self.ylim.unwrap_or((y0, y1)))
    }

    /// The axes as seen from one side of `yyaxis` axes: that side's y axis is the active one.
    pub fn side(&self, right: bool) -> Axes {
        let mut a = self.clone();
        if let Some(yy) = a.yy.as_mut() {
            if yy.right_active != right {
                yy.right_active = right;
                let other = YySide { ylim: a.ylim, yticks: a.yticks.clone(), ylabel: a.ylabel.clone(), ylog: a.ylog };
                let side = std::mem::replace(&mut yy.other, other);
                (a.ylim, a.yticks, a.ylabel, a.ylog) = (side.ylim, side.yticks, side.ylabel, side.ylog);
            }
        }
        a
    }

    /// Makes the other side of `yyaxis` axes active (swapping the y axis settings).
    pub fn switch_side(&mut self, right: bool) {
        *self = self.side(right);
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

/// Pixel width and height of an image in data units.
pub fn image_pixel(s: &Series) -> (f64, f64) {
    let (rows, cols) = s.grid;
    let step = |v: &[f64], n: usize| match (v.first(), v.last()) {
        (Some(a), Some(b)) if n > 1 && a != b => (b - a).abs() / (n - 1) as f64,
        _ => 1.0,
    };
    (step(&s.x, cols), step(&s.y, rows))
}

impl Axes {
    /// Whether the axes show a 3-D view.
    pub fn is_3d(&self) -> bool {
        matches!(self.view, Some((az, el)) if !(az == 0.0 && el == 90.0))
    }

    /// Extent of the z data ((-1, 1) without any); `zlim` wins.
    pub fn zbounds(&self) -> (f64, f64) {
        if let Some(l) = self.zlim {
            return l;
        }
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for s in self.series.iter().filter(|s| s.visible && s.kind != SeriesKind::Light) {
            let tips: Vec<f64> = s.z.iter().zip(&s.w).map(|(z, w)| z + w).collect();
            for &z in s.z.iter().chain(&tips) {
                if z.is_finite() {
                    lo = lo.min(z);
                    hi = hi.max(z);
                }
            }
        }
        if !lo.is_finite() {
            return (-1.0, 1.0);
        }
        if lo == hi {
            return (lo - 1.0, hi + 1.0);
        }
        (lo, hi)
    }

    /// Color limits: `clim`, or the range of the scaled color data.
    pub fn color_limits(&self) -> (f64, f64) {
        if let Some(l) = self.clim {
            return l;
        }
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for s in self.series.iter().filter(|s| s.visible && !(s.kind == SeriesKind::Image && (s.direct || s.channels == 3))) {
            for &c in &s.c {
                if c.is_finite() {
                    lo = lo.min(c);
                    hi = hi.max(c);
                }
            }
        }
        if !lo.is_finite() {
            return (0.0, 1.0);
        }
        if lo == hi {
            return (lo - 1.0, hi + 1.0);
        }
        (lo, hi)
    }
}

/// A MATLAB colormap by name with `n` colors (`None` for unknown names).
pub fn colormap(name: &str, n: usize) -> Option<Vec<[f64; 3]>> {
    let name = name.to_ascii_lowercase();
    let lin = |i: usize| if n > 1 { i as f64 / (n - 1) as f64 } else { 0.0 };
    let gray: Vec<f64> = (0..n).map(lin).collect();
    let hot = || {
        let n1 = (3 * n / 8).max(1);
        (0..n)
            .map(|i| {
                let r = if i < n1 { (i + 1) as f64 / n1 as f64 } else { 1.0 };
                let g = if i < n1 { 0.0 } else if i < 2 * n1 { (i - n1 + 1) as f64 / n1 as f64 } else { 1.0 };
                let b = if i < 2 * n1 { 0.0 } else { (i - 2 * n1 + 1) as f64 / (n - 2 * n1) as f64 };
                [r, g, b]
            })
            .collect::<Vec<_>>()
    };
    Some(match name.as_str() {
        "parula" | "turbo" | "sky" | "abyss" => {
            let line = include_str!("colormaps.txt").lines().find(|l| l.split(' ').next() == Some(name.as_str()))?;
            let v: Vec<f64> = line.split(' ').skip(1).filter_map(|t| t.parse().ok()).collect();
            (0..n)
                .map(|i| {
                    let p = if n > 1 { i as f64 * 255.0 / (n - 1) as f64 } else { 0.0 };
                    let a = p.floor() as usize;
                    let b = (a + 1).min(255);
                    let f = p - a as f64;
                    [0, 1, 2].map(|k| v[3 * a + k] * (1.0 - f) + v[3 * b + k] * f)
                })
                .collect()
        }
        "gray" | "grey" => gray.iter().map(|&g| [g, g, g]).collect(),
        "cool" => gray.iter().map(|&g| [g, 1.0 - g, 1.0]).collect(),
        "spring" => gray.iter().map(|&g| [1.0, g, 1.0 - g]).collect(),
        "summer" => gray.iter().map(|&g| [g, 0.5 + g / 2.0, 0.4]).collect(),
        "autumn" => gray.iter().map(|&g| [1.0, g, 0.0]).collect(),
        "winter" => gray.iter().map(|&g| [0.0, g, 0.5 + (1.0 - g) / 2.0]).collect(),
        "copper" => gray.iter().map(|&g| [(g * 1.25).min(1.0), (g * 0.7812).min(1.0), (g * 0.4975).min(1.0)]).collect(),
        "white" => vec![[1.0; 3]; n],
        "colorcube" => colorcube(n),
        "hot" => hot(),
        "bone" => {
            let h = hot();
            gray.iter().zip(&h).map(|(&g, h)| [(7.0 * g + h[2]) / 8.0, (7.0 * g + h[1]) / 8.0, (7.0 * g + h[0]) / 8.0]).collect()
        }
        "pink" => {
            let h = hot();
            gray.iter().zip(&h).map(|(&g, h)| [0, 1, 2].map(|k| ((2.0 * g + h[k]) / 3.0).sqrt())).collect()
        }
        "jet" => {
            let q = n.div_ceil(4);
            let mut u: Vec<f64> = (1..=q).map(|i| i as f64 / q as f64).collect();
            u.extend(std::iter::repeat(1.0).take(q.saturating_sub(1)));
            u.extend((1..=q).rev().map(|i| i as f64 / q as f64));
            let g0 = q.div_ceil(2) as i64 - if n % 4 == 1 { 1 } else { 0 };
            let mut out = vec![[0.0; 3]; n];
            for (k, &val) in u.iter().enumerate() {
                let g = g0 + k as i64 + 1;
                for (ch, row) in [(0usize, g + q as i64), (1, g), (2, g - q as i64)] {
                    if row >= 1 && row as usize <= n {
                        out[row as usize - 1][ch] = val;
                    }
                }
            }
            out
        }
        "hsv" => (0..n).map(|i| hsv_to_rgb(i as f64 / n as f64, 1.0, 1.0)).collect(),
        "lines" => (0..n).map(|i| crate::gfx::COLOR_ORDER[i % 7]).collect(),
        "prism" => {
            let p = [[1.0, 0.0, 0.0], [1.0, 0.5, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [2.0 / 3.0, 0.0, 1.0]];
            (0..n).map(|i| p[i % 6]).collect()
        }
        "flag" => {
            let p = [[1.0, 0.0, 0.0], [1.0, 1.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0]];
            (0..n).map(|i| p[i % 4]).collect()
        }
        _ => return None,
    })
}

/// An RGB cube without its grays and pure primaries, then red, green and blue ramps and a gray
/// ramp filling the rest. The cube has k levels for the largest k with k³ < n (k = 2 at n = 8);
/// at n = 27, 64, 125 and 216 MATLAB uses k = ∛n with one level fewer in blue.
fn colorcube(n: usize) -> Vec<[f64; 3]> {
    let root = (1..).find(|k: &usize| k.pow(3) >= n).unwrap_or(1);
    let (a, b) = match n {
        8 => (2, 2),
        27 | 64 | 125 | 216 => (root, root - 1),
        _ => {
            let k = (1..).take_while(|k: &usize| k.pow(3) < n).last().unwrap_or(0);
            (k, k)
        }
    };
    if a < 2 {
        return (0..n).map(|i| [if n > 1 { i as f64 / (n - 1) as f64 } else { 0.0 }; 3]).collect();
    }
    let lv = |i: usize, k: usize| i as f64 / (k - 1) as f64;
    let mut out = Vec::with_capacity(n);
    for bi in 0..b {
        for ri in 0..a {
            for gi in 0..a {
                let c = [lv(ri, a), lv(gi, a), lv(bi, b)];
                let nonzero = c.iter().filter(|&&x| x > 0.0).count();
                if nonzero > 1 && !(c[0] == c[1] && c[1] == c[2]) {
                    out.push(c);
                }
            }
        }
    }
    let rest = n.saturating_sub(out.len());
    let s = rest.saturating_sub(1) / 4;
    for ch in 0..3 {
        for i in 1..=s {
            let mut c = [0.0; 3];
            c[ch] = i as f64 / s as f64;
            out.push(c);
        }
    }
    let g = rest - 3 * s;
    for i in 0..g {
        out.push([if g > 1 { i as f64 / (g - 1) as f64 } else { 0.0 }; 3]);
    }
    out
}

pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> [f64; 3] {
    let h6 = (h * 6.0).rem_euclid(6.0);
    let k = h6.floor();
    let f = h6 - k;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match k as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// The color for value `v` under limits `lim` (clamped to the ends of `map`).
pub fn map_color(map: &[[f64; 3]], lim: (f64, f64), v: f64) -> Option<[f64; 3]> {
    if !v.is_finite() || map.is_empty() {
        return None;
    }
    let n = map.len();
    let f = if lim.1 > lim.0 { (v - lim.0) / (lim.1 - lim.0) } else { 0.5 };
    let i = ((f * n as f64).floor() as i64).clamp(0, n as i64 - 1) as usize;
    Some(map[i])
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
    /// The figure colormap (`None` = parula).
    pub colormap: Option<Vec<[f64; 3]>>,
    /// `tiledlayout('flow')`: the grid grows to fit the tiles.
    pub flow: bool,
    /// Tiled layout and annotations.
    pub objects: Vec<FigObject>,
    /// The tile `nexttile` fills next (0-based).
    pub next_tile: usize,
    /// `colororder` of the figure: used by axes without their own.
    pub color_order: Option<Vec<[f64; 3]>>,
    /// False for a new figure: MATLAB makes the first axes only when something needs them
    /// (`plot`, `gca`, `axes`, `subplot`), so `Children` is empty until then.
    pub axes_made: bool,
}

impl Figure {
    pub fn new(id: u32) -> Self {
        Self { id, name: String::new(), grid: (1, 1), axes: vec![Axes::default()], current_axes: 0, colormap: None, flow: false, objects: Vec::new(), next_tile: 0, color_order: None, axes_made: false }
    }

    pub fn axes_mut(&mut self) -> &mut Axes {
        self.axes_made = true;
        &mut self.axes[self.current_axes]
    }

    /// Whether the figure has axes yet (a grid of several always does).
    pub fn has_axes(&self) -> bool {
        self.axes_made || self.axes.len() != 1
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

/// The outline of `rectangle` with Position [x y w h] and Curvature [cx cy] (fractions of the
/// width and height rounded off at the corners), counter-clockwise from the bottom left.
pub fn rect_polygon(pos: &[f64], curv: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let (x, y, w, h) = (pos[0], pos[1], pos[2], pos[3]);
    let cx = curv.first().copied().unwrap_or(0.0).clamp(0.0, 1.0);
    let cy = curv.get(1).copied().unwrap_or(cx).clamp(0.0, 1.0);
    if cx == 0.0 || cy == 0.0 {
        return (vec![x, x + w, x + w, x, x], vec![y, y, y + h, y + h, y]);
    }
    let (rx, ry) = (cx * w / 2.0, cy * h / 2.0);
    let corners = [(x + w - rx, y + ry, -90.0), (x + w - rx, y + h - ry, 0.0), (x + rx, y + h - ry, 90.0), (x + rx, y + ry, 180.0)];
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for (ox, oy, start) in corners {
        for k in 0..=12 {
            let t = (start + 90.0 * k as f64 / 12.0_f64).to_radians();
            xs.push(ox + rx * t.cos());
            ys.push(oy + ry * t.sin());
        }
    }
    xs.push(xs[0]);
    ys.push(ys[0]);
    (xs, ys)
}

impl Axes {
    /// A point of a series as (theta in radians, r): polar axes store it so, polar-grid axes
    /// store Cartesian x and y.
    pub fn polar_point(&self, x: f64, y: f64) -> (f64, f64) {
        if self.polar { (x, y) } else { (y.atan2(x), x.hypot(y)) }
    }

    /// Theta limits in degrees and r limits. Automatic r runs from 0 to the largest radius
    /// rounded up to a tick.
    pub fn polar_limits(&self) -> ((f64, f64), (f64, f64)) {
        let theta = if self.polar { self.xlim.unwrap_or((0.0, 360.0)) } else { (0.0, 360.0) };
        if let (true, Some(r)) = (self.polar, self.ylim) {
            return (theta, r);
        }
        let mut top = 0.0f64;
        for s in self.series.iter().filter(|s| s.visible && !matches!(s.kind, SeriesKind::Light | SeriesKind::Text)) {
            for (&x, &y) in s.x.iter().zip(&s.y) {
                let r = self.polar_point(x, y).1;
                if r.is_finite() {
                    top = top.max(r);
                }
            }
        }
        if top <= 0.0 {
            return (theta, (0.0, 1.0));
        }
        let step = nice_step(top, 4.0);
        (theta, (0.0, (top / step - 1e-9).ceil() * step))
    }

    /// r tick values: set ones, or nice steps across the r limits.
    pub fn polar_r_ticks(&self) -> Vec<f64> {
        if let (true, Some(v)) = (self.polar, &self.yticks.values) {
            return v.clone();
        }
        let (_, (r0, r1)) = self.polar_limits();
        let step = nice_step(r1 - r0, 4.0);
        let n = ((r1 - r0) / step + 1e-9).floor() as usize;
        (0..=n.min(40)).map(|k| r0 + k as f64 * step).collect()
    }

    /// Theta tick values in degrees: set ones, or every 30 degrees within the limits.
    pub fn polar_theta_ticks(&self) -> Vec<f64> {
        if let (true, Some(v)) = (self.polar, &self.xticks.values) {
            return v.clone();
        }
        let ((t0, t1), _) = self.polar_limits();
        let first = (t0 / 30.0).ceil() as i64;
        let last = (t1 / 30.0 + 1e-9).floor() as i64;
        (first..=last).map(|k| k as f64 * 30.0).collect()
    }

    /// Theta tick labels (`30°`); a full circle labels 0 and not 360.
    pub fn polar_theta_labels(&self) -> Vec<String> {
        if let (true, Some(l)) = (self.polar, &self.xticks.labels) {
            return l.clone();
        }
        let ((t0, t1), _) = self.polar_limits();
        let full = t1 - t0 >= 360.0 - 1e-9;
        self.polar_theta_ticks()
            .into_iter()
            .filter(|&v| !(full && (v - t1).abs() < 1e-9))
            .map(|v| match &self.xticks.format {
                Some(f) => crate::gfx::format_tick(Some(f), v),
                None => format!("{}°", crate::display::fmt_g(v, 5)),
            })
            .collect()
    }

    /// r tick labels.
    pub fn polar_r_labels(&self) -> Vec<String> {
        if let (true, Some(l)) = (self.polar, &self.yticks.labels) {
            return l.clone();
        }
        self.polar_r_ticks().into_iter().map(|v| crate::gfx::format_tick(self.yticks.format.as_deref(), v)).collect()
    }
}
