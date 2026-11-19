#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "Installing FantasTech desktop entries and icons..."

mkdir -p "${HOME}/.local/share/applications"
mkdir -p "${HOME}/.local/bin"
mkdir -p "${HOME}/.local/share/icons/hicolor/128x128/apps"
mkdir -p "${HOME}/.local/share/icons/hicolor/64x64/apps"

cp "${SCRIPT_DIR}/apps/FantasTech.desktop" "${HOME}/.local/share/applications/"

cp "${SCRIPT_DIR}/apps/icons/Yarny_128.png" "${HOME}/.local/share/icons/hicolor/128x128/apps/fantastech.png"
cp "${SCRIPT_DIR}/apps/icons/Yarny_64.png" "${HOME}/.local/share/icons/hicolor/64x64/apps/fantastech.png"

ln -sf "${SCRIPT_DIR}/fantastech" "${HOME}/.local/bin/fantastech"

echo "FantasTech desktop integration installed successfully!"
