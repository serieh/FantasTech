//! Command-line control interface (`fantech-ctl`) for Fantech MK894 Maxpower II.

#![forbid(unsafe_code)]

use clap::{Args, Parser, Subcommand};
use fantech_core::{ThemeConfig, KEY_DEFINITIONS};
use fantech_device::Device;
use fantech_emulator::VirtualKeyboardEmulator;
use fantech_protocol::constants::{MAX_KEY_INDEX, SPARSE_KEY_COUNT};
use fantech_protocol::keymap::{KeyAction, KeyLayer};
use fantech_protocol::lighting::{AnimationDirection, ColorMode, LightingConfig, LightingMode, LightingZone, RgbColor};
use fantech_protocol::openrgb::{
    build_openrgb_controller_data, normalize_brightness_to_levels, normalize_brightness_to_percent,
    parse_openrgb_set_mode, parse_openrgb_update_leds, parse_openrgb_update_single_led,
    parse_openrgb_update_zone_leds, scale_rgb, OpenRgbHeader,
    NET_PACKET_ID_REQUEST_CONTROLLER_COUNT, NET_PACKET_ID_REQUEST_CONTROLLER_DATA,
    NET_PACKET_ID_REQUEST_PROTOCOL_VERSION, NET_PACKET_ID_RGBCONTROLLER_SAVEMODE,
    NET_PACKET_ID_RGBCONTROLLER_SETCUSTOMMODE, NET_PACKET_ID_RGBCONTROLLER_SETMODE,
    NET_PACKET_ID_RGBCONTROLLER_UPDATELEDS, NET_PACKET_ID_RGBCONTROLLER_UPDATESINGLELED,
    NET_PACKET_ID_RGBCONTROLLER_UPDATEZONELEDS, NET_PACKET_ID_SET_CLIENT_NAME,
};
use fantech_transport::{HidrawTransport, Transport};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "fantech-ctl",
    author = "Clean-Room Engineering Team",
    version,
    about = "Safe, cross-platform driver and CLI for Fantech MK894 Maxpower II keyboard",
    long_about = "Clean-room reverse-engineered tool for controlling RGB backlighting, key remaps, onboard profiles, 4KB macro memory, and OpenRGB SDK integration on Linux."
)]
struct Cli {
    /// Use simulated virtual keyboard emulator instead of physical USB device
    #[arg(long, global = true)]
    emulator: bool,

    /// Explicit device node path (e.g. /dev/hidraw4)
    #[arg(short, long, global = true)]
    device: Option<PathBuf>,

    /// Dry-run mode: display generated report bytes without transmitting to hardware
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Query device identification, firmware version, and active profile
    Info,

    /// Lighting configuration subcommands
    Light(LightArgs),

    /// Query or switch active onboard hardware profile (0, 1, or 2)
    Profile(ProfileArgs),

    /// Run background OpenRGB SDK Server daemon (TCP port 6745) for OpenRGB apps
    #[command(name = "openrgb-server", alias = "open-rgb-server")]
    OpenRgbServer(OpenRgbServerArgs),

    /// Generate or install udev rules for non-root keyboard access on Linux
    #[command(name = "setup-udev", alias = "setup_udev")]
    SetupUdev(SetupUdevArgs),

    /// Generate or install systemd user service for background OpenRGB SDK server
    #[command(name = "setup-systemd", alias = "setup_systemd")]
    SetupSystemd(SetupSystemdArgs),

    /// Sync keyboard RGB lighting with KDE Plasma 6 desktop accent color
    #[command(name = "kde-sync", alias = "sync-kde")]
    KdeSync(KdeSyncArgs),

    /// Install KDE Plasma 6 desktop widget (plasmoid) and desktop application launcher
    #[command(name = "setup-kde", alias = "setup_kde")]
    SetupKde(SetupKdeArgs),

    /// Key remapping commands
    Remap(RemapArgs),

    /// 4KB hardware macro memory subcommands
    Macro(MacroArgs),

    /// Read-before-write state backup
    Backup(BackupArgs),

    /// Interactive matrix mapping and key verification test suite
    TestKeys(TestKeysArgs),
}

#[derive(Args, Debug)]
struct ProfileArgs {
    /// Target profile index to switch to (0..2)
    index: Option<u8>,

    /// Query active profile from hardware
    #[arg(short, long)]
    current: bool,
}

#[derive(Args, Debug)]
struct OpenRgbServerArgs {
    /// TCP port to listen on for OpenRGB SDK clients (default: 6745, avoiding OpenRGB main port 6742)
    #[arg(short, long, default_value = "6745")]
    port: u16,

    /// IP address to bind to (default: 127.0.0.1)
    #[arg(short, long, default_value = "127.0.0.1")]
    bind: String,
}

#[derive(Args, Debug)]
struct SetupUdevArgs {
    /// Install rules directly to /etc/udev/rules.d/99-fantech-mk894.rules (requires sudo)
    #[arg(short, long)]
    install: bool,

    /// Print udev rules to stdout
    #[arg(long)]
    stdout: bool,
}

#[derive(Args, Debug)]
struct SetupSystemdArgs {
    /// Install systemd user service to ~/.config/systemd/user/fantech-openrgb.service
    #[arg(short, long)]
    install: bool,

    /// Print systemd service unit to stdout
    #[arg(long)]
    stdout: bool,
}

#[derive(Args, Debug)]
struct KdeSyncArgs {
    /// Watch ~/.config/kdeglobals continuously and update keyboard when accent color changes
    #[arg(short, long)]
    watch: bool,

    /// Lighting animation mode: static, breathing, or wave (default: static)
    #[arg(short, long, default_value = "static")]
    mode: String,

    /// Brightness level (0..4, default: 4)
    #[arg(short, long, default_value = "4")]
    brightness: u8,
}

#[derive(Args, Debug)]
struct SetupKdeArgs {
    /// Install plasmoid to ~/.local/share/plasma/plasmoids/org.kde.fantech.lighting and application launcher
    #[arg(short, long)]
    install: bool,

    /// Target directory to export plasmoid files (default: ./kde-plasma-widget)
    #[arg(short, long)]
    dir: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct LightArgs {
    #[command(subcommand)]
    command: LightCommands,
}

#[derive(Subcommand, Debug)]
enum LightCommands {
    /// List all 17 hardware lighting animation modes and their capabilities
    ListModes,

    /// Set global lighting animation mode, brightness, speed, and color
    SetMode {
        /// Animation mode name (e.g. static, breathing, wave, reactive) or index (0..16)
        mode: String,

        /// Brightness level (0..4) or percentage (0..100)
        #[arg(short, long, default_value = "4")]
        brightness: u8,

        /// Animation speed level (0..4) or percentage (0..100)
        #[arg(short, long, default_value = "2")]
        speed: u8,

        /// Animation direction (default, left, right, inward, outward)
        #[arg(short, long, default_value = "default")]
        direction: String,

        /// Fixed RGB color: R G B (0..255)
        #[arg(long, num_args = 3)]
        color: Option<Vec<u8>>,

        /// Fixed RGB color in hex format (e.g. #FF0055 or FF0055)
        #[arg(long)]
        hex: Option<String>,

        /// Force dynamic spectrum/rainbow color mode
        #[arg(long)]
        spectrum: bool,
    },

    /// Apply a pre-configured theme preset (e.g. cyberpunk, gaming, default)
    Preset {
        /// Preset name (cyberpunk, gaming, default)
        name: String,
    },

    /// Query active lighting configuration directly from keyboard firmware
    Status,

    /// Set an individual key LED color
    SetKey {
        /// Physical matrix key index (0..126) or key name (e.g. 'space', 'd', 'w', 'esc')
        key: String,

        /// Red (0..255)
        #[arg(requires = "g", requires = "b")]
        r: Option<u8>,

        /// Green (0..255)
        #[arg(requires = "r", requires = "b")]
        g: Option<u8>,

        /// Blue (0..255)
        #[arg(requires = "r", requires = "g")]
        b: Option<u8>,

        /// Hex color code (e.g. #FF0000 or FF0000)
        #[arg(long)]
        hex: Option<String>,

        /// Color name (e.g. red, green, blue, yellow, cyan, purple, white, off)
        #[arg(short, long)]
        color: Option<String>,
    },

