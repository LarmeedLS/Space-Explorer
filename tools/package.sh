#!/usr/bin/env bash
# ==============================================================================
# Space-Explorer: Release Packaging Engine
# Builds standalone, non-native portable bundles for Linux and Windows (x86_64)
# Compressed using high-efficiency 7-Zip LZMA2 ultra solid compression
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
cd "${PROJECT_ROOT}"

# Ensure rustup toolchain takes precedence over system Arch Linux rust
# (Crucial for cross-compilation target standard libraries like x86_64-pc-windows-gnu)
export PATH="${HOME}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:${HOME}/.cargo/bin:${PATH}"

DIST_DIR="/tmp/dist"
TARGET_DIR="/tmp/dist-target"

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

print_banner() {
    echo -e "${CYAN}${BOLD}"
    echo "  ╔═══════════════════════════════════════════════════════════════════════╗"
    echo "  ║              SPACE-EXPLORER STANDALONE PACKAGING                      ║"
    echo "  ║     Portable x86_64 Release Bundles for Linux & Windows (7z Ultra)    ║"
    echo "  ╚═══════════════════════════════════════════════════════════════════════╝"
    echo -e "${RESET}"
}

check_tool() {
    local cmd="$1"
    local desc="$2"
    if ! command -v "${cmd}" &> /dev/null; then
        log_error "${desc} ('${cmd}') is required but not installed."
        return 1
    fi
}

compress_7z() {
    local archive_path="$1"
    local source_dir="$2"
    local base_name
    base_name="$(basename "${source_dir}")"
    local parent_dir
    parent_dir="$(dirname "${source_dir}")"

    log_info "Compressing bundle with 7-Zip (LZMA2 Ultra Solid 1024MB Dictionary)..."
    
    # Run 7z from the parent directory so the archive contains a clean root folder
    (
        cd "${parent_dir}"
        # If previous archive exists, move it aside or overwrite
        if [ -f "${archive_path}" ]; then
            mkdir -p "${parent_dir}/.backup"
            mv -f "${archive_path}" "${parent_dir}/.backup/" 2>/dev/null || true
        fi

        7z a -t7z -m0=lzma2 -mx=9 -mmt=on -md=1024m -ms=on -mfb=273 -mmemuse=p90 "${archive_path}" "${base_name}"
    )

    local size
    size="$(du -h "${archive_path}" | cut -f1)"
    log_success "Archive generated: ${archive_path} (${size})"
}

build_linux() {
    log_info "============================================================"
    log_info "Building Portable Linux Release (Generic x86_64 Baseline)..."
    log_info "============================================================"

    check_tool "7z" "7-Zip archiver"
    check_tool "cargo" "Rust Cargo"

    mkdir -p "${DIST_DIR}"
    local bundle_dir="${DIST_DIR}/Space-Explorer-Linux"
    mkdir -p "${bundle_dir}"

    # Compile with generic baseline (NO -C target-cpu=native to ensure compatibility on any AMD/Intel x86_64)
    log_info "Compiling generic release binary..."
    RUSTC_WRAPPER="" RUSTFLAGS="" cargo build --release --target-dir "${TARGET_DIR}/linux"

    log_info "Copying executable and assets into bundle..."
    cp -f "${TARGET_DIR}/linux/release/space_explorer" "${bundle_dir}/space_explorer"
    chmod +x "${bundle_dir}/space_explorer"

    # Copy assets tree
    mkdir -p "${bundle_dir}/assets"
    cp -rf "${PROJECT_ROOT}/assets/"* "${bundle_dir}/assets/"

    # Bundle Linux .desktop installer for Wayland/X11 taskbar icon & menu shortcut
    cp -f "${PROJECT_ROOT}/tools/install_desktop.sh" "${bundle_dir}/install_desktop.sh"
    chmod +x "${bundle_dir}/install_desktop.sh"

    local archive="${DIST_DIR}/Space-Explorer-Linux-x86_64.7z"
    compress_7z "${archive}" "${bundle_dir}"
    log_success "Linux release packaging complete!"
}

build_windows() {
    log_info "============================================================"
    log_info "Building Portable Windows Release (x86_64 MinGW GNU)..."
    log_info "============================================================"

    check_tool "7z" "7-Zip archiver"
    check_tool "x86_64-w64-mingw32-gcc" "MinGW-w64 C Compiler / Linker"

    # Verify windows target in rustup
    if ! rustup target list | grep -q "x86_64-pc-windows-gnu (installed)"; then
        log_info "Target x86_64-pc-windows-gnu not installed. Installing via rustup..."
        rustup target add x86_64-pc-windows-gnu
    fi

    mkdir -p "${DIST_DIR}"
    local bundle_dir="${DIST_DIR}/Space-Explorer-Windows"
    mkdir -p "${bundle_dir}"

    # Compile for Windows x86_64 (clear RUSTFLAGS so Linux mold/clang flags never leak into MinGW PE linking)
    log_info "Compiling Windows x86_64 release binary..."
    RUSTC_WRAPPER="" \
    RUSTFLAGS="" \
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="x86_64-w64-mingw32-gcc" \
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR="x86_64-w64-mingw32-ar" \
        cargo build --release --target x86_64-pc-windows-gnu --target-dir "${TARGET_DIR}/windows"

    log_info "Copying executable and assets into bundle..."
    cp -f "${TARGET_DIR}/windows/x86_64-pc-windows-gnu/release/space_explorer.exe" "${bundle_dir}/space_explorer.exe"

    # Copy assets tree
    mkdir -p "${bundle_dir}/assets"
    cp -rf "${PROJECT_ROOT}/assets/"* "${bundle_dir}/assets/"

    # Bundle MinGW C++ runtime DLLs (libstdc++-6.dll, libgcc_s_seh-1.dll, libwinpthread-1.dll)
    # Required for Windows systems, Wine, and Proton without requiring MinGW installed
    log_info "Bundling MinGW C++ runtime DLLs..."
    local mingw_bin="/usr/x86_64-w64-mingw32/bin"
    for dll in libstdc++-6.dll libgcc_s_seh-1.dll libwinpthread-1.dll; do
        if [ -f "${mingw_bin}/${dll}" ]; then
            cp -f "${mingw_bin}/${dll}" "${bundle_dir}/"
        fi
    done

    local archive="${DIST_DIR}/Space-Explorer-Windows-x86_64.7z"
    compress_7z "${archive}" "${bundle_dir}"
    log_success "Windows release packaging complete!"
}

usage() {
    echo -e "${BOLD}Usage:${RESET} ./run.sh package [TARGET]  (or ./tools/package.sh [TARGET])"
    echo ""
    echo -e "${BOLD}Targets:${RESET}"
    echo -e "  ${CYAN}linux${RESET}    Build generic Linux x86_64 bundle & Space-Explorer-Linux-x86_64.7z"
    echo -e "  ${CYAN}windows${RESET}  Cross-compile Windows x86_64 bundle & Space-Explorer-Windows-x86_64.7z"
    echo -e "  ${CYAN}all${RESET}      Build and package both Linux and Windows releases"
    echo ""
}

main() {
    local target="${1:-all}"
    print_banner

    case "${target}" in
        linux)
            build_linux
            ;;
        windows)
            build_windows
            ;;
        all)
            build_linux
            build_windows
            ;;
        help|--help|-h)
            usage
            ;;
        *)
            log_error "Unknown packaging target: '${target}'"
            usage
            exit 1
            ;;
    esac
}

main "$@"
