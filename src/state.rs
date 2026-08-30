//! GUI State Types
//!
//! State management types for different UI components.

use std::collections::VecDeque;



use crate::api::ModelInfo;





// (Legacy `Tab` enum removed — superseded by Section navigation in
// state.rs. The two remaining references in app.rs (current_tab field
// + its initializer) were the only live sites and they're going too.)

/// Navigation section for the new sidebar layout
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Chat,
    Terminal,
    Models,
    Settings,
    ServerLog,
    /// Media Studio — generate every media modality (image, music,
    /// SFX, MIDI, video, speech) from a single dedicated tab, hitting
    /// the server's `/v1/*` generation endpoints directly.
    MediaStudio,
}

// ============================================================================
// Connection State
// ============================================================================

/// Coarse connection state to the configured server.
///
/// An enum rather than a status string, because three sites read it - the
/// top-bar dot, the top-bar label and the Settings row - and classifying a
/// string with `.contains("Connected")` ladders means rewording the message
/// desyncs the colour from the label without anything failing. Colour and
/// label both derive from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    /// Not connected / offline / handshake failed.
    #[default]
    Disconnected,
    /// A connection attempt (list-models fetch) is in flight.
    Connecting,
    /// The last fetch succeeded.
    Connected,
}

impl ConnectionState {
    /// Status-dot colour, from the shared theme palette so the top bar,
    /// Settings row, and every other state dot speak the same language.
    pub fn color(self) -> eframe::egui::Color32 {
        match self {
            ConnectionState::Connected => crate::theme::SUCCESS,
            ConnectionState::Connecting => crate::theme::WARNING,
            ConnectionState::Disconnected => crate::theme::ERROR,
        }
    }

    /// Canonical short label for the status dot.
    pub fn label(self) -> &'static str {
        match self {
            ConnectionState::Connected => "Connected",
            ConnectionState::Connecting => "Connecting…",
            ConnectionState::Disconnected => "Offline",
        }
    }
}

/// Connection state plus a human-readable detail line - "Connected - 5 models,
/// 2 loaded", "Disconnected: Connection refused". The detail feeds tooltips and
/// the CLI `status` command; the state drives colour and label.
#[derive(Debug, Clone)]
pub struct ConnectionStatus {
    pub state: ConnectionState,
    pub detail: String,
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            detail: "Not connected".to_string(),
        }
    }
}

impl ConnectionStatus {
    pub fn connecting() -> Self {
        Self { state: ConnectionState::Connecting, detail: "Connecting…".to_string() }
    }
    pub fn connected(detail: impl Into<String>) -> Self {
        Self { state: ConnectionState::Connected, detail: detail.into() }
    }
    pub fn disconnected(detail: impl Into<String>) -> Self {
        Self { state: ConnectionState::Disconnected, detail: detail.into() }
    }
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Detail is the descriptive form; fall back to the enum label
        // if it's somehow empty.
        if self.detail.is_empty() {
            f.write_str(self.state.label())
        } else {
            f.write_str(&self.detail)
        }
    }
}

// ============================================================================
// Chat State
// ============================================================================

/// A single chat message
#[derive(Debug, Clone, Default)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
    /// Timing info for assistant responses (tokens/sec, duration_ms, token_count)
    pub timing: Option<MessageTiming>,
    /// Attached images (base64-encoded, for vision model input)
    pub images: Vec<String>,
    /// Generated images (base64-encoded PNG, from image generation models)
    pub generated_images: Vec<String>,
    /// Generated audio payloads (base64-encoded WAV, from TTS models).
    /// Populated when the server's handle_chat_tts returns. Rendered
    /// as a "Save audio" button in the chat tab.
    pub generated_audios: Vec<String>,
}

impl ChatMessage {
    /// Build a system message with the current timestamp and empty
    /// image/audio fields.
    ///
    /// System messages are emitted in ~19 sites for:
    ///   - empty-prompt / no-model-selected guidance
    ///   - "Generation cancelled" breadcrumbs (Esc, model swap, etc.)
    ///   - inline error notices (audio playback failed, attachment
    ///     too large, etc.)
    ///
    /// All share the same shape (role=system, vec![] media, no timing).
    /// The helper folds that boilerplate into one call.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
            timestamp: crate::timefmt::chat_now(),
            timing: None,
            images: Vec::new(),
            generated_images: Vec::new(),
            generated_audios: Vec::new(),
        }
    }
}

/// Performance metrics for a chat message
#[derive(Debug, Clone, Copy)]
pub struct MessageTiming {
    pub tokens_per_sec: f32,
    pub duration_ms: u64,
    pub token_count: u32,
}

/// Layer processing mode: determines which layers are used during inference.
///
/// Only two modes survive because they map to options the live request
/// path (`/api/chat`, `/v1/chat/completions`) actually honors:
///   - AllLayers — no extra options; the engine runs every layer.
///   - Adaptive — sends `early_exit_threshold: 0.1`; the engine may exit
///     early on high-confidence tokens.
///
/// There is deliberately no "CUDA only" mode. The flag it would send is not read
/// by any request handler - the underlying switch applies at load time - so the
/// toggle would be clickable and inert, which is a worse failure than stating the
/// limitation.
// serde derives so AppConfig can persist the user's last-picked
// layer mode across GUI launches. Without these, power users who
// explicitly chose Adaptive for perf would have to re-toggle on
// every startup (the chat-tab toggle's state lived only on
// ChatState, which is built fresh from Default each launch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum LayerMode {
    #[default]
    AllLayers,   // Process every layer (normal, highest quality)
    Adaptive,    // Exit early if confident (sends early_exit_threshold)
}

/// Chat tab state
#[derive(Debug, Clone, Default)]
pub struct ChatState {
    pub messages: VecDeque<ChatMessage>,
    pub input: String,
    pub is_generating: bool,
    /// Smart routing: send turns to /conversation, where the server picks the
    /// right model per prompt (rules + a tiny classifier LLM) — chat, vision,
    /// image generation or TTS — instead of the fixed selected model.
    pub smart_auto: bool,
    /// Server-side conversation identity for /conversation (keeps the routed
    /// working set warm across turns). Regenerated when the chat is cleared.
    pub conversation_id: String,
    /// AbortHandle for the in-flight generation task. `Some` while
    /// `is_generating` is true so a "Stop" button can interrupt the
    /// task without waiting for the server to finish. Cleared when
    /// the task completes naturally OR after a manual abort.
    /// Skipped from Debug/Clone via the existing #[derive(Default)]
    /// (Default::default() returns None which is the correct idle
    /// state).
    #[doc(hidden)]
    pub generation_abort: Option<tokio::task::AbortHandle>,
    /// Streaming content being generated (partial response)
    pub streaming_content: String,
    /// Layer processing mode: which layers to use
    pub layer_mode: LayerMode,
    /// Attached images for the next message (base64-encoded).
    ///
    /// **Invariant**: `attached_images.len() == attached_image_paths.len()`
    /// and the two vecs are indexed in parallel — `attached_images[i]`
    /// is the base64 payload for the file whose display path is
    /// `attached_image_paths[i]`. Mutations must always touch both
    /// vecs at the same index, or the chip-row render (which zips
    /// the two by index) will display the wrong filename / preview.
    /// Use the `add_attachment` / `remove_attachment` / `clear_attachments`
    /// helpers below to keep the invariant — direct mutation is still
    /// allowed for power users but loses the invariant guarantee.
    pub attached_images: Vec<String>,
    /// File paths of attached images (for display). See
    /// `attached_images` for the index-parallel invariant.
    pub attached_image_paths: Vec<String>,
    /// Image generation progress (completed, total steps)
    pub image_gen_progress: Option<(usize, usize)>,
    /// Wall-clock instant of the first observed progress update for
    /// the current image-gen request, used to compute ETA on the
    /// progress bar. Set when image_gen_progress transitions from
    /// None to Some; cleared when the request completes or fails.
    pub image_gen_started_at: Option<std::time::Instant>,
    /// Worker-thread channel for file dialogs (attach files, save image,
    /// save-all). rfd's sync API blocks the egui main thread, which
    /// deadlocks against the XDG portal on Linux (the portal needs the
    /// main loop alive to dispatch events to it). Each dialog click
    /// spawns a std::thread that writes its result into this slot;
    /// chat_tab::render drains it at the top of each frame.
    pub pending_dialog: std::sync::Arc<std::sync::Mutex<Option<ChatDialogResult>>>,
    /// True while a file-dialog worker thread is alive. Used to gate
    /// dialog-trigger buttons so a double-click can't spawn two
    /// overlapping dialogs (the second result would overwrite the
    /// first in pending_dialog, silently losing the user's pick).
    /// Set when a worker spawns, cleared when the worker exits.
    pub dialog_in_flight: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Past prompts the user has sent in this session, newest last.
    /// Used by the Up/Down keyboard shortcuts in the chat input to
    /// recall and tweak prior prompts — particularly useful when
    /// iterating on image-gen text where the user often wants to
    /// nudge a single word ("...at sunset" → "...at sunrise"). Bounded
    /// to PROMPT_HISTORY_LIMIT to avoid unbounded growth.
    pub prompt_history: Vec<String>,
    /// Current position when browsing prompt_history with Up/Down.
    ///
    /// - None: at the live edit buffer (no history nav active)
    /// - `Some(i)`: displaying `prompt_history[i]`
    ///
    /// Reset to None on send.
    pub history_cursor: Option<usize>,
    /// Snapshot of the in-progress input the user had typed before
    /// they pressed Up for the first time. Restored when they Down-
    /// scroll past the end so they don't lose what they were writing.
    pub history_draft: String,
    /// Locked seed for image-gen re-rolls. Set by the chat bubble's
    /// "Lock seed" button on a generated-image's seed badge;
    /// consumed (and cleared) on the very next chat send so the
    /// user gets exactly one re-roll per click. None means the
    /// server picks a fresh random seed (default behaviour).
    pub locked_seed: Option<u64>,
    /// User-set override for the image-gen `num_steps` parameter.
    /// `None` means "let the server pick its per-model default"
    /// (Flux Schnell: 4, Z-Image Turbo: 9). Sticky across sends —
    /// the user sets it once and every subsequent gen in the same
    /// chat uses it, until they Clear or manually reset. Reset by
    /// `clear_conversation` so a fresh chat starts at server defaults.
    pub image_num_steps: Option<u32>,
    /// User-set override for the img2img `strength` parameter.
    /// Only used when the user attaches an input image to an
    /// image-gen prompt (controls how much of the original survives:
    /// 0.0 = preserve original, 1.0 = full txt2img re-roll). `None`
    /// means "use the server's per-route default" (0.3 → 0.75). Same
    /// sticky-per-chat semantics as image_num_steps; reset on Clear.
    pub image_strength: Option<f32>,
    /// User-set override for image-gen output dimensions, stored as
    /// (width, height). `None` means "use the server's per-model
    /// default" (Flux 512², Z-Image 1024²). Same sticky-per-chat
    /// semantics as the other image_* overrides; reset on Clear.
    /// Server clamps dimensions to multiples of 16 in [16, 2048]
    /// — the GUI dropdown only offers values that already satisfy
    /// those bounds.
    pub image_size: Option<(u32, u32)>,
    /// TTS voice preset for chat synthesis (one of the OpenAI-shaped
    /// `KNOWN_VOICES` on the server: alloy / echo / fable / onyx /
    /// nova / shimmer / ash / ballad / coral / sage / verse). `None`
    /// means "let the server pick its default voice description".
    /// Threaded via `options.voice` on chat send when the loaded
    /// model is AudioTts modality. Sticky per-chat; reset on Clear.
    pub tts_voice: Option<String>,
    /// TTS speed multiplier in [0.25, 4.0]. `None` = 1.0 (server
    /// default). Applied server-side via time-domain resample so
    /// duration changes match `/v1/audio/speech`. Sticky per-chat;
    /// reset on Clear.
    pub tts_speed: Option<f32>,
}

/// Cap on prompt history retained per session.
pub const PROMPT_HISTORY_LIMIT: usize = 100;

impl ChatState {
    /// Push a freshly-sent prompt onto the history, deduplicating
    /// against the most-recent entry and capping at
    /// PROMPT_HISTORY_LIMIT (oldest evicted from the front).
    ///
    /// Also resets the navigation cursor + draft snapshot — the
    /// next Up press should start from the new head.
    ///
    /// Empty / whitespace-only input is silently ignored — defensive
    /// guard against future callers that don't pre-filter. Send_chat
    /// already gates at the call site but a Ctrl+history nav helper
    /// shouldn't store junk.
    pub fn push_prompt_history(&mut self, prompt: String) {
        let trimmed_is_empty = prompt.trim().is_empty();
        if !trimmed_is_empty && self.prompt_history.last() != Some(&prompt) {
            self.prompt_history.push(prompt);
            while self.prompt_history.len() > PROMPT_HISTORY_LIMIT {
                self.prompt_history.remove(0);
            }
        }
        self.history_cursor = None;
        self.history_draft.clear();
    }

