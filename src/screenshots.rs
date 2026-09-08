//! The screenshots the documentation shows, drawn by running the application.
//!
//! `cargo test --release screenshots -- --ignored` writes them under `docs/img/`. Ignored by
//! default: they write files and need a graphics adapter, where the rest of the suite runs
//! anywhere.
//!
//! The harness drives the real `eframe::App`, so a shot contains the window as it is built -
//! the top bar, the sidebar, the tab strip and the panel - rather than one panel rendered on
//! a bare background. Reproducing the shell in the test instead would photograph a copy of
//! the layout, and a copy drifts from what ships.
//!
//! The state is overwritten after construction. `AppConfig::load` reads the developer's own
//! configuration off disk, and a screenshot must not depend on it, nor carry a server address
//! someone happens to have saved.

#![cfg(test)]

use egui_kittest::Harness;

use crate::api::types::ModelInfo;
use crate::app::{Args, LLMGuiApp};
use crate::config::AppConfig;
use crate::log_buffer::{LogBuffer, LogEntry, LogLevel};
use crate::state::{ChatMessage, MessageTiming, Section};

const OUT: &str = "docs/img";
const WINDOW: (f32, f32) = (1280.0, 860.0);

fn model(name: &str, size: &str, family: &str, caps: &[&str]) -> ModelInfo {
    ModelInfo {
        name: name.into(),
        size: size.into(),
        size_bytes: 0,
        modified_at: "2026-08-30T12:00:00Z".into(),
        source: "ollama".into(),
        family: family.into(),
        capabilities: caps.iter().map(|c| (*c).to_string()).collect(),
        defaults: None,
    }
}

fn shoot(name: &str, section: Section, dress: impl FnOnce(&mut LLMGuiApp) + 'static) {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(WINDOW.0, WINDOW.1))
        .build_eframe(move |cc| {
            let mut app = LLMGuiApp::new(cc, Args { model: None, server: None }, LogBuffer::new(256));
            app.config = AppConfig::default();
            app.current_section = section;
            app.sidebar_expanded = true;
            app.connection_status = crate::state::ConnectionStatus::connected("2 models loaded");
            app.models.available_models = vec![
                model("qwen3:8b", "5.2 GB", "qwen3", &["chat"]),
                model("llama3.2:1b", "1.3 GB", "llama", &["chat"]),
                model("z-image", "12.8 GB", "z-image", &["image"]),
            ];
            app.models.loaded_models = vec!["qwen3:8b".into()];
            app.models.selected_model = Some("qwen3:8b".into());
            dress(&mut app);
            app
        });
    // Without the loaders every icon resolves to a missing-glyph box, which is the exact
    // failure the SVG icons exist to prevent.
    egui_extras::install_image_loaders(&harness.ctx);
    // A fixed number of steps, never `run`: a panel with a spinner asks for another frame
    // forever, and `run` gives up rather than returning one to photograph.
    harness.run_steps(6);
    std::fs::create_dir_all(OUT).expect("docs/img");
    harness
        .render()
        .expect("a graphics adapter")
        .save(format!("{OUT}/{name}.png"))
        .unwrap_or_else(|e| panic!("write {name}.png: {e}"));
}

/// A conversation mid-answer. An empty chat shows the placeholder and none of what the tab is
/// for, so the fixture is a exchange with a reply still streaming.
#[test]
#[ignore = "writes docs/img and needs a graphics adapter"]
fn chat() {
    shoot("atelier-chat", Section::Chat, |app| {
        app.chat.messages.push_back(ChatMessage {
            role: "user".into(),
            content: "Explain what a KV cache holds, briefly.".into(),
            timestamp: "14:02".into(),
            ..Default::default()
        });
        app.chat.messages.push_back(ChatMessage {
            role: "assistant".into(),
            content: "It holds the keys and values already computed for every token in the \
                      context, so each new token attends over them instead of recomputing the \
                      whole prefix. Its size grows with the context length, not with the \
                      prompt you just sent."
                .into(),
            timestamp: "14:02".into(),
            timing: Some(MessageTiming {
                tokens_per_sec: 148.6,
                duration_ms: 1240,
                token_count: 184,
            }),
            ..Default::default()
        });
        app.chat.messages.push_back(ChatMessage {
            role: "user".into(),
            content: "Does a second question reuse it?".into(),
            timestamp: "14:03".into(),
            ..Default::default()
        });
        app.chat.is_generating = true;
        app.chat.streaming_content =
            "Yes, as long as the prefix matches. The shared part is kept and only".into();
    });
}

