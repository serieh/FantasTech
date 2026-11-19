//! Keyboard hardware settings (sleep timeout, report rate, profiles).


use crate::args::*;
use fantastech_device::{Device, ReadOnly};
use fantastech_transport::Transport;

pub(crate) fn handle_setting_command<T: Transport>(
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
