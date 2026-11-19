//! Device configuration backup and restore commands.


use crate::args::*;
use fantastech_device::{Device, FullDeviceBackup, ReadOnly};
use fantastech_transport::Transport;
use std::path::PathBuf;

pub(crate) fn handle_backup_command<T: Transport>(
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
