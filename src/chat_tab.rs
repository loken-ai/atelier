//! Chat Tab Rendering
//!
//! A chrome row, the message list in a well, the composer in a second well.
//! A message is a full-width row: its role in capitals, a stripe in the
//! role's tint, the body, and a monospace footer.
//!
//! The pure helpers live beside this rather than in it: `crate::modality`
//! (ModelModality, helpers and constants), `crate::texture` (the texture cache),
//! `crate::audio_playback` (playback), `crate::dialog` (file dialogs on a worker
//! thread).

use eframe::egui;
use egui::{Color32, RichText, TextureHandle};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::audio_playback::play_audio_blob;
use crate::dialog::{save_button, spawn_dialog_worker, spawn_save};
use crate::icons::Icon;
use crate::modality::{
    base64_looks_like_image, chat_input_visible_rows, chat_send_allowed,
    format_attachment_cap_mb, format_timing_line, is_error_system_message,
    parse_seed_prefix, path_is_image_ext, truncate_with_ellipsis,
    ModelModality, CHAT_ATTACHMENT_MAX_BYTES, CHAT_AUDIO_EXTS, CHAT_IMAGE_EXTS,
    CHAT_INPUT_ATTACH_STRIP_PX, CHAT_INPUT_BASE_CHROME_PX, CHAT_INPUT_ROW_PX,
    IMAGE_NUM_STEPS_MAX, IMAGE_NUM_STEPS_MIN, IMAGE_NUM_STEPS_PLACEHOLDER,
    IMAGE_SIZE_PRESETS, IMAGE_STRENGTH_MAX, IMAGE_STRENGTH_MIN, IMAGE_STRENGTH_PLACEHOLDER,
    TTS_INPUT_MAX_CHARS_CLIENT, TTS_SPEED_DEFAULT, TTS_SPEED_MAX, TTS_SPEED_MIN,
    TTS_VOICE_PRESETS,
};
use crate::state::{ChatDialogResult, ChatMessage, ChatState, ModelState};
use crate::texture::{clear_attach_chip_textures, image_cache_key, load_base64_texture};
use crate::theme::{self, text, HAIRLINE};
use crate::ui::{surface, widgets};

use std::collections::HashMap;

/// The wash over the window while files are dragged over it, and its label.
const DROP_OVERLAY_A: u8 = 40;
const DROP_LABEL_PT: f32 = 24.0;
/// The least height the message well keeps when the window is short.
const MIN_OUTPUT_H: f32 = 60.0;
/// Space above the empty state's icon.
const EMPTY_STATE_TOP: f32 = 40.0;
/// Point size of the icons in the chrome row, and of the smaller ones beside
/// a control.
const ICON_PT: f32 = 14.0;
const ICON_PT_SMALL: f32 = 11.0;
/// Fixed cells of the chrome row: the profile name and the modality.
const PROFILE_W: f32 = widgets::READOUT_WIDE_W;
const MODALITY_W: f32 = 64.0;
/// Characters of a profile or model name shown in the chrome row.
const PROFILE_CHARS: usize = 20;
const MODEL_CHARS: usize = 25;
/// The model picker and its popup.
const MODEL_PICKER_W: f32 = 220.0;
const MODEL_POPUP_H: f32 = 360.0;
/// Width of the cell that says whether a slider overrides the server.
const OVERRIDE_W: f32 = 56.0;
/// Side of an attachment thumbnail in the composer.
const CHIP_THUMB_PX: f32 = 40.0;
/// Side of the close mark on a chip.
const CHIP_CLOSE_PX: f32 = 16.0;
/// The caret at the end of a streaming reply.
const CARET_SIZE: egui::Vec2 = egui::Vec2::new(7.0, 14.0);

/// Height to keep below the message list for everything the composer draws.
///
/// The strip that says a reply is on its way sits between the list and the input row, and it
/// was not counted: the composer was pushed past the bottom of the window for the whole time
/// a model was answering, which is the only time anyone is looking. Its height is taken from
/// the style rather than written down, so a spacing change moves the reservation with it.
fn chat_input_reserved_height(
    ui: &egui::Ui,
    input_rows: usize,
    has_attachments: bool,
    is_generating: bool,
) -> f32 {
    let spacing = ui.spacing().clone();
    let generating = if is_generating {
        // One horizontal row of the spinner's own size, the 4px that follows it, and the gap
        // the layout puts between that row and the input.
        spacing.interact_size.y + spacing.item_spacing.y + 4.0
    } else {
        0.0
    };
    CHAT_INPUT_BASE_CHROME_PX
        + (input_rows as f32) * CHAT_INPUT_ROW_PX
        + if has_attachments { CHAT_INPUT_ATTACH_STRIP_PX } else { 0.0 }
        + generating
}

/// Bundle of values `render` produces for the caller to consume.
///
/// - `send_clicked`: true when the Send button was clicked this frame.
///   app.rs ORs this with its own Enter-key detection so the button
///   and the Enter shortcut share a single send code path.
/// - `modality`: the selected model's classification. app.rs uses it
///   for the send-gate check that would otherwise re-walk
///   `ModelModality::from_model_name` a second time per chat-tab
///   frame.
pub struct ChatRenderOutput {
    pub send_clicked: bool,
    pub modality: ModelModality,
}

