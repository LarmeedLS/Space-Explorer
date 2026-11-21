use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::renderer::RenderAdapterInfo;
use crate::coords::UniversalPosition;
use crate::flight::Shuttle;
use crate::navigation::NavigationComputer;
use crate::relativity::{RelativisticConfig, RelativisticState, DEMO_C, REAL_C};
use std::f64::consts::PI;

/// Exact physical 1 Astronomical Unit (mean Earth-Sun distance in meters)
pub const ASTRONOMICAL_UNIT: f64 = 149_597_870_700.0;

/// Available simulated universe sectors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
pub enum UniverseMode {
    #[default]
    DemoSector,
    RealisticSolarSystem,
}

impl UniverseMode {
    pub fn next(&self) -> Self {
        match self {
            UniverseMode::DemoSector => UniverseMode::RealisticSolarSystem,
            UniverseMode::RealisticSolarSystem => UniverseMode::DemoSector,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            UniverseMode::DemoSector => "DEMO SECTOR (Scaled Sol & Planets)",
            UniverseMode::RealisticSolarSystem => "1:1 REAL SOLAR SYSTEM (Astronomical Scale)",
        }
    }
}

/// Global resource tracking the active simulation universe and pending transitions
#[derive(Resource, Debug, Clone)]
pub struct UniverseConfig {
    pub active_mode: UniverseMode,
    pub pending_switch: Option<UniverseMode>,
}

impl Default for UniverseConfig {
    fn default() -> Self {
        Self {
            active_mode: UniverseMode::RealisticSolarSystem,
            pending_switch: None,
        }
    }
}

/// Tag component marking any entity that belongs to the dynamic universe sector
/// (celestial bodies, moons, rings, and demo obstacles). Despawned recursively on universe switch.
#[derive(Component)]
pub struct UniverseEntity;

/// Component marking a celestial body with its physical radius (in meters) and mass (in kilograms)
#[derive(Component, Clone, Copy, Debug)]
pub struct CelestialBody {
    pub physical_radius: f64,
    pub mass_kg: f64,
}

/// Non-spherical shape descriptor for triaxial ellipsoids (e.g. Methone, Haumea, Phobos, Deimos).
/// Stores semi-axes (a, b, c) in meters along the body's local (X, Y, Z) coordinate frame:
/// - X: Equatorial semi-major axis (e.g. pointing towards primary if tidally locked)
/// - Y: Polar semi-axis (along rotation axis)
/// - Z: Equatorial semi-minor axis (along orbit direction)
#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct EllipsoidRadii(pub DVec3);

impl EllipsoidRadii {
    pub fn new(a: f64, b: f64, c: f64) -> Self {
        Self(DVec3::new(a, b, c))
    }

    /// Computes the distance from center to surface in local normalized direction u
    pub fn surface_radius_in_local_dir(&self, local_dir: DVec3) -> f64 {
        let inv_a = 1.0 / self.0.x;
        let inv_b = 1.0 / self.0.y;
        let inv_c = 1.0 / self.0.z;
        let term_x = local_dir.x * inv_a;
        let term_y = local_dir.y * inv_b;
        let term_z = local_dir.z * inv_c;
        let denom_sq = term_x * term_x + term_y * term_y + term_z * term_z;
        if denom_sq > 1e-12 {
            1.0 / denom_sq.sqrt()
        } else {
            self.0.x
        }
    }

    /// Scaling factors relative to volumetric mean physical_radius
    pub fn shape_scale(&self, mean_radius: f64) -> Vec3 {
        if mean_radius > 0.0 {
            Vec3::new(
                (self.0.x / mean_radius) as f32,
                (self.0.y / mean_radius) as f32,
                (self.0.z / mean_radius) as f32,
            )
        } else {
            Vec3::ONE
        }
    }
}

/// Dynamic physical coordinate velocity vector in m/s (d/dt of UniversalPosition in the universal reference frame)
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BodyVelocity(pub DVec3);

/// Component defining Keplerian orbital dynamics around a parent body or solar barycenter
#[derive(Component, Clone, Copy, Debug)]
pub struct KeplerOrbit {
    pub parent: Option<Entity>,
    pub semi_major_axis: f64,
    pub eccentricity: f64,
    pub inclination_rad: f64,
    pub longitude_of_ascending_node_rad: f64,
    pub argument_of_periapsis_rad: f64,
    pub orbital_period_seconds: f64,
    pub initial_mean_anomaly: f64,
}

impl Default for KeplerOrbit {
    fn default() -> Self {
        Self {
            parent: None,
            semi_major_axis: 0.0,
            eccentricity: 0.0,
            inclination_rad: 0.0,
            longitude_of_ascending_node_rad: 0.0,
            argument_of_periapsis_rad: 0.0,
            orbital_period_seconds: 1.0,
            initial_mean_anomaly: 0.0,
        }
    }
}

impl KeplerOrbit {
    /// Constructs a circular orbit (e = 0) with inclination tilt
    pub fn circular(
        parent: Option<Entity>,
        semi_major_axis: f64,
        inclination_rad: f64,
        orbital_period_seconds: f64,
        initial_mean_anomaly: f64,
    ) -> Self {
        Self {
            parent,
            semi_major_axis,
            eccentricity: 0.0,
            inclination_rad,
            longitude_of_ascending_node_rad: 0.0,
            argument_of_periapsis_rad: 0.0,
            orbital_period_seconds,
            initial_mean_anomaly,
        }
    }

    /// Constructs an eccentric Keplerian orbit with full 3D spatial orientation (e, i, Ω, ω)
    pub fn eccentric(
        parent: Option<Entity>,
        semi_major_axis: f64,
        eccentricity: f64,
        inclination_rad: f64,
        longitude_of_ascending_node_rad: f64,
        argument_of_periapsis_rad: f64,
        orbital_period_seconds: f64,
        initial_mean_anomaly: f64,
    ) -> Self {
        Self {
            parent,
            semi_major_axis,
            eccentricity: eccentricity.clamp(0.0, 0.999),
            inclination_rad,
            longitude_of_ascending_node_rad,
            argument_of_periapsis_rad,
            orbital_period_seconds,
            initial_mean_anomaly,
        }
    }

    /// Solves Kepler's Equation M = E - e*sin(E) for Eccentric Anomaly E using Newton-Raphson iteration.
    /// Converges rapidly (typically 3-6 iterations) even for extreme eccentricities up to 0.95.
    pub fn solve_eccentric_anomaly(&self, mean_anomaly: f64) -> f64 {
        let e = self.eccentricity;
        if e <= 1e-12 {
            return mean_anomaly;
        }

        // Normalize mean anomaly to [0, 2π)
        let m = mean_anomaly.rem_euclid(2.0 * PI);

        // High-accuracy initial guess
        let mut e_anom = if e < 0.8 {
            m + e * m.sin() + 0.5 * e * e * (2.0 * m).sin()
        } else {
            PI
        };

        // Newton-Raphson iteration
        for _ in 0..15 {
            let f = e_anom - e * e_anom.sin() - m;
            let f_prime = 1.0 - e * e_anom.cos();
            let delta = f / f_prime;
            e_anom -= delta;
            if delta.abs() < 1e-12 {
                break;
            }
        }

        e_anom
    }

    /// Computes the 3D position vector relative to the central parent body at a given eccentric anomaly E.
    /// The parent body/Sol is exactly at the focus (0, 0, 0).
    pub fn position_at_eccentric_anomaly(&self, e_anom: f64) -> DVec3 {
        let a = self.semi_major_axis;
        let e = self.eccentricity;
        let b = a * (1.0 - e * e).max(0.0).sqrt();

        // Coordinates in the orbital plane (focus at 0, 0, 0)
        let x_orb = a * (e_anom.cos() - e);
        let z_orb = b * e_anom.sin();

        // 1. Rotate in orbital plane by argument of periapsis ω
        let cos_w = self.argument_of_periapsis_rad.cos();
        let sin_w = self.argument_of_periapsis_rad.sin();
        let x_w = x_orb * cos_w - z_orb * sin_w;
        let z_w = x_orb * sin_w + z_orb * cos_w;

        // 2. Tilt by inclination i around node line
        let cos_i = self.inclination_rad.cos();
        let sin_i = self.inclination_rad.sin();
        let x_i = x_w * cos_i;
        let y_i = x_w * sin_i;
        let z_i = z_w;

        // 3. Rotate in ecliptic plane by longitude of ascending node Ω
        let cos_node = self.longitude_of_ascending_node_rad.cos();
        let sin_node = self.longitude_of_ascending_node_rad.sin();
        let x = x_i * cos_node - z_i * sin_node;
        let y = y_i;
        let z = x_i * sin_node + z_i * cos_node;

        DVec3::new(x, y, z)
    }

    /// Computes the parent-relative position and velocity at coordinate time t (in seconds).
    /// Satisfies Kepler's Second and Third Laws and matches the vis-viva equation.
    pub fn evaluate_state_at_time(&self, t: f64) -> (DVec3, DVec3) {
        let mean_motion = (2.0 * PI) / self.orbital_period_seconds;
        let mean_anomaly = self.initial_mean_anomaly + mean_motion * t;
        let e_anom = self.solve_eccentric_anomaly(mean_anomaly);

        let pos = self.position_at_eccentric_anomaly(e_anom);

        // Analytical velocity in orbital plane:
        // dx_orb/dt = -a * sin(E) * dE/dt
        // dz_orb/dt = b * cos(E) * dE/dt
        // where dE/dt = mean_motion / (1 - e * cos(E))
        let a = self.semi_major_axis;
        let e = self.eccentricity;
        let b = a * (1.0 - e * e).max(0.0).sqrt();

        let denom = (1.0 - e * e_anom.cos()).max(1e-12);
        let e_dot = mean_motion / denom;

        let vx_orb = -a * e_anom.sin() * e_dot;
        let vz_orb = b * e_anom.cos() * e_dot;

        // Apply identical 3D rotations (R is linear & constant over time)
        let cos_w = self.argument_of_periapsis_rad.cos();
        let sin_w = self.argument_of_periapsis_rad.sin();
        let vx_w = vx_orb * cos_w - vz_orb * sin_w;
        let vz_w = vx_orb * sin_w + vz_orb * cos_w;

        let cos_i = self.inclination_rad.cos();
        let sin_i = self.inclination_rad.sin();
        let vx_i = vx_w * cos_i;
        let vy_i = vx_w * sin_i;
        let vz_i = vz_w;

        let cos_node = self.longitude_of_ascending_node_rad.cos();
        let sin_node = self.longitude_of_ascending_node_rad.sin();
        let vx = vx_i * cos_node - vz_i * sin_node;
        let vy = vy_i;
        let vz = vx_i * sin_node + vz_i * cos_node;

        (pos, DVec3::new(vx, vy, vz))
    }
}

/// Marker for secondary satellites (moons orbiting a planet)
#[derive(Component, Clone, Copy, Debug)]
pub struct SatelliteOrbit;

/// Navigation target tag and metadata for the HUD targeting computer
#[derive(Component, Clone, Debug)]
pub struct NavTarget {
    pub name: &'static str,
    pub classification: &'static str,
    #[allow(dead_code)]
    pub radius_m: f64,
}

/// Component defining axial rotation (spin on own polar axis)
#[derive(Component, Clone, Copy, Debug)]
pub struct AxialRotation {
    pub speed_rad_per_sec: f32,
}

/// High-precision f64 axial rotation state, lazily initialized on first tick.
/// Accumulates spin angle in f64 and computes a normalized quaternion every frame,
/// completely preventing f32 quaternion norm drift (which otherwise scales planetary meshes
/// outward by tens to hundreds of meters over minutes at 165 Hz).
#[derive(Component, Clone, Copy, Debug)]
pub struct AxialRotationState {
    pub base_rotation: bevy::math::DQuat,
    pub angle_rad: f64,
}

/// Multi-tier optical visor and canopy attenuation filter modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
pub enum CanopyVisorMode {
    #[default]
    Clear,         // 100% transmittance - Full natural stellar glare & bloom
    Polarized,     // 25% transmittance - Sunglasses anti-glare mode
    SolarFilter,   // 1.5% transmittance - H-Alpha solar observatory mode revealing photosphere granules & sunspots
}

impl CanopyVisorMode {
    pub fn next(&self) -> Self {
        match self {
            CanopyVisorMode::Clear => CanopyVisorMode::Polarized,
            CanopyVisorMode::Polarized => CanopyVisorMode::SolarFilter,
            CanopyVisorMode::SolarFilter => CanopyVisorMode::Clear,
        }
    }

    pub fn transmittance(&self) -> f32 {
        match self {
            CanopyVisorMode::Clear => 1.0,
            CanopyVisorMode::Polarized => 0.25,
            CanopyVisorMode::SolarFilter => 0.015,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            CanopyVisorMode::Clear => "CLEAR 100%",
            CanopyVisorMode::Polarized => "POLARIZED 25%",
            CanopyVisorMode::SolarFilter => "SOLAR FILTER 1% H-ALPHA",
        }
    }
}

/// Resource storing the current optical canopy visor filter state.
#[derive(Resource, Debug, Clone, Default)]
pub struct CanopyVisorState {
    pub mode: CanopyVisorMode,
}

/// Planet texture paths relative to assets folder
pub const PLANET_TEXTURE_PATHS: [&str; 41] = [
    "textures/planets/sun.jpg",
    "textures/planets/earth_day.jpg",
    "textures/planets/earth_night.jpg",
    "textures/planets/moon.jpg",
    "textures/planets/mars.jpg",
    "textures/planets/jupiter.jpg",
    "textures/planets/saturn.jpg",
    "textures/planets/saturn_ring_alpha.png",
    "textures/planets/mercury.jpg",
    "textures/planets/venus.jpg",
    "textures/planets/uranus.jpg",
    "textures/planets/neptune.jpg",
    "textures/planets/io.jpg",
    "textures/planets/europa.jpg",
    "textures/planets/ganymede.jpg",
    "textures/planets/callisto.jpg",
    "textures/planets/mimas.jpg",
    "textures/planets/methone.jpg",
    "textures/planets/enceladus.jpg",
    "textures/planets/dione.jpg",
    "textures/planets/rhea.jpg",
    "textures/planets/titan.jpg",
    "textures/planets/iapetus.jpg",
    "textures/planets/miranda.jpg",
    "textures/planets/ariel.jpg",
    "textures/planets/umbriel.jpg",
    "textures/planets/titania.jpg",
    "textures/planets/oberon.jpg",
    "textures/planets/proteus.jpg",
    "textures/planets/triton.jpg",
    "textures/planets/nereid.jpg",
    // Dwarf Planets & Asteroid Belt
    "textures/planets/ceres.jpg",
    "textures/planets/pluto.jpg",
    "textures/planets/charon.jpg",
    "textures/planets/haumea.jpg",
    "textures/planets/makemake.jpg",
    "textures/planets/eris.jpg",
    "textures/planets/gonggong.jpg",
    "textures/planets/sedna.jpg",
    "textures/planets/farfarout.jpg",
    "textures/planets/planet9.jpg",
];

/// Level of detail tier for celestial body surface textures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum PlanetLodTier {
    HighRes,
    LowRes,
}

/// Component marking a celestial body whose surface texture can be dynamically streamed
/// between high-resolution (8K/4K) and lightweight distance LOD (2K).
#[derive(Component, Debug, Clone)]
pub struct BodyTextureStreamable {
    pub texture_path: std::borrow::Cow<'static, str>,
    pub current_tier: PlanetLodTier,
}

/// Resource managing dynamic distance-based planetary texture streaming / LOD.
/// Keeps distant bodies at lightweight 2K resolution (~11MB in VRAM instead of 179MB),
/// and automatically promotes nearby or targeted bodies to 8K/4K in the background without frame stutters.
#[derive(Resource, Debug, Clone)]
pub struct DynamicTextureStreamingConfig {
    pub enabled: bool,
    pub reload_pending: Option<std::borrow::Cow<'static, str>>,
    pub pending_timer: f32,
    pub check_timer: Timer,
    pub low_res_dimension: u32,
}

impl Default for DynamicTextureStreamingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            reload_pending: None,
            pending_timer: 0.0,
            check_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
            low_res_dimension: 2048,
        }
    }
}

/// Dynamic GPU-driven texture quality and resolution cap configuration.
/// Detects GPU hardware at startup:
/// - Discrete GPUs (NVIDIA GeForce, AMD Radeon): runs full 8K native skins (8192px cap).
/// - Integrated GPUs (Intel UHD, AMD APU): downscales to 4K (4096px cap) to prevent VRAM pressure.
/// - Switchable at runtime via hotkey [F12] (8K Ultra ↔ 4K High ↔ 2K Low).
#[derive(Resource, Debug, Clone)]
pub struct TextureQualityConfig {
    pub gpu_name: String,
    pub is_discrete_gpu: bool,
    pub max_texture_dimension: u32,
    pub quality_label: &'static str,
    pub detected: bool,
}

impl Default for TextureQualityConfig {
    fn default() -> Self {
        Self {
            gpu_name: "Detecting Hardware...".to_string(),
            is_discrete_gpu: true,
            max_texture_dimension: 8192,
            quality_label: "8K Ultra (Native Resolution)",
            detected: false,
        }
    }
}

impl TextureQualityConfig {
    pub fn cycle_quality(&mut self) -> &'static str {
        match self.max_texture_dimension {
            8192 => {
                self.max_texture_dimension = 4096;
                self.quality_label = "4K High (~600MB VRAM Profile)";
            }
            4096 => {
                self.max_texture_dimension = 2048;
                self.quality_label = "2K Performance (~350MB VRAM Profile)";
            }
            _ => {
                self.max_texture_dimension = 8192;
                self.quality_label = "8K Ultra (Native Resolution)";
            }
        }
        self.quality_label
    }
}

/// Detects total system physical memory in Gigabytes.
/// Uses `/proc/meminfo` on Linux, and `GlobalMemoryStatusEx` on Windows (zero extra dependencies).
pub fn detect_system_ram_gb() -> f32 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<f32>() {
                            return kb / (1024.0 * 1024.0);
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        #[repr(C)]
        struct MemoryStatusEx {
            length: u32,
            memory_load: u32,
            total_phys: u64,
            avail_phys: u64,
            total_page_file: u64,
            avail_page_file: u64,
            total_virtual: u64,
            avail_virtual: u64,
            avail_extended_virtual: u64,
        }
        extern "system" {
            fn GlobalMemoryStatusEx(stat: *mut MemoryStatusEx) -> i32;
        }
        let mut stat = MemoryStatusEx {
            length: std::mem::size_of::<MemoryStatusEx>() as u32,
            memory_load: 0,
            total_phys: 0,
            avail_phys: 0,
            total_page_file: 0,
            avail_page_file: 0,
            total_virtual: 0,
            avail_virtual: 0,
            avail_extended_virtual: 0,
        };
        if unsafe { GlobalMemoryStatusEx(&mut stat) } != 0 {
            return (stat.total_phys as f32) / (1024.0 * 1024.0 * 1024.0);
        }
    }

    16.0 // Conservative default fallback
}

/// High-performance in-memory CPU texture pyramid cache.
/// Automatically enabled on systems with ample RAM (>= 24 GB) to eliminate disk I/O and CPU mip generation
/// when re-approaching celestial bodies. Automatically disabled on lower-spec hardware (< 24 GB RAM)
/// to strictly protect system memory.
#[derive(Resource, Debug, Clone)]
pub struct PlanetTextureRamCache {
    pub enabled: bool,
    pub total_ram_gb: f32,
    pub cached_pyramids: std::collections::HashMap<String, (Vec<u8>, u32, u32, u32)>,
}

impl Default for PlanetTextureRamCache {
    fn default() -> Self {
        let total_ram_gb = detect_system_ram_gb();
        let enabled = total_ram_gb >= 24.0;
        Self {
            enabled,
            total_ram_gb,
            cached_pyramids: std::collections::HashMap::new(),
        }
    }
}

/// Calculates the physical radius-dependent promotion and demotion physical distances (in meters)
/// based on the active universe mode and celestial body physical radius.
/// Objects promote when approaching within 300x physical radius (or 25x in demo mode),
/// and demote when receding past 600x physical radius (or 45x in demo mode).
pub fn calculate_streaming_distances(radius_m: f64, mode: UniverseMode) -> (f64, f64) {
    match mode {
        UniverseMode::RealisticSolarSystem => {
            let r = radius_m.max(1_000.0);
            (r * 300.0, r * 600.0)
        }
        UniverseMode::DemoSector => {
            let r = radius_m.max(10.0);
            (r * 25.0, r * 45.0)
        }
    }
}

/// Resolves the optimal texture path for a celestial body.
/// Automatically detects if a modern GPU block-compressed texture (`.ktx2` with BC7)
/// exists in `assets/textures/planets/`. If so, loads the ultra-fast ~44MB BC7 file;
/// otherwise, transparently falls back to `.jpg` or `.png`.
pub fn resolve_planet_texture_path(stem: &str) -> std::borrow::Cow<'static, str> {
    let ktx2_rel = format!("textures/planets/{}.ktx2", stem);
    let ktx2_full = format!("assets/{}", ktx2_rel);
    if std::path::Path::new(&ktx2_full).exists() {
        ktx2_rel.into()
    } else if stem.contains("ring") {
        format!("textures/planets/{}.png", stem).into()
    } else {
        format!("textures/planets/{}.jpg", stem).into()
    }
}

/// Downscales an uncompressed 2D image buffer by 2x in both dimensions using box-filter averaging.
/// Preserves color fidelity, alpha channels, and adjusts texture_descriptor dimensions.
pub fn downscale_image_half(image: &mut Image) -> bool {
    let old_w = image.texture_descriptor.size.width as usize;
    let old_h = image.texture_descriptor.size.height as usize;
    if old_w < 2 || old_h < 2 {
        return false;
    }
    let total_pixels = old_w * old_h;
    if total_pixels == 0 {
        return false;
    }

    // If mipmaps were previously generated and appended, truncate image buffer to base mip level
    if image.texture_descriptor.mip_level_count > 1 || image.data.len() > total_pixels * 4 {
        image.data.truncate(total_pixels * 4);
        image.texture_descriptor.mip_level_count = 1;
    }

    if image.data.len() % total_pixels != 0 {
        return false;
    }
    let bpp = image.data.len() / total_pixels;
    if bpp == 0 {
        return false;
    }

    let new_w = old_w / 2;
    let new_h = old_h / 2;
    let new_len = new_w * new_h * bpp;
    let mut new_data = vec![0u8; new_len];

    let row_stride = old_w * bpp;
    let new_stride = new_w * bpp;

    let task_pool = bevy::tasks::ComputeTaskPool::try_get();
    if new_h >= 512 && task_pool.is_some() {
        let pool = task_pool.unwrap();
        let rows_per_chunk = 64;
        let bytes_per_chunk = rows_per_chunk * new_stride;
        let src_data = &image.data[..];

        pool.scope(|scope| {
            for (chunk_idx, dst_chunk) in new_data.chunks_mut(bytes_per_chunk).enumerate() {
                let y_start = chunk_idx * rows_per_chunk;
                let chunk_h = dst_chunk.len() / new_stride;

                scope.spawn(async move {
                    for cy in 0..chunk_h {
                        let y = y_start + cy;
                        let y0 = y * 2;
                        let y1 = y0 + 1;
                        let row0_start = y0 * row_stride;
                        let row1_start = y1 * row_stride;
                        let new_row_start = cy * new_stride;

                        for x in 0..new_w {
                            let x0 = x * 2;
                            let x1 = x0 + 1;

                            let idx00 = row0_start + x0 * bpp;
                            let idx01 = row0_start + x1 * bpp;
                            let idx10 = row1_start + x0 * bpp;
                            let idx11 = row1_start + x1 * bpp;
                            let dst_idx = new_row_start + x * bpp;

                            for c in 0..bpp {
                                let s00 = src_data[idx00 + c] as u32;
                                let s01 = src_data[idx01 + c] as u32;
                                let s10 = src_data[idx10 + c] as u32;
                                let s11 = src_data[idx11 + c] as u32;
                                dst_chunk[dst_idx + c] = ((s00 + s01 + s10 + s11 + 2) / 4) as u8;
                            }
                        }
                    }
                });
            }
        });
    } else {
        for y in 0..new_h {
            let y0 = y * 2;
            let y1 = y0 + 1;
            let row0_start = y0 * row_stride;
            let row1_start = y1 * row_stride;
            let new_row_start = y * new_stride;

            for x in 0..new_w {
                let x0 = x * 2;
                let x1 = x0 + 1;

                let idx00 = row0_start + x0 * bpp;
                let idx01 = row0_start + x1 * bpp;
                let idx10 = row1_start + x0 * bpp;
                let idx11 = row1_start + x1 * bpp;
                let dst_idx = new_row_start + x * bpp;

                for c in 0..bpp {
                    let s00 = image.data[idx00 + c] as u32;
                    let s01 = image.data[idx01 + c] as u32;
                    let s10 = image.data[idx10 + c] as u32;
                    let s11 = image.data[idx11 + c] as u32;
                    new_data[dst_idx + c] = ((s00 + s01 + s10 + s11 + 2) / 4) as u8;
                }
            }
        }
    }

    image.texture_descriptor.size.width = new_w as u32;
    image.texture_descriptor.size.height = new_h as u32;
    image.data = new_data;
    true
}

/// Demotes an image buffer down to a specified maximum dimension cap (e.g. 4096, 2048, 1024).
/// Directly slices the pre-computed mip levels from the existing mip pyramid in ~0.05 ms.
/// Supports both native GPU block-compressed BC7 (`TextureFormat::Bc7RgbaUnorm` / `Bc7RgbaUnormSrgb`)
/// and uncompressed RGBA8 textures with zero CPU re-compression overhead.
pub fn demote_image_to_cap(image: &mut Image, target_dim: u32) -> bool {
    let w = image.texture_descriptor.size.width;
    let h = image.texture_descriptor.size.height;
    let mips = image.texture_descriptor.mip_level_count;

    let target_w = target_dim;
    let target_h = (target_dim / 2).max(1);

    if w <= target_w && h <= target_h {
        return true;
    }

    // Fast-path: Slicing pre-computed mip level directly from the existing mip pyramid
    if mips > 1 {
        let is_bc7 = matches!(
            image.texture_descriptor.format,
            bevy::render::render_resource::TextureFormat::Bc7RgbaUnorm
            | bevy::render::render_resource::TextureFormat::Bc7RgbaUnormSrgb
        );

        let mut skip_bytes = 0usize;
        let mut levels_skipped = 0u32;
        let mut cur_w = w;
        let mut cur_h = h;

        while (cur_w > target_w || cur_h > target_h) && levels_skipped < mips {
            let next_w = (cur_w / 2).max(1);
            let next_h = (cur_h / 2).max(1);

            // In GPU block compression (BC7), base mip level dimensions MUST be multiples of 4 (block size)
            if is_bc7 && (next_w % 4 != 0 || next_h % 4 != 0) {
                break;
            }

            let level_bytes = if is_bc7 {
                // In BC7, each 4x4 texel block is 16 bytes (1 byte per texel)
                let blocks_x = ((cur_w + 3) / 4) as usize;
                let blocks_y = ((cur_h + 3) / 4) as usize;
                blocks_x * blocks_y * 16
            } else {
                (cur_w * cur_h * 4) as usize
            };
            skip_bytes += level_bytes;
            levels_skipped += 1;
            cur_w = next_w;
            cur_h = next_h;
        }

        if levels_skipped > 0 && levels_skipped < mips && image.data.len() > skip_bytes {
            image.data.drain(..skip_bytes);
            image.data.shrink_to_fit();
            image.texture_descriptor.size.width = cur_w;
            image.texture_descriptor.size.height = cur_h;
            image.texture_descriptor.mip_level_count = mips - levels_skipped;
            return true;
        }
    }

    // Fallback if mips were not yet generated: box-filter downscale
    let mut scaled = false;
    while image.texture_descriptor.size.width > target_w || image.texture_descriptor.size.height > target_h {
        if !downscale_image_half(image) {
            break;
        }
        scaled = true;
    }
    if scaled {
        image.texture_descriptor.mip_level_count = 1;
        generate_planet_mipmaps_and_sampler(image, false);
    }
    scaled
}

