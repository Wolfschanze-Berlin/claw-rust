//! Agents CRUD list view backed by ConfigManager draft.
//!
//! Shows all configured agents with create, delete, and select
//! actions. Selecting an agent navigates to the detail form
//! (see [`agent_detail`](super::agent_detail)).

use eframe::egui;

use claw_config::{AgentEntry, AgentsConfig};

use crate::config_manager::ConfigManager;
use crate::theme;
use crate::views::agent_detail::{AgentAction, AgentDetailState};

// ---------------------------------------------------------------------------
// Persistent UI state for the agents view
// ---------------------------------------------------------------------------

/// UI state that persists across frames for the agents list.
pub struct AgentsViewState {
    /// Index of the currently selected agent (if any).
    pub selected: Option<usize>,
    /// Text buffer for the new agent ID input.
    pub new_agent_id: String,
    /// Text buffer for the new agent name input.
    pub new_agent_name: String,
    /// Detail form state for the selected agent.
    pub detail_state: Option<AgentDetailState>,
    /// Whether we've already taken an undo snapshot for this detail session.
    detail_snapshot_taken: bool,
}

impl Default for AgentsViewState {
    fn default() -> Self {
        Self {
            selected: None,
            new_agent_id: String::new(),
            new_agent_name: String::new(),
            detail_state: None,
            detail_snapshot_taken: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Render the agents view (list or detail, depending on selection).
pub fn show(ui: &mut egui::Ui, config: &mut ConfigManager, state: &mut AgentsViewState) {
    if state.selected.is_some() {
        show_detail(ui, config, state);
    } else {
        show_list(ui, config, state);
    }
}

// ---------------------------------------------------------------------------
// List view
// ---------------------------------------------------------------------------

fn show_list(ui: &mut egui::Ui, config: &mut ConfigManager, state: &mut AgentsViewState) {
    // Header
    ui.horizontal(|ui| {
        ui.heading("Agents");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let count = config
                .draft()
                .agents
                .as_ref()
                .and_then(|a| a.list.as_ref())
                .map_or(0, |l| l.len());
            ui.label(
                egui::RichText::new(format!("{count} configured"))
                    .color(theme::SUBTEXT)
                    .small(),
            );
        });
    });
    ui.add_space(8.0);

    // Add agent row
    show_add_form(ui, config, state);

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Agent cards
    show_agent_cards(ui, config, state);
}

// ---------------------------------------------------------------------------
// Add agent form
// ---------------------------------------------------------------------------

fn show_add_form(ui: &mut egui::Ui, config: &mut ConfigManager, state: &mut AgentsViewState) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("ID:").color(theme::SUBTEXT));
        ui.add(egui::TextEdit::singleline(&mut state.new_agent_id).desired_width(120.0));

        ui.label(egui::RichText::new("Name:").color(theme::SUBTEXT));
        ui.add(egui::TextEdit::singleline(&mut state.new_agent_name).desired_width(160.0));

        let id = state.new_agent_id.trim();
        let existing_ids: Vec<&str> = config
            .draft()
            .agents
            .as_ref()
            .and_then(|a| a.list.as_ref())
            .map(|list| {
                list.iter()
                    .filter_map(|e| e.id.as_deref())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let duplicate = existing_ids.contains(&id);
        let valid = !id.is_empty() && !duplicate;

        if ui
            .add_enabled(
                valid,
                egui::Button::new(egui::RichText::new("+ Add").color(theme::GREEN)),
            )
            .clicked()
        {
            config.begin_edit();
            let agents = config.draft_mut().agents.get_or_insert_with(AgentsConfig::default);
            let list = agents.list.get_or_insert_with(Vec::new);
            list.push(AgentEntry {
                id: Some(id.to_string()),
                name: if state.new_agent_name.trim().is_empty() {
                    None
                } else {
                    Some(state.new_agent_name.trim().to_string())
                },
                ..Default::default()
            });
            state.new_agent_id.clear();
            state.new_agent_name.clear();
        }
    });
}

// ---------------------------------------------------------------------------
// Agent cards
// ---------------------------------------------------------------------------

