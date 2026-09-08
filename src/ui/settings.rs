//! Settings section: one column of section panels, every field labelled in
//! capitals and explained by a caption.
//!
//! Reads/writes: AppConfig, SettingsState, ConfigEditorState

use eframe::egui::{self, TextEdit};
#[allow(unused_imports)]
use crate::config::{AppConfig, ApiProfile, ApiType};
use crate::config_editor::ConfigEditorState;
use crate::icons::Icon;
use crate::settings::{SettingsAction, SettingsState};
use crate::theme::{self, text};
use crate::ui::widgets;

/// Point size of the icons in the chrome row and the picker.
const ICON_PT: f32 = 14.0;
/// Width of the column of panels.
const SETTINGS_COLUMN_W: f32 = 640.0;
/// Room kept beside a field for the button that follows it.
const FIELD_BUTTON_RESERVE: f32 = 70.0;
/// Width of the profile picker, of the API type cell, and of a confirmation.
const PROFILE_PICKER_W: f32 = 250.0;
const API_TYPE_W: f32 = 64.0;
const CONFIRM_W: f32 = 380.0;
/// The profile popup's height.
const PROFILE_POPUP_H: f32 = 280.0;
/// The folder picker: its default size, the room kept for its action row,
/// the least height of its list, the room beside its path field.
const PICKER_W: f32 = 520.0;
const PICKER_H: f32 = 420.0;
const PICKER_ACTIONS_RESERVE: f32 = 80.0;
const PICKER_LIST_MIN_H: f32 = 120.0;
const PICKER_GO_RESERVE: f32 = 50.0;

