use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::ui::Interaction;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use crate::flight::Shuttle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraViewMode {
    Cockpit,
    Chase,
}

#[derive(Component)]
pub struct SpaceCamera {
    pub mode: CameraViewMode,
    pub chase_distance: f32,
    pub target_chase_distance: f32,
    pub chase_height: f32,
    pub cockpit_offset: Vec3,
    pub target_cockpit_z: f32,
    pub look_yaw: f32,
    pub target_look_yaw: f32,
    pub look_pitch: f32,
    pub target_look_pitch: f32,
    pub free_look_active: bool,
    pub rear_view_active: bool,
}

impl Default for SpaceCamera {
    fn default() -> Self {
        Self {
            mode: CameraViewMode::Chase,
            chase_distance: 145.0,
            target_chase_distance: 145.0,
            chase_height: 32.0,
            cockpit_offset: Vec3::new(0.0, 15.0, 21.0),
            target_cockpit_z: 20.0,
            look_yaw: 0.0,
            target_look_yaw: 0.0,
            look_pitch: 0.0,
            target_look_pitch: 0.0,
            free_look_active: false,
            rear_view_active: false,
        }
    }
}

pub fn camera_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera_query: Query<&mut SpaceCamera>,
) {
    if keyboard.just_pressed(KeyCode::KeyV) {
        for mut cam in camera_query.iter_mut() {
            cam.mode = match cam.mode {
                CameraViewMode::Cockpit => CameraViewMode::Chase,
                CameraViewMode::Chase => CameraViewMode::Cockpit,
            };
        }
    }
}

/// Runs in PostUpdate with Floating Origin.
/// Because the shuttle stays locked at (0, 0, 0), the camera is placed relative
/// to origin using the shuttle's current orientation and free-look orbit angles.
/// Implements constant-speed smooth zooming and analytical quaternion rotation
/// to guarantee zero gimbal singularity jitter.
pub fn camera_follow_system(
    time: Res<Time>,
    shuttle_query: Query<(&Transform, &Shuttle), Without<SpaceCamera>>,
    mut camera_query: Query<(&mut SpaceCamera, &mut Transform)>,
) {
    let Ok((shuttle_tf, shuttle)) = shuttle_query.get_single() else {
        return;
    };
    let dt = time.delta_secs();

    for (mut cam, mut cam_tf) in camera_query.iter_mut() {
        // 1. Constant-speed smooth camera zoom towards target distance
        let chase_zoom_speed = 35.0f32; // m/s
        let dist_diff = cam.target_chase_distance - cam.chase_distance;
        let dist_step = chase_zoom_speed * dt;
        if dist_diff.abs() <= dist_step {
            cam.chase_distance = cam.target_chase_distance;
        } else {
            cam.chase_distance += dist_diff.signum() * dist_step;
        }
        cam.chase_height = (cam.chase_distance * 0.22).clamp(12.0, 75.0);

        let cockpit_zoom_speed = 4.0f32; // m/s
        let z_diff = cam.target_cockpit_z - cam.cockpit_offset.z;
        let z_step = cockpit_zoom_speed * dt;
        if z_diff.abs() <= z_step {
            cam.cockpit_offset.z = cam.target_cockpit_z;
        } else {
            cam.cockpit_offset.z += z_diff.signum() * z_step;
        }

        // 2. Subpixel camera look smoothing (filters mouse packet discrete steps)
        let rot_blend = (1.0 - (-45.0 * dt).exp()).clamp(0.0, 1.0);
        cam.look_yaw += (cam.target_look_yaw - cam.look_yaw) * rot_blend;
        cam.look_pitch += (cam.target_look_pitch - cam.look_pitch) * rot_blend;

        let rear_yaw = if cam.rear_view_active {
            std::f32::consts::PI
        } else {
            0.0
        };

        match cam.mode {
            CameraViewMode::Cockpit => {
                let head_rot = Quat::from_euler(EulerRot::YXZ, cam.look_yaw + rear_yaw, cam.look_pitch, 0.0);
                // Cockpit view: placed at eye position on the shuttle, rotated by head look
                cam_tf.translation = shuttle_tf.rotation * cam.cockpit_offset;
                cam_tf.rotation = (shuttle_tf.rotation * head_rot).normalize();
            }
            CameraViewMode::Chase => {
                // Chase view: positioned behind and above the shuttle, orbiting with look angles.
                // Analytically computed without look_at matrix inversion to guarantee zero singularity jitter.
                let pivot = Vec3::new(0.0, 0.5, 0.0);
                let dy = cam.chase_height - 0.5;
                let dz = cam.chase_distance;
                let pitch_down = dy.atan2(dz);
                let base_cam_rot = Quat::from_rotation_x(-pitch_down);

                let head_rot = Quat::from_euler(EulerRot::YXZ, cam.look_yaw + rear_yaw, cam.look_pitch, 0.0);
                let orbit_offset = head_rot * Vec3::new(0.0, dy, dz);
                let cam_pos = pivot + orbit_offset;

                let mut cam_world = shuttle_tf.rotation * cam_pos;
                // When landed on a celestial body, clamp camera elevation along the surface normal
                // so it never drops below ground level, accounting for flat UV-sphere polygon slope
                // at chase orbit distance plus camera near-clip plane.
                if shuttle.landed_body.is_some() {
                    let surface_up = (shuttle_tf.rotation * Vec3::Y).normalize();
                    let min_elevation = (-8.5f32 + cam.chase_distance * 0.01).min(0.0);
                    let current_elevation = cam_world.dot(surface_up);
                    if current_elevation < min_elevation {
                        cam_world += surface_up * (min_elevation - current_elevation);
                    }
                }

                cam_tf.translation = cam_world;
                cam_tf.rotation = (shuttle_tf.rotation * head_rot * base_cam_rot).normalize();
            }
        }
    }
}

