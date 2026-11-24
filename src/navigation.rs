use bevy::prelude::*;
use bevy::math::DVec3;
use crate::celestial::NavTarget;
use crate::coords::UniversalPosition;
use crate::flight::Shuttle;
use crate::relativity::RelativisticState;

/// Minimum on-screen pixel separation required between the apparent aberrated target and the true
/// rest-frame target position before the secondary [TRUE / REST FRAME] reticle is displayed.
/// Change this value (default: 50.0 px) if you want the second reticle to appear sooner or later.
pub const ABERRATION_RETICLE_MIN_PIXEL_OFFSET: f32 = 50.0;

/// Global Navigation and Targeting Computer State
#[derive(Resource)]
pub struct NavigationComputer {
    pub selected_index: Option<usize>,
    pub pending_target_entity: Option<Entity>,
    pub target_entity: Option<Entity>,
    pub target_name: String,
    pub target_class: String,
    pub distance_m: f64,
    /// Distance from the collision surface of the target in meters: (distance_m - target_radius_m).max(0.0)
    pub altitude_m: Option<f64>,
    /// Physical radius of the locked target in meters
    pub target_radius_m: f64,
    /// Whether direct line-of-sight to the locked target is obstructed by an intervening celestial body
    pub is_occluded: bool,
    /// Name of the celestial body obstructing line-of-sight to the locked target
    pub occluded_by_name: Option<String>,
    pub radial_velocity: f64,
    pub lateral_velocity: f64,
    pub relative_speed: f64,
    pub target_speed: f64,
    pub relative_velocity: DVec3,
    pub closing: bool,
    pub eta_seconds: Option<f64>,
    pub bearing_deg: f32,
    pub screen_pos: Option<Vec2>,
    pub is_in_front: bool,
    /// On-screen viewport coordinates for true geometric position in universal rest frame
    pub true_screen_pos: Option<Vec2>,
    pub true_is_in_front: bool,
    /// Whether aberration has displaced the visual image by >= ABERRATION_RETICLE_MIN_PIXEL_OFFSET
    pub show_true_reticle: bool,
    /// Distance in screen pixels between apparent target and true rest-frame target
    pub reticle_pixel_offset: f32,
    /// Target resting thermodynamic blackbody temperature in Kelvin (T_0)
    pub rest_temperature_k: Option<f32>,
    /// Perceived Doppler-shifted blackbody temperature in Kelvin (T_eff = D * T_0)
    pub perceived_temperature_k: Option<f32>,
    /// Line-of-sight Doppler factor D to target
    pub doppler_factor: f32,
    /// Entity of the closest celestial body with physical radius
    pub closest_body_entity: Option<Entity>,
    /// Name of the closest celestial body with physical radius
    pub closest_body_name: Option<String>,
    /// Distance from the surface of the closest celestial body: (closest_distance_m - radius).max(0.0)
    pub closest_altitude_m: Option<f64>,
    /// Distance to the center of the closest celestial body
    pub closest_distance_m: f64,
    /// Pending target cycling offset requested by interactive UI rolling buttons (+1 = next, -1 = prev)
    pub cycle_target_offset: i32,
}

impl Default for NavigationComputer {
    fn default() -> Self {
        Self {
            selected_index: None,
            pending_target_entity: None,
            cycle_target_offset: 0,
            target_entity: None,
            target_name: "NO TARGET".to_string(),
            target_class: "N/A".to_string(),
            distance_m: 0.0,
            altitude_m: None,
            target_radius_m: 0.0,
            is_occluded: false,
            occluded_by_name: None,
            radial_velocity: 0.0,
            lateral_velocity: 0.0,
            relative_speed: 0.0,
            target_speed: 0.0,
            relative_velocity: DVec3::ZERO,
            closing: false,
            eta_seconds: None,
            bearing_deg: 0.0,
            screen_pos: None,
            is_in_front: false,
            true_screen_pos: None,
            true_is_in_front: false,
            show_true_reticle: false,
            reticle_pixel_offset: 0.0,
            rest_temperature_k: None,
            perceived_temperature_k: None,
            doppler_factor: 1.0,
            closest_body_entity: None,
            closest_body_name: None,
            closest_altitude_m: None,
            closest_distance_m: 0.0,
        }
    }
}

/// UI Marker for the on-screen holographic targeting bracket (Apparent visual position)
#[derive(Component)]
pub struct TargetReticleUi;

/// UI Marker for the targeting bracket label
#[derive(Component)]
pub struct TargetReticleText;

/// UI Marker for the secondary true geometric rest-frame bracket
#[derive(Component)]
pub struct TrueTargetReticleUi;

/// UI Marker for the secondary true geometric bracket label
#[derive(Component)]
pub struct TrueTargetReticleText;

/// UI Marker for the off-screen directional chevron indicator
#[derive(Component)]
#[allow(dead_code)]
pub struct TargetOffscreenIndicator;

