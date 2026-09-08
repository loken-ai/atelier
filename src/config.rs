//! GUI Configuration Types
//!
//! Configuration and profile management for the GUI application.

use serde::{Deserialize, Serialize};

/// Default URL of the local LOKEN binary. Hardcoded into
/// AppConfig::default(), the seed Default profile, the Settings tab's
/// Quick Connect chip, and the "+ New Profile" template — keeping
/// them in lockstep matters because changing one without the others
/// would make the GUI ship with a default that disagrees with the
/// Quick Connect button it shows.
pub const DEFAULT_LOKEN_URL: &str = "http://localhost:11435";

/// Default URL of a local Ollama server. Used by the seed Ollama
/// profile + the Settings tab's Ollama Quick Connect chip.
pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// API type selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ApiType {
    #[default]
    Loken,
    Ollama,
    OpenApi,
}

impl std::fmt::Display for ApiType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiType::Loken => write!(f, "LOKEN"),
            ApiType::Ollama => write!(f, "Ollama"),
            ApiType::OpenApi => write!(f, "OpenAPI"),
        }
    }
}

/// Ollama-specific parameters
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OllamaParams {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub repeat_penalty: f32,
    pub num_ctx: u32,
    /// Max tokens to generate (-1 = until EOS/context). Maps to `options.num_predict`.
    #[serde(default = "default_num_predict")]
    pub num_predict: i32,
    /// RNG seed (-1 = random each request). Sent only when >= 0 (reproducible output).
    #[serde(default = "default_seed")]
    pub seed: i64,
    /// Comma-separated stop sequences (empty = none). Split into `options.stop` array.
    #[serde(default)]
    pub stop: String,
}

fn default_num_predict() -> i32 {
    -1
}
fn default_seed() -> i64 {
    -1
}

impl Default for OllamaParams {
    fn default() -> Self {
        Self {
            temperature: 0.15,
            top_p: 0.9,
            top_k: 40,
            repeat_penalty: 1.1,
            num_ctx: 2048,
            num_predict: -1,
            seed: -1,
            stop: String::new(),
        }
    }
}

/// OpenAPI-specific parameters
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenApiParams {
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub top_p: f32,
    pub frequency_penalty: f32,
    pub presence_penalty: f32,
}

impl Default for OpenApiParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            max_tokens: Some(1024),
            top_p: 1.0,
            frequency_penalty: 0.0,
            presence_penalty: 0.0,
        }
    }
}

/// LLM Server-specific parameters
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LokenParams {
    pub context_length: u32,
    pub temperature: f32,
}

impl Default for LokenParams {
    fn default() -> Self {
        Self {
            context_length: 2000,
            temperature: 0.15, // Matches the Ollama default + config.toml.
        }
    }
}

/// API Profile configuration
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
/// Persisted: it must survive a file written by an OLDER build. The container-level
/// `serde(default)` is what guarantees that - without it ONE field added here makes serde
/// reject the WHOLE file, and the user loses every setting and every stored prompt, not
/// just the new field. That happened.
///
/// `Default` exists only to serve that: a profile is always read from the file, and the
/// empty one this derives is never a profile the app creates.
#[serde(default)]
pub struct ApiProfile {
    pub name: String,
    pub api_type: ApiType,
    pub server_url: String,
    pub model: String,
    pub ollama_params: OllamaParams,
    pub openapi_params: OpenApiParams,
    pub loken_params: LokenParams,
}

impl ApiProfile {
    pub fn new(name: String, api_type: ApiType, server_url: String, model: String) -> Self {
        Self {
            name,
            api_type,
            server_url,
            model,
            ollama_params: OllamaParams::default(),
            openapi_params: OpenApiParams::default(),
            loken_params: LokenParams::default(),
        }
    }
}

