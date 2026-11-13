#![deny(unsafe_code)]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[allow(unsafe_code)]
mod glibc_compat {
    use std::arch::global_asm;
    // Bind math symbols to base GLIBC 2.2.5 so binaries compiled on rolling-release
    // distributions (like CachyOS/Arch with GLIBC 2.43+) remain backward compatible
    // with stable runtime environments (like FreeDesktop Flatpak 26.08, Debian, Ubuntu).
    global_asm!(".symver atan2f, atan2f@GLIBC_2.2.5");
    global_asm!(".symver acosf, acosf@GLIBC_2.2.5");

    // Rust std dynamically links pidfd_spawnp / pidfd_getpid weakly from GLIBC 2.39+.
    // Defining them here prevents undefined dynamic references to GLIBC_2.39, ensuring
    // seamless compatibility with GLIBC 2.34-2.40+ runtimes (e.g. FreeDesktop Flatpak 26.08).
    #[no_mangle]
    pub extern "C" fn pidfd_spawnp() -> i32 {
        unsafe {
            *libc::__errno_location() = libc::ENOSYS;
        }
        -1
    }

    #[no_mangle]
    pub extern "C" fn pidfd_getpid() -> i32 {
        unsafe {
            *libc::__errno_location() = libc::ENOSYS;
        }
        -1
    }
}

mod service;
mod tray;
#[cfg(target_os = "linux")]
pub(crate) mod volume_guard;

use clap::{Args, Parser, Subcommand};
use fantastech_core::{
    default_layer_keymap, format_action_str, is_service_running, parse_action_str,
    parse_key_identifier, send_ipc_request, ButtonProfile, FantasTechConfig, GuiTheme, IpcRequest,
    IpcResponse, LayerKeymapConfig, LightingProfile, MacroProfile, MacroSequence,
    ProfileKeymapConfig, ProfileManager, SettingsProfile, ThemeConfig, KEY_DEFINITIONS,
};
use fantastech_device::{Device, FullDeviceBackup, ReadOnly};
use fantastech_emulator::VirtualKeyboardEmulator;
use fantastech_protocol::constants::{MAX_KEY_INDEX, SPARSE_KEY_COUNT};
use fantastech_protocol::keymap::KeyLayer;
use fantastech_protocol::lighting::{
    AnimationDirection, ColorMode, LightingConfig, LightingMode, LightingZone, RgbColor,
};
use fantastech_protocol::macro_mem::MacroMemoryTable;
use fantastech_protocol::openrgb::{
    build_openrgb_controller_data, normalize_brightness_to_levels, normalize_brightness_to_percent,
    parse_openrgb_set_mode, parse_openrgb_update_leds, parse_openrgb_update_single_led,
    parse_openrgb_update_zone_leds, resolve_openrgb_mode, scale_rgb, OpenRgbHeader,
    NET_PACKET_ID_REQUEST_CONTROLLER_COUNT, NET_PACKET_ID_REQUEST_CONTROLLER_DATA,
    NET_PACKET_ID_REQUEST_PROTOCOL_VERSION, NET_PACKET_ID_RGBCONTROLLER_SAVEMODE,
    NET_PACKET_ID_RGBCONTROLLER_SETCUSTOMMODE, NET_PACKET_ID_RGBCONTROLLER_SETMODE,
    NET_PACKET_ID_RGBCONTROLLER_UPDATELEDS, NET_PACKET_ID_RGBCONTROLLER_UPDATESINGLELED,
    NET_PACKET_ID_RGBCONTROLLER_UPDATEZONELEDS, NET_PACKET_ID_SET_CLIENT_NAME,
};
use fantastech_transport::{HidrawTransport, Transport};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(
    name = "fantastech",
    version,
    about = "FantasTech — Professional Driver & RGB Control Suite for Fantech MK894",
    long_about = "FantasTech is an open-source driver, multi-domain profile manager, and unified control suite for the Fantech MK894 Maxpower II Mechanical RGB Keyboard (USB VID 0x36AE, PID 0xFE9C)."
)]
struct Cli {
    /// Launch the native graphical interface (FantasTech GUI)
    #[arg(long, global = true)]
    gui: bool,

    /// Show debug CMD console window when running GUI
    #[arg(long, alias = "console", global = true)]
    debug: bool,

    /// Use simulated virtual keyboard emulator instead of physical USB device
    #[arg(short = 'e', long, global = true)]
    emulator: bool,

    /// Explicit device node path (e.g. /dev/hidraw4)
    #[arg(short, long, global = true)]
    device: Option<PathBuf>,

    /// Dry-run mode: display generated report bytes without transmitting to hardware
    #[arg(long, global = true)]
    dry_run: bool,

    /// Bypass background service Unix domain socket and force direct communication
    #[arg(long, global = true)]
    no_ipc: bool,

    /// Output responses in structured JSON format
    #[arg(long, global = true)]
    json: bool,

    /// Enable verbose logging output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Install FantasTech suite to system (desktop launcher, CLI binary, udev rules, service)
    Install(InstallArgs),

    /// Completely uninstall FantasTech suite from system
    Uninstall(UninstallArgs),

    /// Launch the native graphical interface (fantastech-gui)
    Gui(GuiArgs),

    /// Unified background service commands (run, start, stop, restart, status, install, uninstall)
    Service(ServiceArgs),

    /// Launch system tray taskbar applet
    Tray,

    /// Query device identification, firmware version, and active profile
    Info,

    /// Lighting configuration subcommands (preset, set-mode, off, profile, canvas)
    Light(LightArgs),

    /// Multi-domain profile manager and hardware profile switching
    Profile(ProfileArgs),

    /// System configuration controls (show, get, set)
    Config(ConfigArgs),

    /// Key remapping commands
    Remap(RemapArgs),

    /// 4KB hardware macro memory subcommands
    Macro(MacroArgs),

    /// Read-before-write state backup or full JSON snapshot export/restore
    Backup(BackupArgs),

    /// Device settings and hardware maintenance (sleep timeout, factory reset)
    Setting(SettingArgs),

    /// Exclusive volume knob guard daemon against KDE Wayland volume runaway
    #[command(name = "guard-volume", alias = "volume-guard")]
    GuardVolume(GuardVolumeArgs),

    /// Run background OpenRGB SDK Server daemon (TCP port 6742) for OpenRGB apps
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

    /// Generate or install systemd user service for volume knob guard
    #[command(name = "setup-volume-guard", alias = "setup_volume_guard")]
    SetupVolumeGuard(SetupVolumeGuardArgs),

    /// Interactive matrix mapping and key verification test suite
    TestKeys(TestKeysArgs),

    /// Firmware update, check, and flashing commands
    Firmware(FirmwareArgs),
}

#[derive(Args, Debug, Clone, Default)]
pub struct GuiArgs {
    /// Navigate directly to the Settings / Firmware Update section
    #[arg(long)]
    pub update: bool,
}

#[derive(Args, Debug)]
struct ServiceArgs {
    /// Service subcommand (run, start, stop, restart, status, install, uninstall)
    #[command(subcommand)]
    command: Option<ServiceSubcommands>,

    /// Direct action string if subcommand is omitted (e.g. run, start, stop, status)
    action: Option<String>,
}

#[derive(Subcommand, Debug)]
enum ServiceSubcommands {
    /// Run unified service in the foreground (used by systemd user unit)
    Run,
    /// Start background service
    Start,
    /// Stop background service
    Stop,
    /// Restart background service
    Restart,
    /// Query background service status
    Status,
    /// Install and enable systemd user unit
    Install,
    /// Disable and remove systemd user unit
    Uninstall,
}

#[derive(Args, Debug)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommands,
}

#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Show full active configuration JSON
    Show,
    /// Get a specific configuration key, or show all if key is omitted
    Get { key: Option<String> },
    /// Set a configuration key (e.g. openrgb-enabled true, openrgb-port 6742, guard-volume false, theme dark)
    Set { key: String, value: String },
    /// Display the configuration file path
    Path,
    /// Reset configuration to default values
    Reset,
}

#[derive(Args, Debug)]
struct ProfileArgs {
    /// Target profile index to switch to (0..2)
    index: Option<u8>,

    /// Query active profile from hardware
    #[arg(short, long)]
    current: bool,

    #[command(subcommand)]
    command: Option<ProfileCommands>,
}

