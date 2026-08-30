//! Server configuration: the data types and their serde wrappers.
//!
//! Rendering lives in `ui::settings::render`, which reads `ConfigEditorState`
//! directly and emits `SettingsAction::SaveConfig`.

/// Server configuration (local copy for GUI editing)
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// What the server declares here and this editor does not show. Every struct needs its
    /// own: a passthrough on the parent does not catch unknown keys inside a table the parent
    /// names, which left seven keys still being deleted after the first fix - including
    /// `require_auth` and `api_keys`.
    #[serde(flatten)]
    pub passthrough: std::collections::BTreeMap<String, toml::Value>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 11435,
            passthrough: Default::default(),
        }
    }
}

/// Inference configuration from TOML file (local copy for GUI editing)
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct InferenceConfigToml {
    pub model_id: String,
    pub model_source: Option<String>,
    pub max_tokens: Option<usize>,
    pub context_length: Option<usize>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<usize>,
    pub seed: Option<u64>,
    pub device_index: Option<usize>,
    pub max_gpu_memory_fraction: Option<f64>,
    pub force_gpu_layers: Option<usize>,
    pub use_quantized_gpu: Option<bool>,
    pub cpu_threads: Option<usize>,
    pub disable_arc_layers: Option<bool>,
    /// What the server declares here and this editor does not show. Every struct needs its
    /// own: a passthrough on the parent does not catch unknown keys inside a table the parent
    /// names, which left seven keys still being deleted after the first fix - including
    /// `require_auth` and `api_keys`.
    #[serde(flatten)]
    pub passthrough: std::collections::BTreeMap<String, toml::Value>,
}

/// Root configuration structure (local copy for GUI editing)
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub server: Option<ServerConfig>,
    pub inference: InferenceConfigToml,
    pub ollama_models_dir: Option<String>,
    pub huggingface_models_dir: Option<String>,
    /// Everything the server understands and this editor does not.
    ///
    /// Without it a save DELETES what it cannot show, because serialising this struct writes
    /// exactly its own fields: opening Settings on a clustered node and pressing save dropped
    /// the whole `[cluster]` block, the whole `[energy]` block, and every authentication and
    /// rate-limit setting - eighteen keys out of thirty-two. Carrying the rest through is the
    /// fix that does not need a second hand-maintained copy of the server's schema, which is
    /// what drifted in the first place.
    #[serde(flatten)]
    pub passthrough: std::collections::BTreeMap<String, toml::Value>,
}

impl Config {
    pub fn server(&self) -> ServerConfig {
        self.server.clone().unwrap_or_default()
    }

    pub fn load_default() -> anyhow::Result<Self> {
        let cwd = std::path::Path::new("config.toml");
        if cwd.exists() {
            let content = std::fs::read_to_string(cwd)?;
            let config: Config = toml::from_str(&content)?;
            return Ok(config);
        }
        if let Ok(exe) = std::env::current_exe() {
            let exe_dir = exe.parent().unwrap_or(std::path::Path::new(".")).join("config.toml");
            if exe_dir.exists() {
                let content = std::fs::read_to_string(&exe_dir)?;
                let config: Config = toml::from_str(&content)?;
                return Ok(config);
            }
        }
        anyhow::bail!("config.toml not found")
    }

    pub fn save_default(&self) -> anyhow::Result<()> {
        let toml_string = toml::to_string_pretty(self)?;
        std::fs::write("config.toml", toml_string)?;
        Ok(())
    }
}

// ConfigEditorAction + render() removed — the inline config-editing
// UI lives in ui::settings::render and emits SettingsAction::SaveConfig
// directly. The modal-form renderer this enum drove was duplicate UI.

/// Configuration editor state
#[derive(Debug, Clone)]
pub struct ConfigEditorState {
    // Model paths
    pub ollama_models_dir: String,
    pub huggingface_models_dir: String,

    // Server settings
    pub server_host: String,
    pub server_port_str: String,

