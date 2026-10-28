# Fantech MK894 Maxpower II Protocol Specification

**Specification Version:** 1.0.0  
**Status:** Clean-Room Protocol Interoperability Standard  
**Authors:** Clean-Room Engineering Team  
**Scope:** Standalone wire protocol specification for independent driver implementations.

---

## 1. Physical & USB Transport Architecture

The **Fantech MK894 Maxpower II** mechanical keyboard communicates over standard USB 1.10 Full-Speed (12 Mbps).

### 1.1 USB Identifiers

* **Vendor ID (VID):** `0x36AE` (13998 decimal)
* **Product ID (PID):** `0xFE9C` (65180 decimal, standard 104-key ANSI MK894 / AK107)
* **Secondary PID:** `0xFEAB` (65195 decimal, AK110-DS variant)
* **Device Release / BCD:** `0.01`
* **Manufacturer String:** `SDCX`
* **Product String:** `Fantech Maxpower II Keyboard`

### 1.2 USB Interface Descriptors

The device enumerates with 3 distinct HID interfaces:
* **Interface 0 (Input 0):** Boot Keyboard (`bInterfaceClass = 3`, `bInterfaceSubClass = 1`, `bInterfaceProtocol = 1`). Endpoint `0x81` (EP 1 IN), 8 bytes, `bInterval = 1` ms. Emits standard 8-byte boot scancodes.
* **Interface 1 (Input 1):** Extended Consumer Controls (`bInterfaceClass = 3`, `bInterfaceSubClass = 1`, `bInterfaceProtocol = 1`). Endpoint `0x82` (EP 2 IN), 16 bytes, `bInterval = 1` ms. Emits volume knob, mute, and multimedia scancodes.
* **Interface 2 (Input 2):** **Vendor Configuration Interface** (`bInterfaceClass = 3`, `bInterfaceSubClass = 0`, `bInterfaceProtocol = 0`). Usage Page `0xFF00` (Vendor-Defined), Usage `0x0002`.
  * Endpoint `0x83` (EP 3 IN): Interrupt IN, 64 bytes max packet size, `bInterval = 1` ms.
  * Endpoint `0x03` (EP 3 OUT): Interrupt OUT, 64 bytes max packet size, `bInterval = 1` ms.

---

## 2. Wire Framing & Protocol Format

All device management, lighting, macro, and remap commands are transmitted over **Interface 2** using fixed **64-byte USB Output Reports** (matching Endpoint 3 max packet size `wMaxPacketSize = 64` and the unnumbered HID report descriptor).

```
+------------+------------+--------------------+--------------------------------+
| Byte 0     | Byte 1     | Bytes 2..62        | Byte 63                        |
+------------+------------+--------------------+--------------------------------+
| Magic Byte | Opcode     | Command Payload    | Zero Padding                   |
| (0x06)     | (1 byte)   | (Length varies)    | (Padded with 0x00 to 64 bytes) |
+------------+------------+--------------------+--------------------------------+
```

### Protocol Invariants:
1. **Total Packet Length:** Must be exactly **64 bytes**. On Linux `hidraw` write, passing 65 bytes results in `-EINVAL` because the kernel validates against the 64-byte descriptor report size.
2. **Magic Signature (Byte 0):** Strictly `0x06`. Required by the Artery Tek MCU command dispatcher.
3. **Opcode (Byte 1):** 1-byte command identifier.
4. **Padding:** Any trailing bytes between the end of the command payload and Byte 63 must be zeroed (`0x00`).
5. **Checksum:** Fixed `0x00`. Data integrity is enforced at the USB packet token/CRC16 layer.

---

## 3. Command Opcode Reference Table

