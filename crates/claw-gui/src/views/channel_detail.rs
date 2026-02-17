//! Channel detail/edit form with all ChannelConfig fields.
//!
//! Renders when a channel is selected from the channel list view.
//! Includes account management as an embedded sub-form (#54).

use eframe::egui;
use std::collections::HashMap;

use claw_config::{ChannelAccountConfig, ChannelConfig, ValidationResult};

use crate::theme;
use crate::widgets::{form_field, json_view, validation_badge};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Actions the detail form can request from its parent view.
pub enum DetailAction {
    /// No action needed.
    None,
    /// User clicked the back button — return to channel list.
    Back,
    /// User confirmed channel deletion.
    Delete,
}

/// Persistent UI state for the channel detail form.
///
/// Kept separate from `ChannelConfig` so transient UI concerns
/// (visibility toggles, edit buffers, confirmation dialogs) don't
/// pollute the serializable config data.
pub struct ChannelDetailState {
    /// Which channel key is being edited (e.g. "telegram").
    pub channel_key: String,
    /// Secret field visibility toggles per account (account_id -> show).
    pub secret_visibility: HashMap<String, bool>,
    /// JSON edit buffer for channel-level extra fields.
    pub channel_extra_json: String,
    /// Whether the channel extra JSON editor is open.
    pub channel_extra_editing: bool,
    /// Per-account JSON edit buffers.
    pub account_extra_json: HashMap<String, String>,
    /// Per-account JSON editor open state.
    pub account_extra_editing: HashMap<String, bool>,
    /// New account ID being typed in the "Add Account" input.
    pub new_account_id: String,
    /// Whether to show the "delete channel" confirmation dialog.
    pub confirm_delete: bool,
}