/// Media Studio on the image panel, with a prompt and the controls a render actually uses.
#[test]
#[ignore = "writes docs/img and needs a graphics adapter"]
fn media_studio() {
    shoot("atelier-media", Section::MediaStudio, |app| {
        app.media.kind = crate::state::MediaKind::Image;
        app.media.prompt = "a lighthouse on a basalt shore, low sun, long exposure".into();
    });
}

/// The model list with one model loaded, and the import panel under it.
#[test]
#[ignore = "writes docs/img and needs a graphics adapter"]
fn models() {
    shoot("atelier-models", Section::Models, |_| {});
}

/// The server log with a line of every level, one of them a warning and one an error.
#[test]
#[ignore = "writes docs/img and needs a graphics adapter"]
fn logs() {
    shoot("atelier-logs", Section::ServerLog, |app| {
        let lines: [(LogLevel, &str, &str, &str); 8] = [
            (LogLevel::Info, "14:02:03.118", "atelier::api", "GET /api/tags 200 in 12 ms"),
            (LogLevel::Info, "14:02:03.402", "atelier::models", "3 models listed, 1 loaded"),
            (LogLevel::Debug, "14:02:11.204", "atelier::chat", "streaming reply from qwen3:8b"),
            (LogLevel::Trace, "14:02:11.219", "atelier::api", "chunk 1 of the reply, 24 bytes"),
            (LogLevel::Info, "14:02:12.480", "atelier::chat", "reply complete: 184 tokens in 1.2 s"),
            (LogLevel::Warn, "14:02:40.011", "atelier::models", "z-image is not loaded; the first render will load it"),
            (LogLevel::Error, "14:03:02.377", "atelier::api", "POST /v1/images/generations 503: no free device"),
            (LogLevel::Info, "14:03:05.000", "atelier::media", "render cancelled by the user"),
        ];
        for (level, at, target, message) in lines {
            let mut entry = LogEntry::new(level, target.into(), message.into());
            entry.timestamp = at.into();
            app.log_buffer.push(entry);
        }
    });
}

/// Which of the literal glyphs the source uses actually resolve in the bundled fonts. A
/// missing glyph draws as a box, which is what the SVG icons exist to prevent, so this
/// renders each one and reports the ones that came out identical to the replacement
/// character. Re-run it after a font change; `icons::tests::MISSING_GLYPHS` is its result.
#[test]
#[ignore = "diagnostic"]
fn which_literal_glyphs_resolve() {
    for (name, glyph) in [
        ("U+25CF circle", "\u{25CF}"),
        ("U+25C6 diamond", "\u{25C6}"),
        ("U+2022 bullet", "\u{2022}"),
        ("U+2715 cross", "\u{2715}"),
        ("U+2193 down", "\u{2193}"),
        ("U+2191 up", "\u{2191}"),
        ("U+00B7 middot", "\u{00B7}"),
        ("U+00AB laquo", "\u{00AB}"),
        ("U+00BB raquo", "\u{00BB}"),
        ("U+25CB white circle", "\u{25CB}"),
        ("U+2588 block", "\u{2588}"),
        ("U+00D7 times", "\u{00D7}"),
        ("U+FFFD replacement", "\u{FFFD}"),
    ] {
        let g = glyph.to_string();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(64.0, 64.0))
            .build_ui(move |ui| {
                ui.label(egui::RichText::new(&g).size(40.0));
            });
        harness.run_steps(2);
        let image = harness.render().expect("adapter");
        let ink = image.pixels().filter(|p| p.0[3] > 8).count();
        println!("{name}: {ink} inked pixels");
    }
}
