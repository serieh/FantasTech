//! OpenRGB SDK network protocol framing and serialization for Fantech MK894.

use crate::error::ProtocolError;
use crate::lighting::{LightingMode, RgbColor};

/// OpenRGB protocol magic identifier ("ORGB").
pub const OPENRGB_MAGIC: &[u8; 4] = b"ORGB";

/// Default TCP port for OpenRGB SDK server (standard ORGB port 6742).
pub const OPENRGB_DEFAULT_PORT: u16 = 6742;

/// OpenRGB standard device type constants.
pub const DEVICE_TYPE_MOTHERBOARD: i32 = 0;
pub const DEVICE_TYPE_DRAM: i32 = 1;
pub const DEVICE_TYPE_GPU: i32 = 2;
pub const DEVICE_TYPE_COOLER: i32 = 3;
pub const DEVICE_TYPE_LEDSTRIP: i32 = 4;
pub const DEVICE_TYPE_KEYBOARD: i32 = 5;
pub const DEVICE_TYPE_MOUSE: i32 = 6;

/// Standard OpenRGB SDK packet command identifiers.
pub const NET_PACKET_ID_REQUEST_CONTROLLER_COUNT: u32 = 0;
pub const NET_PACKET_ID_REQUEST_CONTROLLER_DATA: u32 = 1;
pub const NET_PACKET_ID_REQUEST_PROTOCOL_VERSION: u32 = 40;
pub const NET_PACKET_ID_SET_CLIENT_NAME: u32 = 50;
pub const NET_PACKET_ID_DEVICE_LIST_UPDATED: u32 = 100;
pub const NET_PACKET_ID_RGBCONTROLLER_RESIZEZONE: u32 = 1000;
pub const NET_PACKET_ID_RGBCONTROLLER_UPDATELEDS: u32 = 1050;
pub const NET_PACKET_ID_RGBCONTROLLER_UPDATEZONELEDS: u32 = 1051;
pub const NET_PACKET_ID_RGBCONTROLLER_UPDATESINGLELED: u32 = 1052;
pub const NET_PACKET_ID_RGBCONTROLLER_SETCUSTOMMODE: u32 = 1100;
pub const NET_PACKET_ID_RGBCONTROLLER_SETMODE: u32 = 1101;
pub const NET_PACKET_ID_RGBCONTROLLER_SAVEMODE: u32 = 1102;

/// 16-byte standard OpenRGB SDK packet header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenRgbHeader {
    pub magic: [u8; 4],
    pub dev_id: u32,
    pub pkt_id: u32,
    pub pkt_size: u32,
}

impl OpenRgbHeader {
    pub fn new(dev_id: u32, pkt_id: u32, pkt_size: u32) -> Self {
        Self {
            magic: *OPENRGB_MAGIC,
            dev_id,
            pkt_id,
            pkt_size,
        }
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..8].copy_from_slice(&self.dev_id.to_le_bytes());
        buf[8..12].copy_from_slice(&self.pkt_id.to_le_bytes());
        buf[12..16].copy_from_slice(&self.pkt_size.to_le_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < 16 {
            return Err(ProtocolError::InvalidLength(bytes.len()));
        }
        if &bytes[0..4] != OPENRGB_MAGIC {
            return Err(ProtocolError::InvalidReportId(bytes[0]));
        }
        let dev_id = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or_default());
        let pkt_id = u32::from_le_bytes(bytes[8..12].try_into().unwrap_or_default());
        let pkt_size = u32::from_le_bytes(bytes[12..16].try_into().unwrap_or_default());

        // Defensive bounding: enforce maximum 1 MB packet length to prevent memory exhaustion
        if pkt_size > 1_048_576 {
            return Err(ProtocolError::InvalidLength(pkt_size as usize));
        }

        Ok(Self {
            magic: *OPENRGB_MAGIC,
            dev_id,
            pkt_id,
            pkt_size,
        })
    }
}

/// Parsed data from an OpenRGB `RGBCONTROLLER_SETMODE` or `RGBCONTROLLER_SAVEMODE` packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRgbModeUpdate {
    pub mode_idx: i32,
    pub name: String,
    pub value: i32,
    pub flags: u32,
    pub speed: u32,
    pub brightness: u32,
    pub direction: u32,
    pub color_mode: u32,
    pub colors: Vec<RgbColor>,
}

