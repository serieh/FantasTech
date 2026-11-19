//! Multi-domain profile commands (button, lighting, macro, settings).

use std::collections::HashMap;
use fantastech_protocol::keymap::KeyLayer;
use fantastech_protocol::lighting::{ColorMode, LightingMode};

use crate::args::*;
use crate::parse::*;
use fantastech_core::*;
use fantastech_device::{Device, ReadOnly};
use fantastech_transport::Transport;

pub(crate) fn handle_profile_command<T: Transport>(
    mut dev: Device<T, ReadOnly>,
    cli: &Cli,
    args: &ProfileArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match &args.command {
        Some(ProfileCommands::List { domain, profile_type }) => {
            let eff = profile_type.as_deref().or(domain.as_deref());
            return handle_profile_list(eff);
        }
        Some(ProfileCommands::Delete { arg1, arg2, profile_type }) => {
            let (dom, id) = resolve_profile_domain_and_id(arg1, arg2.as_deref(), profile_type.as_deref());
            return handle_profile_delete(&dom, &id);
        }
        Some(ProfileCommands::Save { domain, id, name, description, file, sleep }) => {
            return handle_profile_save(domain, id, name.as_deref(), description.as_deref(), file.as_deref(), *sleep);
        }
        Some(ProfileCommands::Apply { arg1, arg2, profile_type }) => {
            let (domain, id) = resolve_profile_domain_and_id(arg1, arg2.as_deref(), profile_type.as_deref());
            let pm = ProfileManager::new(None);
            let mut cfg = FantasTechConfig::load();
            if domain.eq_ignore_ascii_case("lighting") {
                let profile = pm
                    .load_lighting_profile(&id)
                    .ok_or_else(|| format!("Lighting profile '{}' not found", id))?;
                let mut rev_dev = dev.into_reversible();
                rev_dev.set_dry_run(cli.dry_run);
                if profile.mode == LightingMode::CustomMatrixCanvas {
                    let matrix = profile.to_matrix();
                    rev_dev.stream_matrix_canvas(&matrix)?;
                } else {
                    let c = profile.to_lighting_config();
                    rev_dev.set_lighting(&c)?;
                }
                cfg.active_lighting_profile = id.clone();
                let _ = cfg.save();
                println!("✨ Successfully applied lighting profile '{}'", id);
                return Ok(());
            } else if domain.eq_ignore_ascii_case("buttons") || domain.eq_ignore_ascii_case("button") {
                let profile = pm
                    .load_button_profile(&id)
                    .ok_or_else(|| format!("Button profile '{}' not found", id))?;
                let mut persistent_dev = dev
                    .into_reversible()
                    .into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
                persistent_dev.set_dry_run(cli.dry_run);
                let base_actions = profile.base_layer.to_descriptors().map_err(|e| e.to_string())?;
                let fn_actions = profile.fn_layer.to_descriptors().map_err(|e| e.to_string())?;
                persistent_dev.write_keymap_layer(KeyLayer::Base, &base_actions)?;
                persistent_dev.write_keymap_layer(KeyLayer::Fn, &fn_actions)?;
                cfg.active_button_profile = id.clone();
                let _ = cfg.save();
                println!("💾 Successfully applied button profile '{}'", id);
                return Ok(());
            } else if domain.eq_ignore_ascii_case("macros") || domain.eq_ignore_ascii_case("macro") {
                let profile = pm
                    .load_macro_profile(&id)
                    .ok_or_else(|| format!("Macro profile '{}' not found", id))?;
                let mut persistent_dev = dev
                    .into_reversible()
                    .into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
                persistent_dev.set_dry_run(cli.dry_run);
                let mut table = fantastech_protocol::macro_mem::MacroMemoryTable::new_empty();
                let mut offset = 0u16;
                for seq in &profile.macros {
                    offset = seq.compile_into_table(&mut table, offset).map_err(|e| e.to_string())?;
                }
                persistent_dev.write_macro_memory(table.as_bytes())?;
                cfg.active_macro_profile = id.clone();
                let _ = cfg.save();
                println!("⚡ Successfully applied macro profile '{}'", id);
                return Ok(());
            } else if domain.eq_ignore_ascii_case("settings") || domain.eq_ignore_ascii_case("setting") {
                let profile = pm
                    .load_settings_profile(&id)
                    .ok_or_else(|| format!("Settings profile '{}' not found", id))?;
                let mut persistent_dev = dev
                    .into_reversible()
                    .into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
                persistent_dev.set_dry_run(cli.dry_run);
                persistent_dev.set_sleep_timeout(profile.sleep_timeout_mins * 60)?;
                cfg.active_settings_profile = id.clone();
                let _ = cfg.save();
                println!("⚙️  Successfully applied settings profile '{}'", id);
                return Ok(());
            } else {
                return Err(format!(
                    "Unknown profile domain '{}'. Supported domains: lighting, buttons, macros, settings",
                    domain
                )
                .into());
            }
        }
        _ => {}
    }

    let target_index = match &args.command {
        Some(ProfileCommands::Select { index }) => Some(*index),
        Some(ProfileCommands::Current) => None,
        _ => args.index,
    };
    if args.current || target_index.is_none() {
        let info = dev.query_info()?;
        println!("📋 Active Hardware Profile:");
        println!("   Profile Slot:      Profile {}", info.active_profile);
        println!("   Total Slots:       {}", info.profile_count);
        println!("   Profile Name:      {}", info.profile_name);
        println!("   Firmware:          {}", info.firmware_version);
        return Ok(());
    }

    let Some(index) = target_index else {
        return Ok(());
    };
    let info = dev.query_info()?;
    if info.profile_count <= 1 && index > 0 {
        println!("ℹ️  Hardware Revision Information:");
        println!("   Your keyboard firmware reports {} active profile slot (Profile 0).", info.profile_count);
        println!("   This physical hardware revision (AK107 V1.00) uses a single onboard profile; Profile {} is not supported by MCU flash.", index);
        return Ok(());
    }

    let mut rev_dev = dev.into_reversible();
    rev_dev.set_dry_run(cli.dry_run);
    if let Some(record) = rev_dev.switch_profile(index)? {
        println!("🔍 [DRY-RUN] Proposed Packet: {:02X?}", &record.buffer[..4]);
        println!("   Action: {}", record.explanation);
    } else {
        println!("🔄 Switched active hardware profile to Profile {}", index);
    }
    Ok(())
}

