//! egui presentation layer for FantasTech.
//! This module owns rendering and input handling; device operations live in commands.

pub mod modals;
pub mod pages;
pub mod shortcuts;
pub mod widgets;

#[cfg(test)]
mod tests;

use crate::*;
use self::widgets::*;

impl eframe::App for FantasTechApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle incoming single-instance focus / restore / exit requests
        if let Some(ref rx) = self.focus_rx {
            while let Ok(action) = rx.try_recv() {
                if action == "EXIT" || action == "CLOSE" {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    std::thread::spawn(|| {
                        std::thread::sleep(std::time::Duration::from_millis(150));
                        std::process::exit(0);
                    });
                    return;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                if action == "UPDATE" || action == "SETTINGS" || action == "--update" {
                    self.current_tab = NavTab::Setting;
                }
            }
        }

        // Drain firmware progress channel if active
        if let Some(ref rx) = self.firmware_progress_rx {
            while let Ok(update) = rx.try_recv() {
                if update.phase == fantastech_device::FirmwarePhase::Completed {
                    self.firmware_flashing_in_progress = false;
                    self.update_available = false;
                    self.firmware_version = self.latest_firmware_version.clone();
                    self.firmware_status_text = format!("Firmware is up to date ({})", self.firmware_version);
                    self.status_message = "Firmware update completed successfully".to_string();
                } else if update.phase == fantastech_device::FirmwarePhase::Failed {
                    self.firmware_flashing_in_progress = false;
                    self.status_message = format!("Firmware update failed: {}", update.message);
                }
                self.firmware_progress = Some(update);
            }
        }
        if self.firmware_flashing_in_progress {
            ctx.request_repaint();
        }

        // Apply the latest connection snapshot. Service checks and HID discovery
        // are performed by a dedicated worker, never inside the render callback.
        if let Some(snapshot) = self.connection_monitor.take_latest() {
            let was_checking = self.connection_checking;
            let connection_changed = self.service_active != snapshot.service_active
                || self.connected != snapshot.connected
                || self.is_emulator != snapshot.is_emulator
                || self.permission_denied_node != snapshot.permission_denied_node;
            let lighting_changed = snapshot.lighting_config.as_ref().map(|cfg| {
                cfg.mode as u8 != self.selected_mode
                    || cfg.brightness != self.brightness
                    || cfg.speed != self.speed
                    || (cfg.mode.supports_color() && [cfg.color.r, cfg.color.g, cfg.color.b] != self.current_color)
            }).unwrap_or(false);

            self.connection_checking = false;
            self.service_active = snapshot.service_active;
            self.connected = snapshot.connected;
            self.is_emulator = snapshot.is_emulator;
            self.permission_denied_node = snapshot.permission_denied_node.clone();

            if was_checking || (connection_changed && snapshot.connected) || lighting_changed {
                self.apply_hardware_snapshot(&snapshot);
            }

            if snapshot.error_message.is_some() || !self.connected {
                self.status_message = if let Some(error) = snapshot.error_message {
                    error
                } else if self.permission_denied_node.is_some() {
                    "Keyboard detected, but device permissions prevent access".to_owned()
                } else {
                    "No physical keyboard detected; emulator mode is available".to_owned()
                };

                if self.config.auto_check_updates {
                    self.check_for_firmware_updates();
                }
            }
        }
        // Idle windows repaint only when input arrives or the next connection
        // snapshot is due. Macro recording explicitly requests continuous frames.
        ctx.request_repaint_after(std::time::Duration::from_secs(2));

        // Process global keyboard navigation, shortcuts, and controls
        self.handle_keyboard_shortcuts(ctx);

        if self.is_recording {
            ctx.request_repaint();
            enum MacroInputEvent {
                Key(egui::Key, bool, bool),
                Pointer(egui::PointerButton, bool),
            }
            let events: Vec<MacroInputEvent> = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Key { key, pressed, repeat, .. } => {
                            Some(MacroInputEvent::Key(*key, *pressed, *repeat))
                        }
                        egui::Event::PointerButton { button, pressed, .. } => {
                            Some(MacroInputEvent::Pointer(*button, *pressed))
                        }
                        _ => None,
                    })
                    .collect()
            });

            for ev in events {
                match ev {
                    MacroInputEvent::Key(key, pressed, repeat) => {
                        if key == egui::Key::Escape && pressed {
                            self.is_recording = false;
                            self.recording_last_time = None;
                            self.status_message = format!("Macro recording stopped ({} steps)", self.macro_sequence.steps.len());
                            break;
                        }
                        if !repeat {
                            let now = std::time::Instant::now();
                            let delay_ms = if self.fixed_delay_enabled {
                                self.fixed_delay_ms
                            } else if let Some(last) = self.recording_last_time {
                                (now.duration_since(last).as_millis() as u16).clamp(1, 1000)
                            } else {
                                50
                            };
                            self.recording_last_time = Some(now);

                            let key_name = key.name();
                            let code = scancode_for_key_name(key_name).unwrap_or(0);
                            let action = if pressed { "down".to_string() } else { "up".to_string() };

                            self.macro_sequence.steps.push(MacroItem {
                                action,
                                scancode: code,
                                code,
                                name: Some(key_name.to_string()),
                                delay_ms,
                            });
                        }
                    }
                    MacroInputEvent::Pointer(button, pressed) => {
                        let now = std::time::Instant::now();
                        let delay_ms = if self.fixed_delay_enabled {
                            self.fixed_delay_ms
                        } else if let Some(last) = self.recording_last_time {
                            (now.duration_since(last).as_millis() as u16).clamp(1, 1000)
                        } else {
                            50
                        };
                        self.recording_last_time = Some(now);

                        let (btn_name, code) = match button {
                            egui::PointerButton::Primary => ("LeftClick", 0x01),
                            egui::PointerButton::Secondary => ("RightClick", 0x02),
                            egui::PointerButton::Middle => ("MiddleClick", 0x04),
                            _ => ("PointerButton", 0x00),
                        };

                        let action = if pressed { "mouse_down".to_string() } else { "mouse_up".to_string() };
                        self.macro_sequence.steps.push(MacroItem {
                            action,
                            scancode: code,
                            code,
                            name: Some(btn_name.to_string()),
                            delay_ms,
                        });
                    }
                }
            }
        }

        // TOP HEADER BAR: unified single-line navigation and status bar
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let logo_tex = self.text_logo_texture.get_or_insert_with(|| {
                    let color_image = crate::load_text_logo_image().unwrap_or_else(|| {
                        egui::ColorImage::example()
                    });
                    ctx.load_texture("fantastech_text_logo", color_image, egui::TextureOptions::LINEAR)
                });
                // Original logo is 2172 x 724 (~3:1 aspect ratio). Clean height of 28.0 gives width 84.0.
                let logo_img = egui::Image::new(&*logo_tex)
                    .fit_to_exact_size(Vec2::new(84.0, 28.0));
                let logo_btn = ui.add(logo_img.sense(egui::Sense::click()))
                    .on_hover_text("FantasTech MK894 Control Center");
                if logo_btn.clicked() {
                    self.current_tab = NavTab::Button;
                }

                ui.label(
                    RichText::new("Fantech MK894 Maxpower II")
                        .size(14.0)
                        .strong(),
                )
                .on_hover_text("Mechanical RGB Keyboard · USB HID");
                ui.separator();

                if self.connection_checking {
                    draw_status_dot(ui, Color32::from_rgb(148, 163, 184));
                    ui.label(RichText::new("Checking device…").color(Color32::from_rgb(148, 163, 184)).strong());
                } else if self.service_active {
                    draw_status_dot(ui, Color32::from_rgb(34, 197, 94));
                    ui.label(RichText::new("Service active").color(Color32::from_rgb(34, 197, 94)).strong());
                } else if let Some(ref path) = self.permission_denied_node {
                    draw_status_dot(ui, Color32::from_rgb(239, 68, 68));
                    ui.label(RichText::new(format!("Permission denied: {}", path.display())).color(Color32::from_rgb(239, 68, 68)).strong());
                    if ui.button("Device permissions…").clicked() {
                        self.permission_modal_open = true;
                    }
                } else if self.connected && !self.is_emulator {
                    draw_status_dot(ui, Color32::from_rgb(34, 197, 94));
                    ui.label(RichText::new("Direct USB").color(Color32::from_rgb(34, 197, 94)).strong());
                } else if self.is_emulator {
                    draw_status_dot(ui, Color32::from_rgb(245, 158, 11));
                    ui.label(RichText::new("Emulator mode").color(Color32::from_rgb(245, 158, 11)).strong())
                        .on_hover_text("Offline software simulation; no physical keyboard changes are made");
                } else {
                    draw_status_dot(ui, Color32::from_rgb(239, 68, 68));
                    ui.label(RichText::new("Disconnected").color(Color32::from_rgb(239, 68, 68)).strong());
                }

                if self.update_available {
                    ui.separator();
                    let update_badge = ui.add(
                        egui::Button::new(
                            RichText::new("Update Available!")
                                .color(Color32::WHITE)
                                .size(11.5)
                                .strong(),
                        )
                        .fill(Color32::from_rgb(220, 38, 38))
                        .rounding(10.0),
                    )
                    .on_hover_text(format!(
                        "Firmware update available: {} -> {}. Click to open Settings and update.",
                        self.firmware_version, self.latest_firmware_version
                    ));
                    if update_badge.clicked() {
                        self.current_tab = NavTab::Setting;
                    }
                }

                // Controls aligned to the right side of the top bar
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_dark_mode = self.config.gui_theme == GuiTheme::Dark;
                    if draw_circular_theme_toggle(ui, is_dark_mode) {
                        let next_theme = match self.config.gui_theme {
                            GuiTheme::Light => GuiTheme::Dark,
                            GuiTheme::Dark => GuiTheme::Light,
                        };
                        self.config.gui_theme = next_theme;
                        let _ = self.config.save();
                        apply_gui_theme(ctx, next_theme);
                    }

                    if ui.button("Refresh device").on_hover_text("Check service and USB connection now (Ctrl+R)").clicked() {
                        self.connection_monitor.refresh();
                        self.connection_checking = true;
                    }

                    if ui.button("⌨ Shortcuts (F1)").on_hover_text("View all keyboard shortcuts and navigation controls (F1)").clicked() {
                        self.shortcuts_help_modal_open = true;
                    }

                    if self.service_active {
                        if ui.button("Stop service").clicked() {
                            let _ = send_ipc_request(&IpcRequest::ShutdownService);
                            match platform::service_action("stop") {
                                Ok(()) => {
                                    self.connection_monitor.refresh();
                                    self.connection_checking = true;
                                    self.status_message = "Background service stopped".to_owned();
                                }
                                Err(error) => self.status_message = format!("Shutdown requested; {error}"),
                            }
                        }
                    } else if ui.button("Start service").clicked() {
                        self.status_message = match platform::service_action("start") {
                            Ok(()) => {
                                self.connection_monitor.refresh();
                                self.connection_checking = true;
                                "Background service started".to_owned()
                            }
                            Err(error) => error,
                        };
                    }
                });
            });
            ui.add_space(6.0);
        });

        // BOTTOM FOOTER STATUS BAR
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("Status · {}", self.status_message))
                        .size(11.5)
                        .color(Color32::from_rgb(139, 153, 173)),
                );
                ui.separator();
                ui.label(
                    RichText::new("FantasTech · MK894 · VID 0x36AE / PID 0xFE9C")
                        .size(10.5)
                        .color(Color32::from_rgb(119, 133, 153)),
                );
            });
            ui.add_space(3.0);
        });

        // Navigation adapts to available width: a side rail on desktop and a
        // compact wrapping tab strip on narrower windows.
        let compact_layout = ctx.screen_rect().width() < 1400.0;
        if compact_layout {
            egui::TopBottomPanel::top("compact_navigation").show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (tab, label, shortcut) in [
                        (NavTab::Button, "Buttons", "Ctrl+1"),
                        (NavTab::Lighting, "Lighting", "Ctrl+2"),
                        (NavTab::Macro, "Macros", "Ctrl+3"),
                        (NavTab::Setting, "Settings", "Ctrl+4"),
                    ] {
                        let selected = self.current_tab == tab;
                        if ui.selectable_label(selected, RichText::new(label).strong())
                            .on_hover_text(format!("Switch to {} tab ({})", label, shortcut))
                            .clicked()
                        {
                            self.current_tab = tab;
                        }
                    }
                });
            });
        } else {
            egui::SidePanel::left("left_rail")
                .resizable(false)
                .exact_width(132.0)
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        for (tab, label, shortcut) in [
                            (NavTab::Button, "Buttons", "Ctrl+1"),
                            (NavTab::Lighting, "Lighting", "Ctrl+2"),
                            (NavTab::Macro, "Macros", "Ctrl+3"),
                            (NavTab::Setting, "Settings", "Ctrl+4"),
                        ] {
                            let selected = self.current_tab == tab;
                            let response = ui.add_sized(
                                [116.0, 38.0],
                                egui::Button::new(RichText::new(label).size(13.0).strong())
                                    .fill(if selected { Color32::from_rgb(0, 86, 160) } else { Color32::TRANSPARENT })
                                    .stroke(if selected { Stroke::new(1.0_f32, Color32::from_rgb(0, 113, 207)) } else { Stroke::NONE }),
                            )
                            .on_hover_text(format!("Switch to {} tab ({})", label, shortcut));
                            if response.clicked() {
                                self.current_tab = tab;
                            }
                            ui.add_space(4.0);
                        }
                    });
                });
        }

        // CENTRAL CONTENT AREA (With full 2D adaptive scrolling)
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(ref path) = self.permission_denied_node {
                        let is_dark = self.config.gui_theme == GuiTheme::Dark;
                        egui::Frame::none()
                            .fill(if is_dark { Color32::from_rgb(45, 20, 20) } else { Color32::from_rgb(254, 242, 242) })
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(239, 68, 68)))
                            .rounding(6.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new("[!]")
                                            .size(15.0)
                                            .strong()
                                            .color(Color32::from_rgb(239, 68, 68)),
                                    );
                                    let permission_text = if cfg!(target_os = "linux") {
                                        format!("Physical keyboard detected at {}, but Linux device permissions block communication.", path.display())
                                    } else {
                                        format!("Keyboard detected at {}, but this operating system denied access. Check HID support and device permissions.", path.display())
                                    };
                                    ui.label(
                                        RichText::new(permission_text)
                                            .color(if is_dark { Color32::from_rgb(254, 202, 202) } else { Color32::from_rgb(153, 27, 27) })
                                            .strong(),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        let copy_label = if cfg!(target_os = "linux") { "Copy fix command" } else { "Copy device path" };
                                        if ui.button(copy_label).clicked() {
                                            let copy_text = if cfg!(target_os = "linux") {
                                                "sudo ./fantastech setup-udev --install && sudo udevadm trigger".to_owned()
                                            } else {
                                                path.display().to_string()
                                            };
                                            ui.output_mut(|o| o.copied_text = copy_text);
                                            self.status_message = if cfg!(target_os = "linux") {
                                                "Copied the udev setup command to the clipboard".to_owned()
                                            } else {
                                                "Copied the keyboard device path to the clipboard".to_owned()
                                            };
                                        }
                                        if ui.button("Help with access…").clicked() {
                                            self.permission_modal_open = true;
                                        }
                                    });
                                });
                            });
                        ui.add_space(6.0);
                    }

                    match self.current_tab {
                        NavTab::Button => self.render_button_page(ui),
                        NavTab::Lighting => self.render_lighting_page(ui),
                        NavTab::Macro => self.render_macro_page(ui),
                        NavTab::Setting => self.render_setting_page(ui),
                    }
                });
        });

        // MODAL DIALOG: Permission Guidance
        if self.permission_modal_open {
            self.render_permission_modal(ctx);
        }

        // MODAL DIALOG: Modify Key Reassignment
        if self.modify_modal_open {
            self.render_modify_modal(ctx);
        }

        // MODAL DIALOG: Factory Reset Confirmation
        if self.factory_reset_modal_open {
            self.render_factory_reset_modal(ctx);
        }

        // MODAL DIALOG: Firmware Update Risk & Liability Confirmation
        if self.firmware_update_modal_open {
            self.render_firmware_confirmation_modal(ctx);
        }

        // MODAL DIALOG: Auto-Update Firmware Risk Warning
        if self.auto_update_firmware_modal_open {
            self.render_auto_update_firmware_warning_modal(ctx);
        }

        // MODAL DIALOG: Firmware Flashing Live Progress
        if self.firmware_progress_modal_open {
            self.render_firmware_progress_modal(ctx);
        }

        // MODAL DIALOG: Keyboard Shortcuts & Controls
        if self.shortcuts_help_modal_open {
            self.render_shortcuts_help_modal(ctx);
        }

        // MODAL DIALOG: Create Profile
        if self.profile_modal_open {
            self.render_create_profile_modal(ctx);
        }

        // MODAL DIALOG: Delete Profile Confirmation
        if self.delete_profile_modal_open {
            self.render_delete_profile_modal(ctx);
        }
    }
}
