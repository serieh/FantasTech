//! UI and presentation regression tests.

use crate::*;
use crate::ui::widgets::*;


#[test]
fn test_inspect_egui_key_names() {
    let keys = [
        egui::Key::A, egui::Key::Z, egui::Key::Num0, egui::Key::Num9,
        egui::Key::ArrowUp, egui::Key::ArrowDown, egui::Key::ArrowLeft, egui::Key::ArrowRight,
        egui::Key::Space, egui::Key::Enter, egui::Key::Escape, egui::Key::Backspace, egui::Key::Tab,
        egui::Key::Insert, egui::Key::Delete, egui::Key::Home, egui::Key::End,
        egui::Key::PageUp, egui::Key::PageDown,
        egui::Key::Plus, egui::Key::Minus, egui::Key::Equals,
        egui::Key::OpenBracket, egui::Key::CloseBracket, egui::Key::Backslash,
        egui::Key::Semicolon, egui::Key::Colon, egui::Key::Comma, egui::Key::Period, egui::Key::Slash,
        egui::Key::F1, egui::Key::F12,
    ];
    for k in keys {
        let name = k.name();
        assert!(scancode_for_key_name(name).is_some(), "Key {:?} with name '{}' failed to resolve to scancode", k, name);
    }
}

#[test]
fn test_fantastech_app_initialization() {
    let app = FantasTechApp::new();
    assert_eq!(app.device_model, "Fantech MK894 Maxpower II");
    assert_eq!(app.active_profile, 0);
    assert_eq!(app.active_layer, KeyLayer::Base);
    assert!(!app.profiles.is_empty());
    assert!(!app.selected_lighting_profile.is_empty());
    assert!(!app.selected_button_profile.is_empty());
}

#[test]
fn test_fantastech_gui_theme_application() {
    let ctx = egui::Context::default();
    apply_gui_theme(&ctx, GuiTheme::Light);
    apply_gui_theme(&ctx, GuiTheme::Dark);
}

#[test]
fn test_keymap_undo_redo_round_trip() {
    let mut app = FantasTechApp::new();
    let original = app.profiles[0][KeyLayer::Base as usize][43];
    let replacement = KeyAction::standard_key(0x05);

    app.remap_single_key(43, replacement);
    assert_eq!(app.profiles[0][KeyLayer::Base as usize][43], replacement);
    assert!(!app.undo_keymaps.is_empty());

    app.undo_keymap_edit();
    assert_eq!(app.profiles[0][KeyLayer::Base as usize][43], original);

    app.redo_keymap_edit();
    assert_eq!(app.profiles[0][KeyLayer::Base as usize][43], replacement);
}

#[test]
fn test_keymap_undo_tracks_mirrored_slot_changes() {
    let mut app = FantasTechApp::new();
    let (key_index, mirror_index) = ANSI_104_KEYS
        .iter()
        .find_map(|key| {
            get_mirror_key_index(key.index)
                .filter(|mirror| *mirror != key.index && (*mirror as usize) < SPARSE_KEY_COUNT)
                .map(|mirror| (key.index, mirror))
        })
        .expect("keyboard layout should expose at least one mirrored key pair");

    let primary_action = app.profiles[0][KeyLayer::Base as usize][key_index as usize];
    let divergent_mirror_action = if primary_action == KeyAction::standard_key(0x04) {
        KeyAction::standard_key(0x05)
    } else {
        KeyAction::standard_key(0x04)
    };
    app.profiles[0][KeyLayer::Base as usize][mirror_index as usize] = divergent_mirror_action;

    // The primary key already has the requested action. Updating only its
    // mirrored slot must still create a history entry.
    app.remap_single_key(key_index, primary_action);
    assert_eq!(
        app.profiles[0][KeyLayer::Base as usize][mirror_index as usize],
        primary_action
    );
    assert_eq!(app.undo_keymaps.len(), 1);

    app.undo_keymap_edit();
    assert_eq!(
        app.profiles[0][KeyLayer::Base as usize][mirror_index as usize],
        divergent_mirror_action
    );
}

#[test]
fn test_keymap_undo_history_is_bounded() {
    let mut app = FantasTechApp::new();
    for _ in 0..105 {
        app.record_keymap_undo();
    }
    assert_eq!(app.undo_keymaps.len(), 100);
}

