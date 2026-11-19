//! Firmware update detection, validation, confirmation, and flashing progress modals.

use crate::*;

impl FantasTechApp {
    pub(crate) fn check_for_firmware_updates(&mut self) {
        let catalog = fantastech_core::get_catalog_entry();
        self.latest_firmware_version = catalog.version_str.clone();

        let is_avail = fantastech_core::check_firmware_update_available(&self.firmware_version);
        if is_avail {
            self.update_available = true;
            self.firmware_status_text = format!("Update available: {} -> {}", self.firmware_version, catalog.version_str);
            self.status_message = format!("Firmware update available: {}", catalog.version_str);
        } else {
            self.update_available = false;
            let display_ver = if self.firmware_version.is_empty() {
                "V64".to_string()
            } else {
                self.firmware_version.clone()
            };
            self.firmware_status_text = format!("Firmware is up to date ({})", display_ver);
        }
    }

    pub(crate) fn validate_custom_firmware(&mut self) {
        if let Some(ref path) = self.custom_firmware_path {
            match std::fs::read(path) {
                Ok(bytes) => {
                    match fantastech_core::validate_firmware_binary(&bytes) {
                        Ok(info) => {
                            self.update_available = true;
                            self.firmware_status_text = format!(
                                "Custom firmware valid: {} bytes ({})",
                                info.file_size,
                                info.detected_version_str.as_deref().unwrap_or("Ready")
                            );
                        }
                        Err(e) => {
                            self.firmware_status_text = format!("Invalid firmware: {}", e);
                            self.custom_firmware_path = None;
                        }
                    }
                }
                Err(e) => {
                    self.firmware_status_text = format!("Error reading file: {}", e);
                    self.custom_firmware_path = None;
                }
            }
        }
    }

    pub(crate) fn start_firmware_flashing(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.firmware_progress_rx = Some(rx);

        let binary = if let Some(ref path) = self.custom_firmware_path {
            match std::fs::read(path) {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx.send(fantastech_device::FirmwareProgressUpdate {
                        phase: fantastech_device::FirmwarePhase::Failed,
                        step: 1,
                        total_steps: 7,
                        chunk: 0,
                        total_chunks: 480,
                        percentage: 0.0,
                        message: format!("Failed reading custom binary: {}", e),
                    });
                    return;
                }
            }
        } else {
            fantastech_core::get_embedded_firmware_bytes().to_vec()
        };

        let dry_run = self.dry_run_enabled;
        let is_emu = self.is_emulator;

