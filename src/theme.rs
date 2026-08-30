//! Professional Theme and Styling
//!
//! Centralized theme constants and styling helpers for a modern, professional GUI.
//! Uses softer, muted colors for better readability and professional appearance.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

// ============================================================================
// Color Palette - Soft Professional Theme
// ============================================================================

/// Primary accent color (Soft Blue)
pub const PRIMARY: Color32 = Color32::from_rgb(79, 140, 201);

/// Build a tinted (unmultiplied-alpha) variant of `color`. Shared by
/// the ~20 sites that want a softer fill matching a semantic color
/// (e.g. `tinted(theme::PRIMARY, 25)` for a faint blue background
/// behind a Primary-colored label). Replaces the verbose
/// `Color32::from_rgba_unmultiplied(theme::PRIMARY.r(), theme::PRIMARY.g(),
/// theme::PRIMARY.b(), 25)` idiom.
///
/// `alpha` is the unmultiplied alpha byte (0 = transparent,
/// 255 = opaque); typical values for tinted fills are 15-35.
pub fn tinted(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// Success color (Soft Green)
pub const SUCCESS: Color32 = Color32::from_rgb(72, 155, 98);

/// Warning color (Soft Amber) - darker for better text contrast
pub const WARNING: Color32 = Color32::from_rgb(194, 136, 48);

/// Error/danger color (Soft Red)
pub const ERROR: Color32 = Color32::from_rgb(192, 86, 78);

// ============================================================================
// Section Accent Colors — unique identity per navigation section
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

// ============================================================================
// Unicode Icons — cross-platform symbols (no emoji)
// ============================================================================

// Status / state glyphs — pure-geometric Unicode shapes that render
// as text in any font (no emoji presentation). Used as inline
// indicators in status/feature rows (●/○ for active/inactive).
// The emoji-presenting ⚙ (ICON_GEAR) and ⚠ (ICON_WARNING) were
// removed when their callers switched to the SVG-backed
// crate::icons::Icon::{Gear, Warning} variants — render uniformly
// across systems with or without emoji fonts.
pub const ICON_FILLED: &str = "\u{25CF}";   // ●  (status: active/enabled)
pub const ICON_EMPTY: &str = "\u{25CB}";    // ○  (status: inactive/disabled)

// Dark theme colors
// ── Runtime-resolved palette ────────────────────────────────────────────────
// Every tab reads its colours through these accessors rather than naming a
// palette module directly, which is what makes a theme switch reach the whole
// app; `apply()` records which one is active.
use std::sync::atomic::{AtomicBool, Ordering};
static DARK_ACTIVE: AtomicBool = AtomicBool::new(true);

pub fn is_dark() -> bool {
    DARK_ACTIVE.load(Ordering::Relaxed)
}
pub fn bg() -> Color32 { if is_dark() { dark::BG } else { light::BG } }
pub fn surface() -> Color32 { if is_dark() { dark::SURFACE } else { light::SURFACE } }
pub fn surface_elevated() -> Color32 {
    if is_dark() { dark::SURFACE_ELEVATED } else { light::SURFACE_ELEVATED }
}
pub fn border() -> Color32 { if is_dark() { dark::BORDER } else { light::BORDER } }
pub fn text() -> Color32 { if is_dark() { dark::TEXT } else { light::TEXT } }
pub fn text_secondary() -> Color32 {
    if is_dark() { dark::TEXT_SECONDARY } else { light::TEXT_SECONDARY }
}
pub fn text_muted() -> Color32 { if is_dark() { dark::TEXT_MUTED } else { light::TEXT_MUTED } }
pub mod dark {
    use super::Color32;

    /// Background color
    pub const BG: Color32 = Color32::from_rgb(30, 30, 32);

    /// Surface/card background
    pub const SURFACE: Color32 = Color32::from_rgb(42, 44, 48);

    /// Elevated surface
    pub const SURFACE_ELEVATED: Color32 = Color32::from_rgb(54, 56, 62);

    /// Border color
    pub const BORDER: Color32 = Color32::from_rgb(60, 62, 68);

    /// Text primary
    pub const TEXT: Color32 = Color32::from_rgb(220, 222, 228);

    /// Text secondary
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(160, 165, 175);

    /// Text muted. Chosen to clear the WCAG 2.1 AA floor of 4.5:1 for normal
    /// text on BOTH the background and the surface - roughly fifty sites use it,
    /// and a value that passes on one and not the other fails half of them.
    /// Pinned by the `text_muted_meets_wcag_aa_contrast` tests.
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(148, 154, 164);
}

// Light theme colors
pub mod light {
    use super::Color32;

    /// Background color
    pub const BG: Color32 = Color32::from_rgb(250, 250, 252);

    /// Surface/card background
    pub const SURFACE: Color32 = Color32::from_rgb(255, 255, 255);

    /// Elevated surface
    pub const SURFACE_ELEVATED: Color32 = Color32::from_rgb(245, 246, 248);

    /// Border color
    pub const BORDER: Color32 = Color32::from_rgb(225, 228, 232);

    /// Text primary
    pub const TEXT: Color32 = Color32::from_rgb(35, 38, 45);

    /// Text secondary
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(80, 88, 100);

    /// Text muted, the light-theme counterpart. A near-white background is
    /// unforgiving: it takes a markedly darker grey than intuition suggests to
    /// clear the same 4.5:1 floor on both background and surface.
    /// Pinned by the `text_muted_meets_wcag_aa_contrast` tests.
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(104, 111, 121);
}

// ============================================================================
// Spacing and Sizing
// ============================================================================

/// Standard corner rounding
pub const ROUNDING: CornerRadius = CornerRadius {
    nw: 6,
    ne: 6,
    sw: 6,
    se: 6,
};

/// Pill rounding (for tabs, badges)
#[allow(dead_code)]
pub const ROUNDING_PILL: CornerRadius = CornerRadius {
    nw: 100,
    ne: 100,
    sw: 100,
    se: 100,
};

// ============================================================================
// Theme Application
// ============================================================================

/// Build the matching egui `Visuals` for `dark` and install them on
/// `ctx`. Folds the `Visuals::dark()/light()` + `apply_*_theme` +
/// `set_visuals` trio that was duplicated verbatim at three call sites
/// (startup, top-bar theme toggle, Settings theme picker) into one
/// helper so the theme can't be applied inconsistently in one place.
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
    visuals.extreme_bg_color = dark::SURFACE;
    visuals.faint_bg_color = dark::SURFACE;

    // Text fields and code wells sit in a visibly RECESSED background. Given the
    // same fill as the card around them, their bounds disappear.
    visuals.extreme_bg_color = Color32::from_rgb(26, 27, 30);

    // Widgets. Three distinct fills so every control reads against a SURFACE
    // card: rails/checkbox wells are recessed (bg_fill), buttons are raised
    // (weak_bg_fill = SURFACE_ELEVATED), and everything gets a 1 px border
    // (bg_stroke) so starts/ends of sliders and field bounds are explicit.
    visuals.widgets.noninteractive.bg_fill = dark::SURFACE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, dark::TEXT);
    visuals.widgets.noninteractive.weak_bg_fill = dark::SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, dark::BORDER);

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(28, 29, 32);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, dark::TEXT);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(58, 61, 68);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, dark::BORDER);
    visuals.widgets.inactive.corner_radius = ROUNDING;

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(34, 36, 40);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, PRIMARY);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(64, 66, 74);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.hovered.corner_radius = ROUNDING;

    visuals.widgets.active.bg_fill = PRIMARY;
    visuals.widgets.active.fg_stroke = Stroke::new(2.0, Color32::WHITE);
    visuals.widgets.active.weak_bg_fill = PRIMARY;
    visuals.widgets.active.corner_radius = ROUNDING;

    visuals.widgets.open.bg_fill = dark::SURFACE_ELEVATED;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.open.corner_radius = ROUNDING;

    // Selection + the slider's filled span (trailing fill, enabled in
    // apply()): SOLID accent - a translucent tint washed out against the rail
    // and made the filled span hard to see (measured 4.8:1 solid vs ~1.6:1
    // tinted on the dark rail).
    visuals.selection.bg_fill = Color32::from_rgb(70, 120, 175);
    visuals.selection.stroke = Stroke::new(1.0, PRIMARY);

    // Window
    visuals.window_stroke = Stroke::new(1.0, dark::BORDER);
    visuals.window_corner_radius = ROUNDING;

    // Menu
    visuals.menu_corner_radius = ROUNDING;

    // Other
    visuals.striped = true;
}

