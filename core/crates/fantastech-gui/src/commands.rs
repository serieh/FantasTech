//! Application commands and persistence operations.
//! Backend protocol, transport, and safety contracts remain delegated to existing crates.

use super::*;

impl FantasTechApp {
    pub(super) fn check_service(&mut self) -> bool {
        // Connection state is refreshed by the background monitor. Never block
        // the egui thread with service or HID discovery from a UI callback.
        self.service_active
    }

    pub(super) fn record_keymap_undo(&mut self) {
        let profile = (self.active_profile as usize).min(2);
        let layer = self.active_layer as usize;
        self.undo_keymaps.push(self.profiles[profile][layer].to_vec());
        // Keep history deliberately bounded; a keymap snapshot is small, but
        // unbounded edit history is unnecessary for a hardware settings app.
        if self.undo_keymaps.len() > 100 {
            self.undo_keymaps.remove(0);
        }
        self.redo_keymaps.clear();
    }

    pub(super) fn undo_keymap_edit(&mut self) {
        let Some(previous) = self.undo_keymaps.pop() else {
            return;
        };
        if previous.len() != SPARSE_KEY_COUNT {
            self.status_message = "Undo history entry is invalid; no changes were applied".to_owned();
            return;
        }
        let profile = (self.active_profile as usize).min(2);
        let layer = self.active_layer as usize;
        self.redo_keymaps.push(self.profiles[profile][layer].to_vec());
        self.profiles[profile][layer].copy_from_slice(&previous);
        self.persist_current_button_profile();
        self.status_message = "Undid the last keymap change".to_owned();
        self.apply_current_layer_to_hardware();
    }

    pub(super) fn redo_keymap_edit(&mut self) {
        let Some(next) = self.redo_keymaps.pop() else {
            return;
        };
        if next.len() != SPARSE_KEY_COUNT {
            self.status_message = "Redo history entry is invalid; no changes were applied".to_owned();
            return;
        }
        let profile = (self.active_profile as usize).min(2);
        let layer = self.active_layer as usize;
        self.undo_keymaps.push(self.profiles[profile][layer].to_vec());
        self.profiles[profile][layer].copy_from_slice(&next);
        self.persist_current_button_profile();
        self.status_message = "Redid the last keymap change".to_owned();
        self.apply_current_layer_to_hardware();
    }

