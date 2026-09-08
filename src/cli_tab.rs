//! CLI Tab Rendering
//!
//! UI rendering for the CLI command interface.
//! Theme-aware: adapts to dark and light modes using theme constants.

use eframe::egui;
use egui::{Color32, RichText, CornerRadius, Stroke};

use crate::state::{CLIOutput, CLIState, ModelState};
use crate::theme;

/// Render the CLI tab content
pub fn render(ui: &mut egui::Ui, cli: &mut CLIState, _models: &ModelState) {
    let dark = ui.visuals().dark_mode;
    let text_primary = if dark { theme::text() } else { theme::light::TEXT };
    let text_secondary = if dark { theme::text_secondary() } else { theme::light::TEXT_SECONDARY };
    let text_muted = if dark { theme::text_muted() } else { theme::light::TEXT_MUTED };
    let surface = if dark { theme::surface() } else { theme::light::SURFACE };
    let surface_elevated = if dark { theme::surface_elevated() } else { theme::light::SURFACE_ELEVATED };
    let border = if dark { theme::border() } else { theme::light::BORDER };

    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        // Header card
        render_cli_header(ui, dark, text_primary, text_secondary, surface, border);

        ui.add_space(12.0);

        // CLI output area
        let output_height = (ui.available_height() - 60.0).max(60.0);
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .max_height(output_height)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                if cli.outputs.is_empty() {
                    ui.add_space(20.0);
                    ui.label(RichText::new("No commands executed yet.").color(text_muted));
                    ui.add_space(10.0);
                    ui.label(RichText::new("Available commands:").strong().color(text_primary));
                    let cmds = [
                        ("list",           "List available models"),
                        ("loaded",         "List loaded models"),
                        ("load <model>",   "Load a model"),
                        ("unload <model>", "Unload a model"),
                        ("pull <model>",   "Pull/download a model"),
                        ("delete <model>", "Delete a model"),
                        ("ps",             "Show server status"),
                        ("clear",          "Clear output"),
                        ("help",           "Show this list (also rendered after `help`)"),
                    ];
                    for (cmd, desc) in cmds {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("  {:<18}", cmd)).size(12.0).color(theme::accent()).monospace());
                            ui.label(RichText::new(desc).size(12.0).color(text_secondary));
                        });
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Press Up / Down to recall past commands.")
                            .size(11.0)
                            .italics()
                            .color(text_muted),
                    );
                } else {
                    for output in &cli.outputs {
                        render_output(ui, output, dark, text_primary, text_muted, surface, surface_elevated, border);
                        ui.add_space(4.0);
                    }
                }
            });

        ui.add_space(8.0);

        // CLI input area
        render_input_area(ui, cli, dark, surface, border);
    });
}

/// Render CLI header
fn render_cli_header(
    ui: &mut egui::Ui,
    dark: bool,
    text_primary: Color32,
    text_secondary: Color32,
    surface: Color32,
    border: Color32,
) {
    egui::Frame {
        inner_margin: egui::Margin::symmetric(14, 10),
        corner_radius: CornerRadius::same(8),
        fill: surface,
        stroke: Stroke::new(1.0, border),
        shadow: egui::epaint::Shadow {
            offset: [0, 1],
            blur: 3,
            spread: 0,
            color: Color32::from_black_alpha(if dark { 30 } else { 8 }),
        },
        ..Default::default()
    }
    .show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(">_").size(16.0).color(text_primary).monospace());
            ui.label(RichText::new("Terminal").size(15.0).strong().color(text_primary));

            ui.separator();

            ui.label(RichText::new("Commands:").size(11.0).color(text_secondary));

            let badge_bg = if dark {
                theme::surface_elevated()
            } else {
                theme::light::SURFACE_ELEVATED
            };
            for cmd in ["list", "load", "unload", "pull", "ps"] {
                egui::Frame {
                    inner_margin: egui::Margin::symmetric(6, 2),
                    corner_radius: CornerRadius::same(3),
                    fill: badge_bg,
                    ..Default::default()
                }
                .show(ui, |ui| {
                    ui.label(RichText::new(cmd).size(10.0).color(theme::accent()).monospace());
                });
            }
        });
    });
}

