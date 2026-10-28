# Fantech MK894 Maxpower II — Safe Linux Driver & Protocol Suite

Clean-room reverse-engineered driver, protocol specification, CLI tool (`fantech-ctl`), OpenRGB SDK server, virtual emulator, and GUI for the **Fantech MK894 Maxpower II Mechanical Keyboard** (USB Vendor ID `0x36AE`, Product ID `0xFE9C`).

---

## 🔒 Clean-Room Safety & Architecture Guarantees

* **`#![forbid(unsafe_code)]`**: Every crate in the workspace strictly compiles under `#![forbid(unsafe_code)]`.
* **Zero Firmware Modification**: Operates purely over standard Linux USB HID (`/dev/hidraw`) interfaces without modifying original bootloaders or non-volatile firmware code.
* **Banned DFU / Bootloader Jumping**: Opcodes `0x5A` and `0x55` (which trigger internal ROM In-Application Programming bootloader execution) are statically and dynamically rejected by `fantech-protocol` to guarantee no accidental bricking.
* **Volatile & Reversible Lighting**: Per-key RGB colors and lighting modes use volatile SRAM buffers with differential caching (skipping unchanged keys for instantaneous updates).
* **Full Sparse Matrix Isolation**: Covers all 126 sparse slots (`0..125`) with dual-pad addressing (indices `95/96` for `/?`, `122/123` for `Num 0`, `110/111` for `Space`, `75/76` for `Enter`, `84/85` for `LeftShift`) to prevent ghost illumination.

---

## 📦 Workspace Crates

| Crate | Purpose |
| :--- | :--- |
| [`fantech-protocol`](fantech-rs/crates/fantech-protocol) | USB HID framing, 64-byte report serialization, opcode validation, and OpenRGB protocol serialization. |
| [`fantech-transport`](fantech-rs/crates/fantech-transport) | Linux `/dev/hidraw` transport with auto-discovery (Interface 2) and in-memory mock backend. |
| [`fantech-device`](fantech-rs/crates/fantech-device) | Type-state device handle (`ReadOnly`, `ReversibleAccess`, `PersistentAccess`), differential caching, and pacing. |
| [`fantech-core`](fantech-rs/crates/fantech-core) | 104-key physical keymap definitions, matrix coordinates, themes, JSON presets, and macro compiler. |
| [`fantech-emulator`](fantech-rs/crates/fantech-emulator) | Virtual keyboard hardware simulator with RAM state tracking for tests and CI without hardware. |
| [`fantech-cli`](fantech-rs/crates/fantech-cli) | User-facing command-line interface (`fantech-ctl`), OpenRGB SDK server, and Linux systemd/udev setup. |
| [`fantech-gui`](fantech-rs/crates/fantech-gui) | Lightweight desktop GUI application for interactive lighting and mapping. |
| [`openrgb-plugin`](openrgb-plugin) | Upstream C++ OpenRGB native controller and plugin implementation files. |

---

## ⚡ Linux Setup & Non-Root Permissions

### 1. One-Click udev Rule Installation
To run `fantech-ctl` and OpenRGB without needing `sudo`:
```bash
sudo ./fantech-rs/target/release/fantech-ctl setup-udev --install
```
This automatically writes `/etc/udev/rules.d/99-fantech-mk894.rules` and reloads `udevadm`.

### 2. Auto-Start OpenRGB Background Service (systemd)
To run the OpenRGB SDK server daemon automatically on login:
```bash
./fantech-rs/target/release/fantech-ctl setup-systemd --install
```

---

## 🌐 OpenRGB Integration (Two Ways)

### Option A: Built-in OpenRGB SDK Server (Zero Compilation Needed)
Start the built-in OpenRGB server daemon in `fantech-ctl`:
```bash
./fantech-ctl openrgb-server
```
Then in your OpenRGB desktop app, navigate to **SDK Client**, enter IP `127.0.0.1` and Port `6745`, and click **Connect**.
Or using OpenRGB CLI:
```bash
openrgb --client 127.0.0.1:6745 -d "Fantech MK894 Maxpower II" --mode "Rainbow Wave (Horizontal)" --direction right
```
The MK894 appears as a full 104-key mechanical matrix device with real-time per-key RGB control, static mode, all 17 hardware animation effects, speed, brightness, and direction controls!

### Option B: Upstream OpenRGB C++ Plugin
Drop the files in [`openrgb-plugin/`](openrgb-plugin/) directly into the OpenRGB controller source tree to build the driver natively into OpenRGB. See [`openrgb-plugin/README.md`](openrgb-plugin/README.md) for instructions.

---

## 🎨 KDE Plasma 6 Desktop Integration

The Fantech MK894 includes native support for KDE Plasma 6:

