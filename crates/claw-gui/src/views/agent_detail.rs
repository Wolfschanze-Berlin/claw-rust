//! Agent detail/edit form with all AgentEntry fields.
//!
//! Renders when an agent is selected from the agents list view.
//! Follows the same pattern as [`channel_detail`](super::channel_detail).

use eframe::egui;
use std::collections::HashMap;

use claw_config::AgentEntry;

use crate::theme;
use crate::widgets::{form_field, json_view};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Actions the detail form can request from its parent view.
pub enum AgentAction {
    /// No action needed.
    None,
    /// User clicked the back button — return to agent list.
    Back,
    /// User confirmed agent deletion.
    Delete,
}

/// Persistent UI state for the agent detail form.
pub struct AgentDetailState {
    /// Index of the agent being edited.
    _agent_index: usize,
    /// JSON edit buffer for agent extra fields.
    pub extra_json: String,
    /// Whether the extra JSON editor is open.
    pub extra_editing: bool,
    /// Whether to show the "delete agent" confirmation dialog.
    pub confirm_delete: bool,
}

impl AgentDetailState {
    /// Create fresh UI state for editing the agent at the given index.
    pub fn new(index: usize) -> Self {
        Self {
            _agent_index: index,
            extra_json: String::new(),
            extra_editing: false,
            confirm_delete: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Render the full agent detail form.
///
/// Returns an [`AgentAction`] indicating what the parent should do next.
pub fn show(
    ui: &mut egui::Ui,
    agent: &mut AgentEntry,
    state: &mut AgentDetailState,
) -> AgentAction {
    let mut action = AgentAction::None;

    let display_id = agent.id.as_deref().unwrap_or("<no id>");

    // -- Header row: back button, title, delete button -----------------------
    ui.horizontal(|ui| {
        if ui.button("\u{2190} Back").clicked() {
            action = AgentAction::Back;
        }
        ui.heading(
            egui::RichText::new(format!("Agent: {display_id}"))
                .color(theme::TEXT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if state.confirm_delete {
                ui.label(egui::RichText::new("Are you sure?").color(theme::YELLOW));
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("Yes, delete").color(theme::RED),
                    ))
                    .clicked()
                {
                    action = AgentAction::Delete;
                }
                if ui.button("Cancel").clicked() {
                    state.confirm_delete = false;
                }
            } else if ui
                .add(egui::Button::new(
                    egui::RichText::new("Delete Agent").color(theme::RED),
                ))
                .clicked()
            {
                state.confirm_delete = true;
            }
        });
    });

    ui.separator();

    // -- Scrollable form body ------------------------------------------------
    egui::ScrollArea::vertical().show(ui, |ui| {
        // ---- Identity ----
        egui::CollapsingHeader::new(
            egui::RichText::new("Identity").color(theme::TEXT).strong(),
        )
        .id_salt("agent_identity")
        .default_open(true)
        .show(ui, |ui| {
            form_field::text_field(ui, "ID", &mut agent.id);
            form_field::text_field(ui, "Name", &mut agent.name);
            form_field::bool_field(ui, "Default agent", &mut agent.default);
        });

        // ---- Model & Workspace ----
        egui::CollapsingHeader::new(
            egui::RichText::new("Model & Workspace").color(theme::TEXT).strong(),
        )
        .id_salt("agent_model")
        .default_open(true)
        .show(ui, |ui| {
            form_field::text_field(ui, "Model", &mut agent.model);
            form_field::text_field(ui, "Workspace", &mut agent.workspace);
            form_field::text_field(ui, "Agent directory", &mut agent.agent_dir);
        });

        // ---- Subagents ----
        egui::CollapsingHeader::new(
            egui::RichText::new("Subagents").color(theme::TEXT).strong(),
        )
        .id_salt("agent_subagents")
        .default_open(false)
        .show(ui, |ui| {
            let subagents = agent.subagents.get_or_insert_with(Default::default);
            form_field::string_list_field(ui, "Allowed agents", &mut subagents.allow_agents);
        });

        // ---- Extra Fields ----
        egui::CollapsingHeader::new(
            egui::RichText::new("Extra Fields").color(theme::TEXT).strong(),
        )
        .id_salt("agent_extra")
        .default_open(false)
        .show(ui, |ui| {
            let mut extra_value = serde_json::to_value(&agent.extra).unwrap_or_default();
            if json_view::json_field(
                ui,
                "Agent Extra",
                &mut extra_value,
                &mut state.extra_json,
                &mut state.extra_editing,
            ) {
                if let Ok(map) =
                    serde_json::from_value::<HashMap<String, serde_json::Value>>(extra_value)
                {
                    agent.extra = map;
                }
            }
        });
    });

    action
}
