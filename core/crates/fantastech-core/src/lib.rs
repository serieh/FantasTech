//! High-level abstractions for themes, keymap naming, and the 4KB macro compiler.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

pub mod config;
pub mod firmware;
pub mod ipc;
pub mod profiles;
pub mod single_instance;

pub use config::{get_fantastech_config_dir, ColorDisplayFormat, FantasTechConfig, GuiTheme};
pub use firmware::{
    check_firmware_update_available, compute_sha256, get_catalog_entry,
    get_embedded_firmware_bytes, validate_firmware_binary, BOOTLOADER_OFFSET, BOOTLOADER_PID,
    CATALOG_PID, CATALOG_SHA256, CATALOG_VERSION_CODE, CATALOG_VERSION_STR, CATALOG_VID,
    EMBEDDED_FIRMWARE_V101, EXPECTED_FULL_IMAGE_LEN, EXPECTED_PAYLOAD_LEN, FirmwareCatalogEntry,
    FirmwareValidationInfo,
};
pub use ipc::{
    ipc_socket_path, is_service_running, send_ipc_request, IpcRequest, IpcResponse, ServiceStatus,
    WINDOWS_IPC_PORT, WINDOWS_TRAY_PORT, WINDOWS_GUI_PORT,
};
pub use profiles::{
    write_user_owned_file, ButtonProfile, LightingProfile, MacroProfile, ProfileManager, SettingsProfile,
};
pub use single_instance::{
    focus_existing_gui, focus_existing_gui_with_action, is_tray_already_running,
    single_instance_socket_path, tray_socket_path, SingleInstanceGuard, TrayInstanceGuard,
};

/// Installs a global panic hook that writes crash diagnostics to `crash.log` in app data.
pub fn install_crash_handler(component: &'static str) {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let crash_log_path = get_fantastech_config_dir().join("crash.log");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let payload = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "Unknown panic payload"
        };
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed");

        let log_entry = format!(
            "[timestamp: {}] CRASH in '{}' on thread '{}' at {}:\nPayload: {}\n\n",
            now, component, thread_name, location, payload
        );

        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&crash_log_path)
        {
            use std::io::Write;
            let _ = file.write_all(log_entry.as_bytes());
        }

        original_hook(panic_info);
    }));
}

use fantastech_protocol::constants::{MACRO_SLOT_COUNT, SPARSE_KEY_COUNT};
use fantastech_protocol::keymap::{ActionCategory, KeyAction, KeyLayer};
use fantastech_protocol::lighting::RgbColor;
use fantastech_protocol::macro_mem::{MacroActionRecord, MacroEventType, MacroMemoryTable};
use fantastech_protocol::ProtocolError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// High-level core errors.
#[derive(Error, Debug)]
pub enum CoreError {
    /// TOML parsing or serialization error.
    #[error("TOML serialization error: {0}")]
    Toml(#[from] toml::ser::Error),

    /// TOML deserialization error.
    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    /// JSON serialization or parsing error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Underlying protocol error.
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    /// Macro exceeds hardware limits.
    #[error("Macro compilation failed: {0}")]
    MacroCompilationFailed(&'static str),

    /// Unknown key name.
    #[error("Unknown key identifier: {0}")]
    UnknownKeyName(String),

    /// Unknown action descriptor.
    #[error("Unknown action descriptor: {0}")]
    UnknownAction(String),
}

/// A physical key definition with hardware matrix coordinates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct KeyEntry {
    /// Human-friendly key name.
    pub name: &'static str,
    /// Electrical matrix coordinate index (0..126).
    pub index: u8,
    /// Physical matrix row (0..5).
    pub row: u8,
    /// Physical matrix column (0..20).
    pub col: u8,
}

/// The complete, 100% verified hardware matrix of all 104 physical switch positions on the MK894.
pub const KEY_DEFINITIONS: &[KeyEntry] = &[
    // Row 0 (Function Row - 16 keys)
    KeyEntry { name: "Esc", index: 0, row: 0, col: 0 },
    KeyEntry { name: "F1", index: 1, row: 0, col: 1 },
    KeyEntry { name: "F2", index: 2, row: 0, col: 2 },
    KeyEntry { name: "F3", index: 3, row: 0, col: 3 },
    KeyEntry { name: "F4", index: 4, row: 0, col: 4 },
    KeyEntry { name: "F5", index: 5, row: 0, col: 5 },
    KeyEntry { name: "F6", index: 6, row: 0, col: 6 },
    KeyEntry { name: "F7", index: 7, row: 0, col: 7 },
    KeyEntry { name: "F8", index: 8, row: 0, col: 8 },
    KeyEntry { name: "F9", index: 9, row: 0, col: 9 },
    KeyEntry { name: "F10", index: 10, row: 0, col: 10 },
    KeyEntry { name: "F11", index: 11, row: 0, col: 11 },
    KeyEntry { name: "F12", index: 12, row: 0, col: 12 },
    KeyEntry { name: "PrtSc", index: 13, row: 0, col: 13 },
    KeyEntry { name: "ScrLK", index: 14, row: 0, col: 14 },
    KeyEntry { name: "Pause", index: 15, row: 0, col: 15 },

    // Row 1 (Number row - 21 keys)
    KeyEntry { name: "~", index: 21, row: 1, col: 0 },
    KeyEntry { name: "1", index: 22, row: 1, col: 1 },
    KeyEntry { name: "2", index: 23, row: 1, col: 2 },
    KeyEntry { name: "3", index: 24, row: 1, col: 3 },
    KeyEntry { name: "4", index: 25, row: 1, col: 4 },
    KeyEntry { name: "5", index: 26, row: 1, col: 5 },
    KeyEntry { name: "6", index: 27, row: 1, col: 6 },
    KeyEntry { name: "7", index: 28, row: 1, col: 7 },
    KeyEntry { name: "8", index: 29, row: 1, col: 8 },
    KeyEntry { name: "9", index: 30, row: 1, col: 9 },
    KeyEntry { name: "0", index: 31, row: 1, col: 10 },
    KeyEntry { name: "-", index: 32, row: 1, col: 11 },
    KeyEntry { name: "+", index: 33, row: 1, col: 12 },
    KeyEntry { name: "Backspace", index: 34, row: 1, col: 13 },
    KeyEntry { name: "Insert", index: 35, row: 1, col: 14 },
    KeyEntry { name: "Home", index: 36, row: 1, col: 15 },
    KeyEntry { name: "PageUp", index: 37, row: 1, col: 16 },
    KeyEntry { name: "NumLock", index: 38, row: 1, col: 17 },
    KeyEntry { name: "Num /", index: 39, row: 1, col: 18 },
    KeyEntry { name: "Num *", index: 40, row: 1, col: 19 },
    KeyEntry { name: "Num -", index: 41, row: 1, col: 20 },

    // Row 2 (QWERTY row - 21 keys)
    KeyEntry { name: "Tab", index: 42, row: 2, col: 0 },
    KeyEntry { name: "Q", index: 43, row: 2, col: 1 },
    KeyEntry { name: "W", index: 44, row: 2, col: 2 },
    KeyEntry { name: "E", index: 45, row: 2, col: 3 },
    KeyEntry { name: "R", index: 46, row: 2, col: 4 },
    KeyEntry { name: "T", index: 47, row: 2, col: 5 },
    KeyEntry { name: "Y", index: 48, row: 2, col: 6 },
    KeyEntry { name: "U", index: 49, row: 2, col: 7 },
    KeyEntry { name: "I", index: 50, row: 2, col: 8 },
    KeyEntry { name: "O", index: 51, row: 2, col: 9 },
    KeyEntry { name: "P", index: 52, row: 2, col: 10 },
    KeyEntry { name: "[", index: 53, row: 2, col: 11 },
    KeyEntry { name: "]", index: 54, row: 2, col: 12 },
    KeyEntry { name: "\\", index: 55, row: 2, col: 13 },
    KeyEntry { name: "Delete", index: 56, row: 2, col: 14 },
    KeyEntry { name: "End", index: 57, row: 2, col: 15 },
    KeyEntry { name: "PageDown", index: 58, row: 2, col: 16 },
    KeyEntry { name: "Num 7", index: 59, row: 2, col: 17 },
    KeyEntry { name: "Num 8", index: 60, row: 2, col: 18 },
    KeyEntry { name: "Num 9", index: 61, row: 2, col: 19 },
    KeyEntry { name: "Num +", index: 62, row: 2, col: 20 },

    // Row 3 (Home row - 16 keys)
    KeyEntry { name: "CapsLock", index: 63, row: 3, col: 0 },
    KeyEntry { name: "A", index: 64, row: 3, col: 1 },
    KeyEntry { name: "S", index: 65, row: 3, col: 2 },
    KeyEntry { name: "D", index: 66, row: 3, col: 3 },
    KeyEntry { name: "F", index: 67, row: 3, col: 4 },
    KeyEntry { name: "G", index: 68, row: 3, col: 5 },
    KeyEntry { name: "H", index: 69, row: 3, col: 6 },
    KeyEntry { name: "J", index: 70, row: 3, col: 7 },
    KeyEntry { name: "K", index: 71, row: 3, col: 8 },
    KeyEntry { name: "L", index: 72, row: 3, col: 9 },
    KeyEntry { name: ";", index: 73, row: 3, col: 10 },
    KeyEntry { name: "'", index: 74, row: 3, col: 11 },
    KeyEntry { name: "Enter", index: 76, row: 3, col: 12 },
    KeyEntry { name: "Num 4", index: 80, row: 3, col: 13 },
    KeyEntry { name: "Num 5", index: 81, row: 3, col: 14 },
    KeyEntry { name: "Num 6", index: 82, row: 3, col: 15 },

    // Row 4 (ZXCV row - 17 keys)
    KeyEntry { name: "LeftShift", index: 84, row: 4, col: 0 },
    KeyEntry { name: "Z", index: 86, row: 4, col: 2 },
    KeyEntry { name: "X", index: 87, row: 4, col: 3 },
    KeyEntry { name: "C", index: 88, row: 4, col: 4 },
    KeyEntry { name: "V", index: 89, row: 4, col: 5 },
    KeyEntry { name: "B", index: 90, row: 4, col: 6 },
    KeyEntry { name: "N", index: 91, row: 4, col: 7 },
    KeyEntry { name: "M", index: 92, row: 4, col: 8 },
    KeyEntry { name: "<", index: 93, row: 4, col: 9 },
    KeyEntry { name: ">", index: 94, row: 4, col: 10 },
    KeyEntry { name: "/", index: 95, row: 4, col: 11 },
    KeyEntry { name: "RightShift", index: 97, row: 4, col: 13 },
    KeyEntry { name: "Up", index: 99, row: 4, col: 15 },
    KeyEntry { name: "Num 1", index: 101, row: 4, col: 17 },
    KeyEntry { name: "Num 2", index: 102, row: 4, col: 18 },
    KeyEntry { name: "Num 3", index: 103, row: 4, col: 19 },
    KeyEntry { name: "Num Enter", index: 104, row: 4, col: 20 },

    // Row 5 (Bottom row & Arrows - 13 keys)
    KeyEntry { name: "LeftCtrl", index: 105, row: 5, col: 0 },
    KeyEntry { name: "Win", index: 106, row: 5, col: 1 },
    KeyEntry { name: "LeftAlt", index: 107, row: 5, col: 2 },
    KeyEntry { name: "Space", index: 111, row: 5, col: 6 },
    KeyEntry { name: "RightAlt", index: 115, row: 5, col: 10 },
    KeyEntry { name: "Fn", index: 116, row: 5, col: 11 },
    KeyEntry { name: "App", index: 117, row: 5, col: 12 },
    KeyEntry { name: "RightCtrl", index: 118, row: 5, col: 13 },
    KeyEntry { name: "Left", index: 119, row: 5, col: 14 },
    KeyEntry { name: "Down", index: 120, row: 5, col: 15 },
    KeyEntry { name: "Right", index: 121, row: 5, col: 16 },
    KeyEntry { name: "Num 0", index: 123, row: 5, col: 18 },
    KeyEntry { name: "Num .", index: 124, row: 5, col: 19 },
];

/// Returns all keys in the specified matrix row (0..5).
pub fn keys_for_row(row: u8) -> Vec<&'static KeyEntry> {
    KEY_DEFINITIONS.iter().filter(|k| k.row == row).collect()
}