    // Inference parameters
    pub model_id: String,
    pub max_tokens_str: String,
    pub context_length_str: String,
    pub temperature_str: String,
    pub top_p_str: String,
    pub top_k_str: String,
    pub seed_str: String,
    pub device_index_str: String,
    pub max_gpu_memory_fraction_str: String,
    pub force_gpu_layers_str: String,
    pub use_quantized_gpu: bool,
    pub cpu_threads_str: String,

    /// Everything the loaded file held that this editor has no control for, kept so that a
    /// save writes it back untouched. One per section, because an unknown key inside
    /// `[server]` is not carried by a passthrough on the root. Without these, saving is
    /// destructive: see the test at the end of this file.
    pub passthrough: std::collections::BTreeMap<String, toml::Value>,
    pub server_passthrough: std::collections::BTreeMap<String, toml::Value>,
    pub inference_passthrough: std::collections::BTreeMap<String, toml::Value>,

    // UI state
    pub error_message: Option<String>,

    /// State for the in-app folder picker — opens as a modal when the
    /// user clicks Browse on a model-directory input. (target, current
    /// path being browsed). None = picker closed. Pure-egui; no
    /// external file-dialog backend involved.
    pub inapp_picker: Option<(BrowseTarget, std::path::PathBuf)>,
}

/// Identifies which Browse button is being serviced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseTarget {
    OllamaDir,
    HuggingFaceDir,
}

impl ConfigEditorState {
    /// Create from existing config
    pub fn from_config(config: &Config) -> Self {
        let server = config.server();
        let inference = &config.inference;

        Self {
            ollama_models_dir: config
                .ollama_models_dir
                .clone()
                .unwrap_or_default(),
            huggingface_models_dir: config
                .huggingface_models_dir
                .clone()
                .unwrap_or_default(),

            server_host: server.host,
            server_port_str: server.port.to_string(),

            model_id: inference.model_id.clone(),
            max_tokens_str: inference.max_tokens.unwrap_or(2048).to_string(),
            context_length_str: inference.context_length.unwrap_or(4096).to_string(),
            temperature_str: inference.temperature.unwrap_or(0.7).to_string(),
            top_p_str: inference.top_p.unwrap_or(0.9).to_string(),
            top_k_str: inference.top_k.unwrap_or(50).to_string(),
            seed_str: inference.seed.unwrap_or(42).to_string(),
            device_index_str: inference.device_index.map(|d| d.to_string()).unwrap_or_default(),
            max_gpu_memory_fraction_str: inference.max_gpu_memory_fraction.unwrap_or(0.9).to_string(),
            force_gpu_layers_str: inference.force_gpu_layers.map(|l| l.to_string()).unwrap_or_default(),
            use_quantized_gpu: inference.use_quantized_gpu.unwrap_or(true),
            cpu_threads_str: inference.cpu_threads.unwrap_or(0).to_string(),

            passthrough: config.passthrough.clone(),
            server_passthrough: server.passthrough.clone(),
            inference_passthrough: inference.passthrough.clone(),
            error_message: None,
            inapp_picker: None,
        }
    }

