//! Domain command modules and operations on FantasTechApp state.

pub mod backup;
pub mod hardware;
pub mod keymap;
pub mod lighting;
pub mod profiles;
pub mod system;

use super::*;

impl FantasTechApp {
    pub(crate) fn check_service(&mut self) -> bool {
        // Connection state is refreshed by the background monitor. Never block
        // the egui thread with service or HID discovery from a UI callback.
        self.service_active
    }
}
