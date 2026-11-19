// Tests for fantastech-core submodules.
use super::*;
use fantastech_protocol::constants::*;
use fantastech_protocol::keymap::*;
use fantastech_protocol::lighting::*;
use fantastech_protocol::macro_mem::*;


#[test]
fn test_theme_to_matrix_conversion() {
    let theme = ThemeConfig::cyberpunk();
    let matrix = theme.to_matrix();
    assert_eq!(matrix.len(), SPARSE_KEY_COUNT);
    assert_eq!(matrix[0], RgbColor::new(0, 255, 0)); // Esc neon green
    assert_eq!(matrix[44], RgbColor::new(255, 255, 0)); // W electric yellow
    assert_eq!(matrix[64], RgbColor::new(255, 255, 0)); // A electric yellow
    assert_eq!(matrix[65], RgbColor::new(255, 255, 0)); // S electric yellow
    assert_eq!(matrix[66], RgbColor::new(255, 255, 0)); // D electric yellow
    assert_eq!(matrix[111], RgbColor::new(0, 255, 255)); // Spacebar cyan
}

#[test]
fn test_macro_compilation_into_4kb_table() {
    let mut table = MacroMemoryTable::new_empty();
    let seq = MacroSequence {
        name: "Quick A".to_string(),
        slot: 0,
        loop_mode: 0,
        repeat: 1,
        steps: vec![MacroItem {
            action: "click".to_string(),
            scancode: 0x04,
            code: 0x04,
            name: Some("A".to_string()),
            delay_ms: 50,
        }],
    };

    let next_offset = seq.compile_into_table(&mut table, 64).expect("compile macro");
    assert_eq!(next_offset, 64 + 8); // 2 records (down, up) = 8 bytes
    assert_eq!(table.get_slot_pointer(0).unwrap(), 64);

    // Decompile back and verify roundtrip
    let decompiled = table.decompile_slot(0).expect("decompile macro");
    assert_eq!(decompiled.len(), 2);
    assert_eq!(decompiled[0].code, 0x04);
    assert!(decompiled[0].is_press);
    assert_eq!(decompiled[1].code, 0x04);
    assert!(!decompiled[1].is_press);
    assert!(decompiled[1].is_end_of_macro);
}

#[test]
fn test_theme_json_roundtrip() {
    let theme = ThemeConfig::cyberpunk();
    let json = theme.to_json().expect("serialize theme");
    let parsed = ThemeConfig::from_json(&json).expect("deserialize theme");
    assert_eq!(parsed.name, theme.name);
    assert_eq!(parsed.default_color, theme.default_color);
    assert_eq!(parsed.key_overrides.len(), theme.key_overrides.len());
}