/// Helper to serialize a null-terminated string with its 16-bit length prefix.
fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = (bytes.len() + 1) as u16;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(bytes);
    buf.push(0x00);
}

/// Helper to serialize an RGB color (R, G, B, 0x00).
fn write_color(buf: &mut Vec<u8>, color: RgbColor) {
    buf.push(color.r);
    buf.push(color.g);
    buf.push(color.b);
    buf.push(0x00);
}

#[inline]
fn read_u16_le(slice: &[u8], offset: usize) -> Option<u16> {
    slice.get(offset..offset + 2).and_then(|s| s.try_into().ok()).map(u16::from_le_bytes)
}

#[inline]
fn read_u32_le(slice: &[u8], offset: usize) -> Option<u32> {
    slice.get(offset..offset + 4).and_then(|s| s.try_into().ok()).map(u32::from_le_bytes)
}

#[inline]
fn read_i32_le(slice: &[u8], offset: usize) -> Option<i32> {
    slice.get(offset..offset + 4).and_then(|s| s.try_into().ok()).map(i32::from_le_bytes)
}

/// Normalizes an input brightness setting (0..=100 or 0..=4) to a percentage (0..=100).
pub fn normalize_brightness_to_percent(b: u32) -> u8 {
    if b == 0 {
        0
    } else if b <= 4 {
        // Discrete 1..4 levels mapped to 25%, 50%, 75%, 100%
        (b as u8) * 25
    } else {
        b.min(100) as u8
    }
}

/// Normalizes an input brightness setting to discrete hardware levels (0..=4).
pub fn normalize_brightness_to_levels(b: u32) -> u8 {
    let pct = normalize_brightness_to_percent(b);
    match pct {
        0 => 0,
        1..=25 => 1,
        26..=50 => 2,
        51..=75 => 3,
        _ => 4,
    }
}

// OpenRGB Direction Constants
pub const OPENRGB_DIR_LEFT: u32 = 0;
pub const OPENRGB_DIR_RIGHT: u32 = 1;
pub const OPENRGB_DIR_UP: u32 = 2;
pub const OPENRGB_DIR_DOWN: u32 = 3;
pub const OPENRGB_DIR_HORIZONTAL: u32 = 4;
pub const OPENRGB_DIR_VERTICAL: u32 = 5;

// OpenRGB Mode Flags
pub const MODE_FLAG_HAS_SPEED: u32 = 1 << 0;
pub const MODE_FLAG_HAS_DIRECTION_LR: u32 = 1 << 1;
pub const MODE_FLAG_HAS_DIRECTION_UD: u32 = 1 << 2;
pub const MODE_FLAG_HAS_DIRECTION_HV: u32 = 1 << 3;
pub const MODE_FLAG_HAS_BRIGHTNESS: u32 = 1 << 4;
pub const MODE_FLAG_HAS_PER_LED_COLOR: u32 = 1 << 5;
pub const MODE_FLAG_HAS_MODE_SPECIFIC_COLOR: u32 = 1 << 6;
pub const MODE_FLAG_HAS_RANDOM_COLOR: u32 = 1 << 7;

// OpenRGB Color Modes
pub const MODE_COLORS_NONE: u32 = 0;
pub const MODE_COLORS_PER_LED: u32 = 1;
pub const MODE_COLORS_MODE_SPECIFIC: u32 = 2;
pub const MODE_COLORS_RANDOM: u32 = 3;

/// Scales an RGB color by brightness percentage (0..=100).
pub fn scale_rgb(col: RgbColor, brightness_pct: u8) -> RgbColor {
    let pct = brightness_pct.min(100) as u32;
    RgbColor::new(
        ((col.r as u32 * pct) / 100) as u8,
        ((col.g as u32 * pct) / 100) as u8,
        ((col.b as u32 * pct) / 100) as u8,
    )
}