/// Render the Chat tab content. See [`ChatRenderOutput`] for the
/// caller-consumable values produced this frame.
pub fn render(
    ui: &mut egui::Ui,
    chat: &mut ChatState,
    models: &mut ModelState,
    selected_profile_name: Option<&String>,
    profiles: &[crate::config::ApiProfile],
    md_cache: &mut CommonMarkCache,
    image_textures: &mut HashMap<String, TextureHandle>,
) -> ChatRenderOutput {
    // Resolve the selected-model modality once at the top of the
    // chat-tab render. Used inside the with_layout closure for
    // empty-state copy, typing indicator, and the input-area
    // tooltips; also returned to app.rs so its post-render send-
    // gate can skip a second classifier call.
    let modality = models
        .selected_model
        .as_deref()
        .map(ModelModality::from_model_name)
        .unwrap_or(ModelModality::Text);

    // Drain any file-dialog result a worker thread produced since last
    // frame. Applied before drag-and-drop / rest of render so newly-
    // attached files / saved exports show in the same frame they land.
    let dialog_result = chat
        .pending_dialog
        .lock()
        .ok()
        .and_then(|mut g| g.take());
    if let Some(result) = dialog_result {
        match result {
            ChatDialogResult::AttachFiles { files, oversized, unreadable } => {
                // base64 already encoded on the worker thread — just
                // move the strings into the staging vecs here so the
                // GUI thread never blocks on the encode (matters for
                // multi-MB image attachments).
                for (path, b64) in files {
                    chat.add_attachment(b64, path.to_string_lossy().to_string());
                }
                clear_attach_chip_textures(image_textures);
                // Surface oversized rejections inline so the user
                // sees them in the chat scrollback (alternative would
                // be a transient toast, but the chat tab has no toast
                // surface yet). Mirrors the server-side rejection
                // wording for consistency.
                for (path, size) in oversized {
                    let size_mb = size / (1024 * 1024);
                    let cap = format_attachment_cap_mb();
                    // Show just the basename to keep the message
                    // readable — a full filesystem path on Linux
                    // can be 80+ chars and shoves the size + cap
                    // off-screen on narrow chat layouts. Fall back
                    // to the full path only if extraction fails
                    // (path ends in / or contains no UTF-8 segment).
                    let display_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| path.display().to_string());
                    chat.messages.push_back(ChatMessage::system(format!(
                        "Attachment too large: {display_name} is {size_mb} MB (max {cap})",
                    )));
                }
                // Surface unreadable files (perm denied, broken
                // symlink, deleted-between-dialog-and-read) inline
                // too — matches the drag-drop behaviour added in
                // 9f3e1ac. "Failed to read" prefix triggers red
                // error styling via is_error_system_message.
                for (path, err) in unreadable {
                    let display_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| path.display().to_string());
                    chat.messages.push_back(ChatMessage::system(format!(
                        "Failed to read {display_name}: {err}. \
                         Check file permissions and re-attach."
                    )));
                }
            }
            ChatDialogResult::SaveBytes { path, bytes } => {
                // Surface BOTH success and failure inline. Without a
                // success confirmation the user can't tell whether
                // the file dialog's Save button actually wrote
                // anything — they have to alt-tab to a file manager
                // to check. Failures (full disk / permission denied
                // / read-only mount) also surface here so the user
                // sees what went wrong without a silent drop.
                let byte_count = bytes.len();
                match std::fs::write(&path, bytes) {
                    Ok(()) => {
                        let display_name = path.file_name()
                            .and_then(|n| n.to_str())
                            .map(str::to_string)
                            .unwrap_or_else(|| path.display().to_string());
                        chat.messages.push_back(ChatMessage::system(format!(
                            "Saved {display_name} ({}).",
                            crate::api::types::format_size(byte_count as u64),
                        )));
                    }
                    Err(e) => {
                        chat.messages.push_back(ChatMessage::system(format!(
                            "Failed to save {}: {}", path.display(), e,
                        )));
                    }
                }
            }
            ChatDialogResult::SaveBytesMany { dir, files } => {
                let mut failures = Vec::new();
                let mut saved_count = 0usize;
                let mut saved_bytes = 0usize;
                for (name, bytes) in files {
                    let target = dir.join(&name);
                    let len = bytes.len();
                    match std::fs::write(&target, bytes) {
                        Ok(()) => {
                            saved_count += 1;
                            saved_bytes += len;
                        }
                        Err(e) => failures.push(format!("{}: {}", name, e)),
                    }
                }
                if saved_count > 0 {
                    chat.messages.push_back(ChatMessage::system(format!(
                        "Saved {} file{} ({}) to {}.",
                        saved_count,
                        if saved_count == 1 { "" } else { "s" },
                        crate::api::types::format_size(saved_bytes as u64),
                        dir.display(),
                    )));
                }
                if !failures.is_empty() {
                    chat.messages.push_back(ChatMessage::system(format!(
                        "Failed to save {} file{} to {}:\n  {}",
                        failures.len(),
                        if failures.len() == 1 { "" } else { "s" },
                        dir.display(),
                        failures.join("\n  "),
                    )));
                }
            }
            // Media Studio picks only ever land in MediaState's own pending_dialog (a
            // distinct Arc), never in ChatState's - listed explicitly rather than with
            // `_` so a future variant forces a routing decision here.
            ChatDialogResult::MediaAudio { .. } => {}
        }
        ui.ctx().request_repaint();
    }

    // Drag-and-drop: files dropped anywhere onto the window land in
    // ctx.input().raw.dropped_files. We pick them up once per frame at
    // the chat tab entry and stage them via the same attached_images /
    // attached_image_paths slots that the file-picker fills. Path is
    // present on native targets; web targets only get bytes — handle
    // both so the same code works in eframe-wasm builds.
    //
    // Size-gate identically to the file-picker worker (CHAT_ATTACHMENT_MAX_BYTES).
    // Without this, dropping a multi-GB file on the window would
    // synchronously fs::read + base64-encode it on the GUI thread —
    // freezing the UI for seconds and potentially exhausting RAM.
    // Reject oversized files with the same inline system message the
    // file-picker uses so the UX is symmetric across both entry points.
    let (hovered_count, dropped_files): (usize, Vec<egui::DroppedFile>) = ui.ctx()
        .input(|i| (i.raw.hovered_files.len(), i.raw.dropped_files.clone()));
    if !dropped_files.is_empty() {
        use base64::Engine;
        let cap = format_attachment_cap_mb();
        for file in dropped_files {
            // Probe size first so we can reject without reading. On
            // native (path set) we use metadata — cheap, no I/O of
            // the actual bytes. On web (bytes set, no path) we
            // already have the bytes in memory but still gate the
            // base64 + push so the server doesn't reject with 413
            // after the round-trip.
            let display_name = file.path.as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(str::to_string)
                .unwrap_or_else(|| file.name.clone());
            let display_path = file.path.as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| file.name.clone());

            let size_result: Result<u64, std::io::Error> = if let Some(ref path) = file.path {
                std::fs::metadata(path).map(|m| m.len())
            } else {
                Ok(file.bytes.as_ref().map(|b| b.len() as u64).unwrap_or(0))
            };
            let size = match size_result {
                Ok(s) => s,
                Err(e) => {
                    // metadata() failed — most commonly permission
                    // denied or a broken symlink. The previous
                    // `unwrap_or(0)` let this case silently fall
                    // through the cap check, then std::fs::read
                    // failed too and the drop produced NO attachment
                    // and NO error. Surface it inline so the user
                    // knows the drop was registered but the file
                    // isn't readable.
                    chat.messages.push_back(ChatMessage::system(format!(
                        "Failed to read {display_name}: {e}. \
                         Check file permissions and re-attach."
                    )));
                    continue;
                }
            };
            if size > CHAT_ATTACHMENT_MAX_BYTES {
                let size_mb = size / (1024 * 1024);
                chat.messages.push_back(ChatMessage::system(format!(
                    "Skipped {display_name}: {size_mb} MB exceeds the {cap} attachment cap. \
                     Resize or split the file before re-attaching."
                )));
                continue;
            }

            // size-gate passed — actually read the bytes. Different
            // failure modes from metadata (e.g. file deleted between
            // metadata and read, or the disk hiccupped) get the same
            // inline error message instead of silently dropping the
            // attachment.
            let bytes_result: Result<Vec<u8>, std::io::Error> = if let Some(ref path) = file.path {
                std::fs::read(path)
            } else {
                Ok(file.bytes.as_deref().map(<[u8]>::to_vec).unwrap_or_default())
            };
            match bytes_result {
                Ok(bytes) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    chat.add_attachment(b64, display_path);
                }
                Err(e) => {
                    chat.messages.push_back(ChatMessage::system(format!(
                        "Failed to read {display_name}: {e}. \
                         Check file permissions and re-attach."
                    )));
                }
            }
        }
        // Texture cache for input chips keys by index — invalidate so
        // newly-attached files trigger a fresh decode pass.
        clear_attach_chip_textures(image_textures);
    }

    // Drag hover overlay — translucent full-window panel that appears
    // while files are being dragged over the GUI but not yet dropped.
    // Confirms the drop target is live so users don't wonder if the
    // drag is registering.
    if hovered_count > 0 {
        let screen = ui.ctx().content_rect();
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("chat_drop_overlay"),
        ));
        painter.rect_filled(screen, 0.0, theme::tinted(theme::accent(), DROP_OVERLAY_A));
        let label = if hovered_count == 1 {
            "Drop file to attach".to_string()
        } else {
            format!("Drop {} files to attach", hovered_count)
        };
        painter.text(
            screen.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(DROP_LABEL_PT),
            theme::ink(),
        );
    }

    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        // Header card
        render_chat_header(ui, chat, selected_profile_name, profiles, models, image_textures, md_cache);

        ui.add_space(12.0);

        // Not-loaded notice. A model can be *selected* without being
        // loaded into memory; the first send then silently pays a
        // multi-second load. The header dropdown already colours the
        // name amber, but that's easy to miss — surface a persistent
        // one-line warning so the wait isn't a surprise. Hidden while
        // generating (the typing indicator shows the load there).
        if !chat.is_generating {
            if let Some(name) = models.selected_model.as_deref() {
                if !models.is_loaded(name) {
                    ui.horizontal(|ui| {
                        widgets::lamp_inline(ui, true, theme::warning());
                        ui.label(text::note(
                            "Model not loaded: the first response will be slower while it loads into memory.",
                        ));
                    });
                    ui.add_space(widgets::GAP_WIDGETS);
                }
            }
        }

        // Chat messages area — reserve space for input area below.
        // Row count derives from chat_input_visible_rows so the
        // reservation always agrees with the TextEdit's desired_rows
        // (single source of truth in CHAT_INPUT_* constants).
        let input_rows = chat_input_visible_rows(&chat.input);
        let attach_extra = if chat.attached_images.is_empty() { 0.0 } else { CHAT_INPUT_ATTACH_STRIP_PX };
        let input_height = chat_input_reserved_height(
            ui,
            input_rows,
            attach_extra > 0.0,
            chat.is_generating,
        );
        let output_height = (ui.available_height() - input_height).max(MIN_OUTPUT_H);
        let (well_rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), output_height),
            egui::Sense::hover(),
        );
        surface::recess(ui, well_rect, surface::WELL_RADIUS);
        // The list is top-anchored in the well; it follows its tail only while
        // a reply streams.
        let mut list = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(well_rect.shrink(widgets::SECTION_PADDING))
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(chat.is_generating)
            .show(&mut list, |ui| {

                if chat.messages.is_empty() && !chat.is_generating {
                    // Three distinct empty states:
                    //   1. No model selected — guide the user to pick one
                    //      before they wonder why nothing happens.
                    //   2. Model selected + non-Text modality — show the
                    //      modality-specific quick-start prompts.
                    //   3. Model selected + Text — the classic 'No messages
                    //      yet' (no extras to avoid noise on plain LLMs).
                    let (icon, title, prompts): (Icon, &str, &[&str]) = if models.selected_model.is_none() {
                        (
                            Icon::Package,
                            "No model selected",
                            // Empty prompts; hint surfaces via the subtitle.
                            &[][..],
                        )
                    } else {
                        match modality {
                            ModelModality::ImageGen => (
                                Icon::Palette,
                                "Image Generation Ready",
                                &[
                                    "A castle floating in the clouds at sunset",
                                    "A cyberpunk city street at night, neon reflections in puddles",
                                    "A photorealistic portrait of an astronaut on Mars",
                                ][..],
                            ),
                            ModelModality::AudioTts => (
                                Icon::Speaker,
                                "Speech Synthesis Ready",
                                &[
                                    "Welcome to the speech synthesis demo. The weather is beautiful today.",
                                    "This is a quick demonstration of high-quality voice generation.",
                                ][..],
                            ),
                            ModelModality::AudioAsr => (
                                Icon::Mic,
                                "Audio Transcription Ready",
                                // No click-to-fill prompts here. The
                                // previous entry was
                                // "Attach an audio file via the Attach
                                // button to transcribe it." which read
                                // fine as a hint but clicking it
                                // dropped that instruction text into
                                // the chat input — useless as an
                                // initial_prompt for Whisper, and
                                // confusing UX. The modality-specific
                                // subtitle below already shows the
                                // attach guidance for the user.
                                &[][..],
                            ),
                            ModelModality::VideoGen => (
                                Icon::Film,
                                "Video Generation (not runnable)",
                                // No click-to-fill prompts here. Offering the
                                // warning as one drops that text into the chat
                                // input, which is meaningless to send. The
                                // subtitle below ("Video generation
                                // not yet available — see the note
                                // above.") already conveys the state
                                // and the Send button is locked
                                // anyway via chat_send_allowed.
                                &[][..],
                            ),
                            ModelModality::Vision => (
                                Icon::Eye,
                                "Vision Model Ready",
                                &[
                                    "What's in this image?",
                                    "Describe the contents in detail.",
                                    "Extract any text visible in the image.",
                                ][..],
                            ),
                            ModelModality::Text => (
                                Icon::Chat,
                                "No messages yet",
                                &[][..],
                            ),
                        }
                    };
                    // Some image-gen models depend on a gated HF repo
                    // for their VAE (Flux Schnell pulls ae.safetensors
                    // from black-forest-labs/FLUX.1-schnell). When the
                    // user picks one, surface the auth requirement in
                    // the welcome subtitle so they don't waste a click
                    // on Send and discover it only after the loader 401s.
                    let model_name = models.selected_model.as_deref();
                    let flux_needs_auth = model_name.is_some_and(|n| {
                        // ASCII-CI contains so the empty-state hint
                        // doesn't allocate a lowercased copy of the
                        // model name every frame. Z-Image models
                        // (Tongyi-MAI/Z-Image-Turbo) carry "z-image"
                        // and use a different VAE — they DON'T need
                        // the HF auth wall, so exclude them here.
                        crate::log_buffer::contains_ascii_ci(n, b"flux")
                            && !crate::log_buffer::contains_ascii_ci(n, b"z-image")
                    });
                    let subtitle = if model_name.is_none() {
                        // The chat header now has a model dropdown
                        // (commit a786059), so the user has two paths
                        // to selection. Mention both so first-run
                        // users don't tab-hunt for a model picker
                        // that's actually right above them.
                        "Pick a model from the dropdown above, or open the Models tab to import one."
                    } else if flux_needs_auth {
                        // Multi-line subtitle covering the 3 setup steps,
                        // matching the server-side error message
                        // (handle_image_generation → "Flux VAE requires
                        // HuggingFace authentication.").
                        "Flux needs HuggingFace auth for its gated VAE: \
                         (1) accept license at huggingface.co/black-forest-labs/FLUX.1-schnell, \
                         (2) `huggingface-cli login`, (3) restart the server. \
                         For a no-auth alternative, use Tongyi-MAI/Z-Image-Turbo."
                    } else {
                        // Modality-specific subtitle so the empty-state
                        // verb matches what the model does. Uses the
                        // same `modality` hoisted at the top of the
                        // render function so this site doesn't re-walk
                        // ModelModality::from_model_name's classifier.
                        match modality {
                            ModelModality::Text     => "Type a message below to start chatting.",
                            ModelModality::Vision   => "Attach an image (PNG / JPG / GIF / BMP / WebP) and ask a question below.",
                            ModelModality::ImageGen => "Type a prompt below to generate an image.",
                            ModelModality::VideoGen => "This checkpoint is tagged as video-gen, but the server does not yet have a video runtime — chat will fail. Use the API directly once a video pipeline lands.",
                            ModelModality::AudioTts => "Type text below to synthesize speech.",
                            ModelModality::AudioAsr => "Attach an audio file (WAV / MP3 / FLAC / OGG / M4A / AAC) below to transcribe it.",
                        }
                    };
                    ui.vertical_centered(|ui| {
                        ui.add_space(EMPTY_STATE_TOP);
                        icon.show(ui, text::EMPTY_STATE_ICON_PT, theme::ink_dim());
                        ui.add_space(widgets::GAP_WIDGETS);
                        ui.label(text::value(title));
                        ui.add_space(widgets::GAP_LABEL);
                        ui.label(text::note(subtitle));
                        if !prompts.is_empty() {
                            ui.add_space(widgets::GAP_SECTIONS);
                            ui.label(text::label("TRY ONE OF THESE"));
                            ui.add_space(widgets::GAP_LABEL);
                            for prompt in prompts {
                                if widgets::selector_pill(ui, prompt, false)
                                    .on_hover_text("Click to use this prompt")
                                    .clicked()
                                {
                                    chat.input = (*prompt).to_string();
                                }
                            }
                        }
                    });
                } else {
                    // Per-frame seed-lock collector — any "Lock seed"
                    // click during this render pass writes into here.
                    // Drained into chat.locked_seed after the message
                    // loop finishes (can't mutate chat while iter()'d).
                    let seed_lock_request: std::cell::Cell<Option<u64>> =
                        std::cell::Cell::new(None);
                    // Single-shot slot for audio-playback errors
                    // (see render_message's play_error_pending param
                    // for the why). Drained after the iter() so we
                    // can mutate chat.messages without conflicting
                    // with the immutable borrow.
                    let play_error_pending: std::cell::Cell<Option<String>> =
                        std::cell::Cell::new(None);
                    for msg in chat.messages.iter() {
                        render_message(
                            ui, msg, md_cache, image_textures,
                            &chat.pending_dialog, &chat.dialog_in_flight,
                            &seed_lock_request, &play_error_pending,
                        );
                        ui.add_space(4.0);
                    }
                    if let Some(seed) = seed_lock_request.get() {
                        chat.locked_seed = Some(seed);
                    }
                    if let Some(err) = play_error_pending.take() {
                        chat.messages.push_back(ChatMessage::system(err));
                    }

                    if chat.is_generating {
                        if !chat.streaming_content.is_empty() {
                            render_streaming_message(ui, &chat.streaming_content, &chat.image_gen_progress, chat.image_gen_started_at, md_cache);
                            ui.add_space(4.0);
                        } else {
                            // Modality-aware indicator — tells the user
                            // what KIND of generation is happening even
                            // before the first response chunk arrives.
                            // Reuses the hoisted `modality`. When the
                            // target model isn't loaded yet, the first
                            // send is really a model load; surface that
                            // instead of a bare "Thinking…".
                            let loading_model = models
                                .selected_model
                                .as_deref()
                                .filter(|m| !models.is_loaded(m));
                            render_typing_indicator(ui, modality, loading_model);
                        }
                    }
                }
            });

        ui.add_space(8.0);

        // Input area — pass the hoisted `modality` so the input hint
        // text and attach button can adapt to the selected model.
        // Also pass whether any model is selected so the Send button
        // can disable proactively when models.selected_model is None
        // instead of letting the user click and discover the error
        // post-send.
        let model_selected = models.selected_model.is_some();
        let send_clicked = render_input_area(
            ui, chat, image_textures, modality, model_selected,
            input_rows,
        );
        // Stash whether the button was clicked so we can propagate it out
        // of the closure boundary below.
        ui.ctx().memory_mut(|m| {
            m.data.insert_temp(egui::Id::new("chat_send_clicked"), send_clicked);
        });
    });

    // Pull the closure-local send_clicked back out through ctx memory.
    // The closure borrow rules keep us from returning across the
    // ui.with_layout boundary directly.
    let send_clicked = ui
        .ctx()
        .memory(|m| m.data.get_temp::<bool>(egui::Id::new("chat_send_clicked")))
        .unwrap_or(false);
    ChatRenderOutput { send_clicked, modality }
}