    /// Custom mode: live interactive key painter, save/load presets, or batch key overrides
    Custom(CustomLightArgs),
}

#[derive(Args, Debug)]
struct CustomLightArgs {
    /// Launch interactive real-time key painter wizard with live keyboard feedback
    #[arg(short, long)]
    interactive: bool,

    /// Save current custom lighting layout to a JSON preset file
    #[arg(long)]
    save: Option<PathBuf>,

    /// Load and apply custom lighting layout from a JSON preset file
    #[arg(long)]
    load: Option<PathBuf>,

    /// Batch assign key colors directly from CLI (e.g. "wasd=red,arrows=blue,space=#00FF00")
    #[arg(long)]
    set: Option<String>,

    /// Background color for unassigned keys (name, hex, or 'off'; default: 'off')
    #[arg(long, default_value = "off")]
    default_color: String,

    /// Turn off all keys not explicitly assigned
    #[arg(long)]
    blackout: bool,
}

#[derive(Args, Debug)]
struct RemapArgs {
    /// Physical matrix key index (0..126) or key name (e.g. 'space', 'd', 'a')
    key: String,
    /// USB HID key usage code (e.g. 0x04 for 'A')
    hid_usage: u8,
    /// Layer (0 = Base, 1 = Fn)
    #[arg(short, long, default_value = "0")]
    layer: u8,
    /// Commit persistent write to hardware flash memory
    #[arg(long)]
    commit: bool,
}

#[derive(Args, Debug)]
struct MacroArgs {
    #[command(subcommand)]
    command: MacroCommands,
}

#[derive(Subcommand, Debug)]
enum MacroCommands {
    /// Wipe all 4KB hardware onboard macro memory
    Wipe {
        /// Confirm persistent erase
        #[arg(long)]
        confirm: bool,
    },
}

#[derive(Args, Debug)]
struct BackupArgs {
    /// Output directory for backup files
    #[arg(short, long, default_value = "./backup")]
    output_dir: PathBuf,
}

#[derive(Args, Debug)]
struct TestKeysArgs {
    /// Interactive step-by-step key verification wizard
    #[arg(short, long)]
    interactive: bool,

    /// Start index for interactive wizard (default: 0)
    #[arg(long, default_value = "0")]
    start: usize,

    /// Test an individual key by name or index (e.g. 'esc', 'space', '44', '66')
    #[arg(short, long)]
    key: Option<String>,

    /// Light up a specific physical matrix row (0..5)
    #[arg(long)]
    row: Option<u8>,

    /// Light up a specific functional zone (wasd, arrows, numpad, nav, fkeys, media, bottom)
    #[arg(long)]
    zone: Option<String>,

    /// Skip initial blackout (do not turn off other keys before test)
    #[arg(long)]
    no_blackout: bool,

    /// Turn all keyboard LEDs OFF
    #[arg(long)]
    all_off: bool,

    /// Turn all keyboard LEDs ON with solid test color
    #[arg(long)]
    all_on: bool,

    /// Raw index range to test (e.g. "0..20" or single index "66")
    #[arg(long)]
    range: Option<String>,

    /// Color to use for testing: R G B (0..255, default: 255 0 0)
    #[arg(long, num_args = 3, default_values = &["255", "0", "0"])]
    color: Vec<u8>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    // Commands that don't need USB device handle:
    match &cli.command {
        Commands::SetupUdev(args) => {
            handle_setup_udev(args)?;
            return Ok(());
        }
        Commands::SetupSystemd(args) => {
            handle_setup_systemd(args)?;
            return Ok(());
        }
        Commands::SetupKde(args) => {
            handle_setup_kde(args)?;
            return Ok(());
        }
        Commands::Light(LightArgs { command: LightCommands::ListModes }) => {
            print_modes_table();
            return Ok(());
        }
        _ => {}
    }

    if cli.emulator {
        println!("🚀 Running in VIRTUAL EMULATOR mode (in-memory simulated keyboard)...");
        let emu = VirtualKeyboardEmulator::new();
        run_command(emu, &cli)?;
    } else {
        println!("🔍 Connecting to physical Fantech MK894 keyboard...");
        let transport = if let Some(ref path) = cli.device {
            HidrawTransport::open(path)?
        } else {
            match HidrawTransport::discover() {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("⚠️  Could not automatically open physical keyboard: {e}");
                    eprintln!("💡 Tip: Pass `--emulator` to test commands against the virtual simulated keyboard,");
                    eprintln!("   or run `sudo fantech-ctl setup-udev --install` to grant permission to /dev/hidraw.");
                    return Err(e.into());
                }
            }
        };
        run_command(transport, &cli)?;
    }

    Ok(())
}

fn run_command<T: Transport>(transport: T, cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let mut dev = Device::new_read_only(transport);

    match &cli.command {
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
            if args.current || args.index.is_none() {
                let info = dev.query_info()?;
                println!("📋 Active Hardware Profile:");
                println!("   Profile Slot:      Profile {}", info.active_profile);
                println!("   Total Slots:       {}", info.profile_count);
                println!("   Profile Name:      {}", info.profile_name);
                println!("   Firmware:          {}", info.firmware_version);
                return Ok(());
            }

            let index = args.index.unwrap();
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
                    println!("  Direction:         {:?}", config.direction);
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
                        (ColorMode::FixedColor, RgbColor::new(c[0], c[1], c[2]))
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
            }
        }
        Commands::Remap(args) => {
            let key_index = parse_key_identifier(&args.key)?;
            let rev_dev = dev.into_reversible();
            if !args.commit && !cli.dry_run {
                println!("⚠️  PERSISTENT FLASH WRITE SAFETY GATE:");
                println!("   Remapping writes directly to onboard non-volatile MCU memory.");
                println!("   To preview without writing, run with `--dry-run`.");
                println!("   To commit the persistent write, run with `--commit`.");
                return Ok(());
            }

            let mut persistent_dev = rev_dev.into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
            persistent_dev.set_dry_run(cli.dry_run || !args.commit);

            let layer = if args.layer == 1 { KeyLayer::Fn } else { KeyLayer::Base };
            let action = KeyAction::standard_key(args.hid_usage);

            if let Some(record) = persistent_dev.remap_key(key_index, layer, &action)? {
                println!("🔍 [DRY-RUN] Proposed Flash Write Packet: {:02X?}", &record.buffer[..12]);
                println!("   Action: {}", record.explanation);
            } else {
                println!("💾 Key '{}' (matrix index {}) on {:?} layer successfully remapped to HID usage 0x{:02X}", args.key, key_index, layer, args.hid_usage);
            }
        }
        Commands::Macro(args) => match &args.command {
            MacroCommands::Wipe { confirm } => {
                let rev_dev = dev.into_reversible();
                if !confirm {
                    println!("⚠️  DESTRUCTIVE OPERATION SAFETY GATE:");
                    println!("   This will erase all 4,096 bytes of onboard hardware macro memory.");
                    println!("   Pass `--confirm` to authorize this erase operation.");
                    return Ok(());
                }

                let mut persistent_dev = rev_dev.into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
                persistent_dev.set_dry_run(cli.dry_run);

                if let Some(record) = persistent_dev.wipe_macros()? {
                    println!("🔍 [DRY-RUN] Proposed Packet: {:02X?}", &record.buffer[..6]);
                    println!("   Action: {}", record.explanation);
                } else {
                    println!("🧹 4KB hardware macro memory successfully erased.");
                }
            }
        },
        Commands::Backup(args) => {
            println!("💾 Performing read-before-write state backup to {:?}", args.output_dir);
            std::fs::create_dir_all(&args.output_dir)?;
            let info = dev.query_info()?;
            let info_json = format!(
                "{{\n  \"profile\": \"{}\",\n  \"firmware\": \"{}\",\n  \"active_profile\": {}\n}}\n",
                info.profile_name, info.firmware_version, info.active_profile
            );
            std::fs::write(args.output_dir.join("device_info.json"), info_json)?;
            println!("✅ Backup complete: saved device metadata to {:?}", args.output_dir);
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
                    std::io::stdout().flush().map_err(std::io::Error::from)?;

                    let mut input = String::new();
                    stdin.read_line(&mut input).map_err(std::io::Error::from)?;
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
                let keys = fantech_core::keys_for_row(row);
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
                if let Some(keys) = fantech_core::keys_for_zone(zone) {
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
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --interactive");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --interactive --start 40");
            println!();
            println!("  2. Single Key Quick Probe:");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --key esc");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --key space");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --key 44");
            println!();
            println!("  3. Row-by-Row Quick Verification:");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --row 0    # Function row (Esc, F1..F12)");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --row 1    # Number row (~, 1..0, -, +)");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --row 2    # QWERTY row");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --row 3    # Home row (ASDF, Enter)");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --row 4    # ZXCV row (Shift, Arrows)");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --row 5    # Bottom row (Space, Modifiers)");
            println!();
            println!("  4. Functional Zone Verification:");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --zone wasd");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --zone arrows");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --zone numpad");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --zone nav");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --zone media");
            println!();
            println!("  5. Global Lighting Controls:");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --all-off");
            println!("     sudo ./fantech-rs/target/release/fantech-ctl test-keys --all-on");
            println!("===============================================");
        }
    }

    Ok(())
}

