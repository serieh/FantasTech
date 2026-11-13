//! Multi-domain profile manager for FantasTech.
//!
//! Provides independent, unlimited profile storage and management for:
//! - Lighting profiles (effects, speeds, brightness, and direct canvas colors)
//! - Button profiles (base layer and Fn layer key mappings)
//! - Macro profiles (hardware macro sequences)
//! - Settings profiles (hardware sleep timeouts, etc.)

#![forbid(unsafe_code)]

use crate::config::get_fantastech_config_dir;
use crate::{LayerKeymapConfig, MacroSequence, ThemeConfig};
use fantastech_protocol::constants::SPARSE_KEY_COUNT;
use fantastech_protocol::keymap::KeyLayer;
use fantastech_protocol::lighting::{
    AnimationDirection, ColorMode, LightingConfig, LightingMode, LightingZone, RgbColor,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Lighting profile containing complete illumination parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightingProfile {
    /// Unique profile identifier (e.g. "gaming", "cyberpunk").
    pub id: String,
    /// User-visible profile name.
    pub name: String,
    /// Profile description.
    pub description: String,
    /// Animation mode.
    pub mode: LightingMode,
    /// Animation speed (0..100).
    pub speed: u8,
    /// Backlight brightness (0..100).
    pub brightness: u8,
    /// Animation direction (0 = left/up, 1 = right/down).
    pub direction: u8,
    /// Global or primary color [R, G, B].
    pub color: [u8; 3],
    /// Color mode (Dynamic Spectrum vs Fixed Color).
    #[serde(default)]
    pub color_mode: Option<ColorMode>,
    /// Per-key custom matrix overrides (matrix_index -> [R, G, B]).
    #[serde(default)]
    pub canvas: HashMap<usize, [u8; 3]>,
    /// Whether this is a bundled default theme.
    pub is_builtin: bool,
}

impl LightingProfile {
    /// Converts this profile into a `LightingConfig` report payload.
    pub fn to_lighting_config(&self) -> LightingConfig {
        let dir = if self.direction == 1 {
            AnimationDirection::LeftToRight
        } else {
            AnimationDirection::Default
        };
        let c_mode = self.color_mode.unwrap_or_else(|| {
            if self.mode == LightingMode::Spectrum {
                ColorMode::DynamicSpectrum
            } else {
                ColorMode::FixedColor
            }
        });
        let brightness = if self.brightness <= 4 {
            self.brightness
        } else {
            ((self.brightness as f32 / 100.0) * 4.0).round().clamp(0.0, 4.0) as u8
        };
        let speed = if self.speed <= 4 {
            self.speed
        } else {
            ((self.speed as f32 / 100.0) * 4.0).round().clamp(0.0, 4.0) as u8
        };
        LightingConfig {
            zone: LightingZone::MainBacklight,
            mode: self.mode,
            brightness,
            speed,
            direction: dir,
            color_mode: c_mode,
            color: RgbColor::new(self.color[0], self.color[1], self.color[2]),
        }
    }

    /// Converts this profile's canvas and base color into the full 126-key RGB sparse matrix.
    pub fn to_matrix(&self) -> [RgbColor; SPARSE_KEY_COUNT] {
        let default_rgb = RgbColor::new(self.color[0], self.color[1], self.color[2]);
        let mut matrix = [default_rgb; SPARSE_KEY_COUNT];
        for (&idx, &rgb) in &self.canvas {
            if idx < SPARSE_KEY_COUNT {
                matrix[idx] = RgbColor::new(rgb[0], rgb[1], rgb[2]);
            }
        }
        matrix
    }

    // ================= Bundled Preset Profiles ================= //

    /// Gaming preset: Solid royal blue with solid red WASD, yellow arrows, green space/enter, white escape.
    pub fn preset_gaming() -> Self {
        let theme = ThemeConfig::gaming();
        Self {
            id: "gaming".to_string(),
            name: "Gaming High-Contrast".to_string(),
            description: "Solid royal blue background with neon red WASD and yellow arrows".to_string(),
            mode: LightingMode::CustomMatrixCanvas,
            speed: 50,
            brightness: 100,
            direction: 0,
            color: theme.default_color,
            color_mode: Some(ColorMode::FixedColor),
            canvas: theme.key_overrides,
            is_builtin: true,
        }
    }

    /// Cyberpunk preset: Deep magenta with yellow WASD, cyan arrows/space, neon green accents.
    pub fn preset_cyberpunk() -> Self {
        let theme = ThemeConfig::cyberpunk();
        Self {
            id: "cyberpunk".to_string(),
            name: "Cyberpunk Neon".to_string(),
            description: "Vibrant neon magenta canvas with electric yellow WASD and cyan arrows".to_string(),
            mode: LightingMode::CustomMatrixCanvas,
            speed: 50,
            brightness: 100,
            direction: 0,
            color: theme.default_color,
            color_mode: Some(ColorMode::FixedColor),
            canvas: theme.key_overrides,
            is_builtin: true,
        }
    }

    /// Rainbow wave hardware animation preset.
    pub fn preset_rainbow_wave() -> Self {
        Self {
            id: "rainbow_wave".to_string(),
            name: "Rainbow Wave".to_string(),
            description: "Dynamic multi-color rainbow wave flow across the entire keyboard".to_string(),
            mode: LightingMode::StreamLeftToRight,
            speed: 75,
            brightness: 100,
            direction: 0,
            color: [255, 0, 0],
            color_mode: Some(ColorMode::DynamicSpectrum),
            canvas: HashMap::new(),
            is_builtin: true,
        }
    }

    /// Neon sunset preset: Amber gold base with violet/magenta function and nav keys.
    pub fn preset_neon_sunset() -> Self {
        let mut canvas = HashMap::new();
        // Violet/Pink accents for Esc, F-row, arrows, nav keys
        let violet = [180, 0, 255];
        canvas.insert(0, violet); // Esc
        for f in 1..=15 {
            canvas.insert(f, violet); // F1-F12 + PrtSc/ScrLK/Pause
        }
        for k in &[35, 36, 37, 56, 57, 58] {
            canvas.insert(*k, violet); // Nav keys
        }
        for k in &[99, 119, 120, 121] {
            canvas.insert(*k, [255, 0, 128]); // Arrows hot pink
        }
        Self {
            id: "neon_sunset".to_string(),
            name: "Neon Sunset".to_string(),
            description: "Warm amber gold backlighting with vibrant violet and pink accents".to_string(),
            mode: LightingMode::CustomMatrixCanvas,
            speed: 50,
            brightness: 100,
            direction: 0,
            color: [255, 120, 0], // Amber gold
            color_mode: Some(ColorMode::FixedColor),
            canvas,
            is_builtin: true,
        }
    }

    /// Matrix pulse preset: Deep emerald green breathing.
    pub fn preset_matrix() -> Self {
        Self {
            id: "matrix".to_string(),
            name: "Matrix Pulse".to_string(),
            description: "Deep emerald green rhythmic breathing animation".to_string(),
            mode: LightingMode::Breathing,
            speed: 40,
            brightness: 100,
            direction: 0,
            color: [0, 255, 70],
            color_mode: Some(ColorMode::FixedColor),
            canvas: HashMap::new(),
            is_builtin: true,
        }
    }

    /// Static purple preset: Solid vivid purple backlighting.
    pub fn preset_static_purple() -> Self {
        Self {
            id: "static_purple".to_string(),
            name: "Static Purple".to_string(),
            description: "Uniform vivid purple backlighting across all keys".to_string(),
            mode: LightingMode::Static,
            speed: 50,
            brightness: 100,
            direction: 0,
            color: [160, 32, 240],
            color_mode: Some(ColorMode::FixedColor),
            canvas: HashMap::new(),
            is_builtin: true,
        }
    }

    /// All default built-in lighting presets.
    pub fn builtin_presets() -> Vec<Self> {
        vec![
            Self::preset_gaming(),
            Self::preset_cyberpunk(),
            Self::preset_rainbow_wave(),
            Self::preset_neon_sunset(),
            Self::preset_matrix(),
            Self::preset_static_purple(),
        ]
    }
}

/// Button / Keymap profile storing custom mappings for Base and Fn layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonProfile {
    /// Unique profile identifier (e.g. "default", "gaming_wasd").
    pub id: String,
    /// Profile display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Base typing layer overrides.
    pub base_layer: LayerKeymapConfig,
    /// Fn modifier layer overrides.
    pub fn_layer: LayerKeymapConfig,
    /// Built-in profile indicator.
    pub is_builtin: bool,
}

