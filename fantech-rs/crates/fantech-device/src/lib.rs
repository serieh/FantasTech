//! Type-state device safety engine and auditable access control for Fantech MK894.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

use fantech_core::{get_mirror_key_index, KEY_DEFINITIONS};
use fantech_protocol::commands::{
    assert_opcode_safe, build_device_info_query, build_factory_reset, build_profile_select,
    parse_device_info_response, DeviceInfo,
};
use fantech_protocol::constants::{REPORT_LENGTH, SPARSE_KEY_COUNT};
use fantech_protocol::error::ProtocolError;
use fantech_protocol::keymap::{build_keymap_read_request, build_single_remap_report, KeyAction, KeyLayer};
use fantech_protocol::lighting::{
    build_lighting_report, build_matrix_packets, build_single_key_color, AnimationDirection, ColorMode, LightingConfig, LightingMode,
    LightingZone, RgbColor,
};
use fantech_protocol::macro_mem::{
    build_macro_read_request, build_macro_wipe_report, build_macro_write_chunk,
};
use fantech_protocol::report::ReportBuffer;
use fantech_transport::{Transport, TransportError};
use std::marker::PhantomData;
use thiserror::Error;

/// Device-level errors.
#[derive(Error, Debug)]
pub enum DeviceError {
    /// Underlying protocol error.
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    /// Underlying transport error.
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    /// Missing mandatory confirmation token.
    #[error("Operation rejected: persistent write requires an explicit confirmation token")]
    UnauthorizedWrite,

    /// Hardware safety policy rejection.
    #[error("Safety policy violation: {0}")]
    SafetyViolation(&'static str),
}

/// Typestate marker: Read-only access. Only queries and non-destructive reads permitted.
pub struct ReadOnly;

/// Typestate marker: Reversible access. Volatile lighting and active profile selection permitted.
pub struct ReversibleAccess;

/// Typestate marker: Persistent access. Onboard flash memory modification permitted.
pub struct PersistentAccess;

/// Dry-run inspection record describing a proposed USB HID packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunRecord {
    /// Exact 65-byte USB report buffer.
    pub buffer: [u8; REPORT_LENGTH],
    /// Human-readable explanation of the command.
    pub explanation: String,
    /// Safety classification tier.
    pub safety_tier: &'static str,
}

/// The core device handle parameterized by its compile-time typestate.
pub struct Device<T: Transport, State> {
    transport: T,
    dry_run_mode: bool,
    color_cache: [Option<RgbColor>; SPARSE_KEY_COUNT],
    current_mode: Option<LightingMode>,
    current_brightness: u8,
    _state: PhantomData<State>,
}

impl<T: Transport, State> Device<T, State> {
    /// Enables or disables dry-run mode for this device handle.
    pub fn set_dry_run(&mut self, enabled: bool) {
        self.dry_run_mode = enabled;
    }

    /// Returns whether dry-run mode is currently active.
    pub fn is_dry_run(&self) -> bool {
        self.dry_run_mode
    }

    /// Invalidates the local RGB color cache to force full transmission on next batch.
    pub fn invalidate_color_cache(&mut self) {
        self.color_cache = [None; SPARSE_KEY_COUNT];
    }

    /// Sets all entries in the local color cache to a specific color.
    pub fn set_all_color_cache(&mut self, color: RgbColor) {
        self.color_cache = [Some(color); SPARSE_KEY_COUNT];
    }

    /// Returns the cached color for a given sparse key index, if known.
    pub fn color_cache_entry(&self, index: u8) -> Option<RgbColor> {
        if (index as usize) < SPARSE_KEY_COUNT {
            self.color_cache[index as usize]
        } else {
            None
        }
    }

    /// Returns the currently active lighting mode, if known.
    pub fn current_mode(&self) -> Option<LightingMode> {
        self.current_mode
    }

    /// Returns the currently active brightness level (0..4).
    pub fn current_brightness(&self) -> u8 {
        self.current_brightness
    }

    /// Sets the active brightness level tracked by the device handle.
    pub fn set_active_brightness(&mut self, brightness: u8) {
        self.current_brightness = brightness;
    }

    /// Sets the active lighting mode tracked by the device handle.
    pub fn set_active_mode(&mut self, mode: LightingMode) {
        self.current_mode = Some(mode);
    }