#[derive(Subcommand, Debug)]
enum ProfileCommands {
    /// Switch active hardware onboard profile (0..2)
    Select {
        /// Target profile index (0..2)
        index: u8,
    },
    /// Query active profile from hardware
    Current,
    /// List all profiles across domains (lighting, buttons, macros, settings)
    List {
        /// Optional domain filter: lighting, buttons, macros, settings
        domain: Option<String>,
        /// Optional domain filter via flag
        #[arg(long = "type", visible_alias = "domain")]
        profile_type: Option<String>,
    },
    /// Apply a saved profile by domain and ID (e.g. 'fantastech profile apply lighting gaming' or 'fantastech profile apply gaming --type lighting')
    Apply {
        /// First argument: domain or profile ID
        arg1: String,
        /// Second argument: profile ID (when domain given as first argument)
        arg2: Option<String>,
        /// Optional domain flag: lighting, buttons, macros, settings
        #[arg(long = "type", visible_alias = "domain")]
        profile_type: Option<String>,
    },
    /// Save or create a profile for a domain
    Save {
        /// Domain: lighting, button, macro, setting
        domain: String,
        /// Profile ID
        id: String,
        /// Optional human-readable name
        #[arg(long)]
        name: Option<String>,
        /// Optional description
        #[arg(long)]
        description: Option<String>,
        /// Optional path to a JSON file containing the profile data
        #[arg(long)]
        file: Option<PathBuf>,
        /// Inactivity sleep timeout in minutes (for settings domain)
        #[arg(long)]
        sleep: Option<u16>,
    },
    /// Delete a custom profile by domain and ID
    Delete {
        /// First argument: domain or profile ID
        arg1: String,
        /// Second argument: profile ID (when domain given as first argument)
        arg2: Option<String>,
        /// Optional domain flag: lighting, buttons, macros, settings
        #[arg(long = "type", visible_alias = "domain")]
        profile_type: Option<String>,
    },
}

#[derive(Args, Debug)]
struct OpenRgbServerArgs {
    /// TCP port to listen on for OpenRGB SDK clients (default: 6742)
    #[arg(short, long, default_value = "6742")]
    port: u16,

    /// IP address to bind to (default: 127.0.0.1)
    #[arg(short, long, default_value = "127.0.0.1")]
    bind: String,
}

#[derive(Args, Debug, Default)]
struct InstallArgs {
    /// Skip sudo/pkexec udev rule installation
    #[arg(long)]
    no_udev: bool,

    /// Skip systemd user service registration
    #[arg(long)]
    no_service: bool,
}

#[derive(Args, Debug, Default)]
struct UninstallArgs {
    /// Also purge user configuration directory (~/.config/fantastech)
    #[arg(long)]
    purge: bool,
}

#[derive(Args, Debug)]
struct SetupUdevArgs {
    /// Install rules directly to /etc/udev/rules.d/99-fantastech.rules (requires sudo)
    #[arg(short, long)]
    install: bool,

    /// Print udev rules to stdout
    #[arg(long)]
    stdout: bool,
}

#[derive(Args, Debug)]
struct SetupSystemdArgs {
    /// Install systemd user service to ~/.config/systemd/user/fantastech.service
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
    /// Install plasmoid to ~/.local/share/plasma/plasmoids/org.kde.fantastech.lighting and application launcher
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

    /// Turn off all keyboard backlighting immediately
    Off,

    /// Apply a saved lighting profile by identifier (e.g. gaming, cyberpunk, rainbow_wave, neon_sunset, matrix, static_purple)
    Profile {
        /// Profile ID
        id: String,
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
    #[command(subcommand)]
    command: RemapCommands,
}

#[derive(Subcommand, Debug)]
enum RemapCommands {
    /// Remap a key to a specific action (e.g. 'a', 'ctrl+c', 'media:play', 'macro:0', 'mouse:left')
    Set {
        /// Physical matrix key index (0..126) or key name (e.g. 'space', 'caps', 'f1')
        key: String,
        /// Target action string (e.g. 'a', 'b', 'ctrl+c', 'win+d', 'media:mute', 'macro:0', 'custom:win_lock')
        action: String,
        /// Target layer: 0/base or 1/fn (default: base)
        #[arg(short, long, default_value = "base")]
        layer: String,
        /// Commit persistent write to hardware flash memory
        #[arg(long)]
        commit: bool,
    },
    /// Query the current action for a key
    Get {
        /// Physical matrix key index (0..126) or key name (e.g. 'space', 'caps')
        key: String,
        /// Target layer: 0/base or 1/fn (default: base)
        #[arg(short, long, default_value = "base")]
        layer: String,
    },
    /// Dump the entire keymap layer to stdout (human-readable or JSON)
    Dump {
        /// Target layer: 0/base or 1/fn (default: base)
        #[arg(short, long, default_value = "base")]
        layer: String,
        /// Output formatted JSON
        #[arg(long)]
        json: bool,
    },
    /// Reset a keymap layer to factory defaults
    Reset {
        /// Target layer: 0/base or 1/fn (default: base)
        #[arg(short, long, default_value = "base")]
        layer: String,
        /// Commit persistent write to hardware flash memory
        #[arg(long)]
        commit: bool,
    },
    /// Export keymap layer or full profile to a file (.json or .toml)
    Export {
        /// Output file path (.json or .toml)
        file: PathBuf,
        /// Target layer (optional, defaults to both layers profile export)
        #[arg(short, long)]
        layer: Option<String>,
    },
    /// Import keymap layer or full profile from a file (.json or .toml)
    Import {
        /// Input file path (.json or .toml)
        file: PathBuf,
        /// Target layer override (optional)
        #[arg(short, long)]
        layer: Option<String>,
        /// Commit persistent write to hardware flash memory
        #[arg(long)]
        commit: bool,
    },
}

#[derive(Args, Debug)]
struct MacroArgs {
    #[command(subcommand)]
    command: MacroCommands,
}

#[derive(Subcommand, Debug)]
enum MacroCommands {
    /// List all configured hardware macro slots and their action counts
    List,
    /// Dump details of a specific macro slot (0..31)
    Dump {
        /// Macro slot index (0..31)
        slot: usize,
    },
    /// Compile and write a macro sequence from JSON file into a macro slot (0..15)
    Compile {
        /// JSON file containing the macro definition
        file: PathBuf,
        /// Target macro slot index (0..15)
        #[arg(short, long)]
        slot: usize,
        /// Commit persistent write to hardware flash memory
        #[arg(long)]
        commit: bool,
    },
    /// Wipe all 4KB hardware onboard macro memory
    Wipe {
        /// Confirm persistent erase
        #[arg(long)]
        confirm: bool,
        /// Commit persistent write to hardware flash memory
        #[arg(long)]
        commit: bool,
    },
}

#[derive(Args, Debug)]
struct BackupArgs {
    /// Output directory for legacy backup files (default: ./backup)
    #[arg(short, long)]
    output_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<BackupCommands>,
}

#[derive(Subcommand, Debug)]
enum BackupCommands {
    /// Export complete device state (metadata, lighting, base/fn keymaps, 4KB macro memory) to JSON file
    Export {
        /// Target JSON backup file path
        file: PathBuf,
    },
    /// Restore complete device state from a JSON backup file
    Restore {
        /// Path to JSON backup file
        file: PathBuf,

        /// Commit restored configuration to physical hardware flash memory
        #[arg(long)]
        commit: bool,
    },
}

#[derive(Args, Debug)]
struct SettingArgs {
    #[command(subcommand)]
    command: SettingCommands,
}

#[derive(Subcommand, Debug)]
enum SettingCommands {
    /// Configure USB inactivity sleep timeout (in minutes)
    Sleep {
        /// Inactivity timeout in minutes (0 = never sleep)
        minutes: u16,
    },
    /// Reset keyboard to factory default settings
    Reset {
        /// Explicit confirmation token (must be 'I_CONFIRM_FACTORY_RESET')
        #[arg(long)]
        token: Option<String>,

        /// Commit reset to hardware non-volatile flash memory
        #[arg(long)]
        commit: bool,
    },
}

#[derive(Args, Debug)]
struct FirmwareArgs {
    #[command(subcommand)]
    command: FirmwareSubcommands,
}

#[derive(Subcommand, Debug)]
enum FirmwareSubcommands {
    /// Query and check current keyboard firmware version vs latest catalog version
    Check(FirmwareCheckArgs),

    /// Flash firmware update with safety verification and progress tracking
    Update(FirmwareUpdateArgs),
}

#[derive(Args, Debug)]
struct FirmwareCheckArgs {
    /// Output in JSON format
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct FirmwareUpdateArgs {
    /// Optional path to custom firmware binary (.bin file)
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Dry run: simulate all 480 chunk transfers and timing without writing to hardware
    #[arg(long)]
    dry_run: bool,

    /// Automatically accept all risks and liability without interactive confirmation prompt
    #[arg(short = 'y', long)]
    yes: bool,
}

#[derive(Args, Debug)]
struct GuardVolumeArgs {
    /// Run as background daemon process
    #[arg(short, long)]
    daemon: bool,