impl ButtonProfile {
    /// Factory default ANSI keymap profile.
    pub fn default_ansi() -> Self {
        Self {
            id: "default".to_string(),
            name: "Standard ANSI".to_string(),
            description: "Standard factory default ANSI keyboard mapping".to_string(),
            base_layer: LayerKeymapConfig::new_default(KeyLayer::Base),
            fn_layer: LayerKeymapConfig::new_default(KeyLayer::Fn),
            is_builtin: true,
        }
    }

    /// Gaming WASD keymap profile with WinLock on base Win key.
    pub fn gaming_profile() -> Self {
        let mut base = LayerKeymapConfig::new_default(KeyLayer::Base);
        base.overrides.insert("Win".to_string(), "custom:win-lock".to_string());
        Self {
            id: "gaming_lock".to_string(),
            name: "Gaming (Win-Lock)".to_string(),
            description: "Hardware Windows key locked to prevent accidental minimization".to_string(),
            base_layer: base,
            fn_layer: LayerKeymapConfig::new_default(KeyLayer::Fn),
            is_builtin: true,
        }
    }

    /// All default button profiles.
    pub fn builtin_presets() -> Vec<Self> {
        vec![Self::default_ansi(), Self::gaming_profile()]
    }
}

/// Macro profile storing a collection of hardware macro sequences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroProfile {
    /// Unique profile identifier (e.g. "default").
    pub id: String,
    /// Profile display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Macro sequences stored in this profile.
    pub macros: Vec<MacroSequence>,
    /// Built-in profile indicator.
    pub is_builtin: bool,
}