    /// Move history cursor backward (older). Snapshots the live edit
    /// buffer into history_draft on the first move so the user can
    /// recover what they were writing via history_forward past the
    /// newest entry. No-op when the history is empty or already at
    /// the oldest entry.
    pub fn history_back(&mut self) {
        if self.prompt_history.is_empty() {
            return;
        }
        let n = self.prompt_history.len();
        let new_cursor = match self.history_cursor {
            None => {
                self.history_draft = self.input.clone();
                n - 1
            }
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.history_cursor = Some(new_cursor);
        self.input.clone_from(&self.prompt_history[new_cursor]);
    }

    /// Move history cursor forward (newer). When stepping past the
    /// newest entry, restores the live edit buffer from
    /// history_draft. No-op when history nav isn't active.
    pub fn history_forward(&mut self) {
        let Some(i) = self.history_cursor else { return };
        let n = self.prompt_history.len();
        if i + 1 < n {
            self.history_cursor = Some(i + 1);
            self.input.clone_from(&self.prompt_history[i + 1]);
        } else {
            self.history_cursor = None;
            self.input = std::mem::take(&mut self.history_draft);
        }
    }

    /// Detach from history navigation if the user has edited the
    /// recalled prompt — i.e. input no longer matches the entry the
    /// cursor points at. Without this, a subsequent history_forward
    /// would silently replace the user's edit with the next history
    /// entry.
    ///
    /// Called from the chat input render when the TextEdit reports a
    /// change. Returns true iff the detach actually happened (used
    /// only for testing observability — render path discards it).
    pub fn detach_history_if_edited(&mut self) -> bool {
        let Some(i) = self.history_cursor else { return false };
        if self.prompt_history.get(i) != Some(&self.input) {
            self.history_cursor = None;
            self.history_draft.clear();
            true
        } else {
            false
        }
    }

    /// Reset all per-conversation state when the user clears the chat:
    /// the message vec, any attached images, the streaming buffer, the
    /// image-gen progress trackers, and the locked seed. The locked
    /// seed in particular MUST reset — its UX entry point is a seed
    /// badge that lives in `messages`, so a survivor would silently
    /// bind the next send to a seed from a discarded conversation.
    ///
    /// Does NOT touch `prompt_history` (a session-scoped scrollback)
    /// or `input` (the live editor — the user may have already typed
    /// the next prompt before clicking Clear).
    /// The /conversation id, minted lazily per conversation (cleared by
    /// `clear_conversation`). A time-based id is unique enough for a
    /// single-user GUI session.
    pub fn conversation_id_or_new(&mut self) -> String {
        if self.conversation_id.is_empty() {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            self.conversation_id = format!("gui-{ms}");
        }
        self.conversation_id.clone()
    }

    pub fn clear_conversation(&mut self) {
        self.messages.clear();
        self.clear_attachments();
        self.streaming_content.clear();
        self.image_gen_progress = None;
        self.image_gen_started_at = None;
        self.locked_seed = None;
        self.image_num_steps = None;
        self.image_strength = None;
        self.image_size = None;
        self.tts_voice = None;
        self.tts_speed = None;
        // Defensive: Clear is only clickable when NOT generating, so
        // this should already be None. Reset anyway to guarantee no
        // stale handle survives any edge case (e.g. future race where
        // Clear becomes available mid-stream).
        if let Some(handle) = self.generation_abort.take() {
            handle.abort();
        }
        // Fresh server-side conversation identity for /conversation routing.
        self.conversation_id = String::new();
    }

    /// Push one (base64 payload, display path) pair onto the two
    /// parallel attachment vecs in lockstep. Maintains the
    /// `attached_images.len() == attached_image_paths.len()` invariant
    /// documented on the field — call sites that go through this
    /// helper can't accidentally desync the vecs.
    pub fn add_attachment(&mut self, b64: String, path: String) {
        self.attached_images.push(b64);
        self.attached_image_paths.push(path);
    }

    /// Remove the attachment at `idx` from both parallel vecs.
    /// No-op if `idx` is out of bounds (defensive — the caller is
    /// usually a button-click handler that captured the index in
    /// the previous frame, and the underlying vec may have shrunk
    /// since). Returns `true` if a removal actually happened.
    pub fn remove_attachment(&mut self, idx: usize) -> bool {
        if idx >= self.attached_images.len() || idx >= self.attached_image_paths.len() {
            return false;
        }
        self.attached_images.remove(idx);
        self.attached_image_paths.remove(idx);
        true
    }

    /// Drop all staged attachments in lockstep. Used by the
    /// "remove all attachments" chip + by clear_conversation.
    pub fn clear_attachments(&mut self) {
        self.attached_images.clear();
        self.attached_image_paths.clear();
    }

    /// Move the staged base64 attachments out (for the outgoing
    /// chat request) and simultaneously clear the parallel paths
    /// vec, leaving both attachment vecs empty in lockstep.
    /// The caller owns the returned `Vec<String>` to forward to the
    /// HTTP worker without an extra clone.
    pub fn take_attachments(&mut self) -> Vec<String> {
        let taken = std::mem::take(&mut self.attached_images);
        self.attached_image_paths.clear();
        taken
    }

    /// Mark a new generation as in-flight: clear the four
    /// post-completion streaming fields (streaming_content,
    /// image_gen_progress, image_gen_started_at, generation_abort)
    /// to their idle values then flip is_generating = true. The
    /// AbortHandle gets assigned separately AFTER the task spawn
    /// (the spawn returns the handle to abort).
    ///
    /// Defensive reset: even if the previous generation never
    /// reached its TaskResult handler (process killed mid-stream,
    /// network drop, etc.), the next send starts from a clean
    /// slate — no stale progress bar / ETA / streaming text.
    ///
    /// Mirror of reset_streaming_state: that helper covers end-of-
    /// generation (is_generating → false), this one covers start-
    /// of-generation (is_generating → true). Both touch the same
    /// field set so a future field addition has to be wired into
    /// both — pair them at code-review time.
    pub fn prepare_for_generation(&mut self) {
        self.generation_abort = None;
        self.streaming_content.clear();
        self.image_gen_progress = None;
        self.image_gen_started_at = None;
        self.is_generating = true;
    }

    /// Reset all six streaming-state fields to their idle defaults.
    /// Called from both the user-cancellation path (abort_generation
    /// — also calls .abort() on the taken handle) and the natural-
    /// completion paths in app.rs (ChatResponse / ImageGenResponse
    /// handlers — these don't need the .abort() call since the task
    /// has already produced its result).
    ///
    /// Factoring this out of abort_generation keeps the six-field
    /// reset in exactly one place — the two completion handlers
    /// (and the user-abort path) can't drift on which fields get
    /// cleared.
    pub fn reset_streaming_state(&mut self) {
        self.generation_abort = None;
        self.is_generating = false;
        self.streaming_content.clear();
        self.image_gen_progress = None;
        self.image_gen_started_at = None;
    }

    /// Abort an in-flight generation and reset the streaming state so
    /// the UI can immediately accept the next prompt. Returns `true`
    /// if there was a live generation to cancel, `false` if the call
    /// was a no-op (chat already idle). Callers that want a
    /// user-visible breadcrumb (the Stop button, the Esc shortcut)
    /// push a "[Generation cancelled by user]" system message on
    /// `true`; callers that just want to be sure no stale handle
    /// outlives an event (e.g. window-close on a future shutdown
    /// path) can ignore the return.
    ///
    /// Centralised here — not inlined at each call site — so the
    /// six-step cleanup (abort + clear streaming + clear progress +
    /// drop started_at + flip is_generating + drop handle) can't
    /// drift between the Stop button and the Esc-key shortcut.
    pub fn abort_generation(&mut self) -> bool {
        let was_generating = self.is_generating;
        // Pull the handle out BEFORE reset_streaming_state clears it
        // (the reset is destructive — we'd lose the .abort() target).
        let handle = self.generation_abort.take();
        self.reset_streaming_state();
        if let Some(h) = handle {
            h.abort();
        }
        was_generating
    }
}

/// File-dialog result produced on a worker thread and consumed by
/// chat_tab::render. Each variant carries the data needed to complete
/// the operation on the GUI thread.
#[derive(Debug, Clone)]
pub enum ChatDialogResult {
    /// User picked one or more files to attach. Each entry in `files`
    /// is a (file_path, base64_encoded_bytes) pair — base64 encoding
    /// runs on the worker thread so a multi-MB image upload doesn't
    /// stall the egui frame for ~tens of ms while the GUI thread
    /// encodes. `oversized` carries (file_path, bytes_len) for any
    /// files rejected because they exceed CHAT_ATTACHMENT_MAX_BYTES;
    /// the chat tab surfaces those as system messages so the user
    /// sees the rejection inline instead of getting a server-side
    /// PAYLOAD_TOO_LARGE round-trip away.
    AttachFiles {
        files: Vec<(std::path::PathBuf, String)>,
        oversized: Vec<(std::path::PathBuf, u64)>,
        /// Files that couldn't be read at all (permission denied,
        /// broken symlink, deleted between dialog-confirm and the
        /// worker's read). Carries the io::Error message so the
        /// chat tab can surface "Failed to read X: Y" inline
        /// instead of the worker silently dropping the entry —
        /// drag-drop already does this (commit 9f3e1ac); the
        /// file-picker worker had the same silent-fail pattern.
        unreadable: Vec<(std::path::PathBuf, String)>,
    },
    /// User picked a save target for a single image. Bytes were
    /// snapshotted at click time so the worker can write directly
    /// without re-touching the message vec.
    SaveBytes {
        path: std::path::PathBuf,
        bytes: Vec<u8>,
    },
    /// User picked a folder to dump multiple images into. The vec
    /// holds (filename, bytes) pairs.
    SaveBytesMany {
        dir: std::path::PathBuf,
        files: Vec<(String, Vec<u8>)>,
    },
    /// Media Studio audio pick. The worker
    /// thread runs BOTH the blocking rfd dialog and the `fs::read` —
    /// the sync dialog would deadlock the egui thread against the XDG
    /// portal on Linux, and reading a multi-MB clip would stall a
    /// frame. `bytes` is Err(message) when the read failed (deleted /
    /// unreadable between pick and read) so the tab can surface it.
    MediaAudio {
        slot: MediaAudioSlot,
        name: String,
        bytes: Result<Vec<u8>, String>,
    },
}

/// Which Media Studio audio slot a worker-thread file pick targets.
/// Carried inside `ChatDialogResult::MediaAudio` so the single shared
/// `pending_dialog` channel can serve all three picker rows: both the
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaAudioSlot {
    /// `MediaState::image_edit.source` — the image to edit (image file filter).
    EditImage,
    /// `MediaState::transcribe.audio` — the clip to transcribe.
    TranscribeAudio,
    /// `MediaState::sfx.init_audio` — the source clip for a Stable Audio
    /// audio-to-audio variation.
    SfxInit,
    /// A pose or edge image that constrains WHERE things go.
    ImageControl,
    /// `MediaState::separate.audio` - the mix to split into stems.
    SeparateAudio,
    /// `MediaState::video.start_image` - the frame an image-to-video checkpoint
    /// CONTINUES. Not a style hint: that model renders a clip that starts from this
    /// picture, and without one it has nothing to continue and says so.
    VideoStartImage,
}

impl MediaAudioSlot {
    /// Does this slot take an IMAGE rather than an audio clip?
    ///
    /// Asked of the slot rather than inferred by the picker. A picker that names the
    /// image slots it knows offers an audio filter and a "Choose audio..." button for
    /// every slot added after it, in silence. Answering here means a new slot must
    /// declare its kind, and `every_slot_declares_its_kind` fails when it does not.
    pub fn is_image(self) -> bool {
        match self {
            Self::EditImage | Self::ImageControl | Self::VideoStartImage => true,
            Self::TranscribeAudio | Self::SfxInit | Self::SeparateAudio => false,
        }
    }

    /// A stable name for this slot, used to key cached previews.
    ///
    /// Derived from the variant rather than from its position, so reordering the enum
    /// cannot silently make two slots share a cache entry - which would show one
    /// picture in another slot's row.
    pub fn cache_key(self) -> &'static str {
        match self {
            Self::EditImage => "edit-image",
            Self::TranscribeAudio => "transcribe-audio",
            Self::SfxInit => "sfx-init",
            Self::ImageControl => "image-control",
            Self::SeparateAudio => "separate-audio",
            Self::VideoStartImage => "video-start-image",
        }
    }

    /// Every slot, so a scan cannot miss one. Read by `every_slot_declares_its_kind`,
    /// which is what actually holds a new slot to answering `is_image`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALL: [Self; 6] = [
        Self::EditImage,
        Self::TranscribeAudio,
        Self::SfxInit,
        Self::ImageControl,
        Self::SeparateAudio,
        Self::VideoStartImage,
    ];
}

// ============================================================================
// Media Studio State
// ============================================================================

/// The media modality the studio is currently generating. Each kind
/// maps to a server `/v1/*` generation endpoint and surfaces only the
/// parameters relevant to it in the params form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum MediaKind {
    /// Text → PNG image (`/v1/images/generations`).
    #[default]
    Image,
    /// Text → music WAV (`/v1/audio/generations`, model=ace-step).
    Music,
    /// Text → sound-effect WAV (`/v1/audio/generations`, model=ezaudio).
    Sfx,
    /// Text → MIDI file (`/v1/audio/generations`, model=midi).
    Midi,
    /// Text → video mp4/gif (`/v1/video/generations`, model=wan).
    Video,
    /// Text → speech WAV (`/v1/audio/speech`).
    Speech,
    /// Source image + prompt → edited image (`/v1/images/edits`, FLUX Kontext / Qwen-Image-Edit).
    ImageEdit,
    /// Audio file -> text transcript (`/v1/audio/transcriptions`, Whisper/Voxtral).
    Transcribe,
    /// A mix -> its vocal and instrumental stems (`/v1/audio/separate`).
    Separate,
}

/// TTS engine behind the Speech kind. Parler is the English described-voice HF codec model;
/// Kyutai is the multilingual (en/fr) Delayed-Streams TTS; Piper is the native VITS voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SpeechEngine {
    #[default]
    Parler,
    Kyutai,
    Piper,
}

impl SpeechEngine {
    pub const ALL: [SpeechEngine; 3] = [SpeechEngine::Parler, SpeechEngine::Kyutai, SpeechEngine::Piper];
    pub fn label(self) -> &'static str {
        match self {
            SpeechEngine::Parler => "Parler (EN)",
            SpeechEngine::Kyutai => "Kyutai (EN/FR)",
            SpeechEngine::Piper => "Piper",
        }
    }
}

impl MediaKind {
    /// All kinds in display order — drives the segmented kind picker.
    pub const ALL: [MediaKind; 9] = [
        MediaKind::Image,
        MediaKind::ImageEdit,
        MediaKind::Music,
        MediaKind::Sfx,
        MediaKind::Midi,
        MediaKind::Video,
        MediaKind::Speech,
        MediaKind::Transcribe,
        MediaKind::Separate,
    ];

