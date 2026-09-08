//! Colour palette and egui visuals.
//!
//! The application is painted from one `Palette`, of which two instances exist:
//! `DARK` and `LIGHT`. Both obey the same physics: three surface depths (a raised
//! plate, a flat panel, a sunk well), light from above (a dark hairline under a
//! lip, a pale hairline on a far wall), one accent, three status colours. Only
//! the ink flips polarity between the two. The relations between the fields are
//! pinned by the tests at the bottom of this file.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// One skin of the application. Every colour the app paints is a field here.
// The surface fields are read by the painted surfaces (src/ui/surface.rs).
#[allow(dead_code)]
pub struct Palette {
    pub dark: bool,

    // Three depths: the floor the content sits on, a flat panel on it, a control
    // standing off the panel.
    pub bg: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub border: Color32,

    // Ink. `ink_dim` clears WCAG AA (4.5:1) on `bg`, `panel` and `plate_top`.
    pub ink: Color32,
    pub ink_dim: Color32,

    // The one accent, a quieter variant, and the ink that sits on a solid accent
    // fill.
    pub accent: Color32,
    pub accent_dim: Color32,
    pub on_accent: Color32,

    // Status. Never used as an accent; each clears 3:1 on `bg` and `panel`.
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,

    // What the surface primitives read. A plate is lighter at its top than at its
    // bottom; a well is darker than the floor; its edge darker still.
    pub plate_top: Color32,
    pub plate_bottom: Color32,
    pub plate_edge: Color32,
    pub well: Color32,
    pub well_edge: Color32,
    pub lamp_off: Color32,

    // Unmultiplied alphas for the hairlines: black under a lip, white on a far
    // wall, white for the glint on a lamp, white under printed letters.
    pub lip_shadow_a: u8,
    pub light_catch_a: u8,
    pub well_shadow_a: u8,
    pub well_catch_a: u8,
    pub glint_a: u8,
    pub print_halo_a: u8,
}

pub const DARK: Palette = Palette {
    dark: true,
    bg: Color32::from_rgb(0x1A, 0x1A, 0x1E),
    panel: Color32::from_rgb(0x26, 0x26, 0x2C),
    raised: Color32::from_rgb(0x2F, 0x2F, 0x38),
    border: Color32::from_rgb(0x3A, 0x3A, 0x45),
    ink: Color32::from_rgb(0xE8, 0xE8, 0xEE),
    ink_dim: Color32::from_rgb(0x9A, 0x9A, 0xAA),
    accent: Color32::from_rgb(0x6F, 0xA8, 0xE6),
    accent_dim: Color32::from_rgb(0x4C, 0x79, 0xAD),
    on_accent: Color32::from_rgb(0x1A, 0x1A, 0x1E),
    success: Color32::from_rgb(0x48, 0x9B, 0x62),
    warning: Color32::from_rgb(0xC8, 0x96, 0x2A),
    error: Color32::from_rgb(0xC0, 0x56, 0x4E),
    plate_top: Color32::from_rgb(0x32, 0x32, 0x3C),
    plate_bottom: Color32::from_rgb(0x28, 0x28, 0x2F),
    plate_edge: Color32::from_rgb(0x15, 0x15, 0x1A),
    well: Color32::from_rgb(0x13, 0x13, 0x17),
    well_edge: Color32::from_rgb(0x0C, 0x0C, 0x10),
    lamp_off: Color32::from_rgb(0x1E, 0x1E, 0x28),
    lip_shadow_a: 110,
    light_catch_a: 20,
    well_shadow_a: 170,
    well_catch_a: 24,
    glint_a: 150,
    print_halo_a: 30,
};

