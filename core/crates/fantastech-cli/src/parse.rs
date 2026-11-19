//! String parsing, color parsing, and domain resolution utilities.

use fantastech_protocol::keymap::KeyLayer;
use fantastech_protocol::lighting::{AnimationDirection, LightingMode, RgbColor};
use fantastech_core::parse_key_identifier;

/// Formats and prints a comprehensive table of all 17 lighting modes.
pub(crate) fn print_modes_table() {
    println!("💡 ==========================================================================");
    println!("💡 Fantech MK894 Hardware Lighting Modes (Clean-Room Verified)");
    println!("💡 ==========================================================================");
    println!(" {:<4} | {:<20} | {:<16} | {:<8} | {:<10}", "ID", "Name", "Color Support", "Speed", "Brightness");
    println!("------+----------------------+------------------+----------+------------");

    for m in LightingMode::all() {
        let id = *m as u8;
        let name = m.name();
        let color_sup = if *m == LightingMode::Static {
            "Fixed Color (RGB)"
        } else if m.supports_color() {
            "Dynamic / Fixed"
        } else if id == 16 {
            "Per-Key Full RGB"
        } else if id == 0 {
            "None"
        } else {
            "Dynamic Spectrum"
        };
        let speed_sup = if m.supports_speed() { "Yes (0..4)" } else { "No" };
        let bright_sup = if id == 0 { "No" } else { "Yes (0..4)" };

        println!(" {:<4} | {:<20} | {:<16} | {:<8} | {:<10}", id, name, color_sup, speed_sup, bright_sup);
    }
    println!("💡 ==========================================================================");
    println!("Example Commands:");
    println!("  fantastech light set-mode static --hex #FF0000 --brightness 4");
    println!("  fantastech light set-mode wave --speed 3");
    println!("  fantastech light set-mode breathing --hex #00FF55 --speed 2");
    println!("  fantastech light set-mode custom");
}

/// Normalizes 0..100 or 0..4 values to hardware 0..4 range.
pub(crate) fn normalize_level(input: u8) -> u8 {
    if input > 4 {
        // Percentage scale 0..100 -> 0..4
        ((input as u16 * 4) / 100).min(4) as u8
    } else {
        input
    }
}

/// Parses lighting mode from string (numeric ID or human-friendly name).
pub(crate) fn parse_lighting_mode(input: &str) -> Result<LightingMode, Box<dyn std::error::Error>> {
    if let Ok(id) = input.parse::<u8>() {
        return Ok(LightingMode::from_u8(id)?);
    }
    input.parse::<LightingMode>().map_err(|e| e.into())
}

/// Parses animation direction string.
pub(crate) fn parse_direction(input: &str) -> Result<AnimationDirection, Box<dyn std::error::Error>> {
    input.parse::<AnimationDirection>().map_err(|e| e.into())
}

/// Resolves alternate electrical pad index for edge switches with dual solder positions.
pub use fantastech_core::get_mirror_key_index;

/// Parses key identifier or functional zone name, returning all matching physical matrix indices.
pub(crate) fn parse_keys_or_zone(input: &str) -> Result<Vec<u8>, String> {
    let lower = input.to_lowercase();
    if let Some(keys) = fantastech_core::keys_for_zone(&lower) {
        return Ok(keys.iter().map(|k| k.index).collect());
    }
    let single = parse_key_identifier(input).map_err(|e| e.to_string())?;
    Ok(vec![single])
}

/// Parses color specification from name, hex, or comma/space-delimited RGB.
pub(crate) fn parse_color_spec(input: &str) -> Result<RgbColor, String> {
    let trimmed = input.trim();
    if trimmed.starts_with('#') {
        return RgbColor::from_hex(trimmed).ok_or_else(|| format!("Invalid hex color: {}", trimmed));
    }
    match trimmed.to_lowercase().as_str() {
        "red" => Ok(RgbColor::new(255, 0, 0)),
        "green" => Ok(RgbColor::new(0, 255, 0)),
        "blue" => Ok(RgbColor::new(0, 0, 255)),
        "white" => Ok(RgbColor::new(255, 255, 255)),
        "off" | "black" => Ok(RgbColor::new(0, 0, 0)),
        "yellow" => Ok(RgbColor::new(255, 255, 0)),
        "cyan" => Ok(RgbColor::new(0, 255, 255)),
        "magenta" | "purple" => Ok(RgbColor::new(255, 0, 255)),
        "orange" => Ok(RgbColor::new(255, 128, 0)),
        "pink" => Ok(RgbColor::new(255, 105, 180)),
        _ => {
            // Check comma separated "255,0,0" or space separated "255 0 0"
            let parts: Vec<&str> = if trimmed.contains(',') {
                trimmed.split(',').map(|s| s.trim()).collect()
            } else {
                trimmed.split_whitespace().collect()
            };

            if parts.len() == 3 {
                let r: u8 = parts[0].parse().map_err(|_| format!("Invalid red value: {}", parts[0]))?;
                let g: u8 = parts[1].parse().map_err(|_| format!("Invalid green value: {}", parts[1]))?;
                let b: u8 = parts[2].parse().map_err(|_| format!("Invalid blue value: {}", parts[2]))?;
                Ok(RgbColor::new(r, g, b))
            } else {
                // Try hex without '#'
                if (trimmed.len() == 6 || trimmed.len() == 3) && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                    RgbColor::from_hex(&format!("#{}", trimmed)).ok_or_else(|| format!("Invalid hex color: {}", trimmed))
                } else {
                    Err(format!("Unknown color specification '{}'. Use #RRGGBB, 'red', 'blue', or '255,0,0'", input))
                }
            }
        }
    }
}

pub(crate) fn parse_layer(s: &str) -> Result<KeyLayer, String> {
    match s.trim().to_lowercase().as_str() {
        "0" | "base" | "main" | "default" => Ok(KeyLayer::Base),
        "1" | "fn" | "function" => Ok(KeyLayer::Fn),
        _ => Err(format!("Invalid layer '{}'. Expected 'base' ('0') or 'fn' ('1')", s)),
    }
}

pub(crate) fn resolve_profile_domain_and_id(arg1: &str, arg2: Option<&str>, profile_type: Option<&str>) -> (String, String) {
    if let Some(t) = profile_type {
        (t.to_string(), arg1.to_string())
    } else if let Some(id) = arg2 {
        (arg1.to_string(), id.to_string())
    } else {
        let lower = arg1.to_lowercase();
        if lower == "gaming" || lower == "cyberpunk" || lower == "rainbow_wave" || lower == "neon_sunset" || lower == "matrix" || lower == "static_purple" {
            ("lighting".to_string(), arg1.to_string())
        } else if lower == "gaming_lock" || lower == "default_ansi" {
            ("buttons".to_string(), arg1.to_string())
        } else if lower == "never_sleep" {
            ("settings".to_string(), arg1.to_string())
        } else {
            ("lighting".to_string(), arg1.to_string())
        }
    }
}
