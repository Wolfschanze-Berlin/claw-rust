//! Config section editor with tabbed layout.
//!
//! Provides dedicated forms for gateway, models, session, commands,
//! agent defaults, and a raw JSON view for the entire config.

use eframe::egui;
use std::collections::HashMap;

use claw_config::{
    AgentDefaults, BindMode, CommandsConfig, GatewayAuthConfig, GatewayConfig, ModelsConfig,
    OpenClawConfig, SessionConfig, SubagentDefaults, ToggleConfig, ValidationResult,
};

use crate::theme;
use crate::widgets::{form_field, json_view, validation_badge};

/// Which config section tab is active.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfigTab {
    Gateway,
    Models,
    Session,
    Commands,
    AgentDefaults,
    RawConfig,
}

const ALL_TABS: &[(ConfigTab, &str)] = &[
    (ConfigTab::Gateway, "Gateway"),
    (ConfigTab::Models, "Models"),
    (ConfigTab::Session, "Session"),
    (ConfigTab::Commands, "Commands"),
    (ConfigTab::AgentDefaults, "Agent Defaults"),
    (ConfigTab::RawConfig, "Raw Config"),
];

/// Persistent UI state for the config editor.
pub struct ConfigEditorState {
    pub active_tab: ConfigTab,
    // Gateway
    pub gateway_extra_json: String,
    pub gateway_extra_editing: bool,
    pub auth_token_visible: bool,
    // Agent defaults
    pub agent_defaults_extra_json: String,
    pub agent_defaults_extra_editing: bool,
    // Raw config
    pub raw_config_json: String,
    pub raw_config_editing: bool,
    // Model provider edit state
    pub provider_extra_json: HashMap<String, String>,
    pub provider_extra_editing: HashMap<String, bool>,
    // Provider expand state
    pub provider_expanded: HashMap<String, bool>,
}

impl Default for ConfigEditorState {
    fn default() -> Self {
        Self {
            active_tab: ConfigTab::Gateway,
            gateway_extra_json: String::new(),
            gateway_extra_editing: false,
            auth_token_visible: false,
            agent_defaults_extra_json: String::new(),
            agent_defaults_extra_editing: false,
            raw_config_json: String::new(),
            raw_config_editing: false,
            provider_extra_json: HashMap::new(),
            provider_extra_editing: HashMap::new(),
            provider_expanded: HashMap::new(),
        }
    }
}

/// Render the config section editor.
///
/// Returns `true` if any config value changed.
pub fn show(
    ui: &mut egui::Ui,
    config: &mut OpenClawConfig,
    state: &mut ConfigEditorState,
    validation: &ValidationResult,
) -> bool {
    let mut changed = false;

    // Validation summary at top
    validation_badge::validation_summary_bar(ui, validation);
    ui.add_space(8.0);

    // Tab bar
    ui.horizontal(|ui| {
        for (tab, label) in ALL_TABS {
            if ui
                .selectable_label(state.active_tab == *tab, *label)
                .clicked()
            {
                state.active_tab = *tab;
            }
        }
    });
    ui.separator();
    ui.add_space(4.0);

    // Tab content in scroll area
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            match state.active_tab {
                ConfigTab::Gateway => {
                    changed |= show_gateway(ui, config, state, validation);
                }
                ConfigTab::Models => {
                    changed |= show_models(ui, config, state);
                }
                ConfigTab::Session => {
                    changed |= show_session(ui, config);
                }
                ConfigTab::Commands => {
                    changed |= show_commands(ui, config);
                }
                ConfigTab::AgentDefaults => {
                    changed |= show_agent_defaults(ui, config, state);
                }
                ConfigTab::RawConfig => {
                    changed |= show_raw_config(ui, config, state);
                }
            }
        });

    changed
}

// ---------------------------------------------------------------------------
// BindMode helpers
// ---------------------------------------------------------------------------

fn bind_mode_to_str(mode: &BindMode) -> &'static str {
    match mode {
        BindMode::Localhost => "localhost",
        BindMode::Lan => "lan",
        BindMode::All => "all",
    }
}