pub const LIGHT: Palette = Palette {
    dark: false,
    bg: Color32::from_rgb(0xE4, 0xE4, 0xEA),
    panel: Color32::from_rgb(0xF3, 0xF3, 0xF6),
    raised: Color32::from_rgb(0xFB, 0xFB, 0xFD),
    border: Color32::from_rgb(0xC9, 0xC9, 0xD3),
    ink: Color32::from_rgb(0x1E, 0x1E, 0x26),
    ink_dim: Color32::from_rgb(0x62, 0x62, 0x6F),
    accent: Color32::from_rgb(0x2A, 0x66, 0xAA),
    accent_dim: Color32::from_rgb(0x5A, 0x8C, 0xC8),
    on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    success: Color32::from_rgb(0x27, 0x74, 0x4A),
    warning: Color32::from_rgb(0x8F, 0x5E, 0x0E),
    error: Color32::from_rgb(0xB3, 0x39, 0x2F),
    plate_top: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    plate_bottom: Color32::from_rgb(0xEF, 0xEF, 0xF3),
    plate_edge: Color32::from_rgb(0xC2, 0xC2, 0xCC),
    well: Color32::from_rgb(0xDA, 0xDA, 0xE1),
    well_edge: Color32::from_rgb(0xBD, 0xBD, 0xC7),
    lamp_off: Color32::from_rgb(0xCF, 0xCF, 0xD8),
    lip_shadow_a: 28,
    light_catch_a: 220,
    well_shadow_a: 45,
    well_catch_a: 230,
    glint_a: 210,
    print_halo_a: 170,
};

// The active skin. `apply()` records it; every colour is read through
// `palette()` so a theme switch reaches the whole app.
use std::sync::atomic::{AtomicBool, Ordering};
static DARK_ACTIVE: AtomicBool = AtomicBool::new(true);

pub fn is_dark() -> bool {
    DARK_ACTIVE.load(Ordering::Relaxed)
}

pub fn palette() -> &'static Palette {
    if is_dark() { &DARK } else { &LIGHT }
}

pub fn bg() -> Color32 { palette().bg }
pub fn panel() -> Color32 { palette().panel }
pub fn raised() -> Color32 { palette().raised }
pub fn border() -> Color32 { palette().border }
pub fn ink() -> Color32 { palette().ink }
pub fn ink_dim() -> Color32 { palette().ink_dim }
pub fn success() -> Color32 { palette().success }
pub fn warning() -> Color32 { palette().warning }
pub fn error() -> Color32 { palette().error }
pub fn accent() -> Color32 { palette().accent }
// Read by the chrome and the views (src/ui/layout.rs, the view modules).
#[allow(dead_code)]
pub fn accent_dim() -> Color32 { palette().accent_dim }
#[allow(dead_code)]
pub fn on_accent() -> Color32 { palette().on_accent }

