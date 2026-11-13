//! Virtual hardware emulator simulating Fantech MK894 firmware for offline testing.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

use fantastech_core::default_layer_keymap;
use fantastech_protocol::constants::{opcodes, KEY_COUNT, SPARSE_KEY_COUNT, MACRO_MEMORY_SIZE, PROTOCOL_MAGIC, REPORT_LENGTH};
use fantastech_protocol::keymap::{KeyAction, KeyLayer};
use fantastech_protocol::lighting::{LightingMode, RgbColor};
use fantastech_protocol::macro_mem::MacroMemoryTable;
use fantastech_transport::{MockTransport, Transport, TransportError};

/// Internal simulated hardware state of the MK894 keyboard.
pub struct VirtualKeyboardState {
    /// Active profile index (0..2).
    pub active_profile: u8,
    /// Active lighting mode.
    pub lighting_mode: LightingMode,
    /// Active brightness (0..100).
    pub brightness: u8,
    /// Active speed (0..100).
    pub speed: u8,
    /// Active animation direction (0 = left/up, 1 = right/down).
    pub direction: u8,
    /// Per-key RGB matrix canvas.
    pub matrix_colors: [RgbColor; KEY_COUNT],
    /// Onboard 4KB macro memory table.
    pub macro_memory: MacroMemoryTable,
    /// Onboard keymap table across 3 profiles and 2 layers (Base=0, Fn=1).
    pub keymap_profiles: [[[KeyAction; SPARSE_KEY_COUNT]; 2]; 3],
    /// Legacy raw keymap table buffer.
    pub keymap_table: [u8; SPARSE_KEY_COUNT * 4],
    /// Auto-sleep timeout in seconds.
    pub sleep_timeout_seconds: u16,
}

impl Default for VirtualKeyboardState {
    fn default() -> Self {
        let base = default_layer_keymap(KeyLayer::Base);
        let fn_layer = default_layer_keymap(KeyLayer::Fn);
        let keymap_profiles = [
            [base, fn_layer],
            [base, fn_layer],
            [base, fn_layer],
        ];
        Self {
            active_profile: 0,
            lighting_mode: LightingMode::Spectrum,
            brightness: 100,
            speed: 50,
            direction: 0,
            matrix_colors: [RgbColor::new(255, 0, 0); KEY_COUNT],
            macro_memory: MacroMemoryTable::new_empty(),
            keymap_profiles,
            keymap_table: [0u8; SPARSE_KEY_COUNT * 4],
            sleep_timeout_seconds: 0,
        }
    }
}

/// The virtual keyboard emulator implementing `Transport`.
pub struct VirtualKeyboardEmulator {
    state: VirtualKeyboardState,
    mock: MockTransport,
}

impl Default for VirtualKeyboardEmulator {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualKeyboardEmulator {
    /// Creates a new virtual keyboard emulator with factory default state.
    pub fn new() -> Self {
        Self {
            state: VirtualKeyboardState::default(),
            mock: MockTransport::new(),
        }
    }

    /// Accesses the internal hardware state.
    pub fn state(&self) -> &VirtualKeyboardState {
        &self.state
    }

    /// Accesses the internal hardware state mutably.
    pub fn state_mut(&mut self) -> &mut VirtualKeyboardState {
        &mut self.state
    }

