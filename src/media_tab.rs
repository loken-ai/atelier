//! Media Studio tab: generate every media modality from the GUI.
//!
//! One dedicated tab that fans out to the server's `/v1/*` generation
//! endpoints: images, music, sound effects, MIDI, video, and speech.
//! A chrome row, a tab bar of kinds, the prompt in a panel, the parameters
//! of the selected kind grouped in section panels, Generate, a status line,
//! and the results and the earlier ones each behind the glass of a screen.
//!
//! Results are surfaced by how they can be consumed under the GUI's
//! no-new-crates constraint (no in-process audio/video codec):
//!   - images render inline via `texture::load_base64_texture`,
//!   - audio (music / SFX / speech) plays through the system player
//!     (`audio_playback::play_audio_blob`) and can be saved,
//!   - MIDI + video can only be saved to disk and opened in an external
//!     app (they aren't renderable here).

use std::collections::HashMap;

use eframe::egui;

use crate::icons::Icon;
use crate::state::{
    ChatDialogResult, MediaAudioSlot, MediaKind, MediaState, VideoFormat, VideoSampler,
};
use crate::theme::{self, text};
use crate::ui::{surface, widgets};

/// Point size of the icons in the chrome row and beside a control.
const ICON_PT: f32 = 14.0;
const ICON_PT_SMALL: f32 = 11.0;
/// Rows of the prompt box.
const PROMPT_ROWS: usize = 3;
/// Width of the picker of the model that rewrites a prompt.
const ENHANCE_MODEL_W: f32 = 150.0;
/// Width of a fixed text field in a form.
const FIELD_W: f32 = 220.0;
/// The Generate and Cancel buttons.
const GENERATE_SIZE: egui::Vec2 = egui::Vec2::new(140.0, 34.0);
const CANCEL_SIZE: egui::Vec2 = egui::Vec2::new(90.0, 34.0);
/// Longest side of a result image, and of the inline video frame.
const RESULT_MAX_PX: f32 = 360.0;
const VIDEO_INLINE_MAX_PX: f32 = 420.0;
/// The hover bar over a result image: its inset, height, gap between halves.
const OVERLAY_PAD: f32 = 6.0;
const OVERLAY_BAR_H: f32 = 24.0;
const OVERLAY_GAP: f32 = 6.0;
/// The scrim behind the hover bar over a picture, which is not a surface of
/// the palette: it darkens whatever picture is under it.
const OVERLAY_SCRIM: egui::Color32 = egui::Color32::from_black_alpha(170);
const OVERLAY_BUTTON_SCRIM: egui::Color32 = egui::Color32::from_black_alpha(120);
/// Period of the indeterminate sweep, in seconds, and its repaint interval.
const SWEEP_PERIOD_S: f32 = 3.0;
const SWEEP_REPAINT_MS: u64 = 100;
/// Repaint interval while audio plays, so the position moves.
const AUDIO_REPAINT_MS: u64 = 200;
/// Characters of a history entry's prompt shown in its tooltip.
const HISTORY_LABEL_CHARS: usize = 40;

/// Signals raised by the Media Studio render pass that the app layer
/// must act on (they need the HTTP client / tokio runtime, which the
/// tab doesn't own). Everything else — Play, Save, kind switching,
/// param edits — is handled inline within `render`.
#[derive(Debug, Default, Clone, Copy)]
pub struct MediaRenderOutput {
    /// The Generate button was clicked this frame (and was enabled).
    pub generate_clicked: bool,
    /// The "refresh voices" button (Speech kind) was clicked.
    pub refresh_voices_clicked: bool,
    /// The "Enhance" button next to the prompt was clicked.
    pub enhance_clicked: bool,
}

/// Compact label for a model id in a narrow combo: keep the tail after the last
/// '/' (registry prefixes are noise here) and cap the length.
fn short_model_label(name: &str) -> String {
    let tail = name.rsplit('/').next().unwrap_or(name);
    if tail.chars().count() <= 22 {
        tail.to_string()
    } else {
        format!("{}…", tail.chars().take(21).collect::<String>())
    }
}

/// Render the Media Studio tab.
pub fn render(
    ui: &mut egui::Ui,
    media: &mut MediaState,
    image_textures: &mut HashMap<String, egui::TextureHandle>,
    models: &[crate::api::types::ModelInfo],
    loras: &[(String, Option<String>)],
    player: &mut crate::audio_playback::AudioPlayer,
    video: &mut crate::video_engine::VideoPlayback,
) -> MediaRenderOutput {
    let mut out = MediaRenderOutput::default();

    // Drain any completed file-save dialog before drawing so the status
    // line reflects the write result this frame.
    crate::dialog::drain_pending_dialog(media);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            widgets::chrome_row(ui, |ui| {
                Icon::Bolt.show(ui, ICON_PT, theme::ink());
                ui.label(text::title("Media Studio"));
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::caption_row(
                    ui,
                    "Generate images, music, sound effects, MIDI, video, and speech.",
                );
            });
            ui.add_space(widgets::GAP_WIDGETS);

            // The kinds, as tabs; the selected one is underlined.
            let labels: Vec<&str> = MediaKind::ALL.iter().map(|k| k.label()).collect();
            let selected = MediaKind::ALL
                .iter()
                .position(|k| *k == media.kind)
                .unwrap_or(0);
            if let Some(i) = widgets::tab_bar(ui, selected, &labels) {
                media.set_kind(MediaKind::ALL[i]);
            }
            widgets::caption_row(ui, media.kind.tip());
            ui.add_space(widgets::GAP_WIDGETS);

            // The prompt, in its own panel, on a well.
            if !matches!(media.kind, MediaKind::Transcribe | MediaKind::Separate) {
                let prompt_title = match media.kind {
                    MediaKind::Speech => "TEXT",
                    _ => "PROMPT",
                };
                widgets::section_panel(ui, prompt_title, |ui| {
                    let hint = media.kind.prompt_hint();
                    widgets::well(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut media.prompt)
                                .frame(egui::Frame::NONE)
                                .desired_rows(PROMPT_ROWS)
                                .desired_width(f32::INFINITY)
                                .hint_text(hint),
                        );
                    });
                    // The enhancer rewrites the prompt with a local model tuned to this
                    // kind; Undo restores the text it replaced.
                    ui.add_space(widgets::GAP_LABEL);
                    ui.horizontal(|ui| {
                        if media.enhancing_prompt {
                            widgets::lamp_inline(ui, true, theme::accent());
                            ui.label(text::label("ENHANCING"));
                        } else {
                            let can_enhance =
                                !media.prompt.trim().is_empty() && !media.is_generating;
                            if ui
                                .add_enabled(
                                    can_enhance,
                                    egui::Button::image_and_text(
                                        Icon::Bolt.image(ICON_PT_SMALL, theme::ink()),
                                        text::note("Enhance"),
                                    ),
                                )
                                .on_hover_text(
                                    "Rewrite the prompt with a local model: adds concrete detail, \
                                     style and quality cues suited to this media type.",
                                )
                                .clicked()
                            {
                                out.enhance_clicked = true;
                            }
                            // Which model rewrites. Auto walks the candidates and validates
                            // each reply; an explicit pick is used as-is.
                            let chat_models: Vec<&str> = models
                                .iter()
                                .filter(|m| m.has_capability("chat"))
                                .map(|m| m.name.as_str())
                                .collect();
                            if !chat_models.is_empty() {
                                let current = media
                                    .enhance_model
                                    .clone()
                                    .unwrap_or_else(|| "Auto".to_string());
                                egui::ComboBox::from_id_salt("enhance_model")
                                    .selected_text(text::note(&short_model_label(&current)))
                                    .width(ENHANCE_MODEL_W)
                                    .show_ui(ui, |ui| {
                                        if ui
                                            .selectable_label(media.enhance_model.is_none(), "Auto")
                                            .on_hover_text(
                                                "Try capable models in order and keep the first \
                                                 reply that is really an improved prompt.",
                                            )
                                            .clicked()
                                        {
                                            media.enhance_model = None;
                                        }
                                        for name in &chat_models {
                                            let sel = media.enhance_model.as_deref() == Some(*name);
                                            if ui.selectable_label(sel, *name).clicked() {
                                                media.enhance_model = Some((*name).to_string());
                                            }
                                        }
                                    })
                                    .response
                                    .on_hover_text("Model used by Enhance");
                            }
                            if media.prompt_before_enhance.is_some()
                                && ui
                                    .add(egui::Button::new(text::note("Undo")).frame(false))
                                    .on_hover_text(
                                        "Restore the prompt as it was before the enhancement.",
                                    )
                                    .clicked()
                            {
                                if let Some(prev) = media.prompt_before_enhance.take() {
                                    media.prompt = prev;
                                }
                            }
                        }
                    });
                });
                ui.add_space(widgets::GAP_WIDGETS);
            }

            // The parameters of this kind, grouped in section panels.
            render_params(ui, media, &mut out, models, loras);
            ui.add_space(widgets::GAP_WIDGETS);

            // ── Generate button + busy state ──────────────────────────
            let inputs_ready = match media.kind {
                // Image Edit needs an instruction + a source image.
                MediaKind::ImageEdit => {
                    !media.prompt.trim().is_empty() && media.image_edit.source.is_some()
                }
                // Transcribe needs only an audio file.
                MediaKind::Transcribe => media.transcribe.audio.is_some(),
                MediaKind::Separate => media.separate.audio.is_some(),
                _ => !media.prompt.trim().is_empty(),
            };
            let can_generate = !media.is_generating && inputs_ready;
            ui.horizontal(|ui| {
                let gen_btn = egui::Button::image_and_text(
                    Icon::Bolt.image(ICON_PT, theme::on_accent()),
                    text::value(if media.is_generating {
                        "Generating..."
                    } else {
                        "Generate"
                    })
                    .color(theme::on_accent()),
                )
                .fill(theme::accent())
                .min_size(GENERATE_SIZE);
                if ui.add_enabled(can_generate, gen_btn).clicked() {
                    out.generate_clicked = true;
                }
                if media.is_generating {
                    ui.add_space(widgets::GAP_WIDGETS);
                    let cancel_btn = egui::Button::image_and_text(
                        Icon::Cross.image(ICON_PT_SMALL, theme::on_accent()),
                        text::value("Cancel").color(theme::on_accent()),
                    )
                    .fill(theme::error())
                    .min_size(CANCEL_SIZE);
                    if ui
                        .add(cancel_btn)
                        .on_hover_text(
                            "Stop this generation now: the server is told to cancel the \
                             render, and both the denoise and the decode stop at their \
                             next step.",
                        )
                        .clicked()
                    {
                        // The identifier is handed to the caller, which owns the HTTP
                        // client: stopping a render means TELLING the server, because
                        // dropping the connection does not.
                        media.pending_cancel = media.cancel_generation();
                    }
                    ui.add_space(widgets::GAP_WIDGETS);
                    widgets::lamp_inline(ui, true, theme::accent());
                }
                // What this render is expected to cost, next to the button that starts it.
                // Video is the one kind where the answer can be an hour, which is not
                // something to discover by waiting it out.
                //
                // Absent until the server has answered: nothing at all beats a zero, which
                // reads as "instant" for precisely the settings that are not.
                if media.kind == MediaKind::Video && !media.is_generating {
                    if let Some(seconds) = media.video_estimate {
                        ui.add_space(widgets::GAP_WIDGETS);
                        ui.label(text::readout(&format!(
                            "{} of denoising",
                            format_estimate(seconds)
                        )))
                        .on_hover_text(
                            "Expected time in the denoising loop for these settings. \
                             Loading the checkpoint and decoding the frames are on top \
                             of it, so a short clip takes noticeably longer than this \
                             and a long one barely more.",
                        );
                    }
                }
            });
            ui.add_space(widgets::GAP_WIDGETS);

            render_status(ui, media);

            if let Some(err) = media.error.clone() {
                ui.add_space(widgets::GAP_LABEL);
                widgets::panel_frame(ui, |ui| {
                    ui.horizontal(|ui| {
                        widgets::lamp_inline(ui, true, theme::error());
                        ui.label(text::note(&err).color(theme::error()));
                    });
                });
            }

            render_results(ui, media, image_textures, player, video);

            ui.add_space(widgets::GAP_SECTIONS);
        });

    // Drawn LAST and against the context, not the scroll area, so it covers the tab
    // rather than scrolling with it.
    render_video_viewer(ui.ctx(), &mut media.video_viewer, video);

    out
}

/// A wait, said the way someone waiting would say it: `~45 s`, `~2.8 min`, `~1.4 h`.
///
/// The unit changes with the magnitude because the precision that matters changes with it.
/// Seconds past a minute are noise, and a four-digit second count is a number the reader
/// has to divide themselves - which is the arithmetic this label exists to spare them.
///
/// The tilde is not decoration: the estimate comes from one measured render scaled by
/// tokens and passes, so it is the right order of magnitude and not a promise.
fn format_estimate(seconds: f32) -> String {
    let seconds = seconds.max(0.0);
    if seconds < 90.0 {
        format!("~{seconds:.0} s")
    } else if seconds < 5400.0 {
        format!("~{:.1} min", seconds / 60.0)
    } else {
        format!("~{:.1} h", seconds / 3600.0)
    }
}