    /// Returns a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Consumes the device handle and returns the underlying transport.
    pub fn into_transport(self) -> T {
        self.transport
    }

    /// Queries the hardware identity and firmware revision.
    pub fn query_info(&mut self) -> Result<DeviceInfo, DeviceError> {
        let query_report = build_device_info_query()?;
        assert_opcode_safe(query_report.opcode())?;

        self.transport.write_report(query_report.as_bytes())?;

        for attempt in 0..5 {
            let mut resp_buf = [0u8; REPORT_LENGTH];
            self.transport.read_report(&mut resp_buf)?;

            // If this is an asynchronous lighting notification [0xAA, 0xFA], wait for the query response
            if resp_buf[0] == 0xAA && resp_buf[1] == 0xFA {
                eprintln!(
                    "ℹ️  Received hardware notification [0xAA, 0xFA], awaiting query response (attempt {}/5)...",
                    attempt + 1
                );
                continue;
            }

            let parsed = parse_device_info_response(&ReportBuffer::from_raw(resp_buf)?)?;
            return Ok(parsed);
        }

        Err(DeviceError::Transport(TransportError::Timeout(std::time::Duration::from_millis(2000))))
    }

    /// Queries the current active global lighting configuration directly from keyboard firmware.
    pub fn query_light_config(&mut self) -> Result<LightingConfig, DeviceError> {
        let query_report = fantech_protocol::lighting::build_light_query_report()?;
        assert_opcode_safe(query_report.opcode())?;

        self.transport.write_report(query_report.as_bytes())?;

        for attempt in 0..5 {
            let mut resp_buf = [0u8; REPORT_LENGTH];
            self.transport.read_report(&mut resp_buf)?;

            if resp_buf[0] == 0xAA && resp_buf[1] == 0xFA {
                eprintln!(
                    "ℹ️  Received hardware notification [0xAA, 0xFA], awaiting query response (attempt {}/5)...",
                    attempt + 1
                );
                continue;
            }

            let parsed = fantech_protocol::lighting::parse_light_config_response(&ReportBuffer::from_raw(resp_buf)?)?;
            self.current_mode = Some(parsed.mode);
            self.current_brightness = parsed.brightness;
            return Ok(parsed);
        }

        Err(DeviceError::Transport(TransportError::Timeout(std::time::Duration::from_millis(2000))))
    }

    /// Reads a 56-byte chunk of the onboard keymap table.
    pub fn read_keymap_chunk(&mut self, chunk_index: u8) -> Result<[u8; REPORT_LENGTH], DeviceError> {
        let req = build_keymap_read_request(chunk_index, KeyLayer::Base)?;
        self.transport.write_report(req.as_bytes())?;
        let mut buf = [0u8; REPORT_LENGTH];
        self.transport.read_report(&mut buf)?;
        Ok(buf)
    }

    /// Reads a 56-byte chunk of the 4KB hardware macro memory.
    pub fn read_macro_chunk(&mut self, chunk_index: u8) -> Result<[u8; REPORT_LENGTH], DeviceError> {
        let req = build_macro_read_request(chunk_index)?;
        self.transport.write_report(req.as_bytes())?;
        let mut buf = [0u8; REPORT_LENGTH];
        self.transport.read_report(&mut buf)?;
        Ok(buf)
    }
}

impl<T: Transport> Device<T, ReadOnly> {
    /// Opens a new device handle in safe `ReadOnly` mode.
    pub fn new_read_only(transport: T) -> Self {
        Self {
            transport,
            dry_run_mode: false,
            color_cache: [None; SPARSE_KEY_COUNT],
            current_mode: None,
            current_brightness: 4,
            _state: PhantomData,
        }
    }

    /// Promotes this read-only handle to reversible access.
    pub fn into_reversible(self) -> Device<T, ReversibleAccess> {
        Device {
            transport: self.transport,
            dry_run_mode: self.dry_run_mode,
            color_cache: self.color_cache,
            current_mode: self.current_mode,
            current_brightness: self.current_brightness,
            _state: PhantomData,
        }
    }
}