/// Application configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Persisted: it must survive a file written by an OLDER build. The container-level
/// `serde(default)` is what guarantees that - without it ONE field added here makes serde
/// reject the WHOLE file, and the user loses every setting and every stored prompt, not
/// just the new field. That happened.
#[serde(default)]
pub struct AppConfig {
    pub server_url: String,
    /// API key presented to the server, when it requires one.
    ///
    /// Optional because the server's own default is no authentication; a blank value is
    /// treated as absent so an untouched field costs nothing.
    #[serde(default)]
    pub api_key: Option<String>,
    pub dark_theme: bool,
    pub selected_model: Option<String>,
    pub selected_model_source: Option<String>, // "ollama" or "huggingface"
    pub window_width: Option<f32>,
    pub window_height: Option<f32>,
    pub window_maximized: Option<bool>,
    pub profiles: Vec<ApiProfile>,
    pub selected_profile: Option<String>,
    /// Last-used layer-mode toggle for the chat tab. Persisted so a
    /// power user who explicitly picked Adaptive for perf doesn't
    /// have to re-toggle on every GUI launch. `#[serde(default)]`
    /// keeps older configs (saved before this field was added)
    /// loading cleanly — they hydrate to LayerMode::AllLayers.
    #[serde(default)]
    pub layer_mode: crate::state::LayerMode,
    /// Media Studio parameters, saved at exit so a restart keeps every tuned
    /// setting (kind, prompt, per-kind params). None on first launch.
    #[serde(default)]
    pub media: Option<crate::state::MediaPersist>,
    /// Chat "Auto (smart routing)" toggle, persisted like the model
    /// selection it replaces — re-enabling it every launch made the
    /// feature look unwired.
    #[serde(default)]
    pub chat_smart_auto: bool,
    /// Whether the navigation sidebar shows labels (expanded) or is
    /// collapsed to an icons-only rail. Persisted so a user who
    /// prefers the compact rail doesn't have to re-collapse it on
    /// every launch. `#[serde(default = "default_true")]` keeps older
    /// configs (saved before this field existed) loading as expanded,
    /// which matches the historical hardcoded default.
    #[serde(default = "default_true")]
    pub sidebar_expanded: bool,
}

/// serde default for `AppConfig::sidebar_expanded` — the sidebar
/// starts expanded (labels visible) unless the user collapsed it in a
/// prior session. A free function because `#[serde(default = ...)]`
/// needs a path, not a literal.
fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            server_url: DEFAULT_LOKEN_URL.to_string(),
            dark_theme: true,
            selected_model: None,
            selected_model_source: None,
            window_width: Some(1000.0),
            window_height: Some(750.0),
            window_maximized: Some(true),
            layer_mode: crate::state::LayerMode::default(),
            media: None,
            chat_smart_auto: false,
            sidebar_expanded: true,
            profiles: vec![
                ApiProfile::new(
                    "Default".into(),
                    ApiType::Loken,
                    DEFAULT_LOKEN_URL.into(),
                    "default".into(),
                ),
                ApiProfile::new(
                    "Ollama".into(),
                    ApiType::Ollama,
                    DEFAULT_OLLAMA_URL.into(),
                    "llama3.2:latest".into(),
                ),
            ],
            selected_profile: Some("Default".into()),
        }
    }
}

impl AppConfig {
    /// Load configuration from disk.
    ///
    /// The original implementation was a `.and_then(...).unwrap_or_default()`
    /// chain that silently fell back to `AppConfig::default()` on parse
    /// failure — meaning a corrupt JSON file (partial write, manual edit
    /// gone wrong, schema downgrade) would wipe the user's profiles,
    /// server URL, theme, selected model, and window prefs on the very
    /// next launch with no warning. That's real data loss.
    ///
    /// Now: when the file exists but won't parse, the broken bytes are
    /// preserved at `config.json.broken.<timestamp>` before defaults
    /// kick in, and a `tracing::warn!` surfaces in the in-app log
    /// buffer so the user can spot it without diffing two unrelated
    /// behaviour changes. They can then either restore the backup
    /// manually or accept the fresh defaults knowing what happened.
    pub fn load() -> Self {
        let config_path = directories::ProjectDirs::from("com", "loken", "atelier")
            .map(|d| d.config_dir().join("config.json"));

        let mut config = match config_path.as_ref() {
            Some(path) => Self::load_from_path(path),
            None => AppConfig::default(), // no platform config dir
        };

        // Migration: bump persisted profiles still on the legacy
        // temperature=0.8 default up to the current 0.15 default
        // (matches config.toml's server-side temperature). Without
        // this, users who saved a config before the default change
        // would silently keep the higher temperature on next load.
        for profile in &mut config.profiles {
            if profile.loken_params.temperature == 0.8 {
                profile.loken_params.temperature = 0.15;
            }
            if profile.ollama_params.temperature == 0.8 {
                profile.ollama_params.temperature = 0.15;
            }
        }

        // Auto-save migrated config
        config.save();
        config
    }

