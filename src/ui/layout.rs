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
use crate::theme::{self, text};
use crate::state::Section;
use crate::icons::Icon;
use crate::ui::{surface, widgets};

/// Sidebar width (icon + label when expanded)
const SIDEBAR_WIDTH: f32 = 56.0;
const SIDEBAR_WIDTH_EXPANDED: f32 = 160.0;

/// Point size of a navigation icon or glyph.
const NAV_ICON_PT: f32 = 16.0;

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
/// Gap between the blocks of the chrome row, and inside its tail.
const CHROME_GAP: f32 = 12.0;
const CHROME_TAIL_GAP: f32 = 8.0;
/// Fixed cells of the chrome row: the status word, the model name, the
/// loaded count; and how many characters of a model name are shown.
const STATUS_W: f32 = 84.0;
const MODEL_READOUT_W: f32 = 200.0;
const MODEL_READOUT_CHARS: usize = 30;
const METRICS_W: f32 = 72.0;
/// The meter in the tail.
const METER_SIZE_CHROME: Vec2 = Vec2::new(60.0, 8.0);

/// What the top bar shows.
pub struct TopBarInput<'a> {
    pub connection: crate::state::ConnectionState,
    pub connection_detail: &'a str,
    pub active_model: Option<&'a str>,
    pub loaded: usize,
    pub server_running: bool,
    pub refreshing: bool,
    /// Work in flight, 0..1, drawn as a meter in the tail. None hides it.
    pub progress: Option<f32>,
}

/// What the top bar was asked: the Refresh button, the theme toggle.
pub struct TopBarOutput {
    pub refresh_clicked: bool,
    pub theme_toggle_clicked: bool,
}

