# Fantech MK894 Companion Software Architecture & Complete Reverse-Engineering Report

**Document Version:** 1.0.0  
**Date:** 2026-10-26  
**Target:** Official Companion Software `MK894 Setup 1.0.1.exe` / Installed Bundle `MK894.exe` (resources/`app.asar`)  
**Hardware Platform:** Fantech MK894 Maxpower II Mechanical Keyboard (Artery Tek AT32F405 ARM Cortex-M)  
**Classification:** Complete Software Functional Reverse-Engineering Whitepaper  

---

## 1. Executive Summary & Forensic Provenance

The official Fantech MK894 companion software (`MK894 Setup 1.0.1.exe`) is an Electron-based desktop application providing full configuration management for the MK894 mechanical keyboard.

### Target Files & Cryptographic Verification
* **Installer Binary:** `MK894 Setup 1.0.1.exe` (91,490,247 bytes)  
  `SHA-256: 4588482AD61B4408668D336A052FCB212917E719D2BA3DDCAEA9AF59E5ACAEDD`
* **Application Core Archive:** `resources/app.asar` (31,713,899 bytes)  
  `SHA-256: E1DF436FE4D61CAF729F5ABAA6948A0E54B30EA5458E7E47CEB114BFA71589C0`
* **Official Shipped Firmware:** `build/AK110M_UK_V101.bin` (57,344 bytes)  
  `SHA-256: 29D10B3030A532CF08D070C9951BD129D58955E4B55760E7D46BBC74A400D32E`

Reverse engineering of the 312 unpacked files inside `app.asar` reveals that the software is architected as an **Electron v28 runtime hosting a Next.js single-page static export**. All device communication takes place using Chromium's **W3C WebHID API** (`navigator.hid`) directly from the renderer process, without relying on native C++ Node addons (`node-hid`).

---

## 2. Application Architecture & Main Process Lifecycle

### 2.1 Electron Main Process (`main/index.js`)
* **Window Specifications:**
  * Width: 1800 px, Height: 1000 px (Min-Width: 1800, Min-Height: 1000)
  * Menubar: Hidden (`autoHideMenuBar: true`)
  * Zoom: Permanently locked at 1.0 (`setZoomFactor(1)`, `setVisualZoomLevelLimits(1, 1)`)
* **Security & Flag Configuration:**
  * Chromium Flag: `--disable-hid-blocklist` is appended to bypass Chrome's default security blocklist on raw keyboard HID interfaces.
  * Node integration enabled (`nodeIntegration: true`).
* **WebHID Device Selection & Permission Handlers:**
  Instead of prompting the user with Chromium's standard device picker, the main process implements automated permission filters matching devices against an internal offline list (`main/assets/vid_pid_offline.js`):
  * Vendor ID: `0x36AE` (13998 decimal)
  * Product ID: `0xFE9C` (MK894 / AK107 Profile) -> Usage Page `0xFF00`, Usage `0x0002`
  * Product ID: `0xFEAB` (AK110-DS Variant) -> Usage Page `0xFF00`, Usage `0x0001`

---

## 3. The 5 Functional Subsystems (UI Modules)

The companion software UI is divided into 5 discrete functional modules defined in `page-4962631fc89cfadd.js`:

```
+---------------------------------------------------------------------------------+
|                         Fantech MK894 Software Modules                          |
+---------------+---------------+-----------------+---------------+---------------+
| 1. Key Tab    | 2. Light Tab  | 3. Macro Tab    | 4. Config Tab | 5. Update Tab |
| (Remapping)   | (RGB Matrix)  | (4KB Engine)    | (Profiles)    | (DFU Flasher) |
+---------------+---------------+-----------------+---------------+---------------+
```

---

### Module 1: Key Assignment & Remapping Subsystem

The key remapping subsystem allows modifying the function of any of the keyboard's 104 keys across multiple layers (Base Layer 0, Fn Layer 1).

#### Remapping Wire Protocol:
* **Single-Key Remap (`Opcode 0x10`):** Transmits a 65-byte Output Report updating key `index`:
  ```
  [0x06, 0x10, 0x07, offset_lo, offset_hi, 0x00, layer, 0x00, type, code1, code2, code3, ... 0x00]
  ```
  *(Offset = `key_index * 4`)*
* **Batch Remap (`Opcode 0x09`):** Streams the full keymap table in 56-byte chunks.

#### The 6 Key Remapping Action Types:
The software embeds a catalog of 209 action definitions across 6 functional categories (`remap_catalog.json`):

