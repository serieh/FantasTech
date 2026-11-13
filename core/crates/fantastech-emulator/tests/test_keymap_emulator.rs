//! End-to-end integration tests for key remapping, profile switching,
//! and 4KB macro memory transactions against the Virtual MK894 Emulator.

#![forbid(unsafe_code)]

use fantastech_core::{default_layer_keymap, MacroItem, MacroSequence};
use fantastech_device::Device;
use fantastech_emulator::VirtualKeyboardEmulator;
use fantastech_protocol::constants::SPARSE_KEY_COUNT;
use fantastech_protocol::keymap::{ActionCategory, KeyAction, KeyLayer};
use fantastech_protocol::macro_mem::MacroMemoryTable;

#[test]
fn test_emulator_device_info_and_profile_switching() {
    let emu = VirtualKeyboardEmulator::new();
    let mut dev = Device::new_read_only(emu);

    let info = dev.query_info().expect("query_info");
    assert_eq!(info.pid, 0xFE9C);
    assert_eq!(info.active_profile, 0);
    assert_eq!(info.profile_count, 3);
    assert_eq!(info.profile_name, "AK107");

    let mut rev_dev = dev.into_reversible();
    rev_dev.switch_profile(2).expect("switch_profile");
    assert_eq!(rev_dev.transport().state().active_profile, 2);
}

#[test]
fn test_emulator_read_default_keymaps() {
    let emu = VirtualKeyboardEmulator::new();
    let mut dev = Device::new_read_only(emu);

    // Read Base layer (all 9 chunks, 504 bytes)
    let base_layer = dev.read_keymap_layer(KeyLayer::Base).expect("read base layer");
    assert_eq!(base_layer.len(), SPARSE_KEY_COUNT);
    // Esc (slot 0) should be Esc (scancode 41)
    assert_eq!(base_layer[0].category, ActionCategory::StandardHidKey);
    assert_eq!(base_layer[0].code2, 41);
    // Space (slot 111) should be Space (scancode 44)
    assert_eq!(base_layer[111].category, ActionCategory::StandardHidKey);
    assert_eq!(base_layer[111].code2, 44);

    // Read Fn layer (all 9 chunks, 504 bytes)
    let fn_layer = dev.read_keymap_layer(KeyLayer::Fn).expect("read fn layer");
    assert_eq!(fn_layer.len(), SPARSE_KEY_COUNT);
    // WinLock at slot 106 should have Category 31 (HardwareLighting/Custom) and code1 = 7
    assert_eq!(fn_layer[106].category, ActionCategory::HardwareLighting);
    assert_eq!(fn_layer[106].code1, 0x07);
    // F1 at slot 1 on Fn layer should be Media Player (usage 0x0183)
    assert_eq!(fn_layer[1].category, ActionCategory::MultimediaOs);
    assert_eq!(fn_layer[1].code1, 0x83);
    assert_eq!(fn_layer[1].code2, 0x01);
    // F7 at slot 7 on Fn layer should be Play/Pause (usage 0x00CD)
    assert_eq!(fn_layer[7].category, ActionCategory::MultimediaOs);
    assert_eq!(fn_layer[7].code1, 0xCD);
    assert_eq!(fn_layer[7].code2, 0x00);
}

#[test]
fn test_emulator_single_key_remap_roundtrip() {
    let emu = VirtualKeyboardEmulator::new();
    let dev = Device::new_read_only(emu);

    let mut persistent_dev = dev
        .into_reversible()
        .into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")
        .expect("into_persistent");

    // Remap Space (slot 111) to Ctrl+C (modifier 0x01, scancode 0x06)
    let action = KeyAction::combo(0x01, 0x06);
    persistent_dev
        .remap_key_action(111, KeyLayer::Base, &action)
        .expect("remap_key_action");

    // Verify emulator state
    let state_action = persistent_dev.transport().state().keymap_profiles[0][0][111];
    assert_eq!(state_action.category, ActionCategory::StandardHidKey);
    assert_eq!(state_action.code1, 0x01); // Ctrl
    assert_eq!(state_action.code2, 0x06); // C

    // Read back through device handle (Opcode 0x08)
    let mut read_dev = persistent_dev.into_read_only();
    let chunk_idx = 111 / 14; // chunk 7
    let chunk = read_dev.read_keymap_chunk(chunk_idx, KeyLayer::Base).expect("read chunk");
    let read_action = chunk[(111 % 14) as usize];
    assert_eq!(read_action, action);
}

