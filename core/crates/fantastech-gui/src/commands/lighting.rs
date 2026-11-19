//! RGB lighting configuration, custom canvas painting, and designer palettes.

use super::*;
use fantastech_core::theme::ThemeConfig;
use fantastech_protocol::lighting::{AnimationDirection, ColorMode, LightingConfig, LightingMode, LightingZone, RgbColor};

impl FantasTechApp {
    pub(crate) fn set_single_key_color(&mut self, key_index: u8) {
        if key_index as usize >= SPARSE_KEY_COUNT {
            self.status_message = format!("Invalid lighting matrix index {}; no color was applied", key_index);
            return;
        }

        let raw_color = if (key_index as usize) < self.key_colors.len() {
            let col = self.key_colors[key_index as usize];
            [col.r(), col.g(), col.b()]
        } else {
            self.current_color
        };
        let factor = (self.brightness as f32 / 4.0).clamp(0.0, 1.0);
        let color = [
            (raw_color[0] as f32 * factor).round() as u8,
            (raw_color[1] as f32 * factor).round() as u8,
            (raw_color[2] as f32 * factor).round() as u8,
        ];
        let mirror = get_mirror_key_index(key_index);

        // Dry-run must apply uniformly to both the IPC and direct-HID paths.
        if self.dry_run_enabled {
            self.status_message = format!(
                "[DRY-RUN] Previewed key {} color #{:02X}{:02X}{:02X}",
                key_index, color[0], color[1], color[2]
            );
            return;
        }

        if self.check_service() {
            let mut request_error = None;
            for index in std::iter::once(key_index).chain(mirror) {
                match send_ipc_request(&IpcRequest::SetKeyColor {
                    key_index: index,
                    r: color[0],
                    g: color[1],
                    b: color[2],
                }) {
                    Ok(IpcResponse::Ok) => {}
                    Ok(response) => request_error = Some(format!("Unexpected service response: {response:?}")),
                    Err(error) => request_error = Some(format!("IPC error updating key color: {error}")),
                }
            }
            self.status_message = request_error.unwrap_or_else(|| {
                format!("Updated RGB color for key {}", key_index)
            });
        } else if let Some(path) = self.permission_denied_node.as_ref() {
            self.status_message = platform::permission_error("update the key color", path.as_path());
        } else if let Ok(transport) = HidrawTransport::discover() {
            let mut device = Device::new_read_only(transport).into_reversible();
            device.set_dry_run(false);
            let rgb = RgbColor::new(color[0], color[1], color[2]);
            let mut failures = Vec::new();
            if let Err(error) = device.set_single_key_color(key_index, rgb) {
                failures.push(format!("key {}: {error:?}", key_index));
            }
            if let Some(mirror_index) = mirror {
                if let Err(error) = device.set_single_key_color(mirror_index, rgb) {
                    failures.push(format!("key {}: {error:?}", mirror_index));
                }
            }
            self.status_message = if failures.is_empty() {
                format!("Updated RGB color for key {}", key_index)
            } else {
                format!("RGB update failed ({})", failures.join("; "))
            };
        } else if self.is_emulator {
            self.status_message = format!("Virtual emulator: previewed RGB color for key {}", key_index);
        } else {
            self.apply_lighting();
        }
    }

    pub(crate) fn select_all_custom_keys(&mut self) {
        self.custom_select_all = true;
        self.custom_selected_keys.clear();
        for k in ANSI_104_KEYS {
            self.custom_selected_keys.insert(k.index);
        }
        self.status_message = "Selected all 104 keys on Custom Canvas".to_string();
    }

    pub(crate) fn deselect_all_custom_keys(&mut self) {
        self.custom_select_all = false;
        self.custom_selected_keys.clear();
        self.status_message = "Cleared Custom Canvas key selection".to_string();
    }