/// Returns all keys belonging to a named functional zone.
pub fn keys_for_zone(zone: &str) -> Option<Vec<&'static KeyEntry>> {
    match zone.to_lowercase().as_str() {
        "wasd" => Some(KEY_DEFINITIONS.iter().filter(|k| [44, 64, 65, 66].contains(&k.index)).collect()),
        "arrows" | "arrow" => Some(KEY_DEFINITIONS.iter().filter(|k| [99, 119, 120, 121].contains(&k.index)).collect()),
        "numpad" | "num" => Some(KEY_DEFINITIONS.iter().filter(|k| [38, 39, 40, 41, 59, 60, 61, 62, 80, 81, 82, 101, 102, 103, 104, 122, 123, 124].contains(&k.index)).collect()),
        "nav" | "navigation" => Some(KEY_DEFINITIONS.iter().filter(|k| [35, 36, 37, 56, 57, 58].contains(&k.index)).collect()),
        "fkeys" | "fn" | "function" => Some(KEY_DEFINITIONS.iter().filter(|k| (1..=12).contains(&k.index) || k.index == 0).collect()),
        "media" => Some(KEY_DEFINITIONS.iter().filter(|k| (1..=8).contains(&k.index)).collect()),
        "bottom" => Some(KEY_DEFINITIONS.iter().filter(|k| k.row == 5).collect()),
        _ => None,
    }
}

/// Resolves alternate electrical pad index for edge switches with dual solder positions.
pub fn get_mirror_key_index(key_index: u8) -> Option<u8> {
    match key_index {
        84 => Some(85),
        85 => Some(84),
        96 => Some(97),
        97 => Some(96),
        110 => Some(111),
        111 => Some(110),
        122 => Some(123),
        123 => Some(122),
        76 => Some(75),
        75 => Some(76),
        _ => None,
    }
}

/// Maps human key name to default USB HID scancode.
pub fn default_scancode_for_key(name: &str) -> Option<u8> {
    match name {
        "Esc" => Some(41),
        "F1" => Some(58),
        "F2" => Some(59),
        "F3" => Some(60),
        "F4" => Some(61),
        "F5" => Some(62),
        "F6" => Some(63),
        "F7" => Some(64),
        "F8" => Some(65),
        "F9" => Some(66),
        "F10" => Some(67),
        "F11" => Some(68),
        "F12" => Some(69),
        "PrtSc" => Some(70),
        "ScrLK" => Some(71),
        "Pause" => Some(72),
        "~" => Some(53),
        "1" => Some(30),
        "2" => Some(31),
        "3" => Some(32),
        "4" => Some(33),
        "5" => Some(34),
        "6" => Some(35),
        "7" => Some(36),
        "8" => Some(37),
        "9" => Some(38),
        "0" => Some(39),
        "-" => Some(45),
        "+" => Some(46),
        "Backspace" | "Bksp" | "Back" => Some(42),
        "Insert" | "Ins" => Some(73),
        "Home" => Some(74),
        "PageUp" | "PgUp" => Some(75),
        "NumLock" | "NumLk" => Some(83),
        "Num /" => Some(84),
        "Num *" => Some(85),
        "Num -" => Some(86),
        "Tab" | "Tap" => Some(43),
        "Q" => Some(20),
        "W" => Some(26),
        "E" => Some(8),
        "R" => Some(21),
        "T" => Some(23),
        "Y" => Some(28),
        "U" => Some(24),
        "I" => Some(12),
        "O" => Some(18),
        "P" => Some(19),
        "[" => Some(47),
        "]" => Some(48),
        "\\" => Some(49),
        "Delete" | "Del" => Some(76),
        "End" => Some(77),
        "PageDown" | "PgDn" => Some(78),
        "Num 7" => Some(95),
        "Num 8" => Some(96),
        "Num 9" => Some(97),
        "Num +" => Some(87),
        "CapsLock" | "Caps" => Some(57),
        "A" => Some(4),
        "S" => Some(22),
        "D" => Some(7),
        "F" => Some(9),
        "G" => Some(10),
        "H" => Some(11),
        "J" => Some(13),
        "K" => Some(14),
        "L" => Some(15),
        ";" => Some(51),
        "'" => Some(52),
        "Enter" => Some(40),
        "Num 4" => Some(92),
        "Num 5" => Some(93),
        "Num 6" => Some(94),
        "LeftShift" | "Shift" => Some(225),
        "Z" => Some(29),
        "X" => Some(27),
        "C" => Some(6),
        "V" => Some(25),
        "B" => Some(5),
        "N" => Some(17),
        "M" => Some(16),
        "<" => Some(54),
        ">" => Some(55),
        "/" | "?" => Some(56),
        "RightShift" => Some(229),
        "Up" => Some(82),
        "Num 1" => Some(89),
        "Num 2" => Some(90),
        "Num 3" => Some(91),
        "Num Enter" => Some(88),
        "LeftCtrl" | "Ctrl" => Some(224),
        "Win" => Some(227),
        "LeftAlt" | "Alt" => Some(226),
        "Space" => Some(44),
        "RightAlt" => Some(230),
        "Fn" => Some(255),
        "App" => Some(101),
        "RightCtrl" => Some(228),
        "Left" => Some(80),
        "Down" => Some(81),
        "Right" => Some(79),
        "Num 0" => Some(98),
        "Num ." => Some(99),
        _ => None,
    }
}

