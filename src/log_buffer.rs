//! Log Buffer for Real-time Server Logs
//!
//! Provides a shared log buffer that captures tracing logs
//! and makes them available to the GUI.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tracing_subscriber::Layer;

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl LogLevel {
    /// Static label for this level (5 chars or fewer). Used wherever
    /// `format!("{}", level)` would have allocated.
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    /// Static 5-char right-padded label. Replaces
    /// `format!("{:5}", level)` in the log-tab badge render so a
    /// thousand-entry buffer doesn't allocate a fresh padded String
    /// per visible row per repaint.
    pub fn as_str_padded(self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO ",
            LogLevel::Warn => "WARN ",
            LogLevel::Error => "ERROR",
        }
    }
}

impl From<tracing::Level> for LogLevel {
    fn from(level: tracing::Level) -> Self {
        match level {
            tracing::Level::TRACE => LogLevel::Trace,
            tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::ERROR => LogLevel::Error,
        }
    }
}

/// A single log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Timestamp of the log entry
    pub timestamp: String,
    /// Log level
    pub level: LogLevel,
    /// Target/module path
    pub target: String,
    /// Log message
    pub message: String,
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(level: LogLevel, target: String, message: String) -> Self {
        let timestamp = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
        Self {
            timestamp,
            level,
            target,
            message,
        }
    }
}

/// Shared log buffer
#[derive(Debug, Clone)]
pub struct LogBuffer {
    /// Inner buffer protected by mutex
    inner: Arc<Mutex<LogBufferInner>>,
}

#[derive(Debug)]
struct LogBufferInner {
    /// Log entries
    entries: VecDeque<LogEntry>,
    /// Maximum number of entries to keep
    max_entries: usize,
}

/// Allocation-free ASCII case-insensitive substring check. Originally
/// written for the log buffer's search filter (avoids the per-entry
/// `to_lowercase()` alloc that dominated the search hot path); also
/// reused by the Models tab's name filter for the same reason — one
/// retain pass over the full catalog at 60 Hz allocated `2 × N` strings
/// per frame. Non-ASCII bytes in either side are compared as-is (so
/// "café" never matches "CAFÉ" — model + log names are ASCII in practice).
pub(crate) fn contains_ascii_ci(haystack: &str, needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    let hay = haystack.as_bytes();
    if hay.len() < needle.len() {
        return false;
    }
    hay.windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle))
}

impl LogBuffer {
    /// Create a new log buffer with the specified maximum entries.
    /// Level filtering is delegated to tracing's EnvFilter — the buffer
    /// captures everything its layer emits.
    pub fn new(max_entries: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LogBufferInner {
                entries: VecDeque::with_capacity(max_entries),
                max_entries,
            })),
        }
    }

    /// Add a log entry
    pub fn push(&self, entry: LogEntry) {
        if let Ok(mut inner) = self.inner.lock() {
            // Remove oldest if at capacity
            if inner.entries.len() >= inner.max_entries {
                inner.entries.pop_front();
            }
            inner.entries.push_back(entry);
        }
    }

    /// Get filtered log entries (filtered inside lock, then cloned).
    ///
    /// The substring match is ASCII-case-insensitive and allocates NOTHING per
    /// entry. Lower-casing `message` and `target` into fresh strings costs, on a
    /// thousand-entry buffer repainted sixty times a second, on the order of a
    /// hundred thousand short-lived allocations per second for as long as the
    /// search field holds text. `contains_ascii_ci` folds case in place over byte
    /// windows instead.
    pub fn entries_filtered(&self, min_level: LogLevel, search: &str) -> Vec<LogEntry> {
        if let Ok(inner) = self.inner.lock() {
            let needle = search.as_bytes();
            inner
                .entries
                .iter()
                .filter(|e| e.level >= min_level)
                .filter(|e| {
                    if needle.is_empty() {
                        true
                    } else {
                        contains_ascii_ci(&e.message, needle)
                            || contains_ascii_ci(&e.target, needle)
                    }
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get the current count of entries
    pub fn count(&self) -> usize {
        if let Ok(inner) = self.inner.lock() {
            inner.entries.len()
        } else {
            0
        }
    }

    /// Clear all entries. Called from the server-log tab's Clear button.
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.entries.clear();
        }
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// A tracing layer that writes to a log buffer
pub struct LogBufferLayer {
    buffer: LogBuffer,
}

impl LogBufferLayer {
    /// Create a new layer with the given buffer
    pub fn new(buffer: LogBuffer) -> Self {
        Self { buffer }
    }

    /// Get a clone of the buffer
    #[allow(dead_code)]
    pub fn buffer(&self) -> LogBuffer {
        self.buffer.clone()
    }
}

impl<S> Layer<S> for LogBufferLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let level = LogLevel::from(*event.metadata().level());
        let target = event.metadata().target().to_string();

        // Collect all field values
        let mut message = String::new();
        let mut visitor = LogFieldVisitor(&mut message);
        event.record(&mut visitor);

        let entry = LogEntry::new(level, target, message);
        self.buffer.push(entry);
    }
}

/// Visitor to collect field values from a tracing event
struct LogFieldVisitor<'a>(&'a mut String);

