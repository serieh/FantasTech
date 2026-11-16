//! Windows native System Tray implementation using Win32 `Shell_NotifyIconW`.
//!
//! Provides a seamless Windows taskbar notification area icon with full
//! context menu support:
//! - Left-click / double-click: Focuses or launches FantasTech GUI.
//! - Right-click menu:
//!   * FantasTech — MK894 (Status / Header)
//!   * Show FantasTech
//!   * Lighting Profiles (Gaming, Cyberpunk, Rainbow Wave, Neon Sunset, Matrix, Static Purple)
//!   * Lighting Effects (Static, Breathing, Wave, Spectrum, Reactive, Ripple, etc.)
//!   * Quick Colors (Electric Blue, Red, Green, Blue, Cyan, Magenta, Yellow, Orange, White)
//!   * Lighting Off
//!   * Exit FantasTech

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::process::Command;
use fantastech_protocol::lighting::{AnimationDirection, ColorMode, LightingConfig, LightingMode, LightingZone, RgbColor};

const EMBEDDED_ICON_BYTES: &[u8] = include_bytes!("../../../../assets/icons/fantastech.ico");

const WM_APP: u32 = 0x8000;
const WM_TRAY_CALLBACK: u32 = WM_APP + 1;

const WM_COMMAND: u32 = 0x0111;
const WM_DESTROY: u32 = 0x0002;
const WM_LBUTTONUP: usize = 0x0202;
const WM_LBUTTONDBLCLK: usize = 0x0203;
const WM_RBUTTONUP: usize = 0x0205;
const WM_CONTEXTMENU: usize = 0x007B;

const NIM_ADD: u32 = 0x00000000;
const NIM_DELETE: u32 = 0x00000002;

const NIF_MESSAGE: u32 = 0x00000001;
const NIF_ICON: u32 = 0x00000002;
const NIF_TIP: u32 = 0x00000004;

const MF_STRING: u32 = 0x00000000;
const MF_POPUP: u32 = 0x00000010;
const MF_SEPARATOR: u32 = 0x00000800;
const MF_GRAYED: u32 = 0x00000001;
const MF_CHECKED: u32 = 0x00000008;
const MF_UNCHECKED: u32 = 0x00000000;

const TPM_BOTTOMALIGN: u32 = 0x0020;
const TPM_LEFTALIGN: u32 = 0x0000;
const TPM_RIGHTBUTTON: u32 = 0x0002;

const IDI_APPLICATION: usize = 32512;

// Menu Command IDs
const ID_HEADER: usize = 100;
const ID_SHOW: usize = 101;
const ID_LIGHTING_OFF: usize = 102;
const ID_EXIT: usize = 103;
const ID_SERVICE_TOGGLE: usize = 104;
const ID_OPENRGB_TOGGLE: usize = 106;

const ID_PROFILE_BASE: usize = 200;
const ID_EFFECT_BASE: usize = 300;
const ID_COLOR_BASE: usize = 400;

#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
struct MSG {
    hwnd: *mut std::ffi::c_void,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: POINT,
}

#[repr(C)]
struct WNDCLASSEXW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(*mut std::ffi::c_void, u32, usize, isize) -> isize>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: *mut std::ffi::c_void,
    h_icon: *mut std::ffi::c_void,
    h_cursor: *mut std::ffi::c_void,
    hbr_background: *mut std::ffi::c_void,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: *mut std::ffi::c_void,
}

#[repr(C)]
struct NOTIFYICONDATAW {
    cb_size: u32,
    h_wnd: *mut std::ffi::c_void,
    u_id: u32,
    u_flags: u32,
    u_callback_message: u32,
    h_icon: *mut std::ffi::c_void,
    sz_tip: [u16; 128],
    dw_state: u32,
    dw_state_mask: u32,
    sz_info: [u16; 256],
    u_timeout_or_version: u32,
    sz_info_title: [u16; 64],
    dw_info_flags: u32,
    guid_item: [u8; 16],
    h_balloon_icon: *mut std::ffi::c_void,
}

