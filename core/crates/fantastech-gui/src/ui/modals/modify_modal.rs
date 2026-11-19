//! Key remap and modify modal dialog.

use crate::*;

impl FantasTechApp {
    pub(crate) fn render_modify_modal(&mut self, ctx: &egui::Context) {
        let selected_key_idx = match self.selected_key {
            Some(idx) => idx,
            None => {
                self.modify_modal_open = false;
                return;
            }
        };
        let key_name = KEY_DEFINITIONS.iter().find(|k| k.index == selected_key_idx).map(|k| k.name).unwrap_or("Unknown");

        egui::Window::new(RichText::new(format!("Modify key: {} (slot {})", key_name, selected_key_idx)).strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(450.0);

                if !ctx.wants_keyboard_input() {
                    if ctx.input(|i| i.key_pressed(egui::Key::Num1)) {
                        self.modify_tab = ModifyTab::SystemButton;
                    } else if ctx.input(|i| i.key_pressed(egui::Key::Num2)) {
                        self.modify_tab = ModifyTab::KeyboardKeys;
                    } else if ctx.input(|i| i.key_pressed(egui::Key::Num3)) {
                        self.modify_tab = ModifyTab::MacroButton;
                    }
                }

                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(self.modify_tab == ModifyTab::SystemButton, "System button (1)").clicked() {
                        self.modify_tab = ModifyTab::SystemButton;
                    }
                    if ui.selectable_label(self.modify_tab == ModifyTab::KeyboardKeys, "Keyboard keys (2)").clicked() {
                        self.modify_tab = ModifyTab::KeyboardKeys;
                    }
                    if ui.selectable_label(self.modify_tab == ModifyTab::MacroButton, "Macro button (3)").clicked() {
                        self.modify_tab = ModifyTab::MacroButton;
                    }
                });

                ui.separator();

                match self.modify_tab {
                    ModifyTab::SystemButton => {
                        ui.label(RichText::new("Select System, Multimedia, or Power Action:").strong());
                        ui.add_space(4.0);

                        egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                            ui.label(RichText::new("Shortcuts").color(Color32::from_rgb(0, 180, 216)).strong());
                            let shortcuts = [
                                ("CTRL_A", KeyAction::combo(0x01, 0x04)),
                                ("CTRL_C", KeyAction::combo(0x01, 0x06)),
                                ("CTRL_V", KeyAction::combo(0x01, 0x19)),
                                ("CTRL_S", KeyAction::combo(0x01, 0x16)),
                                ("CTRL_G", KeyAction::combo(0x01, 0x0A)),
                                ("CTRL_Z", KeyAction::combo(0x01, 0x1D)),
                            ];
                            ui.horizontal_wrapped(|ui| {
                                for (lbl, act) in shortcuts {
                                    let is_sel = self.selected_system_action == Some(act);
                                    if ui.selectable_label(is_sel, lbl).clicked() {
                                        self.selected_system_action = Some(act);
                                    }
                                }
                            });

                            ui.add_space(8.0);
                            ui.label(RichText::new("Multimedia controls").color(Color32::from_rgb(0, 180, 216)).strong());
                            let media = [
                                ("Volume +", 0x00E9), ("Volume -", 0x00EA), ("Mute", 0x00E2),
                                ("Play/Pause", 0x00CD), ("Stop", 0xB7), ("Prev Track", 0xB6), ("Next Track", 0xB5),
                                ("Media", 0x0183), ("Calculator", 0x0192), ("My PC", 0x0194), ("Mail", 0x018A),
                                ("Homepage", 0x0223), ("Search", 0x0221), ("Web-Refresh", 0x0227), ("Web-Stop", 0x0226),
                                ("Web-Forward", 0x0225), ("Web-Backward", 0x0224), ("Web-Favorites", 0x022A),
                                ("Display Brt+", 0x006F), ("Display Brt-", 0x0070),
                            ];
                            ui.horizontal_wrapped(|ui| {
                                for (lbl, usage) in media {
                                    let act = KeyAction::multimedia(usage);
                                    let is_sel = self.selected_system_action == Some(act);
                                    if ui.selectable_label(is_sel, lbl).clicked() {
                                        self.selected_system_action = Some(act);
                                    }
                                }
                            });

                            ui.add_space(8.0);
                            ui.label(RichText::new("ACPI power and functions").color(Color32::from_rgb(0, 180, 216)).strong());
                            let system_acts = [
                                ("Power", KeyAction::power(1)),
                                ("Sleep", KeyAction::power(2)),
                                ("Wakeup", KeyAction::power(4)),
                                ("WinLock (GUI Lock)", KeyAction::win_lock()),
                                ("Disable key", KeyAction::disabled()),
                            ];
                            ui.horizontal_wrapped(|ui| {
                                for (lbl, act) in system_acts {
                                    let is_sel = self.selected_system_action == Some(act);
                                    if ui.selectable_label(is_sel, lbl).clicked() {
                                        self.selected_system_action = Some(act);
                                    }
                                }
                            });
                        });