/// Build a tinted (unmultiplied-alpha) variant of `color`. `alpha` is the
/// unmultiplied alpha byte (0 = transparent, 255 = opaque); typical values
/// for tinted fills are 15-35.
pub fn tinted(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

// Status glyphs for the places a dot is built into a formatted string, where
// an SVG cannot slot in. Both resolve in the bundled fonts (see the glyph audit
// in screenshots.rs). Wherever a standalone icon is drawn, use
// crate::icons::Icon instead.
pub const ICON_FILLED: &str = "\u{2022}";   // bullet (status: active/enabled)
pub const ICON_EMPTY: &str = "\u{25CB}";    // white circle (status: inactive/disabled)

// ============================================================================
// Spacing and Sizing
// ============================================================================

/// Corner radius of every card, button and well, in points.
pub const RADIUS_PX: u8 = 4;

/// `RADIUS_PX` on all four corners.
pub const RADIUS: CornerRadius = CornerRadius::same(RADIUS_PX);

// ============================================================================
// Theme Application
// ============================================================================

/// Stroke width of every border, rule and ring.
pub const HAIRLINE: f32 = 1.0;

/// Alpha of the accent wash under a selection.
const SELECTION_A: u8 = 90;

/// Alpha of the accent wash on a pressed widget.
const ACTIVE_WASH_A: u8 = 60;

/// Strength of the focus ring on hover; the full ring appears when pressed.
const FOCUS_RING_HOVER_GAMMA: f32 = 0.5;

/// Gap between widgets, horizontally, and the padding inside a button.
pub const GAP_WIDGETS: f32 = 8.0;

/// Gap between a control and its label, and the vertical pitch of a form.
pub const GAP_LABEL: f32 = 4.0;

/// Height of every interactive control.
pub const CONTROL_H: f32 = 24.0;

/// Width sliders and combos share so form columns line up.
pub const FORM_CONTROL_W: f32 = 240.0;

/// Thickness of a slider rail.
const SLIDER_RAIL_H: f32 = 4.0;

/// Padding inside windows and menus.
const PANEL_PADDING: i8 = 8;

/// Build the egui visuals and style for `dark` and install them on `ctx`.
/// Records the active skin for `palette()`.
pub fn apply(ctx: &egui::Context, dark: bool) {
    let p = if dark { &DARK } else { &LIGHT };
    DARK_ACTIVE.store(dark, Ordering::Relaxed);
    ctx.set_visuals(visuals_for(p));
    ctx.all_styles_mut(|style| style_for(p, style));
}

/// One widget state: a fill, a weaker fill for buttons, a border and an ink.
/// No expansion, so nothing grows under the pointer.
fn wv(bg_fill: Color32, weak_bg_fill: Color32, bg_stroke: Color32, fg: Color32) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill,
        weak_bg_fill,
        bg_stroke: Stroke::new(HAIRLINE, bg_stroke),
        corner_radius: RADIUS,
        fg_stroke: Stroke::new(HAIRLINE, fg),
        expansion: 0.0,
    }
}

/// A one-pixel shadow under a lip; never a blur.
fn hairline_shadow(p: &Palette) -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 1],
        blur: 0,
        spread: 0,
        color: Color32::from_black_alpha(p.lip_shadow_a),
    }
}

/// The egui visuals of one skin. Panels are flat, text fields and rails are
/// sunk in the well, buttons stand on the raised fill, and the accent appears
/// as a ring on hover and a wash when pressed.
pub fn visuals_for(p: &Palette) -> egui::Visuals {
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.well;
    v.text_edit_bg_color = Some(p.well);
    v.code_bg_color = p.well;
    v.faint_bg_color = p.raised;
    v.window_stroke = Stroke::new(HAIRLINE, p.border);
    v.window_corner_radius = RADIUS;
    v.menu_corner_radius = RADIUS;
    v.window_shadow = hairline_shadow(p);
    v.popup_shadow = hairline_shadow(p);
    v.selection.bg_fill = tinted(p.accent, SELECTION_A);
    v.selection.stroke = Stroke::new(HAIRLINE, p.accent);
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.error;
    v.widgets.noninteractive = wv(p.panel, p.panel, p.border, p.ink);
    v.widgets.inactive = wv(p.well, p.raised, p.border, p.ink);
    v.widgets.hovered = wv(p.raised, p.raised, p.accent.gamma_multiply(FOCUS_RING_HOVER_GAMMA), p.ink);
    let wash = tinted(p.accent, ACTIVE_WASH_A);
    v.widgets.active = wv(wash, wash, p.accent, p.ink);
    v.widgets.open = wv(p.raised, p.raised, p.accent, p.ink);
    v.striped = true;
    v.slider_trailing_fill = true;
    v
}

/// The spacing and the type scale of one skin: an 8 px grid, 24 px controls,
/// one form width, five text styles.
pub fn style_for(_p: &Palette, style: &mut egui::Style) {
    style.text_styles = text::styles();
    style.spacing.item_spacing = egui::vec2(GAP_WIDGETS, GAP_LABEL);
    style.spacing.button_padding = egui::vec2(GAP_WIDGETS, GAP_LABEL);
    style.spacing.interact_size.y = CONTROL_H;
    style.spacing.slider_width = FORM_CONTROL_W;
    style.spacing.combo_width = FORM_CONTROL_W;
    style.spacing.slider_rail_height = SLIDER_RAIL_H;
    style.spacing.window_margin = egui::Margin::same(PANEL_PADDING);
    style.spacing.menu_margin = egui::Margin::same(PANEL_PADDING);
}

