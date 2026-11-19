//! FantasTech — Professional Driver & RGB Control Suite for Fantech MK894.
//!
//! License: Apache-2.0

#![forbid(unsafe_code)]

use std::path::PathBuf;

mod commands;
mod platform;
mod ui;

use eframe::egui::{self, Color32, RichText, Slider, Stroke, Vec2};
#[cfg(target_os = "linux")]
use fantastech_core::write_user_owned_file;
use fantastech_core::{
    default_layer_keymap, default_scancode_for_key, format_action_str, get_mirror_key_index,
    is_service_running, key_name_for_scancode, scancode_for_key_name, send_ipc_request,
    ColorDisplayFormat, FantasTechConfig, GuiTheme,
    IpcRequest, IpcResponse, LayerKeymapConfig, MacroItem,
    MacroSequence, ProfileManager, KEY_DEFINITIONS,
};
use fantastech_device::{Device, FullDeviceBackup};
use fantastech_protocol::constants::{MACRO_MEMORY_SIZE, SPARSE_KEY_COUNT};
use fantastech_protocol::keymap::{KeyAction, KeyLayer};
use fantastech_protocol::lighting::{
    AnimationDirection, ColorMode, LightingConfig, LightingMode, LightingZone, RgbColor,
};
use fantastech_protocol::macro_mem::MacroMemoryTable;
use fantastech_transport::{HidrawTransport, TransportError};

fn apply_gui_theme(ctx: &egui::Context, theme: GuiTheme) {
    match theme {
        GuiTheme::Light => {
            let mut visuals = egui::Visuals::light();
            visuals.override_text_color = Some(Color32::from_rgb(15, 23, 42)); // Slate 900
            visuals.window_fill = Color32::from_rgb(246, 248, 252);
            visuals.panel_fill = Color32::from_rgb(246, 248, 252);
            visuals.extreme_bg_color = Color32::from_rgb(255, 255, 255);
            visuals.faint_bg_color = Color32::from_rgb(239, 243, 249);
            visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(255, 255, 255);
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(224, 230, 239));
            visuals.widgets.inactive.bg_fill = Color32::from_rgb(255, 255, 255);
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(211, 219, 231));
            visuals.widgets.hovered.bg_fill = Color32::from_rgb(235, 243, 253);
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, Color32::from_rgb(0, 113, 207));
            visuals.widgets.active.bg_fill = Color32::from_rgb(0, 101, 184);
            visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0, 87, 160));
            visuals.widgets.noninteractive.rounding = egui::Rounding::same(7.0);
            visuals.widgets.inactive.rounding = egui::Rounding::same(7.0);
            visuals.widgets.hovered.rounding = egui::Rounding::same(7.0);
            visuals.widgets.active.rounding = egui::Rounding::same(7.0);
            visuals.selection.bg_fill = Color32::from_rgb(0, 113, 207);
            ctx.set_visuals(visuals);
        }
        GuiTheme::Dark => {
            let mut visuals = egui::Visuals::dark();
            visuals.override_text_color = Some(Color32::from_rgb(241, 245, 249)); // Clean crisp white
            visuals.window_fill = Color32::from_rgb(13, 18, 28);
            visuals.panel_fill = Color32::from_rgb(13, 18, 28);
            visuals.extreme_bg_color = Color32::from_rgb(9, 13, 21);
            visuals.faint_bg_color = Color32::from_rgb(20, 28, 42);
            visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(19, 27, 41);
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(39, 51, 70));
            visuals.widgets.inactive.bg_fill = Color32::from_rgb(25, 35, 52);
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(47, 62, 84));
            visuals.widgets.hovered.bg_fill = Color32::from_rgb(35, 51, 73);
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, Color32::from_rgb(55, 157, 230));
            visuals.widgets.active.bg_fill = Color32::from_rgb(0, 101, 184);
            visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0, 137, 220));
            visuals.widgets.noninteractive.rounding = egui::Rounding::same(7.0);
            visuals.widgets.inactive.rounding = egui::Rounding::same(7.0);
            visuals.widgets.hovered.rounding = egui::Rounding::same(7.0);
            visuals.widgets.active.rounding = egui::Rounding::same(7.0);
            visuals.selection.bg_fill = Color32::from_rgb(0, 113, 207);
            ctx.set_visuals(visuals);
        }
    }
}

