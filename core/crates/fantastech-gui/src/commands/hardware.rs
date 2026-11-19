//! Hardware reset, active profile switching, sleep timeout, and macro memory flashing.

use super::*;
use fantastech_core::ipc::IpcRequest;

impl FantasTechApp {
    pub(crate) fn set_active_profile(&mut self, p: u8) {
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

    pub(crate) fn apply_hardware_snapshot(&mut self, snapshot: &platform::ConnectionSnapshot) {
        if let Some(ref info) = snapshot.device_info {
            self.firmware_version = info.firmware_version.clone();
            self.active_profile = info.active_profile.min(2);
        }

        if let Some((base, fn_layer)) = snapshot.hardware_keymaps {
            let p_idx = (self.active_profile as usize).min(2);
            self.profiles[p_idx][0] = base;
            self.profiles[p_idx][1] = fn_layer;

            // Check if hardware keymap matches an existing button profile
            let b_profiles = self.profile_manager.list_button_profiles();
            if let Some(matching) = b_profiles.iter().find(|bp| {
                if let Some(loaded) = self.profile_manager.load_button_profile(&bp.id) {
                    loaded.base_layer.to_descriptors().ok() == Some(base)
                        && loaded.fn_layer.to_descriptors().ok() == Some(fn_layer)
                } else {
                    false
                }
            }) {
                self.selected_button_profile = matching.id.clone();
            }
        }

        if let Some(ref st) = snapshot.service_status {
            if !st.active_button_profile.is_empty() {
                self.selected_button_profile = st.active_button_profile.clone();
            }
            if !st.active_lighting_profile.is_empty() {
                self.selected_lighting_profile = st.active_lighting_profile.clone();
            }
        }

        if let Some(cfg) = snapshot.lighting_config {
            self.apply_queried_lighting_config(cfg);
        }

        let mode_name = fantastech_protocol::lighting::LightingMode::from_u8(self.selected_mode)
            .map(|m| m.name())
            .unwrap_or("Custom");
        self.status_message = format!(
            "Synced with hardware: {} (FW {}, Profile {}, Mode: {})",
            self.device_model,
            self.firmware_version,
            self.active_profile + 1,
            mode_name
        );
    }

    pub(crate) fn execute_factory_reset(&mut self) {
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

    pub(crate) fn apply_sleep_timeout(&mut self, minutes: u16) {
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

    pub(crate) fn wipe_all_macros(&mut self) {
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

    pub(crate) fn flash_macro_slot(&mut self) {
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
