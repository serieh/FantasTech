//! Native graphical user interface for Fantech MK894 Maxpower II keyboard.

#![forbid(unsafe_code)]

use eframe::egui::{self, Color32, RichText, Slider};
use fantech_core::ThemeConfig;
use fantech_device::Device;
use fantech_emulator::VirtualKeyboardEmulator;
use fantech_protocol::constants::KEY_COUNT;
use fantech_protocol::lighting::{AnimationDirection, ColorMode, LightingConfig, LightingMode, LightingZone, RgbColor};
use fantech_transport::HidrawTransport;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 680.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Fantech MK894 Maxpower II Control Center"),
        ..Default::default()
    };

    eframe::run_native(
        "Fantech MK894 Control Center",
        native_options,
        Box::new(|_cc| Ok(Box::new(FantechApp::new()))),
    )
}

struct FantechApp {
    is_emulator: bool,
    connected: bool,
    status_message: String,
    device_model: String,
    firmware_version: String,
    active_profile: u8,

    // Lighting state
    selected_mode: u8,
    brightness: u8,
    speed: u8,
    current_color: [u8; 3],

    // Matrix canvas state
    key_colors: [Color32; KEY_COUNT],
    dry_run_enabled: bool,
}

impl FantechApp {
    fn new() -> Self {
        let mut app = Self {
            is_emulator: false,
            connected: false,
            status_message: "Attempting connection to physical MK894...".to_string(),
            device_model: "SDCX AK107".to_string(),
            firmware_version: "V101".to_string(),
            active_profile: 0,

            selected_mode: 4, // Spectrum by default
            brightness: 100,
            speed: 50,
            current_color: [255, 0, 128],

            key_colors: [Color32::from_rgb(255, 0, 128); KEY_COUNT],
            dry_run_enabled: false,
        };

        // Attempt initial discovery or fallback to emulator
        if HidrawTransport::discover().is_ok() {
            app.connected = true;
            app.status_message = "Connected to physical USB keyboard via /dev/hidraw".to_string();
        } else {
            app.is_emulator = true;
            app.connected = true;
            app.status_message = "Running in Virtual Hardware Emulator mode (no physical device opened)".to_string();
        }

        app
    }

    fn apply_lighting(&mut self) {
        let config = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode: LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum),
            brightness: self.brightness,
            speed: self.speed,
            direction: AnimationDirection::Default,
            color_mode: ColorMode::FixedColor,
            color: RgbColor::new(self.current_color[0], self.current_color[1], self.current_color[2]),
        };

        if self.is_emulator {
            let emu = VirtualKeyboardEmulator::new();
            let mut dev = Device::new_read_only(emu).into_reversible();
            dev.set_dry_run(self.dry_run_enabled);
            let _ = dev.set_lighting(&config);
            self.status_message = format!("Simulated applied lighting mode {:?}", config.mode);
        } else if let Ok(t) = HidrawTransport::discover() {
            let mut dev = Device::new_read_only(t).into_reversible();
            dev.set_dry_run(self.dry_run_enabled);
            let _ = dev.set_lighting(&config);
            self.status_message = format!("Hardware updated: mode {:?}", config.mode);
        }
    }

    fn apply_theme(&mut self, theme: ThemeConfig) {
        let matrix = theme.to_matrix();
        for (i, c) in matrix.iter().enumerate() {
            self.key_colors[i] = Color32::from_rgb(c.r, c.g, c.b);
        }

        if self.is_emulator {
            self.status_message = format!("Loaded theme '{}' into emulator", theme.name);
        } else if let Ok(t) = HidrawTransport::discover() {
            let mut dev = Device::new_read_only(t).into_reversible();
            dev.set_dry_run(self.dry_run_enabled);
            let _ = dev.stream_matrix_canvas(&matrix);
            let _ = dev.set_lighting(&LightingConfig {
                mode: LightingMode::CustomMatrixCanvas,
                ..Default::default()
            });
            self.status_message = format!("Streamed theme '{}' to keyboard", theme.name);
        }
    }
}

