//! Skins mirroring ModernWinForms: the default light skin and `Win11DarkSkin`
//! (values taken from ModernTheme / ModernSkin / Win11DarkSkin).

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, Rounding, Stroke, TextStyle};

#[derive(Clone)]
pub struct Skin {
    pub dark: bool,
    pub window: Color32,
    pub surface: Color32,
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_disabled: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_pressed: Color32,
    pub on_accent: Color32,
    pub button_fill: Color32,
    pub button_hover: Color32,
    pub button_pressed: Color32,
    pub button_stroke: Color32,
    pub button_stroke_bottom: Color32,
    pub field_fill: Color32,
    pub subtle_hover: Color32,
    pub subtle_pressed: Color32,
    pub subtle_selected: Color32,
    pub border: Color32,
    pub separator: Color32,
    pub code_bg: Color32,
    pub code_border: Color32,
    pub chart_bg: Color32,
    pub chart_grid: Color32,
    pub chart_axis: Color32,
    pub chart_axis_text: Color32,
    pub palette: [Color32; 8],
    pub error: Color32,
    pub error_bg: Color32,
    pub warning: Color32,
    pub warning_bg: Color32,
    pub close_hover: Color32,
    pub syn_keyword: Color32,
    pub syn_string: Color32,
    pub syn_comment: Color32,
    pub syn_number: Color32,
    pub syn_section: Color32,
    pub track: Color32,
    pub scroll_thumb: Color32,
    pub scroll_thumb_hover: Color32,
}

