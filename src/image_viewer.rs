//! Fullscreen image viewer with zoom and pan.
//!
//! Any inline image rendered through [`clickable_image`] becomes a click
//! target that opens a modal overlay covering the whole viewport. The overlay
//! supports wheel/pinch zoom, drag to pan, and closes on Escape, a click
//! outside the image, or the close button.
//!
//! The open request travels through egui's per-frame temp data
//! (`request_open_set`), so the many image call sites don't need to thread a
//! `&mut Option<FullscreenImage>` through their call chains — only the
//! top-level [`show`] owns the state and reads the request.

use egui::{Align2, CursorIcon, Id, Key, Order, Rect, Sense, Vec2};

use crate::icons::Icon;
use crate::theme::{self, text};
use crate::ui::widgets;

/// Live state of the fullscreen viewer. Holds owned [`egui::TextureHandle`]s
/// (cheap ref-counted clones) so the images stay valid even if the source
/// `image_textures` map is cleared while the viewer is open.
///
/// The viewer keeps the WHOLE set it was opened from, not just the image that
/// was clicked, so it can step through them without the caller staying alive or
/// re-opening the overlay per image.
pub struct FullscreenImage {
    texes: Vec<egui::TextureHandle>,
    /// Which of `texes` is on screen. Always a valid index: the set is never
    /// empty, and stepping wraps.
    idx: usize,
    /// Multiplier applied on top of the fit-to-screen scale (1.0 = fit).
    zoom: f32,
    /// Pan offset in screen points, relative to a centered image.
    pan: Vec2,
}

impl FullscreenImage {
    fn new(texes: Vec<egui::TextureHandle>, idx: usize) -> Self {
        let idx = if texes.is_empty() {
            0
        } else {
            idx.min(texes.len() - 1)
        };
        Self {
            texes,
            idx,
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }

    fn current(&self) -> &egui::TextureHandle {
        &self.texes[self.idx]
    }

    /// Step by `delta`, wrapping. Zoom and pan reset, because carrying a pan
    /// from one image onto a differently-sized one lands the viewer somewhere
    /// the user did not choose - often entirely off the new image.
    fn step(&mut self, delta: isize) {
        let n = self.texes.len();
        if n <= 1 {
            return;
        }
        let cur = self.idx as isize;
        self.idx = (cur + delta).rem_euclid(n as isize) as usize;
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
    }
}

const REQUEST_ID: &str = "fullscreen_image_open_request";
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 20.0;
/// Zoom per wheel point, and per click on the zoom buttons.
const WHEEL_RATE: f32 = 0.0015;
const ZOOM_STEP: f32 = 1.25;
/// Share of the viewport the fitted image may take, leaving room for the bar.
const FIT_W_FRAC: f32 = 0.92;
const FIT_H_FRAC: f32 = 0.88;
/// Distance of the control bar from the bottom edge.
const BAR_OFFSET_Y: f32 = 24.0;
/// Width of the counter and zoom readouts.
const COUNTER_W: f32 = 64.0;

/// Newtype so the open request can round-trip through egui temp data:
/// `remove_temp` requires `Default`, which `TextureHandle` does not implement.
#[derive(Clone, Default)]
struct OpenRequest(Option<(Vec<egui::TextureHandle>, usize)>);

/// Open on `texes[idx]` and let the viewer step through the whole set.
pub fn request_open_set(ctx: &egui::Context, texes: &[egui::TextureHandle], idx: usize) {
    if texes.is_empty() {
        return;
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            Id::new(REQUEST_ID),
            OpenRequest(Some((texes.to_vec(), idx))),
        )
    });
}

/// Render an inline image sized to `display_size` that opens the fullscreen
/// viewer when clicked. Drop-in replacement for
/// `ui.image(SizedTexture::new(tex.id(), display_size))`.
pub fn clickable_image(ui: &mut egui::Ui, tex: &egui::TextureHandle, display_size: Vec2) {
    clickable_image_in_set(ui, std::slice::from_ref(tex), 0, display_size);
}

/// Draw the image and return its response WITHOUT opening anything.
///
/// For callers that overlay their own controls on the image: they must decide
/// whether a click was meant for the picture or for a button sitting on top of
/// it, and only they know where those buttons ended up.
pub fn image_response(
    ui: &mut egui::Ui,
    tex: &egui::TextureHandle,
    display_size: Vec2,
    hint: &str,
) -> egui::Response {
    let image = egui::Image::new(egui::load::SizedTexture::new(tex.id(), display_size))
        .sense(Sense::click());
    ui.add(image)
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(hint.to_owned())
}

/// As [`clickable_image`], but the viewer it opens can step through `texes`.
/// Returns the image's response so callers can hang hover controls off its rect.
pub fn clickable_image_in_set(
    ui: &mut egui::Ui,
    texes: &[egui::TextureHandle],
    idx: usize,
    display_size: Vec2,
) -> egui::Response {
    let Some(tex) = texes.get(idx) else {
        return ui.allocate_response(Vec2::ZERO, Sense::hover());
    };
    let image = egui::Image::new(egui::load::SizedTexture::new(tex.id(), display_size))
        .sense(Sense::click());
    let hint = if texes.len() > 1 {
        "Click to view fullscreen (then use the arrow keys to browse)"
    } else {
        "Click to view fullscreen"
    };
    let resp = ui
        .add(image)
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(hint);
    if resp.clicked() {
        request_open_set(ui.ctx(), texes, idx);
    }
    resp
}

