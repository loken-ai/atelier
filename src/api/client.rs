//! HTTP client for communicating with Ollama/OpenAI-compatible servers

use reqwest::{Client as HttpClient, Response};
use serde::de::DeserializeOwned;
use std::time::Duration;
use tracing::{error, info, warn};

use super::types::*;

/// HTTP client for the LLM server (Ollama-compatible)
pub struct Client {
    base_url: String,
    http_client: reqwest::Client,
    /// Separate client for server-sent-event endpoints - see `post_stream` for why a
    /// streaming request must not carry the one-shot client's total deadline.
    stream_client: reqwest::Client,
}

/// How long a stream may go SILENT before it is considered dead.
///
/// Not how long a render may take: the server sends SSE keep-alive comments, so this
/// only fires when nothing at all arrives. Generous enough to survive a model load and
/// a slow denoising step on a busy machine.
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(180);

/// The request body for an image render, built once so the streaming and non-streaming
/// transports cannot drift on which controls they carry.
///
/// They did drift, and it is why the Media tab posted and waited: only the ollama route
/// streamed, and that route reads four options where this one reads twenty. Sharing the
/// body makes streaming a transport choice rather than a trade against the controls.
#[allow(clippy::too_many_arguments)]
pub fn image_request_body(
    model: &str,
    prompt: &str,
    width: u32,
    height: u32,
    n: u32,
    steps: u32,
    guidance: Option<f64>,
    seed: Option<u64>,
    controls: &RenderControls,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "size": format!("{}x{}", width, height),
        "n": n,
        "num_steps": steps,
    });
    if !controls.output_format.is_empty() {
        body["output_format"] = serde_json::json!(controls.output_format);
    }
    if let Some(g) = guidance {
        body["guidance"] = serde_json::json!(g);
    }
    if let Some(s) = seed {
        body["seed"] = serde_json::json!(s);
    }
    if let Some((image, scale)) = &controls.control {
        use base64::Engine as _;
        body["control_image"] =
            serde_json::json!(base64::engine::general_purpose::STANDARD.encode(image));
        body["control_scale"] = serde_json::json!(scale);
    }
    if !controls.regions.is_empty() {
        // Converted here rather than in the widget: the API takes a rectangle, the
        // user picked an area, and the translation belongs next to the request.
        body["regions"] = serde_json::Value::Array(
            controls
                .regions
                .iter()
                .filter(|r| !r.prompt.trim().is_empty())
                .map(|r| {
                    let (x, y, w, h) = r.area.rect();
                    serde_json::json!({
                        "prompt": r.prompt.trim(),
                        "x": x, "y": y, "w": w, "h": h,
                        "strength": r.strength,
                    })
                })
                .collect(),
        );
    }
    if !controls.loras.is_empty() {
        body["loras"] = serde_json::Value::Array(
            controls
                .loras
                .iter()
                .map(|(name, strength)| serde_json::json!({"name": name, "strength": strength}))
                .collect(),
        );
    }
    if !controls.sampler.is_empty() {
        body["sampler"] = serde_json::json!(controls.sampler);
    }
    if !controls.negative_prompt.trim().is_empty() {
        body["negative_prompt"] = serde_json::json!(controls.negative_prompt.trim());
    }
    if !controls.scheduler.is_empty() {
        body["scheduler"] = serde_json::json!(controls.scheduler);
    }
    body
}

/// The request body for a speech synthesis, built once so the event-stream and the
/// one-shot transports cannot drift on what they ask for.
///
/// Same reason `image_request_body` exists: the streamed variant of `/v1/audio/speech`
/// differs from the plain one by ONE field, `stream_format`, and nothing else. Building
/// the shared part here means the fallback re-sends exactly what the user asked for
/// rather than a second, hand-copied approximation of it.
pub fn speech_request_body(
    model: &str,
    input: &str,
    voice: Option<&str>,
    voice_description: Option<&str>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "input": input,
        "response_format": "wav",
    });
    if let Some(v) = voice {
        if !v.is_empty() {
            body["voice"] = serde_json::json!(v);
        }
    }
    // Parler's real control surface: a free-text description of the voice AND
    // delivery ("an old man shouting angrily", "a soft whispering woman").
    if let Some(d) = voice_description {
        if !d.trim().is_empty() {
            body["voice_description"] = serde_json::json!(d);
        }
    }
    body
}

impl Client {
    /// Create a new client.
    ///
    /// `base_url` is normalised: trailing slashes are stripped so the
    /// `format!("{}/api/...", self.base_url)` pattern used throughout
    /// this module produces canonical paths. Without this, a user who
    /// pasted "http://localhost:11435/" (a common copy-from-browser
    /// shape) ended up sending requests to "http://localhost:11435//api/tags"
    /// which most reverse proxies + frameworks reject as 404.
    pub fn new(base_url: &str) -> Self {
        Self::with_key(base_url, None)
    }

