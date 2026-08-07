//! Hardware Dashboard Tab
//!
//! Dashboard-style UI: header → stats row (5 cards) → 50/50 split panes
//! Left pane: hardware devices + loaded models
//! Right pane: layer performance + summary

use eframe::egui;
use egui::{Color32, RichText, CornerRadius, Stroke, Vec2};

use crate::icons::Icon;
use crate::state::{DeviceInfo, HardwareState, ModelTopology};
use crate::theme;
use crate::state::LayerPerformance;
use crate::ui::components::panel::Panel;

// ── Theme-aware color helpers ──────────────────────────────────────────────

fn surface(dark: bool) -> Color32 {
    if dark {
        theme::surface()
    } else {
        theme::light::SURFACE
    }
}
fn elevated(dark: bool) -> Color32 {
    if dark {
        theme::surface_elevated()
    } else {
        theme::light::SURFACE_ELEVATED
    }
}
fn border(dark: bool) -> Color32 {
    if dark {
        theme::border()
    } else {
        theme::light::BORDER
    }
}
fn text_primary(dark: bool) -> Color32 {
    if dark {
        theme::text()
    } else {
        theme::light::TEXT
    }
}
fn text_secondary(dark: bool) -> Color32 {
    if dark {
        theme::text_secondary()
    } else {
        theme::light::TEXT_SECONDARY
    }
}
fn text_muted(dark: bool) -> Color32 {
    if dark {
        theme::text_muted()
    } else {
        theme::light::TEXT_MUTED
    }
}

/// Magenta accent for the Models stat card + the loaded-models cards
/// in the left pane. Hardware-tab-local (deliberately different from
/// theme::ACCENT_MODELS which the sidebar nav uses) — single source so
/// the stat card and the model_card frame strokes stay in lockstep.
const HW_MODELS_ACCENT: Color32 = Color32::from_rgb(160, 100, 180);

// ── Main entry ─────────────────────────────────────────────────────────────

/// Render the Hardware Dashboard tab. Returns true if a refresh was requested.
pub fn render(ui: &mut egui::Ui, hardware: &mut HardwareState, server_url: &str) -> bool {
    let mut refresh = false;
    let dark = ui.visuals().dark_mode;

    // Fixed top: header
    render_header(ui, hardware, server_url, &mut refresh, dark);
    ui.add_space(8.0);

    // Error banner (if any)
    if let Some(ref err) = hardware.error {
        render_error_banner(ui, err);
        ui.add_space(8.0);
    }

    // Stats row: 5 equal-width cards
    render_stats_row(ui, hardware, dark);
    ui.add_space(8.0);

    // Two-pane layout via Panel system (responsive horizontal split)
    let split_height = ui.available_height().max(100.0);

    // Borrow &HardwareState into both pane closures via the Panel's 'a
    // lifetime. Satisfying `move` captures instead means cloning HardwareState
    // twice per frame - the device, topology and layer-performance vectors plus
    // the in-flight snapshot - which at sixty frames a second with several devices
    // adds up; borrowing
    // is functionally identical and zero-cost.
    let hw_ref = &*hardware;

    Panel::branch()
        .min_col_width(300.0)
        .child(Panel::leaf(move |ui| {
            egui::ScrollArea::vertical()
                .id_salt("hw_left_pane")
                .max_height(split_height)
                .show(ui, |ui| {
                    render_devices_section(ui, hw_ref, dark);
                    ui.add_space(8.0);
                    render_models_section(ui, &hw_ref.model_topologies, dark);
                });
        }))
        .child(Panel::leaf(move |ui| {
            egui::ScrollArea::vertical()
                .id_salt("hw_right_pane")
                .max_height(split_height)
                .show(ui, |ui| {
                    render_energy_section(ui, hw_ref, dark);
                    ui.add_space(8.0);
                    render_performance_section(ui, hw_ref, dark);
                    ui.add_space(8.0);
                    render_scheduler_section(ui, hw_ref, dark);
                    ui.add_space(8.0);
                    render_summary_section(ui, hw_ref, dark);
                });
        }))
        .show(ui, 0);

    // No-op per-frame: the actual /api/layer_perf fetch happens in
    // App::refresh_hardware on Refresh-button click. Kept as a stable
    // hook so this tab doesn't need to know about app-layer plumbing.
    hardware.refresh_layer_performance();
    refresh
}

// ── Header bar ─────────────────────────────────────────────────────────────

fn render_header(
    ui: &mut egui::Ui,
    hardware: &mut HardwareState,
    server_url: &str,
    refresh: &mut bool,
    dark: bool,
) {
    let bg = if dark {
        Color32::from_rgb(38, 40, 48)
    } else {
        theme::light::SURFACE_ELEVATED
    };

    egui::Frame::NONE
        .fill(bg)
        .inner_margin(egui::vec2(12.0, 8.0))
        .corner_radius(theme::ROUNDING)
        .stroke(Stroke::new(1.0, border(dark)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                Icon::Chart.show(ui, 20.0, text_primary(dark));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Hardware Dashboard")
                        .size(16.0)
                        .strong()
                        .color(text_primary(dark)),
                );
                ui.separator();
                ui.label(RichText::new("●").color(theme::SUCCESS));
                ui.label(
                    RichText::new(server_url)
                        .size(11.0)
                        .color(text_secondary(dark)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if hardware.is_loading {
                        ui.spinner();
                        ui.label(
                            RichText::new("Refreshing…")
                                .size(11.0)
                                .color(text_secondary(dark)),
                        );
                    } else {
                        let btn = egui::Button::new(
                            RichText::new("Refresh").size(12.0).color(Color32::WHITE),
                        )
                        .fill(theme::PRIMARY)
                        .corner_radius(theme::ROUNDING);
                        if ui.add(btn).clicked() {
                            *refresh = true;
                            hardware.is_loading = true;
                            hardware.error = None;
                        }
                    }
                    if let Some(ref t) = hardware.last_refresh {
                        ui.label(
                            RichText::new(format!("Last: {t}"))
                                .size(10.0)
                                .color(text_muted(dark)),
                        );
                    }
                });
            });
        });
}

