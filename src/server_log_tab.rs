//! Server Log Tab Rendering
//!
//! A chrome row with the server's lamp and the filters, and the log behind
//! the glass of a screen. An entry is one line: a stripe in the level's tint,
//! the time, the level, the target and the message in monospace. Only Warn
//! and Error are coloured; the level is also written, so the colour is never
//! the only carrier.

use eframe::egui::{self, Color32};

use crate::icons::Icon;
use crate::log_buffer::{LogBuffer, LogLevel};
use crate::state::{LogLevelFilter, ServerState, ServerStatus};
use crate::theme::{self, text};
use crate::ui::widgets;

/// Point size of the icons in the chrome row.
const ICON_PT: f32 = 14.0;
/// Fixed cells in the chrome row: the status word and the port.
const STATUS_W: f32 = 84.0;
const PORT_W: f32 = 120.0;
/// Width of the search field.
const SEARCH_W: f32 = 200.0;
/// Fixed cells in an entry: the time and the level.
const TIME_W: f32 = 96.0;
const LEVEL_W: f32 = 48.0;
/// Padding inside an entry.
const ROW_PAD_X: i8 = 8;
const ROW_PAD_Y: i8 = 2;
/// Margin kept under the screen, and the least height it keeps.
const SCREEN_BOTTOM_MARGIN: f32 = 20.0;
const SCREEN_MIN_H: f32 = 100.0;
/// Space above the empty state.
const EMPTY_TOP: f32 = 40.0;

/// Render the Server Log tab content
pub fn render(
    ui: &mut egui::Ui,
    server: &mut ServerState,
    embedded_port: Option<u16>,
    log_buffer: &LogBuffer,
) {
    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        render_log_header(ui, server, embedded_port, log_buffer);
        ui.add_space(widgets::GAP_WIDGETS);
        render_log_output(ui, server, embedded_port, log_buffer);
    });
}

/// The server's state as a lamp and a word.
fn server_lamp(status: &ServerStatus) -> (bool, Color32, &'static str) {
    match status {
        ServerStatus::NotStarted => (false, theme::ink_dim(), "NOT STARTED"),
        ServerStatus::Starting => (true, theme::warning(), "STARTING"),
        ServerStatus::Running { .. } => (true, theme::success(), "RUNNING"),
        ServerStatus::Failed(_) => (true, theme::error(), "FAILED"),
    }
}

/// The chrome row: title, the server lamp and its word, the port, the level
/// filters as pills, the search, Copy and Clear in the tail.
fn render_log_header(
    ui: &mut egui::Ui,
    server: &mut ServerState,
    embedded_port: Option<u16>,
    log_buffer: &LogBuffer,
) {
    widgets::chrome_row(ui, |ui| {
        Icon::Server.show(ui, ICON_PT, theme::ink());
        ui.label(text::title("Server Logs"));
        ui.add_space(widgets::GAP_WIDGETS);

        let (lit, tint, word) = server_lamp(&server.status);
        widgets::lamp_inline(ui, lit, tint);
        widgets::fixed_label(ui, STATUS_W, text::label(word));
        if let Some(port) = embedded_port {
            widgets::readout(ui, PORT_W, &format!("localhost:{port}"));
        }
        ui.add_space(widgets::GAP_WIDGETS);

        ui.label(text::label("LEVEL"));
        for filter in [
            LogLevelFilter::All,
            LogLevelFilter::Info,
            LogLevelFilter::Warn,
            LogLevelFilter::Error,
        ] {
            if widgets::selector_pill(ui, filter.display_name(), server.log_filter == filter).clicked() {
                server.log_filter = filter;
            }
        }
        ui.add_space(widgets::GAP_WIDGETS);

        Icon::Search.show(ui, ICON_PT, theme::ink_dim());
        ui.add(
            egui::TextEdit::singleline(&mut server.log_search)
                .desired_width(SEARCH_W)
                .hint_text("Search"),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, Icon::Trash, "Clear the log").clicked() {
                log_buffer.clear();
            }
            // The visible lines, as "HH:MM:SS.mmm LEVEL target: message".
            if widgets::icon_button(ui, Icon::Copy, "Copy the visible (filtered) lines to the clipboard").clicked() {
                let text = server
                    .filtered_logs(log_buffer)
                    .iter()
                    .map(|e| format!("{} {} {}: {}", e.timestamp, e.level.as_str(), e.target, e.message))
                    .collect::<Vec<_>>()
                    .join("\n");
                ui.ctx().copy_text(text);
            }
        });
    });
}

