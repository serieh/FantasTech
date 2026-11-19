//! Platform integration and background connection monitoring.
//!
//! The UI only consumes snapshots from this module. HID discovery and service
//! detection never run from the egui frame callback.

use super::{is_service_running, HidrawTransport, TransportError};
use fantastech_core::ServiceStatus;
use fantastech_device::Device;
use fantastech_protocol::commands::DeviceInfo;
use fantastech_protocol::constants::SPARSE_KEY_COUNT;
use fantastech_protocol::keymap::{KeyAction, KeyLayer};
use fantastech_protocol::lighting::LightingConfig;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::time::Duration;

#[allow(dead_code)]
const CONNECTION_POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub(super) struct ConnectionSnapshot {
    pub(super) service_active: bool,
    pub(super) connected: bool,
    pub(super) is_emulator: bool,
    pub(super) permission_denied_node: Option<PathBuf>,
    pub(super) error_message: Option<String>,
    pub(super) lighting_config: Option<LightingConfig>,
    pub(super) device_info: Option<DeviceInfo>,
    pub(super) hardware_keymaps: Option<([KeyAction; SPARSE_KEY_COUNT], [KeyAction; SPARSE_KEY_COUNT])>,
    pub(super) service_status: Option<ServiceStatus>,
}

impl ConnectionSnapshot {
    #[cfg(test)]
    pub(super) fn new_mock() -> Self {
        Self {
            service_active: false,
            connected: true,
            is_emulator: true,
            permission_denied_node: None,
            error_message: None,
            lighting_config: None,
            device_info: None,
            hardware_keymaps: None,
            service_status: None,
        }
    }
}

pub(super) struct ConnectionMonitor {
    receiver: Receiver<ConnectionSnapshot>,
    refresh_sender: SyncSender<()>,
}

impl ConnectionMonitor {
    #[cfg(test)]
    pub(super) fn new_mock() -> Self {
        let (_sender, receiver) = mpsc::channel();
        let (refresh_sender, _refresh_receiver) = mpsc::sync_channel(1);
        Self { receiver, refresh_sender }
    }

    #[allow(dead_code)]
    pub(super) fn start() -> Self {
        let (sender, receiver) = mpsc::channel();
        let fallback_sender = sender.clone();
        let (refresh_sender, refresh_receiver) = mpsc::sync_channel(1);

        // The worker exits naturally when the application drops its receiver.
        // A detached thread avoids blocking window shutdown on a slow HID scan.
        let worker = std::thread::Builder::new()
            .name("fantastech-connection-monitor".to_owned())
            .spawn(move || {
                loop {
                    let full_scan = match refresh_receiver.recv_timeout(CONNECTION_POLL_INTERVAL) {
                        Ok(()) => true,
                        Err(RecvTimeoutError::Timeout) => false,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                    let snapshot = detect_connection(full_scan);
                    if sender.send(snapshot).is_err() {
                        break;
                    }
                }
            });

        if let Err(error) = worker {
            let _ = fallback_sender.send(ConnectionSnapshot {
                service_active: false,
                connected: false,
                is_emulator: true,
                permission_denied_node: None,
                error_message: Some(format!("Could not start the connection monitor: {error}")),
                lighting_config: None,
                device_info: None,
                hardware_keymaps: None,
                service_status: None,
            });
        }

        Self { receiver, refresh_sender }
    }

    pub(super) fn take_latest(&self) -> Option<ConnectionSnapshot> {
        let mut latest = None;
        while let Ok(snapshot) = self.receiver.try_recv() {
            latest = Some(snapshot);
        }
        latest
    }

    pub(super) fn refresh(&self) {
        let _ = self.refresh_sender.try_send(());
    }
}

#[allow(dead_code)]
pub(super) fn detect_connection(include_keymaps: bool) -> ConnectionSnapshot {
    if is_service_running() {
        let lighting_config = match fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::GetLightingConfig) {
            Ok(fantastech_core::IpcResponse::LightingConfig(cfg)) => Some(cfg),
            _ => None,
        };
        let device_info = match fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::GetDeviceInfo) {
            Ok(fantastech_core::IpcResponse::DeviceInfo(info)) => Some(info),
            _ => None,
        };
        let service_status = match fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::GetStatus) {
            Ok(fantastech_core::IpcResponse::Status(st)) => Some(st),
            _ => None,
        };
        return ConnectionSnapshot {
            service_active: true,
            connected: true,
            is_emulator: false,
            permission_denied_node: None,
            error_message: None,
            lighting_config,
            device_info,
            hardware_keymaps: None,
            service_status,
        };
    }

    match HidrawTransport::discover() {
        Ok(transport) => {
            let mut dev = Device::new_read_only(transport);
            let device_info = dev.query_info().ok();
            let lighting_config = dev.query_light_config().ok();
            let hardware_keymaps = if include_keymaps {
                let base_layer = dev.read_keymap_layer(KeyLayer::Base).ok();
                let fn_layer = dev.read_keymap_layer(KeyLayer::Fn).ok();
                match (base_layer, fn_layer) {
                    (Some(b), Some(f)) => Some((b, f)),
                    (Some(b), None) => Some((b, [KeyAction::disabled(); SPARSE_KEY_COUNT])),
                    _ => None,
                }
            } else {
                None
            };
            ConnectionSnapshot {
                service_active: false,
                connected: true,
                is_emulator: false,
                permission_denied_node: None,
                error_message: None,
                lighting_config,
                device_info,
                hardware_keymaps,
                service_status: None,
            }
        }
        Err(TransportError::PermissionDenied(path)) => ConnectionSnapshot {
            service_active: false,
            connected: false,
            is_emulator: false,
            permission_denied_node: Some(path),
            error_message: None,
            lighting_config: None,
            device_info: None,
            hardware_keymaps: None,
            service_status: None,
        },
        Err(_) => ConnectionSnapshot {
            service_active: false,
            connected: false,
            is_emulator: true,
            permission_denied_node: None,
            error_message: None,
            lighting_config: None,
            device_info: None,
            hardware_keymaps: None,
            service_status: None,
        },
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[allow(dead_code)]
pub(super) struct IntegrationStatus {
    pub(super) binaries: bool,
    pub(super) desktop_launcher: bool,
    pub(super) user_service: bool,
    pub(super) device_rules: bool,
}

#[allow(dead_code)]
pub(super) fn integration_status() -> IntegrationStatus {
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        IntegrationStatus {
            binaries: home.join(".local/bin/fantastech").exists()
                || PathBuf::from("/usr/local/bin/fantastech").exists(),
            desktop_launcher: home
                .join(".local/share/applications/fantastech.desktop")
                .exists()
                || PathBuf::from("/usr/share/applications/fantastech.desktop").exists(),
            user_service: home
                .join(".config/systemd/user/fantastech.service")
                .exists(),
            device_rules: PathBuf::from("/etc/udev/rules.d/99-fantastech.rules").exists()
                || PathBuf::from("/etc/udev/rules.d/70-fantech-mk894.rules").exists(),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        IntegrationStatus::default()
    }
}

#[cfg(target_os = "linux")]
fn is_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || std::path::Path::new("/.flatpak-info").exists()
}

