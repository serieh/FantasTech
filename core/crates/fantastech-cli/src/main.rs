#![deny(unsafe_code)]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[allow(unsafe_code)]
mod glibc_compat {
    use std::arch::global_asm;
    global_asm!(".symver atan2f, atan2f@GLIBC_2.2.5");
    global_asm!(".symver acosf, acosf@GLIBC_2.2.5");

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

pub mod args;
pub mod commands;
pub mod ipc_client;
pub mod output;
pub mod parse;
mod service;
mod tray;
#[cfg(target_os = "linux")]
pub(crate) mod volume_guard;

#[cfg(test)]
mod tests;

use clap::Parser;
use crate::args::*;
use crate::commands::*;
use crate::ipc_client::try_route_via_ipc;
use crate::parse::*;
use fantastech_emulator::VirtualKeyboardEmulator;
use fantastech_transport::HidrawTransport;

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
