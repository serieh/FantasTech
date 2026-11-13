# Fantech MK894 Maxpower II USB HID Protocol Specification

**Document Version:** 1.0.0  
**Author:** Reverse-Engineering Research Team  
**Date:** 2026-10-26  
**Target Hardware:** Fantech MK894 Maxpower II Mechanical Keyboard (and compatible Artery Tek / SinoWealth OEM platforms)  
**Research Classification:** Clean-Room Protocol Interoperability Specification  

---

## 1. Executive Summary & Hardware Architecture

The **Fantech MK894 Maxpower II** is a full-sized (104/107-key ANSI) mechanical gaming keyboard featuring per-key addressable RGB backlighting, onboard profile storage, hardware macro recording, and dynamic lighting effects. 

Static reverse-engineering of the official vendor companion software (`MK894 Setup 1.0.1.exe` / Electron bundle) reveals that the device is built on an **Artery Tek AT32** ARM Cortex-M microcontroller platform (OEM platform `AK107` / `AK110-DS`). Unlike peripherals requiring proprietary kernel filter drivers, the companion software interfaces with the keyboard via standard OS-level USB Human Interface Device (HID) APIs using the W3C WebHID interface.

This specification documents the complete host-to-device binary communication protocol, enabling the development of independent, cross-platform, open-source drivers (e.g., Linux CLI utilities, OpenRGB controllers, QMK/VIA bridges).

---

## 2. USB Transport & Device Identification

### 2.1 Identifiers

| Parameter | Hexadecimal | Decimal | Description |
| :--- | :--- | :--- | :--- |
| **Vendor ID (VID)** | `0x36AE` | 13998 | OEM Vendor Identifier |
| **Product ID (PID)** | `0xFE9C` | 65180 | Fantech MK894 (Profile `AK107` - 104/107 Key ANSI RGB) |
| **Secondary PID** | `0xFEAB` | 65195 | Variant Model (Profile `AK110-DS`) |

### 2.2 USB Interfaces & HID Descriptors

The keyboard enumerates as a USB Composite Device providing multiple HID collections:
* **Interface 0 (MI_00)**: Standard HID Keyboard (Usage Page `0x01` Generic Desktop, Usage `0x06` Keyboard) - Delivers standard 8-byte boot keyboard scancodes.
* **Interface 1 (MI_01)**: Consumer / Media Keys (Usage Page `0x0C` Consumer, Usage `0x01` Consumer Control) - Volume, mute, media playback.
* **Interface 2 (MI_02)**: **Vendor Control Interface** (Usage Page `0xFF00` Vendor-Defined, Usage `0x0002` Main Control) - Used for configuration, lighting, macros, and remapping.

### 2.3 Wire Transport Framing

All configuration commands are transmitted over the Vendor Control Interface using **65-byte USB HID Output Reports** (or Feature Reports depending on OS driver binding).

```
+------------+------------+--------------------+--------------------------------+
| Byte 0     | Byte 1     | Bytes 2..63        | Byte 64                        |
+------------+------------+--------------------+--------------------------------+
| Report ID  | Opcode     | Command Parameters | Zero Padding                   |
| (0x06)     | (1 byte)   | & Data Payload     | (Padded to exactly 65 bytes)   |
+------------+------------+--------------------+--------------------------------+
```

* **Buffer Length:** Strictly **65 bytes**.
* **Report ID (Byte 0):** Always `0x06`.  
  *(Note for WebHID developers: When using `device.sendReport(reportId, data)`, pass `reportId = 6` and provide a 64-byte payload for bytes 1..64).*
* **Opcode (Byte 1):** The specific hardware command identifier.
* **Checksum:** Fixed `0x00`. No software CRC or XOR checksum byte is required in the application payload; data integrity is maintained at the USB transport layer (USB token and packet CRC16).

---

## 3. Command Opcode Reference Table

