//! High-level abstractions for themes, keymap naming, and the 4KB macro compiler.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

use fantech_protocol::constants::SPARSE_KEY_COUNT;
use fantech_protocol::lighting::RgbColor;
use fantech_protocol::macro_mem::{MacroActionRecord, MacroEventType, MacroMemoryTable};
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

    /// Macro exceeds hardware limits.
    #[error("Macro compilation failed: {0}")]
    MacroCompilationFailed(&'static str),

    /// Unknown key name.
    #[error("Unknown key identifier: {0}")]
    UnknownKeyName(String),
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
    KeyEntry { name: "PrtSc", index: 13, row: 0, col: 14 },
    KeyEntry { name: "ScrLK", index: 14, row: 0, col: 15 },
    KeyEntry { name: "Pause", index: 15, row: 0, col: 16 },

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
    KeyEntry { name: "Enter", index: 76, row: 3, col: 13 },
    KeyEntry { name: "Num 4", index: 80, row: 3, col: 17 },
    KeyEntry { name: "Num 5", index: 81, row: 3, col: 18 },
    KeyEntry { name: "Num 6", index: 82, row: 3, col: 19 },

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
        "numpad" | "num" => Some(KEY_DEFINITIONS.iter().filter(|k| [38, 39, 40, 41, 59, 60, 61, 62, 80, 81, 82, 101, 102, 103, 104, 122, 124].contains(&k.index)).collect()),
        "nav" | "navigation" => Some(KEY_DEFINITIONS.iter().filter(|k| [35, 36, 37, 56, 57, 58].contains(&k.index)).collect()),
        "fkeys" | "fn" | "function" => Some(KEY_DEFINITIONS.iter().filter(|k| (1..=12).contains(&k.index) || k.index == 0).collect()),
        "media" => Some(KEY_DEFINITIONS.iter().filter(|k| (17..=20).contains(&k.index)).collect()),
        "bottom" => Some(KEY_DEFINITIONS.iter().filter(|k| k.row == 5).collect()),
        _ => None,
    }
}

/// Resolves alternate electrical pad index for edge switches with dual solder positions.
pub fn get_mirror_key_index(key_index: u8) -> Option<u8> {
    match key_index {
        96 => Some(95),
        95 => Some(96),
        122 => Some(123),
        123 => Some(122),
        111 => Some(110),
        110 => Some(111),
        76 => Some(75),
        75 => Some(76),
        84 => Some(85),
        85 => Some(84),
        _ => None,
    }
}

/// A human-readable color theme for per-key matrix canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// A human-readable macro action item for authoring macros in TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroItem {
    /// Action type: "down", "up", or "click".
    pub action: String,
    /// Target USB HID scancode (e.g. 0x04 for 'A').
    pub scancode: u8,
    /// Pre-action delay in milliseconds.
    pub delay_ms: u16,
}

/// A human-readable macro sequence definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroSequence {
    /// Name of the macro.
    pub name: String,
    /// Target hardware macro slot (0..31).
    pub slot: usize,
    /// Sequence of actions.
    pub steps: Vec<MacroItem>,
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
        if self.slot >= 32 {
            return Err(CoreError::MacroCompilationFailed("Macro slot must be within 0..31"));
        }

        let mut offset = start_offset as usize;
        let mut records = Vec::new();

        for (i, step) in self.steps.iter().enumerate() {
            let is_last = i == self.steps.len() - 1;
            match step.action.as_str() {
                "down" => {
                    records.push(MacroActionRecord {
                        delay_ms: step.delay_ms,
                        is_press: true,
                        is_end_of_macro: is_last,
                        event_type: MacroEventType::KeyboardKey,
                        code: step.scancode,
                    });
                }
                "up" => {
                    records.push(MacroActionRecord {
                        delay_ms: step.delay_ms,
                        is_press: false,
                        is_end_of_macro: is_last,
                        event_type: MacroEventType::KeyboardKey,
                        code: step.scancode,
                    });
                }
                "click" => {
                    records.push(MacroActionRecord {
                        delay_ms: step.delay_ms,
                        is_press: true,
                        is_end_of_macro: false,
                        event_type: MacroEventType::KeyboardKey,
                        code: step.scancode,
                    });
                    records.push(MacroActionRecord {
                        delay_ms: 25,
                        is_press: false,
                        is_end_of_macro: is_last,
                        event_type: MacroEventType::KeyboardKey,
                        code: step.scancode,
                    });
                }
                _ => return Err(CoreError::MacroCompilationFailed("Invalid action type (expected 'down', 'up', or 'click')")),
            }
        }

        // Write records into table
        for r in &records {
            let bytes = r.to_bytes();
            table
                .write_bytes(offset, &bytes)
                .map_err(|_| CoreError::MacroCompilationFailed("4KB macro memory capacity exceeded"))?;
            offset += 4;
        }

        // Update slot pointer
        table
            .set_slot_pointer(self.slot, start_offset)
            .map_err(|_| CoreError::MacroCompilationFailed("Failed setting macro slot pointer"))?;

        Ok(offset as u16)
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
            steps: vec![MacroItem {
                action: "click".to_string(),
                scancode: 0x04,
                delay_ms: 50,
            }],
        };

        let next_offset = seq.compile_into_table(&mut table, 64).expect("compile macro");
        assert_eq!(next_offset, 64 + 8); // 2 records (down, up) = 8 bytes
        assert_eq!(table.get_slot_pointer(0).unwrap(), 64);
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
}
