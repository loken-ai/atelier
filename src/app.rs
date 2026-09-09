//! Main GUI Application
//!
//! Main application struct and eframe::App implementation.

use std::sync::{Arc, Mutex};

use clap::Parser;
use eframe::egui;
use egui::Vec2;
use tokio::runtime::Runtime;
use tracing::{error, info, warn};

use crate::api::{Client, LoadModelResponse, Message, OllamaChatRequest, OllamaChatResponse};

use crate::config::{ApiType, AppConfig};
use crate::log_buffer::LogBuffer;
use crate::settings::{SettingsAction, SettingsState};
use crate::state::{
    ActionStatus, CLIOutput, CLIState, ChatMessage, ChatState, ConnectionStatus, MediaKind,
    ModelState, Section, ServerState, ServerStatus,
};
use crate::task::{MediaOutput, TaskResult};
use crate::theme;
use crate::toast::{Toast, ToastSeverity};

// ============================================================================
// Media Studio helpers
// ============================================================================

/// A time-derived pseudo-random seed for media kinds whose server
/// endpoint defaults to a fixed seed (0) when none is given. Lets an
/// empty seed field mean "different each run" without a rand crate.
fn time_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Route a `/v1/audio` or `/v1/video` `data[]` payload into a
/// `MediaOutput`, dispatching each item by its `content_type`:
/// images render inline, WAV audio plays, and MIDI / video become
/// downloadable (filename, bytes) blobs.
fn media_data_to_output(data: Vec<crate::api::types::MediaDatum>, status: String) -> MediaOutput {
    use base64::Engine;
    let decode = |b64: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap_or_default()
    };
    let mut out = MediaOutput {
        status,
        ..Default::default()
    };
    // ONE stamp for the whole render, so its files sort together and two renders
    // cannot collide. Without it every clip lands as `media_1.mp4` and each render
    // silently overwrites the last one saved.
    let stamp = crate::modality::output_stamp();
    for (i, d) in data.into_iter().enumerate() {
        let ct = d.content_type.as_deref().unwrap_or("").to_lowercase();
        if ct.contains("midi") {
            out.files.push((
                crate::modality::output_filename("midi", &stamp, i, "mid"),
                decode(&d.b64_json),
            ));
        } else if ct.contains("gif") {
            out.files.push((
                crate::modality::output_filename("video", &stamp, i, "gif"),
                decode(&d.b64_json),
            ));
        } else if ct.contains("mp4") || ct.contains("video") {
            out.files.push((
                crate::modality::output_filename("video", &stamp, i, "mp4"),
                decode(&d.b64_json),
            ));
        } else if ct.contains("png") || ct.contains("image") {
            out.images.push(d.b64_json);
        } else {
            // Default: treat as WAV audio (music / SFX).
            out.audios.push(d.b64_json);
        }
    }
    out
}

/// Spawn a one-shot (non-streaming) media request on the runtime and
/// Accept a rewritten prompt only if it IS one. Small models fail this task in
/// recognisable ways - echoing the instruction back, answering the user, or
/// rambling into a multi-paragraph essay - and silently pasting that over the
/// user's prompt is worse than reporting that the model could not do it.
fn validate_enhanced_prompt(text: &str, original: &str) -> Result<String, String> {
    let t = text.trim().trim_matches('"').trim();
    if t.is_empty() {
        return Err("the model returned no usable text".to_string());
    }
    let low = t.to_lowercase();
    // Echoed instruction: the reply repeats the task framing instead of doing it.
    for marker in [
        "rewrite the user",
        "improved generation prompt",
        "do not write lists",
        "output the rewritten",
    ] {
        if low.contains(marker) {
            return Err(
                "the model echoed the instruction instead of rewriting the prompt".to_string(),
            );
        }
    }
    // Addressed the user / refused instead of producing a prompt.
    for marker in [
        "i'm sorry",
        "i am sorry",
        "as an ai",
        "how can i help",
        "what is your question",
    ] {
        if low.starts_with(marker) || low.contains(marker) {
            return Err(
                "the model answered conversationally instead of rewriting the prompt".to_string(),
            );
        }
    }
    let words = t.split_whitespace().count();
    if words > 140 || t.lines().filter(|l| !l.trim().is_empty()).count() > 4 {
        return Err("the model produced an essay rather than a single prompt".to_string());
    }
    // A rewrite must ADD substance. An unchanged echo is a no-op, and a reply
    // shorter than the ask ("a cat lounging on a rooftop" for a 5-word input)
    // is not an enhanced prompt - it cannot carry subject, composition,
    // lighting and style. Reject both so the loop tries a more capable model.
    if t.eq_ignore_ascii_case(original.trim()) {
        return Err("the model returned the prompt unchanged".to_string());
    }
    let original_words = original.split_whitespace().count();
    if words < 12 || words <= original_words {
        return Err("the model returned a prompt with no added detail".to_string());
    }
    Ok(t.to_string())
}

/// funnel its `Result` into the shared task queue as a `MediaResult`,
/// waking the UI. Sibling of `stream_media` for the kinds without SSE
/// progress — keeps each `send_media` arm down to building its future.
fn spawn_media_result(
    rt: &Runtime,
    tasks: Arc<Mutex<Vec<TaskResult>>>,
    egui_ctx: egui::Context,
    fut: impl std::future::Future<Output = Result<MediaOutput, String>> + Send + 'static,
) -> tokio::task::AbortHandle {
    rt.spawn(async move {
        let result = fut.await;
        tasks.lock().unwrap().push(TaskResult::MediaResult(result));
        egui_ctx.request_repaint();
    })
    .abort_handle()
}

/// " - 34.2s, 412 J" consumption suffix for a completion status line.
/// Either figure may be absent; an empty string when both are.
fn format_consumption(render_ms: Option<u64>, energy_j: Option<f64>) -> String {
    match (render_ms, energy_j) {
        (Some(ms), Some(j)) => format!(" - {:.1}s, {j:.0} J", ms as f64 / 1000.0),
        (Some(ms), None) => format!(" - {:.1}s", ms as f64 / 1000.0),
        (None, Some(j)) => format!(" - {j:.0} J"),
        (None, None) => String::new(),
    }
}

/// Drive a streaming `/v1/*` generation (`"stream": true`): parse the
/// SSE `data:` events, forward each `rendering` step as a
/// `TaskResult::MediaProgress`, and push the final `MediaResult` on
/// `done` / `error`. Shared by the Music and Video kinds.
async fn stream_media(
    client: Client,
    path: &'static str,
    body: serde_json::Value,
    tasks: Arc<Mutex<Vec<TaskResult>>>,
    egui_ctx: egui::Context,
    status_label: String,
    // Where to publish the render's identifier once the server names it. Cancelling is done
    // BY NAME: dropping the connection does not stop a render, which was measured rather
    // than assumed - a clip ran to completion for a client that had already gone.
    render_id: Arc<Mutex<Option<String>>>,
) {
    // Read before the body moves into the request: the batch size is what turns n
    // variations into one bar instead of n.
    let batch = body.get("n").and_then(|x| x.as_u64()).unwrap_or(1).max(1);
    let result = match client.post_stream(path, body).await {
        Ok(response) => {
            consume_media_stream(
                response,
                batch,
                &tasks,
                &egui_ctx,
                &status_label,
                &render_id,
            )
            .await
        }
        Err(e) => Err(e.to_string()),
    };
    tasks.lock().unwrap().push(TaskResult::MediaResult(result));
    egui_ctx.request_repaint();
}

/// Read an already-open SSE body to its end, forwarding progress and returning the
/// outcome. Split out of [`stream_media`] so a caller that has to INSPECT the response
/// before committing to it - the speech path, which falls back to a plain request when
/// the server answers with audio instead of events - reuses the same parser rather than
/// carrying a second copy of it.
async fn consume_media_stream(
    mut response: reqwest::Response,
    batch: u64,
    tasks: &Arc<Mutex<Vec<TaskResult>>>,
    egui_ctx: &egui::Context,
    status_label: &str,
    render_id: &Arc<Mutex<Option<String>>>,
) -> Result<MediaOutput, String> {
    let mut line_buf = String::new();
    let mut final_result: Option<Result<MediaOutput, String>> = None;
    // The image route streams a DIFFERENT shape from the audio/video ones: no `status`,
    // one `{index, step, total}` per denoise step and one `{index, done, b64_json}` per
    // finished variation, because the server runs the variation loop itself. Collected
    // here so the batch reads as one bar and each image lands as it finishes.
    let mut images: Vec<String> = Vec::new();

    'outer: loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                line_buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(pos) = line_buf.find('\n') {
                    let line = line_buf[..pos].trim().to_string();
                    line_buf.drain(..=pos);
                    if line.is_empty() {
                        continue;
                    }
                    let json_str = line
                        .strip_prefix("data: ")
                        .or_else(|| line.strip_prefix("data:"))
                        .unwrap_or(&line);
                    if json_str.is_empty() || json_str.starts_with(':') {
                        continue;
                    }
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) else {
                        continue;
                    };
                    // The first message names the render. Kept so Cancel can reach the
                    // server rather than only dropping this end of the wire.
                    if let Some(id) = render_name(&v) {
                        *render_id.lock().unwrap() = Some(id.to_string());
                    }
                    match v.get("status").and_then(|s| s.as_str()).unwrap_or("") {
                        // The render has a name and nothing else to say yet.
                        "started" => {}
                        // `loading` / `synthesizing` are the speech route's phases; they
                        // carry the same four fields as `rendering` and are shown the same
                        // way, so the phase a server adds tomorrow reads correctly here.
                        "rendering" | "loading" | "synthesizing" => {
                            let (label, step, total, node) = progress_from_event(&v);
                            tasks
                                .lock()
                                .unwrap()
                                .push(TaskResult::MediaProgress(label, step, total, node));
                            egui_ctx.request_repaint();
                        }
                        "done" => {
                            let data: Vec<crate::api::types::MediaDatum> = v
                                .get("data")
                                .and_then(|d| serde_json::from_value(d.clone()).ok())
                                .unwrap_or_default();
                            let label = format!(
                                "{}{}",
                                status_label,
                                format_consumption(
                                    v.get("render_ms").and_then(|x| x.as_u64()),
                                    v.get("energy_j").and_then(|x| x.as_f64()),
                                )
                            );
                            final_result = Some(Ok(media_data_to_output(data, label)));
                            break 'outer;
                        }
                        "error" => {
                            let e = v
                                .get("error")
                                .and_then(|s| s.as_str())
                                .unwrap_or("render error")
                                .to_string();
                            final_result = Some(Err(e));
                            break 'outer;
                        }
                        // No `status` at all: the image route. Distinguished by the
                        // fields rather than by a name, because that is what it sends.
                        _ => {
                            if let Some(msg) = v
                                .get("error")
                                .and_then(|e| e.get("message").or(Some(e)))
                                .and_then(|m| m.as_str())
                            {
                                final_result = Some(Err(msg.to_string()));
                                break 'outer;
                            }
                            let index = v.get("index").and_then(|x| x.as_u64()).unwrap_or(0);
                            if v.get("done").and_then(|x| x.as_bool()).unwrap_or(false) {
                                if let Some(b64) = v
                                    .get("b64_json")
                                    .or_else(|| v.get("url"))
                                    .and_then(|x| x.as_str())
                                {
                                    images.push(b64.to_string());
                                    // Land it now rather than at the end of the batch:
                                    // waiting for the fifth variation before showing the
                                    // first is a long time to look at nothing.
                                    tasks.lock().unwrap().push(TaskResult::MediaPartial(
                                        MediaOutput {
                                            images: images.clone(),
                                            status: format!(
                                                "{} of {batch} done - rendering the next...",
                                                images.len()
                                            ),
                                            ..Default::default()
                                        },
                                    ));
                                    egui_ctx.request_repaint();
                                }
                            } else if let (Some(step), Some(total)) = (
                                v.get("step").and_then(|x| x.as_u64()),
                                v.get("total").and_then(|x| x.as_u64()),
                            ) {
                                // ONE bar across the batch: the server numbers the
                                // variations, so a five-image run is a single line to
                                // watch instead of five that each restart at zero.
                                let done = index * total + step;
                                let label = if batch > 1 {
                                    format!("Rendering {}/{batch}", index + 1)
                                } else {
                                    "Rendering".to_string()
                                };
                                tasks.lock().unwrap().push(TaskResult::MediaProgress(
                                    label,
                                    done,
                                    batch * total,
                                    render_node(&v),
                                ));
                                egui_ctx.request_repaint();
                            } else if let Some(text) = v.get("response").and_then(|x| x.as_str()) {
                                // A loading stage, in the server's own words, before any
                                // step exists to count.
                                if !text.is_empty() {
                                    tasks.lock().unwrap().push(TaskResult::MediaProgress(
                                        text.to_string(),
                                        0,
                                        0,
                                        render_node(&v),
                                    ));
                                    egui_ctx.request_repaint();
                                }
                            }
                        }
                    }
                }
            }
            Ok(None) => break,
            Err(e) => {
                if final_result.is_none() {
                    final_result = Some(Err(format!("stream error: {e}")));
                }
                break;
            }
        }
    }

    // The image route has no terminating event - the body simply ends once every
    // variation is done - so a stream that produced images and no error succeeded.
    final_result.unwrap_or_else(|| {
        if images.is_empty() {
            Err("stream ended without a result".to_string())
        } else {
            Ok(MediaOutput {
                images,
                status: status_label.to_string(),
                ..Default::default()
            })
        }
    })
}

/// The server's name for the render this event belongs to, when it carries one.
///
/// Two spellings because the routes use two: `id` on the media ones, `render_id` on
/// speech. Reading only the first left Cancel with nothing to send for a synthesis - it
/// aborted this end of the wire and the server kept speaking - which is exactly what
/// publishing a name is for. The prefix (`r` for media, `s` for speech) is the server's
/// business: this is an opaque key into its registry.
fn render_name(v: &serde_json::Value) -> Option<&str> {
    v.get("id")
        .or_else(|| v.get("render_id"))
        .and_then(|x| x.as_str())
}

