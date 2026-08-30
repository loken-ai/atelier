//! The screenshots the documentation shows, drawn by the code that draws the application.
//!
//! `cargo test --release screenshots -- --ignored` writes them under `docs/img/`. Ignored by
//! default: it writes files and asks for a graphics adapter, where the rest of the suite runs
//! anywhere.
//!
//! A capture taken by hand drifts from the build the moment either moves, and it carries
//! whatever happened to be on screen - a server address, a prompt someone typed, a file path
//! with a name in it. These render the real panels from state written below, so a screenshot
//! cannot show anything this file did not put in it.

#![cfg(test)]

use std::collections::HashMap;

use egui_kittest::Harness;

const OUT: &str = "docs/img";

fn shoot(name: &str, size: (f32, f32), build: impl FnMut(&mut egui::Ui) + 'static) {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(size.0, size.1))
        .build_ui(build);
    crate::theme::apply(&harness.ctx, true);
    // Without the loaders every icon resolves to a missing-glyph box, which is the exact
    // failure the SVG icons exist to avoid - and a screenshot showing it would be a screenshot
    // of the harness, not of the application.
    egui_extras::install_image_loaders(&harness.ctx);
    // A fixed number of steps, never `run`: a panel with a spinner in it asks for another
    // frame forever, and `run` gives up rather than returning one to photograph.
    harness.run_steps(4);
    std::fs::create_dir_all(OUT).expect("docs/img");
    harness
        .render()
        .expect("a graphics adapter")
        .save(format!("{OUT}/{name}.png"))
        .unwrap_or_else(|e| panic!("write {name}.png: {e}"));
}

/// A conversation mid-answer: a question, an answer with its timing, and the model still
/// streaming. An empty chat shows the placeholder and none of what the tab is for.
#[test]
#[ignore = "writes docs/img and needs a graphics adapter"]
fn chat_tab() {
    use crate::state::{ChatMessage, ChatState, MessageTiming, ModelState};

    let mut chat = ChatState::default();
    chat.messages.push_back(ChatMessage {
        role: "user".into(),
        content: "Explain what a KV cache holds, briefly.".into(),
        timestamp: "14:02".into(),
        ..Default::default()
    });
    chat.messages.push_back(ChatMessage {
        role: "assistant".into(),
        content: "It holds the keys and values already computed for every token in the \
                  context, so each new token attends over them instead of recomputing the \
                  whole prefix. Its size grows with the context length, not with the prompt \
                  you just sent."
            .into(),
        timestamp: "14:02".into(),
        timing: Some(MessageTiming {
            tokens_per_sec: 148.6,
            duration_ms: 1240,
            token_count: 184,
        }),
        ..Default::default()
    });
    chat.messages.push_back(ChatMessage {
        role: "user".into(),
        content: "Does a second question reuse it?".into(),
        timestamp: "14:03".into(),
        ..Default::default()
    });
    chat.is_generating = true;
    chat.streaming_content = "A second question would reuse it as long as the prefix".into();

    let mut models = ModelState {
        selected_model: Some("qwen3:8b".into()),
        ..Default::default()
    };

    let mut cache = egui_commonmark::CommonMarkCache::default();
    let mut textures: HashMap<String, egui::TextureHandle> = HashMap::new();
    shoot("atelier-chat", (1000.0, 640.0), move |ui| {
        let _ = crate::chat_tab::render(
            ui,
            &mut chat,
            &mut models,
            None,
            &[],
            &mut cache,
            &mut textures,
        );
    });
}

/// Media Studio on the image panel, with a prompt and the controls a generation actually uses.
#[test]
#[ignore = "writes docs/img and needs a graphics adapter"]
fn media_tab() {
    use crate::state::{MediaKind, MediaState};

    let mut media = MediaState {
        kind: MediaKind::Image,
        prompt: "a lighthouse on a basalt shore, low sun, long exposure".into(),
        ..Default::default()
    };

    let mut textures: HashMap<String, egui::TextureHandle> = HashMap::new();
    shoot("atelier-media", (1000.0, 640.0), move |ui| {
        let mut player = crate::audio_playback::AudioPlayer::default();
        let mut video = crate::video_engine::VideoPlayback::default();
        let _ = crate::media_tab::render(
            ui,
            &mut media,
            &mut textures,
            &[],
            &[],
            &mut player,
            &mut video,
        );
    });
}

/// Which of the literal glyphs the source still uses actually resolve in the bundled fonts.
/// A missing glyph draws as a box, which is what the SVG icons exist to prevent, so this
/// renders each one large enough to tell apart and reports the ones that came out identical
/// to the replacement character.
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
        let ink: u32 = image.pixels().filter(|p| p.0[3] > 8).count() as u32;
        println!("{name}: {ink} inked pixels");
    }
}