    pub(crate) fn toggle_custom_key_selection(&mut self, key_index: u8, multi: bool) {
        if multi {
            if self.custom_select_all {
                self.custom_select_all = false;
                self.custom_selected_keys.clear();
                for k in ANSI_104_KEYS {
                    if k.index != key_index {
                        self.custom_selected_keys.insert(k.index);
                    }
                }
            } else if self.custom_selected_keys.contains(&key_index) {
                self.custom_selected_keys.remove(&key_index);
            } else {
                self.custom_selected_keys.insert(key_index);
            }
        } else {
            self.custom_select_all = false;
            self.custom_selected_keys.clear();
            self.custom_selected_keys.insert(key_index);
        }
        if self.custom_selected_keys.len() == ANSI_104_KEYS.len() {
            self.custom_select_all = true;
        }
    }

    pub(crate) fn record_used_color(&mut self, rgb: [u8; 3]) {
        self.recent_colors.retain(|&c| c != rgb);
        self.recent_colors.insert(0, rgb);
        if self.recent_colors.len() > 6 {
            self.recent_colors.truncate(6);
        }
    }

    pub(crate) fn paint_key_and_mirror(&mut self, key_index: u8, rgb: [u8; 3]) {
        self.record_used_color(rgb);
        let painted = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        let idx = key_index as usize;
        if idx < SPARSE_KEY_COUNT {
            self.key_colors[idx] = painted;
            if let Some(mirror) = get_mirror_key_index(key_index) {
                if (mirror as usize) < SPARSE_KEY_COUNT {
                    self.key_colors[mirror as usize] = painted;
                }
            }
            self.persist_current_lighting_profile();
            self.set_single_key_color(key_index);
        }
    }