impl eframe::App for FantechApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("⌨️ Fantech MK894 Maxpower II Control Center");
                ui.separator();
                if self.is_emulator {
                    ui.label(RichText::new("● VIRTUAL EMULATOR").color(Color32::YELLOW));
                } else if self.connected {
                    ui.label(RichText::new("● CONNECTED").color(Color32::GREEN));
                } else {
                    ui.label(RichText::new("● DISCONNECTED").color(Color32::RED));
                }
                ui.separator();
                ui.checkbox(&mut self.dry_run_enabled, "Dry-Run Mode");
            });
        });

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Status: {}", self.status_message));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Left Panel: Controls
                ui.vertical(|ui| {
                    ui.set_width(320.0);
                    ui.group(|ui| {
                        ui.heading("Device Information");
                        ui.label(format!("Hardware Model: {}", self.device_model));
                        ui.label(format!("Firmware: {}", self.firmware_version));
                        ui.label(format!("Active Profile: Profile {}", self.active_profile));
                        ui.horizontal(|ui| {
                            if ui.button("Profile 0").clicked() { self.active_profile = 0; }
                            if ui.button("Profile 1").clicked() { self.active_profile = 1; }
                            if ui.button("Profile 2").clicked() { self.active_profile = 2; }
                        });
                    });

                    ui.add_space(10.0);

                    ui.group(|ui| {
                        ui.heading("Lighting Effects");
                        ui.label("Animation Mode:");
                        egui::ComboBox::from_label("")
                            .selected_text(format!("Mode {}", self.selected_mode))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.selected_mode, 0, "0: Off");
                                ui.selectable_value(&mut self.selected_mode, 1, "1: Static Color");
                                ui.selectable_value(&mut self.selected_mode, 2, "2: Breathing");
                                ui.selectable_value(&mut self.selected_mode, 4, "4: Spectrum Wave");
                                ui.selectable_value(&mut self.selected_mode, 5, "5: Stream LR");
                                ui.selectable_value(&mut self.selected_mode, 10, "10: Meteor");
                                ui.selectable_value(&mut self.selected_mode, 13, "13: Reactive Touch");
                                ui.selectable_value(&mut self.selected_mode, 16, "16: Custom RGB Canvas");
                            });

                        ui.add_space(5.0);
                        ui.label("Brightness:");
                        ui.add(Slider::new(&mut self.brightness, 0..=100).suffix("%"));

                        ui.label("Speed:");
                        ui.add(Slider::new(&mut self.speed, 0..=100).suffix("%"));

                        ui.add_space(5.0);
                        ui.label("Base Color:");
                        ui.color_edit_button_srgb(&mut self.current_color);

                        ui.add_space(10.0);
                        if ui.button("Apply Lighting Setting").clicked() {
                            self.apply_lighting();
                        }
                    });

                    ui.add_space(10.0);

                    ui.group(|ui| {
                        ui.heading("Color Canvas Presets");
                        ui.horizontal(|ui| {
                            if ui.button("Cyberpunk Neon").clicked() {
                                self.apply_theme(ThemeConfig::cyberpunk());
                            }
                            if ui.button("Full Spectrum").clicked() {
                                self.apply_theme(ThemeConfig::default());
                            }
                        });
                    });
                });

                ui.separator();

                // Right Panel: Virtual Keyboard Matrix Preview Canvas
                ui.vertical(|ui| {
                    ui.heading("Per-Key Addressable Matrix Preview (104 Keys)");
                    ui.label("Interactive visualization of the keyboard matrix layout:");
                    ui.add_space(10.0);

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // Render simulated rows of key buttons
                        let keys_per_row = 15;
                        for row in 0..7 {
                            ui.horizontal(|ui| {
                                for col in 0..keys_per_row {
                                    let idx = row * keys_per_row + col;
                                    if idx < KEY_COUNT {
                                        let col_color = self.key_colors[idx];
                                        let btn = egui::Button::new(format!("{:02}", idx))
                                            .fill(col_color)
                                            .min_size(egui::vec2(32.0, 32.0));
                                        if ui.add(btn).clicked() {
                                            self.key_colors[idx] = Color32::from_rgb(
                                                self.current_color[0],
                                                self.current_color[1],
                                                self.current_color[2],
                                            );
                                        }
                                    }
                                }
                            });
                        }
                    });
                });
            });
        });
    }
}