fn render_error_banner(ui: &mut egui::Ui, error: &str) {
    // theme::tinted(theme::ERROR, alpha) gives a translucent overlay
    // that reads correctly in BOTH dark and light mode — the previous
    // hardcoded Color32::from_rgb(80, 40, 40) + (255, 200, 200) text
    // pair was tuned for dark mode only, leaving light-mode users
    // with a near-black banner that didn't match the rest of the UI.
    egui::Frame::NONE
        .fill(theme::tinted(theme::ERROR, 60))
        .stroke(Stroke::new(1.0, theme::ERROR))
        .inner_margin(egui::vec2(10.0, 6.0))
        .corner_radius(theme::ROUNDING)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Tint the warning glyph with theme::ERROR (not
                // theme::WARNING) so the icon colour matches the
                // banner's red fill + border. The previous amber
                // tint on a red banner mixed the signal "this is a
                // warning, not an error" with "this is an error" —
                // the banner itself is for hardware-fetch errors
                // (refresh failure surfaced via hardware.error), so
                // error tone is the correct match.
                Icon::Warning.show(ui, 14.0, theme::ERROR);
                ui.add_space(4.0);
                ui.label(RichText::new(error).color(theme::ERROR));
            });
        });
}

// ── Stats row (5 equal-width cards) ────────────────────────────────────────

fn render_stats_row(ui: &mut egui::Ui, hw: &HardwareState, dark: bool) {
    ui.columns(5, |cols| {
        stat_card(
            &mut cols[0],
            dark,
            Icon::Server,  // physical compute devices
            "Devices",
            &format!("{}", hw.total_device_count()),
            &format!("{} avail", hw.available_device_count()),
            theme::PRIMARY,
        );
        stat_card(
            &mut cols[1],
            dark,
            Icon::Chart,   // memory as a measured quantity
            "Memory",
            &format!("{:.1} GB", hw.total_memory_gb),
            &format!("{:.1} GB usable", hw.usable_memory_gb),
            theme::SUCCESS,
        );
        stat_card(
            &mut cols[2],
            dark,
            Icon::Package, // models as packaged artifacts
            "Models",
            &format!("{}", hw.model_topologies.len()),
            if hw.is_distributed() {
                "Distributed"
            } else {
                "Single-node"
            },
            HW_MODELS_ACCENT,
        );
        stat_card(
            &mut cols[3],
            dark,
            Icon::Globe,   // remote / network servers
            "Servers",
            &format!("{}", hw.remote_servers.len()),
            if hw.remote_servers.is_empty() {
                "Local only"
            } else {
                "Connected"
            },
            theme::WARNING,
        );
        features_card(&mut cols[4], dark, hw);
    });
}

fn stat_card(
    ui: &mut egui::Ui,
    dark: bool,
    icon: Icon,
    title: &str,
    value: &str,
    subtitle: &str,
    accent: Color32,
) {
    egui::Frame::NONE
        .fill(surface(dark))
        .stroke(Stroke::new(1.0, accent.linear_multiply(0.5)))
        .inner_margin(egui::vec2(8.0, 6.0))
        .corner_radius(theme::ROUNDING)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                icon.show(ui, 14.0, accent);
                ui.label(RichText::new(title).size(10.0).color(text_secondary(dark)));
            });
            ui.label(
                RichText::new(value)
                    .size(18.0)
                    .strong()
                    .color(text_primary(dark)),
            );
            ui.label(RichText::new(subtitle).size(9.0).color(text_muted(dark)));
        });
}

fn features_card(ui: &mut egui::Ui, dark: bool, hw: &HardwareState) {
    egui::Frame::NONE
        .fill(surface(dark))
        .stroke(Stroke::new(1.0, border(dark)))
        .inner_margin(egui::vec2(8.0, 6.0))
        .corner_radius(theme::ROUNDING)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                Icon::Gear.show(ui, 14.0, text_secondary(dark));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Features")
                        .size(10.0)
                        .color(text_secondary(dark)),
                );
            });
            let (cuda_lbl, cuda_c) = if hw.compiled_features.cuda {
                (&format!("{} CUDA", theme::ICON_FILLED), theme::SUCCESS)
            } else {
                (&format!("{} CUDA", theme::ICON_EMPTY), text_muted(dark))
            };
            let (sycl_lbl, sycl_c) = if hw.compiled_features.sycl {
                (&format!("{} SYCL", theme::ICON_FILLED), theme::SUCCESS)
            } else {
                (&format!("{} SYCL", theme::ICON_EMPTY), text_muted(dark))
            };
            ui.label(RichText::new(cuda_lbl).size(10.0).color(cuda_c));
            ui.label(RichText::new(sycl_lbl).size(10.0).color(sycl_c));
        });
}

// ── Left pane: Hardware Devices ────────────────────────────────────────────

fn render_devices_section(ui: &mut egui::Ui, hw: &HardwareState, dark: bool) {
    section_frame(ui, dark, Icon::Gear, "Hardware Devices", |ui| {
        // Local devices
        if !hw.local_devices.is_empty() {
            ui.label(
                RichText::new("LOCAL")
                    .size(11.0)
                    .strong()
                    .color(theme::SUCCESS),
            );
            ui.add_space(4.0);
            for dev in &hw.local_devices {
                device_card(ui, dev, dark);
                ui.add_space(4.0);
            }
        }

        // Remote servers
        for srv in &hw.remote_servers {
            let status_c = remote_server_status_color(&srv.status);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("[{}]", srv.server_id))
                        .size(11.0)
                        .strong()
                        .color(theme::PRIMARY),
                );
                ui.label(
                    RichText::new(format!("{} ({})", srv.endpoint, srv.status))
                        .size(11.0)
                        .color(status_c),
                );
                ui.label(
                    RichText::new(format!("{:.0}ms", srv.latency_ms))
                        .size(10.0)
                        .color(text_muted(dark)),
                );
            });
            ui.add_space(4.0);
            for dev in &srv.devices {
                device_card(ui, dev, dark);
                ui.add_space(4.0);
            }
        }

        if hw.devices.is_empty() && !hw.is_loading {
            ui.label(
                RichText::new("No devices detected. Click Refresh.")
                    .color(text_muted(dark))
                    .italics(),
            );
        }
    });
}

