//! System-player audio playback for generated WAV blobs.
//!
//! Extracted from `chat_tab.rs` (audit #5). Shared by the chat tab's
//! TTS Play button and the Media Studio's audio results.

/// Decode a base64 WAV blob, write it to a temp file, and shell out
/// to the first available system audio player. Backgrounds the player
/// process so the GUI thread doesn't block (rfd's process spawn returns
/// immediately; the player runs to completion on its own).
///
/// Tried in order: paplay (PulseAudio — most likely on modern Linux
/// desktops), aplay (ALSA), afplay (macOS), xdg-open (last resort —
/// hands the file to the user's default audio app, which usually opens
/// a GUI player). On Windows the xdg-open branch becomes `start`.
/// Returns an error if base64 decode fails, the temp write fails, or
/// no player command is found on PATH.
pub(crate) fn play_audio_blob(wav_b64: &str) -> Result<(), String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(wav_b64)
        .map_err(|e| format!("base64 decode: {e}"))?;
    // Fire-and-forget: one process-wide transport, so a second press replaces the
    // first clip instead of layering two players over each other - which is what the
    // spawn-a-player implementation did.
    static ONESHOT: std::sync::OnceLock<std::sync::Mutex<crate::audio_engine::Transport>> =
        std::sync::OnceLock::new();
    let t = ONESHOT.get_or_init(|| std::sync::Mutex::new(crate::audio_engine::Transport::new()));
    let mut guard = t.lock().unwrap_or_else(|e| e.into_inner());
    guard.stop();
    guard.start(&bytes)
}

/// Delete `llmgui_tts_*.wav` files in the platform temp dir whose
/// mtime is older than `max_age`. Called from `play_audio_blob`
/// before each new write so a long-running session can't accumulate
/// stale per-click WAVs in /tmp. Best-effort — any individual file
/// failure (permission, busy player) is ignored.
///
/// Extracted as a pub(crate) free function so unit tests can drive
/// it against a controlled directory layout without touching the
/// real platform temp dir.
pub(crate) fn sweep_old_tts_temp_files(max_age: std::time::Duration) {
    sweep_old_tts_temp_files_in(&std::env::temp_dir(), max_age);
}

