# FantasTech — Cross-Platform Driver & Control Suite for Fantech MK894

<p align="center">
  <img src="assets/icons/Yarny.png" alt="Yarny Mascot" width="128" height="128" />
  <br />
  <strong>FantasTech</strong>
  <br />
  <em>Professional Cross-Platform Driver, RGB Control Center & Microcontroller Safety Suite for the Fantech MK894 Maxpower II Mechanical RGB Keyboard</em>
</p>

---

## Overview

**FantasTech** is a high-performance, cross-platform driver, background daemon, OpenRGB server, and native graphical control center for the **Fantech MK894 Maxpower II** mechanical RGB keyboard (USB Vendor ID `0x36AE`, Product ID `0xFE9C`, IAP Bootloader PID `0xFEAB`).

Engineered entirely in clean-room Rust, FantasTech replaces proprietary vendor blobs with a safe, responsive, unified single executable that requires zero external runtimes (no Python, no PyQt, no Electron).

---

## Key Features

* **Native GUI Control Center**: Modern, responsive user interface featuring the authentic physical 104-key ANSI layout, live per-key RGB canvas painting with dual-pad mirrored spacebar addressing, preset library, macro editor, and Yarny mascot branding.
* **Microcontroller Protection & Hardware Safety**:
  * **Safe IAP Bootloader State Machine**: Full USB re-enumeration management between application (`0xFE9C`) and bootloader (`0xFEAB`).
  * **Hardware Checksum Verification**: Reads back and verifies MCU flash checksum prior to rebooting, preventing unverified or corrupt firmware execution.
  * **Firmware Safety Warning Modal**: Prevents accidental background flash updates; enabling automatic firmware updates requires explicit user acknowledgment of hardware risks.
* **Full Lighting Suite**: Complete control of all 17 hardware lighting modes, speed, brightness, custom palettes, and per-key RGB matrix canvas painting.
* **Autonomous Onboard Macro Engine**: Full compiler and decompiler for the keyboard's 4KB onboard EEPROM/flash macro table (up to 32 independent macro sequences).
* **Multi-Layer Key Remapping**: Custom key actions and layer assignment across Base and Fn layers with bounded undo/redo history.
* **Volume Knob Runaway Guard (Linux)**: Real-time event debouncer preventing volume runaway spikes on Linux (GNOME, KDE Plasma, Wayland) via exclusive evdev capture (`EVIOCGRAB`).
* **Built-in OpenRGB TCP Server**: Exposes the keyboard over standard OpenRGB SDK protocol (port 6742) for seamless synchronization with OpenRGB ecosystems.
* **Multi-Domain Profile Manager**: Save, load, and switch profiles independently for lighting, keymaps, macros, and device settings.
* **Clean-Room Virtual Emulator**: Built-in in-memory virtual MCU simulation (`--emulator`) allowing full development, testing, and CI verification without requiring physical hardware.

---

## Hardware Specifications & Architecture Notes

Reverse engineering of the Fantech MK894 (Artery AT32F405 MCU, firmware v1.01) identified the following hardware characteristics:
* **Per-Key RGB Backlight Matrix**: Features 126 coordinate slots across rows 0..5 and columns 0..20, fully addressable via Opcode `0x14` for custom static colors, dynamic canvases, and OpenRGB streaming.
* **Animation Flow**: Built-in animation presets (e.g. Wave, Stream, Bloom) utilize fixed firmware animation directions; direction reversal is not supported by the MCU's internal lighting engine.
* **Lock Indicator LEDs (Num, Caps, Win, Scroll Lock)**: Driven by dedicated GPIO output pins directly tied to the MCU's internal USB HID keyboard state. They are physical single-color monochrome white LEDs; custom brightness, colors, and software animation effects are not supported by the hardware.
* **Side Chassis RGB Strips**: Controlled via dedicated physical keyboard hotkeys (`Fn + Pause` to toggle modes/off, `Fn + Scroll Lock` for speed/color). They operate on an isolated secondary hardware circuit and are not part of the addressable 104-key switch matrix canvas.

---

## Project Architecture

