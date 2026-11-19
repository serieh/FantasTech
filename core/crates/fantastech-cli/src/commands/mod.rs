//! Domain command dispatchers and implementations.


use std::io::Write;

pub mod backup;
pub mod firmware;
pub mod gui_launch;
pub mod light;
pub mod macro_cmd;
pub mod openrgb;
pub mod painter;
pub mod profile;
pub mod remap;
pub mod setting;
pub mod system;

pub(crate) use backup::*;
pub(crate) use firmware::*;
pub(crate) use gui_launch::*;
pub(crate) use light::*;
pub(crate) use macro_cmd::*;
pub(crate) use openrgb::*;
pub(crate) use profile::*;
pub(crate) use remap::*;
pub(crate) use setting::*;
pub(crate) use system::*;

use crate::args::*;
use crate::parse::*;
use fantastech_core::*;
use fantastech_device::Device;
use fantastech_protocol::constants::*;
use fantastech_protocol::lighting::*;
use fantastech_transport::Transport;

pub fn run_command<T: Transport + 'static>(transport: T, cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let mut dev = Device::new_read_only(transport);
    let cmd = match &cli.command {
        Some(cmd) => cmd,
        None => return Ok(()),
    };

    match cmd {
        Commands::Info => {
            println!("📋 Querying device identification...");
            let info = dev.query_info()?;
            println!("==========================================");
            println!("  Hardware Profile:  {}", info.profile_name);
            println!("  Product ID (PID):  0x{:04X}", info.pid);
            println!("  Firmware Version:  {}", info.firmware_version);
            println!("  Active Profile:    Profile {}", info.active_profile);
            println!("  Total Profiles:    {}", info.profile_count);
            println!("  Battery Level:     {}%", info.battery);
            println!("  Serial Number:     {}", if info.serial_number.is_empty() { "N/A" } else { &info.serial_number });
            println!("==========================================");
        }
        Commands::Profile(args) => {
            handle_profile_command(dev, cli, args)?;
        }
        Commands::OpenRgbServer(args) => {
            let mut rev_dev = dev.into_reversible();
            rev_dev.set_dry_run(cli.dry_run);
            run_openrgb_server(rev_dev, &args.bind, args.port)?;
        }
        Commands::SetupUdev(args) => {
            handle_setup_udev(args)?;
        }
        Commands::SetupSystemd(args) => {
            handle_setup_systemd(args)?;
        }
        Commands::KdeSync(args) => {
            let mut rev_dev = dev.into_reversible();
            rev_dev.set_dry_run(cli.dry_run);
            handle_kde_sync(&mut rev_dev, args)?;
        }
        Commands::SetupKde(args) => {
            handle_setup_kde(args)?;
        }
        Commands::Light(args) => {
            match &args.command {
                LightCommands::ListModes => {
                    print_modes_table();
                }
                LightCommands::Status => {
                    println!("💡 Querying active lighting configuration from keyboard...");
                    let config = dev.query_light_config()?;
                    println!("==========================================");
                    println!("  Lighting Zone:     {:?}", config.zone);
                    println!("  Lighting Mode:     {:?} (Index {})", config.mode, config.mode as u8);
                    println!("  Brightness Level:  {} / 4", config.brightness);
                    println!("  Speed Level:       {} / 4", config.speed);
                    println!("  Color Mode:        {:?}", config.color_mode);
                    println!("  Current Color:     RGB({}, {}, {})", config.color.r, config.color.g, config.color.b);
                    println!("==========================================");
                }
                LightCommands::SetMode {
                    mode,
                    brightness,
                    speed,
                    direction,
                    color,
                    hex,
                    spectrum,
                } => {
                    let mut rev_dev = dev.into_reversible();
                    rev_dev.set_dry_run(cli.dry_run);

                    let lighting_mode = parse_lighting_mode(mode)?;
                    let norm_brightness = normalize_level(*brightness);
                    let norm_speed = normalize_level(*speed);
                    let anim_direction = parse_direction(direction)?;

                    let (color_mode, rgb) = if *spectrum {
                        (ColorMode::DynamicSpectrum, RgbColor::new(255, 0, 0))
                    } else if let Some(ref h) = hex {
                        (ColorMode::FixedColor, RgbColor::from_hex(h).ok_or("Invalid hex color")?)
                    } else if let Some(ref c) = color {
                        let parsed = parse_color_spec(&c.join(" "))?;
                        (ColorMode::FixedColor, parsed)
                    } else if lighting_mode.supports_color() {
                        (ColorMode::FixedColor, RgbColor::new(255, 0, 0))
                    } else {
                        (ColorMode::DynamicSpectrum, RgbColor::new(255, 0, 0))
                    };

                    let config = LightingConfig {
                        zone: LightingZone::MainBacklight,
                        mode: lighting_mode,
                        brightness: norm_brightness,
                        speed: norm_speed,
                        direction: anim_direction,
                        color_mode,
                        color: rgb,
                    };

                    if let Some(record) = rev_dev.set_lighting(&config)? {
                        println!("🔍 [DRY-RUN] Proposed Packet: {:02X?}", &record.buffer[..16]);
                        println!("   Action: {}", record.explanation);
                    } else {
                        println!(
                            "✨ Applied lighting mode '{:?}' (mode={}, brightness={}/4, speed={}/4)",
                            lighting_mode, lighting_mode as u8, norm_brightness, norm_speed
                        );
                    }
                }
                LightCommands::Preset { name } => {
                    let mut rev_dev = dev.into_reversible();
                    rev_dev.set_dry_run(cli.dry_run);

                    let theme = match name.to_lowercase().as_str() {
                        "cyberpunk" => ThemeConfig::cyberpunk(),
                        "gaming" | "contrast" => ThemeConfig::gaming(),
                        _ => ThemeConfig::default(),
                    };

                    apply_theme_to_keyboard(&mut rev_dev, &theme)?;
                }
                LightCommands::SetKey {
                    key,
                    r,
                    g,
                    b,
                    hex,
                    color,
                } => {
                    let key_index = parse_key_identifier(key)?;
                    let color = if let (Some(r_val), Some(g_val), Some(b_val)) = (*r, *g, *b) {
                        RgbColor::new(r_val, g_val, b_val)
                    } else if let Some(ref h) = hex {
                        RgbColor::from_hex(h).ok_or("Invalid hex color")?
                    } else if let Some(ref c) = color {
                        parse_color_spec(c)?
                    } else {
                        return Err("Please specify a color: `--hex #FF0000`, `-c red`, or `R G B`".into());
                    };

                    let mut rev_dev = dev.into_reversible();
                    rev_dev.set_dry_run(cli.dry_run);

                    if let Some(record) = rev_dev.set_single_key_color(key_index, color)? {
                        println!("🔍 [DRY-RUN] Proposed Packet: {:02X?}", &record.buffer[..10]);
                        println!("   Action: {}", record.explanation);
                    } else {
                        println!("💡 Set key '{}' (matrix index {}) LED to RGB({}, {}, {})", key, key_index, color.r, color.g, color.b);
                        // Also update dual-pad mirror if applicable
                        if let Some(mirror) = get_mirror_key_index(key_index) {
                            rev_dev.set_single_key_color(mirror, color)?;
                        }
                    }
                }
                LightCommands::Custom(custom_args) => {
                    let mut rev_dev = dev.into_reversible();
                    rev_dev.set_dry_run(cli.dry_run);

                    handle_custom_lighting(&mut rev_dev, custom_args)?;
                }
                LightCommands::Off => {
                    let mut rev_dev = dev.into_reversible();
                    rev_dev.set_dry_run(cli.dry_run);
                    let off_cfg = LightingConfig {
                        zone: LightingZone::MainBacklight,
                        mode: LightingMode::Off,
                        brightness: 0,
                        speed: 0,
                        direction: AnimationDirection::Default,
                        color_mode: ColorMode::DynamicSpectrum,
                        color: RgbColor::new(0, 0, 0),
                    };
                    if let Some(record) = rev_dev.set_lighting(&off_cfg)? {
                        println!("🔍 [DRY-RUN] Proposed Packet: {:02X?}", &record.buffer[..16]);
                        println!("   Action: {}", record.explanation);
                    } else {
                        println!("🌑 Backlighting turned OFF.");
                    }
                }
                LightCommands::Profile { id } => {
                    let mut rev_dev = dev.into_reversible();
                    rev_dev.set_dry_run(cli.dry_run);
                    let pm = ProfileManager::new(None);
                    let profile = pm.load_lighting_profile(id)
                        .ok_or_else(|| format!("Lighting profile '{}' not found", id))?;
                    if profile.mode == LightingMode::CustomMatrixCanvas {
                        let matrix = profile.to_matrix();
                        if let Some(record) = rev_dev.stream_matrix_canvas(&matrix)? {
                            println!("🔍 [DRY-RUN] Proposed Packet: {:02X?}", &record[0].buffer[..16]);
                        } else {
                            println!("✨ Applied custom canvas lighting profile '{}'", id);
                        }
                    } else {
                        let cfg = profile.to_lighting_config();
                        if let Some(record) = rev_dev.set_lighting(&cfg)? {
                            println!("🔍 [DRY-RUN] Proposed Packet: {:02X?}", &record.buffer[..16]);
                            println!("   Action: {}", record.explanation);
                        } else {
                            println!("✨ Applied lighting profile '{}'", id);
                        }
                    }
                }
            }
        }
        Commands::Remap(args) => {
            handle_remap_command(dev, cli, args)?;
        }
        Commands::Macro(args) => {
            handle_macro_command(dev, cli, args)?;
        }
        Commands::Backup(args) => {
            handle_backup_command(dev, cli, args)?;
        }
        Commands::Setting(args) => {
            handle_setting_command(dev, cli, args)?;
        }
        Commands::Firmware(args) => {
            handle_firmware_command(dev, cli, args)?;
        }
        Commands::GuardVolume(args) => {
            handle_guard_volume(args)?;
        }
        Commands::SetupVolumeGuard(args) => {
            handle_setup_volume_guard(args)?;
        }
        Commands::TestKeys(args) => {
            let mut rev_dev = dev.into_reversible();
            rev_dev.set_dry_run(cli.dry_run);

            let test_color = RgbColor::new(args.color[0], args.color[1], args.color[2]);
            let off_color = RgbColor::new(0, 0, 0);

            if args.all_off {
                print!("🌑 Turning off all keyboard LEDs...");
                let _ = std::io::stdout().flush();
                let all_off: Vec<(u8, RgbColor)> = (0..=MAX_KEY_INDEX as u8)
                    .map(|k| (k, off_color))
                    .collect();
                rev_dev.set_multiple_keys_color(&all_off)?;
                println!("\r🌑 All keyboard LEDs turned OFF.                                ");
                return Ok(());
            }

            if args.all_on {
                let on_config = LightingConfig {
                    mode: LightingMode::Static,
                    brightness: 4,
                    speed: 2,
                    direction: AnimationDirection::Default,
                    color_mode: ColorMode::FixedColor,
                    color: test_color,
                    ..Default::default()
                };
                rev_dev.set_lighting(&on_config)?;
                println!("💡 All keyboard LEDs turned ON with RGB({}, {}, {}).", test_color.r, test_color.g, test_color.b);
                return Ok(());
            }

            if let Some(ref key_str) = args.key {
                let key_index = parse_key_identifier(key_str)?;
                let entry = KEY_DEFINITIONS.iter().find(|k| k.index == key_index);
                let name = entry.map(|k| k.name).unwrap_or(key_str.as_str());
                let row_info = entry.map(|k| format!("(Row {}, Col {})", k.row, k.col)).unwrap_or_default();

                if !args.no_blackout {
                    print!("🌑 Turning off all other keys and isolating '{}' (index {})...", name, key_index);
                    let _ = std::io::stdout().flush();
                    let mut updates: Vec<(u8, RgbColor)> = (0..=MAX_KEY_INDEX as u8)
                        .filter(|&k| k != key_index)
                        .map(|k| (k, off_color))
                        .collect();
                    updates.push((key_index, test_color));
                    rev_dev.set_multiple_keys_color(&updates)?;
                    println!("\r✨ Target key isolated: ONLY '{}' is ON, all other keys are OFF.     ", name);
                } else {
                    rev_dev.set_single_key_color(key_index, test_color)?;
                }

                println!("🔍 Probing Key: '{}' (Matrix Index {}) {}", name, key_index, row_info);
                println!("💡 ONLY this key is now glowing on your physical keyboard.");

                println!("👉 Press [Enter] to turn off and exit...");
                let mut dummy = String::new();
                let _ = std::io::stdin().read_line(&mut dummy);

                let all_off: Vec<(u8, RgbColor)> = (0..=MAX_KEY_INDEX as u8)
                    .map(|k| (k, off_color))
                    .collect();
                rev_dev.set_multiple_keys_color(&all_off)?;
                println!("🌑 Key '{}' turned off.", name);
                return Ok(());
            }

            if args.interactive {
                println!("🎹 Fantech MK894 Interactive Key Matrix Verification Wizard");
                println!("============================================================");
                println!("This wizard tests each key one by one. ONLY the active key will glow.");
                println!();
                println!("Controls at each prompt:");
                println!("  [Enter]  -> Confirms the key matches the expected name");
                println!("  <name>   -> Re-assigns the key name (e.g., 'F1', 'Esc', 'Space')");
                println!("  's'      -> Skips this key");
                println!("  'q'      -> Saves current verified keys and exits");
                println!("============================================================");
                println!();

                if !args.no_blackout {
                    print!("🌑 Turning off all keys before wizard...");
                    let _ = std::io::stdout().flush();
                    let all_off: Vec<(u8, RgbColor)> = KEY_DEFINITIONS.iter().map(|k| (k.index, off_color)).collect();
                    rev_dev.set_multiple_keys_color(&all_off)?;
                    println!("\r✨ All keys are now OFF. Starting wizard...                        ");
                }

                let stdin = std::io::stdin();
                let mut verified: Vec<(&str, u8, &str)> = Vec::new();
                let keys = &KEY_DEFINITIONS[args.start.min(KEY_DEFINITIONS.len())..];

                let mut prev_index: Option<u8> = None;

                for (offset, key) in keys.iter().enumerate() {
                    let num = args.start + offset + 1;

                    if let Some(prev) = prev_index {
                        rev_dev.set_single_key_color(prev, off_color)?;
                    }

                    rev_dev.set_single_key_color(key.index, test_color)?;
                    prev_index = Some(key.index);

                    print!("[{}/{}] Index {:<3} (Expected: '{:<10}' Row {} Col {}). [Enter=OK / Name / s / q]: ",
                        num,
                        KEY_DEFINITIONS.len(),
                        key.index,
                        key.name,
                        key.row,
                        key.col
                    );
                    std::io::stdout().flush()?;

                    let mut input = String::new();
                    stdin.read_line(&mut input)?;
                    let trimmed = input.trim();

                    if trimmed.eq_ignore_ascii_case("q") {
                        rev_dev.set_single_key_color(key.index, off_color)?;
                        println!("⏹️  Interactive test stopped by user.");
                        break;
                    } else if trimmed.eq_ignore_ascii_case("s") {
                        println!("⏭️  Skipped index {}.", key.index);
                    } else if trimmed.is_empty() {
                        println!("✅ Confirmed: Index {} is '{}'", key.index, key.name);
                        verified.push((key.name, key.index, key.name));
                    } else {
                        println!("✏️  Corrected: Index {} mapped to '{}'", key.index, trimmed);
                        verified.push((key.name, key.index, Box::leak(trimmed.to_string().into_boxed_str())));
                    }
                }

                if let Some(last) = prev_index {
                    rev_dev.set_single_key_color(last, off_color)?;
                }

                println!("\n📋 Verified Key Count: {} / {}", verified.len(), KEY_DEFINITIONS.len());
                if !verified.is_empty() {
                    let mut json_str = String::from("[\n");
                    for (i, (exp, idx, actual)) in verified.iter().enumerate() {
                        let esc_exp = exp.replace('\\', "\\\\").replace('"', "\\\"");
                        let esc_act = actual.replace('\\', "\\\\").replace('"', "\\\"");
                        json_str.push_str(&format!(
                            "  {{\"expected\": \"{}\", \"index\": {}, \"actual\": \"{}\"}}{}\n",
                            esc_exp, idx, esc_act, if i + 1 < verified.len() { "," } else { "" }
                        ));
                    }
                    json_str.push_str("]\n");
                    let _ = std::fs::write("keymap_verified.json", json_str);
                    println!("💾 Saved verified key map to 'keymap_verified.json'");
                }
                return Ok(());
            }

            if let Some(row) = args.row {
                let keys = fantastech_core::keys_for_row(row);
                if keys.is_empty() {
                    println!("❌ Invalid row {}. Please choose 0..5.", row);
                    return Ok(());
                }

                if !args.no_blackout {
                    print!("🌑 Turning off other keys...");
                    let _ = std::io::stdout().flush();
                    let target_indices: Vec<u8> = keys.iter().map(|k| k.index).collect();
                    let other_off: Vec<(u8, RgbColor)> = (0..=MAX_KEY_INDEX as u8)
                        .filter(|k| !target_indices.contains(k))
                        .map(|k| (k, off_color))
                        .collect();
                    rev_dev.set_multiple_keys_color(&other_off)?;
                    println!("\r✨ Other keys are now OFF.                                    ");
                }

                println!("💡 Illuminating Row {} ({} keys)...", row, keys.len());
                let key_pairs: Vec<(u8, RgbColor)> = keys.iter().map(|k| (k.index, test_color)).collect();
                rev_dev.set_multiple_keys_color(&key_pairs)?;

                let names: Vec<&str> = keys.iter().map(|k| k.name).collect();
                println!("💡 Illuminated Row {} ({} keys):", row, keys.len());
                println!("   {}", names.join(", "));
                println!("\n👉 Press [Enter] to turn off Row {} and exit...", row);

                let mut dummy = String::new();
                let _ = std::io::stdin().read_line(&mut dummy);

                let all_off: Vec<(u8, RgbColor)> = (0..=MAX_KEY_INDEX as u8)
                    .map(|k| (k, off_color))
                    .collect();
                rev_dev.set_multiple_keys_color(&all_off)?;
                println!("🌑 Row {} turned off.", row);
                return Ok(());
            }

            if let Some(ref zone) = args.zone {
                if let Some(keys) = fantastech_core::keys_for_zone(zone) {
                    if !args.no_blackout {
                        print!("🌑 Turning off other keys...");
                        let _ = std::io::stdout().flush();
                        let target_indices: Vec<u8> = keys.iter().map(|k| k.index).collect();
                        let other_off: Vec<(u8, RgbColor)> = (0..=MAX_KEY_INDEX as u8)
                            .filter(|k| !target_indices.contains(k))
                            .map(|k| (k, off_color))
                            .collect();
                        rev_dev.set_multiple_keys_color(&other_off)?;
                        println!("\r✨ Other keys are now OFF.                                    ");
                    }

                    println!("💡 Illuminating Zone '{}' ({} keys)...", zone, keys.len());
                    let key_pairs: Vec<(u8, RgbColor)> = keys.iter().map(|k| (k.index, test_color)).collect();
                    rev_dev.set_multiple_keys_color(&key_pairs)?;

                    let names: Vec<&str> = keys.iter().map(|k| k.name).collect();
                    println!("💡 Illuminated Zone '{}' ({} keys):", zone, keys.len());
                    println!("   {}", names.join(", "));
                    println!("\n👉 Press [Enter] to turn off zone '{}' and exit...", zone);

                    let mut dummy = String::new();
                    let _ = std::io::stdin().read_line(&mut dummy);

                    let all_off: Vec<(u8, RgbColor)> = (0..=MAX_KEY_INDEX as u8)
                        .map(|k| (k, off_color))
                        .collect();
                    rev_dev.set_multiple_keys_color(&all_off)?;
                    println!("🌑 Zone '{}' turned off.", zone);
                    return Ok(());
                } else {
                    println!("❌ Unknown zone '{}'. Available zones: wasd, arrows, numpad, nav, fkeys, media, bottom", zone);
                    return Ok(());
                }
            }

            if let Some(ref range_str) = args.range {
                let indices: Vec<u8> = if let Some((start_s, end_s)) = range_str.split_once("..") {
                    let start: u8 = start_s.trim().parse().map_err(|e| format!("Invalid start index: {}", e))?;
                    let end: u8 = end_s.trim().parse().map_err(|e| format!("Invalid end index: {}", e))?;
                    (start..=end).collect()
                } else {
                    let single: u8 = range_str.trim().parse().map_err(|e| format!("Invalid index: {}", e))?;
                    vec![single]
                };

                let key_pairs: Vec<(u8, RgbColor)> = indices.iter().map(|&idx| (idx, test_color)).collect();
                rev_dev.set_multiple_keys_color(&key_pairs)?;
                println!("💡 Illuminated {} matrix indices: {:?}", indices.len(), indices);
                println!("\n👉 Press [Enter] to turn off and exit...");
                let mut dummy = String::new();
                let _ = std::io::stdin().read_line(&mut dummy);

                let off_pairs: Vec<(u8, RgbColor)> = indices.iter().map(|&idx| (idx, off_color)).collect();
                rev_dev.set_multiple_keys_color(&off_pairs)?;
                return Ok(());
            }

            println!("🎹 Fantech MK894 Key Matrix Test & Mapping Suite");
            println!("===============================================");
            println!("Available Test Modes:");
            println!("  1. Interactive Wizard (Walk through each key one by one):");
            println!("     fantastech test-keys --interactive");
            println!("     fantastech test-keys --interactive --start 40");
            println!();
            println!("  2. Single Key Quick Probe:");
            println!("     fantastech test-keys --key esc");
            println!("     fantastech test-keys --key space");
            println!("     fantastech test-keys --key 44");
            println!();
            println!("  3. Row-by-Row Quick Verification:");
            println!("     fantastech test-keys --row 0    # Function row (Esc, F1..F12)");
            println!("     fantastech test-keys --row 1    # Number row (~, 1..0, -, +)");
            println!("     fantastech test-keys --row 2    # QWERTY row");
            println!("     fantastech test-keys --row 3    # Home row (ASDF, Enter)");
            println!("     fantastech test-keys --row 4    # ZXCV row (Shift, Arrows)");
            println!("     fantastech test-keys --row 5    # Bottom row (Space, Modifiers)");
            println!();
            println!("  4. Functional Zone Verification:");
            println!("     fantastech test-keys --zone wasd");
            println!("     fantastech test-keys --zone arrows");
            println!("     fantastech test-keys --zone numpad");
            println!("     fantastech test-keys --zone nav");
            println!("     fantastech test-keys --zone media");
            println!();
            println!("  5. Global Lighting Controls:");
            println!("     fantastech test-keys --all-off");
            println!("     fantastech test-keys --all-on");
            println!("===============================================");
        }
        Commands::Gui(args) => {
            handle_gui_launch(args.update)?;
        }
        Commands::Tray => {
            handle_tray_launch()?;
        }
        Commands::Service(args) => {
            handle_service_dispatch(args, cli.emulator, cli.device.clone())?;
        }
        Commands::Config(args) => {
            handle_config_command(args)?;
        }
        Commands::Install(_) | Commands::Uninstall(_) => unreachable!(),
    }

    Ok(())
}
