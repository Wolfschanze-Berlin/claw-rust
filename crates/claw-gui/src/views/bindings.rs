//! Bindings CRUD view — agent × channel routing with full match criteria.
//!
//! Combines a quick-toggle matrix (agent × channel grid) with a full CRUD
//! list for editing advanced match rules (account_id, peer, guild_id, roles).

use eframe::egui;

use claw_config::{BindingEntry, BindingMatch, BindingPeer, OpenClawConfig};

use crate::config_manager::ConfigManager;
use crate::theme;
use crate::widgets::form_field;

// ---------------------------------------------------------------------------
// Persistent UI state
// ---------------------------------------------------------------------------

/// UI state that persists across frames for the bindings view.
pub struct BindingsViewState {
    /// Index of the currently selected binding (if any).
    pub selected: Option<usize>,
    /// Detail form state for the selected binding.
    pub detail_state: Option<BindingDetailState>,
    /// Whether we've already taken an undo snapshot for this detail session.
    detail_snapshot_taken: bool,
    /// Whether the creation section is expanded.
    pub create_section_open: bool,
    /// Selected agent ID for new binding creation.
    pub new_agent_idx: Option<usize>,
    /// Selected channel name for new binding creation.
    pub new_channel_idx: Option<usize>,
    /// Whether the quick-toggle matrix is expanded.
    pub matrix_open: bool,
}

impl Default for BindingsViewState {
    fn default() -> Self {
        Self {
            selected: None,
            detail_state: None,
            detail_snapshot_taken: false,
            create_section_open: false,
            new_agent_idx: None,
            new_channel_idx: None,
            matrix_open: false,
        }
    }
}

/// Persistent UI state for the binding detail form.
pub struct BindingDetailState {
    /// Index of the binding being edited.
    pub binding_idx: usize,
    /// Whether to show the delete confirmation.
    pub confirm_delete: bool,
}

impl BindingDetailState {
    fn new(idx: usize) -> Self {
        Self {
            binding_idx: idx,
            confirm_delete: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Render the bindings view (list or detail, depending on selection).
pub fn show(ui: &mut egui::Ui, mgr: &mut ConfigManager, state: &mut BindingsViewState) {
    if state.selected.is_some() {
        show_detail(ui, mgr, state);
    } else {
        show_list(ui, mgr, state);
    }
}

// ---------------------------------------------------------------------------
// List view
// ---------------------------------------------------------------------------

fn show_list(ui: &mut egui::Ui, mgr: &mut ConfigManager, state: &mut BindingsViewState) {
    // Header
    ui.horizontal(|ui| {
        ui.heading("Bindings");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let count = mgr
                .draft()
                .bindings
                .as_ref()
                .map_or(0, |b| b.len());
            ui.label(
                egui::RichText::new(format!("{count} configured"))
                    .color(theme::SUBTEXT)
                    .small(),
            );
        });
    });
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Map agents to channels with optional match criteria.")
            .color(theme::SUBTEXT),
    );
    ui.add_space(8.0);

    let draft = mgr.draft().clone();
    let agent_ids = collect_agent_ids(&draft);
    let channel_names = collect_channel_names(&draft);

    // Quick-toggle matrix (collapsible)
    show_matrix_section(ui, mgr, state, &agent_ids, &channel_names);

    ui.add_space(8.0);

    // New binding form
    show_add_form(ui, mgr, state, &agent_ids, &channel_names);

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Binding cards
    show_binding_cards(ui, mgr, state, &agent_ids, &channel_names);
}

// ---------------------------------------------------------------------------
// Quick-toggle matrix (collapsible)
// ---------------------------------------------------------------------------

