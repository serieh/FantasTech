//! High-level abstractions for themes, keymap naming, and the 4KB macro compiler.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

pub mod actions;
pub mod config;
pub mod error;
pub mod firmware;
pub mod ipc;
pub mod key_definitions;
pub mod keymap_config;
pub mod macros;
pub mod profiles;
pub mod scancodes;
pub mod single_instance;
pub mod theme;

#[cfg(test)]
mod tests;

pub use actions::{default_layer_keymap, format_action_str, parse_action_str, parse_key_identifier};
pub use config::{get_fantastech_config_dir, ColorDisplayFormat, FantasTechConfig, GuiTheme};
pub use error::CoreError;
pub use firmware::{
    check_firmware_update_available, compute_sha256, get_catalog_entry,
    get_embedded_firmware_bytes, validate_firmware_binary, BOOTLOADER_OFFSET, BOOTLOADER_PID,
    CATALOG_PID, CATALOG_SHA256, CATALOG_VERSION_CODE, CATALOG_VERSION_STR, CATALOG_VID,
    EMBEDDED_FIRMWARE_V101, EXPECTED_FULL_IMAGE_LEN, EXPECTED_PAYLOAD_LEN, FirmwareCatalogEntry,
    FirmwareValidationInfo,
};
pub use ipc::{
    ipc_socket_path, is_service_running, send_ipc_request, IpcRequest, IpcResponse, ServiceStatus,
    WINDOWS_GUI_PORT, WINDOWS_IPC_PORT, WINDOWS_TRAY_PORT,
};
pub use key_definitions::{
    get_mirror_key_index, keys_for_row, keys_for_zone, KeyEntry, KEY_DEFINITIONS,
};
pub use keymap_config::{LayerKeymapConfig, ProfileKeymapConfig};
pub use macros::{MacroItem, MacroSequence};
pub use profiles::{
    write_user_owned_file, ButtonProfile, LightingProfile, MacroProfile, ProfileManager,
    SettingsProfile,
};
pub use scancodes::{
    default_scancode_for_key, key_name_for_scancode, media_name_for_usage, scancode_for_key_name,
};
pub use single_instance::{
    focus_existing_gui, focus_existing_gui_with_action, is_tray_already_running,
    single_instance_socket_path, tray_socket_path, SingleInstanceGuard, TrayInstanceGuard,
};
pub use theme::ThemeConfig;

/// Installs a global panic hook that writes crash diagnostics to `crash.log` in app data.
pub fn install_crash_handler(component: &'static str) {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let crash_log_path = get_fantastech_config_dir().join("crash.log");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let payload = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "Unknown panic payload"
        };
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed");

        let log_entry = format!(
            "[timestamp: {}] CRASH in '{}' on thread '{}' at {}:\nPayload: {}\n\n",
            now, component, thread_name, location, payload
        );

        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&crash_log_path)
        {
            use std::io::Write;
            let _ = file.write_all(log_entry.as_bytes());
        }

        original_hook(panic_info);
    }));
}
