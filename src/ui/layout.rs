//! Dashboard layout — top bar + sidebar navigation + content area
//!
//! ```text
//! +-----------------------------------------------------+
//! |  TOP BAR  [Server status] [Active model] [Metrics]   |
//! +------+----------------------------------------------+
//! |      |                                              |
//! | NAV  |           MAIN CONTENT AREA                  |
//! | SIDE |                                              |
//! |      |                                              |
//! +------+----------------------------------------------+
//! ```

use eframe::egui::{self, Color32, RichText, CornerRadius, Stroke, Vec2};
use crate::theme;
use crate::state::Section;
use crate::icons::Icon;

/// Sidebar width (icon + label when expanded)
const SIDEBAR_WIDTH: f32 = 56.0;
const SIDEBAR_WIDTH_EXPANDED: f32 = 160.0;

/// Sidebar navigation item. `icon` is an Icon variant rather than an emoji
/// character. Terminal keeps a text glyph (">_"), which is a command-prompt
/// convention rather than an emoji.
struct NavItem {
    icon: NavIcon,
    label: &'static str,
    /// One-line description shown in the hover tooltip alongside the
    /// label. Helps users learn what each section does — especially
    /// useful in collapsed-sidebar mode where only the icon is
    /// visible.
    tip: &'static str,
    section: Section,
}

