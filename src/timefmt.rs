//! Centralised time-format helpers — single source of truth for the
//! two timestamp shapes used across the GUI.
//!
//! Before this module each call site repeated
//! `chrono::Local::now().format("%H:%M").to_string()` inline (6 sites
//! in app.rs for chat messages, 13 sites for CLI command output). One
//! drive-by typo (e.g. `%H:%m`) would have produced a confusing single-
//! site formatting drift; pinning the strings here means any future
//! format change touches one place.
//!
//! The two shapes:
//!
//! - `chat_now()` → `"HH:MM"` — minute-resolution, matches the visual
//!   density of message bubbles (no need for seconds in casual chat).
//! - `cli_now()` → `"HH:MM:SS"` — second-resolution, useful in the
//!   CLI tab where users may run commands in quick succession and
//!   want to distinguish their ordering.

/// Current local time formatted as `HH:MM` for chat message bubbles.
pub fn chat_now() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// Current local time formatted as `HH:MM:SS` for CLI output entries.
pub fn cli_now() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_now_is_hh_mm() {
        let s = chat_now();
        // Exactly "HH:MM" — 5 chars, single colon at index 2,
        // digits on either side. Pins the shape so a future
        // tweak to "%H:%M:%S" (silently widening the chat-bubble
        // timestamp) breaks the test.
        assert_eq!(s.len(), 5, "chat_now() = {s:?}");
        assert_eq!(&s[2..3], ":", "missing single colon at index 2 in {s:?}");
        for ch in s.chars().filter(|c| *c != ':') {
            assert!(ch.is_ascii_digit(), "non-digit {ch:?} in {s:?}");
        }
    }

    #[test]
    fn cli_now_is_hh_mm_ss() {
        let s = cli_now();
        // Exactly "HH:MM:SS" — 8 chars, colons at indexes 2 and 5.
        assert_eq!(s.len(), 8, "cli_now() = {s:?}");
        assert_eq!(&s[2..3], ":", "missing colon at index 2 in {s:?}");
        assert_eq!(&s[5..6], ":", "missing colon at index 5 in {s:?}");
        for ch in s.chars().filter(|c| *c != ':') {
            assert!(ch.is_ascii_digit(), "non-digit {ch:?} in {s:?}");
        }
    }

    #[test]
    fn chat_now_and_cli_now_share_hh_mm_prefix() {
        // The first 5 chars must agree (within a clock tick) — both
        // come from the same chrono::Local::now() format spec. Drift
        // here would indicate one drifted to UTC accidentally.
        // We allow a small window for the two calls to land in
        // different minutes.
        let chat = chat_now();
        let cli = cli_now();
        let chat_min = &chat[3..5];
        let cli_min = &cli[3..5];
        let chat_hr = &chat[..2];
        let cli_hr = &cli[..2];
        // Same hour (in the same minute we expect same min too,
        // but allow ±1 to cover a tick boundary).
        let m_diff = (chat_min.parse::<i32>().unwrap() - cli_min.parse::<i32>().unwrap()).abs();
        let h_diff = (chat_hr.parse::<i32>().unwrap() - cli_hr.parse::<i32>().unwrap()).abs();
        assert!(h_diff <= 1, "hour drift chat={chat} cli={cli}");
        assert!(
            m_diff <= 1 || m_diff == 59,
            "minute drift > 1 chat={chat} cli={cli}"
        );
    }
}
