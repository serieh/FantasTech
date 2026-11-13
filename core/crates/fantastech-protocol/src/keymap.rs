//! Key remapping descriptors, layers, and wire framing.

use crate::constants::{opcodes, MAX_KEY_INDEX, SPARSE_KEY_COUNT};
use crate::error::ProtocolError;
use crate::report::ReportBuffer;

/// Target keyboard functional layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum KeyLayer {
    /// Normal primary typing layer.
    Base = 0x00,
    /// Function (Fn) key modifier layer.
    Fn = 0x01,
}

impl KeyLayer {
    /// Parses layer from byte (0 => Base, 1 => Fn).
    pub fn from_u8(val: u8) -> Result<Self, ProtocolError> {
        match val {
            0 => Ok(Self::Base),
            1 => Ok(Self::Fn),
            other => Err(ProtocolError::ParameterOutOfRange {
                field: "key_layer",
                value: other as u32,
                min: 0,
                max: 1,
            }),
        }
    }
}

/// Action category identifiers supported by the MK894 onboard firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum ActionCategory {
    Disabled = 0,
    MouseAction = 16,
    CursorControl = 17,
    HardwareLighting = 31,
    StandardHidKey = 32,
    MultimediaOs = 48,
    PowerManagement = 64,
    HardwareMacro = 96,
}

/// A 4-byte key remap definition entry.
///
/// Wire layout:
/// - Byte 0 (Report Byte 8): `category` (ActionCategory)
/// - Byte 1 (Report Byte 9): `code1` (Modifier mask for standard key; Action ID for custom; Button mask for mouse; Slot for macro)
/// - Byte 2 (Report Byte 10): `code2` (USB HID scancode; High byte for consumer usage; Repeat count for macro)
/// - Byte 3 (Report Byte 11): `code3` (Loop execution mode for macro; Wheel delta for mouse)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct KeyAction {
    pub category: ActionCategory,
    pub code1: u8,
    pub code2: u8,
    pub code3: u8,
}

impl Default for KeyAction {
    fn default() -> Self {
        Self::disabled()
    }
}

impl KeyAction {
    /// Creates a disabled / unmapped key action.
    pub const fn disabled() -> Self {
        Self {
            category: ActionCategory::Disabled,
            code1: 0,
            code2: 0,
            code3: 0,
        }
    }

    /// Creates a standard USB HID keyboard key action (no modifiers).
    pub const fn standard(scancode: u8) -> Self {
        Self {
            category: ActionCategory::StandardHidKey,
            code1: 0,
            code2: scancode,
            code3: 0,
        }
    }

    /// Alias for `standard(scancode)` for backward compatibility.
    pub const fn standard_key(scancode: u8) -> Self {
        Self::standard(scancode)
    }

    /// Creates a key combination (modifiers bitmask + base scancode).
    ///
    /// Modifiers bitmask:
    /// - Bit 0 (`0x01`): Left Ctrl (Bit 4 `0x10`: Right Ctrl)
    /// - Bit 1 (`0x02`): Left Shift (Bit 5 `0x20`: Right Shift)
    /// - Bit 2 (`0x04`): Left Alt (Bit 6 `0x40`: Right Alt)
    /// - Bit 3 (`0x08`): Left Win (Bit 7 `0x80`: Right Win)
    pub const fn combo(modifiers: u8, scancode: u8) -> Self {
        Self {
            category: ActionCategory::StandardHidKey,
            code1: modifiers,
            code2: scancode,
            code3: 0,
        }
    }

    /// Creates a mouse button action.
    ///
    /// Buttons bitmask: 1=Left, 2=Right, 4=Middle, 8=Backward, 16=Forward.
    pub const fn mouse_button(buttons: u8) -> Self {
        Self {
            category: ActionCategory::MouseAction,
            code1: buttons,
            code2: 0,
            code3: 0,
        }
    }

    /// Creates a mouse wheel scroll action.
    ///
    /// `delta`: 1 for Wheel Up, -1 (255 / 0xFF) for Wheel Down.
    pub const fn mouse_wheel(delta: i8) -> Self {
        Self {
            category: ActionCategory::MouseAction,
            code1: 0,
            code2: 0,
            code3: delta as u8,
        }
    }

    /// Creates a relative cursor move action.
    pub const fn cursor_move(dx: i8, dy: i8) -> Self {
        Self {
            category: ActionCategory::CursorControl,
            code1: dx as u8,
            code2: dy as u8,
            code3: 0,
        }
    }

    /// Creates a multimedia consumer control action from a 16-bit USB HID Consumer usage ID.
    pub const fn multimedia(consumer_usage_id: u16) -> Self {
        Self {
            category: ActionCategory::MultimediaOs,
            code1: (consumer_usage_id & 0xFF) as u8,
            code2: ((consumer_usage_id >> 8) & 0xFF) as u8,
            code3: 0,
        }
    }

    /// Creates a hardware lighting or device function action (Type 31).
    pub const fn hardware_custom(action_id: u8) -> Self {
        Self {
            category: ActionCategory::HardwareLighting,
            code1: action_id,
            code2: 0,
            code3: 0,
        }
    }

