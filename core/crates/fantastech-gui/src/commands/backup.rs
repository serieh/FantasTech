//! Hardware state backup export and restore operations.

use super::*;

impl FantasTechApp {
    pub(crate) fn export_backup_file(&mut self) {
        let path = std::path::PathBuf::from(&self.backup_file_path);
        let prof = (self.active_profile as usize).min(2);
        let base_keymap = self.profiles[prof][0].to_vec();
        let fn_keymap = self.profiles[prof][1].to_vec();

        let backup = if self.is_emulator {
            FullDeviceBackup {
                format_version: 1,
                timestamp_epoch_sec: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                device_info: fantastech_protocol::commands::DeviceInfo {
                    profile_name: self.device_model.clone(),
                    firmware_version: self.firmware_version.clone(),
                    active_profile: self.active_profile,
                    profile_count: 3,
                    pid: 0xFE9C,
                    battery: 100,
                    charge: 0,
                    serial_number: "EMULATOR".to_string(),
                },
                lighting_config: LightingConfig {
                    zone: LightingZone::MainBacklight,
                    mode: LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum),
                    brightness: self.brightness,
                    speed: self.speed,
                    direction: AnimationDirection::Default,
                    color_mode: ColorMode::DynamicSpectrum,
                    color: RgbColor::new(self.current_color[0], self.current_color[1], self.current_color[2]),
                },
                base_keymap,
                fn_keymap,
                macro_memory: self.macro_memory.as_bytes().to_vec(),
            }
        } else if let Ok(t) = HidrawTransport::discover() {
            let mut dev = Device::new_read_only(t);
            match dev.export_full_backup() {
                Ok(b) => b,
                Err(e) => {
                    self.status_message = format!("Failed to read device backup: {:?}", e);
                    return;
                }
            }
        } else {
            FullDeviceBackup {
                format_version: 1,
                timestamp_epoch_sec: 0,
                device_info: fantastech_protocol::commands::DeviceInfo {
                    profile_name: self.device_model.clone(),
                    firmware_version: self.firmware_version.clone(),
                    active_profile: self.active_profile,
                    profile_count: 3,
                    pid: 0xFE9C,
                    battery: 100,
                    charge: 0,
                    serial_number: "LOCAL".to_string(),
                },
                lighting_config: LightingConfig::default(),
                base_keymap,
                fn_keymap,
                macro_memory: self.macro_memory.as_bytes().to_vec(),
            }
        };

        match serde_json::to_string_pretty(&backup) {
            Ok(json) => {
                if let Err(e) = fantastech_core::write_user_owned_file(&path, json.as_bytes()) {
                    self.status_message = format!("Failed to save backup to {:?}: {}", path, e);
                } else {
                    self.status_message = format!("Backup successfully saved to {:?}", path);
                }
            }
            Err(e) => {
                self.status_message = format!("JSON serialization error: {}", e);
            }
        }
    }

    pub(crate) fn restore_backup_file(&mut self) {
        let path = std::path::PathBuf::from(&self.backup_file_path);
        let json = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) => {
                self.status_message = format!("Failed to open backup file {:?}: {}", path, e);
                return;
            }
        };

        let backup: FullDeviceBackup = match serde_json::from_str(&json) {
            Ok(b) => b,
            Err(e) => {
                self.status_message = format!("Corrupt backup file JSON: {}", e);
                return;
            }
        };

        if backup.base_keymap.len() != SPARSE_KEY_COUNT || backup.fn_keymap.len() != SPARSE_KEY_COUNT {
            self.status_message = "Invalid backup: keymap layer slot count mismatch".to_string();
            return;
        }

        let prof = (self.active_profile as usize).min(2);
        self.profiles[prof][0].copy_from_slice(&backup.base_keymap);
        self.profiles[prof][1].copy_from_slice(&backup.fn_keymap);
        if backup.macro_memory.len() == MACRO_MEMORY_SIZE {
            let _ = self.macro_memory.write_bytes(0, &backup.macro_memory);
        }

        if self.dry_run_enabled {
            self.status_message = format!("[DRY-RUN] Backup restore from {:?} simulated", path);
            return;
        }

        if self.is_emulator {
            self.status_message = format!("Virtual Emulator: Restored keyboard state from {:?}", path);
        } else if let Some(ref p) = self.permission_denied_node {
            self.status_message = format!("Restored locally. {}", platform::permission_error("flash the restored backup", p.as_path()));
        } else if let Ok(t) = HidrawTransport::discover() {
            let dev = Device::new_read_only(t);
            if let Ok(mut persistent_dev) = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE") {
                persistent_dev.set_dry_run(false);
                match persistent_dev.restore_full_backup(&backup) {
                    Ok(_) => {
                        self.status_message = format!("Hardware successfully restored from {:?}", path);
                    }
                    Err(e) => {
                        self.status_message = format!("Failed restoring backup: {:?}", e);
                    }
                }
            }
        } else {
            self.status_message = format!("Restored keyboard state from {:?} locally", path);
        }
    }

}
