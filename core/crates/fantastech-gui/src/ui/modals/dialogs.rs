//! General modal dialogs: shortcuts help, factory reset confirmation, and USB permissions.

use crate::*;
use crate::ui::widgets::*;

impl FantasTechApp {
    pub(crate) fn render_shortcuts_help_modal(&mut self, ctx: &egui::Context) {
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

    pub(crate) fn render_factory_reset_modal(&mut self, ctx: &egui::Context) {
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


    pub(crate) fn render_permission_modal(&mut self, ctx: &egui::Context) {
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


}
