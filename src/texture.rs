//! Base64-image texture loading + the texture-cache key scheme.
//!
//! Extracted from `chat_tab.rs` (audit #5). Owns the whole texture-
//! cache surface: decoding base64 payloads into egui textures (with
//! one-shot warn + placeholder fail-caching), the content-hashed
//! cache-key format, and the attach-namespace invalidation helper.

use std::collections::HashMap;

use eframe::egui;
use egui::{ColorImage, TextureHandle};

/// Stable, collision-resistant texture-cache key for a base64
/// image attachment. Combines a `kind` namespace ("msg", "gen",
/// "attach") with a hash of the base64 payload + the position
/// `i` inside the source message.
///
/// Replaces the previous `format!("{kind}_{timestamp}_{i}", ...)`
/// scheme, which used ChatMessage.timestamp ("HH:MM" — minute
/// resolution). Two image-gen messages produced in the same
/// minute collided on `(timestamp, i)`, and the cache served
/// whichever loaded first — meaning the user saw the WRONG image
/// for the second message. Hashing the payload makes collisions
/// effectively impossible for distinct images, while two messages
/// with byte-identical images intentionally share a cache entry
/// (same image → one upload, correct render).
pub(crate) fn image_cache_key(kind: &str, b64: &str, i: usize) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b64.hash(&mut h);
    format!("{kind}_{:016x}_{i}", h.finish())
}

/// Prefix used by every input-chip texture key produced via
/// `image_cache_key("attach", …)`. Centralised so the per-frame
/// `retain(!starts_with(…))` calls and the namespace constant can't
/// drift apart.
pub(crate) const ATTACH_KEY_PREFIX: &str = "attach_";

/// Drop every cached texture in the chat-tab input-chip namespace
/// (keys produced by `image_cache_key("attach", …)`). Called from
/// 5 sites that mutate the attached-images vec — attach-files,
/// drag-drop, per-chip Remove, Remove-all, and Send (the staging
/// vec moves into the chat bubble, so the chip-side cache is
/// stale). Lifted out of those sites so a future re-namespace of
/// the texture key can update one place, not five.
pub(crate) fn clear_attach_chip_textures(
    image_textures: &mut HashMap<String, TextureHandle>,
) {
    image_textures.retain(|k, _| !k.starts_with(ATTACH_KEY_PREFIX));
}

/// Load a base64-encoded image as an egui texture.
///
/// On any decode failure (corrupt base64, unsupported format, truncated
/// payload, etc.) emits a one-shot `tracing::warn!` keyed by `name`
/// and returns `Some` containing a 1×1 transparent placeholder texture.
/// The placeholder lets the caller's `contains_key` gate succeed so
/// the next frame doesn't re-decode the same broken bytes — without
/// the fail-cache, a corrupt server response would re-decode 60×/sec
/// and spam the log + waste CPU. The placeholder renders invisibly
/// in the bubble's image slot, which is the same UX outcome as
/// rendering nothing.
pub(crate) fn load_base64_texture(ui: &mut egui::Ui, base64_data: &str, name: &str) -> TextureHandle {
    use base64::Engine;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(base64_data) {
        Ok(b) => b,
        Err(e) => {
            warn_once_per_texture(name, || format!(
                "image texture {name}: base64 decode failed ({e}) — \
                 server returned non-base64 payload or it was truncated"
            ));
            return transparent_placeholder_texture(ui, name);
        }
    };
    // ORIENTATION, like everywhere else. This one path did not apply it, so a phone
    // portrait previewed a quarter-turn over in the chat while the SAME file was upright
    // in the media studio and upright again on the server. A preview exists to answer
    // "is this the right picture", and it was answering wrong.
    //
    // The server has a build gate that walks its own crate for un-annotated decodes; it
    // cannot see this one, which is how the divergence survived.
    let img = match crate::media_tab::decode_oriented(&bytes).ok_or(()) {
        Ok(i) => i,
        Err(e) => {
            let _ = e;
            warn_once_per_texture(name, || format!(
                "image texture {name}: decode failed - {} bytes of \
                 unrecognised or corrupt image data",
                bytes.len(),
            ));
            return transparent_placeholder_texture(ui, name);
        }
    };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();
    let color_image = ColorImage::from_rgba_unmultiplied(size, &pixels);
    ui.ctx().load_texture(name, color_image, egui::TextureOptions::LINEAR)
}

