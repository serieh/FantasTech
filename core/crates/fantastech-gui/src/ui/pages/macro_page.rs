//! Macro studio page and event sequence editor.

use crate::*;

impl FantasTechApp {
    pub(crate) fn render_macro_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Macro studio");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Erase onboard macros").clicked() {
                    self.wipe_all_macros();
                }
                ui.label(RichText::new(format!("M{:02}", self.active_macro_slot)).color(Color32::from_rgb(0, 180, 216)).strong());
                ui.label(RichText::new("Active Slot:").strong());
                ui.separator();
                if ui.button("Load profile…").on_hover_text("Open file picker to select and load a profile JSON file").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Profile (*.json)", &["json"])
                        .pick_file()
                    {
                        self.import_macro_profile_from_path(&path);
                    }
                }
                if ui.button("Save profile…").on_hover_text("Open file dialog to save current macro profile to JSON (editable file name)").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("macro_profile.json")
                        .add_filter("Macro Profile (*.json)", &["json"])
                        .save_file()
                    {
                        self.export_macro_profile_to_path(&path);
                    }
                }
            });
        });
        ui.separator();

        ui.horizontal_wrapped(|ui| {
            // Left: Macro Slot List (M0..M15)
            ui.vertical(|ui| {
                ui.set_width(180.0);
                ui.heading("Macro Slots (16)");
                ui.separator();

                for slot in 0..16 {
                    let is_active = self.active_macro_slot == slot;
                    let label = format!("Slot M{:02}", slot);
                    if ui.selectable_label(is_active, label).clicked() {
                        self.active_macro_slot = slot;
                        self.macro_sequence.slot = slot;
                        self.macro_sequence.name = format!("Macro {}", slot);
                    }
                }

                ui.add_space(10.0);
                if ui.button("Erase onboard macros").clicked() {
                    self.wipe_all_macros();
                }
            });

            ui.separator();

            // Right: Macro Editor & Step Sequence Table
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading(format!("Macro Editor: Slot M{:02}", self.active_macro_slot));
                    let flash_label = format!("Flash to Slot M{:02}", self.active_macro_slot);
                    if ui.button(RichText::new(flash_label).color(Color32::WHITE).strong()).clicked() {
                        self.flash_macro_slot();
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label("Macro Name:");
                    ui.text_edit_singleline(&mut self.macro_sequence.name);

                    ui.separator();

                    if self.is_recording {
                        if ui.button(RichText::new("Stop recording").color(Color32::WHITE).strong()).clicked() {
                            self.is_recording = false;
                            self.recording_last_time = None;
                            if let Some(last) = self.macro_sequence.steps.last() {
                                if last.action == "mouse_down" && last.code == 0x01 {
                                    self.macro_sequence.steps.pop();
                                }
                            }
                            self.status_message = format!("Macro recording stopped ({} steps)", self.macro_sequence.steps.len());
                        }
                        ui.label(
                            RichText::new("RECORDING — type keys or press Escape to finish")
                                .color(Color32::from_rgb(255, 75, 75))
                                .strong(),
                        );
                    } else if ui.button(RichText::new("Start recording").color(Color32::from_rgb(255, 75, 75)).strong()).clicked() {
                        self.is_recording = true;
                        self.recording_last_time = Some(std::time::Instant::now());
                        self.status_message = "Recording live keystrokes and mouse clicks. Press Escape or Stop Recording to finish.".to_string();
                    }

                    ui.separator();
                    ui.checkbox(&mut self.fixed_delay_enabled, "Fixed Delay");
                    if self.fixed_delay_enabled {
                        ui.add(Slider::new(&mut self.fixed_delay_ms, 1..=500).suffix(" ms"));
                    }
                });

                ui.separator();

                // Step sequence table
                ui.label(RichText::new("Sequential Actions:").strong());
                let steps_len = self.macro_sequence.steps.len();
                let mut move_up = None;
                let mut move_down = None;
                let mut remove_idx = None;

                egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                    for (i, step) in self.macro_sequence.steps.iter_mut().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!("#{:02}", i + 1));

                            if ui.add_enabled(i > 0, egui::Button::new("⬆")).clicked() {
                                move_up = Some(i);
                            }
                            if ui.add_enabled(i + 1 < steps_len, egui::Button::new("⬇")).clicked() {
                                move_down = Some(i);
                            }

                            egui::ComboBox::from_id_salt(format!("action_{}", i))
                                .selected_text(&step.action)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut step.action, "down".to_string(), "Key Press (Down)");
                                    ui.selectable_value(&mut step.action, "up".to_string(), "Key Release (Up)");
                                    ui.selectable_value(&mut step.action, "click".to_string(), "Key Click");
                                    ui.selectable_value(&mut step.action, "mouse_down".to_string(), "Mouse Press");
                                    ui.selectable_value(&mut step.action, "mouse_up".to_string(), "Mouse Release");
                                    ui.selectable_value(&mut step.action, "wheel_v".to_string(), "Wheel Scroll");
                                });

                            let key_name = step.name.as_deref().unwrap_or_else(|| key_name_for_scancode(step.code).unwrap_or("Unknown"));
                            ui.label(format!("Code: 0x{:02X} ({})", step.code, key_name));

                            ui.label("Delay:");
                            ui.add(Slider::new(&mut step.delay_ms, 1..=500).suffix(" ms"));

                            if ui.button("Remove").clicked() {
                                remove_idx = Some(i);
                            }
                        });
                    }
                });

                if let Some(i) = move_up {
                    self.macro_sequence.steps.swap(i, i - 1);
                }
                if let Some(i) = move_down {
                    self.macro_sequence.steps.swap(i, i + 1);
                }
                if let Some(idx) = remove_idx {
                    self.macro_sequence.steps.remove(idx);
                }

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("+ Add Key Press").clicked() {
                        self.macro_sequence.steps.push(MacroItem {
                            action: "down".to_string(),
                            scancode: 0x04, // A
                            code: 0x04,
                            name: Some("A".to_string()),
                            delay_ms: 50,
                        });
                    }
                    if ui.button("+ Add Key Release").clicked() {
                        self.macro_sequence.steps.push(MacroItem {
                            action: "up".to_string(),
                            scancode: 0x04,
                            code: 0x04,
                            name: Some("A".to_string()),
                            delay_ms: 25,
                        });
                    }
                    if ui.button("+ Add Mouse Click").clicked() {
                        self.macro_sequence.steps.push(MacroItem {
                            action: "mouse_down".to_string(),
                            scancode: 0x01,
                            code: 0x01,
                            name: Some("LeftClick".to_string()),
                            delay_ms: 30,
                        });
                        self.macro_sequence.steps.push(MacroItem {
                            action: "mouse_up".to_string(),
                            scancode: 0x01,
                            code: 0x01,
                            name: Some("LeftClick".to_string()),
                            delay_ms: 30,
                        });
                    }
                    if ui.button("Clear all steps").clicked() {
                        self.macro_sequence.steps.clear();
                    }
                });
            });
        });
    }


}