### 1. Automatic KDE Accent Color Sync
Match your keyboard backlighting automatically with your active KDE Plasma wallpaper and accent color:
```bash
# Apply active KDE Plasma accent color once
sudo ./fantech-rs/target/release/fantech-ctl kde-sync

# Watch for color changes in the background (updates keyboard live when you change KDE theme)
sudo ./fantech-rs/target/release/fantech-ctl kde-sync --watch
```

### 2. Desktop Widget (Plasmoid) for KDE Panel
Install the custom Plasma 6 desktop/panel widget:
```bash
./fantech-rs/target/release/fantech-ctl setup-kde --install
```
This installs:
- Panel Plasmoid widget to `~/.local/share/plasma/plasmoids/org.kde.fantech.lighting/`
- Application launcher entry to `~/.local/share/applications/fantech-ctl.desktop`

To add the widget to your panel:
1. Right-click your KDE Plasma panel or desktop.
2. Select **Add Widgets...**
3. Search for **Fantech MK894 Lighting** and drag it onto your panel!
4. You get 1-click preset switching (Gaming, Cyberpunk, KDE Accent Sync, Off), animation dropdown, and a live brightness slider directly in your taskbar!

---

## 🎮 CLI Usage (`fantech-ctl`)

### 1. View Hardware Lighting Modes (All 17 Modes)
```bash
./fantech-rs/target/release/fantech-ctl light list-modes
```
Outputs a full matrix table of all 17 modes (Static, Breathing, Wave, Ripple, Rainbow, Neon, Raindrop, Reactive, etc.) along with speed, brightness, and color capabilities.

### 2. Configure Lighting Animation Mode
```bash
# Static solid color with hex code or RGB
sudo ./fantech-rs/target/release/fantech-ctl light set-mode static --hex '#FF0055' --brightness 4

# Rainbow Wave with custom speed and direction
sudo ./fantech-rs/target/release/fantech-ctl light set-mode wave --speed 3 --direction right

# Breathing mode with custom green color
sudo ./fantech-rs/target/release/fantech-ctl light set-mode breathing --hex '#00FF55' --speed 2

# Audio reactive mode
sudo ./fantech-rs/target/release/fantech-ctl light set-mode audio --brightness 4
```

### 3. Custom Mode: Live Interactive Key Painter
Launch an interactive painter where keys update LIVE on your keyboard as you type:
```bash
sudo ./fantech-rs/target/release/fantech-ctl light custom --interactive
```
Interactive commands:
- `w red` -> lights up 'W' in red
- `wasd yellow` -> lights up WASD cluster in yellow
- `arrows cyan` -> lights up arrow keys in cyan
- `space #00FF55` -> lights up Spacebar in neon spring green
- `row 0 blue` -> lights up Function row in blue
- `save mypreset.json` -> saves the current layout to JSON
- `exit` -> keeps your custom lighting active and exits

### 4. Custom Mode: Batch Assignments & Presets
```bash
# Set specific keys in a single command
sudo ./fantech-rs/target/release/fantech-ctl light custom --set 'wasd=red,arrows=blue,space=green,esc=white'

# Save custom batch layout to a JSON file
sudo ./fantech-rs/target/release/fantech-ctl light custom --set 'wasd=red,arrows=blue' --save gaming_custom.json

# Load and apply preset from JSON file
sudo ./fantech-rs/target/release/fantech-ctl light custom --load gaming_custom.json
```

### 5. Built-in Theme Presets
```bash
# Gaming Preset: Red WASD, Yellow Arrows, Green Space/Enter, White Esc, Royal Blue background
sudo ./fantech-rs/target/release/fantech-ctl light preset gaming

# Cyberpunk Neon: Yellow WASD, Cyan Arrows/Space, Green Esc/Enter, Hot Magenta background
sudo ./fantech-rs/target/release/fantech-ctl light preset cyberpunk
```

### 6. Profile Management
```bash
# Query active hardware profile
sudo ./fantech-rs/target/release/fantech-ctl profile --current

# Switch profile (0..2)
sudo ./fantech-rs/target/release/fantech-ctl profile 0
```

### 7. Key Matrix Verification & Single-Key Testing
```bash
# Isolate key 'P' with solid red (all other 103 keys cleanly blacked out)
sudo ./fantech-rs/target/release/fantech-ctl test-keys --key p --color 255 0 0

# Probe Spacebar
sudo ./fantech-rs/target/release/fantech-ctl test-keys --key space --color 0 255 0

# Turn all LEDs off
sudo ./fantech-rs/target/release/fantech-ctl test-keys --all-off
```

---

## 🧪 Testing & Virtual Emulation

Run the entire test suite without physical hardware:
```bash
cargo test --workspace
```
Or test any command against the virtual simulated keyboard:
```bash
./fantech-rs/target/release/fantech-ctl --emulator info
./fantech-rs/target/release/fantech-ctl --emulator light set-mode wave --speed 3
./fantech-rs/target/release/fantech-ctl --emulator light custom --set 'wasd=red'
```
