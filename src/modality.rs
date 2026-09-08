//! Model-modality detection + pure chat-tab helpers.
//!
//! Extracted from `chat_tab.rs` (audit #5). Everything here is
//! UI-free and unit-testable: the `ModelModality` classifier, the
//! chat-input layout math, server-mirroring constants (attachment
//! caps, TTS presets, image-size presets), and the small pure
//! formatters/parsers the chat tab and app layer share.


use crate::state::MessageTiming;

// ── Model modality detection ──
//
// Maps a model name to a coarse modality so the chat UI can adapt:
//   - input hint text ("Describe an image..." vs "Type your message...")
//   - header badge (so users see at a glance what kind of model is loaded)
//   - attach-button enablement (vision/transcription models accept files;
//     pure-text + image-gen models don't surface the attach control)
//
// Detection is conservative: substring match on the lowercased name.
// Unknown models fall through to Text — same UX as before this helper
// landed.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ModelModality {
    /// Standard text-in / text-out chat (default fallback).
    Text,
    /// Vision-capable LLM — accepts image attachments alongside text
    /// (moondream, llava-style, gemma3 vision, etc.).
    Vision,
    /// Image generation — text-in, image-out (Flux, Z-Image, SD, MM-DiT,
    /// Wuerstchen). Surfaced via different progress UI + Save button.
    ImageGen,
    /// Video generation — text-in, video-out. No server-side perimeter
    /// model today; included so the GUI can label downloaded checkpoints
    /// (sora-style / veo / cogvideo / videocrafter / wan-) appropriately
    /// and surface a 'not yet runnable' hint instead of pretending the
    /// request will succeed as chat.
    VideoGen,
    /// Audio synthesis (TTS) — text-in, audio-out (Parler, *tts*).
    AudioTts,
    /// Audio transcription (ASR) — audio-in, text-out (Whisper).
    AudioAsr,
}

impl ModelModality {
    /// Heuristic detection from the model id. Mirrors the server-side
    /// dispatch hints (api/handlers.rs is_*_model) so the GUI badge
    /// matches what the server actually does with the request.
    pub(crate) fn from_model_name(name: &str) -> Self {
        let n = name.to_ascii_lowercase();
        if n.contains("whisper") {
            Self::AudioAsr
        } else if n.contains("parler")
            || n.contains("-tts")
            // `tts` at the start ("tts-1", "tts-1-hd") OR after a path
            // separator ("openai/tts-1") — the second form matches the
            // server-side is_tts_model classifier so GUI and server
            // agree on routing for OpenAI-style model ids.
            || n.starts_with("tts")
            || n.contains("/tts-")
            || n.contains("bark")
            || n.contains("kokoro")
            || n.contains("f5-tts")
            || n.contains("fish-speech")
            || n.contains("metavoice")
        {
            Self::AudioTts
        } else if n.contains("sora")
            || n.contains("veo")
            || n.contains("cogvideo")
            || n.contains("videocrafter")
            || n.contains("video-diffusion")
            || n.contains("wan-")
            // Further open-source and hosted video families, so their checkpoints
            // label correctly. Each entry matches the prefix or substring that project
            // uses in its repository id.
            || n.contains("hunyuanvideo") || n.contains("hunyuan-video")
            || n.contains("mochi-")        // genmoai/mochi-1-preview
            || n.contains("ltx-video") || n.contains("lightricks")
            || n.contains("allegro")
            || n.contains("step-video")
            || n.contains("opensora")
            || n.contains("modelscope") && n.contains("text-to-video")
            // Hosted-only families for label parity. Note: `gen-3` alone
            // would false-match `imagen-3` (Google's image model), so we
            // require the `runway-gen` / `runway/gen-3` prefix.
            || n.contains("runway-gen") || n.contains("runway/gen-")
            || n.contains("pika-")
            || n.contains("haiper-")
        {
            // Video-gen detection runs before image-gen so SD-based video
            // variants (e.g. videocrafter, which has 'sd' co-occurrences)
            // don't get misclassified as ImageGen.
            Self::VideoGen
        } else if n.contains("flux") || n.contains("z-image") || n.contains("zimage")
            || n.contains("qwen-image") || n.contains("boogu")
            || n.contains("flux2") || n.contains("klein")
            || n.contains("sdxl") || n.contains("raymnants") || n.contains("rayctifier")
            || n.contains("rayburn")
            || n.starts_with("sd")
            || n.contains("stable-diffusion")
            || n.contains("stable-cascade")
            || n.contains("wuerstchen")
            || n.contains("mm-dit")
            || n.contains("kandinsky")
            || n.contains("pixart")
            || n.contains("playground-v")
            || n.contains("kolors")
            || n.contains("lumina")
            || n.contains("hidream")
            || n.contains("openjourney")
            || n.contains("dall-e")
            || n.contains("dalle")
            // Hosted-only image families surfaced for label parity; the
            // server can't actually generate against these but the GUI
            // should label them correctly so the user isn't misled by a
            // generic "Text" badge on midjourney/imagen/ideogram/etc.
            || n.contains("midjourney")
            || n.contains("imagen")
            || n.contains("ideogram")
            || n.contains("recraft")
            || n.contains("nightcafe")
        {
            Self::ImageGen
        } else if n.contains("moondream")
            || n.contains("llava")
            || n.contains("vision")
            || n.contains("gemma3")
            // Multimodal LLM families that don't carry "vision" in the name.
            // Listed individually so plain text-only variants (e.g. "qwen2"
            // without -vl) don't get misclassified.
            || n.contains("qwen2-vl") || n.contains("qwen2.5-vl") || n.contains("qwen3-vl")
            || n.contains("internvl") || n.contains("intern-vl")
            || n.contains("minicpm-v")
            || n.contains("molmo")
            || n.contains("pixtral")
            || n.contains("phi3-vision") || n.contains("phi-3-vision")
            || n.contains("phi-4-vision")
            || n.contains("cogvlm")
            || n.contains("glm-4v")
            || n.contains("yi-vl")
            || n.contains("paligemma")
            || n.contains("mobilevlm")
            || n.contains("fuyu-")
            || n.contains("idefics")
        {
            Self::Vision
        } else {
            Self::Text
        }
    }

    /// Short display label for the header badge.
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Text     => "Text",
            Self::Vision   => "Vision",
            Self::ImageGen => "Image Gen",
            Self::VideoGen => "Video Gen",
            Self::AudioTts => "Audio (TTS)",
            Self::AudioAsr => "Audio (ASR)",
        }
    }

    /// Hint text for the chat input box. Tailored per modality so the
    /// user knows what kind of input the model expects.
    pub(crate) fn input_hint(&self) -> &'static str {
        match self {
            Self::Text     => "Type your message...",
            Self::Vision   => "Type a message (attach images via the Attach button)...",
            Self::ImageGen => "Describe the image you want to generate...",
            Self::VideoGen => "Video generation not yet supported by this server build",
            Self::AudioTts => "Type the text to synthesize as speech...",
            // Be explicit that typed text is IGNORED for ASR — the
            // server's chat handler passes initial_prompt: None to
            // Whisper (see handlers.rs ~2805), so anything in the
            // input field has no effect on the transcription.
            // Without this clarification, users would type setup
            // notes or context, hit Send, and be surprised that the
            // text didn't appear in Whisper's output.
            Self::AudioAsr => "Attach an audio file (Attach button) — typed text is ignored, only the attachment is transcribed",
        }
    }

    /// Format remaining seconds as a compact "left" string used on the
    /// image-gen progress bar. Pulled out of render_streaming_message
    /// so the formatting can be unit-tested independently of egui.
    ///
    /// Behaviour:
    /// - <60s  → "Ns" (ceil so a sub-second remainder never renders as 0s)
    /// - ≥60s  → "Nm SSs" (floor minutes, truncated seconds — MM:SS feel)
    pub(crate) fn format_eta_remaining(remaining_secs: f32) -> String {
        if remaining_secs < 0.0 {
            return "0s".to_string();
        }
        if remaining_secs >= 60.0 {
            let m = (remaining_secs / 60.0).floor() as u32;
            let s = (remaining_secs % 60.0) as u32;
            format!("{}m {:02}s", m, s)
        } else {
            format!("{}s", remaining_secs.ceil() as u32)
        }
    }

    /// Label shown in the typing-indicator bubble while a response
    /// is being prepared. Modality-aware so users see "Generating
    /// image..." instead of the generic "Thinking..." that text
    /// chat uses.
    pub(crate) fn typing_label(&self) -> &'static str {
        match self {
            Self::Text     => "Thinking...",
            Self::Vision   => "Analysing image...",
            Self::ImageGen => "Preparing image generation...",
            Self::VideoGen => "Preparing video generation...",
            Self::AudioTts => "Preparing audio synthesis...",
            Self::AudioAsr => "Transcribing audio...",
        }
    }

    /// Tooltip text for the modality badge — spells out input/output
    /// shape so users hovering see exactly what the model expects.
    pub(crate) fn tooltip(&self) -> &'static str {
        match self {
            Self::Text     => "Text → text. Standard chat / completion model.",
            Self::Vision   => "Image + text → text. Accepts image attachments alongside the prompt.",
            Self::ImageGen => "Text → image. Prompt becomes an image (PNG returned in the response).",
            Self::VideoGen => "Text → video. Server doesn't yet have a video-gen runtime; label only.",
            Self::AudioTts => "Text → audio. Synthesizes spoken audio from the prompt text.",
            Self::AudioAsr => "Audio → text. Transcribes attached audio into text.",
        }
    }
}