/// The chrome row of the chat: title, profile, the model picker with its
/// lamp, the modality, the layer mode, Clear.
#[allow(clippy::too_many_arguments)]
fn render_chat_header(
    ui: &mut egui::Ui,
    chat: &mut ChatState,
    selected_profile_name: Option<&String>,
    profiles: &[crate::config::ApiProfile],
    models: &mut ModelState,
    image_textures: &mut HashMap<String, TextureHandle>,
    md_cache: &mut CommonMarkCache,
) {
    widgets::chrome_row(ui, |ui| {
        Icon::Chat.show(ui, ICON_PT, theme::ink());
        ui.label(text::title("Chat"));
        ui.add_space(widgets::GAP_WIDGETS);

        // The profile in a fixed cell; API type and server on hover. A warning
        // lamp stands in when none is selected.
        match selected_profile_name.and_then(|name| profiles.iter().find(|p| p.name == *name)) {
            Some(profile) => {
                let shown = truncate_with_ellipsis(&profile.name, PROFILE_CHARS);
                let api_kind = match profile.api_type {
                    crate::config::ApiType::Loken => "LOKEN",
                    crate::config::ApiType::Ollama => "Ollama",
                    crate::config::ApiType::OpenApi => "OpenAPI",
                };
                widgets::fixed_label(ui, PROFILE_W, text::mono(shown.as_ref())).on_hover_ui(|ui| {
                    ui.label(format!(
                        "Profile: {}\nAPI: {}\nServer: {}",
                        profile.name, api_kind, profile.server_url,
                    ));
                });
            }
            None => {
                widgets::lamp_inline(ui, true, theme::warning());
                widgets::fixed_label(ui, PROFILE_W, text::note("No profile")).on_hover_text(
                    "No API profile selected.\n\
                     Open the Settings tab and pick (or create) a profile under\n\
                     \"API Profiles\" to define which server + parameters this\n\
                     chat uses.",
                );
            }
        }
        ui.add_space(widgets::GAP_WIDGETS);

        // The model picker. Its lamp says whether the selection is loaded; the
        // picker is locked while a reply streams, since the request went out
        // against the model selected at send time. The selection write is
        // deferred past the popup so the list can be borrowed while it is open.
        let selected: Option<&String> = models.selected_model.as_ref();
        let current_is_loaded =
            chat.smart_auto || selected.map(String::as_str).is_some_and(|m| models.is_loaded(m));
        widgets::lamp_inline(ui, current_is_loaded, theme::success());
        let selected_text: std::borrow::Cow<str> = if chat.smart_auto {
            "Auto (smart routing)".into()
        } else {
            match selected.map(String::as_str) {
                Some(m) => truncate_with_ellipsis(m, MODEL_CHARS),
                None => "No model selected".into(),
            }
        };
        let mut new_selection: Option<String> = None;
        let mut auto_clicked = false;
        let combo_resp = ui.add_enabled_ui(!chat.is_generating, |ui| {
            egui::ComboBox::from_id_salt("chat_model_picker")
                .selected_text(
                    RichText::new(selected_text.as_ref()).size(text::SECTION_PT).color(theme::ink()),
                )
                .width(MODEL_PICKER_W)
                .show_ui(ui, |ui| {
                    let auto_row = ui
                        .selectable_label(
                            chat.smart_auto,
                            RichText::new("Auto (smart routing)").size(text::SECTION_PT),
                        )
                        .on_hover_text(
                            "Let the server pick the model per prompt: chat, vision, \
                             image generation or speech (rules + classifier LLM).",
                        );
                    if auto_row.clicked() {
                        auto_clicked = true;
                    }
                    ui.separator();
                    if models.available_models.is_empty() {
                        ui.label(text::note("No models available: open the Models tab to import one"));
                        return;
                    }
                    // Grouped by modality, the likeliest target first.
                    const GROUPS: &[(ModelModality, &str)] = &[
                        (ModelModality::Text, "TEXT"),
                        (ModelModality::Vision, "VISION"),
                        (ModelModality::ImageGen, "IMAGE"),
                        (ModelModality::AudioTts, "TTS"),
                        (ModelModality::AudioAsr, "ASR"),
                        (ModelModality::VideoGen, "VIDEO"),
                    ];
                    let mut sorted: Vec<&crate::api::ModelInfo> =
                        models.available_models.iter().collect();
                    sorted.sort_by(|a, b| a.name.cmp(&b.name));
                    egui::ScrollArea::vertical()
                        .max_height(MODEL_POPUP_H)
                        .show(ui, |ui| {
                            let mut by_modality: HashMap<ModelModality, Vec<&crate::api::ModelInfo>> =
                                HashMap::with_capacity(GROUPS.len());
                            for m in &sorted {
                                let modality = ModelModality::from_model_name(&m.name);
                                by_modality.entry(modality).or_default().push(m);
                            }
                            for (group_modality, group_label) in GROUPS {
                                let Some(in_group) = by_modality.get(group_modality) else {
                                    continue;
                                };
                                if in_group.is_empty() {
                                    continue;
                                }
                                ui.label(text::label(group_label));
                                for m in in_group {
                                    let is_selected =
                                        selected.map(String::as_str) == Some(m.name.as_str());
                                    let is_loaded = models.is_loaded(&m.name);
                                    ui.horizontal(|ui| {
                                        widgets::lamp_inline(ui, is_loaded, theme::success());
                                        let row = ui
                                            .selectable_label(
                                                is_selected,
                                                RichText::new(&m.name)
                                                    .size(text::SECTION_PT)
                                                    .color(theme::ink()),
                                            )
                                            .on_hover_text(if is_loaded {
                                                "Loaded in memory"
                                            } else {
                                                "Not loaded: the first send triggers a load"
                                            });
                                        if row.clicked() {
                                            new_selection = Some(m.name.clone());
                                        }
                                    });
                                }
                                ui.add_space(widgets::GAP_LABEL);
                            }
                        });
                })
        })
        .inner;
        let selected_name = selected.map(String::as_str);
        combo_resp.response.on_hover_ui(|ui| {
            if chat.is_generating {
                ui.label("Locked while a response is generating.");
                ui.label("Stop (or press Esc) to switch models.");
            } else if let Some(m) = selected_name {
                ui.label(text::value(m));
                ui.label(if current_is_loaded {
                    "Loaded in memory"
                } else {
                    "Not loaded: the first send triggers a load"
                });
                ui.label("(click to switch model)");
            } else {
                ui.label("Click to pick a model from the available list");
            }
        });

        // The modality, for anything but plain text.
        if let Some(model) = selected {
            let modality = ModelModality::from_model_name(model);
            if modality != ModelModality::Text {
                widgets::fixed_label(ui, MODALITY_W, text::note(modality.label()))
                    .on_hover_text(modality.tooltip());
            }
        }

        if auto_clicked {
            chat.smart_auto = !chat.smart_auto;
        }
        if let Some(name) = new_selection {
            models.selected_model = Some(name);
            // Picking a concrete model leaves Auto mode.
            chat.smart_auto = false;
        }
        ui.add_space(widgets::GAP_WIDGETS);

        // Layer mode: two pills, one seated. There is no third: a "CUDA only"
        // state would send an option no handler reads.
        use crate::state::LayerMode;
        if widgets::selector_pill(ui, "ALL LAYERS", chat.layer_mode == LayerMode::AllLayers)
            .on_hover_text("All Layers: run every layer (highest quality).")
            .clicked()
        {
            chat.layer_mode = LayerMode::AllLayers;
        }
        if widgets::selector_pill(ui, "ADAPTIVE", chat.layer_mode == LayerMode::Adaptive)
            .on_hover_text("Adaptive: exit early on high-confidence tokens (faster, slight quality drop).")
            .clicked()
        {
            chat.layer_mode = LayerMode::Adaptive;
        }

        // Clear, at the right edge. Locked while a reply streams.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let can_clear = !chat.is_generating
                && (!chat.messages.is_empty() || !chat.attached_images.is_empty());
            let clear_btn = egui::Button::image_and_text(
                Icon::Trash.image(ICON_PT_SMALL, theme::ink_dim()),
                text::note("Clear"),
            )
            .frame(false);
            let resp = ui.add_enabled(can_clear, clear_btn);
            let msg_n = chat.messages.len();
            let att_n = chat.attached_images.len();
            let is_gen = chat.is_generating;
            resp.clone().on_hover_ui(|ui| {
                let text = if can_clear {
                    format!(
                        "Clear {} message{} and {} attached file{}.\n\
                         Saved images on disk and prompt history \
                         (Ctrl+Up / Ctrl+Down) are kept.",
                        msg_n, if msg_n == 1 { "" } else { "s" },
                        att_n, if att_n == 1 { "" } else { "s" },
                    )
                } else if is_gen {
                    "Wait for the current response to finish".to_string()
                } else {
                    "Nothing to clear".to_string()
                };
                ui.label(text);
            });
            if resp.clicked() {
                chat.clear_conversation();
                // The textures keyed by msg_*, gen_* and attach_* go with the
                // messages, and so does the markdown viewer's scroll state.
                image_textures.clear();
                md_cache.clear_scrollable();
            }
        });
    });
}

