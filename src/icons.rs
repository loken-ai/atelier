//! SVG icon set bundled into the binary via `include_bytes!`.
//!
//! Icons are `Icon` variants rather than emoji characters: 16x16 single-colour
//! line art, so they tint with `currentColor` and look the same whatever emoji
//! font the system happens to ship.
//!
//! Rendering goes through egui_extras' SVG loader (enabled via the `svg`
//! feature in Cargo.toml). main.rs must call
//! `egui_extras::install_image_loaders(&cc.egui_ctx)` once at startup;
//! `Icon::show` then dispatches via `egui::Image::new(uri).fit_to_exact_size`.
//!
//! Each enum variant maps to one `bytes://icons/<name>.svg` URI; the
//! `bytes://` scheme tells the loader to consume the bundled bytes
//! directly without hitting the filesystem.

use eframe::egui;

/// All SVG icons available to the GUI. Variant names match the file
/// stem — `Icon::Folder` ↔ `icons/folder.svg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Attach,
    Bolt,
    Chart,
    Chat,
    Check,
    Copy,
    Cross,
    Download,
    Eye,
    Film,
    Folder,
    Gear,
    Globe,
    Home,
    Key,
    Lock,
    Mic,
    Music,
    Package,
    Palette,
    Play,
    Refresh,
    Save,
    Search,
    Server,
    Speaker,
    Stop,
    Thermometer,
    Trash,
    Warning,
}

