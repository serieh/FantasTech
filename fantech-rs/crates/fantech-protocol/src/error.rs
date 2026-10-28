//! Strongly-typed protocol and validation errors.

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("Invalid report buffer length: expected 65 bytes, got {0}")]
    InvalidLength(usize),

    #[error("Invalid report ID: expected 0x06, got 0x{0:02X}")]
    InvalidReportId(u8),

    #[error("Attempted to construct banned or destructive opcode 0x{0:02X} (goToIAP / bootloader jump)")]
    BannedOpcodeAttempted(u8),

    #[error("Unrecognized or unsupported opcode: 0x{0:02X}")]
    UnknownOpcode(u8),

    #[error("Key index {0} exceeds physical key count (104 keys)")]
    KeyIndexOutOfBounds(usize),

    #[error("Macro slot {0} exceeds maximum supported macro slots (32 slots)")]
    MacroSlotOutOfBounds(usize),

    #[error("Invalid parameter value for {field}: {value} (allowed range: {min}..={max})")]
    ParameterOutOfRange {
        field: &'static str,
        value: u32,
        min: u32,
        max: u32,
    },

    #[error("Malformed response from device for opcode 0x{opcode:02X}: {detail}")]
    MalformedResponse {
        opcode: u8,
        detail: &'static str,
    },
}