/// The composer: one well holding the seed lock, the per-modality controls,
/// the attachment chips, the generating strip and the input row.
#[allow(clippy::too_many_arguments)]
fn render_input_area(
    ui: &mut egui::Ui,
    chat: &mut ChatState,
    image_textures: &mut HashMap<String, TextureHandle>,
    modality: ModelModality,
    // True when a model is selected; Send is disabled up front otherwise.
    model_selected: bool,
    // The TextEdit's row count, computed once by the caller, which also
    // reserves the output height with it.
    visible_rows: usize,
) -> bool {
    // True when the user clicked Send this frame; the caller merges it with
    // the Enter shortcut so both share one send path.
    let mut send_clicked = false;
    widgets::well(ui, |ui| {
        // The seed lock: the next send reuses this seed. The close mark clears it.
        if let Some(seed) = chat.locked_seed {
            ui.horizontal(|ui| {
                Icon::Lock.show(ui, ICON_PT_SMALL, theme::ink_dim());
                widgets::readout(ui, widgets::READOUT_WIDE_W, &format!("SEED {seed}"))
                    .on_hover_text("The next send reuses this seed");
                if widgets::close_button(ui, CHIP_CLOSE_PX)
                    .on_hover_text("Clear the seed lock: the next send uses a fresh random seed")
                    .clicked()
                {
                    chat.locked_seed = None;
                }
            });
            ui.add_space(widgets::GAP_LABEL);
        }

        // Image-gen controls. Dragging a slider commits an override; the
        // refresh mark reverts to the server's per-model default. Strength
        // only acts on the img2img path, so it is greyed without an input image.
        if modality == ModelModality::ImageGen {
            ui.horizontal(|ui| {
                ui.label(text::label("SIZE"));
                let current_label: std::borrow::Cow<str> = match chat.image_size {
                    Some((w, h)) => format!("{w}x{h}").into(),
                    None => "(default)".into(),
                };
                egui::ComboBox::from_id_salt("image_size_combo")
                    .selected_text(current_label.as_ref())
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(chat.image_size.is_none(), "(default): server picks per model")
                            .clicked()
                        {
                            chat.image_size = None;
                        }
                        for (label, w, h) in IMAGE_SIZE_PRESETS {
                            let selected = chat.image_size == Some((*w, *h));
                            if ui.selectable_label(selected, *label).clicked() {
                                chat.image_size = Some((*w, *h));
                            }
                        }
                    })
                    .response
                    .on_hover_text(
                        "Image dimensions. Higher is sharper but slower (1024 squared is \
                         about four times the work of 512 squared). Server defaults: \
                         Flux 512, Z-Image 1024.",
                    );
                ui.add_space(widgets::GAP_WIDGETS);

                ui.label(text::label("STEPS"));
                let mut steps: u32 = chat.image_num_steps.unwrap_or(IMAGE_NUM_STEPS_PLACEHOLDER);
                let changed = slider_with_readout(
                    ui,
                    &mut steps,
                    IMAGE_NUM_STEPS_MIN..=IMAGE_NUM_STEPS_MAX,
                    0,
                    "Number of denoising steps. Higher is more detail but slower. \
                     Server defaults: Flux Schnell 4, Z-Image Turbo 9.",
                );
                if changed {
                    chat.image_num_steps = Some(steps);
                }
                override_cell(ui, chat.image_num_steps.is_some());
                if chat.image_num_steps.is_some()
                    && widgets::icon_button(ui, Icon::Refresh, "Revert to the server's per-model default step count")
                        .clicked()
                {
                    chat.image_num_steps = None;
                }
                ui.add_space(widgets::GAP_WIDGETS);

                let has_input_image = !chat.attached_images.is_empty();
                ui.label(text::label("STRENGTH"));
                let mut strength: f32 = chat.image_strength.unwrap_or(IMAGE_STRENGTH_PLACEHOLDER);
                let changed = ui
                    .add_enabled_ui(has_input_image, |ui| {
                        slider_with_readout(
                            ui,
                            &mut strength,
                            IMAGE_STRENGTH_MIN..=IMAGE_STRENGTH_MAX,
                            2,
                            if has_input_image {
                                "img2img mix: 0.0 preserves the input image, 1.0 is a full \
                                 txt2img re-roll. Server chat-path default: 0.4."
                            } else {
                                "Attach an input image to enable img2img strength. Pure \
                                 txt2img ignores this control."
                            },
                        )
                    })
                    .inner;
                if changed {
                    chat.image_strength = Some(strength);
                }
                override_cell(ui, chat.image_strength.is_some());
                if chat.image_strength.is_some()
                    && widgets::icon_button(ui, Icon::Refresh, "Revert to the server's per-route default strength")
                        .clicked()
                {
                    chat.image_strength = None;
                }
            });
            ui.add_space(widgets::GAP_LABEL);
        }

        // TTS controls: the voice preset and the speed.
        if modality == ModelModality::AudioTts {
            ui.horizontal(|ui| {
                ui.label(text::label("VOICE"));
                let current_voice = chat.tts_voice.as_deref().unwrap_or("(default)");
                egui::ComboBox::from_id_salt("tts_voice_picker")
                    .selected_text(RichText::new(current_voice).size(text::SECTION_PT))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(chat.tts_voice.is_none(), "(default): server picks a neutral voice")
                            .clicked()
                        {
                            chat.tts_voice = None;
                        }
                        for v in TTS_VOICE_PRESETS {
                            let selected = chat.tts_voice.as_deref() == Some(*v);
                            if ui.selectable_label(selected, *v).clicked() {
                                chat.tts_voice = Some((*v).to_string());
                            }
                        }
                    })
                    .response
                    .on_hover_text(
                        "OpenAI-shaped voice preset. The server maps each name to a \
                         Parler-TTS voice description that nudges the model toward the \
                         requested timbre and tempo.",
                    );
                ui.add_space(widgets::GAP_WIDGETS);

                ui.label(text::label("SPEED"));
                let mut speed: f32 = chat.tts_speed.unwrap_or(TTS_SPEED_DEFAULT);
                let changed = slider_with_readout(
                    ui,
                    &mut speed,
                    TTS_SPEED_MIN..=TTS_SPEED_MAX,
                    2,
                    "Playback speed multiplier. 1.0 is the natural pace, 0.5 half speed, \
                     2.0 double speed. Applied server-side by resampling so the duration \
                     matches /v1/audio/speech.",
                );
                if changed {
                    chat.tts_speed = Some(speed);
                }
                override_cell(ui, chat.tts_speed.is_some());
                if chat.tts_speed.is_some()
                    && widgets::icon_button(ui, Icon::Refresh, "Revert to the server's default speed (1.0)")
                        .clicked()
                {
                    chat.tts_speed = None;
                }
            });
            ui.add_space(widgets::GAP_LABEL);
        }

        // Attached files. The same staging list holds images (vision) and
        // audio (ASR); a texture is decoded only for a path that looks like an
        // image, audio gets the music icon.
        if !chat.attached_images.is_empty() {
            if chat.attached_images.len() >= 2 {
                ui.horizontal(|ui| {
                    ui.label(text::note(&format!("{} attached", chat.attached_images.len())));
                    ui.add_space(widgets::GAP_LABEL);
                    if ui
                        .add(
                            egui::Button::image_and_text(
                                Icon::Cross.image(ICON_PT_SMALL, theme::ink_dim()),
                                text::note("Remove all"),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Clear all attached files (does not affect already-sent messages)")
                        .clicked()
                    {
                        chat.clear_attachments();
                        clear_attach_chip_textures(image_textures);
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                let mut remove_idx = None;
                for (i, path) in chat.attached_image_paths.iter().enumerate() {
                    let is_image = path_is_image_ext(path);
                    // Keyed by content, not by position: removing one attachment
                    // shifts the ones after it.
                    let tex_key = image_cache_key("attach", &chat.attached_images[i], i);
                    let tex_id_opt = if is_image {
                        let tex = image_textures.entry(tex_key.clone()).or_insert_with(|| {
                            load_base64_texture(ui, &chat.attached_images[i], &tex_key)
                        });
                        Some(tex.id())
                    } else {
                        None
                    };
                    // base64 length back-solved to the source size.
                    let bytes_len = (chat.attached_images[i].len() as f32 * 0.75) as u64;
                    let size_label = crate::api::types::format_size(bytes_len);
                    chip_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(tex_id) = tex_id_opt {
                                ui.image(egui::load::SizedTexture::new(
                                    tex_id,
                                    egui::vec2(CHIP_THUMB_PX, CHIP_THUMB_PX),
                                ));
                            } else {
                                Icon::Music.show(ui, text::EMPTY_STATE_ICON_PT, theme::ink_dim());
                            }
                            ui.vertical(|ui| {
                                let filename: std::borrow::Cow<str> = std::path::Path::new(path)
                                    .file_name()
                                    .map(|n| n.to_string_lossy())
                                    .unwrap_or_else(|| std::borrow::Cow::Owned(format!("attachment_{}", i)));
                                let filename_display = truncate_with_ellipsis(&filename, 32);
                                ui.label(text::note(filename_display.as_ref()))
                                    .on_hover_text(filename.as_ref());
                                ui.label(text::readout(&size_label));
                                if widgets::close_button(ui, CHIP_CLOSE_PX)
                                    .on_hover_text("Remove this attachment")
                                    .clicked()
                                {
                                    remove_idx = Some(i);
                                }
                            });
                        });
                    });
                }
                if let Some(idx) = remove_idx {
                    chat.remove_attachment(idx);
                    // Every attach_* texture goes: the indices after idx shifted.
                    clear_attach_chip_textures(image_textures);
                }
            });
            ui.add_space(widgets::GAP_LABEL);
        }

        // The generating strip. Its height is what chat_input_reserved_height
        // counts, so the row is pinned to the control height.
        if chat.is_generating {
            ui.horizontal(|ui| {
                ui.set_min_height(ui.spacing().interact_size.y);
                widgets::lamp_inline(ui, true, theme::accent());
                ui.label(text::label("GENERATING"));
            });
            ui.add_space(widgets::GAP_LABEL);
        }

        // The input row: attach, the text, the counter, Send or Stop.
        ui.horizontal(|ui| {
            // Attach is for text, vision and ASR models; image-gen, TTS and
            // video are text-only request paths.
            let attach_visible = matches!(
                modality,
                ModelModality::Text | ModelModality::Vision | ModelModality::AudioAsr
            );
            if attach_visible {
                let (tooltip, filter_label, filter_exts): (&str, &str, &[&str]) = match modality {
                    ModelModality::AudioAsr => (
                        "Attach audio file (for transcription).\n\
                         Tip: drag-and-drop also works. Drop a file anywhere on the window.",
                        "Audio",
                        CHAT_AUDIO_EXTS,
                    ),
                    _ => (
                        "Attach image (for vision models).\n\
                         Tip: drag-and-drop also works. Drop a file anywhere on the window.",
                        "Images",
                        CHAT_IMAGE_EXTS,
                    ),
                };
                // One dialog at a time: a second would overwrite the first's result.
                use std::sync::atomic::Ordering;
                let dialog_busy = chat.dialog_in_flight.load(Ordering::Relaxed);
                let attach_tip = if dialog_busy {
                    "Waiting for the open file dialog to close..."
                } else {
                    tooltip
                };
                let attach_resp = ui.add_enabled_ui(!dialog_busy, |ui| {
                    widgets::icon_button(ui, Icon::Attach, attach_tip)
                });
                if attach_resp.inner.clicked() {
                    // rfd's synchronous picker blocks the egui thread and
                    // deadlocks against the XDG portal on Linux, so it runs on a
                    // worker that also encodes the files.
                    let slot = chat.pending_dialog.clone();
                    let label = filter_label.to_string();
                    let exts: Vec<String> = filter_exts.iter().map(|s| s.to_string()).collect();
                    spawn_dialog_worker(chat.dialog_in_flight.clone(), ui.ctx().clone(), move |_| {
                        use base64::Engine;
                        let exts_ref: Vec<&str> = exts.iter().map(String::as_str).collect();
                        let Some(paths) = rfd::FileDialog::new()
                            .add_filter(&label, &exts_ref)
                            .pick_files()
                        else {
                            return;
                        };
                        let mut files: Vec<(std::path::PathBuf, String)> = Vec::new();
                        let mut oversized: Vec<(std::path::PathBuf, u64)> = Vec::new();
                        let mut unreadable: Vec<(std::path::PathBuf, String)> = Vec::new();
                        for p in paths {
                            let size = match std::fs::metadata(&p) {
                                Ok(m) => m.len(),
                                Err(e) => {
                                    unreadable.push((p, e.to_string()));
                                    continue;
                                }
                            };
                            if size > CHAT_ATTACHMENT_MAX_BYTES {
                                oversized.push((p, size));
                                continue;
                            }
                            match std::fs::read(&p) {
                                Ok(b) => {
                                    let b64 = base64::engine::general_purpose::STANDARD.encode(&b);
                                    files.push((p, b64));
                                }
                                Err(e) => unreadable.push((p, e.to_string())),
                            }
                        }
                        if !files.is_empty() || !oversized.is_empty() || !unreadable.is_empty() {
                            if let Ok(mut g) = slot.lock() {
                                *g = Some(ChatDialogResult::AttachFiles {
                                    files,
                                    oversized,
                                    unreadable,
                                });
                            }
                        }
                    });
                }
            }

            // Multi-line input. Enter sends (app.rs), Shift+Enter breaks the
            // line. It sits on the well: no frame of its own.
            let response = ui.add(
                egui::TextEdit::multiline(&mut chat.input)
                    .frame(egui::Frame::NONE)
                    .desired_width(ui.available_width() - SEND_RESERVE_W)
                    .desired_rows(visible_rows)
                    .hint_text(modality.input_hint())
                    .id_salt("chat_input")
                    .return_key(Some(egui::KeyboardShortcut::new(
                        egui::Modifiers::SHIFT,
                        egui::Key::Enter,
                    ))),
            );
            // Editing a recalled prompt detaches from history navigation.
            if response.changed() {
                chat.detach_history_if_edited();
            }
            if ui.ctx().memory(egui::Memory::focused).is_none() {
                response.request_focus();
            }

            // TTS: the character count against the server cap, live.
            if matches!(modality, ModelModality::AudioTts) {
                let chars = chat.input.chars().count();
                let over = chars > TTS_INPUT_MAX_CHARS_CLIENT;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut counter = text::readout(&format!("{} / {} chars", chars, TTS_INPUT_MAX_CHARS_CLIENT));
                    if over {
                        counter = counter.color(theme::error()).strong();
                    }
                    let resp = ui.add(egui::Label::new(counter).sense(egui::Sense::hover()));
                    if over {
                        resp.on_hover_text(
                            "TTS input exceeds the 4096-char cap. \
                             Split into multiple sends or trim before submitting.",
                        );
                    }
                });
            }

            // Send, gated by chat_send_allowed (the same helper send_chat uses),
            // or Stop while a reply streams.
            let input_empty = chat.input.trim().is_empty();
            let has_attachment = !chat.attached_images.is_empty();
            let video_gen_blocked = matches!(modality, ModelModality::VideoGen);
            let is_asr = matches!(modality, ModelModality::AudioAsr);
            let asr_blocked = is_asr && !has_attachment;
            let text_blocked = !is_asr && !video_gen_blocked && input_empty;
            let send_enabled = model_selected
                && !chat.is_generating
                && chat_send_allowed(modality, input_empty, has_attachment);
            let send_tip = if !model_selected {
                "Pick a model from the dropdown above (or open the \
                 Models tab to import one) before sending."
            } else if video_gen_blocked {
                "Video generation isn't implemented server-side yet.\n\
                 Switch to an image-gen, TTS, or text model to send."
            } else if chat.is_generating {
                "Wait for the current response to finish, then Send again."
            } else if asr_blocked {
                "Attach an audio file (WAV / MP3 / FLAC / OGG / M4A / AAC) \
                 above to enable transcription."
            } else if text_blocked {
                "Type a message in the input above to enable Send."
            } else {
                "Send message\n\n\
                 Keyboard:\n  \
                   Enter: send\n  \
                   Shift+Enter: newline\n  \
                   Ctrl+Up / Ctrl+Down: recall prompt history\n  \
                   Ctrl+L: clear conversation\n  \
                   Esc: cancel in-flight generation"
            };
            if chat.is_generating {
                let stop_btn = egui::Button::image_and_text(
                    Icon::Stop.image(ICON_PT_SMALL, theme::on_accent()),
                    text::value("Stop").color(theme::on_accent()),
                )
                .fill(theme::error());
                if ui
                    .add(stop_btn)
                    .on_hover_text(
                        "Cancel the in-flight generation (or press Esc).\n\
                         Already-streamed content is kept; \
                         the next chunk after abort is dropped.",
                    )
                    .clicked()
                    && chat.abort_generation()
                {
                    chat.messages.push_back(ChatMessage::system("[Generation cancelled by user]"));
                }
            } else if send_enabled {
                let send_btn = egui::Button::image_and_text(
                    Icon::Play.image(ICON_PT_SMALL, theme::on_accent()),
                    text::value("Send").color(theme::on_accent()),
                )
                .fill(theme::accent());
                send_clicked = ui.add(send_btn).on_hover_text(send_tip).clicked();
            } else {
                ui.add_enabled(
                    false,
                    egui::Button::image_and_text(Icon::Play.image(ICON_PT_SMALL, theme::ink_dim()), "Send"),
                )
                .on_hover_text(send_tip);
            }
        });
    });
    send_clicked
}