fn device_card(ui: &mut egui::Ui, dev: &DeviceInfo, dark: bool) {
    let type_color = device_type_brand_color(&dev.device_type, dark);

    egui::Frame::NONE
        .fill(elevated(dark))
        .stroke(Stroke::new(1.0, type_color.linear_multiply(0.4)))
        .inner_margin(egui::vec2(8.0, 6.0))
        .corner_radius(theme::ROUNDING)
        .show(ui, |ui| {
            // Row 1: type badge + ID + name + status dot
            ui.horizontal(|ui| {
                egui::Frame::NONE
                    .fill(type_color.linear_multiply(0.2))
                    .inner_margin(egui::vec2(5.0, 2.0))
                    .corner_radius(CornerRadius::same(3))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&dev.device_type)
                                .size(9.0)
                                .strong()
                                .color(type_color),
                        );
                    });
                ui.label(
                    RichText::new(format!("#{}", dev.device_id))
                        .size(9.0)
                        .color(text_muted(dark)),
                );
                // Cap the device name: a card reports vendor, family and marketing
                // suffix together, which runs past thirty characters often enough to
                // push the right-anchored availability
                // dot off the card on narrow windows. Tooltip below
                // reveals the full name on hover so power users can
                // still read the marketing string.
                let name_display = crate::modality::truncate_with_ellipsis(&dev.name, 40);
                ui.label(
                    RichText::new(name_display.as_ref())
                        .size(11.0)
                        .color(text_primary(dark)),
                )
                .on_hover_text(&dev.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let c = if dev.available {
                        theme::SUCCESS
                    } else {
                        theme::WARNING
                    };
                    let dot = ui.label(RichText::new("●").color(c));
                    // Hover tooltip surfaces *why* an unavailable
                    // device is unavailable + the suggested remediation.
                    // For an available device, show the priority +
                    // status string as a smaller info hover.
                    if !dev.available {
                        let reason = dev.unavailable_reason.as_deref().unwrap_or("unavailable");
                        let suggestion = dev.unavailable_suggestion.as_deref();
                        dot.on_hover_ui(|ui| {
                            ui.label(RichText::new(reason).strong());
                            if let Some(s) = suggestion {
                                if !s.is_empty() {
                                    ui.add_space(2.0);
                                    ui.label(RichText::new(s).size(11.0));
                                }
                            }
                        });
                    } else {
                        // Lazy tooltip — keep the format! inside the
                        // hover closure so we only allocate when the
                        // user actually hovers the status dot. The
                        // Hardware tab refreshes every 200ms with N
                        // device cards visible, so building it eagerly costs one
                        // format!() per card five times a second, for nothing.
                        dot.on_hover_ui(|ui| {
                            ui.label(format!(
                                "{} (priority {})", dev.status, dev.priority
                            ));
                        });
                    }
                });
            });

            // Row 2: memory bar
            if dev.memory_bytes > 0 {
                ui.add_space(3.0);
                draw_memory_bar(ui, dev);
            }

            // Row 3: live telemetry chips (util / temp / power) when
            // NVML data is present. Skipped silently when fields are
            // None (non-CUDA devices) so non-NVIDIA boxes keep their
            // compact layout.
            let has_live = dev.utilization_gpu_percent.is_some()
                || dev.temperature_c.is_some()
                || dev.power_watts.is_some();
            if has_live {
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    if let Some(u) = dev.utilization_gpu_percent {
                        let c = if u > 80.0 { theme::WARNING } else { theme::SUCCESS };
                        let r = ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            Icon::Bolt.show(ui, 9.0, c);
                            ui.label(RichText::new(format!("{u:.0}%")).size(9.0).color(c));
                        }).response;
                        r.on_hover_text("Live GPU compute utilization (NVML)");
                    }
                    if let Some(t) = dev.temperature_c {
                        let c = if t > 80.0 { theme::ERROR }
                                else if t > 65.0 { theme::WARNING }
                                else { theme::SUCCESS };
                        let r = ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            Icon::Thermometer.show(ui, 9.0, c);
                            ui.label(RichText::new(format!("{t:.0}°C")).size(9.0).color(c));
                        }).response;
                        r.on_hover_text("GPU core temperature (NVML)");
                    }
                    if let (Some(p), Some(lim)) = (dev.power_watts, dev.power_limit_watts) {
                        let pct = if lim > 0.0 { p / lim * 100.0 } else { 0.0 };
                        let c = if pct > 90.0 { theme::WARNING } else { text_secondary(dark) };
                        let r = ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            Icon::Gear.show(ui, 9.0, c);
                            ui.label(RichText::new(format!("{p:.0}/{lim:.0}W")).size(9.0).color(c));
                        }).response;
                        r.on_hover_ui(|ui| {
                            ui.label(format!(
                                "Live power draw / TDP ({pct:.0}% of limit)"
                            ));
                        });
                    } else if let Some(p) = dev.power_watts {
                        let r = ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            Icon::Gear.show(ui, 9.0, text_secondary(dark));
                            ui.label(RichText::new(format!("{p:.0}W")).size(9.0).color(text_secondary(dark)));
                        }).response;
                        r.on_hover_text("Live power draw (NVML)");
                    }
                });
            }
        });
}