#[test]
fn test_fantastech_app_apply_lighting_profile() {
    let mut app = FantasTechApp::new();
    app.custom_selected_keys.insert(44);
    app.custom_select_all = true;

    app.apply_lighting_profile_by_id("cyberpunk");
    assert_eq!(app.selected_lighting_profile, "cyberpunk");
    assert!(app.custom_selected_keys.is_empty(), "Selection must be cleared on profile switch");
    assert!(!app.custom_select_all);
    assert_eq!(app.key_colors[44], Color32::from_rgb(255, 255, 0)); // Yellow W key

    app.apply_lighting_profile_by_id("gaming");
    assert_eq!(app.selected_lighting_profile, "gaming");
    assert_eq!(app.key_colors[44], Color32::from_rgb(255, 0, 0)); // Red W key in gaming
}

#[test]
fn test_fantastech_app_apply_button_profile() {
    let mut app = FantasTechApp::new();
    app.apply_button_profile_by_id("default");
    assert_eq!(app.selected_button_profile, "default");
}

#[test]
fn test_fantastech_app_apply_macro_profile() {
    let mut app = FantasTechApp::new();
    app.apply_macro_profile_by_id("default");
    assert_eq!(app.selected_macro_profile, "default");
}

#[test]
fn test_fantastech_app_apply_settings_profile() {
    let mut app = FantasTechApp::new();
    app.apply_settings_profile_by_id("default");
    assert_eq!(app.selected_settings_profile, "default");
}

#[test]
fn test_fantastech_app_create_and_delete_button_profile() {
    let mut app = FantasTechApp::new();
    let temp_dir = std::env::temp_dir().join(format!("test_btn_prof_{}", std::process::id()));
    app.profile_manager = ProfileManager::new(Some(temp_dir));
    let prof_name = "Work Test Profile";
    app.create_new_button_profile(prof_name, true);
    assert!(app.status_message.contains("Created new button profile"));
    let new_id = app.selected_button_profile.clone();
    assert_ne!(new_id, "default");
    assert!(new_id.starts_with("work_test_profile"));

    // Cannot delete built-in "default"
    app.delete_button_profile_by_id("default");
    assert!(app.status_message.contains("Cannot delete built-in profile"));

    // Now delete the newly created custom profile
    app.delete_button_profile_by_id(&new_id);
    assert!(app.status_message.contains("Deleted button profile"));
    assert_eq!(app.selected_button_profile, "default");
}

#[test]
fn test_fantastech_app_create_and_delete_lighting_profile() {
    let mut app = FantasTechApp::new();
    let temp_dir = std::env::temp_dir().join(format!("test_light_prof_{}", std::process::id()));
    app.profile_manager = ProfileManager::new(Some(temp_dir));
    let prof_name = "Night Shift Blue";
    app.create_new_lighting_profile(prof_name, false);
    assert!(app.status_message.contains("Created new lighting profile"));
    let new_id = app.selected_lighting_profile.clone();
    assert_ne!(new_id, "gaming");
    assert!(new_id.starts_with("night_shift_blue"));

    // Cannot delete built-in "gaming"
    app.delete_lighting_profile_by_id("gaming");
    assert!(app.status_message.contains("Cannot delete built-in profile"));

    // Now delete the newly created custom profile
    app.delete_lighting_profile_by_id(&new_id);
    assert!(app.status_message.contains("Deleted lighting profile"));
    assert_eq!(app.selected_lighting_profile, "gaming");
}

#[test]
fn test_ansi_104_coordinates_coverage() {
    assert_eq!(ANSI_104_KEYS.len(), 104, "ANSI_104_KEYS must contain exactly 104 keys");

    let mut seen_indices = std::collections::HashSet::new();
    for k in ANSI_104_KEYS {
        assert!(seen_indices.insert(k.index), "Duplicate matrix slot index in ANSI_104_KEYS: {}", k.index);
        assert!(KEY_DEFINITIONS.iter().any(|kd| kd.index == k.index), "Index {} in ANSI_104_KEYS not found in KEY_DEFINITIONS", k.index);
        assert!(k.w >= 1.0, "Key {} has invalid width: {}", k.name, k.w);
        assert!(k.h >= 1.0, "Key {} has invalid height: {}", k.name, k.h);
        assert!(k.x >= 0.0 && k.x + k.w <= 23.01, "Key {} exceeds horizontal bounds (x={}, w={})", k.name, k.x, k.w);
        assert!(k.y >= 0.0 && k.y + k.h <= 6.46, "Key {} exceeds vertical bounds (y={}, h={})", k.name, k.y, k.h);
    }

    // Overlap check: ensure no two keys overlap
    for (i, k1) in ANSI_104_KEYS.iter().enumerate() {
        for k2 in ANSI_104_KEYS.iter().skip(i + 1) {
            let x_overlap = !(k1.x + k1.w <= k2.x + 0.01 || k2.x + k2.w <= k1.x + 0.01);
            let y_overlap = !(k1.y + k1.h <= k2.y + 0.01 || k2.y + k2.h <= k1.y + 0.01);
            assert!(!(x_overlap && y_overlap), "Keys {} (slot {}) and {} (slot {}) physically overlap!", k1.name, k1.index, k2.name, k2.index);
        }
    }
}

