//! Type-state device safety engine and auditable access control for Fantech MK894.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

use fantastech_core::{get_mirror_key_index, KEY_DEFINITIONS};
use fantastech_protocol::commands::{
    assert_opcode_safe, build_device_info_query, build_factory_reset, build_profile_select,
    parse_device_info_response, DeviceInfo,
};
use fantastech_protocol::constants::{MACRO_MEMORY_SIZE, REPORT_LENGTH, SPARSE_KEY_COUNT};
use fantastech_protocol::error::ProtocolError;
use fantastech_protocol::keymap::{
    build_batch_keymap_reports, build_keymap_read_request, build_single_remap_report,
    parse_keymap_chunk_response, KeyAction, KeyLayer,
};
use fantastech_protocol::lighting::{
    build_lighting_report, build_single_key_color, AnimationDirection, ColorMode, LightingConfig, LightingMode,
    LightingZone, RgbColor,
};
use fantastech_protocol::macro_mem::{
    build_macro_read_request, build_macro_wipe_report, build_macro_write_chunk,
};
use fantastech_protocol::report::ReportBuffer;
use fantastech_transport::{Transport, TransportError};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use thiserror::Error;

pub mod firmware;
pub mod traits;

pub use firmware::*;
pub use traits::*;

/// Complete device state snapshot containing device metadata, lighting configuration,
/// base keymap, fn keymap, and full 4KB onboard hardware macro memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullDeviceBackup {
    /// Schema format version (currently 1).
    pub format_version: u32,
    /// Unix timestamp when backup was generated.
    pub timestamp_epoch_sec: u64,
    /// Device metadata queried from hardware.
    pub device_info: DeviceInfo,
    /// Active global lighting configuration.
    pub lighting_config: LightingConfig,
    /// Normal primary layer keymap (126 sparse slots).
    pub base_keymap: Vec<KeyAction>,
    /// Function modifier layer keymap (126 sparse slots).
    pub fn_keymap: Vec<KeyAction>,
    /// Full 4,096-byte hardware macro memory table.
    pub macro_memory: Vec<u8>,
}

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

    /// Returns a mutable reference to the underlying transport.
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
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
        let query_report = fantastech_protocol::lighting::build_light_query_report()?;
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

            let parsed = fantastech_protocol::lighting::parse_light_config_response(&ReportBuffer::from_raw(resp_buf)?)?;
            self.current_mode = Some(parsed.mode);
            self.current_brightness = parsed.brightness;
            return Ok(parsed);
        }

        Err(DeviceError::Transport(TransportError::Timeout(std::time::Duration::from_millis(2000))))
    }

    /// Reads a 56-byte chunk of the onboard keymap table (14 KeyActions) for a given layer (Opcode 0x08).
    pub fn read_keymap_chunk(&mut self, chunk_index: u8, layer: KeyLayer) -> Result<[KeyAction; 14], DeviceError> {
        let req = build_keymap_read_request(chunk_index, layer)?;
        self.transport.write_report(req.as_bytes())?;
        let mut buf = [0u8; REPORT_LENGTH];
        self.transport.read_report(&mut buf)?;
        let actions = parse_keymap_chunk_response(&buf)?;
        Ok(actions)
    }

    /// Reads all 9 chunks of a layer keymap table (all 126 sparse matrix slots, 504 bytes total).
    pub fn read_keymap_layer(&mut self, layer: KeyLayer) -> Result<[KeyAction; SPARSE_KEY_COUNT], DeviceError> {
        let mut layer_map = [KeyAction::disabled(); SPARSE_KEY_COUNT];
        for chunk_idx in 0..9 {
            let chunk = self.read_keymap_chunk(chunk_idx as u8, layer)?;
            let start = chunk_idx * 14;
            for (i, action) in chunk.iter().enumerate() {
                if start + i < SPARSE_KEY_COUNT {
                    layer_map[start + i] = *action;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok(layer_map)
    }

    /// Reads a 56-byte chunk of the 4KB hardware macro memory (Opcode 0x0C).
    pub fn read_macro_chunk(&mut self, chunk_index: u8) -> Result<[u8; 56], DeviceError> {
        let req = build_macro_read_request(chunk_index)?;
        self.transport.write_report(req.as_bytes())?;
        let mut buf = [0u8; REPORT_LENGTH];
        self.transport.read_report(&mut buf)?;
        let mut chunk_data = [0u8; 56];
        chunk_data.copy_from_slice(&buf[8..64]);
        Ok(chunk_data)
    }

    /// Reads the entire 4,096-byte onboard macro memory in 74 sequential chunks (Opcode 0x0C).
    pub fn read_macro_memory(&mut self) -> Result<[u8; MACRO_MEMORY_SIZE], DeviceError> {
        let mut memory = [0u8; MACRO_MEMORY_SIZE];
        for chunk_idx in 0..74 {
            let chunk = self.read_macro_chunk(chunk_idx as u8)?;
            let offset = chunk_idx * 56;
            let end = (offset + 56).min(MACRO_MEMORY_SIZE);
            let len = end - offset;
            memory[offset..end].copy_from_slice(&chunk[..len]);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok(memory)
    }

    /// Reads all device state and exports a complete snapshot backup.
    pub fn export_full_backup(&mut self) -> Result<FullDeviceBackup, DeviceError> {
        let device_info = self.query_info()?;
        let lighting_config = self.query_light_config()?;
        let base_keymap = self.read_keymap_layer(KeyLayer::Base)?;
        let fn_keymap = self.read_keymap_layer(KeyLayer::Fn)?;
        let macro_memory = self.read_macro_memory()?;

        let timestamp_epoch_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(FullDeviceBackup {
            format_version: 1,
            timestamp_epoch_sec,
            device_info,
            lighting_config,
            base_keymap: base_keymap.to_vec(),
            fn_keymap: fn_keymap.to_vec(),
            macro_memory: macro_memory.to_vec(),
        })
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

        // Differential update: filter to keys that actually changed.
        // If an explicit single key is passed, always transmit to ensure user-directed clicks are honored.
        let diff: Vec<(u8, RgbColor)> = if keys.len() == 1 {
            keys.to_vec()
        } else {
            keys.iter()
                .copied()
                .filter(|&(idx, color)| {
                    if (idx as usize) < SPARSE_KEY_COUNT {
                        self.color_cache[idx as usize] != Some(color)
                    } else {
                        true
                    }
                })
                .collect()
        };

        if diff.is_empty() {
            return Ok(None);
        }

        // Switch to CustomMatrixCanvas mode if not already active with 30ms pacing
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

        // Physical MK894 hardware (PID 0xFE9C) requires sequential Opcode 0x14 reports
        // paced at 12-15ms. We write all keys and dual-switch mirrors reliably.
        for &(key_idx, color) in &diff {
            let report = build_single_key_color(key_idx, color)?;
            self.transport.write_report(report.as_bytes())?;
            if (key_idx as usize) < SPARSE_KEY_COUNT {
                self.color_cache[key_idx as usize] = Some(color);
            }
            if let Some(mirror) = get_mirror_key_index(key_idx) {
                let m_report = build_single_key_color(mirror, color)?;
                self.transport.write_report(m_report.as_bytes())?;
                if (mirror as usize) < SPARSE_KEY_COUNT {
                    self.color_cache[mirror as usize] = Some(color);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(12));
        }

        Ok(None)
    }

    /// Sets all 104 physical keys via sequential Opcode 0x14 streaming and commits Custom Matrix Canvas.
    pub fn apply_matrix_keys(&mut self, matrix: &[RgbColor; SPARSE_KEY_COUNT]) -> Result<Option<DryRunRecord>, DeviceError> {
        if self.dry_run_mode {
            return Ok(Some(DryRunRecord {
                buffer: [0u8; REPORT_LENGTH],
                explanation: format!("Set {} physical keys via Opcode 0x14 and activate Custom Matrix Canvas", KEY_DEFINITIONS.len()),
                safety_tier: "WRITE-REVERSIBLE",
            }));
        }

        // Switch to CustomMatrixCanvas mode if not already active
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

        // Transmit sequential Opcode 0x14 reports for all 104 ANSI physical keys + mirror slots
        for key_def in KEY_DEFINITIONS.iter() {
            let idx = key_def.index;
            let color = matrix[idx as usize];
            let report = build_single_key_color(idx, color)?;
            self.transport.write_report(report.as_bytes())?;
            self.color_cache[idx as usize] = Some(color);

            if let Some(mirror) = get_mirror_key_index(idx) {
                let m_report = build_single_key_color(mirror, color)?;
                self.transport.write_report(m_report.as_bytes())?;
                if (mirror as usize) < SPARSE_KEY_COUNT {
                    self.color_cache[mirror as usize] = Some(color);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(12));
        }

        // Mode 16 commit latch
        let commit_cfg = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode: LightingMode::CustomMatrixCanvas,
            brightness: self.current_brightness,
            speed: 0,
            direction: AnimationDirection::Default,
            color_mode: ColorMode::FixedColor,
            color: RgbColor::new(0, 0, 0),
        };
        self.set_lighting(&commit_cfg)?;

        Ok(None)
    }

    /// Streams per-key RGB matrix canvas using verified Opcode 0x14 reports for all 104 physical keys.
    pub fn stream_matrix_canvas(&mut self, matrix: &[RgbColor; SPARSE_KEY_COUNT]) -> Result<Option<Vec<DryRunRecord>>, DeviceError> {
        if self.dry_run_mode {
            let records = KEY_DEFINITIONS
                .iter()
                .map(|k| DryRunRecord {
                    buffer: [0u8; REPORT_LENGTH],
                    explanation: format!("Matrix canvas key {} RGB({}, {}, {}) via Opcode 0x14", k.index, matrix[k.index as usize].r, matrix[k.index as usize].g, matrix[k.index as usize].b),
                    safety_tier: "WRITE-REVERSIBLE",
                })
                .collect();
            return Ok(Some(records));
        }

        let _ = self.apply_matrix_keys(matrix)?;
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

    /// Alias for `remap_key` matching the trait specifications.
    pub fn remap_key_action(
        &mut self,
        key_index: u8,
        layer: KeyLayer,
        action: &KeyAction,
    ) -> Result<Option<DryRunRecord>, DeviceError> {
        self.remap_key(key_index, layer, action)
    }

    /// Writes all 504 bytes across 9 packets for a complete layer keymap (Opcode 0x09).
    pub fn write_keymap_layer(
        &mut self,
        layer: KeyLayer,
        actions: &[KeyAction; SPARSE_KEY_COUNT],
    ) -> Result<Option<Vec<DryRunRecord>>, DeviceError> {
        let reports = build_batch_keymap_reports(layer, actions)?;

        if self.dry_run_mode {
            let records = reports
                .iter()
                .enumerate()
                .map(|(i, r)| DryRunRecord {
                    buffer: *r.as_bytes(),
                    explanation: format!("Batch keymap write chunk {}/9 for {:?} layer", i + 1, layer),
                    safety_tier: "WRITE-PERSISTENT",
                })
                .collect();
            return Ok(Some(records));
        }

        for report in reports {
            assert_opcode_safe(report.opcode())?;
            self.transport.write_report(report.as_bytes())?;
            std::thread::sleep(std::time::Duration::from_millis(15));
        }

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

    /// Writes the full 4,096-byte macro memory table into onboard flash memory across 74 chunks (Opcode 0x0D).
    pub fn write_macro_memory(
        &mut self,
        memory: &[u8; MACRO_MEMORY_SIZE],
    ) -> Result<Option<Vec<DryRunRecord>>, DeviceError> {
        let mut reports = Vec::with_capacity(74);
        for chunk_idx in 0..74 {
            let offset = chunk_idx * 56;
            let end = (offset + 56).min(MACRO_MEMORY_SIZE);
            let chunk_data = &memory[offset..end];
            let report = build_macro_write_chunk(chunk_idx as u8, chunk_data)?;
            reports.push(report);
        }

        if self.dry_run_mode {
            let records = reports
                .iter()
                .enumerate()
                .map(|(i, r)| DryRunRecord {
                    buffer: *r.as_bytes(),
                    explanation: format!("Write 4KB macro memory chunk {}/74", i + 1),
                    safety_tier: "WRITE-PERSISTENT",
                })
                .collect();
            return Ok(Some(records));
        }

        for report in reports {
            assert_opcode_safe(report.opcode())?;
            self.transport.write_report(report.as_bytes())?;
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

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

    /// Alias for `wipe_macros`.
    pub fn wipe_macro_memory(&mut self) -> Result<Option<DryRunRecord>, DeviceError> {
        self.wipe_macros()
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

    /// Sets the inactivity auto-sleep timeout in seconds (Opcode 0xFC).
    pub fn set_sleep_timeout(&mut self, seconds: u16) -> Result<Option<DryRunRecord>, DeviceError> {
        let report = fantastech_protocol::commands::build_auto_sleep_timeout(seconds)?;
        assert_opcode_safe(report.opcode())?;

        if self.dry_run_mode {
            return Ok(Some(DryRunRecord {
                buffer: *report.as_bytes(),
                explanation: format!("Set inactivity auto-sleep timeout to {} seconds", seconds),
                safety_tier: "WRITE-PERSISTENT",
            }));
        }

        self.transport.write_report(report.as_bytes())?;
        Ok(None)
    }

    /// Executes a full factory reset (Opcode 0x0F 0xFF), requiring explicit confirmation token.
    pub fn execute_factory_reset(&mut self, token: &str) -> Result<Option<DryRunRecord>, DeviceError> {
        if token != "I_CONFIRM_FACTORY_RESET" {
            return Err(DeviceError::SafetyViolation(
                "Factory reset rejected: invalid confirmation token. Expected 'I_CONFIRM_FACTORY_RESET'.",
            ));
        }
        self.factory_reset()
    }

    /// Restores a full device backup snapshot onto hardware.
    pub fn restore_full_backup(
        &mut self,
        backup: &FullDeviceBackup,
    ) -> Result<Option<Vec<DryRunRecord>>, DeviceError> {
        if backup.base_keymap.len() != SPARSE_KEY_COUNT {
            return Err(DeviceError::SafetyViolation(
                "Corrupt backup: base_keymap must contain exactly 126 slots",
            ));
        }
        if backup.fn_keymap.len() != SPARSE_KEY_COUNT {
            return Err(DeviceError::SafetyViolation(
                "Corrupt backup: fn_keymap must contain exactly 126 slots",
            ));
        }
        if backup.macro_memory.len() != MACRO_MEMORY_SIZE {
            return Err(DeviceError::SafetyViolation(
                "Corrupt backup: macro_memory must contain exactly 4096 bytes",
            ));
        }

        let mut base_arr = [KeyAction::disabled(); SPARSE_KEY_COUNT];
        base_arr.copy_from_slice(&backup.base_keymap);

        let mut fn_arr = [KeyAction::disabled(); SPARSE_KEY_COUNT];
        fn_arr.copy_from_slice(&backup.fn_keymap);

        let mut macro_arr = [0u8; MACRO_MEMORY_SIZE];
        macro_arr.copy_from_slice(&backup.macro_memory);

        if self.dry_run_mode {
            let mut dry_runs = Vec::new();
            let light_report = build_lighting_report(&backup.lighting_config)?;
            dry_runs.push(DryRunRecord {
                buffer: *light_report.as_bytes(),
                explanation: format!(
                    "Restore lighting preset mode {:?} (brightness={}, speed={})",
                    backup.lighting_config.mode, backup.lighting_config.brightness, backup.lighting_config.speed
                ),
                safety_tier: "WRITE-REVERSIBLE",
            });
            if let Some(recs) = self.write_keymap_layer(KeyLayer::Base, &base_arr)? {
                dry_runs.extend(recs);
            }
            if let Some(recs) = self.write_keymap_layer(KeyLayer::Fn, &fn_arr)? {
                dry_runs.extend(recs);
            }
            if let Some(recs) = self.write_macro_memory(&macro_arr)? {
                dry_runs.extend(recs);
            }
            return Ok(Some(dry_runs));
        }

        // Apply lighting preset (Opcode 0x0B)
        let light_report = build_lighting_report(&backup.lighting_config)?;
        assert_opcode_safe(light_report.opcode())?;
        self.transport.write_report(light_report.as_bytes())?;
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Apply base keymap
        self.write_keymap_layer(KeyLayer::Base, &base_arr)?;
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Apply Fn keymap
        self.write_keymap_layer(KeyLayer::Fn, &fn_arr)?;
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Apply 4KB macro memory
        self.write_macro_memory(&macro_arr)?;

        Ok(None)
    }

    /// Demotes this persistent access handle back to reversible access.
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

    /// Demotes this persistent access handle back to safe read-only mode.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use fantastech_protocol::constants::opcodes;
    use fantastech_protocol::lighting::{AnimationDirection, ColorMode, LightingConfig, LightingMode, LightingZone, RgbColor};
    use fantastech_transport::MockTransport;

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
        assert_eq!(reports2[0][3], 44 * 3);
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
        assert_eq!(reports[0][3], 44 * 3);
        assert_eq!(reports[0][8], 255); // R
    }

    #[test]
    fn test_sleep_timeout_execution() {
        let mock = MockTransport::new();
        let dev = Device::new_read_only(mock);
        let mut persistent = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE").unwrap();

        persistent.set_sleep_timeout(600).expect("set sleep timeout 10m");
        let reports = persistent.into_read_only().into_transport().outgoing;
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0][1], 0xFC);
        assert_eq!(reports[0][5], (600 & 0xFF) as u8);
        assert_eq!(reports[0][6], (600 >> 8) as u8);
    }

    #[test]
    fn test_factory_reset_safety_token() {
        let mock = MockTransport::new();
        let dev = Device::new_read_only(mock);
        let mut persistent = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE").unwrap();

        // Rejects invalid token
        assert!(persistent.execute_factory_reset("WRONG_TOKEN").is_err());

        // Accepts valid token
        persistent.execute_factory_reset("I_CONFIRM_FACTORY_RESET").expect("valid token");
        let reports = persistent.into_read_only().into_transport().outgoing;
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0][1], opcodes::RESTORE_FACTORY);
        assert_eq!(reports[0][2], 0xFF);
    }

    #[test]
    fn test_full_device_backup_json_roundtrip() {
        let backup = FullDeviceBackup {
            format_version: 1,
            timestamp_epoch_sec: 1700000000,
            device_info: DeviceInfo {
                profile_name: "MK894 Test".to_string(),
                firmware_version: "V101".to_string(),
                active_profile: 1,
                profile_count: 3,
                pid: 0xFE9C,
                battery: 100,
                charge: 0,
                serial_number: "TEST123456".to_string(),
            },
            lighting_config: LightingConfig {
                zone: LightingZone::MainBacklight,
                mode: LightingMode::Spectrum,
                brightness: 4,
                speed: 3,
                direction: AnimationDirection::Default,
                color_mode: ColorMode::DynamicSpectrum,
                color: RgbColor::new(255, 0, 0),
            },
            base_keymap: vec![KeyAction::disabled(); SPARSE_KEY_COUNT],
            fn_keymap: vec![KeyAction::disabled(); SPARSE_KEY_COUNT],
            macro_memory: vec![0xAA; MACRO_MEMORY_SIZE],
        };

        let json = serde_json::to_string_pretty(&backup).expect("serialize backup");
        let deserialized: FullDeviceBackup = serde_json::from_str(&json).expect("deserialize backup");
        assert_eq!(backup, deserialized);
    }

    #[test]
    fn test_restore_full_backup_live_and_dry_run() {
        let backup = FullDeviceBackup {
            format_version: 1,
            timestamp_epoch_sec: 1700000000,
            device_info: DeviceInfo {
                profile_name: "MK894 Test".to_string(),
                firmware_version: "V101".to_string(),
                active_profile: 0,
                profile_count: 3,
                pid: 0xFE9C,
                battery: 100,
                charge: 0,
                serial_number: "TEST123456".to_string(),
            },
            lighting_config: LightingConfig::default(),
            base_keymap: vec![KeyAction::disabled(); SPARSE_KEY_COUNT],
            fn_keymap: vec![KeyAction::disabled(); SPARSE_KEY_COUNT],
            macro_memory: vec![0x00; MACRO_MEMORY_SIZE],
        };

        // 1. Dry run mode
        let mock_dry = MockTransport::new();
        let dev_dry = Device::new_read_only(mock_dry);
        let mut persistent_dry = dev_dry.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE").unwrap();
        persistent_dry.set_dry_run(true);
        let records = persistent_dry.restore_full_backup(&backup).unwrap().expect("dry run records");
        // 1 lighting + 9 base + 9 fn + 74 macro = 93 operations
        assert_eq!(records.len(), 93);

        // 2. Corrupt backup rejection
        let mut corrupt_backup = backup.clone();
        corrupt_backup.base_keymap.pop();
        assert!(persistent_dry.restore_full_backup(&corrupt_backup).is_err());

        let mut corrupt_macro = backup.clone();
        corrupt_macro.macro_memory.pop();
        assert!(persistent_dry.restore_full_backup(&corrupt_macro).is_err());
    }
}