fn show_matrix_section(
    ui: &mut egui::Ui,
    mgr: &mut ConfigManager,
    state: &mut BindingsViewState,
    agent_ids: &[String],
    channel_names: &[String],
) {
    let toggle_label = if state.matrix_open {
        "\u{25bc} Quick Toggle Matrix"
    } else {
        "\u{25b6} Quick Toggle Matrix"
    };
    if ui
        .add(egui::Button::new(
            egui::RichText::new(toggle_label).color(theme::BLUE),
        ))
        .clicked()
    {
        state.matrix_open = !state.matrix_open;
    }

    if !state.matrix_open {
        return;
    }

    if agent_ids.is_empty() || channel_names.is_empty() {
        empty_state(ui, agent_ids.is_empty(), channel_names.is_empty());
        return;
    }

    let draft = mgr.draft().clone();

    egui::Frame::default()
        .fill(theme::SURFACE0)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            egui::Grid::new("bindings_matrix")
                .spacing([16.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    // Header row
                    ui.label(egui::RichText::new("Agent \\ Channel").color(theme::OVERLAY));
                    for ch in channel_names {
                        ui.label(
                            egui::RichText::new(ch.as_str()).color(theme::BLUE).strong(),
                        );
                    }
                    ui.end_row();

                    // One row per agent
                    for agent_id in agent_ids {
                        ui.label(egui::RichText::new(agent_id.as_str()).color(theme::TEXT));

                        for ch in channel_names {
                            let bound = has_binding(&draft, agent_id, ch);

                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(20.0, 20.0),
                                egui::Sense::click(),
                            );

                            let bg = if bound { theme::BLUE } else { theme::SURFACE1 };
                            let fill = if response.hovered() {
                                lerp_color(bg, theme::TEXT, 0.15)
                            } else {
                                bg
                            };

                            ui.painter().rect_filled(
                                rect,
                                egui::CornerRadius::same(4),
                                fill,
                            );

                            if bound {
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "\u{2713}",
                                    egui::FontId::proportional(14.0),
                                    theme::BASE,
                                );
                            }

                            let response = response.on_hover_text(if bound {
                                format!("Remove: {agent_id} \u{2192} {ch}")
                            } else {
                                format!("Add: {agent_id} \u{2192} {ch}")
                            });

                            if response.clicked() {
                                mgr.begin_edit();
                                if bound {
                                    remove_binding(mgr.draft_mut(), agent_id, ch);
                                } else {
                                    add_binding(mgr.draft_mut(), agent_id, ch);
                                }
                            }
                        }

                        ui.end_row();
                    }
                });
        });
}

// ---------------------------------------------------------------------------
// Add binding form
// ---------------------------------------------------------------------------

fn show_add_form(
    ui: &mut egui::Ui,
    mgr: &mut ConfigManager,
    state: &mut BindingsViewState,
    agent_ids: &[String],
    channel_names: &[String],
) {
    let toggle_label = if state.create_section_open {
        "\u{25bc} New Binding"
    } else {
        "\u{25b6} New Binding"
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

    if agent_ids.is_empty() || channel_names.is_empty() {
        ui.label(
            egui::RichText::new("Add agents and channels first before creating bindings.")
                .color(theme::YELLOW)
                .small(),
        );
        return;
    }

    egui::Frame::default()
        .fill(theme::SURFACE0)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Agent picker
                ui.label(egui::RichText::new("Agent:").color(theme::SUBTEXT));
                let agent_label = state
                    .new_agent_idx
                    .and_then(|i| agent_ids.get(i))
                    .map(|s| s.as_str())
                    .unwrap_or("Select agent...");
                egui::ComboBox::from_id_salt("new_binding_agent")
                    .selected_text(agent_label)
                    .show_ui(ui, |ui| {
                        for (idx, id) in agent_ids.iter().enumerate() {
                            ui.selectable_value(&mut state.new_agent_idx, Some(idx), id.as_str());
                        }
                    });

                ui.add_space(8.0);

                // Channel picker
                ui.label(egui::RichText::new("Channel:").color(theme::SUBTEXT));
                let channel_label = state
                    .new_channel_idx
                    .and_then(|i| channel_names.get(i))
                    .map(|s| s.as_str())
                    .unwrap_or("Select channel...");
                egui::ComboBox::from_id_salt("new_binding_channel")
                    .selected_text(channel_label)
                    .show_ui(ui, |ui| {
                        for (idx, name) in channel_names.iter().enumerate() {
                            ui.selectable_value(
                                &mut state.new_channel_idx,
                                Some(idx),
                                name.as_str(),
                            );
                        }
                    });
            });

            ui.add_space(4.0);

            // Create button
            let valid = state.new_agent_idx.is_some() && state.new_channel_idx.is_some();

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        valid,
                        egui::Button::new(
                            egui::RichText::new("+ Create Binding").color(theme::GREEN),
                        ),
                    )
                    .clicked()
                {
                    let agent_id = agent_ids[state.new_agent_idx.unwrap()].clone();
                    let channel = channel_names[state.new_channel_idx.unwrap()].clone();

                    mgr.begin_edit();
                    add_binding(mgr.draft_mut(), &agent_id, &channel);
                    state.new_agent_idx = None;
                    state.new_channel_idx = None;
                    state.create_section_open = false;
                }

                // Check for duplicate
                if valid {
                    let agent_id = &agent_ids[state.new_agent_idx.unwrap()];
                    let channel = &channel_names[state.new_channel_idx.unwrap()];
                    if has_binding(mgr.draft(), agent_id, channel) {
                        ui.label(
                            egui::RichText::new("Binding already exists (will create duplicate)")
                                .color(theme::YELLOW)
                                .small(),
                        );
                    }
                }
            });
        });
}