    /// Processes an incoming host report and updates simulated firmware state.
    fn handle_host_report(&mut self, report: &[u8; REPORT_LENGTH]) {
        if report[0] != PROTOCOL_MAGIC {
            return;
        }

        let opcode = report[1];
        match opcode {
            opcodes::GET_DEVICE_INFO => {
                let mut resp = [0u8; REPORT_LENGTH];
                resp[0] = PROTOCOL_MAGIC;
                resp[1] = opcodes::GET_DEVICE_INFO;
                resp[2] = 40; // status/len
                resp[5..7].copy_from_slice(&101u16.to_le_bytes()); // version
                resp[7..9].copy_from_slice(&0xFE9Cu16.to_le_bytes()); // PID
                resp[9..11].copy_from_slice(&101u16.to_le_bytes()); // firmware
                resp[11] = 0; // workMode
                resp[12] = 1; // linkStatus
                resp[13] = 100; // battery
                resp[14] = 0; // charge
                resp[15] = 3; // profileCount
                resp[16] = self.state.active_profile; // active profile
                resp[17] = 2; // layerCount
                resp[18] = 0; // layer
                resp[21..26].copy_from_slice(b"AK107");
                self.mock.push_incoming(resp);
            }
            opcodes::GET_LIGHT_CONFIG => {
                let mut resp = [0u8; REPORT_LENGTH];
                resp[0] = 0xAA;
                resp[1] = opcodes::GET_LIGHT_CONFIG;
                resp[2] = 11;
                resp[5] = 1; // MainBacklight zone
                resp[7] = self.state.lighting_mode as u8;
                resp[8] = if self.state.brightness <= 4 { self.state.brightness } else { (self.state.brightness / 25).min(4) };
                resp[9] = if self.state.speed <= 4 { self.state.speed } else { (self.state.speed / 25).min(4) };
                resp[10] = self.state.direction;
                resp[11] = 0; // color_mode
                resp[13] = 0; // H
                resp[14] = 255; // S
                resp[15] = 255; // V
                self.mock.push_incoming(resp);
            }
            opcodes::SET_LIGHT_CONFIG => {
                if let Ok(mode) = LightingMode::from_u8(report[7]) {
                    self.state.lighting_mode = mode;
                }
                self.state.brightness = report[8];
                self.state.speed = report[9];
                self.state.direction = report[10];
            }
            opcodes::SET_SINGLE_KEY_LED => {
                let offset = u16::from_le_bytes([report[3], report[4]]) as usize;
                let key_idx = offset / 3;
                if key_idx < KEY_COUNT {
                    self.state.matrix_colors[key_idx] = RgbColor::new(report[8], report[9], report[10]);
                }
            }
            opcodes::STREAM_RGB_MATRIX => {
                let offset = u16::from_le_bytes([report[3], report[4]]) as usize;
                let byte_len = report[2] as usize;
                let payload = &report[8..8 + byte_len.min(56)];
                let start_key = offset / 3;
                #[allow(clippy::chunks_exact_to_as_chunks)]
                for (i, chunk) in payload.chunks_exact(3).enumerate() {
                    let key = start_key + i;
                    if key < KEY_COUNT {
                        self.state.matrix_colors[key] = RgbColor::new(chunk[0], chunk[1], chunk[2]);
                    }
                }
            }
            opcodes::SELECT_PROFILE => {
                if report[2] <= 2 {
                    self.state.active_profile = report[2];
                }
            }
            opcodes::SET_SINGLE_KEYMAP => {
                let offset = u16::from_le_bytes([report[3], report[4]]) as usize;
                let key_idx = offset / 4;
                let layer = (report[6] as usize).min(1);
                let prof = (self.state.active_profile as usize).min(2);
                if let Ok(action) = KeyAction::from_bytes([report[8], report[9], report[10], report[11]]) {
                    if key_idx < SPARSE_KEY_COUNT {
                        self.state.keymap_profiles[prof][layer][key_idx] = action;
                        if offset + 4 <= self.state.keymap_table.len() {
                            self.state.keymap_table[offset..offset + 4].copy_from_slice(&report[8..12]);
                        }
                    }
                }
            }
            opcodes::SET_KEYMAP_CHUNK => {
                let offset = u16::from_le_bytes([report[3], report[4]]) as usize;
                let layer = (report[6] as usize).min(1);
                let prof = (self.state.active_profile as usize).min(2);
                let start_slot = offset / 4;
                for i in 0..14 {
                    let slot = start_slot + i;
                    if slot < SPARSE_KEY_COUNT {
                        let byte_offset = 8 + i * 4;
                        if let Ok(action) = KeyAction::from_bytes([
                            report[byte_offset],
                            report[byte_offset + 1],
                            report[byte_offset + 2],
                            report[byte_offset + 3],
                        ]) {
                            self.state.keymap_profiles[prof][layer][slot] = action;
                            let k_off = slot * 4;
                            if k_off + 4 <= self.state.keymap_table.len() {
                                self.state.keymap_table[k_off..k_off + 4].copy_from_slice(&report[byte_offset..byte_offset + 4]);
                            }
                        }
                    }
                }
            }
            opcodes::GET_KEYMAP_CHUNK => {
                let offset = u16::from_le_bytes([report[3], report[4]]) as usize;
                let layer = (report[6] as usize).min(1);
                let prof = (self.state.active_profile as usize).min(2);
                let mut resp = [0u8; REPORT_LENGTH];
                resp[0] = PROTOCOL_MAGIC;
                resp[1] = opcodes::GET_KEYMAP_CHUNK;
                resp[2] = 58;
                resp[3] = report[3];
                resp[4] = report[4];
                resp[5] = 0x00;
                resp[6] = layer as u8;
                resp[7] = 0x00;
                let start_slot = offset / 4;
                for i in 0..14 {
                    let slot = start_slot + i;
                    if slot < SPARSE_KEY_COUNT {
                        let b = self.state.keymap_profiles[prof][layer][slot].to_bytes();
                        resp[8 + i * 4..8 + i * 4 + 4].copy_from_slice(&b);
                    }
                }
                self.mock.push_incoming(resp);
            }
            opcodes::GET_MACRO_CHUNK => {
                let offset = u16::from_le_bytes([report[3], report[4]]) as usize;
                let mut resp = [0u8; REPORT_LENGTH];
                resp[0] = PROTOCOL_MAGIC;
                resp[1] = opcodes::GET_MACRO_CHUNK;
                resp[2] = report[2];
                resp[3] = report[3];
                resp[4] = report[4];
                let end = (offset + 56).min(MACRO_MEMORY_SIZE);
                if offset < MACRO_MEMORY_SIZE {
                    let len = end - offset;
                    resp[8..8 + len].copy_from_slice(&self.state.macro_memory.as_bytes()[offset..end]);
                }
                self.mock.push_incoming(resp);
            }
            opcodes::SET_MACRO_CHUNK => {
                let offset = u16::from_le_bytes([report[3], report[4]]) as usize;
                let chunk_len = (report[2] as usize).min(59);
                if offset + chunk_len <= MACRO_MEMORY_SIZE {
                    let _ = self.state.macro_memory.write_bytes(offset, &report[5..5 + chunk_len]);
                }
            }
            opcodes::RESTORE_FACTORY => {
                if report[2] == 0xFF {
                    self.state = VirtualKeyboardState::default();
                } else if report[2] == 0x04 {
                    self.state.macro_memory = MacroMemoryTable::new_empty();
                }
            }
            opcodes::SET_SLEEP_TIMEOUT => {
                let seconds = u16::from_le_bytes([report[5], report[6]]);
                self.state.sleep_timeout_seconds = seconds;
            }
            _ => {}
        }
    }
}

impl Transport for VirtualKeyboardEmulator {
    fn read_report(&mut self, buf: &mut [u8; REPORT_LENGTH]) -> Result<usize, TransportError> {
        self.mock.read_report(buf)
    }