/// Render the settings section
pub fn render(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    settings_state: &mut SettingsState,
    connection_state: crate::state::ConnectionState,
) -> Vec<SettingsAction> {
    let mut actions = Vec::new();

    // Auto-initialize config editor state if not yet loaded
    if settings_state.config_editor.is_none() {
        if let Ok(cfg) = crate::config_editor::Config::load_default() {
            settings_state.config_editor = Some(ConfigEditorState::from_config(&cfg));
        }
    }

    // In-app folder picker modal (independent of rfd/portal/zenity).
    // Renders when editor.inapp_picker is Some — opens via the "..."
    // button next to Browse, lets the user navigate with parent / sub
    // links + a "Use this folder" commit button, and writes the result
    // straight into the editor field. Always works, even on systems
    // where the native picker is broken.
    if let Some(editor) = settings_state.config_editor.as_mut() {
        if editor.inapp_picker.is_some() {
            render_inapp_folder_picker(ctx, editor);
        }
    }

    // (Previous rfd worker-thread drain removed — Browse now uses the
    // pure-egui in-app picker rendered above. No more pending_browse /
    // pending_browse_error Mutex round-trip.)

    // Reset GUI defaults confirmation modal. Two-step confirm matches
    // the pattern used for the model-delete confirmation (41088d1) —
    // single-click Reset would wipe profiles, server URL, and the
    // selected model without recovery, so guard against fat-fingers
    // and accidental clicks.
    if settings_state.reset_confirm_pending {
        let mut close_modal = false;
        let mut confirmed = false;
        egui::Window::new("Reset GUI defaults")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(CONFIRM_W);
                ui.add_space(widgets::GAP_LABEL);
                ui.label(text::value("Reset all GUI settings to defaults?"));
                ui.add_space(widgets::GAP_WIDGETS);
                ui.label(text::note(
                    "This wipes:\n  - All API profiles\n  - Server URL\n  - Selected model\n  - Theme / window preferences",
                ));
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::caption_row(ui, "Local model files on disk are not affected.");
                widgets::caption_row(
                    ui,
                    "A backup of the current config.json is saved as \
                     config.json.before_reset.<timestamp> in the same folder, so you can \
                     restore it manually if needed.",
                );
                ui.add_space(widgets::GAP_WIDGETS);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(text::note("Cancel"))).clicked() {
                        close_modal = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let reset_btn = egui::Button::new(text::value("Reset").color(theme::on_accent()))
                            .fill(theme::error());
                        if ui.add(reset_btn).clicked() {
                            confirmed = true;
                        }
                    });
                });
                ui.add_space(widgets::GAP_LABEL);
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close_modal = true;
        }
        if confirmed {
            // Snapshot the current config.json to
            // <path>.before_reset.<unix_ts> so an accidental reset
            // can be undone by copying the backup back. Best-effort:
            // failures log + the reset still proceeds (a backup
            // hiccup shouldn't block the user's intent).
            if let Some(dirs) = directories::ProjectDirs::from("com", "loken", "atelier") {
                AppConfig::backup_before_reset(dirs.config_dir());
            }
            *config = AppConfig::default();
            settings_state.url_input = config.server_url.clone();
            config.save();
            settings_state.reset_confirm_pending = false;
        } else if close_modal {
            settings_state.reset_confirm_pending = false;
        }
    }

    widgets::chrome_row(ui, |ui| {
        Icon::Gear.show(ui, ICON_PT, theme::ink());
        ui.label(text::title("Settings"));
    });
    ui.add_space(widgets::GAP_WIDGETS);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.set_width(ui.available_width());

        // ── Server Connection ──
        settings_panel(ui, "CONNECTION", |ui| {
            form_row(ui, "SERVER URL", |ui| {
                // Apply button width budget so the text field doesn't
                // hug the right edge of the card.
                let resp = ui.add(
                    TextEdit::singleline(&mut settings_state.url_input)
                        .desired_width(ui.available_width() - FIELD_BUTTON_RESERVE),
                );
                let trimmed_input = settings_state.url_input.trim();
                let non_empty = !trimmed_input.is_empty();
                let dirty = settings_state.url_input != config.server_url;
                let can_apply = dirty && non_empty;
                // Explicit Apply button next to the field. Previously
                // the URL change was applied only on Enter — easy to
                // miss for users who edit then mouse away to Quick
                // Connect or another card, losing the typed value.
                // Gated on can_apply (= dirty + non-empty) so an empty
                // field can never clobber the saved server URL.
                let apply_btn = egui::Button::new(text::value("Apply").color(theme::on_accent()))
                    .fill(theme::accent());
                let apply_resp = ui.add_enabled(can_apply, apply_btn);
                let apply_clicked = apply_resp.clicked();
                let apply_tip = if dirty && !non_empty {
                    "URL is empty: type a server URL to enable Apply.".to_string()
                } else if can_apply {
                    format!("Apply '{}' as the active server URL", trimmed_input)
                } else {
                    String::new()
                };
                if !apply_tip.is_empty() {
                    apply_resp.on_hover_text(apply_tip);
                }
                let enter_pressed = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (enter_pressed || apply_clicked) && can_apply {
                    // Auto-prefix http:// when the user types a bare
                    // host:port (e.g. "localhost:11434"). reqwest
                    // requires a scheme — without one, every refresh
                    // would surface the "invalid URL" error from the
                    // request layer. Trim whitespace too since URLs
                    // pasted from clipboards often carry trailing \n.
                    //
                    // Also strip trailing slashes so the persisted URL
                    // matches the canonical form Client::new produces
                    // (a paste like "http://localhost:11435/" would
                    // otherwise produce double-slash request paths).
                    // Client::new defenses against this too — fixing
                    // it here keeps the saved config canonical so the
                    // input field doesn't visually keep the slash
                    // after the next round-trip.
                    let mut url = settings_state.url_input.trim()
                        .trim_end_matches('/')
                        .to_string();
                    if !url.is_empty()
                        && !url.starts_with("http://")
                        && !url.starts_with("https://")
                    {
                        url = format!("http://{}", url);
                    }
                    settings_state.url_input = url.clone();
                    config.server_url = url;
                    config.save();
                }
            });
            widgets::caption_row(ui, "The web address where your AI model is running. Press Enter or click Apply to connect. Use the Quick Connect buttons below for common setups.");
            ui.add_space(widgets::GAP_LABEL);

            // API key. Only needed when the server has `require_auth` on; without this
            // field, turning authentication on server-side would lock this app out with
            // a bare 401 and no way to fix it from the UI.
            form_row(ui, "API KEY", |ui| {
                let mut key = config.api_key.clone().unwrap_or_default();
                let resp = ui.add(
                    TextEdit::singleline(&mut key)
                        .password(true)
                        .hint_text("only if the server requires one")
                        .desired_width(ui.available_width()),
                );
                if resp.changed() {
                    // Blank means absent, so clearing the field really removes the
                    // credential rather than sending an empty one.
                    let trimmed = key.trim();
                    config.api_key =
                        if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
                    // No client invalidation needed: get_client() builds a fresh one per
                    // request, so the next call already carries the new key.
                }
            });
            widgets::caption_row(ui, "Sent as a bearer token on every request. Leave empty unless the server is configured with require_auth.");
            ui.add_space(widgets::GAP_LABEL);

            form_row(ui, "STATUS", |ui| {
                // Colour + label both derive from the ConnectionState
                // enum — no more `.contains()` sniffing that could
                // desync the dot from the text.
                let lit = connection_state != crate::state::ConnectionState::Disconnected;
                widgets::lamp_inline(ui, lit, connection_state.color());
                ui.label(text::note(connection_state.label()));
            });

            ui.add_space(widgets::GAP_LABEL);

            ui.horizontal(|ui| {
                ui.label(text::label("QUICK CONNECT"));
                for (label, url) in [
                    ("LOKEN", crate::config::DEFAULT_LOKEN_URL),
                    ("OLLAMA", crate::config::DEFAULT_OLLAMA_URL),
                ] {
                    // Highlight based on the SAVED server URL, not
                    // the in-progress input. Typing in the field
                    // shouldn't desync the active-preset indicator —
                    // it just means the user is composing a new URL,
                    // not that they've abandoned the current one.
                    let is_active = config.server_url == url;
                    let btn_resp = widgets::selector_pill(ui, label, is_active);
                    btn_resp.clone().on_hover_ui(|ui| {
                        // Lazy tooltip — format! only runs when the
                        // user is actually hovering, not every frame.
                        ui.label(format!("Use {} ({})", label, url));
                    });
                    if btn_resp.clicked() {
                        settings_state.url_input = url.to_string();
                        config.server_url = url.to_string();
                        config.save();
                    }
                }
            });
        });

        ui.add_space(widgets::GAP_WIDGETS);

        // ── Appearance ──
        settings_panel(ui, "APPEARANCE", |ui| {
            form_row(ui, "THEME", |ui| {
                for (label, value) in [("DARK", true), ("LIGHT", false)] {
                    let is_active = config.dark_theme == value;
                    if widgets::selector_pill(ui, label, is_active).clicked() && config.dark_theme != value {
                        config.dark_theme = value;
                        theme::apply(ctx, value);
                        config.save();
                    }
                }
            });
        });

        ui.add_space(widgets::GAP_WIDGETS);

        // ── API Profiles ──
        settings_panel(ui, "API PROFILES", |ui| {
            form_row(ui, "ACTIVE PROFILE", |ui| {
                egui::ComboBox::from_id_salt("profile_selector_settings")
                    .selected_text(config.selected_profile.as_deref().unwrap_or("(none)"))
                    .width(ui.available_width().min(PROFILE_PICKER_W))
                    .show_ui(ui, |ui| {
                        // Wrap in ScrollArea so a user with many
                        // profiles (10+ via "+ New Profile" clicks
                        // over time) doesn't get a popup taller than
                        // the screen. Matches the chat-header model
                        // dropdown cap from commit 4353f8f.
                        //
                        // Iterate `config.profiles` directly here
                        // (inside the show_ui closure, which only
                        // fires when the popup is OPEN) instead of
                        // pre-cloning a Vec<String> every frame on
                        // the Settings tab. The pre-clone allocated
                        // N strings on every render whether or not
                        // the user opened the dropdown — pure waste
                        // for a popup that's open <1% of the time.
                        // selectable_value still needs the per-entry
                        // String clone (it takes the value by value
                        // since Option<String>: !Copy), but that
                        // now only fires inside the open popup.
                        egui::ScrollArea::vertical()
                            .max_height(PROFILE_POPUP_H)
                            .show(ui, |ui| {
                                // Disjoint borrow: split field borrows
                                // through the closure (Rust 2021+
                                // captures by field) so we can mutate
                                // selected_profile while iterating
                                // profiles read-only.
                                let selected = &mut config.selected_profile;
                                for p in &config.profiles {
                                    ui.selectable_value(
                                        selected,
                                        Some(p.name.clone()),
                                        &p.name,
                                    );
                                }
                            });
                    });
            });

            // Resolve the selected profile's index from its name without
            // cloning either the Option<String> or the ApiProfile struct
            // every frame. The render closure below borrows
            // &config.profiles[profile_idx] for read-only display; the
            // rare Delete/Edit button clicks set flags that get applied
            // after the closure exits, releasing the borrow so the
            // mutations can land.
            let profile_idx = config
                .selected_profile
                .as_ref()
                .and_then(|name| config.profiles.iter().position(|p| &p.name == name));
            if let Some(profile_idx) = profile_idx {
                let mut delete_clicked = false;
                let mut edit_clicked = false;
                let profile = &config.profiles[profile_idx];
                let profiles_count = config.profiles.len();
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::well(ui, |ui| {
                    ui.horizontal(|ui| {
                        widgets::fixed_label(ui, API_TYPE_W, text::label(&format!("{}", profile.api_type)));
                        ui.label(text::readout(&profile.server_url));
                        // Right-anchored Edit + Delete action row.
                        // Previously there was no way to edit a
                        // profile or delete one from the inline
                        // view — the render_edit_modal exists in
                        // settings.rs but had no trigger surface,
                        // so users were stuck with the defaults
                        // applied at + New Profile creation.
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Disable Delete when this is the only
                            // profile — leaving the user with zero
                            // profiles requires a Reset (or +New)
                            // to recover and is rarely intended.
                            let can_delete = profiles_count > 1;
                            let del_btn = egui::Button::new(text::note("Delete").color(theme::error())).frame(false);
                            let del_tip = if can_delete {
                                "Delete this profile"
                            } else {
                                "Can't delete the last profile (add another first)"
                            };
                            if ui
                                .add_enabled(can_delete, del_btn)
                                .on_hover_text(del_tip)
                                .clicked()
                            {
                                delete_clicked = true;
                            }
                            ui.add_space(widgets::GAP_LABEL);
                            let edit_btn = egui::Button::new(text::note("Edit")).frame(false);
                            if ui.add(edit_btn).on_hover_text("Open the profile editor").clicked() {
                                edit_clicked = true;
                            }
                        });
                    });
                    ui.add_space(widgets::GAP_LABEL);
                    match profile.api_type {
                        ApiType::Ollama | ApiType::Loken => {
                            param_grid(ui, &[
                                ("TEMPERATURE", &format!("{:.2}", profile.ollama_params.temperature)),
                                ("TOP P", &format!("{:.2}", profile.ollama_params.top_p)),
                                ("TOP K", &format!("{}", profile.ollama_params.top_k)),
                                ("CONTEXT", &format!("{}", profile.ollama_params.num_ctx)),
                            ]);
                        }
                        ApiType::OpenApi => {
                            param_grid(ui, &[
                                ("TEMPERATURE", &format!("{:.2}", profile.openapi_params.temperature)),
                                ("MAX TOKENS", &format!("{:?}", profile.openapi_params.max_tokens)),
                                ("TOP P", &format!("{:.2}", profile.openapi_params.top_p)),
                            ]);
                        }
                    }
                });
                // The render-closure borrow on config is dropped here.
                // Apply any deferred mutations.
                if delete_clicked {
                    config.profiles.remove(profile_idx);
                    config.selected_profile = config
                        .profiles
                        .first()
                        .map(|p| p.name.clone());
                    config.save();
                } else if edit_clicked {
                    settings_state.editing_profile = Some(profile_idx);
                    settings_state.edit_profile_data =
                        Some(config.profiles[profile_idx].clone());
                }
            }

            ui.add_space(widgets::GAP_WIDGETS);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(text::note("New profile"))).clicked() {
                    // Pick a name that doesn't collide with any
                    // existing profile. The previous
                    // `format!("Profile {}", len + 1)` could yield
                    // a duplicate after a middle-delete (e.g.
                    // delete "Default" from ["Default","Profile 2"]
                    // leaves ["Profile 2"], len+1 = 2 → collision).
                    // Helper lives in AppConfig so it has a unit-
                    // test surface — see config::tests::
                    // next_profile_name_avoids_collision.
                    let new_name = AppConfig::next_profile_name(&config.profiles);
                    config.profiles.push(ApiProfile::new(
                        new_name.clone(), ApiType::Loken,
                        crate::config::DEFAULT_LOKEN_URL.to_string(),
                        "default".to_string(),
                    ));
                    config.selected_profile = Some(new_name);
                    config.save();
                }
            });
        });

        ui.add_space(widgets::GAP_WIDGETS);

        // ── Server Configuration (config.toml) — inline ──
        if let Some(editor) = settings_state.config_editor.as_mut() {
            // Error banner
            if let Some(error) = &editor.error_message {
                ui.allocate_ui(egui::vec2(SETTINGS_COLUMN_W, 0.0), |ui| {
                    widgets::panel_frame(ui, |ui| {
                        ui.horizontal(|ui| {
                            widgets::lamp_inline(ui, true, theme::error());
                            ui.label(text::note(error).color(theme::error()));
                        });
                    });
                });
                ui.add_space(widgets::GAP_WIDGETS);
            }

            // Model Directories — Browse buttons spawn worker threads
            // for the file dialog rather than blocking the egui main
            // thread. On Linux the synchronous rfd path deadlocks
            // against the XDG portal because the portal needs the main
            // loop alive to dispatch events to it. Two non-obvious
            // pieces required to make this work reliably:
            //
            //   1. set_directory only fires for non-empty start paths.
            //      Passing "" through to rfd causes the XDG portal to
            //      reject the FileChooser request (returns immediately
            //      without showing UI) — looks identical to "the button
            //      did nothing" from the user's perspective.
            //
            //   2. ctx.request_repaint() must fire on the worker thread
            //      AFTER writing the result. The drain in render() only
            //      sees the new slot value if a frame draws — without
            //      a repaint signal, egui sleeps until the user moves
            //      the mouse, and the picked path silently sits in the
            //      Mutex unconsumed.
            // Browse uses the in-app folder picker (pure egui, no rfd
            // / XDG portal / zenity dependency) — guaranteed to work
            // regardless of the desktop environment. Removed the
            // previous Browse + "…" two-button layout: showing two
            // affordances that do nearly the same thing was confusing.
            // The rfd path is gone with it; no more silent-None failures
            // when zenity isn't installed.
            settings_panel(ui, "MODEL DIRECTORIES", |ui| {
                ui.label(text::label("OLLAMA MODELS"));
                ui.horizontal(|ui| {
                    TextEdit::singleline(&mut editor.ollama_models_dir)
                        .desired_width(ui.available_width() - FIELD_BUTTON_RESERVE)
                        .hint_text("~/.ollama/models")
                        .show(ui);
                    if ui
                        .add(egui::Button::new(text::note("Browse")))
                        .on_hover_text("Open the folder picker")
                        .clicked()
                    {
                        let start = inapp_picker_start(&editor.ollama_models_dir);
                        editor.inapp_picker = Some((
                            crate::config_editor::BrowseTarget::OllamaDir,
                            start,
                        ));
                    }
                });
                widgets::caption_row(ui, "Folder where Ollama keeps its downloaded models. Leave empty to use the standard location.");
                ui.add_space(widgets::GAP_LABEL);

                ui.label(text::label("HUGGINGFACE MODELS"));
                ui.horizontal(|ui| {
                    TextEdit::singleline(&mut editor.huggingface_models_dir)
                        .desired_width(ui.available_width() - FIELD_BUTTON_RESERVE)
                        .hint_text("~/.cache/huggingface/hub")
                        .show(ui);
                    if ui
                        .add(egui::Button::new(text::note("Browse")))
                        .on_hover_text("Open the folder picker")
                        .clicked()
                    {
                        let start = inapp_picker_start(&editor.huggingface_models_dir);
                        editor.inapp_picker = Some((
                            crate::config_editor::BrowseTarget::HuggingFaceDir,
                            start,
                        ));
                    }
                });
                widgets::caption_row(ui, "Folder where HuggingFace models are cached after download. Leave empty to use the standard location.");
            });

            ui.add_space(widgets::GAP_WIDGETS);

            // Server bind
            settings_panel(ui, "SERVER BIND", |ui| {
                config_row_pair(ui,
                    ("HOST", &mut editor.server_host),
                    ("PORT", &mut editor.server_port_str),
                );
                hint_row(ui,
                    "Use 127.0.0.1 for this computer only, or 0.0.0.0 to allow connections from other devices on your network",
                    "Port number the server listens on. Default 11435. Change if another app already uses this port.",
                );
            });

            ui.add_space(widgets::GAP_WIDGETS);

            // Inference
            settings_panel(ui, "INFERENCE", |ui| {
                ui.label(text::label("MODEL ID"));
                TextEdit::singleline(&mut editor.model_id)
                    .desired_width(ui.available_width())
                    .show(ui);
                widgets::caption_row(ui, "Name of the AI model to load when the server starts. Must match a model you have downloaded (e.g. devstral-small-2).");
                ui.add_space(widgets::GAP_WIDGETS);

                config_row_pair(ui,
                    ("MAX TOKENS", &mut editor.max_tokens_str),
                    ("CONTEXT LENGTH", &mut editor.context_length_str),
                );
                hint_row(ui,
                    "Limits how long each response can be. Higher = longer answers but slower.",
                    "How much conversation history the model can remember. Larger values use more memory.",
                );
                ui.add_space(widgets::GAP_LABEL);
                config_row_pair(ui,
                    ("TEMPERATURE", &mut editor.temperature_str),
                    ("TOP P", &mut editor.top_p_str),
                );
                hint_row(ui,
                    "Controls creativity. Low (0.1) = focused and predictable. High (1.5) = creative and varied.",
                    "Filters unlikely words. Lower values (e.g. 0.5) make output more focused. Usually fine at 0.9.",
                );
                ui.add_space(widgets::GAP_LABEL);
                config_row_pair(ui,
                    ("TOP K", &mut editor.top_k_str),
                    ("SEED", &mut editor.seed_str),
                );
                hint_row(ui,
                    "How many word choices the model considers at each step. Lower = more predictable.",
                    "A number that makes outputs repeatable. Same seed + same prompt = same answer.",
                );
            });

            ui.add_space(widgets::GAP_WIDGETS);

            // GPU & Device
            settings_panel(ui, "GPU AND DEVICE", |ui| {
                config_row_pair(ui,
                    ("DEVICE INDEX", &mut editor.device_index_str),
                    ("GPU MEMORY %", &mut editor.max_gpu_memory_fraction_str),
                );
                hint_row(ui,
                    "Which GPU to use if you have multiple. Leave empty to pick automatically.",
                    "How much of your GPU's memory to use (0.9 = 90%). Lower this if you get out-of-memory errors.",
                );
                ui.add_space(widgets::GAP_LABEL);
                config_row_pair(ui,
                    ("FORCE GPU LAYERS", &mut editor.force_gpu_layers_str),
                    ("CPU THREADS", &mut editor.cpu_threads_str),
                );
                hint_row(ui,
                    "Force a specific number of model layers onto the GPU. Leave empty to let the server decide based on available memory.",
                    "Number of CPU cores to use for processing. Set to 0 to use all available cores automatically.",
                );
                ui.add_space(widgets::GAP_LABEL);
                ui.checkbox(&mut editor.use_quantized_gpu, text::note("Use quantized GPU"));
                widgets::caption_row(ui, "Compresses the model to use less GPU memory at a small quality cost. Recommended if your GPU has limited memory (8 GB or less).");
            });

            ui.add_space(widgets::GAP_WIDGETS);

            // Save / Reload / Reset row
            settings_panel(ui, "ACTIONS", |ui| {
                ui.horizontal(|ui| {
                    // Save config.toml
                    let save_btn = egui::Button::new(text::value("Save config.toml").color(theme::on_accent()))
                        .fill(theme::accent());
                    if ui
                        .add(save_btn)
                        .on_hover_text(
                            "Write the model-directory + server + inference parameters\n\
                             above to config.toml. Requires a server restart for the\n\
                             server-config changes to take effect.",
                        )
                        .clicked()
                    {
                        match editor.to_config() {
                            Ok(cfg) => {
                                actions.push(SettingsAction::SaveConfig(cfg));
                                editor.error_message = None;
                            }
                            Err(e) => {
                                editor.error_message = Some(e);
                            }
                        }
                    }

                    ui.add_space(widgets::GAP_WIDGETS);

                    // Reload from disk
                    let reload_btn = egui::Button::new(text::note("Reload from disk"));
                    if ui
                        .add(reload_btn)
                        .on_hover_text(
                            "Discard any unsaved edits in this section and reload\n\
                             the values from config.toml on disk.",
                        )
                        .clicked()
                    {
                        if let Ok(cfg) = crate::config_editor::Config::load_default() {
                            *editor = ConfigEditorState::from_config(&cfg);
                        }
                    }

                    ui.add_space(widgets::GAP_WIDGETS);

                    // Reset GUI defaults — two-step confirm via modal,
                    // since this wipes profiles + server URL + selected
                    // model in one click. The modal lives at the top
                    // of this render fn; this button only sets the
                    // pending flag. While the modal is already open
                    // disable the button so re-clicking it doesn't
                    // re-trigger or visually hint that a second click
                    // is needed.
                    let reset_btn = egui::Button::new(text::note("Reset GUI defaults").color(theme::error()));
                    if ui
                        .add_enabled(!settings_state.reset_confirm_pending, reset_btn)
                        .clicked()
                    {
                        settings_state.reset_confirm_pending = true;
                    }
                });
            });
        }
    });

    actions
}