    /// Client that presents `api_key` on every request.
    ///
    /// Set as a DEFAULT HEADER rather than added per call: there are dozens of request
    /// sites, and a scheme where each one has to remember the credential is a scheme
    /// where one of them will not. An empty or whitespace-only key is treated as absent,
    /// so a blank settings field does not send `Bearer ` and earn a confusing 401.
    pub fn with_key(base_url: &str, api_key: Option<&str>) -> Self {
        let mut builder = HttpClient::builder().timeout(Duration::from_secs(30));
        // A second, streaming client. `timeout` in reqwest is a TOTAL deadline that
        // covers reading the body, so on a server-sent-event stream it caps how long
        // the whole render may take - a 50-step video hit the 30-minute ceiling and
        // surfaced as "stream error: error decoding response body", which reads like a
        // protocol fault rather than a stopwatch. What a stream actually needs is an
        // INACTIVITY timeout: `read_timeout` resets on every successful read, so a
        // render may take as long as it takes while a genuinely dead connection is
        // still caught. The server keeps its end alive with SSE comments.
        let mut stream_builder = HttpClient::builder().read_timeout(STREAM_IDLE_TIMEOUT);
        if let Some(key) = api_key.map(str::trim).filter(|k| !k.is_empty()) {
            let mut headers = reqwest::header::HeaderMap::new();
            if let Ok(mut v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {key}")) {
                // Keeps the key out of logs that dump headers.
                v.set_sensitive(true);
                headers.insert(reqwest::header::AUTHORIZATION, v.clone());
                builder = builder.default_headers(headers.clone());
                stream_builder = stream_builder.default_headers(headers);
            }
        }
        let http_client = builder.build().expect("Failed to initialize HTTP client");
        let stream_client = stream_builder
            .build()
            .expect("Failed to initialize streaming HTTP client");

        Self {
            base_url: normalise_base_url(base_url),
            http_client,
            stream_client,
        }
    }

    /// Read-only access to the canonicalised base URL. Useful in tests
    /// and for diagnostic UIs that want to display what the client
    /// will actually hit (vs the raw config value the user typed).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// Strip trailing slashes from a base URL. Idempotent; safe to call
/// on an already-canonical input. Free function so it has a unit-test
/// surface without needing to construct the full reqwest stack.
fn normalise_base_url(raw: &str) -> String {
    raw.trim_end_matches('/').to_string()
}

impl Client {
    /// Snapshot of in-flight + queued requests (GET /api/inflight). Used
    /// by the Hardware tab to render real-time scheduler state.
    pub async fn inflight(&self) -> Result<InflightSnapshot, ClientError> {
        let url = format!("{}/api/inflight", self.base_url);
        let response = self
            .http_client
            .get(&url)
            .timeout(Duration::from_secs(2))
            .send()
            .await?;
        self.handle_response(response).await
    }

    /// Per-device topology (GET /api/distributed/devices). Feeds the Hardware
    /// tab's "Devices" stat card and its device list - without this call the card
    /// has nothing to count and reads zero.
    pub async fn distributed_devices(
        &self,
    ) -> Result<crate::api::types::DevicesResponse, ClientError> {
        let url = format!("{}/api/distributed/devices", self.base_url);
        let response = self
            .http_client
            .get(&url)
            .timeout(Duration::from_secs(3))
            .send()
            .await?;
        self.handle_response(response).await
    }

    /// Per-layer inference metrics (GET /api/layer_perf). Empty until
    /// the server has run at least one forward pass on a tracked model
    /// (z-image-turbo, flux-schnell, or any model wired into LlmEngine's
    /// LayerTimer). Used by the Hardware tab's per-layer panel.
    pub async fn layer_performance(
        &self,
    ) -> Result<crate::api::types::LayerPerfResponse, ClientError> {
        let url = format!("{}/api/layer_perf", self.base_url);
        let response = self
            .http_client
            .get(&url)
            .timeout(Duration::from_secs(2))
            .send()
            .await?;
        self.handle_response(response).await
    }

    /// List available models (GET /api/tags)
    pub async fn list_models(&self) -> Result<ListModelsResponse, ClientError> {
        let url = format!("{}/api/tags", self.base_url);
        info!("GET {}", url);
        let response = self
            .http_client
            .get(&url)
            // A wedged server must not hang this request forever: without a
            // timeout the task never completes, so nothing ever clears the UI
            // state that waits on it.
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| {
                error!("Failed to connect to {}: {}", url, e);
                ClientError::from(e)
            })?;
        let ollama_response: OllamaListModelsResponse = self.handle_response(response).await?;
        info!("Listed {} models", ollama_response.models.len());
        Ok(ListModelsResponse::from_ollama(ollama_response))
    }

