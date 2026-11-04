#!/bin/bash
# Take Web Browser Installer
# Standalone installer with system detection and dependency auto-resolution.
set -euo pipefail

# --- Paths ---
INSTALL_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/takeweb"
CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/takeweb"
USER_DATA_DIR="$DATA_DIR/takeweb"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --- Color Definitions (ANSI 256 + truecolor) ---
C_PURPLE='\033[38;5;141m'
C_VIOLET='\033[38;5;135m'
C_SKY='\033[38;5;75m'
C_CYAN='\033[38;5;51m'
C_WHITE='\033[1;37m'
C_GREEN='\033[38;5;48m'
C_RED='\033[38;5;196m'
C_YELLOW='\033[38;5;226m'
C_DIM='\033[38;5;245m'
C_BOLD='\033[1m'
C_RESET='\033[0m'

# --- Helpers ---
info()    { echo -e "  ${C_GREEN}[✓]${C_RESET} $*"; }
warn()    { echo -e "  ${C_YELLOW}[!]${C_RESET} $*"; }
err()     { echo -e "  ${C_RED}[✗]${C_RESET} $*" >&2; }
item()    { echo -e "  ${C_PURPLE}•${C_RESET} $*"; }
section() { echo -e "\n${C_BOLD}${C_VIOLET}$*${C_RESET}"; }

# --- Check non-interactive argument ---
AUTO_CONFIRM=false
for arg in "$@"; do
    case "$arg" in
        -y|--yes)
            AUTO_CONFIRM=true
            ;;
    esac
done

# --- Print Colorful CLI Banner ---
print_banner() {
    echo
    echo -e "${C_PURPLE}  ████████╗ █████╗ ██╗  ██╗███████╗   ██╗    ██╗███████╗██████╗ ${C_RESET}"
    echo -e "${C_VIOLET}  ╚══██╔══╝██╔══██╗██║ ██╔╝██╔════╝   ██║    ██║██╔════╝██╔══██╗${C_RESET}"
    echo -e "${C_SKY}     ██║   ███████║█████╔╝ █████╗     ██║ █╗ ██║█████╗  ██████╔╝${C_RESET}"
    echo -e "${C_CYAN}     ██║   ██╔══██║██╔═██╗ ██╔══╝     ██║███╗██║██╔══╝  ██╔══██╗${C_RESET}"
    echo -e "${C_CYAN}     ██║   ██║  ██║██║  ██╗███████╗    ╚███╔███╔╝███████╗██████╔╝${C_RESET}"
    echo -e "${C_DIM}     ╚═╝   ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝     ╚══╝╚══╝ ╚══════╝╚═════╝ ${C_RESET}"
    echo -e "   ${C_BOLD}${C_WHITE}TakeWeb${C_RESET} ${C_DIM}— Next-Gen Native Linux Browser (Rust + GTK4 + WebKitGTK 6.0)${C_RESET}"
    echo -e "   ${C_GREEN}[Zero-Cloud Privacy]${C_RESET} ${C_SKY}[Bergamot WASM]${C_RESET} ${C_PURPLE}[Bubblewrap Sandbox]${C_RESET}"
    echo
}

# --- System Detection ---
detect_system() {
    DISTRO_NAME="Linux"
    DISTRO_ID="unknown"
    if [ -f /etc/os-release ]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        DISTRO_NAME="${PRETTY_NAME:-$NAME}"
        DISTRO_ID="${ID:-unknown}"
    fi

    DESKTOP_ENV="${XDG_CURRENT_DESKTOP:-${DESKTOP_SESSION:-Unknown}}"
    ARCH_NAME="$(uname -m)"

    PKG_MGR="unknown"
    if command -v pacman > /dev/null 2>&1; then
        PKG_MGR="pacman"
    elif command -v apt-get > /dev/null 2>&1; then
        PKG_MGR="apt"
    elif command -v dnf > /dev/null 2>&1; then
        PKG_MGR="dnf"
    elif command -v zypper > /dev/null 2>&1; then
        PKG_MGR="zypper"
    fi
}

