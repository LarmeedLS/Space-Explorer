#!/usr/bin/env bash
# ==============================================================================
# Space-Explorer: Version Management & Release Preparation Engine
# 1. Reads or bumps the canonical game version in Cargo.toml & Cargo.lock
# 2. Inserts a new version heading into CHANGELOG.md and opens it in your editor
# 3. Interactively prompts to stage files (git add), commit (git commit without -m),
#    and push (git push) to trigger the GitHub Actions release workflow
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
CHANGELOG_FILE="${PROJECT_ROOT}/CHANGELOG.md"

SKIP_EDIT=false
SKIP_GIT=false

get_current_version() {
    grep -E '^version\s*=' "${CARGO_TOML}" | head -n1 | sed -E 's/version\s*=\s*"([^"]+)"/\1/'
}

parse_version() {
    local ver="$1"
    ver="${ver#v}"
    ver="${ver#V}"
    local major minor patch
    IFS='.' read -r major minor patch <<< "${ver}"
    if [[ -z "${major:-}" || -z "${minor:-}" || -z "${patch:-}" ]]; then
        echo "Error: Version '${ver}' is not in valid semver format (X.Y.Z)" >&2
        return 1
    fi
    echo "${major} ${minor} ${patch}"
}

ask_yes_no() {
    local prompt="$1"
    local default="${2:-n}"
    local suffix="[y/N]"
    if [[ "${default}" == "y" ]]; then
        suffix="[Y/n]"
    fi

    local answer
    while true; do
        read -rp "${prompt} ${suffix} " answer || answer=""
        answer="${answer,,}"
        if [[ -z "${answer}" ]]; then
            [[ "${default}" == "y" ]] && return 0 || return 1
        fi
        case "${answer}" in
            y|yes|o|oui) return 0 ;;
            n|no|non)    return 1 ;;
            *) echo "Please answer y/yes or n/no." ;;
        esac
    done
}

insert_changelog_entry() {
    local new_ver="$1"
    local today
    today="$(date +%Y-%m-%d)"

    python3 - "${CHANGELOG_FILE}" "${new_ver}" "${today}" <<'PY'
import re, sys
from pathlib import Path

changelog_path = Path(sys.argv[1])
new_ver = sys.argv[2]
today = sys.argv[3]

header = f"## [v{new_ver}] - {today}"
template = (
    f"{header}\n\n"
    f"### Features\n"
    f"- \n\n"
    f"### Bug Fixes\n"
    f"- \n\n"
)

if not changelog_path.exists():
    content = (
        "# Changelog\n\n"
        "All notable changes to **Space-Explorer** are documented in this file.\n"
        "Each version section (`## [vX.Y.Z]`) is automatically published as the GitHub Release notes when pushing a new version.\n\n"
        + template
    )
    changelog_path.write_text(content, encoding="utf-8")
    print("CREATED")
    sys.exit(0)

content = changelog_path.read_text(encoding="utf-8")
if re.search(rf'^##\s+\[?v?{re.escape(new_ver)}\]?(?:\s|$)', content, re.M):
    print("EXISTS")
    sys.exit(0)

# Insert right before the first existing version heading (newest first)
first_heading = re.search(r'^##\s+', content, re.M)
if first_heading:
    idx = first_heading.start()
    updated = content[:idx] + template + content[idx:]
else:
    sep = "\n\n" if not content.endswith("\n\n") else ""
    updated = content + sep + template

changelog_path.write_text(updated, encoding="utf-8")
print("INSERTED")
PY
}

