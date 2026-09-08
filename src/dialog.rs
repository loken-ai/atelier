//! Worker-thread file-dialog scaffolding shared by the chat tab and
//! the Media Studio (audit #5 + #8).
//!
//! rfd's synchronous dialogs block until the user picks/cancels; run
//! on the egui main thread they deadlock against the XDG portal on
//! Linux. Every dialog therefore runs on a fresh worker thread via
//! `spawn_dialog_worker`, and results travel back through the shared
//! `pending_dialog` slot (`ChatDialogResult`) that each tab drains at
//! the top of its render pass.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::icons::Icon;
use crate::state::{ChatDialogResult, MediaAudioSlot, MediaState};
use crate::theme;

/// Spawn the `body` closure on a fresh OS thread so an rfd file
/// dialog (which blocks until the user picks / cancels) doesn't
/// deadlock the egui main thread against the XDG portal on Linux.
///
/// Wraps the four pieces of boilerplate every site needs:
///   - Set `flag` to `true` BEFORE spawning so the UI can disable
///     the originating button until the dialog returns.
///   - Request a repaint immediately so the disabled state is
///     visible without waiting for the next input event.
///   - Bind a ClearOnDrop guard inside the worker so the flag
///     flips back to false + a repaint fires no matter how the
///     closure exits (dialog cancel, body panic, early return).
///   - Pass `ctx` to the closure so it can also request_repaint
///     after pushing a result into the shared slot.
///
/// Factored out of four near-identical call sites (Attach picker,
/// SaveBytes, SaveBytesMany, TTS Save) that each had their own
/// copy of the ClearOnDrop struct + flag-toggle code.
pub(crate) fn spawn_dialog_worker<F>(
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ctx: egui::Context,
    body: F,
)
where
    F: FnOnce(egui::Context) + Send + 'static,
{
    flag.store(true, std::sync::atomic::Ordering::Relaxed);
    ctx.request_repaint();
    let ctx_for_body = ctx.clone();
    std::thread::spawn(move || {
        struct ClearOnDrop {
            flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
            ctx: egui::Context,
        }
        impl Drop for ClearOnDrop {
            fn drop(&mut self) {
                self.flag.store(false, std::sync::atomic::Ordering::Relaxed);
                self.ctx.request_repaint();
            }
        }
        let _guard = ClearOnDrop { flag, ctx };
        body(ctx_for_body);
    });
}

/// A Save button that is disabled while a file dialog is already in
/// flight (a second overlapping dialog would clobber the first result
/// in the shared `pending_dialog` slot).
///
/// Promoted from media_tab (audit #8) — takes the in-flight flag
/// directly so both the chat tab (ChatState) and the Media Studio
/// (MediaState) can gate their Save buttons through one widget.
pub(crate) fn save_button(
    ui: &mut egui::Ui,
    dialog_in_flight: &Arc<AtomicBool>,
    label: &str,
) -> egui::Response {
    let busy = dialog_in_flight.load(std::sync::atomic::Ordering::Relaxed);
    ui.add_enabled(
        !busy,
        egui::Button::image_and_text(
            Icon::Save.image(11.0, theme::ink()),
            crate::theme::text::note(label),
        ),
    )
}

/// Spawn the rfd save dialog on a worker thread and route the pick
/// back through the shared `ChatDialogResult::SaveBytes` slot, which
/// the owning tab's drain writes to disk on the next frame.
///
/// Promoted from media_tab (audit #8) — takes the `pending_dialog` /
/// `dialog_in_flight` pair directly (instead of `&MediaState`) so the
/// chat tab's single-file save sites share the same worker.
pub(crate) fn spawn_save(
    ctx: egui::Context,
    pending: Arc<Mutex<Option<ChatDialogResult>>>,
    dialog_in_flight: Arc<AtomicBool>,
    default_name: String,
    ext: &'static [&'static str],
    filter_label: &'static str,
    bytes: Vec<u8>,
) {
    spawn_dialog_worker(dialog_in_flight, ctx, move |_| {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter(filter_label, ext)
            .save_file()
        else {
            return; // cancelled
        };
        if let Ok(mut g) = pending.lock() {
            *g = Some(ChatDialogResult::SaveBytes { path, bytes });
        }
    });
}

