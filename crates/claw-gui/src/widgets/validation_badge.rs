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
/// "No issues" message when the config is clean. When issues exist,
/// clicking the bar expands an inline list of all issues with their
/// field paths and messages so the user can identify exactly what's wrong.
pub fn validation_summary_bar(ui: &mut egui::Ui, result: &ValidationResult) {
    let error_count = result.errors().len();
    let warning_count = result.warnings().len();

    egui::Frame::default()
        .fill(theme::SURFACE0)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            if error_count == 0 && warning_count == 0 {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(rect.center(), 5.0, theme::GREEN);
                    ui.label(egui::RichText::new("No issues").color(theme::GREEN));
                });
            } else {
                // Collapsible header shows counts; body lists each issue
                egui::CollapsingHeader::new({
                    let mut job = egui::text::LayoutJob::default();
                    if error_count > 0 {
                        job.append(
                            &format!(
                                "{} error{}",
                                error_count,
                                if error_count == 1 { "" } else { "s" }
                            ),
                            0.0,
                            egui::TextFormat {
                                color: theme::RED,
                                ..Default::default()
                            },
                        );
                    }
                    if warning_count > 0 {
                        if error_count > 0 {
                            job.append(
                                ", ",
                                0.0,
                                egui::TextFormat {
                                    color: theme::SUBTEXT,
                                    ..Default::default()
                                },
                            );
                        }
                        job.append(
                            &format!(
                                "{} warning{}",
                                warning_count,
                                if warning_count == 1 { "" } else { "s" }
                            ),
                            0.0,
                            egui::TextFormat {
                                color: theme::YELLOW,
                                ..Default::default()
                            },
                        );
                    }
                    job
                })
                .id_salt("validation_summary_details")
                .default_open(false)
                .show(ui, |ui| {
                    for issue in &result.issues {
                        let color = match issue.severity {
                            IssueSeverity::Error => theme::RED,
                            IssueSeverity::Warning => theme::YELLOW,
                            IssueSeverity::Info => theme::BLUE,
                        };
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(8.0, 8.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().circle_filled(rect.center(), 4.0, color);
                            ui.label(
                                egui::RichText::new(&issue.path)
                                    .color(theme::BLUE)
                                    .small()
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(&issue.message).color(color).small(),
                            );
                        });
                    }
                });
            }
        });
}

/// Check whether a validation result has any issues whose path starts
/// with the given prefix. Useful for showing error indicators on tabs.
pub fn has_issues_for_prefix(result: &ValidationResult, prefix: &str) -> (usize, usize) {
    let mut errors = 0usize;
    let mut warnings = 0usize;
    for issue in &result.issues {
        if issue.path.starts_with(prefix) {
            match issue.severity {
                IssueSeverity::Error => errors += 1,
                IssueSeverity::Warning => warnings += 1,
                IssueSeverity::Info => {}
            }
        }
    }
    (errors, warnings)
}
