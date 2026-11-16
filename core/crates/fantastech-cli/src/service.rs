//! Unified background service for FantasTech suite.
//!
//! Coordinates exclusive hardware access, Unix Domain Socket IPC,
//! automatic lighting profile application, OpenRGB SDK server,
//! Volume Knob debouncer guard, and cross-platform system tray.

use fantastech_core::{
    check_firmware_update_available, is_service_running, send_ipc_request,
    FantasTechConfig, IpcRequest, IpcResponse, ProfileManager,
    ServiceStatus, KEY_DEFINITIONS,
};
#[cfg(target_os = "windows")]
use fantastech_core::WINDOWS_IPC_PORT;
#[cfg(unix)]
use fantastech_core::ipc_socket_path;
use fantastech_device::{Device, PersistentAccess, ReversibleAccess};
use fantastech_emulator::VirtualKeyboardEmulator;
use fantastech_protocol::constants::SPARSE_KEY_COUNT;
use fantastech_protocol::keymap::KeyLayer;
use fantastech_protocol::lighting::{
    AnimationDirection, ColorMode, LightingConfig, LightingMode, LightingZone, RgbColor,
};
use fantastech_protocol::openrgb::{
    build_openrgb_controller_data, normalize_brightness_to_levels, normalize_brightness_to_percent,
    parse_openrgb_set_mode, parse_openrgb_update_leds, parse_openrgb_update_single_led,
    parse_openrgb_update_zone_leds, resolve_openrgb_mode_named, scale_rgb, OpenRgbHeader,
    MODE_COLORS_NONE, MODE_COLORS_RANDOM, OPENRGB_DIR_DOWN, OPENRGB_DIR_RIGHT,
    NET_PACKET_ID_REQUEST_CONTROLLER_COUNT, NET_PACKET_ID_REQUEST_CONTROLLER_DATA,
    NET_PACKET_ID_REQUEST_PROTOCOL_VERSION, NET_PACKET_ID_RGBCONTROLLER_SAVEMODE,
    NET_PACKET_ID_RGBCONTROLLER_SETCUSTOMMODE, NET_PACKET_ID_RGBCONTROLLER_SETMODE,
    NET_PACKET_ID_RGBCONTROLLER_UPDATELEDS, NET_PACKET_ID_RGBCONTROLLER_UPDATESINGLELED,
    NET_PACKET_ID_RGBCONTROLLER_UPDATEZONELEDS, NET_PACKET_ID_SET_CLIENT_NAME,
};
use fantastech_transport::{HidrawTransport, Transport};
#[cfg(unix)]
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Child;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Trait object representing an active keyboard device handle.
pub trait DeviceControl: Send {
    fn apply_lighting_config(&mut self, config: &LightingConfig) -> Result<(), String>;
    fn apply_matrix_canvas(&mut self, matrix: &[RgbColor; SPARSE_KEY_COUNT]) -> Result<(), String>;
    fn set_single_key(&mut self, key_idx: u8, color: RgbColor) -> Result<(), String>;
    fn turn_off_lighting(&mut self) -> Result<(), String>;
    fn switch_hw_profile(&mut self, index: u8) -> Result<(), String>;
    fn set_sleep_timeout(&mut self, seconds: u16) -> Result<(), String>;
    fn factory_reset(&mut self) -> Result<(), String>;
    fn apply_keymap_layer(&mut self, layer: KeyLayer, actions: &[fantastech_protocol::keymap::KeyAction; SPARSE_KEY_COUNT]) -> Result<(), String>;
    fn apply_macro_memory(&mut self, table: &[u8; fantastech_protocol::constants::MACRO_MEMORY_SIZE]) -> Result<(), String>;
    fn get_firmware_version(&mut self) -> Result<String, String>;
}

pub enum DeviceHolder<T: Transport> {
    Reversible(Device<T, ReversibleAccess>),
    Persistent(Device<T, PersistentAccess>),
    Empty,
}

impl<T: Transport + 'static> DeviceHolder<T> {
    fn ensure_reversible(&mut self) -> Result<&mut Device<T, ReversibleAccess>, String> {
        let state = std::mem::replace(self, DeviceHolder::Empty);
        match state {
            DeviceHolder::Reversible(dev) => {
                *self = DeviceHolder::Reversible(dev);
            }
            DeviceHolder::Persistent(dev) => {
                *self = DeviceHolder::Reversible(dev.into_reversible());
            }
            DeviceHolder::Empty => return Err("Device state empty".into()),
        }
        match self {
            DeviceHolder::Reversible(dev) => Ok(dev),
            _ => unreachable!(),
        }
    }

    fn ensure_persistent(&mut self) -> Result<&mut Device<T, PersistentAccess>, String> {
        let state = std::mem::replace(self, DeviceHolder::Empty);
        match state {
            DeviceHolder::Persistent(dev) => {
                *self = DeviceHolder::Persistent(dev);
            }
            DeviceHolder::Reversible(dev) => {
                let p = dev.into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")
                    .map_err(|e| e.to_string())?;
                *self = DeviceHolder::Persistent(p);
            }
            DeviceHolder::Empty => return Err("Device state empty".into()),
        }
        match self {
            DeviceHolder::Persistent(dev) => Ok(dev),
            _ => unreachable!(),
        }
    }
}

