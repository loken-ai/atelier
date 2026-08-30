//! Async Task Results
//!
//! Types for handling async operation results in the GUI.

use crate::api::{LoadModelResponse, ModelInfo};

/// Successful output of a Media Studio generation, normalised across
/// every modality so the single `TaskResult::MediaResult` handler can
/// route it into `MediaState`:
///   - `images`: base64 PNGs (rendered inline)
///   - `audios`: base64 WAVs (played via the system player / saved)
///   - `files`:  (filename, bytes) blobs that can't render in-GUI
///     (MIDI, video) — offered as Save + Open
///   - `status`: optional human-readable completion note
#[derive(Debug, Clone, Default)]
pub struct MediaOutput {
    pub images: Vec<String>,
    pub audios: Vec<String>,
    pub files: Vec<(String, Vec<u8>)>,
    /// Text payload (Transcribe kind).
    pub text: Option<String>,
    pub status: String,
}

/// Result of an async task
pub enum TaskResult {
    /// Media Studio generation finished (image / music / SFX / MIDI /
    /// video / speech). Ok carries the normalised MediaOutput; Err is a
    /// user-facing error string surfaced as the tab's error banner.
    MediaResult(Result<MediaOutput, String>),
    /// Intermediate snapshot of a multi-variation run: carries everything
    /// produced SO FAR so finished variations render while the rest keep
    /// generating (the final `MediaResult` still closes the run).
    MediaPartial(MediaOutput),
    /// Prompt-enhancer finished: Ok = the rewritten prompt, Err = message.
    MediaPromptEnhanced(Result<String, String>),
    /// Smart-routed chat turn finished (/conversation). Ok carries the
    /// parsed response (route, model, content, optional image/audio).
    ConversationResult(Result<crate::api::client::ConversationOutput, String>),
    /// Media Studio streaming progress from an SSE `rendering` event: the phase's
    /// human label, then `(step, total)`.
    ///
    /// `total == 0` is a phase with nothing to count - a load, an encode - and the tab
    /// shows its name without a bar. That case is most of a render's wall time and used
    /// to be a bare spinner, which cannot be told apart from a hang.
    MediaProgress(String, u64, u64),
    /// Media Studio voice list fetched (GET /v1/audio/voices).
    MediaVoicesFetched(Vec<String>),
    /// What a video render is expected to cost, from `POST /v1/video/plan`.
    ///
    /// Tagged with the settings signature it was asked for, because the settings can move
    /// while the answer is in flight - an untagged reply would be shown against a frame
    /// size or a length it was never computed for. `None` seconds = the server could not
    /// answer, and the tab shows nothing rather than a stale number.
    VideoEstimate {
        key: String,
        seconds: Option<f32>,
    },
    /// Models list fetched
    ModelsFetched(Vec<ModelInfo>, Vec<String>),
    /// The LoRA adapter names the server can apply, from `/v1/loras`. Fetched with the
    /// model list so the picker is populated before the user opens it.
    /// Adapter names with the architecture each was trained for (`None` = the
    /// server does not recognise the layout).
    LorasFetched(Vec<(String, Option<String>)>),
    /// Model load completed
    ModelLoaded(Result<LoadModelResponse, String>, String),
    /// Model delete completed
    ModelDeleted(Result<(), String>, String),
    /// Model pull completed
    ModelPulled(Result<(), String>, String),
    /// Model pull streaming progress `(completed_bytes, total_bytes)`
    /// parsed from the NDJSON `/api/pull` stream. Drives the determinate
    /// download bar in the Models settings tab; `total == 0` = manifest
    /// phase (indeterminate).
    PullProgress(u64, u64),
    /// Chat response received (content, timing_info, generated_audios).
    /// The audios slot carries base64-encoded WAVs from TTS pipeline
    /// responses (server's handle_chat_tts) and is normally empty for
    /// plain text chats.
    ChatResponse(Result<(String, Option<crate::state::MessageTiming>, Vec<String>), String>),
    /// Image generation completed (generated_images base64)
    ImageGenResponse(Result<Vec<String>, String>),
    /// Generic error
    Error(String),
    /// CLI-triggered model load with original command for feedback
    CLILoadModel(Result<LoadModelResponse, String>, String, String),
    /// CLI-triggered model pull with original command for feedback  
    CLIPullModel(Result<(), String>, String, String),
    /// CLI-triggered model delete with original command for feedback
    CLIDeleteModel(Result<(), String>, String, String),
    /// Model unload completed
    ModelUnloaded(Result<(), String>, String),
}
