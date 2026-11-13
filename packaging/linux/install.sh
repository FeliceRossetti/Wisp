#!/bin/sh
# Install Wisp for the current user (no root needed).
# Run from the extracted release folder:  ./install.sh
# Remove again with:                      ./install.sh --uninstall
set -eu

BIN="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
ICON="$DATA/icons/hicolor/512x512/apps/wisp.png"
DESKTOP="$DATA/applications/wisp.desktop"
# Written by Wisp when "Open Wisp when you sign in" is on.
AUTOSTART="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/wisp.desktop"

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BIN/wisp" "$ICON" "$DESKTOP" "$AUTOSTART"
    echo "Wisp removed. Settings stay in ~/.config/wisp."
    exit 0
fi

here="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$BIN" "$(dirname "$ICON")" "$(dirname "$DESKTOP")"
install -m 755 "$here/wisp" "$BIN/wisp"
install -m 644 "$here/wisp.png" "$ICON"
sed "s|^Exec=.*|Exec=$BIN/wisp|" "$here/wisp.desktop" > "$DESKTOP"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DATA/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$DATA/icons/hicolor" || true
echo "Wisp installed. Find it in your app launcher, or run: $BIN/wisp"