    /// Pull a model with explicit source (POST /api/pull)
    pub async fn pull_model_with_source(
        &self,
        model_name: &str,
        source: &str,
    ) -> Result<OllamaPullResponse, ClientError> {
        let url = format!("{}/api/pull", self.base_url);
        info!("POST {} (model={}, source={})", url, model_name, source);
        let mut request = OllamaPullRequest::new(model_name.to_string());
        request.source = source.to_string();
        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .timeout(Duration::from_secs(600))
            .send()
            .await?;
        self.handle_response(response).await
    }

    /// Pull a model, consuming the server's NDJSON progress stream
    /// (POST /api/pull with `stream: true`). Each `{status,total,completed}`
    /// line drives `on_progress(completed, total)`; a `{"status":"success"}`
    /// line resolves Ok, and `{"status":"error",...}` resolves Err. Mirrors
    /// `chat_stream`'s reqwest `.chunk()` + newline-split parse loop.
    pub async fn pull_model_stream(
        &self,
        model_name: &str,
        source: &str,
        mut on_progress: impl FnMut(u64, u64),
    ) -> Result<(), ClientError> {
        let url = format!("{}/api/pull", self.base_url);
        info!(
            "POST {} stream (model={}, source={})",
            url, model_name, source
        );
        let mut request = OllamaPullRequest::new(model_name.to_string());
        request.source = source.to_string();
        request.stream = true;
        let mut response = self
            .http_client
            .post(&url)
            .json(&request)
            .timeout(Duration::from_secs(6 * 3600))
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ClientError::Http(format!(
                "Pull failed with status {}: {}",
                status, text
            )));
        }

        let mut line_buf = String::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    line_buf.push_str(&String::from_utf8_lossy(&chunk));
                    while let Some(pos) = line_buf.find('\n') {
                        let line: String = line_buf[..pos].trim().to_string();
                        line_buf.drain(..=pos);
                        if line.is_empty() {
                            continue;
                        }
                        let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
                            continue;
                        };
                        match val.get("status").and_then(|v| v.as_str()) {
                            Some("error") => {
                                let msg = val
                                    .get("error")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("pull failed");
                                return Err(ClientError::Http(msg.to_string()));
                            }
                            Some("success") => return Ok(()),
                            _ => {
                                if let (Some(c), Some(t)) = (
                                    val.get("completed").and_then(serde_json::Value::as_u64),
                                    val.get("total").and_then(serde_json::Value::as_u64),
                                ) {
                                    on_progress(c, t);
                                }
                            }
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => return Err(ClientError::Request(e.to_string())),
            }
        }
        // Stream closed without an explicit terminal line — treat a clean
        // EOF as success (the download completed; the server just didn't
        // emit a final marker).
        Ok(())
    }

    /// Delete a model (DELETE /api/delete)
    pub async fn delete_model(&self, model_name: &str) -> Result<(), ClientError> {
        let url = format!("{}/api/delete", self.base_url);
        info!("DELETE {} (model={})", url, model_name);
        let request = OllamaDeleteRequest {
            name: model_name.to_string(),
        };
        // 60s instead of the 30s default — deleting a multi-GB
        // model from a spinning disk or a networked filesystem
        // (NFS-mounted ~/.cache/huggingface) can take longer than
        // 30s for the unlink alone, plus Ollama's manifest cleanup.
        let response = self
            .http_client
            .delete(&url)
            .json(&request)
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(ClientError::Http(format!(
                "Delete failed with status {}: {}",
                status, text
            )))
        }
    }

    /// Show model info (POST /api/show)
    /// Chat completion using Ollama API (POST /api/chat)
    pub async fn chat(
        &self,
        request: &OllamaChatRequest,
    ) -> Result<OllamaChatResponse, ClientError> {
        let url = format!("{}/api/chat", self.base_url);
        info!(
            "POST {} (model={}, messages={})",
            url,
            request.model,
            request.messages.len()
        );
        let response = self
            .http_client
            .post(&url)
            .json(request)
            .timeout(Duration::from_secs(300))
            .send()
            .await?;
        self.handle_response(response).await
    }

    /// Chat completion with streaming (returns raw response for NDJSON parsing)
    pub async fn chat_stream(
        &self,
        request: &OllamaChatRequest,
    ) -> Result<reqwest::Response, ClientError> {
        let url = format!("{}/api/chat", self.base_url);
        info!(
            "POST {} stream (model={}, messages={})",
            url,
            request.model,
            request.messages.len()
        );
        let response = self
            .http_client
            .post(&url)
            .json(request)
            .timeout(Duration::from_secs(6 * 3600))
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ClientError::Http(format!(
                "Request failed with status {}: {}",
                status, text
            )));
        }
        Ok(response)
    }

    /// List loaded models (custom endpoint)
    pub async fn list_loaded_models(&self) -> Result<ListLoadedModelsResponse, ClientError> {
        let url = format!("{}/api/models/loaded", self.base_url);
        let response = self
            .http_client
            .get(&url)
            .timeout(Duration::from_secs(20))
            .send()
            .await;
        match response {
            Ok(resp) if resp.status().is_success() => self.handle_response(resp).await,
            _ => Ok(ListLoadedModelsResponse::new(Vec::new())),
        }
    }

    /// Load a model (POST /api/generate with empty prompt per Ollama spec)
    pub async fn load_model(&self, model_name: &str) -> Result<LoadModelResponse, ClientError> {
        let url = format!("{}/api/generate", self.base_url);
        info!("Loading model '{}' via {}", model_name, url);
        let mut request = OllamaGenerateRequest::new(model_name.to_string(), String::new());
        request.keep_alive = Some("5m".to_string());
        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .timeout(Duration::from_secs(600))
            .send()
            .await?;
        let _ollama_response: OllamaGenerateResponse = self.handle_response(response).await?;
        Ok(LoadModelResponse {
            model: model_name.to_string(),
            status: "success".to_string(),
            message: format!("Model {} loaded", model_name),
        })
    }

    /// Unload a model (POST /api/generate with empty prompt and keep_alive: 0)
    pub async fn unload_model(&self, model_name: &str) -> Result<(), ClientError> {
        let url = format!("{}/api/generate", self.base_url);
        info!("Unloading model '{}' via {}", model_name, url);
        let mut request = OllamaGenerateRequest::new(model_name.to_string(), String::new());
        request.keep_alive = Some("0".to_string());
        let response = self.http_client.post(&url).json(&request).send().await;
        match response {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                Err(ClientError::Http(format!(
                    "Unload failed with status {}: {}",
                    status, text
                )))
            }
            Err(e) => Err(ClientError::Request(e.to_string())),
        }
    }

    // ── Media Studio: /v1 generation endpoints ────────────────────

    /// Generate images (POST /v1/images/generations). Returns the
    /// base64-encoded PNGs from the `data[].b64_json` array. `size` is
    /// sent as the OpenAI-shaped "WIDTHxHEIGHT" string; `seed` is passed
    /// through when the caller pinned one (None → server picks random).
    #[allow(clippy::too_many_arguments)]
    pub async fn images_generate(
        &self,
        model: &str,
        prompt: &str,
        width: u32,
        height: u32,
        n: u32,
        steps: u32,
        guidance: Option<f64>,
        seed: Option<u64>,
        controls: &crate::api::types::RenderControls,
    ) -> Result<(Vec<String>, Option<u64>, Option<f64>, Vec<String>), ClientError> {
        let url = format!("{}/v1/images/generations", self.base_url);
        let body = image_request_body(
            model, prompt, width, height, n, steps, guidance, seed, controls,
        );
        info!("POST {url} (model={model}, {width}x{height}, n={n})");
        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .timeout(Duration::from_secs(600))
            .send()
            .await?;
        let parsed: ImagesGenerationResponse = self.handle_response(response).await?;
        Ok((
            parsed.data.into_iter().map(|d| d.b64_json).collect(),
            parsed.render_ms,
            parsed.energy_j,
            parsed.notes,
        ))
    }

    /// The LoRA adapters the server can apply (GET /v1/loras).
    ///
    /// Names, not paths: the server resolves them inside its own adapter directory, so
    /// the picker offers exactly what will resolve.
    /// Returns `(name, target family)`. The family is what the adapter was TRAINED
    /// for; `None` when the server does not recognise its layout. Offering an adapter
    /// for one architecture while another model is selected produces a render that
    /// fails with a message about tensor names - the picker filters on this instead.
    pub async fn list_loras(&self) -> Result<Vec<(String, Option<String>)>, ClientError> {
        let url = format!("{}/v1/loras", self.base_url);
        let response = self
            .http_client
            .get(&url)
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        let parsed: serde_json::Value = self.handle_response(response).await?;
        Ok(parsed
            .get("data")
            .and_then(|d| d.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|e| {
                        let id = e.get("id").and_then(|i| i.as_str())?.to_string();
                        let fam = e.get("family").and_then(|f| f.as_str()).map(str::to_string);
                        Some((id, fam))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Smart-routed conversation turn (POST /conversation): the server picks
    /// the model (chat / vision / image-gen / TTS) from the prompt via rules and
    /// a tiny classifier LLM, keeps the conversation's models warm, and
    /// returns the assistant message plus an optional image/audio payload.
    pub async fn conversation(
        &self,
        messages: serde_json::Value,
        conversation_id: &str,
        chat_model_hint: Option<&str>,
    ) -> Result<ConversationOutput, ClientError> {
        let url = format!("{}/conversation", self.base_url);
        let mut body = serde_json::json!({
            "messages": messages,
            "conversation_id": conversation_id,
            "smart_routing": true,
        });
        // Steers only the CHAT/VISION branch (image/TTS branches read their
        // own fields) - the user's picked model keeps serving text turns.
        if let Some(m) = chat_model_hint {
            body["model"] = serde_json::json!(m);
        }
        info!("POST {} (smart routing, conv {})", url, conversation_id);
        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .timeout(Duration::from_secs(900))
            .send()
            .await?;
        let v: serde_json::Value = self.handle_response(response).await?;
        Ok(ConversationOutput {
            route: v
                .get("route")
                .and_then(|x| x.as_str())
                .unwrap_or("chat")
                .to_string(),
            routed_by: v
                .get("routed_by")
                .and_then(|x| x.as_str())
                .unwrap_or("rules")
                .to_string(),
            model: v
                .get("model")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            content: v
                .pointer("/message/content")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            image: v.get("image").and_then(|x| x.as_str()).map(str::to_string),
            audio: v.get("audio").and_then(|x| x.as_str()).map(str::to_string),
        })
    }

    /// Edit an image (POST /v1/images/edits, multipart). `steps`/`guidance`
    /// of 0 are omitted so the server applies the edit model's defaults.
    #[allow(clippy::too_many_arguments)]
    pub async fn images_edit(
        &self,
        model: &str,
        prompt: &str,
        image_name: &str,
        image_bytes: Vec<u8>,
        strength: f32,
        steps: u32,
        guidance: f32,
        n: u32,
        seed: Option<u64>,
        // Adapters by NAME, from the server's own listing - never a path.
        loras: &[(String, f32)],
        // What the guidance steers AWAY from. Omitted when blank.
        negative_prompt: &str,
    ) -> Result<(Vec<String>, Option<u64>, Option<f64>), ClientError> {
        let url = format!("{}/v1/images/edits", self.base_url);
        let mut form = reqwest::multipart::Form::new()
            .part(
                "image",
                reqwest::multipart::Part::bytes(image_bytes).file_name(image_name.to_string()),
            )
            .text("prompt", prompt.to_string())
            .text("model", model.to_string())
            .text("strength", strength.to_string())
            .text("n", n.to_string());
        if !negative_prompt.trim().is_empty() {
            form = form.text("negative_prompt", negative_prompt.trim().to_string());
        }
        if steps > 0 {
            form = form.text("num_steps", steps.to_string());
        }
        if guidance > 0.0 {
            form = form.text("guidance", guidance.to_string());
        }
        if let Some(sd) = seed {
            form = form.text("seed", sd.to_string());
        }
        if !loras.is_empty() {
            // The multipart field carries the same JSON shape as the JSON endpoints, so
            // one parser serves both and the two cannot drift apart.
            let spec: Vec<serde_json::Value> = loras
                .iter()
                .map(|(name, strength)| serde_json::json!({"name": name, "strength": strength}))
                .collect();
            form = form.text("loras", serde_json::Value::Array(spec).to_string());
        }
        info!("POST {} (edit model={}, strength={})", url, model, strength);
        let response = self
            .http_client
            .post(&url)
            .multipart(form)
            .timeout(Duration::from_secs(900))
            .send()
            .await?;
        let parsed: ImagesGenerationResponse = self.handle_response(response).await?;
        Ok((
            parsed.data.into_iter().map(|d| d.b64_json).collect(),
            parsed.render_ms,
            parsed.energy_j,
        ))
    }

    /// Transcribe (or translate to English) an audio file
    /// (POST /v1/audio/{transcriptions,translations}, multipart). Returns the text.
    pub async fn audio_transcribe(
        &self,
        model: &str,
        file_name: &str,
        file_bytes: Vec<u8>,
        translate: bool,
        language: &str,
    ) -> Result<String, ClientError> {
        let ep = if translate {
            "translations"
        } else {
            "transcriptions"
        };
        let url = format!("{}/v1/audio/{}", self.base_url, ep);
        let mut form = reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes(file_bytes).file_name(file_name.to_string()),
        );
        if !model.trim().is_empty() {
            form = form.text("model", model.to_string());
        }
        // Empty means "detect it" - the server's own default - so it is not sent at all
        // rather than sent as an empty string it would have to interpret.
        if !language.trim().is_empty() {
            form = form.text("language", language.trim().to_string());
        }
        info!("POST {} ({} bytes)", url, file_name);
        let response = self
            .http_client
            .post(&url)
            .multipart(form)
            .timeout(Duration::from_secs(600))
            .send()
            .await?;
        #[derive(serde::Deserialize)]
        struct Tr {
            text: String,
        }
        let parsed: Tr = self.handle_response(response).await?;
        Ok(parsed.text)
    }

    /// Split a mix into stems (POST /v1/audio/separate). Returns the base64 WAVs
    /// the server produced, labelled, in the order a listener would want them.
    pub async fn audio_separate(
        &self,
        file_name: &str,
        file_bytes: Vec<u8>,
        stems: &str,
    ) -> Result<Vec<(String, String)>, ClientError> {
        let url = format!("{}/v1/audio/separate", self.base_url);
        let form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(file_bytes).file_name(file_name.to_string()),
            )
            .text("stems", stems.to_string());
        info!("POST {url} (stems={stems})");
        let response = self
            .http_client
            .post(&url)
            .multipart(form)
            // Separation runs a transformer over the whole track, so it is minutes for
            // a long song, not seconds.
            .timeout(Duration::from_secs(6 * 3600))
            .send()
            .await?;
        #[derive(serde::Deserialize)]
        struct Sep {
            vocals: Option<String>,
            instrumental: Option<String>,
        }
        let parsed: Sep = self.handle_response(response).await?;
        let mut out = Vec::new();
        if let Some(v) = parsed.vocals {
            out.push(("vocals".to_string(), v));
        }
        if let Some(i) = parsed.instrumental {
            out.push(("instrumental".to_string(), i));
        }
        if out.is_empty() {
            return Err(ClientError::Http("the server returned no stems".into()));
        }
        Ok(out)
    }

    /// Generate audio / MIDI (POST /v1/audio/generations). The caller
    /// builds the JSON body (fields differ per model: ace-step music
    /// takes bpm, ezaudio SFX doesn't, midi takes max_tokens/temperature/
    /// top_p). Returns the raw `data[]` items so the caller can route by
    /// `content_type` (audio/wav → play, audio/midi → save).
    pub async fn audio_generate(
        &self,
        body: serde_json::Value,
    ) -> Result<Vec<crate::api::types::MediaDatum>, ClientError> {
        let url = format!("{}/v1/audio/generations", self.base_url);
        info!("POST {} (audio/midi generation)", url);
        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .timeout(Duration::from_secs(900))
            .send()
            .await?;
        let parsed: ImagesGenerationResponse = self.handle_response(response).await?;
        Ok(parsed.data)
    }

    /// POST /v1/embeddings → the L2-normalized embedding vector for one input.
    /// NB: the model must be LOADED first (embeddings don't auto-load like chat).
    pub async fn embeddings(&self, model: &str, input: &str) -> Result<Vec<f32>, ClientError> {
        #[derive(serde::Deserialize)]
        struct Resp {
            data: Vec<Item>,
        }
        #[derive(serde::Deserialize)]
        struct Item {
            embedding: Vec<f32>,
        }
        let url = format!("{}/v1/embeddings", self.base_url);
        info!("POST {} (embeddings)", url);
        let response = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({ "model": model, "input": input }))
            .timeout(Duration::from_secs(120))
            .send()
            .await?;
        let parsed: Resp = self.handle_response(response).await?;
        parsed
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| ClientError::Parse("empty embeddings response".into()))
    }

    /// POST /v1/rerank → `(document_index, relevance_score)` pairs, highest-first.
    /// NB: the model must be LOADED first (like embeddings).
    pub async fn rerank(
        &self,
        model: &str,
        query: &str,
        documents: Vec<String>,
    ) -> Result<Vec<(usize, f32)>, ClientError> {
        #[derive(serde::Deserialize)]
        struct Resp {
            results: Vec<Item>,
        }
        #[derive(serde::Deserialize)]
        struct Item {
            index: usize,
            relevance_score: f32,
        }
        let url = format!("{}/v1/rerank", self.base_url);
        info!("POST {} (rerank, {} docs)", url, documents.len());
        let response = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({ "model": model, "query": query, "documents": documents }))
            .timeout(Duration::from_secs(120))
            .send()
            .await?;
        let parsed: Resp = self.handle_response(response).await?;
        Ok(parsed
            .results
            .into_iter()
            .map(|r| (r.index, r.relevance_score))
            .collect())
    }

    /// POST a JSON body to a `/v1/*` generation endpoint and return the
    /// raw streaming Response for SSE parsing (used by the Media Studio's
    /// Music / Video progress path with `"stream": true`). The caller
    /// reads the body chunk-by-chunk and parses the `data:` events.
    /// Tell the server to stop a render, by the name it gave in its first message.
    ///
    /// Dropping the request does NOT stop one: the server does not abandon a handler whose
    /// response has not started, so the work runs to completion for a client that has gone -
    /// measured, a whole clip. Cancelling is something the client has to SAY.
    pub async fn cancel_render(&self, id: &str) -> Result<(), ClientError> {
        let url = format!("{}/v1/renders/{}/cancel", self.base_url, id);
        let response = self.http_client.post(&url).send().await?;
        Self::check_status(response, "/v1/renders/cancel").await?;
        Ok(())
    }

    /// Ask what a video render will cost BEFORE starting one: `POST /v1/video/plan`.
    ///
    /// The body either describes an intent (`seconds`, `quality`, `model`) and lets the
    /// server choose the settings, or states settings already chosen (`width`, `height`,
    /// `frames`, `steps`, `model`) and asks only for their cost. The reply carries
    /// `estimated_seconds` plus the `basis` that number rests on - the denoise alone, not
    /// the load or the decode - which is why the caller must repeat that qualifier rather
    /// than present it as a total.
    ///
    /// Returned raw: the interface reads two fields out of it, and pinning a struct here
    /// would make every future field the server adds a compile error in the GUI.
    pub async fn plan_video(
        &self,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let url = format!("{}/v1/video/plan", self.base_url);
        let response = self
            .http_client
            .post(&url)
            .json(&body)
            // Arithmetic on the server, no model touched: a request that has not answered
            // in seconds is a server that cannot answer, and the estimate simply stays
            // hidden rather than holding a slot open behind a slider drag.
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        let response = Self::check_status(response, "/v1/video/plan").await?;
        Ok(response.json().await?)
    }

    pub async fn post_stream(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, ClientError> {
        info!("POST {}{} (stream)", self.base_url, path);
        let url = format!("{}{}", self.base_url, path);
        let response = self.stream_client.post(&url).json(&body).send().await?;
        Self::check_status(response, path).await
    }

    /// POST a JSON body to `path` and check the status — the shared
    /// front half of every one-shot request. Non-2xx becomes ONE
    /// canonical `ClientError::Http` wording keyed on the endpoint, so a failure
    /// reads the same whichever method produced it.
    async fn post_json(
        &self,
        path: &str,
        body: &serde_json::Value,
        timeout: Duration,
    ) -> Result<reqwest::Response, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .http_client
            .post(&url)
            .json(body)
            .timeout(timeout)
            .send()
            .await?;
        Self::check_status(response, path).await
    }

    /// Turn a non-2xx into the one canonical wording, reading the body for the reason.
    async fn check_status(
        response: reqwest::Response,
        path: &str,
    ) -> Result<reqwest::Response, ClientError> {
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ClientError::Http(format!(
                "{path} failed with status {status}: {text}"
            )));
        }
        Ok(response)
    }

    /// Synthesise speech (POST /v1/audio/speech) from a body built by
    /// [`speech_request_body`], and return the clip base64-encoded - the route answers
    /// with a raw WAV *binary* body, and base64 is the `result_audios` convention the
    /// rest of the Media Studio uses (Play routes through play_audio_blob, which decodes
    /// it again).
    ///
    /// The one-shot half of the speech transport: the event-stream variant falls back to
    /// it with the SAME body minus `stream_format`, so a server that does not know the
    /// field still speaks.
    pub async fn audio_speech_from_body(
        &self,
        body: &serde_json::Value,
    ) -> Result<String, ClientError> {
        info!("POST {}/v1/audio/speech (speech)", self.base_url);
        let response = self
            .post_json("/v1/audio/speech", body, Duration::from_secs(300))
            .await?;
        let bytes = response.bytes().await.map_err(ClientError::from)?;
        use base64::Engine;
        Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
    }

    /// List available TTS voices (GET /v1/audio/voices).
    pub async fn list_voices(&self) -> Result<Vec<String>, ClientError> {
        let url = format!("{}/v1/audio/voices", self.base_url);
        let response = self
            .http_client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        let parsed: crate::api::types::VoicesResponse = self.handle_response(response).await?;
        Ok(parsed.data.into_iter().map(|d| d.voice).collect())
    }

    /// Handle HTTP response
    async fn handle_response<T: DeserializeOwned>(
        &self,
        response: Response,
    ) -> Result<T, ClientError> {
        let url = response.url().to_string();
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            // The body can echo the request, so it is not logged; it still reaches
            // the user through the error returned below.
            error!("{} returned {} ({} byte body)", url, status, text.len());
            // Same canonical wording as post_json so error bubbles read
            // the same regardless of which path produced them.
            return Err(ClientError::Http(format!(
                "{url} failed with status {status}: {text}"
            )));
        }
        let json = response.json::<T>().await.map_err(|e| {
            warn!("Failed to parse response from {}: {}", url, e);
            ClientError::from(e)
        })?;
        Ok(json)
    }
}