/// The progress a `rendering` / `loading` / `synthesizing` event carries: the label the
/// server already resolved, and how far through the phase it is.
///
/// The label is taken from the server rather than mapped here, so a phase added there
/// reads correctly without a client change. Zeroes mean the backend has no count to give,
/// not that it has made no progress - see [`media_progress_status`].
fn progress_from_event(v: &serde_json::Value) -> (String, u64, u64, Option<String>) {
    let num = |k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    let label = v
        .get("phase_label")
        .or_else(|| v.get("phase"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    (label, num("step"), num("total"), render_node(v))
}

/// The node an event says is rendering, when the server named one. A server that runs
/// alone sends null or nothing, and the status then says nothing of where.
fn render_node(v: &serde_json::Value) -> Option<String> {
    v.get("node")
        .and_then(|x| x.as_str())
        .filter(|n| !n.is_empty())
        .map(str::to_string)
}

/// The status line for a progress event: what the server says it is doing, and how far in
/// when there is a distance to be far in.
///
/// A phase with nothing to count says only its name. Some backends report no count at all
/// - a Piper voice is one `.onnx` file, not a directory of shards, so it announces its
/// load with `step: 0, total: 0` - and rendering that as "0/0" states a progress the
/// server never claimed, next to a bar that cannot move.
fn media_progress_status(label: &str, step: u64, total: u64, node: Option<&str>) -> String {
    let what = match (label.is_empty(), total > 0) {
        (true, true) => format!("Rendering step {step}/{total}"),
        (true, false) => "Working".to_string(),
        (false, true) => format!("{label} {step}/{total}"),
        (false, false) => label.to_string(),
    };
    match node {
        Some(node) => format!("{what} on {node}"),
        None => what,
    }
}

/// What a `/v1/audio/speech` reply actually IS, judged by its content type.
///
/// The field that asks for events is one a server can simply not know: nothing on that
/// route rejects unknown fields, so a build that predates it answers 200 with a body of
/// audio. That answer is correct in itself and unreadable to an SSE parser, which
/// reports "stream ended without a result" for a clip that arrived intact.
fn speech_reply_is_events(content_type: Option<&str>) -> bool {
    content_type
        .map(|ct| ct.to_ascii_lowercase().contains("text/event-stream"))
        .unwrap_or(false)
}

/// Should a refused streamed attempt be re-sent as a plain one?
///
/// Only for a refusal of the request SHAPE - any 4xx. That is the family a server which
/// REFUSES `stream_format` (rather than ignoring it) answers with, and the user must
/// still get their speech. A 5xx or a dead connection is not a shape problem: the plain
/// request would meet the same wall, so it is reported instead of spent twice.
fn speech_retry_without_events(err: &str) -> bool {
    // `ClientError::Http` reads "<path> failed with status 400 Bad Request: <body>".
    let Some(rest) = err.split("failed with status ").nth(1) else {
        return false;
    };
    rest.split_whitespace()
        .next()
        .and_then(|c| c.parse::<u16>().ok())
        .is_some_and(|code| (400..500).contains(&code))
}

/// Drive `/v1/audio/speech` as an event stream, falling back to the plain request when
/// the server cannot answer that way.
///
/// The defect: the tab posted and waited. A cold checkpoint takes tens of seconds to
/// read, and every one of them looked like a frozen window - there was no phase to show
/// because the client never asked for one. It asks now, and it still works against a
/// server that has never heard of the field: an audio body is used as the clip it is, and
/// a 4xx is answered by sending the same body without `stream_format`.
async fn stream_speech(
    client: Client,
    body: serde_json::Value,
    tasks: Arc<Mutex<Vec<TaskResult>>>,
    egui_ctx: egui::Context,
    status_label: String,
    render_id: Arc<Mutex<Option<String>>>,
) {
    // The plain body is the streamed one minus the field, so the fallback re-sends what
    // the user asked for rather than a second reading of it.
    let plain = body.clone();
    let mut streamed = body;
    streamed["stream_format"] = serde_json::json!("sse");

    let plainly = || async {
        client
            .audio_speech_from_body(&plain)
            .await
            .map(|b64| MediaOutput {
                audios: vec![b64],
                status: status_label.clone(),
                ..Default::default()
            })
            .map_err(|e| e.to_string())
    };

    let result = match client.post_stream("/v1/audio/speech", streamed).await {
        Ok(response) => {
            let ct = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            if speech_reply_is_events(ct.as_deref()) {
                // Past this point a failure is a failure: the synthesis has begun, and
                // asking again would spend the whole thing a second time.
                consume_media_stream(response, 1, &tasks, &egui_ctx, &status_label, &render_id)
                    .await
            } else {
                // A body of audio - a server that does not know the field. The clip is
                // already on its way, so it is taken as it is rather than paid for twice.
                // Should reading it fail, the plain request is still tried: the streaming
                // client's budget is a SILENCE deadline, and a server that says nothing
                // until a cold checkpoint has been read and spoken can outlast it.
                match speech_body_to_output(response, &status_label).await {
                    Ok(out) => Ok(out),
                    Err(_) => plainly().await,
                }
            }
        }
        Err(e) => {
            let e = e.to_string();
            if speech_retry_without_events(&e) {
                plainly().await
            } else {
                Err(e)
            }
        }
    };
    tasks.lock().unwrap().push(TaskResult::MediaResult(result));
    egui_ctx.request_repaint();
}

/// Read a body of audio off an open response and present it the way the rest of the
/// Media Studio expects: base64 in `audios`.
async fn speech_body_to_output(
    response: reqwest::Response,
    status_label: &str,
) -> Result<MediaOutput, String> {
    use base64::Engine as _;
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("the server returned an empty clip".to_string());
    }
    Ok(MediaOutput {
        audios: vec![base64::engine::general_purpose::STANDARD.encode(&bytes)],
        status: status_label.to_string(),
        ..Default::default()
    })
}

// ============================================================================
// Command-line Arguments
// ============================================================================

/// Atelier - chat and media generation against a local inference server.
#[derive(Parser)]
#[command(name = "atelier")]
#[command(about = "Atelier - chat and media generation against a local inference server")]
#[command(version)]
pub struct Args {
    /// Model to load on startup
    #[arg(short, long)]
    pub model: Option<String>,

    /// Server URL to connect to (default: localhost:11435)
    #[arg(short, long)]
    pub server: Option<String>,
}

// ============================================================================
// Main Application
// ============================================================================

/// Main GUI Application
pub struct LLMGuiApp {
    /// In-GUI audio transport (play/pause/stop/seek over the system player).
    pub audio_player: crate::audio_playback::AudioPlayer,
    pub config: AppConfig,
    pub chat: ChatState,
    pub media: crate::state::MediaState,
    pub cli: CLIState,
    pub models: ModelState,
    pub server: ServerState,
    pub rt: Runtime,
    pub pending_tasks: Arc<Mutex<Vec<TaskResult>>>,
    pub settings_state: SettingsState,
    pub connection_status: ConnectionStatus,
    pub embedded_port: Option<u16>,
    /// Shared log buffer for capturing tracing logs
    pub log_buffer: LogBuffer,
    /// Shared streaming text buffer updated by async task during generation
    pub streaming_text: Arc<Mutex<String>>,
    /// Cache for markdown rendering in chat messages
    pub markdown_cache: egui_commonmark::CommonMarkCache,
    /// Cache for image textures (base64 -> TextureHandle)
    pub image_textures: std::collections::HashMap<String, egui::TextureHandle>,
    /// In-app video playback (decoder + current frame). Beside the texture cache for the
    /// same reason: neither is a value MediaState could clone.
    pub video: crate::video_engine::VideoPlayback,
    /// Open fullscreen image viewer (zoom/pan), or None when closed.
    pub fullscreen_image: Option<crate::image_viewer::FullscreenImage>,
    /// LoRA adapter names the server advertises. Empty when it has none.
    /// Adapters the server offers, with the architecture each was trained for.
    pub available_loras: Vec<(String, Option<String>)>,
    /// Current navigation section (new sidebar layout)
    pub current_section: Section,
    /// Whether the sidebar is expanded (shows labels)
    pub sidebar_expanded: bool,
    /// Throttle for the per-frame window-size save. Without this,
    /// holding the resize handle would call config.save() at 60 Hz
    /// for the duration of the drag (each frame ticks the rect by
    /// 1+ px, triggering a fresh save). Cap at one save per second
    /// — the final dimensions still land within ~1 s of mouse-up.
    pub last_window_size_save: Option<std::time::Instant>,
    /// Cached top-bar metrics string + the (loaded, gpu) counts that
    /// produced it. Rebuilt only when those counts change instead of
    /// every frame — the top bar otherwise allocates a fresh String
    /// per repaint for text that almost never moves (a user with a
    /// loaded model + fixed GPU count saw 60 allocs/sec for the
    /// "1 loaded · 2 GPU" line).
    /// Transient bottom-right notifications. Pushed via `self.toast(..)`,
    /// rendered + expired in update() after the CentralPanel.
    pub(crate) toasts: Vec<Toast>,
    /// Set when the user clicks the top-bar Refresh so the following
    /// ModelsFetched can fire a "refreshed" toast — without this,
    /// every background/startup refresh would emit a noisy toast.
    refresh_toast_pending: bool,
}

impl LLMGuiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Args, log_buffer: LogBuffer) -> Self {
        let config = AppConfig::load();

        // Apply professional theme
        theme::apply(&cc.egui_ctx, config.dark_theme);

        // Configure fonts for Unicode emoji and icons support

        // Image loaders are installed in main.rs before LLMGuiApp::new
        // is called — no need to install them again here. Removing the
        // duplicate keeps the icons.rs docstring (which points at the
        // main.rs install site as the canonical location) honest.

