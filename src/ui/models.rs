//! Models section: a chrome row of filters, the list in a well, the import
//! panel under it.
//!
//! Reads/writes: ModelState.

use eframe::egui;
use crate::api::ModelInfo;
use crate::icons::Icon;
use crate::state::{ActionStatus, ModelState, ModelSortDirection, ModelSortField};
use crate::settings::SettingsAction;
use crate::theme::{self, text, HAIRLINE};
use crate::ui::{surface, widgets};
use crate::modality::ModelModality;

/// Popular models for suggestions
const POPULAR_MODELS: &[(&str, &str)] = &[
    ("tinyllama:latest", "TinyLlama 1.1B"),
    ("llama3.2:latest", "Llama 3.2"),
    ("mistral:latest", "Mistral 7B"),
    ("phi3:latest", "Phi-3"),
    ("devstral-small:latest", "Devstral Small"),
];

/// Point size of the icons in the chrome row.
const ICON_PT: f32 = 14.0;
/// Width of the filter field, and of the cells beside it.
const FILTER_W: f32 = 200.0;
const SORT_DIR_W: f32 = 40.0;
/// Fixed cells in a model row: modality, source, size.
const MODALITY_W: f32 = 72.0;
const SOURCE_W: f32 = 48.0;
const SIZE_W: f32 = 64.0;
/// Padding inside a model row.
const ROW_PAD_X: i8 = 12;
const ROW_PAD_Y: i8 = 6;
/// Height kept under the list for the import panel, and the least the list keeps.
const FOOTER_RESERVE: f32 = 200.0;
const LIST_MIN_H: f32 = 120.0;
/// Width of the delete confirmation, and the least room the pull field keeps.
const CONFIRM_W: f32 = 360.0;
const PULL_FIELD_MIN_W: f32 = 140.0;
const PULL_BUTTON_RESERVE: f32 = 64.0;
/// Speed of the indeterminate sweep while a pull has no byte count yet.
const SWEEP_HZ: f64 = 0.55;
/// Side of the close mark that clears the filter.
const CLEAR_PX: f32 = 16.0;