open_in_editor() {
    local file_path="$1"

    # 1. Respect explicit $VISUAL or $EDITOR (blocking by default for terminal editors;
    #    add wait flags if user configured kate/code/gedit without one)
    local env_editor="${VISUAL:-${EDITOR:-}}"
    if [[ -n "${env_editor}" ]]; then
        echo -e "${BLUE}${BOLD}[INFO]${RESET} Opening ${BOLD}${file_path##*/}${RESET} with ${CYAN}${env_editor}${RESET}..."
        # shellcheck disable=SC2206
        local cmd=( ${env_editor} )
        local bin_name="${cmd[0]##*/}"
        if [[ "${#cmd[@]}" -eq 1 ]]; then
            case "${bin_name}" in
                kate)              cmd+=(--block) ;;
                code|codium)       cmd+=(--wait) ;;
                gedit)             cmd+=(--wait) ;;
                gnome-text-editor) cmd+=(-s) ;;
            esac
        fi
        "${cmd[@]}" "${file_path}"
        return 0
    fi

    # 2. Graphical desktop environment editors (blocking so we wait until save & close)
    if [[ -n "${DISPLAY:-}" || -n "${WAYLAND_DISPLAY:-}" ]]; then
        if command -v kate >/dev/null 2>&1; then
            echo -e "${BLUE}${BOLD}[INFO]${RESET} Opening ${BOLD}${file_path##*/}${RESET} with ${CYAN}kate --block${RESET} (save and close when done)..."
            kate --block "${file_path}"
            return 0
        elif command -v kwrite >/dev/null 2>&1; then
            echo -e "${BLUE}${BOLD}[INFO]${RESET} Opening ${BOLD}${file_path##*/}${RESET} with ${CYAN}kwrite${RESET} (save and close when done)..."
            kwrite "${file_path}"
            return 0
        elif command -v code >/dev/null 2>&1; then
            echo -e "${BLUE}${BOLD}[INFO]${RESET} Opening ${BOLD}${file_path##*/}${RESET} with ${CYAN}code --wait${RESET} (save and close when done)..."
            code --wait "${file_path}"
            return 0
        elif command -v gnome-text-editor >/dev/null 2>&1; then
            echo -e "${BLUE}${BOLD}[INFO]${RESET} Opening ${BOLD}${file_path##*/}${RESET} with ${CYAN}gnome-text-editor -s${RESET}..."
            gnome-text-editor -s "${file_path}"
            return 0
        elif command -v gedit >/dev/null 2>&1; then
            echo -e "${BLUE}${BOLD}[INFO]${RESET} Opening ${BOLD}${file_path##*/}${RESET} with ${CYAN}gedit --wait${RESET}..."
            gedit --wait "${file_path}"
            return 0
        fi
    fi

    # 3. WSL / Windows fallback
    if command -v notepad.exe >/dev/null 2>&1; then
        echo -e "${BLUE}${BOLD}[INFO]${RESET} Opening ${BOLD}${file_path##*/}${RESET} with ${CYAN}notepad.exe${RESET}..."
        notepad.exe "${file_path}"
        return 0
    elif command -v explorer.exe >/dev/null 2>&1; then
        echo -e "${BLUE}${BOLD}[INFO]${RESET} Opening ${BOLD}${file_path##*/}${RESET} with ${CYAN}explorer.exe${RESET}..."
        explorer.exe "${file_path}" || true
        read -rp "Press Enter once you have saved and closed ${file_path##*/}... " _
        return 0
    fi

    # 4. Terminal editors
    for cand in nvim nano vim vi; do
        if command -v "${cand}" >/dev/null 2>&1; then
            echo -e "${BLUE}${BOLD}[INFO]${RESET} Opening ${BOLD}${file_path##*/}${RESET} with ${CYAN}${cand}${RESET}..."
            "${cand}" "${file_path}"
            return 0
        fi
    done

    # 5. Generic desktop fallback (non-blocking -> wait for user confirmation)
    if command -v xdg-open >/dev/null 2>&1; then
        echo -e "${BLUE}${BOLD}[INFO]${RESET} Opening ${BOLD}${file_path##*/}${RESET} via ${CYAN}xdg-open${RESET}..."
        xdg-open "${file_path}" >/dev/null 2>&1 &
        read -rp "Press Enter once you have saved and closed ${file_path##*/}... " _
        return 0
    fi

    echo -e "${YELLOW}${BOLD}[WARN]${RESET} No text editor found. Please edit ${file_path} manually."
    read -rp "Press Enter when finished editing ${file_path##*/}... " _
}

