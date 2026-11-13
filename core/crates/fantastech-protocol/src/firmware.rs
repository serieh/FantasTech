//! Wire protocol packets and builders for the IAP bootloader flashing state machine.

#![forbid(unsafe_code)]

use crate::constants::{opcodes, REPORT_LENGTH};
use crate::error::ProtocolError;
use crate::report::ReportBuffer;

/// Bootloader magic byte identifying IAP commands.
pub const IAP_MAGIC: u8 = 0x55;

/// Bootloader sub-magic tag.
pub const IAP_TAG: u8 = 0xFF;

/// IAP Command Opcode: Handshake / Ping.
pub const IAP_CMD_HANDSHAKE: u8 = 0x01;

/// IAP Command Opcode: Erase Application Partition & Set Write Address/Length.
pub const IAP_CMD_ERASE_INIT: u8 = 0x02;

/// IAP Command Opcode: Checksum Verification.
pub const IAP_CMD_VERIFY: u8 = 0x04;

/// IAP Command Opcode: Reboot MCU to Normal Operating Mode.
pub const IAP_CMD_REBOOT: u8 = 0x05;

/// Raw 64-byte IAP bootloader packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootloaderReport {
    data: [u8; REPORT_LENGTH],
}

impl Default for BootloaderReport {
    fn default() -> Self {
        Self {
            data: [0u8; REPORT_LENGTH],
        }
    }
}

impl BootloaderReport {
    /// Returns reference to raw 64-byte payload.
    #[inline]
    pub fn as_bytes(&self) -> &[u8; REPORT_LENGTH] {
        &self.data
    }

    /// Creates a report from a raw 64-byte array.
    pub fn from_raw(data: [u8; REPORT_LENGTH]) -> Self {
        Self { data }
    }
}

/// Builds the normal-mode command that instructs the MCU to reboot into the IAP bootloader.
///
/// NOTE: This opcode (0x5A) will reset the device. Only issue with an authorized safety token!
pub fn build_bootloader_jump_report() -> ReportBuffer {
    let mut report = ReportBuffer::new(opcodes::BANNED_JUMP_BOOTLOADER);
    let _ = report.set_byte(2, 0xA0);
    report
}

/// Builds the IAP handshake packet to ping the bootloader.
///
/// Format: `[0x55, 0xFF, 0x01, 0x00, 0x00, ...]` (64 bytes).
pub fn build_bootloader_handshake_report() -> BootloaderReport {
    let mut data = [0u8; REPORT_LENGTH];
    data[0] = IAP_MAGIC;
    data[1] = IAP_TAG;
    data[2] = IAP_CMD_HANDSHAKE;
    data[3] = 0x00;
    BootloaderReport { data }
}

/// Builds the IAP packet to erase the application flash partition and set write parameters.
///
/// Format: `[0x55, 0xFF, 0x02, 0x04, hi(addr), lo(addr), hi(len), lo(len), ...]`
pub fn build_flash_erase_init_report(start_addr: u16, length: u16) -> BootloaderReport {
    let mut data = [0u8; REPORT_LENGTH];
    data[0] = IAP_MAGIC;
    data[1] = IAP_TAG;
    data[2] = IAP_CMD_ERASE_INIT;
    data[3] = 0x04; // Payload length of parameter block (4 bytes)
    data[4] = (start_addr >> 8) as u8;
    data[5] = (start_addr & 0xFF) as u8;
    data[6] = (length >> 8) as u8;
    data[7] = (length & 0xFF) as u8;
    BootloaderReport { data }
}

/// Builds a 64-byte raw chunk write packet for the IAP bootloader stream.
pub fn build_flash_chunk_report(chunk: &[u8]) -> Result<BootloaderReport, ProtocolError> {
    if chunk.len() > REPORT_LENGTH {
        return Err(ProtocolError::InvalidLength(chunk.len()));
    }
    let mut data = [0xFFu8; REPORT_LENGTH]; // Unwritten bytes default to erased flash 0xFF
    data[..chunk.len()].copy_from_slice(chunk);
    Ok(BootloaderReport { data })
}

/// Builds the IAP checksum verification request packet.
///
/// Format: `[0x55, 0xFF, 0x04, 0x00, ...]`
pub fn build_flash_verify_report() -> BootloaderReport {
    let mut data = [0u8; REPORT_LENGTH];
    data[0] = IAP_MAGIC;
    data[1] = IAP_TAG;
    data[2] = IAP_CMD_VERIFY;
    data[3] = 0x00;
    BootloaderReport { data }
}