impl MacroProfile {
    /// Default empty macro profile.
    pub fn default_profile() -> Self {
        Self {
            id: "default".to_string(),
            name: "Default Macros".to_string(),
            description: "Default macro sequence collection".to_string(),
            macros: Vec::new(),
            is_builtin: true,
        }
    }

    /// All default macro profiles.
    pub fn builtin_presets() -> Vec<Self> {
        vec![Self::default_profile()]
    }
}

/// Settings profile storing hardware timeout and power behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsProfile {
    /// Unique profile identifier (e.g. "default", "never_sleep").
    pub id: String,
    /// Display name.
    pub name: String,
    /// Sleep timeout in minutes (0 = never sleep).
    pub sleep_timeout_mins: u16,
    /// Built-in profile indicator.
    pub is_builtin: bool,
}

impl SettingsProfile {
    /// Default settings (10 minute auto-sleep).
    pub fn default_settings() -> Self {
        Self {
            id: "default".to_string(),
            name: "Default (10m Sleep)".to_string(),
            sleep_timeout_mins: 10,
            is_builtin: true,
        }
    }

    /// Never sleep preset.
    pub fn never_sleep() -> Self {
        Self {
            id: "never_sleep".to_string(),
            name: "Always On (No Sleep)".to_string(),
            sleep_timeout_mins: 0,
            is_builtin: true,
        }
    }

    /// All default settings profiles.
    pub fn builtin_presets() -> Vec<Self> {
        vec![Self::default_settings(), Self::never_sleep()]
    }
}

/// Central profile storage manager coordinating disk operations for all domains.
#[derive(Debug, Clone)]
pub struct ProfileManager {
    root_dir: PathBuf,
}

/// Helper to write files atomically (via temporary file and atomic rename)
/// with safe user permissions (0600 / 0700) and chown to $SUDO_UID:$SUDO_GID when running under sudo.
pub fn write_user_owned_file(path: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            if let (Ok(uid_str), Ok(gid_str)) = (std::env::var("SUDO_UID"), std::env::var("SUDO_GID")) {
                if let (Ok(uid), Ok(gid)) = (uid_str.parse::<u32>(), gid_str.parse::<u32>()) {
                    let _ = std::os::unix::fs::chown(parent, Some(uid), Some(gid));
                }
            }
        }
    }

    // Atomic write pattern: write to .tmp file then atomic rename
    let pid = std::process::id();
    let tmp_path = if let Some(ext) = path.extension() {
        path.with_extension(format!("{}.tmp.{}", ext.to_string_lossy(), pid))
    } else {
        path.with_extension(format!("tmp.{}", pid))
    };

    fs::write(&tmp_path, content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
        if let (Ok(uid_str), Ok(gid_str)) = (std::env::var("SUDO_UID"), std::env::var("SUDO_GID")) {
            if let (Ok(uid), Ok(gid)) = (uid_str.parse::<u32>(), gid_str.parse::<u32>()) {
                let _ = std::os::unix::fs::chown(&tmp_path, Some(uid), Some(gid));
            }
        }
    }

    fs::rename(&tmp_path, path)?;

    Ok(())
}

