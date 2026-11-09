# Relativistic Space Simulator - Development Roadmap

This roadmap tracks the development of a real-time relativistic space flight simulator built with **Rust** and **Bevy**, targeting discrete Vulkan rendering (NVIDIA RTX 5070).

---

## Progress Overview

- [x] **Phase 1: Engine Foundation & 3D Flight Controls**
- [x] **Phase 2: Relativistic Kinematics (The Math Core)**
- [x] **Phase 3: The Cosmos & Celestial Scale Handling**
- [x] **Phase 4: Relativistic Visual Shaders (Optics on GPU)**
- [x] **Phase 5: Cockpit HUD & Interactive Controls**
- [ ] **Phase 6: Orbital Mechanics, Collisions & Interstellar Exploration**
- [ ] **Phase 7: Advanced Relativistic & Gravitational Effects (Special & General Relativity)**


---

## Detailed Breakdown

### Phase 1: Engine Foundation & 3D Flight Controls
- [x] Configure `Cargo.toml` with Bevy engine, fast-compile profiles, and dependencies.
- [x] Configure Vulkan rendering backend to automatically prefer the high-performance discrete GPU (RTX 5070).
- [x] Implement 6-DOF (Degrees of Freedom) shuttle flight dynamics:
  - Pitch, Yaw, Roll via mouse look and keyboard.
  - Linear thrusters: Forward/Reverse (`W`/`S`), Lateral Strafe (`A`/`D`), Vertical Thrusters (`Space`/`Shift` or `C`).
  - True vacuum conservation of momentum: velocity is 100% conserved (no auto-deceleration).
  - Rotational SAS / Flight Assist toggle (`Z`).
  - Emergency retro-brake (`X`).
  - Speed boost / Afterburner (`LShift`).
- [x] Implement dual-camera system:
  - First-person Cockpit Camera (rock-solid, zero-jitter transform tracking).
  - Third-person Chase Camera (toggle with `V`).
  - Dedicated quit on `Escape`, mouse unlock on `Tab`.
- [x] Camera-Relative Floating Origin System (`f64` CPU $\to$ `f32` GPU):
  - Shuttle and world entities track 64-bit universal coordinates (`UniversalPosition(DVec3)`).
  - Shuttle stays anchored at render origin `(0, 0, 0)` with zero 32-bit float precision degradation.
  - Live 64-bit universal coordinates ($X, Y, Z$) displayed on the HUD telemetry.
- [x] Create reference grid, space beacons/waypoints, and spatial markers for speed and orientation perception.
- [x] Basic HUD overlay showing current flight mode, camera view, orientation, and controls.

---

### Phase 2: Relativistic Kinematics (The Math Core)
- [x] Replace Newtonian linear acceleration ($v = at$) with relativistic proper acceleration:
  $$v(t) = \frac{\alpha t}{\sqrt{1 + (\alpha t / c)^2}} \quad \implies \quad \beta = \frac{v}{c} < 1.0$$
- [x] Define velocity in terms of rapidity: $\boldsymbol{\beta} = \tanh(\boldsymbol{\zeta}) \cdot \hat{\mathbf{n}}$ (guaranteed $\beta < 1.0$).
- [x] Calculate Lorentz factor $\gamma = \cosh(\|\boldsymbol{\zeta}\|) = \frac{1}{\sqrt{1 - \beta^2}}$ in real time.
- [x] Implement dual time tracking:
  - Ship Proper Time ($\tau$): Time experienced inside the shuttle.
  - Coordinate Time ($t$): Planetary/Universal time ($dt = \gamma \, d\tau$).
- [x] Relativistic 4-velocity ($u^\mu = (\gamma c, \gamma \mathbf{v})$) and relativistic energy/momentum state representation.
- [x] Real-time Speed of Light ($c$) configuration:
  - Hotkey `1`: Real physical $c = 299,792,458\text{ m/s}$.
  - Hotkey `2`: Demonstration $c = 5,000\text{ m/s}$ (experience full relativistic effects in the sector).
  - Hotkeys `[` / `]`: Smoothly decrease / increase $c$ in real time.
- [x] Relativistic Cockpit Telemetry HUD:
  - Live $\beta = v/c$ gauge and progress tape.
  - Real-time Lorentz factor $\gamma$ readout.
  - Twin Spacetime Clocks: Ship Clock ($\tau$) vs. Universe Clock ($t$) with live dilation ratio.

---

### Phase 3: The Cosmos & Celestial Scale Handling
- [x] Procedural 3D starfield generator:
  - Generating 10,000+ stars with realistic 3D distribution.
  - Blackbody temperatures (M-class red dwarfs to O-class blue supergiants).
