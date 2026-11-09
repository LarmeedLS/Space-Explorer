use bevy::math::DVec3;
use bevy::prelude::*;

pub const REAL_C: f64 = 299_792_458.0; // True speed of light in vacuum (m/s)
pub const DEMO_C: f64 = 5_000.0;       // Scaled demonstration speed of light (m/s)
pub const GRAVITATIONAL_CONSTANT: f64 = 6.67430e-11; // Universal Gravitational Constant G (m³·kg⁻¹·s⁻²)

#[derive(Resource)]
pub struct RelativisticConfig {
    /// Active speed of light constant (m/s)
    pub c: f64,
    /// Simulation time multiplier (default 1.0)
    pub time_scale: f64,
    /// Whether demonstration mode is active
    pub is_demo_mode: bool,
}

impl Default for RelativisticConfig {
    fn default() -> Self {
        Self {
            c: REAL_C,
            time_scale: 1.0,
            is_demo_mode: false,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct RelativisticOpticsConfig {
    /// Whether relativistic aberration (searchlight cone compression) is enabled
    pub aberration_enabled: bool,
    /// Whether relativistic Doppler shift (blueshift/redshift) is enabled
    pub doppler_enabled: bool,
    /// Whether relativistic beaming (radiance amplification D^3) is enabled
    pub beaming_enabled: bool,
}

impl Default for RelativisticOpticsConfig {
    fn default() -> Self {
        Self {
            aberration_enabled: true,
            doppler_enabled: true,
            beaming_enabled: true,
        }
    }
}

/// Core relativistic state of an entity in spacetime.
#[derive(Component, Clone, Debug)]
pub struct RelativisticState {
    /// Rapidity vector ζ in direction of motion (dimensionless).
    /// Rapidity accumulates linearly under proper acceleration: dζ = (α/c) dτ
    pub rapidity: DVec3,
    /// Physical coordinate velocity v = c * tanh(||ζ||) * (ζ/||ζ||) (m/s)
    pub velocity: DVec3,
    /// Fraction of speed of light: β = ||v|| / c in [0.0, 1.0)
    pub beta: f64,
    /// Directional beta vector: β_vec = v / c
    pub beta_vec: DVec3,
    /// Lorentz factor: γ = 1 / sqrt(1 - β²) = cosh(||ζ||) >= 1.0
    pub gamma: f64,
    /// Ship Proper Time τ (seconds experienced on the cockpit clock)
    pub proper_time: f64,
    /// Universe / Coordinate Time t (seconds elapsed across the static universe)
    pub coordinate_time: f64,
    /// 4-Velocity: u^μ = (γc, γv)
    pub four_velocity: (f64, DVec3),
    /// Space shuttle rest mass (kg, ~50 metric tons)
    pub rest_mass: f64,
    /// Total relativistic energy: E = γ m₀ c² (Joules)
    pub total_energy: f64,
    /// Relativistic kinetic energy: E_k = (γ - 1) m₀ c² (Joules)
    pub kinetic_energy: f64,
    /// Relativistic momentum: p = γ m₀ v (kg·m/s)
    pub momentum: DVec3,
}

impl Default for RelativisticState {
    fn default() -> Self {
        let rest_mass = 50_000.0;
        let c = DEMO_C;
        let e0 = rest_mass * c * c;
        Self {
            rapidity: DVec3::ZERO,
            velocity: DVec3::ZERO,
            beta: 0.0,
            beta_vec: DVec3::ZERO,
            gamma: 1.0,
            proper_time: 0.0,
            coordinate_time: 0.0,
            four_velocity: (c, DVec3::ZERO),
            rest_mass,
            total_energy: e0,
            kinetic_energy: 0.0,
            momentum: DVec3::ZERO,
        }
    }
}

/// Computes a relativistic rapidity vector ζ from an arbitrary 3-velocity vector v:
/// ζ = atanh(||v|| / c) * (v / ||v||)
/// Clamps ||v|| to < c to prevent infinite rapidity.
pub fn rapidity_from_velocity(v: DVec3, c: f64) -> DVec3 {
    let speed = v.length();
    if speed < 1e-12 || c <= 0.0 {
        return DVec3::ZERO;
    }
    let beta = (speed / c).clamp(0.0, 0.999999999999999);
    let rap_mag = beta.atanh();
    (v / speed) * rap_mag
}

impl RelativisticState {
    /// Updates all relativistic quantities from the current rapidity vector and active speed of light c
    pub fn update_from_rapidity(&mut self, c: f64) {
        let zeta = self.rapidity.length();

        if zeta < 1e-14 {
            self.beta = 0.0;
            self.beta_vec = DVec3::ZERO;
            self.velocity = DVec3::ZERO;
            self.gamma = 1.0;
            self.four_velocity = (c, DVec3::ZERO);
            self.total_energy = self.rest_mass * c * c;
            self.kinetic_energy = 0.0;
            self.momentum = DVec3::ZERO;
            return;
        }

        let dir = self.rapidity / zeta;

        // β = tanh(ζ)
        let beta = zeta.tanh();
        self.beta = beta;
        self.beta_vec = dir * beta;

        // v = c * β
        self.velocity = self.beta_vec * c;

        // γ = cosh(ζ) = 1 / sqrt(1 - β²)
        let gamma = zeta.cosh();
        self.gamma = gamma;

        // 4-Velocity: u^μ = (γc, γv)
        let u0 = gamma * c;
        let u_spatial = dir * (c * zeta.sinh());
        self.four_velocity = (u0, u_spatial);

        // Relativistic energy and momentum
        self.total_energy = gamma * self.rest_mass * c * c;
        self.kinetic_energy = (gamma - 1.0) * self.rest_mass * c * c;
        self.momentum = self.velocity * (gamma * self.rest_mass);
    }

    /// Preserves physical velocity (m/s) when c is changed, recalculating rapidity
    pub fn preserve_velocity_on_c_change(&mut self, new_c: f64) {
        let current_speed = self.velocity.length();
        if current_speed < 1e-6 {
            self.rapidity = DVec3::ZERO;
            self.update_from_rapidity(new_c);
            return;
        }

        let dir = self.velocity / current_speed;
        let new_beta = (current_speed / new_c).min(0.99999999);
        let new_zeta = new_beta.atanh();

        self.rapidity = dir * new_zeta;
        self.update_from_rapidity(new_c);
    }
}

/// System to toggle speed of light presets or continuously adjust c while holding O/P.
pub fn relativistic_config_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut config: ResMut<RelativisticConfig>,
    mut optics: ResMut<RelativisticOpticsConfig>,
    mut shuttle_query: Query<&mut RelativisticState>,
) {
    let mut config_changed = false;
    let dt = time.delta_secs_f64();

    // F4 toggles Relativistic Aberration (Searchlight effect cone compression)
    if keyboard.just_pressed(KeyCode::F4) {
        optics.aberration_enabled = !optics.aberration_enabled;
    }

    // F5 toggles Relativistic Doppler Shift & Radiance Beaming
    if keyboard.just_pressed(KeyCode::F5) {
        optics.doppler_enabled = !optics.doppler_enabled;
        optics.beaming_enabled = optics.doppler_enabled;
    }

    // Key '1' (or '&' on AZERTY): Switch to Real Speed of Light
    if keyboard.just_pressed(KeyCode::Digit1) {
        config.c = REAL_C;
        config.is_demo_mode = false;
        config_changed = true;
    }

    // Key '2' (or 'é' on AZERTY): Switch to Demonstration Speed of Light
    if keyboard.just_pressed(KeyCode::Digit2) {
        config.c = DEMO_C;
        config.is_demo_mode = true;
        config_changed = true;
    }

    // CONTINUOUS TUNING: Hold 'O' or Numpad - / Minus to smoothly decrease c
    if keyboard.pressed(KeyCode::KeyO)
        || keyboard.pressed(KeyCode::NumpadSubtract)
        || keyboard.pressed(KeyCode::Minus)
    {
        // Smooth continuous exponential decrease (~40% per second)
        let rate = 1.6_f64;
        config.c = (config.c * (1.0 / rate).powf(dt)).max(100.0);
        config_changed = true;
    }

    // CONTINUOUS TUNING: Hold 'P' or Numpad + / Equal to smoothly increase c
    if keyboard.pressed(KeyCode::KeyP)
        || keyboard.pressed(KeyCode::NumpadAdd)
        || keyboard.pressed(KeyCode::Equal)
    {
        // Smooth continuous exponential increase (~40% per second)
        let rate = 1.6_f64;
        config.c = (config.c * rate.powf(dt)).min(REAL_C);
        config_changed = true;
    }

    // TIME SCALE / BULLET-TIME CONTROLS:
    // F6 or [ / BracketLeft: Slow down simulation (1.0 -> 0.5 -> 0.25 -> 0.10 -> 0.05)
    // F7 or ] / BracketRight: Speed up simulation (0.05 -> 0.10 -> 0.25 -> 0.5 -> 1.0 -> 2.0)
    // F8 or \ / Backslash: Reset time scale to 1.0 (realtime)
    if keyboard.just_pressed(KeyCode::F6) || keyboard.just_pressed(KeyCode::BracketLeft) {
        let scales = [0.05, 0.10, 0.25, 0.50, 1.0, 2.0];
        let current = config.time_scale;
        if let Some(&new_scale) = scales.iter().rev().find(|&&s| s < current - 0.01) {
            config.time_scale = new_scale;
        } else {
            config.time_scale = 0.05;
        }
    }
    if keyboard.just_pressed(KeyCode::F7) || keyboard.just_pressed(KeyCode::BracketRight) {
        let scales = [0.05, 0.10, 0.25, 0.50, 1.0, 2.0];
        let current = config.time_scale;
        if let Some(&new_scale) = scales.iter().find(|&&s| s > current + 0.01) {
            config.time_scale = new_scale;
        } else {
            config.time_scale = 2.0;
        }
    }
    if keyboard.just_pressed(KeyCode::F8) || keyboard.just_pressed(KeyCode::Backslash) {
        config.time_scale = 1.0;
    }

    if config_changed {
        for mut state in shuttle_query.iter_mut() {
            state.preserve_velocity_on_c_change(config.c);
        }
    }
}

/// Calculates the relativistic pseudo-Newtonian (Paczyński–Wiita) gravitational acceleration
/// vector exerted by a celestial mass M at `body_upos` on a body at `shuttle_upos`:
///
/// Outside the body (r >= R):
///   a_grav = (G * M / (r - r_s)^2) * normalize(rel_pos)
///   where r_s = 2 * G * M / c^2 is the Schwarzschild radius (event horizon).
///
/// Inside a normal celestial body (r < R and R > r_s):
///   By Newton's shell theorem, interior mass scales as (r / R)^3, yielding linear
///   force scaling towards the core: a_grav = a_surface * (r / R).
///   This physically prevents any singularity / division-by-zero if a craft penetrates terrain!
///
/// Physical Properties:
/// - When r >> r_s (planets, normal stars): Identical to Newtonian gravity G * M / r^2.
/// - When r -> r_s (neutron stars, black holes): Gravity diverges asymptotically,
///   analytically reproducing the Schwarzschild event horizon, ISCO (3 * r_s), and marginally bound orbit (2 * r_s).
pub fn paczynski_wiita_gravity(
    shuttle_upos: DVec3,
    body_upos: DVec3,
    body_mass: f64,
    body_radius: f64,
    c: f64,
) -> DVec3 {
    let rel_pos = body_upos - shuttle_upos;
    let dist = rel_pos.length();

    if dist < 0.1 || body_mass <= 0.0 {
        return DVec3::ZERO;
    }

    // Schwarzschild radius: r_s = 2 * G * M / c^2
    let r_s = 2.0 * GRAVITATIONAL_CONSTANT * body_mass / (c * c);

    // If dist < body_radius and body_radius > r_s (interior of solid/gaseous planet):
    // Gravity linearly scales with r (Newton's shell theorem): a = a_surface * (dist / body_radius)
    if body_radius > r_s && dist < body_radius {
        let effective_surf_dist = (body_radius - r_s).max(1.0);
        let surf_accel = (GRAVITATIONAL_CONSTANT * body_mass) / (effective_surf_dist * effective_surf_dist);
        let interior_accel = surf_accel * (dist / body_radius);
        let dir = rel_pos / dist;
        return dir * interior_accel;
    }

    // Outside the body (or compact object where body_radius <= r_s):
    let effective_dist = if dist > r_s {
        (dist - r_s).max(r_s * 0.001).max(1.0)
    } else {
        r_s * 0.001
    };

    let accel_mag = (GRAVITATIONAL_CONSTANT * body_mass) / (effective_dist * effective_dist);
    let dir = rel_pos / dist;
    dir * accel_mag
}

/// Integrates a 3-acceleration vector into the shuttle's relativistic rapidity vector over time dt.
///
/// In Special Relativity, momentum is p = m * u = m * gamma * v.
/// The equation of motion is:
///   d(gamma * v) / dt = a_grav
///
/// Given current rapidity zeta, proper velocity is u = c * sinh(||zeta||) * dir.
/// Under acceleration a_grav:
///   u_new = u + a_grav * dt
/// Then new rapidity is:
///   zeta_new = asinh(||u_new|| / c) * (u_new / ||u_new||)
///
/// This mathematically guarantees that coordinate velocity v = c * tanh(zeta) can NEVER exceed c!
pub fn integrate_relativistic_acceleration(rapidity: DVec3, accel: DVec3, dt: f64, c: f64) -> DVec3 {
    if accel.length_squared() < 1e-18 || dt <= 0.0 {
        return rapidity;
    }

    let rap_mag = rapidity.length();
    let current_u = if rap_mag > 1e-12 {
        let dir = rapidity / rap_mag;
        dir * (c * rap_mag.sinh())
    } else {
        DVec3::ZERO
    };

    let new_u = current_u + accel * dt;
    let new_u_mag = new_u.length();

    if new_u_mag > 1e-12 {
        let new_dir = new_u / new_u_mag;
        let new_rap_mag = (new_u_mag / c).asinh();
        new_dir * new_rap_mag
    } else {
        DVec3::ZERO
    }
}

/// Calculates relativistic light aberration for an incoming direction vector.
/// Given an un-aberrated unit direction `dir_to_source` (from observer towards light source),
/// and observer's velocity vector `beta_vec` (where beta = ||v||/c < 1.0),
/// returns the observed unit direction vector in the moving frame.
#[allow(dead_code)]
pub fn aberrate_direction(dir_to_source: Vec3, beta_vec: Vec3) -> Vec3 {
    let beta_sq = beta_vec.length_squared() as f64;
    if beta_sq < 1e-10 {
        return dir_to_source;
    }
    let beta = beta_sq.sqrt().min(0.999999);
    let dir = dir_to_source.as_dvec3().normalize();
    let beta_hat = beta_vec.as_dvec3() / beta;
    let gamma = 1.0 / (1.0 - beta * beta).sqrt();

    let cos_theta = dir.dot(beta_hat).clamp(-1.0, 1.0);
    let n_par = beta_hat * cos_theta;
    let n_perp = dir - n_par;

    // Standard relativistic aberration formula with 64-bit numerical stability:
    let denom = (1.0 + beta * cos_theta).max(1e-12);
    let aberrated = (n_par + beta_hat * beta + n_perp * (1.0 / gamma)) / denom;
    aberrated.normalize().as_vec3()
}

/// Analytical relativistic aberration using rapidity half-angle formulation:
/// tan(theta'/2) = exp(-||zeta||) * tan(theta/2)
/// 100% continuous, zero singularities, zero float quantization jumps.
pub fn aberrate_direction_from_rapidity(dir_to_source: DVec3, rapidity: DVec3) -> DVec3 {
    let zeta = rapidity.length();
    if zeta < 1e-12 {
        return dir_to_source.normalize();
    }

    let beta_hat = rapidity / zeta;
    let dir = dir_to_source.normalize();

    let cos_theta = dir.dot(beta_hat).clamp(-1.0, 1.0);
    let theta = cos_theta.acos();

    if theta < 1e-10 {
        return beta_hat;
    }
    if theta > std::f64::consts::PI - 1e-10 {
        return -beta_hat;
    }

    // Exact relativistic half-angle formula
    let half_theta = theta * 0.5;
    let exp_minus_zeta = (-zeta).exp();
    let tan_half_prime = exp_minus_zeta * half_theta.tan();
    let theta_prime = 2.0 * tan_half_prime.atan();

    let n_perp = dir - beta_hat * cos_theta;
    let len_perp = n_perp.length();
    let u_perp = if len_perp > 1e-12 {
        n_perp / len_perp
    } else {
        DVec3::ZERO
    };

    (beta_hat * theta_prime.cos() + u_perp * theta_prime.sin()).normalize()
}

/// Computes the exact apparent visual radius of a spherical celestial body under relativistic aberration:
/// In Special Relativity, Penrose (1959) proved that the apparent boundary of a moving sphere
/// remains an exact circle under any Lorentz transformation.
/// 
/// The rest angular half-angle of the sphere is: sin(alpha) = R / d.
/// Under relativistic aberration, the angular boundary rays transform via:
///     tan(alpha' / 2) = M * tan(alpha / 2)
/// where M = 1 / [gamma * (1 + beta_vec . dir)].
/// - Approaching (beta_vec . dir > 0): M = exp(-zeta) < 1, shrinking the visual radius
///   in exact proportion to the orbital aberration cone compression (preventing planet collisions).
/// - Receding (beta_vec . dir < 0): M = exp(+zeta) > 1, expanding the apparent angular radius
///   across the sky so the star takes up the visual field in deep-red Doppler shift.
/// 
/// The resulting visual radius is: R_visual = d * sin(alpha').
/// For camera safety, alpha' is capped at 85 degrees (sin <= 0.996), guaranteeing that the
/// sphere fills the entire screen without its 3D mesh ever crossing or engulfing the camera.
pub fn penrose_apparent_angular_radius(
    physical_radius: f64,
    distance: f64,
    dir_to_source: DVec3,
    beta_vec: DVec3,
    gamma: f64,
) -> f64 {
    if distance <= physical_radius || distance < 1.0 {
        return physical_radius;
    }

    let beta_sq = beta_vec.length_squared();
    let sin_alpha = (physical_radius / distance).clamp(0.0, 1.0);
    let cos_alpha = (1.0 - sin_alpha * sin_alpha).sqrt();
    let tan_half_alpha = sin_alpha / (1.0 + cos_alpha);

    if beta_sq < 1e-12 {
        return physical_radius;
    }

    let dir = dir_to_source.normalize_or_zero();
    let beta_dot_dir = beta_vec.dot(dir);
    let denom = gamma * (1.0 + beta_dot_dir);
    if denom <= 1e-12 {
        return physical_radius;
    }

    let m = 1.0 / denom;
    let tan_half_prime = m * tan_half_alpha;
    let alpha_prime = 2.0 * tan_half_prime.atan();

    let clamped_alpha_prime = alpha_prime.clamp(0.0001, std::f64::consts::FRAC_PI_2);
    let sin_alpha_prime = clamped_alpha_prime.sin();

    let visual_radius = distance * sin_alpha_prime;

    if m <= 1.0 {
        // Approaching/transverse: contract visual radius smoothly to match aberration cone
        // and eliminate visual collisions between orbiting planets and the central star.
        visual_radius.clamp(physical_radius * 0.005, physical_radius)
    } else {
        // Receding (aft): soft-knee relativistic optical expansion up to 3.5x physical size.
        // Uses hyperbolic tangent asymptotic saturation so that expansion remains smooth (C^inf)
        // without any sudden stalling or freezing, while preserving a safe 20km+ orbital clearance
        // to Terra and never crossing the camera plane.
        let delta_r = (visual_radius - physical_radius).max(0.0);
        let max_expansion_headroom = physical_radius * 2.5; // Up to 3.5x total radius
        let smooth_expansion = max_expansion_headroom * (delta_r / max_expansion_headroom).tanh();
        let expanded_radius = physical_radius + smooth_expansion;

        // Camera clearance: ensure sphere front surface remains at least 10% of distance away
        let camera_clearance_limit = distance * 0.90;
        expanded_radius.clamp(physical_radius, camera_clearance_limit.max(physical_radius))
    }
}

/// Calculates the relativistic Doppler shift factor D = nu' / nu.
/// D > 1.0 indicates blueshift (higher frequency, shorter wavelength).
/// D < 1.0 indicates redshift (lower frequency, longer wavelength).
pub fn relativistic_doppler_factor(dir_to_source: Vec3, beta_vec: Vec3, gamma: f32) -> f32 {
    let beta_sq = beta_vec.length_squared();
    if beta_sq < 1e-8 {
        return 1.0;
    }
    // D = gamma * (1 + beta_vec . dir_to_source)
    let cos_theta = dir_to_source.dot(beta_vec);
    let factor = gamma * (1.0 + cos_theta);
    factor.max(0.001)
}

/// Computes the Doppler-shifted color for an input RGB color based on the Doppler factor D.
/// Ahead (D > 1.0): Blueshift towards cyan, violet, and high-energy white.
/// Behind (D < 1.0): Physical Redshift into Infrared (The Aft Shadow):
/// 1. D in [0.70, 1.00]: Blue extinguishes, gold/amber transition.
/// 2. D in [0.35, 0.70]: Green extinguishes, shifts to deep scarlet and ruby red.
/// 3. D in [0.15, 0.35]: Red shifts to dark wine / bordeaux as peak radiation leaves visible band.
/// 4. D in [0.03, 0.15]: Fades to dark chocolate brown / faint ember.
/// 5. D < 0.03: Complete infrared extinction into pitch black (invisible).
#[allow(dead_code)]
pub fn doppler_shift_color(color: LinearRgba, doppler: f32) -> LinearRgba {
    if (doppler - 1.0).abs() < 0.005 {
        return color;
    }

    if doppler > 1.0 {
        // Blueshift: red channel attenuates, blue and cyan intensify proportionally
        let t = ((doppler - 1.0) * 0.5).clamp(0.0, 1.0);
        let base_intensity = color.red.max(color.green).max(color.blue);
        let r = color.red * (1.0 - 0.75 * t);
        let g = color.green * (1.0 - 0.25 * t) + base_intensity * 0.3 * t;
        let b = color.blue * (1.0 + 0.8 * t) + base_intensity * 0.5 * t;
        LinearRgba::new(r, g, b, color.alpha)
    } else {
        let d = doppler.clamp(0.0, 1.0);

        // Blue extinguishes monotonically by D = 0.65
        let b = if d > 0.65 {
            color.blue * ((d - 0.65) / 0.35).powi(2)
        } else {
            0.0
        };

        // Green extinguishes monotonically all the way down to D = 0.03
        // Strictly monotonic cubic decay: guarantees that green NEVER increases as redshift deepens,
        // eliminating any discontinuous orange flare-ups.
        let green_factor = if d > 0.03 {
            ((d - 0.03) / 0.97).powf(3.0)
        } else {
            0.0
        };
        let g = color.green * green_factor;

        // Red shifts smoothly:
        // D in [0.40, 1.00]: Red stays at color.red (pure un-amplified red)
        // D in [0.01, 0.40]: Red decays smoothly from 1.0 down to 0.0
        // D <= 0.01: Total infrared extinction into pitch black
        let r = if d > 0.40 {
            color.red
        } else if d > 0.01 {
            let t = (d - 0.01) / 0.39;
            color.red * (t * t * (3.0 - 2.0 * t))
        } else {
            0.0
        };

        LinearRgba::new(r, g, b, color.alpha)
    }
}

/// Computes the relativistic beaming radiance amplification factor I' / I = D^3.
/// Extinguishes smoothly to 0.0 in the aft shadow (infrared extinction).
pub fn relativistic_beaming_factor(doppler: f32) -> f32 {
    let d = doppler.max(0.0);
    (d * d * d).clamp(0.0, 180.0)
}

/// Converts an astrophysical blackbody temperature T (in Kelvin) to a physical LinearRgba color.
///
/// Implements Wien's displacement law (lambda_peak = b / T) and Planckian locus spectral integration:
/// - T <= 300 K: Deep infrared, completely invisible to the human eye (LinearRgba::BLACK).
/// - 300 K < T < 1,800 K: Continuous Wien extinction into the infrared without abrupt cliff edges.
/// - 1,800 K <= T <= 6,600 K: Warm incandescent Planckian locus (deep ruby red -> orange -> solar yellow).
/// - 6,600 K < T <= 40,000 K: Crisp white -> diamond blue-white -> electric cyan (A, B, O spectral classes).
/// - T > 40,000 K: Asymptotic Rayleigh-Jeans limit in the visible spectrum (intense violet-blue).
pub fn blackbody_temperature_to_linear_rgb(temp_kelvin: f32) -> LinearRgba {
    if temp_kelvin <= 300.0 {
        return LinearRgba::new(0.0, 0.0, 0.0, 1.0);
    }

    let t = (temp_kelvin / 100.0).clamp(3.0, 1000.0);

    // Calculate non-linear sRGB chromaticity based on analytical Planckian locus polynomial fits
    let r_srgb = if t <= 66.0 {
        1.0
    } else {
        (329.698727446 * (t - 60.0).powf(-0.1332047592) / 255.0).clamp(0.0, 1.0)
    };

    let g_srgb = if t <= 66.0 {
        ((99.4708025861 * t.ln() - 161.1195681661) / 255.0).clamp(0.0, 1.0)
    } else {
        ((288.1221695283 * (t - 60.0).powf(-0.0755148492)) / 255.0).clamp(0.0, 1.0)
    };

    let b_srgb = if t <= 19.0 {
        0.0
    } else if t < 66.0 {
        ((138.5177312231 * (t - 10.0).ln() - 305.0447927307) / 255.0).clamp(0.0, 1.0)
    } else {
        1.0
    };

    // Smoothstep transition below 1,800 K to model continuous Wien extinction into the infrared
    let infrared_fade = if temp_kelvin < 1800.0 {
        let x = (temp_kelvin / 1800.0).clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    } else {
        1.0
    };

    // Convert from sRGB non-linear curve to Linear sRGB for physically based HDR shading
    let to_linear = |c: f32| -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };

    let r_lin = to_linear(r_srgb) * infrared_fade;
    let g_lin = to_linear(g_srgb) * infrared_fade;
    let b_lin = to_linear(b_srgb) * infrared_fade;

    LinearRgba::new(r_lin, g_lin, b_lin, 1.0)
}

/// Computes the Doppler-shifted blackbody color and relativistic radiance amplification
/// for a primary star or blackbody emitter of resting temperature T_0 (e.g. Sol or background stars).
///
/// Chromatic adaptation: In the observer's rest frame (doppler = 1.0), the star's rest temperature
/// establishes the observer's white reference point, returning pure brilliant white (1.0, 1.0, 1.0).
/// When blueshifted (doppler > 1.0), it shifts smoothly into diamond blue-white and electric sapphire blue.
/// When redshifted (doppler < 1.0), it shifts smoothly into golden, orange, deep ruby red, and smoothly fades to black.
pub fn blackbody_star_doppler(rest_temp_kelvin: f32, doppler: f32) -> (LinearRgba, f32) {
    let d = doppler.max(0.0);
    let shifted_temp = d * rest_temp_kelvin;

    // Relativistic beaming factor I' / I = D^3
    let beaming = d * d * d;

    // Continuous infrared extinction: as temperature drops into the infrared (< 1,800 K),
    // visible radiance smoothly decays to zero without any sudden cliff edges.
    let infrared_fade = if shifted_temp < 1800.0 {
        let x = (shifted_temp / 1800.0).clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    } else {
        1.0
    };

    let base_color = blackbody_temperature_to_linear_rgb(rest_temp_kelvin);
    let shifted_color = blackbody_temperature_to_linear_rgb(shifted_temp.max(300.0));

    let r_ratio = if base_color.red > 1e-4 {
        shifted_color.red / base_color.red
    } else {
        1.0
    };
    let g_ratio = if base_color.green > 1e-4 {
        shifted_color.green / base_color.green
    } else {
        1.0
    };
    let b_ratio = if base_color.blue > 1e-4 {
        shifted_color.blue / base_color.blue
    } else {
        1.0
    };

    let peak = r_ratio.max(g_ratio).max(b_ratio).max(1e-4);
    let normalized_color = LinearRgba::new(
        (r_ratio / peak) * infrared_fade,
        (g_ratio / peak) * infrared_fade,
        (b_ratio / peak) * infrared_fade,
        1.0,
    );

    (normalized_color, beaming.min(180.0))
}

/// Computes the continuous spectral color modulation, radiance amplification, and reflectance scale
/// for sunlight (T_sol = 5,778 K) reflected off a planetary body or ring.
///
/// Returns:
/// 1. `color_tint`: LinearRgba multiplier for the planet's texture/base_color.
/// 2. `dim_or_bright_factor`: Radiance beaming factor for the diffuse & specular reflection.
/// 3. `reflectance_scale`: Factor in [0.0, 1.0] to scale material reflectance (eliminates specular when redshifted).
pub fn blackbody_reflected_sunlight_shift(doppler: f32) -> (LinearRgba, f32, f32) {
    const T_SOL: f32 = 5778.0;
    let base_sun_color = blackbody_temperature_to_linear_rgb(T_SOL);

    let shifted_temp = (doppler * T_SOL).max(0.0);
    let shifted_color = blackbody_temperature_to_linear_rgb(shifted_temp);

    // Compute relative chromaticity shift compared to rest-frame sunlight
    let r_ratio = if base_sun_color.red > 1e-4 {
        shifted_color.red / base_sun_color.red
    } else {
        1.0
    };
    let g_ratio = if base_sun_color.green > 1e-4 {
        shifted_color.green / base_sun_color.green
    } else {
        1.0
    };
    let b_ratio = if base_sun_color.blue > 1e-4 {
        shifted_color.blue / base_sun_color.blue
    } else {
        1.0
    };

    let beaming = relativistic_beaming_factor(doppler);

    if doppler < 1.0 {
        // Redshift into Infrared (The Aft Darkness):
        // 1. Color shifts from solar yellow -> orange -> crimson red -> burgundy wine.
        // 2. Total visible radiance diminishes monotonically as photons shift into the infrared.
        let visible_fraction = (shifted_color.red + shifted_color.green + shifted_color.blue)
            / (base_sun_color.red + base_sun_color.green + base_sun_color.blue);
        let dim_factor = (beaming * visible_fraction).clamp(0.0, 1.0);

        let color_tint = LinearRgba::new(
            r_ratio * dim_factor,
            g_ratio * dim_factor,
            b_ratio * dim_factor,
            1.0,
        );
        // Specular reflectance scales down to zero with dim_factor:
        // When redshifting, the reflection of Sol completely extinguishes!
        (color_tint, dim_factor, dim_factor)
    } else {
        // Blueshift (Approaching planet):
        // 1. Color shifts to crisp white -> brilliant cyan/blue.
        // 2. Beaming amplifies illuminated radiance, making the planet's sunlit side luminous!
        let bright_factor = beaming.clamp(1.0, 6.0);
        let color_tint = LinearRgba::new(
            r_ratio * bright_factor,
            g_ratio * bright_factor,
            b_ratio * bright_factor,
            1.0,
        );
        (color_tint, bright_factor, 1.0)
    }
}

/// Computes the blueshifted intrinsic thermal blackbody emission for a planetary body (e.g. Venus, Jupiter, Earth)
/// as observed by a relativistic traveler approaching at high speeds.
///
/// Physical Draper Point & Incandescence Progression:
/// - In the rest frame, cold planets (100 K - 737 K) emit exclusively in the far infrared, appearing pitch black in the visible spectrum.
/// - Below T_eff = 800 K: Complete visible darkness (0.0). Venus (737 K) and Mercury (440 K) remain dark at sub-relativistic speeds.
/// - 800 K <= T_eff < 1,400 K: Deep infrared photons leak into the visible spectrum as a very faint, dim brown ember (like an L/T-type brown dwarf).
/// - 1,400 K <= T_eff < 2,200 K: Deep crimson red ember.
/// - 2,200 K <= T_eff < 3,200 K: Incandescent glowing scarlet / ruby red.
/// - 3,200 K <= T_eff < 4,500 K: Fiery incandescent orange (M/K-type).
/// - 4,500 K <= T_eff < 6,500 K: Radiant golden solar yellow to white (G-type).
/// - T_eff >= 6,500 K: Dazzling diamond white and electric sapphire blue (A/B/O-type).
pub fn planet_blueshifted_thermal_emission(rest_temp_k: f32, doppler: f32) -> LinearRgba {
    let t_eff = doppler * rest_temp_k;
    if t_eff < 800.0 {
        return LinearRgba::new(0.0, 0.0, 0.0, 1.0);
    }

    // Smooth cubic fade-in above the 800 K incandescence threshold
    // Prevents any step discontinuities or popping
    let u = ((t_eff - 800.0) / 2200.0).clamp(0.0, 1.0);
    let intensity_fade = u * u * (3.0 - 2.0 * u);

    // Relativistic beaming boost: as the observer approaches near c,
    // total flux scales with D^4, transforming cold dark worlds into incandescent glowing stellar bodies!
    // Smoothly scale beaming up to 40.0 for extreme ultra-relativistic encounters.
    let beaming_mult = if doppler > 8.0 {
        4.0 + ((doppler - 8.0) * 1.5).min(36.0)
    } else {
        (doppler / 2.0).clamp(0.2, 4.0)
    };
    let total_intensity = intensity_fade * beaming_mult;

    // Chromatic progression from dim brown dwarf to blue-hot incandescence
    let color = if t_eff < 1400.0 {
        // Very dim brown dwarf ember (burnt mahogany / warm chocolate umber)
        let t = (t_eff - 800.0) / 600.0;
        LinearRgba::new(
            0.40 + 0.45 * t,
            0.16 + 0.02 * t,
            0.04 - 0.01 * t,
            1.0,
        )
    } else if t_eff < 2200.0 {
        // Deep crimson red ember -> glowing scarlet
        let t = (t_eff - 1400.0) / 800.0;
        LinearRgba::new(
            0.85 + 0.15 * t,
            0.18 + 0.17 * t,
            0.03 + 0.02 * t,
            1.0,
        )
    } else {
        // Full Planckian incandescence (orange -> yellow -> white -> sapphire blue)
        blackbody_temperature_to_linear_rgb(t_eff)
    };

    LinearRgba::new(
        color.red * total_intensity,
        color.green * total_intensity,
        color.blue * total_intensity,
        1.0,
    )
}

/// Computes the forward Cosmic Microwave Background (CMB) dipole temperature and visible color
/// along the flight vector for a given Lorentz factor gamma and velocity ratio beta.
///
/// In the cosmic rest frame, the universe is filled with isotropic 2.7255 K microwave radiation.
/// At extreme relativistic speeds (gamma > 100), the forward Doppler factor D_max = gamma * (1 + beta)
/// shifts the peak wavelength from microwaves (1 mm) into the visible spectrum:
/// - gamma < 110: T'_max <= 600 K (infrared, invisible to human eye).
/// - 110 <= gamma < 250: 600 K < T'_max <= 1,360 K (faint deep crimson/ruby red ember forward halo).
/// - 250 <= gamma < 600: 1,360 K < T'_max <= 3,270 K (warm fiery incandescent orange dawn).
/// - gamma >= 600: T'_max >= 3,270 K (dazzling diamond white and electric violet-cyan "Eye of Light").
pub fn cmb_dipole_forward_optics(gamma: f32, beta: f32) -> (f32, LinearRgba, f32) {
    const T_CMB: f32 = 2.7255;
    if beta < 0.1 || gamma < 1.0 {
        return (T_CMB, LinearRgba::new(0.0, 0.0, 0.0, 1.0), 0.0);
    }
    let d_max = gamma * (1.0 + beta);
    let peak_temp_k = d_max * T_CMB;
    let visible_color = blackbody_temperature_to_linear_rgb(peak_temp_k);

    // Radiant intensity scales with relativistic beaming above the 600 K visible threshold
    let intensity = if peak_temp_k > 600.0 {
        let t = (peak_temp_k - 600.0) / 1000.0;
        t.powf(1.4).clamp(0.0, 20.0)
    } else {
        0.0
    };

    (peak_temp_k, visible_color, intensity)
}

/// Computes the spectral color of the Relativistic Cosmic Background Rainbow Ring
/// (Cosmic Infrared Background & Cosmic Microwave Background).
///
/// In ScienceClic's relativistic visualization ("Que verrait-on à la vitesse de la lumière ?"),
/// the cosmic background radiation shifts into the visible spectrum as a distinct circular HOLLOW RING:
/// - Inside the ring (towards forward apex): radiation is shifted into the ultraviolet (< 380 nm), appearing black.
/// - Inside edge: Electric blue and cyan (380 - 480 nm).
/// - Middle of ring: Vibrant emerald green (490 - 560 nm).
/// - Outer edge: Golden yellow, warm orange, and scarlet red (570 - 700 nm).
/// - Outside the ring (towards aft/flanks): radiation remains in the infrared (> 700 nm), appearing black.
///
/// As speed increases (beta: 0.70 -> 0.80 -> 0.95 -> 0.9988 -> 0.99999):
/// The ring appears wide on the sides (~50°), then slowly crunches forward towards the front (~18°, then ~8°).
#[allow(dead_code)]
pub fn relativistic_background_rainbow(theta_deg: f32, beta: f32, intensity: f32, gamma: f32) -> LinearRgba {
    if intensity <= 0.001 || beta < 0.60 {
        return LinearRgba::new(0.0, 0.0, 0.0, 0.0);
    }

    let (th_ring, delta_th) = relativistic_rainbow_ring_params(beta);
    let r_inner = th_ring - delta_th;
    let r_outer = th_ring + delta_th;

    // Check if within the visible rainbow ring
    if theta_deg < r_inner || theta_deg > r_outer {
        // High-energy focal beam ("Eye of Light") in the extreme forward apex at gamma > 50
        if theta_deg < 2.5 && gamma > 50.0 {
            let core_t = (1.0 - theta_deg / 2.5).clamp(0.0, 1.0);
            let boost = ((gamma - 50.0) / 100.0).clamp(0.0, 1.5) * intensity * core_t;
            return LinearRgba::new(boost, boost, boost * 1.25, core_t);
        }
        return LinearRgba::new(0.0, 0.0, 0.0, 0.0);
    }

    let u = (theta_deg - r_inner) / (r_outer - r_inner);
    relativistic_rainbow_color_at_u(u, intensity)
}

/// Computes the exact spectral color across the radial thickness of the relativistic rainbow ring.
/// u in [0.0, 1.0]:
/// - u = 0.0: Inner boundary of the ring (smooth Hermite fade into dark ultraviolet void)
/// - u ~ 0.15: Electric Royal Blue (440 nm)
/// - u ~ 0.25: Electric Cyan (480 nm)
/// - u ~ 0.45: Vibrant Emerald Green (530 nm)
/// - u ~ 0.70: Golden Solar Yellow & Warm Orange (600 nm)
/// - u ~ 0.90: Deep Scarlet Red (660 nm)
/// - u = 1.0: Outer boundary of the ring (smooth Hermite fade into dark infrared void)
pub fn relativistic_rainbow_color_at_u(u: f32, intensity: f32) -> LinearRgba {
    if intensity <= 0.001 || u < 0.0 || u > 1.0 {
        return LinearRgba::new(0.0, 0.0, 0.0, 0.0);
    }

    let wl = 420.0 + u * 260.0;

    let (r, g, b) = if wl < 440.0 {
        (-(wl - 440.0) / 60.0, 0.0, 1.0)
    } else if wl < 490.0 {
        (0.0, (wl - 440.0) / 50.0, 1.0)
    } else if wl < 510.0 {
        (0.0, 1.0, -(wl - 510.0) / 20.0)
    } else if wl < 580.0 {
        ((wl - 510.0) / 70.0, 1.0, 0.0)
    } else if wl < 645.0 {
        (1.0, -(wl - 645.0) / 65.0, 0.0)
    } else {
        (1.0, 0.0, 0.0)
    };

    // Smooth cubic Hermite fade near inner edge (u < 0.12) and outer edge (u > 0.88)
    let edge_fade = if u < 0.12 {
        let t = u / 0.12;
        t * t * (3.0 - 2.0 * t)
    } else if u > 0.88 {
        let t = (1.0 - u) / 0.12;
        t * t * (3.0 - 2.0 * t)
    } else {
        1.0
    };

    let final_r = r * edge_fade * intensity;
    let final_g = g * edge_fade * intensity;
    let final_b = b * edge_fade * intensity;
    let final_a = edge_fade * intensity;

    LinearRgba::new(final_r, final_g, final_b, final_a)
}

/// Calculates the center angle and half-width of the relativistic rainbow ring in degrees from 64-bit rapidity zeta.
/// As speed approaches c (zeta -> infty), the ring smoothly contracts towards 0° with no artificial size floor
/// and zero float32 quantization jumps, and proportionally thins out into a delicate circular halo.
pub fn relativistic_rainbow_ring_params_from_zeta(zeta: f64) -> (f32, f32) {
    let factor = (-0.48 * (zeta - 1.0).max(0.0)).exp() as f32;
    let th_ring = 52.0 * factor;
    // Ring half-thickness in degrees: shrinks proportionally from ~16 deg down to <1 deg at ultra-relativistic speeds
    let thickness_ratio = 0.22 + 0.10 * factor;
    let delta_th = (th_ring * thickness_ratio).max(0.02);
    (th_ring, delta_th)
}

/// Calculates the center angle and half-width of the relativistic rainbow ring in degrees from beta.
pub fn relativistic_rainbow_ring_params(beta: f32) -> (f32, f32) {
    let beta_clamped = (beta as f64).clamp(0.0, 0.99999999);
    let zeta = beta_clamped.atanh();
    relativistic_rainbow_ring_params_from_zeta(zeta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relativistic_aberration_identity_at_zero_beta() {
        let dir = Vec3::new(0.5, 0.5, 0.7071).normalize();
        let aberrated = aberrate_direction(dir, Vec3::ZERO);
        assert!((aberrated - dir).length() < 1e-5);
    }

    #[test]
    fn test_relativistic_aberration_fixed_points() {
        let beta_vec = Vec3::new(0.0, 0.0, -0.8); // 80% speed of light forward (-Z)
        
        // Forward star stays directly forward
        let forward = Vec3::new(0.0, 0.0, -1.0);
        let aberrated_fwd = aberrate_direction(forward, beta_vec);
        assert!((aberrated_fwd - forward).length() < 1e-4);

        // Aft star stays directly aft
        let aft = Vec3::new(0.0, 0.0, 1.0);
        let aberrated_aft = aberrate_direction(aft, beta_vec);
        assert!((aberrated_aft - aft).length() < 1e-4);
    }

    #[test]
    fn test_relativistic_aberration_right_angle_compression() {
        let beta = 0.8f32;
        let beta_vec = Vec3::new(0.0, 0.0, -beta); // Forward is -Z
        let side_star = Vec3::new(1.0, 0.0, 0.0);   // Star at 90 deg (dot with beta_vec = 0)

        let aberrated = aberrate_direction(side_star, beta_vec);
        // Observed angle must have cos(theta') = beta along forward vector (-Z)
        let fwd_component = aberrated.dot(Vec3::new(0.0, 0.0, -1.0));
        assert!((fwd_component - beta).abs() < 1e-4);
        assert!((aberrated.length() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_relativistic_doppler_blueshift_and_redshift() {
        let beta = 0.8f32;
        let gamma = 1.0 / (1.0 - beta * beta).sqrt(); // 1.6667
        let beta_vec = Vec3::new(0.0, 0.0, -beta);    // Forward is -Z

        // Forward direction: D = sqrt((1 + beta) / (1 - beta)) = sqrt(1.8 / 0.2) = 3.0
        let forward = Vec3::new(0.0, 0.0, -1.0);
        let d_fwd = relativistic_doppler_factor(forward, beta_vec, gamma);
        assert!((d_fwd - 3.0).abs() < 1e-3);

        // Aft direction: D = sqrt((1 - beta) / (1 + beta)) = sqrt(0.2 / 1.8) = 1/3 ~ 0.3333
        let aft = Vec3::new(0.0, 0.0, 1.0);
        let d_aft = relativistic_doppler_factor(aft, beta_vec, gamma);
        assert!((d_aft - 1.0 / 3.0).abs() < 1e-3);
    }

    #[test]
    fn test_doppler_color_shift_and_beaming() {
        let white = LinearRgba::new(1.0, 1.0, 1.0, 1.0);

        // Blueshift: Red decreases, blue increases
        let blue_shifted = doppler_shift_color(white, 2.5);
        assert!(blue_shifted.blue > blue_shifted.red);

        // Moderate Redshift: Blue extinguishes, red dominates
        let red_shifted = doppler_shift_color(white, 0.5);
        assert!(red_shifted.red > red_shifted.blue);
        assert_eq!(red_shifted.blue, 0.0);

        // Dark Bordeaux / Wine: Red intensity declines as radiation leaves visible spectrum
        let bordeaux_shifted = doppler_shift_color(white, 0.20);
        assert!(bordeaux_shifted.red < red_shifted.red);
        assert_eq!(bordeaux_shifted.blue, 0.0);

        // Total Infrared Extinction (Aft Shadow): D < 0.03 completely pitch black
        let extinguished = doppler_shift_color(white, 0.01);
        assert_eq!(extinguished.red, 0.0);
        assert_eq!(extinguished.green, 0.0);
        assert_eq!(extinguished.blue, 0.0);

        // Strict Monotonicity: Green and blue must never increase as redshift deepens (no orange flashes)
        let d_values = [0.80, 0.60, 0.50, 0.40, 0.35, 0.34, 0.30, 0.20, 0.10, 0.05, 0.02];
        let mut prev_g = 1.0f32;
        for &d in &d_values {
            let col = doppler_shift_color(white, d);
            assert!(col.green <= prev_g, "Green must monotonically decrease, at d={d} got g={} > prev={prev_g}", col.green);
            prev_g = col.green;
        }

        // Beaming factor grows cubically, and extinguishes to zero
        assert!((relativistic_beaming_factor(1.0) - 1.0).abs() < 1e-5);
        assert!((relativistic_beaming_factor(2.0) - 8.0).abs() < 1e-5);
        assert_eq!(relativistic_beaming_factor(0.0), 0.0);
    }

    #[test]
    fn test_aberrate_direction_from_rapidity_continuity() {
        // At 80% lightspeed, rapidity zeta = atanh(0.8)
        let beta = 0.8f64;
        let zeta = beta.atanh();
        let rapidity = DVec3::new(0.0, 0.0, -zeta);
        let side_star = DVec3::new(1.0, 0.0, 0.0);

        let aberrated = aberrate_direction_from_rapidity(side_star, rapidity);
        let fwd = aberrated.dot(DVec3::new(0.0, 0.0, -1.0));
        assert!((fwd - beta).abs() < 1e-10);

        // Extreme relativistic boost (zeta = 8.0, gamma = cosh(8) = 1490, beta = 0.9999998)
        let extreme_rapidity = DVec3::new(0.0, 0.0, -8.0);
        let aberrated_extreme = aberrate_direction_from_rapidity(side_star, extreme_rapidity);
        assert!(aberrated_extreme.length().is_finite());
        assert!((aberrated_extreme.length() - 1.0).abs() < 1e-12);
        // Angle must be smoothly compressed towards forward
        assert!(aberrated_extreme.z < -0.999);
    }

    #[test]
    fn test_penrose_apparent_angular_radius_bounds() {
        let physical_radius = 4500.0;
        let distance = 90_000.0;
        let dir = DVec3::new(0.0, 0.0, -1.0);
        let beta = 0.8f64;
        let gamma = 1.0 / (1.0 - beta * beta).sqrt();

        // 1. Approaching (-Z motion towards source at -Z): contracts visual radius
        let beta_fwd = DVec3::new(0.0, 0.0, -beta);
        let r_fwd = penrose_apparent_angular_radius(physical_radius, distance, dir, beta_fwd, gamma);
        assert!(r_fwd < physical_radius);

        // 2. Receding (+Z motion away from source at -Z): expands safely up to 3.5x physical size
        let beta_aft = DVec3::new(0.0, 0.0, beta);
        let r_aft = penrose_apparent_angular_radius(physical_radius, distance, dir, beta_aft, gamma);
        assert!(r_aft > physical_radius);
        assert!(r_aft <= physical_radius * 3.5);
        assert!(r_aft <= distance * 0.90);

        // 3. Zero velocity: exact rest radius
        let r_zero = penrose_apparent_angular_radius(physical_radius, distance, dir, DVec3::ZERO, 1.0);
        assert_eq!(r_zero, physical_radius);
    }

    #[test]
    fn test_blackbody_temperature_to_linear_rgb() {
        // Cold objects (T <= 600 K) are completely in the infrared, returning black
        let cold = blackbody_temperature_to_linear_rgb(300.0);
        assert_eq!(cold.red, 0.0);
        assert_eq!(cold.green, 0.0);
        assert_eq!(cold.blue, 0.0);

        // M-class red dwarf (3,000 K): red dominates, blue is faint
        let m_dwarf = blackbody_temperature_to_linear_rgb(3000.0);
        assert!(m_dwarf.red > m_dwarf.green);
        assert!(m_dwarf.green > m_dwarf.blue);
        assert!(m_dwarf.blue < 0.25);

        // Sol (5,778 K): solar yellow-white
        let sol = blackbody_temperature_to_linear_rgb(5778.0);
        assert_eq!(sol.red, 1.0);
        assert!(sol.green > 0.80);
        assert!(sol.blue > 0.70);

        // A-class star (Vega, 9,500 K): blue exceeds red
        let vega = blackbody_temperature_to_linear_rgb(9500.0);
        assert!(vega.blue > vega.red);

        // O-class supergiant (35,000 K): intense electric blue
        let o_star = blackbody_temperature_to_linear_rgb(35000.0);
        assert_eq!(o_star.blue, 1.0);
        assert!(o_star.blue > o_star.green);
        assert!(o_star.green > o_star.red);
    }

    #[test]
    fn test_blackbody_star_doppler_identity_at_rest_and_color_shifts() {
        // At rest (D = 1.0): must be exactly pure white (1.0, 1.0, 1.0) with beaming 1.0
        let (col_rest, beaming_rest) = blackbody_star_doppler(5778.0, 1.0);
        assert!((col_rest.red - 1.0).abs() < 1e-4);
        assert!((col_rest.green - 1.0).abs() < 1e-4);
        assert!((col_rest.blue - 1.0).abs() < 1e-4);
        assert!((beaming_rest - 1.0).abs() < 1e-4);

        // Blueshift (D = 2.0): diamond/sapphire blue (blue component dominates)
        let (col_blue, beaming_blue) = blackbody_star_doppler(5778.0, 2.0);
        assert_eq!(col_blue.blue, 1.0);
        assert!(col_blue.blue > col_blue.green);
        assert!(col_blue.green > col_blue.red);
        assert!(beaming_blue > 2.0);

        // Redshift (D = 0.5): warm red/orange (red component dominates)
        let (col_red, beaming_red) = blackbody_star_doppler(5778.0, 0.5);
        assert_eq!(col_red.red, 1.0);
        assert!(col_red.red > col_red.green);
        assert!(col_red.green > col_red.blue);
        assert!(beaming_red < 0.5);

        // Extreme redshift (D = 0.104, gamma ≈ 4.85 receding from Sol):
        // Sol radiance must smoothly decay into infrared without cliff-edge step functions
        let (col_gamma_485, beaming_gamma_485) = blackbody_star_doppler(5778.0, 0.104);
        assert!(col_gamma_485.red < 0.35, "At gamma ≈ 4.85, red must be faded into infrared");
        assert!(col_gamma_485.red > 0.15, "At gamma ≈ 4.85, red must still be continuously visible ember");
        assert!(beaming_gamma_485 < 0.002);

        // Near-complete infrared extinction (D = 0.01):
        let (col_dark, beaming_dark) = blackbody_star_doppler(5778.0, 0.01);
        assert!(col_dark.red < 0.001);
        assert!(beaming_dark < 0.0001);
    }

    #[test]
    fn test_blackbody_reflected_sunlight_shift() {
        // At rest (D = 1.0): identity scaling
        let (tint_rest, bright_rest, refl_rest) = blackbody_reflected_sunlight_shift(1.0);
        assert!((tint_rest.red - 1.0).abs() < 0.02);
        assert!((tint_rest.green - 1.0).abs() < 0.02);
        assert!((tint_rest.blue - 1.0).abs() < 0.02);
        assert_eq!(bright_rest, 1.0);
        assert_eq!(refl_rest, 1.0);

        // Extreme Redshift (D = 0.20): Chronos receding at 0.90c
        let (tint_red, dim_red, refl_red) = blackbody_reflected_sunlight_shift(0.20);
        assert!(refl_red < 0.02, "Specular reflectance must extinguish when redshifted, got {}", refl_red);
        assert!(dim_red < 0.02);
        assert!(tint_red.blue < 0.001);

        // Total Infrared Extinction (D = 0.02): completely dark
        let (tint_ext, dim_ext, refl_ext) = blackbody_reflected_sunlight_shift(0.02);
        assert_eq!(refl_ext, 0.0);
        assert_eq!(dim_ext, 0.0);
        assert_eq!(tint_ext.red, 0.0);
        assert_eq!(tint_ext.green, 0.0);
        assert_eq!(tint_ext.blue, 0.0);

        // Blueshift (D = 2.0): approaching planet
        let (tint_blue, bright_blue, refl_blue) = blackbody_reflected_sunlight_shift(2.0);
        assert!(bright_blue >= 2.0, "Approaching planet must brighten up, got {}", bright_blue);
        assert_eq!(refl_blue, 1.0);
        assert!(tint_blue.blue > tint_blue.red);
    }

    #[test]
    fn test_planet_blueshifted_thermal_emission() {
        // 1. Venus (737 K) at rest (D = 1.0) and at 3,600 m/s (D ≈ 1.00001): must be completely dark (T < 800 K)
        let venus_rest = planet_blueshifted_thermal_emission(737.0, 1.0);
        assert_eq!(venus_rest.red, 0.0);
        assert_eq!(venus_rest.green, 0.0);
        assert_eq!(venus_rest.blue, 0.0);

        let venus_low_speed = planet_blueshifted_thermal_emission(737.0, 1.00001);
        assert_eq!(venus_low_speed.red, 0.0);

        // 2. Jupiter (165 K) at gamma = 1.184 (D ≈ 1.818, T_eff ≈ 300 K): must be completely dark
        let jupiter_low_gamma = planet_blueshifted_thermal_emission(165.0, 1.818);
        assert_eq!(jupiter_low_gamma.red, 0.0);

        // 3. Brown dwarf ember phase (T_eff ≈ 1,000 K, e.g. Venus at D = 1.36): dim brown mahogany ember
        let brown_dwarf = planet_blueshifted_thermal_emission(737.0, 1.36);
        assert!(brown_dwarf.red > 0.0 && brown_dwarf.red < 0.10, "Brown dwarf must be very dim ember");
        assert!(brown_dwarf.green > 0.0 && brown_dwarf.green < brown_dwarf.red);
        assert!(brown_dwarf.blue < brown_dwarf.green);

        // 4. Crimson red ember phase (T_eff ≈ 1,800 K, e.g. Earth at D = 6.25):
        let crimson = planet_blueshifted_thermal_emission(288.0, 6.25);
        assert!(crimson.red > 0.15 && crimson.red < 2.0);
        assert!(crimson.red > crimson.green);

        // 5. Incandescent orange phase (T_eff ≈ 3,600 K, e.g. Jupiter at D = 22.0):
        let orange = planet_blueshifted_thermal_emission(165.0, 22.0);
        assert!(orange.red > orange.green);
        assert!(orange.green > orange.blue);

        // 6. High relativistic blue phase (T_eff ≈ 12,000 K, e.g. Jupiter at D = 73.0):
        let blue = planet_blueshifted_thermal_emission(165.0, 73.0);
        assert!(blue.blue > blue.red, "Ultra-relativistic blueshift must shift to blue");
    }

    #[test]
    fn test_cmb_dipole_forward_optics() {
        // Rest state (gamma = 1.0, beta = 0.0): isotropic 2.73 K microwaves, completely dark in visible band
        let (temp_0, col_0, int_0) = cmb_dipole_forward_optics(1.0, 0.0);
        assert!((temp_0 - 2.7255).abs() < 1e-3);
        assert_eq!(col_0.red, 0.0);
        assert_eq!(col_0.green, 0.0);
        assert_eq!(col_0.blue, 0.0);
        assert_eq!(int_0, 0.0);

        // Sub-relativistic speed (gamma = 50.0, beta = 0.9998): T' ~ 272 K <= 600 K, still infrared/invisible
        let (temp_50, col_50, int_50) = cmb_dipole_forward_optics(50.0, 0.9998);
        assert!(temp_50 < 600.0);
        assert_eq!(col_50.red, 0.0);
        assert_eq!(int_50, 0.0);

        // Extreme relativistic boost (gamma = 163.0, beta = 0.999981): T' ~ 888 K, crimson ember halo emerges
        let (temp_163, col_163, int_163) = cmb_dipole_forward_optics(163.0, 0.999981);
        assert!(temp_163 > 800.0 && temp_163 < 950.0);
        assert!(col_163.red > 0.0);
        assert_eq!(col_163.blue, 0.0); // Infrared edge: pure red/crimson
        assert!(int_163 > 0.10);

        // Ultra-relativistic boost (gamma = 1,000.0, beta = 0.9999995): T' ~ 5,450 K, incandescent solar white
        let (temp_1000, col_1000, int_1000) = cmb_dipole_forward_optics(1000.0, 0.9999995);
        assert!(temp_1000 > 5000.0);
        assert!(col_1000.red > 0.8 && col_1000.green > 0.7 && col_1000.blue > 0.5);
        assert!(int_1000 > 5.0);

        // Super-relativistic boost (gamma = 4,000.0): T' > 20,000 K, dazzling electric violet-blue
        let (temp_4000, col_4000, int_4000) = cmb_dipole_forward_optics(4000.0, 0.9999999);
        assert!(temp_4000 > 20000.0);
        assert_eq!(col_4000.blue, 1.0);
        assert!(col_4000.blue > col_4000.red);
        assert!(int_4000 > 15.0);
    }

    #[test]
    fn test_relativistic_background_rainbow() {
        // Sub-relativistic speed (beta = 0.28): 100% pitch black everywhere
        let low_speed = relativistic_background_rainbow(45.0, 0.28, 0.35, 1.04);
        assert_eq!(low_speed.red, 0.0);
        assert_eq!(low_speed.green, 0.0);
        assert_eq!(low_speed.blue, 0.0);

        // At beta = 0.8082 (ScienceClic Screenshot 1 speed):
        // 1. Inside ring (theta = 10 deg): pitch black space
        let inside = relativistic_background_rainbow(10.0, 0.8082, 0.45, 1.70);
        assert_eq!(inside.red, 0.0);
        assert_eq!(inside.green, 0.0);
        assert_eq!(inside.blue, 0.0);

        // 2. Inner edge of ring (theta = 38 deg): Electric Blue / Cyan
        let blue_edge = relativistic_background_rainbow(38.0, 0.8082, 0.45, 1.70);
        assert!(blue_edge.blue > 0.2);

        // 3. Middle of ring (theta = 48 deg): Emerald Green dominance
        let green_mid = relativistic_background_rainbow(48.0, 0.8082, 0.45, 1.70);
        assert!(green_mid.green > 0.2);

        // 4. Outer edge of ring (theta = 60 deg): Orange / Scarlet Red
        let red_edge = relativistic_background_rainbow(60.0, 0.8082, 0.45, 1.70);
        assert!(red_edge.red > 0.2);
        assert_eq!(red_edge.blue, 0.0);

        // 5. Outside ring (theta = 85 deg): pitch black space
        let outside = relativistic_background_rainbow(85.0, 0.8082, 0.45, 1.70);
        assert_eq!(outside.red, 0.0);
        assert_eq!(outside.green, 0.0);
        assert_eq!(outside.blue, 0.0);

        // Continuous contraction without artificial limit:
        // - At beta = 0.80c: wide on side (~49°)
        // - At beta = 0.9988c: crunch forward (~14°)
        // - At beta = 0.99999c: tight halo (<5°)
        // - At beta = 0.999999c: thin needle ring (<3°)
        let (th_80, dth_80) = relativistic_rainbow_ring_params(0.8082);
        let (th_9988, dth_9988) = relativistic_rainbow_ring_params(0.9988);
        let (th_99999, dth_99999) = relativistic_rainbow_ring_params(0.99999);
        let (th_999999, dth_999999) = relativistic_rainbow_ring_params(0.999999);

        assert!(th_80 > 40.0, "At 0.80c ring must be wide on side, got {}", th_80);
        assert!(th_9988 < 20.0, "At 0.9988c ring must crunch to front, got {}", th_9988);
        assert!(th_99999 < 6.0, "At 0.99999c ring must continue crunching, got {}", th_99999);
        assert!(th_999999 < 3.5, "At 0.999999c ring must reach thin needle halo, got {}", th_999999);

        // Half-thickness must thin proportionally as speed approaches c
        assert!(dth_80 > 10.0);
        assert!(dth_9988 < dth_80);
        assert!(dth_99999 < dth_9988);
        assert!(dth_999999 < 1.0, "At ultra-relativistic speed thickness must be < 1.0 deg, got {}", dth_999999);

        // Continuous precision test across gamma = 2360 to 4100 (where float32 quantization stalls):
        // 64-bit rapidity zeta guarantees strictly monotonic shrinking with zero stalls/jumps
        let zeta_2360 = (2360.0_f64).acosh();
        let zeta_3000 = (3000.0_f64).acosh();
        let zeta_4100 = (4100.0_f64).acosh();
        let (th_2360, _) = relativistic_rainbow_ring_params_from_zeta(zeta_2360);
        let (th_3000, _) = relativistic_rainbow_ring_params_from_zeta(zeta_3000);
        let (th_4100, _) = relativistic_rainbow_ring_params_from_zeta(zeta_4100);
        assert!(th_2360 > th_3000, "Must shrink between gamma=2360 and gamma=3000, got {} vs {}", th_2360, th_3000);
        assert!(th_3000 > th_4100, "Must shrink between gamma=3000 and gamma=4100, got {} vs {}", th_3000, th_4100);
    }

    #[test]
    fn test_relativistic_rainbow_color_radial_spectrum() {
        // Inner edge (u = 0.0): alpha 0, completely dark
        let c0 = relativistic_rainbow_color_at_u(0.0, 0.45);
        assert_eq!(c0.alpha, 0.0);
        assert_eq!(c0.red, 0.0);
        assert_eq!(c0.green, 0.0);
        assert_eq!(c0.blue, 0.0);

        // Blue edge (u = 0.15): royal blue
        let c_blue = relativistic_rainbow_color_at_u(0.15, 0.45);
        assert!(c_blue.blue > 0.3);
        assert_eq!(c_blue.red, 0.0);

        // Green middle (u = 0.45): emerald green
        let c_green = relativistic_rainbow_color_at_u(0.45, 0.45);
        assert!(c_green.green > 0.3);

        // Red outer edge (u = 0.90): deep red
        let c_red = relativistic_rainbow_color_at_u(0.90, 0.45);
        assert!(c_red.red > 0.3);
        assert_eq!(c_red.blue, 0.0);

        // Outer boundary (u = 1.0): alpha 0, completely dark
        let c1 = relativistic_rainbow_color_at_u(1.0, 0.45);
        assert_eq!(c1.alpha, 0.0);
        assert_eq!(c1.red, 0.0);
        assert_eq!(c1.green, 0.0);
        assert_eq!(c1.blue, 0.0);

        // Out of bounds (u < 0 or u > 1)
        assert_eq!(relativistic_rainbow_color_at_u(-0.1, 0.45).alpha, 0.0);
        assert_eq!(relativistic_rainbow_color_at_u(1.1, 0.45).alpha, 0.0);
    }

    #[test]
    fn test_paczynski_wiita_gravity_planetary_and_horizon_limits() {
        let earth_mass = 5.9722e24; // kg
        let earth_radius = 6_371_000.0; // m
        let c = REAL_C;

        let body_pos = DVec3::ZERO;
        // Surface test
        let shuttle_surf = DVec3::new(earth_radius, 0.0, 0.0);
        let g_surf = paczynski_wiita_gravity(shuttle_surf, body_pos, earth_mass, earth_radius, c);
        let g_mag = g_surf.length();
        // Earth standard surface gravity ~ 9.82 m/s²
        assert!((g_mag - 9.82).abs() < 0.05, "Surface gravity should be ~9.82 m/s², got {}", g_mag);
        assert!(g_surf.x < 0.0, "Gravity vector must point towards body at origin");

        // High orbit test (2 * R_earth): should follow inverse-square law (~ g / 4)
        let shuttle_orbit = DVec3::new(earth_radius * 2.0, 0.0, 0.0);
        let g_orbit = paczynski_wiita_gravity(shuttle_orbit, body_pos, earth_mass, earth_radius, c);
        assert!((g_orbit.length() - (g_mag / 4.0)).abs() < 0.05);

        // Planetary interior test (halfway to core): should linearly scale to g / 2
        let shuttle_interior = DVec3::new(earth_radius * 0.5, 0.0, 0.0);
        let g_int = paczynski_wiita_gravity(shuttle_interior, body_pos, earth_mass, earth_radius, c);
        assert!((g_int.length() - (g_mag * 0.5)).abs() < 0.05);

        // Core test (dist = 0): should be 0
        let g_core = paczynski_wiita_gravity(DVec3::ZERO, body_pos, earth_mass, earth_radius, c);
        assert_eq!(g_core, DVec3::ZERO);

        // Black hole horizon limit test (M = 10 solar masses, r -> r_s)
        let bh_mass = 10.0 * 1.9885e30;
        let r_s = 2.0 * GRAVITATIONAL_CONSTANT * bh_mass / (c * c);
        // At 1.01 * r_s, effective distance is 0.01 * r_s, acceleration diverges immensely
        let shuttle_bh = DVec3::new(r_s * 1.01, 0.0, 0.0);
        let g_bh = paczynski_wiita_gravity(shuttle_bh, body_pos, bh_mass, 0.0, c);
        assert!(g_bh.length() > 1e12, "Gravity near event horizon must diverge");
    }

    #[test]
    fn test_integrate_relativistic_acceleration_speed_of_light_limit() {
        let c = DEMO_C; // 5000 m/s
        let mut rapidity = DVec3::ZERO;
        let accel = DVec3::new(100_000.0, 0.0, 0.0); // Extreme acceleration (100,000 m/s²)
        let dt = 1.0; // 1 second

        // Accelerate for 100 seconds
        for _ in 0..100 {
            rapidity = integrate_relativistic_acceleration(rapidity, accel, dt, c);
        }

        // Coordinate velocity v = c * tanh(zeta)
        let v = c * rapidity.length().tanh();
        assert!(v < c, "Coordinate velocity must NEVER reach or exceed c (got {} vs c = {})", v, c);
        assert!(v > c * 0.9999, "Velocity should approach c asymptotically");
    }
}