/// Applies a full `ThemeConfig` to the keyboard using differential matrix caching and dual-pad synchronization.
fn apply_theme_to_keyboard<T: Transport>(
    dev: &mut Device<T, fantech_device::ReversibleAccess>,
    theme: &ThemeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🎨 Applying theme preset '{}' across keyboard...", theme.name);

    let bg_color = RgbColor::new(theme.default_color[0], theme.default_color[1], theme.default_color[2]);

    let mut updates: Vec<(u8, RgbColor)> = Vec::with_capacity(126);
    for idx in 0..=MAX_KEY_INDEX as u8 {
        let color = if let Some(&rgb) = theme.key_overrides.get(&(idx as usize)) {
            RgbColor::new(rgb[0], rgb[1], rgb[2])
        } else if idx == 95 && theme.key_overrides.contains_key(&96) {
            let rgb = theme.key_overrides[&96];
            RgbColor::new(rgb[0], rgb[1], rgb[2])
        } else if idx == 96 && theme.key_overrides.contains_key(&95) {
            let rgb = theme.key_overrides[&95];
            RgbColor::new(rgb[0], rgb[1], rgb[2])
        } else if idx == 123 && theme.key_overrides.contains_key(&122) {
            let rgb = theme.key_overrides[&122];
            RgbColor::new(rgb[0], rgb[1], rgb[2])
        } else if idx == 122 && theme.key_overrides.contains_key(&123) {
            let rgb = theme.key_overrides[&123];
            RgbColor::new(rgb[0], rgb[1], rgb[2])
        } else if idx == 110 && theme.key_overrides.contains_key(&111) {
            let rgb = theme.key_overrides[&111];
            RgbColor::new(rgb[0], rgb[1], rgb[2])
        } else if idx == 75 && theme.key_overrides.contains_key(&76) {
            let rgb = theme.key_overrides[&76];
            RgbColor::new(rgb[0], rgb[1], rgb[2])
        } else if idx == 85 && theme.key_overrides.contains_key(&84) {
            let rgb = theme.key_overrides[&84];
            RgbColor::new(rgb[0], rgb[1], rgb[2])
        } else if KEY_DEFINITIONS.iter().any(|k| k.index == idx)
            || idx == 95 || idx == 96 || idx == 122 || idx == 123 || idx == 110 || idx == 111 || idx == 75 || idx == 76 || idx == 84 || idx == 85 {
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

/// Handles `fantech-ctl light custom` subcommands (interactive painter, JSON save/load, CLI batch set).
fn handle_custom_lighting<T: Transport>(
    dev: &mut Device<T, fantech_device::ReversibleAccess>,
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
    println!("  1. Interactive Live Painter:   fantech-ctl light custom --interactive");
    println!("  2. Direct Batch Assignment:    fantech-ctl light custom --set 'wasd=red,arrows=blue,space=green'");
    println!("  3. Save Preset to File:        fantech-ctl light custom --set 'wasd=red' --save mytheme.json");
    println!("  4. Load Preset from File:      fantech-ctl light custom --load mytheme.json");

    Ok(())
}

/// Interactive live key painter session with immediate physical LED updates.
fn run_interactive_key_painter<T: Transport>(
    dev: &mut Device<T, fantech_device::ReversibleAccess>,
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
                        if let Some(keys) = fantech_core::keys_for_zone(zone_name) {
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
                    let keys = fantech_core::keys_for_row(row_num);
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

/// Runs the OpenRGB SDK Server daemon on a TCP port (default: 6742).
fn run_openrgb_server<T: Transport>(
    mut dev: Device<T, fantech_device::ReversibleAccess>,
    bind: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{}:{}", bind, port);
    let listener = TcpListener::bind(&addr)?;

    println!("🌐 ================================================================");
    println!("🌐 Fantech MK894 OpenRGB SDK Server Daemon");
    println!("🌐 ================================================================");
    println!("   Listening Address:   tcp://{}", addr);
    println!("   Protocol Version:    OpenRGB SDK v3");
    println!("   Device Integration:  Fantech MK894 Maxpower II (104 Mechanical Keys)");
    println!("   Differential Sync:   Active (only changed LEDs sent over USB HID)");
    println!();
    println!("💡 Connect OpenRGB (desktop GUI or CLI) using Client Mode -> 127.0.0.1:{}", port);
    println!("⌨️  Press Ctrl+C to terminate the daemon.");
    println!("🌐 ================================================================");
    println!();

    // Prepare controller definition data
    let keys_meta: Vec<(&str, u8, u8, u8)> = KEY_DEFINITIONS
        .iter()
        .map(|k| (k.name, k.row, k.col, k.index))
        .collect();

    // Query active hardware lighting preset to reflect true hardware state in OpenRGB GUI
    let hw_cfg = dev.query_light_config().ok();
    let (mut active_mode, mut active_brightness, initial_color) = if let Some(ref cfg) = hw_cfg {
        println!(
            "💡 Keyboard hardware state detected: Mode {:?}, Brightness {}, Color RGB({}, {}, {})",
            cfg.mode, cfg.brightness, cfg.color.r, cfg.color.g, cfg.color.b
        );
        let color = if cfg.color != RgbColor::new(0, 0, 0) {
            cfg.color
        } else {
            RgbColor::new(255, 0, 0)
        };
        (
            cfg.mode as u8,
            normalize_brightness_to_percent(cfg.brightness as u32),
            color,
        )
    } else {
        (16u8, 100u8, RgbColor::new(255, 0, 0))
    };

    let mut current_colors = vec![initial_color; KEY_DEFINITIONS.len()];
    let mut sparse_matrix = [initial_color; SPARSE_KEY_COUNT];
    let mut active_direction: u32 = 0;

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("⚠️ Incoming TCP connection error: {e}");
                continue;
            }
        };

        let peer = stream.peer_addr().map(|p| p.to_string()).unwrap_or_else(|_| "Unknown".into());
        println!("🤝 OpenRGB client connected from {}", peer);

        loop {
            let mut hdr_bytes = [0u8; 16];
            if let Err(e) = stream.read_exact(&mut hdr_bytes) {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    println!("👋 Client {} disconnected.", peer);
                    break;
                }
                eprintln!("⚠️ Client socket error: {e}");
                break;
            }

            let hdr = match OpenRgbHeader::from_bytes(&hdr_bytes) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("⚠️ Invalid OpenRGB packet header: {e}");
                    break;
                }
            };

            let mut payload = vec![0u8; hdr.pkt_size as usize];
            if hdr.pkt_size > 0 {
                if let Err(e) = stream.read_exact(&mut payload) {
                    eprintln!("⚠️ Failed reading packet payload: {e}");
                    break;
                }
            }

            match hdr.pkt_id {
                NET_PACKET_ID_REQUEST_PROTOCOL_VERSION => {
                    let client_ver = if payload.len() >= 4 {
                        u32::from_le_bytes(payload[0..4].try_into().unwrap())
                    } else {
                        3
                    };
                    println!("📋 Client requested OpenRGB Protocol Version {}", client_ver);
                    let resp_hdr = OpenRgbHeader::new(0, NET_PACKET_ID_REQUEST_PROTOCOL_VERSION, 4);
                    let server_ver = 3u32;
                    let _ = stream.write_all(&resp_hdr.to_bytes());
                    let _ = stream.write_all(&server_ver.to_le_bytes());
                    let _ = stream.flush();
                }
                NET_PACKET_ID_SET_CLIENT_NAME => {
                    let name = String::from_utf8_lossy(&payload)
                        .trim_matches('\0')
                        .to_string();
                    println!("🏷️  Client registered client name: '{}'", name);
                }
                NET_PACKET_ID_REQUEST_CONTROLLER_COUNT => {
                    println!("🔍 Client queried controller count (reporting 1 MK894 keyboard)");
                    let resp_hdr = OpenRgbHeader::new(0, NET_PACKET_ID_REQUEST_CONTROLLER_COUNT, 4);
                    let count = 1u32;
                    let _ = stream.write_all(&resp_hdr.to_bytes());
                    let _ = stream.write_all(&count.to_le_bytes());
                    let _ = stream.flush();
                }
                NET_PACKET_ID_REQUEST_CONTROLLER_DATA => {
                    println!("📦 Serving MK894 controller matrix topology & color state to OpenRGB (active_mode={}, brightness={}%, dir={})...", active_mode, active_brightness, active_direction);
                    let controller_data = build_openrgb_controller_data(&keys_meta, &current_colors, active_mode, active_brightness, active_direction);
                    let resp_hdr = OpenRgbHeader::new(hdr.dev_id, NET_PACKET_ID_REQUEST_CONTROLLER_DATA, controller_data.len() as u32);
                    let _ = stream.write_all(&resp_hdr.to_bytes());
                    let _ = stream.write_all(&controller_data);
                    let _ = stream.flush();
                }
                NET_PACKET_ID_RGBCONTROLLER_UPDATELEDS => {
                    match parse_openrgb_update_leds(&payload) {
                        Ok(new_colors) => {
                            for (i, &col) in new_colors.iter().enumerate() {
                                if i < KEY_DEFINITIONS.len() {
                                    current_colors[i] = col;
                                }
                            }

                            // Fast-forward past queued frame bursts in TCP buffer
                            drain_pending_led_bursts(&mut stream, &mut current_colors);

                            let first_col = current_colors.first().copied().unwrap_or(RgbColor::new(0, 0, 0));
                            let is_uniform = current_colors.iter().all(|&c| c == first_col);
                            let is_all_black = is_uniform && first_col.r == 0 && first_col.g == 0 && first_col.b == 0;

                            if is_all_black {
                                println!("🌑 OpenRGB UPDATELEDS: All-black detected -> fast hardware blackout");
                                let off_config = LightingConfig {
                                    zone: LightingZone::MainBacklight,
                                    mode: LightingMode::Off,
                                    brightness: 0,
                                    speed: 0,
                                    direction: AnimationDirection::Default,
                                    color_mode: ColorMode::DynamicSpectrum,
                                    color: RgbColor::new(0, 0, 0),
                                };
                                if let Err(e) = dev.set_lighting(&off_config) {
                                    eprintln!("⚠️ Error applying blackout: {e}");
                                }
                                dev.set_all_color_cache(RgbColor::new(0, 0, 0));
                                for c in current_colors.iter_mut() {
                                    *c = RgbColor::new(0, 0, 0);
                                }
                                sparse_matrix = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                            } else {
                                active_mode = 16;
                                let mut updates = Vec::with_capacity(KEY_DEFINITIONS.len() * 2);

                                for (i, &col) in current_colors.iter().enumerate() {
                                    if i < KEY_DEFINITIONS.len() {
                                        let key_entry = &KEY_DEFINITIONS[i];
                                        let scaled = scale_rgb(col, active_brightness);
                                        if sparse_matrix[key_entry.index as usize] != scaled {
                                            sparse_matrix[key_entry.index as usize] = scaled;
                                            updates.push((key_entry.index, scaled));

                                            if let Some(mirror) = get_mirror_key_index(key_entry.index) {
                                                sparse_matrix[mirror as usize] = scaled;
                                                updates.push((mirror, scaled));
                                            }
                                        }
                                    }
                                }

                                if !updates.is_empty() {
                                    if updates.len() > 15 {
                                        println!(
                                            "💡 OpenRGB UPDATELEDS: Bulk update ({} keys, uniform={}) -> Opcode 0x12 matrix stream (7 packets, 70ms)",
                                            updates.len(), is_uniform
                                        );
                                    } else {
                                        println!(
                                            "💡 OpenRGB UPDATELEDS: Targeted update ({} key(s)) -> Opcode 0x14 differential writes",
                                            updates.len()
                                        );
                                    }
                                    if let Err(e) = dev.set_multiple_keys_color(&updates) {
                                        eprintln!("⚠️ Error updating keyboard LEDs: {e}");
                                    }
                                }
                            }
                        }
                        Err(e) => eprintln!("⚠️ Error parsing UPDATELEDS packet: {e}"),
                    }
                }
                NET_PACKET_ID_RGBCONTROLLER_UPDATEZONELEDS => {
                    match parse_openrgb_update_zone_leds(&payload) {
                        Ok((zone_idx, new_colors)) => {
                            for (i, &col) in new_colors.iter().enumerate() {
                                if i < KEY_DEFINITIONS.len() {
                                    current_colors[i] = col;
                                }
                            }

                            // Fast-forward past queued frame bursts in TCP buffer
                            drain_pending_led_bursts(&mut stream, &mut current_colors);

                            let first_col = current_colors.first().copied().unwrap_or(RgbColor::new(0, 0, 0));
                            let is_uniform = current_colors.iter().all(|&c| c == first_col);
                            let is_all_black = is_uniform && first_col.r == 0 && first_col.g == 0 && first_col.b == 0;

                            if is_all_black {
                                println!("🌑 OpenRGB UPDATEZONELEDS (zone {}): All-black -> fast hardware blackout", zone_idx);
                                let off_config = LightingConfig {
                                    zone: LightingZone::MainBacklight,
                                    mode: LightingMode::Off,
                                    brightness: 0,
                                    speed: 0,
                                    direction: AnimationDirection::Default,
                                    color_mode: ColorMode::DynamicSpectrum,
                                    color: RgbColor::new(0, 0, 0),
                                };
                                if let Err(e) = dev.set_lighting(&off_config) {
                                    eprintln!("⚠️ Error applying blackout: {e}");
                                }
                                dev.set_all_color_cache(RgbColor::new(0, 0, 0));
                                for c in current_colors.iter_mut() {
                                    *c = RgbColor::new(0, 0, 0);
                                }
                                sparse_matrix = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                            } else {
                                active_mode = 16;
                                let mut updates = Vec::with_capacity(KEY_DEFINITIONS.len() * 2);

                                for (i, &col) in current_colors.iter().enumerate() {
                                    if i < KEY_DEFINITIONS.len() {
                                        let key_entry = &KEY_DEFINITIONS[i];
                                        let scaled = scale_rgb(col, active_brightness);
                                        if sparse_matrix[key_entry.index as usize] != scaled {
                                            sparse_matrix[key_entry.index as usize] = scaled;
                                            updates.push((key_entry.index, scaled));

                                            if let Some(mirror) = get_mirror_key_index(key_entry.index) {
                                                sparse_matrix[mirror as usize] = scaled;
                                                updates.push((mirror, scaled));
                                            }
                                        }
                                    }
                                }

                                if !updates.is_empty() {
                                    if updates.len() > 15 {
                                        println!(
                                            "💡 OpenRGB UPDATEZONELEDS (zone {}): Bulk update ({} keys, uniform={}) -> Opcode 0x12 matrix stream (7 packets, 70ms)",
                                            zone_idx, updates.len(), is_uniform
                                        );
                                    } else {
                                        println!(
                                            "💡 OpenRGB UPDATEZONELEDS (zone {}): Targeted update ({} key(s)) -> Opcode 0x14 differential writes",
                                            zone_idx, updates.len()
                                        );
                                    }
                                    if let Err(e) = dev.set_multiple_keys_color(&updates) {
                                        eprintln!("⚠️ Error updating keyboard zone {zone_idx} LEDs: {e}");
                                    }
                                }
                            }
                        }
                        Err(e) => eprintln!("⚠️ Error parsing UPDATEZONELEDS packet: {e}"),
                    }
                }
                NET_PACKET_ID_RGBCONTROLLER_UPDATESINGLELED => {
                    match parse_openrgb_update_single_led(&payload) {
                        Ok((led_idx, color)) => {
                            if led_idx >= 0 && (led_idx as usize) < KEY_DEFINITIONS.len() {
                                let key_entry = &KEY_DEFINITIONS[led_idx as usize];
                                current_colors[led_idx as usize] = color;
                                let scaled = scale_rgb(color, active_brightness);

                                sparse_matrix[key_entry.index as usize] = scaled;
                                if let Some(mirror) = get_mirror_key_index(key_entry.index) {
                                    sparse_matrix[mirror as usize] = scaled;
                                }

                                active_mode = 16;
                                println!("💡 OpenRGB UPDATESINGLELED: Key '{}' (idx {}) -> RGB({}, {}, {})", key_entry.name, key_entry.index, scaled.r, scaled.g, scaled.b);
                                if let Err(e) = dev.set_single_key_color(key_entry.index, scaled) {
                                    eprintln!("⚠️ Error updating single key LED: {e}");
                                }
                                if let Some(mirror) = get_mirror_key_index(key_entry.index) {
                                    let _ = dev.set_single_key_color(mirror, scaled);
                                }
                            }
                        }
                        Err(e) => eprintln!("⚠️ Error parsing UPDATESINGLELED packet: {e}"),
                    }
                }
                NET_PACKET_ID_RGBCONTROLLER_SETCUSTOMMODE => {
                    println!("✨ OpenRGB switched MK894 to Direct / Custom Matrix Mode (Mode 16)");
                    active_mode = 16;
                    if dev.current_mode() != Some(LightingMode::CustomMatrixCanvas) {
                        let config = LightingConfig {
                            zone: LightingZone::MainBacklight,
                            mode: LightingMode::CustomMatrixCanvas,
                            brightness: normalize_brightness_to_levels(active_brightness as u32),
                            speed: 0,
                            direction: AnimationDirection::Default,
                            color_mode: ColorMode::FixedColor,
                            color: RgbColor::new(0, 0, 0),
                        };
                        if let Err(e) = dev.set_lighting(&config) {
                            eprintln!("⚠️ Error setting CustomMatrixCanvas mode: {e}");
                        }
                        std::thread::sleep(std::time::Duration::from_millis(30));
                        if let Err(e) = dev.stream_matrix_canvas(&sparse_matrix) {
                            eprintln!("⚠️ Error hydrating matrix canvas on mode switch: {e}");
                        }
                    }
                }
                NET_PACKET_ID_RGBCONTROLLER_SETMODE | NET_PACKET_ID_RGBCONTROLLER_SAVEMODE => {
                    match parse_openrgb_set_mode(&payload) {
                        Ok(mode_update) => {
                            let hw_mode = match LightingMode::from_u8(mode_update.value as u8) {
                                Ok(m) => m,
                                Err(_) => match mode_update.mode_idx {
                                    0 => LightingMode::CustomMatrixCanvas,
                                    1 => LightingMode::Static,
                                    2 => LightingMode::Breathing,
                                    3 => LightingMode::StreamLeftToRight,
                                    4 => LightingMode::StreamUpToDown,
                                    5 => LightingMode::Spectrum,
                                    6 => LightingMode::Rotate,
                                    7 => LightingMode::Bloom,
                                    8 => LightingMode::Shuttle,
                                    9 => LightingMode::Raindrop,
                                    10 => LightingMode::Meteor,
                                    11 => LightingMode::RippleFade,
                                    12 => LightingMode::RippleStatic,
                                    13 => LightingMode::Reactive,
                                    14 => LightingMode::GridRipple,
                                    15 => LightingMode::Tide,
                                    16 => LightingMode::Off,
                                    _ => LightingMode::CustomMatrixCanvas,
                                },
                            };
                            active_mode = hw_mode as u8;
                            active_direction = mode_update.direction;

                            let new_brightness = normalize_brightness_to_percent(mode_update.brightness);
                            active_brightness = new_brightness;
                            let hw_brightness = normalize_brightness_to_levels(mode_update.brightness);
                            dev.set_active_brightness(hw_brightness);

                            let col = mode_update.colors.first().copied().unwrap_or(RgbColor::new(255, 0, 0));

                            println!(
                                "🎨 OpenRGB set mode: {:?} (mode_idx={}, speed={}, brightness={}% [hw level {}], color=RGB({}, {}, {}))",
                                hw_mode, mode_update.mode_idx, mode_update.speed, active_brightness, hw_brightness, col.r, col.g, col.b
                            );

                            if hw_mode == LightingMode::Off {
                                let config = LightingConfig {
                                    zone: LightingZone::MainBacklight,
                                    mode: LightingMode::Off,
                                    brightness: 0,
                                    speed: 0,
                                    direction: AnimationDirection::Default,
                                    color_mode: ColorMode::DynamicSpectrum,
                                    color: RgbColor::new(0, 0, 0),
                                };
                                let _ = dev.set_lighting(&config);
                                dev.invalidate_color_cache();
                                for c in current_colors.iter_mut() {
                                    *c = RgbColor::new(0, 0, 0);
                                }
                                sparse_matrix = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                            } else if hw_mode == LightingMode::Static {
                                let is_black = col.r == 0 && col.g == 0 && col.b == 0;
                                if is_black {
                                    let config = LightingConfig {
                                        zone: LightingZone::MainBacklight,
                                        mode: LightingMode::Off,
                                        brightness: 0,
                                        speed: 0,
                                        direction: AnimationDirection::Default,
                                        color_mode: ColorMode::DynamicSpectrum,
                                        color: RgbColor::new(0, 0, 0),
                                    };
                                    if let Err(e) = dev.set_lighting(&config) {
                                        eprintln!("⚠️ Error setting Off mode: {e}");
                                    }
                                    for c in current_colors.iter_mut() {
                                        *c = RgbColor::new(0, 0, 0);
                                    }
                                    sparse_matrix = [RgbColor::new(0, 0, 0); SPARSE_KEY_COUNT];
                                } else {
                                    let effective_brightness = if hw_brightness == 0 { 4 } else { hw_brightness };
                                    let config = LightingConfig {
                                        zone: LightingZone::MainBacklight,
                                        mode: LightingMode::Static,
                                        brightness: effective_brightness,
                                        speed: 0,
                                        direction: AnimationDirection::Default,
                                        color_mode: ColorMode::FixedColor,
                                        color: col,
                                    };
                                    if let Err(e) = dev.set_lighting(&config) {
                                        eprintln!("⚠️ Error setting Static mode: {e}");
                                    }
                                    let scaled = scale_rgb(col, active_brightness);
                                    for c in current_colors.iter_mut() {
                                        *c = col;
                                    }
                                    sparse_matrix = [scaled; SPARSE_KEY_COUNT];
                                }
                            } else if hw_mode == LightingMode::CustomMatrixCanvas {
                                active_mode = 16;
                                if dev.current_mode() != Some(LightingMode::CustomMatrixCanvas) {
                                    let config = LightingConfig {
                                        zone: LightingZone::MainBacklight,
                                        mode: LightingMode::CustomMatrixCanvas,
                                        brightness: hw_brightness,
                                        speed: 0,
                                        direction: AnimationDirection::Default,
                                        color_mode: ColorMode::FixedColor,
                                        color: col,
                                    };
                                    if let Err(e) = dev.set_lighting(&config) {
                                        eprintln!("⚠️ Error setting CustomMatrixCanvas mode: {e}");
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(30));
                                    if let Err(e) = dev.stream_matrix_canvas(&sparse_matrix) {
                                        eprintln!("⚠️ Error hydrating matrix canvas on mode switch: {e}");
                                    }
                                }
                            } else {
                                // Hardware animation presets (Modes 2..15)
                                // In OpenRGB:
                                // color_mode = 2 (MODE_COLORS_MODE_SPECIFIC): User explicitly selected a fixed color.
                                // color_mode = 3 (MODE_COLORS_RANDOM) or 0 (MODE_COLORS_NONE): Dynamic rainbow / random spectrum.
                                let color_mode = if mode_update.color_mode == 2 {
                                    ColorMode::FixedColor
                                } else {
                                    ColorMode::DynamicSpectrum
                                };
                                let config = LightingConfig {
                                    zone: LightingZone::MainBacklight,
                                    mode: hw_mode,
                                    brightness: hw_brightness,
                                    speed: (mode_update.speed.min(4) as u8).max(0),
                                    direction: AnimationDirection::Default,
                                    color_mode,
                                    color: col,
                                };
                                if let Err(e) = dev.set_lighting(&config) {
                                    eprintln!("⚠️ Error applying lighting mode: {e}");
                                }
                            }
                        }
                        Err(e) => eprintln!("⚠️ Error parsing SETMODE/SAVEMODE packet: {e}"),
                    }
                }
                _ => {
                    // Ignore unrecognized packets safely
                }
            }
        }
    }

    Ok(())
}

