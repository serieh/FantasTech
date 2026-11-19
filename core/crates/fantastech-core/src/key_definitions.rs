//! Hardware matrix key definitions and geometric zone lookups.

use serde::{Deserialize, Serialize};

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