                        ui.add_space(8.0);
                        if let Some(act) = self.selected_system_action {
                            ui.label(RichText::new(format!("Selected: {}", format_action_str(&act))).strong());
                        } else {
                            ui.label(RichText::new("Click an action above to select it.").italics());
                        }

                        ui.add_space(10.0);
                        ui.horizontal_wrapped(|ui| {
                            let can_save = self.selected_system_action.is_some();
                            let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.add_enabled(can_save, egui::Button::new("Save (Enter)")).clicked()
                                || (can_save && enter_pressed)
                            {
                                if let Some(act) = self.selected_system_action {
                                    self.remap_single_key(selected_key_idx, act);
                                    self.modify_modal_open = false;
                                }
                            }
                            if ui.button("Cancel (Esc)").clicked() {
                                self.modify_modal_open = false;
                            }
                        });
                    }
                    ModifyTab::KeyboardKeys => {
                        ui.label(RichText::new("Combo Modifiers:").strong());
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut self.combo_ctrl, "Ctrl");
                            ui.checkbox(&mut self.combo_shift, "Shift");
                            ui.checkbox(&mut self.combo_alt, "Alt");
                            ui.checkbox(&mut self.combo_win, "Win");
                        });

                        ui.add_space(8.0);
                        ui.label("Target Key (Letter, Digit, or Name):");
                        ui.text_edit_singleline(&mut self.combo_key_str);

                        let mut modifier_mask = 0u8;
                        if self.combo_ctrl { modifier_mask |= 0x01; }
                        if self.combo_shift { modifier_mask |= 0x02; }
                        if self.combo_alt { modifier_mask |= 0x04; }
                        if self.combo_win { modifier_mask |= 0x08; }

                        let scancode = scancode_for_key_name(&self.combo_key_str).unwrap_or(0x06);
                        let action = KeyAction::combo(modifier_mask, scancode);

                        ui.add_space(8.0);
                        ui.label(format!("Constructed Action: {}", format_action_str(&action)));

                        ui.add_space(10.0);
                        ui.horizontal_wrapped(|ui| {
                            let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.button("Save (Enter)").clicked() || enter_pressed {
                                self.remap_single_key(selected_key_idx, action);
                                self.modify_modal_open = false;
                            }
                            if ui.button("Cancel (Esc)").clicked() {
                                self.modify_modal_open = false;
                            }
                        });
                    }
                    ModifyTab::MacroButton => {
                        ui.label(RichText::new("Macro Trigger Assignment:").strong());
                        egui::ComboBox::from_label("Macro Slot")
                            .selected_text(format!("Macro M{:02}", self.macro_modal_slot))
                            .show_ui(ui, |ui| {
                                for s in 0..16 {
                                    ui.selectable_value(&mut self.macro_modal_slot, s, format!("Macro M{:02}", s));
                                }
                            });

                        ui.add_space(6.0);
                        ui.label(RichText::new("Loop Execution Mode:").strong());
                        ui.radio_value(&mut self.macro_modal_loop_mode, 0, "Loop specified times");
                        if self.macro_modal_loop_mode == 0 {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Times:");
                                ui.add(egui::DragValue::new(&mut self.macro_modal_repeat).range(1..=255));
                            });
                        }
                        ui.radio_value(&mut self.macro_modal_loop_mode, 1, "Loop until key is released (Hold to loop)");
                        ui.radio_value(&mut self.macro_modal_loop_mode, 2, "Loop until any key is pressed");
                        ui.radio_value(&mut self.macro_modal_loop_mode, 3, "Toggle loop (Press to start, press to stop)");

                        let action = KeyAction::macro_trigger(self.macro_modal_slot as u8, self.macro_modal_loop_mode, self.macro_modal_repeat);

                        ui.add_space(8.0);
                        ui.label(format!("Constructed Action: {}", format_action_str(&action)));

                        ui.add_space(10.0);
                        ui.horizontal_wrapped(|ui| {
                            let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.button("Save (Enter)").clicked() || enter_pressed {
                                self.remap_single_key(selected_key_idx, action);
                                self.modify_modal_open = false;
                            }
                            if ui.button("Cancel (Esc)").clicked() {
                                self.modify_modal_open = false;
                            }
                        });
                    }
                }
            });
    }


}
