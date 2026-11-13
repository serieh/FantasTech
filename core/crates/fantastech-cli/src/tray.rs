//! FantasTech — Native Freedesktop StatusNotifierItem (System Tray) & DBusMenu
//!
//! Provides a 100% pure-Rust StatusNotifierItem and DBusMenu implementation for KDE Plasma,
//! GNOME (AppIndicator), XFCE, and modern Wayland/X11 desktops over D-Bus.
//!
//! Enforces:
//! - Strictly ONE system tray instance across the entire user session.
//! - Strictly ONE application window: Left click or context menu "Show" brings the existing
//!   window to focus without spawning duplicate processes or windows.
//! - Rich context menu matching the full FantasTech control suite:
//!   * Header title: FantasTech — MK894
//!   * Lighting Profiles submenu (Gaming, Cyberpunk, Rainbow Wave, Neon Sunset, Matrix, Static Purple)
//!   * Quick Colors submenu (Electric Blue, Red, Green, Blue, Cyan, Magenta, Yellow, Orange, White)
//!   * Lighting Off
//!   * Show FantasTech (GUI)
//!   * OpenRGB Server toggle
//!   * Volume Knob Guard toggle
//!   * Exit FantasTech
//!
//! License: Apache-2.0

#[cfg(target_os = "linux")]
mod linux_tray {
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use fantastech_protocol::lighting::LightingMode;
use zbus::interface;
use zbus::zvariant::Value;

pub struct FantasTechTrayItem {
    pub app_path: PathBuf,
}

#[interface(name = "org.kde.StatusNotifierItem")]
impl FantasTechTrayItem {
    #[zbus(property)]
    fn category(&self) -> String {
        "Hardware".to_string()
    }

    #[zbus(property)]
    fn id(&self) -> String {
        "org.fantastech.FantasTech".to_string()
    }

    #[zbus(property)]
    fn title(&self) -> String {
        "FantasTech".to_string()
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "Active".to_string()
    }

    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }

    #[zbus(property)]
    fn icon_name(&self) -> String {
        "org.fantastech.FantasTech".to_string()
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    fn overlay_icon_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    fn attention_icon_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    fn attention_movie_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    #[allow(clippy::type_complexity)]
    fn tool_tip(&self) -> (String, Vec<(i32, i32, Vec<u8>)>, String, String) {
        (
            "org.fantastech.FantasTech".to_string(),
            Vec::new(),
            "FantasTech".to_string(),
            "Fantech MK894 Control Center".to_string(),
        )
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn menu(&self) -> zbus::zvariant::OwnedObjectPath {
        zbus::zvariant::ObjectPath::try_from("/MenuBar")
            .unwrap()
            .into()
    }

    fn activate(&self, _x: i32, _y: i32) {
        // Left click: focus existing window or launch if not running
        if !fantastech_core::focus_existing_gui() {
            let _ = Command::new(&self.app_path).arg("gui").spawn();
        }
    }

    fn secondary_activate(&self, _x: i32, _y: i32) {
        // Middle click: focus existing window or launch if not running
        if !fantastech_core::focus_existing_gui() {
            let _ = Command::new(&self.app_path).arg("gui").spawn();
        }
    }

    fn context_menu(&self, _x: i32, _y: i32) {
        // Fallback for desktop hosts without DBusMenu: focus window, never spawn duplicate
        if !fantastech_core::focus_existing_gui() {
            let _ = Command::new(&self.app_path).arg("gui").spawn();
        }
    }

    fn scroll(&self, _delta: i32, _orientation: String) {}
}

pub struct FantasTechDbusMenu {
    pub app_path: PathBuf,
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
            let _ = dev.set_lighting(&profile.to_lighting_config());
        }
    }
}

