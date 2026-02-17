//! Agents CRUD list view backed by ConfigManager draft.
//!
//! Shows all configured agents with create, delete, and select
//! actions. Selecting an agent navigates to the detail form
//! (see [`agent_detail`](super::agent_detail)).
//!
//! Also discovers agent templates from the `agents/` directory.

use std::path::PathBuf;

use eframe::egui;

use claw_config::{AgentEntry, AgentsConfig};

use crate::config_manager::ConfigManager;
use crate::theme;
use crate::views::agent_detail::{AgentAction, AgentDetailState};

// ---------------------------------------------------------------------------
// Agent template discovery
// ---------------------------------------------------------------------------

/// An agent template discovered from the `agents/` directory.
#[derive(Debug, Clone)]
pub struct AgentTemplate {
    /// Directory name (e.g., "claw-agent").
    pub dir_name: String,
    /// Full path to the template directory.
    pub path: PathBuf,
    /// Whether this template has a `workspace/` subdirectory.
    pub has_workspace: bool,
    /// Whether this template has its own `config/config.json`.
    pub has_config: bool,
}

/// Scan the `agents/` directory for agent template subdirectories.
fn discover_templates() -> Vec<AgentTemplate> {
    let agents_dir = PathBuf::from("agents");
    let Ok(entries) = std::fs::read_dir(&agents_dir) else {
        return Vec::new();
    };

    let mut templates = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Skip hidden directories
        if dir_name.starts_with('.') {
            continue;
        }
        templates.push(AgentTemplate {
            dir_name: dir_name.to_string(),
            has_workspace: path.join("workspace").is_dir(),
            has_config: path.join("config").join("config.json").is_file(),
            path,
        });
    }
    templates.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));
    templates
}

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
    /// Create from an agent template in `agents/`.
    Template,
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
    /// Current creation mode (blank, clone, or template).
    pub create_mode: AgentCreateMode,
    /// Source agent index to clone from (when create_mode == Clone).
    pub clone_source: Option<usize>,
    /// Selected template index (when create_mode == Template).
    pub template_source: Option<usize>,
    /// Discovered agent templates from `agents/` directory.
    pub templates: Vec<AgentTemplate>,
    /// Whether templates have been loaded from disk.
    templates_loaded: bool,
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
            template_source: None,
            templates: Vec::new(),
            templates_loaded: false,
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
    // Lazy-load templates on first render
    if !state.templates_loaded {
        state.templates = discover_templates();
        state.templates_loaded = true;
    }

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
            let tmpl_count = state.templates.len();
            ui.label(
                egui::RichText::new(format!(
                    "{count} configured \u{00b7} {tmpl_count} template{}",
                    if tmpl_count == 1 { "" } else { "s" }
                ))
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

    // Agent cards (configured agents)
    show_agent_cards(ui, config, state);

    // Agent templates section
    if !state.templates.is_empty() {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        show_templates_section(ui, config, state);
    }
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

    // Check if there are existing agents (in agents/ directory) to clone from
    let has_clone_sources = !state.templates.is_empty();

    // Auto-fallback: if in Clone mode but no agents exist, switch to Blank
    if state.create_mode == AgentCreateMode::Clone && !has_clone_sources {
        state.create_mode = AgentCreateMode::Blank;
        state.clone_source = None;
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
                    state.template_source = None;
                }
                // Only show "Clone Existing" when there are agents/ directories to clone from
                if has_clone_sources {
                    if ui
                        .selectable_label(
                            state.create_mode == AgentCreateMode::Clone,
                            "Clone Existing",
                        )
                        .clicked()
                    {
                        state.create_mode = AgentCreateMode::Clone;
                        state.template_source = None;
                    }
                }
                if !state.templates.is_empty() {
                    if ui
                        .selectable_label(
                            state.create_mode == AgentCreateMode::Template,
                            "From Template",
                        )
                        .clicked()
                    {
                        state.create_mode = AgentCreateMode::Template;
                        state.clone_source = None;
                    }
                }
            });

            ui.add_space(4.0);

            // Template source picker (only shown in Template mode)
            if state.create_mode == AgentCreateMode::Template {
                let templates = &state.templates;
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Template:").color(theme::SUBTEXT),
                    );

                    let selected_label = state
                        .template_source
                        .and_then(|idx| templates.get(idx))
                        .map(|t| t.dir_name.as_str())
                        .unwrap_or("Select template...");

                    egui::ComboBox::from_id_salt("template_source")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            for (idx, tmpl) in templates.iter().enumerate() {
                                let label = &tmpl.dir_name;
                                if ui
                                    .selectable_value(
                                        &mut state.template_source,
                                        Some(idx),
                                        label,
                                    )
                                    .changed()
                                {
                                    // Auto-fill ID and name from template dir name
                                    if state.new_agent_id.is_empty() {
                                        state.new_agent_id = tmpl.dir_name.clone();
                                    }
                                    if state.new_agent_name.is_empty() {
                                        state.new_agent_name = tmpl.dir_name.clone();
                                    }
                                }
                            }
                        });
                });

                // Show template info
                if let Some(idx) = state.template_source {
                    if let Some(tmpl) = state.templates.get(idx) {
                        let mut info_parts = Vec::new();
                        if tmpl.has_workspace {
                            info_parts.push("has workspace/");
                        }
                        if tmpl.has_config {
                            info_parts.push("has config/");
                        }
                        let info = if info_parts.is_empty() {
                            "empty template directory".to_string()
                        } else {
                            info_parts.join(", ")
                        };
                        ui.label(
                            egui::RichText::new(format!("  {}", info))
                                .color(theme::OVERLAY)
                                .small()
                                .italics(),
                        );
                    }
                }

                ui.add_space(4.0);
            }

            // Clone source picker (only shown in Clone mode — sources from agents/ directory)
            if state.create_mode == AgentCreateMode::Clone {
                let templates = &state.templates;
                if templates.is_empty() {
                    ui.label(
                        egui::RichText::new("No agents found in agents/ directory.")
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
                            .and_then(|idx| templates.get(idx))
                            .map(|t| t.dir_name.as_str())
                            .unwrap_or("Select agent...");

                        egui::ComboBox::from_id_salt("clone_source")
                            .selected_text(selected_label)
                            .show_ui(ui, |ui| {
                                for (idx, tmpl) in templates.iter().enumerate() {
                                    let label = &tmpl.dir_name;
                                    if ui
                                        .selectable_value(
                                            &mut state.clone_source,
                                            Some(idx),
                                            label,
                                        )
                                        .changed()
                                    {
                                        // Auto-fill ID and name from template dir name
                                        if state.new_agent_id.is_empty() {
                                            state.new_agent_id = tmpl.dir_name.clone();
                                        }
                                        if state.new_agent_name.is_empty() {
                                            state.new_agent_name =
                                                format!("Copy of {}", tmpl.dir_name);
                                        }
                                    }
                                }
                            });
                    });

                    // Show template info when selected
                    if let Some(idx) = state.clone_source {
                        if let Some(tmpl) = templates.get(idx) {
                            let mut info_parts = Vec::new();
                            if tmpl.has_workspace {
                                info_parts.push("has workspace/");
                            }
                            if tmpl.has_config {
                                info_parts.push("has config/");
                            }
                            let info = if info_parts.is_empty() {
                                "empty agent directory".to_string()
                            } else {
                                info_parts.join(", ")
                            };
                            ui.label(
                                egui::RichText::new(format!("  {}", info))
                                    .color(theme::OVERLAY)
                                    .small()
                                    .italics(),
                            );
                        }
                    }
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
                && match state.create_mode {
                    AgentCreateMode::Blank => true,
                    AgentCreateMode::Clone => state.clone_source.is_some(),
                    AgentCreateMode::Template => state.template_source.is_some(),
                };

            ui.horizontal(|ui| {
                let button_text = match state.create_mode {
                    AgentCreateMode::Blank => "+ Create Agent",
                    AgentCreateMode::Clone => "+ Clone Agent",
                    AgentCreateMode::Template => "+ Create from Template",
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
                            // Clone from agents/ directory template
                            let mut entry = AgentEntry::default();
                            if let Some(tmpl) = state
                                .clone_source
                                .and_then(|idx| state.templates.get(idx))
                            {
                                let agent_dir = tmpl.path.canonicalize()
                                    .unwrap_or_else(|_| tmpl.path.clone());
                                if tmpl.has_workspace {
                                    entry.workspace = Some(
                                        agent_dir.join("workspace")
                                            .to_string_lossy()
                                            .into_owned(),
                                    );
                                }
                                entry.agent_dir = Some(
                                    agent_dir.to_string_lossy().into_owned(),
                                );
                            }
                            entry
                        }
                        AgentCreateMode::Template => {
                            // Create from template directory
                            let mut entry = AgentEntry::default();
                            if let Some(tmpl) = state
                                .template_source
                                .and_then(|idx| state.templates.get(idx))
                            {
                                let agent_dir = tmpl.path.canonicalize()
                                    .unwrap_or_else(|_| tmpl.path.clone());
                                if tmpl.has_workspace {
                                    entry.workspace = Some(
                                        agent_dir.join("workspace")
                                            .to_string_lossy()
                                            .into_owned(),
                                    );
                                }
                                entry.agent_dir = Some(
                                    agent_dir.to_string_lossy().into_owned(),
                                );
                            }
                            entry
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
                    state.template_source = None;
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
// Agent templates section (discovered from agents/ directory)
// ---------------------------------------------------------------------------

fn show_templates_section(
    ui: &mut egui::Ui,
    config: &mut ConfigManager,
    state: &mut AgentsViewState,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("TEMPLATES")
                .color(theme::OVERLAY)
                .small(),
        );
        ui.label(
            egui::RichText::new("(from agents/ directory)")
                .color(theme::OVERLAY)
                .small()
                .italics(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Refresh").clicked() {
                state.templates = discover_templates();
            }
        });
    });
    ui.add_space(4.0);

    // Collect configured agent IDs to check which templates are already added
    let configured_ids: Vec<String> = config
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

    // Also collect configured agentDir paths for dedup
    let configured_dirs: Vec<String> = config
        .draft()
        .agents
        .as_ref()
        .and_then(|a| a.list.as_ref())
        .map(|list| {
            list.iter()
                .filter_map(|e| e.agent_dir.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let templates = state.templates.clone();
    let mut add_template: Option<usize> = None;

    for (idx, tmpl) in templates.iter().enumerate() {
        // Check if this template is already in the config
        let already_configured = configured_ids.iter().any(|id| id == &tmpl.dir_name)
            || configured_dirs.iter().any(|d| {
                let d_path = PathBuf::from(d);
                d_path == tmpl.path
                    || tmpl
                        .path
                        .canonicalize()
                        .ok()
                        .is_some_and(|canon| d_path == canon)
            });

        egui::Frame::default()
            .fill(theme::SURFACE0)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Template icon (folder indicator)
                    let color = if already_configured {
                        theme::GREEN
                    } else {
                        theme::BLUE
                    };
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 5.0, color);

                    // Template info
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(&tmpl.dir_name)
                                .size(16.0)
                                .color(theme::TEXT),
                        );
                        let mut subtitle_parts = Vec::new();
                        subtitle_parts
                            .push(format!("path: {}", tmpl.path.display()));
                        if tmpl.has_workspace {
                            subtitle_parts.push("workspace".to_string());
                        }
                        if tmpl.has_config {
                            subtitle_parts.push("config".to_string());
                        }
                        if already_configured {
                            subtitle_parts.push("already configured".to_string());
                        }
                        ui.label(
                            egui::RichText::new(subtitle_parts.join(" \u{00b7} "))
                                .small()
                                .color(theme::SUBTEXT),
                        );
                    });

                    // Right side: Add to Config button
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if already_configured {
                                ui.label(
                                    egui::RichText::new("In Config")
                                        .color(theme::GREEN)
                                        .small(),
                                );
                            } else if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("+ Add to Config")
                                            .color(theme::GREEN),
                                    )
                                    .small(),
                                )
                                .clicked()
                            {
                                add_template = Some(idx);
                            }
                        },
                    );
                });
            });

        ui.add_space(4.0);
    }

    // Apply deferred template addition
    if let Some(idx) = add_template {
        if let Some(tmpl) = templates.get(idx) {
            config.begin_edit();
            let agents = config
                .draft_mut()
                .agents
                .get_or_insert_with(AgentsConfig::default);
            let list = agents.list.get_or_insert_with(Vec::new);

            let mut new_agent = AgentEntry::default();
            new_agent.id = Some(tmpl.dir_name.clone());
            new_agent.name = Some(tmpl.dir_name.clone());
            let agent_dir = tmpl
                .path
                .canonicalize()
                .unwrap_or_else(|_| tmpl.path.clone());
            new_agent.agent_dir = Some(agent_dir.to_string_lossy().into_owned());
            if tmpl.has_workspace {
                new_agent.workspace = Some(
                    agent_dir
                        .join("workspace")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            list.push(new_agent);
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
