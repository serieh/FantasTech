//! Device maintenance, query commands, and profile switching.

use crate::constants::{opcodes, PROTOCOL_MAGIC};
use crate::error::ProtocolError;
use crate::report::ReportBuffer;

/// Decoded hardware device identity and status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub profile_name: String,
    pub firmware_version: String,
    pub active_profile: u8,
    pub profile_count: u8,
    pub pid: u16,
    pub battery: u8,
    pub charge: u8,
    pub serial_number: String,
}

/// Builds the 64-byte USB Output Report to query device identification and status (Opcode 0x05).
pub fn build_device_info_query() -> Result<ReportBuffer, ProtocolError> {
    Ok(ReportBuffer::new(opcodes::GET_DEVICE_INFO))
}

/// Parses a 64-byte response from the keyboard to a device info query.
pub fn parse_device_info_response(buf: &ReportBuffer) -> Result<DeviceInfo, ProtocolError> {
    let raw = buf.as_bytes();
    if (raw[0] != PROTOCOL_MAGIC && raw[0] != 0xAA) || (raw[1] != opcodes::GET_DEVICE_INFO && raw[1] != 0xFA) {
        return Err(ProtocolError::MalformedResponse {
            opcode: raw[1],
            detail: "Expected Magic 0x06/0xAA and Opcode 0x05 for device query response",
        });
    }

    let pid = u16::from_le_bytes([raw[7], raw[8]]);
    let fw = u16::from_le_bytes([raw[9], raw[10]]);
    let firmware_version = if fw > 0 { format!("V{:X}", fw) } else { "V101".to_string() };

    let battery = raw[13];
    let charge = raw[14];
    let profile_count = raw[15].max(1);
    let active_profile = raw[16];

    let serial_bytes = &raw[21..43.min(raw.len())];
    let serial_end = serial_bytes.iter().position(|&b| b == 0).unwrap_or(serial_bytes.len());
    let serial_number = String::from_utf8_lossy(&serial_bytes[..serial_end]).trim().to_string();
    let profile_name = if serial_number.is_empty() {
        "MK894 (AK107)".to_string()
    } else {
        serial_number.clone()
    };

    Ok(DeviceInfo {
        profile_name,
        firmware_version,
        active_profile,
        profile_count,
        pid: if pid > 0 { pid } else { 0xFE9C },
        battery,
        charge,
        serial_number,
    })
}

/// Builds the 64-byte USB Output Report to switch the active hardware profile (Opcode 0xFB).
pub fn build_profile_select(profile_index: u8) -> Result<ReportBuffer, ProtocolError> {
    if profile_index > 2 {
        return Err(ProtocolError::ParameterOutOfRange {
            field: "profile_index",
            value: profile_index as u32,
            min: 0,
            max: 2,
        });
    }
    let mut report = ReportBuffer::new(opcodes::SELECT_PROFILE);
    report.set_byte(2, profile_index)?;
    Ok(report)
}

/// Builds the 64-byte USB Output Report to trigger a full hardware factory reset (Opcode 0x0F, subcommand 0xFF).
pub fn build_factory_reset() -> Result<ReportBuffer, ProtocolError> {
    let mut report = ReportBuffer::new(opcodes::RESTORE_FACTORY);
    report.set_byte(2, 0xFF)?; // Subcommand 0xFF: Full Factory Reset
    Ok(report)
}

/// Verifies whether an opcode is permitted under the Keyboard Safe Commands Policy.
pub fn assert_opcode_safe(opcode: u8) -> Result<(), ProtocolError> {
    match opcode {
        opcodes::BANNED_JUMP_BOOTLOADER | opcodes::BANNED_ISP_FLASH => {
            Err(ProtocolError::BannedOpcodeAttempted(opcode))
        }
        _ => Ok(()),
    }
}
