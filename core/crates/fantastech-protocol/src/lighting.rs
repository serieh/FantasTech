//! Lighting control, animation modes, and RGB matrix canvas serialization.

use crate::constants::{opcodes, MAX_KEY_INDEX, SPARSE_KEY_COUNT, MATRIX_RGB_BYTE_COUNT, PROTOCOL_MAGIC};
use crate::error::ProtocolError;
use crate::report::ReportBuffer;

/// Target lighting zone on the keyboard hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum LightingZone {
    /// Main 104-key per-key backlight matrix (hardware identifier = 1).
    MainBacklight = 0x01,
    /// Edge accent lighting strips along the chassis (hardware identifier = 2).
    SideStrips = 0x02,
}

/// The 17 supported hardware lighting animation presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum LightingMode {
    Off = 0,
    Static = 1,
    Breathing = 2,
    Rotate = 3,
    Spectrum = 4,
    StreamLeftToRight = 5,
    Bloom = 6,
    StreamUpToDown = 7,
    Shuttle = 8,
    Raindrop = 9,
    Meteor = 10,
    RippleFade = 11,
    RippleStatic = 12,
    Reactive = 13,
    GridRipple = 14,
    Tide = 15,
    CustomMatrixCanvas = 16,
}

impl LightingMode {
    pub fn from_u8(val: u8) -> Result<Self, ProtocolError> {
        match val {
            0 => Ok(Self::Off),
            1 => Ok(Self::Static),
            2 => Ok(Self::Breathing),
            3 => Ok(Self::Rotate),
            4 => Ok(Self::Spectrum),
            5 => Ok(Self::StreamLeftToRight),
            6 => Ok(Self::Bloom),
            7 => Ok(Self::StreamUpToDown),
            8 => Ok(Self::Shuttle),
            9 => Ok(Self::Raindrop),
            10 => Ok(Self::Meteor),
            11 => Ok(Self::RippleFade),
            12 => Ok(Self::RippleStatic),
            13 => Ok(Self::Reactive),
            14 => Ok(Self::GridRipple),
            15 => Ok(Self::Tide),
            16 => Ok(Self::CustomMatrixCanvas),
            _ => Err(ProtocolError::ParameterOutOfRange {
                field: "lighting_mode",
                value: val as u32,
                min: 0,
                max: 16,
            }),
        }
    }

    /// Returns a list of all 17 supported hardware lighting modes.
    pub fn all() -> &'static [LightingMode] {
        &[
            Self::Off,
            Self::Static,
            Self::Breathing,
            Self::Rotate,
            Self::Spectrum,
            Self::StreamLeftToRight,
            Self::Bloom,
            Self::StreamUpToDown,
            Self::Shuttle,
            Self::Raindrop,
            Self::Meteor,
            Self::RippleFade,
            Self::RippleStatic,
            Self::Reactive,
            Self::GridRipple,
            Self::Tide,
            Self::CustomMatrixCanvas,
        ]
    }

    /// User-friendly name and description of the mode.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Static => "Static (Constant Solid)",
            Self::Breathing => "Breathing",
            Self::Rotate => "Rotate (Color Cycle)",
            Self::Spectrum => "Spectrum (Rainbow Cycle)",
            Self::StreamLeftToRight => "Stream (Wave Left to Right)",
            Self::Bloom => "Bloom (Radial Pulse)",
            Self::StreamUpToDown => "Stream (Cascade Up to Down)",
            Self::Shuttle => "Shuttle (Back and Forth)",
            Self::Raindrop => "Raindrop",
            Self::Meteor => "Meteor",
            Self::RippleFade => "Ripple Fade",
            Self::RippleStatic => "Ripple Static",
            Self::Reactive => "Reactive (Key Press Glow)",
            Self::GridRipple => "Grid Ripple (Crosshair)",
            Self::Tide => "Tide (Ocean Waves)",
            Self::CustomMatrixCanvas => "Custom (Per-Key Canvas)",
        }
    }

    /// Whether this mode supports custom color assignment.
    pub fn supports_color(&self) -> bool {
        matches!(
            self,
            Self::Static
                | Self::Breathing
                | Self::StreamLeftToRight
                | Self::Bloom
                | Self::StreamUpToDown
                | Self::Shuttle
                | Self::Raindrop
                | Self::RippleFade
                | Self::RippleStatic
                | Self::Reactive
                | Self::GridRipple
                | Self::Tide
        )
    }

    /// Whether this mode supports speed adjustment.
    pub fn supports_speed(&self) -> bool {
        !matches!(self, Self::Off | Self::Static | Self::CustomMatrixCanvas)
    }

    /// Whether this mode supports direction controls.
    /// Mode 5 (StreamLeftToRight) and Mode 7 (StreamUpToDown) support direction flipping in hardware.
    pub fn supports_direction(&self) -> bool {
        matches!(self, Self::StreamLeftToRight | Self::StreamUpToDown)
    }
}

