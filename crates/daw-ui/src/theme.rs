// SPDX-License-Identifier: GPL-3.0-or-later

//! The look: colour tokens for the dark and light themes, the fonts, and the
//! spacing. Everything that draws asks the palette for its colours, so both
//! themes come from one set of names.

use std::sync::Arc;

use egui::{
    Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Theme, Visuals,
};

/// Colours that tell tracks apart, in order (readable on both themes).
pub const TRACK_COLORS: [Color32; 8] = [
    Color32::from_rgb(236, 142, 72),
    Color32::from_rgb(72, 199, 186),
    Color32::from_rgb(141, 132, 232),
    Color32::from_rgb(92, 166, 240),
    Color32::from_rgb(231, 112, 160),
    Color32::from_rgb(214, 190, 82),
    Color32::from_rgb(120, 196, 104),
    Color32::from_rgb(200, 120, 230),
];

/// The colour of the track at `index` in the list.
pub fn track_color(index: usize) -> Color32 {
    TRACK_COLORS[index % TRACK_COLORS.len()]
}

/// Named colours. Each theme fills in the same names.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Behind everything.
    pub background: Color32,
    /// Bars and panels around the focus surface.
    pub surface: Color32,
    /// Buttons and fields on a surface.
    pub raised: Color32,
    /// Hovered buttons.
    pub raised_hover: Color32,
    /// Separating lines between areas.
    pub divider: Color32,
    /// Main text.
    pub text: Color32,
    /// Secondary text: hints, labels, units.
    pub text_weak: Color32,
    /// Notes, the playhead, focus, the active state of toggles.
    pub accent: Color32,
    /// Text and icons drawn on the accent colour.
    pub on_accent: Color32,
    /// Selected notes.
    pub selected: Color32,
    /// Recording.
    pub record: Color32,
    /// Piano roll background.
    pub grid: Color32,
    /// Rows of the black keys.
    pub grid_dark_row: Color32,
    /// Lines between steps.
    pub grid_line: Color32,
    /// Lines between beats.
    pub grid_beat: Color32,
    /// Lines between bars.
    pub grid_bar: Color32,
    /// The looped part of the ruler.
    pub loop_region: Color32,
    /// White piano keys.
    pub key_white: Color32,
    /// Black piano keys.
    pub key_black: Color32,
    /// Outline of a note.
    pub note_edge: Color32,
}

impl Palette {
    /// The dark theme.
    pub const DARK: Self = Self {
        background: Color32::from_rgb(21, 23, 26),
        surface: Color32::from_rgb(29, 32, 36),
        raised: Color32::from_rgb(41, 45, 51),
        raised_hover: Color32::from_rgb(52, 57, 64),
        divider: Color32::from_rgb(44, 48, 54),
        text: Color32::from_rgb(232, 234, 237),
        text_weak: Color32::from_rgb(154, 160, 169),
        accent: Color32::from_rgb(72, 199, 186),
        on_accent: Color32::from_rgb(10, 28, 27),
        selected: Color32::from_rgb(250, 214, 120),
        record: Color32::from_rgb(236, 84, 84),
        grid: Color32::from_rgb(25, 27, 31),
        grid_dark_row: Color32::from_rgb(21, 23, 26),
        grid_line: Color32::from_rgb(35, 38, 43),
        grid_beat: Color32::from_rgb(46, 50, 56),
        grid_bar: Color32::from_rgb(72, 78, 87),
        loop_region: Color32::from_rgba_premultiplied(30, 80, 75, 90),
        key_white: Color32::from_rgb(226, 229, 233),
        key_black: Color32::from_rgb(38, 41, 46),
        note_edge: Color32::from_rgb(8, 9, 11),
    };

    /// The light theme.
    pub const LIGHT: Self = Self {
        background: Color32::from_rgb(244, 245, 247),
        surface: Color32::from_rgb(234, 236, 240),
        raised: Color32::from_rgb(255, 255, 255),
        raised_hover: Color32::from_rgb(246, 248, 250),
        divider: Color32::from_rgb(214, 218, 224),
        text: Color32::from_rgb(26, 28, 32),
        text_weak: Color32::from_rgb(98, 105, 115),
        accent: Color32::from_rgb(0, 128, 122),
        on_accent: Color32::from_rgb(255, 255, 255),
        selected: Color32::from_rgb(196, 130, 0),
        record: Color32::from_rgb(204, 44, 44),
        grid: Color32::from_rgb(252, 252, 253),
        grid_dark_row: Color32::from_rgb(240, 242, 245),
        grid_line: Color32::from_rgb(230, 233, 237),
        grid_beat: Color32::from_rgb(214, 218, 224),
        grid_bar: Color32::from_rgb(168, 175, 186),
        loop_region: Color32::from_rgba_premultiplied(0, 90, 85, 40),
        key_white: Color32::from_rgb(255, 255, 255),
        key_black: Color32::from_rgb(52, 56, 62),
        note_edge: Color32::from_rgb(0, 70, 66),
    };