/// Sidebar icon variants. Either an SVG-backed Icon or a literal
/// text glyph (for the Terminal section's ">_" command-prompt mark
/// where a generic icon would lose the semantics).
enum NavIcon {
    Svg(Icon),
    Text(&'static str),
}

const NAV_ITEMS: &[NavItem] = &[
    NavItem { icon: NavIcon::Svg(Icon::Chat),    label: "Chat",     tip: "Talk to a loaded model — text, vision, image-gen, TTS / ASR.", section: Section::Chat },
    NavItem { icon: NavIcon::Text(">_"),         label: "Terminal", tip: "REPL for model commands: list / load / unload / pull / ps.",   section: Section::Terminal },
    NavItem { icon: NavIcon::Svg(Icon::Package), label: "Models",   tip: "Browse, install, load, and delete local models.",              section: Section::Models },
    NavItem { icon: NavIcon::Svg(Icon::Gear),    label: "Settings", tip: "Server URL, API profiles, theme, config.toml editor.",         section: Section::Settings },
    NavItem { icon: NavIcon::Svg(Icon::Bolt),    label: "Studio",   tip: "Media Studio — generate images, music, SFX, MIDI, video, speech.", section: Section::MediaStudio },
    NavItem { icon: NavIcon::Svg(Icon::Server),  label: "Logs",     tip: "Live server log stream with level + search filters.",          section: Section::ServerLog },
];

/// Render the top bar
// egui 0.34 deprecates `Panel::show(&Context)` in favour of
// `Panel::show_inside(&mut Ui)`, but the new form requires the panel
// to be rendered inside a parent Ui — there is no top-level Ui in the
// eframe entry point yet, so we keep the deprecated top-level form.
// When egui exposes a top-level Ui scaffold we'll migrate; until then,
// allow the deprecated call locally rather than across the whole crate.
/// Top-bar render output. `refresh_clicked` is the Refresh button;
/// `theme_toggle_clicked` is the dark/light theme-toggle button.
/// Both signals propagate to app.rs's update() loop for handling.
pub struct TopBarOutput {
    pub refresh_clicked: bool,
    pub theme_toggle_clicked: bool,
}

#[allow(deprecated)]
pub fn top_bar(
    ui: &mut egui::Ui,
    dark: bool,
    connection_state: crate::state::ConnectionState,
    connection_detail: &str,
    active_model: Option<&str>,
    metrics_text: &str,
    server_running: bool,
    refreshing: bool,
) -> TopBarOutput {
    let mut refresh_clicked = false;
    let mut theme_toggle_clicked = false;

    let bar_fill = if dark { Color32::from_rgb(28, 30, 36) } else { Color32::WHITE };
    let bar_border = if dark { theme::border() } else { theme::light::BORDER };
    let text_primary = if dark { theme::text() } else { theme::light::TEXT };
    let text_secondary = if dark { theme::text_secondary() } else { theme::light::TEXT_SECONDARY };

    egui::Panel::top("top_bar")
        .frame(egui::Frame {
            inner_margin: egui::Margin::symmetric(16, 8),
            fill: bar_fill,
            stroke: Stroke::new(1.0, bar_border),
            shadow: egui::epaint::Shadow {
                offset: [0, 1],
                blur: 3,
                spread: 0,
                color: Color32::from_black_alpha(if dark { 30 } else { 8 }),
            },
            ..Default::default()
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(32.0);

                // App title. It named the server before, which is the one thing this
                // window is not: it holds no model and can point at any backend.
                ui.label(RichText::new("Atelier").size(16.0).strong().color(text_primary));
                ui.add_space(16.0);

                // Server status dot + label. The short_status text
                // ('Online' / 'Connected' / 'Offline') doesn't carry the
                // detail string from the underlying connection_status
                // (e.g. 'Connecting to http://localhost:11435...' or
                // 'Connection refused: ...'). Wrap both in a group that
                // tooltips the full status so users hovering can see the
                // raw connection state.
                let (status_color, short_status) =
                    classify_top_bar_status(connection_state, server_running);
                let group_resp = ui
                    .horizontal(|ui| {
                        let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot_rect.center(), 4.0, status_color);
                        ui.label(RichText::new(short_status).size(12.0).color(text_secondary));
                    })
                    .response;
                group_resp.on_hover_text(connection_detail);

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                // Active (selected) model — truncate long names. The
                // label now says 'selected' rather than 'loaded': a
                // model can be picked from the Models tab without being
                // loaded into memory yet, and the previous 'No model
                // loaded' wording conflated those two states.
                //
                // For long names the truncated label hides the suffix
                // (often the quant tag or sub-version: "…32B-q4_0:latest").
                // `Label::sense(hover)` makes the truncated text
                // tooltip-eligible so hovering reveals the full name —
                // matching the pattern used in the chat-header dropdown,
                // scheduler row, Hardware-tab device cards, etc.
                if let Some(model) = active_model {
                    let display_name = crate::modality::truncate_with_ellipsis(model, 30);
                    let lbl = ui.add(
                        egui::Label::new(
                            RichText::new(display_name.as_ref())
                                .size(12.0)
                                .strong()
                                .color(text_primary),
                        )
                        .sense(egui::Sense::hover()),
                    );
                    if model.len() > display_name.len() {
                        lbl.on_hover_text(model);
                    }
                } else {
                    ui.label(RichText::new("No model selected").size(12.0).color(text_secondary));
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                // Metrics
                if !metrics_text.is_empty() {
                    ui.label(RichText::new(metrics_text).size(11.0).color(text_secondary));
                }

                // Right side
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Refresh button — pulls model list + hardware info
                    // from the server. The same action lives on the
                    // Models tab toolbar (↻ Refresh) for users already
                    // looking at the list; the top-bar copy is a global
                    // shortcut available from every section.
                    //
                    // While a refresh is already in flight, disable the
                    // button and show an inline spinner so the click registers as
                    // "working" rather than dead: the fetch is async, and without
                    // this nothing moves until the model list changes.
                    if refreshing {
                        let btn = egui::Button::image_and_text(
                            Icon::Refresh.image(13.0, Color32::WHITE),
                            RichText::new("Refresh").size(12.0).color(Color32::WHITE),
                        )
                        .fill(theme::accent())
                        .corner_radius(CornerRadius::same(4));
                        ui.add_enabled(false, btn)
                            .on_hover_text("Refreshing model list and hardware info…");
                        ui.add_space(4.0);
                        ui.spinner();
                    } else {
                        let btn = egui::Button::image_and_text(
                            Icon::Refresh.image(13.0, Color32::WHITE),
                            RichText::new("Refresh").size(12.0).color(Color32::WHITE),
                        )
                        .fill(theme::accent())
                        .corner_radius(CornerRadius::same(4));
                        if ui.add(btn).on_hover_text("Refresh model list and hardware info").clicked() {
                            refresh_clicked = true;
                        }
                    }
                    ui.add_space(4.0);

                    // Theme toggle — quick dark/light switch without
                    // bouncing to Settings. Same Eye icon used in the
                    // Vision modality badge (currentColor SVG retints).
                    // Tooltip reflects the OPPOSITE state so the user
                    // knows what clicking will produce.
                    let (theme_tip, theme_label) = if dark {
                        ("Switch to light theme", "Light")
                    } else {
                        ("Switch to dark theme", "Dark")
                    };
                    let theme_btn = egui::Button::image_and_text(
                        Icon::Palette.image(13.0, text_secondary),
                        RichText::new(theme_label).size(12.0).color(text_secondary),
                    )
                    .fill(if dark { theme::surface_elevated() } else { theme::light::SURFACE_ELEVATED })
                    .stroke(Stroke::new(1.0, bar_border))
                    .corner_radius(CornerRadius::same(4));
                    if ui.add(theme_btn).on_hover_text(theme_tip).clicked() {
                        theme_toggle_clicked = true;
                    }
                });
            });
        });

    TopBarOutput { refresh_clicked, theme_toggle_clicked }
}

