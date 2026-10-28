//! Key remapping descriptors, layers, and wire framing.

use crate::constants::{opcodes, MAX_KEY_INDEX};
use crate::error::ProtocolError;
use crate::report::ReportBuffer;

/// Target keyboard functional layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyLayer {
    /// Normal primary typing layer.
    Base = 0x00,
    /// Function (Fn) key modifier layer.
    Fn = 0x01,
}

/// Action category identifiers supported by the MK894 onboard firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ActionCategory {
    MouseAction = 16,
    CursorControl = 17,
    HardwareLighting = 31,
    StandardHidKey = 32,
    MultimediaOs = 48,
    PowerManagement = 64,
    HardwareMacro = 96,
}

/// A 4-byte key remap definition entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyAction {
    pub category: ActionCategory,
    pub code1: u8,
    pub code2: u8,
    pub code3: u8,
}

impl KeyAction {
    /// Creates a standard USB HID keyboard key action.
    pub const fn standard_key(hid_usage_id: u8) -> Self {
        Self {
            category: ActionCategory::StandardHidKey,
            code1: hid_usage_id,
            code2: 0,
            code3: 0,
        }
    }

    /// Creates a multimedia consumer control action.
    pub const fn multimedia(consumer_usage_id: u16) -> Self {
        Self {
            category: ActionCategory::MultimediaOs,
            code1: (consumer_usage_id & 0xFF) as u8,
            code2: ((consumer_usage_id >> 8) & 0xFF) as u8,
            code3: 0,
        }
    }

    /// Creates a hardware macro trigger action targeting slot index 0..15.
    pub const fn macro_trigger(slot_index: u8) -> Self {
        Self {
            category: ActionCategory::HardwareMacro,
            code1: slot_index,
            code2: 0,
            code3: 0,
        }
    }

    /// Creates a WinLock toggle action.
    pub const fn win_lock() -> Self {
        Self {
            category: ActionCategory::HardwareLighting,
            code1: 0x0A,
            code2: 0,
            code3: 0,
        }
    }

    /// Encodes this action into a 4-byte slice.
    pub fn to_bytes(&self) -> [u8; 4] {
        [self.category as u8, self.code1, self.code2, self.code3]
    }

    /// Decodes an action from a 4-byte slice.
    pub fn from_bytes(bytes: [u8; 4]) -> Result<Self, ProtocolError> {
        let cat = match bytes[0] {
            16 => ActionCategory::MouseAction,
            17 => ActionCategory::CursorControl,
            31 => ActionCategory::HardwareLighting,
            32 => ActionCategory::StandardHidKey,
            48 => ActionCategory::MultimediaOs,
            64 => ActionCategory::PowerManagement,
            96 => ActionCategory::HardwareMacro,
            other => {
                return Err(ProtocolError::ParameterOutOfRange {
                    field: "action_category",
                    value: other as u32,
                    min: 16,
                    max: 96,
                })
            }
        };
        Ok(Self {
            category: cat,
            code1: bytes[1],
            code2: bytes[2],
            code3: bytes[3],
        })
    }
}

/// Builds a 64-byte USB Output Report to remap a single key on the keyboard (Opcode 0x10).
pub fn build_single_remap_report(
    key_index: u8,
    layer: KeyLayer,
    action: &KeyAction,
) -> Result<ReportBuffer, ProtocolError> {
    if (key_index as usize) > MAX_KEY_INDEX {
        return Err(ProtocolError::KeyIndexOutOfBounds(key_index as usize));
    }

    let offset = (key_index as u16) * 4;
    let mut report = ReportBuffer::new(opcodes::SET_SINGLE_KEYMAP);
    report.set_byte(2, 0x07)?; // payload descriptor length (7)
    report.set_byte(3, (offset & 0xFF) as u8)?;
    report.set_byte(4, ((offset >> 8) & 0xFF) as u8)?;
    report.set_byte(5, 0x00)?; // reserved
    report.set_byte(6, layer as u8)?;
    report.set_byte(7, 0x00)?; // reserved
    report.set_byte(8, action.category as u8)?;
    report.set_byte(9, action.code1)?;
    report.set_byte(10, action.code2)?;
    report.set_byte(11, action.code3)?;
    Ok(report)
}

/// Builds a 64-byte USB Output Report requesting a keymap table chunk readback (Opcode 0x08).
pub fn build_keymap_read_request(chunk_index: u8, layer: KeyLayer) -> Result<ReportBuffer, ProtocolError> {
    let offset = (chunk_index as u16) * 56;
    let mut report = ReportBuffer::new(opcodes::GET_KEYMAP_CHUNK);
    report.set_byte(2, 58)?; // header length descriptor
    report.set_byte(3, (offset & 0xFF) as u8)?;
    report.set_byte(4, ((offset >> 8) & 0xFF) as u8)?;
    report.set_byte(5, 0x00)?;
    report.set_byte(6, layer as u8)?;
    Ok(report)
}