print_system_info() {
    section "Host Environment:"
    item "Distribution : ${C_WHITE}${DISTRO_NAME}${C_RESET} (${ARCH_NAME})"
    item "Desktop Env  : ${C_WHITE}${DESKTOP_ENV}${C_RESET}"
    item "Pkg Manager  : ${C_WHITE}${PKG_MGR}${C_RESET}"
    item "Supported DE : ${C_GREEN}All Environments${C_RESET} (GNOME, COSMIC, KDE, Sway, Hyprland, XFCE, Cinnamon)"
}

# --- Dependencies Verification & Auto-Resolve ---
check_and_resolve_dependencies() {
    section "Checking Build & Runtime Dependencies:"

    MISSING_REQUIRED=()

    # 1. Rust / Cargo
    if command -v cargo > /dev/null 2>&1; then
        CARGO_VER="$(cargo --version 2>&1 | awk '{print $2}')"
        info "Rust / Cargo (${CARGO_VER}) found."
    else
        err "Rust / Cargo compiler not found."
        MISSING_REQUIRED+=("cargo")
    fi

    # 2. pkg-config
    if command -v pkg-config > /dev/null 2>&1; then
        info "pkg-config found."
    else
        err "pkg-config not found."
        MISSING_REQUIRED+=("pkg-config")
    fi

    # 3. GTK4
    if pkg-config --exists gtk4 2>/dev/null; then
        GTK_VER="$(pkg-config --modversion gtk4 2>/dev/null || echo '4.x')"
        info "GTK4 development headers (${GTK_VER}) found."
    else
        err "GTK4 development headers not found."
        MISSING_REQUIRED+=("gtk4")
    fi

    # 4. Libadwaita
    if pkg-config --exists libadwaita-1 2>/dev/null; then
        ADW_VER="$(pkg-config --modversion libadwaita-1 2>/dev/null || echo '1.x')"
        info "Libadwaita headers (${ADW_VER}) found."
    else
        err "Libadwaita headers (libadwaita-1) not found."
        MISSING_REQUIRED+=("adw")
    fi

    # 5. WebKitGTK 6.0
    if pkg-config --exists webkitgtk-6.0 2>/dev/null; then
        WEBKIT_VER="$(pkg-config --modversion webkitgtk-6.0 2>/dev/null || echo '6.0')"
        info "WebKitGTK 6.0 development headers (${WEBKIT_VER}) found."
    elif pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
        warn "WebKitGTK 6.0 not found, but webkit2gtk-4.1 detected."
        MISSING_REQUIRED+=("webkit6")
    else
        err "WebKitGTK 6.0 development headers not found."
        MISSING_REQUIRED+=("webkit6")
    fi

    # 6. SQLite3
    if pkg-config --exists sqlite3 2>/dev/null; then
        SQLITE_VER="$(pkg-config --modversion sqlite3 2>/dev/null || echo '3.x')"
        info "SQLite3 headers (${SQLITE_VER}) found."
    else
        warn "SQLite3 development headers not found via pkg-config."
        MISSING_REQUIRED+=("sqlite3")
    fi

    # 7. OpenSSL
    if pkg-config --exists openssl 2>/dev/null; then
        SSL_VER="$(pkg-config --modversion openssl 2>/dev/null || echo '3.x')"
        info "OpenSSL headers (${SSL_VER}) found."
    else
        warn "OpenSSL development headers not found."
        MISSING_REQUIRED+=("openssl")
    fi

    if [ ${#MISSING_REQUIRED[@]} -gt 0 ]; then
        echo
        err "Required system packages are missing."
        suggest_and_install_packages "${MISSING_REQUIRED[@]}"
    fi
}

get_distro_packages() {
    local target_pkg="$1"
    case "$PKG_MGR" in
        pacman)
            case "$target_pkg" in
                cargo)      echo "rust cargo" ;;
                pkg-config) echo "pkgconf" ;;
                gtk4)       echo "gtk4" ;;
                adw)        echo "libadwaita" ;;
                webkit6)    echo "webkitgtk-6.0" ;;
                sqlite3)    echo "sqlite" ;;
                openssl)    echo "openssl" ;;
            esac
            ;;
        apt)
            case "$target_pkg" in
                cargo)      echo "cargo rustc" ;;
                pkg-config) echo "pkg-config" ;;
                gtk4)       echo "libgtk-4-dev" ;;
                adw)        echo "libadwaita-1-dev" ;;
                webkit6)    echo "libwebkitgtk-6.0-dev" ;;
                sqlite3)    echo "libsqlite3-dev" ;;
                openssl)    echo "libssl-dev" ;;
            esac
            ;;
        dnf)
            case "$target_pkg" in
                cargo)      echo "cargo rust" ;;
                pkg-config) echo "pkgconf-pkg-config" ;;
                gtk4)       echo "gtk4-devel" ;;
                adw)        echo "libadwaita-devel" ;;
                webkit6)    echo "webkitgtk6.0-devel" ;;
                sqlite3)    echo "sqlite-devel" ;;
                openssl)    echo "openssl-devel" ;;
            esac
            ;;
        *)
            echo "$target_pkg"
            ;;
    esac
}