fn setup_custom_fonts(ctx: &egui::Context) {
    // Use egui's bundled default fonts so typography is consistent and does not
    // depend on distribution-specific font paths or extra runtime file reads.
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(9.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.spacing.interact_size = Vec2::new(38.0, 32.0);
    style.spacing.slider_width = 150.0;
    style.animation_time = 0.12;
    ctx.set_style(style);
}

fn fit_keycap_text(text: &str, max_width: f32) -> (String, egui::FontId) {
    let sizes = [12.5, 11.0, 10.0, 9.0];
    let max_line_len = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    for &sz in &sizes {
        let approx_width = max_line_len as f32 * (sz * 0.68);
        if approx_width <= max_width {
            return (text.to_string(), egui::FontId::proportional(sz));
        }
    }
    let sz = 8.5;
    let max_chars = ((max_width / (sz * 0.68)).floor() as usize).max(2);
    if text.contains('\n') {
        let lines: Vec<String> = text
            .lines()
            .map(|l| {
                if l.chars().count() > max_chars {
                    let mut truncated = l.chars().take(max_chars.saturating_sub(1)).collect::<String>();
                    truncated.push('…');
                    truncated
                } else {
                    l.to_string()
                }
            })
            .collect();
        (lines.join("\n"), egui::FontId::proportional(sz))
    } else if text.chars().count() > max_chars {
        let mut truncated = text.chars().take(max_chars.saturating_sub(1)).collect::<String>();
        truncated.push('…');
        (truncated, egui::FontId::proportional(sz))
    } else {
        (text.to_string(), egui::FontId::proportional(sz))
    }
}

pub(crate) fn hsv_to_rgb_simple(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let h_prime = (h / 60.0) % 6.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum NavTab {
    Button,
    Lighting,
    Macro,
    Setting,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PaletteTab {
    Basic,
    Media,
    Shortcut,
    Custom,
    Macro,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ModifyTab {
    SystemButton,
    KeyboardKeys,
    MacroButton,
}

#[derive(Debug, Clone, Copy)]
pub struct VisualKeyDef {
    pub name: &'static str,
    pub fn_name: &'static str,
    pub index: u8,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub static ANSI_104_KEYS: &[VisualKeyDef] = &[
    // Row 0 (F-row, 16 keys) - y = 0.0, h = 1.0
    VisualKeyDef { name: "Esc", fn_name: "Reset", index: 0, x: 0.0, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F1", fn_name: "Media", index: 1, x: 2.0, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F2", fn_name: "Vol-", index: 2, x: 3.0, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F3", fn_name: "Vol+", index: 3, x: 4.0, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F4", fn_name: "Mute", index: 4, x: 5.0, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F5", fn_name: "Stop", index: 5, x: 6.5, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F6", fn_name: "Prev", index: 6, x: 7.5, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F7", fn_name: "Play", index: 7, x: 8.5, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F8", fn_name: "Next", index: 8, x: 9.5, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F9", fn_name: "Find", index: 9, x: 11.0, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F10", fn_name: "Home", index: 10, x: 12.0, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F11", fn_name: "PC", index: 11, x: 13.0, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F12", fn_name: "Calc", index: 12, x: 14.0, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "PrtSc", fn_name: "PrtSc", index: 13, x: 15.5, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "ScrLK", fn_name: "ScrLK", index: 14, x: 16.5, y: 0.0, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Pause", fn_name: "Play", index: 15, x: 17.5, y: 0.0, w: 1.0, h: 1.0 },

    // Row 1 (Number row, 21 keys) - y = 1.35, h = 1.0
    VisualKeyDef { name: "~", fn_name: "~", index: 21, x: 0.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "1", fn_name: "1", index: 22, x: 1.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "2", fn_name: "2", index: 23, x: 2.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "3", fn_name: "3", index: 24, x: 3.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "4", fn_name: "4", index: 25, x: 4.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "5", fn_name: "5", index: 26, x: 5.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "6", fn_name: "6", index: 27, x: 6.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "7", fn_name: "7", index: 28, x: 7.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "8", fn_name: "8", index: 29, x: 8.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "9", fn_name: "9", index: 30, x: 9.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "0", fn_name: "0", index: 31, x: 10.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "-", fn_name: "-", index: 32, x: 11.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "=", fn_name: "=", index: 33, x: 12.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Bksp", fn_name: "Bksp", index: 34, x: 13.0, y: 1.35, w: 2.0, h: 1.0 },
    VisualKeyDef { name: "Ins", fn_name: "Mode", index: 35, x: 15.5, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Home", fn_name: "Home", index: 36, x: 16.5, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "PgUp", fn_name: "PgUp", index: 37, x: 17.5, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "NumLk", fn_name: "NumLk", index: 38, x: 19.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num /", fn_name: "Num /", index: 39, x: 20.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num *", fn_name: "Num *", index: 40, x: 21.0, y: 1.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num -", fn_name: "Num -", index: 41, x: 22.0, y: 1.35, w: 1.0, h: 1.0 },

    // Row 2 (QWERTY row, 21 keys) - y = 2.35, h = 1.0 (Num+ h = 2.0)
    VisualKeyDef { name: "Tab", fn_name: "Tab", index: 42, x: 0.0, y: 2.35, w: 1.5, h: 1.0 },
    VisualKeyDef { name: "Q", fn_name: "Q", index: 43, x: 1.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "W", fn_name: "W", index: 44, x: 2.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "E", fn_name: "E", index: 45, x: 3.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "R", fn_name: "R", index: 46, x: 4.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "T", fn_name: "T", index: 47, x: 5.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Y", fn_name: "Y", index: 48, x: 6.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "U", fn_name: "U", index: 49, x: 7.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "I", fn_name: "I", index: 50, x: 8.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "O", fn_name: "O", index: 51, x: 9.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "P", fn_name: "P", index: 52, x: 10.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "[", fn_name: "[", index: 53, x: 11.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "]", fn_name: "]", index: 54, x: 12.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "\\", fn_name: "\\", index: 55, x: 13.5, y: 2.35, w: 1.5, h: 1.0 },
    VisualKeyDef { name: "Del", fn_name: "Color", index: 56, x: 15.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "End", fn_name: "End", index: 57, x: 16.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "PgDn", fn_name: "PgDn", index: 58, x: 17.5, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num 7", fn_name: "Num 7", index: 59, x: 19.0, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num 8", fn_name: "Num 8", index: 60, x: 20.0, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num 9", fn_name: "Num 9", index: 61, x: 21.0, y: 2.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num +", fn_name: "Num +", index: 62, x: 22.0, y: 2.35, w: 1.0, h: 2.0 },

    // Row 3 (Home row, 16 keys) - y = 3.35, h = 1.0
    VisualKeyDef { name: "Caps", fn_name: "Caps", index: 63, x: 0.0, y: 3.35, w: 1.75, h: 1.0 },
    VisualKeyDef { name: "A", fn_name: "A", index: 64, x: 1.75, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "S", fn_name: "S", index: 65, x: 2.75, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "D", fn_name: "D", index: 66, x: 3.75, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "F", fn_name: "F", index: 67, x: 4.75, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "G", fn_name: "G", index: 68, x: 5.75, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "H", fn_name: "H", index: 69, x: 6.75, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "J", fn_name: "J", index: 70, x: 7.75, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "K", fn_name: "K", index: 71, x: 8.75, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "L", fn_name: "L", index: 72, x: 9.75, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: ";", fn_name: ";", index: 73, x: 10.75, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "'", fn_name: "'", index: 74, x: 11.75, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Enter", fn_name: "Enter", index: 76, x: 12.75, y: 3.35, w: 2.25, h: 1.0 },
    VisualKeyDef { name: "Num 4", fn_name: "Num 4", index: 80, x: 19.0, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num 5", fn_name: "Num 5", index: 81, x: 20.0, y: 3.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num 6", fn_name: "Num 6", index: 82, x: 21.0, y: 3.35, w: 1.0, h: 1.0 },

    // Row 4 (ZXCV row, 17 keys) - y = 4.35, h = 1.0 (Num Enter h = 2.0)
    VisualKeyDef { name: "Shift", fn_name: "Shift", index: 84, x: 0.0, y: 4.35, w: 2.25, h: 1.0 },
    VisualKeyDef { name: "Z", fn_name: "Z", index: 86, x: 2.25, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "X", fn_name: "X", index: 87, x: 3.25, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "C", fn_name: "C", index: 88, x: 4.25, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "V", fn_name: "V", index: 89, x: 5.25, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "B", fn_name: "B", index: 90, x: 6.25, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "N", fn_name: "N", index: 91, x: 7.25, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "M", fn_name: "M", index: 92, x: 8.25, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "<", fn_name: "<", index: 93, x: 9.25, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: ">", fn_name: ">", index: 94, x: 10.25, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "/", fn_name: "/", index: 95, x: 11.25, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Shift", fn_name: "Shift", index: 97, x: 12.25, y: 4.35, w: 2.75, h: 1.0 },
    VisualKeyDef { name: "Up", fn_name: "Brt+", index: 99, x: 16.5, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num 1", fn_name: "Num 1", index: 101, x: 19.0, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num 2", fn_name: "Num 2", index: 102, x: 20.0, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num 3", fn_name: "Num 3", index: 103, x: 21.0, y: 4.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Num\nEnter", fn_name: "Num\nEnter", index: 104, x: 22.0, y: 4.35, w: 1.0, h: 2.0 },

    // Row 5 (Bottom row, 13 keys) - y = 5.35, h = 1.0
    VisualKeyDef { name: "Ctrl", fn_name: "Ctrl", index: 105, x: 0.0, y: 5.35, w: 1.25, h: 1.0 },
    VisualKeyDef { name: "Win", fn_name: "WinLk", index: 106, x: 1.25, y: 5.35, w: 1.25, h: 1.0 },
    VisualKeyDef { name: "Alt", fn_name: "Alt", index: 107, x: 2.50, y: 5.35, w: 1.25, h: 1.0 },
    VisualKeyDef { name: "Space", fn_name: "LedOff", index: 111, x: 3.75, y: 5.35, w: 6.25, h: 1.0 },
    VisualKeyDef { name: "Alt", fn_name: "Alt", index: 115, x: 10.0, y: 5.35, w: 1.25, h: 1.0 },
    VisualKeyDef { name: "Fn", fn_name: "Fn", index: 116, x: 11.25, y: 5.35, w: 1.25, h: 1.0 },
    VisualKeyDef { name: "App", fn_name: "App", index: 117, x: 12.50, y: 5.35, w: 1.25, h: 1.0 },
    VisualKeyDef { name: "Ctrl", fn_name: "Ctrl", index: 118, x: 13.75, y: 5.35, w: 1.25, h: 1.0 },
    VisualKeyDef { name: "Left", fn_name: "Spd-", index: 119, x: 15.5, y: 5.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Down", fn_name: "Brt-", index: 120, x: 16.5, y: 5.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "Right", fn_name: "Spd+", index: 121, x: 17.5, y: 5.35, w: 1.0, h: 1.0 },
    VisualKeyDef { name: "0", fn_name: "0", index: 123, x: 19.0, y: 5.35, w: 2.0, h: 1.0 },
    VisualKeyDef { name: ".", fn_name: ".", index: 124, x: 21.0, y: 5.35, w: 1.0, h: 1.0 },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Finds the nearest visual key in the specified geometric arrow direction on the physical ANSI 104 canvas.
pub fn find_neighbor_key(current_index: u8, direction: ArrowDirection) -> u8 {
    let current = ANSI_104_KEYS.iter().find(|k| k.index == current_index);
    let (cur_cx, cur_cy) = match current {
        Some(k) => (k.x + k.w / 2.0, k.y + k.h / 2.0),
        None => return ANSI_104_KEYS.first().map(|k| k.index).unwrap_or(0),
    };

    let mut best_key = current_index;
    let mut best_score = f32::MAX;

    for candidate in ANSI_104_KEYS {
        if candidate.index == current_index {
            continue;
        }
        let cand_cx = candidate.x + candidate.w / 2.0;
        let cand_cy = candidate.y + candidate.h / 2.0;

        let dx = cand_cx - cur_cx;
        let dy = cand_cy - cur_cy;

        let is_valid = match direction {
            ArrowDirection::Right => dx > 0.05,
            ArrowDirection::Left => dx < -0.05,
            ArrowDirection::Down => dy > 0.15,
            ArrowDirection::Up => dy < -0.15,
        };

        if is_valid {
            let score = match direction {
                // When navigating horizontally, heavily weight horizontal proximity and penalize row deviation
                ArrowDirection::Right => dx + dy.abs() * 4.0,
                ArrowDirection::Left => (-dx) + dy.abs() * 4.0,
                // When navigating vertically, prioritize the column match with strong vertical alignment
                ArrowDirection::Down => dy * 2.5 + dx.abs(),
                ArrowDirection::Up => (-dy) * 2.5 + dx.abs(),
            };

            if score < best_score {
                best_score = score;
                best_key = candidate.index;
            }
        }
    }

    best_key
}

#[derive(Debug, Clone, Copy)]
pub struct VisualKey {
    pub name: &'static str,
    pub index: u8,
    pub width_units: f32,
}

pub fn get_ansi_104_layout() -> [Vec<VisualKey>; 6] {
    let mut rows: [Vec<VisualKey>; 6] = Default::default();
    for k in ANSI_104_KEYS {
        let row_idx = if k.y < 1.0 {
            0
        } else if k.y < 2.0 {
            1
        } else if k.y < 3.0 {
            2
        } else if k.y < 4.0 {
            3
        } else if k.y < 5.0 {
            4
        } else {
            5
        };
        rows[row_idx].push(VisualKey {
            name: k.name,
            index: k.index,
            width_units: k.w,
        });
    }
    rows
}

fn load_app_icon() -> Option<egui::IconData> {
    let icon_bytes = include_bytes!("../assets/Yarny_256.png");
    if let Ok(img) = image::load_from_memory(icon_bytes) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Some(egui::IconData {
            rgba: rgba.into_raw(),
            width: w,
            height: h,
        })
    } else {
        None
    }
}

pub(crate) fn load_text_logo_image() -> Option<egui::ColorImage> {
    let logo_bytes = include_bytes!("../assets/FantasTech Text Logo.png");
    if let Ok(img) = image::load_from_memory(logo_bytes) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Some(egui::ColorImage::from_rgba_unmultiplied(
            [w as usize, h as usize],
            &rgba.into_raw(),
        ))
    } else {
        None
    }
}

pub fn run_gui() -> eframe::Result<()> {
    fantastech_core::install_crash_handler("fantastech-gui");

    #[cfg(target_os = "windows")]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        std::env::set_var("WGPU_BACKEND", "dx12");
    }

    let _ = tracing_subscriber::fmt::try_init();

    // Check single instance: if already running, focus existing window and exit!
    if fantastech_core::focus_existing_gui() {
        println!("ℹ️ FantasTech is already running. Brought existing window to focus.");
        return Ok(());
    }

    let initial_config = FantasTechConfig::load();
    let initial_theme = initial_config.gui_theme;

    let mut viewport = egui::ViewportBuilder::default()
        .with_app_id("org.fantastech.FantasTech")
        .with_inner_size([1520.0, 960.0])
        .with_min_inner_size([1240.0, 720.0])
        .with_title("FantasTech — Fantech MK894 Maxpower II Control Center");
    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "org.fantastech.FantasTech",
        native_options,
        Box::new(move |cc| {
            setup_custom_fonts(&cc.egui_ctx);
            apply_gui_theme(&cc.egui_ctx, initial_theme);

            let (focus_tx, focus_rx) = std::sync::mpsc::channel();
            let ctx_clone = cc.egui_ctx.clone();
            let guard = fantastech_core::SingleInstanceGuard::bind(focus_tx, move || {
                ctx_clone.request_repaint();
            }).ok();

            let mut app = FantasTechApp::new();
            app.focus_rx = Some(focus_rx);
            app._single_instance_guard = guard;

            if std::env::args().any(|a| a == "--settings-update" || a == "--update") {
                app.current_tab = NavTab::Setting;
            }
            Ok(Box::new(app))
        }),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileDomain {
    Button,
    Lighting,
}

struct FantasTechApp {
    config: FantasTechConfig,
    profile_manager: ProfileManager,
    selected_lighting_profile: String,
    selected_button_profile: String,
    selected_macro_profile: String,
    #[allow(dead_code)]
    selected_settings_profile: String,

    // Profile creation & deletion modal state
    profile_modal_open: bool,
    profile_modal_domain: ProfileDomain,
    profile_modal_name: String,
    profile_modal_copy_current: bool,
    delete_profile_modal_open: bool,
    delete_profile_id: String,
    delete_profile_name: String,
    delete_profile_domain: ProfileDomain,

    // Hardware & connectivity
    is_emulator: bool,
    connected: bool,
    service_active: bool,
    permission_denied_node: Option<PathBuf>,
    connection_monitor: platform::ConnectionMonitor,
    connection_checking: bool,
    permission_modal_open: bool,
    status_message: String,
    device_model: String,
    firmware_version: String,
    active_profile: u8,
    active_layer: KeyLayer,
    dry_run_enabled: bool,

    // Navigation
    current_tab: NavTab,

    // Button page state
    profiles: [[[KeyAction; SPARSE_KEY_COUNT]; 2]; 3],
    selected_key: Option<u8>,
    palette_tab: PaletteTab,
    palette_search: String,
    undo_keymaps: Vec<Vec<KeyAction>>,
    redo_keymaps: Vec<Vec<KeyAction>>,

    // Modify modal state
    modify_modal_open: bool,
    modify_tab: ModifyTab,
    selected_system_action: Option<KeyAction>,
    combo_ctrl: bool,
    combo_shift: bool,
    combo_alt: bool,
    combo_win: bool,
    combo_key_str: String,
    macro_modal_slot: usize,
    macro_modal_loop_mode: u8,
    macro_modal_repeat: u8,

    // Lighting page state
    selected_mode: u8,
    brightness: u8,
    speed: u8,
    direction: u8,
    is_fixed_color: bool,
    current_color: [u8; 3],
    copied_color: Option<[u8; 3]>,
    recent_colors: Vec<[u8; 3]>,
    text_logo_texture: Option<egui::TextureHandle>,
    key_colors: [Color32; SPARSE_KEY_COUNT],
    custom_selected_keys: std::collections::BTreeSet<u8>,
    custom_select_all: bool,
    pub(crate) reactive_origin: Option<(f32, f32, f32)>,

    // Macro page state
    macro_memory: MacroMemoryTable,
    active_macro_slot: usize,
    macro_sequence: MacroSequence,
    is_recording: bool,
    recording_last_time: Option<std::time::Instant>,
    fixed_delay_enabled: bool,
    fixed_delay_ms: u16,

    // Setting page state
    factory_reset_input: String,
    factory_reset_modal_open: bool,
    sleep_timeout_mins: u16,
    backup_file_path: String,
    #[allow(dead_code)]
    install_status_message: Option<String>,

    // Firmware update state
    update_available: bool,
    latest_firmware_version: String,
    firmware_update_modal_open: bool,
    auto_update_firmware_modal_open: bool,
    firmware_progress_modal_open: bool,
    firmware_flashing_in_progress: bool,
    firmware_progress: Option<fantastech_device::FirmwareProgressUpdate>,
    custom_firmware_path: Option<PathBuf>,
    firmware_status_text: String,
    firmware_progress_rx: Option<std::sync::mpsc::Receiver<fantastech_device::FirmwareProgressUpdate>>,

    // Single-instance IPC focus receiver & guard
    focus_rx: Option<std::sync::mpsc::Receiver<String>>,
    _single_instance_guard: Option<fantastech_core::SingleInstanceGuard>,

    // Auto-update modal state
    auto_update_modal_just_opened: bool,

    // Active firmware update check timestamp
    checking_updates_start: Option<std::time::Instant>,

    // Keyboard shortcuts help modal
    shortcuts_help_modal_open: bool,
}

impl FantasTechApp {
    fn new() -> Self {
        #[cfg(test)]
        let (connection_monitor, initial_snapshot) = (
            platform::ConnectionMonitor::new_mock(),
            platform::ConnectionSnapshot::new_mock(),
        );
        #[cfg(not(test))]
        let (connection_monitor, initial_snapshot) = {
            let initial = platform::detect_connection(true);
            let monitor = platform::ConnectionMonitor::start();
            (monitor, initial)
        };

        let mut base = default_layer_keymap(KeyLayer::Base);
        let mut fn_layer = default_layer_keymap(KeyLayer::Fn);
        if let Some((hw_base, hw_fn)) = initial_snapshot.hardware_keymaps {
            base = hw_base;
            fn_layer = hw_fn;
        }
        let profiles = [
            [base, fn_layer],
            [base, fn_layer],
            [base, fn_layer],
        ];

        let config = FantasTechConfig::load();
        if config.auto_start_enabled && !platform::is_autostart_registered() {
            let _ = platform::service_action("enable");
        }
        let profile_manager = ProfileManager::new(None);
        let _ = profile_manager.ensure_seed_profiles();
        let service_active = initial_snapshot.service_active;
        let connected = initial_snapshot.connected;
        let is_emulator = initial_snapshot.is_emulator;
        let permission_denied_node = initial_snapshot.permission_denied_node.clone();
        let firmware_version = initial_snapshot
            .device_info
            .as_ref()
            .map(|info| info.firmware_version.clone())
            .unwrap_or_else(|| "V101".to_string());
        let active_profile = initial_snapshot
            .device_info
            .as_ref()
            .map(|info| info.active_profile.min(2))
            .unwrap_or(0);
        let status_message = if let Some(ref error) = initial_snapshot.error_message {
            error.clone()
        } else if initial_snapshot.service_active {
            "Connected to FantasTech background service".to_owned()
        } else if initial_snapshot.permission_denied_node.is_some() {
            "Keyboard detected, but device permissions prevent access".to_owned()
        } else if initial_snapshot.connected && !initial_snapshot.is_emulator {
            if let Some(ref hw) = initial_snapshot.lighting_config {
                format!("Connected directly to keyboard: FW {}, Profile {}, Mode: {}", firmware_version, active_profile + 1, hw.mode.name())
            } else {
                "Connected directly to the keyboard".to_owned()
            }
        } else {
            "No physical keyboard detected; emulator mode is available".to_owned()
        };

        let selected_lighting_profile = config.active_lighting_profile.clone();
        let selected_button_profile = config.active_button_profile.clone();
        let selected_macro_profile = config.active_macro_profile.clone();
        let selected_settings_profile = config.active_settings_profile.clone();

        let (init_mode, init_b, init_s, init_dir, init_color, init_fixed, init_key_colors, init_profile) = if let Some(ref hw) = initial_snapshot.lighting_config {
            let b = if hw.brightness == 0 && hw.mode != LightingMode::Off { 4 } else { hw.brightness };
            let s = hw.speed;
            let dir = match hw.direction { AnimationDirection::LeftToRight => 1, _ => 0 };
            let fixed = hw.color_mode == ColorMode::FixedColor || hw.mode == LightingMode::Static;
            let color = [hw.color.r, hw.color.g, hw.color.b];
            let colors = [Color32::from_rgb(color[0], color[1], color[2]); SPARSE_KEY_COUNT];
            let profiles = profile_manager.list_lighting_profiles();
            let matched_profile = profiles
                .iter()
                .find(|p| p.mode == hw.mode && (!hw.mode.supports_color() || p.color == color))
                .or_else(|| profiles.iter().find(|p| p.mode == hw.mode))
                .map(|p| p.id.clone())
                .unwrap_or_else(|| selected_lighting_profile.clone());
            (hw.mode as u8, b, s, dir, color, fixed, colors, matched_profile)
        } else if let Some(p) = profile_manager.load_lighting_profile(&selected_lighting_profile) {
            let b = if p.brightness <= 4 { p.brightness } else { ((p.brightness as f32 / 100.0) * 4.0).round().clamp(0.0, 4.0) as u8 };
            let s = if p.speed <= 4 { p.speed } else { ((p.speed as f32 / 100.0) * 4.0).round().clamp(0.0, 4.0) as u8 };
            let fixed = p.color_mode.map(|cm| cm == ColorMode::FixedColor).unwrap_or(p.mode == LightingMode::Static);
            let mut colors = [Color32::from_rgb(p.color[0], p.color[1], p.color[2]); SPARSE_KEY_COUNT];
            if p.mode == LightingMode::CustomMatrixCanvas {
                let matrix = p.to_matrix();
                for (idx, rgb) in matrix.iter().enumerate() {
                    if idx < SPARSE_KEY_COUNT {
                        colors[idx] = Color32::from_rgb(rgb.r, rgb.g, rgb.b);
                    }
                }
            }
            (p.mode as u8, if b == 0 && p.mode != LightingMode::Off { 4 } else { b }, s, p.direction, p.color, fixed, colors, selected_lighting_profile.clone())
        } else {
            (4, 4, 2, 0, [0, 113, 207], false, [Color32::from_rgb(0, 113, 207); SPARSE_KEY_COUNT], selected_lighting_profile.clone())
        };
        let selected_lighting_profile = init_profile;

        let mut app = Self {
            config,
            profile_manager,
            selected_lighting_profile,
            selected_button_profile,
            selected_macro_profile,
            selected_settings_profile,

            profile_modal_open: false,
            profile_modal_domain: ProfileDomain::Button,
            profile_modal_name: String::new(),
            profile_modal_copy_current: true,
            delete_profile_modal_open: false,
            delete_profile_id: String::new(),
            delete_profile_name: String::new(),
            delete_profile_domain: ProfileDomain::Button,

            is_emulator,
            connected,
            service_active,
            permission_denied_node,
            connection_monitor,
            connection_checking: false,
            permission_modal_open: false,
            status_message,
            device_model: "Fantech MK894 Maxpower II".to_string(),
            firmware_version,
            active_profile,
            active_layer: KeyLayer::Base,
            dry_run_enabled: cfg!(test),

            current_tab: NavTab::Button,

            profiles,
            selected_key: None, // No key selected until user clicks on canvas
            palette_tab: PaletteTab::Basic,
            palette_search: String::new(),
            undo_keymaps: Vec::new(),
            redo_keymaps: Vec::new(),

            modify_modal_open: false,
            modify_tab: ModifyTab::KeyboardKeys,
            selected_system_action: None,
            combo_ctrl: true,
            combo_shift: false,
            combo_alt: false,
            combo_win: false,
            combo_key_str: "C".to_string(),
            macro_modal_slot: 0,
            macro_modal_loop_mode: 0,
            macro_modal_repeat: 1,

            selected_mode: init_mode,
            brightness: init_b,
            speed: init_s,
            direction: init_dir,
            is_fixed_color: init_fixed,
            current_color: init_color,
            copied_color: None,
            recent_colors: vec![
                [229, 62, 62],  // Red
                [0, 113, 207],  // Blue
                [34, 197, 94],  // Green
                [168, 85, 247], // Purple
                [249, 115, 22], // Orange
                [6, 182, 212],  // Cyan
            ],
            text_logo_texture: None,
            key_colors: init_key_colors,
            custom_selected_keys: std::collections::BTreeSet::new(),
            custom_select_all: false,
            reactive_origin: None,
            install_status_message: None,

            macro_memory: MacroMemoryTable::new_empty(),
            active_macro_slot: 0,
            macro_sequence: MacroSequence {
                name: "Macro 1".to_string(),
                slot: 0,
                loop_mode: 0,
                repeat: 1,
                steps: vec![
                    MacroItem {
                        action: "down".to_string(),
                        scancode: 0x06, // C
                        code: 0x06,
                        name: Some("C".to_string()),
                        delay_ms: 50,
                    },
                    MacroItem {
                        action: "up".to_string(),
                        scancode: 0x06,
                        code: 0x06,
                        name: Some("C".to_string()),
                        delay_ms: 25,
                    },
                ],
            },
            is_recording: false,
            recording_last_time: None,
            fixed_delay_enabled: false,
            fixed_delay_ms: 50,

            factory_reset_input: String::new(),
            factory_reset_modal_open: false,
            sleep_timeout_mins: 10,
            backup_file_path: "fantastech_mk894_backup.json".to_string(),

            update_available: false,
            latest_firmware_version: fantastech_core::CATALOG_VERSION_STR.to_string(),
            firmware_update_modal_open: false,
            auto_update_firmware_modal_open: false,
            firmware_progress_modal_open: false,
            firmware_flashing_in_progress: false,
            firmware_progress: None,
            custom_firmware_path: None,
            firmware_status_text: "Firmware is up to date (v1.01)".to_string(),
            firmware_progress_rx: None,

            focus_rx: None,
            _single_instance_guard: None,

            auto_update_modal_just_opened: false,
            checking_updates_start: None,
            shortcuts_help_modal_open: false,
        };
        if initial_snapshot.connected {
            app.apply_hardware_snapshot(&initial_snapshot);
        }
        app
    }
}
