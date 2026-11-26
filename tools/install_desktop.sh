#!/usr/bin/env bash
# ==============================================================================
# Space-Explorer: Linux Desktop Entry & Wayland Taskbar Icon Installer
# Registers ~/.local/share/applications/space-explorer.desktop so Wayland
# compositors (KDE Plasma, GNOME, etc.) and application launchers display the
# Space-Explorer titlebar, taskbar, and Alt+Tab icon.
#
# Works both from the source repository (./run.sh desktop) and from an
# extracted portable Linux release bundle (./install_desktop.sh).
# ==============================================================================
set -euo pipefail

BOLD='\033[1m'
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RESET='\033[0m'

log_info() {
    echo -e "${BLUE}${BOLD}[INFO]${RESET} $1"
}

log_success() {
    echo -e "${GREEN}${BOLD}[SUCCESS]${RESET} $1"
}

log_warn() {
    echo -e "${YELLOW}${BOLD}[WARN]${RESET} $1"
}

log_error() {
    echo -e "${RED}${BOLD}[ERROR]${RESET} $1"
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
XDG_DATA_DIR="${XDG_DATA_HOME:-${HOME}/.local/share}"
APPS_DIR="${XDG_DATA_DIR}/applications"
ICON_THEME_DIR="${XDG_DATA_DIR}/icons/hicolor"
INSTALLED_ICON_DIR="${ICON_THEME_DIR}/256x256/apps"
INSTALLED_ICON_FILE="${INSTALLED_ICON_DIR}/space-explorer.png"
DESKTOP_FILE="${APPS_DIR}/space-explorer.desktop"

print_help() {
    echo -e "${BOLD}Usage:${RESET} $(basename "$0") [install|status|help]"
    echo ""
    echo "Registers Space-Explorer's Linux .desktop entry in:"
    echo "  ${DESKTOP_FILE}"
    echo ""
    echo "This enables the application menu shortcut as well as the titlebar,"
    echo "taskbar, and Alt+Tab icon on Linux Wayland desktops (e.g., KDE Plasma)."
    echo ""
    echo -e "${BOLD}Commands:${RESET}"
    echo "  install  (default) Create or update space-explorer.desktop (backs up existing file)"
    echo "  status             Show current desktop entry status and paths"
    echo "  help               Show this help message"
}

detect_layout() {
    if [ -f "${SCRIPT_DIR}/space_explorer" ]; then
        BASE_DIR="${SCRIPT_DIR}"
        EXEC_TARGET="${BASE_DIR}/space_explorer"
        LAYOUT_MODE="Portable Linux Release Bundle"
    elif [ -f "${SCRIPT_DIR}/../Cargo.toml" ] && [ -f "${SCRIPT_DIR}/../run.sh" ]; then
        BASE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
        EXEC_TARGET="${BASE_DIR}/run.sh"
        LAYOUT_MODE="Source Repository"
    else
        log_error "Could not locate 'space_explorer' binary or repository root from '${SCRIPT_DIR}'."
        exit 1
    fi

    ICON_PATH="${BASE_DIR}/assets/icons/icon.png"
}

show_status() {
    if [ -f "${DESKTOP_FILE}" ]; then
        log_success "Desktop entry installed: ${DESKTOP_FILE}"
        echo ""
        cat "${DESKTOP_FILE}"
    else
        log_warn "Desktop entry is not installed (${DESKTOP_FILE} not found)."
        log_info "Run '$(basename "$0")' (or './run.sh desktop' in the repo) to install it."
    fi
}

refresh_desktop_caches() {
    if command -v gtk-update-icon-cache &> /dev/null && [ -d "${ICON_THEME_DIR}" ]; then
        gtk-update-icon-cache -f -t "${ICON_THEME_DIR}" >/dev/null 2>&1 || true
        log_info "Updated icon theme cache (${ICON_THEME_DIR})."
    fi

    if command -v update-desktop-database &> /dev/null; then
        update-desktop-database "${APPS_DIR}" 2>/dev/null || true
        log_info "Updated XDG desktop database (${APPS_DIR})."
    fi

    if command -v kbuildsycoca6 &> /dev/null; then
        kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
        log_info "Refreshed KDE Plasma 6 service cache (kbuildsycoca6)."
    elif command -v kbuildsycoca5 &> /dev/null; then
        kbuildsycoca5 --noincremental >/dev/null 2>&1 || true
        log_info "Refreshed KDE Plasma 5 service cache (kbuildsycoca5)."
    fi
}

install_desktop_entry() {
    detect_layout

    log_info "Detected mode: ${BOLD}${LAYOUT_MODE}${RESET}"
    log_info "Install target: ${BOLD}${DESKTOP_FILE}${RESET}"

    local desktop_icon_value
    if [ -f "${ICON_PATH}" ]; then
        mkdir -p "${INSTALLED_ICON_DIR}"
        cp -f "${ICON_PATH}" "${INSTALLED_ICON_FILE}"
        chmod 644 "${INSTALLED_ICON_FILE}"
        desktop_icon_value="space-explorer"
        log_info "Installed theme icon: ${BOLD}${INSTALLED_ICON_FILE}${RESET}"
    else
        desktop_icon_value="${ICON_PATH}"
        log_warn "Icon file not found yet at '${ICON_PATH}'."
        log_warn "The desktop entry will still point there; re-run this script once icon.png is in place."
    fi

    mkdir -p "${APPS_DIR}"

    local new_content
    new_content="$(cat <<EOF
[Desktop Entry]
Type=Application
Name=Space-Explorer
GenericName=Relativistic Space Simulator
Comment=Real-Time Relativistic Special Relativity Space Simulator
Exec="${EXEC_TARGET}"
Path=${BASE_DIR}
Icon=${desktop_icon_value}
Terminal=false
Categories=Game;Simulation;Science;
StartupNotify=true
StartupWMClass=space-explorer
EOF
)"

    if [ -f "${DESKTOP_FILE}" ]; then
        local backup_file="${DESKTOP_FILE}.bak"
        cp -f "${DESKTOP_FILE}" "${backup_file}"
        log_info "Backed up existing desktop entry to ${backup_file}"
    fi

    printf '%s\n' "${new_content}" > "${DESKTOP_FILE}"
    chmod 644 "${DESKTOP_FILE}"

    refresh_desktop_caches

    log_success "Installed Space-Explorer desktop entry!"
    echo -e "  ${BOLD}Exec:${RESET} ${EXEC_TARGET}"
    echo -e "  ${BOLD}Icon:${RESET} ${desktop_icon_value} (${INSTALLED_ICON_FILE})"
    echo -e "  ${BOLD}AppID:${RESET} space-explorer"
}

main() {
    local action="${1:-install}"
    case "${action}" in
        install)
            install_desktop_entry
            ;;
        status)
            show_status
            ;;
        help|--help|-h)
            print_help
            ;;
        *)
            log_error "Unknown action: '${action}'"
            echo ""
            print_help
            exit 1
            ;;
    esac
}

main "$@"
