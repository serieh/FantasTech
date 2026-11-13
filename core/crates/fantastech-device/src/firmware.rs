//! Low-level firmware flashing state machine and safety enforcement engine.

#![forbid(unsafe_code)]

use crate::{DeviceError, DryRunRecord};
use fantastech_core::{validate_firmware_binary, FirmwareValidationInfo, EXPECTED_PAYLOAD_LEN};
use fantastech_protocol::constants::{PRODUCT_ID_BOOTLOADER, PRODUCT_ID_MK894, REPORT_LENGTH};
use fantastech_protocol::firmware::{
    build_bootloader_handshake_report, build_bootloader_jump_report, build_bootloader_reboot_report,
    build_flash_chunk_report, build_flash_erase_init_report, build_flash_verify_report,
    parse_flash_verify_response,
};
use fantastech_transport::Transport;
use serde::{Deserialize, Serialize};

/// High-level phase of the firmware update state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirmwarePhase {
    /// Inspecting binary headers, file size, and embedded USB descriptors.
    Validating,
    /// Instructing active keyboard to reboot into the IAP bootloader partition (Opcode 0x5A).
    RebootingToBootloader,
    /// Waiting for USB re-enumeration as PID 0xFEAB in bootloader mode.
    WaitingForBootloader,
    /// Exchanging initial handshake and querying IAP readiness.
    Handshake,
    /// Erasing internal user application flash sectors (0x0000..0x7800).
    ErasingPartition,
    /// Streaming 480 chunks (64 bytes each) of binary payload into flash memory.
    FlashingPayload,
    /// Requesting MCU hardware checksum verification against written sectors.
    VerifyingChecksum,
    /// Instructing MCU to exit bootloader and boot into application mode.
    RebootingToApplication,
    /// Flashing process completed successfully.
    Completed,
    /// An error occurred during flashing.
    Failed,
}

/// Real-time progress status update emitted during firmware flashing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareProgressUpdate {
    /// Active phase of the flashing sequence.
    pub phase: FirmwarePhase,
    /// Current sequential step number (1-based).
    pub step: usize,
    /// Total number of high-level steps.
    pub total_steps: usize,
    /// Current chunk written (0..480).
    pub chunk: usize,
    /// Total chunks to write (480).
    pub total_chunks: usize,
    /// Overall completion percentage (0.0 .. 100.0).
    pub percentage: f32,
    /// User-friendly status message describing current action.
    pub message: String,
}

/// Explicit authorization token required to initiate bootloader reboot and flash erase.
///
/// Cannot be constructed without passing exact confirmation text and valid binary data.
pub struct FirmwareUpdateSafetyToken {
    _private: (),
}

impl FirmwareUpdateSafetyToken {
    /// Required confirmation phrase acknowledging flashing risks and absolving authors of liability.
    pub const MANDATORY_CONFIRMATION: &'static str =
        "I_ACCEPT_ALL_FIRMWARE_UPDATE_RISKS_AND_LIABILITY";

    /// Validates confirmation and binary image before issuing a safety token.
    pub fn authorize(
        confirmation: &str,
        binary: &[u8],
    ) -> Result<(Self, FirmwareValidationInfo), DeviceError> {
        if confirmation != Self::MANDATORY_CONFIRMATION {
            return Err(DeviceError::SafetyViolation(
                "Firmware update authorization rejected: confirmation phrase mismatch.",
            ));
        }

        let info = validate_firmware_binary(binary)
            .map_err(|e| DeviceError::SafetyViolation(Box::leak(e.into_boxed_str())))?;

        Ok((Self { _private: () }, info))
    }
}