    /// Short button label for the kind picker.
    pub fn label(&self) -> &'static str {
        match self {
            MediaKind::Image => "Image",
            MediaKind::Music => "Music",
            MediaKind::Sfx => "SFX",
            MediaKind::Midi => "MIDI",
            MediaKind::Video => "Video",
            MediaKind::Speech => "Speech",
            MediaKind::ImageEdit => "Image Edit",
            MediaKind::Transcribe => "Transcribe",
            MediaKind::Separate => "Separate",
        }
    }

    /// One-line description shown under the picker / as a tooltip.
    pub fn tip(&self) -> &'static str {
        match self {
            MediaKind::Image => "Generate an image from a text prompt (PNG).",
            MediaKind::Music => "Generate a music clip from a text prompt (WAV, 48 kHz).",
            MediaKind::Sfx => "Generate a sound effect from a text prompt (WAV, 24 kHz).",
            MediaKind::Midi => "Generate a multi-track MIDI score from a text prompt (.mid).",
            MediaKind::Video => "Generate a short video from a text prompt (mp4 / gif).",
            MediaKind::Speech => "Synthesise speech from text (WAV). Engine: Parler (EN), Kyutai (EN/FR) or Piper.",
            MediaKind::ImageEdit => "Edit an existing image with a text instruction (FLUX Kontext / Qwen-Image-Edit).",
            MediaKind::Transcribe => "Transcribe an audio file to text (Whisper / Voxtral).",
            MediaKind::Separate => {
                "Split a song into its vocal and instrumental stems (Mel-Band RoFormer).\n\
                 The two stems add back to the original exactly."
            }
        }
    }

    /// The placeholder shown in the prompt box for this kind.
    pub fn prompt_hint(&self) -> &'static str {
        match self {
            MediaKind::Image => "a serene mountain lake at sunrise, photorealistic",
            MediaKind::Music => "upbeat synthwave with driving bass, 128 bpm",
            MediaKind::Sfx => "heavy rain on a tin roof with distant thunder",
            MediaKind::Midi => "a cheerful piano and strings waltz",
            MediaKind::Video => "one scene per line — they are cut together into one video:\na paper plane launched from a rooftop at dusk\nthe plane gliding between glass towers\nthe plane landing in a park fountain",
            MediaKind::Speech => "Hello, welcome to the Media Studio.",
            MediaKind::ImageEdit => "make the sky a dramatic sunset; keep everything else unchanged",
            MediaKind::Transcribe | MediaKind::Separate => "",
        }
    }
}

/// Output container / format for the Video kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum VideoFormat {
    #[default]
    Mp4,
    Gif,
}

impl VideoFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            VideoFormat::Mp4 => "mp4",
            VideoFormat::Gif => "gif",
        }
    }
}

/// Image-kind parameters (`/v1/images/generations`).
/// How a generated image comes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ImageFileFormat {
    /// Lossless, and the default when nothing says otherwise.
    #[default]
    Png,
    Jpeg,
    WebP,
}

impl ImageFileFormat {
    /// The value the API's `output_format` takes.
    pub fn wire(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::WebP => "webp",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Png => "PNG (lossless)",
            Self::Jpeg => "JPEG (small)",
            Self::WebP => "WebP (small)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Persisted: every field must survive a file written by an OLDER build. The
/// container-level `serde(default)` is what guarantees that - without it ONE field added
/// here makes serde reject the WHOLE config.json, and the user loses their settings and
/// their prompt history, not just the new field.
#[serde(default)]
pub struct ImageParams {
    /// Image model id ("z-image" or "flux-schnell").
    pub model: String,
    /// Output width in px (multiple of 16).
    pub width: u32,
    /// Output height in px (multiple of 16).
    pub height: u32,
    /// Number of images to generate.
    pub n: u32,
    /// Diffusion step count (Z-Image default: 9).
    pub steps: u32,
    /// Classifier-free guidance. 0.0 = let the server pick the model's default.
    pub guidance: f32,
    /// Which model's advertised defaults are currently loaded into the knobs above,
    /// so they are adopted once per model change and never fight the user's edits.
    #[serde(skip)]
    pub applied_defaults_for: Option<String>,
    /// LoRA adapters to apply, as `(name, strength)`. Names come from the server's
    /// `/v1/loras` listing - the client never sends a path.
    #[serde(default)]
    pub loras: Vec<(String, f32)>,
    /// Negative prompt - what the guidance steers AWAY from. This is how a caller
    /// removes what a positive prompt cannot name: a watermark, a warped hand.
    #[serde(default)]
    pub negative_prompt: String,
    /// Solver. Empty = the model's own default.
    #[serde(default)]
    pub sampler: String,
    /// Sigma curve. Empty = the model's own default.
    #[serde(default)]
    pub scheduler: String,
    /// Regional prompts. Empty = one prompt over the whole frame, as before.
    #[serde(default)]
    pub regions: Vec<ImageRegion>,
    /// The pose/edge image for structural conditioning, and how hard it pulls.
    ///
    /// A prompt cannot say where the limbs are, which is why "too many legs" survives
    /// more steps and more guidance. This is the conditioning that can.
    #[serde(skip)]
    pub control: Option<(String, Vec<u8>)>,
    #[serde(default = "default_control_scale")]
    pub control_scale: f32,
    /// Encoding of the returned image. The server transcodes for it; "png" keeps the
    /// lossless default, "jpeg" and "webp" trade exactness for a much smaller file.
    pub file_format: ImageFileFormat,

}

impl Default for ImageParams {
    fn default() -> Self {
        Self {
            model: "z-image".to_string(),
            width: 1024,
            height: 1024,
            n: 1,
            steps: 9,
            guidance: 0.0,
            applied_defaults_for: None,
            loras: Vec::new(),
            negative_prompt: String::new(),
            sampler: String::new(),
            scheduler: String::new(),
            regions: Vec::new(),
            control: None,
            control_scale: default_control_scale(),
            file_format: ImageFileFormat::default(),
        }
    }
}

/// Full strength: a pose given is a pose meant.
fn default_control_scale() -> f32 {
    1.0
}

/// Where a regional prompt applies, as a named area rather than four numbers.
///
/// The API takes a rectangle in 0..1, which a canvas editor would draw. This is the
/// shape the feature exists for: two subjects that a single prompt merges into one
/// body, kept apart by saying which half each belongs to. Named areas make that
/// reachable without a canvas, and they convert to the rectangle the server wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RegionArea {
    Left,
    Right,
    Top,
    Bottom,
    LeftThird,
    CentreThird,
    RightThird,
}

impl RegionArea {
    /// `(x, y, w, h)` in 0..1, the form `/v1/images/generations` takes.
    pub fn rect(self) -> (f32, f32, f32, f32) {
        match self {
            Self::Left => (0.0, 0.0, 0.5, 1.0),
            Self::Right => (0.5, 0.0, 0.5, 1.0),
            Self::Top => (0.0, 0.0, 1.0, 0.5),
            Self::Bottom => (0.0, 0.5, 1.0, 0.5),
            Self::LeftThird => (0.0, 0.0, 1.0 / 3.0, 1.0),
            Self::CentreThird => (1.0 / 3.0, 0.0, 1.0 / 3.0, 1.0),
            Self::RightThird => (2.0 / 3.0, 0.0, 1.0 / 3.0, 1.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left half",
            Self::Right => "Right half",
            Self::Top => "Top half",
            Self::Bottom => "Bottom half",
            Self::LeftThird => "Left third",
            Self::CentreThird => "Centre third",
            Self::RightThird => "Right third",
        }
    }

    pub const ALL: [Self; 7] = [
        Self::Left,
        Self::Right,
        Self::Top,
        Self::Bottom,
        Self::LeftThird,
        Self::CentreThird,
        Self::RightThird,
    ];
}

/// One regional prompt: what to put there, where, and how hard.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Persisted: every field must survive a file written by an OLDER build. The
/// container-level `serde(default)` is what guarantees that - without it ONE field added
/// here makes serde reject the WHOLE config.json, and the user loses their settings and
/// their prompt history, not just the new field.
#[serde(default)]
pub struct ImageRegion {
    pub prompt: String,
    pub area: RegionArea,
    pub strength: f32,
}

impl Default for ImageRegion {
    fn default() -> Self {
        Self { prompt: String::new(), area: RegionArea::Left, strength: 1.0 }
    }
}

/// Image-Edit-kind parameters (`/v1/images/edits`, multipart).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Persisted: every field must survive a file written by an OLDER build. The
/// container-level `serde(default)` is what guarantees that - without it ONE field added
/// here makes serde reject the WHOLE config.json, and the user loses their settings and
/// their prompt history, not just the new field.
#[serde(default)]
pub struct ImageEditParams {
    /// Edit model id ("flux-kontext" or "qwen-image-edit").
    pub model: String,
    /// Source image to edit: (file name, bytes). Not persisted (blob).
    #[serde(skip)]
    pub source: Option<(String, Vec<u8>)>,
    /// Edit strength (0 = keep source, 1 = full re-generation).
    pub strength: f32,
    /// Diffusion step count. 0 = server default for the model.
    pub steps: u32,
    /// Guidance. 0.0 = server default for the model.
    pub guidance: f32,
    /// Number of edited variants to produce.
    pub n: u32,
    /// LoRA adapters for the edit, as `(name, strength)`. Names come from the server's
    /// `/v1/loras` listing - the client never sends a path.
    #[serde(default)]
    pub loras: Vec<(String, f32)>,
    /// What the edit's guidance steers AWAY from. Empty = the family's own default.
    #[serde(default)]
    pub negative_prompt: String,
}

impl Default for ImageEditParams {
    fn default() -> Self {
        Self {
            model: "flux-kontext".to_string(),
            source: None,
            strength: 0.75,
            steps: 0,
            guidance: 0.0,
            n: 1,
            loras: Vec::new(),
            negative_prompt: String::new(),
        }
    }
}

/// Transcribe-kind parameters (`/v1/audio/transcriptions`, multipart).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SeparateParams {
    /// The mix to split: (file name, bytes). Not persisted (blob).
    #[serde(skip)]
    pub audio: Option<(String, Vec<u8>)>,
    /// Which stems to ask for.
    pub stems: SeparateStems,
}

/// Which stems the request asks for. Both is the default because the pair is what
/// makes the result verifiable - they must add back to the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SeparateStems {
    #[default]
    Both,
    Vocals,
    Instrumental,
}

impl SeparateStems {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Vocals => "vocals",
            Self::Instrumental => "instrumental",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Both => "Both stems",
            Self::Vocals => "Vocals only",
            Self::Instrumental => "Instrumental only",
        }
    }
}

