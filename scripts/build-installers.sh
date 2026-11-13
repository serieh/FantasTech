#!/usr/bin/env bash
# ==============================================================================
# FantasTech — Master Installer & Distribution Compilation Script
# ==============================================================================
# Compiles the single unified application executable and packages the
# native installers for each target platform into the dist/ directory.
#
# Usage:
#   ./scripts/build-installers.sh          # Auto-detect host OS and build its installer
#   ./scripts/build-installers.sh linux    # Build Linux Flatpak bundle (only)
#   ./scripts/build-installers.sh windows  # Package Windows Inno Setup installer
#   ./scripts/build-installers.sh all      # Build all available platform packages
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_DIR="${REPO_ROOT}/dist"

TARGET="${1:-auto}"

BLUE='\033[0;34m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${BLUE}====================================================================${NC}"
echo -e "${BLUE}🚀 FantasTech — Installer & Packaging Builder${NC}"
echo -e "${BLUE}====================================================================${NC}"

mkdir -p "${DIST_DIR}"

build_linux() {
    echo -e "\n${BLUE}🐧 Building Linux Flatpak...${NC}"
    "${REPO_ROOT}/distribution/flatpak/build-flatpak.sh"
}

build_windows() {
    echo -e "\n${BLUE}🪟 Building Windows Installer...${NC}"
    if command -v iscc >/dev/null 2>&1; then
        (cd "${REPO_ROOT}" && iscc distribution/windows/installer.iss)
    else
        echo -e "${YELLOW}ℹ️  Inno Setup compiler ('iscc') not found in PATH.${NC}"
        echo -e "   The installer descriptor is ready at: distribution/windows/installer.iss"
        echo -e "   On Windows, run: 'iscc distribution\\windows\\installer.iss' or 'distribution\\windows\\build-installer.bat'"
    fi
}

case "${TARGET}" in
    linux)
        build_linux
        ;;
    windows)
        build_windows
        ;;
    all)
        build_linux
        build_windows
        ;;
    auto)
        case "$(uname -s)" in
            Linux*)  build_linux ;;
            MINGW*|MSYS*|CYGWIN*) build_windows ;;
            *) build_linux ;;
        esac
        ;;
    *)
        echo "Unknown target: ${TARGET}. Valid options: linux, windows, all, auto."
        exit 1
        ;;
esac

echo -e "\n${BLUE}====================================================================${NC}"
echo -e "${GREEN}🎉 Installer generation complete! Output files in dist/:${NC}"
echo -e "${BLUE}====================================================================${NC}"
ls -lh "${DIST_DIR}"/*.flatpak "${DIST_DIR}"/*.exe 2>/dev/null || ls -lh "${DIST_DIR}"
echo -e "${BLUE}====================================================================${NC}"