/// Truncate a string to fit inside a label, appending "..." if cut.
///
/// `max_bytes` is the maximum output byte length (typical label
/// widths land at 25-30 bytes for ASCII model names). When the
/// input fits, returns it unchanged. When it doesn't, takes the
/// first `(max_bytes - 3)` bytes — clamped to the nearest char
/// boundary so a multibyte UTF-8 model name (e.g. with non-Latin
/// publisher segments from HuggingFace) doesn't panic the GUI on
/// slice-at-non-char-boundary.
///
/// Borrow when the input fits; allocate the `"<prefix>..."` only
/// when truncation is actually needed.
pub(crate) fn truncate_with_ellipsis(s: &str, max_bytes: usize) -> std::borrow::Cow<'_, str> {
    if s.len() <= max_bytes {
        return std::borrow::Cow::Borrowed(s);
    }
    let target = max_bytes.saturating_sub(3);
    let mut safe = target.min(s.len());
    while safe > 0 && !s.is_char_boundary(safe) {
        safe -= 1;
    }
    std::borrow::Cow::Owned(format!("{}...", &s[..safe]))
}

// Chat input layout constants — shared between render's output-area
// reservation and render_input_area's TextEdit desired_rows so both
// always agree on how tall the input may grow. Single source of truth
// prevents a future tweak in one place from drifting from the other.
//
// ROW_PX matches egui's default body font row height at zoom = 1.0;
// small overshoot on dense fonts is fine — erring on extra reservation
// is preferable to clipping input. BASE_CHROME accounts for the input
// frame margins + the Send-button row beneath the TextEdit.
pub(crate) const CHAT_INPUT_ROW_PX: f32 = 18.0;
pub(crate) const CHAT_INPUT_BASE_CHROME_PX: f32 = 46.0;
pub(crate) const CHAT_INPUT_ATTACH_STRIP_PX: f32 = 64.0;
pub(crate) const CHAT_INPUT_MAX_ROWS: usize = 8;

/// Bounds for the image-gen num_steps override slider. 1 is the
/// absolute minimum the schedulers will accept (any lower would
/// emit 0 timesteps). 50 is conservative for the supported turbo
/// models (Flux Schnell tops out around 8, Z-Image around 20) but
/// leaves headroom for users probing higher-step distilled checkpoints.
pub(crate) const IMAGE_NUM_STEPS_MIN: u32 = 1;
pub(crate) const IMAGE_NUM_STEPS_MAX: u32 = 50;
/// Slider value displayed when the user hasn't committed an override
/// yet. The actual sent value is whatever the server's per-model
/// default resolves to (Flux 4 / Z-Image 9); this placeholder just
/// gives the slider a sensible mid-range start so the first drag
/// lands somewhere reasonable.
pub(crate) const IMAGE_NUM_STEPS_PLACEHOLDER: u32 = 8;

/// The name a generated artefact is offered under, everywhere it is offered.
///
/// One shape for every kind, because the alternative was what this replaced: images and
/// audio carried a timestamp while video, GIF and MIDI did not - so every clip was
/// `media_1.mp4` and each render silently overwrote the last one the user had saved.
///
/// `stamp` comes from the caller so every file of ONE render shares it: generating it per
/// file lets a batch straddle a second boundary and split across two names.
pub(crate) fn output_filename(kind: &str, stamp: &str, index: usize, ext: &str) -> String {
    format!("atelier_{kind}_{stamp}_{}.{ext}", index + 1)
}

/// The stamp [`output_filename`] takes: local time, sortable, no separators that a file
/// dialog or a shell would argue about.
pub(crate) fn output_stamp() -> String {
    chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
}

/// Hard cap on chat-tab attachment file size. Mirrors the server-side
/// IMAGE_INPUT_MAX_BYTES / AUDIO_INPUT_MAX_BYTES. The pre-flight check at attach time
/// saves a large upload and a server round-trip on rejection: the user sees the "too
/// large" error inline before they hit Send. It MUST NOT be stricter than the server,
/// or the GUI refuses files the server would have taken.
pub(crate) const CHAT_ATTACHMENT_MAX_BYTES: u64 = 512 * 1024 * 1024;

/// Mirror of the server's TTS_INPUT_MAX_CHARS = 50_000 (sentence-
/// tts-1 documented cap). Used by the chat-tab send path to
/// pre-flight oversized TTS input so the user sees the rejection
/// inline before .input.clear() drops the text into the void.
pub(crate) const TTS_INPUT_MAX_CHARS_CLIENT: usize = 50_000;

/// Format an image-gen progress chunk as the `Step <C>/<N>` string
/// `parse_image_step_progress` expects to parse back. Used by
/// `app.rs::send_chat` to synthesise streaming_text from the
/// structured `completed`/`total` fields the server sends. Pulling
/// the format into a helper means the chat-tab parser and the
/// synthesiser can't drift — the round-trip test pins it.
pub(crate) fn format_image_step_progress(completed: u64, total: u64) -> String {
    format!("Step {completed}/{total}")
}

/// Single source of truth for the (modality, input, attachment)
/// piece of "can the user Send right now?". Shared between three
/// surfaces so none can drift on which combinations the chat tab
/// considers sendable:
///
/// - the Send button's enabled state (`render_input_area` in this file)
/// - the Enter-key handler in app.rs (`CentralPanel::update`)
/// - the `send_chat` runtime guard in app.rs
///
/// The two button/Enter sites layer additional gates on top of this
/// helper before flipping `send_enabled` true:
///
/// - `model_selected` (`models.selected_model.is_some()`) — disable
///   up-front when no model is picked so the user doesn't click
///   into a "No model selected" error post-send (commit a6b27a1).
/// - `!chat.is_generating` — the in-flight gate; matches the Stop
///   button swap so a second send can't queue.
///
/// Both of those are universal gates that don't depend on modality,
/// so they live at the call sites instead of in this helper.
///
/// Per-modality rules:
/// - VideoGen: never sendable (server has no video runtime;
///   gate proactively so the user gets a tooltip, not a 500).
/// - AudioAsr: sendable when an attachment is staged. Text is
///   optional context — the audio IS the payload.
/// - Everything else: sendable when text is non-empty.
pub(crate) fn chat_send_allowed(
    modality: ModelModality,
    input_empty: bool,
    has_attachment: bool,
) -> bool {
    match modality {
        ModelModality::VideoGen => false,
        ModelModality::AudioAsr => has_attachment,
        _ => !input_empty,
    }
}

/// Format `CHAT_ATTACHMENT_MAX_BYTES` as a human-readable string
/// for the rejection message. Kept in MB to match the
/// server-side error wording so the two surfaces use the same
/// units.
pub(crate) fn format_attachment_cap_mb() -> String {
    format!("{} MB", CHAT_ATTACHMENT_MAX_BYTES / (1024 * 1024))
}

/// Bounds for the img2img strength override slider. Strength is a
/// 0..=1 mix coefficient (1.0 = full re-roll, 0.0 = preserve the
/// input). Server clamps to this range on intake; the slider matches
/// so the displayed value always equals the value actually applied.
pub(crate) const IMAGE_STRENGTH_MIN: f32 = 0.0;
pub(crate) const IMAGE_STRENGTH_MAX: f32 = 1.0;
/// Placeholder mid-point matching the server's chat-path default
/// (0.4) — a balanced "noticeable but not destructive" starting
/// point for the first drag. Other server routes default to 0.3 or
/// 0.75; the GUI picks the most common chat-path value.
pub(crate) const IMAGE_STRENGTH_PLACEHOLDER: f32 = 0.4;

/// Bounds for the TTS speed slider. Matches the server-side
/// `/v1/audio/speech` validated range so the GUI can't ever
/// surface a value the server would reject. Server default = 1.0.
pub(crate) const TTS_SPEED_MIN: f32 = 0.25;
pub(crate) const TTS_SPEED_MAX: f32 = 4.0;
pub(crate) const TTS_SPEED_DEFAULT: f32 = 1.0;

/// OpenAI-shaped TTS voice presets surfaced in the chat-tab voice
/// dropdown. Mirrors the server-side KNOWN_VOICES list (see
/// handlers.rs::KNOWN_VOICES). The server maps each preset name
/// into a Parler-TTS voice_description prompt via
/// `openai_voice_to_description`.
pub(crate) const TTS_VOICE_PRESETS: &[&str] = &[
    "alloy", "echo", "fable", "onyx", "nova", "shimmer",
    "ash", "ballad", "coral", "sage", "verse",
];

/// Common image-gen output sizes surfaced in the Size dropdown.
/// All entries satisfy the server's multiple-of-16 + <= 2048
/// boundary checks. The (label, w, h) shape feeds directly into
/// egui::ComboBox so adding a new preset is one line. Mix of
/// square + 16:9 + 9:16 + portrait covers the common asks
/// without overwhelming the dropdown.
pub(crate) const IMAGE_SIZE_PRESETS: &[(&str, u32, u32)] = &[
    ("512² (Flux default)",     512,  512),
    ("768²",                    768,  768),
    ("1024² (Z-Image default)", 1024, 1024),
    ("1280×720 (16:9 HD)",      1280, 720),
    ("720×1280 (9:16 portrait)", 720, 1280),
    ("1152×896 (4:3 landscape)", 1152, 896),
    ("896×1152 (3:4 portrait)",  896, 1152),
];