#[link(name = "user32")]
#[link(name = "shell32")]
#[link(name = "kernel32")]
extern "system" {
    fn RegisterClassExW(lp_wnd_class: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        dw_ex_style: u32,
        lp_class_name: *const u16,
        lp_window_name: *const u16,
        dw_style: u32,
        x: i32,
        y: i32,
        n_width: i32,
        n_height: i32,
        h_wnd_parent: *mut std::ffi::c_void,
        h_menu: *mut std::ffi::c_void,
        h_instance: *mut std::ffi::c_void,
        lp_param: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
    fn DestroyWindow(h_wnd: *mut std::ffi::c_void) -> i32;
    fn DefWindowProcW(h_wnd: *mut std::ffi::c_void, msg: u32, w_param: usize, l_param: isize) -> isize;
    fn GetMessageW(lp_msg: *mut MSG, h_wnd: *mut std::ffi::c_void, w_msg_filter_min: u32, w_msg_filter_max: u32) -> i32;
    fn TranslateMessage(lp_msg: *const MSG) -> i32;
    fn DispatchMessageW(lp_msg: *const MSG) -> isize;
    fn PostQuitMessage(n_exit_code: i32);
    fn GetCursorPos(lp_point: *mut POINT) -> i32;
    fn SetForegroundWindow(h_wnd: *mut std::ffi::c_void) -> i32;
    fn CreatePopupMenu() -> *mut std::ffi::c_void;
    fn AppendMenuW(h_menu: *mut std::ffi::c_void, u_flags: u32, u_id_new_item: usize, lp_new_item: *const u16) -> i32;
    fn TrackPopupMenu(
        h_menu: *mut std::ffi::c_void,
        u_flags: u32,
        x: i32,
        y: i32,
        n_reserved: i32,
        h_wnd: *mut std::ffi::c_void,
        prc_rect: *const std::ffi::c_void,
    ) -> i32;
    fn DestroyMenu(h_menu: *mut std::ffi::c_void) -> i32;
    fn LoadIconW(h_instance: *mut std::ffi::c_void, lp_icon_name: *const u16) -> *mut std::ffi::c_void;
    fn LoadImageW(
        h_inst: *mut std::ffi::c_void,
        name: *const u16,
        type_: u32,
        cx: i32,
        cy: i32,
        fu_load: u32,
    ) -> *mut std::ffi::c_void;
    fn CreateIconFromResourceEx(
        pb_icon_bits: *const u8,
        cb_icon_bits: u32,
        f_icon: i32,
        dw_version: u32,
        cx_desired: i32,
        cy_desired: i32,
        u_flags: u32,
    ) -> *mut std::ffi::c_void;
    fn GetModuleHandleW(lp_module_name: *const u16) -> *mut std::ffi::c_void;
    fn ExtractIconW(
        h_inst: *mut std::ffi::c_void,
        lpsz_exe_file: *const u16,
        n_icon_index: u32,
    ) -> *mut std::ffi::c_void;
    fn Shell_NotifyIconW(dw_message: u32, lp_data: *mut NOTIFYICONDATAW) -> i32;
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

#[allow(unsafe_code)]
fn create_icon_from_embedded(ico_bytes: &[u8]) -> Option<*mut std::ffi::c_void> {
    if ico_bytes.len() < 6 {
        return None;
    }
    let count = u16::from_le_bytes([ico_bytes[4], ico_bytes[5]]) as usize;
    if count == 0 || ico_bytes.len() < 6 + count * 16 {
        return None;
    }

    let mut best_entry = None;
    for i in 0..count {
        let entry_offset = 6 + i * 16;
        let w = ico_bytes[entry_offset];
        let h = ico_bytes[entry_offset + 1];
        let bytes_in_res = u32::from_le_bytes([
            ico_bytes[entry_offset + 8],
            ico_bytes[entry_offset + 9],
            ico_bytes[entry_offset + 10],
            ico_bytes[entry_offset + 11],
        ]) as usize;
        let img_offset = u32::from_le_bytes([
            ico_bytes[entry_offset + 12],
            ico_bytes[entry_offset + 13],
            ico_bytes[entry_offset + 14],
            ico_bytes[entry_offset + 15],
        ]) as usize;

        if img_offset + bytes_in_res <= ico_bytes.len() {
            if w == 16 && h == 16 {
                best_entry = Some((img_offset, bytes_in_res));
                break;
            } else if w == 32 && h == 32 && best_entry.is_none() {
                best_entry = Some((img_offset, bytes_in_res));
            } else if best_entry.is_none() {
                best_entry = Some((img_offset, bytes_in_res));
            }
        }
    }

    if let Some((offset, size)) = best_entry {
        let hicon = unsafe {
            CreateIconFromResourceEx(
                ico_bytes[offset..].as_ptr(),
                size as u32,
                1, // TRUE for icon
                0x00030000,
                16,
                16,
                0,
            )
        };
        if !hicon.is_null() {
            return Some(hicon);
        }
    }
    None
}

#[allow(unsafe_code)]
fn load_tray_icon() -> *mut std::ffi::c_void {
    // 1. Try loading from executable directory or installed icon paths
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let candidates = [
                parent.join("icons").join("fantastech.ico"),
                parent.join("fantastech.ico"),
                parent.join("..").join("..").join("assets").join("icons").join("fantastech.ico"),
                parent.join("..").join("..").join("..").join("assets").join("icons").join("fantastech.ico"),
                PathBuf::from("assets/icons/fantastech.ico"),
            ];
            for path in candidates {
                if path.exists() {
                    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
                    let hicon = unsafe {
                        LoadImageW(
                            std::ptr::null_mut(),
                            wide.as_ptr(),
                            1, // IMAGE_ICON
                            16, // SM_CXSMICON
                            16, // SM_CYSMICON
                            0x00000010, // LR_LOADFROMFILE
                        )
                    };
                    if !hicon.is_null() {
                        return hicon;
                    }
                }
            }
        }
    }

    // 2. Try loading embedded resource from PE binary (ID 1)
    let h_module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let res_icon = unsafe { LoadIconW(h_module, 1 as *const u16) };
    if !res_icon.is_null() {
        return res_icon;
    }

    // 3. Try ExtractIconW from running executable
    if let Ok(exe_path) = std::env::current_exe() {
        let wide: Vec<u16> = exe_path.as_os_str().encode_wide().chain(Some(0)).collect();
        let extracted = unsafe { ExtractIconW(h_module, wide.as_ptr(), 0) };
        if !extracted.is_null() && extracted as usize != 1 {
            return extracted;
        }
    }

    // 4. Extract 16x16 icon directly from embedded ICO bytes in memory
    if let Some(hicon) = create_icon_from_embedded(EMBEDDED_ICON_BYTES) {
        return hicon;
    }

    // 5. Ultimate fallback: system application icon
    unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION as *const u16) }
}