| Opcode (Hex) | Opcode (Dec) | Direction | Command Name | Safety Classification | Brief Description |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `0x05` | 5 | Host <- Device | `GET_DEVICE_INFO` | **READ** | Queries firmware version, profile count, hardware state. |
| `0x06` | 6 | Host <- Device | `GET_LAYOUT_BYTES` | **READ** | Reads key matrix physical geometry chunk. |
| `0x08` | 8 | Host <- Device | `GET_KEYMAP_CHUNK` | **READ** | Reads 4-byte key remap definitions (56-byte chunks). |
| `0x09` | 9 | Host -> Device | `SET_KEYMAP_CHUNK` | **WRITE-PERSISTENT** | Writes 4-byte key remap definitions in chunks. |
| `0x0A` | 10 | Host <- Device | `GET_LIGHT_CONFIG` | **READ** | Reads current global lighting settings. |
| `0x0B` | 11 | Host -> Device | `SET_LIGHT_CONFIG` | **WRITE-REVERSIBLE** | Sets preset lighting mode, brightness, speed, HSV. |
| `0x0C` | 12 | Host <- Device | `GET_MACRO_CHUNK` | **READ** | Reads 4,096-byte onboard macro memory in 56-byte chunks. |
| `0x0D` | 13 | Host -> Device | `SET_MACRO_CHUNK` | **WRITE-PERSISTENT** | Writes 4,096-byte onboard macro memory in chunks. |
| `0x0F` | 15 | Host -> Device | `DEVICE_MAINTENANCE`| **DESTRUCTIVE** | Reset factory (`0xFF`) or wipe macros (`0x04`). |
| `0x10` | 16 | Host -> Device | `SET_SINGLE_KEYMAP` | **WRITE-PERSISTENT** | Updates remap descriptor for a single specified key. |
| `0x12` | 18 | Host -> Device | `STREAM_RGB_MATRIX` | **WRITE-REVERSIBLE** | Streams 312-byte per-key RGB canvas in 7 packets. |
| `0x14` | 20 | Host -> Device | `SET_SINGLE_KEY_LED`| **WRITE-REVERSIBLE** | Sets static RGB color for an individual key LED. |
| `0xFB` | 251 | Host -> Device | `SELECT_PROFILE` | **WRITE-REVERSIBLE** | Activates hardware profile 0, 1, or 2. |
| `0xFC` | 252 | Host -> Device | `SET_SLEEP_TIMEOUT`| **WRITE-PERSISTENT** | Sets wireless sleep timeout (no-op on wired MK894). |
| `0x5A` | 90 | Host -> Device | `JUMP_BOOTLOADER` | **DESTRUCTIVE / BANNED** | Disconnects HID; jumps to Artery Tek ROM IAP. |

---

## 4. Command Specifications

### 4.1 Device Info Query (`0x05`)
* **Direction:** Request (Host -> Device), Response (Device -> Host)
* **Request Packet:** `[0x06, 0x05, 0x00, ... 0x00]` (65 bytes)
* **Response Packet (65 bytes):**
  * `Byte 0`: `0x06` (Report ID)
  * `Byte 1`: `0x05` (Acknowledge Opcode)
  * `Bytes 2..9`: Hardware profile string (e.g. `AK107\0`)
  * `Bytes 10..13`: Firmware version string (e.g. `V101`)
  * `Byte 14`: Active profile index (`0`, `1`, or `2`)
  * `Byte 15`: Total available profiles (`0x03`)

---

### 4.2 Global Lighting Configuration (`0x0B`)
* **Direction:** Host -> Device (Volatile lighting state)
* **Request Packet Layout (64 bytes):**
  ```
  Byte 0:  0x06 (Report ID / Command Magic)
  Byte 1:  0x0B (Opcode: SET_LIGHT_CONFIG)
  Byte 2:  0x0B (Payload Length = 11 bytes)
  Byte 3:  0x00 (Reserved)
  Byte 4:  0x00 (Reserved)
  Byte 5:  Zone (0x01 = Main Key Matrix Backlight, 0x02 = Edge Accent Strips)
  Byte 6:  0x00 (Reserved)
  Byte 7:  Mode Index (0..16)
  Byte 8:  Brightness (0..4 discrete levels, where 4 = 100%)
  Byte 9:  Speed (0..4 discrete levels, where 4 = fastest, 2 = normal)
  Byte 10: Direction (0 = Default, 1 = Left-to-Right, 2 = Up-to-Down)
  Byte 11: Color Mode (0 = Dynamic Rainbow/Spectrum, 1 = Fixed User Color)
  Byte 12: 0x00 (Reserved)
  Byte 13: Hue (0..255)
  Byte 14: Saturation (0..255)
  Byte 15: Value (0..255)
  Bytes 16..63: 0x00 (Zero Padding)
  ```
