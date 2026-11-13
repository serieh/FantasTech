//! Global configuration management for FantasTech suite.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Resolves standard FantasTech configuration directory across platforms:
/// - Windows: `%APPDATA%\FantasTech`
/// - Linux: `$XDG_CONFIG_HOME/fantastech` (or `~/.config/fantastech`)
pub fn get_fantastech_config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.trim().is_empty() {
                return PathBuf::from(appdata.trim()).join("FantasTech");
            }
        }
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            if !userprofile.trim().is_empty() {
                return PathBuf::from(userprofile.trim())
                    .join("AppData")
                    .join("Roaming")
                    .join("FantasTech");
            }
        }
    }

    #[cfg(unix)]
    {
        if let Ok(sudo_user) = std::env::var("SUDO_USER") {
            if !sudo_user.trim().is_empty() && sudo_user != "root" {
                let sudo_home = PathBuf::from(format!("/home/{}", sudo_user.trim()));
                if sudo_home.exists() {
                    return sudo_home.join(".config").join("fantastech");
                }
            }
        }
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.trim().is_empty() {
                return PathBuf::from(xdg.trim()).join("fantastech");
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.trim().is_empty() && home != "/root" {
                return PathBuf::from(home.trim()).join(".config").join("fantastech");
            }
        }
    }

    PathBuf::from(".config").join("fantastech")
}

/// GUI theme selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GuiTheme {
    /// Modern clean light theme (Default).
    #[default]
    Light,
    /// Sleek neon dark theme.
    Dark,
}

/// Color code display format selection for color picker and lighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColorDisplayFormat {
    /// Hexadecimal color format (e.g. #FF0055).
    #[default]
    Hex,
    /// Standard RGB values (e.g. rgb(255, 0, 85)).
    Rgb,
    /// Hue, Saturation, Value (e.g. hsv(340°, 100%, 100%)).
    Hsv,
    /// Hue, Saturation, Lightness (e.g. hsl(340°, 100%, 50%)).
    Hsl,
}

/// Global FantasTech configuration stored at `~/.config/fantastech/config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FantasTechConfig {
    /// Active lighting profile identifier (default: "gaming").
    pub active_lighting_profile: String,
    /// Active button remapping profile identifier (default: "default").
    pub active_button_profile: String,
    /// Active macro profile identifier (default: "default").
    pub active_macro_profile: String,
    /// Active settings profile identifier (default: "default").
    pub active_settings_profile: String,
    /// Auto-apply active lighting profile when service starts (if OpenRGB is off).
    pub auto_apply_lighting: bool,
    /// Whether OpenRGB SDK server is enabled.
    #[serde(default = "default_true")]
    pub openrgb_enabled: bool,
    /// Host IP address to bind OpenRGB SDK server (default: "127.0.0.1").
    pub openrgb_host: String,
    /// Port for OpenRGB SDK server (default: 6742).
    pub openrgb_port: u16,
    /// Whether volume knob guard against runaway volume is enabled.
    pub guard_volume_enabled: bool,
    /// Whether system tray icon is enabled.
    pub tray_icon_enabled: bool,
    /// Whether background service should auto-start on user login.
    pub auto_start_enabled: bool,
    /// GUI appearance theme (Light or Dark).
    pub gui_theme: GuiTheme,
    /// Color code format in GUI (Hex, RGB, HSV, HSL).
    #[serde(default)]
    pub color_display_format: ColorDisplayFormat,
    /// Automatically check for firmware updates on startup (Default: true).
    #[serde(default = "default_true")]
    pub auto_check_updates: bool,
    /// Automatically install firmware updates without manual intervention (Default: false).
    #[serde(default)]
    pub auto_update_firmware: bool,
    /// Show desktop notifications when a new firmware update is available (Default: false).
    #[serde(default)]
    pub update_notifications_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for FantasTechConfig {
    fn default() -> Self {
        Self {
            active_lighting_profile: "gaming".to_string(),
            active_button_profile: "default".to_string(),
            active_macro_profile: "default".to_string(),
            active_settings_profile: "default".to_string(),
            auto_apply_lighting: false,
            openrgb_enabled: true,
            openrgb_host: "127.0.0.1".to_string(),
            openrgb_port: 6742,
            guard_volume_enabled: true,
            tray_icon_enabled: true,
            auto_start_enabled: true,
            gui_theme: GuiTheme::Light,
            color_display_format: ColorDisplayFormat::Hex,
            auto_check_updates: true,
            auto_update_firmware: false,
            update_notifications_enabled: false,
        }
    }
}

