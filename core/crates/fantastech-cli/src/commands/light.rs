//! Lighting commands, presets, and direct matrix painting.

use std::collections::HashMap;
use std::io::Write;
use crate::commands::painter::run_interactive_key_painter;

use crate::args::*;
use crate::parse::*;
use fantastech_core::*;
use fantastech_device::Device;
use fantastech_protocol::constants::*;
use fantastech_protocol::lighting::*;
use fantastech_transport::Transport;

/// Applies a full `ThemeConfig` to the keyboard using differential matrix caching and dual-pad synchronization.
pub(crate) fn apply_theme_to_keyboard<T: Transport>(
    dev: &mut Device<T, fantastech_device::ReversibleAccess>,
    theme: &ThemeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🎨 Applying theme preset '{}' across keyboard...", theme.name);

    let bg_color = RgbColor::new(theme.default_color[0], theme.default_color[1], theme.default_color[2]);

    let mut updates: Vec<(u8, RgbColor)> = Vec::with_capacity(126);
    for idx in 0..=MAX_KEY_INDEX as u8 {
        let mirror_opt = fantastech_core::get_mirror_key_index(idx);
        let color = if let Some(&rgb) = theme.key_overrides.get(&(idx as usize)) {
            RgbColor::new(rgb[0], rgb[1], rgb[2])
        } else if let Some(m_idx) = mirror_opt.filter(|&m| theme.key_overrides.contains_key(&(m as usize))) {
            let rgb = theme.key_overrides[&(m_idx as usize)];
            RgbColor::new(rgb[0], rgb[1], rgb[2])
        } else if KEY_DEFINITIONS.iter().any(|k| k.index == idx)
            || mirror_opt.is_some_and(|m| KEY_DEFINITIONS.iter().any(|k| k.index == m)) {
            bg_color
        } else {
            RgbColor::new(0, 0, 0)
        };
        updates.push((idx, color));
    }

    print!("⏳ Applying per-key RGB across keyboard (Opcode 0x14)...");
    let _ = std::io::stdout().flush();
    dev.set_multiple_keys_color(&updates)?;
    println!("\r✨ Preset '{}' successfully applied across all keys!           ", theme.name);
    Ok(())
}

/// Handles `fantastech light custom` subcommands (interactive painter, JSON save/load, CLI batch set).
pub(crate) fn handle_custom_lighting<T: Transport>(
    dev: &mut Device<T, fantastech_device::ReversibleAccess>,
    args: &CustomLightArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(ref path) = args.load {
        println!("📂 Loading custom theme from {:?}", path);
        let theme = ThemeConfig::load_from_file(path)?;
        apply_theme_to_keyboard(dev, &theme)?;
        return Ok(());
    }

    if let Some(ref set_spec) = args.set {
        let bg_color = parse_color_spec(&args.default_color)?;
        let mut key_overrides: HashMap<usize, [u8; 3]> = HashMap::new();

        for item in set_spec.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            if let Some((key_part, color_part)) = item.split_once('=') {
                let color = parse_color_spec(color_part.trim())?;
                let indices = parse_keys_or_zone(key_part.trim())?;
                for idx in indices {
                    key_overrides.insert(idx as usize, [color.r, color.g, color.b]);
                    if let Some(mirror) = get_mirror_key_index(idx) {
                        key_overrides.insert(mirror as usize, [color.r, color.g, color.b]);
                    }
                }
            } else {
                return Err(format!("Invalid key assignment '{}'. Expected format 'key=color', e.g. 'wasd=red'", item).into());
            }
        }

        let theme = ThemeConfig {
            name: "CLI Custom Batch".to_string(),
            default_color: [bg_color.r, bg_color.g, bg_color.b],
            key_overrides,
        };

        if let Some(ref save_path) = args.save {
            theme.save_to_file(save_path)?;
            println!("💾 Custom theme saved to {:?}", save_path);
        }

        apply_theme_to_keyboard(dev, &theme)?;
        return Ok(());
    }

    if args.interactive {
        run_interactive_key_painter(dev, args)?;
        return Ok(());
    }

    println!("🎨 Custom Mode Options:");
    println!("  1. Interactive Live Painter:   fantastech light custom --interactive");
    println!("  2. Direct Batch Assignment:    fantastech light custom --set 'wasd=red,arrows=blue,space=green'");
    println!("  3. Save Preset to File:        fantastech light custom --set 'wasd=red' --save mytheme.json");
    println!("  4. Load Preset from File:      fantastech light custom --load mytheme.json");

    Ok(())
}