        if let (Some(w), Some(h)) = (config.window_width, config.window_height) {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(w, h)));
        }

        let embedded_port = None;
        // Apply the `--server` override after the app is built, and only when one
        // was given. Resolving it here with a fallback would clone the configured
        // URL in the common case where there is no override, just to assign it back
        // onto itself below. Move the override when present; skip the no-op
        // assignment otherwise.
        let server_url_override = args.server;

        // Capture the persisted sidebar state before `config` is moved
        // into the struct literal below (field shorthand `config,`
        // takes ownership first). Restores the user's collapsed/
        // expanded preference across launches.
        let sidebar_expanded = config.sidebar_expanded;

        let settings_state = SettingsState::new(&config);

        // Log buffer is passed from run() where tracing was initialized

        let mut app = Self {
            audio_player: crate::audio_playback::AudioPlayer::default(),
            available_loras: Vec::new(),
            config,
            chat: ChatState::default(),
            media: crate::state::MediaState::default(),
            cli: CLIState::default(),
            models: ModelState::default(),
            server: ServerState::default(),
            rt: Runtime::new().expect("Failed to create tokio runtime"),
            pending_tasks: Arc::new(Mutex::new(Vec::new())),
            settings_state,
            connection_status: ConnectionStatus::default(),
            embedded_port,
            log_buffer,
            streaming_text: Arc::new(Mutex::new(String::new())),
            markdown_cache: egui_commonmark::CommonMarkCache::default(),
            image_textures: std::collections::HashMap::new(),
            video: crate::video_engine::VideoPlayback::default(),
            fullscreen_image: None,
            current_section: Section::default(),
            sidebar_expanded,
            last_window_size_save: None,
            toasts: Vec::new(),
            refresh_toast_pending: false,
            // (usize::MAX, usize::MAX) is a sentinel that no real
            // (loaded, gpu) tuple can produce; forces the first
            // top-bar render to populate the cache instead of
            // mis-matching against (0, 0).
        };

        if let Some(url) = server_url_override {
            app.config.server_url = url;
        }

        // Hydrate the chat tab's layer-mode toggle from the
        // persisted config. The default is AllLayers, so this is what carries a
        // user's choice of Adaptive across a launch. Settings writes changes back
        // through the config-diff path.
        app.chat.layer_mode = app.config.layer_mode;
        app.chat.smart_auto = app.config.chat_smart_auto;

        // Restore the Media Studio parameters saved at last exit.
        if let Some(mp) = app.config.media.clone() {
            app.media.apply_persist(&mp);
        }

        // Load initial model: from args or from config
        if let Some(model) = args.model {
            app.load_model(model);
        } else if let Some(model) = app.config.selected_model.clone() {
            // Auto-load model from config on startup
            info!("Auto-loading model from config: {}", model);
            app.load_model(model);
        }

        app.refresh_models();
        app
    }

    fn get_client(&self) -> Client {
        Client::with_key(&self.config.server_url, self.config.api_key.as_deref())
    }

    /// Kick off a Media Studio generation for the currently-selected
    /// kind. Spawns the matching `/v1/*` request on the tokio runtime
    /// Rewrite the Media Studio prompt with a local chat model. Model choice is
    /// capability-driven (never a name list): prefer an already-LOADED
    /// chat-capable model (no swap cost), else the first chat-capable model the
    /// server advertises.
    pub fn enhance_media_prompt(&mut self, ctx: &egui::Context) {
        if self.media.enhancing_prompt || self.media.prompt.trim().is_empty() {
            return;
        }
        let chat_capable: Vec<&crate::api::types::ModelInfo> = self
            .models
            .available_models
            .iter()
            .filter(|m| m.has_capability("chat"))
            .collect();
        // Model preference order:
        // 1. the model the user picked in the Chat tab (their trusted assistant,
        //    quality expectations are theirs) when it is chat-capable;
        // 2. an already-loaded chat model (no swap cost);
        // 3. the smallest instruction-worthy ollama entry (>= 1 GB blob: the
        //    sub-GB tail behaves base-model-ish and ignores the rewrite
        //    instruction; HF entries carry untrustworthy sizes and are skipped).
        // CANDIDATES, tried in order until one returns a usable rewrite. Model size
        // is a poor capability proxy on this catalog (a 4.2 GB MoE with 1 B active
        // params fails the task while an 8 B dense model nails it), so instead of
        // guessing once, the enhancer validates each reply and moves to the next
        // candidate when a model cannot follow the instruction:
        //   1. the model selected in the Chat tab (the user's own trusted assistant);
        //   2. models already resident (no load cost);
        //   3. the rest inside a UTILITY SIZE BAND, smallest first: a prompt rewrite
        //      must not drag a 50 GB reasoning model into VRAM, and models under a
        //      few GB cannot follow the instruction. Anything outside the band is
        //      kept as a last resort only.
        // An explicit pick in the Media Studio wins outright: the user chose the
        // model, so it is the only candidate (its reply is still validated, so a
        // weak choice reports WHY rather than silently pasting junk).
        if let Some(pick) = self.media.enhance_model.clone() {
            if chat_capable.iter().any(|m| m.name == pick) {
                let candidates = vec![pick];
                return self.spawn_enhance(ctx, candidates);
            }
        }
        let selected = self.models.selected_model.clone();
        let mut candidates: Vec<String> = Vec::new();
        let push = |name: &str, out: &mut Vec<String>| {
            if !out.iter().any(|n| n == name) {
                out.push(name.to_string());
            }
        };
        if let Some(sel) = selected.as_deref() {
            if chat_capable.iter().any(|m| m.name == sel) {
                push(sel, &mut candidates);
            }
        }
        for m in &chat_capable {
            if self.models.loaded_models.iter().any(|l| l == &m.name) {
                push(&m.name, &mut candidates);
            }
        }
        // Utility band: big enough to follow an instruction, small enough that a
        // rewrite stays a quick side call (the 7-13 B quantized tier on this
        // catalog). Ascending inside the band, then whatever is left.
        const BAND_LO: u64 = 3_000_000_000;
        const BAND_HI: u64 = 12_000_000_000;
        let sized: Vec<&&crate::api::types::ModelInfo> = chat_capable
            .iter()
            .filter(|m| m.source == "ollama" && m.size_bytes > 0)
            .collect();
        let mut in_band: Vec<_> = sized
            .iter()
            .filter(|m| (BAND_LO..=BAND_HI).contains(&m.size_bytes))
            .collect();
        in_band.sort_by_key(|m| m.size_bytes);
        for m in in_band {
            push(&m.name, &mut candidates);
        }
        let mut out_band: Vec<_> = sized
            .iter()
            .filter(|m| !(BAND_LO..=BAND_HI).contains(&m.size_bytes))
            .collect();
        out_band.sort_by_key(|m| m.size_bytes);
        for m in out_band {
            push(&m.name, &mut candidates);
        }
        // Three attempts is the useful ceiling: each failure costs a model load.
        candidates.truncate(3);
        let model = candidates.first().cloned();
        if model.is_none() {
            self.toast(
                ToastSeverity::Error,
                "No chat-capable model available to enhance the prompt.".to_string(),
            );
            return;
        }
        self.spawn_enhance(ctx, candidates)
    }

    /// Run the enhancement against `candidates` in order, keeping the first reply
    /// that validates as a real rewritten prompt.
    fn spawn_enhance(&mut self, ctx: &egui::Context, candidates: Vec<String>) {
        let medium = match self.media.kind {
            // An edit prompt is an INSTRUCTION about an existing image, not a scene
            // description - rewriting it as one makes the editor regenerate instead
            // of edit.
            MediaKind::ImageEdit => "a precise image-EDIT instruction: name exactly what to change and how (colour, material, position, addition or removal), and state what must stay untouched; never describe a whole new scene",
            MediaKind::Image => "an image-generation prompt: add concrete subject, composition, lighting, style and quality details",
            MediaKind::Music => "a music-generation prompt: add genre, instrumentation, tempo, mood and production details",
            MediaKind::Sfx => "a sound-effect prompt: describe the acoustic event precisely (source, material, space, dynamics)",
            MediaKind::Video => "a video-generation prompt: add subject, motion, camera framing, lighting and scene details",
            MediaKind::Midi => "a MIDI/melody prompt: add key, tempo, structure and instrumentation details",
            _ => "a text-to-speech script: improve flow and clarity without changing the meaning",
        };
        let system = format!(
            "Rewrite the user's text into ONE improved generation prompt: {medium}. \
             Rules: output a single paragraph, 60 words or less; keep the user's language \
             and intent; do NOT write lists, headings, advice or explanations; do NOT \
             address the user; output the rewritten prompt text and nothing else."
        );
        let user_text = self.media.prompt.clone();
        let make_request = move |model: String, system: String, user: String| {
            let mut r = crate::api::types::OllamaChatRequest::new(
                model,
                vec![
                    crate::api::types::Message::new("system".into(), system),
                    crate::api::types::Message::new("user".into(), user),
                ],
            );
            r.stream = false;
            r.options = Some(serde_json::json!({"temperature": 0.4, "num_predict": 400}));
            r.thinking = Some("disabled".into());
            r
        };
        self.media.enhancing_prompt = true;
        let original = self.media.prompt.clone();
        let client = self.get_client();
        let tasks = self.pending_tasks.clone();
        let egui_ctx = ctx.clone();
        self.rt.spawn(async move {
            let mut last_err = "no candidate model".to_string();
            let mut result: Option<String> = None;
            for cand in &candidates {
                let req = make_request(cand.clone(), system.clone(), user_text.clone());
                let attempt = client
                    .chat(&req)
                    .await
                    .map(|r| {
                        // Thinking models may still emit an inline <think> block -
                        // keep only the answer that follows it.
                        let c = r.message.content;
                        let c = match c.rfind("</think>") {
                            Some(i) => c[i + "</think>".len()..].to_string(),
                            None => c,
                        };
                        c.trim().trim_matches('"').trim().to_string()
                    })
                    .map_err(|e| e.to_string())
                    .and_then(|t| validate_enhanced_prompt(&t, &original));
                match attempt {
                    Ok(t) => {
                        result = Some(t);
                        break;
                    }
                    Err(e) => last_err = format!("{cand}: {e}"),
                }
            }
            tasks
                .lock()
                .unwrap()
                .push(TaskResult::MediaPromptEnhanced(result.ok_or(last_err)));
            egui_ctx.request_repaint();
        });
    }

    /// and pushes a `TaskResult::MediaResult` when it completes.
    pub fn send_media(&mut self, ctx: &egui::Context) {
        if self.media.is_generating {
            return;
        }
        if self.media.prompt.trim().is_empty()
            && !matches!(self.media.kind, MediaKind::Transcribe | MediaKind::Separate)
        {
            self.media.error = Some("Enter a prompt first.".to_string());
            return;
        }

        let client = self.get_client();
        let tasks = self.pending_tasks.clone();
        let egui_ctx = ctx.clone();
        let kind = self.media.kind;
        let prompt = self.media.prompt.clone();
        let seed = self.media.parsed_seed();

        self.media.begin_generation();

        match kind {
            MediaKind::Image => {
                let m = &self.media.image;
                let guidance = (m.guidance > 0.0).then_some(m.guidance as f64);
                let controls = crate::api::types::RenderControls {
                    loras: m.loras.clone(),
                    regions: m.regions.clone(),
                    control: m
                        .control
                        .as_ref()
                        .map(|(_, b)| (b.clone(), m.control_scale)),
                    sampler: m.sampler.clone(),
                    scheduler: m.scheduler.clone(),
                    output_format: m.file_format.wire().to_string(),
                    negative_prompt: m.negative_prompt.clone(),
                };
                // STREAM IT. The same endpoint and the same body - `image_request_body` is
                // shared with the plain POST - so no control is traded away for the
                // progress. The server already ran the variation loop itself, which is why
                // the client no longer does: doing both offsets every seed twice.
                let mut body = crate::api::client::image_request_body(
                    &m.model, &prompt, m.width, m.height, m.n, m.steps, guidance, seed, &controls,
                );
                body["stream"] = serde_json::json!(true);
                self.media.generation_abort = Some(
                    self.rt
                        .spawn(stream_media(
                            client,
                            "/v1/images/generations",
                            body,
                            tasks,
                            egui_ctx,
                            "Image generated.".to_string(),
                            self.media.render_id.clone(),
                        ))
                        .abort_handle(),
                );
            }
            MediaKind::Music => {
                let m = self.media.music.clone();
                let seed_val = seed.unwrap_or_else(time_seed);
                let mut body = serde_json::json!({
                    "model": "ace-step",
                    "prompt": prompt,
                    "seconds": m.seconds,
                    "steps": m.steps,
                    "bpm": m.bpm,
                    "cfg": m.cfg,
                    "temperature": m.temperature,
                    "top_p": m.top_p,
                    "force_duration": m.force_duration,
                    "loop": m.loop_mode,
                    "bars": m.loop_bars,
                    "dit_model": m.dit_model,
                    "seed": seed_val,
                    "stream": true,
                });
                for (key, val) in [
                    ("lyrics", &m.lyrics),
                    ("negative_prompt", &m.negative_prompt),
                    ("keyscale", &m.keyscale),
                    ("language", &m.language),
                ] {
                    if !val.trim().is_empty() {
                        body[key] = serde_json::json!(val);
                    }
                }
                self.media.generation_abort = Some(
                    self.rt
                        .spawn(stream_media(
                            client,
                            "/v1/audio/generations",
                            body,
                            tasks,
                            egui_ctx,
                            "Music generated.".to_string(),
                            self.media.render_id.clone(),
                        ))
                        .abort_handle(),
                );
            }
            MediaKind::Sfx => {
                let s = &self.media.sfx;
                let is_sao = s.model == "stable-audio";
                let seed_val = seed.unwrap_or_else(time_seed);
                let mut body = serde_json::json!({
                    "model": if is_sao { "stable-audio" } else { "ezaudio" },
                    "prompt": prompt,
                    "seconds": s.seconds,
                    "steps": s.steps,
                    "cfg": s.cfg,
                    "seed": seed_val,
                });
                if is_sao {
                    // The stable-audio path streams per-step progress (SSE).
                    body["stream"] = serde_json::json!(true);
                    if !s.negative_prompt.trim().is_empty() {
                        body["negative_prompt"] = serde_json::json!(s.negative_prompt);
                    }
                    if let Some((_, bytes)) = &s.init_audio {
                        use base64::Engine;
                        body["init_audio"] = serde_json::json!(
                            base64::engine::general_purpose::STANDARD.encode(bytes)
                        );
                        body["init_noise_level"] = serde_json::json!(s.init_noise_level);
                    }
                    if s.loop_mode {
                        body["loop"] = serde_json::json!(true);
                        body["bars"] = serde_json::json!(s.loop_bars);
                        body["bpm"] = serde_json::json!(s.loop_bpm);
                    }
                    self.media.generation_abort = Some(
                        self.rt
                            .spawn(stream_media(
                                client,
                                "/v1/audio/generations",
                                body,
                                tasks,
                                egui_ctx,
                                "Sound generated.".to_string(),
                                self.media.render_id.clone(),
                            ))
                            .abort_handle(),
                    );
                } else {
                    // EzAudio streams per-step progress too (same SSE shape).
                    body["stream"] = serde_json::json!(true);
                    self.media.generation_abort = Some(
                        self.rt
                            .spawn(stream_media(
                                client,
                                "/v1/audio/generations",
                                body,
                                tasks,
                                egui_ctx,
                                "Sound effect generated.".to_string(),
                                self.media.render_id.clone(),
                            ))
                            .abort_handle(),
                    );
                }
            }
            MediaKind::Speech => {
                use crate::state::SpeechEngine;
                // The `/v1/audio/speech` backend routes by MODEL name: Parler by preset
                // voice, Kyutai/Piper by voice embedded in the model id.
                let vname = self.media.speech.voice_name.trim().to_string();
                let (model, voice) = match self.media.speech.engine {
                    SpeechEngine::Parler => (
                        "parler-tts-mini-v1".to_string(),
                        self.media.speech.voice.clone(),
                    ),
                    SpeechEngine::Kyutai => (
                        if vname.is_empty() {
                            "kyutai".to_string()
                        } else {
                            format!("kyutai-{vname}")
                        },
                        String::new(),
                    ),
                    SpeechEngine::Piper => (
                        if vname.is_empty() {
                            "piper".to_string()
                        } else {
                            format!("piper/{vname}")
                        },
                        String::new(),
                    ),
                };
                let desc_opt = (!self.media.speech.voice_description.trim().is_empty())
                    .then(|| self.media.speech.voice_description.clone());
                let voice_opt = (!voice.trim().is_empty()).then_some(voice.as_str());
                // STREAM IT, for the phase the user waits through longest: reading the
                // checkpoint. Same body either way, so nothing is traded for the progress
                // and a server without the field still answers - see `stream_speech`.
                let body = crate::api::client::speech_request_body(
                    &model,
                    &prompt,
                    voice_opt,
                    desc_opt.as_deref(),
                );
                self.media.generation_abort = Some(
                    self.rt
                        .spawn(stream_speech(
                            client,
                            body,
                            tasks,
                            egui_ctx,
                            "Speech generated.".to_string(),
                            self.media.render_id.clone(),
                        ))
                        .abort_handle(),
                );
            }
            MediaKind::Midi => {
                let (max_tokens, temperature, top_p) = (
                    self.media.midi.max_tokens,
                    self.media.midi.temperature,
                    self.media.midi.top_p,
                );
                let seed_val = seed.unwrap_or_else(time_seed);
                let body = serde_json::json!({
                    "model": "midi",
                    "prompt": prompt,
                    "max_tokens": max_tokens,
                    "temperature": temperature,
                    "top_p": top_p,
                    "seed": seed_val,
                });
                self.media.generation_abort =
                    Some(spawn_media_result(&self.rt, tasks, egui_ctx, async move {
                        client
                            .audio_generate(body)
                            .await
                            .map(|data| media_data_to_output(data, "MIDI generated.".to_string()))
                            .map_err(|e| e.to_string())
                    }));
            }
            MediaKind::Video => {
                let (frames, w, h, steps, cfg) = (
                    crate::state::frames_for_seconds(self.media.video.seconds),
                    self.media.video.width,
                    self.media.video.height,
                    self.media.video.steps,
                    self.media.video.cfg,
                );
                let fmt = self.media.video.format.as_str().to_string();
                let seed_val = seed.unwrap_or_else(time_seed);
                let mut body = serde_json::json!({
                    "model": self.media.video.model,
                    "prompt": prompt,
                    "frames": frames,
                    "size": format!("{}x{}", w, h),
                    "steps": steps,
                    "seed": seed_val,
                    "format": fmt,
                    "stream": true,
                });
                // 0 = let the server apply its resolution-aware default CFG.
                if cfg > 0.0 {
                    body["cfg"] = serde_json::json!(cfg);
                }
                if let Some(smp) = self.media.video.sampler.as_str() {
                    body["sampler"] = serde_json::json!(smp);
                }
                // Omitted when blank so the server keeps its own unconditional branch.
                if !self.media.video.negative_prompt.trim().is_empty() {
                    body["negative_prompt"] =
                        serde_json::json!(self.media.video.negative_prompt.trim());
                }
                // The frame an image-to-video checkpoint continues. Without it that model
                // has nothing to continue and refuses, which is a 400 the user can read -
                // but only if the tab can send the picture at all, and it could not.
                if let Some((_, bytes)) = self.media.video.start_image.as_ref() {
                    use base64::Engine as _;
                    body["image"] =
                        serde_json::json!(base64::engine::general_purpose::STANDARD.encode(bytes));
                }
                self.media.generation_abort = Some(
                    self.rt
                        .spawn(stream_media(
                            client,
                            "/v1/video/generations",
                            body,
                            tasks,
                            egui_ctx,
                            "Video generated.".to_string(),
                            self.media.render_id.clone(),
                        ))
                        .abort_handle(),
                );
            }
            MediaKind::ImageEdit => {
                let p = self.media.image_edit.clone();
                let partial_tasks = tasks.clone();
                let partial_ctx = egui_ctx.clone();
                self.media.generation_abort =
                    Some(spawn_media_result(&self.rt, tasks, egui_ctx, async move {
                        let Some((name, bytes)) = p.source else {
                            return Err("Choose a source image first.".to_string());
                        };
                        if p.n <= 1 {
                            return client
                                .images_edit(
                                    &p.model,
                                    &prompt,
                                    &name,
                                    bytes,
                                    p.strength,
                                    p.steps,
                                    p.guidance,
                                    p.n,
                                    seed,
                                    &p.loras,
                                    &p.negative_prompt,
                                )
                                .await
                                .map(|(images, ms, j)| MediaOutput {
                                    images,
                                    status: format!("Image edited.{}", format_consumption(ms, j)),
                                    ..Default::default()
                                })
                                .map_err(|e| e.to_string());
                        }
                        // Multi-variation edits: one request per output, streamed to
                        // the panel as they finish (seed+i mirrors the batch seeding).
                        let base_seed = seed.unwrap_or_else(time_seed);
                        let mut images: Vec<String> = Vec::new();
                        let (mut ms_sum, mut j_sum) = (0u64, 0f64);
                        for i in 0..p.n {
                            let (mut imgs, ms, j) = client
                                .images_edit(
                                    &p.model,
                                    &prompt,
                                    &name,
                                    bytes.clone(),
                                    p.strength,
                                    p.steps,
                                    p.guidance,
                                    1,
                                    Some(base_seed + i as u64),
                                    &p.loras,
                                    &p.negative_prompt,
                                )
                                .await
                                .map_err(|e| format!("variation {}/{}: {e}", i + 1, p.n))?;
                            images.append(&mut imgs);
                            ms_sum += ms.unwrap_or(0);
                            j_sum += j.unwrap_or(0.0);
                            if i + 1 < p.n {
                                partial_tasks.lock().unwrap().push(TaskResult::MediaPartial(
                                    MediaOutput {
                                        images: images.clone(),
                                        status: format!(
                                            "Variation {}/{} done — rendering the next…",
                                            i + 1,
                                            p.n
                                        ),
                                        ..Default::default()
                                    },
                                ));
                                partial_ctx.request_repaint();
                            }
                        }
                        let ms = (ms_sum > 0).then_some(ms_sum);
                        let j = (j_sum > 0.0).then_some(j_sum);
                        Ok(MediaOutput {
                            images,
                            status: format!(
                                "{} edit variations generated.{}",
                                p.n,
                                format_consumption(ms, j)
                            ),
                            ..Default::default()
                        })
                    }));
            }
            MediaKind::Separate => {
                let p = self.media.separate.clone();
                self.media.generation_abort =
                    Some(spawn_media_result(&self.rt, tasks, egui_ctx, async move {
                        let Some((name, bytes)) = p.audio else {
                            return Err("Choose the track to split first.".to_string());
                        };
                        let stems = client
                            .audio_separate(&name, bytes, p.stems.wire())
                            .await
                            .map_err(|e| e.to_string())?;
                        let labels: Vec<&str> = stems.iter().map(|(k, _)| k.as_str()).collect();
                        Ok(MediaOutput {
                            audios: stems.iter().map(|(_, b64)| b64.clone()).collect(),
                            status: format!("Separated: {}.", labels.join(" + ")),
                            ..Default::default()
                        })
                    }));
            }
            MediaKind::Transcribe => {
                let p = self.media.transcribe.clone();
                self.media.generation_abort =
                    Some(spawn_media_result(&self.rt, tasks, egui_ctx, async move {
                        match p.audio {
                            Some((name, bytes)) => client
                                .audio_transcribe(&p.model, &name, bytes, p.translate, &p.language)
                                .await
                                .map(|text| MediaOutput {
                                    text: Some(text),
                                    status: "Transcribed.".to_string(),
                                    ..Default::default()
                                })
                                .map_err(|e| e.to_string()),
                            None => Err("Choose an audio file first.".to_string()),
                        }
                    }));
            }
        }
    }

    /// Refresh the voice list for the Speech kind from
    /// GET /v1/audio/voices.
    pub fn refresh_media_voices(&mut self, ctx: &egui::Context) {
        let client = self.get_client();
        let tasks = self.pending_tasks.clone();
        let egui_ctx = ctx.clone();
        self.rt.spawn(async move {
            if let Ok(voices) = client.list_voices().await {
                tasks
                    .lock()
                    .unwrap()
                    .push(TaskResult::MediaVoicesFetched(voices));
                egui_ctx.request_repaint();
            }
        });
    }

    /// Keep the Media Studio's video estimate in step with the settings on screen.
    ///
    /// A video render is the only thing here that can run for an hour, and the settings
    /// that decide it - frame size, length, steps, checkpoint - were all adjustable with no
    /// indication of what any of them cost, so the wait was discovered by waiting. The
    /// server can price a set of settings without touching a model, so it is asked as the
    /// controls move.
    ///
    /// Called every frame, and cheap on the frames where nothing changed: it only dispatches
    /// when the settings signature differs from the one already asked about, and never while
    /// an answer is outstanding. A slider drag therefore turns into a short chain of
    /// requests rather than one per frame, and it still converges on the value the user
    /// stopped at, because the frame after each answer re-checks the signature.
    pub fn refresh_video_estimate(&mut self, ctx: &egui::Context) {
        let key = self.media.video_estimate_key();
        if self.media.estimate_in_flight || self.media.estimate_key.as_deref() == Some(&key) {
            return;
        }
        // A failure asked for again, but not at once: the tab recovers on its own when the
        // server comes back, without turning an unreachable one into a request per frame.
        if self
            .media
            .estimate_retry_after
            .is_some_and(|t| std::time::Instant::now() < t)
        {
            return;
        }
        self.media.estimate_retry_after = None;
        // Marked as asked BEFORE the request leaves: a failure must not put the tab into a
        // retry loop against a server that is not answering.
        self.media.estimate_key = Some(key.clone());
        self.media.estimate_in_flight = true;

        // Settings are sent EXPLICITLY rather than as an intent, so the number shown is
        // the cost of the controls the user is looking at and not of a plan the server
        // would have chosen for them.
        let body = serde_json::json!({
            "model": self.media.video.model,
            "width": self.media.video.width,
            "height": self.media.video.height,
            "frames": crate::state::frames_for_seconds(self.media.video.seconds),
            "steps": self.media.video.steps,
        });
        let client = self.get_client();
        let tasks = self.pending_tasks.clone();
        let egui_ctx = ctx.clone();
        self.rt.spawn(async move {
            let seconds = client
                .plan_video(body)
                .await
                .ok()
                .and_then(|v| v.get("estimated_seconds").and_then(|x| x.as_f64()))
                .map(|s| s as f32);
            tasks
                .lock()
                .unwrap()
                .push(TaskResult::VideoEstimate { key, seconds });
            egui_ctx.request_repaint();
        });
    }

    /// Queue a transient bottom-right toast. The single funnel for
    /// ephemeral feedback (model actions, errors, config saves, server
    /// switches) so every surface reports through the same channel.
    pub fn toast(&mut self, severity: ToastSeverity, message: impl Into<String>) {
        self.toasts.push(Toast::new(severity, message));
    }

    pub fn refresh_models(&mut self) {
        if self.models.action_status.is_in_progress() {
            return;
        }

        let client = self.get_client();
        let tasks = self.pending_tasks.clone();

        // A refresh is NOT a mutating action: gating the Models-tab buttons on it
        // meant a refresh whose response never arrived (a wedged or restarted
        // server) left Load / Unload / Delete disabled forever, with no way back
        // from the UI. Connection status already reports refresh progress.
        self.connection_status = ConnectionStatus::connecting();

        self.rt.spawn(async move {
            let available = match client.list_models().await {
                Ok(resp) => resp.models,
                Err(e) => {
                    tasks
                        .lock()
                        .unwrap()
                        .push(TaskResult::Error(format!("Failed to list models: {}", e)));
                    return;
                }
            };

            let loaded = match client.list_loaded_models().await {
                Ok(resp) => resp.models.into_iter().map(|m| m.model).collect(),
                Err(e) => {
                    // The available-models fetch above already
                    // succeeded, so connection_status will read
                    // "Connected" — but the loaded count silently
                    // shows 0, which is misleading if the server
                    // actually has models loaded (it just hiccuped
                    // on /api/ps). Log so the user can trace why
                    // the count under-reports via the Server Log.
                    tracing::warn!("/api/ps (loaded models): {e}");
                    Vec::new()
                }
            };

            // Adapters are inventory too, and a server without any (or an older one
            // with no such route) simply reports none - this must not fail the refresh.
            let loras = client.list_loras().await.unwrap_or_default();

            let mut q = tasks.lock().unwrap();
            q.push(TaskResult::ModelsFetched(available, loaded));
            q.push(TaskResult::LorasFetched(loras));
        });
    }

    pub fn load_model(&mut self, model_name: String) {
        if self.models.action_status.is_in_progress() {
            return;
        }

        // Loading a new model unloads everything else first. If the
        // user is mid-chat against one of those soon-to-be-unloaded
        // models, the in-flight request would error or hang once the
        // server drops the model. Pre-empt by aborting the chat task
        // here (mirrors the Esc / Stop button cleanup) so the user
        // sees a clean "[Generation cancelled — model swap]" instead
        // of a confusing server error.
        self.abort_chat_if_against_other_model(&model_name);

        let client = self.get_client();
        let tasks = self.pending_tasks.clone();
        let model = model_name.clone();
        let is_ollama = self.config.server_url.contains(":11434");

        // Unload all other loaded models first to free GPU memory
        let models_to_unload: Vec<String> = self
            .models
            .loaded_models
            .iter()
            .filter(|m| **m != model_name)
            .cloned()
            .collect();

        self.models.action_started_at = Some(std::time::Instant::now());
        self.models.action_status = ActionStatus::InProgress(format!("Loading {}...", model_name));

        let model_for_result = model_name.clone();
        self.rt.spawn(async move {
            // Unload other models before loading the new one
            for other in &models_to_unload {
                if let Err(e) = client.unload_model(other).await {
                    tracing::warn!("Failed to unload {}: {}", other, e);
                }
            }

            let result = if is_ollama {
                let request = OllamaChatRequest::new(
                    model.clone(),
                    vec![Message::new("system".into(), "".into())],
                );
                client
                    .chat(&request)
                    .await
                    .map(|_| LoadModelResponse {
                        model: model.clone(),
                        status: "loaded".into(),
                        message: format!("Model {} ready", model),
                    })
                    .map_err(|e| e.to_string())
            } else {
                client.load_model(&model).await.map_err(|e| e.to_string())
            };
            tasks
                .lock()
                .unwrap()
                .push(TaskResult::ModelLoaded(result, model_for_result));
        });
    }

    pub fn unload_model(&mut self, model_name: String) {
        // Deliberately NOT gated on action_status: unload is cheap and idempotent,
        // and it is what a user reaches for when something else is stuck. Gating it
        // is what made the button look dead.

        // If the user is mid-chat against THIS model, abort the
        // in-flight request before pulling the rug out. Otherwise
        // the chat task keeps streaming until the server errors,
        // which surfaces as a generic "stream error" the user
        // can't connect to "I just clicked Unload".
        self.abort_chat_if_against_model(&model_name);

        let client = self.get_client();
        let tasks = self.pending_tasks.clone();
        let model = model_name.clone();

        self.models.action_status =
            ActionStatus::InProgress(format!("Unloading {}...", model_name));

        let model_for_result = model_name.clone();
        self.rt.spawn(async move {
            // Call the server to actually unload the model
            // For Ollama, we use keep_alive: 0 to unload
            let result = client.unload_model(&model).await;
            tasks.lock().unwrap().push(TaskResult::ModelUnloaded(
                result.map_err(|e| e.to_string()),
                model_for_result,
            ));
        });
    }

    pub fn delete_model(&mut self, model_name: String) {
        if self.models.action_status.is_in_progress() {
            return;
        }

        // Same pre-empt as unload: if chat is mid-stream against the
        // doomed model, abort first so the user gets a clean
        // breadcrumb instead of an opaque server error.
        self.abort_chat_if_against_model(&model_name);

        let client = self.get_client();
        let tasks = self.pending_tasks.clone();
        let model = model_name.clone();

        self.models.action_started_at = Some(std::time::Instant::now());
        self.models.action_status = ActionStatus::InProgress(format!("Deleting {}...", model_name));

        let model_for_result = model_name.clone();
        self.rt.spawn(async move {
            let result = client.delete_model(&model).await;
            let mut tasks = tasks.lock().unwrap();
            tasks.push(TaskResult::ModelDeleted(
                result.map_err(|e| e.to_string()),
                model_for_result,
            ));
        });
    }

    /// Abort the in-flight chat task IFF it's running against
    /// `target_model`. Used by unload/delete so the user gets a
    /// clean "[Generation cancelled — model unloaded]" breadcrumb
    /// instead of a generic server error when the model disappears
    /// out from under the stream.
    ///
    /// No-op if the chat is idle OR the chat is against a different
    /// model OR no model is currently selected — the safe path is
    /// to let the existing task finish naturally in all those cases.
    fn abort_chat_if_against_model(&mut self, target_model: &str) {
        if !self.chat.is_generating {
            return;
        }
        let selected = self.models.selected_model.as_deref();
        if selected != Some(target_model) {
            return;
        }
        if self.chat.abort_generation() {
            self.chat
                .messages
                .push_back(crate::state::ChatMessage::system(format!(
                    "[Generation cancelled — model {target_model} unloaded]"
                )));
        }
    }

    /// Abort the in-flight chat task IFF the chat is running against
    /// a model OTHER than `incoming_model`. Used by load_model — when
    /// loading model X, the server first unloads everything else,
    /// so any chat against Y (the previous model) would error mid-
    /// stream. Pre-empt with a clean breadcrumb naming the new model.
    fn abort_chat_if_against_other_model(&mut self, incoming_model: &str) {
        if !self.chat.is_generating {
            return;
        }
        let selected = self.models.selected_model.as_deref();
        match selected {
            // Already against the incoming model — load is a no-op
            // from chat's perspective; leave the stream alone.
            Some(m) if m == incoming_model => return,
            _ => {}
        }
        if self.chat.abort_generation() {
            self.chat
                .messages
                .push_back(crate::state::ChatMessage::system(format!(
                    "[Generation cancelled — loading {incoming_model}]"
                )));
        }
    }

    pub fn pull_model_with_source(&mut self, model_name: &str, source: &str) {
        if self.models.action_status.is_in_progress() {
            return;
        }

        if model_name.trim().is_empty() {
            self.models.action_status =
                ActionStatus::Failed("Please enter a model name".to_string());
            return;
        }

        let client = self.get_client();
        let tasks = self.pending_tasks.clone();
        let model = model_name.to_string();
        let source_str = source.to_string();

        self.models.action_started_at = Some(std::time::Instant::now());
        self.models.action_status =
            ActionStatus::InProgress(format!("Pulling {} from {}...", model_name, source));
        // Fresh pull → clear any stale progress from a previous download so
        // the bar starts in the indeterminate manifest phase.
        self.models.pull_progress = None;

        let model_for_result = model_name.to_string();
        let progress_tasks = self.pending_tasks.clone();
        self.rt.spawn(async move {
            // Consume the NDJSON progress stream; each byte-progress line
            // is forwarded as a PullProgress task so the UI bar advances.
            // The 100ms in-progress repaint (app.update) surfaces them.
            let result = match client
                .pull_model_stream(&model, &source_str, |completed, total| {
                    progress_tasks
                        .lock()
                        .unwrap()
                        .push(TaskResult::PullProgress(completed, total));
                })
                .await
            {
                Ok(()) => Ok(()),
                Err(e) => Err(e.to_string()),
            };
            let mut tasks = tasks.lock().unwrap();
            tasks.push(TaskResult::ModelPulled(result, model_for_result));
        });
    }

    pub fn send_chat(&mut self, ctx: &egui::Context) {
        // Smart routing (Auto): the server picks the model per prompt via
        // /conversation. Bypasses the fixed-model modality gates entirely.
        if self.chat.smart_auto {
            if self.chat.is_generating {
                return;
            }
            let input_empty = self.chat.input.trim().is_empty();
            let has_attachment = !self.chat.attached_images.is_empty();
            if input_empty && !has_attachment {
                return;
            }
            let prompt = self.chat.input.trim().to_string();
            let images: Vec<String> = self.chat.attached_images.clone();
            // Fold the whole visible history (user/assistant turns) into the
            // request so the routed chat model keeps context across turns.
            let mut msgs: Vec<serde_json::Value> = self
                .chat
                .messages
                .iter()
                .filter(|m| m.role == "user" || m.role == "assistant")
                .map(|m| serde_json::json!({ "role": m.role, "content": m.content }))
                .collect();
            let mut user_turn = serde_json::json!({ "role": "user", "content": prompt });
            if !images.is_empty() {
                user_turn["images"] = serde_json::json!(images);
            }
            msgs.push(user_turn);
            let conv_id = self.chat.conversation_id_or_new();
            // The selection is forwarded as a chat-model PREFERENCE only; the
            // server is the authority on whether it can chat and silently
            // falls back to its default LLM when it cannot (e.g. an image
            // checkpoint was selected when Auto got enabled).
            let chat_model_hint = self.models.selected_model.clone();

            self.chat.messages.push_back(ChatMessage {
                role: "user".to_string(),
                content: prompt.clone(),
                timestamp: crate::timefmt::chat_now(),
                timing: None,
                images: images.clone(),
                generated_images: Vec::new(),
                generated_audios: Vec::new(),
            });
            self.chat.push_prompt_history(prompt.clone());
            self.chat.input.clear();
            self.chat.clear_attachments();
            self.chat.is_generating = true;

            let client = self.get_client();
            let tasks = self.pending_tasks.clone();
            let egui_ctx = ctx.clone();
            self.rt.spawn(async move {
                let result = client
                    .conversation(
                        serde_json::Value::Array(msgs),
                        &conv_id,
                        chat_model_hint.as_deref(),
                    )
                    .await
                    .map_err(|e| e.to_string());
                tasks
                    .lock()
                    .unwrap()
                    .push(crate::task::TaskResult::ConversationResult(result));
                egui_ctx.request_repaint();
            });
            return;
        }
        // Single source of truth shared with the Send button enabling
        // (chat_tab.rs::chat_send_allowed). Same helper drives both
        // surfaces so the gate can't drift on which modality / input /
        // attachment combinations are sendable.
        let modality_for_guard = self
            .models
            .selected_model
            .as_deref()
            .map(crate::modality::ModelModality::from_model_name)
            .unwrap_or(crate::modality::ModelModality::Text);
        let input_empty = self.chat.input.trim().is_empty();
        let has_attachment = !self.chat.attached_images.is_empty();
        if !crate::modality::chat_send_allowed(modality_for_guard, input_empty, has_attachment)
            || self.chat.is_generating
        {
            return;
        }

        let model = match &self.models.selected_model {
            Some(m) => m.clone(),
            None => {
                // Surface the error in BOTH places: the Models tab action
                // status (legacy behaviour, useful when settings is open),
                // AND inline in the chat as a system message — users
                // clicking Send in the Chat tab won't notice a status
                // change on a tab they're not looking at.
                self.models.action_status = ActionStatus::Failed("No model selected".to_string());
                // Match the empty-state subtitle wording so the two
                // entry points (empty chat vs. attempted send with
                // nothing picked) don't disagree on where to pick a
                // model. Header dropdown is the fast path; Models
                // tab is the import path.
                self.chat.messages.push_back(ChatMessage::system(
                    "No model selected — pick one from the dropdown above, \
                     or open the Models tab to import one.",
                ));
                return;
            }
        };

        // First send to an unloaded model triggers a load on the server
        // — several seconds with no token stream. Warn once so the wait
        // reads as expected rather than a hang (the typing indicator +
        // header banner reinforce it).
        if !self.models.is_loaded(&model) {
            self.toast(
                ToastSeverity::Warning,
                format!("Loading {model} into memory — first response will be slower"),
            );
        }

        let prompt = self.chat.input.trim().to_string();

        // Pre-flight by modality. All checks early-return BEFORE
        // .input.clear() / push_prompt_history so the user can edit
        // + retry without losing what they typed.
        //
        // Reuse `modality_for_guard` from the early-return check above
        // — at this point we know selected_model.is_some() (otherwise
        // the early-return arm at line 607 would have returned), so
        // the guard's value is identical to what
        // `from_model_name(&model)` would produce. One classifier walk
        // per send instead of two.
        let modality_for_send = modality_for_guard;
        match modality_for_send {
            // TTS: server caps input at 4096 chars (OpenAI tts-1).
            crate::modality::ModelModality::AudioTts => {
                let chars = prompt.chars().count();
                if chars > crate::modality::TTS_INPUT_MAX_CHARS_CLIENT {
                    self.chat.messages.push_back(ChatMessage::system(format!(
                        "TTS input is {chars} chars; cap is {} (split client-side).",
                        crate::modality::TTS_INPUT_MAX_CHARS_CLIENT,
                    )));
                    return;
                }
            }
            // ASR: whisper requires an audio attachment. Without one
            // the server returns 400 anyway, but pre-flight saves
            // the network round-trip and keeps the user's typed
            // context (e.g. an initial_prompt or commentary) intact
            // for them to add the attachment + retry.
            crate::modality::ModelModality::AudioAsr => {
                if self.chat.attached_images.is_empty() {
                    self.chat.messages.push_back(ChatMessage::system(
                        "Whisper requires an audio attachment. \
                         Use the Attach button to add a \
                         WAV / MP3 / FLAC / OGG / M4A / AAC file.",
                    ));
                    return;
                }
            }
            _ => {}
        }

        // Push to prompt history (dedupe + cap + cursor reset) via
        // the testable helper on ChatState.
        self.chat.push_prompt_history(prompt.clone());
        self.chat.input.clear();
        // Single helper for the start-of-generation invariants —
        // mirrors reset_streaming_state at the end. See the
        // prepare_for_generation docstring for the field list.
        self.chat.prepare_for_generation();

        // Clear the shared streaming buffer. Poisoning is tolerated: the protected
        // String carries no invariants, so a worker that panicked mid-write leaves
        // it merely stale, not unsafe.
        if let Ok(mut buf) = self.streaming_text.lock() {
            buf.clear();
        }

        // Take attached images for this message. The helper moves
        // the base64 vec out (forwarded to the HTTP worker without
        // an extra clone) and clears the parallel paths vec in
        // lockstep — keeps the documented `attached_images.len() ==
        // attached_image_paths.len()` invariant intact across the
        // send.
        let attached_images = self.chat.take_attachments();
        // Release the input-chip GPU textures keyed by attach_<hash>_<i>.
        // The chat-bubble copies that survive are keyed by msg_<hash>_<i>
        // so they re-decode on first paint after this send — independent
        // cache namespace. Without this retain(), every send leaks one
        // texture handle per attachment (chip preview is gone but the
        // cache still holds it until the next file is attached or Clear
        // fires). For users who send + don't re-attach, the leak
        // accumulates across the session.
        crate::texture::clear_attach_chip_textures(&mut self.image_textures);

        // Add user message
        self.chat.messages.push_back(ChatMessage {
            role: "user".to_string(),
            content: prompt.clone(),
            timestamp: crate::timefmt::chat_now(),
            timing: None,
            images: attached_images.clone(),
            generated_images: Vec::new(),
            generated_audios: Vec::new(),
        });

        let client = self.get_client();
        let tasks = self.pending_tasks.clone();
        let streaming_text = self.streaming_text.clone();
        let egui_ctx = ctx.clone();

        // Get parameters from current profile
        let mut options = self.get_chat_options();

        // Map LayerMode to request options via a pure helper so the
        // mapping has a unit-test surface (regression guard against
        // re-introducing dead options the server doesn't read).
        apply_layer_mode_options(&mut options, self.chat.layer_mode);

        // Image-gen re-roll: if the user clicked "Lock seed" on a
        // previous gen's badge, thread that seed into options.seed
        // so the server picks the same starting noise. The lock is
        // single-shot — cleared the moment we send, so a tweaked
        // prompt automatically falls back to a fresh seed on the
        // next gen unless re-locked.
        if let Some(seed) = self.chat.locked_seed.take() {
            insert_option_field(&mut options, "seed", serde_json::json!(seed));
        }

        // Image-gen overrides: only thread when the loaded model is
        // actually image-gen. text-gen uses `num_predict`, not
        // `num_steps`, so sending an irrelevant key wouldn't break
        // anything but adds noise to the request body and could
        // confuse a logger / proxy reading the wire. Gating prevents
        // a stale sticky override from a previous image-gen session
        // leaking into a follow-up text chat.
        //
        // Reuse `modality_for_send` instead of re-running the
        // ModelModality::from_model_name classifier — same value.
        if matches!(modality_for_send, crate::modality::ModelModality::ImageGen) {
            // num_steps: sticky GUI preference. Cloned (not taken)
            // so successive sends in the same chat continue to use
            // it without the user re-setting per send.
            if let Some(steps) = self.chat.image_num_steps {
                insert_option_field(&mut options, "num_steps", serde_json::json!(steps));
            }
            // img2img strength override: only meaningful when the
            // user has attached a source image, but the sticky
            // semantics match — we thread it unconditionally and the
            // server only consults strength on the img2img code path
            // (sentinel field input_image is set).
            if let Some(strength) = self.chat.image_strength {
                insert_option_field(&mut options, "strength", serde_json::json!(strength));
            }
            // Dimensions override. Server consults options.width /
            // options.height; absence falls back to image_model_defaults
            // (Flux 512², Z-Image 1024²).
            if let Some((w, h)) = self.chat.image_size {
                insert_option_field(&mut options, "width", serde_json::json!(w));
                insert_option_field(&mut options, "height", serde_json::json!(h));
            }
        }

        // TTS chat overrides — thread voice + speed when the loaded
        // model is AudioTts. Same modality-gating pattern as
        // image-gen overrides so a stale TTS preference from a
        // previous chat doesn't leak into a follow-up text/image
        // request.
        if matches!(modality_for_send, crate::modality::ModelModality::AudioTts,) {
            if let Some(ref voice) = self.chat.tts_voice {
                insert_option_field(&mut options, "voice", serde_json::json!(voice));
            }
            if let Some(speed) = self.chat.tts_speed {
                insert_option_field(&mut options, "speed", serde_json::json!(speed));
            }
        }

        let handle = self.rt.spawn(async move {
            // Build message with optional images for vision models
            let message = if attached_images.is_empty() {
                Message::new("user".to_string(), prompt)
            } else {
                Message::with_images("user".to_string(), prompt, attached_images)
            };
            let mut request = OllamaChatRequest::new(model, vec![message]);
            request.stream = true;
            request.options = options;

            let result = match client.chat_stream(&request).await {
                Ok(mut response) => {
                    let mut accumulated = String::new();
                    // The reasoning a thinking model streams apart from its answer.
                    let mut thinking_acc = String::new();
                    let mut line_buf = String::new();
                    let mut timing_info: Option<crate::state::MessageTiming> = None;
                    let mut generated_images: Vec<String> = Vec::new();
                    let mut generated_audios: Vec<String> = Vec::new();
                    let mut is_image_gen = false;

                    loop {
                        match response.chunk().await {
                            Ok(Some(chunk)) => {
                                // Append raw bytes to line buffer
                                let text = String::from_utf8_lossy(&chunk);
                                line_buf.push_str(&text);

                                // Process complete lines
                                let mut done = false;
                                while let Some(newline_pos) = line_buf.find('\n') {
                                    // Copy the line out, then drop the line
                                    // + its trailing newline from line_buf
                                    // via in-place drain. The previous code
                                    // did `line_buf = line_buf[pos+1..].
                                    // to_string()` which re-allocated the
                                    // remaining buffer on every line — an
                                    // O(suffix_len) full copy per line.
                                    // drain shifts the suffix in place
                                    // without an extra allocation.
                                    let line: String = line_buf[..newline_pos].trim().to_string();
                                    line_buf.drain(..=newline_pos);

                                    if line.is_empty() {
                                        continue;
                                    }

                                    // Strip SSE "data: " prefix if present (our server uses SSE format)
                                    let json_str = line
                                        .strip_prefix("data: ")
                                        .or_else(|| line.strip_prefix("data:"))
                                        .unwrap_or(&line);

                                    // Skip SSE comments and empty data
                                    if json_str.is_empty() || json_str.starts_with(':') {
                                        continue;
                                    }

                                    // Try parsing as chat response first, then as generic JSON (for image gen)
                                    if let Ok(mut chunk_resp) =
                                        serde_json::from_str::<OllamaChatResponse>(json_str)
                                    {
                                        // TTS path: server's handle_chat_tts returns
                                        // a single non-streaming response with
                                        // message.audios populated. Pull them into
                                        // generated_audios so the chat tab renders
                                        // Play/Save buttons (audio_playback::play_audio_blob).
                                        //
                                        // `.take()` moves the Vec<String> out of
                                        // chunk_resp instead of borrowing + cloning
                                        // each entry. Each entry is a base64 WAV
                                        // (~640 KB for a typical 10-second TTS clip)
                                        // so cloning was substantial wasted work for
                                        // a value chunk_resp drops at end of scope
                                        // anyway.
                                        if let Some(audios) = chunk_resp.message.audios.take() {
                                            for a in audios {
                                                if !a.is_empty() {
                                                    generated_audios.push(a);
                                                }
                                            }
                                        }
                                        let thought =
                                            chunk_resp.thinking.as_deref().unwrap_or_default();
                                        if !chunk_resp.message.content.is_empty()
                                            || !thought.is_empty()
                                        {
                                            thinking_acc.push_str(thought);
                                            accumulated.push_str(&chunk_resp.message.content);
                                            // The shared buffer carries the thought ahead of the
                                            // answer, in the one form the chat reads.
                                            if let Ok(mut buf) = streaming_text.lock() {
                                                *buf = crate::modality::with_thinking(
                                                    &thinking_acc,
                                                    &accumulated,
                                                );
                                            }
                                            egui_ctx.request_repaint();
                                        }
                                        if chunk_resp.done {
                                            // Capture timing from final response
                                            if let (Some(eval_count), Some(eval_duration)) =
                                                (chunk_resp.eval_count, chunk_resp.eval_duration)
                                            {
                                                let duration_ms = eval_duration / 1_000_000;
                                                let duration_sec =
                                                    eval_duration as f32 / 1_000_000_000.0;
                                                let tokens_per_sec = if duration_sec > 0.0 {
                                                    eval_count as f32 / duration_sec
                                                } else {
                                                    0.0
                                                };
                                                timing_info = Some(crate::state::MessageTiming {
                                                    tokens_per_sec,
                                                    duration_ms,
                                                    token_count: eval_count as u32,
                                                });
                                            }
                                            done = true;
                                            break;
                                        }
                                    } else if let Ok(mut val) =
                                        serde_json::from_str::<serde_json::Value>(json_str)
                                    {
                                        // Image generation response (generate format, not chat)
                                        // Progress update: {"completed": N, "total": M, "done": false}
                                        if let (Some(completed), Some(total)) = (
                                            val.get("completed")
                                                .and_then(serde_json::Value::as_u64),
                                            val.get("total").and_then(serde_json::Value::as_u64),
                                        ) {
                                            is_image_gen = true;
                                            // Synthesise the streaming_text via the
                                            // shared formatter — parse_image_step_progress
                                            // (chat_tab.rs) parses this exact format, so
                                            // round-tripping through the helper keeps
                                            // synthesiser and parser locked together.
                                            let progress_text =
                                                crate::modality::format_image_step_progress(
                                                    completed, total,
                                                );
                                            if let Ok(mut buf) = streaming_text.lock() {
                                                *buf = progress_text;
                                            }
                                            egui_ctx.request_repaint();
                                        }
                                        // Loading status: {"response": "Loading T5...", "done": false}
                                        else if let Some(resp) =
                                            val.get("response").and_then(|v| v.as_str())
                                        {
                                            if !resp.is_empty() {
                                                is_image_gen = true;
                                                if let Ok(mut buf) = streaming_text.lock() {
                                                    *buf = resp.to_string();
                                                }
                                                egui_ctx.request_repaint();
                                            }
                                        }
                                        // Final image: {"done": true, "images": [...]}
                                        if val
                                            .get("done")
                                            .and_then(serde_json::Value::as_bool)
                                            .unwrap_or(false)
                                        {
                                            // Drain the images array out of `val`
                                            // and pattern-match Value::String to
                                            // move the inner String instead of
                                            // cloning each. Each base64-encoded
                                            // PNG is ~500 KB for 512² / ~2 MB for
                                            // 1024², so per-image clones added up
                                            // for batched gen responses.
                                            if let Some(images) =
                                                val.get_mut("images").and_then(|v| v.as_array_mut())
                                            {
                                                is_image_gen = true;
                                                for img in images.drain(..) {
                                                    if let serde_json::Value::String(s) = img {
                                                        generated_images.push(s);
                                                    }
                                                }
                                            }
                                            done = true;
                                            break;
                                        }
                                        // Error
                                        if let Some(err) = val.get("error").and_then(|v| v.as_str())
                                        {
                                            accumulated = format!("Error: {}", err);
                                            done = true;
                                            break;
                                        }
                                    } else {
                                        // Cap the logged chunk so a misbehaving
                                        // server that sends repeated unparseable
                                        // multi-KB chunks doesn't flood the in-app
                                        // Server Log buffer (max_entries = 1000
                                        // would fill in seconds at 60Hz streaming).
                                        // 240 chars is enough to see the chunk's
                                        // shape (typically Ollama-NDJSON keys) for
                                        // debugging without dumping the response
                                        // body verbatim.
                                        //
                                        // Cheap byte-length check first — `>720` is
                                        // the maximum byte budget for 240 Unicode
                                        // scalars (each at most 4 bytes in UTF-8).
                                        // If under that, the full chunk fits inside
                                        // the cap so we skip the `.chars().take(240)`
                                        // collect entirely; otherwise we do exactly
                                        // one pass (no second `.chars().count()`).
                                        const MAX_CHARS: usize = 240;
                                        if json_str.len() <= MAX_CHARS * 4 {
                                            warn!("Failed to parse streaming chunk: {}", json_str);
                                        } else {
                                            let preview: String =
                                                json_str.chars().take(MAX_CHARS).collect();
                                            warn!(
                                                "Failed to parse streaming chunk (truncated, \
                                                 {} bytes total): {}",
                                                json_str.len(),
                                                preview,
                                            );
                                        }
                                    }
                                }
                                if done {
                                    break;
                                }
                            }
                            Ok(None) => {
                                // Stream ended
                                break;
                            }
                            Err(e) => {
                                // Network error during streaming
                                if accumulated.is_empty() {
                                    tasks.lock().unwrap().push(TaskResult::ChatResponse(Err(
                                        format!("Stream error: {}", e),
                                    )));
                                    return;
                                }
                                // If we have partial content, use it
                                break;
                            }
                        }
                    }

                    // Process any remaining data in the line buffer
                    let remaining = line_buf.trim().to_string();
                    if !remaining.is_empty() {
                        // Strip SSE prefix if present
                        let json_str = remaining
                            .strip_prefix("data: ")
                            .or_else(|| remaining.strip_prefix("data:"))
                            .unwrap_or(&remaining);
                        if let Ok(mut chunk_resp) =
                            serde_json::from_str::<OllamaChatResponse>(json_str)
                        {
                            // TTS audio capture, mirroring the in-loop
                            // parser above. A non-streaming TTS response
                            // arrives here (single JSON object with no
                            // trailing newline) — without this branch the
                            // audios payload was silently dropped and the
                            // chat tab's Play / Save audio buttons never
                            // rendered because msg.generated_audios was
                            // empty.
                            if let Some(audios) = chunk_resp.message.audios.take() {
                                for a in audios {
                                    if !a.is_empty() {
                                        generated_audios.push(a);
                                    }
                                }
                            }
                            if let Some(thought) = chunk_resp.thinking.as_deref() {
                                thinking_acc.push_str(thought);
                            }
                            if !chunk_resp.message.content.is_empty() {
                                accumulated.push_str(&chunk_resp.message.content);
                            }
                            // Capture timing if not already captured
                            if timing_info.is_none() {
                                if let (Some(eval_count), Some(eval_duration)) =
                                    (chunk_resp.eval_count, chunk_resp.eval_duration)
                                {
                                    let duration_ms = eval_duration / 1_000_000;
                                    let duration_sec = eval_duration as f32 / 1_000_000_000.0;
                                    let tokens_per_sec = if duration_sec > 0.0 {
                                        eval_count as f32 / duration_sec
                                    } else {
                                        0.0
                                    };
                                    timing_info = Some(crate::state::MessageTiming {
                                        tokens_per_sec,
                                        duration_ms,
                                        token_count: eval_count as u32,
                                    });
                                }
                            }
                        }
                    }

                    if is_image_gen && !generated_images.is_empty() {
                        // Image generation completed — return images
                        return tasks
                            .lock()
                            .unwrap()
                            .push(TaskResult::ImageGenResponse(Ok(generated_images)));
                    }
                    // Image-gen format detected but no images produced —
                    // surface as an explicit error rather than falling
                    // through to the chat path (which would emit an empty
                    // assistant bubble after the cb7531d empty-content
                    // guard suppressed the redundant text).
                    if is_image_gen {
                        let msg = if accumulated.is_empty() {
                            "Image generation produced no images".to_string()
                        } else {
                            format!(
                                "Image generation produced no images. Last status: {}",
                                accumulated
                            )
                        };
                        return tasks
                            .lock()
                            .unwrap()
                            .push(TaskResult::ImageGenResponse(Err(msg)));
                    }

                    // Post-process: strip instruction/template tags that the model may generate
                    // Same cleanup as the non-streaming generate_quantized() path
                    for stop_tag in &["[INST]", "[/INST]", "[SYSTEM_PROMPT]", "</s>"] {
                        if let Some(pos) = accumulated.find(stop_tag) {
                            accumulated.truncate(pos);
                        }
                    }
                    let accumulated =
                        crate::modality::with_thinking(thinking_acc.trim(), accumulated.trim());

                    Ok((accumulated, timing_info, generated_audios))
                }
                Err(e) => Err(e.to_string()),
            };

            tasks.lock().unwrap().push(TaskResult::ChatResponse(result));
        });
        // Store the abort handle so the Stop button can cancel the
        // in-flight task. abort_handle() is cheap (clones an Arc).
        // Cleared in process_pending_tasks once the task completes
        // (which produces a TaskResult::ChatResponse) so the GUI
        // doesn't accidentally re-abort a finished task.
        self.chat.generation_abort = Some(handle.abort_handle());
    }

    fn get_chat_options(&self) -> Option<serde_json::Value> {
        if let Some(ref profile_name) = self.config.selected_profile {
            if let Some(profile) = self
                .config
                .profiles
                .iter()
                .find(|p| p.name == *profile_name)
            {
                return match profile.api_type {
                    ApiType::Ollama => {
                        let p = &profile.ollama_params;
                        let mut o = serde_json::json!({
                            "temperature": p.temperature,
                            "top_p": p.top_p,
                            "top_k": p.top_k,
                            "repeat_penalty": p.repeat_penalty,
                            "num_ctx": p.num_ctx,
                            "num_predict": p.num_predict,
                        });
                        // seed only when pinned (>=0); -1 means "random each request".
                        if p.seed >= 0 {
                            o["seed"] = serde_json::json!(p.seed);
                        }
                        // stop: comma-separated → array; skip when empty.
                        let stops: Vec<String> = p
                            .stop
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if !stops.is_empty() {
                            o["stop"] = serde_json::json!(stops);
                        }
                        Some(o)
                    }
                    ApiType::Loken => Some(serde_json::json!({
                        "temperature": profile.loken_params.temperature,
                        "num_ctx": profile.loken_params.context_length,
                    })),
                    ApiType::OpenApi => Some(serde_json::json!({
                        "temperature": profile.openapi_params.temperature,
                        "max_tokens": profile.openapi_params.max_tokens,
                        "top_p": profile.openapi_params.top_p,
                        "frequency_penalty": profile.openapi_params.frequency_penalty,
                        "presence_penalty": profile.openapi_params.presence_penalty,
                    })),
                };
            }
        }
        None
    }

    pub fn process_pending_tasks(&mut self) {
        // UI gate watchdog: a Models-tab action whose task never reported back
        // (server wedged or restarted mid-request) must not disable Load / Unload
        // / Delete forever - which is exactly what happened tonight. Any gate older
        // than this deadline is cleared, so the tab always recovers by itself.
        // Runs before the fast-path return: a stuck gate has no pending task.
        // The deadline must outlast a LEGITIMATE load (a 13 GB checkpoint takes
        // minutes), so it only catches gates that will never resolve.
        const GATE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(300);
        match (
            self.models.action_started_at,
            self.models.action_status.is_in_progress(),
        ) {
            (Some(t0), true) if t0.elapsed() > GATE_DEADLINE => {
                self.models.action_status = ActionStatus::Failed(
                    "the previous action never reported back (server restarted?)".to_string(),
                );
                self.models.action_started_at = None;
            }
            // A gate with NO timestamp cannot expire - stamp it so it does. Covers
            // any path that sets the status without going through the helpers.
            (None, true) => self.models.action_started_at = Some(std::time::Instant::now()),
            (Some(_), false) => self.models.action_started_at = None,
            _ => {}
        }

        // Fast path: skip the drain + Vec allocation when there's
        // nothing pending. process_pending_tasks fires every frame
        // (called from update()), and 99%+ of frames have zero
        // queued task results — the previous unconditional
        // `tasks.drain(..).collect()` allocated an empty Vec on
        // every one of those frames just to discard it.
        let mut tasks = self.pending_tasks.lock().unwrap();
        if tasks.is_empty() {
            return;
        }
        let tasks_to_process: Vec<_> = tasks.drain(..).collect();
        drop(tasks);

        for task in tasks_to_process {
            match task {
                TaskResult::LorasFetched(names) => {
                    // Drop any selection the server no longer offers, so a request can
                    // never carry a name that would come back as a 400.
                    self.media
                        .image
                        .loras
                        .retain(|(n, _)| names.iter().any(|(a, _)| a == n));
                    self.media
                        .image_edit
                        .loras
                        .retain(|(n, _)| names.iter().any(|(a, _)| a == n));
                    self.available_loras = names;
                }
                TaskResult::ModelsFetched(available, loaded) => {
                    info!(
                        "Fetched {} available models, {} loaded",
                        available.len(),
                        loaded.len()
                    );
                    self.models.available_models = available;
                    self.models.loaded_models = loaded;
                    self.models.select_best_model();
                    self.connection_status = ConnectionStatus::connected(format!(
                        "Connected - {} models, {} loaded",
                        self.models.available_models.len(),
                        self.models.loaded_models.len()
                    ));
                    if self.models.action_status.is_in_progress() {
                        self.models.action_status = ActionStatus::Idle;
                    }
                    // Confirm an explicit user-initiated refresh (top-bar
                    // or Models toolbar) with a toast. Gated on the flag
                    // so background / startup / post-action refreshes stay
                    // silent.
                    if self.refresh_toast_pending {
                        self.refresh_toast_pending = false;
                        self.toast(
                            ToastSeverity::Success,
                            format!(
                                "Refreshed — {} models, {} loaded",
                                self.models.available_models.len(),
                                self.models.loaded_models.len()
                            ),
                        );
                    }
                }
                TaskResult::ModelLoaded(result, model_name) => match result {
                    Ok(resp) => {
                        info!("Model '{}' loaded successfully", resp.model);
                        if !self.models.is_loaded(&resp.model) {
                            self.models.loaded_models.push(resp.model.clone());
                        }
                        self.models.selected_model = Some(resp.model.clone());
                        // Save model selection to config for auto-loading on next startup
                        self.config.selected_model = Some(resp.model.clone());
                        // Try to find the source from available models
                        if let Some(model_info) = self
                            .models
                            .available_models
                            .iter()
                            .find(|m| m.name == resp.model)
                        {
                            self.config.selected_model_source = Some(model_info.source.clone());
                        }
                        self.config.save();
                        // Toast covers the outcome; drop the stale-persistent
                        // Success banner (set Idle) so it doesn't linger on
                        // the Models tab until the next action.
                        self.models.action_status = ActionStatus::Idle;
                        self.toast(
                            ToastSeverity::Success,
                            format!("Model {} loaded", model_name),
                        );
                        // Refresh hardware topology after model load
                    }
                    Err(e) => {
                        error!("Failed to load model '{}': {}", model_name, e);
                        self.models.action_status = ActionStatus::Idle;
                        self.toast(
                            ToastSeverity::Error,
                            format!("Failed to load {}: {}", model_name, e),
                        );
                    }
                },
                TaskResult::ModelDeleted(result, model_name) => match result {
                    Ok(()) => {
                        self.models.action_status = ActionStatus::Idle;
                        self.toast(
                            ToastSeverity::Success,
                            format!("Model {} deleted", model_name),
                        );
                        // If the deleted model was the active selection,
                        // clear it so the chat header doesn't keep
                        // displaying a model the server no longer has.
                        // Otherwise the next Send would fail at /api/chat
                        // with a model-not-found error and the user
                        // would wonder why a "loaded"-looking name 404s.
                        if self.models.selected_model.as_deref() == Some(model_name.as_str()) {
                            self.models.selected_model = None;
                            self.config.selected_model = None;
                            self.config.selected_model_source = None;
                            self.config.save();
                        }
                        // Drop the same name from the loaded list eagerly
                        // — refresh_models will re-fetch but in the
                        // intervening few hundred ms the UI shouldn't
                        // display the deleted model as still loaded.
                        self.models.loaded_models.retain(|m| m != &model_name);
                        self.refresh_models();
                    }
                    Err(e) => {
                        self.models.action_status = ActionStatus::Idle;
                        self.toast(
                            ToastSeverity::Error,
                            format!("Failed to delete {}: {}", model_name, e),
                        );
                    }
                },
                TaskResult::PullProgress(completed, total) => {
                    // Latest byte-count for the in-flight download; the
                    // Models tab renders a determinate bar once total > 0.
                    if self.models.action_status.is_in_progress() {
                        self.models.pull_progress = Some((completed, total));
                    }
                }
                TaskResult::ModelPulled(result, model_name) => {
                    self.models.pull_progress = None;
                    match result {
                        Ok(()) => {
                            self.models.pull_model_input.clear();
                            self.models.action_status = ActionStatus::Idle;
                            self.toast(
                                ToastSeverity::Success,
                                format!("Model {} pulled successfully", model_name),
                            );
                            self.refresh_models();
                        }
                        Err(e) => {
                            self.models.action_status = ActionStatus::Idle;
                            self.toast(
                                ToastSeverity::Error,
                                format!("Failed to pull {}: {}", model_name, e),
                            );
                        }
                    }
                }
                TaskResult::ChatResponse(result) => {
                    // Six-field reset (is_generating, abort handle,
                    // streaming_content, image_gen_progress, started_at)
                    // lives in ChatState::reset_streaming_state so the
                    // ChatResponse + ImageGenResponse handlers + the
                    // user-cancel path (abort_generation) all share one
                    // implementation. Without this, e.g. a chat error
                    // mid-image-gen-stream could leave the progress bar
                    // stuck because each handler had its own list of
                    // fields to clear.
                    self.chat.reset_streaming_state();
                    if let Ok(mut buf) = self.streaming_text.lock() {
                        buf.clear();
                    }
                    match result {
                        Ok((content, timing_info, generated_audios)) => {
                            self.chat.messages.push_back(ChatMessage {
                                role: "assistant".to_string(),
                                content,
                                timestamp: crate::timefmt::chat_now(),
                                timing: timing_info,
                                generated_audios,
                                ..Default::default()
                            });
                        }
                        Err(e) => {
                            self.chat
                                .messages
                                .push_back(ChatMessage::system(format!("Error: {}", e)));
                        }
                    }
                }
                TaskResult::ImageGenResponse(result) => {
                    // Shared cleanup with ChatResponse — see
                    // reset_streaming_state's docstring.
                    self.chat.reset_streaming_state();
                    if let Ok(mut buf) = self.streaming_text.lock() {
                        buf.clear();
                    }
                    match result {
                        Ok(images) => {
                            // Leave content empty for image-gen responses —
                            // the rendered image grid + the 'N images'
                            // toolbar (from 8efc723) already conveys count.
                            // Empty content also suppresses the Copy-text
                            // button (76a37c7) which would have copied a
                            // useless 'Generated 4 image(s)' string.
                            self.chat.messages.push_back(ChatMessage {
                                role: "assistant".to_string(),
                                timestamp: crate::timefmt::chat_now(),
                                generated_images: images,
                                ..Default::default()
                            });
                        }
                        Err(e) => {
                            self.chat.messages.push_back(ChatMessage::system(format!(
                                "Image generation error: {}",
                                e
                            )));
                        }
                    }
                }
                TaskResult::ConversationResult(result) => {
                    self.chat.is_generating = false;
                    match result {
                        Ok(out) => {
                            self.chat.messages.push_back(ChatMessage {
                                role: "assistant".to_string(),
                                content: out.content,
                                timestamp: crate::timefmt::chat_now(),
                                timing: None,
                                images: Vec::new(),
                                generated_images: out.image.into_iter().collect(),
                                generated_audios: out.audio.into_iter().collect(),
                            });
                            self.toast(
                                ToastSeverity::Success,
                                format!(
                                    "Auto: {} via {} ({})",
                                    out.route,
                                    if out.model.is_empty() {
                                        "server default"
                                    } else {
                                        &out.model
                                    },
                                    out.routed_by
                                ),
                            );
                        }
                        Err(e) => {
                            self.chat.messages.push_back(ChatMessage::system(format!(
                                "Smart routing failed: {e}"
                            )));
                        }
                    }
                }
                TaskResult::MediaResult(result) => {
                    self.media.finish_generation();
                    match result {
                        Ok(out) => {
                            self.media.result_images = out.images;
                            self.media.result_audios = out.audios;
                            self.media.result_files = out.files;
                            self.media.result_text = out.text;
                            self.media.status = if out.status.is_empty() {
                                "Done.".to_string()
                            } else {
                                out.status
                            };
                            self.media.error = None;
                        }
                        Err(e) => {
                            error!("Media generation failed: {}", e);
                            self.media.error = Some(e);
                            self.media.status.clear();
                        }
                    }
                }
                TaskResult::MediaPromptEnhanced(result) => {
                    self.media.enhancing_prompt = false;
                    match result {
                        Ok(text) => {
                            self.media.prompt_before_enhance =
                                Some(std::mem::take(&mut self.media.prompt));
                            self.media.prompt = text;
                        }
                        Err(e) => self.toast(
                            ToastSeverity::Error,
                            format!("Prompt enhancement failed: {e}"),
                        ),
                    }
                }
                TaskResult::MediaPartial(out) => {
                    // Show finished variations immediately; the run is still live.
                    if self.media.is_generating {
                        self.media.result_images = out.images;
                        self.media.result_audios = out.audios;
                        self.media.result_files = out.files;
                        if !out.status.is_empty() {
                            self.media.status = out.status;
                        }
                    }
                }
                TaskResult::MediaProgress(label, step, total, node) => {
                    // Only meaningful while a generation is in flight;
                    // late events after completion are ignored.
                    if self.media.is_generating {
                        // A phase with nothing to count clears the bar rather than
                        // freezing it at the last step of the previous phase, which
                        // read as progress that had stalled.
                        self.media.progress = (total > 0).then_some((step, total));
                        self.media.status =
                            media_progress_status(&label, step, total, node.as_deref());
                    }
                }
                TaskResult::MediaVoicesFetched(voices) => {
                    self.media.speech.voices = voices;
                }
                TaskResult::VideoEstimate { key, seconds } => {
                    self.media.estimate_in_flight = false;
                    // Only if it still describes what is on screen. Answers can land out of
                    // order, and an old one overwriting a newer one would leave the tab
                    // showing the cost of settings the user has already moved past.
                    if self.media.estimate_key.as_deref() == Some(key.as_str()) {
                        self.media.video_estimate = seconds;
                        if seconds.is_none() {
                            // Nothing came back. Let it be asked again rather than leaving
                            // the tab permanently blank, after a pause that keeps a server
                            // which is not answering from being asked every frame.
                            const RETRY_AFTER: std::time::Duration =
                                std::time::Duration::from_secs(3);
                            self.media.estimate_key = None;
                            self.media.estimate_retry_after =
                                Some(std::time::Instant::now() + RETRY_AFTER);
                        }
                    }
                }
                TaskResult::Error(e) => {
                    error!("{}", e);
                    // Surface the failure on the connection status so
                    // the top-bar dot flips from Connecting → red and
                    // the tooltip reflects the actual reason instead
                    // of being stuck at "Connecting..." until the
                    // next refresh attempt. Failures that come through
                    // this path are typically network errors against
                    // /api/tags or /api/ps, which exactly maps to
                    // "can't talk to the server".
                    self.connection_status =
                        ConnectionStatus::disconnected(format!("Disconnected: {}", e));
                    // Surface it as a toast, so the failure is visible from whichever
                    // tab the user is on rather than only from Models. Idle clears the
                    // banner so the two do not both linger.
                    self.models.action_status = ActionStatus::Idle;
                    self.toast(ToastSeverity::Error, e);
                }
                TaskResult::CLILoadModel(result, model_name, _command) => {
                    match result {
                        Ok(resp) => {
                            if !self.models.is_loaded(&resp.model) {
                                self.models.loaded_models.push(resp.model.clone());
                            }
                            self.models.selected_model = Some(resp.model.clone());
                            self.models.action_status = ActionStatus::Idle;
                            // Update the last output with success
                            if let Some(last) = self.cli.outputs.last_mut() {
                                last.output = format!("Model {} loaded successfully", model_name);
                                last.in_progress = false;
                            }
                            // Refresh hardware topology after model load
                        }
                        Err(e) => {
                            self.models.action_status = ActionStatus::Idle;
                            // Update the last output with error
                            if let Some(last) = self.cli.outputs.last_mut() {
                                last.output = format!("Failed to load {}: {}", model_name, e);
                                last.is_error = true;
                                last.in_progress = false;
                            }
                        }
                    }
                }
                TaskResult::CLIPullModel(result, model_name, _command) => {
                    match result {
                        Ok(()) => {
                            self.models.pull_model_input.clear();
                            self.models.action_status = ActionStatus::Idle;
                            // Update the last output with success
                            if let Some(last) = self.cli.outputs.last_mut() {
                                last.output = format!("Model {} pulled successfully", model_name);
                                last.in_progress = false;
                            }
                            self.refresh_models();
                            // Refresh hardware topology after model pull
                        }
                        Err(e) => {
                            self.models.action_status = ActionStatus::Idle;
                            // Update the last output with error
                            if let Some(last) = self.cli.outputs.last_mut() {
                                last.output = format!("Failed to pull {}: {}", model_name, e);
                                last.is_error = true;
                                last.in_progress = false;
                            }
                        }
                    }
                }
                TaskResult::CLIDeleteModel(result, model_name, _command) => {
                    match result {
                        Ok(()) => {
                            self.models.action_status = ActionStatus::Idle;
                            // Update the last output with success
                            if let Some(last) = self.cli.outputs.last_mut() {
                                last.output = format!("Model {} deleted successfully", model_name);
                                last.in_progress = false;
                            }
                            self.refresh_models();
                            // Refresh hardware topology after model delete
                        }
                        Err(e) => {
                            self.models.action_status = ActionStatus::Idle;
                            // Update the last output with error
                            if let Some(last) = self.cli.outputs.last_mut() {
                                last.output = format!("Failed to delete {}: {}", model_name, e);
                                last.is_error = true;
                                last.in_progress = false;
                            }
                        }
                    }
                }
                TaskResult::ModelUnloaded(result, model_name) => {
                    match result {
                        Ok(()) => {
                            // Remove from local loaded list
                            self.models.loaded_models.retain(|m| m != &model_name);
                            self.models.action_status = ActionStatus::Idle;
                            self.toast(
                                ToastSeverity::Success,
                                format!("Model {} unloaded", model_name),
                            );
                            // Refresh hardware topology after model unload
                        }
                        Err(e) => {
                            self.models.action_status = ActionStatus::Idle;
                            self.toast(
                                ToastSeverity::Error,
                                format!("Failed to unload {}: {}", model_name, e),
                            );
                        }
                    }
                }
            }
        }
    }

    pub fn execute_cli_command(&mut self) {
        let input = self.cli.input.trim().to_string();
        if input.is_empty() {
            return;
        }

        self.cli.input.clear();

        // Add to history via the testable helper (dedupe + cap +
        // index reset).
        self.cli.push_history(input.clone());

        use crate::cli_tab::CLICommand;
        let command = CLICommand::parse(&input);

        match command {
            CLICommand::Clear => {
                self.cli.outputs.clear();
            }
            CLICommand::List => {
                use crate::modality::ModelModality;
                let models_info: Vec<String> = self
                    .models
                    .available_models
                    .iter()
                    .map(|m| {
                        let modality = ModelModality::from_model_name(&m.name);
                        // Print the modality tag only when non-Text so
                        // CLI output stays tight for plain LLMs; matches
                        // the badge-suppression policy in the GUI list.
                        if modality != ModelModality::Text {
                            format!("  {} ({}) [{}]", m.name, m.size, modality.label())
                        } else {
                            format!("  {} ({})", m.name, m.size)
                        }
                    })
                    .collect();
                let output = if models_info.is_empty() {
                    "No models available.".to_string()
                } else {
                    format!("Available models:\n{}", models_info.join("\n"))
                };
                self.cli.push_output(CLIOutput {
                    timestamp: crate::timefmt::cli_now(),
                    command: input,
                    output,
                    ..Default::default()
                });
            }
            CLICommand::Loaded => {
                use crate::modality::ModelModality;
                let output = if self.models.loaded_models.is_empty() {
                    "No models currently loaded.".to_string()
                } else {
                    format!(
                        "Loaded models:\n{}",
                        self.models
                            .loaded_models
                            .iter()
                            .map(|m| {
                                // Same modality-tag policy as `list`: tag
                                // only non-Text models so the line stays
                                // tight for plain LLMs.
                                let modality = ModelModality::from_model_name(m);
                                if modality != ModelModality::Text {
                                    format!("  {} [{}]", m, modality.label())
                                } else {
                                    format!("  {}", m)
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                };
                self.cli.push_output(CLIOutput {
                    timestamp: crate::timefmt::cli_now(),
                    command: input,
                    output,
                    ..Default::default()
                });
            }
            CLICommand::Load(model_name) => {
                if self.models.action_status.is_in_progress() {
                    self.cli.push_output(CLIOutput {
                        timestamp: crate::timefmt::cli_now(),
                        command: input,
                        output: "Another operation is in progress. Please wait.".to_string(),
                        is_error: true,
                        ..Default::default()
                    });
                } else {
                    let client = self.get_client();
                    let tasks = self.pending_tasks.clone();
                    let model = model_name.clone();
                    let is_ollama = self.config.server_url.contains(":11434");
                    let model_for_result = model_name.clone();

                    self.models.action_status =
                        ActionStatus::InProgress(format!("Loading {}...", model_name));

                    // Add in-progress output
                    self.cli.push_output(CLIOutput {
                        timestamp: crate::timefmt::cli_now(),
                        command: input,
                        output: format!("Loading model: {}...", model_name),
                        in_progress: true,
                        ..Default::default()
                    });

                    self.rt.spawn(async move {
                        let result = if is_ollama {
                            let request = OllamaChatRequest::new(
                                model.clone(),
                                vec![Message::new("system".into(), "".into())],
                            );
                            client
                                .chat(&request)
                                .await
                                .map(|_| LoadModelResponse {
                                    model: model.clone(),
                                    status: "loaded".into(),
                                    message: format!("Model {} ready", model),
                                })
                                .map_err(|e| e.to_string())
                        } else {
                            client.load_model(&model).await.map_err(|e| e.to_string())
                        };
                        tasks.lock().unwrap().push(TaskResult::CLILoadModel(
                            result,
                            model_for_result,
                            "load".to_string(),
                        ));
                    });
                }
            }
            CLICommand::Unload(model_name) => {
                if self.models.is_loaded(&model_name) {
                    self.unload_model(model_name.clone());
                    self.cli.push_output(CLIOutput {
                        timestamp: crate::timefmt::cli_now(),
                        command: input,
                        output: format!("Model {} unloaded.", model_name),
                        ..Default::default()
                    });
                    // Refresh hardware topology after model unload
                } else {
                    self.cli.push_output(CLIOutput {
                        timestamp: crate::timefmt::cli_now(),
                        command: input,
                        output: format!("Model {} is not loaded.", model_name),
                        is_error: true,
                        ..Default::default()
                    });
                }
            }
            CLICommand::Pull(model_name) => {
                if self.models.action_status.is_in_progress() {
                    self.cli.push_output(CLIOutput {
                        timestamp: crate::timefmt::cli_now(),
                        command: input,
                        output: "Another operation is in progress. Please wait.".to_string(),
                        is_error: true,
                        ..Default::default()
                    });
                } else {
                    let client = self.get_client();
                    let tasks = self.pending_tasks.clone();
                    let model = model_name.clone();
                    let model_for_result = model_name.clone();

                    self.models.action_status =
                        ActionStatus::InProgress(format!("Pulling {}...", model_name));

                    // Add in-progress output
                    self.cli.push_output(CLIOutput {
                        timestamp: crate::timefmt::cli_now(),
                        command: input,
                        output: format!("Pulling model: {}...", model_name),
                        in_progress: true,
                        ..Default::default()
                    });

                    self.rt.spawn(async move {
                        // CLI `pull` defaults to the Ollama registry —
                        // route through pull_model_with_source for
                        // parity with the Models-tab Pull path so
                        // there's one code path to the server.
                        let result = match client.pull_model_with_source(&model, "ollama").await {
                            Ok(_) => Ok(()),
                            Err(e) => Err(e.to_string()),
                        };
                        tasks.lock().unwrap().push(TaskResult::CLIPullModel(
                            result,
                            model_for_result,
                            "pull".to_string(),
                        ));
                    });
                }
            }
            CLICommand::Delete(model_name) => {
                if self.models.action_status.is_in_progress() {
                    self.cli.push_output(CLIOutput {
                        timestamp: crate::timefmt::cli_now(),
                        command: input,
                        output: "Another operation is in progress. Please wait.".to_string(),
                        is_error: true,
                        ..Default::default()
                    });
                } else {
                    let client = self.get_client();
                    let tasks = self.pending_tasks.clone();
                    let model = model_name.clone();
                    let model_for_result = model_name.clone();

                    self.models.action_status =
                        ActionStatus::InProgress(format!("Deleting {}...", model_name));

                    // Add in-progress output
                    self.cli.push_output(CLIOutput {
                        timestamp: crate::timefmt::cli_now(),
                        command: input,
                        output: format!("Deleting model: {}...", model_name),
                        in_progress: true,
                        ..Default::default()
                    });

                    self.rt.spawn(async move {
                        let result = client.delete_model(&model).await;
                        tasks.lock().unwrap().push(TaskResult::CLIDeleteModel(
                            result.map_err(|e| e.to_string()),
                            model_for_result,
                            "delete".to_string(),
                        ));
                    });
                }
            }
            CLICommand::Ps => {
                let mut status_lines = vec!["Server Status:".to_string()];

                match &self.server.status {
                    ServerStatus::NotStarted => {
                        status_lines.push("  Status: Not started (client mode)".to_string());
                    }
                    ServerStatus::Starting => {
                        status_lines.push("  Status: Starting...".to_string());
                    }
                    ServerStatus::Running { port } => {
                        status_lines.push(format!("  Status: Running on port {}", port));
                        status_lines.push(format!("  URL: http://localhost:{}", port));
                    }
                    ServerStatus::Failed(e) => {
                        status_lines.push(format!("  Status: Failed - {}", e));
                    }
                }

                status_lines.push("".to_string());
                status_lines.push("Connection:".to_string());
                status_lines.push(format!("  Server URL: {}", self.config.server_url));
                status_lines.push(format!("  Status: {}", self.connection_status));

                status_lines.push("".to_string());
                status_lines.push("Models:".to_string());
                status_lines.push(format!(
                    "  Available: {}",
                    self.models.available_models.len()
                ));
                status_lines.push(format!("  Loaded: {}", self.models.loaded_models.len()));

                if let Some(ref selected) = self.models.selected_model {
                    use crate::modality::ModelModality;
                    let modality = ModelModality::from_model_name(selected);
                    if modality != ModelModality::Text {
                        status_lines.push(format!(
                            "  Selected: {} [{}]",
                            selected,
                            modality.label()
                        ));
                    } else {
                        status_lines.push(format!("  Selected: {}", selected));
                    }
                }

                self.cli.push_output(CLIOutput {
                    timestamp: crate::timefmt::cli_now(),
                    command: input,
                    output: status_lines.join("\n"),
                    ..Default::default()
                });
            }
            CLICommand::Help => {
                let help_text = r#"Available commands:
  list              - List available models
  loaded            - List loaded models
  load <model>      - Load a model into memory
  unload <model>    - Unload a model from memory
  pull <model>      - Pull/download a model
  delete <model>    - Delete a model
  ps                - Show server and connection status
  clear             - Clear the output
  help              - Show this help message

Keyboard:
  Enter             - Run command
  Up / Down         - Recall past commands"#;
                self.cli.push_output(CLIOutput {
                    timestamp: crate::timefmt::cli_now(),
                    command: input,
                    output: help_text.to_string(),
                    ..Default::default()
                });
            }
            CLICommand::Unknown(msg) => {
                self.cli.push_output(CLIOutput {
                    timestamp: crate::timefmt::cli_now(),
                    command: input,
                    output: msg,
                    is_error: true,
                    ..Default::default()
                });
            }
        }
    }
}

/// Margin between the floor's edge and the panels standing on it.
const FLOOR_MARGIN: i8 = 12;

impl eframe::App for LLMGuiApp {
    /// Flush the in-memory config to disk before the GUI exits.
    /// The 1 s rate-limit on the per-frame window-size save (commit
    /// a26d63d) means the very last drag end may not have reached
    /// disk yet — and the user closing the window right after
    /// releasing the resize handle would lose those final
    /// dimensions on the next launch. on_exit fires synchronously
    /// before the process tears down so a final config.save()
    /// catches whatever's still in-memory.
    ///
    /// Best-effort: errors are surfaced via the existing
    /// config.save tracing path (see commit 6ed8b3e). The
    /// `_gl` argument is the glow context that eframe hands in
    /// for graphics-API cleanup; we don't touch GPU state here.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.config.media = Some(self.media.to_persist());
        self.config.save();
    }

    // egui 0.34 deprecates `CentralPanel::show(&Context)` in favour of
    // `show_inside(&mut Ui)`, but the new form requires a parent Ui that
    // doesn't exist at the eframe::App::update entry. See ui/layout.rs.
    // The entry point hands us the root Ui, so panels show INSIDE it rather than on
    // the Context. A cloned `ctx` is kept - it is a cheap Arc - for the repaint,
    // input and theme calls that still take one.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Advance video playback on the UI clock. egui reports the frame time, so the
        // player accumulates REAL elapsed seconds rather than counting repaints - a
        // clip must run at its own rate whatever the display does. Only a playing clip
        // asks for the next repaint, so an idle player costs nothing.
        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.25);
        if self.video.tick(&ctx, dt) {
            ctx.request_repaint();
        }
        // Process async tasks
        self.process_pending_tasks();

        // Sync streaming text from async task to chat state.
        // clone_from reuses the destination's allocation when capacity
        // is enough — pairs with the worker-side optimisation in
        // 7de6815 so a long streaming response goes through zero new
        // String allocations after the first chunk grows the buffer.
        if self.chat.is_generating {
            if let Ok(buf) = self.streaming_text.lock() {
                if !buf.is_empty() && *buf != self.chat.streaming_content {
                    self.chat.streaming_content.clone_from(&*buf);
                    // Detect image gen progress: "Step X/Y". On the
                    // first observed step, capture the wall-clock
                    // start so the progress bar can show an ETA based
                    // on mean step duration so far. Parser lives in
                    // chat_tab so the wire format is unit-tested.
                    if let Some((completed, total)) =
                        crate::modality::parse_image_step_progress(&buf)
                    {
                        self.chat.image_gen_progress = Some((completed, total));
                        // Anchor the ETA clock on the FIRST real step
                        // completion (completed >= 1), not the synthetic
                        // 0/total "starting" event the server now emits
                        // before step 1 (commit 275dc5a). Anchoring on
                        // the 0/total event would fold the cold-cache
                        // / VAE-prep latency into per-step duration and
                        // bias the ETA estimate high for the rest of
                        // the run.
                        if completed >= 1 && self.chat.image_gen_started_at.is_none() {
                            self.chat.image_gen_started_at = Some(std::time::Instant::now());
                        }
                    }
                }
            }
            // Repaint cadence depends on whether the user is *looking*
            // at the chat. When on the Chat tab they need full frame
            // rate for smooth token-by-token streaming. When on a
            // different tab (Hardware, Settings, etc.) the streaming
            // content is invisible — burning 60 FPS to repaint hidden
            // text wastes CPU and battery. Throttle to ~200ms there;
            // that's still snappy enough that re-entering the Chat
            // tab feels instant (one tick lag at most) and the
            // is_generating → false transition (TaskResult delivery)
            // surfaces within 200ms of the request completing.
            if self.current_section == Section::Chat {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }

        // Keep hardware refreshing for live layer performance data.
        // Re-fetch /api/inflight + topology on a 2s cadence while the
        // tab is visible so the scheduler panel reflects requests
        // that arrive mid-session without requiring a manual click.
        // 2s is a balance: tighter is wasted RPS, looser lags behind
        // request bursts (which often complete in < 1s).

        // Force repaints while any tokio task is in flight. The async
        // workers (refresh_models, load/unload/delete, pull, list-
        // loaded, etc.) push their results into self.pending_tasks but
        // don't directly signal egui — without an active repaint the
        // GUI sleeps in its event loop and the result sits in the
        // queue until the user moves the mouse. Same problem the
        // worker-dialog threads had (fixed in 80bac8e); same fix
        // applied here at the engine level so every async path
        // benefits without needing per-spawn ctx threading.
        //
        // Throttled to 100 ms so we're not spinning at full frame rate
        // for the duration of a long pull / load.
        if self.models.action_status.is_in_progress() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Keep repainting while a Media Studio generation is in flight
        // so the progress bar / status / streamed SSE steps advance and
        // the final result surfaces without waiting for a mouse move.
        // Full rate when the Studio tab is visible; throttled otherwise
        // (the progress is invisible on other tabs).
        if self.media.is_generating {
            if self.current_section == Section::MediaStudio {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }

        // ── GLOBAL KEYBOARD NAV ──
        // Ctrl/Cmd + 1..7 jump straight to a section, in the same order
        // as the sidebar NAV_ITEMS. `modifiers.command` is Ctrl on
        // Windows/Linux and Cmd on macOS. The command modifier keeps
        // these from colliding with the chat input's bare Enter/Up/Down/
        // Esc shortcuts, so no text-focus guard is needed.
        let section_hotkey = ctx.input(|i| {
            if !i.modifiers.command {
                return None;
            }
            use egui::Key::*;
            for (key, section) in [
                (Num1, Section::Chat),
                (Num2, Section::Terminal),
                (Num3, Section::Models),
                (Num4, Section::Settings),
                (Num5, Section::MediaStudio),
                (Num6, Section::ServerLog),
            ] {
                if i.key_pressed(key) {
                    return Some(section);
                }
            }
            None
        });
        if let Some(section) = section_hotkey {
            self.current_section = section;
        }

        // ── TOP BAR ──
        let active_model = self.models.selected_model.as_deref();
        let server_running = matches!(self.server.status, ServerStatus::Running { .. });

        // The top-bar Refresh reflects an in-flight refresh: refresh_models
        // sets action_status to InProgress("Refreshing…") for the duration
        // of the /api/tags + /api/ps round-trip, so observing it gives the
        // button a spinner + disabled state without a second flag.
        let refreshing = self.models.action_status.is_in_progress();
        // A reply's image steps or a render's steps, as a fraction for the
        // meter in the top bar's tail.
        let progress = self
            .chat
            .image_gen_progress
            .map(|(done, total)| done as f32 / total.max(1) as f32)
            .or_else(|| {
                self.media
                    .progress
                    .map(|(step, total)| step as f32 / total.max(1) as f32)
            });
        let top_out = crate::ui::layout::top_bar(
            ui,
            &crate::ui::layout::TopBarInput {
                connection: self.connection_status.state,
                connection_detail: &self.connection_status.detail,
                active_model,
                loaded: self.models.loaded_models.len(),
                server_running,
                refreshing,
                progress,
            },
        );
        if top_out.refresh_clicked {
            // Arm the completion toast for this explicit refresh.
            self.refresh_toast_pending = true;
            self.refresh_models();
        }
        if top_out.theme_toggle_clicked {
            // Toggle dark ↔ light and re-apply the matching egui
            // visuals so the swap takes effect on the next frame
            // (matches the Settings-tab theme picker's behaviour).
            self.config.dark_theme = !self.config.dark_theme;
            theme::apply(&ctx, self.config.dark_theme);
            self.config.save();
        }

        // ── LEFT SIDEBAR ──
        let sidebar_toggled =
            crate::ui::layout::sidebar(ui, &mut self.current_section, self.sidebar_expanded);
        if sidebar_toggled {
            // Flip the runtime flag and mirror it into the persisted
            // config so the collapsed/expanded choice survives restart.
            self.sidebar_expanded = !self.sidebar_expanded;
            self.config.sidebar_expanded = self.sidebar_expanded;
            self.config.save();
        }

        // ── MAIN CONTENT AREA ──
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg())
                    .inner_margin(FLOOR_MARGIN),
            )
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                match self.current_section {
                    Section::Chat => {
                        let crate::chat_tab::ChatRenderOutput {
                            send_clicked,
                            modality: modality_for_gate,
                        } = crate::chat_tab::render(
                            ui,
                            &mut self.chat,
                            &mut self.models,
                            self.config.selected_profile.as_ref(),
                            &self.config.profiles,
                            &mut self.markdown_cache,
                            &mut self.image_textures,
                        );

                        // Persist a dropdown-driven model change so it
                        // survives restarts. Mirrors the persistence done in
                        // TaskResult::ModelLoaded (load via Models tab) — both
                        // entry points end at the same config layout, so a
                        // user who picks via the chat header doesn't get a
                        // surprise revert on the next session.
                        //
                        // Compare models.selected_model against config rather
                        // than against a pre-render clone: in steady state the
                        // two are in sync (TaskResult::ModelLoaded + this very
                        // block keep them so), and the only path that diverges
                        // them is the chat-header dropdown firing inside
                        // chat_tab::render this frame. Dropping the per-frame
                        // Option<String> clone shaves one allocation off every
                        // Chat-section paint.
                        if self.models.selected_model != self.config.selected_model {
                            self.config.selected_model = self.models.selected_model.clone();
                            // Look up the source ("ollama"/"huggingface") so
                            // auto-load on next startup hits the right
                            // backend. Skip when the new selection is None
                            // (a deselect — keep source as None too).
                            self.config.selected_model_source =
                                self.models.selected_model.as_ref().and_then(|name| {
                                    self.models
                                        .available_models
                                        .iter()
                                        .find(|m| &m.name == name)
                                        .map(|m| m.source.clone())
                                });
                            self.config.save();
                        }

                        // Persist a layer-mode toggle the same way. Cheap
                        // Copy-type equality check, save only on change
                        // — so a steady-state idle render doesn't churn
                        // the config file every frame.
                        if self.chat.layer_mode != self.config.layer_mode {
                            self.config.layer_mode = self.chat.layer_mode;
                            self.config.save();
                        }
                        if self.chat.smart_auto != self.config.chat_smart_auto {
                            self.config.chat_smart_auto = self.chat.smart_auto;
                            self.config.save();
                        }

                        // Handle chat send — either Enter or Send-button click
                        // route through the same send_chat path. Without OR'ing
                        // in the button click, the rendered Send button looked
                        // active but did nothing.
                        // Send on Enter (without Shift). Shift+Enter is the
                        // multi-line newline shortcut configured on the
                        // TextEdit via return_key — the gate here prevents
                        // a Shift+Enter from double-firing as both newline-
                        // insert AND chat-send.
                        //
                        // input_empty reads &self.chat.input directly (was
                        // a per-frame clone() of a potentially long prompt).
                        // The clone wasn't needed — the value is only used
                        // for the bool gate below, and that read can be
                        // done without owning the String.
                        let is_generating = self.chat.is_generating;
                        let enter_pressed =
                            ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                        // Match the Send-button gate in chat_tab exactly:
                        // both surfaces go through chat_send_allowed for
                        // the modality / input / attachment rules, then
                        // layer on model_selected + !is_generating. The
                        // previous inline `!input.trim().is_empty()` was
                        // an over-approximation that broke ASR: an ASR
                        // user with an attached audio file but empty text
                        // could click Send (button allows it via
                        // chat_send_allowed → has_attachment) but NOT
                        // press Enter (Enter required non-empty text).
                        // Inconsistent escape hatch fixed here.
                        // modality_for_gate came back from chat_tab::render
                        // above — no need to re-walk ModelModality::from_model_name
                        // here just to gate the same Enter / Send-button send
                        // path the chat-tab UI already gates on.
                        let input_empty = self.chat.input.trim().is_empty();
                        let has_attachment = !self.chat.attached_images.is_empty();
                        let model_selected = self.models.selected_model.is_some();
                        let should_send = (enter_pressed || send_clicked)
                            && model_selected
                            && !is_generating
                            && crate::modality::chat_send_allowed(
                                modality_for_gate,
                                input_empty,
                                has_attachment,
                            );

                        // Prompt-history navigation via Ctrl+Up / Ctrl+Down.
                        // Plain Up/Down stays bound to in-text cursor movement
                        // (the chat input is multi-line so users still need
                        // it to navigate paragraphs in long pasted prompts).
                        // Ctrl-modified versions recall the previous / next
                        // prompt from chat.prompt_history. State mutation
                        // lives in ChatState::{history_back, history_forward}
                        // so the logic is unit-tested independently of egui.
                        let (ctrl_up, ctrl_down) = ui.input(|i| {
                            (
                                i.modifiers.ctrl && i.key_pressed(egui::Key::ArrowUp),
                                i.modifiers.ctrl && i.key_pressed(egui::Key::ArrowDown),
                            )
                        });
                        if !is_generating {
                            if ctrl_up {
                                self.chat.history_back();
                            } else if ctrl_down {
                                self.chat.history_forward();
                            }
                        }

                        // Esc cancels the in-flight generation. Mirrors the
                        // Stop button — same abort + system-message path.
                        // Gate on is_generating so plain Esc still propagates
                        // to modal close-handlers when no chat is running.
                        if is_generating {
                            let esc_pressed = ui
                                .input(|i| i.key_pressed(egui::Key::Escape) && !i.modifiers.any());
                            if esc_pressed && self.chat.abort_generation() {
                                self.chat
                                    .messages
                                    .push_back(crate::state::ChatMessage::system(
                                        "[Generation cancelled by user]",
                                    ));
                            }
                        }

                        // Ctrl+L clears the chat (terminal-style "clear screen"
                        // convention). Mirrors the Clear button in the chat
                        // header — same conversation wipe + texture/markdown
                        // cache cleanup. Gated on !is_generating because mid-
                        // stream clears leave the streaming buffer orphaned.
                        if !is_generating {
                            let ctrl_l =
                                ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::L));
                            if ctrl_l
                                && (!self.chat.messages.is_empty()
                                    || !self.chat.attached_images.is_empty())
                            {
                                self.chat.clear_conversation();
                                self.image_textures.clear();
                                self.markdown_cache.clear_scrollable();
                            }
                        }

                        if should_send {
                            self.send_chat(&ctx);
                        }
                    }
                    Section::Terminal => {
                        crate::cli_tab::render(ui, &mut self.cli, &self.models);

                        // Handle CLI command. Plain Up / Down for history
                        // navigation (single-line input — no conflict with
                        // in-text cursor movement). Mirrors the chat tab's
                        // Ctrl+Up / Ctrl+Down history nav (but unmodified
                        // here since the CLI input doesn't need arrows for
                        // line nav).
                        //
                        // Read `.trim().is_empty()` outside the closure so it captures a
                        // bool rather than the input String. Capturing the String costs an
                        // allocation on every frame the CLI tab is visible.
                        let input_empty = self.cli.input.trim().is_empty();
                        let (enter, up, down) = ui.input(|i| {
                            (
                                i.key_pressed(egui::Key::Enter) && !input_empty,
                                i.key_pressed(egui::Key::ArrowUp),
                                i.key_pressed(egui::Key::ArrowDown),
                            )
                        });
                        if enter {
                            self.execute_cli_command();
                        } else if up {
                            self.cli.history_back();
                        } else if down {
                            self.cli.history_forward();
                        }
                    }
                    Section::Models => {
                        let actions = crate::ui::models::render(ui, &mut self.models);
                        for action in actions {
                            match action {
                                SettingsAction::RefreshModels => self.refresh_models(),
                                SettingsAction::LoadModel(model) => self.load_model(model),
                                SettingsAction::UnloadModel(model) => self.unload_model(model),
                                SettingsAction::DeleteModel(model) => self.delete_model(model),
                                SettingsAction::PullModel(model, source) => {
                                    self.pull_model_with_source(&model, &source)
                                }
                                _ => {}
                            }
                        }
                    }
                    Section::Settings => {
                        let config_before = self.config.clone();
                        let actions = crate::ui::settings::render(
                            &ctx,
                            ui,
                            &mut self.config,
                            &mut self.settings_state,
                            self.connection_status.state,
                        );
                        for action in actions {
                            if let SettingsAction::SaveConfig(cfg) = action {
                                match cfg.save_default() {
                                    Ok(()) => {
                                        info!("Configuration saved");
                                        // Clear any prior error banner now
                                        // that a save succeeded.
                                        if let Some(editor) =
                                            self.settings_state.config_editor.as_mut()
                                        {
                                            editor.error_message = None;
                                        }
                                        self.refresh_models();
                                    }
                                    Err(e) => {
                                        // Surface save failure (disk full,
                                        // read-only mount, permission
                                        // denied) in the editor error
                                        // banner instead of just logging
                                        // to stderr where the user never
                                        // sees it. The Settings tab's
                                        // banner picks it up from
                                        // editor.error_message; a toast
                                        // makes it visible from any tab too.
                                        let msg = format!("Failed to save config.toml: {}", e);
                                        error!("{}", msg);
                                        self.toast(ToastSeverity::Error, msg.clone());
                                        if let Some(editor) =
                                            self.settings_state.config_editor.as_mut()
                                        {
                                            editor.error_message = Some(msg);
                                        }
                                    }
                                }
                            }
                        }
                        if config_before != self.config {
                            self.config.save();
                            // If the user changed the server URL, the
                            // existing models / hardware / connection
                            // status all point at the OLD server. Without
                            // an auto-refresh the user has to remember
                            // to bounce to the Models tab and click
                            // Refresh — that's a confusing dead state
                            // ("I changed servers, why is everything the
                            // same?"). Refresh against the new URL the
                            // moment we detect the change so the rest
                            // of the GUI catches up.
                            if config_before.server_url != self.config.server_url {
                                self.connection_status = ConnectionStatus::connecting();
                                self.toast(
                                    ToastSeverity::Success,
                                    format!("Connecting to {}", self.config.server_url),
                                );
                                self.refresh_models();
                            }
                        }
                    }
                    Section::ServerLog => {
                        crate::server_log_tab::render(
                            ui,
                            &mut self.server,
                            self.embedded_port,
                            &self.log_buffer,
                        );
                    }
                    Section::MediaStudio => {
                        let out = crate::media_tab::render(
                            ui,
                            &mut self.media,
                            &mut self.image_textures,
                            &self.models.available_models,
                            &self.available_loras,
                            &mut self.audio_player,
                            &mut self.video,
                        );
                        if out.generate_clicked {
                            self.send_media(&ctx);
                        }
                        // Price the settings on screen, so the wait is known before it is
                        // started rather than after. Only for the kind that can cost an hour;
                        // the others answer in seconds and would just be traffic.
                        if self.media.kind == crate::state::MediaKind::Video {
                            self.refresh_video_estimate(&ctx);
                        }
                        // Tell the SERVER to stop. Dropping the request only ends this side of
                        // it: the render carries on to completion otherwise, holding the cards.
                        if let Some(id) = self.media.pending_cancel.take() {
                            let client = self.get_client();
                            self.rt.spawn(async move {
                                let _ = client.cancel_render(&id).await;
                            });
                        }
                        if out.refresh_voices_clicked {
                            self.refresh_media_voices(&ctx);
                        }
                        if out.enhance_clicked {
                            self.enhance_media_prompt(&ctx);
                        }
                    }
                }
            });

        // ── TOASTS ──
        // Rendered after the CentralPanel so the bottom-right stack
        // floats above all tab content. Handles its own expiry +
        // repaint scheduling.
        crate::toast::render(&ctx, &mut self.toasts);

        // Fullscreen image viewer (zoom/pan). Rendered last so its modal
        // backdrop covers all tab content and toasts. Opens when any image
        // rendered via image_viewer::clickable_image is clicked.
        crate::image_viewer::show(&ctx, &mut self.fullscreen_image);

        // Sync window size into config so a resize doesn't get
        // lost if the user closes the GUI before touching any
        // other setting. Previously the window size updated
        // in-memory here but only reached disk if some OTHER
        // config-diff path fired (theme toggle, profile pick,
        // etc.) — a user who resized and quit got the default
        // 1000×750 back on next launch.
        //
        // Rate-limited to one save per second so a sustained
        // resize-drag doesn't write the config 60×/sec (each drag
        // frame ticks the rect by 1+ px). The final dimensions
        // still land within ~1 s of mouse-up — a brief lag the
        // user doesn't see because the GUI doesn't close that
        // fast post-release.
        let (rect, maximized) = ctx.input(|i| {
            let v = i.viewport();
            (v.inner_rect, v.maximized)
        });
        // Track maximize toggles alongside resize: without this the
        // user un-maximises, quits, and the next launch still
        // re-maximises because window_maximized stayed Some(true)
        // from the previous session.
        if let Some(new_max) = maximized {
            if self.config.window_maximized != Some(new_max) {
                self.config.window_maximized = Some(new_max);
                // Save immediately — maximize toggles are discrete
                // events, not the per-pixel drag firehose the
                // resize path defends against.
                self.config.save();
            }
        }
        if let Some(rect) = rect {
            let new_w = rect.width();
            let new_h = rect.height();
            let old_w = self.config.window_width.unwrap_or(0.0);
            let old_h = self.config.window_height.unwrap_or(0.0);
            // Update the in-memory config every frame so a
            // subsequent config.save() triggered by another path
            // (theme toggle, model pick) captures the latest size
            // even within the 1 s throttle window.
            if (new_w - old_w).abs() >= 1.0 || (new_h - old_h).abs() >= 1.0 {
                self.config.window_width = Some(new_w);
                self.config.window_height = Some(new_h);
                let should_save = self
                    .last_window_size_save
                    .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(1));
                if should_save {
                    self.config.save();
                    self.last_window_size_save = Some(std::time::Instant::now());
                }
            }
        }

        // Profile Edit Modal (still needed for settings)
        crate::settings::render_edit_modal(&ctx, &mut self.config, &mut self.settings_state);

        // Note: render_config_editor_modal is intentionally NOT called
        // here. The server-configuration editor renders INLINE inside
        // the Settings panel (ui::settings::render reads
        // settings_state.config_editor for that purpose). Wiring the
        // modal on top duplicated the UI — every entry to Settings
        // opened the same editor twice.
    }
}