/// Image extensions the chat input accepts. Kept in sync with the
/// rfd filter at the Attach button site (~963). lowercase-only —
/// path_is_image_ext normalises the input via eq_ignore_ascii_case
/// so file names with mixed case still match.
/// ONE list of image extensions for every picker in the app.
///
/// It was three: the chat picker offered GIF, the media studio's two did not, and none
/// offered TIFF - which the server reads. A user could edit a GIF frame through
/// chat and not choose the same file in a media panel.
pub(crate) const CHAT_IMAGE_EXTS: &[&str] =
    &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff"];

/// Audio extensions accepted for ASR (whisper-style) model input.
/// Used by the rfd attach filter — the user-facing subtitle text
/// uses a curated string ("WAV / MP3 / FLAC / OGG / M4A / AAC") that
/// matches this list visually but isn't generated from it because
/// some extensions have nicer capitalised forms ("WebP", not "WEBP")
/// and JPG/JPEG would render as duplicates from a naive format.
pub(crate) const CHAT_AUDIO_EXTS: &[&str] = &["wav", "mp3", "flac", "ogg", "m4a", "aac"];

/// True when the given file path has an extension matching the chat
/// input's image set. Allocation-free — previous implementation called
/// .to_ascii_lowercase() per attached file per render, allocating a
/// fresh String every frame for the attachment-chip extension check.
/// At 60 fps × N attachments the alloc rate adds up; eq_ignore_ascii_case
/// compares in place without any heap traffic.
pub(crate) fn path_is_image_ext(path: &str) -> bool {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    CHAT_IMAGE_EXTS.iter().any(|known| ext.eq_ignore_ascii_case(known))
}

/// Parse a streaming-buffer line of the form "Step C/N" emitted by
/// the server's image-gen progress callbacks. Returns Some((c, n))
/// when both halves parse as non-zero usize, otherwise None.
///
/// Extracted from the inline parse in app.rs::update so the wire-
/// format contract is testable (and so a future server-side format
/// tweak — e.g. "step X / N" — is caught by the test suite instead
/// of silently degrading the progress bar to "Step ??".
pub(crate) fn parse_image_step_progress(buf: &str) -> Option<(usize, usize)> {
    let rest = buf.strip_prefix("Step ")?;
    let (c_str, n_str) = rest.split_once('/')?;
    let completed: usize = c_str.parse().ok()?;
    let total: usize = n_str.parse().ok()?;
    // Reject 0-total — would cause divide-by-zero downstream and is
    // never a sensible image-gen step total.
    if total == 0 {
        return None;
    }
    Some((completed, total))
}

/// Approximate characters per visual row at the GUI's default font
/// and window width. Used as the wrap budget when estimating how many
/// rows a long no-newline paste will occupy. Conservative — real value
/// depends on window width, font, and glyph mix. Under-estimating
/// inflates the reservation, which is the safe failure mode.
const CHAT_INPUT_WRAP_CHARS_PER_ROW: usize = 80;

/// Compute the visible-row count for the chat input TextEdit + the
/// output-area reservation. Both call sites must agree to avoid layout
/// flicker.
///
/// Two ways content occupies multiple rows, and they STACK:
///
/// 1. Explicit newlines — each `\n` ends one logical line.
/// 2. Soft wrapping on long lines — each logical line whose char
///    count exceeds the wrap budget contributes
///    `ceil(chars / wrap_budget)` rows instead of 1.
///
/// The visible row count is `sum_over_lines(line_wraps)` clamped to
/// `[1, CHAT_INPUT_MAX_ROWS]`. This matches the TextEdit's actual
/// vertical layout: a mix of short + wrapping lines reserves
/// `short_count + wrap_count_of_each_long` rows, not the max of the
/// two — see test `visible_rows_sums_per_line_wraps`.
///
/// Character width is counted via `chars().count()`, which treats
/// each Unicode scalar as one wrap-budget unit. CJK / wide glyphs
/// render as 2 columns in monospace fonts, so the estimate slightly
/// under-counts for those — acceptable since under-estimating
/// inflates the reservation (the safe failure mode: extra blank
/// space below the input instead of a clipped TextEdit).
pub(crate) fn chat_input_visible_rows(input: &str) -> usize {
    // Sum the per-line wrap count, not max — when input has multiple
    // logical lines AND some of them wrap, the per-line wraps stack.
    //
    // The previous implementation used `max(logical_lines, longest_line_wraps)`
    // which under-estimated. For e.g. one 150-char line + a short
    // second line:
    //   logical_lines=2, longest_wraps=ceil(150/80)=2, max(2,2)=2
    //   actual visual rows = 2 (wraps of line 1) + 1 (line 2) = 3
    // The TextEdit would reserve 2 rows of vertical space, then
    // overflow inside an internal scrollbar — the GUI's layout
    // expectation (reserved input height in render()) got the wrong
    // number too, producing a small visual hiccup on long pastes.
    //
    // Per-line wrap = ceil(chars / wrap_budget), clamped to at least
    // 1 (an empty line still counts as one visual row).
    let total_visual_rows: usize = input
        .split('\n')
        .map(|l| l.chars().count().div_ceil(CHAT_INPUT_WRAP_CHARS_PER_ROW).max(1))
        .sum();
    total_visual_rows.clamp(1, CHAT_INPUT_MAX_ROWS)
}

/// Classify a system-role message as an error vs an info notice.
///
/// The chat layer prefixes most failures with "Error:" (app.rs ~860 /
/// 896 / streaming-loop fallback at 624); file-write failures from the
/// worker-dialog drain use "Failed to save..." (chat_tab ~363 / 383).
/// Both should render with the red error styling rather than the muted
/// yellow info-notice styling.
///
/// Conservative on "Failed " — requires the trailing space so generic
/// content containing "Failed" mid-sentence doesn't accidentally
/// trigger error styling.
pub(crate) fn is_error_system_message(content: &str) -> bool {
    content.starts_with("Error:") || content.starts_with("Failed ")
}

/// Extract a `[seed: <u64>]` prefix from an assistant message produced
/// by the image-gen pipeline, if present.
///
/// Returns `(Some(seed), rest)` where `rest` is the message body with
/// the seed-prefix stripped (including any leading whitespace it
/// leaves behind). Returns `(None, original)` for non-image messages
/// or any unparseable shape.
///
/// The server (see api/handlers.rs commit 27cbef3) starts every
/// image-gen Ollama-compat response with `"[seed: 12345]"` so chat
/// callers can extract + copy the exact seed for a deterministic
/// re-roll. The GUI uses this helper to lift the seed into a
/// dedicated badge instead of leaking the bracketed text into the
/// rendered bubble.
pub(crate) fn parse_seed_prefix(content: &str) -> (Option<u64>, &str) {
    let trimmed = content.trim_start();
    let after_lbracket = match trimmed.strip_prefix("[seed:") {
        Some(s) => s,
        None => return (None, content),
    };
    let close_idx = match after_lbracket.find(']') {
        Some(i) => i,
        None => return (None, content),
    };
    let body = after_lbracket[..close_idx].trim();
    let seed: u64 = match body.parse() {
        Ok(n) => n,
        Err(_) => return (None, content),
    };
    // Re-derive the rest from the original `content` slice so the
    // returned &str borrows from the caller's input, not from a
    // freshly-trimmed view.
    let consumed = content.len() - after_lbracket.len() + close_idx + 1;
    let rest = content[consumed..].trim_start();
    (Some(seed), rest)
}

/// Format the per-message timing line shown under each assistant
/// bubble. Two shapes:
///   - Token-bearing responses (chat / vision): "N.N tok/s · M tokens · T.Ts"
///   - Token-less responses  (image-gen / TTS): "T.Ts"   or "T ms" for <1s
///
/// Image-gen and TTS responses arrive with token_count = 0 because
/// the server returns image/audio bytes, not text tokens. Showing
/// "0.0 tok/s | 0 tokens" alongside the real duration is misleading
/// — collapse to just the duration in that case.
pub(crate) fn format_timing_line(timing: MessageTiming) -> String {
    let secs = timing.duration_ms as f32 / 1000.0;
    let dur = if timing.duration_ms < 1000 {
        format!("{} ms", timing.duration_ms)
    } else {
        format!("{:.1}s", secs)
    };
    if timing.token_count == 0 {
        dur
    } else {
        format!(
            "{:.1} tok/s \u{00B7} {} tokens \u{00B7} {}",
            timing.tokens_per_sec, timing.token_count, dur,
        )
    }
}