/// Width kept to the right of the input for the counter and the Send button.
const SEND_RESERVE_W: f32 = 70.0;

/// A slider of fixed track width followed by its value in a fixed monospace
/// cell. Returns whether either changed the value.
fn slider_with_readout<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    decimals: usize,
    tip: &str,
) -> bool {
    let slider = ui.scope(|ui| {
        ui.spacing_mut().slider_width = widgets::SLIDER_TRACK_W;
        ui.add(egui::Slider::new(value, range.clone()).show_value(false))
    })
    .inner
    .on_hover_text(tip);
    let drag = ui.scope(|ui| {
        ui.style_mut().override_font_id = Some(egui::FontId::monospace(text::VALUE_PT));
        widgets::drag_fixed(
            ui,
            egui::DragValue::new(value).range(range).fixed_decimals(decimals),
            widgets::READOUT_W,
        )
    })
    .inner;
    slider.changed() || drag.changed()
}

/// Says whether a control overrides the server's default, in a cell that
/// keeps its width either way.
fn override_cell(ui: &mut egui::Ui, overridden: bool) {
    widgets::fixed_label(
        ui,
        OVERRIDE_W,
        text::label(if overridden { "OVERRIDE" } else { "DEFAULT" }),
    );
}

/// The frame of an attachment chip: raised, with a hairline.
fn chip_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(theme::raised())
        .stroke(egui::Stroke::new(HAIRLINE, theme::border()))
        .corner_radius(theme::RADIUS)
        .inner_margin(widgets::GAP_LABEL as i8)
}