    /// Convert to Config struct
    pub fn to_config(&self) -> Result<Config, String> {
        let server_port: u16 = self.server_port_str.parse()
            .map_err(|_| "Invalid server port (must be 0-65535)".to_string())?;

        let max_tokens: usize = self.max_tokens_str.parse()
            .map_err(|_| "Invalid max_tokens (must be a positive integer)".to_string())?;

        let context_length: usize = self.context_length_str.parse()
            .map_err(|_| "Invalid context_length (must be a positive integer)".to_string())?;

        let temperature: f64 = self.temperature_str.parse()
            .map_err(|_| "Invalid temperature (must be a number)".to_string())?;
        // Temperature is a softmax scale: zero means greedy, negatives
        // would flip the distribution upside-down, > 2.0 produces
        // near-uniform sampling nobody actually wants. Clamp range
        // matches the server-side cap.
        if !(0.0..=2.0).contains(&temperature) {
            return Err("temperature out of range (must be 0.0..=2.0)".to_string());
        }

        let top_p: f64 = self.top_p_str.parse()
            .map_err(|_| "Invalid top_p (must be a number)".to_string())?;
        if !(0.0..=1.0).contains(&top_p) {
            return Err("top_p out of range (must be 0.0..=1.0)".to_string());
        }

        let top_k: usize = self.top_k_str.parse()
            .map_err(|_| "Invalid top_k (must be a positive integer)".to_string())?;

        let seed: u64 = self.seed_str.parse()
            .map_err(|_| "Invalid seed (must be a positive integer)".to_string())?;

        let device_index = if self.device_index_str.is_empty() {
            None
        } else {
            Some(self.device_index_str.parse::<usize>()
                .map_err(|_| "Invalid device_index (must be a positive integer)".to_string())?)
        };

        let max_gpu_memory_fraction: f64 = self.max_gpu_memory_fraction_str.parse()
            .map_err(|_| "Invalid max_gpu_memory_fraction (must be a number 0.0-1.0)".to_string())?;
        // The error message already promised 0..=1; enforce it here
        // instead of silently accepting 1.5 and surprising the user
        // when the server rejects the saved config on next start.
        if !(0.0..=1.0).contains(&max_gpu_memory_fraction) {
            return Err("max_gpu_memory_fraction out of range (must be 0.0..=1.0)".to_string());
        }

        let force_gpu_layers = if self.force_gpu_layers_str.is_empty() {
            None
        } else {
            Some(self.force_gpu_layers_str.parse::<usize>()
                .map_err(|_| "Invalid force_gpu_layers (must be a positive integer)".to_string())?)
        };

        let cpu_threads: usize = self.cpu_threads_str.parse()
            .map_err(|_| "Invalid cpu_threads (must be a positive integer)".to_string())?;

        Ok(Config {
            passthrough: self.passthrough.clone(),
            server: Some(ServerConfig {
                host: self.server_host.clone(),
                port: server_port,
                passthrough: self.server_passthrough.clone(),
            }),
            inference: InferenceConfigToml {
                passthrough: self.inference_passthrough.clone(),
                model_id: self.model_id.clone(),
                model_source: None,
                max_tokens: Some(max_tokens),
                context_length: Some(context_length),
                temperature: Some(temperature),
                top_p: Some(top_p),
                top_k: Some(top_k),
                seed: Some(seed),
                device_index,
                max_gpu_memory_fraction: Some(max_gpu_memory_fraction),
                force_gpu_layers,
                use_quantized_gpu: Some(self.use_quantized_gpu),
                cpu_threads: Some(cpu_threads),
                disable_arc_layers: None,
            },
            ollama_models_dir: if self.ollama_models_dir.is_empty() {
                None
            } else {
                Some(self.ollama_models_dir.clone())
            },
            huggingface_models_dir: if self.huggingface_models_dir.is_empty() {
                None
            } else {
                Some(self.huggingface_models_dir.clone())
            },
        })
    }
}

#[cfg(test)]
mod to_config_range_tests {
    use super::*;

    /// Minimal valid editor state — all parses succeed, all ranges
    /// pass. Tests below clone this and tweak one field to verify
    /// individual range checks fire without false-positives from
    /// unrelated fields.
    fn valid() -> ConfigEditorState {
        ConfigEditorState {
            ollama_models_dir: String::new(),
            huggingface_models_dir: String::new(),
            server_host: "127.0.0.1".into(),
            server_port_str: "11435".into(),
            model_id: String::new(),
            max_tokens_str: "2048".into(),
            context_length_str: "4096".into(),
            temperature_str: "0.7".into(),
            top_p_str: "0.9".into(),
            top_k_str: "50".into(),
            seed_str: "42".into(),
            device_index_str: String::new(),
            max_gpu_memory_fraction_str: "0.9".into(),
            force_gpu_layers_str: String::new(),
            use_quantized_gpu: true,
            cpu_threads_str: "0".into(),
            passthrough: Default::default(),
            server_passthrough: Default::default(),
            inference_passthrough: Default::default(),
            error_message: None,
            inapp_picker: None,
        }
    }