fn set_direct_color_action(r: u8, g: u8, b: u8) {
    if fantastech_core::is_service_running() {
        let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::SetDirectColor { r, g, b });
        return;
    }
    if let Ok(transport) = fantastech_transport::HidrawTransport::discover() {
        let mut dev = fantastech_device::Device::new_read_only(transport).into_reversible();
        let config = fantastech_protocol::lighting::LightingConfig {
            zone: fantastech_protocol::lighting::LightingZone::MainBacklight,
            mode: fantastech_protocol::lighting::LightingMode::Static,
            brightness: 4,
            speed: 2,
            direction: fantastech_protocol::lighting::AnimationDirection::LeftToRight,
            color_mode: fantastech_protocol::lighting::ColorMode::FixedColor,
            color: fantastech_protocol::lighting::RgbColor::new(r, g, b),
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
        let config = fantastech_protocol::lighting::LightingConfig {
            zone: fantastech_protocol::lighting::LightingZone::MainBacklight,
            mode: fantastech_protocol::lighting::LightingMode::Off,
            brightness: 0,
            speed: 0,
            direction: fantastech_protocol::lighting::AnimationDirection::LeftToRight,
            color_mode: fantastech_protocol::lighting::ColorMode::FixedColor,
            color: fantastech_protocol::lighting::RgbColor::new(0, 0, 0),
        };
        let _ = dev.set_lighting(&config);
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

fn toggle_volume_guard_action() {
    let mut cfg = fantastech_core::FantasTechConfig::load();
    cfg.guard_volume_enabled = !cfg.guard_volume_enabled;
    let _ = cfg.save();
    if fantastech_core::is_service_running() {
        let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::SetVolumeGuardEnabled(
            cfg.guard_volume_enabled,
        ));
    }
}

fn apply_lighting_mode_action(mode: LightingMode) {
    if fantastech_core::is_service_running() {
        let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::SetLightingMode {
            mode,
            brightness: 4,
            speed: 2,
            direction: 1,
            color_mode: if mode.supports_color() {
                fantastech_protocol::lighting::ColorMode::FixedColor
            } else {
                fantastech_protocol::lighting::ColorMode::DynamicSpectrum
            },
            color: [0, 113, 207],
        });
        return;
    }
    if let Ok(transport) = fantastech_transport::HidrawTransport::discover() {
        let mut dev = fantastech_device::Device::new_read_only(transport).into_reversible();
        let config = fantastech_protocol::lighting::LightingConfig {
            zone: fantastech_protocol::lighting::LightingZone::MainBacklight,
            mode,
            brightness: 4,
            speed: 2,
            direction: fantastech_protocol::lighting::AnimationDirection::LeftToRight,
            color_mode: if mode.supports_color() {
                fantastech_protocol::lighting::ColorMode::FixedColor
            } else {
                fantastech_protocol::lighting::ColorMode::DynamicSpectrum
            },
            color: fantastech_protocol::lighting::RgbColor::new(0, 113, 207),
        };
        let _ = dev.set_lighting(&config);
    }
}

