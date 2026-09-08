//! Settings shared types + modal renderers.
//!
//! The legacy side-panel `render()` and its 9 helper fns were superseded
//! by `ui::settings::render` and have been deleted. The server-config
//! editor is now rendered inline by ui::settings (auto-populates
//! settings_state.config_editor on demand); the previous standalone
//! `render_config_editor_modal` wrapper was removed in 3f9fabe because
//! wiring it as a modal in app.rs caused the editor to render twice.
//! What remains here:
//!   - `SettingsState` / `SettingsAction` types (consumed by ui::settings
//!     and ui::models for action dispatch).
//!   - `render_edit_modal` — profile edit dialog.

use eframe::egui;
use egui::RichText;

use crate::config::{ApiProfile, ApiType, AppConfig};
use crate::config_editor::ConfigEditorState;

/// Settings panel state for handling edits
pub struct SettingsState {
    pub url_input: String,
    pub editing_profile: Option<usize>,
    pub edit_profile_data: Option<ApiProfile>,
    pub config_editor: Option<ConfigEditorState>,
    /// True when the user clicked "Reset GUI defaults" but hasn't
    /// confirmed yet. Renders a centered confirmation Window in
    /// ui::settings::render that wipes the AppConfig on Confirm or
    /// clears the flag on Cancel/Escape. Reset blasts away profiles,
    /// server URL, and selected_model — destructive enough to warrant
    /// a guard against accidental clicks.
    pub reset_confirm_pending: bool,
}

impl SettingsState {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            url_input: config.server_url.clone(),
            editing_profile: None,
            edit_profile_data: None,
            config_editor: None,
            reset_confirm_pending: false,
        }
    }
}

/// Actions that can be triggered from the settings panel.
///
/// SaveConfig carries the whole Config (~hundreds of bytes); the other arms
/// are short Strings or unit. Same large_enum_variant rationale as
/// ConfigEditorAction — emitted per UI tick, drained immediately.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum SettingsAction {
    RefreshModels,
    LoadModel(String),
    UnloadModel(String),
    DeleteModel(String),
    PullModel(String, String), // (model_name, source)
    SaveConfig(crate::config_editor::Config),
}

/// Render profile edit modal
pub fn render_edit_modal(
    ctx: &egui::Context,
    config: &mut AppConfig,
    settings_state: &mut SettingsState,
) {
    if let Some(edit_idx) = settings_state.editing_profile {
        if let Some(profile) = settings_state.edit_profile_data.as_mut() {
            let mut close_modal = false;
            let mut save_and_close = false;

            egui::Window::new("Edit Profile")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(400.0);

                    // Profile Name
                    ui.label(RichText::new("Profile Name").strong());
                    ui.add(
                        egui::TextEdit::singleline(&mut profile.name).desired_width(f32::INFINITY),
                    );

                    ui.add_space(8.0);

                    // API Type. selectable_value already mutates profile.api_type
                    // when the user clicks; the empty `if .clicked() {}` checks
                    // around each call were no-ops (clippy::if_then_empty).
                    ui.label(RichText::new("API Type").strong());
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut profile.api_type, ApiType::Loken, "LOKEN");
                        ui.selectable_value(&mut profile.api_type, ApiType::Ollama, "Ollama");
                        ui.selectable_value(&mut profile.api_type, ApiType::OpenApi, "OpenAPI");
                    });

                    ui.add_space(8.0);

                    // Server URL
                    ui.label(RichText::new("Server URL").strong());
                    ui.add(
                        egui::TextEdit::singleline(&mut profile.server_url)
                            .desired_width(f32::INFINITY)
                            .hint_text(crate::config::DEFAULT_LOKEN_URL),
                    );

                    ui.add_space(8.0);

                    // Default Model
                    ui.label(RichText::new("Default Model").strong());
                    ui.add(
                        egui::TextEdit::singleline(&mut profile.model)
                            .desired_width(f32::INFINITY)
                            .hint_text("model:tag"),
                    );

                    ui.add_space(12.0);

                    // API-specific parameters
                    render_profile_params(ui, profile);

                    ui.add_space(12.0);

                    // Save needs a non-empty profile name — the API
                    // profile dropdown keys off `name` and an empty
                    // value renders as an unselectable blank row that
                    // the user then can't tell apart from other
                    // profiles. Disable Save with a tooltip pointing
                    // at the empty field instead of silently letting
                    // them save garbage.
                    let name_valid = !profile.name.trim().is_empty();

                    // Buttons. Save on the left so Tab order from
                    // the form lands on it first (primary action).
                    ui.horizontal(|ui| {
                        let save_resp = ui.add_enabled(name_valid, egui::Button::new("Save"));
                        let tip = if name_valid {
                            "Save changes and close the modal\n\n\
                             Keyboard: Enter to save, Esc to cancel"
                        } else {
                            "Profile name is required — type a label \
                             in the field above to enable Save."
                        };
                        if save_resp.on_hover_text(tip).clicked() {
                            save_and_close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });

            // Keyboard shortcuts. Esc closes without saving (standard
            // dialog convention); Enter saves when the form is valid.
            // Read after the Window block so they see the same input
            // frame the modal just consumed — without this, key events
            // from the TextEdits below leak through to chat-tab key
            // handlers on the next frame.
            ctx.input(|i| {
                if i.key_pressed(egui::Key::Escape) {
                    close_modal = true;
                }
                // Enter saves only when name validation passes, matching
                // the disabled-button gate. Plain Enter (no Shift/Ctrl)
                // so multi-line Server-URL paste with Shift+Enter still
                // works if the underlying TextEdit ever gains multiline.
                if i.key_pressed(egui::Key::Enter)
                    && !i.modifiers.any()
                    && !profile.name.trim().is_empty()
                {
                    save_and_close = true;
                }
            });

            if save_and_close {
                if let Some(edited) = settings_state.edit_profile_data.take() {
                    config.profiles[edit_idx] = edited;
                    config.save();
                }
                settings_state.editing_profile = None;
                settings_state.edit_profile_data = None;
            } else if close_modal {
                settings_state.editing_profile = None;
                settings_state.edit_profile_data = None;
            }
        }
    }
}

