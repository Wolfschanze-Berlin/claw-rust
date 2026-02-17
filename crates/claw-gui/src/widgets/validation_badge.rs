//! Inline validation error/warning display widgets.
//!
//! Renders colored indicators next to form fields and a summary bar
//! showing aggregate validation health.

use eframe::egui;

use claw_config::{IssueSeverity, ValidationResult};

use crate::theme;

/// Show validation badges next to a field if there are matching issues.
///
/// `field_path` is the dot-notation path (e.g. `"gateway.port"`).
/// Matching issues render as colored dots with hover tooltips.
pub fn validation_badge(ui: &mut egui::Ui, field_path: &str, result: &ValidationResult) {
    let matching: Vec<_> = result
        .issues
        .iter()
        .filter(|issue| issue.path == field_path)
        .collect();

    for issue in matching {
        let color = match issue.severity {
            IssueSeverity::Error => theme::RED,
            IssueSeverity::Warning => theme::YELLOW,
            IssueSeverity::Info => theme::BLUE,
        };

        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 5.0, color);

        response.on_hover_text(
            egui::RichText::new(&issue.message).color(color),
        );
    }
}

/// Show a summary bar with total error/warning counts.
///
/// Displays "N errors, M warnings" with themed colors, or a green
/// "No issues" message when the config is clean.
pub fn validation_summary_bar(ui: &mut egui::Ui, result: &ValidationResult) {
    let error_count = result.errors().len();
    let warning_count = result.warnings().len();

    egui::Frame::default()
        .fill(theme::SURFACE0)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if error_count == 0 && warning_count == 0 {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(rect.center(), 5.0, theme::GREEN);
                    ui.label(egui::RichText::new("No issues").color(theme::GREEN));
                } else {
                    if error_count > 0 {
                        let (rect, _) = ui
                            .allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter()
                            .circle_filled(rect.center(), 5.0, theme::RED);
                        ui.label(
                            egui::RichText::new(format!(
                                "{} error{}",
                                error_count,
                                if error_count == 1 { "" } else { "s" }
                            ))
                            .color(theme::RED),
                        );
                    }

                    if warning_count > 0 {
                        if error_count > 0 {
                            ui.label(
                                egui::RichText::new(",").color(theme::SUBTEXT),
                            );
                        }
                        let (rect, _) = ui
                            .allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter()
                            .circle_filled(rect.center(), 5.0, theme::YELLOW);
                        ui.label(
                            egui::RichText::new(format!(
                                "{} warning{}",
                                warning_count,
                                if warning_count == 1 { "" } else { "s" }
                            ))
                            .color(theme::YELLOW),
                        );
                    }
                }
            });
        });
}