- [x] Floating origin system (origin rebasing) using 64-bit precision (`f64`) to eliminate jitter across astronomical distances.
- [x] Celestial reference bodies (Sol, Terra, Luna, Ares, Chronos, Titan) with Keplerian orbital hierarchy.
- [x] Waypoint target locking, offscreen indicator reticle, and distance telemetry.
- [x] Dynamic Sol illumination tracking and aerospace navigation lighting.
- [ ] **Dual Celestial System Scales (Compressed Playable vs 1:1 Real Solar System)**:
  - Mode 1: Compressed Sector Scale (playable flight and immediate orbital rendezvous without warp gates).
  - Mode 2: True Astronomical 1:1 Solar System Scale (Physical solar radius $696,340\text{ km}$, 1 AU Earth distance $149,600,000\text{ km}$, true orbital periods).
  - *(Deferred: To be implemented after completing and testing the relativistic optical physics).*

---

### Phase 4: Relativistic Visual Shaders (Optics on GPU)
- [x] **Aberration of Light (Searchlight Effect)**:
  - Vector relativistic aberration formula applied dynamically to all 15,000 stars and celestial bodies:
    $$\hat{\mathbf{n}}' = \frac{\hat{\mathbf{n}} + \left[ (\hat{\mathbf{n}} \cdot \hat{\boldsymbol{\beta}}) \left(1 - \frac{1}{\gamma}\right) + \beta \right] \hat{\boldsymbol{\beta}}}{\gamma (1 + \boldsymbol{\beta} \cdot \hat{\mathbf{n}})}$$
  - Dynamic field-of-view compression towards the forward velocity vector as $\beta \to 1$.
  - Real-time toggle on hotkey `F4`.
- [x] **Doppler Shift & Relativistic Beaming**:
  - Doppler frequency factor: $D = \frac{1}{\gamma (1 - \boldsymbol{\beta} \cdot \hat{\mathbf{n}})}$.
  - Wavelength shift: forward spectral blueshift (cyan/violet/white) and aft redshift (amber/ruby red/IR extinction).
  - Radiance boost ($D^3$) causing intense forward optical searchlight brightening.
  - Real-time toggle on hotkey `F5` and live forward Doppler factor ($D$) readout on the HUD.
