//! API types for GUI client communication
//!
//! Subset of the server's API types needed for HTTP client operations.
//! These are wire-format structs matching the Ollama/OpenAI API protocols.

use serde::{Deserialize, Serialize};

// ============================================================================
// Common Types
// ============================================================================

/// Model source for pull/list operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelSource {
    Ollama,
    HuggingFace,
}

impl std::fmt::Display for ModelSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelSource::Ollama => write!(f, "ollama"),
            ModelSource::HuggingFace => write!(f, "huggingface"),
        }
    }
}

/// Tool definition for tool calling / function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<ToolFunction>,
}

/// Tool function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// Model's tool call in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<ToolCallFunction>,
}

/// Function call details in model response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

// ============================================================================
// Ollama-compatible Types
// ============================================================================

/// Ollama chat request (POST /api/chat)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
}

impl OllamaChatRequest {
    pub fn new(model: String, messages: Vec<Message>) -> Self {
        Self {
            model,
            messages,
            stream: false,
            format: None,
            options: None,
            keep_alive: None,
            thinking: None,
            tools: None,
        }
    }
}

/// Ollama chat response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatResponse {
    pub model: String,
    pub created_at: String,
    pub message: Message,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_reason: Option<String>,
    #[serde(default)]
    pub total_duration: Option<u64>,
    #[serde(default)]
    pub load_duration: Option<u64>,
    #[serde(default)]
    pub prompt_eval_count: Option<u64>,
    #[serde(default)]
    pub prompt_eval_duration: Option<u64>,
    #[serde(default)]
    pub eval_count: Option<u64>,
    #[serde(default)]
    pub eval_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vision_processed: Option<bool>,
}

/// Ollama generate request (POST /api/generate)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaGenerateRequest {
    pub model: String,
    pub prompt: String,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

impl OllamaGenerateRequest {
    pub fn new(model: String, prompt: String) -> Self {
        Self {
            model,
            prompt,
            stream: false,
            format: None,
            options: None,
            keep_alive: None,
            thinking: None,
            images: None,
        }
    }
}

/// Ollama generate response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaGenerateResponse {
    pub model: String,
    pub created_at: String,
    pub response: String,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_reason: Option<String>,
    #[serde(default)]
    pub context: Option<Vec<i32>>,
    #[serde(default)]
    pub total_duration: Option<u64>,
    #[serde(default)]
    pub load_duration: Option<u64>,
    #[serde(default)]
    pub prompt_eval_count: Option<u64>,
    #[serde(default)]
    pub prompt_eval_duration: Option<u64>,
    #[serde(default)]
    pub eval_count: Option<u64>,
    #[serde(default)]
    pub eval_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vision_processed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

/// Ollama model information (from /api/tags)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub modified_at: String,
    pub size: u64,
    #[serde(default)]
    pub digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<OllamaModelDetails>,
    #[serde(default = "default_model_source_str")]
    pub source: String,
    /// Server-declared capabilities ("txt2img", "edit", "sfx", "music", ...).
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Server-declared recommended defaults ({"steps":..,"cfg":..}, ...).
    #[serde(default)]
    pub defaults: Option<serde_json::Value>,
}

fn default_model_source_str() -> String {
    "ollama".to_string()
}

/// Ollama model details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelDetails {
    pub format: String,
    pub family: String,
    pub parameter_size: String,
    #[serde(default)]
    pub quantization_level: Option<String>,
}

/// Ollama list models response (GET /api/tags)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaListModelsResponse {
    pub models: Vec<OllamaModel>,
}

/// Ollama pull request (POST /api/pull)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaPullRequest {
    pub name: String,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insecure: Option<bool>,
    #[serde(default = "default_model_source")]
    pub source: String,
}

fn default_model_source() -> String {
    "ollama".to_string()
}

impl OllamaPullRequest {
    pub fn new(name: String) -> Self {
        Self {
            name,
            stream: false,
            insecure: None,
            source: "ollama".to_string(),
        }
    }
}

/// Ollama pull response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaPullResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<u64>,
}

/// Ollama delete request (DELETE /api/delete)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaDeleteRequest {
    pub name: String,
}

// ============================================================================
// Common Types (used by both Ollama and OpenAI-compatible APIs)
// ============================================================================

/// Chat message (compatible with both Ollama and OpenAI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    /// Audio payloads (base64-encoded WAV) returned by TTS pipelines.
    /// Server-side: emitted by handle_chat_tts on /api/chat for
    /// parler/kokoro/etc. - see api/handlers.rs:handle_chat_tts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audios: Option<Vec<String>>,
}

impl Message {
    pub fn new(role: String, content: String) -> Self {
        Self {
            role,
            content,
            images: None,
            audios: None,
        }
    }

    pub fn with_images(role: String, content: String, images: Vec<String>) -> Self {
        Self {
            role,
            content,
            images: Some(images),
            audios: None,
        }
    }
}

/// Model information (unified format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub size: String,
    pub size_bytes: u64,
    pub modified_at: String,
    #[serde(default = "default_model_source_str")]
    pub source: String,
    /// Model family from /api/tags (`details.family`, e.g. "qwen", "flux",
    /// "z-image", "qwen-image"). Drives capability-aware UI filtering - e.g. the
    /// Media Studio image-model dropdown lists only image-gen families.
    #[serde(default)]
    pub family: String,
    /// Server-declared capabilities - the GUI builds every model picker from
    /// these instead of hardcoding model names.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Server-declared recommended defaults for this model's knobs.
    #[serde(default)]
    pub defaults: Option<serde_json::Value>,
}

impl ModelInfo {
    pub fn from_ollama(model: OllamaModel) -> Self {
        Self {
            name: model.name,
            size: format_size(model.size),
            size_bytes: model.size,
            modified_at: model.modified_at,
            source: model.source,
            family: model.details.map(|d| d.family).unwrap_or_default(),
            capabilities: model.capabilities,
            defaults: model.defaults,
        }
    }

    /// Whether the server declared `cap` for this model.
    pub fn has_capability(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c == cap)
    }

    /// Server-recommended default for a numeric knob, if declared.
    pub fn default_f64(&self, key: &str) -> Option<f64> {
        self.defaults.as_ref()?.get(key)?.as_f64()
    }

    /// True when this model is a text-to-image generation model (its family is
    /// one the server's image engine can serve). Kept in sync with the server's
    /// image-gen families - new image models appear automatically once the
    /// server classifies them into one of these families.
    /// A text-to-video model. The server reports the family, so a fine-tune dropped in
    /// its directory is offered without a client change.
    pub fn is_video_gen(&self) -> bool {
        self.family == "video"
    }

    pub fn is_image_gen(&self) -> bool {
        matches!(
            self.family.as_str(),
            "flux" | "flux2" | "z-image" | "qwen-image" | "boogu" | "sdxl" | "stable-diffusion"
        )
    }
}

/// Format a byte count for a human. Picks the largest unit that still yields a
/// number at or above one - KB, MB, GB, TB - so a multi-gigabyte model reads as
/// a handful of GB rather than five digits of MB.
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;
    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ============================================================================
// Media Studio - /v1 generation endpoint response shapes
// ============================================================================

