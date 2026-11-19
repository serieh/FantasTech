//! CLI argument models and Clap subcommands.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "fantastech",
    version,
    about = "FantasTech — Professional Driver & RGB Control Suite for Fantech MK894",
    long_about = "FantasTech is an open-source driver, multi-domain profile manager, and unified control suite for the Fantech MK894 Maxpower II Mechanical RGB Keyboard (USB VID 0x36AE, PID 0xFE9C)."
)]
pub struct Cli {
    /// Launch the native graphical interface (FantasTech GUI)
    #[arg(long, global = true)]
    pub gui: bool,

    /// Show debug CMD console window when running GUI
    #[arg(long, alias = "console", global = true)]
    pub debug: bool,

    /// Use simulated virtual keyboard emulator instead of physical USB device
    #[arg(short = 'e', long, global = true)]
    pub emulator: bool,

    /// Explicit device node path (e.g. /dev/hidraw4)
    #[arg(short, long, global = true)]
    pub device: Option<PathBuf>,

    /// Dry-run mode: display generated report bytes without transmitting to hardware
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Bypass background service Unix domain socket and force direct communication
    #[arg(long, global = true)]
    pub no_ipc: bool,

    /// Output responses in structured JSON format
    #[arg(long, global = true)]
    pub json: bool,

    /// Enable verbose logging output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
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
    #[command(name = "light", alias = "lighting")]
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
pub struct ServiceArgs {
    /// Service subcommand (run, start, stop, restart, status, install, uninstall)
    #[command(subcommand)]
    pub command: Option<ServiceSubcommands>,