impl LogFieldVisitor<'_> {
    /// Write a leading space separator if the accumulator already
    /// has content. Centralised so the seven record_* methods don't
    /// each carry a copy of the `if !self.0.is_empty() ...` block.
    fn write_separator(&mut self) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
    }
}

impl tracing::field::Visit for LogFieldVisitor<'_> {
    // All record_* methods write via `std::fmt::Write::write_fmt`
    // into the borrowed accumulator instead of `push_str(&format!(...))`.
    // The previous form allocated a temporary String per field per
    // log event just to push its bytes; this version writes through
    // directly. Marginal per-event but tracing visitors fire for
    // every log line, and the GUI streams hundreds of log events
    // during a chat / image-gen run.
    //
    // .unwrap() on the write is safe: writing to a String never
    // produces an io::Error (the fmt::Result is from the trait, not
    // a real I/O failure source).
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        use std::fmt::Write;
        self.write_separator();
        write!(self.0, "{}={value}", field.name()).unwrap();
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        use std::fmt::Write;
        self.write_separator();
        write!(self.0, "{}={value}", field.name()).unwrap();
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        use std::fmt::Write;
        self.write_separator();
        write!(self.0, "{}={value}", field.name()).unwrap();
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        use std::fmt::Write;
        self.write_separator();
        write!(self.0, "{}={value}", field.name()).unwrap();
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        use std::fmt::Write;
        self.write_separator();
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            write!(self.0, "{}={value}", field.name()).unwrap();
        }
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        use std::fmt::Write;
        self.write_separator();
        write!(self.0, "{}={value}", field.name()).unwrap();
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write;
        self.write_separator();
        if field.name() == "message" {
            write!(self.0, "{value:?}").unwrap();
        } else {
            write!(self.0, "{}={value:?}", field.name()).unwrap();
        }
    }
}

