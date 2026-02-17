//! Draft commit workflow: validate → diff → confirm → write.
//!
//! Provides the save/discard toolbar with dirty indicator and a
//! confirmation dialog showing validation results and a diff preview
//! before persisting changes to disk.

use eframe::egui;

use claw_config::ValidationResult;

use crate::config_manager::ConfigManager;
use crate::theme;
use crate::widgets::diff_view;
use crate::widgets::validation_badge;

/// State of the commit confirmation flow.
#[derive(Debug, Clone, PartialEq)]
pub enum CommitPhase {
    /// No commit in progress — normal editing.
    Idle,
    /// User clicked Save — showing diff + validation for confirmation.
    Confirming,
    /// Commit succeeded — showing success feedback briefly.
    Success,
    /// Commit failed — showing error message.
    Error(String),
}

/// Persistent UI state for the commit workflow.
pub struct CommitWorkflowState {
    pub phase: CommitPhase,
    /// Countdown frames for auto-dismissing success/error banners.
    pub feedback_frames: u32,
}

impl Default for CommitWorkflowState {
    fn default() -> Self {
        Self {
            phase: CommitPhase::Idle,
            feedback_frames: 0,
        }
    }
}

/// Feedback banner duration in frames (~60fps → ~3 seconds).
const FEEDBACK_DURATION: u32 = 180;

/// Render the commit workflow toolbar and optional confirmation dialog.
///
/// This should be rendered above or below the config editor. It shows:
/// - Dirty indicator (dot + "Unsaved changes")
/// - Undo/Redo buttons
/// - Save / Discard buttons
/// - Validation errors blocking save
/// - Diff preview in confirmation mode
///
/// Returns `true` if a commit or discard occurred (caller should refresh).
pub fn show(
    ui: &mut egui::Ui,
    manager: &mut ConfigManager,
    state: &mut CommitWorkflowState,
    validation: &ValidationResult,
) -> bool {
    let mut action_taken = false;

    // Auto-dismiss feedback banners
    if matches!(state.phase, CommitPhase::Success | CommitPhase::Error(_)) {
        state.feedback_frames = state.feedback_frames.saturating_sub(1);
        if state.feedback_frames == 0 {
            state.phase = CommitPhase::Idle;
        }
        ui.ctx().request_repaint();
    }

    // -- Toolbar --
    egui::Frame::default()
        .fill(theme::SURFACE0)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Dirty indicator
                let dirty = manager.is_dirty();
                if dirty {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(rect.center(), 5.0, theme::YELLOW);
                    ui.label(
                        egui::RichText::new("Unsaved changes").color(theme::YELLOW),
                    );
                } else {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(rect.center(), 5.0, theme::GREEN);
                    ui.label(
                        egui::RichText::new("Saved").color(theme::GREEN),
                    );
                }

                ui.add_space(16.0);

                // Undo / Redo
                let undo_btn = ui.add_enabled(
                    manager.can_undo(),
                    egui::Button::new("Undo"),
                );
                if undo_btn.clicked() {
                    manager.undo();
                    action_taken = true;
                }

                let redo_btn = ui.add_enabled(
                    manager.can_redo(),
                    egui::Button::new("Redo"),
                );
                if redo_btn.clicked() {
                    manager.redo();
                    action_taken = true;
                }

                ui.add_space(16.0);

                // Save button — disabled if not dirty or has validation errors
                let has_errors = validation.has_errors();
                let can_save = dirty && !has_errors;

                let save_btn = ui.add_enabled(
                    can_save,
                    egui::Button::new(
                        egui::RichText::new("Save").color(if can_save {
                            theme::GREEN
                        } else {
                            theme::OVERLAY
                        }),
                    ),
                );
                if save_btn.clicked() {
                    state.phase = CommitPhase::Confirming;
                }
                if has_errors && dirty {
                    save_btn.on_hover_text("Fix validation errors before saving");
                }

                // Discard button
                let discard_btn = ui.add_enabled(
                    dirty,
                    egui::Button::new(
                        egui::RichText::new("Discard").color(if dirty {
                            theme::RED
                        } else {
                            theme::OVERLAY
                        }),
                    ),
                );
                if discard_btn.clicked() {
                    manager.discard();
                    state.phase = CommitPhase::Idle;
                    action_taken = true;
                }

                // Feedback banners (inline)
                match &state.phase {
                    CommitPhase::Success => {
                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new("Config saved successfully")
                                .color(theme::GREEN),
                        );
                    }
                    CommitPhase::Error(msg) => {
                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new(format!("Save failed: {msg}"))
                                .color(theme::RED),
                        );
                    }
                    _ => {}
                }
            });
        });

    // -- Confirmation dialog --
    if state.phase == CommitPhase::Confirming {
        ui.add_space(8.0);

        egui::Frame::default()
            .fill(theme::MANTLE)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(12))
            .stroke(egui::Stroke::new(1.0, theme::BLUE))
            .show(ui, |ui| {
                ui.heading("Confirm save");
                ui.add_space(4.0);

                // Validation summary
                validation_badge::validation_summary_bar(ui, validation);
                ui.add_space(8.0);

                // Diff preview
                ui.label(
                    egui::RichText::new("Changes to be written:")
                        .color(theme::SUBTEXT),
                );
                ui.add_space(4.0);
                diff_view::config_diff_view(ui, manager.live(), manager.draft());
                ui.add_space(8.0);

                // Confirm / Cancel buttons
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new("Confirm & Write").color(theme::GREEN),
                        ))
                        .clicked()
                    {
                        match manager.commit() {
                            Ok(()) => {
                                state.phase = CommitPhase::Success;
                                state.feedback_frames = FEEDBACK_DURATION;
                                action_taken = true;
                            }
                            Err(e) => {
                                state.phase = CommitPhase::Error(e.to_string());
                                state.feedback_frames = FEEDBACK_DURATION;
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        state.phase = CommitPhase::Idle;
                    }
                });
            });
    }

    action_taken
}