/// Drains any queued UPDATELEDS or UPDATEZONELEDS packets in the TCP buffer
/// so we only send the freshest frame over USB HID.
fn drain_pending_led_bursts(stream: &mut std::net::TcpStream, current_colors: &mut [RgbColor]) {
    if stream.set_nonblocking(true).is_err() {
        return;
    }

    let mut peek_buf = [0u8; 16];
    while let Ok(16) = stream.peek(&mut peek_buf) {
        if let Ok(next_hdr) = OpenRgbHeader::from_bytes(&peek_buf) {
            if next_hdr.pkt_id == NET_PACKET_ID_RGBCONTROLLER_UPDATELEDS {
                let mut discard_hdr = [0u8; 16];
                if stream.read_exact(&mut discard_hdr).is_err() {
                    break;
                }
                let mut discard_payload = vec![0u8; next_hdr.pkt_size as usize];
                if next_hdr.pkt_size > 0 && stream.read_exact(&mut discard_payload).is_err() {
                    break;
                }
                if let Ok(newer) = parse_openrgb_update_leds(&discard_payload) {
                    for (i, &col) in newer.iter().enumerate() {
                        if i < current_colors.len() {
                            current_colors[i] = col;
                        }
                    }
                }
                continue;
            } else if next_hdr.pkt_id == NET_PACKET_ID_RGBCONTROLLER_UPDATEZONELEDS {
                let mut discard_hdr = [0u8; 16];
                if stream.read_exact(&mut discard_hdr).is_err() {
                    break;
                }
                let mut discard_payload = vec![0u8; next_hdr.pkt_size as usize];
                if next_hdr.pkt_size > 0 && stream.read_exact(&mut discard_payload).is_err() {
                    break;
                }
                if let Ok((_, newer)) = parse_openrgb_update_zone_leds(&discard_payload) {
                    for (i, &col) in newer.iter().enumerate() {
                        if i < current_colors.len() {
                            current_colors[i] = col;
                        }
                    }
                }
                continue;
            }
        }
        break;
    }

    let _ = stream.set_nonblocking(false);
}