impl ProfileManager {
    /// Creates a profile manager using standard `~/.config/fantastech/profiles/` or a custom path.
    pub fn new(custom_root: Option<PathBuf>) -> Self {
        let root = custom_root.unwrap_or_else(|| get_fantastech_config_dir().join("profiles"));
        let mgr = Self { root_dir: root };
        let _ = mgr.ensure_seed_profiles();
        mgr
    }

    /// Root directory path.
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn lighting_dir(&self) -> PathBuf {
        self.root_dir.join("lighting")
    }

    pub fn buttons_dir(&self) -> PathBuf {
        self.root_dir.join("buttons")
    }

    pub fn macros_dir(&self) -> PathBuf {
        self.root_dir.join("macros")
    }

    pub fn settings_dir(&self) -> PathBuf {
        self.root_dir.join("settings")
    }

    pub fn seed_marker_path(&self) -> PathBuf {
        self.root_dir.join(".seeded")
    }

    /// Ensures all directories exist and seeds bundled default profiles if empty.
    pub fn ensure_seed_profiles(&self) -> std::io::Result<()> {
        if fs::create_dir_all(self.lighting_dir()).is_err() {
            return Ok(()); // In read-only filesystems or sandboxes, fallback safely to built-ins
        }
        let _ = fs::create_dir_all(self.buttons_dir());
        let _ = fs::create_dir_all(self.macros_dir());
        let _ = fs::create_dir_all(self.settings_dir());

        if self.seed_marker_path().exists() {
            return Ok(());
        }

        for p in LightingProfile::builtin_presets() {
            let path = self.lighting_dir().join(format!("{}.json", p.id));
            if !path.exists() {
                let _ = self.save_lighting_profile(&p);
            }
        }

        for b in ButtonProfile::builtin_presets() {
            let path = self.buttons_dir().join(format!("{}.json", b.id));
            if !path.exists() {
                let _ = self.save_button_profile(&b);
            }
        }

        for m in MacroProfile::builtin_presets() {
            let path = self.macros_dir().join(format!("{}.json", m.id));
            if !path.exists() {
                let _ = self.save_macro_profile(&m);
            }
        }

        for s in SettingsProfile::builtin_presets() {
            let path = self.settings_dir().join(format!("{}.json", s.id));
            if !path.exists() {
                let _ = self.save_settings_profile(&s);
            }
        }

        let _ = fs::write(self.seed_marker_path(), b"1");

        Ok(())
    }

    // ================= Lighting Profiles ================= //