/// Builds the IAP reboot packet instructing the MCU to exit bootloader and boot into application mode.
///
/// Format: `[0x55, 0xFF, 0x05, 0x01, 0x01, ...]`
pub fn build_bootloader_reboot_report() -> BootloaderReport {
    let mut data = [0u8; REPORT_LENGTH];
    data[0] = IAP_MAGIC;
    data[1] = IAP_TAG;
    data[2] = IAP_CMD_REBOOT;
    data[3] = 0x01; // Length
    data[4] = 0x01; // Reset target: Application partition
    BootloaderReport { data }
}

/// Parses the 64-byte verify report response returned by the IAP bootloader.
///
/// Returns `Ok(true)` if hardware checksum matched, `Ok(false)` on mismatch,
/// or `Err(ProtocolError)` if packet header is invalid.
pub fn parse_flash_verify_response(report: &[u8; REPORT_LENGTH]) -> Result<bool, ProtocolError> {
    if report[0] != IAP_MAGIC || report[1] != IAP_TAG {
        return Err(ProtocolError::MalformedResponse {
            opcode: report[2],
            detail: "Invalid IAP bootloader response magic header",
        });
    }
    if report[2] != IAP_CMD_VERIFY {
        return Err(ProtocolError::UnknownOpcode(report[2]));
    }
    // Byte 4 is the status code: 0x00 = success / checksum match
    let success = report[4] == 0x00;
    Ok(success)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::PROTOCOL_MAGIC;

    #[test]
    fn test_bootloader_jump_framing() {
        let report = build_bootloader_jump_report();
        let bytes = report.as_bytes();
        assert_eq!(bytes[0], PROTOCOL_MAGIC);
        assert_eq!(bytes[1], 0x5A);
        assert_eq!(bytes[2], 0xA0);
    }

    #[test]
    fn test_bootloader_handshake_framing() {
        let report = build_bootloader_handshake_report();
        let bytes = report.as_bytes();
        assert_eq!(bytes[0], 0x55);
        assert_eq!(bytes[1], 0xFF);
        assert_eq!(bytes[2], 0x01);
        assert_eq!(bytes[3], 0x00);
    }

    #[test]
    fn test_flash_erase_init_framing() {
        let report = build_flash_erase_init_report(0x0000, 0x7800);
        let bytes = report.as_bytes();
        assert_eq!(bytes[0], 0x55);
        assert_eq!(bytes[1], 0xFF);
        assert_eq!(bytes[2], 0x02);
        assert_eq!(bytes[3], 0x04);
        assert_eq!(bytes[4], 0x00);
        assert_eq!(bytes[5], 0x00);
        assert_eq!(bytes[6], 0x78);
        assert_eq!(bytes[7], 0x00);
    }

    #[test]
    fn test_flash_chunk_framing() {
        let raw_chunk = [0x42u8; 64];
        let report = build_flash_chunk_report(&raw_chunk).unwrap();
        assert_eq!(report.as_bytes(), &raw_chunk);

        let partial_chunk = [0x11u8; 32];
        let partial_report = build_flash_chunk_report(&partial_chunk).unwrap();
        assert_eq!(&partial_report.as_bytes()[..32], &partial_chunk);
        assert_eq!(&partial_report.as_bytes()[32..], &[0xFF; 32]);
    }

    #[test]
    fn test_flash_verify_and_reboot_framing() {
        let verify = build_flash_verify_report();
        assert_eq!(verify.as_bytes()[0..4], [0x55, 0xFF, 0x04, 0x00]);

        let reboot = build_bootloader_reboot_report();
        assert_eq!(reboot.as_bytes()[0..5], [0x55, 0xFF, 0x05, 0x01, 0x01]);
    }

    #[test]
    fn test_parse_flash_verify_response() {
        // Valid response: checksum matched (Byte 4 = 0x00)
        let mut ok_resp = [0u8; REPORT_LENGTH];
        ok_resp[0] = IAP_MAGIC;
        ok_resp[1] = IAP_TAG;
        ok_resp[2] = IAP_CMD_VERIFY;
        ok_resp[3] = 0x01;
        ok_resp[4] = 0x00;
        assert_eq!(parse_flash_verify_response(&ok_resp), Ok(true));

        // Valid response: checksum failed (Byte 4 = 0x01)
        let mut fail_resp = ok_resp;
        fail_resp[4] = 0x01;
        assert_eq!(parse_flash_verify_response(&fail_resp), Ok(false));

        // Invalid magic / opcode
        let mut bad_magic = ok_resp;
        bad_magic[0] = 0x00;
        assert!(parse_flash_verify_response(&bad_magic).is_err());
    }
}
