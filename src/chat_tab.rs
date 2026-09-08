//! Chat Tab Rendering
//!
//! Professional UI rendering for the chat interface with message bubbles.
//! Theme-aware: adapts to dark and light modes using theme constants.
//!
//! The pure helpers live beside this rather than in it: `crate::modality`
//! (ModelModality, helpers and constants), `crate::texture` (the texture cache),
//! `crate::audio_playback` (playback), `crate::dialog` (file dialogs on a worker
//! thread).

use eframe::egui;
use egui::{Color32, CornerRadius, RichText, Stroke, TextureHandle};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::audio_playback::play_audio_blob;
use crate::dialog::{save_button, spawn_dialog_worker, spawn_save};
use crate::icons::Icon;
use crate::modality::{
    base64_looks_like_image, chat_input_visible_rows, chat_send_allowed,
    format_attachment_cap_mb, format_timing_line, is_error_system_message,
    modality_badge_color, parse_seed_prefix, path_is_image_ext, truncate_with_ellipsis,
    ModelModality, CHAT_ATTACHMENT_MAX_BYTES, CHAT_AUDIO_EXTS, CHAT_IMAGE_EXTS,
    CHAT_INPUT_ATTACH_STRIP_PX, CHAT_INPUT_BASE_CHROME_PX, CHAT_INPUT_ROW_PX,
    IMAGE_NUM_STEPS_MAX, IMAGE_NUM_STEPS_MIN, IMAGE_NUM_STEPS_PLACEHOLDER,
    IMAGE_SIZE_PRESETS, IMAGE_STRENGTH_MAX, IMAGE_STRENGTH_MIN, IMAGE_STRENGTH_PLACEHOLDER,
    TTS_INPUT_MAX_CHARS_CLIENT, TTS_SPEED_DEFAULT, TTS_SPEED_MAX, TTS_SPEED_MIN,
    TTS_VOICE_PRESETS,
};
use crate::state::{ChatDialogResult, ChatMessage, ChatState, ModelState};
use crate::texture::{clear_attach_chip_textures, image_cache_key, load_base64_texture};
use crate::theme;

use std::collections::HashMap;

// ── Theme helpers ──

struct ChatTheme {
    text_primary: Color32,
    text_secondary: Color32,
    text_muted: Color32,
    surface: Color32,
    surface_elevated: Color32,
    border: Color32,
}

impl ChatTheme {
    fn from_mode(dark: bool) -> Self {
        if dark {
            Self {
                text_primary: theme::ink(),
                text_secondary: theme::ink_dim(),
                text_muted: theme::ink_dim(),
                surface: theme::panel(),
                surface_elevated: theme::raised(),
                border: theme::border(),
            }
        } else {
            Self {
                text_primary: theme::ink(),
                text_secondary: theme::ink_dim(),
                text_muted: theme::ink_dim(),
                surface: theme::panel(),
                surface_elevated: theme::raised(),
                border: theme::border(),
            }
        }
    }

    /// Bubble colors for a message role.
    ///
    /// `error_flag` upgrades a 'system' role from neutral warning yellow
    /// to red — server errors (content beginning with "Error:") need to
    /// pop out from benign info messages ("No model selected", etc.).
    fn bubble(&self, role: &str, dark: bool, error_flag: bool) -> (Color32, Color32, Color32) {
        // (fill, stroke, text_color)
        match role {
            "user" => if dark {
                (self.surface, theme::success(), self.text_primary)
            } else {
                (Color32::from_rgb(232, 245, 233), theme::success(), self.text_primary)
            },
            "system" if error_flag => if dark {
                (self.surface, theme::error(), self.text_primary)
            } else {
                (Color32::from_rgb(253, 232, 232), theme::error(), self.text_primary)
            },
            "system" => if dark {
                (self.surface, theme::warning(), self.text_primary)
            } else {
                (Color32::from_rgb(255, 248, 225), theme::warning(), self.text_primary)
            },
            _ => if dark {
                (self.surface, theme::accent(), self.text_primary)
            } else {
                (Color32::from_rgb(232, 240, 254), theme::accent(), self.text_primary)
            },
        }
    }