impl ChannelDetailState {
    /// Create fresh UI state for editing the given channel key.
    pub fn new(channel_key: String) -> Self {
        Self {
            channel_key,
            secret_visibility: HashMap::new(),
            channel_extra_json: String::new(),
            channel_extra_editing: false,
            account_extra_json: HashMap::new(),
            account_extra_editing: HashMap::new(),
            new_account_id: String::new(),
            confirm_delete: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Render the full channel detail form.
///
/// Returns a [`DetailAction`] indicating what the parent should do next
/// (navigate back, delete the channel, or nothing).
pub fn show(
    ui: &mut egui::Ui,
    channel_key: &str,
    config: &mut ChannelConfig,
    state: &mut ChannelDetailState,
    validation: &ValidationResult,
) -> DetailAction {
    let mut action = DetailAction::None;

    // -- Header row: back button, title, delete button -----------------------
    ui.horizontal(|ui| {
        if ui.button("\u{2190} Back").clicked() {
            action = DetailAction::Back;
        }
        ui.heading(
            egui::RichText::new(format!("Channel: {channel_key}"))
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
                    action = DetailAction::Delete;
                }
                if ui.button("Cancel").clicked() {
                    state.confirm_delete = false;
                }
            } else if ui
                .add(egui::Button::new(
                    egui::RichText::new("Delete Channel").color(theme::RED),
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
        let prefix = format!("channels.{channel_key}");

        // ---- Basic Settings ------------------------------------------------
        egui::CollapsingHeader::new(
            egui::RichText::new("Basic Settings").color(theme::TEXT).strong(),
        )
        .id_salt("ch_basic")
        .default_open(true)
        .show(ui, |ui| {
            show_basic_settings(ui, &prefix, config, validation);
        });

        // ---- Access Control ------------------------------------------------
        egui::CollapsingHeader::new(
            egui::RichText::new("Access Control").color(theme::TEXT).strong(),
        )
        .id_salt("ch_access")
        .default_open(false)
        .show(ui, |ui| {
            form_field::string_list_field(ui, "Allow From", &mut config.allow_from);
            ui.horizontal(|ui| {
                validation_badge::validation_badge(
                    ui,
                    &format!("{prefix}.allowFrom"),
                    validation,
                );
            });
        });

        // ---- Accounts (#54) ------------------------------------------------
        egui::CollapsingHeader::new(
            egui::RichText::new("Accounts").color(theme::TEXT).strong(),
        )
        .id_salt("ch_accounts")
        .default_open(true)
        .show(ui, |ui| {
            show_accounts(ui, channel_key, config, state, validation);
        });

        // ---- Extra Fields --------------------------------------------------
        egui::CollapsingHeader::new(
            egui::RichText::new("Extra Fields").color(theme::TEXT).strong(),
        )
        .id_salt("ch_extra")
        .default_open(false)
        .show(ui, |ui| {
            let mut extra_value = serde_json::to_value(&config.extra).unwrap_or_default();
            if json_view::json_field(
                ui,
                "Channel Extra",
                &mut extra_value,
                &mut state.channel_extra_json,
                &mut state.channel_extra_editing,
            ) {
                if let Ok(map) = serde_json::from_value::<HashMap<String, serde_json::Value>>(
                    extra_value,
                ) {
                    config.extra = map;
                }
            }
        });
    });

    action
}

// ---------------------------------------------------------------------------
// Basic Settings section
// ---------------------------------------------------------------------------

fn show_basic_settings(
    ui: &mut egui::Ui,
    prefix: &str,
    config: &mut ChannelConfig,
    validation: &ValidationResult,
) {
    // enabled
    ui.horizontal(|ui| {
        form_field::bool_field(ui, "Enabled", &mut config.enabled);
        validation_badge::validation_badge(ui, &format!("{prefix}.enabled"), validation);
    });

    // dm_policy
    ui.horizontal(|ui| {
        form_field::select_field(
            ui,
            "DM Policy",
            &mut config.dm_policy,
            &["pairing", "thread", "ignore"],
        );
        validation_badge::validation_badge(ui, &format!("{prefix}.dmPolicy"), validation);
    });

    // group_policy
    ui.horizontal(|ui| {
        form_field::select_field(
            ui,
            "Group Policy",
            &mut config.group_policy,
            &["thread", "mention", "all"],
        );
        validation_badge::validation_badge(ui, &format!("{prefix}.groupPolicy"), validation);
    });

    // stream_mode
    ui.horizontal(|ui| {
        form_field::select_field(
            ui,
            "Stream Mode",
            &mut config.stream_mode,
            &["streaming", "chunked", "off"],
        );
        validation_badge::validation_badge(ui, &format!("{prefix}.streamMode"), validation);
    });

    // reaction_level
    ui.horizontal(|ui| {
        form_field::select_field(
            ui,
            "Reaction Level",
            &mut config.reaction_level,
            &["full", "minimal", "off"],
        );
        validation_badge::validation_badge(
            ui,
            &format!("{prefix}.reactionLevel"),
            validation,
        );
    });

    // link_preview
    ui.horizontal(|ui| {
        form_field::bool_field(ui, "Link Preview", &mut config.link_preview);
        validation_badge::validation_badge(
            ui,
            &format!("{prefix}.linkPreview"),
            validation,
        );
    });
}

// ---------------------------------------------------------------------------
// Accounts sub-form (#54)
// ---------------------------------------------------------------------------

fn show_accounts(
    ui: &mut egui::Ui,
    channel_key: &str,
    config: &mut ChannelConfig,
    state: &mut ChannelDetailState,
    validation: &ValidationResult,
) {
    let accounts = config.accounts.get_or_insert_with(HashMap::new);
    let prefix = format!("channels.{channel_key}.accounts");

    if accounts.is_empty() {
        ui.label(egui::RichText::new("No accounts configured.").color(theme::OVERLAY).italics());
    }

    // Collect keys to iterate without borrowing accounts mutably twice.
    let keys: Vec<String> = accounts.keys().cloned().collect();
    let mut to_delete: Option<String> = None;

    for account_id in &keys {
        let Some(account) = accounts.get_mut(account_id) else {
            continue;
        };
        let acct_prefix = format!("{prefix}.{account_id}");

        egui::CollapsingHeader::new(
            egui::RichText::new(format!("Account: {account_id}")).color(theme::BLUE),
        )
        .id_salt(format!("acct_{account_id}"))
        .default_open(true)
        .show(ui, |ui| {
            // bot_token — secret field
            let show = state
                .secret_visibility
                .entry(account_id.clone())
                .or_insert(false);
            ui.horizontal(|ui| {
                form_field::secret_field(ui, "Bot Token", &mut account.bot_token, show);
                validation_badge::validation_badge(
                    ui,
                    &format!("{acct_prefix}.botToken"),
                    validation,
                );
            });

            // Account extra fields
            let json_buf = state
                .account_extra_json
                .entry(account_id.clone())
                .or_default();
            let editing = state
                .account_extra_editing
                .entry(account_id.clone())
                .or_insert(false);

            let mut extra_value =
                serde_json::to_value(&account.extra).unwrap_or_default();
            if json_view::json_field(
                ui,
                "Account Extra",
                &mut extra_value,
                json_buf,
                editing,
            ) {
                if let Ok(map) =
                    serde_json::from_value::<HashMap<String, serde_json::Value>>(extra_value)
                {
                    account.extra = map;
                }
            }

            // Delete account button
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(format!("Delete {account_id}")).color(theme::RED),
                    )
                    .small(),
                )
                .clicked()
            {
                to_delete = Some(account_id.clone());
            }
        });
    }

    // Apply deferred deletion outside the iteration.
    if let Some(id) = to_delete {
        accounts.remove(&id);
        state.secret_visibility.remove(&id);
        state.account_extra_json.remove(&id);
        state.account_extra_editing.remove(&id);
    }

    ui.separator();

    // -- Add Account --------------------------------------------------------
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("New Account ID:").color(theme::SUBTEXT));
        ui.text_edit_singleline(&mut state.new_account_id);
        let id_valid = !state.new_account_id.trim().is_empty()
            && !accounts.contains_key(state.new_account_id.trim());
        if ui
            .add_enabled(
                id_valid,
                egui::Button::new(egui::RichText::new("+ Add Account").color(theme::GREEN)),
            )
            .clicked()
        {
            let new_id = state.new_account_id.trim().to_string();
            accounts.insert(new_id, ChannelAccountConfig::default());
            state.new_account_id.clear();
        }
    });
}
