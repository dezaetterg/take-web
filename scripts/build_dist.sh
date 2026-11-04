#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"
VERSION="0.1.0"
APP_NAME="takeweb"
BIN_SRC="${ROOT_DIR}/target/release/takeweb"

if [ ! -f "${BIN_SRC}" ]; then
    echo "Error: ${BIN_SRC} does not exist. Run 'cargo build --release' first." >&2
    exit 1
fi

echo "=== Packaging TakeWeb v${VERSION} for multiple distributions ==="
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

# Update ~/.local/bin/takeweb for local testing and desktop entry execution
mkdir -p "${HOME}/.local/bin"
install -m 755 "${BIN_SRC}" "${HOME}/.local/bin/takeweb"
echo "-> Updated ${HOME}/.local/bin/takeweb"

# ---------------------------------------------------------------------------
# 1. Debian / Ubuntu package (.deb)
# ---------------------------------------------------------------------------
echo "-> Building Debian package (.deb)..."
DEB_DIR=$(mktemp -d /tmp/takeweb-deb-XXXXXX)
mkdir -p "${DEB_DIR}/DEBIAN"
mkdir -p "${DEB_DIR}/usr/bin"
mkdir -p "${DEB_DIR}/usr/share/applications"
mkdir -p "${DEB_DIR}/usr/share/pixmaps"

# Control file
cat <<EOF > "${DEB_DIR}/DEBIAN/control"
Package: takeweb
Version: ${VERSION}
Section: web
Priority: optional
Architecture: amd64
Depends: libgtk-4-1 (>= 4.12.0), libadwaita-1-0 (>= 1.5.0), libwebkitgtk-6.0-4 (>= 2.52.0), libsoup-3.0-0, libsqlite3-0
Maintainer: TakeWeb Team <contact@takeweb.org>
Description: TakeWeb - Privacy-first, zero-cloud lightweight web browser
 TakeWeb is a modern Linux browser built with Rust, GTK4, Libadwaita, and WebKitGTK.
 Features local offline translations via Bergamot WASM, built-in tracker shield,
 customizable search & start page, and tab management.
EOF

install -m 755 "${BIN_SRC}" "${DEB_DIR}/usr/bin/takeweb"

# System desktop file
cat <<EOF > "${DEB_DIR}/usr/share/applications/io.github.dezaetterg.TakeWeb.desktop"
[Desktop Entry]
Name=Take Web
GenericName=Web Browser
GenericName[ru]=Веб-браузер
Comment=Privacy-first, zero-cloud web browser
Comment[ru]=Конфиденциальный браузер без облачной телеметрии
Exec=takeweb %U
Icon=takeweb
Terminal=false
Type=Application
Categories=Network;WebBrowser;
MimeType=text/html;text/xml;application/xhtml_xml;x-scheme-handler/http;x-scheme-handler/https;
StartupNotify=true
StartupWMClass=takeweb
EOF

# Icons
for size in 16 22 24 32 48 64 96 128 256 512; do
    icon_src="${ROOT_DIR}/resources/icons/takeweb_${size}.png"
    if [ -f "${icon_src}" ]; then
        icon_dest="${DEB_DIR}/usr/share/icons/hicolor/${size}x${size}/apps"
        mkdir -p "${icon_dest}"
        install -m 644 "${icon_src}" "${icon_dest}/takeweb.png"
    fi
done
install -m 644 "${ROOT_DIR}/resources/icons/takeweb_128.png" "${DEB_DIR}/usr/share/pixmaps/takeweb.png"

# TakeWeb built-in symbolic icon theme (ensures identical GNOME-style icons across all DE/WM)
mkdir -p "${DEB_DIR}/usr/share/takeweb/icons" "${DEB_DIR}/usr/share/icons"
cp -rf "${ROOT_DIR}/resources/icons/TakeWeb" "${DEB_DIR}/usr/share/takeweb/icons/"
cp -rf "${ROOT_DIR}/resources/icons/TakeWeb" "${DEB_DIR}/usr/share/icons/"

dpkg-deb --build --root-owner-group "${DEB_DIR}" "${DIST_DIR}/takeweb_${VERSION}_amd64.deb"
rm -rf "${DEB_DIR}"
echo "   Created: dist/takeweb_${VERSION}_amd64.deb"