/// Client error types
#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Request error: {0}")]
    Request(String),
}

impl From<reqwest::Error> for ClientError {
    fn from(err: reqwest::Error) -> Self {
        // reqwest's default Display chain is verbose ("error sending
        // request for url (http://...): error trying to connect:
        // tcp connect error: Connection refused (os error 111)").
        // Most of that is internal plumbing. Surface a short
        // human-readable summary keyed on the failure category so
        // the error bubble in chat doesn't bury the actionable bit.
        let url = err.url().map(|u| u.to_string());
        let summary = if err.is_timeout() {
            "request timed out".to_string()
        } else if err.is_connect() {
            "could not connect to the server".to_string()
        } else if err.is_decode() {
            "server returned an unexpected response shape".to_string()
        } else if err.is_body() {
            "transport error while reading the response body".to_string()
        } else {
            // Fall back to the inner-most source — bypasses reqwest's
            // outer wrapping which adds the URL prefix we'll re-attach
            // below if available.
            let mut s: &dyn std::error::Error = &err;
            while let Some(next) = s.source() {
                s = next;
            }
            s.to_string()
        };
        ClientError::Request(match url {
            Some(u) => format!("{} ({})", summary, u),
            None => summary,
        })
    }
}

#[cfg(test)]
mod tests {

