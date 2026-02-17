//! Channels CRUD list view backed by ConfigManager draft.
//!
//! Shows all configured channels with create, delete, and select
//! actions. Selecting a channel navigates to the detail form
//! (see [`channel_detail`](super::channel_detail)).

use std::collections::HashMap;

use eframe::egui;

use claw_config::{ChannelAccountConfig, ChannelConfig};

use crate::config_manager::ConfigManager;
use crate::theme;
use crate::views::channel_detail::{ChannelDetailState, DetailAction};

/// Action returned by the channels view to the parent.
#[derive(Debug, Default)]
pub enum ChannelAction {
    #[default]
    None,
    /// Navigate to the Logs view filtered to this channel name.
    ViewLogs(String),
}

// ---------------------------------------------------------------------------
// Persistent UI state for the channels view
// ---------------------------------------------------------------------------

/// Supported channel platforms for the creation picker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChannelPlatform {
    Telegram,
    Discord,
    WhatsApp,
    Custom,
}

impl ChannelPlatform {
    /// All available platform choices.
    const ALL: &[ChannelPlatform] = &[
        ChannelPlatform::Telegram,
        ChannelPlatform::Discord,
        ChannelPlatform::WhatsApp,
        ChannelPlatform::Custom,
    ];

    /// Display label for the platform picker.
    fn label(self) -> &'static str {
        match self {
            ChannelPlatform::Telegram => "Telegram",
            ChannelPlatform::Discord => "Discord",
            ChannelPlatform::WhatsApp => "WhatsApp",
            ChannelPlatform::Custom => "Custom",
        }
    }

    /// Default channel key name derived from platform.
    fn default_key(self) -> &'static str {
        match self {
            ChannelPlatform::Telegram => "telegram",
            ChannelPlatform::Discord => "discord",
            ChannelPlatform::WhatsApp => "whatsapp",
            ChannelPlatform::Custom => "",
        }
    }
}

/// UI state that persists across frames for the channels list.
pub struct ChannelsViewState {
    /// Currently selected channel key (if any).
    pub selected: Option<String>,
    /// Text buffer for the "new channel name" input.
    pub new_channel_name: String,
    /// Selected platform for new channel creation.
    pub new_channel_platform: ChannelPlatform,
    /// Whether the creation section is expanded.
    pub create_section_open: bool,
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
            new_channel_platform: ChannelPlatform::Telegram,
            create_section_open: false,
            detail_state: None,
            detail_snapshot_taken: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Render the channels view (list or detail, depending on selection).
///
/// Returns a [`ChannelAction`] when the user triggers a cross-view navigation
/// (e.g. "View Logs" for a specific channel).
pub fn show(
    ui: &mut egui::Ui,
    config: &mut ConfigManager,
    state: &mut ChannelsViewState,
) -> ChannelAction {
    if let Some(selected_key) = state.selected.clone() {
        show_detail(ui, config, state, &selected_key);
        ChannelAction::None
    } else {
        show_list(ui, config, state)
    }
}

// ---------------------------------------------------------------------------
// List view
// ---------------------------------------------------------------------------

fn show_list(ui: &mut egui::Ui, config: &mut ConfigManager, state: &mut ChannelsViewState) -> ChannelAction {
    let mut action = ChannelAction::None;
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

    // Add channel — collapsible creation section with platform picker
    let toggle_label = if state.create_section_open {
        "\u{25bc} New Channel"
    } else {
        "\u{25b6} New Channel"
    };
    if ui
        .add(egui::Button::new(
            egui::RichText::new(toggle_label).color(theme::BLUE),
        ))
        .clicked()
    {
        state.create_section_open = !state.create_section_open;
    }

    if state.create_section_open {
        egui::Frame::default()
            .fill(theme::SURFACE0)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                // Platform picker
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Platform:").color(theme::SUBTEXT));
                    for platform in ChannelPlatform::ALL {
                        let selected = state.new_channel_platform == *platform;
                        if ui.selectable_label(selected, platform.label()).clicked() {
                            state.new_channel_platform = *platform;
                            // Auto-fill name from platform if name is empty or was auto-filled
                            let current = state.new_channel_name.trim().to_lowercase();
                            let is_auto = ChannelPlatform::ALL
                                .iter()
                                .any(|p| p.default_key() == current);
                            if current.is_empty() || is_auto {
                                state.new_channel_name =
                                    platform.default_key().to_string();
                            }
                        }
                    }
                });

                ui.add_space(4.0);

                // Channel name
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Name:").color(theme::SUBTEXT));
                    ui.text_edit_singleline(&mut state.new_channel_name);
                });

                ui.add_space(4.0);

                // Platform hint
                let hint = match state.new_channel_platform {
                    ChannelPlatform::Telegram => "Creates a Telegram channel with bot token account fields.",
                    ChannelPlatform::Discord => "Creates a Discord channel with bot token and application ID fields.",
                    ChannelPlatform::WhatsApp => "Creates a WhatsApp channel with session and pairing fields.",
                    ChannelPlatform::Custom => "Creates a blank channel — configure all fields manually.",
                };
                ui.label(
                    egui::RichText::new(hint)
                        .color(theme::OVERLAY)
                        .small()
                        .italics(),
                );

                ui.add_space(4.0);

                // Create button
                let name = state.new_channel_name.trim().to_lowercase();
                let exists = config
                    .draft()
                    .channels
                    .as_ref()
                    .is_some_and(|m| m.contains_key(&name));
                let valid = !name.is_empty() && !exists;

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            valid,
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "+ Create {} Channel",
                                    state.new_channel_platform.label()
                                ))
                                .color(theme::GREEN),
                            ),
                        )
                        .clicked()
                    {
                        config.begin_edit();
                        let channels =
                            config.draft_mut().channels.get_or_insert_with(HashMap::new);

                        // Build platform-appropriate default config
                        let mut ch = ChannelConfig {
                            enabled: Some(true),
                            ..Default::default()
                        };

                        // Pre-populate a default account with platform-specific hint
                        if state.new_channel_platform != ChannelPlatform::Custom {
                            let mut account = ChannelAccountConfig::default();
                            account.bot_token = Some(String::new());
                            let mut accounts = HashMap::new();
                            accounts.insert("default".to_string(), account);
                            ch.accounts = Some(accounts);
                        }

                        channels.insert(name, ch);
                        state.new_channel_name.clear();
                        state.create_section_open = false;
                    }

                    if !valid && !state.new_channel_name.trim().is_empty() && exists {
                        ui.label(
                            egui::RichText::new("Name already exists")
                                .color(theme::YELLOW)
                                .small(),
                        );
                    }
                });
            });
    }

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
        return action;
    };

    if channels.is_empty() {
        ui.label(
            egui::RichText::new("No channels configured. Add one above.")
                .color(theme::OVERLAY)
                .italics(),
        );
        return action;
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

                        // Right side: logs + edit + delete buttons
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

                            if ui.small_button("Logs").clicked() {
                                action = ChannelAction::ViewLogs(key.clone());
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

    action
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
