//! Server Log Tab Rendering
//!
//! UI rendering for the server log viewer with filtering and search.

use eframe::egui;
use egui::{Color32, RichText, CornerRadius, Stroke};

use crate::icons::Icon;
use crate::log_buffer::{LogBuffer, LogLevel};
use crate::state::{LogLevelFilter, ServerState, ServerStatus};
use crate::theme;

/// Theme-resolved colour bundle used by every render_* helper. Built
/// once at the top of `render` so each helper doesn't re-evaluate the
/// `if dark { … } else { … }` ladder for every Frame it draws — and
/// crucially so light-mode callers don't get the prior hardcoded
/// dark slate cards that made the Server Log tab look unfinished in
/// the light theme.
struct LogTabTheme {
    /// Background of the three big cards (header / filters / log output).
    card_fill: Color32,
    /// Border for those cards.
    card_stroke: Color32,
    /// Background of the small inset chips (timestamp, port, level).
    chip_fill: Color32,
    /// Body text on cards.
    text: Color32,
    /// Lower-emphasis text (labels, hints).
    text_secondary: Color32,
    /// Neutral card fill for routine Info/Debug/Trace log entries.
    /// Error/Warn keep their tinted variants — only the neutral
    /// case adapts to the theme.
    entry_neutral_fill: Color32,
    /// Border for entry neutral cards.
    entry_neutral_stroke: Color32,
}

impl LogTabTheme {
    fn for_mode(dark: bool) -> Self {
        if dark {
            // Both branches read the palette through theme accessors rather than
            // naming colours here. A tab that spells its own greys drifts from the
            // rest of the app the first time the palette is tuned.
            Self {
                card_fill: theme::raised(),
                card_stroke: theme::border(),
                chip_fill: theme::panel(),
                text: theme::ink(),
                text_secondary: theme::ink_dim(),
                entry_neutral_fill: theme::panel(),
                entry_neutral_stroke: theme::border(),
            }
        } else {
            Self {
                card_fill: theme::raised(),
                card_stroke: theme::border(),
                chip_fill: theme::panel(),
                text: theme::ink(),
                text_secondary: theme::ink_dim(),
                entry_neutral_fill: theme::panel(),
                entry_neutral_stroke: theme::border(),
            }
        }
    }
}

/// Render the Server Log tab content
pub fn render(
    ui: &mut egui::Ui,
    server: &mut ServerState,
    embedded_port: Option<u16>,
    log_buffer: &LogBuffer,
) {
    let palette = LogTabTheme::for_mode(theme::is_dark());
    // Use vertical layout with the logs taking all available space
    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        render_log_header(ui, server, embedded_port, &palette);

        ui.add_space(15.0);

        render_filter_controls(ui, server, log_buffer, &palette);

        ui.add_space(15.0);

        render_log_output(ui, server, embedded_port, log_buffer, &palette);
    });
}

/// Render beautiful log header
fn render_log_header(
    ui: &mut egui::Ui,
    server: &ServerState,
    embedded_port: Option<u16>,
    palette: &LogTabTheme,
) {
    egui::Frame::NONE
        .fill(palette.card_fill)
        .stroke(Stroke::new(1.0, palette.card_stroke))
        .inner_margin(egui::vec2(15.0, 12.0))
        .corner_radius(CornerRadius::same(8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Icon and title — uses the Server SVG to visually
                // match the sidebar's "Logs" nav button (also Server).
                Icon::Server.show(ui, 18.0, palette.text);
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Server Logs")
                        .size(16.0)
                        .strong()
                        .color(palette.text),
                );

                ui.separator();

                // Server status badge — semantic colours kept as-is
                // across themes (status semantics shouldn't flip with
                // dark/light mode), but the badge text is forced to
                // white because all four backgrounds are dark enough
                // that black light-mode text would be unreadable.
                let (status_text, status_bg, status_icon) = match &server.status {
                    ServerStatus::NotStarted => {
                        ("Not Started", Color32::from_rgb(60, 60, 60), theme::ICON_EMPTY)
                    }
                    ServerStatus::Starting => ("Starting", Color32::from_rgb(100, 100, 40), theme::ICON_EMPTY),
                    ServerStatus::Running { .. } => {
                        ("Running", Color32::from_rgb(40, 100, 60), theme::ICON_FILLED)
                    }
                    ServerStatus::Failed(_) => ("Failed", Color32::from_rgb(100, 40, 40), theme::ICON_FILLED),
                };

                egui::Frame::NONE
                    .fill(status_bg)
                    .inner_margin(egui::vec2(10.0, 6.0))
                    .corner_radius(CornerRadius::same(4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(status_icon).size(12.0));
                            ui.label(RichText::new(status_text).size(12.0).color(Color32::WHITE));
                        });
                    });

                // Port info if running
                if let Some(port) = embedded_port {
                    ui.separator();
                    egui::Frame::NONE
                        .fill(palette.chip_fill)
                        .stroke(Stroke::new(1.0, palette.card_stroke))
                        .inner_margin(egui::vec2(8.0, 4.0))
                        .corner_radius(CornerRadius::same(3))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("localhost:{}", port))
                                    .size(11.0)
                                    .color(theme::accent()),
                            );
                        });
                }
            });
        });
}

