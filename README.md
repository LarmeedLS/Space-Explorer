# Space-Explorer: Real-Time Relativistic Space Simulator

[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg)](https://www.rust-lang.org/)
[![Engine](https://img.shields.io/badge/Bevy-0.15-blue.svg)](https://bevyengine.org/)
[![Graphics](https://img.shields.io/badge/Vulkan-1.3%20%7C%20DX12-red.svg)](https://www.vulkan.org/)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20Windows-green.svg)](https://github.com/)
[![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-lightgrey.svg)](LICENSE)

**Space-Explorer** is an interactive, physically grounded space simulator built in **Rust** with **Bevy**. It combines 6-DOF Newtonian flight, planetary surface landings, and **Einstein's Special Theory of Relativity** — letting you fly across a 1:1 true-scale Solar System while experiencing real-time time dilation, relativistic light aberration, Doppler color shifts, beaming, and cosmic background radiation effects.

---

## Key Highlights

- **Special Relativistic Kinematics & Optics**:
  - **Proper Acceleration & Rapidity**: Velocity is integrated via rapidity $\boldsymbol{\zeta}$ so $\boldsymbol{\beta} = \mathbf{v}/c = \tanh(\boldsymbol{\zeta}) < 1$, strictly preventing superluminal speed.
  - **Real-Time Lorentz Time Dilation**: Dual tracking of Ship Proper Time ($\tau$) and Universe Coordinate Time ($t$) with $dt = \gamma \, d\tau$, where $\gamma = \cosh(\|\boldsymbol{\zeta}\|)$.
  - **Relativistic Visual Effects**: Light aberration (searchlight cone compression), Penrose apparent angular scaling, Planck blackbody Doppler color shifting & beaming on stars and planets, and the relativistic Cosmic Microwave Background (CMB) optical dipole rainbow.
  - **Two Universe Modes (<kbd>U</kbd> / <kbd>F10</kbd>)**: Switch seamlessly between the **1:1 Realistic Solar System** ($c = 299{,}792{,}458\text{ m/s}$) and the compact **Demo Sector** ($c = 5{,}000\text{ m/s}$) to explore extreme relativistic effects within local orbits.

- **True 1:1 Astronomical Floating-Origin Engine**:
  - 64-bit double-precision universal coordinates (`UniversalPosition(DVec3)`) across astronomical distances (Earth $R = 6{,}371\text{ km}$, Moon, outer planets, and Kuiper Belt / dwarf planets) at true 1:1 scale relative to the 168 m spacecraft.
  - Reversed-Z 32-bit depth buffer with a $10^{16}\text{ m}$ far plane and infinite-depth starfield occlusion.
  - 512×512 equirectangular planetary spheres with geometric sagitta alignment, `f64`-accumulated axial rotation, and smooth planetary surface touchdown, yaw steering, and liftoff.

- **Celestial System & Performance Scaling**:
  - **Full Solar System Hierarchy**: Sol, Mercury, Venus, Earth & Moon, Mars & moons, Jupiter & Galilean moons, Saturn & Cassini-division rings/moons, Uranus, Neptune, Pluto/Charon, and outer dwarf planets (including triaxial ellipsoid Haumea).
  - **Dynamic Proximity Texture Streaming & Quality Profiles (<kbd>F12</kbd> / <kbd>Shift+F12</kbd>)**: Automatically scales texture quality (`8K` / `4K` / `2K`) and streams high-resolution maps on demand so the simulation stays playable on both high-end rigs and lower-tier hardware (<16 GB RAM, <4 GB VRAM, integrated GPUs).
  - **Procedural Starfield**: 15,000+ stars distributed across Morgan-Keenan blackbody temperatures with GPU vertex aberration and Doppler shifting.

- **Flight Dynamics, Autopilot & Cockpit Avionics**:
  - True 6-DOF vacuum flight with toggleable Rotational SAS damping, slow-motion rate-limited mouse steering, and emergency retro-braking.
  - **Autopilot Modes**: Target Alignment (<kbd>F</kbd>), Sci-Fi Hyperdrive Warp (<kbd>H</kbd>), and Relativistic Lorentz Brachistochrone flip-and-burn (<kbd>L</kbd>).
  - **Cockpit HUD & Instruments**: $\beta$ velocity tape, logarithmic $\gamma$ dilation meter, aberration searchlight cone indicator, interactive **Orbital Radar (<kbd>N</kbd>)**, **Variable Canopy Visor & Solar Filter (<kbd>K</kbd>)**, and in-cockpit **MFD Settings & Help Overlay (<kbd>F9</kbd>)**.
  - **Dual Camera System (<kbd>V</kbd>)**: First-person Cockpit and Third-person Chase cameras with free-look orbit, zoom, and surface ground-collision protection.

---

## System Requirements & Prerequisites

- **Operating System:** Linux (64-bit) or Windows 10/11 (64-bit).
- **GPU:** Dedicated or integrated GPU supporting **Vulkan 1.3** or **DirectX 12**.
- **CPU:** Multi-core x86_64 processor.

### Recommended Build Tools (Linux)
For fast incremental compilation, [`run.sh`](run.sh) automatically uses `mold` (or `lld`), `clang`, and `sccache` when installed:

```bash
# CachyOS / Arch Linux
sudo pacman -Syu --needed rust clang mold sccache
```

### On Windows
1. Install Rust via [rustup.rs](https://rustup.rs/) (with the default MSVC toolchain).
2. Ensure your GPU drivers are up to date.

---

## Quick Start: Build & Run

### Linux
```bash
# Launch optimized native release build (default)
./run.sh

# Fast development build
./run.sh dev

# Run unit & physics test suite
./run.sh test

# Bump version, update CHANGELOG.md, and interactively commit/push
./run.sh version patch   # or minor / major / <X.Y.Z>
```

### Windows
- **GUI:** Double-click [`run.bat`](run.bat) in File Explorer.
- **Terminal:** Run `.\run.ps1` or `cargo run --release`.

### Standard Cargo Commands
```bash
cargo run            # Fast dev build
cargo run --release  # Optimized release build (O3 + ThinLTO)
cargo test           # Run all automated tests
```

> [!TIP]
> On Linux systems with plenty of RAM or `tmpfs` `/tmp`, `run.sh` automatically sets `CARGO_TARGET_DIR=/tmp/cargo-target` for faster builds and zero SSD wear.

---

## Flight Controls & Hotkeys

Supports both **QWERTY** and **AZERTY** physical keyboard layouts:

### 1. Flight Dynamics & Surface Controls
| Action | Key / Input | Notes |
| :--- | :--- | :--- |
| **Forward Thrusters / Takeoff** | `W` (QWERTY) / `Z` (AZERTY) or `Left Click` | Linear thrust; also lifts off smoothly when landed |
| **Reverse / Retro-Burn** | `S` or `Right Click` | Reverse linear thrust |
| **Lateral Strafe** | `A` / `D` (`Q` / `D` on AZERTY) | Port / starboard translation thrusters |
| **Vertical Thrusters / Liftoff** | `Space` / `C` | Dorsal / ventral thrust; `Space` lifts straight off surfaces |
| **Boost (Afterburner)** | `Left Shift` | High-acceleration impulse burn |
| **Target / Local Retro-Brake** | `X` | Cancels spin and matches velocity with target / closest body |
| **Universal Solar Brake** | `Shift + X` | Brakes to 0 m/s in the Solar rest frame |
| **Pitch & Yaw Steering** | `Mouse` or `Arrow Keys` | Attitude control (yaw steering remains active while landed) |
| **Roll Steering** | `Q` / `E` (`A` / `E` on AZERTY) | Axial roll rotation |

### 2. Cruise Control & Autopilot
| Action | Key / Input | Notes |
| :--- | :--- | :--- |
| **Forward / Reverse Cruise Lock** | `Ctrl + W/S` (`Ctrl + Z/S`) or `Ctrl + Click` | Locks continuous thrust (add `Shift` for boosted cruise) |
| **Target Alignment** | `F` | Smoothly brakes spin and aligns nose to target |
| **Hyperdrive Warp Autopilot** | `H` | High-speed warp autopilot with arrival braking |
| **Lorentz Brachistochrone** | `L` | Relativistic accelerate → flip → retro-burn trajectory |
| **Toggle Flight Assist (SAS)** | `Z` (`W` on AZERTY) | Toggles smooth attitude damping vs raw Newtonian tumble |

### 3. Navigation, Targeting & Radar
| Action | Key / Input | Notes |
| :--- | :--- | :--- |
| **Cycle Target** | `T` / `Shift + T` | Cycles celestial bodies in astronomical order |
| **Target Nearest Body** | `Y` | Locks the closest celestial body |
| **Point & Select / Clear Target** | `G` / `Middle Click` (`Shift + G` to clear) | Locks the body under the reticle or clears lock |
| **Orbital Radar** | `N` / `Shift + N` | Toggles radar map / cycles zoom focus mode |

### 4. Camera & View Modes
| Action | Key / Input | Notes |
| :--- | :--- | :--- |
| **Toggle Camera View** | `V` | Switches between First-Person Cockpit and Third-Person Chase |
| **Free-Look Orbit** | Hold `Alt` / `F3` / `Numpad 2,4,6,8` (`5` resets) | Orbits camera freely around the ship |
| **Camera Zoom** | `Scroll Wheel` | Smooth zoom in/out |
| **Unlock / Lock Mouse Cursor** | `Tab` | Releases cursor for radar interaction or window management |
| **Mouse Sensitivity** | `Ctrl + Scroll` or `+` / `-` | Tunes steering & orbit sensitivity ($\pm 20\%$) |

### 5. Spacetime, Optics & System
| Action | Key / Input | Notes |
| :--- | :--- | :--- |
| **Switch Universe Sector** | `U` / `F10` | Switches between 1:1 Realistic Solar System and Demo Sector |
| **Cockpit MFD Menu & Help** | `F9` | Toggles live telemetry & keybinding reference modal |
| **Canopy Visor / Solar Filter** | `K` | Cycles `CLEAR 100%` → `POLARIZED 25%` → `SOLAR FILTER 1.5%` |
| **Pause / Resume** | `F1` or `Enter` | Freezes simulation; camera free-look remains active |
| **Toggle VSync / Fullscreen** | `F2` / `F11` | Toggles VSync / Borderless Fullscreen |
| **Texture Quality / Streaming** | `F12` / `Shift + F12` | Cycles 8K/4K/2K textures / toggles dynamic proximity LOD |
| **Toggle Aberration / Doppler** | `F4` / `F5` | Toggles relativistic aberration / Doppler shift & beaming |
| **Time Scale (Slow-Mo / Warp)** | `F6` / `F7` (or `[` / `]`), `F8` / `\` resets | Adjusts simulation speed ($0.05\times \to 2.0\times$) |
| **Speed of Light Presets / Tune** | `1` (Real $c$), `2` (Demo $c$), `O` / `P` | Switches or continuously tunes $c$ |
| **Mute Soundtrack / Reset Ship** | `M` / `R` (or `Numpad .`) | Toggles audio mute / resets shuttle & clocks |
| **Exit / Dismiss Overlay** | `Escape` | Closes MFD menu or exits cleanly |

---

## Relativistic Physics Summary

- **Rapidity & Speed Limit:** Proper acceleration $\boldsymbol{\alpha}$ integrates into rapidity $\boldsymbol{\zeta}(t) = \int_0^t \frac{\boldsymbol{\alpha}(\tau)}{c}\,d\tau$, giving velocity $\mathbf{v} = c\tanh(\|\boldsymbol{\zeta}\|)\hat{\boldsymbol{\zeta}}$ strictly bounded below $c$.
- **Lorentz Time Dilation:** $\gamma = \cosh(\|\boldsymbol{\zeta}\|) = \frac{1}{\sqrt{1 - \beta^2}}$, advancing coordinate time as $dt = \gamma\,d\tau$.

---

## Project Structure

```text
Space-Explorer/
├── .github/workflows/
│   └── release.yml          # Automated Linux/Windows 7z builds & CHANGELOG release publishing
├── assets/
│   ├── audio/               # Looping soundtrack (scienceclic.ogg)
│   ├── fonts/               # UI fonts (DejaVu Sans & Mono)
│   ├── models/              # Spacecraft 3D glTF model (ship.glb)
│   ├── shaders/             # GPU relativistic starfield shader (starfield.wgsl)
│   └── textures/planets/    # High-resolution planetary PBR textures & ring maps
├── src/
│   ├── main.rs              # App entry, ECS system schedule, Reversed-Z HDR camera
│   ├── flight.rs            # 6-DOF flight, SAS, autopilots, surface landing & liftoff
│   ├── relativity.rs        # Rapidity integration, Lorentz factor, aberration & Doppler math
│   ├── coords.rs            # 64-bit UniversalPosition, 1:1 floating origin & sagitta alignment
│   ├── celestial.rs         # Solar System orbits, f64 axial rotation, texture streaming & optics
│   ├── camera.rs            # Cockpit & Chase cameras with landed surface elevation clamp
│   ├── navigation.rs        # Targeting computer, altitude telemetry & HUD reticle
│   ├── radar.rs             # Interactive 2D Solar System orbital radar
│   ├── stars.rs             # Procedural 15,000-star field & relativistic CMB dipole dome
│   ├── ui.rs                # Cockpit HUD, relativistic gauges, visor & MFD overlay
│   └── world.rs             # Spacecraft scene assembly, engine plumes & solar lighting
├── tools/
│   ├── version.sh           # Version bump, CHANGELOG editor & interactive Git release workflow
│   ├── package.sh           # Standalone Linux/Windows 7z release bundler
│   ├── compress_textures_ktx2.sh
│   └── convert_skins.sh
├── CHANGELOG.md             # Version history (source for GitHub Release notes)
├── Cargo.toml               # Crate manifest, dependencies & release LTO profile
├── run.sh / run.bat / run.ps1
```

---

## Releases & Changelog

See [CHANGELOG.md](CHANGELOG.md) for release history. Pushing a new version bumped with `./run.sh version patch|minor|major` automatically builds Linux and Windows `.7z` archives via GitHub Actions and publishes the matching `CHANGELOG.md` section on the GitHub Releases page.

---

## Inspiration & Credits

- **Concept & Visual Inspiration:** [ScienceClic](https://www.youtube.com/@ScienceClic) — [*Que verrait-on à la vitesse de la lumière ?*](https://www.youtube.com/watch?v=rrreYWD5_z8)
- **Soundtrack:** *"ScienceClic (Musique mystérieuse)"* by Alessandro Roussel ([YouTube](https://www.youtube.com/watch?v=DBqgHwd4QYo))