    /// Avatar badge colours and icon for a role. `error_flag`
    /// upgrades the system avatar to a red "Error" badge so a system
    /// message reads as a notice or a failure at a glance.
    ///
    /// The icon is an SVG, not a character. The geometric shapes
    /// used here before were chosen to avoid emoji presentation and
    /// drew as missing-glyph boxes instead, which is worse than the
    /// problem they were avoiding.
    fn avatar(&self, role: &str, error_flag: bool) -> (Color32, Icon, &'static str) {
        // (bg, icon, label)
        match role {
            "user" => (
                theme::tinted(theme::success(), 180),
                Icon::User, "You",
            ),
            "system" if error_flag => (
                theme::tinted(theme::error(), 200),
                Icon::Cross, "Error",
            ),
            "system" => (
                theme::tinted(theme::warning(), 180),
                Icon::Warning, "System",
            ),
            _ => (
                theme::tinted(theme::accent(), 180),
                Icon::Bot, "AI",
            ),
        }
    }
}

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
    let dark = ui.visuals().dark_mode;
    let t = ChatTheme::from_mode(dark);

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
        painter.rect_filled(
            screen,
            0.0,
            theme::tinted(theme::accent(), 40),
        );
        let label = if hovered_count == 1 {
            "Drop file to attach".to_string()
        } else {
            format!("Drop {} files to attach", hovered_count)
        };
        painter.text(
            screen.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(24.0),
            Color32::WHITE,
        );
    }

    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        // Header card
        render_chat_header(ui, chat, selected_profile_name, profiles, models, &t, image_textures, md_cache);

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
                    egui::Frame {
                        inner_margin: egui::Margin::symmetric(10, 6),
                        corner_radius: CornerRadius::same(4),
                        fill: theme::tinted(theme::warning(), 22),
                        stroke: Stroke::new(1.0, theme::warning()),
                        ..Default::default()
                    }
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(theme::ICON_EMPTY).size(11.0).color(theme::warning()));
                            ui.label(
                                RichText::new("Model not loaded — the first response will be slower while it loads into memory.")
                                    .size(11.0)
                                    .color(t.text_secondary),
                            );
                        });
                    });
                    ui.add_space(8.0);
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
        let output_height = (ui.available_height() - input_height).max(60.0);
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .max_height(output_height)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

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
                        ui.add_space(40.0);
                        icon.show(ui, 32.0, t.text_muted);
                        ui.add_space(8.0);
                        ui.label(RichText::new(title).size(15.0).strong().color(t.text_primary));
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(subtitle)
                                .size(12.0)
                                .color(t.text_secondary),
                        );
                        if !prompts.is_empty() {
                            ui.add_space(16.0);
                            ui.label(RichText::new("Try one of these:").size(11.0).color(t.text_muted));
                            ui.add_space(4.0);
                            for prompt in prompts {
                                // Pill-shaped suggestion: click to drop the
                                // text into the input box. Keeps the empty
                                // state useful without committing to a full
                                // prompt-template library.
                                if ui
                                    .small_button(RichText::new(*prompt).size(11.0))
                                    .on_hover_text("Click to use this prompt")
                                    .clicked()
                                {
                                    chat.input = (*prompt).to_string();
                                }
                                ui.add_space(2.0);
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
                            ui, msg, &t, md_cache, image_textures,
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
                            render_streaming_message(ui, &chat.streaming_content, &chat.image_gen_progress, chat.image_gen_started_at, &t, md_cache);
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
                            render_typing_indicator(ui, &t, modality, loading_model);
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
            ui, chat, &t, image_textures, modality, model_selected,
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

/// Render chat header bar
// Wide signature carries the chat state, profile palette, theme, and
// the GPU texture map (needed so the Clear button can release msg_*
// and gen_* image textures alongside the message vec — otherwise the
// HashMap grows unbounded). Refactoring to a struct adds boilerplate.
#[allow(clippy::too_many_arguments)]
fn render_chat_header(
    ui: &mut egui::Ui,
    chat: &mut ChatState,
    selected_profile_name: Option<&String>,
    profiles: &[crate::config::ApiProfile],
    models: &mut ModelState,
    t: &ChatTheme,
    image_textures: &mut HashMap<String, TextureHandle>,
    md_cache: &mut CommonMarkCache,
) {
    egui::Frame {
        inner_margin: egui::Margin::symmetric(14, 10),
        corner_radius: CornerRadius::same(8),
        fill: t.surface,
        stroke: Stroke::new(1.0, t.border),
        shadow: egui::epaint::Shadow {
            offset: [0, 1],
            blur: 3,
            spread: 0,
            color: Color32::from_black_alpha(if theme::is_dark() { 30 } else { 8 }),
        },
        ..Default::default()
    }
    .show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            // Icon and title
            Icon::Chat.show(ui, 16.0, t.text_primary);
            ui.add_space(4.0);
            ui.label(RichText::new("Chat").size(15.0).strong().color(t.text_primary));

            ui.separator();

            // Profile badge — tooltip reveals the API type + server URL
            // so users can verify they're hitting the right endpoint
            // without opening Settings. When no profile is selected
            // (first run, profiles list empty, or user reset) render
            // a muted "No profile" badge with a guidance tooltip so
            // the user knows where to set one up — otherwise the chat
            // header silently omits the badge and there's no clue.
            if let Some(profile_name) = selected_profile_name {
                if let Some(profile) = profiles.iter().find(|p| p.name == *profile_name) {
                    let badge_resp = egui::Frame {
                        inner_margin: egui::Margin::symmetric(8, 3),
                        corner_radius: CornerRadius::same(4),
                        fill: theme::tinted(theme::accent(), 25),
                        ..Default::default()
                    }
                    .show(ui, |ui| {
                        // Cap the visible profile name in the badge so a
                        // long user-typed name can't push the rest of the
                        // chat header (model dropdown, layer toggle,
                        // clear button) off the right edge of the
                        // window. Full name stays in the tooltip below.
                        let label = truncate_with_ellipsis(&profile.name, 20);
                        ui.label(RichText::new(label.as_ref()).size(11.0).color(theme::accent()));
                    });
                    let api_kind = match profile.api_type {
                        crate::config::ApiType::Loken => "LOKEN",
                        crate::config::ApiType::Ollama    => "Ollama",
                        crate::config::ApiType::OpenApi   => "OpenAPI",
                    };
                    // on_hover_ui's closure body only runs when the
                    // tooltip is actually being shown — the previous
                    // on_hover_text(format!(...)) ran the format! on
                    // every frame (60 Hz on the chat tab, the most-
                    // rendered surface) regardless of whether the
                    // user was hovering the badge. Three String args
                    // per format = ~180 short-lived heap allocs/sec
                    // for an effect the user sees on hover only.
                    badge_resp.response.on_hover_ui(|ui| {
                        ui.label(format!(
                            "Profile: {}\nAPI: {}\nServer: {}",
                            profile.name, api_kind, profile.server_url,
                        ));
                    });
                }
            } else {
                let badge_resp = egui::Frame {
                    inner_margin: egui::Margin::symmetric(8, 3),
                    corner_radius: CornerRadius::same(4),
                    fill: theme::tinted(theme::warning(), 25),
                    ..Default::default()
                }
                .show(ui, |ui| {
                    ui.label(RichText::new("No profile").size(11.0).color(theme::warning()));
                });
                badge_resp.response.on_hover_text(
                    "No API profile selected.\n\
                     Open the Settings tab and pick (or create) a profile under\n\
                     \"API Profiles\" to define which server + parameters this\n\
                     chat uses.",
                );
            }

            ui.separator();

            // Model picker — turns the previous static load-status
            // badge into a real dropdown so the user can switch
            // models from the chat tab without bouncing to Models
            // (the most-frequent friction in multi-model workflows).
            //
            // Selected text mirrors the badge layout: load-dot glyph then the
            // (truncated) name, tinted by load state - green for loaded, amber for
            // not yet, where the first send triggers the load.
            // Borrowed, not cloned: a copy of selected_model on every chat-header
            // paint allocates once per frame to read a value back. The deferred
            // `new_selection` path below means selected_model is never mutated while
            // this borrow is live.
            let selected: Option<&String> = models.selected_model.as_ref();
            let (current_is_loaded, current_color, current_icon) = match selected.map(String::as_str) {
                _ if chat.smart_auto => (true, theme::success(), theme::ICON_FILLED),
                Some(m) => {
                    let loaded = models.is_loaded(m);
                    if loaded { (true,  theme::success(), theme::ICON_FILLED) }
                    else      { (false, theme::warning(), theme::ICON_EMPTY)  }
                }
                None => (false, t.text_muted, theme::ICON_EMPTY),
            };
            let selected_text = if chat.smart_auto {
                format!("{} Auto (smart routing)", current_icon)
            } else {
                match selected.map(String::as_str) {
                    Some(m) => format!("{} {}", current_icon, truncate_with_ellipsis(m, 25)),
                    None    => format!("{} No model selected", current_icon),
                }
            };
            // ComboBox styles itself via egui defaults; the
            // selected_text colour signals load state at-a-glance.
            //
            // Disable picking while a generation is in flight — the
            // request was dispatched against `selected_model` at
            // send_chat time, so silently swapping the selection
            // mid-stream would leave the response attributed to the
            // wrong model and confuse downstream chunk handlers
            // (which look at `selected_model` for modality routing).
            // Stop button / Esc still abort; user can switch once
            // the chat is idle again.
            // Defer the click-driven selection write so we can hold
            // immutable borrows on models.available_models (via
            // sorted/by_modality) without conflicting with the
            // models.selected_model assignment. Applied after the
            // ComboBox closure ends.
            let mut new_selection: Option<String> = None;
            let mut auto_clicked = false;
            let combo_resp = ui.add_enabled_ui(!chat.is_generating, |ui| {
                egui::ComboBox::from_id_salt("chat_model_picker")
                    .selected_text(
                        RichText::new(&selected_text).size(11.0).color(current_color),
                    )
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        // Smart routing: the server routes each prompt to the right
                        // model (chat / vision / image / speech) via /conversation.
                        let auto_row = ui.selectable_label(
                            chat.smart_auto,
                            RichText::new("Auto (smart routing)").size(11.0),
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
                            ui.label(
                                RichText::new("No models available — open Models tab to import")
                                    .size(11.0)
                                    .color(t.text_muted),
                            );
                            return;
                        }
                        // Sort + group by modality for a scannable list.
                        // Groups appear in this order so the user's most-
                        // likely target is near the top.
                        const GROUPS: &[(ModelModality, &str)] = &[
                            (ModelModality::Text,     "Text"),
                            (ModelModality::Vision,   "Vision"),
                            (ModelModality::ImageGen, "Image"),
                            (ModelModality::AudioTts, "TTS"),
                            (ModelModality::AudioAsr, "ASR"),
                            (ModelModality::VideoGen, "Video"),
                        ];
                        // Sort by reference — no need to clone the full
                        // ModelInfo (4 Strings × N models) just to drive
                        // a read-only popup. Iteration order of the
                        // resulting Vec<&ModelInfo> matches what the
                        // previous clone-then-sort produced.
                        let mut sorted: Vec<&crate::api::ModelInfo> =
                            models.available_models.iter().collect();
                        sorted.sort_by(|a, b| a.name.cmp(&b.name));
                        // Cap the popup at 360 px tall — without this,
                        // a user with 50+ installed models (HF cache
                        // accumulates quickly) gets a popup taller
                        // than the screen that either gets clipped or
                        // pushes the chat tab off the visible area.
                        // ScrollArea wraps the inner rows so the
                        // popup stays bounded and the user can scroll
                        // through long catalogs cleanly.
                        egui::ScrollArea::vertical()
                            .max_height(360.0)
                            .show(ui, |ui| {
                                // Bucket each model into its modality
                                // group in a single pass instead of
                                // doing 6 filter passes over `sorted`
                                // (one per group label). For a catalog
                                // of ~50 installed models that's 300
                                // modality-classifier calls per popup
                                // open vs 50 here. Also drops the
                                // intermediate Vec<&ModelInfo> the
                                // previous code allocated per group.
                                use std::collections::HashMap;
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
                                    ui.label(
                                        RichText::new(*group_label)
                                            .size(10.0)
                                            .strong()
                                            .color(t.text_secondary),
                                    );
                                    for m in in_group {
                                        let is_selected = selected.map(String::as_str) == Some(m.name.as_str());
                                        let is_loaded = models.is_loaded(&m.name);
                                        let row_icon = if is_loaded { theme::ICON_FILLED } else { theme::ICON_EMPTY };
                                        let row_color = if is_loaded { theme::success() } else { t.text_primary };
                                        let label = format!("{} {}", row_icon, m.name);
                                        if ui.selectable_label(is_selected, RichText::new(label).size(11.0).color(row_color)).clicked() {
                                            new_selection = Some(m.name.clone());
                                        }
                                    }
                                    ui.add_space(2.0);
                                }
                            });
                    })
            }).inner;
            // Tooltip on the closed combobox — reveals the full
            // model name (helpful when truncated past 25 chars)
            // plus explicit load-state wording. While generating
            // we surface why the picker is locked. Uses the snapshot
            // taken at the top of this header render — applying any
            // pending new_selection is deferred to AFTER the
            // modality badge below to keep the `selected` borrow
            // alive across both. Tooltip body builds inside on_hover_ui
            // so the multi-line format! only allocates when the user
            // actually hovers — the chat tab repaints at 60 Hz, so the
            // previous eager format!() spent ~120 short-lived heap
            // allocs/sec on tooltip strings that were almost never
            // visible.
            let selected_name = selected.map(String::as_str);
            combo_resp.response.on_hover_ui(|ui| {
                if chat.is_generating {
                    ui.label("Locked while a response is generating.");
                    ui.label("Stop (or press Esc) to switch models.");
                } else if let Some(m) = selected_name {
                    ui.label(egui::RichText::new(m).strong());
                    if current_is_loaded {
                        ui.label(format!("{} Loaded in memory", theme::ICON_FILLED));
                    } else {
                        ui.label(format!(
                            "{} Not loaded - first send will trigger a load",
                            theme::ICON_EMPTY
                        ));
                    }
                    ui.label("(click to switch model)");
                } else {
                    ui.label("Click to pick a model from the available list");
                }
            });

            // Modality badge — surfaces at-a-glance whether the
            // selected model is text / vision / image-gen / audio.
            // Only render for non-Text modalities; Text is the
            // default and adding a "Text" badge to every plain LLM
            // would be visual noise.
            if let Some(model) = selected {
                let modality = ModelModality::from_model_name(model);
                if modality != ModelModality::Text {
                    let badge_color = modality_badge_color(modality);
                    let badge_resp = egui::Frame {
                        inner_margin: egui::Margin::symmetric(8, 3),
                        corner_radius: CornerRadius::same(4),
                        fill: theme::tinted(badge_color, 30),
                        ..Default::default()
                    }
                    .show(ui, |ui| {
                        ui.label(RichText::new(modality.label()).size(11.0).color(badge_color));
                    });
                    badge_resp.response.on_hover_text(modality.tooltip());
                }
            }

            // Apply any dropdown-driven selection change now that the
            // `selected` borrow above is no longer in use. The change
            // surfaces in next frame's render — tooltip + badge above
            // reflect the previous selection until then, which is
            // acceptable (one-frame UI lag, invisible in practice).
            if auto_clicked {
                chat.smart_auto = !chat.smart_auto;
            }
            if let Some(name) = new_selection {
                models.selected_model = Some(name);
                // Picking a concrete model leaves Auto mode.
                chat.smart_auto = false;
            }

            ui.separator();

            // Layer mode toggle. Two-state: AllLayers (default) ↔
            // Adaptive (early-exit on high-confidence tokens). Deliberately not
            // three: a "CUDA only" state would send an option no handler reads.
            use crate::state::LayerMode;
            let (toggle_color, toggle_text) = match chat.layer_mode {
                LayerMode::AllLayers => (t.text_muted, "All Layers"),
                LayerMode::Adaptive  => (theme::warning(), "Adaptive"),
            };
            let toggle_tip = match chat.layer_mode {
                LayerMode::AllLayers => "All Layers: run every layer (highest quality).\nClick to switch to Adaptive.",
                LayerMode::Adaptive  => "Adaptive: exit early on high-confidence tokens (faster, slight quality drop).\nClick to switch back to All Layers.",
            };
            let toggle_btn = egui::Button::new(
                RichText::new(toggle_text).size(11.0).color(toggle_color),
            )
            .fill(theme::tinted(toggle_color, 20))
            .corner_radius(CornerRadius::same(4));
            if ui.add(toggle_btn).on_hover_text(toggle_tip).clicked() {
                chat.layer_mode = match chat.layer_mode {
                    LayerMode::AllLayers => LayerMode::Adaptive,
                    LayerMode::Adaptive  => LayerMode::AllLayers,
                };
            }

            // Clear-chat button: drops conversation history + attached
            // staging files in one click. Disabled while a generation is
            // in flight (clearing mid-stream would leave the streaming
            // text orphaned). Push to the right edge so it doesn't
            // crowd the modality/profile badges.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_clear = !chat.is_generating
                    && (!chat.messages.is_empty() || !chat.attached_images.is_empty());
                // Trash-icon SVG ships in icons/trash.svg; egui's
                // Button supports an image_and_text constructor that
                // composes the icon + label in one widget.
                let clear_btn = egui::Button::image_and_text(
                    Icon::Trash.image(13.0, theme::error()),
                    RichText::new("Clear").size(11.0).color(theme::error()),
                )
                .fill(theme::tinted(theme::error(), 20))
                .corner_radius(CornerRadius::same(4));
                let resp = ui.add_enabled(can_clear, clear_btn);
                // Tooltip enumerates what gets cleared + what doesn't
                // so users know it's a session-local action, not a
                // disk-side wipe. Prompt history (Ctrl+Up /
                // Ctrl+Down) survives intentionally so users can recall
                // and re-send prompts after clearing.
                //
                // Build the tooltip inside on_hover_ui so the per-
                // frame format!() goes away — the chat tab renders at
                // 60 Hz and the tooltip body is only visible on hover.
                // Snapshot the counts before the closure to keep the
                // hover read independent of mid-frame mutations.
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
                    // Release GPU textures keyed by msg_*, gen_*, attach_*
                    // alongside the message vec. Without this, every
                    // Clear leaks textures: a session that runs Clear N
                    // times with K images per session accumulates N*K
                    // texture handles even though no UI references
                    // them. ColorImage::from_rgba_unmultiplied uploads
                    // up to ~16 MB per Mpixel; freeing matters for
                    // long-running GUI sessions doing image gen.
                    image_textures.clear();
                    // Drop egui_commonmark's per-viewer scroll-state
                    // cache too. Each rendered message creates a
                    // scrollable entry keyed by viewer id; after Clear
                    // those entries point at messages that no longer
                    // exist. Bounded (~KB per session usually) but
                    // measurable for users who Clear repeatedly in a
                    // long-running session — same intent as the
                    // image_textures.clear() right above.
                    md_cache.clear_scrollable();
                }
            });
        });
    });
}

