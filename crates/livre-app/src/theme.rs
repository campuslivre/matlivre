//! Skins mirroring ModernWinForms: the default light skin and `Win11DarkSkin`. Field comments
//! name the design token each value comes from.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, Rounding, Stroke, TextStyle};

#[derive(Clone)]
pub struct Skin {
    pub dark: bool,
    /// `--bg`: the window, and the code block background.
    pub window: Color32,
    /// `--tv-bg`: the panel and document cards.
    pub card: Color32,
    pub surface: Color32,
    /// `--fg`, `--fg2`, `--fg3`.
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_tertiary: Color32,
    pub text_disabled: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub on_accent: Color32,
    pub button_fill: Color32,
    pub button_hover: Color32,
    pub button_pressed: Color32,
    pub button_stroke: Color32,
    pub button_stroke_bottom: Color32,
    /// `--tb-*`: text boxes.
    pub field_fill: Color32,
    pub field_hover: Color32,
    pub field_focus: Color32,
    pub field_stroke_bottom: Color32,
    pub field_disabled: Color32,
    pub subtle_hover: Color32,
    pub subtle_pressed: Color32,
    pub subtle_selected: Color32,
    /// `--sel`: a panel toggle whose panel is open.
    pub selected: Color32,
    /// `--pn-border`.
    pub border: Color32,
    /// `--tb-hover-border`: the window frame.
    pub frame: Color32,
    /// `--sep`.
    pub separator: Color32,
    pub code_bg: Color32,
    pub code_border: Color32,
    /// `--menu-*`.
    pub menu_bg: Color32,
    pub menu_border: Color32,
    pub menu_hover: Color32,
    /// `--ts-sel`, `--ts-sel-border`: the selected title-bar tab.
    pub tab_selected: Color32,
    pub tab_selected_border: Color32,
    /// `--tv-hover`, `--tv-sel`: tree rows.
    pub row_hover: Color32,
    pub row_selected: Color32,
    /// `--sp-grip`, `--split-active`.
    pub grip: Color32,
    pub split_active: Color32,
    /// `--tk-*`: track bar.
    pub slider_track: Color32,
    pub slider_thumb: Color32,
    pub slider_thumb_border: Color32,
    /// `--sw-off-border`: unchecked check box border.
    pub check_border: Color32,
    pub check_fill: Color32,
    pub chart_bg: Color32,
    pub chart_grid: Color32,
    pub chart_axis: Color32,
    pub chart_axis_text: Color32,
    pub palette: [Color32; 8],
    pub error: Color32,
    pub error_bg: Color32,
    pub warning: Color32,
    pub warning_bg: Color32,
    pub info_bg: Color32,
    pub close_hover: Color32,
    pub close_pressed: Color32,
    pub syn_text: Color32,
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

/// `rgba(r, g, b, a)` with a CSS alpha (unpremultiplied).
fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8)
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
            card: Color32::WHITE,
            surface: rgb(251, 251, 251),
            text: rgb(29, 29, 29),
            text_secondary: rgb(96, 96, 96),
            text_tertiary: rgb(138, 138, 138),
            text_disabled: rgb(160, 160, 160),
            accent: rgb(0, 95, 184),
            accent_hover: rgb(25, 110, 191),
            on_accent: Color32::WHITE,
            button_fill: rgb(251, 251, 251),
            button_hover: rgb(245, 245, 245),
            button_pressed: rgb(238, 238, 238),
            button_stroke: rgba(0, 0, 0, 0.073),
            button_stroke_bottom: rgba(0, 0, 0, 0.16),
            field_fill: rgb(251, 251, 251),
            field_hover: rgb(246, 246, 246),
            field_focus: Color32::WHITE,
            field_stroke_bottom: rgba(0, 0, 0, 0.45),
            field_disabled: rgb(245, 245, 245),
            subtle_hover: rgba(0, 0, 0, 0.04),
            subtle_pressed: rgba(0, 0, 0, 0.024),
            subtle_selected: rgba(0, 0, 0, 0.06),
            selected: rgba(0, 0, 0, 0.055),
            border: rgb(229, 229, 229),
            frame: rgb(200, 200, 200),
            separator: rgba(0, 0, 0, 0.078),
            code_bg: rgb(243, 243, 243),
            code_border: rgb(229, 229, 229),
            menu_bg: rgb(252, 252, 252),
            menu_border: rgba(0, 0, 0, 0.1),
            menu_hover: rgba(0, 0, 0, 0.037),
            tab_selected: rgb(230, 230, 230),
            tab_selected_border: rgb(207, 207, 207),
            row_hover: rgba(0, 0, 0, 0.037),
            row_selected: rgba(0, 0, 0, 0.06),
            grip: rgb(138, 138, 138),
            split_active: rgba(0, 0, 0, 0.06),
            slider_track: rgb(204, 204, 204),
            slider_thumb: Color32::WHITE,
            slider_thumb_border: rgb(214, 214, 214),
            check_border: rgb(140, 140, 140),
            check_fill: rgba(0, 0, 0, 0.024),
            chart_bg: Color32::WHITE,
            chart_grid: rgb(232, 232, 232),
            chart_axis: rgb(200, 200, 200),
            chart_axis_text: rgb(110, 110, 110),
            palette: CHART_PALETTE,
            error: rgb(196, 43, 28),
            error_bg: rgb(253, 231, 233),
            warning: rgb(157, 93, 0),
            warning_bg: rgb(255, 244, 206),
            info_bg: rgb(246, 246, 246),
            close_hover: rgb(196, 43, 28),
            close_pressed: rgb(200, 63, 49),
            syn_text: rgb(29, 29, 29),
            syn_keyword: rgb(14, 0, 255),
            syn_string: rgb(167, 9, 245),
            syn_comment: rgb(0, 128, 19),
            syn_number: rgb(29, 29, 29),
            syn_section: rgb(0, 128, 19),
            track: rgb(237, 237, 237),
            scroll_thumb: rgb(194, 194, 194),
            scroll_thumb_hover: rgb(160, 160, 160),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            window: rgb(32, 32, 32),
            card: rgb(38, 38, 38),
            surface: rgb(39, 39, 39),
            text: Color32::WHITE,
            text_secondary: rgb(200, 200, 200),
            text_tertiary: rgb(157, 157, 157),
            text_disabled: rgb(113, 113, 113),
            accent: rgb(76, 194, 255),
            accent_hover: rgb(71, 177, 232),
            on_accent: Color32::BLACK,
            button_fill: rgb(45, 45, 45),
            button_hover: rgb(50, 50, 50),
            button_pressed: rgb(39, 39, 39),
            button_stroke: rgba(255, 255, 255, 0.093),
            button_stroke_bottom: rgba(255, 255, 255, 0.07),
            field_fill: rgb(45, 45, 45),
            field_hover: rgb(50, 50, 50),
            field_focus: rgb(31, 31, 31),
            field_stroke_bottom: rgba(255, 255, 255, 0.54),
            field_disabled: rgb(39, 39, 39),
            subtle_hover: rgba(255, 255, 255, 0.059),
            subtle_pressed: rgba(255, 255, 255, 0.043),
            subtle_selected: rgba(255, 255, 255, 0.08),
            selected: rgba(255, 255, 255, 0.075),
            border: rgb(55, 55, 55),
            frame: rgb(65, 65, 65),
            separator: rgba(255, 255, 255, 0.082),
            code_bg: rgb(32, 32, 32),
            code_border: rgb(55, 55, 55),
            menu_bg: rgb(44, 44, 44),
            menu_border: rgb(62, 62, 62),
            menu_hover: rgba(255, 255, 255, 0.06),
            tab_selected: rgb(45, 45, 45),
            tab_selected_border: rgb(69, 69, 69),
            row_hover: rgba(255, 255, 255, 0.06),
            row_selected: rgba(255, 255, 255, 0.08),
            grip: rgb(157, 157, 157),
            split_active: rgb(50, 50, 50),
            slider_track: rgb(90, 90, 90),
            slider_thumb: rgb(69, 69, 69),
            slider_thumb_border: rgb(85, 85, 85),
            check_border: rgb(160, 160, 160),
            check_fill: rgba(0, 0, 0, 0.1),
            chart_bg: rgb(38, 38, 38),
            chart_grid: rgb(58, 58, 58),
            chart_axis: rgb(70, 70, 70),
            chart_axis_text: rgb(170, 170, 170),
            palette: CHART_PALETTE,
            error: rgb(255, 153, 164),
            error_bg: rgb(68, 39, 38),
            warning: rgb(252, 225, 0),
            warning_bg: rgb(67, 53, 25),
            info_bg: rgb(43, 43, 43),
            close_hover: rgb(196, 43, 28),
            close_pressed: rgb(200, 63, 49),
            syn_text: Color32::WHITE,
            syn_keyword: rgb(86, 156, 214),
            syn_string: rgb(206, 145, 120),
            syn_comment: rgb(106, 153, 85),
            syn_number: rgb(181, 206, 168),
            syn_section: rgb(106, 153, 85),
            track: rgb(28, 28, 28),
            scroll_thumb: rgb(106, 106, 106),
            scroll_thumb_hover: rgb(140, 140, 140),
        }
    }

    /// Applies the skin to egui's visuals so stock widgets (menus, text edits, scroll bars)
    /// match the custom-painted controls.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut v = if self.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = self.window;
        v.window_fill = self.menu_bg;
        v.extreme_bg_color = self.field_fill;
        v.faint_bg_color = self.code_bg;
        v.code_bg_color = self.code_bg;
        v.override_text_color = Some(self.text);
        v.hyperlink_color = self.accent;
        v.selection.bg_fill = if self.dark { rgba(76, 194, 255, 0.43) } else { rgba(0, 95, 184, 0.27) };
        v.selection.stroke = Stroke::new(1.0_f32, self.accent);
        v.window_rounding = Rounding::same(RADIUS_LARGE);
        v.menu_rounding = Rounding::same(RADIUS_LARGE);
        v.window_stroke = Stroke::new(1.0_f32, self.menu_border);
        // --shadow-flyout: 0 8px 16px rgba(0,0,0,.14).
        v.window_shadow = egui::epaint::Shadow { offset: egui::vec2(0.0, 8.0), blur: 16.0, spread: 0.0, color: rgba(0, 0, 0, if self.dark { 0.3 } else { 0.14 }) };
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
        w.hovered.weak_bg_fill = self.menu_hover;
        w.active.weak_bg_fill = self.subtle_pressed;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::NONE;
        w.hovered.bg_stroke = Stroke::NONE;
        w.active.bg_stroke = Stroke::NONE;
        ctx.set_visuals(v);

        ctx.style_mut(|s| {
            s.spacing.item_spacing = egui::vec2(8.0, 6.0);
            s.spacing.button_padding = egui::vec2(12.0, 5.0);
            s.spacing.menu_margin = egui::Margin::same(4.0);
            // ModernScrollBar: a 6 px rounded thumb centred in a 12 px gutter, always shown.
            s.spacing.scroll = egui::style::ScrollStyle {
                bar_width: 6.0,
                bar_inner_margin: 3.0,
                bar_outer_margin: 3.0,
                handle_min_length: 24.0,
                ..egui::style::ScrollStyle::solid()
            };
            s.spacing.interact_size.y = 28.0;
            s.interaction.tooltip_delay = 0.5;
            s.text_styles = [
                (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
                (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
                (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
                (TextStyle::Heading, FontId::new(20.0, FontFamily::Name("semibold".into()))),
                (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
                (TextStyle::Name("title".into()), FontId::new(28.0, FontFamily::Name("semibold".into()))),
                (TextStyle::Name("h1".into()), FontId::new(20.0, FontFamily::Name("semibold".into()))),
                (TextStyle::Name("h2".into()), FontId::new(16.0, FontFamily::Name("semibold".into()))),
            ]
            .into();
        });
    }
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
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
    let mut mono_bold = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    if let Some(b) = load(&["consolab.ttf", "CascadiaMono.ttf"]) {
        fonts.font_data.insert("code-bold".into(), FontData::from_owned(b));
        mono_bold.insert(0, "code-bold".into());
    }
    fonts.families.insert(FontFamily::Name("mono-bold".into()), mono_bold);
    let proportional = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    semibold_family.extend(proportional);
    fonts.families.insert(FontFamily::Name("semibold".into()), semibold_family);
    ctx.set_fonts(fonts);
}
