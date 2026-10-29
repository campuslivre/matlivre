//! The Windows 11 style colour icons on a 16 px grid (paths from the design's `icon()`),
//! rasterised with resvg at the screen's pixel density and cached as textures. Outline icons
//! use the text colour; disabled icons are greyscaled at 45 % opacity.

use std::collections::HashMap;
use std::sync::Mutex;

use eframe::egui::{self, pos2, Color32, ColorImage, Rect, TextureHandle, TextureOptions};

use crate::theme::Skin;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Ic {
    New,
    Open,
    Save,
    SaveAs,
    ExportM,
    Pdf,
    Add,
    Run,
    RunSmall,
    RunAll,
    Advance,
    Stop,
    Clear,
    Workspace,
    FilesPanel,
    CmdPanel,
    Folder,
    FolderOpen,
    Control,
    Slider,
    Dropdown,
    Checkbox,
    EditField,
    ButtonCtl,
    CopyImg,
    SaveImg,
    Check,
    MoveUp,
    MoveDown,
    Delete,
    Drive,
    Pc,
    Text,
    Code,
    Title,
    Heading,
    Subheading,
    SectionBreak,
    ClearOutput,
    Theme,
    Help,
    Mlx,
    MFile,
    Csv,
    Md,
    File,
    BrazilFlag,
}

const G: &str = "#13A10E";
const B: &str = "#0F6CBD";