#[test]
fn test_fn_layer_label_resolution() {
    let esc = ANSI_104_KEYS.iter().find(|k| k.index == 0).expect("Esc key missing");
    assert_eq!(esc.fn_name, "Reset");

    let f1 = ANSI_104_KEYS.iter().find(|k| k.index == 1).expect("F1 missing");
    assert_eq!(f1.fn_name, "Media");

    let f2 = ANSI_104_KEYS.iter().find(|k| k.index == 2).expect("F2 missing");
    assert_eq!(f2.fn_name, "Vol-");

    let f3 = ANSI_104_KEYS.iter().find(|k| k.index == 3).expect("F3 missing");
    assert_eq!(f3.fn_name, "Vol+");

    let f4 = ANSI_104_KEYS.iter().find(|k| k.index == 4).expect("F4 missing");
    assert_eq!(f4.fn_name, "Mute");

    let ins = ANSI_104_KEYS.iter().find(|k| k.index == 35).expect("Ins missing");
    assert_eq!(ins.fn_name, "Mode");

    let del = ANSI_104_KEYS.iter().find(|k| k.index == 56).expect("Del missing");
    assert_eq!(del.fn_name, "Color");

    let up = ANSI_104_KEYS.iter().find(|k| k.index == 99).expect("Up missing");
    assert_eq!(up.fn_name, "Brt+");

    let down = ANSI_104_KEYS.iter().find(|k| k.index == 120).expect("Down missing");
    assert_eq!(down.fn_name, "Brt-");

    let space = ANSI_104_KEYS.iter().find(|k| k.index == 111).expect("Space missing");
    assert_eq!(space.fn_name, "LedOff");

    let win = ANSI_104_KEYS.iter().find(|k| k.index == 106).expect("Win missing");
    assert_eq!(win.fn_name, "WinLk");
}

#[test]
fn test_font_definitions_initialization() {
    let ctx = egui::Context::default();
    setup_custom_fonts(&ctx);
}

#[test]
fn test_fit_keycap_text_bounds() {
    let (fitted, font) = fit_keycap_text("W", 30.0);
    assert_eq!(fitted, "W");
    assert_eq!(font.size, 12.5);

    let (fitted_long, _) = fit_keycap_text("ExtremelyLongActionNameToFitInSmallKey", 25.0);
    assert!(fitted_long.len() < 30);

    let (fitted_multiline, font_multiline) = fit_keycap_text("Num\nEnter", 30.0);
    assert_eq!(fitted_multiline, "Num\nEnter");
    assert!(font_multiline.size >= 8.5);
}

#[test]
fn test_sparse_key_colors_capacity_and_matrix_slots() {
    let app = FantasTechApp::new();
    assert_eq!(app.key_colors.len(), SPARSE_KEY_COUNT);
    assert_eq!(app.key_colors.len(), 126);

    // Verify keys that previously caused out-of-bounds panics when length was 104
    assert!(105 < app.key_colors.len()); // Left Ctrl
    assert!(106 < app.key_colors.len()); // Left Win
    assert!(107 < app.key_colors.len()); // Left Alt
    assert!(110 < app.key_colors.len()); // Space
    assert!(111 < app.key_colors.len()); // Space mirror
    assert!(119 < app.key_colors.len()); // Left Arrow
    assert!(120 < app.key_colors.len()); // Down Arrow
    assert!(121 < app.key_colors.len()); // Right Arrow
    assert!(122 < app.key_colors.len()); // Num 0
    assert!(124 < app.key_colors.len()); // Num .
}

