//! Channels CRUD list view backed by ConfigManager draft.
//!
//! Shows all configured channels with create, delete, and select
//! actions. Selecting a channel navigates to the detail form
//! (see [`channel_detail`](super::channel_detail)).

use std::collections::HashMap;

use eframe::egui;

use claw_config::ChannelConfig;

use crate::config_manager::ConfigManager;
use crate::theme;
use crate::views::channel_detail::{ChannelDetailState, DetailAction};

// ---------------------------------------------------------------------------
// Persistent UI state for the channels view
// ---------------------------------------------------------------------------

/// UI state that persists across frames for the channels list.
pub struct ChannelsViewState {
    /// Currently selected channel key (if any).
    pub selected: Option<String>,
    /// Text buffer for the "new channel name" input.
    pub new_channel_name: String,
    /// Detail form state for the selected channel.
    pub detail_state: Option<ChannelDetailState>,
    /// Whether we've already taken an undo snapshot for this detail session.
    detail_snapshot_taken: bool,
}

impl Default for ChannelsViewState {
    fn default() -> Self {
        Self {
            selected: None,
            new_channel_name: String::new(),
            detail_state: None,
            detail_snapshot_taken: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Render the channels view (list or detail, depending on selection).
pub fn show(ui: &mut egui::Ui, config: &mut ConfigManager, state: &mut ChannelsViewState) {
    if let Some(selected_key) = state.selected.clone() {
        show_detail(ui, config, state, &selected_key);
    } else {
        show_list(ui, config, state);
    }
}

// ---------------------------------------------------------------------------
// List view
// ---------------------------------------------------------------------------

fn show_list(ui: &mut egui::Ui, config: &mut ConfigManager, state: &mut ChannelsViewState) {
    // Header
    ui.horizontal(|ui| {
        ui.heading("Channels");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let channels = config.draft().channels.as_ref();
            let count = channels.map_or(0, |m| m.len());
            ui.label(
                egui::RichText::new(format!("{count} configured"))
                    .color(theme::SUBTEXT)
                    .small(),
            );
        });
    });
    ui.add_space(8.0);

    // Add channel row
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("New channel:").color(theme::SUBTEXT));
        ui.text_edit_singleline(&mut state.new_channel_name);

        let name = state.new_channel_name.trim().to_lowercase();
        let exists = config
            .draft()
            .channels
            .as_ref()
            .is_some_and(|m| m.contains_key(&name));
        let valid = !name.is_empty() && !exists;

        if ui
            .add_enabled(
                valid,
                egui::Button::new(egui::RichText::new("+ Add").color(theme::GREEN)),
            )
            .clicked()
        {
            config.begin_edit();
            let channels = config.draft_mut().channels.get_or_insert_with(HashMap::new);
            channels.insert(
                name,
                ChannelConfig {
                    enabled: Some(true),
                    ..Default::default()
                },
            );
            state.new_channel_name.clear();
        }
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Channel cards
    let channels = config.draft().channels.clone();
    let Some(channels) = channels.as_ref() else {
        ui.label(
            egui::RichText::new("No channels configured. Add one above.")
                .color(theme::OVERLAY)
                .italics(),
        );
        return;
    };

    if channels.is_empty() {
        ui.label(
            egui::RichText::new("No channels configured. Add one above.")
                .color(theme::OVERLAY)
                .italics(),
        );
        return;
    }

    // Sort by name for stable ordering
    let mut keys: Vec<&String> = channels.keys().collect();
    keys.sort();

    let mut to_delete: Option<String> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        for key in keys {
            let ch = &channels[key];
            let enabled = ch.enabled.unwrap_or(true);
            let account_count = ch.accounts.as_ref().map_or(0, |a| a.len());

            egui::Frame::default()
                .fill(theme::SURFACE0)
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Enabled dot
                        let color = if enabled { theme::GREEN } else { theme::OVERLAY };
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 5.0, color);

                        // Channel info (clickable to select)
                        let response = ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(key)
                                    .size(16.0)
                                    .color(theme::TEXT),
                            );
                            let subtitle = format!(
                                "{} account{} · {}",
                                account_count,
                                if account_count == 1 { "" } else { "s" },
                                if enabled { "enabled" } else { "disabled" },
                            );
                            ui.label(
                                egui::RichText::new(subtitle)
                                    .small()
                                    .color(theme::SUBTEXT),
                            );
                        });

                        // Make the whole info area clickable
                        if response.response.interact(egui::Sense::click()).clicked() {
                            state.selected = Some(key.clone());
                            state.detail_state = Some(ChannelDetailState::new(key.clone()));
                        }

                        // Right side: edit + delete buttons
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new("Delete").color(theme::RED),
                                ).small())
                                .clicked()
                            {
                                to_delete = Some(key.clone());
                            }

                            if ui.small_button("Edit").clicked() {
                                state.selected = Some(key.clone());
                                state.detail_state =
                                    Some(ChannelDetailState::new(key.clone()));
                            }
                        });
                    });
                });

            ui.add_space(4.0);
        }
    });

    // Apply deferred deletion
    if let Some(key) = to_delete {
        config.begin_edit();
        if let Some(channels) = config.draft_mut().channels.as_mut() {
            channels.remove(&key);
        }
    }
}

// ---------------------------------------------------------------------------
// Detail view (delegates to channel_detail module)
// ---------------------------------------------------------------------------

fn show_detail(
    ui: &mut egui::Ui,
    config: &mut ConfigManager,
    state: &mut ChannelsViewState,
    channel_key: &str,
) {
    // Ensure detail state exists
    if state.detail_state.is_none() {
        state.detail_state = Some(ChannelDetailState::new(channel_key.to_string()));
    }

    let detail_state = state.detail_state.as_mut().unwrap();

    // Get the channel config from draft (or redirect back if missing)
    let has_channel = config
        .draft()
        .channels
        .as_ref()
        .is_some_and(|m| m.contains_key(channel_key));

    if !has_channel {
        state.selected = None;
        state.detail_state = None;
        return;
    }

    // Run validation for the detail view
    let validation = claw_config::validate_config(config.draft());

    // Snapshot once when first entering detail view (not every frame).
    if !state.detail_snapshot_taken {
        config.begin_edit();
        state.detail_snapshot_taken = true;
    }

    let channel_config = config
        .draft_mut()
        .channels
        .as_mut()
        .unwrap()
        .get_mut(channel_key)
        .unwrap();

    let action =
        super::channel_detail::show(ui, channel_key, channel_config, detail_state, &validation);

    match action {
        DetailAction::Back => {
            state.selected = None;
            state.detail_state = None;
            state.detail_snapshot_taken = false;
        }
        DetailAction::Delete => {
            if let Some(channels) = config.draft_mut().channels.as_mut() {
                channels.remove(channel_key);
            }
            state.selected = None;
            state.detail_state = None;
            state.detail_snapshot_taken = false;
        }
        DetailAction::None => {}
    }
}