// ── Helper components ──

/// A section panel in the settings column.
fn settings_panel(ui: &mut egui::Ui, caps: &str, add_body: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui(egui::vec2(SETTINGS_COLUMN_W, 0.0), |ui| {
        widgets::section_panel(ui, caps, add_body);
    });
}

/// A form row: the label in capitals in a column of fixed width, right
/// aligned, and the content beside it.
fn form_row(ui: &mut egui::Ui, caps: &str, add_content: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui(egui::vec2(widgets::FORM_LABEL_W, ui.spacing().interact_size.y), |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(widgets::GAP_WIDGETS);
                ui.label(text::label(caps));
            });
        });
        add_content(ui);
    });
}

/// Key and value pairs in two columns: the key in capitals, the value in
/// monospace.
fn param_grid(ui: &mut egui::Ui, params: &[(&str, &str)]) {
    ui.columns(2, |cols| {
        for (i, (key, val)) in params.iter().enumerate() {
            let col = &mut cols[i % 2];
            col.horizontal(|ui| {
                ui.label(text::label(key));
                ui.label(text::mono(val));
            });
        }
    });
}

/// Two labelled text fields side by side.
fn config_row_pair(
    ui: &mut egui::Ui,
    (caps_a, val_a): (&str, &mut String),
    (caps_b, val_b): (&str, &mut String),
) {
    ui.columns(2, |cols| {
        cols[0].label(text::label(caps_a));
        TextEdit::singleline(val_a).desired_width(f32::INFINITY).show(&mut cols[0]);

        cols[1].label(text::label(caps_b));
        TextEdit::singleline(val_b).desired_width(f32::INFINITY).show(&mut cols[1]);
    });
}

