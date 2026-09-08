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
    warning: Color32::from_rgb(0xC2, 0x88, 0x30),
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
pub fn surface() -> Color32 { palette().panel }
pub fn surface_elevated() -> Color32 { palette().raised }
pub fn border() -> Color32 { palette().border }
pub fn text() -> Color32 { palette().ink }
pub fn text_secondary() -> Color32 { palette().ink_dim }
pub fn text_muted() -> Color32 { palette().ink_dim }
pub fn success() -> Color32 { palette().success }
pub fn warning() -> Color32 { palette().warning }
pub fn error() -> Color32 { palette().error }

/// Primary accent color (Soft Blue)
pub const PRIMARY: Color32 = Color32::from_rgb(79, 140, 201);

/// Build a tinted (unmultiplied-alpha) variant of `color`. `alpha` is the
/// unmultiplied alpha byte (0 = transparent, 255 = opaque); typical values
/// for tinted fills are 15-35.
pub fn tinted(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

// ============================================================================
// Section Accent Colors - unique identity per navigation section
// ============================================================================

/// Chat section accent (soft blue)
pub const ACCENT_CHAT: Color32 = Color32::from_rgb(100, 180, 255);

/// Terminal section accent (green)
pub const ACCENT_TERMINAL: Color32 = Color32::from_rgb(100, 255, 140);

/// Models section accent (orange)
pub const ACCENT_MODELS: Color32 = Color32::from_rgb(255, 180, 100);

/// Settings section accent (neutral)
pub const ACCENT_SETTINGS: Color32 = Color32::from_rgb(160, 160, 175);

/// Server log section accent (muted cyan)
pub const ACCENT_LOGS: Color32 = Color32::from_rgb(120, 200, 200);

/// Media Studio section accent (magenta/pink)
pub const ACCENT_MEDIA: Color32 = Color32::from_rgb(255, 140, 200);

// Status glyphs for the places a dot is built into a formatted string, where
// an SVG cannot slot in. Both resolve in the bundled fonts (see the glyph audit
// in screenshots.rs). Wherever a standalone icon is drawn, use
// crate::icons::Icon instead.
pub const ICON_FILLED: &str = "\u{2022}";   // bullet (status: active/enabled)
pub const ICON_EMPTY: &str = "\u{25CB}";    // white circle (status: inactive/disabled)

// Palette fields under their former names. Call sites that branch on the
// theme themselves read these; they collapse onto the accessors above.
pub mod dark {
    use super::{Color32, DARK};
    pub const BG: Color32 = DARK.bg;
    pub const SURFACE: Color32 = DARK.panel;
    pub const SURFACE_ELEVATED: Color32 = DARK.raised;
    pub const BORDER: Color32 = DARK.border;
    pub const TEXT: Color32 = DARK.ink;
}

pub mod light {
    use super::{Color32, LIGHT};
    pub const BG: Color32 = LIGHT.bg;
    pub const SURFACE: Color32 = LIGHT.panel;
    pub const SURFACE_ELEVATED: Color32 = LIGHT.raised;
    pub const BORDER: Color32 = LIGHT.border;
    pub const TEXT: Color32 = LIGHT.ink;
    pub const TEXT_SECONDARY: Color32 = LIGHT.ink_dim;
    pub const TEXT_MUTED: Color32 = LIGHT.ink_dim;
}

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

/// Build the matching egui `Visuals` for `dark` and install them on `ctx`.
/// Records the active skin for `palette()`.
pub fn apply(ctx: &egui::Context, dark: bool) {
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    if dark {
        apply_dark_theme(&mut visuals);
    } else {
        apply_light_theme(&mut visuals);
    }
    DARK_ACTIVE.store(dark, std::sync::atomic::Ordering::Relaxed);
    ctx.set_visuals(visuals);

    // Global ergonomics: uniform control sizes and breathing room. Sliders
    // and combos share one width so every form column lines up; buttons get
    // real padding (comfortable click targets); vertical rhythm is airier
    // than egui's compact default.
    ctx.all_styles_mut(|style| {
        // Filled slider span (rail start -> handle) in the accent color:
        // makes a slider's range and current value legible at a glance.
        style.visuals.slider_trailing_fill = true;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 24.0;
        style.spacing.slider_width = 240.0;
        style.spacing.combo_width = 240.0;
    });
}

/// Apply professional dark theme
pub fn apply_dark_theme(visuals: &mut egui::Visuals) {
    visuals.window_fill = dark::BG;
    visuals.panel_fill = dark::BG;
    visuals.faint_bg_color = dark::SURFACE;

    // Text fields and code wells sit in a visibly RECESSED background. Given the
    // same fill as the card around them, their bounds disappear.
    visuals.extreme_bg_color = DARK.well;

    // Widgets. Three distinct fills so every control reads against a SURFACE
    // card: rails/checkbox wells are recessed (bg_fill), buttons are raised
    // (weak_bg_fill = SURFACE_ELEVATED), and everything gets a 1 px border
    // (bg_stroke) so starts/ends of sliders and field bounds are explicit.
    visuals.widgets.noninteractive.bg_fill = dark::SURFACE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, dark::TEXT);
    visuals.widgets.noninteractive.weak_bg_fill = dark::SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, dark::BORDER);

    visuals.widgets.inactive.bg_fill = DARK.well;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, dark::TEXT);
    visuals.widgets.inactive.weak_bg_fill = dark::SURFACE_ELEVATED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, dark::BORDER);
    visuals.widgets.inactive.corner_radius = RADIUS;

    visuals.widgets.hovered.bg_fill = dark::SURFACE_ELEVATED;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, PRIMARY);
    visuals.widgets.hovered.weak_bg_fill = dark::SURFACE_ELEVATED;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.hovered.corner_radius = RADIUS;

    visuals.widgets.active.bg_fill = PRIMARY;
    visuals.widgets.active.fg_stroke = Stroke::new(2.0, Color32::WHITE);
    visuals.widgets.active.weak_bg_fill = PRIMARY;
    visuals.widgets.active.corner_radius = RADIUS;

    visuals.widgets.open.bg_fill = dark::SURFACE_ELEVATED;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.open.corner_radius = RADIUS;

    // Selection + the slider's filled span (trailing fill, enabled in
    // apply()): SOLID accent - a translucent tint washed out against the rail
    // and made the filled span hard to see.
    visuals.selection.bg_fill = Color32::from_rgb(70, 120, 175);
    visuals.selection.stroke = Stroke::new(1.0, PRIMARY);

    visuals.window_stroke = Stroke::new(1.0, dark::BORDER);
    visuals.window_corner_radius = RADIUS;
    visuals.menu_corner_radius = RADIUS;
    visuals.striped = true;
}