/// Resolves standard USB HID scancode for any human key identifier (case-insensitive).
pub fn scancode_for_key_name(name: &str) -> Option<u8> {
    let lower = name.trim().to_lowercase();
    match lower.as_str() {
        "a" => Some(4),
        "b" => Some(5),
        "c" => Some(6),
        "d" => Some(7),
        "e" => Some(8),
        "f" => Some(9),
        "g" => Some(10),
        "h" => Some(11),
        "i" => Some(12),
        "j" => Some(13),
        "k" => Some(14),
        "l" => Some(15),
        "m" => Some(16),
        "n" => Some(17),
        "o" => Some(18),
        "p" => Some(19),
        "q" => Some(20),
        "r" => Some(21),
        "s" => Some(22),
        "t" => Some(23),
        "u" => Some(24),
        "v" => Some(25),
        "w" => Some(26),
        "x" => Some(27),
        "y" => Some(28),
        "z" => Some(29),
        "1" => Some(30),
        "2" => Some(31),
        "3" => Some(32),
        "4" => Some(33),
        "5" => Some(34),
        "6" => Some(35),
        "7" => Some(36),
        "8" => Some(37),
        "9" => Some(38),
        "0" => Some(39),
        "enter" | "return" => Some(40),
        "esc" | "escape" => Some(41),
        "backspace" | "bksp" => Some(42),
        "tab" => Some(43),
        "space" | "spc" => Some(44),
        "-" | "minus" => Some(45),
        "=" | "+" | "equal" | "equals" | "plus" => Some(46),
        "[" | "{" | "openbracket" | "bracketleft" => Some(47),
        "]" | "}" | "closebracket" | "bracketright" => Some(48),
        "\\" | "|" | "backslash" | "pipe" => Some(49),
        ";" | ":" | "semicolon" | "colon" => Some(51),
        "'" | "\"" | "quote" | "singlequote" | "doublequote" => Some(52),
        "`" | "~" | "grave" | "backquote" => Some(53),
        "," | "<" | "comma" => Some(54),
        "." | ">" | "period" | "dot" => Some(55),
        "/" | "?" | "slash" | "question" | "questionmark" => Some(56),
        "caps" | "capslock" => Some(57),
        "f1" => Some(58),
        "f2" => Some(59),
        "f3" => Some(60),
        "f4" => Some(61),
        "f5" => Some(62),
        "f6" => Some(63),
        "f7" => Some(64),
        "f8" => Some(65),
        "f9" => Some(66),
        "f10" => Some(67),
        "f11" => Some(68),
        "f12" => Some(69),
        "prtsc" | "print" | "printscreen" => Some(70),
        "scrlk" | "scroll" | "scrolllock" => Some(71),
        "pause" => Some(72),
        "insert" | "ins" => Some(73),
        "home" => Some(74),
        "pageup" | "pgup" => Some(75),
        "del" | "delete" => Some(76),
        "end" => Some(77),
        "pagedown" | "pgdn" => Some(78),
        "right" | "arrowright" => Some(79),
        "left" | "arrowleft" => Some(80),
        "down" | "arrowdown" => Some(81),
        "up" | "arrowup" => Some(82),
        "numlock" | "num" => Some(83),
        "num /" | "num_divide" | "num/" => Some(84),
        "num *" | "num_multiply" | "num*" => Some(85),
        "num -" | "num_subtract" | "num-" => Some(86),
        "num +" | "num_add" | "num+" => Some(87),
        "num enter" | "num_enter" | "numenter" => Some(88),
        "num 1" | "num1" => Some(89),
        "num 2" | "num2" => Some(90),
        "num 3" | "num3" => Some(91),
        "num 4" | "num4" => Some(92),
        "num 5" | "num5" => Some(93),
        "num 6" | "num6" => Some(94),
        "num 7" | "num7" => Some(95),
        "num 8" | "num8" => Some(96),
        "num 9" | "num9" => Some(97),
        "num 0" | "num0" => Some(98),
        "num ." | "num." => Some(99),
        "app" | "menu" => Some(101),
        "f13" => Some(104),
        "f14" => Some(105),
        "f15" => Some(106),
        "f16" => Some(107),
        "f17" => Some(108),
        "f18" => Some(109),
        "f19" => Some(110),
        "f20" => Some(111),
        "f21" => Some(112),
        "f22" => Some(113),
        "f23" => Some(114),
        "f24" => Some(115),
        "ctrl" | "lctrl" | "leftctrl" => Some(224),
        "shift" | "lshift" | "leftshift" => Some(225),
        "alt" | "lalt" | "leftalt" => Some(226),
        "win" | "lwin" | "gui" | "super" => Some(227),
        "rctrl" | "rightctrl" => Some(228),
        "rshift" | "rightshift" => Some(229),
        "ralt" | "rightalt" => Some(230),
        "rwin" | "rightwin" => Some(231),
        "fn" => Some(255),
        _ => None,
    }
}

/// Resolves human key name for standard USB HID scancode.
pub fn key_name_for_scancode(scancode: u8) -> Option<&'static str> {
    match scancode {
        4 => Some("A"),
        5 => Some("B"),
        6 => Some("C"),
        7 => Some("D"),
        8 => Some("E"),
        9 => Some("F"),
        10 => Some("G"),
        11 => Some("H"),
        12 => Some("I"),
        13 => Some("J"),
        14 => Some("K"),
        15 => Some("L"),
        16 => Some("M"),
        17 => Some("N"),
        18 => Some("O"),
        19 => Some("P"),
        20 => Some("Q"),
        21 => Some("R"),
        22 => Some("S"),
        23 => Some("T"),
        24 => Some("U"),
        25 => Some("V"),
        26 => Some("W"),
        27 => Some("X"),
        28 => Some("Y"),
        29 => Some("Z"),
        30 => Some("1"),
        31 => Some("2"),
        32 => Some("3"),
        33 => Some("4"),
        34 => Some("5"),
        35 => Some("6"),
        36 => Some("7"),
        37 => Some("8"),
        38 => Some("9"),
        39 => Some("0"),
        40 => Some("Enter"),
        41 => Some("Esc"),
        42 => Some("Backspace"),
        43 => Some("Tab"),
        44 => Some("Space"),
        45 => Some("-"),
        46 => Some("+"),
        47 => Some("["),
        48 => Some("]"),
        49 => Some("\\"),
        51 => Some(";"),
        52 => Some("'"),
        53 => Some("~"),
        54 => Some("<"),
        55 => Some(">"),
        56 => Some("/"),
        57 => Some("CapsLock"),
        58 => Some("F1"),
        59 => Some("F2"),
        60 => Some("F3"),
        61 => Some("F4"),
        62 => Some("F5"),
        63 => Some("F6"),
        64 => Some("F7"),
        65 => Some("F8"),
        66 => Some("F9"),
        67 => Some("F10"),
        68 => Some("F11"),
        69 => Some("F12"),
        70 => Some("PrtSc"),
        71 => Some("ScrLK"),
        72 => Some("Pause"),
        73 => Some("Insert"),
        74 => Some("Home"),
        75 => Some("PageUp"),
        76 => Some("Delete"),
        77 => Some("End"),
        78 => Some("PageDown"),
        79 => Some("Right"),
        80 => Some("Left"),
        81 => Some("Down"),
        82 => Some("Up"),
        83 => Some("NumLock"),
        84 => Some("Num /"),
        85 => Some("Num *"),
        86 => Some("Num -"),
        87 => Some("Num +"),
        88 => Some("Num Enter"),
        89 => Some("Num 1"),
        90 => Some("Num 2"),
        91 => Some("Num 3"),
        92 => Some("Num 4"),
        93 => Some("Num 5"),
        94 => Some("Num 6"),
        95 => Some("Num 7"),
        96 => Some("Num 8"),
        97 => Some("Num 9"),
        98 => Some("Num 0"),
        99 => Some("Num ."),
        101 => Some("App"),
        104 => Some("F13"),
        105 => Some("F14"),
        106 => Some("F15"),
        107 => Some("F16"),
        108 => Some("F17"),
        109 => Some("F18"),
        110 => Some("F19"),
        111 => Some("F20"),
        112 => Some("F21"),
        113 => Some("F22"),
        114 => Some("F23"),
        115 => Some("F24"),
        224 => Some("LeftCtrl"),
        225 => Some("LeftShift"),
        226 => Some("LeftAlt"),
        227 => Some("Win"),
        228 => Some("RightCtrl"),
        229 => Some("RightShift"),
        230 => Some("RightAlt"),
        231 => Some("RightWin"),
        255 => Some("Fn"),
        _ => None,
    }
}