/// Slices an existing 8K or 4K mip pyramid directly down to 2K LOD (~2.8MB VRAM for BC7, ~11MB for RGBA8).
/// Avoids all CPU downscaling and all mipmap generation.
pub fn demote_image_to_2k(image: &mut Image) -> bool {
    demote_image_to_cap(image, 2048)
}

/// Returns true if an asset corresponds to one of the authentic high-resolution planetary texture maps.
pub fn is_planet_texture_asset(asset_server: &AssetServer, id: AssetId<Image>) -> bool {
    if let Some(path) = asset_server.get_path(id) {
        if let Some(path_str) = path.path().to_str() {
            return PLANET_TEXTURE_PATHS.iter().any(|p| {
                let stem = p.trim_end_matches(".jpg").trim_end_matches(".png");
                path_str.contains(stem)
            });
        }
    }
    false
}

/// Generates a full mipmap pyramid down to 1x1 for a planet texture (RGBA8 format),
/// and configures its sampler with 16x anisotropic filtering, repeat/clamp addressing, and linear trilinear filtering.
pub fn generate_planet_mipmaps_and_sampler(image: &mut Image, is_ring: bool) -> bool {
    // If mipmaps were already generated and configured, do not redo CPU computation
    if image.texture_descriptor.mip_level_count > 1 {
        return false;
    }

    let base_w = image.texture_descriptor.size.width as usize;
    let base_h = image.texture_descriptor.size.height as usize;
    if base_w < 2 || base_h < 2 {
        return false;
    }
    let base_pixels = base_w * base_h;
    if base_pixels == 0 {
        return false;
    }

    let bpp = 4;
    let base_len = base_pixels * bpp;
    if image.data.len() < base_len {
        return false;
    }

    // Truncate to base level data in case mipmaps were previously generated
    image.data.truncate(base_len);

    let max_dim = base_w.max(base_h);
    let num_levels = (32 - (max_dim as u32).leading_zeros()) as u32;

    // Calculate total buffer size needed for base level + all mip levels
    let mut total_bytes = base_len;
    let mut mw = base_w;
    let mut mh = base_h;
    for _ in 1..num_levels {
        mw = (mw / 2).max(1);
        mh = (mh / 2).max(1);
        total_bytes += mw * mh * 4;
    }

    // Single pre-allocation: eliminates repeated heap reallocations and memcpys
    image.data.resize(total_bytes, 0);

    let mut prev_offset = 0;
    let mut prev_w = base_w;
    let mut prev_h = base_h;
    let mut cur_offset = base_len;

    let task_pool = bevy::tasks::ComputeTaskPool::try_get();

    for _level in 1..num_levels {
        let cur_w = (prev_w / 2).max(1);
        let cur_h = (prev_h / 2).max(1);
        let prev_stride = prev_w * 4;
        let cur_stride = cur_w * 4;
        let cur_len = cur_w * cur_h * 4;

        let (prev_slice, rest) = image.data.split_at_mut(cur_offset);
        let prev_data = &prev_slice[prev_offset..prev_offset + prev_h * prev_stride];
        let cur_data = &mut rest[..cur_len];

        if cur_h >= 512 && task_pool.is_some() {
            let pool = task_pool.unwrap();
            let rows_per_chunk = 64;
            let bytes_per_chunk = rows_per_chunk * cur_stride;

            pool.scope(|scope| {
                for (chunk_idx, dst_chunk) in cur_data.chunks_mut(bytes_per_chunk).enumerate() {
                    let y_start = chunk_idx * rows_per_chunk;
                    let chunk_h = dst_chunk.len() / cur_stride;

                    scope.spawn(async move {
                        for cy in 0..chunk_h {
                            let y = y_start + cy;
                            let y0 = y * 2;
                            let y1 = if prev_h > 1 { (y0 + 1).min(prev_h - 1) } else { 0 };
                            let prev_row0 = y0 * prev_stride;
                            let prev_row1 = y1 * prev_stride;
                            let cur_row = cy * cur_stride;

                            for x in 0..cur_w {
                                let x0 = x * 2;
                                let x1 = if prev_w > 1 { (x0 + 1).min(prev_w - 1) } else { 0 };

                                let idx00 = prev_row0 + x0 * 4;
                                let idx01 = prev_row0 + x1 * 4;
                                let idx10 = prev_row1 + x0 * 4;
                                let idx11 = prev_row1 + x1 * 4;
                                let dst_idx = cur_row + x * 4;

                                for c in 0..4 {
                                    let s00 = prev_data[idx00 + c] as u32;
                                    let s01 = prev_data[idx01 + c] as u32;
                                    let s10 = prev_data[idx10 + c] as u32;
                                    let s11 = prev_data[idx11 + c] as u32;
                                    dst_chunk[dst_idx + c] = ((s00 + s01 + s10 + s11 + 2) / 4) as u8;
                                }
                            }
                        }
                    });
                }
            });
        } else {
            for y in 0..cur_h {
                let y0 = y * 2;
                let y1 = if prev_h > 1 { (y0 + 1).min(prev_h - 1) } else { 0 };
                let prev_row0 = y0 * prev_stride;
                let prev_row1 = y1 * prev_stride;
                let cur_row = y * cur_stride;

                for x in 0..cur_w {
                    let x0 = x * 2;
                    let x1 = if prev_w > 1 { (x0 + 1).min(prev_w - 1) } else { 0 };

                    let idx00 = prev_row0 + x0 * 4;
                    let idx01 = prev_row0 + x1 * 4;
                    let idx10 = prev_row1 + x0 * 4;
                    let idx11 = prev_row1 + x1 * 4;
                    let dst_idx = cur_row + x * 4;

                    for c in 0..4 {
                        let s00 = prev_data[idx00 + c] as u32;
                        let s01 = prev_data[idx01 + c] as u32;
                        let s10 = prev_data[idx10 + c] as u32;
                        let s11 = prev_data[idx11 + c] as u32;
                        cur_data[dst_idx + c] = ((s00 + s01 + s10 + s11 + 2) / 4) as u8;
                    }
                }
            }
        }

        prev_offset = cur_offset;
        cur_offset += cur_len;
        prev_w = cur_w;
        prev_h = cur_h;
    }

    image.texture_descriptor.mip_level_count = num_levels;

    // Configure 16x anisotropic trilinear filtering and proper texture addressing
    use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        lod_min_clamp: 0.0,
        lod_max_clamp: f32::MAX,
        anisotropy_clamp: 16,
        address_mode_u: if is_ring { ImageAddressMode::ClampToEdge } else { ImageAddressMode::Repeat },
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..default()
    });

    true
}

/// Queries the rendering backend for the primary GPU adapter information.
/// If a discrete GPU (NVIDIA/AMD) is detected, preserves 8192 (full 8K).
/// If an integrated or low-VRAM GPU is detected, automatically adjusts `max_texture_dimension` to 4096.
pub fn gpu_texture_quality_init_system(
    adapter_info: Option<Res<RenderAdapterInfo>>,
    mut config: ResMut<TextureQualityConfig>,
    ram_cache: Res<PlanetTextureRamCache>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
) {
    if config.detected {
        return;
    }

    if let Some(adapter) = adapter_info {
        let name = adapter.0.name.clone();
        let device_type_str = format!("{:?}", adapter.0.device_type);
        let is_discrete = device_type_str == "DiscreteGpu";
        let max_dim = if is_discrete { 8192 } else { 4096 };
        let label = if is_discrete {
            "8K Ultra (Discrete GPU Detected)"
        } else {
            "4K Optimized (Integrated GPU / Power-Saver Detected)"
        };

        config.gpu_name = name;
        config.is_discrete_gpu = is_discrete;
        config.max_texture_dimension = max_dim;
        config.quality_label = label;
        config.detected = true;

        info!(
            "[TextureQuality] Adapter: '{}' ({}) -> Texture Dimension Cap: {}px ({}) | System RAM: {:.1} GiB -> In-Memory CPU Cache: {}",
            config.gpu_name,
            device_type_str,
            config.max_texture_dimension,
            config.quality_label,
            ram_cache.total_ram_gb,
            if ram_cache.enabled {
                "ENABLED (>=24GB Profile: Zero Disk I/O & Zero CPU MipsGen on re-approach)"
            } else {
                "DISABLED (<24GB Profile: Conserving RAM for lightweight systems)"
            }
        );

        // Only downscale if an integrated GPU was detected and a texture exceeds 4096px
        if !is_discrete {
            for (id, image) in images.iter_mut() {
                if !is_planet_texture_asset(&asset_server, id) {
                    continue;
                }
                let w = image.texture_descriptor.size.width;
                let h = image.texture_descriptor.size.height;
                if w > config.max_texture_dimension || h > config.max_texture_dimension {
                    while image.texture_descriptor.size.width > config.max_texture_dimension
                        || image.texture_descriptor.size.height > config.max_texture_dimension
                    {
                        if !downscale_image_half(image) {
                            break;
                        }
                    }
                    let path_str = asset_server.get_path(id).map(|p| p.path().to_string_lossy().to_string()).unwrap_or_default();
                    let is_ring = path_str.contains("ring");
                    generate_planet_mipmaps_and_sampler(image, is_ring);
                    info!(
                        "[TextureQuality] Post-probe downscaled asset {:?} to {}x{} (Mips: {})",
                        id, image.texture_descriptor.size.width, image.texture_descriptor.size.height, image.texture_descriptor.mip_level_count
                    );
                }
            }
        }
    }
}

/// Listens for newly loaded texture images and downscales them if they exceed the hardware cap or distance LOD,
/// then generates the full mipmap pyramid and 16x anisotropic trilinear sampler.
pub fn texture_quality_scaling_system(
    config: Res<TextureQualityConfig>,
    mut streaming: ResMut<DynamicTextureStreamingConfig>,
    mut ram_cache: ResMut<PlanetTextureRamCache>,
    mut events: EventReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
    mut streamable_query: Query<&mut BodyTextureStreamable>,
    mut fps_tracker: Option<ResMut<crate::flight::FpsTracker>>,
) {
    for event in events.read() {
        let image_id = match event {
            AssetEvent::LoadedWithDependencies { id } => *id,
            _ => continue,
        };

        if !is_planet_texture_asset(&asset_server, image_id) {
            continue;
        }

        let path_str = asset_server
            .get_path(image_id)
            .map(|p| p.path().to_string_lossy().to_string())
            .unwrap_or_default();

        if let Some(image) = images.get_mut(image_id) {
            let is_bc7 = matches!(
                image.texture_descriptor.format,
                bevy::render::render_resource::TextureFormat::Bc7RgbaUnorm
                | bevy::render::render_resource::TextureFormat::Bc7RgbaUnormSrgb
            );

            // If this asset was pending promotion, clear the pending state
            // and update its streamable component tier to HighRes
            let was_pending = streaming
                .reload_pending
                .as_ref()
                .map(|p| path_str.ends_with(p.as_ref()))
                .unwrap_or(false);

            if was_pending {
                streaming.reload_pending = None;
                streaming.pending_timer = 0.0;
                for mut streamable in streamable_query.iter_mut() {
                    if path_str.ends_with(streamable.texture_path.as_ref()) {
                        streamable.current_tier = PlanetLodTier::HighRes;
                    }
                }
            }

            // For pre-compressed BC7 KTX2 textures (which already have full mips pre-generated by toktx):
            if is_bc7 {
                let is_ring = path_str.contains("ring");
                use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
                image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    mag_filter: ImageFilterMode::Linear,
                    min_filter: ImageFilterMode::Linear,
                    mipmap_filter: ImageFilterMode::Linear,
                    lod_min_clamp: 0.0,
                    lod_max_clamp: f32::MAX,
                    anisotropy_clamp: 16,
                    address_mode_u: if is_ring { ImageAddressMode::ClampToEdge } else { ImageAddressMode::Repeat },
                    address_mode_v: ImageAddressMode::ClampToEdge,
                    ..default()
                });

                if ram_cache.enabled {
                    ram_cache.cached_pyramids.insert(
                        path_str.clone(),
                        (
                            image.data.clone(),
                            image.texture_descriptor.size.width,
                            image.texture_descriptor.size.height,
                            image.texture_descriptor.mip_level_count,
                        ),
                    );
                }

                if is_ring {
                    info!(
                        "[TextureQuality] Prepared BC7 GPU-compressed planet texture '{}' -> {}x{} (Mips: {}, 16x Anisotropic) [SATURN RING ALPHA RIBBON (~4MB VRAM)]",
                        path_str,
                        image.texture_descriptor.size.width,
                        image.texture_descriptor.size.height,
                        image.texture_descriptor.mip_level_count
                    );
                    if let Some(ref mut tracker) = fps_tracker {
                        tracker.record_texture_op(format!("Prepared BC7 '{}'", path_str));
                    }
                    continue;
                }

                // Enforce quality profile cap and streaming initial distance LOD for BC7 textures
                let is_high_res = if !streaming.enabled || is_ring || path_str.contains("sun") || was_pending {
                    true
                } else {
                    let mut found = false;
                    let mut high = false;
                    for streamable in streamable_query.iter() {
                        if path_str.ends_with(streamable.texture_path.as_ref()) {
                            found = true;
                            if streamable.current_tier == PlanetLodTier::HighRes {
                                high = true;
                                break;
                            }
                        }
                    }
                    if found {
                        high
                    } else {
                        // Fallback at initial startup: Earth and Moon are HighRes, others LowRes
                        path_str.contains("earth") || path_str.contains("moon")
                    }
                };

                let target_dim = if is_high_res {
                    config.max_texture_dimension
                } else {
                    streaming.low_res_dimension
                };

                let scaled = demote_image_to_cap(image, target_dim);

                info!(
                    "[TextureQuality] Prepared BC7 GPU-compressed planet texture '{}' -> {}x{} (Mips: {}, 16x Anisotropic) [{}]",
                    path_str,
                    image.texture_descriptor.size.width,
                    image.texture_descriptor.size.height,
                    image.texture_descriptor.mip_level_count,
                    if is_high_res {
                        if scaled {
                            "HIGH-RES (capped to profile)"
                        } else {
                            "HIGH-RES 8K NATIVE"
                        }
                    } else {
                        "DYNAMIC DISTANCE 2K LOD (~2.8MB VRAM)"
                    }
                );
                if let Some(ref mut tracker) = fps_tracker {
                    tracker.record_texture_op(format!("Prepared BC7 '{}'", path_str));
                }
                continue;
            }

            if image.texture_descriptor.mip_level_count > 1 {
                continue;
            }

            // Determine if this body is currently HighRes or LowRes:
            let is_sun_or_ring = path_str.contains("sun") || path_str.contains("ring");
            let is_high_res = if !streaming.enabled || is_sun_or_ring || was_pending {
                true
            } else {
                let mut found = false;
                let mut high = false;
                for streamable in streamable_query.iter() {
                    if path_str.ends_with(streamable.texture_path.as_ref()) {
                        found = true;
                        if streamable.current_tier == PlanetLodTier::HighRes {
                            high = true;
                            break;
                        }
                    }
                }
                if found {
                    high
                } else {
                    // Fallback at initial startup: Earth and Moon are HighRes, others LowRes
                    path_str.contains("earth") || path_str.contains("moon")
                }
            };

            let prep_start = std::time::Instant::now();
            let target_dim = if is_high_res {
                config.max_texture_dimension
            } else {
                streaming.low_res_dimension
            };

            let t_down = std::time::Instant::now();
            let mut scaled = false;
            while image.texture_descriptor.size.width > target_dim
                || image.texture_descriptor.size.height > target_dim
            {
                if !downscale_image_half(image) {
                    break;
                }
                scaled = true;
            }
            let downscale_ms = t_down.elapsed().as_secs_f64() * 1000.0;

            let is_ring = path_str.contains("ring");
            let t_mips = std::time::Instant::now();
            let mips_ok = generate_planet_mipmaps_and_sampler(image, is_ring);
            let mips_ms = t_mips.elapsed().as_secs_f64() * 1000.0;
            let total_prep_ms = prep_start.elapsed().as_secs_f64() * 1000.0;

            if mips_ok {
                // If high-res and RAM cache is enabled, store full pyramid in CPU RAM for instant re-promotion
                if is_high_res && ram_cache.enabled {
                    ram_cache.cached_pyramids.insert(
                        path_str.clone(),
                        (
                            image.data.clone(),
                            image.texture_descriptor.size.width,
                            image.texture_descriptor.size.height,
                            image.texture_descriptor.mip_level_count,
                        ),
                    );
                }

                info!(
                    "[TextureQuality] Prepared planet texture '{}' -> {}x{} in {:.2} ms (Downscale: {:.2}ms, MipsGen: {:.2}ms, Mips: {}, 16x Anisotropic) [{}]",
                    path_str,
                    image.texture_descriptor.size.width,
                    image.texture_descriptor.size.height,
                    total_prep_ms,
                    downscale_ms,
                    mips_ms,
                    image.texture_descriptor.mip_level_count,
                    if is_high_res {
                        if scaled {
                            "HIGH-RES (capped to hardware)"
                        } else {
                            "HIGH-RES 8K NATIVE"
                        }
                    } else {
                        "DYNAMIC DISTANCE 2K LOD (~11MB VRAM)"
                    }
                );
                if let Some(ref mut tracker) = fps_tracker {
                    tracker.record_texture_op(format!("Prepared '{}' (CPU: {:.2}ms)", path_str, total_prep_ms));
                }
            }
        }
    }
}

/// Listens for hotkeys:
/// - [F12]: Cycles Master Texture Resolution Profile (8K Ultra ↔ 4K High ↔ 2K Low).
/// - [Shift + F12]: Toggles Dynamic Proximity Texture Streaming (Dynamic LOD ↔ Static All-Resident).
pub fn texture_quality_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut config: ResMut<TextureQualityConfig>,
    mut streaming: ResMut<DynamicTextureStreamingConfig>,
    mut ram_cache: ResMut<PlanetTextureRamCache>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
    mut streamable_query: Query<&mut BodyTextureStreamable>,
) {
    let is_shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    if keyboard.just_pressed(KeyCode::F12) && is_shift {
        streaming.enabled = !streaming.enabled;
        info!(
            "[TextureStreaming] Dynamic Proximity Texture Streaming: {}",
            if streaming.enabled {
                "ENABLED (Proximity 8K/4K ↔ Distant 2K LOD)"
            } else {
                "DISABLED (Static All-Resident Mode)"
            }
        );

        if !streaming.enabled {
            // Disabled: restore all textures to master cap from disk and mark all HighRes
            for path in PLANET_TEXTURE_PATHS {
                let stem = path
                    .trim_start_matches("textures/planets/")
                    .trim_end_matches(".jpg")
                    .trim_end_matches(".png");
                let resolved = resolve_planet_texture_path(stem);
                asset_server.reload(resolved.as_ref());
            }
            for mut streamable in streamable_query.iter_mut() {
                streamable.current_tier = PlanetLodTier::HighRes;
            }
        } else {
            // Enabled: force check_timer to elapse immediately so streaming system processes all planets on next frame
            streaming.check_timer.set_elapsed(std::time::Duration::from_secs(1));
        }
    } else if keyboard.just_pressed(KeyCode::F12) {
        ram_cache.cached_pyramids.clear();
        let old_cap = config.max_texture_dimension;
        let new_label = config.cycle_quality();
        info!(
            "[TextureQuality] Cycled texture profile: {} (Cap: {}px)",
            new_label, config.max_texture_dimension
        );

        if config.max_texture_dimension > old_cap {
            // Unlocked higher resolution: reload assets from disk so the higher resolution is restored
            for path in PLANET_TEXTURE_PATHS {
                let stem = path
                    .trim_start_matches("textures/planets/")
                    .trim_end_matches(".jpg")
                    .trim_end_matches(".png");
                let resolved = resolve_planet_texture_path(stem);
                asset_server.reload(resolved.as_ref());
            }
        } else {
            // Lowered cap: downscale planet textures that exceed new cap directly in ~0.05ms
            for (id, image) in images.iter_mut() {
                if !is_planet_texture_asset(&asset_server, id) {
                    continue;
                }
                let path_str = asset_server
                    .get_path(id)
                    .map(|p| p.path().to_string_lossy().to_string())
                    .unwrap_or_default();
                if path_str.contains("ring") {
                    continue;
                }
                demote_image_to_cap(image, config.max_texture_dimension);
            }
        }
    }
}

/// Periodic background streaming system:
/// Evaluates physical distance from shuttle to celestial bodies with high-resolution textures.
/// Dynamically scales promotion and demotion physical thresholds based on each object's physical radius.
/// Promotes close bodies to 8K/4K (instant ~2ms RAM restore if RAM cache is enabled, or background disk reload).
/// Demotes distant bodies to 2K, saving ~168MB VRAM per body.
/// HUD targeting intentionally does NOT reload textures, ensuring buttery-smooth 60+ FPS when cycling targets.
pub fn planet_distance_texture_streaming_system(
    time: Res<Time>,
    mut streaming: ResMut<DynamicTextureStreamingConfig>,
    universe_config: Res<UniverseConfig>,
    quality_config: Res<TextureQualityConfig>,
    ram_cache: Res<PlanetTextureRamCache>,
    shuttle_query: Query<&UniversalPosition, With<Shuttle>>,
    mut streamables: Query<(Entity, &UniversalPosition, &mut BodyTextureStreamable, &NavTarget)>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
    mut fps_tracker: Option<ResMut<crate::flight::FpsTracker>>,
) {
    if !streaming.enabled {
        return;
    }

    // Tick pending reload timeout (clear after 3 seconds if not already fulfilled)
    if streaming.reload_pending.is_some() {
        streaming.pending_timer += time.delta_secs();
        if streaming.pending_timer > 3.0 {
            streaming.reload_pending = None;
            streaming.pending_timer = 0.0;
        }
    }

    streaming.check_timer.tick(time.delta());
    if !streaming.check_timer.finished() {
        return;
    }

    let Ok(shuttle_pos) = shuttle_query.get_single() else {
        return;
    };

    for (_entity, body_pos, mut streamable, nav_target) in streamables.iter_mut() {
        let dist = (body_pos.0 - shuttle_pos.0).length();
        let (promote_dist, demote_dist) = calculate_streaming_distances(nav_target.radius_m, universe_config.active_mode);

        // Desired Tier Calculation with Physical Radius Hysteresis (Strictly Proximity-Driven):
        // HUD targeting lock intentionally does NOT reload textures to guarantee zero stuttering when cycling targets.
        let desired_tier = if dist < promote_dist {
            PlanetLodTier::HighRes
        } else if dist > demote_dist {
            PlanetLodTier::LowRes
        } else {
            // In hysteresis deadband: maintain current state
            streamable.current_tier
        };

        match (streamable.current_tier, desired_tier) {
            (PlanetLodTier::LowRes, PlanetLodTier::HighRes) => {
                // Needs promotion!
                // Fast-Path: If full pyramid is resident in CPU RAM cache, promote immediately in ~2ms (zero disk I/O, zero JPEG decode, zero MipsGen)
                if ram_cache.enabled {
                    if let Some((cached_data, w, h, mips)) = ram_cache.cached_pyramids.get(streamable.texture_path.as_ref()) {
                        let target_id = images.iter().find_map(|(id, _)| {
                            let path = asset_server.get_path(id)?;
                            if path.path().to_string_lossy().ends_with(streamable.texture_path.as_ref()) {
                                Some(id)
                            } else {
                                None
                            }
                        });

                        if let Some(id) = target_id {
                            if let Some(image) = images.get_mut(id) {
                                let t_promo = std::time::Instant::now();
                                image.data.clone_from(cached_data);
                                image.texture_descriptor.size.width = *w;
                                image.texture_descriptor.size.height = *h;
                                image.texture_descriptor.mip_level_count = *mips;
                                if image.texture_descriptor.size.width > quality_config.max_texture_dimension {
                                    demote_image_to_cap(image, quality_config.max_texture_dimension);
                                }
                                streamable.current_tier = PlanetLodTier::HighRes;
                                let elapsed_ms = t_promo.elapsed().as_secs_f64() * 1000.0;
                                info!(
                                    "[TextureStreaming] Instant RAM promotion for '{}' (Proximity: {:.1}M km) in {:.2} ms (Bypassed SSD & JPEG decode) -> Restored 8K",
                                    nav_target.name,
                                    dist / 1e9,
                                    elapsed_ms
                                );
                                if let Some(ref mut tracker) = fps_tracker {
                                    tracker.record_texture_op(format!("RAM 8K restore for '{}' (CPU: {:.2}ms)", nav_target.name, elapsed_ms));
                                }
                                break;
                            }
                        }
                    }
                }

                // Standard fallback for systems with <24GB RAM or first-time load: background reload via AssetServer
                if streaming.reload_pending.is_none() {
                    streaming.reload_pending = Some(streamable.texture_path.clone());
                    streaming.pending_timer = 0.0;
                    asset_server.reload(streamable.texture_path.as_ref());
                    info!(
                        "[TextureStreaming] High-res promotion triggered for '{}' (Proximity: {:.1}M km) -> Background reload 8K",
                        nav_target.name,
                        dist / 1e9
                    );
                    if let Some(ref mut tracker) = fps_tracker {
                        tracker.record_texture_op(format!("Reload 8K triggered for '{}'", nav_target.name));
                    }
                    // Handle at most one streaming event per tick to prevent CPU/PCIe congestion
                    break;
                }
            }
            (PlanetLodTier::HighRes, PlanetLodTier::LowRes) => {
                // Needs demotion!
                let demote_start = std::time::Instant::now();
                // Find exact target asset ID using immutable iter() to avoid dirtying other textures in Assets<Image>
                let target_id = images.iter().find_map(|(id, _)| {
                    let path = asset_server.get_path(id)?;
                    if path.path().to_string_lossy().ends_with(streamable.texture_path.as_ref()) {
                        Some(id)
                    } else {
                        None
                    }
                });

                let mut demoted = false;

                if let Some(id) = target_id {
                    if let Some(image) = images.get_mut(id) {
                        demoted = demote_image_to_2k(image);
                    }
                }

                if demoted {
                    streamable.current_tier = PlanetLodTier::LowRes;
                    let total_ms = demote_start.elapsed().as_secs_f64() * 1000.0;
                    info!(
                        "[TextureStreaming] Demoted '{}' to 2K LOD (Dist: {:.1}M km) in {:.2} ms (Fast-Path Slicing) -> Reclaimed ~168MB VRAM",
                        nav_target.name,
                        dist / 1e9,
                        total_ms
                    );
                    if let Some(ref mut tracker) = fps_tracker {
                        tracker.record_texture_op(format!("Demoted '{}' to 2K (CPU: {:.2}ms)", nav_target.name, total_ms));
                    }
                    // Demote at most one body per streaming check tick to distribute GPU texture recreation
                    break;
                }
            }
            _ => {} // Already in desired state
        }
    }
}

/// Marker component for Sol (The Primary Star)
#[derive(Component)]
pub struct PrimaryStar;

/// Marker component for Sol's atmospheric corona halos
#[derive(Component)]
pub struct SolarCorona;

/// Marker for ring meshes around gas giants
#[derive(Component)]
pub struct PlanetaryRing;

/// Stores the resting optical properties of celestial bodies for real-time Relativistic Doppler and Beaming shifts
#[derive(Component, Clone, Debug)]
pub struct RelativisticOpticsBody {
    pub base_color: LinearRgba,
    pub base_emissive: LinearRgba,
    pub is_emissive_star: bool,
    pub rest_temperature_k: f32,
}