interactive_git_workflow() {
    local new_ver="$1"

    if ! command -v git >/dev/null 2>&1; then
        echo -e "${YELLOW}${BOLD}[WARN]${RESET} 'git' not found in PATH. Skipping Git workflow."
        return 0
    fi

    if ! git -C "${PROJECT_ROOT}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        echo -e "${YELLOW}${BOLD}[WARN]${RESET} Not inside a Git repository. Skipping Git workflow."
        return 0
    fi

    echo ""
    echo -e "${CYAN}${BOLD}=== Git Status ===${RESET}"
    git -C "${PROJECT_ROOT}" status --short
    echo ""

    # 1. Prompt to stage files
    echo -e "${BOLD}Stage files for commit?${RESET}"
    echo "  1. Stage all changes (git add -A) [default]"
    echo "  2. Stage only version & changelog files (Cargo.toml, Cargo.lock, CHANGELOG.md)"
    echo "  3. Do not run git add (keep current staging area as-is)"
    local stage_choice
    read -rp "Choice [1/2/3, default=1]: " stage_choice || stage_choice=""
    stage_choice="${stage_choice:-1}"

    case "${stage_choice}" in
        1)
            git -C "${PROJECT_ROOT}" add -A
            echo -e "${GREEN}${BOLD}[OK]${RESET} Staged all changes."
            ;;
        2)
            git -C "${PROJECT_ROOT}" add -- Cargo.toml Cargo.lock CHANGELOG.md
            echo -e "${GREEN}${BOLD}[OK]${RESET} Staged Cargo.toml, Cargo.lock, and CHANGELOG.md."
            ;;
        3)
            echo -e "${BLUE}${BOLD}[INFO]${RESET} Skipped git add."
            ;;
        *)
            echo -e "${YELLOW}${BOLD}[WARN]${RESET} Invalid choice '${stage_choice}'. Skipping git add."
            ;;
    esac

    # 2. Prompt to commit (without -m so Git opens the commit message editor)
    echo ""
    if ask_yes_no "Create Git commit now (runs 'git commit' in your editor)?" "y"; then
        if git -C "${PROJECT_ROOT}" diff --cached --quiet; then
            echo -e "${YELLOW}${BOLD}[WARN]${RESET} Nothing staged to commit."
        else
            if git -C "${PROJECT_ROOT}" commit; then
                echo -e "${GREEN}${BOLD}[OK]${RESET} Commit created successfully."
            else
                echo -e "${YELLOW}${BOLD}[WARN]${RESET} Commit aborted or failed. Skipping push."
                return 0
            fi
        fi
    else
        echo -e "${BLUE}${BOLD}[INFO]${RESET} Skipping commit. Your changes remain local."
        return 0
    fi

    # 3. Prompt to push
    echo ""
    if ask_yes_no "Push to remote now ('git push') to publish v${new_ver}?" "n"; then
        git -C "${PROJECT_ROOT}" push
        echo ""
        echo -e "${GREEN}${BOLD}[SUCCESS]${RESET} Pushed to remote!"
        echo -e "${BLUE}${BOLD}[INFO]${RESET} GitHub Actions will automatically:"
        echo "  - detect version v${new_ver} from Cargo.toml,"
        echo "  - build Linux & Windows .7z release bundles,"
        echo "  - extract the v${new_ver} section from CHANGELOG.md as the Release Notes,"
        echo "  - create tag v${new_ver} and publish the GitHub Release."
    else
        echo -e "${BLUE}${BOLD}[INFO]${RESET} Push skipped. Run 'git push' whenever you are ready to publish v${new_ver}."
    fi
}