    #[test]
    fn valid_baseline_round_trips_through_to_config() {
        // Sanity check — without any tweaks the default state must
        // pass all validations. If this fails the rest of the
        // module's range tests can't distinguish a false-positive
        // (broken baseline) from a real range catch.
        assert!(valid().to_config().is_ok());
    }

    #[test]
    fn temperature_rejects_negative() {
        let mut s = valid();
        s.temperature_str = "-0.1".into();
        let err = s.to_config().unwrap_err();
        assert!(err.contains("temperature"), "got: {}", err);
    }

    #[test]
    fn temperature_rejects_above_two() {
        let mut s = valid();
        s.temperature_str = "2.5".into();
        let err = s.to_config().unwrap_err();
        assert!(err.contains("temperature"), "got: {}", err);
    }

    #[test]
    fn top_p_rejects_above_one() {
        let mut s = valid();
        s.top_p_str = "1.5".into();
        let err = s.to_config().unwrap_err();
        assert!(err.contains("top_p"), "got: {}", err);
    }

    #[test]
    fn gpu_fraction_rejects_above_one() {
        let mut s = valid();
        s.max_gpu_memory_fraction_str = "1.2".into();
        let err = s.to_config().unwrap_err();
        assert!(err.contains("max_gpu_memory_fraction"), "got: {}", err);
    }

    #[test]
    fn server_port_rejects_overflow_and_nonnumeric() {
        let mut s = valid();
        s.server_port_str = "abc".into();
        assert!(s.to_config().unwrap_err().contains("port"));

        // u16::MAX is 65535 — overflow surfaces as a parse error too.
        let mut s = valid();
        s.server_port_str = "70000".into();
        assert!(s.to_config().unwrap_err().contains("port"));
    }

    #[test]
    fn integer_fields_reject_negative_input() {
        // max_tokens / context_length / top_k / cpu_threads are usize.
        // A leading '-' makes them un-parseable (usize doesn't accept
        // negatives), so the error must surface with the field name.
        let check = |field: &str, err: String| {
            assert!(err.contains(field), "{field}: got {err}");
        };
        let mut s = valid();
        s.max_tokens_str = "-1".into();
        check("max_tokens", s.to_config().unwrap_err());

        let mut s = valid();
        s.context_length_str = "-1".into();
        check("context_length", s.to_config().unwrap_err());

        let mut s = valid();
        s.top_k_str = "-1".into();
        check("top_k", s.to_config().unwrap_err());

        let mut s = valid();
        s.cpu_threads_str = "-1".into();
        check("cpu_threads", s.to_config().unwrap_err());
    }

    #[test]
    fn temperature_rejects_nonnumeric_input() {
        let mut s = valid();
        s.temperature_str = "warm".into();
        let err = s.to_config().unwrap_err();
        assert!(err.contains("temperature"), "got: {}", err);
    }

    #[test]
    fn optional_fields_treat_empty_string_as_none() {
        // device_index + force_gpu_layers are Option<usize>; an empty
        // string must map to None (no override), not produce a parse
        // error. The GUI's "leave blank to auto-pick" UX depends on this.
        let mut s = valid();
        s.device_index_str = "".into();
        s.force_gpu_layers_str = "".into();
        let cfg = s.to_config().expect("empty optional fields must validate");
        assert_eq!(cfg.inference.device_index, None);
        assert_eq!(cfg.inference.force_gpu_layers, None);
    }

    #[test]
    fn optional_fields_reject_garbage_when_non_empty() {
        let mut s = valid();
        s.device_index_str = "first".into();
        let err = s.to_config().unwrap_err();
        assert!(err.contains("device_index"), "got: {}", err);
    }