fn draw_memory_bar(ui: &mut egui::Ui, dev: &DeviceInfo) {
    let usage = dev.memory_usage_percent();
    let bar_color = memory_bar_color(usage);

    let width = ui.available_width().max(40.0);
    let height = 14.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());

    // Background
    ui.painter()
        .rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(25, 25, 30));

    // Filled portion
    let frac = (usage / 100.0).clamp(0.0, 1.0);
    if frac > 0.0 {
        let fill_rect =
            egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * frac, rect.height()));
        ui.painter()
            .rect_filled(fill_rect, CornerRadius::same(3), bar_color);
    }

    // Border
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(3),
        Stroke::new(1.0, Color32::from_gray(60)),
        egui::epaint::StrokeKind::Outside,
    );

    // Text overlay
    let label = if dev.available_memory_bytes > 0 && dev.available_memory_bytes < dev.memory_bytes {
        format!(
            "{:.1}/{:.1} GB ({:.0}%)",
            dev.memory_gb() - dev.available_memory_gb(),
            dev.memory_gb(),
            usage
        )
    } else {
        format!("{:.1} GB", dev.memory_gb())
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(9.0),
        Color32::WHITE,
    );
}

// ── Left pane: Loaded Models ───────────────────────────────────────────────

fn render_models_section(ui: &mut egui::Ui, models: &[ModelTopology], dark: bool) {
    section_frame(ui, dark, Icon::Package, "Loaded Models", |ui| {
        if models.is_empty() {
            ui.label(
                RichText::new("No models loaded.")
                    .color(text_muted(dark))
                    .italics(),
            );
            return;
        }
        for model in models {
            model_card(ui, model, dark);
            ui.add_space(4.0);
        }
    });
}

fn model_card(ui: &mut egui::Ui, model: &ModelTopology, dark: bool) {
    let accent = HW_MODELS_ACCENT;

    egui::Frame::NONE
        .fill(elevated(dark))
        .stroke(Stroke::new(1.0, accent.linear_multiply(0.4)))
        .inner_margin(egui::vec2(8.0, 6.0))
        .corner_radius(theme::ROUNDING)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("◆").size(14.0));
                // Cap the model_id label so a long HF path (e.g.
                // "Tongyi-MAI/Z-Image-Turbo-Distilled-v2-FP16",
                // sometimes 50+ chars with org/repo/variant) doesn't
                // wrap the row to a second line and push the size
                // suffix off the visible card. Tooltip carries the
                // full id for users who need the exact string.
                let id_display = crate::modality::truncate_with_ellipsis(&model.model_id, 50);
                ui.label(
                    RichText::new(id_display.as_ref())
                        .size(12.0)
                        .strong()
                        .color(text_primary(dark)),
                )
                .on_hover_text(&model.model_id);
                ui.label(
                    RichText::new(format!("({:.1} GB)", model.size_gb()))
                        .size(10.0)
                        .color(text_secondary(dark)),
                );
            });

            if !model.layer_distribution.is_empty() {
                ui.add_space(3.0);
                ui.label(
                    RichText::new(format!("{} layers total", model.total_layers))
                        .size(10.0)
                        .color(text_secondary(dark)),
                );

                // Layer distribution grid
                egui::Grid::new(format!(
                    "layers_{}",
                    model.model_id.replace(['/', ':', '.'], "_")
                ))
                .num_columns(4)
                .spacing([8.0, 2.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Location").size(9.0).color(text_muted(dark)));
                    ui.label(RichText::new("Device").size(9.0).color(text_muted(dark)));
                    ui.label(RichText::new("Layers").size(9.0).color(text_muted(dark)));
                    ui.label(RichText::new("Memory").size(9.0).color(text_muted(dark)));
                    ui.end_row();

                    for ld in &model.layer_distribution {
                        // Common case ("LOCAL") takes the borrowed branch
                        // so we skip a `.to_string()` per row per refresh.
                        // Remote-server rows still allocate the bracketed
                        // form since the server-id varies.
                        let loc_owned;
                        let loc: &str = if ld.location == "LOCAL" {
                            "LOCAL"
                        } else {
                            loc_owned = format!("[{}]", ld.location);
                            &loc_owned
                        };
                        ui.label(RichText::new(loc).size(9.0).color(theme::PRIMARY));
                        ui.label(
                            RichText::new(format!("{} #{}", ld.device_type, ld.device_id))
                                .size(9.0)
                                .color(text_secondary(dark)),
                        );
                        // The API's layer_end is INCLUSIVE ("Last layer index
                        // (inclusive)" — api::types::LayerDistribution), so a
                        // segment covering layers 0..=13 renders "L0-13". The
                        // old `end - 1` here dropped the last layer of every
                        // segment (a 40-layer model showed "L0-38").
                        let range = if ld.layer_range.1 > ld.layer_range.0 {
                            format!("L{}-{}", ld.layer_range.0, ld.layer_range.1)
                        } else {
                            format!("L{}", ld.layer_range.0)
                        };
                        ui.label(RichText::new(range).size(9.0).color(text_primary(dark)));
                        ui.label(
                            RichText::new(format!("{:.2} GB", ld.memory_gb()))
                                .size(9.0)
                                .color(theme::SUCCESS),
                        );
                        ui.end_row();
                    }
                });
            } else {
                ui.label(
                    RichText::new("Layer distribution not available.")
                        .size(9.0)
                        .color(text_muted(dark))
                        .italics(),
                );
            }
        });
}

// ── Right pane: Layer Performance ──────────────────────────────────────────

