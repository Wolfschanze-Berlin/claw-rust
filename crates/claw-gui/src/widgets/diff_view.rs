//! Config change diff preview widget.
//!
//! Provides line-level diffing using a longest common subsequence (LCS)
//! algorithm, rendered with colored backgrounds in a scroll area.

use eframe::egui;

use crate::theme;

/// A single diff line classified by change type.
#[derive(Debug)]
pub enum DiffLine {
    /// Line exists only in the new version.
    Added(String),
    /// Line exists only in the old version.
    Removed(String),
    /// Line is identical in both versions.
    Context(String),
}

/// Compute a line-level diff between two strings using LCS.
///
/// Returns a sequence of [`DiffLine`] values representing additions,
/// removals, and unchanged context lines.
pub fn compute_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let m = old_lines.len();
    let n = new_lines.len();

    // Build LCS table
    let mut dp = vec![vec![0u32; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if old_lines[i - 1] == new_lines[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack to build diff
    let mut result = Vec::new();
    let mut i = m;
    let mut j = n;

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old_lines[i - 1] == new_lines[j - 1] {
            result.push(DiffLine::Context(old_lines[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            result.push(DiffLine::Added(new_lines[j - 1].to_string()));
            j -= 1;
        } else if i > 0 {
            result.push(DiffLine::Removed(old_lines[i - 1].to_string()));
            i -= 1;
        }
    }

    result.reverse();
    result
}

/// Render a diff view showing changes between old and new text.
///
/// Added lines get a green background with `+` prefix, removed lines
/// get red with `-`, and context lines are dimmed with a space prefix.
/// Large diffs are wrapped in a [`egui::ScrollArea`].
pub fn diff_view(ui: &mut egui::Ui, old_json: &str, new_json: &str) {
    if old_json == new_json {
        ui.label(
            egui::RichText::new("No changes")
                .color(theme::OVERLAY)
                .italics(),
        );
        return;
    }

    let lines = compute_diff(old_json, new_json);

    egui::ScrollArea::vertical()
        .max_height(400.0)
        .show(ui, |ui| {
            ui.style_mut().spacing.item_spacing.y = 0.0;

            for line in &lines {
                match line {
                    DiffLine::Added(text) => {
                        let bg = egui::Color32::from_rgba_premultiplied(
                            theme::GREEN.r() / 5,
                            theme::GREEN.g() / 5,
                            theme::GREEN.b() / 5,
                            40,
                        );
                        egui::Frame::default()
                            .fill(bg)
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(format!("+ {text}"))
                                        .color(theme::GREEN)
                                        .monospace(),
                                );
                            });
                    }
                    DiffLine::Removed(text) => {
                        let bg = egui::Color32::from_rgba_premultiplied(
                            theme::RED.r() / 5,
                            theme::RED.g() / 5,
                            theme::RED.b() / 5,
                            40,
                        );
                        egui::Frame::default()
                            .fill(bg)
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(format!("- {text}"))
                                        .color(theme::RED)
                                        .monospace(),
                                );
                            });
                    }
                    DiffLine::Context(text) => {
                        ui.label(
                            egui::RichText::new(format!("  {text}"))
                                .color(theme::OVERLAY)
                                .monospace(),
                        );
                    }
                }
            }
        });
}

/// Convenience: diff two [`claw_config::OpenClawConfig`] values by
/// serializing to pretty JSON first.
pub fn config_diff_view(
    ui: &mut egui::Ui,
    old: &claw_config::OpenClawConfig,
    new: &claw_config::OpenClawConfig,
) {
    let old_json = serde_json::to_string_pretty(old).unwrap_or_default();
    let new_json = serde_json::to_string_pretty(new).unwrap_or_default();
    diff_view(ui, &old_json, &new_json);
}
