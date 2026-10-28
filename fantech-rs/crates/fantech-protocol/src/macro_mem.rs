//! The "4KB Engine": Autonomous 4,096-byte onboard hardware macro memory.

use crate::constants::{opcodes, MACRO_MEMORY_SIZE, MACRO_SLOT_COUNT};
use crate::error::ProtocolError;
use crate::report::ReportBuffer;

/// Event type executed within a macro action record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MacroEventType {
    KeyboardKey = 2,
    MouseButton = 3,
    MouseWheelVertical = 4,
    MouseWheelHorizontal = 5,
}

/// A 4-byte action record stored within the keyboard's 4KB macro memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacroActionRecord {
    /// Delay before execution in milliseconds (0..65535 ms).
    pub delay_ms: u16,
    /// Whether the key/button is pressed (true) or released (false).
    pub is_press: bool,
    /// Whether this record marks the end of the macro sequence.
    pub is_end_of_macro: bool,
    /// The event type (keyboard scancode vs mouse event).
    pub event_type: MacroEventType,
    /// Target USB HID scancode or mouse button code.
    pub code: u8,
}

impl MacroActionRecord {
    /// Encodes this record into the 4-byte hardware wire format.
    pub fn to_bytes(&self) -> [u8; 4] {
        let mut flags = self.event_type as u8 & 0x3F;
        if self.is_press {
            flags |= 0x40; // Bit 6 = Key Down
        }
        if self.is_end_of_macro {
            flags |= 0x80; // Bit 7 = End of Macro
        }

        [
            (self.delay_ms & 0xFF) as u8,
            ((self.delay_ms >> 8) & 0xFF) as u8,
            flags,
            self.code,
        ]
    }

    /// Decodes a 4-byte hardware wire slice into an action record.
    pub fn from_bytes(bytes: [u8; 4]) -> Result<Self, ProtocolError> {
        let delay_ms = u16::from_le_bytes([bytes[0], bytes[1]]);
        let flags = bytes[2];
        let is_end_of_macro = (flags & 0x80) != 0;
        let is_press = (flags & 0x40) != 0;
        let event_val = flags & 0x3F;

        let event_type = match event_val {
            2 => MacroEventType::KeyboardKey,
            3 => MacroEventType::MouseButton,
            4 => MacroEventType::MouseWheelVertical,
            5 => MacroEventType::MouseWheelHorizontal,
            other => {
                return Err(ProtocolError::ParameterOutOfRange {
                    field: "macro_event_type",
                    value: other as u32,
                    min: 2,
                    max: 5,
                })
            }
        };

        Ok(Self {
            delay_ms,
            is_press,
            is_end_of_macro,
            event_type,
            code: bytes[3],
        })
    }
}

/// An in-memory representation of the full 4,096-byte onboard macro memory table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroMemoryTable {
    raw: [u8; MACRO_MEMORY_SIZE],
}

impl Default for MacroMemoryTable {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl MacroMemoryTable {
    /// Creates a new empty macro memory buffer with all 32 slots pointing to 0xFFFF.
    pub fn new_empty() -> Self {
        let mut raw = [0u8; MACRO_MEMORY_SIZE];
        for i in 0..MACRO_SLOT_COUNT {
            raw[i * 2] = 0xFF;
            raw[i * 2 + 1] = 0xFF;
        }
        Self { raw }
    }

    /// Creates a table from a raw 4096-byte array.
    pub fn from_raw(raw: [u8; MACRO_MEMORY_SIZE]) -> Self {
        Self { raw }
    }

    /// Returns an immutable reference to the raw 4KB memory bytes.
    pub fn as_bytes(&self) -> &[u8; MACRO_MEMORY_SIZE] {
        &self.raw
    }

    /// Reads the 16-bit starting pointer for a specified macro slot (0..31).
    pub fn get_slot_pointer(&self, slot: usize) -> Result<u16, ProtocolError> {
        if slot >= MACRO_SLOT_COUNT {
            return Err(ProtocolError::MacroSlotOutOfBounds(slot));
        }
        let lo = self.raw[slot * 2];
        let hi = self.raw[slot * 2 + 1];
        Ok(u16::from_le_bytes([lo, hi]))
    }

    /// Sets the 16-bit starting pointer for a specified macro slot (0..31).
    pub fn set_slot_pointer(&mut self, slot: usize, offset: u16) -> Result<(), ProtocolError> {
        if slot >= MACRO_SLOT_COUNT {
            return Err(ProtocolError::MacroSlotOutOfBounds(slot));
        }
        let bytes = offset.to_le_bytes();
        self.raw[slot * 2] = bytes[0];
        self.raw[slot * 2 + 1] = bytes[1];
        Ok(())
    }

    /// Writes raw action bytes at the specified byte offset within the 4KB memory space.
    pub fn write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), ProtocolError> {
        if offset < 64 || offset + data.len() > MACRO_MEMORY_SIZE {
            return Err(ProtocolError::InvalidLength(offset + data.len()));
        }
        self.raw[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }
}

/// Builds a 64-byte USB Output Report to write a chunk of macro memory (Opcode 0x0D).
pub fn build_macro_write_chunk(chunk_index: u8, data: &[u8]) -> Result<ReportBuffer, ProtocolError> {
    if data.len() > 56 {
        return Err(ProtocolError::InvalidLength(data.len()));
    }

    let offset = (chunk_index as u16) * 56;
    let mut report = ReportBuffer::new(opcodes::SET_MACRO_CHUNK);
    report.set_byte(2, data.len() as u8)?;
    report.set_byte(3, (offset & 0xFF) as u8)?;
    report.set_byte(4, ((offset >> 8) & 0xFF) as u8)?;
    report.write_payload(5, data)?;
    Ok(report)
}

/// Builds a 64-byte USB Output Report requesting a chunk readback of macro memory (Opcode 0x0C).
pub fn build_macro_read_request(chunk_index: u8) -> Result<ReportBuffer, ProtocolError> {
    let offset = (chunk_index as u16) * 56;
    let len = if offset + 56 <= 4096 {
        56
    } else {
        (4096 - offset) as u8
    };

    let mut report = ReportBuffer::new(opcodes::GET_MACRO_CHUNK);
    report.set_byte(2, len)?;
    report.set_byte(3, (offset & 0xFF) as u8)?;
    report.set_byte(4, ((offset >> 8) & 0xFF) as u8)?;
    Ok(report)
}

/// Builds the report to wipe all onboard macros (Opcode 0x0F, subcommand 0x04).
pub fn build_macro_wipe_report() -> Result<ReportBuffer, ProtocolError> {
    let mut report = ReportBuffer::new(opcodes::RESTORE_FACTORY);
    report.set_byte(2, 0x04)?; // Subcommand 4: Wipe macro memory
    Ok(report)
}