impl<T: Transport> Device<T, ReversibleAccess> {
    /// Applies a global lighting preset (Opcode 0x0B).
    pub fn set_lighting(&mut self, config: &LightingConfig) -> Result<Option<DryRunRecord>, DeviceError> {
        let report = build_lighting_report(config)?;
        assert_opcode_safe(report.opcode())?;

        if self.dry_run_mode {
            return Ok(Some(DryRunRecord {
                buffer: *report.as_bytes(),
                explanation: format!("Set lighting preset mode {:?} (brightness={}, speed={})", config.mode, config.brightness, config.speed),
                safety_tier: "WRITE-REVERSIBLE",
            }));
        }

        self.transport.write_report(report.as_bytes())?;
        self.current_mode = Some(config.mode);
        self.current_brightness = config.brightness;
        match config.mode {
            LightingMode::Off => {
                self.color_cache = [Some(RgbColor::new(0, 0, 0)); SPARSE_KEY_COUNT];
            }
            LightingMode::Static => {
                self.color_cache = [Some(config.color); SPARSE_KEY_COUNT];
            }
            LightingMode::CustomMatrixCanvas => {
                // Retain current color cache in Custom Matrix Canvas mode
            }
            _ => {
                self.color_cache = [None; SPARSE_KEY_COUNT];
            }
        }
        Ok(None)
    }

    /// Sets the color of an individual physical key LED (Opcode 0x14).
    pub fn set_single_key_color(&mut self, key_index: u8, color: RgbColor) -> Result<Option<DryRunRecord>, DeviceError> {
        let report = build_single_key_color(key_index, color)?;
        assert_opcode_safe(report.opcode())?;

        if self.dry_run_mode {
            return Ok(Some(DryRunRecord {
                buffer: *report.as_bytes(),
                explanation: format!("Set key LED index {} to RGB({}, {}, {})", key_index, color.r, color.g, color.b),
                safety_tier: "WRITE-REVERSIBLE",
            }));
        }

        // If not already in CustomMatrixCanvas mode (e.g. was in Static or Hardware preset),
        // switch to CustomMatrixCanvas (Opcode 0x0B) with 30ms pacing so MCU reconfigures PWM DMA.
        if self.current_mode != Some(LightingMode::CustomMatrixCanvas) {
            let config = LightingConfig {
                zone: LightingZone::MainBacklight,
                mode: LightingMode::CustomMatrixCanvas,
                brightness: self.current_brightness,
                speed: 0,
                direction: AnimationDirection::Default,
                color_mode: ColorMode::FixedColor,
                color: RgbColor::new(0, 0, 0),
            };
            self.set_lighting(&config)?;
            std::thread::sleep(std::time::Duration::from_millis(30));
        }

        self.transport.write_report(report.as_bytes())?;
        if (key_index as usize) < SPARSE_KEY_COUNT {
            self.color_cache[key_index as usize] = Some(color);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));