    /// Read + parse `config.json` from an explicit path. On parse
    /// failure, back up the broken bytes to
    /// `<path>.broken.<unix_ts>` (skipping the backup if the file is
    /// 0-byte / empty — common after a crashed write) and fall back
    /// to `AppConfig::default()`. Logs the disposition via
    /// `tracing::warn!` so the user sees it in the Server Log tab.
    ///
    /// Separated from `load()` so unit tests can drive it against a
    /// tempdir without going through `ProjectDirs::from`, which
    /// resolves to the user's real platform config dir and would
    /// pollute it.
    pub(crate) fn load_from_path(path: &std::path::Path) -> Self {
        let Ok(s) = std::fs::read_to_string(path) else {
            return AppConfig::default(); // file absent — normal first run
        };
        match serde_json::from_str::<AppConfig>(&s) {
            Ok(cfg) => cfg,
            Err(parse_err) => {
                if !s.is_empty() {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let backup = path.with_extension(format!("json.broken.{ts}"));
                    if let Err(e) = std::fs::write(&backup, &s) {
                        tracing::warn!(
                            "config.json parse failed ({parse_err}); ALSO failed to back \
                             up the broken file to {}: {e}. Falling back to defaults.",
                            backup.display(),
                        );
                    } else {
                        tracing::warn!(
                            "config.json parse failed ({parse_err}); broken file backed \
                             up to {}. Falling back to defaults — re-edit settings or \
                             restore the backup manually.",
                            backup.display(),
                        );
                    }
                } else {
                    tracing::warn!(
                        "config.json was empty (likely a crashed write); using defaults"
                    );
                }
                AppConfig::default()
            }
        }
    }

    /// Save configuration to disk.
    ///
    /// Returns `Ok` on success or a human-readable error string. The
    /// returned `Result` is `must_use` so call sites can't silently
    /// drop a failure on the floor again — the previous version used
    /// `let _ = std::fs::write(...)` which left the user staring at
    /// the Settings tab with no clue that their changes never landed
    /// (read-only mount, full disk, sandboxed config dir, etc.).
    ///
    /// Callers that genuinely don't want to surface the error to the
    /// user should still log it via `tracing::warn!` rather than
    /// discarding it; helper `save_logged` does that automatically.
    #[must_use = "Config save can fail (full disk, read-only mount, permission denied); \
                  handle the Result or call save() for a logged fire-and-forget"]
    pub fn try_save(&self) -> Result<std::path::PathBuf, String> {
        let dir = directories::ProjectDirs::from("com", "loken", "atelier").ok_or_else(|| {
            "no platform config directory available (ProjectDirs returned None)".to_string()
        })?;
        self.save_to_dir(dir.config_dir())
    }

    /// Test-friendly variant: write into an explicit directory rather
    /// than the platform-resolved config dir. Lets unit tests assert
    /// success + failure paths against a tempdir without polluting
    /// the user's real ~/.config/atelier.
    pub(crate) fn save_to_dir(
        &self,
        config_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        std::fs::create_dir_all(config_dir)
            .map_err(|e| format!("create config dir {}: {}", config_dir.display(), e))?;
        let target = config_dir.join("config.json");
        let json =
            serde_json::to_string_pretty(self).map_err(|e| format!("serialise config: {e}"))?;
        std::fs::write(&target, &json).map_err(|e| format!("write {}: {}", target.display(), e))?;
        Ok(target)
    }

    /// Save, and log a failure through `tracing` - which reaches both the in-app
    /// log buffer and the console. Fire-and-forget at the call site, but a failure
    /// is visible rather than dropped.
    pub fn save(&self) {
        if let Err(e) = self.try_save() {
            tracing::warn!("config.save failed: {e}");
        }
    }

    /// Pick the lowest unused "Profile N" name given the existing
    /// profile list. Used by the Settings tab's "+ New Profile"
    /// button so a delete-from-middle-then-add sequence can't
    /// produce a duplicate name (which would violate the
    /// unique-names invariant the default test pins).
    ///
    /// Probes upward from 2 (existing seed profiles are "Default"
    /// and "Ollama", so "Profile 1" would just look weird as a
    /// first user-created profile). The probe is bounded only by
    /// the input range; in practice the loop exits in O(N) where N
    /// is the number of existing profiles.
    pub fn next_profile_name(existing: &[ApiProfile]) -> String {
        (2usize..)
            .map(|i| format!("Profile {i}"))
            .find(|name| !existing.iter().any(|p| &p.name == name))
            .expect("infinite range always yields an unused name")
    }

