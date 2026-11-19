//! Multi-domain profile management: buttons, lighting, macros, and settings.

use super::*;
use fantastech_core::profiles::{ButtonProfile, LightingProfile, MacroProfile};

impl FantasTechApp {
    fn generate_profile_id(name: &str) -> Option<(String, String)> {
        let clean = name.trim();
        if clean.is_empty() {
            return None;
        }
        let slug: String = clean
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let slug_trimmed = slug.trim_matches('_');
        let base_slug = if slug_trimmed.is_empty() { "profile" } else { slug_trimmed };
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() % 100000;
        Some((clean.to_string(), format!("{}_{}", base_slug, ts)))
    }

    fn export_profile_json<T: serde::Serialize>(&mut self, path: &std::path::Path, profile: &T, kind: &str) -> bool {
        match serde_json::to_string_pretty(profile) {
            Ok(json_str) => match fantastech_core::write_user_owned_file(path, json_str.as_bytes()) {
                Ok(_) => {
                    self.status_message = format!("Exported and saved {} profile to '{}'", kind, path.display());
                    true
                }
                Err(e) => {
                    self.status_message = format!("Failed to write {} profile to '{}': {}", kind, path.display(), e);
                    false
                }
            },
            Err(e) => {
                self.status_message = format!("Failed to serialize {} profile: {}", kind, e);
                false
            }
        }
    }

