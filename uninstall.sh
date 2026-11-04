#!/bin/bash
# Take Web Uninstaller
set -euo pipefail

INSTALL_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/takeweb"
CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/takeweb"
USER_DATA_DIR="$DATA_DIR/takeweb"

BOLD='\033[1m'
RED='\033[0;31m'
GREEN='\033[0;32m'
PURPLE='\033[38;5;141m'
NC='\033[0m'

PURGE=false
AUTO_CONFIRM=false
for arg in "$@"; do
    case "$arg" in
        --purge|-p) PURGE=true ;;
        -y|--yes)   AUTO_CONFIRM=true ;;
    esac
done

echo -e "${BOLD}${PURPLE}Take Web Uninstaller${NC}"
echo
echo "The following components will be removed:"
echo "  $INSTALL_DIR/takeweb"
echo "  $DESKTOP_DIR/takeweb.desktop"
echo "  $DESKTOP_DIR/io.github.dezaetterg.TakeWeb.desktop"
echo "  $ICON_DIR/*/apps/takeweb.*"
echo

USER_DESKTOP="$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Рабочий стол")"
if [ ! -d "$USER_DESKTOP" ] && [ -d "$HOME/Desktop" ]; then
    USER_DESKTOP="$HOME/Desktop"
fi
if [ -f "$USER_DESKTOP/takeweb.desktop" ]; then
    echo "  $USER_DESKTOP/takeweb.desktop"
fi

if [ "$PURGE" = true ]; then
    echo -e "${RED}PURGE MODE: User database and cache will also be PERMANENTLY REMOVED:${NC}"
    echo "  $USER_DATA_DIR"
    echo "  $CONFIG_DIR"
    echo "  $CACHE_DIR"
else
    echo "The following will be KEPT (your bookmarks, history and settings):"
    echo "  $USER_DATA_DIR"
    echo "  $CONFIG_DIR"
    echo "  $CACHE_DIR"
fi
echo

if [ "$AUTO_CONFIRM" = false ]; then
    read -rp "Proceed with uninstall? [y/N] " answer
    case "$answer" in
        [yY][eE][sS]|[yY]) ;;
        *)
            echo "Uninstall cancelled."
            exit 0
            ;;
    esac
fi

echo "Removing binary and desktop entries..."
rm -f "$INSTALL_DIR/takeweb"
rm -f "$DESKTOP_DIR/takeweb.desktop" "$DESKTOP_DIR/io.github.dezaetterg.TakeWeb.desktop"
rm -f "$USER_DESKTOP/takeweb.desktop" 2>/dev/null || true

echo "Removing installed icons..."
rm -f "$ICON_DIR/scalable/apps/takeweb.svg" "$ICON_DIR/scalable/apps/io.github.dezaetterg.TakeWeb.svg"
for size in 16 22 24 32 48 64 96 128 256 512; do
    rm -f "$ICON_DIR/${size}x${size}/apps/takeweb.png"
    rm -f "$ICON_DIR/${size}x${size}/apps/io.github.dezaetterg.TakeWeb.png"
done

if [ "$PURGE" = true ]; then
    echo "Purging configuration and storage..."
    rm -rf "$USER_DATA_DIR" "$CONFIG_DIR" "$CACHE_DIR"
fi

if command -v update-desktop-database > /dev/null 2>&1; then
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache > /dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$ICON_DIR" 2>/dev/null || true
fi

echo
echo -e "${GREEN}Take Web has been uninstalled.${NC}"
if [ "$PURGE" = false ]; then
    echo "Your user profile and bookmarks are preserved at: $USER_DATA_DIR"
    echo -e "To remove them, run: ${RED}./uninstall.sh --purge${NC}"
fi
echo
