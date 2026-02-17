//! Skills CRUD list view backed by ConfigManager draft.
//!
//! Shows all configured skills with create, delete, and inline
//! editing. Skills are keyed by string name (like channels, unlike
//! agents which are Vec-indexed).

use std::collections::HashMap;

use eframe::egui;

use claw_config::{SkillEntry, SkillsConfig};

use crate::config_manager::ConfigManager;
use crate::theme;
use crate::widgets::{form_field, json_view};

// ---------------------------------------------------------------------------
// Persistent UI state for the skills view
// ---------------------------------------------------------------------------

/// UI state that persists across frames for the skills list.
pub struct SkillsViewState {
    /// Currently selected skill key (if any).
    pub selected: Option<String>,
    /// Text buffer for the "new skill name" input.
    pub new_skill_name: String,
    /// Detail form state for the selected skill.
    pub detail_state: Option<SkillDetailState>,
    /// Whether we've already taken an undo snapshot for this detail session.
    detail_snapshot_taken: bool,
}

impl Default for SkillsViewState {
    fn default() -> Self {
        Self {
            selected: None,
            new_skill_name: String::new(),
            detail_state: None,
            detail_snapshot_taken: false,
        }
    }
}

/// Persistent UI state for the skill detail form.
pub struct SkillDetailState {
    /// Which skill key is being edited.
    pub skill_key: String,
    /// Secret field visibility toggle for the API key.
    pub show_api_key: bool,
    /// JSON edit buffer for skill extra fields.
    pub extra_json: String,
    /// Whether the extra JSON editor is open.
    pub extra_editing: bool,
    /// Whether to show the "delete skill" confirmation dialog.
    pub confirm_delete: bool,
}

impl SkillDetailState {
    fn new(skill_key: String) -> Self {
        Self {
            skill_key,
            show_api_key: false,
            extra_json: String::new(),
            extra_editing: false,
            confirm_delete: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Render the skills view (list or detail, depending on selection).
pub fn show(ui: &mut egui::Ui, config: &mut ConfigManager, state: &mut SkillsViewState) {
    if state.selected.is_some() {
        show_detail(ui, config, state);
    } else {
        show_list(ui, config, state);
    }
}

// ---------------------------------------------------------------------------
// List view
// ---------------------------------------------------------------------------

fn show_list(ui: &mut egui::Ui, config: &mut ConfigManager, state: &mut SkillsViewState) {
    // Header
    ui.horizontal(|ui| {
        ui.heading("Skills");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let count = config
                .draft()
                .skills
                .as_ref()
                .and_then(|s| s.entries.as_ref())
                .map_or(0, |m| m.len());
            ui.label(
                egui::RichText::new(format!("{count} configured"))
                    .color(theme::SUBTEXT)
                    .small(),
            );
        });
    });
    ui.add_space(8.0);

    // Top-level skills settings (load.watch, install preferences)
    show_skills_settings(ui, config);

    ui.add_space(8.0);

    // Add skill row
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("New skill:").color(theme::SUBTEXT));
        ui.text_edit_singleline(&mut state.new_skill_name);