* **Supported Modes (0..16):**
  `0`=Off, `1`=Static, `2`=Breathing, `3`=Rotate, `4`=Spectrum Cycle, `5`=Stream Left-to-Right, `6`=Bloom, `7`=Stream Up-to-Down, `8`=Shuttle, `9`=Raindrop, `10`=Meteor, `11`=Ripple Fade, `12`=Ripple Static, `13`=Reactive, `14`=Grid Ripple, `15`=Tide, `16`=Custom RGB Matrix Canvas.

### 4.2.1 Query Active Lighting Configuration (`0x0A`)
* **Direction:** Request (Host -> Device), Response (Device -> Host)
* **Request Packet:** `[0x06, 0x0A, 0x00, ... 0x00]` (64 bytes)
* **Response Packet (64 bytes):**
  ```
  Byte 0:  0xAA (Device Response Magic)
  Byte 1:  0x0A (Opcode: GET_LIGHT_CONFIG)
  Byte 2:  0x0B (Payload Length = 11 bytes)
  Byte 3..4: 0x00 (Reserved)
  Byte 5:  Zone (0x01 = Main Matrix)
  Byte 6:  0x00
  Byte 7:  Mode (0..16)
  Byte 8:  Brightness (0..4)
  Byte 9:  Speed (0..4)
  Byte 10: Direction
  Byte 11: Color Mode (0 = Spectrum, 1 = Fixed Color)
  Byte 12: Single Color Index
  Byte 13: Hue (0..255)
  Byte 14: Saturation (0..255)
  Byte 15: Value (0..255)
  Bytes 16..63: 0x00
  ```

---

### 4.3 Per-Key RGB Matrix Streaming (`0x12`)
To activate a custom color canvas:
1. Stream **312 raw RGB bytes** (104 physical keys × 3 bytes [R, G, B]) across **7 sequential 65-byte Output Reports**:
   * **Packets 1..5:** 56 color bytes each (Header length = 59 bytes; payload offsets 0, 56, 112, 168, 224).
   * **Packet 6:** 32 color bytes (Header length = 35 bytes; payload offset 280).
   * **Packet 7:** Final termination / synchronization packet.
   ```
   Byte 0:  0x06 (Report ID)
   Byte 1:  0x12 (Opcode)
   Byte 2:  Sequence / Chunk Index (0..6)
   Byte 3:  Payload Byte Length (56 or 32)
   Byte 4:  Offset Low Byte
   Byte 5:  Offset High Byte
   Bytes 6..61: RGB Color Data Chunks
   Bytes 62..64: Zero Padding
   ```
2. Immediately send `Opcode 0x0B` with `Mode = 16` to activate the uploaded custom canvas.

---

### 4.4 Single-Key LED Color (`0x14`)
* **Direction:** Host -> Device
* **Matrix Key Index Range:** `0..126` (sparse hardware matrix coordinate, e.g. Esc=0, Pause=15, Space=111, Num.=124).
* **Request Packet Layout (64 bytes):**
  ```
  Byte 0:  0x06 (Report ID / Command Magic)
  Byte 1:  0x14 (Opcode: SET_SINGLE_KEY_LED)
  Byte 2:  0x03 (Payload Length = 3 RGB bytes)
  Byte 3:  Offset Low Byte ((key_index * 3) & 0xFF)
  Byte 4:  Offset High Byte ((key_index * 3) >> 8)
  Byte 5..7: 0x00 (Reserved)
  Byte 8:  Red (0..255)
  Byte 9:  Green (0..255)
  Byte 10: Blue (0..255)
  Bytes 11..63: 0x00 (Zero Padding)
  ```
* **Display Activation:** Transmitting `Opcode 0x14` updates the key color in internal canvas RAM. To render the updated canvas on the physical LEDs, it must be followed by `Opcode 0x0B` with `Mode = 16` (`CustomMatrixCanvas`).