/// Test surface for the sweep. Same logic against an explicit dir.
pub(crate) fn sweep_old_tts_temp_files_in(dir: &std::path::Path, max_age: std::time::Duration) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in read_dir.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        // Match the exact prefix + extension the writer uses so we
        // don't accidentally chew on unrelated WAVs the user has in
        // /tmp (e.g. from other tools).
        if !name_str.starts_with("llmgui_tts_") || !name_str.ends_with(".wav") {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(mtime) = meta.modified() else {
            continue;
        };
        let Ok(age) = now.duration_since(mtime) else {
            continue;
        };
        if age >= max_age {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_audio_blob_rejects_invalid_base64() {
        // Garbage that won't even base64-decode → clear error, no panic.
        let err = play_audio_blob("!!!notb64!!!").unwrap_err();
        assert!(err.contains("base64 decode"), "got: {err}");
    }

    #[test]
    fn play_audio_blob_decodes_before_it_reaches_the_device() {
        // Tiny valid WAV header + 1 sample. We can't assert that the
        // player actually plays (no audio device on most CI hosts) and
        // we can't reliably assert that NO player exists either, so the
        // outcome split is:
        //   - Player available → Ok(()) returned
        //   - No player → Err("no audio player found ...")
        // Either way, the function must NOT panic and must NOT 1) leak
        // an undecoded blob to the player or 2) silently swallow errors.
        let wav = b"RIFF\x24\x00\x00\x00WAVEfmt \x10\x00\x00\x00\x01\x00\
                    \x01\x00\x44\xac\x00\x00\x88\x58\x01\x00\x02\x00\x10\x00\
                    data\x00\x00\x00\x00";
        let b64 = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(wav)
        };
        // Two legitimate outcomes: it played, or the host has no output device.
        // A panic is not one of them, and neither is an error about some external
        // program - playback is in-process.
        match play_audio_blob(&b64) {
            Ok(()) => {}
            Err(e) => assert!(
                e.contains("no audio output device") || e.contains("native-audio"),
                "unexpected error shape: {e}",
            ),
        }
    }

    #[test]
    fn sweep_old_tts_temp_files_with_zero_age_deletes_only_matching_files() {
        // max_age = ZERO: every existing file is "older than the cutoff"
        // by definition (any file at all has mtime <= now). So this
        // test pins the prefix + extension filters specifically — only
        // llmgui_tts_*.wav siblings should be touched. Unrelated
        // tools' temp files (other prefix, other extension, no
        // extension at all) must survive even with the most
        // aggressive sweep.
        let dir = std::env::temp_dir().join(format!("llmgui_sweep_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("seed tempdir");

        let our_wav = dir.join("llmgui_tts_abc.wav"); // delete
        let other_pfx = dir.join("not_ours_abc.wav"); // keep (prefix)
        let other_ext = dir.join("llmgui_tts_abc.mp3"); // keep (extension)
        let other_both = dir.join("random_file"); // keep (neither)
        for p in [&our_wav, &other_pfx, &other_ext, &other_both] {
            std::fs::write(p, b"x").expect("seed");
        }

        sweep_old_tts_temp_files_in(&dir, std::time::Duration::ZERO);

        assert!(
            !our_wav.exists(),
            "llmgui_tts_*.wav must be deleted with zero-age sweep"
        );
        assert!(
            other_pfx.exists(),
            "non-llmgui_tts_ files must survive — prefix gate protects \
             unrelated WAVs other tools left in /tmp"
        );
        assert!(
            other_ext.exists(),
            "non-.wav files must survive even with the matching prefix"
        );
        assert!(other_both.exists(), "files matching neither must survive");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sweep_old_tts_temp_files_with_long_max_age_keeps_recent_files() {
        // Mirror of the zero-age test: an extremely long max_age (24 h)
        // shouldn't touch a freshly-written llmgui_tts_*.wav. Together
        // these two tests bracket the behaviour:
        //   - zero   → delete every matching file
        //   - 24 h   → keep every matching file (none can be that old)
        // The production call uses 1 h which sits between the two.
        let dir =
            std::env::temp_dir().join(format!("llmgui_sweep_keep_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("seed tempdir");
        let fresh = dir.join("llmgui_tts_recent.wav");
        std::fs::write(&fresh, b"x").expect("seed");

        sweep_old_tts_temp_files_in(&dir, std::time::Duration::from_secs(24 * 3600));

        assert!(
            fresh.exists(),
            "freshly-written llmgui_tts_*.wav must survive a long max_age sweep"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sweep_old_tts_temp_files_handles_missing_dir() {
        // The sweep is best-effort — pointing it at a nonexistent
        // directory must NOT panic. (Defensive: covers the case
        // where TMPDIR is unset or pointing somewhere weird.)
        let bogus =
            std::env::temp_dir().join(format!("llmgui_sweep_nonexistent_{}", uuid::Uuid::new_v4()));
        assert!(!bogus.exists());
        sweep_old_tts_temp_files_in(&bogus, std::time::Duration::from_secs(60));
        // Just not panicking is the assertion.
    }
}

// ============================================================================
// In-GUI player with transport controls (play / pause / stop / seek)
// ============================================================================
//
// Built on the same system players as `play_audio_blob` but keeping the CHILD
// PROCESS under our control instead of fire-and-forget:
//   - pause / resume  = SIGSTOP / SIGCONT on the player process
//   - stop            = kill the process
//   - seek            = rewrite the temp WAV starting at the target offset and
//                       relaunch the player there (WAV trim is a header fixup)
//   - position        = wall clock since (re)start + the seek base, frozen
//                       while paused; duration parsed from the WAV header.

/// The transport, now backed by the in-process output device.
///
/// The previous implementation spawned a system player per clip, so pause meant
/// killing it, seek meant respawning from a temporary file at an offset, and position
/// was wall-clock arithmetic that drifted from what was audible. All three now read
/// and write the same sample cursor, which is why they agree.
pub struct AudioPlayer {
    transport: crate::audio_engine::Transport,
    /// Which result row owns the transport.
    id: Option<String>,
    paused: bool,
}

impl AudioPlayer {
    /// Start playing a blob (replacing any current playback). `id` marks which
    /// result row owns the transport (see [`AudioPlayer::playing_id`]).
    pub fn play(&mut self, id: &str, wav_b64: &str) -> Result<(), String> {
        use base64::Engine;
        self.stop();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(wav_b64)
            .map_err(|e| format!("base64 decode: {e}"))?;
        self.transport.start(&bytes)?;
        self.id = Some(id.to_string());
        self.paused = false;
        Ok(())
    }

    /// Pause and resume in place. The cursor stops advancing; nothing is torn down,
    /// so resuming continues from the same sample rather than the same second.
    pub fn toggle_pause(&mut self) {
        if self.id.is_none() {
            return;
        }
        self.paused = !self.paused;
        self.transport.set_paused(self.paused);
    }

    pub fn stop(&mut self) {
        self.transport.stop();
        self.id = None;
        self.paused = false;
    }

    pub fn seek(&mut self, pos: f32) {
        self.transport.seek(pos.clamp(0.0, self.duration()));
    }

    /// Which result row is playing (None when idle / finished).
    pub fn playing_id(&mut self) -> Option<String> {
        if self.id.is_some() && self.transport.finished() {
            self.stop();
        }
        self.id.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn duration(&self) -> f32 {
        self.transport.duration()
    }

    /// Current position (s), read from the sample cursor the device is consuming.
    pub fn position(&mut self) -> f32 {
        self.transport.position()
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self {
            transport: crate::audio_engine::Transport::new(),
            id: None,
            paused: false,
        }
    }
}
