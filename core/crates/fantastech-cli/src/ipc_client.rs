//! Client-side IPC routing to the background daemon.

use crate::args::*;
use crate::parse::*;
use fantastech_core::*;
use fantastech_protocol::lighting::*;

pub(crate) fn try_route_via_ipc(cli: &Cli) -> Result<bool, Box<dyn std::error::Error>> {
    if cli.no_ipc || cli.emulator || !is_service_running() {
        return Ok(false);
    }

    let cmd = match &cli.command {
        Some(cmd) => cmd,
        None => return Ok(false),
    };

    match cmd {
        Commands::Info => {
            match send_ipc_request(&IpcRequest::GetStatus) {
                Ok(IpcResponse::Status(st)) => {
                    println!("📋 FantasTech Service & Hardware Status (via IPC):");
                    println!("==========================================");
                    println!("  Service Running:         {}", st.service_running);
                    println!("  Process ID (PID):        {}", st.pid);
                    println!("  Active Lighting Profile: {}", st.active_lighting_profile);
                    println!("  Active Button Profile:   {}", st.active_button_profile);
                    println!("  Volume Knob Guard:       {}", if st.volume_guard_running { "Active" } else { "Inactive" });
                    println!("  OpenRGB SDK Server:      {}", if st.openrgb_running { format!("Listening on port {}", st.openrgb_port) } else { "Disabled".into() });
                    println!("==========================================");
                    return Ok(true);
                }
                Ok(resp) => println!("Response: {:?}", resp),
                Err(e) => eprintln!("IPC Error: {}", e),
            }
            Ok(true)
        }
        Commands::Light(LightArgs { command: LightCommands::Off }) => {
            println!("🌑 Turning off all backlighting via FantasTech background service...");
            match send_ipc_request(&IpcRequest::LightingOff) {
                Ok(IpcResponse::Ok) => println!("✅ Keyboard backlighting turned OFF."),
                Ok(other) => println!("Response: {:?}", other),
                Err(e) => eprintln!("❌ IPC Error: {}", e),
            }
            Ok(true)
        }
        Commands::Light(LightArgs { command: LightCommands::Profile { id } }) => {
            println!("🎨 Applying lighting profile '{}' via FantasTech background service...", id);
            match send_ipc_request(&IpcRequest::ApplyLightingProfile { id: id.clone() }) {
                Ok(IpcResponse::Ok) => println!("✅ Applied lighting profile '{}'", id),
                Ok(IpcResponse::Error(err)) => eprintln!("❌ Service error: {}", err),
                Ok(other) => println!("Response: {:?}", other),
                Err(e) => eprintln!("❌ IPC Error: {}", e),
            }
            Ok(true)
        }
        Commands::Light(LightArgs {
            command: LightCommands::SetMode {
                mode,
                brightness,
                speed,
                direction,
                color,
                hex,
                spectrum,
            },
        }) => {
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

            let req = IpcRequest::SetLightingMode {
                mode: lighting_mode,
                brightness: norm_brightness,
                speed: norm_speed,
                direction: anim_direction as u8,
                color_mode,
                color: [rgb.r, rgb.g, rgb.b],
            };
            match send_ipc_request(&req) {
                Ok(IpcResponse::Ok) => println!("✨ Applied lighting mode '{:?}' via service", lighting_mode),
                Ok(other) => println!("Response: {:?}", other),
                Err(e) => eprintln!("❌ IPC Error: {}", e),
            }
            Ok(true)
        }
        Commands::Light(LightArgs {
            command: LightCommands::SetKey {
                key,
                r,
                g,
                b,
                hex,
                color,
            },
        }) => {
            let key_index = parse_key_identifier(key)?;
            let col = if let (Some(r_val), Some(g_val), Some(b_val)) = (*r, *g, *b) {
                RgbColor::new(r_val, g_val, b_val)
            } else if let Some(ref h) = hex {
                RgbColor::from_hex(h).ok_or("Invalid hex color")?
            } else if let Some(ref c) = color {
                parse_color_spec(c)?
            } else {
                return Err("Please specify a color: `--hex #FF0000`, `-c red`, or `R G B`".into());
            };
            match send_ipc_request(&IpcRequest::SetKeyColor {
                key_index,
                r: col.r,
                g: col.g,
                b: col.b,
            }) {
                Ok(IpcResponse::Ok) => println!("💡 Key '{}' (index {}) set to RGB({}, {}, {}) via service", key, key_index, col.r, col.g, col.b),
                Ok(other) => println!("Response: {:?}", other),
                Err(e) => eprintln!("❌ IPC Error: {}", e),
            }
            Ok(true)
        }
        Commands::Profile(ProfileArgs {
            command: Some(ProfileCommands::Apply { arg1, arg2, profile_type }),
            ..
        }) => {
            let (domain, id) = resolve_profile_domain_and_id(arg1, arg2.as_deref(), profile_type.as_deref());
            if domain.eq_ignore_ascii_case("lighting") {
                match send_ipc_request(&IpcRequest::ApplyLightingProfile { id: id.clone() }) {
                    Ok(IpcResponse::Ok) => println!("✅ Applied lighting profile '{}' via service", id),
                    Ok(IpcResponse::Error(err)) => eprintln!("❌ Service error: {}", err),
                    Ok(other) => println!("Response: {:?}", other),
                    Err(e) => eprintln!("❌ IPC Error: {}", e),
                }
                return Ok(true);
            } else if domain.eq_ignore_ascii_case("buttons") || domain.eq_ignore_ascii_case("button") {
                match send_ipc_request(&IpcRequest::ApplyButtonProfile { id: id.clone() }) {
                    Ok(IpcResponse::Ok) => println!("✅ Applied button profile '{}' via service", id),
                    Ok(IpcResponse::Error(err)) => eprintln!("❌ Service error: {}", err),
                    Ok(other) => println!("Response: {:?}", other),
                    Err(e) => eprintln!("❌ IPC Error: {}", e),
                }
                return Ok(true);
            } else if domain.eq_ignore_ascii_case("macros") || domain.eq_ignore_ascii_case("macro") {
                match send_ipc_request(&IpcRequest::ApplyMacroProfile { id: id.clone() }) {
                    Ok(IpcResponse::Ok) => println!("⚡ Applied macro profile '{}' via service", id),
                    Ok(IpcResponse::Error(err)) => eprintln!("❌ Service error: {}", err),
                    Ok(other) => println!("Response: {:?}", other),
                    Err(e) => eprintln!("❌ IPC Error: {}", e),
                }
                return Ok(true);
            } else if domain.eq_ignore_ascii_case("settings") || domain.eq_ignore_ascii_case("setting") {
                match send_ipc_request(&IpcRequest::ApplySettingsProfile { id: id.clone() }) {
                    Ok(IpcResponse::Ok) => println!("⚙️  Applied settings profile '{}' via service", id),
                    Ok(IpcResponse::Error(err)) => eprintln!("❌ Service error: {}", err),
                    Ok(other) => println!("Response: {:?}", other),
                    Err(e) => eprintln!("❌ IPC Error: {}", e),
                }
                return Ok(true);
            }
            Ok(false)
        }
        Commands::Profile(ProfileArgs {
            command: Some(ProfileCommands::Select { index }),
            ..
        }) => {
            match send_ipc_request(&IpcRequest::SwitchHardwareProfile(*index)) {
                Ok(IpcResponse::Ok) => println!("🔄 Switched hardware profile to Profile {} via service", index),
                Ok(IpcResponse::Error(err)) => eprintln!("❌ Service error: {}", err),
                Ok(other) => println!("Response: {:?}", other),
                Err(e) => eprintln!("❌ IPC Error: {}", e),
            }
            Ok(true)
        }
        Commands::Profile(ProfileArgs { index: Some(idx), .. }) => {
            match send_ipc_request(&IpcRequest::SwitchHardwareProfile(*idx)) {
                Ok(IpcResponse::Ok) => println!("🔄 Switched hardware profile to Profile {} via service", idx),
                Ok(IpcResponse::Error(err)) => eprintln!("❌ Service error: {}", err),
                Ok(other) => println!("Response: {:?}", other),
                Err(e) => eprintln!("❌ IPC Error: {}", e),
            }
            Ok(true)
        }
        Commands::Setting(SettingArgs { command: SettingCommands::Sleep { minutes } }) => {
            match send_ipc_request(&IpcRequest::SetSleepTimeout(minutes * 60)) {
                Ok(IpcResponse::Ok) => println!("💤 Sleep timeout set to {} minutes via service", minutes),
                Ok(other) => println!("Response: {:?}", other),
                Err(e) => eprintln!("❌ IPC Error: {}", e),
            }
            Ok(true)
        }
        Commands::Setting(SettingArgs { command: SettingCommands::Reset { token, commit } }) => {
            if token.as_deref() == Some("I_CONFIRM_FACTORY_RESET") && *commit {
                match send_ipc_request(&IpcRequest::FactoryReset) {
                    Ok(IpcResponse::Ok) => println!("🔄 Factory reset executed via service"),
                    Ok(other) => println!("Response: {:?}", other),
                    Err(e) => eprintln!("❌ IPC Error: {}", e),
                }
                return Ok(true);
            }
            Ok(false)
        }
        Commands::Light(LightArgs { command: LightCommands::Preset { name } }) => {
            let n = name.to_lowercase();
            if n == "off" {
                match send_ipc_request(&IpcRequest::LightingOff) {
                    Ok(IpcResponse::Ok) => println!("🌑 Keyboard backlighting turned OFF via service."),
                    Ok(other) => println!("Response: {:?}", other),
                    Err(e) => eprintln!("❌ IPC Error: {}", e),
                }
                return Ok(true);
            }
            if n == "rainbow_wave" || n == "rainbow" {
                let req = IpcRequest::SetLightingMode {
                    mode: LightingMode::Spectrum,
                    brightness: 4,
                    speed: 2,
                    direction: 0,
                    color_mode: ColorMode::DynamicSpectrum,
                    color: [0, 0, 0],
                };
                match send_ipc_request(&req) {
                    Ok(IpcResponse::Ok) => println!("🌈 Applied Rainbow Wave preset via service"),
                    Ok(other) => println!("Response: {:?}", other),
                    Err(e) => eprintln!("❌ IPC Error: {}", e),
                }
                return Ok(true);
            }
            match send_ipc_request(&IpcRequest::ApplyLightingProfile { id: name.clone() }) {
                Ok(IpcResponse::Ok) => println!("✅ Applied lighting profile '{}' via service", name),
                _ => {
                    let theme = match n.as_str() {
                        "cyberpunk" => ThemeConfig::cyberpunk(),
                        "gaming" | "contrast" => ThemeConfig::gaming(),
                        _ => ThemeConfig::default(),
                    };
                    let matrix = theme.to_matrix();
                    let colors: Vec<[u8; 3]> = matrix.iter().map(|c| [c.r, c.g, c.b]).collect();
                    match send_ipc_request(&IpcRequest::SetCanvasMatrix { colors }) {
                        Ok(IpcResponse::Ok) => println!("✨ Applied preset '{}' matrix canvas via service", name),
                        Ok(other) => println!("Response: {:?}", other),
                        Err(e) => eprintln!("❌ IPC Error: {}", e),
                    }
                }
            }
            Ok(true)
        }
        Commands::Remap(RemapArgs { command: RemapCommands::Reset { layer, commit } }) => {
            if *commit {
                let l = parse_layer(layer)?;
                let default_actions = default_layer_keymap(l);
                match send_ipc_request(&IpcRequest::ApplyKeymapLayer {
                    layer: l,
                    actions: default_actions.to_vec(),
                }) {
                    Ok(IpcResponse::Ok) => println!("🔄 Reset {:?} layer keymap to factory defaults via service", l),
                    Ok(other) => println!("Response: {:?}", other),
                    Err(e) => eprintln!("❌ IPC Error: {}", e),
                }
                return Ok(true);
            }
            Ok(false)
        }
        Commands::Remap(RemapArgs { command: RemapCommands::Set { key, action, layer, commit } }) => {
            if *commit {
                let key_index = parse_key_identifier(key)?;
                let l = parse_layer(layer)?;
                let act = parse_action_str(action)?;
                let mut layer_map = default_layer_keymap(l);
                if (key_index as usize) < layer_map.len() {
                    layer_map[key_index as usize] = act;
                }
                if let Some(mirror) = get_mirror_key_index(key_index) {
                    if (mirror as usize) < layer_map.len() {
                        layer_map[mirror as usize] = act;
                    }
                }
                match send_ipc_request(&IpcRequest::ApplyKeymapLayer {
                    layer: l,
                    actions: layer_map.to_vec(),
                }) {
                    Ok(IpcResponse::Ok) => println!("✨ Key '{}' (index {}) set to {} on {:?} layer via service", key, key_index, action, l),
                    Ok(other) => println!("Response: {:?}", other),
                    Err(e) => eprintln!("❌ IPC Error: {}", e),
                }
                return Ok(true);
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}