/// Render input area
#[allow(clippy::too_many_arguments)] // ui/state/theme bundle + modality/model/visible_rows
fn render_input_area(
    ui: &mut egui::Ui,
    chat: &mut ChatState,
    t: &ChatTheme,
    image_textures: &mut HashMap<String, TextureHandle>,
    modality: ModelModality,
    // True when models.selected_model.is_some() — gates the Send
    // button. Without this, the user could type, hit Send, and
    // discover post-click that the server-side guard rejects the
    // request with "No model selected"; disabling Send up-front
    // with a clear tooltip avoids the wasted click.
    model_selected: bool,
    // Pre-computed visible-row count for the TextEdit. Hoisted to
    // the caller (which also uses it to reserve output-area height)
    // so the per-frame O(N) chars-counting pass over the input
    // buffer happens once, not twice.
    visible_rows: usize,
) -> bool {
    // True when the user clicked Send this frame — surfaced back to the
    // caller so the button and the Enter shortcut share one send path.
    let mut send_clicked = false;
    egui::Frame {
        inner_margin: egui::Margin::symmetric(12, 10),
        corner_radius: CornerRadius::same(8),
        fill: t.surface,
        stroke: Stroke::new(1.0, t.border),
        shadow: egui::epaint::Shadow {
            offset: [0, -1],
            blur: 3,
            spread: 0,
            color: Color32::from_black_alpha(if theme::is_dark() { 20 } else { 6 }),
        },
        ..Default::default()
    }
    .show(ui, |ui| {
        // Seed-lock indicator. When the user clicked the Lock-seed button
        // on a prior bubble, surface a chip so they know the next
        // send will reuse that seed (otherwise the lock is invisible
        // until the next response echoes back). The clear button clears the
        // lock so a user who clicked by mistake can opt out without
        // sending the request.
        if let Some(seed) = chat.locked_seed {
            ui.horizontal(|ui| {
                egui::Frame {
                    inner_margin: egui::Margin::symmetric(8, 3),
                    corner_radius: CornerRadius::same(4),
                    fill: theme::tinted(theme::success(), 30),
                    ..Default::default()
                }
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        Icon::Lock.show(ui, 11.0, theme::success());
                        ui.label(
                            RichText::new(format!("Seed locked: {seed}"))
                                .size(10.0)
                                .strong()
                                .color(theme::success())
                                .family(egui::FontFamily::Monospace),
                        );
                    });
                });
                let clear = ui.add(
                    egui::Button::image_and_text(
                        Icon::Cross.image(10.0, t.text_secondary),
                        "",
                    ).min_size(egui::vec2(20.0, 20.0)),
                );
                if clear
                    .on_hover_text("Clear the seed lock — next send will use a fresh random seed")
                    .clicked()
                {
                    chat.locked_seed = None;
                }
            });
            ui.add_space(4.0);
        }

        // Image-gen control row. Shown only when an image-gen model
        // is loaded (the same `modality` that drives the badge color)
        // so it doesn't waste vertical space for text/audio chats
        // where it has no effect. Dragging either slider commits an
        // override; the per-control refresh button reverts to the server's
        // per-model default. Strength only takes effect when the
        // user has attached an input image (img2img path); the
        // tooltip surfaces that conditional.
        if modality == ModelModality::ImageGen {
            // Size dropdown — first control on the row, since dimensions
            // change wall-clock more than any other knob (1024² ≈ 4× the
            // work of 512²). Default = "let the server pick its per-
            // model default" (Flux 512² / Z-Image 1024²).
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Size:")
                        .size(10.0)
                        .color(t.text_secondary),
                );
                let current_label: String = match chat.image_size {
                    Some((w, h)) => format!("{w}×{h}"),
                    None => "(default)".to_string(),
                };
                egui::ComboBox::from_id_salt("image_size_combo")
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(
                            chat.image_size.is_none(),
                            "(default) — server picks per-model",
                        ).clicked() {
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
                        "Image dimensions. Higher = sharper but slower \
                         (1024² ≈ 4× the work of 512²). Server defaults: \
                         Flux 512², Z-Image 1024².",
                    );
                ui.add_space(8.0);

                let mut shown_value: u32 = chat.image_num_steps.unwrap_or(IMAGE_NUM_STEPS_PLACEHOLDER);
                ui.label(
                    RichText::new("Steps:")
                        .size(10.0)
                        .color(t.text_secondary),
                );
                let slider = ui.add(
                    egui::Slider::new(&mut shown_value, IMAGE_NUM_STEPS_MIN..=IMAGE_NUM_STEPS_MAX)
                        .integer()
                        .show_value(true)
                        .text(if chat.image_num_steps.is_some() { "override" } else { "default" }),
                );
                if slider.changed() {
                    // Any drag = commit the override. The user can
                    // revert to "let the server pick" via the refresh button below.
                    chat.image_num_steps = Some(shown_value);
                }
                slider.on_hover_text(
                    "Number of denoising steps. Higher = more detail but slower. \
                     Server defaults: Flux Schnell 4, Z-Image Turbo 9.",
                );
                if chat.image_num_steps.is_some() {
                    let reset = ui.add(
                        egui::Button::image_and_text(
                            Icon::Refresh.image(10.0, t.text_secondary),
                            "",
                        ).min_size(egui::vec2(20.0, 20.0)),
                    );
                    if reset
                        .on_hover_text("Revert to the server's per-model default step count")
                        .clicked()
                    {
                        chat.image_num_steps = None;
                    }
                }

                ui.add_space(8.0);
                let mut shown_strength: f32 = chat.image_strength.unwrap_or(IMAGE_STRENGTH_PLACEHOLDER);
                // Strength is a no-op for pure txt2img: the server only
                // consults it on the img2img path (input_image set). Disable
                // the slider when nothing is attached so the user isn't
                // tricked into thinking a strength change will affect the
                // pending generation. Re-enables the instant they attach.
                let has_input_image = !chat.attached_images.is_empty();
                ui.label(
                    RichText::new("Strength:")
                        .size(10.0)
                        .color(if has_input_image { t.text_secondary } else { t.text_muted }),
                );
                let strength_slider = ui.add_enabled(
                    has_input_image,
                    egui::Slider::new(&mut shown_strength, IMAGE_STRENGTH_MIN..=IMAGE_STRENGTH_MAX)
                        .fixed_decimals(2)
                        .show_value(true)
                        .text(if chat.image_strength.is_some() { "override" } else { "default" }),
                );
                if strength_slider.changed() {
                    chat.image_strength = Some(shown_strength);
                }
                strength_slider.on_hover_text(if has_input_image {
                    "img2img mix: 0.0 preserves the input image, 1.0 is full \
                     txt2img re-roll. Server chat-path default: 0.4."
                } else {
                    "Attach an input image to enable img2img strength. Pure \
                     txt2img ignores this control."
                });
                if chat.image_strength.is_some() {
                    let reset = ui.add(
                        egui::Button::image_and_text(
                            Icon::Refresh.image(10.0, t.text_secondary),
                            "",
                        ).min_size(egui::vec2(20.0, 20.0)),
                    );
                    if reset
                        .on_hover_text("Revert to the server's per-route default strength")
                        .clicked()
                    {
                        chat.image_strength = None;
                    }
                }
            });
            ui.add_space(4.0);
        }

        // TTS chat control row. Shown only when an AudioTts model is
        // loaded. Voice dropdown maps to the server's KNOWN_VOICES
        // (OpenAI-shaped presets) which openai_voice_to_description
        // expands into the Parler-TTS voice_description prompt.
        // Speed slider drives time-domain resample at gen end so
        // duration matches /v1/audio/speech (the well-tested path).
        if modality == ModelModality::AudioTts {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Voice:")
                        .size(10.0)
                        .color(t.text_secondary),
                );
                let current_voice = chat.tts_voice.as_deref().unwrap_or("(default)");
                egui::ComboBox::from_id_salt("tts_voice_picker")
                    .selected_text(RichText::new(current_voice).size(11.0))
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(
                            chat.tts_voice.is_none(),
                            "(default) — server picks a neutral voice",
                        ).clicked() {
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
                        "OpenAI-shaped voice preset. Server maps each name to a \
                         Parler-TTS voice_description prompt that nudges the \
                         model toward the requested timbre + tempo.",
                    );

                ui.add_space(8.0);
                let mut shown_speed: f32 = chat.tts_speed.unwrap_or(TTS_SPEED_DEFAULT);
                ui.label(
                    RichText::new("Speed:")
                        .size(10.0)
                        .color(t.text_secondary),
                );
                let speed_slider = ui.add(
                    egui::Slider::new(&mut shown_speed, TTS_SPEED_MIN..=TTS_SPEED_MAX)
                        .fixed_decimals(2)
                        .show_value(true)
                        .text(if chat.tts_speed.is_some() { "override" } else { "default" }),
                );
                if speed_slider.changed() {
                    chat.tts_speed = Some(shown_speed);
                }
                speed_slider.on_hover_text(
                    "Playback speed multiplier. 1.0 = natural pace, 0.5 = half \
                     speed (longer), 2.0 = double speed (shorter). Applied \
                     server-side via time-domain resample so duration matches \
                     /v1/audio/speech.",
                );
                if chat.tts_speed.is_some() {
                    let reset = ui.add(
                        egui::Button::image_and_text(
                            Icon::Refresh.image(10.0, t.text_secondary),
                            "",
                        ).min_size(egui::vec2(20.0, 20.0)),
                    );
                    if reset
                        .on_hover_text("Revert to the server's default speed (1.0)")
                        .clicked()
                    {
                        chat.tts_speed = None;
                    }
                }
            });
            ui.add_space(4.0);
        }

        // Attached file chips. The same staging vec covers images
        // (vision models) and audio (ASR models); we only try to decode
        // a texture when the path looks like an image — for audio we
        // show the Icon::Music SVG instead so the thumbnail isn't an
        // empty box.
        if !chat.attached_images.is_empty() {
            // Header strip: count + "Remove all" button. Only renders
            // when 2+ attachments are staged — removing 1 file via the
            // per-chip clear button is the same number of clicks either way.
            if chat.attached_images.len() >= 2 {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} attached", chat.attached_images.len()))
                            .size(10.0)
                            .color(t.text_secondary),
                    );
                    ui.add_space(6.0);
                    if ui
                        .add(egui::Button::image_and_text(
                            Icon::Cross.image(11.0, t.text_secondary),
                            RichText::new("Remove all").size(10.0),
                        ).small())
                        .on_hover_text("Clear all attached files (does not affect already-sent messages)")
                        .clicked()
                    {
                        chat.clear_attachments();
                        clear_attach_chip_textures(image_textures);
                    }
                });
                ui.add_space(2.0);
            }
            ui.horizontal_wrapped(|ui| {
                let mut remove_idx = None;
                for (i, path) in chat.attached_image_paths.iter().enumerate() {
                    let is_image = path_is_image_ext(path);
                    // Content-hashed key, not positional. The previous
                    // `attach_<i>` scheme misbehaved when the user
                    // removed an attachment and added a different
                    // one: index 0 was reused and served the cached
                    // texture of the removed file.
                    let tex_key = image_cache_key("attach", &chat.attached_images[i], i);
                    // One HashMap lookup per chip per frame. Extract
                    // tex.id() is Copy, so take it here and let the borrow of
                    // image_textures end before the Frame.show closure below - which
                    // would otherwise need a second lookup inside it.
                    let tex_id_opt = if is_image {
                        let tex = image_textures.entry(tex_key.clone()).or_insert_with(|| {
                            load_base64_texture(ui, &chat.attached_images[i], &tex_key)
                        });
                        Some(tex.id())
                    } else {
                        None
                    };
                    // base64 char count ≈ ceil(bytes / 3) * 4; back-solve
                    // to display the source file size for context.
                    // Use the shared api::types::format_size helper so
                    // every byte-size display in the GUI uses the same
                    // unit-picking + decimal rules.
                    let bytes_len = (chat.attached_images[i].len() as f32 * 0.75) as u64;
                    let size_label = crate::api::types::format_size(bytes_len);
                    egui::Frame {
                        inner_margin: egui::Margin::same(4),
                        corner_radius: CornerRadius::same(4),
                        fill: t.surface_elevated,
                        stroke: Stroke::new(0.5, t.border),
                        ..Default::default()
                    }
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(tex_id) = tex_id_opt {
                                ui.image(egui::load::SizedTexture::new(tex_id, egui::vec2(40.0, 40.0)));
                            } else {
                                // Audio (or unknown) file: Icon::Music
                                // placeholder — matches the visual weight
                                // of the image thumbnail without faking
                                // a decode.
                                Icon::Music.show(ui, 32.0, t.text_secondary);
                            }
                            ui.vertical(|ui| {
                                // Keep filename + filename_display as
                                // Cow<str> so the common ASCII path is
                                // alloc-free: to_string_lossy returns
                                // Cow::Borrowed for valid UTF-8 paths,
                                // and truncate_with_ellipsis returns
                                // Cow::Borrowed when the name fits in
                                // 32 bytes — the previous `.into_owned()`
                                // forced an allocation per chip per
                                // frame even when nothing needed
                                // copying.
                                let filename: std::borrow::Cow<str> = std::path::Path::new(path)
                                    .file_name()
                                    .map(|n| n.to_string_lossy())
                                    .unwrap_or_else(|| std::borrow::Cow::Owned(format!("attachment_{}", i)));
                                // Cap the visible filename so a long
                                // generated name (uuid-stamped
                                // exports / `output_2026-05-18_…`)
                                // doesn't force the chip to overflow
                                // its parent's horizontal layout.
                                // Tooltip surfaces the full filename
                                // so users can read it on hover.
                                let filename_display = truncate_with_ellipsis(&filename, 32);
                                ui.label(
                                    RichText::new(filename_display.as_ref())
                                        .size(10.0)
                                        .color(t.text_secondary),
                                )
                                .on_hover_text(filename.as_ref());
                                ui.label(RichText::new(size_label).size(9.0).color(t.text_muted));
                                if ui
                                    .add(egui::Button::image(
                                        Icon::Cross.image(10.0, t.text_secondary),
                                    ).small().frame(false))
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
                    // Invalidate ALL attach_* textures, not just the
                    // removed index. Removing from a Vec shifts every
                    // entry after `idx` down by one, so attach_{idx+1}
                    // → attach_{idx}, attach_{idx+2} → attach_{idx+1},
                    // etc. Only clearing attach_{idx} leaves stale
                    // textures pointing at the wrong attached file
                    // (the previous neighbor's image renders for the
                    // shifted entry). Matches the same retain pattern
                    // used at attach-files / drag-drop paths.
                    clear_attach_chip_textures(image_textures);
                }
            });
            ui.add_space(4.0);
        }

        // Generating status
        if chat.is_generating {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Generating...").size(12.0).color(theme::accent()));
            });
            ui.add_space(4.0);
        }

        // Input row
        ui.horizontal(|ui| {
            // Attach button — modality-aware. Vision models accept image
            // files (current behaviour). ASR models accept audio files;
            // we surface them through the same attached_image* slots
            // since the server's /api/chat with images[] handles the
            // base64 bytes regardless (whisper sees them as audio bytes
            // via the model's pre-processor). Text also shows the
            // button (defaults to image attach) so the user can switch
            // to a vision model mid-thread without losing the staging
            // slot. ImageGen / AudioTts / VideoGen hide the button —
            // those are text-only request paths server-side.
            let attach_visible = matches!(
                modality,
                ModelModality::Text | ModelModality::Vision | ModelModality::AudioAsr
            );
            if attach_visible {
                let (tooltip, filter_label, filter_exts): (&str, &str, &[&str]) = match modality {
                    ModelModality::AudioAsr => (
                        "Attach audio file (for transcription).\n\
                         Tip: drag-and-drop also works — drop a file anywhere on the window.",
                        "Audio",
                        CHAT_AUDIO_EXTS,
                    ),
                    // Default + Vision: image attach. The Text fallback
                    // keeps the button available so users can switch to
                    // a vision model without losing the staging slot.
                    _ => (
                        "Attach image (for vision models).\n\
                         Tip: drag-and-drop also works — drop a file anywhere on the window.",
                        "Images",
                        CHAT_IMAGE_EXTS,
                    ),
                };
                let attach_btn = egui::Button::image(
                    Icon::Attach.image(16.0, t.text_primary),
                )
                .fill(t.surface_elevated)
                .stroke(Stroke::new(0.5, t.border))
                .corner_radius(CornerRadius::same(4));
                // Disable while a dialog worker is alive — a second
                // click would spawn an overlapping dialog whose result
                // could overwrite the first in pending_dialog.
                use std::sync::atomic::Ordering;
                let dialog_busy = chat.dialog_in_flight.load(Ordering::Relaxed);
                let attach_resp = ui.add_enabled(!dialog_busy, attach_btn);
                let attach_tip = if dialog_busy {
                    "Waiting for the open file dialog to close…"
                } else {
                    tooltip
                };
                if attach_resp.on_hover_text(attach_tip).clicked() {
                    // Worker-thread variant: rfd's sync pick_files
                    // blocks the egui main thread which deadlocks
                    // against the XDG portal on Linux. spawn_dialog_worker
                    // handles the in-flight flag + ClearOnDrop guard so
                    // this closure only carries the dialog logic.
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
                        // Read + base64-encode on the worker so the
                        // GUI thread doesn't stall encoding multi-MB
                        // attachments when the dialog returns. Reject
                        // files over the server-side attachment cap UP-FRONT
                        // (via fs::metadata before reading) so the GUI
                        // surfaces the error inline instead of after
                        // a multi-MB upload round-trip.
                        let mut files: Vec<(std::path::PathBuf, String)> = Vec::new();
                        let mut oversized: Vec<(std::path::PathBuf, u64)> = Vec::new();
                        let mut unreadable: Vec<(std::path::PathBuf, String)> = Vec::new();
                        for p in paths {
                            // metadata() can fail (permission denied,
                            // broken symlink). Previous code used
                            // `.unwrap_or(0)` which let metadata-fails
                            // pass the size check and then silently
                            // disappear at the read step. Surface the
                            // failure via the new unreadable bucket.
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

            // Multi-line input with chat-standard send semantics:
            //   - Enter sends (handled by app.rs key detection)
            //   - Shift+Enter inserts a newline (return_key override)
            // The pattern every chat application uses, and it matters here: an
            // image prompt is often several pasted paragraphs.
            //
            // desired_rows is the same `visible_rows` the caller reserves the
            // output-area height with, so both sites agree and the layout does not
            // flicker. It is computed in render() so the O(N) pass over the input
            // runs once per frame
            // instead of twice.
            let response = ui.add(
                egui::TextEdit::multiline(&mut chat.input)
                    .desired_width(ui.available_width() - 70.0)
                    .desired_rows(visible_rows)
                    .hint_text(modality.input_hint())
                    .id_salt("chat_input")
                    .return_key(Some(egui::KeyboardShortcut::new(
                        egui::Modifiers::SHIFT,
                        egui::Key::Enter,
                    ))),
            );

            // Detach from history navigation as soon as the user edits
            // the recalled prompt. Without this, after Ctrl+Up loads a
            // past prompt and the user starts tweaking it, a subsequent
            // Ctrl+Down would replace their edit with the next history
            // entry — surprising and clobbers work. Helper is on
            // ChatState so the rule is unit-tested.
            if response.changed() {
                chat.detach_history_if_edited();
            }

            if ui.ctx().memory(egui::Memory::focused).is_none() {
                response.request_focus();
            }

            // TTS char-count indicator: show "N / 4096 chars" with a
            // red tint when over the server-side cap. Previously the
            // user only found out post-send via a system-message reject;
            // showing the limit live lets them split or trim before
            // submitting. Gated on AudioTts modality so it doesn't add
            // visual noise to text/image/ASR chats.
            if matches!(modality, ModelModality::AudioTts) {
                let chars = chat.input.chars().count();
                let over = chars > TTS_INPUT_MAX_CHARS_CLIENT;
                let color = if over { theme::error() } else { t.text_muted };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut text = RichText::new(format!(
                        "{} / {} chars",
                        chars, TTS_INPUT_MAX_CHARS_CLIENT,
                    ))
                    .size(10.0)
                    .color(color);
                    if over {
                        text = text.strong();
                    }
                    let resp = ui.add(
                        egui::Label::new(text).sense(egui::Sense::hover()),
                    );
                    if over {
                        resp.on_hover_text(
                            "TTS input exceeds the 4096-char cap. \
                             Split into multiple sends or trim before submitting.",
                        );
                    }
                });
            }

            // Send button — caller checks the returned bool to fire
            // the same code path as the Enter shortcut. Previously the
            // click did nothing because the renderer didn't surface it.
            //
            // Tooltip documents the keyboard shortcut (Enter to send,
            // Shift+Enter for newline) so users new to the multi-line
            // chat-input UX from 68498d0 know what each key does.
            // VideoGen-tagged checkpoints have no server-side dispatch
            // path today (see ModelModality::tooltip — 'label only'). The
            // empty-state copy warns the user, but if they type a prompt
            // anyway and hit Send the request would fail at /api/chat
            // with a model-error. Disable Send proactively for VideoGen
            // with a tooltip explaining why — same end state, clearer UI.
            // Single source of truth: chat_send_allowed handles the
            // modality-aware "is this combination sendable?" question.
            // ASR with attachment doesn't need text; VideoGen is never
            // sendable; everything else requires non-empty text.
            // send_chat (app.rs) uses the same helper so the button
            // gate and the runtime guard can't drift.
            let input_empty = chat.input.trim().is_empty();
            let has_attachment = !chat.attached_images.is_empty();
            let video_gen_blocked = matches!(modality, ModelModality::VideoGen);
            let is_asr = matches!(modality, ModelModality::AudioAsr);
            let asr_blocked = is_asr && !has_attachment;
            let text_blocked = !is_asr && !video_gen_blocked && input_empty;
            // Three gate sources, all must clear:
            //   1. model_selected: server-side guard would reject
            //      the send otherwise — disable up-front.
            //   2. !chat.is_generating: matches the Stop-button
            //      swap logic; can't queue a second send.
            //   3. chat_send_allowed: modality-specific input/
            //      attachment requirements.
            let send_enabled = model_selected
                && !chat.is_generating
                && chat_send_allowed(modality, input_empty, has_attachment);
            // Per-reason tooltip so the user knows which constraint
            // is blocking Send instead of seeing the generic
            // keyboard-help text on every disabled state.
            let send_tip = if !model_selected {
                // Most-blocking gate goes first — if no model is
                // picked, the other reasons are moot.
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
                   Enter — send\n  \
                   Shift+Enter — newline\n  \
                   Ctrl+Up / Ctrl+Down - recall prompt history\n  \
                   Ctrl+L — clear conversation\n  \
                   Esc — cancel in-flight generation"
            };
            if chat.is_generating {
                // Active-generation state: show a Stop button instead of
                // the disabled Send. Clicking it aborts the in-flight
                // tokio task via the AbortHandle captured in send_chat.
                // Without this, users had to wait out a stuck/slow
                // generation OR restart the GUI — both bad UX.
                //
                // Filled-square icon (Stop) instead of the prior Cross
                // (×): pairs visually with the Play triangle the Send
                // button uses when idle, matching the media-player
                // convention every user already knows. Cross meant
                // "dismiss / close" in the rest of the GUI (chip
                // removal, filter clear) — overloading it for "abort
                // generation" muddled the visual language.
                let stop_btn = egui::Button::image_and_text(
                    Icon::Stop.image(11.0, Color32::WHITE),
                    RichText::new("Stop").size(12.0).color(Color32::WHITE),
                )
                .fill(theme::error())
                .corner_radius(CornerRadius::same(4));
                if ui.add(stop_btn)
                    .on_hover_text("Cancel the in-flight generation (or press Esc).\n\
                                    Already-streamed content is kept; \
                                    the next chunk after abort is dropped.")
                    .clicked()
                    && chat.abort_generation()
                {
                    chat.messages.push_back(ChatMessage::system(
                        "[Generation cancelled by user]",
                    ));
                }
            } else if send_enabled {
                let send_btn = egui::Button::image_and_text(
                    Icon::Play.image(11.0, Color32::WHITE),
                    RichText::new("Send").size(12.0).color(Color32::WHITE),
                )
                .fill(theme::accent())
                .corner_radius(CornerRadius::same(4));
                send_clicked = ui.add(send_btn).on_hover_text(send_tip).clicked();
            } else {
                // Tint the disabled-state Play icon with the theme-
                // matched muted colour so it visually drops out in
                // both dark and light modes (previous hardcode of
                // theme::ink_dim() looked wrong against the
                // light background).
                ui.add_enabled(
                    false,
                    egui::Button::image_and_text(
                        Icon::Play.image(11.0, t.text_muted),
                        "Send",
                    ),
                )
                .on_hover_text(send_tip);
            }
        });
    });
    send_clicked
}