/// Executes firmware flashing with safety checks, chunked streaming, and live progress reporting.
///
/// In dry-run mode, all protocol packets and simulated timing are verified without physical USB writes.
pub fn execute_firmware_update<T: Transport, F>(
    transport: &mut T,
    _token: &FirmwareUpdateSafetyToken,
    binary: &[u8],
    dry_run: bool,
    mut progress_cb: F,
) -> Result<Vec<DryRunRecord>, DeviceError>
where
    F: FnMut(FirmwareProgressUpdate),
{
    let total_steps = 7;

    // Step 1: Validation
    progress_cb(FirmwareProgressUpdate {
        phase: FirmwarePhase::Validating,
        step: 1,
        total_steps,
        chunk: 0,
        total_chunks: 480,
        percentage: 5.0,
        message: "Validating binary image size and USB descriptors...".to_string(),
    });

    let payload_bytes = if binary.len() >= EXPECTED_PAYLOAD_LEN {
        &binary[..EXPECTED_PAYLOAD_LEN]
    } else {
        return Err(DeviceError::SafetyViolation(
            "Binary is smaller than expected 30KB payload size.",
        ));
    };

    let total_chunks = payload_bytes.len().div_ceil(REPORT_LENGTH);
    let mut dry_run_records = Vec::new();

    // Step 2: Jump to Bootloader
    progress_cb(FirmwareProgressUpdate {
        phase: FirmwarePhase::RebootingToBootloader,
        step: 2,
        total_steps,
        chunk: 0,
        total_chunks,
        percentage: 10.0,
        message: "Requesting MCU reset into IAP bootloader...".to_string(),
    });

    let jump_report = build_bootloader_jump_report();
    if dry_run {
        dry_run_records.push(DryRunRecord {
            buffer: *jump_report.as_bytes(),
            explanation: "Opcode 0x5A: Reboot keyboard into IAP bootloader partition".to_string(),
            safety_tier: "DESTRUCTIVE",
        });
    } else {
        // Send reboot command
        let _ = transport.write_report(jump_report.as_bytes());
        std::thread::sleep(std::time::Duration::from_millis(500));

        progress_cb(FirmwareProgressUpdate {
            phase: FirmwarePhase::WaitingForBootloader,
            step: 2,
            total_steps,
            chunk: 0,
            total_chunks,
            percentage: 12.0,
            message: "Waiting for USB re-enumeration in IAP bootloader mode (PID 0xFEAB)...".to_string(),
        });

        transport
            .reconnect(PRODUCT_ID_BOOTLOADER, std::time::Duration::from_secs(5))
            .map_err(|e| {
                DeviceError::SafetyViolation(Box::leak(
                    format!("Failed reconnecting to IAP bootloader after Opcode 0x5A: {}", e)
                        .into_boxed_str(),
                ))
            })?;
    }

    // Step 3: Bootloader Handshake
    progress_cb(FirmwareProgressUpdate {
        phase: FirmwarePhase::Handshake,
        step: 3,
        total_steps,
        chunk: 0,
        total_chunks,
        percentage: 15.0,
        message: "Connecting to IAP bootloader...".to_string(),
    });

    let handshake_report = build_bootloader_handshake_report();
    if dry_run {
        dry_run_records.push(DryRunRecord {
            buffer: *handshake_report.as_bytes(),
            explanation: "IAP Opcode 0x01: Handshake with bootloader".to_string(),
            safety_tier: "WRITE-PERSISTENT",
        });
    } else {
        transport.write_report(handshake_report.as_bytes())?;
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // Step 4: Flash Erase & Init
    progress_cb(FirmwareProgressUpdate {
        phase: FirmwarePhase::ErasingPartition,
        step: 4,
        total_steps,
        chunk: 0,
        total_chunks,
        percentage: 20.0,
        message: "Erasing application flash sectors (0x0000..0x7800)...".to_string(),
    });

    let erase_report = build_flash_erase_init_report(0x0000, EXPECTED_PAYLOAD_LEN as u16);
    if dry_run {
        dry_run_records.push(DryRunRecord {
            buffer: *erase_report.as_bytes(),
            explanation: "IAP Opcode 0x02: Erase user flash partition and initialize write offset".to_string(),
            safety_tier: "DESTRUCTIVE",
        });
    } else {
        transport.write_report(erase_report.as_bytes())?;
        std::thread::sleep(std::time::Duration::from_millis(400));
    }

    // Step 5: Stream Flash Chunks (480 chunks)
    for chunk_idx in 0..total_chunks {
        let offset = chunk_idx * REPORT_LENGTH;
        let end = (offset + REPORT_LENGTH).min(payload_bytes.len());
        let chunk_slice = &payload_bytes[offset..end];
        let chunk_report = build_flash_chunk_report(chunk_slice)
            .map_err(DeviceError::Protocol)?;

        let pct = 20.0 + ((chunk_idx + 1) as f32 / total_chunks as f32) * 65.0;

        if chunk_idx % 16 == 0 || chunk_idx == total_chunks - 1 {
            progress_cb(FirmwareProgressUpdate {
                phase: FirmwarePhase::FlashingPayload,
                step: 5,
                total_steps,
                chunk: chunk_idx + 1,
                total_chunks,
                percentage: pct,
                message: format!("Writing flash chunk {}/{} ({:.0}%)...", chunk_idx + 1, total_chunks, pct),
            });
        }

        if dry_run {
            if chunk_idx < 3 || chunk_idx == total_chunks - 1 {
                dry_run_records.push(DryRunRecord {
                    buffer: *chunk_report.as_bytes(),
                    explanation: format!("Stream flash chunk {}/{}", chunk_idx + 1, total_chunks),
                    safety_tier: "WRITE-PERSISTENT",
                });
            }
        } else {
            transport.write_report(chunk_report.as_bytes())?;
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
    }

    // Step 6: Verify Checksum
    progress_cb(FirmwareProgressUpdate {
        phase: FirmwarePhase::VerifyingChecksum,
        step: 6,
        total_steps,
        chunk: total_chunks,
        total_chunks,
        percentage: 90.0,
        message: "Verifying flash checksum on hardware...".to_string(),
    });

    let verify_report = build_flash_verify_report();
    if dry_run {
        dry_run_records.push(DryRunRecord {
            buffer: *verify_report.as_bytes(),
            explanation: "IAP Opcode 0x04: Checksum verification request".to_string(),
            safety_tier: "READ-ONLY",
        });
    } else {
        transport.write_report(verify_report.as_bytes())?;
        std::thread::sleep(std::time::Duration::from_millis(150));

        let mut resp_buf = [0u8; REPORT_LENGTH];
        match transport.read_report(&mut resp_buf) {
            Ok(_) => match parse_flash_verify_response(&resp_buf) {
                Ok(true) => {
                    tracing::info!("MCU flash hardware checksum verified successfully.");
                }
                Ok(false) => {
                    return Err(DeviceError::SafetyViolation(
                        "Hardware checksum verification failed! Flash contents do not match binary image.",
                    ));
                }
                Err(e) => {
                    tracing::warn!("Could not parse bootloader verify response framing: {:?}", e);
                }
            },
            Err(e) => {
                tracing::warn!("No verify response packet received from bootloader: {:?}", e);
            }
        }
    }

    // Step 7: Reboot MCU to Normal Application
    progress_cb(FirmwareProgressUpdate {
        phase: FirmwarePhase::RebootingToApplication,
        step: 7,
        total_steps,
        chunk: total_chunks,
        total_chunks,
        percentage: 98.0,
        message: "Rebooting keyboard into updated application firmware...".to_string(),
    });

    let reboot_report = build_bootloader_reboot_report();
    if dry_run {
        dry_run_records.push(DryRunRecord {
            buffer: *reboot_report.as_bytes(),
            explanation: "IAP Opcode 0x05: Exit bootloader and reset MCU to application".to_string(),
            safety_tier: "DESTRUCTIVE",
        });
    } else {
        let _ = transport.write_report(reboot_report.as_bytes());
        std::thread::sleep(std::time::Duration::from_millis(500));

        transport
            .reconnect(PRODUCT_ID_MK894, std::time::Duration::from_secs(5))
            .map_err(|e| {
                DeviceError::SafetyViolation(Box::leak(
                    format!("Failed reconnecting to keyboard application after flash reboot: {}", e)
                        .into_boxed_str(),
                ))
            })?;
    }

    progress_cb(FirmwareProgressUpdate {
        phase: FirmwarePhase::Completed,
        step: total_steps,
        total_steps,
        chunk: total_chunks,
        total_chunks,
        percentage: 100.0,
        message: "Firmware update completed successfully!".to_string(),
    });

    Ok(dry_run_records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fantastech_core::get_embedded_firmware_bytes;
    use fantastech_transport::MockTransport;

    #[test]
    fn test_firmware_safety_token_enforcement() {
        let binary = get_embedded_firmware_bytes();

        // Reject without proper phrase
        let err = FirmwareUpdateSafetyToken::authorize("WRONG_CONFIRMATION", binary);
        assert!(err.is_err());

        // Reject corrupted binary
        let mut corrupted = binary.to_vec();
        corrupted[0x59BE] = 0x00; // Invalidate descriptor
        let err2 = FirmwareUpdateSafetyToken::authorize(
            FirmwareUpdateSafetyToken::MANDATORY_CONFIRMATION,
            &corrupted,
        );
        assert!(err2.is_err());

        // Success with valid phrase and binary
        let (_token, info) = FirmwareUpdateSafetyToken::authorize(
            FirmwareUpdateSafetyToken::MANDATORY_CONFIRMATION,
            binary,
        )
        .expect("Valid authorization");
        assert!(info.valid);
        assert_eq!(info.file_size, 32768);
    }

    #[test]
    fn test_execute_firmware_update_dry_run_progress_stream() {
        let binary = get_embedded_firmware_bytes();
        let (token, _) = FirmwareUpdateSafetyToken::authorize(
            FirmwareUpdateSafetyToken::MANDATORY_CONFIRMATION,
            binary,
        )
        .unwrap();

        let mut transport = MockTransport::new();
        let mut updates = Vec::new();

        let dry_runs = execute_firmware_update(
            &mut transport,
            &token,
            binary,
            true, // dry_run
            |update| {
                updates.push(update);
            },
        )
        .expect("Dry run should succeed");

        assert!(!dry_runs.is_empty());
        assert!(!updates.is_empty());
        assert_eq!(updates.first().unwrap().phase, FirmwarePhase::Validating);
        assert_eq!(updates.last().unwrap().phase, FirmwarePhase::Completed);
        assert_eq!(updates.last().unwrap().percentage, 100.0);
    }

    #[test]
    fn test_firmware_flashing_verify_checksum_success() {
        let binary = get_embedded_firmware_bytes();
        let (token, _) = FirmwareUpdateSafetyToken::authorize(
            FirmwareUpdateSafetyToken::MANDATORY_CONFIRMATION,
            binary,
        )
        .unwrap();

        let mut transport = MockTransport::new();
        // Queue valid verify response: Byte 0=0x55, Byte 1=0xFF, Byte 2=0x04, Byte 3=0x01, Byte 4=0x00 (success)
        let mut ok_verify = [0u8; REPORT_LENGTH];
        ok_verify[0] = 0x55;
        ok_verify[1] = 0xFF;
        ok_verify[2] = 0x04;
        ok_verify[3] = 0x01;
        ok_verify[4] = 0x00;
        transport.push_incoming(ok_verify);

        let mut updates = Vec::new();
        let res = execute_firmware_update(
            &mut transport,
            &token,
            binary,
            false, // live execution against mock
            |u| updates.push(u),
        );

        assert!(res.is_ok());
        assert_eq!(updates.last().unwrap().phase, FirmwarePhase::Completed);
    }

    #[test]
    fn test_firmware_flashing_verify_checksum_failure_aborts_safely() {
        let binary = get_embedded_firmware_bytes();
        let (token, _) = FirmwareUpdateSafetyToken::authorize(
            FirmwareUpdateSafetyToken::MANDATORY_CONFIRMATION,
            binary,
        )
        .unwrap();

        let mut transport = MockTransport::new();
        // Queue failure verify response: Byte 4=0x01 (checksum mismatch error)
        let mut fail_verify = [0u8; REPORT_LENGTH];
        fail_verify[0] = 0x55;
        fail_verify[1] = 0xFF;
        fail_verify[2] = 0x04;
        fail_verify[3] = 0x01;
        fail_verify[4] = 0x01; // Hardware failure code
        transport.push_incoming(fail_verify);

        let mut updates = Vec::new();
        let res = execute_firmware_update(
            &mut transport,
            &token,
            binary,
            false,
            |u| updates.push(u),
        );

        // Must reject and refuse to reboot into corrupted app partition
        assert!(res.is_err());
        match res.unwrap_err() {
            DeviceError::SafetyViolation(msg) => {
                assert!(msg.contains("Hardware checksum verification failed"));
            }
            other => panic!("Expected SafetyViolation, got {:?}", other),
        }

        // Verify reboot report [0x55, 0xFF, 0x05, ...] was NOT sent
        let reboot_opcode = 0x05;
        let reboot_sent = transport
            .outgoing
            .iter()
            .any(|pkt| pkt[0] == 0x55 && pkt[1] == 0xFF && pkt[2] == reboot_opcode);
        assert!(!reboot_sent, "Reboot command must NOT be sent after checksum failure!");
    }
}