| Opcode (Hex) | Opcode (Dec) | Direction | Command Name | Description |
| :--- | :--- | :--- | :--- | :--- |
| `0x05` | 5 | Host <- Device | `GET_KEYBOARD_CONFIG` | Queries hardware model, firmware version, profile count, and serial number. |
| `0x06` | 6 | Host <- Device | `GET_KEY_LAYOUT_BYTES`| Reads raw key matrix layout descriptor chunk. |
| `0x08` | 8 | Host <- Device | `GET_KEY_INFOS` | Reads 4-byte key remap definitions (up to 56 bytes per chunk). |
| `0x09` | 9 | Host -> Device | `SET_KEY_INFOS` | Writes 4-byte key remap definitions. |
| `0x0A` | 10 | Host <- Device | `GET_LIGHT_CONFIG` | Reads current global lighting settings. |
| `0x0B` | 11 | Host -> Device | `SET_LIGHT_CONFIG` | Writes global lighting settings (mode, speed, brightness, HSV). |
| `0x0C` | 12 | Host <- Device | `GET_MACRO_DATA` | Reads 4096-byte onboard macro memory in 56-byte chunks. |
| `0x0D` | 13 | Host -> Device | `SET_MACRO_DATA` | Writes macro sequence memory in 56-byte chunks. |
| `0x0F` | 15 | Host -> Device | `RESTORE_FACTORY` | Triggers a full non-volatile factory reset. |
| `0x12` | 18 | Host -> Device | `SET_KEY_COLORS_INFOS`| Streams per-key RGB matrix colors in 56-byte chunks. |
| `0x14` | 20 | Host -> Device | `SET_KEY_COLOR_INFO` | Sets the static RGB color of a single specified key. |
| `0xFB` | 251 | Host -> Device | `SET_CURRENT_PROFILE`| Switches the active hardware profile index. |
| `0xFC` | 252 | Host -> Device | `SET_AUTO_SLEEP_TIME`| Sets inactivity auto-sleep timeout (wireless models). |
| `0x5A` | 90 | Host -> Device | `GO_TO_IAP` | **BANNED**: Jumps MCU to ROM bootloader for firmware flashing. |

---

## 4. Detailed Command Packet Structures

### 4.1 Global Lighting Configuration (`Opcode 0x0B`)

Configures global backlighting modes, brightness, animation speed, direction, and color palette.

#### Packet Format:
| Byte Offset | Field Name | Type | Value / Range | Description |
| :--- | :--- | :--- | :--- | :--- |
| `0x00` | Report ID | uint8 | `0x06` | Always Report ID 6 |
| `0x01` | Opcode | uint8 | `0x0B` | Command `SET_LIGHT_CONFIG` |
| `0x02` | Sub-length | uint8 | `0x0B` | 11 bytes of configuration parameters |
| `0x03` | Reserved | uint8 | `0x00` | Reserved |
| `0x04` | Reserved | uint8 | `0x00` | Reserved |
| `0x05` | Light Zone | uint8 | `0x00` - `0x01` | `0` = Main Key Backlight; `1` = Side Accent Strips |
| `0x06` | Reserved | uint8 | `0x00` | Padding |
| `0x07` | Mode | uint8 | `0` - `16` | Animation mode preset (see table below) |
| `0x08` | Brightness | uint8 | `0` - `100` | LED intensity percentage (0 = Off, 100 = Max) |
| `0x09` | Speed | uint8 | `0` - `100` | Animation velocity percentage |
| `0x0A` | Direction | uint8 | `0` - `2` | `0` = Normal, `1` = Left-to-Right, `2` = Up-to-Down |
| `0x0B` | Color Mode | uint8 | `0` - `1` | `0` = Multicolor/Spectrum, `1` = Custom Fixed Color |
| `0x0C` | Reserved | uint8 | `0x00` | Padding |
| `0x0D` | Hue (H) | uint8 | `0` - `255` | Scaled Hue (Maps: `0..360°` -> `0..255`) |
| `0x0E` | Saturation (S)| uint8 | `0` - `255` | Scaled Saturation (Maps: `0..100%` -> `0..255`) |
| `0x0F` | Value (V) | uint8 | `0` - `255` | Scaled Value/Lightness (Maps: `0..100%` -> `0..255`) |
| `0x10..0x40` | Padding | uint8[49] | `0x00` | Zero-padded to 65 bytes |

