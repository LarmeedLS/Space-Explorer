# Changelog

All notable changes to **Space-Explorer** are documented in this file.
Each version section (`## [vX.Y.Z]`) is automatically published as the GitHub Release notes when pushing a new version.

## [v0.6.30] - 2026-11-21

### Features
- **True 1:1 Astronomical Scale**: Replaced compressed scaling with true 1:1 Euclidean floating-origin coordinates across the Solar System (accurate 168 m spacecraft scale against real planetary radii and AU distances), paired with a `1.0e16 m` Reversed-Z far plane and infinite-depth starfield occlusion.
- **Higher-Detail Planetary Geometry**: Increased planetary equirectangular UV sphere resolution from 256 to 512 subdivisions and added exact polygon sagitta elevation compensation near planetary surfaces.

### Bug Fixes
- **Relative Relativistic Aberration**: Fixed relativistic aberration and Penrose apparent angular size to compute strictly from relative velocity (`v_shuttle - v_body`) rather than heliocentric velocity, preventing planets from visually shifting when orbiting or landed.
- **Pause/Unpause Frame Synchronization**: Fixed a 1-frame orbital and visual jump when toggling pause by synchronizing simulation time (`shuttle_time_system`) ahead of orbital, rotation, and flight updates.
- **Planetary Rotation Precision & Ground Clipping**: Fixed `f32` quaternion norm drift in planetary axial rotation by accumulating spin angles in `f64` with strict normalization, stopping planet meshes from gradually inflating and swallowing landed ships over time.
- **Surface Touchdown & Liftoff**:
  - Automatically align the ship flat on its landing skids upon touchdown (`-13.95 m` clearance) and lock ground pitch/roll while keeping 360° yaw steering active.
  - Fixed surface liftoff so pressing `Space` or `W` lifts off cleanly at any framerate without requiring `Shift` boost.
- **Camera & Flight Controls**:
  - Decoupled camera orbit, pan, and zoom speed from simulation slow-motion and pause.
  - Raised and normal-aligned the landed chase camera elevation clamp so free-look no longer clips below the terrain.
  - Replaced rotational SAS braking with monotonic exponential decay (eliminating rotation rebound/bounce-back when releasing arrow/roll keys) and rate-limited mouse steering by simulation `dt` during slow motion.

### Miscellaneous
- Updated `run.sh` to respect an explicitly unset/empty `RUSTC_WRAPPER`.
- Cleaned up outdated items in `README.md`.

## [v0.6.29] - 2026-11-14

- Initial public release of **Space-Explorer**.