/// Response envelope for POST /v1/images/generations and
/// POST /v1/audio/generations (music / SFX). Both return an OpenAI-shaped
/// `{ "data": [ { "b64_json": "..." }, ... ] }` payload.
#[derive(Debug, Clone, Deserialize)]
pub struct ImagesGenerationResponse {
    pub data: Vec<MediaDatum>,
    /// Things the server did differently from what was asked, in its own words. Empty in
    /// the ordinary case. Carried because a note nobody reads is the same as silence.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Wall time of the render, reported by the server.
    #[serde(default)]
    pub render_ms: Option<u64>,
    /// Energy drawn over the render (J, CPU+GPU domains), when measurable.
    #[serde(default)]
    pub energy_j: Option<f64>,
}


/// One item in a media-generation `data` array. `b64_json` is the
/// base64 payload (PNG for images, WAV for audio, .mid bytes for MIDI,
/// mp4/gif for video). `content_type` disambiguates when present.
#[derive(Debug, Clone, Deserialize)]
pub struct MediaDatum {
    pub b64_json: String,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
}

/// Response envelope for GET /v1/audio/voices - a `{ "data": [ { "voice":
/// "alloy", ... }, ... ] }` list of the voices the TTS model exposes.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VoicesResponse {
    #[serde(default)]
    pub data: Vec<VoiceDatum>,
}

/// One voice entry from GET /v1/audio/voices.
#[derive(Debug, Clone, Deserialize)]
pub struct VoiceDatum {
    pub voice: String,
}

#[cfg(test)]
mod size_tests {
    use super::format_size;

    #[test]
    fn format_size_picks_largest_unit_with_one_decimal() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_size(7_500_000_000), "6.98 GB");
        assert_eq!(format_size(1024_u64.pow(4)), "1.00 TB");
    }

    #[test]
    fn format_size_boundary_transitions() {
        // The boundary between units (e.g. 1023 -> 1024) is where
        // off-by-one bugs hide. Pin every transition:
        assert_eq!(format_size(1023), "1023 B");           // just under KB
        assert_eq!(format_size(1024), "1.0 KB");           // exactly KB
        assert_eq!(format_size(1024 * 1024 - 1), "1024.0 KB"); // just under MB
        assert_eq!(format_size(1024 * 1024), "1.0 MB");    // exactly MB
        assert_eq!(format_size(1024_u64.pow(3) - 1), "1024.0 MB"); // just under GB
        assert_eq!(format_size(1024_u64.pow(3)), "1.00 GB"); // exactly GB
        // Far end - 100 TB stays in TB unit, doesn't roll over to PB.
        assert_eq!(format_size(100 * 1024_u64.pow(4)), "100.00 TB");
    }
}

/// List models response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListModelsResponse {
    pub models: Vec<ModelInfo>,
}

impl ListModelsResponse {
    pub fn new(models: Vec<ModelInfo>) -> Self {
        Self { models }
    }

    pub fn from_ollama(resp: OllamaListModelsResponse) -> Self {
        Self {
            models: resp.models.into_iter().map(ModelInfo::from_ollama).collect(),
        }
    }
}

/// Load model response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadModelResponse {
    pub model: String,
    pub status: String,
    pub message: String,
}

/// Response for listing loaded models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListLoadedModelsResponse {
    pub models: Vec<LoadedModelInfo>,
}

impl ListLoadedModelsResponse {
    pub fn new(models: Vec<LoadedModelInfo>) -> Self {
        Self { models }
    }
}

/// Layer distribution across devices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerDistribution {
    pub device_type: String,
    pub device_id: usize,
    pub layer_start: u32,
    pub layer_end: u32,
    pub memory_bytes: u64,
}

impl LayerDistribution {
    pub fn layer_count(&self) -> u32 {
        self.layer_end.saturating_sub(self.layer_start) + 1
    }
}

/// Information about a loaded model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedModelInfo {
    pub model: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_layers: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer_distribution: Option<Vec<LayerDistribution>>,
}

// -- /api/inflight (Phase 9 GUI integration) --------------------------------

/// Mirror of server's GateSnapshot (in api/gate.rs). The GUI polls
/// /api/inflight to render running + queued requests in the Hardware tab.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct InflightSnapshot {
    pub in_flight: usize,
    pub queue_depth: usize,
    pub queued_batch: usize,
    pub queued_interactive: usize,
    pub queued_fim: usize,
    #[serde(default)]
    pub requests: Vec<InflightRequest>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct InflightRequest {
    pub req_id: u64,
    pub priority: String,
    pub model: String,
    pub endpoint: String,
    pub queued_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub state: String,
}

// -- /api/distributed/devices ----------------------------------------------

/// Per-device record returned by the server's list_devices handler.
/// Field names mirror the JSON shape emitted in handlers.rs:list_devices.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DeviceRecord {
    pub id: usize,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub memory_gb: f64,
    pub memory_bytes: u64,
    /// Live free-memory in bytes from NVML. `None` for non-CUDA devices
    /// or when NVML lookup failed; the GUI falls back to the static
    /// usable_memory_gb estimate in that case.
    #[serde(default)]
    pub free_bytes: Option<u64>,
    /// Live GPU compute utilization 0-100. `None` outside CUDA / NVML.
    #[serde(default)]
    pub utilization_gpu_percent: Option<f32>,
    /// Live memory-controller utilization 0-100. `None` outside CUDA.
    #[serde(default)]
    pub utilization_memory_percent: Option<f32>,
    /// Live GPU core temperature in deg C. `None` outside CUDA.
    #[serde(default)]
    pub temperature_c: Option<f32>,
    /// Live power draw in watts. `None` outside CUDA.
    #[serde(default)]
    pub power_watts: Option<f32>,
    /// Enforced power limit (TDP) in watts. `None` outside CUDA.
    #[serde(default)]
    pub power_limit_watts: Option<f32>,
    pub priority: u8,
    pub status: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub suggestion: Option<String>,
    pub usable_memory_gb: f64,
}

/// Envelope around `Vec<DeviceRecord>` plus the server-side summary.
/// Matches the JSON shape from handlers.rs:list_devices.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DevicesResponse {
    pub devices: Vec<DeviceRecord>,
    #[serde(default)]
    pub summary: Option<DevicesSummary>,
    /// Cumulative session energy for the Hardware-tab Energy card.
    /// `None` when the server has energy reporting disabled (or is an
    /// older build that doesn't emit the field). Rides the existing
    /// /api/distributed/devices poll - no extra request.
    #[serde(default)]
    pub energy: Option<EnergySnapshotWire>,
}

/// Mirror of the server's `energy_report::EnergySnapshot` - cumulative
/// per-session energy the GUI renders in the Hardware-tab Energy card.
/// `by_modality` values are joules (converted to Wh in the view).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct EnergySnapshotWire {
    #[serde(default)]
    pub total_j: f64,
    #[serde(default)]
    pub total_wh: f64,
    #[serde(default)]
    pub total_gco2: f64,
    #[serde(default)]
    pub total_water_l: f64,
    #[serde(default)]
    pub requests: u64,
    /// modality label ("text"/"image"/"audio"/"video"/"tts") -> joules.
    #[serde(default)]
    pub by_modality: std::collections::HashMap<String, f64>,
    #[serde(default)]
    pub last_line: Option<String>,
    #[serde(default)]
    pub session_equiv: EnergyEquivalentsWire,
}