#[cfg(target_os = "linux")]
fn has_systemctl() -> bool {
    std::process::Command::new("systemctl")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub(super) fn service_action(action: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let user_service = home.join(".config/systemd/user/fantastech.service");

        match action {
            "start" => {
                if is_service_running() {
                    return Ok(());
                }

                // 1. Try systemd user service only if unit file exists and systemctl is available
                if user_service.exists() && has_systemctl() {
                    let systemd_status = std::process::Command::new("systemctl")
                        .args(["--user", "start", "fantastech.service"])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();

                    if let Ok(s) = systemd_status {
                        if s.success() {
                            for _ in 0..10 {
                                std::thread::sleep(Duration::from_millis(100));
                                if is_service_running() {
                                    return Ok(());
                                }
                            }
                        }
                    }
                }

                // 2. Direct background daemon spawn fallback
                let mut candidates = Vec::new();
                if let Ok(current) = std::env::current_exe() {
                    if let Some(parent) = current.parent() {
                        candidates.push(parent.join("fantastech"));
                        candidates.push(parent.join("../release/fantastech"));
                    }
                }
                candidates.push(PathBuf::from("./core/target/release/fantastech"));
                candidates.push(home.join(".local/bin/fantastech"));
                candidates.push(PathBuf::from("/usr/local/bin/fantastech"));

                let chosen_bin = candidates.into_iter().find(|p| p.exists())
                    .unwrap_or_else(|| PathBuf::from("fantastech"));

                let config_dir = fantastech_core::config::get_fantastech_config_dir();
                let _ = std::fs::create_dir_all(&config_dir);
                let log_file = config_dir.join("service.log");

                let log_out = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_file)
                    .map(std::process::Stdio::from)
                    .unwrap_or_else(|_| std::process::Stdio::null());

                let log_err = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_file)
                    .map(std::process::Stdio::from)
                    .unwrap_or_else(|_| std::process::Stdio::null());

                let mut cmd = std::process::Command::new(chosen_bin);
                cmd.args(["service", "run"]);
                cmd.stdin(std::process::Stdio::null());
                cmd.stdout(log_out);
                cmd.stderr(log_err);

                let uid = std::fs::metadata("/proc/self")
                    .map(|m| {
                        use std::os::unix::fs::MetadataExt;
                        m.uid()
                    })
                    .unwrap_or(1000);
                let xdg_runtime = std::env::var("XDG_RUNTIME_DIR")
                    .unwrap_or_else(|_| format!("/run/user/{}", uid));
                let dbus_bus = std::env::var("DBUS_SESSION_BUS_ADDRESS")
                    .unwrap_or_else(|_| format!("unix:path={}/bus", xdg_runtime));
                cmd.env("XDG_RUNTIME_DIR", &xdg_runtime);
                cmd.env("DBUS_SESSION_BUS_ADDRESS", &dbus_bus);
                if let Ok(w) = std::env::var("WAYLAND_DISPLAY") {
                    cmd.env("WAYLAND_DISPLAY", w);
                }
                if let Ok(d) = std::env::var("DISPLAY") {
                    cmd.env("DISPLAY", d);
                }

                #[cfg(unix)]
                {
                    use std::os::unix::process::CommandExt;
                    cmd.process_group(0);
                }

                cmd.spawn().map_err(|e| format!("Could not spawn background daemon: {e}"))?;

                for _ in 0..15 {
                    std::thread::sleep(Duration::from_millis(100));
                    if is_service_running() {
                        return Ok(());
                    }
                }

                Ok(())
            }
            "stop" => {
                let _ = super::send_ipc_request(&super::IpcRequest::ShutdownService);
                if user_service.exists() && has_systemctl() {
                    let _ = std::process::Command::new("systemctl")
                        .args(["--user", "stop", "fantastech.service"])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }

                for _ in 0..10 {
                    std::thread::sleep(Duration::from_millis(100));
                    if !is_service_running() {
                        return Ok(());
                    }
                }
                Ok(())
            }
            "restart" => {
                let _ = service_action("stop");
                std::thread::sleep(Duration::from_millis(300));
                service_action("start")
            }
            "enable" => {
                let autostart_dir = home.join(".config/autostart");
                let _ = std::fs::create_dir_all(&autostart_dir);

                if is_flatpak() {
                    let autostart_file = autostart_dir.join("org.fantastech.FantasTech.desktop");
                    let content = r#"[Desktop Entry]
Type=Application
Name=FantasTech Service
Comment=FantasTech Background Keyboard Service Daemon
Exec=flatpak run --command=fantastech org.fantastech.FantasTech service run
Terminal=false
Icon=org.fantastech.FantasTech
Categories=Utility;HardwareSettings;
X-GNOME-Autostart-enabled=true
"#;
                    std::fs::write(&autostart_file, content)
                        .map_err(|e| format!("Could not write autostart desktop entry: {e}"))?;
                } else {
                    let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech"));
                    let autostart_file = autostart_dir.join("fantastech.desktop");
                    let content = format!(r#"[Desktop Entry]
Type=Application
Name=FantasTech Service
Comment=FantasTech Background Keyboard Service Daemon
Exec="{}" service run
Terminal=false
Icon=fantastech
Categories=Utility;HardwareSettings;
X-GNOME-Autostart-enabled=true
"#, current_exe.display());
                    let _ = std::fs::write(&autostart_file, content);

                    // If systemctl is present, also configure systemd user service
                    if has_systemctl() {
                        let user_systemd_dir = home.join(".config/systemd/user");
                        let _ = std::fs::create_dir_all(&user_systemd_dir);
                        let service_file = user_systemd_dir.join("fantastech.service");
                        let unit_content = format!(r#"[Unit]
Description=FantasTech Background Keyboard Service Daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart="{}" service run
Restart=on-failure
RestartSec=3
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
"#, current_exe.display());
                        let _ = std::fs::write(&service_file, unit_content);
                        let _ = std::process::Command::new("systemctl")
                            .args(["--user", "daemon-reload"])
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                        let _ = std::process::Command::new("systemctl")
                            .args(["--user", "enable", "fantastech.service"])
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                    }
                }

                if !is_service_running() {
                    let _ = service_action("start");
                }
                Ok(())
            }
            "disable" => {
                let autostart_dir = home.join(".config/autostart");
                let _ = std::fs::remove_file(autostart_dir.join("org.fantastech.FantasTech.desktop"));
                let _ = std::fs::remove_file(autostart_dir.join("io.github.fantastech.FantasTech.desktop"));
                let _ = std::fs::remove_file(autostart_dir.join("fantastech.desktop"));

                if !is_flatpak() && has_systemctl() {
                    let _ = std::process::Command::new("systemctl")
                        .args(["--user", "disable", "fantastech.service"])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
                Ok(())
            }
            other => {
                if has_systemctl() {
                    let status = std::process::Command::new("systemctl")
                        .args(["--user", other, "fantastech.service"])
                        .status()
                        .map_err(|error| format!("Could not run systemctl: {error}"))?;
                    if status.success() {
                        Ok(())
                    } else {
                        Err(format!("systemctl {other} returned {status}"))
                    }
                } else {
                    Err(format!("systemctl is not available in this environment (cannot run '{other}')"))
                }
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        match action {
            "start" => {
                if is_service_running() {
                    return Ok(());
                }
                let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech.exe"));
                let mut cmd = std::process::Command::new(current_exe);
                cmd.args(["service", "run"]);
                cmd.stdin(std::process::Stdio::null());
                cmd.stdout(std::process::Stdio::null());
                cmd.stderr(std::process::Stdio::null());
                use std::os::windows::process::CommandExt;
                const DETACHED_PROCESS: u32 = 0x00000008;
                const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
                const CREATE_NO_WINDOW: u32 = 0x08000000;
                cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
                cmd.spawn().map_err(|e| format!("Could not spawn background daemon: {e}"))?;
                for _ in 0..15 {
                    std::thread::sleep(Duration::from_millis(100));
                    if is_service_running() {
                        return Ok(());
                    }
                }
                Ok(())
            }
            "stop" => {
                let _ = super::send_ipc_request(&super::IpcRequest::ShutdownService);
                for _ in 0..10 {
                    std::thread::sleep(Duration::from_millis(100));
                    if !is_service_running() {
                        return Ok(());
                    }
                }
                Ok(())
            }
            "restart" => {
                let _ = service_action("stop");
                std::thread::sleep(Duration::from_millis(300));
                service_action("start")
            }
            "enable" => {
                let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech.exe"));
                let exe_str = current_exe.display().to_string();
                let mut success = false;

                if let Some(appdata) = std::env::var_os("APPDATA") {
                    let startup_dir = PathBuf::from(appdata)
                        .join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("Startup");
                    if startup_dir.exists() {
                        let cmd_file = startup_dir.join("FantasTech-Service.cmd");
                        let content = format!("@echo off\r\nstart \"\" \"{}\" service run\r\n", exe_str);
                        if std::fs::write(&cmd_file, content).is_ok() {
                            success = true;
                        }
                    }
                }

                if success {
                    if !is_service_running() {
                        let _ = service_action("start");
                    }
                    Ok(())
                } else {
                    Err("Failed to register auto-start in Windows Startup".to_string())
                }
            }
            "disable" => {
                if let Some(appdata) = std::env::var_os("APPDATA") {
                    let startup_dir = PathBuf::from(appdata)
                        .join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("Startup");
                    let _ = std::fs::remove_file(startup_dir.join("FantasTech Service.lnk"));
                    let _ = std::fs::remove_file(startup_dir.join("FantasTech-Service.cmd"));
                    let _ = std::fs::remove_file(startup_dir.join("FantasTech Service.cmd"));
                }
                Ok(())
            }
            other => Err(format!("Unknown service action: {other}")),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = action;
        Err("Background-service management is not supported on this platform.".to_owned())
    }
}


pub(super) fn permission_error(operation: &str, path: &std::path::Path) -> String {
    #[cfg(target_os = "linux")]
    {
        format!(
            "Cannot {operation}: access to {} is denied. Install the FantasTech udev rule, then reconnect the keyboard.",
            path.display()
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        format!(
            "Cannot {operation}: the operating system denied access to {}. Check device permissions and confirm that a compatible HID backend is available.",
            path.display()
        )
    }
}

pub(super) fn launch_tray_applet() -> Result<(), String> {
    let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech"));
    std::process::Command::new(current_exe)
        .arg("tray")
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not launch native tray applet: {error}"))
}

pub(super) fn is_autostart_registered() -> bool {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let startup_dir = PathBuf::from(appdata)
                .join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("Startup");
            if startup_dir.join("FantasTech Service.lnk").exists()
                || startup_dir.join("FantasTech-Service.cmd").exists()
                || startup_dir.join("FantasTech Service.cmd").exists()
            {
                return true;
            }
        }
        if let Ok(output) = std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "FantasTechService"])
            .output()
        {
            if output.status.success() {
                return true;
            }
        }
        false
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let autostart_dir = home.join(".config/autostart");
        if autostart_dir.join("org.fantastech.FantasTech.desktop").exists()
            || autostart_dir.join("fantastech.desktop").exists()
        {
            return true;
        }
        if has_systemctl() {
            if let Ok(output) = std::process::Command::new("systemctl")
                .args(["--user", "is-enabled", "fantastech.service"])
                .output()
            {
                if output.status.success() {
                    return true;
                }
            }
        }
        false
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        false
    }
}