/// The type scale. Five sizes and a weight each; the colour is the palette's.
// The whole scale is read once the views are on it (src/ui/widgets.rs, the view modules).
#[allow(dead_code)]
pub mod text {
    use super::palette;
    use eframe::egui::{FontFamily, FontId, RichText, TextStyle};
    use std::collections::BTreeMap;

    /// A label over or beside a control: capitals, dim. Callers pass capitals.
    pub const LABEL_PT: f32 = 10.0;
    /// A value, a button, a name: the reading size of a control.
    pub const VALUE_PT: f32 = 12.0;
    /// A section title: capitals in the accent.
    pub const SECTION_PT: f32 = 11.0;
    /// Body prose.
    pub const BODY_PT: f32 = 13.0;
    /// The title in a chrome row.
    pub const TITLE_PT: f32 = 14.0;
    /// Monospace readouts.
    pub const MONO_PT: f32 = 12.0;
    /// A dim monospace readout in a chrome tail.
    pub const READOUT_PT: f32 = 10.0;
    /// Prose under a control, and its line pitch.
    pub const CAPTION_PT: f32 = 8.5;
    pub const CAPTION_PITCH: f32 = 10.5;
    /// Tracked capitals over a hairline, and their letter spacing.
    pub const HEADING_PT: f32 = 9.5;
    pub const TRACKING: f32 = 1.5;
    /// The icon of an empty state.
    pub const EMPTY_STATE_ICON_PT: f32 = 32.0;