/// Initialize logging for the GUI application
pub fn init_gui_logging(buffer: LogBuffer) {
    use tracing_subscriber::prelude::*;

    let layer = LogBufferLayer::new(buffer);

    // Create a subscriber with our layer
    // Use EnvFilter to respect RUST_LOG env var, defaulting to info level for our modules
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("atelier=info,reqwest=info"));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(layer)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr));

    // Try to set as global default - if it fails, a subscriber is already set
    // This can happen if the library code initialized tracing before us
    match tracing::subscriber::set_global_default(subscriber) {
        Ok(()) => {
            // Successfully set our subscriber
        }
        Err(_) => {
            // A global subscriber already exists - try to add our layer to it
            // Unfortunately, tracing doesn't support adding layers to an existing subscriber
            // So we'll just continue without the log buffer layer for now
            // Logs will still go to stderr via the existing subscriber
            eprintln!("Note: Global tracing subscriber already set, GUI log buffer not available");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(level: LogLevel, target: &str, message: &str) -> LogEntry {
        LogEntry {
            timestamp: "00:00:00.000".to_string(),
            level,
            target: target.to_string(),
            message: message.to_string(),
        }
    }

    #[test]
    fn push_evicts_oldest_at_capacity() {
        let buf = LogBuffer::new(3);
        buf.push(entry(LogLevel::Info, "a", "first"));
        buf.push(entry(LogLevel::Info, "b", "second"));
        buf.push(entry(LogLevel::Info, "c", "third"));
        buf.push(entry(LogLevel::Info, "d", "fourth"));
        assert_eq!(buf.count(), 3);
        let entries = buf.entries_filtered(LogLevel::Trace, "");
        // Oldest ("first") was popped; "second" should now lead.
        assert_eq!(entries[0].message, "second");
        assert_eq!(entries[2].message, "fourth");
    }

    #[test]
    fn filter_min_level_keeps_equal_or_higher() {
        let buf = LogBuffer::new(10);
        buf.push(entry(LogLevel::Trace, "t", "trace msg"));
        buf.push(entry(LogLevel::Info, "i", "info msg"));
        buf.push(entry(LogLevel::Error, "e", "error msg"));

        let warns = buf.entries_filtered(LogLevel::Warn, "");
        assert_eq!(warns.len(), 1);
        assert_eq!(warns[0].level, LogLevel::Error);

        let infos = buf.entries_filtered(LogLevel::Info, "");
        assert_eq!(infos.len(), 2);
    }

    #[test]
    fn filter_search_matches_message_or_target_case_insensitively() {
        let buf = LogBuffer::new(10);
        buf.push(entry(LogLevel::Info, "engine::cuda", "kv cache resized"));
        buf.push(entry(LogLevel::Info, "engine::cpu", "model loaded"));

        // Match on message
        assert_eq!(buf.entries_filtered(LogLevel::Trace, "CACHE").len(), 1);
        // Match on target
        assert_eq!(buf.entries_filtered(LogLevel::Trace, "cpu").len(), 1);
        // No match
        assert_eq!(buf.entries_filtered(LogLevel::Trace, "wibble").len(), 0);
        // Empty search returns all
        assert_eq!(buf.entries_filtered(LogLevel::Trace, "").len(), 2);
    }

    #[test]
    fn clear_resets_count_to_zero() {
        let buf = LogBuffer::new(5);
        buf.push(entry(LogLevel::Info, "a", "x"));
        buf.push(entry(LogLevel::Info, "b", "y"));
        assert_eq!(buf.count(), 2);
        buf.clear();
        assert_eq!(buf.count(), 0);
        assert!(buf.entries_filtered(LogLevel::Trace, "").is_empty());
    }

    #[test]
    fn log_level_ordering_supports_min_level_compare() {
        assert!(LogLevel::Error > LogLevel::Warn);
        assert!(LogLevel::Warn > LogLevel::Info);
        assert!(LogLevel::Info > LogLevel::Debug);
        assert!(LogLevel::Debug > LogLevel::Trace);
    }

    #[test]
    fn log_level_display_strings_match_tracing_convention() {
        // Pin the Display strings so the Server-Log tab's level chip
        // labels stay consistent. Mirrors the standard tracing
        // crate's uppercase convention (TRACE/DEBUG/INFO/WARN/ERROR).
        assert_eq!(LogLevel::Trace.to_string(), "TRACE");
        assert_eq!(LogLevel::Debug.to_string(), "DEBUG");
        assert_eq!(LogLevel::Info.to_string(), "INFO");
        assert_eq!(LogLevel::Warn.to_string(), "WARN");
        assert_eq!(LogLevel::Error.to_string(), "ERROR");
    }

    #[test]
    fn log_level_from_tracing_level_covers_all_variants() {
        // Pin the From<tracing::Level> mapping — the LogBufferLayer
        // uses this when ingesting events. A miss here would route
        // tracing INFO entries into a different LogLevel bucket
        // and break the server-log tab's filter.
        assert_eq!(LogLevel::from(tracing::Level::TRACE), LogLevel::Trace);
        assert_eq!(LogLevel::from(tracing::Level::DEBUG), LogLevel::Debug);
        assert_eq!(LogLevel::from(tracing::Level::INFO), LogLevel::Info);
        assert_eq!(LogLevel::from(tracing::Level::WARN), LogLevel::Warn);
        assert_eq!(LogLevel::from(tracing::Level::ERROR), LogLevel::Error);
    }

    #[test]
    fn log_entry_new_emits_hh_mm_ss_milli_timestamp() {
        // LogEntry::new builds the timestamp via chrono with format
        // \"%H:%M:%S%.3f\" — that's HH:MM:SS.mmm = 12 chars.
        // Server-Log tab right-aligns to this width; a future widen
        // would shift the rendered table columns. Pin the format.
        let e = LogEntry::new(LogLevel::Info, "test".into(), "msg".into());
        assert_eq!(
            e.timestamp.len(),
            12,
            "expected HH:MM:SS.mmm (12 chars), got {:?}",
            e.timestamp
        );
        assert_eq!(&e.timestamp[2..3], ":");
        assert_eq!(&e.timestamp[5..6], ":");
        assert_eq!(&e.timestamp[8..9], ".");
        assert_eq!(e.level, LogLevel::Info);
        assert_eq!(e.target, "test");
        assert_eq!(e.message, "msg");
    }

    #[test]
    fn contains_ascii_ci_handles_case_and_position() {
        assert!(contains_ascii_ci("hello world", b"WORLD"));
        assert!(contains_ascii_ci("HELLO world", b"hello"));
        assert!(contains_ascii_ci("MixedCASE", b"DCAS"));
        // Empty needle always matches.
        assert!(contains_ascii_ci("anything", b""));
        // Empty needle still matches empty haystack.
        assert!(contains_ascii_ci("", b""));
        // Needle longer than haystack — never matches.
        assert!(!contains_ascii_ci("ab", b"abcdef"));
        // No match.
        assert!(!contains_ascii_ci("hello", b"xyz"));
    }
}
