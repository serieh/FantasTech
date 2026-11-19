//! Device settings, sleep timeout, and maintenance page.

use crate::*;
use crate::ui::widgets::*;

impl FantasTechApp {
    pub(crate) fn render_setting_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings & maintenance");
        ui.separator();

        // Dry-run safe mode toggle
        ui.horizontal(|ui| {
            let safe_color = if self.dry_run_enabled {
                Color32::from_rgb(34, 197, 94)
            } else {
                Color32::from_rgb(148, 163, 184)
            };
            ui.checkbox(&mut self.dry_run_enabled, RichText::new("Dry-run safe mode").color(safe_color).strong());
            ui.label(RichText::new("(Simulate operations without writing to keyboard hardware)").weak());
        });
        ui.add_space(4.0);
        ui.separator();

        // 1. Background Service Control Panel
        ui.heading("FantasTech Background Service (Daemon)");

        ui.horizontal(|ui| {
            if self.service_active {
                draw_status_dot(ui, Color32::from_rgb(34, 197, 94));
                ui.label(RichText::new("Service Status: RUNNING").color(Color32::from_rgb(34, 197, 94)).strong());
            } else {
                draw_status_dot(ui, Color32::from_rgb(239, 68, 68));
                ui.label(RichText::new("Service Status: STOPPED").color(Color32::from_rgb(239, 68, 68)).strong());
            }

            if ui.button("Refresh").clicked() {
                self.connection_checking = true;
                self.connection_monitor.refresh();
                self.status_message = "Refreshing connection state…".to_owned();
            }
        });

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Start service").clicked() {
                self.status_message = platform::service_action("start")
                    .map(|_| {
                        self.connection_monitor.refresh();
                        self.connection_checking = true;
                        "Background service started".to_owned()
                    })
                    .unwrap_or_else(|error| error);
            }
            if ui.button("Stop service").clicked() {
                if self.service_active {
                    let _ = send_ipc_request(&IpcRequest::ShutdownService);
                }
                self.status_message = platform::service_action("stop")
                    .map(|_| {
                        self.connection_monitor.refresh();
                        self.connection_checking = true;
                        "Background service stopped".to_owned()
                    })
                    .unwrap_or_else(|error| error);
            }
            if ui.button("Restart service").clicked() {
                self.status_message = platform::service_action("restart")
                    .map(|_| {
                        self.connection_monitor.refresh();
                        self.connection_checking = true;
                        "Background service restarted".to_owned()
                    })
                    .unwrap_or_else(|error| error);
            }
        });

        ui.add_space(4.0);
        ui.label(RichText::new("Background Service Features:").strong());

        let mut config_dirty = false;
        #[cfg(target_os = "linux")]
        if ui.checkbox(&mut self.config.guard_volume_enabled, "Volume Knob Guard Debouncer (Protects against rapid spike/drop artifacts)").changed() {
            config_dirty = true;
            if self.service_active {
                let _ = send_ipc_request(&IpcRequest::SetVolumeGuardEnabled(self.config.guard_volume_enabled));
            }
        }

        ui.horizontal_wrapped(|ui| {
            if ui.checkbox(&mut self.config.openrgb_enabled, "OpenRGB TCP SDK Server").changed() {
                config_dirty = true;
                if self.service_active {
                    let _ = send_ipc_request(&IpcRequest::SetOpenRgbEnabled(self.config.openrgb_enabled));
                }
            }
            ui.label("Host:");
            if ui.text_edit_singleline(&mut self.config.openrgb_host).changed() {
                config_dirty = true;
            }
            ui.label("Port:");
            if ui.add(egui::DragValue::new(&mut self.config.openrgb_port)).changed() {
                config_dirty = true;
            }
        });

        ui.horizontal_wrapped(|ui| {
            if ui.checkbox(&mut self.config.auto_apply_lighting, "Auto-apply lighting profile on startup (when OpenRGB is off)").changed() {
                config_dirty = true;
            }
            if self.config.auto_apply_lighting {
                ui.label("Profile:");
                let lighting_profiles = self.profile_manager.list_lighting_profiles();
                egui::ComboBox::from_id_salt("startup_lighting_profile_selector")
                    .selected_text(&self.config.active_lighting_profile)
                    .show_ui(ui, |ui| {
                        for lp in &lighting_profiles {
                            if ui.selectable_value(&mut self.config.active_lighting_profile, lp.id.clone(), &lp.name).clicked() {
                                config_dirty = true;
                            }
                        }
                    });
            }
        });

        if ui.checkbox(&mut self.config.auto_start_enabled, "Auto-start background service on system login").changed() {
            config_dirty = true;
            let action = if self.config.auto_start_enabled { "enable" } else { "disable" };
            if let Err(error) = platform::service_action(action) {
                self.status_message = error;
            } else {
                self.status_message = if self.config.auto_start_enabled {
                    "Enabled auto-start on system login".to_string()
                } else {
                    "Disabled auto-start on system login".to_string()
                };
            }
        }

        ui.horizontal_wrapped(|ui| {
            if ui.checkbox(&mut self.config.tray_icon_enabled, "Enable System Tray Applet").changed() {
                config_dirty = true;
            }
            if ui.button("Launch tray applet").clicked() {
                self.status_message = platform::launch_tray_applet()
                    .map(|_| "Launched FantasTech tray applet".to_owned())
                    .unwrap_or_else(|error| error);
            }
        });

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Color Code Display Format:").strong());
            let fmt_name = match self.config.color_display_format {
                ColorDisplayFormat::Hex => "HEX (#RRGGBB)",
                ColorDisplayFormat::Rgb => "RGB (R, G, B)",
                ColorDisplayFormat::Hsv => "HSV (H°, S%, V%)",
                ColorDisplayFormat::Hsl => "HSL (H°, S%, L%)",
            };
            egui::ComboBox::from_id_salt("setting_color_format_selector")
                .selected_text(fmt_name)
                .show_ui(ui, |ui| {
                    if ui.selectable_value(&mut self.config.color_display_format, ColorDisplayFormat::Hex, "HEX (#RRGGBB)").clicked() {
                        config_dirty = true;
                    }
                    if ui.selectable_value(&mut self.config.color_display_format, ColorDisplayFormat::Rgb, "RGB (R, G, B)").clicked() {
                        config_dirty = true;
                    }
                    if ui.selectable_value(&mut self.config.color_display_format, ColorDisplayFormat::Hsv, "HSV (H°, S%, V%)").clicked() {
                        config_dirty = true;
                    }
                    if ui.selectable_value(&mut self.config.color_display_format, ColorDisplayFormat::Hsl, "HSL (H°, S%, L%)").clicked() {
                        config_dirty = true;
                    }
                });
        });

        if config_dirty {
            let _ = self.config.save();
            if self.service_active {
                let _ = send_ipc_request(&IpcRequest::SaveConfig(self.config.clone()));
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Hardware Information
        ui.heading("Device Hardware Information");
        ui.label(format!("Device: {}", self.device_model));
        ui.label(format!("Firmware Revision: {}", self.firmware_version));
        ui.label("Vendor ID: 0x36AE • Product ID: 0xFE9C");
        ui.label("USB Hardware Interface: Interface 2 (Report ID 0x06, 64-Byte Custom HID)");
        ui.label("Target Architecture: Artery SDCX AK107 MCU");

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // 4. Firmware Update
        ui.heading("Firmware Update");
        ui.horizontal(|ui| {
            if self.update_available {
                draw_status_dot(ui, Color32::from_rgb(245, 158, 11)); // Amber
                ui.label(RichText::new(&self.firmware_status_text).color(Color32::from_rgb(245, 158, 11)).strong());
            } else {
                draw_status_dot(ui, Color32::from_rgb(34, 197, 94)); // Green
                ui.label(RichText::new(&self.firmware_status_text).color(Color32::from_rgb(34, 197, 94)).strong());
            }
        });

        ui.add_space(4.0);
        let is_checking = if let Some(start) = self.checking_updates_start {
            if start.elapsed() < std::time::Duration::from_secs(2) {
                ui.ctx().request_repaint();
                true
            } else {
                self.checking_updates_start = None;
                false
            }
        } else {
            false
        };

        ui.horizontal_wrapped(|ui| {
            let check_btn = egui::Button::new(if is_checking { "Checking..." } else { "Check for updates" });
            if ui.add_enabled(!is_checking, check_btn)
                .on_hover_text("Query latest firmware release")
                .clicked()
            {
                self.checking_updates_start = Some(std::time::Instant::now());
                self.check_for_firmware_updates();
                ui.ctx().request_repaint();
            }

            let update_btn = egui::Button::new(
                RichText::new("Update Firmware")
                    .color(Color32::WHITE)
                    .strong(),
            ).fill(Color32::from_rgb(0, 113, 207));

            if ui.add_enabled(self.update_available || self.custom_firmware_path.is_some(), update_btn)
                .on_hover_text("Open confirmation dialog and begin firmware update")
                .clicked()
            {
                self.firmware_update_modal_open = true;
            }

            if ui.button("Select custom firmware (.bin)...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Firmware Binary (*.bin)", &["bin"])
                    .pick_file()
                {
                    self.custom_firmware_path = Some(path);
                    self.validate_custom_firmware();
                }
            }

            if let Some(ref path) = self.custom_firmware_path {
                ui.label(RichText::new(format!("Custom: {}", path.display())).color(Color32::from_rgb(0, 113, 207)).small());
                if ui.small_button("Clear").clicked() {
                    self.custom_firmware_path = None;
                    self.check_for_firmware_updates();
                }
            }
        });

        ui.add_space(6.0);
        let mut cfg_dirty = false;
        if ui.checkbox(&mut self.config.auto_check_updates, "Automatically check for updates on startup (Default: On)").changed() {
            cfg_dirty = true;
        }
        let mut auto_fw_val = self.config.auto_update_firmware;
        if ui.checkbox(&mut auto_fw_val, "Automatically update firmware (Default: Off)").changed() {
            if auto_fw_val {
                // Must show warning modal and require confirmation before turning on
                self.auto_update_firmware_modal_open = true;
                self.auto_update_modal_just_opened = true;
            } else {
                self.config.auto_update_firmware = false;
                cfg_dirty = true;
            }
        }
        if ui.checkbox(&mut self.config.update_notifications_enabled, "Get desktop notification when update is available (Default: Off)").changed() {
            cfg_dirty = true;
        }
        if cfg_dirty {
            let _ = self.config.save();
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // 5. Hardware Sleep Timeout
        ui.heading("Hardware Sleep & Inactivity Timeout");
        ui.label("Configure automatic MCU low-power sleep timeout on USB inactivity:");
        ui.horizontal_wrapped(|ui| {
            let prev_timeout = self.sleep_timeout_mins;
            egui::ComboBox::from_id_salt("sleep_combo")
                .selected_text(match self.sleep_timeout_mins {
                    0 => "Never sleep (Disabled)".to_string(),
                    m => format!("{} minutes", m),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sleep_timeout_mins, 0, "Never sleep (0m)");
                    ui.selectable_value(&mut self.sleep_timeout_mins, 5, "5 minutes (300s)");
                    ui.selectable_value(&mut self.sleep_timeout_mins, 10, "10 minutes (600s)");
                    ui.selectable_value(&mut self.sleep_timeout_mins, 30, "30 minutes (1800s)");
                });

            if self.sleep_timeout_mins != prev_timeout {
                self.apply_sleep_timeout(self.sleep_timeout_mins);
            }

            if ui.button("Apply Sleep Timeout").clicked() {
                self.apply_sleep_timeout(self.sleep_timeout_mins);
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // 5. Full Device Backup & Restore
        ui.heading("Full Device Backup & Restore");
        ui.label("Save or restore complete keyboard memory (profiles, lighting, keymaps, and 4KB macros) in a single file.");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Export keyboard backup…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("fantastech_mk894_backup.json")
                    .add_filter("Keyboard Backup (*.json)", &["json"])
                    .save_file()
                {
                    self.backup_file_path = path.to_string_lossy().to_string();
                    self.export_backup_file();
                }
            }
            if ui.button("Restore keyboard backup…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Keyboard Backup (*.json)", &["json"])
                    .pick_file()
                {
                    self.backup_file_path = path.to_string_lossy().to_string();
                    self.restore_backup_file();
                }
            }
            if !self.backup_file_path.is_empty() {
                ui.label(RichText::new(format!("File: {}", self.backup_file_path)).color(Color32::GRAY).small());
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // 6. Factory Reset
        ui.heading("Factory Reset (Non-Volatile Flash Erase)");
        ui.label("Restores MCU non-volatile flash to factory defaults (default rainbow spectrum and standard 104-key ANSI layout).");
        if ui.button(RichText::new("Factory Reset Keyboard...").color(Color32::from_rgb(255, 75, 75))).clicked() {
            self.factory_reset_modal_open = true;
            self.factory_reset_input.clear();
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // 7. About & Credits
        ui.heading("About FantasTech");
        ui.label(RichText::new("FantasTech MK894 Unified Keyboard Driver & Control Suite v0.1.5").strong());
        ui.label("Author & Maintainer: Ahmad Serieh");
        ui.label("License: Apache-2.0 Open Source");
    }


}
