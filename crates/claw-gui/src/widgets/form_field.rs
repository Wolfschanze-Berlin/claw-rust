//! Reusable form field widgets for config editing.
//!
//! Each widget handles `Option<T>` ergonomically and returns `true`
//! when the value was modified, allowing callers to track dirty state.

use eframe::egui;

use crate::theme;

/// Render a labeled text input field for `Option<String>`.
///
/// When the value is `None`, shows a placeholder with an "Enable" button.
/// When `Some`, shows the text input with a clear button.
/// Returns `true` if the value changed.
pub fn text_field(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(theme::SUBTEXT));

        match value {
            Some(text) => {
                let response = ui.text_edit_singleline(text);
                if response.changed() {
                    changed = true;
                }
                if ui
                    .add(egui::Button::new(egui::RichText::new("\u{00d7}").color(theme::RED)).small())
                    .on_hover_text("Clear")
                    .clicked()
                {
                    *value = None;
                    changed = true;
                }
            }
            None => {
                ui.label(egui::RichText::new("Not set").color(theme::OVERLAY).italics());
                if ui.small_button("Enable").clicked() {
                    *value = Some(String::new());
                    changed = true;
                }
            }
        }
    });

    changed
}

/// Render a labeled secret/password field for `Option<String>`.
///
/// Like [`text_field`] but with password masking and a show/hide toggle.
/// The `show` flag persists visibility state across frames.
pub fn secret_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<String>,
    show: &mut bool,
) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(theme::SUBTEXT));

        match value {
            Some(text) => {
                let response =
                    ui.add(egui::TextEdit::singleline(text).password(!*show));
                if response.changed() {
                    changed = true;
                }

                let eye = if *show { "\u{1f441}" } else { "\u{1f441}\u{200d}\u{1f5e8}" };
                if ui.small_button(eye).on_hover_text("Toggle visibility").clicked() {
                    *show = !*show;
                }

                if ui
                    .add(egui::Button::new(egui::RichText::new("\u{00d7}").color(theme::RED)).small())
                    .on_hover_text("Clear")
                    .clicked()
                {
                    *value = None;
                    changed = true;
                }
            }
            None => {
                ui.label(egui::RichText::new("Not set").color(theme::OVERLAY).italics());
                if ui.small_button("Enable").clicked() {
                    *value = Some(String::new());
                    changed = true;
                }
            }
        }
    });

    changed
}

/// Render a labeled boolean toggle for `Option<bool>`.
///
/// `None` renders as an unchecked box with a "(default)" hint.
/// Clicking transitions to `Some(true)` / `Some(false)`.
pub fn bool_field(ui: &mut egui::Ui, label: &str, value: &mut Option<bool>) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(theme::SUBTEXT));

        let mut checked = value.unwrap_or(false);
        if ui.checkbox(&mut checked, "").changed() {
            *value = Some(checked);
            changed = true;
        }

        if value.is_none() {
            ui.label(egui::RichText::new("(default)").color(theme::OVERLAY).small());
        } else if ui.small_button("Reset").on_hover_text("Reset to default").clicked() {
            *value = None;
            changed = true;
        }
    });

    changed
}

/// Render a labeled number input for `Option<u16>`.
///
/// Uses a [`egui::DragValue`] clamped to `0..=65535`.
pub fn number_field_u16(ui: &mut egui::Ui, label: &str, value: &mut Option<u16>) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(theme::SUBTEXT));

        match value {
            Some(n) => {
                let mut val = *n as f64;
                let response =
                    ui.add(egui::DragValue::new(&mut val).range(0.0..=65535.0).speed(1.0));
                if response.changed() {
                    *n = val as u16;
                    changed = true;
                }
                if ui
                    .add(egui::Button::new(egui::RichText::new("\u{00d7}").color(theme::RED)).small())
                    .on_hover_text("Clear")
                    .clicked()
                {
                    *value = None;
                    changed = true;
                }
            }
            None => {
                ui.label(egui::RichText::new("Not set").color(theme::OVERLAY).italics());
                if ui.small_button("Set").clicked() {
                    *value = Some(0);
                    changed = true;
                }
            }
        }
    });

    changed
}

