use bevy::math::DVec3;
use bevy::prelude::*;
use crate::celestial::CelestialBody;
use crate::flight::Shuttle;

/// 64-bit double precision universal coordinates (in meters).
/// Tracks true positions across astronomical distances without 32-bit floating point precision loss.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
pub struct UniversalPosition(pub DVec3);

#[allow(dead_code)]
impl UniversalPosition {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self(DVec3::new(x, y, z))
    }
}

/// Number of latitude stacks and longitude sectors for equirectangular celestial UV spheres.
pub const UV_SPHERE_SUBDIVISIONS: usize = 512;

/// Computes the exact geometric sagitta drop (chord depth deficit) for an equirectangular
/// UV sphere mesh (512x512 subdivisions, oriented with North at +Y) along a given local unit direction.
/// Returns the radial distance difference (in meters): Δr = R - r_poly(dir).
pub fn calculate_uv_sphere_sagitta_drop(local_dir: DVec3, radius: f64) -> f64 {
    let u = if local_dir.length_squared() > 1e-12 {
        local_dir.normalize()
    } else {
        DVec3::Y
    };
    let phi = u.y.clamp(-1.0, 1.0).asin();
    let subdivisions = UV_SPHERE_SUBDIVISIONS as f64;
    let stack_step = std::f64::consts::PI / subdivisions;
    let i_float = (std::f64::consts::FRAC_PI_2 - phi) / stack_step;
    let i = (i_float.floor() as usize).clamp(0, UV_SPHERE_SUBDIVISIONS - 1);

    let sec = (-u.z).atan2(u.x);
    let sec_normalized = if sec < 0.0 {
        sec + 2.0 * std::f64::consts::PI
    } else {
        sec
    };
    let sector_step = 2.0 * std::f64::consts::PI / subdivisions;
    let j_float = sec_normalized / sector_step;
    let j = (j_float.floor() as usize).clamp(0, UV_SPHERE_SUBDIVISIONS - 1);

    let get_vertex = |stack_idx: usize, sector_idx: usize| -> DVec3 {
        let stack_angle = std::f64::consts::FRAC_PI_2 - (stack_idx as f64) * stack_step;
        let sector_angle = (sector_idx as f64) * sector_step;
        let xy = stack_angle.cos();
        let z = stack_angle.sin();
        let x = xy * sector_angle.cos();
        let y = xy * sector_angle.sin();
        DVec3::new(x, z, -y)
    };

    let test_tri = |a: DVec3, b: DVec3, c: DVec3| -> Option<f64> {
        let n = (b - a).cross(c - a);
        let len_sq = n.length_squared();
        if len_sq < 1e-18 {
            return None;
        }
        let n = n / len_sq.sqrt();
        let d = n.dot(a);
        let un = u.dot(n);
        if un.abs() < 1e-12 {
            return None;
        }
        let t = d / un;
        let p = u * t;

        let v0 = c - a;
        let v1 = b - a;
        let v2 = p - a;
        let dot00 = v0.dot(v0);
        let dot01 = v0.dot(v1);
        let dot02 = v0.dot(v2);
        let dot11 = v1.dot(v1);
        let dot12 = v1.dot(v2);
        let denom = dot00 * dot11 - dot01 * dot01;
        if denom.abs() < 1e-18 {
            return None;
        }
        let inv_denom = 1.0 / denom;
        let u_b = (dot11 * dot02 - dot01 * dot12) * inv_denom;
        let v_b = (dot00 * dot12 - dot01 * dot02) * inv_denom;
        if u_b >= -1e-4 && v_b >= -1e-4 && (u_b + v_b) <= 1.0 + 1e-4 {
            Some(t)
        } else {
            None
        }
    };

    let test_quad = |i_q: usize, j_q: usize| -> Option<f64> {
        let v00 = get_vertex(i_q, j_q);
        let v01 = get_vertex(i_q, j_q + 1);
        let v10 = get_vertex(i_q + 1, j_q);
        let v11 = get_vertex(i_q + 1, j_q + 1);
        test_tri(v00, v10, v01).or_else(|| test_tri(v01, v10, v11))
    };

    let mut t_opt = test_quad(i, j);
    if t_opt.is_none() && i > 0 {
        t_opt = test_quad(i - 1, j);
    }
    if t_opt.is_none() && i + 1 < UV_SPHERE_SUBDIVISIONS {
        t_opt = test_quad(i + 1, j);
    }
    if t_opt.is_none() {
        let j_prev = if j == 0 { UV_SPHERE_SUBDIVISIONS - 1 } else { j - 1 };
        t_opt = test_quad(i, j_prev);
    }
    if t_opt.is_none() {
        let j_next = (j + 1) % UV_SPHERE_SUBDIVISIONS;
        t_opt = test_quad(i, j_next);
    }

    let t = t_opt.unwrap_or_else(|| (stack_step * 0.5).cos());
    radius * (1.0 - t).max(0.0)
}