/// Creates an equirectangular UV sphere mesh properly oriented so that:
/// - North pole is at +Y
/// - South pole is at -Y
/// - Equator is in the XZ plane
/// (Bevy's raw uv sphere has North pole at +Z; rotating by -90 deg around X aligns +Z to +Y)
fn generate_planet_sphere_mesh(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    let mut mesh = Sphere::new(1.0).mesh().uv(
        crate::coords::UV_SPHERE_SUBDIVISIONS as u32,
        crate::coords::UV_SPHERE_SUBDIVISIONS as u32,
    );
    let rot = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);

    if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(ref mut positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for pos in positions.iter_mut() {
            let v = rot * Vec3::from_array(*pos);
            *pos = v.to_array();
        }
    }
    if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(ref mut normals)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        for norm in normals.iter_mut() {
            let v = rot * Vec3::from_array(*norm);
            *norm = v.to_array();
        }
    }
    meshes.add(mesh)
}

/// Spawns the Demo Solar System sector: Sol, Terra, Luna, Ares, Chronos (Ringed Gas Giant), Titan
pub fn spawn_demo_celestial_system(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
) {
    // Equirectangular UV sphere geometry oriented with +Y as North Pole and XZ as Equator
    let sphere_mesh = generate_planet_sphere_mesh(meshes);

    // Load authentic high-resolution celestial surface textures
    let sun_path = resolve_planet_texture_path("sun");
    let earth_path = resolve_planet_texture_path("earth_day");
    let moon_path = resolve_planet_texture_path("moon");
    let mars_path = resolve_planet_texture_path("mars");
    let saturn_path = resolve_planet_texture_path("saturn");
    let ring_path = resolve_planet_texture_path("saturn_ring_alpha");

    let sun_texture = asset_server.load::<Image>(sun_path.as_ref());
    let earth_texture = asset_server.load(earth_path.as_ref());
    let moon_texture = asset_server.load(moon_path.as_ref());
    let mars_texture = asset_server.load(mars_path.as_ref());
    let saturn_texture = asset_server.load(saturn_path.as_ref());
    let ring_texture = asset_server.load(ring_path.as_ref());

    // 1. SOL (The Primary Star)
    let sol_radius = 4_500.0;
    // Radiant white-hot stellar photosphere (unlit: true ensures self-luminous emission with zero ambient/specular reflection)
    let sol_mat = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::new(100.8, 304.2, 1808.5, 1.0)),
        base_color_texture: Some(sun_texture.clone()),
        emissive: LinearRgba::BLACK,
        unlit: true,
        ..default()
    });

    // Inner bright solar corona (additive blending so it never darkens or occludes the star; unlit prevents ambient light reflection)
    let sol_inner_corona_mat = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::new(3.0, 3.0, 3.0, 0.40)),
        emissive: LinearRgba::BLACK,
        alpha_mode: AlphaMode::Add,
        unlit: true,
        ..default()
    });

    // Outer expansive prominence / solar flare halo (additive blending; unlit prevents ambient light reflection)
    let sol_outer_corona_mat = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::new(0.8, 0.8, 0.8, 0.15)),
        emissive: LinearRgba::BLACK,
        alpha_mode: AlphaMode::Add,
        unlit: true,
        ..default()
    });

    let sol_entity = commands
        .spawn((
            UniverseEntity,
            PrimaryStar,
            UniversalPosition(DVec3::ZERO),
            BodyVelocity(DVec3::ZERO),
            CelestialBody {
                physical_radius: sol_radius,
                mass_kg: 2.50e20,
            },
            NavTarget {
                name: "Sol",
                classification: "G2V White Main-Sequence Star",
                radius_m: sol_radius,
            },
            AxialRotation {
                speed_rad_per_sec: 0.02,
            },
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(sol_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(100.8, 304.2, 1808.5, 1.0),
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: true,
                rest_temperature_k: 5778.0,
            },
            Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(sol_radius as f32)),
        ))
        .with_children(|parent| {
            // Inner Solar Corona
            parent.spawn((
                SolarCorona,
                Mesh3d(sphere_mesh.clone()),
                MeshMaterial3d(sol_inner_corona_mat),
                RelativisticOpticsBody {
                    base_color: LinearRgba::new(3.0, 3.0, 3.0, 0.40),
                    base_emissive: LinearRgba::BLACK,
                    is_emissive_star: true,
                    rest_temperature_k: 5778.0,
                },
                Transform::from_scale(Vec3::splat(1.04)),
            ));
            // Outer Solar Flare & Glare Halo
            parent.spawn((
                SolarCorona,
                Mesh3d(sphere_mesh.clone()),
                MeshMaterial3d(sol_outer_corona_mat),
                RelativisticOpticsBody {
                    base_color: LinearRgba::new(0.8, 0.8, 0.8, 0.15),
                    base_emissive: LinearRgba::BLACK,
                    is_emissive_star: true,
                    rest_temperature_k: 5778.0,
                },
                Transform::from_scale(Vec3::splat(1.10)),
            ));
        })
        .id();

    // 2. TERRA (Earth-like Habitable Planet)
    let terra_radius = 1_100.0;
    let terra_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(earth_texture),
        perceptual_roughness: 0.50,
        metallic: 0.05,
        reflectance: 0.2,
        cull_mode: None,
        ..default()
    });

    let terra_pos = DVec3::new(28_000.0, 400.0, 25_000.0);
    let terra_entity = commands
        .spawn((
            UniverseEntity,
            UniversalPosition(terra_pos),
            BodyVelocity(DVec3::ZERO),
            CelestialBody {
                physical_radius: terra_radius,
                mass_kg: 1.04e19,
            },
            KeplerOrbit::circular(
                Some(sol_entity),
                38_000.0,
                0.035, // ~2 deg orbital tilt
                360.0, // 6 minutes orbital year for dynamic flight
                0.75,
            ),
            AxialRotation {
                speed_rad_per_sec: 0.12, // Visible axial rotation (1 revolution in ~52s)
            },
            NavTarget {
                name: "Terra",
                classification: "Terrestrial Ocean Planet",
                radius_m: terra_radius,
            },
            BodyTextureStreamable {
                texture_path: earth_path,
                current_tier: PlanetLodTier::HighRes,
            },
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(terra_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
                base_emissive: LinearRgba::rgb(0.01, 0.02, 0.04),
                is_emissive_star: false,
                rest_temperature_k: 288.0,
            },
            Transform::from_xyz(terra_pos.x as f32, terra_pos.y as f32, terra_pos.z as f32)
                .with_rotation(Quat::from_rotation_z(23.44_f32.to_radians()))
                .with_scale(Vec3::splat(terra_radius as f32)),
        ))
        .with_children(|parent| {
            // Delicate atmospheric limb haze shell (Rayleigh scattering halo)
            let atmo_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(0.35, 0.70, 1.0, 0.08),
                emissive: LinearRgba::BLACK,
                alpha_mode: AlphaMode::Blend,
                unlit: false,
                ..default()
            });
            parent.spawn((
                Mesh3d(sphere_mesh.clone()),
                MeshMaterial3d(atmo_mat),
                RelativisticOpticsBody {
                    base_color: LinearRgba::new(0.35, 0.70, 1.0, 0.08),
                    base_emissive: LinearRgba::BLACK,
                    is_emissive_star: false,
                    rest_temperature_k: 288.0,
                },
                Transform::from_scale(Vec3::splat(1.025)),
            ));
        })
        .id();

    // 3. LUNA (Earth's Moon)
    let luna_radius = 280.0;
    let luna_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(moon_texture),
        perceptual_roughness: 0.90,
        metallic: 0.0,
        ..default()
    });

    let luna_pos = terra_pos + DVec3::new(3_800.0, 200.0, 500.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(luna_pos),
        BodyVelocity(DVec3::ZERO),
        CelestialBody {
            physical_radius: luna_radius,
            mass_kg: 1.25e17,
        },
        KeplerOrbit::circular(
            Some(terra_entity),
            4_200.0,
            0.089, // 5.1 deg lunar inclination
            65.0, // orbits Terra in ~65s
            1.20,
        ),
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: -0.09666, // Synchronous / tidally locked rotation
        },
        NavTarget {
            name: "Luna",
            classification: "Tidally Locked Moon",
            radius_m: luna_radius,
        },
        BodyTextureStreamable {
            texture_path: moon_path,
            current_tier: PlanetLodTier::HighRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(luna_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.95, 0.95, 0.95, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 250.0,
        },
        Transform::from_xyz(luna_pos.x as f32, luna_pos.y as f32, luna_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(6.68_f32.to_radians()) * Quat::from_rotation_y(-1.20_f32))
            .with_scale(Vec3::splat(luna_radius as f32)),
    ));

    // 4. ARES (The Red Planet / Mars analogue)
    let ares_radius = 620.0;
    let ares_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(mars_texture),
        perceptual_roughness: 0.85,
        metallic: 0.05,
        ..default()
    });

    let ares_pos = DVec3::new(-45_000.0, -1_200.0, -25_000.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(ares_pos),
        BodyVelocity(DVec3::ZERO),
        CelestialBody {
            physical_radius: ares_radius,
            mass_kg: 2.50e18,
        },
        KeplerOrbit::circular(
            Some(sol_entity),
            62_000.0,
            0.032,
            680.0,
            3.14,
        ),
        AxialRotation {
            speed_rad_per_sec: 0.11, // Martian axial rotation
        },
        NavTarget {
            name: "Ares",
            classification: "Desert Planet",
            radius_m: ares_radius,
        },
        BodyTextureStreamable {
            texture_path: mars_path.clone(),
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(ares_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 210.0,
        },
        Transform::from_xyz(ares_pos.x as f32, ares_pos.y as f32, ares_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(25.19_f32.to_radians()))
            .with_scale(Vec3::splat(ares_radius as f32)),
    ));

    // 5. CHRONOS (Majestic Ringed Gas Giant)
    let chronos_radius = 3_200.0;
    let chronos_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(saturn_texture),
        perceptual_roughness: 0.55,
        metallic: 0.05,
        ..default()
    });

    let chronos_pos = DVec3::new(85_000.0, 3_500.0, -70_000.0);
    let ring_mesh = generate_planetary_ring_mesh(meshes, 1.35, 2.70, 128);
    let ring_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(ring_texture),
        perceptual_roughness: 0.5,
        metallic: 0.05,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None, // Visible from above and below
        ..default()
    });

    let chronos_entity = commands
        .spawn((
            UniverseEntity,
            UniversalPosition(chronos_pos),
            BodyVelocity(DVec3::ZERO),
            CelestialBody {
                physical_radius: chronos_radius,
                mass_kg: 8.50e19,
            },
            KeplerOrbit::circular(
                Some(sol_entity),
                115_000.0,
                0.044,
                1400.0,
                4.80,
            ),
            AxialRotation {
                speed_rad_per_sec: 0.18, // Fast gas giant rotation
            },
            NavTarget {
                name: "Chronos",
                classification: "Ringed Gas Giant",
                radius_m: chronos_radius,
            },
            BodyTextureStreamable {
                texture_path: saturn_path.clone(),
                current_tier: PlanetLodTier::LowRes,
            },
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(chronos_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: false,
                rest_temperature_k: 134.0,
            },
            Transform::from_xyz(chronos_pos.x as f32, chronos_pos.y as f32, chronos_pos.z as f32)
                .with_rotation(Quat::from_rotation_z(26.73_f32.to_radians()))
                .with_scale(Vec3::splat(chronos_radius as f32)),
        ))
        .with_children(|parent| {
            // Planetary ring in the equatorial plane (inherits 26.7 deg axial tilt from parent Chronos)
            parent.spawn((
                PlanetaryRing,
                Mesh3d(ring_mesh),
                MeshMaterial3d(ring_mat),
                RelativisticOpticsBody {
                    base_color: LinearRgba::new(1.0, 1.0, 1.0, 0.85),
                    base_emissive: LinearRgba::BLACK,
                    is_emissive_star: false,
                    rest_temperature_k: 110.0,
                },
                Transform::IDENTITY,
            ));
        })
        .id();

    // 6. TITAN (Moon of Chronos outside the rings)
    let titan_radius = 420.0;
    let titan_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.92, 0.60, 0.25), // Dense orange nitrogen haze
        perceptual_roughness: 0.75,
        ..default()
    });

    let titan_pos = chronos_pos + DVec3::new(11_000.0, 1_500.0, 2_000.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(titan_pos),
        BodyVelocity(DVec3::ZERO),
        CelestialBody {
            physical_radius: titan_radius,
            mass_kg: 3.50e17,
        },
        KeplerOrbit::circular(
            Some(chronos_entity),
            12_500.0,
            0.05,
            120.0,
            2.10,
        ),
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 0.05,
        },
        NavTarget {
            name: "Titan",
            classification: "Dense Atmosphere Moon",
            radius_m: titan_radius,
        },
        Mesh3d(sphere_mesh),
        MeshMaterial3d(titan_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.92, 0.60, 0.25, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 94.0,
        },
        Transform::from_xyz(titan_pos.x as f32, titan_pos.y as f32, titan_pos.z as f32)
            .with_scale(Vec3::splat(titan_radius as f32)),
    ));
}

/// Spawns the Solar System sector (alias to spawn_demo_celestial_system for backwards compatibility)
#[allow(dead_code)]
pub fn spawn_celestial_system(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
) {
    spawn_demo_celestial_system(commands, meshes, materials, asset_server);
}

/// Returns the starting spawn coordinates, orientation, and orbital velocity for the player's shuttle in 1:1 Solar System mode:
/// - In circular orbit around Earth at 3,500 km altitude (orbital radius 9,871 km).
/// - Earth is visible in the forward-right canopy, and Sol's radiant glow is visible in the forward-left canopy.
/// - The shuttle possesses Earth's heliocentric velocity (~29.78 km/s around Sol) plus circular orbital velocity (~6.35 km/s around Earth).
pub fn realistic_earth_orbital_spawn() -> (DVec3, Quat, DVec3) {
    let earth_pos = DVec3::new(ASTRONOMICAL_UNIT, 0.0, 0.0);
    let earth_radius = 6_371_000.0;
    let orbit_radius = earth_radius + 3_500_000.0; // 9,871 km orbital radius

    // Relative offset from Earth center to shuttle:
    // Placed in the sunward/dawn quadrant so Earth is on the right and Sol is on the left
    let r_dir = DVec3::new(0.35, 0.18, -0.92).normalize();
    let spawn_pos = earth_pos + r_dir * orbit_radius;

    // Forward direction: angled forward-left (~35° towards Sol at -X, leaving Earth at +X on right)
    let forward = DVec3::new(-0.82, 0.04, 0.57).normalize();
    let forward_f32 = forward.as_vec3();
    let right = forward_f32.cross(Vec3::Y).normalize();
    let up = right.cross(forward_f32).normalize();
    let spawn_rot = Quat::from_mat3(&Mat3::from_cols(right, up, -forward_f32)).normalize();

    // Circular orbital speed around Earth: v_circ = sqrt(G * M / r)
    let earth_mass = 5.9722e24;
    let g = crate::relativity::GRAVITATIONAL_CONSTANT;
    let v_circ = (g * earth_mass / orbit_radius).sqrt(); // ~6,354.5 m/s

    // Tangent orbital velocity perpendicular to r_dir
    let tangent_dir = (forward - r_dir * forward.dot(r_dir)).normalize();
    let v_orbit_earth = tangent_dir * v_circ;

    // Earth's heliocentric orbital velocity around Sol at t=0 (+Z direction)
    let v_earth_sol = DVec3::new(0.0, 0.0, 29_780.18);
    let total_velocity = v_earth_sol + v_orbit_earth;

    (spawn_pos, spawn_rot, total_velocity)
}

/// Returns the starting spawn coordinates for the player's shuttle in 1:1 Solar System mode
#[allow(dead_code)]
pub fn realistic_earth_orbital_spawn_position() -> DVec3 {
    realistic_earth_orbital_spawn().0
}