#[test]
fn test_action_string_parser_and_formatter() {
    // Standard key
    let a = parse_action_str("w").expect("parse w");
    assert_eq!(a, KeyAction::standard(26));
    assert_eq!(format_action_str(&a), "W");

    // Combos
    let ctrl_c = parse_action_str("ctrl+c").expect("parse ctrl+c");
    assert_eq!(ctrl_c, KeyAction::combo(0x01, 6));
    assert_eq!(format_action_str(&ctrl_c), "Ctrl+C");

    let win_shift_s = parse_action_str("win+shift+s").expect("parse win+shift+s");
    assert_eq!(win_shift_s, KeyAction::combo(0x08 | 0x02, 22));
    assert_eq!(format_action_str(&win_shift_s), "Win+Shift+S");

    // Multimedia
    let vol_up = parse_action_str("vol+").expect("parse vol+");
    assert_eq!(vol_up, KeyAction::multimedia(0x00E9));
    assert_eq!(format_action_str(&vol_up), "Volume +");

    // Custom WinLock
    let wl = parse_action_str("custom:win-lock").expect("parse win-lock");
    assert_eq!(wl, KeyAction::win_lock());
    assert_eq!(format_action_str(&wl), "WinLock");

    // Mouse
    let m_left = parse_action_str("mouse:left").expect("parse mouse left");
    assert_eq!(m_left, KeyAction::mouse_button(1));
    assert_eq!(format_action_str(&m_left), "Mouse:Left");

    // Macro
    let m0 = parse_action_str("macro:0:hold").expect("parse macro 0 hold");
    assert_eq!(m0, KeyAction::macro_trigger(0, 1, 1));
    assert_eq!(format_action_str(&m0), "M0:hold");

    // Parse format_action_str output directly (Roundtrip fidelity)
    assert_eq!(parse_action_str("Volume +").unwrap(), KeyAction::multimedia(0x00E9));
    assert_eq!(parse_action_str("Volume -").unwrap(), KeyAction::multimedia(0x00EA));
    assert_eq!(parse_action_str("Play/Pause").unwrap(), KeyAction::multimedia(0x00CD));
    assert_eq!(parse_action_str("Prev Track").unwrap(), KeyAction::multimedia(0x00B6));
    assert_eq!(parse_action_str("Next Track").unwrap(), KeyAction::multimedia(0x00B5));
    assert_eq!(parse_action_str("My PC").unwrap(), KeyAction::multimedia(0x0194));
    assert_eq!(parse_action_str("Led On/Off").unwrap(), KeyAction::hardware_custom(0));
    assert_eq!(parse_action_str("LedBrt+").unwrap(), KeyAction::hardware_custom(1));
    assert_eq!(parse_action_str("LedBrt-").unwrap(), KeyAction::hardware_custom(2));
    assert_eq!(parse_action_str("LedMode").unwrap(), KeyAction::hardware_custom(3));
    assert_eq!(parse_action_str("LedColor").unwrap(), KeyAction::hardware_custom(4));
    assert_eq!(parse_action_str("Speed+").unwrap(), KeyAction::hardware_custom(5));
    assert_eq!(parse_action_str("Speed-").unwrap(), KeyAction::hardware_custom(6));
    assert_eq!(parse_action_str("Reset").unwrap(), KeyAction::hardware_custom(16));
    assert_eq!(parse_action_str("WinLock").unwrap(), KeyAction::win_lock());
    assert_eq!(parse_action_str("M0:hold").unwrap(), KeyAction::macro_trigger(0, 1, 1));
    assert_eq!(parse_action_str("M1").unwrap(), KeyAction::macro_trigger(1, 0, 1));
}

#[test]
fn test_profile_keymap_config_roundtrip() {
    let mut profile = ProfileKeymapConfig::default();
    profile.base_layer.overrides.insert("W".to_string(), "ctrl+c".to_string());
    profile.base_layer.overrides.insert("CapsLock".to_string(), "custom:win-lock".to_string());
    profile.fn_layer.overrides.insert("F1".to_string(), "vol+".to_string());

    let json = profile.to_json().expect("serialize profile");
    let parsed = ProfileKeymapConfig::from_json(&json).expect("deserialize profile");
    assert_eq!(parsed.base_layer.overrides.get("W").unwrap(), "ctrl+c");
    assert_eq!(parsed.fn_layer.overrides.get("F1").unwrap(), "vol+");

    let descriptors = profile.base_layer.to_descriptors().expect("to descriptors");
    assert_eq!(descriptors[44], KeyAction::combo(0x01, 6)); // W (44) is Ctrl+C
    assert_eq!(descriptors[63], KeyAction::win_lock());     // CapsLock (63) is WinLock
    assert_eq!(descriptors[0], KeyAction::standard(41));    // Esc (0) remains default Esc
}