/// Win11 control corner radius (`CornerStyle.Round`).
pub const RADIUS: f32 = 4.0;
/// Popup / card radius.
pub const RADIUS_LARGE: f32 = 8.0;
pub const TITLE_BAR_HEIGHT: f32 = 40.0;

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// `Color.FromArgb(a, r, g, b)` (unpremultiplied).
fn argb(a: u8, r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

const CHART_PALETTE: [Color32; 8] = [
    rgb(76, 194, 255),
    rgb(255, 183, 77),
    rgb(186, 104, 200),
    rgb(129, 199, 132),
    rgb(229, 115, 115),
    rgb(79, 195, 247),
    rgb(255, 213, 79),
    rgb(161, 136, 127),
];

impl Skin {
    pub fn light() -> Self {
        Self {
            dark: false,
            window: rgb(243, 243, 243),
            surface: rgb(251, 251, 251),
            text: rgb(29, 29, 29),
            text_secondary: rgb(96, 96, 96),
            text_disabled: rgb(160, 160, 160),
            accent: rgb(0, 95, 184),
            accent_hover: rgb(25, 110, 191),
            accent_pressed: rgb(49, 124, 197),
            on_accent: Color32::WHITE,
            button_fill: rgb(251, 251, 251),
            button_hover: rgb(246, 246, 246),
            button_pressed: rgb(245, 245, 245),
            button_stroke: argb(19, 0, 0, 0),
            button_stroke_bottom: argb(41, 0, 0, 0),
            field_fill: rgb(251, 251, 251),
            subtle_hover: argb(9, 0, 0, 0),
            subtle_pressed: argb(6, 0, 0, 0),
            subtle_selected: argb(15, 0, 0, 0),
            border: rgb(229, 229, 229),
            separator: argb(15, 0, 0, 0),
            code_bg: rgb(245, 245, 245),
            code_border: rgb(225, 225, 225),
            chart_bg: Color32::WHITE,
            chart_grid: rgb(232, 232, 232),
            chart_axis: rgb(200, 200, 200),
            chart_axis_text: rgb(110, 110, 110),
            palette: CHART_PALETTE,
            error: rgb(196, 43, 28),
            error_bg: rgb(253, 231, 233),
            warning: rgb(157, 93, 0),
            warning_bg: rgb(255, 244, 206),
            close_hover: rgb(196, 43, 28),
            syn_keyword: rgb(14, 0, 255),
            syn_string: rgb(167, 9, 245),
            syn_comment: rgb(0, 128, 19),
            syn_number: rgb(29, 29, 29),
            syn_section: rgb(0, 128, 19),
            track: rgb(237, 237, 237),
            // ModernScrollBarColors defaults.
            scroll_thumb: rgb(194, 194, 194),
            scroll_thumb_hover: rgb(160, 160, 160),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            window: rgb(32, 32, 32),
            surface: rgb(39, 39, 39),
            text: Color32::WHITE,
            text_secondary: rgb(200, 200, 200),
            text_disabled: rgb(113, 113, 113),
            accent: rgb(76, 194, 255),
            accent_hover: rgb(71, 177, 232),
            accent_pressed: rgb(66, 161, 210),
            on_accent: Color32::BLACK,
            button_fill: rgb(45, 45, 45),
            button_hover: rgb(50, 50, 50),
            button_pressed: rgb(39, 39, 39),
            button_stroke: argb(24, 255, 255, 255),
            button_stroke_bottom: argb(18, 255, 255, 255),
            field_fill: rgb(45, 45, 45),
            subtle_hover: argb(15, 255, 255, 255),
            subtle_pressed: argb(10, 255, 255, 255),
            subtle_selected: argb(20, 255, 255, 255),
            border: rgb(55, 55, 55),
            separator: argb(21, 255, 255, 255),
            code_bg: rgb(43, 43, 43),
            code_border: rgb(55, 55, 55),
            chart_bg: rgb(38, 38, 38),
            chart_grid: rgb(58, 58, 58),
            chart_axis: rgb(70, 70, 70),
            chart_axis_text: rgb(170, 170, 170),
            palette: CHART_PALETTE,
            error: rgb(255, 153, 164),
            error_bg: rgb(68, 39, 38),
            warning: rgb(252, 225, 0),
            warning_bg: rgb(67, 53, 25),
            close_hover: rgb(196, 43, 28),
            syn_keyword: rgb(86, 156, 214),
            syn_string: rgb(206, 145, 120),
            syn_comment: rgb(106, 153, 85),
            syn_number: rgb(181, 206, 168),
            syn_section: rgb(106, 153, 85),
            track: rgb(28, 28, 28),
            // Win11DarkSkin.ScrollBar.
            scroll_thumb: rgb(106, 106, 106),
            scroll_thumb_hover: rgb(140, 140, 140),
        }
    }

    /// Applies the skin to egui's visuals so stock widgets (menus, text edits, scroll bars)
    /// match the custom-painted controls.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut v = if self.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = self.window;
        v.window_fill = self.surface;
        v.extreme_bg_color = self.field_fill;
        v.faint_bg_color = self.code_bg;
        v.code_bg_color = self.code_bg;
        v.override_text_color = Some(self.text);
        v.hyperlink_color = self.accent;
        v.selection.bg_fill = if self.dark { argb(110, 76, 194, 255) } else { argb(70, 0, 95, 184) };
        v.selection.stroke = Stroke::new(1.0_f32, self.accent);
        v.window_rounding = Rounding::same(RADIUS_LARGE);
        v.menu_rounding = Rounding::same(RADIUS_LARGE);
        v.window_stroke = Stroke::new(1.0_f32, self.border);
        v.window_shadow = egui::epaint::Shadow { offset: egui::vec2(0.0, 4.0), blur: 16.0, spread: 0.0, color: argb(if self.dark { 90 } else { 40 }, 0, 0, 0) };
        v.popup_shadow = v.window_shadow;
        v.text_cursor.stroke = Stroke::new(1.5_f32, self.text);
        v.striped = false;
        let w = &mut v.widgets;
        for (wv, fill) in [
            (&mut w.noninteractive, self.surface),
            (&mut w.inactive, self.button_fill),
            (&mut w.hovered, self.button_hover),
            (&mut w.active, self.button_pressed),
            (&mut w.open, self.subtle_selected),
        ] {
            wv.bg_fill = fill;
            wv.weak_bg_fill = fill;
            wv.rounding = Rounding::same(RADIUS);
            wv.fg_stroke = Stroke::new(1.0_f32, self.text);
            wv.bg_stroke = Stroke::new(1.0_f32, self.button_stroke);
            wv.expansion = 0.0;
        }
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.separator);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, self.text);
        w.hovered.weak_bg_fill = self.subtle_hover;
        w.active.weak_bg_fill = self.subtle_pressed;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::NONE;
        w.hovered.bg_stroke = Stroke::NONE;
        w.active.bg_stroke = Stroke::NONE;
        ctx.set_visuals(v);

        ctx.style_mut(|s| {
            s.spacing.item_spacing = egui::vec2(8.0, 6.0);
            s.spacing.button_padding = egui::vec2(12.0, 5.0);
            s.spacing.menu_margin = egui::Margin::same(6.0);
            // ModernScrollBar: a 6 px rounded thumb centred in a 12 px gutter, always shown.
            s.spacing.scroll = egui::style::ScrollStyle {
                bar_width: 6.0,
                bar_inner_margin: 3.0,
                bar_outer_margin: 3.0,
                handle_min_length: 24.0,
                ..egui::style::ScrollStyle::solid()
            };
            s.spacing.interact_size.y = 28.0;
            s.text_styles = [
                (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
                (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
                (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
                (TextStyle::Heading, FontId::new(20.0, FontFamily::Name("semibold".into()))),
                (TextStyle::Monospace, FontId::new(13.5, FontFamily::Monospace)),
                (TextStyle::Name("title".into()), FontId::new(28.0, FontFamily::Name("semibold".into()))),
                (TextStyle::Name("h1".into()), FontId::new(21.0, FontFamily::Name("semibold".into()))),
                (TextStyle::Name("h2".into()), FontId::new(17.0, FontFamily::Name("semibold".into()))),
            ]
            .into();
        });
    }
}

/// Loads Segoe UI Variable (the ModernWinForms UI font) and a code font from the Windows
/// font folder, falling back to egui's bundled fonts elsewhere.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let dir = std::env::var("WINDIR").map(|w| std::path::PathBuf::from(w).join("Fonts")).unwrap_or_default();
    let load = |names: &[&str]| -> Option<Vec<u8>> { names.iter().find_map(|n| std::fs::read(dir.join(n)).ok()) };

    let mut semibold_family = vec![];
    if let Some(ui) = load(&["SegUIVar.ttf", "segoeui.ttf"]) {
        fonts.font_data.insert("ui".into(), FontData::from_owned(ui));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui".into());
    }
    if let Some(sb) = load(&["seguisb.ttf", "segoeuib.ttf"]) {
        fonts.font_data.insert("ui-semibold".into(), FontData::from_owned(sb));
        semibold_family.push("ui-semibold".to_string());
    }
    if let Some(sym) = load(&["seguisym.ttf"]) {
        fonts.font_data.insert("symbols".into(), FontData::from_owned(sym));
        fonts.families.entry(FontFamily::Proportional).or_default().push("symbols".into());
    }
    if let Some(mono) = load(&["consola.ttf", "CascadiaMono.ttf"]) {
        fonts.font_data.insert("code".into(), FontData::from_owned(mono));
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "code".into());
    }
    let proportional = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    semibold_family.extend(proportional);
    fonts.families.insert(FontFamily::Name("semibold".into()), semibold_family);
    ctx.set_fonts(fonts);
}