        std::thread::spawn(move || {
            let (token, _info) = match fantastech_device::FirmwareUpdateSafetyToken::authorize(
                fantastech_device::FirmwareUpdateSafetyToken::MANDATORY_CONFIRMATION,
                &binary,
            ) {
                Ok(t) => t,
                Err(e) => {
                    let _ = tx.send(fantastech_device::FirmwareProgressUpdate {
                        phase: fantastech_device::FirmwarePhase::Failed,
                        step: 1,
                        total_steps: 7,
                        chunk: 0,
                        total_chunks: 480,
                        percentage: 0.0,
                        message: format!("Safety token rejection: {}", e),
                    });
                    return;
                }
            };

            if is_emu || dry_run {
                let mut mock = fantastech_transport::MockTransport::new();
                let _ = fantastech_device::execute_firmware_update(
                    &mut mock,
                    &token,
                    &binary,
                    true,
                    |update| {
                        let _ = tx.send(update);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    },
                );
            } else {
                match fantastech_transport::HidrawTransport::discover() {
                    Ok(mut transport) => {
                        let _ = fantastech_device::execute_firmware_update(
                            &mut transport,
                            &token,
                            &binary,
                            false,
                            |update| {
                                let _ = tx.send(update);
                            },
                        );
                    }
                    Err(e) => {
                        let _ = tx.send(fantastech_device::FirmwareProgressUpdate {
                            phase: fantastech_device::FirmwarePhase::Failed,
                            step: 1,
                            total_steps: 7,
                            chunk: 0,
                            total_chunks: 480,
                            percentage: 0.0,
                            message: format!("USB transport error: {}", e),
                        });
                    }
                }
            }
        });
    }

    pub(crate) fn render_firmware_confirmation_modal(&mut self, ctx: &egui::Context) {
        let is_dark = self.config.gui_theme == GuiTheme::Dark;
        egui::Window::new("Confirm Firmware Update & Liability Disclaimer")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.heading(RichText::new("CRITICAL: Microcontroller Flashing").color(Color32::from_rgb(239, 68, 68)));
                ui.separator();
                ui.add_space(4.0);

                ui.label(RichText::new("You are about to flash new microcontroller firmware to your Fantech MK894 keyboard.").strong());
                ui.add_space(6.0);

                ui.label(RichText::new("CRITICAL PRECAUTIONS:").strong().color(Color32::from_rgb(245, 158, 11)));
                ui.label("- DO NOT unplug the USB cable under any circumstances during the update.");
                ui.label("- DO NOT power off, sleep, or reboot your computer.");
                ui.label("- If interrupted, the MCU will remain in IAP bootloader recovery mode (0x36AE:0xFEAB).");
                ui.add_space(8.0);

                ui.label(RichText::new("LEGAL DISCLAIMER & RELEASE OF LIABILITY:").strong().color(Color32::from_rgb(239, 68, 68)));
                egui::Frame::none()
                    .fill(if is_dark { Color32::from_rgb(26, 20, 20) } else { Color32::from_rgb(254, 242, 242) })
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(239, 68, 68)))
                    .rounding(6.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(
                                "BY CONTINUING, YOU EXPRESSLY ACKNOWLEDGE AND AGREE THAT YOU ARE FLASHING \
                                FIRMWARE AT YOUR OWN RISK. THE SOFTWARE AND ITS AUTHORS/CREATORS ARE ENTIRELY \
                                RELEASED FROM ANY RESPONSIBILITIES, LIABILITIES, OR WARRANTY CLAIMS FOR DEVICE \
                                MALFUNCTION, CORRUPTION, DATA LOSS, OR HARDWARE BRICKING."
                            )
                            .size(11.0)
                            .color(if is_dark { Color32::from_rgb(252, 165, 165) } else { Color32::from_rgb(185, 28, 28) })
                        );
                    });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.firmware_update_modal_open = false;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let proceed_btn = egui::Button::new(
                            RichText::new("I Agree — Proceed with Update")
                                .color(Color32::WHITE)
                                .strong(),
                        ).fill(Color32::from_rgb(220, 38, 38));

                        if ui.add(proceed_btn).clicked() {
                            self.firmware_update_modal_open = false;
                            self.firmware_progress_modal_open = true;
                            self.firmware_flashing_in_progress = true;
                            self.start_firmware_flashing();
                        }
                    });
                });
            });
    }

    pub(crate) fn render_auto_update_firmware_warning_modal(&mut self, ctx: &egui::Context) {
        let is_dark = self.config.gui_theme == GuiTheme::Dark;
        let screen_rect = ctx.screen_rect();

        // 1. Semi-transparent backdrop area that captures clicks outside the modal
        let backdrop_id = egui::Id::new("auto_update_firmware_backdrop");
        let backdrop_response = egui::Area::new(backdrop_id)
            .order(egui::Order::Middle)
            .fixed_pos(screen_rect.min)
            .interactable(true)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen_rect.size(), egui::Sense::click());
                ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(140));
                resp
            });

        let mut close_modal = false;
        if !self.auto_update_modal_just_opened && backdrop_response.inner.clicked() {
            close_modal = true;
        }

        // 2. Centered modal window without unrendered emojis/symbols
        let win_resp = egui::Window::new("Warning: Enable Automatic Firmware Updates?")
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.heading(RichText::new("Warning: Microcontroller Firmware Safety").color(Color32::from_rgb(245, 158, 11)));
                ui.separator();
                ui.add_space(4.0);

                ui.label(RichText::new("Automatic firmware updates will rewrite MCU flash memory whenever an update is discovered.").strong());
                ui.add_space(6.0);

                ui.label(RichText::new("HARDWARE RISKS:").strong().color(Color32::from_rgb(239, 68, 68)));
                ui.label("- Flashing firmware rewrites physical microcontroller memory sectors.");
                ui.label("- If a background flash occurs while shutting down, sleeping, or if the USB connection drops, the keyboard can enter bootloader recovery mode or become unresponsive.");
                ui.label("- We strongly recommend keeping this disabled and reviewing firmware updates manually.");
                ui.add_space(8.0);

                egui::Frame::none()
                    .fill(if is_dark { Color32::from_rgb(26, 20, 20) } else { Color32::from_rgb(254, 242, 242) })
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(239, 68, 68)))
                    .rounding(6.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(
                                "To protect your keyboard hardware against unintended flashing interruptions, \
                                manual updates from the Device Firmware section are recommended."
                            )
                            .size(11.0)
                            .color(if is_dark { Color32::from_rgb(252, 165, 165) } else { Color32::from_rgb(185, 28, 28) })
                        );
                    });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Keep Disabled (Recommended)").clicked() {
                        self.auto_update_firmware_modal_open = false;
                        self.config.auto_update_firmware = false;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let enable_btn = egui::Button::new(
                            RichText::new("I Understand the Risks, Enable")
                                .color(Color32::WHITE)
                                .strong(),
                        ).fill(Color32::from_rgb(220, 38, 38));

                        if ui.add(enable_btn).clicked() {
                            self.config.auto_update_firmware = true;
                            self.auto_update_firmware_modal_open = false;
                            let _ = self.config.save();
                        }
                    });
                });
            });

        // 3. Detect pointer click outside window bounds as a cancellation
        if !self.auto_update_modal_just_opened && ctx.input(|i| i.pointer.primary_clicked()) {
            if let Some(ref win) = win_resp {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if !win.response.rect.contains(pos) {
                        close_modal = true;
                    }
                }
            }
        }

        self.auto_update_modal_just_opened = false;

        if close_modal {
            self.auto_update_firmware_modal_open = false;
            self.config.auto_update_firmware = false;
        }
    }

    pub(crate) fn render_firmware_progress_modal(&mut self, ctx: &egui::Context) {
        egui::Window::new("Flashing Keyboard Firmware")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(480.0);
                if let Some(ref p) = self.firmware_progress {
                    ui.heading(format!("Step {}/7: {:?}", p.step, p.phase));
                    ui.separator();
                    ui.add_space(8.0);

                    // Progress bar
                    let progress_fraction = (p.percentage / 100.0).clamp(0.0, 1.0);
                    ui.add(egui::ProgressBar::new(progress_fraction).show_percentage().animate(self.firmware_flashing_in_progress));
                    ui.add_space(6.0);
                    ui.label(RichText::new(&p.message).strong());

                    if p.phase == fantastech_device::FirmwarePhase::Completed {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Firmware updated successfully! Device has rebooted.").color(Color32::from_rgb(34, 197, 94)).strong());
                        if ui.button("Done").clicked() {
                            self.firmware_progress_modal_open = false;
                            self.firmware_flashing_in_progress = false;
                            self.check_for_firmware_updates();
                        }
                    } else if p.phase == fantastech_device::FirmwarePhase::Failed {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Flashing failed.").color(Color32::from_rgb(239, 68, 68)).strong());
                        if ui.button("Close").clicked() {
                            self.firmware_progress_modal_open = false;
                            self.firmware_flashing_in_progress = false;
                        }
                    } else {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Please wait, do not disconnect your keyboard...").color(Color32::from_rgb(245, 158, 11)).italics());
                    }
                } else {
                    ui.spinner();
                    ui.label("Initializing flashing state machine...");
                }
            });
    }
}