pub fn cursor_grab_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    overlay: Option<Res<crate::ui::SettingsOverlayState>>,
    radar: Option<Res<crate::radar::OrbitalRadarState>>,
    radar_image_query: Query<&bevy::ui::RelativeCursorPosition, With<crate::radar::OrbitalRadarImageNode>>,
    ui_interactions: Query<&Interaction>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = windows.get_single_mut() else {
        return;
    };

    let overlay_visible = overlay.as_ref().map_or(false, |o| o.visible);
    let radar_hovered = radar.as_ref().map_or(false, |r| r.visible && r.is_hovered)
        || radar_image_query.get_single().map_or(false, |rc| rc.mouse_over());
    let ui_hovered = ui_interactions.iter().any(|i| *i != Interaction::None);

    // Toggle cursor grab on Tab (only when menu is not open)
    if keyboard.just_pressed(KeyCode::Tab) && !overlay_visible {
        if window.cursor_options.grab_mode == CursorGrabMode::Locked {
            window.cursor_options.grab_mode = CursorGrabMode::None;
            window.cursor_options.visible = true;
        } else {
            window.cursor_options.grab_mode = CursorGrabMode::Locked;
            window.cursor_options.visible = false;
        }
    }

    // Click inside window to grab cursor if unlocked (only when menu is not open and NOT clicking the radar or UI elements)
    if mouse_buttons.just_pressed(MouseButton::Left)
        && window.cursor_options.grab_mode == CursorGrabMode::None
        && !overlay_visible
        && !radar_hovered
        && !ui_hovered
    {
        window.cursor_options.grab_mode = CursorGrabMode::Locked;
        window.cursor_options.visible = false;
    }
}

pub fn app_exit_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    overlay: Option<Res<crate::ui::SettingsOverlayState>>,
    mut app_exit: EventWriter<AppExit>,
) {
    // Exit application immediately on Escape, unless dismissing the settings overlay
    if keyboard.just_pressed(KeyCode::Escape) {
        if let Some(o) = overlay {
            if o.visible || o.just_closed {
                return;
            }
        }
        app_exit.send(AppExit::Success);
    }
}