/// Render the left sidebar and return which section is active.
///
/// Returns `true` when the user clicked the collapse/expand toggle at
/// the top of the rail — the caller flips `sidebar_expanded` and
/// persists it. In the collapsed, icons-only mode the labels are hidden, so the
/// per-item hover tooltips - label plus a one-line description - are how a section
/// is identified.
#[allow(deprecated)] // see top_bar — Panel::show(&Context) until eframe top-level Ui exists.
pub fn sidebar(
    ui: &mut egui::Ui,
    current: &mut Section,
    dark: bool,
    sidebar_expanded: bool,
) -> bool {
    let width = if sidebar_expanded { SIDEBAR_WIDTH_EXPANDED } else { SIDEBAR_WIDTH };
    let bg = if dark { Color32::from_rgb(24, 26, 30) } else { Color32::WHITE };
    let border = if dark { theme::border() } else { theme::light::BORDER };
    let toggle_color = if dark { theme::text_secondary() } else { theme::light::TEXT_SECONDARY };
    let mut toggle_clicked = false;

    egui::Panel::left("nav_sidebar")
        .exact_size(width)
        .resizable(false)
        .frame(egui::Frame {
            inner_margin: egui::Margin::symmetric(4, 8),
            fill: bg,
            stroke: Stroke::new(1.0, border),
            ..Default::default()
        })
        .show(ui, |ui| {
            ui.add_space(4.0);

            // Collapse/expand toggle. A chevron pointing "«" (collapse)
            // when expanded and "»" (expand) when collapsed — the glyph
            // shows the direction the rail will move. Full-width button
            // so it stays clickable in the 56px collapsed rail.
            let (glyph, tip) = if sidebar_expanded {
                ("\u{00AB}", "Collapse sidebar to icons only")
            } else {
                ("\u{00BB}", "Expand sidebar to show labels")
            };
            let toggle = egui::Button::new(RichText::new(glyph).size(16.0).color(toggle_color))
                .fill(Color32::TRANSPARENT)
                .min_size(Vec2::new(ui.available_width(), 24.0));
            if ui.add(toggle).on_hover_text(tip).clicked() {
                toggle_clicked = true;
            }
            ui.add_space(4.0);

            for item in NAV_ITEMS {
                render_nav_item(ui, current, &item.icon, item.label, item.tip, item.section, dark, sidebar_expanded);
                ui.add_space(2.0);
            }
        });

    toggle_clicked
}

/// Render a single navigation item. The active item carries the accent.
#[allow(clippy::too_many_arguments)]
fn render_nav_item(
    ui: &mut egui::Ui,
    current: &mut Section,
    icon: &NavIcon,
    label: &str,
    tip: &str,
    section: Section,
    dark: bool,
    sidebar_expanded: bool,
) {
    let accent = theme::accent();
    let is_active = *current == section;
    let (fill, text_color) = if is_active {
        (
            theme::tinted(accent, 30),
            accent,
        )
    } else {
        (
            Color32::TRANSPARENT,
            if dark { theme::text_secondary() } else { theme::light::TEXT_SECONDARY },
        )
    };

    let resp = egui::Frame {
        inner_margin: egui::Margin::symmetric(8, 6),
        corner_radius: CornerRadius::same(6),
        fill,
        ..Default::default()
    }
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            if is_active {
                let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(3.0, 18.0), egui::Sense::hover());
                ui.painter().rect_filled(bar_rect, 1.5, accent);
                ui.add_space(2.0);
            }

            // SVG icons tint via `text_color` so the colour scheme
            // (active accent vs muted) carries through. Text-glyph
            // fallback ("Terminal" → ">_") keeps the same RichText
            // path it had before the conversion.
            match icon {
                NavIcon::Svg(ic)   => { ic.show(ui, 16.0, text_color); }
                NavIcon::Text(s)   => { ui.label(RichText::new(*s).size(16.0).color(text_color)); }
            }
            if sidebar_expanded {
                ui.add_space(6.0);
                ui.label(RichText::new(label).size(13.0).color(text_color));
            }
        });
    });

    let click_resp = resp.response.interact(egui::Sense::click());
    // Tooltip shows the section label + a one-line description.
    // Especially valuable in collapsed-sidebar mode where only the
    // icon glyph is visible — the bold label disambiguates the icon
    // and the tip teaches what the section actually does, so new
    // users don't have to click every icon to discover the layout.
    let click_resp = click_resp.on_hover_ui(|ui| {
        ui.label(egui::RichText::new(label).strong());
        ui.label(egui::RichText::new(tip).small());
    });
    if click_resp.clicked() {
        *current = section;
    }
}

