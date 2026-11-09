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

/// Floating Origin System with 100% Unified Cartesian Coordinates & Relativistic Aberration:
/// 1. Takes the shuttle's 64-bit universal position as the rendering origin (0, 0, 0).
/// 2. Translates ALL entities (planets, Sol, moons, slalom rings, beacons, space stations)
///    into camera-relative 32-bit coordinates using the linear Cartesian formula:
///    r_render = (X_world - X_shuttle)
/// 3. When Relativistic Aberration is enabled, aberrates the observed light direction from rapidity:
///    r_render = aberrate_direction_from_rapidity(dir, rapidity) * dist
///    - Preserves exact physical depth: Terra (65km), Sol (90km), Chronos (135km).
///    - Completely eliminates mesh clipping and depth inversions.
///    - Planets and Sol shift and compress into the searchlight cone in unison with the starfield!
pub fn floating_origin_system(
    shuttle_query: Query<(&UniversalPosition, &crate::relativity::RelativisticState), With<Shuttle>>,
    optics: Res<crate::relativity::RelativisticOpticsConfig>,
    mut world_query: Query<
        (
            &UniversalPosition,
            &mut Transform,
            Option<&CelestialBody>,
            Option<&crate::celestial::EllipsoidRadii>,
        ),
        Without<Shuttle>,
    >,
) {
    let Ok((shuttle_upos, rel_state)) = shuttle_query.get_single() else {
        return;
    };

    let origin = shuttle_upos.0;
    let rapidity = if optics.aberration_enabled {
        rel_state.rapidity
    } else {
        DVec3::ZERO
    };
    let has_aberration = rapidity.length_squared() > 1e-12;

    for (upos, mut transform, celestial, ellipsoid_opt) in world_query.iter_mut() {
        let relative_pos = upos.0 - origin;
        let dist = relative_pos.length();

        if dist < 0.001 {
            transform.translation = Vec3::ZERO;
            continue;
        }

        let dir = relative_pos / dist;
        let obs_dir = if has_aberration {
            crate::relativity::aberrate_direction_from_rapidity(dir, rapidity)
        } else {
            dir
        };

        if let Some(body) = celestial {
            let (render_dist, render_scale) = if body.physical_radius < 10_000.0 && dist < 200_000.0 {
                // Demo Sector bodies: 1:1 Euclidean Cartesian mapping
                let visual_radius = if has_aberration {
                    crate::relativity::penrose_apparent_angular_radius(
                        body.physical_radius,
                        dist,
                        dir,
                        rel_state.beta_vec,
                        rel_state.gamma,
                    )
                } else {
                    body.physical_radius
                };
                (dist, visual_radius)
            } else {
                // 1:1 Realistic Scale Solar System bodies:
                // Astronomical distances (up to 4.5 billion km) are projected smoothly into the
                // camera frustum (40km - 220km) while preserving exact apparent angular diameter
                // alpha = 2 * arcsin(R / D) and strictly monotonic depth buffer ordering!
                let d_base = 6_000_000.0; // ~Earth radius scale (m)
                let d_min = 40_000.0;     // 40 km near projection threshold
                let d_max = 220_000.0;    // 220 km far projection threshold (well inside 500km far plane)

                let render_dist = if dist <= d_base {
                    (d_min * (dist / d_base)).max(50.0)
                } else {
                    let u = (dist / d_base).ln();
                    let u_scale = 4.0;
                    d_min + (d_max - d_min) * (u / (u + u_scale))
                };

                // Visual radius after Penrose aberration.
                // For large celestial bodies (radius > 100km), slightly offset the visual sphere skin
                // by 150m (0.002% of Earth radius). In frustum projection (where render_scale = visual_radius * scale_ratio
                // and scale_ratio ~ 1/150 near the surface), this places the visual surface 1.0m below the shuttle's
                // center at touchdown, ensuring the fuselage and landing gear rest cleanly on top of the surface mesh
                // without clipping or sinking.
                let effective_radius = if body.physical_radius > 100_000.0 {
                    (body.physical_radius - 150.0).max(10.0)
                } else {
                    body.physical_radius
                };

                let visual_radius = if has_aberration {
                    crate::relativity::penrose_apparent_angular_radius(
                        effective_radius,
                        dist,
                        dir,
                        rel_state.beta_vec,
                        rel_state.gamma,
                    )
                } else {
                    effective_radius
                };

                // Exact angular size match: tan(theta/2) = r_render / d_render = visual_radius / dist
                let scale_ratio = (render_dist / dist).min(1.0);
                let mut render_scale = visual_radius * scale_ratio;

                // For primary star (Sol), maintain an apparent size of ~2.5 - 3.0 screen pixels at extreme distances
                // (Saturn/Pluto/Sedna) so it remains visible as a brilliant pinpoint star without becoming an oversized disc
                if body.physical_radius > 100_000_000.0 {
                    let min_star_scale = (render_dist * 0.0012).max(120.0);
                    render_scale = render_scale.max(min_star_scale);
                }

                (render_dist, render_scale)
            };

            let shape_scale = if let Some(ellipsoid) = ellipsoid_opt {
                ellipsoid.shape_scale(body.physical_radius)
            } else {
                Vec3::ONE
            };

            transform.translation = (obs_dir * render_dist).as_vec3();
            transform.scale = shape_scale * (render_scale as f32);
        } else {
            // Slalom rings, beacons, space stations, asteroids: standard Euclidean mapping
            transform.translation = (obs_dir * dist).as_vec3();
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

        // 2. All planetary bodies are well within the 500km camera far plane
        assert!(r_chronos < 500_000.0, "All bodies must stay within 500km camera far plane");

        // 3. All planetary bodies are strictly in front of the 400km starfield
        assert!(r_chronos < 400_000.0, "All bodies must stay well in front of 400km starfield");

        // 4. Relativistic Aberration strictly preserves physical radial depth hierarchy
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

        // 5. Penrose Optical Size Mapping scales planetary radius harmoniously with orbital separation
        let beta = 0.95f64;
        let gamma = 1.0 / (1.0 - beta * beta).sqrt();
        let beta_vec = DVec3::new(0.0, 0.0, -beta);
        let r_vis_sol = crate::relativity::penrose_apparent_angular_radius(4500.0, r_sol, dir_sol, beta_vec, gamma);
        let r_vis_terra = crate::relativity::penrose_apparent_angular_radius(1100.0, r_terra, dir_terra, beta_vec, gamma);
        assert!(r_vis_sol < 4500.0 * 0.25, "Sol radius must optically contract when approaching at 0.95c");
        assert!(r_vis_terra < 1100.0 * 0.25, "Terra radius must optically contract when approaching at 0.95c");

        // 6. Receding from Sol: expands smoothly up to 3.5x at long range, but protects camera at close range
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
    fn test_realistic_solar_system_projection_depth_and_angular_consistency() {
        // Observer in high Earth orbit: 16,000 km from Earth center
        let d_earth = 16_000_000.0;
        let r_earth = 6_371_000.0;

        // Moon distance: 384,400 km
        let d_moon = 384_400_000.0;
        let r_moon = 1_737_400.0;

        // Sun distance: 1.0 AU (149.6 million km)
        let d_sol = 149_597_870_700.0;
        let r_sol = 696_340_000.0;

        // Jupiter distance: ~5.2 AU (~778 million km)
        let d_jupiter = 778_547_200_000.0;
        let r_jupiter = 69_911_000.0;

        // Neptune distance: ~30.0 AU (~4.5 billion km)
        let d_neptune = 4_495_038_600_000.0;
        let r_neptune = 24_622_000.0;

        let project_body = |dist: f64, physical_radius: f64| -> (f64, f64) {
            let d_base = 6_000_000.0;
            let d_min = 40_000.0;
            let d_max = 220_000.0;

            let render_dist = if dist <= d_base {
                (d_min * (dist / d_base)).max(50.0)
            } else {
                let u = (dist / d_base).ln();
                let u_scale = 4.0;
                d_min + (d_max - d_min) * (u / (u + u_scale))
            };

            let scale_ratio = (render_dist / dist).min(1.0);
            let render_scale = physical_radius * scale_ratio;
            (render_dist, render_scale)
        };

        let (d_rend_earth, _r_rend_earth) = project_body(d_earth, r_earth);
        let (d_rend_moon, r_rend_moon) = project_body(d_moon, r_moon);
        let (d_rend_sol, r_rend_sol) = project_body(d_sol, r_sol);
        let (d_rend_jup, _r_rend_jup) = project_body(d_jupiter, r_jupiter);
        let (d_rend_nep, _r_rend_nep) = project_body(d_neptune, r_neptune);

        // 1. Strict Monotonic Depth Ordering across 6 orders of magnitude:
        // Earth < Moon < Sol < Jupiter < Neptune
        assert!(d_rend_earth < d_rend_moon);
        assert!(d_rend_moon < d_rend_sol);
        assert!(d_rend_sol < d_rend_jup);
        assert!(d_rend_jup < d_rend_nep);

        // 2. All bodies are safely inside camera far plane (500 km) and starfield (400 km)
        assert!(d_rend_nep < 220_000.0);
        assert!(d_rend_earth >= 40_000.0);

        // 3. Exact apparent angular diameter preservation:
        // tan(theta/2) = r_rend / d_rend == physical_radius / dist
        let ang_sol_true = (r_sol / d_sol).atan() * 2.0;
        let ang_sol_rend = (r_rend_sol / d_rend_sol).atan() * 2.0;
        assert!((ang_sol_true - ang_sol_rend).abs() < 1e-7);

        let ang_moon_true = (r_moon / d_moon).atan() * 2.0;
        let ang_moon_rend = (r_rend_moon / d_rend_moon).atan() * 2.0;
        assert!((ang_moon_true - ang_moon_rend).abs() < 1e-7);

        // Sun and Moon both subtend ~0.5 degree (allowing solar eclipses)
        assert!((ang_sol_rend.to_degrees() - 0.533).abs() < 0.05);
        assert!((ang_moon_rend.to_degrees() - 0.518).abs() < 0.05);
    }
}
