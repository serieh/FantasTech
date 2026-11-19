# FantasTech CLI Guide & Command Reference

**FantasTech** is the open-source driver, background daemon, and multi-domain profile management suite for the **Fantech MK894 Maxpower II Mechanical RGB Keyboard** (USB VID `0x36AE`, PID `0xFE9C`).

Author: **Ahmad Serieh**  
GitHub: [https://github.com/serieh](https://github.com/serieh)  
Email: [ahmadserieh.official@gmail.com](mailto:ahmadserieh.official@gmail.com)  
License: **Apache License 2.0**

> [!NOTE]
> All CLI commands can be invoked directly with `fantastech <command>` (the unified binary) or via the `fantastech-ctl` symlink.

---

## Table of Contents
1. [Overview & Architecture](#1-overview--architecture)
2. [Global Flags](#2-global-flags)
3. [Service Commands (`fantastech-ctl service`)](#3-service-commands)
4. [System Tray & GUI Launchers](#4-system-tray--gui-launchers)
5. [Configuration Management (`fantastech-ctl config`)](#5-configuration-management)
6. [Multi-Domain Profile Management (`fantastech-ctl profile`)](#6-multi-domain-profile-management)
7. [Lighting Commands (`fantastech-ctl light`)](#7-lighting-commands)
8. [Key Remapping (`fantastech-ctl remap`)](#8-key-remapping)
9. [Hardware Macro Engine (`fantastech-ctl macro`)](#9-hardware-macro-engine)
10. [Backup & Restore (`fantastech-ctl backup`)](#10-backup--restore)
11. [Hardware Settings & Factory Reset (`fantastech-ctl setting`)](#11-hardware-settings--factory-reset)
12. [Volume Knob Guard (`fantastech-ctl guard-volume`)](#12-volume-knob-guard)
13. [OpenRGB SDK Server (`fantastech-ctl openrgb-server`)](#13-openrgb-sdk-server)
14. [Key Verification Testing (`fantastech-ctl test-keys`)](#14-key-verification-testing)
15. [System Integration Setup (`setup-udev`, `setup-systemd`, `setup-kde`)](#15-system-integration-setup)

---

## 1. Overview & Architecture

FantasTech uses a client/daemon architecture designed for zero `/dev/hidraw` device contention:

```
                      ┌───────────────────────────────────────┐
                      │        FantasTech Background Daemon   │
                      │  - Unix Domain Socket Server          │
                      │    (~/.config/fantastech/             │
                      │     fantastech.sock)                  │
                      │  - OpenRGB TCP Server (Port 6742)     │
                      │  - Volume Knob Guard Debouncer        │
                      └──────────────────┬────────────────────┘
                                         │ Exclusive HID Handle
                                         ▼
                      ┌───────────────────────────────────────┐
                      │    Fantech MK894 Mechanical Keyboard  │
                      │       USB VID: 0x36AE, PID: 0xFE9C    │
                      └───────────────────────────────────────┘
                                         ▲
                IPC Socket Calls         │ Direct /dev/hidraw
               (When service is active)  │ (When service is stopped)
                                         │
        ┌────────────────────────────────┴─────────────────────────┐
        │                                                          │
┌───────┴──────────┐       ┌─────────────────┐       ┌─────────────┴──────────┐
│  FantasTech CLI  │       │ FantasTech GUI  │       │ FantasTech Tray Applet │
│  (`fantastech`)  │       │(`fantastech-gui`)│      │ (`fantastech_tray.py`) │
└──────────────────┘       └─────────────────┘       └────────────────────────┘
```

### Transparent IPC Routing
Whenever the background daemon is active, all commands sent via the `fantastech` CLI or `fantastech-gui` serialize as JSON IPC requests over the Unix domain socket (`~/.config/fantastech/fantastech.sock`), preventing device lockups. If the service is not running, commands automatically fallback to direct `/dev/hidraw` communication (or to the built-in virtual hardware emulator when no physical hardware is connected).

To force direct communication bypassing the IPC socket, pass `--no-ipc`.

---

## 2. Global Flags

These flags can precede any subcommand:

| Flag | Description |
| :--- | :--- |
| `-e`, `--emulator` | Route all commands to the virtual clean-room hardware emulator without needing physical hardware. |
| `--dry-run` | Simulate operations and print hardware payloads without modifying device non-volatile flash. |
| `--no-ipc` | Bypass the background service Unix domain socket and force direct `/dev/hidraw` access. |
| `--json` | Output results in structured JSON format suitable for scripting and piping. |
| `-d`, `--device <PATH>` | Target an explicit `/dev/hidrawX` character device path. |
| `-v`, `--verbose` | Enable debug logging output. |

---

## 3. Service Commands

Manage the unified FantasTech background service daemon:

```bash
# Run the daemon interactively in foreground
fantastech-ctl service run

# Start the background service daemon via systemd user session
fantastech-ctl service start

# Stop the running background service daemon
fantastech-ctl service stop

# Restart the background service daemon
fantastech-ctl service restart

# Query service status (running state, PID, active profiles, OpenRGB port, volume guard)
fantastech-ctl service status

# Install systemd user service unit (~/.config/systemd/user/fantastech.service) and enable autostart
fantastech-ctl service install

# Uninstall and disable systemd user service unit
fantastech-ctl service uninstall
```

---

## 4. System Tray & GUI Launchers

```bash
# Launch the native graphical interface (with auto-started background service)
fantastech-ctl gui
# Or run standalone binary:
fantastech-gui

# Launch the cross-platform Freedesktop system tray applet (hover tooltip: "FantasTech")
fantastech-ctl tray
```

---

## 5. Configuration Management

FantasTech stores persistent user preferences in `~/.config/fantastech/config.json`:

```bash
# Display full active configuration JSON
fantastech-ctl config show

# Get a specific setting value
fantastech-ctl config get active_lighting_profile
fantastech-ctl config get openrgb_port
fantastech-ctl config get guard_volume_enabled

# Set a configuration setting
fantastech-ctl config set openrgb_enabled true
fantastech-ctl config set openrgb_port 6742
fantastech-ctl config set openrgb_host 127.0.0.1
fantastech-ctl config set guard_volume_enabled true
fantastech-ctl config set auto_start_enabled true
fantastech-ctl config set auto_apply_lighting true
fantastech-ctl config set tray_icon_enabled true
fantastech-ctl config set gui_theme dark

# Print path to config file
fantastech-ctl config path

# Reset configuration to factory defaults
fantastech-ctl config reset
```

---

## 6. Multi-Domain Profile Management

FantasTech supports unlimited separate, independent user profiles across four discrete domains:
- **`lighting`**: RGB mode, colors, brightness, speed, canvas matrices
- **`macro`**: 4KB hardware macro memory definitions and triggers
- **`button`**: Main layer and FN layer key reassignment maps
- **`settings`**: Sleep timeout, OpenRGB settings, volume guard state

```bash
# List all saved profiles in a domain
fantastech-ctl profile list lighting
fantastech-ctl profile list macro
fantastech-ctl profile list button
fantastech-ctl profile list settings

# Create a new profile in a domain
fantastech-ctl profile create lighting my_profile --name "My Custom RGB"
fantastech-ctl profile create button fps_gaming --name "FPS WASD Reassignment"

# Apply an active profile immediately to hardware/service
fantastech-ctl profile apply lighting cyberpunk
fantastech-ctl profile apply lighting gaming
fantastech-ctl profile apply button default
fantastech-ctl profile apply macro wasd_rapid

# Export a profile to a JSON file
fantastech-ctl profile export lighting cyberpunk ~/cyberpunk.json

# Import a profile from a JSON file
fantastech-ctl profile import lighting my_theme ~/cyberpunk.json

# Delete a user-created profile
fantastech-ctl profile delete lighting custom_theme

# Switch onboard hardware profile slot (1..3)
fantastech-ctl profile switch 1
fantastech-ctl profile switch 2
fantastech-ctl profile switch 3
```

---

## 7. Lighting Commands

Control onboard RGB backlighting effects, brightness, speed, and individual per-key colors:

```bash
# List all 17 supported lighting effect modes
fantastech-ctl light list-modes

# Apply a preset effect
fantastech-ctl light preset rainbow-wave
fantastech-ctl light preset gaming
fantastech-ctl light preset neon-sunset
fantastech-ctl light preset matrix

# Set specific lighting mode with speed and brightness parameters
# Modes: 0=Off, 1=Static, 2=Breathing, 3=Wave, 4=Fade, 5=Ripple, 6=Reactive, etc.
# Brightness: 0 (Off) to 4 (Max)
# Speed: 1 (Slowest) to 5 (Fastest)
fantastech-ctl light set-mode 1 --color "#0071CF" --brightness 4
fantastech-ctl light set-mode 2 --color "#FF0055" --speed 3 --brightness 3
fantastech-ctl light set-mode 3 --speed 5 --brightness 4

# Turn off all keyboard RGB backlighting
fantastech-ctl light off

# Stream a custom 7x15 RGB color canvas matrix (105 keys)
fantastech-ctl light canvas --hex "#0071CF"
fantastech-ctl light canvas --file my_canvas.json
```

---

## 8. Key Remapping

Remap any key on the MK894's main layer or Fn layer with non-volatile persistence:

```bash
# Read current active remappings
fantastech-ctl remap dump

# Remap Caps Lock to Left Control on main layer
fantastech-ctl remap set CapsLock LeftControl

# Remap Escape to Grave / Tilde
fantastech-ctl remap set Escape Grave

# Remap Fn+W to Up Arrow
fantastech-ctl remap set W UpArrow --fn

# Reset all keymaps to factory defaults
fantastech-ctl remap reset
```

---

## 9. Hardware Macro Engine

Program keystroke macros directly into the keyboard's dedicated 4KB onboard flash memory:

```bash
# List all programmed macros
fantastech-ctl macro list

# Record and compile a macro sequence to a trigger key
fantastech-ctl macro set --key G1 --name "Rapid Fire" --events "Down:W,Delay:50,Up:W,Delay:50"

# Wipe all macros from hardware flash memory
fantastech-ctl macro wipe
```

---

## 10. Backup & Restore

Backup the keyboard's complete non-volatile memory or restore from a known good snapshot:

```bash
# Export full hardware state snapshot to JSON
fantastech-ctl backup create --out mk894_backup.json

# Restore keyboard from snapshot file
fantastech-ctl backup restore mk894_backup.json

# Restore dry-run (validate backup without writing to hardware)
fantastech-ctl backup restore mk894_backup.json --dry-run
```

---

## 11. Hardware Settings & Factory Reset

```bash
# Query hardware info and firmware version
fantastech-ctl info

# Set idle auto-sleep timeout in minutes (0 = disabled)
fantastech-ctl setting sleep 10
fantastech-ctl setting sleep 0

# Factory reset keyboard hardware to out-of-the-box state (requires confirmation token)
fantastech-ctl setting factory-reset --confirm RESET_MK894
```

---

## 12. Volume Knob Guard

Fixes the known Linux / KDE Wayland hardware bug where turning the rotary volume knob generates unbounded key repeats:

```bash
# Run volume knob guard debouncer in foreground
fantastech-ctl guard-volume

# Install and enable volume knob guard as a standalone systemd user service
fantastech-ctl setup-volume-guard --install
```

---

## 13. OpenRGB SDK Server

Exposes the keyboard as a standard OpenRGB SDK v3 controller on standard TCP port `6742`:

```bash
# Run standalone OpenRGB TCP SDK server on port 6742
fantastech-ctl openrgb-server --port 6742 --bind 127.0.0.1
```

Connect any standard OpenRGB client to `127.0.0.1:6742` to synchronize your entire desk setup!

---

## 14. Key Verification Testing

```bash
# Launch interactive matrix mapping and key verification test suite
fantastech-ctl test-keys
```

---

## 15. System Integration Setup

```bash
# Complete system installation (CLI binary, desktop launcher, icons, udev rules, service)
fantastech-ctl install

# Complete uninstallation from system
fantastech-ctl uninstall

# Or uninstall with full config purge
fantastech-ctl uninstall --purge

# Generate or install udev rules for non-root USB access
fantastech-ctl setup-udev --install

# Install KDE Plasma 6 desktop widget and application launcher
fantastech-ctl setup-kde
```

---

## Executable Aliases

`fantastech-ctl` symlinks directly to `fantastech` for quick command-line access.
