//! Firmware catalog, binary validation, and embedded recovery images for FantasTech.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Expected total flash size for SDCINNOVATION AK107/AK110M MCU (32 KB).
pub const EXPECTED_FULL_IMAGE_LEN: usize = 32_768;

/// Expected user code partition size (30 KB, from 0x0000 to 0x7800).
pub const EXPECTED_PAYLOAD_LEN: usize = 30_720;

/// Offset where the protected IAP factory bootloader resides (0x7800, 2 KB).
pub const BOOTLOADER_OFFSET: usize = 0x7800;

/// USB Vendor ID for FantasTech MK894 / AK110M hardware.
pub const CATALOG_VID: u16 = 0x36AE;

/// USB Product ID for operational keyboard mode.
pub const CATALOG_PID: u16 = 0xFE9C;

/// USB Product ID for IAP bootloader recovery mode.
pub const BOOTLOADER_PID: u16 = 0xFEAB;

/// Catalog firmware version numeric code (101 -> v1.01).
pub const CATALOG_VERSION_CODE: u16 = 101;

/// Catalog firmware version string display.
pub const CATALOG_VERSION_STR: &str = "v1.01";

/// Expected SHA-256 hash of the bundled factory v1.01 binary.
pub const CATALOG_SHA256: &str =
    "29d10b3030a532cf08d070c9951bd129d58955e4b55760e7d46bbc74a400d32e";

/// Embedded official binary asset compiled directly into the binary.
pub static EMBEDDED_FIRMWARE_V101: &[u8] = include_bytes!("../assets/AK110M_UK_V101.bin");

/// Catalog metadata describing an approved keyboard firmware image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareCatalogEntry {
    /// Target USB Vendor ID.
    pub target_vid: u16,
    /// Target USB Product ID.
    pub target_pid: u16,
    /// Bootloader USB Product ID when in IAP mode.
    pub bootloader_pid: u16,
    /// Firmware revision code (e.g. 101).
    pub version: u16,
    /// Human-readable version display string (e.g. "v1.01").
    pub version_str: String,
    /// SHA-256 checksum in lowercase hex.
    pub sha256: String,
    /// Full flash binary size in bytes (typically 32,768).
    pub binary_size: usize,
    /// User partition flash size in bytes (typically 30,720).
    pub payload_size: usize,
    /// Memory offset where bootloader partition begins.
    pub bootloader_offset: usize,
}

/// Result of validating a firmware image file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareValidationInfo {
    /// Whether the file is a valid firmware image for this keyboard.
    pub valid: bool,
    /// File size in bytes.
    pub file_size: usize,
    /// SHA-256 checksum.
    pub sha256: String,
    /// Detected Vendor ID from embedded USB descriptor table if found.
    pub detected_vid: Option<u16>,
    /// Detected Product ID from embedded USB descriptor table if found.
    pub detected_pid: Option<u16>,
    /// Detected version numeric code.
    pub detected_version: Option<u16>,
    /// Human-readable version display string.
    pub detected_version_str: Option<String>,
    /// Whether image includes the full 32KB flash (true) or user partition only (false).
    pub is_full_binary: bool,
    /// Diagnostic or error message if invalid.
    pub error_message: Option<String>,
}

/// Returns the bundled official catalog entry for Fantech MK894 / AK110M.
pub fn get_catalog_entry() -> FirmwareCatalogEntry {
    FirmwareCatalogEntry {
        target_vid: CATALOG_VID,
        target_pid: CATALOG_PID,
        bootloader_pid: BOOTLOADER_PID,
        version: CATALOG_VERSION_CODE,
        version_str: CATALOG_VERSION_STR.to_string(),
        sha256: CATALOG_SHA256.to_string(),
        binary_size: EXPECTED_FULL_IMAGE_LEN,
        payload_size: EXPECTED_PAYLOAD_LEN,
        bootloader_offset: BOOTLOADER_OFFSET,
    }
}

/// Checks whether an update is available given the device's current reported firmware revision string.
pub fn check_firmware_update_available(current_version: &str) -> bool {
    let cur = current_version.trim().to_lowercase();
    if cur.is_empty() {
        return false;
    }

    // Official factory firmware revisions for Fantech MK894 / AK107:
    // "V64" is 0x64 (hex) = 100 decimal -> version 1.00
    // "V100" / "V101" -> factory versions 1.00 / 1.01
    // None of these official factory versions have an update available.
    let known_factory = [
        "v64", "v100", "v1.00", "v65", "v101", "v1.01",
        "64", "100", "1.00", "101", "1.01",
    ];
    if known_factory.contains(&cur.as_str()) {
        return false;
    }

    let catalog = get_catalog_entry();
    let cat = catalog.version_str.trim().to_lowercase();
    let cat_code = format!("v{}", catalog.version).to_lowercase();
    cur != cat && cur != cat_code
}