    pub(crate) fn apply_color_to_custom_selection(&mut self, rgb: [u8; 3]) {
        if self.custom_selected_keys.is_empty() && !self.custom_select_all {
            self.status_message = "No keys selected to paint. Click keys to select them first.".to_string();
            return;
        }
        self.record_used_color(rgb);
        self.current_color = rgb;
        let painted = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        let keys_to_paint: Vec<u8> = if self.custom_select_all {
            ANSI_104_KEYS.iter().map(|k| k.index).collect()
        } else {
            self.custom_selected_keys.iter().copied().collect()
        };

        for &key_idx in &keys_to_paint {
            if (key_idx as usize) < SPARSE_KEY_COUNT {
                self.key_colors[key_idx as usize] = painted;
                if let Some(mirror) = get_mirror_key_index(key_idx) {
                    if (mirror as usize) < SPARSE_KEY_COUNT {
                        self.key_colors[mirror as usize] = painted;
                    }
                }
            }
        }
        self.persist_current_lighting_profile();

        if self.custom_select_all {
            self.apply_lighting();
            self.status_message = format!("Painted all keys with #{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
        } else {
            for &k in &keys_to_paint {
                self.set_single_key_color(k);
            }
            self.status_message = format!("Painted {} keys with #{:02X}{:02X}{:02X}", keys_to_paint.len(), rgb[0], rgb[1], rgb[2]);
        }
    }

    pub(crate) fn turn_off_custom_selection(&mut self) {
        self.apply_color_to_custom_selection([0, 0, 0]);
    }

    pub(crate) fn select_custom_key_group(&mut self, group: &str) {
        self.custom_select_all = false;
        self.custom_selected_keys.clear();
        match group {
            "wasd" => {
                for &idx in &[44, 64, 65, 66] {
                    self.custom_selected_keys.insert(idx);
                }
                self.status_message = "Selected WASD keys".to_string();
            }
            "arrows" => {
                for &idx in &[99, 119, 120, 121] {
                    self.custom_selected_keys.insert(idx);
                }
                self.status_message = "Selected Arrow keys".to_string();
            }
            "numpad" => {
                for &idx in &[38, 39, 40, 41, 59, 60, 61, 62, 80, 81, 82, 101, 102, 103, 104, 122, 123, 124] {
                    self.custom_selected_keys.insert(idx);
                }
                self.status_message = "Selected Numpad keys".to_string();
            }
            "fkeys" => {
                self.custom_selected_keys.insert(0); // Esc
                for idx in 1..=12 {
                    self.custom_selected_keys.insert(idx);
                }
                self.status_message = "Selected Function row (F1-F12 + Esc)".to_string();
            }
            _ => {}
        }
    }

    pub(crate) fn apply_designer_palette(&mut self, palette_name: &str) {
        let (primary, p_name) = match palette_name {
            "miami" => ([0, 240, 255], "Cyberpunk Miami"),
            "terminal" => ([0, 255, 65], "Matrix Terminal"),
            "flare" => ([255, 69, 0], "Solar Flare"),
            "vaporwave" => ([157, 78, 221], "Vaporwave Dusk"),
            "frost" => ([58, 134, 255], "Arctic Frost"),
            "blood_moon" => ([217, 4, 41], "Blood Moon"),
            "pastel" => ([183, 228, 199], "Pastel Dream"),
            "rainbow" => ([255, 0, 0], "Spectrum Rainbow"),
            _ => ([0, 113, 207], "Designer Palette"),
        };

        self.current_color = primary;
        self.record_used_color(primary);

        // Selecting a Designer Palette switches to Custom Canvas (16), restores brightness if off
        self.selected_mode = 16;
        if self.brightness == 0 {
            self.brightness = 4;
        }

        // Clear what was before it: deselect all keys and wipe key colors to black
        self.custom_selected_keys.clear();
        self.custom_select_all = false;
        self.key_colors.fill(Color32::BLACK);

        // Compute per-key designer gradient across ANSI_104_KEYS
        for vkey in ANSI_104_KEYS {
            let idx = vkey.index as usize;
            if idx >= SPARSE_KEY_COUNT {
                continue;
            }
            let t_col: f32 = (vkey.x / 22.0_f32).clamp(0.0_f32, 1.0_f32);
            let t_row: f32 = (vkey.y / 5.35_f32).clamp(0.0_f32, 1.0_f32);
            let rgb = match palette_name {
                "miami" => {
                    // Cyan [0, 240, 255] -> Hot Pink [255, 0, 127]
                    [
                        (t_col * 255.0_f32).round() as u8,
                        (240.0_f32 * (1.0_f32 - t_col)).round() as u8,
                        (255.0_f32 * (1.0_f32 - t_col) + 127.0_f32 * t_col).round() as u8,
                    ]
                }
                "terminal" => {
                    // Forest green [0, 59, 0] -> Bright terminal green [0, 255, 65]
                    [
                        0,
                        (59.0_f32 + t_row * (255.0_f32 - 59.0_f32)).round() as u8,
                        (t_row * 65.0_f32).round() as u8,
                    ]
                }
                "flare" => {
                    // Flame Orange [255, 69, 0] -> Sunburst Gold [255, 215, 0]
                    [
                        255,
                        (69.0_f32 + t_col * (215.0_f32 - 69.0_f32)).round() as u8,
                        0,
                    ]
                }
                "vaporwave" => {
                    // Lilac Purple [157, 78, 221] -> Sky Blue [72, 202, 228]
                    let t_diag: f32 = ((vkey.x + vkey.y * 3.0_f32) / 38.0_f32).clamp(0.0_f32, 1.0_f32);
                    [
                        (157.0_f32 * (1.0_f32 - t_diag) + 72.0_f32 * t_diag).round() as u8,
                        (78.0_f32 * (1.0_f32 - t_diag) + 202.0_f32 * t_diag).round() as u8,
                        (221.0_f32 * (1.0_f32 - t_diag) + 228.0_f32 * t_diag).round() as u8,
                    ]
                }
                "frost" => {
                    // Glacial Cyan [224, 251, 252] -> Ice Blue [58, 134, 255]
                    [
                        (224.0_f32 * (1.0_f32 - t_col) + 58.0_f32 * t_col).round() as u8,
                        (251.0_f32 * (1.0_f32 - t_col) + 134.0_f32 * t_col).round() as u8,
                        255,
                    ]
                }
                "blood_moon" => {
                    let is_mod = [0, 13, 27, 28, 42, 55, 69, 83, 84, 96, 98, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114].contains(&vkey.index);
                    if is_mod {
                        [25, 25, 25] // Deep Obsidian
                    } else {
                        [217, 4, 41] // Crimson Red
                    }
                }
                "pastel" => {
                    // Mint [183, 228, 199] -> Peach [255, 202, 212]
                    [
                        (183.0_f32 + t_col * (255.0_f32 - 183.0_f32)).round() as u8,
                        (228.0_f32 * (1.0_f32 - t_col) + 202.0_f32 * t_col).round() as u8,
                        (199.0_f32 * (1.0_f32 - t_col) + 212.0_f32 * t_col).round() as u8,
                    ]
                }
                _ => {
                    let hue = (vkey.x / 22.0_f32) * 360.0_f32;
                    let (r, g, b) = hsv_to_rgb_simple(hue, 1.0, 1.0);
                    [r, g, b]
                }
            };
            self.key_colors[idx] = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            if let Some(mirror) = get_mirror_key_index(vkey.index) {
                if (mirror as usize) < SPARSE_KEY_COUNT {
                    self.key_colors[mirror as usize] = self.key_colors[idx];
                }
            }
        }
        self.apply_lighting();
        self.status_message = format!("Applied '{}' designer palette across keyboard matrix", p_name);
    }

    pub(crate) fn apply_lighting(&mut self) {
        let mode = LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum);
        let color_mode = if mode.supports_color() && (self.is_fixed_color || mode == LightingMode::Static) {
            ColorMode::FixedColor
        } else {
            ColorMode::DynamicSpectrum
        };
        let dir = if mode.supports_direction() && self.direction == 1 {
            AnimationDirection::LeftToRight
        } else {
            AnimationDirection::Default
        };
        let config = LightingConfig {
            zone: LightingZone::MainBacklight,
            mode,
            brightness: self.brightness,
            speed: self.speed,
            direction: dir,
            color_mode,
            color: RgbColor::new(self.current_color[0], self.current_color[1], self.current_color[2]),
        };

        self.persist_current_lighting_profile();

        if self.dry_run_enabled {
            self.status_message = format!("[DRY-RUN] Lighting config simulated: mode {:?}", config.mode);
            return;
        }

        if self.check_service() {
            if mode == LightingMode::CustomMatrixCanvas {
                let mut matrix = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                let factor = (self.brightness as f32 / 4.0).clamp(0.0, 1.0);
                for (idx, color) in self.key_colors.iter().enumerate() {
                    if idx < SPARSE_KEY_COUNT {
                        let r = (color.r() as f32 * factor).round() as u8;
                        let g = (color.g() as f32 * factor).round() as u8;
                        let b = (color.b() as f32 * factor).round() as u8;
                        matrix[idx] = RgbColor::new(r, g, b);
                    }
                }
                let colors: Vec<[u8; 3]> = matrix.iter().map(|c| [c.r, c.g, c.b]).collect();
                match send_ipc_request(&IpcRequest::SetCanvasMatrix { colors }) {
                    Ok(IpcResponse::Ok) => {
                        self.status_message = "Service (IPC): Applied custom matrix canvas".to_string();
                    }
                    Ok(resp) => {
                        self.status_message = format!("Service response: {:?}", resp);
                    }
                    Err(e) => {
                        self.status_message = format!("IPC error setting canvas: {}", e);
                    }
                }
            } else {
                match send_ipc_request(&IpcRequest::SetLightingMode {
                    mode,
                    brightness: self.brightness,
                    speed: self.speed,
                    direction: self.direction,
                    color_mode,
                    color: self.current_color,
                }) {
                    Ok(IpcResponse::Ok) => {
                        self.status_message = format!("Service (IPC): Applied lighting mode {:?}", mode);
                    }
                    Ok(resp) => {
                        self.status_message = format!("Service response: {:?}", resp);
                    }
                    Err(e) => {
                        self.status_message = format!("IPC error setting lighting: {}", e);
                    }
                }
            }
        } else if let Some(ref path) = self.permission_denied_node {
            self.status_message = platform::permission_error("apply lighting to the keyboard", path.as_path());
        } else if let Ok(t) = HidrawTransport::discover() {
            let mut dev = Device::new_read_only(t).into_reversible();
            dev.set_dry_run(self.dry_run_enabled);
            if mode == LightingMode::CustomMatrixCanvas {
                let mut matrix = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                let factor = (self.brightness as f32 / 4.0).clamp(0.0, 1.0);
                for (idx, color) in self.key_colors.iter().enumerate() {
                    if idx < SPARSE_KEY_COUNT {
                        let r = (color.r() as f32 * factor).round() as u8;
                        let g = (color.g() as f32 * factor).round() as u8;
                        let b = (color.b() as f32 * factor).round() as u8;
                        matrix[idx] = RgbColor::new(r, g, b);
                    }
                }
                match dev.stream_matrix_canvas(&matrix) {
                    Ok(_) => {
                        self.status_message = "Hardware updated: Custom Matrix Canvas applied".to_string();
                    }
                    Err(e) => {
                        self.status_message = format!("Hardware error setting canvas: {}", e);
                    }
                }
            } else {
                match dev.set_lighting(&config) {
                    Ok(_) => {
                        self.status_message = format!("Hardware updated: mode {:?}, brightness {}, speed {}", config.mode, self.brightness, self.speed);
                    }
                    Err(e) => {
                        self.status_message = format!("Hardware error setting lighting: {}", e);
                    }
                }
            }
        } else if self.is_emulator {
            self.status_message = format!("Virtual Emulator: Simulated applied lighting mode {:?}", config.mode);
        } else {
            self.status_message = "No physical keyboard detected or device access was denied by the operating system".to_string();
        }
    }

