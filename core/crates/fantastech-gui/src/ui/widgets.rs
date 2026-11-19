//! Custom reusable egui widgets and drawing helpers.

use crate::*;
use egui::{Color32, Pos2, RichText, Stroke, Vec2};

/// Interactive HSV color disc: hue follows the angle and saturation follows
/// the distance from the center. Value is controlled by the adjacent slider.
/// A single small triangle mesh renders a smooth disc without hundreds of
/// independent paint shapes.
pub(crate) fn draw_color_wheel(ui: &mut egui::Ui, color: &mut [u8; 3], is_dark: bool) -> bool {
    let diameter = ui.available_width().clamp(142.0, 228.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(diameter), egui::Sense::hover());
    let center = rect.center();
    let radius = diameter * 0.455;
    let response = ui.interact(
        rect,
        ui.id().with("fantastech_hsv_color_wheel"),
        egui::Sense::click_and_drag(),
    );

    let mut changed = false;
    if response.clicked() || response.dragged() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let delta = pointer - center;
            let distance = delta.length();
            if distance <= radius {
                let angle = delta.y.atan2(delta.x).rem_euclid(std::f32::consts::TAU);
                let hue = ((angle / std::f32::consts::TAU) * 255.0).round() as u8;
                let saturation = ((distance / radius) * 255.0).round() as u8;
                let (_, _, current_value) = fantastech_protocol::lighting::rgb_to_hsv(color[0], color[1], color[2]);
                let value = if current_value == 0 { 255 } else { current_value };
                let (r, g, b) = hsv_to_rgb_simple(
                    hue as f32 / 255.0 * 360.0,
                    saturation as f32 / 255.0,
                    value as f32 / 255.0,
                );
                let next = [r, g, b];
                if *color != next {
                    *color = next;
                    changed = true;
                }
            }
        }
    }

    let painter = ui.painter_at(rect);
    let (_, _, current_value) = fantastech_protocol::lighting::rgb_to_hsv(color[0], color[1], color[2]);
    let value_f = (if current_value == 0 { 255 } else { current_value }) as f32 / 255.0;

    // 72 hue divisions × 24 saturation rings gives a smooth-looking disc
    // in one draw call, with bounded geometry and no per-segment allocations.
    const ANGLE_SEGMENTS: usize = 72;
    const RADIUS_SEGMENTS: usize = 24;
    let stride = ANGLE_SEGMENTS + 1;
    let mut mesh = egui::Mesh::default();
    mesh.reserve_vertices((RADIUS_SEGMENTS + 1) * stride);
    mesh.reserve_triangles(RADIUS_SEGMENTS * ANGLE_SEGMENTS * 2);

    for ring in 0..=RADIUS_SEGMENTS {
        let saturation = ring as f32 / RADIUS_SEGMENTS as f32;
        let ring_radius = radius * saturation;
        for segment in 0..=ANGLE_SEGMENTS {
            let angle = std::f32::consts::TAU * segment as f32 / ANGLE_SEGMENTS as f32;
            let (r, g, b) = hsv_to_rgb_simple(angle / std::f32::consts::TAU * 360.0, saturation, value_f);
            let point = center + Vec2::new(angle.cos() * ring_radius, angle.sin() * ring_radius);
            mesh.colored_vertex(point, Color32::from_rgb(r, g, b));
        }
    }

    for ring in 0..RADIUS_SEGMENTS {
        for segment in 0..ANGLE_SEGMENTS {
            let top_left = (ring * stride + segment) as u32;
            let top_right = top_left + 1;
            let bottom_left = ((ring + 1) * stride + segment) as u32;
            let bottom_right = bottom_left + 1;
            mesh.add_triangle(top_left, top_right, bottom_right);
            mesh.add_triangle(top_left, bottom_right, bottom_left);
        }
    }
    painter.add(egui::Shape::mesh(mesh));

    // A fine outer rim and a two-tone marker keep the current selection clear.
    let rim = if is_dark { Color32::from_rgb(229, 237, 248) } else { Color32::from_rgb(51, 65, 85) };
    painter.circle_stroke(center, radius, Stroke::new(1.2_f32, rim));
    let (hue, saturation, _) = fantastech_protocol::lighting::rgb_to_hsv(color[0], color[1], color[2]);
    let marker_angle = hue as f32 / 255.0 * std::f32::consts::TAU;
    let marker_radius = saturation as f32 / 255.0 * radius;
    let marker = center + Vec2::new(marker_angle.cos() * marker_radius, marker_angle.sin() * marker_radius);
    painter.circle_filled(marker, 8.0, Color32::WHITE);
    painter.circle_stroke(marker, 8.0, Stroke::new(1.5_f32, Color32::from_rgb(20, 29, 43)));
    painter.circle_filled(marker, 4.0, Color32::from_rgb(color[0], color[1], color[2]));

    response.on_hover_text("Drag around the wheel: angle selects hue, distance from center selects saturation");
    changed
}