/// The top bar: a plate of pinned height. Title, status lamp and word,
/// model name in a fixed cell; in the tail the theme toggle, Refresh, the
/// meter and the loaded count as a monospace readout.
#[allow(deprecated)]
pub fn top_bar(ui: &mut egui::Ui, input: &TopBarInput) -> TopBarOutput {
    let mut out = TopBarOutput { refresh_clicked: false, theme_toggle_clicked: false };
    let margin = egui::Margin::symmetric(widgets::CHROME_MARGIN_X, widgets::CHROME_MARGIN_Y);

    egui::Panel::top("top_bar")
        .frame(egui::Frame::NONE.fill(theme::panel()).inner_margin(margin))
        .show(ui, |ui| {
            surface::plate(ui, ui.max_rect().expand2(margin.sum() / 2.0), 0.0);
            ui.horizontal(|ui| {
                ui.set_min_height(widgets::CHROME_ROW_H);

                ui.label(text::title("Atelier"));
                ui.add_space(CHROME_GAP);

                let status = classify_top_bar_status(input.connection, input.server_running);
                widgets::lamp_inline(ui, status.lit, status.color);
                widgets::fixed_label(ui, STATUS_W, text::label(status.caps))
                    .on_hover_text(input.connection_detail);
                ui.add_space(CHROME_GAP);

                match input.active_model {
                    Some(model) => {
                        let shown = crate::modality::truncate_with_ellipsis(model, MODEL_READOUT_CHARS);
                        let cell = widgets::fixed_label(ui, MODEL_READOUT_W, text::value(shown.as_ref()));
                        if model.len() > shown.len() {
                            cell.on_hover_text(model);
                        }
                    }
                    None => {
                        widgets::fixed_label(ui, MODEL_READOUT_W, text::note("No model selected"));
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let theme_tip = if theme::is_dark() {
                        "Switch to light theme"
                    } else {
                        "Switch to dark theme"
                    };
                    if widgets::icon_button(ui, Icon::Palette, theme_tip).clicked() {
                        out.theme_toggle_clicked = true;
                    }
                    ui.add_space(CHROME_TAIL_GAP);

                    if input.refreshing {
                        ui.add_enabled_ui(false, |ui| {
                            widgets::icon_button_lit(ui, Icon::Refresh, "Refreshing model list and hardware info", true);
                        });
                    } else if widgets::icon_button(ui, Icon::Refresh, "Refresh model list and hardware info").clicked() {
                        out.refresh_clicked = true;
                    }
                    ui.add_space(CHROME_TAIL_GAP);

                    if let Some(progress) = input.progress {
                        surface::meter(ui, progress, METER_SIZE_CHROME, theme::accent());
                        ui.add_space(CHROME_TAIL_GAP);
                    }
                    widgets::fixed_label(
                        ui,
                        METRICS_W,
                        text::readout(&format!("{:>2} loaded", input.loaded)),
                    );
                });
            });
        });

    out
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
    sidebar_expanded: bool,
) -> bool {
    let width = if sidebar_expanded { SIDEBAR_WIDTH_EXPANDED } else { SIDEBAR_WIDTH };
    let bg = if theme::is_dark() { Color32::from_rgb(24, 26, 30) } else { Color32::WHITE };
    let border = theme::border();
    let toggle_color = theme::ink_dim();
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
            let toggle = egui::Button::new(RichText::new(glyph).size(NAV_ICON_PT).color(toggle_color))
                .fill(Color32::TRANSPARENT)
                .min_size(Vec2::new(ui.available_width(), 24.0));
            if ui.add(toggle).on_hover_text(tip).clicked() {
                toggle_clicked = true;
            }
            ui.add_space(4.0);

            for item in NAV_ITEMS {
                render_nav_item(ui, current, &item.icon, item.label, item.tip, item.section, sidebar_expanded);
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
            theme::ink_dim(),
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
                NavIcon::Svg(ic)   => { ic.show(ui, NAV_ICON_PT, text_color); }
                NavIcon::Text(s)   => { ui.label(RichText::new(*s).size(NAV_ICON_PT).color(text_color)); }
            }
            if sidebar_expanded {
                ui.add_space(6.0);
                ui.label(text::value(label).color(text_color));
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
/// The status lamp: its colour, its word, whether it is lit.
pub(crate) struct TopBarStatus {
    pub color: Color32,
    pub caps: &'static str,
    /// Off when there is no connection at all.
    pub lit: bool,
}

pub(crate) fn classify_top_bar_status(
    state: crate::state::ConnectionState,
    server_running: bool,
) -> TopBarStatus {
    use crate::state::ConnectionState;
    let caps = if server_running {
        "ONLINE"
    } else {
        match state {
            ConnectionState::Connected => "CONNECTED",
            ConnectionState::Connecting => "CONNECTING",
            _ => "OFFLINE",
        }
    };
    TopBarStatus {
        color: state.color(),
        caps,
        lit: state != ConnectionState::Disconnected,
    }
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
        let status = classify_top_bar_status(ConnectionState::Connecting, false);
        assert_eq!(status.color, theme::warning(),
            "Connecting state must use the theme amber, not red");
        assert_eq!(status.caps, "CONNECTING",
            "label must say Connecting, not Offline, during handshake");
        assert!(status.lit);
    }

    #[test]
    fn top_bar_status_connected_is_green_and_says_connected() {
        use crate::state::ConnectionState;
        let status = classify_top_bar_status(ConnectionState::Connected, false);
        assert_eq!(status.color, theme::success());
        assert_eq!(status.caps, "CONNECTED");
        assert!(status.lit);
    }

    #[test]
    fn top_bar_status_disconnected_is_red_and_says_offline() {
        use crate::state::ConnectionState;
        let status = classify_top_bar_status(ConnectionState::Disconnected, false);
        assert_eq!(status.color, theme::error());
        assert_eq!(status.caps, "OFFLINE");
        assert!(!status.lit, "no connection: the lamp is off");
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
        let status = classify_top_bar_status(ConnectionState::Disconnected, true);
        assert_eq!(status.caps, "ONLINE");
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
                sidebar(ui, &mut section, expanded);
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