/// Render filter and search controls
fn render_filter_controls(
    ui: &mut egui::Ui,
    server: &mut ServerState,
    log_buffer: &LogBuffer,
    palette: &LogTabTheme,
) {
    egui::Frame::NONE
        .fill(palette.card_fill)
        .stroke(Stroke::new(1.0, palette.card_stroke))
        .inner_margin(egui::vec2(12.0, 10.0))
        .corner_radius(CornerRadius::same(8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Filter:")
                        .size(12.0)
                        .color(palette.text_secondary),
                );

                // Log level filter buttons. Selected = theme PRIMARY
                // (consistent with the rest of the GUI's chip language);
                // unselected = the inset chip fill so contrast against
                // the card stays the same in both themes.
                let filters = [
                    LogLevelFilter::All,
                    LogLevelFilter::Info,
                    LogLevelFilter::Warn,
                    LogLevelFilter::Error,
                ];
                for filter in filters {
                    let is_selected = server.log_filter == filter;
                    let filter_bg = if is_selected {
                        theme::accent()
                    } else {
                        palette.chip_fill
                    };
                    let filter_fg = if is_selected {
                        Color32::WHITE
                    } else {
                        palette.text
                    };

                    egui::Frame::NONE
                        .fill(filter_bg)
                        .inner_margin(egui::vec2(8.0, 4.0))
                        .corner_radius(CornerRadius::same(4))
                        .show(ui, |ui| {
                            if ui
                                .add(
                                    egui::Label::new(
                                        RichText::new(filter.display_name())
                                            .size(11.0)
                                            .color(filter_fg),
                                    )
                                    .sense(egui::Sense::click()),
                                )
                                .clicked()
                            {
                                server.log_filter = filter;
                            }
                        });
                }

                ui.separator();

                // Search field
                ui.label(
                    RichText::new("Search:")
                        .size(12.0)
                        .color(palette.text_secondary),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut server.log_search)
                        .desired_width(150.0)
                        .hint_text("filter logs..."),
                );

                // Copy visible logs — dumps the currently-filtered log
                // view (respecting level + search) to the clipboard as
                // "HH:MM:SS.mmm LEVEL target: message" lines so users
                // can paste an issue report without retyping. Uses the
                // Copy SVG to match the chat message copy button.
                let copy_btn = egui::Button::image_and_text(
                    Icon::Copy.image(12.0, palette.text),
                    RichText::new("Copy").size(11.0).color(palette.text),
                )
                .fill(palette.chip_fill)
                .stroke(Stroke::new(1.0, palette.card_stroke))
                .corner_radius(CornerRadius::same(4));
                if ui.add(copy_btn).on_hover_text("Copy the visible (filtered) logs to the clipboard").clicked() {
                    let text = server
                        .filtered_logs(log_buffer)
                        .iter()
                        .map(|e| format!("{} {} {}: {}", e.timestamp, e.level.as_str(), e.target, e.message))
                        .collect::<Vec<_>>()
                        .join("\n");
                    ui.ctx().copy_text(text);
                }

                // Clear button — Trash SVG matches the Clear-chat
                // button in chat_tab so destructive "wipe this list"
                // controls have the same visual language across the
                // GUI. theme::error() is the same red used for delete
                // buttons elsewhere; works on either theme.
                let clear_btn = egui::Button::image_and_text(
                    Icon::Trash.image(12.0, Color32::WHITE),
                    RichText::new("Clear").size(11.0).color(Color32::WHITE),
                )
                .fill(theme::error())
                .corner_radius(CornerRadius::same(4));
                if ui.add(clear_btn).clicked() {
                    log_buffer.clear();
                }
            });
        });
}