impl<T: Transport + 'static> DeviceControl for DeviceHolder<T> {
    fn apply_lighting_config(&mut self, config: &LightingConfig) -> Result<(), String> {
        self.ensure_reversible()?.set_lighting(config).map(|_| ()).map_err(|e| e.to_string())
    }

    fn apply_matrix_canvas(&mut self, matrix: &[RgbColor; SPARSE_KEY_COUNT]) -> Result<(), String> {
        self.ensure_reversible()?.stream_matrix_canvas(matrix).map(|_| ()).map_err(|e| e.to_string())
    }

    fn set_single_key(&mut self, key_idx: u8, color: RgbColor) -> Result<(), String> {
        self.ensure_reversible()?.set_single_key_color(key_idx, color).map(|_| ()).map_err(|e| e.to_string())
    }

    fn turn_off_lighting(&mut self) -> Result<(), String> {
        let off_cfg = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode: LightingMode::Off,
            brightness: 0,
            speed: 0,
            direction: AnimationDirection::Default,
            color_mode: ColorMode::DynamicSpectrum,
            color: RgbColor::new(0, 0, 0),
        };
        self.ensure_reversible()?.set_lighting(&off_cfg).map(|_| ()).map_err(|e| e.to_string())
    }

    fn switch_hw_profile(&mut self, index: u8) -> Result<(), String> {
        self.ensure_reversible()?.switch_profile(index).map(|_| ()).map_err(|e| e.to_string())
    }

    fn set_sleep_timeout(&mut self, seconds: u16) -> Result<(), String> {
        self.ensure_persistent()?.set_sleep_timeout(seconds).map(|_| ()).map_err(|e| e.to_string())
    }

    fn factory_reset(&mut self) -> Result<(), String> {
        self.ensure_persistent()?.execute_factory_reset("I_CONFIRM_FACTORY_RESET").map(|_| ()).map_err(|e| e.to_string())
    }

    fn apply_keymap_layer(&mut self, layer: KeyLayer, actions: &[fantastech_protocol::keymap::KeyAction; SPARSE_KEY_COUNT]) -> Result<(), String> {
        self.ensure_persistent()?.write_keymap_layer(layer, actions).map(|_| ()).map_err(|e| e.to_string())
    }

    fn apply_macro_memory(&mut self, table: &[u8; fantastech_protocol::constants::MACRO_MEMORY_SIZE]) -> Result<(), String> {
        self.ensure_persistent()?.write_macro_memory(table).map(|_| ()).map_err(|e| e.to_string())
    }

    fn get_firmware_version(&mut self) -> Result<String, String> {
        let info = match self {
            DeviceHolder::Reversible(dev) => dev.query_info().map_err(|e| e.to_string())?,
            DeviceHolder::Persistent(dev) => dev.query_info().map_err(|e| e.to_string())?,
            DeviceHolder::Empty => return Err("Device state empty".into()),
        };
        Ok(info.firmware_version)
    }
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn poll_socket_ready<F: std::os::unix::io::AsRawFd>(socket: &F, timeout_ms: i32) -> std::io::Result<bool> {
    let raw_fd = socket.as_raw_fd();
    let mut pfd = libc::pollfd {
        fd: raw_fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ret = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    if ret > 0 {
        if (pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL)) != 0 && (pfd.revents & libc::POLLIN) == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "Socket error in poll"));
        }
        Ok((pfd.revents & libc::POLLIN) != 0)
    } else if ret == 0 {
        Ok(false)
    } else {
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::Interrupted {
            Ok(false)
        } else {
            Err(err)
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn poll_socket_ready<F: std::os::windows::io::AsRawSocket>(socket: &F, timeout_ms: i32) -> std::io::Result<bool> {
    #[repr(C)]
    struct WSAPOLLFD {
        fd: usize,
        events: i16,
        revents: i16,
    }
    #[link(name = "ws2_32")]
    extern "system" {
        fn WSAPoll(fdArray: *mut WSAPOLLFD, fds: u32, timeout: i32) -> i32;
    }
    const POLLIN: i16 = 0x0100 | 0x0200;

    let raw_socket = socket.as_raw_socket() as usize;
    let mut pfd = WSAPOLLFD {
        fd: raw_socket,
        events: POLLIN,
        revents: 0,
    };
    let ret = unsafe { WSAPoll(&mut pfd, 1, timeout_ms) };
    if ret > 0 {
        Ok((pfd.revents & POLLIN) != 0)
    } else if ret == 0 {
        Ok(false)
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_openrgb_server(
    bind: String,
    port: u16,
    dev_control: Arc<Mutex<Box<dyn DeviceControl>>>,
    service_running: Arc<AtomicBool>,
    server_active_flag: Arc<AtomicBool>,
    cached_matrix: Arc<Mutex<[RgbColor; SPARSE_KEY_COUNT]>>,
    openrgb_brightness: Arc<AtomicU8>,
    openrgb_mode: Arc<AtomicU8>,
) {
    thread::spawn(move || {
        let addr = format!("{}:{}", bind, port);
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => {
                println!("🌐 OpenRGB SDK server listening on tcp://{}", addr);
                server_active_flag.store(true, Ordering::SeqCst);
                l
            }
            Err(e) => {
                eprintln!("⚠️ Failed to bind OpenRGB TCP port {}: {}", addr, e);
                server_active_flag.store(false, Ordering::SeqCst);
                return;
            }
        };

        let _ = listener.set_nonblocking(true);

        while service_running.load(Ordering::SeqCst) && server_active_flag.load(Ordering::SeqCst) {
            match poll_socket_ready(&listener, 1000) {
                Ok(true) => {
                    match listener.accept() {
                        Ok((socket, _)) => {
                            let _ = socket.set_nodelay(true);
                            let _ = socket.set_nonblocking(false);
                            let dev_ref = Arc::clone(&dev_control);
                            let mat_ref = Arc::clone(&cached_matrix);
                            let brt_ref = Arc::clone(&openrgb_brightness);
                            let mode_ref = Arc::clone(&openrgb_mode);
                            thread::spawn(move || {
                                handle_openrgb_client(socket, dev_ref, mat_ref, brt_ref, mode_ref);
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => {
                            eprintln!("⚠️ OpenRGB accept error: {}", e);
                        }
                    }
                }
                Ok(false) => {}
                Err(_) => break,
            }
        }

        server_active_flag.store(false, Ordering::SeqCst);
        println!("🌐 OpenRGB SDK server stopped.");
    });
}

pub fn handle_openrgb_client(
    mut stream: TcpStream,
    dev_control: Arc<Mutex<Box<dyn DeviceControl>>>,
    cached_matrix: Arc<Mutex<[RgbColor; SPARSE_KEY_COUNT]>>,
    openrgb_brightness: Arc<AtomicU8>,
    openrgb_mode: Arc<AtomicU8>,
) {
    let mut header_buf = [0u8; 16];
    let keys_meta: Vec<(&str, u8, u8, u8)> = KEY_DEFINITIONS
        .iter()
        .map(|k| (k.name, k.row, k.col, k.index))
        .collect();

    while stream.read_exact(&mut header_buf).is_ok() {
        let header = match OpenRgbHeader::from_bytes(&header_buf) {
            Ok(h) => h,
            Err(_) => break,
        };

        let mut data_buf = vec![0u8; header.pkt_size as usize];
        if header.pkt_size > 0 && stream.read_exact(&mut data_buf).is_err() {
            break;
        }

        match header.pkt_id {
            NET_PACKET_ID_REQUEST_CONTROLLER_COUNT => {
                let resp = OpenRgbHeader::new(0, NET_PACKET_ID_REQUEST_CONTROLLER_COUNT, 4);
                let _ = stream.write_all(&resp.to_bytes());
                let _ = stream.write_all(&1u32.to_le_bytes());
                let _ = stream.flush();
            }
            NET_PACKET_ID_REQUEST_CONTROLLER_DATA => {
                let (current_colors, current_b, current_m) = {
                    let unscaled = cached_matrix.lock().unwrap();
                    let mut cols = Vec::with_capacity(KEY_DEFINITIONS.len());
                    for k in KEY_DEFINITIONS.iter() {
                        cols.push(unscaled[k.index as usize]);
                    }
                    (
                        cols,
                        openrgb_brightness.load(Ordering::SeqCst),
                        openrgb_mode.load(Ordering::SeqCst),
                    )
                };
                let controller_data = build_openrgb_controller_data(
                    &keys_meta,
                    &current_colors,
                    current_m,
                    normalize_brightness_to_levels(current_b as u32),
                    0,  // Direction
                );
                let resp = OpenRgbHeader::new(
                    header.dev_id,
                    NET_PACKET_ID_REQUEST_CONTROLLER_DATA,
                    controller_data.len() as u32,
                );
                let _ = stream.write_all(&resp.to_bytes());
                let _ = stream.write_all(&controller_data);
                let _ = stream.flush();
            }
            NET_PACKET_ID_REQUEST_PROTOCOL_VERSION => {
                let client_version = if data_buf.len() >= 4 {
                    u32::from_le_bytes(data_buf[0..4].try_into().unwrap_or([3, 0, 0, 0]))
                } else {
                    3
                };
                let negotiated = client_version.min(3);
                let resp = OpenRgbHeader::new(0, NET_PACKET_ID_REQUEST_PROTOCOL_VERSION, 4);
                let _ = stream.write_all(&resp.to_bytes());
                let _ = stream.write_all(&negotiated.to_le_bytes());
                let _ = stream.flush();
            }
            NET_PACKET_ID_SET_CLIENT_NAME => {
                // Client registered its name; no response needed per OpenRGB spec
            }
            NET_PACKET_ID_RGBCONTROLLER_UPDATELEDS => {
                if let Ok(led_colors) = parse_openrgb_update_leds(&data_buf) {
                    let b_pct = openrgb_brightness.load(Ordering::SeqCst);
                    let is_canvas_mode = openrgb_mode.load(Ordering::SeqCst) == 16;
                    let mut scaled = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                    {
                        let mut unscaled = cached_matrix.lock().unwrap();
                        for (i, c) in led_colors.iter().enumerate().take(KEY_DEFINITIONS.len()) {
                            let k = &KEY_DEFINITIONS[i];
                            unscaled[k.index as usize] = *c;
                        }
                        if is_canvas_mode {
                            for (idx, color) in unscaled.iter().enumerate() {
                                scaled[idx] = scale_rgb(*color, b_pct);
                            }
                        }
                    }
                    if is_canvas_mode {
                        let mut dev = dev_control.lock().unwrap();
                        let _ = dev.apply_matrix_canvas(&scaled);
                    }
                }
            }
            NET_PACKET_ID_RGBCONTROLLER_UPDATEZONELEDS => {
                if let Ok((_zone, led_colors)) = parse_openrgb_update_zone_leds(&data_buf) {
                    let b_pct = openrgb_brightness.load(Ordering::SeqCst);
                    let is_canvas_mode = openrgb_mode.load(Ordering::SeqCst) == 16;
                    let mut scaled = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                    {
                        let mut unscaled = cached_matrix.lock().unwrap();
                        for (i, c) in led_colors.iter().enumerate().take(KEY_DEFINITIONS.len()) {
                            let k = &KEY_DEFINITIONS[i];
                            unscaled[k.index as usize] = *c;
                        }
                        if is_canvas_mode {
                            for (idx, color) in unscaled.iter().enumerate() {
                                scaled[idx] = scale_rgb(*color, b_pct);
                            }
                        }
                    }
                    if is_canvas_mode {
                        let mut dev = dev_control.lock().unwrap();
                        let _ = dev.apply_matrix_canvas(&scaled);
                    }
                }
            }
            NET_PACKET_ID_RGBCONTROLLER_UPDATESINGLELED => {
                if let Ok((led_idx, color)) = parse_openrgb_update_single_led(&data_buf) {
                    if let Some(k) = KEY_DEFINITIONS.get(led_idx as usize) {
                        let b_pct = openrgb_brightness.load(Ordering::SeqCst);
                        let is_canvas_mode = openrgb_mode.load(Ordering::SeqCst) == 16;
                        {
                            let mut unscaled = cached_matrix.lock().unwrap();
                            unscaled[k.index as usize] = color;
                        }
                        if is_canvas_mode {
                            let scaled_color = scale_rgb(color, b_pct);
                            let mut dev = dev_control.lock().unwrap();
                            let _ = dev.set_single_key(k.index, scaled_color);
                        }
                    }
                }
            }
            NET_PACKET_ID_RGBCONTROLLER_SETCUSTOMMODE => {
                openrgb_mode.store(16, Ordering::SeqCst);
                let b_pct = openrgb_brightness.load(Ordering::SeqCst);
                let scaled = {
                    let unscaled = cached_matrix.lock().unwrap();
                    let mut s = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                    for (idx, color) in unscaled.iter().enumerate() {
                        s[idx] = scale_rgb(*color, b_pct);
                    }
                    s
                };
                let mut dev = dev_control.lock().unwrap();
                let _ = dev.apply_matrix_canvas(&scaled);
            }
            NET_PACKET_ID_RGBCONTROLLER_SETMODE | NET_PACKET_ID_RGBCONTROLLER_SAVEMODE => {
                if let Ok(mode_info) = parse_openrgb_set_mode(&data_buf) {
                    let mode = resolve_openrgb_mode_named(mode_info.mode_idx, mode_info.value, &mode_info.name);
                    openrgb_mode.store(mode as u8, Ordering::SeqCst);

                    let b_pct = normalize_brightness_to_percent(mode_info.brightness);
                    openrgb_brightness.store(b_pct, Ordering::SeqCst);

                    if mode == LightingMode::CustomMatrixCanvas {
                        let scaled = {
                            let unscaled = cached_matrix.lock().unwrap();
                            let mut s = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                            for (idx, color) in unscaled.iter().enumerate() {
                                s[idx] = scale_rgb(*color, b_pct);
                            }
                            s
                        };
                        let mut dev = dev_control.lock().unwrap();
                        let _ = dev.apply_matrix_canvas(&scaled);
                    } else if mode == LightingMode::Off {
                        let mut dev = dev_control.lock().unwrap();
                        let _ = dev.turn_off_lighting();
                    } else {
                        let hw_speed = if mode_info.speed <= 4 {
                            mode_info.speed as u8
                        } else {
                            ((mode_info.speed as f32 / 100.0) * 4.0).round().clamp(0.0, 4.0) as u8
                        };
                        let hw_brightness = normalize_brightness_to_levels(mode_info.brightness);
                        let color = mode_info.colors.first().copied().unwrap_or(RgbColor::new(255, 0, 0));
                        let color_mode = if mode_info.color_mode == MODE_COLORS_RANDOM
                            || mode_info.color_mode == MODE_COLORS_NONE
                            || !mode.supports_color()
                        {
                            ColorMode::DynamicSpectrum
                        } else {
                            ColorMode::FixedColor
                        };
                        let hw_direction = if mode_info.direction == 1
                            || mode_info.direction == OPENRGB_DIR_RIGHT
                            || mode_info.direction == OPENRGB_DIR_DOWN
                        {
                            AnimationDirection::LeftToRight
                        } else {
                            AnimationDirection::Default
                        };
                        let hw_cfg = LightingConfig {
                            zone: LightingZone::MainBacklight,
                            mode,
                            brightness: hw_brightness,
                            speed: hw_speed,
                            direction: hw_direction,
                            color_mode,
                            color,
                        };
                        if mode == LightingMode::Static {
                            let mut om = cached_matrix.lock().unwrap();
                            *om = [color; SPARSE_KEY_COUNT];
                        }
                        let mut dev = dev_control.lock().unwrap();
                        let _ = dev.apply_lighting_config(&hw_cfg);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "linux")]
fn spawn_volume_guard(
    service_running: Arc<AtomicBool>,
    guard_active_flag: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        crate::volume_guard::run_volume_guard_service(service_running, guard_active_flag);
    });
}

#[cfg(not(target_os = "linux"))]
fn spawn_volume_guard(
    _service_running: Arc<AtomicBool>,
    guard_active_flag: Arc<AtomicBool>,
) {
    guard_active_flag.store(false, Ordering::SeqCst);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_ipc_request(
    req: IpcRequest,
    dev_control: &Arc<Mutex<Box<dyn DeviceControl>>>,
    config: &Arc<Mutex<FantasTechConfig>>,
    profile_manager: &Arc<ProfileManager>,
    running: &Arc<AtomicBool>,
    openrgb_running: &Arc<AtomicBool>,
    volume_guard_running: &Arc<AtomicBool>,
    openrgb_matrix: &Arc<Mutex<[RgbColor; SPARSE_KEY_COUNT]>>,
    openrgb_brightness: &Arc<AtomicU8>,
    openrgb_mode: &Arc<AtomicU8>,
) -> IpcResponse {
    match req {
        IpcRequest::Ping => IpcResponse::Pong,
        IpcRequest::GetStatus => {
            let cfg = config.lock().unwrap();
            let fw_ver = dev_control.lock().unwrap().get_firmware_version().unwrap_or_default();
            let update_avail = check_firmware_update_available(&fw_ver);
            IpcResponse::Status(ServiceStatus {
                service_running: true,
                active_lighting_profile: cfg.active_lighting_profile.clone(),
                active_button_profile: cfg.active_button_profile.clone(),
                volume_guard_running: volume_guard_running.load(Ordering::SeqCst),
                openrgb_running: openrgb_running.load(Ordering::SeqCst),
                openrgb_port: cfg.openrgb_port,
                pid: std::process::id(),
                firmware_update_available: update_avail,
            })
        }
        IpcRequest::GetConfig => {
            let cfg = config.lock().unwrap();
            IpcResponse::Config(cfg.clone())
        }
        IpcRequest::SaveConfig(new_cfg) => {
            let (old_openrgb, old_guard) = {
                let cfg = config.lock().unwrap();
                (cfg.openrgb_enabled, cfg.guard_volume_enabled)
            };
            let (new_openrgb, new_guard, host, port) = (
                new_cfg.openrgb_enabled,
                new_cfg.guard_volume_enabled,
                new_cfg.openrgb_host.clone(),
                new_cfg.openrgb_port,
            );
            let _ = new_cfg.save();
            {
                let mut cfg = config.lock().unwrap();
                *cfg = new_cfg;
            }
            if new_openrgb != old_openrgb {
                openrgb_running.store(new_openrgb, Ordering::SeqCst);
                if new_openrgb {
                    spawn_openrgb_server(
                        host,
                        port,
                        Arc::clone(dev_control),
                        Arc::clone(running),
                        Arc::clone(openrgb_running),
                        Arc::clone(openrgb_matrix),
                        Arc::clone(openrgb_brightness),
                        Arc::clone(openrgb_mode),
                    );
                }
            }
            if new_guard != old_guard {
                volume_guard_running.store(new_guard, Ordering::SeqCst);
                if new_guard {
                    spawn_volume_guard(Arc::clone(running), Arc::clone(volume_guard_running));
                }
            }
            IpcResponse::Ok
        }
        IpcRequest::ApplyLightingProfile { id } => {
            if let Some(p) = profile_manager.load_lighting_profile(&id) {
                openrgb_mode.store(p.mode as u8, Ordering::SeqCst);
                let mut dev = dev_control.lock().unwrap();
                let res = if p.mode == LightingMode::CustomMatrixCanvas {
                    let matrix = p.to_matrix();
                    {
                        let mut om = openrgb_matrix.lock().unwrap();
                        *om = matrix;
                    }
                    let _ = dev.apply_lighting_config(&p.to_lighting_config());
                    dev.apply_matrix_canvas(&matrix)
                } else {
                    let hw_cfg = p.to_lighting_config();
                    dev.apply_lighting_config(&hw_cfg)
                };
                match res {
                    Ok(()) => {
                        let mut cfg = config.lock().unwrap();
                        cfg.active_lighting_profile = id;
                        let _ = cfg.save();
                        IpcResponse::Ok
                    }
                    Err(e) => IpcResponse::Error(e),
                }
            } else {
                IpcResponse::Error(format!("Profile '{}' not found", id))
            }
        }
        IpcRequest::SetLightingMode {
            mode,
            brightness,
            speed,
            direction,
            color_mode,
            color,
        } => {
            openrgb_mode.store(mode as u8, Ordering::SeqCst);
            let dir = if direction == 1 {
                AnimationDirection::LeftToRight
            } else {
                AnimationDirection::Default
            };
            let hw_cfg = LightingConfig {
                zone: LightingZone::MainBacklight,
                mode,
                brightness,
                speed,
                direction: dir,
                color_mode,
                color: RgbColor::new(color[0], color[1], color[2]),
            };
            let mut dev = dev_control.lock().unwrap();
            match dev.apply_lighting_config(&hw_cfg) {
                Ok(()) => IpcResponse::Ok,
                Err(e) => IpcResponse::Error(e),
            }
        }
        IpcRequest::SetDirectColor { r, g, b } => {
            openrgb_mode.store(LightingMode::Static as u8, Ordering::SeqCst);
            let hw_cfg = LightingConfig {
                zone: LightingZone::MainBacklight,
                mode: LightingMode::Static,
                brightness: 4,
                speed: 2,
                direction: AnimationDirection::Default,
                color_mode: ColorMode::FixedColor,
                color: RgbColor::new(r, g, b),
            };
            {
                let mut om = openrgb_matrix.lock().unwrap();
                *om = [RgbColor::new(r, g, b); SPARSE_KEY_COUNT];
            }
            let mut dev = dev_control.lock().unwrap();
            match dev.apply_lighting_config(&hw_cfg) {
                Ok(()) => IpcResponse::Ok,
                Err(e) => IpcResponse::Error(e),
            }
        }
        IpcRequest::SetKeyColor { key_index, r, g, b } => {
            openrgb_mode.store(LightingMode::CustomMatrixCanvas as u8, Ordering::SeqCst);
            if (key_index as usize) < SPARSE_KEY_COUNT {
                let mut om = openrgb_matrix.lock().unwrap();
                om[key_index as usize] = RgbColor::new(r, g, b);
            }
            let mut dev = dev_control.lock().unwrap();
            match dev.set_single_key(key_index, RgbColor::new(r, g, b)) {
                Ok(()) => IpcResponse::Ok,
                Err(e) => IpcResponse::Error(e),
            }
        }
        IpcRequest::SetCanvasMatrix { colors } => {
            openrgb_mode.store(LightingMode::CustomMatrixCanvas as u8, Ordering::SeqCst);
            let mut matrix = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
            for (i, c) in colors.iter().enumerate().take(SPARSE_KEY_COUNT) {
                matrix[i] = RgbColor::new(c[0], c[1], c[2]);
            }
            {
                let mut om = openrgb_matrix.lock().unwrap();
                *om = matrix;
            }
            let mut dev = dev_control.lock().unwrap();
            let canvas_cfg = LightingConfig {
                zone: LightingZone::MainBacklight,
                mode: LightingMode::CustomMatrixCanvas,
                brightness: 4,
                speed: 0,
                direction: AnimationDirection::Default,
                color_mode: ColorMode::FixedColor,
                color: RgbColor::new(0, 0, 0),
            };
            let _ = dev.apply_lighting_config(&canvas_cfg);
            match dev.apply_matrix_canvas(&matrix) {
                Ok(()) => IpcResponse::Ok,
                Err(e) => IpcResponse::Error(e),
            }
        }
        IpcRequest::LightingOff => {
            openrgb_mode.store(LightingMode::Off as u8, Ordering::SeqCst);
            let mut dev = dev_control.lock().unwrap();
            match dev.turn_off_lighting() {
                Ok(()) => IpcResponse::Ok,
                Err(e) => IpcResponse::Error(e),
            }
        }
        IpcRequest::SwitchHardwareProfile(idx) => {
            let mut dev = dev_control.lock().unwrap();
            match dev.switch_hw_profile(idx) {
                Ok(()) => IpcResponse::Ok,
                Err(e) => IpcResponse::Error(e),
            }
        }
        IpcRequest::ApplyButtonProfile { id } => {
            if let Some(bp) = profile_manager.load_button_profile(&id) {
                if let (Ok(base_actions), Ok(fn_actions)) = (bp.base_layer.to_descriptors(), bp.fn_layer.to_descriptors()) {
                    let mut dev = dev_control.lock().unwrap();
                    let _ = dev.apply_keymap_layer(KeyLayer::Base, &base_actions);
                    let _ = dev.apply_keymap_layer(KeyLayer::Fn, &fn_actions);
                    let mut cfg = config.lock().unwrap();
                    cfg.active_button_profile = id;
                    let _ = cfg.save();
                    IpcResponse::Ok
                } else {
                    IpcResponse::Error("Failed to resolve keymap descriptors".to_string())
                }
            } else {
                IpcResponse::Error(format!("Button profile '{}' not found", id))
            }
        }
        IpcRequest::ApplyMacroProfile { id } => {
            if let Some(mp) = profile_manager.load_macro_profile(&id) {
                let mut table = fantastech_protocol::macro_mem::MacroMemoryTable::new_empty();
                let mut offset = 0u16;
                let mut err = None;
                for seq in &mp.macros {
                    match seq.compile_into_table(&mut table, offset) {
                        Ok(next_offset) => offset = next_offset,
                        Err(e) => {
                            err = Some(e.to_string());
                            break;
                        }
                    }
                }
                if let Some(e) = err {
                    IpcResponse::Error(e)
                } else {
                    let mut dev = dev_control.lock().unwrap();
                    match dev.apply_macro_memory(table.as_bytes()) {
                        Ok(()) => {
                            let mut cfg = config.lock().unwrap();
                            cfg.active_macro_profile = id;
                            let _ = cfg.save();
                            IpcResponse::Ok
                        }
                        Err(e) => IpcResponse::Error(e),
                    }
                }
            } else {
                IpcResponse::Error(format!("Macro profile '{}' not found", id))
            }
        }
        IpcRequest::ApplySettingsProfile { id } => {
            if let Some(sp) = profile_manager.load_settings_profile(&id) {
                let mut dev = dev_control.lock().unwrap();
                match dev.set_sleep_timeout(sp.sleep_timeout_mins * 60) {
                    Ok(()) => {
                        let mut cfg = config.lock().unwrap();
                        cfg.active_settings_profile = id;
                        let _ = cfg.save();
                        IpcResponse::Ok
                    }
                    Err(e) => IpcResponse::Error(e),
                }
            } else {
                IpcResponse::Error(format!("Settings profile '{}' not found", id))
            }
        }
        IpcRequest::ApplyKeymapLayer { layer, actions } => {
            if actions.len() == SPARSE_KEY_COUNT {
                let mut arr = [fantastech_protocol::keymap::KeyAction::disabled(); SPARSE_KEY_COUNT];
                arr.copy_from_slice(&actions);
                let mut dev = dev_control.lock().unwrap();
                match dev.apply_keymap_layer(layer, &arr) {
                    Ok(()) => IpcResponse::Ok,
                    Err(e) => IpcResponse::Error(e),
                }
            } else {
                IpcResponse::Error("Actions vector must have exactly 126 elements".to_string())
            }
        }
        IpcRequest::SetSleepTimeout(secs) => {
            let mut dev = dev_control.lock().unwrap();
            match dev.set_sleep_timeout(secs) {
                Ok(()) => IpcResponse::Ok,
                Err(e) => IpcResponse::Error(e),
            }
        }
        IpcRequest::FactoryReset => {
            let mut dev = dev_control.lock().unwrap();
            match dev.factory_reset() {
                Ok(()) => IpcResponse::Ok,
                Err(e) => IpcResponse::Error(e),
            }
        }
        IpcRequest::SetOpenRgbEnabled(enabled) => {
            let (should_spawn, host, port) = {
                let mut cfg = config.lock().unwrap();
                cfg.openrgb_enabled = enabled;
                let _ = cfg.save();
                let was_running = openrgb_running.load(Ordering::SeqCst);
                openrgb_running.store(enabled, Ordering::SeqCst);
                (enabled && !was_running, cfg.openrgb_host.clone(), cfg.openrgb_port)
            };
            if should_spawn {
                spawn_openrgb_server(
                    host,
                    port,
                    Arc::clone(dev_control),
                    Arc::clone(running),
                    Arc::clone(openrgb_running),
                    Arc::clone(openrgb_matrix),
                    Arc::clone(openrgb_brightness),
                    Arc::clone(openrgb_mode),
                );
            }
            IpcResponse::Ok
        }
        IpcRequest::SetVolumeGuardEnabled(enabled) => {
            let mut cfg = config.lock().unwrap();
            cfg.guard_volume_enabled = enabled;
            let _ = cfg.save();
            let was_running = volume_guard_running.load(Ordering::SeqCst);
            volume_guard_running.store(enabled, Ordering::SeqCst);
            if enabled && !was_running {
                spawn_volume_guard(Arc::clone(running), Arc::clone(volume_guard_running));
            }
            IpcResponse::Ok
        }
        IpcRequest::ShutdownService => {
            running.store(false, Ordering::SeqCst);
            IpcResponse::Ok
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_ipc_io<R: Read, W: Write>(
    reader: &mut BufReader<R>,
    writer: &mut W,
    dev_control: &Arc<Mutex<Box<dyn DeviceControl>>>,
    config: &Arc<Mutex<FantasTechConfig>>,
    profile_manager: &Arc<ProfileManager>,
    running: &Arc<AtomicBool>,
    openrgb_running: &Arc<AtomicBool>,
    volume_guard_running: &Arc<AtomicBool>,
    openrgb_matrix: &Arc<Mutex<[RgbColor; SPARSE_KEY_COUNT]>>,
    openrgb_brightness: &Arc<AtomicU8>,
    openrgb_mode: &Arc<AtomicU8>,
) {
    let mut line = String::new();
    if reader.read_line(&mut line).is_ok() && !line.trim().is_empty() {
        if let Ok(req) = serde_json::from_str::<IpcRequest>(&line) {
            let is_shutdown = matches!(req, IpcRequest::ShutdownService);
            let resp = dispatch_ipc_request(
                req,
                dev_control,
                config,
                profile_manager,
                running,
                openrgb_running,
                volume_guard_running,
                openrgb_matrix,
                openrgb_brightness,
                openrgb_mode,
            );
            if let Ok(json_resp) = serde_json::to_string(&resp) {
                let _ = writer.write_all(json_resp.as_bytes());
                let _ = writer.write_all(b"\n");
                let _ = writer.flush();
            }
            if is_shutdown {
                std::thread::spawn(|| {
                    std::thread::sleep(Duration::from_millis(100));
                    std::process::exit(0);
                });
            }
        }
    }
}

#[cfg(unix)]
mod unix_service {
use super::*;

/// Runs the unified FantasTech background service in the foreground.
pub fn run_service(
    use_emulator: bool,
    explicit_device: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = ipc_socket_path();

    // Check if another instance is already running
    if is_service_running() {
        eprintln!("❌ Error: FantasTech background service is already running on {}", socket_path.display());
        return Err("Service already active".into());
    }

    // Clean up stale socket file if it exists
    if socket_path.exists() {
        let _ = fs::remove_file(&socket_path);
    }
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }

    println!("============================================================");
    println!("🌈 FantasTech Unified Keyboard Service Daemon");
    println!("   Target Hardware: Fantech MK894 Maxpower II (0x36AE:0xFE9C)");
    println!("   IPC Socket:      {}", socket_path.display());
    println!("============================================================");

    let config = Arc::new(Mutex::new(FantasTechConfig::load()));
    let profile_manager = Arc::new(ProfileManager::new(None));
    let _ = profile_manager.ensure_seed_profiles();

    // Connect to hardware or emulator
    let dev_control: Arc<Mutex<Box<dyn DeviceControl>>> = if use_emulator {
        println!("🎮 Using Virtual Keyboard Emulator.");
        let emu = VirtualKeyboardEmulator::new();
        let dev = Device::new_read_only(emu).into_reversible();
        Arc::new(Mutex::new(Box::new(DeviceHolder::Reversible(dev))))
    } else if let Some(path) = explicit_device {
        println!("🔌 Opening explicit device node: {:?}", path);
        let transport = HidrawTransport::open(path)?;
        let dev = Device::new_read_only(transport).into_reversible();
        Arc::new(Mutex::new(Box::new(DeviceHolder::Reversible(dev))))
    } else {
        match HidrawTransport::discover() {
            Ok(transport) => {
                println!("🔌 Discovered physical Fantech MK894 on /dev/hidraw.");
                let dev = Device::new_read_only(transport).into_reversible();
                Arc::new(Mutex::new(Box::new(DeviceHolder::Reversible(dev))))
            }
            Err(e) => {
                println!("⚠️  Physical MK894 not currently found: {}", e);
                println!("   Falling back to Virtual Keyboard Emulator for offline coordination.");
                let emu = VirtualKeyboardEmulator::new();
                let dev = Device::new_read_only(emu).into_reversible();
                Arc::new(Mutex::new(Box::new(DeviceHolder::Reversible(dev))))
            }
        }
    };

    // Auto-apply active lighting profile if enabled and OpenRGB is off
    {
        let cfg = config.lock().unwrap();
        if cfg.auto_apply_lighting && !cfg.openrgb_enabled {
            println!("💡 Auto-applying active lighting profile '{}'...", cfg.active_lighting_profile);
            if let Some(p) = profile_manager.load_lighting_profile(&cfg.active_lighting_profile) {
                let mut dev = dev_control.lock().unwrap();
                if p.mode == LightingMode::CustomMatrixCanvas {
                    let matrix = p.to_matrix();
                    let _ = dev.apply_matrix_canvas(&matrix);
                } else {
                    let hw_cfg = p.to_lighting_config();
                    let _ = dev.apply_lighting_config(&hw_cfg);
                }
                println!("✅ Applied lighting profile '{}'", p.name);
            }
        }
    }

    // State flags for background threads
    let running = Arc::new(AtomicBool::new(true));

    // Shared OpenRGB state
    let openrgb_matrix = Arc::new(Mutex::new([RgbColor::new(0, 113, 207); SPARSE_KEY_COUNT]));
    let openrgb_brightness = Arc::new(AtomicU8::new(100));
    let initial_mode = {
        let cfg = config.lock().unwrap();
        if let Some(p) = profile_manager.load_lighting_profile(&cfg.active_lighting_profile) {
            p.mode as u8
        } else {
            16
        }
    };
    let openrgb_mode = Arc::new(AtomicU8::new(initial_mode));

    // Initialize matrix from active profile if applicable
    {
        let cfg = config.lock().unwrap();
        if let Some(p) = profile_manager.load_lighting_profile(&cfg.active_lighting_profile) {
            if p.mode == LightingMode::CustomMatrixCanvas {
                let mut om = openrgb_matrix.lock().unwrap();
                *om = p.to_matrix();
            }
        }
    }

    // Spawn OpenRGB thread if enabled
    let openrgb_running = Arc::new(AtomicBool::new(false));
    {
        let cfg = config.lock().unwrap();
        if cfg.openrgb_enabled {
            spawn_openrgb_server(
                cfg.openrgb_host.clone(),
                cfg.openrgb_port,
                Arc::clone(&dev_control),
                Arc::clone(&running),
                Arc::clone(&openrgb_running),
                Arc::clone(&openrgb_matrix),
                Arc::clone(&openrgb_brightness),
                Arc::clone(&openrgb_mode),
            );
        }
    }

    // Spawn Volume Guard thread if enabled
    let volume_guard_running = Arc::new(AtomicBool::new(false));
    {
        let cfg = config.lock().unwrap();
        if cfg.guard_volume_enabled {
            spawn_volume_guard(
                Arc::clone(&running),
                Arc::clone(&volume_guard_running),
            );
        }
    }

    // Spawn System Tray applet if enabled
    let tray_child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    {
        let cfg = config.lock().unwrap();
        if cfg.tray_icon_enabled {
            spawn_tray_applet(Arc::clone(&tray_child));
        }
    }

    // Bind Unix domain socket for IPC, recovering gracefully from orphaned sockets
    let socket_path = ipc_socket_path();
    if socket_path.exists() {
        if UnixStream::connect(&socket_path).is_ok() {
            eprintln!("❌ FantasTech service is already running on {}", socket_path.display());
            return Err("Service already running".into());
        } else {
            // Orphaned socket from previous abnormal termination; safely remove it
            let _ = fs::remove_file(&socket_path);
        }
    }
    let listener = UnixListener::bind(&socket_path)?;
    listener.set_nonblocking(true)?;

    // Ensure socket permissions are 0600 (restricted exclusively to current user)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600));
        let uid = std::fs::metadata("/proc/self")
            .map(|m| {
                use std::os::unix::fs::MetadataExt;
                m.uid()
            })
            .unwrap_or(1000);
        if uid == 0 {
            let s_uid: u32 = std::env::var("SUDO_UID").ok().and_then(|s| s.parse().ok()).unwrap_or(1000);
            let s_gid: u32 = std::env::var("SUDO_GID").ok().and_then(|s| s.parse().ok()).unwrap_or(1000);
            use std::os::unix::fs::chown;
            let _ = chown(&socket_path, Some(s_uid), Some(s_gid));
        }
    }

    println!("🚀 FantasTech Service initialized and listening for IPC connections.");

    let mut last_tray_check = std::time::Instant::now();

    // Main service event loop (event-driven, 0.0% idle CPU)
    while running.load(Ordering::SeqCst) {
        if last_tray_check.elapsed() >= Duration::from_secs(1) {
            last_tray_check = std::time::Instant::now();

            // If running inside Flatpak sandbox, auto-terminate if Flatpak was uninstalled
            if std::env::var("FLATPAK_ID").is_ok() && !std::path::Path::new("/app/bin/fantastech").exists() {
                eprintln!("🛑 Flatpak uninstalled. Exiting background service...");
                let _ = fs::remove_file(&socket_path);
                std::process::exit(0);
            }

            let cfg = config.lock().unwrap();
            if cfg.tray_icon_enabled && !fantastech_core::is_tray_already_running() {
                crate::tray::spawn_native_tray();
            }
        }

        match poll_socket_ready(&listener, 1000) {
            Ok(true) => {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let dev_ref = Arc::clone(&dev_control);
                        let cfg_ref = Arc::clone(&config);
                        let prof_ref = Arc::clone(&profile_manager);
                        let run_ref = Arc::clone(&running);
                        let openrgb_flag = Arc::clone(&openrgb_running);
                        let volume_flag = Arc::clone(&volume_guard_running);
                        let openrgb_mat = Arc::clone(&openrgb_matrix);
                        let openrgb_brt = Arc::clone(&openrgb_brightness);
                        let openrgb_md = Arc::clone(&openrgb_mode);

                        thread::spawn(move || {
                            handle_ipc_connection(
                                stream,
                                dev_ref,
                                cfg_ref,
                                prof_ref,
                                run_ref,
                                openrgb_flag,
                                volume_flag,
                                openrgb_mat,
                                openrgb_brt,
                                openrgb_md,
                            );
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => {
                        eprintln!("⚠️ IPC accept error: {}", e);
                    }
                }
            }
            Ok(false) => {
                // Heartbeat interval (1s) elapsed; continues loop cleanly
            }
            Err(_) => break,
        }
    }

    // Clean up on exit
    println!("🛑 Shutting down FantasTech Service...");
    if let Some(mut child) = tray_child.lock().unwrap().take() {
        let _ = child.kill();
    }
    let _ = fs::remove_file(&socket_path);
    println!("👋 FantasTech Service cleanly stopped.");

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_ipc_connection(
    mut stream: UnixStream,
    dev_control: Arc<Mutex<Box<dyn DeviceControl>>>,
    config: Arc<Mutex<FantasTechConfig>>,
    profile_manager: Arc<ProfileManager>,
    running: Arc<AtomicBool>,
    openrgb_running: Arc<AtomicBool>,
    volume_guard_running: Arc<AtomicBool>,
    openrgb_matrix: Arc<Mutex<[RgbColor; SPARSE_KEY_COUNT]>>,
    openrgb_brightness: Arc<AtomicU8>,
    openrgb_mode: Arc<AtomicU8>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let mut reader = BufReader::new(stream.try_clone().unwrap_or_else(|_| stream.try_clone().unwrap()));
    handle_ipc_io(
        &mut reader,
        &mut stream,
        &dev_control,
        &config,
        &profile_manager,
        &running,
        &openrgb_running,
        &volume_guard_running,
        &openrgb_matrix,
        &openrgb_brightness,
        &openrgb_mode,
    );
}

fn spawn_tray_applet(_child_slot: Arc<Mutex<Option<Child>>>) {
    crate::tray::spawn_native_tray();
}

// ================= Subcommand Handlers ================= //

pub fn handle_service_command(cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        "start" => {
            if is_service_running() {
                println!("✅ FantasTech background service is already running.");
                return Ok(());
            }

            println!("🚀 Starting FantasTech background service...");
            #[cfg(target_os = "linux")]
            {
                let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
                let user_unit = home.join(".config/systemd/user/fantastech.service");
                if user_unit.exists() {
                    let status = Command::new("systemctl")
                        .args(["--user", "start", "fantastech.service"])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();

                    if let Ok(s) = status {
                        if s.success() {
                            thread::sleep(Duration::from_millis(500));
                            if is_service_running() {
                                println!("✅ FantasTech service successfully started via systemd.");
                                return Ok(());
                            }
                        }
                    }
                }
            }

            // Fallback: spawn detached background daemon process
            let bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech"));
            let mut cmd = Command::new(bin);
            cmd.args(["service", "run"]);
            cmd.stdin(std::process::Stdio::null());
            cmd.stdout(std::process::Stdio::null());
            cmd.stderr(std::process::Stdio::null());
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                cmd.process_group(0);
            }
            match cmd.spawn() {
                Ok(_) => {
                    println!("⏳ Waiting for service socket initialization...");
                    for _ in 0..20 {
                        thread::sleep(Duration::from_millis(100));
                        if is_service_running() {
                            println!("✅ FantasTech service daemon successfully launched in background.");
                            return Ok(());
                        }
                    }
                    println!("⚠️ Service spawned but socket connection timed out.");
                }
                Err(e) => eprintln!("❌ Failed to spawn background daemon: {}", e),
            }
        }
        "stop" => {
            if !is_service_running() {
                println!("ℹ️ FantasTech background service is not running.");
                return Ok(());
            }

            println!("🛑 Stopping FantasTech service...");
            let _ = send_ipc_request(&IpcRequest::ShutdownService);
            #[cfg(target_os = "linux")]
            {
                let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
                if home.join(".config/systemd/user/fantastech.service").exists() {
                    let _ = Command::new("systemctl")
                        .args(["--user", "stop", "fantastech.service"])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
            }

            for _ in 0..10 {
                thread::sleep(Duration::from_millis(100));
                if !is_service_running() {
                    println!("✅ FantasTech service successfully stopped.");
                    return Ok(());
                }
            }
            println!("✅ Stop signal transmitted.");
        }
        "restart" => {
            let _ = handle_service_command("stop");
            thread::sleep(Duration::from_millis(500));
            let _ = handle_service_command("start");
        }
        "status" => {
            if is_service_running() {
                match send_ipc_request(&IpcRequest::GetStatus) {
                    Ok(IpcResponse::Status(s)) => {
                        println!("● fantastech.service - FantasTech Background Keyboard Daemon");
                        println!("     Status:                  ACTIVE (running)");
                        println!("     Process PID:             {}", s.pid);
                        println!("     Active Lighting Profile: {}", s.active_lighting_profile);
                        println!("     Active Button Profile:   {}", s.active_button_profile);
                        println!("     Volume Knob Guard:       {}", if s.volume_guard_running { "Active" } else { "Disabled" });
                        println!("     OpenRGB SDK Server:      {}", if s.openrgb_running { format!("Running (Port {})", s.openrgb_port) } else { "Disabled".to_string() });
                        println!("     IPC Control Socket:      {}", ipc_socket_path().display());
                    }
                    _ => {
                        println!("● fantastech.service - FantasTech Background Keyboard Daemon");
                        println!("     Status: ACTIVE (responding to ping)");
                    }
                }
            } else {
                println!("○ fantastech.service - FantasTech Background Keyboard Daemon");
                println!("     Status: INACTIVE (Service is not running)");
                println!("     Run 'fantastech service start' or 'fantastech service run' to launch.");
            }
        }
        "install" => {
            #[cfg(target_os = "linux")]
            install_systemd_user_service()?;
            #[cfg(not(target_os = "linux"))]
            println!("ℹ️ Service install is managed automatically via your platform installer.");
        }
        "uninstall" => {
            #[cfg(target_os = "linux")]
            uninstall_systemd_user_service()?;
            #[cfg(not(target_os = "linux"))]
            println!("ℹ️ Service uninstall is managed automatically via your platform uninstaller.");
        }
        other => {
            eprintln!("Unknown service subcommand '{}'. Options: run, start, stop, restart, status, install, uninstall", other);
        }
    }
    Ok(())
}

fn install_systemd_user_service() -> Result<(), Box<dyn std::error::Error>> {
    let service_content = r#"[Unit]
Description=FantasTech Background Keyboard Service Daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/local/bin/fantastech service run
Restart=on-failure
RestartSec=3
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
"#;

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let unit_dir = PathBuf::from(home).join(".config/systemd/user");
    fs::create_dir_all(&unit_dir)?;
    let unit_path = unit_dir.join("fantastech.service");
    fs::write(&unit_path, service_content)?;

    println!("📄 Generated systemd user unit at {:?}", unit_path);

    let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).status();
    let status = Command::new("systemctl")
        .args(["--user", "enable", "--now", "fantastech.service"])
        .status();

    if let Ok(s) = status {
        if s.success() {
            println!("✅ Successfully enabled and started fantastech.service via systemd!");
            return Ok(());
        }
    }

    println!("💡 Service file written. Run: systemctl --user enable --now fantastech.service");
    Ok(())
}

fn uninstall_systemd_user_service() -> Result<(), Box<dyn std::error::Error>> {
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "--now", "fantastech.service"])
        .status();

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let unit_path = PathBuf::from(home).join(".config/systemd/user/fantastech.service");
    if unit_path.exists() {
        fs::remove_file(&unit_path)?;
        println!("🗑️  Removed systemd unit {:?}", unit_path);
    }
    let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).status();
    println!("✅ fantastech.service uninstalled from systemd.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fantastech_protocol::openrgb::MODE_COLORS_MODE_SPECIFIC;

    #[derive(Default)]
    struct MockDeviceControl {
        last_lighting_config: Arc<Mutex<Option<LightingConfig>>>,
        last_matrix: Arc<Mutex<Option<[RgbColor; SPARSE_KEY_COUNT]>>>,
    }

    impl DeviceControl for MockDeviceControl {
        fn apply_lighting_config(&mut self, config: &LightingConfig) -> Result<(), String> {
            *self.last_lighting_config.lock().unwrap() = Some(*config);
            Ok(())
        }

        fn apply_matrix_canvas(&mut self, matrix: &[RgbColor; SPARSE_KEY_COUNT]) -> Result<(), String> {
            *self.last_matrix.lock().unwrap() = Some(*matrix);
            Ok(())
        }

        fn set_single_key(&mut self, _key_idx: u8, _color: RgbColor) -> Result<(), String> {
            Ok(())
        }

        fn turn_off_lighting(&mut self) -> Result<(), String> {
            Ok(())
        }

        fn switch_hw_profile(&mut self, _index: u8) -> Result<(), String> {
            Ok(())
        }

        fn set_sleep_timeout(&mut self, _seconds: u16) -> Result<(), String> {
            Ok(())
        }

        fn factory_reset(&mut self) -> Result<(), String> {
            Ok(())
        }

        fn apply_keymap_layer(&mut self, _layer: KeyLayer, _actions: &[fantastech_protocol::keymap::KeyAction; SPARSE_KEY_COUNT]) -> Result<(), String> {
            Ok(())
        }

        fn apply_macro_memory(&mut self, _table: &[u8; fantastech_protocol::constants::MACRO_MEMORY_SIZE]) -> Result<(), String> {
            Ok(())
        }

        fn get_firmware_version(&mut self) -> Result<String, String> {
            Ok("V101".to_string())
        }
    }

    #[test]
    fn test_openrgb_brightness_and_random_color_handling() {
        let last_cfg = Arc::new(Mutex::new(None));
        let last_mat = Arc::new(Mutex::new(None));
        let mock_dev = MockDeviceControl {
            last_lighting_config: Arc::clone(&last_cfg),
            last_matrix: Arc::clone(&last_mat),
        };

        let dev_control: Arc<Mutex<Box<dyn DeviceControl>>> = Arc::new(Mutex::new(Box::new(mock_dev)));
        let cached_matrix = Arc::new(Mutex::new([RgbColor::new(200, 100, 50); SPARSE_KEY_COUNT]));
        let openrgb_brightness = Arc::new(AtomicU8::new(100));
        let openrgb_mode = Arc::new(AtomicU8::new(16));

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test port");
        let local_addr = listener.local_addr().unwrap();
        let mut client = std::net::TcpStream::connect(local_addr).expect("connect client");
        let (server_stream, _) = listener.accept().expect("accept server");
        drop(listener);

        let dev_ref = Arc::clone(&dev_control);
        let mat_ref = Arc::clone(&cached_matrix);
        let brt_ref = Arc::clone(&openrgb_brightness);
        let mode_ref = Arc::clone(&openrgb_mode);
        thread::spawn(move || {
            handle_openrgb_client(server_stream, dev_ref, mat_ref, brt_ref, mode_ref);
        });

        // 1. Direct Mode Brightness Scaling
        // Set brightness to 50%
        let mut set_mode_direct = Vec::new();
        set_mode_direct.extend_from_slice(&0u32.to_le_bytes()); // mode_idx 0 (Direct)
        set_mode_direct.extend_from_slice(&0u16.to_le_bytes()); // name length
        set_mode_direct.extend_from_slice(&16i32.to_le_bytes()); // value = 16 (Direct)
        set_mode_direct.extend_from_slice(&0u32.to_le_bytes()); // flags
        set_mode_direct.extend_from_slice(&[0u8; 24]); // spd/brt/col min/max
        set_mode_direct.extend_from_slice(&0u32.to_le_bytes()); // speed
        set_mode_direct.extend_from_slice(&50u32.to_le_bytes()); // brightness = 50%
        set_mode_direct.extend_from_slice(&0u32.to_le_bytes()); // direction
        set_mode_direct.extend_from_slice(&1u32.to_le_bytes()); // color_mode (per led)
        set_mode_direct.extend_from_slice(&0u16.to_le_bytes()); // colors count

        let mut pkt_body = (set_mode_direct.len() as u32 + 4).to_le_bytes().to_vec();
        pkt_body.extend_from_slice(&set_mode_direct);

        let hdr = OpenRgbHeader::new(0, NET_PACKET_ID_RGBCONTROLLER_SETMODE, pkt_body.len() as u32);
        client.write_all(&hdr.to_bytes()).unwrap();
        client.write_all(&pkt_body).unwrap();
        client.flush().unwrap();

        std::thread::sleep(Duration::from_millis(50));

        assert_eq!(openrgb_brightness.load(Ordering::SeqCst), 50);
        let m = last_mat.lock().unwrap().take().expect("matrix canvas applied");
        assert_eq!(m[0], RgbColor::new(100, 50, 25)); // Scaled from (200, 100, 50) by 50%

        // 2. Animation Mode with Random Color
        // Mode 2 = Breathing (value 2), brightness 75%, color_mode 3 (MODE_COLORS_RANDOM)
        let mut set_mode_breathing = Vec::new();
        set_mode_breathing.extend_from_slice(&2u32.to_le_bytes()); // mode_idx 2
        set_mode_breathing.extend_from_slice(&0u16.to_le_bytes()); // name length
        set_mode_breathing.extend_from_slice(&2i32.to_le_bytes()); // value = 2 (Breathing)
        set_mode_breathing.extend_from_slice(&0u32.to_le_bytes()); // flags
        set_mode_breathing.extend_from_slice(&[0u8; 24]); // spd/brt/col min/max
        set_mode_breathing.extend_from_slice(&2u32.to_le_bytes()); // speed
        set_mode_breathing.extend_from_slice(&75u32.to_le_bytes()); // brightness = 75%
        set_mode_breathing.extend_from_slice(&0u32.to_le_bytes()); // direction
        set_mode_breathing.extend_from_slice(&(MODE_COLORS_RANDOM).to_le_bytes()); // Random color mode
        set_mode_breathing.extend_from_slice(&0u16.to_le_bytes()); // colors count

        let mut pkt_body = (set_mode_breathing.len() as u32 + 4).to_le_bytes().to_vec();
        pkt_body.extend_from_slice(&set_mode_breathing);

        let hdr = OpenRgbHeader::new(0, NET_PACKET_ID_RGBCONTROLLER_SETMODE, pkt_body.len() as u32);
        client.write_all(&hdr.to_bytes()).unwrap();
        client.write_all(&pkt_body).unwrap();
        client.flush().unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let cfg = last_cfg.lock().unwrap().take().expect("breathing config applied");
        assert_eq!(cfg.mode, LightingMode::Breathing);
        assert_eq!(cfg.color_mode, ColorMode::DynamicSpectrum); // Verified Random -> DynamicSpectrum!
        assert_eq!(cfg.brightness, 3); // Verified 75% -> level 3!

        // 3. Animation Mode with Specific Color
        let mut set_mode_breathing_color = Vec::new();
        set_mode_breathing_color.extend_from_slice(&2u32.to_le_bytes()); // mode_idx 2
        set_mode_breathing_color.extend_from_slice(&0u16.to_le_bytes()); // name length
        set_mode_breathing_color.extend_from_slice(&2i32.to_le_bytes()); // value = 2
        set_mode_breathing_color.extend_from_slice(&0u32.to_le_bytes()); // flags
        set_mode_breathing_color.extend_from_slice(&[0u8; 24]); // spd/brt/col min/max
        set_mode_breathing_color.extend_from_slice(&2u32.to_le_bytes()); // speed
        set_mode_breathing_color.extend_from_slice(&25u32.to_le_bytes()); // brightness = 25%
        set_mode_breathing_color.extend_from_slice(&0u32.to_le_bytes()); // direction
        set_mode_breathing_color.extend_from_slice(&(MODE_COLORS_MODE_SPECIFIC).to_le_bytes()); // Specific color
        set_mode_breathing_color.extend_from_slice(&1u16.to_le_bytes()); // 1 color
        set_mode_breathing_color.extend_from_slice(&[0, 255, 0, 0]); // Green

        let mut pkt_body = (set_mode_breathing_color.len() as u32 + 4).to_le_bytes().to_vec();
        pkt_body.extend_from_slice(&set_mode_breathing_color);

        let hdr = OpenRgbHeader::new(0, NET_PACKET_ID_RGBCONTROLLER_SETMODE, pkt_body.len() as u32);
        client.write_all(&hdr.to_bytes()).unwrap();
        client.write_all(&pkt_body).unwrap();
        client.flush().unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let cfg = last_cfg.lock().unwrap().take().expect("breathing specific color applied");
        assert_eq!(cfg.mode, LightingMode::Breathing);
        assert_eq!(cfg.color_mode, ColorMode::FixedColor); // Verified Specific -> FixedColor!
        assert_eq!(cfg.brightness, 1); // Verified 25% -> level 1!
        assert_eq!(cfg.color, RgbColor::new(0, 255, 0));

        // Clean up
        drop(client);
    }

    #[test]
    fn test_openrgb_handshake_and_flush() {
        let last_cfg = Arc::new(Mutex::new(None));
        let last_mat = Arc::new(Mutex::new(None));
        let mock_dev = MockDeviceControl {
            last_lighting_config: Arc::clone(&last_cfg),
            last_matrix: Arc::clone(&last_mat),
        };

        let dev_control: Arc<Mutex<Box<dyn DeviceControl>>> = Arc::new(Mutex::new(Box::new(mock_dev)));
        let cached_matrix = Arc::new(Mutex::new([RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT]));
        let openrgb_brightness = Arc::new(AtomicU8::new(100));
        let openrgb_mode = Arc::new(AtomicU8::new(16));

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test port");
        let local_addr = listener.local_addr().unwrap();
        let mut client = std::net::TcpStream::connect(local_addr).expect("connect client");
        let (server_stream, _) = listener.accept().expect("accept server");
        drop(listener);

        let dev_ref = Arc::clone(&dev_control);
        let mat_ref = Arc::clone(&cached_matrix);
        let brt_ref = Arc::clone(&openrgb_brightness);
        let mode_ref = Arc::clone(&openrgb_mode);
        thread::spawn(move || {
            handle_openrgb_client(server_stream, dev_ref, mat_ref, brt_ref, mode_ref);
        });

        // 1. Send Protocol Version Request
        let ver_req = OpenRgbHeader::new(0, NET_PACKET_ID_REQUEST_PROTOCOL_VERSION, 4);
        client.write_all(&ver_req.to_bytes()).unwrap();
        client.write_all(&3u32.to_le_bytes()).unwrap();
        client.flush().unwrap();

        let mut resp_hdr_buf = [0u8; 16];
        client.read_exact(&mut resp_hdr_buf).unwrap();
        let resp_hdr = OpenRgbHeader::from_bytes(&resp_hdr_buf).unwrap();
        assert_eq!(resp_hdr.pkt_id, NET_PACKET_ID_REQUEST_PROTOCOL_VERSION);
        assert_eq!(resp_hdr.pkt_size, 4);
        let mut ver_buf = [0u8; 4];
        client.read_exact(&mut ver_buf).unwrap();
        assert_eq!(u32::from_le_bytes(ver_buf), 3);

        // 2. Send Controller Count Request
        let count_req = OpenRgbHeader::new(0, NET_PACKET_ID_REQUEST_CONTROLLER_COUNT, 0);
        client.write_all(&count_req.to_bytes()).unwrap();
        client.flush().unwrap();

        client.read_exact(&mut resp_hdr_buf).unwrap();
        let resp_hdr = OpenRgbHeader::from_bytes(&resp_hdr_buf).unwrap();
        assert_eq!(resp_hdr.pkt_id, NET_PACKET_ID_REQUEST_CONTROLLER_COUNT);
        assert_eq!(resp_hdr.pkt_size, 4);
        let mut count_buf = [0u8; 4];
        client.read_exact(&mut count_buf).unwrap();
        assert_eq!(u32::from_le_bytes(count_buf), 1);

        // 3. Send Controller Data Request for device 0
        let data_req = OpenRgbHeader::new(0, NET_PACKET_ID_REQUEST_CONTROLLER_DATA, 0);
        client.write_all(&data_req.to_bytes()).unwrap();
        client.flush().unwrap();

        client.read_exact(&mut resp_hdr_buf).unwrap();
        let resp_hdr = OpenRgbHeader::from_bytes(&resp_hdr_buf).unwrap();
        assert_eq!(resp_hdr.pkt_id, NET_PACKET_ID_REQUEST_CONTROLLER_DATA);
        assert!(resp_hdr.pkt_size > 0);
        let mut data_buf = vec![0u8; resp_hdr.pkt_size as usize];
        client.read_exact(&mut data_buf).unwrap();
        let data_str = String::from_utf8_lossy(&data_buf);
        assert!(data_str.contains("Fantech MK894"));

        drop(client);
    }

    #[test]
    fn test_openrgb_active_profile_does_not_override_mode_on_updateleds() {
        let last_cfg = Arc::new(Mutex::new(None));
        let last_mat = Arc::new(Mutex::new(None));
        let mock_dev = MockDeviceControl {
            last_lighting_config: Arc::clone(&last_cfg),
            last_matrix: Arc::clone(&last_mat),
        };

        let dev_control: Arc<Mutex<Box<dyn DeviceControl>>> = Arc::new(Mutex::new(Box::new(mock_dev)));
        let cached_matrix = Arc::new(Mutex::new([RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT]));
        let openrgb_brightness = Arc::new(AtomicU8::new(100));
        let openrgb_mode = Arc::new(AtomicU8::new(16)); // Initially Direct mode

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test port");
        let local_addr = listener.local_addr().unwrap();
        let mut client = std::net::TcpStream::connect(local_addr).expect("connect client");
        let (server_stream, _) = listener.accept().expect("accept server");
        drop(listener);

        let dev_ref = Arc::clone(&dev_control);
        let mat_ref = Arc::clone(&cached_matrix);
        let brt_ref = Arc::clone(&openrgb_brightness);
        let mode_ref = Arc::clone(&openrgb_mode);
        thread::spawn(move || {
            handle_openrgb_client(server_stream, dev_ref, mat_ref, brt_ref, mode_ref);
        });

        // 1. Simulate OpenRGB Profile Loading (e.g., Mars v2 profile with Bloom Pulse mode 7)
        // Profile serialization in OpenRGB JSON leaves value = 0.
        let mut set_mode_bloom = Vec::new();
        set_mode_bloom.extend_from_slice(&7u32.to_le_bytes()); // mode_idx = 7 (Bloom Pulse)
        set_mode_bloom.extend_from_slice(&0u16.to_le_bytes()); // name length
        set_mode_bloom.extend_from_slice(&0i32.to_le_bytes()); // value = 0 (OpenRGB profile serialization behavior)
        set_mode_bloom.extend_from_slice(&0u32.to_le_bytes()); // flags
        set_mode_bloom.extend_from_slice(&[0u8; 24]); // spd/brt/col min/max
        set_mode_bloom.extend_from_slice(&3u32.to_le_bytes()); // speed = 3
        set_mode_bloom.extend_from_slice(&100u32.to_le_bytes()); // brightness = 100%
        set_mode_bloom.extend_from_slice(&0u32.to_le_bytes()); // direction
        set_mode_bloom.extend_from_slice(&(MODE_COLORS_MODE_SPECIFIC).to_le_bytes()); // color_mode = 2
        set_mode_bloom.extend_from_slice(&1u16.to_le_bytes()); // colors count = 1
        set_mode_bloom.extend_from_slice(&[187, 42, 5, 0]); // Mars Orange RGB(187, 42, 5)

        let mut pkt_body = (set_mode_bloom.len() as u32 + 4).to_le_bytes().to_vec();
        pkt_body.extend_from_slice(&set_mode_bloom);

        let hdr = OpenRgbHeader::new(0, NET_PACKET_ID_RGBCONTROLLER_SETMODE, pkt_body.len() as u32);
        client.write_all(&hdr.to_bytes()).unwrap();
        client.write_all(&pkt_body).unwrap();
        client.flush().unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // Verify hardware received Bloom Pulse with Mars Orange
        let cfg = last_cfg.lock().unwrap().take().expect("bloom pulse config applied");
        assert_eq!(cfg.mode, LightingMode::Bloom);
        assert_eq!(cfg.color_mode, ColorMode::FixedColor);
        assert_eq!(cfg.color, RgbColor::new(187, 42, 5));
        assert_eq!(openrgb_mode.load(Ordering::SeqCst), LightingMode::Bloom as u8);

        // 2. OpenRGB immediately follows SetMode with UpdateLEDs containing the controller colors array.
        // In the bug, this unconditionally called apply_matrix_canvas, overwriting Bloom with random matrix colors!
        // With our fix, since openrgb_mode is Bloom (6 != 16), apply_matrix_canvas must NOT be called.
        let mut leds_pkt = Vec::new();
        leds_pkt.extend_from_slice(&0u32.to_le_bytes()); // data_size placeholder
        leds_pkt.extend_from_slice(&(KEY_DEFINITIONS.len() as u16).to_le_bytes());
        for _ in 0..KEY_DEFINITIONS.len() {
            leds_pkt.extend_from_slice(&[255, 0, 0, 0]); // Red dummy colors
        }
        let total_size = (leds_pkt.len() - 4) as u32;
        leds_pkt[0..4].copy_from_slice(&total_size.to_le_bytes());

        let hdr = OpenRgbHeader::new(0, NET_PACKET_ID_RGBCONTROLLER_UPDATELEDS, leds_pkt.len() as u32);
        client.write_all(&hdr.to_bytes()).unwrap();
        client.write_all(&leds_pkt).unwrap();
        client.flush().unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // Crucial assertion: matrix canvas was NOT applied to hardware!
        assert!(last_mat.lock().unwrap().is_none(), "UpdateLEDs must NOT clobber active hardware animation modes!");

        // 3. But when switching back to Direct / Custom Matrix Mode (Mode 16), UpdateLEDs DOES stream to hardware!
        let set_custom_hdr = OpenRgbHeader::new(0, NET_PACKET_ID_RGBCONTROLLER_SETCUSTOMMODE, 0);
        client.write_all(&set_custom_hdr.to_bytes()).unwrap();
        client.flush().unwrap();

        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(openrgb_mode.load(Ordering::SeqCst), 16);
        let _ = last_mat.lock().unwrap().take(); // Drain mode switch canvas

        // Now send UpdateLEDs again with green colors
        let mut leds_green = Vec::new();
        leds_green.extend_from_slice(&0u32.to_le_bytes());
        leds_green.extend_from_slice(&(KEY_DEFINITIONS.len() as u16).to_le_bytes());
        for _ in 0..KEY_DEFINITIONS.len() {
            leds_green.extend_from_slice(&[0, 255, 0, 0]);
        }
        let total_size = (leds_green.len() - 4) as u32;
        leds_green[0..4].copy_from_slice(&total_size.to_le_bytes());

        let hdr = OpenRgbHeader::new(0, NET_PACKET_ID_RGBCONTROLLER_UPDATELEDS, leds_green.len() as u32);
        client.write_all(&hdr.to_bytes()).unwrap();
        client.write_all(&leds_green).unwrap();
        client.flush().unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let m = last_mat.lock().unwrap().take().expect("matrix canvas applied in direct mode");
        assert_eq!(m[0], RgbColor::new(0, 255, 0));

        // 4. Test directional wave mode with OPENRGB_DIR_RIGHT (Mode 5 - Horizontal Stream)
        let mut set_mode_wave = Vec::new();
        set_mode_wave.extend_from_slice(&3u32.to_le_bytes()); // mode_idx = 3 (Horizontal Wave)
        let wave_name = "Rainbow Wave (Horizontal)\0";
        set_mode_wave.extend_from_slice(&(wave_name.len() as u16).to_le_bytes());
        set_mode_wave.extend_from_slice(wave_name.as_bytes());
        set_mode_wave.extend_from_slice(&5i32.to_le_bytes()); // value = 5
        set_mode_wave.extend_from_slice(&0u32.to_le_bytes()); // flags
        set_mode_wave.extend_from_slice(&[0u8; 24]);
        set_mode_wave.extend_from_slice(&2u32.to_le_bytes()); // speed
        set_mode_wave.extend_from_slice(&100u32.to_le_bytes()); // brightness
        set_mode_wave.extend_from_slice(&OPENRGB_DIR_RIGHT.to_le_bytes()); // direction = Right
        set_mode_wave.extend_from_slice(&0u32.to_le_bytes()); // color_mode
        set_mode_wave.extend_from_slice(&0u16.to_le_bytes()); // colors count

        let mut wave_body = (set_mode_wave.len() as u32 + 4).to_le_bytes().to_vec();
        wave_body.extend_from_slice(&set_mode_wave);

        let hdr = OpenRgbHeader::new(0, NET_PACKET_ID_RGBCONTROLLER_SETMODE, wave_body.len() as u32);
        client.write_all(&hdr.to_bytes()).unwrap();
        client.write_all(&wave_body).unwrap();
        client.flush().unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let wave_cfg = last_cfg.lock().unwrap().take().expect("wave config applied");
        assert_eq!(wave_cfg.mode, LightingMode::StreamLeftToRight);
        assert_eq!(wave_cfg.direction, AnimationDirection::LeftToRight);

        drop(client);
    }
}
}

#[cfg(unix)]
pub use unix_service::*;

#[cfg(target_os = "windows")]
mod windows_service {
    use super::*;

    pub fn run_service(
        use_emulator: bool,
        _explicit_device: Option<PathBuf>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let port = WINDOWS_IPC_PORT;
        if is_service_running() {
            eprintln!("❌ Error: FantasTech background service is already running on 127.0.0.1:{}", port);
            return Err("Service already active".into());
        }

        let listener = TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true)?;

        println!("============================================================");
        println!("🌈 FantasTech Unified Keyboard Service Daemon (Windows)");
        println!("   Target Hardware: Fantech MK894 Maxpower II (0x36AE:0xFE9C)");
        println!("   IPC Endpoint:    127.0.0.1:{}", port);
        println!("============================================================");

        let config = Arc::new(Mutex::new(FantasTechConfig::load()));
        let profile_manager = Arc::new(ProfileManager::new(None));
        let _ = profile_manager.ensure_seed_profiles();

        // Connect to hardware or emulator
        let dev_control: Arc<Mutex<Box<dyn DeviceControl>>> = if use_emulator {
            println!("🎮 Using Virtual Keyboard Emulator.");
            let emu = VirtualKeyboardEmulator::new();
            let dev = Device::new_read_only(emu).into_reversible();
            Arc::new(Mutex::new(Box::new(DeviceHolder::Reversible(dev))))
        } else {
            match HidrawTransport::discover() {
                Ok(t) => {
                    println!("🔌 Connected to MK894 hardware transport.");
                    let dev = Device::new_read_only(t).into_reversible();
                    Arc::new(Mutex::new(Box::new(DeviceHolder::Reversible(dev))))
                }
                Err(e) => {
                    eprintln!("⚠️ Could not open physical keyboard: {}. Running in simulated emulator mode.", e);
                    let emu = VirtualKeyboardEmulator::new();
                    let dev = Device::new_read_only(emu).into_reversible();
                    Arc::new(Mutex::new(Box::new(DeviceHolder::Reversible(dev))))
                }
            }
        };

        let running = Arc::new(AtomicBool::new(true));
        let openrgb_running = Arc::new(AtomicBool::new(false));
        let volume_guard_running = Arc::new(AtomicBool::new(false));
        let openrgb_matrix = Arc::new(Mutex::new([RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT]));
        let openrgb_brightness = Arc::new(AtomicU8::new(100));
        let initial_mode = {
            let cfg = config.lock().unwrap();
            if let Some(p) = profile_manager.load_lighting_profile(&cfg.active_lighting_profile) {
                p.mode as u8
            } else {
                16
            }
        };
        let openrgb_mode = Arc::new(AtomicU8::new(initial_mode));

        // Start OpenRGB server if enabled
        {
            let cfg = config.lock().unwrap();
            if cfg.openrgb_enabled {
                openrgb_running.store(true, Ordering::SeqCst);
                spawn_openrgb_server(
                    cfg.openrgb_host.clone(),
                    cfg.openrgb_port,
                    Arc::clone(&dev_control),
                    Arc::clone(&running),
                    Arc::clone(&openrgb_running),
                    Arc::clone(&openrgb_matrix),
                    Arc::clone(&openrgb_brightness),
                    Arc::clone(&openrgb_mode),
                );
            }
        }

        // Start system tray applet if enabled in config
        {
            let cfg = config.lock().unwrap();
            if cfg.tray_icon_enabled && !fantastech_core::is_tray_already_running() {
                crate::tray::spawn_native_tray();
            }
        }

        println!("🚀 FantasTech Service initialized and listening for IPC connections.");

        let mut last_tray_check = std::time::Instant::now();
        while running.load(Ordering::SeqCst) {
            // Ensure tray remains active if enabled (throttled every 2 seconds)
            if last_tray_check.elapsed() >= Duration::from_secs(2) {
                last_tray_check = std::time::Instant::now();
                let cfg = config.lock().unwrap();
                if cfg.tray_icon_enabled && !fantastech_core::is_tray_already_running() {
                    crate::tray::spawn_native_tray();
                }
            }

            match poll_socket_ready(&listener, 1000) {
                Ok(true) => {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let dev_ref = Arc::clone(&dev_control);
                            let cfg_ref = Arc::clone(&config);
                            let prof_ref = Arc::clone(&profile_manager);
                            let run_ref = Arc::clone(&running);
                            let openrgb_flag = Arc::clone(&openrgb_running);
                            let volume_flag = Arc::clone(&volume_guard_running);
                            let openrgb_mat = Arc::clone(&openrgb_matrix);
                            let openrgb_brt = Arc::clone(&openrgb_brightness);
                            let openrgb_md = Arc::clone(&openrgb_mode);

                            thread::spawn(move || {
                                handle_tcp_ipc_connection(
                                    stream,
                                    dev_ref,
                                    cfg_ref,
                                    prof_ref,
                                    run_ref,
                                    openrgb_flag,
                                    volume_flag,
                                    openrgb_mat,
                                    openrgb_brt,
                                    openrgb_md,
                                );
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => {
                            eprintln!("⚠️ IPC accept error: {}", e);
                        }
                    }
                }
                Ok(false) => {}
                Err(_) => break,
            }
        }

        println!("🛑 Shutting down FantasTech Service...");
        println!("👋 FantasTech Service cleanly stopped.");
        Ok(())
    }

    pub fn handle_service_command(cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
        match cmd {
            "start" => {
                if is_service_running() {
                    println!("✅ FantasTech background service is already running on Windows.");
                    return Ok(());
                }
                println!("🚀 Starting FantasTech background service on Windows...");
                let bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech.exe"));
                let mut cmd = Command::new(bin);
                cmd.args(["service", "run"]);
                cmd.stdin(std::process::Stdio::null());
                cmd.stdout(std::process::Stdio::null());
                cmd.stderr(std::process::Stdio::null());
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    const DETACHED_PROCESS: u32 = 0x00000008;
                    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
                    const CREATE_NO_WINDOW: u32 = 0x08000000;
                    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
                }
                match cmd.spawn() {
                    Ok(_) => {
                        println!("⏳ Waiting for service initialization on 127.0.0.1:{}...", WINDOWS_IPC_PORT);
                        for _ in 0..20 {
                            thread::sleep(Duration::from_millis(100));
                            if is_service_running() {
                                println!("✅ FantasTech service daemon successfully launched in background.");
                                return Ok(());
                            }
                        }
                        println!("⚠️ Service spawned but connection timed out.");
                    }
                    Err(e) => eprintln!("❌ Failed to spawn background daemon: {}", e),
                }
            }
            "stop" => {
                if !is_service_running() {
                    println!("ℹ️ FantasTech background service is not running.");
                    return Ok(());
                }
                println!("🛑 Stopping FantasTech service...");
                let _ = send_ipc_request(&IpcRequest::ShutdownService);
                for _ in 0..10 {
                    thread::sleep(Duration::from_millis(100));
                    if !is_service_running() {
                        println!("✅ FantasTech service successfully stopped.");
                        return Ok(());
                    }
                }
                println!("✅ Stop signal transmitted.");
            }
            "restart" => {
                let _ = handle_service_command("stop");
                thread::sleep(Duration::from_millis(500));
                let _ = handle_service_command("start");
            }
            "status" => {
                if is_service_running() {
                    match send_ipc_request(&IpcRequest::GetStatus) {
                        Ok(IpcResponse::Status(s)) => {
                            println!("● FantasTech Background Keyboard Service (Windows)");
                            println!("     Status:                  ACTIVE (running)");
                            println!("     Process PID:             {}", s.pid);
                            println!("     Active Lighting Profile: {}", s.active_lighting_profile);
                            println!("     Active Button Profile:   {}", s.active_button_profile);
                            println!("     OpenRGB SDK Server:      {}", if s.openrgb_running { format!("Running (Port {})", s.openrgb_port) } else { "Disabled".to_string() });
                            println!("     IPC Endpoint:            127.0.0.1:{}", WINDOWS_IPC_PORT);
                        }
                        _ => println!("● FantasTech Background Keyboard Service: ACTIVE (responding to ping)"),
                    }
                } else {
                    println!("○ FantasTech Background Keyboard Service (Windows)");
                    println!("     Status: INACTIVE (Service is not running)");
                    println!("     Run 'fantastech service start' or 'fantastech service run' to launch.");
                }
            }
            "install" => {
                install_windows_user_service()?;
            }
            "uninstall" => {
                uninstall_windows_user_service()?;
            }
            other => eprintln!("Unknown service subcommand '{}'.", other),
        }
        Ok(())
    }

    fn install_windows_user_service() -> Result<(), Box<dyn std::error::Error>> {
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
        if let Some(appdata) = appdata {
            let startup_dir = appdata.join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("Startup");
            if startup_dir.exists() {
                let bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech.exe"));
                let cmd_file = startup_dir.join("FantasTech-Service.cmd");
                let content = format!("@echo off\r\nstart \"\" \"{}\" service run\r\n", bin.display());
                std::fs::write(&cmd_file, content)?;
                println!("✅ Registered FantasTech service for automatic user login (Startup: {}).", cmd_file.display());
                return Ok(());
            }
        }
        println!("ℹ️ Automatic start can also be enabled during installation via the Inno Setup installer.");
        Ok(())
    }

    fn uninstall_windows_user_service() -> Result<(), Box<dyn std::error::Error>> {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let startup_dir = PathBuf::from(appdata)
                .join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("Startup");
            let _ = std::fs::remove_file(startup_dir.join("FantasTech Service.lnk"));
            let _ = std::fs::remove_file(startup_dir.join("FantasTech-Service.cmd"));
            let _ = std::fs::remove_file(startup_dir.join("FantasTech Service.cmd"));
        }
        println!("✅ FantasTech service unregistered from Windows Startup.");
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_tcp_ipc_connection(
        mut stream: TcpStream,
        dev_control: Arc<Mutex<Box<dyn DeviceControl>>>,
        config: Arc<Mutex<FantasTechConfig>>,
        profile_manager: Arc<ProfileManager>,
        running: Arc<AtomicBool>,
        openrgb_running: Arc<AtomicBool>,
        volume_guard_running: Arc<AtomicBool>,
        openrgb_matrix: Arc<Mutex<[RgbColor; SPARSE_KEY_COUNT]>>,
        openrgb_brightness: Arc<AtomicU8>,
        openrgb_mode: Arc<AtomicU8>,
    ) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

        let mut reader = BufReader::new(stream.try_clone().unwrap_or_else(|_| stream.try_clone().unwrap()));
        handle_ipc_io(
            &mut reader,
            &mut stream,
            &dev_control,
            &config,
            &profile_manager,
            &running,
            &openrgb_running,
            &volume_guard_running,
            &openrgb_matrix,
            &openrgb_brightness,
            &openrgb_mode,
        );
    }
}

#[cfg(target_os = "windows")]
pub use windows_service::*;

#[cfg(all(not(unix), not(target_os = "windows")))]
pub fn run_service(_emulator: bool, _device: Option<std::path::PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("ℹ️ The background service daemon is supported on Linux and Windows.");
    Ok(())
}

#[cfg(all(not(unix), not(target_os = "windows")))]
pub fn handle_service_command(_action: &str) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("ℹ️ The background service daemon is supported on Linux and Windows.");
    Ok(())
}
