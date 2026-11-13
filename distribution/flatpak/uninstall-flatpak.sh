#!/usr/bin/env bash
# ==============================================================================
# FantasTech — Complete Flatpak Uninstallation & System Cleanup Helper
# ==============================================================================
set -euo pipefail

echo "===================================================================="
echo "🛑 Uninstalling FantasTech Flatpak & Cleaning State"
echo "===================================================================="

# 1. Stop any running background service or tray daemon processes
echo "1. Terminating any active FantasTech processes..."
pkill -f "fantastech" 2>/dev/null || true
sleep 0.5

# 2. Check user preference for deleting configuration and saved profiles
PURGE_DATA=false
if [[ "${1:-}" == "--delete-data" || "${1:-}" == "--purge" || "${1:-}" == "-y" ]]; then
    PURGE_DATA=true
else
    read -r -p "❓ Do you want to delete all saved profiles and configuration files? [y/N]: " response || true
    if [[ "${response:-}" =~ ^([yY][eE][sS]|[yY])$ ]]; then
        PURGE_DATA=true
    fi
fi

# 3. Uninstall Flatpak application
echo "2. Uninstalling Flatpak application..."
if [ "$PURGE_DATA" = true ]; then
    flatpak uninstall --user -y --delete-data org.fantastech.FantasTech 2>/dev/null || true
    flatpak uninstall --system -y --delete-data org.fantastech.FantasTech 2>/dev/null || true
    flatpak uninstall --user -y --delete-data io.github.fantastech.FantasTech 2>/dev/null || true
    flatpak uninstall --system -y --delete-data io.github.fantastech.FantasTech 2>/dev/null || true
    rm -rf "${HOME}/.var/app/org.fantastech.FantasTech"
    rm -rf "${HOME}/.var/app/io.github.fantastech.FantasTech"
    rm -rf "${HOME}/.config/fantastech"
    echo "   ✅ Configurations and saved profiles purged."
else
    flatpak uninstall --user -y org.fantastech.FantasTech 2>/dev/null || true
    flatpak uninstall --system -y org.fantastech.FantasTech 2>/dev/null || true
    flatpak uninstall --user -y io.github.fantastech.FantasTech 2>/dev/null || true
    flatpak uninstall --system -y io.github.fantastech.FantasTech 2>/dev/null || true
    echo "   ℹ️  Saved profiles preserved in ~/.var/app/org.fantastech.FantasTech."
fi

# 4. Remove runtime sockets and autostart shortcuts
echo "3. Removing runtime sockets and desktop entries..."
rm -f "${XDG_RUNTIME_DIR:-/tmp}/fantastech"* 2>/dev/null || true
rm -f /tmp/fantastech* 2>/dev/null || true
rm -f "${HOME}/.config/autostart/org.fantastech.FantasTech.desktop" 2>/dev/null || true
rm -f "${HOME}/.config/autostart/io.github.fantastech.FantasTech.desktop" 2>/dev/null || true
rm -f "${HOME}/.local/share/applications/org.fantastech.FantasTech.desktop" 2>/dev/null || true
rm -f "${HOME}/.local/share/applications/io.github.fantastech.FantasTech.desktop" 2>/dev/null || true
rm -f "${HOME}/.config/systemd/user/fantastech.service" 2>/dev/null || true

if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload 2>/dev/null || true
fi

# 5. Check for system-level udev rules
if [[ -f /etc/udev/rules.d/99-fantastech.rules || -f /etc/udev/rules.d/70-fantech-mk894.rules || -f /etc/udev/rules.d/60-fantastech-mk894.rules ]]; then
    echo ""
    echo "ℹ️  System hardware udev rules remain in /etc/udev/rules.d/."
    echo "   To remove them, run:"
    echo "   sudo rm -f /etc/udev/rules.d/*fantastech*.rules /etc/udev/rules.d/*fantech*.rules && sudo udevadm control --reload"
fi

echo "===================================================================="
echo "🎉 FantasTech Flatpak uninstallation and cleanup complete!"
echo "===================================================================="