/// The shell of a message row: a full-width frame in the role's fill, a
/// stripe in the role's tint, the role in capitals and the timestamp on the
/// first line. `body` draws the rest.
fn message_row(ui: &mut egui::Ui, fill: Color32, tint: Color32, role_caps: &str, timestamp: &str, body: impl FnOnce(&mut egui::Ui)) {
    let row = egui::Frame::NONE
        .fill(fill)
        .corner_radius(theme::RADIUS)
        .inner_margin(egui::Margin::symmetric(MESSAGE_PAD_X, MESSAGE_PAD_Y))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(text::label(role_caps));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(text::readout(timestamp));
                });
            });
            ui.add_space(widgets::GAP_LABEL);
            body(ui);
        });
    widgets::stripe(ui, row.response.rect, tint);
}

/// Padding inside a message row.
const MESSAGE_PAD_X: i8 = 12;
const MESSAGE_PAD_Y: i8 = 8;

/// One message. The role decides the fill and the tint of the stripe: the
/// assistant on the panel in the accent, the user raised in dim ink, a system
/// notice in the warning colour, a system error in the error colour.
#[allow(clippy::too_many_arguments)]
fn render_message(
    ui: &mut egui::Ui,
    msg: &ChatMessage,
    md_cache: &mut CommonMarkCache,
    image_textures: &mut HashMap<String, TextureHandle>,
    pending_dialog: &std::sync::Arc<std::sync::Mutex<Option<ChatDialogResult>>>,
    dialog_in_flight: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    // Single-shot seed-lock slot, drained by render() into ChatState.
    seed_lock_request: &std::cell::Cell<Option<u64>>,
    // Single-shot playback-error slot, drained by render() into a system message.
    play_error_pending: &std::cell::Cell<Option<String>>,
) {
    let is_user = msg.role == "user";
    let is_system = msg.role == "system";
    let is_error = is_system && is_error_system_message(&msg.content);
    let (fill, tint, role_caps) = if is_user {
        (theme::raised(), theme::ink_dim(), "YOU")
    } else if is_error {
        (theme::panel(), theme::error(), "ERROR")
    } else if is_system {
        (theme::panel(), theme::warning(), "SYSTEM")
    } else {
        (theme::panel(), theme::accent(), "ASSISTANT")
    };
    let text_color = theme::ink();

    message_row(ui, fill, tint, role_caps, &msg.timestamp, |ui| {
        // Attached files: images open in the viewer as one set, audio gets a chip.
        if !msg.images.is_empty() {
            let mut img_texes: Vec<egui::TextureHandle> = Vec::new();
            for (i, b64) in msg.images.iter().enumerate() {
                if !base64_looks_like_image(b64) {
                    continue;
                }
                let key = image_cache_key("msg", b64, i);
                let tex = image_textures
                    .entry(key.clone())
                    .or_insert_with(|| load_base64_texture(ui, b64, &key));
                img_texes.push(tex.clone());
            }
            let mut img_ord = 0usize;
            ui.horizontal_wrapped(|ui| {
                for (i, img_b64) in msg.images.iter().enumerate() {
                    if base64_looks_like_image(img_b64) {
                        let tex_key = image_cache_key("msg", img_b64, i);
                        let tex = image_textures.entry(tex_key.clone()).or_insert_with(|| {
                            load_base64_texture(ui, img_b64, &tex_key)
                        });
                        let tex_size = tex.size_vec2();
                        let scale = (MSG_IMAGE_MAX_PX / tex_size.x.max(tex_size.y)).min(1.0);
                        crate::image_viewer::clickable_image_in_set(ui, &img_texes, img_ord, tex_size * scale);
                        img_ord += 1;
                    } else {
                        let bytes_len = (img_b64.len() as f32 * 0.75) as u64;
                        let size_label = crate::api::types::format_size(bytes_len);
                        chip_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                Icon::Music.show(ui, text::EMPTY_STATE_ICON_PT, theme::ink_dim());
                                ui.vertical(|ui| {
                                    ui.label(text::label("AUDIO"));
                                    ui.label(text::readout(&size_label));
                                });
                            });
                        });
                    }
                }
            });
            ui.add_space(widgets::GAP_LABEL);
        }

        // Image-gen replies begin with `[seed: <u64>]`; the seed becomes a
        // readout with Copy and Lock, and the body loses the prefix.
        let (extracted_seed, content_without_seed) = if is_user || is_system {
            (None, msg.content.as_str())
        } else {
            parse_seed_prefix(&msg.content)
        };
        if let Some(seed) = extracted_seed {
            ui.horizontal(|ui| {
                widgets::readout(ui, widgets::READOUT_WIDE_W, &format!("seed {seed}"));
                if ui
                    .add(
                        egui::Button::image_and_text(
                            Icon::Copy.image(ICON_PT_SMALL, theme::ink_dim()),
                            text::note("Copy"),
                        )
                        .frame(false),
                    )
                    .on_hover_text("Copy the seed: paste it into the next prompt's options.seed to re-roll this exact generation")
                    .clicked()
                {
                    ui.ctx().copy_text(seed.to_string());
                }
                if ui
                    .add(
                        egui::Button::image_and_text(
                            Icon::Lock.image(ICON_PT_SMALL, theme::ink_dim()),
                            text::note("Lock seed"),
                        )
                        .frame(false),
                    )
                    .on_hover_text("Lock this seed for the next send: the same starting noise under a different prompt")
                    .clicked()
                {
                    seed_lock_request.set(Some(seed));
                }
            });
        }
        let body_to_render = if extracted_seed.is_some() {
            content_without_seed
        } else {
            msg.content.as_str()
        };
        // Markdown for the assistant; a selectable label for the user and the
        // system, so a past prompt can be copied by selection.
        if !body_to_render.is_empty() {
            if msg.role == "assistant" {
                CommonMarkViewer::new().show(ui, md_cache, body_to_render);
            } else {
                ui.add(
                    egui::Label::new(RichText::new(body_to_render).color(text_color))
                        .selectable(true),
                );
            }
        }

        // Generated audio: Play and Save per clip.
        if !msg.generated_audios.is_empty() {
            ui.add_space(widgets::GAP_WIDGETS);
            for (i, wav_b64) in msg.generated_audios.iter().enumerate() {
                ui.horizontal(|ui| {
                    Icon::Speaker.show(ui, ICON_PT_SMALL, theme::ink_dim());
                    let label_owned;
                    let label: &str = if msg.generated_audios.len() == 1 {
                        "Audio response"
                    } else {
                        label_owned = format!("Audio {} of {}", i + 1, msg.generated_audios.len());
                        &label_owned
                    };
                    ui.label(text::note(label));
                    let play_btn = egui::Button::image_and_text(
                        Icon::Play.image(ICON_PT_SMALL, theme::ink()),
                        text::note("Play"),
                    );
                    if ui.add(play_btn).clicked() {
                        if let Err(e) = play_audio_blob(wav_b64) {
                            tracing::warn!("audio playback failed: {e}");
                            // "Failed" is the prefix is_error_system_message reads.
                            play_error_pending.set(Some(format!(
                                "Failed to play audio: {e}. \
                                 Use the Save button and play the WAV in an external app."
                            )));
                        }
                    }
                    if save_button(ui, dialog_in_flight, "Save").clicked() {
                        use base64::Engine;
                        let bytes = base64::engine::general_purpose::STANDARD
                            .decode(wav_b64)
                            .unwrap_or_default();
                        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                        let default_name = format!("tts_{ts}.wav");
                        spawn_save(
                            ui.ctx().clone(),
                            pending_dialog.clone(),
                            dialog_in_flight.clone(),
                            default_name,
                            &["wav"],
                            "WAV audio",
                            bytes,
                        );
                    }
                });
            }
        }

        // Generated images, with Save all above a set.
        if !msg.generated_images.is_empty() {
            ui.add_space(widgets::GAP_WIDGETS);
            if msg.generated_images.len() > 1 {
                ui.horizontal(|ui| {
                    widgets::readout(ui, widgets::READOUT_W, &format!("{} images", msg.generated_images.len()));
                    use std::sync::atomic::Ordering;
                    let dialog_busy = dialog_in_flight.load(Ordering::Relaxed);
                    let save_all_resp = ui.add_enabled(!dialog_busy, egui::Button::new(text::note("Save all")));
                    let save_all_tip = if dialog_busy {
                        "Waiting for the open file dialog to close..."
                    } else {
                        "Save all images to a folder"
                    };
                    if save_all_resp.on_hover_text(save_all_tip).clicked() {
                        use base64::Engine;
                        let files: Vec<(String, Vec<u8>)> = msg
                            .generated_images
                            .iter()
                            .enumerate()
                            .filter_map(|(j, b64)| {
                                base64::engine::general_purpose::STANDARD
                                    .decode(b64)
                                    .ok()
                                    .map(|bytes| (format!("image_{}_{}.png", msg.timestamp, j), bytes))
                            })
                            .collect();
                        let slot = pending_dialog.clone();
                        spawn_dialog_worker(dialog_in_flight.clone(), ui.ctx().clone(), move |_| {
                            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                if let Ok(mut g) = slot.lock() {
                                    *g = Some(ChatDialogResult::SaveBytesMany { dir, files });
                                }
                            }
                        });
                    }
                });
                ui.add_space(widgets::GAP_LABEL);
            }
            for (i, img_b64) in msg.generated_images.iter().enumerate() {
                let tex_key = image_cache_key("gen", img_b64, i);
                let tex = image_textures.entry(tex_key.clone()).or_insert_with(|| {
                    load_base64_texture(ui, img_b64, &tex_key)
                });
                let tex_handle = tex.clone();
                let tex_size = tex.size_vec2();
                let max_width = ui.available_width() * GEN_IMAGE_MAX_FRAC;
                let scale = (max_width / tex_size.x).min(1.0);
                crate::image_viewer::clickable_image(ui, &tex_handle, tex_size * scale);
                ui.horizontal(|ui| {
                    widgets::readout(ui, widgets::READOUT_WIDE_W, &format!("{}x{}", tex_size.x as u32, tex_size.y as u32));
                    use std::sync::atomic::Ordering;
                    let dialog_busy = dialog_in_flight.load(Ordering::Relaxed);
                    let save_resp = ui.add_enabled(!dialog_busy, egui::Button::new(text::note("Save")));
                    let save_tip = if dialog_busy {
                        "Waiting for the open file dialog to close..."
                    } else {
                        "Save image to disk"
                    };
                    if save_resp.on_hover_text(save_tip).clicked() {
                        use base64::Engine;
                        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(img_b64) {
                            let default_name = format!("image_{}_{}.png", msg.timestamp, i);
                            spawn_save(
                                ui.ctx().clone(),
                                pending_dialog.clone(),
                                dialog_in_flight.clone(),
                                default_name,
                                &["png"],
                                "PNG image",
                                bytes,
                            );
                        }
                    }
                    if ui
                        .add(
                            egui::Button::image_and_text(
                                Icon::Copy.image(ICON_PT_SMALL, theme::ink_dim()),
                                text::note("Copy b64"),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Copy the base64 string to the clipboard")
                        .clicked()
                    {
                        ui.ctx().copy_text(img_b64.clone());
                    }
                });
                ui.add_space(widgets::GAP_LABEL);
            }
        }

        // The footer of an assistant row: the timing readout on the left,
        // Copy on the right. Image-gen and TTS replies carry no tokens, and
        // format_timing_line collapses to the duration for them.
        if !is_user && !is_system && (!msg.content.is_empty() || msg.timing.is_some()) {
            ui.add_space(widgets::GAP_LABEL);
            ui.horizontal(|ui| {
                if let Some(timing) = msg.timing {
                    ui.label(text::readout(&format_timing_line(timing)));
                }
                if !msg.content.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::image_and_text(
                                    Icon::Copy.image(ICON_PT_SMALL, theme::ink_dim()),
                                    text::note("Copy"),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Copy the response text to the clipboard")
                            .clicked()
                        {
                            ui.ctx().copy_text(msg.content.clone());
                        }
                    });
                }
            });
        }
    });
}