    /// A blank key must be treated as absent, not sent as `Bearer `.
    ///
    /// The failure this prevents is quiet: an untouched settings field would attach an
    /// empty credential, the server would answer 401, and the user would be told their
    /// key is wrong when they never set one.
    #[test]
    fn a_blank_api_key_is_not_sent() {
        // Construction is the observable: with_key must not panic and must accept every
        // shape of "nothing".
        for blank in [None, Some(""), Some("   "), Some("\n")] {
            let c = Client::with_key("http://localhost:11435", blank);
            assert_eq!(c.base_url(), "http://localhost:11435");
        }
        let c = Client::with_key("http://localhost:11435", Some("  real-key  "));
        assert_eq!(c.base_url(), "http://localhost:11435");
    }
    use super::*;

    /// The streamed and the plain speech request must differ by ONE field.
    ///
    /// That is what makes the fallback safe: when the server cannot answer with events,
    /// what goes back is the request the user made, not a second hand-copied reading of
    /// it that could quietly drop the voice or the description.
    #[test]
    fn the_two_speech_transports_ask_for_the_same_thing() {
        let plain = speech_request_body(
            "parler-tts-mini-v1",
            "Hello there.",
            Some("nova"),
            Some("a soft whispering woman"),
        );
        assert!(
            plain.get("stream_format").is_none(),
            "the plain body must not ask for events"
        );
        let mut streamed = plain.clone();
        streamed["stream_format"] = serde_json::json!("sse");
        for key in [
            "model",
            "input",
            "response_format",
            "voice",
            "voice_description",
        ] {
            assert_eq!(plain.get(key), streamed.get(key), "{key} must not drift");
        }
        assert_eq!(streamed["stream_format"], "sse");
    }