/// Render the models section, returning any actions to perform
pub fn render(
    ui: &mut egui::Ui,
    models: &mut ModelState,
) -> Vec<SettingsAction> {
    let mut actions = Vec::new();

    // Delete is two steps: the name is stashed, a centred window asks, and the
    // action is dispatched on confirmation only. Escape closes without acting.
    let pending_action = if let Some(name) = models.delete_confirm_pending.as_ref() {
        let mut close_modal = false;
        let mut confirmed = false;
        egui::Window::new("Confirm delete")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.set_min_width(CONFIRM_W);
                ui.add_space(widgets::GAP_LABEL);
                ui.label(text::value("Delete this model from disk?"));
                ui.add_space(widgets::GAP_LABEL);
                widgets::well(ui, |ui| {
                    ui.label(text::mono(name));
                });
                ui.add_space(widgets::GAP_WIDGETS);
                ui.label(text::note(
                    "This permanently removes the model files. \
                     You can re-download from the source registry later.",
                ));
                ui.add_space(widgets::GAP_WIDGETS);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(text::note("Cancel"))).clicked() {
                        close_modal = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let delete_btn = egui::Button::image_and_text(
                            Icon::Trash.image(ICON_PT, theme::on_accent()),
                            text::value("Delete").color(theme::on_accent()),
                        )
                        .fill(theme::error());
                        if ui.add(delete_btn).clicked() {
                            confirmed = true;
                        }
                    });
                });
                ui.add_space(widgets::GAP_LABEL);
            });
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

    // The filtered view, computed once so the count and the list agree. The
    // modality is classified once per visible model.
    let filter = models.list_filter.trim().to_string();
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
    if let Some(want) = models.list_modality_filter {
        visible_pairs.retain(|(_, modality)| *modality == want);
    }
    let total = models.available_models.len();
    let visible = visible_pairs.len();

    // The chrome row: title, filter, kind, sort, and in the tail Refresh and the count.
    widgets::chrome_row(ui, |ui| {
        Icon::Package.show(ui, ICON_PT, theme::ink());
        ui.label(text::title("Models"));
        ui.add_space(widgets::GAP_WIDGETS);

        Icon::Search.show(ui, ICON_PT, theme::ink_dim());
        ui.add(
            egui::TextEdit::singleline(&mut models.list_filter)
                .hint_text("Filter")
                .desired_width(FILTER_W),
        );
        // Clearing drops the name filter and the kind together.
        let any_filter_active = !models.list_filter.is_empty() || models.list_modality_filter.is_some();
        if any_filter_active
            && widgets::close_button(ui, CLEAR_PX)
                .on_hover_text("Clear name and kind filters")
                .clicked()
        {
            models.list_filter.clear();
            models.list_modality_filter = None;
        }
        ui.add_space(widgets::GAP_WIDGETS);

        // The kind, one pill seated. Clicking the seated one returns to All.
        ui.label(text::label("KIND"));
        let chips: &[(Option<ModelModality>, &str)] = &[
            (None, "ALL"),
            (Some(ModelModality::Text), "TEXT"),
            (Some(ModelModality::Vision), "VISION"),
            (Some(ModelModality::ImageGen), "IMAGE"),
            (Some(ModelModality::AudioTts), "TTS"),
            (Some(ModelModality::AudioAsr), "ASR"),
            (Some(ModelModality::VideoGen), "VIDEO"),
        ];
        for (kind, label) in chips {
            let selected = models.list_modality_filter == *kind;
            if widgets::selector_pill(ui, label, selected).clicked() {
                models.list_modality_filter = if selected { None } else { *kind };
            }
        }
        ui.add_space(widgets::GAP_WIDGETS);

        // The sort field, and its direction as a word in a fixed cell so the
        // pills keep their width.
        ui.label(text::label("SORT"));
        for (field, label) in [
            (ModelSortField::Name, "NAME"),
            (ModelSortField::Size, "SIZE"),
            (ModelSortField::Date, "DATE"),
        ] {
            let selected = models.sort_field == field;
            if widgets::selector_pill(ui, label, selected).clicked() {
                if selected {
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
        let direction = match models.sort_direction {
            ModelSortDirection::Asc => "ASC",
            ModelSortDirection::Desc => "DESC",
        };
        widgets::fixed_label(ui, SORT_DIR_W, text::label(direction));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, Icon::Refresh, "Reload the model list from the server").clicked() {
                actions.push(SettingsAction::RefreshModels);
            }
            ui.add_space(widgets::GAP_WIDGETS);
            let count_text = if filter.is_empty() || visible == total {
                format!("{} model{}", total, if total == 1 { "" } else { "s" })
            } else {
                format!("{} of {}", visible, total)
            };
            widgets::readout(ui, widgets::READOUT_WIDE_W, &count_text);
        });
    });
    ui.add_space(widgets::GAP_WIDGETS);

    // The status of the action in flight, or of the last one.
    match &models.action_status {
        ActionStatus::InProgress(msg) => {
            // A pull is the one long action. Its meter is determinate once the
            // server streams a byte count and a sweep before; app.rs repaints
            // while any action is in progress.
            let is_pull = msg.starts_with("Pulling");
            widgets::panel_frame(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::lamp_inline(ui, true, theme::accent());
                    ui.label(text::note(msg));
                });
                if is_pull {
                    ui.add_space(widgets::GAP_LABEL);
                    match models.pull_progress {
                        Some((completed, total)) if total > 0 => {
                            let frac = (completed as f32 / total as f32).clamp(0.0, 1.0);
                            ui.horizontal(|ui| {
                                surface::meter(ui, frac, widgets::METER_SIZE, theme::accent());
                                widgets::readout(
                                    ui,
                                    widgets::READOUT_WIDE_W,
                                    &format!(
                                        "{:.0}% {} / {}",
                                        frac * 100.0,
                                        crate::api::types::format_size(completed),
                                        crate::api::types::format_size(total),
                                    ),
                                );
                            });
                        }
                        _ => {
                            let t = ui.input(|i| i.time);
                            let frac = (t * SWEEP_HZ).rem_euclid(1.0) as f32;
                            surface::meter(ui, frac, widgets::METER_SIZE, theme::accent_dim());
                        }
                    }
                }
            });
            ui.add_space(widgets::GAP_WIDGETS);
        }
        ActionStatus::Success(msg) => {
            ui.horizontal(|ui| {
                widgets::lamp_inline(ui, true, theme::success());
                ui.label(text::label("OK"));
                ui.label(text::note(msg));
            });
            ui.add_space(widgets::GAP_WIDGETS);
        }
        ActionStatus::Failed(msg) => {
            ui.horizontal(|ui| {
                widgets::lamp_inline(ui, true, theme::error());
                ui.label(text::label("ERROR"));
                ui.label(text::note(msg));
            });
            ui.add_space(widgets::GAP_WIDGETS);
        }
        ActionStatus::Idle => {}
    }

    // The list, in a well that keeps room for the import panel under it.
    let list_height = (ui.available_height() - FOOTER_RESERVE - 2.0 * widgets::SECTION_PADDING).max(LIST_MIN_H);
    widgets::well(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("models_list_scroll")
            .max_height(list_height)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if visible_pairs.is_empty() {
                    let msg = if filter.is_empty() {
                        "No models found. Use the Import panel below to download one."
                    } else {
                        "No models match the filter."
                    };
                    ui.add_space(widgets::GAP_SECTIONS);
                    ui.vertical_centered(|ui| {
                        ui.label(text::note(msg));
                    });
                    return;
                }
                for (model, modality) in &visible_pairs {
                    let modality = *modality;
                    let is_loaded = models.is_loaded(&model.name);
                    let is_selected = models.selected_model.as_deref() == Some(&model.name);

                    // A row: raised when selected, with a stripe; a hairline under
                    // each. The whole row is the select target, sensed for hover
                    // only: a click sense registered after the buttons would sit on
                    // top of them and take their clicks. A click that no button took
                    // selects.
                    let mut acted = false;
                    let fill = if is_selected { theme::raised() } else { egui::Color32::TRANSPARENT };
                    let row = egui::Frame::NONE
                        .fill(fill)
                        .corner_radius(theme::RADIUS)
                        .inner_margin(egui::Margin::symmetric(ROW_PAD_X, ROW_PAD_Y))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                widgets::lamp_inline(ui, is_loaded, theme::success())
                                    .on_hover_text(if is_loaded { "Loaded in memory" } else { "On disk, not loaded" });
                                ui.label(text::value(&model.name));
                                if modality != ModelModality::Text {
                                    widgets::fixed_label(ui, MODALITY_W, text::note(modality.label()))
                                        .on_hover_text(modality.tooltip());
                                }
                                if !model.source.is_empty() {
                                    let src_label = match model.source.as_str() {
                                        "ollama" => "Ollama",
                                        "huggingface" => "HF",
                                        other => other,
                                    };
                                    widgets::fixed_label(ui, SOURCE_W, text::note(src_label)).on_hover_ui(|ui| {
                                        ui.label(format!("Source registry: {}", model.source));
                                    });
                                }

                                // The actions, in the tail. Load waits on any action in
                                // flight; Unload never does, it is what a user reaches for
                                // when something else is stuck.
                                let action_busy = models.action_status.is_in_progress();
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if !is_loaded {
                                        let resp = ui.add_enabled(
                                            !action_busy,
                                            egui::Button::new(text::note("Delete")).frame(false),
                                        );
                                        let tip = if action_busy {
                                            "Another model action is in progress"
                                        } else {
                                            "Delete this model from disk"
                                        };
                                        if resp.on_hover_text(tip).clicked() {
                                            acted = true;
                                            models.delete_confirm_pending = Some(model.name.clone());
                                        }
                                    }
                                    let (label, tip) = if is_loaded {
                                        ("Unload", "Unload from memory (keeps the file)")
                                    } else {
                                        ("Load", "Load into memory for inference")
                                    };
                                    let gated = action_busy && !is_loaded;
                                    let resp = ui.add_enabled(!gated, egui::Button::new(text::note(label)));
                                    let hover = if gated { "Another model action is in progress" } else { tip };
                                    if resp.on_hover_text(hover).clicked() {
                                        acted = true;
                                        if is_loaded {
                                            actions.push(SettingsAction::UnloadModel(model.name.clone()));
                                        } else {
                                            actions.push(SettingsAction::LoadModel(model.name.clone()));
                                        }
                                    }
                                    ui.add_space(widgets::GAP_LABEL);
                                    widgets::readout(ui, SIZE_W, &model.size);
                                });
                            });
                        });
                    let rect = row.response.rect;
                    if is_selected {
                        widgets::stripe(ui, rect, theme::accent());
                    }
                    ui.painter().hline(
                        rect.x_range(),
                        rect.bottom() + HAIRLINE / 2.0,
                        egui::Stroke::new(HAIRLINE, theme::border()),
                    );
                    let row_hover = ui.interact(rect, row.response.id.with("select"), egui::Sense::hover());
                    let clicked_in_row =
                        row_hover.contains_pointer() && ui.input(|i| i.pointer.primary_clicked());
                    row_hover.on_hover_text("Click to select this model");
                    if clicked_in_row && !acted {
                        models.selected_model = Some(model.name.clone());
                    }
                }
            });
    });
    ui.add_space(widgets::GAP_WIDGETS);

    // Import: the source, the name to pull, and the quick installs.
    widgets::section_panel(ui, "IMPORT", |ui| {
        ui.horizontal(|ui| {
            Icon::Download.show(ui, ICON_PT, theme::ink_dim());
            let sources = [("ollama", "OLLAMA"), ("huggingface", "HUGGINGFACE")];
            for (key, label) in sources {
                if widgets::selector_pill(ui, label, models.pull_source == key).clicked() {
                    models.pull_source = key.to_string();
                }
            }
            ui.add_space(widgets::GAP_WIDGETS);

            let resp = ui.add(
                egui::TextEdit::singleline(&mut models.pull_model_input)
                    .hint_text("model:tag (e.g. llama3.2:latest)")
                    .desired_width((ui.available_width() - PULL_BUTTON_RESERVE).max(PULL_FIELD_MIN_W)),
            );
            let has_input = !models.pull_model_input.trim().is_empty();
            // The same gate as Load: app.rs refuses a pull while another action
            // is in flight.
            let action_busy = models.action_status.is_in_progress();
            let pull_enabled = has_input && !action_busy;
            let pull_btn = egui::Button::new(text::value("Pull").color(theme::on_accent())).fill(theme::accent());
            let pull_resp = ui.add_enabled(pull_enabled, pull_btn);
            if action_busy {
                pull_resp.clone().on_hover_text("Another model action is in progress");
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

        // Popular models not yet installed.
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
            ui.add_space(widgets::GAP_LABEL);
            ui.horizontal_wrapped(|ui| {
                ui.label(text::label("QUICK INSTALL"));
                let pills_busy = models.action_status.is_in_progress();
                for (name, desc) in pending {
                    let resp = ui.add_enabled_ui(!pills_busy, |ui| widgets::selector_pill(ui, name, false)).inner;
                    let tip = if pills_busy {
                        "Another model action is in progress".to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ModelInfo;
    use egui_kittest::kittest::Queryable;

    /// A row selects the model on click, and its Load button still takes the click
    /// meant for it: the row's click sense sits under the buttons, never over them.
    #[test]
    fn the_load_button_takes_its_click() {
        let mut models = ModelState::default();
        models.available_models = vec![ModelInfo {
            name: "llama3.2:1b".into(),
            size: "1.3 GB".into(),
            size_bytes: 0,
            modified_at: String::new(),
            source: "ollama".into(),
            family: "llama".into(),
            capabilities: vec!["chat".into()],
            defaults: None,
        }];
        let actions = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let selected = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
        let sink = actions.clone();
        let chosen = selected.clone();
        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            let out = render(ui, &mut models);
            sink.borrow_mut().extend(out);
            *chosen.borrow_mut() = models.selected_model.clone();
        });
        harness.run();
        harness.get_by_label("Load").click();
        harness.run();
        // A click on the name, which no button owns, selects the row.
        harness.get_by_label("llama3.2:1b").click();
        harness.run();
        drop(harness);
        let loads = actions
            .borrow()
            .iter()
            .filter(|a| matches!(a, SettingsAction::LoadModel(_)))
            .count();
        assert_eq!(loads, 1, "one Load, from the button");
        assert_eq!(selected.borrow().as_deref(), Some("llama3.2:1b"), "the row click selected");
    }
}