| Type ID | Hex | Category | Item Count | Key Action Examples |
| :--- | :--- | :--- | :--- | :--- |
| `16` | `0x10` | **Mouse Actions** | 7 | Left Click, Middle Click, Right Click, Forward, Backward, Wheel Up, Wheel Down |
| `17` | `0x11` | **Cursor Control** | 1 | Mouse Cursor Emulation (`CursorMove`) |
| `31` | `0x1F` | **Hardware Lighting**| 11 | Light Toggle, Brightness +/-, Effect Switch, Color Cycle, Speed +/-, Win Lock |
| `32` | `0x20` | **Standard HID Keys** | 133 | Esc, F1-F24, Alphanumerics, Modifiers (Ctrl, Alt, Shift, Win), Japanese/Korean IME |
| `48` | `0x30` | **Multimedia / OS** | 38 | Volume Up/Down/Mute, Play/Pause, Media Player, Calculator, Web Browser, Mail |
| `64` | `0x40` | **Power Management** | 3 | ACPI Power Down, Sleep, System Wakeup |
| `96` | `0x60` | **Hardware Macros** | 16 | Onboard Macro Triggers `M0` through `M15` (`value1` = 0..15) |

---

### Module 2: Lighting & Custom RGB Canvas Subsystem

Provides both preset animations and a per-key addressable RGB canvas.

#### Global Lighting Engine (`Opcode 0x0B`):
* Payload length: 11 bytes.
* Parameters: Zone (0=Main Backlight, 1=Side Light Strips), Mode (0-16), Brightness (0-100), Speed (0-100), Direction (0=Default, 1=Left-Right, 2=Up-Down), Color Mode (0=Spectrum, 1=Custom Fixed), HSV 3-byte values.
* **All 17 Lighting Modes:**
  `0`=Off, `1`=Static, `2`=Breathing, `3`=Rotate, `4`=Spectrum, `5`=Stream LR, `6`=Bloom, `7`=Stream UD, `8`=Shuttle, `9`=Raindrop, `10`=Meteor, `11`=Ripple Fade, `12`=Ripple Static, `13`=Reactive, `14`=Grid Ripple, `15`=Tide, `16`=Custom RGB Canvas.

#### Per-Key RGB Matrix Streaming (`Opcode 0x12`):
* Full-matrix custom layouts require 312 bytes of raw RGB data (104 keys × 3 bytes).
* The software splits the 312 bytes into **7 sequential 65-byte USB Output Reports**:
  * Packets 1 to 5: 56 color bytes each (Header length 59, offsets `0, 56, 112, 168, 224`).
  * Packet 6: 32 color bytes (Header length 35, offset `280`).
* Once uploaded, sending `setLightConfig(mode=16)` instructs the firmware to activate the custom matrix.

---

### Module 3: Hardware Macro Engine Subsystem

The keyboard features an autonomous **4096-byte (4 KB) onboard macro memory** that executes complex keystroke sequences directly in hardware without software intervention.

#### Memory Layout (4096 Bytes):
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
   * Empty/unassigned slots contain `0xFFFF` (`[0xFF, 0xFF]`).
   * Defined macros point to their starting offset (e.g. `M0` points to `0x0040` / `64`).
2. **Action Records (4 bytes each):**
   * **Bytes 0..1:** Delay in milliseconds (`uint16_le`, 0 to 65535 ms).
   * **Byte 2 (Flags Bitmask):**
     * `Bit 7 (0x80)`: **End-Of-Macro Flag** (`1` = Final action of sequence; terminates playback).
     * `Bit 6 (0x40)`: **Action State** (`1` = Key/Button Down, `0` = Key/Button Up).
     * `Bits 0..5 (0x3F)`: **Event Type**:
       * `2` = Keyboard Key Scancode
       * `3` = Mouse Button (1=Left, 2=Right, 4=Middle, 8=Forward, 16=Backward)
       * `4` = Mouse Wheel Vertical (1=Up, 255=Down)
       * `5` = Mouse Wheel Horizontal (1=Right, 255=Left)
   * **Byte 3:** Scancode or mouse button identifier.

#### Communication Opcodes:
* `Opcode 0x0C`: Reads the 4096-byte memory in 56-byte chunks (74 packets).
* `Opcode 0x0D`: Writes macro data in 59-byte chunks.
* `Opcode 0x0F` (Subcommand `4`): Hardware macro memory wipe.
* **Offline Persistence:** The software mirrors macros in browser `localStorage` as `macro_arr` and `macro_action_list`, and exports/imports them as `macro.json`.

---

### Module 4: Hardware Settings & Profiles Subsystem

Manages device hardware state, profiles, and power savings:

* **Onboard Profiles (`Opcode 0xFB`):**
  Switches active profile between `0`, `1`, and `2` (`[0x06, 0xFB, profile_idx]`).
* **Inactivity Auto-Sleep (`Opcode 0xFC`):**
  Sets the sleep timeout in seconds (`uint16_le`). Presets defined by vendor:
  * `0` = Never sleep
  * `120` = 2 minutes
  * `300` = 5 minutes
  * `600` = 10 minutes
  * `1800` = 30 minutes
* **Factory Reset (`Opcode 0x0F`):**
  Triggers a complete non-volatile memory wipe (`[0x06, 0x0F, 0xFF]`).
* **Configuration Query (`Opcode 0x05`):**
  Returns a 65-byte response containing hardware revision, firmware version, link state, battery percentage, active profile, and serial number string.