/// Drive the viewer: pick up any open request, then (if open) draw the overlay
/// and handle input. Call once per frame at the top level, above all tabs.
pub fn show(ctx: &egui::Context, state: &mut Option<FullscreenImage>) {
    if let Some(OpenRequest(Some((texes, idx)))) =
        ctx.data_mut(|d| d.remove_temp::<OpenRequest>(Id::new(REQUEST_ID)))
    {
        *state = Some(FullscreenImage::new(texes, idx));
    }
    let Some(fs) = state.as_mut() else { return };

    let screen = ctx.content_rect();
    let mut close = false;
    let mut step: isize = 0;

    // ── Backdrop + image + zoom/pan input ────────────────────────────────
    egui::Area::new(Id::new("fullscreen_image_overlay"))
        .order(Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let bg = ui.allocate_rect(screen, Sense::click_and_drag());
            ui.painter().rect_filled(screen, 0.0, theme::bg());

            let img = fs.current().size_vec2();
            // Fit the image inside the viewport with margins for the control bar.
            let fit = (screen.width() * FIT_W_FRAC / img.x.max(1.0))
                .min(screen.height() * FIT_H_FRAC / img.y.max(1.0))
                .max(0.001);

            // Wheel and pinch zoom, only while the pointer is over the overlay.
            let (scroll_y, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            if bg.hovered() {
                if scroll_y != 0.0 {
                    fs.zoom = (fs.zoom * (1.0 + scroll_y * WHEEL_RATE)).clamp(MIN_ZOOM, MAX_ZOOM);
                }
                if pinch != 1.0 {
                    fs.zoom = (fs.zoom * pinch).clamp(MIN_ZOOM, MAX_ZOOM);
                }
            }
            if bg.dragged() {
                fs.pan += bg.drag_delta();
            }

            let disp = img * fit * fs.zoom;
            let rect = Rect::from_center_size(screen.center() + fs.pan, disp);
            egui::Image::new(egui::load::SizedTexture::new(fs.current().id(), disp))
                .paint_at(ui, rect);

            // A click that lands outside the image dismisses the viewer.
            if bg.clicked() {
                let inside = bg
                    .interact_pointer_pos()
                    .map(|p| rect.contains(p))
                    .unwrap_or(false);
                if !inside {
                    close = true;
                }
            }
        });

    // ── Control bar (bottom center) ──────────────────────────────────────
    egui::Area::new(Id::new("fullscreen_image_controls"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_BOTTOM, [0.0, -BAR_OFFSET_Y])
        .show(ctx, |ui| {
            widgets::on_plate(ui, |ui| {
                ui.horizontal(|ui| {
                    // Browsing first: with several results open, stepping is the thing
                    // reached for most.
                    if fs.texes.len() > 1 {
                        if widgets::icon_button(ui, Icon::StepBack, "Previous image (Left arrow)")
                            .clicked()
                        {
                            step = -1;
                        }
                        widgets::readout(
                            ui,
                            COUNTER_W,
                            &format!("{} / {}", fs.idx + 1, fs.texes.len()),
                        );
                        if widgets::icon_button(ui, Icon::StepForward, "Next image (Right arrow)")
                            .clicked()
                        {
                            step = 1;
                        }
                        ui.separator();
                    }
                    if widgets::icon_button(ui, Icon::Minus, "Zoom out").clicked() {
                        fs.zoom = (fs.zoom / ZOOM_STEP).clamp(MIN_ZOOM, MAX_ZOOM);
                    }
                    widgets::readout(ui, COUNTER_W, &format!("{:>4.0}%", fs.zoom * 100.0));
                    if widgets::icon_button(ui, Icon::Plus, "Zoom in").clicked() {
                        fs.zoom = (fs.zoom * ZOOM_STEP).clamp(MIN_ZOOM, MAX_ZOOM);
                    }
                    ui.separator();
                    if ui
                        .add(egui::Button::new(text::note("Fit")).frame(false))
                        .on_hover_text("Reset zoom")
                        .clicked()
                    {
                        fs.zoom = 1.0;
                        fs.pan = Vec2::ZERO;
                    }
                    ui.separator();
                    if widgets::icon_button(ui, Icon::Cross, "Close (Esc)").clicked() {
                        close = true;
                    }
                });
            });
        });

    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        close = true;
    }
    // Arrow keys browse. Read them even when a control has focus: the overlay owns
    // the screen while it is open, so there is nothing else they could mean.
    if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
        step -= 1;
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowRight)) {
        step += 1;
    }
    if step != 0 {
        fs.step(step);
    }
    // Keep animating while open so drag/zoom feel smooth.
    ctx.request_repaint();

    if close {
        *state = None;
    }
}
