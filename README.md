# Space-Explorer: Real-Time Relativistic Space Simulator

[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg)](https://www.rust-lang.org/)
[![Engine](https://img.shields.io/badge/Bevy-0.15-blue.svg)](https://bevyengine.org/)
[![Graphics](https://img.shields.io/badge/Vulkan-1.3-red.svg)](https://www.vulkan.org/)
[![Platform](https://img.shields.io/badge/Platform-CachyOS%20%2F%20Arch%20Linux-green.svg)](https://cachyos.org/)
[![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-lightgrey.svg)](LICENSE)

**Space-Explorer** is an interactive, physically rigorous space simulator built in **Rust** using the **Bevy** engine. It bridges real-time 6-DOF Newtonian flight dynamics with **Einstein's Special Theory of Relativity**, allowing pilots to explore a celestial planetary system while experiencing real-time time dilation, relativistic rapidity, dynamic orbital mechanics, and astronomical-scale rendering.

---

## Key Highlights

- **Special Relativistic Kinematics**:
  - Proper acceleration formulation: velocity is parameterized via rapidity $\boldsymbol{\zeta}$ such that $\boldsymbol{\beta} = \mathbf{v}/c = \tanh(\boldsymbol{\zeta}) < 1.0$, guaranteeing that speed never exceeds $c$.
  - Real-time Lorentz factor calculation: $\gamma = \cosh(\|\boldsymbol{\zeta}\|) = \frac{1}{\sqrt{1 - \beta^2}}$.
  - **Twin Spacetime Clocks**: Dual integration of Ship Proper Time ($\tau$) and Planetary Coordinate Time ($t$) showcasing time dilation ($dt = \gamma \, d\tau$).
  - Adjustable speed of light ($c$): Switch dynamically between physical reality ($c = 299,792,458\text{ m/s}$) and demonstration mode ($c = 5,000\text{ m/s}$) to experience extreme dilation within local planetary orbits.

- **Astronomical Precision & Floating Origin Engine**:
  - 64-bit double-precision universal coordinates (`UniversalPosition(DVec3)`) tracking true positions across millions of meters without 32-bit floating-point precision loss or vertex jitter.
  - Two-tier projective horizon scaling: Celestial bodies beyond $40\text{ km}$ are projected onto the viewing horizon while dynamically scaling their visual geometry ($\theta = 2R/D$), completely eliminating far-plane clipping and Z-buffer fighting.

- **Celestial System & Orbital Mechanics**:
  - **Authentic PBR Surface Textures**: High-resolution photographic textures for Sol (photospheric granulations and solar flares), Terra (continents, azure oceans, polar caps, and Rayleigh scattering atmospheric halo), Luna (cratered regolith), Ares (Martian rust-red deserts and canyons), and Chronos (golden ammonia cloud bands).
  - **High-Resolution Alpha Planetary Rings**: Chronos features concentric ring disks with authentic radial opacity variations and the dark Cassini division mapped from NASA/Cassini imagery.
  - **Axial Tilts & Spin Dynamics**: Real physical polar tilts (Terra $23.4^\circ$, Ares $25.2^\circ$, Chronos $26.7^\circ$) and independent diurnal spin dynamics.
  - **Relativistic Texture Modulation**: All photographic surface textures and ring materials are dynamically shifted across the optical spectrum in real time under Doppler & beaming effects.
  - Hierarchical Keplerian orbits: Planets (Terra, Ares, Chronos) orbit the primary star (Sol), while moons (Luna, Titan) orbit their respective parent planets.
  - Dynamic solar illumination: Sunlight shines authentically from Sol, tracking the player's shuttle across space.
  - Deep-space procedural starfield with 15,000+ stars distributed across Morgan-Keenan blackbody stellar temperatures (M-class red dwarfs to O-class blue supergiants).

- **Flight Dynamics & Avionics**:
  - True 6-DOF vacuum momentum conservation (no artificial drag).
  - Rotational SAS (Flight Assist) attitude damping vs raw Newtonian tumbling.
  - Autopilot target tracking with continuous homing alignment.
  - Dual camera system: First-person Cockpit camera and Third-person Chase camera with smooth constant-speed zoom and subpixel analytical orientation.
  - **Relativistic Cockpit Avionics & Flight Instruments (Phase 5)**:
    - **Dynamic $\beta$ Velocity Tape**: Real-time progress bar ($0.0 \to 1.0\,c$) with multi-stage color transitions (Electric Cyan $\to$ Bright Amber $\to$ Pulsing Crimson Red) and calibrated tick marks.
    - **Lorentz $\gamma$ Factor Progress Meter**: Logarithmic progression bar with live exponential threshold alert badges ($>2\times, >5\times, >10\times$).
    - **Aberration Searchlight Cone Meter**: Live direction-aware aperture contractor and telemetry displaying celestial space compression, aft expansion void, and lateral drift.
    - **Variable Canopy Visor & Polarized Solar Filter (<kbd>K</kbd>)**: Multi-tier electrochromic canopy tinting (`CLEAR 100%` $\to$ `POLARIZED 25%` $\to$ `SOLAR FILTER 1%` H-alpha mode). Cuts through Sol's blinding HDR glare to reveal photospheric convective granules and rotating sunspots from NASA surface imagery.
  - ICAO/FAA-compliant aerospace navigation lighting (Port Red LED, Starboard Green LED, Dorsal White Strobe) with local wing point-lights.

---

## System Requirements & Prerequisites

### Minimum Requirements
- **Operating System:** Linux (CachyOS, Arch Linux, Fedora, Ubuntu) or Windows 10/11 (64-bit).
- **GPU:** Dedicated or Integrated GPU supporting **Vulkan 1.3** or **DirectX 12** (NVIDIA GeForce, AMD Radeon, Intel Arc/Iris). The engine automatically prefers discrete high-performance GPUs when available and gracefully defaults to integrated GPUs.
- **CPU:** Multi-core x86_64 processor.

### Recommended Build Tools (Linux)
For fast sub-second incremental compilation, the Linux launch script utilizes `mold` (or `lld`), `clang`, and `sccache`.

#### On CachyOS / Arch Linux:
```bash
sudo pacman -Syu --needed rust clang mold sccache
```

*(Optional AUR helper alternative: `yay -S mold-git sccache` or `paru -S mold-git sccache`)*

#### On Windows:
1. Install the Rust toolchain via [rustup.rs](https://rustup.rs/) (select the default MSVC C++ toolchain).
2. Ensure your GPU graphics drivers are up to date.

---

## Quick Start: Build & Run

### On Linux (Bash)
We provide an automated launcher script ([`run.sh`](run.sh)) that verifies prerequisites, enables `sccache`, activates the high-speed `mold` linker, and compiles the peak-performance native release binary by default:

```bash
# 1. Launch peak-performance simulation:
bash run.sh

# 2. Fast dev mode (quick iteration while editing code):
bash run.sh dev

# 3. Run automated test suites:
bash run.sh test
```

### On Windows (GUI Double-Click or Terminal)
Windows users have two effortless ways to launch:

1. **Double-Click (No Terminal Needed):**
   - Simply double-click [`run.bat`](run.bat) in File Explorer.
   - If Rust is not installed, the script will automatically prompt to download and run the official installer for you!
2. **Terminal (PowerShell / Command Prompt):**
   ```powershell
   # Using the PowerShell launcher:
   .\run.ps1

   # Or standard Cargo command:
   cargo run --release
   ```

---

## Standard Cargo Commands

You can also compile and run the project directly through Cargo:

```bash
# Fast development build
cargo run

# Optimized release build (ThinLTO, -O3, 1 codegen-unit)
cargo run --release

# Execute all physics and unit test suites
cargo test
```

> [!TIP]
> If you have a large RAM capacity or `zram`, you can export `CARGO_TARGET_DIR=/tmp/cargo-target` to perform all compilation inside RAM for faster build times and zero SSD wear.

---

## Flight Controls & Hotkeys

The simulator supports both **AZERTY** and **QWERTY** physical layouts:

### 1. Flight Dynamics & Thrusters
| Action | Key / Input | Notes |
| :--- | :--- | :--- |
| **Forward Thrusters** | `W` (QWERTY) / `Z` (AZERTY) or `Left Click` (hold) | Applies continuous linear acceleration |
| **Reverse / Retro-Burn** | `S` or `Right Click` (hold) | Decelerates linear velocity |
| **Lateral Strafe (Left / Right)** | `A` / `D` (or `Q` / `D` on AZERTY) | Port / starboard translation thrusters |
| **Vertical Thrusters (Up / Down)** | `Space` / `C` | Dorsal / ventral translation thrusters |
| **Speed Boost (Afterburner)** | `Left Shift` | Multiplies engine thrust by 3.0× |
| **Target Retro-Brake** | `X` | Cancels spin and decelerates to 0 m/s relative to locked target / local planet |
| **Universal Solar Brake** | `Shift + X` | Cancels spin and decelerates to 0 m/s relative to the Sun (Universal Rest Frame) |
| **Pitch & Yaw Steering** | `Mouse Motion` or `Arrow Keys` | Direct 1:1 attitude orientation |
| **Roll Steering** | `Q` / `E` (physical `A` / `E` on AZERTY) | Axial roll rotation |

### 2. Cruise Control & Autopilot
| Action | Key / Input | Notes |
| :--- | :--- | :--- |
| **Forward Cruise Lock** | `Ctrl + Z` or `Ctrl + Left Click` | Locks forward thrusters on continuously |
| **Reverse Cruise Lock** | `Ctrl + S` or `Ctrl + Right Click` | Locks reverse thrusters on continuously |
| **Boost Cruise Lock** | `Ctrl + Shift + Z` or `Ctrl + Shift + Left Click` | Locks boosted forward cruise |
| **Autopilot Target Alignment** | `F` | Smoothly brakes current spin and tracks target (inhibited if occluded) |
| **Hyperdrive Warp Autopilot** | `H` | Sci-fi warp drive with pre-stopping, closed-loop dampers & overshoot recovery |
| **Lorentz Brachistochrone** | `L` | Realistic relativistic flip-and-burn trajectory, finishes facing target forward |
| **Toggle Flight Assist (SAS)** | `Z` (physical `W` on AZERTY) | Toggles attitude damping vs raw Newtonian tumble |

### 3. Navigation & Targeting Computer
| Action | Key / Input | Notes |
| :--- | :--- | :--- |
| **Cycle Next Target** | `T` / `Shift T` | Cycles: Sol $\to$ Terra $\to$ Luna $\to$ Ares $\to$ Chronos $\to$ Titan $\to$ Gates |
| **Target Nearest Object** | `Y` | Automatically locks target with smallest distance |
| **Clear Target Lock** | `G` | Unlocks current target |
| **Target HUD Reticle** | Screen Overlay | Green brackets indicate target screen position |
| **Crosshair Info** | Center Reticle | Displays hovered object name and distance |

### 4. Camera & View Modes
| Action | Key / Input | Notes |
| :--- | :--- | :--- |
| **Toggle Camera View** | `V` | Switches between First-Person Cockpit & Third-Person Chase |
| **Free-Look Orbit** | Hold `Alt` / Press `F3` / `Numpad Numbers`  | Orbit camera freely around the shuttle |
| **Camera Zoom** | `Mouse Scroll Wheel` or `2-finger swipe` | Smooth constant-speed zoom in/out |
| **Unlock / Lock Mouse Cursor** | `Tab` | Releases cursor for window management |

### 5. Spacetime & Simulation Metrics
| Action | Key / Input | Notes |
| :--- | :--- | :--- |
| **Cockpit MFD Settings & Help** | `F9` | Toggles in-cockpit reference modal & live flight telemetry |
| **Canopy Visor / Solar Filter** | `K` | Cycles: `CLEAR 100%` → `POLARIZED 25%` → `SOLAR FILTER 1%` |
| **Pause / Resume** | `F1` or `return` | Freezes physics & motion; camera free-look remains active |
| **Toggle VSync** | `F2` | Switches between refresh-synced vs uncapped frame rate |
| **Toggle Soundtrack Mute** | `M` | Mutes / unmutes the looping background music track |
| **Toggle Relativistic Aberration** | `F4` | Toggles searchlight cone compression on/off |
| **Toggle Doppler Shift & Beaming** | `F5` | Toggles blueshift/redshift and radiance amplification on/off |
| **Time Scale / Bullet Time (Slow-Mo)** | `F6` / `F7` (or `[` / `]`) | Cycles time speed ($0.05\times, 0.10\times, 0.25\times, 0.50\times, 1.0\times, 2.0\times$) |
| **Reset Time Scale** | `F8` or `\` | Restores simulation time scale to $1.0\times$ (normal time) |
| **Physical Speed of Light** | `1` or `&` | Sets $c = 299,792,458\text{ m/s}$ (Earth reality) |
| **Demonstration Speed of Light**| `2` or `é` | Sets $c = 5,000\text{ m/s}$ (In-sector relativistic effects) |
| **Tune Speed of Light** | `[` / `]` or `o` / `p` or `Numpad +` / `-` | Decreases / increases $c$ continuously |
| **Mouse Sensitivity** | `Ctrl + Scroll` or `+` / `-` | Dynamically tunes steering & camera sensitivity ($\pm 20\%$) |
| **Respawn / Reset Spacetime** | `R` or `Numpad .` | Resets shuttle position, rapidity, and clocks to origin |
| **Exit Simulation / Dismiss Overlay** | `Escape` | Dismisses MFD overlay if active, or closes application cleanly |

---

## Relativistic Physics Engine

### 1. Proper Acceleration & Rapidity
In classical physics, velocity grows linearly without bound ($v = at$). In Special Relativity, constant engine acceleration in the ship's instantaneous rest frame (proper acceleration $\boldsymbol{\alpha}$) integrates into rapidity $\boldsymbol{\zeta}$:
$$\boldsymbol{\zeta}(t) = \int_0^t \frac{\boldsymbol{\alpha}(\tau)}{c} \, d\tau$$
The laboratory velocity is computed via the hyperbolic tangent:
$$\mathbf{v} = c \cdot \tanh(\|\boldsymbol{\zeta}\|) \cdot \frac{\boldsymbol{\zeta}}{\|\boldsymbol{\zeta}\|}$$
Because $\tanh(x) < 1$ for all real $x$, **velocity is strictly bounded by $c$ regardless of acceleration magnitude or burn duration**.

### 2. Lorentz Factor & Time Dilation
The Lorentz factor $\gamma$ determines the dilation between the pilot's clock ($\tau$) and the planetary coordinate clock ($t$):
$$\gamma = \cosh(\|\boldsymbol{\zeta}\|) = \frac{1}{\sqrt{1 - \beta^2}}, \quad \beta = \frac{\|\mathbf{v}\|}{c}$$
$$dt = \gamma \, d\tau$$
When flying near $c$, hours or years can elapse in the solar system while only seconds pass inside the cockpit, displayed live on the Spacetime Clocks HUD.

---

## Project Structure

```
Space-Explorer/
├── assets/
│   ├── audio/               # Atmospheric soundtrack ("ScienceClic (Musique mystérieuse)")
│   ├── fonts/               # UI typography (DejaVu Sans, DejaVu Sans Mono)
│   └── textures/
│       └── planets/         # Authentic high-resolution PBR surface maps & ring alpha
├── src/
│   ├── main.rs              # Application entry, ECS schedule & HDR camera
│   ├── flight.rs            # 6-DOF thrusters, SAS attitude damping, inputs
│   ├── relativity.rs        # Rapidity integration, Lorentz factor & dual clocks
│   ├── coords.rs            # 64-bit UniversalPosition & two-tier floating origin
│   ├── celestial.rs         # Keplerian orbital hierarchy, PBR skins, Sol, planets & moons
│   ├── camera.rs            # Analytical quaternion cockpit & chase camera
│   ├── navigation.rs        # Targeting computer & offscreen indicator reticle
│   ├── world.rs             # Shuttle meshes, PBR materials & aviation lights
│   ├── stars.rs             # Procedural 15,000-star astronomical starfield
│   └── ui.rs                # Glassmorphism HUD overlays & flight telemetry
├── run.sh                   # Automated build & launch script (sccache + mold)
├── Cargo.toml               # Dependencies, fast profiles & release LTO
└── ROADMAP.md               # Multi-phase project development roadmap
```

---

## Inspiration & Credits

- **Original Inspiration:** The concept and visual optics of this simulator were inspired by the video:
  - **Channel:** [ScienceClic](https://www.youtube.com/@ScienceClic)
  - **Video:** [Que verrait-on à la vitesse de la lumière ?](https://www.youtube.com/watch?v=rrreYWD5_z8)
- **Soundtrack:**
  - **Track:** *"ScienceClic (Musique mystérieuse)"*
  - **Composer:** Alessandro Roussel
  - **Album:** ScienceClic ([Listen on YouTube](https://www.youtube.com/watch?v=DBqgHwd4QYo))

---

## Roadmap & Upcoming Phases

- **Phase 4: Relativistic Visual Shaders & Optics**:
  - [x] Relativistic Light Aberration (Searchlight effect cone compression).
  - [x] Doppler frequency shift & relativistic beaming (radiance amplification).
  - [x] Relativistic Penrose apparent angular size mapping.
  - [x] Photorealistic celestial PBR skins with dynamic Doppler wavelength shifting.
  - [ ] Terrell-Penrose optical rotation (finite photon speed geometric distortion).
- **Phase 5: Cockpit HUD & Interactive Controls**:
  - [x] **Step 1: Graphical Relativistic Avionics Gauges & Visual Instruments**:
    - Dynamic $\beta$ velocity tape ($0.0 \to 1.0\,c$) with multi-stage color transitions (Cyan $\to$ Amber $\to$ Pulsing Crimson) and tick marks.
    - Non-linear Lorentz $\gamma$ meter with exponential threshold alerts ($>2\times, >5\times, >10\times$).
    - Relativistic Aberration searchlight cone meter ($180^\circ \to 5^\circ$ forward aperture indicator).
  - [x] **Step 2: Variable Canopy Visor & Polarized Solar Filter (Hotkey: `K`)**:
    - Multi-tier canopy tinting (`CLEAR` $\to$ `POLARIZED` $\to$ `SOLAR FILTER` H-alpha mode).
    - Dynamic glare attenuation cutting through Sol's blinding white HDR bloom to reveal high-resolution convective granules, sunspots, and limb features from `sun.jpg`.
  - [x] **Step 3: Interactive Settings & Keybindings Overlay (Hotkey: `F9`)**:
    - Semi-transparent cockpit MFD modal dialog with dual-column layout (Live Simulation Telemetry & Complete Flight Manual).
    - Real-time diagnostics for $c$, velocity, $\beta$, $\gamma$, proper/coordinate clocks, time-warp, optics toggles, visor filter, SAS, autopilot, audio volume, and VSync.
    - Dismissable via <kbd>F9</kbd> or <kbd>Escape</kbd>.
- **Phase 6: Advanced Relativistic & Gravitational Effects**:
  - [x] **Thermal Planck Blackbody Radiation Shift**: Continuous astrophysical temperature modulation ($T' = D \cdot T$) across 10,000+ stars, Sol, and all planetary bodies. Reflected sunlight and specular highlights on receding planets (e.g. Chronos) extinguish completely into the aft infrared void, while approaching planets brighten with brilliant blueshifted radiance.
  - [x] **Relativistic Cosmic Background Rainbow & CMB Optical Dipole Dome**: Relativistic Doppler transformation ($D(\theta) = \gamma(1 + \beta\cos\theta)$) of the isotropic cosmic radiation background (infrared & microwaves). Forms a continuous spherical rainbow wrapping around the player ($< 180^\circ$ FOV, inspired by ScienceClic) with a radiant blue-white core, cyan, green, yellow, and red rings on the flanks. Dynamically contracts into the forward searchlight cone under aberration (<kbd>F4</kbd>) or spans the wide sky when aberration is bypassed.
  - [ ] Terrell-Penrose optical rotation (retarded-time geometric photon integration).
  - [ ] General Relativity (gravitational time dilation, gravitational redshift, and light deflection).
