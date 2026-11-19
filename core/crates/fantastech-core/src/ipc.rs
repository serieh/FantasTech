//! Unix Domain Socket IPC for FantasTech suite.
//!
//! Enables zero-contention communication between `fantastech-cli`, `fantastech-gui`,
//! the System Tray applet, and the unified background `fantastech service`.

#![forbid(unsafe_code)]

use crate::config::{get_fantastech_config_dir, FantasTechConfig};
use fantastech_protocol::commands::DeviceInfo;
use fantastech_protocol::keymap::{KeyAction, KeyLayer};
use fantastech_protocol::lighting::{ColorMode, LightingConfig, LightingMode};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// Returns the standard Unix Domain Socket path for FantasTech IPC.
pub fn ipc_socket_path() -> PathBuf {
    get_fantastech_config_dir().join("fantastech.sock")
}

/// Commands and requests that can be sent over IPC to `fantastech service`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcRequest {
    /// Keepalive ping.
    Ping,
    /// Query background service running status and active states.
    GetStatus,
    /// Retrieve active global configuration.
    GetConfig,
    /// Query active hardware lighting configuration.
    GetLightingConfig,
    /// Query active hardware device identification and profile.
    GetDeviceInfo,
    /// Update global configuration.
    SaveConfig(FantasTechConfig),
    /// Apply a saved lighting profile by identifier.
    ApplyLightingProfile { id: String },
    /// Set raw hardware lighting animation mode and parameters.
    SetLightingMode {
        mode: LightingMode,
        brightness: u8,
        speed: u8,
        direction: u8,
        color_mode: ColorMode,
        color: [u8; 3],
    },
    /// Set all keys to a single static RGB color.
    SetDirectColor { r: u8, g: u8, b: u8 },
    /// Set a single physical key's RGB color.
    SetKeyColor { key_index: u8, r: u8, g: u8, b: u8 },
    /// Set the full RGB canvas matrix.
    SetCanvasMatrix { colors: Vec<[u8; 3]> },
    /// Turn off all keyboard backlighting immediately.
    LightingOff,
    /// Switch active hardware onboard profile (0..2).
    SwitchHardwareProfile(u8),
    /// Apply a saved button / keymap profile by identifier.
    ApplyButtonProfile { id: String },
    /// Apply a saved macro profile by identifier.
    ApplyMacroProfile { id: String },
    /// Apply a saved settings profile by identifier.
    ApplySettingsProfile { id: String },
    /// Apply raw keymap layer actions directly.
    ApplyKeymapLayer {
        layer: KeyLayer,
        actions: Vec<KeyAction>,
    },
    /// Set inactivity auto-sleep timeout in seconds.
    SetSleepTimeout(u16),
    /// Execute factory reset on hardware.
    FactoryReset,
    /// Enable or disable the OpenRGB SDK server.
    SetOpenRgbEnabled(bool),
    /// Enable or disable the Volume Guard debouncer.
    SetVolumeGuardEnabled(bool),
    /// Request clean shutdown of the background service daemon.
    ShutdownService,
}

/// Real-time service state information returned by `GetStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub service_running: bool,
    pub active_lighting_profile: String,
    pub active_button_profile: String,
    pub volume_guard_running: bool,
    pub openrgb_running: bool,
    pub openrgb_port: u16,
    pub pid: u32,
    #[serde(default)]
    pub firmware_update_available: bool,
}

/// Responses sent by `fantastech service` over IPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcResponse {
    Ok,
    Pong,
    Status(ServiceStatus),
    Config(FantasTechConfig),
    LightingConfig(LightingConfig),
    DeviceInfo(DeviceInfo),
    Error(String),
}

/// Sends a request to the running `fantastech service` and awaits its response.
#[cfg(unix)]
pub fn send_ipc_request(req: &IpcRequest) -> Result<IpcResponse, String> {
    let socket_path = ipc_socket_path();
    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|e| format!("Failed to connect to FantasTech service socket at {}: {}", socket_path.display(), e))?;

    stream
        .set_read_timeout(Some(Duration::from_millis(2500)))
        .map_err(|e| format!("Failed to set socket read timeout: {}", e))?;
    stream
        .set_write_timeout(Some(Duration::from_millis(2500)))
        .map_err(|e| format!("Failed to set socket write timeout: {}", e))?;

    let json = serde_json::to_string(req)
        .map_err(|e| format!("Failed to serialize IPC request: {}", e))?;

    stream
        .write_all(json.as_bytes())
        .map_err(|e| format!("Failed to send IPC request: {}", e))?;
    stream
        .write_all(b"\n")
        .map_err(|e| format!("Failed to send newline delimiter: {}", e))?;
    stream
        .flush()
        .map_err(|e| format!("Failed to flush IPC stream: {}", e))?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .map_err(|e| format!("Failed to read response from service: {}", e))?;

    if response_line.trim().is_empty() {
        return Err("Received empty response from FantasTech service".to_string());
    }

    let resp: IpcResponse = serde_json::from_str(&response_line)
        .map_err(|e| format!("Failed to parse service response: {}", e))?;

    Ok(resp)
}

