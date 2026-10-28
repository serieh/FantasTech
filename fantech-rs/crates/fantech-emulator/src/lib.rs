//! Virtual hardware emulator simulating Fantech MK894 firmware for offline testing.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

use fantech_protocol::constants::{opcodes, KEY_COUNT, MACRO_MEMORY_SIZE, PROTOCOL_MAGIC, REPORT_LENGTH};
use fantech_protocol::lighting::{LightingMode, RgbColor};
use fantech_protocol::macro_mem::MacroMemoryTable;
use fantech_transport::{MockTransport, Transport, TransportError};

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
    /// Onboard 416-byte keymap table (104 keys * 4 bytes).
    pub keymap_table: [u8; KEY_COUNT * 4],
}

impl Default for VirtualKeyboardState {
    fn default() -> Self {
        Self {
            active_profile: 0,
            lighting_mode: LightingMode::Spectrum,
            brightness: 100,
            speed: 50,
            direction: 0,
            matrix_colors: [RgbColor::new(255, 0, 0); KEY_COUNT],
            macro_memory: MacroMemoryTable::new_empty(),
            keymap_table: [0u8; KEY_COUNT * 4],
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
                resp[8] = (self.state.brightness / 25).min(4);
                resp[9] = (self.state.speed / 25).min(4);
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
            opcodes::SELECT_PROFILE => {
                if report[2] <= 2 {
                    self.state.active_profile = report[2];
                }
            }
            opcodes::SET_SINGLE_KEYMAP => {
                let offset = u16::from_le_bytes([report[3], report[4]]) as usize;
                if offset + 4 <= self.state.keymap_table.len() {
                    self.state.keymap_table[offset..offset + 4].copy_from_slice(&report[8..12]);
                }
            }
            opcodes::SET_MACRO_CHUNK => {
                let offset = u16::from_le_bytes([report[3], report[4]]) as usize;
                let chunk_len = report[2] as usize;
                if offset + chunk_len <= MACRO_MEMORY_SIZE {
                    let _ = self.state.macro_memory.write_bytes(offset.max(64), &report[5..5 + chunk_len]);
                }
            }
            opcodes::RESTORE_FACTORY => {
                if report[2] == 0xFF {
                    self.state = VirtualKeyboardState::default();
                } else if report[2] == 0x04 {
                    self.state.macro_memory = MacroMemoryTable::new_empty();
                }
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
    use fantech_protocol::commands::build_device_info_query;

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
        use fantech_protocol::lighting::{build_lighting_report, LightingConfig, LightingZone, AnimationDirection, ColorMode};

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
}
