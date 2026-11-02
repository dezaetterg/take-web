#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${HOME}/.local/bin"
APPS_DIR="${HOME}/.local/share/applications"
ICONS_DIR="${HOME}/.local/share/icons"
HICOLOR_SCALABLE="${ICONS_DIR}/hicolor/scalable/apps"
HICOLOR_128="${ICONS_DIR}/hicolor/128x128/apps"

echo "=== Установка TakeWeb ==="

# 1. Сборка release-бинарника (если отсутствует)
if [ ! -f "${SCRIPT_DIR}/target/release/takeweb" ]; then
    echo "Сборка оптимизированного бинарника (cargo build --release)..."
    cargo build --release --manifest-path "${SCRIPT_DIR}/Cargo.toml"
fi

# 2. Создание каталогов назначения
mkdir -p "${BIN_DIR}" "${APPS_DIR}" "${ICONS_DIR}" "${HICOLOR_SCALABLE}" "${HICOLOR_128}"

# 3. Копирование исполняемого файла
echo "Копирование бинарника в ${BIN_DIR}/takeweb..."
install -m 755 "${SCRIPT_DIR}/target/release/takeweb" "${BIN_DIR}/takeweb"
ln -sf "${BIN_DIR}/takeweb" "${BIN_DIR}/echo-browser"

# 4. Копирование иконок
echo "Копирование иконок в ${ICONS_DIR}..."
for size in 16 22 24 32 48 64 96 128 256 512; do
    s_dir="${ICONS_DIR}/hicolor/${size}x${size}/apps"
    mkdir -p "${s_dir}"
    if [ -f "${SCRIPT_DIR}/resources/icons/takeweb_${size}.png" ]; then
        cp -f "${SCRIPT_DIR}/resources/icons/takeweb_${size}.png" "${s_dir}/takeweb.png"
        cp -f "${SCRIPT_DIR}/resources/icons/takeweb_${size}.png" "${s_dir}/org.echo_linux.TakeWeb.png"
    elif [ -f "${SCRIPT_DIR}/resources/icons/takeweb.png" ]; then
        cp -f "${SCRIPT_DIR}/resources/icons/takeweb.png" "${s_dir}/takeweb.png"
        cp -f "${SCRIPT_DIR}/resources/icons/takeweb.png" "${s_dir}/org.echo_linux.TakeWeb.png"
    fi
done

if [ -f "${SCRIPT_DIR}/resources/icons/takeweb.svg" ]; then
    cp -f "${SCRIPT_DIR}/resources/icons/takeweb.svg" "${HICOLOR_SCALABLE}/takeweb.svg"
    cp -f "${SCRIPT_DIR}/resources/icons/takeweb.svg" "${HICOLOR_SCALABLE}/org.echo_linux.TakeWeb.svg"
    cp -f "${SCRIPT_DIR}/resources/icons/takeweb.svg" "${ICONS_DIR}/takeweb.svg"
    for theme in MacTahoe MacTahoe-light MacTahoe-dark; do
        if [ -d "${ICONS_DIR}/${theme}/apps/scalable" ]; then
            cp -f "${SCRIPT_DIR}/resources/icons/takeweb.svg" "${ICONS_DIR}/${theme}/apps/scalable/takeweb.svg"
        fi
    done
fi

if [ -f "${SCRIPT_DIR}/resources/icons/takeweb.png" ]; then
    cp -f "${SCRIPT_DIR}/resources/icons/takeweb.png" "${ICONS_DIR}/takeweb.png"
fi

# 5. Копирование .desktop файла
echo "Копирование takeweb.desktop в ${APPS_DIR}..."
cp -f "${SCRIPT_DIR}/takeweb.desktop" "${APPS_DIR}/takeweb.desktop"
cp -f "${SCRIPT_DIR}/takeweb.desktop" "${APPS_DIR}/org.echo_linux.TakeWeb.desktop"

# 6. Обновление баз данных окружения рабочего стола
if command -v update-desktop-database >/dev/null 2>&1; then
    echo "Обновление desktop-database..."
    update-desktop-database "${APPS_DIR}" || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    echo "Обновление кэша иконок..."
    gtk-update-icon-cache -q -f -t "${ICONS_DIR}/hicolor" 2>/dev/null || true
fi

echo "=== Установка завершена успешно! ==="
echo "Запуск: takeweb"