/// Render a labeled number input for `Option<u32>`.
///
/// Uses a [`egui::DragValue`] clamped to `0..=u32::MAX`.
pub fn number_field_u32(ui: &mut egui::Ui, label: &str, value: &mut Option<u32>) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(theme::SUBTEXT));

        match value {
            Some(n) => {
                let mut val = *n as f64;
                let response = ui.add(
                    egui::DragValue::new(&mut val)
                        .range(0.0..=u32::MAX as f64)
                        .speed(1.0),
                );
                if response.changed() {
                    *n = val as u32;
                    changed = true;
                }
                if ui
                    .add(egui::Button::new(egui::RichText::new("\u{00d7}").color(theme::RED)).small())
                    .on_hover_text("Clear")
                    .clicked()
                {
                    *value = None;
                    changed = true;
                }
            }
            None => {
                ui.label(egui::RichText::new("Not set").color(theme::OVERLAY).italics());
                if ui.small_button("Set").clicked() {
                    *value = Some(0);
                    changed = true;
                }
            }
        }
    });

    changed
}

/// Render a labeled dropdown/select for `Option<String>` from fixed options.
///
/// Includes a "None (default)" entry at the top of the list to reset
/// the value back to `None`.
pub fn select_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<String>,
    options: &[&str],
) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(theme::SUBTEXT));

        let current = value.as_deref().unwrap_or("None (default)");
        egui::ComboBox::from_id_salt(label)
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(&mut *value, None, "None (default)")
                    .changed()
                {
                    changed = true;
                }
                for opt in options {
                    let opt_val = Some((*opt).to_string());
                    if ui
                        .selectable_value(value, opt_val, *opt)
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
    });

    changed
}

/// Render a collapsible section header.
///
/// Uses [`egui::CollapsingHeader`] with themed styling.
/// Returns `true` if the section is currently expanded.
pub fn section_header(ui: &mut egui::Ui, title: &str, id: &str) -> bool {
    egui::CollapsingHeader::new(egui::RichText::new(title).color(theme::TEXT).strong())
        .id_salt(id)
        .default_open(false)
        .show(ui, |_| {})
        .body_returned
        .is_some()
}

/// Render a string list editor (add/remove items).
///
/// Used for fields like `allow_from: Vec<String>`. Shows each string as
/// a row with a delete button, plus an "Add" button with a text input.
pub fn string_list_field(
    ui: &mut egui::Ui,
    label: &str,
    values: &mut Option<Vec<String>>,
) -> bool {
    let mut changed = false;

    ui.label(egui::RichText::new(label).color(theme::SUBTEXT));

    match values {
        Some(list) => {
            let mut to_remove: Option<usize> = None;

            for (i, item) in list.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let response = ui.text_edit_singleline(item);
                    if response.changed() {
                        changed = true;
                    }
                    if ui
                        .add(egui::Button::new(egui::RichText::new("\u{00d7}").color(theme::RED)).small())
                        .on_hover_text("Remove")
                        .clicked()
                    {
                        to_remove = Some(i);
                    }
                });
            }

            if let Some(idx) = to_remove {
                list.remove(idx);
                changed = true;
                // Reset to None if last item removed
                if list.is_empty() {
                    *values = None;
                    return changed;
                }
            }

            if ui
                .add(egui::Button::new(egui::RichText::new("+ Add").color(theme::BLUE)).small())
                .clicked()
            {
                list.push(String::new());
                changed = true;
            }
        }
        None => {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Empty").color(theme::OVERLAY).italics());
                if ui.small_button("Add first").clicked() {
                    *values = Some(vec![String::new()]);
                    changed = true;
                }
            });
        }
    }

    changed
}