/// Render input area
fn render_input_area(
    ui: &mut egui::Ui,
    cli: &mut CLIState,
    dark: bool,
    surface: Color32,
    border: Color32,
) {
    egui::Frame {
        inner_margin: egui::Margin::symmetric(12, 10),
        corner_radius: CornerRadius::same(8),
        fill: surface,
        stroke: Stroke::new(1.0, border),
        shadow: egui::epaint::Shadow {
            offset: [0, -1],
            blur: 3,
            spread: 0,
            color: Color32::from_black_alpha(if dark { 20 } else { 6 }),
        },
        ..Default::default()
    }
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            // Terminal prompt badge
            egui::Frame {
                inner_margin: egui::Margin::symmetric(8, 4),
                corner_radius: CornerRadius::same(3),
                fill: theme::accent(),
                ..Default::default()
            }
            .show(ui, |ui| {
                ui.label(RichText::new("$").size(13.0).strong().color(Color32::WHITE).monospace());
            });

            ui.add_space(6.0);

            let response = ui.add(
                egui::TextEdit::singleline(&mut cli.input)
                    .desired_width(ui.available_width())
                    .hint_text("Enter command...  (Up/Down for history)")
                    .id_salt("cli_input"),
            );

            // Only auto-focus when nothing else holds focus. Previously
            // this fired unconditionally every frame, which stole focus
            // back from clicks on the CLI output area — making it
            // impossible to drag-select output text to copy. Matches the
            // pattern used in chat_tab.
            if ui.ctx().memory(egui::Memory::focused).is_none() {
                response.request_focus();
            }
        });
    });
}

/// Render a single CLI output. Wide signature carries the theme palette
/// (dark + 4 colours) plus the output content; refactoring to a Theme
/// struct adds a borrow without simplifying the call sites.
#[allow(clippy::too_many_arguments)]
fn render_output(
    ui: &mut egui::Ui,
    output: &CLIOutput,
    dark: bool,
    text_primary: Color32,
    text_muted: Color32,
    surface: Color32,
    surface_elevated: Color32,
    border: Color32,
) {
    let (stroke_color, output_color) = if output.is_error {
        (theme::error(), theme::error())
    } else if output.in_progress {
        (theme::accent(), theme::accent())
    } else {
        (border, text_primary)
    };

    // Subtle fill tint for errors/progress
    let fill = if output.is_error {
        theme::tinted(theme::error(), if dark { 15 } else { 8 })
    } else if output.in_progress {
        theme::tinted(theme::accent(), if dark { 15 } else { 8 })
    } else {
        surface
    };

    egui::Frame {
        inner_margin: egui::Margin::symmetric(12, 10),
        corner_radius: CornerRadius::same(6),
        fill,
        stroke: Stroke::new(1.0, stroke_color),
        ..Default::default()
    }
    .show(ui, |ui| {
        ui.set_width(ui.available_width());

        // Command header
        ui.horizontal(|ui| {
            // Timestamp badge
            egui::Frame {
                inner_margin: egui::Margin::symmetric(6, 2),
                corner_radius: CornerRadius::same(3),
                fill: surface_elevated,
                ..Default::default()
            }
            .show(ui, |ui| {
                ui.label(RichText::new(&output.timestamp).size(10.0).color(text_muted));
            });

            ui.add_space(6.0);
            ui.label(RichText::new("$").size(13.0).color(theme::accent()).monospace());
            ui.label(RichText::new(&output.command).size(13.0).strong().color(text_primary).monospace());

            if output.in_progress {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spinner();
                    ui.label(RichText::new("Running...").size(11.0).color(theme::accent()));
                });
            }
        });

        ui.add_space(6.0);

        // Output text — selectable so users can drag-select + Ctrl+C
        // to copy individual entries (a CLI tab where you can't copy
        // the output is half useless).
        ui.add(
            egui::Label::new(RichText::new(&output.output).size(12.0).color(output_color))
                .selectable(true),
        );
    });
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