/// Sorts navigation targets into a deterministic astronomical hierarchy:
/// 1. Sol (Central Star)
/// 2. Primary celestial bodies (planets and dwarf planets orbiting Sol), ordered by semi-major axis (Mercury -> Venus -> Earth -> Mars...)
/// 3. For any planet with moons, its moons are placed immediately after it, ordered by their orbital radius around that planet (e.g., Earth -> Luna)
/// 4. Secondary targets (gates, stations, beacons) placed at the end.
pub fn sort_nav_targets<T, F>(items: &mut Vec<T>, mut extract: F)
where
    F: FnMut(&T) -> (Entity, &'static str, Option<&crate::celestial::KeplerOrbit>, f64),
{
    use std::collections::HashMap;

    // Step 1: Identify Sol entity (if present)
    let mut sol_entity = None;
    for item in items.iter() {
        let (ent, name, orbit, _) = extract(item);
        if name.eq_ignore_ascii_case("Sol") || name.eq_ignore_ascii_case("Sun") || (orbit.is_none() && name.eq_ignore_ascii_case("Sol")) {
            sol_entity = Some(ent);
            break;
        }
    }

    // Step 2: Build a map of primary bodies and their orbital distances
    let mut primary_dist_map: HashMap<Entity, f64> = HashMap::new();
    for item in items.iter() {
        let (ent, name, orbit, dist_from_origin) = extract(item);
        if Some(ent) == sol_entity || name.eq_ignore_ascii_case("Sol") {
            continue;
        }

        let is_primary = match orbit {
            Some(orb) => orb.parent == sol_entity || orb.parent.is_none(),
            None => false,
        };

        if is_primary {
            let primary_a = orbit.map(|o| o.semi_major_axis).filter(|&a| a > 0.0).unwrap_or(dist_from_origin);
            primary_dist_map.insert(ent, primary_a);
        }
    }

    // Step 3: Compute sorting keys: (group, primary_dist, moon_dist)
    let mut keys: Vec<(u32, f64, f64)> = Vec::with_capacity(items.len());
    for item in items.iter() {
        let (ent, name, orbit, dist_from_origin) = extract(item);
        let key = if Some(ent) == sol_entity || name.eq_ignore_ascii_case("Sol") {
            (0, 0.0, 0.0)
        } else if let Some(orb) = orbit {
            if let Some(parent_ent) = orb.parent {
                if let Some(&parent_a) = primary_dist_map.get(&parent_ent) {
                    (1, parent_a, orb.semi_major_axis + 1.0)
                } else if Some(parent_ent) == sol_entity {
                    let a = if orb.semi_major_axis > 0.0 { orb.semi_major_axis } else { dist_from_origin };
                    (1, a, 0.0)
                } else {
                    (2, dist_from_origin, 0.0)
                }
            } else {
                let a = if orb.semi_major_axis > 0.0 { orb.semi_major_axis } else { dist_from_origin };
                (1, a, 0.0)
            }
        } else if let Some(&a) = primary_dist_map.get(&ent) {
            (1, a, 0.0)
        } else {
            (2, dist_from_origin, 0.0)
        };
        keys.push(key);
    }

    // Step 4: Sort items according to computed keys
    let mut indices: Vec<usize> = (0..items.len()).collect();
    indices.sort_by(|&a, &b| {
        let ka = keys[a];
        let kb = keys[b];
        ka.0.cmp(&kb.0)
            .then_with(|| ka.1.partial_cmp(&kb.1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| ka.2.partial_cmp(&kb.2).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut temp: Vec<Option<T>> = items.drain(..).map(Some).collect();
    for idx in indices {
        items.push(temp[idx].take().unwrap());
    }
}

/// Handles target cycling (<kbd>T</kbd>, <kbd>Shift+T</kbd>, <kbd>Y</kbd>, <kbd>G / M-Click</kbd>) and real-time range/vector solutions
pub fn navigation_targeting_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Option<Res<ButtonInput<MouseButton>>>,
    shuttle_query: Query<(&UniversalPosition, &Transform, &RelativisticState, &Shuttle)>,
    camera_query: Query<(&Camera, &Transform), With<crate::camera::SpaceCamera>>,
    targets_query: Query<(
        Entity,
        &NavTarget,
        &UniversalPosition,
        Option<&crate::celestial::RelativisticOpticsBody>,
        Option<&crate::celestial::BodyVelocity>,
        Option<&crate::celestial::EllipsoidRadii>,
        Option<&Transform>,
        Option<&crate::celestial::KeplerOrbit>,
    )>,
    optics: Option<Res<crate::relativity::RelativisticOpticsConfig>>,
    mut nav: ResMut<NavigationComputer>,
) {
    let Ok((shuttle_upos, shuttle_tf, rel_state, shuttle)) = shuttle_query.get_single() else {
        return;
    };

    let mut targets: Vec<(
        Entity,
        &NavTarget,
        &UniversalPosition,
        Option<&crate::celestial::RelativisticOpticsBody>,
        Option<&crate::celestial::BodyVelocity>,
        Option<&crate::celestial::EllipsoidRadii>,
        Option<&Transform>,
        Option<&crate::celestial::KeplerOrbit>,
    )> = targets_query.iter().collect();
    if targets.is_empty() {
        return;
    }

    // Sort targets by astronomical hierarchy: Sol -> Mercury -> Venus -> Earth -> Luna -> Mars ...
    sort_nav_targets(&mut targets, |t| (t.0, t.1.name, t.7, t.2.0.length()));

    let in_autonomous_flight = shuttle.autopilot_mode == crate::flight::AutopilotMode::Hyperdrive
        || shuttle.autopilot_mode == crate::flight::AutopilotMode::Lorentz;

    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    // Target Selection Hotkeys, Rolling UI Buttons & Radar Clicks (Disabled during autonomous autopilot to prevent trajectory derailment)
    if !in_autonomous_flight {
        // Apply interactive UI target rolling menu offset ([ < ] and [ > ] buttons)
        if nav.cycle_target_offset != 0 {
            let offset = nav.cycle_target_offset;
            nav.cycle_target_offset = 0;
            let count = targets.len();
            if count > 0 {
                let curr = nav.selected_index.unwrap_or(0) as isize;
                let next = (curr + offset as isize).rem_euclid(count as isize) as usize;
                nav.selected_index = Some(next);
            }
        }

        // Apply pending target selection from Radar click or external UI
        if let Some(requested_ent) = nav.pending_target_entity.take() {
            if let Some(idx) = targets.iter().position(|(ent, ..)| *ent == requested_ent) {
                nav.selected_index = Some(idx);
            }
        }

        // T: Cycle Next | Shift + T: Cycle Prev
        if keyboard.just_pressed(KeyCode::KeyT) {
            let count = targets.len();
            nav.selected_index = match nav.selected_index {
                None => Some(0),
                Some(idx) => {
                    if shift {
                        Some((idx + count - 1) % count)
                    } else {
                        Some((idx + 1) % count)
                    }
                }
            };
        }

        // Y: Lock Nearest Target
        if keyboard.just_pressed(KeyCode::KeyY) {
            let mut nearest_idx = None;
            let mut min_dist = f64::MAX;
            for (i, (_ent, _target, upos, ..)) in targets.iter().enumerate() {
                let dist = (upos.0 - shuttle_upos.0).length();
                if dist < min_dist {
                    min_dist = dist;
                    nearest_idx = Some(i);
                }
            }
            nav.selected_index = nearest_idx;
        }

        // G / Middle Click: Target Object Under Crosshairs (Point & Select) | Shift + G: Clear Target Lock
        let middle_click = mouse_buttons.as_ref().map(|m| m.just_pressed(MouseButton::Middle)).unwrap_or(false);
        if keyboard.just_pressed(KeyCode::KeyG) || middle_click {
            if shift {
                nav.selected_index = None;
                nav.target_entity = None;
            } else {
                let cam_forward = if let Ok((_, cam_tf)) = camera_query.get_single() {
                    cam_tf.forward().as_dvec3()
                } else {
                    shuttle_tf.forward().as_dvec3()
                };

                let aberration_on = optics.as_ref().map(|o| o.aberration_enabled).unwrap_or(false);
                let mut best_idx = None;
                let mut best_cos = 0.866; // Minimum 30° targeting cone around crosshairs

                for (i, (_ent, _target, upos, ..)) in targets.iter().enumerate() {
                    let to_target = upos.0 - shuttle_upos.0;
                    if to_target.length_squared() > 1.0 {
                        let apparent_dir = if aberration_on && rel_state.rapidity.length_squared() > 1e-10 {
                            crate::relativity::aberrate_direction_from_rapidity(to_target, rel_state.rapidity)
                        } else {
                            to_target.normalize()
                        };

                        let cos_angle = cam_forward.dot(apparent_dir);
                        if cos_angle > best_cos {
                            best_cos = cos_angle;
                            best_idx = Some(i);
                        }
                    }
                }

                if let Some(idx) = best_idx {
                    nav.selected_index = Some(idx);
                }
            }
        }
    } else {
        // Discard any external target requests during autonomous autopilot
        nav.pending_target_entity = None;
    }

    // Solve navigation telemetry for active target
    if let Some(idx) = nav.selected_index {
        if idx >= targets.len() {
            nav.selected_index = Some(0);
        }
        let (target_ent, target, target_upos, target_optics, target_vel_opt, target_ellipsoid_opt, target_tf_opt, _target_orbit) = targets[nav.selected_index.unwrap_or(0)];
        nav.target_entity = Some(target_ent);
        nav.target_name = target.name.to_string();
        nav.target_class = target.classification.to_string();

        let target_vel = target_vel_opt.map(|v| v.0).unwrap_or(DVec3::ZERO);
        nav.target_speed = target_vel.length();

        let rel_pos = target_upos.0 - shuttle_upos.0;
        let dist = rel_pos.length();
        nav.distance_m = dist;

        if target.radius_m > 0.0 {
            let effective_r = if let (Some(ellipsoid), Some(tf)) = (target_ellipsoid_opt, target_tf_opt) {
                let to_shuttle = if dist > 0.001 { -rel_pos / dist } else { DVec3::Y };
                let local_dir = tf.rotation.as_dquat().inverse() * to_shuttle;
                ellipsoid.surface_radius_in_local_dir(local_dir)
            } else {
                target.radius_m
            };
            nav.target_radius_m = effective_r;
            if shuttle.landed_body == Some(target_ent) {
                nav.altitude_m = Some(0.0);
            } else {
                nav.altitude_m = Some((dist - effective_r).max(0.0));
            }
        } else {
            nav.target_radius_m = 0.0;
            nav.altitude_m = None;
        }

        if dist > 0.001 {
            let dir_to_target = rel_pos / dist;
            // True relative velocity: shuttle velocity minus target's dynamic velocity
            let v_rel = rel_state.velocity - target_vel;
            nav.relative_velocity = v_rel;
            nav.relative_speed = v_rel.length();

            // Radial velocity: rate of change of distance (closing if negative, opening if positive)
            // v_radial = - (v_rel . dir_to_target)
            let v_radial = -v_rel.dot(dir_to_target);
            nav.radial_velocity = if shuttle.landed_body == Some(target_ent) {
                0.0
            } else {
                v_radial
            };
            nav.closing = nav.radial_velocity < -0.5;

            // Lateral / tangential speed relative to line of sight
            let v_tangent = v_rel - dir_to_target * (-v_radial);
            nav.lateral_velocity = v_tangent.length();

            if nav.closing && v_radial.abs() > 0.5 {
                nav.eta_seconds = Some(dist / v_radial.abs());
            } else {
                nav.eta_seconds = None;
            }

            // Bearing angle relative to shuttle forward
            let forward = shuttle_tf.forward().as_dvec3();
            let cos_angle = forward.dot(dir_to_target).clamp(-1.0, 1.0);
            nav.bearing_deg = cos_angle.acos().to_degrees() as f32;

            // Relativistic Doppler Factor & Thermodynamic Temperature Sensing
            let dir_f32 = dir_to_target.as_vec3();
            let beta_vec = rel_state.beta_vec.as_vec3();
            let gamma = rel_state.gamma as f32;
            let doppler = crate::relativity::relativistic_doppler_factor(dir_f32, beta_vec, gamma);
            nav.doppler_factor = doppler;

            // Line of Sight (LOS) Ray-Sphere Occlusion Check:
            // Check if any intervening celestial body blocks direct path to target
            let mut occluded_by = None;
            for (obs_ent, obs_target, obs_pos, ..) in targets.iter() {
                if *obs_ent == target_ent {
                    continue;
                }
                if obs_target.radius_m <= 0.0 {
                    continue;
                }

                let to_obs = obs_pos.0 - shuttle_upos.0;
                let t = to_obs.dot(dir_to_target);

                // Line-of-sight ray-sphere intersection with safety corridor
                if t > 0.0 && t < dist {
                    let dist_sq = (to_obs.length_squared() - t * t).max(0.0);
                    let r_margin = obs_target.radius_m * 1.15; // 15% safety buffer
                    if dist_sq < r_margin * r_margin {
                        occluded_by = Some(obs_target.name.to_string());
                        break;
                    }
                }
            }
            nav.is_occluded = occluded_by.is_some();
            nav.occluded_by_name = occluded_by;

            if let Some(optics) = target_optics {
                nav.rest_temperature_k = Some(optics.rest_temperature_k);
                nav.perceived_temperature_k = Some(doppler * optics.rest_temperature_k);
            } else {
                nav.rest_temperature_k = None;
                nav.perceived_temperature_k = None;
            }
        } else {
            nav.radial_velocity = 0.0;
            nav.lateral_velocity = 0.0;
            nav.relative_speed = 0.0;
            nav.relative_velocity = DVec3::ZERO;
            nav.closing = false;
            nav.eta_seconds = None;
            nav.bearing_deg = 0.0;
            nav.rest_temperature_k = None;
            nav.perceived_temperature_k = None;
            nav.doppler_factor = 1.0;
            nav.is_occluded = false;
            nav.occluded_by_name = None;
        }
    } else {
        nav.target_entity = None;
        nav.target_name = "NO TARGET".to_string();
        nav.target_class = "N/A".to_string();
        nav.distance_m = 0.0;
        nav.altitude_m = None;
        nav.target_radius_m = 0.0;
        nav.is_occluded = false;
        nav.occluded_by_name = None;
        nav.radial_velocity = 0.0;
        nav.lateral_velocity = 0.0;
        nav.relative_speed = 0.0;
        nav.target_speed = 0.0;
        nav.relative_velocity = DVec3::ZERO;
        nav.closing = false;
        nav.eta_seconds = None;
        nav.bearing_deg = 0.0;
        nav.screen_pos = None;
        nav.is_in_front = false;
        nav.true_screen_pos = None;
        nav.true_is_in_front = false;
        nav.show_true_reticle = false;
        nav.reticle_pixel_offset = 0.0;
        nav.rest_temperature_k = None;
        nav.perceived_temperature_k = None;
        nav.doppler_factor = 1.0;
    }

    // Solve closest celestial body proximity telemetry (always active, independent of target lock)
    let mut closest_ent = None;
    let mut closest_name = None;
    let mut closest_dist = f64::MAX;
    let mut closest_alt = None;

    for (ent, target, pos, _, _, ellipsoid_opt, tf_opt, _) in targets.iter() {
        if target.radius_m > 0.0 {
            let rel = shuttle_upos.0 - pos.0;
            let d = rel.length();
            if d < closest_dist {
                closest_dist = d;
                closest_ent = Some(*ent);
                closest_name = Some(target.name.to_string());
                let effective_r = if let (Some(ellipsoid), Some(tf)) = (ellipsoid_opt, tf_opt) {
                    let to_shuttle = if d > 0.001 { rel / d } else { DVec3::Y };
                    let local_dir = tf.rotation.as_dquat().inverse() * to_shuttle;
                    ellipsoid.surface_radius_in_local_dir(local_dir)
                } else {
                    target.radius_m
                };
                if shuttle.landed_body == Some(*ent) {
                    closest_alt = Some(0.0);
                } else {
                    closest_alt = Some((d - effective_r).max(0.0));
                }
            }
        }
    }

    nav.closest_body_entity = closest_ent;
    nav.closest_body_name = closest_name;
    nav.closest_altitude_m = closest_alt;
    nav.closest_distance_m = closest_dist;
}

/// Computes screen-space projections for both the apparent aberrated targeting reticle
/// and the true geometric rest-frame targeting reticle.
pub fn update_target_screen_position_system(
    camera_query: Query<(&Camera, &Transform), With<crate::camera::SpaceCamera>>,
    targets_query: Query<(&Transform, &UniversalPosition), With<NavTarget>>,
    shuttle_query: Query<&UniversalPosition, With<Shuttle>>,
    optics: Res<crate::relativity::RelativisticOpticsConfig>,
    mut nav: ResMut<NavigationComputer>,
) {
    let Ok((camera, cam_tf)) = camera_query.get_single() else {
        return;
    };
    let cam_global = GlobalTransform::from(*cam_tf);

    let Ok(shuttle_upos) = shuttle_query.get_single() else {
        return;
    };

    let Some(target_ent) = nav.target_entity else {
        nav.screen_pos = None;
        nav.is_in_front = false;
        nav.true_screen_pos = None;
        nav.true_is_in_front = false;
        nav.show_true_reticle = false;
        nav.reticle_pixel_offset = 0.0;
        return;
    };

    if let Ok((target_tf, target_upos)) = targets_query.get(target_ent) {
        let cam_forward = cam_global.forward();

        // 1. Apparent visual target position (with aberration applied by floating_origin_system)
        let to_apparent = (target_tf.translation - cam_global.translation()).normalize_or_zero();
        nav.is_in_front = cam_forward.dot(to_apparent) > 0.0;

        if nav.is_in_front {
            nav.screen_pos = camera.world_to_viewport(&cam_global, target_tf.translation).ok();
        } else {
            nav.screen_pos = None;
        }

        // 2. True geometric target position in universal rest frame (unaberrated)
        let true_rel_pos = target_upos.0 - shuttle_upos.0;
        let true_dist = true_rel_pos.length();

        if true_dist > 0.001 {
            let true_dir = (true_rel_pos / true_dist).as_vec3();
            // Preserve identical rendering depth so depth scaling/projection matches
            let render_dist = target_tf.translation.length();
            let true_world_pos = true_dir * render_dist;

            let to_true = (true_world_pos - cam_global.translation()).normalize_or_zero();
            nav.true_is_in_front = cam_forward.dot(to_true) > 0.0;

            if nav.true_is_in_front {
                nav.true_screen_pos = camera.world_to_viewport(&cam_global, true_world_pos).ok();
            } else {
                nav.true_screen_pos = None;
            }
        } else {
            nav.true_screen_pos = None;
            nav.true_is_in_front = false;
        }

        // 3. Screen pixel distance and dual reticle threshold check
        if optics.aberration_enabled {
            if let (Some(pos_app), Some(pos_true)) = (nav.screen_pos, nav.true_screen_pos) {
                let offset = pos_app.distance(pos_true);
                nav.reticle_pixel_offset = offset;
                nav.show_true_reticle = offset >= ABERRATION_RETICLE_MIN_PIXEL_OFFSET;
            } else {
                nav.reticle_pixel_offset = 0.0;
                nav.show_true_reticle = false;
            }
        } else {
            nav.reticle_pixel_offset = 0.0;
            nav.show_true_reticle = false;
        }
    } else {
        nav.screen_pos = None;
        nav.is_in_front = false;
        nav.true_screen_pos = None;
        nav.true_is_in_front = false;
        nav.show_true_reticle = false;
        nav.reticle_pixel_offset = 0.0;
    }
}

/// Spawns the HUD reticle UI elements
pub fn setup_navigation_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font_mono = asset_server.load("fonts/DejaVuSansMono.ttf");

    // 1. Primary Apparent Target Reticle (Cyan/Green bracket tracking the aberrated visual image)
    commands
        .spawn((
            TargetReticleUi,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(52.0),
                height: Val::Px(52.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.5)),
                display: Display::None,
                ..default()
            },
            BorderColor(Color::srgba(0.2, 0.9, 1.0, 0.85)),
        ))
        .with_children(|parent| {
            parent.spawn((
                TargetReticleText,
                Text::new(""),
                TextFont {
                    font: font_mono.clone(),
                    font_size: 11.5,
                    ..default()
                },
                TextLayout::new_with_justify(JustifyText::Center),
                TextColor(Color::srgb(0.3, 1.0, 0.6)),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(56.0),
                    left: Val::Px(-84.0), // Center 220px wide label under 52px bracket
                    width: Val::Px(220.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ));
        });

    // 2. Secondary True Geometric Rest-Frame Reticle (Amber bracket tracking where target physically is)
    commands
        .spawn((
            TrueTargetReticleUi,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(52.0),
                height: Val::Px(52.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.5)),
                display: Display::None,
                ..default()
            },
            BorderColor(Color::srgba(1.0, 0.78, 0.2, 0.9)), // Amber / Gold indicator
        ))
        .with_children(|parent| {
            parent.spawn((
                TrueTargetReticleText,
                Text::new(""),
                TextFont {
                    font: font_mono,
                    font_size: 11.5,
                    ..default()
                },
                TextLayout::new_with_justify(JustifyText::Center),
                TextColor(Color::srgb(1.0, 0.82, 0.3)),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(56.0),
                    left: Val::Px(-84.0),
                    width: Val::Px(220.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ));
        });
}

/// Updates position and visibility of both the apparent holographic reticle and the true rest-frame reticle
pub fn update_navigation_reticle_system(
    nav: Res<NavigationComputer>,
    hud_state: Option<Res<crate::ui::HudDisplayState>>,
    mut reticle_query: Query<(&mut Node, &mut BorderColor), (With<TargetReticleUi>, Without<TrueTargetReticleUi>)>,
    mut text_query: Query<&mut Text, (With<TargetReticleText>, Without<TrueTargetReticleText>)>,
    mut true_reticle_query: Query<&mut Node, (With<TrueTargetReticleUi>, Without<TargetReticleUi>)>,
    mut true_text_query: Query<&mut Text, (With<TrueTargetReticleText>, Without<TargetReticleText>)>,
) {
    if hud_state.as_ref().map_or(false, |h| h.mode == crate::ui::HudDisplayMode::Hidden) {
        if let Ok((mut node, _)) = reticle_query.get_single_mut() {
            node.display = Display::None;
        }
        if let Ok(mut true_node) = true_reticle_query.get_single_mut() {
            true_node.display = Display::None;
        }
        return;
    }

    // 1. Primary Apparent Reticle
    if let (Ok((mut node, mut border_color)), Ok(mut label)) = (
        reticle_query.get_single_mut(),
        text_query.get_single_mut(),
    ) {
        if let (Some(pos), true) = (nav.screen_pos, nav.is_in_front) {
            node.display = Display::Flex;
            node.left = Val::Px(pos.x - 26.0);
            node.top = Val::Px(pos.y - 26.0);

            let dist_str = format_nav_distance(nav.distance_m);
            if nav.is_occluded {
                let obs_name = nav.occluded_by_name.as_deref().unwrap_or("OBSTACLE");
                **label = format!("{}\n[{} • BLOCKED BY {}]", nav.target_name, dist_str, obs_name.to_uppercase());
                *border_color = BorderColor(Color::srgba(1.0, 0.45, 0.1, 0.95)); // Warning Amber
            } else if nav.show_true_reticle {
                **label = format!("{}\n[{} • APPARENT]", nav.target_name, dist_str);
                if nav.closing {
                    *border_color = BorderColor(Color::srgba(0.2, 1.0, 0.4, 0.9));
                } else {
                    *border_color = BorderColor(Color::srgba(0.2, 0.85, 1.0, 0.85));
                }
            } else {
                **label = format!("{}\n[{}]", nav.target_name, dist_str);
                if nav.closing {
                    *border_color = BorderColor(Color::srgba(0.2, 1.0, 0.4, 0.9));
                } else {
                    *border_color = BorderColor(Color::srgba(0.2, 0.85, 1.0, 0.85));
                }
            }
        } else {
            node.display = Display::None;
        }
    }

    // 2. Secondary True Geometric Rest-Frame Reticle
    if let (Ok(mut true_node), Ok(mut true_label)) = (
        true_reticle_query.get_single_mut(),
        true_text_query.get_single_mut(),
    ) {
        if nav.show_true_reticle && nav.true_is_in_front && nav.true_screen_pos.is_some() {
            let pos = nav.true_screen_pos.unwrap();
            true_node.display = Display::Flex;
            true_node.left = Val::Px(pos.x - 26.0);
            true_node.top = Val::Px(pos.y - 26.0);

            **true_label = format!("TRUE POSITION\n[Δ{:.0}px REST FRAME]", nav.reticle_pixel_offset);
        } else {
            true_node.display = Display::None;
        }
    }
}

/// Formats distance dynamically into meters, kilometers, astronomical light units (ls, lm, lh, ld) and AU
pub fn format_nav_distance(dist_m: f64) -> String {
    let au = crate::celestial::ASTRONOMICAL_UNIT;
    let c = 299_792_458.0;
    if dist_m < 10_000.0 {
        format!("{:.0} m", dist_m)
    } else if dist_m < 10_000_000.0 {
        format!("{:.1} km", dist_m / 1000.0)
    } else if dist_m >= au * 0.1 {
        let ls = dist_m / c;
        let au_val = dist_m / au;
        if ls >= 86_400.0 {
            let ld = ls / 86_400.0;
            format!("{:.2} AU ({:.2} ld)", au_val, ld)
        } else if ls >= 3_600.0 {
            let lh = ls / 3_600.0;
            format!("{:.2} AU ({:.2} lh)", au_val, lh)
        } else if ls >= 60.0 {
            let lm = ls / 60.0;
            format!("{:.2} AU ({:.1} lm)", au_val, lm)
        } else {
            format!("{:.3} AU ({:.1} ls)", au_val, ls)
        }
    } else {
        let ls = dist_m / c;
        format!("{:.0} km ({:.2} ls)", dist_m / 1000.0, ls)
    }
}

/// Formats ETA into mm:ss, hh:mm:ss, or dd:hh:mm:ss
pub fn format_eta(eta_secs: f64) -> String {
    let total_secs = eta_secs.max(0.0) as u64;
    let days = total_secs / 86400;
    let hours = (total_secs % 86400) / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if days > 0 {
        format!("{}d {:02}h {:02}m {:02}s", days, hours, mins, secs)
    } else if hours > 0 {
        format!("{:02}h {:02}m {:02}s", hours, mins, secs)
    } else {
        format!("{:02}m {:02}s", mins, secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::DVec3;

    #[test]
    fn test_format_nav_distance() {
        let au = crate::celestial::ASTRONOMICAL_UNIT;
        assert_eq!(format_nav_distance(500.0), "500 m");
        assert_eq!(format_nav_distance(15_000.0), "15.0 km");
        assert_eq!(format_nav_distance(150_000_000_000.0), "1.00 AU (8.3 lm)");
        assert_eq!(format_nav_distance(40.0 * au), "40.00 AU (5.54 lh)");
        assert_eq!(format_nav_distance(500.0 * au), "500.00 AU (2.89 ld)");
    }

    #[test]
    fn test_format_eta() {
        assert_eq!(format_eta(45.0), "00m 45s");
        assert_eq!(format_eta(125.0), "02m 05s");
        assert_eq!(format_eta(3665.0), "01h 01m 05s");
        assert_eq!(format_eta(90065.0), "1d 01h 01m 05s");
    }

    #[test]
    fn test_autopilot_locks_target_cycling() {
        let mut app = App::new();
        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::KeyT); // Attempt to cycle target
        app.insert_resource(keyboard);
        app.insert_resource(NavigationComputer {
            selected_index: Some(0),
            ..default()
        });

        // Spawn targets
        app.world_mut().spawn((
            NavTarget { name: "Target 1", classification: "Planet", radius_m: 1000.0 },
            UniversalPosition(DVec3::new(0.0, 0.0, 1000.0)),
        ));
        app.world_mut().spawn((
            NavTarget { name: "Target 2", classification: "Moon", radius_m: 500.0 },
            UniversalPosition(DVec3::new(0.0, 0.0, 2000.0)),
        ));

        // Spawn shuttle in Lorentz Autopilot mode
        app.world_mut().spawn((
            Shuttle {
                autopilot_mode: crate::flight::AutopilotMode::Lorentz,
                ..default()
            },
            UniversalPosition::default(),
            Transform::default(),
            RelativisticState::default(),
        ));

        app.add_systems(Update, navigation_targeting_system);
        app.update();

        let nav = app.world().resource::<NavigationComputer>();
        // During autonomous flight, pressing T must NOT change target!
        assert_eq!(nav.selected_index, Some(0));
    }

    #[test]
    fn test_relative_velocity_and_closing_rate() {
        let mut app = App::new();
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(NavigationComputer {
            selected_index: Some(0),
            ..default()
        });

        // Target: Moon orbiting at (0, 0, 30_000 m/s) at distance 100,000 m along +Z
        app.world_mut().spawn((
            NavTarget { name: "Luna", classification: "Moon", radius_m: 1_737_400.0 },
            UniversalPosition(DVec3::new(0.0, 0.0, 100_000.0)),
            crate::celestial::BodyVelocity(DVec3::new(0.0, 0.0, 30_000.0)),
        ));

        // Shuttle moving at (0, 100.0, 30_500 m/s)
        // Relative velocity v_shuttle - v_target = (0, 100.0, 500.0)
        // Closing towards target (target is at +Z, shuttle is approaching at 500 m/s along Z)
        let c = 299_792_458.0;
        let v_shuttle = DVec3::new(0.0, 100.0, 30_500.0);
        let rap = crate::relativity::rapidity_from_velocity(v_shuttle, c);

        app.world_mut().spawn((
            Shuttle::default(),
            UniversalPosition(DVec3::ZERO),
            Transform::default(),
            RelativisticState {
                rapidity: rap,
                velocity: v_shuttle,
                ..default()
            },
        ));

        app.add_systems(Update, navigation_targeting_system);
        app.update();

        let nav = app.world().resource::<NavigationComputer>();
        assert!((nav.target_speed - 30_000.0).abs() < 1e-3, "Target speed must be 30,000 m/s, got {}", nav.target_speed);
        // Relative velocity magnitude: sqrt(100^2 + 500^2) = sqrt(260000) ~ 509.90 m/s
        let expected_rel_speed = (100.0_f64.powi(2) + 500.0_f64.powi(2)).sqrt();
        assert!((nav.relative_speed - expected_rel_speed).abs() < 1e-2, "Relative speed must be ~509.9 m/s, got {}", nav.relative_speed);
        // Radial velocity towards target: distance is decreasing at 500 m/s -> -500 m/s
        assert!((nav.radial_velocity - (-500.0)).abs() < 1e-2, "Radial velocity must be -500 m/s, got {}", nav.radial_velocity);
        assert!(nav.closing, "Target should be closing");
        // Lateral velocity: 100 m/s
        assert!((nav.lateral_velocity - 100.0).abs() < 1e-2, "Lateral drift must be 100 m/s, got {}", nav.lateral_velocity);
    }

    #[test]
    fn test_surface_altitude_and_touchdown() {
        let mut app = App::new();
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(NavigationComputer {
            selected_index: Some(0),
            ..default()
        });

        let moon_radius = 1_737_400.0;
        let moon_center_dist = moon_radius + 5_000.0; // 5,000 m (5 km) above surface

        // Target: Moon
        app.world_mut().spawn((
            NavTarget { name: "Luna", classification: "Moon", radius_m: moon_radius },
            UniversalPosition(DVec3::new(0.0, 0.0, moon_center_dist)),
            crate::celestial::BodyVelocity(DVec3::ZERO),
        ));

        // Shuttle at origin
        app.world_mut().spawn((
            Shuttle::default(),
            UniversalPosition(DVec3::ZERO),
            Transform::default(),
            RelativisticState::default(),
        ));

        app.add_systems(Update, navigation_targeting_system);
        app.update();

        let nav = app.world().resource::<NavigationComputer>();
        assert!((nav.distance_m - moon_center_dist).abs() < 1e-2, "Center distance must be 1,742,400 m");
        assert_eq!(nav.target_radius_m, moon_radius);
        assert!(nav.altitude_m.is_some(), "Altitude must be computed for bodies with radius");
        let alt = nav.altitude_m.unwrap();
        assert!((alt - 5_000.0).abs() < 1e-2, "Surface altitude must be 5,000 m, got {alt}");
    }

    #[test]
    fn test_haumea_ellipsoid_landed_zero_altitude_stability() {
        let mut app = App::new();
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(NavigationComputer {
            selected_index: Some(0),
            ..default()
        });

        let a = 1_161_000.0;
        let b = 513_000.0;
        let c = 852_000.0;
        let haumea_ellipsoid = crate::celestial::EllipsoidRadii::new(a, b, c);

        let haumea_ent = app.world_mut().spawn((
            NavTarget { name: "Haumea", classification: "Dwarf Planet", radius_m: 816_000.0 },
            UniversalPosition(DVec3::ZERO),
            crate::celestial::BodyVelocity(DVec3::ZERO),
            haumea_ellipsoid,
            Transform::from_rotation(Quat::IDENTITY),
        )).id();

        // Shuttle landed on Haumea
        app.world_mut().spawn((
            Shuttle {
                landed_body: Some(haumea_ent),
                landed_local_normal: DVec3::Y,
                ..default()
            },
            UniversalPosition(DVec3::new(0.0, b, 0.0)),
            Transform::default(),
            RelativisticState::default(),
        ));

        app.add_systems(Update, navigation_targeting_system);
        app.update();

        let nav = app.world().resource::<NavigationComputer>();
        assert_eq!(nav.target_entity, Some(haumea_ent));
        assert_eq!(nav.altitude_m, Some(0.0), "Landed altitude on targeted ellipsoid must be exactly 0.0 m");
        assert_eq!(nav.closest_altitude_m, Some(0.0), "Closest body altitude must be exactly 0.0 m");
        assert_eq!(nav.radial_velocity, 0.0, "Landed radial velocity must be 0.0 m/s");
    }

    #[test]
    fn test_line_of_sight_occlusion() {
        let mut app = App::new();
        app.insert_resource(ButtonInput::<KeyCode>::default());
        // Select Luna (index 1)
        app.insert_resource(NavigationComputer {
            selected_index: Some(1),
            ..default()
        });

        let earth_radius = 6_371_000.0;
        let moon_radius = 1_737_400.0;

        // Obstacle: Earth at (0, 0, 20_000_000.0)
        app.world_mut().spawn((
            NavTarget { name: "Earth", classification: "Planet", radius_m: earth_radius },
            UniversalPosition(DVec3::new(0.0, 0.0, 20_000_000.0)),
            crate::celestial::BodyVelocity(DVec3::ZERO),
        ));

        // Target: Luna at (0, 0, 384_000_000.0) directly behind Earth
        app.world_mut().spawn((
            NavTarget { name: "Luna", classification: "Moon", radius_m: moon_radius },
            UniversalPosition(DVec3::new(0.0, 0.0, 384_000_000.0)),
            crate::celestial::BodyVelocity(DVec3::ZERO),
        ));

        // Shuttle at origin (0, 0, 0)
        let shuttle_entity = app.world_mut().spawn((
            Shuttle::default(),
            UniversalPosition(DVec3::ZERO),
            Transform::default(),
            RelativisticState::default(),
        )).id();

        app.add_systems(Update, navigation_targeting_system);
        app.update();

        let nav = app.world().resource::<NavigationComputer>();
        assert!(nav.is_occluded, "Luna behind Earth must be flagged as occluded");
        assert_eq!(nav.occluded_by_name.as_deref(), Some("Earth"), "Occluding body name must be Earth");

        // Now move the shuttle sideways to (50,000,000, 0, 0) where LOS to Luna is clear
        let mut shuttle_pos = app.world_mut().get_mut::<UniversalPosition>(shuttle_entity).unwrap();
        shuttle_pos.0 = DVec3::new(50_000_000.0, 0.0, 0.0);

        app.update();

        let nav = app.world().resource::<NavigationComputer>();
        assert!(!nav.is_occluded, "Luna from offset angle must not be occluded");
        assert_eq!(nav.occluded_by_name, None);

        // Verify the 15% safety buffer:
        // Ray passing close to Earth limb within 1.15 corridor triggers occlusion
        let mut shuttle_pos = app.world_mut().get_mut::<UniversalPosition>(shuttle_entity).unwrap();
        shuttle_pos.0 = DVec3::new(earth_radius * 1.08, 0.0, 0.0);
        app.update();
        let nav = app.world().resource::<NavigationComputer>();
        assert!(nav.is_occluded, "Ray passing at 1.08x radius must be flagged by 1.15 safety corridor");

        // Ray passing well outside 1.15 corridor (X = 1.30x Earth radius -> dist ~ 1.23x R at Earth) is clear
        let mut shuttle_pos = app.world_mut().get_mut::<UniversalPosition>(shuttle_entity).unwrap();
        shuttle_pos.0 = DVec3::new(earth_radius * 1.30, 0.0, 0.0);
        app.update();
        let nav = app.world().resource::<NavigationComputer>();
        assert!(!nav.is_occluded, "Ray passing at 1.30x radius must not be occluded");
    }

    #[test]
    fn test_closest_body_proximity_telemetry() {
        let mut app = App::new();
        app.insert_resource(ButtonInput::<KeyCode>::default());
        // Target is Sol (index 0 in astronomical hierarchy) at (0, 0, 150_000_000_000.0)
        app.insert_resource(NavigationComputer {
            selected_index: Some(0),
            ..default()
        });

        let venus_radius = 6_051_800.0;
        let sol_radius = 696_340_000.0;

        // Closest body: Venus at (0, 0, 0)
        let venus_ent = app.world_mut().spawn((
            NavTarget { name: "Venus", classification: "Planet", radius_m: venus_radius },
            UniversalPosition(DVec3::ZERO),
            crate::celestial::BodyVelocity(DVec3::ZERO),
        )).id();

        // Target: Sol far away
        app.world_mut().spawn((
            NavTarget { name: "Sol", classification: "Star", radius_m: sol_radius },
            UniversalPosition(DVec3::new(0.0, 0.0, 150_000_000_000.0)),
            crate::celestial::BodyVelocity(DVec3::ZERO),
        ));

        // Shuttle at (0, 0, venus_radius + 1_250.0) -> altitude 1,250 m above Venus
        app.world_mut().spawn((
            Shuttle::default(),
            UniversalPosition(DVec3::new(0.0, 0.0, venus_radius + 1_250.0)),
            Transform::default(),
            RelativisticState::default(),
        ));

        app.add_systems(Update, navigation_targeting_system);
        app.update();

        let nav = app.world().resource::<NavigationComputer>();
        // Target is Sol
        assert_eq!(nav.target_name, "Sol");
        // But closest body is Venus
        assert_eq!(nav.closest_body_entity, Some(venus_ent));
        assert_eq!(nav.closest_body_name.as_deref(), Some("Venus"));
        assert!(nav.closest_altitude_m.is_some());
        let alt = nav.closest_altitude_m.unwrap();
        assert!((alt - 1_250.0).abs() < 1e-2, "Closest altitude above Venus must be 1,250 m, got {alt}");
    }

    #[test]
    fn test_point_and_select_target_ahead() {
        let mut app = App::new();
        let mut kb = ButtonInput::<KeyCode>::default();
        kb.press(KeyCode::KeyG); // Press 'G' to select target under crosshairs
        app.insert_resource(kb);
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.insert_resource(crate::relativity::RelativisticOpticsConfig::default());
        app.insert_resource(NavigationComputer::default());

        // Spawn Target 1 off to the side (+X: 10,000, 0, 0)
        app.world_mut().spawn((
            NavTarget { name: "Target 1", classification: "Planet", radius_m: 1000.0 },
            UniversalPosition(DVec3::new(10_000.0, 0.0, 0.0)),
        ));

        // Spawn Target 2 straight ahead (+Z: 0, 0, 10,000)
        app.world_mut().spawn((
            NavTarget { name: "Target 2", classification: "Moon", radius_m: 500.0 },
            UniversalPosition(DVec3::new(0.0, 0.0, 10_000.0)),
        ));

        // Shuttle at origin looking at +Z (rotated 180 deg around Y so forward -Z points at +Z)
        let rot_to_pos_z = Quat::from_rotation_y(std::f32::consts::PI);
        app.world_mut().spawn((
            Shuttle::default(),
            UniversalPosition(DVec3::ZERO),
            Transform::from_rotation(rot_to_pos_z),
            RelativisticState::default(),
        ));

        // Camera looking at +Z
        app.world_mut().spawn((
            crate::camera::SpaceCamera::default(),
            Camera::default(),
            Transform::from_rotation(rot_to_pos_z),
            GlobalTransform::default(),
        ));

        app.add_systems(Update, navigation_targeting_system);
        app.update();

        let nav = app.world().resource::<NavigationComputer>();
        // Target 2 is straight ahead in camera view, so 'G' must select Target 2 (index 1)
        assert_eq!(nav.selected_index, Some(1), "Point & Select with 'G' must lock onto target ahead in crosshairs");
        assert_eq!(nav.target_name, "Target 2");

        // Shift + G clears selection
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.release(KeyCode::KeyG);
            kb.press(KeyCode::ShiftLeft);
            kb.press(KeyCode::KeyG);
        }
        app.update();
        let nav2 = app.world().resource::<NavigationComputer>();
        assert_eq!(nav2.selected_index, None, "Shift + G must clear target selection");
    }

    #[test]
    fn test_pending_target_entity_and_t_cycling_no_lockup() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.insert_resource(NavigationComputer::default());

        let t1 = app.world_mut().spawn((
            NavTarget { name: "Earth", classification: "Terrestrial", radius_m: 6_371_000.0 },
            UniversalPosition(DVec3::new(100.0, 0.0, 0.0)),
        )).id();

        let t2 = app.world_mut().spawn((
            NavTarget { name: "Mars", classification: "Terrestrial", radius_m: 3_389_500.0 },
            UniversalPosition(DVec3::new(200.0, 0.0, 0.0)),
        )).id();

        app.world_mut().spawn((
            Shuttle::default(),
            UniversalPosition(DVec3::ZERO),
            Transform::IDENTITY,
            RelativisticState::default(),
        ));

        app.add_systems(Update, navigation_targeting_system);

        // 1. External UI (e.g. Radar map click) requests Mars (t2)
        app.world_mut().resource_mut::<NavigationComputer>().pending_target_entity = Some(t2);
        app.update();

        let nav = app.world().resource::<NavigationComputer>();
        assert_eq!(nav.target_entity, Some(t2));
        assert_eq!(nav.target_name, "Mars");
        assert_eq!(nav.selected_index, Some(1));

        // 2. Press 'T' to cycle target. It must switch to Earth (index 0) without being reverted!
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.press(KeyCode::KeyT);
        }
        app.update();

        let nav_after_t = app.world().resource::<NavigationComputer>();
        assert_eq!(nav_after_t.selected_index, Some(0), "'T' must advance target cycle without getting stuck");
        assert_eq!(nav_after_t.target_name, "Earth");
        assert_eq!(nav_after_t.target_entity, Some(t1));
    }

    #[test]
    fn test_autopilot_blocks_radar_target_selection() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.insert_resource(NavigationComputer {
            selected_index: Some(0),
            ..default()
        });

        let t1 = app.world_mut().spawn((
            NavTarget { name: "Earth", classification: "Terrestrial", radius_m: 6_371_000.0 },
            UniversalPosition(DVec3::new(100.0, 0.0, 0.0)),
        )).id();

        let t2 = app.world_mut().spawn((
            NavTarget { name: "Mars", classification: "Terrestrial", radius_m: 3_389_500.0 },
            UniversalPosition(DVec3::new(200.0, 0.0, 0.0)),
        )).id();

        let shuttle_ent = app.world_mut().spawn((
            Shuttle {
                autopilot_mode: crate::flight::AutopilotMode::Hyperdrive,
                ..default()
            },
            UniversalPosition(DVec3::ZERO),
            Transform::IDENTITY,
            RelativisticState::default(),
        )).id();

        app.add_systems(Update, navigation_targeting_system);
        app.update();

        // Target should be Earth (t1) initially
        let nav = app.world().resource::<NavigationComputer>();
        assert_eq!(nav.target_entity, Some(t1));

        // Attempt to select Mars (t2) via radar map click during Hyperdrive autopilot
        app.world_mut().resource_mut::<NavigationComputer>().pending_target_entity = Some(t2);
        app.update();

        // Autopilot must block target change!
        let nav_blocked = app.world().resource::<NavigationComputer>();
        assert_eq!(nav_blocked.target_entity, Some(t1), "Hyperdrive autopilot must prevent changing target via Radar map click");
        assert_eq!(nav_blocked.pending_target_entity, None, "Pending target must be discarded");

        // Same for Lorentz autopilot
        app.world_mut().entity_mut(shuttle_ent).get_mut::<Shuttle>().unwrap().autopilot_mode = crate::flight::AutopilotMode::Lorentz;
        app.world_mut().resource_mut::<NavigationComputer>().pending_target_entity = Some(t2);
        app.update();

        let nav_lorentz = app.world().resource::<NavigationComputer>();
        assert_eq!(nav_lorentz.target_entity, Some(t1), "Lorentz autopilot must prevent changing target via Radar map click");
        assert_eq!(nav_lorentz.pending_target_entity, None);
    }

    #[test]
    fn test_sort_nav_targets_astronomical_order() {
        let mut app = App::new();

        let sol = app.world_mut().spawn(NavTarget { name: "Sol", classification: "Star", radius_m: 696_340_000.0 }).id();
        let earth = app.world_mut().spawn(NavTarget { name: "Earth", classification: "Planet", radius_m: 6_371_000.0 }).id();
        let moon = app.world_mut().spawn(NavTarget { name: "Luna", classification: "Moon", radius_m: 1_737_400.0 }).id();
        let mars = app.world_mut().spawn(NavTarget { name: "Mars", classification: "Planet", radius_m: 3_389_500.0 }).id();
        let jupiter = app.world_mut().spawn(NavTarget { name: "Jupiter", classification: "Gas Giant", radius_m: 69_911_000.0 }).id();
        let io = app.world_mut().spawn(NavTarget { name: "Io", classification: "Moon", radius_m: 1_821_600.0 }).id();
        let europa = app.world_mut().spawn(NavTarget { name: "Europa", classification: "Moon", radius_m: 1_560_800.0 }).id();
        let gate = app.world_mut().spawn(NavTarget { name: "Gate Alpha", classification: "Artifact", radius_m: 100.0 }).id();

        let earth_orbit = crate::celestial::KeplerOrbit {
            parent: Some(sol),
            semi_major_axis: 1.0 * 149_597_870_700.0,
            ..default()
        };
        let moon_orbit = crate::celestial::KeplerOrbit {
            parent: Some(earth),
            semi_major_axis: 384_400_000.0,
            ..default()
        };
        let mars_orbit = crate::celestial::KeplerOrbit {
            parent: Some(sol),
            semi_major_axis: 1.524 * 149_597_870_700.0,
            ..default()
        };
        let jupiter_orbit = crate::celestial::KeplerOrbit {
            parent: Some(sol),
            semi_major_axis: 5.204 * 149_597_870_700.0,
            ..default()
        };
        let io_orbit = crate::celestial::KeplerOrbit {
            parent: Some(jupiter),
            semi_major_axis: 421_700_000.0,
            ..default()
        };
        let europa_orbit = crate::celestial::KeplerOrbit {
            parent: Some(jupiter),
            semi_major_axis: 670_900_000.0,
            ..default()
        };

        // Create an intentionally shuffled list of targets
        struct TargetItem {
            ent: Entity,
            name: &'static str,
            orbit: Option<crate::celestial::KeplerOrbit>,
            dist: f64,
        }

        let mut items = vec![
            TargetItem { ent: europa, name: "Europa", orbit: Some(europa_orbit), dist: 5.2 * 149e9 },
            TargetItem { ent: mars, name: "Mars", orbit: Some(mars_orbit), dist: 1.524 * 149e9 },
            TargetItem { ent: gate, name: "Gate Alpha", orbit: None, dist: 1000.0 },
            TargetItem { ent: moon, name: "Luna", orbit: Some(moon_orbit), dist: 1.0 * 149e9 },
            TargetItem { ent: sol, name: "Sol", orbit: None, dist: 0.0 },
            TargetItem { ent: io, name: "Io", orbit: Some(io_orbit), dist: 5.2 * 149e9 },
            TargetItem { ent: earth, name: "Earth", orbit: Some(earth_orbit), dist: 1.0 * 149e9 },
            TargetItem { ent: jupiter, name: "Jupiter", orbit: Some(jupiter_orbit), dist: 5.2 * 149e9 },
        ];

        sort_nav_targets(&mut items, |it| (it.ent, it.name, it.orbit.as_ref(), it.dist));

        let sorted_names: Vec<&str> = items.iter().map(|it| it.name).collect();
        assert_eq!(
            sorted_names,
            vec!["Sol", "Earth", "Luna", "Mars", "Jupiter", "Io", "Europa", "Gate Alpha"],
            "Targets must sort in exact astronomical hierarchy: Sol -> Earth -> Luna -> Mars -> Jupiter -> Io -> Europa -> Gate"
        );
    }
}