/// Render a single chat message
// Wide signature carries the message, theme palette, markdown cache,
// GPU texture map, and the worker-dialog handles (slot + busy flag).
// Refactoring to a struct adds boilerplate without simplifying the
// single render() call site.
#[allow(clippy::too_many_arguments)]
fn render_message(
    ui: &mut egui::Ui,
    msg: &ChatMessage,
    t: &ChatTheme,
    md_cache: &mut CommonMarkCache,
    image_textures: &mut HashMap<String, TextureHandle>,
    pending_dialog: &std::sync::Arc<std::sync::Mutex<Option<ChatDialogResult>>>,
    dialog_in_flight: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    // Single-shot seed-lock slot. Written by the "Lock seed"
    // button on a seed badge; drained by the outer render() loop
    // and copied into ChatState.locked_seed for the next send.
    // Cell lets multiple message-render passes write into one slot
    // without needing &mut ChatState here (we'd otherwise have to
    // shadow the borrow split that drives the per-message loop).
    seed_lock_request: &std::cell::Cell<Option<u64>>,
    // Single-shot Play-error slot. Written by the audio Play
    // button when play_audio_blob fails (no audio backend on PATH,
    // /tmp write denied, etc.); drained by the outer render() loop
    // and pushed as a system message so the user sees what went
    // wrong inline instead of having to open the Server Log tab.
    play_error_pending: &std::cell::Cell<Option<String>>,
) {
    let is_user = msg.role == "user";
    let is_system = msg.role == "system";
    // Detection lives in is_error_system_message so the prefix rules
    // are unit-tested (rendering itself is hard to test against egui).
    let is_error = is_system && is_error_system_message(&msg.content);

    let alignment = if is_user { egui::Align::RIGHT } else { egui::Align::LEFT };

    ui.with_layout(egui::Layout::left_to_right(alignment), |ui| {
        // Avatar badge
        let (avatar_bg, avatar_icon, avatar_label) = t.avatar(&msg.role, is_error);

        egui::Frame {
            inner_margin: egui::Margin::symmetric(8, 4),
            corner_radius: CornerRadius::same(4),
            fill: avatar_bg,
            ..Default::default()
        }
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(avatar_icon.image(12.0, Color32::WHITE));
                ui.label(RichText::new(avatar_label).size(11.0).strong().color(Color32::WHITE));
            });
        });

        ui.add_space(8.0);

        // Message bubble
        let (bubble_fill, bubble_stroke, text_color) = t.bubble(&msg.role, theme::is_dark(), is_error);

        egui::Frame {
            inner_margin: egui::Margin::symmetric(12, 10),
            corner_radius: CornerRadius::same(6),
            fill: bubble_fill,
            stroke: Stroke::new(1.0, bubble_stroke),
            ..Default::default()
        }
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width() * 0.75);

            ui.label(RichText::new(&msg.timestamp).size(10.0).color(t.text_muted));
            ui.add_space(4.0);

            // Attached files (vision input OR audio for ASR models).
            // Detects content type from base64 magic bytes so audio
            // attachments render as Icon::Music chips rather than empty
            // boxes.
            if !msg.images.is_empty() {
                // Pre-pass over the IMAGE attachments only (the same list may hold
                // audio chips), so opening one gives the viewer the whole set to step
                // through rather than a single frozen image.
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
                // Position of the current attachment WITHIN img_texes, which skips
                // the non-image entries, so the arrows never land on an audio chip.
                let mut img_ord = 0usize;
                ui.horizontal_wrapped(|ui| {
                    for (i, img_b64) in msg.images.iter().enumerate() {
                        let is_image = base64_looks_like_image(img_b64);
                        if is_image {
                            // Key on bytes hash + index, not on the
                            // minute-resolution timestamp — see
                            // image_cache_key's docstring for the
                            // collision bug this avoids.
                            let tex_key = image_cache_key("msg", img_b64, i);
                            // One HashMap lookup per visible image per
                            // frame. The handle returned by
                            // entry().or_insert_with() borrows
                            // image_textures, but tex_id + tex_size are
                            // Copy — extract them so the borrow ends
                            // before ui.image() runs.
                            // The pre-pass above already inserted this texture, so
                            // this is the same cached handle it collected.
                            let tex = image_textures.entry(tex_key.clone()).or_insert_with(|| {
                                load_base64_texture(ui, img_b64, &tex_key)
                            });
                            let tex_size = tex.size_vec2();
                            let max_side = 200.0;
                            let scale = (max_side / tex_size.x.max(tex_size.y)).min(1.0);
                            crate::image_viewer::clickable_image_in_set(
                                ui,
                                &img_texes,
                                img_ord,
                                tex_size * scale,
                            );
                            img_ord += 1;
                        } else {
                            // Audio (or unknown binary): music-icon
                            // placeholder with the encoded size.
                            // Matches the input-chip styling for
                            // visual continuity between pre-send and
                            // post-send views. Uses the shared
                            // api::types::format_size helper.
                            let bytes_len = (img_b64.len() as f32 * 0.75) as u64;
                            let size_label = crate::api::types::format_size(bytes_len);
                            egui::Frame {
                                inner_margin: egui::Margin::same(8),
                                corner_radius: CornerRadius::same(4),
                                fill: t.surface_elevated,
                                stroke: Stroke::new(0.5, t.border),
                                ..Default::default()
                            }
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    Icon::Music.show(ui, 32.0, t.text_secondary);
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new("Audio").size(11.0).strong().color(t.text_secondary));
                                        ui.label(RichText::new(size_label).size(10.0).color(t.text_muted));
                                    });
                                });
                            });
                        }
                    }
                });
                ui.add_space(4.0);
            }

            // Content: markdown for AI, plain text for user/system.
            // Guard on non-empty so image-only assistant responses
            // (content cleared in app.rs ImageGenResponse handler) don't
            // emit an empty markdown block above the image grid.
            //
            // The user/system label is rendered as a selectable Label so
            // users can drag-select + Ctrl+C their own past prompts
            // (handy when iterating on an image-gen prompt). Assistant
            // markdown goes through CommonMarkViewer which doesn't
            // expose a selectability knob — the per-message Copy
            // button below covers that case.
            //
            // Image-gen responses begin with `[seed: <u64>] …` (server
            // commit 27cbef3). Lift the seed into a dedicated badge
            // with a copy-to-clipboard button so the user can re-roll
            // by pasting it into their next prompt's options.seed —
            // the bracketed text in the body is noise once a badge
            // surfaces it explicitly.
            //
            // Skip the parse for user + system messages — they can
            // never carry the server-emitted seed prefix, so running
            // the trim + strip_prefix scan over the user's typed
            // prompts every frame is pure waste.
            let (extracted_seed, content_without_seed) = if is_user || is_system {
                (None, msg.content.as_str())
            } else {
                parse_seed_prefix(&msg.content)
            };
            if let Some(seed) = extracted_seed {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    egui::Frame {
                        inner_margin: egui::Margin::symmetric(8, 3),
                        corner_radius: CornerRadius::same(4),
                        fill: theme::tinted(theme::success(), 30),
                        ..Default::default()
                    }
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("seed {seed}"))
                                .size(10.0)
                                .strong()
                                .color(theme::success())
                                .family(egui::FontFamily::Monospace),
                        );
                    });
                    // SVG icon (not emoji) to match the rest of the
                    // chat-tab visual language - the same Copy button as the
                    // assistant action row and the image base64 copy. An SVG icon
                    // rather than a clipboard emoji, like every other site here.
                    let copy = ui.add(
                        egui::Button::image_and_text(
                            Icon::Copy.image(11.0, t.text_primary),
                            RichText::new("Copy").size(10.0),
                        )
                        .min_size(egui::vec2(0.0, 20.0)),
                    );
                    if copy
                        .on_hover_text("Copy seed — paste into the next prompt's options.seed to re-roll this exact generation")
                        .clicked()
                    {
                        ui.ctx().copy_text(seed.to_string());
                    }
                    // Lock seed — one-click re-roll. Sets ChatState.
                    // locked_seed which the next chat send consumes,
                    // threading options.seed=<this> into the request.
                    // Single-shot: cleared after send so a tweaked
                    // prompt automatically gets a fresh random seed
                    // on subsequent gens unless the user re-locks.
                    let lock = ui.add(
                        egui::Button::image_and_text(
                            Icon::Lock.image(11.0, t.text_primary),
                            RichText::new("Lock seed").size(10.0),
                        ).min_size(egui::vec2(0.0, 20.0)),
                    );
                    if lock
                        .on_hover_text("Lock this seed for your NEXT send — generate with the same starting noise but a different prompt to A/B compare variations")
                        .clicked()
                    {
                        seed_lock_request.set(Some(seed));
                    }
                });
                ui.add_space(2.0);
            }
            let body_to_render = if extracted_seed.is_some() {
                content_without_seed
            } else {
                msg.content.as_str()
            };
            if !body_to_render.is_empty() {
                if msg.role == "assistant" {
                    CommonMarkViewer::new().show(ui, md_cache, body_to_render);
                } else {
                    ui.add(
                        egui::Label::new(
                            RichText::new(body_to_render).size(13.0).color(text_color),
                        )
                        .selectable(true),
                    );
                }
            }

            // Generated audio (TTS responses): render Play + Save buttons
            // per WAV blob. Playback is best-effort — we shell out to the
            // first available system player (paplay → aplay → afplay →
            // xdg-open), no Rust audio dep, no libasound2-dev requirement.
            if !msg.generated_audios.is_empty() {
                ui.add_space(8.0);
                for (i, wav_b64) in msg.generated_audios.iter().enumerate() {
                    ui.horizontal(|ui| {
                        Icon::Speaker.show(ui, 12.0, text_color);
                        // Common case (1 audio response) takes the
                        // borrowed branch — no per-frame allocation
                        // for the label. Multi-audio chats hit the
                        // format!() owned branch.
                        let label_owned;
                        let label: &str = if msg.generated_audios.len() == 1 {
                            "Audio response"
                        } else {
                            label_owned = format!(
                                "Audio {} of {}", i + 1, msg.generated_audios.len(),
                            );
                            &label_owned
                        };
                        ui.label(RichText::new(label).size(11.0).color(text_color));

                        let play_btn = egui::Button::image_and_text(
                            Icon::Play.image(11.0, t.text_primary),
                            RichText::new("Play").size(11.0),
                        );
                        if ui.add(play_btn).clicked() {
                            if let Err(e) = play_audio_blob(wav_b64) {
                                tracing::warn!("audio playback failed: {e}");
                                // Surface the failure inline too —
                                // tracing::warn alone only shows in
                                // the Server Log tab, which most users
                                // never open. Common cases worth
                                // making visible: no system audio
                                // player on PATH (server-only Linux
                                // install), no /tmp write perm.
                                // Prefix with "Failed " so the
                                // is_error_system_message helper picks
                                // it up and the bubble gets the
                                // error-tint styling, not neutral
                                // system styling (which a user might
                                // glance past).
                                play_error_pending.set(Some(format!(
                                    "Failed to play audio: {e}. \
                                     Install paplay/aplay/afplay or use the Save \
                                     button and play the WAV in an external app."
                                )));
                            }
                        }
                        // Shared save path (dialog::save_button +
                        // dialog::spawn_save — audit #8): the button
                        // is disabled while another dialog worker is
                        // alive, the decode happens at click time
                        // (cheap, matches the Media Studio), and the
                        // pick routes through ChatDialogResult::
                        // SaveBytes so the main-thread drain surfaces
                        // success/failure inline (mirrors image-save).
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

            // Generated images
            if !msg.generated_images.is_empty() {
                ui.add_space(8.0);
                // Save-all toolbar: only appears for multi-image responses
                // where N>1 makes per-image clicking tedious. Picks a
                // directory once and writes image_<ts>_0.png ... _N-1.png
                // into it. Skips dialog cancels silently.
                if msg.generated_images.len() > 1 {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{} images", msg.generated_images.len()))
                                .size(10.0)
                                .strong()
                                .color(t.text_secondary),
                        );
                        ui.separator();
                        use std::sync::atomic::Ordering;
                        let dialog_busy = dialog_in_flight.load(Ordering::Relaxed);
                        let save_all_btn = egui::Button::new(
                            RichText::new("Save all").size(11.0),
                        );
                        let save_all_resp = ui.add_enabled(!dialog_busy, save_all_btn);
                        let save_all_tip = if dialog_busy {
                            "Waiting for the open file dialog to close…"
                        } else {
                            "Save all images to a folder"
                        };
                        if save_all_resp.on_hover_text(save_all_tip).clicked() {
                            // Snapshot all image bytes on the GUI
                            // thread (cheap base64 decode), then run
                            // pick_folder off-thread so the dialog
                            // doesn't deadlock the egui main loop.
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
                    ui.add_space(4.0);
                }
                for (i, img_b64) in msg.generated_images.iter().enumerate() {
                    // See msg_* call site comment + image_cache_key
                    // docstring — content-hash keying defeats the
                    // same-minute timestamp collision.
                    let tex_key = image_cache_key("gen", img_b64, i);
                    // One HashMap lookup per visible generated image
                    // per frame. The handle returned by
                    // entry().or_insert_with borrows image_textures
                    // but tex_id and tex_size are Copy - extract them so the borrow
                    // ends before ui.image() runs, without a second lookup.
                    let tex = image_textures.entry(tex_key.clone()).or_insert_with(|| {
                        load_base64_texture(ui, img_b64, &tex_key)
                    });
                    let tex_handle = tex.clone();
                    let tex_size = tex.size_vec2();
                    let max_width = ui.available_width() * 0.9;
                    let scale = (max_width / tex_size.x).min(1.0);
                    crate::image_viewer::clickable_image(ui, &tex_handle, tex_size * scale);
                    let tex_size_opt = Some(tex_size);
                    // Save / Copy actions row: dimensions + buttons. Renders even
                    // before the texture decodes so the user has a stable target.
                    ui.horizontal(|ui| {
                        if let Some(sz) = tex_size_opt {
                            ui.label(
                                RichText::new(format!("{}\u{00D7}{}", sz.x as u32, sz.y as u32))
                                    .size(10.0)
                                    .color(t.text_muted),
                            );
                            ui.separator();
                        }
                        // Save: opens a file dialog defaulting to PNG. The
                        // server emits PNG-encoded base64 today; if/when WebP
                        // support lands, the format selector will follow.
                        // Worker-thread to avoid the rfd-on-Linux deadlock.
                        use std::sync::atomic::Ordering;
                        let dialog_busy = dialog_in_flight.load(Ordering::Relaxed);
                        let save_btn = egui::Button::new(
                            RichText::new("Save").size(11.0),
                        );
                        let save_resp = ui.add_enabled(!dialog_busy, save_btn);
                        let save_tip = if dialog_busy {
                            "Waiting for the open file dialog to close…"
                        } else {
                            "Save image to disk"
                        };
                        if save_resp.on_hover_text(save_tip).clicked() {
                            use base64::Engine;
                            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(img_b64) {
                                let default_name = format!("image_{}_{}.png", msg.timestamp, i);
                                // Shared single-file save worker
                                // (dialog::spawn_save — audit #8);
                                // same SaveBytes round-trip as the
                                // TTS save + Media Studio saves.
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
                        // Copy as base64 to clipboard. Useful when chaining
                        // through external tools that consume data: URIs.
                        if ui
                            .add(egui::Button::image_and_text(
                                Icon::Copy.image(12.0, t.text_secondary),
                                RichText::new("Copy b64").size(11.0),
                            ).small())
                            .on_hover_text("Copy base64 string to clipboard")
                            .clicked()
                        {
                            ui.ctx().copy_text(img_b64.clone());
                        }
                    });
                    ui.add_space(6.0);
                }
            }

            // Assistant action row: copy text to clipboard. Available
            // whenever the message has any text content (skip for
            // image-only generations where content is empty).
            if !is_user && !is_system && !msg.content.is_empty() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::image_and_text(
                            Icon::Copy.image(12.0, t.text_secondary),
                            RichText::new("Copy").size(11.0),
                        ).small())
                        .on_hover_text("Copy response text to clipboard")
                        .clicked()
                    {
                        ui.ctx().copy_text(msg.content.clone());
                    }
                });
            }

            // Timing info — text models report tokens/sec + count;
            // image-gen / TTS responses arrive with token_count=0 so
            // those columns would render as "0.0 tok/s | 0 tokens"
            // which is noise. format_timing_line() collapses to a
            // duration-only string in that case.
            if !is_user && !is_system {
                if let Some(timing) = msg.timing {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format_timing_line(timing))
                            .size(10.0)
                            .color(t.text_muted),
                    );
                }
            }
        });
    });
}