/// Resolves human name for 16-bit USB HID consumer control usage.
pub fn media_name_for_usage(usage: u16) -> Option<&'static str> {
    match usage {
        0x00E9 => Some("Volume +"),
        0x00EA => Some("Volume -"),
        0x00E2 => Some("Mute"),
        0x00CD => Some("Play/Pause"),
        0x00B7 => Some("Stop"),
        0x00B6 => Some("Prev Track"),
        0x00B5 => Some("Next Track"),
        0x0183 => Some("Media"),
        0x0192 => Some("Calculator"),
        0x0194 => Some("My PC"),
        0x018A => Some("Mail"),
        0x0223 => Some("Homepage"),
        0x0221 => Some("Search"),
        0x0227 => Some("Web-Refresh"),
        0x0226 => Some("Web-Stop"),
        0x0225 => Some("Web-Forward"),
        0x0224 => Some("Web-Backward"),
        0x022A => Some("Web-Favorites"),
        0x006F => Some("Display Brt+"),
        0x0070 => Some("Display Brt-"),
        _ => None,
    }
}

/// Parses a key identifier (either physical name e.g. "Space", "W", "Esc", or matrix index e.g. "44", "0").
pub fn parse_key_identifier(input: &str) -> Result<u8, CoreError> {
    let trimmed = input.trim();
    if let Ok(idx) = trimmed.parse::<u8>() {
        if (idx as usize) < SPARSE_KEY_COUNT {
            return Ok(idx);
        }
    }

    let lower = trimmed.to_lowercase();
    for key in KEY_DEFINITIONS {
        if key.name.eq_ignore_ascii_case(&lower) {
            return Ok(key.index);
        }
    }

    // Secondary aliases
    match lower.as_str() {
        "ctrl" | "lctrl" => return Ok(105),
        "rctrl" => return Ok(118),
        "shift" | "lshift" => return Ok(84),
        "rshift" => return Ok(97),
        "alt" | "lalt" => return Ok(107),
        "ralt" => return Ok(115),
        "super" | "win" | "gui" => return Ok(106),
        "spc" | "spacebar" => return Ok(111),
        "ret" | "return" | "enter" => return Ok(76),
        "numenter" | "num enter" => return Ok(104),
        "caps" => return Ok(63),
        "del" => return Ok(56),
        "ins" => return Ok(35),
        "pgup" => return Ok(37),
        "pgdn" => return Ok(58),
        "prtscr" | "print" => return Ok(13),
        "scrlk" | "scroll" => return Ok(14),
        "pause" => return Ok(15),
        "num0" | "num 0" => return Ok(123),
        _ => {}
    }

    Err(CoreError::UnknownKeyName(input.to_string()))
}

/// Generates the factory default keymap table for a specified layer.
pub fn default_layer_keymap(layer: KeyLayer) -> [KeyAction; SPARSE_KEY_COUNT] {
    let mut map = [KeyAction::disabled(); SPARSE_KEY_COUNT];

    // Assign standard HID scancodes to all 104 physical keys
    for key in KEY_DEFINITIONS {
        if let Some(scancode) = default_scancode_for_key(key.name) {
            map[key.index as usize] = KeyAction::standard(scancode);
            if let Some(mirror) = get_mirror_key_index(key.index) {
                map[mirror as usize] = KeyAction::standard(scancode);
            }
        }
    }

    if layer == KeyLayer::Fn {
        // Fn Layer overrides matching official software:
        map[0] = KeyAction::hardware_custom(16); // Esc -> Reset
        map[1] = KeyAction::multimedia(0x0183);  // F1 -> Media
        map[2] = KeyAction::multimedia(0x00EA);  // F2 -> Volume -
        map[3] = KeyAction::multimedia(0x00E9);  // F3 -> Volume +
        map[4] = KeyAction::multimedia(0x00E2);  // F4 -> Mute
        map[5] = KeyAction::multimedia(0x00B7);  // F5 -> Stop
        map[6] = KeyAction::multimedia(0x00B6);  // F6 -> Prev Track
        map[7] = KeyAction::multimedia(0x00CD);  // F7 -> Play/Pause
        map[8] = KeyAction::multimedia(0x00B5);  // F8 -> Next Track
        map[9] = KeyAction::multimedia(0x0221);  // F9 -> Search
        map[10] = KeyAction::multimedia(0x0223); // F10 -> Homepage
        map[11] = KeyAction::multimedia(0x0194); // F11 -> My PC
        map[12] = KeyAction::multimedia(0x0192); // F12 -> Calculator
        map[15] = KeyAction::multimedia(0x00CD); // Pause -> Play/Pause
        map[35] = KeyAction::hardware_custom(3); // Insert -> LedMode
        map[56] = KeyAction::hardware_custom(4); // Delete -> LedColor
        map[99] = KeyAction::hardware_custom(1); // Up -> LedBrt+
        map[120] = KeyAction::hardware_custom(2); // Down -> LedBrt-
        map[119] = KeyAction::hardware_custom(6); // Left -> Speed-
        map[121] = KeyAction::hardware_custom(5); // Right -> Speed+
        map[106] = KeyAction::win_lock();         // Win -> WinLock
        map[111] = KeyAction::hardware_custom(0); // Space -> Led On/Off
        map[110] = KeyAction::hardware_custom(0); // Space mirror -> Led On/Off
    }

    map
}

