//! Fixed 64-byte USB Output Report buffer with strict framing and validation.

use crate::constants::{PROTOCOL_MAGIC, REPORT_LENGTH};
use crate::error::ProtocolError;

/// A validated, fixed 64-byte USB HID report buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportBuffer {
    data: [u8; REPORT_LENGTH],
}

impl Default for ReportBuffer {
    fn default() -> Self {
        Self::new(0x00)
    }
}

impl ReportBuffer {
    /// Creates a new report buffer with Magic 0x06 at byte 0, opcode at byte 1, and zeroes.
    pub fn new(opcode: u8) -> Self {
        let mut data = [0u8; REPORT_LENGTH];
        data[0] = PROTOCOL_MAGIC; // 0x06
        data[1] = opcode;
        Self { data }
    }

    /// Creates a report buffer from an existing 64-byte array, asserting valid protocol magic.
    pub fn from_raw(data: [u8; REPORT_LENGTH]) -> Result<Self, ProtocolError> {
        if data[0] != PROTOCOL_MAGIC && data[0] != 0xAA {
            return Err(ProtocolError::InvalidReportId(data[0]));
        }
        Ok(Self { data })
    }

    /// Creates a report buffer from a byte slice with length assertion.
    pub fn from_slice(slice: &[u8]) -> Result<Self, ProtocolError> {
        if slice.len() != REPORT_LENGTH {
            return Err(ProtocolError::InvalidLength(slice.len()));
        }
        let mut arr = [0u8; REPORT_LENGTH];
        arr.copy_from_slice(slice);
        Self::from_raw(arr)
    }

    /// Returns the protocol magic byte (Byte 0), which is always 0x06.
    #[inline]
    pub fn magic(&self) -> u8 {
        self.data[0]
    }

    /// For backwards compatibility: returns magic byte 0x06.
    #[inline]
    pub fn report_id(&self) -> u8 {
        self.data[0]
    }

    /// Returns the command opcode (Byte 1).
    #[inline]
    pub fn opcode(&self) -> u8 {
        self.data[1]
    }

    /// Returns an immutable reference to the raw 64-byte buffer.
    #[inline]
    pub fn as_bytes(&self) -> &[u8; REPORT_LENGTH] {
        &self.data
    }

    /// Returns a slice to the payload bytes (Bytes 2..64).
    #[inline]
    pub fn payload(&self) -> &[u8] {
        &self.data[2..]
    }

    /// Sets a single byte at the given index.
    pub fn set_byte(&mut self, index: usize, val: u8) -> Result<(), ProtocolError> {
        if index == 0 && val != PROTOCOL_MAGIC {
            return Err(ProtocolError::InvalidReportId(val));
        }
        if index >= REPORT_LENGTH {
            return Err(ProtocolError::InvalidLength(index));
        }
        self.data[index] = val;
        Ok(())
    }

    /// Copies a byte slice into the report buffer starting at the specified offset.
    pub fn write_payload(&mut self, start_offset: usize, bytes: &[u8]) -> Result<(), ProtocolError> {
        if start_offset < 2 || start_offset + bytes.len() > REPORT_LENGTH {
            return Err(ProtocolError::InvalidLength(start_offset + bytes.len()));
        }
        self.data[start_offset..start_offset + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
}