suggest_and_install_packages() {
    local missing_items=("$@")
    local install_list=""
    for m in "${missing_items[@]}"; do
        pkgs="$(get_distro_packages "$m")"
        install_list="$install_list $pkgs"
    done
    install_list="$(echo "$install_list" | xargs)"

    echo -e "\n  ${C_BOLD}Required package command:${C_RESET}"
    case "$PKG_MGR" in
        pacman) echo "    sudo pacman -S --needed $install_list" ;;
        apt)    echo "    sudo apt install -y $install_list" ;;
        dnf)    echo "    sudo dnf install -y $install_list" ;;
        *)      echo "    Install: $install_list" ;;
    esac
    echo

    if [ "$PKG_MGR" != "unknown" ]; then
        if [ "$AUTO_CONFIRM" = true ]; then
            answer="y"
        else
            read -rp "  Install missing dependencies automatically via sudo? [y/N]: " answer
        fi
        case "$answer" in
            [yY][eE][sS]|[yY])
                echo -e "  ${C_SKY}Running package manager...${C_RESET}"
                case "$PKG_MGR" in
                    pacman) sudo pacman -S --needed --noconfirm $install_list ;;
                    apt)    sudo apt-get update && sudo apt-get install -y $install_list ;;
                    dnf)    sudo dnf install -y $install_list ;;
                esac
                echo -e "  ${C_GREEN}Dependencies installed. Continuing installation...${C_RESET}"
                return
                ;;
        esac
    fi

    err "Required dependencies are not satisfied. Please install them and re-run install.sh."
    exit 1
}

# --- Show Target Paths ---
show_target_plan() {
    section "Installation Target Layout:"
    echo -e "  ${C_DIM}┌───────────────────────────────────────────────────────────────────────────┐${C_RESET}"
    echo -e "  ${C_DIM}│${C_RESET}  ${C_BOLD}Binary Executable${C_RESET} : ${C_WHITE}$INSTALL_DIR/takeweb${C_RESET}"
    echo -e "  ${C_DIM}│${C_RESET}  ${C_BOLD}Desktop Entry${C_RESET}    : ${C_WHITE}$DESKTOP_DIR/takeweb.desktop${C_RESET}"
    echo -e "  ${C_DIM}│${C_RESET}  ${C_BOLD}Application Icons${C_RESET}: ${C_WHITE}$ICON_DIR/scalable/apps/takeweb.svg${C_RESET}"
    echo -e "  ${C_DIM}│${C_RESET}  ${C_BOLD}Configuration${C_RESET}    : ${C_WHITE}$CONFIG_DIR${C_RESET}"
    echo -e "  ${C_DIM}│${C_RESET}  ${C_BOLD}SQLite Storage${C_RESET}   : ${C_WHITE}$USER_DATA_DIR/browser.db${C_RESET}"
    echo -e "  ${C_DIM}└───────────────────────────────────────────────────────────────────────────┘${C_RESET}"
    echo

    if [ "$AUTO_CONFIRM" = false ]; then
        read -rp "  Proceed with installation? [Y/n]: " answer
        case "$answer" in
            [nN][oO]|[nN])
                echo "Installation cancelled by user."
                exit 0
                ;;
        esac
    fi
}

