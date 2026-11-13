#!/usr/bin/env bash
# ==============================================================================
# FantasTech — Flatpak Build & Installation Helper
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

MANIFEST="${SCRIPT_DIR}/org.fantastech.FantasTech.yaml"
BUILD_DIR="${REPO_ROOT}/dist/flatpak-build"
REPO_DIR="${REPO_ROOT}/dist/flatpak-repo"
BUNDLE_OUTPUT="${REPO_ROOT}/dist/FantasTech-0.1.0.flatpak"

echo "===================================================================="
echo "📦 Building FantasTech Flatpak Package"
echo "===================================================================="

# 1. Compile release binary on host first (fast, uses local crate cache)
echo "⚡ Compiling release binary on host with Cargo..."
(cd "${REPO_ROOT}/core" && cargo build --release --bin fantastech)

# 2. Check for flatpak-builder
if ! command -v flatpak-builder >/dev/null 2>&1; then
    echo "❌ 'flatpak-builder' is not installed."
    echo "To install on Arch/CachyOS: sudo pacman -S flatpak-builder"
    echo "To install on Ubuntu/Debian: sudo apt install flatpak-builder"
    echo "To install on Fedora:        sudo dnf install flatpak-builder"
    exit 1
fi

# 3. Check for Flathub remote and install required runtime if missing
echo "🔍 Checking FreeDesktop 26.08 Runtime..."
if ! (flatpak info --user org.freedesktop.Platform//26.08 >/dev/null 2>&1 || flatpak info org.freedesktop.Platform//26.08 >/dev/null 2>&1); then
    echo "📥 Installing FreeDesktop 26.08 Platform..."
    flatpak install --user -y flathub org.freedesktop.Platform//26.08 || flatpak install -y flathub org.freedesktop.Platform//26.08 || true
fi

# 4. Build and install to user environment (offline, completely self-contained)
echo "⚙️  Packaging Flatpak sandbox..."
mkdir -p "${REPO_ROOT}/dist"
flatpak-builder --user --install --force-clean --disable-rofiles-fuse \
    --repo="${REPO_DIR}" \
    "${BUILD_DIR}" \
    "${MANIFEST}"

# 5. Create standalone .flatpak single-file bundle
echo "📦 Exporting standalone single-file bundle to ${BUNDLE_OUTPUT}..."
flatpak build-bundle "${REPO_DIR}" "${BUNDLE_OUTPUT}" org.fantastech.FantasTech stable

# 6. Clean up temporary staging directories and flatpak-builder cache to save disk space
echo "🧹 Cleaning up staging build directories..."
rm -rf "${BUILD_DIR}" "${REPO_DIR}" "${REPO_ROOT}/.flatpak-builder"

echo "===================================================================="
echo "🎉 Flatpak build and installation complete!"
echo "• Run application:     flatpak run org.fantastech.FantasTech"
echo "• Standalone bundle:   ${BUNDLE_OUTPUT}"
echo "• Complete uninstall:  ${SCRIPT_DIR}/uninstall-flatpak.sh"
echo "===================================================================="