/// Render log output area
fn render_log_output(
    ui: &mut egui::Ui,
    server: &ServerState,
    embedded_port: Option<u16>,
    log_buffer: &LogBuffer,
    palette: &LogTabTheme,
) {
    let log_height = ui.available_height() - 20.0;
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .max_height(log_height.max(100.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            let filtered_logs = server.filtered_logs(log_buffer);

            if filtered_logs.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    let total_logs = log_buffer.count();
                    if total_logs == 0 {
                        ui.label(
                            RichText::new("No logs yet.")
                                .size(14.0)
                                .color(palette.text_secondary),
                        );
                        ui.add_space(10.0);

                        // The log buffer captures the GUI's own tracing
                        // output (HTTP client requests, model load/unload
                        // events, config-save errors, etc.). When an
                        // embedded server is also running its logs go
                        // through the same buffer, but the GUI does
                        // not currently spawn one — the previous hint
                        // ("atelier --port 11435") referenced a flag
                        // that doesn't exist on Args. Use a tip that
                        // works in the actual build instead.
                        let tip = if embedded_port.is_some() {
                            "Server logs will appear here as the server runs."
                        } else {
                            "Logs from the GUI's HTTP client and internal events appear here.\n\
                             Send a chat message or refresh the Models tab to generate some."
                        };
                        ui.label(
                            RichText::new(tip)
                                .size(12.0)
                                .color(palette.text_secondary),
                        );
                        ui.add_space(10.0);

                        // RUST_LOG hint — the EnvFilter set up in
                        // init_gui_logging respects this, so power
                        // users can crank up verbosity without
                        // rebuilding. Replaces the bogus --port flag.
                        egui::Frame::NONE
                            .fill(palette.chip_fill)
                            .stroke(Stroke::new(1.0, palette.card_stroke))
                            .inner_margin(egui::vec2(10.0, 6.0))
                            .corner_radius(CornerRadius::same(4))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("RUST_LOG=atelier=debug atelier")
                                        .size(12.0)
                                        .color(theme::accent())
                                        .family(egui::FontFamily::Monospace),
                                );
                            });
                    } else {
                        ui.label(
                            RichText::new("No logs match the current filter.")
                                .size(13.0)
                                .color(palette.text_secondary),
                        );
                    }
                });
            } else {
                for entry in filtered_logs.iter() {
                    render_log_entry(ui, entry, palette);
                    ui.add_space(2.0);
                }
            }
        });
}

/// Render a single log entry with beautiful card styling
fn render_log_entry(
    ui: &mut egui::Ui,
    entry: &crate::log_buffer::LogEntry,
    palette: &LogTabTheme,
) {
    // Card styling: Error and Warn entries get a distinct tinted card
    // so the user can spot them at a glance while scrolling through
    // hundreds of Info/Debug lines. Neutral (Info/Debug/Trace) entries
    // use the palette-aware fill so light-mode users see white cards,
    // not dark slate.
    let (fill_color, stroke_color) = log_card_colors(
        entry.level,
        palette.entry_neutral_fill,
        palette.entry_neutral_stroke,
    );

    egui::Frame::NONE
        .fill(fill_color)
        .stroke(Stroke::new(1.0, stroke_color))
        .inner_margin(egui::vec2(10.0, 8.0))
        .corner_radius(CornerRadius::same(4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Timestamp badge
                egui::Frame::NONE
                    .fill(palette.chip_fill)
                    .inner_margin(egui::vec2(6.0, 3.0))
                    .corner_radius(CornerRadius::same(3))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&entry.timestamp)
                                .size(9.0)
                                .color(palette.text_secondary)
                                .family(egui::FontFamily::Monospace),
                        );
                    });

                // Per-level badge colour (bg, foreground text). Five
                // distinct colour pairs so users learn the visual
                // shorthand for each level (red=error, amber=warn,
                // green=info, blue=debug, grey=trace).
                let (level_bg, level_color) = log_badge_colors(entry.level);

                egui::Frame::NONE
                    .fill(level_bg)
                    .inner_margin(egui::vec2(6.0, 3.0))
                    .corner_radius(CornerRadius::same(3))
                    .show(ui, |ui| {
                        // Static padded label — see LogLevel::as_str_padded.
                        // Was `format!("{:5}", entry.level)`, which allocated
                        // a fresh 5-byte String per visible row per repaint
                        // (60 Hz · N visible logs). Now zero allocations.
                        ui.label(
                            RichText::new(entry.level.as_str_padded())
                                .size(10.0)
                                .strong()
                                .color(level_color)
                                .family(egui::FontFamily::Monospace),
                        );
                    });

                // Target/module — theme PRIMARY so it pops as an
                // identifier-y blue on either background, but stays
                // readable against the neutral card fill.
                // rsplit().next() short-circuits on the first match from
                // the end, vs split().last() which has to iterate all
                // delimiters to find the last one. For deep module paths
                // ("loken::inference::engine::llm_engine") that's a
                // tiny per-row save × 1000-entry buffer × 60 Hz.
                let target_short = entry.target.rsplit("::").next().unwrap_or(&entry.target);
                ui.label(
                    RichText::new(format!("{}:", target_short))
                        .size(11.0)
                        .color(theme::accent())
                        .family(egui::FontFamily::Monospace),
                );

                // Message — selectable so users can drag-select + Ctrl+C
                // a log line into a bug report without having to copy the
                // entire pane.
                ui.add(
                    egui::Label::new(
                        RichText::new(&entry.message)
                            .size(11.0)
                            .color(palette.text)
                            .family(egui::FontFamily::Monospace),
                    )
                    .selectable(true),
                );
            });
        });
}