/// Parses a human-friendly action string into a strongly-typed `KeyAction`.
pub fn parse_action_str(s: &str) -> Result<KeyAction, CoreError> {
    let trimmed = s.trim();
    let lower = trimmed.to_lowercase();
    if lower.is_empty() || lower == "none" || lower == "disabled" || lower == "unmapped" {
        return Ok(KeyAction::disabled());
    }

    // Normalized string without spaces, slashes, or underscores for fast canonical token matching
    let stripped: String = lower
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '/')
        .collect();

    // 1. Hardware Lighting / Custom Key Actions (matches both bare "LedMode", "WinLock" and "custom:led-mode"):
    let custom_cand = stripped.strip_prefix("custom:").unwrap_or(&stripped);
    match custom_cand {
        "winlock" | "win-lock" | "guilock" | "gui-lock" => return Ok(KeyAction::win_lock()),
        "ledonoff" | "led-on-off" | "ledtoggle" | "led-toggle" | "toggleled" | "off" => return Ok(KeyAction::hardware_custom(0)),
        "brightness+" | "brt+" | "ledbrt+" | "led-brt+" => return Ok(KeyAction::hardware_custom(1)),
        "brightness-" | "brt-" | "ledbrt-" | "led-brt-" => return Ok(KeyAction::hardware_custom(2)),
        "ledmode" | "led-mode" | "mode" | "effect" => return Ok(KeyAction::hardware_custom(3)),
        "color" | "colorcycle" | "color-cycle" | "ledcolor" | "led-color" => return Ok(KeyAction::hardware_custom(4)),
        "speed+" | "spd+" => return Ok(KeyAction::hardware_custom(5)),
        "speed-" | "spd-" => return Ok(KeyAction::hardware_custom(6)),
        "mac" | "macdevice" | "mac-device" => return Ok(KeyAction::hardware_custom(13)),
        "win" | "windows" | "windevice" | "win-device" => return Ok(KeyAction::hardware_custom(14)),
        "reset" => return Ok(KeyAction::hardware_custom(16)),
        _ => {}
    }
    if let Some(rest) = lower.strip_prefix("custom:") {
        if let Ok(num) = rest.parse::<u8>() {
            return Ok(KeyAction::hardware_custom(num));
        }
        return Err(CoreError::UnknownAction(s.to_string()));
    }

    // 2. Mouse actions:
    let mouse_cand = stripped.strip_prefix("mouse:").unwrap_or_else(|| stripped.strip_prefix("mouse").unwrap_or(""));
    if !mouse_cand.is_empty() {
        return match mouse_cand {
            "left" | "lclick" | "btn1" => Ok(KeyAction::mouse_button(1)),
            "right" | "rclick" | "btn2" => Ok(KeyAction::mouse_button(2)),
            "middle" | "mclick" | "btn3" => Ok(KeyAction::mouse_button(4)),
            "backward" | "back" | "btn4" => Ok(KeyAction::mouse_button(8)),
            "forward" | "btn5" => Ok(KeyAction::mouse_button(16)),
            "wheelup" | "wheel-up" | "scrollup" | "scroll-up" => Ok(KeyAction::mouse_wheel(1)),
            "wheeldown" | "wheel-down" | "scrolldown" | "scroll-down" => Ok(KeyAction::mouse_wheel(-1)),
            other => {
                if let Ok(btn) = other.parse::<u8>() {
                    Ok(KeyAction::mouse_button(btn))
                } else {
                    Err(CoreError::UnknownAction(s.to_string()))
                }
            }
        };
    }

    // 3. Macro trigger actions: "macro:0:hold", "M0:hold", "M0", "macro:0"
    let macro_body = if let Some(rest) = lower.strip_prefix("macro:") {
        Some(rest)
    } else if lower.starts_with('m') && lower.len() >= 2 && lower.chars().nth(1).map(|c| c.is_ascii_digit()).unwrap_or(false) {
        Some(&lower[1..])
    } else {
        None
    };
    if let Some(rest) = macro_body {
        let parts: Vec<&str> = rest.split(':').collect();
        if let Ok(slot) = parts[0].parse::<u8>() {
            if slot <= 15 {
                let mut loop_mode = 0u8;
                let mut repeat = 1u8;
                if parts.len() > 1 {
                    match parts[1] {
                        "hold" | "untilreleased" | "until-released" => loop_mode = 1,
                        "anykey" | "untilanykey" | "until-any-key" => loop_mode = 2,
                        "toggle" | "untilpressedagain" | "until-pressed-again" => loop_mode = 3,
                        rep if rep.starts_with("repeat=") => {
                            repeat = rep.strip_prefix("repeat=").unwrap().parse().unwrap_or(1);
                            loop_mode = 0;
                        }
                        "once" | "repeat" => {
                            loop_mode = 0;
                            repeat = 1;
                        }
                        _ => {}
                    }
                }
                return Ok(KeyAction::macro_trigger(slot, loop_mode, repeat));
            }
        }
    }

    // 4. Multimedia & OS actions:
    match stripped.as_str() {
        "vol+" | "volume+" | "volumeup" | "volup" | "vol-up" => return Ok(KeyAction::multimedia(0x00E9)),
        "vol-" | "volume-" | "volumedown" | "voldown" | "vol-down" => return Ok(KeyAction::multimedia(0x00EA)),
        "mute" => return Ok(KeyAction::multimedia(0x00E2)),
        "play" | "pause" | "playpause" | "play-pause" => return Ok(KeyAction::multimedia(0x00CD)),
        "stop" => return Ok(KeyAction::multimedia(0x00B7)),
        "prev" | "prevtrack" | "prev-track" => return Ok(KeyAction::multimedia(0x00B6)),
        "next" | "nexttrack" | "next-track" => return Ok(KeyAction::multimedia(0x00B5)),
        "media" | "mediaplayer" => return Ok(KeyAction::multimedia(0x0183)),
        "calc" | "calculator" => return Ok(KeyAction::multimedia(0x0192)),
        "mypc" | "mycomputer" | "thispc" | "my-pc" => return Ok(KeyAction::multimedia(0x0194)),
        "mail" | "email" => return Ok(KeyAction::multimedia(0x018A)),
        "browser" | "homepage" | "webhome" | "web-home" => return Ok(KeyAction::multimedia(0x0223)),
        "search" | "websearch" | "web-search" => return Ok(KeyAction::multimedia(0x0221)),
        "webrefresh" | "refresh" | "web-refresh" => return Ok(KeyAction::multimedia(0x0227)),
        "webstop" | "web-stop" => return Ok(KeyAction::multimedia(0x0226)),
        "webforward" | "web-forward" => return Ok(KeyAction::multimedia(0x0225)),
        "webback" | "webbackward" | "web-back" | "web-backward" => return Ok(KeyAction::multimedia(0x0224)),
        "webfavorites" | "favorites" | "web-favorites" => return Ok(KeyAction::multimedia(0x022A)),
        "displaybrt+" | "screenbrt+" | "display-brt+" | "screen-brt+" => return Ok(KeyAction::multimedia(0x006F)),
        "displaybrt-" | "screenbrt-" | "display-brt-" | "screen-brt-" => return Ok(KeyAction::multimedia(0x0070)),
        "power" => return Ok(KeyAction::power(1)),
        "sleep" => return Ok(KeyAction::power(2)),
        "wake" | "wakeup" => return Ok(KeyAction::power(4)),
        _ => {}
    }

    // 5. Key combinations (separated by '+'):
    if lower.contains('+') {
        let parts: Vec<&str> = lower.split('+').collect();
        let mut modifiers = 0u8;
        let mut base_key = "";

        for part in parts {
            let p = part.trim();
            match p {
                "ctrl" | "lctrl" | "rctrl" => modifiers |= 0x01,
                "shift" | "lshift" | "rshift" => modifiers |= 0x02,
                "alt" | "lalt" | "ralt" => modifiers |= 0x04,
                "win" | "lwin" | "rwin" | "gui" | "super" => modifiers |= 0x08,
                key => base_key = key,
            }
        }

        if !base_key.is_empty() {
            if let Some(scancode) = scancode_for_key_name(base_key) {
                return Ok(KeyAction::combo(modifiers, scancode));
            }
        }
    }

    // 6. Direct numeric scancode (decimal or 0x hex):
    if let Ok(scancode) = lower.parse::<u8>() {
        return Ok(KeyAction::standard(scancode));
    }
    if let Some(hex_str) = lower.strip_prefix("0x") {
        if let Ok(scancode) = u8::from_str_radix(hex_str, 16) {
            return Ok(KeyAction::standard(scancode));
        }
    }

    // 7. Standard individual key scancode:
    if let Some(scancode) = scancode_for_key_name(&lower) {
        return Ok(KeyAction::standard(scancode));
    }

    Err(CoreError::UnknownAction(s.to_string()))
}

/// Formats a `KeyAction` into a concise, human-friendly display string.
pub fn format_action_str(action: &KeyAction) -> String {
    match action.category {
        ActionCategory::Disabled => "Disabled".to_string(),
        ActionCategory::StandardHidKey => {
            let base_name = key_name_for_scancode(action.code2).unwrap_or("Unknown");
            if action.code1 == 0 {
                base_name.to_string()
            } else {
                let mut mods = Vec::new();
                if (action.code1 & 0x01) != 0 || (action.code1 & 0x10) != 0 {
                    mods.push("Ctrl");
                }
                if (action.code1 & 0x08) != 0 || (action.code1 & 0x80) != 0 {
                    mods.push("Win");
                }
                if (action.code1 & 0x04) != 0 || (action.code1 & 0x40) != 0 {
                    mods.push("Alt");
                }
                if (action.code1 & 0x02) != 0 || (action.code1 & 0x20) != 0 {
                    mods.push("Shift");
                }
                mods.push(base_name);
                mods.join("+")
            }
        }
        ActionCategory::MultimediaOs => {
            let usage = (action.code1 as u16) | ((action.code2 as u16) << 8);
            media_name_for_usage(usage).unwrap_or("Multimedia").to_string()
        }
        ActionCategory::HardwareLighting => {
            match action.code1 {
                0x07 => "WinLock".to_string(),
                0 => "Led On/Off".to_string(),
                1 => "LedBrt+".to_string(),
                2 => "LedBrt-".to_string(),
                3 => "LedMode".to_string(),
                4 => "LedColor".to_string(),
                5 => "Speed+".to_string(),
                6 => "Speed-".to_string(),
                13 => "Mac Device".to_string(),
                14 => "Win Device".to_string(),
                16 => "Reset".to_string(),
                other => format!("Custom:{}", other),
            }
        }
        ActionCategory::MouseAction => {
            if action.code3 == 1 {
                "Mouse:WheelUp".to_string()
            } else if action.code3 == 255 {
                "Mouse:WheelDown".to_string()
            } else {
                match action.code1 {
                    1 => "Mouse:Left".to_string(),
                    2 => "Mouse:Right".to_string(),
                    4 => "Mouse:Middle".to_string(),
                    8 => "Mouse:Backward".to_string(),
                    16 => "Mouse:Forward".to_string(),
                    other => format!("Mouse:{}", other),
                }
            }
        }
        ActionCategory::CursorControl => format!("CursorMove({}, {})", action.code1 as i8, action.code2 as i8),
        ActionCategory::PowerManagement => {
            match action.code1 {
                1 => "Power".to_string(),
                2 => "Sleep".to_string(),
                4 => "Wakeup".to_string(),
                other => format!("Power:{}", other),
            }
        }
        ActionCategory::HardwareMacro => {
            let mode_str = match action.code3 {
                1 => ":hold",
                2 => ":anykey",
                3 => ":toggle",
                _ if action.code2 > 1 => ":repeat",
                _ => "",
            };
            format!("M{}{}", action.code1, mode_str)
        }
    }
}

