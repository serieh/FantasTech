//! Macro sequence definitions, JSON serialization, and 4KB EEPROM table compilation.

use crate::error::CoreError;
use crate::scancodes::{key_name_for_scancode, scancode_for_key_name};
use fantastech_protocol::constants::MACRO_SLOT_COUNT;
use fantastech_protocol::macro_mem::{MacroActionRecord, MacroEventType, MacroMemoryTable};
use serde::{Deserialize, Serialize};

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

