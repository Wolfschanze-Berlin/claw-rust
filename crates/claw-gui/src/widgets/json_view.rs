//! Raw JSON viewer and editor widget.
//!
//! Provides a recursive tree view with syntax coloring for read-only
//! display, a multiline editor with live parse validation, and a
//! combined viewer/editor toggle.

use eframe::egui;

use crate::theme;

/// Render a read-only JSON tree view with syntax coloring.
///
/// Objects and arrays are collapsible. Leaf values are colored by type:
/// strings in green, numbers in yellow, booleans in blue, and null in overlay.
pub fn json_viewer(ui: &mut egui::Ui, value: &serde_json::Value) {
    json_node(ui, None, value, 0);
}

/// Internal recursive renderer for a single JSON node.
fn json_node(ui: &mut egui::Ui, key: Option<&str>, value: &serde_json::Value, depth: usize) {
    let indent = depth;

    match value {
        serde_json::Value::Object(map) => {
            let header = match key {
                Some(k) => format!("{k}: {{{}}} ", map.len()),
                None => format!("{{{}}} ", map.len()),
            };
            egui::CollapsingHeader::new(egui::RichText::new(header).color(theme::TEXT))
                .id_salt(format!("json_obj_{:?}_{}", key, indent))
                .default_open(depth < 2)
                .show(ui, |ui| {
                    for (k, v) in map {
                        json_node(ui, Some(k), v, depth + 1);
                    }
                });
        }
        serde_json::Value::Array(arr) => {
            let header = match key {
                Some(k) => format!("{k}: [{}] ", arr.len()),
                None => format!("[{}] ", arr.len()),
            };
            egui::CollapsingHeader::new(egui::RichText::new(header).color(theme::TEXT))
                .id_salt(format!("json_arr_{:?}_{}", key, indent))
                .default_open(depth < 2)
                .show(ui, |ui| {
                    for (i, v) in arr.iter().enumerate() {
                        json_node(ui, Some(&i.to_string()), v, depth + 1);
                    }
                });
        }
        serde_json::Value::String(s) => {
            ui.horizontal(|ui| {
                if let Some(k) = key {
                    ui.label(egui::RichText::new(format!("{k}:")).color(theme::SUBTEXT));
                }
                ui.label(egui::RichText::new(format!("\"{s}\"")).color(theme::GREEN));
            });
        }
        serde_json::Value::Number(n) => {
            ui.horizontal(|ui| {
                if let Some(k) = key {
                    ui.label(egui::RichText::new(format!("{k}:")).color(theme::SUBTEXT));
                }
                ui.label(egui::RichText::new(n.to_string()).color(theme::YELLOW));
            });
        }
        serde_json::Value::Bool(b) => {
            ui.horizontal(|ui| {
                if let Some(k) = key {
                    ui.label(egui::RichText::new(format!("{k}:")).color(theme::SUBTEXT));
                }
                ui.label(egui::RichText::new(b.to_string()).color(theme::BLUE));
            });
        }
        serde_json::Value::Null => {
            ui.horizontal(|ui| {
                if let Some(k) = key {
                    ui.label(egui::RichText::new(format!("{k}:")).color(theme::SUBTEXT));
                }
                ui.label(egui::RichText::new("null").color(theme::OVERLAY));
            });
        }
    }
}

/// Render an editable JSON text area with live parse validation.
///
/// Returns `Some(Ok(value))` when valid JSON is entered and changed,
/// `Some(Err(msg))` when the text is invalid JSON, or `None` if
/// the text hasn't changed.
pub fn json_editor(
    ui: &mut egui::Ui,
    id: &str,
    text_buffer: &mut String,
) -> Option<Result<serde_json::Value, String>> {
    let response = ui.add(
        egui::TextEdit::multiline(text_buffer)
            .id(egui::Id::new(id))
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY)
            .desired_rows(10),
    );

    if !response.changed() {
        return None;
    }

    match serde_json::from_str::<serde_json::Value>(text_buffer) {
        Ok(val) => Some(Ok(val)),
        Err(e) => {
            ui.label(egui::RichText::new(format!("Parse error: {e}")).color(theme::RED).small());
            Some(Err(e.to_string()))
        }
    }
}

/// Combined viewer/editor toggle for a JSON value.
///
/// In view mode, renders the tree viewer with an "Edit" button.
/// In edit mode, renders the text editor with "Save" and "Cancel" buttons.
/// Returns `true` if the value was changed (on successful save).
pub fn json_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut serde_json::Value,
    text_buffer: &mut String,
    editing: &mut bool,
) -> bool {
    let mut changed = false;

    ui.label(egui::RichText::new(label).color(theme::SUBTEXT));

    if *editing {
        let parse_result = json_editor(ui, &format!("json_edit_{label}"), text_buffer);

        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(egui::RichText::new("Save").color(theme::GREEN)))
                .clicked()
            {
                match serde_json::from_str::<serde_json::Value>(text_buffer) {
                    Ok(parsed) => {
                        *value = parsed;
                        *editing = false;
                        changed = true;
                    }
                    Err(e) => {
                        ui.label(
                            egui::RichText::new(format!("Cannot save: {e}"))
                                .color(theme::RED)
                                .small(),
                        );
                    }
                }
            }
            if ui.button("Cancel").clicked() {
                *editing = false;
            }
        });

        // Show live validation status (no-op if editor already showed it)
        if let Some(Err(msg)) = parse_result {
            ui.label(egui::RichText::new(msg).color(theme::RED).small());
        }
    } else {
        json_viewer(ui, value);
        if ui
            .add(egui::Button::new(egui::RichText::new("Edit").color(theme::BLUE)))
            .clicked()
        {
            *text_buffer = serde_json::to_string_pretty(value).unwrap_or_default();
            *editing = true;
        }
    }

    changed
}