/// Keymap configuration for a single layer (Base or Fn).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerKeymapConfig {
    /// Layer type.
    pub layer: KeyLayer,
    /// Explicit overrides for physical keys (key name or index -> action string).
    #[serde(default)]
    pub overrides: HashMap<String, String>,
}

impl LayerKeymapConfig {
    /// Creates a layer config with no overrides (all keys default).
    pub fn new_default(layer: KeyLayer) -> Self {
        Self {
            layer,
            overrides: HashMap::new(),
        }
    }

    /// Converts this layer configuration into the full hardware 126-slot descriptor table.
    pub fn to_descriptors(&self) -> Result<[KeyAction; SPARSE_KEY_COUNT], CoreError> {
        let mut descriptors = default_layer_keymap(self.layer);
        for (key_ident, action_str) in &self.overrides {
            let key_idx = parse_key_identifier(key_ident)?;
            let action = parse_action_str(action_str)?;
            if (key_idx as usize) < SPARSE_KEY_COUNT {
                descriptors[key_idx as usize] = action;
                if let Some(mirror) = get_mirror_key_index(key_idx) {
                    descriptors[mirror as usize] = action;
                }
            }
        }
        Ok(descriptors)
    }

    /// Creates a layer config by comparing descriptors against the layer's factory defaults.
    pub fn from_descriptors(descriptors: &[KeyAction; SPARSE_KEY_COUNT], layer: KeyLayer) -> Self {
        let defaults = default_layer_keymap(layer);
        let mut overrides = HashMap::new();
        for key in KEY_DEFINITIONS {
            let actual = descriptors[key.index as usize];
            let expected = defaults[key.index as usize];
            if actual != expected {
                overrides.insert(key.name.to_string(), format_action_str(&actual));
            }
        }
        Self { layer, overrides }
    }

    /// Serializes this layer configuration to JSON.
    pub fn to_json(&self) -> Result<String, CoreError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Deserializes a layer configuration from JSON.
    pub fn from_json(json: &str) -> Result<Self, CoreError> {
        Ok(serde_json::from_str(json)?)
    }

    /// Serializes this layer configuration to TOML.
    pub fn to_toml(&self) -> Result<String, CoreError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Deserializes a layer configuration from TOML.
    pub fn from_toml(toml_str: &str) -> Result<Self, CoreError> {
        Ok(toml::from_str(toml_str)?)
    }
}

/// Keymap configuration for an onboard profile (containing Base and Fn layers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileKeymapConfig {
    /// Profile slot index (0..2).
    pub profile_index: u8,
    /// Human-friendly profile name.
    pub name: String,
    /// Base typing layer configuration.
    pub base_layer: LayerKeymapConfig,
    /// Fn modifier layer configuration.
    pub fn_layer: LayerKeymapConfig,
}

impl Default for ProfileKeymapConfig {
    fn default() -> Self {
        Self {
            profile_index: 0,
            name: "Default Profile".to_string(),
            base_layer: LayerKeymapConfig::new_default(KeyLayer::Base),
            fn_layer: LayerKeymapConfig::new_default(KeyLayer::Fn),
        }
    }
}

impl ProfileKeymapConfig {
    /// Serializes to formatted JSON.
    pub fn to_json(&self) -> Result<String, CoreError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Deserializes from JSON.
    pub fn from_json(json: &str) -> Result<Self, CoreError> {
        Ok(serde_json::from_str(json)?)
    }

    /// Serializes to TOML.
    pub fn to_toml(&self) -> Result<String, CoreError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Deserializes from TOML.
    pub fn from_toml(toml_str: &str) -> Result<Self, CoreError> {
        Ok(toml::from_str(toml_str)?)
    }

    /// Saves this profile configuration to a JSON or TOML file on disk.
    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let p = path.as_ref();
        let content = if p.extension().is_some_and(|ext| ext == "toml") {
            self.to_toml()?
        } else {
            self.to_json()?
        };
        std::fs::write(p, content)?;
        Ok(())
    }

    /// Loads a profile configuration from a JSON or TOML file on disk.
    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let p = path.as_ref();
        let content = std::fs::read_to_string(p)?;
        if p.extension().is_some_and(|ext| ext == "toml") {
            Ok(Self::from_toml(&content)?)
        } else {
            Ok(Self::from_json(&content)?)
        }
    }
}

/// A human-readable color theme for per-key matrix canvas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Name of the theme.
    pub name: String,
    /// Default background color for all keys.
    pub default_color: [u8; 3],
    /// Specific key overrides by physical index (0..103) or name.
    #[serde(default)]
    pub key_overrides: HashMap<usize, [u8; 3]>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "Default Spectrum".to_string(),
            default_color: [255, 0, 0],
            key_overrides: HashMap::new(),
        }
    }
}

impl ThemeConfig {
    /// Converts this theme into a full sparse RGB color matrix (126 keys).
    pub fn to_matrix(&self) -> [RgbColor; SPARSE_KEY_COUNT] {
        let default_rgb = RgbColor::new(self.default_color[0], self.default_color[1], self.default_color[2]);
        let mut matrix = [default_rgb; SPARSE_KEY_COUNT];
        for (&idx, &rgb) in &self.key_overrides {
            if idx < SPARSE_KEY_COUNT {
                matrix[idx] = RgbColor::new(rgb[0], rgb[1], rgb[2]);
            }
        }
        matrix
    }

    /// Creates a Cyberpunk neon preset theme.
    pub fn cyberpunk() -> Self {
        let mut overrides = HashMap::new();
        // WASD keys (W=44, A=64, S=65, D=66) set to bright electric yellow
        for &k in &[44, 64, 65, 66] {
            overrides.insert(k, [255, 255, 0]);
        }
        // Arrow cluster (Up=99, Left=119, Down=120, Right=121) set to neon cyan
        for &k in &[99, 119, 120, 121] {
            overrides.insert(k, [0, 255, 255]);
        }
        // Spacebar (110 and 111) set to bright cyan
        overrides.insert(110, [0, 255, 255]);
        overrides.insert(111, [0, 255, 255]);
        // Esc (0) and Enter (75 and 76) set to neon green
        overrides.insert(0, [0, 255, 0]);
        overrides.insert(75, [0, 255, 0]);
        overrides.insert(76, [0, 255, 0]);

        Self {
            name: "Cyberpunk Neon".to_string(),
            default_color: [240, 0, 255], // Rich neon violet/magenta background with full blue strength
            key_overrides: overrides,
        }
    }

    /// High-contrast test theme: Deep Blue background, Solid Red WASD, Vibrant Green Space & Enter, Yellow Arrows.
    pub fn gaming() -> Self {
        let mut overrides = HashMap::new();
        // WASD keys set to solid red
        for &k in &[44, 64, 65, 66] {
            overrides.insert(k, [255, 0, 0]);
        }
        // Arrow cluster set to bright yellow
        for &k in &[99, 119, 120, 121] {
            overrides.insert(k, [255, 255, 0]);
        }
        // Spacebar (110 and 111) and Enter (75 and 76) set to vibrant green
        overrides.insert(110, [0, 255, 0]);
        overrides.insert(111, [0, 255, 0]);
        overrides.insert(75, [0, 255, 0]);
        overrides.insert(76, [0, 255, 0]);
        // Esc (0) set to bright white
        overrides.insert(0, [255, 255, 255]);

        Self {
            name: "Gaming High-Contrast".to_string(),
            default_color: [0, 60, 255], // Vivid Royal Blue background
            key_overrides: overrides,
        }
    }

    /// Serializes this theme config to a formatted JSON string.
    pub fn to_json(&self) -> Result<String, CoreError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parses a theme config from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, CoreError> {
        Ok(serde_json::from_str(json)?)
    }

    /// Saves this theme to a JSON file on disk.
    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let json = self.to_json()?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Loads a theme from a JSON file on disk.
    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self::from_json(&content)?)
    }
}

/// A human-readable macro action item for authoring macros in TOML/JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroItem {
    /// Action type: "down", "up", "click", "mouse_down", "mouse_up", "wheel_v", "wheel_h".
    pub action: String,
    /// Target USB HID scancode (for backward compatibility).
    #[serde(default)]
    pub scancode: u8,
    /// Target code (scancode or mouse button code).
    #[serde(default)]
    pub code: u8,
    /// Optional human-readable button or key name.
    #[serde(default)]
    pub name: Option<String>,
    /// Pre-action delay in milliseconds.
    pub delay_ms: u16,
}

impl MacroItem {
    pub fn get_code(&self) -> u8 {
        if self.code != 0 {
            self.code
        } else if self.scancode != 0 {
            self.scancode
        } else if let Some(ref name) = self.name {
            scancode_for_key_name(name).unwrap_or(0)
        } else {
            0
        }
    }
}