/// The parameters of the selected kind, grouped in section panels; the
/// kinds with many sit in two columns. Each control carries its description
/// on hover, and each section ends with the one sentence that names its
/// trade-off. A control the selected model ignores is not shown.
fn render_params(
    ui: &mut egui::Ui,
    media: &mut MediaState,
    out: &mut MediaRenderOutput,
    models: &[crate::api::types::ModelInfo],
    loras: &[(String, Option<String>)],
) {
    match media.kind {
        MediaKind::Image => {
            // Every image-gen model the server reports; the selection falls back to
            // the first when the remembered one is gone.
            let image_models: Vec<&str> = models
                .iter()
                .filter(|m| m.is_image_gen())
                .map(|m| m.name.as_str())
                .collect();
            if !image_models.is_empty() && !image_models.contains(&media.image.model.as_str()) {
                media.image.model = image_models[0].to_string();
            }
            // A newly picked model brings its own recommended settings: a distilled
            // model is built for 9 steps at guidance 5, a base model needs 25 at 7.
            if media.image.applied_defaults_for.as_deref() != Some(media.image.model.as_str()) {
                if let Some(d) = models
                    .iter()
                    .find(|m| m.name == media.image.model)
                    .and_then(|m| m.defaults.as_ref())
                {
                    if let Some(v) = d.get("steps").and_then(serde_json::Value::as_u64) {
                        media.image.steps = v as u32;
                    }
                    if let Some(v) = d.get("cfg").and_then(serde_json::Value::as_f64) {
                        media.image.guidance = v as f32;
                    }
                    if let Some(v) = d.get("size").and_then(serde_json::Value::as_u64) {
                        media.image.width = v as u32;
                        media.image.height = v as u32;
                    }
                    media.image.applied_defaults_for = Some(media.image.model.clone());
                }
            }
            let family = model_family_of(models, &media.image.model);
            let takes_loras = family_takes_loras(models, &media.image.model);
            let is_sdxl = family == "sdxl";
            ui.columns(2, |cols| {
                let ui = &mut cols[0];
                widgets::section_panel(ui, "MODEL", |ui| {
                    param_grid(ui, "media_params_image_model", |ui| {
                        param_label(ui, "MODEL");
                        egui::ComboBox::from_id_salt("media_image_model")
                            .selected_text(if media.image.model.is_empty() {
                                "(no image model)".to_string()
                            } else {
                                media.image.model.clone()
                            })
                            .show_ui(ui, |ui| {
                                if image_models.is_empty() {
                                    ui.label("No image-gen model available");
                                }
                                for name in &image_models {
                                    ui.selectable_value(&mut media.image.model, name.to_string(), *name);
                                }
                            })
                            .response
                            .on_hover_text(
                                "Which image model renders: each has its own style, speed and \
                                 ideal step count (turbo models need few steps, base models more).",
                            );
                        ui.end_row();
                        file_format_row(ui, &mut media.image.file_format);
                    });
                    widgets::caption_row(ui, "Picking a model applies its own recommended steps, guidance and size.");
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "SHAPE AND SIZE", |ui| {
                    param_grid(ui, "media_params_image_shape", |ui| {
                        shape_row(ui, &mut media.image.width, &mut media.image.height);
                        size_row(ui, &mut media.image.width, &mut media.image.height, 64..=2048);
                    });
                    widgets::caption_row(ui, "The named shapes are the buckets these models were trained on; another ratio costs some composition.");
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "SAMPLING", |ui| {
                    param_grid(ui, "media_params_image_sampling", |ui| {
                        slider_row(ui, "STEPS", &mut media.image.steps, 1..=200, SliderOpts::default(),
                            "Denoising iterations: more is finer detail up to the model's sweet \
                             spot, with time growing linearly.");
                        slider_row(ui, "COUNT", &mut media.image.n, 1..=8, SliderOpts::default(),
                            "Number of variations rendered in one run (seed+1 each).");
                        // The solver is selectable for the SDXL family only.
                        if is_sdxl {
                            solver_rows(ui, &mut media.image.sampler, &mut media.image.scheduler);
                        }
                    });
                    widgets::caption_row(ui, "Turbo and distilled models are built for 4-9 steps; base models like 20-50.");
                });

                let ui = &mut cols[1];
                widgets::section_panel(ui, "GUIDANCE", |ui| {
                    param_grid(ui, "media_params_image_guidance", |ui| {
                        slider_row(ui, "GUIDANCE", &mut media.image.guidance, 0.0..=30.0, SliderOpts { decimals: 2, ..Default::default() },
                            "Prompt adherence (CFG): higher follows the text more literally but \
                             can over-saturate and distort; lower is freer. 0 is the model's own \
                             recommended value.");
                        param_label(ui, "NEGATIVE");
                        ui.add(
                            egui::TextEdit::multiline(&mut media.image.negative_prompt)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .hint_text("watermark, extra fingers, blurry..."),
                        )
                        .on_hover_text(
                            "What the render should steer away from: a watermark, a warped hand. \
                             Empty keeps the model family's own default.",
                        );
                        ui.end_row();
                    });
                    widgets::caption_row(ui, "The negative prompt removes what a prompt cannot name.");
                });
                // Adapters and pose are honoured by some families only; a knob the
                // model ignores is not shown.
                if takes_loras || is_sdxl {
                    ui.add_space(widgets::GAP_WIDGETS);
                    widgets::section_panel(ui, "ADAPTERS", |ui| {
                        param_grid(ui, "media_params_image_adapters", |ui| {
                            if takes_loras {
                                lora_rows(ui, &mut media.image.loras, loras, &family);
                            }
                            if is_sdxl {
                                audio_picker_row(
                                    ui,
                                    media,
                                    "POSE",
                                    MediaAudioSlot::ImageControl,
                                    "Optional. A pose or edge image that says where things go, \
                                     which a prompt cannot. At strength 0 the render is what it \
                                     would have been without one.",
                                );
                                if media.image.control.is_some() {
                                    slider_row(ui, "POSE STRENGTH", &mut media.image.control_scale, 0.0..=2.0,
                                        SliderOpts { decimals: 2, ..Default::default() },
                                        "How hard the pose image constrains the composition.");
                                }
                            }
                        });
                        widgets::caption_row(ui, "Only the adapters this model's architecture can take are offered.");
                    });
                }
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "REGIONS", |ui| {
                    param_grid(ui, "media_params_image_regions", |ui| {
                        region_rows(ui, &mut media.image.regions);
                    });
                    widgets::caption_row(ui, "A prompt naming two subjects renders them merged; give each an area and they stay apart.");
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "SEED", |ui| {
                    param_grid(ui, "media_params_image_seed", |ui| {
                        seed_row(ui, media);
                    });
                    widgets::caption_row(ui, "Same seed and settings, same result. Empty is a fresh seed each run.");
                });
            });
        }
        MediaKind::Music => {
            const DIT_MODELS: &[(&str, &str, u32)] = &[
                ("turbo", "Turbo (fast, 8-step)", 27),
                ("sft", "SFT 2B (quality, ~50 steps)", 50),
                ("base", "Base 2B (~50 steps)", 50),
                ("xl-turbo", "XL Turbo 4B", 27),
                ("xl-sft", "XL SFT 4B (best, ~50 steps)", 50),
                ("xl-base", "XL Base 4B (~50 steps)", 50),
            ];
            const REC_CFG: &[(&str, f32)] = &[
                ("turbo", 1.0),
                ("sft", 4.5),
                ("base", 4.5),
                ("xl-turbo", 1.0),
                ("xl-sft", 4.5),
                ("xl-base", 4.5),
            ];
            ui.columns(2, |cols| {
                let ui = &mut cols[0];
                widgets::section_panel(ui, "COMPOSITION", |ui| {
                    param_grid(ui, "media_params_music_composition", |ui| {
                        param_label(ui, "MODEL");
                        let current_label = DIT_MODELS
                            .iter()
                            .find(|(id, _, _)| *id == media.music.dit_model)
                            .map(|(_, l, _)| *l)
                            .unwrap_or("Turbo (fast, 8-step)");
                        egui::ComboBox::from_id_salt("media_music_dit")
                            .selected_text(current_label)
                            .show_ui(ui, |ui| {
                                for (id, label, rec_steps) in DIT_MODELS {
                                    if ui
                                        .selectable_label(media.music.dit_model == *id, *label)
                                        .clicked()
                                    {
                                        media.music.dit_model = (*id).to_string();
                                        // The checkpoint's recommended step count and CFG follow.
                                        media.music.steps = *rec_steps;
                                        if let Some((_, c)) = REC_CFG.iter().find(|(mid, _)| mid == id) {
                                            media.music.cfg = *c;
                                        }
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                "ACE-Step checkpoint: Turbo renders in seconds (8-27 steps, CFG-free); \
                                 SFT/Base follow prompts and lyrics better but want ~50 steps; XL \
                                 variants (4B) are richer and slower.",
                            );
                        ui.end_row();
                        slider_row(ui, "DURATION", &mut media.music.seconds, 2.0..=600.0,
                            SliderOpts { suffix: " s", logarithmic: true, decimals: 0 },
                            "Track length, up to 10 minutes. Render time grows with it; without \
                             Full length the model may end the song earlier.");
                        param_label(ui, "LOOP");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut media.music.loop_mode, "seamless loop");
                            if media.music.loop_mode {
                                widgets::drag_fixed(ui, egui::DragValue::new(&mut media.music.loop_bars).range(1..=64).suffix(" bars"), widgets::READOUT_WIDE_W);
                            }
                        })
                        .response
                        .on_hover_text("Bar-exact length from bars x bpm, tail crossfaded into the head: drops into a DAW and loops without a click.");
                        ui.end_row();
                        param_label(ui, "FULL LENGTH");
                        ui.checkbox(&mut media.music.force_duration, "force the whole duration")
                            .on_hover_text("Ban the model's natural end-of-song until the requested length is reached.");
                        ui.end_row();
                        slider_row(ui, "TEMPO", &mut media.music.bpm, 40..=220, SliderOpts { suffix: " bpm", ..Default::default() },
                            "Beats per minute; also the bar grid for loops.");
                    });
                    widgets::caption_row(ui, "Picking a checkpoint sets its recommended steps and CFG.");
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "RENDERING", |ui| {
                    param_grid(ui, "media_params_music_rendering", |ui| {
                        slider_row(ui, "STEPS", &mut media.music.steps, 1..=200, SliderOpts::default(),
                            "Diffusion steps for the audio detail pass. Turbo: ~8-27; SFT/Base: ~50.");
                        slider_row(ui, "CFG", &mut media.music.cfg, 1.0..=10.0, SliderOpts { decimals: 1, ..Default::default() },
                            "Caption and lyrics adherence. Turbo is CFG-free (keep 1.0); SFT/Base \
                             follow the prompt better around 4-7, at two evaluations per step.");
                        slider_row(ui, "TEMPERATURE", &mut media.music.temperature, 0.1..=1.5, SliderOpts { decimals: 2, ..Default::default() },
                            "Composition randomness: low is safe and repetitive, high is adventurous.");
                        slider_row(ui, "TOP-P", &mut media.music.top_p, 0.1..=1.0, SliderOpts { decimals: 2, ..Default::default() },
                            "Nucleus cutoff on the composer: lower keeps only the likeliest continuations.");
                    });
                    widgets::caption_row(ui, "Turbo wants CFG 1.0; SFT and Base follow the prompt around 4-7.");
                });

                let ui = &mut cols[1];
                widgets::section_panel(ui, "CONDITIONING", |ui| {
                    param_grid(ui, "media_params_music_conditioning", |ui| {
                        param_label(ui, "KEY / SCALE");
                        ui.text_edit_singleline(&mut media.music.keyscale)
                            .on_hover_text("Tonality constraint, e.g. C minor, A major. Empty is the model's choice.");
                        ui.end_row();
                        param_label(ui, "LANGUAGE");
                        ui.text_edit_singleline(&mut media.music.language)
                            .on_hover_text("Lyrics language code (en, fr, ...); guides pronunciation.");
                        ui.end_row();
                        param_label(ui, "LYRICS");
                        ui.add(
                            egui::TextEdit::multiline(&mut media.music.lyrics)
                                .desired_rows(4)
                                .desired_width(f32::INFINITY)
                                .hint_text("[verse]\nCity lights below us..."),
                        )
                        .on_hover_text(
                            "Sung text. [verse], [chorus] and [bridge] tags shape the arrangement; \
                             with lyrics present the server adds a sung-vocals directive.",
                        );
                        ui.end_row();
                        param_label(ui, "NEGATIVE");
                        ui.add(
                            egui::TextEdit::multiline(&mut media.music.negative_prompt)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .hint_text("spoken word, talking, monotone..."),
                        )
                        .on_hover_text("What the music should avoid. Effective when CFG is above 1.");
                        ui.end_row();
                    });
                    widgets::caption_row(ui, "Lyrics tags shape the arrangement; the negative prompt needs CFG above 1.");
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "SEED", |ui| {
                    param_grid(ui, "media_params_music_seed", |ui| {
                        seed_row(ui, media);
                    });
                    widgets::caption_row(ui, "Same seed and settings, same result. Empty is a fresh seed each run.");
                });
            });
        }
        MediaKind::Sfx => {
            let sfx_models: Vec<&crate::api::types::ModelInfo> =
                models.iter().filter(|m| m.has_capability("sfx")).collect();
            if !sfx_models.is_empty() && !sfx_models.iter().any(|m| m.name == media.sfx.model) {
                media.sfx.model = sfx_models[0].name.clone();
            }
            let sel = sfx_models
                .iter()
                .find(|m| m.name == media.sfx.model)
                .copied();
            let has_loops = sel.map(|m| m.has_capability("loops")).unwrap_or(false);
            let has_variations = sel
                .map(|m| m.has_capability("audio-variations"))
                .unwrap_or(false);
            let max_secs = sel
                .and_then(|m| m.default_f64("max_seconds"))
                .unwrap_or(30.0) as f32;
            widgets::section_panel(ui, "MODEL", |ui| {
                param_grid(ui, "media_params_sfx_model", |ui| {
                    param_label(ui, "MODEL");
                    egui::ComboBox::from_id_salt("sfx_model")
                        .selected_text(media.sfx.model.clone())
                        .show_ui(ui, |ui| {
                            if sfx_models.is_empty() {
                                ui.label("No SFX model available");
                            }
                            for m in &sfx_models {
                                let mut label = m.name.clone();
                                if m.has_capability("loops") {
                                    label.push_str(" (loops)");
                                }
                                if ui.selectable_label(media.sfx.model == m.name, label).clicked() {
                                    media.sfx.model = m.name.clone();
                                    if let Some(st) = m.default_f64("steps") {
                                        media.sfx.steps = st as u32;
                                    }
                                    if let Some(cf) = m.default_f64("cfg") {
                                        media.sfx.cfg = cf as f32;
                                    }
                                }
                            }
                        })
                        .response
                        .on_hover_text("Engines and their limits come from the server; picking one applies its recommended steps and CFG.");
                    ui.end_row();
                });
                widgets::caption_row(
                    ui,
                    "Picking an engine applies its recommended steps and CFG.",
                );
            });
            ui.add_space(widgets::GAP_WIDGETS);
            widgets::section_panel(ui, "RENDERING", |ui| {
                param_grid(ui, "media_params_sfx_rendering", |ui| {
                    slider_row(
                        ui,
                        "DURATION",
                        &mut media.sfx.seconds,
                        1.0..=max_secs,
                        SliderOpts {
                            suffix: " s",
                            decimals: 1,
                            ..Default::default()
                        },
                        if has_loops {
                            "Clip length; the model's training window caps at ~47 s."
                        } else {
                            "Clip length. The model is trained on short clips (~10 s); long ones lose coherence."
                        },
                    );
                    slider_row(
                        ui,
                        "STEPS",
                        &mut media.sfx.steps,
                        1..=200,
                        SliderOpts::default(),
                        "Diffusion steps: 50-100 is the model's quality regime.",
                    );
                    if has_loops {
                        param_label(ui, "LOOP");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut media.sfx.loop_mode, "seamless");
                            if media.sfx.loop_mode {
                                widgets::drag_fixed(ui, egui::DragValue::new(&mut media.sfx.loop_bars).range(1..=32).suffix(" bars"), widgets::READOUT_WIDE_W);
                                widgets::drag_fixed(ui, egui::DragValue::new(&mut media.sfx.loop_bpm).range(40..=300).suffix(" bpm"), widgets::READOUT_WIDE_W);
                            }
                        })
                        .response
                        .on_hover_text("Bar-exact segment (bars x bpm) whose tail is crossfaded into the head: the WAV loops without a click.");
                        ui.end_row();
                    }
                });
                widgets::caption_row(
                    ui,
                    "50-100 steps is the quality regime; a loop is cut bar-exact.",
                );
            });
            ui.add_space(widgets::GAP_WIDGETS);
            widgets::section_panel(ui, "GUIDANCE", |ui| {
                param_grid(ui, "media_params_sfx_guidance", |ui| {
                    slider_row(ui, "CFG", &mut media.sfx.cfg, 0.5..=12.0, SliderOpts { decimals: 1, ..Default::default() },
                        "Prompt adherence: higher follows the description more literally but can distort; ~3 for EzAudio, ~7 for Stable Audio.");
                    if has_loops {
                        param_label(ui, "NEGATIVE");
                        ui.add(
                            egui::TextEdit::multiline(&mut media.sfx.negative_prompt)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .hint_text("distortion, low quality, muffled..."),
                        )
                        .on_hover_text(
                            "What the sound should avoid; steers the CFG's negative branch.",
                        );
                        ui.end_row();
                    }
                });
                widgets::caption_row(ui, "About 3 for EzAudio, about 7 for Stable Audio.");
            });
            if has_variations {
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "VARIATION", |ui| {
                    param_grid(ui, "media_params_sfx_variation", |ui| {
                        audio_picker_row(ui, media, "VARIATION OF", MediaAudioSlot::SfxInit,
                            "Optional source clip (WAV): the render becomes an audio-to-audio variation of it instead of starting from noise.");
                        if media.sfx.init_audio.is_some() {
                            slider_row(ui, "STRENGTH", &mut media.sfx.init_noise_level, 0.4..=100.0,
                                SliderOpts { logarithmic: true, decimals: 1, ..Default::default() },
                                "How far to drift from the source clip: ~1 keeps its structure, ~10+ reinterprets it freely.");
                        }
                    });
                    widgets::caption_row(ui, "Around 1 keeps the source's structure; past 10 it is reinterpreted freely.");
                });
            }
            ui.add_space(widgets::GAP_WIDGETS);
            widgets::section_panel(ui, "SEED", |ui| {
                param_grid(ui, "media_params_sfx_seed", |ui| {
                    seed_row(ui, media);
                });
                widgets::caption_row(
                    ui,
                    "Same seed and settings, same result. Empty is a fresh seed each run.",
                );
            });
        }
        MediaKind::Midi => {
            widgets::section_panel(ui, "SAMPLING", |ui| {
                param_grid(ui, "media_params_midi", |ui| {
                    slider_row(ui, "MAX TOKENS", &mut media.midi.max_tokens, 64..=8192, SliderOpts::default(),
                        "Length budget for the score: more tokens is more notes and bars. The model may end earlier.");
                    slider_row(
                        ui,
                        "TEMPERATURE",
                        &mut media.midi.temperature,
                        0.1..=2.0,
                        SliderOpts {
                            decimals: 2,
                            ..Default::default()
                        },
                        "Note-choice randomness: low is predictable, high is surprising.",
                    );
                    slider_row(
                        ui,
                        "TOP-P",
                        &mut media.midi.top_p,
                        0.1..=1.0,
                        SliderOpts {
                            decimals: 2,
                            ..Default::default()
                        },
                        "Keeps only the most likely notes at each step; lower is safer.",
                    );
                });
                widgets::caption_row(
                    ui,
                    "Tokens are the length budget; the model may stop earlier on its own.",
                );
            });
            ui.add_space(widgets::GAP_WIDGETS);
            widgets::section_panel(ui, "SEED", |ui| {
                param_grid(ui, "media_params_midi_seed", |ui| {
                    seed_row(ui, media);
                });
                widgets::caption_row(
                    ui,
                    "Same seed and settings, same result. Empty is a fresh seed each run.",
                );
            });
        }
        MediaKind::Video => {
            // The list comes from the server, so a checkpoint dropped in its video
            // directory appears here without a GUI change.
            let video_models: Vec<&str> = models
                .iter()
                .filter(|m| m.is_video_gen())
                .map(|m| m.name.as_str())
                .collect();
            if !video_models.is_empty() && !video_models.contains(&media.video.model.as_str()) {
                media.video.model = video_models[0].to_string();
            }
            ui.columns(2, |cols| {
                let ui = &mut cols[0];
                widgets::section_panel(ui, "MODEL", |ui| {
                    param_grid(ui, "media_params_video_model", |ui| {
                        param_label(ui, "MODEL");
                        egui::ComboBox::from_id_salt("media_video_model")
                            .selected_text(media.video.model.clone())
                            .show_ui(ui, |ui| {
                                for name in &video_models {
                                    ui.selectable_value(&mut media.video.model, (*name).to_string(), *name);
                                }
                            })
                            .response
                            .on_hover_text("The video checkpoint. A name containing 14 is the 14B, slower and better.");
                        ui.end_row();
                    });
                    widgets::caption_row(ui, "A fine-tune dropped in the server's video directory shows up here on its own.");
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "SHAPE AND SIZE", |ui| {
                    param_grid(ui, "media_params_video_shape", |ui| {
                        slider_row(ui, "DURATION", &mut media.video.seconds, 0.5..=300.0, SliderOpts { suffix: " s", decimals: 1, ..Default::default() },
                            "Length of each scene. The temporal VAE compresses time by four, so a \
                             duration becomes a frame count rounded up. Past the 5 s the model was \
                             trained on it is denoised over overlapping windows, so the cost grows \
                             with the duration rather than its square.");
                        size_row(ui, &mut media.video.width, &mut media.video.height, 128..=1280);
                    });
                    let f = crate::state::frames_for_seconds(media.video.seconds);
                    widgets::caption_row(ui, &format!(
                        "{f} frames ({:.2} s at 16 fps). A minute costs about twice what thirty seconds does.",
                        crate::state::seconds_for_frames(f)
                    ));
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "SAMPLING", |ui| {
                    param_grid(ui, "media_params_video_sampling", |ui| {
                        slider_row(ui, "STEPS", &mut media.video.steps, 1..=60, SliderOpts::default(),
                            "Denoising iterations per scene; time grows linearly. ~20 is balanced.");
                        slider_row(ui, "CFG", &mut media.video.cfg, 0.0..=15.0, SliderOpts { decimals: 1, ..Default::default() },
                            "Prompt adherence: higher is more literal but risks burn-out (all-white \
                             frames at high values on small sizes). 0 is the server's measured default.");
                        param_label(ui, "SAMPLER");
                        ui.horizontal(|ui| {
                            for smp in [VideoSampler::Auto, VideoSampler::UniPc, VideoSampler::Heun] {
                                if widgets::selector_pill(ui, smp.label(), media.video.sampler == smp).clicked() {
                                    media.video.sampler = smp;
                                }
                            }
                        })
                        .response
                        .on_hover_text("Integration method. Auto picks the measured best per resolution (UniPC at native scale, Heun below).");
                        ui.end_row();
                    });
                    widgets::caption_row(ui, "Auto picks the measured best sampler per resolution; change it only if a render shows artifacts.");
                });

                let ui = &mut cols[1];
                widgets::section_panel(ui, "START FRAME", |ui| {
                    param_grid(ui, "media_params_video_start", |ui| {
                        audio_picker_row(ui, media, "START FRAME", MediaAudioSlot::VideoStartImage,
                            "The picture the clip starts from, for a model whose name carries i2v. \
                             A text-to-video model ignores it; an image-to-video one refuses to render without it.");
                    });
                    widgets::caption_row(ui, "An i2v model continues a frame rather than inventing one; a t2v model ignores this.");
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "GUIDANCE", |ui| {
                    param_grid(ui, "media_params_video_guidance", |ui| {
                        param_label(ui, "NEGATIVE");
                        ui.add(
                            egui::TextEdit::multiline(&mut media.video.negative_prompt)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .hint_text("blurry, distorted, flickering..."),
                        )
                        .on_hover_text("What the motion should steer away from. Empty leaves the plain unconditional branch.");
                        ui.end_row();
                    });
                    widgets::caption_row(ui, "Empty leaves the unconditional branch the model was trained against.");
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "OUTPUT", |ui| {
                    param_grid(ui, "media_params_video_output", |ui| {
                        param_label(ui, "FORMAT");
                        ui.horizontal(|ui| {
                            for (fmt, label) in [(VideoFormat::Mp4, "MP4"), (VideoFormat::Gif, "GIF")] {
                                if widgets::selector_pill(ui, label, media.video.format == fmt).clicked() {
                                    media.video.format = fmt;
                                }
                            }
                        })
                        .response
                        .on_hover_text("MP4 is true colour and ~10x smaller; GIF is universal but 256 colours.");
                        ui.end_row();
                    });
                    widgets::caption_row(ui, "MP4 is true colour and ten times smaller; GIF is universal but 256 colours.");
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "SEED", |ui| {
                    param_grid(ui, "media_params_video_seed", |ui| {
                        seed_row(ui, media);
                    });
                    widgets::caption_row(ui, "Same seed and settings, same result. Empty is a fresh seed each run.");
                });
            });
        }
        MediaKind::Speech => {
            use crate::state::SpeechEngine;
            widgets::section_panel(ui, "ENGINE", |ui| {
                param_grid(ui, "media_params_speech", |ui| {
                    param_label(ui, "ENGINE");
                    egui::ComboBox::from_id_salt("media_speech_engine")
                        .selected_text(media.speech.engine.label())
                        .show_ui(ui, |ui| {
                            for e in SpeechEngine::ALL {
                                ui.selectable_value(&mut media.speech.engine, e, e.label());
                            }
                        })
                        .response
                        .on_hover_text("Parler: describe any voice freely (Style). Kyutai: natural EN/FR voices. Piper: fast fixed voices.");
                    ui.end_row();
                    if media.speech.engine == SpeechEngine::Parler {
                        param_label(ui, "STYLE");
                        ui.text_edit_singleline(&mut media.speech.voice_description)
                            .on_hover_text("How the text should sound, voice and delivery: an old man shouting angrily, a soft whispering woman. Overrides the preset voice.");
                        ui.end_row();
                    }
                    match media.speech.engine {
                        SpeechEngine::Parler => {
                            // One-shot fetch of the preset voices.
                            if media.speech.voices.is_empty() && !media.speech.voices_fetched {
                                media.speech.voices_fetched = true;
                                out.refresh_voices_clicked = true;
                            }
                            param_label(ui, "VOICE");
                            ui.horizontal(|ui| {
                                let current = if media.speech.voice.is_empty() { "(default)".to_string() } else { media.speech.voice.clone() };
                                egui::ComboBox::from_id_salt("media_voice_picker")
                                    .selected_text(current)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut media.speech.voice, String::new(), "(default)");
                                        for v in &media.speech.voices {
                                            ui.selectable_value(&mut media.speech.voice, v.clone(), v);
                                        }
                                    })
                                    .response
                                    .on_hover_text("Preset voice (OpenAI-style names). A Style overrides it when filled.");
                                if widgets::icon_button(ui, Icon::Refresh, "Fetch the voice list from the server").clicked() {
                                    out.refresh_voices_clicked = true;
                                }
                            });
                            ui.end_row();
                        }
                        SpeechEngine::Kyutai => {
                            param_label(ui, "VOICE");
                            ui.add(
                                egui::TextEdit::singleline(&mut media.speech.voice_name)
                                    .hint_text("default (e.g. alba-mackenna)")
                                    .desired_width(FIELD_W),
                            )
                            .on_hover_text(
                                "A kyutai/tts-voices name substring; empty is the server default.",
                            );
                            ui.end_row();
                        }
                        SpeechEngine::Piper => {
                            param_label(ui, "VOICE");
                            ui.add(
                                egui::TextEdit::singleline(&mut media.speech.voice_name)
                                    .hint_text("e.g. fr_FR-tom-medium")
                                    .desired_width(FIELD_W),
                            )
                            .on_hover_text(
                                "A Piper voice id installed under <hf_models_dir>/piper/<voice>/.",
                            );
                            ui.end_row();
                        }
                    }
                });
                widgets::caption_row(ui, "Parler takes a free description of the voice; Kyutai and Piper take a voice name.");
            });
        }
        MediaKind::ImageEdit => {
            // Every model the server declares edit-capable or img2img-capable.
            let editors: Vec<&crate::api::types::ModelInfo> = models
                .iter()
                .filter(|m| m.has_capability("edit") || m.has_capability("img2img"))
                .collect();
            if !editors.is_empty() && !editors.iter().any(|m| m.name == media.image_edit.model) {
                media.image_edit.model = editors[0].name.clone();
            }
            let sel_is_edit = models
                .iter()
                .find(|m| m.name == media.image_edit.model)
                .map(|m| m.has_capability("edit"))
                .unwrap_or(false);
            let family = model_family_of(models, &media.image_edit.model);
            let takes_loras = family_takes_loras(models, &media.image_edit.model);
            ui.columns(2, |cols| {
                let ui = &mut cols[0];
                widgets::section_panel(ui, "MODEL", |ui| {
                    param_grid(ui, "media_params_edit_model", |ui| {
                        param_label(ui, "MODEL");
                        egui::ComboBox::from_id_salt("media_edit_model")
                            .selected_text(media.image_edit.model.clone())
                            .show_ui(ui, |ui| {
                                if editors.is_empty() {
                                    ui.label("No edit-capable model available");
                                }
                                for m in &editors {
                                    let mode = if m.has_capability("edit") { "instruction edit" } else { "img2img" };
                                    let sel = media.image_edit.model == m.name;
                                    if ui.selectable_label(sel, format!("{} ({mode})", m.name)).clicked() {
                                        media.image_edit.model = m.name.clone();
                                        // Editors differ by an order of magnitude: a Kontext-class
                                        // model needs ~28 steps at guidance ~2.5, a distilled editor
                                        // is done in 4-8.
                                        if let Some(st) = m.default_f64("steps") {
                                            media.image_edit.steps = st as u32;
                                        }
                                        if let Some(g) = m.default_f64("cfg").or_else(|| m.default_f64("guidance")) {
                                            media.image_edit.guidance = g as f32;
                                        }
                                    }
                                }
                            })
                            .response
                            .on_hover_text(if sel_is_edit {
                                "Instruction editor: follows a textual instruction while preserving the rest of the image."
                            } else {
                                "img2img: re-imagines the source guided by the prompt; Strength sets how far it may drift."
                            });
                        ui.end_row();
                    });
                    widgets::caption_row(ui, "Picking a model applies its recommended steps and guidance; too few steps is why an instruction comes back barely applied.");
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "SOURCE", |ui| {
                    param_grid(ui, "media_params_edit_source", |ui| {
                        audio_picker_row(ui, media, "SOURCE IMAGE", MediaAudioSlot::EditImage, "Pick the image to edit (png/jpg/webp)");
                    });
                    widgets::caption_row(ui, "The picture the instruction applies to.");
                });
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "SAMPLING", |ui| {
                    param_grid(ui, "media_params_edit_sampling", |ui| {
                        slider_row(ui, "STRENGTH", &mut media.image_edit.strength, 0.0..=1.0, SliderOpts { decimals: 2, ..Default::default() },
                            "How far the edit may drift from the source: low keeps composition and details, 1 is a full re-generation.");
                        slider_row(ui, "STEPS", &mut media.image_edit.steps, 0..=100, SliderOpts::default(),
                            "Denoising iterations: more is finer detail, linearly slower. 0 is the model's recommended count.");
                        slider_row(ui, "GUIDANCE", &mut media.image_edit.guidance, 0.0..=30.0, SliderOpts { decimals: 2, ..Default::default() },
                            "Instruction adherence (CFG): higher applies the edit more forcefully but can distort; 0 is the model's default.");
                        slider_row(ui, "COUNT", &mut media.image_edit.n, 1..=4, SliderOpts::default(),
                            "Number of edit variations rendered in one run.");
                    });
                    widgets::caption_row(ui, "Strength 0 keeps the source, 1 re-generates it.");
                });

                let ui = &mut cols[1];
                widgets::section_panel(ui, "GUIDANCE", |ui| {
                    param_grid(ui, "media_params_edit_guidance", |ui| {
                        param_label(ui, "NEGATIVE");
                        ui.add(
                            egui::TextEdit::multiline(&mut media.image_edit.negative_prompt)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .hint_text("watermark, extra fingers, blurry..."),
                        )
                        .on_hover_text("What the edit should steer away from: the artefacts an instruction cannot name.");
                        ui.end_row();
                    });
                    widgets::caption_row(ui, "The negative prompt removes what an instruction cannot name.");
                });
                if takes_loras {
                    ui.add_space(widgets::GAP_WIDGETS);
                    widgets::section_panel(ui, "ADAPTERS", |ui| {
                        param_grid(ui, "media_params_edit_adapters", |ui| {
                            lora_rows(ui, &mut media.image_edit.loras, loras, &family);
                        });
                        widgets::caption_row(ui, "Only the adapters this model's architecture can take are offered.");
                    });
                }
                ui.add_space(widgets::GAP_WIDGETS);
                widgets::section_panel(ui, "SEED", |ui| {
                    param_grid(ui, "media_params_edit_seed", |ui| {
                        seed_row(ui, media);
                    });
                    widgets::caption_row(ui, "Same seed and settings, same result. Empty is a fresh seed each run.");
                });
            });
        }
        MediaKind::Transcribe => {
            widgets::section_panel(ui, "INPUT", |ui| {
                param_grid(ui, "media_params_transcribe_input", |ui| {
                    audio_picker_row(
                        ui,
                        media,
                        "AUDIO FILE",
                        MediaAudioSlot::TranscribeAudio,
                        "Pick the audio clip to transcribe (wav/mp3/flac/ogg/m4a)",
                    );
                });
                widgets::caption_row(ui, "Any common format; the server converts it.");
            });
            ui.add_space(widgets::GAP_WIDGETS);
            widgets::section_panel(ui, "RECOGNITION", |ui| {
                param_grid(ui, "media_params_transcribe", |ui| {
                    param_label(ui, "MODEL");
                    ui.text_edit_singleline(&mut media.transcribe.model)
                        .on_hover_text(
                            "ASR model id; empty is the server default (whisper, voxtral).",
                        );
                    ui.end_row();
                    // Detection is the default and not free of mistakes: a short clip, an
                    // accent, music under the voice.
                    param_label(ui, "LANGUAGE");
                    let cur = SPOKEN_LANGUAGES
                        .iter()
                        .find(|(code, _)| *code == media.transcribe.language)
                        .map(|(_, name)| *name)
                        .unwrap_or("Detect");
                    egui::ComboBox::from_id_salt("transcribe_language")
                        .selected_text(cur)
                        .show_ui(ui, |ui| {
                            for (code, name) in SPOKEN_LANGUAGES {
                                let mut sel = media.transcribe.language.clone();
                                if ui.selectable_value(&mut sel, code.to_string(), *name).clicked() {
                                    media.transcribe.language = code.to_string();
                                }
                            }
                        })
                        .response
                        .on_hover_text("Leave on Detect unless it gets it wrong; naming the language also stops it drifting mid-file.");
                    ui.end_row();
                    param_label(ui, "TRANSLATE");
                    ui.checkbox(&mut media.transcribe.translate, "to English")
                        .on_hover_text(
                            "Translate the transcription to English (Whisper translate task).",
                        );
                    ui.end_row();
                });
                widgets::caption_row(ui, "Detection can mishear a short clip or an accent; naming the language fixes it.");
            });
        }
        MediaKind::Separate => {
            widgets::section_panel(ui, "INPUT", |ui| {
                param_grid(ui, "media_params_separate_input", |ui| {
                    audio_picker_row(
                        ui,
                        media,
                        "TRACK",
                        MediaAudioSlot::SeparateAudio,
                        "Pick the song to split (wav/mp3/flac/ogg/m4a)",
                    );
                });
                widgets::caption_row(
                    ui,
                    "Any common format; it is converted to the model's 44.1 kHz stereo.",
                );
            });
            ui.add_space(widgets::GAP_WIDGETS);
            widgets::section_panel(ui, "STEMS", |ui| {
                param_grid(ui, "media_params_separate", |ui| {
                    param_label(ui, "STEMS");
                    egui::ComboBox::from_id_salt("media_separate_stems")
                        .selected_text(media.separate.stems.label())
                        .show_ui(ui, |ui| {
                            for st in [
                                crate::state::SeparateStems::Both,
                                crate::state::SeparateStems::Vocals,
                                crate::state::SeparateStems::Instrumental,
                            ] {
                                ui.selectable_value(&mut media.separate.stems, st, st.label());
                            }
                        })
                        .response
                        .on_hover_text(
                            "Which stems to return. Both adds back to the original exactly.",
                        );
                    ui.end_row();
                });
                widgets::caption_row(ui, "Both stems add back to the original exactly, so what was taken out can be heard.");
            });
        }
    }
}

/// The grid of a section: a label column of fixed width and the controls.
fn param_grid(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .min_col_width(widgets::PARAM_LABEL_W)
        .spacing([widgets::GAP_WIDGETS, widgets::GAP_LABEL])
        .show(ui, add);
}

/// A parameter's label: capitals, dim.
fn param_label(ui: &mut egui::Ui, caps: &str) {
    ui.label(text::label(caps));
}

/// How a slider row formats its value.
#[derive(Default, Clone, Copy)]
struct SliderOpts {
    suffix: &'static str,
    logarithmic: bool,
    decimals: usize,
}

/// One grid row: the label, a slider of fixed track width, the value in a
/// monospace cell of fixed width. The description is on hover.
fn slider_row<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    caps: &str,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    opts: SliderOpts,
    tip: &str,
) -> egui::Response {
    param_label(ui, caps);
    let response = ui
        .horizontal(|ui| {
            let slider = ui
                .scope(|ui| {
                    ui.spacing_mut().slider_width = widgets::SLIDER_TRACK_W;
                    ui.add(
                        egui::Slider::new(value, range.clone())
                            .show_value(false)
                            .logarithmic(opts.logarithmic),
                    )
                })
                .inner
                .on_hover_text(tip);
            ui.scope(|ui| {
                ui.style_mut().override_font_id = Some(egui::FontId::monospace(text::VALUE_PT));
                widgets::drag_fixed(
                    ui,
                    egui::DragValue::new(value)
                        .range(range)
                        .fixed_decimals(opts.decimals)
                        .suffix(opts.suffix),
                    widgets::READOUT_WIDE_W,
                )
            })
            .inner
            .on_hover_text(tip);
            slider
        })
        .inner;
    ui.end_row();
    response
}

/// How tall a chosen-file preview is. Small enough that a filled row does not push the
/// form around, large enough to recognise what is in it.
const THUMBNAIL_HEIGHT: f32 = 44.0;

/// The decoded preview for a chosen file, built once and kept.
///
/// Decoding runs on the frame path, so the result is cached against the slot AND the
/// byte length: replacing a slot's file changes the length and rebuilds the texture,
/// while re-rendering the same choice sixty times a second does not decode anything.
/// A file that will not decode simply has no preview - it is still a valid upload for
/// the server to reject with a real message.
/// Decode honouring the rotation the file DECLARES.
///
/// A phone stores the frame in sensor order and records how to turn it; a plain decode
/// returns that sensor order, so an upright photo previews lying on its side. The
/// server rotates before it renders, so an unrotated preview also disagrees with what
/// the request will actually do - and the preview exists to answer "is this the right
/// picture".
pub(crate) fn decode_oriented(bytes: &[u8]) -> Option<image::DynamicImage> {
    // `orientation` lives on the ImageDecoder TRAIT, which has to be in scope for the
    // call to resolve. Without this the file compiles everywhere it is not called and
    // fails only here - which is how a rotation fix shipped that had never once run.
    use image::ImageDecoder as _;
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    // A missing or unreadable tag is not an error: it means no rotation.
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).ok()?;
    img.apply_orientation(orientation);
    Some(img)
}