    /// egui's named styles on the same scale.
    pub fn styles() -> BTreeMap<TextStyle, FontId> {
        [
            (TextStyle::Body, FontId::new(BODY_PT, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(VALUE_PT, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(TITLE_PT, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(MONO_PT, FontFamily::Monospace)),
            (TextStyle::Small, FontId::new(LABEL_PT, FontFamily::Proportional)),
        ]
        .into()
    }

    pub fn label(caps: &str) -> RichText {
        RichText::new(caps).size(LABEL_PT).color(palette().ink_dim)
    }
    pub fn value(s: &str) -> RichText {
        RichText::new(s).size(VALUE_PT).strong().color(palette().ink)
    }
    pub fn section(caps: &str) -> RichText {
        RichText::new(caps).size(SECTION_PT).strong().color(palette().accent)
    }
    pub fn title(s: &str) -> RichText {
        RichText::new(s).size(TITLE_PT).strong().color(palette().ink)
    }
    pub fn note(s: &str) -> RichText {
        RichText::new(s).size(VALUE_PT).color(palette().ink_dim)
    }
    pub fn mono(s: &str) -> RichText {
        RichText::new(s).monospace().size(MONO_PT).color(palette().ink)
    }
    pub fn readout(s: &str) -> RichText {
        RichText::new(s).monospace().size(READOUT_PT).color(palette().ink_dim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two skins, named for assertion messages.
    const SKINS: [(&str, &Palette); 2] = [("dark", &DARK), ("light", &LIGHT)];

    /// WCAG AA floor for normal text.
    const AA_TEXT: f64 = 4.5;

    /// WCAG AA floor for user-interface components and large text.
    const AA_COMPONENT: f64 = 3.0;

    #[test]
    fn tinted_forwards_to_from_rgba_unmultiplied() {
        // Pin that tinted(c, a) == Color32::from_rgba_unmultiplied(c.r(),
        // c.g(), c.b(), a) for every alpha; egui's internal encoding is
        // premultiplied, so channel equality with the source cannot be
        // asserted directly.
        for &color in &[LIGHT.accent, DARK.success, DARK.warning, DARK.error] {
            for alpha in [0u8, 1, 18, 25, 30, 180, 200, 255] {
                let expected = Color32::from_rgba_unmultiplied(
                    color.r(), color.g(), color.b(), alpha,
                );
                let got = tinted(color, alpha);
                assert_eq!(got, expected,
                    "tinted({color:?}, {alpha}) must equal the direct \
                     Color32::from_rgba_unmultiplied call it replaces");
            }
        }
    }

    #[test]
    fn tinted_opaque_alpha_equals_source() {
        assert_eq!(tinted(LIGHT.accent, 255), LIGHT.accent);
        assert_eq!(tinted(DARK.success, 255), DARK.success);
    }

    #[test]
    fn tinted_zero_alpha_is_fully_transparent() {
        let t = tinted(LIGHT.accent, 0);
        assert_eq!(t, Color32::TRANSPARENT);
    }

    /// WCAG 2.1 relative luminance, in [0.0, 1.0].
    fn rel_luminance(c: Color32) -> f64 {
        fn chan(v: u8) -> f64 {
            let s = v as f64 / 255.0;
            if s <= 0.03928 { s / 12.92 }
            else { ((s + 0.055) / 1.055).powf(2.4) }
        }
        0.2126 * chan(c.r()) + 0.7152 * chan(c.g()) + 0.0722 * chan(c.b())
    }

    fn contrast_ratio(a: Color32, b: Color32) -> f64 {
        let la = rel_luminance(a);
        let lb = rel_luminance(b);
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Luminance on raw bytes, for ordering surfaces against each other.
    fn byte_lum(c: Color32) -> f64 {
        0.2126 * c.r() as f64 + 0.7152 * c.g() as f64 + 0.0722 * c.b() as f64
    }

    fn assert_contrast(what: &str, fg: Color32, bg: Color32, floor: f64) {
        let ratio = contrast_ratio(fg, bg);
        assert!(ratio >= floor, "{what}: contrast {ratio:.2}:1 < {floor}:1");
    }

    /// Manhattan distance in RGB bytes; two colours closer than
    /// `HUE_APART` read as the same hue at a glance.
    fn rgb_distance(a: Color32, b: Color32) -> i32 {
        (a.r() as i32 - b.r() as i32).abs()
            + (a.g() as i32 - b.g() as i32).abs()
            + (a.b() as i32 - b.b() as i32).abs()
    }
    const HUE_APART: i32 = 90;

    #[test]
    fn the_accent_is_not_a_status_colour() {
        // The accent marks selection and focus; a status colour marks an
        // outcome. Neither may be mistaken for the other, and no two
        // outcomes may share a hue.
        for (skin, p) in SKINS {
            let named = [
                ("accent", p.accent),
                ("success", p.success),
                ("warning", p.warning),
                ("error", p.error),
            ];
            for (i, (na, ca)) in named.iter().enumerate() {
                for (nb, cb) in named.iter().skip(i + 1) {
                    let d = rgb_distance(*ca, *cb);
                    assert!(d > HUE_APART, "{skin}: {na} and {nb} are {d} apart, under {HUE_APART}");
                }
            }
        }
    }

    #[test]
    fn ink_meets_wcag_aa_on_bg_and_panel_both_palettes() {
        for (skin, p) in SKINS {
            assert_contrast(&format!("{skin} ink on bg"), p.ink, p.bg, AA_TEXT);
            assert_contrast(&format!("{skin} ink on panel"), p.ink, p.panel, AA_TEXT);
        }
    }

    #[test]
    fn ink_dim_meets_wcag_aa_on_bg_and_panel_both_palettes() {
        // ink_dim carries hints, disabled labels and secondary metadata on
        // both the floor and the panels; a value that clears one and not the
        // other takes half of those sites under the floor.
        for (skin, p) in SKINS {
            assert_contrast(&format!("{skin} ink_dim on bg"), p.ink_dim, p.bg, AA_TEXT);
            assert_contrast(&format!("{skin} ink_dim on panel"), p.ink_dim, p.panel, AA_TEXT);
        }
    }

    #[test]
    fn ink_dim_reads_on_the_plate() {
        for (skin, p) in SKINS {
            assert_contrast(&format!("{skin} ink_dim on plate_top"), p.ink_dim, p.plate_top, AA_TEXT);
        }
    }

    #[test]
    fn status_reads_on_bg_and_panel_both_palettes() {
        for (skin, p) in SKINS {
            for (name, c) in [("success", p.success), ("warning", p.warning), ("error", p.error)] {
                assert_contrast(&format!("{skin} {name} on bg"), c, p.bg, AA_COMPONENT);
                assert_contrast(&format!("{skin} {name} on panel"), c, p.panel, AA_COMPONENT);
            }
        }
    }

    #[test]
    fn on_accent_reads_on_the_accent() {
        for (skin, p) in SKINS {
            assert_contrast(&format!("{skin} on_accent on accent"), p.on_accent, p.accent, AA_TEXT);
        }
    }

    #[test]
    fn the_well_is_sunk_and_the_plate_is_raised() {
        // Light comes from above in both skins: the well is the darkest
        // surface, the plate the lightest, and a plate is lighter at its top.
        for (skin, p) in SKINS {
            let order = [
                ("well_edge", p.well_edge),
                ("well", p.well),
                ("bg", p.bg),
                ("panel", p.panel),
                ("plate_top", p.plate_top),
            ];
            for pair in order.windows(2) {
                let (na, a) = pair[0];
                let (nb, b) = pair[1];
                assert!(byte_lum(a) < byte_lum(b), "{skin}: {na} must be darker than {nb}");
            }
            assert!(byte_lum(p.plate_bottom) < byte_lum(p.plate_top),
                "{skin}: the plate must be lighter at its top");
        }
    }

    #[test]
    fn the_ink_reads_against_the_plate() {
        assert!(byte_lum(DARK.ink) > byte_lum(DARK.ink_dim));
        assert!(byte_lum(DARK.ink_dim) > byte_lum(DARK.plate_top));
        assert!(byte_lum(LIGHT.ink) < byte_lum(LIGHT.ink_dim));
        assert!(byte_lum(LIGHT.ink_dim) < byte_lum(LIGHT.plate_bottom));
    }

    #[test]
    fn the_light_skin_inverts_only_the_ink() {
        let surfaces = |p: &Palette| [
            p.bg, p.panel, p.raised, p.border, p.plate_top, p.plate_bottom,
            p.plate_edge, p.well, p.well_edge, p.lamp_off,
        ];
        for (d, l) in surfaces(&DARK).iter().zip(surfaces(&LIGHT).iter()) {
            assert!(byte_lum(*l) > byte_lum(*d), "every light surface is lighter than its dark twin");
        }
        for (d, l) in [(DARK.ink, LIGHT.ink), (DARK.ink_dim, LIGHT.ink_dim)] {
            assert!(byte_lum(l) < byte_lum(d), "every light ink is darker than its dark twin");
        }
    }

    #[test]
    fn status_icon_glyphs_are_single_chars() {
        assert_eq!(ICON_FILLED.chars().count(), 1, "ICON_FILLED must be 1 char");
        assert_eq!(ICON_EMPTY.chars().count(),  1, "ICON_EMPTY must be 1 char");
    }

    #[test]
    fn apply_installs_the_palette() {
        let ctx = egui::Context::default();
        apply(&ctx, false);
        assert!(!is_dark());
        assert_eq!(ctx.global_style().visuals.panel_fill, LIGHT.panel);
        assert_eq!(ctx.global_style().visuals.extreme_bg_color, LIGHT.well);
        assert_eq!(ctx.global_style().spacing.interact_size.y, CONTROL_H);
        assert_eq!(ctx.global_style().text_styles[&egui::TextStyle::Body].size, text::BODY_PT);
        apply(&ctx, true);
        assert!(is_dark());
        assert_eq!(ctx.global_style().visuals.panel_fill, DARK.panel);
        assert_eq!(ctx.global_style().visuals.extreme_bg_color, DARK.well);
        assert_eq!(ctx.global_style().visuals.widgets.hovered.expansion, 0.0);
    }
}