        Ok(None)
    }

    /// Sets multiple key LEDs smoothly via proven Opcode 0x14 streaming with 10ms pacing.
    ///
    /// Implements high-performance differential updates: keys matching the current local cache are skipped,
    /// turning partial/incremental updates into near-instantaneous transfers.
    ///
    /// Opcode 0x0B is only sent once if entering canvas mode for the first time, eliminating PWM reset flickering.
    pub fn set_multiple_keys_color(&mut self, keys: &[(u8, RgbColor)]) -> Result<Option<Vec<DryRunRecord>>, DeviceError> {
        if keys.is_empty() {
            return Ok(None);
        }

        if self.dry_run_mode {
            let records = keys
                .iter()
                .map(|&(idx, color)| DryRunRecord {
                    buffer: [0u8; REPORT_LENGTH],
                    explanation: format!("Set key {} RGB({}, {}, {}) via Opcode 0x14", idx, color.r, color.g, color.b),
                    safety_tier: "WRITE-REVERSIBLE",
                })
                .collect();
            return Ok(Some(records));
        }

        // If not already in CustomMatrixCanvas mode, switch to CustomMatrixCanvas first with 30ms pacing
        if self.current_mode != Some(LightingMode::CustomMatrixCanvas) {
            let config = LightingConfig {
                zone: LightingZone::MainBacklight,
                mode: LightingMode::CustomMatrixCanvas,
                brightness: self.current_brightness,
                speed: 0,
                direction: AnimationDirection::Default,
                color_mode: ColorMode::FixedColor,
                color: RgbColor::new(0, 0, 0),
            };
            self.set_lighting(&config)?;
            std::thread::sleep(std::time::Duration::from_millis(30));
        }

        // Differential update: filter to keys that actually changed
        let diff: Vec<(u8, RgbColor)> = keys
            .iter()
            .copied()
            .filter(|&(idx, color)| {
                if (idx as usize) < SPARSE_KEY_COUNT {
                    self.color_cache[idx as usize] != Some(color)
                } else {
                    true
                }
            })
            .collect();

        if diff.is_empty() {
            return Ok(None);
        }

        // If more than 15 keys changed (e.g. setting entire keyboard, matrix selection, theme),
        // stream the entire canvas in 7 fast chunks via Opcode 0x12 (70ms)
        // instead of flooding the USB endpoint with dozens/hundreds of single-key reports.
        if diff.len() > 15 {
            let mut full_matrix = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
            for (i, cached) in self.color_cache.iter().enumerate() {
                full_matrix[i] = cached.unwrap_or(RgbColor::new(0, 0, 0));
            }
            for &(idx, color) in keys {
                if (idx as usize) < SPARSE_KEY_COUNT {
                    full_matrix[idx as usize] = color;
                    if let Some(mirror) = get_mirror_key_index(idx) {
                        if (mirror as usize) < SPARSE_KEY_COUNT {
                            full_matrix[mirror as usize] = color;
                        }
                    }
                }
            }
            self.stream_matrix_canvas(&full_matrix)?;
            return Ok(None);
        }

        // For targeted small updates (<= 15 keys, e.g. clicking individual buttons),
        // stream via Opcode 0x14 with 10ms pacing:
        if self.current_mode != Some(LightingMode::CustomMatrixCanvas) {
            let config = LightingConfig {
                zone: LightingZone::MainBacklight,
                mode: LightingMode::CustomMatrixCanvas,
                brightness: self.current_brightness,
                speed: 0,
                direction: AnimationDirection::Default,
                color_mode: ColorMode::FixedColor,
                color: RgbColor::new(0, 0, 0),
            };
            self.set_lighting(&config)?;
            std::thread::sleep(std::time::Duration::from_millis(30));
        }

        for &(key_idx, color) in &diff {
            let report = build_single_key_color(key_idx, color)?;
            self.transport.write_report(report.as_bytes())?;
            if (key_idx as usize) < SPARSE_KEY_COUNT {
                self.color_cache[key_idx as usize] = Some(color);
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }

        Ok(None)
    }

    /// Sets all 104 physical keys via fast Opcode 0x12 matrix streaming (7 packets, 70ms).
    pub fn apply_matrix_keys(&mut self, matrix: &[RgbColor; SPARSE_KEY_COUNT]) -> Result<Option<DryRunRecord>, DeviceError> {
        if self.dry_run_mode {
            return Ok(Some(DryRunRecord {
                buffer: [0u8; REPORT_LENGTH],
                explanation: format!("Set {} physical keys via Opcode 0x12 and activate Custom Matrix Canvas", KEY_DEFINITIONS.len()),
                safety_tier: "WRITE-REVERSIBLE",
            }));
        }

        self.stream_matrix_canvas(matrix)?;
        Ok(None)
    }

    /// Streams per-key RGB matrix canvas using verified Opcode 0x12 reports in 7 fast chunks (70ms total).
    pub fn stream_matrix_canvas(&mut self, matrix: &[RgbColor; SPARSE_KEY_COUNT]) -> Result<Option<Vec<DryRunRecord>>, DeviceError> {
        if self.dry_run_mode {
            let records = KEY_DEFINITIONS
                .iter()
                .map(|k| DryRunRecord {
                    buffer: [0u8; REPORT_LENGTH],
                    explanation: format!("Matrix canvas key {} RGB({}, {}, {}) via Opcode 0x12", k.index, matrix[k.index as usize].r, matrix[k.index as usize].g, matrix[k.index as usize].b),
                    safety_tier: "WRITE-REVERSIBLE",
                })
                .collect();
            return Ok(Some(records));
        }

        // If not already in CustomMatrixCanvas mode, switch to CustomMatrixCanvas first with 30ms pacing
        if self.current_mode != Some(LightingMode::CustomMatrixCanvas) {
            let config = LightingConfig {
                zone: LightingZone::MainBacklight,
                mode: LightingMode::CustomMatrixCanvas,
                brightness: self.current_brightness,
                speed: 0,
                direction: AnimationDirection::Default,
                color_mode: ColorMode::FixedColor,
                color: RgbColor::new(0, 0, 0),
            };
            self.set_lighting(&config)?;
            std::thread::sleep(std::time::Duration::from_millis(30));
        }

        let packets = build_matrix_packets(matrix)?;
        for pkt in packets {
            assert_opcode_safe(pkt.opcode())?;
            self.transport.write_report(pkt.as_bytes())?;
            std::thread::sleep(std::time::Duration::from_millis(15));
        }

        for (i, &col) in matrix.iter().enumerate() {
            self.color_cache[i] = Some(col);
        }

        Ok(None)
    }

    /// Switches the active hardware profile (Opcode 0xFB).
    pub fn switch_profile(&mut self, profile_index: u8) -> Result<Option<DryRunRecord>, DeviceError> {
        let report = build_profile_select(profile_index)?;
        assert_opcode_safe(report.opcode())?;

        if self.dry_run_mode {
            return Ok(Some(DryRunRecord {
                buffer: *report.as_bytes(),
                explanation: format!("Switch active hardware profile to Profile {}", profile_index),
                safety_tier: "WRITE-REVERSIBLE",
            }));
        }

        self.transport.write_report(report.as_bytes())?;
        Ok(None)
    }

    /// Demotes this handle back to safe read-only mode.
    pub fn into_read_only(self) -> Device<T, ReadOnly> {
        Device {
            transport: self.transport,
            dry_run_mode: self.dry_run_mode,
            color_cache: self.color_cache,
            current_mode: self.current_mode,
            current_brightness: self.current_brightness,
            _state: PhantomData,
        }
    }

    /// Promotes this handle to persistent access requiring an explicit typed confirmation.
    pub fn into_persistent(self, confirmation: &str) -> Result<Device<T, PersistentAccess>, DeviceError> {
        if confirmation != "I_CONFIRM_PERSISTENT_HARDWARE_WRITE" {
            return Err(DeviceError::UnauthorizedWrite);
        }
        Ok(Device {
            transport: self.transport,
            dry_run_mode: self.dry_run_mode,
            color_cache: self.color_cache,
            current_mode: self.current_mode,
            current_brightness: self.current_brightness,
            _state: PhantomData,
        })
    }
}