impl Default for SeparateParams {
    fn default() -> Self {
        Self { audio: None, stems: SeparateStems::default() }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Persisted: every field must survive a file written by an OLDER build. The
/// container-level `serde(default)` is what guarantees that - without it ONE field added
/// here makes serde reject the WHOLE config.json, and the user loses their settings and
/// their prompt history, not just the new field.
#[serde(default)]
pub struct TranscribeParams {
    /// ASR model id (server default when empty; e.g. "whisper", "voxtral").
    pub model: String,
    /// Audio file to transcribe: (file name, bytes). Not persisted (blob).
    #[serde(skip)]
    pub audio: Option<(String, Vec<u8>)>,
    /// Translate to English instead of transcribing verbatim.
    pub translate: bool,
    /// The language spoken, or empty for "work it out".
    ///
    /// Whisper detects a language when none is given, and detection is not free of
    /// mistakes - a short clip, an accent, music under the speech. Without a way to say
    /// so, a French user whose clip was heard as English had no correction available at
    /// all: the server has taken this parameter all along and the studio never sent it.
    pub language: String,
}

impl Default for TranscribeParams {
    fn default() -> Self {
        Self {
            model: String::new(),
            audio: None,
            translate: false,
            language: String::new(),
        }
    }
}

/// Music-kind parameters (`/v1/audio/generations`, model=ace-step).
/// Kept separate from `SfxParams` even though both carry
/// seconds + steps: the defaults differ (27 vs 25 steps), bpm is
/// music-only, and per-kind structs mean a user's SFX tweaks never
/// clobber their music settings (and vice versa).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Persisted: every field must survive a file written by an OLDER build. The
/// container-level `serde(default)` is what guarantees that - without it ONE field added
/// here makes serde reject the WHOLE config.json, and the user loses their settings and
/// their prompt history, not just the new field.
#[serde(default)]
pub struct MusicParams {
    /// Clip length in seconds (the engine renders up to ~10 minutes).
    pub seconds: f32,
    /// Diffusion step count.
    pub steps: u32,
    /// Tempo (bpm).
    pub bpm: u32,
    /// Sung lyrics (with optional [verse]/[chorus] section tags). Empty = instrumental.
    pub lyrics: String,
    /// Negative prompt — traits to push AWAY from (stronger than caption negation).
    pub negative_prompt: String,
    /// Key/scale directive ("C minor", "A major", …). Empty = model's choice.
    pub keyscale: String,
    /// Lyrics language code (en/fr/…).
    pub language: String,
    /// DiT guidance (CFG). Turbo is distilled CFG-free (1.0 = off); the SFT/Base
    /// quality checkpoints follow the caption/lyrics better around 4-7.
    pub cfg: f32,
    /// LM sampling temperature.
    pub temperature: f32,
    /// LM nucleus top-p.
    pub top_p: f32,
    /// Ban the early end-of-song: force the LM toward the full requested duration.
    pub force_duration: bool,
    /// ACE-Step DiT checkpoint: "turbo" (8-step distilled, fast), "sft"/"base"
    /// (2B, ~50 steps, higher quality), "xl-*" (4B variants, best quality).
    pub dit_model: String,
    /// Seamless-loop mode: render a bar-exact segment and crossfade the tail into
    /// the head so the WAV loops cleanly in a DAW/sampler.
    pub loop_mode: bool,
    /// Loop length in bars (loop mode; duration derives from bars x bpm).
    pub loop_bars: u32,
}

impl Default for MusicParams {
    fn default() -> Self {
        Self {
            seconds: 30.0,
            steps: 27,
            bpm: 120,
            lyrics: String::new(),
            negative_prompt: String::new(),
            keyscale: String::new(),
            language: "en".to_string(),
            cfg: 3.0,
            temperature: 0.85,
            top_p: 0.9,
            force_duration: false,
            dit_model: "turbo".to_string(),
            loop_mode: false,
            loop_bars: 4,
        }
    }
}

/// Sound-effect-kind parameters (`/v1/audio/generations`, model=ezaudio).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Persisted: every field must survive a file written by an OLDER build. The
/// container-level `serde(default)` is what guarantees that - without it ONE field added
/// here makes serde reject the WHOLE config.json, and the user loses their settings and
/// their prompt history, not just the new field.
#[serde(default)]
pub struct SfxParams {
    /// Clip length in seconds.
    pub seconds: f32,
    /// Diffusion step count.
    pub steps: u32,
    /// Classifier-free guidance.
    pub cfg: f32,
    /// Engine: "ezaudio" (24 kHz mono, ~30 s) or "stable-audio"
    /// (Stable Audio Open, 44.1 kHz stereo, ~47 s, loops).
    #[serde(default)]
    pub model: String,
    /// Negative prompt - what the sound should avoid. Honoured by both engines.
    #[serde(default)]
    pub negative_prompt: String,
    /// Seamless-loop mode (stable-audio only).
    #[serde(default)]
    pub loop_mode: bool,
    /// Loop length in bars (loop mode).
    #[serde(default = "default_sfx_loop_bars")]
    pub loop_bars: u32,
    /// Loop tempo in BPM (loop mode).
    #[serde(default = "default_sfx_loop_bpm")]
    pub loop_bpm: u32,
    /// Source clip for an audio-to-audio variation (stable-audio only):
    /// (file name, WAV bytes). Not persisted.
    #[serde(skip)]
    pub init_audio: Option<(String, Vec<u8>)>,
    /// Variation strength when `init_audio` is set (schedule sigma_max:
    /// ~1 stays close to the source, ~10+ reinterprets it).
    #[serde(default = "default_sfx_init_noise")]
    pub init_noise_level: f32,
}

fn default_sfx_init_noise() -> f32 {
    1.0
}

fn default_sfx_loop_bars() -> u32 {
    4
}

fn default_sfx_loop_bpm() -> u32 {
    120
}

impl Default for SfxParams {
    fn default() -> Self {
        Self {
            seconds: 10.0,
            steps: 60,
            cfg: 3.0,
            model: String::new(),
            negative_prompt: String::new(),
            loop_mode: false,
            loop_bars: 4,
            loop_bpm: 120,
            init_audio: None,
            init_noise_level: 1.0,
        }
    }
}

/// MIDI-kind parameters (`/v1/audio/generations`, model=midi).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Persisted: every field must survive a file written by an OLDER build. The
/// container-level `serde(default)` is what guarantees that - without it ONE field added
/// here makes serde reject the WHOLE config.json, and the user loses their settings and
/// their prompt history, not just the new field.
#[serde(default)]
pub struct MidiParams {
    /// Token budget for the MIDI language model.
    pub max_tokens: u32,
    /// Sampling temperature.
    pub temperature: f32,
    /// Nucleus sampling top_p.
    pub top_p: f32,
}

impl Default for MidiParams {
    fn default() -> Self {
        Self { max_tokens: 512, temperature: 1.0, top_p: 0.98 }
    }
}

/// Frames per second the video engine renders at. The clip's length is expressed in
/// SECONDS everywhere a person sees it, and converted here - nobody thinks in frames.
pub const VIDEO_FPS: f32 = 16.0;

/// The frame count for a duration, on the only counts the temporal VAE can express.
///
/// It compresses time by four, so exact lengths are 1, 5, 9, ... Rounding UP means a
/// clip is never shorter than what was asked for, and the caller is told the real length
/// rather than left to notice it.
pub fn frames_for_seconds(seconds: f32) -> u32 {
    let want = (seconds.max(0.0) * VIDEO_FPS).round().max(1.0) as u32;
    4 * want.saturating_sub(1).div_ceil(4) + 1
}

/// What [`frames_for_seconds`] actually renders, in seconds.
pub fn seconds_for_frames(frames: u32) -> f32 {
    frames as f32 / VIDEO_FPS
}

/// Video-kind parameters (`/v1/video/generations`, model=wan). Video
/// owns its width/height rather than sharing the image ones: its
/// slider range (128..=768) and default (512², chosen small because
/// video is far heavier) differ from the image kind's (64..=2048,
/// 1024²).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Persisted: every field must survive a file written by an OLDER build. The
/// container-level `serde(default)` is what guarantees that - without it ONE field added
/// here makes serde reject the WHOLE config.json, and the user loses their settings and
/// their prompt history, not just the new field.
#[serde(default)]
pub struct VideoParams {
    /// Which video checkpoint. "wan" is the base; a fine-tune dropped in the server's
    /// `wan` directory is offered under its own name.
    #[serde(default = "default_video_model")]
    pub model: String,
    /// Clip length in SECONDS. Converted to a frame count the temporal VAE can express
    /// exactly; see [`frames_for_seconds`].
    #[serde(default = "default_video_seconds")]
    pub seconds: f32,
    /// Output width in px.
    pub width: u32,
    /// Output height in px.
    pub height: u32,
    /// Diffusion step count.
    pub steps: u32,
    /// Classifier-free guidance. 0.0 = resolution-aware server default.
    pub cfg: f32,
    /// Output container / format.
    pub format: VideoFormat,
    /// Denoise sampler (Auto = resolution-aware server default).
    pub sampler: VideoSampler,
    /// Negative prompt - what the guidance steers AWAY from. Empty = the plain
    /// unconditional branch the model was trained against.
    pub negative_prompt: String,
    /// The frame an image-to-video checkpoint continues.
    pub start_image: Option<(String, Vec<u8>)>,
}

/// The base video model, and what an older config file gets when it has no `model`.
fn default_video_model() -> String {
    "wan".to_string()
}

/// Three seconds: long enough to read as motion, short enough to iterate on.
fn default_video_seconds() -> f32 {
    3.0
}

impl Default for VideoParams {
    fn default() -> Self {
        Self {
            model: default_video_model(),
            seconds: default_video_seconds(),
            width: 512,
            height: 512,
            steps: 8,
            cfg: 0.0,
            format: VideoFormat::default(),
            sampler: VideoSampler::default(),
            negative_prompt: String::new(),
            start_image: None,
        }
    }
}

/// Denoise sampler for the Video kind. Auto lets the server pick its
/// measured resolution-aware default (UniPC at native scale, Heun below).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum VideoSampler {
    #[default]
    Auto,
    UniPc,
    Heun,
}

impl VideoSampler {
    pub fn as_str(self) -> Option<&'static str> {
        match self {
            VideoSampler::Auto => None,
            VideoSampler::UniPc => Some("unipc"),
            VideoSampler::Heun => Some("heun"),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            VideoSampler::Auto => "Auto",
            VideoSampler::UniPc => "UniPC",
            VideoSampler::Heun => "Heun",
        }
    }
}

/// Speech-kind parameters (`/v1/audio/speech`).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
/// Persisted: every field must survive a file written by an OLDER build. The
/// container-level `serde(default)` is what guarantees that - without it ONE field added
/// here makes serde reject the WHOLE config.json, and the user loses their settings and
/// their prompt history, not just the new field.
#[serde(default)]
pub struct SpeechParams {
    /// Which TTS engine backs the Speech kind (Parler / Kyutai / Piper).
    pub engine: SpeechEngine,
    /// Selected preset voice (Parler engine); empty = server default.
    pub voice: String,
    /// Voices offered by the server (from GET /v1/audio/voices). Session-only.
    #[serde(skip)]
    pub voices: Vec<String>,
    /// Whether a voice-list fetch has been requested this session — gates
    /// the one-shot auto-fetch when the Speech kind is first shown so the
    /// render loop doesn't re-request every frame.
    #[serde(skip)]
    pub voices_fetched: bool,
    /// Free-text voice name for Kyutai / Piper (a `kyutai/tts-voices` name substring, or a
    /// `piper/<voice>` id). Empty = the engine's default voice.
    pub voice_name: String,
    /// Parler voice + delivery description ("an old man shouting angrily",
    /// "a soft whispering woman", …). Overrides the preset when set.
    pub voice_description: String,
}

/// The persistable slice of the Media Studio state: every parameter the user
/// tunes, none of the runtime/results/blobs. Saved into the app config at exit
/// and hydrated at launch so settings survive restarts. `serde(default)` per
/// container keeps configs forward/backward compatible as kinds gain fields.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
/// Persisted: every field must survive a file written by an OLDER build. The
/// container-level `serde(default)` is what guarantees that - without it ONE field added
/// here makes serde reject the WHOLE config.json, and the user loses their settings and
/// their prompt history, not just the new field.
#[serde(default)]
pub struct MediaPersist {
    #[serde(default)]
    pub kind: MediaKind,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub seed: String,
    #[serde(default)]
    pub image: Option<ImageParams>,
    #[serde(default)]
    pub image_edit: Option<ImageEditParams>,
    #[serde(default)]
    pub music: Option<MusicParams>,
    #[serde(default)]
    pub sfx: Option<SfxParams>,
    #[serde(default)]
    pub midi: Option<MidiParams>,
    #[serde(default)]
    pub video: Option<VideoParams>,
    #[serde(default)]
    pub speech: Option<SpeechParams>,
    #[serde(default)]
    pub transcribe: Option<TranscribeParams>,
}

/// One past generation, kept so a new run never destroys the previous output.
#[derive(Debug, Clone)]
pub struct MediaHistoryEntry {
    pub kind: MediaKind,
    pub prompt: String,
    pub images: Vec<String>,
    pub audios: Vec<String>,
    pub files: Vec<(String, Vec<u8>)>,
    pub text: Option<String>,
}

impl MediaState {
    /// Move the CURRENT results into history (most recent first) before a new run
    /// overwrites them. No-op when there is nothing to keep.
    pub fn archive_results(&mut self) {
        if self.result_images.is_empty()
            && self.result_audios.is_empty()
            && self.result_files.is_empty()
            && self.result_text.is_none()
        {
            return;
        }
        // Bounded: these payloads are multi-MB base64 blobs.
        const MAX_HISTORY: usize = 8;
        self.history.insert(
            0,
            MediaHistoryEntry {
                kind: self.kind,
                prompt: self.prompt.clone(),
                images: std::mem::take(&mut self.result_images),
                audios: std::mem::take(&mut self.result_audios),
                files: std::mem::take(&mut self.result_files),
                text: self.result_text.take(),
            },
        );
        self.history.truncate(MAX_HISTORY);
    }

    /// Put a history entry back into the main result view (swapping the current
    /// results into history so nothing is lost either way).
    pub fn restore_from_history(&mut self, idx: usize) {
        if idx >= self.history.len() {
            return;
        }
        let entry = self.history.remove(idx);
        self.archive_results();
        self.result_images = entry.images;
        self.result_audios = entry.audios;
        self.result_files = entry.files;
        self.result_text = entry.text;
    }

    /// Snapshot the persistable parameters (for the exit-time config save).
    pub fn to_persist(&self) -> MediaPersist {
        MediaPersist {
            kind: self.kind,
            prompt: self.prompt.clone(),
            seed: self.seed.clone(),
            image: Some(self.image.clone()),
            image_edit: Some(self.image_edit.clone()),
            music: Some(self.music.clone()),
            sfx: Some(self.sfx.clone()),
            midi: Some(self.midi.clone()),
            video: Some(self.video.clone()),
            speech: Some(self.speech.clone()),
            transcribe: Some(self.transcribe.clone()),
        }
    }

    /// Hydrate the tunable parameters from a saved snapshot (launch time).
    pub fn apply_persist(&mut self, p: &MediaPersist) {
        self.kind = p.kind;
        self.prompt = p.prompt.clone();
        self.seed = p.seed.clone();
        if let Some(v) = &p.image { self.image = v.clone(); }
        if let Some(v) = &p.image_edit { self.image_edit = v.clone(); }
        if let Some(v) = &p.music { self.music = v.clone(); }
        if let Some(v) = &p.sfx { self.sfx = v.clone(); }
        if let Some(v) = &p.midi { self.midi = v.clone(); }
        if let Some(v) = &p.video { self.video = v.clone(); }
        if let Some(v) = &p.speech { self.speech = v.clone(); }
        if let Some(v) = &p.transcribe { self.transcribe = v.clone(); }
    }
}

/// Media Studio tab state. Holds the selected kind, one shared prompt,
/// one params sub-struct per kind (only the relevant widgets are shown
/// per kind), and the in-flight / result bookkeeping. Result payloads
/// are split by how they're consumed: images render inline, audios play
/// via the system player, and everything else (MIDI, video) is offered
/// as a (filename, bytes) save/open pair since the GUI can't render them.
///
/// Each kind owns its parameters, so switching kinds keeps every kind's tweaks
/// intact. Sharing width, height, steps and seconds across kinds means each
/// switch resets them to that kind's defaults, silently discarding whatever the
/// user had set.
/// Cached previews of chosen files.
///
/// A newtype because `MediaState` derives `Debug` and a GPU texture handle does not
/// implement it - and printing one would say nothing useful anyway. The manual impl
/// reports the count, which is the only part worth seeing in a dump.
#[derive(Clone, Default)]
pub struct ThumbnailCache(
    pub std::collections::HashMap<(&'static str, usize), egui::TextureHandle>,
);

impl std::fmt::Debug for ThumbnailCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ThumbnailCache({} cached)", self.0.len())
    }
}

/// Fullscreen inspection of a rendered clip.
///
/// A panel-sized player is the wrong instrument for what a clip has to be judged on -
/// consistency across frames, flicker, artefacts around a moving subject. None of that
/// is visible at 420 px, and it is what a render is accepted or rejected on.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoViewer {
    pub open: bool,
    /// 1.0 = fit the window. Above that, the clip is inspected at pixel scale.
    pub zoom: f32,
    /// Offset from centred, in screen pixels, so panning survives a zoom change.
    pub pan: (f32, f32),
}

