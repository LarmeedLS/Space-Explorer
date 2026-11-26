use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};
use crate::camera::{CameraViewMode, SpaceCamera};
use crate::coords::UniversalPosition;
use crate::relativity::{RelativisticConfig, RelativisticState};

#[inline]
fn quat_to_dquat(q: Quat) -> DQuat {
    DQuat::from_xyzw(q.x as f64, q.y as f64, q.z as f64, q.w as f64)
}

#[derive(Resource, Default)]
pub struct SimulationPause(pub bool);

/// Target autopilot alignment rotation speed in degrees per second.
/// Adjust this value to control how fast the ship pivots to focus on locked targets (default: 30.0 deg/s).
pub const TARGET_ALIGN_SPEED_DEG: f32 = 30.0;

pub fn pause_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut pause: ResMut<SimulationPause>,
    loading_tracker: Option<Res<crate::ui::StartupLoadingTracker>>,
) {
    if loading_tracker.as_ref().map_or(false, |t| !t.is_loaded) {
        return;
    }
    if keyboard.just_pressed(KeyCode::Backspace) || keyboard.just_pressed(KeyCode::F1) {
        pause.0 = !pause.0;
    }
}

/// Advances simulation time across coordinate and proper time frames in strict synchrony
/// with celestial orbital mechanics and axial rotation. Runs at the very start of the
/// update chain so all systems (planets, moons, shuttle, targeting) evaluate at the exact
/// same timestamp, eliminating any 1-frame orbital jump when pausing or unpausing.
pub fn shuttle_time_system(
    time: Res<Time>,
    pause: Res<SimulationPause>,
    config: Res<RelativisticConfig>,
    mut query: Query<&mut RelativisticState, With<Shuttle>>,
) {
    if pause.0 {
        return;
    }
    // Use the exact same f64 clamped dt_frame as shuttle_input_system so celestial orbits
    // and shuttle motion stay bit-for-bit synchronized even during frame hitches.
    let dt_frame = time.delta_secs_f64().min(0.05);
    let d_tau = dt_frame * config.time_scale;
    for mut rel_state in query.iter_mut() {
        rel_state.proper_time += d_tau;
        let dt_coordinate = rel_state.gamma * d_tau;
        rel_state.coordinate_time += dt_coordinate;
    }
}

#[derive(Resource)]
pub struct FpsTracker {
    pub smoothed_fps: f64,
    pub smoothed_frame_time_ms: f64,
    pub recent_texture_op: Option<(String, std::time::Instant)>,
}

impl Default for FpsTracker {
    fn default() -> Self {
        Self {
            smoothed_fps: 60.0,
            smoothed_frame_time_ms: 16.6,
            recent_texture_op: None,
        }
    }
}

impl FpsTracker {
    pub fn record_texture_op(&mut self, description: String) {
        self.recent_texture_op = Some((description, std::time::Instant::now()));
    }
}

pub fn fps_tracking_system(
    time: Res<Time>,
    mut tracker: ResMut<FpsTracker>,
) {
    let dt = time.delta_secs_f64();
    if dt > 0.0001 {
        let current_fps = 1.0 / dt;
        let current_ms = dt * 1000.0;
        // Consistent ~0.2s time constant exponential smoothing across any FPS
        let alpha = (1.0 - (-5.0 * dt).exp()).clamp(0.01, 1.0);
        tracker.smoothed_fps = tracker.smoothed_fps * (1.0 - alpha) + current_fps * alpha;
        tracker.smoothed_frame_time_ms = tracker.smoothed_frame_time_ms * (1.0 - alpha) + current_ms * alpha;

        // Diagnostic Performance Alert: Detect frame time hitch exceeding 50ms (drops below 20 FPS)
        // Suppresses alerts during the first 3.0 seconds of engine startup (asset loading & shader pipeline compilation)
        if current_ms >= 50.0 && time.elapsed_secs() > 3.0 {
            if let Some((ref op_desc, timestamp)) = tracker.recent_texture_op {
                let elapsed_ms = timestamp.elapsed().as_secs_f64() * 1000.0;
                if elapsed_ms < 1500.0 {
                    warn!(
                        "[PerfAlert] Frame hitch detected: {:.1} ms ({:.1} FPS, smoothed: {:.1} FPS) -> Correlated with texture operation '{}' finished {:.1} ms ago",
                        current_ms, current_fps, tracker.smoothed_fps, op_desc, elapsed_ms
                    );
                } else {
                    warn!(
                        "[PerfAlert] Frame hitch detected: {:.1} ms ({:.1} FPS, smoothed: {:.1} FPS)",
                        current_ms, current_fps, tracker.smoothed_fps
                    );
                }
            } else {
                warn!(
                    "[PerfAlert] Frame hitch detected: {:.1} ms ({:.1} FPS, smoothed: {:.1} FPS)",
                    current_ms, current_fps, tracker.smoothed_fps
                );
            }
        }
    }
}

pub fn vsync_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    // F2 toggles between VSync (synced to monitor refresh rate) and Uncapped (raw RTX 5070 performance)
    if keyboard.just_pressed(KeyCode::F2) {
        if let Ok(mut window) = windows.get_single_mut() {
            let is_vsync = matches!(
                window.present_mode,
                PresentMode::AutoVsync | PresentMode::Fifo | PresentMode::FifoRelaxed
            );
            window.present_mode = if is_vsync {
                PresentMode::AutoNoVsync
            } else {
                PresentMode::AutoVsync
            };
        }
    }
}

/// Flight and Navigation Autopilot Mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutopilotMode {
    #[default]
    Off,
    /// Facing/Target auto-alignment (F key)
    AlignOnly,
    /// Sci-fi Hyperdrive Warp Cruise: full boost burn towards target until final approach,
    /// then instant exponential emergency retro-brake drop (H key)
    Hyperdrive,
    /// Realistic Relativistic Brachistochrone: accelerate first half, 180° flip at midpoint,
    /// retro-thrust burn to zero velocity at orbital arrival (L key)
    Lorentz,
}

/// Execution phase of the autonomous flight state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutopilotPhase {
    #[default]
    Idle,
    /// Pre-flight emergency braking: stopping shuttle completely before starting autopilot run
    PreBraking,
    /// Aligning ship attitude to target (or retrograde vector)
    Aligning,
    /// Main engine firing forward at full throttle
    Accelerating,
    /// Relativistic high-speed cruise
    Cruising,
    /// Midcourse 180° turnaround flip maneuver (Lorentz mode)
    Flipping,
    /// Retrograde thruster burn deceleration (Lorentz mode)
    Decelerating,
    /// Low-speed final approach guidance into orbit
    TerminalApproach,
    /// High-G exponential emergency braking drop (Hyperdrive mode)
    EmergencyBraking,
    /// Safely parked in target orbit
    Arrived,
}

/// Ground contact elevation offset for the Federal Corvette airframe:
/// The 3D model spans 27.9m in height, with landing skids positioned 13.95m below the center.
pub const SHUTTLE_BELLY_CLEARANCE: f64 = 13.95;

#[derive(Component)]
pub struct Shuttle {
    pub angular_velocity: Vec3,
    pub prev_rotation: Quat,
    /// Mouse rotation buffer for physical rate-limiting in slow motion
    pub mouse_accum: Vec2,
    pub smoothed_angular_speed_deg: f32,
    pub current_proper_accel: f64,
    pub current_grav_accel: f64,
    pub flight_assist: bool,
    pub boost_active: bool,
    pub braking_active: bool,
    pub thruster_forward: f32,
    pub thruster_strafe: f32,
    pub thruster_vertical: f32,
    pub mouse_sensitivity: Vec2,
    /// Cruise control / Autopilot locks:
    /// -1.0 for reverse cruise, +1.0 for forward cruise, 0.0 for none
    pub locked_thrust_dir: f32,
    pub locked_boost: bool,
    /// Automated alignment to current navigation target (Key F)
    pub align_to_target: bool,
    /// Active target tracking turn speed in deg/s (default: TARGET_ALIGN_SPEED_DEG = 30.0)
    pub active_track_speed_deg: f32,
    /// Two-phase target tracking: when true, RCS thrusters first brake existing fast spin
    /// along its current direction before pivoting to target.
    pub tracking_braking: bool,
    /// Whether initial braking should stop completely at zero (if spinning away from target)
    /// or stop at 30 deg/s (if already spinning towards target).
    pub tracking_brake_to_zero: bool,
    /// Autopilot high-level flight mode
    pub autopilot_mode: AutopilotMode,
    /// Current phase within the autonomous autopilot state machine
    pub autopilot_phase: AutopilotPhase,
    /// When true, holding X brakes relative to the universal Sun frame (Shift+X) rather than target
    pub brake_to_universal: bool,
    /// Celestial body on which the shuttle is currently landed/docked
    pub landed_body: Option<Entity>,
    /// Unit normal vector in the planet's local body frame at the touchdown site
    pub landed_local_normal: DVec3,
    /// Timer for line-of-sight occlusion warning banner (in seconds)
    pub occlusion_warning_timer: f32,
    /// Previous autopilot mode for change-detection logging
    pub prev_autopilot_mode: AutopilotMode,
    /// Previous autopilot phase for change-detection logging
    pub prev_autopilot_phase: AutopilotPhase,
    /// Timer for periodic diagnostic logging during autonomous flight
    pub last_ap_log_timer: f32,
    /// Last retro-burn state for change-detection logging in Lorentz decelerating phase
    pub last_need_burn: bool,
}

impl Default for Shuttle {
    fn default() -> Self {
        Self {
            angular_velocity: Vec3::ZERO,
            prev_rotation: Quat::IDENTITY,
            mouse_accum: Vec2::ZERO,
            smoothed_angular_speed_deg: 0.0,
            current_proper_accel: 0.0,
            current_grav_accel: 0.0,
            flight_assist: true,
            boost_active: false,
            braking_active: false,
            brake_to_universal: false,
            landed_body: None,
            landed_local_normal: DVec3::Y,
            occlusion_warning_timer: 0.0,
            prev_autopilot_mode: AutopilotMode::Off,
            prev_autopilot_phase: AutopilotPhase::Idle,
            last_ap_log_timer: 0.0,
            last_need_burn: false,
            thruster_forward: 0.0,
            thruster_strafe: 0.0,
            thruster_vertical: 0.0,
            mouse_sensitivity: Vec2::new(0.0003, 0.0003),
            locked_thrust_dir: 0.0,
            locked_boost: false,
            align_to_target: false,
            active_track_speed_deg: TARGET_ALIGN_SPEED_DEG,
            tracking_braking: false,
            tracking_brake_to_zero: false,
            autopilot_mode: AutopilotMode::Off,
            autopilot_phase: AutopilotPhase::Idle,
        }
    }
}

/// Component for visual engine thruster plumes with customizable nozzle position and flame length
#[derive(Component, Debug, Clone, Copy)]
pub struct EnginePlume {
    pub nozzle_z: f32,
    pub flame_length: f32,
}

impl Default for EnginePlume {
    fn default() -> Self {
        Self {
            nozzle_z: 79.8,
            flame_length: 35.0,
        }
    }
}

/// Marker component for the 4 glowing horizontal rectangular emitter slats in each engine housing
#[derive(Component, Debug, Clone, Copy)]
pub struct EngineSlat;

// ==============================================================================
// ENGINE SLATS LUMINOSITY CONFIGURATION (Emissive Intensity of the 8 Rectangles)
// ==============================================================================
pub const ENGINE_SLAT_LUM_IDLE: f32 = 4.0;     // Cruising / resting glow (no forward thrust)
pub const ENGINE_SLAT_LUM_THRUST: f32 = 15.0;  // Standard forward burn (holding 'Z' key)
pub const ENGINE_SLAT_LUM_BOOST: f32 = 30.0;   // Hyperdrive / afterburner boost (holding 'Shift' key)

/// Marker component for the engine bay lighting
#[derive(Component, Debug, Clone, Copy)]
pub struct EngineBayLight {
    pub base_intensity: f32,
}

/// Computes a stable, roll-neutral target orientation pointing towards `dir`.
/// Unlike Quat::from_rotation_arc, this uses orthogonal vector cross-products
/// which never suffer from near-zero cross-product jitter or axis singularities.
pub fn target_rotation_facing(dir: Vec3, current_up: Vec3, current_right: Vec3) -> Quat {
    let mut right = dir.cross(current_up);
    if right.length_squared() < 0.0001 {
        right = dir.cross(current_right);
    }
    if right.length_squared() < 0.0001 {
        right = Vec3::X;
    }
    let right = right.normalize();
    let up = right.cross(dir).normalize();

    // In Bevy coordinate system: +X is right, +Y is up, -Z is forward (dir)
    let mat = Mat3::from_cols(right, up, -dir);
    Quat::from_mat3(&mat).normalize()
}

/// Aligns shuttle orientation so that local +Y points along the celestial surface normal
/// and local forward (-Z) is preserved along the terrain tangent plane.
#[allow(dead_code)]
pub fn align_shuttle_to_surface(current_rot: Quat, surface_normal: Vec3) -> Quat {
    let surface_up = surface_normal.normalize();
    let current_fwd = current_rot * Vec3::NEG_Z;
    let tangent_fwd = (current_fwd - surface_up * current_fwd.dot(surface_up)).normalize_or_zero();
    let final_fwd = if tangent_fwd.length_squared() > 0.5 {
        tangent_fwd
    } else {
        let fallback = if surface_up.y.abs() < 0.9 { Vec3::Y } else { Vec3::Z };
        surface_up.cross(fallback).normalize()
    };
    let right = final_fwd.cross(surface_up).normalize();
    let up = surface_up;
    let mat = Mat3::from_cols(right, up, -final_fwd);
    Quat::from_mat3(&mat).normalize()
}

/// Computes the optimal orbital arrival distance for a celestial target based on its physical size.
/// For the Sun: ~5.5 ls (outside solar corona)
/// For Gas Giants (Jupiter, Saturn): ~3.2 ls (user-requested 3.0 - 3.5 ls)
/// For Ice Giants (Uranus, Neptune): ~1.5 ls
/// For Major Terrestrial (Earth, Venus): ~0.6 ls
/// For Small Terrestrial (Mars, Mercury): ~0.2 ls
/// For Moons and Minor Bodies (Luna): ~0.1 ls (user-requested 0.1 ls for close approaches)
pub fn target_arrival_distance(body_radius: f64, c: f64, is_demo: bool) -> f64 {
    if is_demo {
        (body_radius * 2.0).max(150.0)
    } else if body_radius > 500_000_000.0 {
        // Sol (Sun, radius ~696,340 km): ~5.5 light-seconds (~1.65 million km)
        body_radius * 2.4
    } else if body_radius > 40_000_000.0 {
        // Gas Giants (Jupiter, Saturn): 3.2 light-seconds (~960,000 km)
        3.2 * c
    } else if body_radius > 15_000_000.0 {
        // Ice Giants (Uranus, Neptune): 1.5 light-seconds (~450,000 km)
        1.5 * c
    } else if body_radius > 5_000_000.0 {
        // Major Terrestrial (Earth, Venus): 0.6 light-seconds (~180,000 km)
        0.6 * c
    } else if body_radius > 2_000_000.0 {
        // Small Terrestrial (Mars, Mercury): 0.2 light-seconds (~60,000 km)
        0.2 * c
    } else {
        // Moons & Minor Bodies (Luna): 0.1 light-seconds (~30,000 km)
        0.1 * c
    }
}

/// Computes the exact coordinate stopping distance under exponential velocity decay
/// dv/dτ = -k * v matching the simulation engine's active brake dampers:
/// In each frame, v decays by exp(-k * dτ) and coordinate translation is
/// dx = v * dt_coord = v * γ * dτ = c * (β / sqrt(1 - β^2)) * dτ.
pub fn calculate_exponential_brake_stopping_distance(c: f64, theta: f64, k: f64, dt_frame: f64) -> f64 {
    if theta <= 1e-6 || k <= 1e-6 {
        return 0.0;
    }

    let dt = dt_frame.clamp(0.001, 0.05);
    let decay = (-k * dt).exp();
    let mut beta = theta.tanh().clamp(0.0, 0.9999999999);
    let mut distance = 0.0;

    while beta > 1e-5 {
        beta *= decay;
        let gamma = 1.0 / (1.0 - beta * beta).sqrt();
        distance += c * beta * gamma * dt;
    }

    distance
}

/// Initiates two-phase target tracking smoothly:
/// If the ship is currently tumbling or turning fast (e.g. SAS off or rapid spin),
/// Phase 1 first fires RCS retro-thrusters to brake the existing spin along its current
/// spin direction down to rest before pivoting towards the target.
pub fn initiate_target_alignment(
    shuttle: &mut Shuttle,
    transform: &Transform,
) {
    shuttle.align_to_target = true;
    let ang_speed_deg = shuttle.smoothed_angular_speed_deg.max(shuttle.angular_velocity.length().to_degrees());
    if ang_speed_deg > 3.0 {
        shuttle.tracking_braking = true;
        shuttle.tracking_brake_to_zero = true;
        if shuttle.angular_velocity.length() < 0.05 {
            let q_curr = quat_to_dquat(transform.rotation);
            let q_prev = quat_to_dquat(shuttle.prev_rotation);
            let rot_diff_world = q_curr * q_prev.inverse();
            let v = DVec3::new(rot_diff_world.x, rot_diff_world.y, rot_diff_world.z);
            if v.length_squared() > 1e-12 {
                let axis_world = (v / v.length()).as_vec3();
                let axis_local: Vec3 = transform.rotation.inverse() * axis_world;
                shuttle.angular_velocity = axis_local * ang_speed_deg.to_radians();
            }
        }
    } else {
        shuttle.tracking_braking = false;
        shuttle.tracking_brake_to_zero = false;
        shuttle.active_track_speed_deg = 4.0;
    }
}