/// Translate `LayerMode` into the `options` JSON that the server
/// actually understands. `Adaptive` adds `early_exit_threshold: 0.1`;
/// `AllLayers` is a no-op (the engine default).
///
/// Lives outside `LLMGuiApp` so it has a unit-test surface — see
/// `mod tests::apply_layer_mode_options_*`. A previous third variant
/// ("CudaOnly") sent `cuda_only: true` here even though no request
/// handler ever read it; that mistake is the reason this helper now
/// has explicit tests.
fn apply_layer_mode_options(
    options: &mut Option<serde_json::Value>,
    mode: crate::state::LayerMode,
) {
    if !matches!(mode, crate::state::LayerMode::Adaptive) {
        return;
    }
    insert_option_field(options, "early_exit_threshold", serde_json::json!(0.1));
}

/// Insert `value` at `key` in the chat-request `options` object.
///
/// Used by `send_chat` to thread image-gen / TTS overrides into the
/// request body. Replaces the duplicated 5-line idiom:
///
///   if let Some(ref mut opts) = options {
///       if let Some(obj) = opts.as_object_mut() {
///           obj.insert(key.to_string(), value);
///       }
///   } else {
///       *options = Some(serde_json::json!({ key: value }));
///   }
///
/// — which had ~6 copies in `send_chat` alone (seed, num_steps,
/// strength, width+height, voice, speed) plus the
/// `apply_layer_mode_options` helper above.
fn insert_option_field(
    options: &mut Option<serde_json::Value>,
    key: &str,
    value: serde_json::Value,
) {
    match options {
        Some(opts) => {
            if let Some(obj) = opts.as_object_mut() {
                obj.insert(key.to_string(), value);
            }
        }
        None => {
            let mut map = serde_json::Map::new();
            map.insert(key.to_string(), value);
            *options = Some(serde_json::Value::Object(map));
        }
    }
}