fn thumbnail_for(
    ctx: &egui::Context,
    media: &mut MediaState,
    slot: MediaAudioSlot,
) -> Option<egui::TextureHandle> {
    let len = slot_bytes(media, slot)?.len();
    let key = (slot.cache_key(), len);
    if let Some(t) = media.thumbnails.0.get(&key) {
        return Some(t.clone());
    }
    // Copied once, on a cache MISS only - the decode needs the bytes while the cache
    // needs a mutable borrow of the same state, and a miss happens once per pick.
    let bytes = slot_bytes(media, slot)?.clone();
    let img = decode_oriented(&bytes)?;
    // Downscale BEFORE uploading: the source may be several thousand pixels wide and
    // the preview is under fifty tall, so the full-size upload would be pure waste.
    let img = img.thumbnail(256, 256).to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let colour = egui::ColorImage::from_rgba_unmultiplied([w, h], img.as_raw());
    let tex = ctx.load_texture(
        format!("thumb-{}-{}", key.0, key.1),
        colour,
        egui::TextureOptions::LINEAR,
    );
    media.thumbnails.0.insert(key, tex.clone());
    Some(tex)
}

#[cfg(test)]
mod oversize_tests {
    use super::oversize_message;

    /// A file inside the cap passes without comment.
    #[test]
    fn a_small_file_is_accepted() {
        let dir = std::env::temp_dir().join(format!("atelier-size-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("small.png");
        std::fs::write(&p, vec![0u8; 1024]).unwrap();
        assert!(oversize_message(&p).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One over the cap is refused, and the message states BOTH what was sent and what
    /// the limit is - "too large" alone leaves the user guessing how much to shrink by.
    #[test]
    fn an_oversized_file_says_what_was_sent_and_what_is_allowed() {
        let dir = std::env::temp_dir().join(format!("atelier-size-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("huge.png");
        // SPARSE, not written: the cap is large enough that materialising it would put
        // half a gigabyte of zeroes on disk for one assertion. `set_len` gives metadata
        // the length `oversize_message` reads without allocating the blocks.
        let over = crate::modality::CHAT_ATTACHMENT_MAX_BYTES + 1;
        std::fs::File::create(&p).unwrap().set_len(over).unwrap();
        let msg = oversize_message(&p).expect("refused");
        assert!(msg.contains("huge.png"), "{msg}");
        // Derived from the constant, so raising the cap cannot leave this pinning a
        // figure the message no longer prints.
        let cap_mb = (crate::modality::CHAT_ATTACHMENT_MAX_BYTES / (1024 * 1024)).to_string();
        assert!(msg.contains(&cap_mb), "the limit is named: {msg}");
        assert!(msg.contains("MB"), "{msg}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that cannot be measured is NOT refused: the server still gets its say, and
    /// refusing here would block a readable file over an unreadable stat.
    #[test]
    fn an_unmeasurable_file_is_left_to_the_server() {
        assert!(oversize_message(std::path::Path::new("/definitely/not/here.png")).is_none());
    }
}

/// Languages offered for transcription. The empty code means "detect it", which is what
/// the server does when the field is absent.
///
/// Deliberately short: Whisper knows ninety-nine, and a ninety-nine-entry dropdown is
/// worse than none. These are the ones a user of this build actually speaks, and any
/// other can still be sent through the API.
const SPOKEN_LANGUAGES: &[(&str, &str)] = &[
    ("", "Detect"),
    ("fr", "Francais"),
    ("en", "English"),
    ("es", "Espanol"),
    ("de", "Deutsch"),
    ("it", "Italiano"),
    ("pt", "Portugues"),
    ("nl", "Nederlands"),
    ("ja", "Japanese"),
    ("zh", "Chinese"),
    ("ru", "Russian"),
    ("ar", "Arabic"),
];

/// Refuse a file the server will refuse, and say so BEFORE it is read.
///
/// The cap belongs to the server; this mirrors it so the rejection arrives while the
/// user is still looking at the file dialog rather than after a multi-megabyte upload.
/// It says what was sent and what the limit is, because "too large" without either
/// leaves the user guessing how much to shrink by.
fn oversize_message(path: &std::path::Path) -> Option<String> {
    let len = std::fs::metadata(path).ok()?.len();
    if len <= crate::modality::CHAT_ATTACHMENT_MAX_BYTES {
        return None;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("that file");
    Some(format!(
        "{name} is {:.1} MB; the limit is {} MB. Resize it or export at a lower quality.",
        len as f64 / (1024.0 * 1024.0),
        crate::modality::CHAT_ATTACHMENT_MAX_BYTES / (1024 * 1024),
    ))
}

/// The bytes currently in `slot`, if any.
fn slot_bytes(media: &MediaState, slot: MediaAudioSlot) -> Option<&Vec<u8>> {
    let entry = match slot {
        MediaAudioSlot::EditImage => &media.image_edit.source,
        MediaAudioSlot::TranscribeAudio => &media.transcribe.audio,
        MediaAudioSlot::SfxInit => &media.sfx.init_audio,
        MediaAudioSlot::VideoStartImage => &media.video.start_image,
        MediaAudioSlot::ImageControl => &media.image.control,
        MediaAudioSlot::SeparateAudio => &media.separate.audio,
    };
    entry.as_ref().map(|(_, b)| b)
}

/// A grid row that fills the MediaState slot named by `slot` with a chosen
/// file: Choose, Clear, the preview and the name.
///
/// The rfd dialog and the file read run on a worker thread: the synchronous
/// picker would deadlock the egui thread against the XDG portal on Linux, and
/// reading a multi-MB clip would stall the frame. The pick lands in
/// `pending_dialog` as `ChatDialogResult::MediaAudio`.
fn audio_picker_row(
    ui: &mut egui::Ui,
    media: &mut MediaState,
    caps: &str,
    slot: MediaAudioSlot,
    hover: &str,
) {
    param_label(ui, caps);
    ui.horizontal(|ui| {
        // One dialog at a time: a second would clobber the first's result.
        let busy = media
            .dialog_in_flight
            .load(std::sync::atomic::Ordering::Relaxed);
        let choose = if busy {
            "Choosing..."
        } else if slot.is_image() {
            "Choose image"
        } else {
            "Choose audio"
        };
        if ui
            .add_enabled(
                !busy,
                egui::Button::image_and_text(
                    Icon::Folder.image(ICON_PT_SMALL, theme::ink()),
                    text::note(choose),
                ),
            )
            .on_hover_text(hover)
            .clicked()
        {
            let pending = media.pending_dialog.clone();
            crate::dialog::spawn_dialog_worker(
                media.dialog_in_flight.clone(),
                ui.ctx().clone(),
                move |_| {
                    let dlg = if slot.is_image() {
                        rfd::FileDialog::new().add_filter("Image", crate::modality::CHAT_IMAGE_EXTS)
                    } else {
                        rfd::FileDialog::new().add_filter("Audio", crate::modality::CHAT_AUDIO_EXTS)
                    };
                    let Some(path) = dlg.pick_file() else {
                        return;
                    };
                    // The size is checked before the read, so the refusal arrives while
                    // the user still looks at the dialog rather than after an upload.
                    if let Some(msg) = oversize_message(&path) {
                        if let Ok(mut g) = pending.lock() {
                            *g = Some(crate::state::ChatDialogResult::MediaAudio {
                                slot,
                                name: String::new(),
                                bytes: Err(msg),
                            });
                        }
                        return;
                    }
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(if slot.is_image() { "image" } else { "audio" })
                        .to_string();
                    let bytes = std::fs::read(&path).map_err(|e| format!("Read audio: {e}"));
                    if let Ok(mut g) = pending.lock() {
                        *g = Some(ChatDialogResult::MediaAudio { slot, name, bytes });
                    }
                },
            );
        }
        // A slot picked by mistake can be emptied, not only replaced.
        let chosen = slot_bytes(media, slot).is_some();
        if chosen
            && ui
                .add_enabled(!busy, egui::Button::new(text::note("Clear")).frame(false))
                .on_hover_text("Remove the chosen file from this slot")
                .clicked()
        {
            match slot {
                MediaAudioSlot::EditImage => media.image_edit.source = None,
                MediaAudioSlot::TranscribeAudio => media.transcribe.audio = None,
                MediaAudioSlot::SfxInit => media.sfx.init_audio = None,
                MediaAudioSlot::VideoStartImage => media.video.start_image = None,
                MediaAudioSlot::ImageControl => media.image.control = None,
                MediaAudioSlot::SeparateAudio => media.separate.audio = None,
            }
        }
        // Built before `current` borrows the slot: the cache lives in the same state.
        let thumb = if slot.is_image() {
            thumbnail_for(ui.ctx(), media, slot)
        } else {
            None
        };
        let current = match slot {
            MediaAudioSlot::EditImage => &media.image_edit.source,
            MediaAudioSlot::TranscribeAudio => &media.transcribe.audio,
            MediaAudioSlot::SfxInit => &media.sfx.init_audio,
            MediaAudioSlot::VideoStartImage => &media.video.start_image,
            MediaAudioSlot::ImageControl => &media.image.control,
            MediaAudioSlot::SeparateAudio => &media.separate.audio,
        };
        match current {
            Some((name, bytes)) => {
                // The picture itself, so the user can tell it is the right one.
                if let Some(tex) = &thumb {
                    ui.add(
                        egui::Image::new(tex)
                            .max_height(THUMBNAIL_HEIGHT)
                            .corner_radius(theme::RADIUS),
                    )
                    .on_hover_text(name.clone());
                }
                ui.label(text::note(&format!("{name} ({} KB)", bytes.len() / 1024)));
            }
            None => {
                ui.label(text::note("no file chosen"));
            }
        }
    });
    ui.end_row();
}

/// The standard latent-diffusion shapes, all within a few percent of one megapixel.
///
/// Models are trained on buckets like these, so an arbitrary ratio costs quality as well
/// as time. The free width x height row stays below for anything else - this is the
/// shortcut, not a restriction.
const IMAGE_SHAPES: [(&str, u32, u32); 7] = [
    ("Square 1:1", 1024, 1024),
    ("Portrait 4:5", 896, 1152),
    ("Portrait 2:3", 832, 1216),
    ("Portrait 9:16", 768, 1344),
    ("Landscape 5:4", 1152, 896),
    ("Landscape 3:2", 1216, 832),
    ("Landscape 16:9", 1344, 768),
];

/// Pick a shape by name instead of typing two numbers.
fn shape_row(ui: &mut egui::Ui, width: &mut u32, height: &mut u32) {
    let current = IMAGE_SHAPES
        .iter()
        .find(|(_, w, h)| *w == *width && *h == *height)
        .map(|(n, _, _)| *n)
        .unwrap_or("Custom");
    param_label(ui, "SHAPE");
    egui::ComboBox::new("image_shape", "")
        .selected_text(current)
        .show_ui(ui, |ui| {
            for (name, w, h) in IMAGE_SHAPES {
                if ui
                    .selectable_label(*width == w && *height == h, name)
                    .clicked()
                {
                    *width = w;
                    *height = h;
                }
            }
        })
        .response
        .on_hover_text(
            "Aspect ratio, at the sizes these models are trained on. Anything else still \
             works, set the numbers below, but an off-bucket shape usually costs some \
             composition quality.",
        );
    ui.end_row();
}

/// How the finished image comes back.
fn file_format_row(ui: &mut egui::Ui, fmt: &mut crate::state::ImageFileFormat) {
    use crate::state::ImageFileFormat as F;
    param_label(ui, "FILE FORMAT");
    egui::ComboBox::new("image_file_format", "")
        .selected_text(fmt.label())
        .show_ui(ui, |ui| {
            for opt in [F::Png, F::Jpeg, F::WebP] {
                if ui.selectable_label(*fmt == opt, opt.label()).clicked() {
                    *fmt = opt;
                }
            }
        })
        .response
        .on_hover_text(
            "PNG keeps every pixel exactly and is what the server returns by default; JPEG \
             and WebP are a fraction of the size and lose a little detail. The server \
             transcodes, so the render itself is identical either way.",
        );
    ui.end_row();
}

/// The paired width x height row; Image and Video share it, only the range differs.
fn size_row(
    ui: &mut egui::Ui,
    width: &mut u32,
    height: &mut u32,
    range: std::ops::RangeInclusive<u32>,
) {
    param_label(ui, "SIZE");
    ui.horizontal(|ui| {
        ui.scope(|ui| {
            ui.style_mut().override_font_id = Some(egui::FontId::monospace(text::VALUE_PT));
            widgets::drag_fixed(
                ui,
                egui::DragValue::new(width)
                    .range(range.clone())
                    .speed(SIZE_STEP),
                widgets::READOUT_W,
            );
            ui.label(text::label("x"));
            widgets::drag_fixed(
                ui,
                egui::DragValue::new(height).range(range).speed(SIZE_STEP),
                widgets::READOUT_W,
            );
        });
    })
    .response
    .on_hover_text("Resolution in pixels (multiples of 16): more detail but slower and more VRAM.");
    ui.end_row();
}

/// Pixels a size drag moves per point.
const SIZE_STEP: f64 = 16.0;

/// Whether the model's family applies LoRA adapters at all.
///
/// SDXL and Flux do; Qwen-Image and Z-Image have no adapter path in their pipelines, so
/// a request carrying one there is accepted and ignored. Rather than let the user set a
/// value that does nothing, the picker is absent - and this reads the family the SERVER
/// reported, so a family becomes visible here by being served, not by being added to a
/// list in the GUI.
fn model_family_of(models: &[crate::api::types::ModelInfo], name: &str) -> String {
    models
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.family.clone())
        .unwrap_or_default()
}

fn family_takes_loras(models: &[crate::api::types::ModelInfo], name: &str) -> bool {
    models
        .iter()
        .find(|m| m.name == name)
        .is_some_and(|m| matches!(m.family.as_str(), "sdxl" | "flux"))
}

/// Adapter picker: one checkbox per adapter the server advertises, with a strength
/// slider for the ones that are on.
///
/// The list is the server's, so a name that cannot resolve is not offerable; the client
/// never types a path. Strength runs past 1.0 because over-driving an adapter is a real
/// technique, and below 0 because subtracting a style is one too.

/// Regional prompts: one row per area, each with what belongs there and how
/// hard. The base prompt still applies everywhere; a region adds to it over
/// its rectangle.
fn region_rows(ui: &mut egui::Ui, regions: &mut Vec<crate::state::ImageRegion>) {
    use crate::state::{ImageRegion, RegionArea};
    param_label(ui, "REGIONS");
    ui.vertical(|ui| {
        let mut remove: Option<usize> = None;
        for (i, r) in regions.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("region-area", i))
                    .selected_text(r.area.label())
                    .width(REGION_AREA_W)
                    .show_ui(ui, |ui| {
                        for a in RegionArea::ALL {
                            ui.selectable_value(&mut r.area, a, a.label());
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut r.prompt)
                        .hint_text("what goes here")
                        .desired_width(REGION_PROMPT_W),
                );
                widgets::drag_fixed(
                    ui,
                    egui::DragValue::new(&mut r.strength)
                        .speed(0.05)
                        .range(0.0..=8.0)
                        .fixed_decimals(2),
                    widgets::READOUT_W,
                )
                .on_hover_text("How hard this region's prompt outweighs the base one there.");
                if widgets::close_button(ui, REGION_CLOSE_PX)
                    .on_hover_text("Remove this region")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            regions.remove(i);
        }
        if ui
            .add(egui::Button::new(text::note("Add a region")))
            .clicked()
        {
            // A second region defaults to the opposite half.
            let area = if regions.len() == 1 {
                RegionArea::Right
            } else {
                RegionArea::Left
            };
            regions.push(ImageRegion {
                area,
                ..Default::default()
            });
        }
    });
    ui.end_row();
}

/// Widths of a region row's area picker and prompt, and the side of its close mark.
const REGION_AREA_W: f32 = 110.0;
const REGION_PROMPT_W: f32 = 180.0;
const REGION_CLOSE_PX: f32 = 16.0;

/// The adapter picker, showing only what can actually be applied to `model_family`.
///
/// An adapter trained for another architecture matches NOTHING and fails the render
/// outright. Offering every adapter whatever model is selected is how a user gets a run
/// of failures whose only clue is a message about tensor names - so the ones that
/// cannot apply are not offered, and the count of them is stated rather than hidden.
fn lora_rows(
    ui: &mut egui::Ui,
    selected: &mut Vec<(String, f32)>,
    available: &[(String, Option<String>)],
    model_family: &str,
) {
    // Unknown layout = offered: not recognising an adapter is not evidence against it.
    let usable: Vec<&String> = available
        .iter()
        .filter(|(_, fam)| fam.as_deref().is_none_or(|f| f == model_family))
        .map(|(n, _)| n)
        .collect();
    let hidden = available.len() - usable.len();
    // Anything selected that this model cannot take would fail the render silently at
    // send time; drop it as the model changes.
    selected.retain(|(n, _)| usable.iter().any(|u| *u == n));
    let available: Vec<String> = usable.into_iter().cloned().collect();
    let available = &available[..];
    lora_rows_inner(ui, selected, available, hidden)
}

fn lora_rows_inner(
    ui: &mut egui::Ui,
    selected: &mut Vec<(String, f32)>,
    available: &[String],
    hidden: usize,
) {
    param_label(ui, "ADAPTERS");
    if available.is_empty() {
        ui.label(text::note(&if hidden > 0 {
            format!("none for this model ({hidden} for other architectures)")
        } else {
            "none on this server".to_string()
        }))
        .on_hover_text(
            "LoRA adapters let a model render a style, character or concept it was not \
             trained on. Drop .safetensors files in the server's lora directory and they \
             appear here.",
        );
        ui.end_row();
        return;
    }
    ui.vertical(|ui| {
        for name in available {
            let mut on = selected.iter().any(|(n, _)| n == name);
            if ui.checkbox(&mut on, name).changed() {
                if on {
                    selected.push((name.clone(), 1.0));
                } else {
                    selected.retain(|(n, _)| n != name);
                }
            }
            if let Some(entry) = selected.iter_mut().find(|(n, _)| n == name) {
                ui.horizontal(|ui| {
                    ui.label(text::label("STRENGTH"));
                    widgets::drag_fixed(
                        ui,
                        egui::DragValue::new(&mut entry.1).speed(0.05).range(-1.0..=2.0).fixed_decimals(2),
                        widgets::READOUT_W,
                    )
                    .on_hover_text("1.0 is as trained; 0 disables one without removing it; past 1 over-drives it.");
                });
            }
        }
    })
    .response
    .on_hover_text("LoRA adapters apply a style, character or concept on top of the model. Several stack.");
    ui.end_row();
}

/// Solver and sigma-curve pickers. Empty string is the model's own default,
/// which is what almost every render should use.
fn solver_rows(ui: &mut egui::Ui, sampler: &mut String, scheduler: &mut String) {
    const SAMPLERS: &[(&str, &str)] = &[
        ("", "Default"),
        ("euler", "Euler"),
        ("dpmpp_2m", "DPM++ 2M"),
    ];
    const SCHEDULERS: &[(&str, &str)] = &[
        ("", "Default"),
        ("normal", "Normal"),
        ("karras", "Karras"),
        ("exponential", "Exponential"),
    ];
    let pick = |ui: &mut egui::Ui, id: &str, cur: &mut String, opts: &[(&str, &str)], tip: &str| {
        let label = opts
            .iter()
            .find(|(v, _)| v == cur)
            .map(|(_, l)| *l)
            .unwrap_or("Default");
        egui::ComboBox::from_id_salt(id)
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (v, l) in opts {
                    ui.selectable_value(cur, v.to_string(), *l);
                }
            })
            .response
            .on_hover_text(tip);
    };
    param_label(ui, "SAMPLER");
    pick(ui, "media_image_sampler", sampler, SAMPLERS,
        "How the run moves between two noise levels. DPM++ 2M reaches the same quality in fewer steps; Euler is the plain first-order step.");
    ui.end_row();
    param_label(ui, "SCHEDULE");
    pick(ui, "media_image_scheduler", scheduler, SCHEDULERS,
        "Which noise levels the run visits. Karras spends more of the budget at low noise, where detail is decided.");
    ui.end_row();
}

/// The seed: free text, empty means random.
fn seed_row(ui: &mut egui::Ui, media: &mut MediaState) {
    param_label(ui, "SEED");
    ui.scope(|ui| {
        ui.style_mut().override_font_id = Some(egui::FontId::monospace(text::VALUE_PT));
        ui.add(
            egui::TextEdit::singleline(&mut media.seed)
                .hint_text("random")
                .desired_width(SEED_W),
        )
    })
    .inner
    .on_hover_text(
        "Same seed and settings, the exact same result. Empty is a fresh random seed each run.",
    );
    ui.end_row();
}

/// Width of the seed field.
const SEED_W: f32 = 140.0;

/// The status line: a lamp, the phase and the count as text, a meter. The
/// phase stays beside the count, because a load that counts each file
/// separately otherwise reads as a bar restarting for no reason.
fn render_status(ui: &mut egui::Ui, media: &MediaState) {
    if !media.is_generating && media.status.is_empty() {
        return;
    }

    if let Some((step, total)) = media.progress {
        let frac = step as f32 / total.max(1) as f32;
        let elapsed = media
            .started_at
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        let eta = if step > 0 && frac > 0.0 {
            let per = elapsed / step as f32;
            let remaining = per * (total.saturating_sub(step)) as f32;
            format!(" ~{remaining:.0}s left")
        } else {
            String::new()
        };
        let what = if media.status.is_empty() {
            format!("Step {step}/{total}")
        } else {
            media.status.clone()
        };
        ui.horizontal(|ui| {
            widgets::lamp_inline(ui, true, theme::accent());
            ui.label(text::mono(&format!("{what}  {elapsed:.0}s{eta}")));
        });
        ui.add_space(widgets::GAP_LABEL);
        surface::meter(ui, frac, widgets::METER_SIZE, theme::accent());
    } else if media.is_generating {
        let elapsed = media
            .started_at
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        ui.horizontal(|ui| {
            widgets::lamp_inline(ui, true, theme::accent());
            let label = if media.status.is_empty() {
                format!("Generating  {elapsed:.0}s")
            } else {
                format!("{}  {elapsed:.0}s", media.status)
            };
            ui.label(text::mono(&label));
        });
        ui.add_space(widgets::GAP_LABEL);
        // A kind without step events gets a sweep driven by the clock, so every
        // generation shows a moving meter.
        let sweep = (elapsed / SWEEP_PERIOD_S).fract();
        surface::meter(ui, sweep, widgets::METER_SIZE, theme::accent_dim());
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(SWEEP_REPAINT_MS));
    } else if !media.status.is_empty() {
        ui.horizontal(|ui| {
            widgets::lamp_inline(ui, true, theme::success());
            ui.label(text::note(&media.status));
        });
    }
}

/// A button drawn ON TOP of an image, taking no space in the layout.
///
/// `Ui::put` looks like the natural call and is the wrong one here: it ALLOCATES its
/// rect, which grows the surrounding `horizontal_wrapped`'s extent and moves where the
/// next thumbnail wraps. Since these controls only exist while the pointer is over an
/// image, the grid re-flowed under the cursor on every hover.
///
/// `Ui::interact` registers a click region without allocating, so the visuals are
/// painted by hand. The id is per-image and per-role, because two thumbnails would
/// otherwise share one interaction id and report each other's clicks.
fn overlay_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    index: usize,
    role: &str,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let id = ui.id().with(("media_overlay", role, index));
    let resp = ui.interact(rect, id, egui::Sense::click());
    let hovered = enabled && resp.hovered();
    let fill = if hovered {
        theme::accent()
    } else {
        OVERLAY_BUTTON_SCRIM
    };
    ui.painter().rect_filled(rect, theme::RADIUS, fill);
    let text_col = if hovered {
        theme::on_accent()
    } else if enabled {
        egui::Color32::WHITE
    } else {
        theme::ink_dim()
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(text::VALUE_PT),
        text_col,
    );
    resp
}

/// Render the result payloads: images inline, audio Play/Save, and
/// non-renderable files (MIDI, video) as Save + Open buttons.
fn render_results(
    ui: &mut egui::Ui,
    media: &mut MediaState,
    image_textures: &mut HashMap<String, egui::TextureHandle>,
    player: &mut crate::audio_playback::AudioPlayer,
    video: &mut crate::video_engine::VideoPlayback,
) {
    let has_results = !media.result_images.is_empty()
        || !media.result_audios.is_empty()
        || !media.result_files.is_empty()
        || media.result_text.is_some();
    if !has_results {
        // No CURRENT result, but earlier ones may still be worth reaching.
        render_history(ui, media, image_textures);
        return;
    }

    ui.add_space(widgets::GAP_WIDGETS);
    widgets::section_panel(ui, "RESULT", |ui| {
        // Text (Transcribe).
        if let Some(result) = &media.result_text {
            let mut shown = result.clone();
            widgets::well(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut shown)
                        .frame(egui::Frame::NONE)
                        .desired_rows(TRANSCRIPT_ROWS)
                        .desired_width(f32::INFINITY)
                        .interactive(true),
                );
            });
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(text::note("Copy text")).frame(false))
                    .clicked()
                {
                    ui.ctx().copy_text(result.clone());
                }
            });
            ui.add_space(widgets::GAP_LABEL);
        }

        // ── Images (inline) ──────────────────────────────────────────────
        // Results are iterated by reference throughout: these vecs hold
        // multi-MB base64 payloads, and this runs every repaint. Cloning them per
        // frame to appease the borrow checker burns allocator bandwidth sixty times a
        // second; only the clicked item's bytes are cloned, at click time.
        if !media.result_images.is_empty() {
            // Collect the textures first so the viewer can be opened on the WHOLE set
            // (arrow keys then browse the results) and so the hover controls below can
            // sit on each image instead of in a detached row.
            let mut texes: Vec<egui::TextureHandle> = Vec::with_capacity(media.result_images.len());
            for (i, b64) in media.result_images.iter().enumerate() {
                let key = crate::texture::image_cache_key("media", b64, i);
                let tex = image_textures
                    .entry(key.clone())
                    .or_insert_with(|| crate::texture::load_base64_texture(ui, b64, &key));
                texes.push(tex.clone());
            }
            // Which image the hover controls acted on, resolved after the borrow ends.
            let mut edit_idx: Option<usize> = None;
            let mut save_idx: Option<usize> = None;
            widgets::screen_well(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, tex_handle) in texes.iter().enumerate() {
                        let tex_size = tex_handle.size_vec2();
                        let scale = (RESULT_MAX_PX / tex_size.x.max(tex_size.y)).min(1.0);
                        let resp = crate::image_viewer::image_response(
                            ui,
                            tex_handle,
                            tex_size * scale,
                            "Click to view fullscreen (then use the arrow keys to browse)",
                        );
                        // Controls ON the image, shown while the pointer is over it. The old
                        // row of "Edit #2 / Save #2" buttons sat below ALL the thumbnails, so
                        // hitting the wrong image was a matter of counting - the button never
                        // pointed at anything the eye could check.
                        //
                        // Hover is tested with rect_contains_pointer, not resp.hovered():
                        // once the pointer is over one of these buttons the BUTTON owns the
                        // hover, and the controls would flicker out from under the cursor.
                        let rect = resp.rect;
                        // Capped at a third of the image so the bar can never swallow the
                        // click that opens the viewer.
                        let bar_h = OVERLAY_BAR_H.min(rect.height() / 3.0);
                        let bar = egui::Rect::from_min_max(
                            egui::pos2(
                                rect.left() + OVERLAY_PAD,
                                rect.bottom() - OVERLAY_PAD - bar_h,
                            ),
                            egui::pos2(rect.right() - OVERLAY_PAD, rect.bottom() - OVERLAY_PAD),
                        );
                        // A click on the image opens the viewer - EXCEPT over the control bar,
                        // where it belongs to Edit/Save. Decided here rather than left to widget
                        // ordering, so pressing Edit cannot also throw the fullscreen overlay up.
                        let on_bar = ui.rect_contains_pointer(bar);
                        if resp.clicked() && !on_bar {
                            crate::image_viewer::request_open_set(ui.ctx(), &texes, i);
                        }
                        if ui.rect_contains_pointer(rect) {
                            ui.painter().rect_filled(
                                bar.expand2(egui::vec2(0.0, OVERLAY_GAP / 2.0)),
                                theme::RADIUS,
                                OVERLAY_SCRIM,
                            );
                            let half = (bar.width() - OVERLAY_GAP) * 0.5;
                            let left =
                                egui::Rect::from_min_size(bar.min, egui::vec2(half, bar.height()));
                            let right = egui::Rect::from_min_size(
                                egui::pos2(bar.min.x + half + OVERLAY_GAP, bar.min.y),
                                egui::vec2(half, bar.height()),
                            );
                            let saving = media
                                .dialog_in_flight
                                .load(std::sync::atomic::Ordering::Relaxed);
                            if overlay_button(ui, left, i, "edit", "Edit", true)
                                .on_hover_text("Send THIS image to the Image Edit kind")
                                .clicked()
                            {
                                edit_idx = Some(i);
                            }
                            // Mirrors dialog::save_button: a second dialog cannot be opened
                            // while one is in flight, so say so instead of dropping the click.
                            if overlay_button(ui, right, i, "save", "Save", !saving)
                                .on_hover_text(if saving {
                                    "A save dialog is already open"
                                } else {
                                    "Save THIS image"
                                })
                                .clicked()
                                && !saving
                            {
                                save_idx = Some(i);
                            }
                        }
                    }
                });
            });
            ui.add_space(widgets::GAP_LABEL);
            // Act on whichever image the hover controls named. Resolved here, after the
            // borrow of result_images inside the loop has ended.
            if let Some(i) = edit_idx {
                if let Some(b64) = media.result_images.get(i) {
                    let bytes = decode_b64(b64);
                    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                    media.image_edit.source = Some((format!("result_{ts}.png"), bytes));
                    media.kind = MediaKind::ImageEdit;
                }
            }
            if let Some(i) = save_idx {
                if let Some(b64) = media.result_images.get(i) {
                    let bytes = decode_b64(b64);
                    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                    crate::dialog::spawn_save(
                        ui.ctx().clone(),
                        media.pending_dialog.clone(),
                        media.dialog_in_flight.clone(),
                        crate::modality::output_filename("image", &ts.to_string(), i, "png"),
                        &["png"],
                        "PNG image",
                        bytes,
                    );
                }
            }
        }

        // ── Audio (Play + Save) ──────────────────────────────────────────
        // Indexed loop (not `.clone().iter()`): the loop body also writes
        // `media.error`, so a plain `.iter()` borrow would conflict —
        // indexing keeps each `&media.result_audios[i]` borrow short-lived
        // instead of cloning the multi-MB base64 WAVs every frame.
        if !media.result_audios.is_empty() {
            ui.add_space(widgets::GAP_LABEL);
            let n_audios = media.result_audios.len();
            for i in 0..n_audios {
                ui.horizontal(|ui| {
                    Icon::Music.show(ui, ICON_PT, theme::ink_dim());
                    ui.label(text::note(&if n_audios == 1 {
                        "Audio".to_string()
                    } else {
                        format!("Audio #{}", i + 1)
                    }));
                    // Transport: play/pause toggle + stop + seek slider, one row per
                    // result; only the row that owns the current track shows the
                    // position controls.
                    let row_id = format!("media_audio_{i}");
                    let is_current = player.playing_id().as_deref() == Some(row_id.as_str());
                    let playing = is_current && !player.is_paused();
                    let (play_icon, play_tip) = if playing {
                        (Icon::Pause, "Pause")
                    } else {
                        (Icon::Play, "Play")
                    };
                    if widgets::icon_button(ui, play_icon, play_tip).clicked() {
                        if is_current {
                            player.toggle_pause();
                        } else if let Err(e) = player.play(&row_id, &media.result_audios[i]) {
                            media.error = Some(format!("Playback failed: {e}"));
                        }
                    }
                    if is_current {
                        if widgets::icon_button(ui, Icon::Stop, "Stop").clicked() {
                            player.stop();
                        }
                        let dur = player.duration().max(0.01);
                        let mut pos = player.position();
                        let resp = ui
                            .scope(|ui| {
                                ui.spacing_mut().slider_width = widgets::SLIDER_TRACK_W;
                                ui.add(egui::Slider::new(&mut pos, 0.0..=dur).show_value(false))
                            })
                            .inner;
                        if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                            player.seek(pos);
                        }
                        widgets::readout(
                            ui,
                            widgets::READOUT_WIDE_W,
                            &format!(
                                "{}:{:02} / {}:{:02}",
                                (pos as u32) / 60,
                                (pos as u32) % 60,
                                (dur as u32) / 60,
                                (dur as u32) % 60
                            ),
                        );
                        // The position moves while playing.
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(
                                AUDIO_REPAINT_MS,
                            ));
                    }
                    if crate::dialog::save_button(ui, &media.dialog_in_flight, "Save").clicked() {
                        let bytes = decode_b64(&media.result_audios[i]);
                        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                        let default_name =
                            crate::modality::output_filename("audio", &ts.to_string(), i, "wav");
                        crate::dialog::spawn_save(
                            ui.ctx().clone(),
                            media.pending_dialog.clone(),
                            media.dialog_in_flight.clone(),
                            default_name,
                            &["wav"],
                            "WAV audio",
                            bytes,
                        );
                    }
                });
            }
        }

        // ── Files (MIDI / video — Save + Open) ───────────────────────────
        // Same indexed pattern as audio: the Open branch writes
        // `media.error`, so borrow each (name, bytes) briefly per use
        // rather than cloning every blob every frame. Save still clones —
        // but only the one clicked file's bytes, at click time.
        if !media.result_files.is_empty() {
            ui.add_space(widgets::GAP_LABEL);
            if render_video_player(ui, video) {
                media.video_viewer.open = true;
            }
            for i in 0..media.result_files.len() {
                ui.horizontal(|ui| {
                    let (name, bytes) = &media.result_files[i];
                    Icon::Film.show(ui, ICON_PT, theme::ink_dim());
                    ui.label(text::note(&format!(
                        "{name} ({})",
                        crate::api::types::format_size(bytes.len() as u64)
                    )));
                    if crate::dialog::save_button(ui, &media.dialog_in_flight, "Save").clicked() {
                        let ext = std::path::Path::new(name)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("bin")
                            .to_string();
                        let ext_static: &[&str] = match ext.as_str() {
                            "mid" => &["mid"],
                            "mp4" => &["mp4"],
                            "gif" => &["gif"],
                            _ => &["bin"],
                        };
                        crate::dialog::spawn_save(
                            ui.ctx().clone(),
                            media.pending_dialog.clone(),
                            media.dialog_in_flight.clone(),
                            name.clone(),
                            ext_static,
                            "File",
                            bytes.clone(),
                        );
                    }
                    // Playing HERE is the point of the in-app decoder; "Open" stays for
                    // handing the file to something else.
                    if name.to_lowercase().ends_with(".mp4")
                        && widgets::icon_button(ui, Icon::Play, "Decode and play in the app")
                            .clicked()
                    {
                        video.open(i, bytes.clone());
                    }
                    if ui
                        .add(egui::Button::new(text::note("Open")).frame(false))
                        .on_hover_text("Write to a temp file and open in the system default app")
                        .clicked()
                    {
                        if let Err(e) = open_with_system(name, bytes) {
                            media.error = Some(format!("Open failed: {e}"));
                        }
                    }
                });
            }
        }
    });

    // Earlier generations, kept so a new run never destroys what came before.
    render_history(ui, media, image_textures);
}

