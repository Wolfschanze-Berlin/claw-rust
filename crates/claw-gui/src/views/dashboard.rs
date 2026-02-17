//! Gateway Status dashboard view.

use eframe::egui;

use crate::state::GatewayStatus;
use crate::theme;

/// Render the gateway status dashboard.
pub fn show(ui: &mut egui::Ui, gateway: &GatewayStatus) {
    ui.heading("Dashboard");
    ui.add_space(16.0);

    // Status card
    egui::Frame::default()
        .fill(theme::SURFACE0)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (color, label) = if gateway.running {
                    (theme::GREEN, "Running")
                } else {
                    (theme::RED, "Stopped")
                };

                // Colored circle indicator
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 6.0, color);

                ui.label(egui::RichText::new(label).size(18.0).color(color));
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);

            egui::Grid::new("gateway_stats")
                .spacing([40.0, 8.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Uptime").color(theme::SUBTEXT));
                    let secs = gateway.uptime.as_secs();
                    let hours = secs / 3600;
                    let mins = (secs % 3600) / 60;
                    ui.label(
                        egui::RichText::new(format!("{}h {}m", hours, mins)).color(theme::TEXT),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Port").color(theme::SUBTEXT));
                    ui.label(egui::RichText::new(gateway.port.to_string()).color(theme::TEXT));
                    ui.end_row();

                    ui.label(egui::RichText::new("Clients").color(theme::SUBTEXT));
                    ui.label(
                        egui::RichText::new(gateway.connected_clients.to_string())
                            .color(theme::TEXT),
                    );
                    ui.end_row();
                });
        });
}