// ---------------------------------------------------------------------------
// Binding cards
// ---------------------------------------------------------------------------

fn show_binding_cards(
    ui: &mut egui::Ui,
    mgr: &mut ConfigManager,
    state: &mut BindingsViewState,
    _agent_ids: &[String],
    _channel_names: &[String],
) {
    let bindings = mgr.draft().bindings.clone();
    let Some(bindings) = bindings.as_ref() else {
        ui.label(
            egui::RichText::new("No bindings configured. Use the matrix or add form above.")
                .color(theme::OVERLAY)
                .italics(),
        );
        return;
    };

    if bindings.is_empty() {
        ui.label(
            egui::RichText::new("No bindings configured. Use the matrix or add form above.")
                .color(theme::OVERLAY)
                .italics(),
        );
        return;
    }

    let mut to_delete: Option<usize> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (idx, binding) in bindings.iter().enumerate() {
            let agent = binding.agent_id.as_deref().unwrap_or("<no agent>");
            let channel = binding
                .match_rule
                .as_ref()
                .and_then(|m| m.channel.as_deref())
                .unwrap_or("<any>");
            let has_advanced = binding_has_advanced_match(binding);

            egui::Frame::default()
                .fill(theme::SURFACE0)
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Binding indicator dot
                        let color = theme::BLUE;
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 5.0, color);

                        // Binding info (clickable)
                        let response = ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{agent} \u{2192} {channel}"))
                                    .size(16.0)
                                    .color(theme::TEXT),
                            );
                            let mut subtitle_parts = vec![format!("#{}", idx + 1)];
                            if has_advanced {
                                subtitle_parts.push("advanced match".to_string());
                            }
                            let account = binding
                                .match_rule
                                .as_ref()
                                .and_then(|m| m.account_id.as_deref());
                            if let Some(acc) = account {
                                subtitle_parts.push(format!("account: {acc}"));
                            }
                            ui.label(
                                egui::RichText::new(subtitle_parts.join(" \u{00b7} "))
                                    .small()
                                    .color(theme::SUBTEXT),
                            );
                        });

                        if response.response.interact(egui::Sense::click()).clicked() {
                            state.selected = Some(idx);
                            state.detail_state = Some(BindingDetailState::new(idx));
                        }

                        // Right side: edit + delete buttons
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
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
                                    state.detail_state = Some(BindingDetailState::new(idx));
                                }
                            },
                        );
                    });
                });

            ui.add_space(4.0);
        }
    });

    // Apply deferred deletion
    if let Some(idx) = to_delete {
        mgr.begin_edit();
        if let Some(bindings) = mgr.draft_mut().bindings.as_mut() {
            if idx < bindings.len() {
                bindings.remove(idx);
                if bindings.is_empty() {
                    mgr.draft_mut().bindings = None;
                }
                // Fix selection if needed
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
// Detail view
// ---------------------------------------------------------------------------

fn show_detail(ui: &mut egui::Ui, mgr: &mut ConfigManager, state: &mut BindingsViewState) {
    let Some(idx) = state.selected else {
        state.detail_state = None;
        return;
    };

    // Ensure detail state exists
    if state.detail_state.is_none() {
        state.detail_state = Some(BindingDetailState::new(idx));
    }

    let binding_count = mgr
        .draft()
        .bindings
        .as_ref()
        .map_or(0, |b| b.len());

    if idx >= binding_count {
        state.selected = None;
        state.detail_state = None;
        return;
    }

    // Snapshot once when first entering detail view
    if !state.detail_snapshot_taken {
        mgr.begin_edit();
        state.detail_snapshot_taken = true;
    }

    let detail = state.detail_state.as_mut().unwrap();
    let mut action = DetailAction::None;

    // Collect agent/channel lists for dropdown pickers
    let draft_snapshot = mgr.draft().clone();
    let agent_ids = collect_agent_ids(&draft_snapshot);
    let channel_names = collect_channel_names(&draft_snapshot);

    // Get a preview of current binding for the header
    let preview_agent = mgr
        .draft()
        .bindings
        .as_ref()
        .and_then(|b| b.get(idx))
        .and_then(|b| b.agent_id.clone())
        .unwrap_or_else(|| "<no agent>".to_string());
    let preview_channel = mgr
        .draft()
        .bindings
        .as_ref()
        .and_then(|b| b.get(idx))
        .and_then(|b| b.match_rule.as_ref())
        .and_then(|m| m.channel.clone())
        .unwrap_or_else(|| "<any>".to_string());

    // -- Header row --
    ui.horizontal(|ui| {
        if ui.button("\u{2190} Back").clicked() {
            action = DetailAction::Back;
        }
        ui.heading(
            egui::RichText::new(format!(
                "Binding: {} \u{2192} {}",
                preview_agent, preview_channel
            ))
            .color(theme::TEXT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if detail.confirm_delete {
                ui.label(egui::RichText::new("Are you sure?").color(theme::YELLOW));
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("Yes, delete").color(theme::RED),
                    ))
                    .clicked()
                {
                    action = DetailAction::Delete;
                }
                if ui.button("Cancel").clicked() {
                    detail.confirm_delete = false;
                }
            } else if ui
                .add(egui::Button::new(
                    egui::RichText::new("Delete Binding").color(theme::RED),
                ))
                .clicked()
            {
                detail.confirm_delete = true;
            }
        });
    });

    ui.separator();

    // Get mutable reference to the binding
    let binding = mgr
        .draft_mut()
        .bindings
        .as_mut()
        .unwrap()
        .get_mut(idx)
        .unwrap();

    // -- Scrollable form body --
    egui::ScrollArea::vertical().show(ui, |ui| {
        // ---- Agent Assignment ----
        egui::CollapsingHeader::new(
            egui::RichText::new("Agent Assignment")
                .color(theme::TEXT)
                .strong(),
        )
        .id_salt("binding_agent")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Agent:").color(theme::SUBTEXT));

                let current_agent = binding
                    .agent_id
                    .as_deref()
                    .unwrap_or("None");
                egui::ComboBox::from_id_salt("binding_agent_select")
                    .selected_text(current_agent)
                    .show_ui(ui, |ui| {
                        // "None" option
                        if ui
                            .selectable_label(binding.agent_id.is_none(), "None")
                            .clicked()
                        {
                            binding.agent_id = None;
                        }
                        for id in &agent_ids {
                            let selected =
                                binding.agent_id.as_deref() == Some(id.as_str());
                            if ui.selectable_label(selected, id.as_str()).clicked() {
                                binding.agent_id = Some(id.clone());
                            }
                        }
                    });
            });
        });

        // ---- Match Criteria ----
        egui::CollapsingHeader::new(
            egui::RichText::new("Match Criteria")
                .color(theme::TEXT)
                .strong(),
        )
        .id_salt("binding_match")
        .default_open(true)
        .show(ui, |ui| {
            let match_rule = binding.match_rule.get_or_insert_with(BindingMatch::default);

            // Channel selector (dropdown)
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Channel:").color(theme::SUBTEXT));

                let current_ch = match_rule
                    .channel
                    .as_deref()
                    .unwrap_or("Any (no filter)");
                egui::ComboBox::from_id_salt("binding_channel_select")
                    .selected_text(current_ch)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(match_rule.channel.is_none(), "Any (no filter)")
                            .clicked()
                        {
                            match_rule.channel = None;
                        }
                        for name in &channel_names {
                            let selected =
                                match_rule.channel.as_deref() == Some(name.as_str());
                            if ui.selectable_label(selected, name.as_str()).clicked() {
                                match_rule.channel = Some(name.clone());
                            }
                        }
                    });
            });

            form_field::text_field(ui, "Account ID", &mut match_rule.account_id);
            form_field::text_field(ui, "Guild ID", &mut match_rule.guild_id);
            form_field::text_field(ui, "Team ID", &mut match_rule.team_id);
            form_field::string_list_field(ui, "Roles", &mut match_rule.roles);
        });

        // ---- Peer Match ----
        egui::CollapsingHeader::new(
            egui::RichText::new("Peer Match")
                .color(theme::TEXT)
                .strong(),
        )
        .id_salt("binding_peer")
        .default_open(false)
        .show(ui, |ui| {
            let match_rule = binding.match_rule.get_or_insert_with(BindingMatch::default);

            let has_peer = match_rule.peer.is_some();
            if has_peer {
                let peer = match_rule.peer.as_mut().unwrap();

                form_field::select_field(
                    ui,
                    "Kind",
                    &mut peer.kind,
                    &["direct", "group", "channel"],
                );
                form_field::text_field(ui, "Peer ID", &mut peer.id);

                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("Remove Peer Filter").color(theme::RED),
                    ).small())
                    .clicked()
                {
                    match_rule.peer = None;
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("No peer filter set.")
                            .color(theme::OVERLAY)
                            .italics(),
                    );
                    if ui.small_button("Add Peer Filter").clicked() {
                        match_rule.peer = Some(BindingPeer::default());
                    }
                });
            }
        });
    });

    // Handle actions
    match action {
        DetailAction::Back => {
            state.selected = None;
            state.detail_state = None;
            state.detail_snapshot_taken = false;
        }
        DetailAction::Delete => {
            if let Some(bindings) = mgr.draft_mut().bindings.as_mut() {
                if idx < bindings.len() {
                    bindings.remove(idx);
                    if bindings.is_empty() {
                        mgr.draft_mut().bindings = None;
                    }
                }
            }
            state.selected = None;
            state.detail_state = None;
            state.detail_snapshot_taken = false;
        }
        DetailAction::None => {}
    }
}

