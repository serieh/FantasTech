//! Native GUI and system tray launch dispatchers.


use crate::tray;
use fantastech_core::FantasTechConfig;

pub(crate) fn handle_gui_launch(update: bool) -> Result<(), Box<dyn std::error::Error>> {
    let action = if update { "UPDATE" } else { "FOCUS" };
    if fantastech_core::focus_existing_gui_with_action(action) {
        println!("ℹ️ FantasTech is already running. Brought existing window to focus.");
        return Ok(());
    }

    println!("🖥️  Launching FantasTech Graphical Interface...");
    let cfg = FantasTechConfig::load();
    if cfg.tray_icon_enabled && !fantastech_core::is_service_running() && !fantastech_core::is_tray_already_running() {
        tray::spawn_native_tray();
    }
    match fantastech_gui::run_gui() {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("\n============================================================");
            eprintln!("⚠️  Graphics Display Initialization Notice");
            eprintln!("============================================================");
            eprintln!("The graphical interface encountered an error: {}", e);
            eprintln!("If running inside a Virtual Machine, ensure 3D acceleration");
            eprintln!("is enabled in VM settings, or use FantasTech via the CLI:");
            eprintln!();
            eprintln!("  • fantastech status               View device & service status");
            eprintln!("  • fantastech profile list         List saved profiles");
            eprintln!("  • fantastech profile apply <id>   Activate a lighting/keymap profile");
            eprintln!("  • fantastech lighting list        List all 17 lighting modes");
            eprintln!("  • fantastech lighting set <mode>  Apply hardware lighting directly");
            eprintln!("  • fantastech service run          Run background daemon & OpenRGB server");
            eprintln!("============================================================\n");

            #[cfg(target_os = "windows")]
            show_windows_graphics_error_dialog(&e.to_string());

            Err(format!("GUI exited with error: {}", e).into())
        }
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
pub(crate) fn show_windows_graphics_error_dialog(err_msg: &str) {
    use std::os::windows::ffi::OsStrExt;
    let title: Vec<u16> = std::ffi::OsStr::new("FantasTech — Graphics Notice")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let msg = format!(
        "FantasTech Graphical Interface could not initialize:\n\n{}\n\n\
        Note: If running inside a Virtual Machine or Remote Desktop, ensure \
        '3D Acceleration' is enabled in your VM Display Settings.\n\n\
        FantasTech is fully functional from the Command Prompt (CLI):\n\
        • fantastech status\n\
        • fantastech lighting list\n\
        • fantastech service run",
        err_msg
    );
    let msg_wide: Vec<u16> = std::ffi::OsStr::new(&msg)
        .encode_wide()
        .chain(Some(0))
        .collect();

    extern "system" {
        fn MessageBoxW(
            hwnd: *mut std::ffi::c_void,
            lpText: *const u16,
            lpCaption: *const u16,
            uType: u32,
        ) -> i32;
    }
    const MB_OK: u32 = 0x00000000;
    const MB_ICONWARNING: u32 = 0x00000030;
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            msg_wide.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONWARNING,
        );
    }
}

pub(crate) fn handle_tray_launch() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔔 Running FantasTech Native System Tray...");
    tray::run_tray_loop()
}