#### Lighting Modes Reference:
| Mode ID | Mode Name | Description |
| :--- | :--- | :--- |
| `0` | Off | Disables all LEDs in selected zone. |
| `1` | Static | Fixed single color across all keys. |
| `2` | Breathing | Smooth pulse between dim and full intensity. |
| `3` | Rotate / Cycle | Smooth color wheel transition across all keys. |
| `4` | Spectrum | Rainbow color wave sweeping across the board. |
| `5` | Stream (LR) | Horizontal left-to-right light cascade. |
| `6` | Blossom / Bloom | Radially expanding color waves from center outwards. |
| `7` | Stream (UD) | Vertical top-to-bottom light cascade. |
| `8` | Shuttle | Moving light bar bouncing horizontally. |
| `9` | Raindrops | Random key clusters illuminated like raindrops. |
| `10` | Meteor | Diagonal trail effect across matrix. |
| `11` | Ripple Fade | Pressing a key sends expanding wave that fades to black. |
| `12` | Ripple Static | Pressing a key sends expanding wave over background color. |
| `13` | Reactive | Individual keys illuminate when pressed and fade out. |
| `14` | Grid / Matrix | 3x3 surrounding keys illuminate upon actuation. |
| `15` | Tide | Sinusoidal alternating wave across keys. |
| `16` | Custom RGB | Per-key matrix color mode (reads from per-key memory). |

---

### 4.2 Set Single Key RGB Color (`Opcode 0x14`)

Updates the 24-bit RGB color of an individual key switch.

#### Packet Format:
| Byte Offset | Field Name | Type | Value / Range | Description |
| :--- | :--- | :--- | :--- | :--- |
| `0x00` | Report ID | uint8 | `0x06` | Always Report ID 6 |
| `0x01` | Opcode | uint8 | `0x14` | Command `SET_KEY_COLOR_INFO` |
| `0x02` | Sub-length | uint8 | `0x03` | 3 color payload bytes (R, G, B) |
| `0x03` | Offset Low | uint8 | `(index * 3) & 0xFF` | Low byte of byte offset into matrix memory |
| `0x04` | Offset High | uint8 | `((index * 3) >> 8) & 0xFF` | High byte of byte offset into matrix memory |
| `0x05..0x07` | Reserved | uint8[3] | `0x00` | Alignment padding |
| `0x08` | Red | uint8 | `0x00` - `0xFF` | 8-bit Red component (0..255) |
| `0x09` | Green | uint8 | `0x00` - `0xFF` | 8-bit Green component (0..255) |
| `0x0A` | Blue | uint8 | `0x00` - `0xFF` | 8-bit Blue component (0..255) |
| `0x0B..0x40` | Padding | uint8[54] | `0x00` | Zero-padded to 65 bytes |

---

### 4.3 Batch Per-Key RGB Matrix (`Opcode 0x12`)

Streams full-matrix custom RGB patterns across multiple consecutive 65-byte packets. Each packet transfers up to 56 contiguous RGB bytes (`18` keys + `2` channel bytes per chunk).

#### Packet Format:
| Byte Offset | Field Name | Type | Description |
| :--- | :--- | :--- | :--- |
| `0x00` | Report ID | uint8 | Always `0x06` |
| `0x01` | Opcode | uint8 | Always `0x12` (`SET_KEY_COLORS_INFOS`) |
| `0x02` | Header Length| uint8 | `len(data) + 3` (Usually `59` for full 56-byte chunks) |
| `0x03` | Offset Low | uint8 | `offset & 0xFF` (Byte offset into matrix memory) |
| `0x04` | Offset High | uint8 | `(offset >> 8) & 0xFF` |
| `0x05..0x07` | Reserved | uint8[3] | `0x00, 0x00, 0x00` |
| `0x08..0x3F` | Color Stream | uint8[56] | Raw interleaved RGB bytes: `[R0, G0, B0, R1, G1, B1, ...]` |
| `0x40` | Padding | uint8 | `0x00` (Trailing pad to 65 bytes) |

*For a 104-key ANSI keyboard (312 total color bytes), exactly 6 sequential packets are transmitted (5 packets of 56 bytes + 1 packet of 32 bytes).*

> [!IMPORTANT]
> **Hardware Implementation Finding (Opcode `0x14` vs `0x12`)**:
> On physical MK894 hardware (PID `0xFE9C`), streaming via Opcode `0x12` is unreferenced in the vendor application dispatch and susceptible to chunk boundary byte-alignment issues. The production-proven mechanism for all custom lighting configurations is sequential **Opcode `0x14` (`SET_SINGLE_KEY_LED`)** reports paced at **15–20ms** per key, followed by a mode 16 (`CustomMatrixCanvas`) commit via Opcode `0x0B`. Complete matrix isolation requires clearing all 126 sparse slots (`0..125`) to prevent ghost illumination on adjacent dual-switch pads (e.g. `/?` at 95/96, `Num 0` at 122/123, `Space` at 110/111).

---