fn render_performance_section(ui: &mut egui::Ui, hw: &HardwareState, dark: bool) {
    section_frame(ui, dark, Icon::Bolt, "Layer Performance", |ui| {
        if hw.layer_performance.is_empty() {
            ui.label(
                RichText::new("No performance data yet. Metrics appear during inference.")
                    .color(text_muted(dark))
                    .italics(),
            );
            return;
        }

        // Group layers by model name
        let mut models: Vec<String> = Vec::new();
        for layer in &hw.layer_performance {
            if !models.contains(&layer.model_name) {
                models.push(layer.model_name.clone());
            }
        }
        let show_model_header = models.len() > 1 || models.first().is_some_and(|m| !m.is_empty());

        ui.label(
            RichText::new(format!("{} layers tracked", hw.layer_performance.len()))
                .size(10.0)
                .color(text_secondary(dark)),
        );
        ui.add_space(4.0);

        for model in &models {
            let model_layers: Vec<&LayerPerformance> = hw.layer_performance.iter()
                .filter(|l| l.model_name == *model)
                .collect();
            if model_layers.is_empty() { continue; }

            // Show model header when multiple models are loaded.
            // Cap the visible name at 50 chars (matches the
            // model-topology card cap in model_card so the two
            // surfaces displaying the same HF id stay visually
            // consistent) with hover-for-full so users can verify
            // the exact model when correlating per-layer perf data
            // back to the topology card above.
            if show_model_header {
                let label = if model.is_empty() { "LLM" } else { model.as_str() };
                let label_display = crate::modality::truncate_with_ellipsis(label, 50);
                ui.add_space(4.0);
                ui.label(
                    RichText::new(label_display.as_ref())
                        .size(11.0)
                        .strong()
                        .color(theme::PRIMARY),
                )
                .on_hover_text(label);
                ui.add_space(2.0);
            }

            // Performance grid
            let grid_id = format!("layer_perf_grid_{}", model);
            egui::Grid::new(grid_id)
                .num_columns(6)
                .spacing([8.0, 3.0])
                .striped(true)
                .show(ui, |ui| {
                    for col in ["Layer", "Device", "Speed", "Time", "Tokens", "Exits"] {
                        ui.label(RichText::new(col).size(9.0).strong().color(text_secondary(dark)));
                    }
                    ui.end_row();

                    for layer in &model_layers {
                        perf_row(ui, layer, dark);
                    }
                });
        }
    });
}

fn perf_row(ui: &mut egui::Ui, layer: &LayerPerformance, dark: bool) {
    let speed_color = if layer.tokens_per_second > 10.0 {
        theme::SUCCESS
    } else if layer.tokens_per_second > 1.0 {
        theme::WARNING
    } else {
        theme::ERROR
    };

    // Color code device types
    let dc = if layer.device_type.contains("CUDA") {
        Color32::from_rgb(118, 185, 0)  // Green for CUDA
    } else if layer.device_type.contains("Arc") {
        Color32::from_rgb(66, 165, 245)  // Blue for Arc/OpenCL
    } else if layer.device_type.contains("CPU") {
        Color32::from_rgb(156, 39, 176)  // Purple for CPU
    } else {
        text_secondary(dark)
    };

    ui.label(
        RichText::new(format!("#{}", layer.layer_idx))
            .size(9.0)
            .color(theme::PRIMARY),
    );
    ui.label(RichText::new(&layer.device_type).size(9.0).color(dc).strong());
    ui.label(
        RichText::new(format!("{:.1} tok/s", layer.tokens_per_second))
            .size(9.0)
            .strong()
            .color(speed_color),
    );
    ui.label(
        RichText::new(format!("{:.1}ms", layer.avg_ms_per_token))
            .size(9.0)
            .color(text_secondary(dark)),
    );
    ui.label(
        RichText::new(format!("{}", layer.token_count))
            .size(9.0)
            .color(text_muted(dark)),
    );
    if layer.early_exit_count > 0 {
        ui.label(
            RichText::new(format!("{}", layer.early_exit_count))
                .size(9.0)
                .strong()
                .color(Color32::from_rgb(200, 140, 40)),
        );
    } else {
        ui.label(
            RichText::new("—")
                .size(9.0)
                .color(text_muted(dark)),
        );
    }
    ui.end_row();
}

// ── Right pane: Summary ────────────────────────────────────────────────────

/// Phase 9: scheduler / in-flight requests panel. Polls /api/inflight via
/// `HardwareState.inflight` (refreshed by the periodic refresh_hardware
/// loop) and renders the running + queued snapshot.
fn render_scheduler_section(ui: &mut egui::Ui, hw: &HardwareState, dark: bool) {
    section_frame(ui, dark, Icon::Play, "Active Requests", |ui| {
        let snap = match hw.inflight.as_ref() {
            Some(s) => s,
            None => {
                ui.label(
                    RichText::new("(no scheduler data — fetch in progress or server idle)")
                        .size(11.0).italics()
                        .color(text_secondary(dark)),
                );
                return;
            }
        };

        // Counter strip. Labels carry their trailing ":" as part of the
        // static string so the chip helper doesn't allocate a
        // `format!("{}: ", label)` per chip per render — at the 5 Hz
        // Hardware-tab refresh rate that was 5 × 5 = 25 short String
        // allocs/sec for unchanging text. The value still goes through
        // format!() since it varies, but the label side is zero-alloc.
        ui.horizontal(|ui| {
            let chip = |ui: &mut egui::Ui, label: &'static str, val: usize, c: Color32| {
                ui.label(RichText::new(label).size(11.0).color(text_secondary(dark)));
                ui.label(RichText::new(format!("{}", val)).size(11.0).strong().color(c));
                ui.add_space(8.0);
            };
            chip(ui, "running:", snap.in_flight, theme::SUCCESS);
            chip(ui, "queued:", snap.queue_depth,
                if snap.queue_depth == 0 { text_primary(dark) } else { theme::PRIMARY });
            ui.label(RichText::new("(").size(11.0).color(text_secondary(dark)));
            chip(ui, "FIM:", snap.queued_fim, theme::PRIMARY);
            chip(ui, "Int:", snap.queued_interactive, text_primary(dark));
            chip(ui, "Bat:", snap.queued_batch, text_secondary(dark));
            ui.label(RichText::new(")").size(11.0).color(text_secondary(dark)));
        });

        if snap.requests.is_empty() {
            ui.add_space(2.0);
            ui.label(
                RichText::new("idle — no active requests")
                    .size(11.0).italics()
                    .color(text_secondary(dark)),
            );
            return;
        }

        ui.add_space(4.0);
        let now_ms = chrono::Utc::now().timestamp_millis();
        egui::Grid::new("scheduler_grid")
            .num_columns(5)
            .spacing([10.0, 2.0])
            .striped(true)
            .show(ui, |ui| {
                // Header row
                let hdr = |s: &str| RichText::new(s).size(10.0).strong().color(text_secondary(dark));
                ui.label(hdr("state"));
                ui.label(hdr("priority"));
                ui.label(hdr("model"));
                ui.label(hdr("endpoint"));
                ui.label(hdr("elapsed"));
                ui.end_row();

                for r in &snap.requests {
                    let (state_lbl, state_c) = match r.state.as_str() {
                        "running" => ("● run", theme::SUCCESS),
                        "queued" => ("◌ wait", theme::PRIMARY),
                        other => (other, text_primary(dark)),
                    };
                    ui.label(RichText::new(state_lbl).size(11.0).color(state_c));

                    let prio_c = match r.priority.as_str() {
                        "fim" => theme::PRIMARY,
                        "interactive" => text_primary(dark),
                        "batch" => text_secondary(dark),
                        _ => text_primary(dark),
                    };
                    ui.label(RichText::new(&r.priority).size(11.0).color(prio_c));

                    // Model name truncate. Two fixes to the previous
                    // inline byte-slice:
                    //   - `&r.model[..23]` would PANIC at runtime
                    //     if r.model has a multi-byte UTF-8 char
                    //     spanning the boundary at byte 23 (e.g. a
                    //     model name with CJK or emoji chars in the
                    //     publisher segment of a HF id). Reuse the
                    //     UTF-8-safe truncate_with_ellipsis helper
                    //     that the rest of the GUI standardised on.
                    //   - Add a hover tooltip showing the full name
                    //     so users debugging a stalled request can
                    //     see the exact model id (the scheduler
                    //     panel is exactly where you'd look during
                    //     a hang investigation).
                    let m_display = crate::modality::truncate_with_ellipsis(&r.model, 24);
                    ui.label(
                        RichText::new(m_display.as_ref()).size(11.0).color(text_primary(dark)),
                    )
                    .on_hover_text(&r.model);

                    ui.label(RichText::new(&r.endpoint).size(10.0).color(text_secondary(dark)));

                    let started = r.started_at_ms.unwrap_or(r.queued_at_ms);
                    let elapsed_ms = (now_ms - started).max(0);
                    let elapsed = if elapsed_ms < 1000 {
                        format!("{}ms", elapsed_ms)
                    } else {
                        format!("{:.1}s", elapsed_ms as f32 / 1000.0)
                    };
                    ui.label(RichText::new(elapsed).size(10.0).color(text_secondary(dark)));
                    ui.end_row();
                }
            });
    });
}

