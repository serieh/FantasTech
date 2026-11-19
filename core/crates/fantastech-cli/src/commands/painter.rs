//! Interactive terminal keyboard lighting painter.

use std::collections::HashMap;
use std::io::Write;
use crate::commands::light::apply_theme_to_keyboard;

use crate::args::*;
use crate::parse::*;
use fantastech_core::*;
use fantastech_device::Device;
use fantastech_protocol::constants::*;
use fantastech_protocol::lighting::*;
use fantastech_transport::Transport;

/// Interactive live key painter session with immediate physical LED updates.
pub(crate) fn run_interactive_key_painter<T: Transport>(
    dev: &mut Device<T, fantastech_device::ReversibleAccess>,
    _args: &CustomLightArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🎨 ================================================================");
    println!("🎨 Fantech MK894 Live Interactive Key Painter");
    println!("🎨 ================================================================");
    println!("Paint individual keys or zones live on your keyboard with real-time feedback.");
    println!();
    println!("Commands:");
    println!("  <key> <color>      Paint key (e.g. 'w red', 'esc #00FF55', 'space cyan')");
    println!("  zone <name> <col>  Paint zone (wasd, arrows, numpad, nav, fkeys, bottom)");
    println!("  row <0..5> <col>   Paint entire row (0=F-row, 1=Numbers, 2=QWERTY, 3=Home, 4=ZXCV, 5=Bottom)");
    println!("  all <color>        Flood whole keyboard with color");
    println!("  clear              Turn off all keys (blackout)");
    println!("  save <file.json>   Save current painted canvas to a JSON preset file");
    println!("  load <file.json>   Load and apply a JSON preset file");
    println!("  show               List currently customized keys");
    println!("  exit / quit / q    Exit interactive painter");
    println!("🎨 ================================================================");
    println!();

    let mut current_map: HashMap<usize, [u8; 3]> = HashMap::new();
    let stdin = std::io::stdin();

    loop {
        print!("painter> ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        match parts[0].to_lowercase().as_str() {
            "exit" | "quit" | "q" => {
                println!("👋 Exiting live painter. Your custom colors remain active on the keyboard!");
                break;
            }
            "clear" | "off" => {
                println!("🌑 Clearing all keys...");
                let all_off: Vec<(u8, RgbColor)> = (0..=MAX_KEY_INDEX as u8)
                    .map(|k| (k, RgbColor::new(0, 0, 0)))
                    .collect();
                dev.set_multiple_keys_color(&all_off)?;
                current_map.clear();
            }
            "all" => {
                if parts.len() < 2 {
                    println!("❌ Usage: all <color> (e.g. 'all blue', 'all #FF0055')");
                    continue;
                }
                match parse_color_spec(parts[1]) {
                    Ok(col) => {
                        println!("💡 Painting all keys to RGB({}, {}, {})...", col.r, col.g, col.b);
                        let all_keys: Vec<(u8, RgbColor)> = (0..=MAX_KEY_INDEX as u8)
                            .map(|k| (k, col))
                            .collect();
                        dev.set_multiple_keys_color(&all_keys)?;
                        for idx in 0..=MAX_KEY_INDEX {
                            current_map.insert(idx, [col.r, col.g, col.b]);
                        }
                    }
                    Err(e) => println!("❌ {}", e),
                }
            }
            "zone" => {
                if parts.len() < 3 {
                    println!("❌ Usage: zone <wasd|arrows|numpad|nav|fkeys|bottom> <color>");
                    continue;
                }
                let zone_name = parts[1];
                match parse_color_spec(parts[2]) {
                    Ok(col) => {
                        if let Some(keys) = fantastech_core::keys_for_zone(zone_name) {
                            let mut updates = Vec::new();
                            for k in &keys {
                                updates.push((k.index, col));
                                current_map.insert(k.index as usize, [col.r, col.g, col.b]);
                                if let Some(mirror) = get_mirror_key_index(k.index) {
                                    updates.push((mirror, col));
                                    current_map.insert(mirror as usize, [col.r, col.g, col.b]);
                                }
                            }
                            dev.set_multiple_keys_color(&updates)?;
                            println!("✨ Zone '{}' painted with {} keys.", zone_name, keys.len());
                        } else {
                            println!("❌ Unknown zone '{}'. Choose: wasd, arrows, numpad, nav, fkeys, bottom", zone_name);
                        }
                    }
                    Err(e) => println!("❌ {}", e),
                }
            }
            "row" => {
                if parts.len() < 3 {
                    println!("❌ Usage: row <0..5> <color>");
                    continue;
                }
                if let Ok(row_num) = parts[1].parse::<u8>() {
                    let keys = fantastech_core::keys_for_row(row_num);
                    if keys.is_empty() {
                        println!("❌ Invalid row {}. Please select 0..5.", row_num);
                        continue;
                    }
                    match parse_color_spec(parts[2]) {
                        Ok(col) => {
                            let mut updates = Vec::new();
                            for k in &keys {
                                updates.push((k.index, col));
                                current_map.insert(k.index as usize, [col.r, col.g, col.b]);
                                if let Some(mirror) = get_mirror_key_index(k.index) {
                                    updates.push((mirror, col));
                                    current_map.insert(mirror as usize, [col.r, col.g, col.b]);
                                }
                            }
                            dev.set_multiple_keys_color(&updates)?;
                            println!("✨ Row {} ({} keys) painted.", row_num, keys.len());
                        }
                        Err(e) => println!("❌ {}", e),
                    }
                } else {
                    println!("❌ Row must be a number 0..5.");
                }
            }
            "save" => {
                if parts.len() < 2 {
                    println!("❌ Usage: save <path.json>");
                    continue;
                }
                let path = parts[1];
                let theme = ThemeConfig {
                    name: format!("Custom Canvas ({})", path),
                    default_color: [0, 0, 0],
                    key_overrides: current_map.clone(),
                };
                if let Err(e) = theme.save_to_file(path) {
                    println!("❌ Failed saving theme: {}", e);
                } else {
                    println!("💾 Theme saved successfully to '{}' ({} keys)", path, current_map.len());
                }
            }
            "load" => {
                if parts.len() < 2 {
                    println!("❌ Usage: load <path.json>");
                    continue;
                }
                let path = parts[1];
                match ThemeConfig::load_from_file(path) {
                    Ok(theme) => {
                        apply_theme_to_keyboard(dev, &theme)?;
                        current_map = theme.key_overrides;
                        println!("📂 Loaded theme '{}' with {} key overrides.", theme.name, current_map.len());
                    }
                    Err(e) => println!("❌ Failed loading theme: {}", e),
                }
            }
            "show" => {
                println!("📋 Currently painted keys: {} keys", current_map.len());
                let mut sorted_keys: Vec<usize> = current_map.keys().copied().collect();
                sorted_keys.sort_unstable();
                for k in sorted_keys {
                    let name = KEY_DEFINITIONS.iter().find(|d| d.index as usize == k).map(|d| d.name).unwrap_or("Pad");
                    let rgb = current_map[&k];
                    println!("   Key {:<10} (idx {:<3}) -> RGB({}, {}, {})", name, k, rgb[0], rgb[1], rgb[2]);
                }
            }
            _ => {
                // Key and color syntax: "<key> <color>"
                if parts.len() >= 2 {
                    let key_str = parts[0];
                    let color_str = parts[1];
                    match (parse_keys_or_zone(key_str), parse_color_spec(color_str)) {
                        (Ok(indices), Ok(col)) => {
                            let mut updates = Vec::new();
                            for idx in indices {
                                updates.push((idx, col));
                                current_map.insert(idx as usize, [col.r, col.g, col.b]);
                                if let Some(mirror) = get_mirror_key_index(idx) {
                                    updates.push((mirror, col));
                                    current_map.insert(mirror as usize, [col.r, col.g, col.b]);
                                }
                            }
                            dev.set_multiple_keys_color(&updates)?;
                            println!("💡 Painted '{}' -> RGB({}, {}, {})", key_str, col.r, col.g, col.b);
                        }
                        (Err(e), _) => println!("❌ {}", e),
                        (_, Err(e)) => println!("❌ {}", e),
                    }
                } else {
                    println!("❌ Unknown command '{}'. Type 'exit' to quit or e.g. 'w red', 'zone wasd yellow'", trimmed);
                }
            }
        }
    }

    Ok(())
}