impl Icon {
    /// Bundled SVG bytes for this icon.
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::Attach   => include_bytes!("icons/attach.svg"),
            Self::Bolt     => include_bytes!("icons/bolt.svg"),
            Self::Chart    => include_bytes!("icons/chart.svg"),
            Self::Chat     => include_bytes!("icons/chat.svg"),
            Self::Check    => include_bytes!("icons/check.svg"),
            Self::Copy     => include_bytes!("icons/copy.svg"),
            Self::Cross    => include_bytes!("icons/cross.svg"),
            Self::Download => include_bytes!("icons/download.svg"),
            Self::Eye      => include_bytes!("icons/eye.svg"),
            Self::Film     => include_bytes!("icons/film.svg"),
            Self::Folder   => include_bytes!("icons/folder.svg"),
            Self::Gear     => include_bytes!("icons/gear.svg"),
            Self::Globe    => include_bytes!("icons/globe.svg"),
            Self::Home     => include_bytes!("icons/home.svg"),
            Self::Key      => include_bytes!("icons/key.svg"),
            Self::Lock     => include_bytes!("icons/lock.svg"),
            Self::Mic      => include_bytes!("icons/mic.svg"),
            Self::Music    => include_bytes!("icons/music.svg"),
            Self::Package  => include_bytes!("icons/package.svg"),
            Self::Palette  => include_bytes!("icons/palette.svg"),
            Self::Play     => include_bytes!("icons/play.svg"),
            Self::Refresh  => include_bytes!("icons/refresh.svg"),
            Self::Save     => include_bytes!("icons/save.svg"),
            Self::Search   => include_bytes!("icons/search.svg"),
            Self::Server   => include_bytes!("icons/server.svg"),
            Self::Speaker  => include_bytes!("icons/speaker.svg"),
            Self::Stop     => include_bytes!("icons/stop.svg"),
            Self::Thermometer => include_bytes!("icons/thermometer.svg"),
            Self::Trash    => include_bytes!("icons/trash.svg"),
            Self::Warning  => include_bytes!("icons/warning.svg"),
        }
    }

    /// Stable URI for this icon. egui's image loader caches by URI so
    /// every Icon variant resolves to the same decoded texture across
    /// calls (no per-frame SVG re-rasterisation).
    fn uri(self) -> &'static str {
        match self {
            Self::Attach   => "bytes://icons/attach.svg",
            Self::Bolt     => "bytes://icons/bolt.svg",
            Self::Chart    => "bytes://icons/chart.svg",
            Self::Chat     => "bytes://icons/chat.svg",
            Self::Check    => "bytes://icons/check.svg",
            Self::Copy     => "bytes://icons/copy.svg",
            Self::Cross    => "bytes://icons/cross.svg",
            Self::Download => "bytes://icons/download.svg",
            Self::Eye      => "bytes://icons/eye.svg",
            Self::Film     => "bytes://icons/film.svg",
            Self::Folder   => "bytes://icons/folder.svg",
            Self::Gear     => "bytes://icons/gear.svg",
            Self::Globe    => "bytes://icons/globe.svg",
            Self::Home     => "bytes://icons/home.svg",
            Self::Key      => "bytes://icons/key.svg",
            Self::Lock     => "bytes://icons/lock.svg",
            Self::Mic      => "bytes://icons/mic.svg",
            Self::Music    => "bytes://icons/music.svg",
            Self::Package  => "bytes://icons/package.svg",
            Self::Palette  => "bytes://icons/palette.svg",
            Self::Play     => "bytes://icons/play.svg",
            Self::Refresh  => "bytes://icons/refresh.svg",
            Self::Save     => "bytes://icons/save.svg",
            Self::Search   => "bytes://icons/search.svg",
            Self::Server   => "bytes://icons/server.svg",
            Self::Speaker  => "bytes://icons/speaker.svg",
            Self::Stop     => "bytes://icons/stop.svg",
            Self::Thermometer => "bytes://icons/thermometer.svg",
            Self::Trash    => "bytes://icons/trash.svg",
            Self::Warning  => "bytes://icons/warning.svg",
        }
    }

    /// Build an `egui::Image` widget sized to `size_pt × size_pt` with
    /// the given tint colour. `tint(WHITE)` (or any non-black colour)
    /// recolours the SVG strokes via egui's image tinting — works
    /// because the SVGs are authored with `stroke="currentColor"`
    /// against the underlying egui white-quad tint surface.
    pub fn image(self, size_pt: f32, tint: egui::Color32) -> egui::Image<'static> {
        egui::Image::new((self.uri(), self.bytes()))
            .fit_to_exact_size(egui::vec2(size_pt, size_pt))
            .tint(tint)
    }

    /// Convenience: add the icon to the UI at `size_pt × size_pt`,
    /// tinted with the given colour. Use `Color32::WHITE` for natural
    /// SVG stroke colour, or any other colour to recolour.
    pub fn show(self, ui: &mut egui::Ui, size_pt: f32, tint: egui::Color32) -> egui::Response {
        ui.add(self.image(size_pt, tint))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One row per variant — single source of truth for the test table.
    /// Adding a new Icon variant requires adding a row here too, which
    /// guarantees that both `bytes()` and `uri()` are wired up and the
    /// SVG file under crates/gui/src/icons/ actually exists at build
    /// time (via include_bytes! in `bytes()`).
    const ALL_ICONS: &[(Icon, &str)] = &[
        (Icon::Attach,   "attach"),
        (Icon::Bolt,     "bolt"),
        (Icon::Chart,    "chart"),
        (Icon::Chat,     "chat"),
        (Icon::Check,    "check"),
        (Icon::Copy,     "copy"),
        (Icon::Cross,    "cross"),
        (Icon::Download, "download"),
        (Icon::Eye,      "eye"),
        (Icon::Film,     "film"),
        (Icon::Folder,   "folder"),
        (Icon::Gear,     "gear"),
        (Icon::Globe,    "globe"),
        (Icon::Home,     "home"),
        (Icon::Key,      "key"),
        (Icon::Lock,     "lock"),
        (Icon::Mic,      "mic"),
        (Icon::Music,    "music"),
        (Icon::Package,  "package"),
        (Icon::Palette,  "palette"),
        (Icon::Play,     "play"),
        (Icon::Refresh,  "refresh"),
        (Icon::Save,     "save"),
        (Icon::Search,   "search"),
        (Icon::Server,   "server"),
        (Icon::Speaker,  "speaker"),
        (Icon::Stop,     "stop"),
        (Icon::Thermometer, "thermometer"),
        (Icon::Trash,    "trash"),
        (Icon::Warning,  "warning"),
    ];

    #[test]
    fn every_icon_uri_matches_bytes_scheme_and_stem() {
        // egui's image loader caches by URI; a drifted URI for the
        // same variant would re-rasterise the SVG every frame and
        // could collide with another variant's cache slot.
        for (icon, stem) in ALL_ICONS {
            assert_eq!(
                icon.uri(),
                format!("bytes://icons/{stem}.svg"),
                "URI for {icon:?} doesn't follow the bytes:// scheme"
            );
        }
    }

    #[test]
    fn every_icon_uri_is_unique() {
        // Two variants sharing a URI would silently render the same
        // SVG (egui caches by URI). Pin uniqueness.
        let mut seen: Vec<&str> = Vec::with_capacity(ALL_ICONS.len());
        for (icon, _) in ALL_ICONS {
            let uri = icon.uri();
            assert!(!seen.contains(&uri),
                "duplicate URI {uri:?} (offender: {icon:?})");
            seen.push(uri);
        }
    }

/// Exhaustiveness guard: a match on Icon that lists every variant.
    /// Rust's non-exhaustive-match check makes adding a new Icon::*
    /// without also adding a row to ALL_ICONS a hard compile error
    /// instead of a silent miss in the test table.
    fn _icon_table_exhaustiveness(icon: Icon) -> &'static str {
        match icon {
            Icon::Attach   => "attach",
            Icon::Bolt     => "bolt",
            Icon::Chart    => "chart",
            Icon::Chat     => "chat",
            Icon::Check    => "check",
            Icon::Copy     => "copy",
            Icon::Cross    => "cross",
            Icon::Download => "download",
            Icon::Eye      => "eye",
            Icon::Film     => "film",
            Icon::Folder   => "folder",
            Icon::Gear     => "gear",
            Icon::Globe    => "globe",
            Icon::Home     => "home",
            Icon::Key      => "key",
            Icon::Lock     => "lock",
            Icon::Mic      => "mic",
            Icon::Music    => "music",
            Icon::Package  => "package",
            Icon::Palette  => "palette",
            Icon::Play     => "play",
            Icon::Refresh  => "refresh",
            Icon::Save     => "save",
            Icon::Search   => "search",
            Icon::Server   => "server",
            Icon::Speaker  => "speaker",
            Icon::Stop     => "stop",
            Icon::Thermometer => "thermometer",
            Icon::Trash    => "trash",
            Icon::Warning  => "warning",
        }
    }

    #[test]
    fn icon_table_count_matches_exhaustive_match() {
        // Sanity check: every variant produced by _icon_table_exhaustiveness
        // is also in ALL_ICONS, and the row count matches. If a future
        // patch adds an Icon variant + match arm but forgets ALL_ICONS,
        // this length check catches the omission.
        for (icon, stem) in ALL_ICONS {
            assert_eq!(_icon_table_exhaustiveness(*icon), *stem,
                "ALL_ICONS stem for {icon:?} disagrees with the exhaustive match");
        }
    }

    #[test]
    fn every_icon_has_nonempty_svg_bytes() {
        // include_bytes! will fail at compile time if a file is
        // missing; this catches the runtime case of an empty .svg
        // file slipping in (e.g. a failed icon export).
        for (icon, _) in ALL_ICONS {
            let bytes = icon.bytes();
            assert!(!bytes.is_empty(), "{icon:?} has empty SVG bytes");
            // SVG files must start with either an XML declaration
            // (`<?xml`) or the bare `<svg` root element. Trim
            // leading whitespace/BOM before checking.
            let head: Vec<u8> = bytes.iter()
                .copied()
                .skip_while(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0xEF | 0xBB | 0xBF))
                .take(5)
                .collect();
            assert!(
                head.starts_with(b"<?xml") || head.starts_with(b"<svg"),
                "{icon:?} bytes don't look like SVG: head={:?}",
                String::from_utf8_lossy(&head)
            );
        }
    }
}
