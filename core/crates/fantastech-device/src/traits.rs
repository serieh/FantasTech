//! Standard hardware device traits, capability contracts, and driver registry.
//!
//! Provides the extensible device architecture allowing FantasTech to support
//! future keyboards and peripherals without altering the application core.

#![forbid(unsafe_code)]

use crate::{DeviceError, DeviceInfo, FirmwareProgressUpdate};
use fantastech_protocol::keymap::KeyAction;
use fantastech_protocol::lighting::{LightingConfig, RgbColor};
use serde::{Deserialize, Serialize};

/// Metadata identifying a physical or simulated keyboard hardware device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceDescriptor {
    /// USB Vendor ID.
    pub vendor_id: u16,
    /// USB Product ID.
    pub product_id: u16,
    /// Human-readable vendor name (e.g., "Fantech").
    pub vendor_name: &'static str,
    /// Human-readable product name (e.g., "MaxPower II MK894").
    pub product_name: &'static str,
    /// Optional serial number or hardware revision identifier.
    pub serial_number: Option<String>,
}

/// Explicit hardware capability declarations.
///
/// Frontends (GUI and CLI) inspect these capabilities to dynamically adjust
/// available pages, options, and commands, ensuring no invalid commands are
/// dispatched to peripherals that do not support them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    /// Whether the device supports per-key addressable RGB backlighting.
    pub has_per_key_rgb: bool,
    /// Whether the device features onboard hardware lighting animations.
    pub has_lighting_effects: bool,
    /// Number of distinct lighting modes supported.
    pub lighting_mode_count: u8,
    /// Whether the device supports key remapping across layers.
    pub has_key_remapping: bool,
    /// Number of keymap layers available (e.g., Base, Fn).
    pub layer_count: u8,
    /// Whether the device features onboard hardware macro storage.
    pub has_hardware_macros: bool,
    /// Number of hardware macro slots in onboard memory.
    pub macro_slot_count: u8,
    /// Total onboard macro flash memory size in bytes.
    pub macro_memory_size: usize,
    /// Whether the device supports inactivity auto-sleep timeout.
    pub has_sleep_timeout: bool,
    /// Whether the device has onboard hardware profile switching.
    pub has_hardware_profiles: bool,
    /// Number of onboard hardware profiles supported.
    pub profile_count: u8,
    /// Whether the device features a rotary volume knob.
    pub has_volume_knob: bool,
    /// Whether the device supports firmware update over USB bootloader.
    pub supports_firmware_updates: bool,
    /// Whether the device supports factory reset over USB.
    pub supports_factory_reset: bool,
}

impl Default for DeviceCapabilities {
    fn default() -> Self {
        Self {
            has_per_key_rgb: true,
            has_lighting_effects: true,
            lighting_mode_count: 17,
            has_key_remapping: true,
            layer_count: 2,
            has_hardware_macros: true,
            macro_slot_count: 32,
            macro_memory_size: 4096,
            has_sleep_timeout: true,
            has_hardware_profiles: true,
            profile_count: 3,
            has_volume_knob: true,
            supports_firmware_updates: true,
            supports_factory_reset: true,
        }
    }
}

/// The unified interface contract for all keyboard hardware drivers.
///
/// Every supported keyboard implementation (including the Fantech MK894
/// and future peripheral drivers) implements this trait.
pub trait KeyboardDevice: Send + Sync {
    /// Returns the static metadata descriptor for this device.
    fn descriptor(&self) -> &DeviceDescriptor;

    /// Returns the declared capabilities of this device.
    fn capabilities(&self) -> DeviceCapabilities;

    /// Queries device identification and firmware version from hardware.
    fn get_device_info(&mut self) -> Result<DeviceInfo, DeviceError>;

    /// Applies a global lighting animation configuration.
    fn apply_lighting_config(&mut self, config: &LightingConfig) -> Result<(), DeviceError>;

    /// Streams per-key RGB canvas data to the device matrix.
    fn apply_matrix_canvas(&mut self, matrix: &[RgbColor]) -> Result<(), DeviceError>;

    /// Updates the static color of a single addressed key.
    fn set_single_key_color(&mut self, key_index: u8, color: RgbColor) -> Result<(), DeviceError>;

    /// Turns off all backlighting immediately.
    fn turn_off_lighting(&mut self) -> Result<(), DeviceError>;

    /// Writes keymap actions for a specified layer.
    fn apply_keymap_layer(&mut self, layer: u8, actions: &[KeyAction]) -> Result<(), DeviceError>;

    /// Writes the complete raw hardware macro memory block.
    fn apply_macro_memory(&mut self, data: &[u8]) -> Result<(), DeviceError>;

    /// Configures the inactivity auto-sleep timeout in seconds.
    fn set_sleep_timeout(&mut self, seconds: u16) -> Result<(), DeviceError>;

    /// Switches the active hardware profile index.
    fn switch_profile(&mut self, profile_idx: u8) -> Result<(), DeviceError>;

