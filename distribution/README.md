# FantasTech Distribution & Installers

This directory contains the packaging and installer configurations for **FantasTech** across Linux and Windows.

In accordance with distribution standards, the core application payload is a **single unified executable** (`fantastech`), packaged using each operating system's native conventions.

---

## 🐧 Linux — Flatpak (Only)

Linux distribution is provided exclusively as a sandboxed Flatpak package (`org.fantastech.FantasTech`):

```bash
# Build Flatpak bundle into dist/
./distribution/flatpak/build-flatpak.sh

# Install Flatpak bundle locally
flatpak install --user -y dist/FantasTech-0.1.0.flatpak

# Run application
flatpak run org.fantastech.FantasTech

# Complete uninstallation & cleanup (prompts to purge user data)
./distribution/flatpak/uninstall-flatpak.sh
```

The manifest includes:
- `org.fantastech.FantasTech.yaml`: Manifest with display (Wayland/X11), localhost network (OpenRGB SDK), and hardware HID device permissions.
- `60-fantastech-mk894.rules`: Non-root udev rules for the MK894 normal PID (`0xFE9C`) and IAP bootloader PID (`0xFEAB`).
- `org.fantastech.FantasTech.desktop`: Desktop launcher entry.

---

## 🪟 Windows — Inno Setup Installer (.exe)

Produces a standard Windows installer `.exe` (`FantasTech-Setup-0.1.0.exe`):
- Installs to `%ProgramFiles%\FantasTech` or `%LocalAppData%\Programs\FantasTech`.
- Creates Start Menu entries and desktop shortcuts.
- Registers in Windows Settings (`Apps -> Installed apps` / Add or Remove Programs).
- Uninstaller automatically terminates any running app or service (`taskkill /F /IM fantastech.exe`), removes binaries and shortcuts, and interactively prompts the user whether to delete configurations and saved profiles (`%APPDATA%\FantasTech`).

**To compile with Inno Setup Compiler (`iscc`):**
```cmd
distribution\windows\build-installer.bat
```