#[test]
fn test_verify_physical_key_definitions_and_scancodes() {
    assert_eq!(KEY_DEFINITIONS.len(), 104);

    // Row 4 (ZXCV row) matrix slots & scancodes
    let z = KEY_DEFINITIONS.iter().find(|k| k.name == "Z").unwrap();
    assert_eq!(z.index, 86);
    assert_eq!(default_scancode_for_key("Z").unwrap(), 29);

    let x = KEY_DEFINITIONS.iter().find(|k| k.name == "X").unwrap();
    assert_eq!(x.index, 87);
    assert_eq!(default_scancode_for_key("X").unwrap(), 27);

    let c = KEY_DEFINITIONS.iter().find(|k| k.name == "C").unwrap();
    assert_eq!(c.index, 88);
    assert_eq!(default_scancode_for_key("C").unwrap(), 6);

    let v = KEY_DEFINITIONS.iter().find(|k| k.name == "V").unwrap();
    assert_eq!(v.index, 89);
    assert_eq!(default_scancode_for_key("V").unwrap(), 25);

    let b = KEY_DEFINITIONS.iter().find(|k| k.name == "B").unwrap();
    assert_eq!(b.index, 90);
    assert_eq!(default_scancode_for_key("B").unwrap(), 5);

    let n = KEY_DEFINITIONS.iter().find(|k| k.name == "N").unwrap();
    assert_eq!(n.index, 91);
    assert_eq!(default_scancode_for_key("N").unwrap(), 17);

    let m = KEY_DEFINITIONS.iter().find(|k| k.name == "M").unwrap();
    assert_eq!(m.index, 92);
    assert_eq!(default_scancode_for_key("M").unwrap(), 16);

    let comma = KEY_DEFINITIONS.iter().find(|k| k.name == "<").unwrap();
    assert_eq!(comma.index, 93);
    assert_eq!(default_scancode_for_key("<").unwrap(), 54);

    let period = KEY_DEFINITIONS.iter().find(|k| k.name == ">").unwrap();
    assert_eq!(period.index, 94);
    assert_eq!(default_scancode_for_key(">").unwrap(), 55);

    let slash = KEY_DEFINITIONS.iter().find(|k| k.name == "/").unwrap();
    assert_eq!(slash.index, 95);
    assert_eq!(default_scancode_for_key("/").unwrap(), 56);

    let rshift = KEY_DEFINITIONS.iter().find(|k| k.name == "RightShift").unwrap();
    assert_eq!(rshift.index, 97);
    assert_eq!(default_scancode_for_key("RightShift").unwrap(), 229);

    // Function row
    let f1 = KEY_DEFINITIONS.iter().find(|k| k.name == "F1").unwrap();
    assert_eq!(f1.index, 1);
    assert_eq!(default_scancode_for_key("F1").unwrap(), 58);

    let f12 = KEY_DEFINITIONS.iter().find(|k| k.name == "F12").unwrap();
    assert_eq!(f12.index, 12);
    assert_eq!(default_scancode_for_key("F12").unwrap(), 69);

    // Bottom row
    let space = KEY_DEFINITIONS.iter().find(|k| k.name == "Space").unwrap();
    assert_eq!(space.index, 111);
    assert_eq!(get_mirror_key_index(111), Some(110));

    let num0 = KEY_DEFINITIONS.iter().find(|k| k.name == "Num 0").unwrap();
    assert_eq!(num0.index, 123);
    assert_eq!(get_mirror_key_index(123), Some(122));

    // Shift stabilizer pads
    assert_eq!(get_mirror_key_index(84), Some(85));
    assert_eq!(get_mirror_key_index(85), Some(84));
    assert_eq!(get_mirror_key_index(97), Some(96));
    assert_eq!(get_mirror_key_index(96), Some(97));

    // Default base layer keymap validation
    let base = default_layer_keymap(KeyLayer::Base);
    assert_eq!(base[86].code2, 29); // Z
    assert_eq!(base[87].code2, 27); // X
    assert_eq!(base[88].code2, 6);  // C
    assert_eq!(base[89].code2, 25); // V
    assert_eq!(base[90].code2, 5);  // B
    assert_eq!(base[91].code2, 17); // N
    assert_eq!(base[92].code2, 16); // M
    assert_eq!(base[93].code2, 54); // <
    assert_eq!(base[94].code2, 55); // >
    assert_eq!(base[95].code2, 56); // /
    assert_eq!(base[97].code2, 229);// RightShift
    assert_eq!(base[1].code2, 58);  // F1
    assert_eq!(base[12].code2, 69); // F12
    assert_eq!(base[111].code2, 44);// Space
    assert_eq!(base[110].code2, 44);// Space mirror
    assert_eq!(base[123].code2, 98);// Num 0
    assert_eq!(base[122].code2, 98);// Num 0 mirror
}