/// Render streaming message
fn render_streaming_message(
    ui: &mut egui::Ui,
    content: &str,
    image_progress: &Option<(usize, usize)>,
    image_started_at: Option<std::time::Instant>,
    t: &ChatTheme,
    md_cache: &mut CommonMarkCache,
) {
    ui.horizontal(|ui| {
        // AI avatar
        let (avatar_bg, _, _) = t.avatar("assistant", false);
        egui::Frame {
            inner_margin: egui::Margin::symmetric(8, 4),
            corner_radius: CornerRadius::same(4),
            fill: avatar_bg,
            ..Default::default()
        }
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(Icon::Bot.image(12.0, Color32::WHITE));
                ui.label(RichText::new("AI").size(11.0).strong().color(Color32::WHITE));
            });
        });

        ui.add_space(8.0);

        // Streaming bubble
        let (bubble_fill, _, _) = t.bubble("assistant", theme::is_dark(), false);
        egui::Frame {
            inner_margin: egui::Margin::symmetric(12, 10),
            corner_radius: CornerRadius::same(6),
            fill: bubble_fill,
            stroke: Stroke::new(2.0, theme::accent()),
            ..Default::default()
        }
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width() * 0.75);

            if let Some((completed, total)) = image_progress {
                // ETA: extrapolate from observed mean step duration.
                // Skip while completed < 1 (no data point yet); once
                // we have one observation the linear estimate is
                // good enough — diffusion step duration is fairly
                // uniform per request (constant batch + resolution).
                // ETA off-by-one fix: when we observe "Step C/N",
                // step C is in progress (started_at was captured the
                // moment we saw step 1 start, so at that instant no
                // step has completed yet). Completed = c - 1; gate on
                // c >= 2 so we have at least one full step duration
                // to extrapolate. At c=1, elapsed ≈ 0 and dividing
                // by c=1 would yield "~0s left" briefly, which is
                // misleading on a 30-step run.
                let eta_str = if let (Some(start), c) = (image_started_at, *completed) {
                    if c >= 2 && c <= *total {
                        let elapsed = start.elapsed().as_secs_f32();
                        let completed_steps = (c - 1) as f32;
                        let per_step = elapsed / completed_steps;
                        let remaining_steps = (*total - c + 1) as f32;
                        let remaining = per_step * remaining_steps;
                        format!(" · ~{} left", ModelModality::format_eta_remaining(remaining))
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new(format!("Generating image... Step {}/{}{}", completed, total, eta_str))
                            .size(12.0)
                            .color(theme::warning()),
                    );
                });
                ui.add_space(4.0);
                let progress = *completed as f32 / (*total).max(1) as f32;
                ui.add(egui::ProgressBar::new(progress).text(format!("{:.0}%", progress * 100.0)));
            } else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Generating...").size(10.0).color(t.text_muted));
                });
                ui.add_space(4.0);
                CommonMarkViewer::new().show(ui, md_cache, content);
                // Painted rather than written: U+2588 FULL BLOCK is not in
                // the bundled fonts and drew as a box outline, which reads as
                // a broken character trailing the text it belongs to.
                let (caret, _) =
                    ui.allocate_exact_size(egui::vec2(7.0, 14.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(caret.shrink2(egui::vec2(1.0, 1.0)), 1.0, theme::accent());
            }
        });
    });
}