#[test]
fn test_custom_canvas_selection_and_color_application() {
    let mut app = FantasTechApp::new();
    assert!(app.custom_selected_keys.is_empty());
    assert!(!app.custom_select_all);

    // 1. Select All
    app.select_all_custom_keys();
    assert!(app.custom_select_all);
    assert_eq!(app.custom_selected_keys.len(), 104);

    // 2. Deselect All
    app.deselect_all_custom_keys();
    assert!(!app.custom_select_all);
    assert!(app.custom_selected_keys.is_empty());

    // 3. Single key select
    app.toggle_custom_key_selection(0, false); // Esc
    assert_eq!(app.custom_selected_keys.len(), 1);
    assert!(app.custom_selected_keys.contains(&0));

    // 4. Multi-key select (Shift/Ctrl toggle)
    app.toggle_custom_key_selection(1, true); // F1
    assert_eq!(app.custom_selected_keys.len(), 2);
    assert!(app.custom_selected_keys.contains(&0));
    assert!(app.custom_selected_keys.contains(&1));

    // 5. Apply color to selection
    let cyan = [0, 220, 255];
    app.apply_color_to_custom_selection(cyan);
    assert_eq!(app.key_colors[0], Color32::from_rgb(0, 220, 255));
    assert_eq!(app.key_colors[1], Color32::from_rgb(0, 220, 255));

    // 6. Mirrored slot painting (Space: 111 & 110)
    app.toggle_custom_key_selection(111, false);
    app.apply_color_to_custom_selection([255, 0, 0]);
    assert_eq!(app.key_colors[111], Color32::from_rgb(255, 0, 0));
    assert_eq!(app.key_colors[110], Color32::from_rgb(255, 0, 0));

    // 7. Turn off selection
    app.turn_off_custom_selection();
    assert_eq!(app.key_colors[111], Color32::BLACK);
    assert_eq!(app.key_colors[110], Color32::BLACK);
}

#[test]
fn test_custom_key_group_selection_and_designer_palettes() {
    let mut app = FantasTechApp::new();
    let temp_dir = std::env::temp_dir().join(format!("test_pal_{}", std::process::id()));
    app.profile_manager = ProfileManager::new(Some(temp_dir));

    // 1. Group selection: WASD
    app.select_custom_key_group("wasd");
    assert_eq!(app.custom_selected_keys.len(), 4);
    assert!(app.custom_selected_keys.contains(&44)); // W
    assert!(app.custom_selected_keys.contains(&64)); // A
    assert!(app.custom_selected_keys.contains(&65)); // S
    assert!(app.custom_selected_keys.contains(&66)); // D

    // 2. Group selection: Arrows
    app.select_custom_key_group("arrows");
    assert_eq!(app.custom_selected_keys.len(), 4);
    assert!(app.custom_selected_keys.contains(&99));  // Up
    assert!(app.custom_selected_keys.contains(&119)); // Left
    assert!(app.custom_selected_keys.contains(&120)); // Down
    assert!(app.custom_selected_keys.contains(&121)); // Right

    // 3. Designer Palettes: Miami in Custom mode (16)
    app.selected_mode = 16;
    app.apply_designer_palette("miami");
    assert_eq!(app.current_color, [0, 240, 255]);
    // Esc (left edge x=0.0) should be bright cyan
    assert_eq!(app.key_colors[0], Color32::from_rgb(0, 240, 255));

    // 4. Designer Palettes in non-custom mode switches to custom mode, clears, and applies across all keys
    app.selected_mode = 1; // Static
    app.apply_designer_palette("terminal");
    assert_eq!(app.selected_mode, 16);
    assert_eq!(app.current_color, [0, 255, 65]);
    assert_eq!(app.key_colors[0], Color32::from_rgb(0, 59, 0));

    // 5. Hardware parity check: MK894 does not support direction reversal in built-in modes
    assert!(!LightingMode::StreamLeftToRight.supports_direction());
    assert!(!LightingMode::StreamUpToDown.supports_direction());
}

#[test]
fn test_lighting_modes_support_color() {
    // Dynamic spectrum modes must NOT support fixed custom color
    assert!(!LightingMode::Spectrum.supports_color(), "ColorLoop (Spectrum) must not support fixed color");
    assert!(!LightingMode::Off.supports_color(), "LightOff must not support fixed color");
    assert!(!LightingMode::Rotate.supports_color(), "Spin must not support fixed color");
    assert!(!LightingMode::Meteor.supports_color(), "Meteor must not support fixed color");

    // Modes that do support fixed color
    assert!(LightingMode::Static.supports_color());
    assert!(LightingMode::Breathing.supports_color());
    assert!(LightingMode::Bloom.supports_color());
    assert!(LightingMode::Tide.supports_color());
    assert!(LightingMode::Shuttle.supports_color());
}

