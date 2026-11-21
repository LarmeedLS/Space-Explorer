#!/usr/bin/env bash
# ==============================================================================
# Space-Explorer: High-Performance Build & Launch Script
# Optimized for CachyOS / Arch Linux with Clang, Mold, and Sccache
# ==============================================================================
set -euo pipefail

# ANSI Color Palette
BOLD='\033[1m'
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
RESET='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

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
    echo "  ║                  SPACE-EXPLORER LAUNCH ENGINE                         ║"
    echo "  ║      Real-Time Relativistic Special Relativity Space Simulator        ║"
    echo "  ╚═══════════════════════════════════════════════════════════════════════╝"
    echo -e "${RESET}"
}

check_prerequisites() {
    log_info "Verifying development toolchain and compilation prerequisites..."

    local missing_pkgs=()

    if ! command -v rustc &> /dev/null || ! command -v cargo &> /dev/null; then
        missing_pkgs+=("rust")
    fi

    if ! command -v clang &> /dev/null; then
        missing_pkgs+=("clang")
    fi

    if ! command -v mold &> /dev/null && ! command -v lld &> /dev/null; then
        missing_pkgs+=("mold")
    fi

    if ! command -v sccache &> /dev/null; then
        missing_pkgs+=("sccache")
    fi

    if [ ${#missing_pkgs[@]} -gt 0 ]; then
        log_warn "Some recommended performance tools are not found in your PATH:"
        for pkg in "${missing_pkgs[@]}"; do
            echo -e "  ${YELLOW}• ${pkg}${RESET}"
        done
        echo ""
        log_info "On CachyOS / Arch Linux, you can install them with:"
        echo -e "  ${BOLD}sudo pacman -Syu --needed ${missing_pkgs[*]}${RESET}"
        echo ""
        read -rp "Would you like to continue anyway with standard cargo? [y/N] " yn
        case "${yn:-N}" in
            [yY]|[yY][eE][sS])
                log_info "Proceeding with standard build..."
                ;;
            *)
                log_info "Aborting. Please install the packages above and re-run."
                exit 0
                ;;
        esac
    else
        log_success "All high-performance compilation tools detected (rustc, cargo, clang, mold, sccache)!"
    fi
}

configure_optimizations() {
    # 1. Use sccache as compiler cache wrapper if available (unless RUSTC_WRAPPER is explicitly set)
    if command -v sccache &> /dev/null && [ -z "${RUSTC_WRAPPER+x}" ]; then
        export RUSTC_WRAPPER="sccache"
        log_info "Compiler Cache: Enabled sccache (${BOLD}RUSTC_WRAPPER=sccache${RESET})"
    elif [ -n "${RUSTC_WRAPPER:-}" ]; then
        log_info "Compiler Cache: Custom wrapper (${BOLD}RUSTC_WRAPPER=${RUSTC_WRAPPER}${RESET})"
    else
        log_info "Compiler Cache: Disabled"
    fi

    # 2. Configure lightning-fast linker (mold > lld)
    if [ -z "${RUSTFLAGS:-}" ]; then
        if command -v mold &> /dev/null && command -v clang &> /dev/null; then
            export RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=mold"
            log_info "Linker: Enabled high-speed mold linker (-fuse-ld=mold)"
        elif command -v lld &> /dev/null && command -v clang &> /dev/null; then
            export RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=lld"
            log_info "Linker: Enabled LLVM lld linker (-fuse-ld=lld)"
        fi
    fi

    # 3. RAM target directory configuration
    if [ -z "${CARGO_TARGET_DIR:-}" ]; then
        if [ -d "/tmp" ] && [ -w "/tmp" ]; then
            export CARGO_TARGET_DIR="/tmp/cargo-target"
            mkdir -p "${CARGO_TARGET_DIR}"
            log_info "Target Directory: Using fast tmpfs at ${BOLD}${CARGO_TARGET_DIR}${RESET}"
        fi
    else
        log_info "Target Directory: Inherited ${BOLD}${CARGO_TARGET_DIR}${RESET}"
    fi
}