    pub(super) fn remap_single_key(&mut self, key_index: u8, action: KeyAction) {
        if self.active_layer == KeyLayer::Base && action == KeyAction::hardware_custom(16) {
            self.status_message = "⚠️ Cannot assign Hardware Reset to Base layer keys (intended for Fn layer only).".to_string();
            return;
        }

        if key_index as usize >= SPARSE_KEY_COUNT {
            self.status_message = format!("Invalid key index {}; no changes were applied", key_index);
            return;
        }
        let prof = (self.active_profile as usize).min(2);
        let lay = self.active_layer as usize;
        let mirror = get_mirror_key_index(key_index)
            .filter(|index| (*index as usize) < SPARSE_KEY_COUNT);
        let primary_changed = self.profiles[prof][lay][key_index as usize] != action;
        let mirror_changed = mirror
            .map(|index| self.profiles[prof][lay][index as usize] != action)
            .unwrap_or(false);
        if primary_changed || mirror_changed {
            self.record_keymap_undo();
        }
        self.profiles[prof][lay][key_index as usize] = action;
        if let Some(mirror_index) = mirror {
            self.profiles[prof][lay][mirror_index as usize] = action;
        }
        self.persist_current_button_profile();

        let action_str = format_action_str(&action);
        if self.dry_run_enabled {
            self.status_message = format!("[DRY-RUN] Key {} on {:?} layer set to {}", key_index, self.active_layer, action_str);
            return;
        }

        if self.check_service() {
            match send_ipc_request(&IpcRequest::ApplyKeymapLayer {
                layer: self.active_layer,
                actions: self.profiles[prof][lay].to_vec(),
            }) {
                Ok(IpcResponse::Ok) => {
                    self.status_message = format!("Service (IPC): Key {} remapped to {}", key_index, action_str);
                }
                Ok(resp) => {
                    self.status_message = format!("Service response: {:?}", resp);
                }
                Err(e) => {
                    self.status_message = format!("IPC error remapping key: {}", e);
                }
            }
        } else if let Some(ref path) = self.permission_denied_node {
            self.status_message = platform::permission_error("write to the keyboard", path.as_path());
        } else if self.is_emulator {
            self.status_message = format!("Virtual Emulator: Key {} remapped to {}", key_index, action_str);
        } else if let Ok(t) = HidrawTransport::discover() {
            let dev = Device::new_read_only(t);
            if let Ok(mut persistent_dev) = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE") {
                persistent_dev.set_dry_run(false);
                let _ = persistent_dev.remap_key_action(key_index, self.active_layer, &action);
                if let Some(mirror_index) = mirror {
                    let _ = persistent_dev.remap_key_action(mirror_index, self.active_layer, &action);
                }
                self.status_message = format!("Hardware: Key {} remapped to {}", key_index, action_str);
            }
        }
    }

    pub(super) fn set_active_profile(&mut self, p: u8) {
        if p > 2 {
            return;
        }
        self.active_profile = p;
        self.undo_keymaps.clear();
        self.redo_keymaps.clear();
        if self.dry_run_enabled {
            self.status_message = format!("[DRY-RUN] Switched view to Profile {}", p + 1);
            return;
        }

        if self.check_service() {
            match send_ipc_request(&IpcRequest::SwitchHardwareProfile(p)) {
                Ok(IpcResponse::Ok) => {
                    self.status_message = format!("Service (IPC): Hardware switched to Profile {}", p + 1);
                }
                Ok(resp) => {
                    self.status_message = format!("Service response: {:?}", resp);
                }
                Err(e) => {
                    self.status_message = format!("IPC error switching profile: {}", e);
                }
            }
        } else if let Some(ref path) = self.permission_denied_node {
            self.status_message = platform::permission_error("switch the hardware profile", path.as_path());
        } else if self.is_emulator {
            self.status_message = format!("Virtual Emulator: Switched to Profile {}", p + 1);
        } else if let Ok(t) = HidrawTransport::discover() {
            let mut dev = Device::new_read_only(t).into_reversible();
            dev.set_dry_run(false);
            match dev.switch_profile(p) {
                Ok(_) => {
                    self.status_message = format!("Hardware switched to Profile {}", p + 1);
                }
                Err(e) => {
                    self.status_message = format!("Failed to switch profile: {:?}", e);
                }
            }
        } else {
            self.status_message = format!("Switched view to Profile {}", p + 1);
        }
    }

    pub(super) fn execute_factory_reset(&mut self) {
        let base = default_layer_keymap(KeyLayer::Base);
        let fn_layer = default_layer_keymap(KeyLayer::Fn);
        self.profiles = [
            [base, fn_layer],
            [base, fn_layer],
            [base, fn_layer],
        ];
        self.active_profile = 0;
        self.active_layer = KeyLayer::Base;
        self.selected_key = None;
        self.undo_keymaps.clear();
        self.redo_keymaps.clear();
        self.selected_mode = 4; // Spectrum Cycle (hardware factory default)
        self.brightness = 4;    // 100%
        self.speed = 2;         // Normal
        self.direction = 0;
        self.is_fixed_color = false;
        self.custom_selected_keys.clear();
        self.custom_select_all = false;
        self.selected_button_profile = "default".to_string();
        self.selected_lighting_profile = "gaming".to_string();
        self.selected_macro_profile = "default".to_string();
        self.macro_memory = MacroMemoryTable::new_empty();
        self.factory_reset_input.clear();
        self.factory_reset_modal_open = false;

        if self.dry_run_enabled {
            self.status_message = "[DRY-RUN] Keyboard factory reset simulated".to_string();
            return;
        }

        if self.check_service() {
            match send_ipc_request(&IpcRequest::FactoryReset) {
                Ok(IpcResponse::Ok) => {
                    self.status_message = "Service (IPC): Hardware factory reset executed successfully".to_string();
                }
                Ok(resp) => {
                    self.status_message = format!("Service response: {:?}", resp);
                }
                Err(e) => {
                    self.status_message = format!("IPC error executing factory reset: {}", e);
                }
            }
        } else if let Some(ref path) = self.permission_denied_node {
            self.status_message = platform::permission_error("factory-reset the keyboard", path.as_path());
        } else if self.is_emulator {
            self.status_message = "Virtual Emulator: Keyboard factory reset executed successfully".to_string();
        } else if let Ok(t) = HidrawTransport::discover() {
            let dev = Device::new_read_only(t);
            match dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE") {
                Ok(mut persistent_dev) => {
                    persistent_dev.set_dry_run(false);
                    match persistent_dev.execute_factory_reset("I_CONFIRM_FACTORY_RESET") {
                        Ok(_) => {
                            self.status_message = "Hardware factory reset executed successfully".to_string();
                        }
                        Err(e) => {
                            self.status_message = format!("Hardware factory reset error: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    self.status_message = format!("Safety Authorization error: {:?}", e);
                }
            }
        } else {
            self.status_message = "Keyboard factory reset executed locally".to_string();
        }
    }

    pub(super) fn apply_sleep_timeout(&mut self, minutes: u16) {
        self.sleep_timeout_mins = minutes;
        let seconds = minutes.saturating_mul(60);

        if self.dry_run_enabled {
            self.status_message = format!("[DRY-RUN] Auto-sleep timeout set to {} minutes ({}s)", minutes, seconds);
            return;
        }

        if self.check_service() {
            match send_ipc_request(&IpcRequest::SetSleepTimeout(seconds)) {
                Ok(IpcResponse::Ok) => {
                    self.status_message = format!("Service (IPC): Auto-sleep timeout set to {} minutes ({}s)", minutes, seconds);
                }
                Ok(resp) => {
                    self.status_message = format!("Service response: {:?}", resp);
                }
                Err(e) => {
                    self.status_message = format!("IPC error setting sleep timeout: {}", e);
                }
            }
        } else if let Some(ref path) = self.permission_denied_node {
            self.status_message = platform::permission_error("set the keyboard sleep timeout", path.as_path());
        } else if self.is_emulator {
            self.status_message = format!("Virtual Emulator: Auto-sleep timeout set to {} minutes ({}s)", minutes, seconds);
        } else if let Ok(t) = HidrawTransport::discover() {
            let dev = Device::new_read_only(t);
            if let Ok(mut persistent_dev) = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE") {
                persistent_dev.set_dry_run(false);
                match persistent_dev.set_sleep_timeout(seconds) {
                    Ok(_) => {
                        self.status_message = format!("Hardware auto-sleep timeout set to {} minutes ({}s)", minutes, seconds);
                    }
                    Err(e) => {
                        self.status_message = format!("Failed to set sleep timeout: {:?}", e);
                    }
                }
            }
        } else {
            self.status_message = format!("Auto-sleep timeout set to {} minutes ({}s) locally", minutes, seconds);
        }
    }

    #[cfg(target_os = "linux")]
    #[allow(dead_code)]
    pub(super) fn install_system_integration(&mut self) {
        let mut logs = Vec::new();
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let bin_dir = std::path::PathBuf::from(&home).join(".local/bin");
        let desktop_dir = std::path::PathBuf::from(&home).join(".local/share/applications");
        let icon_dir = std::path::PathBuf::from(&home).join(".local/share/icons/hicolor/scalable/apps");
        let systemd_dir = std::path::PathBuf::from(&home).join(".config/systemd/user");

        let _ = std::fs::create_dir_all(&bin_dir);
        let _ = std::fs::create_dir_all(&desktop_dir);
        let _ = std::fs::create_dir_all(&icon_dir);
        let _ = std::fs::create_dir_all(&systemd_dir);

        if let Ok(cur_exe) = std::env::current_exe() {
            if let Some(parent) = cur_exe.parent() {
                let cli_src = parent.join("fantastech");
                let gui_src = parent.join("fantastech-gui");
                if cli_src.exists() {
                    let _ = std::fs::copy(&cli_src, bin_dir.join("fantastech"));
                    let _ = std::fs::copy(&cli_src, bin_dir.join("fantastech-ctl"));
                    logs.push("Installed fantastech and fantastech-ctl to ~/.local/bin/");
                }
                if gui_src.exists() {
                    let _ = std::fs::copy(&gui_src, bin_dir.join("fantastech-gui"));
                    logs.push("Installed fantastech-gui to ~/.local/bin/");
                }
            }
        }

        let desktop_content = "[Desktop Entry]\nName=FantasTech\nComment=Fantech MK894 Mechanical RGB Keyboard Control Center\nExec=fantastech-gui\nIcon=fantastech\nTerminal=false\nType=Application\nCategories=Utility;HardwareSettings;\nKeywords=rgb;keyboard;lighting;macro;openrgb;fantastech;\nStartupNotify=true\n";
        let _ = write_user_owned_file(&desktop_dir.join("fantastech.desktop"), desktop_content.as_bytes());
        logs.push("Installed desktop launcher to ~/.local/share/applications/fantastech.desktop");

        let svg_content = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <rect width="64" height="64" rx="14" fill="#E53E3E"/>
  <rect x="8" y="14" width="48" height="36" rx="6" fill="#1E293B" stroke="#FFFFFF" stroke-opacity="0.7" stroke-width="1.5"/>
  <circle cx="16" cy="24" r="3" fill="#0071CF"/>
  <circle cx="24" cy="24" r="3" fill="#F59E0B"/>
  <circle cx="32" cy="24" r="3" fill="#10B981"/>
  <circle cx="40" cy="24" r="3" fill="#06B6D4"/>
  <circle cx="48" cy="24" r="3" fill="#A855F7"/>
  <circle cx="16" cy="34" r="3" fill="#EC4899"/>
  <circle cx="24" cy="34" r="3" fill="#FFFFFF"/>
  <circle cx="32" cy="34" r="3" fill="#0071CF"/>
  <circle cx="40" cy="34" r="3" fill="#F59E0B"/>
  <circle cx="48" cy="34" r="3" fill="#10B981"/>
  <rect x="20" y="42" width="24" height="4" rx="2" fill="#FFFFFF"/>
</svg>"##;
        let _ = write_user_owned_file(&icon_dir.join("fantastech.svg"), svg_content.as_bytes());
        logs.push("Installed scalable app icon to ~/.local/share/icons/hicolor/scalable/apps/fantastech.svg");

        let service_content = "[Unit]\nDescription=FantasTech Background Keyboard Daemon (Fantech MK894)\nDocumentation=https://github.com/serieh/fantastech\nAfter=default.target\n\n[Service]\nType=simple\nExecStart=%h/.local/bin/fantastech service run\nRestart=on-failure\nRestartSec=3\n\n[Install]\nWantedBy=default.target\n";
        let _ = write_user_owned_file(&systemd_dir.join("fantastech.service"), service_content.as_bytes());
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "enable", "--now", "fantastech.service"])
            .status();
        logs.push("Enabled and started systemd user service fantastech.service");

        self.install_status_message = Some(format!("✅ Installation complete! ({})", logs.join("; ")));
        self.status_message = "Installed FantasTech system integration".to_string();
    }
    #[cfg(not(target_os = "linux"))]
    #[allow(dead_code)]
    pub(super) fn install_system_integration(&mut self) {
        self.install_status_message = Some(
            "Native installer integration is currently implemented for Linux only. The GUI remains available on this platform.".to_owned(),
        );
        self.status_message = "System installation is not available on this platform".to_owned();
    }


    #[cfg(target_os = "linux")]
    #[allow(dead_code)]
    pub(super) fn uninstall_system_integration(&mut self) {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "--now", "fantastech.service"])
            .status();
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".config/systemd/user/fantastech.service"));
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".local/share/applications/fantastech.desktop"));
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".local/share/icons/hicolor/scalable/apps/fantastech.svg"));
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".local/bin/fantastech"));
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".local/bin/fantastech-ctl"));
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".local/bin/fantastech-gui"));
        self.install_status_message = Some("🗑️ Successfully uninstalled FantasTech system integration (binaries, desktop launcher, systemd service)".to_string());
        self.status_message = "Uninstalled FantasTech system integration".to_string();
    }
    #[cfg(not(target_os = "linux"))]
    #[allow(dead_code)]
    pub(super) fn uninstall_system_integration(&mut self) {
        self.install_status_message = Some(
            "No Linux system integration was changed. Platform-specific installer support is not configured here.".to_owned(),
        );
        self.status_message = "System uninstallation is not available on this platform".to_owned();
    }


    pub(super) fn export_backup_file(&mut self) {
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
                    self.status_message = format!("✅ Backup successfully saved to {:?}", path);
                }
            }
            Err(e) => {
                self.status_message = format!("JSON serialization error: {}", e);
            }
        }
    }

    pub(super) fn restore_backup_file(&mut self) {
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

    pub(super) fn wipe_all_macros(&mut self) {
        self.macro_memory = MacroMemoryTable::new_empty();
        self.macro_sequence.steps.clear();

        if self.dry_run_enabled {
            self.status_message = "[DRY-RUN] Macro memory wipe simulated".to_string();
            return;
        }

        if let Some(ref path) = self.permission_denied_node {
            self.status_message = platform::permission_error("erase onboard macros", path.as_path());
        } else if self.is_emulator {
            self.status_message = "Virtual Emulator: Erased all onboard macro pointers and action memory".to_string();
        } else if let Ok(t) = HidrawTransport::discover() {
            let dev = Device::new_read_only(t);
            match dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE") {
                Ok(mut persistent_dev) => {
                    persistent_dev.set_dry_run(false);
                    match persistent_dev.wipe_macros() {
                        Ok(_) => {
                            self.status_message = "Hardware: Erased all onboard macro pointers and action memory".to_string();
                        }
                        Err(e) => {
                            self.status_message = format!("Hardware macro wipe error: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    self.status_message = format!("Safety Authorization error: {:?}", e);
                }
            }
        } else {
            self.status_message = "Erased all onboard macro pointers and action memory locally".to_string();
        }
    }

    pub(super) fn apply_current_layer_to_hardware(&mut self) {
        let prof = (self.active_profile as usize).min(2);
        let lay = self.active_layer as usize;
        let actions = self.profiles[prof][lay];

        if self.dry_run_enabled {
            self.status_message = format!("[DRY-RUN] 9 packets (504 bytes) ready to flash for {:?} layer on Profile {}", self.active_layer, self.active_profile);
            return;
        }

        if self.check_service() {
            match send_ipc_request(&IpcRequest::ApplyKeymapLayer {
                layer: self.active_layer,
                actions: actions.to_vec(),
            }) {
                Ok(IpcResponse::Ok) => {
                    self.status_message = format!("Service (IPC): Flashed {:?} layer on Profile {}", self.active_layer, self.active_profile);
                }
                Ok(resp) => {
                    self.status_message = format!("Service response: {:?}", resp);
                }
                Err(e) => {
                    self.status_message = format!("IPC error flashing keymap layer: {}", e);
                }
            }
        } else if let Some(ref path) = self.permission_denied_node {
            self.status_message = platform::permission_error("flash the keymap layer", path.as_path());
        } else if self.is_emulator {
            self.status_message = format!("Virtual Emulator: Flashed 504 bytes for {:?} layer on Profile {}", self.active_layer, self.active_profile);
        } else if let Ok(t) = HidrawTransport::discover() {
            let dev = Device::new_read_only(t);
            if let Ok(mut persistent_dev) = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE") {
                persistent_dev.set_dry_run(false);
                match persistent_dev.write_keymap_layer(self.active_layer, &actions) {
                    Ok(_) => {
                        self.status_message = format!("Hardware flashed successfully: {:?} layer on Profile {}", self.active_layer, self.active_profile);
                    }
                    Err(e) => {
                        self.status_message = format!("Hardware write error: {:?}", e);
                    }
                }
            }
        }
    }

    pub(super) fn set_single_key_color(&mut self, key_index: u8) {
        if key_index as usize >= SPARSE_KEY_COUNT {
            self.status_message = format!("Invalid lighting matrix index {}; no color was applied", key_index);
            return;
        }

        let factor = (self.brightness as f32 / 4.0).clamp(0.0, 1.0);
        let color = [
            (self.current_color[0] as f32 * factor).round() as u8,
            (self.current_color[1] as f32 * factor).round() as u8,
            (self.current_color[2] as f32 * factor).round() as u8,
        ];
        let mirror = get_mirror_key_index(key_index);

        // Dry-run must apply uniformly to both the IPC and direct-HID paths.
        if self.dry_run_enabled {
            self.status_message = format!(
                "[DRY-RUN] Previewed key {} color #{:02X}{:02X}{:02X}",
                key_index, color[0], color[1], color[2]
            );
            return;
        }

        if self.check_service() {
            let mut request_error = None;
            for index in std::iter::once(key_index).chain(mirror) {
                match send_ipc_request(&IpcRequest::SetKeyColor {
                    key_index: index,
                    r: color[0],
                    g: color[1],
                    b: color[2],
                }) {
                    Ok(IpcResponse::Ok) => {}
                    Ok(response) => request_error = Some(format!("Unexpected service response: {response:?}")),
                    Err(error) => request_error = Some(format!("IPC error updating key color: {error}")),
                }
            }
            self.status_message = request_error.unwrap_or_else(|| {
                format!("Updated RGB color for key {}", key_index)
            });
        } else if let Some(path) = self.permission_denied_node.as_ref() {
            self.status_message = platform::permission_error("update the key color", path.as_path());
        } else if self.is_emulator {
            self.status_message = format!("Virtual emulator: previewed RGB color for key {}", key_index);
        } else if let Ok(transport) = HidrawTransport::discover() {
            let mut device = Device::new_read_only(transport).into_reversible();
            device.set_dry_run(false);
            let rgb = RgbColor::new(color[0], color[1], color[2]);
            let mut failures = Vec::new();
            if let Err(error) = device.set_single_key_color(key_index, rgb) {
                failures.push(format!("key {}: {error:?}", key_index));
            }
            if let Some(mirror_index) = mirror {
                if let Err(error) = device.set_single_key_color(mirror_index, rgb) {
                    failures.push(format!("key {}: {error:?}", mirror_index));
                }
            }
            self.status_message = if failures.is_empty() {
                format!("Updated RGB color for key {}", key_index)
            } else {
                format!("RGB update failed ({})", failures.join("; "))
            };
        } else {
            self.apply_lighting();
        }
    }

    pub(super) fn select_all_custom_keys(&mut self) {
        self.custom_select_all = true;
        self.custom_selected_keys.clear();
        for k in ANSI_104_KEYS {
            self.custom_selected_keys.insert(k.index);
        }
        self.status_message = "Selected all 104 keys on Custom Canvas".to_string();
    }

    pub(super) fn deselect_all_custom_keys(&mut self) {
        self.custom_select_all = false;
        self.custom_selected_keys.clear();
        self.status_message = "Cleared Custom Canvas key selection".to_string();
    }

    pub(super) fn toggle_custom_key_selection(&mut self, key_index: u8, multi: bool) {
        if multi {
            if self.custom_select_all {
                self.custom_select_all = false;
                self.custom_selected_keys.clear();
                for k in ANSI_104_KEYS {
                    if k.index != key_index {
                        self.custom_selected_keys.insert(k.index);
                    }
                }
            } else if self.custom_selected_keys.contains(&key_index) {
                self.custom_selected_keys.remove(&key_index);
            } else {
                self.custom_selected_keys.insert(key_index);
            }
        } else {
            self.custom_select_all = false;
            self.custom_selected_keys.clear();
            self.custom_selected_keys.insert(key_index);
        }
        if self.custom_selected_keys.len() == ANSI_104_KEYS.len() {
            self.custom_select_all = true;
        }
    }

    pub(super) fn record_used_color(&mut self, rgb: [u8; 3]) {
        self.recent_colors.retain(|&c| c != rgb);
        self.recent_colors.insert(0, rgb);
        if self.recent_colors.len() > 6 {
            self.recent_colors.truncate(6);
        }
    }

    pub(super) fn paint_key_and_mirror(&mut self, key_index: u8, rgb: [u8; 3]) {
        self.record_used_color(rgb);
        let painted = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        let idx = key_index as usize;
        if idx < SPARSE_KEY_COUNT {
            self.key_colors[idx] = painted;
            if let Some(mirror) = get_mirror_key_index(key_index) {
                if (mirror as usize) < SPARSE_KEY_COUNT {
                    self.key_colors[mirror as usize] = painted;
                }
            }
            self.persist_current_lighting_profile();
            self.set_single_key_color(key_index);
        }
    }

    pub(super) fn apply_color_to_custom_selection(&mut self, rgb: [u8; 3]) {
        if self.custom_selected_keys.is_empty() && !self.custom_select_all {
            self.status_message = "No keys selected to paint. Click keys to select them first.".to_string();
            return;
        }
        self.record_used_color(rgb);
        self.current_color = rgb;
        let painted = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        let keys_to_paint: Vec<u8> = if self.custom_select_all {
            ANSI_104_KEYS.iter().map(|k| k.index).collect()
        } else {
            self.custom_selected_keys.iter().copied().collect()
        };

        for &key_idx in &keys_to_paint {
            if (key_idx as usize) < SPARSE_KEY_COUNT {
                self.key_colors[key_idx as usize] = painted;
                if let Some(mirror) = get_mirror_key_index(key_idx) {
                    if (mirror as usize) < SPARSE_KEY_COUNT {
                        self.key_colors[mirror as usize] = painted;
                    }
                }
            }
        }
        self.persist_current_lighting_profile();

        if keys_to_paint.len() == 1 {
            self.set_single_key_color(keys_to_paint[0]);
        } else if keys_to_paint.len() <= 10 {
            for &k in &keys_to_paint {
                self.set_single_key_color(k);
            }
            self.status_message = format!("Painted {} keys with #{:02X}{:02X}{:02X}", keys_to_paint.len(), rgb[0], rgb[1], rgb[2]);
        } else {
            self.apply_lighting();
            self.status_message = format!("Painted {} keys with #{:02X}{:02X}{:02X}", keys_to_paint.len(), rgb[0], rgb[1], rgb[2]);
        }
    }

    pub(super) fn turn_off_custom_selection(&mut self) {
        self.apply_color_to_custom_selection([0, 0, 0]);
    }

    pub(super) fn select_custom_key_group(&mut self, group: &str) {
        self.custom_select_all = false;
        self.custom_selected_keys.clear();
        match group {
            "wasd" => {
                for &idx in &[44, 64, 65, 66] {
                    self.custom_selected_keys.insert(idx);
                }
                self.status_message = "Selected WASD keys".to_string();
            }
            "arrows" => {
                for &idx in &[99, 119, 120, 121] {
                    self.custom_selected_keys.insert(idx);
                }
                self.status_message = "Selected Arrow keys".to_string();
            }
            "numpad" => {
                for &idx in &[38, 39, 40, 41, 59, 60, 61, 62, 80, 81, 82, 101, 102, 103, 104, 122, 123, 124] {
                    self.custom_selected_keys.insert(idx);
                }
                self.status_message = "Selected Numpad keys".to_string();
            }
            "fkeys" => {
                self.custom_selected_keys.insert(0); // Esc
                for idx in 1..=12 {
                    self.custom_selected_keys.insert(idx);
                }
                self.status_message = "Selected Function row (F1-F12 + Esc)".to_string();
            }
            _ => {}
        }
    }

    pub(super) fn apply_designer_palette(&mut self, palette_name: &str) {
        let (primary, p_name) = match palette_name {
            "miami" => ([0, 240, 255], "Cyberpunk Miami"),
            "terminal" => ([0, 255, 65], "Matrix Terminal"),
            "flare" => ([255, 69, 0], "Solar Flare"),
            "vaporwave" => ([157, 78, 221], "Vaporwave Dusk"),
            "frost" => ([58, 134, 255], "Arctic Frost"),
            "blood_moon" => ([217, 4, 41], "Blood Moon"),
            "pastel" => ([183, 228, 199], "Pastel Dream"),
            "rainbow" => ([255, 0, 0], "Spectrum Rainbow"),
            _ => ([0, 113, 207], "Designer Palette"),
        };

        self.current_color = primary;
        self.record_used_color(primary);

        if self.selected_mode == 16 {
            // In Custom Matrix Canvas, compute per-key designer gradient across ANSI_104_KEYS
            for vkey in ANSI_104_KEYS {
                let idx = vkey.index as usize;
                if idx >= SPARSE_KEY_COUNT {
                    continue;
                }
                let t_col: f32 = (vkey.x / 22.0_f32).clamp(0.0_f32, 1.0_f32);
                let t_row: f32 = (vkey.y / 5.35_f32).clamp(0.0_f32, 1.0_f32);
                let rgb = match palette_name {
                    "miami" => {
                        // Cyan [0, 240, 255] -> Hot Pink [255, 0, 127]
                        [
                            (t_col * 255.0_f32).round() as u8,
                            (240.0_f32 * (1.0_f32 - t_col)).round() as u8,
                            (255.0_f32 * (1.0_f32 - t_col) + 127.0_f32 * t_col).round() as u8,
                        ]
                    }
                    "terminal" => {
                        // Forest green [0, 59, 0] -> Bright terminal green [0, 255, 65]
                        [
                            0,
                            (59.0_f32 + t_row * (255.0_f32 - 59.0_f32)).round() as u8,
                            (t_row * 65.0_f32).round() as u8,
                        ]
                    }
                    "flare" => {
                        // Flame Orange [255, 69, 0] -> Sunburst Gold [255, 215, 0]
                        [
                            255,
                            (69.0_f32 + t_col * (215.0_f32 - 69.0_f32)).round() as u8,
                            0,
                        ]
                    }
                    "vaporwave" => {
                        // Lilac Purple [157, 78, 221] -> Sky Blue [72, 202, 228]
                        let t_diag: f32 = ((vkey.x + vkey.y * 3.0_f32) / 38.0_f32).clamp(0.0_f32, 1.0_f32);
                        [
                            (157.0_f32 * (1.0_f32 - t_diag) + 72.0_f32 * t_diag).round() as u8,
                            (78.0_f32 * (1.0_f32 - t_diag) + 202.0_f32 * t_diag).round() as u8,
                            (221.0_f32 * (1.0_f32 - t_diag) + 228.0_f32 * t_diag).round() as u8,
                        ]
                    }
                    "frost" => {
                        // Glacial Cyan [224, 251, 252] -> Ice Blue [58, 134, 255]
                        [
                            (224.0_f32 * (1.0_f32 - t_col) + 58.0_f32 * t_col).round() as u8,
                            (251.0_f32 * (1.0_f32 - t_col) + 134.0_f32 * t_col).round() as u8,
                            255,
                        ]
                    }
                    "blood_moon" => {
                        let is_mod = [0, 13, 27, 28, 42, 55, 69, 83, 84, 96, 98, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114].contains(&vkey.index);
                        if is_mod {
                            [25, 25, 25] // Deep Obsidian
                        } else {
                            [217, 4, 41] // Crimson Red
                        }
                    }
                    "pastel" => {
                        // Mint [183, 228, 199] -> Peach [255, 202, 212]
                        [
                            (183.0_f32 + t_col * (255.0_f32 - 183.0_f32)).round() as u8,
                            (228.0_f32 * (1.0_f32 - t_col) + 202.0_f32 * t_col).round() as u8,
                            (199.0_f32 * (1.0_f32 - t_col) + 212.0_f32 * t_col).round() as u8,
                        ]
                    }
                    _ => {
                        let hue = (vkey.x / 22.0_f32) * 360.0_f32;
                        let (r, g, b) = hsv_to_rgb_simple(hue, 1.0, 1.0);
                        [r, g, b]
                    }
                };
                self.key_colors[idx] = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                if let Some(mirror) = get_mirror_key_index(vkey.index) {
                    if (mirror as usize) < SPARSE_KEY_COUNT {
                        self.key_colors[mirror as usize] = self.key_colors[idx];
                    }
                }
            }
            self.apply_lighting();
            self.status_message = format!("Applied '{}' designer palette across keyboard matrix", p_name);
        } else {
            self.is_fixed_color = true;
            self.apply_lighting();
            self.status_message = format!("Set color to '{}' primary", p_name);
        }
    }

    pub(super) fn apply_lighting(&mut self) {
        let mode = LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum);
        let color_mode = if self.is_fixed_color || mode == LightingMode::Static {
            ColorMode::FixedColor
        } else {
            ColorMode::DynamicSpectrum
        };
        let dir = if self.direction == 1 {
            AnimationDirection::LeftToRight
        } else {
            AnimationDirection::Default
        };
        let config = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode,
            brightness: self.brightness,
            speed: self.speed,
            direction: dir,
            color_mode,
            color: RgbColor::new(self.current_color[0], self.current_color[1], self.current_color[2]),
        };

        self.persist_current_lighting_profile();

        if self.dry_run_enabled {
            self.status_message = format!("[DRY-RUN] Lighting config simulated: mode {:?}", config.mode);
            return;
        }

        if self.check_service() {
            if mode == LightingMode::CustomMatrixCanvas {
                let mut matrix = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                let factor = (self.brightness as f32 / 4.0).clamp(0.0, 1.0);
                for (idx, color) in self.key_colors.iter().enumerate() {
                    if idx < SPARSE_KEY_COUNT {
                        let r = (color.r() as f32 * factor).round() as u8;
                        let g = (color.g() as f32 * factor).round() as u8;
                        let b = (color.b() as f32 * factor).round() as u8;
                        matrix[idx] = RgbColor::new(r, g, b);
                    }
                }
                let colors: Vec<[u8; 3]> = matrix.iter().map(|c| [c.r, c.g, c.b]).collect();
                let _ = send_ipc_request(&IpcRequest::SetCanvasMatrix { colors });
            }
            match send_ipc_request(&IpcRequest::SetLightingMode {
                mode,
                brightness: self.brightness,
                speed: self.speed,
                direction: self.direction,
                color_mode,
                color: self.current_color,
            }) {
                Ok(IpcResponse::Ok) => {
                    self.status_message = format!("Service (IPC): Applied lighting mode {:?}", mode);
                }
                Ok(resp) => {
                    self.status_message = format!("Service response: {:?}", resp);
                }
                Err(e) => {
                    self.status_message = format!("IPC error setting lighting: {}", e);
                }
            }
        } else if let Some(ref path) = self.permission_denied_node {
            self.status_message = platform::permission_error("apply lighting to the keyboard", path.as_path());
        } else if self.is_emulator {
            self.status_message = format!("Virtual Emulator: Simulated applied lighting mode {:?}", config.mode);
        } else if let Ok(t) = HidrawTransport::discover() {
            let mut dev = Device::new_read_only(t).into_reversible();
            dev.set_dry_run(self.dry_run_enabled);
            match dev.set_lighting(&config) {
                Ok(_) => {
                    if mode == LightingMode::CustomMatrixCanvas {
                        let mut matrix = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                        let factor = (self.brightness as f32 / 4.0).clamp(0.0, 1.0);
                        for (idx, color) in self.key_colors.iter().enumerate() {
                            if idx < SPARSE_KEY_COUNT {
                                let r = (color.r() as f32 * factor).round() as u8;
                                let g = (color.g() as f32 * factor).round() as u8;
                                let b = (color.b() as f32 * factor).round() as u8;
                                matrix[idx] = RgbColor::new(r, g, b);
                            }
                        }
                        let _ = dev.apply_matrix_keys(&matrix);
                    }
                    self.status_message = format!("Hardware updated: mode {:?}, brightness {}, speed {}", config.mode, self.brightness, self.speed);
                }
                Err(e) => {
                    self.status_message = format!("❌ Hardware error setting lighting: {}", e);
                }
            }
        } else {
            self.status_message = "No physical keyboard detected or device access was denied by the operating system".to_string();
        }
    }

    #[allow(dead_code)]
    pub(super) fn apply_theme(&mut self, theme: ThemeConfig) {
        let matrix = theme.to_matrix();
        for (i, c) in matrix.iter().enumerate() {
            if i < self.key_colors.len() {
                self.key_colors[i] = Color32::from_rgb(c.r, c.g, c.b);
            }
        }

        if self.dry_run_enabled {
            self.status_message = format!("[DRY-RUN] Streamed theme '{}'", theme.name);
            return;
        }

        if self.check_service() {
            let colors: Vec<[u8; 3]> = matrix.iter().map(|c| [c.r, c.g, c.b]).collect();
            match send_ipc_request(&IpcRequest::SetCanvasMatrix { colors }) {
                Ok(IpcResponse::Ok) => {
                    self.status_message = format!("Service (IPC): Streamed theme '{}' to keyboard", theme.name);
                }
                Ok(resp) => {
                    self.status_message = format!("Service response: {:?}", resp);
                }
                Err(e) => {
                    self.status_message = format!("IPC error streaming theme: {}", e);
                }
            }
        } else if let Some(ref path) = self.permission_denied_node {
            self.status_message = platform::permission_error("stream the lighting theme", path.as_path());
        } else if self.is_emulator {
            self.status_message = format!("Loaded theme '{}' into emulator", theme.name);
        } else if let Ok(t) = HidrawTransport::discover() {
            let mut dev = Device::new_read_only(t).into_reversible();
            dev.set_dry_run(self.dry_run_enabled);
            let _ = dev.stream_matrix_canvas(&matrix);
            let _ = dev.set_lighting(&LightingConfig {
                mode: LightingMode::CustomMatrixCanvas,
                ..Default::default()
            });
            self.status_message = format!("Streamed theme '{}' to keyboard", theme.name);
        }
    }

    pub(super) fn apply_lighting_profile_by_id(&mut self, id: &str) {
        if let Some(p) = self.profile_manager.load_lighting_profile(id) {
            self.selected_lighting_profile = p.id.clone();
            self.selected_mode = p.mode as u8;
            self.brightness = p.brightness;
            self.speed = p.speed;
            self.direction = p.direction;
            self.current_color = p.color;
            if let Some(cm) = p.color_mode {
                self.is_fixed_color = cm == ColorMode::FixedColor;
            } else {
                self.is_fixed_color = p.mode == LightingMode::Static;
            }

            for (key_idx, rgb) in &p.canvas {
                if *key_idx < SPARSE_KEY_COUNT {
                    self.key_colors[*key_idx] = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                }
            }

            if self.check_service() {
                match send_ipc_request(&IpcRequest::ApplyLightingProfile { id: id.to_string() }) {
                    Ok(IpcResponse::Ok) => {
                        self.status_message = format!("Service (IPC): Applied lighting profile '{}'", p.name);
                    }
                    Ok(resp) => {
                        self.status_message = format!("Service response: {:?}", resp);
                    }
                    Err(e) => {
                        self.status_message = format!("IPC error applying profile: {}", e);
                    }
                }
            } else {
                self.apply_lighting();
            }
        } else {
            self.status_message = format!("Profile '{}' not found", id);
        }
    }

    #[allow(dead_code)]
    pub(super) fn apply_button_profile_by_id(&mut self, id: &str) {
        self.undo_keymaps.clear();
        self.redo_keymaps.clear();
        if let Some(b) = self.profile_manager.load_button_profile(id) {
            self.selected_button_profile = b.id.clone();
            let prof = (self.active_profile as usize).min(2);
            match b.base_layer.to_descriptors() {
                Ok(base_actions) => {
                    self.profiles[prof][0] = base_actions;
                }
                Err(e) => {
                    self.status_message = format!("Error resolving base keymap: {}", e);
                    return;
                }
            }
            match b.fn_layer.to_descriptors() {
                Ok(fn_actions) => {
                    self.profiles[prof][1] = fn_actions;
                }
                Err(e) => {
                    self.status_message = format!("Error resolving Fn keymap: {}", e);
                    return;
                }
            }

            if self.check_service() {
                match send_ipc_request(&IpcRequest::ApplyButtonProfile { id: id.to_string() }) {
                    Ok(IpcResponse::Ok) => {
                        self.status_message = format!("Service (IPC): Applied button profile '{}'", b.name);
                    }
                    Ok(resp) => {
                        self.status_message = format!("Service response: {:?}", resp);
                    }
                    Err(e) => {
                        self.status_message = format!("IPC error applying button profile: {}", e);
                    }
                }
            } else {
                self.apply_current_layer_to_hardware();
            }
        } else {
            self.status_message = format!("Button profile '{}' not found", id);
        }
    }

    #[allow(dead_code)]
    pub(super) fn apply_macro_profile_by_id(&mut self, id: &str) {
        if let Some(m) = self.profile_manager.load_macro_profile(id) {
            self.selected_macro_profile = m.id.clone();
            if let Some(first) = m.macros.first() {
                self.macro_sequence = first.clone();
                self.active_macro_slot = first.slot;
            }
            if self.check_service() {
                match send_ipc_request(&IpcRequest::ApplyMacroProfile { id: id.to_string() }) {
                    Ok(IpcResponse::Ok) => {
                        self.status_message = format!("Service (IPC): Applied macro profile '{}'", m.name);
                    }
                    Ok(resp) => {
                        self.status_message = format!("Service response: {:?}", resp);
                    }
                    Err(e) => {
                        self.status_message = format!("IPC error applying macro profile: {}", e);
                    }
                }
            } else {
                self.status_message = format!("Loaded macro profile '{}' ({} sequences)", m.name, m.macros.len());
            }
        } else {
            self.status_message = format!("Macro profile '{}' not found", id);
        }
    }

    pub(super) fn clone_active_button_profile(&mut self) {
        let current_id = self.selected_button_profile.clone();
        let base_profile = self.profile_manager.load_button_profile(&current_id)
            .or_else(|| self.profile_manager.load_button_profile("default"));
        let (base_layer, fn_layer, orig_name) = if let Some(ref bp) = base_profile {
            (bp.base_layer.clone(), bp.fn_layer.clone(), bp.name.clone())
        } else {
            let prof = (self.active_profile as usize).min(2);
            (
                LayerKeymapConfig::from_descriptors(&self.profiles[prof][0], KeyLayer::Base),
                LayerKeymapConfig::from_descriptors(&self.profiles[prof][1], KeyLayer::Fn),
                "Default".to_string(),
            )
        };
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() % 10000;
        let new_id = format!("{}_copy_{}", current_id, ts);
        let new_name = format!("{} (Copy)", orig_name);
        let new_profile = ButtonProfile {
            id: new_id.clone(),
            name: new_name,
            description: format!("Cloned from {}", orig_name),
            base_layer,
            fn_layer,
            is_builtin: false,
        };
        let _ = self.profile_manager.save_button_profile(&new_profile);
        self.apply_button_profile_by_id(&new_id);
        self.status_message = format!("Cloned profile '{}'", new_profile.name);
    }

    pub(super) fn create_new_button_profile(&mut self, name: &str, copy_current: bool) {
        let clean_name = name.trim();
        if clean_name.is_empty() {
            self.status_message = "Profile name cannot be empty".to_string();
            return;
        }
        let slug: String = clean_name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let slug_trimmed = slug.trim_matches('_');
        let base_slug = if slug_trimmed.is_empty() { "profile" } else { slug_trimmed };
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() % 100000;
        let new_id = format!("{}_{}", base_slug, ts);

        let (base_layer, fn_layer) = if copy_current {
            let prof = (self.active_profile as usize).min(2);
            (
                LayerKeymapConfig::from_descriptors(&self.profiles[prof][0], KeyLayer::Base),
                LayerKeymapConfig::from_descriptors(&self.profiles[prof][1], KeyLayer::Fn),
            )
        } else {
            (
                LayerKeymapConfig::from_descriptors(&default_layer_keymap(KeyLayer::Base), KeyLayer::Base),
                LayerKeymapConfig::from_descriptors(&default_layer_keymap(KeyLayer::Fn), KeyLayer::Fn),
            )
        };

        let new_profile = ButtonProfile {
            id: new_id.clone(),
            name: clean_name.to_string(),
            description: format!("Custom button profile: {}", clean_name),
            base_layer,
            fn_layer,
            is_builtin: false,
        };

        match self.profile_manager.save_button_profile(&new_profile) {
            Ok(_) => {
                self.apply_button_profile_by_id(&new_id);
                self.status_message = format!("Created new button profile '{}'", clean_name);
            }
            Err(e) => {
                self.status_message = format!("Failed to create profile '{}': {}", clean_name, e);
            }
        }
    }

    pub(super) fn delete_button_profile_by_id(&mut self, id: &str) {
        if let Some(p) = self.profile_manager.load_button_profile(id) {
            if p.is_builtin || id == "default" {
                self.status_message = format!("Cannot delete built-in profile '{}'", p.name);
                return;
            }
            match self.profile_manager.delete_button_profile(id) {
                Ok(true) => {
                    if self.selected_button_profile == id {
                        self.apply_button_profile_by_id("default");
                    }
                    self.status_message = format!("Deleted button profile '{}'", p.name);
                }
                Ok(false) => {
                    self.status_message = format!("Profile '{}' was not found on disk", p.name);
                }
                Err(e) => {
                    self.status_message = format!("Error deleting profile '{}': {}", p.name, e);
                }
            }
        }
    }

    pub(super) fn delete_active_button_profile(&mut self) {
        let current_id = self.selected_button_profile.clone();
        self.delete_button_profile_by_id(&current_id);
    }

    pub(super) fn reset_active_button_profile(&mut self) {
        self.undo_keymaps.clear();
        self.redo_keymaps.clear();
        let prof = (self.active_profile as usize).min(2);
        self.profiles[prof][0] = default_layer_keymap(KeyLayer::Base);
        self.profiles[prof][1] = default_layer_keymap(KeyLayer::Fn);
        let current_id = self.selected_button_profile.clone();
        if let Some(mut p) = self.profile_manager.load_button_profile(&current_id) {
            if !p.is_builtin {
                p.base_layer = LayerKeymapConfig::from_descriptors(&self.profiles[prof][0], KeyLayer::Base);
                p.fn_layer = LayerKeymapConfig::from_descriptors(&self.profiles[prof][1], KeyLayer::Fn);
                let _ = self.profile_manager.save_button_profile(&p);
            }
        }
        self.apply_current_layer_to_hardware();
        self.status_message = format!("Reset {:?} layer to factory defaults and applied to keyboard", self.active_layer);
    }

    pub(super) fn persist_current_button_profile(&mut self) {
        let current_id = self.selected_button_profile.clone();
        let prof = (self.active_profile as usize).min(2);
        let base_layer = LayerKeymapConfig::from_descriptors(&self.profiles[prof][0], KeyLayer::Base);
        let fn_layer = LayerKeymapConfig::from_descriptors(&self.profiles[prof][1], KeyLayer::Fn);
        if let Some(mut p) = self.profile_manager.load_button_profile(&current_id) {
            p.base_layer = base_layer;
            p.fn_layer = fn_layer;
            let _ = self.profile_manager.save_button_profile(&p);
        } else {
            let p = ButtonProfile {
                id: current_id.clone(),
                name: current_id.clone(),
                description: "Custom button profile".to_string(),
                base_layer,
                fn_layer,
                is_builtin: false,
            };
            let _ = self.profile_manager.save_button_profile(&p);
        }
    }

    pub(super) fn clone_active_lighting_profile(&mut self) {
        let current_id = self.selected_lighting_profile.clone();
        let base_profile = self.profile_manager.load_lighting_profile(&current_id)
            .or_else(|| self.profile_manager.load_lighting_profile("gaming"));
        let (mode, speed, brightness, direction, color, canvas, orig_name) = if let Some(ref lp) = base_profile {
            (lp.mode, lp.speed, lp.brightness, lp.direction, lp.color, lp.canvas.clone(), lp.name.clone())
        } else {
            let mut canvas_map = std::collections::HashMap::new();
            for (i, c) in self.key_colors.iter().enumerate() {
                if *c != Color32::BLACK {
                    canvas_map.insert(i, [c.r(), c.g(), c.b()]);
                }
            }
            (
                LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum),
                self.speed,
                self.brightness,
                self.direction,
                self.current_color,
                canvas_map,
                "Gaming".to_string(),
            )
        };
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() % 10000;
        let new_id = format!("{}_copy_{}", current_id, ts);
        let new_name = format!("{} (Copy)", orig_name);
        let new_profile = LightingProfile {
            id: new_id.clone(),
            name: new_name,
            description: format!("Cloned from {}", orig_name),
            mode,
            speed,
            brightness,
            direction,
            color,
            color_mode: Some(if self.is_fixed_color || self.selected_mode == 1 { ColorMode::FixedColor } else { ColorMode::DynamicSpectrum }),
            canvas,
            is_builtin: false,
        };
        let _ = self.profile_manager.save_lighting_profile(&new_profile);
        self.apply_lighting_profile_by_id(&new_id);
        self.status_message = format!("Cloned lighting profile '{}'", new_profile.name);
    }

    pub(super) fn create_new_lighting_profile(&mut self, name: &str, copy_current: bool) {
        let clean_name = name.trim();
        if clean_name.is_empty() {
            self.status_message = "Profile name cannot be empty".to_string();
            return;
        }
        let slug: String = clean_name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let slug_trimmed = slug.trim_matches('_');
        let base_slug = if slug_trimmed.is_empty() { "profile" } else { slug_trimmed };
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() % 100000;
        let new_id = format!("{}_{}", base_slug, ts);

        let (mode, speed, brightness, direction, color, color_mode, canvas) = if copy_current {
            let mut canvas_map = std::collections::HashMap::new();
            for (i, c) in self.key_colors.iter().enumerate() {
                if *c != Color32::BLACK {
                    canvas_map.insert(i, [c.r(), c.g(), c.b()]);
                }
            }
            (
                LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum),
                self.speed,
                self.brightness,
                self.direction,
                self.current_color,
                Some(if self.is_fixed_color || self.selected_mode == 1 { ColorMode::FixedColor } else { ColorMode::DynamicSpectrum }),
                canvas_map,
            )
        } else {
            (
                LightingMode::Spectrum,
                3,
                4,
                0,
                [255, 0, 0],
                Some(ColorMode::DynamicSpectrum),
                std::collections::HashMap::new(),
            )
        };

        let new_profile = LightingProfile {
            id: new_id.clone(),
            name: clean_name.to_string(),
            description: format!("Custom lighting profile: {}", clean_name),
            mode,
            speed,
            brightness,
            direction,
            color,
            color_mode,
            canvas,
            is_builtin: false,
        };

        match self.profile_manager.save_lighting_profile(&new_profile) {
            Ok(_) => {
                self.apply_lighting_profile_by_id(&new_id);
                self.status_message = format!("Created new lighting profile '{}'", clean_name);
            }
            Err(e) => {
                self.status_message = format!("Failed to create lighting profile '{}': {}", clean_name, e);
            }
        }
    }

    pub(super) fn delete_lighting_profile_by_id(&mut self, id: &str) {
        if let Some(p) = self.profile_manager.load_lighting_profile(id) {
            if p.is_builtin || id == "gaming" || id == "rainbow_wave" {
                self.status_message = format!("Cannot delete built-in profile '{}'", p.name);
                return;
            }
            match self.profile_manager.delete_lighting_profile(id) {
                Ok(true) => {
                    if self.selected_lighting_profile == id {
                        self.apply_lighting_profile_by_id("gaming");
                    }
                    self.status_message = format!("Deleted lighting profile '{}'", p.name);
                }
                Ok(false) => {
                    self.status_message = format!("Profile '{}' was not found on disk", p.name);
                }
                Err(e) => {
                    self.status_message = format!("Error deleting profile '{}': {}", p.name, e);
                }
            }
        }
    }

    pub(super) fn delete_active_lighting_profile(&mut self) {
        let current_id = self.selected_lighting_profile.clone();
        self.delete_lighting_profile_by_id(&current_id);
    }

    pub(super) fn reset_active_lighting_profile(&mut self) {
        self.selected_mode = 4; // Spectrum
        self.brightness = 4;
        self.speed = 3;
        self.direction = 0;
        self.current_color = [255, 0, 0];
        self.key_colors.fill(Color32::BLACK);
        let current_id = self.selected_lighting_profile.clone();
        if let Some(mut p) = self.profile_manager.load_lighting_profile(&current_id) {
            if !p.is_builtin {
                p.mode = LightingMode::Spectrum;
                p.brightness = 4;
                p.speed = 3;
                p.direction = 0;
                p.color = [255, 0, 0];
                p.canvas.clear();
                let _ = self.profile_manager.save_lighting_profile(&p);
            }
        }
        self.apply_lighting();
        self.status_message = "Reset lighting profile to default Spectrum mode".to_string();
    }

    pub(super) fn persist_current_lighting_profile(&mut self) {
        let current_id = self.selected_lighting_profile.clone();
        let mut canvas_map = std::collections::HashMap::new();
        for (i, c) in self.key_colors.iter().enumerate() {
            if *c != Color32::BLACK {
                canvas_map.insert(i, [c.r(), c.g(), c.b()]);
            }
        }
        let c_mode = if self.is_fixed_color || self.selected_mode == 1 {
            ColorMode::FixedColor
        } else {
            ColorMode::DynamicSpectrum
        };
        if let Some(mut p) = self.profile_manager.load_lighting_profile(&current_id) {
            p.mode = LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum);
            p.brightness = self.brightness;
            p.speed = self.speed;
            p.direction = self.direction;
            p.color = self.current_color;
            p.color_mode = Some(c_mode);
            p.canvas = canvas_map;
            let _ = self.profile_manager.save_lighting_profile(&p);
        } else {
            let p = LightingProfile {
                id: current_id.clone(),
                name: current_id.clone(),
                description: "Custom lighting profile".to_string(),
                mode: LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum),
                brightness: self.brightness,
                speed: self.speed,
                direction: self.direction,
                color: self.current_color,
                color_mode: Some(c_mode),
                canvas: canvas_map,
                is_builtin: false,
            };
            let _ = self.profile_manager.save_lighting_profile(&p);
        }
    }

    pub(super) fn export_button_profile_to_path(&mut self, path: &std::path::Path) {
        let prof = (self.active_profile as usize).min(2);
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("button_profile");
        let id = name.trim().to_lowercase().replace(' ', "_");
        let button_profile = ButtonProfile {
            id: id.clone(),
            name: name.to_string(),
            description: format!("FantasTech exported button profile {}", name),
            base_layer: LayerKeymapConfig::from_descriptors(&self.profiles[prof][0], KeyLayer::Base),
            fn_layer: LayerKeymapConfig::from_descriptors(&self.profiles[prof][1], KeyLayer::Fn),
            is_builtin: false,
        };
        match serde_json::to_string_pretty(&button_profile) {
            Ok(json_str) => match fantastech_core::write_user_owned_file(path, json_str.as_bytes()) {
                Ok(_) => {
                    let _ = self.profile_manager.save_button_profile(&button_profile);
                    self.selected_button_profile = id;
                    self.status_message = format!("Exported and saved button profile to '{}'", path.display());
                }
                Err(e) => {
                    self.status_message = format!("Failed to write button profile to '{}': {}", path.display(), e);
                }
            },
            Err(e) => {
                self.status_message = format!("Failed to serialize button profile: {}", e);
            }
        }
    }

    pub(super) fn import_any_profile_from_path(&mut self, path: &std::path::Path) -> bool {
        let json_str = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                self.status_message = format!("Failed to read profile file '{}': {}", path.display(), e);
                return false;
            }
        };

        // 1. Try parsing as LightingProfile
        if let Ok(lp) = serde_json::from_str::<LightingProfile>(&json_str) {
            self.selected_lighting_profile = lp.id.clone();
            self.selected_mode = lp.mode as u8;
            self.brightness = lp.brightness;
            self.speed = lp.speed;
            self.direction = lp.direction;
            self.current_color = lp.color;
            for (key_idx, rgb) in &lp.canvas {
                if *key_idx < SPARSE_KEY_COUNT {
                    self.key_colors[*key_idx] = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                }
            }
            let _ = self.profile_manager.save_lighting_profile(&lp);
            self.apply_lighting();
            self.status_message = format!("Loaded Lighting profile '{}' from '{}'", lp.name, path.display());
            return true;
        }

        // 2. Try parsing as ButtonProfile
        if let Ok(bp) = serde_json::from_str::<ButtonProfile>(&json_str) {
            self.undo_keymaps.clear();
            self.redo_keymaps.clear();
            let prof = (self.active_profile as usize).min(2);
            let mut valid = true;
            match bp.base_layer.to_descriptors() {
                Ok(base_actions) => self.profiles[prof][0] = base_actions,
                Err(e) => {
                    self.status_message = format!("Error loading Base layer from '{}': {}", path.display(), e);
                    valid = false;
                }
            }
            if valid {
                match bp.fn_layer.to_descriptors() {
                    Ok(fn_actions) => self.profiles[prof][1] = fn_actions,
                    Err(e) => {
                        self.status_message = format!("Error loading Fn layer from '{}': {}", path.display(), e);
                        valid = false;
                    }
                }
            }
            if valid {
                let _ = self.profile_manager.save_button_profile(&bp);
                self.selected_button_profile = bp.id.clone();
                self.status_message = format!("Loaded Button profile '{}' from '{}'", bp.name, path.display());
                return true;
            }
        }

        // 3. Try parsing as MacroProfile
        if let Ok(mp) = serde_json::from_str::<MacroProfile>(&json_str) {
            self.selected_macro_profile = mp.id.clone();
            if let Some(first) = mp.macros.first() {
                self.macro_sequence = first.clone();
                self.active_macro_slot = first.slot;
            }
            let _ = self.profile_manager.save_macro_profile(&mp);
            self.status_message = format!("Loaded Macro profile '{}' from '{}'", mp.name, path.display());
            return true;
        }

        // 4. Try parsing as FullDeviceBackup
        if let Ok(backup) = serde_json::from_str::<FullDeviceBackup>(&json_str) {
            self.selected_mode = backup.lighting_config.mode as u8;
            self.brightness = backup.lighting_config.brightness;
            self.speed = backup.lighting_config.speed;
            self.direction = backup.lighting_config.direction as u8;
            self.current_color = [backup.lighting_config.color.r, backup.lighting_config.color.g, backup.lighting_config.color.b];
            let prof = (self.active_profile as usize).min(2);
            for (idx, action) in backup.base_keymap.iter().enumerate().take(SPARSE_KEY_COUNT) {
                self.profiles[prof][0][idx] = *action;
            }
            for (idx, action) in backup.fn_keymap.iter().enumerate().take(SPARSE_KEY_COUNT) {
                self.profiles[prof][1][idx] = *action;
            }
            self.status_message = format!("Restored device backup from '{}'", path.display());
            return true;
        }

        self.status_message = format!("File '{}' is not a valid FantasTech profile (buttons, lighting, or macro)", path.display());
        false
    }

    pub(super) fn import_button_profile_from_path(&mut self, path: &std::path::Path) {
        self.import_any_profile_from_path(path);
    }

    pub(super) fn export_lighting_profile_to_path(&mut self, path: &std::path::Path) {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("lighting_profile");
        let id = name.trim().to_lowercase().replace(' ', "_");
        let mut canvas_map = std::collections::HashMap::new();
        for (i, c) in self.key_colors.iter().enumerate() {
            if *c != Color32::BLACK {
                canvas_map.insert(i, [c.r(), c.g(), c.b()]);
            }
        }
        let lp = LightingProfile {
            id: id.clone(),
            name: name.to_string(),
            description: format!("FantasTech exported lighting profile {}", name),
            mode: LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum),
            speed: self.speed,
            brightness: self.brightness,
            direction: self.direction,
            color: self.current_color,
            color_mode: Some(if self.is_fixed_color || self.selected_mode == 1 { ColorMode::FixedColor } else { ColorMode::DynamicSpectrum }),
            canvas: canvas_map,
            is_builtin: false,
        };
        match serde_json::to_string_pretty(&lp) {
            Ok(json_str) => match fantastech_core::write_user_owned_file(path, json_str.as_bytes()) {
                Ok(_) => {
                    let _ = self.profile_manager.save_lighting_profile(&lp);
                    self.selected_lighting_profile = id;
                    self.status_message = format!("Exported and saved lighting profile to '{}'", path.display());
                }
                Err(e) => {
                    self.status_message = format!("Failed to write lighting profile to '{}': {}", path.display(), e);
                }
            },
            Err(e) => {
                self.status_message = format!("Failed to serialize lighting profile: {}", e);
            }
        }
    }

    pub(super) fn import_lighting_profile_from_path(&mut self, path: &std::path::Path) {
        self.import_any_profile_from_path(path);
    }

    pub(super) fn export_macro_profile_to_path(&mut self, path: &std::path::Path) {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("macro_profile");
        let id = name.trim().to_lowercase().replace(' ', "_");
        let mp = MacroProfile {
            id: id.clone(),
            name: name.to_string(),
            description: format!("FantasTech exported macro profile {}", name),
            macros: vec![self.macro_sequence.clone()],
            is_builtin: false,
        };
        match serde_json::to_string_pretty(&mp) {
            Ok(json_str) => match fantastech_core::write_user_owned_file(path, json_str.as_bytes()) {
                Ok(_) => {
                    self.selected_macro_profile = id;
                    self.status_message = format!("Exported macro profile to '{}'", path.display());
                }
                Err(e) => {
                    self.status_message = format!("Failed to write macro profile to '{}': {}", path.display(), e);
                }
            },
            Err(e) => {
                self.status_message = format!("Failed to serialize macro profile: {}", e);
            }
        }
    }

    pub(super) fn import_macro_profile_from_path(&mut self, path: &std::path::Path) {
        self.import_any_profile_from_path(path);
    }

    #[allow(dead_code)]
    pub(super) fn apply_settings_profile_by_id(&mut self, id: &str) {
        if let Some(s) = self.profile_manager.load_settings_profile(id) {
            self.selected_settings_profile = s.id.clone();
            self.sleep_timeout_mins = s.sleep_timeout_mins;
            if self.check_service() {
                match send_ipc_request(&IpcRequest::ApplySettingsProfile { id: id.to_string() }) {
                    Ok(IpcResponse::Ok) => {
                        self.status_message = format!("Service (IPC): Applied settings profile '{}'", s.name);
                    }
                    Ok(resp) => {
                        self.status_message = format!("Service response: {:?}", resp);
                    }
                    Err(e) => {
                        self.status_message = format!("IPC error applying settings profile: {}", e);
                    }
                }
            } else {
                self.apply_sleep_timeout(s.sleep_timeout_mins);
            }
        } else {
            self.status_message = format!("Settings profile '{}' not found", id);
        }
    }

    pub(super) fn flash_macro_slot(&mut self) {
        let slot = self.active_macro_slot;
        let mut next_free = 64u16;
        for s in 0..32 {
            if s != slot {
                if let Ok(p) = self.macro_memory.get_slot_pointer(s) {
                    if p != 0xFFFF && p >= 64 {
                        if let Ok(recs) = self.macro_memory.decompile_slot(s) {
                            let end_p = p + (recs.len() as u16) * 4;
                            if end_p > next_free {
                                next_free = end_p;
                            }
                        }
                    }
                }
            }
        }

        self.macro_sequence.slot = slot;
        if let Err(e) = self.macro_sequence.compile_into_table(&mut self.macro_memory, next_free) {
            self.status_message = format!("Macro compilation error: {:?}", e);
            return;
        }

        if self.dry_run_enabled {
            self.status_message = format!("[DRY-RUN] Compiled Macro M{} ({} steps)", slot, self.macro_sequence.steps.len());
            return;
        }

        if let Some(ref path) = self.permission_denied_node {
            self.status_message = platform::permission_error("flash the macro to the keyboard", path.as_path());
        } else if self.is_emulator {
            self.status_message = format!("Virtual Emulator: Macro M{} flashed ({} steps)", slot, self.macro_sequence.steps.len());
        } else if let Ok(t) = HidrawTransport::discover() {
            let dev = Device::new_read_only(t);
            match dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE") {
                Ok(mut persistent_dev) => {
                    persistent_dev.set_dry_run(false);
                    match persistent_dev.write_macro_memory(self.macro_memory.as_bytes()) {
                        Ok(_) => {
                            self.status_message = format!("Hardware: Macro M{} written to onboard flash ({} steps)", slot, self.macro_sequence.steps.len());
                        }
                        Err(e) => {
                            self.status_message = format!("Hardware macro write error: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    self.status_message = format!("Safety Authorization error: {:?}", e);
                }
            }
        }
    }
}