/// Headline everyday-equivalences mirrored from the server's
/// `EnergyEquivalentsLite`.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct EnergyEquivalentsWire {
    #[serde(default)]
    pub led_bulb_minutes: f64,
    #[serde(default)]
    pub smartphone_charges: f64,
    #[serde(default)]
    pub ev_meters: f64,
    #[serde(default)]
    pub hot_water_cups: f64,
}

/// `summary` block from /api/distributed/devices. Carries compile-time
/// feature flags so the GUI's "Features" chips (CUDA / SYCL) reflect
/// what the running binary actually supports instead of always showing
/// the default false.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DevicesSummary {
    #[serde(default)]
    pub compiled_features: CompiledFeaturesWire,
}

/// What the server was built with, as it names them.
///
/// An open map rather than a field per flag: this was two named booleans, `cuda` and `sycl`,
/// against a server emitting `cuda`, `opencl`, `image`, `video`, `audio`, `midi` and `energy`.
/// `sycl` matched nothing, so the second badge was permanently dark whatever the build, and
/// five flags were never read at all. A hand-copied list of another program's names drifts the
/// moment that program adds one.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct CompiledFeaturesWire(pub std::collections::BTreeMap<String, bool>);

impl CompiledFeaturesWire {
    pub fn enabled(&self, name: &str) -> bool {
        self.0.get(name).copied().unwrap_or(false)
    }
}

// -- /api/layer_perf -------------------------------------------------------

/// Wire record from layer_performance_endpoint (handlers.rs). Note the
/// server emits `total_duration_ms` as f64 - the GUI's local
/// LayerPerformance struct doesn't carry the cumulative duration,
/// so we drop it here too.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct LayerPerfRecord {
    pub layer_idx: usize,
    pub device_type: String,
    pub model_name: String,
    pub token_count: usize,
    pub avg_ms_per_token: f64,
    pub tokens_per_second: f64,
    pub early_exit_count: usize,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct LayerPerfResponse {
    pub layers: Vec<LayerPerfRecord>,
}

#[cfg(test)]
mod device_record_tests {
    use super::*;

    #[test]
    fn device_record_deserializes_full_live_payload() {
        // Pin the wire shape from
        // crates/server/src/api/handlers.rs::list_devices so a future
        // rename of util/temp/power keys server-side fails this test
        // instead of silently hiding chips on the Hardware tab.
        let payload = serde_json::json!({
            "id": 0,
            "type": "CUDA",
            "name": "NVIDIA GeForce RTX 5070 Ti",
            "memory_gb": 15.92,
            "memory_bytes": 17094934528_u64,
            "free_bytes": 1127350272_u64,
            "utilization_gpu_percent": 0.0,
            "utilization_memory_percent": 1.0,
            "temperature_c": 42.0,
            "power_watts": 38.3,
            "power_limit_watts": 300.0,
            "priority": 100,
            "status": "available",
            "reason": serde_json::Value::Null,
            "suggestion": serde_json::Value::Null,
            "usable_memory_gb": 12.74,
        });
        let r: DeviceRecord = serde_json::from_value(payload).expect("deserialize");
        assert_eq!(r.id, 0);
        assert_eq!(r.kind, "CUDA");
        assert_eq!(r.free_bytes, Some(1127350272));
        assert_eq!(r.utilization_gpu_percent, Some(0.0));
        assert_eq!(r.temperature_c, Some(42.0));
        assert_eq!(r.power_watts, Some(38.3));
        assert_eq!(r.power_limit_watts, Some(300.0));
    }

    #[test]
    fn devices_response_summary_compiled_features_roundtrip() {
        // Pin the full envelope shape (devices + summary.compiled_features)
        // so the GUI's CUDA/SYCL chips on the Hardware tab keep working
        // after a server-side schema change. summary is optional so a
        // server that drops the block doesn't break clients.
        let payload = serde_json::json!({
            "devices": [],
            "summary": {
                "compiled_features": { "cuda": true, "opencl": false, "image": true }
            }
        });
        let resp: DevicesResponse = serde_json::from_value(payload).expect("deserialize");
        assert_eq!(resp.devices.len(), 0);
        let s = resp.summary.expect("summary present");
        assert!(s.compiled_features.enabled("cuda"));
        assert!(s.compiled_features.enabled("cuda"));
        assert!(!s.compiled_features.enabled("opencl"));
        // A flag the server adds later is carried, not dropped.
        assert!(s.compiled_features.enabled("image"));
    }

    #[test]
    fn devices_response_carries_session_energy_snapshot() {
        // Pin the /api/distributed/devices `energy` block (server's
        // energy_report::EnergySnapshot) so the Hardware-tab Energy card
        // keeps working after a server-side schema change. energy is
        // serde(default) = Option::None so an older server (or one with
        // reporting disabled -> JSON null) doesn't break the client.
        let payload = serde_json::json!({
            "devices": [],
            "summary": { "compiled_features": { "cuda": true, "opencl": false, "image": true } },
            "energy": {
                "total_j": 3600.0,
                "total_wh": 1.0,
                "total_gco2": 0.05,
                "total_water_l": 0.0018,
                "requests": 3,
                "by_modality": { "text": 3000.0, "image": 600.0 },
                "last_line": "1.00 Wh ...",
                "session_equiv": {
                    "led_bulb_minutes": 8.571,
                    "smartphone_charges": 0.0667,
                    "ev_meters": 5.556,
                    "hot_water_cups": 0.0429
                }
            }
        });
        let resp: DevicesResponse = serde_json::from_value(payload).expect("deserialize");
        let e = resp.energy.expect("energy present");
        assert_eq!(e.requests, 3);
        assert!((e.total_wh - 1.0).abs() < 1e-9);
        assert!((e.total_gco2 - 0.05).abs() < 1e-9);
        assert!((e.total_water_l - 0.0018).abs() < 1e-9);
        assert_eq!(e.by_modality.get("text").copied(), Some(3000.0));
        assert!((e.session_equiv.hot_water_cups - 0.0429).abs() < 1e-6);
        assert_eq!(e.last_line.as_deref(), Some("1.00 Wh ..."));

        // Reporting-disabled / older server: energy absent -> None.
        let no_energy = serde_json::json!({ "devices": [] });
        let resp: DevicesResponse = serde_json::from_value(no_energy).expect("deserialize");
        assert!(resp.energy.is_none());
    }

    #[test]
    fn layer_perf_response_roundtrips_the_server_shape() {
        // Counterpart to the server-side layers_to_json wire-shape
        // pin: confirms the GUI's LayerPerfResponse deserializes
        // exactly the JSON keys the server emits. If either side
        // ever renames (e.g. avg_ms_per_token -> avg_latency_ms),
        // this test catches it before the Hardware-tab per-layer
        // panel silently goes blank.
        let payload = serde_json::json!({
            "layers": [
                {
                    "layer_idx": 7,
                    "device_type": "CUDA",
                    "model_name": "qwen3:latest",
                    "total_duration_ms": 250.0,
                    "token_count": 5,
                    "avg_ms_per_token": 50.0,
                    "tokens_per_second": 20.0,
                    "early_exit_count": 0,
                }
            ]
        });
        let resp: LayerPerfResponse = serde_json::from_value(payload).expect("deserialize");
        assert_eq!(resp.layers.len(), 1);
        let l = &resp.layers[0];
        assert_eq!(l.layer_idx, 7);
        assert_eq!(l.device_type, "CUDA");
        assert_eq!(l.model_name, "qwen3:latest");
        assert_eq!(l.token_count, 5);
        assert!((l.avg_ms_per_token - 50.0).abs() < 1e-6);
        assert!((l.tokens_per_second - 20.0).abs() < 1e-6);
        assert_eq!(l.early_exit_count, 0);
    }