fn show_agent_cards(ui: &mut egui::Ui, config: &mut ConfigManager, state: &mut AgentsViewState) {
    let agents_list = config
        .draft()
        .agents
        .as_ref()
        .and_then(|a| a.list.clone());

    let Some(agents) = agents_list.as_ref() else {
        ui.label(
            egui::RichText::new("No agents configured. Add one above.")
                .color(theme::OVERLAY)
                .italics(),
        );
        return;
    };

    if agents.is_empty() {
        ui.label(
            egui::RichText::new("No agents configured. Add one above.")
                .color(theme::OVERLAY)
                .italics(),
        );
        return;
    }

    let mut to_delete: Option<usize> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (idx, agent) in agents.iter().enumerate() {
            let id = agent.id.as_deref().unwrap_or("<no id>");
            let name = agent.name.as_deref().unwrap_or("");
            let model = agent.model.as_deref().unwrap_or("default");
            let is_default = agent.default.unwrap_or(false);

            egui::Frame::default()
                .fill(theme::SURFACE0)
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Default agent indicator
                        let color = if is_default { theme::BLUE } else { theme::OVERLAY };
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 5.0, color);

                        // Agent info (clickable to select)
                        let response = ui.vertical(|ui| {
                            let label = if name.is_empty() {
                                id.to_string()
                            } else {
                                format!("{name} ({id})")
                            };
                            ui.label(
                                egui::RichText::new(label)
                                    .size(16.0)
                                    .color(theme::TEXT),
                            );
                            let mut subtitle_parts = vec![format!("model: {model}")];
                            if is_default {
                                subtitle_parts.push("default".to_string());
                            }
                            ui.label(
                                egui::RichText::new(subtitle_parts.join(" \u{00b7} "))
                                    .small()
                                    .color(theme::SUBTEXT),
                            );
                        });

                        if response.response.interact(egui::Sense::click()).clicked() {
                            state.selected = Some(idx);
                            state.detail_state = Some(AgentDetailState::new(idx));
                        }

                        // Right side: edit + delete buttons
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("Delete").color(theme::RED),
                                    )
                                    .small(),
                                )
                                .clicked()
                            {
                                to_delete = Some(idx);
                            }

                            if ui.small_button("Edit").clicked() {
                                state.selected = Some(idx);
                                state.detail_state = Some(AgentDetailState::new(idx));
                            }
                        });
                    });
                });

            ui.add_space(4.0);
        }
    });

    // Apply deferred deletion
    if let Some(idx) = to_delete {
        config.begin_edit();
        if let Some(agents) = config
            .draft_mut()
            .agents
            .as_mut()
            .and_then(|a| a.list.as_mut())
        {
            if idx < agents.len() {
                agents.remove(idx);
                if state.selected == Some(idx) {
                    state.selected = None;
                    state.detail_state = None;
                } else if let Some(sel) = state.selected {
                    if sel > idx {
                        state.selected = Some(sel - 1);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Detail view (delegates to agent_detail module)
// ---------------------------------------------------------------------------

fn show_detail(ui: &mut egui::Ui, config: &mut ConfigManager, state: &mut AgentsViewState) {
    let Some(idx) = state.selected else {
        state.detail_state = None;
        return;
    };

    // Ensure detail state exists
    if state.detail_state.is_none() {
        state.detail_state = Some(AgentDetailState::new(idx));
    }

    let detail_state = state.detail_state.as_mut().unwrap();

    // Check agent still exists at this index
    let agent_count = config
        .draft()
        .agents
        .as_ref()
        .and_then(|a| a.list.as_ref())
        .map_or(0, |l| l.len());

    if idx >= agent_count {
        state.selected = None;
        state.detail_state = None;
        return;
    }

    // Snapshot once when first entering detail view
    if !state.detail_snapshot_taken {
        config.begin_edit();
        state.detail_snapshot_taken = true;
    }

    let agent = config
        .draft_mut()
        .agents
        .as_mut()
        .unwrap()
        .list
        .as_mut()
        .unwrap()
        .get_mut(idx)
        .unwrap();

    let action = super::agent_detail::show(ui, agent, detail_state);

    match action {
        AgentAction::Back => {
            state.selected = None;
            state.detail_state = None;
            state.detail_snapshot_taken = false;
        }
        AgentAction::Delete => {
            if let Some(list) = config
                .draft_mut()
                .agents
                .as_mut()
                .and_then(|a| a.list.as_mut())
            {
                if idx < list.len() {
                    list.remove(idx);
                }
            }
            state.selected = None;
            state.detail_state = None;
            state.detail_snapshot_taken = false;
        }
        AgentAction::None => {}
    }
}
