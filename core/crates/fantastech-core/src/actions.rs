//! Key action string parsing, serialization, and layer map generation.

use crate::error::CoreError;
use crate::key_definitions::{get_mirror_key_index, KEY_DEFINITIONS};
use crate::scancodes::{default_scancode_for_key, key_name_for_scancode, media_name_for_usage, scancode_for_key_name};
use fantastech_protocol::constants::SPARSE_KEY_COUNT;
use fantastech_protocol::keymap::{ActionCategory, KeyAction, KeyLayer};

pub fn parse_key_identifier(input: &str) -> Result<u8, CoreError> {
    let trimmed = input.trim();
    if let Ok(idx) = trimmed.parse::<u8>() {
        if (idx as usize) < SPARSE_KEY_COUNT {
            return Ok(idx);
        }
    }

    let lower = trimmed.to_lowercase();
    for key in KEY_DEFINITIONS {
        if key.name.eq_ignore_ascii_case(&lower) {
            return Ok(key.index);
        }
    }

    // Secondary aliases
    match lower.as_str() {
        "ctrl" | "lctrl" => return Ok(105),
        "rctrl" => return Ok(118),
        "shift" | "lshift" => return Ok(84),
        "rshift" => return Ok(97),
        "alt" | "lalt" => return Ok(107),
        "ralt" => return Ok(115),
        "super" | "win" | "gui" => return Ok(106),
        "spc" | "spacebar" => return Ok(111),
        "ret" | "return" | "enter" => return Ok(76),
        "numenter" | "num enter" => return Ok(104),
        "caps" => return Ok(63),
        "del" => return Ok(56),
        "ins" => return Ok(35),
        "pgup" => return Ok(37),
        "pgdn" => return Ok(58),
        "prtscr" | "print" => return Ok(13),
        "scrlk" | "scroll" => return Ok(14),
        "pause" => return Ok(15),
        "num0" | "num 0" => return Ok(123),
        _ => {}
    }

    Err(CoreError::UnknownKeyName(input.to_string()))
}

/// Generates the factory default keymap table for a specified layer.
pub fn default_layer_keymap(layer: KeyLayer) -> [KeyAction; SPARSE_KEY_COUNT] {
    let mut map = [KeyAction::disabled(); SPARSE_KEY_COUNT];

    // Assign standard HID scancodes to all 104 physical keys
    for key in KEY_DEFINITIONS {
        if let Some(scancode) = default_scancode_for_key(key.name) {
            map[key.index as usize] = KeyAction::standard(scancode);
            if let Some(mirror) = get_mirror_key_index(key.index) {
                map[mirror as usize] = KeyAction::standard(scancode);
            }
        }
    }

    if layer == KeyLayer::Fn {
        // Fn Layer overrides matching official software:
        map[0] = KeyAction::hardware_custom(16); // Esc -> Reset
        map[1] = KeyAction::multimedia(0x0183);  // F1 -> Media
        map[2] = KeyAction::multimedia(0x00EA);  // F2 -> Volume -
        map[3] = KeyAction::multimedia(0x00E9);  // F3 -> Volume +
        map[4] = KeyAction::multimedia(0x00E2);  // F4 -> Mute
        map[5] = KeyAction::multimedia(0x00B7);  // F5 -> Stop
        map[6] = KeyAction::multimedia(0x00B6);  // F6 -> Prev Track
        map[7] = KeyAction::multimedia(0x00CD);  // F7 -> Play/Pause
        map[8] = KeyAction::multimedia(0x00B5);  // F8 -> Next Track
        map[9] = KeyAction::multimedia(0x0221);  // F9 -> Search
        map[10] = KeyAction::multimedia(0x0223); // F10 -> Homepage
        map[11] = KeyAction::multimedia(0x0194); // F11 -> My PC
        map[12] = KeyAction::multimedia(0x0192); // F12 -> Calculator
        map[15] = KeyAction::multimedia(0x00CD); // Pause -> Play/Pause
        map[35] = KeyAction::hardware_custom(3); // Insert -> LedMode
        map[56] = KeyAction::hardware_custom(4); // Delete -> LedColor
        map[99] = KeyAction::hardware_custom(1); // Up -> LedBrt+
        map[120] = KeyAction::hardware_custom(2); // Down -> LedBrt-
        map[119] = KeyAction::hardware_custom(6); // Left -> Speed-
        map[121] = KeyAction::hardware_custom(5); // Right -> Speed+
        map[106] = KeyAction::win_lock();         // Win -> WinLock
        map[111] = KeyAction::hardware_custom(0); // Space -> Led On/Off
        map[110] = KeyAction::hardware_custom(0); // Space mirror -> Led On/Off
    }

    map
}

