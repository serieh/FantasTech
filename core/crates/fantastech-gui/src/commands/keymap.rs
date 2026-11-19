//! Key remapping, layer state, and bounded undo/redo history.

use super::*;
use fantastech_protocol::keymap::KeyAction;

impl FantasTechApp {
    pub(crate) fn record_keymap_undo(&mut self) {
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

    pub(crate) fn undo_keymap_edit(&mut self) {
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

    pub(crate) fn redo_keymap_edit(&mut self) {
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

    pub(crate) fn remap_single_key(&mut self, key_index: u8, action: KeyAction) {
        if self.active_layer == KeyLayer::Base && action == KeyAction::hardware_custom(16) {
            self.status_message = "Cannot assign Hardware Reset to Base layer keys (intended for Fn layer only).".to_string();
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
        if self.dry_run_enabled || cfg!(test) {
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
        } else {
            self.status_message = format!("Key {} remapped to {} (click Apply to save to keyboard)", key_index, action_str);
        }
    }

    pub(crate) fn apply_current_layer_to_hardware(&mut self) {
        let prof = (self.active_profile as usize).min(2);
        let lay = self.active_layer as usize;
        let actions = self.profiles[prof][lay];

        if self.dry_run_enabled || cfg!(test) {
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

}