    #[test]
    fn inflight_snapshot_roundtrips_gate_snapshot() {
        // Pin /api/inflight wire shape: server's GateSnapshot in
        // crates/server/src/api/gate.rs:248-256 must deserialize as
        // GUI's InflightSnapshot. Hardware tab's scheduler panel
        // depends on every field - a rename of any counter (in_flight,
        // queue_depth, queued_batch/interactive/fim) or any request
        // field (req_id, priority, model, endpoint, queued_at_ms,
        // started_at_ms, state) would silently blank the panel.
        let payload = serde_json::json!({
            "in_flight": 2,
            "queue_depth": 3,
            "queued_batch": 1,
            "queued_interactive": 2,
            "queued_fim": 0,
            "requests": [
                {
                    "req_id": 42,
                    "priority": "Interactive",
                    "model": "qwen3:latest",
                    "endpoint": "/api/chat",
                    "queued_at_ms": 1_700_000_000_000_i64,
                    "started_at_ms": 1_700_000_000_100_i64,
                    "state": "running"
                }
            ]
        });
        let snap: InflightSnapshot = serde_json::from_value(payload).expect("deserialize");
        assert_eq!(snap.in_flight, 2);
        assert_eq!(snap.queue_depth, 3);
        assert_eq!(snap.queued_batch, 1);
        assert_eq!(snap.queued_interactive, 2);
        assert_eq!(snap.queued_fim, 0);
        assert_eq!(snap.requests.len(), 1);
        let r = &snap.requests[0];
        assert_eq!(r.req_id, 42);
        assert_eq!(r.priority, "Interactive");
        assert_eq!(r.model, "qwen3:latest");
        assert_eq!(r.endpoint, "/api/chat");
        assert_eq!(r.queued_at_ms, 1_700_000_000_000);
        assert_eq!(r.started_at_ms, Some(1_700_000_000_100));
        assert_eq!(r.state, "running");
    }

    #[test]
    fn ollama_model_details_quantization_level_is_optional() {
        // Server's OllamaModelDetails always emits format / family /
        // parameter_size, but quantization_level only for GGUF
        // checkpoints with a recognizable Q-tag. Pin both shapes so
        // the GUI Models tab works for HF safetensors (no quant tag)
        // and Ollama GGUF (with quant tag) alike.
        let with_quant = serde_json::json!({
            "format": "gguf",
            "family": "qwen",
            "parameter_size": "8B",
            "quantization_level": "Q4_K_M"
        });
        let d: OllamaModelDetails = serde_json::from_value(with_quant).expect("with quant");
        assert_eq!(d.format, "gguf");
        assert_eq!(d.family, "qwen");
        assert_eq!(d.parameter_size, "8B");
        assert_eq!(d.quantization_level.as_deref(), Some("Q4_K_M"));

        let without_quant = serde_json::json!({
            "format": "safetensors",
            "family": "llama",
            "parameter_size": "7B"
        });
        let d: OllamaModelDetails = serde_json::from_value(without_quant).expect("without quant");
        assert_eq!(d.format, "safetensors");
        assert!(d.quantization_level.is_none(),
            "quantization_level must be None when wire payload omits it");
    }

    #[test]
    fn layer_distribution_layer_count_handles_edge_cases() {
        // layer_count is end - start + 1 (inclusive range). Three
        // edge cases worth pinning:
        //   - Single-layer segment (start == end): count = 1.
        //   - Multi-layer normal: count = end - start + 1.
        //   - Inverted (start > end): saturating_sub clamps to 0,
        //     +1 yields 1 - not a crash. Documents the safety net
        //     for a future server bug emitting bogus ranges.
        let one_layer = LayerDistribution {
            device_type: "CPU".into(), device_id: 0,
            layer_start: 5, layer_end: 5, memory_bytes: 0,
        };
        assert_eq!(one_layer.layer_count(), 1);

        let normal = LayerDistribution {
            device_type: "CUDA".into(), device_id: 0,
            layer_start: 0, layer_end: 47, memory_bytes: 0,
        };
        assert_eq!(normal.layer_count(), 48);

        let inverted = LayerDistribution {
            device_type: "CUDA".into(), device_id: 0,
            layer_start: 10, layer_end: 3, memory_bytes: 0,
        };
        assert_eq!(inverted.layer_count(), 1,
            "inverted range must saturate, not underflow-panic");
    }

    #[test]
    fn ollama_generate_request_with_images_for_vision_models() {
        // OllamaGenerateRequest carries an optional `images` field for
        // vision models that don't use chat semantics (e.g. moondream
        // via /api/generate). Pin that:
        //   - The field name on the wire is \"images\" (not e.g.
        //     \"image\" / \"image_data\") so the server's vision
        //     dispatch in ollama_generate finds it.
        //   - When None it must be dropped from the payload
        //     (skip_serializing_if).
        let mut req = OllamaGenerateRequest::new(
            "moondream:1.8b".to_string(),
            "What is in this image?".to_string(),
        );
        let json = serde_json::to_value(&req).expect("serialize w/o images");
        assert!(json.get("images").is_none(),
            "images field leaked when None");

        req.images = Some(vec!["iVBORw0KGgo".to_string()]);
        let json = serde_json::to_value(&req).expect("serialize w/ images");
        assert_eq!(json["images"][0], "iVBORw0KGgo");
        assert_eq!(json["prompt"], "What is in this image?");
    }

    #[test]
    fn tool_call_function_arguments_is_an_opaque_string() {
        // The function.arguments field carries a JSON-encoded STRING
        // (per OpenAI's tool-call convention), not a nested object.
        // The GUI must not try to parse it server-side - that's the
        // tool caller's job. Pin both the present and absent cases.
        let with_args = serde_json::json!({
            "name": "get_weather",
            "arguments": "{\"city\":\"Paris\",\"unit\":\"celsius\"}"
        });
        let f: ToolCallFunction = serde_json::from_value(with_args).expect("with args");
        assert_eq!(f.name, "get_weather");
        let raw = f.arguments.expect("arguments present");
        // raw is the JSON-encoded string - confirm it's still a
        // parseable JSON object (caller's responsibility to parse).
        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .expect("arguments parses as JSON");
        assert_eq!(parsed["city"], "Paris");
        assert_eq!(parsed["unit"], "celsius");

        // No-args case (some tools take zero parameters): arguments
        // is None on the wire, must hydrate as Option::None.
        let no_args = serde_json::json!({ "name": "ping" });
        let f: ToolCallFunction = serde_json::from_value(no_args).expect("no args");
        assert_eq!(f.name, "ping");
        assert!(f.arguments.is_none(),
            "arguments must be None when the wire payload omits it");
    }