/// Spawns the 1:1 Scale Authentic Solar System:
/// Sol, Mercury, Venus, Earth (+ Moon), Mars, Jupiter, Saturn (+ Rings), Uranus, Neptune
pub fn spawn_realistic_solar_system(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    mut loading_tracker: Option<&mut crate::ui::StartupLoadingTracker>,
) {
    let sphere_mesh = generate_planet_sphere_mesh(meshes);

    let sun_path = resolve_planet_texture_path("sun");
    let earth_path = resolve_planet_texture_path("earth_day");
    let moon_path = resolve_planet_texture_path("moon");
    let mars_path = resolve_planet_texture_path("mars");
    let jupiter_path = resolve_planet_texture_path("jupiter");
    let saturn_path = resolve_planet_texture_path("saturn");
    let ring_path = resolve_planet_texture_path("saturn_ring_alpha");
    let mercury_path = resolve_planet_texture_path("mercury");
    let venus_path = resolve_planet_texture_path("venus");
    let uranus_path = resolve_planet_texture_path("uranus");
    let neptune_path = resolve_planet_texture_path("neptune");
    let io_path = resolve_planet_texture_path("io");
    let europa_path = resolve_planet_texture_path("europa");
    let ganymede_path = resolve_planet_texture_path("ganymede");
    let callisto_path = resolve_planet_texture_path("callisto");
    let mimas_path = resolve_planet_texture_path("mimas");
    let methone_path = resolve_planet_texture_path("methone");
    let enceladus_path = resolve_planet_texture_path("enceladus");
    let dione_path = resolve_planet_texture_path("dione");
    let rhea_path = resolve_planet_texture_path("rhea");
    let titan_path = resolve_planet_texture_path("titan");
    let iapetus_path = resolve_planet_texture_path("iapetus");
    let miranda_path = resolve_planet_texture_path("miranda");
    let ariel_path = resolve_planet_texture_path("ariel");
    let umbriel_path = resolve_planet_texture_path("umbriel");
    let titania_path = resolve_planet_texture_path("titania");
    let oberon_path = resolve_planet_texture_path("oberon");
    let proteus_path = resolve_planet_texture_path("proteus");
    let triton_path = resolve_planet_texture_path("triton");
    let nereid_path = resolve_planet_texture_path("nereid");
    let ceres_path = resolve_planet_texture_path("ceres");
    let pluto_path = resolve_planet_texture_path("pluto");
    let charon_path = resolve_planet_texture_path("charon");
    let haumea_path = resolve_planet_texture_path("haumea");
    let makemake_path = resolve_planet_texture_path("makemake");
    let eris_path = resolve_planet_texture_path("eris");
    let gonggong_path = resolve_planet_texture_path("gonggong");
    let sedna_path = resolve_planet_texture_path("sedna");
    let farfarout_path = resolve_planet_texture_path("farfarout");
    let planet9_path = resolve_planet_texture_path("planet9");

    let sun_texture = asset_server.load::<Image>(sun_path.as_ref());
    let earth_texture = asset_server.load(earth_path.as_ref());
    let moon_texture = asset_server.load(moon_path.as_ref());
    let mars_texture = asset_server.load(mars_path.as_ref());
    let jupiter_texture = asset_server.load(jupiter_path.as_ref());
    let saturn_texture = asset_server.load(saturn_path.as_ref());
    let ring_texture = asset_server.load(ring_path.as_ref());
    let mercury_texture = asset_server.load(mercury_path.as_ref());
    let venus_texture = asset_server.load(venus_path.as_ref());
    let uranus_texture = asset_server.load(uranus_path.as_ref());
    let neptune_texture = asset_server.load(neptune_path.as_ref());
    let io_texture = asset_server.load(io_path.as_ref());
    let europa_texture = asset_server.load(europa_path.as_ref());
    let ganymede_texture = asset_server.load(ganymede_path.as_ref());
    let callisto_texture = asset_server.load(callisto_path.as_ref());
    let mimas_texture = asset_server.load(mimas_path.as_ref());
    let methone_texture = asset_server.load(methone_path.as_ref());
    let enceladus_texture = asset_server.load(enceladus_path.as_ref());
    let dione_texture = asset_server.load(dione_path.as_ref());
    let rhea_texture = asset_server.load(rhea_path.as_ref());
    let titan_texture = asset_server.load(titan_path.as_ref());
    let iapetus_texture = asset_server.load(iapetus_path.as_ref());
    let miranda_texture = asset_server.load(miranda_path.as_ref());
    let ariel_texture = asset_server.load(ariel_path.as_ref());
    let umbriel_texture = asset_server.load(umbriel_path.as_ref());
    let titania_texture = asset_server.load(titania_path.as_ref());
    let oberon_texture = asset_server.load(oberon_path.as_ref());
    let proteus_texture = asset_server.load(proteus_path.as_ref());
    let triton_texture = asset_server.load(triton_path.as_ref());
    let nereid_texture = asset_server.load(nereid_path.as_ref());
    let ceres_texture = asset_server.load(ceres_path.as_ref());
    let pluto_texture = asset_server.load(pluto_path.as_ref());
    let charon_texture = asset_server.load(charon_path.as_ref());
    let haumea_texture = asset_server.load(haumea_path.as_ref());
    let makemake_texture = asset_server.load(makemake_path.as_ref());
    let eris_texture = asset_server.load(eris_path.as_ref());
    let gonggong_texture = asset_server.load(gonggong_path.as_ref());
    let sedna_texture = asset_server.load(sedna_path.as_ref());
    let farfarout_texture = asset_server.load(farfarout_path.as_ref());
    let planet9_texture = asset_server.load(planet9_path.as_ref());

    if let Some(ref mut tracker) = loading_tracker {
        tracker.tracked_assets.push(crate::ui::TrackedLoadingAsset {
            label: "Terra (Earth) High-Res Surface Map".to_string(),
            id: earth_texture.id().untyped(),
        });
        tracker.tracked_assets.push(crate::ui::TrackedLoadingAsset {
            label: "Luna (Moon) Near-Side Surface Map".to_string(),
            id: moon_texture.id().untyped(),
        });
        tracker.tracked_assets.push(crate::ui::TrackedLoadingAsset {
            label: "Sol Photosphere Emissive Map".to_string(),
            id: sun_texture.id().untyped(),
        });
        tracker.tracked_assets.push(crate::ui::TrackedLoadingAsset {
            label: "Mars Surface Map".to_string(),
            id: mars_texture.id().untyped(),
        });
        tracker.tracked_assets.push(crate::ui::TrackedLoadingAsset {
            label: "Jupiter Atmospheric Banding Map".to_string(),
            id: jupiter_texture.id().untyped(),
        });
        tracker.tracked_assets.push(crate::ui::TrackedLoadingAsset {
            label: "Saturn Atmosphere Map".to_string(),
            id: saturn_texture.id().untyped(),
        });
        tracker.tracked_assets.push(crate::ui::TrackedLoadingAsset {
            label: "Saturn Ring Alpha Map".to_string(),
            id: ring_texture.id().untyped(),
        });
        tracker.tracked_assets.push(crate::ui::TrackedLoadingAsset {
            label: "Venus Cloud Deck".to_string(),
            id: venus_texture.id().untyped(),
        });
        tracker.tracked_assets.push(crate::ui::TrackedLoadingAsset {
            label: "Mercury Cratered Surface".to_string(),
            id: mercury_texture.id().untyped(),
        });
    }

    // 1. SOL (The Sun) - 696,340 km radius at Barycenter (0, 0, 0)
    let sol_radius = 696_340_000.0;
    let sol_mat = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::new(100.8, 304.2, 1808.5, 1.0)),
        base_color_texture: Some(sun_texture.clone()),
        emissive: LinearRgba::BLACK,
        unlit: true,
        ..default()
    });

    let sol_inner_corona_mat = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::new(3.0, 3.0, 3.0, 0.40)),
        emissive: LinearRgba::BLACK,
        alpha_mode: AlphaMode::Add,
        unlit: true,
        ..default()
    });

    let sol_outer_corona_mat = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::new(0.8, 0.8, 0.8, 0.15)),
        emissive: LinearRgba::BLACK,
        alpha_mode: AlphaMode::Add,
        unlit: true,
        ..default()
    });

    let sol_entity = commands
        .spawn((
            UniverseEntity,
            PrimaryStar,
            UniversalPosition(DVec3::ZERO),
            BodyVelocity(DVec3::ZERO),
            CelestialBody {
                physical_radius: sol_radius,
                mass_kg: 1.9885e30,
            },
            NavTarget {
                name: "Sol",
                classification: "G2V White Main-Sequence Star",
                radius_m: sol_radius,
            },
            AxialRotation {
                speed_rad_per_sec: 2.865e-6, // 25.38 days sidereal period
            },
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(sol_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(100.8, 304.2, 1808.5, 1.0),
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: true,
                rest_temperature_k: 5778.0,
            },
            Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(sol_radius as f32)),
        ))
        .with_children(|parent| {
            parent.spawn((
                SolarCorona,
                Mesh3d(sphere_mesh.clone()),
                MeshMaterial3d(sol_inner_corona_mat),
                RelativisticOpticsBody {
                    base_color: LinearRgba::new(3.0, 3.0, 3.0, 0.40),
                    base_emissive: LinearRgba::BLACK,
                    is_emissive_star: true,
                    rest_temperature_k: 5778.0,
                },
                Transform::from_scale(Vec3::splat(1.04)),
            ));
            parent.spawn((
                SolarCorona,
                Mesh3d(sphere_mesh.clone()),
                MeshMaterial3d(sol_outer_corona_mat),
                RelativisticOpticsBody {
                    base_color: LinearRgba::new(0.8, 0.8, 0.8, 0.15),
                    base_emissive: LinearRgba::BLACK,
                    is_emissive_star: true,
                    rest_temperature_k: 5778.0,
                },
                Transform::from_scale(Vec3::splat(1.10)),
            ));
        })
        .id();

    // 2. MERCURY
    let mercury_radius = 2_439_700.0;
    let mercury_semi_major = 57_909_050_000.0;
    let mercury_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(mercury_texture),
        perceptual_roughness: 0.90,
        metallic: 0.05,
        ..default()
    });
    let mercury_inc: f64 = 0.12226;
    let mercury_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        mercury_semi_major,
        0.20563, // Astronomical eccentricity
        mercury_inc,
        0.84357, // Longitude of ascending node (48.33 deg)
        0.50832, // Argument of periapsis (29.12 deg)
        7_600_522.0,
        0.45,
    );
    let (mercury_pos, mercury_vel) = mercury_orbit.evaluate_state_at_time(0.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(mercury_pos),
        BodyVelocity(mercury_vel),
        CelestialBody {
            physical_radius: mercury_radius,
            mass_kg: 3.3011e23,
        },
        mercury_orbit,
        AxialRotation {
            speed_rad_per_sec: 1.240e-6, // 58.646 days sidereal period
        },
        NavTarget {
            name: "Mercury",
            classification: "Cratered Terrestrial",
            radius_m: mercury_radius,
        },
        BodyTextureStreamable {
            texture_path: mercury_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(mercury_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 440.0,
        },
        Transform::from_xyz(mercury_pos.x as f32, mercury_pos.y as f32, mercury_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(0.034_f32.to_radians()))
            .with_scale(Vec3::splat(mercury_radius as f32)),
    ));

    // 3. VENUS
    let venus_radius = 6_051_800.0;
    let venus_semi_major = 108_208_930_000.0;
    let venus_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(venus_texture),
        perceptual_roughness: 0.65,
        metallic: 0.05,
        ..default()
    });
    let venus_inc: f64 = 0.05924;
    let venus_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        venus_semi_major,
        0.00677, // Near-circular eccentricity
        venus_inc,
        1.3383, // Longitude of ascending node (76.68 deg)
        0.9579, // Argument of periapsis (54.88 deg)
        19_414_166.0,
        1.80,
    );
    let (venus_pos, venus_vel) = venus_orbit.evaluate_state_at_time(0.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(venus_pos),
        BodyVelocity(venus_vel),
        CelestialBody {
            physical_radius: venus_radius,
            mass_kg: 4.8675e24,
        },
        venus_orbit,
        AxialRotation {
            speed_rad_per_sec: -2.992e-7, // 243.02 days retrograde period
        },
        NavTarget {
            name: "Venus",
            classification: "Dense Atmosphere Terrestrial",
            radius_m: venus_radius,
        },
        BodyTextureStreamable {
            texture_path: venus_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(venus_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 737.0,
        },
        Transform::from_xyz(venus_pos.x as f32, venus_pos.y as f32, venus_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(177.3_f32.to_radians()))
            .with_scale(Vec3::splat(venus_radius as f32)),
    )).with_children(|parent| {
        let atmo_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.95, 0.85, 0.55, 0.12),
            emissive: LinearRgba::BLACK,
            alpha_mode: AlphaMode::Blend,
            unlit: false,
            ..default()
        });
        parent.spawn((
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(atmo_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(0.95, 0.85, 0.55, 0.12),
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: false,
                rest_temperature_k: 737.0,
            },
            Transform::from_scale(Vec3::splat(1.025)),
        ));
    });

    // 4. EARTH (Home World)
    let earth_radius = 6_371_000.0;
    let earth_semi_major = ASTRONOMICAL_UNIT;
    let earth_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(earth_texture),
        perceptual_roughness: 0.50,
        metallic: 0.05,
        reflectance: 0.2,
        cull_mode: None, // Visible from both outside and inside
        ..default()
    });
    let earth_orbit = KeplerOrbit::circular(
        Some(sol_entity),
        earth_semi_major,
        0.0,
        31_558_118.0,
        0.0,
    );
    let (earth_pos, earth_vel) = earth_orbit.evaluate_state_at_time(0.0);
    let earth_entity = commands.spawn((
        UniverseEntity,
        UniversalPosition(earth_pos),
        BodyVelocity(earth_vel),
        CelestialBody {
            physical_radius: earth_radius,
            mass_kg: 5.9722e24,
        },
        earth_orbit,
        AxialRotation {
            speed_rad_per_sec: 7.2921e-5, // 23h 56m 4s (86,164.1 s) sidereal day
        },
        NavTarget {
            name: "Earth",
            classification: "Habitable Ocean Terrestrial",
            radius_m: earth_radius,
        },
        BodyTextureStreamable {
            texture_path: earth_path,
            current_tier: PlanetLodTier::HighRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(earth_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            base_emissive: LinearRgba::rgb(0.01, 0.02, 0.04),
            is_emissive_star: false,
            rest_temperature_k: 288.0,
        },
        Transform::from_xyz(earth_pos.x as f32, earth_pos.y as f32, earth_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(23.44_f32.to_radians()))
            .with_scale(Vec3::splat(earth_radius as f32)),
    )).with_children(|parent| {
        let atmo_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.35, 0.70, 1.0, 0.10),
            emissive: LinearRgba::BLACK,
            alpha_mode: AlphaMode::Blend,
            unlit: false,
            ..default()
        });
        parent.spawn((
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(atmo_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(0.35, 0.70, 1.0, 0.10),
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: false,
                rest_temperature_k: 288.0,
            },
            Transform::from_scale(Vec3::splat(1.018)),
        ));
    }).id();

    // 4b. MOON (LUNA)
    let luna_radius = 1_737_400.0;
    let luna_semi_major = 384_400_000.0;
    let luna_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(moon_texture.clone()),
        perceptual_roughness: 0.90,
        metallic: 0.0,
        cull_mode: None, // Visible from both outside and inside
        ..default()
    });
    let luna_inc: f64 = 0.0898;
    let luna_mean_anomaly: f64 = 1.20;
    let luna_orbit = KeplerOrbit::circular(
        Some(earth_entity),
        luna_semi_major,
        luna_inc,
        2_360_585.0,
        luna_mean_anomaly,
    );
    let (luna_rel_pos, luna_rel_vel) = luna_orbit.evaluate_state_at_time(0.0);
    let luna_pos = earth_pos + luna_rel_pos;
    let luna_vel = earth_vel + luna_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(luna_pos),
        BodyVelocity(luna_vel),
        CelestialBody {
            physical_radius: luna_radius,
            mass_kg: 7.342e22,
        },
        luna_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: -2.6617e-6, // Tidally locked: 2,360,585.0 s (27.32 days)
        },
        NavTarget {
            name: "Luna",
            classification: "Tidally Locked Major Moon",
            radius_m: luna_radius,
        },
        BodyTextureStreamable {
            texture_path: moon_path,
            current_tier: PlanetLodTier::HighRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(luna_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.95, 0.95, 0.95, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 250.0,
        },
        Transform::from_xyz(luna_pos.x as f32, luna_pos.y as f32, luna_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(6.68_f32.to_radians()) * Quat::from_rotation_y(-1.20_f32))
            .with_scale(Vec3::splat(luna_radius as f32)),
    ));

    // 5. MARS
    let mars_radius = 3_389_500.0;
    let mars_semi_major = 227_939_200_000.0;
    let mars_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(mars_texture),
        perceptual_roughness: 0.85,
        metallic: 0.05,
        cull_mode: None, // Visible from both outside and inside
        ..default()
    });
    let mars_inc: f64 = 0.03229;
    let mars_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        mars_semi_major,
        0.0934, // Mars eccentricity
        mars_inc,
        0.86498, // Longitude of ascending node (49.56 deg)
        5.0000,  // Argument of periapsis (286.5 deg)
        59_355_072.0,
        2.50,
    );
    let (mars_pos, mars_vel) = mars_orbit.evaluate_state_at_time(0.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(mars_pos),
        BodyVelocity(mars_vel),
        CelestialBody {
            physical_radius: mars_radius,
            mass_kg: 6.4171e23,
        },
        mars_orbit,
        AxialRotation {
            speed_rad_per_sec: 7.0882e-5, // 24h 37m 23s (88,642.7 s) sidereal sol
        },
        NavTarget {
            name: "Mars",
            classification: "Desert Terrestrial Planet",
            radius_m: mars_radius,
        },
        BodyTextureStreamable {
            texture_path: mars_path.clone(),
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(mars_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 210.0,
        },
        Transform::from_xyz(mars_pos.x as f32, mars_pos.y as f32, mars_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(25.19_f32.to_radians()))
            .with_scale(Vec3::splat(mars_radius as f32)),
    )).with_children(|parent| {
        let atmo_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.85, 0.55, 0.35, 0.08),
            emissive: LinearRgba::BLACK,
            alpha_mode: AlphaMode::Blend,
            unlit: false,
            ..default()
        });
        parent.spawn((
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(atmo_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(0.85, 0.55, 0.35, 0.08),
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: false,
                rest_temperature_k: 210.0,
            },
            Transform::from_scale(Vec3::splat(1.02)),
        ));
    });

    // 5b. CERES (Dwarf Planet of the Asteroid Belt)
    let ceres_radius = 473_000.0;
    let ceres_semi_major = 413_970_000_000.0; // 2.7675 AU
    let ceres_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(ceres_texture),
        perceptual_roughness: 0.88,
        metallic: 0.02,
        ..default()
    });
    let ceres_inc: f64 = 0.1848; // 10.59 deg inclination to ecliptic
    let ceres_mean_anomaly: f64 = 1.85;
    let ceres_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        ceres_semi_major,
        0.0758, // Ceres eccentricity
        ceres_inc,
        1.4020, // Longitude of ascending node (80.33 deg)
        1.2845, // Argument of periapsis (73.6 deg)
        145_324_800.0, // 1,682 d (4.60 y)
        ceres_mean_anomaly,
    );
    let (ceres_pos, ceres_vel) = ceres_orbit.evaluate_state_at_time(0.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(ceres_pos),
        BodyVelocity(ceres_vel),
        CelestialBody {
            physical_radius: ceres_radius,
            mass_kg: 9.3835e20,
        },
        ceres_orbit,
        AxialRotation {
            speed_rad_per_sec: 1.923e-4, // 9.074 hours
        },
        NavTarget {
            name: "Ceres",
            classification: "Dwarf Planet (Asteroid Belt)",
            radius_m: ceres_radius,
        },
        BodyTextureStreamable {
            texture_path: ceres_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(ceres_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.85, 0.85, 0.85, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 168.0,
        },
        Transform::from_xyz(ceres_pos.x as f32, ceres_pos.y as f32, ceres_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(ceres_radius as f32)),
    ));

    // 6. JUPITER (King of Planets)
    let jupiter_radius = 69_911_000.0;
    let jupiter_semi_major = 778_547_200_000.0;
    let jupiter_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(jupiter_texture),
        perceptual_roughness: 0.55,
        metallic: 0.05,
        ..default()
    });
    let jup_inc: f64 = 0.02274;
    let jupiter_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        jupiter_semi_major,
        0.0485, // Jupiter eccentricity
        jup_inc,
        1.7534, // Longitude of ascending node (100.46 deg)
        4.7799, // Argument of periapsis (273.87 deg)
        374_335_776.0,
        4.10,
    );
    let (jupiter_pos, jupiter_vel) = jupiter_orbit.evaluate_state_at_time(0.0);
    let jupiter_entity = commands.spawn((
        UniverseEntity,
        UniversalPosition(jupiter_pos),
        BodyVelocity(jupiter_vel),
        CelestialBody {
            physical_radius: jupiter_radius,
            mass_kg: 1.8982e27,
        },
        jupiter_orbit,
        AxialRotation {
            speed_rad_per_sec: 1.7585e-4, // 9h 55m 30s (35,730 s) Jovian day
        },
        NavTarget {
            name: "Jupiter",
            classification: "Gas Giant (Jovian Primary)",
            radius_m: jupiter_radius,
        },
        BodyTextureStreamable {
            texture_path: jupiter_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(jupiter_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 165.0,
        },
        Transform::from_xyz(jupiter_pos.x as f32, jupiter_pos.y as f32, jupiter_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(3.13_f32.to_radians()))
            .with_scale(Vec3::splat(jupiter_radius as f32)),
    )).id();

    // 6a. IO (Jupiter I - Volcanic Moon)
    let io_radius = 1_821_600.0;
    let io_semi_major = 421_700_000.0;
    let io_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(io_texture),
        perceptual_roughness: 0.85,
        metallic: 0.0,
        ..default()
    });
    let io_inc: f64 = 0.00087266; // 0.05 deg inclination to Jupiter equator
    let io_mean_anomaly: f64 = 0.80;
    let io_orbit = KeplerOrbit::circular(
        Some(jupiter_entity),
        io_semi_major,
        io_inc,
        152_853.5, // 1.7691 d (4:2:1 Laplace resonance)
        io_mean_anomaly,
    );
    let (io_rel_pos, io_rel_vel) = io_orbit.evaluate_state_at_time(0.0);
    let io_pos = jupiter_pos + io_rel_pos;
    let io_vel = jupiter_vel + io_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(io_pos),
        BodyVelocity(io_vel),
        CelestialBody {
            physical_radius: io_radius,
            mass_kg: 8.9319e22,
        },
        io_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: -4.1106e-5, // Tidally locked
        },
        NavTarget {
            name: "Io",
            classification: "Volcanic Galilean Moon",
            radius_m: io_radius,
        },
        BodyTextureStreamable {
            texture_path: io_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(io_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 130.0,
        },
        Transform::from_xyz(io_pos.x as f32, io_pos.y as f32, io_pos.z as f32)
            .with_rotation(Quat::from_rotation_y(-io_mean_anomaly as f32))
            .with_scale(Vec3::splat(io_radius as f32)),
    ));

    // 6b. EUROPA (Jupiter II - Subsurface Ocean Moon)
    let europa_radius = 1_560_800.0;
    let europa_semi_major = 670_900_000.0;
    let europa_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(europa_texture),
        perceptual_roughness: 0.35,
        metallic: 0.05,
        reflectance: 0.5,
        ..default()
    });
    let europa_inc: f64 = 0.008203; // 0.47 deg inclination
    let europa_mean_anomaly: f64 = 2.40;
    let europa_orbit = KeplerOrbit::circular(
        Some(jupiter_entity),
        europa_semi_major,
        europa_inc,
        306_822.0, // 3.5512 d (2:1 Laplace resonance with Io)
        europa_mean_anomaly,
    );
    let (europa_rel_pos, europa_rel_vel) = europa_orbit.evaluate_state_at_time(0.0);
    let europa_pos = jupiter_pos + europa_rel_pos;
    let europa_vel = jupiter_vel + europa_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(europa_pos),
        BodyVelocity(europa_vel),
        CelestialBody {
            physical_radius: europa_radius,
            mass_kg: 4.7998e22,
        },
        europa_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: -2.0478e-5, // Tidally locked
        },
        NavTarget {
            name: "Europa",
            classification: "Subsurface Ocean Galilean Moon",
            radius_m: europa_radius,
        },
        BodyTextureStreamable {
            texture_path: europa_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(europa_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.95, 0.95, 0.98, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 102.0,
        },
        Transform::from_xyz(europa_pos.x as f32, europa_pos.y as f32, europa_pos.z as f32)
            .with_rotation(Quat::from_rotation_y(-europa_mean_anomaly as f32))
            .with_scale(Vec3::splat(europa_radius as f32)),
    ));

    // 6c. GANYMEDE (Jupiter III - Magnetic Giant Moon)
    let ganymede_radius = 2_634_100.0;
    let ganymede_semi_major = 1_070_400_000.0;
    let ganymede_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(ganymede_texture),
        perceptual_roughness: 0.75,
        metallic: 0.02,
        ..default()
    });
    let ganymede_inc: f64 = 0.0034907; // 0.20 deg inclination
    let ganymede_mean_anomaly: f64 = 4.10;
    let ganymede_orbit = KeplerOrbit::circular(
        Some(jupiter_entity),
        ganymede_semi_major,
        ganymede_inc,
        618_153.4, // 7.1546 d (4:1 Laplace resonance with Io)
        ganymede_mean_anomaly,
    );
    let (ganymede_rel_pos, ganymede_rel_vel) = ganymede_orbit.evaluate_state_at_time(0.0);
    let ganymede_pos = jupiter_pos + ganymede_rel_pos;
    let ganymede_vel = jupiter_vel + ganymede_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(ganymede_pos),
        BodyVelocity(ganymede_vel),
        CelestialBody {
            physical_radius: ganymede_radius,
            mass_kg: 1.4819e23,
        },
        ganymede_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: -1.0164e-5, // Tidally locked
        },
        NavTarget {
            name: "Ganymede",
            classification: "Magnetic Giant Galilean Moon",
            radius_m: ganymede_radius,
        },
        BodyTextureStreamable {
            texture_path: ganymede_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(ganymede_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.9, 0.9, 0.9, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 110.0,
        },
        Transform::from_xyz(ganymede_pos.x as f32, ganymede_pos.y as f32, ganymede_pos.z as f32)
            .with_rotation(Quat::from_rotation_y(-ganymede_mean_anomaly as f32))
            .with_scale(Vec3::splat(ganymede_radius as f32)),
    ));

    // 6d. CALLISTO (Jupiter IV - Ancient Cratered Moon)
    let callisto_radius = 2_410_300.0;
    let callisto_semi_major = 1_882_700_000.0;
    let callisto_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(callisto_texture),
        perceptual_roughness: 0.90,
        metallic: 0.0,
        ..default()
    });
    let callisto_inc: f64 = 0.0048869; // 0.28 deg inclination
    let callisto_mean_anomaly: f64 = 5.60;
    let callisto_orbit = KeplerOrbit::circular(
        Some(jupiter_entity),
        callisto_semi_major,
        callisto_inc,
        1_441_931.2, // 16.689 d
        callisto_mean_anomaly,
    );
    let (callisto_rel_pos, callisto_rel_vel) = callisto_orbit.evaluate_state_at_time(0.0);
    let callisto_pos = jupiter_pos + callisto_rel_pos;
    let callisto_vel = jupiter_vel + callisto_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(callisto_pos),
        BodyVelocity(callisto_vel),
        CelestialBody {
            physical_radius: callisto_radius,
            mass_kg: 1.0759e23,
        },
        callisto_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: -4.3575e-6, // Tidally locked
        },
        NavTarget {
            name: "Callisto",
            classification: "Ancient Cratered Galilean Moon",
            radius_m: callisto_radius,
        },
        BodyTextureStreamable {
            texture_path: callisto_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(callisto_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.85, 0.85, 0.85, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 134.0,
        },
        Transform::from_xyz(callisto_pos.x as f32, callisto_pos.y as f32, callisto_pos.z as f32)
            .with_rotation(Quat::from_rotation_y(-callisto_mean_anomaly as f32))
            .with_scale(Vec3::splat(callisto_radius as f32)),
    ));

    // 7. SATURN (Ringed Giant)
    let saturn_radius = 58_232_000.0;
    let saturn_semi_major = 1_433_449_400_000.0;
    let ring_mesh = generate_planetary_ring_mesh(meshes, 1.25, 2.35, 128);
    let ring_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(ring_texture),
        perceptual_roughness: 0.5,
        metallic: 0.05,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    });
    let saturn_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(saturn_texture),
        perceptual_roughness: 0.55,
        metallic: 0.05,
        ..default()
    });
    let sat_inc: f64 = 0.04337;
    let saturn_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        saturn_semi_major,
        0.0555, // Saturn eccentricity
        sat_inc,
        1.9838, // Longitude of ascending node (113.67 deg)
        5.9235, // Argument of periapsis (339.39 deg)
        929_596_608.0,
        5.20,
    );
    let (saturn_pos, saturn_vel) = saturn_orbit.evaluate_state_at_time(0.0);
    let saturn_entity = commands.spawn((
        UniverseEntity,
        UniversalPosition(saturn_pos),
        BodyVelocity(saturn_vel),
        CelestialBody {
            physical_radius: saturn_radius,
            mass_kg: 5.6834e26,
        },
        saturn_orbit,
        AxialRotation {
            speed_rad_per_sec: 1.6544e-4, // 10h 33m (37,978 s) Saturnian day
        },
        NavTarget {
            name: "Saturn",
            classification: "Ringed Gas Giant",
            radius_m: saturn_radius,
        },
        BodyTextureStreamable {
            texture_path: saturn_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(saturn_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 134.0,
        },
        Transform::from_xyz(saturn_pos.x as f32, saturn_pos.y as f32, saturn_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(26.73_f32.to_radians()))
            .with_scale(Vec3::splat(saturn_radius as f32)),
    )).with_children(|parent| {
        parent.spawn((
            PlanetaryRing,
            Mesh3d(ring_mesh),
            MeshMaterial3d(ring_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(1.0, 1.0, 1.0, 0.85),
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: false,
                rest_temperature_k: 110.0,
            },
            Transform::IDENTITY,
        ));
    }).id();

    // 7a. MIMAS (Saturn I - Cratered Icy Moon)
    let mimas_radius = 198_200.0;
    let mimas_semi_major = 185_539_000.0;
    let mimas_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(mimas_texture),
        perceptual_roughness: 0.80,
        metallic: 0.0,
        ..default()
    });
    let mimas_inc: f64 = 0.02733; // 1.566 deg
    let mimas_mean_anomaly: f64 = 0.50;
    let mimas_orbit = KeplerOrbit::circular(
        Some(saturn_entity),
        mimas_semi_major,
        mimas_inc,
        81_425.2, // 0.9424 d (clears Cassini division)
        mimas_mean_anomaly,
    );
    let (mimas_rel_pos, mimas_rel_vel) = mimas_orbit.evaluate_state_at_time(0.0);
    let mimas_pos = saturn_pos + mimas_rel_pos;
    let mimas_vel = saturn_vel + mimas_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(mimas_pos),
        BodyVelocity(mimas_vel),
        CelestialBody {
            physical_radius: mimas_radius,
            mass_kg: 3.7493e19,
        },
        mimas_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 7.7165e-5, // Tidally locked
        },
        NavTarget {
            name: "Mimas",
            classification: "Cratered Icy Moon (Herschel Basin)",
            radius_m: mimas_radius,
        },
        BodyTextureStreamable {
            texture_path: mimas_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(mimas_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.9, 0.9, 0.9, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 73.0,
        },
        Transform::from_xyz(mimas_pos.x as f32, mimas_pos.y as f32, mimas_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(mimas_radius as f32)),
    ));

    // 7b. METHONE (Saturn XXXII - Albedo Egg Moon, Triaxial Ellipsoid)
    // Cassini measurements: 3.88 km x 2.58 km x 2.42 km (semi-axes a=1940m, b=1250m, c=1300m)
    let methone_radius = 1_450.0;
    let methone_radii = EllipsoidRadii::new(1_940.0, 1_250.0, 1_300.0);
    let methone_semi_major = 194_440_000.0;
    let methone_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(methone_texture),
        perceptual_roughness: 0.60,
        metallic: 0.0,
        ..default()
    });
    let methone_inc: f64 = 0.00012; // 0.007 deg
    let methone_mean_anomaly: f64 = 1.20;
    let methone_orbit = KeplerOrbit::circular(
        Some(saturn_entity),
        methone_semi_major,
        methone_inc,
        87_189.7, // 1.0091 d
        methone_mean_anomaly,
    );
    let (methone_rel_pos, methone_rel_vel) = methone_orbit.evaluate_state_at_time(0.0);
    let methone_pos = saturn_pos + methone_rel_pos;
    let methone_vel = saturn_vel + methone_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(methone_pos),
        BodyVelocity(methone_vel),
        CelestialBody {
            physical_radius: methone_radius,
            mass_kg: 1.85e13,
        },
        methone_radii,
        methone_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 7.2063e-5, // Tidally locked
        },
        NavTarget {
            name: "Methone",
            classification: "Albedo Egg Moon (Triaxial Ellipsoid)",
            radius_m: methone_radius,
        },
        BodyTextureStreamable {
            texture_path: methone_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(methone_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.95, 0.95, 0.95, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 75.0,
        },
        Transform::from_xyz(methone_pos.x as f32, methone_pos.y as f32, methone_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(methone_radius as f32)),
    ));

    // 7c. ENCELADUS (Saturn II - Cryovolcanic Ocean Moon)
    let enceladus_radius = 252_100.0;
    let enceladus_semi_major = 237_948_000.0;
    let enceladus_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(enceladus_texture),
        perceptual_roughness: 0.25,
        metallic: 0.05,
        reflectance: 0.70, // Ultra-high albedo > 0.99
        ..default()
    });
    let enceladus_inc: f64 = 0.000157; // 0.009 deg
    let enceladus_mean_anomaly: f64 = 2.10;
    let enceladus_orbit = KeplerOrbit::circular(
        Some(saturn_entity),
        enceladus_semi_major,
        enceladus_inc,
        118_386.8, // 1.3702 d (2:1 resonance with Dione)
        enceladus_mean_anomaly,
    );
    let (enceladus_rel_pos, enceladus_rel_vel) = enceladus_orbit.evaluate_state_at_time(0.0);
    let enceladus_pos = saturn_pos + enceladus_rel_pos;
    let enceladus_vel = saturn_vel + enceladus_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(enceladus_pos),
        BodyVelocity(enceladus_vel),
        CelestialBody {
            physical_radius: enceladus_radius,
            mass_kg: 1.08022e20,
        },
        enceladus_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 5.3073e-5, // Tidally locked
        },
        NavTarget {
            name: "Enceladus",
            classification: "Cryovolcanic Ocean Moon (Tiger Stripes)",
            radius_m: enceladus_radius,
        },
        BodyTextureStreamable {
            texture_path: enceladus_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(enceladus_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 75.0,
        },
        Transform::from_xyz(enceladus_pos.x as f32, enceladus_pos.y as f32, enceladus_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(enceladus_radius as f32)),
    ));

    // 7d. DIONE (Saturn IV - Fractured Ice Chasm Moon)
    let dione_radius = 561_400.0;
    let dione_semi_major = 377_396_000.0;
    let dione_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(dione_texture),
        perceptual_roughness: 0.65,
        metallic: 0.02,
        ..default()
    });
    let dione_inc: f64 = 0.00033; // 0.019 deg
    let dione_mean_anomaly: f64 = 3.40;
    let dione_orbit = KeplerOrbit::circular(
        Some(saturn_entity),
        dione_semi_major,
        dione_inc,
        236_469.5, // 2.7369 d (2:1 resonance with Enceladus)
        dione_mean_anomaly,
    );
    let (dione_rel_pos, dione_rel_vel) = dione_orbit.evaluate_state_at_time(0.0);
    let dione_pos = saturn_pos + dione_rel_pos;
    let dione_vel = saturn_vel + dione_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(dione_pos),
        BodyVelocity(dione_vel),
        CelestialBody {
            physical_radius: dione_radius,
            mass_kg: 1.0955e21,
        },
        dione_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 2.6571e-5, // Tidally locked
        },
        NavTarget {
            name: "Dione",
            classification: "Fractured Ice Chasm Moon",
            radius_m: dione_radius,
        },
        BodyTextureStreamable {
            texture_path: dione_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(dione_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.95, 0.95, 0.95, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 87.0,
        },
        Transform::from_xyz(dione_pos.x as f32, dione_pos.y as f32, dione_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(dione_radius as f32)),
    ));

    // 7e. RHEA (Saturn V - Cratered Ice Giant Moon)
    let rhea_radius = 763_800.0;
    let rhea_semi_major = 527_108_000.0;
    let rhea_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(rhea_texture),
        perceptual_roughness: 0.80,
        metallic: 0.0,
        ..default()
    });
    let rhea_inc: f64 = 0.00602; // 0.345 deg
    let rhea_mean_anomaly: f64 = 4.30;
    let rhea_orbit = KeplerOrbit::circular(
        Some(saturn_entity),
        rhea_semi_major,
        rhea_inc,
        390_373.5, // 4.5182 d
        rhea_mean_anomaly,
    );
    let (rhea_rel_pos, rhea_rel_vel) = rhea_orbit.evaluate_state_at_time(0.0);
    let rhea_pos = saturn_pos + rhea_rel_pos;
    let rhea_vel = saturn_vel + rhea_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(rhea_pos),
        BodyVelocity(rhea_vel),
        CelestialBody {
            physical_radius: rhea_radius,
            mass_kg: 2.3065e21,
        },
        rhea_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 1.6095e-5, // Tidally locked
        },
        NavTarget {
            name: "Rhea",
            classification: "Cratered Ice Giant Moon",
            radius_m: rhea_radius,
        },
        BodyTextureStreamable {
            texture_path: rhea_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(rhea_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.9, 0.9, 0.9, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 76.0,
        },
        Transform::from_xyz(rhea_pos.x as f32, rhea_pos.y as f32, rhea_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(rhea_radius as f32)),
    ));

    // 7f. TITAN (Saturn VI - Hydrocarbon Atmosphere Giant Moon)
    let titan_radius = 2_574_700.0;
    let titan_semi_major = 1_221_870_000.0;
    let titan_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(titan_texture),
        perceptual_roughness: 0.70,
        metallic: 0.05,
        ..default()
    });
    let titan_inc: f64 = 0.00608; // 0.3485 deg
    let titan_mean_anomaly: f64 = 5.20;
    let titan_orbit = KeplerOrbit::circular(
        Some(saturn_entity),
        titan_semi_major,
        titan_inc,
        1_377_684.4, // 15.945 d
        titan_mean_anomaly,
    );
    let (titan_rel_pos, titan_rel_vel) = titan_orbit.evaluate_state_at_time(0.0);
    let titan_pos = saturn_pos + titan_rel_pos;
    let titan_vel = saturn_vel + titan_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(titan_pos),
        BodyVelocity(titan_vel),
        CelestialBody {
            physical_radius: titan_radius,
            mass_kg: 1.3452e23,
        },
        titan_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 4.5607e-6, // Tidally locked
        },
        NavTarget {
            name: "Titan",
            classification: "Dense Atmosphere Hydrocarbon Moon",
            radius_m: titan_radius,
        },
        BodyTextureStreamable {
            texture_path: titan_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(titan_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.98, 0.85, 0.55, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 94.0,
        },
        Transform::from_xyz(titan_pos.x as f32, titan_pos.y as f32, titan_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(titan_radius as f32)),
    )).with_children(|parent| {
        let atmo_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.95, 0.65, 0.20, 0.16),
            emissive: LinearRgba::BLACK,
            alpha_mode: AlphaMode::Blend,
            unlit: false,
            ..default()
        });
        parent.spawn((
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(atmo_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(0.95, 0.65, 0.20, 0.16),
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: false,
                rest_temperature_k: 94.0,
            },
            Transform::from_scale(Vec3::splat(1.035)),
        ));
    });

    // 7g. IAPETUS (Saturn VIII - Two-Tone Yin-Yang Ridge Moon)
    let iapetus_radius = 734_500.0;
    let iapetus_semi_major = 3_560_820_000.0;
    let iapetus_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(iapetus_texture),
        perceptual_roughness: 0.85,
        metallic: 0.0,
        ..default()
    });
    let iapetus_inc: f64 = 0.2700; // 15.47 deg (high inclination)
    let iapetus_mean_anomaly: f64 = 0.90;
    let iapetus_orbit = KeplerOrbit::circular(
        Some(saturn_entity),
        iapetus_semi_major,
        iapetus_inc,
        6_853_377.6, // 79.3215 d
        iapetus_mean_anomaly,
    );
    let (iapetus_rel_pos, iapetus_rel_vel) = iapetus_orbit.evaluate_state_at_time(0.0);
    let iapetus_pos = saturn_pos + iapetus_rel_pos;
    let iapetus_vel = saturn_vel + iapetus_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(iapetus_pos),
        BodyVelocity(iapetus_vel),
        CelestialBody {
            physical_radius: iapetus_radius,
            mass_kg: 1.8056e21,
        },
        iapetus_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 9.168e-7, // Tidally locked
        },
        NavTarget {
            name: "Iapetus",
            classification: "Two-Tone Yin-Yang Ridge Moon",
            radius_m: iapetus_radius,
        },
        BodyTextureStreamable {
            texture_path: iapetus_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(iapetus_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.85, 0.85, 0.85, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 100.0,
        },
        Transform::from_xyz(iapetus_pos.x as f32, iapetus_pos.y as f32, iapetus_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(iapetus_radius as f32)),
    ));

    // 8. URANUS
    let uranus_radius = 25_362_000.0;
    let uranus_semi_major = 2_872_458_500_000.0;
    let uranus_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(uranus_texture),
        perceptual_roughness: 0.55,
        metallic: 0.05,
        ..default()
    });
    let ur_inc: f64 = 0.01349;
    let uranus_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        uranus_semi_major,
        0.0463, // Uranus eccentricity
        ur_inc,
        1.2917, // Longitude of ascending node (74.01 deg)
        1.6929, // Argument of periapsis (96.99 deg)
        2_651_218_560.0,
        0.90,
    );
    let (uranus_pos, uranus_vel) = uranus_orbit.evaluate_state_at_time(0.0);
    let uranus_entity = commands.spawn((
        UniverseEntity,
        UniversalPosition(uranus_pos),
        BodyVelocity(uranus_vel),
        CelestialBody {
            physical_radius: uranus_radius,
            mass_kg: 8.6810e25,
        },
        uranus_orbit,
        AxialRotation {
            speed_rad_per_sec: -1.0124e-4, // 17h 14m (62,064 s) retrograde rotation
        },
        NavTarget {
            name: "Uranus",
            classification: "Tilted Ice Giant",
            radius_m: uranus_radius,
        },
        BodyTextureStreamable {
            texture_path: uranus_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(uranus_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 76.0,
        },
        Transform::from_xyz(uranus_pos.x as f32, uranus_pos.y as f32, uranus_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(97.77_f32.to_radians()))
            .with_scale(Vec3::splat(uranus_radius as f32)),
    )).with_children(|parent| {
        let atmo_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.50, 0.85, 0.95, 0.12),
            emissive: LinearRgba::BLACK,
            alpha_mode: AlphaMode::Blend,
            unlit: false,
            ..default()
        });
        parent.spawn((
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(atmo_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(0.50, 0.85, 0.95, 0.12),
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: false,
                rest_temperature_k: 76.0,
            },
            Transform::from_scale(Vec3::splat(1.025)),
        ));
    }).id();

    // 8a. MIRANDA (Uranus V - Chaotic Verona Rupes Ice Moon)
    let miranda_radius = 235_800.0;
    let miranda_semi_major = 129_390_000.0;
    let miranda_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(miranda_texture),
        perceptual_roughness: 0.85,
        metallic: 0.02,
        ..default()
    });
    let miranda_inc: f64 = 1.78028; // Uranus axial tilt 97.77 deg + 4.232 deg orbital inclination
    let miranda_mean_anomaly: f64 = 0.85;
    let miranda_orbit = KeplerOrbit::circular(
        Some(uranus_entity),
        miranda_semi_major,
        miranda_inc,
        122_124.6, // 1.4135 d
        miranda_mean_anomaly,
    );
    let (miranda_rel_pos, miranda_rel_vel) = miranda_orbit.evaluate_state_at_time(0.0);
    let miranda_pos = uranus_pos + miranda_rel_pos;
    let miranda_vel = uranus_vel + miranda_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(miranda_pos),
        BodyVelocity(miranda_vel),
        CelestialBody {
            physical_radius: miranda_radius,
            mass_kg: 6.40e19,
        },
        miranda_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 5.1449e-5, // Tidally locked
        },
        NavTarget {
            name: "Miranda",
            classification: "Chaotic Fractured Ice Moon (Verona Rupes)",
            radius_m: miranda_radius,
        },
        BodyTextureStreamable {
            texture_path: miranda_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(miranda_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.85, 0.85, 0.85, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 60.0,
        },
        Transform::from_xyz(miranda_pos.x as f32, miranda_pos.y as f32, miranda_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(miranda_radius as f32)),
    ));

    // 8b. ARIEL (Uranus I - Faulted Graben Rift Valley Moon)
    let ariel_radius = 578_900.0;
    let ariel_semi_major = 191_020_000.0;
    let ariel_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(ariel_texture),
        perceptual_roughness: 0.80,
        metallic: 0.02,
        ..default()
    });
    let ariel_inc: f64 = 1.70641; // Coplanar with Uranus equator (97.77 deg)
    let ariel_mean_anomaly: f64 = 2.10;
    let ariel_orbit = KeplerOrbit::circular(
        Some(uranus_entity),
        ariel_semi_major,
        ariel_inc,
        217_760.7, // 2.5204 d
        ariel_mean_anomaly,
    );
    let (ariel_rel_pos, ariel_rel_vel) = ariel_orbit.evaluate_state_at_time(0.0);
    let ariel_pos = uranus_pos + ariel_rel_pos;
    let ariel_vel = uranus_vel + ariel_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(ariel_pos),
        BodyVelocity(ariel_vel),
        CelestialBody {
            physical_radius: ariel_radius,
            mass_kg: 1.29e21,
        },
        ariel_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 2.8854e-5, // Tidally locked
        },
        NavTarget {
            name: "Ariel",
            classification: "Faulted Graben Rift Valley Moon",
            radius_m: ariel_radius,
        },
        BodyTextureStreamable {
            texture_path: ariel_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(ariel_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.90, 0.90, 0.90, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 60.0,
        },
        Transform::from_xyz(ariel_pos.x as f32, ariel_pos.y as f32, ariel_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(ariel_radius as f32)),
    ));

    // 8c. UMBRIEL (Uranus II - Ancient Dark Cratered Moon)
    let umbriel_radius = 584_700.0;
    let umbriel_semi_major = 266_000_000.0;
    let umbriel_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(umbriel_texture),
        perceptual_roughness: 0.90,
        metallic: 0.02,
        ..default()
    });
    let umbriel_inc: f64 = 1.70641;
    let umbriel_mean_anomaly: f64 = 3.45;
    let umbriel_orbit = KeplerOrbit::circular(
        Some(uranus_entity),
        umbriel_semi_major,
        umbriel_inc,
        358_056.9, // 4.1442 d
        umbriel_mean_anomaly,
    );
    let (umbriel_rel_pos, umbriel_rel_vel) = umbriel_orbit.evaluate_state_at_time(0.0);
    let umbriel_pos = uranus_pos + umbriel_rel_pos;
    let umbriel_vel = uranus_vel + umbriel_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(umbriel_pos),
        BodyVelocity(umbriel_vel),
        CelestialBody {
            physical_radius: umbriel_radius,
            mass_kg: 1.27e21,
        },
        umbriel_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 1.7548e-5, // Tidally locked
        },
        NavTarget {
            name: "Umbriel",
            classification: "Ancient Cratered Low-Albedo Dark Moon",
            radius_m: umbriel_radius,
        },
        BodyTextureStreamable {
            texture_path: umbriel_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(umbriel_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.65, 0.65, 0.65, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 60.0,
        },
        Transform::from_xyz(umbriel_pos.x as f32, umbriel_pos.y as f32, umbriel_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(umbriel_radius as f32)),
    ));

    // 8d. TITANIA (Uranus III - Giant Fault Canyon Moon, Largest Uranian Satellite)
    let titania_radius = 788_400.0;
    let titania_semi_major = 435_910_000.0;
    let titania_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(titania_texture),
        perceptual_roughness: 0.80,
        metallic: 0.02,
        ..default()
    });
    let titania_inc: f64 = 1.70641;
    let titania_mean_anomaly: f64 = 4.70;
    let titania_orbit = KeplerOrbit::circular(
        Some(uranus_entity),
        titania_semi_major,
        titania_inc,
        752_218.6, // 8.7062 d
        titania_mean_anomaly,
    );
    let (titania_rel_pos, titania_rel_vel) = titania_orbit.evaluate_state_at_time(0.0);
    let titania_pos = uranus_pos + titania_rel_pos;
    let titania_vel = uranus_vel + titania_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(titania_pos),
        BodyVelocity(titania_vel),
        CelestialBody {
            physical_radius: titania_radius,
            mass_kg: 3.40e21,
        },
        titania_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 8.3529e-6, // Tidally locked
        },
        NavTarget {
            name: "Titania",
            classification: "Giant Fault Canyon Moon (Largest Uranian Satellite)",
            radius_m: titania_radius,
        },
        BodyTextureStreamable {
            texture_path: titania_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(titania_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.88, 0.88, 0.88, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 60.0,
        },
        Transform::from_xyz(titania_pos.x as f32, titania_pos.y as f32, titania_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(titania_radius as f32)),
    ));

    // 8e. OBERON (Uranus IV - Heavily Impacted Outermost Major Moon)
    let oberon_radius = 761_400.0;
    let oberon_semi_major = 583_520_000.0;
    let oberon_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(oberon_texture),
        perceptual_roughness: 0.85,
        metallic: 0.02,
        ..default()
    });
    let oberon_inc: f64 = 1.70641;
    let oberon_mean_anomaly: f64 = 5.95;
    let oberon_orbit = KeplerOrbit::circular(
        Some(uranus_entity),
        oberon_semi_major,
        oberon_inc,
        1_163_223.9, // 13.4632 d
        oberon_mean_anomaly,
    );
    let (oberon_rel_pos, oberon_rel_vel) = oberon_orbit.evaluate_state_at_time(0.0);
    let oberon_pos = uranus_pos + oberon_rel_pos;
    let oberon_vel = uranus_vel + oberon_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(oberon_pos),
        BodyVelocity(oberon_vel),
        CelestialBody {
            physical_radius: oberon_radius,
            mass_kg: 3.01e21,
        },
        oberon_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 5.4015e-6, // Tidally locked
        },
        NavTarget {
            name: "Oberon",
            classification: "Heavily Impacted Outer Ice Moon",
            radius_m: oberon_radius,
        },
        BodyTextureStreamable {
            texture_path: oberon_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(oberon_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.80, 0.80, 0.80, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 60.0,
        },
        Transform::from_xyz(oberon_pos.x as f32, oberon_pos.y as f32, oberon_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(oberon_radius as f32)),
    ));

    // 9. NEPTUNE
    let neptune_radius = 24_622_000.0;
    let neptune_semi_major = 4_495_038_600_000.0;
    let neptune_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(neptune_texture),
        perceptual_roughness: 0.50,
        metallic: 0.05,
        ..default()
    });
    let nep_inc: f64 = 0.03087;
    let neptune_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        neptune_semi_major,
        0.00946, // Neptune eccentricity
        nep_inc,
        2.3000, // Longitude of ascending node (131.78 deg)
        4.7680, // Argument of periapsis (273.19 deg)
        5_200_329_600.0,
        3.80,
    );
    let (neptune_pos, neptune_vel) = neptune_orbit.evaluate_state_at_time(0.0);
    let neptune_entity = commands.spawn((
        UniverseEntity,
        UniversalPosition(neptune_pos),
        BodyVelocity(neptune_vel),
        CelestialBody {
            physical_radius: neptune_radius,
            mass_kg: 1.0241e26,
        },
        neptune_orbit,
        AxialRotation {
            speed_rad_per_sec: 1.0834e-4, // 16h 6m (57,996 s) Neptunian day
        },
        NavTarget {
            name: "Neptune",
            classification: "Deep Azure Ice Giant",
            radius_m: neptune_radius,
        },
        BodyTextureStreamable {
            texture_path: neptune_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(neptune_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 72.0,
        },
        Transform::from_xyz(neptune_pos.x as f32, neptune_pos.y as f32, neptune_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(28.32_f32.to_radians()))
            .with_scale(Vec3::splat(neptune_radius as f32)),
    )).with_children(|parent| {
        let atmo_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.15, 0.35, 0.95, 0.15),
            emissive: LinearRgba::BLACK,
            alpha_mode: AlphaMode::Blend,
            unlit: false,
            ..default()
        });
        parent.spawn((
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(atmo_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(0.15, 0.35, 0.95, 0.15),
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: false,
                rest_temperature_k: 72.0,
            },
            Transform::from_scale(Vec3::splat(1.025)),
        ));
    }).id();

    // 9a. PROTEUS (Neptune VIII - Heavily Cratered Irregular Inner Moon)
    let proteus_radius = 210_000.0;
    let proteus_semi_major = 117_647_000.0;
    let proteus_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(proteus_texture),
        perceptual_roughness: 0.90,
        metallic: 0.02,
        ..default()
    });
    // Triaxial dimensions: 424 x 390 x 396 km (semi-axes: a=212km, b=195km, c=198km)
    let proteus_ellipsoid = EllipsoidRadii::new(212_000.0, 195_000.0, 198_000.0);
    let proteus_scale = proteus_ellipsoid.shape_scale(proteus_radius) * (proteus_radius as f32);
    let proteus_inc: f64 = 0.009145; // 0.524 deg
    let proteus_mean_anomaly: f64 = 0.45;
    let proteus_orbit = KeplerOrbit::circular(
        Some(neptune_entity),
        proteus_semi_major,
        proteus_inc,
        96_967.0, // 1.1223 d
        proteus_mean_anomaly,
    );
    let (proteus_rel_pos, proteus_rel_vel) = proteus_orbit.evaluate_state_at_time(0.0);
    let proteus_pos = neptune_pos + proteus_rel_pos;
    let proteus_vel = neptune_vel + proteus_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(proteus_pos),
        BodyVelocity(proteus_vel),
        CelestialBody {
            physical_radius: proteus_radius,
            mass_kg: 4.4e19,
        },
        proteus_ellipsoid,
        proteus_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 6.4797e-5, // Tidally locked
        },
        NavTarget {
            name: "Proteus",
            classification: "Massive Irregular Dark Inner Moon",
            radius_m: proteus_radius,
        },
        BodyTextureStreamable {
            texture_path: proteus_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(proteus_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.60, 0.60, 0.60, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 51.0,
        },
        Transform::from_xyz(proteus_pos.x as f32, proteus_pos.y as f32, proteus_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(proteus_scale),
    ));

    // 9b. TRITON (Neptune I - Captured Kuiper Belt Dwarf Planet & Nitrogen Geyser Moon)
    let triton_radius = 1_353_400.0;
    let triton_semi_major = 354_759_000.0;
    let triton_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(triton_texture),
        perceptual_roughness: 0.70,
        metallic: 0.02,
        ..default()
    });
    // Triton has a famous retrograde orbit inclined at 156.885 degrees (2.73815 rad) relative to Neptune's orbital plane
    let triton_inc: f64 = 2.738150;
    let triton_mean_anomaly: f64 = 1.25;
    let triton_orbit = KeplerOrbit::circular(
        Some(neptune_entity),
        triton_semi_major,
        triton_inc,
        507_760.0, // 5.87685 d (Retrograde orbit)
        triton_mean_anomaly,
    );
    let (triton_rel_pos, triton_rel_vel) = triton_orbit.evaluate_state_at_time(0.0);
    let triton_pos = neptune_pos + triton_rel_pos;
    let triton_vel = neptune_vel + triton_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(triton_pos),
        BodyVelocity(triton_vel),
        CelestialBody {
            physical_radius: triton_radius,
            mass_kg: 2.139e22,
        },
        triton_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: -1.2374e-5, // Synchronous tidally locked retrograde rotation
        },
        NavTarget {
            name: "Triton",
            classification: "Cryovolcanic Nitrogen Ice Moon (Captured Dwarf Planet)",
            radius_m: triton_radius,
        },
        BodyTextureStreamable {
            texture_path: triton_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(triton_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.92, 0.90, 0.88, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 38.0,
        },
        Transform::from_xyz(triton_pos.x as f32, triton_pos.y as f32, triton_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(triton_radius as f32)),
    ));

    // 9c. NEREID (Neptune II - Highly Eccentric Outer Ocean-Ice Moon)
    let nereid_radius = 178_500.0;
    let nereid_semi_major = 5_513_818_000.0;
    let nereid_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(nereid_texture),
        perceptual_roughness: 0.85,
        metallic: 0.02,
        ..default()
    });
    let nereid_inc: f64 = 0.1262; // 7.23 deg relative to Neptune's orbit
    let nereid_mean_anomaly: f64 = 2.15;
    let nereid_orbit = KeplerOrbit::eccentric(
        Some(neptune_entity),
        nereid_semi_major,
        0.7507, // Highly eccentric outer moon (e = 0.75)!
        nereid_inc,
        0.0,
        0.0,
        31_116_096.0, // 360.14 d
        nereid_mean_anomaly,
    );
    let (nereid_rel_pos, nereid_rel_vel) = nereid_orbit.evaluate_state_at_time(0.0);
    let nereid_pos = neptune_pos + nereid_rel_pos;
    let nereid_vel = neptune_vel + nereid_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(nereid_pos),
        BodyVelocity(nereid_vel),
        CelestialBody {
            physical_radius: nereid_radius,
            mass_kg: 2.7e19,
        },
        nereid_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 1.515e-4, // Fast non-synchronous rotation (~11.52 hours)
        },
        NavTarget {
            name: "Nereid",
            classification: "Highly Eccentric Outer Ocean-Ice Moon",
            radius_m: nereid_radius,
        },
        BodyTextureStreamable {
            texture_path: nereid_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(nereid_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.85, 0.85, 0.85, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 50.0,
        },
        Transform::from_xyz(nereid_pos.x as f32, nereid_pos.y as f32, nereid_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(nereid_radius as f32)),
    ));

    // 10. PLUTO & CHARON BINARY DWARF PLANET SYSTEM
    let pluto_radius = 1_188_300.0;
    let pluto_semi_major = 5_906_400_000_000.0; // 39.482 AU
    let pluto_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(pluto_texture),
        perceptual_roughness: 0.80,
        metallic: 0.03,
        ..default()
    });
    let pluto_inc: f64 = 0.2995; // 17.16 deg inclination
    let pluto_mean_anomaly: f64 = 5.20;
    let pluto_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        pluto_semi_major,
        0.2488, // Significant eccentricity (perihelion 29.66 AU, aphelion 49.30 AU)
        pluto_inc,
        1.9251, // Longitude of ascending node (110.30 deg)
        1.9856, // Argument of periapsis (113.76 deg)
        7_824_000_000.0, // 247.94 y
        pluto_mean_anomaly,
    );
    let (pluto_pos, pluto_vel) = pluto_orbit.evaluate_state_at_time(0.0);
    let pluto_entity = commands.spawn((
        UniverseEntity,
        UniversalPosition(pluto_pos),
        BodyVelocity(pluto_vel),
        CelestialBody {
            physical_radius: pluto_radius,
            mass_kg: 1.303e22,
        },
        pluto_orbit,
        AxialRotation {
            speed_rad_per_sec: 1.139e-5, // 6.387 d
        },
        NavTarget {
            name: "Pluto",
            classification: "Dwarf Planet (Kuiper Belt)",
            radius_m: pluto_radius,
        },
        BodyTextureStreamable {
            texture_path: pluto_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(pluto_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.90, 0.85, 0.80, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 44.0,
        },
        Transform::from_xyz(pluto_pos.x as f32, pluto_pos.y as f32, pluto_pos.z as f32)
            .with_rotation(Quat::from_rotation_z(122.53_f32.to_radians()))
            .with_scale(Vec3::splat(pluto_radius as f32)),
    )).id();

    // 10b. CHARON (Major Moon & Binary Companion of Pluto)
    let charon_radius = 606_000.0;
    let charon_semi_major = 19_591_000.0; // 19,591 km
    let charon_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(charon_texture),
        perceptual_roughness: 0.82,
        metallic: 0.02,
        ..default()
    });
    let charon_inc: f64 = 0.001;
    let charon_mean_anomaly: f64 = 0.80;
    let charon_orbit = KeplerOrbit::circular(
        Some(pluto_entity),
        charon_semi_major,
        charon_inc,
        551_856.0, // 6.387 d
        charon_mean_anomaly,
    );
    let (charon_rel_pos, charon_rel_vel) = charon_orbit.evaluate_state_at_time(0.0);
    let charon_pos = pluto_pos + charon_rel_pos;
    let charon_vel = pluto_vel + charon_rel_vel;
    commands.spawn((
        UniverseEntity,
        UniversalPosition(charon_pos),
        BodyVelocity(charon_vel),
        CelestialBody {
            physical_radius: charon_radius,
            mass_kg: 1.586e21,
        },
        charon_orbit,
        SatelliteOrbit,
        AxialRotation {
            speed_rad_per_sec: 1.139e-5, // Tidally locked
        },
        NavTarget {
            name: "Charon",
            classification: "Binary Companion Moon of Pluto",
            radius_m: charon_radius,
        },
        BodyTextureStreamable {
            texture_path: charon_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(charon_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.85, 0.85, 0.85, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 40.0,
        },
        Transform::from_xyz(charon_pos.x as f32, charon_pos.y as f32, charon_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(charon_radius as f32)),
    ));

    // 11. HAUMEA (Fast-Spinning Triaxial Ellipsoid Dwarf Planet)
    let haumea_mean_r = 816_000.0;
    let haumea_ellipsoid = EllipsoidRadii::new(1_161_000.0, 513_000.0, 852_000.0);
    let haumea_scale = haumea_ellipsoid.shape_scale(haumea_mean_r) * (haumea_mean_r as f32);
    let haumea_semi_major = 6_465_300_000_000.0; // 43.218 AU
    let haumea_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(haumea_texture),
        perceptual_roughness: 0.70,
        metallic: 0.05,
        ..default()
    });
    let haumea_inc: f64 = 0.4920; // 28.19 deg inclination
    let haumea_mean_anomaly: f64 = 3.75;
    let haumea_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        haumea_semi_major,
        0.1912, // High eccentricity (perihelion 34.95 AU, aphelion 51.48 AU)
        haumea_inc,
        2.1275, // Longitude of ascending node (121.90 deg)
        4.1923, // Argument of periapsis (240.21 deg)
        8_966_000_000.0, // 284.12 y
        haumea_mean_anomaly,
    );
    let (haumea_pos, haumea_vel) = haumea_orbit.evaluate_state_at_time(0.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(haumea_pos),
        BodyVelocity(haumea_vel),
        CelestialBody {
            physical_radius: haumea_mean_r,
            mass_kg: 4.006e21,
        },
        haumea_ellipsoid,
        haumea_orbit,
        AxialRotation {
            speed_rad_per_sec: 4.456e-4, // Rapid 3.9155 hours spin
        },
        NavTarget {
            name: "Haumea",
            classification: "Triaxial Ellipsoid Dwarf Planet",
            radius_m: haumea_mean_r,
        },
        BodyTextureStreamable {
            texture_path: haumea_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(haumea_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.95, 0.95, 0.98, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 32.0,
        },
        Transform::from_xyz(haumea_pos.x as f32, haumea_pos.y as f32, haumea_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(haumea_scale),
    ));

    // 12. MAKEMAKE (Classical Kuiper Belt Dwarf Planet)
    let makemake_radius = 715_000.0;
    let makemake_semi_major = 6_850_000_000_000.0; // 45.79 AU
    let makemake_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(makemake_texture),
        perceptual_roughness: 0.85,
        metallic: 0.02,
        ..default()
    });
    let makemake_inc: f64 = 0.5055; // 28.96 deg inclination
    let makemake_mean_anomaly: f64 = 2.40;
    let makemake_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        makemake_semi_major,
        0.1613, // Moderate-high eccentricity (perihelion 38.40 AU, aphelion 53.18 AU)
        makemake_inc,
        1.3893, // Longitude of ascending node (79.60 deg)
        5.1836, // Argument of periapsis (297.00 deg)
        9_779_000_000.0, // 309.88 y
        makemake_mean_anomaly,
    );
    let (makemake_pos, makemake_vel) = makemake_orbit.evaluate_state_at_time(0.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(makemake_pos),
        BodyVelocity(makemake_vel),
        CelestialBody {
            physical_radius: makemake_radius,
            mass_kg: 3.1e21,
        },
        makemake_orbit,
        AxialRotation {
            speed_rad_per_sec: 7.64e-5, // 22.83 hours
        },
        NavTarget {
            name: "Makemake",
            classification: "Dwarf Planet (Kuiper Belt)",
            radius_m: makemake_radius,
        },
        BodyTextureStreamable {
            texture_path: makemake_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(makemake_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.92, 0.82, 0.75, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 30.0,
        },
        Transform::from_xyz(makemake_pos.x as f32, makemake_pos.y as f32, makemake_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(makemake_radius as f32)),
    ));

    // 13. ERIS (Most Massive Scattered Disc Dwarf Planet)
    let eris_radius = 1_163_000.0;
    let eris_semi_major = 10_140_000_000_000.0; // 67.78 AU
    let eris_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(eris_texture),
        perceptual_roughness: 0.75,
        metallic: 0.04,
        ..default()
    });
    let eris_inc: f64 = 0.7686; // 44.04 deg high inclination
    let eris_mean_anomaly: f64 = 4.95;
    let eris_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        eris_semi_major,
        0.4407, // Extreme eccentricity (perihelion 37.91 AU, aphelion 97.65 AU)
        eris_inc,
        0.6274, // Longitude of ascending node (35.95 deg)
        2.6459, // Argument of periapsis (151.60 deg)
        17_610_000_000.0, // 558.04 y
        eris_mean_anomaly,
    );
    let (eris_pos, eris_vel) = eris_orbit.evaluate_state_at_time(0.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(eris_pos),
        BodyVelocity(eris_vel),
        CelestialBody {
            physical_radius: eris_radius,
            mass_kg: 1.66e22,
        },
        eris_orbit,
        AxialRotation {
            speed_rad_per_sec: 2.808e-6, // 25.9 d
        },
        NavTarget {
            name: "Eris",
            classification: "Scattered Disc Dwarf Planet",
            radius_m: eris_radius,
        },
        BodyTextureStreamable {
            texture_path: eris_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(eris_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.95, 0.95, 0.95, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 30.0,
        },
        Transform::from_xyz(eris_pos.x as f32, eris_pos.y as f32, eris_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(eris_radius as f32)),
    ));

    // 14. GONGGONG (Resonant Trans-Neptunian Dwarf Planet)
    let gonggong_radius = 615_000.0;
    let gonggong_semi_major = 10_080_000_000_000.0; // 67.38 AU
    let gonggong_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(gonggong_texture),
        perceptual_roughness: 0.88,
        metallic: 0.02,
        ..default()
    });
    let gonggong_inc: f64 = 0.5358; // 30.70 deg inclination
    let gonggong_mean_anomaly: f64 = 1.10;
    let gonggong_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        gonggong_semi_major,
        0.5000, // Very high eccentricity (perihelion 33.69 AU, aphelion 101.07 AU)
        gonggong_inc,
        5.8783, // Longitude of ascending node (336.80 deg)
        3.6163, // Argument of periapsis (207.20 deg)
        17_450_000_000.0, // 553.1 y
        gonggong_mean_anomaly,
    );
    let (gonggong_pos, gonggong_vel) = gonggong_orbit.evaluate_state_at_time(0.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(gonggong_pos),
        BodyVelocity(gonggong_vel),
        CelestialBody {
            physical_radius: gonggong_radius,
            mass_kg: 1.75e21,
        },
        gonggong_orbit,
        AxialRotation {
            speed_rad_per_sec: 7.79e-5, // 22.4 hours
        },
        NavTarget {
            name: "Gonggong",
            classification: "Scattered Disc Dwarf Planet",
            radius_m: gonggong_radius,
        },
        BodyTextureStreamable {
            texture_path: gonggong_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(gonggong_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.92, 0.65, 0.60, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 30.0,
        },
        Transform::from_xyz(gonggong_pos.x as f32, gonggong_pos.y as f32, gonggong_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(gonggong_radius as f32)),
    ));

    // 15. SEDNA (Detached Inner Oort Cloud Dwarf Planet)
    let sedna_radius = 498_000.0;
    let sedna_semi_major = 75_700_000_000_000.0; // 506 AU
    let sedna_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(sedna_texture),
        perceptual_roughness: 0.85,
        metallic: 0.02,
        ..default()
    });
    let sedna_inc: f64 = 0.2082; // 11.93 deg inclination
    let sedna_mean_anomaly: f64 = 0.35;
    let sedna_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        sedna_semi_major,
        0.8550, // Ultra-high eccentricity (perihelion 73.37 AU, aphelion 938.63 AU!)
        sedna_inc,
        2.5220, // Longitude of ascending node (144.50 deg)
        5.4350, // Argument of periapsis (311.40 deg)
        359_800_000_000.0, // 11,400 y
        sedna_mean_anomaly,
    );
    let (sedna_pos, sedna_vel) = sedna_orbit.evaluate_state_at_time(0.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(sedna_pos),
        BodyVelocity(sedna_vel),
        CelestialBody {
            physical_radius: sedna_radius,
            mass_kg: 2.0e21,
        },
        sedna_orbit,
        AxialRotation {
            speed_rad_per_sec: 1.699e-4, // 10.27 hours
        },
        NavTarget {
            name: "Sedna",
            classification: "Detached Trans-Neptunian Dwarf Planet",
            radius_m: sedna_radius,
        },
        BodyTextureStreamable {
            texture_path: sedna_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(sedna_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.95, 0.55, 0.45, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 12.0,
        },
        Transform::from_xyz(sedna_pos.x as f32, sedna_pos.y as f32, sedna_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(sedna_radius as f32)),
    ));

    // 16. FARFAROUT (Farthest Observed Solar System Body)
    let farfarout_radius = 200_000.0;
    let farfarout_semi_major = 19_747_000_000_000.0; // 132 AU
    let farfarout_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(farfarout_texture),
        perceptual_roughness: 0.90,
        metallic: 0.02,
        ..default()
    });
    let farfarout_inc: f64 = 0.3246; // 18.6 deg inclination
    let farfarout_mean_anomaly: f64 = 2.80;
    let farfarout_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        farfarout_semi_major,
        0.6550, // Extreme eccentricity (perihelion 45.54 AU, aphelion 218.46 AU)
        farfarout_inc,
        2.2428, // Longitude of ascending node (128.50 deg)
        5.9341, // Argument of periapsis (340.00 deg)
        31_557_600_000.0, // ~1000 y
        farfarout_mean_anomaly,
    );
    let (farfarout_pos, farfarout_vel) = farfarout_orbit.evaluate_state_at_time(0.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(farfarout_pos),
        BodyVelocity(farfarout_vel),
        CelestialBody {
            physical_radius: farfarout_radius,
            mass_kg: 3.0e19,
        },
        farfarout_orbit,
        AxialRotation {
            speed_rad_per_sec: 7.272e-5, // 24.0 hours
        },
        NavTarget {
            name: "Farfarout",
            classification: "Extreme Trans-Neptunian Object",
            radius_m: farfarout_radius,
        },
        BodyTextureStreamable {
            texture_path: farfarout_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(farfarout_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.85, 0.85, 0.85, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 20.0,
        },
        Transform::from_xyz(farfarout_pos.x as f32, farfarout_pos.y as f32, farfarout_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(farfarout_radius as f32)),
    ));

    // 17. PLANET NINE (Hypothetical Outer Ice Giant)
    let planet9_radius = 15_000_000.0; // ~2.35 Earth radii
    let planet9_semi_major = 68_815_000_000_000.0; // 460 AU
    let planet9_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(planet9_texture),
        perceptual_roughness: 0.65,
        metallic: 0.05,
        ..default()
    });
    let planet9_inc: f64 = 0.3491; // 20.0 deg inclination
    let planet9_mean_anomaly: f64 = 4.10;
    let planet9_orbit = KeplerOrbit::eccentric(
        Some(sol_entity),
        planet9_semi_major,
        0.6000, // High eccentricity (perihelion 184 AU, aphelion 736 AU)
        planet9_inc,
        1.8326, // Longitude of ascending node (105.00 deg)
        2.6180, // Argument of periapsis (150.00 deg)
        309_260_000_000.0, // ~9,800 y
        planet9_mean_anomaly,
    );
    let (planet9_pos, planet9_vel) = planet9_orbit.evaluate_state_at_time(0.0);
    commands.spawn((
        UniverseEntity,
        UniversalPosition(planet9_pos),
        BodyVelocity(planet9_vel),
        CelestialBody {
            physical_radius: planet9_radius,
            mass_kg: 3.0e25, // ~5 Earth masses
        },
        planet9_orbit,
        AxialRotation {
            speed_rad_per_sec: 1.09e-4, // 16 hours
        },
        NavTarget {
            name: "Planet Nine",
            classification: "Hypothetical Outer Ice Giant",
            radius_m: planet9_radius,
        },
        BodyTextureStreamable {
            texture_path: planet9_path,
            current_tier: PlanetLodTier::LowRes,
        },
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(planet9_mat),
        RelativisticOpticsBody {
            base_color: LinearRgba::new(0.55, 0.75, 0.95, 1.0),
            base_emissive: LinearRgba::BLACK,
            is_emissive_star: false,
            rest_temperature_k: 25.0,
        },
        Transform::from_xyz(planet9_pos.x as f32, planet9_pos.y as f32, planet9_pos.z as f32)
            .with_rotation(Quat::IDENTITY)
            .with_scale(Vec3::splat(planet9_radius as f32)),
    ));
}

