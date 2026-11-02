#!/usr/bin/env bash
set -euo pipefail

BIN_DIR="${HOME}/.local/bin"
APPS_DIR="${HOME}/.local/share/applications"
ICONS_DIR="${HOME}/.local/share/icons"
HICOLOR_SCALABLE="${ICONS_DIR}/hicolor/scalable/apps"
HICOLOR_128="${ICONS_DIR}/hicolor/128x128/apps"
CONFIG_DIR="${XDG_CONFIG_HOME:-${HOME}/.config}/echo-browser"
CACHE_DIR="${XDG_CACHE_HOME:-${HOME}/.cache}/echo-browser"

PURGE=false
if [ "${1:-}" = "--purge" ] || [ "${1:-}" = "-p" ]; then
    PURGE=true
fi

echo "=== Удаление TakeWeb ==="

# 1. Удаление бинарников и симлинков
echo "Удаление бинарников из ${BIN_DIR}..."
rm -f "${BIN_DIR}/takeweb" "${BIN_DIR}/echo-browser"

# 2. Удаление .desktop файлов
echo "Удаление takeweb.desktop из ${APPS_DIR}..."
rm -f "${APPS_DIR}/takeweb.desktop"

# 3. Удаление иконок
echo "Удаление иконок из ${ICONS_DIR}..."
rm -f "${HICOLOR_SCALABLE}/takeweb.svg"
rm -f "${ICONS_DIR}/takeweb.svg"
rm -f "${HICOLOR_128}/takeweb.png"
rm -f "${ICONS_DIR}/takeweb.png"

# 4. Опциональная очистка пользовательских данных
if [ "${PURGE}" = true ]; then
    echo "Очистка пользовательских данных и профилей (--purge)..."
    rm -rf "${CONFIG_DIR}" "${CACHE_DIR}"
    echo "Конфигурация и кэш удалены."
else
    echo "Пользовательские данные сохранены (${CONFIG_DIR}). Для полного удаления запустите: ./uninstall.sh --purge"
fi

# 5. Обновление баз данных окружения рабочего стола
if command -v update-desktop-database >/dev/null 2>&1; then
    echo "Обновление desktop-database..."
    update-desktop-database "${APPS_DIR}" || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    echo "Обновление кэша иконок..."
    gtk-update-icon-cache -q -f -t "${ICONS_DIR}/hicolor" 2>/dev/null || true
fi

echo "=== Удаление завершено! ==="