/// Drain the Media Studio's file-dialog slot: perform a pending save
/// (recording the outcome in `media.status` / `media.error`) or route
/// a worker-thread audio pick into its MediaState slot.
///
/// Media-specific by design (the chat tab has its own drain in
/// `chat_tab::render` that surfaces outcomes as chat system messages
/// instead of a status line) — it lives here with its `save_button` /
/// `spawn_save` siblings so the whole dialog round-trip reads in one
/// module.
pub(crate) fn drain_pending_dialog(media: &mut MediaState) {
    let result = media.pending_dialog.lock().ok().and_then(|mut g| g.take());
    match result {
        Some(ChatDialogResult::SaveBytes { path, bytes }) => {
            let byte_count = bytes.len();
            match std::fs::write(&path, bytes) {
                Ok(()) => {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| path.display().to_string());
                    media.status = format!(
                        "Saved {name} ({}).",
                        crate::api::types::format_size(byte_count as u64)
                    );
                    media.error = None;
                }
                Err(e) => {
                    media.error = Some(format!("Failed to save {}: {e}", path.display()));
                }
            }
        }
        Some(ChatDialogResult::MediaAudio { slot, name, bytes }) => match bytes {
            Ok(bytes) => {
                let dest = match slot {
                    MediaAudioSlot::EditImage => &mut media.image_edit.source,
                    MediaAudioSlot::TranscribeAudio => &mut media.transcribe.audio,
                    MediaAudioSlot::SfxInit => &mut media.sfx.init_audio,
                    MediaAudioSlot::ImageControl => &mut media.image.control,
                    MediaAudioSlot::SeparateAudio => &mut media.separate.audio,
                    MediaAudioSlot::VideoStartImage => &mut media.video.start_image,
                };
                *dest = Some((name, bytes));
            }
            // A failed read surfaces in the error banner above the results, which is
            // where the user is already looking.
            Err(e) => media.error = Some(e),
        },
        // AttachFiles / SaveBytesMany are chat-tab-only; the media
        // studio never spawns workers that produce them.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_dialog_worker_sets_flag_true_synchronously_and_clears_on_exit() {
        // The flag must flip true BEFORE spawn returns so the next
        // render-frame's `flag.load()` sees it true and disables the
        // originating button. (If we set it inside the worker, there's
        // a window where rapid re-clicks could spawn duplicate
        // workers.) Then the ClearOnDrop guard must flip it back to
        // false when the worker exits — even via early return.
        //
        // We can't actually drive an rfd dialog in tests, so the
        // body closure is a no-op completion signal. Block on a
        // brief sleep loop until the flag clears, then assert.
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let flag = Arc::new(AtomicBool::new(false));
        let ctx = egui::Context::default();
        let done = Arc::new(AtomicBool::new(false));

        // Pre-condition: flag idle.
        assert!(!flag.load(Ordering::Relaxed));

        let done_clone = done.clone();
        spawn_dialog_worker(flag.clone(), ctx, move |_| {
            // Inside the worker, flag should still be true (the
            // ClearOnDrop guard's Drop hasn't fired yet).
            done_clone.store(true, Ordering::Relaxed);
        });

        // Immediately after spawn returns: flag is true (set
        // synchronously). The worker may or may not have run yet.
        assert!(flag.load(Ordering::Relaxed),
            "flag must be true immediately after spawn — protects against \
             rapid re-clicks spawning duplicate workers");

        // Wait up to 2 s for the worker to finish + the ClearOnDrop
        // guard to flip the flag back to false. Polling instead of
        // blocking JoinHandle because spawn_dialog_worker
        // intentionally doesn't return one (fire-and-forget).
        let start = std::time::Instant::now();
        while flag.load(Ordering::Relaxed) || !done.load(Ordering::Relaxed) {
            if start.elapsed() > std::time::Duration::from_secs(2) {
                panic!(
                    "worker didn't complete + clear flag within 2 s — \
                     ClearOnDrop guard may have leaked"
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(!flag.load(Ordering::Relaxed),
            "flag must be false after worker exits — re-clicks can spawn again");
        assert!(done.load(Ordering::Relaxed),
            "body closure must have run");
    }
}