- [x] **Terrell-Penrose Optical Size Contraction & Conformal Spherical Invariance**:
  - Exact analytical optical size contraction: $M(\theta) = \frac{1}{\gamma (1 + \boldsymbol{\beta} \cdot \hat{\mathbf{r}})}$ applied across all celestial bodies.
  - Scales planetary visual diameters in exact mathematical harmony with relativistic orbital aberration cone compression.
  - Prevents planets from appearing to collide with Sol at near-light speeds, preserving visual orbital clearance.
  - Conformal invariance (Penrose's theorem): Relativistically moving spheres preserve circular optical cross-sections.

---

### Phase 5: Cockpit HUD & Interactive Controls
- [x] **Step 1: Graphical Relativistic Avionics Gauges & Visual Instruments**:
  - Dynamic $\beta$ velocity tape/gauge ($0.0 \to 1.0\,c$) with color grading (Cyan $\to$ Amber $\to$ Pulsing Crimson Red) and tick markers ($0.25c, 0.50c, 0.75c, 0.90c, 0.99c$).
  - Non-linear Lorentz $\gamma$ factor meter with exponential visual threshold alerts ($>2\times, >5\times, >10\times$).
  - Relativistic Aberration searchlight cone visualizer (live opening half-angle $\theta_{\text{half}} = 2 \arctan\left(\sqrt{\frac{1-\beta}{1+\beta}}\right)$ showing forward celestial compression).
- [x] **Step 2: Variable Canopy Visor & Polarized Solar Filter (Hotkey: `K`)**:
  - Multi-tier canopy tinting and solar attenuation filter: `CLEAR` (100% transmittance), `POLARIZED` (25% sunglasses mode), and `SOLAR FILTER` (1% H-alpha observatory mode).
  - Selectively cuts through Sol's blinding white HDR glare and bloom, bringing out the high-resolution photographic photospheric granules, sunspots, and coronal flares from `sun.jpg`.
- [x] **Step 3: Interactive Settings & Keybindings Overlay (Hotkey: `F9`)**:
  - Aerospace glassmorphism Cockpit MFD modal overlay toggled by <kbd>F9</kbd> and dismissed via <kbd>Escape</kbd>.
  - Dual-column telemetry & manual layout:
    - **Live Telemetry & Diagnostics**: Real-time readouts of $c$ (demo vs real vacuum), coordinate velocity $v$ (m/s and km/s), relativistic ratio $\beta$, Lorentz factor $\gamma$, time acceleration/warp factor, ship proper time $\tau$ vs solar coordinate time $t$, optical aberration status, Doppler frequency status, relativistic beaming status, canopy visor filter mode, rotational SAS status, autopilot tracking status, navigation target lock, audio soundtrack volume, and display VSync mode.
    - **Flight Controls & Operations Manual**: Comprehensive categorized reference for 6-DOF thrusters, cruise locks, autopilot, targeting computer, camera views, relativistic toggles, and simulation mechanics for both QWERTY and AZERTY layouts.

---
### Phase 6: Orbital Mechanics, Collisions & Interstellar Exploration
- [x] **Relativistic Pseudo-Newtonian Gravity (Paczyński–Wiita Formulation)**:
  - High-performance, exact pseudo-Newtonian effective potential reproducing General Relativity around compact objects (neutron stars, black holes) while preserving pure Keplerian orbits for planets ($r \gg r_s$):
    $$\mathbf{a}_{\text{grav}} = - \frac{GM}{(r - r_s)^2} \, \frac{\mathbf{r}}{r} \quad \text{where} \quad r_s = \frac{2GM}{c^2}$$
  - Exact analytical reproduction of the Schwarzschild Event Horizon ($r_s$), Innermost Stable Circular Orbit (ISCO at $3r_s$), and Marginally Bound Orbit ($2r_s$).
  - Relativistic momentum integration $\frac{d(\gamma \mathbf{v})}{dt} = \mathbf{a}_{\text{grav}}$, guaranteeing that gravity respects the cosmic speed limit $c$.
  - Slingshot gravity assists around Jupiter and orbital insertion around planetary bodies.
- [x] **Collisions & Enhanced Ground Proximity Warning System (EGPWS)**:
  - Real-time radar altimeter computing true surface clearance: $h = \|\mathbf{P}_{\text{shuttle}} - \mathbf{P}_{\text{body}}\| - R_{\text{body}}$.
  - Multi-stage audio-visual terrain collision alarms (`TERRAIN PROXIMITY [PULL UP]`) based on closure rate and time-to-impact ($\text{TTI} = h / |v_r| < 5.0\,\text{s}$).
  - Relativistic kinetic surface impact detection: releases kinetic energy $E_k = (\gamma - 1)mc^2$. Catastrophic impact telemetry and recovery.
- [x] **Targeting Computer Relativistic Thermal Diagnostics**:
  - Live rest temperature ($T_0$) vs. perceived Doppler-shifted temperature ($T_{\text{eff}} = D \cdot T_0$) displayed on the cockpit TARGETING & NAVIGATION MFD panel.
- [ ] **Interstellar Star Systems & Exoplanets**:
  - 64-bit universal coordinates (`UniversalPosition(DVec3)`) and floating origin extended to multi-light-year distances.
  - Adding neighboring star systems (Alpha Centauri A/B, Proxima Centauri, Sirius) with navigable exoplanetary systems.


---
### Phase 7: Advanced Relativistic & Gravitational Effects (Special & General Relativity)
- [ ] **Terrell-Penrose Rotation (Visual Kinematics)**:
  - Non-simultaneous photon arrival (retarded-time integration) across moving 3D surface geometry.
  - Reconciles physical Lorentz contraction ($L' = L/\gamma$) with optical observation: moving spheres remain circular, flat hulls appear rotated rather than squashed.
- [x] **Thermal Planck Blackbody Radiation Shift (Continuous Spectral Shift)**:
  - Astrophysical continuous temperature modulation using Wien's displacement law: $T' = D \cdot T$.
  - 10,000+ procedural stars undergo continuous Planckian spectral evolution: cool M-class red dwarfs shift to solar yellow, then crisp A-class white, then hot O-class electric blue, while stars in the aft sky extinguish into the infrared void.
  - Analytical Planckian locus chromaticity integration converting temperature $T \in [0, \infty)\,\text{K}$ to physically based Linear sRGB with exponential Wien infrared extinction below $1,200\,\text{K}$ and strictly black emission at $T \le 600\,\text{K}$.
  - **Planetary Reflected Sunlight Shifting & Extinction**:
    - Reflected sunlight ($T_{\text{Sol}} = 5,778\,\text{K}$) transformed per-body via $T'_{\text{refl}} = D_{\text{planet}} \cdot T_{\text{Sol}}$ and relativistic beaming factor $D^3$.
    - Receding planets ($D < 1$): Diffuse color AND specular reflectance of Sol extinguish smoothly down to 0.0, completely eliminating phantom white specular highlights on redshifted crescents (e.g. Chronos).
    - Approaching planets ($D > 1$): Illuminated crescent brightens dramatically with blueshifted radiance ($D^3$, up to $6.0\times$ HDR bloom).
    - Dark side thermal physics: Cold planetary bodies ($100 - 300\,\text{K}$) remain authentically pitch-black on their unlit hemisphere, with blueshifted thermal blackbody radiation only faintly appearing as cherry embers at extreme speeds ($D > 6 - 10$).
- [x] **Relativistic Cosmic Background Rainbow & CMB Optical Dipole Dome**:
  - Full spherical celestial dome visualizing the relativistic Doppler transformation of the isotropic Cosmic Microwave Background (CMB) and Cosmic Infrared Background (CIB):
    $$D(\theta) = \gamma (1 + \beta \cos\theta)$$
  - Distinct circular **Hollow Rainbow Ring** (annulus) matching ScienceClic's visualization:
    - **Inside the Ring**: Background radiation is blueshifted beyond the visible window into the ultraviolet ($< 380\,\text{nm}$), leaving dark space inside the ring (except for clustered stars and high-energy focal beam).
    - **Inner Edge of Ring**: Electric blue and royal cyan ($380 - 480\,\text{nm}$).
    - **Middle of Ring**: Vibrant emerald green ($490 - 560\,\text{nm}$).
    - **Outer Edge of Ring**: Golden solar yellow, warm orange, and scarlet red ($570 - 700\,\text{nm}$).
    - **Outside the Ring**: Radiation remains in the infrared ($> 700\,\text{nm}$), extinguishing into pitch black space.
  - **Relativistic Ring Crunch Dynamics**:
    - At $\beta \approx 0.70 - 0.80c$: The rainbow ring emerges wide on the sides/flanks ($\sim 45^\circ - 55^\circ$ off-axis, as in ScienceClic at $80.82\%c$).
    - At $\beta \approx 0.95 - 0.9988c$: The ring crunches forward into a circular halo ahead of the ship ($\sim 15^\circ - 20^\circ$ off-axis, as in ScienceClic at $99.88\%c$).
    - At $\beta \ge 0.99999c$: The ring crunches into a compact circular halo ($\sim 8^\circ$ off-axis) surrounding the forward focal point.
  - **Starfield Lateral & Aft Redshift Extinction**:
    - Evaluates stellar Doppler using unaberrated resting directions $\hat{\mathbf{u}}_0 = \text{base\_dir}$.
    - Aft and lateral stars ($D \le 1$) undergo genuine relativistic redshifting and $D^3$ radiance extinction, fading completely into the aft darkness and leaving space on the sides dark, with the CMB ring prominently visible.
  - **Avionics & Canopy Visor Integration**:
    - Emerges authentically at relativistic speeds ($\beta \ge 0.60c$), reaching full brilliance at $\beta \ge 0.80c$ through $0.999999c$. Below $0.60c$, deep space remains authentically pitch black.
    - Fully reactive to Canopy Visor modes (<kbd>K</kbd>: Clear $100\%$, Polarized $40\%$, Solar Filter $8\%$).
    - Tracked in <kbd>F9</kbd> telemetry overlay with real-time blueshift temperature and velocity ratio.
- [ ] **General Relativity: Gravitational Time Dilation**:
  - Gravitational slowing of coordinate time near Sol and massive planetary bodies:
    $$d\tau = \sqrt{1 - \frac{2GM}{r c^2}} \, dt$$
  - Clocks in deep gravitational wells tick measurably slower than in deep-space vacuum.
- [ ] **General Relativity: Gravitational Redshift**:
  - Energy loss of photons escaping gravitational wells, shifting light towards longer wavelengths independent of observer velocity.
- [ ] **General Relativity: Gravitational Lensing & Light Deflection**:
  - Spacetime curvature light-bending around Sol and massive celestial bodies:
    $$\Delta \theta = \frac{4GM}{c^2 b}$$
  - Background starfield deflection around Sol's limb, forming Einstein rings and gravitational arcs during transits.
- [ ] **General Relativity: Frame Dragging (Lense-Thirring Precession)**:
  - Spacetime dragging induced by rotating celestial masses, causing orbital plane precession and gyroscopic drift in the vicinity of rotating stars.