    #[allow(dead_code)]
    pub(crate) fn apply_theme(&mut self, theme: ThemeConfig) {
        let matrix = theme.to_matrix();
        for (i, c) in matrix.iter().enumerate() {
            if i < self.key_colors.len() {
                self.key_colors[i] = Color32::from_rgb(c.r, c.g, c.b);
            }
        }

        if self.dry_run_enabled {
            self.status_message = format!("[DRY-RUN] Streamed theme '{}'", theme.name);
            return;
        }

        if self.check_service() {
            let colors: Vec<[u8; 3]> = matrix.iter().map(|c| [c.r, c.g, c.b]).collect();
            match send_ipc_request(&IpcRequest::SetCanvasMatrix { colors }) {
                Ok(IpcResponse::Ok) => {
                    self.status_message = format!("Service (IPC): Streamed theme '{}' to keyboard", theme.name);
                }
                Ok(resp) => {
                    self.status_message = format!("Service response: {:?}", resp);
                }
                Err(e) => {
                    self.status_message = format!("IPC error streaming theme: {}", e);
                }
            }
        } else if let Some(ref path) = self.permission_denied_node {
            self.status_message = platform::permission_error("stream the lighting theme", path.as_path());
        } else if self.is_emulator {
            self.status_message = format!("Loaded theme '{}' into emulator", theme.name);
        } else if let Ok(t) = HidrawTransport::discover() {
            let mut dev = Device::new_read_only(t).into_reversible();
            dev.set_dry_run(self.dry_run_enabled);
            let _ = dev.stream_matrix_canvas(&matrix);
            let _ = dev.set_lighting(&LightingConfig {
                mode: LightingMode::CustomMatrixCanvas,
                ..Default::default()
            });
            self.status_message = format!("Streamed theme '{}' to keyboard", theme.name);
        }
    }

    pub(crate) fn apply_queried_lighting_config(&mut self, config: LightingConfig) {
        self.selected_mode = config.mode as u8;
        self.brightness = if config.brightness == 0 && config.mode != LightingMode::Off {
            4
        } else {
            config.brightness
        };
        self.speed = config.speed;
        self.direction = match config.direction {
            AnimationDirection::LeftToRight => 1,
            _ => 0,
        };
        self.is_fixed_color = config.color_mode == ColorMode::FixedColor || config.mode == LightingMode::Static;
        self.current_color = [config.color.r, config.color.g, config.color.b];
        self.record_used_color(self.current_color);

        let profiles = self.profile_manager.list_lighting_profiles();
        if let Some(matching) = profiles
            .iter()
            .find(|p| {
                p.mode == config.mode
                    && (!config.mode.supports_color() || p.color == [config.color.r, config.color.g, config.color.b])
            })
            .or_else(|| profiles.iter().find(|p| p.mode == config.mode))
        {
            self.selected_lighting_profile = matching.id.clone();
        }
    }

}