/// Vector-drawn status dot to eliminate non-rendering unicode glyphs across environments.
pub(crate) fn draw_status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    painter.circle_filled(center, 4.5, color.gamma_multiply(0.25));
    painter.circle_filled(center, 3.2, color);
}

/// Circular vector-painted theme toggle button (Golden Sun for Dark mode, Indigo Crescent for Light mode).
pub(crate) fn draw_circular_theme_toggle(ui: &mut egui::Ui, is_dark: bool) -> bool {
    let size = Vec2::new(26.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let hovered = response.hovered();
    let center = rect.center();
    let painter = ui.painter_at(rect);

    let bg_color = if is_dark {
        if hovered { Color32::from_rgb(45, 58, 79) } else { Color32::from_rgb(30, 41, 59) }
    } else {
        if hovered { Color32::from_rgb(226, 232, 240) } else { Color32::from_rgb(241, 245, 249) }
    };
    let border_color = if is_dark {
        if hovered { Color32::from_rgb(0, 220, 255) } else { Color32::from_rgb(71, 85, 105) }
    } else {
        if hovered { Color32::from_rgb(0, 113, 207) } else { Color32::from_rgb(203, 213, 225) }
    };

    painter.circle_filled(center, 12.0, bg_color);
    painter.circle_stroke(center, 12.0, Stroke::new(1.2_f32, border_color));

    if is_dark {
        // Dark theme: Golden radiant sun
        let sun_color = Color32::from_rgb(251, 191, 36);
        painter.circle_filled(center, 4.5, sun_color);
        for i in 0..8 {
            let angle = i as f32 * std::f32::consts::TAU / 8.0;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let r_inner = center + dir * 6.0;
            let r_outer = center + dir * 8.5;
            painter.line_segment([r_inner, r_outer], Stroke::new(1.3_f32, sun_color));
        }
    } else {
        // Light theme: Crisp indigo crescent moon
        let moon_color = Color32::from_rgb(99, 102, 241);
        let moon_radius = 6.0;
        painter.circle_filled(center, moon_radius, moon_color);
        let cutout_center = center + Vec2::new(2.6, -2.0);
        painter.circle_filled(cutout_center, moon_radius * 0.82, bg_color);
    }

    let tooltip = if is_dark { "Switch to light theme" } else { "Switch to dark theme" };
    response.on_hover_text(tooltip).clicked()
}

/// Smooth vector arrow renderer for Undo and Redo operations.
fn draw_vector_arrow(painter: &egui::Painter, center: Pos2, stroke: Stroke, is_undo: bool) {
    let d = if is_undo { -1.0_f32 } else { 1.0_f32 };

    // 1. Arrowhead pointing in the direction of action
    let tip = center + Vec2::new(d * 5.5, -0.5);
    let top_w = center + Vec2::new(d * 1.8, -4.5);
    let bot_w = center + Vec2::new(d * 1.8, 3.5);
    painter.add(egui::Shape::line(vec![top_w, tip, bot_w], stroke));

    // 2. Smooth parametric arch curving over the top
    let p0 = center + Vec2::new(d * 3.5, -0.5);
    let p1 = center + Vec2::new(-d * 0.5, -5.5);
    let p2 = center + Vec2::new(-d * 5.0, -1.5);
    let p3 = center + Vec2::new(-d * 5.0, 4.5);

    let steps = 18;
    let mut arch_pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let one_minus_t = 1.0_f32 - t;
        let b0 = one_minus_t * one_minus_t * one_minus_t;
        let b1 = 3.0_f32 * one_minus_t * one_minus_t * t;
        let b2 = 3.0_f32 * one_minus_t * t * t;
        let b3 = t * t * t;
        let x = b0 * p0.x + b1 * p1.x + b2 * p2.x + b3 * p3.x;
        let y = b0 * p0.y + b1 * p1.y + b2 * p2.y + b3 * p3.y;
        arch_pts.push(Pos2::new(x, y));
    }
    painter.add(egui::Shape::line(arch_pts, stroke));
}

