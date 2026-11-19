//! Keymap layer configuration models and JSON/TOML serialization.

use crate::actions::{default_layer_keymap, format_action_str, parse_action_str, parse_key_identifier};
use crate::key_definitions::{get_mirror_key_index, KEY_DEFINITIONS};
use crate::error::CoreError;
use fantastech_protocol::constants::SPARSE_KEY_COUNT;
use fantastech_protocol::keymap::{KeyAction, KeyLayer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