    fn write_report(&mut self, buf: &[u8; REPORT_LENGTH]) -> Result<(), TransportError> {
        self.handle_host_report(buf);
        self.mock.write_report(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fantastech_protocol::commands::build_device_info_query;

    #[test]
    fn test_virtual_emulator_device_info_handshake() {
        let mut emu = VirtualKeyboardEmulator::new();
        let query = build_device_info_query().unwrap();
        emu.write_report(query.as_bytes()).unwrap();

        let mut resp = [0u8; REPORT_LENGTH];
        let n = emu.read_report(&mut resp).unwrap();
        assert_eq!(n, REPORT_LENGTH);
        assert_eq!(resp[0], 0x06);
        assert_eq!(resp[1], 0x05);
        assert_eq!(&resp[21..26], b"AK107");
    }

    #[test]
    fn test_virtual_emulator_lighting_direction_and_color() {
        use fantastech_protocol::lighting::{build_lighting_report, LightingConfig, LightingZone, AnimationDirection, ColorMode};

        let mut emu = VirtualKeyboardEmulator::new();
        let config = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode: LightingMode::StreamLeftToRight,
            brightness: 4,
            speed: 3,
            direction: AnimationDirection::LeftToRight,
            color_mode: ColorMode::FixedColor,
            color: RgbColor::new(0, 255, 0),
        };
        let report = build_lighting_report(&config).unwrap();
        emu.write_report(report.as_bytes()).unwrap();

        assert_eq!(emu.state().lighting_mode, LightingMode::StreamLeftToRight);
        assert_eq!(emu.state().brightness, 4);
        assert_eq!(emu.state().speed, 3);
        assert_eq!(emu.state().direction, 1);
    }

    #[test]
    fn test_virtual_emulator_sleep_timeout_and_factory_reset() {
        use fantastech_protocol::commands::{build_auto_sleep_timeout, build_factory_reset_full};

        let mut emu = VirtualKeyboardEmulator::new();

        // Test sleep timeout
        let sleep_rep = build_auto_sleep_timeout(1800).unwrap();
        emu.write_report(sleep_rep.as_bytes()).unwrap();
        assert_eq!(emu.state().sleep_timeout_seconds, 1800);

        // Modify some state then test factory reset
        emu.state_mut().active_profile = 2;
        let reset_rep = build_factory_reset_full().unwrap();
        emu.write_report(reset_rep.as_bytes()).unwrap();
        assert_eq!(emu.state().active_profile, 0);
        assert_eq!(emu.state().sleep_timeout_seconds, 0);
    }
}