---

### 4.5 Single-Key Remapping (`0x10`)
* **Direction:** Host -> Device (Persistent Onboard Flash Write)
* **Layout:**
  ```
  Byte 0:  0x06 (Report ID)
  Byte 1:  0x10 (Opcode)
  Byte 2:  Payload Length (0x07)
  Byte 3:  Key Memory Offset Low Byte (key_index * 4)
  Byte 4:  Key Memory Offset High Byte
  Byte 5:  0x00 (Reserved)
  Byte 6:  Layer (0x00 = Base Layer, 0x01 = Fn Layer)
  Byte 7:  0x00 (Reserved)
  Byte 8:  Action Category Type ID (16, 17, 31, 32, 48, 64, 96)
  Byte 9:  Param Code 1
  Byte 10: Param Code 2
  Byte 11: Param Code 3
  Bytes 12..64: 0x00 (Padding)
  ```
* **Action Types:**
  * `0x10` (16): Mouse Button (1=Left, 2=Right, 4=Middle, 8=Forward, 16=Back, 32=WheelUp, 64=WheelDown).
  * `0x1F` (31): Hardware Lighting / Win Lock (`code1 = 0x0A` for WinLock).
  * `0x20` (32): Standard HID Keyboard Scancode (`code1 = USB HID Usage ID`, e.g. `0x04` for 'A').
  * `0x30` (48): Consumer / Multimedia (`code1, code2 = 16-bit Consumer Usage ID`).
  * `0x60` (96): Macro Trigger (`code1 = Macro Slot Index 0..15`).

---

### 4.6 The "4KB Engine": Hardware Macro Memory (`0x0C`, `0x0D`)
The MK894 contains an autonomous **4,096-byte (4 KB) onboard macro memory**:

```
+-----------------------------------+---------------------------------------------+
| Bytes 0000..0063 (64 bytes)       | Bytes 0064..4095 (4032 bytes)               |
+-----------------------------------+---------------------------------------------+
| Offset Pointer Table (32 macros)  | Sequential 4-Byte Macro Action Records      |
| [ptr0_lo, ptr0_hi, ptr1_lo, ...]  | [delay_lo, delay_hi, flags, code]           |
+-----------------------------------+---------------------------------------------+
```

1. **Header (Pointers Table):**
   * Stores 32 16-bit little-endian pointers (one for each macro slot `M0` to `M31`).
   * Empty/unassigned slots contain `0xFFFF`.
   * Assigned slots contain their starting byte offset (e.g. `M0` points to `0x0040` / `64`).
2. **Action Records (4 bytes each):**
   * **Bytes 0..1:** Delay in milliseconds (`uint16_le`, 0 to 65535 ms).
   * **Byte 2 (Flags Bitmask):**
     * `Bit 7 (0x80)`: **End-Of-Macro Flag** (`1` = Final action of sequence; terminates playback).
     * `Bit 6 (0x40)`: **Action State** (`1` = Key Down, `0` = Key Up).
     * `Bits 0..5 (0x3F)`: **Event Type** (2=Keyboard Key, 3=Mouse Button, 4=Wheel V, 5=Wheel H).
   * **Byte 3:** Scancode or mouse button identifier.
3. **Chunk Transfer:**
   * `Opcode 0x0C`: Reads 4096 bytes from device in 56-byte chunks (74 packets).
   * `Opcode 0x0D`: Writes 4096 bytes to device in 56-byte chunks (74 packets).

---

### 4.7 Profile Selection (`0xFB`)
* **Direction:** Host -> Device
* **Layout:** `[0x06, 0xFB, profile_index, 0x00, ... 0x00]` (`profile_index` in `0..2`).
* Activates the saved onboard configuration profile in hardware.

---

### 4.8 Safety Enforcement & Banned Opcodes
* **`Opcode 0x5A` (`goToIAP`):** Strictly banned in user drivers.
* **`Opcode 0x55` (Bootloader Flashing):** Strictly banned in user drivers.
* Rejection policy: Attempting to serialize or transmit these opcodes raises an unrecoverable `PermissionError` / `SecurityError`.