/// Rows of the transcript box.
const TRANSCRIPT_ROWS: usize = 6;

/// The in-app video player: current frame plus a transport.
///
/// Draws nothing until a clip is opened, so the panel does not reserve space for a
/// feature the current result may not have.
/// Returns true when the viewer was asked for, which the caller acts on - passing the
/// viewer in would borrow `media` twice at the call site for no gain.
fn render_video_player(ui: &mut egui::Ui, video: &mut crate::video_engine::VideoPlayback) -> bool {
    if video.decoding.is_some() {
        ui.horizontal(|ui| {
            widgets::lamp_inline(ui, true, theme::accent());
            ui.label(text::label("DECODING"));
        });
        ui.ctx().request_repaint();
        return false;
    }
    if let Some(err) = &video.error {
        ui.label(text::note(&format!("Video: {err}")).color(theme::error()));
        return false;
    }
    let Some(tex) = video.texture.clone() else {
        return false;
    };

    let (mut seek_to, mut toggle, mut close) = (None, false, false);
    let mut open_viewer = false;

    let size = tex.size_vec2();
    let scale = (VIDEO_INLINE_MAX_PX / size.x.max(size.y)).min(1.0);
    // Clicking the picture opens the fullscreen viewer, the same gesture a rendered
    // image answers to.
    widgets::screen_well(ui, |ui| {
        if crate::image_viewer::image_response(
            ui,
            &tex,
            size * scale,
            "Click to watch it fullscreen (zoom, pan, step frame by frame)",
        )
        .clicked()
        {
            open_viewer = true;
        }
    });
    // A frame the decoder refused leaves the PREVIOUS one on screen while the counter
    // walks on: the clip looks frozen and nothing says why. Name it.
    if let ViewerNotice::Over(notice) | ViewerNotice::Instead(notice) =
        viewer_notice(true, false, None, video.stale_frame(), video.shown_frame())
    {
        ui.label(text::note(&notice).color(theme::warning()));
    }
    let (idx, total, playing, fps, looping, samples, dur) = match &video.player {
        Some(p) => (
            p.index(),
            p.frame_count(),
            p.is_playing(),
            p.fps(),
            p.looping,
            p.samples(),
            p.duration_s(),
        ),
        None => return false,
    };
    // A clip that decoded SHORT is otherwise invisible - it just plays briefly and the
    // user assumes that is the render. Say it instead.
    if total < samples {
        ui.label(
            text::note(&format!(
                "Only {total} of {samples} frames decoded: the file uses features this \
                 decoder does not support"
            ))
            .color(theme::warning()),
        );
    }
    ui.horizontal(|ui| {
        let (icon, tip) = if playing {
            (Icon::Pause, "Pause")
        } else {
            (Icon::Play, "Play")
        };
        if widgets::icon_button(ui, icon, tip).clicked() {
            toggle = true;
        }
        let mut pos = idx;
        // Seeking is instant because the clip is decoded up front.
        let seek = ui
            .scope(|ui| {
                ui.spacing_mut().slider_width = widgets::SLIDER_TRACK_W;
                ui.add(egui::Slider::new(&mut pos, 0..=total.saturating_sub(1)).show_value(false))
            })
            .inner;
        if seek.changed() {
            seek_to = Some(pos);
        }
        widgets::readout(
            ui,
            VIDEO_READOUT_W,
            &format!("{}/{}  {:.1}s  {:.0} fps", idx + 1, total, dur, fps),
        );
        if widgets::selector_pill(ui, "LOOP", looping).clicked() {
            if let Some(p) = video.player.as_mut() {
                p.looping = !looping;
            }
        }
        if widgets::icon_button(
            ui,
            Icon::Expand,
            "Fullscreen: zoom, pan and step frame by frame",
        )
        .clicked()
        {
            open_viewer = true;
        }
        if ui
            .add(egui::Button::new(text::note("Close")).frame(false))
            .clicked()
        {
            close = true;
        }
    });
    if let Some(p) = video.player.as_mut() {
        if toggle {
            p.toggle();
        }
        if let Some(i) = seek_to {
            p.seek(i);
        }
    }
    if close {
        video.close();
    }
    open_viewer
}

