//! Global toast notifications.
//!
//! A unified, transient feedback channel. Before this, action results
//! were scattered: the Models tab showed a stale-persistent
//! `ActionStatus` banner, the chat tab had no notification surface at
//! all (chat_tab.rs noted "no toast"), and `TaskResult::Error` was
//! silent on any tab other than Models. Toasts fix that with one
//! bottom-right stack that auto-dismisses.
//!
//! The queue lives on `LLMGuiApp` (`toasts: Vec<Toast>`); push via
//! `LLMGuiApp::toast(sev, msg)`; render + expire in `update()` after
//! the CentralPanel via `toast::render`.

use crate::theme::{self, text, HAIRLINE};
use crate::ui::widgets;
use eframe::egui::{self, Color32};

/// Width of a toast, and its distance from the window's corner.
const TOAST_W: f32 = 360.0;
const TOAST_MARGIN: f32 = 16.0;
/// Gap between stacked toasts.
const TOAST_GAP: i8 = 6;

/// How long a toast stays on screen before auto-dismissing.
const TOAST_TTL: std::time::Duration = std::time::Duration::from_millis(4500);

/// Severity of a toast — drives its accent colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastSeverity {
    Success,
    Warning,
    Error,
}

impl ToastSeverity {
    /// The status colour of the palette.
    fn color(self) -> Color32 {
        match self {
            ToastSeverity::Success => theme::success(),
            ToastSeverity::Warning => theme::warning(),
            ToastSeverity::Error => theme::error(),
        }
    }

    /// The word beside the stripe, so the severity is never carried by the
    /// colour alone.
    fn label(self) -> &'static str {
        match self {
            ToastSeverity::Success => "OK",
            ToastSeverity::Warning => "WARNING",
            ToastSeverity::Error => "ERROR",
        }
    }
}

/// A single transient notification.
#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub severity: ToastSeverity,
    pub created_at: std::time::Instant,
}

impl Toast {
    pub fn new(severity: ToastSeverity, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            severity,
            created_at: std::time::Instant::now(),
        }
    }
}

/// Render the toast stack (bottom-right, newest on top), expire any
/// that have outlived `TOAST_TTL`, and schedule the next repaint so a
/// toast disappears on time even without user input. Clicking a toast
/// dismisses it early.
#[allow(deprecated)] // Area::show at ctx top-level, same rationale as layout.rs.
pub fn render(ctx: &egui::Context, toasts: &mut Vec<Toast>) {
    // Drop expired toasts first so we don't paint a frame of a toast
    // that's already past its TTL.
    toasts.retain(|t| t.created_at.elapsed() < TOAST_TTL);
    if toasts.is_empty() {
        return;
    }

    let mut dismiss: Option<usize> = None;

    egui::Area::new(egui::Id::new("toast_stack"))
        .anchor(
            egui::Align2::RIGHT_BOTTOM,
            egui::vec2(-TOAST_MARGIN, -TOAST_MARGIN),
        )
        .interactable(true)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                // bottom_up draws the first item lowest, so oldest first puts
                // the newest on top.
                for (idx, toast) in toasts.iter().enumerate() {
                    let tint = toast.severity.color();
                    let resp = egui::Frame::NONE
                        .fill(theme::raised())
                        .stroke(egui::Stroke::new(HAIRLINE, theme::border()))
                        .corner_radius(theme::RADIUS)
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .outer_margin(egui::Margin {
                            top: TOAST_GAP,
                            ..egui::Margin::same(0)
                        })
                        .show(ui, |ui| {
                            ui.set_max_width(TOAST_W);
                            ui.horizontal(|ui| {
                                ui.label(text::label(toast.severity.label()));
                                ui.add(
                                    egui::Label::new(
                                        text::note(&toast.message).color(theme::ink()),
                                    )
                                    .wrap(),
                                );
                            });
                        });
                    // The stripe runs along the frame itself, below its outer margin.
                    let rect = resp.response.rect;
                    widgets::stripe(
                        ui,
                        egui::Rect::from_min_max(
                            egui::pos2(rect.left(), rect.top() + TOAST_GAP as f32),
                            rect.right_bottom(),
                        ),
                        tint,
                    );
                    if resp.response.interact(egui::Sense::click()).clicked() {
                        dismiss = Some(idx);
                    }
                }
            });
        });

    if let Some(idx) = dismiss {
        toasts.remove(idx);
    }

    // Schedule a wake-up at the soonest expiry so toasts vanish on time
    // even if the user never moves the mouse.
    if let Some(soonest_remaining) = toasts
        .iter()
        .map(|t| TOAST_TTL.saturating_sub(t.created_at.elapsed()))
        .min()
    {
        ctx.request_repaint_after(soonest_remaining);
    }
}