/// Spawns the native tray in a detached background OS process if not already running
pub fn spawn_native_tray() {
    if fantastech_core::is_tray_already_running() {
        return;
    }

    let bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech.exe"));
    let mut cmd = Command::new(bin);
    cmd.arg("tray");
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x01000000;
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
    let _ = cmd.spawn();
}

/// Runs the native Win32 system tray message loop synchronously.
#[allow(unsafe_code)]
pub fn run_tray_loop() -> Result<(), Box<dyn std::error::Error>> {
    let _tray_guard = match fantastech_core::TrayInstanceGuard::bind() {
        Some(guard) => guard,
        None => {
            println!("ℹ️ FantasTech system tray is already active in another process.");
            return Ok(());
        }
    };

    println!("🔔 Running FantasTech Native Windows System Tray...");

    let class_name = to_wide("FantasTechTrayClass");
    let window_title = to_wide("FantasTech Tray Window");

    let wnd_class = WNDCLASSEXW {
        cb_size: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfn_wnd_proc: Some(wnd_proc),
        cb_cls_extra: 0,
        cb_wnd_extra: 0,
        h_instance: std::ptr::null_mut(),
        h_icon: std::ptr::null_mut(),
        h_cursor: std::ptr::null_mut(),
        hbr_background: std::ptr::null_mut(),
        lpsz_menu_name: std::ptr::null(),
        lpsz_class_name: class_name.as_ptr(),
        h_icon_sm: std::ptr::null_mut(),
    };

    unsafe {
        RegisterClassExW(&wnd_class);
    }

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };

    if hwnd.is_null() {
        return Err("Failed to create hidden tray window".into());
    }

    let icon = load_tray_icon();

    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cb_size = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.h_wnd = hwnd;
    nid.u_id = 1001;
    nid.u_flags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.u_callback_message = WM_TRAY_CALLBACK;
    nid.h_icon = icon;

    let tip = to_wide("FantasTech — Fantech MK894 Control Center");
    for (i, &ch) in tip.iter().take(127).enumerate() {
        nid.sz_tip[i] = ch;
    }

    let mut add_ok = unsafe { Shell_NotifyIconW(NIM_ADD, &mut nid) };
    if add_ok == 0 {
        // Fallback for older Shell32 struct sizes
        nid.cb_size = if std::mem::size_of::<usize>() == 8 { 952 } else { 936 };
        add_ok = unsafe { Shell_NotifyIconW(NIM_ADD, &mut nid) };
        if add_ok == 0 {
            nid.cb_size = if std::mem::size_of::<usize>() == 8 { 504 } else { 488 };
            unsafe { Shell_NotifyIconW(NIM_ADD, &mut nid) };
        }
    }

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &mut nid);
        DestroyWindow(hwnd);
    }

    Ok(())
}

