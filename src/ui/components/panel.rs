//! Recursive Panel Layout System
//!
//! Ported from VoxVST — a declarative tree of panels that alternate
//! horizontal/vertical stacking at each depth level.
//!
//! Usage:
//! ```ignore
//! Panel::branch()
//!     .child(Panel::branch()
//!         .header("SECTION A", ACCENT_HARDWARE)
//!         .child(Panel::leaf(|ui| { /* left column */ }))
//!         .child(Panel::leaf(|ui| { /* right column */ }))
//!     )
//!     .show(ui, 0);
//! ```

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use crate::theme;

// ── Spacing constants ──────────────────────────────────────────────
pub const SECTION_PADDING: f32 = 12.0;
pub const GAP_WIDGETS: f32 = 8.0;
pub const GAP_SECTIONS: f32 = 16.0;

// ── Recursive Panel ────────────────────────────────────────────────

/// Content of a panel node: either a leaf closure or child panels.
enum PanelContent<'a> {
    Leaf(Box<dyn FnOnce(&mut Ui) + 'a>),
    Branch(Vec<Panel<'a>>),
}

/// A recursive panel node. Builds a tree of panels that alternate
/// stacking direction at each depth level (horizontal → vertical → ...).
pub struct Panel<'a> {
    header: Option<(&'a str, Color32)>,
    enabled: bool,
    min_col_width: f32,
    content: PanelContent<'a>,
}

impl<'a> Panel<'a> {
    /// Create a leaf panel — content rendered directly.
    pub fn leaf(content: impl FnOnce(&mut Ui) + 'a) -> Self {
        Self {
            header: None,
            enabled: true,
            min_col_width: 200.0,
            content: PanelContent::Leaf(Box::new(content)),
        }
    }

    /// Create a branch panel that contains child panels.
    pub fn branch() -> Self {
        Self {
            header: None,
            enabled: true,
            min_col_width: 200.0,
            content: PanelContent::Branch(Vec::new()),
        }
    }

    /// Set minimum column width before wrapping (default 200.0).
    pub fn min_col_width(mut self, w: f32) -> Self {
        self.min_col_width = w;
        self
    }

    /// Add a section header with accent color.
    pub fn header(mut self, label: &'a str, accent: Color32) -> Self {
        self.header = Some((label, accent));
        self
    }

    /// Set enabled state. Disabled = reduced opacity overlay, content still rendered.
    #[allow(dead_code)]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Add a child panel (only meaningful on branch panels).
    pub fn child(mut self, child: Panel<'a>) -> Self {
        if let PanelContent::Branch(ref mut children) = self.content {
            children.push(child);
        }
        self
    }

    /// Render this panel at the given nesting depth.
    /// Even depth = children horizontal, odd depth = children vertical.
    pub fn show(self, ui: &mut Ui, depth: u32) {
        if depth >= 1 && self.header.is_some() {
            self.show_framed(ui, depth);
        } else {
            self.show_inner(ui, depth);
        }
    }

    /// Render with border and background — framed panels get a colored left accent stripe.
    fn show_framed(self, ui: &mut Ui, depth: u32) {
        let Panel { header, enabled, min_col_width, content } = self;
        let accent_color = header.map(|(_, c)| c);
        let dark = ui.visuals().dark_mode;

        let (fill, border) = if dark {
            (theme::panel(), theme::border())
        } else {
            (theme::panel(), theme::border())
        };

        let frame_resp = egui::Frame::NONE
            .fill(fill)
            .corner_radius(4.0)
            .inner_margin(SECTION_PADDING as i8)
            .stroke(Stroke::new(1.0, border))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                Self::show_content(ui, header, enabled, min_col_width, content, depth);
            });

        // Left accent stripe
        if let Some(color) = accent_color {
            let r = frame_resp.response.rect;
            ui.painter().rect_filled(
                Rect::from_min_size(r.left_top(), Vec2::new(3.0, r.height())),
                egui::CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 },
                color,
            );
        }
    }

    /// Render without border (child panels).
    fn show_inner(self, ui: &mut Ui, depth: u32) {
        let Panel { header, enabled, min_col_width, content } = self;
        Self::show_content(ui, header, enabled, min_col_width, content, depth);
    }

    fn show_content(
        ui: &mut Ui,
        header: Option<(&str, Color32)>,
        enabled: bool,
        min_col_width: f32,
        content: PanelContent<'a>,
        depth: u32,
    ) {
        if let Some((label, accent)) = header {
            section_header(ui, label, accent);
        }

        if !enabled {
            let rect_before = ui.cursor();
            ui.disable();
            Self::render_content(ui, content, min_col_width, depth);
            let bg = theme::bg();
            let full_rect = Rect::from_min_max(
                Pos2::new(rect_before.left(), rect_before.top()),
                Pos2::new(ui.min_rect().right(), ui.min_rect().bottom()),
            );
            ui.painter().rect_filled(
                full_rect,
                4.0,
                theme::tinted(bg, 140),
            );
        } else {
            Self::render_content(ui, content, min_col_width, depth);
        }
    }

    fn render_content(ui: &mut Ui, content: PanelContent<'a>, min_col_width: f32, depth: u32) {
        match content {
            PanelContent::Leaf(f) => {
                f(ui);
            }
            PanelContent::Branch(children) => {
                let n = children.len();
                if n == 0 {
                    return;
                }
                if depth.is_multiple_of(2) {
                    // Horizontal: children in columns
                    let avail_w = ui.available_width();
                    let cols_per_row = ((avail_w / min_col_width).floor() as usize).max(1).min(n);
                    let mut children_opt: Vec<Option<Panel>> =
                        children.into_iter().map(Some).collect();
                    for chunk in children_opt.chunks_mut(cols_per_row) {
                        let chunk_len = chunk.len();
                        ui.columns(chunk_len, |cols| {
                            for (i, child) in chunk.iter_mut().enumerate() {
                                if let Some(child) = child.take() {
                                    child.show(&mut cols[i], depth + 1);
                                }
                            }
                        });
                    }
                } else {
                    // Vertical: children stacked, separated by section gap
                    let mut first = true;
                    for child in children {
                        if !first {
                            if child.header.is_some() {
                                ui.add_space(GAP_SECTIONS);
                            } else {
                                ui.add_space(GAP_WIDGETS);
                                ui.separator();
                                ui.add_space(GAP_WIDGETS);
                            }
                        }
                        first = false;
                        ui.scope(|ui| {
                            child.show(ui, depth + 1);
                        });
                    }
                }
            }
        }
    }
}

// ── Section Header ─────────────────────────────────────────────────

/// Prominent section header with accent bar + tinted background.
///
/// `label` is rendered verbatim: callers pass it already uppercased. Doing the
/// uppercasing here allocates a small String on every render, for one call site
/// whose label is a static `"Local Models"`-shaped string; the convention lives
/// at the call site and removes the
/// per-frame allocation).
pub fn section_header(ui: &mut Ui, label: &str, accent: Color32) {
    let response = ui.horizontal(|ui| {
        let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(4.0, 18.0), Sense::hover());
        ui.painter().rect_filled(bar_rect, 2u8, accent);
        ui.label(RichText::new(label).color(accent).strong().size(12.0));
    });
    // Subtle tinted background behind header row
    let header_rect = response.response.rect;
    ui.painter_at(Rect::from_min_max(header_rect.min, header_rect.max)).rect_filled(
        header_rect.expand2(Vec2::new(4.0, 1.0)),
        2.0,
        theme::tinted(accent, 12),
    );
    ui.add_space(6.0);
}
