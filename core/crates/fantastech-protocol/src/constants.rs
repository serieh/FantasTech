//! Constants and wire-level protocol identifiers for the Fantech MK894 Maxpower II.

/// Standard Vendor ID for SDCX OEM.
pub const VENDOR_ID: u16 = 0x36AE;

/// Primary Product ID for MK894 / AK107 (104-key ANSI RGB).
pub const PRODUCT_ID_MK894: u16 = 0xFE9C;

/// Secondary Product ID for variant model AK110-DS.
pub const PRODUCT_ID_AK110_DS: u16 = 0xFEAB;

/// Product ID when keyboard re-enumerates in IAP bootloader recovery mode.
pub const PRODUCT_ID_BOOTLOADER: u16 = 0xFEAB;

/// Vendor Usage Page for configuration interface.
pub const USAGE_PAGE_VENDOR: u16 = 0xFF00;

/// Vendor Usage ID for MK894 configuration interface.
pub const USAGE_VENDOR_CONTROL: u16 = 0x0002;

/// Fixed length of all USB Output/Feature Reports in bytes over the wire.
pub const REPORT_LENGTH: usize = 64;

/// Protocol command magic signature required by keyboard firmware (Byte 0).
pub const PROTOCOL_MAGIC: u8 = 0x06;

/// Total number of physically addressable keys on standard MK894 matrix.
pub const KEY_COUNT: usize = 104;

/// Maximum sparse matrix key index addressable by hardware (0..126).
pub const MAX_KEY_INDEX: usize = 126;

/// Total number of sparse matrix slots in hardware layout grid (6 rows * 21 cols).
pub const SPARSE_KEY_COUNT: usize = 126;

/// Total size of per-key RGB canvas in bytes (126 sparse keys * 3 RGB bytes = 378 bytes).
pub const MATRIX_RGB_BYTE_COUNT: usize = 378;

/// Size of the autonomous onboard hardware macro memory in bytes (4 KB).
pub const MACRO_MEMORY_SIZE: usize = 4096;

/// Maximum number of hardware macro slots supported in header table.
pub const MACRO_SLOT_COUNT: usize = 32;

/// Opcode identifiers for hardware commands (Byte 1).
pub mod opcodes {
    pub const GET_DEVICE_INFO: u8 = 0x05;
    pub const GET_LAYOUT_BYTES: u8 = 0x06;
    pub const GET_KEYMAP_CHUNK: u8 = 0x08;
    pub const SET_KEYMAP_CHUNK: u8 = 0x09;
    pub const GET_LIGHT_CONFIG: u8 = 0x0A;
    pub const SET_LIGHT_CONFIG: u8 = 0x0B;
    pub const GET_MACRO_CHUNK: u8 = 0x0C;
    pub const SET_MACRO_CHUNK: u8 = 0x0D;
    pub const RESTORE_FACTORY: u8 = 0x0F;
    pub const SET_SINGLE_KEYMAP: u8 = 0x10;
    pub const STREAM_RGB_MATRIX: u8 = 0x12;
    pub const SET_SINGLE_KEY_LED: u8 = 0x14;
    pub const SELECT_PROFILE: u8 = 0xFB;
    pub const SET_SLEEP_TIMEOUT: u8 = 0xFC;

    /// Dangerous opcode that triggers bootloader mode; permanently banned in safe driver.
    pub const BANNED_JUMP_BOOTLOADER: u8 = 0x5A;

    /// Bootloader flashing opcode; permanently banned in safe driver.
    pub const BANNED_ISP_FLASH: u8 = 0x55;
}