    #[test]
    fn from_config_applies_documented_defaults_for_missing_options() {
        // Config TOML allows omitting most inference knobs — the GUI
        // backfills with sensible defaults so the editor never opens
        // with empty strings the user has to fill in. Pin every
        // Option<…>::unwrap_or default so a quiet change reaches a
        // power user as a value swap, not a silent reset.
        let empty_inference = InferenceConfigToml {
            passthrough: Default::default(),
            model_id: "llama3:latest".into(),
            model_source: None,
            max_tokens: None,
            context_length: None,
            temperature: None,
            top_p: None,
            top_k: None,
            seed: None,
            device_index: None,
            max_gpu_memory_fraction: None,
            force_gpu_layers: None,
            use_quantized_gpu: None,
            cpu_threads: None,
            disable_arc_layers: None,
        };
        let cfg = Config {
            passthrough: Default::default(),
            server: None,
            inference: empty_inference,
            ollama_models_dir: None,
            huggingface_models_dir: None,
        };
        let s = ConfigEditorState::from_config(&cfg);
        // Server falls back to 127.0.0.1:11435.
        assert_eq!(s.server_host, "127.0.0.1");
        assert_eq!(s.server_port_str, "11435");
        // Inference defaults — pin every backfilled value.
        assert_eq!(s.model_id, "llama3:latest");
        assert_eq!(s.max_tokens_str, "2048");
        assert_eq!(s.context_length_str, "4096");
        assert_eq!(s.temperature_str, "0.7");
        assert_eq!(s.top_p_str, "0.9");
        assert_eq!(s.top_k_str, "50");
        assert_eq!(s.seed_str, "42");
        assert_eq!(s.max_gpu_memory_fraction_str, "0.9");
        assert!(s.use_quantized_gpu, "use_quantized_gpu defaults to true");
        assert_eq!(s.cpu_threads_str, "0");
        // Optional integer fields: None → empty string (the "leave
        // blank to auto" UX path that's tested separately).
        assert_eq!(s.device_index_str, "");
        assert_eq!(s.force_gpu_layers_str, "");
        assert_eq!(s.ollama_models_dir, "");
        assert_eq!(s.huggingface_models_dir, "");
        // The defaulted state must also pass to_config() — otherwise
        // a fresh editor session can't save without the user touching
        // anything.
        assert!(s.to_config().is_ok(), "from_config defaults must round-trip");
    }

    #[test]
    fn from_config_preserves_user_supplied_values() {
        // The other half of the contract: when the TOML *does* set
        // a value, from_config must thread it through unchanged
        // (not silently replace it with the default).
        let inf = InferenceConfigToml {
            passthrough: Default::default(),
            model_id: "qwen3-coder:30b".into(),
            model_source: Some("ollama".into()),
            max_tokens: Some(8192),
            context_length: Some(16384),
            temperature: Some(0.3),
            top_p: Some(0.95),
            top_k: Some(20),
            seed: Some(1234),
            device_index: Some(1),
            max_gpu_memory_fraction: Some(0.75),
            force_gpu_layers: Some(28),
            use_quantized_gpu: Some(false),
            cpu_threads: Some(8),
            disable_arc_layers: Some(true),
        };
        let cfg = Config {
            passthrough: Default::default(),
            server: Some(ServerConfig { host: "0.0.0.0".into(), port: 8080, passthrough: Default::default() }),
            inference: inf,
            ollama_models_dir: Some("/data/ollama".into()),
            huggingface_models_dir: Some("/data/hf".into()),
        };
        let s = ConfigEditorState::from_config(&cfg);
        assert_eq!(s.server_host, "0.0.0.0");
        assert_eq!(s.server_port_str, "8080");
        assert_eq!(s.model_id, "qwen3-coder:30b");
        assert_eq!(s.max_tokens_str, "8192");
        assert_eq!(s.context_length_str, "16384");
        assert_eq!(s.temperature_str, "0.3");
        assert_eq!(s.top_p_str, "0.95");
        assert_eq!(s.top_k_str, "20");
        assert_eq!(s.seed_str, "1234");
        assert_eq!(s.device_index_str, "1");
        assert_eq!(s.max_gpu_memory_fraction_str, "0.75");
        assert_eq!(s.force_gpu_layers_str, "28");
        assert!(!s.use_quantized_gpu);
        assert_eq!(s.cpu_threads_str, "8");
        assert_eq!(s.ollama_models_dir, "/data/ollama");
        assert_eq!(s.huggingface_models_dir, "/data/hf");
    }