impl Default for VideoViewer {
    fn default() -> Self {
        Self { open: false, zoom: 1.0, pan: (0.0, 0.0) }
    }
}

#[derive(Debug, Clone)]
pub struct MediaState {
    /// Fullscreen viewer for the rendered clip - see [`VideoViewer`].
    pub video_viewer: VideoViewer,
    /// AbortHandle for the in-flight media generation task, so the Cancel button can drop
    /// this end of the HTTP request.
    ///
    /// Dropping it is NOT what stops the render. That was believed and it is false: the
    /// server does not abandon a handler whose response has not started, so a clip goes on
    /// to completion for a client that has gone - measured, twice. Cancelling means telling
    /// the server, which is what [`Self::render_id`] is for.
    pub generation_abort: Option<tokio::task::AbortHandle>,
    /// The server's name for the render in flight, from its first message. `None` until it
    /// arrives, or when nothing is running.
    pub render_id: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    /// A render the user asked to stop, waiting for the owner of the HTTP client to say so
    /// over the API. Taken once sent.
    pub pending_cancel: Option<String>,
    /// A prompt-enhancement request is in flight (LLM rewriting the prompt).
    pub enhancing_prompt: bool,
    /// The prompt as it was before the last enhancement, for one-click undo.
    pub prompt_before_enhance: Option<String>,
    /// Model the user explicitly picked for prompt enhancement. `None` = Auto:
    /// the enhancer walks its own candidate list and validates each reply.
    pub enhance_model: Option<String>,
    /// Decoded thumbnails of the files chosen for processing, keyed by slot.
    ///
    /// A picked image was described by its name and its size in kilobytes, which does
    /// not answer the question the user is actually asking - is this the right picture.
    /// Cached because decoding runs on the frame path: keyed by slot AND by the byte
    /// length, so replacing a slot's file rebuilds the texture and re-picking the same
    /// one does not.
    pub thumbnails: ThumbnailCache,
    /// Currently selected modality.
    pub kind: MediaKind,
    /// Shared text prompt / input across kinds.
    pub prompt: String,

    // ── Per-kind parameters ────────────────────────────────────────
    pub image: ImageParams,
    pub music: MusicParams,
    pub sfx: SfxParams,
    pub midi: MidiParams,
    pub video: VideoParams,
    pub speech: SpeechParams,
    pub image_edit: ImageEditParams,
    pub transcribe: TranscribeParams,
    pub separate: SeparateParams,

    // ── Shared ─────────────────────────────────────────────────────
    /// Seed as free text; empty string = server picks a random seed.
    pub seed: String,

    // ── Runtime ────────────────────────────────────────────────────
    /// True while a generation request is in flight (disables Generate).
    pub is_generating: bool,
    /// Streaming progress (step, total) parsed from SSE, when available.
    pub progress: Option<(u64, u64)>,
    /// Wall-clock anchor for the current request — drives elapsed / ETA.
    pub started_at: Option<std::time::Instant>,
    /// Human-readable status line ("Rendering step 4/27…", "Done", …).
    pub status: String,
    /// Last error, surfaced as a banner. Cleared on the next Generate.
    pub error: Option<String>,
    /// Base64 PNGs from the most recent image generation (rendered inline).
    pub result_images: Vec<String>,
    /// Base64 WAVs from the most recent audio/speech generation (Play/Save).
    pub result_audios: Vec<String>,
    /// (filename, bytes) blobs that can't be rendered in-GUI (MIDI, video)
    /// — offered as Save + Open-with-system buttons.
    pub result_files: Vec<(String, Vec<u8>)>,
    /// Text result (Transcribe kind) — rendered as selectable text.
    pub result_text: Option<String>,
    /// Earlier generations, most recent first, so a new run does not wipe the panel
    /// and take with it whatever was not saved on the spot - including a batch still
    /// being compared. Capped; each entry keeps what it needs to be restored into the
    /// main view.
    pub history: Vec<MediaHistoryEntry>,

    /// Expected denoising time for the CURRENT video settings, in seconds, from
    /// `/v1/video/plan`. `None` = not answered yet, and the tab shows nothing: a video
    /// render is the one place where the wait can be an afternoon, and a zero standing in
    /// for "unknown" is the reading that costs someone that afternoon.
    pub video_estimate: Option<f32>,
    /// Signature of the settings the last estimate was REQUESTED for - see
    /// [`MediaState::video_estimate_key`].
    ///
    /// Kept because the frame path re-evaluates every frame: without it the tab asks the
    /// server sixty times a second for an answer that has not changed. It also tags the
    /// reply, so an answer arriving after the user has moved a slider on is discarded
    /// instead of being shown against settings it was never computed for.
    pub estimate_key: Option<String>,
    /// When a FAILED estimate may be asked for again.
    ///
    /// A failure must not read as an answer. Leaving the key marked as asked means a
    /// tab opened while the server is unreachable never shows a cost again until some
    /// setting moves; clearing it outright swings the other way and hammers a server
    /// that is not answering. So a failure clears it AND waits.
    pub estimate_retry_after: Option<std::time::Instant>,
    /// An estimate request is in flight. Dragging a slider walks through hundreds of
    /// distinct settings; one request at a time collapses that walk into a short chain,
    /// because the next dispatch only happens once the previous answer has landed.
    pub estimate_in_flight: bool,