    /// Direct action string if subcommand is omitted (e.g. run, start, stop, status)
    pub action: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum ServiceSubcommands {
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
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
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
pub struct ProfileArgs {
    /// Target profile index to switch to (0..2)
    pub index: Option<u8>,

    /// Query active profile from hardware
    #[arg(short, long)]
    pub current: bool,

    #[command(subcommand)]
    pub command: Option<ProfileCommands>,
}

#[derive(Subcommand, Debug)]
pub enum ProfileCommands {
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
pub struct OpenRgbServerArgs {
    /// TCP port to listen on for OpenRGB SDK clients (default: 6742)
    #[arg(short, long, default_value = "6742")]
    pub port: u16,

    /// IP address to bind to (default: 127.0.0.1)
    #[arg(short, long, default_value = "127.0.0.1")]
    pub bind: String,
}

#[derive(Args, Debug, Default)]
pub struct InstallArgs {
    /// Skip sudo/pkexec udev rule installation
    #[arg(long)]
    pub no_udev: bool,

    /// Skip systemd user service registration
    #[arg(long)]
    pub no_service: bool,
}

#[derive(Args, Debug, Default)]
pub struct UninstallArgs {
    /// Also purge user configuration directory (~/.config/fantastech)
    #[arg(long)]
    pub purge: bool,
}

#[derive(Args, Debug)]
pub struct SetupUdevArgs {
    /// Install rules directly to /etc/udev/rules.d/99-fantastech.rules (requires sudo)
    #[arg(short, long)]
    pub install: bool,

    /// Print udev rules to stdout
    #[arg(long)]
    pub stdout: bool,
}

#[derive(Args, Debug)]
pub struct SetupSystemdArgs {
    /// Install systemd user service to ~/.config/systemd/user/fantastech.service
    #[arg(short, long)]
    pub install: bool,

    /// Print systemd service unit to stdout
    #[arg(long)]
    pub stdout: bool,
}

#[derive(Args, Debug)]
pub struct KdeSyncArgs {
    /// Watch ~/.config/kdeglobals continuously and update keyboard when accent color changes
    #[arg(short, long)]
    pub watch: bool,

    /// Lighting animation mode: static, breathing, or wave (default: static)
    #[arg(short, long, default_value = "static")]
    pub mode: String,

    /// Brightness level (0..4, default: 4)
    #[arg(short, long, default_value = "4")]
    pub brightness: u8,
}

#[derive(Args, Debug)]
pub struct SetupKdeArgs {
    /// Install plasmoid to ~/.local/share/plasma/plasmoids/org.kde.fantastech.lighting and application launcher
    #[arg(short, long)]
    pub install: bool,

    /// Target directory to export plasmoid files (default: ./kde-plasma-widget)
    #[arg(short, long)]
    pub dir: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct LightArgs {
    #[command(subcommand)]
    pub command: LightCommands,
}

#[derive(Subcommand, Debug)]
pub enum LightCommands {
    /// List all 17 hardware lighting animation modes and their capabilities
    #[command(name = "list-modes", alias = "list", alias = "modes")]
    ListModes,

    /// Set global lighting animation mode, brightness, speed, and color
    #[command(name = "set-mode", alias = "set")]
    SetMode {
        /// Animation mode name (e.g. static, breathing, wave, reactive) or index (0..16)
        mode: String,

        /// Brightness level (0..4) or percentage (0..100)
        #[arg(short, long, default_value = "4")]
        brightness: u8,

        /// Animation speed level (0..4) or percentage (0..100)
        #[arg(short, long, default_value = "2")]
        speed: u8,

        /// Animation direction (deprecated: MK894 hardware does not support direction reversal)
        #[arg(long, default_value = "default", hide = true)]
        direction: String,

        /// Fixed RGB color: name (e.g. 'red'), hex ('#FF0000'), or components ('255 0 0')
        #[arg(short, long, num_args = 1..=3)]
        color: Option<Vec<String>>,

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
pub struct CustomLightArgs {
    /// Launch interactive real-time key painter wizard with live keyboard feedback
    #[arg(short, long)]
    pub interactive: bool,

    /// Save current custom lighting layout to a JSON preset file
    #[arg(long)]
    pub save: Option<PathBuf>,

    /// Load and apply custom lighting layout from a JSON preset file
    #[arg(long)]
    pub load: Option<PathBuf>,

    /// Batch assign key colors directly from CLI (e.g. "wasd=red,arrows=blue,space=#00FF00")
    #[arg(long)]
    pub set: Option<String>,

    /// Background color for unassigned keys (name, hex, or 'off'; default: 'off')
    #[arg(long, default_value = "off")]
    pub default_color: String,

    /// Turn off all keys not explicitly assigned
    #[arg(long)]
    pub blackout: bool,
}

#[derive(Args, Debug)]
pub struct RemapArgs {
    #[command(subcommand)]
    pub command: RemapCommands,
}

#[derive(Subcommand, Debug)]
pub enum RemapCommands {
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
pub struct MacroArgs {
    #[command(subcommand)]
    pub command: MacroCommands,
}

#[derive(Subcommand, Debug)]
pub enum MacroCommands {
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
pub struct BackupArgs {
    /// Output directory for legacy backup files (default: ./backup)
    #[arg(short, long)]
    pub output_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<BackupCommands>,
}

#[derive(Subcommand, Debug)]
pub enum BackupCommands {
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
pub struct SettingArgs {
    #[command(subcommand)]
    pub command: SettingCommands,
}

#[derive(Subcommand, Debug)]
pub enum SettingCommands {
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
pub struct FirmwareArgs {
    #[command(subcommand)]
    pub command: FirmwareSubcommands,
}

#[derive(Subcommand, Debug)]
pub enum FirmwareSubcommands {
    /// Query and check current keyboard firmware version vs latest catalog version
    Check(FirmwareCheckArgs),

    /// Flash firmware update with safety verification and progress tracking
    Update(FirmwareUpdateArgs),
}

#[derive(Args, Debug)]
pub struct FirmwareCheckArgs {
    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct FirmwareUpdateArgs {
    /// Optional path to custom firmware binary (.bin file)
    #[arg(short, long)]
    pub file: Option<PathBuf>,

    /// Dry run: simulate all 480 chunk transfers and timing without writing to hardware
    #[arg(long)]
    pub dry_run: bool,

    /// Automatically accept all risks and liability without interactive confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct GuardVolumeArgs {
    /// Run as background daemon process
    #[arg(short, long)]
    pub daemon: bool,

    /// Explicit event device path (e.g. /dev/input/by-id/...-event-kbd or /dev/input/event5)
    #[arg(long)]
    pub device: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct SetupVolumeGuardArgs {
    /// Install systemd user service to ~/.config/systemd/user/fantastech.service
    #[arg(short, long)]
    pub install: bool,

    /// Print systemd service file to stdout
    #[arg(long)]
    pub stdout: bool,
}

#[derive(Args, Debug)]
pub struct TestKeysArgs {
    /// Interactive step-by-step key verification wizard
    #[arg(short, long)]
    pub interactive: bool,

    /// Start index for interactive wizard (default: 0)
    #[arg(long, default_value = "0")]
    pub start: usize,

    /// Test an individual key by name or index (e.g. 'esc', 'space', '44', '66')
    #[arg(short, long)]
    pub key: Option<String>,

    /// Light up a specific physical matrix row (0..5)
    #[arg(long)]
    pub row: Option<u8>,

    /// Light up a specific functional zone (wasd, arrows, numpad, nav, fkeys, media, bottom)
    #[arg(long)]
    pub zone: Option<String>,

    /// Skip initial blackout (do not turn off other keys before test)
    #[arg(long)]
    pub no_blackout: bool,

    /// Turn all keyboard LEDs OFF
    #[arg(long)]
    pub all_off: bool,

    /// Turn all keyboard LEDs ON with solid test color
    #[arg(long)]
    pub all_on: bool,

    /// Raw index range to test (e.g. "0..20" or single index "66")
    #[arg(long)]
    pub range: Option<String>,

    /// Color to use for testing: R G B (0..255, default: 255 0 0)
    #[arg(long, num_args = 3, default_values = &["255", "0", "0"])]
    pub color: Vec<u8>,
}