    /// Creates a WinLock (GUI Lock) toggle action (Type 31, code1 = 0x07).
    pub const fn win_lock() -> Self {
        Self {
            category: ActionCategory::HardwareLighting,
            code1: 0x07,
            code2: 0,
            code3: 0,
        }
    }

    /// Creates an ACPI power management action (1=Power, 2=Sleep, 4=Wakeup).
    pub const fn power(code: u8) -> Self {
        Self {
            category: ActionCategory::PowerManagement,
            code1: code,
            code2: 0,
            code3: 0,
        }
    }

    /// Creates a hardware macro trigger action targeting slot index 0..15.
    ///
    /// `loop_mode`:
    /// - 0: Play once or Repeat `repeat` times (code2 = repeat)
    /// - 1: Loop until key is released (Hold to loop)
    /// - 2: Loop until any key is pressed
    /// - 3: Loop until trigger key is pressed again (Toggle loop)
    pub const fn macro_trigger(slot_index: u8, loop_mode: u8, repeat: u8) -> Self {
        Self {
            category: ActionCategory::HardwareMacro,
            code1: slot_index,
            code2: repeat,
            code3: loop_mode,
        }
    }

    /// Encodes this action into a 4-byte slice.
    pub fn to_bytes(&self) -> [u8; 4] {
        [self.category as u8, self.code1, self.code2, self.code3]
    }

    /// Decodes an action from a 4-byte slice.
    pub fn from_bytes(bytes: [u8; 4]) -> Result<Self, ProtocolError> {
        let cat = match bytes[0] {
            0 => ActionCategory::Disabled,
            16 => ActionCategory::MouseAction,
            17 => ActionCategory::CursorControl,
            31 => ActionCategory::HardwareLighting,
            32 => ActionCategory::StandardHidKey,
            48 => ActionCategory::MultimediaOs,
            64 => ActionCategory::PowerManagement,
            96 => ActionCategory::HardwareMacro,
            _other => ActionCategory::Disabled,
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

/// Builds 9 64-byte USB Output Reports to write the full 504-byte keymap table for a layer (Opcode 0x09).
///
/// 126 sparse slots * 4 bytes = 504 bytes across 9 packets (56 bytes each).
pub fn build_batch_keymap_reports(
    layer: KeyLayer,
    descriptors: &[KeyAction; SPARSE_KEY_COUNT],
) -> Result<Vec<ReportBuffer>, ProtocolError> {
    let mut raw_bytes = [0u8; SPARSE_KEY_COUNT * 4];
    for (i, action) in descriptors.iter().enumerate() {
        let b = action.to_bytes();
        raw_bytes[i * 4..i * 4 + 4].copy_from_slice(&b);
    }

    let mut reports = Vec::with_capacity(9);
    for chunk in 0..9 {
        let offset = (chunk as u16) * 56;
        let mut report = ReportBuffer::new(opcodes::SET_KEYMAP_CHUNK);
        report.set_byte(2, 59)?; // 56 bytes payload + 3 header bytes
        report.set_byte(3, (offset & 0xFF) as u8)?;
        report.set_byte(4, ((offset >> 8) & 0xFF) as u8)?;
        report.set_byte(5, 0x00)?;
        report.set_byte(6, layer as u8)?;
        report.set_byte(7, 0x00)?;
        report.write_payload(8, &raw_bytes[offset as usize..offset as usize + 56])?;
        reports.push(report);
    }
    Ok(reports)
}

/// Builds a 64-byte USB Output Report requesting a keymap table chunk readback (Opcode 0x08).
///
/// Chunks 0..8 (56 bytes each, covering 504 bytes).
pub fn build_keymap_read_request(chunk_index: u8, layer: KeyLayer) -> Result<ReportBuffer, ProtocolError> {
    if chunk_index > 8 {
        return Err(ProtocolError::ParameterOutOfRange {
            field: "chunk_index",
            value: chunk_index as u32,
            min: 0,
            max: 8,
        });
    }

    let offset = (chunk_index as u16) * 56;
    let mut report = ReportBuffer::new(opcodes::GET_KEYMAP_CHUNK);
    report.set_byte(2, 58)?; // header length descriptor
    report.set_byte(3, (offset & 0xFF) as u8)?;
    report.set_byte(4, ((offset >> 8) & 0xFF) as u8)?;
    report.set_byte(5, 0x00)?;
    report.set_byte(6, layer as u8)?;
    Ok(report)
}

/// Parses a 56-byte payload chunk from a device response into 14 `KeyAction`s.
pub fn parse_keymap_chunk_response(resp: &[u8]) -> Result<[KeyAction; 14], ProtocolError> {
    if resp.len() < 64 {
        return Err(ProtocolError::InvalidLength(resp.len()));
    }
    let data = &resp[8..64]; // 56 bytes
    let mut actions = [KeyAction::disabled(); 14];
    for i in 0..14 {
        let b = [data[i * 4], data[i * 4 + 1], data[i * 4 + 2], data[i * 4 + 3]];
        actions[i] = KeyAction::from_bytes(b)?;
    }
    Ok(actions)
}