---

### Module 5: Firmware Update / DFU Bootloader Subsystem

The companion software includes an integrated ROM bootloader flasher.

#### Microcontroller & Target Hardware:
* **MCU Architecture:** Artery Tek **AT32F405** (`AT405`) ARM Cortex-M series microcontroller.
* **Firmware Binary:** Shipped inside `build/` as `AK110M_UK_V101.bin` (57,344 bytes).

#### The 7-Step Bootloader Flashing Protocol:
```
[Host] ----------------- Opcode 0x55 [0xFF, 0x00, 0x00] ---------------> [MK894]
                       (Initiate Firmware Update Mode)

[Host] ----------------- Opcode 0x5A [0xA0] (goToIAP) ------------------> [MK894]
                       (Jump MCU to Artery Tek ROM Bootloader)

[Host] ----------------- Opcode 0x55 [0xFF, 0x01, 0x00] ---------------> [Bootloader]
                       (Bootloader Handshake / Connection)

[Host] --------- Opcode 0x55 [0xFF, 0x02, 0x04, Addr_H, Addr_L, ...] --> [Bootloader]
                       (Set Flash Write Address & Length)

[Host] ----------------- Sequential ROM Chunk Data --------------------> [Bootloader]
                       (Transmit Binary Payload in Chunks)

[Host] ----------------- Opcode 0x55 [0xFF, 0x04, 0x00] ---------------> [Bootloader]
                       (ROM Checksum Verification)

[Host] ----------------- Opcode 0x55 [0xFF, 0x05, 0x01, 0x01] ----------> [Bootloader]
                       (Exit Bootloader & Reboot Firmware)
```

> [!CAUTION]
> **Safety Boundary:** Opcode `0x5A` (`goToIAP`) disconnects the standard USB HID keyboard driver and forces the MCU into the Artery Tek ROM bootloader. If executed without an intact firmware flashing pipeline, the keyboard will appear unpowered or unresponsive until a valid ROM is written. The clean-room driver permanently blocks this opcode.

---

## 4. Multi-Device Platform Ecosystem

Analysis of `build/update.json` and `build/_next/static/chunks/` shows that this companion software is an OEM white-label client platform built by Shenzhen Shengyuan / SDCX. It contains firmware references and layout templates for over 40 distinct peripherals:

1. **Macropads (3-key to 16-key with rotary knobs):**
   * Profiles `3key`, `4key`, `6key`, `8key`, `9key`, `10key`, `12key`, `15key`, `16key` (e.g. `0816_246d` to `0816_2479`).
2. **Compact Keyboards:**
   * 61-key and 68-key templates (`61-template.json`, `68-template.json`).
3. **Full-Size & 98-Key Keyboards:**
   * `AK107` / MK894 (`36ae_fe9c`)
   * `AK110-DS` (`36ae_feab`)
   * `rfk98` (`0817_dcfb`)
   * `gamekb17` (`0816_021f`)

All devices share the identical 65-byte USB Output Report framing and WebHID opcode dispatch architecture.

---

## 5. Clean-Room Alternative Verification

Every single operational capability provided by the vendor companion app has been independently reverse-engineered and cleanly implemented in our standalone Python suite without third-party dependencies:

| Vendor Feature | Reverse-Engineered Mechanism | Clean-Room Driver Implementation |
| :--- | :--- | :--- |
| **Device Identification** | W3C WebHID enumerator | [`mk894_control.py --info`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/mk894_control.py) |
| **Lighting Effects (17 Modes)**| Opcode `0x0B` Output Report | [`mk894_control.py --set-mode`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/mk894_control.py) |
| **Custom RGB Matrix Canvas** | Opcode `0x12` multi-packet stream | [`mk894_control.py --preset / --load-colors`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/mk894_control.py) |
| **Per-Key Single LED** | Opcode `0x14` 3-byte payload | [`mk894_control.py --set-key`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/mk894_control.py) |
| **Physical Keymap Table** | Webpack chunk `2517` | [`keymap.py`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/keymap.py) & `--dump-keys` |
| **Key Remapping & Disabling** | Opcode `0x10` 4-byte offset | [`mk894_control.py --remap-key / --disable-key`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/mk894_control.py) |
| **Onboard Macro Memory** | 4096-byte pointer/action buffer | [`protocol.build_macro_buffer()`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/protocol.py) |
| **Profile Switching** | Opcode `0xFB` | [`mk894_control.py --profile`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/mk894_control.py) |
| **Auto-Sleep Configuration** | Opcode `0xFC` | [`mk894_control.py --sleep`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/mk894_control.py) |
| **Factory Reset** | Opcode `0x0F` | [`mk894_control.py --factory-reset --confirm`](file:///C:/Users/user/AppData/Local/Programs/MK894/re-workspace/driver/mk894_control.py) |
| **Safety Interceptor** | Banned Opcode `0x5A` | `protocol.BANNED_OPCODES` (`PermissionError`) |