/// Standardizes a key name to OpenRGB's canonical LED naming convention.
/// OpenRGB GUI parses these exact strings to render readable key labels on matrix buttons.
pub fn to_openrgb_led_name(name: &str) -> String {
    let raw = name.strip_prefix("Key: ").unwrap_or(name);
    let mapped = match raw {
        "Esc" | "Escape" => "Key: Escape",
        "PrtSc" | "Print" | "Print Screen" => "Key: Print Screen",
        "ScrLK" | "ScrLock" | "Scroll Lock" => "Key: Scroll Lock",
        "Pause" | "Pause/Break" => "Key: Pause/Break",
        "~" | "`" => "Key: `",
        "-" => "Key: -",
        "+" | "=" => "Key: =",
        "Backspace" | "Back" => "Key: Backspace",
        "Insert" => "Key: Insert",
        "Home" => "Key: Home",
        "PageUp" | "PgUp" | "Page Up" => "Key: Page Up",
        "Delete" | "Delet" => "Key: Delete",
        "End" => "Key: End",
        "PageDown" | "PgDown" | "Page Down" => "Key: Page Down",
        "CapsLock" | "Caps" | "Caps Lock" => "Key: Caps Lock",
        "[" => "Key: [",
        "]" => "Key: ]",
        "\\" => "Key: \\",
        ";" => "Key: ;",
        "'" => "Key: '",
        "Enter" => "Key: Enter",
        "LeftShift" => "Key: Left Shift",
        "RightShift" => "Key: Right Shift",
        "<" | "," => "Key: ,",
        ">" | "." => "Key: .",
        "/" => "Key: /",
        "Up" | "Up Arrow" => "Key: Up Arrow",
        "LeftCtrl" | "Left Control" => "Key: Left Control",
        "RightCtrl" | "Right Control" => "Key: Right Control",
        "Win" | "Left Windows" => "Key: Left Windows",
        "LeftAlt" | "Left Alt" => "Key: Left Alt",
        "RightAlt" | "Right Alt" => "Key: Right Alt",
        "Space" => "Key: Space",
        "Fn" | "Function" | "Right Fn" => "Key: Right Fn",
        "App" | "Menu" => "Key: Menu",
        "Left" | "Left Arrow" => "Key: Left Arrow",
        "Down" | "Down Arrow" => "Key: Down Arrow",
        "Right" | "Right Arrow" => "Key: Right Arrow",
        "NumLock" | "Num Lock" => "Key: Num Lock",
        "Num /" | "Number Pad /" => "Key: Number Pad /",
        "Num *" | "Number Pad *" => "Key: Number Pad *",
        "Num -" | "Number Pad -" => "Key: Number Pad -",
        "Num +" | "Number Pad +" => "Key: Number Pad +",
        "Num Enter" | "Number Pad Enter" => "Key: Number Pad Enter",
        "Num ." | "Number Pad ." => "Key: Number Pad .",
        "Num 0" | "Number Pad 0" => "Key: Number Pad 0",
        "Num 1" | "Number Pad 1" => "Key: Number Pad 1",
        "Num 2" | "Number Pad 2" => "Key: Number Pad 2",
        "Num 3" | "Number Pad 3" => "Key: Number Pad 3",
        "Num 4" | "Number Pad 4" => "Key: Number Pad 4",
        "Num 5" | "Number Pad 5" => "Key: Number Pad 5",
        "Num 6" | "Number Pad 6" => "Key: Number Pad 6",
        "Num 7" | "Number Pad 7" => "Key: Number Pad 7",
        "Num 8" | "Number Pad 8" => "Key: Number Pad 8",
        "Num 9" | "Number Pad 9" => "Key: Number Pad 9",
        other => return format!("Key: {}", other),
    };
    mapped.to_string()
}