/// Longest side of an attached image in a message, and the share of the row
/// a generated image may take.
const MSG_IMAGE_MAX_PX: f32 = 200.0;
const GEN_IMAGE_MAX_FRAC: f32 = 0.9;

/// The reply being streamed: an assistant row with a lamp. An image reply
/// shows its step and a meter; a text reply shows the markdown so far and a
/// painted caret.
fn render_streaming_message(
    ui: &mut egui::Ui,
    content: &str,
    image_progress: &Option<(usize, usize)>,
    image_started_at: Option<std::time::Instant>,
    md_cache: &mut CommonMarkCache,
) {
    message_row(ui, theme::panel(), theme::accent(), "ASSISTANT", "", |ui| {
        ui.horizontal(|ui| {
            widgets::lamp_inline(ui, true, theme::accent());
            ui.label(text::label("GENERATING"));
            if let Some((completed, total)) = image_progress {
                // ETA from the mean step so far. Step c is in progress, so c - 1
                // steps are complete; the estimate needs at least one.
                let eta = match (image_started_at, *completed) {
                    (Some(start), c) if c >= 2 && c <= *total => {
                        let per_step = start.elapsed().as_secs_f32() / (c - 1) as f32;
                        let remaining = per_step * (*total - c + 1) as f32;
                        format!(" ~{} left", ModelModality::format_eta_remaining(remaining))
                    }
                    _ => String::new(),
                };
                ui.label(text::readout(&format!("step {}/{}{}", completed, total, eta)));
            }
        });
        if let Some((completed, total)) = image_progress {
            ui.add_space(widgets::GAP_LABEL);
            let progress = *completed as f32 / (*total).max(1) as f32;
            surface::meter(ui, progress, widgets::METER_SIZE, theme::accent());
        } else {
            ui.add_space(widgets::GAP_LABEL);
            CommonMarkViewer::new().show(ui, md_cache, content);
            // Painted: U+2588 FULL BLOCK is not in the bundled fonts.
            let (caret, _) = ui.allocate_exact_size(CARET_SIZE, egui::Sense::hover());
            ui.painter()
                .rect_filled(caret.shrink2(egui::vec2(1.0, 1.0)), 1.0, theme::accent());
        }
    });
}