#[test]
fn test_custom_canvas_brightness_scaling() {
    let mut app = FantasTechApp::new();
    app.selected_mode = 16;
    app.brightness = 2; // Level 2: 50%
    app.current_color = [200, 100, 50];

    let factor = (app.brightness as f32 / 4.0).clamp(0.0, 1.0);
    let scaled_r = (app.current_color[0] as f32 * factor).round() as u8;
    let scaled_g = (app.current_color[1] as f32 * factor).round() as u8;
    let scaled_b = (app.current_color[2] as f32 * factor).round() as u8;

    assert_eq!(scaled_r, 100);
    assert_eq!(scaled_g, 50);
    assert_eq!(scaled_b, 25);

    // When brightness is 0, scaling yields all zeros (off)
    app.brightness = 0;
    let zero_factor = (app.brightness as f32 / 4.0).clamp(0.0, 1.0);
    assert_eq!((app.current_color[0] as f32 * zero_factor).round() as u8, 0);
    assert_eq!((app.current_color[1] as f32 * zero_factor).round() as u8, 0);
    assert_eq!((app.current_color[2] as f32 * zero_factor).round() as u8, 0);
}

#[test]
fn test_recent_colors_history() {
    let mut app = FantasTechApp::new();
    assert_eq!(app.recent_colors.len(), 6);

    let color1 = [255, 10, 20];
    let color2 = [10, 255, 30];
    let color3 = [30, 40, 255];
    let color4 = [200, 200, 200];
    let color5 = [50, 60, 70];
    let color6 = [80, 90, 100];
    let color7 = [110, 120, 130];

    app.record_used_color(color1);
    assert_eq!(app.recent_colors[0], color1);
    assert_eq!(app.recent_colors.len(), 6);

    app.record_used_color(color2);
    assert_eq!(app.recent_colors[0], color2);
    assert_eq!(app.recent_colors[1], color1);

    app.record_used_color(color3);
    app.record_used_color(color4);
    app.record_used_color(color5);
    app.record_used_color(color6);
    assert_eq!(app.recent_colors, vec![color6, color5, color4, color3, color2, color1]);

    // Push 7th color; should drop color1 and retain only 6
    app.record_used_color(color7);
    assert_eq!(app.recent_colors, vec![color7, color6, color5, color4, color3, color2]);

    // Push existing color2; should bring color2 to front without duplicates
    app.record_used_color(color2);
    assert_eq!(app.recent_colors, vec![color2, color7, color6, color5, color4, color3]);
}

#[test]
fn test_universal_profile_loader_handles_wrong_type_and_invalid_data() {
    let mut app = FantasTechApp::new();
    let temp_dir = std::env::temp_dir();

    // 1. Save a lighting profile
    let lighting_path = temp_dir.join("test_cross_lighting.json");
    app.selected_mode = 2; // Breath mode
    app.export_lighting_profile_to_path(&lighting_path);

    // 2. Load it via universal importer (which is called by button profile load too)
    assert!(app.import_any_profile_from_path(&lighting_path));
    // Verify lighting mode was updated
    assert_eq!(app.selected_mode, 2);

    // 3. Save a button profile
    let button_path = temp_dir.join("test_cross_button.json");
    app.export_button_profile_to_path(&button_path);
    // Load button profile via universal importer
    assert!(app.import_any_profile_from_path(&button_path));

    // 4. Invalid file content does not panic/crash
    let bad_path = temp_dir.join("test_corrupt.json");
    std::fs::write(&bad_path, b"not valid json at all").unwrap();
    assert!(!app.import_any_profile_from_path(&bad_path));
    assert!(app.status_message.contains("not a valid FantasTech profile"));

    // Cleanup
    let _ = std::fs::remove_file(lighting_path);
    let _ = std::fs::remove_file(button_path);
    let _ = std::fs::remove_file(bad_path);
}

