//! Channels view with state-machine toggles and error feedback.

use eframe::egui;

use crate::commands::{Command, CommandSender};
use crate::state::{ChannelInfo, ChannelStatus};
use crate::theme;

/// Render the channels view.
pub fn show(ui: &mut egui::Ui, channels: &[ChannelInfo], cmd: &CommandSender) {
    ui.heading("Channels");
    ui.add_space(16.0);

    for channel in channels {
        egui::Frame::default()
            .fill(theme::SURFACE0)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Status dot
                    let status_color = match &channel.status {
                        ChannelStatus::Running => theme::GREEN,
                        ChannelStatus::Starting | ChannelStatus::Stopping => theme::YELLOW,
                        ChannelStatus::Error(_) => theme::RED,
                        ChannelStatus::Stopped => theme::OVERLAY,
                    };

                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 5.0, status_color);

                    // Channel info
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(&channel.name)
                                .size(16.0)
                                .color(theme::TEXT),
                        );
                        ui.label(
                            egui::RichText::new(&channel.channel_type)
                                .small()
                                .color(theme::SUBTEXT),
                        );
                    });

                    // Right-aligned controls
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let status_text = match &channel.status {
                            ChannelStatus::Stopped => "Stopped",
                            ChannelStatus::Starting => "Starting...",
                            ChannelStatus::Running => "Running",
                            ChannelStatus::Stopping => "Stopping...",
                            ChannelStatus::Error(_) => "Error",
                        };
                        ui.label(egui::RichText::new(status_text).color(status_color));

                        // Start/Stop button — disabled during transitions
                        let is_transitioning = matches!(
                            channel.status,
                            ChannelStatus::Starting | ChannelStatus::Stopping
                        );

                        let button_text = match &channel.status {
                            ChannelStatus::Stopped | ChannelStatus::Error(_) => "Start",
                            ChannelStatus::Running => "Stop",
                            ChannelStatus::Starting => "Starting...",
                            ChannelStatus::Stopping => "Stopping...",
                        };

                        if ui
                            .add_enabled(!is_transitioning, egui::Button::new(button_text))
                            .clicked()
                        {
                            match &channel.status {
                                ChannelStatus::Stopped | ChannelStatus::Error(_) => {
                                    cmd.send(Command::StartChannel(channel.name.clone()));
                                }
                                ChannelStatus::Running => {
                                    cmd.send(Command::StopChannel(channel.name.clone()));
                                }
                                _ => {}
                            }
                        }
                    });
                });

                // Error banner
                if let ChannelStatus::Error(msg) = &channel.status {
                    ui.add_space(8.0);
                    egui::Frame::default()
                        .fill(egui::Color32::from_rgba_premultiplied(60, 20, 25, 255))
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("⚠ {}", msg)).color(theme::RED),
                            );
                        });
                }
            });

        ui.add_space(8.0);
    }
}