/// Two captions side by side, under a `config_row_pair`.
fn hint_row(ui: &mut egui::Ui, hint_a: &str, hint_b: &str) {
    ui.columns(2, |cols| {
        widgets::caption_row(&mut cols[0], hint_a);
        widgets::caption_row(&mut cols[1], hint_b);
    });
}

/// Pick a starting directory for the in-app folder picker. Falls
/// back through: current field value → $HOME → "/" so the picker
/// always opens on a valid directory.
fn inapp_picker_start(current: &str) -> std::path::PathBuf {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        let p = std::path::PathBuf::from(trimmed);
        if p.is_dir() {
            return p;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        return std::path::PathBuf::from(home);
    }
    std::path::PathBuf::from("/")
}

/// Render the in-app folder picker modal. Pure egui — no rfd, no XDG
/// portal, no zenity. Lists subdirectories of the current path, lets
/// the user navigate via ".." (parent), click any subdir to descend,
/// or "Use this folder" to commit the displayed path to the editor
/// field. Cancel / Escape closes without committing.
fn render_inapp_folder_picker(ctx: &egui::Context, editor: &mut ConfigEditorState) {
    let Some((target, current)) = editor.inapp_picker.clone() else { return };
    let mut new_path: Option<std::path::PathBuf> = None;
    let mut commit = false;
    let mut close = false;

    egui::Window::new("Choose a folder")
        .collapsible(false)
        .resizable(true)
        .default_width(PICKER_W)
        .default_height(PICKER_H)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // Editable path bar — lets the user paste a full path and
            // press Enter to jump there instead of clicking subfolders
            // one at a time. The Go button is disabled when the typed
            // path isn't a directory (visible feedback that the path
            // is wrong instead of silent "Enter does nothing").
            ui.horizontal(|ui| {
                ui.label(text::label("PATH"));
                let mut path_str = current.display().to_string();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut path_str)
                        .desired_width(ui.available_width() - PICKER_GO_RESERVE)
                        .font(egui::TextStyle::Monospace),
                );
                let typed_path = std::path::PathBuf::from(path_str.trim());
                let typed_valid = typed_path.is_dir();
                let go_btn_resp = ui.add_enabled(typed_valid, egui::Button::new(text::note("Go")));
                if !typed_valid && path_str.trim() != current.display().to_string() {
                    go_btn_resp.clone().on_hover_text("Path doesn't exist or isn't a directory");
                }
                let go = go_btn_resp.clicked()
                    || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && typed_valid);
                if go {
                    new_path = Some(typed_path);
                }
            });
            ui.add_space(widgets::GAP_LABEL);

            // Quick jumps: $HOME, /
            ui.horizontal(|ui| {
                if let Some(home) = std::env::var_os("HOME") {
                    let home_resp = ui.add(
                        egui::Button::image_and_text(Icon::Home.image(ICON_PT, theme::ink_dim()), text::note("Home"))
                            .frame(false),
                    );
                    if home_resp.on_hover_text("Jump to $HOME").clicked() {
                        new_path = Some(std::path::PathBuf::from(home));
                    }
                }
                if ui.add(egui::Button::new(text::note("/ Root")).frame(false)).on_hover_text("Jump to /").clicked() {
                    new_path = Some(std::path::PathBuf::from("/"));
                }
            });
            ui.add_space(widgets::GAP_LABEL);

            // The list, keeping room for the action row under it.
            let list_h = (ui.available_height() - PICKER_ACTIONS_RESERVE).max(PICKER_LIST_MIN_H);
            egui::ScrollArea::vertical()
                .id_salt("inapp_picker_scroll")
                .max_height(list_h)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    // Parent ("..") link — only renders when there IS
                    // a parent (we're not already at "/"). Plain ".."
                    // is the universal file-picker convention; no
                    // exotic-arrow Unicode required.
                    if current.parent().is_some()
                        && ui
                            .add(egui::Button::new(text::mono("..")).frame(false))
                            .clicked()
                    {
                        new_path = current.parent().map(std::path::PathBuf::from);
                    }

                    // List subdirectories, sorted alphabetically. Hide
                    // ones we can't read (permission denied) — they'd
                    // just frustrate the user with click-no-result.
                    let mut entries: Vec<std::path::PathBuf> = match std::fs::read_dir(&current) {
                        Ok(rd) => rd
                            .filter_map(|e| e.ok())
                            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                            .map(|e| e.path())
                            .collect(),
                        Err(e) => {
                            ui.colored_label(theme::error(), format!("Can't read directory: {}", e));
                            Vec::new()
                        }
                    };
                    entries.sort();

                    for entry in &entries {
                        let name = entry
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| entry.display().to_string());
                        // Show all directories including dotfiles —
                        // Ollama models live in ~/.ollama/models and
                        // HuggingFace cache in ~/.cache/... so hiding
                        // dotted entries would block the primary use
                        // case for this picker.
                        if ui
                            .add(
                                egui::Button::image_and_text(
                                    Icon::Folder.image(ICON_PT, theme::ink_dim()),
                                    text::note(&name).color(theme::ink()),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            new_path = Some(entry.clone());
                        }
                    }
                });

            ui.add_space(widgets::GAP_LABEL);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(text::note("Cancel"))).clicked() {
                    close = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let commit_btn = egui::Button::new(text::value("Use this folder").color(theme::on_accent()))
                        .fill(theme::accent());
                    if ui.add(commit_btn).clicked() {
                        commit = true;
                    }
                });
            });
        });

    // Escape closes without committing — standard modal shortcut.
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }

    if commit {
        let p = current.to_string_lossy().to_string();
        match target {
            crate::config_editor::BrowseTarget::OllamaDir       => editor.ollama_models_dir = p,
            crate::config_editor::BrowseTarget::HuggingFaceDir  => editor.huggingface_models_dir = p,
        }
        editor.inapp_picker = None;
    } else if close {
        editor.inapp_picker = None;
    } else if let Some(p) = new_path {
        editor.inapp_picker = Some((target, p));
    }
}

