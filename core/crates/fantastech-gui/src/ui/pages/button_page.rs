//! Keymap and button assignment studio page.

use crate::*;
use crate::ui::widgets::*;

impl FantasTechApp {
    pub(crate) fn render_button_page(&mut self, ui: &mut egui::Ui) {
        let is_dark = self.config.gui_theme == GuiTheme::Dark;
        let prof = (self.active_profile as usize).min(2);
        let lay = self.active_layer as usize;
        let default_actions = default_layer_keymap(self.active_layer);

        ui.horizontal(|ui| {
            ui.heading("Key mapping");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let reset_btn = egui::Button::new("Reset").min_size(Vec2::new(65.0, 26.0));
                if ui.add(reset_btn).on_hover_text("Reset layer to default mapping").clicked() {
                    self.reset_active_button_profile();
                }
                let apply_btn = egui::Button::new(RichText::new("Apply").color(Color32::WHITE).strong())
                    .fill(Color32::from_rgb(0, 105, 190))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(28, 145, 220)))
                    .min_size(Vec2::new(75.0, 26.0));
                if ui.add(apply_btn).on_hover_text("Apply current layer mapping to keyboard").clicked() {
                    self.apply_current_layer_to_hardware();
                }
            });
        });
        ui.add_space(5.0);

        // Single unified toolbar row: Hardware profiles on the left, Profile management on the right
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Hardware profile").strong());
                for p in 0..3 {
                    let name = format!("Profile {}", p + 1);
                    if ui.selectable_label(self.active_profile == p, name).clicked() {
                        self.set_active_profile(p);
                    }
                }

                ui.separator();
                ui.label(RichText::new("Layer").strong());
                if ui.selectable_label(self.active_layer == KeyLayer::Base, "Main").clicked()
                    && self.active_layer != KeyLayer::Base
                {
                    self.active_layer = KeyLayer::Base;
                    self.undo_keymaps.clear();
                    self.redo_keymaps.clear();
                }
                if ui.selectable_label(self.active_layer == KeyLayer::Fn, "Fn").clicked()
                    && self.active_layer != KeyLayer::Fn
                {
                    self.active_layer = KeyLayer::Fn;
                    self.undo_keymaps.clear();
                    self.redo_keymaps.clear();
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if draw_undo_button(ui, !self.undo_keymaps.is_empty(), is_dark) {
                        self.undo_keymap_edit();
                    }
                    if draw_redo_button(ui, !self.redo_keymaps.is_empty(), is_dark) {
                        self.redo_keymap_edit();
                    }
                });
                ui.separator();
                if let Some(key_idx) = self.selected_key {
                    let key_name = KEY_DEFINITIONS.iter().find(|k| k.index == key_idx).map(|k| k.name).unwrap_or("Unknown");
                    let cur_act = self.profiles[prof][lay][key_idx as usize];
                    ui.label(RichText::new(format!("{}: {}", key_name, format_action_str(&cur_act))).color(Color32::from_rgb(0, 180, 216)).strong());

                    if ui.button("Modify").clicked() {
                        self.modify_modal_open = true;
                    }
                    if ui.button("Restore Key").clicked() {
                        self.remap_single_key(key_idx, default_actions[key_idx as usize]);
                    }
                }

                // Software Profile tools placed on the right side of this same row
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Load…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Profile (*.json)", &["json"])
                            .pick_file()
                        {
                            self.import_button_profile_from_path(&path);
                        }
                    }
                    if ui.button("Save as…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("button_profile.json")
                            .add_filter("Button Profile (*.json)", &["json"])
                            .save_file()
                        {
                            self.export_button_profile_to_path(&path);
                        }
                    }
                    let button_profiles = self.profile_manager.list_button_profiles();
                    let is_builtin = button_profiles
                        .iter()
                        .find(|p| p.id == self.selected_button_profile)
                        .map(|p| p.is_builtin)
                        .unwrap_or(true);
                    if !is_builtin && ui.button("Delete").on_hover_text("Delete the selected custom profile").clicked() {
                        self.delete_active_button_profile();
                    }
                    if ui.button("Clone").on_hover_text("Duplicate the current button profile").clicked() {
                        self.clone_active_button_profile();
                    }
                    let current_name = button_profiles
                        .iter()
                        .find(|p| p.id == self.selected_button_profile)
                        .map(|p| p.name.as_str())
                        .unwrap_or("Default");
                    egui::ComboBox::from_id_salt("button_profile_selector")
                        .selected_text(current_name)
                        .width(140.0)
                        .show_ui(ui, |ui| {
                            for p in &button_profiles {
                                if ui.selectable_label(self.selected_button_profile == p.id, &p.name).clicked() {
                                    let id = p.id.clone();
                                    self.apply_button_profile_by_id(&id);
                                }
                            }
                        });
                    ui.label(RichText::new("Profile").strong());
                });
            });
        });

        ui.separator();

        // 104-KEY TRUE ANSI CANVAS (Responsive scaling with window width)
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

            let is_dark = self.config.gui_theme == GuiTheme::Dark;

            // Recessed mechanical chassis frame
            let chassis_rect = canvas_rect.expand(6.0);
            let chassis_fill = if is_dark {
                Color32::from_rgb(11, 14, 20)
            } else {
                Color32::from_rgb(238, 242, 246)
            };
            let chassis_stroke = if is_dark {
                Stroke::new(1.5_f32, Color32::from_rgb(33, 40, 54))
            } else {
                Stroke::new(1.5_f32, Color32::from_rgb(203, 213, 225))
            };
            painter.rect(chassis_rect, 8.0, chassis_fill, chassis_stroke);

            for vkey in ANSI_104_KEYS {
                let selected = self.selected_key == Some(vkey.index);
                let cur_action = self.profiles[prof][lay][vkey.index as usize];
                let def_action = default_actions[vkey.index as usize];
                let is_remapped = cur_action != def_action;

                let key_rect = egui::Rect::from_min_size(
                    canvas_rect.min + Vec2::new(vkey.x * unit_size + 1.5, vkey.y * unit_size + 1.5),
                    Vec2::new(vkey.w * unit_size - 3.0, vkey.h * unit_size - 3.0),
                );

                let key_id = ui.id().with(("btn_key", vkey.index));
                let response = ui.interact(key_rect, key_id, egui::Sense::click());
                let hover = response.hovered();

                let (fill_color, stroke_color, stroke_width) = if selected {
                    if is_dark {
                        (Color32::from_rgb(15, 46, 74), Color32::from_rgb(0, 229, 255), 2.0_f32)
                    } else {
                        (Color32::from_rgb(221, 242, 255), Color32::from_rgb(0, 113, 207), 2.0_f32)
                    }
                } else if is_remapped {
                    if is_dark {
                        (Color32::from_rgb(18, 42, 60), Color32::from_rgb(56, 189, 248), 1.5_f32)
                    } else {
                        (Color32::from_rgb(224, 242, 254), Color32::from_rgb(2, 132, 199), 1.5_f32)
                    }
                } else if hover {
                    if is_dark {
                        (Color32::from_rgb(38, 48, 64), Color32::from_rgb(0, 210, 255), 1.5_f32)
                    } else {
                        (Color32::from_rgb(226, 232, 240), Color32::from_rgb(0, 113, 207), 1.5_f32)
                    }
                } else {
                    if is_dark {
                        (Color32::from_rgb(22, 29, 41), Color32::from_rgb(40, 53, 74), 1.0_f32)
                    } else {
                        (Color32::from_rgb(248, 250, 252), Color32::from_rgb(203, 213, 225), 1.0_f32)
                    }
                };

                painter.rect(key_rect, 4.0, fill_color, Stroke::new(stroke_width, stroke_color));

                // Subtle top edge tactile highlight for dark theme
                if is_dark && !selected {
                    let top_highlight = egui::Rect::from_min_max(
                        egui::pos2(key_rect.left() + 2.0, key_rect.top() + 1.0),
                        egui::pos2(key_rect.right() - 2.0, key_rect.top() + 2.0),
                    );
                    painter.rect_filled(top_highlight, 1.0, Color32::from_rgba_premultiplied(255, 255, 255, 12));
                }

                let is_fn_layer = self.active_layer == KeyLayer::Fn;
                let has_fn_override = is_fn_layer && vkey.fn_name != vkey.name;

                if is_fn_layer {
                    if has_fn_override {
                        let (base_label, base_font) = fit_keycap_text(vkey.name, key_rect.width() - 4.0);
                        let base_color = if is_dark { Color32::from_rgb(160, 174, 192) } else { Color32::from_rgb(100, 116, 139) };
                        let (fn_label, fn_font) = fit_keycap_text(vkey.fn_name, key_rect.width() - 4.0);
                        let fn_color = if is_dark { Color32::from_rgb(56, 189, 248) } else { Color32::from_rgb(2, 132, 199) };

                        if is_remapped {
                            painter.text(egui::pos2(key_rect.center().x, key_rect.center().y - (key_rect.height() * 0.28)), egui::Align2::CENTER_CENTER, base_label, base_font, base_color);
                            painter.text(egui::pos2(key_rect.center().x, key_rect.center().y), egui::Align2::CENTER_CENTER, fn_label, fn_font, fn_color);
                        } else {
                            painter.text(egui::pos2(key_rect.center().x, key_rect.center().y - (key_rect.height() * 0.22)), egui::Align2::CENTER_CENTER, base_label, base_font, base_color);
                            painter.text(egui::pos2(key_rect.center().x, key_rect.center().y + (key_rect.height() * 0.22)), egui::Align2::CENTER_CENTER, fn_label, fn_font, fn_color);
                        }
                    } else {
                        let (label_str, font_id) = fit_keycap_text(vkey.name, key_rect.width() - 4.0);
                        let text_color = if is_dark { Color32::from_rgb(160, 174, 192) } else { Color32::from_rgb(100, 116, 139) };
                        let pos = if is_remapped { egui::pos2(key_rect.center().x, key_rect.center().y - (key_rect.height() * 0.22)) } else { key_rect.center() };
                        painter.text(pos, egui::Align2::CENTER_CENTER, label_str, font_id, text_color);
                    }
                } else {
                    let (label_str, font_id) = fit_keycap_text(vkey.name, key_rect.width() - 4.0);
                    let text_color = if selected {
                        if is_dark { Color32::from_rgb(0, 229, 255) } else { Color32::from_rgb(0, 113, 207) }
                    } else if is_dark {
                        Color32::from_rgb(241, 245, 249)
                    } else {
                        Color32::from_rgb(15, 23, 42)
                    };
                    let pos = if is_remapped { egui::pos2(key_rect.center().x, key_rect.top() + (unit_size * 0.26)) } else { key_rect.center() };
                    painter.text(pos, egui::Align2::CENTER_CENTER, label_str, font_id, text_color);
                }

                if is_remapped {
                    let act_str = format_action_str(&cur_action);
                    let (badge_str, badge_font) = fit_keycap_text(&act_str, key_rect.width() - 4.0);
                    let badge_color = if is_dark { Color32::from_rgb(56, 189, 248) } else { Color32::from_rgb(2, 132, 199) };
                    painter.text(egui::pos2(key_rect.center().x, key_rect.bottom() - (unit_size * 0.22)), egui::Align2::CENTER_CENTER, badge_str, badge_font, badge_color);
                }

                let tooltip = format!(
                    "Key: {}\nDefault: {}\nCurrent Action: {}\nFn Legend: {}\nMatrix Slot: {}",
                    vkey.name,
                    format_action_str(&def_action),
                    format_action_str(&cur_action),
                    vkey.fn_name,
                    vkey.index
                );
                let response = response.on_hover_text(tooltip);

                if selected {
                    response.scroll_to_me(Some(egui::Align::Center));
                }
                if response.clicked() {
                    self.selected_key = Some(vkey.index);
                }
                if response.secondary_clicked() {
                    self.selected_key = Some(vkey.index);
                    self.modify_modal_open = true;
                }
            }
        });

        ui.add_space(6.0);

        // BOTTOM QUICK ACTION PALETTE
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Quick Palette:").strong());
                if ui.selectable_label(self.palette_tab == PaletteTab::Basic, "Keyboard").clicked() {
                    self.palette_tab = PaletteTab::Basic;
                }
                if ui.selectable_label(self.palette_tab == PaletteTab::Media, "Media").clicked() {
                    self.palette_tab = PaletteTab::Media;
                }
                if ui.selectable_label(self.palette_tab == PaletteTab::Shortcut, "Shortcuts").clicked() {
                    self.palette_tab = PaletteTab::Shortcut;
                }
                if ui.selectable_label(self.palette_tab == PaletteTab::Custom, "Hardware").clicked() {
                    self.palette_tab = PaletteTab::Custom;
                }
                if ui.selectable_label(self.palette_tab == PaletteTab::Macro, "Macro Triggers").clicked() {
                    self.palette_tab = PaletteTab::Macro;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.palette_search.is_empty() && ui.small_button("Clear").clicked() {
                        self.palette_search.clear();
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.palette_search)
                            .hint_text("Search category…")
                            .desired_width(170.0),
                    );
                    ui.label("Find action:");
                });
            });

            ui.separator();
            let action_query = self.palette_search.trim().to_lowercase();

            match self.palette_tab {
                PaletteTab::Basic => {
                    let mut assign_scancode = None;

                    if let Some(selected_idx) = self.selected_key {
                        let key_name = KEY_DEFINITIONS.iter().find(|k| k.index == selected_idx).map(|k| k.name).unwrap_or("Unknown");
                        ui.label(RichText::new(format!("Click any key below to reassign '{}' (Slot {}) to that key's scancode:", key_name, selected_idx)).color(Color32::from_rgb(0, 180, 216)).strong());
                    } else {
                        ui.label(RichText::new("Select a key on the keyboard above first, then click any key below to assign it (or click below to select a key).").color(Color32::from_rgb(0, 180, 216)).italics());
                    }

                    ui.add_space(4.0);

                    egui::ScrollArea::horizontal()
                        .auto_shrink([true, true])
                        .show(ui, |ui| {
                            let avail_mini = (ui.available_width() - 8.0).max(1.0);
                            let mini_unit_size = (avail_mini / 23.0).clamp(14.0, 36.0);
                            let mini_canvas_size = Vec2::new(23.0 * mini_unit_size, 6.45 * mini_unit_size);
                            let (mini_canvas_rect, _) = ui.allocate_exact_size(mini_canvas_size, egui::Sense::hover());
                            let painter = ui.painter_at(mini_canvas_rect);

                            // Recessed plate background
                            let plate_rect = mini_canvas_rect.expand(6.0);
                            let plate_fill = if is_dark { Color32::from_rgb(11, 14, 20) } else { Color32::from_rgb(238, 242, 246) };
                            let plate_stroke = if is_dark { Stroke::new(1.0_f32, Color32::from_rgb(33, 40, 54)) } else { Stroke::new(1.0_f32, Color32::from_rgb(203, 213, 225)) };
                            painter.rect(plate_rect, 6.0, plate_fill, plate_stroke);

                            for vkey in ANSI_104_KEYS {
                                if !action_query.is_empty() && !vkey.name.to_lowercase().contains(&action_query) {
                                    continue;
                                }
                                let scancode = KEY_DEFINITIONS.iter().find(|k| k.index == vkey.index)
                                    .and_then(|k| default_scancode_for_key(k.name))
                                    .or_else(|| default_scancode_for_key(vkey.name));

                                let key_rect = egui::Rect::from_min_size(
                                    mini_canvas_rect.min + Vec2::new(vkey.x * mini_unit_size + 1.0, vkey.y * mini_unit_size + 1.0),
                                    Vec2::new(vkey.w * mini_unit_size - 2.0, vkey.h * mini_unit_size - 2.0),
                                );

                                let key_id = ui.id().with(("mini_pal_key", vkey.index));
                                let resp = ui.interact(key_rect, key_id, egui::Sense::click());
                                let hover = resp.hovered();

                                if resp.clicked() {
                                    if self.selected_key.is_some() {
                                        if let Some(sc) = scancode {
                                            assign_scancode = Some(sc);
                                        }
                                    } else {
                                        self.selected_key = Some(vkey.index);
                                    }
                                }

                                let (key_fill, key_stroke, text_color) = if hover {
                                    if is_dark {
                                        (Color32::from_rgb(37, 99, 235), Stroke::new(1.5_f32, Color32::from_rgb(96, 165, 250)), Color32::WHITE)
                                    } else {
                                        (Color32::from_rgb(191, 219, 254), Stroke::new(1.5_f32, Color32::from_rgb(37, 99, 235)), Color32::from_rgb(30, 41, 59))
                                    }
                                } else {
                                    if is_dark {
                                        (Color32::from_rgb(28, 33, 44), Stroke::new(1.0_f32, Color32::from_rgb(45, 55, 72)), Color32::from_rgb(226, 232, 240))
                                    } else {
                                        (Color32::from_rgb(255, 255, 255), Stroke::new(1.0_f32, Color32::from_rgb(203, 213, 225)), Color32::from_rgb(30, 41, 59))
                                    }
                                };

                                painter.rect(key_rect, 4.0, key_fill, key_stroke);
                                let font_size = if vkey.name.len() > 3 {
                                    9.5
                                } else if vkey.name.len() > 2 {
                                    10.5
                                } else {
                                    12.0
                                };
                                painter.text(
                                    key_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    vkey.name,
                                    egui::FontId::proportional(font_size),
                                    text_color,
                                );

                                if let Some(sc) = scancode {
                                    resp.on_hover_text(format!("Assign '{}' (HID 0x{:02X})", vkey.name, sc));
                                }
                            }
                        });

                    if let Some(sc) = assign_scancode {
                        if let Some(k) = self.selected_key {
                            self.remap_single_key(k, KeyAction::standard_key(sc));
                        }
                    }
                }
                PaletteTab::Media => {
                    ui.horizontal_wrapped(|ui| {
                        let media_actions = [
                            ("Volume +", 0x00E9), ("Volume -", 0x00EA), ("Mute", 0x00E2),
                            ("Play/Pause", 0x00CD), ("Stop", 0xB7), ("Prev Track", 0xB6), ("Next Track", 0xB5),
                            ("Media", 0x0183), ("Calculator", 0x0192), ("My PC", 0x0194), ("Mail", 0x018A),
                            ("Homepage", 0x0223), ("Search", 0x0221), ("Web-Refresh", 0x0227), ("Web-Stop", 0x0226),
                            ("Web-Forward", 0x0225), ("Web-Backward", 0x0224), ("Web-Favorites", 0x022A),
                            ("Display Brt+", 0x006F), ("Display Brt-", 0x0070),
                        ];
                        for (label, usage) in media_actions {
                            if !action_query.is_empty() && !label.to_lowercase().contains(&action_query) {
                                continue;
                            }
                            if ui.button(label).clicked() {
                                if let Some(k) = self.selected_key {
                                    self.remap_single_key(k, KeyAction::multimedia(usage));
                                }
                            }
                        }
                    });
                }
                PaletteTab::Shortcut => {
                    ui.horizontal_wrapped(|ui| {
                        let shortcuts = [
                            ("CTRL_A", 0x01, 0x04),
                            ("CTRL_C", 0x01, 0x06),
                            ("CTRL_V", 0x01, 0x19),
                            ("CTRL_S", 0x01, 0x16),
                            ("CTRL_G", 0x01, 0x0A),
                            ("CTRL_Z", 0x01, 0x1D),
                        ];
                        for (label, modifier, scancode) in shortcuts {
                            if !action_query.is_empty() && !label.to_lowercase().contains(&action_query) {
                                continue;
                            }
                            if ui.button(label).clicked() {
                                if let Some(k) = self.selected_key {
                                    self.remap_single_key(k, KeyAction::combo(modifier, scancode));
                                }
                            }
                        }
                    });
                }
                PaletteTab::Custom => {
                    ui.horizontal_wrapped(|ui| {
                        let custom_actions = [
                            ("Led On/Off", KeyAction::hardware_custom(0), "Toggle backlighting LEDs on/off"),
                            ("LedMode", KeyAction::hardware_custom(3), "Cycle hardware lighting effect mode"),
                            ("Speed+", KeyAction::hardware_custom(5), "Increase lighting animation speed"),
                            ("Speed-", KeyAction::hardware_custom(6), "Decrease lighting animation speed"),
                            ("LedColor", KeyAction::hardware_custom(4), "Cycle lighting color palette"),
                            ("LedBrt+", KeyAction::hardware_custom(1), "Increase LED brightness level"),
                            ("LedBrt-", KeyAction::hardware_custom(2), "Decrease LED brightness level"),
                            ("GUI Lck", KeyAction::win_lock(), "Hardware Windows key lock toggle"),
                            ("HW Reset (Fn)", KeyAction::hardware_custom(16), "Hardware reset function (intended strictly for Fn+Esc, not Base layer)"),
                        ];
                        for (label, action, desc) in custom_actions {
                            if !action_query.is_empty()
                                && !label.to_lowercase().contains(&action_query)
                                && !desc.to_lowercase().contains(&action_query)
                            {
                                continue;
                            }
                            if ui.button(label).on_hover_text(desc).clicked() {
                                if let Some(k) = self.selected_key {
                                    self.remap_single_key(k, action);
                                }
                            }
                        }
                    });
                }
                PaletteTab::Macro => {
                    ui.horizontal_wrapped(|ui| {
                        for slot in 0..16 {
                            let label = format!("Macro M{:02}", slot);
                            if !action_query.is_empty() && !label.to_lowercase().contains(&action_query) {
                                continue;
                            }
                            if ui.button(&label).clicked() {
                                if let Some(k) = self.selected_key {
                                    self.remap_single_key(k, KeyAction::macro_trigger(slot as u8, 0, 1));
                                }
                            }
                        }
                    });
                }
            }
        });
    }


}