/// Classify the top-bar connection status into a (dot colour, short
/// label) pair. Used by `top_bar` and unit-tested below.
///
/// Three states the user can be in, each must match consistently
/// across colour + label:
///   - Connected  → green   "Connected" (or "Online" if server_running)
///   - Connecting → amber   "Connecting…"
///   - other      → red     "Offline"
///
/// `server_running` forces "Online" (label only) — the embedded
/// server is up so the GUI knows it's reachable even if no /api/tags
/// has come back yet.
///
/// Colour + base label both derive from the `ConnectionState` enum
/// (single source of truth) — no more `.contains()` string sniffing.
pub(crate) fn classify_top_bar_status(
    state: crate::state::ConnectionState,
    server_running: bool,
) -> (Color32, &'static str) {
    // ConnectionState::color() uses the same theme::{SUCCESS,WARNING,
    // ERROR} palette the rest of the GUI uses for state dots / chips.
    let colour = state.color();
    let label = if server_running {
        "Online"
    } else {
        state.label()
    };
    (colour, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Section;

    // ── classify_top_bar_status ───────────────────────────────────

    #[test]
    fn top_bar_status_connecting_uses_amber_and_matching_label() {
        // What this guards: a label deciding Online/Connected/Offline while the
        // colour decides Connected/Connecting/red, from two independent
        // `.contains()` ladders that can disagree. Both derive
        // from the ConnectionState enum, so they can't desync.
        use crate::state::ConnectionState;
        let (colour, label) = classify_top_bar_status(ConnectionState::Connecting, false);
        assert_eq!(colour, theme::warning(),
            "Connecting state must use the theme amber, not red");
        assert_eq!(label, "Connecting…",
            "label must say Connecting, not Offline, during handshake");
    }

    #[test]
    fn top_bar_status_connected_is_green_and_says_connected() {
        use crate::state::ConnectionState;
        let (colour, label) = classify_top_bar_status(ConnectionState::Connected, false);
        assert_eq!(colour, theme::success());
        assert_eq!(label, "Connected");
    }

    #[test]
    fn top_bar_status_disconnected_is_red_and_says_offline() {
        use crate::state::ConnectionState;
        let (colour, label) = classify_top_bar_status(ConnectionState::Disconnected, false);
        assert_eq!(colour, theme::error());
        assert_eq!(label, "Offline");
    }

    #[test]
    fn top_bar_status_server_running_label_wins_over_status_check() {
        // server_running=true: label says "Online" regardless of
        // connection_status text (the embedded server is up). The
        // dot colour is still driven by connection_status — so a
        // freshly-spawned embedded server in mid-handshake renders
        // a green-or-amber dot with "Online" label (informative
        // overlap, not contradictory).
        use crate::state::ConnectionState;
        let (_, label) = classify_top_bar_status(ConnectionState::Disconnected, true);
        assert_eq!(label, "Online");
    }

    /// The sidebar draws its collapse toggle once. The block was pasted twice, which put two
    /// identical chevron buttons at the top of the rail; it was invisible in review and
    /// obvious the moment the window was rendered.
    #[test]
    fn the_sidebar_has_one_collapse_toggle() {
        fn chevrons(expanded: bool) -> usize {
            use egui_kittest::kittest::NodeT;
            let mut section = Section::Chat;
            let mut harness = egui_kittest::Harness::new_ui(move |ui| {
                sidebar(ui, &mut section, true, expanded);
            });
            harness.run();
            harness
                .root()
                .children_recursive()
                .filter(|n| {
                    let ak = n.accesskit_node();
                    let text = format!(
                        "{}{}",
                        ak.label().unwrap_or_default(),
                        ak.value().unwrap_or_default()
                    );
                    text.contains('\u{00AB}') || text.contains('\u{00BB}')
                })
                .count()
        }
        assert_eq!(chevrons(true), 1, "expanded rail");
        assert_eq!(chevrons(false), 1, "collapsed rail");
    }
}