impl std::str::FromStr for LightingMode {
    type Err = ProtocolError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let clean = s.trim().to_lowercase().replace(['-', '_', ' '], "");
        match clean.as_str() {
            "0" | "off" => Ok(Self::Off),
            "1" | "static" | "solid" | "constant" => Ok(Self::Static),
            "2" | "breathing" | "breath" => Ok(Self::Breathing),
            "3" | "rotate" | "wheel" => Ok(Self::Rotate),
            "4" | "spectrum" | "rainbow" => Ok(Self::Spectrum),
            "5" | "stream" | "streamlefttoright" | "wave" | "streamlr" => Ok(Self::StreamLeftToRight),
            "6" | "bloom" | "flower" => Ok(Self::Bloom),
            "7" | "streamuptodown" | "verticalstream" | "cascade" | "streamud" => Ok(Self::StreamUpToDown),
            "8" | "shuttle" | "travel" => Ok(Self::Shuttle),
            "9" | "raindrop" | "rain" => Ok(Self::Raindrop),
            "10" | "meteor" | "stars" => Ok(Self::Meteor),
            "11" | "ripplefade" | "rippleoff" => Ok(Self::RippleFade),
            "12" | "ripplestatic" | "rippleon" => Ok(Self::RippleStatic),
            "13" | "reactive" | "trigger" | "touch" => Ok(Self::Reactive),
            "14" | "gridripple" | "grid" | "crosshair" => Ok(Self::GridRipple),
            "15" | "tide" => Ok(Self::Tide),
            "16" | "custom" | "matrix" | "canvas" | "user" => Ok(Self::CustomMatrixCanvas),
            _ => {
                if let Ok(val) = s.parse::<u8>() {
                    Self::from_u8(val)
                } else {
                    Err(ProtocolError::ParameterOutOfRange {
                        field: "lighting_mode",
                        value: 255,
                        min: 0,
                        max: 16,
                    })
                }
            }
        }
    }
}

/// Animation direction for directional presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum AnimationDirection {
    Default = 0,
    LeftToRight = 1,
}

impl std::str::FromStr for AnimationDirection {
    type Err = ProtocolError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "0" | "default" | "normal" | "left" | "up" => Ok(Self::Default),
            "1" | "reverse" | "right" | "down" | "lefttoright" | "lr" | "ud" => Ok(Self::LeftToRight),
            _ => Ok(Self::Default),
        }
    }
}

/// Color mode: dynamic rainbow/spectrum vs. fixed single color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum ColorMode {
    DynamicSpectrum = 0,
    FixedColor = 1,
}

