//! CLI Tab Rendering
//!
//! A chrome row naming the commands, the output behind the glass of a
//! screen, the input in a well. An entry is a row with a stripe when it
//! failed or is still running.

use eframe::egui;

use crate::state::{CLIOutput, CLIState, ModelState};
use crate::theme::{self, text};
use crate::ui::widgets;

/// Height kept under the output for the input well, and the least height
/// the output keeps.
const CLI_INPUT_RESERVE: f32 = 72.0;
const CLI_OUTPUT_MIN_H: f32 = 60.0;
/// Space above the empty state.
const EMPTY_TOP: f32 = 20.0;
/// Width of the command column in the help list.
const HELP_CMD_W: f32 = 160.0;
/// Point size of the prompt mark in the chrome row.
const MARK_PT: f32 = 16.0;
/// Padding inside an entry.
const ROW_PAD_X: i8 = 12;
const ROW_PAD_Y: i8 = 6;

/// The commands the chrome row names.
const COMMANDS: [&str; 5] = ["list", "load", "unload", "pull", "ps"];

/// Render the CLI tab content
pub fn render(ui: &mut egui::Ui, cli: &mut CLIState, _models: &ModelState) {
    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        widgets::chrome_row(ui, |ui| {
            ui.label(
                egui::RichText::new(">_")
                    .size(MARK_PT)
                    .monospace()
                    .color(theme::ink_dim()),
            );
            ui.label(text::title("Terminal"));
            ui.add_space(widgets::GAP_WIDGETS);
            ui.label(text::label("COMMANDS"));
            for cmd in COMMANDS {
                ui.label(text::readout(cmd));
            }
        });
        ui.add_space(widgets::GAP_WIDGETS);

        // The output follows its tail: new lines are appended, and the screen
        // fills the height, so there is no void above.
        let output_height =
            (ui.available_height() - CLI_INPUT_RESERVE - 2.0 * widgets::SECTION_PADDING)
                .max(CLI_OUTPUT_MIN_H);
        widgets::screen_well(ui, |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(output_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if cli.outputs.is_empty() {
                        ui.add_space(EMPTY_TOP);
                        ui.label(text::note("No commands executed yet."));
                        ui.add_space(widgets::GAP_WIDGETS);
                        ui.label(text::label("AVAILABLE COMMANDS"));
                        let cmds = [
                            ("list", "List available models"),
                            ("loaded", "List loaded models"),
                            ("load <model>", "Load a model"),
                            ("unload <model>", "Unload a model"),
                            ("pull <model>", "Pull/download a model"),
                            ("delete <model>", "Delete a model"),
                            ("ps", "Show server status"),
                            ("clear", "Clear output"),
                            ("help", "Show this list (also rendered after `help`)"),
                        ];
                        for (cmd, desc) in cmds {
                            ui.horizontal(|ui| {
                                widgets::fixed_label(
                                    ui,
                                    HELP_CMD_W,
                                    text::mono(cmd).color(theme::accent()),
                                );
                                ui.label(text::note(desc));
                            });
                        }
                        ui.add_space(widgets::GAP_WIDGETS);
                        widgets::caption_row(ui, "Press Up / Down to recall past commands.");
                    } else {
                        for output in &cli.outputs {
                            render_output(ui, output);
                            ui.add_space(widgets::GAP_LABEL);
                        }
                    }
                });
        });
        ui.add_space(widgets::GAP_WIDGETS);

        render_input_area(ui, cli);
    });
}

/// The input: a prompt mark and the field, on a well.
fn render_input_area(ui: &mut egui::Ui, cli: &mut CLIState) {
    widgets::well(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(text::mono("$").color(theme::accent()));
            let response = ui.add(
                egui::TextEdit::singleline(&mut cli.input)
                    .frame(egui::Frame::NONE)
                    .desired_width(ui.available_width())
                    .hint_text("Enter command...  (Up/Down for history)")
                    .id_salt("cli_input"),
            );
            // Focus is taken only when nothing else holds it, so the output
            // stays selectable for copying.
            if ui.ctx().memory(egui::Memory::focused).is_none() {
                response.request_focus();
            }
        });
    });
}