impl<T: Transport> Device<T, PersistentAccess> {
    /// Remaps a single key in onboard non-volatile flash memory (Opcode 0x10).
    pub fn remap_key(
        &mut self,
        key_index: u8,
        layer: KeyLayer,
        action: &KeyAction,
    ) -> Result<Option<DryRunRecord>, DeviceError> {
        let report = build_single_remap_report(key_index, layer, action)?;
        assert_opcode_safe(report.opcode())?;

        if self.dry_run_mode {
            return Ok(Some(DryRunRecord {
                buffer: *report.as_bytes(),
                explanation: format!("Persistent remap key {} on {:?} layer to action {:?}", key_index, layer, action),
                safety_tier: "WRITE-PERSISTENT",
            }));
        }

        self.transport.write_report(report.as_bytes())?;
        Ok(None)
    }

    /// Writes a 56-byte chunk into the 4KB onboard macro memory (Opcode 0x0D).
    pub fn write_macro_chunk(&mut self, chunk_index: u8, data: &[u8]) -> Result<Option<DryRunRecord>, DeviceError> {
        let report = build_macro_write_chunk(chunk_index, data)?;
        assert_opcode_safe(report.opcode())?;

        if self.dry_run_mode {
            return Ok(Some(DryRunRecord {
                buffer: *report.as_bytes(),
                explanation: format!("Write 4KB macro memory chunk {}", chunk_index),
                safety_tier: "WRITE-PERSISTENT",
            }));
        }

        self.transport.write_report(report.as_bytes())?;
        Ok(None)
    }