fn str_to_bind_mode(s: &str) -> Option<BindMode> {
    match s {
        "localhost" => Some(BindMode::Localhost),
        "lan" => Some(BindMode::Lan),
        "all" => Some(BindMode::All),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Gateway tab
// ---------------------------------------------------------------------------

fn show_gateway(
    ui: &mut egui::Ui,
    config: &mut OpenClawConfig,
    state: &mut ConfigEditorState,
    validation: &ValidationResult,
) -> bool {
    let mut changed = false;
    let gw = config.gateway.get_or_insert_with(GatewayConfig::default);

    ui.heading("Gateway");
    ui.add_space(4.0);

    // Port
    ui.horizontal(|ui| {
        changed |= form_field::number_field_u16(ui, "Port", &mut gw.port);
        validation_badge::validation_badge(ui, "gateway.port", validation);
    });

    // Bind mode — convert between BindMode and Option<String> for select_field
    let mut bind_str = gw.bind.as_ref().map(|b| bind_mode_to_str(b).to_string());
    if form_field::select_field(ui, "Bind", &mut bind_str, &["localhost", "lan", "all"]) {
        gw.bind = bind_str.as_deref().and_then(str_to_bind_mode);
        changed = true;
    }

    // Mode
    changed |= form_field::text_field(ui, "Mode", &mut gw.mode);

    // Control UI
    changed |= form_field::bool_field(ui, "Control UI enabled", &mut gw.control_ui_enabled);

    ui.add_space(8.0);

    // Auth section (collapsible)
    egui::CollapsingHeader::new(
        egui::RichText::new("Authentication").color(theme::TEXT).strong(),
    )
    .id_salt("gateway_auth")
    .default_open(false)
    .show(ui, |ui| {
        let auth = gw.auth.get_or_insert_with(GatewayAuthConfig::default);

        changed |= form_field::select_field(ui, "Auth mode", &mut auth.mode, &["none", "token"]);

        // Only show token field when mode is "token"
        if auth.mode.as_deref() == Some("token") {
            ui.horizontal(|ui| {
                changed |= form_field::secret_field(
                    ui,
                    "Token",
                    &mut auth.token,
                    &mut state.auth_token_visible,
                );
                validation_badge::validation_badge(ui, "gateway.auth.token", validation);
            });
        }
    });

    ui.add_space(8.0);

    // Extra fields as JSON
    let mut extra_val = serde_json::to_value(&gw.extra).unwrap_or_default();
    if json_view::json_field(
        ui,
        "Extra fields",
        &mut extra_val,
        &mut state.gateway_extra_json,
        &mut state.gateway_extra_editing,
    ) {
        if let Ok(map) = serde_json::from_value::<HashMap<String, serde_json::Value>>(extra_val) {
            gw.extra = map;
            changed = true;
        }
    }

    changed
}

// ---------------------------------------------------------------------------
// Models tab
// ---------------------------------------------------------------------------

fn show_models(
    ui: &mut egui::Ui,
    config: &mut OpenClawConfig,
    state: &mut ConfigEditorState,
) -> bool {
    let mut changed = false;
    let models = config.models.get_or_insert_with(ModelsConfig::default);

    ui.heading("Models");
    ui.add_space(4.0);

    // Default model
    changed |= form_field::text_field(ui, "Default model", &mut models.default);
    ui.add_space(8.0);

    // Providers
    if let Some(providers) = &mut models.providers {
        // Collect keys to avoid borrow issues
        let keys: Vec<String> = providers.keys().cloned().collect();

        for key in &keys {
            if let Some(provider) = providers.get_mut(key) {
                let expanded = state.provider_expanded.entry(key.clone()).or_insert(false);

                egui::CollapsingHeader::new(
                    egui::RichText::new(key).color(theme::BLUE).strong(),
                )
                .id_salt(format!("provider_{key}"))
                .open(if *expanded { Some(true) } else { None })
                .show(ui, |ui| {
                    *expanded = true;
                    changed |= form_field::text_field(ui, "Base URL", &mut provider.base_url);
                    changed |= form_field::text_field(ui, "Auth", &mut provider.auth);
                    changed |= form_field::text_field(ui, "API", &mut provider.api);

                    // Model entries
                    if let Some(model_list) = &provider.models {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(format!("Models ({})", model_list.len()))
                                .color(theme::SUBTEXT),
                        );
                        for entry in model_list {
                            let id = entry.id.as_deref().unwrap_or("?");
                            let name = entry.name.as_deref().unwrap_or("");
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(id).color(theme::TEXT));
                                if !name.is_empty() && name != id {
                                    ui.label(
                                        egui::RichText::new(format!("({name})"))
                                            .color(theme::OVERLAY),
                                    );
                                }
                            });
                        }
                    }

                    // Provider extra
                    let buf = state
                        .provider_extra_json
                        .entry(key.clone())
                        .or_default();
                    let editing = state
                        .provider_extra_editing
                        .entry(key.clone())
                        .or_insert(false);
                    let mut extra_val =
                        serde_json::to_value(&provider.extra).unwrap_or_default();
                    if json_view::json_field(
                        ui,
                        "Extra fields",
                        &mut extra_val,
                        buf,
                        editing,
                    ) {
                        if let Ok(map) = serde_json::from_value::<
                            HashMap<String, serde_json::Value>,
                        >(extra_val)
                        {
                            provider.extra = map;
                            changed = true;
                        }
                    }
                });
            }
        }
    } else {
        ui.label(egui::RichText::new("No providers configured").color(theme::OVERLAY).italics());
    }

    changed
}