    #[test]
    fn tool_wrapper_serializes_with_type_field_and_optional_function() {
        // Tool is the request-side wrapper for ToolFunction: it has
        // a `type` raw-identifier field (Rust keyword escape) and
        // an optional `function`. Pin both that:
        //   - The keyword-escaped `r#type` serializes as wire key
        //     "type" (parallel to the ToolCall.type test, but on the
        //     request side this time).
        //   - function: None is dropped from the payload via
        //     skip_serializing_if so the wire stays clean.
        let with_fn = Tool {
            r#type: "function".to_string(),
            function: Some(ToolFunction {
                name: "lookup".to_string(),
                description: None,
                parameters: None,
            }),
        };
        let json = serde_json::to_value(&with_fn).expect("serialize with fn");
        assert_eq!(json["type"], "function");
        assert_eq!(json["function"]["name"], "lookup");
        assert!(json.get("r#type").is_none(), "r# prefix must not leak");

        // function: None case - should be dropped entirely, leaving
        // just the type field on the wire. Some servers handle the
        // 'name your tool category but no signature' use case.
        let bare = Tool {
            r#type: "custom".to_string(),
            function: None,
        };
        let json = serde_json::to_value(&bare).expect("serialize bare");
        assert_eq!(json["type"], "custom");
        assert!(json.get("function").is_none(), "None function leaked");
    }