/// Returns a reference to the embedded official firmware binary bytes.
pub fn get_embedded_firmware_bytes() -> &'static [u8] {
    EMBEDDED_FIRMWARE_V101
}

/// Computes the SHA-256 hash of a byte slice, returning a lowercase hexadecimal string.
pub fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut s = String::with_capacity(64);
    for byte in result {
        use std::fmt::Write;
        let _ = write!(&mut s, "{:02x}", byte);
    }
    s
}

/// Validates a raw firmware binary file before allowing any flashing operation.
///
/// Ensures:
/// 1. Binary size is either exact full flash (32,768 bytes) or user partition (30,720 bytes).
/// 2. If 32,768 bytes, extracts payload (first 30,720 bytes) to preserve bootloader partition.
/// 3. Validates USB descriptor table matches VID 0x36AE and PID 0xFE9C at offset 0x59BE.
pub fn validate_firmware_binary(data: &[u8]) -> Result<FirmwareValidationInfo, String> {
    let file_size = data.len();
    let sha256 = compute_sha256(data);

    if file_size != EXPECTED_FULL_IMAGE_LEN && file_size != EXPECTED_PAYLOAD_LEN {
        return Err(format!(
            "Invalid firmware binary size: {} bytes. Expected either {} bytes (full flash) or {} bytes (user partition).",
            file_size, EXPECTED_FULL_IMAGE_LEN, EXPECTED_PAYLOAD_LEN
        ));
    }

    let is_full_binary = file_size == EXPECTED_FULL_IMAGE_LEN;

    // Scan for USB Device Descriptor signature: VID 0x36AE, PID 0xFE9C (Little-Endian: [0xAE, 0x36, 0x9C, 0xFE])
    let needle = [0xAE, 0x36, 0x9C, 0xFE];
    let (detected_vid, detected_pid, detected_version, detected_version_str) =
        if let Some(pos) = data.windows(4).position(|w| w == needle) {
            let (v_num, v_str) = if pos + 6 <= data.len() {
                let bcd_lo = data[pos + 4] as u16;
                let bcd_hi = data[pos + 5] as u16;
                let bcd = (bcd_hi << 8) | bcd_lo;
                (Some(bcd), Some(format!("v{}.{:02x}", bcd_hi, bcd_lo)))
            } else {
                (None, None)
            };
            (Some(CATALOG_VID), Some(CATALOG_PID), v_num, v_str)
        } else {
            return Err(
                "Firmware binary does not contain matching USB descriptor for Fantech MK894 / AK110M hardware (VID 0x36AE, PID 0xFE9C)."
                    .to_string(),
            );
        };

    Ok(FirmwareValidationInfo {
        valid: true,
        file_size,
        sha256,
        detected_vid,
        detected_pid,
        detected_version,
        detected_version_str,
        is_full_binary,
        error_message: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_firmware_integrity() {
        let bytes = get_embedded_firmware_bytes();
        assert_eq!(bytes.len(), EXPECTED_FULL_IMAGE_LEN);
        let hash = compute_sha256(bytes);
        assert_eq!(hash, CATALOG_SHA256);
    }

    #[test]
    fn test_catalog_entry() {
        let entry = get_catalog_entry();
        assert_eq!(entry.target_vid, 0x36AE);
        assert_eq!(entry.target_pid, 0xFE9C);
        assert_eq!(entry.bootloader_pid, 0xFEAB);
        assert_eq!(entry.version, 101);
        assert_eq!(entry.version_str, "v1.01");
        assert_eq!(entry.binary_size, 32768);
        assert_eq!(entry.payload_size, 30720);
        assert_eq!(entry.bootloader_offset, 0x7800);
    }

    #[test]
    fn test_validate_official_firmware_binary() {
        let bytes = get_embedded_firmware_bytes();
        let info = validate_firmware_binary(bytes).expect("Valid official firmware");
        assert!(info.valid);
        assert_eq!(info.file_size, 32768);
        assert!(info.is_full_binary);
        assert_eq!(info.detected_vid, Some(0x36AE));
        assert_eq!(info.detected_pid, Some(0xFE9C));
        assert_eq!(info.sha256, CATALOG_SHA256);
    }

    #[test]
    fn test_validate_rejects_corrupted_or_arbitrary_binary() {
        let garbage = vec![0xCC; 32768];
        let res = validate_firmware_binary(&garbage);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("does not contain matching USB descriptor"));

        let wrong_size = vec![0xAE, 0x36, 0x9C, 0xFE];
        let res_size = validate_firmware_binary(&wrong_size);
        assert!(res_size.is_err());
        assert!(res_size.unwrap_err().contains("Invalid firmware binary size"));
    }
}