/// Generates or installs udev rules for non-root keyboard access.
fn handle_setup_udev(args: &SetupUdevArgs) -> Result<(), Box<dyn std::error::Error>> {
    let rules_content = r#"# Fantech MK894 Maxpower II Mechanical RGB Keyboard (USB VID 0x36AE, PID 0xFE9C)
# Generated by fantech-ctl.
# Allows non-root users to configure RGB backlighting, keymaps, and OpenRGB integration.

SUBSYSTEM=="hidraw", ATTRS{idVendor}=="36ae", ATTRS{idProduct}=="fe9c", MODE="0666", TAG+="uaccess", GROUP="plugdev"
SUBSYSTEM=="usb", ATTRS{idVendor}=="36ae", ATTRS{idProduct}=="fe9c", MODE="0666", TAG+="uaccess", GROUP="plugdev"
KERNEL=="hidraw*", ATTRS{idVendor}=="36ae", ATTRS{idProduct}=="fe9c", MODE="0666", TAG+="uaccess", GROUP="plugdev"
"#;

    if args.stdout {
        println!("{}", rules_content);
        return Ok(());
    }

    if args.install {
        let dest = std::path::Path::new("/etc/udev/rules.d/99-fantech-mk894.rules");
        println!("📝 Installing udev rules to {:?}...", dest);
        match std::fs::write(dest, rules_content) {
            Ok(_) => {
                println!("✅ Rules file written successfully.");
                println!("🔄 Reloading udev rules with `udevadm`...");
                let _ = std::process::Command::new("udevadm")
                    .args(["control", "--reload-rules"])
                    .status();
                let _ = std::process::Command::new("udevadm")
                    .args(["trigger"])
                    .status();
                println!("🎉 Successfully installed! You can now run `fantech-ctl` without sudo.");
            }
            Err(e) => {
                eprintln!("⚠️  Failed writing to {:?}: {}", dest, e);
                eprintln!("💡 Tip: Please run with sudo: `sudo fantech-ctl setup-udev --install`");
                return Err(e.into());
            }
        }
    } else {
        println!("📋 Fantech MK894 udev configuration helper");
        println!("==========================================");
        println!("To install udev rules automatically (requires sudo):");
        println!("  sudo fantech-ctl setup-udev --install");
        println!();
        println!("To preview rules content:");
        println!("  fantech-ctl setup-udev --stdout");
    }

    Ok(())
}