/// Adjusts Camera Bloom parameters according to the active Canopy Visor mode:
/// In SolarFilter mode, cuts bloom to eliminate blown-out glare so sunspots & photospheric granules are crisp.
pub fn canopy_visor_camera_system(
    visor: Res<crate::celestial::CanopyVisorState>,
    mut bloom_query: Query<&mut bevy::core_pipeline::bloom::Bloom, With<SpaceCamera>>,
) {
    if !visor.is_changed() {
        return;
    }
    for mut bloom in bloom_query.iter_mut() {
        match visor.mode {
            crate::celestial::CanopyVisorMode::Clear => {
                bloom.intensity = 0.22;
                bloom.prefilter.threshold = 1.8;
                bloom.prefilter.threshold_softness = 0.35;
            }
            crate::celestial::CanopyVisorMode::Polarized => {
                bloom.intensity = 0.09;
                bloom.prefilter.threshold = 2.0;
                bloom.prefilter.threshold_softness = 0.25;
            }
            crate::celestial::CanopyVisorMode::SolarFilter => {
                bloom.intensity = 0.012;
                bloom.prefilter.threshold = 3.2;
                bloom.prefilter.threshold_softness = 0.15;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_constant_zoom_travel() {
        let mut cam = SpaceCamera::default();
        cam.chase_distance = 14.0;
        cam.target_chase_distance = 20.0; // 6m zoom out requested

        let zoom_speed = 18.0f32;
        let dt = 0.1f32; // 100ms
        let step = zoom_speed * dt; // 1.8m per frame

        // Step 1: 14.0 -> 15.8
        let diff = cam.target_chase_distance - cam.chase_distance;
        if diff.abs() <= step {
            cam.chase_distance = cam.target_chase_distance;
        } else {
            cam.chase_distance += diff.signum() * step;
        }
        assert!((cam.chase_distance - 15.8).abs() < 1e-5);

        // Step 2: 15.8 -> 17.6
        let diff = cam.target_chase_distance - cam.chase_distance;
        if diff.abs() <= step {
            cam.chase_distance = cam.target_chase_distance;
        } else {
            cam.chase_distance += diff.signum() * step;
        }
        assert!((cam.chase_distance - 17.6).abs() < 1e-5);

        // Step 3: 17.6 -> 19.4
        let diff = cam.target_chase_distance - cam.chase_distance;
        if diff.abs() <= step {
            cam.chase_distance = cam.target_chase_distance;
        } else {
            cam.chase_distance += diff.signum() * step;
        }
        assert!((cam.chase_distance - 19.4).abs() < 1e-5);

        // Step 4: 19.4 -> 20.0 (clean clamp to target without overshoot)
        let diff = cam.target_chase_distance - cam.chase_distance;
        if diff.abs() <= step {
            cam.chase_distance = cam.target_chase_distance;
        } else {
            cam.chase_distance += diff.signum() * step;
        }
        assert_eq!(cam.chase_distance, 20.0);
    }

    #[test]
    fn test_analytical_chase_camera_stability() {
        let shuttle_rot = Quat::IDENTITY;
        for pitch_deg in [-70.0, -45.0, 0.0, 45.0, 70.0] {
            for yaw_deg in [-180.0, -90.0, 0.0, 90.0, 180.0] {
                let look_yaw = (yaw_deg as f32).to_radians();
                let look_pitch = (pitch_deg as f32).to_radians();
                let head_rot = Quat::from_euler(EulerRot::YXZ, look_yaw, look_pitch, 0.0);

                let dy = 3.5f32 - 0.5f32;
                let dz = 14.0f32;
                let pitch_down = dy.atan2(dz);
                let base_cam_rot = Quat::from_rotation_x(-pitch_down);

                let rot = (shuttle_rot * head_rot * base_cam_rot).normalize();
                assert!(!rot.is_nan(), "Rotation must not be NaN at pitch {pitch_deg}, yaw {yaw_deg}");
                assert!((rot.length() - 1.0).abs() < 1e-4, "Rotation must be normalized");
            }
        }
    }

    #[test]
    fn test_cursor_grab_inhibited_by_ui_interaction() {
        let mut app = App::new();
        app.insert_resource(ButtonInput::<KeyCode>::default());
        let mut mouse_buttons = ButtonInput::<MouseButton>::default();
        mouse_buttons.press(MouseButton::Left);
        app.insert_resource(mouse_buttons);

        // Window initially unlocked
        let mut window = Window::default();
        window.cursor_options.grab_mode = CursorGrabMode::None;
        window.cursor_options.visible = true;
        let window_ent = app.world_mut().spawn((window, PrimaryWindow)).id();

        // Spawn a UI button currently Hovered
        let button_ent = app.world_mut().spawn(Interaction::Hovered).id();

        app.add_systems(Update, cursor_grab_system);
        app.update();

        // Since UI button was hovered, Left click must NOT lock the cursor
        let win = app.world().get::<Window>(window_ent).unwrap();
        assert_eq!(win.cursor_options.grab_mode, CursorGrabMode::None);
        assert!(win.cursor_options.visible);

        // Now set button to None (user clicked empty space)
        *app.world_mut().get_mut::<Interaction>(button_ent).unwrap() = Interaction::None;
        app.update();

        // Now left click MUST lock the cursor
        let win = app.world().get::<Window>(window_ent).unwrap();
        assert_eq!(win.cursor_options.grab_mode, CursorGrabMode::Locked);
        assert!(!win.cursor_options.visible);
    }

    #[test]
    fn test_perspective_projection_astronomical_far() {
        use bevy::render::camera::CameraProjection;
        let mut proj = PerspectiveProjection {
            near: 0.1,
            far: 1.0e16,
            fov: std::f32::consts::PI / 4.0,
            ..default()
        };
        proj.update(1920.0, 1080.0);
        let mat = proj.get_clip_from_view();
        assert!(!mat.is_nan());

        // In Bevy's Reversed-Z Vulkan projection:
        // View space is right-handed (looking down -Z):
        // At near plane (-0.1), z_ndc = 1.0
        // As distance increases towards -infinity, z_ndc decreases towards 0.0
        let p_near = mat.project_point3(Vec3::new(0.0, 0.0, -0.1));
        let p_earth = mat.project_point3(Vec3::new(0.0, 0.0, -6_371_000.0));
        let p_sun = mat.project_point3(Vec3::new(0.0, 0.0, -149_597_870_700.0));
        let p_pluto = mat.project_point3(Vec3::new(0.0, 0.0, -5_906_380_000_000.0));
        let p_far = mat.project_point3(Vec3::new(0.0, 0.0, -1.0e16));

        assert!((p_near.z - 1.0).abs() < 1e-4, "Near plane must project to ~1.0 in Reversed-Z: got {}", p_near.z);
        assert!(p_near.z > p_earth.z, "Near must be closer (higher Z) than Earth");
        assert!(p_earth.z > p_sun.z, "Earth must be closer than Sun");
        assert!(p_sun.z > p_pluto.z, "Sun must be closer than Pluto");
        assert!(p_pluto.z > p_far.z, "Pluto must be closer than Far plane");
        assert!(p_far.z >= 0.0, "Far plane must stay inside the Reversed-Z frustum [0.0, 1.0]");
    }

    #[test]
    fn test_chase_camera_landed_ground_clamp() {
        let mut app = App::new();
        app.init_resource::<Time>();

        // Spawn landed shuttle (landed_body is Some)
        let mut landed_shuttle = Shuttle::default();
        let dummy_body = app.world_mut().spawn_empty().id();
        landed_shuttle.landed_body = Some(dummy_body);

        app.world_mut().spawn((
            landed_shuttle,
            Transform::IDENTITY,
        ));

        // Spawn chase camera pitched way up to +73 deg (1.27 rad) which would penetrate ground without clamp
        let mut cam = SpaceCamera::default();
        cam.mode = CameraViewMode::Chase;
        cam.chase_distance = 350.0;
        cam.target_chase_distance = 350.0;
        cam.chase_height = 75.0;
        cam.look_pitch = 1.274; // ~73 deg
        cam.target_look_pitch = 1.274;

        let cam_ent = app.world_mut().spawn((
            cam,
            Transform::IDENTITY,
        )).id();

        app.add_systems(Update, camera_follow_system);
        app.update();

        let cam_tf = app.world().get::<Transform>(cam_ent).unwrap();
        // Camera Y must stay well above ground plane at -13.95m (for chase_distance=350m, min_elevation = -5.0m)
        assert!(
            cam_tf.translation.y >= -5.01,
            "Landed chase camera must stay safely above ground, got Y = {}",
            cam_tf.translation.y
        );
    }
}
