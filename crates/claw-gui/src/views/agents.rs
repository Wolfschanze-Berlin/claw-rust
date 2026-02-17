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

/// Agent creation mode for the wizard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentCreateMode {
    /// Create a blank agent with just ID and name.
    Blank,
    /// Clone an existing agent's settings.
    Clone,
}

/// UI state that persists across frames for the agents list.
pub struct AgentsViewState {
    /// Index of the currently selected agent (if any).
    pub selected: Option<usize>,
    /// Text buffer for the new agent ID input.
    pub new_agent_id: String,
    /// Text buffer for the new agent name input.
    pub new_agent_name: String,
    /// Whether the creation section is expanded.
    pub create_section_open: bool,
    /// Current creation mode (blank or clone).
    pub create_mode: AgentCreateMode,
    /// Source agent index to clone from (when create_mode == Clone).
    pub clone_source: Option<usize>,
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
            create_section_open: false,
            create_mode: AgentCreateMode::Blank,
            clone_source: None,
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
    // Collapsible creation section
    let toggle_label = if state.create_section_open {
        "\u{25bc} New Agent"
    } else {
        "\u{25b6} New Agent"
    };
    if ui
        .add(egui::Button::new(
            egui::RichText::new(toggle_label).color(theme::BLUE),
        ))
        .clicked()
    {
        state.create_section_open = !state.create_section_open;
    }

    if !state.create_section_open {
        return;
    }

    egui::Frame::default()
        .fill(theme::SURFACE0)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            // Mode selector
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Mode:").color(theme::SUBTEXT));
                if ui
                    .selectable_label(
                        state.create_mode == AgentCreateMode::Blank,
                        "Blank",
                    )
                    .clicked()
                {
                    state.create_mode = AgentCreateMode::Blank;
                    state.clone_source = None;
                }
                if ui
                    .selectable_label(
                        state.create_mode == AgentCreateMode::Clone,
                        "Clone Existing",
                    )
                    .clicked()
                {
                    state.create_mode = AgentCreateMode::Clone;
                }
            });

            ui.add_space(4.0);

            // Clone source picker (only shown in Clone mode)
            if state.create_mode == AgentCreateMode::Clone {
                let agents_list = config
                    .draft()
                    .agents
                    .as_ref()
                    .and_then(|a| a.list.as_ref());

                if let Some(list) = agents_list {
                    if list.is_empty() {
                        ui.label(
                            egui::RichText::new("No agents to clone from.")
                                .color(theme::YELLOW)
                                .small(),
                        );
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Clone from:").color(theme::SUBTEXT),
                            );

                            let selected_label = state
                                .clone_source
                                .and_then(|idx| list.get(idx))
                                .and_then(|a| a.id.as_deref())
                                .unwrap_or("Select agent...");

                            egui::ComboBox::from_id_salt("clone_source")
                                .selected_text(selected_label)
                                .show_ui(ui, |ui| {
                                    for (idx, agent) in list.iter().enumerate() {
                                        let label = agent
                                            .id
                                            .as_deref()
                                            .unwrap_or("<no id>");
                                        if ui
                                            .selectable_value(
                                                &mut state.clone_source,
                                                Some(idx),
                                                label,
                                            )
                                            .changed()
                                        {
                                            // Auto-fill name as "Copy of <source>"
                                            if state.new_agent_name.is_empty() {
                                                let source_name = agent
                                                    .name
                                                    .as_deref()
                                                    .or(agent.id.as_deref())
                                                    .unwrap_or("agent");
                                                state.new_agent_name =
                                                    format!("Copy of {source_name}");
                                            }
                                        }
                                    }
                                });
                        });
                    }
                } else {
                    ui.label(
                        egui::RichText::new("No agents to clone from.")
                            .color(theme::YELLOW)
                            .small(),
                    );
                }

                ui.add_space(4.0);
            }

            // ID and Name inputs
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("ID:").color(theme::SUBTEXT));
                ui.add(
                    egui::TextEdit::singleline(&mut state.new_agent_id).desired_width(120.0),
                );

                ui.label(egui::RichText::new("Name:").color(theme::SUBTEXT));
                ui.add(
                    egui::TextEdit::singleline(&mut state.new_agent_name).desired_width(160.0),
                );
            });

            ui.add_space(4.0);

            // Create button
            let id = state.new_agent_id.trim().to_string();
            let existing_ids: Vec<String> = config
                .draft()
                .agents
                .as_ref()
                .and_then(|a| a.list.as_ref())
                .map(|list| {
                    list.iter()
                        .filter_map(|e| e.id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let duplicate = existing_ids.iter().any(|x| x == &id);
            let valid = !id.is_empty()
                && !duplicate
                && (state.create_mode == AgentCreateMode::Blank
                    || state.clone_source.is_some());

            ui.horizontal(|ui| {
                let button_text = match state.create_mode {
                    AgentCreateMode::Blank => "+ Create Agent",
                    AgentCreateMode::Clone => "+ Clone Agent",
                };
                if ui
                    .add_enabled(
                        valid,
                        egui::Button::new(egui::RichText::new(button_text).color(theme::GREEN)),
                    )
                    .clicked()
                {
                    config.begin_edit();
                    let agents =
                        config.draft_mut().agents.get_or_insert_with(AgentsConfig::default);
                    let list = agents.list.get_or_insert_with(Vec::new);

                    let mut new_agent = match state.create_mode {
                        AgentCreateMode::Clone => {
                            // Clone from source agent
                            state
                                .clone_source
                                .and_then(|idx| list.get(idx).cloned())
                                .unwrap_or_default()
                        }
                        AgentCreateMode::Blank => AgentEntry::default(),
                    };

                    // Override ID and name
                    new_agent.id = Some(id.to_string());
                    new_agent.name = if state.new_agent_name.trim().is_empty() {
                        None
                    } else {
                        Some(state.new_agent_name.trim().to_string())
                    };
                    // Cloned agent should not inherit the "default" flag
                    if state.create_mode == AgentCreateMode::Clone {
                        new_agent.default = None;
                    }

                    list.push(new_agent);
                    state.new_agent_id.clear();
                    state.new_agent_name.clear();
                    state.clone_source = None;
                    state.create_section_open = false;
                }

                if duplicate && !id.is_empty() {
                    ui.label(
                        egui::RichText::new("ID already exists")
                            .color(theme::YELLOW)
                            .small(),
                    );
                }
            });
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
