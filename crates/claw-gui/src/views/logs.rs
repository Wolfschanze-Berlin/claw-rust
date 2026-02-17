//! Live Logs view with auto-scroll, level-based coloring, and optional channel filter.

use eframe::egui;

use crate::logging::LogBuffer;
use crate::theme;

/// Render the logs view with an optional channel filter.
///
/// When `channel_filter` is `Some("telegram")`, only entries whose target or
/// message contains that string are shown. A filter bar at the top lets the
/// user clear the filter.
pub fn show(ui: &mut egui::Ui, log_buffer: &LogBuffer, channel_filter: &mut Option<String>) {
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

    // Channel filter bar
    if let Some(channel) = channel_filter.clone() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Filtered: {channel}"))
                    .color(theme::BLUE)
                    .strong(),
            );
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("\u{00d7} Clear").color(theme::RED))
                        .small(),
                )
                .on_hover_text("Show all logs")
                .clicked()
            {
                *channel_filter = None;
            }
        });
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let has_filter = channel_filter.is_some();
            let entries: Vec<_> = if let Some(needle) = channel_filter.as_deref() {
                log_buffer.entries_filtered(needle).collect()
            } else {
                log_buffer.entries().collect()
            };

            if entries.is_empty() {
                let msg = if has_filter {
                    "No log events matching this channel filter."
                } else {
                    "No log events yet. Waiting for activity..."
                };
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new(msg).color(theme::SUBTEXT));
                });
            } else {
                for entry in entries {
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