    /// The palette of the theme egui is showing.
    pub fn of(ctx: &egui::Context) -> &'static Self {
        match ctx.theme() {
            Theme::Dark => &Self::DARK,
            Theme::Light => &Self::LIGHT,
        }
    }
}

/// Font sizes, in points.
pub mod size {
    /// Body text.
    pub const BODY: f32 = 15.0;
    /// Small labels and hints.
    pub const SMALL: f32 = 12.5;
    /// Headings and the location line.
    pub const HEADING: f32 = 17.0;
    /// The big readouts in the transport bar (position, tempo).
    pub const READOUT: f32 = 22.0;
}

/// Installs the fonts and both themes' styles.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions(None));
    install_styles(ctx);
}

/// Adds a font from the computer that draws what ours cannot (Japanese
/// lyrics and voicebank names, decision 0027), after our own.
pub fn use_fallback_font(ctx: &egui::Context, bytes: Vec<u8>, index: u32) {
    let mut data = FontData::from_owned(bytes);
    data.index = index;
    ctx.set_fonts(definitions(Some(data)));
}

/// Our fonts, with a fallback for every family when there is one.
fn definitions(fallback: Option<FontData>) -> FontDefinitions {
    let mut fonts = FontDefinitions::empty();
    let regular: &'static [u8] = include_bytes!("../../../assets/fonts/Go-Regular.ttf");
    let medium: &'static [u8] = include_bytes!("../../../assets/fonts/Go-Medium.ttf");
    let mono: &'static [u8] = include_bytes!("../../../assets/fonts/Go-Mono.ttf");
    fonts
        .font_data
        .insert("go".to_owned(), Arc::new(FontData::from_static(regular)));
    fonts.font_data.insert(
        "go-medium".to_owned(),
        Arc::new(FontData::from_static(medium)),
    );
    fonts
        .font_data
        .insert("go-mono".to_owned(), Arc::new(FontData::from_static(mono)));
    fonts
        .families
        .insert(FontFamily::Proportional, vec!["go".to_owned()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["go-mono".to_owned()]);
    fonts.families.insert(
        FontFamily::Name(MEDIUM.into()),
        vec!["go-medium".to_owned()],
    );
    if let Some(data) = fallback {
        fonts
            .font_data
            .insert("fallback".to_owned(), Arc::new(data));
        for family in fonts.families.values_mut() {
            family.push("fallback".to_owned());
        }
    }
    fonts
}

/// Both themes' styles.
fn install_styles(ctx: &egui::Context) {
    for (theme, palette) in [(Theme::Dark, Palette::DARK), (Theme::Light, Palette::LIGHT)] {
        ctx.style_mut_of(theme, |style| {
            style.visuals = visuals(theme, &palette);
            style.text_styles = [
                (TextStyle::Body, FontId::proportional(size::BODY)),
                (TextStyle::Button, FontId::proportional(size::BODY)),
                (TextStyle::Small, FontId::proportional(size::SMALL)),
                (
                    TextStyle::Heading,
                    FontId::new(size::HEADING, FontFamily::Name(MEDIUM.into())),
                ),
                (TextStyle::Monospace, FontId::monospace(size::BODY)),
            ]
            .into();
            let spacing = &mut style.spacing;
            spacing.item_spacing = egui::vec2(8.0, 6.0);
            spacing.button_padding = egui::vec2(12.0, 6.0);
            spacing.interact_size.y = 30.0;
            spacing.window_margin = egui::Margin::same(16);
            spacing.menu_margin = egui::Margin::same(8);
            style.interaction.tooltip_delay = 0.35;
        });
    }
}

/// The name of the medium-weight font family.
pub const MEDIUM: &str = "medium";

fn visuals(theme: Theme, p: &Palette) -> Visuals {
    let mut v = match theme {
        Theme::Dark => Visuals::dark(),
        Theme::Light => Visuals::light(),
    };
    v.override_text_color = Some(p.text);
    v.panel_fill = p.surface;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.background;
    v.faint_bg_color = p.raised;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.window_stroke = Stroke::new(1.0, p.divider);
    v.window_corner_radius = egui::CornerRadius::same(10);
    v.menu_corner_radius = egui::CornerRadius::same(8);
    let radius = egui::CornerRadius::same(6);
    for (widget, fill) in [
        (&mut v.widgets.noninteractive, p.surface),
        (&mut v.widgets.inactive, p.raised),
        (&mut v.widgets.hovered, p.raised_hover),
        (&mut v.widgets.active, p.raised_hover),
        (&mut v.widgets.open, p.raised_hover),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.corner_radius = radius;
        widget.fg_stroke = Stroke::new(1.5, p.text);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.divider);
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.6));
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v
}