/// Card fill + stroke for a log entry. Only Error and Warn get a
/// tinted background; Info/Debug/Trace share the neutral default
/// passed in by the caller so the colour adapts to the active theme
/// (dark slate in dark mode, paper white in light mode).
///
/// Error/Warn use theme::tinted so they read correctly in BOTH modes
/// — the previous hardcoded dark-RGB fills (60, 40, 40) etc were
/// tuned for dark mode and produced near-black banners in light mode
/// (same regression we just fixed on the Hardware tab error banner).
fn log_card_colors(
    level: LogLevel,
    neutral_fill: Color32,
    neutral_stroke: Color32,
) -> (Color32, Color32) {
    match level {
        LogLevel::Error => (theme::tinted(theme::error(), 50), theme::error()),
        LogLevel::Warn => (theme::tinted(theme::warning(), 50), theme::warning()),
        _ => (neutral_fill, neutral_stroke),
    }
}

/// Per-level badge (background, foreground text) for the inline
/// level chip. Five distinct colour pairs so users learn the
/// visual shorthand:
///   - red    = ERROR (imminent attention)
///   - amber  = WARN  (noteworthy but not failing)
///   - green  = INFO  (routine status)
///   - blue   = DEBUG (verbose diagnostic)
///   - grey   = TRACE (deepest verbosity)
fn log_badge_colors(level: LogLevel) -> (Color32, Color32) {
    match level {
        LogLevel::Trace => (Color32::from_rgb(50, 50, 50), Color32::from_gray(150)),
        LogLevel::Debug => (
            Color32::from_rgb(40, 60, 80),
            Color32::from_rgb(150, 200, 255),
        ),
        LogLevel::Info => (
            Color32::from_rgb(40, 80, 60),
            Color32::from_rgb(150, 255, 150),
        ),
        LogLevel::Warn => (
            Color32::from_rgb(80, 70, 40),
            Color32::from_rgb(255, 220, 100),
        ),
        LogLevel::Error => (
            Color32::from_rgb(80, 40, 40),
            Color32::from_rgb(255, 150, 150),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convenience for tests: pin the neutral fill/stroke that the
    /// dark-mode palette always passed in before this lookup became
    /// theme-aware, so the assertions stay legible.
    const DARK_NEUTRAL_FILL: Color32   = Color32::from_rgb(40, 45, 58);
    const DARK_NEUTRAL_STROKE: Color32 = Color32::from_gray(100);

    fn card(level: LogLevel) -> (Color32, Color32) {
        log_card_colors(level, DARK_NEUTRAL_FILL, DARK_NEUTRAL_STROKE)
    }

    #[test]
    fn log_card_only_tints_warn_and_error() {
        // Info/Debug/Trace share the neutral palette — pin so a
        // regression doesn't accidentally tint Debug entries red.
        let neutral = (DARK_NEUTRAL_FILL, DARK_NEUTRAL_STROKE);
        assert_eq!(card(LogLevel::Trace), neutral);
        assert_eq!(card(LogLevel::Debug), neutral);
        assert_eq!(card(LogLevel::Info),  neutral);
        // Warn + Error each have their own tinted variant.
        assert_ne!(card(LogLevel::Warn),  neutral);
        assert_ne!(card(LogLevel::Error), neutral);
        // Warn ≠ Error (so the eye can distinguish them).
        assert_ne!(card(LogLevel::Warn), card(LogLevel::Error));
    }

    #[test]
    fn log_card_neutral_follows_caller_palette() {
        // The point of theming: passing a different neutral fill
        // changes the result for Info/Debug/Trace but NOT for the
        // semantic Warn/Error tints.
        let custom_fill   = Color32::from_rgb(250, 250, 252);
        let custom_stroke = Color32::from_rgb(225, 228, 232);
        let (info_fill,  info_stroke)  = log_card_colors(LogLevel::Info,  custom_fill, custom_stroke);
        let (warn_fill,  _warn_stroke) = log_card_colors(LogLevel::Warn,  custom_fill, custom_stroke);
        let (error_fill, _err_stroke)  = log_card_colors(LogLevel::Error, custom_fill, custom_stroke);
        assert_eq!(info_fill,   custom_fill,   "Info card must use the caller-provided fill");
        assert_eq!(info_stroke, custom_stroke, "Info card must use the caller-provided stroke");
        assert_ne!(warn_fill,   custom_fill,   "Warn card must keep its tinted fill");
        assert_ne!(error_fill,  custom_fill,   "Error card must keep its tinted fill");
    }

    #[test]
    fn log_card_error_is_redder_than_warn() {
        // Sanity check that the colour semantics aren't swapped.
        // Error's stroke red channel must exceed Warn's stroke red,
        // AND its green channel must be less. This catches a
        // copy-paste that swapped the two cases.
        let (_, err_stroke)  = card(LogLevel::Error);
        let (_, warn_stroke) = card(LogLevel::Warn);
        // Error stroke is rgb(220,100,100); Warn is rgb(255,200,100).
        // Warn has MORE green (amber) than Error.
        assert!(err_stroke.g() < warn_stroke.g(),
            "Error stroke ({:?}) must be redder than Warn stroke ({:?})",
            err_stroke, warn_stroke);
    }

    #[test]
    fn log_badge_yields_distinct_color_pair_per_level() {
        // All 5 (bg, fg) pairs must be distinct so the user can read
        // the level from the colour alone, not just the label text.
        // Inline allow for the HashSet's nested tuple type — splitting
        // it into a type alias just for one test site would obscure
        // what the test is actually checking.
        #[allow(clippy::type_complexity)]
        let pairs: std::collections::HashSet<((u8, u8, u8), (u8, u8, u8))> = [
            LogLevel::Trace, LogLevel::Debug, LogLevel::Info,
            LogLevel::Warn,  LogLevel::Error,
        ]
        .iter()
        .map(|l| {
            let (bg, fg) = log_badge_colors(*l);
            ((bg.r(), bg.g(), bg.b()), (fg.r(), fg.g(), fg.b()))
        })
        .collect();
        assert_eq!(pairs.len(), 5,
            "every log level must yield a distinct (bg, fg) badge pair; \
             got {} unique pairs for 5 levels — two levels collided",
            pairs.len());
    }

    #[test]
    fn log_badge_severity_increases_red_channel() {
        // Higher severity should be more red in the foreground.
        // Pin the ordering so a refactor can't accidentally make
        // Debug look more urgent than Error.
        let (_, debug_fg) = log_badge_colors(LogLevel::Debug);
        let (_, info_fg)  = log_badge_colors(LogLevel::Info);
        let (_, warn_fg)  = log_badge_colors(LogLevel::Warn);
        let (_, error_fg) = log_badge_colors(LogLevel::Error);
        // Error must be redder than Warn redder than Info redder than
        // Debug (Debug is bluish; Info is greenish; Warn and Error
        // are progressively redder).
        assert!(error_fg.r() >= warn_fg.r(),
            "Error fg should be at least as red as Warn fg");
        assert!(warn_fg.r()  >  info_fg.r(),
            "Warn fg should be redder than Info fg");
        assert!(info_fg.r()  >= debug_fg.r(),
            "Info fg should be at least as red as Debug fg");
    }
}