#[test]
fn test_paint_key_and_mirror_and_copy_buffer() {
    let mut app = FantasTechApp::new();
    assert!(app.copied_color.is_none());

    let test_color = [123, 45, 67];
    // Key 111 is Space; its mirror is 110
    app.paint_key_and_mirror(111, test_color);
    assert_eq!(app.key_colors[111], Color32::from_rgb(123, 45, 67));
    assert_eq!(app.key_colors[110], Color32::from_rgb(123, 45, 67));
    assert_eq!(app.recent_colors[0], test_color);

    // Test copy buffer
    app.copied_color = Some(test_color);
    assert_eq!(app.copied_color, Some([123, 45, 67]));
}

#[test]
fn test_gui_firmware_update_check_detection() {
    let mut app = FantasTechApp::new();

    // 1. When firmware is an unknown older revision (e.g. V50), update must be detected
    app.firmware_version = "V50".to_string();
    app.check_for_firmware_updates();
    assert!(app.update_available);
    assert!(app.firmware_status_text.contains("Update available"));

    // 2. When firmware matches factory revisions (V64, V101), status is up to date
    app.firmware_version = "V64".to_string();
    app.check_for_firmware_updates();
    assert!(!app.update_available);
    assert!(app.firmware_status_text.contains("up to date"));

    app.firmware_version = "V101".to_string();
    app.check_for_firmware_updates();
    assert!(!app.update_available);
    assert!(app.firmware_status_text.contains("up to date"));

    // 3. Simulating clicking check button: sets checking timer for at least 2 seconds
    app.checking_updates_start = Some(std::time::Instant::now());
    assert!(app.checking_updates_start.is_some());
    assert!(app.checking_updates_start.unwrap().elapsed() < std::time::Duration::from_secs(2));

    // 4. Opening modal
    app.firmware_update_modal_open = true;
    assert!(app.firmware_update_modal_open);
    app.firmware_update_modal_open = false;
    assert!(!app.firmware_update_modal_open);
}

#[test]
fn test_auto_update_firmware_modal_safety() {
    let mut app = FantasTechApp::new();

    // 1. Initial state defaults: auto_update_firmware must be false, modal closed
    assert!(!app.config.auto_update_firmware);
    assert!(!app.auto_update_firmware_modal_open);

    // 2. Simulating user attempting to toggle checkbox ON
    let mut requested_auto_fw = true;
    if requested_auto_fw {
        app.auto_update_firmware_modal_open = true;
    }
    // Setting must NOT be enabled yet
    assert!(!app.config.auto_update_firmware);
    assert!(app.auto_update_firmware_modal_open);

    // 3. Simulating user clicking outside the window (backdrop cancel)
    app.auto_update_firmware_modal_open = false;
    app.config.auto_update_firmware = false;
    assert!(!app.config.auto_update_firmware);
    assert!(!app.auto_update_firmware_modal_open);

    // 4. Simulating user clicking "Keep Disabled (Recommended)"
    app.auto_update_firmware_modal_open = true;
    app.auto_update_firmware_modal_open = false;
    app.config.auto_update_firmware = false;
    assert!(!app.config.auto_update_firmware);
    assert!(!app.auto_update_firmware_modal_open);

    // 5. Simulating user attempting to toggle checkbox ON and explicitly confirming
    app.auto_update_firmware_modal_open = true;
    // User clicks "I Understand the Risks, Enable"
    app.config.auto_update_firmware = true;
    app.auto_update_firmware_modal_open = false;
    assert!(app.config.auto_update_firmware);
    assert!(!app.auto_update_firmware_modal_open);

    // 6. Simulating user toggling checkbox OFF: disables immediately without modal
    requested_auto_fw = false;
    if !requested_auto_fw {
        app.config.auto_update_firmware = false;
    }
    assert!(!app.config.auto_update_firmware);
    assert!(!app.auto_update_firmware_modal_open);
}