        let name = state.new_skill_name.trim().to_lowercase();
        let exists = config
            .draft()
            .skills
            .as_ref()
            .and_then(|s| s.entries.as_ref())
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
            let skills = config.draft_mut().skills.get_or_insert_with(SkillsConfig::default);
            let entries = skills.entries.get_or_insert_with(HashMap::new);
            entries.insert(
                name,
                SkillEntry {
                    enabled: Some(true),
                    ..Default::default()
                },
            );
            state.new_skill_name.clear();
        }
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Skill cards
    let entries = config
        .draft()
        .skills
        .as_ref()
        .and_then(|s| s.entries.clone());
    let Some(entries) = entries.as_ref() else {
        ui.label(
            egui::RichText::new("No skills configured. Add one above.")
                .color(theme::OVERLAY)
                .italics(),
        );
        return;
    };

    if entries.is_empty() {
        ui.label(
            egui::RichText::new("No skills configured. Add one above.")
                .color(theme::OVERLAY)
                .italics(),
        );
        return;
    }

    // Sort by name for stable ordering
    let mut keys: Vec<&String> = entries.keys().collect();
    keys.sort();

    let mut to_delete: Option<String> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        for key in keys {
            let skill = &entries[key];
            let enabled = skill.enabled.unwrap_or(true);
            let has_api_key = skill.api_key.is_some();

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

                        // Skill info (clickable to select)
                        let response = ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(key)
                                    .size(16.0)
                                    .color(theme::TEXT),
                            );
                            let mut subtitle_parts = Vec::new();
                            subtitle_parts.push(
                                if enabled { "enabled" } else { "disabled" }.to_string(),
                            );
                            if has_api_key {
                                subtitle_parts.push("has API key".to_string());
                            }
                            let extra_count = skill.extra.len();
                            if extra_count > 0 {
                                subtitle_parts
                                    .push(format!("{extra_count} extra field{}", if extra_count == 1 { "" } else { "s" }));
                            }
                            ui.label(
                                egui::RichText::new(subtitle_parts.join(" \u{00b7} "))
                                    .small()
                                    .color(theme::SUBTEXT),
                            );
                        });

                        if response.response.interact(egui::Sense::click()).clicked() {
                            state.selected = Some(key.clone());
                            state.detail_state = Some(SkillDetailState::new(key.clone()));
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
                                to_delete = Some(key.clone());
                            }

                            if ui.small_button("Edit").clicked() {
                                state.selected = Some(key.clone());
                                state.detail_state = Some(SkillDetailState::new(key.clone()));
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
        if let Some(skills) = config.draft_mut().skills.as_mut() {
            if let Some(entries) = skills.entries.as_mut() {
                entries.remove(&key);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Skills settings (collapsible top-level config)
// ---------------------------------------------------------------------------

fn show_skills_settings(ui: &mut egui::Ui, config: &mut ConfigManager) {
    egui::CollapsingHeader::new(
        egui::RichText::new("Skills Settings")
            .color(theme::TEXT)
            .strong(),
    )
    .id_salt("skills_settings")
    .default_open(false)
    .show(ui, |ui| {
        let skills = config
            .draft_mut()
            .skills
            .get_or_insert_with(SkillsConfig::default);

        // Load settings
        ui.label(egui::RichText::new("Loading").color(theme::SUBTEXT).small());
        let load = skills.load.get_or_insert_with(Default::default);
        form_field::bool_field(ui, "Watch for changes", &mut load.watch);

        ui.add_space(4.0);

        // Install settings
        ui.label(
            egui::RichText::new("Installation")
                .color(theme::SUBTEXT)
                .small(),
        );
        let install = skills.install.get_or_insert_with(Default::default);
        form_field::bool_field(ui, "Prefer Homebrew", &mut install.prefer_brew);
        form_field::text_field(ui, "Node manager", &mut install.node_manager);
    });
}

// ---------------------------------------------------------------------------
// Detail view
// ---------------------------------------------------------------------------

fn show_detail(ui: &mut egui::Ui, config: &mut ConfigManager, state: &mut SkillsViewState) {
    let Some(selected_key) = state.selected.clone() else {
        state.detail_state = None;
        return;
    };

    // Ensure detail state exists
    if state.detail_state.is_none() {
        state.detail_state = Some(SkillDetailState::new(selected_key.clone()));
    }

    let has_skill = config
        .draft()
        .skills
        .as_ref()
        .and_then(|s| s.entries.as_ref())
        .is_some_and(|m| m.contains_key(&selected_key));

    if !has_skill {
        state.selected = None;
        state.detail_state = None;
        return;
    }

    // Snapshot once when first entering detail view
    if !state.detail_snapshot_taken {
        config.begin_edit();
        state.detail_snapshot_taken = true;
    }

    let detail = state.detail_state.as_mut().unwrap();
    let mut action = DetailAction::None;

    // -- Header row: back button, title, delete button -----------------------
    ui.horizontal(|ui| {
        if ui.button("\u{2190} Back").clicked() {
            action = DetailAction::Back;
        }
        ui.heading(
            egui::RichText::new(format!("Skill: {}", selected_key))
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
                    egui::RichText::new("Delete Skill").color(theme::RED),
                ))
                .clicked()
            {
                detail.confirm_delete = true;
            }
        });
    });

    ui.separator();

    // Get mutable reference to the skill entry
    let skill = config
        .draft_mut()
        .skills
        .as_mut()
        .unwrap()
        .entries
        .as_mut()
        .unwrap()
        .get_mut(&selected_key)
        .unwrap();

    // -- Scrollable form body ------------------------------------------------
    egui::ScrollArea::vertical().show(ui, |ui| {
        // ---- Basic Settings ----
        egui::CollapsingHeader::new(
            egui::RichText::new("Basic Settings")
                .color(theme::TEXT)
                .strong(),
        )
        .id_salt("skill_basic")
        .default_open(true)
        .show(ui, |ui| {
            form_field::bool_field(ui, "Enabled", &mut skill.enabled);
            form_field::secret_field(
                ui,
                "API Key",
                &mut skill.api_key,
                &mut detail.show_api_key,
            );
        });

        // ---- Extra Fields ----
        egui::CollapsingHeader::new(
            egui::RichText::new("Extra Fields")
                .color(theme::TEXT)
                .strong(),
        )
        .id_salt("skill_extra")
        .default_open(false)
        .show(ui, |ui| {
            let mut extra_value = serde_json::to_value(&skill.extra).unwrap_or_default();
            if json_view::json_field(
                ui,
                "Skill Extra",
                &mut extra_value,
                &mut detail.extra_json,
                &mut detail.extra_editing,
            ) {
                if let Ok(map) =
                    serde_json::from_value::<HashMap<String, serde_json::Value>>(extra_value)
                {
                    skill.extra = map;
                }
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
            if let Some(skills) = config.draft_mut().skills.as_mut() {
                if let Some(entries) = skills.entries.as_mut() {
                    entries.remove(&selected_key);
                }
            }
            state.selected = None;
            state.detail_state = None;
            state.detail_snapshot_taken = false;
        }
        DetailAction::None => {}
    }
}

/// Actions the detail form can request from its parent.
enum DetailAction {
    None,
    Back,
    Delete,
}