/// Apply professional light theme
pub fn apply_light_theme(visuals: &mut egui::Visuals) {
    visuals.window_fill = light::BG;
    visuals.panel_fill = light::BG;
    // Recessed wells for text fields (visible bounds on white cards).
    visuals.extreme_bg_color = Color32::from_rgb(238, 240, 244);
    visuals.faint_bg_color = light::SURFACE_ELEVATED;

    // Widgets - same three-level scheme as dark: recessed rails, raised
    // buttons, explicit 1 px borders.
    visuals.widgets.noninteractive.bg_fill = light::SURFACE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, light::TEXT);
    visuals.widgets.noninteractive.weak_bg_fill = light::SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, light::BORDER);

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(226, 229, 234);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, light::TEXT);
    visuals.widgets.inactive.weak_bg_fill = light::SURFACE_ELEVATED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(205, 209, 216));
    visuals.widgets.inactive.corner_radius = ROUNDING;

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(218, 222, 228);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, PRIMARY);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(235, 237, 241);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.hovered.corner_radius = ROUNDING;

    visuals.widgets.active.bg_fill = PRIMARY;
    visuals.widgets.active.fg_stroke = Stroke::new(2.0, Color32::WHITE);
    visuals.widgets.active.weak_bg_fill = PRIMARY;
    visuals.widgets.active.corner_radius = ROUNDING;

    visuals.widgets.open.bg_fill = light::SURFACE_ELEVATED;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.open.corner_radius = ROUNDING;

    // Selection + slider trailing span: solid accent (see dark theme note);
    // (65,125,190) clears 3:1 against the light rail while keeping dark
    // selected text readable (~3.5:1).
    visuals.selection.bg_fill = Color32::from_rgb(65, 125, 190);
    visuals.selection.stroke = Stroke::new(1.0, PRIMARY);

    // Window
    visuals.window_stroke = Stroke::new(1.0, light::BORDER);
    visuals.window_corner_radius = ROUNDING;

    // Menu
    visuals.menu_corner_radius = ROUNDING;

    // Other
    visuals.striped = true;
}