/// A 24-bit RGB color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parses a hex color string like "#FF00AA" or "FF00AA" or "F0A".
    pub fn from_hex(hex: &str) -> Option<Self> {
        let clean = hex.trim().trim_start_matches('#');
        if clean.len() == 6 {
            let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
            let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
            let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
            Some(Self::new(r, g, b))
        } else if clean.len() == 3 {
            let r = u8::from_str_radix(&clean[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&clean[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&clean[2..3], 16).ok()? * 17;
            Some(Self::new(r, g, b))
        } else {
            None
        }
    }
}

/// Converts standard 8-bit RGB (0-255) to 8-bit HSV (0-255) as expected by MK894 firmware.
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let r_norm = r as f32 / 255.0;
    let g_norm = g as f32 / 255.0;
    let b_norm = b as f32 / 255.0;

    let max = r_norm.max(g_norm).max(b_norm);
    let min = r_norm.min(g_norm).min(b_norm);
    let delta = max - min;

    let mut h = if delta == 0.0 {
        0.0
    } else if (max - r_norm).abs() < f32::EPSILON {
        60.0 * (((g_norm - b_norm) / delta) % 6.0)
    } else if (max - g_norm).abs() < f32::EPSILON {
        60.0 * (((b_norm - r_norm) / delta) + 2.0)
    } else {
        60.0 * (((r_norm - g_norm) / delta) + 4.0)
    };

    if h < 0.0 {
        h += 360.0;
    }

    let s = if max == 0.0 { 0.0 } else { delta / max };
    let v = max;

    (
        ((h / 360.0) * 255.0).round() as u8,
        (s * 255.0).round() as u8,
        (v * 255.0).round() as u8,
    )
}