    pub(crate) fn apply_lighting_profile_by_id(&mut self, id: &str) {
        if let Some(p) = self.profile_manager.load_lighting_profile(id) {
            self.selected_lighting_profile = p.id.clone();
            self.selected_mode = p.mode as u8;
            self.brightness = if p.brightness <= 4 {
                p.brightness
            } else {
                ((p.brightness as f32 / 100.0) * 4.0).round().clamp(0.0, 4.0) as u8
            };
            if self.brightness == 0 && p.mode != LightingMode::Off {
                self.brightness = 4;
            }
            self.speed = if p.speed <= 4 {
                p.speed
            } else {
                ((p.speed as f32 / 100.0) * 4.0).round().clamp(0.0, 4.0) as u8
            };
            self.direction = p.direction;
            self.current_color = p.color;
            if let Some(cm) = p.color_mode {
                self.is_fixed_color = cm == ColorMode::FixedColor;
            } else {
                self.is_fixed_color = p.mode == LightingMode::Static;
            }

            // Clear what was before it: deselect all keys and wipe key colors to black
            self.custom_selected_keys.clear();
            self.custom_select_all = false;
            self.key_colors.fill(Color32::BLACK);

            if p.mode != LightingMode::Off {
                let matrix = p.to_matrix();
                for (key_idx, rgb) in matrix.iter().enumerate() {
                    if key_idx < SPARSE_KEY_COUNT {
                        self.key_colors[key_idx] = Color32::from_rgb(rgb.r, rgb.g, rgb.b);
                    }
                }
            }

            if self.check_service() {
                match send_ipc_request(&IpcRequest::ApplyLightingProfile { id: id.to_string() }) {
                    Ok(IpcResponse::Ok) => {
                        self.status_message = format!("Service (IPC): Applied lighting profile '{}'", p.name);
                    }
                    Ok(resp) => {
                        self.status_message = format!("Service response: {:?}", resp);
                    }
                    Err(e) => {
                        self.status_message = format!("IPC error applying profile: {}", e);
                    }
                }
            } else {
                self.apply_lighting();
            }
        } else {
            self.status_message = format!("Profile '{}' not found", id);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn apply_button_profile_by_id(&mut self, id: &str) {
        self.undo_keymaps.clear();
        self.redo_keymaps.clear();
        if let Some(b) = self.profile_manager.load_button_profile(id) {
            self.selected_button_profile = b.id.clone();
            let prof = (self.active_profile as usize).min(2);
            match b.base_layer.to_descriptors() {
                Ok(base_actions) => {
                    self.profiles[prof][0] = base_actions;
                }
                Err(e) => {
                    self.status_message = format!("Error resolving base keymap: {}", e);
                    return;
                }
            }
            match b.fn_layer.to_descriptors() {
                Ok(fn_actions) => {
                    self.profiles[prof][1] = fn_actions;
                }
                Err(e) => {
                    self.status_message = format!("Error resolving Fn keymap: {}", e);
                    return;
                }
            }

            if self.check_service() {
                match send_ipc_request(&IpcRequest::ApplyButtonProfile { id: id.to_string() }) {
                    Ok(IpcResponse::Ok) => {
                        self.status_message = format!("Service (IPC): Applied button profile '{}'", b.name);
                    }
                    Ok(resp) => {
                        self.status_message = format!("Service response: {:?}", resp);
                    }
                    Err(e) => {
                        self.status_message = format!("IPC error applying button profile: {}", e);
                    }
                }
            } else {
                self.apply_current_layer_to_hardware();
            }
        } else {
            self.status_message = format!("Button profile '{}' not found", id);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn apply_macro_profile_by_id(&mut self, id: &str) {
        if let Some(m) = self.profile_manager.load_macro_profile(id) {
            self.selected_macro_profile = m.id.clone();
            if let Some(first) = m.macros.first() {
                self.macro_sequence = first.clone();
                self.active_macro_slot = first.slot;
            }
            if self.check_service() {
                match send_ipc_request(&IpcRequest::ApplyMacroProfile { id: id.to_string() }) {
                    Ok(IpcResponse::Ok) => {
                        self.status_message = format!("Service (IPC): Applied macro profile '{}'", m.name);
                    }
                    Ok(resp) => {
                        self.status_message = format!("Service response: {:?}", resp);
                    }
                    Err(e) => {
                        self.status_message = format!("IPC error applying macro profile: {}", e);
                    }
                }
            } else {
                self.status_message = format!("Loaded macro profile '{}' ({} sequences)", m.name, m.macros.len());
            }
        } else {
            self.status_message = format!("Macro profile '{}' not found", id);
        }
    }

    pub(crate) fn clone_active_button_profile(&mut self) {
        let current_id = self.selected_button_profile.clone();
        let base_profile = self.profile_manager.load_button_profile(&current_id)
            .or_else(|| self.profile_manager.load_button_profile("default"));
        let (base_layer, fn_layer, orig_name) = if let Some(ref bp) = base_profile {
            (bp.base_layer.clone(), bp.fn_layer.clone(), bp.name.clone())
        } else {
            let prof = (self.active_profile as usize).min(2);
            (
                LayerKeymapConfig::from_descriptors(&self.profiles[prof][0], KeyLayer::Base),
                LayerKeymapConfig::from_descriptors(&self.profiles[prof][1], KeyLayer::Fn),
                "Default".to_string(),
            )
        };
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() % 10000;
        let new_id = format!("{}_copy_{}", current_id, ts);
        let new_name = format!("{} (Copy)", orig_name);
        let new_profile = ButtonProfile {
            id: new_id.clone(),
            name: new_name,
            description: format!("Cloned from {}", orig_name),
            base_layer,
            fn_layer,
            is_builtin: false,
        };
        let _ = self.profile_manager.save_button_profile(&new_profile);
        self.apply_button_profile_by_id(&new_id);
        self.status_message = format!("Cloned profile '{}'", new_profile.name);
    }

    pub(crate) fn create_new_button_profile(&mut self, name: &str, copy_current: bool) {
        let (clean_name, new_id) = match Self::generate_profile_id(name) {
            Some(res) => res,
            None => {
                self.status_message = "Profile name cannot be empty".to_string();
                return;
            }
        };

        let (base_layer, fn_layer) = if copy_current {
            let prof = (self.active_profile as usize).min(2);
            (
                LayerKeymapConfig::from_descriptors(&self.profiles[prof][0], KeyLayer::Base),
                LayerKeymapConfig::from_descriptors(&self.profiles[prof][1], KeyLayer::Fn),
            )
        } else {
            (
                LayerKeymapConfig::from_descriptors(&default_layer_keymap(KeyLayer::Base), KeyLayer::Base),
                LayerKeymapConfig::from_descriptors(&default_layer_keymap(KeyLayer::Fn), KeyLayer::Fn),
            )
        };

        let new_profile = ButtonProfile {
            id: new_id.clone(),
            name: clean_name.clone(),
            description: format!("Custom button profile: {}", clean_name),
            base_layer,
            fn_layer,
            is_builtin: false,
        };

        match self.profile_manager.save_button_profile(&new_profile) {
            Ok(_) => {
                self.apply_button_profile_by_id(&new_id);
                self.status_message = format!("Created new button profile '{}'", clean_name);
            }
            Err(e) => {
                self.status_message = format!("Failed to create profile '{}': {}", clean_name, e);
            }
        }
    }

    pub(crate) fn delete_button_profile_by_id(&mut self, id: &str) {
        if let Some(p) = self.profile_manager.load_button_profile(id) {
            if p.is_builtin || id == "default" {
                self.status_message = format!("Cannot delete built-in profile '{}'", p.name);
                return;
            }
            match self.profile_manager.delete_button_profile(id) {
                Ok(true) => {
                    if self.selected_button_profile == id {
                        self.apply_button_profile_by_id("default");
                    }
                    self.status_message = format!("Deleted button profile '{}'", p.name);
                }
                Ok(false) => {
                    self.status_message = format!("Profile '{}' was not found on disk", p.name);
                }
                Err(e) => {
                    self.status_message = format!("Error deleting profile '{}': {}", p.name, e);
                }
            }
        }
    }

    pub(crate) fn delete_active_button_profile(&mut self) {
        let current_id = self.selected_button_profile.clone();
        self.delete_button_profile_by_id(&current_id);
    }

    pub(crate) fn reset_active_button_profile(&mut self) {
        self.undo_keymaps.clear();
        self.redo_keymaps.clear();
        let prof = (self.active_profile as usize).min(2);
        self.profiles[prof][0] = default_layer_keymap(KeyLayer::Base);
        self.profiles[prof][1] = default_layer_keymap(KeyLayer::Fn);
        let current_id = self.selected_button_profile.clone();
        if let Some(mut p) = self.profile_manager.load_button_profile(&current_id) {
            if !p.is_builtin {
                p.base_layer = LayerKeymapConfig::from_descriptors(&self.profiles[prof][0], KeyLayer::Base);
                p.fn_layer = LayerKeymapConfig::from_descriptors(&self.profiles[prof][1], KeyLayer::Fn);
                let _ = self.profile_manager.save_button_profile(&p);
            }
        }
        self.apply_current_layer_to_hardware();
        self.status_message = format!("Reset {:?} layer to factory defaults and applied to keyboard", self.active_layer);
    }

    pub(crate) fn persist_current_button_profile(&mut self) {
        let current_id = self.selected_button_profile.clone();
        let prof = (self.active_profile as usize).min(2);
        let base_layer = LayerKeymapConfig::from_descriptors(&self.profiles[prof][0], KeyLayer::Base);
        let fn_layer = LayerKeymapConfig::from_descriptors(&self.profiles[prof][1], KeyLayer::Fn);
        if let Some(mut p) = self.profile_manager.load_button_profile(&current_id) {
            p.base_layer = base_layer;
            p.fn_layer = fn_layer;
            let _ = self.profile_manager.save_button_profile(&p);
        } else {
            let p = ButtonProfile {
                id: current_id.clone(),
                name: current_id.clone(),
                description: "Custom button profile".to_string(),
                base_layer,
                fn_layer,
                is_builtin: false,
            };
            let _ = self.profile_manager.save_button_profile(&p);
        }
    }

    pub(crate) fn clone_active_lighting_profile(&mut self) {
        let current_id = self.selected_lighting_profile.clone();
        let base_profile = self.profile_manager.load_lighting_profile(&current_id)
            .or_else(|| self.profile_manager.load_lighting_profile("gaming"));
        let (mode, speed, brightness, direction, color, canvas, orig_name) = if let Some(ref lp) = base_profile {
            (lp.mode, lp.speed, lp.brightness, lp.direction, lp.color, lp.canvas.clone(), lp.name.clone())
        } else {
            let mut canvas_map = std::collections::HashMap::new();
            for (i, c) in self.key_colors.iter().enumerate() {
                if *c != Color32::BLACK {
                    canvas_map.insert(i, [c.r(), c.g(), c.b()]);
                }
            }
            (
                LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum),
                self.speed,
                self.brightness,
                self.direction,
                self.current_color,
                canvas_map,
                "Gaming".to_string(),
            )
        };
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() % 10000;
        let new_id = format!("{}_copy_{}", current_id, ts);
        let new_name = format!("{} (Copy)", orig_name);
        let new_profile = LightingProfile {
            id: new_id.clone(),
            name: new_name,
            description: format!("Cloned from {}", orig_name),
            mode,
            speed,
            brightness,
            direction,
            color,
            color_mode: Some(if self.is_fixed_color || self.selected_mode == 1 { ColorMode::FixedColor } else { ColorMode::DynamicSpectrum }),
            canvas,
            is_builtin: false,
        };
        let _ = self.profile_manager.save_lighting_profile(&new_profile);
        self.apply_lighting_profile_by_id(&new_id);
        self.status_message = format!("Cloned lighting profile '{}'", new_profile.name);
    }

    pub(crate) fn create_new_lighting_profile(&mut self, name: &str, copy_current: bool) {
        let (clean_name, new_id) = match Self::generate_profile_id(name) {
            Some(res) => res,
            None => {
                self.status_message = "Profile name cannot be empty".to_string();
                return;
            }
        };

        let (mode, speed, brightness, direction, color, color_mode, canvas) = if copy_current {
            let mut canvas_map = std::collections::HashMap::new();
            for (i, c) in self.key_colors.iter().enumerate() {
                if *c != Color32::BLACK {
                    canvas_map.insert(i, [c.r(), c.g(), c.b()]);
                }
            }
            (
                LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum),
                self.speed,
                self.brightness,
                self.direction,
                self.current_color,
                Some(if self.is_fixed_color || self.selected_mode == 1 { ColorMode::FixedColor } else { ColorMode::DynamicSpectrum }),
                canvas_map,
            )
        } else {
            (
                LightingMode::Spectrum,
                3,
                4,
                0,
                [255, 0, 0],
                Some(ColorMode::DynamicSpectrum),
                std::collections::HashMap::new(),
            )
        };

        let new_profile = LightingProfile {
            id: new_id.clone(),
            name: clean_name.to_string(),
            description: format!("Custom lighting profile: {}", clean_name),
            mode,
            speed,
            brightness,
            direction,
            color,
            color_mode,
            canvas,
            is_builtin: false,
        };

        match self.profile_manager.save_lighting_profile(&new_profile) {
            Ok(_) => {
                self.apply_lighting_profile_by_id(&new_id);
                self.status_message = format!("Created new lighting profile '{}'", clean_name);
            }
            Err(e) => {
                self.status_message = format!("Failed to create lighting profile '{}': {}", clean_name, e);
            }
        }
    }

    pub(crate) fn delete_lighting_profile_by_id(&mut self, id: &str) {
        if let Some(p) = self.profile_manager.load_lighting_profile(id) {
            if p.is_builtin || id == "gaming" || id == "rainbow_wave" {
                self.status_message = format!("Cannot delete built-in profile '{}'", p.name);
                return;
            }
            match self.profile_manager.delete_lighting_profile(id) {
                Ok(true) => {
                    if self.selected_lighting_profile == id {
                        self.apply_lighting_profile_by_id("gaming");
                    }
                    self.status_message = format!("Deleted lighting profile '{}'", p.name);
                }
                Ok(false) => {
                    self.status_message = format!("Profile '{}' was not found on disk", p.name);
                }
                Err(e) => {
                    self.status_message = format!("Error deleting profile '{}': {}", p.name, e);
                }
            }
        }
    }

    pub(crate) fn delete_active_lighting_profile(&mut self) {
        let current_id = self.selected_lighting_profile.clone();
        self.delete_lighting_profile_by_id(&current_id);
    }

    pub(crate) fn reset_active_lighting_profile(&mut self) {
        self.selected_mode = 4; // Spectrum
        self.brightness = 4;
        self.speed = 3;
        self.direction = 0;
        self.current_color = [255, 0, 0];
        self.key_colors.fill(Color32::BLACK);
        let current_id = self.selected_lighting_profile.clone();
        if let Some(mut p) = self.profile_manager.load_lighting_profile(&current_id) {
            if !p.is_builtin {
                p.mode = LightingMode::Spectrum;
                p.brightness = 4;
                p.speed = 3;
                p.direction = 0;
                p.color = [255, 0, 0];
                p.canvas.clear();
                let _ = self.profile_manager.save_lighting_profile(&p);
            }
        }
        self.apply_lighting();
        self.status_message = "Reset lighting profile to default Spectrum mode".to_string();
    }

    pub(crate) fn persist_current_lighting_profile(&mut self) {
        let current_id = self.selected_lighting_profile.clone();
        let mut canvas_map = std::collections::HashMap::new();
        for (i, c) in self.key_colors.iter().enumerate() {
            if *c != Color32::BLACK {
                canvas_map.insert(i, [c.r(), c.g(), c.b()]);
            }
        }
        let c_mode = if self.is_fixed_color || self.selected_mode == 1 {
            ColorMode::FixedColor
        } else {
            ColorMode::DynamicSpectrum
        };
        if let Some(mut p) = self.profile_manager.load_lighting_profile(&current_id) {
            p.mode = LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum);
            p.brightness = self.brightness;
            p.speed = self.speed;
            p.direction = self.direction;
            p.color = self.current_color;
            p.color_mode = Some(c_mode);
            p.canvas = canvas_map;
            let _ = self.profile_manager.save_lighting_profile(&p);
        } else {
            let p = LightingProfile {
                id: current_id.clone(),
                name: current_id.clone(),
                description: "Custom lighting profile".to_string(),
                mode: LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum),
                brightness: self.brightness,
                speed: self.speed,
                direction: self.direction,
                color: self.current_color,
                color_mode: Some(c_mode),
                canvas: canvas_map,
                is_builtin: false,
            };
            let _ = self.profile_manager.save_lighting_profile(&p);
        }
    }

