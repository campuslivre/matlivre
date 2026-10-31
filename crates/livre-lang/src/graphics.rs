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
    /// View (azimuth, elevation) in degrees; `None` is the 2-D view (0, 90).
    pub view: Option<(f64, f64)>,
    /// Color limits (`clim`); `None` = the range of the color data.
    pub clim: Option<(f64, f64)>,
    /// The axes colormap (`colormap(ax, ...)`); `None` uses the figure's.
    pub colormap: Option<Vec<[f64; 3]>>,
    /// y increases downwards (images).
    pub y_reverse: bool,
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
        for s in self.series.iter().filter(|s| s.visible) {
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
}

impl Figure {
    pub fn new(id: u32) -> Self {
        Self { id, name: String::new(), grid: (1, 1), axes: vec![Axes::default()], current_axes: 0, colormap: None }
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