pub fn shuttle_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut mouse_wheel: EventReader<MouseWheel>,
    config: Res<RelativisticConfig>,
    pause: Res<SimulationPause>,
    nav: Res<crate::navigation::NavigationComputer>,
    target_query: Query<
        (
            &UniversalPosition,
            Option<&crate::celestial::CelestialBody>,
            Option<&crate::celestial::BodyVelocity>,
            Option<&crate::celestial::NavTarget>,
        ),
        (With<crate::celestial::NavTarget>, Without<Shuttle>),
    >,
    celestial_query: Query<
        (
            Entity,
            &UniversalPosition,
            &crate::celestial::CelestialBody,
            Option<&crate::celestial::BodyVelocity>,
            Option<&crate::celestial::AxialRotation>,
            Option<&Transform>,
            Option<&crate::celestial::EllipsoidRadii>,
        ),
        Without<Shuttle>,
    >,
    mut query: Query<
        (&mut Shuttle, &mut RelativisticState, &mut UniversalPosition, &mut Transform),
        (With<Shuttle>, Without<crate::celestial::NavTarget>),
    >,
    time: Res<Time>,
    windows: Query<&Window>,
    mut camera_query: Query<&mut SpaceCamera>,
    loading_tracker: Option<Res<crate::ui::StartupLoadingTracker>>,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };
    let is_cursor_locked = window.cursor_options.grab_mode != bevy::window::CursorGrabMode::None;

    // Suppress pilot steering inputs while the startup loading screen is active
    if loading_tracker.as_ref().map_or(false, |t| !t.is_loaded) {
        return;
    }
    let dt_frame = time.delta_secs_f64().min(0.05);
    // In slow-motion, manual steering and attitude dynamics scale proportionally with simulation speed
    // so that the entire simulation (autopilots, ship rotation, orbital physics, flight velocity) moves in unified slow-motion.
    let sim_time_factor = (config.time_scale as f32).clamp(0.001, 5.0);
    let dt_f32 = (time.delta_secs() * sim_time_factor).min(0.05);

    let ctrl_pressed = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let shift_pressed = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    // Read mouse scroll wheel / trackpad 2-finger swipe
    let mut scroll_delta = 0.0f32;
    for ev in mouse_wheel.read() {
        match ev.unit {
            MouseScrollUnit::Line => scroll_delta += ev.y,
            MouseScrollUnit::Pixel => scroll_delta += ev.y * 0.05,
        }
    }

    // Mouse Sensitivity Adjustment:
    // 1. Ctrl + Scroll (or Ctrl + 2-finger trackpad swipe)
    // 2. Keyboard buttons: [-] and [+] on numpad or main keyboard
    let mut sens_change = 0.0f32;
    if ctrl_pressed && scroll_delta.abs() > 0.0 {
        sens_change += scroll_delta * 0.15;
    }
    if keyboard.just_pressed(KeyCode::NumpadAdd) || keyboard.just_pressed(KeyCode::Equal) {
        sens_change += 0.20; // +20%
    }
    if keyboard.just_pressed(KeyCode::NumpadSubtract) || keyboard.just_pressed(KeyCode::Minus) {
        sens_change -= 0.20; // -20%
    }

    // Camera Zoom via normal Scroll (when Ctrl is not held and cursor is locked for flight piloting)
    // In Free Mouse mode (Tab, cursor unlocked), scroll wheel zooms the orbital radar map instead
    // Sets target zoom distance for constant-speed smooth travel
    if is_cursor_locked && !ctrl_pressed && scroll_delta.abs() > 0.0 {
        for mut cam in camera_query.iter_mut() {
            match cam.mode {
                CameraViewMode::Chase => {
                    cam.target_chase_distance = (cam.target_chase_distance - scroll_delta * 8.0).clamp(60.0, 350.0);
                }
                CameraViewMode::Cockpit => {
                    cam.target_cockpit_z = (cam.target_cockpit_z + scroll_delta * 0.5).clamp(14.0, 25.0);
                }
            }
        }
    }

    // 1. Check Free-Look (holding Alt temporarily, permanent toggle via F3 / Numpad, or when Paused)
    // Permanent free-look (F3 / Numpad) is NOT cancelled by pressing Alt, so OS shortcuts like Alt+Tab
    // never accidentally reset the camera back to center. Press F3 or Numpad 5 to recenter.
    let alt_pressed = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);

    for mut cam in camera_query.iter_mut() {
        if keyboard.just_pressed(KeyCode::F3) {
            cam.free_look_active = !cam.free_look_active;
        }
    }

    let in_autonomous_flight = query.iter().next().map(|(s, _, _, _)| {
        s.autopilot_mode == AutopilotMode::Hyperdrive || s.autopilot_mode == AutopilotMode::Lorentz
    }).unwrap_or(false);

    let is_free_look = pause.0
        || camera_query.iter().any(|c| c.free_look_active)
        || alt_pressed
        || in_autonomous_flight;

    // 2. Read mouse motion
    let mut mouse_delta = Vec2::ZERO;
    if is_cursor_locked {
        for motion in mouse_motion.read() {
            mouse_delta += motion.delta;
        }
    } else {
        for _ in mouse_motion.read() {}
    }

    // Apply mouse sensitivity change if triggered (operates in both active flight and pause modes)
    if sens_change != 0.0 {
        for (mut shuttle, _, _, _) in query.iter_mut() {
            let new_sens = (shuttle.mouse_sensitivity.x * (1.0 + sens_change)).clamp(0.00005, 0.0030);
            shuttle.mouse_sensitivity.x = new_sens;
            shuttle.mouse_sensitivity.y = new_sens;
        }
    }

    // 3. Numpad Camera Controls:
    // 8: Pitch Up, 2: Pitch Down, 4: Yaw Left, 6: Yaw Right
    // 7: Up-Left, 9: Up-Right, 1: Down-Left, 3: Down-Right
    // 5: Recenter view to front
    // 0: Look directly back (180° snap view)
    let mut numpad_pitch_dir = 0.0f32;
    let mut numpad_yaw_dir = 0.0f32;

    if keyboard.pressed(KeyCode::Numpad8) {
        numpad_pitch_dir += 1.0;
    }
    if keyboard.pressed(KeyCode::Numpad2) {
        numpad_pitch_dir -= 1.0;
    }
    if keyboard.pressed(KeyCode::Numpad4) {
        numpad_yaw_dir += 1.0;
    }
    if keyboard.pressed(KeyCode::Numpad6) {
        numpad_yaw_dir -= 1.0;
    }
    if keyboard.pressed(KeyCode::Numpad7) {
        numpad_pitch_dir += 1.0;
        numpad_yaw_dir += 1.0;
    }
    if keyboard.pressed(KeyCode::Numpad9) {
        numpad_pitch_dir += 1.0;
        numpad_yaw_dir -= 1.0;
    }
    if keyboard.pressed(KeyCode::Numpad1) {
        numpad_pitch_dir -= 1.0;
        numpad_yaw_dir += 1.0;
    }
    if keyboard.pressed(KeyCode::Numpad3) {
        numpad_pitch_dir -= 1.0;
        numpad_yaw_dir -= 1.0;
    }

    let numpad_center = keyboard.just_pressed(KeyCode::Numpad5);
    let numpad_look_back = keyboard.pressed(KeyCode::Numpad0) || keyboard.pressed(KeyCode::Insert);
    for mut cam in camera_query.iter_mut() {
        cam.rear_view_active = numpad_look_back;
    }

    // Real wall-clock frame delta for camera viewport controls (unaffected by simulation slow-motion/warp)
    let dt_real = time.delta_secs().min(0.05);

    let numpad_active = numpad_pitch_dir != 0.0 || numpad_yaw_dir != 0.0;
    if numpad_active {
        let len = (numpad_pitch_dir * numpad_pitch_dir + numpad_yaw_dir * numpad_yaw_dir).sqrt();
        let norm_pitch = numpad_pitch_dir / len;
        let norm_yaw = numpad_yaw_dir / len;
        let numpad_pan_speed = 1.6f32; // rad/s (approx 90 deg/s)

        for mut cam in camera_query.iter_mut() {
            cam.free_look_active = true;
            cam.target_look_yaw += norm_yaw * numpad_pan_speed * dt_real;
            cam.target_look_pitch = (cam.target_look_pitch + norm_pitch * numpad_pan_speed * dt_real).clamp(-1.30, 1.30);
        }
    }

    if numpad_center {
        for mut cam in camera_query.iter_mut() {
            cam.free_look_active = false;
            cam.target_look_yaw = 0.0;
            cam.target_look_pitch = 0.0;
        }
    }

    // 4. Update Camera Orbit / Look if Free-Look is active, or smooth return to center if not
    let shuttle_sens = query.iter().next().map(|(s, _, _, _)| s.mouse_sensitivity.x).unwrap_or(0.0004);
    // Free orbit camera sensitivity matches the player's calibrated steering sensitivity 1:1,
    // eliminating pixel jumps on micro-movements
    let orbit_sens = shuttle_sens;

    if is_free_look || numpad_active {
        let is_landed = query.iter().next().map_or(false, |(s, _, _, _)| s.landed_body.is_some());
        for mut cam in camera_query.iter_mut() {
            cam.target_look_yaw -= mouse_delta.x * orbit_sens;
            let pitch_max = if is_landed {
                let dy = cam.chase_height - 0.5;
                let dz = cam.chase_distance;
                let pitch_down = dy.atan2(dz);
                let r_chase = (dy * dy + dz * dz).sqrt();
                let min_elevation = (-8.5f32 + cam.chase_distance * 0.01).min(0.0);
                let safe_val = ((min_elevation - 0.5) / r_chase).clamp(-1.0, 1.0);
                pitch_down - safe_val.asin()
            } else {
                1.30
            };
            cam.target_look_pitch = (cam.target_look_pitch - mouse_delta.y * orbit_sens).clamp(-1.30, pitch_max);
        }
    } else {
        for mut cam in camera_query.iter_mut() {
            let decay = (-12.0 * dt_real).exp();
            cam.target_look_yaw *= decay;
            cam.target_look_pitch *= decay;
            if cam.target_look_yaw.abs() < 0.0001 {
                cam.target_look_yaw = 0.0;
            }
            if cam.target_look_pitch.abs() < 0.0001 {
                cam.target_look_pitch = 0.0;
            }
        }
    }

    // 4. If PAUSED: Freeze entire spaceship completely! No rotation, no movement, no physics!
    if pause.0 {
        for (mut shuttle, _rel_state, mut upos, mut transform) in query.iter_mut() {
            transform.translation = Vec3::ZERO;
            shuttle.smoothed_angular_speed_deg = 0.0;
            shuttle.prev_rotation = transform.rotation;

            // If resting / landed on a celestial body, solidly maintain surface lock even while paused
            if let Some(landed_ent) = shuttle.landed_body {
                for (b_ent, b_upos, b_body, _b_vel_opt, _b_rot_opt, b_tf_opt, b_ellipsoid_opt) in celestial_query.iter() {
                    if b_ent == landed_ent {
                        let effective_r = if let (Some(ellipsoid), Some(_b_tf)) = (b_ellipsoid_opt, b_tf_opt) {
                            ellipsoid.surface_radius_in_local_dir(shuttle.landed_local_normal)
                        } else {
                            b_body.physical_radius
                        };

                        let body_rot = b_tf_opt.map(|tf| tf.rotation.as_dquat()).unwrap_or(DQuat::IDENTITY);
                        let world_normal = if shuttle.landed_local_normal.length_squared() > 0.5 {
                            (body_rot * shuttle.landed_local_normal).normalize()
                        } else {
                            let rel = upos.0 - b_upos.0;
                            if rel.length_squared() > 1e-6 { rel.normalize() } else { DVec3::Y }
                        };

                        upos.0 = b_upos.0 + world_normal * effective_r;
                        break;
                    }
                }
            }
        }
        return;
    }

    for (mut shuttle, mut rel_state, mut upos, mut transform) in query.iter_mut() {
        // Toggle Flight Assist (SAS attitude damping vs raw Newtonian tumble) with physical 'W' (KeyZ on AZERTY)
        if keyboard.just_pressed(KeyCode::KeyZ) {
            shuttle.flight_assist = !shuttle.flight_assist;
        }

        if shuttle.occlusion_warning_timer > 0.0 {
            shuttle.occlusion_warning_timer = (shuttle.occlusion_warning_timer - dt_f32).max(0.0);
        }

        // 1. Toggle Autopilot Target Alignment with physical 'F'
        if keyboard.just_pressed(KeyCode::KeyF) {
            if in_autonomous_flight {
                // Ignore manual tracking toggle while full autopilot (Hyperdrive or Lorentz) is running
            } else if nav.is_occluded {
                shuttle.occlusion_warning_timer = 3.5; // Inhibit alignment through planetary obstacles
            } else if nav.target_entity.is_some() {
                if shuttle.autopilot_mode == AutopilotMode::AlignOnly {
                    shuttle.autopilot_mode = AutopilotMode::Off;
                    shuttle.autopilot_phase = AutopilotPhase::Idle;
                    shuttle.align_to_target = false;
                    shuttle.tracking_braking = false;
                    shuttle.tracking_brake_to_zero = false;
                } else {
                    shuttle.autopilot_mode = AutopilotMode::AlignOnly;
                    shuttle.autopilot_phase = AutopilotPhase::Aligning;
                    shuttle.braking_active = false;
                    initiate_target_alignment(&mut shuttle, &transform);
                }
            } else {
                shuttle.autopilot_mode = AutopilotMode::Off;
                shuttle.autopilot_phase = AutopilotPhase::Idle;
                shuttle.align_to_target = false;
                shuttle.tracking_braking = false;
                shuttle.tracking_brake_to_zero = false;
            }
        }

        // 2. Toggle Sci-Fi Hyperdrive Warp Autopilot with physical 'H' (Instant Exponential Drop)
        if keyboard.just_pressed(KeyCode::KeyH) {
            if shuttle.autopilot_mode == AutopilotMode::Lorentz {
                // Disallow mode clash while Lorentz is actively engaged
            } else if shuttle.autopilot_mode == AutopilotMode::Hyperdrive {
                shuttle.autopilot_mode = AutopilotMode::Off;
                shuttle.autopilot_phase = AutopilotPhase::Idle;
                shuttle.align_to_target = false;
                shuttle.tracking_braking = false;
                shuttle.tracking_brake_to_zero = false;
                shuttle.braking_active = false;
            } else if nav.is_occluded {
                shuttle.occlusion_warning_timer = 3.5; // Inhibit warp through planetary obstacles
            } else if let Some(target_ent) = nav.target_entity {
                // If already at or inside target arrival distance, do nothing
                let mut already_arrived = false;
                if let Ok((target_upos, target_body, _, _)) = target_query.get(target_ent) {
                    let dist = (target_upos.0 - upos.0).length();
                    let body_radius = target_body.map(|b| b.physical_radius).unwrap_or(100.0);
                    let target_arr_dist = target_arrival_distance(body_radius, config.c, config.is_demo_mode);
                    let arrival_margin = if config.is_demo_mode { 50.0 } else { 0.25 * config.c };
                    if dist <= target_arr_dist + arrival_margin {
                        already_arrived = true;
                    }
                }

                if already_arrived {
                    // Already at target orbit: do nothing
                    shuttle.autopilot_mode = AutopilotMode::Off;
                    shuttle.autopilot_phase = AutopilotPhase::Idle;
                    shuttle.align_to_target = false;
                    shuttle.braking_active = false;
                } else {
                    shuttle.autopilot_mode = AutopilotMode::Hyperdrive;
                    let is_moving = rel_state.rapidity.length_squared() > 1e-8 || rel_state.velocity.length_squared() > 0.25;
                    if is_moving {
                        shuttle.autopilot_phase = AutopilotPhase::PreBraking;
                        shuttle.braking_active = true;
                    } else {
                        shuttle.autopilot_phase = AutopilotPhase::Aligning;
                        shuttle.braking_active = false;
                    }
                    initiate_target_alignment(&mut shuttle, &transform);
                }
            }
        }

        // 3. Toggle Realistic Relativistic Lorentz Brachistochrone Autopilot with physical 'L' (Midcourse Flip)
        if keyboard.just_pressed(KeyCode::KeyL) {
            if shuttle.autopilot_mode == AutopilotMode::Hyperdrive {
                // Disallow mode clash while Hyperdrive is actively engaged
            } else if shuttle.autopilot_mode == AutopilotMode::Lorentz {
                shuttle.autopilot_mode = AutopilotMode::Off;
                shuttle.autopilot_phase = AutopilotPhase::Idle;
                shuttle.align_to_target = false;
                shuttle.tracking_braking = false;
                shuttle.tracking_brake_to_zero = false;
                shuttle.braking_active = false;
            } else if nav.is_occluded {
                shuttle.occlusion_warning_timer = 3.5; // Inhibit Lorentz brachistochrone through planetary obstacles
            } else if let Some(target_ent) = nav.target_entity {
                // If already at or inside target arrival distance, do nothing
                let mut already_arrived = false;
                if let Ok((target_upos, target_body, _, _)) = target_query.get(target_ent) {
                    let dist = (target_upos.0 - upos.0).length();
                    let body_radius = target_body.map(|b| b.physical_radius).unwrap_or(100.0);
                    let target_arr_dist = target_arrival_distance(body_radius, config.c, config.is_demo_mode);
                    let arrival_margin = if config.is_demo_mode { 50.0 } else { 0.25 * config.c };
                    if dist <= target_arr_dist + arrival_margin {
                        already_arrived = true;
                    }
                }

                if already_arrived {
                    // Already at target orbit: do nothing
                    shuttle.autopilot_mode = AutopilotMode::Off;
                    shuttle.autopilot_phase = AutopilotPhase::Idle;
                    shuttle.align_to_target = false;
                    shuttle.braking_active = false;
                } else {
                    shuttle.autopilot_mode = AutopilotMode::Lorentz;
                    let is_moving = rel_state.rapidity.length_squared() > 1e-8 || rel_state.velocity.length_squared() > 0.25;
                    if is_moving {
                        shuttle.autopilot_phase = AutopilotPhase::PreBraking;
                        shuttle.braking_active = true;
                    } else {
                        shuttle.autopilot_phase = AutopilotPhase::Aligning;
                        shuttle.braking_active = false;
                    }
                    initiate_target_alignment(&mut shuttle, &transform);
                }
            }
        }

        // Auto-cancel if target is cleared
        if nav.target_entity.is_none() && shuttle.autopilot_mode != AutopilotMode::Off {
            shuttle.autopilot_mode = AutopilotMode::Off;
            shuttle.autopilot_phase = AutopilotPhase::Idle;
            shuttle.align_to_target = false;
            shuttle.tracking_braking = false;
            shuttle.tracking_brake_to_zero = false;
            shuttle.braking_active = false;
        }

        // Active Emergency Retro-Brake with X (disengages autopilot as fail-safe abort):
        // Hold X: brake relative to target / nearest celestial body
        // Hold Shift + X: brake relative to the Sun (Universal Rest Frame)
        if keyboard.pressed(KeyCode::KeyX) {
            shuttle.braking_active = true;
            shuttle.brake_to_universal = shift_pressed;
            if in_autonomous_flight {
                shuttle.autopilot_mode = AutopilotMode::Off;
                shuttle.autopilot_phase = AutopilotPhase::Idle;
                shuttle.align_to_target = false;
            }
        } else if !in_autonomous_flight {
            shuttle.braking_active = false;
            shuttle.brake_to_universal = false;
        }

        // Respawn / Return to Origin (Numpad . or Key R): Resets position, rapidity, velocity, orientation, and clocks
        if keyboard.just_pressed(KeyCode::NumpadDecimal) || keyboard.just_pressed(KeyCode::KeyR) {
            shuttle.locked_thrust_dir = 0.0;
            shuttle.locked_boost = false;
            shuttle.align_to_target = false;
            shuttle.autopilot_mode = AutopilotMode::Off;
            shuttle.autopilot_phase = AutopilotPhase::Idle;
            shuttle.tracking_braking = false;
            shuttle.tracking_brake_to_zero = false;
            shuttle.active_track_speed_deg = TARGET_ALIGN_SPEED_DEG;
            shuttle.landed_body = None;
            if config.is_demo_mode {
                upos.0 = crate::world::DEMO_SPAWN_POSITION;
                transform.rotation = Quat::IDENTITY;
                rel_state.rapidity = DVec3::ZERO;
            } else {
                let (spawn_pos, spawn_rot, spawn_vel) = crate::celestial::realistic_earth_orbital_spawn();
                upos.0 = spawn_pos;
                transform.rotation = spawn_rot;
                rel_state.rapidity = crate::relativity::rapidity_from_velocity(spawn_vel, config.c);
            }
            rel_state.update_from_rapidity(config.c);
            rel_state.proper_time = 0.0;
            rel_state.coordinate_time = 0.0;
            shuttle.angular_velocity = Vec3::ZERO;
        }

        // --- AUTONOMOUS AUTOPILOT GUIDANCE LOGIC ---
        let mut ap_forward_input = 0.0f32;
        let mut ap_boost = false;
        let mut ap_braking = false;

        if in_autonomous_flight {
            if let Some(target_ent) = nav.target_entity {
                if let Ok((target_upos, target_body, _, _)) = target_query.get(target_ent) {
                    let to_target = target_upos.0 - upos.0;
                    let dist = to_target.length();
                    let target_dir = if dist > 0.001 { to_target / dist } else { DVec3::NEG_Z };

                    let body_radius = target_body.map(|b| b.physical_radius).unwrap_or(100.0);
                    let target_arrival_dist = target_arrival_distance(body_radius, config.c, config.is_demo_mode);
                    let remaining_dist = (dist - target_arrival_dist).max(0.0);

                    let periodic_log = {
                        shuttle.last_ap_log_timer += dt_f32;
                        if shuttle.last_ap_log_timer >= 2.5 {
                            shuttle.last_ap_log_timer = 0.0;
                            true
                        } else {
                            false
                        }
                    };

                    let forward = transform.forward().as_dvec3();
                    let cos_angle = forward.dot(target_dir).clamp(-1.0, 1.0);
                    let angle_to_target_deg = cos_angle.acos().to_degrees() as f32;

                    let current_rapidity = rel_state.rapidity.length();
                    let current_speed = rel_state.velocity.length();

                    let retro_dir = if nav.relative_speed > 0.5 {
                        -nav.relative_velocity.normalize()
                    } else {
                        -target_dir
                    };
                    let cos_retro = forward.dot(retro_dir).clamp(-1.0, 1.0);
                    let angle_to_retro_deg = cos_retro.acos().to_degrees() as f32;

                    // Common Pre-Flight Braking: ensure complete stop relative to closest celestial body before initiating burns
                    if shuttle.autopilot_phase == AutopilotPhase::PreBraking {
                        ap_forward_input = 0.0;
                        ap_boost = false;
                        ap_braking = true;
                        shuttle.brake_to_universal = false; // Brake relative to closest celestial body!
                        shuttle.align_to_target = true; // Allow Phase 1 RCS tracking_braking to decelerate spin smoothly!

                        let mut closest_dist = f64::MAX;
                        let mut closest_vel = DVec3::ZERO;
                        for (_, b_upos, _, b_vel_opt, _, _, _) in celestial_query.iter() {
                            let d = (upos.0 - b_upos.0).length();
                            if d < closest_dist {
                                closest_dist = d;
                                closest_vel = b_vel_opt.map(|bv| bv.0).unwrap_or(DVec3::ZERO);
                            }
                        }

                        let rel_speed_to_closest = (rel_state.velocity - closest_vel).length();
                        let is_angular_stopped = !shuttle.tracking_braking && shuttle.angular_velocity.length() < 0.05;
                        let is_linear_stopped = rel_speed_to_closest < 5.0 || current_rapidity < 1e-4;

                        if is_angular_stopped && is_linear_stopped {
                            rel_state.rapidity = crate::relativity::rapidity_from_velocity(closest_vel, config.c);
                            rel_state.update_from_rapidity(config.c);
                            shuttle.angular_velocity = Vec3::ZERO;
                            shuttle.braking_active = false;
                            shuttle.brake_to_universal = false;
                            shuttle.landed_body = None;
                            shuttle.autopilot_phase = AutopilotPhase::Aligning;
                        }
                    }

                    match shuttle.autopilot_mode {
                        AutopilotMode::Hyperdrive => {
                            // HYPERDRIVE: Uses exact same brake dampers as physical 'X' (k = 4.0)
                            let s_brake = calculate_exponential_brake_stopping_distance(
                                config.c,
                                current_rapidity,
                                4.0,
                                dt_frame,
                            );

                            match shuttle.autopilot_phase {
                                AutopilotPhase::PreBraking => {}
                                AutopilotPhase::Aligning => {
                                    shuttle.align_to_target = true;
                                    if angle_to_target_deg < 2.0 && !shuttle.tracking_braking {
                                        shuttle.autopilot_phase = AutopilotPhase::Accelerating;
                                    }
                                }
                                AutopilotPhase::Accelerating | AutopilotPhase::Cruising => {
                                    shuttle.align_to_target = true;

                                    // Tight lead buffer (1.5 frames) guarantees ship drops out of warp right at target_arrival_dist
                                    let lead_buffer = if config.is_demo_mode { 20.0 } else { config.c * dt_frame * 1.5 };

                                    // Overshoot detection: target has passed behind the ship at high speed,
                                    // or ship reached the target arrival zone and distance is opening up.
                                    let is_overshot = (cos_angle < -0.2 && nav.relative_speed >= 250.0)
                                        || (remaining_dist <= target_arrival_dist * 2.0 && nav.radial_velocity > 100.0);

                                    if remaining_dist <= s_brake + lead_buffer || is_overshot {
                                        shuttle.autopilot_phase = AutopilotPhase::EmergencyBraking;
                                        ap_forward_input = 0.0;
                                        ap_boost = false;
                                        ap_braking = true;
                                        info!(
                                            "[HYPERDRIVE] Dropping from warp to sublight | Brake dampers active (k=4.0) | Rem dist: {} | s_brake: {} | Speed: {:.1} km/s",
                                            crate::navigation::format_nav_distance(remaining_dist),
                                            crate::navigation::format_nav_distance(s_brake),
                                            nav.relative_speed / 1000.0
                                        );
                                    } else {
                                        ap_forward_input = 1.0;
                                        ap_boost = true;
                                        if rel_state.beta > 0.85 {
                                            shuttle.autopilot_phase = AutopilotPhase::Cruising;
                                        }
                                        if periodic_log && in_autonomous_flight {
                                            info!(
                                                "[HYPERDRIVE] Cruising at warp | Rem dist: {} | RelSpeed: {:.1} km/s | β: {:.6} | γ: {:.2}",
                                                crate::navigation::format_nav_distance(remaining_dist),
                                                nav.relative_speed / 1000.0,
                                                rel_state.beta,
                                                rel_state.gamma
                                            );
                                        }
                                    }
                                }
                                AutopilotPhase::EmergencyBraking => {
                                    shuttle.align_to_target = true;
                                    ap_braking = true; // Engage exact 'X' dampers (k = 4.0)
                                    shuttle.brake_to_universal = false; // Brake relative to target body!

                                    // Smoothly decelerate relative to target body:
                                    if nav.relative_speed < 15.0 || (current_rapidity < 1e-4 && current_speed < 15.0) {
                                        let arrival_tolerance = if config.is_demo_mode { 80.0 } else { 0.2 * config.c };
                                        if remaining_dist <= arrival_tolerance {
                                            shuttle.autopilot_phase = AutopilotPhase::Arrived;
                                        } else {
                                            // Overshot or deflected: re-orient to target and re-engage Hyperdrive!
                                            initiate_target_alignment(&mut shuttle, &transform);
                                            shuttle.autopilot_phase = AutopilotPhase::Aligning;
                                        }
                                    }
                                }
                                AutopilotPhase::Arrived => {
                                    // Parked safely in orbit: match target orbital velocity and disengage autopilot
                                    shuttle.autopilot_mode = AutopilotMode::Off;
                                    shuttle.autopilot_phase = AutopilotPhase::Idle;
                                    shuttle.align_to_target = false;
                                    shuttle.braking_active = false;
                                    ap_forward_input = 0.0;
                                    ap_boost = false;
                                    ap_braking = false;
                                    let target_vel = if let Some(target_ent) = nav.target_entity {
                                        if let Ok((_, _, target_vel_opt, _)) = target_query.get(target_ent) {
                                            target_vel_opt.map(|v| v.0).unwrap_or(DVec3::ZERO)
                                        } else {
                                            DVec3::ZERO
                                        }
                                    } else {
                                        DVec3::ZERO
                                    };
                                    rel_state.rapidity = crate::relativity::rapidity_from_velocity(target_vel, config.c);
                                    rel_state.update_from_rapidity(config.c);
                                    shuttle.angular_velocity = Vec3::ZERO;
                                    info!(
                                        "[HYPERDRIVE] Orbit insertion complete! Arrived at {} | Dist: {}",
                                        nav.target_name,
                                        crate::navigation::format_nav_distance(dist)
                                    );
                                }
                                _ => {}
                            }
                        }
                        AutopilotMode::Lorentz => {
                            // LORENTZ: Realistic Relativistic Brachistochrone trajectory with closed-loop retro-burn.
                            let proper_accel = if config.is_demo_mode {
                                600.0 // Full boost acceleration in Demo mode
                            } else {
                                25_000_000.0 // Full relativistic drive acceleration in Solar mode
                            };

                            let rel_speed = nav.relative_speed;
                            // Use exact rel_state.gamma to eliminate floating-point beta saturation (prevents gamma capping at 2236)
                            let rel_gamma = if rel_state.gamma > 1.0 && rel_speed > 0.5 * config.c {
                                rel_state.gamma
                            } else {
                                let beta = (rel_speed / config.c).clamp(0.0, 0.999999999999999);
                                1.0 / (1.0 - beta * beta).sqrt()
                            };
                            let d_stop = (config.c * config.c * (rel_gamma - 1.0)) / proper_accel;
                            // In proper time, turnover flip at 50 deg/s takes ~4.05 s.
                            // We use a safe margin (5.0s proper time) so the flip ALWAYS finishes BEFORE the required braking point!
                            let t_flip_proper = 5.0;
                            let d_flip = rel_gamma * rel_speed * t_flip_proper;

                            // Discrete frame lead buffer: accounts for relativistic coordinate distance traversed
                            // in 1 frame (dt_coord = gamma * dt_frame) to eliminate sampling overshoot at v -> c.
                            let frame_lead = rel_gamma.min(500.0) * rel_speed * dt_frame;
                            // Braking lead margin: 2% of stopping distance + sublight buffer.
                            // Initiates retro-burn slightly sooner than theoretical 1D stopping distance,
                            // absorbing steering cosine variance over interstellar distances.
                            // If velocity burns off ahead of schedule, the closed-loop controller cuts thrust
                            // and coasts in free fall until remaining distance catches up!
                            let lead_margin = if config.is_demo_mode {
                                30.0
                            } else {
                                (d_stop * 0.02).max(0.15 * config.c)
                            };

                            match shuttle.autopilot_phase {
                                AutopilotPhase::PreBraking => {}
                                AutopilotPhase::Aligning => {
                                    shuttle.align_to_target = true;
                                    let approach_window = if config.is_demo_mode { 200.0 } else { 3.0 * config.c };
                                    if remaining_dist <= approach_window && nav.relative_speed < 10_000.0 {
                                        // Already in close approach window at sublight speed: transition directly to TerminalApproach
                                        initiate_target_alignment(&mut shuttle, &transform);
                                        shuttle.autopilot_phase = AutopilotPhase::TerminalApproach;
                                    } else if angle_to_target_deg < 2.0 && !shuttle.tracking_braking {
                                        shuttle.autopilot_phase = AutopilotPhase::Accelerating;
                                    }
                                }
                                AutopilotPhase::Accelerating => {
                                    shuttle.align_to_target = true;
                                    let arrival_tolerance = if config.is_demo_mode { 50.0 } else { 0.15 * config.c };
                                    let is_overshot = (cos_angle < -0.2 && nav.relative_speed >= 250.0)
                                        || (remaining_dist <= target_arrival_dist * 2.0 && nav.radial_velocity > 100.0);

                                    if remaining_dist <= arrival_tolerance {
                                        shuttle.autopilot_phase = AutopilotPhase::Arrived;
                                        ap_forward_input = 0.0;
                                        ap_boost = false;
                                        ap_braking = true;
                                    } else if remaining_dist <= d_stop + d_flip + frame_lead + lead_margin || is_overshot {
                                        shuttle.autopilot_phase = AutopilotPhase::Flipping;
                                        info!(
                                            "[LORENTZ] Midcourse turnover flip initiated (180°) | Rem dist: {} | d_stop: {} | d_flip: {} | RelSpeed: {:.1} km/s | γ: {:.1}",
                                            crate::navigation::format_nav_distance(remaining_dist),
                                            crate::navigation::format_nav_distance(d_stop),
                                            crate::navigation::format_nav_distance(d_flip),
                                            nav.relative_speed / 1000.0,
                                            rel_gamma
                                        );
                                    } else {
                                        ap_forward_input = 1.0;
                                        ap_boost = true;
                                        if periodic_log && in_autonomous_flight {
                                            info!(
                                                "[LORENTZ] Accelerating | Rem dist: {} | RelSpeed: {:.1} km/s | β: {:.6} | γ: {:.2}",
                                                crate::navigation::format_nav_distance(remaining_dist),
                                                nav.relative_speed / 1000.0,
                                                rel_speed / config.c,
                                                rel_gamma
                                            );
                                        }
                                    }
                                }
                                AutopilotPhase::Flipping => {
                                    ap_forward_input = 0.0;
                                    ap_boost = false;
                                    shuttle.align_to_target = true;
                                    // Tracking system turns towards physical retrograde (-rel_state.velocity)
                                    // Once rotation reaches < 1.5° of retrograde, it transitions to Decelerating
                                }
                                AutopilotPhase::Decelerating => {
                                    shuttle.align_to_target = true;

                                    let is_overshot = !nav.closing && nav.radial_velocity > 50.0;
                                    let retro_trigger = d_stop + frame_lead + lead_margin;

                                    // CLOSED-LOOP ROCKET BRAKING CONTROLLER:
                                    // If remaining distance is greater than the required stopping distance,
                                    // the craft is ahead of schedule: coast in free fall facing retrograde!
                                    // Only engage retro-burn when distance reaches the exact required stopping runway.
                                    let need_burn = remaining_dist <= retro_trigger || is_overshot;

                                    if need_burn {
                                        if angle_to_retro_deg < 25.0 {
                                            if config.is_demo_mode || nav.relative_speed > 25.0 {
                                                ap_forward_input = 1.0;
                                                ap_boost = true;
                                                ap_braking = false;
                                            } else {
                                                // Near complete stop relative to target: apply dampers to zero residual motion
                                                ap_forward_input = 0.0;
                                                ap_boost = false;
                                                ap_braking = true;
                                            }
                                        } else {
                                            // Slewing nose toward retrograde
                                            ap_forward_input = 0.0;
                                            ap_boost = false;
                                            if is_overshot {
                                                ap_braking = true; // Slam dampers while slewing to retrograde during overshoot
                                            }
                                        }
                                    } else {
                                        // Ahead of schedule: coast in free fall facing retrograde towards target!
                                        ap_forward_input = 0.0;
                                        ap_boost = false;
                                        ap_braking = false;
                                    }

                                    // Diagnostic logging for retro-burn vs free-fall state changes and periodic updates
                                    if need_burn != shuttle.last_need_burn || (periodic_log && in_autonomous_flight) {
                                        shuttle.last_need_burn = need_burn;
                                        if need_burn {
                                            info!(
                                                "[LORENTZ] Retro-burn firing | Rem dist: {} | d_stop: {} | RelSpeed: {:.1} km/s | β: {:.6} | γ: {:.2}",
                                                crate::navigation::format_nav_distance(remaining_dist),
                                                crate::navigation::format_nav_distance(d_stop),
                                                nav.relative_speed / 1000.0,
                                                rel_speed / config.c,
                                                rel_gamma,
                                            );
                                        } else {
                                            let delta_dist = remaining_dist - d_stop;
                                            info!(
                                                "[LORENTZ] Free-fall retro-coasting | Rem dist: {} | d_stop: {} (excess: {}) | RelSpeed: {:.1} km/s",
                                                crate::navigation::format_nav_distance(remaining_dist),
                                                crate::navigation::format_nav_distance(d_stop),
                                                crate::navigation::format_nav_distance(delta_dist),
                                                nav.relative_speed / 1000.0,
                                            );
                                        }
                                    }

                                    let approach_window = if config.is_demo_mode { 200.0 } else { 3.0 * config.c };
                                    if remaining_dist <= approach_window && nav.relative_speed < 10_000.0 {
                                        initiate_target_alignment(&mut shuttle, &transform);
                                        shuttle.autopilot_phase = AutopilotPhase::TerminalApproach;
                                        info!(
                                            "[LORENTZ] Entering terminal approach | Rem dist: {} | RelSpeed: {:.1} km/s",
                                            crate::navigation::format_nav_distance(remaining_dist),
                                            nav.relative_speed / 1000.0
                                        );
                                    } else if (remaining_dist > approach_window || is_overshot) && nav.relative_speed < 500.0 {
                                        // Stopped outside approach window or finished stopping after overshoot:
                                        // re-orient to target and re-engage Lorentz!
                                        initiate_target_alignment(&mut shuttle, &transform);
                                        shuttle.autopilot_phase = AutopilotPhase::Aligning;
                                        info!(
                                            "[LORENTZ] Repositioning burn needed | Rem dist: {} | Re-engaging alignment",
                                            crate::navigation::format_nav_distance(remaining_dist)
                                        );
                                    }
                                }
                                AutopilotPhase::TerminalApproach => {
                                    shuttle.align_to_target = true;
                                    let arrival_tolerance = if config.is_demo_mode { 50.0 } else { 0.15 * config.c };

                                    if remaining_dist <= arrival_tolerance {
                                        ap_forward_input = 0.0;
                                        ap_boost = false;
                                        ap_braking = true;
                                        // Wait for craft to face target forward and damp out all residual motion
                                        if angle_to_target_deg < 5.0 && !shuttle.tracking_braking && (nav.relative_speed < 15.0 || (current_rapidity < 1e-4 && current_speed < 15.0)) {
                                            shuttle.autopilot_phase = AutopilotPhase::Arrived;
                                        }
                                    } else if angle_to_target_deg < 25.0 {
                                        let target_v = if config.is_demo_mode {
                                            ((remaining_dist * 2.0).sqrt() * 5.0).clamp(10.0, 200.0)
                                        } else {
                                            ((remaining_dist * 200_000.0).sqrt()).clamp(50.0, 0.05 * config.c)
                                        };

                                        let current_v = nav.relative_speed;
                                        if current_v < target_v * 0.85 {
                                            ap_forward_input = 1.0;
                                            ap_boost = remaining_dist > 0.05 * config.c;
                                            ap_braking = false;
                                        } else if current_v > target_v * 1.15 {
                                            ap_forward_input = 0.0;
                                            ap_boost = false;
                                            ap_braking = true;
                                        } else {
                                            ap_forward_input = 0.0;
                                            ap_boost = false;
                                            ap_braking = false;
                                        }
                                    } else {
                                        // Waiting for rotation to align with target
                                        ap_forward_input = 0.0;
                                        ap_boost = false;
                                        ap_braking = true;
                                    }

                                    if periodic_log {
                                        let action_str = if ap_braking {
                                            "Braking"
                                        } else if ap_boost {
                                            "Boost"
                                        } else if ap_forward_input > 0.0 {
                                            "Thrust"
                                        } else {
                                            "Coasting"
                                        };
                                        info!(
                                            "[LORENTZ] Terminal approach glide | Rem dist: {} | Speed: {:.1} km/s | Action: {}",
                                            crate::navigation::format_nav_distance(remaining_dist),
                                            nav.relative_speed / 1000.0,
                                            action_str
                                        );
                                    }
                                }
                                AutopilotPhase::Arrived => {
                                    // Parked safely in orbit: match target orbital velocity and disengage autopilot
                                    shuttle.autopilot_mode = AutopilotMode::Off;
                                    shuttle.autopilot_phase = AutopilotPhase::Idle;
                                    shuttle.align_to_target = false;
                                    shuttle.braking_active = false;
                                    ap_forward_input = 0.0;
                                    ap_boost = false;
                                    ap_braking = false;
                                    let target_vel = if let Some(target_ent) = nav.target_entity {
                                        if let Ok((_, _, target_vel_opt, _)) = target_query.get(target_ent) {
                                            target_vel_opt.map(|v| v.0).unwrap_or(DVec3::ZERO)
                                        } else {
                                            DVec3::ZERO
                                        }
                                    } else {
                                        DVec3::ZERO
                                    };
                                    rel_state.rapidity = crate::relativity::rapidity_from_velocity(target_vel, config.c);
                                    rel_state.update_from_rapidity(config.c);
                                    shuttle.angular_velocity = Vec3::ZERO;
                                    info!(
                                        "[LORENTZ] Orbit insertion complete! Arrived at {} | Dist: {}",
                                        nav.target_name,
                                        crate::navigation::format_nav_distance(dist)
                                    );
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if shuttle.autopilot_mode != shuttle.prev_autopilot_mode
            || shuttle.autopilot_phase != shuttle.prev_autopilot_phase
        {
            let target_str = if shuttle.autopilot_mode != AutopilotMode::Off {
                format!("{} [{}]", nav.target_name, crate::navigation::format_nav_distance(nav.distance_m))
            } else {
                "None".to_string()
            };
            info!(
                "[AUTOPILOT] Mode: {:?} | Phase: {:?} -> {:?} | Target: {} | RelSpeed: {:.1} km/s | β: {:.6} | γ: {:.2}",
                shuttle.autopilot_mode,
                shuttle.prev_autopilot_phase,
                shuttle.autopilot_phase,
                target_str,
                nav.relative_speed / 1000.0,
                rel_state.beta,
                rel_state.gamma,
            );
            shuttle.prev_autopilot_mode = shuttle.autopilot_mode;
            shuttle.prev_autopilot_phase = shuttle.autopilot_phase;
        }

        // --- CRUISE CONTROL / AUTOPILOT THRUSTER LOCKS ---
        let mouse_left_held = is_cursor_locked && mouse_buttons.pressed(MouseButton::Left);
        let mouse_right_held = is_cursor_locked && mouse_buttons.pressed(MouseButton::Right);
        let mouse_left_clicked = is_cursor_locked && mouse_buttons.just_pressed(MouseButton::Left);
        let mouse_right_clicked = is_cursor_locked && mouse_buttons.just_pressed(MouseButton::Right);

        // Ctrl + Z or Ctrl + Left Click: Forward Cruise Lock
        if ctrl_pressed && (keyboard.just_pressed(KeyCode::KeyW) || mouse_left_clicked) {
            shuttle.locked_thrust_dir = 1.0;
            shuttle.locked_boost = shift_pressed;
        }

        // Ctrl + S or Ctrl + Right Click: Reverse Cruise Lock
        if ctrl_pressed && (keyboard.just_pressed(KeyCode::KeyS) || mouse_right_clicked) {
            shuttle.locked_thrust_dir = -1.0;
            shuttle.locked_boost = shift_pressed;
        }

        // Cancel lock if user taps Z, S, Left Click, Right Click, or X without Ctrl
        if !ctrl_pressed && (
            keyboard.just_pressed(KeyCode::KeyW)
            || keyboard.just_pressed(KeyCode::KeyS)
            || keyboard.just_pressed(KeyCode::KeyX)
            || mouse_left_clicked
            || mouse_right_clicked
        ) {
            shuttle.locked_thrust_dir = 0.0;
            shuttle.locked_boost = false;
        }

        // Determine forward/reverse burn and boost state
        let forward_input = if in_autonomous_flight {
            shuttle.boost_active = ap_boost;
            shuttle.braking_active = ap_braking;
            ap_forward_input
        } else if shuttle.locked_thrust_dir != 0.0 {
            shuttle.boost_active = shuttle.locked_boost || (shift_pressed && shuttle.locked_thrust_dir != 0.0);
            shuttle.locked_thrust_dir
        } else {
            let mut f_in = 0.0f32;
            if keyboard.pressed(KeyCode::KeyW) || mouse_left_held { // Physical 'Z' on AZERTY or Left Click
                f_in += 1.0;
            }
            if keyboard.pressed(KeyCode::KeyS) || mouse_right_held { // Physical 'S' on AZERTY or Right Click
                f_in -= 1.0;
            }
            f_in
        };
        shuttle.thruster_forward = forward_input;

        let strafe_input = if in_autonomous_flight {
            0.0f32
        } else {
            let mut s_in = 0.0f32;
            if keyboard.pressed(KeyCode::KeyD) { // Physical 'D' on AZERTY
                s_in += 1.0;
            }
            if keyboard.pressed(KeyCode::KeyA) { // Physical 'Q' on AZERTY
                s_in -= 1.0;
            }
            s_in
        };
        shuttle.thruster_strafe = strafe_input;

        let vertical_input = if in_autonomous_flight {
            0.0f32
        } else {
            let mut v_in = 0.0f32;
            if keyboard.pressed(KeyCode::Space) {
                v_in += 1.0;
            }
            if keyboard.pressed(KeyCode::KeyC) {
                v_in -= 1.0;
            }
            v_in
        };
        shuttle.thruster_vertical = vertical_input;

        if !in_autonomous_flight && shuttle.locked_thrust_dir == 0.0 {
            let any_thrust = forward_input != 0.0 || strafe_input != 0.0 || vertical_input != 0.0;
            shuttle.boost_active = shift_pressed && any_thrust;
        }

        // --- ROTATIONAL / ATTITUDE INPUTS ---
        let is_landed = shuttle.landed_body.is_some();
        let (mouse_pitch, mouse_yaw) = if is_free_look {
            (0.0f32, 0.0f32)
        } else if is_landed {
            (0.0f32, -mouse_delta.x * shuttle.mouse_sensitivity.x)
        } else {
            (
                -mouse_delta.y * shuttle.mouse_sensitivity.y,
                -mouse_delta.x * shuttle.mouse_sensitivity.x,
            )
        };

        let mut key_pitch = 0.0f32;
        let mut key_yaw = 0.0f32;
        let mut key_roll = 0.0f32;

        if !in_autonomous_flight {
            if !is_landed {
                if keyboard.pressed(KeyCode::ArrowUp) {
                    key_pitch += 1.8;
                }
                if keyboard.pressed(KeyCode::ArrowDown) {
                    key_pitch -= 1.8;
                }
            }
            if keyboard.pressed(KeyCode::ArrowLeft) {
                key_yaw += 1.8;
            }
            if keyboard.pressed(KeyCode::ArrowRight) {
                key_yaw -= 1.8;
            }
            if !is_landed {
                if keyboard.pressed(KeyCode::KeyQ) { // Physical 'A' on AZERTY
                    key_roll += 2.8;
                }
                if keyboard.pressed(KeyCode::KeyE) { // Physical 'E' on AZERTY
                    key_roll -= 2.8;
                }
            }
        }

        if is_landed {
            shuttle.angular_velocity.x = 0.0;
            shuttle.angular_velocity.z = 0.0;
            shuttle.mouse_accum.x = 0.0;
        }

        // --- ANGULAR DYNAMICS (100% Frame-Rate Independent) ---
        let has_manual_rot = mouse_yaw.abs() > 0.0001
            || mouse_pitch.abs() > 0.0001
            || key_pitch != 0.0
            || key_yaw != 0.0
            || key_roll != 0.0;

        if has_manual_rot && !in_autonomous_flight {
            shuttle.align_to_target = false;
            shuttle.tracking_braking = false;
            shuttle.tracking_brake_to_zero = false;
            if shuttle.autopilot_mode == AutopilotMode::AlignOnly {
                shuttle.autopilot_mode = AutopilotMode::Off;
                shuttle.autopilot_phase = AutopilotPhase::Idle;
            }
        }

        if shuttle.align_to_target {
            if let Some(target_ent) = nav.target_entity {
                if let Ok((target_upos, _, _, _)) = target_query.get(target_ent) {
                    if shuttle.tracking_braking {
                        // PHASE 1: Gentle RCS retro-thrust braking along CURRENT spin direction
                        let ang_speed = shuttle.angular_velocity.length();
                        let target_ang_speed = if shuttle.tracking_brake_to_zero {
                            0.0f32
                        } else {
                            TARGET_ALIGN_SPEED_DEG.to_radians()
                        };

                        if ang_speed > target_ang_speed + 0.02 {
                            let axis = shuttle.angular_velocity / ang_speed;
                            let brake_accel = 50.0f32.to_radians();
                            let new_speed = (ang_speed - brake_accel * dt_f32).max(target_ang_speed);
                            shuttle.angular_velocity = axis * new_speed;

                            let delta_rot = Quat::from_axis_angle(axis, new_speed * dt_f32);
                            transform.rotation = (transform.rotation * delta_rot).normalize();

                            if new_speed <= target_ang_speed + 0.02 {
                                shuttle.tracking_braking = false;
                                shuttle.angular_velocity = Vec3::ZERO;
                                shuttle.active_track_speed_deg = if shuttle.tracking_brake_to_zero {
                                    4.0
                                } else {
                                    TARGET_ALIGN_SPEED_DEG
                                };
                            }
                        } else {
                            shuttle.tracking_braking = false;
                            shuttle.angular_velocity = Vec3::ZERO;
                            shuttle.active_track_speed_deg = if shuttle.tracking_brake_to_zero {
                                4.0
                            } else {
                                TARGET_ALIGN_SPEED_DEG
                            };
                        }
                    } else {
                        // PHASE 2: Smooth roll-neutral steering toward target (or retrograde during flip/decel)
                        let to_target = target_upos.0 - upos.0;
                        let to_target_dist = to_target.length();
                        if to_target_dist > 1.0 {
                            let to_target_dir = (to_target / to_target_dist).as_vec3();
                            let is_retrograde = shuttle.autopilot_mode == AutopilotMode::Lorentz
                                && (shuttle.autopilot_phase == AutopilotPhase::Flipping
                                    || shuttle.autopilot_phase == AutopilotPhase::Decelerating);

                            // When relative speed is very small (< 25 m/s), cleanly align to -to_target_dir to prevent void spinning
                            let v_rel = nav.relative_velocity;
                            let retro_dir = if v_rel.length() > 25.0 {
                                (-v_rel.normalize()).as_vec3()
                            } else {
                                -to_target_dir
                            };

                            let dir = if is_retrograde { retro_dir } else { to_target_dir };

                            let target_rot = target_rotation_facing(
                                dir,
                                transform.up().as_vec3(),
                                transform.right().as_vec3(),
                            );

                            let diff = transform.rotation.inverse() * target_rot;
                            let angle = 2.0 * diff.xyz().length().atan2(diff.w.abs());
                            let angle_deg = angle.to_degrees();

                            // If flipping, transition to decelerating once rotation aligns to retrograde within 1.5 degrees
                            if shuttle.autopilot_phase == AutopilotPhase::Flipping && angle_deg < 1.5 {
                                shuttle.autopilot_phase = AutopilotPhase::Decelerating;
                            }

                            // 1. Time-based smooth acceleration towards target turn speed
                            let max_turn_speed = if shuttle.autopilot_phase == AutopilotPhase::Flipping {
                                50.0f32 // Snappy 50 deg/s flip
                            } else {
                                TARGET_ALIGN_SPEED_DEG
                            };

                            let accel = 50.0f32;
                            shuttle.active_track_speed_deg = (shuttle.active_track_speed_deg + accel * dt_f32).min(max_turn_speed);

                            // 2. Cubic Hermite ease-out deceleration when approaching target within 5.5 degrees
                            let decel_zone_deg = 5.5f32;
                            let min_tracking_speed = 10.0f32;
                            if angle_deg < decel_zone_deg {
                                let u = (angle_deg / decel_zone_deg).clamp(0.0, 1.0);
                                let smooth_poly = 3.0 * u * u - 2.0 * u * u * u;
                                let allowed_speed = min_tracking_speed + (max_turn_speed - min_tracking_speed) * smooth_poly;
                                shuttle.active_track_speed_deg = shuttle.active_track_speed_deg.min(allowed_speed);
                            }

                            let max_turn = shuttle.active_track_speed_deg.to_radians() * dt_f32;
                            if angle > max_turn && angle > 1e-5 {
                                let t = (max_turn / angle).min(1.0);
                                transform.rotation = transform.rotation.slerp(target_rot, t).normalize();
                            } else {
                                transform.rotation = target_rot;
                            }
                            shuttle.angular_velocity = Vec3::ZERO;
                        }
                    }
                } else {
                    shuttle.align_to_target = false;
                    shuttle.tracking_braking = false;
                }
            } else {
                shuttle.align_to_target = false;
                shuttle.tracking_braking = false;
            }
        } else if shuttle.flight_assist {
            // Rotational SAS:
            // 1. Keyboard angular velocity damping:
            // When arrow keys or roll keys are pressed, angular velocity accelerates towards key input.
            // When keys are released, angular velocity decelerates monotonically via exponential decay (ZERO rebound / bounce-back).
            let has_key_rot = key_pitch != 0.0 || key_yaw != 0.0 || key_roll != 0.0;
            if has_key_rot {
                let damp_rate = 6.0f32;
                let decay = (-damp_rate * dt_f32).exp();

                shuttle.angular_velocity.x = key_pitch + (shuttle.angular_velocity.x - key_pitch) * decay;
                shuttle.angular_velocity.y = key_yaw + (shuttle.angular_velocity.y - key_yaw) * decay;
                shuttle.angular_velocity.z = key_roll + (shuttle.angular_velocity.z - key_roll) * decay;
            } else {
                let damp_rate = 8.0f32;
                let decay = (-damp_rate * dt_f32).exp();
                shuttle.angular_velocity *= decay;
                if shuttle.angular_velocity.length() < 0.002 {
                    shuttle.angular_velocity = Vec3::ZERO;
                }
            }

            // Apply exact physical axis-angle rotation integration from keyboard angular velocity
            let ang_speed = shuttle.angular_velocity.length();
            if ang_speed > 1e-6 && dt_f32 > 0.0 {
                let axis = shuttle.angular_velocity / ang_speed;
                let delta_rot = Quat::from_axis_angle(axis, ang_speed * dt_f32);
                transform.rotation = (transform.rotation * delta_rot).normalize();
            }

            // 2. Mouse input buffering: physical rate-limiting strictly governed by simulation dt_f32.
            // Prevents bypassing slow motion with high-speed mouse flicks, and smoothly consumes
            // rotation forward without any overshoot or reversal.
            if mouse_pitch != 0.0 || mouse_yaw != 0.0 {
                shuttle.mouse_accum.x += mouse_pitch;
                shuttle.mouse_accum.y += mouse_yaw;
                shuttle.mouse_accum = shuttle.mouse_accum.clamp_length_max(45.0f32.to_radians());
            }

            let accum_len = shuttle.mouse_accum.length();
            if accum_len > 1e-5 && dt_f32 > 0.0 {
                let max_mouse_turn_rate = 2.5f32; // rad/s (~143 deg/s)
                let max_step = max_mouse_turn_rate * dt_f32;
                let step_len = accum_len.min(max_step);
                let dir = shuttle.mouse_accum / accum_len;
                let step = dir * step_len;
                shuttle.mouse_accum -= step;

                let rot_yaw = Quat::from_rotation_y(step.y);
                let rot_pitch = Quat::from_rotation_x(step.x);
                transform.rotation = (transform.rotation * rot_yaw * rot_pitch).normalize();
            }
        } else {
            // RAW SAS OFF: Newtonian rotational inertia (tumbling)
            // Mouse imparts rotational torque acceleration (scaled by dt_f32)
            let mouse_accel_scale = 35.0f32;
            shuttle.angular_velocity.x += mouse_pitch * mouse_accel_scale * (dt_f32 * 60.0).min(1.0);
            shuttle.angular_velocity.y += mouse_yaw * mouse_accel_scale * (dt_f32 * 60.0).min(1.0);

            // Keyboard imparts continuous acceleration
            shuttle.angular_velocity.x += key_pitch * 2.5 * dt_f32;
            shuttle.angular_velocity.y += key_yaw * 2.5 * dt_f32;
            shuttle.angular_velocity.z += key_roll * 3.5 * dt_f32;

            // Apply exact physical axis-angle rotation integration
            let ang_speed = shuttle.angular_velocity.length();
            if ang_speed > 1e-6 {
                let axis = shuttle.angular_velocity / ang_speed;
                let delta_rot = Quat::from_axis_angle(axis, ang_speed * dt_f32);
                transform.rotation = (transform.rotation * delta_rot).normalize();
            }
        }

        // --- ANGULAR SPEED COMPUTATION (Exact physical rotation rate) ---
        // Uses double precision atan2 to eliminate float precision loss near acos(1.0)
        if dt_frame > 1e-6 {
            let q_curr = quat_to_dquat(transform.rotation);
            let q_prev = quat_to_dquat(shuttle.prev_rotation);
            let rot_diff = q_curr * q_prev.inverse();
            let v = DVec3::new(rot_diff.x, rot_diff.y, rot_diff.z);
            let angle_rad = 2.0 * v.length().atan2(rot_diff.w.abs());
            let inst_deg_s = (angle_rad / dt_frame).to_degrees() as f32;

            // Frame-rate independent exponential smoothing (time constant ~0.12s)
            let alpha = (1.0 - (-8.0 * dt_f32).exp()).clamp(0.0, 1.0);
            shuttle.smoothed_angular_speed_deg =
                shuttle.smoothed_angular_speed_deg * (1.0 - alpha) + inst_deg_s * alpha;

            if inst_deg_s < 0.05 && shuttle.smoothed_angular_speed_deg < 0.05 {
                shuttle.smoothed_angular_speed_deg = 0.0;
            }
        }
        shuttle.prev_rotation = transform.rotation;

        // --- PROPER ACCELERATION SCALING ---
        let proper_accel = if config.is_demo_mode {
            if shuttle.boost_active { 600.0 } else { 120.0 }
        } else {
            if shuttle.boost_active {
                25_000_000.0
            } else {
                // Check closest celestial body altitude for fine landing RCS throttle
                let mut min_surface_alt = f64::MAX;
                for (_, b_upos, b_body, _, _, b_tf_opt, b_ellipsoid_opt) in celestial_query.iter() {
                    if b_body.physical_radius > 0.0 {
                        let rel = upos.0 - b_upos.0;
                        let d = rel.length();
                        let effective_r = if let (Some(ellipsoid), Some(b_tf)) = (b_ellipsoid_opt, b_tf_opt) {
                            let normal = if d > 0.001 { rel / d } else { DVec3::Y };
                            let local_dir = b_tf.rotation.as_dquat().inverse() * normal;
                            ellipsoid.surface_radius_in_local_dir(local_dir)
                        } else {
                            b_body.physical_radius
                        };
                        let alt = d - effective_r;
                        if alt < min_surface_alt {
                            min_surface_alt = alt;
                        }
                    }
                }

                // If within 200 km of a planetary surface, scale smoothly down to fine landing thrust (30 m/s² ~ 3 Gs)
                // so manual touchdown maneuvers under 1-2 m/s are effortless and precise.
                if min_surface_alt < 200_000.0 {
                    let t = (min_surface_alt.max(0.0) / 200_000.0).clamp(0.0, 1.0);
                    30.0 + (2_000.0 - 30.0) * t
                } else {
                    2_000.0
                }
            }
        };

        let local_forward = transform.forward().as_dvec3();
        let local_right = transform.right().as_dvec3();
        let local_up = transform.up().as_dvec3();

        let mut thrust_dir = DVec3::ZERO;
        thrust_dir += local_forward * (forward_input as f64);
        thrust_dir += local_right * (strafe_input as f64);
        thrust_dir += local_up * (vertical_input as f64);

        // Check active user thrust input (forward/reverse, vertical, strafe)
        let was_landed = shuttle.landed_body.is_some();
        let user_active_thrust = forward_input.abs() > 0.01 || strafe_input.abs() > 0.01 || vertical_input > 0.01;
        if user_active_thrust {
            shuttle.landed_body = None;
        }

        // When resting near or on a celestial body:
        // - Space (vertical thruster) directly pushes upward along surface normal.
        // - W (forward thruster) when launching from the surface adds a gentle VTOL takeoff lift
        //   along the surface normal so either Space or W lifts off cleanly without requiring Shift.
        if vertical_input > 0.0 || (forward_input > 0.0 && was_landed) || forward_input > 0.0 {
            for (_, b_upos, b_body, _, _, b_tf_opt, b_ellipsoid_opt) in celestial_query.iter() {
                if b_body.physical_radius <= 0.0 {
                    continue;
                }
                let rel = upos.0 - b_upos.0;
                let d = rel.length();
                let effective_r = if let (Some(ellipsoid), Some(b_tf)) = (b_ellipsoid_opt, b_tf_opt) {
                    let normal = if d > 0.001 { rel / d } else { DVec3::Y };
                    let local_dir = b_tf.rotation.as_dquat().inverse() * normal;
                    ellipsoid.surface_radius_in_local_dir(local_dir)
                } else {
                    b_body.physical_radius
                };
                if vertical_input > 0.0 && d <= effective_r + 50.0 {
                    let normal = if d > 0.001 { rel / d } else { DVec3::Y };
                    thrust_dir += normal * (vertical_input as f64 * 2.0);
                    break;
                } else if forward_input > 0.0 && (was_landed || d <= effective_r + 5.0) {
                    let normal = if d > 0.001 { rel / d } else { DVec3::Y };
                    thrust_dir += normal * (forward_input as f64 * 0.75);
                    break;
                }
            }
        }

        let d_tau = dt_frame * config.time_scale;

        if thrust_dir.length_squared() > 0.0 {
            thrust_dir = thrust_dir.normalize();
            let mut delta_rapidity = (proper_accel / config.c) * d_tau;

            // In Lorentz deceleration: if thrusters oppose velocity, clamp delta_rapidity
            // to target-relative rapidity so deceleration can NEVER overshoot into reverse velocity!
            let is_retro_burn = shuttle.autopilot_mode == AutopilotMode::Lorentz
                && shuttle.autopilot_phase == AutopilotPhase::Decelerating;
            if is_retro_burn {
                let rel_rapidity = crate::relativity::rapidity_from_velocity(nav.relative_velocity, config.c).length();
                if rel_rapidity > 0.0 {
                    delta_rapidity = delta_rapidity.min(rel_rapidity);
                }
            }

            rel_state.rapidity += thrust_dir * delta_rapidity;
            shuttle.current_proper_accel = proper_accel;
        } else {
            shuttle.current_proper_accel = 0.0;
        }

        // Active Emergency Retro-Brake (holding X), PreBraking, or Hyperdrive Warp Drop:
        // Decelerates relative to local reference frame (target body if locked, closest body if within 10x radius, or deep space)
        if shuttle.braking_active {
            let decay_k = 4.0; // Emergency dampers (identical across physical 'X', PreBraking, and Hyperdrive)
            let brake_factor = (-decay_k * d_tau).exp();
            shuttle.angular_velocity *= brake_factor as f32;

            let ref_vel = if shuttle.brake_to_universal {
                // Shift + X: Brake relative to the Sun (Universal Rest Frame)
                DVec3::ZERO
            } else if shuttle.autopilot_phase == AutopilotPhase::EmergencyBraking
                || shuttle.autopilot_phase == AutopilotPhase::TerminalApproach
                || shuttle.autopilot_phase == AutopilotPhase::Decelerating
            {
                // Destination arrival braking: decelerate relative to the locked target
                if let Some(target_ent) = nav.target_entity {
                    if let Ok((_, _, bv_opt, _)) = target_query.get(target_ent) {
                        bv_opt.map(|bv| bv.0).unwrap_or(DVec3::ZERO)
                    } else {
                        DVec3::ZERO
                    }
                } else {
                    DVec3::ZERO
                }
            } else {
                // Manual holding 'X' and PreBraking: brake relative to closest celestial body!
                // This ensures landing and maneuvering near a moon or planet works even if targeting a distant star/planet.
                let mut closest_dist = f64::MAX;
                let mut closest_v = DVec3::ZERO;
                for (_, b_upos, _, b_vel_opt, _, _, _) in celestial_query.iter() {
                    let d = (upos.0 - b_upos.0).length();
                    if d < closest_dist {
                        closest_dist = d;
                        closest_v = b_vel_opt.map(|bv| bv.0).unwrap_or(DVec3::ZERO);
                    }
                }
                closest_v
            };

            let v_rel = rel_state.velocity - ref_vel;
            let new_v_rel = v_rel * brake_factor;
            let new_v_shuttle = ref_vel + new_v_rel;
            rel_state.rapidity = crate::relativity::rapidity_from_velocity(new_v_shuttle, config.c);
        }

        // Dual Spacetime Integration: dt = γ * dτ (universe coordinate frame)
        let dt_coordinate = rel_state.gamma * d_tau;

        // --- RELATIVISTIC PSEUDO-NEWTONIAN GRAVITY (Paczyński–Wiita) ---
        let mut total_grav_accel = DVec3::ZERO;
        for (_, body_upos, body, _, _, _, _) in celestial_query.iter() {
            total_grav_accel += crate::relativity::paczynski_wiita_gravity(
                upos.0,
                body_upos.0,
                body.mass_kg,
                body.physical_radius,
                config.c,
            );
        }
        shuttle.current_grav_accel = total_grav_accel.length();

        // Integrate pseudo-Newtonian gravitational acceleration in coordinate time
        if shuttle.landed_body.is_none() && !shuttle.braking_active && total_grav_accel.length_squared() > 1e-12 {
            rel_state.rapidity = crate::relativity::integrate_relativistic_acceleration(
                rel_state.rapidity,
                total_grav_accel,
                dt_coordinate,
                config.c,
            );
        }

        // Recalculate relativistic quantities from rapidity
        rel_state.update_from_rapidity(config.c);

        // If resting / landed on a celestial body, solidly lock position and velocity to surface
        if let Some(landed_ent) = shuttle.landed_body {
            let mut found = false;
            for (b_ent, b_upos, b_body, b_vel_opt, b_rot_opt, b_tf_opt, b_ellipsoid_opt) in celestial_query.iter() {
                if b_ent == landed_ent {
                    found = true;
                    let b_vel = b_vel_opt.map(|bv| bv.0).unwrap_or(DVec3::ZERO);

                    let effective_r = if let (Some(ellipsoid), Some(_b_tf)) = (b_ellipsoid_opt, b_tf_opt) {
                        ellipsoid.surface_radius_in_local_dir(shuttle.landed_local_normal)
                    } else {
                        b_body.physical_radius
                    };

                    if let (Some(rot), Some(b_tf)) = (b_rot_opt, b_tf_opt) {
                        // Body-fixed local normal guarantees zero rotational drift between visual mesh and shuttle
                        let body_rot = b_tf.rotation.as_dquat();
                        let world_normal = (body_rot * shuttle.landed_local_normal).normalize();

                        upos.0 = b_upos.0 + world_normal * effective_r;

                        let polar_axis = (b_tf.rotation * Vec3::Y).as_dvec3().normalize();
                        let dtheta = rot.speed_rad_per_sec as f64 * dt_coordinate;
                        let rot_quat = Quat::from_axis_angle(polar_axis.as_vec3(), dtheta as f32);
                        transform.rotation = rot_quat * transform.rotation;
                        transform.rotation = align_shuttle_to_surface(transform.rotation, world_normal.as_vec3());

                        let tangential_v = polar_axis.cross(world_normal * effective_r) * rot.speed_rad_per_sec as f64;
                        let surface_vel = b_vel + tangential_v;
                        rel_state.velocity = surface_vel;
                        rel_state.rapidity = crate::relativity::rapidity_from_velocity(surface_vel, config.c);
                        rel_state.update_from_rapidity(config.c);
                    } else {
                        let body_rot = b_tf_opt.map(|tf| tf.rotation.as_dquat()).unwrap_or(DQuat::IDENTITY);
                        let world_normal = if shuttle.landed_local_normal.length_squared() > 0.5 {
                            (body_rot * shuttle.landed_local_normal).normalize()
                        } else {
                            let rel = upos.0 - b_upos.0;
                            if rel.length_squared() > 1e-6 { rel.normalize() } else { DVec3::Y }
                        };
                        upos.0 = b_upos.0 + world_normal * effective_r;
                        rel_state.velocity = b_vel;
                        rel_state.rapidity = crate::relativity::rapidity_from_velocity(b_vel, config.c);
                        rel_state.update_from_rapidity(config.c);
                        transform.rotation = align_shuttle_to_surface(transform.rotation, world_normal.as_vec3());
                    }

                    shuttle.current_grav_accel = 0.0;
                    shuttle.current_proper_accel = 0.0;
                    shuttle.angular_velocity.x = 0.0;
                    shuttle.angular_velocity.z = 0.0;
                    break;
                }
            }
            if !found {
                shuttle.landed_body = None;
            }
        } else {
            // Floating Origin: dx = v * dt
            upos.0 += rel_state.velocity * dt_coordinate;
        }

        // Terrain collision resolution & surface contact (inherits body orbital motion & normal contact force)
        for (body_ent, body_upos, body, body_vel_opt, b_rot_opt, b_tf_opt, b_ellipsoid_opt) in celestial_query.iter() {
            if body.physical_radius <= 0.0 {
                continue;
            }
            let rel_to_body = upos.0 - body_upos.0;
            let dist = rel_to_body.length();
            let normal = if dist > 0.001 { rel_to_body / dist } else { DVec3::Y };

            let effective_surface_r = if let (Some(ellipsoid), Some(b_tf)) = (b_ellipsoid_opt, b_tf_opt) {
                let local_dir = b_tf.rotation.as_dquat().inverse() * normal;
                ellipsoid.surface_radius_in_local_dir(local_dir)
            } else {
                body.physical_radius
            };

            // Check if on or inside the surface (with a small contact skin of 2.0 m)
            if dist <= effective_surface_r + 2.0 {
                let body_vel = body_vel_opt.map(|b| b.0).unwrap_or(DVec3::ZERO);
                let surface_vel = if let (Some(rot), Some(b_tf)) = (b_rot_opt, b_tf_opt) {
                    let polar_axis = (b_tf.rotation * Vec3::Y).as_dvec3().normalize();
                    let tangential_v = polar_axis.cross(normal * effective_surface_r) * rot.speed_rad_per_sec as f64;
                    body_vel + tangential_v
                } else {
                    body_vel
                };
                let v_rel = rel_state.velocity - surface_vel;
                let v_normal = v_rel.dot(normal);
                let v_tangent = v_rel - normal * v_normal;

                // If actively climbing away from the surface (e.g. lifting off with Space or W at 165 Hz),
                // do NOT slam the shuttle back down to effective_surface_r — allow smooth immediate liftoff!
                if dist >= effective_surface_r && (v_normal > 0.05 || (user_active_thrust && v_normal > 0.0)) {
                    shuttle.landed_body = None;
                    continue;
                }

                // Clamp shuttle position to surface if penetrating or landing/resting
                if dist < effective_surface_r || !user_active_thrust {
                    upos.0 = body_upos.0 + normal * effective_surface_r;
                }

                if v_normal < 0.0 {
                    let impact_speed = -v_normal;
                    if impact_speed > 75.0 {
                        // High-speed collision bounce (e = 0.45 restitution, 0.85 surface friction)
                        let new_v_normal = impact_speed * 0.45;
                        let new_v_tangent = v_tangent * 0.85;
                        let new_v_rel = new_v_tangent + normal * new_v_normal;
                        let new_v_shuttle = surface_vel + new_v_rel;
                        rel_state.rapidity = crate::relativity::rapidity_from_velocity(new_v_shuttle, config.c);
                        rel_state.update_from_rapidity(config.c);
                        shuttle.landed_body = None;
                        if shuttle.autopilot_mode != AutopilotMode::Off {
                            shuttle.autopilot_phase = AutopilotPhase::PreBraking;
                        }
                    } else if user_active_thrust {
                        // Active user thrust (forward/reverse/lateral): cancel normal penetration but preserve forward/tangential thrust
                        let new_v_shuttle = surface_vel + v_tangent;
                        rel_state.rapidity = crate::relativity::rapidity_from_velocity(new_v_shuttle, config.c);
                        rel_state.update_from_rapidity(config.c);
                        shuttle.landed_body = None;
                    } else {
                        // Gentle touchdown (< 75 m/s): lock to surface
                        if shuttle.landed_body.is_none() {
                            let body_rot = b_tf_opt.map(|tf| tf.rotation.as_dquat()).unwrap_or(DQuat::IDENTITY);
                            shuttle.landed_body = Some(body_ent);
                            shuttle.landed_local_normal = (body_rot.inverse() * normal).normalize();
                            transform.rotation = align_shuttle_to_surface(transform.rotation, normal.as_vec3());
                            shuttle.angular_velocity = Vec3::ZERO;
                        }
                        rel_state.velocity = surface_vel;
                        rel_state.rapidity = crate::relativity::rapidity_from_velocity(surface_vel, config.c);
                        rel_state.update_from_rapidity(config.c);
                    }
                } else if user_active_thrust {
                    shuttle.landed_body = None;
                } else {
                    // Resting on surface without thrust
                    if shuttle.landed_body.is_none() {
                        let body_rot = b_tf_opt.map(|tf| tf.rotation.as_dquat()).unwrap_or(DQuat::IDENTITY);
                        shuttle.landed_body = Some(body_ent);
                        shuttle.landed_local_normal = (body_rot.inverse() * normal).normalize();
                        transform.rotation = align_shuttle_to_surface(transform.rotation, normal.as_vec3());
                        shuttle.angular_velocity = Vec3::ZERO;
                    }
                    rel_state.velocity = surface_vel;
                    rel_state.rapidity = crate::relativity::rapidity_from_velocity(surface_vel, config.c);
                    rel_state.update_from_rapidity(config.c);
                }
                rel_state.update_from_rapidity(config.c);
            }
        }

        transform.translation = Vec3::ZERO;
    }
}

pub fn update_engine_plumes(
    shuttle_query: Query<(&Shuttle, &RelativisticState)>,
    pause: Res<SimulationPause>,
    mut plume_query: Query<(&EnginePlume, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
    slat_query: Query<&MeshMaterial3d<StandardMaterial>, With<EngineSlat>>,
    mut light_query: Query<(&EngineBayLight, &mut PointLight)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if pause.0 {
        return;
    }

    let Ok((shuttle, rel_state)) = shuttle_query.get_single() else {
        return;
    };

    let is_main_thrust = shuttle.thruster_forward > 0.0;
    let is_boosting = is_main_thrust && shuttle.boost_active;

    let burn_factor = if is_boosting {
        1.8 // Blinding afterburning torch flame on boost
    } else if is_main_thrust {
        1.0 // Standard burn
    } else {
        0.05 // Idle pilot glow at nozzle rim
    };

    // Relativistic spectral progression (Thermal combustion -> Plasma fusion -> Relativistic antimatter -> Gamma white):
    let beta = rel_state.beta;
    let (r, g, b) = if beta < 0.20 {
        // Warm golden amber / orange combustion flame
        (1.0, 0.50, 0.12)
    } else if beta < 0.60 {
        // High-energy plasma cyan / azure
        let t = ((beta - 0.20) / 0.40) as f32;
        (1.0 - t * 0.9, 0.50 + t * 0.35, 0.12 + t * 0.88)
    } else if beta < 0.92 {
        // Relativistic antimatter violet / magenta
        let t = ((beta - 0.60) / 0.32) as f32;
        (0.10 + t * 0.80, 0.85 - t * 0.65, 1.0)
    } else {
        // Blinding ultraviolet gamma-white
        (1.0, 1.0, 1.0)
    };

    let plume_intensity = if is_boosting { 8.0 } else if is_main_thrust { 3.5 } else { 0.4 };
    let slat_intensity = if is_boosting {
        ENGINE_SLAT_LUM_BOOST
    } else if is_main_thrust {
        ENGINE_SLAT_LUM_THRUST
    } else {
        ENGINE_SLAT_LUM_IDLE
    };

    // Update rectangular plumes
    for (plume, mut transform, material_handle) in plume_query.iter_mut() {
        let length = plume.flame_length * burn_factor;
        transform.scale.z = length;
        transform.translation.z = plume.nozzle_z + length / 2.0;

        if let Some(mat) = materials.get_mut(material_handle) {
            mat.base_color = Color::srgba(r, g, b, if is_main_thrust { 0.45 } else { 0.10 });
            mat.emissive = LinearRgba::rgb(r * plume_intensity, g * plume_intensity, b * plume_intensity);
        }
    }

    // Update 4 glowing slats per engine
    for material_handle in slat_query.iter() {
        if let Some(mat) = materials.get_mut(material_handle) {
            mat.base_color = Color::LinearRgba(LinearRgba::new(
                r * slat_intensity,
                g * slat_intensity,
                b * slat_intensity,
                1.0,
            ));
            mat.emissive = LinearRgba::rgb(
                r * slat_intensity,
                g * slat_intensity,
                b * slat_intensity,
            );
        }
    }

    // Update engine bay lights
    for (light_cfg, mut light) in light_query.iter_mut() {
        light.color = Color::srgb(r, g, b);
        let factor = if is_boosting { 2.5 } else if is_main_thrust { 1.0 } else { 0.15 };
        light.intensity = light_cfg.base_intensity * factor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecs_schedule_and_query_safety() {
        let mut app = App::new();
        app.insert_resource(RelativisticConfig::default());
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause::default());
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<Mesh>>();

        // Spawn dummy window, camera, shuttle, and nav target to test entity queries
        app.world_mut().spawn((
            Window::default(),
            bevy::window::PrimaryWindow,
        ));
        app.world_mut().spawn((
            SpaceCamera::default(),
            Camera::default(),
            Transform::default(),
            GlobalTransform::default(),
        ));
        app.world_mut().spawn((
            Shuttle::default(),
            RelativisticState::default(),
            UniversalPosition::default(),
            Transform::default(),
        ));
        app.world_mut().spawn((
            crate::celestial::NavTarget {
                name: "Test Target",
                classification: "Test",
                radius_m: 100.0,
            },
            crate::celestial::CelestialBody { physical_radius: 100.0, mass_kg: 0.0 },
            UniversalPosition::default(),
            Transform::default(),
        ));
        app.world_mut().spawn((
            crate::world::SunDirectionalLight,
            Transform::default(),
        ));

        app.add_systems(
            Update,
            (
                shuttle_input_system,
                (
                    crate::celestial::planet_orbit_system,
                    crate::celestial::satellite_orbit_system,
                ).chain(),
                crate::world::sun_light_tracking_system,
                crate::world::pulse_runway_lights_system,
                crate::world::configure_ship_materials_system,
                crate::ui::target_rolling_button_system,
                crate::ui::crosshair_hover_system,
                crate::camera::cursor_grab_system,
                crate::celestial::celestial_optics_system,
            ),
        );

        // app.update() triggers full schedule building and query validation
        app.update();
    }

    #[test]
    fn test_target_rotation_facing() {
        let current_up = Vec3::Y;
        let current_right = Vec3::X;

        // 1. Facing straight ahead (-Z in Bevy)
        let rot_fwd = target_rotation_facing(-Vec3::Z, current_up, current_right);
        let fwd = rot_fwd * -Vec3::Z;
        assert!((fwd - -Vec3::Z).length() < 1e-4);

        // 2. Facing straight backward (+Z)
        let rot_back = target_rotation_facing(Vec3::Z, current_up, current_right);
        let fwd_back = rot_back * -Vec3::Z;
        assert!((fwd_back - Vec3::Z).length() < 1e-4);

        // 3. Facing straight up (+Y)
        let rot_up = target_rotation_facing(Vec3::Y, current_up, current_right);
        let fwd_up = rot_up * -Vec3::Z;
        assert!((fwd_up - Vec3::Y).length() < 1e-4);

        // 4. Facing arbitrary diagonal
        let dir = Vec3::new(1.0, 2.0, -3.0).normalize();
        let rot_diag = target_rotation_facing(dir, current_up, current_right);
        let fwd_diag = rot_diag * -Vec3::Z;
        assert!((fwd_diag - dir).length() < 1e-4);
    }

    #[test]
    fn test_angular_speed_fps_invariance() {
        // Test that tumbling at 148.6 deg/s measures identically at 60 FPS and 600 FPS
        let speed_deg = 148.6f64;
        let speed_rad = speed_deg.to_radians();
        let axis = Vec3::new(0.3, 0.7, 0.6).normalize();

        // 60 FPS: dt = 1/60
        let dt_60 = 1.0 / 60.0;
        let delta_rot_60 = Quat::from_axis_angle(axis, (speed_rad * dt_60) as f32);
        let q_prev = Quat::IDENTITY;
        let q_curr_60 = delta_rot_60 * q_prev;
        let rot_diff_60 = quat_to_dquat(q_curr_60) * quat_to_dquat(q_prev).inverse();
        let v_60 = DVec3::new(rot_diff_60.x, rot_diff_60.y, rot_diff_60.z);
        let angle_60 = 2.0 * v_60.length().atan2(rot_diff_60.w.abs());
        let measured_deg_60 = (angle_60 / dt_60).to_degrees();

        // 600 FPS: dt = 1/600
        let dt_600 = 1.0 / 600.0;
        let delta_rot_600 = Quat::from_axis_angle(axis, (speed_rad * dt_600) as f32);
        let q_curr_600 = delta_rot_600 * q_prev;
        let rot_diff_600 = quat_to_dquat(q_curr_600) * quat_to_dquat(q_prev).inverse();
        let v_600 = DVec3::new(rot_diff_600.x, rot_diff_600.y, rot_diff_600.z);
        let angle_600 = 2.0 * v_600.length().atan2(rot_diff_600.w.abs());
        let measured_deg_600 = (angle_600 / dt_600).to_degrees();

        assert!((measured_deg_60 - speed_deg).abs() < 0.01);
        assert!((measured_deg_600 - speed_deg).abs() < 0.01);
        assert!((measured_deg_60 - measured_deg_600).abs() < 0.005);
    }

    #[test]
    fn test_angular_speed_invariant_to_time_scale() {
        // A shuttle rotating at 45 deg/s
        let turn_speed_deg = 45.0f64;
        let turn_speed_rad = turn_speed_deg.to_radians();
        let dt_frame = 1.0 / 60.0; // 60 FPS
        let axis = Vec3::Y;

        // Case 1: time_scale = 1.0
        let delta_rot_1 = Quat::from_axis_angle(axis, (turn_speed_rad * dt_frame) as f32);
        let rot_diff_1 = quat_to_dquat(delta_rot_1);
        let v1 = DVec3::new(rot_diff_1.x, rot_diff_1.y, rot_diff_1.z);
        let angle_1 = 2.0 * v1.length().atan2(rot_diff_1.w.abs());
        let inst_deg_1 = (angle_1 / dt_frame).to_degrees();

        // Case 2: time_scale = 2.0 (F7)
        // Since dt_f32 is proper real time, physical rotation is invariant to time_scale
        let delta_rot_2 = Quat::from_axis_angle(axis, (turn_speed_rad * dt_frame) as f32);
        let rot_diff_2 = quat_to_dquat(delta_rot_2);
        let v2 = DVec3::new(rot_diff_2.x, rot_diff_2.y, rot_diff_2.z);
        let angle_2 = 2.0 * v2.length().atan2(rot_diff_2.w.abs());
        let inst_deg_2 = (angle_2 / dt_frame).to_degrees();

        assert!((inst_deg_1 - turn_speed_deg).abs() < 1e-3);
        assert!((inst_deg_2 - turn_speed_deg).abs() < 1e-3);
        assert!((inst_deg_1 - inst_deg_2).abs() < 1e-5, "Angular speed must not double when time scale is 2x!");
    }

    #[test]
    fn test_slow_motion_scales_manual_steering() {
        // In slow-motion (time_scale = 0.1), sim_time_factor must scale steering proportionally
        let ts_slow = 0.1f64;
        let sim_time_factor_slow = (ts_slow as f32).clamp(0.001, 2.0);
        assert_eq!(sim_time_factor_slow, 0.1);

        // Above 1.0x (time_scale = 2.0), steering also scales with simulation speed
        let ts_warp = 2.0f64;
        let sim_time_factor_warp = (ts_warp as f32).clamp(0.001, 2.0);
        assert_eq!(sim_time_factor_warp, 2.0);
    }

    #[test]
    fn test_pause_mouse_sensitivity_adjustment() {
        let mut app = App::new();
        app.insert_resource(RelativisticConfig::default());
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(true)); // Paused!
        app.insert_resource(crate::navigation::NavigationComputer::default());
        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::NumpadAdd); // +20%
        app.insert_resource(keyboard);
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((
            Window::default(),
            bevy::window::PrimaryWindow,
        ));
        app.world_mut().spawn((
            SpaceCamera::default(),
            Camera::default(),
            Transform::default(),
            GlobalTransform::default(),
        ));
        let shuttle_entity = app.world_mut().spawn((
            Shuttle::default(),
            RelativisticState::default(),
            UniversalPosition::default(),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);

        let initial_sens = app.world().get::<Shuttle>(shuttle_entity).unwrap().mouse_sensitivity.x;
        app.update();
        let updated_sens = app.world().get::<Shuttle>(shuttle_entity).unwrap().mouse_sensitivity.x;

        // In pause mode, pressing NumpadAdd should still increase sensitivity by 20%
        assert!((updated_sens - initial_sens * 1.20).abs() < 1e-6);
    }

    #[test]
    fn test_cubic_smoothstep_curve() {
        let smoothstep = |u: f32| -> f32 {
            let u = u.clamp(0.0, 1.0);
            3.0 * u * u - 2.0 * u * u * u
        };

        // Boundary conditions
        assert_eq!(smoothstep(0.0), 0.0);
        assert_eq!(smoothstep(1.0), 1.0);
        assert_eq!(smoothstep(0.5), 0.5);

        // Slow start (first derivative starts at 0)
        assert!(smoothstep(0.1) < 0.05); // 0.1 gives 0.028

        // Slow arrival (smooth deceleration towards 1.0)
        assert!(smoothstep(0.9) > 0.95); // 0.9 gives 0.972
    }

    #[test]
    fn test_calculate_exponential_brake_stopping_distance() {
        let c_demo = 3000.0;
        let c_real = 299_792_458.0;
        let dt = 1.0 / 60.0;

        // At rest (theta = 0), stopping distance must be 0
        assert_eq!(calculate_exponential_brake_stopping_distance(c_demo, 0.0, 4.0, dt), 0.0);
        assert_eq!(calculate_exponential_brake_stopping_distance(c_real, 0.0, 4.0, dt), 0.0);

        // At beta = 0.5 (theta ~ 0.5493) in Demo mode with k = 4.0:
        let s_demo_05 = calculate_exponential_brake_stopping_distance(c_demo, 0.549306, 4.0, dt);
        assert!((s_demo_05 - 378.5).abs() < 5.0);

        // At beta = 0.99 (theta ~ 2.64665) in Demo mode with k = 4.0:
        let s_demo_99 = calculate_exponential_brake_stopping_distance(c_demo, 2.64665, 4.0, dt);
        assert!((s_demo_99 - 954.1).abs() < 10.0);

        // In Real c mode, stopping distance scales exactly with c
        let s_real_99 = calculate_exponential_brake_stopping_distance(c_real, 2.64665, 4.0, dt);
        assert!((s_real_99 - 95_340_000.0).abs() < 1_000_000.0); // ~0.318 light-seconds

        // At ultra-relativistic beta = 0.9999 (theta ~ 4.9517) in Real c mode with k = 4.0:
        let s_real_9999 = calculate_exponential_brake_stopping_distance(c_real, 4.9517, 4.0, dt);
        assert!((s_real_9999 - 97_815_000.0).abs() < 1_000_000.0); // ~0.326 light-seconds
    }

    #[test]
    fn test_target_arrival_distances() {
        let c = 299_792_458.0;
        let r_luna = 1_737_000.0;
        let r_jupiter = 69_911_000.0;
        let r_sun = 696_340_000.0;

        // Luna target arrival distance must be 0.1 ls
        let d_luna = target_arrival_distance(r_luna, c, false);
        assert!((d_luna - 0.10 * c).abs() < 1e-3);

        // Jupiter target arrival distance must be 3.2 ls (in 3.0 - 3.5 ls range)
        let d_jup = target_arrival_distance(r_jupiter, c, false);
        assert!((d_jup - 3.20 * c).abs() < 1e-3);

        // Sun target arrival distance must be ~5.5 ls
        let d_sun = target_arrival_distance(r_sun, c, false);
        assert!((d_sun - r_sun * 2.4).abs() < 1e-3);
    }

    #[test]
    fn test_autopilot_modes_and_defaults() {
        let mut shuttle = Shuttle::default();
        assert_eq!(shuttle.autopilot_mode, AutopilotMode::Off);
        assert_eq!(shuttle.autopilot_phase, AutopilotPhase::Idle);

        // Engage Hyperdrive mode
        shuttle.autopilot_mode = AutopilotMode::Hyperdrive;
        shuttle.autopilot_phase = AutopilotPhase::Aligning;
        assert_eq!(shuttle.autopilot_mode, AutopilotMode::Hyperdrive);
        assert_eq!(shuttle.autopilot_phase, AutopilotPhase::Aligning);

        // Switch to Lorentz mode
        shuttle.autopilot_mode = AutopilotMode::Lorentz;
        shuttle.autopilot_phase = AutopilotPhase::Accelerating;
        assert_eq!(shuttle.autopilot_mode, AutopilotMode::Lorentz);
        assert_eq!(shuttle.autopilot_phase, AutopilotPhase::Accelerating);

        // Terminal approach mode
        shuttle.autopilot_phase = AutopilotPhase::TerminalApproach;
        assert_eq!(shuttle.autopilot_phase, AutopilotPhase::TerminalApproach);
    }

    #[test]
    fn test_relativistic_brachistochrone_stopping_distance() {
        let c = 3000.0f64;
        let a = 600.0f64;
        let beta = 0.90f64;
        let gamma = 1.0 / (1.0 - beta * beta).sqrt(); // ~2.294157

        // Relativistic work-energy theorem: d_stop = c^2 * (gamma - 1) / a
        let d_stop = (c * c * (gamma - 1.0)) / a;
        assert!((d_stop - 19412.3).abs() < 10.0);
    }

    #[test]
    fn test_lorentz_flip_distance_accounts_for_rotation_duration() {
        let c: f64 = 299_792_458.0;
        let v: f64 = 0.90 * c;
        let gamma = 1.0 / (1.0 - 0.90 * 0.90f64).sqrt(); // ~2.294
        let t_flip: f64 = 5.0; // ~5.0s proper time for 180° flip at 50 deg/s
        let d_flip: f64 = gamma * v * t_flip;

        // In universe coordinate time (dilated by gamma), shuttle covers ~3.09 million km
        assert!(d_flip > 3_000_000_000.0);
    }

    #[test]
    fn test_initiate_target_alignment_triggers_tracking_braking() {
        let mut shuttle = Shuttle {
            smoothed_angular_speed_deg: 45.0, // Spinning fast!
            ..default()
        };
        let transform = Transform::IDENTITY;

        initiate_target_alignment(&mut shuttle, &transform);
        // Phase 1 must be active to brake existing spin smoothly instead of snapping to 0
        assert!(shuttle.tracking_braking);
        assert!(shuttle.tracking_brake_to_zero);
        assert!(shuttle.align_to_target);
    }

    #[test]
    fn test_lorentz_retrograde_anti_velocity_orientation() {
        // Shuttle moving in +Z direction (forward towards target)
        let velocity = DVec3::new(0.0, 0.0, 100_000.0);
        let to_target = DVec3::new(0.0, 0.0, 500_000.0); // Target is ahead

        let retro_dir = if velocity.length_squared() > 1.0 {
            (-velocity.normalize()).as_vec3()
        } else {
            -to_target.normalize().as_vec3()
        };
        assert!((retro_dir - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-6);

        // Shuttle overshot: target is now BEHIND (-Z), but shuttle is still flying +Z
        let to_target_behind = DVec3::new(0.0, 0.0, -500_000.0);
        let retro_dir_overshot = if velocity.length_squared() > 1.0 {
            (-velocity.normalize()).as_vec3()
        } else {
            -to_target_behind.normalize().as_vec3()
        };
        // Retrograde MUST point in -Z (opposing +Z velocity) to brake, NOT forward!
        assert!((retro_dir_overshot - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-6);
    }

    #[test]
    fn test_flight_assist_rotation_controls_active() {
        let mut app = App::new();
        app.insert_resource(RelativisticConfig::default());
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::KeyE); // Roll input
        app.insert_resource(keyboard);
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((
            Window::default(),
            bevy::window::PrimaryWindow,
        ));
        app.world_mut().spawn((
            SpaceCamera::default(),
            Camera::default(),
            Transform::default(),
            GlobalTransform::default(),
        ));
        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                flight_assist: true,
                align_to_target: false,
                ..default()
            },
            RelativisticState::default(),
            UniversalPosition::default(),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let shuttle = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        // Keyboard roll must be applied to angular_velocity when flight_assist is true!
        assert!(shuttle.angular_velocity.z.abs() > 0.01, "Roll angular velocity must not be zero!");
    }

    #[test]
    fn test_numpad_camera_controls() {
        let mut app = App::new();
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(RelativisticConfig::default());
        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::Numpad4); // Look Left
        app.insert_resource(keyboard);
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((
            Window::default(),
            bevy::window::PrimaryWindow,
        ));
        let cam_entity = app.world_mut().spawn((
            SpaceCamera::default(),
            Camera::default(),
            Transform::default(),
            GlobalTransform::default(),
        )).id();
        app.world_mut().spawn((
            Shuttle::default(),
            RelativisticState::default(),
            UniversalPosition::default(),
            Transform::default(),
        ));

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(50));
        app.update();

        let cam = app.world().get::<SpaceCamera>(cam_entity).unwrap();
        assert!(cam.free_look_active, "Numpad look must activate free look");
        assert!(cam.target_look_yaw > 0.05, "Numpad 4 must pan target_look_yaw left");

        // When pressing Numpad 0, rear_view_active must be true
        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.release(KeyCode::Numpad4);
        keyboard.press(KeyCode::Numpad0);
        app.update();
        let cam = app.world().get::<SpaceCamera>(cam_entity).unwrap();
        assert!(cam.rear_view_active, "Holding Numpad 0 must activate rear_view_active");

        // When releasing Numpad 0, rear_view_active must return to false
        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.release(KeyCode::Numpad0);
        app.update();
        let cam = app.world().get::<SpaceCamera>(cam_entity).unwrap();
        assert!(!cam.rear_view_active, "Releasing Numpad 0 must restore forward view");
    }

    #[test]
    fn test_gravity_pull_and_surface_contact_clamp() {
        let mut app = App::new();
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(RelativisticConfig {
            c: crate::relativity::REAL_C,
            time_scale: 1.0,
            is_demo_mode: false,
        });
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((
            Window::default(),
            bevy::window::PrimaryWindow,
        ));
        app.world_mut().spawn((
            SpaceCamera::default(),
            Camera::default(),
            Transform::default(),
            GlobalTransform::default(),
        ));

        // Earth at origin
        let earth_radius = 6_371_000.0;
        let earth_mass = 5.9722e24;
        app.world_mut().spawn((
            crate::celestial::CelestialBody {
                physical_radius: earth_radius,
                mass_kg: earth_mass,
            },
            UniversalPosition(DVec3::ZERO),
        ));

        // Shuttle in low Earth orbit at 7,000 km
        let shuttle_entity = app.world_mut().spawn((
            Shuttle::default(),
            RelativisticState::default(),
            UniversalPosition(DVec3::new(7_000_000.0, 0.0, 0.0)),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(50));
        app.update();

        let (shuttle, rel_state, upos) = (
            app.world().get::<Shuttle>(shuttle_entity).unwrap(),
            app.world().get::<RelativisticState>(shuttle_entity).unwrap(),
            app.world().get::<UniversalPosition>(shuttle_entity).unwrap(),
        );

        // Gravity at 7000 km should be g * (6371 / 7000)^2 ~ 8.13 m/s²
        assert!(shuttle.current_grav_accel > 7.5 && shuttle.current_grav_accel < 8.5,
            "Gravitational acceleration at 7000 km must be ~8.13 m/s², got {}", shuttle.current_grav_accel);

        // Shuttle should have gained negative X rapidity (towards Earth at origin)
        assert!(rel_state.rapidity.x < 0.0, "Shuttle must fall towards Earth");
        assert!(upos.0.x < 7_000_000.0, "Position must decrease along X towards Earth");

        // Surface collision clamp test: place shuttle slightly inside planet surface
        {
            let mut upos_mut = app.world_mut().get_mut::<UniversalPosition>(shuttle_entity).unwrap();
            upos_mut.0 = DVec3::new(earth_radius - 50.0, 0.0, 0.0); // 50m below surface
        }
        app.update();

        let upos_clamped = app.world().get::<UniversalPosition>(shuttle_entity).unwrap();
        assert!((upos_clamped.0.x - earth_radius).abs() < 1.0,
            "Shuttle should be safely clamped to surface at radius {}, got {}", earth_radius, upos_clamped.0.x);
    }

    #[test]
    fn test_collision_rebound_and_orbital_velocity_inheritance() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((
            Window::default(),
            bevy::window::PrimaryWindow,
        ));
        app.world_mut().spawn((
            SpaceCamera::default(),
            Camera::default(),
            Transform::default(),
            GlobalTransform::default(),
        ));

        // Moving celestial body (Moon at origin, orbiting at 30,000 m/s along Z)
        let moon_radius = 1_737_400.0;
        app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: moon_radius,
                mass_kg: 7.342e22,
            },
            crate::celestial::BodyVelocity(DVec3::new(0.0, 0.0, 30_000.0)),
        ));

        // Shuttle hitting surface along -X at 100 m/s relative, matching Z orbital velocity (30,000 m/s)
        let v_shuttle = DVec3::new(-100.0, 0.0, 30_000.0);
        let rap = crate::relativity::rapidity_from_velocity(v_shuttle, c);
        let shuttle_entity = app.world_mut().spawn((
            Shuttle::default(),
            RelativisticState {
                rapidity: rap,
                velocity: v_shuttle,
                ..default()
            },
            UniversalPosition(DVec3::new(moon_radius - 10.0, 0.0, 0.0)), // 10m inside surface
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let (upos, rel_state) = (
            app.world().get::<UniversalPosition>(shuttle_entity).unwrap(),
            app.world().get::<RelativisticState>(shuttle_entity).unwrap(),
        );

        // 1. Clamped to surface radius
        assert!((upos.0.x - moon_radius).abs() < 1.0, "Must be clamped to surface");

        // 2. Rebounded along +X with e = 0.45 -> ~45 m/s
        assert!(rel_state.velocity.x > 40.0 && rel_state.velocity.x < 50.0,
            "Rebound velocity along normal should be ~45 m/s, got {}", rel_state.velocity.x);

        // 3. Inherits Moon's orbital velocity along +Z (30,000 m/s)
        assert!((rel_state.velocity.z - 30_000.0).abs() < 10.0,
            "Must preserve Moon orbital velocity along Z, got {}", rel_state.velocity.z);
    }

    #[test]
    fn test_stable_surface_touchdown_and_relative_retro_braking() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());

        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::KeyX); // Hold Emergency Retro-Brake
        app.insert_resource(keyboard);
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((
            Window::default(),
            bevy::window::PrimaryWindow,
        ));
        app.world_mut().spawn((
            SpaceCamera::default(),
            Camera::default(),
            Transform::default(),
            GlobalTransform::default(),
        ));

        let earth_radius = 6_371_000.0;
        let v_earth = DVec3::new(0.0, 0.0, 29_780.0);
        let earth_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: earth_radius,
                mass_kg: 5.9722e24,
            },
            crate::celestial::BodyVelocity(v_earth),
            crate::celestial::NavTarget {
                name: "Earth",
                classification: "Planet",
                radius_m: earth_radius,
            },
        )).id();

        app.world_mut().resource_mut::<crate::navigation::NavigationComputer>().target_entity = Some(earth_ent);

        // Shuttle at 10 km altitude above Earth, moving with 500 m/s relative speed + Earth's orbital speed
        let v_rel_initial = DVec3::new(300.0, 400.0, 0.0); // 500 m/s relative
        let v_initial = v_earth + v_rel_initial;
        let rap = crate::relativity::rapidity_from_velocity(v_initial, c);
        let shuttle_entity = app.world_mut().spawn((
            Shuttle::default(),
            RelativisticState {
                rapidity: rap,
                velocity: v_initial,
                ..default()
            },
            UniversalPosition(DVec3::new(earth_radius + 10_000.0, 0.0, 0.0)),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);

        // Advance 2 seconds with X held down
        for _ in 0..120 {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
            app.update();
        }

        let rel_state = app.world().get::<RelativisticState>(shuttle_entity).unwrap();
        let v_rel_final = rel_state.velocity - v_earth;

        // Relative speed should have decayed by e^(-4 * 2) ~ e^(-8) ~ 0.000335 (< 1 m/s)
        assert!(v_rel_final.length() < 1.0,
            "Holding X must brake relative to target body down to < 1 m/s, got {}", v_rel_final.length());
    }

    #[test]
    fn test_shift_x_universal_braking_and_surface_landed_lock() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());

        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::ShiftLeft);
        keyboard.press(KeyCode::KeyX); // Hold Shift + X (Universal Solar Brake)
        app.insert_resource(keyboard);
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((
            Window::default(),
            bevy::window::PrimaryWindow,
        ));
        app.world_mut().spawn((
            SpaceCamera::default(),
            Camera::default(),
            Transform::default(),
            GlobalTransform::default(),
        ));

        let earth_radius = 6_371_000.0;
        let v_earth = DVec3::new(0.0, 0.0, 29_780.0);
        let earth_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: earth_radius,
                mass_kg: 5.9722e24,
            },
            crate::celestial::BodyVelocity(v_earth),
            crate::celestial::NavTarget {
                name: "Earth",
                classification: "Planet",
                radius_m: earth_radius,
            },
        )).id();

        // Shuttle with velocity (0, 0, 10,000 m/s) in universal frame
        let v_initial = DVec3::new(0.0, 0.0, 10_000.0);
        let rap = crate::relativity::rapidity_from_velocity(v_initial, c);
        let shuttle_entity = app.world_mut().spawn((
            Shuttle::default(),
            RelativisticState {
                rapidity: rap,
                velocity: v_initial,
                ..default()
            },
            UniversalPosition(DVec3::new(earth_radius + 50_000.0, 0.0, 0.0)),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let shuttle = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        assert!(shuttle.brake_to_universal, "Shift+X must set brake_to_universal flag");

        // Surface landed lock & liftoff test:
        // Set landed_body to earth_ent
        {
            let mut s_mut = app.world_mut().get_mut::<Shuttle>(shuttle_entity).unwrap();
            s_mut.landed_body = Some(earth_ent);
            s_mut.landed_local_normal = DVec3::X;
            s_mut.braking_active = false;
        }
        // Release keyboard keys
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.release(KeyCode::ShiftLeft);
            kb.release(KeyCode::KeyX);
        }

        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let (upos_landed, rel_landed, s_landed) = (
            app.world().get::<UniversalPosition>(shuttle_entity).unwrap(),
            app.world().get::<RelativisticState>(shuttle_entity).unwrap(),
            app.world().get::<Shuttle>(shuttle_entity).unwrap(),
        );

        // Clamped to surface radius
        assert!((upos_landed.0.x - earth_radius).abs() < 1e-2, "Position must be clamped to Earth surface");
        // Velocity matches Earth
        assert!((rel_landed.velocity.z - v_earth.z).abs() < 1e-2, "Must match Earth velocity");
        assert_eq!(s_landed.landed_body, Some(earth_ent), "Must remain landed");

        // Now press Space to lift off
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.press(KeyCode::Space);
        }
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s_liftoff = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        assert_eq!(s_liftoff.landed_body, None, "Pressing Space must disengage landed lock for liftoff");
    }

    #[test]
    fn test_occlusion_inhibits_autopilots() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer {
            is_occluded: true,
            occluded_by_name: Some("Earth".to_string()),
            ..default()
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());

        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::KeyH); // Try to engage Hyperdrive while occluded
        app.insert_resource(keyboard);
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        let shuttle_entity = app.world_mut().spawn((
            Shuttle::default(),
            RelativisticState::default(),
            UniversalPosition(DVec3::ZERO),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        assert_eq!(s.autopilot_mode, AutopilotMode::Off, "Hyperdrive must NOT engage when occluded");
        assert!(s.occlusion_warning_timer > 3.0, "Occlusion warning banner must be triggered");

        // Release H, press L (Lorentz)
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.release(KeyCode::KeyH);
            kb.press(KeyCode::KeyL);
        }
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        assert_eq!(s.autopilot_mode, AutopilotMode::Off, "Lorentz must NOT engage when occluded");

        // Release L, press F (Aim)
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.release(KeyCode::KeyL);
            kb.press(KeyCode::KeyF);
        }
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        assert_eq!(s.autopilot_mode, AutopilotMode::Off, "Aim must NOT engage when occluded");
    }

    #[test]
    fn test_prebraking_relative_to_closest_body() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));

        let v_planet = DVec3::new(0.0, 0.0, 30_000.0);
        let planet_radius = 6_000_000.0;
        // Closest body: PlanetX at origin moving at 30,000 m/s
        app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: planet_radius,
                mass_kg: 5.972e24,
            },
            crate::celestial::BodyVelocity(v_planet),
            crate::celestial::NavTarget {
                name: "PlanetX",
                classification: "Planet",
                radius_m: planet_radius,
            },
        ));

        // Destination target: FarTarget at 50,000,000 km away
        let target_radius = 1_000_000.0;
        let target_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::new(0.0, 0.0, 50_000_000_000.0)),
            crate::celestial::CelestialBody {
                physical_radius: target_radius,
                mass_kg: 1e23,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            crate::celestial::NavTarget {
                name: "FarTarget",
                classification: "Planet",
                radius_m: target_radius,
            },
        )).id();

        app.insert_resource(crate::navigation::NavigationComputer {
            target_entity: Some(target_ent),
            distance_m: 50_000_000_000.0,
            ..default()
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());

        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        // Shuttle moving at (0, 0, 30,500 m/s) in universal frame -> 500 m/s relative to PlanetX
        let v_initial = DVec3::new(0.0, 0.0, 30_500.0);
        let rap = crate::relativity::rapidity_from_velocity(v_initial, c);
        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                autopilot_mode: AutopilotMode::Hyperdrive,
                autopilot_phase: AutopilotPhase::PreBraking,
                braking_active: true,
                ..default()
            },
            RelativisticState {
                rapidity: rap,
                velocity: v_initial,
                ..default()
            },
            UniversalPosition(DVec3::new(0.0, 0.0, planet_radius + 500_000.0)),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);

        // Advance until PreBraking completes and transitions to Aligning
        let mut transitioned_to_aligning = false;
        let mut min_rel_to_planet = f64::MAX;
        for _ in 0..100 {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
            app.update();

            let (s, rel) = (
                app.world().get::<Shuttle>(shuttle_entity).unwrap(),
                app.world().get::<RelativisticState>(shuttle_entity).unwrap(),
            );
            let rel_speed = (rel.velocity - v_planet).length();
            if rel_speed < min_rel_to_planet {
                min_rel_to_planet = rel_speed;
            }
            if s.autopilot_phase == AutopilotPhase::Aligning {
                transitioned_to_aligning = true;
                break;
            }
        }

        assert!(transitioned_to_aligning, "PreBraking must transition to Aligning once relative speed < 5 m/s");
        assert!(min_rel_to_planet < 5.0, "PreBraking must damp relative to closest body velocity ({:?}), got min: {}", v_planet, min_rel_to_planet);
    }

    #[test]
    fn test_hyperdrive_overshoot_recovery() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));

        let target_radius = 1_737_400.0;
        let target_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::new(0.0, 0.0, 50_000_000.0)),
            crate::celestial::CelestialBody {
                physical_radius: target_radius,
                mass_kg: 7.34e22,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            crate::celestial::NavTarget {
                name: "TargetBody",
                classification: "Moon",
                radius_m: target_radius,
            },
        )).id();

        // Nav computer with distance opening up (positive radial velocity, target behind ship)
        app.insert_resource(crate::navigation::NavigationComputer {
            target_entity: Some(target_ent),
            distance_m: 60_000_000.0,
            radial_velocity: 500.0, // Moving AWAY from target
            relative_speed: 500.0,
            ..default()
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        // Shuttle facing +Z, while target is behind or distance is increasing
        let v_shuttle = DVec3::new(0.0, 0.0, -10_000.0);
        let rap = crate::relativity::rapidity_from_velocity(v_shuttle, c);
        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                autopilot_mode: AutopilotMode::Hyperdrive,
                autopilot_phase: AutopilotPhase::Cruising,
                ..default()
            },
            RelativisticState {
                rapidity: rap,
                velocity: v_shuttle,
                ..default()
            },
            UniversalPosition(DVec3::ZERO),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        assert_eq!(s.autopilot_phase, AutopilotPhase::EmergencyBraking, "Overshoot must trigger EmergencyBraking immediately");
        assert!(s.braking_active, "Emergency dampers must be engaged");
    }

    #[test]
    fn test_lorentz_arrival_orientation_forward() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));

        let uranus_radius = 25_362_000.0;
        let uranus_arr_dist = target_arrival_distance(uranus_radius, c, false); // 1.5 ls = 449,688,687 m
        let uranus_pos = DVec3::new(0.0, 0.0, uranus_arr_dist);
        let uranus_ent = app.world_mut().spawn((
            UniversalPosition(uranus_pos),
            crate::celestial::CelestialBody {
                physical_radius: uranus_radius,
                mass_kg: 8.681e25,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            crate::celestial::NavTarget {
                name: "Uranus",
                classification: "Ice Giant",
                radius_m: uranus_radius,
            },
        )).id();

        app.insert_resource(crate::navigation::NavigationComputer {
            target_entity: Some(uranus_ent),
            distance_m: uranus_arr_dist,
            relative_speed: 10.0, // Deceleration retro-burn almost finished
            ..default()
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        // Shuttle at origin, initially facing backward (retrograde: pointing -Z away from Uranus at +Z)
        let v_shuttle = DVec3::new(0.0, 0.0, 10.0);
        let rap = crate::relativity::rapidity_from_velocity(v_shuttle, c);
        // Quat rotated 180 degrees around Y so forward is -Z
        let backwards_rot = Quat::from_rotation_y(std::f32::consts::PI);
        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                autopilot_mode: AutopilotMode::Lorentz,
                autopilot_phase: AutopilotPhase::Decelerating,
                align_to_target: true,
                ..default()
            },
            RelativisticState {
                rapidity: rap,
                velocity: v_shuttle,
                ..default()
            },
            UniversalPosition(DVec3::ZERO),
            Transform::from_rotation(backwards_rot),
        )).id();

        app.add_systems(Update, shuttle_input_system);

        // Frame 1: Deceleration detects rel_speed < 25 m/s and transitions to TerminalApproach
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s1 = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        assert_eq!(s1.autopilot_phase, AutopilotPhase::TerminalApproach, "Must transition from Decelerating to TerminalApproach");
        assert!(s1.align_to_target, "Must keep align_to_target active to rotate forward");

        // Advance frames while the attitude controller rotates the ship forward toward Uranus (+Z)
        for _ in 0..300 {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
            app.update();
        }

        let (s_final, tf_final) = (
            app.world().get::<Shuttle>(shuttle_entity).unwrap(),
            app.world().get::<Transform>(shuttle_entity).unwrap(),
        );

        // Craft should now be facing Uranus (+Z) and arrived
        let forward = tf_final.forward().as_dvec3();
        let target_dir = uranus_pos.normalize();
        let cos_fwd = forward.dot(target_dir);
        assert!(cos_fwd > 0.95, "Ship must face forward toward Uranus upon arrival, got cos: {}", cos_fwd);
        assert_eq!(s_final.autopilot_mode, AutopilotMode::Off, "Autopilot must be disengaged once arrived");
        assert_eq!(s_final.autopilot_phase, AutopilotPhase::Idle);
    }

    #[test]
    fn test_lorentz_overshoot_recovery() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));

        let neptune_radius = 24_622_000.0;
        let neptune_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::new(0.0, 0.0, 50_000_000.0)),
            crate::celestial::CelestialBody {
                physical_radius: neptune_radius,
                mass_kg: 1.024e26,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            crate::celestial::NavTarget {
                name: "Neptune",
                classification: "Ice Giant",
                radius_m: neptune_radius,
            },
        )).id();

        // Nav computer with distance opening up (overshot, moving away from Neptune)
        app.insert_resource(crate::navigation::NavigationComputer {
            target_entity: Some(neptune_ent),
            distance_m: 60_000_000.0,
            radial_velocity: 16_000_000.0, // Distance opening at 16,000 km/s
            relative_speed: 16_000_000.0,
            ..default()
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        // Shuttle in Decelerating phase with high velocity moving away (-Z)
        let v_shuttle = DVec3::new(0.0, 0.0, -16_000_000.0);
        let rap = crate::relativity::rapidity_from_velocity(v_shuttle, c);
        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                autopilot_mode: AutopilotMode::Lorentz,
                autopilot_phase: AutopilotPhase::Decelerating,
                align_to_target: true,
                ..default()
            },
            RelativisticState {
                rapidity: rap,
                velocity: v_shuttle,
                ..default()
            },
            UniversalPosition(DVec3::ZERO),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        // In overshoot, Decelerating must NOT coast with thrusters off; it must engage thrust/braking!
        assert!(s.boost_active || s.braking_active, "Lorentz overshoot must fire retro-thrust or dampers, never coast idle!");
    }

    #[test]
    fn test_landed_forward_thrust_unlocked() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        let moon_radius = 1_737_400.0;
        let moon_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: moon_radius,
                mass_kg: 7.342e22,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            crate::celestial::NavTarget {
                name: "Luna",
                classification: "Moon",
                radius_m: moon_radius,
            },
        )).id();

        // Shuttle landed on surface of Moon at +X
        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                landed_body: Some(moon_ent),
                landed_local_normal: DVec3::X,
                ..default()
            },
            RelativisticState::default(),
            UniversalPosition(DVec3::new(moon_radius, 0.0, 0.0)),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);

        // Press 'W' (forward thrust)
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.press(KeyCode::KeyW);
        }

        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        let rel = app.world().get::<RelativisticState>(shuttle_entity).unwrap();

        assert_eq!(s.landed_body, None, "Forward thrust 'W' must disengage landed lock");
        assert!(s.current_proper_accel > 0.0, "Proper acceleration must be active when pressing 'W'");
        assert!(rel.velocity.length() > 0.0, "Forward velocity must increase when pressing 'W'");
    }

    #[test]
    fn test_interplanetary_launch_no_false_overshoot() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));

        // Mercury at 90,000,000 km along +Z
        let mercury_radius = 2_439_700.0;
        let mercury_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::new(0.0, 0.0, 90_000_000_000.0)),
            crate::celestial::CelestialBody {
                physical_radius: mercury_radius,
                mass_kg: 3.30e23,
            },
            crate::celestial::BodyVelocity(DVec3::new(0.0, 0.0, 47_000.0)),
            crate::celestial::NavTarget {
                name: "Mercury",
                classification: "Planet",
                radius_m: mercury_radius,
            },
        )).id();

        // Nav computer targeting Mercury: distance opening at 15 km/s due to Keplerian orbits
        app.insert_resource(crate::navigation::NavigationComputer {
            target_entity: Some(mercury_ent),
            distance_m: 90_000_000_000.0,
            radial_velocity: 15_000.0, // Opening at 15 km/s
            relative_speed: 30_000.0,
            ..default()
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        // Shuttle at Earth orbit facing Mercury (+Z)
        let fwd_rot = Quat::from_rotation_y(std::f32::consts::PI); // Bevy default -Z rotated 180 to face +Z
        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                autopilot_mode: AutopilotMode::Hyperdrive,
                autopilot_phase: AutopilotPhase::Accelerating,
                align_to_target: true,
                ..default()
            },
            RelativisticState::default(),
            UniversalPosition(DVec3::ZERO),
            Transform::from_rotation(fwd_rot),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        // Hyperdrive launch must NOT drop to EmergencyBraking due to Keplerian orbital opening velocity!
        assert_ne!(s.autopilot_phase, AutopilotPhase::EmergencyBraking, "Hyperdrive must not falsely abort on interplanetary launch");
        assert!(s.boost_active, "Hyperdrive warp propulsion must be active during Accelerating");
    }

    #[test]
    fn test_lorentz_sun_arrival_terminal_approach() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));

        // Sol at origin with physical radius 696,340,000 m
        let sol_radius = 696_340_000.0;
        let sol_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: sol_radius,
                mass_kg: 1.989e30,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            crate::celestial::NavTarget {
                name: "Sol",
                classification: "Star",
                radius_m: sol_radius,
            },
        )).id();

        // Target arrival distance is sol_radius * 2.4 = 1,671,216,000 m (~5.5746 ls)
        // Shuttle position: 1,800,000,000 m (~6.004 ls from Sol, remaining_dist = ~128,784,000 m = ~0.429 ls)
        let shuttle_dist = 1_800_000_000.0;
        let arrival_dist = sol_radius * 2.4;
        let remaining_dist = shuttle_dist - arrival_dist;
        assert!(remaining_dist < 3.0 * c, "Must be within 3 ls approach window");

        app.insert_resource(crate::navigation::NavigationComputer {
            target_entity: Some(sol_ent),
            distance_m: shuttle_dist,
            radial_velocity: -20.0, // Moving towards Sol at 20 m/s
            relative_speed: 20.0,   // Low relative speed (< 25 m/s)
            relative_velocity: DVec3::new(0.0, 0.0, -20.0),
            closest_body_entity: Some(sol_ent),
            closest_distance_m: shuttle_dist,
            ..default()
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        // Shuttle in Decelerating phase after retro braking down to 20 m/s
        let v_shuttle = DVec3::new(0.0, 0.0, -20.0);
        let rap = crate::relativity::rapidity_from_velocity(v_shuttle, c);
        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                autopilot_mode: AutopilotMode::Lorentz,
                autopilot_phase: AutopilotPhase::Decelerating,
                align_to_target: true,
                ..default()
            },
            RelativisticState {
                velocity: v_shuttle,
                rapidity: rap,
                gamma: 1.0,
                ..default()
            },
            UniversalPosition(DVec3::new(0.0, 0.0, shuttle_dist)),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        // Lorentz must transition directly to TerminalApproach instead of looping back to Aligning
        assert_eq!(
            s.autopilot_phase,
            AutopilotPhase::TerminalApproach,
            "Lorentz should transition to TerminalApproach when decelerated within approach window"
        );
    }

    #[test]
    fn test_lorentz_mercury_orbital_approach() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));

        // Mercury with radius 2,439,700 m, orbiting at 47,870 m/s
        let mercury_radius = 2_439_700.0;
        let mercury_pos = DVec3::new(0.0, 0.0, 58_000_000_000.0);
        let mercury_vel = DVec3::new(47_870.0, 0.0, 0.0);
        let mercury_ent = app.world_mut().spawn((
            UniversalPosition(mercury_pos),
            crate::celestial::CelestialBody {
                physical_radius: mercury_radius,
                mass_kg: 3.30e23,
            },
            crate::celestial::BodyVelocity(mercury_vel),
            crate::celestial::NavTarget {
                name: "Mercury",
                classification: "Planet",
                radius_m: mercury_radius,
            },
        )).id();

        // Shuttle at 4.74 ls from Mercury
        let shuttle_dist = 4.74 * c;
        let shuttle_pos = mercury_pos + DVec3::new(0.0, 0.0, -shuttle_dist);
        let v_rel = DVec3::new(0.0, 0.0, 246.0); // Moving towards Mercury at 246 m/s
        let v_shuttle = mercury_vel + v_rel; // Universal velocity includes Mercury's 47.87 km/s orbit
        let rap = crate::relativity::rapidity_from_velocity(v_shuttle, c);

        app.insert_resource(crate::navigation::NavigationComputer {
            target_entity: Some(mercury_ent),
            distance_m: shuttle_dist,
            radial_velocity: -246.0,
            relative_speed: 246.0,
            relative_velocity: v_rel,
            closest_body_entity: Some(mercury_ent),
            closest_distance_m: shuttle_dist,
            ..default()
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        // Shuttle in Decelerating phase after retro braking down to 246 m/s relative speed
        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                autopilot_mode: AutopilotMode::Lorentz,
                autopilot_phase: AutopilotPhase::Decelerating,
                align_to_target: true,
                ..default()
            },
            RelativisticState {
                velocity: v_shuttle,
                rapidity: rap,
                gamma: (1.0 / (1.0 - (v_shuttle.length() / c).powi(2)).sqrt()),
                ..default()
            },
            UniversalPosition(shuttle_pos),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        // Since remaining distance (4.54 ls) is outside the 3 ls approach window,
        // it must transition to Aligning to re-engage forward thrust and close the distance,
        // NEVER getting stuck in Decelerating (RETRO-COASTING)!
        assert_eq!(
            s.autopilot_phase,
            AutopilotPhase::Aligning,
            "Lorentz should transition to Aligning outside approach window to perform closing thrust"
        );
    }

    #[test]
    fn test_shift_no_engine_glare_when_idle() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());

        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::ShiftLeft); // Holding Shift alone (e.g. for Shift+T)
        app.insert_resource(keyboard);
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        let shuttle_entity = app.world_mut().spawn((
            Shuttle::default(),
            RelativisticState::default(),
            UniversalPosition(DVec3::ZERO),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        assert!(!s.boost_active, "Holding Shift without forward/directional keys must NOT activate boost!");
        assert_eq!(s.thruster_forward, 0.0, "Thrusters must remain idle at 0.0");
    }

    #[test]
    fn test_hyperdrive_cruise_glide_no_spam_braking() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));

        let sol_radius = 696_340_000.0;
        let sol_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: sol_radius,
                mass_kg: 1.989e30,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            crate::celestial::NavTarget {
                name: "Sol",
                classification: "Star",
                radius_m: sol_radius,
            },
        )).id();

        // Target at 1 AU away (~500 ls)
        let dist = 149_597_870_700.0;
        app.insert_resource(crate::navigation::NavigationComputer {
            target_entity: Some(sol_ent),
            distance_m: dist,
            relative_speed: 0.999 * c,
            closing: true,
            ..default()
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        // Shuttle in Cruising phase with high gamma (e.g. gamma = 5000)
        let v_shuttle = DVec3::new(0.0, 0.0, -0.99999998 * c);
        let rap = crate::relativity::rapidity_from_velocity(v_shuttle, c);
        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                autopilot_mode: AutopilotMode::Hyperdrive,
                autopilot_phase: AutopilotPhase::Cruising,
                align_to_target: true,
                ..default()
            },
            RelativisticState {
                velocity: v_shuttle,
                rapidity: rap,
                gamma: 5000.0,
                beta: 0.99999998,
                ..default()
            },
            UniversalPosition(DVec3::new(0.0, 0.0, dist)),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        // At 1 AU, gamma*v exceeds max_allowed_gamma_v. It must smoothly coast (Cruising, ap_braking == false)
        // instead of slamming dampers (EmergencyBraking) and spamming back and forth!
        assert_eq!(s.autopilot_phase, AutopilotPhase::Cruising);
        assert!(!s.braking_active, "Hyperdrive must coast smoothly without spam-braking dampers");
    }

    #[test]
    fn test_lorentz_retro_coasting_and_closed_loop_burn() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));

        let sedna_radius = 500_000.0;
        let sedna_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: sedna_radius,
                mass_kg: 1.0e21,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            crate::celestial::NavTarget {
                name: "Sedna",
                classification: "Dwarf Planet",
                radius_m: sedna_radius,
            },
        )).id();

        // Shuttle at 240 AU from Sedna, moving toward Sedna (-Z) at gamma ~ 9580
        // d_stop from gamma 9580 is ~ 230 AU.
        // Since remaining distance (240 AU) is greater than d_stop (230 AU),
        // it must coast in free fall facing retrograde (need_burn == false) rather than burning prematurely!
        let dist = 240.0 * 149_597_870_700.0;
        let v_shuttle = DVec3::new(0.0, 0.0, -c * 0.99999999);
        let rap = crate::relativity::rapidity_from_velocity(v_shuttle, c);

        app.insert_resource(crate::navigation::NavigationComputer {
            target_entity: Some(sedna_ent),
            distance_m: dist,
            relative_speed: c * 0.99999999,
            relative_velocity: v_shuttle,
            radial_velocity: -c * 0.99999999,
            closing: true,
            ..default()
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        // Shuttle facing retrograde (+Z)
        let backwards_rot = Quat::from_rotation_y(std::f32::consts::PI);
        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                autopilot_mode: AutopilotMode::Lorentz,
                autopilot_phase: AutopilotPhase::Decelerating,
                align_to_target: true,
                ..default()
            },
            RelativisticState {
                velocity: v_shuttle,
                rapidity: rap,
                gamma: 9580.0,
                beta: 0.99999999,
                ..default()
            },
            UniversalPosition(DVec3::new(0.0, 0.0, dist)),
            Transform::from_rotation(backwards_rot),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        // Since 240 AU > d_stop (230 AU), it must retro-coast in free fall without thrusting!
        assert_eq!(s.autopilot_phase, AutopilotPhase::Decelerating);
        assert!(!s.boost_active, "Must coast in free fall when ahead of required stopping distance");
        assert_eq!(s.thruster_forward, 0.0, "Thrusters must be idle during retro-coast");
    }

    #[test]
    fn test_haumea_triaxial_ellipsoid_landing_exact_surface_contact() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::relativity::RelativisticConfig::default());
        let a = 1_161_000.0;
        let b = 513_000.0;
        let c = 852_000.0;
        let haumea_ellipsoid = crate::celestial::EllipsoidRadii::new(a, b, c);

        let haumea_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: 816_000.0,
                mass_kg: 4.0e21,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            crate::celestial::AxialRotation {
                speed_rad_per_sec: 0.05,
            },
            haumea_ellipsoid,
            Transform::from_rotation(Quat::IDENTITY),
        )).id();

        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        // 1. Touchdown on elongated semi-major axis (+X)
        let shuttle_ent = app.world_mut().spawn((
            Shuttle {
                landed_body: Some(haumea_ent),
                landed_local_normal: DVec3::X,
                braking_active: false,
                ..default()
            },
            RelativisticState::default(),
            UniversalPosition(DVec3::new(a, 0.0, 0.0)),
            Transform::default(),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let upos = app.world().get::<UniversalPosition>(shuttle_ent).unwrap();
        assert!((upos.0.length() - a).abs() < 1.0, "Must be clamped exactly at semi-major axis radius a (1161 km)");

        // 2. Touchdown on polar axis (+Y)
        {
            let mut s = app.world_mut().get_mut::<Shuttle>(shuttle_ent).unwrap();
            s.landed_local_normal = DVec3::Y;
        }
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let upos_y = app.world().get::<UniversalPosition>(shuttle_ent).unwrap();
        assert!((upos_y.0.length() - b).abs() < 1.0, "Must be clamped exactly at polar radius b (513 km)");

        // 3. Touchdown on intermediate equatorial axis (+Z)
        {
            let mut s = app.world_mut().get_mut::<Shuttle>(shuttle_ent).unwrap();
            s.landed_local_normal = DVec3::Z;
        }
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let upos_z = app.world().get::<UniversalPosition>(shuttle_ent).unwrap();
        assert!((upos_z.0.length() - c).abs() < 1.0, "Must be clamped exactly at intermediate radius c (852 km)");
    }

    #[test]
    fn test_no_autorotation_and_free_steering_when_landed_on_ellipsoid() {
        let mut app = App::new();
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::relativity::RelativisticConfig::default());
        let a = 1_161_000.0;
        let b = 513_000.0;
        let c = 852_000.0;
        let haumea_ellipsoid = crate::celestial::EllipsoidRadii::new(a, b, c);

        let haumea_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: 816_000.0,
                mass_kg: 4.0e21,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            haumea_ellipsoid,
            Transform::from_rotation(Quat::IDENTITY),
        )).id();

        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        // Initial custom player orientation looking pitched/angled
        let initial_rot = Quat::from_euler(EulerRot::YXZ, 0.45, -0.35, 0.12).normalize();

        // 1. Gentle touchdown on Haumea major axis (+X)
        let shuttle_ent = app.world_mut().spawn((
            Shuttle {
                landed_body: None,
                braking_active: false,
                flight_assist: true,
                ..default()
            },
            RelativisticState {
                velocity: DVec3::new(-1.0, 0.0, 0.0), // Low impact speed (< 75 m/s)
                ..default()
            },
            UniversalPosition(DVec3::new(a + 1.0, 0.0, 0.0)),
            Transform::from_rotation(initial_rot),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        // Must be landed and orientation aligned to surface normal (flat on landing skids)
        let s_landed = app.world().get::<Shuttle>(shuttle_ent).unwrap();
        let tf_landed = app.world().get::<Transform>(shuttle_ent).unwrap();
        assert_eq!(s_landed.landed_body, Some(haumea_ent), "Must land gently on Haumea surface");
        let expected_landed_rot = align_shuttle_to_surface(initial_rot, Vec3::X);
        let dot_landed = tf_landed.rotation.dot(expected_landed_rot).abs();
        assert!(
            (dot_landed - 1.0).abs() < 1e-4,
            "Shuttle rotation upon touchdown must align flat on its landing gear to the surface normal, dot: {}",
            dot_landed
        );

        // 2. Free steering while landed: yaw steering control must work (never stuck)
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.press(KeyCode::ArrowLeft);
        }
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let tf_steered = app.world().get::<Transform>(shuttle_ent).unwrap();
        let dot_steered = tf_steered.rotation.dot(expected_landed_rot).abs();
        assert!(
            dot_steered < 1.0 - 1e-8,
            "Steering input while landed must rotate the ship without auto-lock resetting it, dot: {}",
            dot_steered
        );

        // 3. Space vertical liftoff on Haumea (1161 km):
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.press(KeyCode::Space);
        }
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s_liftoff = app.world().get::<Shuttle>(shuttle_ent).unwrap();
        assert_eq!(s_liftoff.landed_body, None, "Pressing Space must disengage landed lock for liftoff from Haumea");
    }

    #[test]
    fn test_haumea_ellipsoid_landed_surface_normal_drift_free() {
        let mut app = App::new();
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::relativity::RelativisticConfig::default());
        let a = 1_161_000.0;
        let b = 513_000.0;
        let c = 852_000.0;
        let haumea_ellipsoid = crate::celestial::EllipsoidRadii::new(a, b, c);

        let haumea_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: 816_000.0,
                mass_kg: 4.0e21,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            crate::celestial::AxialRotation {
                speed_rad_per_sec: 4.456e-4, // Fast 3.9h spin
            },
            haumea_ellipsoid,
            Transform::from_rotation(Quat::IDENTITY),
        )).id();

        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        let initial_local_normal = DVec3::new(0.6, 0.8, 0.0).normalize();
        let effective_r = haumea_ellipsoid.surface_radius_in_local_dir(initial_local_normal);

        let shuttle_ent = app.world_mut().spawn((
            Shuttle {
                landed_body: Some(haumea_ent),
                landed_local_normal: initial_local_normal,
                braking_active: false,
                flight_assist: true,
                ..default()
            },
            RelativisticState::default(),
            UniversalPosition(initial_local_normal * effective_r),
            Transform::default(),
        )).id();

        app.add_systems(
            Update,
            (
                crate::celestial::axial_rotation_system,
                shuttle_input_system,
            ).chain(),
        );

        // Run 120 frames (~2 seconds of simulation)
        for _ in 0..120 {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
            app.update();
        }

        let s_final = app.world().get::<Shuttle>(shuttle_ent).unwrap();
        assert_eq!(s_final.landed_body, Some(haumea_ent), "Must remain landed on Haumea");
        assert!(
            (s_final.landed_local_normal - initial_local_normal).length() < 1e-6,
            "Landed local normal must NOT drift across frames, expected {:?}, got {:?}",
            initial_local_normal,
            s_final.landed_local_normal
        );
    }

    #[test]
    fn test_lorentz_cross_track_guidance_and_terminal_approach() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(SimulationPause(false));
        let config = crate::relativity::RelativisticConfig {
            is_demo_mode: false,
            ..default()
        };
        let _c = config.c;
        app.insert_resource(config);

        let sedna_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody {
                physical_radius: 500_000.0,
                mass_kg: 2e21,
            },
            crate::celestial::NavTarget {
                name: "Sedna",
                classification: "Dwarf Planet",
                radius_m: 500_000.0,
            },
            crate::celestial::BodyVelocity(DVec3::ZERO),
            Transform::IDENTITY,
        )).id();

        let dist = 100_000_000.0; // 100,000 km
        app.insert_resource(crate::navigation::NavigationComputer {
            target_entity: Some(sedna_ent),
            target_name: "Sedna".to_string(),
            distance_m: dist,
            relative_speed: 1_000.0,
            relative_velocity: DVec3::new(150.0, 0.0, -1_000.0), // Lateral drift along X
            radial_velocity: -1_000.0,
            closing: true,
            ..default()
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                autopilot_mode: AutopilotMode::Lorentz,
                autopilot_phase: AutopilotPhase::TerminalApproach,
                align_to_target: true,
                ..default()
            },
            RelativisticState::default(),
            UniversalPosition(DVec3::new(0.0, 0.0, dist)),
            Transform::from_rotation(Quat::IDENTITY),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let s = app.world().get::<Shuttle>(shuttle_entity).unwrap();
        // At 100,000 km, TerminalApproach must use warp boost to swiftly close distance
        assert!(s.boost_active, "Terminal approach at 100,000 km must engage boost for swift arrival");
        assert_eq!(s.thruster_forward, 1.0, "Thrusters must be full forward");
    }

    #[test]
    fn test_align_shuttle_to_surface() {
        // 1. Surface normal +Y (North pole): should produce identity orientation (Up = +Y, Fwd = -Z)
        let rot_y = align_shuttle_to_surface(Quat::IDENTITY, Vec3::Y);
        let up_y = rot_y * Vec3::Y;
        let fwd_y = rot_y * Vec3::NEG_Z;
        assert!((up_y - Vec3::Y).length() < 1e-4, "Up must be +Y");
        assert!(fwd_y.dot(Vec3::Y).abs() < 1e-4, "Forward must be tangent to surface");

        // 2. Surface normal +X (Equator at +X): Up must be +X, Forward must be orthogonal to +X
        let rot_x = align_shuttle_to_surface(Quat::IDENTITY, Vec3::X);
        let up_x = rot_x * Vec3::Y;
        let fwd_x = rot_x * Vec3::NEG_Z;
        assert!((up_x - Vec3::X).length() < 1e-4, "Up must be +X");
        assert!(fwd_x.dot(Vec3::X).abs() < 1e-4, "Forward must be tangent to surface");

        // 3. Diagonal normal (Haumea surface at arbitrary latitude): Up must match normal, Forward tangent
        let normal = Vec3::new(0.6, 0.8, 0.0).normalize();
        let rot_diag = align_shuttle_to_surface(Quat::IDENTITY, normal);
        let up_diag = rot_diag * Vec3::Y;
        let fwd_diag = rot_diag * Vec3::NEG_Z;
        assert!((up_diag - normal).length() < 1e-4, "Up must match surface normal");
        assert!(fwd_diag.dot(normal).abs() < 1e-4, "Forward must be tangent to surface");
    }

    #[test]
    fn test_forward_flight_points_dead_center_along_line_of_sight() {
        // Line-of-sight along -Z
        let to_target = DVec3::new(0.0, 0.0, -100_000.0);
        let to_target_dist = to_target.length();
        let to_target_dir = (to_target / to_target_dist).as_vec3();

        assert_eq!(to_target_dir, Vec3::NEG_Z, "Forward direction must be dead-center along LOS");
        let angle_deg = to_target_dir.dot(Vec3::NEG_Z).clamp(-1.0, 1.0).acos().to_degrees();
        assert!(angle_deg < 1e-4, "Angle error to target must be 0 degrees");
    }

    #[test]
    fn test_hyperdrive_lead_buffer_warp_braking() {
        let c: f64 = 299_792_458.0;
        let dt_frame: f64 = 0.006; // 165 FPS
        let lead_buffer = c * dt_frame * 1.5;

        // Hyperdrive lead buffer is tight (1.5 frames) to brake at the very last moment like sci-fi
        assert!(lead_buffer < 0.02 * c, "Lead buffer must remain tight for last-moment braking");
    }

    #[test]
    fn test_landed_surface_lock_maintained_during_simulation_pause() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(true)); // Paused!
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());

        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        let moon_radius = 1_737_400.0;
        let moon_pos = DVec3::new(384_400_000.0, 0.0, 0.0);
        let moon_ent = app.world_mut().spawn((
            UniversalPosition(moon_pos),
            crate::celestial::CelestialBody {
                physical_radius: moon_radius,
                mass_kg: 7.342e22,
            },
            Transform::IDENTITY,
        )).id();

        let landed_normal = DVec3::Y;
        let initial_shuttle_pos = moon_pos + landed_normal * moon_radius;

        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                landed_body: Some(moon_ent),
                landed_local_normal: landed_normal,
                ..default()
            },
            RelativisticState::default(),
            UniversalPosition(initial_shuttle_pos),
            Transform::from_rotation(Quat::IDENTITY),
        )).id();

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        // Check that shuttle position remains locked exactly to the Moon surface with 0 drift
        let shuttle_upos = app.world().get::<UniversalPosition>(shuttle_entity).unwrap();
        let dist = (shuttle_upos.0 - moon_pos).length();
        assert!(
            (dist - moon_radius).abs() < 1e-4,
            "Landed shuttle position must stay exactly on Moon surface while paused: dist = {}, expected {}",
            dist,
            moon_radius
        );
    }

    #[test]
    fn test_pause_unpause_zero_orbital_jump() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());

        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        let sol_pos = DVec3::ZERO;
        let sol_ent = app.world_mut().spawn((
            UniversalPosition(sol_pos),
            crate::celestial::CelestialBody {
                physical_radius: 696_340_000.0,
                mass_kg: 1.989e30,
            },
            Transform::IDENTITY,
        )).id();

        let earth_radius = 6_371_000.0;
        let earth_orbit = crate::celestial::KeplerOrbit::circular(Some(sol_ent), 149_597_870_700.0, 0.0, 31_558_149.0, 0.0);
        let (e_pos, e_vel) = earth_orbit.evaluate_state_at_time(0.0);
        let earth_ent = app.world_mut().spawn((
            UniversalPosition(e_pos),
            crate::celestial::CelestialBody {
                physical_radius: earth_radius,
                mass_kg: 5.972e24,
            },
            crate::celestial::BodyVelocity(e_vel),
            earth_orbit,
            Transform::IDENTITY,
        )).id();

        let moon_radius = 1_737_400.0;
        let moon_orbit = crate::celestial::KeplerOrbit::circular(Some(earth_ent), 384_400_000.0, 0.0, 2_360_585.0, 0.0);
        let (m_pos, m_vel) = moon_orbit.evaluate_state_at_time(0.0);
        let moon_ent = app.world_mut().spawn((
            UniversalPosition(e_pos + m_pos),
            crate::celestial::CelestialBody {
                physical_radius: moon_radius,
                mass_kg: 7.342e22,
            },
            crate::celestial::BodyVelocity(e_vel + m_vel),
            moon_orbit,
            crate::celestial::SatelliteOrbit,
            crate::celestial::NavTarget {
                name: "Luna",
                classification: "Moon",
                radius_m: moon_radius,
            },
            Transform::IDENTITY,
        )).id();

        let landed_normal = DVec3::Y;
        let initial_moon_upos = e_pos + m_pos;
        let initial_shuttle_pos = initial_moon_upos + landed_normal * moon_radius;

        let shuttle_entity = app.world_mut().spawn((
            Shuttle {
                landed_body: Some(moon_ent),
                landed_local_normal: landed_normal,
                ..default()
            },
            RelativisticState::default(),
            UniversalPosition(initial_shuttle_pos),
            Transform::from_rotation(Quat::IDENTITY),
        )).id();

        app.add_systems(
            Update,
            (
                shuttle_time_system,
                crate::celestial::axial_rotation_system,
                crate::celestial::planet_orbit_system,
                crate::celestial::satellite_orbit_system,
                shuttle_input_system,
                crate::navigation::navigation_targeting_system,
                crate::coords::floating_origin_system,
            ).chain(),
        );

        // Frame 1: Running (unpaused) for 16ms
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let m_upos_1 = app.world().get::<UniversalPosition>(moon_ent).unwrap().0;
        let s_upos_1 = app.world().get::<UniversalPosition>(shuttle_entity).unwrap().0;
        let dist_1 = (s_upos_1 - m_upos_1).length();
        let m_tf_1 = app.world().get::<Transform>(moon_ent).unwrap().translation;

        // Frame 2: Toggle PAUSE (F1 pressed)
        app.world_mut().resource_mut::<SimulationPause>().0 = true;
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let m_upos_2 = app.world().get::<UniversalPosition>(moon_ent).unwrap().0;
        let s_upos_2 = app.world().get::<UniversalPosition>(shuttle_entity).unwrap().0;
        let dist_2 = (s_upos_2 - m_upos_2).length();
        let m_tf_2 = app.world().get::<Transform>(moon_ent).unwrap().translation;

        // Frame 3: Remains PAUSED
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        let m_upos_3 = app.world().get::<UniversalPosition>(moon_ent).unwrap().0;
        let s_upos_3 = app.world().get::<UniversalPosition>(shuttle_entity).unwrap().0;
        let dist_3 = (s_upos_3 - m_upos_3).length();
        let m_tf_3 = app.world().get::<Transform>(moon_ent).unwrap().translation;

        // Verify zero orbital jump when paused (less than 1 millimeter drift)
        assert!((dist_2 - dist_1).abs() < 1e-4, "Distance to Moon must not jump on pause: dist1 = {}, dist2 = {}", dist_1, dist_2);
        assert!((dist_3 - dist_2).abs() < 1e-4, "Distance to Moon must not drift during pause: dist2 = {}, dist3 = {}", dist_2, dist_3);
        assert!((m_tf_2 - m_tf_1).length() < 1e-4, "Moon visual translation must not jump on pause: tf1 = {:?}, tf2 = {:?}", m_tf_1, m_tf_2);
        assert!((m_tf_3 - m_tf_2).length() < 1e-4, "Moon visual translation must not move while paused: tf2 = {:?}, tf3 = {:?}", m_tf_2, m_tf_3);
        assert!((m_upos_2 - m_upos_1).length() < 1e-6, "Moon universal position must freeze completely on pause");
        assert!((s_upos_2 - s_upos_1).length() < 1e-6, "Shuttle universal position must freeze completely on pause");
    }

    #[test]
    fn test_mouse_steering_rate_limited_in_slow_motion() {
        let run_sim = |time_scale: f64| -> f32 {
            let mut app = App::new();
            let mut config = RelativisticConfig::default();
            config.time_scale = time_scale;
            app.insert_resource(config);
            app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
            app.insert_resource(SimulationPause(false));
            app.insert_resource(crate::navigation::NavigationComputer::default());
            app.insert_resource(ButtonInput::<KeyCode>::default());
            app.insert_resource(ButtonInput::<MouseButton>::default());
            app.add_event::<bevy::input::mouse::MouseMotion>();
            app.add_event::<bevy::input::mouse::MouseWheel>();
            app.init_resource::<Time>();

            let mut window = Window::default();
            window.cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
            app.world_mut().spawn((
                window,
                bevy::window::PrimaryWindow,
            ));
            app.world_mut().spawn((
                SpaceCamera::default(),
                Camera::default(),
                Transform::default(),
                GlobalTransform::default(),
            ));
            let shuttle_ent = app.world_mut().spawn((
                Shuttle {
                    flight_assist: true,
                    ..default()
                },
                RelativisticState::default(),
                UniversalPosition::default(),
                Transform::from_rotation(Quat::IDENTITY),
            )).id();

            app.add_systems(Update, (shuttle_time_system, shuttle_input_system).chain());

            // Initialize systems
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
            app.update();

            // Send a large mouse motion event (e.g. 200 pixels)
            app.world_mut().send_event(bevy::input::mouse::MouseMotion {
                delta: Vec2::new(200.0, 0.0),
            });
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
            app.update();

            let rot = app.world().get::<Transform>(shuttle_ent).unwrap().rotation;
            let diff = rot.inverse() * Quat::IDENTITY;
            2.0 * diff.xyz().length().atan2(diff.w.abs())
        };

        let rot_normal = run_sim(1.0);
        let rot_slow = run_sim(0.05);

        assert!(rot_normal > 0.001, "Normal time must produce rotation from mouse movement: got {}", rot_normal);
        assert!(rot_slow > 0.0, "Slow motion must still rotate: got {}", rot_slow);
        assert!(
            rot_slow < rot_normal * 0.15,
            "Slow motion rotation must be strictly rate-limited by simulation time: normal = {}, slow = {}",
            rot_normal, rot_slow
        );
    }

    #[test]
    fn test_liftoff_without_shift_at_165hz_space_and_forward() {
        for key in [KeyCode::Space, KeyCode::KeyW] {
            let mut app = App::new();
            let c = 299_792_458.0;
            app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
            app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
            app.insert_resource(SimulationPause(false));
            app.insert_resource(crate::navigation::NavigationComputer::default());
            app.insert_resource(crate::celestial::CanopyVisorState::default());
            let mut kb = ButtonInput::<KeyCode>::default();
            kb.press(key);
            app.insert_resource(kb);
            app.insert_resource(ButtonInput::<MouseButton>::default());
            app.add_event::<bevy::input::mouse::MouseMotion>();
            app.add_event::<bevy::input::mouse::MouseWheel>();
            app.init_resource::<Time>();

            app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
            app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

            let earth_radius = 6_371_000.0;
            let earth_ent = app.world_mut().spawn((
                UniversalPosition(DVec3::ZERO),
                crate::celestial::CelestialBody {
                    physical_radius: earth_radius,
                    mass_kg: 5.9722e24,
                },
                crate::celestial::BodyVelocity(DVec3::ZERO),
                Transform::IDENTITY,
            )).id();

            // Landed flat on Earth's North Pole (+Y)
            let shuttle_ent = app.world_mut().spawn((
                Shuttle {
                    landed_body: Some(earth_ent),
                    landed_local_normal: DVec3::Y,
                    ..default()
                },
                RelativisticState::default(),
                UniversalPosition(DVec3::new(0.0, earth_radius, 0.0)),
                Transform::IDENTITY,
            )).id();

            app.add_systems(Update, (shuttle_time_system, shuttle_input_system).chain());

            // Simulate 165 frames (1.0 second at 165 Hz) holding Space or W WITHOUT Shift
            let frame_dt = std::time::Duration::from_micros(6061);
            for _ in 0..165 {
                app.world_mut().resource_mut::<Time>().advance_by(frame_dt);
                app.update();
            }

            let s = app.world().get::<Shuttle>(shuttle_ent).unwrap();
            let upos = app.world().get::<UniversalPosition>(shuttle_ent).unwrap();
            let alt = upos.0.length() - earth_radius;
            assert_eq!(s.landed_body, None, "Key {:?} without Shift must disengage landed lock", key);
            assert!(
                alt > 2.0,
                "Holding {:?} without Shift at 165 Hz on Earth must lift off cleanly above 2m within 1s, got alt = {:.3} m",
                key,
                alt
            );
        }
    }

    #[test]
    fn test_time_scale_and_pause_transitions_zero_lunar_altitude_jump() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));

        let sol_ent = app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody { physical_radius: 696_340_000.0, mass_kg: 0.0 },
            Transform::IDENTITY,
        )).id();

        let earth_orbit = crate::celestial::KeplerOrbit::circular(Some(sol_ent), 149_597_870_700.0, 0.0, 31_558_149.0, 0.0);
        let (e_pos, e_vel) = earth_orbit.evaluate_state_at_time(0.0);
        let earth_ent = app.world_mut().spawn((
            UniversalPosition(e_pos),
            crate::celestial::CelestialBody { physical_radius: 6_371_000.0, mass_kg: 0.0 },
            crate::celestial::BodyVelocity(e_vel),
            earth_orbit,
            Transform::IDENTITY,
        )).id();

        let moon_radius = 1_737_400.0;
        let moon_orbit = crate::celestial::KeplerOrbit::circular(Some(earth_ent), 384_400_000.0, 0.0, 2_360_585.0, 0.0);
        let (m_pos, m_vel) = moon_orbit.evaluate_state_at_time(0.0);
        let moon_init_pos = e_pos + m_pos;
        let moon_init_vel = e_vel + m_vel;
        let moon_ent = app.world_mut().spawn((
            UniversalPosition(moon_init_pos),
            crate::celestial::CelestialBody { physical_radius: moon_radius, mass_kg: 0.0 },
            crate::celestial::BodyVelocity(moon_init_vel),
            moon_orbit,
            crate::celestial::SatelliteOrbit,
            Transform::IDENTITY,
        )).id();

        let hover_alt = 1_600.0;
        let mut rel = RelativisticState::default();
        rel.rapidity = crate::relativity::rapidity_from_velocity(moon_init_vel, c);
        rel.update_from_rapidity(c);

        let shuttle_ent = app.world_mut().spawn((
            Shuttle::default(),
            rel,
            UniversalPosition(moon_init_pos + DVec3::new(0.0, 0.0, moon_radius + hover_alt)),
            Transform::IDENTITY,
        )).id();

        app.add_systems(
            Update,
            (
                pause_toggle_system,
                crate::relativity::relativistic_config_system,
                shuttle_time_system,
                crate::celestial::planet_orbit_system,
                crate::celestial::satellite_orbit_system,
                shuttle_input_system,
            ).chain(),
        );

        let get_alt = |world: &World| -> f64 {
            let s = world.get::<UniversalPosition>(shuttle_ent).unwrap().0;
            let m = world.get::<UniversalPosition>(moon_ent).unwrap().0;
            (s - m).length() - moon_radius
        };

        // Frame 1: normal 1.0x
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();
        let alt_1 = get_alt(app.world());

        // Frame 2: press F7 to jump to 2.0x -> must NOT cause an orbital desync jump!
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::F7);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();
        let alt_2 = get_alt(app.world());
        assert!(
            (alt_2 - alt_1).abs() < 0.05,
            "Changing time_scale with F7 must not cause an orbital altitude jump: alt1={}, alt2={}",
            alt_1, alt_2
        );

        // Frame 3: press F1 to pause -> must NOT cause an altitude jump!
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.release(KeyCode::F7);
            kb.press(KeyCode::F1);
        }
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();
        let alt_3 = get_alt(app.world());
        assert!(
            (alt_3 - alt_2).abs() < 1e-6,
            "Pausing with F1 must freeze altitude with zero jump: alt2={}, alt3={}",
            alt_2, alt_3
        );
    }

    #[test]
    fn test_shuttle_input_no_panic_when_window_closed() {
        let mut app = App::new();
        app.insert_resource(RelativisticConfig::default());
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        // Intentionally spawn NO Window entity (simulating closing the window via 'X' or Alt+F4)
        app.world_mut().spawn((SpaceCamera::default(), Camera::default(), Transform::default(), GlobalTransform::default()));
        app.world_mut().spawn((Shuttle::default(), RelativisticState::default(), UniversalPosition::default(), Transform::default()));

        app.add_systems(Update, shuttle_input_system);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update(); // Must return cleanly without panicking
    }

    #[test]
    fn test_alt_tab_preserves_permanent_free_look_camera() {
        let mut app = App::new();
        app.insert_resource(RelativisticConfig::default());
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(crate::navigation::NavigationComputer::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.add_event::<bevy::input::mouse::MouseWheel>();
        app.init_resource::<Time>();

        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        let mut cam = SpaceCamera::default();
        cam.free_look_active = true;
        cam.target_look_yaw = 1.25;
        cam.target_look_pitch = -0.35;
        let cam_ent = app.world_mut().spawn((cam, Camera::default(), Transform::default(), GlobalTransform::default())).id();
        app.world_mut().spawn((Shuttle::default(), RelativisticState::default(), UniversalPosition::default(), Transform::default()));

        app.add_systems(Update, shuttle_input_system);

        // Simulate pressing Alt+Tab (Alt pressed first, then Tab, then released after window switch)
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.press(KeyCode::AltLeft);
        }
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.press(KeyCode::Tab);
        }
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        app.update();

        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.release(KeyCode::AltLeft);
            kb.release(KeyCode::Tab);
        }
        for _ in 0..10 {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
            app.update();
        }

        let cam_after = app.world().get::<SpaceCamera>(cam_ent).unwrap();
        assert!(cam_after.free_look_active, "Alt+Tab must NOT disable permanent free-look mode");
        assert!(
            (cam_after.target_look_yaw - 1.25).abs() < 1e-4 && (cam_after.target_look_pitch - (-0.35)).abs() < 1e-4,
            "Alt+Tab must NOT reset camera orbit angles: yaw={}, pitch={}",
            cam_after.target_look_yaw,
            cam_after.target_look_pitch
        );
    }
}