### 4.4 Get Keyboard Configuration (`Opcode 0x05`)

Queries device identity, firmware build, and operational status.

#### Request Packet:
```
[0x06, 0x05, 0x00, 0x00, ... zero-padded to 65 bytes]
```

#### Response Buffer Layout (65 bytes):
| Byte Range | Field Name | Encoding | Description |
| :--- | :--- | :--- | :--- |
| `0x00..0x04` | Transport Headers | uint8[5] | Framing headers |
| `0x05..0x06` | Hardware Version | uint16_le | Hardware revision number |
| `0x07..0x08` | Product ID | uint16_le | Matches USB PID (e.g., `0xFE9C`) |
| `0x09..0x0A` | Firmware Version | uint16_le | Firmware build number |
| `0x0B` | Work Mode | uint8 | `0` = Wired USB, `1` = 2.4 GHz RF, `2` = Bluetooth |
| `0x0C` | Link Status | uint8 | `1` = Connected / Paired |
| `0x0D` | Battery Level | uint8 | Battery percentage (`0..100%`) |
| `0x0E` | Charging State | uint8 | `0` = Discharging, `1` = Charging, `2` = Full |
| `0x0F` | Profile Count | uint8 | Total onboard profiles supported (typically 3) |
| `0x10` | Active Profile | uint8 | Currently active profile index (`0..N-1`) |
| `0x11` | Layer Count | uint8 | Supported keymap layers (e.g. Base, Fn layer) |
| `0x12` | Active Layer | uint8 | Currently selected keymap layer |
| `0x13..0x14` | Sleep Timeout | uint16_le | Inactivity timeout in seconds |
| `0x15..0x2A` | Serial Number | ASCII string | 22-byte ASCII alphanumeric serial string |
| `0x2B..0x40` | Reserved | uint8[22] | Vendor reserved |

---

### 4.5 Key Remapping (`Opcodes 0x09` / `0x10`)

The keyboard supports both single-key and batch remapping across multiple keymap layers (e.g. Layer 0 = Default, Layer 1 = Fn).

#### Single-Key Remap (`Opcode 0x10`):
| Byte Offset | Field Name | Type | Value / Description |
| :--- | :--- | :--- | :--- |
| `0x00` | Report ID | uint8 | `0x06` |
| `0x01` | Opcode | uint8 | `0x10` (`OP_SET_KEY_INFO_SINGLE`) |
| `0x02` | Sub-length | uint8 | `0x07` (7 payload bytes) |
| `0x03` | Offset Low | uint8 | `(key_index * 4) & 0xFF` |
| `0x04` | Offset High | uint8 | `((key_index * 4) >> 8) & 0xFF` |
| `0x05` | Reserved | uint8 | `0x00` |
| `0x06` | Layer Index | uint8 | `0` = Base Layer, `1` = Fn Layer |
| `0x07` | Reserved | uint8 | `0x00` |
| `0x08` | Remap Type | uint8 | `32` = Standard HID key, `7` = Disabled |
| `0x09` | Code 1 | uint8 | Target HID Scancode (e.g. `0x2C` Space) |
| `0x0A` | Code 2 | uint8 | Modifier or parameter byte |
| `0x0B` | Code 3 | uint8 | Extended parameter byte |
| `0x0C..0x40` | Padding | uint8 | Zero-padded to 65 bytes |

* **Remap Types:**
  * `32` (`0x20`) = Standard HID Keyboard Scancode (Code 1 = HID usage).
  * `1` (`0x01`) = Mouse Action (Code 1 = button bitmask: 1=Left, 2=Right, 4=Middle).
  * `2` (`0x02`) = Consumer / Multimedia Usage (Code 1, 2 = 16-bit Consumer code).
  * `4` (`0x04`) = Macro Trigger (Code 1 = Macro ID).
  * `7` (`0x07`) = Disabled / Unbound Key.

---

### 4.6 Hardware Profiles & Safety Controls

#### Set Active Profile (`Opcode 0xFB` / `251`):
Switches the active onboard configuration profile:
```
[0x06, 0xFB, <profile_index>, 0x00, ... zero-padded to 65 bytes]
```

#### Restore Factory Settings (`Opcode 0x0F` / `15`):
Clears onboard EEPROM/Flash emulation and restores factory defaults:
```
[0x06, 0x0F, 0xFF, 0x00, ... zero-padded to 65 bytes]
```