/// Render typing indicator. Label text adapts to modality so users
/// see at-a-glance what flavor of generation is starting.
fn render_typing_indicator(
    ui: &mut egui::Ui,
    t: &ChatTheme,
    modality: ModelModality,
    loading_model: Option<&str>,
) {
    // A selected-but-unloaded model pays a load on the first send —
    // the server streams no tokens until the weights are in memory,
    // which can be several seconds and otherwise looks like a hang.
    // Swap the generic "Thinking…" for an explicit load message so the
    // wait is legible.
    let loading_label = loading_model
        .map(|m| format!("Loading {} into memory…", truncate_with_ellipsis(m, 28)));
    let label_text: &str = loading_label.as_deref().unwrap_or(modality.typing_label());
    ui.horizontal(|ui| {
        let (avatar_bg, _, _) = t.avatar("assistant", false);
        egui::Frame {
            inner_margin: egui::Margin::symmetric(8, 4),
            corner_radius: CornerRadius::same(4),
            fill: avatar_bg,
            ..Default::default()
        }
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(Icon::Bot.image(12.0, Color32::WHITE));
                ui.label(RichText::new("AI").size(11.0).strong().color(Color32::WHITE));
            });
        });

        ui.add_space(8.0);

        egui::Frame {
            inner_margin: egui::Margin::symmetric(12, 10),
            corner_radius: CornerRadius::same(6),
            fill: t.surface,
            stroke: Stroke::new(1.0, theme::accent()),
            ..Default::default()
        }
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(label_text)
                        .size(13.0)
                        .color(theme::accent())
                        .italics(),
                );
            });
        });
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
}