/// Emit a tracing warning at most once per texture-name across the
/// process lifetime. Defends against per-frame spam when a corrupt
/// image stays in the chat scrollback — without dedup, a 60 Hz
/// repaint would log the same failure 60 times per second for the
/// duration of the session.
///
/// Important: the message-building closure is called BEFORE
/// `tracing::warn!`, not inline in its format args. That order is
/// load-bearing because tracing macros short-circuit their argument
/// evaluation when no subscriber is registered (the case in tests).
/// Building eagerly + binding to a local lets the dedup behaviour
/// be unit-tested via the closure's side effects instead of needing
/// to register a tracing subscriber in tests.
fn warn_once_per_texture(name: &str, msg: impl FnOnce() -> String) {
    use std::sync::{Mutex, OnceLock};
    static SEEN: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    if let Ok(mut g) = seen.lock() {
        if g.insert(name.to_string()) {
            let formatted = msg();
            tracing::warn!("{}", formatted);
        }
    }
}

/// 1×1 fully-transparent placeholder texture. Used as a "we tried,
/// it didn't decode, don't retry" sentinel in the texture cache so
/// load_base64_texture failures don't re-fire every frame.
fn transparent_placeholder_texture(ui: &mut egui::Ui, name: &str) -> TextureHandle {
    let color_image = ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 0]);
    ui.ctx().load_texture(
        format!("{name}__broken"),
        color_image,
        egui::TextureOptions::NEAREST,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── image_cache_key ───────────────────────────────────────────
    // What these tests guard: a key of `(timestamp, i)` at minute resolution is
    // not unique - two images generated within the same minute collide, and the
    // cache then serves whichever loaded first for both of them.

    #[test]
    fn image_cache_key_distinguishes_different_payloads() {
        let a = image_cache_key("gen", "iVBORw0KGgoFIRST", 0);
        let b = image_cache_key("gen", "iVBORw0KGgoSECOND", 0);
        assert_ne!(a, b,
            "two distinct image payloads must yield distinct cache keys, \
             even at the same kind+index — otherwise the second image \
             renders the first one's pixels (the bug this guards against)");
    }

    #[test]
    fn image_cache_key_distinguishes_same_payload_at_different_indices() {
        // Same image attached twice in one message (rare but possible
        // via copy-paste workflow). The (i) suffix keeps the slots
        // separate so removing slot 0 doesn't invalidate slot 1's
        // cache entry.
        let img = "samebytes";
        let k0 = image_cache_key("attach", img, 0);
        let k1 = image_cache_key("attach", img, 1);
        assert_ne!(k0, k1);
    }

    #[test]
    fn image_cache_key_distinguishes_kinds_for_same_payload() {
        // The same bytes might appear in BOTH user-uploaded
        // (`attach`) and message-displayed (`msg`) contexts (e.g.
        // a vision model echoing back the input). Namespace by
        // kind so a Clear that drops `attach_*` doesn't kick the
        // `msg_*` cache entry pointing at the same pixels.
        let img = "samebytes";
        let m = image_cache_key("msg", img, 0);
        let a = image_cache_key("attach", img, 0);
        let g = image_cache_key("gen", img, 0);
        assert_ne!(m, a);
        assert_ne!(a, g);
        assert_ne!(m, g);
    }

    #[test]
    fn image_cache_key_is_stable_across_calls() {
        // Same inputs → same output. Needed so contains_key()
        // lookups hit the cached texture instead of re-decoding
        // the image every frame.
        let img = "abcdef";
        assert_eq!(
            image_cache_key("gen", img, 3),
            image_cache_key("gen", img, 3),
        );
    }

    #[test]
    fn warn_once_per_texture_dedupes_repeat_calls_for_the_same_key() {
        // The reason this helper exists: at 60 Hz a corrupt image
        // stuck in the chat scrollback would otherwise log the same
        // failure ~60 times per second. Pin the dedup so a refactor
        // that drops it (e.g. switches to plain tracing::warn!) gets
        // caught by CI before users see the log flood.
        //
        // Strategy: track how many times the closure body runs. The
        // closure builds the message string, so a re-run means the
        // dedup gate let the warn through a second time.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let counter = AtomicUsize::new(0);
        let key = "image_cache_warn_dedup_test_key";

        for _ in 0..10 {
            warn_once_per_texture(key, || {
                counter.fetch_add(1, Ordering::SeqCst);
                "boom".to_string()
            });
        }
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "warn_once_per_texture must invoke the message builder \
             exactly once per key — otherwise the log floods at 60 Hz \
             on a corrupt image stuck in the bubble"
        );
    }

    #[test]
    fn warn_once_per_texture_does_not_dedupe_distinct_keys() {
        // Failures on different textures should each get their own
        // warning. The dedup is per-key, not global, so a user with
        // multiple corrupt images can see which one(s) failed
        // instead of only the first.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let counter = AtomicUsize::new(0);
        for key in ["distinct_keys_test_a", "distinct_keys_test_b", "distinct_keys_test_c"] {
            warn_once_per_texture(key, || {
                counter.fetch_add(1, Ordering::SeqCst);
                "boom".to_string()
            });
        }
        assert_eq!(counter.load(Ordering::SeqCst), 3,
            "three distinct keys should produce three warnings");
    }

    #[test]
    fn image_cache_key_preserves_kind_prefix() {
        // The Clear-attachments path uses `clear_attach_chip_textures`
        // (which `retain(|k, _| !k.starts_with(ATTACH_KEY_PREFIX))`) —
        // that pattern only works if our keys actually start with the
        // kind prefix. Pin the format so a future refactor that moves
        // the prefix later in the string can't silently break the
        // cleanup.
        for (kind, img) in [("attach", "x"), ("msg", "y"), ("gen", "z")] {
            let k = image_cache_key(kind, img, 0);
            assert!(
                k.starts_with(&format!("{kind}_")),
                "cache key {k:?} must start with {kind}_ for the \
                 starts_with-based retain() cleanup to drop it"
            );
        }
    }

    #[test]
    fn clear_attach_chip_textures_drops_attach_keys_only() {
        // Pins the helper's contract: every key produced via
        // image_cache_key("attach", …) is dropped; every msg/gen key
        // survives. The retain predicate keying off ATTACH_KEY_PREFIX
        // depends on the cache-key format pinned above — both tests
        // share the same dependency surface.
        // TextureHandle has no Default and requires an egui Context
        // to construct — so we exercise the retain predicate against
        // a parallel HashSet of just the keys. The helper itself is
        // a one-line `image_textures.retain(…)`; if it drops the
        // right keys from a String-keyed set, it drops them from a
        // String-keyed HashMap<_, TextureHandle> too.
        let keys: Vec<String> = vec![
            image_cache_key("attach", "a-payload", 0),
            image_cache_key("attach", "b-payload", 1),
            image_cache_key("msg",    "c-payload", 0),
            image_cache_key("gen",    "d-payload", 0),
            image_cache_key("gen",    "e-payload", 1),
        ];
        let mut survivors: std::collections::HashSet<String> = keys.iter().cloned().collect();
        survivors.retain(|k| !k.starts_with(ATTACH_KEY_PREFIX));
        assert_eq!(survivors.len(), 3,
            "exactly the 3 non-attach keys should survive the prefix retain");
        for k in &survivors {
            assert!(!k.starts_with(ATTACH_KEY_PREFIX),
                "survivor {k:?} should not be in the attach namespace");
        }
    }
}
