//! Custom dark theme for the Claw Control Panel.
//!
//! Inspired by Catppuccin Mocha palette for a polished, modern look.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals, style::WidgetVisuals};

// Catppuccin Mocha-inspired palette
pub const BASE: Color32 = Color32::from_rgb(30, 30, 46);       // #1e1e2e — main background
pub const MANTLE: Color32 = Color32::from_rgb(24, 24, 37);     // #181825 — sidebar/panel bg
pub const SURFACE0: Color32 = Color32::from_rgb(49, 50, 68);   // #313244 — cards/frames
pub const SURFACE1: Color32 = Color32::from_rgb(69, 71, 90);   // #45475a — hover states
pub const SURFACE2: Color32 = Color32::from_rgb(88, 91, 112);  // #585b70 — borders
pub const TEXT: Color32 = Color32::from_rgb(205, 214, 244);     // #cdd6f4 — primary text
pub const SUBTEXT: Color32 = Color32::from_rgb(166, 173, 200);  // #a6adc8 — secondary text
pub const BLUE: Color32 = Color32::from_rgb(137, 180, 250);     // #89b4fa — accent/links
pub const GREEN: Color32 = Color32::from_rgb(166, 227, 161);    // #a6e3a1 — success/running
pub const RED: Color32 = Color32::from_rgb(243, 139, 168);      // #f38ba8 — error/stopped
pub const YELLOW: Color32 = Color32::from_rgb(249, 226, 175);   // #f9e2af — warning/starting
pub const OVERLAY: Color32 = Color32::from_rgb(108, 112, 134);  // #6c7086 — disabled/muted

/// Apply the custom Claw theme to the egui context.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    // Window and panel backgrounds
    visuals.window_fill = BASE;
    visuals.panel_fill = BASE;
    visuals.extreme_bg_color = MANTLE;
    visuals.faint_bg_color = SURFACE0;

    let corner_radius = CornerRadius::same(6);

    visuals.widgets.noninteractive = WidgetVisuals {
        bg_fill: SURFACE0,
        weak_bg_fill: SURFACE0,
        bg_stroke: Stroke::new(1.0, SURFACE2),
        corner_radius,
        fg_stroke: Stroke::new(1.0, TEXT),
        expansion: 0.0,
    };

    visuals.widgets.inactive = WidgetVisuals {
        bg_fill: SURFACE0,
        weak_bg_fill: SURFACE0,
        bg_stroke: Stroke::new(1.0, SURFACE2),
        corner_radius,
        fg_stroke: Stroke::new(1.0, SUBTEXT),
        expansion: 0.0,
    };

    visuals.widgets.hovered = WidgetVisuals {
        bg_fill: SURFACE1,
        weak_bg_fill: SURFACE1,
        bg_stroke: Stroke::new(1.0, BLUE),
        corner_radius,
        fg_stroke: Stroke::new(1.0, TEXT),
        expansion: 1.0,
    };

    visuals.widgets.active = WidgetVisuals {
        bg_fill: SURFACE2,
        weak_bg_fill: SURFACE2,
        bg_stroke: Stroke::new(1.0, BLUE),
        corner_radius,
        fg_stroke: Stroke::new(2.0, TEXT),
        expansion: 0.0,
    };

    visuals.widgets.open = WidgetVisuals {
        bg_fill: SURFACE1,
        weak_bg_fill: SURFACE1,
        bg_stroke: Stroke::new(1.0, BLUE),
        corner_radius,
        fg_stroke: Stroke::new(1.0, TEXT),
        expansion: 0.0,
    };

    // Selection
    visuals.selection.bg_fill = Color32::from_rgba_premultiplied(
        (BLUE.r() as f32 * 0.3) as u8,
        (BLUE.g() as f32 * 0.3) as u8,
        (BLUE.b() as f32 * 0.3) as u8,
        (BLUE.a() as f32 * 0.3) as u8,
    );
    visuals.selection.stroke = Stroke::new(1.0, BLUE);

    visuals.window_stroke = Stroke::new(1.0, SURFACE2);

    ctx.set_visuals(visuals);
}