# --- Installation Steps ---
perform_installation() {
    section "Building and Installing Take Web:"

    mkdir -p "$INSTALL_DIR" "$DESKTOP_DIR" "$CONFIG_DIR" "$CACHE_DIR" "$USER_DATA_DIR"
    info "Target directories initialized."

    # 2. Release compilation
    RELEASE_BIN="$SCRIPT_DIR/target/release/takeweb"
    if [ ! -f "$RELEASE_BIN" ]; then
        echo -e "  ${C_SKY}Compiling release binary with optimizations (cargo build --release)...${C_RESET}"
        cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"
    fi

    if [ ! -f "$RELEASE_BIN" ]; then
        err "Compilation failed: $RELEASE_BIN not found."
        exit 1
    fi
    info "Release binary built successfully."

    # 3. Deploy binary
    install -m 755 "$RELEASE_BIN" "$INSTALL_DIR/takeweb"
    info "Binary installed to $INSTALL_DIR/takeweb"

    # 4. Deploy icons (raster resolutions 16..512 + scalable SVG)
    for size in 16 22 24 32 48 64 96 128 256 512; do
        s_dir="$ICON_DIR/${size}x${size}/apps"
        mkdir -p "$s_dir"
        if [ -f "$SCRIPT_DIR/resources/icons/takeweb_${size}.png" ]; then
            cp -f "$SCRIPT_DIR/resources/icons/takeweb_${size}.png" "$s_dir/takeweb.png"
            cp -f "$SCRIPT_DIR/resources/icons/takeweb_${size}.png" "$s_dir/io.github.dezaetterg.TakeWeb.png"
        elif [ -f "$SCRIPT_DIR/resources/icons/takeweb.png" ]; then
            cp -f "$SCRIPT_DIR/resources/icons/takeweb.png" "$s_dir/takeweb.png"
            cp -f "$SCRIPT_DIR/resources/icons/takeweb.png" "$s_dir/io.github.dezaetterg.TakeWeb.png"
        fi
    done

    mkdir -p "$ICON_DIR/scalable/apps"
    if [ -f "$SCRIPT_DIR/resources/icons/takeweb.svg" ]; then
        cp -f "$SCRIPT_DIR/resources/icons/takeweb.svg" "$ICON_DIR/scalable/apps/takeweb.svg"
        cp -f "$SCRIPT_DIR/resources/icons/takeweb.svg" "$ICON_DIR/scalable/apps/io.github.dezaetterg.TakeWeb.svg"
        # Support MacTahoe custom desktop theme if installed
        for theme in MacTahoe MacTahoe-light MacTahoe-dark; do
            if [ -d "$DATA_DIR/icons/${theme}/apps/scalable" ]; then
                cp -f "$SCRIPT_DIR/resources/icons/takeweb.svg" "$DATA_DIR/icons/${theme}/apps/scalable/takeweb.svg"
            fi
        done
    fi
    info "Application icons registered across all sizes."

    # 4b. Deploy TakeWeb built-in icon theme (guarantees crisp GNOME-style icons on any DE/WM)
    mkdir -p "$USER_DATA_DIR/icons" "$DATA_DIR/icons"
    cp -rf "$SCRIPT_DIR/resources/icons/TakeWeb" "$USER_DATA_DIR/icons/"
    cp -rf "$SCRIPT_DIR/resources/icons/TakeWeb" "$DATA_DIR/icons/"
    info "TakeWeb built-in symbolic icon theme deployed."

    # 5. Desktop entry
    if [ -f "$SCRIPT_DIR/takeweb.desktop" ]; then
        cp -f "$SCRIPT_DIR/takeweb.desktop" "$DESKTOP_DIR/takeweb.desktop"
        cp -f "$SCRIPT_DIR/takeweb.desktop" "$DESKTOP_DIR/io.github.dezaetterg.TakeWeb.desktop"
        info "Desktop application shortcut registered."
    fi

    USER_DESKTOP="$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Рабочий стол")"
    if [ ! -d "$USER_DESKTOP" ] && [ -d "$HOME/Desktop" ]; then
        USER_DESKTOP="$HOME/Desktop"
    fi

    if [ -d "$USER_DESKTOP" ] && [ -f "$SCRIPT_DIR/takeweb.desktop" ]; then
        cp -f "$SCRIPT_DIR/takeweb.desktop" "$USER_DESKTOP/takeweb.desktop"
        chmod +x "$USER_DESKTOP/takeweb.desktop"
        if command -v gio > /dev/null 2>&1; then
            gio set "$USER_DESKTOP/takeweb.desktop" metadata::trusted true 2>/dev/null || true
        fi
        info "Desktop shortcut placed on $USER_DESKTOP"
    fi

    # 6. Update system caches
    if command -v update-desktop-database > /dev/null 2>&1; then
        update-desktop-database "$DESKTOP_DIR" 2>/dev/null && info "Desktop database updated." || true
    fi
    if command -v gtk-update-icon-cache > /dev/null 2>&1; then
        gtk-update-icon-cache -f -t "$ICON_DIR" 2>/dev/null && info "Icon cache updated." || true
    fi
}