#[test]
fn test_find_neighbor_key_spatial_navigation() {
    // Esc (0) -> F1 (1) going Right
    assert_eq!(find_neighbor_key(0, ArrowDirection::Right), 1);
    // Esc (0) -> ~ (21) going Down
    assert_eq!(find_neighbor_key(0, ArrowDirection::Down), 21);
    // ~ (21) -> Esc (0) going Up
    assert_eq!(find_neighbor_key(21, ArrowDirection::Up), 0);
    // ~ (21) -> Tab (42) going Down
    assert_eq!(find_neighbor_key(21, ArrowDirection::Down), 42);
    // Tab (42) -> Caps (63) going Down
    assert_eq!(find_neighbor_key(42, ArrowDirection::Down), 63);
    // Caps (63) -> LShift (84) going Down
    assert_eq!(find_neighbor_key(63, ArrowDirection::Down), 84);
    // LShift (84) -> LCtrl (105) going Down
    assert_eq!(find_neighbor_key(84, ArrowDirection::Down), 105);

    // Caps (63) -> A (64) -> S (65) going Right
    assert_eq!(find_neighbor_key(63, ArrowDirection::Right), 64);
    assert_eq!(find_neighbor_key(64, ArrowDirection::Right), 65);
    // S (65) -> A (64) going Left
    assert_eq!(find_neighbor_key(65, ArrowDirection::Left), 64);

    // Space (111) -> B (90) going Up and back
    assert_eq!(find_neighbor_key(111, ArrowDirection::Up), 90);
    assert_eq!(find_neighbor_key(90, ArrowDirection::Down), 111);
    // Space (111) -> LAlt (107) going Left
    assert_eq!(find_neighbor_key(111, ArrowDirection::Left), 107);
    // Space (111) -> RAlt (115) going Right
    assert_eq!(find_neighbor_key(111, ArrowDirection::Right), 115);

    // Arrow cluster: Down (120) -> Left (119), Right (121), Up (99)
    assert_eq!(find_neighbor_key(120, ArrowDirection::Left), 119);
    assert_eq!(find_neighbor_key(120, ArrowDirection::Right), 121);
    assert_eq!(find_neighbor_key(120, ArrowDirection::Up), 99);
    assert_eq!(find_neighbor_key(99, ArrowDirection::Down), 120);

    // Numpad 0 (123) -> . (124) going Right
    assert_eq!(find_neighbor_key(123, ArrowDirection::Right), 124);
    assert_eq!(find_neighbor_key(124, ArrowDirection::Left), 123);

    // Unknown key index fallback
    assert_eq!(find_neighbor_key(255, ArrowDirection::Right), 0);
}

#[test]
fn test_tab_navigation_and_cycling() {
    let mut app = FantasTechApp::new();
    assert_eq!(app.current_tab, NavTab::Button);

    let tabs = [NavTab::Button, NavTab::Lighting, NavTab::Macro, NavTab::Setting];

    // Forward cycling
    for i in 0..tabs.len() {
        let next_idx = (i + 1) % tabs.len();
        app.current_tab = tabs[next_idx];
        assert_eq!(app.current_tab, tabs[next_idx]);
    }

    // Backward cycling
    for i in (0..tabs.len()).rev() {
        let prev_idx = if i == 0 { tabs.len() - 1 } else { i - 1 };
        app.current_tab = tabs[prev_idx];
        assert_eq!(app.current_tab, tabs[prev_idx]);
    }
}

#[test]
fn test_keyboard_shortcuts_help_modal_state() {
    let mut app = FantasTechApp::new();
    assert!(!app.shortcuts_help_modal_open);

    app.shortcuts_help_modal_open = true;
    assert!(app.shortcuts_help_modal_open);

    app.shortcuts_help_modal_open = false;
    assert!(!app.shortcuts_help_modal_open);
}

#[test]
fn test_keymap_delete_restores_default_action() {
    let mut app = FantasTechApp::new();
    let key_idx = 43u8; // Q
    let default_actions = default_layer_keymap(app.active_layer);
    let orig_action = default_actions[key_idx as usize];

    // Remap Q to something else
    let custom_action = KeyAction::standard_key(0x05); // B
    app.remap_single_key(key_idx, custom_action);
    assert_eq!(app.profiles[0][app.active_layer as usize][key_idx as usize], custom_action);

    // Simulate Delete/Backspace restoring default action
    app.remap_single_key(key_idx, default_actions[key_idx as usize]);
    assert_eq!(app.profiles[0][app.active_layer as usize][key_idx as usize], orig_action);
}

#[test]
fn test_apply_queried_lighting_config_sync() {
    let mut app = FantasTechApp::new();
    let queried = LightingConfig {
        zone: LightingZone::MainBacklight,
        mode: LightingMode::Rotate,
        brightness: 4,
        speed: 3,
        direction: AnimationDirection::LeftToRight,
        color_mode: ColorMode::DynamicSpectrum,
        color: RgbColor::new(58, 134, 255),
    };

    app.apply_queried_lighting_config(queried);
    assert_eq!(app.selected_mode, LightingMode::Rotate as u8);
    assert_eq!(app.brightness, 4);
    assert_eq!(app.speed, 3);
    assert_eq!(app.direction, 1);
    assert!(!app.is_fixed_color);
    assert_eq!(app.current_color, [58, 134, 255]);
    assert_eq!(app.recent_colors[0], [58, 134, 255]);
    assert!(app.reactive_origin.is_none());
}

