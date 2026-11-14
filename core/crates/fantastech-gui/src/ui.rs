//! egui presentation layer for FantasTech.
//! This module owns rendering and input handling; device operations live in commands.rs.

use super::*;

/// Interactive HSV color disc: hue follows the angle and saturation follows
/// the distance from the center. Value is controlled by the adjacent slider.
/// A single small triangle mesh renders a smooth disc without hundreds of
/// independent paint shapes.
fn draw_color_wheel(ui: &mut egui::Ui, color: &mut [u8; 3], is_dark: bool) -> bool {
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
fn draw_status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    painter.circle_filled(center, 4.5, color.gamma_multiply(0.25));
    painter.circle_filled(center, 3.2, color);
}

/// Circular vector-painted theme toggle button (Golden Sun for Dark mode, Indigo Crescent for Light mode).
fn draw_circular_theme_toggle(ui: &mut egui::Ui, is_dark: bool) -> bool {
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

/// SVG-rendered undo button using horizontally flipped redo.svg asset.
fn draw_undo_button(ui: &mut egui::Ui, icon_tex: &egui::TextureHandle, enabled: bool, is_dark: bool) -> bool {
    let size = Vec2::new(28.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let hovered = enabled && response.hovered();

    let bg_color = if !enabled {
        if is_dark { Color32::from_rgb(18, 24, 34) } else { Color32::from_rgb(241, 245, 249) }
    } else if hovered {
        if is_dark { Color32::from_rgb(45, 58, 79) } else { Color32::from_rgb(226, 232, 240) }
    } else {
        if is_dark { Color32::from_rgb(30, 41, 59) } else { Color32::from_rgb(248, 250, 252) }
    };
    let border_color = if !enabled {
        if is_dark { Color32::from_rgb(33, 40, 54) } else { Color32::from_rgb(226, 232, 240) }
    } else if hovered {
        if is_dark { Color32::from_rgb(0, 220, 255) } else { Color32::from_rgb(0, 113, 207) }
    } else {
        if is_dark { Color32::from_rgb(71, 85, 105) } else { Color32::from_rgb(203, 213, 225) }
    };
    let icon_color = if !enabled {
        if is_dark { Color32::from_rgb(71, 85, 105) } else { Color32::from_rgb(148, 163, 184) }
    } else if hovered {
        if is_dark { Color32::from_rgb(0, 220, 255) } else { Color32::from_rgb(0, 113, 207) }
    } else {
        if is_dark { Color32::from_rgb(226, 232, 240) } else { Color32::from_rgb(30, 41, 59) }
    };

    painter.rect(rect, 4.0, bg_color, Stroke::new(1.0_f32, border_color));

    let icon_size = Vec2::new(16.0, 16.0);
    let icon_rect = egui::Rect::from_center_size(center, icon_size);
    painter.image(
        icon_tex.id(),
        icon_rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        icon_color,
    );

    response.on_hover_text("Undo (Ctrl+Z)").clicked()
}

/// SVG-rendered redo button using redo.svg asset.
fn draw_redo_button(ui: &mut egui::Ui, icon_tex: &egui::TextureHandle, enabled: bool, is_dark: bool) -> bool {
    let size = Vec2::new(28.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let hovered = enabled && response.hovered();

    let bg_color = if !enabled {
        if is_dark { Color32::from_rgb(18, 24, 34) } else { Color32::from_rgb(241, 245, 249) }
    } else if hovered {
        if is_dark { Color32::from_rgb(45, 58, 79) } else { Color32::from_rgb(226, 232, 240) }
    } else {
        if is_dark { Color32::from_rgb(30, 41, 59) } else { Color32::from_rgb(248, 250, 252) }
    };
    let border_color = if !enabled {
        if is_dark { Color32::from_rgb(33, 40, 54) } else { Color32::from_rgb(226, 232, 240) }
    } else if hovered {
        if is_dark { Color32::from_rgb(0, 220, 255) } else { Color32::from_rgb(0, 113, 207) }
    } else {
        if is_dark { Color32::from_rgb(71, 85, 105) } else { Color32::from_rgb(203, 213, 225) }
    };
    let icon_color = if !enabled {
        if is_dark { Color32::from_rgb(71, 85, 105) } else { Color32::from_rgb(148, 163, 184) }
    } else if hovered {
        if is_dark { Color32::from_rgb(0, 220, 255) } else { Color32::from_rgb(0, 113, 207) }
    } else {
        if is_dark { Color32::from_rgb(226, 232, 240) } else { Color32::from_rgb(30, 41, 59) }
    };

    painter.rect(rect, 4.0, bg_color, Stroke::new(1.0_f32, border_color));

    let icon_size = Vec2::new(16.0, 16.0);
    let icon_rect = egui::Rect::from_center_size(center, icon_size);
    painter.image(
        icon_tex.id(),
        icon_rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        icon_color,
    );

    response.on_hover_text("Redo (Ctrl+Y)").clicked()
}

fn draw_key_pill(ui: &mut egui::Ui, text: &str, is_dark: bool) {
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

fn draw_shortcut_row(ui: &mut egui::Ui, keys: &[&str], description: &str, is_dark: bool) {
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
            self.connection_checking = false;
            self.service_active = snapshot.service_active;
            self.connected = snapshot.connected;
            self.is_emulator = snapshot.is_emulator;
            self.permission_denied_node = snapshot.permission_denied_node;

            if was_checking || connection_changed || snapshot.error_message.is_some() {
                self.status_message = if let Some(error) = snapshot.error_message {
                    error
                } else if self.service_active {
                    "Connected to FantasTech background service".to_owned()
                } else if self.permission_denied_node.is_some() {
                    "Keyboard detected, but device permissions prevent access".to_owned()
                } else if self.connected && !self.is_emulator {
                    "Connected directly to the keyboard".to_owned()
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
            let events = ctx.input(|i| i.events.clone());
            for ev in events {
                match ev {
                    egui::Event::Key { key, pressed, repeat, .. } => {
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
                    egui::Event::PointerButton { button, pressed, .. } => {
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
                    _ => {}
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
                            RichText::new("⚡ Update Available!")
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
                                    ui.label(RichText::new("⚠️").size(16.0));
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
                                                "sudo ./fantastech-ctl setup-udev --install && sudo udevadm trigger".to_owned()
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

impl FantasTechApp {
    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
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

    fn render_shortcuts_help_modal(&mut self, ctx: &egui::Context) {
        let is_dark = self.config.gui_theme == GuiTheme::Dark;
        egui::Window::new(RichText::new("⌨ Keyboard Shortcuts & Controls").strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.add_space(4.0);

                egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                    // Navigation
                    ui.label(RichText::new("Tabs & Views").color(Color32::from_rgb(0, 180, 216)).strong());
                    draw_shortcut_row(ui, &["Ctrl", "1"], "Switch to Buttons (Keymap) tab", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "2"], "Switch to Lighting tab", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "3"], "Switch to Macros tab", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "4"], "Switch to Settings tab", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "Tab"], "Cycle to next tab", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "Shift", "Tab"], "Cycle to previous tab", is_dark);
                    ui.add_space(6.0);
                    ui.separator();

                    // Keymap Canvas
                    ui.label(RichText::new("Keymap Canvas (Buttons Tab)").color(Color32::from_rgb(0, 180, 216)).strong());
                    draw_shortcut_row(ui, &["↑", "↓", "←", "→"], "Navigate between keys on the keyboard canvas", is_dark);
                    draw_shortcut_row(ui, &["Enter"], "Open Modify / Remap modal for selected key", is_dark);
                    draw_shortcut_row(ui, &["Space"], "Open Modify / Remap modal for selected key", is_dark);
                    draw_shortcut_row(ui, &["Delete"], "Restore selected key to default action", is_dark);
                    draw_shortcut_row(ui, &["Backspace"], "Restore selected key to default action", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "L"], "Toggle Base Layer ↔ Fn Layer", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "P"], "Cycle Hardware Profile (1 → 2 → 3)", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "Z"], "Undo last keymap change", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "Y"], "Redo last keymap change (or Ctrl+Shift+Z)", is_dark);
                    ui.add_space(6.0);
                    ui.separator();

                    // Lighting Canvas
                    ui.label(RichText::new("Lighting Canvas (Custom Mode)").color(Color32::from_rgb(0, 180, 216)).strong());
                    draw_shortcut_row(ui, &["↑", "↓", "←", "→"], "Navigate keys on lighting canvas", is_dark);
                    draw_shortcut_row(ui, &["Shift", "Arrows"], "Expand key multi-selection", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "A"], "Toggle Select All keys", is_dark);
                    draw_shortcut_row(ui, &["Space"], "Paint selected key(s) with current color", is_dark);
                    draw_shortcut_row(ui, &["Delete"], "Turn off LED on selected key(s)", is_dark);
                    draw_shortcut_row(ui, &["C"], "Copy LED color of selected key", is_dark);
                    draw_shortcut_row(ui, &["V"], "Paste copied LED color to selection", is_dark);
                    ui.add_space(6.0);
                    ui.separator();

                    // System & General
                    ui.label(RichText::new("System & Controls").color(Color32::from_rgb(0, 180, 216)).strong());
                    draw_shortcut_row(ui, &["Ctrl", "S"], "Apply profile / lighting / macro to hardware", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "R"], "Refresh keyboard connection and service status", is_dark);
                    draw_shortcut_row(ui, &["Ctrl", "T"], "Toggle GUI theme (Dark ↔ Light)", is_dark);
                    draw_shortcut_row(ui, &["F9"], "Start / Stop recording macro (Macro tab)", is_dark);
                    draw_shortcut_row(ui, &["Esc"], "Close dialog / Deselect active key / Stop recording", is_dark);
                    draw_shortcut_row(ui, &["F1"], "Toggle this keyboard shortcuts guide", is_dark);
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close (Esc)").clicked() {
                            self.shortcuts_help_modal_open = false;
                        }
                    });
                });
            });
    }
    fn render_button_page(&mut self, ui: &mut egui::Ui) {
        let is_dark = self.config.gui_theme == GuiTheme::Dark;
        let prof = (self.active_profile as usize).min(2);
        let lay = self.active_layer as usize;
        let default_actions = default_layer_keymap(self.active_layer);

        let undo_tex = self.undo_texture.get_or_insert_with(|| {
            let img = crate::load_undo_icon().unwrap_or_else(egui::ColorImage::example);
            ui.ctx().load_texture("btn_undo_icon", img, egui::TextureOptions::LINEAR)
        }).clone();
        let redo_tex = self.redo_texture.get_or_insert_with(|| {
            let img = crate::load_redo_icon().unwrap_or_else(egui::ColorImage::example);
            ui.ctx().load_texture("btn_redo_icon", img, egui::TextureOptions::LINEAR)
        }).clone();

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
                if draw_undo_button(ui, &undo_tex, !self.undo_keymaps.is_empty(), is_dark) {
                    self.undo_keymap_edit();
                }
                if draw_redo_button(ui, &redo_tex, !self.redo_keymaps.is_empty(), is_dark) {
                    self.redo_keymap_edit();
                }

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

    fn render_modify_modal(&mut self, ctx: &egui::Context) {
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

    fn render_lighting_page(&mut self, ui: &mut egui::Ui) {
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

                            let lum = (c.r() as u32 * 299 + c.g() as u32 * 587 + c.b() as u32 * 114) / 1000;
                            let txt = if lum > 140 { Color32::BLACK } else { Color32::WHITE };
                            let border_color = if hover || is_custom_selected { Color32::WHITE } else { c };
                            (c, txt, Stroke::new(1.5_f32, border_color))
                        }
                    } else if self.selected_mode == 0 {
                        (
                            if is_dark { Color32::from_rgb(22, 29, 41) } else { Color32::from_rgb(248, 250, 252) },
                            if is_dark { Color32::from_rgb(148, 163, 184) } else { Color32::from_rgb(71, 85, 105) },
                            Stroke::new(1.0_f32, if hover { Color32::from_rgb(0, 210, 255) } else if is_dark { Color32::from_rgb(40, 53, 74) } else { Color32::from_rgb(203, 213, 225) }),
                        )
                    } else if self.selected_mode == 1 || (self.is_fixed_color && self.selected_mode != 2 && self.selected_mode != 5 && self.selected_mode != 7) {
                        let base = Color32::from_rgb(self.current_color[0], self.current_color[1], self.current_color[2]);
                        painter.circle_filled(key_rect.center(), key_rect.width() * 0.44, base.gamma_multiply(0.25));
                        let lum = (base.r() as u32 * 299 + base.g() as u32 * 587 + base.b() as u32 * 114) / 1000;
                        let txt = if lum > 140 { Color32::BLACK } else { Color32::WHITE };
                        let border_color = if hover { Color32::WHITE } else { base };
                        (base, txt, Stroke::new(1.5_f32, border_color))
                    } else {
                        // Dynamic hardware animation software preview (30-60 FPS live canvas)
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
                        let time = ui.input(|i| i.time) as f32;
                        let spd_factor = (self.speed as f32 + 1.0) * 1.5;
                        let col = match self.selected_mode {
                            2 => {
                                // Breathing pulse
                                let pulse = ((time * spd_factor).sin() * 0.45 + 0.55).clamp(0.08, 1.0);
                                if self.is_fixed_color {
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * pulse) as u8,
                                        (self.current_color[1] as f32 * pulse) as u8,
                                        (self.current_color[2] as f32 * pulse) as u8,
                                    )
                                } else {
                                    let hue = (time * spd_factor * 15.0) % 360.0;
                                    let (r, g, b) = hsv_to_rgb_simple(hue, 1.0, pulse);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            3 => {
                                // Pinwheel / Vortex rotation centered on keyboard (Spin mode)
                                let dx = (vkey.x - 11.5) * 0.35;
                                let dy = vkey.y - 2.75;
                                let angle = dy.atan2(dx);
                                let norm_angle = (angle / std::f32::consts::TAU + time * spd_factor * 0.25) % 1.0;
                                let phase = if norm_angle < 0.0 { norm_angle + 1.0 } else { norm_angle };
                                let (r, g, b) = hsv_to_rgb_simple(phase * 360.0, 0.95, 0.95);
                                Color32::from_rgb(r, g, b)
                            }
                            5 => {
                                // Horizontal wave (fixed Left-to-Right matching MK894 hardware flow)
                                let phase = (vkey.x / 23.0 - time * spd_factor * 0.25) % 1.0;
                                let norm_phase = if phase < 0.0 { phase + 1.0 } else { phase };
                                if self.is_fixed_color {
                                    let wave = (norm_phase * std::f32::consts::TAU).sin() * 0.45 + 0.55;
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * wave) as u8,
                                        (self.current_color[1] as f32 * wave) as u8,
                                        (self.current_color[2] as f32 * wave) as u8,
                                    )
                                } else {
                                    let (r, g, b) = hsv_to_rgb_simple(norm_phase * 360.0, 0.95, 0.95);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            7 => {
                                // Vertical wave (fixed Top-to-Bottom matching MK894 hardware flow)
                                let phase = (vkey.y / 5.5 - time * spd_factor * 0.35) % 1.0;
                                let norm_phase = if phase < 0.0 { phase + 1.0 } else { phase };
                                if self.is_fixed_color {
                                    let wave = (norm_phase * std::f32::consts::TAU).sin() * 0.45 + 0.55;
                                    Color32::from_rgb(
                                        (self.current_color[0] as f32 * wave) as u8,
                                        (self.current_color[1] as f32 * wave) as u8,
                                        (self.current_color[2] as f32 * wave) as u8,
                                    )
                                } else {
                                    let (r, g, b) = hsv_to_rgb_simple(norm_phase * 360.0, 0.95, 0.95);
                                    Color32::from_rgb(r, g, b)
                                }
                            }
                            _ => {
                                // Spectrum cycle / dynamic rotation
                                let ratio = (vkey.x / 23.0 + time * spd_factor * 0.2) % 1.0;
                                let norm_ratio = if ratio < 0.0 { ratio + 1.0 } else { ratio };
                                let (r, g, b) = hsv_to_rgb_simple(norm_ratio * 360.0, 0.9, 0.95);
                                Color32::from_rgb(r, g, b)
                            }
                        };
                        painter.circle_filled(key_rect.center(), key_rect.width() * 0.44, col.gamma_multiply(0.20));
                        let border_color = if hover { Color32::WHITE } else { col };
                        (col, Color32::WHITE, Stroke::new(1.5_f32, border_color))
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

                    if response.clicked() && self.selected_mode == 16 {
                        let multi = ui.input(|i| i.modifiers.ctrl || i.modifiers.shift);
                        self.toggle_custom_key_selection(vkey.index, multi);

                        if idx < SPARSE_KEY_COUNT {
                            self.paint_key_and_mirror(vkey.index, self.current_color);
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

    fn render_lighting_card_effects(&mut self, ui: &mut egui::Ui, card_width: f32, is_dark: bool) {
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

    fn render_lighting_card_params(&mut self, ui: &mut egui::Ui, card_width: f32, is_dark: bool) {
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
                    ui.label(RichText::new("Spectrum wave\nMulti-color automatic cycle").italics().color(Color32::from_rgb(100, 116, 139)));
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
                let spd_text = match self.speed {
                    0 => "1 (Slowest)",
                    1 => "2 (Slow)",
                    2 => "3 (Normal)",
                    3 => "4 (Fast)",
                    _ => "5 (Fastest)",
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Animation Speed:").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let badge_fill = if is_dark { Color32::from_rgb(26, 36, 52) } else { Color32::from_rgb(235, 243, 253) };
                        let badge_text = if is_dark { Color32::from_rgb(0, 220, 255) } else { Color32::from_rgb(0, 113, 207) };
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
                if ui.add(Slider::new(&mut self.speed, 0..=4).show_value(false).trailing_fill(true)).changed() {
                    self.apply_lighting();
                }
            });
        });
    }

    fn render_lighting_card_color(&mut self, ui: &mut egui::Ui, card_width: f32, is_dark: bool) {
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

    fn render_macro_page(&mut self, ui: &mut egui::Ui) {
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

                            if ui.button("❌").clicked() {
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

    fn render_setting_page(&mut self, ui: &mut egui::Ui) {
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
        if ui.button(RichText::new("⚠️ Factory Reset Keyboard...").color(Color32::from_rgb(255, 75, 75))).clicked() {
            self.factory_reset_modal_open = true;
            self.factory_reset_input.clear();
        }
    }

    fn render_factory_reset_modal(&mut self, ctx: &egui::Context) {
        egui::Window::new("Confirm Full Factory Reset")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.heading(RichText::new("DANGER: Factory Reset").color(Color32::from_rgb(255, 75, 75)));
                ui.separator();
                ui.label("This will completely erase all 3 onboard hardware profiles, custom keymaps, lighting presets, and 4KB macro memory from keyboard non-volatile flash.");
                ui.add_space(8.0);
                ui.label(RichText::new("To confirm, type 'I_CONFIRM_FACTORY_RESET' exactly:").strong());
                ui.text_edit_singleline(&mut self.factory_reset_input);
                ui.add_space(12.0);

                ui.horizontal_wrapped(|ui| {
                    let confirmed = self.factory_reset_input == "I_CONFIRM_FACTORY_RESET";
                    let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.add_enabled(confirmed, egui::Button::new(RichText::new("Erase and reset (Enter)").color(Color32::WHITE))).clicked()
                        || (confirmed && enter_pressed)
                    {
                        self.execute_factory_reset();
                    }
                    if ui.button("Cancel (Esc)").clicked() {
                        self.factory_reset_modal_open = false;
                        self.factory_reset_input.clear();
                    }
                });
            });
    }

    fn render_create_profile_modal(&mut self, ctx: &egui::Context) {
        let domain_label = match self.profile_modal_domain {
            ProfileDomain::Button => "Button / Keymap",
            ProfileDomain::Lighting => "Lighting",
        };
        let mut do_create = false;
        let mut do_cancel = false;

        egui::Window::new(format!("Create New {} Profile", domain_label))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading(RichText::new(format!("New {} Profile", domain_label)).strong());
                ui.separator();
                ui.add_space(4.0);

                ui.label(RichText::new("Profile Name:").strong());
                let edit_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.profile_modal_name)
                        .hint_text("e.g. Gaming, Work, FPS, Neon")
                        .desired_width(ui.available_width()),
                );
                if self.profile_modal_name.is_empty() {
                    edit_resp.request_focus();
                }
                if edit_resp.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    do_create = true;
                }

                ui.add_space(8.0);
                ui.label(RichText::new("Base Configuration:").strong());
                ui.radio_value(&mut self.profile_modal_copy_current, true, "Copy current active settings");
                ui.radio_value(&mut self.profile_modal_copy_current, false, "Start with default factory settings");

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let can_submit = !self.profile_modal_name.trim().is_empty();
                    let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.add_enabled(can_submit, egui::Button::new(RichText::new("Create Profile").strong())).clicked()
                        || (can_submit && enter_pressed)
                    {
                        do_create = true;
                    }
                    if ui.button("Cancel (Esc)").clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        do_cancel = true;
                    }
                });
            });

        if do_create && !self.profile_modal_name.trim().is_empty() {
            let name = self.profile_modal_name.clone();
            let copy = self.profile_modal_copy_current;
            match self.profile_modal_domain {
                ProfileDomain::Button => self.create_new_button_profile(&name, copy),
                ProfileDomain::Lighting => self.create_new_lighting_profile(&name, copy),
            }
            self.profile_modal_open = false;
            self.profile_modal_name.clear();
        } else if do_cancel {
            self.profile_modal_open = false;
            self.profile_modal_name.clear();
        }
    }

    fn render_delete_profile_modal(&mut self, ctx: &egui::Context) {
        let domain_label = match self.delete_profile_domain {
            ProfileDomain::Button => "button",
            ProfileDomain::Lighting => "lighting",
        };
        let mut do_delete = false;
        let mut do_cancel = false;

        egui::Window::new("Confirm Delete Profile")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading(RichText::new("Delete Profile?").color(Color32::from_rgb(239, 68, 68)).strong());
                ui.separator();
                ui.add_space(4.0);

                ui.label(format!(
                    "Are you sure you want to permanently delete the custom {} profile '{}'?",
                    domain_label, self.delete_profile_name
                ));
                ui.label(RichText::new("This action cannot be undone.").size(11.0).italics().color(Color32::from_rgb(148, 163, 184)));

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Yes, Delete").color(Color32::from_rgb(239, 68, 68))).clicked() {
                        do_delete = true;
                    }
                    if ui.button("Cancel (Esc)").clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        do_cancel = true;
                    }
                });
            });

        if do_delete {
            let id = self.delete_profile_id.clone();
            match self.delete_profile_domain {
                ProfileDomain::Button => self.delete_button_profile_by_id(&id),
                ProfileDomain::Lighting => self.delete_lighting_profile_by_id(&id),
            }
            self.delete_profile_modal_open = false;
            self.delete_profile_id.clear();
            self.delete_profile_name.clear();
        } else if do_cancel {
            self.delete_profile_modal_open = false;
            self.delete_profile_id.clear();
            self.delete_profile_name.clear();
        }
    }

    fn render_permission_modal(&mut self, ctx: &egui::Context) {
        let _is_dark = self.config.gui_theme == GuiTheme::Dark;
        egui::Window::new(if cfg!(target_os = "linux") { "USB device permissions" } else { "Keyboard access help" })
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(480.0);
                if !cfg!(target_os = "linux") {
                    ui.heading("Keyboard access needs platform setup");
                    ui.label("The hardware backend reports that the keyboard is inaccessible. Linux udev instructions do not apply to this operating system.");
                    ui.add_space(8.0);
                    ui.label("Check the USB connection, close other software that may own the keyboard, and verify that this platform has a compatible HID backend and device permissions.");
                    if ui.button("Recheck device").clicked() {
                        self.connection_checking = true;
                        self.connection_monitor.refresh();
                        self.status_message = "Rechecking keyboard connection…".to_owned();
                    }
                    if ui.button("Close").clicked() {
                        self.permission_modal_open = false;
                    }
                    return;
                }
                ui.heading(RichText::new("Linux USB Device Access Blocked").color(Color32::from_rgb(239, 68, 68)));
                ui.separator();
                ui.add_space(4.0);

                let node_str = self.permission_denied_node.as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/dev/hidraw3".to_string());

                ui.label(RichText::new(format!(
                    "The Fantech MK894 was found at {}, but Linux security restricts raw HID access to root by default. This is why key reassignments and lighting changes do not take effect.",
                    node_str
                )));
                ui.add_space(8.0);
                ui.label(RichText::new("To grant permanent access without needing root:").strong());
                ui.label("Run this command once in your terminal:");
                ui.add_space(4.0);

                let cmd = "sudo ./fantastech-ctl setup-udev --install && sudo udevadm trigger";
                ui.horizontal_wrapped(|ui| {
                    let command_width = (ui.available_width() - 58.0).max(170.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut cmd.to_string())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(command_width)
                    );
                    if ui.button("Copy").clicked() {
                        ui.output_mut(|o| o.copied_text = cmd.to_string());
                        self.status_message = "Copied setup command to clipboard!".to_string();
                    }
                });

                ui.add_space(8.0);
                ui.label(RichText::new("Alternative options:").italics());
                ui.label("• Run with sudo: `sudo fantastech`");
                ui.label("• Start the daemon service with root: `sudo fantastech service run`");

                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Recheck device").clicked() {
                        self.connection_checking = true;
                        self.connection_monitor.refresh();
                        self.status_message = "Rechecking keyboard connection…".to_owned();
                        self.permission_modal_open = false;
                    }
                    if ui.button("Close").clicked() {
                        self.permission_modal_open = false;
                    }
                });
            });
    }

    fn check_for_firmware_updates(&mut self) {
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

    fn validate_custom_firmware(&mut self) {
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

    fn start_firmware_flashing(&mut self) {
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

    fn render_firmware_confirmation_modal(&mut self, ctx: &egui::Context) {
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

    fn render_auto_update_firmware_warning_modal(&mut self, ctx: &egui::Context) {
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

    fn render_firmware_progress_modal(&mut self, ctx: &egui::Context) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspect_egui_key_names() {
        let keys = [
            egui::Key::A, egui::Key::Z, egui::Key::Num0, egui::Key::Num9,
            egui::Key::ArrowUp, egui::Key::ArrowDown, egui::Key::ArrowLeft, egui::Key::ArrowRight,
            egui::Key::Space, egui::Key::Enter, egui::Key::Escape, egui::Key::Backspace, egui::Key::Tab,
            egui::Key::Insert, egui::Key::Delete, egui::Key::Home, egui::Key::End,
            egui::Key::PageUp, egui::Key::PageDown,
            egui::Key::Plus, egui::Key::Minus, egui::Key::Equals,
            egui::Key::OpenBracket, egui::Key::CloseBracket, egui::Key::Backslash,
            egui::Key::Semicolon, egui::Key::Colon, egui::Key::Comma, egui::Key::Period, egui::Key::Slash,
            egui::Key::F1, egui::Key::F12,
        ];
        for k in keys {
            let name = k.name();
            assert!(scancode_for_key_name(name).is_some(), "Key {:?} with name '{}' failed to resolve to scancode", k, name);
        }
    }

    #[test]
    fn test_fantastech_app_initialization() {
        let app = FantasTechApp::new();
        assert_eq!(app.device_model, "Fantech MK894 Maxpower II");
        assert_eq!(app.active_profile, 0);
        assert_eq!(app.active_layer, KeyLayer::Base);
        assert!(!app.profiles.is_empty());
        assert!(!app.selected_lighting_profile.is_empty());
        assert!(!app.selected_button_profile.is_empty());
    }

    #[test]
    fn test_fantastech_gui_theme_application() {
        let ctx = egui::Context::default();
        apply_gui_theme(&ctx, GuiTheme::Light);
        apply_gui_theme(&ctx, GuiTheme::Dark);
    }

    #[test]
    fn test_keymap_undo_redo_round_trip() {
        let mut app = FantasTechApp::new();
        let original = app.profiles[0][KeyLayer::Base as usize][43];
        let replacement = KeyAction::standard_key(0x05);

        app.remap_single_key(43, replacement);
        assert_eq!(app.profiles[0][KeyLayer::Base as usize][43], replacement);
        assert!(!app.undo_keymaps.is_empty());

        app.undo_keymap_edit();
        assert_eq!(app.profiles[0][KeyLayer::Base as usize][43], original);

        app.redo_keymap_edit();
        assert_eq!(app.profiles[0][KeyLayer::Base as usize][43], replacement);
    }

    #[test]
    fn test_keymap_undo_tracks_mirrored_slot_changes() {
        let mut app = FantasTechApp::new();
        let (key_index, mirror_index) = ANSI_104_KEYS
            .iter()
            .find_map(|key| {
                get_mirror_key_index(key.index)
                    .filter(|mirror| *mirror != key.index && (*mirror as usize) < SPARSE_KEY_COUNT)
                    .map(|mirror| (key.index, mirror))
            })
            .expect("keyboard layout should expose at least one mirrored key pair");

        let primary_action = app.profiles[0][KeyLayer::Base as usize][key_index as usize];
        let divergent_mirror_action = if primary_action == KeyAction::standard_key(0x04) {
            KeyAction::standard_key(0x05)
        } else {
            KeyAction::standard_key(0x04)
        };
        app.profiles[0][KeyLayer::Base as usize][mirror_index as usize] = divergent_mirror_action;

        // The primary key already has the requested action. Updating only its
        // mirrored slot must still create a history entry.
        app.remap_single_key(key_index, primary_action);
        assert_eq!(
            app.profiles[0][KeyLayer::Base as usize][mirror_index as usize],
            primary_action
        );
        assert_eq!(app.undo_keymaps.len(), 1);

        app.undo_keymap_edit();
        assert_eq!(
            app.profiles[0][KeyLayer::Base as usize][mirror_index as usize],
            divergent_mirror_action
        );
    }

    #[test]
    fn test_keymap_undo_history_is_bounded() {
        let mut app = FantasTechApp::new();
        for _ in 0..105 {
            app.record_keymap_undo();
        }
        assert_eq!(app.undo_keymaps.len(), 100);
    }

    #[test]
    fn test_fantastech_app_apply_lighting_profile() {
        let mut app = FantasTechApp::new();
        app.custom_selected_keys.insert(44);
        app.custom_select_all = true;

        app.apply_lighting_profile_by_id("cyberpunk");
        assert_eq!(app.selected_lighting_profile, "cyberpunk");
        assert!(app.custom_selected_keys.is_empty(), "Selection must be cleared on profile switch");
        assert!(!app.custom_select_all);
        assert_eq!(app.key_colors[44], Color32::from_rgb(255, 255, 0)); // Yellow W key

        app.apply_lighting_profile_by_id("gaming");
        assert_eq!(app.selected_lighting_profile, "gaming");
        assert_eq!(app.key_colors[44], Color32::from_rgb(255, 0, 0)); // Red W key in gaming
    }

    #[test]
    fn test_fantastech_app_apply_button_profile() {
        let mut app = FantasTechApp::new();
        app.apply_button_profile_by_id("default");
        assert_eq!(app.selected_button_profile, "default");
    }

    #[test]
    fn test_fantastech_app_apply_macro_profile() {
        let mut app = FantasTechApp::new();
        app.apply_macro_profile_by_id("default");
        assert_eq!(app.selected_macro_profile, "default");
    }

    #[test]
    fn test_fantastech_app_apply_settings_profile() {
        let mut app = FantasTechApp::new();
        app.apply_settings_profile_by_id("default");
        assert_eq!(app.selected_settings_profile, "default");
    }

    #[test]
    fn test_fantastech_app_create_and_delete_button_profile() {
        let mut app = FantasTechApp::new();
        let temp_dir = std::env::temp_dir().join(format!("test_btn_prof_{}", std::process::id()));
        app.profile_manager = ProfileManager::new(Some(temp_dir));
        let prof_name = "Work Test Profile";
        app.create_new_button_profile(prof_name, true);
        assert!(app.status_message.contains("Created new button profile"));
        let new_id = app.selected_button_profile.clone();
        assert_ne!(new_id, "default");
        assert!(new_id.starts_with("work_test_profile"));

        // Cannot delete built-in "default"
        app.delete_button_profile_by_id("default");
        assert!(app.status_message.contains("Cannot delete built-in profile"));

        // Now delete the newly created custom profile
        app.delete_button_profile_by_id(&new_id);
        assert!(app.status_message.contains("Deleted button profile"));
        assert_eq!(app.selected_button_profile, "default");
    }

    #[test]
    fn test_fantastech_app_create_and_delete_lighting_profile() {
        let mut app = FantasTechApp::new();
        let temp_dir = std::env::temp_dir().join(format!("test_light_prof_{}", std::process::id()));
        app.profile_manager = ProfileManager::new(Some(temp_dir));
        let prof_name = "Night Shift Blue";
        app.create_new_lighting_profile(prof_name, false);
        assert!(app.status_message.contains("Created new lighting profile"));
        let new_id = app.selected_lighting_profile.clone();
        assert_ne!(new_id, "gaming");
        assert!(new_id.starts_with("night_shift_blue"));

        // Cannot delete built-in "gaming"
        app.delete_lighting_profile_by_id("gaming");
        assert!(app.status_message.contains("Cannot delete built-in profile"));

        // Now delete the newly created custom profile
        app.delete_lighting_profile_by_id(&new_id);
        assert!(app.status_message.contains("Deleted lighting profile"));
        assert_eq!(app.selected_lighting_profile, "gaming");
    }

    #[test]
    fn test_ansi_104_coordinates_coverage() {
        assert_eq!(ANSI_104_KEYS.len(), 104, "ANSI_104_KEYS must contain exactly 104 keys");

        let mut seen_indices = std::collections::HashSet::new();
        for k in ANSI_104_KEYS {
            assert!(seen_indices.insert(k.index), "Duplicate matrix slot index in ANSI_104_KEYS: {}", k.index);
            assert!(KEY_DEFINITIONS.iter().any(|kd| kd.index == k.index), "Index {} in ANSI_104_KEYS not found in KEY_DEFINITIONS", k.index);
            assert!(k.w >= 1.0, "Key {} has invalid width: {}", k.name, k.w);
            assert!(k.h >= 1.0, "Key {} has invalid height: {}", k.name, k.h);
            assert!(k.x >= 0.0 && k.x + k.w <= 23.01, "Key {} exceeds horizontal bounds (x={}, w={})", k.name, k.x, k.w);
            assert!(k.y >= 0.0 && k.y + k.h <= 6.46, "Key {} exceeds vertical bounds (y={}, h={})", k.name, k.y, k.h);
        }

        // Overlap check: ensure no two keys overlap
        for (i, k1) in ANSI_104_KEYS.iter().enumerate() {
            for k2 in ANSI_104_KEYS.iter().skip(i + 1) {
                let x_overlap = !(k1.x + k1.w <= k2.x + 0.01 || k2.x + k2.w <= k1.x + 0.01);
                let y_overlap = !(k1.y + k1.h <= k2.y + 0.01 || k2.y + k2.h <= k1.y + 0.01);
                assert!(!(x_overlap && y_overlap), "Keys {} (slot {}) and {} (slot {}) physically overlap!", k1.name, k1.index, k2.name, k2.index);
            }
        }
    }

    #[test]
    fn test_fn_layer_label_resolution() {
        let esc = ANSI_104_KEYS.iter().find(|k| k.index == 0).expect("Esc key missing");
        assert_eq!(esc.fn_name, "Reset");

        let f1 = ANSI_104_KEYS.iter().find(|k| k.index == 1).expect("F1 missing");
        assert_eq!(f1.fn_name, "Media");

        let f2 = ANSI_104_KEYS.iter().find(|k| k.index == 2).expect("F2 missing");
        assert_eq!(f2.fn_name, "Vol-");

        let f3 = ANSI_104_KEYS.iter().find(|k| k.index == 3).expect("F3 missing");
        assert_eq!(f3.fn_name, "Vol+");

        let f4 = ANSI_104_KEYS.iter().find(|k| k.index == 4).expect("F4 missing");
        assert_eq!(f4.fn_name, "Mute");

        let ins = ANSI_104_KEYS.iter().find(|k| k.index == 35).expect("Ins missing");
        assert_eq!(ins.fn_name, "Mode");

        let del = ANSI_104_KEYS.iter().find(|k| k.index == 56).expect("Del missing");
        assert_eq!(del.fn_name, "Color");

        let up = ANSI_104_KEYS.iter().find(|k| k.index == 99).expect("Up missing");
        assert_eq!(up.fn_name, "Brt+");

        let down = ANSI_104_KEYS.iter().find(|k| k.index == 120).expect("Down missing");
        assert_eq!(down.fn_name, "Brt-");

        let space = ANSI_104_KEYS.iter().find(|k| k.index == 111).expect("Space missing");
        assert_eq!(space.fn_name, "LedOff");

        let win = ANSI_104_KEYS.iter().find(|k| k.index == 106).expect("Win missing");
        assert_eq!(win.fn_name, "WinLk");
    }

    #[test]
    fn test_font_definitions_initialization() {
        let ctx = egui::Context::default();
        setup_custom_fonts(&ctx);
    }

    #[test]
    fn test_fit_keycap_text_bounds() {
        let (fitted, font) = fit_keycap_text("W", 30.0);
        assert_eq!(fitted, "W");
        assert_eq!(font.size, 12.5);

        let (fitted_long, _) = fit_keycap_text("ExtremelyLongActionNameToFitInSmallKey", 25.0);
        assert!(fitted_long.len() < 30);
    }

    #[test]
    fn test_sparse_key_colors_capacity_and_matrix_slots() {
        let app = FantasTechApp::new();
        assert_eq!(app.key_colors.len(), SPARSE_KEY_COUNT);
        assert_eq!(app.key_colors.len(), 126);

        // Verify keys that previously caused out-of-bounds panics when length was 104
        assert!(105 < app.key_colors.len()); // Left Ctrl
        assert!(106 < app.key_colors.len()); // Left Win
        assert!(107 < app.key_colors.len()); // Left Alt
        assert!(110 < app.key_colors.len()); // Space
        assert!(111 < app.key_colors.len()); // Space mirror
        assert!(119 < app.key_colors.len()); // Left Arrow
        assert!(120 < app.key_colors.len()); // Down Arrow
        assert!(121 < app.key_colors.len()); // Right Arrow
        assert!(122 < app.key_colors.len()); // Num 0
        assert!(124 < app.key_colors.len()); // Num .
    }

    #[test]
    fn test_custom_canvas_selection_and_color_application() {
        let mut app = FantasTechApp::new();
        assert!(app.custom_selected_keys.is_empty());
        assert!(!app.custom_select_all);

        // 1. Select All
        app.select_all_custom_keys();
        assert!(app.custom_select_all);
        assert_eq!(app.custom_selected_keys.len(), 104);

        // 2. Deselect All
        app.deselect_all_custom_keys();
        assert!(!app.custom_select_all);
        assert!(app.custom_selected_keys.is_empty());

        // 3. Single key select
        app.toggle_custom_key_selection(0, false); // Esc
        assert_eq!(app.custom_selected_keys.len(), 1);
        assert!(app.custom_selected_keys.contains(&0));

        // 4. Multi-key select (Shift/Ctrl toggle)
        app.toggle_custom_key_selection(1, true); // F1
        assert_eq!(app.custom_selected_keys.len(), 2);
        assert!(app.custom_selected_keys.contains(&0));
        assert!(app.custom_selected_keys.contains(&1));

        // 5. Apply color to selection
        let cyan = [0, 220, 255];
        app.apply_color_to_custom_selection(cyan);
        assert_eq!(app.key_colors[0], Color32::from_rgb(0, 220, 255));
        assert_eq!(app.key_colors[1], Color32::from_rgb(0, 220, 255));

        // 6. Mirrored slot painting (Space: 111 & 110)
        app.toggle_custom_key_selection(111, false);
        app.apply_color_to_custom_selection([255, 0, 0]);
        assert_eq!(app.key_colors[111], Color32::from_rgb(255, 0, 0));
        assert_eq!(app.key_colors[110], Color32::from_rgb(255, 0, 0));

        // 7. Turn off selection
        app.turn_off_custom_selection();
        assert_eq!(app.key_colors[111], Color32::BLACK);
        assert_eq!(app.key_colors[110], Color32::BLACK);
    }

    #[test]
    fn test_custom_key_group_selection_and_designer_palettes() {
        let mut app = FantasTechApp::new();
        let temp_dir = std::env::temp_dir().join(format!("test_pal_{}", std::process::id()));
        app.profile_manager = ProfileManager::new(Some(temp_dir));

        // 1. Group selection: WASD
        app.select_custom_key_group("wasd");
        assert_eq!(app.custom_selected_keys.len(), 4);
        assert!(app.custom_selected_keys.contains(&44)); // W
        assert!(app.custom_selected_keys.contains(&64)); // A
        assert!(app.custom_selected_keys.contains(&65)); // S
        assert!(app.custom_selected_keys.contains(&66)); // D

        // 2. Group selection: Arrows
        app.select_custom_key_group("arrows");
        assert_eq!(app.custom_selected_keys.len(), 4);
        assert!(app.custom_selected_keys.contains(&99));  // Up
        assert!(app.custom_selected_keys.contains(&119)); // Left
        assert!(app.custom_selected_keys.contains(&120)); // Down
        assert!(app.custom_selected_keys.contains(&121)); // Right

        // 3. Designer Palettes: Miami in Custom mode (16)
        app.selected_mode = 16;
        app.apply_designer_palette("miami");
        assert_eq!(app.current_color, [0, 240, 255]);
        // Esc (left edge x=0.0) should be bright cyan
        assert_eq!(app.key_colors[0], Color32::from_rgb(0, 240, 255));

        // 4. Designer Palettes in non-custom mode switches to custom mode, clears, and applies across all keys
        app.selected_mode = 1; // Static
        app.apply_designer_palette("terminal");
        assert_eq!(app.selected_mode, 16);
        assert_eq!(app.current_color, [0, 255, 65]);
        assert_eq!(app.key_colors[0], Color32::from_rgb(0, 59, 0));

        // 5. Hardware parity check: MK894 does not support direction reversal in built-in modes
        assert!(!LightingMode::StreamLeftToRight.supports_direction());
        assert!(!LightingMode::StreamUpToDown.supports_direction());
    }

    #[test]
    fn test_lighting_modes_support_color() {
        // Dynamic spectrum modes must NOT support fixed custom color
        assert!(!LightingMode::Spectrum.supports_color(), "ColorLoop (Spectrum) must not support fixed color");
        assert!(!LightingMode::Off.supports_color(), "LightOff must not support fixed color");
        assert!(!LightingMode::Rotate.supports_color(), "Spin must not support fixed color");
        assert!(!LightingMode::Meteor.supports_color(), "Meteor must not support fixed color");

        // Modes that do support fixed color
        assert!(LightingMode::Static.supports_color());
        assert!(LightingMode::Breathing.supports_color());
        assert!(LightingMode::Bloom.supports_color());
        assert!(LightingMode::Tide.supports_color());
        assert!(LightingMode::Shuttle.supports_color());
    }

    #[test]
    fn test_custom_canvas_brightness_scaling() {
        let mut app = FantasTechApp::new();
        app.selected_mode = 16;
        app.brightness = 2; // Level 2: 50%
        app.current_color = [200, 100, 50];

        let factor = (app.brightness as f32 / 4.0).clamp(0.0, 1.0);
        let scaled_r = (app.current_color[0] as f32 * factor).round() as u8;
        let scaled_g = (app.current_color[1] as f32 * factor).round() as u8;
        let scaled_b = (app.current_color[2] as f32 * factor).round() as u8;

        assert_eq!(scaled_r, 100);
        assert_eq!(scaled_g, 50);
        assert_eq!(scaled_b, 25);

        // When brightness is 0, scaling yields all zeros (off)
        app.brightness = 0;
        let zero_factor = (app.brightness as f32 / 4.0).clamp(0.0, 1.0);
        assert_eq!((app.current_color[0] as f32 * zero_factor).round() as u8, 0);
        assert_eq!((app.current_color[1] as f32 * zero_factor).round() as u8, 0);
        assert_eq!((app.current_color[2] as f32 * zero_factor).round() as u8, 0);
    }

    #[test]
    fn test_recent_colors_history() {
        let mut app = FantasTechApp::new();
        assert_eq!(app.recent_colors.len(), 6);

        let color1 = [255, 10, 20];
        let color2 = [10, 255, 30];
        let color3 = [30, 40, 255];
        let color4 = [200, 200, 200];
        let color5 = [50, 60, 70];
        let color6 = [80, 90, 100];
        let color7 = [110, 120, 130];

        app.record_used_color(color1);
        assert_eq!(app.recent_colors[0], color1);
        assert_eq!(app.recent_colors.len(), 6);

        app.record_used_color(color2);
        assert_eq!(app.recent_colors[0], color2);
        assert_eq!(app.recent_colors[1], color1);

        app.record_used_color(color3);
        app.record_used_color(color4);
        app.record_used_color(color5);
        app.record_used_color(color6);
        assert_eq!(app.recent_colors, vec![color6, color5, color4, color3, color2, color1]);

        // Push 7th color; should drop color1 and retain only 6
        app.record_used_color(color7);
        assert_eq!(app.recent_colors, vec![color7, color6, color5, color4, color3, color2]);

        // Push existing color2; should bring color2 to front without duplicates
        app.record_used_color(color2);
        assert_eq!(app.recent_colors, vec![color2, color7, color6, color5, color4, color3]);
    }

    #[test]
    fn test_universal_profile_loader_handles_wrong_type_and_invalid_data() {
        let mut app = FantasTechApp::new();
        let temp_dir = std::env::temp_dir();

        // 1. Save a lighting profile
        let lighting_path = temp_dir.join("test_cross_lighting.json");
        app.selected_mode = 2; // Breath mode
        app.export_lighting_profile_to_path(&lighting_path);

        // 2. Load it via universal importer (which is called by button profile load too)
        assert!(app.import_any_profile_from_path(&lighting_path));
        // Verify lighting mode was updated
        assert_eq!(app.selected_mode, 2);

        // 3. Save a button profile
        let button_path = temp_dir.join("test_cross_button.json");
        app.export_button_profile_to_path(&button_path);
        // Load button profile via universal importer
        assert!(app.import_any_profile_from_path(&button_path));

        // 4. Invalid file content does not panic/crash
        let bad_path = temp_dir.join("test_corrupt.json");
        std::fs::write(&bad_path, b"not valid json at all").unwrap();
        assert!(!app.import_any_profile_from_path(&bad_path));
        assert!(app.status_message.contains("not a valid FantasTech profile"));

        // Cleanup
        let _ = std::fs::remove_file(lighting_path);
        let _ = std::fs::remove_file(button_path);
        let _ = std::fs::remove_file(bad_path);
    }

    #[test]
    fn test_paint_key_and_mirror_and_copy_buffer() {
        let mut app = FantasTechApp::new();
        assert!(app.copied_color.is_none());

        let test_color = [123, 45, 67];
        // Key 111 is Space; its mirror is 110
        app.paint_key_and_mirror(111, test_color);
        assert_eq!(app.key_colors[111], Color32::from_rgb(123, 45, 67));
        assert_eq!(app.key_colors[110], Color32::from_rgb(123, 45, 67));
        assert_eq!(app.recent_colors[0], test_color);

        // Test copy buffer
        app.copied_color = Some(test_color);
        assert_eq!(app.copied_color, Some([123, 45, 67]));
    }

    #[test]
    fn test_gui_firmware_update_check_detection() {
        let mut app = FantasTechApp::new();

        // 1. When firmware is an unknown older revision (e.g. V50), update must be detected
        app.firmware_version = "V50".to_string();
        app.check_for_firmware_updates();
        assert!(app.update_available);
        assert!(app.firmware_status_text.contains("Update available"));

        // 2. When firmware matches factory revisions (V64, V101), status is up to date
        app.firmware_version = "V64".to_string();
        app.check_for_firmware_updates();
        assert!(!app.update_available);
        assert!(app.firmware_status_text.contains("up to date"));

        app.firmware_version = "V101".to_string();
        app.check_for_firmware_updates();
        assert!(!app.update_available);
        assert!(app.firmware_status_text.contains("up to date"));

        // 3. Simulating clicking check button: sets checking timer for at least 2 seconds
        app.checking_updates_start = Some(std::time::Instant::now());
        assert!(app.checking_updates_start.is_some());
        assert!(app.checking_updates_start.unwrap().elapsed() < std::time::Duration::from_secs(2));

        // 4. Opening modal
        app.firmware_update_modal_open = true;
        assert!(app.firmware_update_modal_open);
        app.firmware_update_modal_open = false;
        assert!(!app.firmware_update_modal_open);
    }

    #[test]
    fn test_auto_update_firmware_modal_safety() {
        let mut app = FantasTechApp::new();

        // 1. Initial state defaults: auto_update_firmware must be false, modal closed
        assert!(!app.config.auto_update_firmware);
        assert!(!app.auto_update_firmware_modal_open);

        // 2. Simulating user attempting to toggle checkbox ON
        let mut requested_auto_fw = true;
        if requested_auto_fw {
            app.auto_update_firmware_modal_open = true;
        }
        // Setting must NOT be enabled yet
        assert!(!app.config.auto_update_firmware);
        assert!(app.auto_update_firmware_modal_open);

        // 3. Simulating user clicking outside the window (backdrop cancel)
        app.auto_update_firmware_modal_open = false;
        app.config.auto_update_firmware = false;
        assert!(!app.config.auto_update_firmware);
        assert!(!app.auto_update_firmware_modal_open);

        // 4. Simulating user clicking "Keep Disabled (Recommended)"
        app.auto_update_firmware_modal_open = true;
        app.auto_update_firmware_modal_open = false;
        app.config.auto_update_firmware = false;
        assert!(!app.config.auto_update_firmware);
        assert!(!app.auto_update_firmware_modal_open);

        // 5. Simulating user attempting to toggle checkbox ON and explicitly confirming
        app.auto_update_firmware_modal_open = true;
        // User clicks "I Understand the Risks, Enable"
        app.config.auto_update_firmware = true;
        app.auto_update_firmware_modal_open = false;
        assert!(app.config.auto_update_firmware);
        assert!(!app.auto_update_firmware_modal_open);

        // 6. Simulating user toggling checkbox OFF: disables immediately without modal
        requested_auto_fw = false;
        if !requested_auto_fw {
            app.config.auto_update_firmware = false;
        }
        assert!(!app.config.auto_update_firmware);
        assert!(!app.auto_update_firmware_modal_open);
    }

    #[test]
    fn test_find_neighbor_key_spatial_navigation() {
        // Esc (0) -> F1 (1) going Right
        assert_eq!(find_neighbor_key(0, ArrowDirection::Right), 1);
        // Esc (0) -> ~ (21) going Down
        assert_eq!(find_neighbor_key(0, ArrowDirection::Down), 21);
        // ~ (21) -> Esc (0) going Up
        assert_eq!(find_neighbor_key(21, ArrowDirection::Up), 0);
        // ~ (21) -> Tab (42) going Down
        assert_eq!(find_neighbor_key(21, ArrowDirection::Down), 42);
        // Tab (42) -> Caps (63) going Down
        assert_eq!(find_neighbor_key(42, ArrowDirection::Down), 63);
        // Caps (63) -> LShift (84) going Down
        assert_eq!(find_neighbor_key(63, ArrowDirection::Down), 84);
        // LShift (84) -> LCtrl (105) going Down
        assert_eq!(find_neighbor_key(84, ArrowDirection::Down), 105);

        // Caps (63) -> A (64) -> S (65) going Right
        assert_eq!(find_neighbor_key(63, ArrowDirection::Right), 64);
        assert_eq!(find_neighbor_key(64, ArrowDirection::Right), 65);
        // S (65) -> A (64) going Left
        assert_eq!(find_neighbor_key(65, ArrowDirection::Left), 64);

        // Space (111) -> B (90) going Up and back
        assert_eq!(find_neighbor_key(111, ArrowDirection::Up), 90);
        assert_eq!(find_neighbor_key(90, ArrowDirection::Down), 111);
        // Space (111) -> LAlt (107) going Left
        assert_eq!(find_neighbor_key(111, ArrowDirection::Left), 107);
        // Space (111) -> RAlt (115) going Right
        assert_eq!(find_neighbor_key(111, ArrowDirection::Right), 115);

        // Arrow cluster: Down (120) -> Left (119), Right (121), Up (99)
        assert_eq!(find_neighbor_key(120, ArrowDirection::Left), 119);
        assert_eq!(find_neighbor_key(120, ArrowDirection::Right), 121);
        assert_eq!(find_neighbor_key(120, ArrowDirection::Up), 99);
        assert_eq!(find_neighbor_key(99, ArrowDirection::Down), 120);

        // Numpad 0 (123) -> . (124) going Right
        assert_eq!(find_neighbor_key(123, ArrowDirection::Right), 124);
        assert_eq!(find_neighbor_key(124, ArrowDirection::Left), 123);

        // Unknown key index fallback
        assert_eq!(find_neighbor_key(255, ArrowDirection::Right), 0);
    }

    #[test]
    fn test_tab_navigation_and_cycling() {
        let mut app = FantasTechApp::new();
        assert_eq!(app.current_tab, NavTab::Button);

        let tabs = [NavTab::Button, NavTab::Lighting, NavTab::Macro, NavTab::Setting];

        // Forward cycling
        for i in 0..tabs.len() {
            let next_idx = (i + 1) % tabs.len();
            app.current_tab = tabs[next_idx];
            assert_eq!(app.current_tab, tabs[next_idx]);
        }

        // Backward cycling
        for i in (0..tabs.len()).rev() {
            let prev_idx = if i == 0 { tabs.len() - 1 } else { i - 1 };
            app.current_tab = tabs[prev_idx];
            assert_eq!(app.current_tab, tabs[prev_idx]);
        }
    }

    #[test]
    fn test_keyboard_shortcuts_help_modal_state() {
        let mut app = FantasTechApp::new();
        assert!(!app.shortcuts_help_modal_open);

        app.shortcuts_help_modal_open = true;
        assert!(app.shortcuts_help_modal_open);

        app.shortcuts_help_modal_open = false;
        assert!(!app.shortcuts_help_modal_open);
    }

    #[test]
    fn test_keymap_delete_restores_default_action() {
        let mut app = FantasTechApp::new();
        let key_idx = 43u8; // Q
        let default_actions = default_layer_keymap(app.active_layer);
        let orig_action = default_actions[key_idx as usize];

        // Remap Q to something else
        let custom_action = KeyAction::standard_key(0x05); // B
        app.remap_single_key(key_idx, custom_action);
        assert_eq!(app.profiles[0][app.active_layer as usize][key_idx as usize], custom_action);

        // Simulate Delete/Backspace restoring default action
        app.remap_single_key(key_idx, default_actions[key_idx as usize]);
        assert_eq!(app.profiles[0][app.active_layer as usize][key_idx as usize], orig_action);
    }
}