    /// Snapshot the existing `config.json` to
    /// `<path>.before_reset.<unix_ts>` before a destructive reset
    /// overwrites it with defaults. Symmetric with the corrupt-file
    /// backup in `load_from_path` — gives users a manual rescue
    /// path if they hit "Reset" by accident.
    ///
    /// Pure helper around fs::copy so the Settings tab can call it
    /// without depending on directories::ProjectDirs. Best-effort:
    /// failures are logged and the reset still proceeds (a backup
    /// failure shouldn't block the user's intent to reset).
    ///
    /// `dir` is the config directory (same one save_to_dir writes
    /// into). No-op if the source `config.json` is missing — there's
    /// nothing to back up before a first-time reset.
    pub fn backup_before_reset(dir: &std::path::Path) {
        let src = dir.join("config.json");
        if !src.exists() {
            return;
        }
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let dst = src.with_extension(format!("json.before_reset.{ts}"));
        match std::fs::copy(&src, &dst) {
            Ok(_) => {
                tracing::info!(
                    "Config snapshot saved to {} before reset — restore manually if needed",
                    dst.display()
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to back up config.json to {} before reset: {e}. \
                     Reset will still proceed.",
                    dst.display()
                );
            }
        }
    }
}

#[cfg(test)]
// Same rationale as state::chat_history_tests: tests mutate
// individual AppConfig fields after Default::default() to set up
// scenarios. Inline field mutation is more readable than the
// `AppConfig { field: x, ..Default::default() }` struct-update
// syntax when toggling 1-2 fields out of ~10 for a focused test.
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn app_config_default_has_two_profiles_with_unique_names() {
        // AppConfig::default seeds two profiles (Default for LOKEN
        // and Ollama for vanilla Ollama). Pin both so a future change
        // to remove or rename either can't silently break the
        // first-run UX where the Settings tab expects to find them.
        let cfg = AppConfig::default();
        assert_eq!(cfg.profiles.len(), 2, "default seeds exactly 2 profiles");

        let names: Vec<&str> = cfg.profiles.iter().map(|p| p.name.as_str()).collect();
        assert!(
            names.contains(&"Default"),
            "Default profile missing — names: {names:?}"
        );
        assert!(
            names.contains(&"Ollama"),
            "Ollama profile missing — names: {names:?}"
        );

        // Profiles must have distinct names (the Settings dropdown
        // keys by name; a duplicate would mask one of them).
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            names.len(),
            "default profile names must be unique"
        );

        // selected_profile must point at one of them so the GUI
        // doesn't start without an active profile.
        let selected = cfg
            .selected_profile
            .as_deref()
            .expect("default config must have selected_profile set");
        assert!(
            names.contains(&selected),
            "selected_profile '{selected}' not in names {names:?}"
        );
    }

    #[test]
    fn app_config_default_endpoints_match_well_known_ports() {
        // Default's LOKEN URL must point at LOKEN's canonical
        // port (11435) so first-run users hit a server that exists if
        // they followed the README. Ollama profile's URL must point at
        // the upstream Ollama port (11434).
        let cfg = AppConfig::default();
        let by_name = |n: &str| cfg.profiles.iter().find(|p| p.name == n).unwrap();

        assert!(
            by_name("Default").server_url.ends_with(":11435"),
            "Default profile must target loken port 11435 — got '{}'",
            by_name("Default").server_url
        );
        assert!(
            by_name("Ollama").server_url.ends_with(":11434"),
            "Ollama profile must target Ollama port 11434 — got '{}'",
            by_name("Ollama").server_url
        );
    }

    #[test]
    fn api_type_display_strings_are_stable_for_dropdown() {
        // The Settings tab dropdown renders ApiType via Display.
        // Pin the human-readable strings so a future rename
        // (Loken → 'LOKEN' or 'Server', etc.) doesn't
        // silently change the dropdown label and confuse users
        // mid-session.
        assert_eq!(ApiType::Loken.to_string(), "LOKEN");
        assert_eq!(ApiType::Ollama.to_string(), "Ollama");
        assert_eq!(ApiType::OpenApi.to_string(), "OpenAPI");
    }

    #[test]
    fn api_type_default_is_loken() {
        // ApiProfile::new takes an ApiType, but the serde Default
        // derive on ApiType (used when a saved config is missing
        // the field) must hydrate to Loken — that's the local
        // server the GUI primarily targets. A change here would
        // shift first-run users onto Ollama / OpenAPI silently.
        assert_eq!(ApiType::default(), ApiType::Loken);
    }

    #[test]
    fn api_type_roundtrips_through_json() {
        // ApiType is serialized inside ApiProfile in the persisted
        // config.json. Pin the JSON encoding so a future config
        // saved with one variant still loads as that variant.
        for t in [ApiType::Loken, ApiType::Ollama, ApiType::OpenApi] {
            let json = serde_json::to_string(&t).expect("serialize");
            let back: ApiType = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(t, back, "{t} did not roundtrip through JSON");
        }
    }

    #[test]
    fn param_defaults_are_sane_for_first_run() {
        // Pin sane-defaults for all three param categories so a
        // future regression that drops temperature to 0.0 (greedy)
        // or sets top_p to 0.0 (no sampling) doesn't silently
        // produce degenerate output for first-run users.
        let o = OllamaParams::default();
        assert!(
            (0.0..=2.0).contains(&o.temperature),
            "ollama temp = {}",
            o.temperature
        );
        assert!(
            o.top_p > 0.0 && o.top_p <= 1.0,
            "ollama top_p = {}",
            o.top_p
        );
        assert!(o.top_k > 0, "ollama top_k must be > 0");
        assert!(o.repeat_penalty > 0.0, "ollama repeat_penalty must be > 0");
        assert!(o.num_ctx >= 512, "ollama num_ctx must be sane");

        let l = LokenParams::default();
        assert!(
            (0.0..=2.0).contains(&l.temperature),
            "LOKEN temp = {}",
            l.temperature
        );
        assert!(l.context_length >= 512, "LOKEN context too small");

        let p = OpenApiParams::default();
        assert!(
            (0.0..=2.0).contains(&p.temperature),
            "openapi temp = {}",
            p.temperature
        );
        assert!(
            p.top_p > 0.0 && p.top_p <= 1.0,
            "openapi top_p = {}",
            p.top_p
        );
        assert!(
            p.max_tokens.unwrap_or(0) > 0,
            "openapi max_tokens must default to >0"
        );
        // Frequency / presence penalty in OpenAI's [-2, 2] band.
        assert!((-2.0..=2.0).contains(&p.frequency_penalty));
        assert!((-2.0..=2.0).contains(&p.presence_penalty));
    }

    #[test]
    fn api_profile_new_initializes_all_param_categories() {
        // ApiProfile::new must initialize all 3 nested params
        // (ollama_params / openapi_params / loken_params) so the
        // Settings tab's param sliders never get None back when the
        // user switches profiles. Catches a future field add to
        // ApiProfile that forgets the new default wire-up.
        let p = ApiProfile::new(
            "Custom".into(),
            ApiType::Loken,
            "http://localhost:9999".into(),
            "test-model".into(),
        );
        assert_eq!(p.name, "Custom");
        assert_eq!(p.api_type, ApiType::Loken);
        assert_eq!(p.server_url, "http://localhost:9999");
        assert_eq!(p.model, "test-model");
        // Defaults of each nested param block. We don't check
        // every field — just confirm each block is present + the
        // post-migration temperature (0.15) is in place.
        assert!(
            (p.ollama_params.temperature - 0.15).abs() < 1e-6,
            "ollama temperature default must be 0.15 (post-migration)"
        );
        assert!(
            (p.loken_params.temperature - 0.15).abs() < 1e-6,
            "LOKEN temperature default must be 0.15 (post-migration)"
        );
    }

    #[test]
    fn app_config_roundtrips_through_json() {
        // Save/load relies on serde_json::to_string_pretty +
        // serde_json::from_str. Pin a roundtrip so a future
        // schema change that adds a required field without
        // #[serde(default)] breaks here instead of silently
        // erasing the user's saved config on next launch.
        let original = AppConfig::default();
        let json = serde_json::to_string_pretty(&original).expect("serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.profiles.len(), original.profiles.len());
        assert_eq!(back.selected_profile, original.selected_profile);
        assert_eq!(back.server_url, original.server_url);
        assert_eq!(back.dark_theme, original.dark_theme);
        assert_eq!(back.layer_mode, original.layer_mode);
    }

    #[test]
    fn layer_mode_persists_via_explicit_round_trip() {
        // The headline contract for the new field: a non-default
        // LayerMode survives serialize → deserialize so the user's
        // chat-tab toggle preference carries across GUI launches.
        let mut original = AppConfig::default();
        original.layer_mode = crate::state::LayerMode::Adaptive;

        let json = serde_json::to_string(&original).expect("serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            back.layer_mode,
            crate::state::LayerMode::Adaptive,
            "Adaptive must survive the round trip — otherwise the chat-tab \
             toggle resets to AllLayers on every launch"
        );
    }

    #[test]
    fn legacy_config_without_layer_mode_loads_as_default() {
        // Older saved configs predate the layer_mode field. Pin
        // that #[serde(default)] keeps them loading cleanly — the
        // missing field hydrates to LayerMode::default()
        // (AllLayers) instead of failing the whole deserialize
        // (which would have triggered the corrupt-config backup
        // path from 10ffc30 — recoverable but invasive).
        let legacy_json = r#"{
            "server_url": "http://localhost:11435",
            "dark_theme": true,
            "selected_model": null,
            "selected_model_source": null,
            "window_width": 1000.0,
            "window_height": 750.0,
            "window_maximized": true,
            "profiles": [],
            "selected_profile": null
        }"#;
        let parsed: AppConfig = serde_json::from_str(legacy_json)
            .expect("legacy config (missing layer_mode) must still deserialize");
        assert_eq!(
            parsed.layer_mode,
            crate::state::LayerMode::default(),
            "missing field hydrates to default, not Err"
        );
    }

    /// Make a fresh empty temp dir for save tests. Manual instead of
    /// pulling in the `tempfile` crate just for two tests — uuid is
    /// already a workspace dep for the rest of the GUI.
    fn fresh_tempdir(label: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("atelier-config-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("seed tempdir");
        path
    }

    #[test]
    fn save_to_dir_writes_json_at_expected_path() {
        // The save target must be <dir>/config.json — Settings tab
        // and external scripts both rely on that filename. A future
        // rename would silently strand the user's saved config.
        let dir = fresh_tempdir("happy");
        let cfg = AppConfig::default();
        let path = cfg.save_to_dir(&dir).expect("save_to_dir should succeed");

        assert_eq!(path, dir.join("config.json"));
        assert!(path.exists(), "config.json must exist on disk after save");

        // Round-trip: what's on disk must deserialize back to an
        // equal config. Guards against accidental lossy serde.
        let raw = std::fs::read_to_string(&path).expect("read back saved file");
        let parsed: AppConfig = serde_json::from_str(&raw).expect("parse saved");
        assert_eq!(parsed.profiles.len(), cfg.profiles.len());
        assert_eq!(parsed.selected_profile, cfg.selected_profile);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_to_dir_propagates_disk_failure_instead_of_silently_dropping() {
        // The prior `pub fn save(&self)` used `let _ = std::fs::write(...)`
        // and a permission-denied or read-only mount would be invisible:
        // the user clicked Save, nothing changed on disk, and they had
        // no way to find out. Now the Result surfaces the real cause.
        //
        // Strategy: point save_to_dir at a path that exists as a FILE,
        // not a directory. create_dir_all then fails with NotADirectory
        // (or AlreadyExists depending on platform) and we get a Err
        // whose message names the offending path.
        let parent = fresh_tempdir("disk-fail");
        let blocker = parent.join("not_a_directory.txt");
        std::fs::write(&blocker, b"i'm a file, not a dir").expect("seed blocker file");

        let cfg = AppConfig::default();
        let err = cfg
            .save_to_dir(&blocker)
            .expect_err("save_to_dir into a file path must fail, not silently succeed");

        assert!(
            err.contains(&blocker.display().to_string()),
            "error message must mention the offending path so users can act on it; got: {err}"
        );

        std::fs::remove_dir_all(&parent).ok();
    }

    #[test]
    fn load_from_path_returns_defaults_when_file_missing() {
        // First-run case: no config.json on disk yet. Must yield
        // AppConfig::default() so the GUI starts cleanly, NOT panic
        // or fail.
        let dir = fresh_tempdir("missing");
        let path = dir.join("config.json");
        assert!(!path.exists());

        let loaded = AppConfig::load_from_path(&path);
        // Default seeds 2 profiles (Default + Ollama) — same shape
        // the pre-existing default test pins.
        assert_eq!(loaded.profiles.len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_path_roundtrips_a_valid_config() {
        // The happy path: save + load yields an equal-shape config.
        // Catches accidental serde-skip / rename drift that would
        // silently drop user fields on the next launch.
        let dir = fresh_tempdir("roundtrip");
        let mut cfg = AppConfig::default();
        cfg.server_url = "http://example.com:11436".to_string();
        cfg.dark_theme = !cfg.dark_theme;
        cfg.save_to_dir(&dir).expect("seed disk");

        let loaded = AppConfig::load_from_path(&dir.join("config.json"));
        assert_eq!(loaded.server_url, "http://example.com:11436");
        assert_eq!(loaded.dark_theme, cfg.dark_theme);
        assert_eq!(loaded.profiles.len(), cfg.profiles.len());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_path_backs_up_corrupt_file_instead_of_wiping_user_data() {
        // The bug this guards: original load() used
        // `.and_then(serde_json::from_str(&s).ok()).unwrap_or_default()`
        // which silently fell back to defaults on parse failure —
        // wiping the user's profiles + URL + theme + selected model
        // on next launch with no clue what happened.
        //
        // Now: load_from_path detects the parse error, writes the
        // broken bytes to a sibling .broken.<ts> file, and returns
        // defaults. The user can rescue their data manually.
        let dir = fresh_tempdir("corrupt");
        let path = dir.join("config.json");
        let bogus = b"{not valid json at all";
        std::fs::write(&path, bogus).expect("seed bogus file");

        let loaded = AppConfig::load_from_path(&path);
        // Fell back to defaults (didn't panic, didn't return garbage).
        assert_eq!(loaded.profiles.len(), 2);

        // A .broken.<ts> backup exists alongside the original, and
        // contains the exact bogus bytes the user typed.
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .expect("read tempdir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                let n = e.file_name();
                let s = n.to_string_lossy();
                s.starts_with("config.json.broken.")
            })
            .collect();
        assert_eq!(
            backups.len(),
            1,
            "exactly one .broken.<ts> backup should be created for a corrupt config"
        );
        let backed_up = std::fs::read(backups[0].path()).expect("read backup");
        assert_eq!(
            backed_up, bogus,
            "backup must preserve the original bytes verbatim for manual recovery"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn next_profile_name_starts_from_profile_2_with_default_seed() {
        // Default seeds 2 profiles ("Default", "Ollama"). First
        // user-created profile should be "Profile 2" — neither
        // "Profile 1" (would look odd as the first user profile
        // when 2 seeds already exist) nor a collision with
        // anything.
        let existing = AppConfig::default().profiles;
        assert_eq!(AppConfig::next_profile_name(&existing), "Profile 2");
    }

    #[test]
    fn next_profile_name_avoids_collision_after_middle_delete() {
        // The headline bug the helper exists to fix: if the user
        // deletes "Default" from ["Default", "Profile 2"], the
        // previous `format!("Profile {}", len + 1)` returned
        // "Profile 2" — duplicating the existing name. The helper
        // probes upward until it finds an unused slot.
        let mut cfg = AppConfig::default();
        cfg.profiles.clear();
        cfg.profiles.push(ApiProfile::new(
            "Profile 2".to_string(),
            ApiType::Loken,
            "http://localhost:11435".to_string(),
            "default".to_string(),
        ));

        let next = AppConfig::next_profile_name(&cfg.profiles);
        assert_eq!(
            next, "Profile 3",
            "must skip the existing Profile 2 instead of duplicating"
        );
    }

    #[test]
    fn next_profile_name_skips_gaps() {
        // ["Profile 2", "Profile 5"] — next slot is 3, not 6.
        // Lowest unused wins.
        let mut cfg = AppConfig::default();
        cfg.profiles.clear();
        for n in [2usize, 5] {
            cfg.profiles.push(ApiProfile::new(
                format!("Profile {n}"),
                ApiType::Loken,
                "http://localhost:11435".to_string(),
                "default".to_string(),
            ));
        }
        assert_eq!(AppConfig::next_profile_name(&cfg.profiles), "Profile 3");
    }

    #[test]
    fn backup_before_reset_copies_existing_config_with_unique_suffix() {
        // Pin the reset-undo flow: an existing config.json gets
        // copied to a sibling .before_reset.<ts> file, so a user
        // who hits "Reset" by accident can restore manually.
        // Bytes preserved verbatim.
        let dir = fresh_tempdir("reset-backup");
        let src = dir.join("config.json");
        let original_bytes = br#"{"server_url":"http://test:11435"}"#;
        std::fs::write(&src, original_bytes).expect("seed config");

        AppConfig::backup_before_reset(&dir);

        let backups: Vec<_> = std::fs::read_dir(&dir)
            .expect("read tempdir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("config.json.before_reset.")
            })
            .collect();
        assert_eq!(
            backups.len(),
            1,
            "exactly one .before_reset.<ts> backup should be created"
        );
        let backed_up = std::fs::read(backups[0].path()).expect("read backup");
        assert_eq!(
            backed_up, original_bytes,
            "backup must preserve the original bytes verbatim for restore"
        );
        // Source untouched — reset itself hasn't run yet.
        assert_eq!(std::fs::read(&src).expect("source"), original_bytes);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn backup_before_reset_is_noop_when_no_config_yet() {
        // First-run reset (or reset after the user deleted
        // config.json manually) — nothing to back up, no panic.
        let dir = fresh_tempdir("reset-noop");
        assert!(!dir.join("config.json").exists());

        AppConfig::backup_before_reset(&dir);

        let backups: Vec<_> = std::fs::read_dir(&dir)
            .expect("read tempdir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("config.json.before_reset.")
            })
            .collect();
        assert!(
            backups.is_empty(),
            "missing config.json must not generate an empty .before_reset backup"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A config written by an OLDER build must still load.
    ///
    /// This is the regression that cost a user their settings AND their stored prompts:
    /// one field was added to a persisted struct without a default, so serde rejected the
    /// WHOLE file - not the field, not the section, the file - and the app came up empty.
    /// The JSON below is deliberately minimal: it names only fields that existed long
    /// before, so any field added from now on has to be tolerated or this fails.
    #[test]
    fn a_config_from_an_older_build_still_loads() {
        let dir = fresh_tempdir("older-build");
        let path = dir.join("config.json");
        std::fs::write(
            &path,
            r#"{
              "server_url": "http://127.0.0.1:11435",
              "dark_theme": true,
              "selected_model": "qwen3:8b",
              "profiles": [{
                "name": "Default",
                "api_type": "Loken",
                "server_url": "http://127.0.0.1:11435",
                "model": "qwen3:8b"
              }],
              "media": { "prompt": "a cat", "image": { "model": "z-image", "width": 1024 } }
            }"#,
        )
        .unwrap();
        let loaded = AppConfig::load_from_path(&path);
        // The values from the file survived - a fallback to defaults would lose them,
        // which is exactly the failure and would leave `selected_model` empty.
        assert_eq!(loaded.selected_model.as_deref(), Some("qwen3:8b"));
        assert_eq!(loaded.profiles.len(), 1);
        assert_eq!(loaded.profiles[0].name, "Default");
        let media = loaded.media.expect("the media section survived");
        assert_eq!(media.prompt, "a cat");
        let image = media.image.expect("the image section survived");
        assert_eq!(image.width, 1024);
        // And nothing was quarantined: a backup file means the parse failed.
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("broken"))
            .collect();
        assert!(
            backups.is_empty(),
            "the old config was rejected, not loaded"
        );
    }

    #[test]
    fn load_from_path_skips_backup_for_empty_file() {
        // A 0-byte config.json is the symptom of a crashed write
        // mid-save. There's nothing to back up — backing up
        // emptiness would only litter the config dir with .broken
        // siblings. Verify the load still recovers (returns defaults)
        // and DOES NOT create a backup.
        let dir = fresh_tempdir("empty");
        let path = dir.join("config.json");
        std::fs::write(&path, b"").expect("seed empty file");

        let loaded = AppConfig::load_from_path(&path);
        assert_eq!(loaded.profiles.len(), 2);

        let backups: Vec<_> = std::fs::read_dir(&dir)
            .expect("read tempdir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("config.json.broken.")
            })
            .collect();
        assert!(
            backups.is_empty(),
            "empty config.json should NOT generate a .broken backup — \
             nothing of value to preserve"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