fn render_summary_section(ui: &mut egui::Ui, hw: &HardwareState, dark: bool) {
    section_frame(ui, dark, Icon::Chart, "Summary", |ui| {
        egui::Grid::new("summary_grid")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Total devices:")
                        .size(11.0)
                        .color(text_secondary(dark)),
                );
                ui.label(
                    RichText::new(format!(
                        "{} ({} available)",
                        hw.total_device_count(),
                        hw.available_device_count()
                    ))
                    .size(11.0)
                    .color(text_primary(dark)),
                );
                ui.end_row();

                ui.label(
                    RichText::new("Remote servers:")
                        .size(11.0)
                        .color(text_secondary(dark)),
                );
                ui.label(
                    RichText::new(format!(
                        "{}{}",
                        hw.remote_servers.len(),
                        if hw.is_distributed() {
                            " (distributed)"
                        } else {
                            ""
                        }
                    ))
                    .size(11.0)
                    .color(text_primary(dark)),
                );
                ui.end_row();

                ui.label(
                    RichText::new("Total memory:")
                        .size(11.0)
                        .color(text_secondary(dark)),
                );
                ui.label(
                    RichText::new(format!("{:.1} GB", hw.total_memory_gb))
                        .size(11.0)
                        .color(text_primary(dark)),
                );
                ui.end_row();

                ui.label(
                    RichText::new("Usable memory:")
                        .size(11.0)
                        .color(text_secondary(dark)),
                );
                ui.label(
                    RichText::new(format!("{:.1} GB", hw.usable_memory_gb))
                        .size(11.0)
                        .color(text_primary(dark)),
                );
                ui.end_row();

                ui.label(
                    RichText::new("Models loaded:")
                        .size(11.0)
                        .color(text_secondary(dark)),
                );
                ui.label(
                    RichText::new(format!("{}", hw.model_topologies.len()))
                        .size(11.0)
                        .color(text_primary(dark)),
                );
                ui.end_row();

                ui.label(
                    RichText::new("Inference mode:")
                        .size(11.0)
                        .color(text_secondary(dark)),
                );
                let (mode, mode_c) = if hw.is_distributed() {
                    ("DISTRIBUTED", theme::PRIMARY)
                } else {
                    ("SINGLE-NODE", theme::SUCCESS)
                };
                ui.label(RichText::new(mode).size(11.0).strong().color(mode_c));
                ui.end_row();
            });
    });
}

// ── Right pane: Energy / environmental impact ──────────────────────────────