/// Converts 8-bit HSV (0-255) back to standard 8-bit RGB (0-255).
pub fn hsv_to_rgb(h: u8, s: u8, v: u8) -> RgbColor {
    let h_deg = (h as f32 / 255.0) * 360.0;
    let s_norm = s as f32 / 255.0;
    let v_norm = v as f32 / 255.0;
    let c = v_norm * s_norm;
    let h_prime = h_deg / 60.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
    let m = v_norm - c;
    let sector = (h_prime as u32) % 6;
    let (r_p, g_p, b_p) = match sector {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    RgbColor {
        r: ((r_p + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        g: ((g_p + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        b: ((b_p + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    }
}

/// Full configuration parameters for global lighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LightingConfig {
    pub zone: LightingZone,
    pub mode: LightingMode,
    pub brightness: u8,
    pub speed: u8,
    pub direction: AnimationDirection,
    pub color_mode: ColorMode,
    pub color: RgbColor,
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            zone: LightingZone::MainBacklight,
            mode: LightingMode::Spectrum,
            brightness: 4,
            speed: 2,
            direction: AnimationDirection::Default,
            color_mode: ColorMode::DynamicSpectrum,
            color: RgbColor::new(255, 0, 0),
        }
    }
}

/// Builds a 64-byte USB Output Report for setting global lighting (Opcode 0x0B).
pub fn build_lighting_report(config: &LightingConfig) -> Result<ReportBuffer, ProtocolError> {
    if config.brightness > 100 {
        return Err(ProtocolError::ParameterOutOfRange {
            field: "brightness",
            value: config.brightness as u32,
            min: 0,
            max: 100,
        });
    }
    if config.speed > 100 {
        return Err(ProtocolError::ParameterOutOfRange {
            field: "speed",
            value: config.speed as u32,
            min: 0,
            max: 100,
        });
    }

    // Hardware strictly expects discrete levels 0..4 at byte 8 (brightness) and byte 9 (speed).
    // Values exceeding 4 are rejected by the keyboard MCU (EV-031).
    // If discrete levels 0..=4 are passed, pass them directly. If a 0..100 percentage is passed,
    // map cleanly to the 0..=4 hardware scale.
    let hw_brightness = if config.brightness <= 4 {
        config.brightness
    } else {
        ((config.brightness as f32 / 100.0) * 4.0).round().clamp(0.0, 4.0) as u8
    };

    let hw_speed = if config.speed <= 4 {
        config.speed
    } else {
        ((config.speed as f32 / 100.0) * 4.0).round().clamp(0.0, 4.0) as u8
    };

    let (h, s, v) = if config.mode.supports_color() {
        rgb_to_hsv(config.color.r, config.color.g, config.color.b)
    } else {
        (0, 0, 0)
    };

    let (effective_mode, color_mode) = if config.mode == LightingMode::Static
        && config.color.r == 0
        && config.color.g == 0
        && config.color.b == 0
    {
        (LightingMode::Off as u8, 0)
    } else {
        match config.mode {
            LightingMode::Off => (0, 0),
            LightingMode::Static => (1, 1),
            mode if !mode.supports_color() => (mode as u8, 0),
            _ => (config.mode as u8, config.color_mode as u8),
        }
    };

    let effective_direction = if config.mode.supports_direction() {
        config.direction as u8
    } else {
        0
    };

    let payload_content = [
        config.zone as u8,
        0x00,
        effective_mode,
        hw_brightness,
        hw_speed,
        effective_direction,
        color_mode,
        0x00,
        h,
        s,
        v,
    ];

    let mut report = ReportBuffer::new(opcodes::SET_LIGHT_CONFIG);
    report.set_byte(2, payload_content.len() as u8)?; // 11 bytes length
    report.set_byte(3, 0x00)?;
    report.set_byte(4, 0x00)?;
    report.write_payload(5, &payload_content)?;
    Ok(report)
}

/// Builds the 64-byte USB Output Report to query current lighting configuration (Opcode 0x0A).
pub fn build_light_query_report() -> Result<ReportBuffer, ProtocolError> {
    Ok(ReportBuffer::new(opcodes::GET_LIGHT_CONFIG))
}

/// Parses a 64-byte response from the keyboard to a lighting query.
pub fn parse_light_config_response(buf: &ReportBuffer) -> Result<LightingConfig, ProtocolError> {
    let raw = buf.as_bytes();
    if (raw[0] != PROTOCOL_MAGIC && raw[0] != 0xAA) || raw[1] != opcodes::GET_LIGHT_CONFIG {
        return Err(ProtocolError::MalformedResponse {
            opcode: raw[1],
            detail: "Expected Magic 0x06/0xAA and Opcode 0x0A for lighting query response",
        });
    }

    let zone = match raw[5] {
        2 => LightingZone::SideStrips,
        _ => LightingZone::MainBacklight,
    };
    let mode = LightingMode::from_u8(raw[7])?;
    let brightness = raw[8];
    let speed = raw[9];
    let direction = match raw[10] {
        1 => AnimationDirection::LeftToRight,
        _ => AnimationDirection::Default,
    };
    let color_mode = if raw[11] == 1 {
        ColorMode::FixedColor
    } else {
        ColorMode::DynamicSpectrum
    };

    let h = raw[13];
    let s = raw[14];
    let v = raw[15];
    let color = hsv_to_rgb(h, s, v);

    Ok(LightingConfig {
        zone,
        mode,
        brightness,
        speed,
        direction,
        color_mode,
        color,
    })
}

/// Builds the 64-byte USB Output Report to query the hardware default configuration for an effect (Opcode 0x16).
pub fn build_effect_default_query_report(mode: u8) -> Result<ReportBuffer, ProtocolError> {
    if mode > 16 {
        return Err(ProtocolError::ParameterOutOfRange {
            field: "lighting_mode",
            value: mode as u32,
            min: 0,
            max: 16,
        });
    }
    let mut report = ReportBuffer::new(opcodes::GET_EFFECT_DEFAULT_CONFIG);
    report.set_byte(2, 0x03)?; // Length of query payload
    report.set_byte(5, LightingZone::MainBacklight as u8)?;
    report.set_byte(7, mode)?;
    Ok(report)
}

/// Parses the 64-byte response from the keyboard to an effect default query (Opcode 0x16).
pub fn parse_effect_default_response(buf: &ReportBuffer) -> Result<LightingConfig, ProtocolError> {
    let raw = buf.as_bytes();
    if (raw[0] != PROTOCOL_MAGIC && raw[0] != 0xAA) || raw[1] != opcodes::GET_EFFECT_DEFAULT_CONFIG {
        return Err(ProtocolError::MalformedResponse {
            opcode: raw[1],
            detail: "Expected Magic 0x06/0xAA and Opcode 0x16 for effect default query response",
        });
    }

    let zone = match raw[5] {
        2 => LightingZone::SideStrips,
        _ => LightingZone::MainBacklight,
    };
    let mode = LightingMode::from_u8(raw[7])?;
    let brightness = raw[8];
    let speed = raw[9];
    let direction = match raw[10] {
        1 => AnimationDirection::LeftToRight,
        _ => AnimationDirection::Default,
    };
    let color_mode = if raw[11] == 1 {
        ColorMode::FixedColor
    } else {
        ColorMode::DynamicSpectrum
    };

    let h = raw[13];
    let s = raw[14];
    let v = raw[15];
    let color = hsv_to_rgb(h, s, v);

    Ok(LightingConfig {
        zone,
        mode,
        brightness,
        speed,
        direction,
        color_mode,
        color,
    })
}

/// Parses an asynchronous hardware event report (Magic 0xAA, Sub-header 0xFA).
/// When physical Fn shortcuts (`Fn + \`, `Fn + Up/Down`, `Fn + Left/Right`) are pressed on the keyboard,
/// the MCU emits this 64-byte report autonomously to keep the host synchronized.
pub fn parse_async_lighting_notification(raw: &[u8]) -> Result<Option<LightingConfig>, ProtocolError> {
    if raw.len() < 16 {
        return Ok(None);
    }
    if raw[0] != 0xAA || raw[1] != 0xFA {
        return Ok(None);
    }

    let zone = match raw[5] {
        2 => LightingZone::SideStrips,
        _ => LightingZone::MainBacklight,
    };
    let mode = match LightingMode::from_u8(raw[7]) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    let brightness = raw[8];
    let speed = raw[9];
    let direction = match raw[10] {
        1 => AnimationDirection::LeftToRight,
        _ => AnimationDirection::Default,
    };
    let color_mode = if raw[11] == 1 {
        ColorMode::FixedColor
    } else {
        ColorMode::DynamicSpectrum
    };

    let h = raw[13];
    let s = raw[14];
    let v = raw[15];
    let color = hsv_to_rgb(h, s, v);

    Ok(Some(LightingConfig {
        zone,
        mode,
        brightness,
        speed,
        direction,
        color_mode,
        color,
    }))
}

/// Builds a 64-byte USB Output Report for setting the static color of an individual key (Opcode 0x14).
pub fn build_single_key_color(key_index: u8, color: RgbColor) -> Result<ReportBuffer, ProtocolError> {
    if (key_index as usize) > MAX_KEY_INDEX {
        return Err(ProtocolError::KeyIndexOutOfBounds(key_index as usize));
    }

    let offset = (key_index as u16) * 3;
    let offset_lo = (offset & 0xFF) as u8;
    let offset_hi = ((offset >> 8) & 0xFF) as u8;

    let mut report = ReportBuffer::new(opcodes::SET_SINGLE_KEY_LED);
    report.set_byte(2, 0x03)?; // Length: 3 RGB bytes
    report.set_byte(3, offset_lo)?;
    report.set_byte(4, offset_hi)?;
    report.set_byte(8, color.r)?;
    report.set_byte(9, color.g)?;
    report.set_byte(10, color.b)?;
    Ok(report)
}

/// Builds the 7 sequential 64-byte USB Output Reports required to stream a complete 378-byte per-key RGB canvas (Opcode 0x12).
/// Uses 54-byte chunks (18 keys * 3 bytes) so that no RGB triplets are severed across packet boundaries.
pub fn build_matrix_packets(matrix: &[RgbColor; SPARSE_KEY_COUNT]) -> Result<Vec<ReportBuffer>, ProtocolError> {
    let mut raw_colors = [0u8; MATRIX_RGB_BYTE_COUNT];
    for (i, key) in matrix.iter().enumerate() {
        raw_colors[i * 3] = key.r;
        raw_colors[i * 3 + 1] = key.g;
        raw_colors[i * 3 + 2] = key.b;
    }

    const CHUNK_SIZE: usize = 54;
    let total_bytes = raw_colors.len();
    let num_chunks = total_bytes.div_ceil(CHUNK_SIZE);
    let mut packets = Vec::with_capacity(num_chunks);

    for c in 0..num_chunks {
        let offset = c * CHUNK_SIZE;
        let chunk_len = CHUNK_SIZE.min(total_bytes - offset);
        let header_len = (chunk_len + 3) as u8;

        let mut pkt = ReportBuffer::new(opcodes::STREAM_RGB_MATRIX);
        pkt.set_byte(2, header_len)?;
        pkt.set_byte(3, (offset & 0xFF) as u8)?;
        pkt.set_byte(4, ((offset >> 8) & 0xFF) as u8)?;
        pkt.write_payload(8, &raw_colors[offset..offset + chunk_len])?;
        packets.push(pkt);
    }

    Ok(packets)
}