# ---------------------------------------------------------------------------
# 2. Universal Linux AppImage (.AppImage)
# ---------------------------------------------------------------------------
echo "-> Building AppImage..."
APPDIR=$(mktemp -d /tmp/takeweb-appdir-XXXXXX)
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/scalable/apps"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/128x128/apps"

install -m 755 "${BIN_SRC}" "${APPDIR}/usr/bin/takeweb"
install -m 644 "${ROOT_DIR}/resources/icons/takeweb.svg" "${APPDIR}/usr/share/icons/hicolor/scalable/apps/takeweb.svg"
install -m 644 "${ROOT_DIR}/resources/icons/takeweb_128.png" "${APPDIR}/usr/share/icons/hicolor/128x128/apps/takeweb.png"
install -m 644 "${ROOT_DIR}/resources/icons/takeweb_128.png" "${APPDIR}/takeweb.png"
install -m 644 "${ROOT_DIR}/resources/icons/takeweb.svg" "${APPDIR}/.DirIcon"

# TakeWeb built-in symbolic icon theme (ensures identical GNOME-style icons across all DE/WM)
mkdir -p "${APPDIR}/usr/share/takeweb/icons" "${APPDIR}/usr/share/icons"
cp -rf "${ROOT_DIR}/resources/icons/TakeWeb" "${APPDIR}/usr/share/takeweb/icons/"
cp -rf "${ROOT_DIR}/resources/icons/TakeWeb" "${APPDIR}/usr/share/icons/"

cat <<EOF > "${APPDIR}/takeweb.desktop"
[Desktop Entry]
Name=Take Web
GenericName=Web Browser
GenericName[ru]=Веб-браузер
Comment=Privacy-first, zero-cloud web browser
Comment[ru]=Конфиденциальный браузер без облачной телеметрии
Exec=takeweb %U
Icon=takeweb
Terminal=false
Type=Application
Categories=Network;WebBrowser;
MimeType=text/html;text/xml;application/xhtml_xml;x-scheme-handler/http;x-scheme-handler/https;
StartupNotify=true
StartupWMClass=takeweb
EOF