/// Actions the detail form can request.
enum DetailAction {
    None,
    Back,
    Delete,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check if a binding has advanced match criteria beyond just channel.
fn binding_has_advanced_match(binding: &BindingEntry) -> bool {
    binding
        .match_rule
        .as_ref()
        .is_some_and(|m| {
            m.account_id.is_some()
                || m.peer.is_some()
                || m.guild_id.is_some()
                || m.roles.is_some()
                || m.team_id.is_some()
        })
}

/// Show empty state when agents or channels are missing.
fn empty_state(ui: &mut egui::Ui, no_agents: bool, no_channels: bool) {
    egui::Frame::default()
        .fill(theme::SURFACE0)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(24))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("Cannot build bindings matrix")
                        .color(theme::YELLOW)
                        .size(16.0),
                );
                ui.add_space(8.0);
                if no_agents {
                    ui.label(
                        egui::RichText::new("No agents defined. Add agents first.")
                            .color(theme::SUBTEXT),
                    );
                }
                if no_channels {
                    ui.label(
                        egui::RichText::new("No channels defined. Add channels first.")
                            .color(theme::SUBTEXT),
                    );
                }
            });
        });
}

/// Collect sorted agent IDs from the config.
fn collect_agent_ids(config: &OpenClawConfig) -> Vec<String> {
    let mut ids: Vec<String> = config
        .agents
        .as_ref()
        .and_then(|a| a.list.as_ref())
        .map(|list| {
            list.iter()
                .filter_map(|agent| agent.id.clone())
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids
}

/// Collect sorted channel names from the config.
fn collect_channel_names(config: &OpenClawConfig) -> Vec<String> {
    let mut names: Vec<String> = config
        .channels
        .as_ref()
        .map(|ch| ch.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// Check whether a binding exists for the given agent and channel.
fn has_binding(config: &OpenClawConfig, agent_id: &str, channel: &str) -> bool {
    config
        .bindings
        .as_ref()
        .map(|bindings| {
            bindings.iter().any(|b| {
                b.agent_id.as_deref() == Some(agent_id)
                    && b.match_rule
                        .as_ref()
                        .and_then(|m| m.channel.as_deref())
                        == Some(channel)
            })
        })
        .unwrap_or(false)
}

/// Add a new binding entry for the given agent and channel.
fn add_binding(config: &mut OpenClawConfig, agent_id: &str, channel: &str) {
    let entry = BindingEntry {
        agent_id: Some(agent_id.to_string()),
        match_rule: Some(BindingMatch {
            channel: Some(channel.to_string()),
            ..Default::default()
        }),
    };

    match &mut config.bindings {
        Some(bindings) => bindings.push(entry),
        None => config.bindings = Some(vec![entry]),
    }
}

/// Remove the binding entry matching the given agent and channel.
fn remove_binding(config: &mut OpenClawConfig, agent_id: &str, channel: &str) {
    if let Some(bindings) = &mut config.bindings {
        bindings.retain(|b| {
            !(b.agent_id.as_deref() == Some(agent_id)
                && b.match_rule
                    .as_ref()
                    .and_then(|m| m.channel.as_deref())
                    == Some(channel))
        });

        if bindings.is_empty() {
            config.bindings = None;
        }
    }
}

/// Linearly interpolate between two colors.
fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let mix = |x: u8, y: u8| -> u8 { (x as f32 * (1.0 - t) + y as f32 * t) as u8 };
    egui::Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}