/// Cumulative session energy card: headline Wh + gCO₂, everyday
/// equivalences, a per-modality breakdown and the request count. Driven
/// by `hw.energy` (polled from /api/distributed/devices). When the server
/// has energy reporting disabled the field is `None` and we show a subtle
/// hint instead of stale zeros.
fn render_energy_section(ui: &mut egui::Ui, hw: &HardwareState, dark: bool) {
    section_frame(ui, dark, Icon::Bolt, "Énergie", |ui| {
        let Some(e) = hw.energy.as_ref() else {
            ui.label(
                RichText::new("Mesure d'énergie désactivée")
                    .size(11.0)
                    .italics()
                    .color(text_muted(dark)),
            );
            return;
        };

        if e.requests == 0 {
            ui.label(
                RichText::new("Aucune requête mesurée pour l'instant.")
                    .size(11.0)
                    .italics()
                    .color(text_muted(dark)),
            );
            return;
        }

        // Headline: session Wh (green = energy) + gCO₂ (amber), large.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(
                RichText::new(fmt_energy_wh(e.total_wh))
                    .size(22.0)
                    .strong()
                    .color(theme::SUCCESS),
            );
            ui.label(
                RichText::new("·")
                    .size(18.0)
                    .color(text_muted(dark)),
            );
            ui.label(
                RichText::new(fmt_co2(e.total_gco2))
                    .size(22.0)
                    .strong()
                    .color(theme::WARNING),
            );
        });
        ui.label(
            RichText::new(format!(
                "sur {} requête{} · {} d'eau",
                e.requests,
                if e.requests > 1 { "s" } else { "" },
                fmt_water(e.total_water_l),
            ))
            .size(10.0)
            .color(text_muted(dark)),
        );

        ui.add_space(6.0);

        // Everyday equivalences (compact, one line, wraps if narrow).
        let eq = &e.session_equiv;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(
                RichText::new(format!("≈ {} d'ampoule LED", fmt_led(eq.led_bulb_minutes)))
                    .size(11.0)
                    .color(text_secondary(dark)),
            );
            ui.label(RichText::new("·").size(11.0).color(text_muted(dark)));
            ui.label(
                RichText::new(fmt_charges(eq.smartphone_charges))
                    .size(11.0)
                    .color(text_secondary(dark)),
            );
            ui.label(RichText::new("·").size(11.0).color(text_muted(dark)));
            ui.label(
                RichText::new(fmt_cups(eq.hot_water_cups))
                    .size(11.0)
                    .color(text_secondary(dark)),
            );
        });

        // Per-modality breakdown (text / image / audio / video / tts).
        if !e.by_modality.is_empty() {
            ui.add_space(6.0);
            // Deterministic order: descending by energy so the biggest
            // consumer is always on top (HashMap iteration order is
            // otherwise random frame-to-frame → visual jitter).
            let mut rows: Vec<(&String, f64)> =
                e.by_modality.iter().map(|(k, j)| (k, *j)).collect();
            rows.sort_by(|a, b| b.1.total_cmp(&a.1));
            for (modality, joules) in rows {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(modality_label(modality))
                            .size(11.0)
                            .color(text_secondary(dark)),
                    );
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(
                                RichText::new(fmt_energy_wh(joules / 3600.0))
                                    .size(11.0)
                                    .color(text_primary(dark)),
                            );
                        },
                    );
                });
            }
        }
    });
}

/// Human-friendly energy (mWh ↔ Wh ↔ kWh) — mirrors the server's fmt_wh.
fn fmt_energy_wh(wh: f64) -> String {
    if wh >= 1000.0 {
        format!("{:.2} kWh", wh / 1000.0)
    } else if wh >= 1.0 {
        format!("{:.2} Wh", wh)
    } else {
        format!("{:.1} mWh", wh * 1000.0)
    }
}

/// Human-friendly CO₂ mass (g ↔ kg).
fn fmt_co2(g: f64) -> String {
    if g >= 1000.0 {
        format!("{:.2} kg CO₂", g / 1000.0)
    } else if g >= 1.0 {
        format!("{:.1} g CO₂", g)
    } else {
        format!("{:.2} g CO₂", g)
    }
}

/// Water volume (mL ↔ L).
fn fmt_water(liters: f64) -> String {
    if liters >= 1.0 {
        format!("{:.1} L", liters)
    } else {
        format!("{:.0} mL", liters * 1000.0)
    }
}

/// LED-bulb equivalent as a duration (min ↔ h).
fn fmt_led(minutes: f64) -> String {
    if minutes >= 60.0 {
        format!("{:.1} h", minutes / 60.0)
    } else if minutes >= 1.0 {
        format!("{:.0} min", minutes)
    } else {
        format!("{:.0} s", minutes * 60.0)
    }
}

/// Smartphone-charge equivalent — fractions read as a percentage of one.
fn fmt_charges(n: f64) -> String {
    if n >= 1.0 {
        format!("{:.1} charges de smartphone", n)
    } else {
        format!("{:.0}% d'une charge de smartphone", n * 100.0)
    }
}

/// Hot-water-cup equivalent — fractions read as a percentage of one.
fn fmt_cups(c: f64) -> String {
    if c >= 1.0 {
        format!("{:.1} tasses d'eau chaude", c)
    } else {
        format!("{:.0}% d'une tasse d'eau chaude", c * 100.0)
    }
}

/// Localised modality label for the per-modality breakdown rows.
fn modality_label(m: &str) -> String {
    match m {
        "text" => "Texte",
        "image" => "Image",
        "audio" => "Audio",
        "video" => "Vidéo",
        "tts" => "Synthèse vocale",
        other => other,
    }
    .to_string()
}

// ── Shared: section frame with icon + title ────────────────────────────────

fn section_frame(
    ui: &mut egui::Ui,
    dark: bool,
    icon: Icon,
    title: &str,
    content: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::NONE
        .fill(surface(dark))
        .stroke(Stroke::new(1.0, border(dark)))
        .inner_margin(egui::vec2(10.0, 8.0))
        .corner_radius(theme::ROUNDING)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                icon.show(ui, 15.0, text_primary(dark));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(title)
                        .size(14.0)
                        .strong()
                        .color(text_primary(dark)),
                );
            });
            ui.add_space(6.0);
            content(ui);
        });
}

/// Map a memory-usage percentage to the per-device VRAM-bar fill colour.
///
/// Three tiers, picked to match the at-a-glance "how much VRAM do I have
/// left" question users ask when watching the Hardware tab:
///   - ≤ 70 % used → SUCCESS (green) — plenty of headroom
///   - 70-90 %     → WARNING (amber) — getting tight, plan ahead
///   - > 90 %      → ERROR (red)   — about to OOM on next allocation
///
/// Boundaries chosen to be friendly with NVML's free-VRAM refresh
/// granularity (memory pressure crossing 70 / 90 % is a real signal,
/// not jitter). Extracted from `draw_memory_bar` so the tier logic
/// can be unit-tested without an egui Ui in scope.
fn memory_bar_color(usage_percent: f32) -> Color32 {
    if usage_percent > 90.0 {
        theme::ERROR
    } else if usage_percent > 70.0 {
        theme::WARNING
    } else {
        theme::SUCCESS
    }
}