/// Configure fonts to support Unicode emoji and icons
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::LayerMode;
    use serde_json::json;

    // -- Media Studio: speech over the event stream -----------------
    //
    // The events below are TRANSCRIBED FROM THE WIRE (a Piper synthesis on
    // /v1/audio/speech with "stream_format": "sse"), not invented from the handler's
    // source, so a change of shape on the server shows up here as a failure.

    #[test]
    fn a_load_phase_with_no_count_states_the_phase_rather_than_zero_of_zero() {
        // The exact defect: a Piper voice is one .onnx file, so its loader has nothing to
        // count and the event reads step 0 of 0. Printing "0/0" claims a progress the
        // server never made, beside a bar that cannot move - and this is the phase the
        // user waits through longest on a cold engine.
        let ev = json!({
            "status": "loading", "phase": "load-model",
            "phase_label": "Loading the model", "step": 0, "total": 0, "elapsed_ms": 0
        });
        let (label, step, total, node) = progress_from_event(&ev);
        assert_eq!((step, total), (0, 0));
        assert_eq!(node, None);
        assert_eq!(
            media_progress_status(&label, step, total, None),
            "Loading the model"
        );
    }

    #[test]
    fn a_counted_phase_says_how_far_through_it_is() {
        let ev = json!({
            "status": "synthesizing", "phase": "synthesize",
            "phase_label": "Synthesising the speech", "step": 1, "total": 2,
            "elapsed_ms": 965
        });
        let (label, step, total, _) = progress_from_event(&ev);
        assert_eq!(
            media_progress_status(&label, step, total, None),
            "Synthesising the speech 1/2"
        );
    }

    #[test]
    fn a_phase_the_client_has_never_heard_of_is_still_shown_by_name() {
        // The label is the server's, not a table here: a phase added there must read
        // correctly without a GUI change.
        let ev = json!({"status": "loading", "phase": "warm-cache", "step": 3, "total": 9});
        let (label, step, total, _) = progress_from_event(&ev);
        // No phase_label on this one - the raw phase name stands in rather than nothing.
        assert_eq!(
            media_progress_status(&label, step, total, None),
            "warm-cache 3/9"
        );
    }

    #[test]
    fn a_render_handed_over_says_which_node_has_it() {
        // Sent to one node, rendered by another: the events name the one that renders.
        let ev = json!({
            "status": "rendering", "phase": "denoise", "phase_label": "Denoising",
            "step": 17, "total": 40, "elapsed_ms": 9000, "node": "desktop"
        });
        let (label, step, total, node) = progress_from_event(&ev);
        assert_eq!(
            media_progress_status(&label, step, total, node.as_deref()),
            "Denoising 17/40 on desktop"
        );
        // A server that runs alone says null, and the status says nothing of where.
        let alone = json!({"status": "rendering", "phase": "denoise", "step": 1, "total": 4, "node": null});
        assert_eq!(progress_from_event(&alone).3, None);
    }

    #[test]
    fn an_event_with_no_label_at_all_still_reads_as_activity() {
        assert_eq!(media_progress_status("", 4, 20, None), "Rendering step 4/20");
        assert_eq!(media_progress_status("", 0, 0, None), "Working");
    }

    #[test]
    fn cancelling_a_synthesis_has_a_name_to_send() {
        // Speech names itself `render_id` and prefixes with `s`; the media routes say
        // `id` and prefix with `r`. Reading only `id` is what left Cancel with nothing to
        // POST for a synthesis - it dropped this end of the wire and the server spoke on.
        let started = json!({
            "status": "started", "model": "piper", "render_id": "s1", "format": "wav"
        });
        assert_eq!(render_name(&started), Some("s1"));
        assert_eq!(
            render_name(&json!({"status": "started", "id": "r7"})),
            Some("r7")
        );
        // A progress event carries no name, and must not erase the one already held.
        assert_eq!(render_name(&json!({"status": "loading", "step": 0})), None);
    }

    #[test]
    fn an_event_stream_is_recognised_by_its_content_type() {
        assert!(speech_reply_is_events(Some("text/event-stream")));
        assert!(speech_reply_is_events(Some(
            "text/event-stream; charset=utf-8"
        )));
        assert!(speech_reply_is_events(Some("TEXT/EVENT-STREAM")));
    }

    #[test]
    fn a_server_that_ignores_the_field_answers_with_audio_and_is_believed() {
        // Nothing on that route rejects unknown fields, so a build predating
        // `stream_format` returns 200 with a body of audio. Feeding that to an SSE parser
        // reports "stream ended without a result" for a clip that arrived intact, which
        // is the user losing TTS to a server they have not restarted yet.
        assert!(!speech_reply_is_events(Some("audio/wav")));
        assert!(!speech_reply_is_events(Some(
            "audio/L16; rate=22050; channels=1"
        )));
        assert!(!speech_reply_is_events(None));
    }

    #[test]
    fn a_refused_field_is_retried_without_it() {
        // A server that REFUSES the field rather than ignoring it answers 4xx. The user
        // must still get their speech, so the same body goes back without it.
        for status in [
            "400 Bad Request",
            "422 Unprocessable Entity",
            "404 Not Found",
        ] {
            let e = format!("/v1/audio/speech failed with status {status}: unknown field");
            assert!(speech_retry_without_events(&e), "{status} should retry");
        }
    }

    #[test]
    fn a_failure_that_is_not_about_the_request_shape_is_not_spent_twice() {
        // A 5xx or a dead connection would meet the same wall on the plain path: retrying
        // only doubles the wait before showing the same error.
        for e in [
            "/v1/audio/speech failed with status 500 Internal Server Error: tts synth: oom",
            "/v1/audio/speech failed with status 503 Service Unavailable: ",
            "error sending request for url (http://localhost:11435/v1/audio/speech)",
            "",
        ] {
            assert!(!speech_retry_without_events(e), "{e} should not retry");
        }
    }

    // ── Media Studio: media_data_to_output routing ─────────────────

    fn datum(b64: &str, ct: Option<&str>) -> crate::api::types::MediaDatum {
        crate::api::types::MediaDatum {
            b64_json: b64.to_string(),
            content_type: ct.map(str::to_string),
            sample_rate: None,
        }
    }

    #[test]
    fn media_router_sends_wav_to_audios_by_default() {
        // A datum with no content_type (or audio/wav) is music/SFX audio
        // — must land in `audios` so the Play/Save buttons render.
        let out = media_data_to_output(
            vec![datum("QUJD", None), datum("REVG", Some("audio/wav"))],
            "ok".into(),
        );
        assert_eq!(out.audios.len(), 2);
        assert!(out.images.is_empty() && out.files.is_empty());
        assert_eq!(out.status, "ok");
    }

    #[test]
    fn media_router_sends_image_content_type_to_images() {
        let out = media_data_to_output(vec![datum("QUJD", Some("image/png"))], "i".into());
        assert_eq!(out.images.len(), 1);
        assert!(out.audios.is_empty() && out.files.is_empty());
    }

    #[test]
    fn media_router_decodes_midi_and_video_to_files_with_ext() {
        // "QUJD" is base64 for "ABC" — verify it's decoded to bytes and
        // given the right extension per content_type.
        let out = media_data_to_output(
            vec![
                datum("QUJD", Some("audio/midi")),
                datum("QUJD", Some("video/mp4")),
                datum("QUJD", Some("image/gif")),
            ],
            "f".into(),
        );
        assert_eq!(out.files.len(), 3);
        assert!(out.files[0].0.ends_with(".mid"));
        assert!(out.files[1].0.ends_with(".mp4"));
        assert!(out.files[2].0.ends_with(".gif"));
        // Decoded, not left as base64.
        assert_eq!(out.files[0].1, b"ABC");
    }

    #[test]
    fn apply_layer_mode_options_all_layers_leaves_none_alone() {
        // AllLayers must NOT lazily allocate an empty options object —
        // the chat path treats `None` as "use server defaults" and
        // returning Some({}) would override a no-op with an empty
        // object, which downstream Ollama-shape mergers may treat
        // differently.
        let mut opts: Option<serde_json::Value> = None;
        apply_layer_mode_options(&mut opts, LayerMode::AllLayers);
        assert!(opts.is_none(), "AllLayers should not allocate options");
    }

    #[test]
    fn apply_layer_mode_options_all_layers_leaves_existing_object_alone() {
        // AllLayers must not strip or mutate options the caller has
        // already put in. (Otherwise toggling away from Adaptive
        // mid-session could drop temperature/top_p/etc.)
        let mut opts = Some(json!({"temperature": 0.7, "seed": 42}));
        apply_layer_mode_options(&mut opts, LayerMode::AllLayers);
        let obj = opts.as_ref().and_then(|v| v.as_object()).unwrap();
        assert_eq!(obj.get("temperature"), Some(&json!(0.7)));
        assert_eq!(obj.get("seed"), Some(&json!(42)));
        assert!(!obj.contains_key("early_exit_threshold"));
        assert!(
            !obj.contains_key("cuda_only"),
            "cuda_only must never appear — server never read it"
        );
    }

    #[test]
    fn apply_layer_mode_options_adaptive_creates_options_when_absent() {
        let mut opts: Option<serde_json::Value> = None;
        apply_layer_mode_options(&mut opts, LayerMode::Adaptive);
        let obj = opts.as_ref().and_then(|v| v.as_object()).unwrap();
        assert_eq!(obj.get("early_exit_threshold"), Some(&json!(0.1)));
    }

    #[test]
    fn apply_layer_mode_options_adaptive_merges_into_existing_options() {
        let mut opts = Some(json!({"temperature": 0.7, "seed": 42}));
        apply_layer_mode_options(&mut opts, LayerMode::Adaptive);
        let obj = opts.as_ref().and_then(|v| v.as_object()).unwrap();
        // Original values preserved.
        assert_eq!(obj.get("temperature"), Some(&json!(0.7)));
        assert_eq!(obj.get("seed"), Some(&json!(42)));
        // New option added.
        assert_eq!(obj.get("early_exit_threshold"), Some(&json!(0.1)));
    }

    #[test]
    fn insert_option_field_creates_object_when_options_none() {
        let mut opts: Option<serde_json::Value> = None;
        insert_option_field(&mut opts, "seed", json!(42));
        let obj = opts.as_ref().and_then(|v| v.as_object()).unwrap();
        assert_eq!(obj.get("seed"), Some(&json!(42)));
        assert_eq!(
            obj.len(),
            1,
            "freshly-allocated options should contain only the inserted key"
        );
    }

    #[test]
    fn insert_option_field_merges_into_existing_object() {
        let mut opts = Some(json!({"temperature": 0.7}));
        insert_option_field(&mut opts, "seed", json!(42));
        let obj = opts.as_ref().and_then(|v| v.as_object()).unwrap();
        assert_eq!(
            obj.get("temperature"),
            Some(&json!(0.7)),
            "existing keys must survive a later insert"
        );
        assert_eq!(obj.get("seed"), Some(&json!(42)));
    }

    #[test]
    fn insert_option_field_overwrites_same_key() {
        // Successive calls with the same key replace the previous
        // value — the chat-send path relies on this when both a
        // sticky `image_num_steps` and a user lock are present.
        let mut opts = Some(json!({"seed": 1}));
        insert_option_field(&mut opts, "seed", json!(2));
        let obj = opts.as_ref().and_then(|v| v.as_object()).unwrap();
        assert_eq!(obj.get("seed"), Some(&json!(2)));
    }

    #[test]
    fn insert_option_field_no_op_when_options_is_non_object() {
        // Defensive: if a caller has stuffed a non-object Value into
        // options (e.g. Number, String) the helper silently no-ops
        // instead of panicking. Tested so a future regression that
        // promotes this to a panic is caught.
        let mut opts = Some(json!(42));
        insert_option_field(&mut opts, "seed", json!(7));
        assert_eq!(
            opts,
            Some(json!(42)),
            "non-object options should be left alone"
        );
    }

    #[test]
    fn apply_layer_mode_options_never_emits_cuda_only() {
        // A guard, not a formality: `{"cuda_only": true}` is read by no request
        // handler, so emitting it would give the user a control that does nothing.
        // A patch that introduces the option must wire it to the engine first, and
        // will have to update this test to do so.
        for mode in [LayerMode::AllLayers, LayerMode::Adaptive] {
            let mut opts: Option<serde_json::Value> = None;
            apply_layer_mode_options(&mut opts, mode);
            if let Some(obj) = opts.as_ref().and_then(|v| v.as_object()) {
                assert!(
                    !obj.contains_key("cuda_only"),
                    "mode {mode:?} must not emit cuda_only — the server never read it"
                );
            }
        }
    }
}