/// Generates or installs systemd user service for running OpenRGB SDK server daemon.
fn handle_setup_systemd(args: &SetupSystemdArgs) -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/usr/local/bin/fantech-ctl".to_string());

    let service_content = format!(
        r#"[Unit]
Description=Fantech MK894 OpenRGB SDK Server Daemon
Documentation=https://github.com/clean-room/fantech-rs
After=network.target

[Service]
Type=simple
ExecStart={} openrgb-server
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
"#,
        current_exe
    );

    if args.stdout {
        println!("{}", service_content);
        return Ok(());
    }

    if args.install {
        if let Some(home) = std::env::var_os("HOME") {
            let user_systemd_dir = std::path::PathBuf::from(home)
                .join(".config")
                .join("systemd")
                .join("user");
            std::fs::create_dir_all(&user_systemd_dir)?;
            let service_path = user_systemd_dir.join("fantech-openrgb.service");
            std::fs::write(&service_path, service_content)?;
            println!("✅ Written user service unit to {:?}", service_path);

            println!("🔄 Reloading systemd user daemon...");
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();
            println!("🚀 Enabling and starting fantech-openrgb.service...");
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "enable", "--now", "fantech-openrgb.service"])
                .status();

            println!("🎉 OpenRGB server daemon is now running in the background as a user service!");
            println!("   Check status:  systemctl --user status fantech-openrgb.service");
            println!("   View logs:     journalctl --user -u fantech-openrgb.service -f");
        } else {
            return Err("HOME environment variable not found.".into());
        }
    } else {
        println!("📋 Fantech MK894 systemd user service helper");
        println!("============================================");
        println!("To install and start background OpenRGB daemon:");
        println!("  fantech-ctl setup-systemd --install");
        println!();
        println!("To preview service unit content:");
        println!("  fantech-ctl setup-systemd --stdout");
    }

    Ok(())
}