/// The log, behind the glass of a screen that fills the height and follows
/// its tail.
fn render_log_output(
    ui: &mut egui::Ui,
    server: &ServerState,
    embedded_port: Option<u16>,
    log_buffer: &LogBuffer,
) {
    let log_height = (ui.available_height() - SCREEN_BOTTOM_MARGIN - 2.0 * widgets::SECTION_PADDING)
        .max(SCREEN_MIN_H);
    widgets::screen_well(ui, |ui| {
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .max_height(log_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                let filtered_logs = server.filtered_logs(log_buffer);

                if filtered_logs.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(EMPTY_TOP);
                        let total_logs = log_buffer.count();
                        if total_logs == 0 {
                            ui.label(text::value("No logs yet."));
                            ui.add_space(widgets::GAP_WIDGETS);
                            // The buffer holds the GUI's own tracing output; an embedded
                            // server's lines go through the same buffer.
                            let tip = if embedded_port.is_some() {
                                "Server logs will appear here as the server runs."
                            } else {
                                "Logs from the GUI's HTTP client and internal events appear here. \
                                 Send a chat message or refresh the Models tab to generate some."
                            };
                            ui.label(text::note(tip));
                            ui.add_space(widgets::GAP_WIDGETS);
                            // The EnvFilter set up in init_gui_logging reads RUST_LOG.
                            ui.label(text::mono("RUST_LOG=atelier=debug atelier"));
                        } else {
                            ui.label(text::note("No logs match the current filter."));
                        }
                    });
                } else {
                    for entry in filtered_logs.iter() {
                        render_log_entry(ui, entry);
                    }
                }
            });
    });
}

/// The tint of an entry's stripe and message: only Warn and Error are
/// coloured, the rest are dim ink.
fn level_stripe(level: LogLevel) -> Color32 {
    match level {
        LogLevel::Error => theme::error(),
        LogLevel::Warn => theme::warning(),
        LogLevel::Trace | LogLevel::Debug | LogLevel::Info => theme::ink_dim(),
    }
}

/// One entry: a stripe, the time and the level in fixed cells, the target,
/// the message. Selectable, so a line can be copied into a report.
fn render_log_entry(ui: &mut egui::Ui, entry: &crate::log_buffer::LogEntry) {
    let tint = level_stripe(entry.level);
    let coloured = matches!(entry.level, LogLevel::Warn | LogLevel::Error);
    let row = egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(ROW_PAD_X, ROW_PAD_Y))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::readout(ui, TIME_W, &entry.timestamp);
                widgets::readout(ui, LEVEL_W, entry.level.as_str_padded());
                let target_short = entry.target.rsplit("::").next().unwrap_or(&entry.target);
                ui.label(text::readout(target_short));
                let ink = if coloured { tint } else { theme::ink() };
                ui.add(egui::Label::new(text::mono(&entry.message).color(ink)).selectable(true));
            });
        });
    if coloured {
        widgets::stripe(ui, row.response.rect, tint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Routine levels share the dim ink; Warn and Error each have their own
    /// colour, and not each other's.
    #[test]
    fn only_warn_and_error_are_coloured() {
        let _skin = theme::skin_lock();
        let dim = theme::ink_dim();
        assert_eq!(level_stripe(LogLevel::Trace), dim);
        assert_eq!(level_stripe(LogLevel::Debug), dim);
        assert_eq!(level_stripe(LogLevel::Info), dim);
        assert_ne!(level_stripe(LogLevel::Warn), dim);
        assert_ne!(level_stripe(LogLevel::Error), dim);
        assert_ne!(level_stripe(LogLevel::Warn), level_stripe(LogLevel::Error));
    }

    /// The level is written beside the stripe, so a reader who cannot tell
    /// the colours apart still reads it.
    #[test]
    fn the_level_is_also_written() {
        let words: std::collections::HashSet<&str> = [
            LogLevel::Trace,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
        ]
        .into_iter()
        .map(LogLevel::as_str_padded)
        .collect();
        assert_eq!(words.len(), 5, "every level has its own word");
    }
}