#[cfg(test)]
mod enhance_validation_tests {
    use super::validate_enhanced_prompt;

    /// Real replies captured from three model classes on the same request. The
    /// good one must pass; the two failure shapes must be rejected so the user
    /// gets a clear message instead of a corrupted prompt.
    #[test]
    fn accepts_a_real_rewrite_and_rejects_the_observed_failures() {
        let original = "un chat sur un toit";
        let good = "A fluffy white cat lounges on a sunlit rooftop, paws relaxed, eyes closed, \
                    surrounded by greenery and a clear blue sky. Soft natural light bathes the \
                    scene in warm tones.";
        assert!(validate_enhanced_prompt(good, original).is_ok());

        // Observed from a 7 B model: grammatical, but it adds nothing - it must be
        // rejected so the candidate loop tries a more capable model.
        assert!(validate_enhanced_prompt("a cat lounging on a rooftop", original).is_err());

        let echoed = "Rewrite the user's text into ONE improved generation prompt: an \
                      image-generation prompt: add concrete subject, composition, lighting.";
        assert!(validate_enhanced_prompt(echoed, original).is_err());

        let conversational = "I'm sorry, but I'm here to help you with a question for you: \
                             \"What is your question?\"";
        assert!(validate_enhanced_prompt(conversational, original).is_err());

        let essay = (0..200).map(|_| "word").collect::<Vec<_>>().join(" ");
        assert!(validate_enhanced_prompt(&essay, original).is_err());

        assert!(validate_enhanced_prompt(original, original).is_err());
        assert!(validate_enhanced_prompt("   ", original).is_err());
    }
}