#[allow(unsafe_code)]
unsafe extern "system" fn wnd_proc(hwnd: *mut std::ffi::c_void, msg: u32, w_param: usize, l_param: isize) -> isize {
    match msg {
        WM_TRAY_CALLBACK => {
            let event = l_param as usize;
            if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                focus_or_spawn_gui();
            } else if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
                show_tray_context_menu(hwnd);
            }
            0
        }
        WM_COMMAND => {
            let cmd_id = (w_param & 0xFFFF) as usize;
            handle_tray_command(cmd_id, hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w_param, l_param),
    }
}

fn focus_or_spawn_gui() {
    if !fantastech_core::focus_existing_gui() {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech.exe"));
        let _ = Command::new(exe).arg("gui").spawn();
    }
}

#[allow(unsafe_code)]
unsafe fn show_tray_context_menu(hwnd: *mut std::ffi::c_void) {
    let mut pt: POINT = std::mem::zeroed();
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);

    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }

    // Title / Header
    AppendMenuW(menu, MF_STRING | MF_GRAYED, ID_HEADER, to_wide("FantasTech — MK894").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

    // Show FantasTech GUI
    AppendMenuW(menu, MF_STRING, ID_SHOW, to_wide("Show FantasTech").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

    // Submenu: Lighting Profiles
    let profiles_menu = CreatePopupMenu();
    let profiles = [
        (1, "Gaming"),
        (2, "Cyberpunk"),
        (3, "Rainbow Wave"),
        (4, "Neon Sunset"),
        (5, "Matrix"),
        (6, "Static Purple"),
    ];
    for (id, label) in profiles {
        AppendMenuW(profiles_menu, MF_STRING, ID_PROFILE_BASE + id, to_wide(label).as_ptr());
    }
    AppendMenuW(menu, MF_POPUP, profiles_menu as usize, to_wide("Lighting Profiles ▶").as_ptr());

    // Submenu: Lighting Effects
    let effects_menu = CreatePopupMenu();
    let effects = [
        (1, "Static"),
        (2, "Breathing"),
        (3, "Rotate"),
        (4, "Spectrum"),
        (5, "Stream Left to Right"),
        (6, "Bloom"),
        (7, "Stream Up to Down"),
        (8, "Shuttle"),
        (9, "Raindrop"),
        (10, "Meteor"),
        (11, "Ripple Fade"),
        (12, "Ripple Static"),
        (13, "Reactive"),
        (14, "Grid Ripple"),
        (15, "Tide"),
    ];
    for (id, label) in effects {
        AppendMenuW(effects_menu, MF_STRING, ID_EFFECT_BASE + id, to_wide(label).as_ptr());
    }
    AppendMenuW(menu, MF_POPUP, effects_menu as usize, to_wide("Lighting Effects ▶").as_ptr());

    // Submenu: Quick Colors
    let colors_menu = CreatePopupMenu();
    let colors = [
        (1, "Electric Blue"),
        (2, "Red"),
        (3, "Green"),
        (4, "Blue"),
        (5, "Cyan"),
        (6, "Magenta"),
        (7, "Yellow"),
        (8, "Orange"),
        (9, "White"),
    ];
    for (id, label) in colors {
        AppendMenuW(colors_menu, MF_STRING, ID_COLOR_BASE + id, to_wide(label).as_ptr());
    }
    AppendMenuW(menu, MF_POPUP, colors_menu as usize, to_wide("Quick Colors ▶").as_ptr());

    // Lighting Off
    AppendMenuW(menu, MF_STRING, ID_LIGHTING_OFF, to_wide("Lighting Off").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

    // Background Service Status & Toggle
    let is_srv_running = fantastech_core::is_service_running();
    let srv_label = if is_srv_running {
        "⚙️ Service: Running"
    } else {
        "⚙️ Service: Stopped"
    };
    let srv_flag = if is_srv_running { MF_CHECKED } else { MF_UNCHECKED };
    AppendMenuW(menu, MF_STRING | srv_flag, ID_SERVICE_TOGGLE, to_wide(srv_label).as_ptr());

    // OpenRGB Server Toggle
    let cfg = fantastech_core::FantasTechConfig::load();
    let openrgb_active = cfg.openrgb_enabled;
    let openrgb_label = if openrgb_active {
        format!("🌐 OpenRGB: Port {} (Active)", cfg.openrgb_port)
    } else {
        "🌐 OpenRGB: Disabled".to_string()
    };
    let openrgb_flag = if openrgb_active { MF_CHECKED } else { MF_UNCHECKED };
    AppendMenuW(menu, MF_STRING | openrgb_flag, ID_OPENRGB_TOGGLE, to_wide(&openrgb_label).as_ptr());

    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

    // Exit
    AppendMenuW(menu, MF_STRING, ID_EXIT, to_wide("Exit FantasTech").as_ptr());

    TrackPopupMenu(
        menu,
        TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        hwnd,
        std::ptr::null_mut(),
    );

    DestroyMenu(menu);
}

#[allow(unsafe_code)]
fn handle_tray_command(cmd_id: usize, hwnd: *mut std::ffi::c_void) {
    match cmd_id {
        ID_SHOW => {
            focus_or_spawn_gui();
        }
        ID_LIGHTING_OFF => {
            turn_lighting_off_action();
        }
        ID_SERVICE_TOGGLE => {
            toggle_service_action();
        }
        ID_OPENRGB_TOGGLE => {
            toggle_openrgb_action();
        }
        ID_EXIT => {
            // Close running graphical application if open
            fantastech_core::focus_existing_gui_with_action("EXIT");

            // Stop background service if active
            if fantastech_core::is_service_running() {
                let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::ShutdownService);
            }
            let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
            nid.cb_size = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.h_wnd = hwnd;
            nid.u_id = 1001;
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &mut nid);
                PostQuitMessage(0);
            }
            std::process::exit(0);
        }
        c if (ID_PROFILE_BASE + 1..=ID_PROFILE_BASE + 6).contains(&c) => {
            let p_idx = c - ID_PROFILE_BASE;
            let profile_ids = ["gaming", "cyberpunk", "rainbow_wave", "neon_sunset", "matrix", "static_purple"];
            if let Some(&p_id) = profile_ids.get(p_idx - 1) {
                apply_lighting_profile_action(p_id);
            }
        }
        c if (ID_EFFECT_BASE + 1..=ID_EFFECT_BASE + 15).contains(&c) => {
            let mode_idx = c - ID_EFFECT_BASE;
            apply_lighting_mode_action(mode_idx);
        }
        c if (ID_COLOR_BASE + 1..=ID_COLOR_BASE + 9).contains(&c) => {
            let color_idx = c - ID_COLOR_BASE;
            let rgb_colors = [
                (0, 212, 255),   // Electric Blue
                (255, 0, 0),     // Red
                (0, 255, 0),     // Green
                (0, 0, 255),     // Blue
                (0, 255, 255),   // Cyan
                (255, 0, 255),   // Magenta
                (255, 255, 0),   // Yellow
                (255, 127, 0),   // Orange
                (255, 255, 255), // White
            ];
            if let Some(&(r, g, b)) = rgb_colors.get(color_idx - 1) {
                set_direct_color_action(r, g, b);
            }
        }
        _ => {}
    }
}