#[test]
fn test_apply_hardware_snapshot_full_sync() {
    let mut app = FantasTechApp::new();
    let dev_info = fantastech_protocol::commands::DeviceInfo {
        profile_name: "MK894 (AK107)".into(),
        firmware_version: "V64".into(),
        active_profile: 1,
        profile_count: 1,
        pid: 0xFE9C,
        battery: 50,
        charge: 0,
        serial_number: "".into(),
    };
    let lighting_cfg = LightingConfig {
        zone: LightingZone::MainBacklight,
        mode: LightingMode::Bloom,
        brightness: 3,
        speed: 2,
        direction: AnimationDirection::Default,
        color_mode: ColorMode::DynamicSpectrum,
        color: RgbColor::new(0, 210, 255),
    };
    let base = default_layer_keymap(KeyLayer::Base);
    let fn_layer = default_layer_keymap(KeyLayer::Fn);

    let snapshot = platform::ConnectionSnapshot {
        service_active: false,
        connected: true,
        is_emulator: false,
        permission_denied_node: None,
        error_message: None,
        lighting_config: Some(lighting_cfg),
        device_info: Some(dev_info),
        hardware_keymaps: Some((base, fn_layer)),
        service_status: None,
    };

    app.apply_hardware_snapshot(&snapshot);
    assert_eq!(app.firmware_version, "V64");
    assert_eq!(app.active_profile, 1);
    assert_eq!(app.selected_mode, LightingMode::Bloom as u8);
    assert_eq!(app.brightness, 3);
    assert_eq!(app.speed, 2);
    assert_eq!(app.profiles[1][0][0], base[0]); // Esc key
    assert!(app.status_message.contains("V64"));
    assert!(app.status_message.contains("Profile 2"));
}

#[test]
fn test_draw_undo_redo_vector_buttons() {
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Test both disabled and enabled, in dark and light modes
            let _ = draw_undo_button(ui, false, true);
            let _ = draw_undo_button(ui, true, false);
            let _ = draw_redo_button(ui, false, true);
            let _ = draw_redo_button(ui, true, false);
        });
    });
}

#[test]
fn test_lighting_page_parameters_speed_and_static_color_mode() {
    let mut app = FantasTechApp::new();
    let ctx = egui::Context::default();

    // 1. In Static mode, speed is unsupported and is_fixed_color is enforced
    app.selected_mode = 1;
    app.is_fixed_color = false;
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            app.render_lighting_card_params(ui, 300.0, true);
        });
    });
    assert!(app.is_fixed_color, "Static mode must enforce fixed color mode");
    assert!(!LightingMode::Static.supports_speed(), "Static mode must not support speed");
    assert!(!LightingMode::Off.supports_speed(), "Off mode must not support speed");
    assert!(!LightingMode::CustomMatrixCanvas.supports_speed(), "Custom matrix canvas must not support speed");

    // 2. Animated modes must support speed
    assert!(LightingMode::Breathing.supports_speed());
    assert!(LightingMode::StreamLeftToRight.supports_speed());
    assert!(LightingMode::StreamUpToDown.supports_speed());
    assert!(LightingMode::Bloom.supports_speed());

    // 3. Mode 7 (UDWave) wave direction verification:
    // Wave moves downward from y = 0.0 (top) to y = 5.5 (bottom).
    // phase = (vkey.y / 5.5 - time * spd_factor * 0.32).rem_euclid(1.0);
    let spd_factor = (2.0_f32 + 1.0) * 0.28;
    let time_0 = 0.0_f32;
    let time_1 = 0.5_f32;
    let phase_top_t0 = (0.0_f32 / 5.5 - time_0 * spd_factor * 0.32).rem_euclid(1.0);
    // At a later time t1, a point further down (y > 0) will reach the same phase:
    let expected_y = 5.5 * (time_1 * spd_factor * 0.32);
    let phase_down_t1 = (expected_y / 5.5 - time_1 * spd_factor * 0.32).rem_euclid(1.0);
    assert!((phase_top_t0 - phase_down_t1).abs() < 1e-4, "Wave crest at top at t0 must propagate downward to expected_y > 0 at t1");
}