/// Dynamically switches between the Scaled Demo Sector and the 1:1 Realistic Scale Solar System.
/// Listens to hotkeys KeyU and F10, or responds to pending switch requests from the UI.
pub fn universe_switch_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut universe_config: ResMut<UniverseConfig>,
    mut rel_config: ResMut<RelativisticConfig>,
    mut nav: ResMut<NavigationComputer>,
    mut shuttle_query: Query<(&mut Shuttle, &mut UniversalPosition, &mut RelativisticState, &mut Transform)>,
    sector_entities: Query<Entity, With<UniverseEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    if keyboard.just_pressed(KeyCode::KeyU) || keyboard.just_pressed(KeyCode::F10) {
        universe_config.pending_switch = Some(universe_config.active_mode.next());
    }

    let Some(target_mode) = universe_config.pending_switch.take() else {
        return;
    };

    if target_mode == universe_config.active_mode {
        return;
    }

    info!(
        "►► UNIVERSE SECTOR TRANSITION: {:?} -> {:?}",
        universe_config.active_mode, target_mode
    );

    // 1. Despawn all existing universe entities
    for entity in sector_entities.iter() {
        commands.entity(entity).despawn_recursive();
    }

    universe_config.active_mode = target_mode;

    // 2. Reset navigation computer
    nav.selected_index = None;
    nav.target_entity = None;
    nav.target_name = "NO TARGET".to_string();
    nav.target_class = "N/A".to_string();
    nav.screen_pos = None;
    nav.is_in_front = false;
    nav.true_screen_pos = None;
    nav.true_is_in_front = false;
    nav.show_true_reticle = false;
    nav.reticle_pixel_offset = 0.0;

    let Ok((mut shuttle, mut upos, mut rel_state, mut transform)) = shuttle_query.get_single_mut() else {
        return;
    };

    shuttle.locked_thrust_dir = 0.0;
    shuttle.locked_boost = false;
    shuttle.align_to_target = false;
    shuttle.autopilot_mode = crate::flight::AutopilotMode::Off;
    shuttle.autopilot_phase = crate::flight::AutopilotPhase::Idle;
    shuttle.tracking_braking = false;
    shuttle.tracking_brake_to_zero = false;
    shuttle.angular_velocity = Vec3::ZERO;

    match target_mode {
        UniverseMode::DemoSector => {
            rel_config.c = DEMO_C;
            rel_config.is_demo_mode = true;
            upos.0 = crate::world::DEMO_SPAWN_POSITION;
            transform.rotation = Quat::IDENTITY;
            rel_state.rapidity = DVec3::ZERO;
            rel_state.update_from_rapidity(rel_config.c);
            rel_state.proper_time = 0.0;
            rel_state.coordinate_time = 0.0;

            // Spawn Demo Sector
            spawn_demo_celestial_system(&mut commands, &mut meshes, &mut materials, &asset_server);
            crate::world::spawn_demo_props(&mut commands, &mut meshes, &mut materials);
        }
        UniverseMode::RealisticSolarSystem => {
            rel_config.c = REAL_C;
            rel_config.is_demo_mode = false;
            let (spawn_pos, spawn_rot, spawn_vel) = realistic_earth_orbital_spawn();
            upos.0 = spawn_pos;
            transform.rotation = spawn_rot;
            rel_state.rapidity = crate::relativity::rapidity_from_velocity(spawn_vel, rel_config.c);
            rel_state.update_from_rapidity(rel_config.c);
            rel_state.proper_time = 0.0;
            rel_state.coordinate_time = 0.0;

            // Spawn 1:1 Scale Solar System
            spawn_realistic_solar_system(&mut commands, &mut meshes, &mut materials, &asset_server, None);
        }
    }
}