/// Builds the OpenRGB Controller Data payload for the Fantech MK894.
pub fn build_openrgb_controller_data(
    keys: &[(&str, u8, u8, u8)], // (name, row, col, index)
    current_colors: &[RgbColor],
    current_mode: u8,
    current_brightness: u8,
    _current_direction: u32,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(4096);

    // Placeholder for total data_size (u32)
    payload.extend_from_slice(&0u32.to_le_bytes());

    // Device Type: 5 = DEVICE_TYPE_KEYBOARD
    payload.extend_from_slice(&DEVICE_TYPE_KEYBOARD.to_le_bytes());

    // Strings
    write_string(&mut payload, "Fantech MK894 Maxpower II");
    write_string(&mut payload, "Fantech");
    write_string(&mut payload, "Fantech MK894 Mechanical RGB Keyboard (Clean-Room Driver)");
    write_string(&mut payload, "1.0.0");
    write_string(&mut payload, "003123456789");
    write_string(&mut payload, "USB: 36AE:FE9C");

    // Complete Modes table matching MK894 firmware:
    // (name, value, flags, speed_min, speed_max, color_mode, speed, colors_min, colors_max)
    type ModeEntry = (&'static str, i32, u32, u32, u32, u32, u32, u32, u32);
    let modes: &[ModeEntry] = &[
        ("Direct (Per-Key Custom)", 16, MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_PER_LED_COLOR, 0, 0, MODE_COLORS_PER_LED, 0, 0, 0),
        ("Static (Constant)", 1, MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR, 0, 0, MODE_COLORS_MODE_SPECIFIC, 0, 1, 1),
        ("Breathing", 2, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_RANDOM_COLOR, 0, 4, MODE_COLORS_MODE_SPECIFIC, 2, 1, 1),
        ("Rainbow Wave (Horizontal)", 5, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_RANDOM_COLOR, 0, 4, MODE_COLORS_MODE_SPECIFIC, 2, 1, 1),
        ("Rainbow Wave (Vertical)", 7, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_RANDOM_COLOR, 0, 4, MODE_COLORS_MODE_SPECIFIC, 2, 1, 1),
        ("Spectrum Cycle", 4, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS, 0, 4, MODE_COLORS_NONE, 2, 0, 0),
        ("Rotate Wheel", 3, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS, 0, 4, MODE_COLORS_NONE, 2, 0, 0),
        ("Bloom Pulse", 6, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_RANDOM_COLOR, 0, 4, MODE_COLORS_MODE_SPECIFIC, 2, 1, 1),
        ("Shuttle", 8, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_RANDOM_COLOR, 0, 4, MODE_COLORS_MODE_SPECIFIC, 2, 1, 1),
        ("Raindrop", 9, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_RANDOM_COLOR, 0, 4, MODE_COLORS_MODE_SPECIFIC, 2, 1, 1),
        ("Meteor", 10, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS, 0, 4, MODE_COLORS_NONE, 2, 0, 0),
        ("Ripple Fade", 11, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_RANDOM_COLOR, 0, 4, MODE_COLORS_MODE_SPECIFIC, 2, 1, 1),
        ("Ripple Static", 12, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_RANDOM_COLOR, 0, 4, MODE_COLORS_MODE_SPECIFIC, 2, 1, 1),
        ("Reactive (Key Glow)", 13, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_RANDOM_COLOR, 0, 4, MODE_COLORS_MODE_SPECIFIC, 2, 1, 1),
        ("Grid Ripple", 14, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_RANDOM_COLOR, 0, 4, MODE_COLORS_MODE_SPECIFIC, 2, 1, 1),
        ("Tide Ocean", 15, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_RANDOM_COLOR, 0, 4, MODE_COLORS_MODE_SPECIFIC, 2, 1, 1),
        ("Off", 0, 0, 0, 0, MODE_COLORS_NONE, 0, 0, 0),
    ];

    payload.extend_from_slice(&(modes.len() as u16).to_le_bytes());
    let active_mode_idx = modes.iter().position(|m| m.1 == current_mode as i32).unwrap_or(0) as i32;
    payload.extend_from_slice(&active_mode_idx.to_le_bytes());

    for (name, val, flags, spd_min, spd_max, col_mode, spd, col_min, col_max) in modes {
        write_string(&mut payload, name);
        payload.extend_from_slice(&val.to_le_bytes());
        payload.extend_from_slice(&flags.to_le_bytes());
        payload.extend_from_slice(&spd_min.to_le_bytes());
        payload.extend_from_slice(&spd_max.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes()); // brightness_min
        payload.extend_from_slice(&100u32.to_le_bytes()); // brightness_max (0..100% scale)
        payload.extend_from_slice(&col_min.to_le_bytes()); // colors_min
        payload.extend_from_slice(&col_max.to_le_bytes()); // colors_max
        payload.extend_from_slice(&spd.to_le_bytes()); // speed
        let brt_val = normalize_brightness_to_percent(current_brightness as u32) as u32;
        payload.extend_from_slice(&brt_val.to_le_bytes()); // brightness (protocol version 3+)
        let mode_dir = OPENRGB_DIR_LEFT;
        payload.extend_from_slice(&mode_dir.to_le_bytes()); // direction
        payload.extend_from_slice(&col_mode.to_le_bytes()); // color_mode
        payload.extend_from_slice(&(*col_max as u16).to_le_bytes()); // colors count
        for _ in 0..*col_max {
            write_color(&mut payload, RgbColor::new(255, 0, 0)); // default mode color
        }
    }

    // Zones (1 Zone: Matrix 6 rows x 21 cols = 126 slots)
    payload.extend_from_slice(&1u16.to_le_bytes()); // zones count = 1
    write_string(&mut payload, "Keyboard Matrix");
    payload.extend_from_slice(&1i32.to_le_bytes()); // zone_type: 1 = ZONE_TYPE_MATRIX
    payload.extend_from_slice(&0u32.to_le_bytes()); // leds_min
    payload.extend_from_slice(&(keys.len() as u32).to_le_bytes()); // leds_max
    payload.extend_from_slice(&(keys.len() as u32).to_le_bytes()); // leds_count

    // Matrix Map
    let rows = 6u32;
    let cols = 21u32;
    let matrix_len = (4 + 4 + (rows * cols * 4)) as u16; // zone_matrix_len (unsigned short)
    payload.extend_from_slice(&matrix_len.to_le_bytes());
    payload.extend_from_slice(&rows.to_le_bytes());
    payload.extend_from_slice(&cols.to_le_bytes());

    for r in 0..rows {
        for c in 0..cols {
            // Find key in this row and col
            if let Some(pos) = keys.iter().position(|k| k.1 == r as u8 && k.2 == c as u8) {
                payload.extend_from_slice(&(pos as u32).to_le_bytes());
            } else {
                payload.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // Unpopulated
            }
        }
    }

    // LEDs array
    payload.extend_from_slice(&(keys.len() as u16).to_le_bytes());
    for (name, _, _, idx) in keys {
        write_string(&mut payload, &to_openrgb_led_name(name));
        payload.extend_from_slice(&(*idx as u32).to_le_bytes());
    }

    // Colors array
    payload.extend_from_slice(&(keys.len() as u16).to_le_bytes());
    for i in 0..keys.len() {
        let c = if i < current_colors.len() {
            current_colors[i]
        } else {
            RgbColor::new(0, 0, 0)
        };
        write_color(&mut payload, c);
    }

    // Write final total data_size at byte 0..4
    let total_len = payload.len() as u32;
    payload[0..4].copy_from_slice(&total_len.to_le_bytes());

    payload
}

/// Parses an OpenRGB `RGBCONTROLLER_UPDATELEDS` packet body.
/// Returns the list of parsed `RgbColor` values.
pub fn parse_openrgb_update_leds(payload: &[u8]) -> Result<Vec<RgbColor>, ProtocolError> {
    let num_colors = read_u16_le(payload, 4).ok_or(ProtocolError::InvalidLength(payload.len()))? as usize;
    let expected_len = 6 + (num_colors * 4);
    if payload.len() < expected_len {
        return Err(ProtocolError::InvalidLength(payload.len()));
    }

    let mut colors = Vec::with_capacity(num_colors);
    for i in 0..num_colors {
        let offset = 6 + (i * 4);
        let r = payload[offset];
        let g = payload[offset + 1];
        let b = payload[offset + 2];
        colors.push(RgbColor::new(r, g, b));
    }

    Ok(colors)
}

/// Parses an OpenRGB `RGBCONTROLLER_UPDATEZONELEDS` packet body.
/// Returns (zone_index, list of `RgbColor` values).
pub fn parse_openrgb_update_zone_leds(payload: &[u8]) -> Result<(u32, Vec<RgbColor>), ProtocolError> {
    let zone_idx = read_u32_le(payload, 4).ok_or(ProtocolError::InvalidLength(payload.len()))?;
    let num_colors = read_u16_le(payload, 8).ok_or(ProtocolError::InvalidLength(payload.len()))? as usize;
    let expected_len = 10 + (num_colors * 4);
    if payload.len() < expected_len {
        return Err(ProtocolError::InvalidLength(payload.len()));
    }

    let mut colors = Vec::with_capacity(num_colors);
    for i in 0..num_colors {
        let offset = 10 + (i * 4);
        let r = payload[offset];
        let g = payload[offset + 1];
        let b = payload[offset + 2];
        colors.push(RgbColor::new(r, g, b));
    }

    Ok((zone_idx, colors))
}

/// Parses an OpenRGB `RGBCONTROLLER_UPDATESINGLELED` packet body.
/// Returns (led_index, color).
pub fn parse_openrgb_update_single_led(payload: &[u8]) -> Result<(i32, RgbColor), ProtocolError> {
    if payload.len() < 7 {
        return Err(ProtocolError::InvalidLength(payload.len()));
    }
    let led_index = read_i32_le(payload, 0).ok_or(ProtocolError::InvalidLength(payload.len()))?;
    let r = payload[4];
    let g = payload[5];
    let b = payload[6];
    Ok((led_index, RgbColor::new(r, g, b)))
}

/// Parses an OpenRGB `RGBCONTROLLER_SETMODE` or `RGBCONTROLLER_SAVEMODE` packet body.
/// Extracts mode index, name, hardware value, speed, brightness, direction, color mode, and colors.
pub fn parse_openrgb_set_mode(payload: &[u8]) -> Result<OpenRgbModeUpdate, ProtocolError> {
    let mode_idx = read_i32_le(payload, 4).ok_or(ProtocolError::InvalidLength(payload.len()))?;
    if payload.len() < 10 {
        return Ok(OpenRgbModeUpdate {
            mode_idx,
            name: String::new(),
            value: 0,
            flags: 0,
            speed: 2,
            brightness: 4,
            direction: 0,
            color_mode: 0,
            colors: Vec::new(),
        });
    }

    let mut pos = 8;
    let str_len = read_u16_le(payload, pos).ok_or(ProtocolError::InvalidLength(payload.len()))? as usize;
    pos += 2;
    if payload.len() < pos + str_len {
        return Err(ProtocolError::InvalidLength(payload.len()));
    }
    let name = String::from_utf8_lossy(&payload[pos..pos + str_len])
        .trim_matches('\0')
        .to_string();
    pos += str_len;

    if payload.len() < pos + 48 {
        return Ok(OpenRgbModeUpdate {
            mode_idx,
            name,
            value: 0,
            flags: 0,
            speed: 2,
            brightness: 4,
            direction: 0,
            color_mode: 0,
            colors: Vec::new(),
        });
    }

    let value = read_i32_le(payload, pos).unwrap_or(0);
    let flags = read_u32_le(payload, pos + 4).unwrap_or(0);
    // skip spd_min (4), spd_max (4), brt_min (4), brt_max (4), col_min (4), col_max (4) = 24 bytes
    let speed = read_u32_le(payload, pos + 32).unwrap_or(2);
    let brightness = read_u32_le(payload, pos + 36).unwrap_or(4);
    let direction = read_u32_le(payload, pos + 40).unwrap_or(0);
    let color_mode = read_u32_le(payload, pos + 44).unwrap_or(0);
    pos += 48;

    let mut colors = Vec::new();
    if payload.len() >= pos + 2 {
        let col_count = read_u16_le(payload, pos).unwrap_or(0) as usize;
        pos += 2;
        for _ in 0..col_count {
            if pos + 4 <= payload.len() {
                colors.push(RgbColor::new(payload[pos], payload[pos + 1], payload[pos + 2]));
                pos += 4;
            }
        }
    }

    Ok(OpenRgbModeUpdate {
        mode_idx,
        name,
        value,
        flags,
        speed,
        brightness,
        direction,
        color_mode,
        colors,
    })
}

/// Resolves the effective `LightingMode` from an OpenRGB mode index, mode value, and profile/mode name.
/// When OpenRGB applies an Active Profile from a `.orp` file, `mode_val` is often 0 or omitted in JSON,
/// while `name` carries the human-readable effect title.
pub fn resolve_openrgb_mode_named(mode_idx: i32, mode_val: i32, name: &str) -> LightingMode {
    let clean = name.trim().to_lowercase();
    if !clean.is_empty() {
        if clean.contains("direct") || clean.contains("custom") || clean.contains("canvas") {
            return LightingMode::CustomMatrixCanvas;
        } else if clean.contains("static") || clean.contains("solid") || clean.contains("constant") {
            return LightingMode::Static;
        } else if clean.contains("breath") {
            return LightingMode::Breathing;
        } else if clean.contains("horizontal") || clean.contains("left to right") || clean.contains("streamlr") {
            return LightingMode::StreamLeftToRight;
        } else if clean.contains("vertical") || clean.contains("up to down") || clean.contains("streamud") {
            return LightingMode::StreamUpToDown;
        } else if clean.contains("spectrum") || clean.contains("rainbow cycle") {
            return LightingMode::Spectrum;
        } else if clean.contains("rotate") || clean.contains("wheel") {
            return LightingMode::Rotate;
        } else if clean.contains("bloom") || clean.contains("pulse") {
            return LightingMode::Bloom;
        } else if clean.contains("shuttle") || clean.contains("back and forth") {
            return LightingMode::Shuttle;
        } else if clean.contains("rain") {
            return LightingMode::Raindrop;
        } else if clean.contains("meteor") || clean.contains("star") {
            return LightingMode::Meteor;
        } else if clean.contains("ripple fade") || clean.contains("ripple off") {
            return LightingMode::RippleFade;
        } else if clean.contains("ripple static") || clean.contains("ripple on") {
            return LightingMode::RippleStatic;
        } else if clean.contains("reactive") || clean.contains("glow") {
            return LightingMode::Reactive;
        } else if clean.contains("grid") || clean.contains("crosshair") {
            return LightingMode::GridRipple;
        } else if clean.contains("tide") || clean.contains("ocean") {
            return LightingMode::Tide;
        } else if clean == "off" || clean.contains("light off") {
            return LightingMode::Off;
        }
    }

    resolve_openrgb_mode(mode_idx, mode_val)
}

/// Resolves the effective `LightingMode` from an OpenRGB mode index and mode value.
/// When OpenRGB applies an Active Profile, `mode_val` is typically 0 (not serialized in profile JSON),
/// so the mode must be resolved using `mode_idx`.
pub fn resolve_openrgb_mode(mode_idx: i32, mode_val: i32) -> LightingMode {
    if mode_val != 0 {
        if let Ok(m) = LightingMode::from_u8(mode_val as u8) {
            return m;
        }
    }
    match mode_idx {
        0 => LightingMode::CustomMatrixCanvas,
        1 => LightingMode::Static,
        2 => LightingMode::Breathing,
        3 => LightingMode::StreamLeftToRight,
        4 => LightingMode::StreamUpToDown,
        5 => LightingMode::Spectrum,
        6 => LightingMode::Rotate,
        7 => LightingMode::Bloom,
        8 => LightingMode::Shuttle,
        9 => LightingMode::Raindrop,
        10 => LightingMode::Meteor,
        11 => LightingMode::RippleFade,
        12 => LightingMode::RippleStatic,
        13 => LightingMode::Reactive,
        14 => LightingMode::GridRipple,
        15 => LightingMode::Tide,
        16 => LightingMode::Off,
        _ => LightingMode::CustomMatrixCanvas,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_openrgb_mode_explicit_value() {
        assert_eq!(resolve_openrgb_mode(7, 6), LightingMode::Bloom);
        assert_eq!(resolve_openrgb_mode(0, 16), LightingMode::CustomMatrixCanvas);
        assert_eq!(resolve_openrgb_mode(1, 1), LightingMode::Static);
    }

    #[test]
    fn test_resolve_openrgb_mode_zero_value_profile_fallback() {
        // Active Profile from OpenRGB JSON has value = 0 and specifies mode_idx
        assert_eq!(resolve_openrgb_mode(0, 0), LightingMode::CustomMatrixCanvas);
        assert_eq!(resolve_openrgb_mode(1, 0), LightingMode::Static);
        assert_eq!(resolve_openrgb_mode(2, 0), LightingMode::Breathing);
        assert_eq!(resolve_openrgb_mode(3, 0), LightingMode::StreamLeftToRight);
        assert_eq!(resolve_openrgb_mode(4, 0), LightingMode::StreamUpToDown);
        assert_eq!(resolve_openrgb_mode(5, 0), LightingMode::Spectrum);
        assert_eq!(resolve_openrgb_mode(6, 0), LightingMode::Rotate);
        assert_eq!(resolve_openrgb_mode(7, 0), LightingMode::Bloom);
        assert_eq!(resolve_openrgb_mode(8, 0), LightingMode::Shuttle);
        assert_eq!(resolve_openrgb_mode(9, 0), LightingMode::Raindrop);
        assert_eq!(resolve_openrgb_mode(10, 0), LightingMode::Meteor);
        assert_eq!(resolve_openrgb_mode(11, 0), LightingMode::RippleFade);
        assert_eq!(resolve_openrgb_mode(12, 0), LightingMode::RippleStatic);
        assert_eq!(resolve_openrgb_mode(13, 0), LightingMode::Reactive);
        assert_eq!(resolve_openrgb_mode(14, 0), LightingMode::GridRipple);
        assert_eq!(resolve_openrgb_mode(15, 0), LightingMode::Tide);
        assert_eq!(resolve_openrgb_mode(16, 0), LightingMode::Off);
        // Invalid index defaults to CustomMatrixCanvas
        assert_eq!(resolve_openrgb_mode(99, 0), LightingMode::CustomMatrixCanvas);
    }

    #[test]
    fn test_resolve_openrgb_mode_named() {
        assert_eq!(resolve_openrgb_mode_named(99, 0, "Rainbow Wave (Horizontal)"), LightingMode::StreamLeftToRight);
        assert_eq!(resolve_openrgb_mode_named(99, 0, "Rainbow Wave (Vertical)"), LightingMode::StreamUpToDown);
        assert_eq!(resolve_openrgb_mode_named(99, 0, "Static (Constant)"), LightingMode::Static);
        assert_eq!(resolve_openrgb_mode_named(99, 0, "Breathing"), LightingMode::Breathing);
        assert_eq!(resolve_openrgb_mode_named(99, 0, "Direct (Per-Key Custom)"), LightingMode::CustomMatrixCanvas);
        assert_eq!(resolve_openrgb_mode_named(99, 0, "Off"), LightingMode::Off);
        // Fallback to mode_idx when name is unknown
        assert_eq!(resolve_openrgb_mode_named(1, 0, ""), LightingMode::Static);
    }

    #[test]
    fn test_openrgb_payload_parsers_malformed_input() {
        // Truncated payloads must return Err(InvalidLength), NEVER panic
        assert!(parse_openrgb_update_leds(&[]).is_err());
        assert!(parse_openrgb_update_leds(&[0; 5]).is_err());
        assert!(parse_openrgb_update_leds(&[0, 0, 0, 0, 10, 0]).is_err()); // claims 10 colors, 0 provided

        assert!(parse_openrgb_update_zone_leds(&[]).is_err());
        assert!(parse_openrgb_update_zone_leds(&[0; 9]).is_err());
        assert!(parse_openrgb_update_zone_leds(&[0, 0, 0, 0, 0, 0, 0, 0, 5, 0]).is_err()); // claims 5 colors

        assert!(parse_openrgb_update_single_led(&[]).is_err());
        assert!(parse_openrgb_update_single_led(&[0; 6]).is_err());

        assert!(parse_openrgb_set_mode(&[]).is_err());
        assert!(parse_openrgb_set_mode(&[0; 7]).is_err());
    }
}
