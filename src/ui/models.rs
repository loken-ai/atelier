//! Models section — two-column layout for local models and import
//!
//! Reads/writes: ModelState, AppConfig

use eframe::egui::{self, Color32, RichText, CornerRadius, Stroke};
use crate::api::ModelInfo;
use crate::config::AppConfig;
use crate::icons::Icon;
use crate::state::{ActionStatus, ModelState, ModelSortDirection, ModelSortField};
use crate::settings::SettingsAction;
use crate::theme;
use crate::ui::components::panel;
use crate::modality::ModelModality;

/// Popular models for suggestions
const POPULAR_MODELS: &[(&str, &str)] = &[
    ("tinyllama:latest", "TinyLlama 1.1B"),
    ("llama3.2:latest", "Llama 3.2"),
    ("mistral:latest", "Mistral 7B"),
    ("phi3:latest", "Phi-3"),
    ("devstral-small:latest", "Devstral Small"),
];

/// Render the models section, returning any actions to perform
pub fn render(
    ui: &mut egui::Ui,
    models: &mut ModelState,
    config: &AppConfig,
) -> Vec<SettingsAction> {
    let mut actions = Vec::new();
    let dark = config.dark_theme;
    let text_primary = if dark { theme::text() } else { theme::light::TEXT };
    let text_secondary = if dark { theme::text_secondary() } else { theme::light::TEXT_SECONDARY };
    let surface = if dark { theme::surface() } else { theme::light::SURFACE };
    let border = if dark { theme::border() } else { theme::light::BORDER };

    // Delete confirmation modal. Two-step delete (set the pending name
    // in ModelState, render a centered Window prompting the user to
    // confirm) prevents an accidental click on the Delete button from
    // silently dropping a multi-gigabyte model file. Cancel clears the
    // pending name without action; Confirm dispatches DeleteModel and
    // clears. The pending name is borrowed (not cloned per frame) and
    // moved out via take() only when the user confirms.
    let pending_action = if let Some(name) = models.delete_confirm_pending.as_ref() {
        let mut close_modal = false;
        let mut confirmed = false;
        egui::Window::new("Confirm delete")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.set_min_width(360.0);
                ui.add_space(4.0);
                ui.label(RichText::new("Delete this model from disk?").size(13.0).strong());
                ui.add_space(6.0);
                egui::Frame {
                    inner_margin: egui::Margin::symmetric(10, 6),
                    corner_radius: CornerRadius::same(4),
                    fill: surface,
                    stroke: Stroke::new(1.0, border),
                    ..Default::default()
                }
                .show(ui, |ui| {
                    ui.label(RichText::new(name).size(12.0).monospace().color(text_primary));
                });
                ui.add_space(8.0);
                ui.label(
                    RichText::new("This permanently removes the model files. \
                                   You can re-download from the source registry later.")
                        .size(11.0)
                        .color(text_secondary),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Cancel").size(12.0)).clicked() {
                        close_modal = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let delete_btn = egui::Button::image_and_text(
                            Icon::Trash.image(13.0, Color32::WHITE),
                            RichText::new("Delete").size(12.0).color(Color32::WHITE),
                        )
                        .fill(theme::error())
                        .corner_radius(CornerRadius::same(4));
                        if ui.add(delete_btn).clicked() {
                            confirmed = true;
                        }
                    });
                });
                ui.add_space(4.0);
            });
        // Escape closes the modal without acting — standard dialog
        // shortcut, prevents the user being stuck if they fat-fingered
        // Delete and the mouse isn't near the Cancel button.
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            close_modal = true;
        }
        if confirmed {
            Some(true)
        } else if close_modal {
            Some(false)
        } else {
            None
        }
    } else {
        None
    };
    if let Some(do_delete) = pending_action {
        if let Some(name) = models.delete_confirm_pending.take() {
            if do_delete {
                actions.push(SettingsAction::DeleteModel(name));
            }
        }
    }

    // Status bar
    match &models.action_status {
        ActionStatus::InProgress(msg) => {
            // A pull is a long, unbounded download. The server's
            // /api/pull is a single blocking request that reports no
            // intermediate byte/total progress (the model_manager
            // progress callback is passed None and the response is one
            // JSON at completion), so a determinate %/ETA bar isn't
            // possible without a server-side streaming change. Show an
            // indeterminate *moving* ProgressBar for pulls — clearer
            // "work in progress, unknown duration" than a bare spinner.
            // Other in-flight actions (refresh, load) are short, so keep
            // the spinner for those.
            let is_pull = msg.starts_with("Pulling");
            egui::Frame {
                inner_margin: egui::Margin::symmetric(12, 8),
                corner_radius: CornerRadius::same(4),
                // Tinted from theme::PRIMARY so the fill matches the
                // stroke colour — the previous hardcoded (37, 99, 235)
                // was Tailwind blue-600, which diverged from the
                // theme's (79, 140, 201) PRIMARY when the palette was
                // last tuned, leaving a visible blue-on-blue mismatch.
                fill: theme::tinted(theme::PRIMARY, 20),
                stroke: Stroke::new(1.0, theme::PRIMARY),
                ..Default::default()
            }
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.add_space(8.0);
                    ui.label(RichText::new(msg).size(12.0).color(theme::PRIMARY));
                });
                if is_pull {
                    ui.add_space(6.0);
                    // Determinate once the server streams byte progress
                    // (total > 0). During the ollama manifest phase (or
                    // before the first byte line) total == 0 → fall back
                    // to the indeterminate sweep. app.rs requests a 100ms
                    // repaint while any action is in progress, so both the
                    // animation and the streamed % updates tick without
                    // user input.
                    match models.pull_progress {
                        Some((completed, total)) if total > 0 => {
                            let frac = (completed as f32 / total as f32).clamp(0.0, 1.0);
                            let label = format!(
                                "{:.0}% · {} / {}",
                                frac * 100.0,
                                crate::api::types::format_size(completed),
                                crate::api::types::format_size(total),
                            );
                            ui.add(
                                egui::ProgressBar::new(frac)
                                    .desired_width(ui.available_width())
                                    .text(label),
                            );
                        }
                        _ => {
                            // Indeterminate sweep driven by the frame clock —
                            // a repeating 0→1 fill so the bar visibly moves.
                            let t = ui.input(|i| i.time);
                            let frac = (t * 0.55).rem_euclid(1.0) as f32;
                            ui.add(
                                egui::ProgressBar::new(frac)
                                    .desired_width(ui.available_width())
                                    .animate(true),
                            );
                        }
                    }
                }
            });
            ui.add_space(8.0);
        }
        ActionStatus::Success(msg) => {
            ui.horizontal(|ui| {
                status_chip(ui, Icon::Check, "OK", theme::success());
                ui.add_space(6.0);
                ui.label(RichText::new(msg).size(11.0).color(text_secondary));
            });
            ui.add_space(8.0);
        }
        ActionStatus::Failed(msg) => {
            ui.horizontal(|ui| {
                status_chip(ui, Icon::Cross, "Error", theme::error());
                ui.add_space(6.0);
                ui.label(RichText::new(msg).size(11.0).color(text_secondary));
            });
            ui.add_space(8.0);
        }
        ActionStatus::Idle => {}
    }

    // Single-column layout. Previously a two-column horizontal split
    // jammed the model list into the left half and the Import card
    // into the right; both wrapped poorly on narrow windows and the
    // section_header bar inside a horizontal row produced awkward
    // alignment. Cleaner architecture:
    //
    //   1. Section header (full width)
    //   2. Toolbar row: filter | sort | refresh | count
    //   3. Model list (scrollable, fills available height minus footer)
    //   4. Import footer card (always visible at bottom)

    // Pre-compute filter view so the count + the list match.
    // The owned String here is intentional — the TextEdit below
    // mutably borrows `models.list_filter`, so a borrow held in
    // `filter` would conflict. The cost is one short alloc per frame
    // for the trimmed filter; the big win is dropping the per-model
    // `.to_ascii_lowercase()` clone inside the retain (was N strings
    // every frame), and the retain itself is skipped entirely when
    // the filter is empty — the common steady-state case.
    let filter = models.list_filter.trim().to_string();
    // Pair each visible model with its modality up-front so the
    // modality-filter retain AND the per-card render loop share one
    // ModelModality::from_model_name call per model per frame (down
    // from two in the modality-filter-active case, and one in the
    // common case where the badge still wanted it). Cost: O(N) up-
    // front; benefit: zero recompute later. The map() runs after the
    // name-filter retain so we don't classify models we'll discard.
    let mut visible_pairs: Vec<(ModelInfo, ModelModality)> = models
        .get_sorted_models()
        .into_iter()
        .filter(|m| {
            filter.is_empty()
                || crate::log_buffer::contains_ascii_ci(&m.name, filter.as_bytes())
        })
        .map(|m| {
            let modality = ModelModality::from_model_name(&m.name);
            (m, modality)
        })
        .collect();
    // Modality filter — chips toolbar lets the user narrow a long
    // catalog to a single kind (TTS / ASR / image-gen / vision /
    // text). None ⇒ show all, no extra retain pass.
    if let Some(want) = models.list_modality_filter {
        visible_pairs.retain(|(_, modality)| *modality == want);
    }
    let total = models.available_models.len();
    let visible = visible_pairs.len();

    // ── 1. Section header ─────────────────────────────────────────
    panel::section_header(ui, "LOCAL MODELS", theme::ACCENT_MODELS);
    ui.add_space(8.0);

    // ── 2. Toolbar: filter, sort, refresh, count ─────────────────
    egui::Frame {
        inner_margin: egui::Margin::symmetric(10, 8),
        corner_radius: CornerRadius::same(6),
        fill: surface,
        stroke: Stroke::new(1.0, border),
        ..Default::default()
    }
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            // Filter input
            Icon::Search.show(ui, 13.0, text_secondary);
            let filter_width = (ui.available_width() * 0.4).clamp(140.0, 320.0);
            ui.add(
                egui::TextEdit::singleline(&mut models.list_filter)
                    .hint_text("Filter…")
                    .desired_width(filter_width),
            );
            // Clear button — drops BOTH the name filter and the
            // modality chip so a single click resets the view.
            let any_filter_active = !models.list_filter.is_empty()
                || models.list_modality_filter.is_some();
            if any_filter_active
                && ui
                    .add(egui::Button::image(
                        Icon::Cross.image(10.0, text_secondary),
                    ).small().frame(false))
                    .on_hover_text("Clear name + modality filters")
                    .clicked()
            {
                models.list_filter.clear();
                models.list_modality_filter = None;
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // Modality filter chips — narrow a long catalog to one kind.
            // None = "All", clicking the active chip again returns to All.
            // Order follows decreasing-use-frequency in the perimeter
            // (text-LLM is the most-common workload).
            ui.label(RichText::new("Kind").size(11.0).color(text_secondary));
            let chips: &[(Option<ModelModality>, &str)] = &[
                (None,                          "All"),
                (Some(ModelModality::Text),     "Text"),
                (Some(ModelModality::Vision),   "Vision"),
                (Some(ModelModality::ImageGen), "Image"),
                (Some(ModelModality::AudioTts), "TTS"),
                (Some(ModelModality::AudioAsr), "ASR"),
                (Some(ModelModality::VideoGen), "Video"),
            ];
            for (kind, label) in chips {
                let selected = models.list_modality_filter == *kind;
                let chip_btn = egui::Button::new(
                    RichText::new(*label)
                        .size(11.0)
                        .color(if selected { theme::PRIMARY } else { text_secondary }),
                )
                .fill(if selected {
                    theme::tinted(theme::PRIMARY, 25)
                } else {
                    Color32::TRANSPARENT
                })
                .corner_radius(CornerRadius::same(3));
                if ui.add(chip_btn).clicked() {
                    // Toggle: clicking the active chip returns to All
                    // (saves a second click vs forcing the user to
                    // hit the "All" chip explicitly).
                    models.list_modality_filter = if selected { None } else { *kind };
                }
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // Sort controls with direction arrow on the active field
            ui.label(RichText::new("Sort").size(11.0).color(text_secondary));
            for (field, base_label) in [
                (ModelSortField::Name, "Name"),
                (ModelSortField::Size, "Size"),
                (ModelSortField::Date, "Date"),
            ] {
                let selected = models.sort_field == field;
                // The direction is a word, not an arrow: neither arrow
                // character is in the bundled fonts and both drew as boxes,
                // and a sort control whose direction is a box says nothing.
                let label = if selected {
                    let direction = match models.sort_direction {
                        ModelSortDirection::Asc  => "ascending",
                        ModelSortDirection::Desc => "descending",
                    };
                    format!("{} ({})", base_label, direction)
                } else {
                    base_label.to_string()
                };
                let btn = egui::Button::new(
                    RichText::new(label)
                        .size(11.0)
                        .color(if selected { theme::PRIMARY } else { text_secondary }),
                )
                .fill(if selected {
                    theme::tinted(theme::PRIMARY, 25)
                } else {
                    Color32::TRANSPARENT
                })
                .corner_radius(CornerRadius::same(3));
                if ui.add(btn).clicked() {
                    if models.sort_field == field {
                        models.sort_direction = match models.sort_direction {
                            ModelSortDirection::Asc => ModelSortDirection::Desc,
                            ModelSortDirection::Desc => ModelSortDirection::Asc,
                        };
                    } else {
                        models.sort_field = field;
                        models.sort_direction = ModelSortDirection::Asc;
                    }
                }
            }

            // Right-anchored count + refresh
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::image_and_text(
                        Icon::Refresh.image(12.0, text_secondary),
                        RichText::new("Refresh").size(11.0),
                    ).small())
                    .on_hover_text("Reload model list from server")
                    .clicked()
                {
                    actions.push(SettingsAction::RefreshModels);
                }
                ui.add_space(8.0);
                let count_text = if filter.is_empty() || visible == total {
                    format!("{} model{}", total, if total == 1 { "" } else { "s" })
                } else {
                    format!("{} of {}", visible, total)
                };
                ui.label(RichText::new(count_text).size(11.0).color(text_secondary));
            });
        });
    });

    ui.add_space(8.0);

    // ── 3. Model list ─────────────────────────────────────────────
    // Reserve ~180px for the Import footer so the list never crushes
    // it off-screen on short windows.
    let footer_reserve = 200.0;
    let list_height = (ui.available_height() - footer_reserve).max(120.0);
    egui::ScrollArea::vertical()
        .id_salt("models_list_scroll")
        .max_height(list_height)
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if visible_pairs.is_empty() {
                let msg = if filter.is_empty() {
                    "No models found. Use the Import card below to download one."
                } else {
                    "No models match the filter."
                };
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(msg).color(text_secondary).italics().size(12.0));
                });
                return;
            }
            for (model, modality) in &visible_pairs {
                let modality = *modality;
                let is_loaded = models.is_loaded(&model.name);
                let is_selected = models.selected_model.as_deref() == Some(&model.name);
                let mut name_clicked = false;

                let card_fill = if is_selected {
                    theme::tinted(theme::PRIMARY, 18)
                } else {
                    surface
                };
                let card = egui::Frame {
                    inner_margin: egui::Margin::symmetric(12, 8),
                    corner_radius: CornerRadius::same(6),
                    fill: card_fill,
                    stroke: Stroke::new(
                        if is_selected { 1.5 } else { 1.0 },
                        if is_selected { theme::PRIMARY } else { border },
                    ),
                    ..Default::default()
                }
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Loaded indicator dot
                        let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            dot_rect.center(),
                            4.0,
                            if is_loaded { theme::success() } else { Color32::from_gray(60) },
                        );
                        if is_loaded {
                            ui.add_space(2.0);
                        }

                        // Model name — plain label now; the whole card
                        // is the select target (wired below the frame).
                        ui.label(
                            RichText::new(&model.name).size(14.0).strong().color(text_primary),
                        );

                        // Modality badge — color via the shared
                        // modality::modality_badge_color helper so the
                        // badge color stays in lockstep with the chat-
                        // tab modality header (otherwise a tweak in
                        // one file would drift the two surfaces apart).
                        // `modality` comes from the precomputed
                        // visible_pairs vec built once at the top of
                        // this render — no per-card from_model_name
                        // call needed here.
                        if modality != ModelModality::Text {
                            let badge_color = crate::modality::modality_badge_color(modality);
                            let badge_resp = egui::Frame {
                                inner_margin: egui::Margin::symmetric(6, 1),
                                corner_radius: CornerRadius::same(3),
                                fill: theme::tinted(badge_color, 35),
                                ..Default::default()
                            }
                            .show(ui, |ui| {
                                ui.label(RichText::new(modality.label()).size(10.0).color(badge_color));
                            });
                            badge_resp.response.on_hover_text(modality.tooltip());
                        }

                        // Source badge — only render when known (some
                        // mock / test payloads may have empty source).
                        // Tells users at a glance whether each model
                        // came from the Ollama registry or HuggingFace,
                        // which matters when the same display name
                        // exists in both registries with different
                        // weights / templates.
                        if !model.source.is_empty() {
                            let (src_label, src_color) = match model.source.as_str() {
                                "ollama"      => ("Ollama", theme::success()),
                                "huggingface" => ("HF",     theme::PRIMARY),
                                other         => (other,    text_secondary),
                            };
                            let src_resp = egui::Frame {
                                inner_margin: egui::Margin::symmetric(6, 1),
                                corner_radius: CornerRadius::same(3),
                                fill: theme::tinted(src_color, 20),
                                ..Default::default()
                            }
                            .show(ui, |ui| {
                                ui.label(RichText::new(src_label).size(10.0).color(src_color));
                            });
                            // Lazy tooltip — fires the format! only on
                            // hover. The Models tab renders this per
                            // installed model on every frame, so a
                            // 30-model catalog at 60 Hz was 1800
                            // ephemeral allocs/sec for "Source registry:
                            // ollama" strings nobody was reading.
                            src_resp.response.on_hover_ui(|ui| {
                                ui.label(format!("Source registry: {}", model.source));
                            });
                        }

                        // Right-anchored size + action buttons. Disable
                        // when any model action is already in flight —
                        // load/unload/delete/pull each set the global
                        // action_status to InProgress and short-circuit
                        // duplicate dispatches in app.rs. Without the
                        // visual disable, buttons looked clickable but
                        // clicks were no-ops, leaving users wondering
                        // if the GUI was hung.
                        let action_busy = models.action_status.is_in_progress();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !is_loaded {
                                let resp = ui.add_enabled(
                                    !action_busy,
                                    egui::Button::new(RichText::new("Delete").size(11.0)),
                                );
                                let tip = if action_busy {
                                    "Another model action is in progress…"
                                } else {
                                    "Delete this model from disk"
                                };
                                if resp.on_hover_text(tip).clicked() {
                                    // Two-step delete: stash the pending
                                    // model name and let the confirmation
                                    // modal (rendered above the list at
                                    // the top of this render call) gate
                                    // the actual destructive dispatch.
                                    models.delete_confirm_pending = Some(model.name.clone());
                                }
                            }
                            let (label, tip) = if is_loaded {
                                ("Unload", "Unload from memory (keeps the file)")
                            } else {
                                ("Load", "Load into memory for inference")
                            };
                            // UNLOAD is never gated: it is cheap, idempotent and the
                            // one action a user reaches for precisely when something
                            // else is stuck. Only Load (which would fight for VRAM)
                            // waits on the in-flight gate.
                            let gated = action_busy && !is_loaded;
                            let resp = ui.add_enabled(
                                !gated,
                                egui::Button::new(RichText::new(label).size(11.0)),
                            );
                            let hover = if gated {
                                "Another model action is in progress…"
                            } else {
                                tip
                            };
                            if resp.on_hover_text(hover).clicked() {
                                if is_loaded {
                                    actions.push(SettingsAction::UnloadModel(model.name.clone()));
                                } else {
                                    actions.push(SettingsAction::LoadModel(model.name.clone()));
                                }
                            }
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(&model.size)
                                    .size(11.0)
                                    .color(text_secondary)
                                    .monospace(),
                            );
                        });
                    });
                });
                // Whole-card click selects the model. The Load / Unload /
                // Delete buttons inside are separate widgets that take
                // pointer priority, so clicking them doesn't also select
                // (and even if it did, selecting a model you're acting on
                // is harmless). Pointing-hand cursor + tooltip advertise
                // the affordance across the full row, not just the name.
                let card_click = card.response.interact(egui::Sense::click());
                if card_click.clicked() {
                    name_clicked = true;
                }
                card_click.on_hover_text("Click to select this model");
                if name_clicked {
                    models.selected_model = Some(model.name.clone());
                }
                ui.add_space(4.0);
            }
        });

    ui.add_space(10.0);

    // ── 4. Import footer ──────────────────────────────────────────
    egui::Frame {
        inner_margin: egui::Margin::symmetric(12, 10),
        corner_radius: CornerRadius::same(6),
        fill: surface,
        stroke: Stroke::new(1.0, border),
        ..Default::default()
    }
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            Icon::Download.show(ui, 14.0, text_primary);
            ui.add_space(4.0);
            ui.label(
                RichText::new("Import")
                    .size(13.0)
                    .strong()
                    .color(text_primary),
            );
            ui.add_space(8.0);

            // Source selector
            let sources = [("ollama", "Ollama"), ("huggingface", "HuggingFace")];
            for (key, label) in sources {
                let selected = models.pull_source == key;
                let btn = egui::Button::new(
                    RichText::new(label)
                        .size(11.0)
                        .color(if selected { Color32::WHITE } else { text_secondary }),
                )
                .fill(if selected { theme::PRIMARY } else { Color32::TRANSPARENT })
                .corner_radius(CornerRadius::same(3));
                if ui.add(btn).clicked() {
                    models.pull_source = key.to_string();
                }
            }

            ui.add_space(8.0);

            // Pull input + Pull button
            let resp = ui.add(
                egui::TextEdit::singleline(&mut models.pull_model_input)
                    .hint_text("model:tag (e.g. llama3.2:latest)")
                    .desired_width((ui.available_width() - 64.0).max(140.0)),
            );
            let has_input = !models.pull_model_input.trim().is_empty();
            // Same action_status gate as the Load/Unload/Delete buttons:
            // app.rs short-circuits pull_model_with_source if any other
            // action is in flight, so reflect that in the UI rather than
            // letting the user click into a silent no-op.
            let action_busy = models.action_status.is_in_progress();
            let pull_enabled = has_input && !action_busy;
            let pull_btn = egui::Button::new(
                RichText::new("Pull").size(12.0).color(Color32::WHITE),
            )
            .fill(if pull_enabled { theme::PRIMARY } else { Color32::from_gray(80) })
            .corner_radius(CornerRadius::same(4));
            let pull_resp = ui.add_enabled(pull_enabled, pull_btn);
            if action_busy {
                pull_resp.clone().on_hover_text("Another model action is in progress…");
            }
            let pull_clicked = pull_resp.clicked();
            let enter_pressed = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if pull_enabled && (pull_clicked || enter_pressed) {
                actions.push(SettingsAction::PullModel(
                    models.pull_model_input.trim().to_string(),
                    models.pull_source.clone(),
                ));
                models.pull_model_input.clear();
            }
        });

        // Popular suggestions inline below (hide installed entries).
        // Build the installed set once per render rather than re-doing
        // an O(N) Vec::contains for every popular entry — 5 popular ×
        // up to ~50 installed = 250 string compares vs. one HashSet
        // build + 5 lookups.
        let installed: std::collections::HashSet<&str> = models
            .available_models
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        let pending: Vec<(&str, &str)> = POPULAR_MODELS
            .iter()
            .copied()
            .filter(|(name, _)| !installed.contains(name))
            .collect();
        if !pending.is_empty() {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("Quick install:")
                        .size(10.0)
                        .color(text_secondary),
                );
                let pills_busy = models.action_status.is_in_progress();
                for (name, desc) in pending {
                    let btn = egui::Button::new(
                        RichText::new(name).size(10.0).color(text_secondary),
                    )
                    .fill(theme::tinted(theme::PRIMARY, 15))
                    .corner_radius(CornerRadius::same(3));
                    let resp = ui.add_enabled(!pills_busy, btn);
                    // Tooltip surfaces the friendly description from
                    // POPULAR_MODELS (e.g. "TinyLlama 1.1B") plus the
                    // raw pull-tag and source. Without the description a hover
                    // tells the user nothing beyond the bare model:tag string.
                    let tip = if pills_busy {
                        "Another model action is in progress…".to_string()
                    } else {
                        format!(
                            "{}\nPull {} from {}",
                            desc,
                            name,
                            if models.pull_source == "huggingface" { "HuggingFace" } else { "Ollama" },
                        )
                    };
                    if resp.on_hover_text(tip).clicked() {
                        actions.push(SettingsAction::PullModel(
                            name.to_string(),
                            models.pull_source.clone(),
                        ));
                    }
                }
            });
        }
    });

    actions
}

/// A small coloured status chip: an SVG icon + short label on a tinted
/// pill in the given accent colour. Replaces the bare "OK" / "ERR"
/// text labels in the Models status bar so success / failure read as
/// distinct coloured badges (icon backs up the colour for
/// accessibility) rather than plain monochrome words.
fn status_chip(ui: &mut egui::Ui, icon: Icon, label: &str, color: Color32) {
    egui::Frame {
        inner_margin: egui::Margin::symmetric(6, 2),
        corner_radius: CornerRadius::same(3),
        fill: theme::tinted(color, 25),
        ..Default::default()
    }
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            icon.show(ui, 11.0, color);
            ui.add_space(3.0);
            ui.label(RichText::new(label).size(11.0).strong().color(color));
        });
    });
}
