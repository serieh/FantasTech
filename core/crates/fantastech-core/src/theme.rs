//! RGB theme configurations, color palettes, and matrix conversion.

use crate::error::CoreError;
use fantastech_protocol::constants::SPARSE_KEY_COUNT;
use fantastech_protocol::lighting::RgbColor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