    /// Explicit event device path (e.g. /dev/input/by-id/...-event-kbd or /dev/input/event5)
    #[arg(long)]
    device: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct SetupVolumeGuardArgs {
    /// Install systemd user service to ~/.config/systemd/user/fantastech.service
    #[arg(short, long)]
    install: bool,

    /// Print systemd service file to stdout
    #[arg(long)]
    stdout: bool,
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

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn attach_console_if_present() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(dwProcessId: u32) -> i32;
        fn AllocConsole() -> i32;
    }

    let raw_args: Vec<String> = std::env::args().collect();
    let has_debug_flag = raw_args.iter().any(|a| a == "--debug" || a == "--console");
    let is_cli = raw_args.len() > 1 && !raw_args.iter().all(|a| a == "--gui" || a == "--debug" || a == "--console");

    if is_cli || has_debug_flag {
        let attached = unsafe { AttachConsole(0xFFFF_FFFF) };
        if attached == 0 && has_debug_flag {
            // Allocate a dedicated console window when explicitly requested via --debug / --console launch option
            unsafe {
                AllocConsole();
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn attach_console_if_present() {}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    fantastech_core::install_crash_handler("fantastech-cli");
    attach_console_if_present();
    let _ = tracing_subscriber::fmt::try_init();
    let cli = Cli::parse();

    if cli.gui || cli.command.is_none() {
        handle_gui_launch(false)?;
        return Ok(());
    }

    let cmd = match cli.command.as_ref() {
        Some(c) => c,
        None => return Ok(()),
    };

    // Commands that don't need USB device handle or have special routing:
    match cmd {
        Commands::Install(args) => {
            handle_install(args)?;
            return Ok(());
        }
        Commands::Uninstall(args) => {
            handle_uninstall(args)?;
            return Ok(());
        }
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
        Commands::GuardVolume(args) => {
            handle_guard_volume(args)?;
            return Ok(());
        }
        Commands::SetupVolumeGuard(args) => {
            handle_setup_volume_guard(args)?;
            return Ok(());
        }
        Commands::Light(LightArgs { command: LightCommands::ListModes }) => {
            print_modes_table();
            return Ok(());
        }
        Commands::Gui(args) => {
            handle_gui_launch(args.update)?;
            return Ok(());
        }
        Commands::Tray => {
            handle_tray_launch()?;
            return Ok(());
        }
        Commands::Service(args) => {
            handle_service_dispatch(args, cli.emulator, cli.device.clone())?;
            return Ok(());
        }
        Commands::Config(args) => {
            handle_config_command(args)?;
            return Ok(());
        }
        Commands::Profile(ProfileArgs { command: Some(ProfileCommands::List { domain, profile_type }), .. }) => {
            let eff = profile_type.as_deref().or(domain.as_deref());
            handle_profile_list(eff)?;
            return Ok(());
        }
        Commands::Profile(ProfileArgs { command: Some(ProfileCommands::Delete { arg1, arg2, profile_type }), .. }) => {
            let (dom, id) = resolve_profile_domain_and_id(arg1, arg2.as_deref(), profile_type.as_deref());
            handle_profile_delete(&dom, &id)?;
            return Ok(());
        }
        Commands::Profile(ProfileArgs { command: Some(ProfileCommands::Save { domain, id, name, description, file, sleep }), .. }) => {
            handle_profile_save(domain, id, name.as_deref(), description.as_deref(), file.as_deref(), *sleep)?;
            return Ok(());
        }
        _ => {}
    }

    // Try routing via IPC socket if background service is running and not in emulator mode
    if try_route_via_ipc(&cli)? {
        return Ok(());
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
                    eprintln!("   or run `sudo fantastech setup-udev --install` to grant permission to /dev/hidraw.");
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

/// Applies a full `ThemeConfig` to the keyboard using differential matrix caching and dual-pad synchronization.
fn apply_theme_to_keyboard<T: Transport>(
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
fn handle_custom_lighting<T: Transport>(
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

/// Interactive live key painter session with immediate physical LED updates.
fn run_interactive_key_painter<T: Transport>(
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

/// Runs the OpenRGB SDK Server daemon on a TCP port (default: 6742).
fn run_openrgb_server<T: Transport>(
    mut dev: Device<T, fantastech_device::ReversibleAccess>,
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
        let _ = stream.set_nodelay(true);

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
                        u32::from_le_bytes(payload[0..4].try_into().unwrap_or([3, 0, 0, 0]))
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

                            // OpenRGB profile loading sends UpdateLEDs after SetMode.
                            // Only stream matrix to hardware if actively in Direct / Custom Matrix mode (16).
                            if active_mode != 16 {
                                continue;
                            }

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

                            // OpenRGB profile loading sends UpdateLEDs after SetMode.
                            // Only stream matrix to hardware if actively in Direct / Custom Matrix mode (16).
                            if active_mode != 16 {
                                continue;
                            }

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

                                if active_mode != 16 {
                                    continue;
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
                            let hw_mode = resolve_openrgb_mode(mode_update.mode_idx, mode_update.value);
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
                                    speed: mode_update.speed.min(4) as u8,
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
# Generated by FantasTech.
# Allows non-root users to configure RGB backlighting, keymaps, and OpenRGB integration.

SUBSYSTEM=="hidraw", ATTRS{idVendor}=="36ae", ATTRS{idProduct}=="fe9c", MODE="0666", TAG+="uaccess", GROUP="plugdev"
SUBSYSTEM=="usb", ATTRS{idVendor}=="36ae", ATTRS{idProduct}=="fe9c", MODE="0666", TAG+="uaccess", GROUP="plugdev"
KERNEL=="hidraw*", ATTRS{idVendor}=="36ae", ATTRS{idProduct}=="fe9c", MODE="0666", TAG+="uaccess", GROUP="plugdev"
SUBSYSTEMS=="usb|hidraw", ATTRS{idVendor}=="36ae", ATTRS{idProduct}=="fe9c", MODE="0666", TAG+="uaccess"
"#;

    if args.stdout {
        println!("{}", rules_content);
        return Ok(());
    }

    if args.install {
        let dest = std::path::Path::new("/etc/udev/rules.d/99-fantastech.rules");
        println!("📝 Installing udev rules to {:?}...", dest);
        match std::fs::write(dest, rules_content) {
            Ok(_) => {
                println!("✅ Rules file written successfully.");
                println!("🔄 Reloading udev rules with `udevadm`...");
                let _ = std::process::Command::new("udevadm")
                    .args(["control", "--reload-rules"])
                    .status();
                let _ = std::process::Command::new("udevadm")
                    .args(["trigger", "--action=add", "--subsystem-match=hidraw"])
                    .status();
                let _ = std::process::Command::new("udevadm")
                    .args(["trigger"])
                    .status();
                println!("🎉 Successfully installed! You can now run `fantastech` without sudo.");
            }
            Err(e) => {
                eprintln!("⚠️  Failed writing to {:?}: {}", dest, e);
                eprintln!("💡 Tip: Please run with sudo: `sudo fantastech setup-udev --install`");
                return Err(e.into());
            }
        }
    } else {
        println!("📋 FantasTech udev configuration helper for Fantech MK894");
        println!("=======================================================");
        println!("To install udev rules automatically (requires sudo):");
        println!("  sudo fantastech setup-udev --install");
        println!();
        println!("To preview rules content:");
        println!("  fantastech setup-udev --stdout");
    }

    Ok(())
}

/// Generates or installs systemd user service for running FantasTech daemon.
fn handle_setup_systemd(args: &SetupSystemdArgs) -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/usr/local/bin/fantastech".to_string());

    let service_content = format!(
        r#"[Unit]
Description=FantasTech Background Service - Fantech MK894 Keyboard Daemon
After=network.target

[Service]
Type=simple
ExecStart={} service run
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
            let service_path = user_systemd_dir.join("fantastech.service");
            std::fs::write(&service_path, service_content)?;
            println!("✅ Written user service unit to {:?}", service_path);

            println!("🔄 Reloading systemd user daemon...");
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();
            println!("🚀 Enabling and starting fantastech.service...");
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "enable", "--now", "fantastech.service"])
                .status();

            println!("🎉 FantasTech background service is now running as a user service!");
            println!("   Check status:  systemctl --user status fantastech.service");
            println!("   View logs:     journalctl --user -u fantastech.service -f");
        } else {
            return Err("HOME environment variable not found.".into());
        }
    } else {
        println!("📋 FantasTech systemd user service helper");
        println!("============================================");
        println!("To install and start background FantasTech daemon:");
        println!("  fantastech setup-systemd --install");
        println!();
        println!("To preview service unit content:");
        println!("  fantastech setup-systemd --stdout");
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
    dev: &mut Device<T, fantastech_device::ReversibleAccess>,
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
                "Name": "FantasTech Contributors"
            }
        ],
        "Category": "Hardware Control",
        "Description": "FantasTech RGB Lighting & Profile Controller for Fantech MK894",
        "Icon": "input-keyboard-symbolic",
        "Id": "org.kde.fantastech.lighting",
        "Name": "FantasTech Lighting",
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
            text: "FantasTech MK894 Controller"
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
        // Invokes fantastech command via system process
        print("FantasTech MK894 Action: " + args.join(" "))
    }
}
"#;

    let desktop_entry = r#"[Desktop Entry]
Name=FantasTech Control Center
Comment=RGB Lighting, Macro, Keymap, and Profile Suite for Fantech MK894
Exec=fantastech-gui
Icon=input-keyboard-symbolic
Terminal=false
Type=Application
Categories=Settings;HardwareSettings;Utility;
Keywords=rgb;keyboard;fantastech;openrgb;mk894;
"#;

    // Export to project directory
    let export_dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("kde-plasma-widget"));
    let plasmoid_dir = export_dir.join("org.kde.fantastech.lighting");
    let qml_dir = plasmoid_dir.join("contents").join("ui");
    std::fs::create_dir_all(&qml_dir)?;
    std::fs::write(plasmoid_dir.join("metadata.json"), metadata_json)?;
    std::fs::write(qml_dir.join("main.qml"), main_qml)?;
    std::fs::write(export_dir.join("fantastech.desktop"), desktop_entry)?;
    println!("💾 Exported KDE Plasma widget files to {:?}", export_dir);

    if args.install {
        if let Some(home) = std::env::var_os("HOME") {
            let home_path = PathBuf::from(home);
            let plasmoid_install = home_path
                .join(".local")
                .join("share")
                .join("plasma")
                .join("plasmoids")
                .join("org.kde.fantastech.lighting");
            let plasmoid_qml = plasmoid_install.join("contents").join("ui");
            std::fs::create_dir_all(&plasmoid_qml)?;
            std::fs::write(plasmoid_install.join("metadata.json"), metadata_json)?;
            std::fs::write(plasmoid_qml.join("main.qml"), main_qml)?;
            println!("✅ Installed Plasma widget to {:?}", plasmoid_install);

            let apps_dir = home_path.join(".local").join("share").join("applications");
            std::fs::create_dir_all(&apps_dir)?;
            std::fs::write(apps_dir.join("fantastech.desktop"), desktop_entry)?;
            println!("✅ Installed desktop launcher to {:?}", apps_dir.join("fantastech.desktop"));

            println!();
            println!("🎉 KDE Plasma integration installed successfully!");
            println!("   1. To add the widget: Right-click your desktop or panel -> 'Add Widgets...' -> search 'FantasTech MK894'.");
            println!("   2. To sync your KDE accent color automatically: run `fantastech kde-sync --watch`");
        }
    } else {
        println!();
        println!("💡 To install directly into KDE Plasma:");
        println!("   fantastech setup-kde --install");
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
    println!("  fantastech light set-mode static --hex #FF0000 --brightness 4");
    println!("  fantastech light set-mode wave --speed 3");
    println!("  fantastech light set-mode breathing --hex #00FF55 --speed 2");
    println!("  fantastech light set-mode custom");
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
    fantastech_core::get_mirror_key_index(key_index)
}

/// Parses key identifier or functional zone name, returning all matching physical matrix indices.
fn parse_keys_or_zone(input: &str) -> Result<Vec<u8>, String> {
    let lower = input.to_lowercase();
    if let Some(keys) = fantastech_core::keys_for_zone(&lower) {
        return Ok(keys.iter().map(|k| k.index).collect());
    }
    let single = parse_key_identifier(input).map_err(|e| e.to_string())?;
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

fn parse_layer(s: &str) -> Result<KeyLayer, String> {
    match s.trim().to_lowercase().as_str() {
        "0" | "base" | "main" | "default" => Ok(KeyLayer::Base),
        "1" | "fn" | "function" => Ok(KeyLayer::Fn),
        _ => Err(format!("Invalid layer '{}'. Expected 'base' ('0') or 'fn' ('1')", s)),
    }
}

fn resolve_profile_domain_and_id(arg1: &str, arg2: Option<&str>, profile_type: Option<&str>) -> (String, String) {
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

fn handle_profile_command<T: Transport>(
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

fn handle_install(args: &InstallArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Installing FantasTech to system...");
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let home_path = PathBuf::from(&home);

    // 1. Install CLI binary to ~/.local/bin
    let local_bin = home_path.join(".local/bin");
    fs::create_dir_all(&local_bin)?;
    let current_exe = std::env::current_exe()?;
    let target_bin = local_bin.join("fantastech");

    if current_exe != target_bin {
        fs::copy(&current_exe, &target_bin)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&target_bin)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&target_bin, perms)?;
        }
        println!("✅ Installed CLI binary to {}", target_bin.display());
    }

    let target_ctl = local_bin.join("fantastech-ctl");
    let _ = fs::remove_file(&target_ctl);
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink(&target_bin, &target_ctl);

    // 2. Install desktop application launcher & icon
    let apps_dir = home_path.join(".local/share/applications");
    fs::create_dir_all(&apps_dir)?;
    let icons_dir = home_path.join(".local/share/icons/hicolor/scalable/apps");
    fs::create_dir_all(&icons_dir)?;

    let mut repo_dir = current_exe.clone();
    for _ in 0..3 {
        if let Some(parent) = repo_dir.parent() {
            repo_dir = parent.to_path_buf();
        }
    }

    let exec_cmd = target_bin.display().to_string();

    let desktop_content = format!(
        "[Desktop Entry]\nName=FantasTech\nGenericName=Keyboard Control Center\nComment=FantasTech Mechanical RGB Keyboard Driver & Control Suite\nExec={}\nIcon=fantastech\nTerminal=false\nType=Application\nCategories=Settings;HardwareSettings;Utility;\nStartupNotify=true\n",
        exec_cmd
    );

    let desktop_dest = apps_dir.join("fantastech.desktop");
    fs::write(&desktop_dest, desktop_content)?;
    println!("✅ Installed desktop launcher to {}", desktop_dest.display());

    let src_icon = repo_dir.join("apps/icons/fantastech.svg");
    if src_icon.exists() {
        let _ = fs::copy(&src_icon, icons_dir.join("fantastech.svg"));
        println!("✅ Installed icon to {}", icons_dir.join("fantastech.svg").display());
    }

    // 3. Install udev rules
    if !args.no_udev {
        println!("📝 Setting up non-root udev rules...");
        let udev_args = SetupUdevArgs { install: true, stdout: false };
        let _ = handle_setup_udev(&udev_args);
    }

    // 4. Install systemd user service
    if !args.no_service {
        println!("⚙️  Setting up background systemd user service...");
        let _ = service::handle_service_command("install");
        let _ = service::handle_service_command("start");
    }

    println!("\n🎉 FantasTech installation complete! Launch from your application menu or run `fantastech` in terminal.");
    Ok(())
}

fn handle_uninstall(args: &UninstallArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("🛑 Uninstalling FantasTech from system...");
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let home_path = PathBuf::from(&home);

    // 1. Stop and uninstall service
    let _ = service::handle_service_command("stop");
    let _ = service::handle_service_command("uninstall");

    // 2. If Flatpak is installed, uninstall it
    #[cfg(unix)]
    {
        let _ = Command::new("flatpak")
            .args(["uninstall", "--user", "-y", "org.fantastech.FantasTech"])
            .status();
        let _ = Command::new("flatpak")
            .args(["uninstall", "--system", "-y", "org.fantastech.FantasTech"])
            .status();
        let _ = Command::new("flatpak")
            .args(["uninstall", "--user", "-y", "io.github.fantastech.FantasTech"])
            .status();
        let _ = Command::new("flatpak")
            .args(["uninstall", "--system", "-y", "io.github.fantastech.FantasTech"])
            .status();
    }

    // 3. Remove all desktop launchers
    let desktop_files = [
        home_path.join(".local/share/applications/fantastech.desktop"),
        home_path.join(".local/share/applications/org.fantastech.FantasTech.desktop"),
        home_path.join(".local/share/applications/io.github.fantastech.FantasTech.desktop"),
    ];
    for df in &desktop_files {
        if df.exists() {
            let _ = fs::remove_file(df);
            println!("🗑️  Removed {}", df.display());
        }
    }

    // 4. Remove icon
    let icon_file = home_path.join(".local/share/icons/hicolor/scalable/apps/fantastech.svg");
    if icon_file.exists() {
        let _ = fs::remove_file(&icon_file);
        println!("🗑️  Removed {}", icon_file.display());
    }

    // 5. Remove binaries
    for bin_name in ["fantastech", "fantastech-ctl", "fantastech-gui"] {
        let bin = home_path.join(format!(".local/bin/{}", bin_name));
        if bin.exists() {
            let _ = fs::remove_file(&bin);
            println!("🗑️  Removed {}", bin.display());
        }
    }

    // 6. Purge sandboxed Flatpak data and local configs if requested
    if args.purge {
        #[cfg(unix)]
        for pkg_name in ["org.fantastech.FantasTech", "io.github.fantastech.FantasTech"] {
            let var_dir = home_path.join(format!(".var/app/{}", pkg_name));
            if var_dir.exists() {
                let _ = fs::remove_dir_all(&var_dir);
                println!("🗑️  Removed Flatpak application storage {}", var_dir.display());
            }
        }
        let cfg_dir = fantastech_core::get_fantastech_config_dir();
        if cfg_dir.exists() {
            let _ = fs::remove_dir_all(&cfg_dir);
            println!("🗑️  Purged configuration directory {}", cfg_dir.display());
        }
        #[cfg(windows)]
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            let local_dir = PathBuf::from(localappdata).join("FantasTech");
            if local_dir.exists() {
                let _ = fs::remove_dir_all(&local_dir);
            }
        }
    }

    // 7. Remove runtime sockets
    let _ = fs::remove_file(fantastech_core::single_instance_socket_path());
    let _ = fs::remove_file(fantastech_core::tray_socket_path());
    let _ = fs::remove_file(fantastech_core::ipc_socket_path());

    // 8. Remove Windows startup link and registry run key if present
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let startup_dir = PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup");
            let _ = fs::remove_file(startup_dir.join("FantasTech Service.lnk"));
            let _ = fs::remove_file(startup_dir.join("FantasTech-Service.cmd"));
            let _ = fs::remove_file(startup_dir.join("FantasTech Service.cmd"));
        }
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", "Remove-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Run' -Name 'FantasTechService' -ErrorAction SilentlyContinue"])
            .status();
        let _ = Command::new("reg")
            .args(["delete", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "FantasTechService", "/f"])
            .status();
    }