/// A human-readable macro sequence definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroSequence {
    /// Name of the macro.
    pub name: String,
    /// Target hardware macro slot (0..31).
    pub slot: usize,
    /// Optional loop execution mode (0=repeat, 1=until released, 2=until any key, 3=toggle).
    #[serde(default)]
    pub loop_mode: u8,
    /// Number of repeat cycles (default: 1).
    #[serde(default = "default_macro_repeat")]
    pub repeat: u8,
    /// Sequence of actions.
    pub steps: Vec<MacroItem>,
}

fn default_macro_repeat() -> u8 {
    1
}

impl MacroSequence {
    /// Compiles this sequence and inserts it into a 4KB MacroMemoryTable starting at the specified offset.
    pub fn compile_into_table(
        &self,
        table: &mut MacroMemoryTable,
        start_offset: u16,
    ) -> Result<u16, CoreError> {
        if self.steps.is_empty() {
            return Err(CoreError::MacroCompilationFailed("Macro steps list cannot be empty"));
        }
        if self.slot >= MACRO_SLOT_COUNT {
            return Err(CoreError::MacroCompilationFailed("Macro slot must be within 0..31"));
        }

        let mut records = Vec::new();

        for (i, step) in self.steps.iter().enumerate() {
            let is_last = i == self.steps.len() - 1;
            let code = step.get_code();
            match step.action.as_str() {
                "down" | "press" => {
                    records.push(MacroActionRecord {
                        delay_ms: step.delay_ms,
                        is_press: true,
                        is_end_of_macro: is_last,
                        event_type: MacroEventType::KeyboardKey,
                        code,
                    });
                }
                "up" | "release" => {
                    records.push(MacroActionRecord {
                        delay_ms: step.delay_ms,
                        is_press: false,
                        is_end_of_macro: is_last,
                        event_type: MacroEventType::KeyboardKey,
                        code,
                    });
                }
                "click" => {
                    records.push(MacroActionRecord {
                        delay_ms: step.delay_ms,
                        is_press: true,
                        is_end_of_macro: false,
                        event_type: MacroEventType::KeyboardKey,
                        code,
                    });
                    records.push(MacroActionRecord {
                        delay_ms: 25,
                        is_press: false,
                        is_end_of_macro: is_last,
                        event_type: MacroEventType::KeyboardKey,
                        code,
                    });
                }
                "mouse_down" => {
                    records.push(MacroActionRecord {
                        delay_ms: step.delay_ms,
                        is_press: true,
                        is_end_of_macro: is_last,
                        event_type: MacroEventType::MouseButton,
                        code,
                    });
                }
                "mouse_up" => {
                    records.push(MacroActionRecord {
                        delay_ms: step.delay_ms,
                        is_press: false,
                        is_end_of_macro: is_last,
                        event_type: MacroEventType::MouseButton,
                        code,
                    });
                }
                "wheel_v" => {
                    records.push(MacroActionRecord {
                        delay_ms: step.delay_ms,
                        is_press: false,
                        is_end_of_macro: is_last,
                        event_type: MacroEventType::MouseWheelVertical,
                        code,
                    });
                }
                "wheel_h" => {
                    records.push(MacroActionRecord {
                        delay_ms: step.delay_ms,
                        is_press: false,
                        is_end_of_macro: is_last,
                        event_type: MacroEventType::MouseWheelHorizontal,
                        code,
                    });
                }
                _ => return Err(CoreError::MacroCompilationFailed("Invalid action type (expected 'down', 'up', 'click', 'mouse_down', etc.)")),
            }
        }

        let next_offset = table
            .compile_slot(self.slot, &records, start_offset)
            .map_err(|_| CoreError::MacroCompilationFailed("4KB macro memory capacity exceeded"))?;

        Ok(next_offset)
    }

    /// Constructs a `MacroSequence` from decoded hardware `MacroActionRecord`s.
    pub fn from_records(name: &str, slot: usize, records: &[MacroActionRecord]) -> Self {
        let mut steps = Vec::with_capacity(records.len());
        for r in records {
            let action_str = match r.event_type {
                MacroEventType::KeyboardKey => {
                    if r.is_press { "down" } else { "up" }
                }
                MacroEventType::MouseButton => {
                    if r.is_press { "mouse_down" } else { "mouse_up" }
                }
                MacroEventType::MouseWheelVertical => "wheel_v",
                MacroEventType::MouseWheelHorizontal => "wheel_h",
            };
            let key_name = if r.event_type == MacroEventType::KeyboardKey {
                key_name_for_scancode(r.code).map(|s| s.to_string())
            } else {
                None
            };
            steps.push(MacroItem {
                action: action_str.to_string(),
                scancode: r.code,
                code: r.code,
                name: key_name,
                delay_ms: r.delay_ms,
            });
        }
        Self {
            name: name.to_string(),
            slot,
            loop_mode: 0,
            repeat: 1,
            steps,
        }
    }

    /// Serializes to TOML.
    pub fn to_toml(&self) -> Result<String, CoreError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Deserializes from TOML.
    pub fn from_toml(toml_str: &str) -> Result<Self, CoreError> {
        Ok(toml::from_str(toml_str)?)
    }

