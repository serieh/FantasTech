//! Keyboard MCU firmware query, catalog checks, and flashing commands.


use crate::args::*;
use fantastech_device::{Device, ReadOnly};
use fantastech_transport::Transport;
use std::io::Write;

pub(crate) fn handle_firmware_command<T: Transport>(
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
