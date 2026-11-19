//! Global keyboard shortcut handler and hotkey navigation.

use crate::*;

impl FantasTechApp {
    pub(crate) fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        if self.is_recording {
            return;
        }

        let wants_kb = ctx.wants_keyboard_input();

        // 1. Help Dialog Toggle: F1 or '?' (when not typing in a text field)
        let f1_pressed = ctx.input(|i| i.key_pressed(egui::Key::F1));
        let question_pressed = !wants_kb && ctx.input(|i| i.key_pressed(egui::Key::Questionmark));
        if f1_pressed || question_pressed {
            self.shortcuts_help_modal_open = !self.shortcuts_help_modal_open;
            return;
        }

        // 2. Escape: dismiss modals or clear selection
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.profile_modal_open {
                self.profile_modal_open = false;
            } else if self.delete_profile_modal_open {
                self.delete_profile_modal_open = false;
            } else if self.shortcuts_help_modal_open {
                self.shortcuts_help_modal_open = false;
            } else if self.permission_modal_open {
                self.permission_modal_open = false;
            } else if self.modify_modal_open {
                self.modify_modal_open = false;
            } else if self.factory_reset_modal_open {
                self.factory_reset_modal_open = false;
            } else if self.firmware_update_modal_open {
                self.firmware_update_modal_open = false;
            } else if self.auto_update_firmware_modal_open {
                self.auto_update_firmware_modal_open = false;
            } else if self.firmware_progress_modal_open && !self.firmware_flashing_in_progress {
                self.firmware_progress_modal_open = false;
            } else if self.current_tab == NavTab::Button && self.selected_key.is_some() {
                self.selected_key = None;
            } else if self.current_tab == NavTab::Lighting && (self.custom_select_all || !self.custom_selected_keys.is_empty()) {
                self.deselect_all_custom_keys();
            }
            return;
        }

        let has_open_modal = self.shortcuts_help_modal_open
            || self.permission_modal_open
            || self.modify_modal_open
            || self.factory_reset_modal_open
            || self.firmware_update_modal_open
            || self.auto_update_firmware_modal_open
            || self.firmware_progress_modal_open
            || self.profile_modal_open
            || self.delete_profile_modal_open;

        if has_open_modal {
            return;
        }

        let cmd = ctx.input(|i| i.modifiers.command || i.modifiers.ctrl);
        let shift = ctx.input(|i| i.modifiers.shift);

        // 3. Modifier-based shortcuts (Ctrl / Cmd)
        if cmd {
            // Tab Navigation: Ctrl+1..4
            if ctx.input(|i| i.key_pressed(egui::Key::Num1)) {
                self.current_tab = NavTab::Button;
                return;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Num2)) {
                self.current_tab = NavTab::Lighting;
                return;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Num3)) {
                self.current_tab = NavTab::Macro;
                return;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Num4)) {
                self.current_tab = NavTab::Setting;
                return;
            }

            // Tab Cycling: Ctrl+Tab (next), Ctrl+Shift+Tab (prev)
            if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
                let tabs = [NavTab::Button, NavTab::Lighting, NavTab::Macro, NavTab::Setting];
                let cur_idx = tabs.iter().position(|&t| t == self.current_tab).unwrap_or(0);
                if shift {
                    let prev_idx = if cur_idx == 0 { tabs.len() - 1 } else { cur_idx - 1 };
                    self.current_tab = tabs[prev_idx];
                } else {
                    let next_idx = (cur_idx + 1) % tabs.len();
                    self.current_tab = tabs[next_idx];
                }
                return;
            }

            // Save / Apply: Ctrl+S
            if ctx.input(|i| i.key_pressed(egui::Key::S)) {
                match self.current_tab {
                    NavTab::Button => {
                        self.apply_current_layer_to_hardware();
                        self.status_message = "Applied current button layer to keyboard (Ctrl+S)".to_string();
                    }
                    NavTab::Lighting => {
                        if self.selected_mode == 16 {
                            self.apply_color_to_custom_selection(self.current_color);
                            self.status_message = "Applied custom LED colors to keyboard (Ctrl+S)".to_string();
                        } else {
                            self.apply_lighting();
                            self.status_message = "Applied lighting configuration to keyboard (Ctrl+S)".to_string();
                        }
                    }
                    NavTab::Macro => {
                        self.flash_macro_slot();
                        self.status_message = format!("Flashed Macro Slot M{:02} to keyboard (Ctrl+S)", self.active_macro_slot);
                    }
                    NavTab::Setting => {
                        let _ = self.config.save();
                        self.status_message = "Saved settings to disk (Ctrl+S)".to_string();
                    }
                }
                return;
            }

            // Refresh Device Connection: Ctrl+R
            if ctx.input(|i| i.key_pressed(egui::Key::R)) {
                self.connection_monitor.refresh();
                self.connection_checking = true;
                self.status_message = "Checking device connection (Ctrl+R)…".to_string();
                return;
            }

            // Toggle GUI Theme: Ctrl+T
            if ctx.input(|i| i.key_pressed(egui::Key::T)) {
                let next_theme = match self.config.gui_theme {
                    GuiTheme::Light => GuiTheme::Dark,
                    GuiTheme::Dark => GuiTheme::Light,
                };
                self.config.gui_theme = next_theme;
                let _ = self.config.save();
                apply_gui_theme(ctx, next_theme);
                self.status_message = format!("Theme set to {:?} (Ctrl+T)", next_theme);
                return;
            }

            // Undo / Redo & Contextual shortcuts (only when not typing in text edit)
            if !wants_kb {
                if (ctx.input(|i| i.key_pressed(egui::Key::Z)) && shift)
                    || ctx.input(|i| i.key_pressed(egui::Key::Y))
                {
                    if self.current_tab == NavTab::Button {
                        self.redo_keymap_edit();
                    }
                    return;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Z)) {
                    if self.current_tab == NavTab::Button {
                        self.undo_keymap_edit();
                    }
                    return;
                }

                // Ctrl+L: Toggle layer (Buttons tab)
                if ctx.input(|i| i.key_pressed(egui::Key::L)) && self.current_tab == NavTab::Button {
                    self.active_layer = match self.active_layer {
                        KeyLayer::Base => KeyLayer::Fn,
                        KeyLayer::Fn => KeyLayer::Base,
                    };
                    self.undo_keymaps.clear();
                    self.redo_keymaps.clear();
                    self.status_message = format!("Switched to {:?} layer (Ctrl+L)", self.active_layer);
                    return;
                }

                // Ctrl+P: Cycle hardware profile (Buttons tab)
                if ctx.input(|i| i.key_pressed(egui::Key::P)) && self.current_tab == NavTab::Button {
                    let next_prof = (self.active_profile + 1) % 3;
                    self.set_active_profile(next_prof);
                    self.status_message = format!("Switched to Hardware Profile {} (Ctrl+P)", next_prof + 1);
                    return;
                }

                // Ctrl+A: Select all (Lighting tab, custom mode)
                if ctx.input(|i| i.key_pressed(egui::Key::A))
                    && self.current_tab == NavTab::Lighting
                    && self.selected_mode == 16
                {
                    if self.custom_select_all {
                        self.deselect_all_custom_keys();
                    } else {
                        self.select_all_custom_keys();
                    }
                    return;
                }
            }
        }

        // 4. Non-modifier canvas navigation & single-key controls (only when not typing in a text field)
        if wants_kb {
            return;
        }

        // Buttons Tab Canvas Navigation & Actions
        if self.current_tab == NavTab::Button {
            let dir = if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                Some(ArrowDirection::Up)
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                Some(ArrowDirection::Down)
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                Some(ArrowDirection::Left)
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                Some(ArrowDirection::Right)
            } else {
                None
            };

            if let Some(d) = dir {
                let cur = self.selected_key.unwrap_or(0);
                let next_key = find_neighbor_key(cur, d);
                self.selected_key = Some(next_key);
                return;
            }

            if let Some(key_idx) = self.selected_key {
                if ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space)) {
                    self.modify_modal_open = true;
                    return;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                    let default_actions = default_layer_keymap(self.active_layer);
                    self.remap_single_key(key_idx, default_actions[key_idx as usize]);
                    self.status_message = format!("Restored key {} to default action", key_idx);
                    return;
                }
            }
        }

        // Lighting Tab Canvas Navigation & Actions (Custom Mode)
        if self.current_tab == NavTab::Lighting && self.selected_mode == 16 {
            let dir = if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                Some(ArrowDirection::Up)
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                Some(ArrowDirection::Down)
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                Some(ArrowDirection::Left)
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                Some(ArrowDirection::Right)
            } else {
                None
            };

            if let Some(d) = dir {
                let current_lead = self.custom_selected_keys.iter().copied().last().unwrap_or(0);
                let next_key = find_neighbor_key(current_lead, d);
                if shift {
                    self.custom_selected_keys.insert(next_key);
                } else {
                    self.custom_selected_keys.clear();
                    self.custom_selected_keys.insert(next_key);
                    self.custom_select_all = false;
                }
                return;
            }

            // Space / Enter: Paint selected key(s) with current color
            if ctx.input(|i| i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::Enter)) {
                if !self.custom_selected_keys.is_empty() {
                    self.apply_color_to_custom_selection(self.current_color);
                }
                return;
            }

            // Delete / Backspace: Turn off LED on selected key(s)
            if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                self.turn_off_custom_selection();
                return;
            }

            // C: Copy color from active selected key
            if ctx.input(|i| i.key_pressed(egui::Key::C)) {
                if let Some(&k) = self.custom_selected_keys.iter().last() {
                    let col = self.key_colors.get(k as usize).copied().unwrap_or(Color32::BLACK);
                    self.copied_color = Some([col.r(), col.g(), col.b()]);
                    self.status_message = format!("Copied key color #{:02X}{:02X}{:02X} (C)", col.r(), col.g(), col.b());
                }
                return;
            }

            // V: Paste copied color to selected keys
            if ctx.input(|i| i.key_pressed(egui::Key::V)) {
                if let Some(col) = self.copied_color {
                    self.apply_color_to_custom_selection(col);
                    self.status_message = format!("Pasted color #{:02X}{:02X}{:02X} to selection (V)", col[0], col[1], col[2]);
                }
                return;
            }
        }

        // Macro Tab: F9 toggle recording
        if self.current_tab == NavTab::Macro && ctx.input(|i| i.key_pressed(egui::Key::F9)) {
            self.is_recording = true;
            self.recording_last_time = None;
            self.status_message = "Macro recording started (Press Esc to stop)".to_string();
        }
    }


}