```
maxpower_ii_rea/
├── core/                         # Rust Cargo Workspace
│   ├── Cargo.toml
│   └── crates/
│       ├── fantastech-protocol/  # Wire report framing, opcodes, OpenRGB SDK framing
│       ├── fantastech-transport/ # Cross-platform USB HID & Linux /dev/hidraw transport
│       ├── fantastech-device/    # Hardware handles, device abstraction traits & safe flasher
│       ├── fantastech-emulator/  # In-memory virtual keyboard MCU emulator
│       ├── fantastech-core/      # Config, profile management & cross-platform IPC
│       ├── fantastech-gui/       # Native graphical interface (eframe/egui)
│       └── fantastech-cli/       # Unified application executable (CLI + GUI dispatch)
│
├── assets/                       # Assets & icons
│   └── icons/                    # Yarny mascot icons, SVG glyphs, ICO
│
├── distribution/                 # Cross-platform packaging & installer scripts
│   ├── flatpak/                  # Linux Flatpak manifest & udev rules (org.fantastech.FantasTech)
│   └── windows/                  # Inno Setup installer configuration (.iss)
│
├── docs/                         # Technical documentation & guides
│   ├── CLI_GUIDE.md              # Complete CLI command reference
│   ├── PROTOCOL.md               # USB HID binary wire protocol specification
│   ├── ARCHITECTURE.md           # Internal crate architecture & IPC specifications
│   ├── OPENRGB_INTEGRATION.md    # OpenRGB SDK integration & controller mapping
│   └── mk894_layout.json         # ANSI-104 sparse matrix mapping specification
```

---

## Installation & Quick Start

### 1. Linux (Flatpak)
FantasTech on Linux is distributed as a sandboxed Flatpak package:
```bash
# Build Flatpak bundle into dist/
./distribution/flatpak/build-flatpak.sh

# Install Flatpak bundle locally
flatpak install --user -y dist/FantasTech-0.1.5.flatpak

# Run application
flatpak run org.fantastech.FantasTech

# Complete uninstallation & cleanup
./distribution/flatpak/uninstall-flatpak.sh
```

### 2. Windows (Inno Setup Installer)
Build or run the standalone Windows installer:
```cmd
distribution\windows\build-installer.bat
```
* Installs to `%ProgramFiles%\FantasTech` or `%LocalAppData%\Programs\FantasTech`.
* Creates Start Menu entries, desktop shortcut, and notification tray integration.
* Automatically registers clean Windows uninstallation in Windows Settings.

### 3. Running from Source
You can also run the binary directly via Cargo:
```bash
# Launch GUI
cargo run --release --manifest-path core/Cargo.toml --bin fantastech

# Run CLI subcommands
cargo run --release --manifest-path core/Cargo.toml --bin fantastech -- info
```

---

## CLI Reference

For complete CLI documentation, see [docs/CLI_GUIDE.md](docs/CLI_GUIDE.md).

```bash
# Query hardware info and firmware version
fantastech info

# Query info using virtual emulator (no physical keyboard needed)
fantastech -e info

# Lighting controls
fantastech light mode 3 --speed 4 --brightness 4
fantastech light off

# Profile management
fantastech profile apply lighting cyberwave
fantastech profile list lighting

# Key remapping
fantastech remap set CapsLock LeftControl
fantastech remap list

# Run OpenRGB SDK server
fantastech openrgb-server --port 6742

# Background service management
fantastech service status
fantastech service start
fantastech service stop
```

---

## OpenRGB Synchronization

1. Start the FantasTech OpenRGB server:
   ```bash
   fantastech openrgb-server --port 6742
   ```
2. Open the OpenRGB application, navigate to the **SDK Client** tab, set Host to `127.0.0.1` and Port to `6742`, then click **Connect**.
3. The keyboard appears with full per-key matrix control.

---

## Platform Installers

Package configurations are provided in `distribution/`:
* **Linux Flatpak**: `distribution/flatpak/org.fantastech.FantasTech.yaml`
* **Windows Standalone Installer**: `distribution/windows/installer.iss` (Inno Setup)

---

## Automated Testing

Run all unit tests, safety verification tests, and virtual MCU emulator integration tests:
```bash
cargo test --manifest-path core/Cargo.toml --workspace
```

---

## Authors & Credits

Developed by **Ahmad Serieh** ([GitHub](https://github.com/serieh)).

---

## License

Licensed under the **Apache License, Version 2.0**. See the [LICENSE](LICENSE) file for terms.