// ============================================================================
// UI Component Helpers
// ============================================================================

/// Create a tab pill frame (active state)
#[allow(dead_code)]
pub fn tab_active(_dark_mode: bool) -> egui::Frame {
    egui::Frame {
        inner_margin: egui::Margin::symmetric(14, 6),
        outer_margin: egui::Margin::same(0),
        corner_radius: ROUNDING_PILL,
        stroke: Stroke::NONE,
        shadow: egui::epaint::Shadow::NONE,
        fill: PRIMARY,
    }
}

/// Create a tab pill frame (inactive state)
#[allow(dead_code)]
pub fn tab_inactive(dark_mode: bool) -> egui::Frame {
    egui::Frame {
        inner_margin: egui::Margin::symmetric(14, 6),
        outer_margin: egui::Margin::same(0),
        corner_radius: ROUNDING_PILL,
        stroke: Stroke::NONE,
        shadow: egui::epaint::Shadow::NONE,
        fill: if dark_mode {
            dark::SURFACE_ELEVATED
        } else {
            light::SURFACE_ELEVATED
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tinted_forwards_to_from_rgba_unmultiplied() {
        // The helper is a thin wrapper that delegates straight to
        // Color32::from_rgba_unmultiplied. egui's encoding is
        // premultiplied internally, so we can't assert on .r()/.g()/.b()
        // equality with the source — instead, pin that
        // tinted(c, a) == Color32::from_rgba_unmultiplied(c.r(), c.g(),
        // c.b(), a) for every alpha. That's all the helper promises.
        for &color in &[PRIMARY, SUCCESS, WARNING, ERROR] {
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
        // alpha=255 is the common-case fast path inside egui's
        // from_rgba_unmultiplied (no premultiplication) — the result
        // is the opaque source color. Pin that, since several call
        // sites use alpha ~200 expecting a near-source color over
        // the background.
        assert_eq!(tinted(PRIMARY, 255), PRIMARY);
        assert_eq!(tinted(SUCCESS, 255), SUCCESS);
    }

    #[test]
    fn tinted_zero_alpha_is_fully_transparent() {
        // alpha=0 short-circuits to TRANSPARENT regardless of source.
        // Won't actually fire from real call sites (the minimum alpha
        // used is 8), but pin the contract anyway.
        let t = tinted(PRIMARY, 0);
        assert_eq!(t, Color32::TRANSPARENT);
    }

    /// WCAG 2.1 relative luminance formula. Returns a value in [0.0, 1.0].
    /// Used to compute contrast ratios for accessibility checks.
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

    /// All section accents — drift here would make two sections share
    /// the same colored navigation pill, which is the *whole point* of
    /// per-section accent identity.
    const SECTION_ACCENTS: &[(&str, Color32)] = &[
        ("ACCENT_CHAT",     ACCENT_CHAT),
        ("ACCENT_TERMINAL", ACCENT_TERMINAL),
        ("ACCENT_MODELS",   ACCENT_MODELS),
        ("ACCENT_SETTINGS", ACCENT_SETTINGS),
        ("ACCENT_LOGS",     ACCENT_LOGS),
        ("ACCENT_MEDIA",    ACCENT_MEDIA),
    ];

    /// Status accents — confused colors here look like a state bug
    /// (e.g. warnings rendered the same hue as errors).
    const STATUS_COLORS: &[(&str, Color32)] = &[
        ("PRIMARY", PRIMARY),
        ("SUCCESS", SUCCESS),
        ("WARNING", WARNING),
        ("ERROR",   ERROR),
    ];

    #[test]
    fn section_accent_colors_are_distinct() {
        // Pairwise distinctness check (O(N²) but N=6, and the
        // assert! message names the offending pair which is more
        // useful for diagnosing a collision than a HashSet
        // "expected N, got M" failure).
        //
        // enumerate + skip(i+1) instead of index ranges so clippy's
        // needless_range_loop lint stays quiet.
        for (i, (na, ca)) in SECTION_ACCENTS.iter().enumerate() {
            for (nb, cb) in SECTION_ACCENTS.iter().skip(i + 1) {
                assert_ne!(
                    (ca.r(), ca.g(), ca.b()),
                    (cb.r(), cb.g(), cb.b()),
                    "{na} and {nb} share the same RGB — section navigation\
                     pills would be visually indistinguishable"
                );
            }
        }
    }

    #[test]
    fn status_colors_are_distinct() {
        // Same pairwise pattern as section_accent_colors_are_distinct
        // — enumerate + skip(i+1) for clippy quiet, named-pair
        // failure for diagnostics.
        for (i, (na, ca)) in STATUS_COLORS.iter().enumerate() {
            for (nb, cb) in STATUS_COLORS.iter().skip(i + 1) {
                assert_ne!(
                    (ca.r(), ca.g(), ca.b()),
                    (cb.r(), cb.g(), cb.b()),
                    "{na} and {nb} share the same RGB"
                );
            }
        }
    }

    #[test]
    fn dark_theme_primary_text_meets_wcag_aa_contrast() {
        // WCAG 2.1 AA requires 4.5:1 for normal text. Pin both surface
        // pairings — text appears on BG (body) and on SURFACE (cards).
        let ratio_bg = contrast_ratio(dark::TEXT, dark::BG);
        let ratio_surface = contrast_ratio(dark::TEXT, dark::SURFACE);
        assert!(ratio_bg >= 4.5,
            "dark::TEXT on dark::BG contrast {ratio_bg:.2}:1 < 4.5:1");
        assert!(ratio_surface >= 4.5,
            "dark::TEXT on dark::SURFACE contrast {ratio_surface:.2}:1 < 4.5:1");
    }

    #[test]
    fn light_theme_primary_text_meets_wcag_aa_contrast() {
        let ratio_bg = contrast_ratio(light::TEXT, light::BG);
        let ratio_surface = contrast_ratio(light::TEXT, light::SURFACE);
        assert!(ratio_bg >= 4.5,
            "light::TEXT on light::BG contrast {ratio_bg:.2}:1 < 4.5:1");
        assert!(ratio_surface >= 4.5,
            "light::TEXT on light::SURFACE contrast {ratio_surface:.2}:1 < 4.5:1");
    }

    #[test]
    fn text_muted_meets_wcag_aa_contrast_both_themes() {
        // TEXT_MUTED carries roughly fifty sites: hints, disabled labels, secondary
        // metadata. This pins AA on BOTH the body background and the card surface,
        // in both themes, because a palette tweak that clears one and not the other
        // takes half of those sites under the floor without anything failing.
        for (label, muted, bg, surface) in [
            ("dark",  dark::TEXT_MUTED,  dark::BG,  dark::SURFACE),
            ("light", light::TEXT_MUTED, light::BG, light::SURFACE),
        ] {
            let ratio_bg = contrast_ratio(muted, bg);
            let ratio_surface = contrast_ratio(muted, surface);
            assert!(ratio_bg >= 4.5,
                "{label}::TEXT_MUTED on {label}::BG contrast {ratio_bg:.2}:1 < 4.5:1");
            assert!(ratio_surface >= 4.5,
                "{label}::TEXT_MUTED on {label}::SURFACE contrast {ratio_surface:.2}:1 < 4.5:1");
        }
    }

    #[test]
    fn status_icon_glyphs_are_single_chars() {
        // ICON_FILLED / ICON_EMPTY are inlined into RichText labels next
        // to text. If someone widened them to multi-char strings, the
        // status rows would re-flow. Pin the single-codepoint shape.
        assert_eq!(ICON_FILLED.chars().count(), 1, "ICON_FILLED must be 1 char");
        assert_eq!(ICON_EMPTY.chars().count(),  1, "ICON_EMPTY must be 1 char");
    }

    #[test]
    fn rounding_is_uniform_on_all_corners() {
        // ROUNDING is used as the standard card/button radius; pin
        // symmetry so a corner-specific edit doesn't slip in.
        assert_eq!(ROUNDING.nw, ROUNDING.ne);
        assert_eq!(ROUNDING.ne, ROUNDING.sw);
        assert_eq!(ROUNDING.sw, ROUNDING.se);
        assert_eq!(ROUNDING_PILL.nw, ROUNDING_PILL.ne);
        assert_eq!(ROUNDING_PILL.ne, ROUNDING_PILL.sw);
        assert_eq!(ROUNDING_PILL.sw, ROUNDING_PILL.se);
    }
}