/// Per-status colour for a remote inference server's row. The Hardware
/// tab lists each remote node with an "Online / Degraded / Offline"
/// status label; the user reads the colour first (green/amber/red)
/// to spot which nodes need attention. Unknown statuses default to
/// red — pessimistic so a misspelled status string from a future
/// server version surfaces as "needs check", not silently green.
fn remote_server_status_color(status: &str) -> Color32 {
    match status {
        "Online"   => theme::SUCCESS,
        "Degraded" => theme::WARNING,
        _          => theme::ERROR,
    }
}

/// Brand colour for a device card based on its device_type string.
/// Maps to the vendor's official brand hue when known (NVIDIA green
/// for CUDA, Intel blue for Arc/Xe), otherwise to a neutral secondary
/// text colour. Pin this so the user's at-a-glance vendor
/// identification stays correct.
fn device_type_brand_color(device_type: &str, dark: bool) -> Color32 {
    match device_type {
        "CUDA" => Color32::from_rgb(118, 185, 0),       // NVIDIA brand green
        "Intel Arc" | "Intel Xe" => Color32::from_rgb(0, 113, 197), // Intel blue
        _ => text_secondary(dark),                       // CPU / unknown
    }
}

#[cfg(test)]
mod brand_color_tests {
    use super::*;

    #[test]
    fn remote_server_status_color_maps_documented_states() {
        // The three documented states map to a distinct semantic colour.
        // A regression that flipped Online → red or Degraded → green
        // would mislead the operator at a glance.
        assert_eq!(remote_server_status_color("Online"),   theme::SUCCESS);
        assert_eq!(remote_server_status_color("Degraded"), theme::WARNING);
        // Anything else is pessimistic-red ("needs check").
        assert_eq!(remote_server_status_color("Offline"),    theme::ERROR);
        assert_eq!(remote_server_status_color(""),           theme::ERROR);
        assert_eq!(remote_server_status_color("Maintenance"),theme::ERROR);
        assert_eq!(remote_server_status_color("online"),     theme::ERROR,
            "lowercase variant is unknown → pessimistic red");
    }

    #[test]
    fn remote_server_status_tiers_are_distinct() {
        // Three semantic tiers, three distinct colours.
        assert_ne!(remote_server_status_color("Online"),   remote_server_status_color("Degraded"));
        assert_ne!(remote_server_status_color("Degraded"), remote_server_status_color("Offline"));
        assert_ne!(remote_server_status_color("Online"),   remote_server_status_color("Offline"));
    }

    #[test]
    fn device_type_brand_color_uses_vendor_hues_for_known_brands() {
        // NVIDIA brand green (118, 185, 0) — recognisable vendor colour.
        let cuda = device_type_brand_color("CUDA", true);
        assert_eq!(cuda, Color32::from_rgb(118, 185, 0));
        // Intel brand blue (0, 113, 197) — both Arc and Xe map to it.
        let arc = device_type_brand_color("Intel Arc", true);
        let xe  = device_type_brand_color("Intel Xe", true);
        let intel_blue = Color32::from_rgb(0, 113, 197);
        assert_eq!(arc, intel_blue);
        assert_eq!(xe,  intel_blue);
        // CUDA and Intel must be distinct (green ≠ blue).
        assert_ne!(cuda, arc);
    }

    #[test]
    fn device_type_brand_color_falls_back_to_secondary_for_unknown() {
        // CPU / unknown / future vendor → neutral secondary text colour.
        // Pin that the fallback is NOT the same as either branded
        // colour (which would mislead identification).
        let cpu  = device_type_brand_color("CPU", true);
        let amd  = device_type_brand_color("AMD", true);  // not yet branded
        let empty = device_type_brand_color("", true);
        assert_eq!(cpu, amd);
        assert_eq!(cpu, empty);
        assert_ne!(cpu, Color32::from_rgb(118, 185, 0), "CPU shouldn't look like CUDA");
        assert_ne!(cpu, Color32::from_rgb(0, 113, 197), "CPU shouldn't look like Intel");
    }
}

#[cfg(test)]
mod memory_bar_tests {
    use super::*;

    #[test]
    fn green_tier_for_usage_at_or_below_70_percent() {
        // Plenty of headroom — green.
        assert_eq!(memory_bar_color(0.0),  theme::SUCCESS);
        assert_eq!(memory_bar_color(50.0), theme::SUCCESS);
        // Boundary: 70.0 exact stays green (strict > 70 trips amber).
        assert_eq!(memory_bar_color(70.0), theme::SUCCESS);
    }

    #[test]
    fn amber_tier_for_usage_in_70_to_90_percent_band() {
        // Just past green threshold.
        assert_eq!(memory_bar_color(70.1), theme::WARNING);
        assert_eq!(memory_bar_color(85.0), theme::WARNING);
        // Boundary: 90.0 exact stays amber (strict > 90 trips red).
        assert_eq!(memory_bar_color(90.0), theme::WARNING);
    }

    #[test]
    fn red_tier_for_usage_above_90_percent() {
        // Imminent OOM territory — red.
        assert_eq!(memory_bar_color(90.1),  theme::ERROR);
        assert_eq!(memory_bar_color(99.0),  theme::ERROR);
        assert_eq!(memory_bar_color(100.0), theme::ERROR);
        // Above-100 (e.g. NVML returned stale stats) still red, not panic.
        assert_eq!(memory_bar_color(150.0), theme::ERROR);
    }

    #[test]
    fn tier_thresholds_are_distinct_colors() {
        // A regression that collapses two tiers to the same colour
        // would defeat the at-a-glance signal of the memory bar.
        assert_ne!(memory_bar_color(50.0), memory_bar_color(80.0));
        assert_ne!(memory_bar_color(80.0), memory_bar_color(95.0));
        assert_ne!(memory_bar_color(50.0), memory_bar_color(95.0));
    }
}