fn toggle_service_action(app_path: &Path) {
    if fantastech_core::is_service_running() {
        let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::ShutdownService);
        #[cfg(target_os = "linux")]
        {
            let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
            if home.join(".config/systemd/user/fantastech.service").exists() {
                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "stop", "fantastech.service"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
    } else {
        let _ = Command::new(app_path).args(["service", "start"]).spawn();
    }
}

fn item_properties(id: i32) -> HashMap<String, Value<'static>> {
    let mut props = HashMap::new();
    match id {
        10 => {
            props.insert("label".to_string(), Value::from("FantasTech — MK894"));
            props.insert("enabled".to_string(), Value::from(false));
            props.insert("visible".to_string(), Value::from(true));
        }
        11 | 12 | 41 | 80 => {
            props.insert("type".to_string(), Value::from("separator"));
        }
        50 => {
            props.insert("label".to_string(), Value::from("🖥️ Show FantasTech"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        20 => {
            props.insert("label".to_string(), Value::from("💡 Lighting Profiles"));
            props.insert("children-display".to_string(), Value::from("submenu"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        21 => {
            props.insert("label".to_string(), Value::from("Gaming High-Contrast"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        22 => {
            props.insert("label".to_string(), Value::from("Cyberpunk Neon"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        23 => {
            props.insert("label".to_string(), Value::from("Rainbow Wave"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        24 => {
            props.insert("label".to_string(), Value::from("Neon Sunset"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        25 => {
            props.insert("label".to_string(), Value::from("Matrix Pulse"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        26 => {
            props.insert("label".to_string(), Value::from("Static Purple"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        28 => {
            props.insert("label".to_string(), Value::from("✨ Lighting Effects"));
            props.insert("children-display".to_string(), Value::from("submenu"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        101 => {
            props.insert("label".to_string(), Value::from("Static Solid"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        102 => {
            props.insert("label".to_string(), Value::from("Breathing"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        103 => {
            props.insert("label".to_string(), Value::from("Rainbow Wave"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        104 => {
            props.insert("label".to_string(), Value::from("Spectrum Cycle"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        105 => {
            props.insert("label".to_string(), Value::from("Radial Bloom"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        106 => {
            props.insert("label".to_string(), Value::from("Cascade Stream"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        107 => {
            props.insert("label".to_string(), Value::from("Shuttle"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        108 => {
            props.insert("label".to_string(), Value::from("Raindrop"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        109 => {
            props.insert("label".to_string(), Value::from("Meteor"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        110 => {
            props.insert("label".to_string(), Value::from("Ripple Fade"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        111 => {
            props.insert("label".to_string(), Value::from("Ripple Static"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        112 => {
            props.insert("label".to_string(), Value::from("Reactive Glow"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        113 => {
            props.insert("label".to_string(), Value::from("Grid Ripple"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        114 => {
            props.insert("label".to_string(), Value::from("Tide Wave"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        115 => {
            props.insert("label".to_string(), Value::from("Rotate Cycle"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        30 => {
            props.insert("label".to_string(), Value::from("🎨 Quick Colors"));
            props.insert("children-display".to_string(), Value::from("submenu"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        31 => {
            props.insert("label".to_string(), Value::from("Electric Blue"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        32 => {
            props.insert("label".to_string(), Value::from("Red"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        33 => {
            props.insert("label".to_string(), Value::from("Green"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        34 => {
            props.insert("label".to_string(), Value::from("Blue"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        35 => {
            props.insert("label".to_string(), Value::from("Cyan"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        36 => {
            props.insert("label".to_string(), Value::from("Magenta"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        37 => {
            props.insert("label".to_string(), Value::from("Yellow"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        38 => {
            props.insert("label".to_string(), Value::from("Orange"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        39 => {
            props.insert("label".to_string(), Value::from("White"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        40 => {
            props.insert("label".to_string(), Value::from("🌑 Lighting Off"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        55 => {
            let update_available = if fantastech_core::is_service_running() {
                if let Ok(fantastech_core::IpcResponse::Status(st)) =
                    fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::GetStatus)
                {
                    st.firmware_update_available
                } else {
                    false
                }
            } else if let Ok(transport) = fantastech_transport::HidrawTransport::discover() {
                let mut dev = fantastech_device::Device::new_read_only(transport);
                if let Ok(info) = dev.query_info() {
                    fantastech_core::check_firmware_update_available(&info.firmware_version)
                } else {
                    false
                }
            } else {
                false
            };
            let label = if update_available {
                "🔔 Firmware Update Available"
            } else {
                "✓ Firmware: Up to date"
            };
            props.insert("label".to_string(), Value::from(label));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        60 => {
            let cfg = fantastech_core::FantasTechConfig::load();
            let label = if cfg.openrgb_enabled {
                format!("🌐 OpenRGB: Port {} (Active)", cfg.openrgb_port)
            } else {
                "🌐 OpenRGB: Disabled".to_string()
            };
            props.insert("label".to_string(), Value::from(label));
            props.insert("toggle-type".to_string(), Value::from("checkmark"));
            props.insert("toggle-state".to_string(), Value::from(if cfg.openrgb_enabled { 1 } else { 0 }));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        70 => {
            let cfg = fantastech_core::FantasTechConfig::load();
            let label = if cfg.guard_volume_enabled {
                "🛡️ Volume Guard: Active".to_string()
            } else {
                "🛡️ Volume Guard: Disabled".to_string()
            };
            props.insert("label".to_string(), Value::from(label));
            props.insert("toggle-type".to_string(), Value::from("checkmark"));
            props.insert("toggle-state".to_string(), Value::from(if cfg.guard_volume_enabled { 1 } else { 0 }));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        75 => {
            let running = fantastech_core::is_service_running();
            let label = if running {
                "⚙️ Service: Running"
            } else {
                "⚙️ Service: Stopped"
            };
            props.insert("label".to_string(), Value::from(label));
            props.insert("toggle-type".to_string(), Value::from("checkmark"));
            props.insert("toggle-state".to_string(), Value::from(if running { 1 } else { 0 }));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        90 => {
            props.insert("label".to_string(), Value::from("❌ Exit FantasTech"));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        _ => {}
    }
    props
}

#[interface(name = "com.canonical.dbusmenu")]
impl FantasTechDbusMenu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> String {
        "ltr".to_string()
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "normal".to_string()
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        true
    }

    #[allow(clippy::type_complexity)]
    fn get_layout(
        &self,
        parent_id: i32,
        _recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> (u32, (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>)) {
        if parent_id == 20 {
            // Lighting profiles submenu
            let mut children = Vec::new();
            for id in [21, 22, 23, 24, 25, 26] {
                let node: (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>) =
                    (id, item_properties(id), Vec::new());
                children.push(Value::from(node));
            }
            return (1, (20, item_properties(20), children));
        }

        if parent_id == 28 {
            // Lighting effects submenu
            let mut children = Vec::new();
            for id in 101..=115 {
                let node: (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>) =
                    (id, item_properties(id), Vec::new());
                children.push(Value::from(node));
            }
            return (1, (28, item_properties(28), children));
        }

        if parent_id == 30 {
            // Quick colors submenu
            let mut children = Vec::new();
            for id in [31, 32, 33, 34, 35, 36, 37, 38, 39] {
                let node: (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>) =
                    (id, item_properties(id), Vec::new());
                children.push(Value::from(node));
            }
            return (1, (30, item_properties(30), children));
        }

        if parent_id != 0 {
            return (1, (parent_id, HashMap::new(), Vec::new()));
        }

        // Build profiles submenu node
        let mut profiles_children = Vec::new();
        for id in [21, 22, 23, 24, 25, 26] {
            let node: (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>) =
                (id, item_properties(id), Vec::new());
            profiles_children.push(Value::from(node));
        }
        let profiles_node: (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>) =
            (20, item_properties(20), profiles_children);

        // Build effects submenu node
        let mut effects_children = Vec::new();
        for id in 101..=115 {
            let node: (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>) =
                (id, item_properties(id), Vec::new());
            effects_children.push(Value::from(node));
        }
        let effects_node: (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>) =
            (28, item_properties(28), effects_children);

        // Build quick colors submenu node
        let mut colors_children = Vec::new();
        for id in [31, 32, 33, 34, 35, 36, 37, 38, 39] {
            let node: (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>) =
                (id, item_properties(id), Vec::new());
            colors_children.push(Value::from(node));
        }
        let colors_node: (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>) =
            (30, item_properties(30), colors_children);

        // Root children:
        // 10 (title), 11 (sep), 50 (Show FantasTech), 12 (sep),
        // profiles_node (20), effects_node (28), colors_node (30),
        // 40 (off), 41 (sep), 55 (update status), 60 (openrgb), 70 (volume guard), 75 (service status), 80 (sep), 90 (exit)
        let top_ids = [10, 11, 50, 12];
        let mut children = Vec::new();
        for &id in &top_ids {
            let node: (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>) =
                (id, item_properties(id), Vec::new());
            children.push(Value::from(node));
        }
        children.push(Value::from(profiles_node));
        children.push(Value::from(effects_node));
        children.push(Value::from(colors_node));

        let rest_ids = [40, 41, 55, 60, 70, 75, 80, 90];
        for &id in &rest_ids {
            let node: (i32, HashMap<String, Value<'_>>, Vec<Value<'_>>) =
                (id, item_properties(id), Vec::new());
            children.push(Value::from(node));
        }

        (1, (0, HashMap::new(), children))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, Value<'_>>)> {
        let mut result = Vec::new();
        for id in ids {
            result.push((id, item_properties(id)));
        }
        result
    }

    fn get_property(&self, id: i32, name: String) -> Value<'_> {
        let props = item_properties(id);
        if let Some(val) = props.get(&name) {
            val.clone()
        } else {
            Value::from("")
        }
    }

    fn event(&self, id: i32, event_id: String, _data: Value, _timestamp: u32) {
        if event_id != "clicked" {
            return;
        }

        match id {
            // Show FantasTech (GUI)
            50 => {
                if !fantastech_core::focus_existing_gui() {
                    let _ = Command::new(&self.app_path).arg("gui").spawn();
                }
            }

            // Lighting profiles
            21 => apply_lighting_profile_action("gaming"),
            22 => apply_lighting_profile_action("cyberpunk"),
            23 => apply_lighting_profile_action("rainbow_wave"),
            24 => apply_lighting_profile_action("neon_sunset"),
            25 => apply_lighting_profile_action("matrix"),
            26 => apply_lighting_profile_action("static_purple"),

            // Lighting effects
            101 => apply_lighting_mode_action(LightingMode::Static),
            102 => apply_lighting_mode_action(LightingMode::Breathing),
            103 => apply_lighting_mode_action(LightingMode::StreamLeftToRight),
            104 => apply_lighting_mode_action(LightingMode::Spectrum),
            105 => apply_lighting_mode_action(LightingMode::Bloom),
            106 => apply_lighting_mode_action(LightingMode::StreamUpToDown),
            107 => apply_lighting_mode_action(LightingMode::Shuttle),
            108 => apply_lighting_mode_action(LightingMode::Raindrop),
            109 => apply_lighting_mode_action(LightingMode::Meteor),
            110 => apply_lighting_mode_action(LightingMode::RippleFade),
            111 => apply_lighting_mode_action(LightingMode::RippleStatic),
            112 => apply_lighting_mode_action(LightingMode::Reactive),
            113 => apply_lighting_mode_action(LightingMode::GridRipple),
            114 => apply_lighting_mode_action(LightingMode::Tide),
            115 => apply_lighting_mode_action(LightingMode::Rotate),

            // Quick colors
            31 => set_direct_color_action(0, 113, 207),    // Electric Blue
            32 => set_direct_color_action(255, 0, 0),      // Red
            33 => set_direct_color_action(0, 255, 0),      // Green
            34 => set_direct_color_action(0, 0, 255),      // Blue
            35 => set_direct_color_action(0, 255, 255),    // Cyan
            36 => set_direct_color_action(255, 0, 255),    // Magenta
            37 => set_direct_color_action(255, 255, 0),    // Yellow
            38 => set_direct_color_action(255, 128, 0),    // Orange
            39 => set_direct_color_action(255, 255, 255),  // White

            // Lighting off
            40 => turn_lighting_off_action(),

            // Firmware update status clicked -> open GUI on update section
            55 => {
                if !fantastech_core::focus_existing_gui_with_action("UPDATE") {
                    let _ = Command::new(&self.app_path).args(["gui", "--update"]).spawn();
                }
            }

            // OpenRGB server toggle
            60 => toggle_openrgb_action(),

            // Volume guard toggle
            70 => toggle_volume_guard_action(),

            // Service status toggle
            75 => toggle_service_action(&self.app_path),

            // Exit FantasTech
            90 => {
                fantastech_core::focus_existing_gui_with_action("EXIT");
                let _ = fantastech_core::send_ipc_request(&fantastech_core::IpcRequest::ShutdownService);
                #[cfg(target_os = "linux")]
                {
                    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
                    if home.join(".config/systemd/user/fantastech.service").exists() {
                        let _ = std::process::Command::new("systemctl")
                            .args(["--user", "stop", "fantastech.service"])
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                    }
                }
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

/// Runs the native system tray StatusNotifierItem loop
pub fn run_tray_loop() -> Result<(), Box<dyn std::error::Error>> {
    // Strictly enforce single tray instance across the entire user session
    if fantastech_core::is_tray_already_running() {
        println!("ℹ️ FantasTech system tray is already active.");
        return Ok(());
    }

    let _tray_guard = match fantastech_core::TrayInstanceGuard::bind() {
        Some(guard) => guard,
        None => {
            println!("ℹ️ FantasTech system tray is already active (lock held).");
            return Ok(());
        }
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech"));
        let item = FantasTechTrayItem {
            app_path: current_exe.clone(),
        };
        let menu = FantasTechDbusMenu {
            app_path: current_exe,
        };

        let pid = std::process::id();
        let service_name = format!("org.kde.StatusNotifierItem-{}-1", pid);
        let conn = zbus::connection::Builder::session()?
            .serve_at("/StatusNotifierItem", item)?
            .serve_at("/MenuBar", menu)?
            .build()
            .await?;

        // Request well-known name if allowed by environment, but continue if forbidden
        let _ = conn.request_name(service_name).await;

        // Register with StatusNotifierWatcher on desktop
        let watcher_proxy = zbus::Proxy::new(
            &conn,
            "org.kde.StatusNotifierWatcher",
            "/StatusNotifierWatcher",
            "org.kde.StatusNotifierWatcher",
        )
        .await;

        if let Ok(proxy) = watcher_proxy {
            let _ = proxy
                .call::<_, (&str,), ()>("RegisterStatusNotifierItem", &("/StatusNotifierItem",))
                .await;
        }

        println!("✅ Native FantasTech System Tray (StatusNotifierItem) active with full quick-control menu.");

        // Keep the event loop running
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            if std::env::var("FLATPAK_ID").is_ok() && !std::path::Path::new("/app/bin/fantastech").exists() {
                std::process::exit(0);
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

/// Spawns the native tray in a detached background OS process if not already running
pub fn spawn_native_tray() {
    if fantastech_core::is_tray_already_running() {
        return;
    }

    let bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fantastech"));
    let mut cmd = Command::new(bin);
    cmd.arg("tray");
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let _ = cmd.spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dbusmenu_full_layout() {
        let menu = FantasTechDbusMenu {
            app_path: PathBuf::from("fantastech"),
        };
        let (rev, (parent, _props, children)) = menu.get_layout(0, 1, Vec::new());
        assert_eq!(rev, 1);
        assert_eq!(parent, 0);
        // Root children: 10 (title), 11 (sep), 50 (show), 12 (sep), 20 (profiles),
        // 28 (effects), 30 (colors), 40 (off), 41 (sep), 55 (update), 60 (openrgb),
        // 70 (volume guard), 75 (service status), 80 (sep), 90 (exit)
        assert_eq!(children.len(), 15);
    }

    #[test]
    fn test_dbusmenu_submenus() {
        let menu = FantasTechDbusMenu {
            app_path: PathBuf::from("fantastech"),
        };

        // Submenu 20: Lighting Profiles
        let (_rev, (parent, _props, children)) = menu.get_layout(20, 1, Vec::new());
        assert_eq!(parent, 20);
        assert_eq!(children.len(), 6);

        // Submenu 28: Lighting Effects
        let (_rev, (parent, _props, children)) = menu.get_layout(28, 1, Vec::new());
        assert_eq!(parent, 28);
        assert_eq!(children.len(), 15);

        // Submenu 30: Quick Colors
        let (_rev, (parent, _props, children)) = menu.get_layout(30, 1, Vec::new());
        assert_eq!(parent, 30);
        assert_eq!(children.len(), 9);
    }

    #[test]
    fn test_dbusmenu_properties() {
        let props10 = item_properties(10);
        assert_eq!(props10.get("label").unwrap().to_string(), "\"FantasTech — MK894\"");

        let props50 = item_properties(50);
        let label50 = props50.get("label").unwrap().to_string();
        assert!(label50.contains("Show FantasTech"));
        assert!(!label50.contains("GUI"));

        let props20 = item_properties(20);
        assert_eq!(props20.get("children-display").unwrap().to_string(), "\"submenu\"");

        let props28 = item_properties(28);
        assert_eq!(props28.get("children-display").unwrap().to_string(), "\"submenu\"");

        let props40 = item_properties(40);
        assert!(props40.get("label").unwrap().to_string().contains("Lighting Off"));

        let props55 = item_properties(55);
        assert!(props55.get("label").unwrap().to_string().contains("Firmware"));

        let props75 = item_properties(75);
        assert!(props75.get("label").unwrap().to_string().contains("Service:"));

        let props90 = item_properties(90);
        assert!(props90.get("label").unwrap().to_string().contains("Exit FantasTech"));
    }
}
}

#[cfg(target_os = "linux")]
pub use linux_tray::*;

#[cfg(target_os = "windows")]
#[path = "windows_tray.rs"]
mod windows_tray;
#[cfg(target_os = "windows")]
pub use windows_tray::*;

#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
pub fn spawn_native_tray() {}

#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
pub fn run_tray_loop() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("ℹ️ The native system tray applet is not supported on this platform.");
    Ok(())
}
