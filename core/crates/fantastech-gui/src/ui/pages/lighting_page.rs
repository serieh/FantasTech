//! Lighting studio page, animation effects, and per-key matrix customizer.

use crate::*;
use crate::ui::widgets::*;

#[inline]
fn contrast_text_color(c: Color32) -> Color32 {
    let lum = c.r() as f32 * 0.299 + c.g() as f32 * 0.587 + c.b() as f32 * 0.114;
    let t = ((lum - 75.0) / 95.0).clamp(0.0, 1.0);
    let smooth_t = t * t * (3.0 - 2.0 * t);
    let txt_val = ((1.0 - smooth_t) * 235.0 + 20.0).round() as u8;
    Color32::from_rgb(txt_val, txt_val, txt_val)
}

impl FantasTechApp {
    pub(crate) fn render_lighting_page(&mut self, ui: &mut egui::Ui) {
        let is_dark = self.config.gui_theme == GuiTheme::Dark;

        ui.horizontal(|ui| {
            ui.heading("Lighting studio");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Load…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Profile (*.json)", &["json"])
                        .pick_file()
                    {
                        self.import_lighting_profile_from_path(&path);
                    }
                }
                if ui.button("Save as…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("lighting_profile.json")
                        .add_filter("Lighting Profile (*.json)", &["json"])
                        .save_file()
                    {
                        self.export_lighting_profile_to_path(&path);
                    }
                }
                let lighting_profiles = self.profile_manager.list_lighting_profiles();
                let is_builtin = lighting_profiles
                    .iter()
                    .find(|p| p.id == self.selected_lighting_profile)
                    .map(|p| p.is_builtin)
                    .unwrap_or(true);
                if !is_builtin && ui.button("Delete").clicked() {
                    self.delete_active_lighting_profile();
                }
                if ui.button("Reset defaults").on_hover_text("Reset active lighting profile back to factory default Spectrum effect").clicked() {
                    self.reset_active_lighting_profile();
                }
                if ui.button("Clone").on_hover_text("Duplicate the current lighting profile").clicked() {
                    self.clone_active_lighting_profile();
                }
                if ui.button("Sync from Keyboard").on_hover_text("Query the physical keyboard for active lighting effect, brightness, speed, and color").clicked() {
                    self.connection_checking = true;
                    self.connection_monitor.refresh();
                }
                let current_name = lighting_profiles
                    .iter()
                    .find(|p| p.id == self.selected_lighting_profile)
                    .map(|p| p.name.as_str())
                    .unwrap_or("Gaming");
                egui::ComboBox::from_id_salt("lighting_profile_selector")
                    .selected_text(current_name)
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        for p in &lighting_profiles {
                            if ui.selectable_label(self.selected_lighting_profile == p.id, &p.name).clicked() {
                                let id = p.id.clone();
                                self.apply_lighting_profile_by_id(&id);
                            }
                        }
                    });
                ui.label(RichText::new("Profile").strong());
            });
        });
        ui.separator();

        if self.selected_mode == 16 {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);

                if ui.button(RichText::new("Select All").strong()).on_hover_text("Select all 104 keys across the board").clicked() {
                    self.select_all_custom_keys();
                }
                if ui.button("Deselect All").on_hover_text("Clear key selection").clicked() {
                    self.deselect_all_custom_keys();
                }
                if ui.button("WASD").on_hover_text("Select WASD movement keys").clicked() {
                    self.select_custom_key_group("wasd");
                }
                if ui.button("Arrows").on_hover_text("Select Arrow keys").clicked() {
                    self.select_custom_key_group("arrows");
                }
                if ui.button("Numpad").on_hover_text("Select numeric keypad cluster").clicked() {
                    self.select_custom_key_group("numpad");
                }
                if ui.button("F-Keys").on_hover_text("Select Function row (F1-F12 + Esc)").clicked() {
                    self.select_custom_key_group("fkeys");
                }
                ui.separator();
                if ui.button(RichText::new("Paint Selected").color(Color32::from_rgb(0, 220, 255)).strong())
                    .on_hover_text("Apply currently selected color to all selected keys")
                    .clicked()
                {
                    self.apply_color_to_custom_selection(self.current_color);
                }
                if ui.button("Turn Off Selected")
                    .on_hover_text("Turn off LEDs for all selected keys (set to #000000)")
                    .clicked()
                {
                    self.turn_off_custom_selection();
                }

                ui.separator();

                // Live Selection Status Indicator
                let sel_text = if self.custom_select_all {
                    RichText::new("Selected: All Keys (104 keys)").color(Color32::from_rgb(0, 220, 255)).strong()
                } else if self.custom_selected_keys.is_empty() {
                    RichText::new("Selected: None (Click any key to select and paint)").color(Color32::from_rgb(148, 163, 184))
                } else if self.custom_selected_keys.len() == 1 {
                    let k_idx = self.custom_selected_keys.iter().copied().next().unwrap_or(0);
                    let name = ANSI_104_KEYS.iter().find(|k| k.index == k_idx).map(|k| k.name).unwrap_or("Unknown");
                    let c = self.key_colors.get(k_idx as usize).copied().unwrap_or(Color32::BLACK);
                    RichText::new(format!("Selected: {} (Slot {}) • Color: #{:02X}{:02X}{:02X}", name, k_idx, c.r(), c.g(), c.b()))
                        .color(Color32::from_rgb(0, 220, 255))
                        .strong()
                } else if self.custom_selected_keys.len() <= 6 {
                    let names: Vec<&str> = self.custom_selected_keys.iter()
                        .filter_map(|&idx| ANSI_104_KEYS.iter().find(|k| k.index == idx).map(|k| k.name))
                        .collect();
                    RichText::new(format!("Selected ({}): {}", self.custom_selected_keys.len(), names.join(", ")))
                        .color(Color32::from_rgb(0, 220, 255))
                        .strong()
                } else {
                    RichText::new(format!("Selected: {} keys", self.custom_selected_keys.len()))
                        .color(Color32::from_rgb(0, 220, 255))
                        .strong()
                };
                ui.label(sel_text);
            });
            ui.add_space(4.0);
        }

        // 1. VISUAL 104-KEY PHYSICAL KEYBOARD CANVAS (Responsive scaling with window width)
        let avail_w = (ui.available_width() - 12.0).max(1.0);
        let unit_size = (avail_w / 23.0).clamp(20.0, 56.0);
        let canvas_height = 6.45 * unit_size;

        egui::ScrollArea::horizontal()
            .auto_shrink([false, true])
            .max_height(canvas_height + 20.0)
            .show(ui, |ui| {
                let canvas_size = Vec2::new(23.0 * unit_size, canvas_height);
                let (canvas_rect, _) = ui.allocate_exact_size(canvas_size, egui::Sense::hover());
                let painter = ui.painter_at(canvas_rect);

                // Recessed mechanical chassis frame
                let chassis_rect = canvas_rect.expand(6.0);
                let chassis_fill = if is_dark {
                    Color32::from_rgb(11, 14, 20)
                } else {
                    Color32::from_rgb(238, 242, 246)
                };
                let chassis_stroke = if self.selected_mode == 16 && self.custom_select_all {
                    Stroke::new(3.0_f32, Color32::from_rgb(0, 220, 255))
                } else if is_dark {
                    Stroke::new(1.5_f32, Color32::from_rgb(33, 40, 54))
                } else {
                    Stroke::new(1.5_f32, Color32::from_rgb(203, 213, 225))
                };
                painter.rect(chassis_rect, 8.0, chassis_fill, chassis_stroke);

                if self.selected_mode == 16 && self.custom_select_all {
                    // Glowing outer border around the whole keyboard frame when select all is active
                    painter.rect_stroke(chassis_rect.expand(3.0), 10.0, Stroke::new(2.5_f32, Color32::from_rgb(0, 220, 255)));
                }

                for vkey in ANSI_104_KEYS {
                    let idx = vkey.index as usize;
                    let is_custom_selected = self.selected_mode == 16 && (self.custom_select_all || self.custom_selected_keys.contains(&vkey.index));
                    let key_rect = egui::Rect::from_min_size(
                        canvas_rect.min + Vec2::new(vkey.x * unit_size + 1.5, vkey.y * unit_size + 1.5),
                        Vec2::new(vkey.w * unit_size - 3.0, vkey.h * unit_size - 3.0),
                    );

                    let key_id = ui.id().with(("light_key", vkey.index));
                    let response = ui.interact(key_rect, key_id, egui::Sense::click());
                    let hover = response.hovered();

                    let (fill_color, text_color, border_stroke) = if self.selected_mode == 16 {
                        let c = self.key_colors.get(idx).copied().unwrap_or(Color32::BLACK);
                        if c == Color32::BLACK || (c.r() == 0 && c.g() == 0 && c.b() == 0) {
                            (
                                if is_dark { Color32::from_rgb(22, 29, 41) } else { Color32::from_rgb(248, 250, 252) },
                                if is_dark { Color32::from_rgb(148, 163, 184) } else { Color32::from_rgb(71, 85, 105) },
                                Stroke::new(1.0_f32, if hover || is_custom_selected { Color32::from_rgb(0, 210, 255) } else if is_dark { Color32::from_rgb(40, 53, 74) } else { Color32::from_rgb(203, 213, 225) }),
                            )
                        } else {
                            // Soft radial LED glow behind keycap
                            painter.circle_filled(key_rect.center(), key_rect.width() * 0.44, c.gamma_multiply(0.25));

                            let txt = contrast_text_color(c);
                            let border_color = if hover || is_custom_selected { Color32::WHITE } else { c };
                            (c, txt, Stroke::new(1.5_f32, border_color))
                        }
                    } else if self.selected_mode == 0 || self.brightness == 0 {
                        (
                            if is_dark { Color32::from_rgb(22, 29, 41) } else { Color32::from_rgb(248, 250, 252) },
                            if is_dark { Color32::from_rgb(148, 163, 184) } else { Color32::from_rgb(71, 85, 105) },
                            Stroke::new(1.0_f32, if hover { Color32::from_rgb(0, 210, 255) } else if is_dark { Color32::from_rgb(40, 53, 74) } else { Color32::from_rgb(203, 213, 225) }),
                        )
                    } else if self.selected_mode == 1 {
                        let brt = (self.brightness as f32 / 4.0).clamp(0.0, 1.0);
                        let base = Color32::from_rgb(
                            (self.current_color[0] as f32 * brt) as u8,
                            (self.current_color[1] as f32 * brt) as u8,
                            (self.current_color[2] as f32 * brt) as u8,
                        );
                        painter.circle_filled(key_rect.center(), key_rect.width() * 0.44, base.gamma_multiply(0.25));
                        let txt = contrast_text_color(base);
                        let border_color = if hover { Color32::WHITE } else { base };
                        (base, txt, Stroke::new(1.5_f32, border_color))
                    } else {
                        // Dynamic hardware animation software preview (30-60 FPS live canvas)
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
                        let time = ui.input(|i| i.time) as f32;
                        // Scaled slightly slower for smooth, pleasing, authentic visual flow
                        let spd_factor = (self.speed as f32 + 1.0) * 0.28;
                        let brt_factor = (self.brightness as f32 / 4.0).clamp(0.1, 1.0);
                        let col = match self.selected_mode {
                            2 => {
                                // Breathing: smooth sinusoidal brightness pulse
                                let pulse = ((time * spd_factor * 1.8).sin() * 0.45 + 0.55).clamp(0.08, 1.0);
                                if self.is_fixed_color {
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * pulse * brt_factor) as u8,
                                        (self.current_color[1] as f32 * pulse * brt_factor) as u8,
                                        (self.current_color[2] as f32 * pulse * brt_factor) as u8,
                                    )
                                } else {
                                    let hue = (time * spd_factor * 16.0).rem_euclid(360.0);
                                    let (r, g, b) = hsv_to_rgb_simple(hue, 1.0, pulse * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            3 => {
                                // Rotate: Pinwheel / Vortex rotation centered on keyboard (11.5, 2.75)
                                let dx = (vkey.x - 11.5) * 0.4;
                                let dy = vkey.y - 2.75;
                                let angle = dy.atan2(dx);
                                let phase = ((angle / std::f32::consts::TAU) + time * spd_factor * 0.25).rem_euclid(1.0);
                                if self.is_fixed_color {
                                    let wave = (phase * std::f32::consts::TAU).sin() * 0.45 + 0.55;
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * wave * brt_factor) as u8,
                                        (self.current_color[1] as f32 * wave * brt_factor) as u8,
                                        (self.current_color[2] as f32 * wave * brt_factor) as u8,
                                    )
                                } else {
                                    let (r, g, b) = hsv_to_rgb_simple(phase * 360.0, 0.95, 0.95 * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            4 => {
                                // ColorLoop: Whole keyboard smoothly cycles color synchronously across the 360° RGB spectrum
                                let hue = (time * spd_factor * 22.0).rem_euclid(360.0);
                                let (r, g, b) = hsv_to_rgb_simple(hue, 1.0, brt_factor);
                                Color32::from_rgb(r, g, b)
                            }
                            5 => {
                                // StreamLeftToRight: Horizontal flow matching hardware direction
                                let dir_sign = if self.direction == 1 { 1.0 } else { -1.0 };
                                let phase = (vkey.x / 23.0 + dir_sign * time * spd_factor * 0.28).rem_euclid(1.0);
                                if self.is_fixed_color {
                                    let wave = (phase * std::f32::consts::TAU).sin() * 0.45 + 0.55;
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * wave * brt_factor) as u8,
                                        (self.current_color[1] as f32 * wave * brt_factor) as u8,
                                        (self.current_color[2] as f32 * wave * brt_factor) as u8,
                                    )
                                } else {
                                    let (r, g, b) = hsv_to_rgb_simple(phase * 360.0, 0.95, 0.95 * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            6 => {
                                // Bloom: Concentric radial pulse radiating from center (11.5, 2.75) outward with rich, vibrant RGB spectrum
                                let dx = (vkey.x - 11.5) * 0.5;
                                let dy = vkey.y - 2.75;
                                let dist = (dx * dx + dy * dy).sqrt();
                                let phase = (dist * 0.35 - time * spd_factor * 0.35).rem_euclid(1.0);
                                let wave = (phase * std::f32::consts::TAU).sin() * 0.45 + 0.55;
                                if self.is_fixed_color {
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * wave * brt_factor) as u8,
                                        (self.current_color[1] as f32 * wave * brt_factor) as u8,
                                        (self.current_color[2] as f32 * wave * brt_factor) as u8,
                                    )
                                } else {
                                    let hue = (dist * 28.0 + time * 30.0).rem_euclid(360.0);
                                    let (r, g, b) = hsv_to_rgb_simple(hue, 1.0, wave * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            7 => {
                                // StreamUpToDown: Vertical cascading wave from F-row to Spacebar
                                let dir_sign = if self.direction == 1 { 1.0 } else { -1.0 };
                                let phase = (vkey.y / 5.5 + dir_sign * time * spd_factor * 0.32).rem_euclid(1.0);
                                if self.is_fixed_color {
                                    let wave = (phase * std::f32::consts::TAU).sin() * 0.45 + 0.55;
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * wave * brt_factor) as u8,
                                        (self.current_color[1] as f32 * wave * brt_factor) as u8,
                                        (self.current_color[2] as f32 * wave * brt_factor) as u8,
                                    )
                                } else {
                                    let (r, g, b) = hsv_to_rgb_simple(phase * 360.0, 0.95, 0.95 * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            8 => {
                                // Cross: Diagonal cross wave starting from the top-left corner (0.0, 0.0) and sweeping across
                                let diag = vkey.x * 0.65 + vkey.y * 1.5;
                                let wave_pos = (time * spd_factor * 7.5).rem_euclid(26.0);
                                let dist = (diag - wave_pos).abs();
                                let beam = (1.0 - (dist / 3.0)).max(0.0).powi(2);
                                let intensity = (beam * 0.94 + 0.06).clamp(0.0, 1.0);
                                if self.is_fixed_color {
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * intensity * brt_factor) as u8,
                                        (self.current_color[1] as f32 * intensity * brt_factor) as u8,
                                        (self.current_color[2] as f32 * intensity * brt_factor) as u8,
                                    )
                                } else {
                                    let hue = (diag * 14.0 + time * 25.0).rem_euclid(360.0);
                                    let (r, g, b) = hsv_to_rgb_simple(hue, 1.0, intensity * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            9 => {
                                // Rain: Only a few dozen keys lit up at any instant with beautiful, vibrant RGB jewels against dark keycaps
                                let seed = ((vkey.index as f32 * 19.17 + 43.19).sin() * 43_758.547).fract().abs();
                                let period = 6.5 + seed * 4.5;
                                let phase = (time * spd_factor * 0.7 + seed * period).rem_euclid(period);
                                let drop_dur = 1.05;
                                let intensity = if phase < drop_dur {
                                    let p = phase / drop_dur;
                                    if p < 0.20 {
                                        p / 0.20
                                    } else {
                                        ((1.0 - p) / 0.80).powi(2)
                                    }
                                } else {
                                    0.0
                                };
                                if intensity <= 0.01 {
                                    Color32::from_rgb(0, 0, 0)
                                } else if self.is_fixed_color {
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * intensity * brt_factor) as u8,
                                        (self.current_color[1] as f32 * intensity * brt_factor) as u8,
                                        (self.current_color[2] as f32 * intensity * brt_factor) as u8,
                                    )
                                } else {
                                    let hue = (seed * 360.0 + (vkey.index as f32 * 41.0)).rem_euclid(360.0);
                                    let (r, g, b) = hsv_to_rgb_simple(hue, 1.0, intensity * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            10 => {
                                // Meteor: Diagonal trailing streaks shooting from top-left to bottom-right
                                let diag = vkey.x * 0.5 + vkey.y * 1.2;
                                let meteor_phase = (diag * 0.12 - time * spd_factor * 0.38).rem_euclid(1.0);
                                let tail = if meteor_phase > 0.55 {
                                    ((meteor_phase - 0.55) / 0.45).powi(3)
                                } else {
                                    0.06
                                };
                                if self.is_fixed_color {
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * tail * brt_factor) as u8,
                                        (self.current_color[1] as f32 * tail * brt_factor) as u8,
                                        (self.current_color[2] as f32 * tail * brt_factor) as u8,
                                    )
                                } else {
                                    let hue = (diag * 18.0 + time * 15.0).rem_euclid(360.0);
                                    let (r, g, b) = hsv_to_rgb_simple(hue, 0.95, tail * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            11 => {
                                // RippleFade: Expanding circular ripple wave fading completely to black
                                let (ox, oy, otime) = match self.reactive_origin {
                                    Some((x, y, t)) if (time - t) < 3.0 => (x, y, t),
                                    _ => {
                                        let period = 2.6;
                                        let t = (time / period).floor() * period;
                                        (11.5, 2.75, t)
                                    }
                                };
                                let elapsed = time - otime;
                                let radius = elapsed * (6.5 * spd_factor);
                                let dist = (((vkey.x - ox) * 0.5).powi(2) + (vkey.y - oy).powi(2)).sqrt();
                                let ring_dist = (dist - radius).abs();
                                let ring_intensity = (1.0 - ring_dist / 1.6).max(0.0).powi(2);
                                let fade = (1.0 - elapsed / 2.5).max(0.0);
                                let intensity = (ring_intensity * fade).clamp(0.04, 1.0);
                                if self.is_fixed_color {
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * intensity * brt_factor) as u8,
                                        (self.current_color[1] as f32 * intensity * brt_factor) as u8,
                                        (self.current_color[2] as f32 * intensity * brt_factor) as u8,
                                    )
                                } else {
                                    let hue = (dist * 25.0 + otime * 40.0).rem_euclid(360.0);
                                    let (r, g, b) = hsv_to_rgb_simple(hue, 0.95, intensity * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            12 => {
                                // RippleStatic: Expanding circular wave over illuminated static base
                                let (ox, oy, otime) = match self.reactive_origin {
                                    Some((x, y, t)) if (time - t) < 3.0 => (x, y, t),
                                    _ => {
                                        let period = 2.6;
                                        let t = (time / period).floor() * period;
                                        (11.5, 2.75, t)
                                    }
                                };
                                let elapsed = time - otime;
                                let radius = elapsed * (6.5 * spd_factor);
                                let dist = (((vkey.x - ox) * 0.5).powi(2) + (vkey.y - oy).powi(2)).sqrt();
                                let ring_dist = (dist - radius).abs();
                                let ring_intensity = (1.0 - ring_dist / 1.6).max(0.0).powi(2);
                                let fade = (1.0 - elapsed / 2.5).max(0.0);
                                let total_lum = (0.22 + ring_intensity * fade * 0.78).clamp(0.0, 1.0);
                                if self.is_fixed_color {
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * total_lum * brt_factor) as u8,
                                        (self.current_color[1] as f32 * total_lum * brt_factor) as u8,
                                        (self.current_color[2] as f32 * total_lum * brt_factor) as u8,
                                    )
                                } else {
                                    let hue = (dist * 25.0 + otime * 40.0).rem_euclid(360.0);
                                    let (r, g, b) = hsv_to_rgb_simple(hue, 0.95, total_lum * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            13 => {
                                // Reactive: Key glow on press decaying smoothly to off
                                let (ox, oy, otime) = match self.reactive_origin {
                                    Some((x, y, t)) if (time - t) < 2.5 => (x, y, t),
                                    _ => {
                                        let period = 2.2;
                                        let t = (time / period).floor() * period;
                                        (11.5, 2.75, t)
                                    }
                                };
                                let elapsed = time - otime;
                                let dist = (((vkey.x - ox) * 0.6).powi(2) + (vkey.y - oy).powi(2)).sqrt();
                                let decay = (-elapsed * (2.8 * spd_factor)).exp();
                                let proximity = (1.0 - dist / 1.8).max(0.0);
                                let intensity = (proximity * decay).clamp(0.04, 1.0);
                                if self.is_fixed_color {
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * intensity * brt_factor) as u8,
                                        (self.current_color[1] as f32 * intensity * brt_factor) as u8,
                                        (self.current_color[2] as f32 * intensity * brt_factor) as u8,
                                    )
                                } else {
                                    let hue = (otime * 50.0).rem_euclid(360.0);
                                    let (r, g, b) = hsv_to_rgb_simple(hue, 0.95, intensity * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            14 => {
                                // GridRipple: Crosshair blast expanding along row and column from origin
                                let (ox, oy, otime) = match self.reactive_origin {
                                    Some((x, y, t)) if (time - t) < 2.5 => (x, y, t),
                                    _ => {
                                        let period = 2.4;
                                        let t = (time / period).floor() * period;
                                        (11.5, 2.75, t)
                                    }
                                };
                                let elapsed = time - otime;
                                let radius = elapsed * (7.5 * spd_factor);
                                let dx = (vkey.x - ox).abs();
                                let dy = (vkey.y - oy).abs();
                                let h_beam = if dy < 0.6 { (1.0 - (dx - radius).abs() / 1.5).max(0.0) } else { 0.0 };
                                let v_beam = if dx < 0.8 { (1.0 - (dy - radius).abs() / 1.2).max(0.0) } else { 0.0 };
                                let fade = (1.0 - elapsed / 2.2).max(0.0);
                                let intensity = ((h_beam.max(v_beam)) * fade).clamp(0.04, 1.0);
                                if self.is_fixed_color {
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * intensity * brt_factor) as u8,
                                        (self.current_color[1] as f32 * intensity * brt_factor) as u8,
                                        (self.current_color[2] as f32 * intensity * brt_factor) as u8,
                                    )
                                } else {
                                    let hue = (radius * 30.0 + otime * 30.0).rem_euclid(360.0);
                                    let (r, g, b) = hsv_to_rgb_simple(hue, 0.95, intensity * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            15 => {
                                // Tide: Ocean waves rising from bottom to top, then washing back down from top to bottom
                                let key_y_norm = (5.5 - vkey.y).clamp(0.0, 5.5) / 5.5; // 0.0 at bottom (spacebar), 1.0 at top (F-row)
                                // Smooth oscillation between 0.0 and 1.0: 0.0 = low tide at bottom, 1.0 = high tide at top
                                let tide_phase = (time * spd_factor * 0.9).sin();
                                let tide_crest = (tide_phase + 1.0) * 0.5;
                                let dist_to_crest = (key_y_norm - tide_crest).abs();
                                let foam = (1.0 - dist_to_crest * 2.8).max(0.0).powi(2);
                                let submerged = if key_y_norm <= tide_crest {
                                    0.28 + 0.45 * (1.0 - (tide_crest - key_y_norm) * 0.4).max(0.0)
                                } else {
                                    0.05
                                };
                                let lateral_swell = (vkey.x * 0.35 + (time * spd_factor * 0.6).sin()).sin() * 0.08;
                                let tide = (submerged + foam * 0.5 + lateral_swell).clamp(0.05, 1.0);
                                if self.is_fixed_color {
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * tide * brt_factor) as u8,
                                        (self.current_color[1] as f32 * tide * brt_factor) as u8,
                                        (self.current_color[2] as f32 * tide * brt_factor) as u8,
                                    )
                                } else {
                                    // Deep ocean azure (205°) in deep water, shifting to vibrant turquoise (160°) at surf/foam
                                    let ocean_hue = 205.0 - (tide * 45.0);
                                    let (r, g, b) = hsv_to_rgb_simple(ocean_hue, 0.92, tide * brt_factor);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            _ => {
                                // Fallback spectrum flow
                                let ratio = (vkey.x / 23.0 + time * spd_factor * 0.2).rem_euclid(1.0);
                                let (r, g, b) = hsv_to_rgb_simple(ratio * 360.0, 0.9, 0.95 * brt_factor);
                                Color32::from_rgb(r, g, b)
                            }
                        };
                        let is_unlit = col.r() < 4 && col.g() < 4 && col.b() < 4;
                        if is_unlit {
                            (
                                if is_dark { Color32::from_rgb(22, 29, 41) } else { Color32::from_rgb(248, 250, 252) },
                                if is_dark { Color32::from_rgb(148, 163, 184) } else { Color32::from_rgb(71, 85, 105) },
                                Stroke::new(1.0_f32, if hover { Color32::from_rgb(0, 210, 255) } else if is_dark { Color32::from_rgb(40, 53, 74) } else { Color32::from_rgb(203, 213, 225) }),
                            )
                        } else {
                            painter.circle_filled(key_rect.center(), key_rect.width() * 0.44, col.gamma_multiply(0.20));
                            let txt = contrast_text_color(col);
                            let border_color = if hover { Color32::WHITE } else { col };
                            (col, txt, Stroke::new(1.5_f32, border_color))
                        }
                    };

                    painter.rect(key_rect, 4.0, fill_color, border_stroke);

                    if is_custom_selected {
                        // High-contrast glowing cyan border around the selected key
                        painter.rect_stroke(key_rect.expand(2.0), 5.0, Stroke::new(2.5_f32, Color32::from_rgb(0, 220, 255)));
                    }

                    let (label_str, font_id) = fit_keycap_text(vkey.name, key_rect.width() - 4.0);
                    painter.text(key_rect.center(), egui::Align2::CENTER_CENTER, label_str, font_id, text_color);

                    let tooltip = if self.selected_mode == 16 {
                        let c = self.key_colors.get(idx).copied().unwrap_or(Color32::BLACK);
                        let sel_status = if is_custom_selected { " [SELECTED]" } else { "" };
                        format!("Key: {}{}\nMatrix Slot: {}\nColor: #{:02X}{:02X}{:02X}\n(Click to select/paint, Ctrl+Click for multi-select)", vkey.name, sel_status, vkey.index, c.r(), c.g(), c.b())
                    } else {
                        format!("Key: {}\nMatrix Slot: {}", vkey.name, vkey.index)
                    };
                    let response = response.on_hover_text(tooltip);

                    let key_idx = vkey.index;
                    let key_name = vkey.name;
                    let key_rgb = if self.selected_mode == 16 {
                        let c = self.key_colors.get(idx).copied().unwrap_or(Color32::BLACK);
                        [c.r(), c.g(), c.b()]
                    } else if self.selected_mode == 0 {
                        [0, 0, 0]
                    } else if self.is_fixed_color || self.selected_mode == 1 {
                        self.current_color
                    } else {
                        let ratio = (vkey.x / 23.0).clamp(0.0, 1.0);
                        let (r, g, b) = hsv_to_rgb_simple(ratio * 360.0, 0.9, 0.95);
                        [r, g, b]
                    };

                    response.context_menu(|menu_ui| {
                        menu_ui.horizontal(|menu_ui| {
                            menu_ui.label(RichText::new(format!("{} (Slot {})", key_name, key_idx)).strong());
                        });
                        let hex = format!("#{:02X}{:02X}{:02X}", key_rgb[0], key_rgb[1], key_rgb[2]);
                        menu_ui.horizontal(|menu_ui| {
                            let (rect, _) = menu_ui.allocate_exact_size(Vec2::new(14.0, 14.0), egui::Sense::hover());
                            menu_ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(key_rgb[0], key_rgb[1], key_rgb[2]));
                            menu_ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0_f32, Color32::GRAY));
                            menu_ui.label(RichText::new(format!("Color: {}", hex)).monospace().size(11.0));
                        });
                        menu_ui.separator();

                        if menu_ui.button(RichText::new("Copy Color").strong())
                            .on_hover_text(format!("Copy #{:02X}{:02X}{:02X} to clipboard & buffer", key_rgb[0], key_rgb[1], key_rgb[2]))
                            .clicked()
                        {
                            self.copied_color = Some(key_rgb);
                            menu_ui.output_mut(|o| o.copied_text = format!("#{:02X}{:02X}{:02X}", key_rgb[0], key_rgb[1], key_rgb[2]));
                            self.status_message = format!("Copied key '{}' color #{:02X}{:02X}{:02X} to clipboard", key_name, key_rgb[0], key_rgb[1], key_rgb[2]);
                            menu_ui.close_menu();
                        }

                        if let Some(copied) = self.copied_color {
                            let paste_lbl = format!("Paste Color (#{ :02X}{:02X}{:02X})", copied[0], copied[1], copied[2]);
                            if menu_ui.button(RichText::new(paste_lbl)).on_hover_text("Apply copied color to this key (or current selection)").clicked() {
                                if self.selected_mode == 16 {
                                    if self.custom_selected_keys.contains(&key_idx) || self.custom_select_all {
                                        self.apply_color_to_custom_selection(copied);
                                    } else {
                                        self.paint_key_and_mirror(key_idx, copied);
                                    }
                                } else {
                                    self.current_color = copied;
                                    self.record_used_color(copied);
                                    self.is_fixed_color = true;
                                    self.apply_lighting();
                                }
                                menu_ui.close_menu();
                            }
                        } else {
                            menu_ui.add_enabled(false, egui::Button::new("Paste Color (None copied)"));
                        }

                        let cur = self.current_color;
                        let apply_lbl = format!("Apply Active Color (#{ :02X}{:02X}{:02X})", cur[0], cur[1], cur[2]);
                        if menu_ui.button(RichText::new(apply_lbl)).on_hover_text("Apply active palette color to this key (or current selection)").clicked() {
                            if self.selected_mode == 16 {
                                if self.custom_selected_keys.contains(&key_idx) || self.custom_select_all {
                                    self.apply_color_to_custom_selection(cur);
                                } else {
                                    self.paint_key_and_mirror(key_idx, cur);
                                }
                            } else {
                                self.is_fixed_color = true;
                                self.apply_lighting();
                            }
                            menu_ui.close_menu();
                        }

                        if menu_ui.button("Pick Color (Eyedropper)").on_hover_text("Set this key's color as the active palette color").clicked() {
                            self.current_color = key_rgb;
                            self.record_used_color(key_rgb);
                            self.status_message = format!("Eyedropper picked #{:02X}{:02X}{:02X} from key '{}'", key_rgb[0], key_rgb[1], key_rgb[2], key_name);
                            menu_ui.close_menu();
                        }

                        if menu_ui.button("Turn Off Key").on_hover_text("Set key color to #000000").clicked() {
                            if self.selected_mode == 16 {
                                if self.custom_selected_keys.contains(&key_idx) || self.custom_select_all {
                                    self.turn_off_custom_selection();
                                } else {
                                    self.paint_key_and_mirror(key_idx, [0, 0, 0]);
                                }
                            } else {
                                self.paint_key_and_mirror(key_idx, [0, 0, 0]);
                            }
                            menu_ui.close_menu();
                        }

                        menu_ui.separator();

                        if self.selected_mode == 16 {
                            if self.custom_selected_keys.contains(&key_idx) {
                                if menu_ui.button("Deselect Key").clicked() {
                                    self.custom_selected_keys.remove(&key_idx);
                                    menu_ui.close_menu();
                                }
                            } else {
                                if menu_ui.button("Select Key").clicked() {
                                    self.custom_selected_keys.insert(key_idx);
                                    menu_ui.close_menu();
                                }
                            }
                            if menu_ui.button("Select All Keys (104)").clicked() {
                                self.select_all_custom_keys();
                                menu_ui.close_menu();
                            }
                            if menu_ui.button("Clear Selection").clicked() {
                                self.deselect_all_custom_keys();
                                menu_ui.close_menu();
                            }
                        } else {
                            if menu_ui.button("Switch to Custom Canvas").on_hover_text("Switch lighting mode to Per-Key Custom canvas").clicked() {
                                self.selected_mode = 16;
                                self.apply_lighting();
                                menu_ui.close_menu();
                            }
                        }
                    });

                    if response.clicked() {
                        if self.selected_mode == 16 {
                            let multi = ui.input(|i| i.modifiers.ctrl || i.modifiers.shift);
                            self.toggle_custom_key_selection(vkey.index, multi);

                            if idx < SPARSE_KEY_COUNT {
                                self.paint_key_and_mirror(vkey.index, self.current_color);
                            }
                        } else if (11..=14).contains(&self.selected_mode) {
                            let time = ui.input(|i| i.time) as f32;
                            self.reactive_origin = Some((vkey.x, vkey.y, time));
                        }
                    }
                }
            });

        ui.add_space(6.0);
        ui.separator();

        // 2. BOTTOM SECTION: 3 CONTROL CARDS (Adaptive & Responsive Scaling)
        let total_avail = ui.available_width().max(1.0);
        let card_spacing = 10.0;
        let card_width = ((total_avail - (2.0 * card_spacing) - 4.0) / 3.0).clamp(176.0, 420.0);

        // The cards wrap to additional rows rather than forcing a wide canvas
        // or horizontal scrolling on narrow displays.
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = card_spacing;
            self.render_lighting_card_effects(ui, card_width, is_dark);
            self.render_lighting_card_params(ui, card_width, is_dark);
            self.render_lighting_card_color(ui, card_width, is_dark);
        });
    }

    pub(crate) fn render_lighting_card_effects(&mut self, ui: &mut egui::Ui, card_width: f32, is_dark: bool) {
        ui.group(|ui| {
            ui.set_width(card_width);
            ui.vertical(|ui| {
                ui.heading("RGB lighting effect");
                ui.add_space(4.0);

                const TILES: [(u8, &str); 17] = [
                    (0, "LightOff"),
                    (1, "Static"),
                    (2, "Breath"),
                    (3, "Spin"),
                    (4, "ColorLoop"),
                    (5, "Stream"),
                    (6, "Bloom"),
                    (7, "UDWave"),
                    (8, "Cross"),
                    (9, "Rain"),
                    (10, "Meteor"),
                    (11, "TrigSpread"),
                    (12, "TrigSpreadReverse"),
                    (13, "TrigSingle"),
                    (14, "Soduko"),
                    (15, "Tide"),
                    (16, "CustomLighting"),
                ];

                let avail_w = ui.available_width();
                let tile_w = ((avail_w - 8.0) / 2.0).floor().max(80.0);

                egui::Grid::new("lighting_effects_grid")
                    .spacing([6.0, 5.0])
                    .show(ui, |ui| {
                        for (idx, (mode_id, name)) in TILES.iter().enumerate() {
                            let is_active = self.selected_mode == *mode_id;

                            let (btn_fill, btn_stroke, text_color) = if is_active {
                                if is_dark {
                                    (Color32::from_rgb(23, 42, 69), Stroke::new(1.5_f32, Color32::from_rgb(0, 210, 255)), Color32::from_rgb(0, 210, 255))
                                } else {
                                    (Color32::from_rgb(221, 242, 255), Stroke::new(1.5_f32, Color32::from_rgb(0, 113, 207)), Color32::from_rgb(0, 113, 207))
                                }
                            } else {
                                if is_dark {
                                    (Color32::from_rgb(31, 36, 44), Stroke::new(1.0_f32, Color32::from_rgb(48, 54, 61)), Color32::from_rgb(240, 246, 252))
                                } else {
                                    (Color32::from_rgb(241, 245, 249), Stroke::new(1.0_f32, Color32::from_rgb(203, 213, 225)), Color32::from_rgb(30, 41, 59))
                                }
                            };

                            let display_name = match *name {
                                "TrigSpreadReverse" => "TrigReverse",
                                "CustomLighting" => "Custom Canvas",
                                other => other,
                            };

                            let btn = egui::Button::new(
                                RichText::new(display_name)
                                    .size(10.5)
                                    .color(text_color)
                                    .strong(),
                            )
                            .fill(btn_fill)
                            .stroke(btn_stroke)
                            .rounding(4.0)
                            .min_size(Vec2::new(tile_w, 31.0));

                            if ui.add(btn).on_hover_text(*name).clicked() {
                                self.selected_mode = *mode_id;
                                if *mode_id == 0 {
                                    self.brightness = 0;
                                } else if self.brightness == 0 {
                                    self.brightness = 4;
                                }
                                if *mode_id == 1 {
                                    self.is_fixed_color = true;
                                } else if let Ok(m) = LightingMode::from_u8(*mode_id) {
                                    if !m.supports_color() {
                                        self.is_fixed_color = false;
                                    }
                                }
                                self.apply_lighting();
                            }

                            if (idx + 1) % 2 == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });
        });
    }

    pub(crate) fn render_lighting_card_params(&mut self, ui: &mut egui::Ui, card_width: f32, is_dark: bool) {
        ui.group(|ui| {
            ui.set_width(card_width);
            ui.vertical(|ui| {
                ui.heading("Parameters");
                ui.add_space(6.0);

                let mode_enum = LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum);
                let supports_color = mode_enum.supports_color() || self.selected_mode == 16;

                ui.label(RichText::new("Color Mode:").strong());
                ui.add_space(2.0);
                if self.selected_mode == 16 {
                    ui.label(RichText::new("Custom canvas\nPer-key matrix painting").color(Color32::from_rgb(0, 180, 216)));
                } else if self.selected_mode == 1 {
                    self.is_fixed_color = true;
                    ui.label(RichText::new("Solid color\nStatic uniform illumination").color(if is_dark { Color32::from_rgb(148, 163, 184) } else { Color32::from_rgb(100, 116, 139) }));
                } else if supports_color {
                    let mut changed = false;
                    ui.vertical(|ui| {
                        if ui.radio_value(&mut self.is_fixed_color, true, "Fixed Base Color").clicked() {
                            changed = true;
                        }
                        ui.add_space(3.0);
                        if ui.radio_value(&mut self.is_fixed_color, false, "Spectrum Cycle").clicked() {
                            changed = true;
                        }
                    });
                    if changed {
                        self.apply_lighting();
                    }
                } else {
                    ui.label(RichText::new("Spectrum wave\nMulti-color automatic cycle").italics().color(if is_dark { Color32::from_rgb(148, 163, 184) } else { Color32::from_rgb(100, 116, 139) }));
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                let inner_w = ui.available_width();
                let slider_bar_w = (inner_w - 70.0).max(100.0);

                let hw_b_text = match self.brightness {
                    0 => "0% (Off)",
                    1 => "25%",
                    2 => "50%",
                    3 => "75%",
                    _ => "100%",
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Hardware Brightness:").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let badge_fill = if is_dark { Color32::from_rgb(26, 36, 52) } else { Color32::from_rgb(235, 243, 253) };
                        let badge_text = if is_dark { Color32::from_rgb(0, 220, 255) } else { Color32::from_rgb(0, 113, 207) };
                        egui::Frame::none()
                            .fill(badge_fill)
                            .rounding(4.0)
                            .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new(hw_b_text).size(11.5).strong().color(badge_text));
                            });
                    });
                });
                ui.add_space(2.0);
                ui.spacing_mut().slider_width = slider_bar_w;
                if ui.add(Slider::new(&mut self.brightness, 0..=4).show_value(false).trailing_fill(true)).changed() {
                    self.apply_lighting();
                }

                ui.add_space(8.0);
                let supports_speed = mode_enum.supports_speed();
                let spd_text = if supports_speed {
                    match self.speed {
                        0 => "1 (Slowest)",
                        1 => "2 (Slow)",
                        2 => "3 (Normal)",
                        3 => "4 (Fast)",
                        _ => "5 (Fastest)",
                    }
                } else {
                    "N/A"
                };
                ui.horizontal(|ui| {
                    let label = if supports_speed {
                        RichText::new("Animation Speed:").strong()
                    } else {
                        RichText::new("Animation Speed:").strong().color(if is_dark { Color32::from_rgb(100, 116, 139) } else { Color32::from_rgb(148, 163, 184) })
                    };
                    ui.label(label);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (badge_fill, badge_text) = if !supports_speed {
                            let f = if is_dark { Color32::from_rgb(20, 24, 33) } else { Color32::from_rgb(241, 245, 249) };
                            let t = if is_dark { Color32::from_rgb(100, 116, 139) } else { Color32::from_rgb(148, 163, 184) };
                            (f, t)
                        } else if is_dark {
                            (Color32::from_rgb(26, 36, 52), Color32::from_rgb(0, 220, 255))
                        } else {
                            (Color32::from_rgb(235, 243, 253), Color32::from_rgb(0, 113, 207))
                        };
                        egui::Frame::none()
                            .fill(badge_fill)
                            .rounding(4.0)
                            .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new(spd_text).size(11.5).strong().color(badge_text));
                            });
                    });
                });
                ui.add_space(2.0);
                ui.spacing_mut().slider_width = slider_bar_w;
                let slider_resp = ui.add_enabled(
                    supports_speed,
                    Slider::new(&mut self.speed, 0..=4).show_value(false).trailing_fill(true),
                );
                if slider_resp.changed() {
                    self.apply_lighting();
                }
                if !supports_speed {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new("Speed control is not applicable for this effect")
                            .size(11.0)
                            .italics()
                            .color(if is_dark { Color32::from_rgb(100, 116, 139) } else { Color32::from_rgb(148, 163, 184) }),
                    );
                }
            });
        });
    }

    pub(crate) fn render_lighting_card_color(&mut self, ui: &mut egui::Ui, card_width: f32, is_dark: bool) {
        ui.group(|ui| {
            ui.set_width(card_width);
            ui.vertical(|ui| {
                ui.heading("Color");
                ui.add_space(6.0);
                let supports_color = LightingMode::from_u8(self.selected_mode).map(|m| m.supports_color()).unwrap_or(false) || self.selected_mode == 16;
                let color_panel_active = supports_color && (self.is_fixed_color || self.selected_mode == 16 || self.selected_mode == 1);

                if !color_panel_active {
                    ui.label(
                        RichText::new("ℹ Color controls disabled: this effect uses automatic dynamic multi-color cycle.")
                            .size(11.0)
                            .italics()
                            .color(Color32::from_rgb(148, 163, 184)),
                    );
                    ui.add_space(4.0);
                }

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Color:").strong());

                    if color_panel_active {
                        match self.config.color_display_format {
                            ColorDisplayFormat::Hex => {
                                let mut hex_buf = format!("#{:02X}{:02X}{:02X}", self.current_color[0], self.current_color[1], self.current_color[2]);
                                if ui.add(egui::TextEdit::singleline(&mut hex_buf).desired_width(70.0)).lost_focus() {
                                    let clean = hex_buf.trim().trim_start_matches('#');
                                    if clean.len() == 6 {
                                        if let Ok(val) = u32::from_str_radix(clean, 16) {
                                            let r = ((val >> 16) & 0xFF) as u8;
                                            let g = ((val >> 8) & 0xFF) as u8;
                                            let b = (val & 0xFF) as u8;
                                            self.current_color = [r, g, b];
                                            self.record_used_color(self.current_color);
                                            if self.selected_mode == 16 {
                                                if !self.custom_selected_keys.is_empty() || self.custom_select_all {
                                                    self.apply_color_to_custom_selection(self.current_color);
                                                }
                                            } else {
                                                if supports_color { self.is_fixed_color = true; }
                                                self.apply_lighting();
                                            }
                                        }
                                    }
                                }
                            }
                            ColorDisplayFormat::Rgb => {
                                let mut rgb_changed = false;
                                ui.spacing_mut().item_spacing.x = 2.0;
                                ui.label(RichText::new("R:").size(11.0).strong().color(Color32::from_rgb(239, 68, 68)));
                                if ui.add(egui::DragValue::new(&mut self.current_color[0]).range(0..=255).speed(1.0)).changed() {
                                    rgb_changed = true;
                                }
                                ui.label(RichText::new("G:").size(11.0).strong().color(Color32::from_rgb(34, 197, 94)));
                                if ui.add(egui::DragValue::new(&mut self.current_color[1]).range(0..=255).speed(1.0)).changed() {
                                    rgb_changed = true;
                                }
                                ui.label(RichText::new("B:").size(11.0).strong().color(Color32::from_rgb(59, 130, 246)));
                                if ui.add(egui::DragValue::new(&mut self.current_color[2]).range(0..=255).speed(1.0)).changed() {
                                    rgb_changed = true;
                                }
                                if rgb_changed {
                                    self.record_used_color(self.current_color);
                                    if self.selected_mode == 16 {
                                        if !self.custom_selected_keys.is_empty() || self.custom_select_all {
                                            self.apply_color_to_custom_selection(self.current_color);
                                        }
                                    } else {
                                        if supports_color { self.is_fixed_color = true; }
                                        self.apply_lighting();
                                    }
                                }
                            }
                            ColorDisplayFormat::Hsv => {
                                let (h_u8, s_u8, v_u8) = fantastech_protocol::lighting::rgb_to_hsv(self.current_color[0], self.current_color[1], self.current_color[2]);
                                let mut h_deg = ((h_u8 as f32 / 255.0) * 360.0).round() as u16;
                                let mut s_pct = ((s_u8 as f32 / 255.0) * 100.0).round() as u8;
                                let mut v_pct = ((v_u8 as f32 / 255.0) * 100.0).round() as u8;
                                let mut hsv_changed = false;
                                ui.spacing_mut().item_spacing.x = 2.0;
                                ui.label(RichText::new("H:").size(11.0).strong());
                                if ui.add(egui::DragValue::new(&mut h_deg).range(0..=360).suffix("°").speed(1.0)).changed() {
                                    hsv_changed = true;
                                }
                                ui.label(RichText::new("S:").size(11.0).strong());
                                if ui.add(egui::DragValue::new(&mut s_pct).range(0..=100).suffix("%").speed(1.0)).changed() {
                                    hsv_changed = true;
                                }
                                ui.label(RichText::new("V:").size(11.0).strong());
                                if ui.add(egui::DragValue::new(&mut v_pct).range(0..=100).suffix("%").speed(1.0)).changed() {
                                    hsv_changed = true;
                                }
                                if hsv_changed {
                                    let (r, g, b) = hsv_to_rgb_simple(h_deg as f32, s_pct as f32 / 100.0, v_pct as f32 / 100.0);
                                    self.current_color = [r, g, b];
                                    self.record_used_color(self.current_color);
                                    if self.selected_mode == 16 {
                                        if !self.custom_selected_keys.is_empty() || self.custom_select_all {
                                            self.apply_color_to_custom_selection(self.current_color);
                                        }
                                    } else {
                                        if supports_color { self.is_fixed_color = true; }
                                        self.apply_lighting();
                                    }
                                }
                            }
                            ColorDisplayFormat::Hsl => {
                                let r = self.current_color[0] as f32 / 255.0;
                                let g = self.current_color[1] as f32 / 255.0;
                                let b = self.current_color[2] as f32 / 255.0;
                                let max = r.max(g).max(b);
                                let min = r.min(g).min(b);
                                let delta = max - min;
                                let l = (max + min) / 2.0;
                                let s = if delta == 0.0 { 0.0 } else { delta / (1.0 - (2.0 * l - 1.0).abs()) };
                                let h = if delta == 0.0 {
                                    0.0
                                } else if (max - r).abs() < f32::EPSILON {
                                    60.0 * (((g - b) / delta) % 6.0)
                                } else if (max - g).abs() < f32::EPSILON {
                                    60.0 * (((b - r) / delta) + 2.0)
                                } else {
                                    60.0 * (((r - g) / delta) + 4.0)
                                };
                                let mut h_deg = ((h + 360.0) % 360.0).round() as u16;
                                let mut s_pct = (s * 100.0).round() as u8;
                                let mut l_pct = (l * 100.0).round() as u8;
                                let mut hsl_changed = false;
                                ui.spacing_mut().item_spacing.x = 2.0;
                                ui.label(RichText::new("H:").size(11.0).strong());
                                if ui.add(egui::DragValue::new(&mut h_deg).range(0..=360).suffix("°").speed(1.0)).changed() {
                                    hsl_changed = true;
                                }
                                ui.label(RichText::new("S:").size(11.0).strong());
                                if ui.add(egui::DragValue::new(&mut s_pct).range(0..=100).suffix("%").speed(1.0)).changed() {
                                    hsl_changed = true;
                                }
                                ui.label(RichText::new("L:").size(11.0).strong());
                                if ui.add(egui::DragValue::new(&mut l_pct).range(0..=100).suffix("%").speed(1.0)).changed() {
                                    hsl_changed = true;
                                }
                                if hsl_changed {
                                    let (r, g, b) = hsl_to_rgb(h_deg as f32, s_pct as f32 / 100.0, l_pct as f32 / 100.0);
                                    self.current_color = [r, g, b];
                                    self.record_used_color(self.current_color);
                                    if self.selected_mode == 16 {
                                        if !self.custom_selected_keys.is_empty() || self.custom_select_all {
                                            self.apply_color_to_custom_selection(self.current_color);
                                        }
                                    } else {
                                        if supports_color { self.is_fixed_color = true; }
                                        self.apply_lighting();
                                    }
                                }
                            }
                        }
                    } else {
                        ui.label(RichText::new("Dynamic").italics().weak());
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("color_card_fmt_select")
                            .selected_text(match self.config.color_display_format {
                                ColorDisplayFormat::Hex => "HEX",
                                ColorDisplayFormat::Rgb => "RGB",
                                ColorDisplayFormat::Hsv => "HSV",
                                ColorDisplayFormat::Hsl => "HSL",
                            })
                            .width(55.0)
                            .show_ui(ui, |ui| {
                                if ui.selectable_value(&mut self.config.color_display_format, ColorDisplayFormat::Hex, "HEX").clicked() {
                                    let _ = self.config.save();
                                }
                                if ui.selectable_value(&mut self.config.color_display_format, ColorDisplayFormat::Rgb, "RGB").clicked() {
                                    let _ = self.config.save();
                                }
                                if ui.selectable_value(&mut self.config.color_display_format, ColorDisplayFormat::Hsv, "HSV").clicked() {
                                    let _ = self.config.save();
                                }
                                if ui.selectable_value(&mut self.config.color_display_format, ColorDisplayFormat::Hsl, "HSL").clicked() {
                                    let _ = self.config.save();
                                }
                            });
                    });
                });

                ui.add_space(4.0);
                let preview_w = ui.available_width();
                let (preview_rect, _) = ui.allocate_exact_size(Vec2::new(preview_w, 24.0), egui::Sense::hover());
                if color_panel_active {
                    ui.painter().rect(
                        preview_rect,
                        4.0,
                        Color32::from_rgb(self.current_color[0], self.current_color[1], self.current_color[2]),
                        Stroke::new(1.0_f32, if is_dark { Color32::from_rgb(51, 65, 85) } else { Color32::from_rgb(203, 213, 225) }),
                    );
                } else if self.selected_mode == 0 {
                    ui.painter().rect(
                        preview_rect,
                        4.0,
                        Color32::from_rgb(22, 29, 41),
                        Stroke::new(1.0_f32, Color32::from_rgb(51, 65, 85)),
                    );
                    ui.painter().text(
                        preview_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "LEDs Off",
                        egui::FontId::proportional(11.0),
                        Color32::from_rgb(148, 163, 184),
                    );
                } else {
                    let segments = 6;
                    let seg_w = preview_rect.width() / segments as f32;
                    let spectrum_cols = [
                        Color32::from_rgb(239, 68, 68),
                        Color32::from_rgb(249, 115, 22),
                        Color32::from_rgb(234, 179, 8),
                        Color32::from_rgb(34, 197, 94),
                        Color32::from_rgb(6, 182, 212),
                        Color32::from_rgb(168, 85, 247),
                    ];
                    for (i, col) in spectrum_cols.iter().enumerate() {
                        let seg_rect = egui::Rect::from_min_size(
                            preview_rect.min + Vec2::new(i as f32 * seg_w, 0.0),
                            Vec2::new(seg_w + 1.0, preview_rect.height()),
                        ).intersect(preview_rect);
                        ui.painter().rect_filled(seg_rect, 0.0, *col);
                    }
                    ui.painter().rect_stroke(
                        preview_rect,
                        4.0,
                        Stroke::new(1.0_f32, if is_dark { Color32::from_rgb(51, 65, 85) } else { Color32::from_rgb(203, 213, 225) }),
                    );
                    ui.painter().text(
                        preview_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Spectrum Cycle",
                        egui::FontId::proportional(11.0),
                        Color32::WHITE,
                    );
                }

                ui.add_enabled_ui(color_panel_active, |ui| {
                    ui.add_space(6.0);
                    ui.vertical_centered(|ui| {
                        if draw_color_wheel(ui, &mut self.current_color, is_dark) {
                            if self.selected_mode == 16 {
                                if !self.custom_selected_keys.is_empty() || self.custom_select_all {
                                    self.apply_color_to_custom_selection(self.current_color);
                                }
                            } else {
                                if supports_color {
                                    self.is_fixed_color = true;
                                }
                                self.apply_lighting();
                            }
                        }
                    });

                    let (hsv_h, hsv_s, mut hsv_v) = fantastech_protocol::lighting::rgb_to_hsv(
                        self.current_color[0], self.current_color[1], self.current_color[2],
                    );
                    ui.add_space(8.0);
                    let pct = ((hsv_v as f32 / 255.0) * 100.0).round() as u8;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Brightness:").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let badge_fill = if is_dark {
                                Color32::from_rgb(26, 36, 52)
                            } else {
                                Color32::from_rgb(235, 243, 253)
                            };
                            let badge_text = if is_dark {
                                Color32::from_rgb(0, 220, 255)
                            } else {
                                Color32::from_rgb(0, 113, 207)
                            };
                            egui::Frame::none()
                                .fill(badge_fill)
                                .rounding(4.0)
                                .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(format!("{}%", pct)).size(11.5).strong().color(badge_text));
                                });
                        });
                    });
                    ui.add_space(2.0);
                    let slider_w = ui.available_width().max(120.0);
                    ui.spacing_mut().slider_width = slider_w;
                    let slider = Slider::new(&mut hsv_v, 0..=255)
                        .show_value(false)
                        .trailing_fill(true);
                    if ui.add_sized([slider_w, 20.0], slider).changed() {
                        let (r, g, b) = hsv_to_rgb_simple(
                            hsv_h as f32 / 255.0 * 360.0,
                            hsv_s as f32 / 255.0,
                            hsv_v as f32 / 255.0,
                        );
                        self.current_color = [r, g, b];
                        self.record_used_color(self.current_color);
                        if self.selected_mode == 16 {
                            if !self.custom_selected_keys.is_empty() || self.custom_select_all {
                                self.apply_color_to_custom_selection(self.current_color);
                            }
                        } else {
                            if supports_color {
                                self.is_fixed_color = true;
                            }
                            self.apply_lighting();
                        }
                    }



                    ui.add_space(8.0);
                    ui.label(RichText::new("Quick Swatches:").strong());
                    ui.add_space(3.0);
                    let swatches: [(&str, [u8; 3]); 10] = [
                        ("Red", [229, 62, 62]),
                        ("Orange", [249, 115, 22]),
                        ("Yellow", [234, 179, 8]),
                        ("Green", [34, 197, 94]),
                        ("Cyan", [6, 182, 212]),
                        ("Azure Blue", [0, 113, 207]),
                        ("Purple", [168, 85, 247]),
                        ("Pink", [236, 72, 153]),
                        ("White", [255, 255, 255]),
                        ("Off", [0, 0, 0]),
                    ];

                    let avail_swatches_w = ui.available_width();
                    let swatch_w = ((avail_swatches_w - 4.0 * 5.0) / 5.0).floor().clamp(22.0, 52.0);
                    egui::Grid::new("swatches_grid").spacing([5.0, 5.0]).show(ui, |ui| {
                        for (idx, (lbl, rgb)) in swatches.iter().enumerate() {
                            let btn = egui::Button::new(" ")
                                .fill(Color32::from_rgb(rgb[0], rgb[1], rgb[2]))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(100, 116, 139)))
                                .rounding(4.0)
                                .min_size(Vec2::new(swatch_w, 22.0));
                            if ui.add(btn).on_hover_text(*lbl).clicked() {
                                self.current_color = *rgb;
                                self.record_used_color(*rgb);
                                if self.selected_mode == 16 {
                                    if !self.custom_selected_keys.is_empty() || self.custom_select_all {
                                        self.apply_color_to_custom_selection(*rgb);
                                    }
                                } else {
                                    if supports_color {
                                        self.is_fixed_color = true;
                                    }
                                    self.apply_lighting();
                                }
                            }
                            if (idx + 1) % 5 == 0 {
                                ui.end_row();
                            }
                        }
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new("Recent colors:").strong());
                    ui.add_space(3.0);
                    let recent_w = ((avail_swatches_w - 5.0 * 5.0) / 6.0).floor().clamp(18.0, 48.0);
                    let recent_list: Vec<[u8; 3]> = self.recent_colors.iter().copied().take(6).collect();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        for (i, rgb) in recent_list.into_iter().enumerate() {
                            let hex = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                            let is_active = self.current_color == rgb;
                            let stroke = if is_active {
                                Stroke::new(2.0_f32, if is_dark { Color32::from_rgb(0, 220, 255) } else { Color32::from_rgb(0, 113, 207) })
                            } else {
                                Stroke::new(1.0_f32, Color32::from_rgb(100, 116, 139))
                            };
                            let btn = egui::Button::new("")
                                .fill(Color32::from_rgb(rgb[0], rgb[1], rgb[2]))
                                .stroke(stroke)
                                .rounding(4.0)
                                .min_size(Vec2::new(recent_w, 24.0));
                            if ui.add(btn).on_hover_text(format!("Recent #{}: {}\nClick to select and apply", i + 1, hex)).clicked() {
                                self.current_color = rgb;
                                self.record_used_color(rgb);
                                if self.selected_mode == 16 {
                                    if !self.custom_selected_keys.is_empty() || self.custom_select_all {
                                        self.apply_color_to_custom_selection(rgb);
                                    }
                                } else {
                                    if supports_color {
                                        self.is_fixed_color = true;
                                    }
                                    self.apply_lighting();
                                }
                            }
                        }
                    });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                ui.label(RichText::new("Designer Palettes:").strong());
                ui.add_space(3.0);
                let palettes = [
                    ("miami", "Miami", [0, 240, 255]),
                    ("terminal", "Matrix", [0, 255, 65]),
                    ("flare", "Solar", [255, 69, 0]),
                    ("vaporwave", "Vapor", [157, 78, 221]),
                    ("frost", "Frost", [58, 134, 255]),
                    ("blood_moon", "Blood", [217, 4, 41]),
                    ("pastel", "Pastel", [183, 228, 199]),
                    ("rainbow", "Rainbow", [255, 0, 0]),
                ];
                let pal_w = ((ui.available_width() - 3.0 * 5.0) / 4.0).floor().clamp(22.0, 70.0);
                egui::Grid::new("designer_palettes_grid").spacing([5.0, 5.0]).show(ui, |ui| {
                    for (idx, (p_id, p_lbl, c1)) in palettes.iter().enumerate() {
                        let text_color = if *p_id == "pastel" || *p_id == "frost" { Color32::BLACK } else { Color32::WHITE };
                        let btn = egui::Button::new(RichText::new(*p_lbl).size(11.0).strong().color(text_color))
                            .fill(Color32::from_rgb(c1[0], c1[1], c1[2]))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(100, 116, 139)))
                            .rounding(4.0)
                            .min_size(Vec2::new(pal_w, 24.0));
                        if ui.add(btn).on_hover_text(format!("Apply '{}' full-keyboard palette (switches to Custom Canvas)", p_lbl)).clicked() {
                            self.apply_designer_palette(p_id);
                        }
                        if (idx + 1) % 4 == 0 {
                            ui.end_row();
                        }
                    }
                });
            });
        });
    }


}