/// Width of the frame, time and rate readout under a clip.
const VIDEO_READOUT_W: f32 = 160.0;

/// Strip of earlier generations. Clicking one brings it back into the Result panel
/// (the current one is archived in turn, so nothing is lost either way).
fn render_history(
    ui: &mut egui::Ui,
    media: &mut MediaState,
    image_textures: &mut HashMap<String, egui::TextureHandle>,
) {
    if media.history.is_empty() {
        return;
    }
    ui.add_space(widgets::GAP_WIDGETS);
    let mut restore: Option<usize> = None;
    let mut clear = false;
    widgets::section_panel(ui, "EARLIER", |ui| {
        ui.horizontal(|ui| {
            widgets::readout(ui, widgets::READOUT_W, &media.history.len().to_string());
            if ui
                .add(egui::Button::new(text::note("Clear")).frame(false))
                .on_hover_text("Forget the earlier results (frees their memory)")
                .clicked()
            {
                clear = true;
            }
        });
        if clear {
            return;
        }
        widgets::screen_well(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, e) in media.history.iter().enumerate() {
                    let label = if e.prompt.chars().count() > HISTORY_LABEL_CHARS {
                        format!(
                            "{}...",
                            e.prompt
                                .chars()
                                .take(HISTORY_LABEL_CHARS - 1)
                                .collect::<String>()
                        )
                    } else if e.prompt.is_empty() {
                        format!("{:?}", e.kind)
                    } else {
                        e.prompt.clone()
                    };
                    let thumb = egui::vec2(widgets::THUMB_PX, widgets::THUMB_PX);
                    let clicked = match e.images.first() {
                        Some(b64) => {
                            let key = crate::texture::image_cache_key("media_hist", b64, i);
                            let tex = image_textures.entry(key.clone()).or_insert_with(|| {
                                crate::texture::load_base64_texture(ui, b64, &key)
                            });
                            ui.add(
                                egui::Image::new(&*tex)
                                    .fit_to_exact_size(thumb)
                                    .corner_radius(theme::RADIUS)
                                    .sense(egui::Sense::click()),
                            )
                            .on_hover_text(format!("{label}\nClick to bring this result back"))
                            .clicked()
                        }
                        None => {
                            let what = if !e.audios.is_empty() {
                                format!("{} audio", e.audios.len())
                            } else if !e.files.is_empty() {
                                e.files[0].0.clone()
                            } else {
                                "text".to_string()
                            };
                            ui.add_sized(thumb, egui::Button::new(text::note(&what)))
                                .on_hover_text(format!("{label}\nClick to bring this result back"))
                                .clicked()
                        }
                    };
                    if clicked {
                        restore = Some(i);
                    }
                }
            });
        });
    });
    if clear {
        media.history.clear();
    }
    if let Some(i) = restore {
        media.restore_from_history(i);
    }
}

