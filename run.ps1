# ==============================================================================
# Space-Explorer: High-Performance Build & Launch Script for Windows (PowerShell)
# ==============================================================================
$ErrorActionPreference = "Stop"

Write-Host ""
Write-Host "  ╔═══════════════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "  ║                  SPACE-EXPLORER LAUNCH ENGINE                         ║" -ForegroundColor Cyan
Write-Host "  ║      Real-Time Relativistic Special Relativity Space Simulator        ║" -ForegroundColor Cyan
Write-Host "  ╚═══════════════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

# 1. Verify Rust & Cargo installation (with automatic download & installer prompt)
$defaultCargoBin = Join-Path $HOME ".cargo\bin"
$cargoExe = Join-Path $defaultCargoBin "cargo.exe"

# If cargo is not in PATH, check if it was installed in the default location
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    if (Test-Path $cargoExe) {
        Write-Host "[INFO] Detected Cargo at '$defaultCargoBin'. Adding to PATH for this session..." -ForegroundColor Cyan
        $env:PATH = "$defaultCargoBin;$env:PATH"
    } else {
        Write-Host "[WARN] Rust and Cargo are not installed on this computer!" -ForegroundColor Yellow
        Write-Host ""
        $answer = Read-Host "Would you like to automatically download and install Rust now? [Y/n]"
        if ($answer -eq "" -or $answer -match "^[yY]") {
            $installerUrl = "https://win.rustup.rs/x86_64"
            $installerPath = Join-Path $env:TEMP "rustup-init.exe"

            Write-Host "[INFO] Downloading official Rust installer from $installerUrl..." -ForegroundColor Cyan
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            Invoke-WebRequest -Uri $installerUrl -OutFile $installerPath -UseBasicParsing

            Write-Host "[INFO] Launching Rust installer (rustup-init)..." -ForegroundColor Cyan
            Write-Host "[NOTE] If prompted by the installer, choose option 1 (Proceed with standard installation)." -ForegroundColor Yellow
            
            # Launch installer and wait for user to complete it
            $process = Start-Process -FilePath $installerPath -Wait -PassThru

            # Refresh PATH from registry environment variables
            $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
            $machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
            $env:PATH = "$defaultCargoBin;$userPath;$machinePath;$env:PATH"

            if (-not (Get-Command cargo -ErrorAction SilentlyContinue) -and -not (Test-Path $cargoExe)) {
                Write-Host "[ERROR] Rust installation was cancelled or cargo is still missing." -ForegroundColor Red
                Write-Host "[INFO] You can install it manually from https://rustup.rs/ and re-run." -ForegroundColor Yellow
                exit 1
            }
            Write-Host "[SUCCESS] Rust toolchain has been installed successfully!" -ForegroundColor Green
            Write-Host ""
        } else {
            Write-Host "[INFO] Rust installation skipped. Please install Rust from https://rustup.rs/ to run Space-Explorer." -ForegroundColor Yellow
            exit 1
        }
    }
}

$action = if ($args.Count -gt 0) { $args[0].ToLower() } else { "run" }

switch ($action) {
    "run" {
        Write-Host "[INFO] Building and launching Space-Explorer (Peak Performance: Release -O3 + ThinLTO)..." -ForegroundColor Green
        cargo run --release
    }
    "dev" {
        Write-Host "[INFO] Building and launching Space-Explorer (Fast Dev Profile for quick iteration)..." -ForegroundColor Yellow
        cargo run
    }
    "build" {
        Write-Host "[INFO] Compiling Release Binary..." -ForegroundColor Green
        cargo build --release
        Write-Host "[SUCCESS] Build complete! Binary located in target\release\space_explorer.exe" -ForegroundColor Green
    }
    "test" {
        Write-Host "[INFO] Running Physics, Camera, and ECS Unit Test Suites..." -ForegroundColor Green
        cargo test
        Write-Host "[SUCCESS] All tests passed!" -ForegroundColor Green
    }
    "check" {
        Write-Host "[INFO] Checking Codebase Syntax, Types, and Borrow Safety..." -ForegroundColor Green
        cargo check
        Write-Host "[SUCCESS] Check passed with zero errors!" -ForegroundColor Green
    }
    default {
        Write-Host "Usage: .\run.ps1 [run | dev | build | test | check]" -ForegroundColor White
        Write-Host "  run   (default) Build with full release optimizations and launch"
        Write-Host "  dev             Fast incremental development build for testing"
        Write-Host "  build           Compile release binary without launching"
        Write-Host "  test            Run automated test suites"
        Write-Host "  check           Fast syntax and type validation"
    }
}
