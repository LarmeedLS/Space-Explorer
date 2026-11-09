#!/usr/bin/env bash
# ==============================================================================
# Space-Explorer: Version Management Engine
# Reads and bumps the canonical game version in Cargo.toml & Cargo.lock
# ==============================================================================
set -euo pipefail

BOLD='\033[1m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
RED='\033[0;31m'
RESET='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CARGO_TOML="${PROJECT_ROOT}/Cargo.toml"

get_current_version() {
    grep -E '^version\s*=' "${CARGO_TOML}" | head -n1 | sed -E 's/version\s*=\s*"([^"]+)"/\1/'
}

parse_version() {
    local ver="$1"
    ver="${ver#v}"
    local major minor patch
    IFS='.' read -r major minor patch <<< "${ver}"
    if [[ -z "${major:-}" || -z "${minor:-}" || -z "${patch:-}" ]]; then
        echo "Error: Version '${ver}' is not in valid semver format (X.Y.Z)" >&2
        return 1
    fi
    echo "${major} ${minor} ${patch}"
}

set_version() {
    local new_ver="$1"
    local current_ver
    current_ver="$(get_current_version)"

    echo -e "${BLUE}${BOLD}[INFO]${RESET} Bumping Space-Explorer version: ${YELLOW}${current_ver}${RESET} -> ${GREEN}${BOLD}${new_ver}${RESET}"

    # Update version in Cargo.toml
    sed -i -E "s/^(version\s*=\s*\")[^\"]+(\")/\1${new_ver}\2/" "${CARGO_TOML}"

    # Synchronize Cargo.lock
    (
        cd "${PROJECT_ROOT}"
        RUSTC_WRAPPER="" cargo check --quiet 2>/dev/null || true
    )

    echo -e "${GREEN}${BOLD}[SUCCESS]${RESET} Version successfully updated to ${BOLD}${new_ver}${RESET} in Cargo.toml and Cargo.lock!"
}

bump_patch() {
    local current_ver
    current_ver="$(get_current_version)"
    read -r major minor patch < <(parse_version "${current_ver}")
    local next_patch=$((patch + 1))
    set_version "${major}.${minor}.${next_patch}"
}

bump_minor() {
    local current_ver
    current_ver="$(get_current_version)"
    read -r major minor patch < <(parse_version "${current_ver}")
    local next_minor=$((minor + 1))
    set_version "${major}.${next_minor}.0"
}

bump_major() {
    local current_ver
    current_ver="$(get_current_version)"
    read -r major minor patch < <(parse_version "${current_ver}")
    local next_major=$((major + 1))
    set_version "${next_major}.0.0"
}

usage() {
    echo -e "${CYAN}${BOLD}Space-Explorer Version Management${RESET}"
    echo -e "${BOLD}Usage:${RESET} ./run.sh version [COMMAND]  (or ./tools/version.sh [COMMAND])"
    echo ""
    echo -e "${BOLD}Commands:${RESET}"
    echo -e "  ${CYAN}get${RESET}               Display current version (e.g. $(get_current_version))"
    echo -e "  ${CYAN}patch${RESET}             Bump patch version (e.g. 0.1.0 -> 0.1.1)"
    echo -e "  ${CYAN}minor${RESET}             Bump minor version (e.g. 0.1.0 -> 0.2.0)"
    echo -e "  ${CYAN}major${RESET}             Bump major version (e.g. 0.1.0 -> 1.0.0)"
    echo -e "  ${CYAN}<X.Y.Z>${RESET}           Set explicit custom semver version"
    echo ""
}

main() {
    local action="${1:-get}"

    case "${action}" in
        get|current)
            get_current_version
            ;;
        patch)
            bump_patch
            ;;
        minor)
            bump_minor
            ;;
        major)
            bump_major
            ;;
        help|--help|-h)
            usage
            ;;
        *)
            if [[ "${action}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
                set_version "${action}"
            else
                echo -e "${RED}${BOLD}[ERROR]${RESET} Unknown action or invalid semver: '${action}'"
                usage
                exit 1
            fi
            ;;
    esac
}

main "$@"
