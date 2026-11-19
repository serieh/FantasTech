//! Macro slot recording, dumping, and EEPROM programming commands.


use crate::args::*;
use fantastech_core::*;
use fantastech_device::{Device, ReadOnly};
use fantastech_protocol::macro_mem::*;
use fantastech_transport::Transport;

pub(crate) fn handle_macro_command<T: Transport>(mut dev: Device<T, ReadOnly>, cli: &Cli, args: &MacroArgs) -> Result<(), Box<dyn std::error::Error>> {
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