/// The wait before the first chunk: a lamp and what is being waited for. A
/// selected model that is not loaded pays its load on the first send, which
/// is named rather than shown as a generic wait.
fn render_typing_indicator(ui: &mut egui::Ui, modality: ModelModality, loading_model: Option<&str>) {
    let loading_label = loading_model
        .map(|m| format!("Loading {} into memory...", truncate_with_ellipsis(m, MODEL_CHARS)));
    let label_text: &str = loading_label.as_deref().unwrap_or(modality.typing_label());
    ui.horizontal(|ui| {
        widgets::lamp_inline(ui, true, theme::accent());
        ui.label(text::note(label_text));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reservation below the message list must grow while a reply is streaming. It did
    /// not, so the "Generating..." strip pushed the input row and the Send button past the
    /// bottom edge of the window - at every window height, for the whole of every answer.
    #[test]
    fn the_composer_reserves_room_for_the_generating_strip() {
        fn reserved(is_generating: bool) -> f32 {
            let out = std::rc::Rc::new(std::cell::Cell::new(0.0));
            let sink = out.clone();
            let mut harness = egui_kittest::Harness::new_ui(move |ui| {
                sink.set(chat_input_reserved_height(ui, 1, false, is_generating));
            });
            harness.run();
            out.get()
        }
        let idle = reserved(false);
        let streaming = reserved(true);
        assert!(
            streaming > idle,
            "streaming reserves {streaming}, idle reserves {idle}"
        );
        // One interactive row plus its spacing, so the gap is real and not a rounding
        // difference between two ways of adding the same numbers.
        assert!(streaming - idle >= 16.0, "gap {}", streaming - idle);
    }

    /// A message row names its role in text, so the role does not rest on the
    /// colour of a stripe alone.
    #[test]
    fn a_message_row_names_its_role_in_text() {
        use egui_kittest::kittest::NodeT;
        let mut user = ChatMessage::system("hello");
        user.role = "user".to_string();
        let mut assistant = ChatMessage::system("world");
        assistant.role = "assistant".to_string();
        let messages = [user, assistant];
        let mut md_cache = CommonMarkCache::default();
        let mut textures = HashMap::new();
        let pending = std::sync::Arc::new(std::sync::Mutex::new(None));
        let in_flight = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seed = std::cell::Cell::new(None);
        let play_err = std::cell::Cell::new(None);
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            for msg in &messages {
                render_message(ui, msg, &mut md_cache, &mut textures, &pending, &in_flight, &seed, &play_err);
            }
        });
        harness.run();
        let labels: Vec<String> = harness
            .root()
            .children_recursive()
            .map(|n| {
                let ak = n.accesskit_node();
                format!("{}{}", ak.label().unwrap_or_default(), ak.value().unwrap_or_default())
            })
            .collect();
        assert!(labels.iter().any(|l| l == "YOU"), "{labels:?}");
        assert!(labels.iter().any(|l| l == "ASSISTANT"), "{labels:?}");
    }
}