/// Parses a human-friendly action string into a strongly-typed `KeyAction`.
pub fn parse_action_str(s: &str) -> Result<KeyAction, CoreError> {
    let trimmed = s.trim();
    let lower = trimmed.to_lowercase();
    if lower.is_empty() || lower == "none" || lower == "disabled" || lower == "unmapped" {
        return Ok(KeyAction::disabled());
    }

    // Normalized string without spaces, slashes, or underscores for fast canonical token matching
    let stripped: String = lower
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '/')
        .collect();

    // 1. Hardware Lighting / Custom Key Actions (matches both bare "LedMode", "WinLock" and "custom:led-mode"):
    let custom_cand = stripped.strip_prefix("custom:").unwrap_or(&stripped);
    match custom_cand {
        "winlock" | "win-lock" | "guilock" | "gui-lock" => return Ok(KeyAction::win_lock()),
        "ledonoff" | "led-on-off" | "ledtoggle" | "led-toggle" | "toggleled" | "off" => return Ok(KeyAction::hardware_custom(0)),
        "brightness+" | "brt+" | "ledbrt+" | "led-brt+" => return Ok(KeyAction::hardware_custom(1)),
        "brightness-" | "brt-" | "ledbrt-" | "led-brt-" => return Ok(KeyAction::hardware_custom(2)),
        "ledmode" | "led-mode" | "mode" | "effect" => return Ok(KeyAction::hardware_custom(3)),
        "color" | "colorcycle" | "color-cycle" | "ledcolor" | "led-color" => return Ok(KeyAction::hardware_custom(4)),
        "speed+" | "spd+" => return Ok(KeyAction::hardware_custom(5)),
        "speed-" | "spd-" => return Ok(KeyAction::hardware_custom(6)),
        "mac" | "macdevice" | "mac-device" => return Ok(KeyAction::hardware_custom(13)),
        "win" | "windows" | "windevice" | "win-device" => return Ok(KeyAction::hardware_custom(14)),
        "reset" => return Ok(KeyAction::hardware_custom(16)),
        _ => {}
    }
    if let Some(rest) = lower.strip_prefix("custom:") {
        if let Ok(num) = rest.parse::<u8>() {
            return Ok(KeyAction::hardware_custom(num));
        }
        return Err(CoreError::UnknownAction(s.to_string()));
    }

    // 2. Mouse actions:
    let mouse_cand = stripped.strip_prefix("mouse:").unwrap_or_else(|| stripped.strip_prefix("mouse").unwrap_or(""));
    if !mouse_cand.is_empty() {
        return match mouse_cand {
            "left" | "lclick" | "btn1" => Ok(KeyAction::mouse_button(1)),
            "right" | "rclick" | "btn2" => Ok(KeyAction::mouse_button(2)),
            "middle" | "mclick" | "btn3" => Ok(KeyAction::mouse_button(4)),
            "backward" | "back" | "btn4" => Ok(KeyAction::mouse_button(8)),
            "forward" | "btn5" => Ok(KeyAction::mouse_button(16)),
            "wheelup" | "wheel-up" | "scrollup" | "scroll-up" => Ok(KeyAction::mouse_wheel(1)),
            "wheeldown" | "wheel-down" | "scrolldown" | "scroll-down" => Ok(KeyAction::mouse_wheel(-1)),
            other => {
                if let Ok(btn) = other.parse::<u8>() {
                    Ok(KeyAction::mouse_button(btn))
                } else {
                    Err(CoreError::UnknownAction(s.to_string()))
                }
            }
        };
    }

    // 3. Macro trigger actions: "macro:0:hold", "M0:hold", "M0", "macro:0"
    let macro_body = if let Some(rest) = lower.strip_prefix("macro:") {
        Some(rest)
    } else if lower.starts_with('m') && lower.len() >= 2 && lower.chars().nth(1).map(|c| c.is_ascii_digit()).unwrap_or(false) {
        Some(&lower[1..])
    } else {
        None
    };
    if let Some(rest) = macro_body {
        let parts: Vec<&str> = rest.split(':').collect();
        if let Ok(slot) = parts[0].parse::<u8>() {
            if slot <= 15 {
                let mut loop_mode = 0u8;
                let mut repeat = 1u8;
                if parts.len() > 1 {
                    match parts[1] {
                        "hold" | "untilreleased" | "until-released" => loop_mode = 1,
                        "anykey" | "untilanykey" | "until-any-key" => loop_mode = 2,
                        "toggle" | "untilpressedagain" | "until-pressed-again" => loop_mode = 3,
                        rep if rep.starts_with("repeat=") => {
                            repeat = rep.strip_prefix("repeat=").and_then(|v| v.parse().ok()).unwrap_or(1);
                            loop_mode = 0;
                        }
                        "once" | "repeat" => {
                            loop_mode = 0;
                            repeat = 1;
                        }
                        _ => {}
                    }
                }
                return Ok(KeyAction::macro_trigger(slot, loop_mode, repeat));
            }
        }
    }

    // 4. Multimedia & OS actions:
    match stripped.as_str() {
        "vol+" | "volume+" | "volumeup" | "volup" | "vol-up" => return Ok(KeyAction::multimedia(0x00E9)),
        "vol-" | "volume-" | "volumedown" | "voldown" | "vol-down" => return Ok(KeyAction::multimedia(0x00EA)),
        "mute" => return Ok(KeyAction::multimedia(0x00E2)),
        "play" | "pause" | "playpause" | "play-pause" => return Ok(KeyAction::multimedia(0x00CD)),
        "stop" => return Ok(KeyAction::multimedia(0x00B7)),
        "prev" | "prevtrack" | "prev-track" => return Ok(KeyAction::multimedia(0x00B6)),
        "next" | "nexttrack" | "next-track" => return Ok(KeyAction::multimedia(0x00B5)),
        "media" | "mediaplayer" => return Ok(KeyAction::multimedia(0x0183)),
        "calc" | "calculator" => return Ok(KeyAction::multimedia(0x0192)),
        "mypc" | "mycomputer" | "thispc" | "my-pc" => return Ok(KeyAction::multimedia(0x0194)),
        "mail" | "email" => return Ok(KeyAction::multimedia(0x018A)),
        "browser" | "homepage" | "webhome" | "web-home" => return Ok(KeyAction::multimedia(0x0223)),
        "search" | "websearch" | "web-search" => return Ok(KeyAction::multimedia(0x0221)),
        "webrefresh" | "refresh" | "web-refresh" => return Ok(KeyAction::multimedia(0x0227)),
        "webstop" | "web-stop" => return Ok(KeyAction::multimedia(0x0226)),
        "webforward" | "web-forward" => return Ok(KeyAction::multimedia(0x0225)),
        "webback" | "webbackward" | "web-back" | "web-backward" => return Ok(KeyAction::multimedia(0x0224)),
        "webfavorites" | "favorites" | "web-favorites" => return Ok(KeyAction::multimedia(0x022A)),
        "displaybrt+" | "screenbrt+" | "display-brt+" | "screen-brt+" => return Ok(KeyAction::multimedia(0x006F)),
        "displaybrt-" | "screenbrt-" | "display-brt-" | "screen-brt-" => return Ok(KeyAction::multimedia(0x0070)),
        "power" => return Ok(KeyAction::power(1)),
        "sleep" => return Ok(KeyAction::power(2)),
        "wake" | "wakeup" => return Ok(KeyAction::power(4)),
        _ => {}
    }

    // 5. Key combinations (separated by '+'):
    if lower.contains('+') {
        let parts: Vec<&str> = lower.split('+').collect();
        let mut modifiers = 0u8;
        let mut base_key = "";

        for part in parts {
            let p = part.trim();
            match p {
                "ctrl" | "lctrl" | "rctrl" => modifiers |= 0x01,
                "shift" | "lshift" | "rshift" => modifiers |= 0x02,
                "alt" | "lalt" | "ralt" => modifiers |= 0x04,
                "win" | "lwin" | "rwin" | "gui" | "super" => modifiers |= 0x08,
                key => base_key = key,
            }
        }

        if !base_key.is_empty() {
            if let Some(scancode) = scancode_for_key_name(base_key) {
                return Ok(KeyAction::combo(modifiers, scancode));
            }
        }
    }

    // 6. Direct numeric scancode (decimal or 0x hex):
    if let Ok(scancode) = lower.parse::<u8>() {
        return Ok(KeyAction::standard(scancode));
    }
    if let Some(hex_str) = lower.strip_prefix("0x") {
        if let Ok(scancode) = u8::from_str_radix(hex_str, 16) {
            return Ok(KeyAction::standard(scancode));
        }
    }

    // 7. Standard individual key scancode:
    if let Some(scancode) = scancode_for_key_name(&lower) {
        return Ok(KeyAction::standard(scancode));
    }

    Err(CoreError::UnknownAction(s.to_string()))
}