print_help() {
    print_banner
    echo -e "${BOLD}Usage:${RESET} ./run.sh [COMMAND]"
    echo ""
    echo -e "${BOLD}Commands:${RESET}"
    echo -e "  ${CYAN}run${RESET} (default)    Compile with peak native optimizations (-O3, LTO, native CPU) & launch"
    echo -e "  ${CYAN}build${RESET}          Compile the peak native release binary without running"
    echo -e "  ${CYAN}dev${RESET}            Fast incremental development build for editing/debugging"
    echo -e "  ${CYAN}test${RESET}           Execute all physics, camera, and ECS unit test suites"
    echo -e "  ${CYAN}check${RESET}          Fast syntax, type, and ECS borrow safety check"
    echo -e "  ${CYAN}status${RESET}         Display compiler toolchain and sccache cache statistics"
    echo -e "  ${CYAN}package${RESET}        Build standalone 7z release bundles (linux, windows, or all)"
    echo -e "  ${CYAN}version${RESET}        Display or bump project version (get, patch, minor, major)"
    echo -e "  ${CYAN}clean${RESET}          Safely clean build artifacts in CARGO_TARGET_DIR"
    echo -e "  ${CYAN}help${RESET}           Show this help information"
    echo ""
    echo -e "${BOLD}Examples:${RESET}"
    echo -e "  ./run.sh               # Run fully-optimized native game"
    echo -e "  ./run.sh dev           # Fast iteration build while modifying code"
    echo -e "  ./run.sh test          # Run automated unit tests"
    echo ""
}

show_status() {
    print_banner
    log_info "Rust Toolchain:"
    rustc --version || true
    cargo --version || true
    echo ""
    if command -v mold &> /dev/null; then
        log_info "Linker: $(mold --version | head -n1)"
    fi
    if command -v clang &> /dev/null; then
        log_info "C/C++ Compiler: $(clang --version | head -n1)"
    fi
    echo ""
    if command -v sccache &> /dev/null; then
        log_info "Sccache Hit/Miss Statistics:"
        sccache --show-stats || true
    else
        log_warn "sccache is not installed."
    fi
}

clean_target() {
    local target_dir="${CARGO_TARGET_DIR:-${SCRIPT_DIR}/target}"
    log_info "Target directory to clean: ${target_dir}"
    read -rp "Are you sure you want to clean build artifacts in '${target_dir}'? [y/N] " yn
    case "${yn:-N}" in
        [yY]|[yY][eE][sS])
            cargo clean
            log_success "Clean completed successfully."
            ;;
        *)
            log_info "Clean cancelled."
            ;;
    esac
}

main() {
    local command="${1:-run}"

    case "${command}" in
        help|--help|-h)
            print_help
            exit 0
            ;;
        status)
            show_status
            exit 0
            ;;
        clean)
            clean_target
            exit 0
            ;;
    esac

    print_banner
    check_prerequisites
    configure_optimizations

    case "${command}" in
        run|release|native)
            # Default & Run: Single unified maximum-performance release build tailored to the host CPU
            log_info "Building and launching Space-Explorer (Peak Performance: Release -O3 + ThinLTO + Native CPU)..."
            RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=native" cargo run --release
            ;;
        build|build-release)
            log_info "Compiling peak native release binary (-O3, ThinLTO, Native CPU)..."
            RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=native" cargo build --release
            log_success "Build complete! Binary located in ${CARGO_TARGET_DIR:-target}/release/space_explorer"
            ;;
        package|dist)
            shift || true
            bash "${SCRIPT_DIR}/tools/package.sh" "$@"
            exit 0
            ;;
        version)
            shift || true
            bash "${SCRIPT_DIR}/tools/version.sh" "$@"
            exit 0
            ;;
        dev)
            log_info "Building and launching Space-Explorer (fast dev profile for quick iteration)..."
            cargo run
            ;;
        test)
            log_info "Running test suites..."
            cargo test
            log_success "All tests passed!"
            ;;
        check)
            log_info "Running cargo check..."
            cargo check
            log_success "Check passed with zero errors!"
            ;;
        *)
            log_error "Unknown command: ${command}"
            echo ""
            print_help
            exit 1
            ;;
    esac
}

main "$@"