/// Apply professional light theme
pub fn apply_light_theme(visuals: &mut egui::Visuals) {
    visuals.window_fill = light::BG;
    visuals.panel_fill = light::BG;
    // Recessed wells for text fields (visible bounds on white cards).
    visuals.extreme_bg_color = LIGHT.well;
    visuals.faint_bg_color = light::SURFACE_ELEVATED;

    // Widgets - same three-level scheme as dark: recessed rails, raised
    // buttons, explicit 1 px borders.
    visuals.widgets.noninteractive.bg_fill = light::SURFACE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, light::TEXT);
    visuals.widgets.noninteractive.weak_bg_fill = light::SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, light::BORDER);

    visuals.widgets.inactive.bg_fill = LIGHT.well;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, light::TEXT);
    visuals.widgets.inactive.weak_bg_fill = light::SURFACE_ELEVATED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, light::BORDER);
    visuals.widgets.inactive.corner_radius = RADIUS;

    visuals.widgets.hovered.bg_fill = light::SURFACE_ELEVATED;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, PRIMARY);
    visuals.widgets.hovered.weak_bg_fill = light::SURFACE_ELEVATED;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.hovered.corner_radius = RADIUS;

    visuals.widgets.active.bg_fill = PRIMARY;
    visuals.widgets.active.fg_stroke = Stroke::new(2.0, Color32::WHITE);
    visuals.widgets.active.weak_bg_fill = PRIMARY;
    visuals.widgets.active.corner_radius = RADIUS;

    visuals.widgets.open.bg_fill = light::SURFACE_ELEVATED;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.open.corner_radius = RADIUS;

    // Selection + slider trailing span: solid accent (see dark theme note).
    visuals.selection.bg_fill = Color32::from_rgb(65, 125, 190);
    visuals.selection.stroke = Stroke::new(1.0, PRIMARY);

    visuals.window_stroke = Stroke::new(1.0, light::BORDER);
    visuals.window_corner_radius = RADIUS;
    visuals.menu_corner_radius = RADIUS;
    visuals.striped = true;
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
        for &color in &[PRIMARY, DARK.success, DARK.warning, DARK.error] {
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
        assert_eq!(tinted(PRIMARY, 255), PRIMARY);
        assert_eq!(tinted(DARK.success, 255), DARK.success);
    }

    #[test]
    fn tinted_zero_alpha_is_fully_transparent() {
        let t = tinted(PRIMARY, 0);
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

    /// All section accents; two sections sharing a colour would share a
    /// navigation pill.
    const SECTION_ACCENTS: &[(&str, Color32)] = &[
        ("ACCENT_CHAT",     ACCENT_CHAT),
        ("ACCENT_TERMINAL", ACCENT_TERMINAL),
        ("ACCENT_MODELS",   ACCENT_MODELS),
        ("ACCENT_SETTINGS", ACCENT_SETTINGS),
        ("ACCENT_LOGS",     ACCENT_LOGS),
        ("ACCENT_MEDIA",    ACCENT_MEDIA),
    ];

    #[test]
    fn section_accent_colors_are_distinct() {
        for (i, (na, ca)) in SECTION_ACCENTS.iter().enumerate() {
            for (nb, cb) in SECTION_ACCENTS.iter().skip(i + 1) {
                assert_ne!(
                    (ca.r(), ca.g(), ca.b()),
                    (cb.r(), cb.g(), cb.b()),
                    "{na} and {nb} share the same RGB"
                );
            }
        }
    }

    #[test]
    fn status_colors_are_distinct_both_palettes() {
        for (skin, p) in SKINS {
            let status = [("success", p.success), ("warning", p.warning), ("error", p.error)];
            for (i, (na, ca)) in status.iter().enumerate() {
                for (nb, cb) in status.iter().skip(i + 1) {
                    assert_ne!(
                        (ca.r(), ca.g(), ca.b()),
                        (cb.r(), cb.g(), cb.b()),
                        "{skin}: {na} and {nb} share the same RGB"
                    );
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
}