/// Unified flat toolbar history button for Undo and Redo operations.
fn draw_history_button(ui: &mut egui::Ui, enabled: bool, is_dark: bool, is_undo: bool) -> bool {
    let size = Vec2::new(26.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let hovered = enabled && response.hovered();
    let active = enabled && response.is_pointer_button_down_on();

    // Toolbar icon buttons are completely flat by default (no gray box, no corner artifacts)
    if active {
        let fill = if is_dark {
            Color32::from_rgba_unmultiplied(0, 180, 216, 45)
        } else {
            Color32::from_rgb(186, 230, 253)
        };
        painter.rect_filled(rect, 4.0_f32, fill);
    } else if hovered {
        let fill = if is_dark {
            Color32::from_rgba_unmultiplied(255, 255, 255, 20)
        } else {
            Color32::from_rgb(226, 232, 240)
        };
        painter.rect_filled(rect, 4.0_f32, fill);
    }

    let icon_color = if !enabled {
        if is_dark {
            Color32::from_rgb(75, 85, 100)
        } else {
            Color32::from_rgb(180, 190, 205)
        }
    } else if hovered || active {
        if is_dark {
            Color32::from_rgb(0, 220, 255)
        } else {
            Color32::from_rgb(2, 132, 199)
        }
    } else {
        if is_dark {
            Color32::from_rgb(245, 247, 250)
        } else {
            Color32::from_rgb(30, 41, 59)
        }
    };

    let arrow_stroke = Stroke::new(1.8_f32, icon_color);
    draw_vector_arrow(&painter, center, arrow_stroke, is_undo);

    let tooltip = match (is_undo, enabled) {
        (true, true) => "Undo keymap change (Ctrl+Z)",
        (true, false) => "Nothing to undo",
        (false, true) => "Redo keymap change (Ctrl+Y)",
        (false, false) => "Nothing to redo",
    };
    response.on_hover_text(tooltip).clicked()
}

/// Sleek vector-rendered undo button with smooth state transitions.
pub(crate) fn draw_undo_button(ui: &mut egui::Ui, enabled: bool, is_dark: bool) -> bool {
    draw_history_button(ui, enabled, is_dark, true)
}

/// Sleek vector-rendered redo button with smooth state transitions.
pub(crate) fn draw_redo_button(ui: &mut egui::Ui, enabled: bool, is_dark: bool) -> bool {
    draw_history_button(ui, enabled, is_dark, false)
}

pub(crate) fn draw_key_pill(ui: &mut egui::Ui, text: &str, is_dark: bool) {
    let fill = if is_dark {
        Color32::from_rgb(30, 41, 59)
    } else {
        Color32::from_rgb(226, 232, 240)
    };
    let stroke_color = if is_dark {
        Color32::from_rgb(51, 65, 85)
    } else {
        Color32::from_rgb(203, 213, 225)
    };
    let text_color = if is_dark {
        Color32::from_rgb(241, 245, 249)
    } else {
        Color32::from_rgb(15, 23, 42)
    };

    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, stroke_color))
        .rounding(4.0)
        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.5).strong().color(text_color));
        });
}

pub(crate) fn draw_shortcut_row(ui: &mut egui::Ui, keys: &[&str], description: &str, is_dark: bool) {
    ui.horizontal(|ui| {
        for (i, key) in keys.iter().enumerate() {
            if i > 0 {
                ui.label(RichText::new("+").size(11.0).color(Color32::from_rgb(148, 163, 184)));
            }
            draw_key_pill(ui, key, is_dark);
        }
        ui.add_space(8.0);
        ui.label(RichText::new(description).size(12.5));
    });
    ui.add_space(2.0);
}