    #[test]
    fn tool_function_serializes_with_optional_description_and_parameters() {
        // ToolFunction is the request-side tool definition the GUI
        // sends in OllamaChatRequest.tools[].function. Pin both the
        // minimal shape (name only) and the populated shape
        // (description + JSON schema parameters) so a future server
        // adoption of OpenAI's strict tool schema (which requires
        // 'description' and 'parameters') doesn't catch the GUI
        // emitting null values that the server-side validator rejects.
        let minimal = ToolFunction {
            name: "noop".to_string(),
            description: None,
            parameters: None,
        };
        let json = serde_json::to_value(&minimal).expect("serialize minimal");
        assert_eq!(json["name"], "noop");
        // Optional fields dropped (skip_serializing_if) so the wire
        // payload doesn't carry \"description\":null which the
        // OpenAI strict mode rejects.
        assert!(json.get("description").is_none(), "description leaked when None");
        assert!(json.get("parameters").is_none(), "parameters leaked when None");

        let full = ToolFunction {
            name: "get_weather".to_string(),
            description: Some("Get the current weather for a city.".to_string()),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "city": { "type": "string" }
                },
                "required": ["city"]
            })),
        };
        let json = serde_json::to_value(&full).expect("serialize full");
        assert_eq!(json["name"], "get_weather");
        assert_eq!(json["description"], "Get the current weather for a city.");
        // JSON Schema flows through unchanged.
        assert_eq!(json["parameters"]["properties"]["city"]["type"], "string");
        assert_eq!(json["parameters"]["required"][0], "city");
    }

    #[test]
    fn message_constructors_produce_compact_wire_payload() {
        // Message::new and Message::with_images are the two factory
        // paths the chat tab uses. Pin both so the wire payload stays
        // compact (no null fields) and the right Vecs land.
        let plain = Message::new("user".to_string(), "hi".to_string());
        let json = serde_json::to_value(&plain).expect("serialize plain");
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "hi");
        // Both image and audio Vecs absent (skip_serializing_if).
        assert!(json.get("images").is_none(), "images leaked when None");
        assert!(json.get("audios").is_none(), "audios leaked when None");

        let with_imgs = Message::with_images(
            "user".to_string(),
            "describe".to_string(),
            vec!["b64-blob".to_string()],
        );
        let json = serde_json::to_value(&with_imgs).expect("serialize w/ images");
        assert_eq!(json["images"][0], "b64-blob");
        // audios still absent
        assert!(json.get("audios").is_none(), "audios should still be skipped");
    }

    #[test]
    fn ollama_generate_request_deserializes_minimal_payload() {
        // Reverse direction: a stock client posting just
        // {model, prompt} must hydrate cleanly - stream defaults to
        // false, every Option stays None. Pins the serde defaults
        // so a future schema tightening that removes #[serde(default)]
        // breaks the test instead of silently 400-ing minimal
        // Ollama-style requests.
        let payload = serde_json::json!({
            "model": "qwen3:latest",
            "prompt": "Hello"
        });
        let r: OllamaGenerateRequest =
            serde_json::from_value(payload).expect("deserialize minimal");
        assert_eq!(r.model, "qwen3:latest");
        assert_eq!(r.prompt, "Hello");
        assert!(!r.stream, "stream must default to false");
        assert!(r.format.is_none());
        assert!(r.options.is_none());
        assert!(r.keep_alive.is_none());
        assert!(r.thinking.is_none());
        assert!(r.images.is_none());
    }

    #[test]
    fn ollama_generate_request_load_and_unload_shapes() {
        // The GUI's load_model / unload_model both go through
        // /api/generate with magic keep_alive values:
        //   - load: keep_alive="5m" + empty prompt
        //   - unload: keep_alive="0" + empty prompt
        // The server's ollama_generate handler routes by this string
        // when prompt is empty (handle_model_unload). Pin the wire
        // shape so a future rename of keep_alive breaks the load/
        // unload flow visibly.
        let mut load = OllamaGenerateRequest::new(
            "qwen3:latest".to_string(),
            String::new(),
        );
        load.keep_alive = Some("5m".to_string());
        let json = serde_json::to_value(&load).expect("serialize load");
        assert_eq!(json["model"], "qwen3:latest");
        assert_eq!(json["prompt"], "");
        assert_eq!(json["keep_alive"], "5m");
        assert_eq!(json["stream"], false);

        let mut unload = OllamaGenerateRequest::new(
            "qwen3:latest".to_string(),
            String::new(),
        );
        unload.keep_alive = Some("0".to_string());
        let json = serde_json::to_value(&unload).expect("serialize unload");
        assert_eq!(json["keep_alive"], "0");
    }

    #[test]
    fn ollama_pull_request_deserializes_with_default_source() {
        // Reverse direction: a server (or test fixture) that omits
        // the `source` field must hydrate as 'ollama' via
        // default_model_source. Pins backward compat with any
        // Ollama-shape client that doesn't know about the source
        // extension field.
        let payload = serde_json::json!({ "name": "qwen3:latest" });
        let r: OllamaPullRequest =
            serde_json::from_value(payload).expect("deserialize default-source");
        assert_eq!(r.name, "qwen3:latest");
        assert!(!r.stream, "stream must default to false when absent");
        assert!(r.insecure.is_none());
        assert_eq!(r.source, "ollama",
            "source must default to 'ollama' when absent - pinned by default_model_source");
    }

    #[test]
    fn ollama_pull_request_serializes_minimal_and_hf_shapes() {
        // OllamaPullRequest::new defaults source to "ollama". The
        // hf-source path (used by Models tab's HF pull) overrides
        // source to "huggingface". Pin both shapes since the server's
        // pull handler routes by source string.
        let req = OllamaPullRequest::new("qwen3:latest".to_string());
        let json = serde_json::to_value(&req).expect("serialize default");
        assert_eq!(json["name"], "qwen3:latest");
        assert_eq!(json["stream"], false);
        assert_eq!(json["source"], "ollama");
        // insecure must be skipped when None - not emitted as
        // {\"insecure\":null} which some Ollama clients reject.
        assert!(json.get("insecure").is_none(),
            "insecure leaked into wire payload");

        let mut hf_req = OllamaPullRequest::new("Qwen/Qwen3-7B".to_string());
        hf_req.source = "huggingface".to_string();
        let json = serde_json::to_value(&hf_req).expect("serialize hf");
        assert_eq!(json["source"], "huggingface");
        assert_eq!(json["name"], "Qwen/Qwen3-7B");
    }

    #[test]
    fn ollama_chat_request_deserializes_minimal_payload() {
        // Stock Ollama clients post {model, messages} only.
        // Pin that all the serde defaults hydrate cleanly:
        //   - stream = false
        //   - format / options / keep_alive / thinking / tools all None
        // A future schema tightening would silently 400 minimal
        // requests without this guard.
        let payload = serde_json::json!({
            "model": "qwen3:latest",
            "messages": [
                { "role": "user", "content": "Hello" }
            ]
        });
        let r: OllamaChatRequest =
            serde_json::from_value(payload).expect("deserialize minimal chat");
        assert_eq!(r.model, "qwen3:latest");
        assert_eq!(r.messages.len(), 1);
        assert_eq!(r.messages[0].content, "Hello");
        assert!(!r.stream, "stream defaults to false");
        assert!(r.format.is_none());
        assert!(r.options.is_none());
        assert!(r.keep_alive.is_none());
        assert!(r.thinking.is_none());
        assert!(r.tools.is_none());
        // Message defaults: images and audios both None.
        assert!(r.messages[0].images.is_none());
        assert!(r.messages[0].audios.is_none());
    }

    #[test]
    fn ollama_chat_request_serializes_minimal_shape() {
        // Reverse direction: the GUI's outbound request must serialize
        // with the JSON keys the server expects. OllamaChatRequest::new
        // sets all optional fields to None - skip_serializing_if must
        // drop them so the wire payload stays compact.
        let req = OllamaChatRequest::new(
            "qwen3:latest".to_string(),
            vec![Message {
                role: "user".to_string(),
                content: "hi".to_string(),
                images: None,
                audios: None,
            }],
        );
        let json = serde_json::to_value(&req).expect("serialize");
        // Required fields present
        assert_eq!(json["model"], "qwen3:latest");
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "hi");
        assert_eq!(json["stream"], false);
        // Optional fields absent (skip_serializing_if on each Option)
        assert!(json.get("format").is_none(),       "format leaked");
        assert!(json.get("options").is_none(),      "options leaked");
        assert!(json.get("keep_alive").is_none(),   "keep_alive leaked");
        assert!(json.get("thinking").is_none(),     "thinking leaked");
        assert!(json.get("tools").is_none(),        "tools leaked");
        // Message optional Vecs also skipped
        assert!(json["messages"][0].get("images").is_none(), "images leaked");
        assert!(json["messages"][0].get("audios").is_none(), "audios leaked");
    }

    #[test]
    fn ollama_chat_request_streaming_with_tools_serializes() {
        // Populated tools + keep_alive + stream:true should all appear
        // on the wire. Pins the field names that the server's
        // ollama_chat handler expects.
        let mut req = OllamaChatRequest::new(
            "qwen3:latest".to_string(),
            vec![Message::new("user".to_string(), "hi".to_string())],
        );
        req.stream = true;
        req.keep_alive = Some("5m".to_string());
        req.tools = Some(vec![Tool {
            r#type: "function".to_string(),
            function: Some(ToolFunction {
                name: "get_weather".to_string(),
                description: None,
                parameters: None,
            }),
        }]);

        let json = serde_json::to_value(&req).expect("serialize");
        assert_eq!(json["stream"], true);
        assert_eq!(json["keep_alive"], "5m");
        assert_eq!(json["tools"][0]["type"], "function");
        assert_eq!(json["tools"][0]["function"]["name"], "get_weather");
    }

    #[test]
    fn model_info_from_ollama_preserves_fields_and_formats_size() {
        // ListModelsResponse::from_ollama converts the wire OllamaModel
        // into the GUI's unified ModelInfo. Pin the conversion so:
        //   - name, size_bytes, modified_at, source flow through unchanged
        //   - size becomes a human-friendly string via format_size
        // A future change that drops a field or stops formatting size
        // breaks the Models tab's display.
        let wire = OllamaModel {
            name: "qwen3:latest".to_string(),
            modified_at: "2026-05-17T01:00:00Z".to_string(),
            size: 4_500_000_000,  // ~4.19 GB after KB/MB/GB rounding
            digest: "sha256:abc".to_string(),
            details: None,
            source: "ollama".to_string(),
            capabilities: Vec::new(),
            defaults: None,
        };
        let info = ModelInfo::from_ollama(wire);
        assert_eq!(info.name, "qwen3:latest");
        assert_eq!(info.size_bytes, 4_500_000_000);
        assert_eq!(info.modified_at, "2026-05-17T01:00:00Z");
        assert_eq!(info.source, "ollama");
        // format_size picks GB for 4.5e9 -> "4.19 GB" (2 decimals for GB).
        assert!(info.size.ends_with(" GB"), "size = {}", info.size);
    }

    #[test]
    fn tool_call_type_field_is_keyword_escaped() {
        // ToolCall.type is a Rust keyword so the struct uses r#type.
        // Serde drops the r# prefix when (de)serializing, so the JSON
        // key MUST be \"type\" - pin this so a future rename of the
        // Rust field to e.g. `kind` doesn't silently break wire
        // compatibility with Ollama clients that emit \"type\".
        let payload = serde_json::json!({
            "id": "call_1",
            "type": "function",
            "function": { "name": "noop" }
        });
        let tc: ToolCall = serde_json::from_value(payload).expect("deserialize");
        assert_eq!(tc.r#type, "function");
        // Reverse direction: serialize must emit \"type\" too.
        let back = serde_json::to_value(&tc).expect("serialize");
        assert_eq!(back.get("type").and_then(|v| v.as_str()), Some("function"));
        assert!(back.get("r#type").is_none(),
            "r# prefix must not leak into the JSON key");
    }

    #[test]
    fn ollama_chat_response_tool_calls_roundtrip() {
        // /api/chat tool-calling response - final chunk carries
        // `tool_calls` array with id + type + function{name, arguments}.
        // The GUI's chat parser surfaces these to the user as a
        // tool-invocation row. Pin the nested shape so a future
        // server-side rename of any field (arguments -> args, etc.)
        // fails the test instead of silently dropping the row.
        let chunk = serde_json::json!({
            "model": "qwen3:latest",
            "created_at": "2026-05-17T01:00:00Z",
            "message": {
                "role": "assistant",
                "content": ""
            },
            "done": true,
            "tool_calls": [
                {
                    "id": "call_42",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"city\":\"Paris\"}"
                    }
                }
            ]
        });
        let r: OllamaChatResponse =
            serde_json::from_value(chunk).expect("tool_calls chunk deserialize");
        let calls = r.tool_calls.expect("tool_calls present");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_42");
        assert_eq!(calls[0].r#type, "function");
        let f = calls[0].function.as_ref().expect("function present");
        assert_eq!(f.name, "get_weather");
        assert_eq!(f.arguments.as_deref(), Some("{\"city\":\"Paris\"}"));
    }

    #[test]
    fn ollama_chat_response_thinking_chunk_roundtrip() {
        // Reasoning-model response - qwen3, deepseek-r1, etc. emit a
        // `thinking` field with the chain-of-thought before/alongside
        // the main answer. The GUI surfaces this in a collapsible
        // section. Pin so a future rename to e.g. `reasoning` fails
        // the test.
        let chunk = serde_json::json!({
            "model": "deepseek-r1:32b",
            "created_at": "2026-05-17T01:00:00Z",
            "message": {
                "role": "assistant",
                "content": "The answer is 42."
            },
            "done": true,
            "thinking": "Let me reason about this step by step...",
            "thinking_duration": 2_500_000_000_u64
        });
        let r: OllamaChatResponse =
            serde_json::from_value(chunk).expect("thinking chunk deserialize");
        assert_eq!(r.thinking.as_deref(), Some("Let me reason about this step by step..."));
        assert_eq!(r.thinking_duration, Some(2_500_000_000));
    }

    #[test]
    fn ollama_chat_response_streaming_chunk_with_message() {
        // /api/chat mid-stream chunk: just message.role + message.content,
        // done:false, no timing yet. The GUI's chat-streaming parser
        // reads message.content per chunk to grow the displayed text.
        let chunk = serde_json::json!({
            "model": "qwen3:latest",
            "created_at": "2026-05-17T01:00:00Z",
            "message": {
                "role": "assistant",
                "content": " hello"
            },
            "done": false
        });
        let r: OllamaChatResponse =
            serde_json::from_value(chunk).expect("chat mid-stream deserialize");
        assert_eq!(r.message.role, "assistant");
        assert_eq!(r.message.content, " hello");
        assert!(!r.done);
        // Message.images / .audios both Option<None> when absent.
        assert!(r.message.images.is_none());
        assert!(r.message.audios.is_none());
    }

    #[test]
    fn ollama_chat_response_tts_chunk_carries_audios() {
        // /api/chat for a TTS model (parler/etc.) emits the assistant
        // message with audios: [<base64 wav>] populated. The GUI's
        // streaming parser must capture these so the chat tab can
        // render the inline Play / Save buttons.
        let chunk = serde_json::json!({
            "model": "parler-tts-mini-v1",
            "created_at": "2026-05-17T01:00:00Z",
            "message": {
                "role": "assistant",
                "content": "Synthesized.",
                "audios": ["UklGRiQ..."]
            },
            "done": true
        });
        let r: OllamaChatResponse =
            serde_json::from_value(chunk).expect("tts chunk deserialize");
        assert!(r.done);
        let audios = r.message.audios.expect("audios present");
        assert_eq!(audios.len(), 1);
        assert!(audios[0].starts_with("UklGR"));
    }

    #[test]
    fn ollama_chat_response_vision_chunk_carries_images() {
        // Inbound vision message (mostly used by the request side) -
        // the response carries images back for round-trip echo in
        // some flows. Confirm Message.images deserializes.
        let chunk = serde_json::json!({
            "model": "llava:7b",
            "created_at": "2026-05-17T01:00:00Z",
            "message": {
                "role": "user",
                "content": "What is this?",
                "images": ["iVBORw0KGgo..."]
            },
            "done": false
        });
        let r: OllamaChatResponse =
            serde_json::from_value(chunk).expect("vision chunk deserialize");
        let imgs = r.message.images.expect("images present");
        assert_eq!(imgs.len(), 1);
    }

    #[test]
    fn ollama_pull_response_progress_stream_shapes() {
        // /api/pull streams progress chunks. Three canonical shapes
        // the GUI's download progress bar consumes:
        //   1. \`status\` only - initial 'pulling manifest' notice.
        //   2. status + digest + total + completed - mid-download
        //      progress (chunked bytes-downloaded counter).
        //   3. status: 'success' - final terminator.
        let initial = serde_json::json!({ "status": "pulling manifest" });
        let r: OllamaPullResponse = serde_json::from_value(initial).expect("initial");
        assert_eq!(r.status, "pulling manifest");
        assert!(r.digest.is_none());
        assert!(r.total.is_none());

        let progress = serde_json::json!({
            "status": "downloading abc123",
            "digest": "sha256:abc123",
            "total": 4_000_000_000_u64,
            "completed": 1_200_000_000_u64
        });
        let r: OllamaPullResponse = serde_json::from_value(progress).expect("progress");
        assert_eq!(r.digest.as_deref(), Some("sha256:abc123"));
        assert_eq!(r.total, Some(4_000_000_000));
        assert_eq!(r.completed, Some(1_200_000_000));

        let done = serde_json::json!({ "status": "success" });
        let r: OllamaPullResponse = serde_json::from_value(done).expect("done");
        assert_eq!(r.status, "success");
    }

    #[test]
    fn ollama_generate_response_minimal_streaming_chunk() {
        // /api/generate streams chunks with just `response` text +
        // `done: false`. All timing/metadata fields are absent in
        // mid-stream chunks. Pin both shapes so the GUI's
        // chat-streaming parser stays robust to either.
        let chunk = serde_json::json!({
            "model": "qwen3:latest",
            "created_at": "2026-05-17T01:00:00Z",
            "response": " token",
            "done": false
        });
        let r: OllamaGenerateResponse =
            serde_json::from_value(chunk).expect("mid-stream chunk deserialize");
        assert_eq!(r.response, " token");
        assert!(!r.done);
        assert!(r.total_duration.is_none());
        assert!(r.eval_count.is_none());
        assert!(r.context.is_none());
    }

    #[test]
    fn ollama_generate_response_final_chunk_with_timing() {
        // Final chunk with done=true carries the timing/metadata fields
        // the GUI's MessageTiming row depends on.
        let chunk = serde_json::json!({
            "model": "qwen3:latest",
            "created_at": "2026-05-17T01:00:00Z",
            "response": "",
            "done": true,
            "done_reason": "stop",
            "total_duration": 1_500_000_000_u64,
            "load_duration": 5_000_000_u64,
            "prompt_eval_count": 50_u64,
            "prompt_eval_duration": 100_000_000_u64,
            "eval_count": 30_u64,
            "eval_duration": 1_000_000_000_u64
        });
        let r: OllamaGenerateResponse =
            serde_json::from_value(chunk).expect("final chunk deserialize");
        assert!(r.done);
        assert_eq!(r.done_reason.as_deref(), Some("stop"));
        assert_eq!(r.total_duration, Some(1_500_000_000));
        assert_eq!(r.eval_count, Some(30));
        assert_eq!(r.eval_duration, Some(1_000_000_000));
    }

    #[test]
    fn ollama_list_models_roundtrips_with_optional_details() {
        // /api/tags is fetched on Models-tab refresh. Server now
        // populates OllamaModelDetails (format/family/parameter_size)
        // to spare the GUI an /api/show per row. Confirm both shapes
        // deserialize: with full details and without.
        let payload = serde_json::json!({
            "models": [
                {
                    "name": "qwen3:latest",
                    "modified_at": "2026-05-17T01:00:00Z",
                    "size": 4_000_000_000_u64,
                    "digest": "abc123",
                    "details": {
                        "format": "gguf",
                        "family": "qwen",
                        "parameter_size": "8B",
                        "quantization_level": "Q4_K_M"
                    },
                    "source": "ollama"
                },
                {
                    // Minimal: pre-details servers / non-Ollama sources
                    "name": "openai/whisper-small",
                    "modified_at": "2026-05-17T01:00:00Z",
                    "size": 244_000_000_u64
                }
            ]
        });
        let resp: OllamaListModelsResponse =
            serde_json::from_value(payload).expect("deserialize");
        assert_eq!(resp.models.len(), 2);

        let first = &resp.models[0];
        assert_eq!(first.name, "qwen3:latest");
        assert_eq!(first.digest, "abc123");
        assert_eq!(first.source, "ollama");
        let det = first.details.as_ref().expect("details present");
        assert_eq!(det.format, "gguf");
        assert_eq!(det.family, "qwen");
        assert_eq!(det.parameter_size, "8B");
        assert_eq!(det.quantization_level.as_deref(), Some("Q4_K_M"));

        // Minimal entry: digest defaults to "", details None, source
        // defaults to "ollama" via default_model_source_str.
        let second = &resp.models[1];
        assert_eq!(second.name, "openai/whisper-small");
        assert_eq!(second.digest, "");
        assert!(second.details.is_none());
        assert_eq!(second.source, "ollama");
    }

    #[test]
    fn list_loaded_models_response_full_payload_roundtrips() {
        // /api/models/loaded is also on the Hardware tab's 2 s
        // auto-refresh loop. The server emits LoadedModelInfo with
        // optional fields skipped when None - confirm both shapes
        // deserialize: a model with full topology and a minimal
        // entry (just model + status).
        let payload = serde_json::json!({
            "models": [
                {
                    "model": "qwen3-coder:latest",
                    "status": "loaded",
                    "device": "CUDA",
                    "size_bytes": 19_000_000_000_u64,
                    "num_layers": 48,
                    "layer_distribution": [
                        {
                            "device_type": "CUDA",
                            "device_id": 0,
                            "layer_start": 0,
                            "layer_end": 31,
                            "memory_bytes": 14_000_000_000_u64
                        },
                        {
                            "device_type": "CPU",
                            "device_id": 0,
                            "layer_start": 32,
                            "layer_end": 47,
                            "memory_bytes": 5_000_000_000_u64
                        }
                    ]
                },
                {
                    "model": "openai/whisper-small",
                    "status": "loaded"
                }
            ]
        });
        let resp: ListLoadedModelsResponse =
            serde_json::from_value(payload).expect("deserialize");
        assert_eq!(resp.models.len(), 2);
        let first = &resp.models[0];
        assert_eq!(first.model, "qwen3-coder:latest");
        assert_eq!(first.device.as_deref(), Some("CUDA"));
        assert_eq!(first.num_layers, Some(48));
        let dist = first.layer_distribution.as_ref().expect("topology present");
        assert_eq!(dist.len(), 2);
        assert_eq!(dist[0].layer_count(), 32); // 0..=31 = 32 layers
        assert_eq!(dist[1].layer_count(), 16); // 32..=47 = 16 layers
        // Minimal entry: all topology fields absent -> all Option::None.
        let second = &resp.models[1];
        assert_eq!(second.model, "openai/whisper-small");
        assert!(second.device.is_none());
        assert!(second.num_layers.is_none());
        assert!(second.layer_distribution.is_none());
    }

    #[test]
    fn inflight_snapshot_request_started_at_can_be_null() {
        // A queued-but-not-yet-running request has started_at_ms = null
        // - Option<i64> on the GUI side must accept that.
        let payload = serde_json::json!({
            "in_flight": 0,
            "queue_depth": 1,
            "queued_batch": 0,
            "queued_interactive": 1,
            "queued_fim": 0,
            "requests": [
                {
                    "req_id": 99,
                    "priority": "Batch",
                    "model": "deepcoder:14b",
                    "endpoint": "/api/generate",
                    "queued_at_ms": 1_700_000_000_500_i64,
                    "started_at_ms": serde_json::Value::Null,
                    "state": "queued"
                }
            ]
        });
        let snap: InflightSnapshot = serde_json::from_value(payload).expect("deserialize");
        assert_eq!(snap.requests[0].started_at_ms, None);
        assert_eq!(snap.requests[0].state, "queued");
    }

    #[test]
    fn layer_perf_response_empty_layers_array() {
        // Cold-start case: no model has run yet, server emits
        // `{"layers":[]}`. The GUI's per-layer panel handles the
        // empty Vec by rendering 'No performance data yet.'
        let payload = serde_json::json!({ "layers": [] });
        let resp: LayerPerfResponse = serde_json::from_value(payload).expect("deserialize");
        assert!(resp.layers.is_empty());
    }

    #[test]
    fn devices_response_tolerates_missing_summary() {
        // Older servers (or future servers that decide to drop the
        // summary block) should still deserialize - summary is
        // serde(default) = Option::None.
        let payload = serde_json::json!({ "devices": [] });
        let resp: DevicesResponse = serde_json::from_value(payload).expect("deserialize");
        assert!(resp.summary.is_none());
    }

    #[test]
    fn device_record_deserializes_cpu_with_missing_live_fields() {
        // CPU (non-CUDA) devices omit every live field. The GUI must
        // not 400 these out - #[serde(default)] makes each None and
        // the device card just skips the chips row.
        let payload = serde_json::json!({
            "id": 2,
            "type": "CPU",
            "name": "CPU (20 cores)",
            "memory_gb": 62.7,
            "memory_bytes": 67_000_000_000_u64,
            "priority": 20,
            "status": "available",
            "usable_memory_gb": 50.2,
        });
        let r: DeviceRecord = serde_json::from_value(payload).expect("deserialize");
        assert_eq!(r.kind, "CPU");
        assert!(r.free_bytes.is_none());
        assert!(r.utilization_gpu_percent.is_none());
        assert!(r.temperature_c.is_none());
        assert!(r.power_watts.is_none());
        assert!(r.reason.is_none());
        assert!(r.suggestion.is_none());
    }
}

