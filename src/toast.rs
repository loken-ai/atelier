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

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};
use crate::theme;

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
    /// Accent colour from the shared theme palette so toasts speak the
    /// same colour language as status dots / chips elsewhere.
    fn color(self) -> Color32 {
        match self {
            ToastSeverity::Success => theme::success(),
            ToastSeverity::Warning => theme::warning(),
            ToastSeverity::Error => theme::error(),
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

    let text_color = theme::ink();
    let surface = if theme::is_dark() { theme::raised() } else { theme::panel() };

    let mut dismiss: Option<usize> = None;

    egui::Area::new(egui::Id::new("toast_stack"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
        .interactable(true)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                // bottom_up layout draws the first-iterated item lowest,
                // so iterate oldest→newest to stack newest on top.
                for (idx, toast) in toasts.iter().enumerate() {
                    let accent = toast.severity.color();
                    let resp = egui::Frame {
                        inner_margin: egui::Margin::symmetric(12, 8),
                        outer_margin: egui::Margin { top: 6, ..egui::Margin::same(0) },
                        corner_radius: CornerRadius::same(6),
                        fill: surface,
                        // 2px accent border carries the severity signal
                        // without washing the message text in colour.
                        stroke: Stroke::new(2.0, accent),
                        shadow: egui::epaint::Shadow {
                            offset: [0, 2],
                            blur: 6,
                            spread: 0,
                            color: Color32::from_black_alpha(if theme::is_dark() { 60 } else { 30 }),
                        },
                        ..Default::default()
                    }
                    .show(ui, |ui| {
                        ui.set_max_width(360.0);
                        ui.horizontal(|ui| {
                            // Coloured dot backs up the border colour so
                            // the severity reads even for viewers who
                            // can't distinguish the accent hue.
                            let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().circle_filled(dot.center(), 4.0, accent);
                            ui.add_space(4.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&toast.message).size(12.0).color(text_color),
                                )
                                .wrap(),
                            );
                        });
                    });
                    // Whole-toast click dismisses it early.
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