    /// Wipes all onboard macros from hardware memory (Opcode 0x0F, subcommand 4).
    pub fn wipe_macros(&mut self) -> Result<Option<DryRunRecord>, DeviceError> {
        let report = build_macro_wipe_report()?;
        assert_opcode_safe(report.opcode())?;

        if self.dry_run_mode {
            return Ok(Some(DryRunRecord {
                buffer: *report.as_bytes(),
                explanation: "Wipe all 4KB onboard hardware macro memory".to_string(),
                safety_tier: "WRITE-PERSISTENT",
            }));
        }

        self.transport.write_report(report.as_bytes())?;
        Ok(None)
    }

    /// Restores full factory defaults (Opcode 0x0F, subcommand 0xFF).
    pub fn factory_reset(&mut self) -> Result<Option<DryRunRecord>, DeviceError> {
        let report = build_factory_reset()?;
        assert_opcode_safe(report.opcode())?;

        if self.dry_run_mode {
            return Ok(Some(DryRunRecord {
                buffer: *report.as_bytes(),
                explanation: "Full hardware factory reset restoring default flash state".to_string(),
                safety_tier: "DESTRUCTIVE",
            }));
        }

        self.transport.write_report(report.as_bytes())?;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fantech_protocol::constants::opcodes;
    use fantech_protocol::lighting::{AnimationDirection, ColorMode, LightingConfig, LightingMode, LightingZone, RgbColor};
    use fantech_transport::MockTransport;

    #[test]
    fn test_uniform_then_single_key_no_extraneous_reports() {
        let mock = MockTransport::new();
        let dev = Device::new_read_only(mock);
        let mut rev_dev = dev.into_reversible();

        // 1. Set uniform Green in Static mode
        let static_config = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode: LightingMode::Static,
            brightness: 4,
            speed: 0,
            direction: AnimationDirection::Default,
            color_mode: ColorMode::FixedColor,
            color: RgbColor::new(0, 255, 0),
        };
        rev_dev.set_lighting(&static_config).expect("set static lighting");
        rev_dev.set_all_color_cache(RgbColor::new(0, 255, 0));

        let reports = rev_dev.into_read_only().into_transport().outgoing;
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0][1], opcodes::SET_LIGHT_CONFIG);

        // 2. Set key 44 ('W') to Red while already in CustomMatrixCanvas mode
        let mock2 = MockTransport::new();
        let dev2 = Device::new_read_only(mock2);
        let mut rev_dev2 = dev2.into_reversible();
        rev_dev2.current_mode = Some(LightingMode::CustomMatrixCanvas);
        rev_dev2.set_all_color_cache(RgbColor::new(0, 255, 0));

        rev_dev2.set_single_key_color(44, RgbColor::new(255, 0, 0)).expect("set single key");

        let reports2 = rev_dev2.into_read_only().into_transport().outgoing;
        // MUST be exactly 1 report: Opcode 0x14 directly, with NO Opcode 0x0B
        assert_eq!(reports2.len(), 1);
        assert_eq!(reports2[0][1], opcodes::SET_SINGLE_KEY_LED);
        assert_eq!(reports2[0][3], (44 * 3) & 0xFF);
        assert_eq!(reports2[0][8], 255); // R
        assert_eq!(reports2[0][9], 0);   // G
        assert_eq!(reports2[0][10], 0);  // B
    }

    #[test]
    fn test_multiple_keys_color_sends_only_diff_reports() {
        let mock = MockTransport::new();
        let dev = Device::new_read_only(mock);
        let mut rev_dev = dev.into_reversible();
        rev_dev.current_mode = Some(LightingMode::CustomMatrixCanvas);
        rev_dev.set_all_color_cache(RgbColor::new(0, 255, 0));

        // Submit keys where only key 44 is changed, and key 45 is identical to cache
        let updates = vec![
            (44, RgbColor::new(255, 0, 0)),
            (45, RgbColor::new(0, 255, 0)),
        ];
        rev_dev.set_multiple_keys_color(&updates).expect("set multiple keys");

        let reports = rev_dev.into_read_only().into_transport().outgoing;
        // Key 45 was identical, so ONLY key 44 should be sent
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0][1], opcodes::SET_SINGLE_KEY_LED);
        assert_eq!(reports[0][3], (44 * 3) & 0xFF);
        assert_eq!(reports[0][8], 255); // R
    }
}