# --- Summary Card ---
print_summary() {
    echo
    echo -e "  ${C_PURPLE}${C_BOLD}========================================================================${C_RESET}"
    echo -e "  ${C_WHITE}${C_BOLD}                    Take Web successfully installed!                    ${C_RESET}"
    echo -e "  ${C_PURPLE}------------------------------------------------------------------------${C_RESET}"
    echo -e "  ${C_BOLD}Terminal Command${C_RESET} : ${C_SKY}takeweb${C_RESET}"
    echo -e "  ${C_BOLD}Application Menu${C_RESET} : Search for ${C_WHITE}\"Take Web\"${C_RESET} in your application launcher"
    echo -e "  ${C_BOLD}Active Runtime  ${C_RESET} : Rust 64-bit | GTK 4 | Libadwaita 1.5+ | WebKitGTK 6.0"
    echo -e "  ${C_BOLD}Storage Engine  ${C_RESET} : SQLite 3 with WAL & FTS5 full-text omnibox index"
    echo -e "  ${C_BOLD}Sandboxing      ${C_RESET} : ${C_GREEN}Bubblewrap Hardware Isolation Enabled${C_RESET}"
    echo -e "  ${C_PURPLE}========================================================================${C_RESET}"

    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *)
            echo
            warn "'$INSTALL_DIR' is not in your current PATH."
            echo -e "      To run '${C_WHITE}takeweb${C_RESET}' directly from any terminal, add this to ${C_WHITE}~/.bashrc${C_RESET} or ${C_WHITE}~/.zshrc${C_RESET}:"
            echo -e "      ${C_SKY}export PATH=\"\$HOME/.local/bin:\$PATH\"${C_RESET}"
            ;;
    esac
    echo
}

main() {
    print_banner
    detect_system
    print_system_info
    check_and_resolve_dependencies
    show_target_plan
    perform_installation
    print_summary
}

main "$@"