    /// A blank voice or description is ABSENT, not empty: the backend picks its own
    /// default for a field that is missing, and an empty string is a value that overrides
    /// it with nothing.
    #[test]
    fn a_blank_voice_or_description_is_left_out_of_the_speech_body() {
        let body = speech_request_body("piper", "Hello.", Some(""), Some("   "));
        assert!(body.get("voice").is_none());
        assert!(body.get("voice_description").is_none());
        let body = speech_request_body("piper", "Hello.", None, None);
        assert!(body.get("voice").is_none() && body.get("voice_description").is_none());
        assert_eq!(body["response_format"], "wav");
    }

    #[test]
    fn normalise_base_url_strips_single_trailing_slash() {
        // The exact bug: paste from a browser address bar lands
        // "http://localhost:11435/" in the URL field; format!() then
        // produces "http://localhost:11435//api/tags" which reverse
        // proxies (nginx, traefik) and most web frameworks reject.
        assert_eq!(
            normalise_base_url("http://localhost:11435/"),
            "http://localhost:11435",
        );
    }

    #[test]
    fn normalise_base_url_strips_multiple_trailing_slashes() {
        // Defensive: belt-and-braces, since some clipboard handlers
        // append "//" or users hit / a few times. The pattern still
        // produces a single canonical form.
        assert_eq!(
            normalise_base_url("http://localhost:11435///"),
            "http://localhost:11435",
        );
    }