/// Write bytes to a temp file and open them in the system default app
/// (xdg-open / open / start). Mirrors `play_audio_blob`'s approach for
/// non-audio blobs (MIDI, video) that the GUI can't render inline.
fn open_with_system(name: &str, bytes: &[u8]) -> Result<(), String> {
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S_%3f");
    let path = std::env::temp_dir().join(format!("llmgui_media_{ts}.{ext}"));
    std::fs::write(&path, bytes).map_err(|e| format!("temp write: {e}"))?;

    // WINDOWS: hand the path to the shell's own open verb rather than spawning a
    // process. `cmd /C start "" <path>` is the documented portable trick and also,
    // to a behavioural scanner, the exact shape of a dropper: write a file into
    // %TEMP%, then launch it through a command shell.
    // Norton reports the binary as suspicious, and while an unsigned executable
    // will always score badly on reputation alone, there is no reason to hand a
    // heuristic engine a genuine pattern match on top of it.
    //
    // ShellExecuteW is the API `start` itself calls. Declared here as plain FFI:
    // shell32 is already in this binary's import table (the file dialogs use it),
    // so this adds no dependency and no new linkage.
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        unsafe extern "system" {
            fn ShellExecuteW(
                hwnd: *mut std::ffi::c_void,
                lp_operation: *const u16,
                lp_file: *const u16,
                lp_parameters: *const u16,
                lp_directory: *const u16,
                n_show_cmd: i32,
            ) -> *mut std::ffi::c_void;
        }
        const SW_SHOWNORMAL: i32 = 1;
        let wide = |s: &std::ffi::OsStr| -> Vec<u16> {
            s.encode_wide().chain(std::iter::once(0)).collect()
        };
        let verb = wide(std::ffi::OsStr::new("open"));
        let file = wide(path.as_os_str());
        // ShellExecute returns a value ABOVE 32 on success; at or below 32 it is an
        // error code, which is the one thing about this API worth remembering.
        let rc = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        } as isize;
        return if rc > 32 {
            Ok(())
        } else {
            Err(format!(
                "could not open {}: shell error {rc}",
                path.display()
            ))
        };
    }

    // Everything else spawns the platform's opener. Gated on NOT-windows as a whole:
    // the branch above returns, so leaving this reachable on Windows only produced a
    // dead loop over a binding that no longer exists there.
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        let candidates: &[(&str, &[&str])] = &[("open", &[])];
        #[cfg(not(target_os = "macos"))]
        let candidates: &[(&str, &[&str])] = &[("xdg-open", &[])];

        for (cmd, extra) in candidates {
            let mut c = std::process::Command::new(cmd);
            for a in *extra {
                c.arg(a);
            }
            c.arg(&path);
            c.stdout(std::process::Stdio::null());
            c.stderr(std::process::Stdio::null());
            match c.spawn() {
                Ok(_) => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("{cmd}: {e}")),
            }
        }
        Err("no system opener found".into())
    }
}

/// Decode a base64 payload to bytes, returning an empty vec on failure
/// (the save then writes an empty file, which the user will notice — a
/// silent-but-visible failure preferable to a panic in the UI thread).
fn decode_b64(b64: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap_or_default()
}

#[cfg(test)]
mod render_tests {
    //! Headless rendering smoke test for every Media Studio panel. Uses egui_kittest to run
    //! the real egui layout code (no X server, no GPU) so a broken kind — missing match arm,
    //! ID clash, panicking widget — fails CI. Also asserts the new voice panels expose their
    //! distinctive controls in the AccessKit tree.
    use super::overlay_button;
    use super::render;
    use crate::state::{MediaKind, MediaState, SpeechEngine};
    use egui_kittest::kittest::NodeT;
    use egui_kittest::Harness;
    use std::collections::HashMap;

    /// Render one kind (with an optional state tweak) and run a check against the resulting
    /// AccessKit tree. The closure receives a `|label| bool` "contains-label" probe.
    fn on_panel<R>(
        kind: MediaKind,
        tweak: impl FnOnce(&mut MediaState),
        check: impl FnOnce(&dyn Fn(&str) -> bool) -> R,
    ) -> R {
        panel_text(kind, tweak, false, check)
    }

    /// Same, for a panel that is MID-GENERATION.
    ///
    /// A generation in flight draws a spinner and an animated bar, and both ask for
    /// another repaint every frame - so settling is not something that panel ever does,
    /// and `Harness::run` gives up rather than returning a tree. A fixed number of frames
    /// is the only way to look at one.
    fn on_busy_panel<R>(
        kind: MediaKind,
        tweak: impl FnOnce(&mut MediaState),
        check: impl FnOnce(&dyn Fn(&str) -> bool) -> R,
    ) -> R {
        panel_text(kind, tweak, true, check)
    }