// ---------------------------------------------------------------------------
// Session tab
// ---------------------------------------------------------------------------

fn show_session(ui: &mut egui::Ui, config: &mut OpenClawConfig) -> bool {
    let mut changed = false;
    let session = config.session.get_or_insert_with(SessionConfig::default);

    ui.heading("Session");
    ui.add_space(4.0);

    changed |= form_field::text_field(ui, "TTL", &mut session.ttl);
    changed |= form_field::number_field_u32(ui, "Max history", &mut session.max_history);

    changed
}

// ---------------------------------------------------------------------------
// Commands tab
// ---------------------------------------------------------------------------

fn show_commands(ui: &mut egui::Ui, config: &mut OpenClawConfig) -> bool {
    let mut changed = false;
    let cmds = config.commands.get_or_insert_with(CommandsConfig::default);

    ui.heading("Commands");
    ui.add_space(4.0);

    changed |= form_field::bool_field(ui, "Native", &mut cmds.native);
    changed |= form_field::bool_field(ui, "Native skills", &mut cmds.native_skills);
    changed |= form_field::bool_field(ui, "Bash", &mut cmds.bash);
    changed |= form_field::bool_field(ui, "Config", &mut cmds.config);
    changed |= form_field::bool_field(ui, "Debug", &mut cmds.debug);
    changed |= form_field::bool_field(ui, "Restart", &mut cmds.restart);

    changed
}

// ---------------------------------------------------------------------------
// Agent Defaults tab
// ---------------------------------------------------------------------------

fn show_agent_defaults(
    ui: &mut egui::Ui,
    config: &mut OpenClawConfig,
    state: &mut ConfigEditorState,
) -> bool {
    let mut changed = false;

    // Ensure agents.defaults exists
    let agents = config.agents.get_or_insert_with(Default::default);
    let defaults = agents.defaults.get_or_insert_with(AgentDefaults::default);

    ui.heading("Agent Defaults");
    ui.add_space(4.0);

    changed |= form_field::text_field(ui, "Time format", &mut defaults.time_format);
    changed |= form_field::text_field(ui, "Thinking default", &mut defaults.thinking_default);
    changed |= form_field::text_field(ui, "Verbose default", &mut defaults.verbose_default);
    changed |= form_field::text_field(ui, "Elevated default", &mut defaults.elevated_default);
    changed |= form_field::text_field(ui, "Typing mode", &mut defaults.typing_mode);
    changed |= form_field::number_field_u32(ui, "Max concurrent", &mut defaults.max_concurrent);

    ui.add_space(8.0);

    // Memory search toggle
    egui::CollapsingHeader::new(
        egui::RichText::new("Memory search").color(theme::TEXT).strong(),
    )
    .id_salt("agent_defaults_memory_search")
    .default_open(false)
    .show(ui, |ui| {
        let mem = defaults
            .memory_search
            .get_or_insert_with(ToggleConfig::default);
        changed |= form_field::bool_field(ui, "Enabled", &mut mem.enabled);
    });

    // Subagents
    egui::CollapsingHeader::new(
        egui::RichText::new("Subagents").color(theme::TEXT).strong(),
    )
    .id_salt("agent_defaults_subagents")
    .default_open(false)
    .show(ui, |ui| {
        let sub = defaults
            .subagents
            .get_or_insert_with(SubagentDefaults::default);
        changed |= form_field::number_field_u32(ui, "Max concurrent", &mut sub.max_concurrent);
    });

    ui.add_space(8.0);

    // Extra fields as JSON
    let mut extra_val = serde_json::to_value(&defaults.extra).unwrap_or_default();
    if json_view::json_field(
        ui,
        "Extra fields",
        &mut extra_val,
        &mut state.agent_defaults_extra_json,
        &mut state.agent_defaults_extra_editing,
    ) {
        if let Ok(map) = serde_json::from_value::<HashMap<String, serde_json::Value>>(extra_val) {
            defaults.extra = map;
            changed = true;
        }
    }

    changed
}

// ---------------------------------------------------------------------------
// Raw Config tab
// ---------------------------------------------------------------------------

fn show_raw_config(
    ui: &mut egui::Ui,
    config: &mut OpenClawConfig,
    state: &mut ConfigEditorState,
) -> bool {
    let mut changed = false;

    ui.heading("Raw Configuration");
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Edit the full config as JSON. Changes apply when you click Save.")
            .color(theme::SUBTEXT),
    );
    ui.add_space(4.0);

    let mut config_val = serde_json::to_value(&*config).unwrap_or_default();
    if json_view::json_field(
        ui,
        "Config JSON",
        &mut config_val,
        &mut state.raw_config_json,
        &mut state.raw_config_editing,
    ) {
        if let Ok(new_config) = serde_json::from_value::<OpenClawConfig>(config_val) {
            *config = new_config;
            changed = true;
        }
    }

    changed
}