fn f(d: &str, fill: &str) -> String {
    format!(r#"<path d="{d}" fill="{fill}"/>"#)
}

/// A filled path with the 0.9 px grey outline the white shapes use.
fn fo(d: &str, fill: &str, w: f32) -> String {
    format!(r##"<path d="{d}" fill="{fill}" stroke="#8A8A8A" stroke-width="{w}" stroke-linejoin="round"/>"##)
}

fn l(d: &str, stroke: &str, w: f32) -> String {
    format!(r#"<path d="{d}" fill="none" stroke="{stroke}" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round"/>"#)
}

fn circle(cx: f32, cy: f32, r: f32, fill: &str) -> String {
    format!(r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{fill}"/>"#)
}

fn page() -> String {
    let x = 2.5;
    fo(&format!("M{x} 2A1 1 0 0 1 {} 1H{}L{} 4.5V14A1 1 0 0 1 {} 15H{}A1 1 0 0 1 {x} 14Z", x + 1.0, x + 6.5, x + 10.0, x + 9.0, x + 1.0), "#FFFFFF", 0.9)
        + &fo(&format!("M{} 1V3.5A1 1 0 0 0 {} 4.5H{}", x + 6.5, x + 7.5, x + 10.0), "#E0E0E0", 0.9)
}

fn play(x: f32, y: f32, s: f32, c: &str) -> String {
    let tip = x + s * 0.95;
    let mid = y + s / 2.0;
    f(
        &format!(
            "M{x} {}C{x} {} {} {} {} {}L{tip} {}C{} {} {} {} {tip} {}L{} {}C{} {} {x} {} {x} {}Z",
            y + 0.9,
            y + 0.15,
            x + 0.8,
            y - 0.3,
            x + 1.45,
            y + 0.1,
            mid - 0.45,
            tip + 0.6,
            mid - 0.1,
            tip + 0.6,
            mid + 0.6,
            mid + 0.95,
            x + 1.45,
            y + s - 0.1,
            x + 0.8,
            y + s + 0.3,
            y + s - 0.15,
            y + s - 0.9
        ),
        c,
    )
}

fn frame_rect() -> String {
    "M1.5 3A1.5 1.5 0 0 1 3 1.5H13A1.5 1.5 0 0 1 14.5 3V13A1.5 1.5 0 0 1 13 14.5H3A1.5 1.5 0 0 1 1.5 13Z".into()
}

/// The icon's SVG content in a 16 × 16 view box. `bg` stands for `--bg` and `fg` for
/// `currentColor`.
fn body(ic: Ic, bg: &str, fg: &str) -> String {
    let folder = f("M1 3.5A1.5 1.5 0 0 1 2.5 2H5.6L7.2 3.6H13.5A1.5 1.5 0 0 1 15 5.1V12.5A1.5 1.5 0 0 1 13.5 14H2.5A1.5 1.5 0 0 1 1 12.5Z", "#E8A317")
        + &f("M1 6.2A1.2 1.2 0 0 1 2.2 5H13.8A1.2 1.2 0 0 1 15 6.2V12.5A1.5 1.5 0 0 1 13.5 14H2.5A1.5 1.5 0 0 1 1 12.5Z", "#FFC83D");
    let panel = |divider: &str| {
        format!(r#"<rect x="1.5" y="2.5" width="13" height="11" rx="1.5" fill="none" stroke="{fg}" stroke-width="1"/>"#) + &l(divider, fg, 1.0)
    };
    match ic {
        Ic::New => {
            page()
                + &format!(r#"<circle cx="11.8" cy="11.8" r="3.7" fill="{B}" stroke="{bg}" stroke-width="1"/>"#)
                + &l("M11.8 10V13.6M10 11.8H13.6", "#FFFFFF", 1.2)
        }
        Ic::Open => {
            f("M1 3.5A1.5 1.5 0 0 1 2.5 2H5.6L7.2 3.6H12.5A1.5 1.5 0 0 1 14 5.1V7H1Z", "#E8A317")
                + &f("M2.6 7H15.2A.8.8 0 0 1 15.9 8.1L13.9 13.2A1.3 1.3 0 0 1 12.7 14H2.2A1.2 1.2 0 0 1 1 12.8V8.5A1.5 1.5 0 0 1 2.6 7Z", "#FFC83D")
        }
        Ic::FolderOpen => {
            f("M1 3.5A1.5 1.5 0 0 1 2.5 2H5.6L7.2 3.6H12.5A1.5 1.5 0 0 1 14 5.1V7H1Z", "#E8A317")
                + &f("M2.6 7H15.2A.8.8 0 0 1 15.9 8.1L13.9 13.2A1.3 1.3 0 0 1 12.7 14H2.2A1.2 1.2 0 0 1 1 12.8V8.5A1.5 1.5 0 0 1 2.6 7Z", "#FFC83D")
        }
        Ic::Save => {
            f("M1.5 2.8A1.3 1.3 0 0 1 2.8 1.5H11.6L14.5 4.4V13.2A1.3 1.3 0 0 1 13.2 14.5H2.8A1.3 1.3 0 0 1 1.5 13.2Z", B)
                + &f("M4.3 1.5H10.7V5A.6.6 0 0 1 10.1 5.6H4.9A.6.6 0 0 1 4.3 5Z", "#B4D6FA")
                + &f("M8.4 2.4H9.6V4.6H8.4Z", B)
                + &f("M3.6 9.2A.8.8 0 0 1 4.4 8.4H11.6A.8.8 0 0 1 12.4 9.2V14.5H3.6Z", "#FFFFFF")
        }
        Ic::SaveAs => {
            f("M1.5 2.8A1.3 1.3 0 0 1 2.8 1.5H11.6L14.5 4.4V8L8 14.5H2.8A1.3 1.3 0 0 1 1.5 13.2Z", B)
                + &f("M4.3 1.5H10.7V5A.6.6 0 0 1 10.1 5.6H4.9A.6.6 0 0 1 4.3 5Z", "#B4D6FA")
                + &f("M3.6 9.2A.8.8 0 0 1 4.4 8.4H10.5L6.5 12.4V14.5H3.6Z", "#FFFFFF")
                + &f("M13.6 8.6L15.4 10.4L10.6 15.2H8.8V13.4Z", "#E8A317")
        }
        Ic::ExportM => page() + &l("M6 8L4.6 9.8L6 11.6", "#8764B8", 1.2) + &f("M9 9.2H11.5V7.5L15 10.3L11.5 13.1V11.4H9Z", G),
        Ic::Pdf => {
            page()
                + &f("M3.5 8.5H11.5V12.5H3.5Z", "#D13438")
                + &l("M5 9.6V11.4M5 9.6H5.8A.5.5 0 0 1 5.8 10.6H5M7.3 9.6V11.4H7.8A.9.9 0 0 0 7.8 9.6ZM9.6 11.4V9.6H10.6M9.6 10.5H10.3", "#FFFFFF", 0.6)
        }
        Ic::Add => f(&frame_rect(), B) + &l("M8 4.5V11.5M4.5 8H11.5", "#FFFFFF", 1.4),
        Ic::Run => play(2.5, 1.5, 13.0, G),
        Ic::RunSmall => play(3.5, 2.5, 11.0, G),
        Ic::RunAll => play(0.5, 2.0, 12.0, G) + &play(5.0, 2.0, 12.0, "#3FC43A"),
        Ic::Advance => play(1.0, 1.0, 11.0, G) + &f("M12.5 7.5V11.2H10.8L13.4 14.6L16 11.2H14.3V7.5Z", B),
        Ic::Stop => f("M2 4A2 2 0 0 1 4 2H12A2 2 0 0 1 14 4V12A2 2 0 0 1 12 14H4A2 2 0 0 1 2 12Z", "#D13438"),
        Ic::Clear => {
            f("M2.2 9.6L8.6 3.2A1.4 1.4 0 0 1 10.6 3.2L13.6 6.2A1.4 1.4 0 0 1 13.6 8.2L9.2 12.6H5.2Z", "#F1707B")
                + &fo("M2.2 9.6L5.6 6.2L9.6 10.2L7.2 12.6H5.2Z", "#FFFFFF", 0.8)
                + &l("M5 14.5H14", "#8A8A8A", 1.0)
        }
        Ic::Workspace => panel("M10 2.5V13.5"),
        Ic::FilesPanel => panel("M6 2.5V13.5"),
        Ic::CmdPanel => panel("M1.5 9.5H14.5"),
        Ic::Folder => folder,
        Ic::Control => {
            fo(&frame_rect(), "#FFFFFF", 0.9)
                + &l("M4 5.5H12M4 10.5H12", "#B0B0B0", 1.2)
                + &l("M4 5.5H8", B, 1.2)
                + &l("M4 10.5H6", B, 1.2)
                + &circle(8.5, 5.5, 1.6, B)
                + &circle(6.5, 10.5, 1.6, B)
        }
        Ic::Slider => {
            l("M1.5 8H14.5", "#B0B0B0", 2.0)
                + &l("M1.5 8H9", B, 2.0)
                + r##"<circle cx="9.5" cy="8" r="3.2" fill="#FFFFFF" stroke="#8A8A8A" stroke-width="0.8"/>"##
                + &circle(9.5, 8.0, 1.7, B)
        }
        Ic::Dropdown => {
            fo("M1.5 4.5A1.5 1.5 0 0 1 3 3H13A1.5 1.5 0 0 1 14.5 4.5V11.5A1.5 1.5 0 0 1 13 13H3A1.5 1.5 0 0 1 1.5 11.5Z", "#FFFFFF", 0.9)
                + &l("M3.8 8H8.5", "#8A8A8A", 1.2)
                + &l("M10.5 7L12 8.6L13.5 7", B, 1.3)
        }
        Ic::Checkbox => f("M2 4A2 2 0 0 1 4 2H12A2 2 0 0 1 14 4V12A2 2 0 0 1 12 14H4A2 2 0 0 1 2 12Z", B) + &l("M4.8 8.2L7 10.4L11.2 5.8", "#FFFFFF", 1.4),
        Ic::EditField => {
            fo("M1.5 4.5A1.5 1.5 0 0 1 3 3H13A1.5 1.5 0 0 1 14.5 4.5V11.5A1.5 1.5 0 0 1 13 13H3A1.5 1.5 0 0 1 1.5 11.5Z", "#FFFFFF", 0.9)
                + &l("M4.5 5.5V10.5", B, 1.2)
                + &l("M3.6 5.5H5.4M3.6 10.5H5.4", B, 1.0)
        }
        Ic::ButtonCtl => f("M1.5 5A2 2 0 0 1 3.5 3H12.5A2 2 0 0 1 14.5 5V11A2 2 0 0 1 12.5 13H3.5A2 2 0 0 1 1.5 11Z", B) + &l("M5 8H11", "#FFFFFF", 1.2),
        Ic::CopyImg => {
            fo("M4.5 1.5H11.5A1.5 1.5 0 0 1 13 3V10A1.5 1.5 0 0 1 11.5 11.5H4.5A1.5 1.5 0 0 1 3 10V3A1.5 1.5 0 0 1 4.5 1.5Z", "#FFFFFF", 0.9)
                + &f("M4.4 9.6L6.6 6.6L8.2 8.4L9.4 7L11.6 9.6Z", G)
                + &circle(10.0, 4.4, 1.0, "#FFB900")
                + &l("M1.5 5V12.5A2 2 0 0 0 3.5 14.5H10.5", "#8A8A8A", 1.0)
        }
        Ic::SaveImg => {
            fo("M2.5 1.5H11.5A1.5 1.5 0 0 1 13 3V9H2.5A1 1 0 0 1 1 8V3A1.5 1.5 0 0 1 2.5 1.5Z", "#FFFFFF", 0.9)
                + &f("M2.6 8L5.2 4.6L7.2 6.8L8.6 5.4L11 8Z", G)
                + &circle(9.8, 3.6, 0.9, "#FFB900")
                + &f("M10.3 8.5V11.6H8.6L11.6 15L14.6 11.6H12.9V8.5Z", B)
        }
        Ic::Check => l("M3 8.5L6.5 12L13 4.5", G, 1.8),
        Ic::MoveUp => f("M8 1.5L13.5 7.2H10.2V14.5H5.8V7.2H2.5Z", B),
        Ic::MoveDown => f("M8 14.5L13.5 8.8H10.2V1.5H5.8V8.8H2.5Z", B),
        Ic::Delete => {
            f("M3.4 5H12.6L11.8 13.6A1.2 1.2 0 0 1 10.6 14.7H5.4A1.2 1.2 0 0 1 4.2 13.6Z", "#D13438")
                + &f("M2 3.4A.9.9 0 0 1 2.9 2.5H13.1A.9.9 0 0 1 14 3.4V4.2A.6.6 0 0 1 13.4 4.8H2.6A.6.6 0 0 1 2 4.2Z", "#F1707B")
                + &f("M6.2 1.3H9.8V2.5H6.2Z", "#F1707B")
                + &l("M6.5 7.2V12.4M9.5 7.2V12.4", "#FFFFFF", 1.0)
        }
        Ic::Drive => {
            f("M1.5 9.5L3.4 4A1.5 1.5 0 0 1 4.8 3H11.2A1.5 1.5 0 0 1 12.6 4L14.5 9.5Z", "#C8C8C8")
                + &f("M1.5 9.5H14.5V12A1.5 1.5 0 0 1 13 13.5H3A1.5 1.5 0 0 1 1.5 12Z", "#8A8A8A")
                + &f("M3.5 11H8.5V12H3.5Z", "#FFFFFF")
                + &circle(12.0, 11.5, 0.8, "#3FC43A")
        }
        Ic::Pc => {
            f("M1 3A1.5 1.5 0 0 1 2.5 1.5H13.5A1.5 1.5 0 0 1 15 3V10.5A1.5 1.5 0 0 1 13.5 12H2.5A1.5 1.5 0 0 1 1 10.5Z", B)
                + &f("M2.3 2.8H13.7V10.7H2.3Z", "#62ABF5")
                + &f("M6.5 12H9.5L10 14H6Z", "#8A8A8A")
                + &f("M4.5 14H11.5V14.8H4.5Z", "#8A8A8A")
        }
        Ic::Text => page() + &f("M4.5 7H10.5V8H4.5ZM4.5 9.5H10.5V10.5H4.5ZM4.5 12H8.5V13H4.5Z", B),
        Ic::Code => fo(&frame_rect(), "#FFFFFF", 0.9) + &l("M6 5.5L3.8 8L6 10.5M10 5.5L12.2 8L10 10.5", "#8764B8", 1.3),
        Ic::Title => f("M2 2.5H14V5H9.3V14H6.7V5H2Z", B),
        Ic::Heading => f("M2.5 3H4.7V7H9.3V3H11.5V13H9.3V9H4.7V13H2.5Z", B) + &f("M12.8 9.2L14.6 8.4V13H13.4V10.1L12.8 10.4Z", "#8A8A8A"),
        Ic::Subheading => {
            f("M3 4.5H4.9V7.6H8.6V4.5H10.5V13H8.6V9.4H4.9V13H3Z", B) + &l("M12 9.4A1.3 1.3 0 0 1 14.5 9.6C14.5 10.8 12 11.6 12 13H14.6", "#8A8A8A", 1.0)
        }
        Ic::SectionBreak => f("M3 2H13V5.5H3Z", "#B4D6FA") + &f("M3 10.5H13V14H3Z", "#B4D6FA") + &l("M1.5 8H3.5M6 8H7M9 8H10M12.5 8H14.5", B, 1.4),
        Ic::ClearOutput => fo(&frame_rect(), "#FFFFFF", 0.9) + &l("M5 5L11 11M11 5L5 11", "#D13438", 1.4),
        Ic::Theme => r##"<circle cx="8" cy="8" r="6.5" fill="#FFFFFF" stroke="#8A8A8A" stroke-width="0.9"/>"##.to_string() + &f("M8 1.5A6.5 6.5 0 0 1 8 14.5Z", "#1F1F1F"),
        Ic::Help => circle(8.0, 8.0, 6.5, B) + &l("M6.2 6.2A1.8 1.8 0 1 1 8.6 7.9C8.1 8.2 8 8.5 8 9.2", "#FFFFFF", 1.3) + &circle(8.0, 11.3, 0.8, "#FFFFFF"),
        Ic::Mlx => page() + &f("M4.5 7H10.5V8H4.5ZM4.5 9.5H10.5V10.5H4.5Z", "#B0B0B0") + &f("M4.5 12H8.5V13H4.5Z", "#B0B0B0") + &f("M4 4.5H8.5V5.6H4Z", B),
        Ic::MFile => page() + &l("M6 8L4.6 9.8L6 11.6M9 8L10.4 9.8L9 11.6", "#8764B8", 1.2),
        Ic::Csv => page() + &f("M4.3 7H10.7V12.6H4.3Z", G) + &l("M4.3 8.9H10.7M4.3 10.8H10.7M7.5 7V12.6", "#FFFFFF", 0.8),
        Ic::Md => page() + &f("M4.5 7H10.5V8H4.5ZM4.5 9.5H10.5V10.5H4.5ZM4.5 12H8.5V13H4.5Z", "#8A8A8A"),
        Ic::File => page(),
        // The Brazilian flag at 10:7: green field, yellow rhombus, blue globe with its white band.
        Ic::BrazilFlag => {
            r##"<rect x="0.5" y="2.5" width="15" height="11" rx="1.2" fill="#009C3B"/>"##.to_string()
                + &f("M8 3.7L14.2 8L8 12.3L1.8 8Z", "#FFDF00")
                + &circle(8.0, 8.0, 2.6, "#002776")
                + &l("M5.6 7.4C7 7 8.9 7.2 10.4 8.3", "#FFFFFF", 0.6)
        }
    }
}

fn hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

type Key = (Ic, u32, bool, [u8; 4], [u8; 4]);

static CACHE: Mutex<Option<HashMap<Key, TextureHandle>>> = Mutex::new(None);

fn rasterize(svg: &str, px: u32, disabled: bool) -> Option<ColorImage> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
    let s = px as f32 / 16.0;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(s, s), &mut pixmap.as_mut());
    let mut data = pixmap.take();
    if disabled {
        // filter: grayscale(1); opacity: .45 (on premultiplied pixels).
        for p in data.as_chunks_mut::<4>().0 {
            let y = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) * 0.45;
            p[0] = y as u8;
            p[1] = y as u8;
            p[2] = y as u8;
            p[3] = (p[3] as f32 * 0.45) as u8;
        }
    }
    Some(ColorImage::from_rgba_premultiplied([px as usize, px as usize], &data))
}

/// Paints `ic` at `size` points centred on `center`. `fg` is the colour of outline icons.
pub fn paint_with(ui: &egui::Ui, skin: &Skin, ic: Ic, center: egui::Pos2, size: f32, disabled: bool, fg: Color32) {
    let ppp = ui.ctx().pixels_per_point();
    let px = (size * ppp).round().max(1.0) as u32;
    let key: Key = (ic, px, disabled, skin.window.to_array(), fg.to_array());
    let mut guard = CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
    let tex = match cache.get(&key) {
        Some(t) => t.clone(),
        None => {
            let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">{}</svg>"#, body(ic, &hex(skin.window), &hex(fg)));
            let Some(img) = rasterize(&svg, px, disabled) else { return };
            let t = ui.ctx().load_texture(format!("icon-{ic:?}-{px}"), img, TextureOptions::LINEAR);
            cache.insert(key, t.clone());
            t
        }
    };
    drop(guard);
    let side = px as f32 / ppp;
    // Snap to the pixel grid so the bitmap is not resampled.
    let min = pos2(((center.x - side / 2.0) * ppp).round() / ppp, ((center.y - side / 2.0) * ppp).round() / ppp);
    let rect = Rect::from_min_size(min, egui::vec2(side, side));
    ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
}

pub fn paint(ui: &egui::Ui, skin: &Skin, ic: Ic, center: egui::Pos2, size: f32, disabled: bool) {
    paint_with(ui, skin, ic, center, size, disabled, skin.text);
}

/// The icon for a file in the tree, by extension.
pub fn for_file(name: &str) -> Ic {
    match name.rsplit('.').next().map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("mlx") => Ic::Mlx,
        Some("m") => Ic::MFile,
        Some("csv") | Some("xlsx") | Some("xls") => Ic::Csv,
        Some("md") => Ic::Md,
        _ => Ic::File,
    }
}