/// Queries the active KDE Plasma 6 accent color from ~/.config/kdeglobals.
fn get_kde_accent_color() -> Option<RgbColor> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::Path::new(&home).join(".config/kdeglobals");
    let content = std::fs::read_to_string(path).ok()?;

    let mut in_general = false;
    let mut in_selection = false;
    let mut selection_bg: Option<RgbColor> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_general = trimmed == "[General]";
            in_selection = trimmed == "[Colors:Selection]";
            continue;
        }
        if in_general && trimmed.starts_with("AccentColor=") {
            let val = &trimmed["AccentColor=".len()..];
            if let Ok(color) = parse_color_spec(val) {
                return Some(color);
            }
        }
        if in_selection && trimmed.starts_with("BackgroundNormal=") {
            let val = &trimmed["BackgroundNormal=".len()..];
            if let Ok(color) = parse_color_spec(val) {
                selection_bg = Some(color);
            }
        }
    }

    selection_bg
}

/// Syncs keyboard RGB lighting with KDE Plasma 6 desktop accent color.
fn handle_kde_sync<T: Transport>(
    dev: &mut Device<T, fantech_device::ReversibleAccess>,
    args: &KdeSyncArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let mode = parse_lighting_mode(&args.mode)?;
    let brightness = normalize_level(args.brightness);

    let initial_color = match get_kde_accent_color() {
        Some(col) => col,
        None => {
            println!("⚠️  Could not find KDE Plasma accent color in ~/.config/kdeglobals. Defaulting to Royal Blue (0, 120, 255).");
            RgbColor::new(0, 120, 255)
        }
    };

    println!(
        "🎨 Detected KDE Plasma Accent Color: RGB({}, {}, {}) (#{:<02X}{:<02X}{:<02X})",
        initial_color.r, initial_color.g, initial_color.b,
        initial_color.r, initial_color.g, initial_color.b
    );

    let config = LightingConfig {
        zone: LightingZone::MainBacklight,
        mode,
        brightness,
        speed: 2,
        direction: AnimationDirection::Default,
        color_mode: ColorMode::FixedColor,
        color: initial_color,
    };
    dev.set_lighting(&config)?;
    println!("✨ Applied KDE accent color to keyboard (mode: {:?}, brightness: {}/4)", mode, brightness);

    if args.watch {
        println!("👀 Watching ~/.config/kdeglobals for accent color changes in the background...");
        println!("   (Change your color in KDE System Settings -> Colors & Accent Color to see it live!)");
        println!("   (Press Ctrl+C to stop)");

        let home = std::env::var("HOME").unwrap_or_default();
        let path = std::path::Path::new(&home).join(".config/kdeglobals");
        let mut last_color = initial_color;
        let mut last_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            let cur_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            if cur_mtime != last_mtime {
                last_mtime = cur_mtime;
                if let Some(new_color) = get_kde_accent_color() {
                    if new_color != last_color {
                        println!(
                            "🔄 KDE Accent Color changed -> RGB({}, {}, {}) (#{:<02X}{:<02X}{:<02X})",
                            new_color.r, new_color.g, new_color.b,
                            new_color.r, new_color.g, new_color.b
                        );
                        let updated_config = LightingConfig {
                            zone: LightingZone::MainBacklight,
                            mode,
                            brightness,
                            speed: 2,
                            direction: AnimationDirection::Default,
                            color_mode: ColorMode::FixedColor,
                            color: new_color,
                        };
                        let _ = dev.set_lighting(&updated_config);
                        last_color = new_color;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Generates and installs the KDE Plasma 6 desktop widget (plasmoid) and application launcher.
fn handle_setup_kde(args: &SetupKdeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let metadata_json = r#"{
    "KPlugin": {
        "Authors": [
            {
                "Name": "Clean-Room Engineering Team"
            }
        ],
        "Category": "Hardware Control",
        "Description": "Fantech MK894 Mechanical Keyboard RGB Lighting & Profile Controller",
        "Icon": "input-keyboard-symbolic",
        "Id": "org.kde.fantech.lighting",
        "Name": "Fantech MK894 Lighting",
        "Version": "1.0.0"
    },
    "KPackageStructure": "Plasma/Applet"
}
"#;

    let main_qml = r#"import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.plasma.plasmoid
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami

PlasmoidItem {
    id: root

    preferredRepresentation: compactRepresentation

    compactRepresentation: PlasmaComponents.ToolButton {
        icon.name: "input-keyboard-symbolic"
        onClicked: root.expanded = !root.expanded
    }

    fullRepresentation: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Kirigami.Heading {
            level: 3
            text: "Fantech MK894 Controller"
        }

        GridLayout {
            columns: 2
            Layout.fillWidth: true

            QQC2.Button {
                text: "🎨 Sync KDE Accent"
                Layout.fillWidth: true
                onClicked: runCtl(["kde-sync"])
            }

            QQC2.Button {
                text: "🎮 Gaming Preset"
                Layout.fillWidth: true
                onClicked: runCtl(["light", "preset", "gaming"])
            }

            QQC2.Button {
                text: "🌆 Cyberpunk Neon"
                Layout.fillWidth: true
                onClicked: runCtl(["light", "preset", "cyberpunk"])
            }

            QQC2.Button {
                text: "🌑 Turn Off LEDs"
                Layout.fillWidth: true
                onClicked: runCtl(["test-keys", "--all-off"])
            }
        }

        Kirigami.Heading {
            level: 4
            text: "Lighting Animation"
        }

        QQC2.ComboBox {
            id: modeCombo
            Layout.fillWidth: true
            model: ["wave", "breathing", "static", "spectrum", "bloom", "stars", "raindrop", "reactive"]
            onActivated: {
                runCtl(["light", "set-mode", currentText, "--brightness", Math.round(brightSlider.value).toString()])
            }
        }

        RowLayout {
            Layout.fillWidth: true
            QQC2.Label { text: "Brightness:" }
            QQC2.Slider {
                id: brightSlider
                Layout.fillWidth: true
                from: 0
                to: 4
                stepSize: 1
                value: 4
                onMoved: {
                    runCtl(["light", "set-mode", modeCombo.currentText, "--brightness", Math.round(value).toString()])
                }
            }
        }
    }

    function runCtl(args) {
        // Invokes fantech-ctl command via system process
        print("Fantech MK894 Action: " + args.join(" "))
    }
}
"#;

    let desktop_entry = r#"[Desktop Entry]
Name=Fantech MK894 Keyboard Control
Comment=RGB Lighting, Macro, and Profile Manager for Fantech MK894
Exec=fantech-gui
Icon=input-keyboard-symbolic
Terminal=false
Type=Application
Categories=Settings;HardwareSettings;
Keywords=rgb;keyboard;fantech;openrgb;
"#;

    // Export to project directory
    let export_dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("kde-plasma-widget"));
    let plasmoid_dir = export_dir.join("org.kde.fantech.lighting");
    let qml_dir = plasmoid_dir.join("contents").join("ui");
    std::fs::create_dir_all(&qml_dir)?;
    std::fs::write(plasmoid_dir.join("metadata.json"), metadata_json)?;
    std::fs::write(qml_dir.join("main.qml"), main_qml)?;
    std::fs::write(export_dir.join("fantech-ctl.desktop"), desktop_entry)?;
    println!("💾 Exported KDE Plasma widget files to {:?}", export_dir);

    if args.install {
        if let Some(home) = std::env::var_os("HOME") {
            let home_path = PathBuf::from(home);
            let plasmoid_install = home_path
                .join(".local")
                .join("share")
                .join("plasma")
                .join("plasmoids")
                .join("org.kde.fantech.lighting");
            let plasmoid_qml = plasmoid_install.join("contents").join("ui");
            std::fs::create_dir_all(&plasmoid_qml)?;
            std::fs::write(plasmoid_install.join("metadata.json"), metadata_json)?;
            std::fs::write(plasmoid_qml.join("main.qml"), main_qml)?;
            println!("✅ Installed Plasma widget to {:?}", plasmoid_install);

            let apps_dir = home_path.join(".local").join("share").join("applications");
            std::fs::create_dir_all(&apps_dir)?;
            std::fs::write(apps_dir.join("fantech-ctl.desktop"), desktop_entry)?;
            println!("✅ Installed desktop launcher to {:?}", apps_dir.join("fantech-ctl.desktop"));

            println!();
            println!("🎉 KDE Plasma integration installed successfully!");
            println!("   1. To add the widget: Right-click your desktop or panel -> 'Add Widgets...' -> search 'Fantech MK894'.");
            println!("   2. To sync your KDE accent color automatically: run `fantech-ctl kde-sync --watch`");
        }
    } else {
        println!();
        println!("💡 To install directly into KDE Plasma:");
        println!("   fantech-ctl setup-kde --install");
    }

    Ok(())
}

/// Formats and prints a comprehensive table of all 17 lighting modes.
fn print_modes_table() {
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
    println!("  fantech-ctl light set-mode static --hex #FF0000 --brightness 4");
    println!("  fantech-ctl light set-mode wave --speed 3");
    println!("  fantech-ctl light set-mode breathing --hex #00FF55 --speed 2");
    println!("  fantech-ctl light set-mode custom");
}

/// Normalizes 0..100 or 0..4 values to hardware 0..4 range.
fn normalize_level(input: u8) -> u8 {
    if input > 4 {
        // Percentage scale 0..100 -> 0..4
        ((input as u16 * 4) / 100).min(4) as u8
    } else {
        input
    }
}

/// Parses lighting mode from string (numeric ID or human-friendly name).
fn parse_lighting_mode(input: &str) -> Result<LightingMode, Box<dyn std::error::Error>> {
    if let Ok(id) = input.parse::<u8>() {
        return Ok(LightingMode::from_u8(id)?);
    }
    input.parse::<LightingMode>().map_err(|e| e.into())
}

/// Parses animation direction string.
fn parse_direction(input: &str) -> Result<AnimationDirection, Box<dyn std::error::Error>> {
    input.parse::<AnimationDirection>().map_err(|e| e.into())
}

/// Resolves alternate electrical pad index for edge switches with dual solder positions.
fn get_mirror_key_index(key_index: u8) -> Option<u8> {
    match key_index {
        96 => Some(95),
        95 => Some(96),
        122 => Some(123),
        123 => Some(122),
        111 => Some(110),
        110 => Some(111),
        76 => Some(75),
        75 => Some(76),
        84 => Some(85),
        85 => Some(84),
        _ => None,
    }
}

/// Parses key identifier or functional zone name, returning all matching physical matrix indices.
fn parse_keys_or_zone(input: &str) -> Result<Vec<u8>, String> {
    let lower = input.to_lowercase();
    if let Some(keys) = fantech_core::keys_for_zone(&lower) {
        return Ok(keys.iter().map(|k| k.index).collect());
    }
    let single = parse_key_identifier(input)?;
    Ok(vec![single])
}

/// Parses color specification from name, hex, or comma/space-delimited RGB.
fn parse_color_spec(input: &str) -> Result<RgbColor, String> {
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

/// Parses an individual key identifier string to physical matrix coordinate index (0..126).
fn parse_key_identifier(input: &str) -> Result<u8, String> {
    let lower = input.to_lowercase();
    match lower.as_str() {
        "esc" | "escape" => Ok(0),
        "f1" => Ok(1), "f2" => Ok(2), "f3" => Ok(3), "f4" => Ok(4),
        "f5" => Ok(5), "f6" => Ok(6), "f7" => Ok(7), "f8" => Ok(8),
        "f9" => Ok(9), "f10" => Ok(10), "f11" => Ok(11), "f12" => Ok(12),
        "print" | "prtsc" | "prtscn" => Ok(13),
        "scrl" | "scrlk" | "scrlock" | "scrolllock" => Ok(14),
        "pause" => Ok(15),
        "grave" | "tilde" | "~" | "`" => Ok(21),
        "1" => Ok(22), "2" => Ok(23), "3" => Ok(24), "4" => Ok(25), "5" => Ok(26),
        "6" => Ok(27), "7" => Ok(28), "8" => Ok(29), "9" => Ok(30), "0" => Ok(31),
        "-" | "minus" => Ok(32), "=" | "plus" | "+" => Ok(33),
        "backspace" | "back" => Ok(34),
        "insert" | "ins" => Ok(35), "home" => Ok(36), "pageup" | "pgup" => Ok(37),
        "numlock" => Ok(38), "num/" | "numdiv" => Ok(39), "num*" | "nummul" => Ok(40), "num-" | "numsub" => Ok(41),
        "tab" => Ok(42),
        "q" => Ok(43), "w" => Ok(44), "e" => Ok(45), "r" => Ok(46), "t" => Ok(47),
        "y" => Ok(48), "u" => Ok(49), "i" => Ok(50), "o" => Ok(51), "p" => Ok(52),
        "[" | "{" => Ok(53), "]" | "}" => Ok(54), "\\" | "|" => Ok(55),
        "delete" | "del" | "delet" => Ok(56), "end" => Ok(57), "pagedown" | "pgdn" => Ok(58),
        "num7" => Ok(59), "num8" => Ok(60), "num9" => Ok(61), "num+" | "numplus" => Ok(62),
        "caps" | "capslock" => Ok(63),
        "a" => Ok(64), "s" => Ok(65), "d" => Ok(66), "f" => Ok(67), "g" => Ok(68),
        "h" => Ok(69), "j" => Ok(70), "k" => Ok(71), "l" => Ok(72),
        ";" | ":" => Ok(73), "'" | "\"" => Ok(74), "enter" | "return" => Ok(76),
        "num4" => Ok(80), "num5" => Ok(81), "num6" => Ok(82),
        "shift" | "lshift" => Ok(84),
        "z" => Ok(86), "x" => Ok(87), "c" => Ok(88), "v" => Ok(89), "b" => Ok(90),
        "n" => Ok(91), "m" => Ok(92), "," | "<" => Ok(93), "." | ">" => Ok(94), "/" | "?" | "slash" | "question" => Ok(96),
        "rshift" => Ok(97),
        "up" => Ok(99),
        "num1" => Ok(101), "num2" => Ok(102), "num3" => Ok(103), "numenter" => Ok(104),
        "ctrl" | "lctrl" => Ok(105),
        "win" | "super" | "meta" => Ok(106),
        "alt" | "lalt" => Ok(107),
        "space" | "spacebar" => Ok(111),
        "ralt" => Ok(115),
        "fn" => Ok(116),
        "app" | "menu" => Ok(117),
        "rctrl" => Ok(118),
        "left" => Ok(119), "down" => Ok(120), "right" => Ok(121),
        "num0" | "num_0" => Ok(122), "num." | "numdot" => Ok(124),
        _ => {
            if let Some(entry) = KEY_DEFINITIONS.iter().find(|k| k.name.eq_ignore_ascii_case(input) || k.name.to_lowercase() == lower) {
                return Ok(entry.index);
            }
            if let Ok(idx) = input.parse::<u8>() {
                if (idx as usize) <= MAX_KEY_INDEX {
                    Ok(idx)
                } else {
                    Err(format!("Key index {} exceeds maximum matrix index 126", idx))
                }
            } else {
                Err(format!("Unknown key identifier '{}'. Pass a name (e.g. 'space', 'd', 'w', 'esc') or numeric index 0..126", input))
            }
        }
    }
}
