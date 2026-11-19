//! Key remap, swap, and layer commands.


use crate::args::*;
use crate::parse::*;
use fantastech_core::*;
use fantastech_device::{Device, ReadOnly};
use fantastech_protocol::keymap::*;
use fantastech_transport::Transport;

pub(crate) fn handle_remap_command<T: Transport>(mut dev: Device<T, ReadOnly>, cli: &Cli, args: &RemapArgs) -> Result<(), Box<dyn std::error::Error>> {
    match &args.command {
        RemapCommands::Set { key, action, layer, commit } => {
            let key_index = parse_key_identifier(key)?;
            let l = parse_layer(layer)?;
            let act = parse_action_str(action)?;

            if !*commit && !cli.dry_run {
                println!("⚠️  PERSISTENT FLASH WRITE SAFETY GATE:");
                println!("   Remapping writes directly to onboard non-volatile MCU memory.");
                println!("   Proposed: Remap key '{}' (matrix {}) on {:?} layer to action {}", key, key_index, l, format_action_str(&act));
                println!("   To preview without writing, run with `--dry-run`.");
                println!("   To commit the persistent write, run with `--commit`.");
                return Ok(());
            }

            let mut persistent_dev = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
            persistent_dev.set_dry_run(cli.dry_run || !*commit);

            if let Some(record) = persistent_dev.remap_key_action(key_index, l, &act)? {
                println!("🔍 [DRY-RUN] Proposed Flash Write Packet: {:02X?}", &record.buffer[..12]);
                println!("   Action: {}", record.explanation);
            } else {
                println!("💾 Key '{}' (matrix index {}) on {:?} layer successfully remapped to {}", key, key_index, l, format_action_str(&act));
            }
        }
        RemapCommands::Get { key, layer } => {
            let key_index = parse_key_identifier(key)?;
            let l = parse_layer(layer)?;
            let chunk_idx = key_index / 14;
            let chunk = dev.read_keymap_chunk(chunk_idx, l)?;
            let act = chunk[(key_index % 14) as usize];
            println!("📋 Key Action Query:");
            println!("   Matrix Key:   '{}' (slot {})", key, key_index);
            println!("   Layer:        {:?}", l);
            println!("   Category:     {:?}", act.category);
            println!("   Action:       {}", format_action_str(&act));
            println!("   Raw Bytes:    [{:02X}, {:02X}, {:02X}, {:02X}]", act.category as u8, act.code1, act.code2, act.code3);
        }
        RemapCommands::Dump { layer, json } => {
            let l = parse_layer(layer)?;
            let full_layer = dev.read_keymap_layer(l)?;
            if *json {
                let config = LayerKeymapConfig::from_descriptors(&full_layer, l);
                let serialized = config.to_json().map_err(|e| format!("Failed to serialize JSON: {}", e))?;
                println!("{}", serialized);
            } else {
                println!("=== {:?} Keymap Table (126 matrix slots) ===", l);
                println!("{:<5} {:<15} {:<15} {:<25}", "Slot", "Key", "Category", "Action");
                println!("{:-<5} {:-<15} {:-<15} {:-<25}", "", "", "", "");
                for (idx, action) in full_layer.iter().enumerate() {
                    let key_name = KEY_DEFINITIONS.iter().find(|k| k.index == idx as u8).map(|k| k.name).unwrap_or("---");
                    let act_str = format_action_str(action);
                    let cat_str = format!("{:?}", action.category);
                    println!("{:<5} {:<15} {:<15} {:<25}", idx, key_name, cat_str, act_str);
                }
            }
        }
        RemapCommands::Reset { layer, commit } => {
            let l = parse_layer(layer)?;
            let default_actions = default_layer_keymap(l);

            if !*commit && !cli.dry_run {
                println!("⚠️  PERSISTENT FLASH WRITE SAFETY GATE:");
                println!("   Resetting {:?} keymap will overwrite all 126 slots with factory defaults.", l);
                println!("   To preview without writing, run with `--dry-run`.");
                println!("   To commit the persistent write, run with `--commit`.");
                return Ok(());
            }

            let mut persistent_dev = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
            persistent_dev.set_dry_run(cli.dry_run || !*commit);

            if let Some(records) = persistent_dev.write_keymap_layer(l, &default_actions)? {
                println!("🔍 [DRY-RUN] Proposed Reset Keymap Packets ({} packets)", records.len());
            } else {
                println!("🔄 {:?} layer keymap successfully reset to factory defaults.", l);
            }
        }
        RemapCommands::Export { file, layer } => {
            if let Some(l_str) = layer {
                let l = parse_layer(l_str)?;
                let actions = dev.read_keymap_layer(l)?;
                let config = LayerKeymapConfig::from_descriptors(&actions, l);
                let content = if file.extension().is_some_and(|ext| ext == "toml") {
                    config.to_toml().map_err(|e| format!("Failed to serialize TOML: {}", e))?
                } else {
                    config.to_json().map_err(|e| format!("Failed to serialize JSON: {}", e))?
                };
                std::fs::write(file, content)?;
                println!("✅ Exported {:?} layer keymap to {:?}", l, file);
            } else {
                let base_actions = dev.read_keymap_layer(KeyLayer::Base)?;
                let fn_actions = dev.read_keymap_layer(KeyLayer::Fn)?;
                let base_cfg = LayerKeymapConfig::from_descriptors(&base_actions, KeyLayer::Base);
                let fn_cfg = LayerKeymapConfig::from_descriptors(&fn_actions, KeyLayer::Fn);
                let prof_cfg = ProfileKeymapConfig {
                    profile_index: 0,
                    name: "Profile 1".to_string(),
                    base_layer: base_cfg,
                    fn_layer: fn_cfg,
                };
                let content = if file.extension().is_some_and(|ext| ext == "toml") {
                    prof_cfg.to_toml().map_err(|e| format!("Failed to serialize TOML: {}", e))?
                } else {
                    prof_cfg.to_json().map_err(|e| format!("Failed to serialize JSON: {}", e))?
                };
                std::fs::write(file, content)?;
                println!("✅ Exported full profile keymap (Base + Fn) to {:?}", file);
            }
        }
        RemapCommands::Import { file, layer, commit } => {
            let content = std::fs::read_to_string(file)?;
            let is_toml = file.extension().is_some_and(|ext| ext == "toml");
            if let Some(l_str) = layer {
                let l = parse_layer(l_str)?;
                let actions = if is_toml {
                    LayerKeymapConfig::from_toml(&content).map_err(|e| format!("Failed to parse TOML: {}", e))?.to_descriptors().map_err(|e| format!("{:?}", e))?
                } else {
                    LayerKeymapConfig::from_json(&content).map_err(|e| format!("Failed to parse JSON: {}", e))?.to_descriptors().map_err(|e| format!("{:?}", e))?
                };
                if !*commit && !cli.dry_run {
                    println!("⚠️  PERSISTENT FLASH WRITE SAFETY GATE:");
                    println!("   Importing keymap writes directly to onboard non-volatile MCU memory.");
                    println!("   To preview without writing, run with `--dry-run`.");
                    println!("   To commit the persistent write, run with `--commit`.");
                    return Ok(());
                }
                let mut persistent_dev = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
                persistent_dev.set_dry_run(cli.dry_run || !*commit);
                if let Some(records) = persistent_dev.write_keymap_layer(l, &actions)? {
                    println!("🔍 [DRY-RUN] Proposed Batch Keymap Packets ({} packets)", records.len());
                } else {
                    println!("✅ Imported and flashed {:?} layer keymap from {:?}", l, file);
                }
            } else {
                if let Ok(prof) = if is_toml { ProfileKeymapConfig::from_toml(&content) } else { ProfileKeymapConfig::from_json(&content) } {
                    if !*commit && !cli.dry_run {
                        println!("⚠️  PERSISTENT FLASH WRITE SAFETY GATE:");
                        println!("   Importing profile keymaps writes directly to onboard non-volatile MCU memory.");
                        println!("   To preview without writing, run with `--dry-run`.");
                        println!("   To commit the persistent write, run with `--commit`.");
                        return Ok(());
                    }
                    let mut persistent_dev = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
                    persistent_dev.set_dry_run(cli.dry_run || !*commit);
                    let base_actions = prof.base_layer.to_descriptors().map_err(|e| format!("{:?}", e))?;
                    let fn_actions = prof.fn_layer.to_descriptors().map_err(|e| format!("{:?}", e))?;
                    persistent_dev.write_keymap_layer(KeyLayer::Base, &base_actions)?;
                    persistent_dev.write_keymap_layer(KeyLayer::Fn, &fn_actions)?;
                    println!("✅ Imported and flashed full profile keymap (Base + Fn) from {:?}", file);
                } else if let Ok(layer_cfg) = if is_toml { LayerKeymapConfig::from_toml(&content) } else { LayerKeymapConfig::from_json(&content) } {
                    let l = layer_cfg.layer;
                    if !*commit && !cli.dry_run {
                        println!("⚠️  PERSISTENT FLASH WRITE SAFETY GATE:");
                        println!("   Importing keymap writes directly to onboard non-volatile MCU memory.");
                        println!("   To preview without writing, run with `--dry-run`.");
                        println!("   To commit the persistent write, run with `--commit`.");
                        return Ok(());
                    }
                    let mut persistent_dev = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
                    persistent_dev.set_dry_run(cli.dry_run || !*commit);
                    let actions = layer_cfg.to_descriptors().map_err(|e| format!("{:?}", e))?;
                    persistent_dev.write_keymap_layer(l, &actions)?;
                    println!("✅ Imported and flashed {:?} layer keymap from {:?}", l, file);
                } else {
                    return Err(format!("Could not parse {:?} as either LayerKeymapConfig or ProfileKeymapConfig", file).into());
                }
            }
        }
    }
    Ok(())
}