    fn panel_text<R>(
        kind: MediaKind,
        tweak: impl FnOnce(&mut MediaState),
        busy: bool,
        check: impl FnOnce(&dyn Fn(&str) -> bool) -> R,
    ) -> R {
        let mut media = MediaState::default();
        media.kind = kind;
        tweak(&mut media);
        let mut textures: HashMap<String, egui::TextureHandle> = HashMap::new();
        let mut harness = Harness::new_ui(move |ui| {
            let mut player = crate::audio_playback::AudioPlayer::default();
            let mut video = crate::video_engine::VideoPlayback::default();
            let _ = render(
                ui,
                &mut media,
                &mut textures,
                &[],
                &[],
                &mut player,
                &mut video,
            );
        });
        if busy {
            harness.run_steps(2);
        } else {
            harness.run();
        }
        // Walk the whole AccessKit tree and collect every node's label + value (lowercased):
        // egui exposes plain `ui.label` text as the node VALUE and widget captions as the
        // LABEL, so `has(s)` matches either, case-insensitively.
        let mut texts: Vec<String> = Vec::new();
        for n in harness.root().children_recursive() {
            let ak = n.accesskit_node();
            if let Some(l) = ak.label() {
                texts.push(l.to_lowercase());
            }
            if let Some(v) = ak.value() {
                texts.push(v.to_lowercase());
            }
        }
        check(&|s: &str| {
            let s = s.to_lowercase();
            texts.iter().any(|t| t.contains(&s))
        })
    }

    /// The hover controls must take NO part in the layout.
    ///
    /// `Ui::put` looked like the natural call and allocates its rect, which grew the
    /// wrapping row - so the whole thumbnail grid re-flowed whenever the pointer entered
    /// an image. Asserts the property that has to hold rather than the symptom: drawing
    /// the overlay leaves the Ui's extent byte-for-byte as it was.
    #[test]
    fn overlay_controls_do_not_take_layout_space() {
        let seen = std::rc::Rc::new(std::cell::RefCell::new(None::<(egui::Rect, egui::Rect)>));
        let out = seen.clone();
        let mut harness = Harness::new_ui(move |ui| {
            ui.label("anchor so the ui has an extent to compare");
            let before = ui.min_rect();
            // A rect deliberately WIDER and TALLER than what the ui has used, so an
            // allocating call could not fail to show up in the extent.
            let over = egui::Rect::from_min_size(before.min, egui::vec2(400.0, 200.0));
            let _ = overlay_button(ui, over, 0, "edit", "Edit", true);
            let _ = overlay_button(ui, over, 1, "save", "Save", false);
            *out.borrow_mut() = Some((before, ui.min_rect()));
        });
        harness.run();
        let (before, after) = seen.borrow().expect("the ui closure did not run");
        assert_eq!(
            before, after,
            "the overlay controls changed the layout extent"
        );
    }

    #[test]
    fn every_media_kind_renders_without_panic() {
        // If any panel's layout code panics or clashes IDs, this fails; the header check
        // confirms the studio actually drew.
        for kind in MediaKind::ALL {
            on_panel(
                kind,
                |_| {},
                |has| {
                    assert!(
                        has("Media Studio"),
                        "{kind:?} panel did not render the studio header"
                    );
                },
            );
        }
    }

    #[test]
    fn speech_panel_exposes_engine_selector() {
        on_panel(
            MediaKind::Speech,
            |_| {},
            |has| {
                assert!(has("Engine"), "no engine selector on the Parler default");
            },
        );
        // Kyutai engine shows the free-text voice field.
        on_panel(
            MediaKind::Speech,
            |m| m.speech.engine = SpeechEngine::Kyutai,
            |has| {
                assert!(has("Voice"), "kyutai panel missing voice field");
            },
        );
    }

    /// A synthesis that is still reading its checkpoint SAYS SO.
    ///
    /// The defect: the tab posted and waited, so tens of seconds of a cold load showed a
    /// frozen window. The phase now comes off the event stream, and the panel has to put
    /// it on screen - including when the backend has no count to give (a Piper voice is
    /// one .onnx file), where the name of the phase is the whole of the information.
    #[test]
    fn a_speech_load_says_which_phase_it_is_in() {
        on_busy_panel(
            MediaKind::Speech,
            |m| {
                m.is_generating = true;
                m.progress = None;
                m.status = "Loading the model".to_string();
            },
            |has| {
                assert!(has("Loading the model"), "the load phase is not on screen");
            },
        );
    }

    /// A counted phase keeps its NAME beside the count.
    ///
    /// The bar alone said "Step 34/100" and nothing else, so a load that counts each file
    /// separately - Kyutai reads several - looked like a bar restarting for no reason,
    /// with nothing on screen to say the load was still going.
    #[test]
    fn a_counted_phase_is_named_and_not_just_numbered() {
        on_busy_panel(
            MediaKind::Speech,
            |m| {
                m.is_generating = true;
                m.progress = Some((34, 100));
                m.status = "Loading the model 34/100".to_string();
            },
            |has| {
                assert!(
                    has("Loading the model 34/100"),
                    "the count is shown without the phase it belongs to"
                );
            },
        );
    }

    /// The cost of a clip is on screen BEFORE the button that starts it - and only once
    /// the server has actually said what it is.
    ///
    /// A video render is the one thing here that can take an hour, and nothing on the
    /// panel said so; the wait was found by waiting. The absent case is asserted too,
    /// because the tempting placeholder - a zero - reads as "instant" for exactly the
    /// settings that are not.
    #[test]
    fn the_video_panel_states_its_cost_only_once_it_knows_it() {
        on_panel(
            MediaKind::Video,
            |m| m.video_estimate = Some(168.0),
            |has| {
                assert!(has("2.8 min"), "the estimate is not shown next to Generate");
                assert!(
                    has("of denoising"),
                    "the estimate is shown as a TOTAL - it only covers the denoise"
                );
            },
        );
        on_panel(
            MediaKind::Video,
            |_| {},
            |has| {
                assert!(
                    !has("of denoising"),
                    "an unanswered estimate was still shown"
                );
                assert!(
                    !has("~0 s"),
                    "a zero stood in for an answer that had not arrived"
                );
            },
        );
    }

    /// Fullscreen has to stay a PLAYER, not a still: the transport, the zoom controls and
    /// the way out must all be on screen once the overlay is up.
    ///
    /// Asserted through the accessibility tree because that is the only part of the
    /// overlay a headless run can read - the picture itself cannot be judged here.
    #[test]
    fn the_fullscreen_viewer_keeps_the_transport_and_the_way_out() {
        let mut harness = Harness::new_ui(move |ui| {
            // A texture stands in for a decoded frame; the viewer refuses to draw without
            // one, which is the state this test is not about.
            let img = egui::ColorImage::from_rgb([2, 2], &[128u8; 12]);
            let tex = ui
                .ctx()
                .load_texture("test_clip_frame", img, egui::TextureOptions::LINEAR);
            let mut video = crate::video_engine::VideoPlayback::with_frame(tex, 0);
            let mut viewer = crate::state::VideoViewer {
                open: true,
                ..Default::default()
            };
            super::render_video_viewer(ui.ctx(), &mut viewer, &mut video);
            assert!(
                viewer.open,
                "the viewer dismissed itself with a frame on screen"
            );
        });
        // Stepped rather than run to convergence: the viewer repaints continuously on
        // purpose, so that playback stays smooth, and `run` treats a ui that never settles
        // as a failure.
        harness.run_steps(2);
        let mut texts: Vec<String> = Vec::new();
        for n in harness.root().children_recursive() {
            let ak = n.accesskit_node();
            if let Some(l) = ak.label() {
                texts.push(l.to_lowercase());
            }
            if let Some(v) = ak.value() {
                texts.push(v.to_lowercase());
            }
        }
        let has = |s: &str| texts.iter().any(|t| t.contains(s));
        assert!(has("fit"), "no way back to fit-to-window");
        assert!(has("close"), "no way out of the overlay");
        assert!(has("frame 1/1"), "the current frame is not stated");
        assert!(has("100%"), "the zoom level is not stated");
    }
}

#[cfg(test)]
mod thumbnail_orientation_tests {
    /// The shapes must be what their names claim, at sizes the models actually take.
    /// A bucket that is off-ratio or not a multiple of 16 is worse than typing the
    /// numbers by hand, because it looks authoritative.
    #[test]
    fn the_shape_presets_are_what_they_say() {
        for (name, w, h) in super::IMAGE_SHAPES {
            assert_eq!(w % 16, 0, "{name}: width {w} is not a multiple of 16");
            assert_eq!(h % 16, 0, "{name}: height {h} is not a multiple of 16");
            let mp = (w as f32 * h as f32) / 1_048_576.0;
            assert!(
                (0.85..=1.15).contains(&mp),
                "{name}: {mp:.2} MP is off the bucket"
            );
            let (num, den) = name
                .rsplit_once(' ')
                .and_then(|(_, r)| r.split_once(':'))
                .map(|(a, b)| (a.parse::<f32>().unwrap(), b.parse::<f32>().unwrap()))
                .expect("every preset names its ratio");
            let claimed = num / den;
            let actual = w as f32 / h as f32;
            assert!(
                (actual / claimed - 1.0).abs() < 0.04,
                "{name}: {w}x{h} is {actual:.3}, not {claimed:.3}"
            );
        }
    }

    /// Portrait and landscape must mirror each other, or the picker offers shapes that
    /// only exist in one direction.
    #[test]
    fn every_portrait_shape_has_its_landscape() {
        for (name, w, h) in super::IMAGE_SHAPES {
            if w == h {
                continue;
            }
            assert!(
                super::IMAGE_SHAPES
                    .iter()
                    .any(|&(_, w2, h2)| w2 == h && h2 == w),
                "{name} has no mirrored counterpart"
            );
        }
    }

    use super::decode_oriented;

    /// A JPEG carrying an EXIF rotation must come back ROTATED.
    ///
    /// This was written once, claimed once, and shipped without ever being run against
    /// a rotated file - the user found a portrait previewing on its side. A landscape
    /// frame tagged "rotate 90" is 40x20 on disk and must decode to 20x40; if the tag
    /// is ignored the dimensions come back unchanged, which is exactly what happened.
    fn jpeg_with_orientation(w: u32, h: u32, orientation: u16) -> Vec<u8> {
        use image::codecs::jpeg::JpegEncoder;
        let img = image::RgbImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            }
        });
        let mut base = Vec::new();
        JpegEncoder::new(&mut base)
            .encode_image(&img)
            .expect("encode");

        // A minimal TIFF header with one IFD entry: Orientation (0x0112), SHORT, 1.
        let mut exif: Vec<u8> = b"Exif\0\0".to_vec();
        let tiff = exif.len();
        exif.extend_from_slice(b"II*\0");
        exif.extend_from_slice(&8u32.to_le_bytes());
        exif.extend_from_slice(&1u16.to_le_bytes());
        exif.extend_from_slice(&0x0112u16.to_le_bytes());
        exif.extend_from_slice(&3u16.to_le_bytes());
        exif.extend_from_slice(&1u32.to_le_bytes());
        exif.extend_from_slice(&orientation.to_le_bytes());
        exif.extend_from_slice(&0u16.to_le_bytes());
        exif.extend_from_slice(&0u32.to_le_bytes());
        let _ = tiff;

        // Splice the APP1 segment in right after SOI.
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
        out.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(&exif);
        out.extend_from_slice(&base[2..]);
        out
    }

    #[test]
    fn a_rotated_photo_is_previewed_upright() {
        let sideways = jpeg_with_orientation(40, 20, 6); // 6 = rotate 90 clockwise
        let img = decode_oriented(&sideways).expect("decodes");
        assert_eq!(
            (img.width(), img.height()),
            (20, 40),
            "the rotation tag was ignored: a portrait photo previews on its side"
        );
    }

    #[test]
    fn an_untagged_photo_is_left_alone() {
        let plain = jpeg_with_orientation(40, 20, 1); // 1 = no transform
        let img = decode_oriented(&plain).expect("decodes");
        assert_eq!((img.width(), img.height()), (40, 20));
    }
}

/// The fullscreen viewer's transport bar and notice strip, the cap on the
/// fit scale, the zoom per wheel point and per click, and its readouts.
const VIEWER_BAR_H: f32 = 44.0;
const VIEWER_STRIP_H: f32 = 32.0;
const VIEWER_FIT_MAX: f32 = 8.0;
const VIEWER_WHEEL_RATE: f32 = 0.0015;
const VIEWER_ZOOM_STEP: f32 = 1.25;
const VIEWER_FRAME_W: f32 = 96.0;
const VIEWER_ZOOM_W: f32 = 48.0;