    println!("\n🧹 FantasTech has been uninstalled successfully from the system.");

    // 9. Kill all remaining active FantasTech processes
    #[cfg(unix)]
    let _ = Command::new("pkill")
        .args(["-9", "-f", "fantastech"])
        .status();

    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", "fantastech.exe"])
        .status();

    Ok(())
}

fn handle_gui_launch(update: bool) -> Result<(), Box<dyn std::error::Error>> {
    let action = if update { "UPDATE" } else { "FOCUS" };
    if fantastech_core::focus_existing_gui_with_action(action) {
        println!("ℹ️ FantasTech is already running. Brought existing window to focus.");
        return Ok(());
    }

    println!("🖥️  Launching FantasTech Graphical Interface...");
    let cfg = FantasTechConfig::load();
    if cfg.tray_icon_enabled && !fantastech_core::is_service_running() && !fantastech_core::is_tray_already_running() {
        tray::spawn_native_tray();
    }
    match fantastech_gui::run_gui() {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("\n============================================================");
            eprintln!("⚠️  Graphics Display Initialization Notice");
            eprintln!("============================================================");
            eprintln!("The graphical interface encountered an error: {}", e);
            eprintln!("If running inside a Virtual Machine, ensure 3D acceleration");
            eprintln!("is enabled in VM settings, or use FantasTech via the CLI:");
            eprintln!();
            eprintln!("  • fantastech status               View device & service status");
            eprintln!("  • fantastech profile list         List saved profiles");
            eprintln!("  • fantastech profile apply <id>   Activate a lighting/keymap profile");
            eprintln!("  • fantastech lighting list        List all 17 lighting modes");
            eprintln!("  • fantastech lighting set <mode>  Apply hardware lighting directly");
            eprintln!("  • fantastech service run          Run background daemon & OpenRGB server");
            eprintln!("============================================================\n");

            #[cfg(target_os = "windows")]
            show_windows_graphics_error_dialog(&e.to_string());

            Err(format!("GUI exited with error: {}", e).into())
        }
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn show_windows_graphics_error_dialog(err_msg: &str) {
    use std::os::windows::ffi::OsStrExt;
    let title: Vec<u16> = std::ffi::OsStr::new("FantasTech — Graphics Notice")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let msg = format!(
        "FantasTech Graphical Interface could not initialize:\n\n{}\n\n\
        Note: If running inside a Virtual Machine or Remote Desktop, ensure \
        '3D Acceleration' is enabled in your VM Display Settings.\n\n\
        FantasTech is fully functional from the Command Prompt (CLI):\n\
        • fantastech status\n\
        • fantastech lighting list\n\
        • fantastech service run",
        err_msg
    );
    let msg_wide: Vec<u16> = std::ffi::OsStr::new(&msg)
        .encode_wide()
        .chain(Some(0))
        .collect();

    extern "system" {
        fn MessageBoxW(
            hwnd: *mut std::ffi::c_void,
            lpText: *const u16,
            lpCaption: *const u16,
            uType: u32,
        ) -> i32;
    }
    const MB_OK: u32 = 0x00000000;
    const MB_ICONWARNING: u32 = 0x00000030;
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            msg_wide.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONWARNING,
        );
    }
}

