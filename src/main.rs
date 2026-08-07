// A Windows RELEASE build must not open a console behind the window: the default
// subsystem is `console`, which pops a black terminal alongside the GUI. Debug
// builds keep it, because that is where the tracing output is read from.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! LLM GUI Client
//!
//! A standalone desktop GUI that connects to any Ollama/OpenAI-compatible server.
//! Features:
//! - Chat with LLM models
//! - CLI commands (load, unload, list, pull, etc.)
//! - Hardware monitoring (via server API)

pub mod api;
mod app;
mod chat_tab;
mod config;
mod dialog;
mod icons;
mod state;
mod task;
mod texture;
mod theme;
mod toast;
mod timefmt;
pub mod ui;

use clap::Parser;
use eframe::egui;

use app::{Args, LLMGuiApp};

/// Make a crash SAY something.
///
/// A Windows release build runs in the graphical subsystem, so it has no console: the
/// message a panic prints goes nowhere and the window simply disappears. What a user can
/// report is "it quits with no error", which is the least actionable sentence there is, and
/// it is what every panic in this program looked like however careful the code around it
/// was.
///
/// This writes the panic - what, where, and the backtrace - beside the configuration, and
/// puts a dialog on screen naming that file. It does not prevent the crash; it ends the
/// silence, which is what makes the next one fixable.
fn install_crash_reporter() {
    let path = directories::ProjectDirs::from("com", "loken", "atelier")
        .map(|d| d.config_dir().join("crash.log"));
    std::panic::set_hook(Box::new(move |info| {
        // `Backtrace::force_capture` does not need the environment to ask for one, which a
        // user double-clicking an icon never will.
        let report = format!(
            "atelier crashed

when: {:?}
where: {}
what: {info}

backtrace:
{}
",
            std::time::SystemTime::now(),
            info.location().map(|l| l.to_string()).unwrap_or_else(|| "unknown".into()),
            std::backtrace::Backtrace::force_capture()
        );
        let mut written = None;
        if let Some(p) = path.as_ref() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            // Append: a crash that repeats is a pattern, and overwriting hides it.
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                if f.write_all(report.as_bytes()).is_ok() {
                    written = Some(p.clone());
                }
            }
        }
        eprintln!("{report}");
        let where_ = written
            .map(|p| format!("Details were written to:\n{}", p.display()))
            .unwrap_or_else(|| "The details could not be written to disk.".to_string());
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("atelier stopped unexpectedly")
            .set_description(format!(
                "{}\n\n{where_}",
                info.payload()
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| info.payload().downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "The window closed after an internal error.".into())
            ))
            .show();
    }));
}

fn main() -> eframe::Result<()> {
    // Before anything else: a crash from here on says so instead of vanishing.
    install_crash_reporter();

    // Initialize logging with our custom buffer layer
    let log_buffer = log_buffer::LogBuffer::new(1000);
    log_buffer::init_gui_logging(log_buffer.clone());

    let args = Args::parse();

    // Earlier builds played TTS by writing `llmgui_tts_*.wav` into the platform temp
    // dir; the in-process audio engine replaced that, so nothing writes them and -
    // until now - nothing removed them either. The sweeper was written and tested for
    // exactly this job but never called, leaving every one of those files behind on
    // machines that ran an older build. One read_dir at startup clears them.
    audio_playback::sweep_old_tts_temp_files(std::time::Duration::from_secs(3600));

    // Peek the persisted config so the initial window dimensions
    // match what the user had at last close. Without this peek the
    // hardcoded 1000×750 always wins (commit a26d63d persists the
    // size on every resize, but eframe never reads it back). Load
    // once here; LLMGuiApp::new will re-load inside the closure —
    // a tiny duplicate file read at startup, but it keeps the
    // ViewportBuilder construction self-contained and avoids
    // threading the loaded config through eframe::run_native.
    let persisted = config::AppConfig::load();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([
            persisted.window_width.unwrap_or(1000.0),
            persisted.window_height.unwrap_or(750.0),
        ])
        .with_min_inner_size([600.0, 400.0])
        .with_title("LLM GUI");
    // On most window managers a maximised request wins over an explicit size, so
    // the size below only applies when the user has un-maximised. Absent state
    // means first run, which starts maximised.
    if persisted.window_maximized.unwrap_or(true) {
        viewport = viewport.with_maximized(true);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "LLM GUI",
        options,
        Box::new(|cc| {
            // Install egui_extras image loaders (SVG + raster) so the
            // bundled icons in src/icons/*.svg render via the
            // Icon::show() helper. Without this call, Image widgets
            // built from bytes:// URIs draw nothing.
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(LLMGuiApp::new(cc, args, log_buffer)))
        }),
    )
}