impl FantasTechConfig {
    /// Returns default path for `config.json`.
    pub fn config_path() -> PathBuf {
        get_fantastech_config_dir().join("config.json")
    }

    /// Loads configuration from disk, creating default if not found.
    pub fn load() -> Self {
        Self::load_from_path(&Self::config_path())
    }

    /// Loads configuration from a specific file path, backing up corrupt files before restoring default.
    pub fn load_from_path(path: &Path) -> Self {
        if let Ok(content) = fs::read_to_string(path) {
            match serde_json::from_str::<Self>(&content) {
                Ok(config) => return config,
                Err(err) => {
                    let bak_path = path.with_extension("corrupt.bak");
                    let _ = fs::copy(path, &bak_path);
                    eprintln!("⚠️ Corrupted config detected at {}: {}. Backed up to {} and restored defaults.", path.display(), err, bak_path.display());
                }
            }
        }
        let default_config = Self::default();
        let _ = default_config.save_to_path(path);
        default_config
    }

    /// Saves configuration to the standard path.
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to_path(&Self::config_path())
    }

    /// Saves configuration to a specific path.
    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        crate::profiles::write_user_owned_file(path, json.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_values() {
        let config = FantasTechConfig::default();
        assert_eq!(config.active_lighting_profile, "gaming");
        assert!(!config.auto_apply_lighting);
        assert!(config.openrgb_enabled);
        assert_eq!(config.openrgb_host, "127.0.0.1");
        assert_eq!(config.openrgb_port, 6742);
        assert!(config.guard_volume_enabled);
        assert!(config.tray_icon_enabled);
        assert!(config.auto_start_enabled);
        assert_eq!(config.gui_theme, GuiTheme::Light);
        assert!(config.auto_check_updates);
        assert!(!config.auto_update_firmware);
        assert!(!config.update_notifications_enabled);
    }

    #[test]
    fn test_config_backward_compatibility_missing_openrgb_field() {
        let legacy_json = r#"{
            "active_lighting_profile": "gaming",
            "active_button_profile": "default",
            "active_macro_profile": "default",
            "active_settings_profile": "default",
            "auto_apply_lighting": true,
            "openrgb_host": "127.0.0.1",
            "openrgb_port": 6742,
            "guard_volume_enabled": true,
            "tray_icon_enabled": true,
            "auto_start_enabled": true,
            "gui_theme": "Light"
        }"#;
        let parsed: FantasTechConfig = serde_json::from_str(legacy_json).unwrap();
        assert!(parsed.openrgb_enabled);
    }

    #[test]
    fn test_config_backward_compatibility_missing_update_fields() {
        // Legacy JSON without the update fields
        let legacy_json = r#"{
            "active_lighting_profile": "gaming",
            "active_button_profile": "default",
            "active_macro_profile": "default",
            "active_settings_profile": "default",
            "auto_apply_lighting": true,
            "openrgb_enabled": false,
            "openrgb_host": "127.0.0.1",
            "openrgb_port": 6742,
            "guard_volume_enabled": true,
            "tray_icon_enabled": true,
            "auto_start_enabled": true,
            "gui_theme": "Light"
        }"#;
        let parsed: FantasTechConfig = serde_json::from_str(legacy_json).unwrap();
        assert!(parsed.auto_check_updates);
        assert!(!parsed.auto_update_firmware);
        assert!(!parsed.update_notifications_enabled);
    }

    #[test]
    fn test_config_json_roundtrip() {
        let config = FantasTechConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: FantasTechConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_corrupt_config_recovery_and_backup() {
        let temp_dir = std::env::temp_dir().join(format!("fantastech_test_corrupt_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let cfg_path = temp_dir.join("config.json");
        let bak_path = temp_dir.join("config.corrupt.bak");

        // Write intentionally malformed JSON
        fs::write(&cfg_path, b"{\"active_lighting_profile\": INVALID_JSON_DATA").unwrap();

        // Loading should recover to default and create the .corrupt.bak backup
        let loaded = FantasTechConfig::load_from_path(&cfg_path);
        assert_eq!(loaded, FantasTechConfig::default());
        assert!(bak_path.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