/// Returns true if `fantastech service` is running and responsive.
#[cfg(unix)]
pub fn is_service_running() -> bool {
    let socket_path = ipc_socket_path();
    if !socket_path.exists() {
        return false;
    }
    matches!(send_ipc_request(&IpcRequest::Ping), Ok(IpcResponse::Pong))
}

/// Port for localhost TCP IPC on Windows.
pub const WINDOWS_IPC_PORT: u16 = 45894;

/// Port for localhost system tray single-instance guard on Windows.
pub const WINDOWS_TRAY_PORT: u16 = 45895;

/// Port for localhost GUI single-instance guard on Windows.
pub const WINDOWS_GUI_PORT: u16 = 45896;

/// Sends a request to the running `fantastech service` on Windows via localhost TCP IPC.
#[cfg(windows)]
pub fn send_ipc_request(req: &IpcRequest) -> Result<IpcResponse, String> {
    use std::net::TcpStream;

    let mut stream = TcpStream::connect(("127.0.0.1", WINDOWS_IPC_PORT))
        .map_err(|e| format!("Failed to connect to FantasTech service on localhost:{}: {}", WINDOWS_IPC_PORT, e))?;

    stream.set_read_timeout(Some(Duration::from_millis(2500)))
        .map_err(|e| format!("Failed to set socket read timeout: {}", e))?;
    stream.set_write_timeout(Some(Duration::from_millis(2500)))
        .map_err(|e| format!("Failed to set socket write timeout: {}", e))?;

    let json = serde_json::to_string(req)
        .map_err(|e| format!("Failed to serialize IPC request: {}", e))?;

    stream.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to send IPC request: {}", e))?;
    stream.write_all(b"\n")
        .map_err(|e| format!("Failed to send newline delimiter: {}", e))?;
    stream.flush()
        .map_err(|e| format!("Failed to flush IPC stream: {}", e))?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)
        .map_err(|e| format!("Failed to read response from service: {}", e))?;

    if response_line.trim().is_empty() {
        return Err("Received empty response from FantasTech service".to_string());
    }

    let resp: IpcResponse = serde_json::from_str(&response_line)
        .map_err(|e| format!("Failed to parse service response: {}", e))?;

    Ok(resp)
}

#[cfg(windows)]
pub fn is_service_running() -> bool {
    matches!(send_ipc_request(&IpcRequest::Ping), Ok(IpcResponse::Pong))
}

/// Fallback for non-unix/non-windows targets where daemon IPC is bypassed for direct hardware access.
#[cfg(all(not(unix), not(windows)))]
pub fn send_ipc_request(_req: &IpcRequest) -> Result<IpcResponse, String> {
    Err("IPC daemon is not active on this operating system; using direct hardware access".to_string())
}

#[cfg(all(not(unix), not(windows)))]
pub fn is_service_running() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_serialization_roundtrip() {
        let req = IpcRequest::ApplyLightingProfile {
            id: "cyberpunk".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, parsed);

        let status = ServiceStatus {
            service_running: true,
            active_lighting_profile: "gaming".to_string(),
            active_button_profile: "default".to_string(),
            volume_guard_running: true,
            openrgb_running: false,
            openrgb_port: 6742,
            pid: 1234,
            firmware_update_available: false,
        };
        let resp = IpcResponse::Status(status);
        let json_resp = serde_json::to_string(&resp).unwrap();
        let parsed_resp: IpcResponse = serde_json::from_str(&json_resp).unwrap();
        assert_eq!(resp, parsed_resp);

        // Test SetLightingMode roundtrip with ColorMode::DynamicSpectrum
        let lighting_req = IpcRequest::SetLightingMode {
            mode: LightingMode::Breathing,
            brightness: 3,
            speed: 2,
            direction: 0,
            color_mode: ColorMode::DynamicSpectrum,
            color: [255, 128, 0],
        };
        let l_json = serde_json::to_string(&lighting_req).unwrap();
        let parsed_l: IpcRequest = serde_json::from_str(&l_json).unwrap();
        assert_eq!(lighting_req, parsed_l);
    }
}