/// The optional render controls an image request can carry beyond size and steps.
///
/// Grouped rather than passed as three more positional arguments: they travel together,
/// they are all "leave empty for the model's own default", and the call site already
/// takes nine parameters.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderControls {
    /// `(adapter name, strength)`. Names come from the server's `/v1/loras` listing;
    /// the client never sends a filesystem path.
    pub loras: Vec<(String, f32)>,
    /// Regional prompts, already reduced to the rectangles the API takes.
    pub regions: Vec<crate::state::ImageRegion>,
    /// A pose or edge image and how hard it pulls. `None` = an ordinary render.
    pub control: Option<(Vec<u8>, f32)>,
    /// Solver ("euler", "dpmpp_2m"). Empty = the model's default.
    pub sampler: String,
    /// Sigma curve ("normal", "karras", "exponential"). Empty = the model's default.
    pub scheduler: String,
    /// Encoding of the returned image: "png", "jpeg" or "webp". Empty = the server's
    /// default, which is png. The endpoint has always taken this and transcoded for it;
    /// the client had no way to ask, so every render came back as a PNG.
    pub output_format: String,
    /// What the guidance steers AWAY from. Empty = the model family's own default, which
    /// is not the same as an empty conditioning - the families are tuned against theirs.
    pub negative_prompt: String,
}

impl RenderControls {
    /// True when there is nothing to send - the common case, and the one where the
    /// request body should stay exactly as it was before these existed.
    pub fn is_empty(&self) -> bool {
        self.loras.is_empty() && self.sampler.is_empty() && self.scheduler.is_empty()
    }
}