# AppRun launcher script
cat <<'EOF' > "${APPDIR}/AppRun"
#!/bin/sh
SELF=$(readlink -f "$0")
HERE=${SELF%/*}
export PATH="${HERE}/usr/bin:${PATH}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
exec "${HERE}/usr/bin/takeweb" "$@"
EOF
chmod +x "${APPDIR}/AppRun"

APPIMAGETOOL="${HOME}/.local/bin/appimagetool"
if [ ! -x "${APPIMAGETOOL}" ]; then
    APPIMAGETOOL="$(which appimagetool 2>/dev/null || true)"
fi

if [ -n "${APPIMAGETOOL}" ] && [ -x "${APPIMAGETOOL}" ]; then
    ARCH=x86_64 "${APPIMAGETOOL}" --appimage-extract-and-run "${APPDIR}" "${DIST_DIR}/TakeWeb-${VERSION}-x86_64.AppImage" 2>&1 | tail -n 5 || \
    ARCH=x86_64 "${APPIMAGETOOL}" "${APPDIR}" "${DIST_DIR}/TakeWeb-${VERSION}-x86_64.AppImage"
    echo "   Created: dist/TakeWeb-${VERSION}-x86_64.AppImage"
else
    echo "   Warning: appimagetool not found, skipping AppImage."
fi
rm -rf "${APPDIR}"

# ---------------------------------------------------------------------------
# 3. Universal Portable tar.gz Bundle
# ---------------------------------------------------------------------------
echo "-> Building Portable tar.gz bundle..."
PORTABLE_DIR=$(mktemp -d /tmp/takeweb-portable-XXXXXX)
PKG_NAME="takeweb-${VERSION}-linux-x86_64"
TARGET_DIR="${PORTABLE_DIR}/${PKG_NAME}"
mkdir -p "${TARGET_DIR}/bin"
mkdir -p "${TARGET_DIR}/share/applications"
mkdir -p "${TARGET_DIR}/share/icons/hicolor"

install -m 755 "${BIN_SRC}" "${TARGET_DIR}/bin/takeweb"
install -m 644 "${ROOT_DIR}/takeweb.desktop" "${TARGET_DIR}/share/applications/takeweb.desktop"
install -m 755 "${ROOT_DIR}/install.sh" "${TARGET_DIR}/install.sh"
install -m 755 "${ROOT_DIR}/uninstall.sh" "${TARGET_DIR}/uninstall.sh"

for size in 16 22 24 32 48 64 96 128 256 512; do
    icon_src="${ROOT_DIR}/resources/icons/takeweb_${size}.png"
    if [ -f "${icon_src}" ]; then
        dest="${TARGET_DIR}/share/icons/hicolor/${size}x${size}/apps"
        mkdir -p "${dest}"
        install -m 644 "${icon_src}" "${dest}/takeweb.png"
    fi
done
mkdir -p "${TARGET_DIR}/share/icons/hicolor/scalable/apps"
install -m 644 "${ROOT_DIR}/resources/icons/takeweb.svg" "${TARGET_DIR}/share/icons/hicolor/scalable/apps/takeweb.svg"

# TakeWeb built-in symbolic icon theme
mkdir -p "${TARGET_DIR}/share/takeweb/icons" "${TARGET_DIR}/share/icons"
cp -rf "${ROOT_DIR}/resources/icons/TakeWeb" "${TARGET_DIR}/share/takeweb/icons/"
cp -rf "${ROOT_DIR}/resources/icons/TakeWeb" "${TARGET_DIR}/share/icons/"

cat <<EOF > "${TARGET_DIR}/README.txt"
TakeWeb v${VERSION} Portable Linux Bundle
========================================

How to run directly without installation:
  ./bin/takeweb

How to install for the current user (~/.local/bin):
  ./install.sh

How to uninstall:
  ./uninstall.sh

System Requirements:
  - Linux x86_64
  - GTK4 (>= 4.12)
  - Libadwaita (>= 1.5)
  - WebKitGTK (webkitgtk-6.0, >= 2.52)
EOF

tar -czf "${DIST_DIR}/${PKG_NAME}.tar.gz" -C "${PORTABLE_DIR}" "${PKG_NAME}"
rm -rf "${PORTABLE_DIR}"
echo "   Created: dist/${PKG_NAME}.tar.gz"

# ---------------------------------------------------------------------------
# 4. Arch Linux PKGBUILD & Package
# ---------------------------------------------------------------------------
echo "-> Creating Arch Linux PKGBUILD..."
mkdir -p "${DIST_DIR}/arch"
cat <<EOF > "${DIST_DIR}/arch/PKGBUILD"
# Maintainer: TakeWeb Contributors <contact@takeweb.org>
pkgname=takeweb-bin
pkgver=${VERSION}
pkgrel=1
pkgdesc="Privacy-first, zero-cloud lightweight web browser"
arch=('x86_64')
url="https://github.com/dezaetterg/take-web"
license=('GPL3')
depends=('gtk4' 'libadwaita' 'webkitgtk-6.0' 'libsoup3' 'sqlite')
provides=('takeweb')
conflicts=('takeweb')
source=("${PKG_NAME}.tar.gz::file://\${startdir}/../${PKG_NAME}.tar.gz")
sha256sums=('SKIP')

package() {
    cd "\${srcdir}/${PKG_NAME}"
    install -Dm755 "bin/takeweb" "\${pkgdir}/usr/bin/takeweb"
    install -Dm644 "share/applications/takeweb.desktop" "\${pkgdir}/usr/share/applications/io.github.dezaetterg.TakeWeb.desktop"
    
    for size in 16 22 24 32 48 64 96 128 256 512; do
        if [ -f "share/icons/hicolor/\${size}x\${size}/apps/takeweb.png" ]; then
            install -Dm644 "share/icons/hicolor/\${size}x\${size}/apps/takeweb.png" \\
                "\${pkgdir}/usr/share/icons/hicolor/\${size}x\${size}/apps/takeweb.png"
        fi
    done
    install -Dm644 "share/icons/hicolor/scalable/apps/takeweb.svg" \\
        "\${pkgdir}/usr/share/icons/hicolor/scalable/apps/takeweb.svg"
}
EOF
echo "   Created: dist/arch/PKGBUILD"

# ---------------------------------------------------------------------------
# 5. RPM Spec (Fedora / RHEL / openSUSE)
# ---------------------------------------------------------------------------
echo "-> Creating RPM spec..."
mkdir -p "${DIST_DIR}/rpm"
cat <<EOF > "${DIST_DIR}/rpm/takeweb.spec"
Name:           takeweb
Version:        ${VERSION}
Release:        1%{?dist}
Summary:        Privacy-first, zero-cloud lightweight web browser
License:        GPLv3
URL:            https://github.com/dezaetterg/take-web

Requires:       gtk4 >= 4.12
Requires:       libadwaita >= 1.5
Requires:       webkitgtk6.0 >= 2.52
Requires:       libsoup3
Requires:       sqlite

%description
TakeWeb is a modern Linux browser built with Rust, GTK4, Libadwaita, and WebKitGTK.
Features local offline translations via Bergamot WASM, built-in tracker shield,
customizable search & start page, and vertical tab management.

%install
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_datadir}/applications
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/scalable/apps

install -m 755 bin/takeweb %{buildroot}%{_bindir}/takeweb
install -m 644 share/applications/takeweb.desktop %{buildroot}%{_datadir}/applications/io.github.dezaetterg.TakeWeb.desktop
install -m 644 share/icons/hicolor/scalable/apps/takeweb.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/takeweb.svg

%files
%{_bindir}/takeweb
%{_datadir}/applications/io.github.dezaetterg.TakeWeb.desktop
%{_datadir}/icons/hicolor/*/apps/takeweb.*

%changelog
* Wed Nov 04 2026 TakeWeb Team <contact@takeweb.org> - 0.1.0-1
- Release 0.1.0
EOF
echo "   Created: dist/rpm/takeweb.spec"

# ---------------------------------------------------------------------------
# 6. SHA256 Checksums & Documentation
# ---------------------------------------------------------------------------
cd "${DIST_DIR}"
sha256sum *.deb *.AppImage *.tar.gz 2>/dev/null > SHA256SUMS || sha256sum *.* 2>/dev/null > SHA256SUMS || true

cat <<EOF > "${DIST_DIR}/README.md"
# TakeWeb Release Packages (v${VERSION})

Ready-to-use packages for various Linux distributions:

### 1. Universal AppImage (Runs on all distributions)
- **File**: \`TakeWeb-${VERSION}-x86_64.AppImage\`
- **Run**:
  \`\`\`bash
  chmod +x TakeWeb-${VERSION}-x86_64.AppImage
  ./TakeWeb-${VERSION}-x86_64.AppImage
  \`\`\`

### 2. Debian / Ubuntu / Mint / Pop!_OS (.deb)
- **File**: \`takeweb_${VERSION}_amd64.deb\`
- **Install**:
  \`\`\`bash
  sudo apt install ./takeweb_${VERSION}_amd64.deb
  \`\`\`
  Or:
  \`\`\`bash
  sudo dpkg -i takeweb_${VERSION}_amd64.deb
  sudo apt-get install -f
  \`\`\`

### 3. Portable Tarball (Any Linux distro, no root required)
- **File**: \`takeweb-${VERSION}-linux-x86_64.tar.gz\`
- **Usage**:
  \`\`\`bash
  tar -xzf takeweb-${VERSION}-linux-x86_64.tar.gz
  cd takeweb-${VERSION}-linux-x86_64
  # Run directly:
  ./bin/takeweb
  # Or install to ~/.local/bin with desktop shortcut:
  ./install.sh
  \`\`\`

### 4. Arch Linux / Manjaro / EndeavourOS
- **Directory**: \`dist/arch/\`
- **Install**:
  \`\`\`bash
  cd dist/arch
  makepkg -si
  \`\`\`

### 5. Fedora / RHEL / openSUSE
- **Directory**: \`dist/rpm/\`
- **Spec file**: \`takeweb.spec\`

### Verification
Check file integrity using SHA256 checksums:
\`\`\`bash
sha256sum -c SHA256SUMS
\`\`\`
EOF

echo "=== All packages generated successfully in dist/ ==="
ls -lh "${DIST_DIR}"