    #[test]
    fn temperature_at_zero_and_two_is_accepted_inclusive() {
        // Range is documented inclusive (0.0..=2.0). Boundary values
        // must round-trip without error so a power-user can pick exact
        // greedy (0.0) or max-uniform (2.0).
        let mut s = valid();
        s.temperature_str = "0.0".into();
        assert!(s.to_config().is_ok());

        let mut s = valid();
        s.temperature_str = "2.0".into();
        assert!(s.to_config().is_ok());
    }
}

#[cfg(test)]
mod every_config_field_reaches_the_editor {
    //! The server's `config.toml` is the serialised state this editor claims to edit. This
    //! walks it key by key and fails on any key the editor cannot carry - the shape of test
    //! that, in another catalogue of editors, found a missing control in almost every one.
    //!
    //! It walks LEAVES, including inside tables: a shallow walk over top-level keys finds
    //! nothing, because everything interesting lives under `[server]`, `[inference]`,
    //! `[energy]` and `[cluster]`.

    use super::Config;

    /// Every leaf of a TOML document, as `section.key`.
    fn leaves(doc: &toml::Value, prefix: &str, out: &mut Vec<String>) {
        match doc {
            toml::Value::Table(t) => {
                for (k, v) in t {
                    let path = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                    leaves(v, &path, out);
                }
            }
            _ => out.push(prefix.to_string()),
        }
    }

    /// A config carrying every field the server declares, one node would really write.
    const FULL: &str = r#"
ollama_models_dir = "/models/ollama"
huggingface_models_dir = "/models/hf"
lora_dir = "/models/lora"

[server]
host = "0.0.0.0"
port = 11435
require_auth = true
api_keys = ["k"]
allowed_origins = ["https://example.test"]
rate_limit_per_minute = 60
rate_limit_burst = 10

[inference]
model_id = "qwen3:1.7b"
max_tokens = 2048
context_length = 4096
temperature = 0.15
top_p = 0.9
top_k = 50
seed = 42
max_gpu_memory_fraction = 0.95
use_quantized_gpu = true
cpu_threads = 0
kv_quant = "q8"
continuous_batching = true

[energy]
enabled = true
carbon_intensity = 55.0
cpu_tdp_w = 125.0
water_l_per_kwh = 1.8

[cluster]
name = "home"
node_id = "desktop"
advertise = "http://192.0.2.10:11435"
join = ["http://192.0.2.11:11435"]
gossip_interval_ms = 1000
min_speedup = 1.15
"#;

    /// Load a full config, save it back, and compare key sets. A field the editor does not
    /// know is a field the editor DELETES: `save_default` serialises its own struct, so the
    /// round trip is destructive, not merely incomplete. Someone who opens Settings on a
    /// clustered node and presses save loses the whole `[cluster]` block and every
    /// authentication setting.
    #[test]
    fn saving_a_config_does_not_drop_what_the_editor_cannot_show() {
        let original: toml::Value = toml::from_str(FULL).expect("fixture parses");
        let mut before = Vec::new();
        leaves(&original, "", &mut before);

        let parsed: Config = toml::from_str(FULL).expect("the editor parses a full config");
        let round_tripped = toml::to_string_pretty(&parsed).expect("serialise");
        let after_doc: toml::Value = toml::from_str(&round_tripped).expect("re-parse");
        let mut after = Vec::new();
        leaves(&after_doc, "", &mut after);

        let lost: Vec<&String> = before.iter().filter(|k| !after.contains(k)).collect();
        assert!(
            lost.is_empty(),
            "{} of {} keys are dropped by a save: {lost:?}",
            lost.len(),
            before.len()
        );
    }
}