fn apply_lighting_profile_action(profile_id: &str) {
    if fantastech_core::is_service_running() {
        let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::ApplyLightingProfile {
            id: profile_id.to_string(),
        });
        return;
    }
    let pm = fantastech_core::ProfileManager::new(None);
    if let Some(profile) = pm.load_lighting_profile(profile_id) {
        if let Ok(transport) = fantastech_transport::HidrawTransport::discover() {
            let mut dev = fantastech_device::Device::new_read_only(transport).into_reversible();
            let config = profile.to_lighting_config();
            let _ = dev.set_lighting(&config);
        }
    }
}

fn apply_lighting_mode_action(mode_idx: usize) {
    let mode = LightingMode::from_u8(mode_idx as u8).unwrap_or(LightingMode::Spectrum);

    if fantastech_core::is_service_running() {
        let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::SetLightingMode {
            mode,
            brightness: 4,
            speed: 2,
            direction: 0,
            color_mode: ColorMode::DynamicSpectrum,
            color: [0, 212, 255],
        });
        return;
    }

    if let Ok(transport) = fantastech_transport::HidrawTransport::discover() {
        let mut dev = fantastech_device::Device::new_read_only(transport).into_reversible();
        let config = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode,
            brightness: 4,
            speed: 2,
            direction: AnimationDirection::LeftToRight,
            color_mode: ColorMode::DynamicSpectrum,
            color: RgbColor::new(0, 212, 255),
        };
        let _ = dev.set_lighting(&config);
    }
}