pub(crate) fn handle_profile_save(
    domain: &str,
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    file: Option<&std::path::Path>,
    sleep: Option<u16>,
) -> Result<(), Box<dyn std::error::Error>> {
    let pm = ProfileManager::new(None);
    let p_name = name.unwrap_or(id).to_string();
    let p_desc = description.unwrap_or("Custom user profile").to_string();

    match domain.to_lowercase().as_str() {
        "lighting" => {
            let mut prof = if let Some(path) = file {
                let content = std::fs::read_to_string(path)?;
                serde_json::from_str::<LightingProfile>(&content)?
            } else {
                LightingProfile {
                    id: id.to_string(),
                    name: p_name.clone(),
                    description: p_desc.clone(),
                    mode: LightingMode::Static,
                    speed: 50,
                    brightness: 100,
                    direction: 0,
                    color: [0, 113, 207],
                    color_mode: Some(ColorMode::FixedColor),
                    canvas: HashMap::new(),
                    is_builtin: false,
                }
            };
            prof.id = id.to_string();
            prof.name = p_name;
            prof.description = p_desc;
            prof.is_builtin = false;
            pm.save_lighting_profile(&prof)?;
        }
        "buttons" | "button" => {
            let mut prof = if let Some(path) = file {
                let content = std::fs::read_to_string(path)?;
                serde_json::from_str::<ButtonProfile>(&content)?
            } else {
                ButtonProfile::default_ansi()
            };
            prof.id = id.to_string();
            prof.name = p_name;
            prof.description = p_desc;
            prof.is_builtin = false;
            pm.save_button_profile(&prof)?;
        }
        "macros" | "macro" => {
            let mut prof = if let Some(path) = file {
                let content = std::fs::read_to_string(path)?;
                serde_json::from_str::<MacroProfile>(&content)?
            } else {
                MacroProfile::default_profile()
            };
            prof.id = id.to_string();
            prof.name = p_name;
            prof.description = p_desc;
            prof.is_builtin = false;
            pm.save_macro_profile(&prof)?;
        }
        "settings" | "setting" => {
            let prof = SettingsProfile {
                id: id.to_string(),
                name: p_name,
                sleep_timeout_mins: sleep.unwrap_or(10),
                is_builtin: false,
            };
            pm.save_settings_profile(&prof)?;
        }
        _ => return Err(format!("Unknown domain '{}'. Supported: lighting, buttons, macros, settings", domain).into()),
    }
    println!("💾 Saved {} profile '{}'", domain, id);
    Ok(())
}

pub(crate) fn handle_profile_list(domain: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let pm = ProfileManager::new(None);
    let filter = domain.map(|s| s.to_lowercase());

    println!("============================================================");
    println!("               FantasTech Multi-Domain Profiles               ");
    println!("============================================================");

    if filter.is_none() || filter.as_deref() == Some("lighting") {
        println!("\n🎨 Lighting Profiles (domain: lighting):");
        for p in pm.list_lighting_profiles() {
            println!("  • {:<16} - {} [{:?}]", p.id, p.name, p.mode);
        }
    }

    if filter.is_none() || filter.as_deref() == Some("buttons") || filter.as_deref() == Some("button") {
        println!("\n⌨️  Button / Keymap Profiles (domain: buttons):");
        for p in pm.list_button_profiles() {
            println!("  • {:<16} - {}", p.id, p.name);
        }
    }

    if filter.is_none() || filter.as_deref() == Some("macros") || filter.as_deref() == Some("macro") {
        println!("\n⚡ Macro Profiles (domain: macros):");
        for p in pm.list_macro_profiles() {
            println!("  • {:<16} - {} ({} sequences)", p.id, p.name, p.macros.len());
        }
    }

    if filter.is_none() || filter.as_deref() == Some("settings") || filter.as_deref() == Some("setting") {
        println!("\n⚙️  Settings Profiles (domain: settings):");
        for p in pm.list_settings_profiles() {
            println!("  • {:<16} - {} (sleep: {}m)", p.id, p.name, p.sleep_timeout_mins);
        }
    }
    println!("============================================================");
    Ok(())
}

pub(crate) fn handle_profile_delete(domain: &str, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let pm = ProfileManager::new(None);
    match domain.to_lowercase().as_str() {
        "lighting" => { pm.delete_lighting_profile(id)?; }
        "buttons" | "button" => { pm.delete_button_profile(id)?; }
        "macros" | "macro" => { pm.delete_macro_profile(id)?; }
        "settings" | "setting" => { pm.delete_settings_profile(id)?; }
        _ => return Err(format!("Unknown domain '{}'. Use lighting, buttons, macros, or settings.", domain).into()),
    }
    println!("🗑️  Deleted {} profile '{}'", domain, id);
    Ok(())
}