    pub(crate) fn export_button_profile_to_path(&mut self, path: &std::path::Path) {
        let prof = (self.active_profile as usize).min(2);
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("button_profile");
        let id = name.trim().to_lowercase().replace(' ', "_");
        let button_profile = ButtonProfile {
            id: id.clone(),
            name: name.to_string(),
            description: format!("FantasTech exported button profile {}", name),
            base_layer: LayerKeymapConfig::from_descriptors(&self.profiles[prof][0], KeyLayer::Base),
            fn_layer: LayerKeymapConfig::from_descriptors(&self.profiles[prof][1], KeyLayer::Fn),
            is_builtin: false,
        };
        if self.export_profile_json(path, &button_profile, "button") {
            let _ = self.profile_manager.save_button_profile(&button_profile);
            self.selected_button_profile = id;
        }
    }

    pub(crate) fn import_any_profile_from_path(&mut self, path: &std::path::Path) -> bool {
        let json_str = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                self.status_message = format!("Failed to read profile file '{}': {}", path.display(), e);
                return false;
            }
        };

        // 1. Try parsing as LightingProfile
        if let Ok(lp) = serde_json::from_str::<LightingProfile>(&json_str) {
            self.selected_lighting_profile = lp.id.clone();
            self.selected_mode = lp.mode as u8;
            self.brightness = lp.brightness;
            self.speed = lp.speed;
            self.direction = lp.direction;
            self.current_color = lp.color;
            for (key_idx, rgb) in &lp.canvas {
                if *key_idx < SPARSE_KEY_COUNT {
                    self.key_colors[*key_idx] = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                }
            }
            let _ = self.profile_manager.save_lighting_profile(&lp);
            self.apply_lighting();
            self.status_message = format!("Loaded Lighting profile '{}' from '{}'", lp.name, path.display());
            return true;
        }

        // 2. Try parsing as ButtonProfile
        if let Ok(bp) = serde_json::from_str::<ButtonProfile>(&json_str) {
            self.undo_keymaps.clear();
            self.redo_keymaps.clear();
            let prof = (self.active_profile as usize).min(2);
            let mut valid = true;
            match bp.base_layer.to_descriptors() {
                Ok(base_actions) => self.profiles[prof][0] = base_actions,
                Err(e) => {
                    self.status_message = format!("Error loading Base layer from '{}': {}", path.display(), e);
                    valid = false;
                }
            }
            if valid {
                match bp.fn_layer.to_descriptors() {
                    Ok(fn_actions) => self.profiles[prof][1] = fn_actions,
                    Err(e) => {
                        self.status_message = format!("Error loading Fn layer from '{}': {}", path.display(), e);
                        valid = false;
                    }
                }
            }
            if valid {
                let _ = self.profile_manager.save_button_profile(&bp);
                self.selected_button_profile = bp.id.clone();
                self.status_message = format!("Loaded Button profile '{}' from '{}'", bp.name, path.display());
                return true;
            }
        }

        // 3. Try parsing as MacroProfile
        if let Ok(mp) = serde_json::from_str::<MacroProfile>(&json_str) {
            self.selected_macro_profile = mp.id.clone();
            if let Some(first) = mp.macros.first() {
                self.macro_sequence = first.clone();
                self.active_macro_slot = first.slot;
            }
            let _ = self.profile_manager.save_macro_profile(&mp);
            self.status_message = format!("Loaded Macro profile '{}' from '{}'", mp.name, path.display());
            return true;
        }

        // 4. Try parsing as FullDeviceBackup
        if let Ok(backup) = serde_json::from_str::<FullDeviceBackup>(&json_str) {
            self.selected_mode = backup.lighting_config.mode as u8;
            self.brightness = backup.lighting_config.brightness;
            self.speed = backup.lighting_config.speed;
            self.direction = backup.lighting_config.direction as u8;
            self.current_color = [backup.lighting_config.color.r, backup.lighting_config.color.g, backup.lighting_config.color.b];
            let prof = (self.active_profile as usize).min(2);
            for (idx, action) in backup.base_keymap.iter().enumerate().take(SPARSE_KEY_COUNT) {
                self.profiles[prof][0][idx] = *action;
            }
            for (idx, action) in backup.fn_keymap.iter().enumerate().take(SPARSE_KEY_COUNT) {
                self.profiles[prof][1][idx] = *action;
            }
            self.status_message = format!("Restored device backup from '{}'", path.display());
            return true;
        }

        self.status_message = format!("File '{}' is not a valid FantasTech profile (buttons, lighting, or macro)", path.display());
        false
    }

    pub(crate) fn import_button_profile_from_path(&mut self, path: &std::path::Path) {
        self.import_any_profile_from_path(path);
    }

    pub(crate) fn export_lighting_profile_to_path(&mut self, path: &std::path::Path) {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("lighting_profile");
        let id = name.trim().to_lowercase().replace(' ', "_");
        let mut canvas_map = std::collections::HashMap::new();
        for (i, c) in self.key_colors.iter().enumerate() {
            if *c != Color32::BLACK {
                canvas_map.insert(i, [c.r(), c.g(), c.b()]);
            }
        }
        let lp = LightingProfile {
            id: id.clone(),
            name: name.to_string(),
            description: format!("FantasTech exported lighting profile {}", name),
            mode: LightingMode::from_u8(self.selected_mode).unwrap_or(LightingMode::Spectrum),
            speed: self.speed,
            brightness: self.brightness,
            direction: self.direction,
            color: self.current_color,
            color_mode: Some(if self.is_fixed_color || self.selected_mode == 1 { ColorMode::FixedColor } else { ColorMode::DynamicSpectrum }),
            canvas: canvas_map,
            is_builtin: false,
        };
        if self.export_profile_json(path, &lp, "lighting") {
            let _ = self.profile_manager.save_lighting_profile(&lp);
            self.selected_lighting_profile = id;
        }
    }

    pub(crate) fn import_lighting_profile_from_path(&mut self, path: &std::path::Path) {
        self.import_any_profile_from_path(path);
    }

    pub(crate) fn export_macro_profile_to_path(&mut self, path: &std::path::Path) {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("macro_profile");
        let id = name.trim().to_lowercase().replace(' ', "_");
        let mp = MacroProfile {
            id: id.clone(),
            name: name.to_string(),
            description: format!("FantasTech exported macro profile {}", name),
            macros: vec![self.macro_sequence.clone()],
            is_builtin: false,
        };
        if self.export_profile_json(path, &mp, "macro") {
            self.selected_macro_profile = id;
        }
    }

    pub(crate) fn import_macro_profile_from_path(&mut self, path: &std::path::Path) {
        self.import_any_profile_from_path(path);
    }

    #[allow(dead_code)]
    pub(crate) fn apply_settings_profile_by_id(&mut self, id: &str) {
        if let Some(s) = self.profile_manager.load_settings_profile(id) {
            self.selected_settings_profile = s.id.clone();
            self.sleep_timeout_mins = s.sleep_timeout_mins;
            if self.check_service() {
                match send_ipc_request(&IpcRequest::ApplySettingsProfile { id: id.to_string() }) {
                    Ok(IpcResponse::Ok) => {
                        self.status_message = format!("Service (IPC): Applied settings profile '{}'", s.name);
                    }
                    Ok(resp) => {
                        self.status_message = format!("Service response: {:?}", resp);
                    }
                    Err(e) => {
                        self.status_message = format!("IPC error applying settings profile: {}", e);
                    }
                }
            } else {
                self.apply_sleep_timeout(s.sleep_timeout_mins);
            }
        } else {
            self.status_message = format!("Settings profile '{}' not found", id);
        }
    }

}