#[test]
fn test_emulator_batch_layer_remap_504_bytes() {
    let emu = VirtualKeyboardEmulator::new();
    let dev = Device::new_read_only(emu);

    let mut persistent_dev = dev
        .into_reversible()
        .into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")
        .expect("into_persistent");

    let mut custom_layer = default_layer_keymap(KeyLayer::Base);
    // Remap slot 0 to WinLock
    custom_layer[0] = KeyAction::win_lock();
    // Remap slot 1 to Macro Trigger M5 (mode=1, repeat=3)
    custom_layer[1] = KeyAction::macro_trigger(5, 1, 3);
    // Remap slot 63 (CapsLock) to Ctrl
    custom_layer[63] = KeyAction::standard_key(0xE0); // Left Ctrl scancode

    // Batch flash all 9 packets (504 bytes, Opcode 0x09)
    persistent_dev
        .write_keymap_layer(KeyLayer::Base, &custom_layer)
        .expect("write_keymap_layer");

    // Read back all 9 chunks via Opcode 0x08
    let mut read_dev = persistent_dev.into_read_only();
    let readback_layer = read_dev.read_keymap_layer(KeyLayer::Base).expect("read_keymap_layer");

    assert_eq!(readback_layer[0], custom_layer[0]);
    assert_eq!(readback_layer[1], custom_layer[1]);
    assert_eq!(readback_layer[63], custom_layer[63]);
    assert_eq!(readback_layer, custom_layer);
}

#[test]
fn test_emulator_macro_memory_compile_flash_readback_decompile() {
    let emu = VirtualKeyboardEmulator::new();
    let dev = Device::new_read_only(emu);

    let mut persistent_dev = dev
        .into_reversible()
        .into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")
        .expect("into_persistent");

    // Create a macro sequence: press 'A', wait 50ms, release 'A'
    let seq = MacroSequence {
        name: "Rapid Fire A".to_string(),
        slot: 3,
        loop_mode: 0,
        repeat: 1,
        steps: vec![
            MacroItem {
                action: "down".to_string(),
                scancode: 0x04, // 'A'
                code: 0x04,
                name: Some("A".to_string()),
                delay_ms: 50,
            },
            MacroItem {
                action: "up".to_string(),
                scancode: 0x04,
                code: 0x04,
                name: Some("A".to_string()),
                delay_ms: 25,
            },
        ],
    };

    let mut mem_table = MacroMemoryTable::new_empty();
    let next_offset = seq.compile_into_table(&mut mem_table, 64).expect("compile macro");
    assert_eq!(next_offset, 72); // 2 records * 4 bytes = 8 bytes; 64 + 8 = 72

    // Flash 4KB table to emulator
    persistent_dev
        .write_macro_memory(mem_table.as_bytes())
        .expect("write_macro_memory");

    // Read back full 4KB macro memory in 74 chunks (Opcode 0x0C)
    let mut read_dev = persistent_dev.into_read_only();
    let readback_raw = read_dev.read_macro_memory().expect("read_macro_memory");
    let readback_table = MacroMemoryTable::from_raw(readback_raw);

    // Verify slot 3 pointer is 64
    let ptr = readback_table.get_slot_pointer(3).expect("get_slot_pointer");
    assert_eq!(ptr, 64);

    // Decompile slot 3
    let decompiled = readback_table.decompile_slot(3).expect("decompile_slot");
    assert_eq!(decompiled.len(), 2);
    assert_eq!(decompiled[0].code, 0x04);
    assert!(decompiled[0].is_press);
    assert_eq!(decompiled[0].delay_ms, 50);

    assert_eq!(decompiled[1].code, 0x04);
    assert!(!decompiled[1].is_press);
    assert!(decompiled[1].is_end_of_macro);
    assert_eq!(decompiled[1].delay_ms, 25);
}

#[test]
fn test_emulator_macro_wipe() {
    let emu = VirtualKeyboardEmulator::new();
    let dev = Device::new_read_only(emu);

    let mut persistent_dev = dev
        .into_reversible()
        .into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")
        .expect("into_persistent");

    // Write some bytes into slot 0
    let mut mem = MacroMemoryTable::new_empty();
    mem.set_slot_pointer(0, 64).unwrap();
    persistent_dev.write_macro_memory(mem.as_bytes()).unwrap();

    // Wipe macros (Opcode 0x0F, subcommand 4)
    persistent_dev.wipe_macro_memory().expect("wipe_macro_memory");

    // Read back and verify all pointers are 0xFFFF
    let mut read_dev = persistent_dev.into_read_only();
    let readback = read_dev.read_macro_memory().expect("read_macro_memory");
    let table = MacroMemoryTable::from_raw(readback);
    for slot in 0..32 {
        assert_eq!(table.get_slot_pointer(slot).unwrap(), 0xFFFF);
    }
}

#[test]
fn test_emulator_dry_run_safety() {
    let emu = VirtualKeyboardEmulator::new();
    let dev = Device::new_read_only(emu);

    let mut persistent_dev = dev
        .into_reversible()
        .into_persistent("I_CONFIRM_PERSISTENT_HARDWARE_WRITE")
        .expect("into_persistent");

    persistent_dev.set_dry_run(true);

    // Try remapping key 0
    let action = KeyAction::win_lock();
    let dry_record = persistent_dev
        .remap_key_action(0, KeyLayer::Base, &action)
        .expect("dry run remap");

    assert!(dry_record.is_some());
    let rec = dry_record.unwrap();
    assert_eq!(rec.safety_tier, "WRITE-PERSISTENT");

    // Verify emulator state was NOT changed
    assert_ne!(persistent_dev.transport().state().keymap_profiles[0][0][0], action);
}
