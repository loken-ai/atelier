//! Settings section — all configuration inline, no modals
//!
//! Reads/writes: AppConfig, SettingsState, ConfigEditorState

use eframe::egui::{self, Color32, RichText, CornerRadius, Stroke, TextEdit};
#[allow(unused_imports)]
use crate::config::{AppConfig, ApiProfile, ApiType};
use crate::config_editor::ConfigEditorState;
use crate::icons::Icon;
use crate::settings::{SettingsAction, SettingsState};
use crate::theme;

/// Render the settings section
pub fn render(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    settings_state: &mut SettingsState,
    connection_state: crate::state::ConnectionState,
) -> Vec<SettingsAction> {
    let mut actions = Vec::new();
    let dark = config.dark_theme;
    let text_primary = if dark { theme::text() } else { theme::light::TEXT };
    let text_secondary = if dark { theme::text_secondary() } else { theme::light::TEXT_SECONDARY };
    let text_muted = if dark { theme::text_muted() } else { theme::light::TEXT_MUTED };
    let surface = if dark { theme::surface() } else { theme::light::SURFACE };
    let surface_elevated = if dark { theme::surface_elevated() } else { theme::light::SURFACE_ELEVATED };
    let border = if dark { theme::border() } else { theme::light::BORDER };

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
                ui.set_min_width(380.0);
                ui.add_space(4.0);
                ui.label(RichText::new("Reset all GUI settings to defaults?").size(13.0).strong());
                ui.add_space(8.0);
                ui.label(
                    RichText::new("This wipes:\n  • All API profiles\n  • Server URL\n  • Selected model\n  • Theme / window preferences")
                        .size(11.0)
                        .color(text_secondary),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Local model files on disk are not affected.")
                        .size(11.0)
                        .italics()
                        .color(text_muted),
                );
                ui.add_space(4.0);
                // Tell users a backup is taken so the reset feels
                // less terminal — they can restore by copying the
                // .before_reset.<ts> file back to config.json.
                ui.label(
                    RichText::new("A backup of the current config.json is saved as \
                                   config.json.before_reset.<timestamp> in the same \
                                   folder, so you can restore it manually if needed.")
                        .size(11.0)
                        .italics()
                        .color(text_muted),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Cancel").size(12.0)).clicked() {
                        close_modal = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let reset_btn = egui::Button::new(
                            RichText::new("Reset").size(12.0).color(Color32::WHITE),
                        )
                        .fill(theme::error())
                        .corner_radius(CornerRadius::same(4));
                        if ui.add(reset_btn).clicked() {
                            confirmed = true;
                        }
                    });
                });
                ui.add_space(4.0);
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

    // Header
    ui.horizontal(|ui| {
        Icon::Gear.show(ui, 20.0, text_primary);
        ui.add_space(6.0);
        ui.label(RichText::new("Settings").size(18.0).strong().color(text_primary));
    });
    ui.add_space(12.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let card_width = ui.available_width().min(640.0);

        // ── Server Connection ──
        settings_card(ui, Icon::Globe, "Connection", dark, surface, border, card_width, |ui| {
            form_row(ui, "Server URL", text_secondary, |ui| {
                // Apply button width budget so the text field doesn't
                // hug the right edge of the card.
                let resp = ui.add(
                    TextEdit::singleline(&mut settings_state.url_input)
                        .desired_width(ui.available_width() - 70.0),
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
                let apply_btn = egui::Button::new(
                    RichText::new("Apply")
                        .size(11.0)
                        .color(if can_apply { Color32::WHITE } else { text_secondary }),
                )
                .fill(if can_apply { theme::accent() } else { surface_elevated })
                .corner_radius(CornerRadius::same(4));
                let apply_resp = ui.add_enabled(can_apply, apply_btn);
                let apply_clicked = apply_resp.clicked();
                let apply_tip = if dirty && !non_empty {
                    "URL is empty — type a server URL to enable Apply.".to_string()
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
            hint_text(ui, "The web address where your AI model is running. Press Enter or click Apply to connect. Use the Quick Connect buttons below for common setups.");

            ui.add_space(4.0);

            // API key. Only needed when the server has `require_auth` on; without this
            // field, turning authentication on server-side would lock this app out with
            // a bare 401 and no way to fix it from the UI.
            form_row(ui, "API key", text_secondary, |ui| {
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
            hint_text(ui, "Sent as a bearer token on every request. Leave empty unless the server is configured with require_auth.");

            ui.add_space(4.0);

            form_row(ui, "Status", text_secondary, |ui| {
                // Colour + label both derive from the ConnectionState
                // enum — no more `.contains()` sniffing that could
                // desync the dot from the text.
                let dot_color = connection_state.color();
                let status_text = connection_state.label();
                let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(dot_rect.center(), 4.0, dot_color);
                ui.add_space(4.0);
                ui.label(RichText::new(status_text).size(12.0).color(text_primary));
            });

            ui.add_space(6.0);

            ui.label(RichText::new("Quick Connect").size(11.0).color(text_muted));
            ui.horizontal(|ui| {
                for (label, url) in [
                    ("LOKEN", crate::config::DEFAULT_LOKEN_URL),
                    ("Ollama",     crate::config::DEFAULT_OLLAMA_URL),
                ] {
                    // Highlight based on the SAVED server URL, not
                    // the in-progress input. Typing in the field
                    // shouldn't desync the active-preset indicator —
                    // it just means the user is composing a new URL,
                    // not that they've abandoned the current one.
                    let is_active = config.server_url == url;
                    let btn = egui::Button::new(
                        RichText::new(label).size(11.0).color(if is_active { Color32::WHITE } else { text_secondary }),
                    )
                    .fill(if is_active { theme::accent() } else { surface_elevated })
                    .corner_radius(CornerRadius::same(4));
                    let btn_resp = ui.add(btn);
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

        ui.add_space(12.0);

        // ── Appearance ──
        settings_card(ui, Icon::Palette, "Appearance", dark, surface, border, card_width, |ui| {
            form_row(ui, "Theme", text_secondary, |ui| {
                for (label, value) in [("Dark", true), ("Light", false)] {
                    let is_active = config.dark_theme == value;
                    let btn = egui::Button::new(
                        RichText::new(label).size(12.0).color(if is_active { Color32::WHITE } else { text_secondary }),
                    )
                    .fill(if is_active { theme::accent() } else { surface_elevated })
                    .corner_radius(CornerRadius::same(4));
                    if ui.add(btn).clicked() && config.dark_theme != value {
                        config.dark_theme = value;
                        theme::apply(ctx, value);
                        config.save();
                    }
                    ui.add_space(4.0);
                }
            });
        });

        ui.add_space(12.0);

        // ── API Profiles ──
        settings_card(ui, Icon::Key, "API Profiles", dark, surface, border, card_width, |ui| {
            form_row(ui, "Active Profile", text_secondary, |ui| {
                egui::ComboBox::from_id_salt("profile_selector_settings")
                    .selected_text(config.selected_profile.as_deref().unwrap_or("(none)"))
                    .width(ui.available_width().min(250.0))
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
                            .max_height(280.0)
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
                ui.add_space(8.0);
                egui::Frame {
                    inner_margin: egui::Margin::same(10),
                    corner_radius: CornerRadius::same(4),
                    fill: surface_elevated,
                    stroke: Stroke::new(0.5, border),
                    ..Default::default()
                }
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let badge_color = match profile.api_type {
                            ApiType::Loken => theme::accent(),
                            ApiType::Ollama => theme::success(),
                            ApiType::OpenApi => theme::warning(),
                        };
                        pill_badge(ui, &format!("{}", profile.api_type), badge_color);
                        ui.add_space(8.0);
                        ui.label(RichText::new(&profile.server_url).size(11.0).color(text_muted));
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
                            let del_btn = egui::Button::new(
                                RichText::new("Delete").size(10.0).color(theme::error()),
                            )
                            .fill(theme::tinted(theme::error(), 18))
                            .corner_radius(CornerRadius::same(3));
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
                            ui.add_space(4.0);
                            let edit_btn = egui::Button::new(
                                RichText::new("Edit").size(10.0).color(theme::accent()),
                            )
                            .fill(theme::tinted(theme::accent(), 18))
                            .corner_radius(CornerRadius::same(3));
                            if ui.add(edit_btn).on_hover_text("Open the profile editor").clicked() {
                                edit_clicked = true;
                            }
                        });
                    });
                    ui.add_space(4.0);
                    match profile.api_type {
                        ApiType::Ollama | ApiType::Loken => {
                            param_grid(ui, text_secondary, text_primary, &[
                                ("Temperature", &format!("{:.2}", profile.ollama_params.temperature)),
                                ("Top P", &format!("{:.2}", profile.ollama_params.top_p)),
                                ("Top K", &format!("{}", profile.ollama_params.top_k)),
                                ("Context", &format!("{}", profile.ollama_params.num_ctx)),
                            ]);
                        }
                        ApiType::OpenApi => {
                            param_grid(ui, text_secondary, text_primary, &[
                                ("Temperature", &format!("{:.2}", profile.openapi_params.temperature)),
                                ("Max Tokens", &format!("{:?}", profile.openapi_params.max_tokens)),
                                ("Top P", &format!("{:.2}", profile.openapi_params.top_p)),
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

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let btn = egui::Button::new(RichText::new("+ New Profile").size(11.0).color(theme::accent()))
                    .fill(theme::tinted(theme::accent(), 15))
                    .corner_radius(CornerRadius::same(4));
                if ui.add(btn).clicked() {
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

        ui.add_space(12.0);

        // ── Server Configuration (config.toml) — inline ──
        if let Some(editor) = settings_state.config_editor.as_mut() {
            // Error banner
            if let Some(error) = &editor.error_message {
                egui::Frame {
                    inner_margin: egui::Margin::symmetric(12, 8),
                    corner_radius: CornerRadius::same(4),
                    fill: theme::tinted(theme::error(), 20),
                    stroke: Stroke::new(1.0, theme::error()),
                    ..Default::default()
                }
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("!").size(14.0).strong().color(theme::error()));
                        ui.label(RichText::new(error).size(12.0).color(theme::error()));
                    });
                });
                ui.add_space(8.0);
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
            settings_card(ui, Icon::Folder, "Model Directories", dark, surface, border, card_width, |ui| {
                ui.label(RichText::new("Ollama Models").size(11.0).color(text_secondary));
                ui.horizontal(|ui| {
                    TextEdit::singleline(&mut editor.ollama_models_dir)
                        .desired_width(ui.available_width() - 70.0)
                        .hint_text("~/.ollama/models")
                        .show(ui);
                    if ui
                        .add(egui::Button::new("Browse"))
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
                hint_text(ui, "Folder where Ollama keeps its downloaded models. Leave empty to use the standard location.");

                ui.add_space(6.0);

                ui.label(RichText::new("HuggingFace Models").size(11.0).color(text_secondary));
                ui.horizontal(|ui| {
                    TextEdit::singleline(&mut editor.huggingface_models_dir)
                        .desired_width(ui.available_width() - 70.0)
                        .hint_text("~/.cache/huggingface/hub")
                        .show(ui);
                    if ui
                        .add(egui::Button::new("Browse"))
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
                hint_text(ui, "Folder where HuggingFace models are cached after download. Leave empty to use the standard location.");
            });

            ui.add_space(12.0);

            // Server bind
            settings_card(ui, Icon::Server, "Server Bind", dark, surface, border, card_width, |ui| {
                config_row_pair(ui, text_secondary,
                    ("Host", &mut editor.server_host),
                    ("Port", &mut editor.server_port_str),
                );
                hint_row(ui, text_muted,
                    "Use 127.0.0.1 for this computer only, or 0.0.0.0 to allow connections from other devices on your network",
                    "Port number the server listens on. Default 11435. Change if another app already uses this port.",
                );
            });

            ui.add_space(12.0);

            // Inference
            settings_card(ui, Icon::Gear, "Inference", dark, surface, border, card_width, |ui| {
                ui.label(RichText::new("Model ID").size(11.0).color(text_secondary));
                TextEdit::singleline(&mut editor.model_id)
                    .desired_width(ui.available_width())
                    .show(ui);
                hint_text(ui, "Name of the AI model to load when the server starts. Must match a model you have downloaded (e.g. devstral-small-2).");

                ui.add_space(8.0);

                config_row_pair(ui, text_secondary,
                    ("Max Tokens", &mut editor.max_tokens_str),
                    ("Context Length", &mut editor.context_length_str),
                );
                hint_row(ui, text_muted,
                    "Limits how long each response can be. Higher = longer answers but slower.",
                    "How much conversation history the model can remember. Larger values use more memory.",
                );
                ui.add_space(4.0);
                config_row_pair(ui, text_secondary,
                    ("Temperature", &mut editor.temperature_str),
                    ("Top P", &mut editor.top_p_str),
                );
                hint_row(ui, text_muted,
                    "Controls creativity. Low (0.1) = focused and predictable. High (1.5) = creative and varied.",
                    "Filters unlikely words. Lower values (e.g. 0.5) make output more focused. Usually fine at 0.9.",
                );
                ui.add_space(4.0);
                config_row_pair(ui, text_secondary,
                    ("Top K", &mut editor.top_k_str),
                    ("Seed", &mut editor.seed_str),
                );
                hint_row(ui, text_muted,
                    "How many word choices the model considers at each step. Lower = more predictable.",
                    "A number that makes outputs repeatable. Same seed + same prompt = same answer.",
                );
            });

            ui.add_space(12.0);

            // GPU & Device
            settings_card(ui, Icon::Bolt, "GPU & Device", dark, surface, border, card_width, |ui| {
                config_row_pair(ui, text_secondary,
                    ("Device Index", &mut editor.device_index_str),
                    ("GPU Memory %", &mut editor.max_gpu_memory_fraction_str),
                );
                hint_row(ui, text_muted,
                    "Which GPU to use if you have multiple. Leave empty to pick automatically.",
                    "How much of your GPU's memory to use (0.9 = 90%). Lower this if you get out-of-memory errors.",
                );
                ui.add_space(4.0);
                config_row_pair(ui, text_secondary,
                    ("Force GPU Layers", &mut editor.force_gpu_layers_str),
                    ("CPU Threads", &mut editor.cpu_threads_str),
                );
                hint_row(ui, text_muted,
                    "Force a specific number of model layers onto the GPU. Leave empty to let the server decide based on available memory.",
                    "Number of CPU cores to use for processing. Set to 0 to use all available cores automatically.",
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut editor.use_quantized_gpu, "");
                    ui.label(RichText::new("Use Quantized GPU").size(12.0).color(text_primary));
                });
                hint_text(ui, "Compresses the model to use less GPU memory at a small quality cost. Recommended if your GPU has limited memory (8 GB or less).");
            });

            ui.add_space(12.0);

            // Save / Reload / Reset row
            settings_card(ui, Icon::Save, "Actions", dark, surface, border, card_width, |ui| {
                ui.horizontal(|ui| {
                    // Save config.toml
                    let save_btn = egui::Button::new(
                        RichText::new("Save config.toml").size(12.0).color(Color32::WHITE),
                    )
                    .fill(theme::accent())
                    .corner_radius(CornerRadius::same(4));
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

                    ui.add_space(8.0);

                    // Reload from disk
                    let reload_btn = egui::Button::new(
                        RichText::new("Reload from disk").size(12.0).color(text_secondary),
                    )
                    .fill(surface_elevated)
                    .stroke(Stroke::new(0.5, border))
                    .corner_radius(CornerRadius::same(4));
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

                    ui.add_space(8.0);

                    // Reset GUI defaults — two-step confirm via modal,
                    // since this wipes profiles + server URL + selected
                    // model in one click. The modal lives at the top
                    // of this render fn; this button only sets the
                    // pending flag. While the modal is already open
                    // disable the button so re-clicking it doesn't
                    // re-trigger or visually hint that a second click
                    // is needed.
                    let reset_btn = egui::Button::new(
                        RichText::new("Reset GUI defaults").size(12.0).color(theme::error()),
                    )
                    .fill(theme::tinted(theme::error(), 12))
                    .corner_radius(CornerRadius::same(4));
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

/// A settings card with title and body
// Wide signature carries UI handle, icon + title pair, theme colours
// (dark/surface/border), width budget, and the body closure. Bundling
// into a struct adds boilerplate without simplifying the call sites,
// which already pass everything inline.
#[allow(clippy::too_many_arguments)]
fn settings_card(
    ui: &mut egui::Ui,
    icon: Icon,
    title: &str,
    dark: bool,
    surface: Color32,
    border: Color32,
    max_width: f32,
    add_body: impl FnOnce(&mut egui::Ui),
) {
    let title_color = if dark { theme::text() } else { theme::light::TEXT };

    ui.allocate_ui(egui::vec2(max_width, 0.0), |ui| {
        egui::Frame {
            inner_margin: egui::Margin::same(16),
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
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icon.show(ui, 16.0, title_color);
                ui.add_space(6.0);
                ui.label(RichText::new(title).size(14.0).strong().color(title_color));
            });
            ui.add_space(10.0);
            add_body(ui);
        });
    });
}

/// A form row with label on the left and content on the right
fn form_row(
    ui: &mut egui::Ui,
    label: &str,
    label_color: Color32,
    add_content: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.allocate_ui(egui::vec2(110.0, ui.spacing().interact_size.y), |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(label).size(12.0).color(label_color));
            });
        });
        add_content(ui);
    });
}

/// Compact parameter grid (2 columns of key=value)
fn param_grid(ui: &mut egui::Ui, label_color: Color32, value_color: Color32, params: &[(&str, &str)]) {
    ui.columns(2, |cols| {
        for (i, (key, val)) in params.iter().enumerate() {
            let col = &mut cols[i % 2];
            col.horizontal(|ui| {
                ui.label(RichText::new(*key).size(11.0).color(label_color));
                ui.label(RichText::new(*val).size(11.0).strong().color(value_color));
            });
        }
    });
}

/// Two text fields side by side
fn config_row_pair(
    ui: &mut egui::Ui,
    label_color: Color32,
    (label_a, val_a): (&str, &mut String),
    (label_b, val_b): (&str, &mut String),
) {
    ui.columns(2, |cols| {
        cols[0].label(RichText::new(label_a).size(11.0).color(label_color));
        TextEdit::singleline(val_a).desired_width(f32::INFINITY).show(&mut cols[0]);

        cols[1].label(RichText::new(label_b).size(11.0).color(label_color));
        TextEdit::singleline(val_b).desired_width(f32::INFINITY).show(&mut cols[1]);
    });
}

/// Small muted help text below a field
fn hint_text(ui: &mut egui::Ui, text: &str) {
    let dark = ui.visuals().dark_mode;
    let muted = if dark { theme::text_muted() } else { theme::light::TEXT_MUTED };
    ui.label(RichText::new(text).size(10.0).color(muted));
}

/// Two-column hint row (aligned with config_row_pair above it)
fn hint_row(ui: &mut egui::Ui, muted: Color32, hint_a: &str, hint_b: &str) {
    ui.columns(2, |cols| {
        cols[0].label(RichText::new(hint_a).size(10.0).color(muted));
        cols[1].label(RichText::new(hint_b).size(10.0).color(muted));
    });
}

/// Colored pill badge
fn pill_badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame {
        inner_margin: egui::Margin::symmetric(8, 2),
        corner_radius: CornerRadius::same(10),
        fill: theme::tinted(color, 30),
        ..Default::default()
    }
    .show(ui, |ui| {
        ui.label(RichText::new(text).size(11.0).color(color));
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
        .default_width(520.0)
        .default_height(420.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // Editable path bar — lets the user paste a full path and
            // press Enter to jump there instead of clicking subfolders
            // one at a time. The Go button is disabled when the typed
            // path isn't a directory (visible feedback that the path
            // is wrong instead of silent "Enter does nothing").
            ui.horizontal(|ui| {
                ui.label(RichText::new("Path:").size(11.0));
                let mut path_str = current.display().to_string();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut path_str)
                        .desired_width(ui.available_width() - 50.0)
                        .font(egui::TextStyle::Monospace),
                );
                let typed_path = std::path::PathBuf::from(path_str.trim());
                let typed_valid = typed_path.is_dir();
                let go_btn_resp = ui.add_enabled(
                    typed_valid,
                    egui::Button::new(RichText::new("Go").size(11.0)),
                );
                if !typed_valid && path_str.trim() != current.display().to_string() {
                    go_btn_resp.clone().on_hover_text("Path doesn't exist or isn't a directory");
                }
                let go = go_btn_resp.clicked()
                    || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && typed_valid);
                if go {
                    new_path = Some(typed_path);
                }
            });
            ui.add_space(4.0);

            // Quick-jump row: $HOME, /
            ui.horizontal(|ui| {
                if let Some(home) = std::env::var_os("HOME") {
                    let home_resp = ui.add(egui::Button::image_and_text(
                        Icon::Home.image(13.0, theme::accent()),
                        RichText::new("Home").size(11.0),
                    ).small());
                    if home_resp.on_hover_text("Jump to $HOME").clicked() {
                        new_path = Some(std::path::PathBuf::from(home));
                    }
                }
                if ui.small_button("/  Root").on_hover_text("Jump to /").clicked() {
                    new_path = Some(std::path::PathBuf::from("/"));
                }
            });
            ui.add_space(4.0);

            // Scrollable directory list. Reserve ~80px for the action
            // row below so the buttons stay visible at any window size.
            let list_h = (ui.available_height() - 80.0).max(120.0);
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
                            .add(egui::Button::new(RichText::new("..").size(12.0).monospace()).frame(false))
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
                                    Icon::Folder.image(13.0, theme::accent()),
                                    RichText::new(&name).size(12.0),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            new_path = Some(entry.clone());
                        }
                    }
                });

            ui.add_space(6.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Cancel").size(12.0)).clicked() {
                    close = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let commit_btn = egui::Button::new(
                        RichText::new("Use this folder").size(12.0).color(Color32::WHITE),
                    )
                    .fill(theme::accent())
                    .corner_radius(CornerRadius::same(4));
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