    /// Serializes to JSON.
    pub fn to_json(&self) -> Result<String, CoreError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Deserializes from JSON.
    pub fn from_json(json: &str) -> Result<Self, CoreError> {
        Ok(serde_json::from_str(json)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_to_matrix_conversion() {
        let theme = ThemeConfig::cyberpunk();
        let matrix = theme.to_matrix();
        assert_eq!(matrix.len(), SPARSE_KEY_COUNT);
        assert_eq!(matrix[0], RgbColor::new(0, 255, 0)); // Esc neon green
        assert_eq!(matrix[44], RgbColor::new(255, 255, 0)); // W electric yellow
        assert_eq!(matrix[64], RgbColor::new(255, 255, 0)); // A electric yellow
        assert_eq!(matrix[65], RgbColor::new(255, 255, 0)); // S electric yellow
        assert_eq!(matrix[66], RgbColor::new(255, 255, 0)); // D electric yellow
        assert_eq!(matrix[111], RgbColor::new(0, 255, 255)); // Spacebar cyan
    }

    #[test]
    fn test_macro_compilation_into_4kb_table() {
        let mut table = MacroMemoryTable::new_empty();
        let seq = MacroSequence {
            name: "Quick A".to_string(),
            slot: 0,
            loop_mode: 0,
            repeat: 1,
            steps: vec![MacroItem {
                action: "click".to_string(),
                scancode: 0x04,
                code: 0x04,
                name: Some("A".to_string()),
                delay_ms: 50,
            }],
        };

        let next_offset = seq.compile_into_table(&mut table, 64).expect("compile macro");
        assert_eq!(next_offset, 64 + 8); // 2 records (down, up) = 8 bytes
        assert_eq!(table.get_slot_pointer(0).unwrap(), 64);

        // Decompile back and verify roundtrip
        let decompiled = table.decompile_slot(0).expect("decompile macro");
        assert_eq!(decompiled.len(), 2);
        assert_eq!(decompiled[0].code, 0x04);
        assert!(decompiled[0].is_press);
        assert_eq!(decompiled[1].code, 0x04);
        assert!(!decompiled[1].is_press);
        assert!(decompiled[1].is_end_of_macro);
    }

    #[test]
    fn test_theme_json_roundtrip() {
        let theme = ThemeConfig::cyberpunk();
        let json = theme.to_json().expect("serialize theme");
        let parsed = ThemeConfig::from_json(&json).expect("deserialize theme");
        assert_eq!(parsed.name, theme.name);
        assert_eq!(parsed.default_color, theme.default_color);
        assert_eq!(parsed.key_overrides.len(), theme.key_overrides.len());
    }

    #[test]
    fn test_action_string_parser_and_formatter() {
        // Standard key
        let a = parse_action_str("w").expect("parse w");
        assert_eq!(a, KeyAction::standard(26));
        assert_eq!(format_action_str(&a), "W");

        // Combos
        let ctrl_c = parse_action_str("ctrl+c").expect("parse ctrl+c");
        assert_eq!(ctrl_c, KeyAction::combo(0x01, 6));
        assert_eq!(format_action_str(&ctrl_c), "Ctrl+C");

        let win_shift_s = parse_action_str("win+shift+s").expect("parse win+shift+s");
        assert_eq!(win_shift_s, KeyAction::combo(0x08 | 0x02, 22));
        assert_eq!(format_action_str(&win_shift_s), "Win+Shift+S");

        // Multimedia
        let vol_up = parse_action_str("vol+").expect("parse vol+");
        assert_eq!(vol_up, KeyAction::multimedia(0x00E9));
        assert_eq!(format_action_str(&vol_up), "Volume +");

        // Custom WinLock
        let wl = parse_action_str("custom:win-lock").expect("parse win-lock");
        assert_eq!(wl, KeyAction::win_lock());
        assert_eq!(format_action_str(&wl), "WinLock");

        // Mouse
        let m_left = parse_action_str("mouse:left").expect("parse mouse left");
        assert_eq!(m_left, KeyAction::mouse_button(1));
        assert_eq!(format_action_str(&m_left), "Mouse:Left");

        // Macro
        let m0 = parse_action_str("macro:0:hold").expect("parse macro 0 hold");
        assert_eq!(m0, KeyAction::macro_trigger(0, 1, 1));
        assert_eq!(format_action_str(&m0), "M0:hold");

        // Parse format_action_str output directly (Roundtrip fidelity)
        assert_eq!(parse_action_str("Volume +").unwrap(), KeyAction::multimedia(0x00E9));
        assert_eq!(parse_action_str("Volume -").unwrap(), KeyAction::multimedia(0x00EA));
        assert_eq!(parse_action_str("Play/Pause").unwrap(), KeyAction::multimedia(0x00CD));
        assert_eq!(parse_action_str("Prev Track").unwrap(), KeyAction::multimedia(0x00B6));
        assert_eq!(parse_action_str("Next Track").unwrap(), KeyAction::multimedia(0x00B5));
        assert_eq!(parse_action_str("My PC").unwrap(), KeyAction::multimedia(0x0194));
        assert_eq!(parse_action_str("Led On/Off").unwrap(), KeyAction::hardware_custom(0));
        assert_eq!(parse_action_str("LedBrt+").unwrap(), KeyAction::hardware_custom(1));
        assert_eq!(parse_action_str("LedBrt-").unwrap(), KeyAction::hardware_custom(2));
        assert_eq!(parse_action_str("LedMode").unwrap(), KeyAction::hardware_custom(3));
        assert_eq!(parse_action_str("LedColor").unwrap(), KeyAction::hardware_custom(4));
        assert_eq!(parse_action_str("Speed+").unwrap(), KeyAction::hardware_custom(5));
        assert_eq!(parse_action_str("Speed-").unwrap(), KeyAction::hardware_custom(6));
        assert_eq!(parse_action_str("Reset").unwrap(), KeyAction::hardware_custom(16));
        assert_eq!(parse_action_str("WinLock").unwrap(), KeyAction::win_lock());
        assert_eq!(parse_action_str("M0:hold").unwrap(), KeyAction::macro_trigger(0, 1, 1));
        assert_eq!(parse_action_str("M1").unwrap(), KeyAction::macro_trigger(1, 0, 1));
    }

    #[test]
    fn test_profile_keymap_config_roundtrip() {
        let mut profile = ProfileKeymapConfig::default();
        profile.base_layer.overrides.insert("W".to_string(), "ctrl+c".to_string());
        profile.base_layer.overrides.insert("CapsLock".to_string(), "custom:win-lock".to_string());
        profile.fn_layer.overrides.insert("F1".to_string(), "vol+".to_string());

        let json = profile.to_json().expect("serialize profile");
        let parsed = ProfileKeymapConfig::from_json(&json).expect("deserialize profile");
        assert_eq!(parsed.base_layer.overrides.get("W").unwrap(), "ctrl+c");
        assert_eq!(parsed.fn_layer.overrides.get("F1").unwrap(), "vol+");

        let descriptors = profile.base_layer.to_descriptors().expect("to descriptors");
        assert_eq!(descriptors[44], KeyAction::combo(0x01, 6)); // W (44) is Ctrl+C
        assert_eq!(descriptors[63], KeyAction::win_lock());     // CapsLock (63) is WinLock
        assert_eq!(descriptors[0], KeyAction::standard(41));    // Esc (0) remains default Esc
    }

    #[test]
    fn test_verify_physical_key_definitions_and_scancodes() {
        assert_eq!(KEY_DEFINITIONS.len(), 104);

        // Row 4 (ZXCV row) matrix slots & scancodes
        let z = KEY_DEFINITIONS.iter().find(|k| k.name == "Z").unwrap();
        assert_eq!(z.index, 86);
        assert_eq!(default_scancode_for_key("Z").unwrap(), 29);

        let x = KEY_DEFINITIONS.iter().find(|k| k.name == "X").unwrap();
        assert_eq!(x.index, 87);
        assert_eq!(default_scancode_for_key("X").unwrap(), 27);

        let c = KEY_DEFINITIONS.iter().find(|k| k.name == "C").unwrap();
        assert_eq!(c.index, 88);
        assert_eq!(default_scancode_for_key("C").unwrap(), 6);

        let v = KEY_DEFINITIONS.iter().find(|k| k.name == "V").unwrap();
        assert_eq!(v.index, 89);
        assert_eq!(default_scancode_for_key("V").unwrap(), 25);

        let b = KEY_DEFINITIONS.iter().find(|k| k.name == "B").unwrap();
        assert_eq!(b.index, 90);
        assert_eq!(default_scancode_for_key("B").unwrap(), 5);

        let n = KEY_DEFINITIONS.iter().find(|k| k.name == "N").unwrap();
        assert_eq!(n.index, 91);
        assert_eq!(default_scancode_for_key("N").unwrap(), 17);

        let m = KEY_DEFINITIONS.iter().find(|k| k.name == "M").unwrap();
        assert_eq!(m.index, 92);
        assert_eq!(default_scancode_for_key("M").unwrap(), 16);

        let comma = KEY_DEFINITIONS.iter().find(|k| k.name == "<").unwrap();
        assert_eq!(comma.index, 93);
        assert_eq!(default_scancode_for_key("<").unwrap(), 54);

        let period = KEY_DEFINITIONS.iter().find(|k| k.name == ">").unwrap();
        assert_eq!(period.index, 94);
        assert_eq!(default_scancode_for_key(">").unwrap(), 55);

        let slash = KEY_DEFINITIONS.iter().find(|k| k.name == "/").unwrap();
        assert_eq!(slash.index, 95);
        assert_eq!(default_scancode_for_key("/").unwrap(), 56);

        let rshift = KEY_DEFINITIONS.iter().find(|k| k.name == "RightShift").unwrap();
        assert_eq!(rshift.index, 97);
        assert_eq!(default_scancode_for_key("RightShift").unwrap(), 229);

        // Function row
        let f1 = KEY_DEFINITIONS.iter().find(|k| k.name == "F1").unwrap();
        assert_eq!(f1.index, 1);
        assert_eq!(default_scancode_for_key("F1").unwrap(), 58);

        let f12 = KEY_DEFINITIONS.iter().find(|k| k.name == "F12").unwrap();
        assert_eq!(f12.index, 12);
        assert_eq!(default_scancode_for_key("F12").unwrap(), 69);

        // Bottom row
        let space = KEY_DEFINITIONS.iter().find(|k| k.name == "Space").unwrap();
        assert_eq!(space.index, 111);
        assert_eq!(get_mirror_key_index(111), Some(110));

        let num0 = KEY_DEFINITIONS.iter().find(|k| k.name == "Num 0").unwrap();
        assert_eq!(num0.index, 123);
        assert_eq!(get_mirror_key_index(123), Some(122));

        // Shift stabilizer pads
        assert_eq!(get_mirror_key_index(84), Some(85));
        assert_eq!(get_mirror_key_index(85), Some(84));
        assert_eq!(get_mirror_key_index(97), Some(96));
        assert_eq!(get_mirror_key_index(96), Some(97));

        // Default base layer keymap validation
        let base = default_layer_keymap(KeyLayer::Base);
        assert_eq!(base[86].code2, 29); // Z
        assert_eq!(base[87].code2, 27); // X
        assert_eq!(base[88].code2, 6);  // C
        assert_eq!(base[89].code2, 25); // V
        assert_eq!(base[90].code2, 5);  // B
        assert_eq!(base[91].code2, 17); // N
        assert_eq!(base[92].code2, 16); // M
        assert_eq!(base[93].code2, 54); // <
        assert_eq!(base[94].code2, 55); // >
        assert_eq!(base[95].code2, 56); // /
        assert_eq!(base[97].code2, 229);// RightShift
        assert_eq!(base[1].code2, 58);  // F1
        assert_eq!(base[12].code2, 69); // F12
        assert_eq!(base[111].code2, 44);// Space
        assert_eq!(base[110].code2, 44);// Space mirror
        assert_eq!(base[123].code2, 98);// Num 0
        assert_eq!(base[122].code2, 98);// Num 0 mirror
    }
}