/// One entry: the timestamp, the prompt and the command on a line, the output
/// under it, a stripe in the error colour when it failed and in the accent
/// while it runs.
fn render_output(ui: &mut egui::Ui, output: &CLIOutput) {
    let tint = if output.is_error {
        Some(theme::error())
    } else if output.in_progress {
        Some(theme::accent())
    } else {
        None
    };
    let row = egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(ROW_PAD_X, ROW_PAD_Y))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(text::readout(&output.timestamp));
                ui.label(text::mono("$").color(theme::accent()));
                ui.label(text::mono(&output.command));
                if output.in_progress {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(text::label("RUNNING"));
                        widgets::lamp_inline(ui, true, theme::accent());
                    });
                }
            });
            ui.add_space(widgets::GAP_LABEL);
            // Selectable, so an entry can be copied.
            let ink = if output.is_error {
                theme::error()
            } else {
                theme::ink()
            };
            ui.add(egui::Label::new(text::mono(&output.output).color(ink)).selectable(true));
        });
    if let Some(tint) = tint {
        widgets::stripe(ui, row.response.rect, tint);
    }
}

/// CLI command types and parsing
#[derive(Debug)]
pub enum CLICommand {
    List,
    Loaded,
    Load(String),
    Unload(String),
    Pull(String),
    Delete(String),
    Clear,
    Help,
    Ps,
    Unknown(String),
}

impl CLICommand {
    /// Parse a command string
    pub fn parse(input: &str) -> Self {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let command = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();

        match command.as_str() {
            "clear" => CLICommand::Clear,
            "list" => CLICommand::List,
            "loaded" => CLICommand::Loaded,
            "load" => parts
                .get(1)
                .map(|s| CLICommand::Load(s.to_string()))
                .unwrap_or_else(|| CLICommand::Unknown("Usage: load <model_name>".to_string())),
            "unload" => parts
                .get(1)
                .map(|s| CLICommand::Unload(s.to_string()))
                .unwrap_or_else(|| CLICommand::Unknown("Usage: unload <model_name>".to_string())),
            "pull" => parts
                .get(1)
                .map(|s| CLICommand::Pull(s.to_string()))
                .unwrap_or_else(|| CLICommand::Unknown("Usage: pull <model_name>".to_string())),
            "delete" => parts
                .get(1)
                .map(|s| CLICommand::Delete(s.to_string()))
                .unwrap_or_else(|| CLICommand::Unknown("Usage: delete <model_name>".to_string())),
            "help" => CLICommand::Help,
            "ps" => CLICommand::Ps,
            _ => CLICommand::Unknown(format!(
                "Unknown command: {}. Type 'help' for available commands.",
                command
            )),
        }
    }
}

#[cfg(test)]
mod parse_tests {
    use super::CLICommand;

    #[test]
    fn parses_zero_arg_commands() {
        assert!(matches!(CLICommand::parse("list"), CLICommand::List));
        assert!(matches!(CLICommand::parse("loaded"), CLICommand::Loaded));
        assert!(matches!(CLICommand::parse("clear"), CLICommand::Clear));
        assert!(matches!(CLICommand::parse("help"), CLICommand::Help));
        assert!(matches!(CLICommand::parse("ps"), CLICommand::Ps));
    }

    #[test]
    fn parse_is_case_insensitive_on_command_only() {
        // Command is lowercased, argument is preserved.
        match CLICommand::parse("LOAD Qwen3-Coder") {
            CLICommand::Load(name) => assert_eq!(name, "Qwen3-Coder"),
            other => panic!("expected Load, got {other:?}"),
        }
        match CLICommand::parse("UnLoAd gemma4:26b") {
            CLICommand::Unload(name) => assert_eq!(name, "gemma4:26b"),
            other => panic!("expected Unload, got {other:?}"),
        }
    }

    #[test]
    fn parse_extracts_first_token_as_arg() {
        // Extra tokens are ignored; first arg wins.
        match CLICommand::parse("pull mistralai/Mistral-7B extra junk") {
            CLICommand::Pull(name) => assert_eq!(name, "mistralai/Mistral-7B"),
            other => panic!("expected Pull, got {other:?}"),
        }
    }

    #[test]
    fn missing_arg_returns_usage_string() {
        // Each arg-taking command surfaces a usage hint on bare invocation.
        for (cmd, hint) in [
            ("load", "Usage: load"),
            ("unload", "Usage: unload"),
            ("pull", "Usage: pull"),
            ("delete", "Usage: delete"),
        ] {
            match CLICommand::parse(cmd) {
                CLICommand::Unknown(msg) => assert!(
                    msg.starts_with(hint),
                    "{cmd}: expected usage hint starting with {hint:?}, got {msg:?}",
                ),
                other => panic!("expected Unknown for bare {cmd}, got {other:?}"),
            }
        }
    }

    #[test]
    fn empty_or_whitespace_input_is_unknown() {
        match CLICommand::parse("") {
            CLICommand::Unknown(msg) => assert!(msg.starts_with("Unknown command:")),
            other => panic!("expected Unknown, got {other:?}"),
        }
        match CLICommand::parse("   \t  ") {
            CLICommand::Unknown(msg) => assert!(msg.starts_with("Unknown command:")),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }
}