fn set_direct_color_action(r: u8, g: u8, b: u8) {
    if fantastech_core::is_service_running() {
        let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::SetDirectColor { r, g, b });
        return;
    }
    if let Ok(transport) = fantastech_transport::HidrawTransport::discover() {
        let mut dev = fantastech_device::Device::new_read_only(transport).into_reversible();
        let config = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode: LightingMode::Static,
            brightness: 4,
            speed: 2,
            direction: AnimationDirection::LeftToRight,
            color_mode: ColorMode::FixedColor,
            color: RgbColor::new(r, g, b),
        };
        let _ = dev.set_lighting(&config);
    }
}

fn turn_lighting_off_action() {
    if fantastech_core::is_service_running() {
        let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::LightingOff);
        return;
    }
    if let Ok(transport) = fantastech_transport::HidrawTransport::discover() {
        let mut dev = fantastech_device::Device::new_read_only(transport).into_reversible();
        let config = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode: LightingMode::Off,
            brightness: 0,
            speed: 0,
            direction: AnimationDirection::LeftToRight,
            color_mode: ColorMode::FixedColor,
            color: RgbColor::new(0, 0, 0),
        };
        let _ = dev.set_lighting(&config);
    }
}

fn toggle_service_action() {
    if fantastech_core::is_service_running() {
        let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::ShutdownService);
    } else {
        let bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech.exe"));
        let mut cmd = Command::new(bin);
        cmd.args(["service", "run"]);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x01000000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
        let _ = cmd.spawn();
    }
}

fn toggle_openrgb_action() {
    let mut cfg = fantastech_core::FantasTechConfig::load();
    cfg.openrgb_enabled = !cfg.openrgb_enabled;
    let _ = cfg.save();
    if fantastech_core::is_service_running() {
        let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::SetOpenRgbEnabled(
            cfg.openrgb_enabled,
        ));
    }
}