    #[test]
    fn normalise_base_url_is_idempotent_on_canonical_input() {
        // Calling normalise on the already-canonical output must be
        // a no-op (so re-saving the config doesn't churn the field).
        let canon = "http://localhost:11435";
        assert_eq!(normalise_base_url(canon), canon);
        assert_eq!(normalise_base_url(&normalise_base_url(canon)), canon);
    }

    #[test]
    fn normalise_base_url_preserves_path_segments() {
        // Only the trailing slash is stripped — path segments before
        // it stay intact. This matters for reverse-proxied deployments
        // ("https://example.com/loken/") where the path prefix is
        // load-bearing.
        assert_eq!(
            normalise_base_url("https://example.com/loken/"),
            "https://example.com/loken",
        );
        assert_eq!(
            normalise_base_url("https://example.com/loken"),
            "https://example.com/loken",
        );
    }

    #[test]
    fn client_new_canonicalises_base_url_on_construction() {
        // End-to-end: the public Client::new entry point must surface
        // the canonical form via base_url(), so anything in the rest
        // of the GUI that introspects the base (diagnostic UIs,
        // future absolute-URL builders) sees the same value.
        let c = Client::new("http://localhost:11435/");
        assert_eq!(c.base_url(), "http://localhost:11435");
    }
}

/// Parsed /conversation response (smart-routed chat turn).
#[derive(Debug, Clone)]
pub struct ConversationOutput {
    pub route: String,
    pub routed_by: String,
    pub model: String,
    pub content: String,
    pub image: Option<String>,
    pub audio: Option<String>,
}