/// Formats a `KeyAction` into a concise, human-friendly display string.
pub fn format_action_str(action: &KeyAction) -> String {
    match action.category {
        ActionCategory::Disabled => "Disabled".to_string(),
        ActionCategory::StandardHidKey => {
            let base_name = key_name_for_scancode(action.code2).unwrap_or("Unknown");
            if action.code1 == 0 {
                base_name.to_string()
            } else {
                let mut mods = Vec::new();
                if (action.code1 & 0x01) != 0 || (action.code1 & 0x10) != 0 {
                    mods.push("Ctrl");
                }
                if (action.code1 & 0x08) != 0 || (action.code1 & 0x80) != 0 {
                    mods.push("Win");
                }
                if (action.code1 & 0x04) != 0 || (action.code1 & 0x40) != 0 {
                    mods.push("Alt");
                }
                if (action.code1 & 0x02) != 0 || (action.code1 & 0x20) != 0 {
                    mods.push("Shift");
                }
                mods.push(base_name);
                mods.join("+")
            }
        }
        ActionCategory::MultimediaOs => {
            let usage = (action.code1 as u16) | ((action.code2 as u16) << 8);
            media_name_for_usage(usage).unwrap_or("Multimedia").to_string()
        }
        ActionCategory::HardwareLighting => {
            match action.code1 {
                0x07 => "WinLock".to_string(),
                0 => "Led On/Off".to_string(),
                1 => "LedBrt+".to_string(),
                2 => "LedBrt-".to_string(),
                3 => "LedMode".to_string(),
                4 => "LedColor".to_string(),
                5 => "Speed+".to_string(),
                6 => "Speed-".to_string(),
                13 => "Mac Device".to_string(),
                14 => "Win Device".to_string(),
                16 => "Reset".to_string(),
                other => format!("Custom:{}", other),
            }
        }
        ActionCategory::MouseAction => {
            if action.code3 == 1 {
                "Mouse:WheelUp".to_string()
            } else if action.code3 == 255 {
                "Mouse:WheelDown".to_string()
            } else {
                match action.code1 {
                    1 => "Mouse:Left".to_string(),
                    2 => "Mouse:Right".to_string(),
                    4 => "Mouse:Middle".to_string(),
                    8 => "Mouse:Backward".to_string(),
                    16 => "Mouse:Forward".to_string(),
                    other => format!("Mouse:{}", other),
                }
            }
        }
        ActionCategory::CursorControl => format!("CursorMove({}, {})", action.code1 as i8, action.code2 as i8),
        ActionCategory::PowerManagement => {
            match action.code1 {
                1 => "Power".to_string(),
                2 => "Sleep".to_string(),
                4 => "Wakeup".to_string(),
                other => format!("Power:{}", other),
            }
        }
        ActionCategory::HardwareMacro => {
            let mode_str = match action.code3 {
                1 => ":hold",
                2 => ":anykey",
                3 => ":toggle",
                _ if action.code2 > 1 => ":repeat",
                _ => "",
            };
            format!("M{}{}", action.code1, mode_str)
        }
    }
}

