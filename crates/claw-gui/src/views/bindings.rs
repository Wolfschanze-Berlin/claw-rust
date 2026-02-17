//! Bindings matrix view — agent × channel cross-reference table.
//!
//! Displays a grid where rows are agents and columns are channels.
//! Clicking a cell toggles the binding between that agent and channel.

use eframe::egui;

use claw_config::{BindingEntry, BindingMatch, OpenClawConfig};

use crate::config_manager::ConfigManager;
use crate::theme;

/// Render the bindings matrix view.
///
/// Shows agents as rows, channels as columns, with checkbox cells
/// indicating whether a binding exists for each pair. Clicking a cell
/// adds or removes the corresponding [`BindingEntry`].
pub fn show(ui: &mut egui::Ui, mgr: &mut ConfigManager) {
    ui.heading("Bindings");
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Map agents to channels. Click a cell to toggle a binding.")
            .color(theme::SUBTEXT),
    );
    ui.add_space(16.0);

    let draft = mgr.draft().clone();

    let agent_ids = collect_agent_ids(&draft);
    let channel_names = collect_channel_names(&draft);

    if agent_ids.is_empty() || channel_names.is_empty() {
        empty_state(ui, agent_ids.is_empty(), channel_names.is_empty());
        return;
    }

    let mut changed = false;

    // Matrix table
    egui::Frame::default()
        .fill(theme::SURFACE0)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            egui::Grid::new("bindings_matrix")
                .spacing([16.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    // Header row: empty corner cell + channel names
                    ui.label(egui::RichText::new("Agent \\ Channel").color(theme::OVERLAY));
                    for ch in &channel_names {
                        ui.label(egui::RichText::new(ch.as_str()).color(theme::BLUE).strong());
                    }
                    ui.end_row();

                    // One row per agent
                    for agent_id in &agent_ids {
                        ui.label(egui::RichText::new(agent_id.as_str()).color(theme::TEXT));

                        for ch in &channel_names {
                            let bound = has_binding(&draft, agent_id, ch);

                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(20.0, 20.0),
                                egui::Sense::click(),
                            );

                            // Draw checkbox-style cell
                            let bg = if bound { theme::BLUE } else { theme::SURFACE1 };
                            let hovered = response.hovered();
                            let fill = if hovered {
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
                                // Draw checkmark
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "\u{2713}",
                                    egui::FontId::proportional(14.0),
                                    theme::BASE,
                                );
                            }

                            // Tooltip + click (on_hover_text consumes self, returns Response)
                            let response = response.on_hover_text(if bound {
                                format!("Remove binding: {agent_id} \u{2192} {ch}")
                            } else {
                                format!("Add binding: {agent_id} \u{2192} {ch}")
                            });

                            if response.clicked() {
                                mgr.begin_edit();
                                if bound {
                                    remove_binding(mgr.draft_mut(), agent_id, ch);
                                } else {
                                    add_binding(mgr.draft_mut(), agent_id, ch);
                                }
                                changed = true;
                            }
                        }

                        ui.end_row();
                    }
                });
        });

    if changed {
        mgr.validate();
    }

    // Summary below the matrix
    ui.add_space(12.0);
    let binding_count = draft.bindings.as_ref().map_or(0, |b| b.len());
    ui.label(
        egui::RichText::new(format!(
            "{} binding{} configured",
            binding_count,
            if binding_count == 1 { "" } else { "s" }
        ))
        .color(theme::SUBTEXT),
    );
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