fn handle_tray_launch() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔔 Running FantasTech Native System Tray...");
    tray::run_tray_loop()
}

fn handle_service_dispatch(
    args: &ServiceArgs,
    emulator: bool,
    device: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let action = if let Some(sub) = &args.command {
        match sub {
            ServiceSubcommands::Run => "run",
            ServiceSubcommands::Start => "start",
            ServiceSubcommands::Stop => "stop",
            ServiceSubcommands::Restart => "restart",
            ServiceSubcommands::Status => "status",
            ServiceSubcommands::Install => "install",
            ServiceSubcommands::Uninstall => "uninstall",
        }
    } else if let Some(ref a) = args.action {
        a.as_str()
    } else {
        "status"
    };

    if action == "run" {
        service::run_service(emulator, device)?;
        Ok(())
    } else {
        service::handle_service_command(action)?;
        Ok(())
    }
}

fn handle_config_command(args: &ConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = FantasTechConfig::load();
    match &args.command {
        ConfigCommands::Show => {
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        ConfigCommands::Path => {
            println!("{}", fantastech_core::config::get_fantastech_config_dir().join("config.json").display());
        }
        ConfigCommands::Reset => {
            config = FantasTechConfig::default();
            config.save().map_err(|e| format!("Failed to reset config: {}", e))?;
            if is_service_running() {
                let _ = send_ipc_request(&IpcRequest::SaveConfig(config.clone()));
            }
            println!("✅ Reset configuration to default values.");
        }
        ConfigCommands::Get { key } => {
            if let Some(ref k) = key {
                match k.to_lowercase().replace('_', "-").as_str() {
                    "active-lighting-profile" => println!("{}", config.active_lighting_profile),
                    "active-button-profile" => println!("{}", config.active_button_profile),
                    "active-macro-profile" => println!("{}", config.active_macro_profile),
                    "active-settings-profile" => println!("{}", config.active_settings_profile),
                    "auto-apply-lighting" => println!("{}", config.auto_apply_lighting),
                    "openrgb-enabled" => println!("{}", config.openrgb_enabled),
                    "openrgb-host" => println!("{}", config.openrgb_host),
                    "openrgb-port" => println!("{}", config.openrgb_port),
                    "guard-volume-enabled" | "volume-guard" => println!("{}", config.guard_volume_enabled),
                    "tray-icon-enabled" | "tray" => println!("{}", config.tray_icon_enabled),
                    "auto-start-enabled" | "auto-start" => println!("{}", config.auto_start_enabled),
                    "gui-theme" | "theme" => println!("{:?}", config.gui_theme),
                    _ => eprintln!("Unknown config key: {}", k),
                }
            } else {
                println!("{}", serde_json::to_string_pretty(&config)?);
            }
        }
        ConfigCommands::Set { key, value } => {
            let normalized = key.to_lowercase().replace('_', "-");
            match normalized.as_str() {
                "active-lighting-profile" => config.active_lighting_profile = value.clone(),
                "active-button-profile" => config.active_button_profile = value.clone(),
                "active-macro-profile" => config.active_macro_profile = value.clone(),
                "active-settings-profile" => config.active_settings_profile = value.clone(),
                "auto-apply-lighting" => config.auto_apply_lighting = value.parse().map_err(|_| "Expected boolean (true/false)")?,
                "openrgb-enabled" => config.openrgb_enabled = value.parse().map_err(|_| "Expected boolean (true/false)")?,
                "openrgb-host" => config.openrgb_host = value.clone(),
                "openrgb-port" => config.openrgb_port = value.parse().map_err(|_| "Expected integer port number")?,
                "guard-volume-enabled" | "volume-guard" => config.guard_volume_enabled = value.parse().map_err(|_| "Expected boolean (true/false)")?,
                "tray-icon-enabled" | "tray" => config.tray_icon_enabled = value.parse().map_err(|_| "Expected boolean (true/false)")?,
                "auto-start-enabled" | "auto-start" => config.auto_start_enabled = value.parse().map_err(|_| "Expected boolean (true/false)")?,
                "gui-theme" | "theme" => {
                    if value.eq_ignore_ascii_case("dark") {
                        config.gui_theme = GuiTheme::Dark;
                    } else {
                        config.gui_theme = GuiTheme::Light;
                    }
                }
                _ => return Err(format!("Unknown configuration key '{}'", key).into()),
            }
            config.save().map_err(|e| format!("Failed to save config: {}", e))?;
            println!("💾 Configuration key '{}' set to '{}'", key, value);

            if is_service_running() {
                let _ = send_ipc_request(&IpcRequest::SaveConfig(config));
            }
        }
    }
    Ok(())
}

fn handle_profile_save(
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

fn handle_profile_list(domain: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
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

fn handle_profile_delete(domain: &str, id: &str) -> Result<(), Box<dyn std::error::Error>> {
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

fn try_route_via_ipc(cli: &Cli) -> Result<bool, Box<dyn std::error::Error>> {
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
                (ColorMode::FixedColor, RgbColor::new(c[0], c[1], c[2]))
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

fn handle_remap_command<T: Transport>(mut dev: Device<T, ReadOnly>, cli: &Cli, args: &RemapArgs) -> Result<(), Box<dyn std::error::Error>> {
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

fn handle_macro_command<T: Transport>(mut dev: Device<T, ReadOnly>, cli: &Cli, args: &MacroArgs) -> Result<(), Box<dyn std::error::Error>> {
    match &args.command {
        MacroCommands::List => {
            let raw_mem = dev.read_macro_memory()?;
            let table = MacroMemoryTable::from_raw(raw_mem);
            let all = table.decompile_all();
            if all.is_empty() {
                println!("ℹ️  No hardware macros configured in 4KB onboard memory.");
            } else {
                println!("📋 Configured Onboard Hardware Macros ({} active):", all.len());
                for (slot, actions) in &all {
                    let ptr = table.get_slot_pointer(*slot).unwrap_or(0);
                    println!("   Slot M{:02}: {:3} actions, byte pointer 0x{:04X}", slot, actions.len(), ptr);
                }
            }
        }
        MacroCommands::Dump { slot } => {
            if *slot >= 32 {
                return Err(format!("Macro slot {} exceeds maximum 31", slot).into());
            }
            let raw_mem = dev.read_macro_memory()?;
            let table = MacroMemoryTable::from_raw(raw_mem);
            let ptr = table.get_slot_pointer(*slot).map_err(|e| format!("{:?}", e))?;
            if ptr == 0xFFFF || (ptr as usize) < 64 {
                println!("ℹ️  Macro slot M{} is empty (pointer 0x{:04X}).", slot, ptr);
                return Ok(());
            }
            let actions = table.decompile_slot(*slot).map_err(|e| format!("{:?}", e))?;
            println!("=== Macro Slot M{} (pointer 0x{:04X}, {} actions) ===", slot, ptr, actions.len());
            println!("{:<5} {:<20} {:<25} {:<10}", "Step", "Type", "Detail", "Delay");
            println!("{:-<5} {:-<20} {:-<25} {:-<10}", "", "", "", "");
            for (i, act) in actions.iter().enumerate() {
                let type_str = format!("{:?} ({})", act.event_type, if act.is_press { "Press" } else { "Release" });
                let detail_str = match act.event_type {
                    fantastech_protocol::macro_mem::MacroEventType::KeyboardKey => {
                        let key_name = fantastech_core::key_name_for_scancode(act.code).unwrap_or("Unknown");
                        format!("Scancode 0x{:02X} ({})", act.code, key_name)
                    }
                    fantastech_protocol::macro_mem::MacroEventType::MouseButton => {
                        format!("Button Mask 0x{:02X}", act.code)
                    }
                    fantastech_protocol::macro_mem::MacroEventType::MouseWheelVertical => {
                        format!("Delta {}", act.code as i8)
                    }
                    fantastech_protocol::macro_mem::MacroEventType::MouseWheelHorizontal => {
                        format!("Delta {}", act.code as i8)
                    }
                };
                println!("{:<5} {:<20} {:<25} {} ms", i + 1, type_str, detail_str, act.delay_ms);
            }
        }
        MacroCommands::Compile { file, slot, commit } => {
            if *slot > 15 {
                return Err(format!("Macro trigger slot {} exceeds max bindable trigger slot 15", slot).into());
            }
            let json_str = std::fs::read_to_string(file)?;
            let mut seq = MacroSequence::from_json(&json_str).map_err(|e| format!("Failed to parse macro JSON: {}", e))?;
            seq.slot = *slot;
            if !*commit && !cli.dry_run {
                println!("⚠️  PERSISTENT FLASH WRITE SAFETY GATE:");
                println!("   Flashing macro sequence '{}' ({} actions) to slot M{} in onboard flash.", seq.name, seq.steps.len(), slot);
                println!("   To preview without writing, run with `--dry-run`.");
                println!("   To commit the persistent write, run with `--commit`.");
                return Ok(());
            }
            let raw_mem = dev.read_macro_memory()?;
            let mut table = MacroMemoryTable::from_raw(raw_mem);
            let mut next_free = 64u16;
            for s in 0..32 {
                if s != *slot {
                    if let Ok(p) = table.get_slot_pointer(s) {
                        if p != 0xFFFF && p >= 64 {
                            if let Ok(recs) = table.decompile_slot(s) {
                                let end_p = p + (recs.len() as u16) * 4;
                                if end_p > next_free {
                                    next_free = end_p;
                                }
                            }
                        }
                    }
                }
            }
            seq.compile_into_table(&mut table, next_free).map_err(|e| format!("Macro compile error: {:?}", e))?;
            let mut persistent_dev = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
            persistent_dev.set_dry_run(cli.dry_run || !*commit);
            if let Some(records) = persistent_dev.write_macro_memory(table.as_bytes())? {
                println!("🔍 [DRY-RUN] Proposed Macro Flash Packets ({} packets)", records.len());
            } else {
                println!("💾 Macro '{}' successfully flashed to slot M{} ({} actions).", seq.name, slot, seq.steps.len());
            }
        }
        MacroCommands::Wipe { confirm, commit } => {
            if !confirm {
                println!("⚠️  DESTRUCTIVE OPERATION SAFETY GATE:");
                println!("   This will erase all 4,096 bytes of onboard hardware macro memory.");
                println!("   Pass `--confirm` to authorize this erase operation.");
                return Ok(());
            }
            if !commit && !cli.dry_run {
                println!("⚠️  PERSISTENT FLASH WRITE SAFETY GATE:");
                println!("   Erasing macro memory directly modifies onboard MCU flash.");
                println!("   To preview without writing, run with `--dry-run`.");
                println!("   To commit the erase, run with `--confirm --commit`.");
                return Ok(());
            }
            let rev_dev = dev.into_reversible();
            let mut persistent_dev = rev_dev.into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
            persistent_dev.set_dry_run(cli.dry_run || !*commit);
            if let Some(record) = persistent_dev.wipe_macros()? {
                println!("🔍 [DRY-RUN] Proposed Packet: {:02X?}", &record.buffer[..6]);
                println!("   Action: {}", record.explanation);
            } else {
                println!("🧹 4KB hardware macro memory successfully erased.");
            }
        }
    }
    Ok(())
}

fn handle_backup_command<T: Transport>(
    mut dev: Device<T, ReadOnly>,
    cli: &Cli,
    args: &BackupArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match &args.command {
        Some(BackupCommands::Export { file }) => {
            println!("💾 Exporting complete hardware state snapshot to {:?}...", file);
            let backup = dev.export_full_backup()?;
            let json = serde_json::to_string_pretty(&backup)?;
            if let Some(parent) = file.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            std::fs::write(file, json)?;
            println!("✅ Full backup successfully exported to {:?} ({} bytes)", file, std::fs::metadata(file)?.len());
        }
        Some(BackupCommands::Restore { file, commit }) => {
            println!("📂 Loading backup snapshot from {:?}...", file);
            let json = std::fs::read_to_string(file)?;
            let backup: FullDeviceBackup = serde_json::from_str(&json)?;
            println!("   Device: {} ({})", backup.device_info.profile_name, backup.device_info.firmware_version);
            println!("   Lighting Mode: {:?}", backup.lighting_config.mode);
            println!("   Base keys: {}, Fn keys: {}, Macro bytes: {}", backup.base_keymap.len(), backup.fn_keymap.len(), backup.macro_memory.len());

            if !commit && !cli.dry_run {
                eprintln!("⚠️  Dry-run preview: pass `--commit` to write this backup to physical hardware flash.");
                let mut persistent = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
                persistent.set_dry_run(true);
                if let Some(records) = persistent.restore_full_backup(&backup)? {
                    println!("📋 Dry-run planned operations ({} records):", records.len());
                    for (i, r) in records.iter().enumerate().take(5) {
                        println!("   [{}] {}", i + 1, r.explanation);
                    }
                    if records.len() > 5 {
                        println!("   ... and {} more operations", records.len() - 5);
                    }
                }
                return Ok(());
            }

            let mut persistent = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
            persistent.set_dry_run(cli.dry_run);
            persistent.restore_full_backup(&backup)?;
            if cli.dry_run {
                println!("[DRY-RUN] Full backup restore simulated successfully");
            } else {
                println!("✅ Full backup restore successfully applied to keyboard hardware!");
            }
        }
        None => {
            let out_dir = args.output_dir.clone().unwrap_or_else(|| PathBuf::from("./backup"));
            println!("💾 Performing read-before-write state backup to {:?}", out_dir);
            std::fs::create_dir_all(&out_dir)?;
            let info = dev.query_info()?;
            let info_json = format!(
                "{{\n  \"profile\": \"{}\",\n  \"firmware\": \"{}\",\n  \"active_profile\": {}\n}}\n",
                info.profile_name, info.firmware_version, info.active_profile
            );
            std::fs::write(out_dir.join("device_info.json"), info_json)?;
            println!("✅ Backup complete: saved device metadata to {:?}", out_dir);
        }
    }
    Ok(())
}

fn handle_setting_command<T: Transport>(
    dev: Device<T, ReadOnly>,
    cli: &Cli,
    args: &SettingArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match &args.command {
        SettingCommands::Sleep { minutes } => {
            let seconds = minutes.saturating_mul(60);
            if cli.dry_run {
                println!("[DRY-RUN] Setting auto-sleep timeout to {} minutes ({} seconds) via Opcode 0xFC", minutes, seconds);
                return Ok(());
            }
            let mut persistent = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
            persistent.set_dry_run(false);
            persistent.set_sleep_timeout(seconds)?;
            if *minutes == 0 {
                println!("✅ Inactivity sleep disabled (keyboard will stay awake continuously)");
            } else {
                println!("✅ Successfully set auto-sleep timeout to {} minutes ({} seconds)", minutes, seconds);
            }
        }
        SettingCommands::Reset { token, commit } => {
            let tok = token.as_deref().unwrap_or("");
            if tok != "I_CONFIRM_FACTORY_RESET" {
                eprintln!("❌ Factory reset aborted: invalid or missing confirmation token.");
                eprintln!("💡 To perform factory reset, you must pass: --token I_CONFIRM_FACTORY_RESET --commit");
                return Err(fantastech_device::DeviceError::SafetyViolation(
                    "Missing confirmation token: 'I_CONFIRM_FACTORY_RESET' required"
                ).into());
            }
            if !commit && !cli.dry_run {
                eprintln!("⚠️  Safety guard: you must pass `--commit` to write factory defaults to hardware flash.");
                eprintln!("   Alternatively, pass `--dry-run` to simulate without writing.");
                return Ok(());
            }
            if cli.dry_run {
                println!("[DRY-RUN] Full hardware factory reset (Opcode 0x0F 0xFF) simulated");
                return Ok(());
            }
            let mut persistent = dev.into_reversible().into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")?;
            persistent.set_dry_run(false);
            persistent.execute_factory_reset("I_CONFIRM_FACTORY_RESET")?;
            println!("🔥 Keyboard successfully restored to full factory default settings!");
        }
    }
    Ok(())
}

fn handle_firmware_command<T: Transport>(
    mut dev: Device<T, ReadOnly>,
    cli: &Cli,
    args: &FirmwareArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match &args.command {
        FirmwareSubcommands::Check(check_args) => {
            let info = dev.query_info()?;
            let catalog = fantastech_core::get_catalog_entry();
            let is_json = cli.json || check_args.json;

            let cur_fw = info.firmware_version.trim().to_string();
            let cat_fw = catalog.version_str.clone();

            let update_available = fantastech_core::check_firmware_update_available(&cur_fw);

            if is_json {
                let json_obj = serde_json::json!({
                    "device": info.profile_name,
                    "target_vid": format!("0x{:04X}", catalog.target_vid),
                    "target_pid": format!("0x{:04X}", catalog.target_pid),
                    "current_firmware": cur_fw,
                    "latest_catalog_firmware": cat_fw,
                    "catalog_version_code": catalog.version,
                    "catalog_sha256": catalog.sha256,
                    "update_available": update_available,
                    "status": if update_available { "update_available" } else { "up_to_date" },
                });
                println!("{}", serde_json::to_string_pretty(&json_obj)?);
            } else {
                println!("⚡ FantasTech Firmware Diagnostics");
                println!("==========================================");
                println!("  Target Hardware:    {} (USB 0x{:04X}:0x{:04X})", info.profile_name, catalog.target_vid, catalog.target_pid);
                println!("  Current Firmware:   {}", cur_fw);
                println!("  Catalog Firmware:   {} (Latest Release)", cat_fw);
                println!("  Catalog SHA-256:    {}", catalog.sha256);
                if update_available {
                    println!("  Update Status:      ⚡ UPDATE AVAILABLE ({} -> {})", cur_fw, cat_fw);
                    println!("  Action:             Run `fantastech firmware update` to flash.");
                } else {
                    println!("  Update Status:      ✅ Up to date (No update required)");
                }
                println!("==========================================");
            }
        }
        FirmwareSubcommands::Update(update_args) => {
            let binary = if let Some(ref path) = update_args.file {
                println!("📂 Loading custom firmware image from: {:?}", path);
                std::fs::read(path)?
            } else {
                println!("📦 Using official bundled factory firmware binary: AK110M_UK_V101.bin");
                fantastech_core::get_embedded_firmware_bytes().to_vec()
            };

            // Pre-validation
            println!("🔍 Validating firmware binary image...");
            let val_info = match fantastech_core::validate_firmware_binary(&binary) {
                Ok(info) => info,
                Err(e) => {
                    eprintln!("❌ Firmware validation failed: {}", e);
                    return Err(fantastech_device::DeviceError::SafetyViolation(
                        "Invalid firmware binary",
                    ).into());
                }
            };

            println!("   • Image Size:    {} bytes (32 KB)", val_info.file_size);
            println!("   • SHA-256 Hash:  {}", val_info.sha256);
            if let Some(ref v) = val_info.detected_version_str {
                println!("   • MCU Revision:  {}", v);
            }

            if !update_args.yes {
                println!();
                println!("========================================================================");
                println!("⚠️  CRITICAL FIRMWARE UPDATE WARNING & LEGAL DISCLAIMER");
                println!("========================================================================");
                println!("Flashing keyboard microcontroller firmware involves low-level non-volatile");
                println!("flash memory writes. Interruption will leave the hardware in bootloader mode.");
                println!();
                println!("CRITICAL PRECAUTIONS:");
                println!("  1. DO NOT disconnect the USB cable during flashing.");
                println!("  2. DO NOT power off, sleep, or reboot your computer.");
                println!("  3. Ensure a stable power source (do not flash on low laptop battery).");
                println!();
                println!("LEGAL DISCLAIMER:");
                println!("  BY PROCEEDING, YOU EXPRESSLY ACKNOWLEDGE AND AGREE THAT YOU ARE");
                println!("  FLASHING FIRMWARE AT YOUR OWN RISK. FANTASTECH, ITS CREATORS, AND");
                println!("  ITS CONTRIBUTORS ARE ENTIRELY RELEASED FROM ALL LIABILITY,");
                println!("  RESPONSIBILITY, AND WARRANTY CLAIMS IN THE EVENT OF DEVICE FAILURE,");
                println!("  CORRUPTION, OR BRICKING.");
                println!("========================================================================");
                println!();
                print!("To proceed, type 'YES' and press Enter: ");
                std::io::stdout().flush()?;

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if input.trim() != "YES" {
                    println!("❌ Firmware update aborted by user.");
                    return Ok(());
                }
            }

            let (token, _) = fantastech_device::FirmwareUpdateSafetyToken::authorize(
                fantastech_device::FirmwareUpdateSafetyToken::MANDATORY_CONFIRMATION,
                &binary,
            )?;

            let is_dry_run = update_args.dry_run || cli.dry_run;
            if is_dry_run {
                println!("\n[DRY-RUN] Simulating 7-step firmware flashing state machine...");
            } else {
                println!("\n⚡ Initiating firmware flashing sequence...");
            }

            let mut last_reported_pct: i32 = -1;

            let records = fantastech_device::execute_firmware_update(
                dev.transport_mut(),
                &token,
                &binary,
                is_dry_run,
                |p| {
                    let current_pct = p.percentage as i32;
                    if current_pct != last_reported_pct || p.phase == fantastech_device::FirmwarePhase::Completed {
                        last_reported_pct = current_pct;
                        print!("\r[{}/7] [{:<20}] {:>3.0}% | {}",
                            p.step,
                            "=".repeat((p.percentage / 5.0) as usize),
                            p.percentage,
                            p.message
                        );
                        let _ = std::io::stdout().flush();
                    }
                },
            )?;

            println!("\n");
            if is_dry_run {
                println!("✅ Dry-run simulation completed successfully! (Generated {} packet records)", records.len());
            } else {
                println!("🎉 Firmware update completed successfully!");
                println!("Keyboard has rebooted and re-enumerated into normal operational mode.");
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn handle_guard_volume(args: &GuardVolumeArgs) -> Result<(), Box<dyn std::error::Error>> {
    volume_guard::handle_guard_volume(args)
}

#[cfg(not(target_os = "linux"))]
fn handle_guard_volume(_args: &GuardVolumeArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("ℹ️ Volume Guard rotary debouncer is only required on Linux due to raw evdev mechanical chatter.");
    println!("   On Windows, the native OS HID media driver smooths rotary knob steps directly without third-party hooks.");
    Ok(())
}

#[cfg(target_os = "linux")]
fn handle_setup_volume_guard(args: &SetupVolumeGuardArgs) -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/usr/local/bin/fantastech".to_string());

    let service_content = format!(
        r#"[Unit]
Description=FantasTech MK894 Hardware Volume Knob Guard Daemon
After=network.target

[Service]
Type=simple
ExecStart={} guard-volume --daemon
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
            let service_path = user_systemd_dir.join("fantastech-volume-guard.service");
            std::fs::write(&service_path, service_content)?;
            println!("✅ Written user service unit to {:?}", service_path);

            println!("🔄 Reloading systemd user daemon...");
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();
            println!("🚀 Enabling and starting fantastech-volume-guard.service...");
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "enable", "--now", "fantastech-volume-guard.service"])
                .status();
            println!("🛡️  Volume Knob Guard service is now active and protected against KDE Wayland volume runaway!");
        } else {
            eprintln!("❌ Could not determine HOME directory. Run with --stdout to inspect service unit.");
        }
    } else {
        println!("Pass --install to install and activate ~/.config/systemd/user/fantastech-volume-guard.service");
        println!("Pass --stdout to view service unit file contents");
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn handle_setup_volume_guard(_args: &SetupVolumeGuardArgs) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("⚠️ Volume guard service setup is supported on Linux only.");
    Err("Setup volume guard is supported on Linux only".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[cfg(target_os = "linux")]
    use crate::volume_guard::{DebounceAction, RawInputEvent, VolumeDebouncer};
    #[cfg(target_os = "linux")]
    const EV_SYN: u16 = 0x00;
    #[cfg(target_os = "linux")]
    const EV_KEY: u16 = 0x01;
    #[cfg(target_os = "linux")]
    const SYN_REPORT: u16 = 0;
    #[cfg(target_os = "linux")]
    const KEY_VOLUMEUP: u16 = 115;

    #[cfg(target_os = "linux")]
    #[test]
    fn test_raw_input_event_roundtrip() {
        let ev = RawInputEvent::new(EV_KEY, KEY_VOLUMEUP, 1);
        let bytes = ev.to_bytes();
        let parsed = RawInputEvent::from_bytes(&bytes);
        assert_eq!(ev, parsed);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_volume_debouncer_logic() {
        let mut debouncer = VolumeDebouncer::default();
        let t0 = Instant::now();

        // 1. Initial Volume Up press -> Emit pulse
        let ev_press = RawInputEvent::new(EV_KEY, KEY_VOLUMEUP, 1);
        let action = debouncer.process_event(&ev_press, t0);
        assert_eq!(
            action,
            DebounceAction::EmitPulseWithAutoRelease {
                code: KEY_VOLUMEUP,
                release_delay: Duration::from_millis(5),
            }
        );

        // 2. Chattering second press 10ms later (<25ms) -> Dropped
        let t1 = t0 + Duration::from_millis(10);
        let action2 = debouncer.process_event(&ev_press, t1);
        assert_eq!(action2, DebounceAction::Drop);

        // 3. Hardware release event (val = 0) -> Dropped
        let ev_release = RawInputEvent::new(EV_KEY, KEY_VOLUMEUP, 0);
        let action_rel = debouncer.process_event(&ev_release, t1);
        assert_eq!(action_rel, DebounceAction::Drop);

        // 4. Hardware repeat event (val = 2) -> Dropped
        let ev_rep = RawInputEvent::new(EV_KEY, KEY_VOLUMEUP, 2);
        let action_rep = debouncer.process_event(&ev_rep, t1);
        assert_eq!(action_rep, DebounceAction::Drop);

        // 5. Subsequent valid press 35ms after t0 (>=25ms) -> Emitted
        let t2 = t0 + Duration::from_millis(35);
        let action3 = debouncer.process_event(&ev_press, t2);
        assert_eq!(
            action3,
            DebounceAction::EmitPulseWithAutoRelease {
                code: KEY_VOLUMEUP,
                release_delay: Duration::from_millis(5),
            }
        );

        // 6. Normal media key (KEY_MUTE = 113) -> Forward as-is
        let ev_mute = RawInputEvent::new(EV_KEY, 113, 1);
        let action_mute = debouncer.process_event(&ev_mute, t2);
        assert_eq!(action_mute, DebounceAction::ForwardAsIs);

        // 7. EV_SYN event -> Forward as-is
        let ev_syn = RawInputEvent::new(EV_SYN, SYN_REPORT, 0);
        let action_syn = debouncer.process_event(&ev_syn, t2);
        assert_eq!(action_syn, DebounceAction::ForwardAsIs);
    }

    #[test]
    fn test_single_executable_cli_and_gui_dispatch() {
        // When run with no arguments, subcommand is None (defaults to GUI launch)
        let cli_empty = Cli::try_parse_from(["fantastech"]).unwrap();
        assert!(cli_empty.command.is_none());
        assert!(!cli_empty.gui);

        // When run with --gui flag
        let cli_gui_flag = Cli::try_parse_from(["fantastech", "--gui"]).unwrap();
        assert!(cli_gui_flag.gui);

        // When run with gui subcommand
        let cli_gui_cmd = Cli::try_parse_from(["fantastech", "gui"]).unwrap();
        assert!(matches!(cli_gui_cmd.command, Some(Commands::Gui(_))));

        // When run with info subcommand
        let cli_info = Cli::try_parse_from(["fantastech", "info"]).unwrap();
        assert!(matches!(cli_info.command, Some(Commands::Info)));
    }

    #[cfg(unix)]
    #[test]
    fn test_single_instance_gui_and_tray_dispatch() {
        let (tx, rx) = std::sync::mpsc::channel();
        let guard = fantastech_core::SingleInstanceGuard::bind(tx, || {}).expect("Failed to bind guard");

        // When GUI is active, focus_existing_gui() must return true
        assert!(fantastech_core::focus_existing_gui());
        assert_eq!(rx.recv_timeout(std::time::Duration::from_millis(500)).unwrap(), "FOCUS");

        // Single instance can also dispatch EXIT to request GUI closure
        assert!(fantastech_core::focus_existing_gui_with_action("EXIT"));
        assert_eq!(rx.recv_timeout(std::time::Duration::from_millis(500)).unwrap(), "EXIT");

        // When tray is active, is_tray_already_running() must return true
        let tray_guard = fantastech_core::TrayInstanceGuard::bind().expect("Failed to bind tray guard");
        assert!(fantastech_core::is_tray_already_running());

        // Calling run_tray_loop while tray is already active returns Ok(()) immediately
        assert!(tray::run_tray_loop().is_ok());

        drop(tray_guard);
        drop(guard);
    }
}