/// Render profile-specific parameters
fn render_profile_params(ui: &mut egui::Ui, profile: &mut ApiProfile) {
    match profile.api_type {
        ApiType::Ollama => {
            ui.label(RichText::new("Ollama Parameters").strong());
            ui.horizontal(|ui| {
                ui.label("Temperature:");
                ui.add(
                    egui::Slider::new(&mut profile.ollama_params.temperature, 0.0..=2.0)
                        .step_by(0.1),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Top P:");
                ui.add(
                    egui::Slider::new(&mut profile.ollama_params.top_p, 0.0..=1.0).step_by(0.05),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Top K:");
                ui.add(egui::DragValue::new(&mut profile.ollama_params.top_k).range(1..=100));
            });
            ui.horizontal(|ui| {
                ui.label("Repeat Penalty:");
                ui.add(
                    egui::Slider::new(&mut profile.ollama_params.repeat_penalty, 1.0..=2.0)
                        .step_by(0.1),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Context Length:");
                ui.add(egui::DragValue::new(&mut profile.ollama_params.num_ctx).range(512..=32768));
            });
            ui.horizontal(|ui| {
                ui.label("Max Tokens:");
                ui.add(
                    egui::DragValue::new(&mut profile.ollama_params.num_predict).range(-1..=32768),
                );
                ui.label(RichText::new("(-1 = unlimited)").weak());
            });
            ui.horizontal(|ui| {
                ui.label("Seed:");
                ui.add(egui::DragValue::new(&mut profile.ollama_params.seed).range(-1..=i64::MAX));
                ui.label(RichText::new("(-1 = random)").weak());
            });
            ui.horizontal(|ui| {
                ui.label("Stop:");
                ui.add(
                    egui::TextEdit::singleline(&mut profile.ollama_params.stop)
                        .hint_text("comma-separated, e.g. \\n\\n, ###"),
                );
            });
        }
        ApiType::OpenApi => {
            ui.label(RichText::new("OpenAPI Parameters").strong());
            ui.horizontal(|ui| {
                ui.label("Temperature:");
                ui.add(
                    egui::Slider::new(&mut profile.openapi_params.temperature, 0.0..=2.0)
                        .step_by(0.1),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Max Tokens:");
                let mut max_tokens = profile.openapi_params.max_tokens.unwrap_or(1024);
                if ui
                    .add(egui::DragValue::new(&mut max_tokens).range(1..=8192))
                    .changed()
                {
                    profile.openapi_params.max_tokens = Some(max_tokens);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Top P:");
                ui.add(
                    egui::Slider::new(&mut profile.openapi_params.top_p, 0.0..=1.0).step_by(0.05),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Frequency Penalty:");
                ui.add(
                    egui::Slider::new(&mut profile.openapi_params.frequency_penalty, -2.0..=2.0)
                        .step_by(0.1),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Presence Penalty:");
                ui.add(
                    egui::Slider::new(&mut profile.openapi_params.presence_penalty, -2.0..=2.0)
                        .step_by(0.1),
                );
            });
        }
        ApiType::Loken => {
            ui.label(RichText::new("LLM Server Parameters").strong());
            ui.horizontal(|ui| {
                ui.label("Context Length:");
                ui.add(
                    egui::DragValue::new(&mut profile.loken_params.context_length)
                        .range(512..=32768),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Temperature:");
                ui.add(
                    egui::Slider::new(&mut profile.loken_params.temperature, 0.0..=2.0)
                        .step_by(0.1),
                );
            });
        }
    }
}

// render_config_editor_modal removed: server config editing happens
// inline in ui::settings::render reading settings_state.config_editor
// directly. The modal form was duplicate UI and the auto-init at
// ui/settings.rs:30 meant it opened every time the Settings panel
// rendered.
