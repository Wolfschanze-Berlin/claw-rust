//! Config section editor with tabbed layout.
//!
//! Provides dedicated forms for gateway, models, session, commands,
//! agent defaults, and a raw JSON view for the entire config.

use eframe::egui;
use std::collections::HashMap;

use claw_config::{
    AgentDefaults, BindMode, CommandsConfig, ControlUiConfig, GatewayAuthConfig, GatewayConfig,
    GatewayToolsConfig, ModelsConfig, OpenClawConfig, RateLimitConfig, RemoteConfig,
    SessionConfig, SubagentDefaults, TailscaleConfig, ToggleConfig, TrustedProxyConfig,
    ValidationResult,
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
    pub auth_password_visible: bool,
    pub remote_token_visible: bool,
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
            auth_password_visible: false,
            remote_token_visible: false,
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
        BindMode::Auto => "auto",
        BindMode::Localhost => "localhost",
        BindMode::Loopback => "loopback",
        BindMode::Lan => "lan",
        BindMode::Tailnet => "tailnet",
        BindMode::All => "all",
        BindMode::Custom => "custom",
    }
}

fn str_to_bind_mode(s: &str) -> Option<BindMode> {
    match s {
        "auto" => Some(BindMode::Auto),
        "localhost" => Some(BindMode::Localhost),
        "loopback" => Some(BindMode::Loopback),
        "lan" => Some(BindMode::Lan),
        "tailnet" => Some(BindMode::Tailnet),
        "all" => Some(BindMode::All),
        "custom" => Some(BindMode::Custom),
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

    // -- Core settings --------------------------------------------------
    ui.horizontal(|ui| {
        changed |= form_field::number_field_u16(ui, "Port", &mut gw.port, 3000);
        validation_badge::validation_badge(ui, "gateway.port", validation);
    });

    let mut bind_str = gw.bind.as_ref().map(|b| bind_mode_to_str(b).to_string());
    if form_field::select_field(
        ui,
        "Bind",
        &mut bind_str,
        &["auto", "localhost", "loopback", "lan", "tailnet", "all", "custom"],
    ) {
        gw.bind = bind_str.as_deref().and_then(str_to_bind_mode);
        changed = true;
    }

    changed |= form_field::text_field(ui, "Mode", &mut gw.mode);

    ui.add_space(8.0);

    // -- Control UI section ---------------------------------------------
    egui::CollapsingHeader::new(
        egui::RichText::new("Control UI").color(theme::TEXT).strong(),
    )
    .id_salt("gateway_control_ui")
    .default_open(false)
    .show(ui, |ui| {
        // Legacy flat boolean
        changed |= form_field::bool_field(ui, "Enabled (legacy)", &mut gw.control_ui_enabled);

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Nested control_ui config (overrides legacy field):")
                .color(theme::OVERLAY)
                .small()
                .italics(),
        );

        let cui = gw.control_ui.get_or_insert_with(ControlUiConfig::default);
        changed |= form_field::bool_field(ui, "Enabled", &mut cui.enabled);
        changed |= form_field::text_field(ui, "Base path", &mut cui.base_path);
    });

    ui.add_space(4.0);

    // -- Authentication section -----------------------------------------
    egui::CollapsingHeader::new(
        egui::RichText::new("Authentication").color(theme::TEXT).strong(),
    )
    .id_salt("gateway_auth")
    .default_open(false)
    .show(ui, |ui| {
        let auth = gw.auth.get_or_insert_with(GatewayAuthConfig::default);

        changed |= form_field::select_field(
            ui,
            "Auth mode",
            &mut auth.mode,
            &["none", "token", "password"],
        );

        // Token field — shown when mode is "token"
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

        // Password field — shown when mode is "password"
        if auth.mode.as_deref() == Some("password") {
            changed |= form_field::secret_field(
                ui,
                "Password",
                &mut auth.password,
                &mut state.auth_password_visible,
            );
        }

        changed |= form_field::bool_field(ui, "Allow Tailscale", &mut auth.allow_tailscale);

        // Trusted proxy sub-section
        ui.add_space(4.0);
        egui::CollapsingHeader::new(
            egui::RichText::new("Trusted Proxy").color(theme::SUBTEXT),
        )
        .id_salt("gateway_auth_trusted_proxy")
        .default_open(false)
        .show(ui, |ui| {
            let tp = auth
                .trusted_proxy
                .get_or_insert_with(TrustedProxyConfig::default);
            changed |= form_field::text_field(ui, "User header", &mut tp.user_header);
        });

        // Rate limit sub-section
        ui.add_space(4.0);
        egui::CollapsingHeader::new(
            egui::RichText::new("Rate Limit").color(theme::SUBTEXT),
        )
        .id_salt("gateway_auth_rate_limit")
        .default_open(false)
        .show(ui, |ui| {
            let rl = auth
                .rate_limit
                .get_or_insert_with(RateLimitConfig::default);
            changed |= form_field::number_field_u32(ui, "Max attempts", &mut rl.max_attempts);
            changed |= form_field::number_field_u64(ui, "Window (ms)", &mut rl.window_ms);
            changed |= form_field::number_field_u64(ui, "Lockout (ms)", &mut rl.lockout_ms);
            changed |= form_field::bool_field(ui, "Exempt loopback", &mut rl.exempt_loopback);
        });
    });

    ui.add_space(4.0);

    // -- Tailscale section ----------------------------------------------
    egui::CollapsingHeader::new(
        egui::RichText::new("Tailscale").color(theme::TEXT).strong(),
    )
    .id_salt("gateway_tailscale")
    .default_open(false)
    .show(ui, |ui| {
        let ts = gw.tailscale.get_or_insert_with(TailscaleConfig::default);
        changed |= form_field::select_field(
            ui,
            "Mode",
            &mut ts.mode,
            &["off", "serve", "funnel"],
        );
        changed |= form_field::bool_field(ui, "Reset on exit", &mut ts.reset_on_exit);
    });

    ui.add_space(4.0);

    // -- Remote section -------------------------------------------------
    egui::CollapsingHeader::new(
        egui::RichText::new("Remote").color(theme::TEXT).strong(),
    )
    .id_salt("gateway_remote")
    .default_open(false)
    .show(ui, |ui| {
        let remote = gw.remote.get_or_insert_with(RemoteConfig::default);
        changed |= form_field::text_field(ui, "URL", &mut remote.url);
        changed |= form_field::select_field(
            ui,
            "Transport",
            &mut remote.transport,
            &["ssh", "direct"],
        );
        changed |= form_field::secret_field(
            ui,
            "Token",
            &mut remote.token,
            &mut state.remote_token_visible,
        );
    });

    ui.add_space(4.0);

    // -- Trusted Proxies ------------------------------------------------
    egui::CollapsingHeader::new(
        egui::RichText::new("Trusted Proxies").color(theme::TEXT).strong(),
    )
    .id_salt("gateway_trusted_proxies")
    .default_open(false)
    .show(ui, |ui| {
        changed |= form_field::string_list_field(
            ui,
            "Proxy addresses",
            &mut gw.trusted_proxies,
        );
    });

    ui.add_space(4.0);

    // -- Gateway Tools --------------------------------------------------
    egui::CollapsingHeader::new(
        egui::RichText::new("Tools (allow/deny)").color(theme::TEXT).strong(),
    )
    .id_salt("gateway_tools")
    .default_open(false)
    .show(ui, |ui| {
        let tools = gw.tools.get_or_insert_with(GatewayToolsConfig::default);
        changed |= form_field::string_list_field(ui, "Allow", &mut tools.allow);
        ui.add_space(4.0);
        changed |= form_field::string_list_field(ui, "Deny", &mut tools.deny);
    });

    ui.add_space(8.0);

    // -- Extra fields as JSON -------------------------------------------
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