    /// Performs a non-volatile factory reset on the hardware.
    fn factory_reset(&mut self, token: &str) -> Result<(), DeviceError>;

    /// Executes a safe firmware update sequence with progress updates.
    fn update_firmware(
        &mut self,
        binary: &[u8],
        token: &str,
        progress: &mut dyn FnMut(FirmwareProgressUpdate),
    ) -> Result<(), DeviceError>;
}

/// Reference mock device implementation demonstrating how future keyboard
/// drivers can be created, tested, and integrated.
pub struct ReferenceMockDevice {
    descriptor: DeviceDescriptor,
    capabilities: DeviceCapabilities,
    info: DeviceInfo,
    active_profile: u8,
    sleep_timeout: u16,
    is_lighting_on: bool,
}

impl Default for ReferenceMockDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl ReferenceMockDevice {
    /// Creates a new instance of the reference mock device.
    pub fn new() -> Self {
        Self {
            descriptor: DeviceDescriptor {
                vendor_id: 0x1234,
                product_id: 0x5678,
                vendor_name: "Reference",
                product_name: "Mock Keyboard Peripheral",
                serial_number: Some("REF-MOCK-001".to_string()),
            },
            capabilities: DeviceCapabilities::default(),
            info: DeviceInfo {
                profile_name: "Reference Mock 104".to_string(),
                firmware_version: "V100".to_string(),
                active_profile: 0,
                profile_count: 3,
                pid: 0x5678,
                battery: 100,
                charge: 100,
                serial_number: "REF-001".to_string(),
            },
            active_profile: 0,
            sleep_timeout: 600,
            is_lighting_on: true,
        }
    }
}

impl KeyboardDevice for ReferenceMockDevice {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.descriptor
    }

    fn capabilities(&self) -> DeviceCapabilities {
        self.capabilities.clone()
    }

    fn get_device_info(&mut self) -> Result<DeviceInfo, DeviceError> {
        Ok(self.info.clone())
    }

    fn apply_lighting_config(&mut self, _config: &LightingConfig) -> Result<(), DeviceError> {
        self.is_lighting_on = true;
        Ok(())
    }

    fn apply_matrix_canvas(&mut self, _matrix: &[RgbColor]) -> Result<(), DeviceError> {
        Ok(())
    }

    fn set_single_key_color(&mut self, _key_index: u8, _color: RgbColor) -> Result<(), DeviceError> {
        Ok(())
    }

    fn turn_off_lighting(&mut self) -> Result<(), DeviceError> {
        self.is_lighting_on = false;
        Ok(())
    }

    fn apply_keymap_layer(&mut self, _layer: u8, _actions: &[KeyAction]) -> Result<(), DeviceError> {
        Ok(())
    }

    fn apply_macro_memory(&mut self, _data: &[u8]) -> Result<(), DeviceError> {
        Ok(())
    }

    fn set_sleep_timeout(&mut self, seconds: u16) -> Result<(), DeviceError> {
        self.sleep_timeout = seconds;
        Ok(())
    }

    fn switch_profile(&mut self, profile_idx: u8) -> Result<(), DeviceError> {
        if profile_idx >= self.capabilities.profile_count {
            return Err(DeviceError::SafetyViolation("Profile index exceeds hardware capability"));
        }
        self.active_profile = profile_idx;
        self.info.active_profile = profile_idx;
        Ok(())
    }

    fn factory_reset(&mut self, token: &str) -> Result<(), DeviceError> {
        if token != "I_CONFIRM_FACTORY_RESET" {
            return Err(DeviceError::UnauthorizedWrite);
        }
        self.active_profile = 0;
        self.info.active_profile = 0;
        self.sleep_timeout = 600;
        self.is_lighting_on = true;
        Ok(())
    }

    fn update_firmware(
        &mut self,
        _binary: &[u8],
        token: &str,
        _progress: &mut dyn FnMut(FirmwareProgressUpdate),
    ) -> Result<(), DeviceError> {
        if token != crate::firmware::FirmwareUpdateSafetyToken::MANDATORY_CONFIRMATION {
            return Err(DeviceError::UnauthorizedWrite);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reference_mock_device_lifecycle() {
        let mut dev = ReferenceMockDevice::new();
        assert_eq!(dev.descriptor().vendor_id, 0x1234);
        assert_eq!(dev.capabilities().profile_count, 3);

        let info = dev.get_device_info().unwrap();
        assert_eq!(info.active_profile, 0);

        dev.switch_profile(1).unwrap();
        assert_eq!(dev.get_device_info().unwrap().active_profile, 1);

        assert!(dev.switch_profile(5).is_err());

        assert!(dev.factory_reset("WRONG_TOKEN").is_err());
        assert!(dev.factory_reset("I_CONFIRM_FACTORY_RESET").is_ok());
        assert_eq!(dev.get_device_info().unwrap().active_profile, 0);
    }
}