/// Quick content-type sniff on the first few bytes of a base64 string.
/// Decodes just the prefix needed to identify the format — avoids a
/// full base64 decode for the (no-op) common case of redrawing already-
/// rendered attachments. Returns true when the bytes match a known
/// image signature (PNG/JPEG/GIF/BMP/WebP). WAV is also a RIFF
/// container so we disambiguate by checking the 4-byte form tag at
/// offset 8.
pub(crate) fn base64_looks_like_image(b64: &str) -> bool {
    use base64::Engine;
    // First 16 b64 chars decode to 12 bytes — exactly enough for the
    // RIFF/WAVE/WEBP form-tag check. Slicing on a char boundary is
    // safe since b64 uses ASCII-only alphabet.
    let prefix_len = b64.len().min(16);
    let prefix = &b64[..prefix_len];
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(prefix) else {
        return false;
    };
    match bytes.as_slice() {
        // PNG: 89 50 4E 47 0D 0A 1A 0A
        [0x89, 0x50, 0x4E, 0x47, ..] => true,
        // JPEG: FF D8 FF
        [0xFF, 0xD8, 0xFF, ..] => true,
        // GIF: 'GIF8'
        [0x47, 0x49, 0x46, 0x38, ..] => true,
        // BMP: 'BM'
        [0x42, 0x4D, ..] => true,
        // RIFF container — disambiguate via the form tag at offset 8.
        // WEBP = image, WAVE / anything else = not image.
        [0x52, 0x49, 0x46, 0x46, _, _, _, _, b8, b9, b10, b11, ..] => {
            matches!([*b8, *b9, *b10, *b11], [0x57, 0x45, 0x42, 0x50])
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_image_step_progress ↔ parse_image_step_progress ────
    // The chat-tab parser and the app.rs synthesiser format the same
    // "Step C/N" wire format. Round-trip through the formatter then
    // the parser MUST recover the original integers — any drift on
    // spacing / capitalisation / separator breaks the chat tab's
    // progress bar without a compile error.

    #[test]
    fn image_step_progress_format_parse_round_trip() {
        for (c, n) in [(0u64, 8u64), (1, 30), (15, 50), (49, 50), (200, 200)] {
            let formatted = format_image_step_progress(c, n);
            let parsed = parse_image_step_progress(&formatted)
                .unwrap_or_else(|| panic!("formatter output {formatted:?} failed to parse"));
            assert_eq!(parsed, (c as usize, n as usize),
                "round-trip drift: formatted={formatted:?} parsed={parsed:?} expected=({c}, {n})");
        }
    }

    // ── chat_send_allowed ──────────────────────────────────────────
    // Single source of truth between the Send button gate and the
    // send_chat runtime guard. A drift here would surface as either
    // "button enabled, click does nothing" OR "button disabled, but
    // pressing Enter still sent" — both confusing.

    #[test]
    fn chat_send_allowed_text_modalities_require_non_empty_text() {
        for m in [
            ModelModality::Text,
            ModelModality::Vision,
            ModelModality::ImageGen,
            ModelModality::AudioTts,
        ] {
            // Empty text → not sendable, regardless of attachment.
            assert!(!chat_send_allowed(m, /*input_empty=*/ true, false),
                "{m:?} empty + no attachment must block Send");
            assert!(!chat_send_allowed(m, /*input_empty=*/ true, true),
                "{m:?} empty + with attachment still requires text");
            // Non-empty text → sendable.
            assert!(chat_send_allowed(m, /*input_empty=*/ false, false),
                "{m:?} non-empty text must be sendable");
        }
    }

    #[test]
    fn chat_send_allowed_asr_needs_attachment_not_text() {
        // ASR: audio is the payload, text is optional context. Pin
        // that the gate flips on attachment presence, not text.
        assert!(!chat_send_allowed(
            ModelModality::AudioAsr, /*input_empty=*/ true,  /*has_att=*/ false));
        assert!(!chat_send_allowed(
            ModelModality::AudioAsr, /*input_empty=*/ false, /*has_att=*/ false));
        assert!(chat_send_allowed(
            ModelModality::AudioAsr, /*input_empty=*/ true,  /*has_att=*/ true));
        assert!(chat_send_allowed(
            ModelModality::AudioAsr, /*input_empty=*/ false, /*has_att=*/ true));
    }

    #[test]
    fn chat_send_allowed_videogen_never_sendable() {
        // VideoGen has no server runtime; block proactively so the
        // user gets a tooltip instead of a 500.
        for input_empty in [true, false] {
            for has_att in [true, false] {
                assert!(!chat_send_allowed(ModelModality::VideoGen, input_empty, has_att),
                    "VideoGen must always block; input_empty={input_empty}, has_att={has_att}");
            }
        }
    }

    // ── format_attachment_cap_mb ────────────────────────────────────
    // The chat-tab attach pre-flight rejects oversized files BEFORE
    // upload. The rejection message references the cap in MB so the
    // user knows what to compress to. Pin the format so a future
    // bump of CHAT_ATTACHMENT_MAX_BYTES surfaces consistently.

    #[test]
    fn attachment_cap_message_is_mb_unit() {
        let msg = format_attachment_cap_mb();
        assert!(msg.ends_with(" MB"),
            "cap message must end with ' MB'; got {msg:?}");
        // The unit's number must round-trip from the constant.
        let expected = format!("{} MB", CHAT_ATTACHMENT_MAX_BYTES / (1024 * 1024));
        assert_eq!(msg, expected);
    }

    #[test]
    fn tts_input_max_chars_client_matches_server() {
        // Server-side TTS_INPUT_MAX_CHARS = 50_000 (sentence-chunked
        // tts-1). The GUI's pre-flight cap MUST mirror it — tighter
        // GUI cap silently rejects server-acceptable input; looser
        // cap lets users type up to the GUI limit only to be
        // rejected by the server. Both are bad UX.
        assert_eq!(TTS_INPUT_MAX_CHARS_CLIENT, 50_000,
            "GUI cap must mirror server-side TTS_INPUT_MAX_CHARS = 4096");
    }

    #[test]
    fn attachment_cap_matches_server_image_input_max() {
        // The server's IMAGE_INPUT_MAX_BYTES and AUDIO_INPUT_MAX_BYTES are both 512 MB.
        // The GUI's CHAT_ATTACHMENT_MAX_BYTES MUST mirror that - a tighter GUI cap
        // silently rejects files the server would accept, which is how the GUI ends up
        // refusing an ordinary photograph; a looser one defeats the pre-flight check.
        assert_eq!(CHAT_ATTACHMENT_MAX_BYTES, 512 * 1024 * 1024,
            "GUI attachment cap must mirror server IMAGE/AUDIO_INPUT_MAX_BYTES = 512 MB");
    }

    // ── IMAGE_SIZE_PRESETS ─────────────────────────────────────────
    // Static-content test pinning that every dropdown preset satisfies
    // the server's validate_image_dimensions boundary check (multiple
    // of 16, non-zero, within IMAGE_MAX_DIM each axis). Without this a new preset
    // (e.g. "1366x768 widescreen" — 1366 is NOT a multiple of 16)
    // would surface as a 400 from the server only when the user
    // actually picked it, which is a confusing UX.

    #[test]
    fn image_size_presets_satisfy_server_boundaries() {
        const MAX_DIM: u32 = 8192; // server IMAGE_MAX_DIM
        const VAE_ALIGN: u32 = 16;
        assert!(!IMAGE_SIZE_PRESETS.is_empty(),
            "at least one preset must be offered so the dropdown isn't empty");
        for &(label, w, h) in IMAGE_SIZE_PRESETS {
            assert!(w > 0 && h > 0,
                "preset {label:?} has zero dim: {w}x{h}");
            assert_eq!(w % VAE_ALIGN, 0,
                "preset {label:?} width {w} must be a multiple of {VAE_ALIGN} (VAE stride)");
            assert_eq!(h % VAE_ALIGN, 0,
                "preset {label:?} height {h} must be a multiple of {VAE_ALIGN} (VAE stride)");
            assert!(w <= MAX_DIM && h <= MAX_DIM,
                "preset {label:?} ({w}x{h}) exceeds server MAX_DIM={MAX_DIM}");
        }
    }

    // ── TTS presets ────────────────────────────────────────────────
    // Drift guards: the chat-tab voice dropdown lists exactly the
    // names the server's KNOWN_VOICES validator will accept. Speed
    // slider bounds mirror /v1/audio/speech's validated [0.25, 4.0]
    // range. A drift here surfaces as either a chat send 400'd by
    // the server (loose GUI) or a useful preset hidden from the
    // user (tight GUI).

    #[test]
    fn tts_voice_presets_match_server_known_voices_set() {
        // Hard-coded snapshot of handlers.rs::KNOWN_VOICES. If the
        // server adds / removes a preset, this test catches it.
        let server_known: &[&str] = &[
            "alloy", "echo", "fable", "onyx", "nova", "shimmer",
            "ash", "ballad", "coral", "sage", "verse",
        ];
        let mut gui: Vec<&str> = TTS_VOICE_PRESETS.to_vec();
        let mut srv: Vec<&str> = server_known.to_vec();
        gui.sort();
        srv.sort();
        assert_eq!(gui, srv,
            "chat-tab TTS_VOICE_PRESETS must mirror server KNOWN_VOICES exactly");
    }

    #[test]
    fn tts_speed_bounds_match_server_validated_range() {
        // /v1/audio/speech rejects speed outside [0.25, 4.0]. The
        // GUI slider must stay within that envelope.
        assert_eq!(TTS_SPEED_MIN, 0.25);
        assert_eq!(TTS_SPEED_MAX, 4.0);
        // Default lands at the server's documented neutral pace.
        assert_eq!(TTS_SPEED_DEFAULT, 1.0);
    }

    #[test]
    fn image_size_presets_include_server_per_model_defaults() {
        // Each server-side image_model_defaults entry has a documented
        // default size. Surface those in the dropdown so the user can
        // always pick "the model's native default" explicitly (the
        // "(default)" option means "let the server pick", which is
        // different from "pick the right per-model default
        // deliberately"). Missing either would force the user to
        // override or accept the silent server pick — not the same.
        let dims: Vec<(u32, u32)> = IMAGE_SIZE_PRESETS
            .iter()
            .map(|(_, w, h)| (*w, *h))
            .collect();
        assert!(dims.contains(&(512, 512)),
            "preset list must include Flux default 512x512; got {dims:?}");
        assert!(dims.contains(&(1024, 1024)),
            "preset list must include Z-Image default 1024x1024; got {dims:?}");
    }

    #[test]
    fn image_size_presets_labels_are_distinct() {
        // Two presets with the same label would collide in the
        // ComboBox and one would be unreachable.
        let mut labels: Vec<&str> = IMAGE_SIZE_PRESETS.iter().map(|(l, _, _)| *l).collect();
        labels.sort();
        let unique_count = {
            let mut deduped = labels.clone();
            deduped.dedup();
            deduped.len()
        };
        assert_eq!(unique_count, labels.len(),
            "preset labels must be unique; got {labels:?}");
    }

    // ── ModelModality::from_model_name ──

    #[test]
    fn modality_text_default() {
        // Unknown / generic model names fall through to Text.
        assert_eq!(ModelModality::from_model_name("llama3:8b"),    ModelModality::Text);
        assert_eq!(ModelModality::from_model_name("mistral:7b"),   ModelModality::Text);
        assert_eq!(ModelModality::from_model_name("phi3:latest"),  ModelModality::Text);
        assert_eq!(ModelModality::from_model_name(""),             ModelModality::Text);
    }

    #[test]
    fn modality_audio_tts_detection() {
        // Must keep parity with server-side is_tts_model so the GUI
        // routes TTS models to handle_chat_tts on the server. Missing
        // a pattern here means the chat tab shows a generic Text UI
        // for a TTS model and the user gets a confusing routing error.
        for name in [
            "parler-tts/parler-tts-mini-v1",
            "parler-tts-large-v1",
            "tts-1",
            "tts-1-hd",
            "openai/tts-1",   // path-prefixed — matches `/tts-` substring
            "openai/tts-1-hd",
            "suno/bark",
            "hexgrad/kokoro",
            "ai4bharat/f5-tts",
            "fish-speech-base",
            "metavoice-1B-v0.1",
        ] {
            assert_eq!(
                ModelModality::from_model_name(name),
                ModelModality::AudioTts,
                "{name} should classify as AudioTts",
            );
        }
        // Negatives — text/chat/vision/asr/image models must NOT
        // misclassify as AudioTts.
        for name in ["qwen3-coder:latest", "Tongyi-MAI/Z-Image-Turbo",
                     "openai/whisper-small", "llava:7b", ""] {
            assert_ne!(
                ModelModality::from_model_name(name),
                ModelModality::AudioTts,
                "{name} must not be AudioTts",
            );
        }
    }

    #[test]
    fn modality_vision_detection() {
        // Vision-tagged names route to Vision (moondream, llava-*, gemma3,
        // *vision*, plus multimodal LLM families that don't carry "vision"
        // in the name).
        for name in [
            "moondream:1.8b",
            "llava:7b",
            "gemma3:4b",
            "custom-vision-model",
            // Multimodal families, so the chat input offers its image-attach button
            // for them instead of falling back to the text-only layout.
            "Qwen/Qwen2-VL-7B",
            "Qwen/Qwen2.5-VL-7B-Instruct",
            "Qwen/Qwen3-VL-30B",
            "OpenGVLab/InternVL2-8B",
            "OpenGVLab/Intern-VL3",
            "openbmb/MiniCPM-V-2_6",
            "allenai/Molmo-7B-D",
            "mistralai/Pixtral-12B",
            "microsoft/Phi-3-vision-128k-instruct",
            "microsoft/Phi-4-vision",
            "THUDM/cogvlm2-llama3-chat-19B",
            "THUDM/glm-4v-9b",
            "01-ai/Yi-VL-6B",
            "google/paligemma-3b-mix-448",
            "mtgv/MobileVLM-3B",
            "adept/fuyu-8b",
            "HuggingFaceM4/idefics2-8b",
        ] {
            assert_eq!(
                ModelModality::from_model_name(name),
                ModelModality::Vision,
                "{name} should classify as Vision",
            );
        }
        // Negatives — generic text models that share a substring but
        // shouldn't be misclassified.
        for name in [
            "qwen2:7b",          // bare qwen2, not -vl
            "qwen2.5-coder:7b",  // coder variant, not -vl
            "phi-3:14b",         // plain phi-3, not vision
            "llama3:8b",         // plain llama, not vision-tagged
        ] {
            assert_ne!(
                ModelModality::from_model_name(name),
                ModelModality::Vision,
                "{name} should NOT classify as Vision",
            );
        }
    }

    #[test]
    fn modality_image_gen_detection() {
        for name in [
            "flux-schnell",
            "flux-dev:fp16",
            "z-image-turbo",
            "zimage",
            "sd-xl",
            "stable-diffusion:1.5",
            "stable-cascade",
            "wuerstchen",
            "mm-dit-large",
            "kandinsky-2.2",
            "pixart-sigma",
            "playground-v2.5",
            "kolors-v1",
            "lumina-next",
            "hidream-l1",
            "openjourney-v4",
            "dall-e-3",
            "dalle-3",
            // Hosted-only families, listed so their labels match the local ones.
            "midjourney-v6",
            "imagen-3",
            "ideogram-v2",
            "recraft-v3",
            "nightcafe-classic",
        ] {
            assert_eq!(
                ModelModality::from_model_name(name),
                ModelModality::ImageGen,
                "expected ImageGen for {name}",
            );
        }
    }

    #[test]
    fn modality_audio_detection() {
        assert_eq!(ModelModality::from_model_name("whisper-large-v3"), ModelModality::AudioAsr);
        assert_eq!(ModelModality::from_model_name("whisper:tiny"),     ModelModality::AudioAsr);
        assert_eq!(ModelModality::from_model_name("parler-tts-mini"),  ModelModality::AudioTts);
        assert_eq!(ModelModality::from_model_name("custom-tts"),       ModelModality::AudioTts);
        assert_eq!(ModelModality::from_model_name("tts-1"),            ModelModality::AudioTts);
        assert_eq!(ModelModality::from_model_name("bark-small"),       ModelModality::AudioTts);
        assert_eq!(ModelModality::from_model_name("kokoro-v0.19"),     ModelModality::AudioTts);
        assert_eq!(ModelModality::from_model_name("f5-tts-base"),      ModelModality::AudioTts);
        assert_eq!(ModelModality::from_model_name("fish-speech-1.4"),  ModelModality::AudioTts);
        assert_eq!(ModelModality::from_model_name("metavoice-1b"),     ModelModality::AudioTts);
    }

    #[test]
    fn modality_video_detection() {
        // Video-gen patterns route to VideoGen across the full set the
        // detector knows about: the long-standing ones (sora / veo / cogvideo /
        // videocrafter / video-diffusion / wan-) plus the open-source and hosted
        // families added since, so every label matches.
        for name in [
            "sora-1", "veo-2", "cogvideo-x", "videocrafter-2",
            "stable-video-diffusion", "wan-video-1",
            "Tencent/HunyuanVideo", "tencent/hunyuan-video-13b",
            "genmoai/mochi-1-preview",
            "Lightricks/LTX-Video",
            "rhymes-ai/Allegro",
            "stepfun-ai/Step-Video-T2V",
            "hpcai-tech/OpenSora-STDiT-v1",
            "modelscope/text-to-video-synthesis",
            "runway-gen-3-alpha", "runway/gen-3-turbo",
            "pika-1.5", "haiper-2",
        ] {
            assert_eq!(
                ModelModality::from_model_name(name),
                ModelModality::VideoGen,
                "expected VideoGen for {name}",
            );
        }
    }

    #[test]
    fn modality_video_beats_image_when_both_substrings_present() {
        // videocrafter contains 'sd' adjacencies and would have been
        // misclassified as ImageGen without the priority ordering.
        assert_eq!(
            ModelModality::from_model_name("sd-videocrafter"),
            ModelModality::VideoGen,
        );
    }

    #[test]
    fn modality_imagen_is_not_videogen() {
        // A loose 'gen-3' substring matches imagen-3, a text-to-image model, and
        // routes it to VideoGen because VideoGen is tried first. The rule is
        // 'runway-gen' / 'runway/gen-' for that reason. Keep imagen-N pinned to
        // ImageGen so a future relaxation can't silently regress.
        for name in ["imagen-3", "imagen-2", "Imagen-3", "google/imagen-3"] {
            assert_eq!(
                ModelModality::from_model_name(name),
                ModelModality::ImageGen,
                "{name} must classify as ImageGen, not VideoGen",
            );
        }
    }

    #[test]
    fn modality_priority_audio_over_vision() {
        // whisper-with-vision (hypothetical) — whisper check fires first,
        // pinning the priority so ASR isn't accidentally routed as Vision
        // if both substrings ever co-occur.
        assert_eq!(
            ModelModality::from_model_name("whisper-vision-fusion"),
            ModelModality::AudioAsr,
        );
    }

    #[test]
    fn modality_is_case_insensitive() {
        // Detection lowercases before matching so model registries
        // with mixed-case display names work.
        assert_eq!(ModelModality::from_model_name("FLUX-Schnell"),    ModelModality::ImageGen);
        assert_eq!(ModelModality::from_model_name("WHISPER:large"),   ModelModality::AudioAsr);
        assert_eq!(ModelModality::from_model_name("Moondream:1.8b"),  ModelModality::Vision);
    }

    // ── base64_looks_like_image ──
    //
    // Encoded prefixes correspond to:
    //   PNG header   : 89 50 4E 47 0D 0A 1A 0A 00 00 ...
    //   JPEG header  : FF D8 FF E0 00 10 ...
    //   GIF87a       : 47 49 46 38 37 61 ...
    //   BMP          : 42 4D ...
    //   WebP (RIFF)  : 52 49 46 46 .. .. .. .. 57 45 42 50 ...
    //   WAV  (RIFF)  : 52 49 46 46 .. .. .. .. 57 41 56 45 ...

    fn b64_of(bytes: &[u8]) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn sniff_png() {
        let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert!(base64_looks_like_image(&b64_of(&bytes)));
    }

    #[test]
    fn sniff_jpeg() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, 0x4A, 0x46, 0, 0, 0, 0];
        assert!(base64_looks_like_image(&b64_of(&bytes)));
    }

    #[test]
    fn sniff_gif() {
        let bytes = *b"GIF87aXX";
        assert!(base64_looks_like_image(&b64_of(&bytes)));
    }

    #[test]
    fn sniff_bmp() {
        let bytes = *b"BMxxxxxxxxxx";
        assert!(base64_looks_like_image(&b64_of(&bytes)));
    }

    #[test]
    fn sniff_webp_is_image() {
        // RIFF + offset-8 'WEBP' → image.
        let mut bytes = [0u8; 12];
        bytes[..4].copy_from_slice(b"RIFF");
        bytes[8..].copy_from_slice(b"WEBP");
        assert!(base64_looks_like_image(&b64_of(&bytes)));
    }

    #[test]
    fn sniff_wav_is_not_image() {
        // RIFF + offset-8 'WAVE' → NOT image (audio).
        let mut bytes = [0u8; 12];
        bytes[..4].copy_from_slice(b"RIFF");
        bytes[8..].copy_from_slice(b"WAVE");
        assert!(!base64_looks_like_image(&b64_of(&bytes)));
    }

    #[test]
    fn sniff_unknown_is_not_image() {
        // Plain text / MP3 / FLAC / other binary → not classified as image.
        assert!(!base64_looks_like_image(&b64_of(b"hello world\n\x00")));
        assert!(!base64_looks_like_image(&b64_of(b"fLaC\0\0\0\0\0\0\0\0"))); // FLAC
        assert!(!base64_looks_like_image(&b64_of(b"ID3\x03\x00\x00\x00\x00\x00\x00\x00\x00"))); // MP3
        assert!(!base64_looks_like_image(&b64_of(b"OggS\x00\x00\x00\x00\x00\x00\x00\x00")));  // OGG
    }

    #[test]
    fn sniff_malformed_base64_is_not_image() {
        // Invalid base64 must return false (not panic).
        assert!(!base64_looks_like_image("not-valid-base64-!!!"));
        assert!(!base64_looks_like_image(""));
    }

    // ── ModelModality::format_eta_remaining ──

    #[test]
    fn eta_sub_minute_ceils_seconds() {
        // <60s remains in seconds, ceiled so 0.4s never displays as 0s.
        assert_eq!(ModelModality::format_eta_remaining(0.4),  "1s");
        assert_eq!(ModelModality::format_eta_remaining(1.0),  "1s");
        assert_eq!(ModelModality::format_eta_remaining(7.4),  "8s");
        assert_eq!(ModelModality::format_eta_remaining(59.9), "60s");
    }

    #[test]
    fn eta_minute_plus_uses_mm_ss() {
        // ≥60s uses "Nm SSs" with zero-padded seconds.
        assert_eq!(ModelModality::format_eta_remaining(60.0),  "1m 00s");
        assert_eq!(ModelModality::format_eta_remaining(95.0),  "1m 35s");
        assert_eq!(ModelModality::format_eta_remaining(125.7), "2m 05s");
        assert_eq!(ModelModality::format_eta_remaining(3661.0), "61m 01s");
    }

    #[test]
    fn eta_negative_clamps_to_zero() {
        // Negative remaining is a math artifact (per_step * 0 underflow
        // when the start instant is in the future, e.g. clock drift) —
        // never render a negative duration.
        assert_eq!(ModelModality::format_eta_remaining(-1.0), "0s");
        assert_eq!(ModelModality::format_eta_remaining(-0.001), "0s");
    }

    // ── format_timing_line ──

    fn timing(tok_s: f32, count: u32, ms: u64) -> MessageTiming {
        MessageTiming { tokens_per_sec: tok_s, duration_ms: ms, token_count: count }
    }

    #[test]
    fn timing_line_with_tokens_uses_full_breakdown() {
        // Text/vision responses: tok/s · count · duration. Middot
        // separator (U+00B7) not pipe — matches the rest of the GUI.
        assert_eq!(
            format_timing_line(timing(42.5, 128, 3010)),
            "42.5 tok/s \u{00B7} 128 tokens \u{00B7} 3.0s"
        );
    }

    #[test]
    fn timing_line_zero_tokens_collapses_to_duration_only() {
        // Image-gen / TTS responses arrive with token_count = 0 — the
        // tok/s and count fields would be misleading "0.0 tok/s | 0
        // tokens" noise alongside the real duration.
        assert_eq!(format_timing_line(timing(0.0, 0, 4_500)),  "4.5s");
        assert_eq!(format_timing_line(timing(0.0, 0, 12_300)), "12.3s");
    }

    #[test]
    fn timing_line_sub_second_uses_ms() {
        // Under 1 second, seconds-with-tenths loses meaningful
        // precision ("0.1s" for 87 ms is worse than "87 ms").
        assert_eq!(format_timing_line(timing(0.0, 0, 87)),  "87 ms");
        assert_eq!(format_timing_line(timing(0.0, 0, 999)), "999 ms");
        // Token-bearing path uses ms too.
        assert_eq!(
            format_timing_line(timing(50.0, 4, 80)),
            "50.0 tok/s \u{00B7} 4 tokens \u{00B7} 80 ms"
        );
    }

    #[test]
    fn timing_line_boundary_at_one_second_switches_to_seconds() {
        // ≥1000 ms tips into the "T.Ts" form.
        assert_eq!(format_timing_line(timing(0.0, 0, 1000)), "1.0s");
    }

    // ── is_error_system_message ──

    #[test]
    fn error_prefix_triggers_error_styling() {
        // chat error path (app.rs ~860) and image-gen error path
        // (app.rs ~896) both use this exact prefix.
        assert!(is_error_system_message("Error: connection refused"));
        assert!(is_error_system_message("Error: 500 Internal Server Error"));
        assert!(is_error_system_message("Error:"));  // bare prefix still classifies
    }

    #[test]
    fn failed_prefix_triggers_error_styling() {
        // chat_tab ~363 (single save) and ~383 (multi save) prefixes.
        assert!(is_error_system_message("Failed to save /tmp/x.png: Permission denied"));
        assert!(is_error_system_message("Failed to save 3 files to /tmp:\n  ..."));
        // TTS Play-button error pushed by render() when
        // play_audio_blob fails (no audio backend on PATH, etc.).
        // Pin this prefix so a future reword can't silently drop
        // back to neutral system styling — the user needs the red
        // tint to notice the playback failed.
        assert!(is_error_system_message(
            "Failed to play audio: no audio player found on PATH. \
             Install paplay/aplay/afplay or use the Save button and \
             play the WAV in an external app."
        ));
    }

    #[test]
    fn info_notices_dont_trigger_error_styling() {
        // Benign system messages should keep the muted yellow info
        // styling — not be promoted to red.
        assert!(!is_error_system_message("No model selected"));
        assert!(!is_error_system_message("Model loaded"));
        assert!(!is_error_system_message(""));
    }

    #[test]
    fn failed_word_mid_sentence_is_not_error() {
        // Trailing-space gate on "Failed " prevents false positives
        // from generic prose containing the word "Failed" mid-sentence.
        assert!(!is_error_system_message("FailedRequest is a class name"));
        assert!(!is_error_system_message("Failure mode: ..."));
    }

    // ── chat_input_visible_rows ──

    #[test]
    fn visible_rows_empty_is_one() {
        // Empty input still reserves one row so the field doesn't
        // collapse to zero-height.
        assert_eq!(chat_input_visible_rows(""), 1);
    }

    #[test]
    fn visible_rows_single_line_is_one() {
        assert_eq!(chat_input_visible_rows("hello"), 1);
        assert_eq!(chat_input_visible_rows("a long single line with no newlines"), 1);
    }

    #[test]
    fn visible_rows_counts_newlines_plus_one() {
        // N newlines = N+1 logical rows.
        assert_eq!(chat_input_visible_rows("a\nb"),       2);
        assert_eq!(chat_input_visible_rows("a\nb\nc"),    3);
        assert_eq!(chat_input_visible_rows("a\nb\nc\nd"), 4);
    }

    #[test]
    fn visible_rows_clamps_at_max() {
        // Past MAX (8), additional newlines have no effect — overflow
        // is handled by TextEdit's internal scrollbar.
        let huge: String = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        assert_eq!(chat_input_visible_rows(&huge), CHAT_INPUT_MAX_ROWS);
        // 7 newlines = 8 rows = at the cap exactly.
        let at_cap = "1\n2\n3\n4\n5\n6\n7\n8";
        assert_eq!(chat_input_visible_rows(at_cap), CHAT_INPUT_MAX_ROWS);
    }

    #[test]
    fn visible_rows_estimates_wrapping_on_long_no_newline_paste() {
        // A 200-char single line wraps to ceil(200 / 80) = 3 rows.
        // Without the wrap estimate the helper would say 1 row and the
        // TextEdit would overflow past the reservation.
        let long_line: String = "x".repeat(200);
        assert_eq!(chat_input_visible_rows(&long_line), 3);

        // Exactly at the wrap budget = 1 row (no wrap).
        let exact: String = "x".repeat(CHAT_INPUT_WRAP_CHARS_PER_ROW);
        assert_eq!(chat_input_visible_rows(&exact), 1);

        // One char past the budget = 2 rows.
        let just_over: String = "x".repeat(CHAT_INPUT_WRAP_CHARS_PER_ROW + 1);
        assert_eq!(chat_input_visible_rows(&just_over), 2);

        // Wrap math also caps at MAX_ROWS — a 10_000-char paste
        // doesn't request 125 rows.
        let huge: String = "x".repeat(10_000);
        assert_eq!(chat_input_visible_rows(&huge), CHAT_INPUT_MAX_ROWS);
    }

    #[test]
    fn visible_rows_sums_per_line_wraps() {
        // 3 logical lines: short + 300-char (wraps to 4) + short.
        // Correct reserved visual rows = 1 + 4 + 1 = 6, NOT
        // max(logical=3, longest_wraps=4) = 4. The previous
        // implementation used max() which silently underestimated
        // multi-line inputs that contained any wrapping line —
        // the TextEdit would reserve too little vertical space
        // and the rest would scroll inside a too-small input box.
        let mixed = format!("short\n{}\nshort", "x".repeat(300));
        assert_eq!(chat_input_visible_rows(&mixed), 6);

        // Two wrapping lines: 90-char (wraps 2) + 200-char (wraps 3)
        // = 5 total rows.
        let two_wraps = format!("{}\n{}", "y".repeat(90), "z".repeat(200));
        assert_eq!(chat_input_visible_rows(&two_wraps), 5);

        // The sum still clamps at MAX_ROWS. 9 short lines + a
        // wrapping line would naively sum to >MAX; we cap at MAX.
        let overflow = format!("{}\n{}", "a\n".repeat(9), "b".repeat(300));
        assert_eq!(chat_input_visible_rows(&overflow), CHAT_INPUT_MAX_ROWS);
    }

    // ── parse_seed_prefix ──
    //
    // Server-side commit 27cbef3 emits `[seed: <u64>] ...` at the
    // start of image-gen Ollama responses. The GUI lifts that
    // prefix into a copyable badge instead of leaving it in the
    // rendered bubble. parse_seed_prefix is the extraction step.

    #[test]
    fn parse_seed_prefix_extracts_seed_and_trims_rest() {
        let (seed, rest) = parse_seed_prefix("[seed: 12345]");
        assert_eq!(seed, Some(12345));
        assert_eq!(rest, "");
        let (seed, rest) = parse_seed_prefix("[seed: 999] some body");
        assert_eq!(seed, Some(999));
        assert_eq!(rest, "some body");
    }

    #[test]
    fn parse_seed_prefix_tolerates_surrounding_whitespace() {
        // Leading whitespace before [seed: must be skipped.
        let (seed, rest) = parse_seed_prefix("   [seed: 7] ok");
        assert_eq!(seed, Some(7));
        assert_eq!(rest, "ok");
    }

    #[test]
    fn parse_seed_prefix_handles_large_u64_seeds() {
        // Server picks via rand_u64 — full u64 range. Pin that
        // big seeds round-trip (a regression to as i32 would
        // silently truncate).
        let big: u64 = 18_446_744_073_709_551_614; // u64::MAX - 1
        let s = format!("[seed: {big}] body");
        let (seed, _) = parse_seed_prefix(&s);
        assert_eq!(seed, Some(big));
    }

    #[test]
    fn parse_seed_prefix_returns_none_for_non_image_messages() {
        // Regular text responses must not be mis-parsed.
        let (seed, rest) = parse_seed_prefix("Hello, how can I help?");
        assert_eq!(seed, None);
        assert_eq!(rest, "Hello, how can I help?");
        // Empty.
        let (seed, rest) = parse_seed_prefix("");
        assert_eq!(seed, None);
        assert_eq!(rest, "");
    }

    #[test]
    fn parse_seed_prefix_returns_none_on_malformed_prefix() {
        // Various near-misses — none should match.
        assert_eq!(parse_seed_prefix("[seed: abc]").0, None);
        assert_eq!(parse_seed_prefix("[seed: 123").0, None,
            "missing close bracket → no match");
        assert_eq!(parse_seed_prefix("[Seed: 123]").0, None,
            "case-sensitive: 'Seed' is not 'seed'");
        assert_eq!(parse_seed_prefix("[seed: -5]").0, None,
            "negative not a valid u64");
    }

    // ── parse_image_step_progress ──

    #[test]
    fn parses_well_formed_step_lines() {
        assert_eq!(parse_image_step_progress("Step 1/30"),  Some((1, 30)));
        assert_eq!(parse_image_step_progress("Step 15/50"), Some((15, 50)));
        assert_eq!(parse_image_step_progress("Step 0/4"),   Some((0, 4)));
        assert_eq!(parse_image_step_progress("Step 100/100"), Some((100, 100)));
    }

    #[test]
    fn rejects_non_step_lines() {
        // Anything that isn't the exact "Step C/N" shape returns None.
        assert_eq!(parse_image_step_progress(""), None);
        assert_eq!(parse_image_step_progress("Loading T5..."), None);
        assert_eq!(parse_image_step_progress("step 1/30"), None); // case-sensitive
        assert_eq!(parse_image_step_progress("Step 1"),     None); // no slash
        assert_eq!(parse_image_step_progress("Step /30"),   None); // empty numerator
        assert_eq!(parse_image_step_progress("Step 1/"),    None); // empty denominator
        assert_eq!(parse_image_step_progress("Step a/b"),   None); // non-numeric
    }

    #[test]
    fn rejects_zero_total_divide_by_zero_guard() {
        // Total of 0 would cause downstream divide-by-zero in the
        // ETA / percent math — reject at the parser instead of
        // letting it slip through.
        assert_eq!(parse_image_step_progress("Step 1/0"), None);
        assert_eq!(parse_image_step_progress("Step 0/0"), None);
    }

    // ── path_is_image_ext ──

    #[test]
    fn path_is_image_ext_recognises_all_known_extensions() {
        for known in CHAT_IMAGE_EXTS {
            let path = format!("/tmp/screenshot.{}", known);
            assert!(path_is_image_ext(&path), "expected image: {}", path);
        }
    }

    #[test]
    fn path_is_image_ext_is_case_insensitive() {
        // Mixed-case extensions (common from Windows / macOS screenshots).
        assert!(path_is_image_ext("/tmp/Photo.PNG"));
        assert!(path_is_image_ext("/tmp/image.JpEg"));
        assert!(path_is_image_ext("/tmp/clip.WebP"));
    }

    #[test]
    fn path_is_image_ext_rejects_non_images() {
        assert!(!path_is_image_ext("/tmp/clip.wav"));
        assert!(!path_is_image_ext("/tmp/audio.mp3"));
        assert!(!path_is_image_ext("/tmp/text.txt"));
        assert!(!path_is_image_ext("/tmp/noext"));
        assert!(!path_is_image_ext(""));
    }

    #[test]
    fn path_is_image_ext_handles_path_without_directory() {
        // Bare filename — no directory prefix.
        assert!(path_is_image_ext("photo.png"));
        assert!(!path_is_image_ext("song.mp3"));
    }

    // ── ModelModality::typing_label ──

    #[test]
    fn typing_label_distinct_per_modality() {
        // Each modality must produce a distinct typing label so users
        // know what kind of generation is starting. Collect and dedup
        // — set size should equal arm count.
        let labels: std::collections::HashSet<&'static str> = [
            ModelModality::Text,
            ModelModality::Vision,
            ModelModality::ImageGen,
            ModelModality::VideoGen,
            ModelModality::AudioTts,
            ModelModality::AudioAsr,
        ]
        .iter()
        .map(|m| m.typing_label())
        .collect();
        assert_eq!(labels.len(), 6, "every modality should have a unique typing label");
    }

    #[test]
    fn typing_label_text_baseline() {
        // Default text chat uses the generic "Thinking..." that other
        // chat UIs converge on; image-gen variant should not.
        assert_eq!(ModelModality::Text.typing_label(),     "Thinking...");
        assert_ne!(ModelModality::ImageGen.typing_label(), "Thinking...");
        assert_ne!(ModelModality::AudioTts.typing_label(), "Thinking...");
    }

    // ── truncate_with_ellipsis ──

    #[test]
    fn truncate_with_ellipsis_passes_through_when_fits() {
        // Exactly at the cap → unchanged (Cow::Borrowed, no alloc).
        let s = truncate_with_ellipsis("exactly_27_chars__padding!!", 27);
        assert_eq!(s, "exactly_27_chars__padding!!");
        // Under the cap → unchanged.
        let s = truncate_with_ellipsis("short", 30);
        assert_eq!(s, "short");
        // Empty → empty.
        let s = truncate_with_ellipsis("", 10);
        assert_eq!(s, "");
    }

    #[test]
    fn truncate_with_ellipsis_truncates_with_three_dot_suffix_within_cap() {
        // 35-char input, cap=30 → take 27 + "..." = 30 chars total.
        let s = truncate_with_ellipsis("publisher/some_long_model_name_v2", 30);
        assert_eq!(s.len(), 30);
        assert!(s.ends_with("..."), "got: {s}");
        // Prefix is the first 27 bytes of the input.
        assert!(s.starts_with("publisher/some_long_model_n"), "got: {s}");
    }

    #[test]
    fn truncate_with_ellipsis_does_not_panic_on_multibyte_utf8() {
        // The previous inline `&model[..27]` could panic if byte 27
        // landed mid-codepoint on a HuggingFace publisher name with
        // non-Latin characters (or emoji in custom model names). Pin
        // the safety contract:
        //
        // "公開された/llama-very-long-model" — first char `公` is 3
        // bytes; byte 27 might or might not be on a boundary. The
        // helper must return cleanly either way.
        let s = truncate_with_ellipsis("公開された/llama-very-long-model-name-3000", 30);
        assert!(s.ends_with("..."));
        assert!(s.len() <= 30, "got len {}: {}", s.len(), s);
        // The prefix portion must still be valid UTF-8 (no broken
        // codepoint). Round-trip through str.
        let prefix: &str = s.strip_suffix("...").unwrap();
        assert!(prefix.is_char_boundary(prefix.len()));

        // Pure multibyte: each Japanese char = 3 bytes, 10 chars = 30 B.
        // Cap=20 → take 17 B → walk back to nearest boundary (15 = 5
        // full chars). Result: "あいうえお...".
        let s = truncate_with_ellipsis("あいうえおかきくけこ", 20);
        assert!(s.ends_with("..."));
        assert!(s.len() <= 20);

        // Cap=4 with multibyte input (target=1, but ñ starts at 0 and
        // occupies bytes 0-1; safe=0 → "" + "..." = "...").
        let s = truncate_with_ellipsis("ñañañañañañaña", 4);
        assert_eq!(s, "...", "should clamp to empty prefix + ellipsis");
    }

    // ── ModelModality::label / input_hint ──

    #[test]
    fn label_is_unique_per_modality() {
        // The header badge renders modality.label(). If two modalities
        // shared the same label, the user couldn't tell whether the
        // server is in image-gen mode or vision mode.
        let labels: std::collections::HashSet<&'static str> = [
            ModelModality::Text,
            ModelModality::Vision,
            ModelModality::ImageGen,
            ModelModality::VideoGen,
            ModelModality::AudioTts,
            ModelModality::AudioAsr,
        ]
        .iter()
        .map(|m| m.label())
        .collect();
        assert_eq!(labels.len(), 6, "every modality should have a unique badge label");
    }

    #[test]
    fn label_strings_match_user_facing_baseline() {
        // Pin the exact strings. These are what the user sees in the
        // header badge next to the model name. A silent rename is the
        // kind of UX drift that wire-shape tests catch for JSON.
        assert_eq!(ModelModality::Text.label(),     "Text");
        assert_eq!(ModelModality::Vision.label(),   "Vision");
        assert_eq!(ModelModality::ImageGen.label(), "Image Gen");
        assert_eq!(ModelModality::VideoGen.label(), "Video Gen");
        assert_eq!(ModelModality::AudioTts.label(), "Audio (TTS)");
        assert_eq!(ModelModality::AudioAsr.label(), "Audio (ASR)");
    }

    #[test]
    fn input_hint_is_modality_appropriate_and_unique() {
        // Each modality's hint guides the user to the right input
        // type. Distinct hints mean the chat box's affordance
        // reflects what the server actually does with the request.
        let hints: std::collections::HashSet<&'static str> = [
            ModelModality::Text,
            ModelModality::Vision,
            ModelModality::ImageGen,
            ModelModality::VideoGen,
            ModelModality::AudioTts,
            ModelModality::AudioAsr,
        ]
        .iter()
        .map(|m| m.input_hint())
        .collect();
        assert_eq!(hints.len(), 6, "every modality should have a unique input hint");

        // Pin one keyword per active modality so a reword doesn't
        // accidentally lose the user-facing affordance.
        assert!(ModelModality::Vision.input_hint().contains("image"));
        assert!(ModelModality::ImageGen.input_hint().contains("image"));
        assert!(ModelModality::AudioTts.input_hint().contains("speech"));
        let asr = ModelModality::AudioAsr.input_hint();
        assert!(asr.contains("audio"));
        // Must point to the Attach button: the chat tab takes speech through the
        // same affordance as vision models do images. An earlier hint told the user
        // to paste a path or call the API instead, which was misleading - the chat
        // tab IS that API surface.
        assert!(asr.contains("Attach"),
            "ASR hint must reference the Attach button; got: {asr}");
        // VideoGen explicitly tells the user it's not supported so a
        // future build doesn't silently invite a doomed request.
        let v = ModelModality::VideoGen.input_hint().to_ascii_lowercase();
        assert!(v.contains("not") || v.contains("supported"),
            "VideoGen hint should warn about lack of support; got: {v}");
    }

    // ── base64_looks_like_image ──

    fn b64(bytes: &[u8]) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn base64_image_detects_png_jpeg_gif_bmp() {
        // First few bytes of each real format header — anything past
        // the magic is irrelevant to the sniff.
        let png = b64(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0]);
        let jpeg = b64(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0, 0]);
        let gif = b64(b"GIF89a\0\0\0\0\0\0");
        let bmp = b64(&[0x42, 0x4D, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(base64_looks_like_image(&png),  "PNG header should sniff as image");
        assert!(base64_looks_like_image(&jpeg), "JPEG header should sniff as image");
        assert!(base64_looks_like_image(&gif),  "GIF header should sniff as image");
        assert!(base64_looks_like_image(&bmp),  "BMP header should sniff as image");
    }

    #[test]
    fn base64_image_disambiguates_riff_via_form_tag() {
        // RIFF container: 'RIFF' + 4-byte size + 4-byte form tag.
        let webp = b64(b"RIFF\0\0\0\0WEBPVP8 ");
        let wave = b64(b"RIFF\0\0\0\0WAVEfmt ");
        assert!(base64_looks_like_image(&webp), "RIFF/WEBP is an image");
        assert!(!base64_looks_like_image(&wave), "RIFF/WAVE is audio, not an image");
    }

    #[test]
    fn base64_image_rejects_text_and_invalid_b64() {
        // Plain ASCII text base64 — should not match any image magic.
        let plain = b64(b"hello world!!");
        assert!(!base64_looks_like_image(&plain));
        // Garbage that won't even base64-decode — must not panic, returns false.
        assert!(!base64_looks_like_image("!!!notb64!!!"));
        // Empty input — empty slice trivially has no magic.
        assert!(!base64_looks_like_image(""));
    }
    // -- output_filename ------------------------------------------------
    #[test]
    fn every_kind_is_named_the_same_way() {
        assert_eq!(output_filename("video", "20260802_014500", 0, "mp4"),
                   "atelier_video_20260802_014500_1.mp4");
        assert_eq!(output_filename("image", "20260802_014500", 4, "png"),
                   "atelier_image_20260802_014500_5.png");
        // The index is 1-based for a person reading a directory listing.
        assert!(output_filename("audio", "s", 0, "wav").ends_with("_1.wav"));
    }

    /// Two renders must not collide, which is the whole reason for the stamp - and two
    /// files of the SAME render must share it, which is why the caller supplies it.
    #[test]
    fn the_stamp_separates_renders_and_binds_a_batch() {
        let a = output_filename("video", "20260802_014500", 0, "mp4");
        let b = output_filename("video", "20260802_014501", 0, "mp4");
        assert_ne!(a, b, "two renders a second apart must not share a name");
        let one = output_stamp();
        assert_eq!(
            output_filename("image", &one, 0, "png").replace("_1.png", ""),
            output_filename("image", &one, 1, "png").replace("_2.png", ""),
            "files of one render share everything but the index"
        );
    }

    #[test]
    fn the_stamp_is_sortable_and_shell_safe() {
        let s = output_stamp();
        assert_eq!(s.len(), 15, "YYYYmmdd_HHMMSS: {s}");
        assert!(s.chars().all(|c| c.is_ascii_digit() || c == '_'), "{s}");
    }

}