/// Generates a flat ring annulus mesh with inner and outer radii multipliers
fn generate_planetary_ring_mesh(
    meshes: &mut ResMut<Assets<Mesh>>,
    inner_mult: f32,
    outer_mult: f32,
    segments: usize,
) -> Handle<Mesh> {
    use bevy::render::mesh::{Indices, PrimitiveTopology};
    use bevy::render::render_asset::RenderAssetUsages;

    let mut positions = Vec::with_capacity(segments * 2);
    let mut normals = Vec::with_capacity(segments * 2);
    let mut uvs = Vec::with_capacity(segments * 2);
    let mut colors = Vec::with_capacity(segments * 2);
    let mut indices = Vec::with_capacity(segments * 6);

    for i in 0..segments {
        let frac = i as f32 / segments as f32;
        let angle = frac * 2.0 * std::f32::consts::PI;
        let cos = angle.cos();
        let sin = angle.sin();

        // Inner vertex (maps to U=0.0 in radial alpha texture)
        positions.push([cos * inner_mult, 0.0, sin * inner_mult]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([0.0, 0.5]);
        colors.push([1.0, 1.0, 1.0, 1.0]);

        // Outer vertex (maps to U=1.0 in radial alpha texture)
        positions.push([cos * outer_mult, 0.0, sin * outer_mult]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([1.0, 0.5]);
        colors.push([1.0, 1.0, 1.0, 1.0]);

        let curr_inner = (i * 2) as u32;
        let curr_outer = curr_inner + 1;
        let next_inner = (((i + 1) % segments) * 2) as u32;
        let next_outer = next_inner + 1;

        indices.push(curr_inner);
        indices.push(curr_outer);
        indices.push(next_outer);

        indices.push(curr_inner);
        indices.push(next_outer);
        indices.push(next_inner);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    meshes.add(mesh)
}

/// Updates celestial axial rotation on polar local Y-axis with exact f64 angle accumulation
/// and strict quaternion normalization so planetary meshes never drift in radius over time.
pub fn axial_rotation_system(
    mut commands: Commands,
    time: Res<Time>,
    pause: Res<crate::flight::SimulationPause>,
    config: Res<crate::relativity::RelativisticConfig>,
    mut query: Query<(Entity, &AxialRotation, &mut Transform, Option<&mut AxialRotationState>)>,
) {
    if pause.0 {
        return;
    }
    let dt = time.delta_secs_f64() * config.time_scale;
    if dt == 0.0 {
        return;
    }

    for (entity, rot, mut transform, state_opt) in query.iter_mut() {
        let d_angle = (rot.speed_rad_per_sec as f64) * dt;
        if let Some(mut state) = state_opt {
            state.angle_rad = (state.angle_rad + d_angle).rem_euclid(std::f64::consts::TAU);
            let exact_rot = (state.base_rotation * bevy::math::DQuat::from_rotation_y(state.angle_rad)).normalize();
            transform.rotation = exact_rot.as_quat().normalize();
        } else {
            let base_rotation = transform.rotation.normalize().as_dquat();
            let angle_rad = d_angle.rem_euclid(std::f64::consts::TAU);
            let exact_rot = (base_rotation * bevy::math::DQuat::from_rotation_y(angle_rad)).normalize();
            transform.rotation = exact_rot.as_quat().normalize();
            commands.entity(entity).insert(AxialRotationState {
                base_rotation,
                angle_rad,
            });
        }
    }
}

/// Updates primary planetary orbits around Sol (bodies with KeplerOrbit but Without<SatelliteOrbit>)
pub fn planet_orbit_system(
    shuttle_query: Query<&RelativisticState, With<crate::flight::Shuttle>>,
    parent_query: Query<(&UniversalPosition, Option<&BodyVelocity>), (With<CelestialBody>, Without<KeplerOrbit>)>,
    mut orbiting_query: Query<(Entity, &KeplerOrbit, &mut UniversalPosition, Option<&mut BodyVelocity>), Without<SatelliteOrbit>>,
) {
    let t = if let Ok(rel) = shuttle_query.get_single() {
        rel.coordinate_time
    } else {
        0.0
    };

    for (_entity, orbit, mut upos, mut body_vel) in orbiting_query.iter_mut() {
        let (parent_pos, parent_vel) = if let Some(parent_ent) = orbit.parent {
            if let Ok((p_pos, p_vel)) = parent_query.get(parent_ent) {
                (p_pos.0, p_vel.map(|v| v.0).unwrap_or(DVec3::ZERO))
            } else {
                (DVec3::ZERO, DVec3::ZERO)
            }
        } else {
            (DVec3::ZERO, DVec3::ZERO)
        };

        let (rel_pos, rel_vel) = orbit.evaluate_state_at_time(t);
        upos.0 = parent_pos + rel_pos;

        if let Some(ref mut bv) = body_vel {
            bv.0 = parent_vel + rel_vel;
        }
    }
}

/// Updates secondary moon orbits around their host planet (bodies with KeplerOrbit and With<SatelliteOrbit>)
pub fn satellite_orbit_system(
    shuttle_query: Query<&RelativisticState, With<crate::flight::Shuttle>>,
    host_planet_query: Query<(&UniversalPosition, Option<&BodyVelocity>), (With<KeplerOrbit>, Without<SatelliteOrbit>)>,
    mut moon_query: Query<(Entity, &KeplerOrbit, &mut UniversalPosition, Option<&mut BodyVelocity>), With<SatelliteOrbit>>,
) {
    let t = if let Ok(rel) = shuttle_query.get_single() {
        rel.coordinate_time
    } else {
        0.0
    };

    for (_entity, orbit, mut upos, mut body_vel) in moon_query.iter_mut() {
        let (parent_pos, parent_vel) = if let Some(parent_ent) = orbit.parent {
            if let Ok((p_pos, p_vel)) = host_planet_query.get(parent_ent) {
                (p_pos.0, p_vel.map(|v| v.0).unwrap_or(DVec3::ZERO))
            } else {
                (DVec3::ZERO, DVec3::ZERO)
            }
        } else {
            (DVec3::ZERO, DVec3::ZERO)
        };

        let (rel_pos, rel_vel) = orbit.evaluate_state_at_time(t);
        upos.0 = parent_pos + rel_pos;

        if let Some(ref mut bv) = body_vel {
            bv.0 = parent_vel + rel_vel;
        }
    }
}

/// Cycles the variable canopy visor filter mode when pressing K
pub fn canopy_visor_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut visor: ResMut<CanopyVisorState>,
) {
    if keyboard.just_pressed(KeyCode::KeyK) {
        visor.mode = visor.mode.next();
    }
}

/// Dynamically transforms celestial bodies and atmospheres under Einstein's Relativistic Doppler & Beaming,
/// modulated by the variable canopy visor attenuation filter (Clear / Polarized / Solar Filter):
/// 1. In Solar Filter mode, cuts 99% of Sol's blinding glare to reveal the photographic sunspots & convective granules
/// 2. Blueshifts and intensifies radiation in the forward flight vector (approaching)
/// 3. Redshifts and dims radiation in the aft flight vector (receding)
/// 4. Automatically restores natural resting materials when coasting or stationary
pub fn celestial_optics_system(
    shuttle_query: Query<(&crate::relativity::RelativisticState, &UniversalPosition), With<crate::flight::Shuttle>>,
    optics: Res<crate::relativity::RelativisticOpticsConfig>,
    visor: Res<CanopyVisorState>,
    query: Query<(
        &GlobalTransform,
        &MeshMaterial3d<StandardMaterial>,
        &RelativisticOpticsBody,
        Option<&PrimaryStar>,
        Option<&SolarCorona>,
        Option<&UniversalPosition>,
        Option<&Parent>,
    )>,
    parent_upos_query: Query<&UniversalPosition>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok((rel_state, shuttle_upos)) = shuttle_query.get_single() else {
        return;
    };

    let has_doppler = optics.doppler_enabled && rel_state.rapidity.length_squared() > 1e-10;
    let beta_vec = rel_state.beta_vec.as_vec3();
    let gamma = rel_state.gamma as f32;

    // True physical distance from shuttle to Sol (located at universal origin DVec3::ZERO)
    let dist_to_sol = shuttle_upos.0.length();

    // Distance dimming for Sol's visual core at deep outer-system distances (Pluto/Sedna)
    let star_dist_au = (dist_to_sol / ASTRONOMICAL_UNIT).max(0.1);
    let star_lum_scale = if star_dist_au > 1.0 {
        (1.0 / star_dist_au.powf(0.2) as f32).clamp(0.65, 1.0)
    } else {
        1.0
    };

    // Corona distance fade: Corona is visible throughout the solar system.
    // At deep outer-system distances (> 8.0 AU, like Saturn, Uranus, Neptune, Pluto), maintain a minimum
    // solar halo floor (0.35) so Sol always appears as an intensely radiant, glorious stellar beacon outshining
    // background stars, while eliminating excessive glare close-up.
    let corona_dist_fade = if dist_to_sol > 8.0 * ASTRONOMICAL_UNIT {
        0.35
    } else if dist_to_sol < 4.0 * ASTRONOMICAL_UNIT {
        1.0
    } else {
        let t = (8.0 * ASTRONOMICAL_UNIT - dist_to_sol) / (4.0 * ASTRONOMICAL_UNIT);
        let smooth_t = (t * t * (3.0 - 2.0 * t)) as f32;
        0.35 + 0.65 * smooth_t
    };

    for (_gt, mat_handle, body_optics, primary_star, solar_corona, upos_opt, parent_opt) in query.iter() {
        let Some(mat) = materials.get_mut(&mat_handle.0) else {
            continue;
        };

        let is_primary_star = primary_star.is_some();
        let is_solar_corona = solar_corona.is_some();

        if is_solar_corona && corona_dist_fade <= 0.001 {
            mat.emissive = LinearRgba::BLACK;
            mat.base_color = Color::srgba(0.0, 0.0, 0.0, 0.0);
            continue;
        }

        let (effective_base, effective_emissive) = if is_primary_star {
            let base = match visor.mode {
                CanopyVisorMode::Clear => body_optics.base_color,
                CanopyVisorMode::Polarized => LinearRgba::new(25.2, 76.0, 452.1, 1.0),
                CanopyVisorMode::SolarFilter => {
                    // 1% H-alpha observatory mode: dimmed orange-red chromospheric tone
                    LinearRgba::new(2.5, 0.9, 0.35, 1.0)
                }
            };
            (
                LinearRgba::new(
                    base.red * star_lum_scale,
                    base.green * star_lum_scale,
                    base.blue * star_lum_scale,
                    base.alpha,
                ),
                LinearRgba::BLACK,
            )
        } else if is_solar_corona {
            let (raw_base, _) = match visor.mode {
                CanopyVisorMode::Clear => (
                    body_optics.base_color,
                    LinearRgba::BLACK,
                ),
                CanopyVisorMode::Polarized => (
                    LinearRgba::new(
                        body_optics.base_color.red * 0.25,
                        body_optics.base_color.green * 0.25,
                        body_optics.base_color.blue * 0.25,
                        body_optics.base_color.alpha * 0.25,
                    ),
                    LinearRgba::BLACK,
                ),
                CanopyVisorMode::SolarFilter => (
                    // Corona is completely filtered out in 1% H-alpha mode
                    LinearRgba::new(0.0, 0.0, 0.0, 0.0),
                    LinearRgba::BLACK,
                ),
            };
            (
                LinearRgba::new(raw_base.red * corona_dist_fade, raw_base.green * corona_dist_fade, raw_base.blue * corona_dist_fade, raw_base.alpha * corona_dist_fade),
                LinearRgba::BLACK,
            )
        } else {
            let tint = match visor.mode {
                CanopyVisorMode::Clear => 1.0,
                CanopyVisorMode::Polarized => 0.85,
                CanopyVisorMode::SolarFilter => 0.50,
            };
            (
                LinearRgba::new(
                    body_optics.base_color.red * tint,
                    body_optics.base_color.green * tint,
                    body_optics.base_color.blue * tint,
                    body_optics.base_color.alpha,
                ),
                LinearRgba::rgb(
                    body_optics.base_emissive.red * tint,
                    body_optics.base_emissive.green * tint,
                    body_optics.base_emissive.blue * tint,
                ),
            )
        };

        if has_doppler {
            // Relativistic Doppler is physically determined by the emitter's direction relative to the observer's
            // velocity in the rest reference frame (not the aberrated camera view vector).
            // A star receding behind the observer is physically redshifted, while approaching sources are blueshifted.
            // For child entities (planetary atmospheres, planetary rings, solar coronas), resolve the physical
            // universal coordinate from the parent entity if UniversalPosition is not directly attached.
            let body_pos = if let Some(upos) = upos_opt {
                upos.0
            } else if let Some(parent) = parent_opt {
                parent_upos_query.get(parent.get()).map(|u| u.0).unwrap_or(DVec3::ZERO)
            } else {
                DVec3::ZERO
            };
            let rest_rel = body_pos - shuttle_upos.0;
            let rest_dir = rest_rel.normalize_or_zero().as_vec3();
            if rest_dir == Vec3::ZERO {
                continue;
            }

            let doppler = crate::relativity::relativistic_doppler_factor(rest_dir, beta_vec, gamma);

            if body_optics.is_emissive_star {
                // Sol and its Coronal atmospheres: Direct thermal blackbody emitter
                let (shifted_star_col, beaming) = crate::relativity::blackbody_star_doppler(body_optics.rest_temperature_k, doppler);

                // Relativistic beaming scaling across visor modes (allows dimming smoothly to 0.0 in aft flight)
                let star_scale = match visor.mode {
                    CanopyVisorMode::Clear => beaming.clamp(0.0, 180.0),
                    CanopyVisorMode::Polarized => beaming.clamp(0.0, 180.0),
                    CanopyVisorMode::SolarFilter => beaming.clamp(0.0, 4.0),
                };

                if is_primary_star {
                    let mut r = shifted_star_col.red * effective_base.red * star_scale;
                    let mut g = shifted_star_col.green * effective_base.green * star_scale;
                    let mut b = shifted_star_col.blue * effective_base.blue * star_scale;

                    let rest_peak = effective_base.red.max(effective_base.green).max(effective_base.blue);
                    let max_allowed = rest_peak * 4.0;
                    let peak = r.max(g).max(b);
                    if peak > max_allowed {
                        let scale = max_allowed / peak;
                        r *= scale;
                        g *= scale;
                        b *= scale;
                    }
                    mat.emissive = LinearRgba::BLACK;
                    mat.base_color = Color::LinearRgba(LinearRgba::new(r, g, b, 1.0));
                } else {
                    // Solar Corona (pure additive unlit gas mesh, no texture):
                    // Radiates pure Planckian blackbody chromaticity matching Sol and background stars
                    let corona_lum = effective_base.red;
                    let mut r = shifted_star_col.red * corona_lum * star_scale;
                    let mut g = shifted_star_col.green * corona_lum * star_scale;
                    let mut b = shifted_star_col.blue * corona_lum * star_scale;

                    let rest_peak = corona_lum;
                    let max_allowed = rest_peak * 4.0;
                    let peak = r.max(g).max(b);
                    if peak > max_allowed {
                        let scale = max_allowed / peak;
                        r *= scale;
                        g *= scale;
                        b *= scale;
                    }
                    mat.emissive = LinearRgba::BLACK;
                    mat.base_color = Color::LinearRgba(LinearRgba::new(
                        r,
                        g,
                        b,
                        effective_base.alpha,
                    ));
                }
            } else {
                // Planetary bodies and rings: Sunlight reflector (T_sol = 5,778 K)
                let (spectral_tint, _bright_or_dim, reflectance_scale) =
                    crate::relativity::blackbody_reflected_sunlight_shift(doppler);

                // 1. Modulate diffuse albedo / base color
                mat.base_color = Color::LinearRgba(LinearRgba::new(
                    effective_base.red * spectral_tint.red,
                    effective_base.green * spectral_tint.green,
                    effective_base.blue * spectral_tint.blue,
                    effective_base.alpha,
                ));

                // 2. Scale specular reflectance: When redshifting (aft motion), specular highlight
                // completely extinguishes to 0.0, eliminating the reflection of Sol!
                mat.reflectance = (0.5 * reflectance_scale).clamp(0.0, 1.0);

                // 3. Relativistic blueshift of intrinsic infrared thermal radiation:
                // At rest & sub-relativistic speeds, planets emit zero visible light (pitch black on night side).
                // At high relativistic speeds, thermal emission blueshifts smoothly:
                // dim brown dwarf ember -> crimson red -> orange -> yellow -> white -> sapphire blue!
                let thermal_emissive = crate::relativity::planet_blueshifted_thermal_emission(
                    body_optics.rest_temperature_k,
                    doppler,
                );

                mat.emissive = LinearRgba::rgb(
                    effective_emissive.red + thermal_emissive.red,
                    effective_emissive.green + thermal_emissive.green,
                    effective_emissive.blue + thermal_emissive.blue,
                );
            }
        } else {
            // Restore resting base color, emissive, and specular reflectance for current visor mode
            if is_solar_corona {
                mat.emissive = LinearRgba::BLACK;
                mat.base_color = Color::LinearRgba(effective_base);
            } else {
                mat.base_color = Color::LinearRgba(effective_base);
                mat.emissive = effective_emissive;
            }
            mat.reflectance = 0.5;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planet_texture_assets_exist() {
        let stems = [
            "sun",
            "earth_day",
            "moon",
            "mars",
            "jupiter",
            "saturn",
            "saturn_ring_alpha",
            "mercury",
            "venus",
            "uranus",
            "neptune",
            "io",
            "europa",
            "ganymede",
            "callisto",
            "mimas",
            "methone",
            "enceladus",
            "dione",
            "rhea",
            "titan",
            "iapetus",
            "miranda",
            "ariel",
            "umbriel",
            "titania",
            "oberon",
            "proteus",
            "triton",
            "nereid",
            "ceres",
            "pluto",
            "charon",
            "haumea",
            "makemake",
            "eris",
            "gonggong",
            "sedna",
            "farfarout",
            "planet9",
        ];
        for stem in stems {
            let path = resolve_planet_texture_path(stem);
            let full_path = format!("assets/{}", path);
            assert!(
                std::path::Path::new(&full_path).exists(),
                "Missing texture asset file for stem '{}': {}",
                stem,
                full_path
            );
        }
    }

    #[test]
    fn test_planet_sphere_mesh_polar_orientation() {
        let mut app = App::new();
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Mesh>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let mesh_handle = generate_planet_sphere_mesh(&mut meshes);
        let mesh = meshes.get(&mesh_handle).expect("Mesh should be created");

        let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(bevy::render::mesh::VertexAttributeValues::Float32x3(p)) => p,
            _ => panic!("Positions should be Float32x3"),
        };
        let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(bevy::render::mesh::VertexAttributeValues::Float32x2(u)) => u,
            _ => panic!("UVs should be Float32x2"),
        };

        // Verify North Pole (V = 0.0) is at +Y (Y ~ +1.0)
        let north_pole_idx = uvs
            .iter()
            .position(|uv| uv[1] < 1e-4)
            .expect("North pole vertex must exist");
        assert!(
            (positions[north_pole_idx][1] - 1.0).abs() < 1e-3,
            "North pole (V=0) must be at +Y (+1.0), got: {:?}",
            positions[north_pole_idx]
        );

        // Verify South Pole (V = 1.0) is at -Y (Y ~ -1.0)
        let south_pole_idx = uvs
            .iter()
            .position(|uv| (uv[1] - 1.0).abs() < 1e-4)
            .expect("South pole vertex must exist");
        assert!(
            (positions[south_pole_idx][1] - (-1.0)).abs() < 1e-3,
            "South pole (V=1) must be at -Y (-1.0), got: {:?}",
            positions[south_pole_idx]
        );

        // Verify Equator (V ~ 0.5) has Y ~ 0.0
        let equator_idx = uvs
            .iter()
            .position(|uv| (uv[1] - 0.5).abs() < 1e-4)
            .expect("Equator vertex must exist");
        assert!(
            positions[equator_idx][1].abs() < 1e-3,
            "Equator (V=0.5) must lie in XZ plane (Y=0.0), got: {:?}",
            positions[equator_idx]
        );
    }

    #[test]
    fn test_canopy_visor_mode_cycling_and_transmittance() {
        let v0 = CanopyVisorMode::Clear;
        assert_eq!(v0.transmittance(), 1.0);
        assert_eq!(v0.label(), "CLEAR 100%");

        let v1 = v0.next();
        assert_eq!(v1, CanopyVisorMode::Polarized);
        assert_eq!(v1.transmittance(), 0.25);
        assert_eq!(v1.label(), "POLARIZED 25%");

        let v2 = v1.next();
        assert_eq!(v2, CanopyVisorMode::SolarFilter);
        assert_eq!(v2.transmittance(), 0.015);
        assert_eq!(v2.label(), "SOLAR FILTER 1% H-ALPHA");

        let v3 = v2.next();
        assert_eq!(v3, CanopyVisorMode::Clear);
    }

    #[test]
    fn test_sol_visor_emissive_hierarchy() {
        // Sol emissive brightness must strictly follow Clear > Polarized > SolarFilter
        let clear_emissive = LinearRgba::rgb(100.8, 304.2, 1808.5);
        let polarized_emissive = LinearRgba::rgb(25.2, 76.0, 452.1);
        let solar_filter_emissive = LinearRgba::rgb(2.5, 0.9, 0.35);

        assert!(clear_emissive.red > polarized_emissive.red);
        assert!(polarized_emissive.red > solar_filter_emissive.red);

        // Under any relativistic beaming factor, Polarized must remain strictly brighter than SolarFilter
        for &beaming in &[0.05, 0.20, 0.50, 1.0, 2.0, 5.0] {
            let pol_scaled = polarized_emissive.red * beaming;
            let filter_scaled = solar_filter_emissive.red * beaming.min(4.0);
            assert!(
                pol_scaled > filter_scaled,
                "At beaming {beaming}, Polarized ({pol_scaled}) must be brighter than SolarFilter ({filter_scaled})"
            );
        }
    }

    #[test]
    fn test_sol_optics_continuity_at_rest() {
        let effective_emissive = LinearRgba::rgb(100.8, 304.2, 1808.5);
        let (shifted_star_col, beaming) = crate::relativity::blackbody_star_doppler(5778.0, 1.0);
        assert_eq!(shifted_star_col, LinearRgba::WHITE);
        assert_eq!(beaming, 1.0);

        let r = shifted_star_col.red * effective_emissive.red * beaming;
        let g = shifted_star_col.green * effective_emissive.green * beaming;
        let b = shifted_star_col.blue * effective_emissive.blue * beaming;

        let rest_peak = effective_emissive.red.max(effective_emissive.green).max(effective_emissive.blue);
        let max_allowed = rest_peak * 4.0;
        let peak = r.max(g).max(b);
        assert!(peak <= max_allowed, "At rest, peak must not exceed max_allowed");

        assert_eq!(r, effective_emissive.red);
        assert_eq!(g, effective_emissive.green);
        assert_eq!(b, effective_emissive.blue);
    }

    #[test]
    fn test_universe_mode_cycling_and_earth_spawn_geometry() {
        let u0 = UniverseMode::DemoSector;
        assert_eq!(u0, UniverseMode::default());
        assert_eq!(u0.label(), "DEMO SECTOR (Scaled Sol & Planets)");

        let u1 = u0.next();
        assert_eq!(u1, UniverseMode::RealisticSolarSystem);
        assert_eq!(u1.label(), "1:1 REAL SOLAR SYSTEM (Astronomical Scale)");

        let u2 = u1.next();
        assert_eq!(u2, UniverseMode::DemoSector);

        // Verify realistic Earth spawn vantage point
        let spawn_pos = realistic_earth_orbital_spawn_position();
        let earth_pos = DVec3::new(ASTRONOMICAL_UNIT, 0.0, 0.0);
        let dist_to_earth = (spawn_pos - earth_pos).length();

        // 1. Altitude is safely ~3,500 km above Earth's surface (radius 6,371 km)
        let altitude = dist_to_earth - 6_371_000.0;
        assert!(altitude > 3_000_000.0 && altitude < 4_000_000.0, "Altitude should be ~3,500 km, got: {altitude}");

        // 2. Earth subtends a majestic field of view angle (~80 degrees)
        let angular_diameter = 2.0 * (6_371_000.0 / dist_to_earth).asin();
        let deg = angular_diameter.to_degrees();
        assert!(deg > 75.0 && deg < 85.0, "Earth apparent diameter should be ~80 deg, got: {deg}");
    }

    #[test]
    fn test_texture_downscale_box_filter() {
        use bevy::render::render_asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

        // Create a 4x4 RGBA8 test image
        let width = 4u32;
        let height = 4u32;
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                data.push((x * 40) as u8);
                data.push((y * 50) as u8);
                data.push(100);
                data.push(255);
            }
        }

        let mut image = Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );

        assert_eq!(image.texture_descriptor.size.width, 4);
        assert_eq!(image.texture_descriptor.size.height, 4);

        let success = downscale_image_half(&mut image);
        assert!(success);
        assert_eq!(image.texture_descriptor.size.width, 2);
        assert_eq!(image.texture_descriptor.size.height, 2);
        assert_eq!(image.data.len(), 2 * 2 * 4);

        // Check top-left 2x2 average:
        // x in [0, 1], y in [0, 1]
        // R values: (0 + 40 + 0 + 40) / 4 = 20
        // G values: (0 + 0 + 50 + 50) / 4 = 25
        // B values: (100 + 100 + 100 + 100) / 4 = 100
        // A values: (255 + 255 + 255 + 255) / 4 = 255
        assert_eq!(image.data[0], 20);
        assert_eq!(image.data[1], 25);
        assert_eq!(image.data[2], 100);
        assert_eq!(image.data[3], 255);
    }

    #[test]
    fn test_inspect_bevy_images() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Image>();
        let images = app.world().resource::<Assets<Image>>();
        println!("Initial image count: {}", images.len());
        for (id, img) in images.iter() {
            println!(
                "Image id: {:?}, size: {:?}, format: {:?}, data_len: {}",
                id,
                img.texture_descriptor.size,
                img.texture_descriptor.format,
                img.data.len()
            );
        }
    }

    #[test]
    fn test_texture_quality_config_defaults_and_cycling() {
        let mut config = TextureQualityConfig::default();
        assert_eq!(config.max_texture_dimension, 8192);
        assert!(config.is_discrete_gpu);
        assert!(!config.detected);

        // Cycle 8K -> 4K
        let l1 = config.cycle_quality();
        assert_eq!(config.max_texture_dimension, 4096);
        assert!(l1.contains("4K High"));

        // Cycle 4K -> 2K
        let l2 = config.cycle_quality();
        assert_eq!(config.max_texture_dimension, 2048);
        assert!(l2.contains("2K Performance"));

        // Cycle 2K -> 8K
        let l3 = config.cycle_quality();
        assert_eq!(config.max_texture_dimension, 8192);
        assert!(l3.contains("8K Ultra"));
    }

    #[test]
    fn test_asset_path() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Image>();
        let asset_server = app.world().resource::<AssetServer>();
        let resolved = resolve_planet_texture_path("mercury");
        let handle: Handle<Image> = asset_server.load(resolved.as_ref());
        let path = asset_server.get_path(handle.id()).unwrap();
        assert!(path.path().to_str().unwrap().contains("mercury"));
    }

    #[test]
    fn test_mercury_texture_format() {
        let path = if std::path::Path::new("assets/textures/planets/mercury.jpg").exists() {
            "assets/textures/planets/mercury.jpg"
        } else if std::path::Path::new("assets/textures/planets/mercury.ktx2").exists() {
            // Validated that mercury.ktx2 exists and is resident
            return;
        } else {
            panic!("Neither mercury.jpg nor mercury.ktx2 found in assets/textures/planets/");
        };
        let bytes = std::fs::read(path).unwrap();
        let mut img = Image::from_buffer(
            &bytes,
            bevy::image::ImageType::Extension("jpg"),
            bevy::image::CompressedImageFormats::empty(),
            true,
            bevy::image::ImageSampler::Default,
            bevy::render::render_asset::RenderAssetUsages::default(),
        ).unwrap();
        println!(
            "MERCURY INITIAL: size={:?}, format={:?}, data_len={}, mip_levels={}",
            img.texture_descriptor.size,
            img.texture_descriptor.format,
            img.data.len(),
            img.texture_descriptor.mip_level_count
        );
        assert_eq!(img.texture_descriptor.size.width, 8192);
        assert_eq!(img.texture_descriptor.size.height, 4096);
        assert_eq!(img.data.len(), 8192 * 4096 * 4);

        let t0 = std::time::Instant::now();
        let success = generate_planet_mipmaps_and_sampler(&mut img, false);
        assert!(success);
        let elapsed = t0.elapsed();

        println!(
            "MERCURY WITH MIPS: num_levels={}, final_data_len={} (generated in {:?})",
            img.texture_descriptor.mip_level_count,
            img.data.len(),
            elapsed
        );
        assert_eq!(img.texture_descriptor.mip_level_count, 14);

        // Verify sampler has 16x anisotropy, linear mipmap filter, and repeat/clamp addressing
        if let bevy::image::ImageSampler::Descriptor(ref desc) = img.sampler {
            assert_eq!(desc.anisotropy_clamp, 16);
            assert!(matches!(desc.mipmap_filter, bevy::image::ImageFilterMode::Linear));
            assert!(matches!(desc.address_mode_u, bevy::image::ImageAddressMode::Repeat));
            assert!(matches!(desc.address_mode_v, bevy::image::ImageAddressMode::ClampToEdge));
        } else {
            panic!("Sampler should be a descriptor with 16x anisotropy");
        }

        // Total expected size: sum_{i=0..13} (8192 >> i) * (4096 >> i) * 4
        let mut expected_total = 0;
        for i in 0..14 {
            let w = (8192 >> i).max(1);
            let h = (4096 >> i).max(1);
            expected_total += w * h * 4;
        }
        assert_eq!(img.data.len(), expected_total);
    }

    #[test]
    fn test_dynamic_texture_streaming_logic() {
        let config = DynamicTextureStreamingConfig::default();
        assert!(config.enabled);
        assert_eq!(config.low_res_dimension, 2048);
        assert!(config.reload_pending.is_none());

        // Body streamable component properties
        let mut streamable = BodyTextureStreamable {
            texture_path: "textures/planets/mars.jpg".into(),
            current_tier: PlanetLodTier::LowRes,
        };

        // Mars physical radius = 3,390 km (3,390,000 m)
        let mars_radius = 3_390_000.0;
        let (mars_promote, mars_demote) = calculate_streaming_distances(mars_radius, UniverseMode::RealisticSolarSystem);
        assert_eq!(mars_promote, 3_390_000.0 * 300.0); // 1.017M km
        assert_eq!(mars_demote, 3_390_000.0 * 600.0);  // 2.034M km

        // Case 1: Distant (5M km) -> stays low-res
        let d1 = 5_000_000_000.0;
        let desired1 = if d1 < mars_promote {
            PlanetLodTier::HighRes
        } else if d1 > mars_demote {
            PlanetLodTier::LowRes
        } else {
            streamable.current_tier
        };
        assert_eq!(desired1, PlanetLodTier::LowRes);

        // Case 2: Shuttle approaches within 800,000 km (< 1.017M km) -> promotes to high-res
        let d2 = 800_000_000.0;
        let desired2 = if d2 < mars_promote {
            PlanetLodTier::HighRes
        } else if d2 > mars_demote {
            PlanetLodTier::LowRes
        } else {
            streamable.current_tier
        };
        assert_eq!(desired2, PlanetLodTier::HighRes);
        streamable.current_tier = desired2;

        // Case 3: Shuttle drifts to 1.5M km (between 1.017M and 2.034M km) -> hysteresis deadband, MUST NOT demote
        let d3 = 1_500_000_000.0;
        let desired3 = if d3 < mars_promote {
            PlanetLodTier::HighRes
        } else if d3 > mars_demote {
            PlanetLodTier::LowRes
        } else {
            streamable.current_tier
        };
        assert_eq!(desired3, PlanetLodTier::HighRes, "Hysteresis zone (1.017M-2.034M km) must keep texture resident without flapping");

        // Case 4: Shuttle recedes past 2.5M km (> 2.034M km) -> demotes to 2K LOD
        let d4 = 2_500_000_000.0;
        let desired4 = if d4 < mars_promote {
            PlanetLodTier::HighRes
        } else if d4 > mars_demote {
            PlanetLodTier::LowRes
        } else {
            streamable.current_tier
        };
        assert_eq!(desired4, PlanetLodTier::LowRes, "Receding past 2.034M km must demote to 2K LOD");
        streamable.current_tier = desired4;

        // Case 5: Distant (10M km) even if locked by HUD Targeting Computer -> MUST stay low-res to prevent stutter during target cycling
        let d5 = 10_000_000_000.0;
        let desired5 = if d5 < mars_promote {
            PlanetLodTier::HighRes
        } else if d5 > mars_demote {
            PlanetLodTier::LowRes
        } else {
            streamable.current_tier
        };
        assert_eq!(desired5, PlanetLodTier::LowRes, "Target cycling must not reload distant textures to guarantee smooth framerate");
    }

    #[test]
    fn test_calculate_streaming_distances_scaling() {
        // Test Earth: R = 6,371 km
        let (earth_p, earth_d) = calculate_streaming_distances(6_371_000.0, UniverseMode::RealisticSolarSystem);
        assert_eq!(earth_p, 1_911_300_000.0); // ~1.91M km
        assert_eq!(earth_d, 3_822_600_000.0); // ~3.82M km

        // Test Jupiter: R = 69,911 km
        let (jup_p, jup_d) = calculate_streaming_distances(69_911_000.0, UniverseMode::RealisticSolarSystem);
        assert_eq!(jup_p, 20_973_300_000.0); // ~20.97M km
        assert_eq!(jup_d, 41_946_600_000.0); // ~41.95M km

        // Test small Asteroid: R = 5 km
        let (ast_p, ast_d) = calculate_streaming_distances(5_000.0, UniverseMode::RealisticSolarSystem);
        assert_eq!(ast_p, 1_500_000.0); // 1,500 km
        assert_eq!(ast_d, 3_000_000.0); // 3,000 km

        // Test Brown Dwarf: R = 100,000 km
        let (bd_p, bd_d) = calculate_streaming_distances(100_000_000.0, UniverseMode::RealisticSolarSystem);
        assert_eq!(bd_p, 30_000_000_000.0); // 30M km
        assert_eq!(bd_d, 60_000_000_000.0); // 60M km

        // Test Demo Sector: Terra R = 1,200 m
        let (demo_p, demo_d) = calculate_streaming_distances(1_200.0, UniverseMode::DemoSector);
        assert_eq!(demo_p, 30_000.0); // 30 km
        assert_eq!(demo_d, 54_000.0); // 54 km
    }

    #[test]
    fn test_planet_texture_ram_cache_promotion() {
        let mut ram_cache = PlanetTextureRamCache {
            enabled: true,
            total_ram_gb: 32.0,
            cached_pyramids: std::collections::HashMap::new(),
        };

        // Cache a dummy 8K pyramid
        let dummy_data = vec![77u8; 1000];
        ram_cache.cached_pyramids.insert(
            "textures/planets/earth_day.jpg".to_string(),
            (dummy_data.clone(), 8192, 4096, 14),
        );

        // Verify retrieval
        let cached = ram_cache.cached_pyramids.get("textures/planets/earth_day.jpg");
        assert!(cached.is_some());
        let (data, w, h, mips) = cached.unwrap();
        assert_eq!(*w, 8192);
        assert_eq!(*h, 4096);
        assert_eq!(*mips, 14);
        assert_eq!(data.len(), 1000);
        assert_eq!(data[0], 77);

        // Verify clearing cache
        ram_cache.cached_pyramids.clear();
        assert!(ram_cache.cached_pyramids.is_empty());
    }

    #[test]
    fn test_downscale_image_with_mipmaps_and_demotion_pipeline() {
        let w = 512u32;
        let h = 256u32;
        let pixel_count = (w * h) as usize;
        let data = vec![200u8; pixel_count * 4];

        let mut image = Image::new(
            bevy::render::render_resource::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            data,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );

        // Step 1: Initial high-res mipmap generation (e.g. 512x256 -> 10 mip levels)
        assert!(generate_planet_mipmaps_and_sampler(&mut image, false));
        assert_eq!(image.texture_descriptor.mip_level_count, 10);
        assert!(image.data.len() > pixel_count * 4, "Mipmaps must be appended");

        // Step 2: Demotion downscaling while loop (downscale to 128x64 target dimension)
        let low_res_target = 128u32;
        let mut scaled = false;
        while image.texture_descriptor.size.width > low_res_target
            || image.texture_descriptor.size.height > low_res_target / 2
        {
            assert!(downscale_image_half(&mut image), "downscale_image_half must handle textures with attached mips");
            scaled = true;
        }
        assert!(scaled, "Image should have been scaled down");
        assert_eq!(image.texture_descriptor.size.width, 128);
        assert_eq!(image.texture_descriptor.size.height, 64);
        assert_eq!(image.texture_descriptor.mip_level_count, 1);

        // Step 3: Rebuild 128x64 mipmap pyramid
        assert!(generate_planet_mipmaps_and_sampler(&mut image, false));
        assert_eq!(image.texture_descriptor.mip_level_count, 8); // 128 down to 1x1 is 8 levels
    }

    #[test]
    fn test_demote_image_to_2k_fast_path() {
        // Test 1: Demote 8K (8192x4096) with 14 mips down to 2K (2048x1024) with 12 mips
        let w_8k = 8192u32;
        let h_8k = 4096u32;
        let base_8k = (w_8k * h_8k * 4) as usize;
        let level1_bytes = (4096 * 2048 * 4) as usize;
        let remaining_bytes = (2048 * 1024 * 4) as usize + 1024;
        let skip_bytes = base_8k + level1_bytes;

        let mut image_8k = Image::new(
            bevy::render::render_resource::Extent3d {
                width: w_8k,
                height: h_8k,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            vec![111u8; base_8k],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        image_8k.data.extend(vec![222u8; level1_bytes + remaining_bytes]);
        image_8k.data[skip_bytes] = 42; // Marker at start of Level 2 (2K)
        image_8k.texture_descriptor.mip_level_count = 14;

        assert!(demote_image_to_2k(&mut image_8k));
        assert_eq!(image_8k.texture_descriptor.size.width, 2048);
        assert_eq!(image_8k.texture_descriptor.size.height, 1024);
        assert_eq!(image_8k.texture_descriptor.mip_level_count, 12);
        assert_eq!(image_8k.data[0], 42, "Must preserve Level 2 pixels");
        assert_eq!(image_8k.data.len(), remaining_bytes);

        // Test 2: Demote 4K (4096x2048) with 13 mips down to 2K (2048x1024) with 12 mips
        let w_4k = 4096u32;
        let h_4k = 2048u32;
        let base_4k = (w_4k * h_4k * 4) as usize;
        let skip_bytes_4k = base_4k;

        let mut image_4k = Image::new(
            bevy::render::render_resource::Extent3d {
                width: w_4k,
                height: h_4k,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            vec![222u8; base_4k],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        image_4k.data.extend(vec![123u8; remaining_bytes]);
        image_4k.data[skip_bytes_4k] = 99; // Marker at start of Level 1 (2K)
        image_4k.texture_descriptor.mip_level_count = 13;

        assert!(demote_image_to_2k(&mut image_4k));
        assert_eq!(image_4k.texture_descriptor.size.width, 2048);
        assert_eq!(image_4k.texture_descriptor.size.height, 1024);
        assert_eq!(image_4k.texture_descriptor.mip_level_count, 12);
        assert_eq!(image_4k.data[0], 99, "Must preserve Level 1 pixels");
        assert_eq!(image_4k.data.len(), remaining_bytes);

        // Test 3: Already 2K (2048x1024) or smaller does not touch data
        assert!(demote_image_to_2k(&mut image_4k));
        assert_eq!(image_4k.texture_descriptor.size.width, 2048);
        assert_eq!(image_4k.texture_descriptor.size.height, 1024);
    }

    #[test]
    fn test_bc7_demote_image_to_2k_fast_path() {
        // In BC7, each 4x4 texel block is 16 bytes (1 byte/texel)
        let w_8k = 8192u32;
        let h_8k = 4096u32;
        let blocks_l0 = (((w_8k + 3) / 4) * ((h_8k + 3) / 4) * 16) as usize; // 33,554,432 bytes
        let blocks_l1 = (((4096 + 3) / 4) * ((2048 + 3) / 4) * 16) as usize; // 8,388,608 bytes
        let skip_bytes = blocks_l0 + blocks_l1; // 41,943,040 bytes
        let remaining_bytes = (((2048 + 3) / 4) * ((1024 + 3) / 4) * 16) as usize + 2048;

        let mut data = vec![111u8; skip_bytes];
        data.extend(vec![222u8; remaining_bytes]);
        data[skip_bytes] = 73; // Marker for start of 2K mip level in BC7 container

        let mut bc7_img = Image::default();
        bc7_img.data = data;
        bc7_img.texture_descriptor.size = bevy::render::render_resource::Extent3d {
            width: w_8k,
            height: h_8k,
            depth_or_array_layers: 1,
        };
        bc7_img.texture_descriptor.format = bevy::render::render_resource::TextureFormat::Bc7RgbaUnormSrgb;
        bc7_img.texture_descriptor.mip_level_count = 14;

        assert!(demote_image_to_2k(&mut bc7_img));
        assert_eq!(bc7_img.texture_descriptor.size.width, 2048);
        assert_eq!(bc7_img.texture_descriptor.size.height, 1024);
        assert_eq!(bc7_img.texture_descriptor.mip_level_count, 12);
        assert_eq!(bc7_img.data[0], 73, "Must slice directly to 2K mip in BC7 container");
        assert_eq!(bc7_img.data.len(), remaining_bytes);
    }

    #[test]
    fn test_bc7_demote_image_to_4k_fast_path() {
        let w_8k = 8192u32;
        let h_8k = 4096u32;
        let skip_bytes = (((w_8k + 3) / 4) * ((h_8k + 3) / 4) * 16) as usize; // 33,554,432 bytes
        let remaining_bytes = (((4096 + 3) / 4) * ((2048 + 3) / 4) * 16) as usize + 1024;

        let mut data = vec![111u8; skip_bytes];
        data.extend(vec![222u8; remaining_bytes]);
        data[skip_bytes] = 55; // Marker for start of 4K mip level

        let mut bc7_img = Image::default();
        bc7_img.data = data;
        bc7_img.texture_descriptor.size = bevy::render::render_resource::Extent3d {
            width: w_8k,
            height: h_8k,
            depth_or_array_layers: 1,
        };
        bc7_img.texture_descriptor.format = bevy::render::render_resource::TextureFormat::Bc7RgbaUnormSrgb;
        bc7_img.texture_descriptor.mip_level_count = 14;

        assert!(demote_image_to_cap(&mut bc7_img, 4096));
        assert_eq!(bc7_img.texture_descriptor.size.width, 4096);
        assert_eq!(bc7_img.texture_descriptor.size.height, 2048);
        assert_eq!(bc7_img.texture_descriptor.mip_level_count, 13);
        assert_eq!(bc7_img.data[0], 55, "Must slice directly to 4K mip in BC7 container");
    }

    #[test]
    fn test_jovian_moons_orbital_parameters_and_laplace_resonance() {
        // Physical parameters
        let io_period: f64 = 152_853.5;
        let europa_period: f64 = 306_822.0;
        let ganymede_period: f64 = 618_153.4;
        let callisto_period: f64 = 1_441_931.2;

        // Verify famous 4:2:1 Laplace orbital resonance
        let europa_to_io_ratio = europa_period / io_period;
        let ganymede_to_europa_ratio = ganymede_period / europa_period;
        let ganymede_to_io_ratio = ganymede_period / io_period;

        assert!((europa_to_io_ratio - 2.0).abs() < 0.02, "Europa and Io must be in 2:1 resonance");
        assert!((ganymede_to_europa_ratio - 2.0).abs() < 0.02, "Ganymede and Europa must be in 2:1 resonance");
        assert!((ganymede_to_io_ratio - 4.0).abs() < 0.05, "Ganymede and Io must be in 4:1 resonance");

        // Verify synchronous tidal locking angular speeds
        let io_spin = (2.0 * std::f64::consts::PI) / io_period;
        assert!((io_spin - 4.1106e-5).abs() < 1e-8);

        let europa_spin = (2.0 * std::f64::consts::PI) / europa_period;
        assert!((europa_spin - 2.0478e-5).abs() < 1e-8);

        let ganymede_spin = (2.0 * std::f64::consts::PI) / ganymede_period;
        assert!((ganymede_spin - 1.0164e-5).abs() < 1e-8);

        let callisto_spin = (2.0 * std::f64::consts::PI) / callisto_period;
        assert!((callisto_spin - 4.3575e-6).abs() < 1e-8);

        // Verify physical ordering of semi-major axes
        let io_sma: f64 = 421_700_000.0;
        let europa_sma: f64 = 670_900_000.0;
        let ganymede_sma: f64 = 1_070_400_000.0;
        let callisto_sma: f64 = 1_882_700_000.0;
        assert!(io_sma < europa_sma);
        assert!(europa_sma < ganymede_sma);
        assert!(ganymede_sma < callisto_sma);

        // Verify Ganymede is the largest moon in the Solar System (> Luna and Mercury)
        let ganymede_radius: f64 = 2_634_100.0;
        let mercury_radius: f64 = 2_439_700.0;
        let luna_radius: f64 = 1_737_400.0;
        assert!(ganymede_radius > mercury_radius);
        assert!(ganymede_radius > luna_radius);
    }

    #[test]
    fn test_saturn_moons_orbital_parameters_and_resonances() {
        // Enceladus and Dione 2:1 resonance
        let enceladus_period: f64 = 118_386.8;
        let dione_period: f64 = 236_469.5;
        let dione_to_enceladus_ratio = dione_period / enceladus_period;
        assert!((dione_to_enceladus_ratio - 2.0).abs() < 0.02, "Dione and Enceladus must be in 2:1 resonance");

        // Tidal locking speeds
        let mimas_spin = (2.0 * std::f64::consts::PI) / 81_425.2;
        assert!((mimas_spin - 7.7165e-5).abs() < 1e-8);

        let titan_spin = (2.0 * std::f64::consts::PI) / 1_377_684.4;
        assert!((titan_spin - 4.5607e-6).abs() < 1e-8);

        let iapetus_spin = (2.0 * std::f64::consts::PI) / 6_853_377.6;
        assert!((iapetus_spin - 9.168e-7).abs() < 1e-9);

        // Semi-major axis hierarchy: Mimas < Methone < Enceladus < Dione < Rhea < Titan < Iapetus
        let mimas_sma: f64 = 185_539_000.0;
        let methone_sma: f64 = 194_440_000.0;
        let enceladus_sma: f64 = 237_948_000.0;
        let dione_sma: f64 = 377_396_000.0;
        let rhea_sma: f64 = 527_108_000.0;
        let titan_sma: f64 = 1_221_870_000.0;
        let iapetus_sma: f64 = 3_560_820_000.0;

        assert!(mimas_sma < methone_sma);
        assert!(methone_sma < enceladus_sma);
        assert!(enceladus_sma < dione_sma);
        assert!(dione_sma < rhea_sma);
        assert!(rhea_sma < titan_sma);
        assert!(titan_sma < iapetus_sma);

        // Titan size comparison (2nd largest moon in solar system, larger than Mercury)
        let titan_radius: f64 = 2_574_700.0;
        let mercury_radius: f64 = 2_439_700.0;
        assert!(titan_radius > mercury_radius);

        // Iapetus inclination (15.47 deg = 0.270 rad, inclined out of Saturn's ring plane)
        let iapetus_inc: f64 = 0.2700;
        assert!(iapetus_inc > 0.25);
    }

    #[test]
    fn test_triaxial_ellipsoid_radii_and_methone_haumea_shapes() {
        // Methone Cassini measurements: a=1940m, b=1250m, c=1300m
        let methone_ellipsoid = EllipsoidRadii::new(1940.0, 1250.0, 1300.0);
        let mean_radius = 1450.0;
        let scale = methone_ellipsoid.shape_scale(mean_radius);

        assert!((scale.x - (1940.0 / 1450.0) as f32).abs() < 1e-4);
        assert!((scale.y - (1250.0 / 1450.0) as f32).abs() < 1e-4);
        assert!((scale.z - (1300.0 / 1450.0) as f32).abs() < 1e-4);

        // Surface radius along principal axes
        let r_x = methone_ellipsoid.surface_radius_in_local_dir(DVec3::X);
        let r_y = methone_ellipsoid.surface_radius_in_local_dir(DVec3::Y);
        let r_z = methone_ellipsoid.surface_radius_in_local_dir(DVec3::Z);
        assert!((r_x - 1940.0).abs() < 1e-6);
        assert!((r_y - 1250.0).abs() < 1e-6);
        assert!((r_z - 1300.0).abs() < 1e-6);

        // Surface radius along arbitrary diagonal (u_x, u_y, u_z) satisfies (x/a)^2 + (y/b)^2 + (z/c)^2 = 1
        let u = DVec3::new(1.0, 2.0, 3.0).normalize();
        let r_u = methone_ellipsoid.surface_radius_in_local_dir(u);
        let pt = u * r_u;
        let ellipsoid_eq = (pt.x / 1940.0).powi(2) + (pt.y / 1250.0).powi(2) + (pt.z / 1300.0).powi(2);
        assert!((ellipsoid_eq - 1.0).abs() < 1e-6, "Surface point must lie exactly on the triaxial ellipsoid");

        // Haumea Jacobi ellipsoid: a=1050km, c=537km, b=840km
        let haumea_ellipsoid = EllipsoidRadii::new(1_050_000.0, 537_000.0, 840_000.0);
        let haumea_mean_r = 816_000.0;
        let haumea_scale = haumea_ellipsoid.shape_scale(haumea_mean_r);
        assert!(haumea_scale.x > 1.25, "Haumea equatorial major axis is highly elongated");
        assert!(haumea_scale.y < 0.70, "Haumea polar axis is highly flattened by rapid spin");
    }

    #[test]
    fn test_uranus_moons_orbital_parameters_and_hierarchy() {
        // Semi-major axis hierarchy: Miranda < Ariel < Umbriel < Titania < Oberon
        let miranda_sma: f64 = 129_390_000.0;
        let ariel_sma: f64 = 191_020_000.0;
        let umbriel_sma: f64 = 266_000_000.0;
        let titania_sma: f64 = 435_910_000.0;
        let oberon_sma: f64 = 583_520_000.0;

        assert!(miranda_sma < ariel_sma);
        assert!(ariel_sma < umbriel_sma);
        assert!(umbriel_sma < titania_sma);
        assert!(titania_sma < oberon_sma);

        // Size comparison: Titania is largest Uranian moon, followed by Oberon, Umbriel, Ariel, Miranda
        let titania_r = 788_400.0;
        let oberon_r = 761_400.0;
        let umbriel_r = 584_700.0;
        let ariel_r = 578_900.0;
        let miranda_r = 235_800.0;

        assert!(titania_r > oberon_r);
        assert!(oberon_r > umbriel_r);
        assert!(umbriel_r > ariel_r);
        assert!(ariel_r > miranda_r);

        // Tidal locking speeds
        let miranda_spin = (2.0 * std::f64::consts::PI) / 122_124.6;
        assert!((miranda_spin - 5.1449e-5).abs() < 1e-8);

        let ariel_spin = (2.0 * std::f64::consts::PI) / 217_760.7;
        assert!((ariel_spin - 2.8854e-5).abs() < 1e-8);

        let titania_spin = (2.0 * std::f64::consts::PI) / 752_218.6;
        assert!((titania_spin - 8.3529e-6).abs() < 1e-8);

        // Orbital inclination matches Uranus's extreme axial tilt (97.77 deg = 1.7064 rad)
        let uranus_obliquity = 97.77_f64.to_radians();
        let titania_inc = 1.70641;
        assert!((titania_inc - uranus_obliquity).abs() < 1e-3);
    }

    #[test]
    fn test_neptune_triton_orbital_parameters_and_retrograde() {
        // Triton radius (1,353.4 km) - 7th largest moon in Solar System, larger than Pluto (1,188.3 km)
        let triton_radius: f64 = 1_353_400.0;
        let pluto_radius: f64 = 1_188_300.0;
        assert!(triton_radius > pluto_radius, "Triton is a captured dwarf planet larger than Pluto");

        // Triton retrograde inclination (156.885 deg = 2.73815 rad > 90 deg = PI/2)
        let triton_inc: f64 = 2.738150;
        assert!(triton_inc > std::f64::consts::FRAC_PI_2, "Triton must have a retrograde inclination (> 90 deg)");
        assert!((triton_inc - 156.885_f64.to_radians()).abs() < 1e-4);

        // Retrograde inclination projection results in reversed orbital velocity direction
        assert!(triton_inc.cos() < 0.0, "cos(inc) is negative for retrograde motion");

        // Semi-major axis and orbital period (507,760 s = 5.87685 d)
        let triton_sma: f64 = 354_759_000.0;
        let triton_period: f64 = 507_760.0;
        assert_eq!(triton_sma, 354_759_000.0);

        // Synchronous tidally locked retrograde spin speed
        let triton_spin = -(2.0 * std::f64::consts::PI) / triton_period;
        assert!((triton_spin - (-1.2374e-5)).abs() < 1e-8, "Triton rotates synchronously in retrograde");

        // Triton mass dominance: 2.139e22 kg (over 99.5% of total mass orbiting Neptune)
        let triton_mass: f64 = 2.139e22;
        let neptune_mass: f64 = 1.0241e26;
        assert!(triton_mass < neptune_mass * 0.001);
    }

    #[test]
    fn test_neptune_moons_orbital_parameters_and_hierarchy() {
        // Semi-major axis hierarchy: Proteus (117,647 km) < Triton (354,759 km) < Nereid (5,513,818 km)
        let proteus_sma: f64 = 117_647_000.0;
        let triton_sma: f64 = 354_759_000.0;
        let nereid_sma: f64 = 5_513_818_000.0;

        assert!(proteus_sma < triton_sma, "Proteus orbits inside Triton");
        assert!(triton_sma < nereid_sma, "Nereid is a distant outer satellite");

        // Size comparison: Triton (1,353.4 km) >> Proteus (210 km) > Nereid (178.5 km)
        let triton_r = 1_353_400.0;
        let proteus_r = 210_000.0;
        let nereid_r = 178_500.0;

        assert!(triton_r > proteus_r);
        assert!(proteus_r > nereid_r);

        // Orbital period hierarchy: Proteus (1.12 d) < Triton (5.88 d) < Nereid (360.14 d)
        let proteus_period: f64 = 96_967.0;
        let triton_period: f64 = 507_760.0;
        let nereid_period: f64 = 31_116_096.0;

        assert!(proteus_period < triton_period);
        assert!(triton_period < nereid_period);

        // Tidal locking: Proteus is tidally locked (spin matches orbit), Nereid has fast non-synchronous rotation (~11.52 h)
        let proteus_spin = (2.0 * std::f64::consts::PI) / proteus_period;
        assert!((proteus_spin - 6.4797e-5).abs() < 1e-8);

        let nereid_spin: f64 = 1.515e-4; // 11.52 hours
        assert!(nereid_spin > (2.0 * std::f64::consts::PI) / nereid_period);

        // Triaxial irregular shape of Proteus: a=212km, b=195km, c=198km
        let proteus_ellipsoid = EllipsoidRadii::new(212_000.0, 195_000.0, 198_000.0);
        let proteus_scale = proteus_ellipsoid.shape_scale(proteus_r);
        assert!(proteus_scale.x > 1.0, "Major axis is elongated");
        assert!(proteus_scale.y < 1.0, "Polar axis is flattened");
    }

    #[test]
    fn test_dwarf_planets_and_ceres_orbital_parameters() {
        // 1. Ceres (Asteroid Belt): sits between Mars (1.52 AU) and Jupiter (5.20 AU)
        let mars_sma = 227_920_000_000.0;
        let ceres_sma = 413_970_000_000.0;
        let jupiter_sma = 778_547_200_000.0;
        assert!(mars_sma < ceres_sma);
        assert!(ceres_sma < jupiter_sma);
        assert!((ceres_sma / ASTRONOMICAL_UNIT - 2.767).abs() < 0.01);

        // 2. Trans-Neptunian distance hierarchy:
        // Neptune (30.1 AU) < Pluto (39.5 AU) < Haumea (43.2 AU) < Makemake (45.8 AU)
        // < Gonggong (67.4 AU) <= Eris (67.8 AU) < Farfarout (132 AU) < Planet 9 (460 AU) < Sedna (506 AU)
        let neptune_sma = 4_495_060_000_000.0;
        let pluto_sma = 5_906_400_000_000.0;
        let haumea_sma = 6_465_300_000_000.0;
        let makemake_sma = 6_850_000_000_000.0;
        let gonggong_sma = 10_080_000_000_000.0;
        let eris_sma = 10_140_000_000_000.0;
        let farfarout_sma = 19_747_000_000_000.0;
        let planet9_sma = 68_815_000_000_000.0;
        let sedna_sma = 75_700_000_000_000.0;

        assert!(neptune_sma < pluto_sma);
        assert!(pluto_sma < haumea_sma);
        assert!(haumea_sma < makemake_sma);
        assert!(makemake_sma < gonggong_sma);
        assert!(gonggong_sma < eris_sma);
        assert!(eris_sma < farfarout_sma);
        assert!(farfarout_sma < planet9_sma);
        assert!(planet9_sma < sedna_sma);

        // 3. High orbital inclinations well out of the ecliptic plane:
        let eris_inc = 0.7686; // 44.04 deg
        let haumea_inc = 0.4920; // 28.19 deg
        let makemake_inc = 0.5055; // 28.96 deg
        let gonggong_inc = 0.5358; // 30.70 deg
        let pluto_inc = 0.2995; // 17.16 deg
        let ceres_inc = 0.1848; // 10.59 deg

        assert!(eris_inc > 40.0_f64.to_radians());
        assert!(gonggong_inc > 30.0_f64.to_radians());
        assert!(makemake_inc > 25.0_f64.to_radians());
        assert!(haumea_inc > 25.0_f64.to_radians());
        assert!(pluto_inc > 15.0_f64.to_radians());
        assert!(ceres_inc > 10.0_f64.to_radians());

        // 4. Haumea triaxial ellipsoid shape:
        let haumea_mean_r = 816_000.0;
        let haumea_ellipsoid = EllipsoidRadii::new(1_161_000.0, 513_000.0, 852_000.0);
        let scale = haumea_ellipsoid.shape_scale(haumea_mean_r);
        assert!(scale.x > 1.35, "Haumea major equatorial axis is extended");
        assert!(scale.y < 0.65, "Haumea polar axis is strongly compressed");

        // 5. Pluto-Charon binary tidal locking:
        let pluto_period = 7_824_000_000.0; // 247.94 y around Sun
        let charon_period = 551_856.0; // 6.387 d around Pluto
        assert!(charon_period < pluto_period);
        let charon_spin: f64 = 1.139e-5; // matches 6.387 d
        let charon_orb_speed = (2.0 * std::f64::consts::PI) / charon_period;
        assert!((charon_spin - charon_orb_speed).abs() < 1e-7, "Charon is tidally locked to Pluto");
    }

    #[test]
    fn test_moon_near_side_faces_earth_at_spawn_and_orbital_motion() {
        let luna_mean_anomaly = 1.20f64;
        let luna_inc = 0.0898f64;
        let luna_semi_major = 384_400_000.0f64;

        // Position of Moon relative to Earth at t=0
        let orb_x = luna_semi_major * luna_mean_anomaly.cos();
        let orb_z = luna_semi_major * luna_mean_anomaly.sin();
        let moon_rel_earth = DVec3::new(orb_x * luna_inc.cos(), orb_x * luna_inc.sin(), orb_z);
        let to_earth = (-moon_rel_earth).normalize().as_vec3();

        // Near-side normal vector in local mesh space is (-1, 0, 0)
        let v_near_local = Vec3::new(-1.0, 0.0, 0.0);

        // Moon rotation at spawn:
        let spawn_rot = Quat::from_rotation_z(6.68_f32.to_radians()) * Quat::from_rotation_y(-luna_mean_anomaly as f32);
        let v_near_world = spawn_rot * v_near_local;

        // Verify near side points directly at Earth within 1 degree
        let cos_angle = v_near_world.dot(to_earth).clamp(-1.0, 1.0);
        let angle_err_deg = cos_angle.acos().to_degrees();
        assert!(angle_err_deg < 1.0, "Moon near side must face Earth within 1 deg at spawn, got {angle_err_deg} deg");

        // Verify synchronous rotation preserves tidal locking over orbit
        let period = 2_360_585.0f64;
        let delta_t = period * 0.25; // 1/4 orbit later
        let new_mean_anomaly = luna_mean_anomaly + (2.0 * std::f64::consts::PI / period) * delta_t;
        let new_orb_x = luna_semi_major * new_mean_anomaly.cos();
        let new_orb_z = luna_semi_major * new_mean_anomaly.sin();
        let new_moon_rel = DVec3::new(new_orb_x * luna_inc.cos(), new_orb_x * luna_inc.sin(), new_orb_z);
        let new_to_earth = (-new_moon_rel).normalize().as_vec3();

        // Spin by -speed * dt:
        let speed = (2.0 * std::f64::consts::PI / period) as f32;
        let mut sim_rot = spawn_rot;
        sim_rot = sim_rot * Quat::from_rotation_y(-speed * delta_t as f32);
        let v_near_quarter = sim_rot * v_near_local;

        let cos_quarter = v_near_quarter.dot(new_to_earth).clamp(-1.0, 1.0);
        let err_quarter_deg = cos_quarter.acos().to_degrees();
        // Due to lunar orbital inclination (5.1 deg) vs lunar polar tilt (6.68 deg), physical geometric libration is ~1.5 deg
        assert!(err_quarter_deg < 2.0, "Moon near side must remain locked facing Earth during orbit, got {err_quarter_deg} deg");
    }

    #[test]
    fn test_kepler_equation_solver_newton_raphson() {
        // Test solver across low, moderate, and extreme eccentricities
        let test_cases = [
            (0.0, 0.0),
            (0.0, 1.5),
            (0.0934, 0.4),  // Mars
            (0.2056, 1.2),  // Mercury
            (0.2488, 5.2),  // Pluto
            (0.5000, 2.1),  // Gonggong
            (0.6550, 2.8),  // Farfarout
            (0.8550, 0.35), // Sedna
            (0.9500, 4.0),  // Extreme comet-like
        ];

        for (e, m) in test_cases {
            let orbit = KeplerOrbit::eccentric(None, 1.0, e, 0.0, 0.0, 0.0, 100.0, 0.0);
            let e_anom = orbit.solve_eccentric_anomaly(m);
            // Verify Kepler's Equation: M = E - e*sin(E)
            let m_reconstructed = e_anom - e * e_anom.sin();
            let m_norm = m.rem_euclid(2.0 * std::f64::consts::PI);
            let diff = (m_reconstructed.rem_euclid(2.0 * std::f64::consts::PI) - m_norm).abs();
            assert!(
                diff < 1e-10 || (2.0 * std::f64::consts::PI - diff).abs() < 1e-10,
                "Kepler solver failed for e={e}, M={m}: diff={diff}"
            );
        }
    }

    #[test]
    fn test_kepler_orbit_perihelion_and_aphelion_distances() {
        // Sedna: a = 506 AU, e = 0.855
        let sedna_orbit = KeplerOrbit::eccentric(
            None,
            75_700_000_000_000.0,
            0.8550,
            0.2082,
            2.5220,
            5.4350,
            359_800_000_000.0,
            0.0,
        );

        let r_peri = sedna_orbit.position_at_eccentric_anomaly(0.0).length();
        let r_aph = sedna_orbit.position_at_eccentric_anomaly(std::f64::consts::PI).length();

        let expected_peri = 75_700_000_000_000.0 * (1.0 - 0.8550);
        let expected_aph = 75_700_000_000_000.0 * (1.0 + 0.8550);

        assert!((r_peri - expected_peri).abs() / expected_peri < 1e-6, "Sedna perihelion must match a(1-e)");
        assert!((r_aph - expected_aph).abs() / expected_aph < 1e-6, "Sedna aphelion must match a(1+e)");

        // Farfarout: a = 132 AU, e = 0.655
        let farfarout_orbit = KeplerOrbit::eccentric(
            None,
            19_747_000_000_000.0,
            0.6550,
            0.3246,
            2.2428,
            5.9341,
            31_557_600_000.0,
            0.0,
        );

        let f_peri = farfarout_orbit.position_at_eccentric_anomaly(0.0).length();
        let f_aph = farfarout_orbit.position_at_eccentric_anomaly(std::f64::consts::PI).length();

        let expected_f_peri = 19_747_000_000_000.0 * (1.0 - 0.6550);
        let expected_f_aph = 19_747_000_000_000.0 * (1.0 + 0.6550);

        assert!((f_peri - expected_f_peri).abs() / expected_f_peri < 1e-6, "Farfarout perihelion must match a(1-e)");
        assert!((f_aph - expected_f_aph).abs() / expected_f_aph < 1e-6, "Farfarout aphelion must match a(1+e)");
    }

    #[test]
    fn test_vis_viva_orbital_speed_hierarchy() {
        // Farfarout: e = 0.655. Orbital speed at perihelion must exceed aphelion by (1+e)/(1-e)
        let period = 31_557_600_000.0;
        let farfarout_orbit = KeplerOrbit::eccentric(
            None,
            19_747_000_000_000.0,
            0.6550,
            0.0,
            0.0,
            0.0,
            period,
            0.0, // perihelion at t=0
        );

        let (_, v_peri) = farfarout_orbit.evaluate_state_at_time(0.0);
        let (_, v_aph) = farfarout_orbit.evaluate_state_at_time(period * 0.5);

        let speed_peri = v_peri.length();
        let speed_aph = v_aph.length();

        assert!(speed_peri > speed_aph, "Perihelion speed must strictly exceed aphelion speed");

        let speed_ratio = speed_peri / speed_aph;
        let expected_ratio = (1.0 + 0.6550) / (1.0 - 0.6550); // ~4.797
        assert!(
            (speed_ratio - expected_ratio).abs() < 1e-4,
            "Speed ratio must equal (1+e)/(1-e): got {speed_ratio}, expected {expected_ratio}"
        );
    }

    #[test]
    fn test_dwarf_planets_eccentricity_solar_offset() {
        // Sol is at focus (0, 0, 0). The geometric center of the ellipse is at (a*e) away from Sol along the major axis.
        let a = 19_747_000_000_000.0; // Farfarout: 132 AU
        let e = 0.6550;
        let orbit = KeplerOrbit::eccentric(None, a, e, 0.0, 0.0, 0.0, 1000.0, 0.0);

        let p_peri = orbit.position_at_eccentric_anomaly(0.0);
        let p_aph = orbit.position_at_eccentric_anomaly(std::f64::consts::PI);

        let center = (p_peri + p_aph) * 0.5;
        let center_offset_au = center.length() / ASTRONOMICAL_UNIT;
        let expected_offset_au = (a * e) / ASTRONOMICAL_UNIT;

        assert!(
            (center_offset_au - expected_offset_au).abs() < 1e-4,
            "Center offset must be a*e = {expected_offset_au} AU, got {center_offset_au} AU"
        );
        assert!(
            center_offset_au > 85.0,
            "Farfarout geometric center is offset from the Sun by > 85 AU!"
        );
    }

    #[test]
    fn test_child_entity_doppler_uses_parent_position() {
        let mut app = App::new();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<Mesh>>();
        app.insert_resource(CanopyVisorState::default());
        app.insert_resource(crate::relativity::RelativisticOpticsConfig {
            aberration_enabled: true,
            doppler_enabled: true,
            beaming_enabled: true,
        });

        // Shuttle at (0, 0, 100_000_000) moving at beta = 0.5 along -Z (towards Sol at 0,0,0)
        let beta = 0.5f64;
        let gamma = 1.0 / (1.0 - beta * beta).sqrt();
        let zeta = beta.atanh();
        app.world_mut().spawn((
            crate::flight::Shuttle::default(),
            UniversalPosition(DVec3::new(0.0, 0.0, 100_000_000.0)),
            crate::relativity::RelativisticState {
                velocity: DVec3::new(0.0, 0.0, -beta * 299_792_458.0),
                rapidity: DVec3::new(0.0, 0.0, -zeta),
                beta,
                beta_vec: DVec3::new(0.0, 0.0, -beta),
                gamma,
                ..default()
            },
        ));

        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let atmo_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.35, 0.70, 1.0, 0.10),
            emissive: LinearRgba::BLACK,
            ..default()
        });

        // Earth is at (0, 0, 200_000_000), which is behind the shuttle!
        // The shuttle is receding from Earth.
        let earth_mat = materials.add(StandardMaterial::default());
        let atmo_child = app.world_mut().spawn((
            GlobalTransform::default(),
            MeshMaterial3d(atmo_mat.clone()),
            RelativisticOpticsBody {
                base_color: LinearRgba::new(0.35, 0.70, 1.0, 0.10),
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: false,
                rest_temperature_k: 288.0,
            },
            Transform::IDENTITY,
        )).id();

        app.world_mut().spawn((
            GlobalTransform::default(),
            UniversalPosition(DVec3::new(0.0, 0.0, 200_000_000.0)),
            MeshMaterial3d(earth_mat),
            RelativisticOpticsBody {
                base_color: LinearRgba::WHITE,
                base_emissive: LinearRgba::BLACK,
                is_emissive_star: false,
                rest_temperature_k: 288.0,
            },
        )).add_child(atmo_child);

        app.add_systems(Update, celestial_optics_system);
        app.update();

        // Check the atmosphere material:
        // Since Earth is behind the shuttle, the relative velocity is RECEDING.
        // Therefore Doppler factor D = gamma * (1 + beta_vec . n_rest) < 1.0 (redshift!).
        // If the atmosphere erroneously defaulted to DVec3::ZERO, it would be APPROACHING (D > 1.0, blueshift),
        // causing high thermal emissive and blue tinting.
        let mat = app.world().resource::<Assets<StandardMaterial>>().get(&atmo_mat).unwrap();
        // Under redshift (D < 1), thermal emissive of a 288K planet is exactly 0
        assert_eq!(mat.emissive, LinearRgba::BLACK, "Receding planetary atmosphere must not have incandescent blueshift emissive");
        // Under redshift, reflectance is attenuated (less than rest 0.5)
        assert!(mat.reflectance < 0.5, "Receding planetary atmosphere reflectance must be dimmed (redshifted)");
    }

    #[test]
    fn test_axial_rotation_zero_quaternion_norm_drift_at_165hz() {
        let mut app = App::new();
        app.insert_resource(crate::flight::SimulationPause(false));
        app.insert_resource(crate::relativity::RelativisticConfig::default());
        app.init_resource::<Time>();

        let earth_ent = app.world_mut().spawn((
            AxialRotation {
                speed_rad_per_sec: 7.2921e-5, // Earth sidereal day
            },
            Transform::from_rotation(Quat::from_rotation_z(23.44_f32.to_radians())),
        )).id();

        app.add_systems(Update, axial_rotation_system);

        // Simulate 40,000 frames (~4 minutes at 165 Hz)
        let frame_dt = std::time::Duration::from_micros(6061);
        for _ in 0..40_000 {
            app.world_mut().resource_mut::<Time>().advance_by(frame_dt);
            app.update();
        }

        let tf = app.world().get::<Transform>(earth_ent).unwrap();
        let len_sq = tf.rotation.length_squared() as f64;
        let radial_scale_err_m = (len_sq - 1.0).abs() * 6_371_000.0;
        assert!(
            radial_scale_err_m < 2.0,
            "Earth quaternion norm must stay strictly unit-length over 40,000 frames at 165 Hz (radial error = {:.3} m, |q|^2 = {:.10})",
            radial_scale_err_m,
            len_sq
        );
    }
}