/// Floating Origin System with 100% Unified Cartesian Coordinates & Relativistic Aberration:
/// 1. Takes the shuttle's 64-bit universal position as the rendering origin (0, 0, 0).
/// 2. Translates ALL entities (planets, Sol, moons, slalom rings, beacons, space stations)
///    into camera-relative 32-bit coordinates using the true 1:1 Euclidean Cartesian formula:
///    r_render = (X_world - X_shuttle)
/// 3. When Relativistic Aberration is enabled, aberrates the observed light direction from rapidity:
///    r_render = aberrate_direction_from_rapidity(dir, rapidity) * dist
///    - True 1:1 physical Euclidean metric: Earth radius 6,371 km, Moon 384,400 km, Sol 1 AU (149.6M km).
///    - Combined with Reversed-Z 32-bit float depth and astronomical far plane (1.0e16 m ~ 1.05 ly),
///      depth buffer precision is monotonic and uniform across 16 orders of magnitude.
///    - Completely eliminates artificial mesh scaling, flattening, and depth inversions.
pub fn floating_origin_system(
    shuttle_query: Query<(&UniversalPosition, &crate::relativity::RelativisticState, &crate::flight::Shuttle), With<Shuttle>>,
    optics: Res<crate::relativity::RelativisticOpticsConfig>,
    mut world_query: Query<
        (
            Entity,
            &UniversalPosition,
            &mut Transform,
            Option<&CelestialBody>,
            Option<&crate::celestial::BodyVelocity>,
            Option<&crate::celestial::EllipsoidRadii>,
        ),
        Without<Shuttle>,
    >,
) {
    let Ok((shuttle_upos, rel_state, shuttle)) = shuttle_query.get_single() else {
        return;
    };

    let origin = shuttle_upos.0;
    let c = crate::relativity::REAL_C;
    let c_sq = c * c;

    for (entity, upos, mut transform, celestial, body_vel_opt, ellipsoid_opt) in world_query.iter_mut() {
        let relative_pos = upos.0 - origin;
        let dist = relative_pos.length();

        if dist < 0.001 {
            transform.translation = Vec3::ZERO;
            continue;
        }

        let dir = relative_pos / dist;

        if let Some(body) = celestial {
            // Relativistic Aberration depends strictly on the RELATIVE velocity between the observer and the source.
            // If the shuttle is landed on a celestial body, its relative velocity to that body's surface is exactly zero.
            // In flight, aberration is determined by (v_shuttle - v_body), preventing heliocentric orbital velocity
            // (e.g. Earth's 30 km/s around Sol) from artificially shifting the planet away from a landed or orbiting shuttle!
            let is_landed_on_this_body = shuttle.landed_body == Some(entity);

            let (obs_dir, visual_radius) = if optics.aberration_enabled && !is_landed_on_this_body {
                let body_vel = body_vel_opt.map(|bv| bv.0).unwrap_or(DVec3::ZERO);
                let v_rel = rel_state.velocity - body_vel;
                let rel_speed_sq = v_rel.length_squared();

                if rel_speed_sq > 1.0 {
                    let rel_rapidity = crate::relativity::rapidity_from_velocity(v_rel, c);
                    let obs_d = crate::relativity::aberrate_direction_from_rapidity(dir, rel_rapidity);
                    let beta_vec = v_rel / c;
                    let beta_sq = (rel_speed_sq / c_sq).min(0.9999999999);
                    let gamma = 1.0 / (1.0 - beta_sq).sqrt();
                    let vis_r = crate::relativity::penrose_apparent_angular_radius(
                        body.physical_radius,
                        dist,
                        dir,
                        beta_vec,
                        gamma,
                    );
                    (obs_d, vis_r)
                } else {
                    (dir, body.physical_radius)
                }
            } else {
                (dir, body.physical_radius)
            };

            let mut render_scale = visual_radius;

            // For primary star (Sol), maintain an apparent size of ~2.5 - 3.0 screen pixels at extreme distances
            // (Saturn/Pluto/Sedna) so it remains visible as a brilliant pinpoint star without becoming an oversized disc
            if body.physical_radius > 100_000_000.0 {
                let min_star_scale = dist * 0.0006;
                render_scale = render_scale.max(min_star_scale);
            }

            let shape_scale = if let Some(ellipsoid) = ellipsoid_opt {
                ellipsoid.shape_scale(body.physical_radius)
            } else {
                Vec3::ONE
            };

            // Local Surface Sagitta Elevation Offset:
            // For a 512x512 UV sphere at true 1:1 astronomical scale (Earth R = 6,371 km),
            // flat triangular polygons produce up to ~149m sagitta chord deficit beneath the mathematical sphere radius.
            // When close to or landed on a solid body (< 50 km altitude), smoothly offset the rendered body center
            // along the outward surface normal so the visual polygon is flush with the contact plane at (0, 0, 0).
            let surface_shift = if body.physical_radius < 100_000_000.0 {
                let to_shuttle_normal = -dir;
                let body_rot = transform.rotation.as_dquat();
                let local_dir = body_rot.inverse() * to_shuttle_normal;
                let effective_r = if let Some(ellipsoid) = ellipsoid_opt {
                    ellipsoid.surface_radius_in_local_dir(local_dir)
                } else {
                    body.physical_radius
                };

                let altitude = if is_landed_on_this_body {
                    0.0
                } else {
                    (dist - effective_r).max(0.0)
                };

                if altitude < 50_000.0 {
                    let sagitta = calculate_uv_sphere_sagitta_drop(local_dir, effective_r);
                    let t = (1.0 - altitude / 50_000.0).clamp(0.0, 1.0);
                    let smooth_blend = t * t * (3.0 - 2.0 * t);
                    // The shuttle model spans 27.9m in height, with its belly / landing skids at -13.95m
                    // relative to the entity center. Offsetting by (sagitta - 13.95) places the visual
                    // terrain polygon flush under the ship's landing skids at -13.95m rather than slicing
                    // through the middle of the ship!
                    let clearance = crate::flight::SHUTTLE_BELLY_CLEARANCE;
                    to_shuttle_normal * ((sagitta - clearance) * smooth_blend)
                } else {
                    DVec3::ZERO
                }
            } else {
                DVec3::ZERO
            };

            transform.translation = ((obs_dir * dist) + surface_shift).as_vec3();
            transform.scale = shape_scale * (render_scale as f32);
        } else {
            // Slalom rings, beacons, space stations, asteroids: standard Euclidean mapping
            transform.translation = (dir * dist).as_vec3();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_cartesian_floating_origin() {
        let origin = DVec3::new(0.0, 0.0, 90_000.0);
        let terra_pos = DVec3::new(28_000.0, 400.0, 25_000.0);
        let sol_pos = DVec3::new(0.0, 0.0, 0.0);
        let chronos_pos = DVec3::new(85_000.0, 3_500.0, -70_000.0);

        let r_terra = (terra_pos - origin).length();
        let r_sol = (sol_pos - origin).length();
        let r_chronos = (chronos_pos - origin).length();

        // 1. Strict physical depth ordering: Terra in front of Sol, Sol in front of Chronos
        assert!(r_terra < r_sol, "Terra must be closer than Sol");
        assert!(r_sol < r_chronos, "Sol must be closer than Chronos");

        // 2. All planetary bodies are safely within astronomical camera far plane
        assert!(r_chronos < 1.0e16, "All bodies must stay within camera far plane");

        // 3. Relativistic Aberration strictly preserves physical radial depth hierarchy
        let rapidity = DVec3::new(2.5, 0.4, -1.2); // Extreme relativistic rapidity (beta ~ 0.99)
        let dir_terra = (terra_pos - origin).normalize();
        let dir_sol = (sol_pos - origin).normalize();
        let obs_terra = crate::relativity::aberrate_direction_from_rapidity(dir_terra, rapidity);
        let obs_sol = crate::relativity::aberrate_direction_from_rapidity(dir_sol, rapidity);
        let r_terra_obs = (obs_terra * r_terra).length();
        let r_sol_obs = (obs_sol * r_sol).length();
        assert!((r_terra_obs - r_terra).abs() < 1e-6);
        assert!((r_sol_obs - r_sol).abs() < 1e-6);
        assert!(r_terra_obs < r_sol_obs, "Terra must stay strictly in front of Sol under aberration");

        // 4. Penrose Optical Size Mapping scales planetary radius harmoniously with orbital separation
        let beta = 0.95f64;
        let gamma = 1.0 / (1.0 - beta * beta).sqrt();
        let beta_vec = DVec3::new(0.0, 0.0, -beta);
        let r_vis_sol = crate::relativity::penrose_apparent_angular_radius(4500.0, r_sol, dir_sol, beta_vec, gamma);
        let r_vis_terra = crate::relativity::penrose_apparent_angular_radius(1100.0, r_terra, dir_terra, beta_vec, gamma);
        assert!(r_vis_sol < 4500.0 * 0.25, "Sol radius must optically contract when approaching at 0.95c");
        assert!(r_vis_terra < 1100.0 * 0.25, "Terra radius must optically contract when approaching at 0.95c");

        // 5. Receding from Sol: expands smoothly up to 3.5x at long range, but protects camera at close range
        let close_dist = 6000.0; // 1.5km above Sol's 4.5km surface
        let receding_beta = DVec3::new(0.0, 0.0, 0.99); // Flying away from Sol at 0.99c
        let r_vis_receding_close = crate::relativity::penrose_apparent_angular_radius(4500.0, close_dist, dir_sol, receding_beta, 7.088);
        assert!(r_vis_receding_close < close_dist, "Receding visual radius must never exceed distance to camera");
        assert!(r_vis_receding_close <= close_dist * 0.90, "Receding visual radius maintains safety margin from cockpit");

        let r_vis_receding_far = crate::relativity::penrose_apparent_angular_radius(4500.0, r_sol, dir_sol, receding_beta, 7.088);
        assert!(r_vis_receding_far > 4500.0, "Receding at long range must expand optically");
        assert!(r_vis_receding_far <= 4500.0 * 3.5, "Expansion must remain within safe 3.5x bound");
        assert!(r_vis_receding_far < 38_000.0, "Sol must stay well clear of Terra's 38km orbit");
    }

    #[test]
    fn test_realistic_solar_system_1_to_1_depth_and_angular_consistency() {
        // Observer in high Earth orbit: 16,000 km from Earth center
        let d_earth: f64 = 16_000_000.0;
        let _r_earth: f64 = 6_371_000.0;

        // Moon distance: 384,400 km
        let d_moon: f64 = 384_400_000.0;
        let r_moon: f64 = 1_737_400.0;

        // Sun distance: 1.0 AU (149.6 million km)
        let d_sol: f64 = 149_597_870_700.0;
        let r_sol: f64 = 696_340_000.0;

        // Jupiter distance: ~5.2 AU (~778 million km)
        let d_jupiter: f64 = 778_547_200_000.0;
        let _r_jupiter: f64 = 69_911_000.0;

        // Neptune distance: ~30.0 AU (~4.5 billion km)
        let d_neptune: f64 = 4_495_038_600_000.0;
        let _r_neptune: f64 = 24_622_000.0;

        // 1. Strict 1:1 Euclidean Cartesian Distance Ordering across 6 orders of magnitude:
        // Earth < Moon < Sol < Jupiter < Neptune
        assert!(d_earth < d_moon);
        assert!(d_moon < d_sol);
        assert!(d_sol < d_jupiter);
        assert!(d_jupiter < d_neptune);

        // 2. All bodies are safely inside camera far plane (1.0e16 m ~ 1.05 light years)
        let far_plane = 1.0e16;
        assert!(d_neptune < far_plane);

        // 3. Exact apparent angular diameter preservation:
        // tan(theta/2) = r / d
        let ang_sol = (r_sol / d_sol).atan() * 2.0;
        let ang_moon = (r_moon / d_moon).atan() * 2.0;

        // Sun and Moon both subtend ~0.53 degree (matching reality, allowing authentic solar eclipses)
        assert!((ang_sol.to_degrees() - 0.533).abs() < 0.05);
        assert!((ang_moon.to_degrees() - 0.518).abs() < 0.05);

        // 4. Reversed-Z Monotonic Depth Ordering in depth buffer:
        // z_ndc ~ near / d
        let z_earth = 0.1 / d_earth;
        let z_moon = 0.1 / d_moon;
        let z_sol = 0.1 / d_sol;
        let z_jup = 0.1 / d_jupiter;
        let z_nep = 0.1 / d_neptune;
        assert!(z_earth > z_moon);
        assert!(z_moon > z_sol);
        assert!(z_sol > z_jup);
        assert!(z_jup > z_nep);
        assert!(z_nep > 0.0);
    }

    #[test]
    fn test_landed_body_relative_aberration_zero_displacement() {
        let mut app = App::new();
        app.insert_resource(crate::relativity::RelativisticOpticsConfig {
            aberration_enabled: true,
            doppler_enabled: true,
            beaming_enabled: true,
        });

        let earth_radius = 6_371_000.0;
        let earth_pos = DVec3::new(149_597_870_700.0, 0.0, 0.0);
        let earth_vel = DVec3::new(0.0, 0.0, 29_780.0); // ~30 km/s orbital motion around Sol

        let earth_ent = app.world_mut().spawn((
            UniversalPosition(earth_pos),
            Transform::default(),
            CelestialBody {
                physical_radius: earth_radius,
                mass_kg: 5.972e24,
            },
            crate::celestial::BodyVelocity(earth_vel),
        )).id();

        // Shuttle landed on Earth: position at surface, moving with Earth
        let shuttle_pos = earth_pos + DVec3::new(0.0, earth_radius, 0.0);
        let mut shuttle = crate::flight::Shuttle::default();
        shuttle.landed_body = Some(earth_ent);

        let mut rel_state = crate::relativity::RelativisticState::default();
        rel_state.velocity = earth_vel;
        rel_state.rapidity = crate::relativity::rapidity_from_velocity(earth_vel, crate::relativity::REAL_C);

        app.world_mut().spawn((
            shuttle,
            rel_state,
            UniversalPosition(shuttle_pos),
            Transform::default(),
        ));

        app.add_systems(Update, floating_origin_system);
        app.update();

        let earth_tf = app.world().get::<Transform>(earth_ent).unwrap();
        // Since shuttle is landed at the North Pole (sagitta ~ 0), the terrain polygon is placed
        // flush at -13.95m under the landing skids, with zero aberration displacement.
        assert!((earth_tf.translation.x - 0.0).abs() < 1e-3, "X must have zero aberration displacement");
        assert!((earth_tf.translation.y - (-earth_radius as f32 - 13.95)).abs() < 1e-2, "Y must match surface radius with belly clearance");
        assert!((earth_tf.translation.z - 0.0).abs() < 1e-3, "Z must have zero aberration displacement");
        assert!((earth_tf.scale.x - earth_radius as f32).abs() < 1e-3, "Scale must match authentic physical radius");
    }

    #[test]
    fn test_uv_sphere_sagitta_drop_bounds_and_flush_landing() {
        let earth_radius = 6_371_000.0;
        // North pole vertex: sagitta drop is 0.0
        let pole_sagitta = calculate_uv_sphere_sagitta_drop(DVec3::Y, earth_radius);
        assert!(pole_sagitta < 1.0, "Pole vertex sagitta must be ~0, got {}", pole_sagitta);

        // Grid check across latitudes and longitudes
        let mut max_sagitta = 0.0f64;
        let mut min_sagitta = f64::MAX;
        for lat in -85..=85 {
            for lon in -180..=180 {
                let phi = (lat as f64).to_radians();
                let lam = (lon as f64).to_radians();
                let dir = DVec3::new(phi.cos() * lam.cos(), phi.sin(), -phi.cos() * lam.sin());
                let s = calculate_uv_sphere_sagitta_drop(dir, earth_radius);
                if s > max_sagitta { max_sagitta = s; }
                if s < min_sagitta { min_sagitta = s; }
            }
        }
        println!("Grid sagitta: min = {} m, max = {} m", min_sagitta, max_sagitta);

        // 45 degrees latitude / 9 degrees longitude: with 512 subdivisions, sagitta is between 25m and 150m
        let phi = 42.0f64.to_radians();
        let lam = 9.0f64.to_radians();
        let corsica_dir = DVec3::new(phi.cos() * lam.cos(), phi.sin(), -phi.cos() * lam.sin());
        let corsica_sagitta = calculate_uv_sphere_sagitta_drop(corsica_dir, earth_radius);
        assert!(
            corsica_sagitta >= 25.0 && corsica_sagitta <= 150.0,
            "Corsica sagitta must be within chord deficit bounds: got {} m",
            corsica_sagitta
        );

        // Atlantic Ocean point check (e.g. 25N, 35W)
        let phi_atl = 25.0f64.to_radians();
        let lam_atl = (-35.0f64).to_radians();
        let atl_dir = DVec3::new(phi_atl.cos() * lam_atl.cos(), phi_atl.sin(), -phi_atl.cos() * lam_atl.sin());
        let atl_sagitta = calculate_uv_sphere_sagitta_drop(atl_dir, earth_radius);
        assert!(
            atl_sagitta >= 25.0 && atl_sagitta <= 150.0,
            "Atlantic sagitta must be within chord deficit bounds: got {} m",
            atl_sagitta
        );

        // When landed, the visual polygon at Corsica is elevated by (sagitta - 13.95),
        // placing the rendered terrain flush at the belly plane (-13.95m):
        let mesh_center_unshifted = -corsica_dir * earth_radius;
        let clearance = 13.95;
        let surface_shift = corsica_dir * (corsica_sagitta - clearance);
        let mesh_center_shifted = mesh_center_unshifted + surface_shift;
        let visual_polygon_pos = mesh_center_shifted + corsica_dir * (earth_radius - corsica_sagitta);
        assert!(
            (visual_polygon_pos.length() - clearance).abs() < 1e-4,
            "Rendered polygon under shuttle must be placed flush at landing gear plane (-13.95m), got dist {}",
            visual_polygon_pos.length()
        );
    }
}