    pub fn list_lighting_profiles(&self) -> Vec<LightingProfile> {
        let mut map: HashMap<String, LightingProfile> = HashMap::new();
        let seeded = self.seed_marker_path().exists();
        if let Ok(entries) = fs::read_dir(self.lighting_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(profile) = serde_json::from_str::<LightingProfile>(&content) {
                            map.insert(profile.id.clone(), profile);
                        }
                    }
                }
            }
        }
        if map.is_empty() && !seeded {
            for p in LightingProfile::builtin_presets() {
                map.insert(p.id.clone(), p);
            }
        }
        let mut list: Vec<_> = map.into_values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn load_lighting_profile(&self, id: &str) -> Option<LightingProfile> {
        let path = self.lighting_dir().join(format!("{}.json", id));
        if let Ok(content) = fs::read_to_string(&path) {
            serde_json::from_str(&content).ok()
        } else if !self.seed_marker_path().exists() {
            // Check fallback to built-ins only if unseeded (e.g. read-only sandbox)
            LightingProfile::builtin_presets().into_iter().find(|p| p.id == id)
        } else {
            None
        }
    }

    pub fn save_lighting_profile(&self, profile: &LightingProfile) -> std::io::Result<()> {
        let path = self.lighting_dir().join(format!("{}.json", profile.id));
        let json = serde_json::to_string_pretty(profile)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_user_owned_file(&path, json.as_bytes())
    }

    pub fn delete_lighting_profile(&self, id: &str) -> std::io::Result<bool> {
        let _ = fs::write(self.seed_marker_path(), b"1");
        let path = self.lighting_dir().join(format!("{}.json", id));
        if path.exists() {
            fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ================= Button Profiles ================= //

    pub fn list_button_profiles(&self) -> Vec<ButtonProfile> {
        let mut map: HashMap<String, ButtonProfile> = HashMap::new();
        let seeded = self.seed_marker_path().exists();
        if let Ok(entries) = fs::read_dir(self.buttons_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(profile) = serde_json::from_str::<ButtonProfile>(&content) {
                            map.insert(profile.id.clone(), profile);
                        }
                    }
                }
            }
        }
        if map.is_empty() && !seeded {
            for b in ButtonProfile::builtin_presets() {
                map.insert(b.id.clone(), b);
            }
        }
        let mut list: Vec<_> = map.into_values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn load_button_profile(&self, id: &str) -> Option<ButtonProfile> {
        let path = self.buttons_dir().join(format!("{}.json", id));
        if let Ok(content) = fs::read_to_string(&path) {
            serde_json::from_str(&content).ok()
        } else if !self.seed_marker_path().exists() {
            ButtonProfile::builtin_presets().into_iter().find(|p| p.id == id)
        } else {
            None
        }
    }

    pub fn save_button_profile(&self, profile: &ButtonProfile) -> std::io::Result<()> {
        let path = self.buttons_dir().join(format!("{}.json", profile.id));
        let json = serde_json::to_string_pretty(profile)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_user_owned_file(&path, json.as_bytes())
    }

    pub fn delete_button_profile(&self, id: &str) -> std::io::Result<bool> {
        let _ = fs::write(self.seed_marker_path(), b"1");
        let path = self.buttons_dir().join(format!("{}.json", id));
        if path.exists() {
            fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ================= Macro Profiles ================= //

    pub fn list_macro_profiles(&self) -> Vec<MacroProfile> {
        let mut map: HashMap<String, MacroProfile> = HashMap::new();
        let seeded = self.seed_marker_path().exists();
        if let Ok(entries) = fs::read_dir(self.macros_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(profile) = serde_json::from_str::<MacroProfile>(&content) {
                            map.insert(profile.id.clone(), profile);
                        }
                    }
                }
            }
        }
        if map.is_empty() && !seeded {
            for m in MacroProfile::builtin_presets() {
                map.insert(m.id.clone(), m);
            }
        }
        let mut list: Vec<_> = map.into_values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn load_macro_profile(&self, id: &str) -> Option<MacroProfile> {
        let path = self.macros_dir().join(format!("{}.json", id));
        if let Ok(content) = fs::read_to_string(&path) {
            serde_json::from_str(&content).ok()
        } else if !self.seed_marker_path().exists() {
            MacroProfile::builtin_presets().into_iter().find(|p| p.id == id)
        } else {
            None
        }
    }

    pub fn save_macro_profile(&self, profile: &MacroProfile) -> std::io::Result<()> {
        let path = self.macros_dir().join(format!("{}.json", profile.id));
        let json = serde_json::to_string_pretty(profile)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_user_owned_file(&path, json.as_bytes())
    }

    pub fn delete_macro_profile(&self, id: &str) -> std::io::Result<bool> {
        let _ = fs::write(self.seed_marker_path(), b"1");
        let path = self.macros_dir().join(format!("{}.json", id));
        if path.exists() {
            fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ================= Settings Profiles ================= //

    pub fn list_settings_profiles(&self) -> Vec<SettingsProfile> {
        let mut map: HashMap<String, SettingsProfile> = HashMap::new();
        let seeded = self.seed_marker_path().exists();
        if let Ok(entries) = fs::read_dir(self.settings_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(profile) = serde_json::from_str::<SettingsProfile>(&content) {
                            map.insert(profile.id.clone(), profile);
                        }
                    }
                }
            }
        }
        if map.is_empty() && !seeded {
            for s in SettingsProfile::builtin_presets() {
                map.insert(s.id.clone(), s);
            }
        }
        let mut list: Vec<_> = map.into_values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn load_settings_profile(&self, id: &str) -> Option<SettingsProfile> {
        let path = self.settings_dir().join(format!("{}.json", id));
        if let Ok(content) = fs::read_to_string(&path) {
            serde_json::from_str(&content).ok()
        } else if !self.seed_marker_path().exists() {
            SettingsProfile::builtin_presets().into_iter().find(|p| p.id == id)
        } else {
            None
        }
    }

    pub fn save_settings_profile(&self, profile: &SettingsProfile) -> std::io::Result<()> {
        let path = self.settings_dir().join(format!("{}.json", profile.id));
        let json = serde_json::to_string_pretty(profile)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_user_owned_file(&path, json.as_bytes())
    }

    pub fn delete_settings_profile(&self, id: &str) -> std::io::Result<bool> {
        let _ = fs::write(self.seed_marker_path(), b"1");
        let path = self.settings_dir().join(format!("{}.json", id));
        if path.exists() {
            fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_lighting_presets() {
        let presets = LightingProfile::builtin_presets();
        assert_eq!(presets.len(), 6);
        let ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"gaming"));
        assert!(ids.contains(&"cyberpunk"));
        assert!(ids.contains(&"rainbow_wave"));
        assert!(ids.contains(&"neon_sunset"));
        assert!(ids.contains(&"matrix"));
        assert!(ids.contains(&"static_purple"));
    }

    #[test]
    fn test_profile_manager_crud_in_temp_dir() {
        let temp_dir = std::env::temp_dir().join("fantastech_test_profiles_crud");
        let _ = fs::remove_dir_all(&temp_dir);

        let mgr = ProfileManager::new(Some(temp_dir.clone()));
        let initial_lighting = mgr.list_lighting_profiles();
        assert_eq!(initial_lighting.len(), 6);

        // Add a custom profile
        let custom = LightingProfile {
            id: "my_cool_profile".to_string(),
            name: "My Cool Profile".to_string(),
            description: "Custom user profile".to_string(),
            mode: LightingMode::Static,
            speed: 50,
            brightness: 100,
            direction: 0,
            color: [255, 100, 50],
            color_mode: Some(ColorMode::FixedColor),
            canvas: HashMap::new(),
            is_builtin: false,
        };
        mgr.save_lighting_profile(&custom).expect("save profile");

        let loaded = mgr.load_lighting_profile("my_cool_profile").expect("load profile");
        assert_eq!(loaded.name, "My Cool Profile");
        assert_eq!(loaded.color, [255, 100, 50]);

        // Delete profile
        let deleted = mgr.delete_lighting_profile("my_cool_profile").expect("delete profile");
        assert!(deleted);
        assert!(mgr.load_lighting_profile("my_cool_profile").is_none());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_and_alter_premade_preset() {
        let temp_dir = std::env::temp_dir().join("fantastech_test_premade_delete");
        let _ = fs::remove_dir_all(&temp_dir);

        let mgr = ProfileManager::new(Some(temp_dir.clone()));
        assert_eq!(mgr.list_lighting_profiles().len(), 6);
        assert!(mgr.load_lighting_profile("gaming").is_some());

        // Alter premade profile
        let mut altered = mgr.load_lighting_profile("gaming").unwrap();
        altered.name = "Altered Gaming".to_string();
        altered.color = [10, 20, 30];
        mgr.save_lighting_profile(&altered).expect("save altered preset");

        let reloaded = mgr.load_lighting_profile("gaming").unwrap();
        assert_eq!(reloaded.name, "Altered Gaming");
        assert_eq!(reloaded.color, [10, 20, 30]);

        // Delete premade profile
        let deleted = mgr.delete_lighting_profile("gaming").expect("delete preset");
        assert!(deleted);
        assert!(mgr.load_lighting_profile("gaming").is_none());
        assert_eq!(mgr.list_lighting_profiles().len(), 5);

        // Re-instantiate ProfileManager in the same directory (simulating service/app restart)
        let mgr2 = ProfileManager::new(Some(temp_dir.clone()));
        assert!(mgr2.load_lighting_profile("gaming").is_none());
        assert_eq!(mgr2.list_lighting_profiles().len(), 5);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_profile_manager_default_dir() {
        let mgr = ProfileManager::new(None);
        println!("Default root_dir: {:?}", mgr.root_dir());
        let res = mgr.ensure_seed_profiles();
        println!("ensure_seed_profiles res: {:?}", res);
        let lighting = mgr.list_lighting_profiles();
        println!("lighting count: {}", lighting.len());
        assert!(!lighting.is_empty());
    }
}
