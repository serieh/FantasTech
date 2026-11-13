//! Pure clean-room protocol serializer, parser, and wire framing for the Fantech MK894 Maxpower II.
//!
//! Enforces zero-unsafe code, fixed 64-byte USB Output Report framing, parameter bounds validation,
//! and strict rejection of banned bootloader opcodes.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

pub mod commands;
pub mod constants;
pub mod error;
pub mod firmware;
pub mod keymap;
pub mod lighting;
pub mod macro_mem;
pub mod openrgb;
pub mod report;

pub use commands::*;
pub use constants::*;
pub use error::ProtocolError;
pub use firmware::*;
pub use keymap::*;
pub use lighting::*;
pub use macro_mem::*;
pub use openrgb::*;
pub use report::ReportBuffer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_buffer_framing() {
        let r = ReportBuffer::new(opcodes::GET_DEVICE_INFO);
        assert_eq!(r.magic(), 0x06);
        assert_eq!(r.opcode(), 0x05);
        assert_eq!(r.as_bytes().len(), 64);
        assert_eq!(r.as_bytes()[0], 0x06);
        assert_eq!(r.as_bytes()[1], 0x05);
        for &byte in &r.as_bytes()[2..] {
            assert_eq!(byte, 0x00);
        }
    }

    #[test]
    fn test_openrgb_packet_header_roundtrip() {
        let hdr = OpenRgbHeader::new(1, NET_PACKET_ID_REQUEST_CONTROLLER_DATA, 128);
        let bytes = hdr.to_bytes();
        let parsed = OpenRgbHeader::from_bytes(&bytes).expect("parse openrgb header");
        assert_eq!(parsed.magic, *OPENRGB_MAGIC);
        assert_eq!(parsed.dev_id, 1);
        assert_eq!(parsed.pkt_id, NET_PACKET_ID_REQUEST_CONTROLLER_DATA);
        assert_eq!(parsed.pkt_size, 128);
    }

    #[test]
    fn test_openrgb_controller_data_builder() {
        let keys = &[("Esc", 0u8, 0u8, 0u8), ("Space", 5u8, 6u8, 111u8)];
        let colors = &[RgbColor::new(255, 0, 0), RgbColor::new(0, 255, 0)];
        let data = build_openrgb_controller_data(keys, colors, 16, 100, 0);
        assert!(data.len() > 100);
        let data_size = u32::from_le_bytes(data[0..4].try_into().unwrap());
        assert_eq!(data_size as usize, data.len());

        // Single LED parse test
        let mut single_buf = vec![0u8; 8];
        single_buf[0..4].copy_from_slice(&42i32.to_le_bytes());
        single_buf[4] = 255;
        single_buf[5] = 128;
        single_buf[6] = 64;
        let (led_idx, col) = parse_openrgb_update_single_led(&single_buf).expect("parse single led");
        assert_eq!(led_idx, 42);
        assert_eq!(col, RgbColor::new(255, 128, 64));

        // Set mode parse test
        let mut mode_buf = vec![0u8; 8];
        mode_buf[0..4].copy_from_slice(&8u32.to_le_bytes()); // data_size
        mode_buf[4..8].copy_from_slice(&3i32.to_le_bytes()); // mode_idx
        let mode_parsed = parse_openrgb_set_mode(&mode_buf).expect("parse set mode");
        assert_eq!(mode_parsed.mode_idx, 3);

        // Brightness normalization and RGB scaling tests
        use crate::openrgb::{normalize_brightness_to_levels, normalize_brightness_to_percent, scale_rgb};
        assert_eq!(normalize_brightness_to_percent(0), 0);
        assert_eq!(normalize_brightness_to_percent(1), 25);
        assert_eq!(normalize_brightness_to_percent(2), 50);
        assert_eq!(normalize_brightness_to_percent(3), 75);
        assert_eq!(normalize_brightness_to_percent(4), 100);
        assert_eq!(normalize_brightness_to_percent(50), 50);
        assert_eq!(normalize_brightness_to_percent(100), 100);

        assert_eq!(normalize_brightness_to_levels(0), 0);
        assert_eq!(normalize_brightness_to_levels(1), 1);
        assert_eq!(normalize_brightness_to_levels(2), 2);
        assert_eq!(normalize_brightness_to_levels(3), 3);
        assert_eq!(normalize_brightness_to_levels(4), 4);
        assert_eq!(normalize_brightness_to_levels(25), 1);
        assert_eq!(normalize_brightness_to_levels(50), 2);
        assert_eq!(normalize_brightness_to_levels(75), 3);
        assert_eq!(normalize_brightness_to_levels(100), 4);

        let red = RgbColor::new(200, 100, 50);
        assert_eq!(scale_rgb(red, 100), RgbColor::new(200, 100, 50));
        assert_eq!(scale_rgb(red, 50), RgbColor::new(100, 50, 25));
        assert_eq!(scale_rgb(red, 25), RgbColor::new(50, 25, 12));
        assert_eq!(scale_rgb(red, 0), RgbColor::new(0, 0, 0));
    }

    #[test]
    fn test_banned_opcode_rejection() {
        assert!(assert_opcode_safe(opcodes::SET_LIGHT_CONFIG).is_ok());
        assert!(assert_opcode_safe(opcodes::BANNED_JUMP_BOOTLOADER).is_err());
        assert!(assert_opcode_safe(opcodes::BANNED_ISP_FLASH).is_err());
    }

    #[test]
    fn test_lighting_modes_roundtrip() {
        let config = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode: LightingMode::Static,
            brightness: 80,
            speed: 50,
            direction: AnimationDirection::Default,
            color_mode: ColorMode::FixedColor,
            color: RgbColor::new(255, 0, 0),
        };

        let report = build_lighting_report(&config).expect("build lighting report");
        let bytes = report.as_bytes();
        assert_eq!(bytes[0], 0x06);
        assert_eq!(bytes[1], opcodes::SET_LIGHT_CONFIG);
        assert_eq!(bytes[2], 11);   // Length
        assert_eq!(bytes[5], 0x01); // Main zone (1)
        assert_eq!(bytes[7], 1);    // Static mode
        assert_eq!(bytes[8], 3);    // Brightness mapped to discrete level 0..4 (80% -> 3)
        assert_eq!(bytes[9], 2);    // Speed mapped to discrete level 0..4 (50% -> 2)
        assert_eq!(bytes[11], 1);   // Fixed color

        // Verify discrete level mapping (0..4) preserved directly for MCU (EV-031)
        let discrete_cfg = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode: LightingMode::Breathing,
            brightness: 4,
            speed: 3,
            direction: AnimationDirection::Default,
            color_mode: ColorMode::DynamicSpectrum,
            color: RgbColor::new(0, 255, 0),
        };
        let discrete_report = build_lighting_report(&discrete_cfg).expect("build discrete");
        assert_eq!(discrete_report.as_bytes()[8], 4); // Level 4 preserved directly
        assert_eq!(discrete_report.as_bytes()[9], 3); // Level 3 preserved directly
    }

    #[test]
    fn test_lighting_query_and_response_parsing() {
        let query = build_light_query_report().expect("build query");
        assert_eq!(query.opcode(), opcodes::GET_LIGHT_CONFIG);
        assert_eq!(query.as_bytes()[0], 0x06);

        // Simulated device response: [AA, 0A, 0B, 00, 00, 01, 00, 04, 04, 02, 00, 00, 00, 00, FF, FF, ...]
        let mut resp_raw = [0u8; REPORT_LENGTH];
        resp_raw[0] = 0xAA;
        resp_raw[1] = opcodes::GET_LIGHT_CONFIG;
        resp_raw[2] = 11;
        resp_raw[5] = 0x01; // Main zone
        resp_raw[7] = 4;    // Spectrum mode
        resp_raw[8] = 4;    // Brightness 4 (100%)
        resp_raw[9] = 2;    // Speed 2 (50%)
        resp_raw[11] = 0;   // Dynamic spectrum
        resp_raw[13] = 0;   // H = 0
        resp_raw[14] = 255; // S = 255
        resp_raw[15] = 255; // V = 255

        let parsed = parse_light_config_response(&ReportBuffer::from_raw(resp_raw).unwrap())
            .expect("parse response");
        assert_eq!(parsed.zone, LightingZone::MainBacklight);
        assert_eq!(parsed.mode, LightingMode::Spectrum);
        assert_eq!(parsed.brightness, 4);
        assert_eq!(parsed.speed, 2);
        assert_eq!(parsed.color_mode, ColorMode::DynamicSpectrum);
        assert_eq!(parsed.color.r, 255);
        assert_eq!(parsed.color.g, 0);
        assert_eq!(parsed.color.b, 0);
    }

    #[test]
    fn test_single_key_color_bounds() {
        let spacebar = build_single_key_color(111, RgbColor::new(0, 255, 0));
        assert!(spacebar.is_ok());

        let max_key = build_single_key_color(126, RgbColor::new(0, 255, 0));
        assert!(max_key.is_ok());

        let out_of_bounds = build_single_key_color(127, RgbColor::new(0, 255, 0));
        assert!(out_of_bounds.is_err());
    }

    #[test]
    fn test_matrix_canvas_streaming_packets() {
        let mut matrix = [RgbColor::default(); SPARSE_KEY_COUNT];
        matrix[0] = RgbColor::new(255, 0, 0);
        matrix[125] = RgbColor::new(0, 0, 255);

        let packets = build_matrix_packets(&matrix).expect("build matrix packets");
        assert_eq!(packets.len(), 7); // Exactly 7 chunks of 54 bytes = 378 bytes (18 keys per chunk)

        // Packet 0 checks
        assert_eq!(packets[0].magic(), 0x06);
        assert_eq!(packets[0].opcode(), opcodes::STREAM_RGB_MATRIX);
        assert_eq!(packets[0].as_bytes()[2], 57); // Length: 54 + 3 = 57
        assert_eq!(packets[0].as_bytes()[3], 0);  // Offset lo
        assert_eq!(packets[0].as_bytes()[4], 0);  // Offset hi
        assert_eq!(packets[0].as_bytes()[8], 255); // First key R
        assert_eq!(packets[0].as_bytes()[9], 0);   // First key G
        assert_eq!(packets[0].as_bytes()[10], 0);  // First key B

        // Verify that every single packet has an exact 54-byte chunk and offset aligned to a 3-byte RGB boundary
        for (c, pkt) in packets.iter().enumerate() {
            let offset = (pkt.as_bytes()[3] as usize) | ((pkt.as_bytes()[4] as usize) << 8);
            assert_eq!(offset, c * 54);
            assert_eq!(offset % 3, 0, "Packet {} offset {} must align to 3-byte RGB boundary", c, offset);
            assert_eq!(pkt.as_bytes()[2], 57, "Packet {} header length descriptor must be 57", c);
        }
    }

    #[test]
    fn test_single_key_remap_encoding() {
        let action = KeyAction::standard_key(0x04); // Usage ID 0x04 = 'A'
        let report = build_single_remap_report(15, KeyLayer::Base, &action).expect("build remap");
        let bytes = report.as_bytes();

        assert_eq!(bytes[0], 0x06);
        assert_eq!(bytes[1], opcodes::SET_SINGLE_KEYMAP);
        assert_eq!(bytes[2], 0x07); // Length descriptor
        assert_eq!(bytes[3], 60);   // Offset 15 * 4 = 60
        assert_eq!(bytes[4], 0);    // Offset high byte
        assert_eq!(bytes[6], 0);    // Base layer
        assert_eq!(bytes[8], 32);   // Standard HID key
        assert_eq!(bytes[9], 0x00); // Modifier mask (0 = None)
        assert_eq!(bytes[10], 0x04); // Scancode 'A'
    }

    #[test]
    fn test_combo_and_win_lock_encoding() {
        // Combo key: Ctrl + Shift + C (modifiers = 0x01 | 0x02 = 0x03, scancode = 0x06)
        let combo = KeyAction::combo(0x03, 0x06);
        assert_eq!(combo.category, ActionCategory::StandardHidKey);
        assert_eq!(combo.code1, 0x03);
        assert_eq!(combo.code2, 0x06);
        let b = combo.to_bytes();
        assert_eq!(b, [32, 0x03, 0x06, 0]);

        // WinLock action: Type 31, code1 = 0x07
        let wl = KeyAction::win_lock();
        assert_eq!(wl.category, ActionCategory::HardwareLighting);
        assert_eq!(wl.code1, 0x07);
        assert_eq!(wl.to_bytes(), [31, 0x07, 0, 0]);
    }

    #[test]
    fn test_batch_keymap_reports_504_bytes_9_packets() {
        let mut keymap = [KeyAction::disabled(); SPARSE_KEY_COUNT];
        keymap[0] = KeyAction::standard(0x29); // Esc
        keymap[111] = KeyAction::standard(0x2C); // Space (matrix index 111 >= 104)
        keymap[124] = KeyAction::standard(0x63); // Num . (matrix index 124 >= 104)

        let reports = build_batch_keymap_reports(KeyLayer::Base, &keymap).expect("build batch");
        assert_eq!(reports.len(), 9); // Exactly 9 packets

        // Verify each report header
        for (i, r) in reports.iter().enumerate() {
            let b = r.as_bytes();
            assert_eq!(b[0], 0x06);
            assert_eq!(b[1], opcodes::SET_KEYMAP_CHUNK);
            assert_eq!(b[2], 59); // 56 bytes payload + 3
            let expected_offset = (i as u16) * 56;
            assert_eq!(b[3], (expected_offset & 0xFF) as u8);
            assert_eq!(b[4], ((expected_offset >> 8) & 0xFF) as u8);
            assert_eq!(b[6], 0); // Base layer
        }

        // Verify key 0 (Esc) in chunk 0
        assert_eq!(reports[0].as_bytes()[8], 32);
        assert_eq!(reports[0].as_bytes()[10], 0x29);

        // Key 111 (Space): offset = 111 * 4 = 444.
        // 444 / 56 = chunk 7, remainder 444 - 7*56 = 444 - 392 = 52.
        // Byte in report = 8 + 52 = 60.
        assert_eq!(reports[7].as_bytes()[60], 32);
        assert_eq!(reports[7].as_bytes()[62], 0x2C);
    }

    #[test]
    fn test_macro_record_flags_and_roundtrip() {
        let record = MacroActionRecord {
            delay_ms: 150,
            is_press: true,
            is_end_of_macro: false,
            event_type: MacroEventType::KeyboardKey,
            code: 0x04,
        };

        let bytes = record.to_bytes();
        let decoded = MacroActionRecord::from_bytes(bytes).expect("decode record");
        assert_eq!(record, decoded);

        let end_record = MacroActionRecord {
            delay_ms: 25,
            is_press: false,
            is_end_of_macro: true,
            event_type: MacroEventType::KeyboardKey,
            code: 0x04,
        };

        let end_bytes = end_record.to_bytes();
        assert_eq!(end_bytes[2] & 0x80, 0x80); // End of macro bit set
        assert_eq!(end_bytes[2] & 0x40, 0x00); // Key up bit cleared
        let end_decoded = MacroActionRecord::from_bytes(end_bytes).expect("decode end record");
        assert_eq!(end_record, end_decoded);
    }

    #[test]
    fn test_macro_memory_table_pointers() {
        let mut table = MacroMemoryTable::new_empty();
        for i in 0..MACRO_SLOT_COUNT {
            assert_eq!(table.get_slot_pointer(i).unwrap(), 0xFFFF);
        }

        table.set_slot_pointer(0, 0x0040).unwrap();
        assert_eq!(table.get_slot_pointer(0).unwrap(), 0x0040);
    }

    #[test]
    fn test_macro_memory_unprogrammed_flash_decompile() {
        let mut table = MacroMemoryTable::new_empty();
        // Point slot 0 to an offset where data is all 0x00
        table.set_slot_pointer(0, 0x0040).unwrap();
        let res_zero = table.decompile_slot(0).expect("decompile zeroed memory");
        assert!(res_zero.is_empty());

        // Point slot 1 to an offset where data is all 0xFF (raw erased NOR flash)
        let mut table_ff = MacroMemoryTable::new_empty();
        table_ff.write_bytes(0x0040, &[0xFF; 16]).unwrap();
        table_ff.set_slot_pointer(1, 0x0040).unwrap();
        let res_ff = table_ff.decompile_slot(1).expect("decompile erased flash");
        assert!(res_ff.is_empty());
    }

    #[test]
    fn test_macro_trigger_wire_format() {
        let trig = KeyAction::macro_trigger(5, 3, 10);
        assert_eq!(trig.category, ActionCategory::HardwareMacro);
        assert_eq!(trig.code1, 5);  // slot
        assert_eq!(trig.code2, 10); // repeat count
        assert_eq!(trig.code3, 3);  // loop mode
        assert_eq!(trig.to_bytes(), [96, 5, 10, 3]);
    }

    #[test]
    fn test_openrgb_direction_and_all_modes() {
        use crate::openrgb::build_openrgb_controller_data;

        let keys = &[("Esc", 0u8, 0u8, 0u8)];
        let colors = &[RgbColor::new(0, 0, 0)];

        // Test controller data building with Mode 5 (Horizontal Wave)
        let data_lr = build_openrgb_controller_data(keys, colors, 5, 100, 0);
        assert!(data_lr.len() > 200);

        // Test controller data building with Mode 7 (Vertical Wave)
        let data_ud = build_openrgb_controller_data(keys, colors, 7, 100, 0);
        assert!(data_ud.len() > 200);

        // Ensure lighting animation modes do not advertise direction controls (not supported by hardware)
        assert!(!LightingMode::StreamLeftToRight.supports_direction());
        assert!(!LightingMode::StreamUpToDown.supports_direction());
        for mode_val in 0..=16 {
            if let Ok(mode) = LightingMode::from_u8(mode_val) {
                assert!(!mode.supports_direction());
            }
        }
    }

    #[test]
    fn test_auto_sleep_timeout_framing() {
        let rep = build_auto_sleep_timeout(300).expect("build sleep report");
        let bytes = rep.as_bytes();
        assert_eq!(bytes[0], 0x06); // REPORT_ID_CONFIG / PROTOCOL_MAGIC
        assert_eq!(bytes[1], 0xFC); // Opcode SET_SLEEP_TIMEOUT
        assert_eq!(bytes[2], 0x02); // Payload length
        assert_eq!(bytes[3], 0x00);
        assert_eq!(bytes[4], 0x00);
        assert_eq!(bytes[5], 0x2C); // 300 & 0xFF
        assert_eq!(bytes[6], 0x01); // 300 >> 8
        for &b in &bytes[7..] {
            assert_eq!(b, 0x00);
        }

        // Test with 0 (never sleep)
        let rep_zero = build_auto_sleep_timeout(0).expect("build 0 sleep report");
        let bytes_zero = rep_zero.as_bytes();
        assert_eq!(bytes_zero[1], 0xFC);
        assert_eq!(bytes_zero[5], 0x00);
        assert_eq!(bytes_zero[6], 0x00);
    }

    #[test]
    fn test_factory_reset_full_framing() {
        let rep = build_factory_reset_full().expect("build factory reset full");
        let bytes = rep.as_bytes();
        assert_eq!(bytes[0], 0x06);
        assert_eq!(bytes[1], 0x0F); // Opcode RESTORE_FACTORY
        assert_eq!(bytes[2], 0xFF); // Subcommand FULL_FACTORY_RESET
        for &b in &bytes[3..] {
            assert_eq!(b, 0x00);
        }
    }
}

