//! Live Logs view with auto-scroll and level-based coloring.

use eframe::egui;

use crate::logging::LogBuffer;
use crate::theme;

/// Render the logs view.
pub fn show(ui: &mut egui::Ui, log_buffer: &LogBuffer) {
    ui.horizontal(|ui| {
        ui.heading("Logs");
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Live Events").small().color(theme::SUBTEXT));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{} entries", log_buffer.len()))
                    .small()
                    .color(theme::OVERLAY),
            );
        });
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if log_buffer.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("No log events yet. Waiting for activity...")
                            .color(theme::SUBTEXT),
                    );
                });
            } else {
                for entry in log_buffer.entries() {
                    ui.horizontal(|ui| {
                        // Timestamp
                        let time_str = entry.timestamp.format("%H:%M:%S%.3f").to_string();
                        ui.label(
                            egui::RichText::new(time_str)
                                .small()
                                .monospace()
                                .color(theme::OVERLAY),
                        );

                        // Level with color
                        let (level_str, level_color) = match entry.level {
                            tracing::Level::ERROR => ("ERR", theme::RED),
                            tracing::Level::WARN => ("WRN", theme::YELLOW),
                            tracing::Level::INFO => ("INF", theme::TEXT),
                            tracing::Level::DEBUG => ("DBG", theme::SUBTEXT),
                            tracing::Level::TRACE => ("TRC", theme::OVERLAY),
                        };
                        ui.label(
                            egui::RichText::new(level_str)
                                .small()
                                .monospace()
                                .color(level_color),
                        );

                        // Message
                        ui.label(
                            egui::RichText::new(&entry.message)
                                .monospace()
                                .color(level_color),
                        );
                    });
                }
            }
        });
}