set_version() {
    local new_ver="$1"
    local current_ver
    current_ver="$(get_current_version)"

    echo -e "${BLUE}${BOLD}[INFO]${RESET} Bumping Space-Explorer version: ${YELLOW}${current_ver}${RESET} -> ${GREEN}${BOLD}${new_ver}${RESET}"

    # 1. Update version in Cargo.toml
    sed -i -E "s/^(version\s*=\s*\")[^\"]+(\")/\1${new_ver}\2/" "${CARGO_TOML}"

    # 2. Synchronize Cargo.lock
    (
        cd "${PROJECT_ROOT}"
        RUSTC_WRAPPER="" cargo check --quiet 2>/dev/null || true
    )
    echo -e "${GREEN}${BOLD}[SUCCESS]${RESET} Version updated to ${BOLD}${new_ver}${RESET} in Cargo.toml and Cargo.lock!"

    # 3. Prepend new version heading to CHANGELOG.md
    local cl_status
    cl_status="$(insert_changelog_entry "${new_ver}")"
    case "${cl_status}" in
        CREATED)  echo -e "${GREEN}${BOLD}[SUCCESS]${RESET} Created CHANGELOG.md with section ${BOLD}## [v${new_ver}]${RESET}." ;;
        INSERTED) echo -e "${GREEN}${BOLD}[SUCCESS]${RESET} Added section ${BOLD}## [v${new_ver}]${RESET} to CHANGELOG.md." ;;
        EXISTS)   echo -e "${BLUE}${BOLD}[INFO]${RESET} Section ${BOLD}## [v${new_ver}]${RESET} already exists in CHANGELOG.md." ;;
    esac

    # 4. Open CHANGELOG.md in editor (interactive mode)
    if [[ "${SKIP_EDIT}" == "false" && -t 0 ]]; then
        open_in_editor "${CHANGELOG_FILE}"
    fi

    # 5. Interactive git add -> git commit (no -m) -> git push
    if [[ "${SKIP_GIT}" == "false" && -t 0 ]]; then
        interactive_git_workflow "${new_ver}"
    fi
}

bump_patch() {
    local current_ver
    current_ver="$(get_current_version)"
    read -r major minor patch < <(parse_version "${current_ver}")
    set_version "${major}.${minor}.$((patch + 1))"
}

bump_minor() {
    local current_ver
    current_ver="$(get_current_version)"
    read -r major minor patch < <(parse_version "${current_ver}")
    set_version "${major}.$((minor + 1)).0"
}

bump_major() {
    local current_ver
    current_ver="$(get_current_version)"
    read -r major minor patch < <(parse_version "${current_ver}")
    set_version "$((major + 1)).0.0"
}

usage() {
    echo -e "${CYAN}${BOLD}Space-Explorer Version & Release Management${RESET}"
    echo -e "${BOLD}Usage:${RESET} ./run.sh version [COMMAND] [OPTIONS]  (or ./tools/version.sh [COMMAND] [OPTIONS])"
    echo ""
    echo -e "${BOLD}Commands:${RESET}"
    echo -e "  ${CYAN}get${RESET}               Display current version (currently $(get_current_version))"
    echo -e "  ${CYAN}patch${RESET}             Bump patch version (e.g. 0.6.29 -> 0.6.30)"
    echo -e "  ${CYAN}minor${RESET}             Bump minor version (e.g. 0.6.29 -> 0.7.0)"
    echo -e "  ${CYAN}major${RESET}             Bump major version (e.g. 0.6.29 -> 1.0.0)"
    echo -e "  ${CYAN}<X.Y.Z>${RESET}           Set explicit custom semver version"
    echo ""
    echo -e "${BOLD}Options:${RESET}"
    echo -e "  ${CYAN}--no-edit${RESET}         Do not open CHANGELOG.md in an editor"
    echo -e "  ${CYAN}--no-git${RESET}          Skip interactive git add / commit / push prompts"
    echo -e "  ${CYAN}--non-interactive${RESET} Implies --no-edit and --no-git"
    echo ""
}

main() {
    local action=""
    for arg in "$@"; do
        case "${arg}" in
            --no-edit)         SKIP_EDIT=true ;;
            --no-git)          SKIP_GIT=true ;;
            --non-interactive) SKIP_EDIT=true; SKIP_GIT=true ;;
            -h|--help|help)    usage; exit 0 ;;
            *)
                if [[ -z "${action}" ]]; then
                    action="${arg}"
                else
                    echo -e "${RED}${BOLD}[ERROR]${RESET} Unexpected argument: '${arg}'" >&2
                    usage
                    exit 1
                fi
                ;;
        esac
    done

    action="${action:-get}"

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
        *)
            if [[ "${action}" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
                set_version "${action#v}"
            else
                echo -e "${RED}${BOLD}[ERROR]${RESET} Unknown action or invalid semver: '${action}'" >&2
                usage
                exit 1
            fi
            ;;
    esac
}

main "$@"