/// The fullscreen clip viewer: the rendered frame, as large as the window allows, with
/// zoom, pan and frame-by-frame stepping.
///
/// Draws over the whole context rather than inside the tab, because the point is to stop
/// looking at the interface and look at the picture. Escape or a click beside the picture
/// leaves; the transport stays reachable at the bottom so a clip can be judged without
/// going back.
fn render_video_viewer(
    ctx: &egui::Context,
    viewer: &mut crate::state::VideoViewer,
    video: &mut crate::video_engine::VideoPlayback,
) {
    if !viewer.open {
        return;
    }
    // What the overlay owes the viewer when the picture is not simply there. Deciding it
    // BEFORE drawing is what stops the viewer dismissing itself the moment a frame fails
    // to decode - which read as "the fullscreen button does nothing".
    let notice = viewer_notice(
        video.texture.is_some(),
        video.decoding.is_some(),
        video.error.as_deref(),
        video.stale_frame(),
        video.shown_frame(),
    );
    if notice == ViewerNotice::Dismiss {
        viewer.open = false;
        return;
    }
    let tex = video.texture.clone();

    let screen = ctx.content_rect();
    let (mut step, mut toggle) = (0i64, false);
    // Nothing under the overlay may keep keyboard focus while it is up. Without this, the
    // prompt box the user was typing in still owns the keyboard: Space types a space into
    // the prompt instead of pausing, and a focused slider eats the arrow keys and steps
    // twice per press. The overlay owns the screen, so it owns the keys.
    ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    ctx.input(|i| {
        if i.key_pressed(egui::Key::Escape) {
            viewer.open = false;
        }
        if i.key_pressed(egui::Key::Space) {
            toggle = true;
        }
        // Stepping is how flicker is judged: it is a property BETWEEN frames and
        // invisible at playback speed.
        if i.key_pressed(egui::Key::ArrowRight) {
            step += 1;
        }
        if i.key_pressed(egui::Key::ArrowLeft) {
            step -= 1;
        }
        if i.key_pressed(egui::Key::R) {
            viewer.zoom = 1.0;
            viewer.pan = (0.0, 0.0);
        }
    });

    egui::Area::new(egui::Id::new("media_video_viewer"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.set_clip_rect(screen);
            // An opaque ground, so nothing of the interface shows through and biases the
            // eye about brightness or colour.
            ui.painter().rect_filled(screen, 0.0, theme::bg());

            let resp = ui.interact(
                screen,
                egui::Id::new("media_video_viewer_canvas"),
                egui::Sense::click_and_drag(),
            );

            // Where the picture landed, so the click below can tell "on it" from "beside
            // it". Empty while there is nothing to draw, which makes every click a click
            // beside the picture - and the viewer closable even then.
            let mut picture = egui::Rect::NOTHING;
            if let Some(tex) = &tex {
                let raw = tex.size_vec2();
                // Fit first, then the zoom multiplies it - so zoom 1 always means "the
                // whole frame", whatever the clip's resolution.
                let fit = (screen.width() / raw.x)
                    .min(screen.height() / raw.y)
                    .min(VIEWER_FIT_MAX);
                let shown = raw * fit * viewer.zoom;

                if resp.dragged() {
                    let d = resp.drag_delta();
                    viewer.pan = (viewer.pan.0 + d.x, viewer.pan.1 + d.y);
                }
                // Zoom about the POINTER, not the centre: zooming about the middle slides
                // the thing being looked at away exactly when more of it is asked for.
                // Wheel and pinch both, because a trackpad reports the second and would
                // otherwise be a device that cannot zoom.
                let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
                let factor = (1.0 + scroll * VIEWER_WHEEL_RATE) * pinch;
                if (factor - 1.0).abs() > f32::EPSILON {
                    if let Some(p) = resp.hover_pos() {
                        let old = viewer.zoom;
                        let new = (old * factor).clamp(VIEWER_MIN_ZOOM, VIEWER_MAX_ZOOM);
                        if (new - old).abs() > f32::EPSILON {
                            viewer.pan = viewer_zoom_about(
                                viewer.pan,
                                old,
                                new,
                                (p.x, p.y),
                                (screen.center().x, screen.center().y),
                            );
                            viewer.zoom = new;
                        }
                    }
                }
                viewer.pan = viewer_clamp_pan(
                    viewer.pan,
                    (shown.x, shown.y),
                    (screen.width(), screen.height()),
                );

                let centre = screen.center() + egui::vec2(viewer.pan.0, viewer.pan.1);
                picture = egui::Rect::from_center_size(centre, shown);
                // NEAREST above fit: the artefacts being hunted are single pixels, and a
                // smoothing filter is precisely what hides them.
                let mut img = egui::Image::new(egui::load::SizedTexture::new(tex.id(), shown));
                if viewer.zoom > 1.0 {
                    img = img.texture_options(egui::TextureOptions::NEAREST);
                }
                img.paint_at(ui, picture);
            }

            // The transport, over the picture rather than beside it.
            let (idx, total, playing) = match &video.player {
                Some(p) => (p.index(), p.frame_count(), p.is_playing()),
                None => (0, 0, false),
            };
            let bar = egui::Rect::from_min_max(
                egui::pos2(screen.min.x, screen.max.y - VIEWER_BAR_H),
                screen.max,
            );

            // Say what is wrong, rather than leaving a black screen or a picture that
            // quietly belongs to another frame.
            match &notice {
                ViewerNotice::Instead(text) => {
                    ui.painter().text(
                        screen.center(),
                        egui::Align2::CENTER_CENTER,
                        text,
                        egui::FontId::proportional(text::TITLE_PT),
                        theme::ink(),
                    );
                }
                ViewerNotice::Over(text) => {
                    // A strip across the top, not a label beside the picture: the picture
                    // is the only thing being looked at, so a warning about it has to be
                    // in the same field of view.
                    let strip = egui::Rect::from_min_max(
                        screen.min,
                        egui::pos2(screen.max.x, screen.min.y + VIEWER_STRIP_H),
                    );
                    ui.painter().rect_filled(strip, 0.0, theme::panel());
                    ui.painter().text(
                        strip.center(),
                        egui::Align2::CENTER_CENTER,
                        text,
                        egui::FontId::proportional(text::BODY_PT),
                        theme::warning(),
                    );
                }
                ViewerNotice::Clear | ViewerNotice::Dismiss => {}
            }

            surface::plate(ui, bar, 0.0);
            let mut seek: Option<usize> = None;
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(bar.shrink(widgets::GAP_WIDGETS)),
                |ui| {
                    ui.horizontal(|ui| {
                        let (icon, tip) = if playing {
                            (Icon::Pause, "Pause (Space, or click the picture)")
                        } else {
                            (Icon::Play, "Play (Space, or click the picture)")
                        };
                        if widgets::icon_button(ui, icon, tip).clicked() {
                            toggle = true;
                        }
                        if widgets::icon_button(ui, Icon::StepBack, "Previous frame (Left)")
                            .clicked()
                        {
                            step -= 1;
                        }
                        if widgets::icon_button(ui, Icon::StepForward, "Next frame (Right)")
                            .clicked()
                        {
                            step += 1;
                        }
                        if total > 1 {
                            let mut pos = idx;
                            if ui
                                .add(egui::Slider::new(&mut pos, 0..=total - 1).show_value(false))
                                .changed()
                            {
                                seek = Some(pos);
                            }
                        }
                        widgets::readout(
                            ui,
                            VIEWER_FRAME_W,
                            &format!("frame {}/{}", idx + 1, total.max(1)),
                        );
                        // The same three zoom controls as the image viewer, in the same order.
                        if widgets::icon_button(ui, Icon::Minus, "Zoom out").clicked() {
                            viewer.zoom = (viewer.zoom / VIEWER_ZOOM_STEP)
                                .clamp(VIEWER_MIN_ZOOM, VIEWER_MAX_ZOOM);
                        }
                        widgets::readout(
                            ui,
                            VIEWER_ZOOM_W,
                            &format!("{:>4.0}%", viewer.zoom * 100.0),
                        );
                        if widgets::icon_button(ui, Icon::Plus, "Zoom in").clicked() {
                            viewer.zoom = (viewer.zoom * VIEWER_ZOOM_STEP)
                                .clamp(VIEWER_MIN_ZOOM, VIEWER_MAX_ZOOM);
                        }
                        if ui
                            .add(egui::Button::new(text::note("Fit")).frame(false))
                            .on_hover_text("Reset zoom (R)")
                            .clicked()
                        {
                            viewer.zoom = 1.0;
                            viewer.pan = (0.0, 0.0);
                        }
                        if ui
                            .add(egui::Button::new(text::note("Close")).frame(false))
                            .on_hover_text("Escape, or click beside the picture")
                            .clicked()
                        {
                            viewer.open = false;
                        }
                    });
                },
            );

            // A click on the picture pauses it; one beside it leaves. Decided here from
            // the pointer position rather than left to widget ordering, so pressing a
            // transport button can never also dismiss the viewer under it.
            if resp.clicked() {
                match resp.interact_pointer_pos() {
                    Some(p) if bar.contains(p) => {}
                    Some(p) if picture.contains(p) => toggle = true,
                    _ => viewer.open = false,
                }
            }

            if let Some(p) = video.player.as_mut() {
                if toggle {
                    p.toggle();
                }
                if step != 0 {
                    // Stepping implies looking, so it pauses rather than fighting playback.
                    if p.is_playing() {
                        p.toggle();
                    }
                    let n = p.frame_count().max(1) as i64;
                    let next = (p.index() as i64 + step).rem_euclid(n) as usize;
                    p.seek(next);
                }
                if let Some(i) = seek {
                    p.seek(i);
                }
            }
        });
    ctx.request_repaint();
}

// -- the fullscreen clip viewer ----------------------------------------

/// Zoom bounds. Below 1 the clip is smaller than the window, which is what the inline
/// player is for; above 16 a 512-px clip is 8000 px across and there is nothing more to
/// see.
const VIEWER_MIN_ZOOM: f32 = 1.0;
const VIEWER_MAX_ZOOM: f32 = 16.0;

/// What the viewer has to say when the picture is not simply the frame that was asked for.
///
/// Separated from the drawing because otherwise every one of these cases ends the same
/// way - no picture, no message - which is indistinguishable from a broken viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ViewerNotice {
    /// Nothing loaded and nothing on the way: dismiss rather than hold the screen with a
    /// black rectangle the user then has to find their way out of.
    Dismiss,
    /// There is no picture, and this is why. Shown in place of it.
    Instead(String),
    /// A picture is up, but it is not the frame the transport points at. Shown over it.
    Over(String),
    /// The picture is the frame asked for; say nothing.
    Clear,
}

/// Decide what the viewer says, from what the playback state actually knows.
///
/// `stale` is the frame the transport points at when the texture holds another one (or
/// none), and `shown` is what the texture does hold. A frame the decoder refuses leaves
/// the previous picture on screen with the counter still walking: the clip looks frozen
/// and nothing says why, which is the failure this exists to name.
fn viewer_notice(
    has_picture: bool,
    decoding: bool,
    error: Option<&str>,
    stale: Option<usize>,
    shown: Option<usize>,
) -> ViewerNotice {
    if decoding {
        return ViewerNotice::Instead("Decoding the clip...".to_string());
    }
    if let Some(e) = error {
        return ViewerNotice::Instead(format!("This clip could not be decoded: {e}"));
    }
    match (has_picture, stale) {
        (false, None) => ViewerNotice::Dismiss,
        (false, Some(want)) => ViewerNotice::Instead(format!(
            "Frame {} has not decoded - the stream carries nothing readable here",
            want + 1
        )),
        (true, None) => ViewerNotice::Clear,
        (true, Some(want)) => match shown {
            Some(have) => ViewerNotice::Over(format!(
                "Frame {} could not be decoded - showing frame {}",
                want + 1,
                have + 1
            )),
            None => ViewerNotice::Over(format!("Frame {} could not be decoded", want + 1)),
        },
    }
}

/// The pan that keeps the point under the cursor under the cursor across a zoom change.
///
/// Zooming about the window's centre instead is the difference between inspecting a detail
/// and chasing it around the screen: the thing being looked at slides away exactly when
/// the user asks to see more of it.
///
/// All coordinates are screen pixels; `pan` is the image centre's offset from the window
/// centre, which is how it is stored so that a zoom does not have to re-derive it.
fn viewer_zoom_about(
    pan: (f32, f32),
    old_zoom: f32,
    new_zoom: f32,
    cursor: (f32, f32),
    centre: (f32, f32),
) -> (f32, f32) {
    if old_zoom <= 0.0 {
        return pan;
    }
    // Where the cursor sits in image space, then put that same point back under it.
    let k = new_zoom / old_zoom;
    let (cx, cy) = (cursor.0 - centre.0, cursor.1 - centre.1);
    (cx - (cx - pan.0) * k, cy - (cy - pan.1) * k)
}

/// Keep the clip's centre within the window, so it cannot be panned out of sight.
///
/// The limit is half the overhang, which lets any part of a zoomed clip reach the middle
/// of the window and stops there. At fit-zoom the overhang is zero, so the clip is pinned
/// centred and a stray drag does not nudge it off.
fn viewer_clamp_pan(pan: (f32, f32), shown: (f32, f32), window: (f32, f32)) -> (f32, f32) {
    let lim = |s: f32, w: f32| ((s - w) * 0.5).max(0.0);
    let (lx, ly) = (lim(shown.0, window.0), lim(shown.1, window.1));
    (pan.0.clamp(-lx, lx), pan.1.clamp(-ly, ly))
}

#[cfg(test)]
mod viewer_tests {
    use super::*;

    /// The pixel under the cursor stays under the cursor. This is the whole property.
    #[test]
    fn zooming_keeps_the_point_under_the_cursor() {
        let centre = (400.0, 300.0);
        let cursor = (550.0, 250.0); // off-centre, which is where it matters
        for (from, to) in [(1.0f32, 2.0f32), (2.0, 1.0), (1.0, 4.0), (4.0, 1.5)] {
            let pan = viewer_zoom_about((0.0, 0.0), from, to, cursor, centre);
            // Image-space position of the cursor, before and after: (cursor - centre - pan)/zoom
            let before = ((cursor.0 - centre.0) / from, (cursor.1 - centre.1) / from);
            let after = (
                (cursor.0 - centre.0 - pan.0) / to,
                (cursor.1 - centre.1 - pan.1) / to,
            );
            assert!(
                (before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3,
                "{from} -> {to}: {before:?} became {after:?}"
            );
        }
    }

    /// Zooming about the exact centre does not move anything.
    #[test]
    fn zooming_at_the_centre_leaves_the_pan_alone() {
        let c = (400.0, 300.0);
        assert_eq!(viewer_zoom_about((0.0, 0.0), 1.0, 3.0, c, c), (0.0, 0.0));
    }

    /// A clip that fits cannot be dragged off centre; a zoomed one can be panned exactly
    /// to the edge of its overhang and no further.
    #[test]
    fn panning_stops_at_the_edge_of_the_overhang() {
        // Fits the window: pinned.
        assert_eq!(
            viewer_clamp_pan((90.0, -40.0), (800.0, 600.0), (800.0, 600.0)),
            (0.0, 0.0)
        );
        assert_eq!(
            viewer_clamp_pan((90.0, -40.0), (400.0, 300.0), (800.0, 600.0)),
            (0.0, 0.0)
        );
        // Twice the window: half the overhang each way is 400 x 300.
        assert_eq!(
            viewer_clamp_pan((9999.0, -9999.0), (1600.0, 1200.0), (800.0, 600.0)),
            (400.0, -300.0),
            "each axis clamps toward the side it was pushed"
        );
        assert_eq!(
            viewer_clamp_pan((-9999.0, 9999.0), (1600.0, 1200.0), (800.0, 600.0)),
            (-400.0, 300.0)
        );
        // Inside the limit, untouched.
        assert_eq!(
            viewer_clamp_pan((10.0, -20.0), (1600.0, 1200.0), (800.0, 600.0)),
            (10.0, -20.0)
        );
    }

    /// A degenerate zoom must not produce NaN pans that then poison the clamp.
    #[test]
    fn a_degenerate_zoom_is_declined_rather_than_propagated() {
        assert_eq!(
            viewer_zoom_about((5.0, 6.0), 0.0, 2.0, (1.0, 1.0), (0.0, 0.0)),
            (5.0, 6.0)
        );
    }

    /// A clip still being decoded must KEEP the viewer, and say what it is waiting for.
    ///
    /// A viewer that requires a texture and closes itself without one turns opening it
    /// a moment too early into a fullscreen button that does nothing.
    #[test]
    fn a_clip_being_decoded_holds_the_viewer_open() {
        let n = viewer_notice(false, true, None, None, None);
        assert!(matches!(n, ViewerNotice::Instead(ref t) if t.to_lowercase().contains("decod")));
    }

    /// A failed decode names itself instead of leaving a black screen.
    #[test]
    fn a_decode_error_is_stated_in_the_viewer() {
        let n = viewer_notice(false, false, Some("mp4: no H.264 video track"), None, None);
        match n {
            ViewerNotice::Instead(t) => assert!(t.contains("no H.264 video track")),
            other => panic!("the error was swallowed: {other:?}"),
        }
    }

    /// The case that made the viewer look broken: a picture is up, the counter has moved
    /// on, and the two are not the same frame. Both numbers must be on screen.
    #[test]
    fn a_frame_that_did_not_decode_is_named_over_the_stale_picture() {
        match viewer_notice(true, false, None, Some(41), Some(12)) {
            ViewerNotice::Over(t) => {
                assert!(t.contains("42"), "the frame asked for is not named: {t}");
                assert!(
                    t.contains("13"),
                    "the frame actually shown is not named: {t}"
                );
            }
            other => panic!("a stale picture went unannounced: {other:?}"),
        }
    }

    /// When the picture IS the frame asked for, the viewer stays quiet - a warning that
    /// is always on screen is a warning nobody reads.
    #[test]
    fn a_frame_that_is_on_screen_draws_no_warning() {
        assert_eq!(
            viewer_notice(true, false, None, None, Some(7)),
            ViewerNotice::Clear
        );
    }

    /// Nothing loaded, nothing decoding, no error: there is nothing to look at, and
    /// holding the screen with a black rectangle is worse than closing.
    #[test]
    fn an_empty_viewer_dismisses_itself() {
        assert_eq!(
            viewer_notice(false, false, None, None, None),
            ViewerNotice::Dismiss
        );
    }

    /// And the viewer acts on that decision: opened over nothing, it closes.
    #[test]
    fn the_viewer_closes_when_there_is_nothing_behind_it() {
        let ctx = egui::Context::default();
        let mut viewer = crate::state::VideoViewer {
            open: true,
            ..Default::default()
        };
        let mut video = crate::video_engine::VideoPlayback::default();
        super::render_video_viewer(&ctx, &mut viewer, &mut video);
        assert!(
            !viewer.open,
            "the viewer held the screen with nothing on it"
        );
    }

    /// The very first picture failing is not the same message as a picture mid-clip
    /// failing: there is nothing on screen to compare it against.
    #[test]
    fn a_clip_whose_first_picture_fails_says_so_in_place_of_the_picture() {
        match viewer_notice(false, false, None, Some(0), None) {
            ViewerNotice::Instead(t) => assert!(t.contains('1'), "frame not named: {t}"),
            other => panic!("expected a stand-in message, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod estimate_label_tests {
    use super::*;

    /// The unit tracks the magnitude, and every wording carries the tilde - the estimate
    /// is scaled from one measured render, not a promise.
    #[test]
    fn a_wait_is_worded_in_the_unit_someone_would_use() {
        assert_eq!(format_estimate(45.0), "~45 s");
        assert_eq!(format_estimate(89.0), "~89 s");
        assert_eq!(format_estimate(168.0), "~2.8 min");
        assert_eq!(format_estimate(600.0), "~10.0 min");
        assert_eq!(format_estimate(7200.0), "~2.0 h");
        for s in [0.0f32, 1.0, 91.0, 3600.0, 100_000.0] {
            assert!(format_estimate(s).starts_with('~'), "{s} lost its tilde");
        }
    }

    /// The label never reads as more precise than it is, and never crosses a unit boundary
    /// into a number the reader has to divide themselves.
    #[test]
    fn the_wording_stays_readable_across_the_whole_range() {
        // Just under each boundary stays in the smaller unit, just over crosses.
        assert!(format_estimate(89.9).ends_with(" s"));
        assert!(format_estimate(90.1).ends_with(" min"));
        assert!(format_estimate(5399.0).ends_with(" min"));
        assert!(format_estimate(5401.0).ends_with(" h"));
        // A negative can only come from a malformed reply; it must not print a minus sign
        // next to the Generate button.
        assert_eq!(format_estimate(-5.0), "~0 s");
    }
}