#### DFU Bootloader Jump (`Opcode 0x5A` / `90`) — **CRITICAL SAFETY BANNED**:
```
[0x06, 0x5A, 0xA0, 0x00, ... zero-padded to 65 bytes]
```
> [!CAUTION]
> **DO NOT TRANSMIT OPCODES `0x5A 0xA0`:** This command instructs the Artery Tek MCU to exit normal execution and jump to the internal ROM IAP (In-Application Programming) bootloader for flash reprogramming. Transmitting incomplete or corrupted firmware in this state will brick the device. All compliant drivers MUST block this opcode with a permission check.

---

## 5. Verified Hardware Matrix Mapping (ANSI 104-Key Sparse Layout)

Hardware matrix addressing uses a 6-row by 21-column sparse grid (126 addressable slots, physical indices `0` to `125`), fully reverse-engineered and verified via physical probe:

| Matrix Row | Physical Key Count | Key Index Mapping | Unpopulated / Gap Indices |
| :--- | :--- | :--- | :--- |
| **Row 0** (Function) | 16 keys | `0` = Esc, `1..12` = F1..F12, `13` = PrtSc, `14` = ScrLK, `15` = Pause | `16` (Gap), `17..20` (Rotary knob commands) |
| **Row 1** (Number) | 21 keys | `21` = ~, `22..31` = 1..0, `32` = -, `33` = +, `34` = Backspace, `35..37` = Ins/Home/PgUp, `38..41` = NumLock/Div/Mul/Sub | None |
| **Row 2** (QWERTY) | 21 keys | `42` = Tab, `43..52` = Q..P, `53` = [, `54` = ], `55` = \, `56..58` = Del/End/PgDn, `59..62` = Num7/8/9/Add | None |
| **Row 3** (Home) | 16 keys | `63` = CapsLock, `64..72` = A..L, `73` = ;, `74` = ', `76` = Enter, `80..82` = Num4/5/6 | `75` (ISO Enter pad), `77..79` (Gap) |
| **Row 4** (ZXCV) | 17 keys | `84` = LShift, `86..92` = Z..M, `93` = <, `94` = >, `96` = /?, `97` = RShift, `99` = Up, `101..103` = Num1/2/3, `104` = NumEnter | `85` (ISO Shift gap), `95` (Gap), `98` (Gap), `100` (Gap) |
| **Row 5** (Bottom) | 13 keys | `105` = LCtrl, `106` = Win, `107` = LAlt, `111` = Spacebar, `115` = RAlt, `116` = Fn, `117` = App, `118` = RCtrl, `119..121` = Left/Down/Right, `122` = Num0, `124` = Num. | `108..110` (Space left gap), `112..114` (Space right gap), `123` (Num 0 latch), `125` (Gap) |

*Total physical switches*: **104 switches**.
*Total sparse matrix slots*: **126 slots**. Unpopulated slots must be explicitly zeroed to prevent lingering firmware latches.

---

## 6. Linux Integration: `udev` Permissions

On Linux systems, non-root access to raw USB HID interfaces requires a `udev` rule granting read/write permissions to the `uaccess` tag or `plugdev` group.

Create `/etc/udev/rules.d/99-fantech-mk894.rules`:
```udev
# Fantech MK894 Maxpower II Mechanical Keyboard
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="36ae", ATTRS{idProduct}=="fe9c", MODE="0660", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="36ae", ATTRS{idProduct}=="feab", MODE="0660", TAG+="uaccess"
```

Reload rules:
```bash
sudo udevadm control --reload-rules && sudo udevadm trigger
```

---

## 7. Python Implementation Reference

The accompanying clean-room Python driver is located in:
* Core Protocol Encoder/Decoder: [`protocol.py`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/protocol.py)
* CLI Control Script: [`mk894_control.py`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/mk894_control.py)
* Test Suite: [`test_protocol.py`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/tests/test_protocol.py)

### Example: Setting Static Purple Color (Clean Python)
```python
from re_workspace.driver.protocol import MK894Protocol, LightingMode

# Build a 65-byte USB HID report
packet = MK894Protocol.encode_set_light_config(
    mode=LightingMode.STATIC,
    brightness=100,
    speed=50,
    direction=0,
    color_mode=1,       # Custom color
    rgb=(180, 0, 255)   # Purple
)

# Transmit to device via hidapi
import hid
device = hid.device()
device.open(0x36AE, 0xFE9C)
device.write(packet)
device.close()
```