    /// Worker-thread channel for file-save dialogs (mirrors ChatState).
    pub pending_dialog: std::sync::Arc<std::sync::Mutex<Option<ChatDialogResult>>>,
    /// True while a save-dialog worker thread is alive (gates buttons).
    pub dialog_in_flight: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Default for MediaState {
    fn default() -> Self {
        Self {
            render_id: std::sync::Arc::new(std::sync::Mutex::new(None)),
            pending_cancel: None,
            video_viewer: VideoViewer::default(),
            generation_abort: None,
            enhancing_prompt: false,
            prompt_before_enhance: None,
            enhance_model: None,
            thumbnails: ThumbnailCache::default(),
            kind: MediaKind::default(),
            prompt: String::new(),
            image: ImageParams::default(),
            music: MusicParams::default(),
            sfx: SfxParams::default(),
            midi: MidiParams::default(),
            video: VideoParams::default(),
            speech: SpeechParams::default(),
            image_edit: ImageEditParams::default(),
            transcribe: TranscribeParams::default(),
            separate: SeparateParams::default(),
            seed: String::new(),
            is_generating: false,
            progress: None,
            started_at: None,
            status: String::new(),
            error: None,
            result_images: Vec::new(),
            result_audios: Vec::new(),
            result_files: Vec::new(),
            result_text: None,
            history: Vec::new(),
            video_estimate: None,
            estimate_key: None,
            estimate_retry_after: None,
            estimate_in_flight: false,
            pending_dialog: std::sync::Arc::new(std::sync::Mutex::new(None)),
            dialog_in_flight: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

impl MediaState {
    /// The video settings that move the estimate, as one comparable string.
    ///
    /// Only what the cost actually depends on goes in: the checkpoint (more than two to
    /// one between the small and the wide one), the frame, the number of frames and the
    /// step count. The prompt, the seed and the container do not change the arithmetic, so
    /// including them would re-ask the server on every keystroke.
    ///
    /// Frames rather than seconds, because the temporal VAE only renders 4k+1 of them: two
    /// nearby durations that round to the same frame count cost the same, and asking twice
    /// for the same answer is what this key exists to prevent.
    pub fn video_estimate_key(&self) -> String {
        let v = &self.video;
        format!(
            "{}|{}x{}|{}f|{}s",
            v.model,
            v.width,
            v.height,
            frames_for_seconds(v.seconds),
            v.steps
        )
    }

    /// Switch the active kind. Each kind's parameters live in their own
    /// sub-struct (with per-kind Default impls), so switching preserves
    /// every kind's tweaks — no reset dance. Leaves the shared prompt
    /// untouched so a user can re-run the same idea across kinds.
    pub fn set_kind(&mut self, kind: MediaKind) {
        if self.kind != kind {
            self.kind = kind;
            // Results from the previous kind no longer make sense next
            // to the new kind's form — clear them for a clean slate.
            self.clear_results();
            // Forget which settings were already priced, so returning to the video kind
            // asks again. An estimate that failed once - the tab was opened before the
            // server was reachable - would otherwise stay blank forever, because the
            // signature it failed on is still recorded as asked and nothing retries it.
            self.estimate_key = None;
        }
    }

    /// Drop all result payloads + status/error/progress. Used on kind
    /// switch and at the start of a new generation.
    pub fn clear_results(&mut self) {
        self.result_images.clear();
        self.result_audios.clear();
        self.result_files.clear();
        self.result_text = None;
        self.progress = None;
        self.status.clear();
        self.error = None;
    }

    /// Parse the free-text seed field into an optional u64. Empty (or
    /// unparseable) → None, meaning "let the server pick a random seed".
    pub fn parsed_seed(&self) -> Option<u64> {
        let t = self.seed.trim();
        if t.is_empty() {
            None
        } else {
            t.parse::<u64>().ok()
        }
    }

    /// Mark a new generation as in-flight: clear stale results, flip the
    /// busy flag, and anchor the ETA clock.
    pub fn begin_generation(&mut self) {
        // Keep the previous output instead of wiping it: a new run must not
        // destroy media the user has not saved yet.
        self.archive_results();
        self.is_generating = true;
        self.started_at = Some(std::time::Instant::now());
        self.status = "Starting…".to_string();
        self.generation_abort = None; // the spawn assigns the fresh handle
    }

    /// Reset the in-flight bookkeeping when a generation completes or
    /// fails. Leaves results in place (the completion handler populates
    /// them just before calling this).
    pub fn finish_generation(&mut self) {
        self.is_generating = false;
        self.progress = None;
        self.started_at = None;
        self.generation_abort = None;
    }

    /// Cancel the in-flight generation: abort the task (dropping its HTTP
    /// request, which the server turns into a render cancellation) and reset
    /// the busy state. No-op when idle.
    /// Stop the generation. Returns the server's name for it, when it has one.
    ///
    /// Aborting the local task only drops THIS end of the wire, and that is not what stops a
    /// render: the server does not abandon a handler whose response has not started, so the
    /// work runs to completion for nobody. The caller uses the returned identifier to say so
    /// over the API.
    pub fn cancel_generation(&mut self) -> Option<String> {
        let id = self.render_id.lock().ok().and_then(|mut g| g.take());
        if let Some(h) = self.generation_abort.take() {
            h.abort();
        }
        if self.is_generating {
            self.finish_generation();
            self.status = "Cancelled.".to_string();
        }
        id
    }
}

// ============================================================================
// CLI State
// ============================================================================

/// CLI command output
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct CLIOutput {
    pub timestamp: String,
    pub command: String,
    pub output: String,
    pub is_error: bool,
    /// Whether this output is still in progress (showing spinner)
    pub in_progress: bool,
}


/// CLI tab state
#[derive(Debug, Clone, Default)]
pub struct CLIState {
    pub input: String,
    pub outputs: Vec<CLIOutput>,
    /// Past commands the user has executed in this session, newest
    /// last. history_index points at one-past-the-end when no nav
    /// is active. Bounded to CLI_HISTORY_LIMIT.
    pub history: Vec<String>,
    pub history_index: usize,
    /// Snapshot of the in-progress input when the user starts Up-
    /// navigation, restored when Down-scrolling past the newest.
    pub history_draft: String,
}

/// Cap on CLI command history retained per session.
pub const CLI_HISTORY_LIMIT: usize = 200;

/// Cap on CLI output entries retained per session. Each entry
/// holds the command, full output text, timestamp + two flags.
/// Without a cap, a long-running session could accumulate thousands
/// of entries — every one re-rendered every frame in the CLI
/// scrollback, eventually showing up as visible lag on the tab.
/// `clear` (the literal CLI verb) still resets the list to empty;
/// this cap protects users who never run it.
pub const CLI_OUTPUT_LIMIT: usize = 500;

impl CLIState {
    /// Push a new CLI output entry, evicting the oldest if the cap
    /// is exceeded. Centralised so a future change to the eviction
    /// policy (e.g. VecDeque, FIFO with a different cap) updates
    /// in one place. Call sites go through this helper rather than pushing onto
    /// `cli.outputs` directly, so the cap cannot be bypassed.
    pub fn push_output(&mut self, output: CLIOutput) {
        self.outputs.push(output);
        while self.outputs.len() > CLI_OUTPUT_LIMIT {
            self.outputs.remove(0);
        }
    }

    /// Push a freshly-executed command onto the history (dedupe vs
    /// head, cap at CLI_HISTORY_LIMIT). Resets history_index to the
    /// one-past-end sentinel so the next Up starts from the newest.
    ///
    /// Empty / whitespace-only input is silently ignored — defensive
    /// guard mirroring ChatState::push_prompt_history so a future
    /// caller can't accidentally fill the recall list with blanks.
    pub fn push_history(&mut self, cmd: String) {
        if !cmd.trim().is_empty() && self.history.last() != Some(&cmd) {
            self.history.push(cmd);
            while self.history.len() > CLI_HISTORY_LIMIT {
                self.history.remove(0);
            }
        }
        self.history_index = self.history.len();
        self.history_draft.clear();
    }

    /// Move history cursor backward (older). Snapshots the live input
    /// into history_draft on the first move so Down past the end
    /// restores the in-progress edit. No-op when history is empty or
    /// already at the oldest entry.
    pub fn history_back(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let n = self.history.len();
        // history_index == n means "at the draft / one-past-end".
        if self.history_index == n {
            self.history_draft = self.input.clone();
            self.history_index = n - 1;
        } else if self.history_index > 0 {
            self.history_index -= 1;
        } else {
            return; // already at oldest
        }
        self.input.clone_from(&self.history[self.history_index]);
    }

    /// Move history cursor forward (newer). Past the newest entry,
    /// restores the live edit from history_draft and parks the index
    /// at the one-past-end sentinel.
    pub fn history_forward(&mut self) {
        let n = self.history.len();
        if self.history_index >= n {
            return;
        }
        self.history_index += 1;
        if self.history_index == n {
            self.input = std::mem::take(&mut self.history_draft);
        } else {
            self.input.clone_from(&self.history[self.history_index]);
        }
    }
}

// ============================================================================
// Model State
// ============================================================================

/// Action status for model operations
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ActionStatus {
    #[default]
    Idle,
    InProgress(String),
    Success(String),
    Failed(String),
}

impl ActionStatus {
    pub fn is_in_progress(&self) -> bool {
        matches!(self, ActionStatus::InProgress(_))
    }
}

/// Sort field for models
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelSortField {
    #[default]
    Name,
    Size,
    Date,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelSortDirection {
    #[default]
    Asc,
    Desc,
}

/// Model management state
#[derive(Debug, Clone, Default)]
pub struct ModelState {
    pub available_models: Vec<ModelInfo>,
    pub loaded_models: Vec<String>,
    pub selected_model: Option<String>,
    pub action_status: ActionStatus,
    /// When the current `action_status` became `InProgress`. A gate that outlives its
    /// request - a task that never completes because the server wedged - would disable
    /// every Models-tab button for good, so the watchdog in `app.rs` clears one older
    /// than its deadline and the UI recovers by itself.
    pub action_started_at: Option<std::time::Instant>,
    pub pull_model_input: String,
    pub pull_source: String,  // "ollama" or "huggingface"
    /// Live download progress for the in-flight pull, `(completed, total)`
    /// bytes parsed from the server's NDJSON `/api/pull` stream. `None`
    /// when no pull is running (or before the first progress line arrives);
    /// `total == 0` means the manifest phase (indeterminate) — a real
    /// determinate bar is shown only once `total > 0`.
    pub pull_progress: Option<(u64, u64)>,
    pub sort_field: ModelSortField,
    pub sort_direction: ModelSortDirection,
    /// Substring filter for the model list (case-insensitive, matches
    /// against model name). Empty string = no filter.
    pub list_filter: String,
    /// Modality filter for the model list. `None` = show all; `Some(m)`
    /// only shows models classified as that modality via
    /// `ModelModality::from_model_name`. Lets the user narrow a long
    /// catalog to just TTS / ASR / image-gen / vision / text.
    /// Cleared by the toolbar's clear button alongside `list_filter`.
    /// pub(crate) because ModelModality is also pub(crate) — rustc
    /// warns on a pub field with a less-visible type.
    pub(crate) list_modality_filter: Option<crate::modality::ModelModality>,
    /// Model the user has clicked Delete on but not yet confirmed.
    /// Holding state at the ModelState level (not at the ui::models
    /// scope) lets the confirmation modal render at the top of the
    /// settings tab regardless of which row the user clicked, and
    /// avoids losing the confirmation prompt if the user scrolls.
    /// Cleared on Confirm (delete fires) or Cancel.
    pub delete_confirm_pending: Option<String>,
}

impl ModelState {
    /// Get sorted list of models. Returns owned ModelInfo so the
    /// caller can hold the vec across mutable borrows of self (e.g.
    /// setting selected_model on click). The clone is small (N
    /// usually < 50) so the per-frame cost is negligible vs. the
    /// complexity of deferring mutations out of the render loop.
    pub fn get_sorted_models(&self) -> Vec<ModelInfo> {
        let mut models = self.available_models.clone();
        match self.sort_field {
            ModelSortField::Name => models.sort_by(|a, b| a.name.cmp(&b.name)),
            ModelSortField::Size => models.sort_by(|a, b| a.size_bytes.cmp(&b.size_bytes)),
            ModelSortField::Date => models.sort_by(|a, b| a.modified_at.cmp(&b.modified_at)),
        }
        if self.sort_direction == ModelSortDirection::Desc {
            models.reverse();
        }
        models
    }

    /// Check if a model is loaded
    pub fn is_loaded(&self, model_name: &str) -> bool {
        // Was: self.loaded_models.contains(&model_name.to_string())
        // which allocated a String per call. Called per-model per-frame
        // on the Models tab (60 fps × N models) — the alloc is cheap
        // but pointless. Use iter().any(|m| m == model_name) to compare
        // &str-vs-String directly without the intermediate allocation.
        self.loaded_models.iter().any(|m| m == model_name)
    }

    /// Select the best available model. Three-stage decision:
    ///
    /// 1. If a model is already selected AND it still exists in the
    ///    available_models list, keep it (don't yank the user's pick
    ///    out from under them on every refresh).
    /// 2. Otherwise prefer the first loaded model — it's already in
    ///    memory so the next Send won't trigger a multi-second load.
    /// 3. Otherwise pick the smallest available model — quickest to
    ///    load when the user inevitably hits Send.
    ///
    /// Returns silently if no models are available (selected_model
    /// stays None and the chat-header empty-state guides the user
    /// to the Models tab).
    pub fn select_best_model(&mut self) {
        // Keep current selection if valid.
        if let Some(ref selected) = self.selected_model {
            if self.available_models.iter().any(|m| &m.name == selected) {
                return;
            }
        }

        // Prefer loaded models. The if-let carries no panic risk if the
        // is_empty()/.first() sequence is ever refactored.
        if let Some(first_loaded) = self.loaded_models.first() {
            self.selected_model = Some(first_loaded.clone());
            return;
        }

        // Fall back to smallest available model.
        self.selected_model = self
            .available_models
            .iter()
            .min_by_key(|m| m.size_bytes)
            .map(|m| m.name.clone());
    }
}

// ============================================================================
// Server State
// ============================================================================

/// Server status
#[derive(Debug, Clone, PartialEq, Default)]
#[allow(dead_code)]
pub enum ServerStatus {
    #[default]
    NotStarted,
    Starting,
    Running {
        port: u16,
    },
    Failed(String),
}

/// Log level filter for display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevelFilter {
    All,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevelFilter {
    /// Get the display name
    pub fn display_name(&self) -> &'static str {
        match self {
            LogLevelFilter::All => "All",
            LogLevelFilter::Info => "Info+",
            LogLevelFilter::Warn => "Warn+",
            LogLevelFilter::Error => "Error",
        }
    }
}

/// Server tab state
#[derive(Debug, Default)]
pub struct ServerState {
    pub status: ServerStatus,
    /// Current log level filter for display
    pub log_filter: LogLevelFilter,
    /// Search filter
    pub log_search: String,
}

impl ServerState {
    /// Get filtered log entries from the buffer
    /// Filtering happens inside the lock, only matching entries are cloned
    pub fn filtered_logs(
        &self,
        buffer: &crate::log_buffer::LogBuffer,
    ) -> Vec<crate::log_buffer::LogEntry> {
        use crate::log_buffer::LogLevel;

        let min_level = match self.log_filter {
            LogLevelFilter::All => LogLevel::Trace,
            LogLevelFilter::Info => LogLevel::Info,
            LogLevelFilter::Warn => LogLevel::Warn,
            LogLevelFilter::Error => LogLevel::Error,
        };

        buffer.entries_filtered(min_level, &self.log_search)
    }
}

// ============================================================================
// Hardware State
// ============================================================================











#[cfg(test)]
// Tests mutate individual ChatState fields after constructing
// with Default::default() — clippy::field_reassign_with_default
// suggests the struct-update syntax (`ChatState { field: x,
// ..Default::default() }`) but that pattern obscures which fields
// are intentionally being toggled vs. inherited, especially for
// tests that touch 4+ fields. Inline field mutation is more
// readable here; suppress the lint at the module boundary.
#[allow(clippy::field_reassign_with_default)]
mod chat_history_tests {
    use super::*;

    // ── Media Studio state ──────────────────────────────────────────

    #[test]
    fn media_parsed_seed_empty_is_random_and_valid_parses() {
        let mut m = MediaState::default();
        assert_eq!(m.parsed_seed(), None, "empty seed → server random");
        m.seed = "  42 ".into();
        assert_eq!(m.parsed_seed(), Some(42), "trimmed numeric seed parses");
        m.seed = "notanum".into();
        assert_eq!(m.parsed_seed(), None, "garbage seed falls back to random");
    }

    #[test]
    fn media_set_kind_clears_results_and_keeps_per_kind_params() {
        let mut m = MediaState::default();
        m.result_images.push("x".into());
        m.status = "old".into();
        // Tweak the image params, then switch kinds. Per-kind
        // Per-kind sub-structs mean the video kind starts from its own defaults
        // AND the image tweaks survive the round-trip. Shared width and steps would
        // reset on every switch, discarding what the user set.
        m.image.steps = 30;
        m.set_kind(MediaKind::Video);
        assert_eq!(m.kind, MediaKind::Video);
        assert_eq!(m.video.steps, 8, "video kind has its own step default");
        assert_eq!(m.video.width, 512, "video starts small (heavier than image)");
        assert!(m.result_images.is_empty(), "results cleared on kind switch");
        assert!(m.status.is_empty());
        m.set_kind(MediaKind::Image);
        assert_eq!(m.image.steps, 30, "image tweaks survive a kind round-trip");
        assert_eq!(m.image.width, 1024);
    }

    #[test]
    fn media_begin_and_finish_generation_toggle_busy_state() {
        let mut m = MediaState::default();
        m.result_audios.push("stale".into());
        m.begin_generation();
        assert!(m.is_generating);
        assert!(m.started_at.is_some());
        assert!(m.result_audios.is_empty(), "begin clears stale results");
        m.progress = Some((3, 10));
        m.finish_generation();
        assert!(!m.is_generating);
        assert_eq!(m.progress, None);
        assert!(m.started_at.is_none());
    }

    fn push_n(c: &mut ChatState, prompts: &[&str]) {
        for p in prompts {
            c.push_prompt_history((*p).to_string());
        }
    }

    #[test]
    fn chat_message_system_helper_sets_role_and_clears_media_fields() {
        let m = ChatMessage::system("hello");
        assert_eq!(m.role, "system");
        assert_eq!(m.content, "hello");
        assert!(m.images.is_empty(), "system messages should have no input images");
        assert!(m.generated_images.is_empty(), "system messages should have no gen images");
        assert!(m.generated_audios.is_empty(), "system messages should have no gen audios");
        assert!(m.timing.is_none(), "system messages should have no timing");
        // Timestamp is HH:MM:SS via timefmt::chat_now — 8 chars min.
        assert!(!m.timestamp.is_empty(), "timestamp should be populated");
    }

    #[test]
    fn chat_message_system_accepts_string_and_str() {
        // impl Into<String> covers both — pin so a future signature
        // change that drops the &str impl is caught.
        let a = ChatMessage::system("a");
        let b = ChatMessage::system(String::from("b"));
        assert_eq!(a.content, "a");
        assert_eq!(b.content, "b");
    }

    #[test]
    fn push_dedupes_consecutive_duplicates() {
        let mut c = ChatState::default();
        push_n(&mut c, &["a", "a", "b", "b", "b", "a"]);
        // a, b, a — consecutive duplicates collapsed; non-consecutive
        // (the second "a") is kept as a separate entry, matching the
        // shell convention where Up should still see the most-recent
        // use of each prompt.
        assert_eq!(c.prompt_history, vec!["a", "b", "a"]);
    }

    #[test]
    fn push_caps_at_history_limit_evicting_oldest() {
        let mut c = ChatState::default();
        for i in 0..PROMPT_HISTORY_LIMIT + 5 {
            c.push_prompt_history(format!("p{}", i));
        }
        assert_eq!(c.prompt_history.len(), PROMPT_HISTORY_LIMIT);
        // Front 5 evicted, back is the newest 5.
        assert_eq!(c.prompt_history.first().map(String::as_str), Some("p5"));
        assert_eq!(
            c.prompt_history.last().map(String::as_str),
            Some(format!("p{}", PROMPT_HISTORY_LIMIT + 4)).as_deref(),
        );
    }

    #[test]
    fn push_resets_cursor_and_draft() {
        let mut c = ChatState::default();
        push_n(&mut c, &["a", "b"]);
        c.history_cursor = Some(0);
        c.history_draft = "scratch".into();
        c.push_prompt_history("c".into());
        assert_eq!(c.history_cursor, None);
        assert!(c.history_draft.is_empty());
    }

    #[test]
    fn back_walks_history_and_snapshots_draft_once() {
        let mut c = ChatState::default();
        push_n(&mut c, &["one", "two", "three"]);
        c.input = "in-progress".into();

        c.history_back();
        assert_eq!(c.input, "three");
        assert_eq!(c.history_cursor, Some(2));
        // Draft snapshotted on first move only.
        assert_eq!(c.history_draft, "in-progress");

        c.history_back();
        assert_eq!(c.input, "two");
        // Draft must not be re-snapshotted (would clobber "in-progress").
        assert_eq!(c.history_draft, "in-progress");

        c.history_back();
        assert_eq!(c.input, "one");
        assert_eq!(c.history_cursor, Some(0));

        // Past oldest is a no-op (stays at index 0).
        c.history_back();
        assert_eq!(c.input, "one");
        assert_eq!(c.history_cursor, Some(0));
    }

    #[test]
    fn forward_restores_draft_at_end() {
        let mut c = ChatState::default();
        push_n(&mut c, &["one", "two"]);
        c.input = "draft".into();

        c.history_back();        // -> "two"
        c.history_back();        // -> "one"
        c.history_forward();     // -> "two"
        assert_eq!(c.input, "two");
        assert_eq!(c.history_cursor, Some(1));

        c.history_forward();     // past newest -> restore draft
        assert_eq!(c.input, "draft");
        assert_eq!(c.history_cursor, None);
        assert!(c.history_draft.is_empty());
    }

    #[test]
    fn back_on_empty_history_is_noop() {
        let mut c = ChatState {
            input: "x".into(),
            ..Default::default()
        };
        c.history_back();
        assert_eq!(c.input, "x");
        assert_eq!(c.history_cursor, None);
    }

    #[test]
    fn forward_when_not_navigating_is_noop() {
        let mut c = ChatState::default();
        push_n(&mut c, &["one"]);
        c.input = "x".into();
        c.history_forward();
        assert_eq!(c.input, "x");
        assert_eq!(c.history_cursor, None);
    }

    #[test]
    fn detach_after_edit_clears_cursor_and_draft() {
        // After Ctrl+Up loads a past prompt and the user starts
        // tweaking it, detach_history_if_edited should drop the
        // cursor so a subsequent Down doesn't clobber the edit.
        let mut c = ChatState::default();
        push_n(&mut c, &["one", "two"]);
        c.input = "draft".into();
        c.history_back();             // input = "two", cursor = Some(1), draft = "draft"
        assert_eq!(c.history_cursor, Some(1));

        // Simulate the user editing the recalled prompt.
        c.input = "two-edited".into();
        assert!(c.detach_history_if_edited());
        assert_eq!(c.history_cursor, None);
        assert!(c.history_draft.is_empty());
        // Edit preserved.
        assert_eq!(c.input, "two-edited");

        // Forward is now a no-op (cursor cleared) — edit survives.
        c.history_forward();
        assert_eq!(c.input, "two-edited");
    }

    #[test]
    fn detach_no_op_when_input_matches_recalled() {
        // The TextEdit fires response.changed() for any modification,
        // including ones we trigger ourselves when loading a recalled
        // prompt. Calling detach right after Ctrl+Up must NOT clear
        // the cursor, or Ctrl+Down would never work.
        let mut c = ChatState::default();
        push_n(&mut c, &["one", "two"]);
        c.history_back(); // input = "two", cursor = Some(1)
        assert!(!c.detach_history_if_edited());
        assert_eq!(c.history_cursor, Some(1));
    }

    #[test]
    fn detach_when_not_navigating_is_noop() {
        let mut c = ChatState::default();
        push_n(&mut c, &["one"]);
        c.input = "fresh".into();
        assert!(!c.detach_history_if_edited());
        assert_eq!(c.history_cursor, None);
    }

    #[test]
    fn push_silently_ignores_empty_input() {
        // Defensive guard against future callers that don't pre-filter.
        let mut c = ChatState::default();
        c.push_prompt_history(String::new());
        c.push_prompt_history("   ".into());
        c.push_prompt_history("\n\t".into());
        assert!(c.prompt_history.is_empty(), "no empty/whitespace entries should be stored");
    }

    // ── clear_conversation ─────────────────────────────────────────
    // The Clear-chat handler delegates here so test coverage on this
    // method directly governs what happens in the UI. Tests pin the
    // full set of fields that must reset (and equally important, the
    // ones that must NOT reset).

    #[test]
    fn clear_conversation_resets_locked_seed() {
        // The locked seed is the BUG that motivated extracting this
        // method: a leftover lock after Clear would silently bind
        // the next send to a seed from the discarded chat.
        let mut c = ChatState::default();
        c.locked_seed = Some(42);
        c.clear_conversation();
        assert_eq!(c.locked_seed, None);
    }

    #[test]
    fn clear_conversation_resets_image_num_steps() {
        // image_num_steps is a sticky per-chat preference. Clearing
        // the chat must drop it so the next session starts at server
        // defaults (Flux 4 / Z-Image 9) rather than inheriting the
        // tuning the user did for a now-discarded conversation.
        let mut c = ChatState::default();
        c.image_num_steps = Some(20);
        c.clear_conversation();
        assert_eq!(c.image_num_steps, None);
    }

    #[test]
    fn clear_conversation_resets_image_strength() {
        // image_strength is the img2img-counterpart to image_num_steps:
        // sticky per-chat, must drop on Clear so the next conversation
        // doesn't accidentally start with a low-strength override that
        // bakes the structure of a reference image from the previous conversation
        // into a different workflow.
        let mut c = ChatState::default();
        c.image_strength = Some(0.55);
        c.clear_conversation();
        assert_eq!(c.image_strength, None);
    }

    #[test]
    fn clear_conversation_resets_image_size() {
        // image_size is sticky per-chat like the other image_* knobs.
        // Clearing must drop the dimension override so a fresh chat
        // doesn't try to use 1280x720 against a model whose defaults
        // are tuned for 512² or 1024².
        let mut c = ChatState::default();
        c.image_size = Some((1280, 720));
        c.clear_conversation();
        assert_eq!(c.image_size, None);
    }

    #[test]
    fn clear_conversation_drops_all_per_conversation_fields() {
        let mut c = ChatState::default();
        c.messages.push_back(ChatMessage {
            role: "user".into(),
            content: "hi".into(),
            ..Default::default()
        });
        c.attached_images.push("data".into());
        c.attached_image_paths.push("/tmp/x.png".into());
        c.streaming_content = "partial".into();
        c.image_gen_progress = Some((3, 10));
        c.image_gen_started_at = Some(std::time::Instant::now());
        c.locked_seed = Some(7);
        c.image_num_steps = Some(16);
        c.image_strength = Some(0.42);
        c.image_size = Some((1024, 768));

        c.clear_conversation();

        assert!(c.messages.is_empty());
        assert!(c.attached_images.is_empty());
        assert!(c.attached_image_paths.is_empty());
        assert!(c.streaming_content.is_empty());
        assert_eq!(c.image_gen_progress, None);
        assert!(c.image_gen_started_at.is_none());
        assert_eq!(c.locked_seed, None);
        assert_eq!(c.image_num_steps, None);
        assert_eq!(c.image_strength, None);
        assert_eq!(c.image_size, None);
    }

    #[test]
    fn abort_generation_returns_false_when_idle() {
        // No live generation → no-op + report nothing-to-cancel.
        // Without this gate, Esc on an empty chat would push a
        // bogus "[Generation cancelled by user]" system message.
        let mut c = ChatState::default();
        assert!(!c.is_generating);
        assert!(!c.abort_generation());
        assert!(c.messages.is_empty(),
            "abort on idle must not push any system breadcrumb");
    }

    #[test]
    fn prepare_for_generation_flips_state_to_in_flight_and_clears_stale_progress() {
        // Set up a state that looks like the leftovers of a prior
        // generation that was killed mid-stream (the defensive
        // reset case prepare_for_generation exists for):
        //   - is_generating already false (cleanup didn't fire)
        //   - streaming_content has stale partial response
        //   - image_gen_progress + started_at left at mid-gen values
        //   - generation_abort points at a now-completed handle
        // (We can't construct an AbortHandle without tokio; the
        //  generation_abort = None reset is what we verify.)
        let mut c = ChatState::default();
        c.streaming_content = "stale partial".into();
        c.image_gen_progress = Some((7, 30));
        c.image_gen_started_at = Some(std::time::Instant::now());

        c.prepare_for_generation();

        assert!(c.is_generating, "new generation must flip is_generating true");
        assert!(c.streaming_content.is_empty(),
            "stale partial must be cleared before the new stream writes");
        assert_eq!(c.image_gen_progress, None,
            "stale progress bar must be cleared");
        assert!(c.image_gen_started_at.is_none(),
            "stale ETA anchor must be cleared");
        assert!(c.generation_abort.is_none());
    }

    #[test]
    fn prepare_for_generation_preserves_messages_and_input_draft() {
        // The helper must NOT touch chat-content fields. Pin so a
        // future refactor that confuses "prepare for generation"
        // with "reset chat" can't accidentally delete the
        // conversation history when the user sends.
        let mut c = ChatState::default();
        c.messages.push_back(ChatMessage {
            role: "assistant".into(),
            content: "Earlier reply".into(),
            ..Default::default()
        });
        c.input = "User's just-cleared input would normally be empty here".into();
        c.prompt_history.push("earlier prompt".into());

        c.prepare_for_generation();

        assert_eq!(c.messages.len(), 1, "messages preserved across prepare");
        assert_eq!(c.messages[0].content, "Earlier reply");
        assert_eq!(c.input, "User's just-cleared input would normally be empty here",
            "input field NOT cleared here (send_chat does the input.clear() \
             explicitly above the prepare call)");
        assert_eq!(c.prompt_history.len(), 1,
            "prompt_history preserved — recall must keep working");
    }

    #[test]
    fn reset_streaming_state_clears_five_fields_without_returning_bool() {
        // Direct coverage for the helper introduced in commit
        // 6a51677. Pin that all five resettable fields go back to
        // their idle defaults so a future field addition either
        // (a) gets included in the reset and this test extends to
        // cover it, or (b) gets noticed via a state-leak symptom
        // because the test failed to update.
        //
        // Distinct from abort_generation_resets_full_streaming_state
        // which exercises this helper via abort_generation. If a
        // future refactor decouples the two (e.g. abort_generation
        // gets renamed / merged elsewhere) the helper still has
        // direct test coverage.
        let mut c = ChatState::default();
        c.is_generating = true;
        c.streaming_content = "partial".into();
        c.image_gen_progress = Some((3, 10));
        c.image_gen_started_at = Some(std::time::Instant::now());

        c.reset_streaming_state();

        assert!(!c.is_generating);
        assert!(c.generation_abort.is_none(),
            "generation_abort must be None after reset — leaving a stale \
             handle was the bug fixed by 8a33e82");
        assert!(c.streaming_content.is_empty());
        assert_eq!(c.image_gen_progress, None);
        assert!(c.image_gen_started_at.is_none());
    }

    #[test]
    fn abort_generation_resets_full_streaming_state() {
        // Live generation: must abort + reset all six streaming fields
        // in one shot. The helper owns that cleanup so the Stop button and the Esc
        // shortcut share one code path and cannot drift apart.
        let mut c = ChatState::default();
        c.is_generating = true;
        c.streaming_content = "partial response".into();
        c.image_gen_progress = Some((5, 20));
        c.image_gen_started_at = Some(std::time::Instant::now());
        // generation_abort is None — can't construct an AbortHandle in
        // a unit test without a tokio runtime, but the take()+abort()
        // path is exercised by the chat_tab integration in practice
        // (and is a one-liner). The state-mutation part is what we
        // test here because it's what drives the UI.

        let was_live = c.abort_generation();

        assert!(was_live, "must report there was a live generation to cancel");
        assert!(!c.is_generating, "is_generating flips to false");
        assert!(c.streaming_content.is_empty(), "partial stream is dropped");
        assert_eq!(c.image_gen_progress, None);
        assert!(c.image_gen_started_at.is_none());
    }

    #[test]
    fn abort_generation_preserves_messages_and_input() {
        // Aborting the in-flight task must NOT wipe what the user
        // already received or what they were typing. The caller
        // chooses whether to push a breadcrumb message — abort itself
        // stays out of the message log to keep the helper reusable.
        let mut c = ChatState::default();
        c.is_generating = true;
        c.messages.push_back(ChatMessage {
            role: "assistant".into(),
            content: "Previously-completed answer".into(),
            ..Default::default()
        });
        c.input = "user's next draft, mid-type".into();

        c.abort_generation();

        assert_eq!(c.messages.len(), 1, "prior messages survive abort");
        assert_eq!(c.messages[0].content, "Previously-completed answer");
        assert_eq!(c.input, "user's next draft, mid-type",
            "live input survives abort — Esc shouldn't lose the next draft");
    }

    #[test]
    fn add_attachment_keeps_parallel_vecs_in_sync() {
        // The .len() == .len() invariant is what the chip-row render
        // relies on — zipping the two vecs by index would render the
        // wrong preview / filename for the desynced entry. Pin that
        // the helper trio (add / remove / clear / take) preserves it.
        let mut c = ChatState::default();
        assert_eq!(c.attached_images.len(), c.attached_image_paths.len());

        c.add_attachment("b64a".into(), "/tmp/a.png".into());
        c.add_attachment("b64b".into(), "/tmp/b.png".into());
        c.add_attachment("b64c".into(), "/tmp/c.png".into());
        assert_eq!(c.attached_images.len(), 3);
        assert_eq!(c.attached_image_paths.len(), 3);
        // Pairs are at matching indices.
        assert_eq!(c.attached_images[1], "b64b");
        assert_eq!(c.attached_image_paths[1], "/tmp/b.png");
    }

    #[test]
    fn remove_attachment_removes_both_or_neither() {
        let mut c = ChatState::default();
        c.add_attachment("b64a".into(), "/tmp/a.png".into());
        c.add_attachment("b64b".into(), "/tmp/b.png".into());
        c.add_attachment("b64c".into(), "/tmp/c.png".into());

        // In-bounds remove returns true and drops both at idx.
        assert!(c.remove_attachment(1));
        assert_eq!(c.attached_images.len(), 2);
        assert_eq!(c.attached_image_paths.len(), 2);
        assert_eq!(c.attached_images, vec!["b64a", "b64c"]);
        assert_eq!(c.attached_image_paths, vec!["/tmp/a.png", "/tmp/c.png"]);

        // Out-of-bounds is a no-op (returns false, doesn't panic).
        assert!(!c.remove_attachment(99));
        assert_eq!(c.attached_images.len(), 2);
        assert_eq!(c.attached_image_paths.len(), 2);
    }

    #[test]
    fn take_attachments_moves_b64_and_clears_paths_in_lockstep() {
        // The send_chat path needs the base64 vec by move (HTTP
        // worker takes ownership) AND a fresh-empty paths vec. The
        // helper packages both ops so the state remains invariant.
        let mut c = ChatState::default();
        c.add_attachment("b64a".into(), "/tmp/a.png".into());
        c.add_attachment("b64b".into(), "/tmp/b.png".into());

        let taken = c.take_attachments();
        assert_eq!(taken, vec!["b64a", "b64b"]);
        assert!(c.attached_images.is_empty());
        assert!(c.attached_image_paths.is_empty(),
            "paths must clear in lockstep with the b64 take — otherwise \
             the next render would show ghost chip filenames");
    }

    #[test]
    fn clear_attachments_resets_both_vecs() {
        let mut c = ChatState::default();
        c.add_attachment("b64a".into(), "/tmp/a.png".into());
        c.add_attachment("b64b".into(), "/tmp/b.png".into());

        c.clear_attachments();
        assert!(c.attached_images.is_empty());
        assert!(c.attached_image_paths.is_empty());
    }

    #[test]
    fn clear_conversation_preserves_prompt_history_and_input() {
        // Per the docstring: prompt_history is session-scoped (Up/Down
        // scrollback must keep working after Clear) and input is the
        // live editor (user may have already typed the next prompt
        // before clicking Clear).
        let mut c = ChatState::default();
        c.push_prompt_history("earlier prompt".into());
        c.input = "user is already typing the next one".into();
        c.messages.push_back(ChatMessage {
            role: "assistant".into(),
            content: "old".into(),
            ..Default::default()
        });

        c.clear_conversation();

        assert!(c.messages.is_empty(), "conversation cleared");
        assert_eq!(c.prompt_history, vec!["earlier prompt"], "history preserved");
        assert_eq!(c.input, "user is already typing the next one", "live input preserved");
    }
}

#[cfg(test)]
mod cli_history_tests {
    use super::*;

    fn push_n(c: &mut CLIState, cmds: &[&str]) {
        for cmd in cmds {
            c.push_history((*cmd).to_string());
        }
    }

    #[test]
    fn push_dedupes_consecutive() {
        let mut c = CLIState::default();
        push_n(&mut c, &["list", "list", "ps", "ps", "list"]);
        assert_eq!(c.history, vec!["list", "ps", "list"]);
        // history_index parks at one-past-end after each push.
        assert_eq!(c.history_index, 3);
    }

    #[test]
    fn push_caps_at_cli_history_limit() {
        let mut c = CLIState::default();
        for i in 0..CLI_HISTORY_LIMIT + 10 {
            c.push_history(format!("cmd{}", i));
        }
        assert_eq!(c.history.len(), CLI_HISTORY_LIMIT);
        assert_eq!(c.history.first().map(String::as_str), Some("cmd10"));
    }

    #[test]
    fn push_output_caps_at_cli_output_limit() {
        // Pin the unbounded-growth fix for cli.outputs. Without the
        // cap, a long-running session that issued thousands of
        // commands would accumulate every output forever — visibly
        // slowing down the CLI scroll area (rendered every frame)
        // and leaking memory for the duration of the session.
        let mut c = CLIState::default();
        for i in 0..CLI_OUTPUT_LIMIT + 25 {
            c.push_output(CLIOutput {
                timestamp: format!("12:{:02}", i % 60),
                command: format!("cmd{}", i),
                output: format!("output {}", i),
                ..Default::default()
            });
        }
        assert_eq!(c.outputs.len(), CLI_OUTPUT_LIMIT);
        // Oldest 25 evicted in FIFO order — first surviving entry
        // is cmd25.
        assert_eq!(
            c.outputs.first().map(|o| o.command.as_str()),
            Some("cmd25"),
        );
        // Newest is the most-recent push (cmd N-1 for N total).
        assert_eq!(
            c.outputs.last().map(|o| o.command.as_str()),
            Some(format!("cmd{}", CLI_OUTPUT_LIMIT + 24).as_str()),
        );
    }

    #[test]
    fn push_output_below_cap_is_pure_append() {
        // Under the cap: push behaves like Vec::push, ordered.
        let mut c = CLIState::default();
        for i in 0..3 {
            c.push_output(CLIOutput {
                command: format!("cmd{}", i),
                ..Default::default()
            });
        }
        assert_eq!(c.outputs.len(), 3);
        assert_eq!(c.outputs[0].command, "cmd0");
        assert_eq!(c.outputs[2].command, "cmd2");
    }

    #[test]
    fn back_then_forward_round_trip_preserves_draft() {
        let mut c = CLIState::default();
        push_n(&mut c, &["a", "b", "c"]);
        c.input = "draft-cmd".into();

        c.history_back();                    // -> "c"
        assert_eq!(c.input, "c");
        assert_eq!(c.history_draft, "draft-cmd");

        c.history_back();                    // -> "b"
        c.history_back();                    // -> "a"
        // Past oldest is no-op.
        c.history_back();
        assert_eq!(c.input, "a");

        c.history_forward();                 // -> "b"
        c.history_forward();                 // -> "c"
        c.history_forward();                 // -> draft
        assert_eq!(c.input, "draft-cmd");
        assert_eq!(c.history_index, 3);
        assert!(c.history_draft.is_empty());

        // Past one-past-end is no-op.
        c.history_forward();
        assert_eq!(c.input, "draft-cmd");
    }

    #[test]
    fn back_on_empty_history_is_noop() {
        let mut c = CLIState {
            input: "x".into(),
            ..Default::default()
        };
        c.history_back();
        assert_eq!(c.input, "x");
    }

    #[test]
    fn push_silently_ignores_empty_input() {
        let mut c = CLIState::default();
        c.push_history(String::new());
        c.push_history("   ".into());
        c.push_history("\t\n".into());
        assert!(c.history.is_empty(), "no empty/whitespace entries should be stored");
    }
}

#[cfg(test)]
mod select_best_model_tests {
    use super::*;
    use crate::api::types::ModelInfo;

    fn mi(name: &str, size_bytes: u64) -> ModelInfo {
        ModelInfo {
            name: name.to_string(),
            size: String::new(),
            size_bytes,
            modified_at: String::new(),
            source: "ollama".to_string(),
            family: String::new(),
            capabilities: Vec::new(),
            defaults: None,
        }
    }

    #[test]
    fn keeps_current_selection_when_still_available() {
        let mut s = ModelState {
            available_models: vec![mi("a", 1), mi("b", 2)],
            selected_model: Some("a".into()),
            ..Default::default()
        };
        s.select_best_model();
        assert_eq!(s.selected_model.as_deref(), Some("a"));
    }

    #[test]
    fn drops_selection_when_no_longer_available_and_picks_loaded() {
        // The previously-selected model has been deleted from disk; a
        // different model is loaded — prefer the loaded one.
        let mut s = ModelState {
            available_models: vec![mi("a", 100), mi("c", 50)],
            loaded_models: vec!["c".into()],
            selected_model: Some("missing".into()),
            ..Default::default()
        };
        s.select_best_model();
        assert_eq!(s.selected_model.as_deref(), Some("c"));
    }

    #[test]
    fn picks_smallest_available_when_nothing_loaded() {
        let mut s = ModelState {
            available_models: vec![mi("big", 30), mi("small", 5), mi("mid", 10)],
            ..Default::default()
        };
        s.select_best_model();
        assert_eq!(
            s.selected_model.as_deref(),
            Some("small"),
            "smallest model should win when nothing is loaded yet",
        );
    }

    #[test]
    fn leaves_selection_none_when_no_models_at_all() {
        let mut s = ModelState::default();
        s.select_best_model();
        assert!(s.selected_model.is_none());
    }

    #[test]
    fn prefers_loaded_over_smaller_available() {
        // Loaded model is bigger than a non-loaded option — loaded
        // still wins (avoids forcing the user to wait for another
        // model to load).
        let mut s = ModelState {
            available_models: vec![mi("tiny", 1), mi("loaded-big", 100)],
            loaded_models: vec!["loaded-big".into()],
            ..Default::default()
        };
        s.select_best_model();
        assert_eq!(s.selected_model.as_deref(), Some("loaded-big"));
    }
}

#[cfg(test)]
mod media_persist_tests {
    use super::*;

    /// The persist snapshot must round-trip through JSON (the config format)
    /// and re-hydrate every tuned parameter (blobs excluded by design).
    #[test]
    fn media_persist_round_trips_through_json() {
        let mut m = MediaState::default();
        m.kind = MediaKind::Music;
        m.prompt = "epic".into();
        m.seed = "42".into();
        m.music.seconds = 300.0;
        m.music.dit_model = "xl-sft".into();
        m.music.lyrics = "[verse]\nhello".into();
        m.music.cfg = 4.5;
        m.video.sampler = VideoSampler::Heun;
        m.image.loras = vec![("lcm".into(), 0.75)];
        m.image.sampler = "dpmpp_2m".into();
        m.image.scheduler = "karras".into();
        m.image_edit.strength = 0.33;
        m.image_edit.source = Some(("x.png".into(), vec![1, 2, 3]));
        let json = serde_json::to_string(&m.to_persist()).unwrap();
        let back: MediaPersist = serde_json::from_str(&json).unwrap();
        let mut m2 = MediaState::default();
        m2.apply_persist(&back);
        assert_eq!(m2.kind, MediaKind::Music);
        assert_eq!(m2.prompt, "epic");
        assert_eq!(m2.seed, "42");
        assert_eq!(m2.music.seconds, 300.0);
        assert_eq!(m2.music.dit_model, "xl-sft");
        // The adapter selection is a tuned parameter, not a blob: losing it across a
        // restart would silently render the base model on the next click.
        assert_eq!(m2.image.loras, vec![("lcm".to_string(), 0.75)]);
        assert_eq!(m2.image.sampler, "dpmpp_2m");
        assert_eq!(m2.image.scheduler, "karras");
        assert_eq!(m2.music.lyrics, "[verse]\nhello");
        assert_eq!(m2.music.cfg, 4.5);
        assert_eq!(m2.video.sampler, VideoSampler::Heun);
        assert_eq!(m2.image_edit.strength, 0.33);
        // Blobs are session-only by design.
        assert!(m2.image_edit.source.is_none());
    }
}

#[cfg(test)]
mod media_slot_tests {
    use super::MediaAudioSlot;

    /// The picker chooses its file filter and its button label from `is_image`. A slot
    /// missing from that match would not compile; a slot answering WRONG offers an
    /// audio filter for a photo, which is the bug this replaced.
    #[test]
    fn every_slot_declares_its_kind() {
        // Kept in step with the enum ON PURPOSE: this assertion is the thing that fails
        // when someone adds a slot and forgets the array, which is exactly what had
        // happened to both video slots - the array said a scan could not miss one, and the
        // scan was missing two.
        assert_eq!(
            MediaAudioSlot::ALL.len(),
            6,
            "a new slot must be added to ALL"
        );
        let images: Vec<_> = MediaAudioSlot::ALL.iter().filter(|s| s.is_image()).collect();
        assert_eq!(
            images.len(),
            3,
            "the image slots are edit, pose and the video start frame"
        );
        assert!(MediaAudioSlot::VideoStartImage.is_image());
        assert!(MediaAudioSlot::ImageControl.is_image());
        assert!(MediaAudioSlot::EditImage.is_image());
        // Distinct cache keys, or two slots share a preview entry and one shows the
        // other's picture - which is worse than no preview, because it looks right.
        let mut keys: Vec<&str> = MediaAudioSlot::ALL.iter().map(|s| s.cache_key()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "two slots share a preview cache key");
    }
}

#[cfg(test)]
mod video_duration_tests {
    use super::*;

    /// A duration becomes a length the temporal VAE can express exactly, and never a
    /// shorter one than was asked for.
    #[test]
    fn a_duration_rounds_up_onto_the_four_k_plus_one_grid() {
        for secs in [0.5f32, 1.0, 1.4, 3.0, 5.0, 5.1, 12.0, 40.0] {
            let f = frames_for_seconds(secs);
            assert_eq!((f - 1) % 4, 0, "{secs}s gave {f}, which the VAE cannot express");
            assert!(
                seconds_for_frames(f) + 1e-6 >= secs,
                "{secs}s became {:.3}s - shorter than asked",
                seconds_for_frames(f)
            );
            // And not wastefully longer: at most one grid step over.
            assert!(seconds_for_frames(f) - secs < 4.0 / VIDEO_FPS + 1e-6);
        }
    }

    /// The reference's own default, five seconds, lands exactly on 81.
    #[test]
    fn five_seconds_is_the_models_own_eighty_one_frames() {
        assert_eq!(frames_for_seconds(5.0), 81);
        assert_eq!(frames_for_seconds(1.0), 17);
    }

    /// A degenerate duration still produces a renderable clip rather than zero frames.
    #[test]
    fn a_zero_or_negative_duration_still_renders_one_frame() {
        assert_eq!(frames_for_seconds(0.0), 1);
        assert_eq!(frames_for_seconds(-3.0), 1);
    }
}

#[cfg(test)]
mod video_estimate_key_tests {
    use super::*;

    /// What the estimate depends on is in the key; what it does not depend on is not.
    /// Getting this wrong costs a request per keystroke on the prompt.
    #[test]
    fn only_the_settings_that_move_the_cost_move_the_key() {
        let mut m = MediaState::default();
        let base = m.video_estimate_key();

        m.prompt = "a boat at sea".to_string();
        m.seed = "1234".to_string();
        m.video.negative_prompt = "blurry".to_string();
        m.video.cfg = 7.5;
        assert_eq!(m.video_estimate_key(), base, "a free field re-asked the server");

        let changes: [(&str, fn(&mut MediaState)); 5] = [
            ("width", |m| m.video.width = 640),
            ("height", |m| m.video.height = 640),
            ("steps", |m| m.video.steps = 20),
            ("seconds", |m| m.video.seconds = 30.0),
            ("model", |m| m.video.model = "wan-i2v-14b".to_string()),
        ];
        for (name, change) in changes {
            let mut m = MediaState::default();
            change(&mut m);
            assert_ne!(m.video_estimate_key(), base, "{name} did not move the key");
        }
    }

    /// Two durations that render the SAME frames cost the same, so they must not be asked
    /// about twice - the VAE only renders 4k+1 frames.
    #[test]
    fn durations_landing_on_the_same_frame_count_share_a_key() {
        let mut a = MediaState::default();
        let mut b = MediaState::default();
        a.video.seconds = 3.0;
        b.video.seconds = 3.0 + 1.0 / (4.0 * VIDEO_FPS);
        assert_eq!(frames_for_seconds(a.video.seconds), frames_for_seconds(b.video.seconds));
        assert_eq!(a.video_estimate_key(), b.video_estimate_key());
    }

    /// A fresh state has no estimate and nothing in flight: the tab shows nothing until the
    /// server has answered, rather than a zero that reads as "instant".
    #[test]
    fn nothing_is_shown_before_an_answer_arrives() {
        let m = MediaState::default();
        assert!(m.video_estimate.is_none());
        assert!(m.estimate_key.is_none());
        assert!(!m.estimate_in_flight);
    }
}
