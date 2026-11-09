use bevy::math::DVec3;
use bevy::prelude::*;
use crate::coords::UniversalPosition;
use crate::flight::{EngineBayLight, EngineSlat, Shuttle, SimulationPause};
use crate::relativity::RelativisticState;

#[derive(Component)]
pub struct RotatingLandmark {
    pub rotation_axis: Vec3,
    pub speed: f32,
}

#[derive(Component)]
pub struct PulsingBeacon {
    pub base_emissive: LinearRgba,
    pub frequency: f32,
}

#[derive(Component)]
pub struct SunDirectionalLight;

/// Marker component for the dynamic Relativistic Cosmic Searchlight directional light,
/// which shines intense blueshifted starlight onto the shuttle as velocity approaches c.
#[derive(Component)]
pub struct CosmicSearchlightDirectionalLight;

/// Default spawn coordinates for Universe 1 (Scaled Demo Sector)
/// Shuttle spawns 90 km out along the +Z axis facing Sol at (0, 0, 0).
pub const DEMO_SPAWN_POSITION: DVec3 = DVec3::new(0.0, 0.0, 90_000.0);

/// Marker component for the physical 3D shuttle cockpit canopy glass
#[derive(Component)]
pub struct CockpitCanopyGlass;

#[derive(Component)]
pub struct RunwayLight {
    pub base_emissive: LinearRgba,
    pub phase: f32,
    pub frequency: f32,
}

/// Marker on the glTF root scene entity spawned under the Shuttle
#[derive(Component)]
pub struct ShipSceneRoot;

pub fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut star_materials: ResMut<Assets<crate::stars::RelativisticStarMaterial>>,
    asset_server: Res<AssetServer>,
    mut loading_tracker: Option<ResMut<crate::ui::StartupLoadingTracker>>,
) {
    // 1. Dynamic Sun Directional Light (aligned directly to Sol)
    commands.spawn((
        SunDirectionalLight,
        DirectionalLight {
            illuminance: 4_500.0,
            color: Color::srgb(1.0, 1.0, 1.0),
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 50.0, 200.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // 1b. Dynamic Cosmic Searchlight Directional Light (Relativistic Starfield Beaming)
    commands.spawn((
        CosmicSearchlightDirectionalLight,
        DirectionalLight {
            illuminance: 0.0,
            color: Color::srgb(0.35, 0.65, 1.0),
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, -150.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // 2. Spawn the Player's Space Shuttle with Universal Coordinates
    spawn_shuttle(&mut commands, &mut meshes, &mut materials, &asset_server, loading_tracker.as_deref_mut());

    // 3. Spawn Procedural 12,000-star Cosmos on GPU (Morgan-Keenan Blackbody Spectra)
    crate::stars::spawn_procedural_starfield(&mut commands, &mut meshes, &mut materials, &mut star_materials);

    // 4. Spawn 1:1 Scale Authentic Solar System (Default Universe Mode)
    crate::celestial::spawn_realistic_solar_system(&mut commands, &mut meshes, &mut materials, &asset_server, loading_tracker.as_deref_mut());
}

/// Spawns flight obstacle course and local landmarks for the Demo Sector
pub fn spawn_demo_props(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    spawn_slalom_course(commands, meshes, materials);
    spawn_spatial_beacons(commands, meshes, materials);
    spawn_space_station(commands, meshes, materials);
    spawn_asteroids(commands, meshes, materials);
}

// ==============================================================================
// SHIP ENGINE RECTANGLES CONFIGURATION (Federal Corvette - 8 Thruster Slats)
// ==============================================================================
// 1. Dimensions of the rectangular thrusters [Width (X), Height (Y), Depth (Z)]:
pub const ENGINE_SLAT_SIZE: Vec3 = Vec3::new(10.25, 1.05, 0.40);

// 2. Left Engine Thruster Slats: [X, Y, Z] coordinates for each of the 4 rectangles:
pub const LEFT_ENGINE_SLAT_POSITIONS: [[f32; 3]; 4] = [
    [-18.45, -1.05, 79.1],  // Left Slat 1 (Top)
    [-18.45, -2.45, 79.1],  // Left Slat 2 (Upper-mid)
    [-18.45, -3.82, 79.1],  // Left Slat 3 (Lower-mid)
    [-18.45, -5.20, 79.1],  // Left Slat 4 (Bottom)
];

// 3. Right Engine Thruster Slats: [X, Y, Z] coordinates for each of the 4 rectangles:
pub const RIGHT_ENGINE_SLAT_POSITIONS: [[f32; 3]; 4] = [
    [ 18.45, -1.05, 79.1],  // Right Slat 1 (Top)
    [ 18.45, -2.45, 79.1],  // Right Slat 2 (Upper-mid)
    [ 18.45, -3.82, 79.1],  // Right Slat 3 (Lower-mid)
    [ 18.45, -5.20, 79.1],  // Right Slat 4 (Bottom)
];

fn spawn_shuttle(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    mut loading_tracker: Option<&mut crate::ui::StartupLoadingTracker>,
) {
    // Engine Slat Material (4 glowing horizontal rectangular emitter bars per engine)
    let engine_slat_material = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::new(
            1.0 * crate::flight::ENGINE_SLAT_LUM_IDLE,
            0.55 * crate::flight::ENGINE_SLAT_LUM_IDLE,
            0.15 * crate::flight::ENGINE_SLAT_LUM_IDLE,
            1.0,
        )),
        emissive: LinearRgba::rgb(
            1.0 * crate::flight::ENGINE_SLAT_LUM_IDLE,
            0.55 * crate::flight::ENGINE_SLAT_LUM_IDLE,
            0.15 * crate::flight::ENGINE_SLAT_LUM_IDLE,
        ),
        unlit: true,
        ..default()
    });

    // Root Shuttle Entity at Universal Coordinates:
    // Spawns in orbit around Earth with orbital velocity, overlooking Earth on the right and Sol on the left
    let (spawn_pos, spawn_rot, spawn_vel) = crate::celestial::realistic_earth_orbital_spawn();
    let mut initial_rel_state = RelativisticState::default();
    initial_rel_state.rapidity = crate::relativity::rapidity_from_velocity(spawn_vel, crate::relativity::REAL_C);
    initial_rel_state.update_from_rapidity(crate::relativity::REAL_C);

    let shuttle_entity = commands
        .spawn((
            Shuttle::default(),
            initial_rel_state,
            UniversalPosition(spawn_pos),
            Transform::from_rotation(spawn_rot),
            Visibility::default(),
        ))
        .id();

    // Shuttle Meshes & Model Hierarchy
    commands.entity(shuttle_entity).with_children(|parent| {
        // 1. 3D Ship glTF Model (Federal Corvette: 168m x 87m x 28m)
        // Scaled to 1:1 true scale (0.01 scale for cm-modeled glTF)
        // The glTF file already contains Sketchfab's root transform orienting -Z forward and +Y up
        let ship_scene_handle = asset_server.load("models/ship.glb#Scene0");
        if let Some(ref mut tracker) = loading_tracker {
            tracker.tracked_assets.push(crate::ui::TrackedLoadingAsset {
                label: "Federal Corvette Airframe (models/ship.glb)".to_string(),
                id: ship_scene_handle.id().untyped(),
            });
        }

        parent.spawn((
            ShipSceneRoot,
            SceneRoot(ship_scene_handle),
            Transform::from_scale(Vec3::splat(0.01)),
        ));

        // Shared rectangular mesh geometry for all 8 thruster slats
        let slat_mesh = meshes.add(Cuboid::new(
            ENGINE_SLAT_SIZE.x,
            ENGINE_SLAT_SIZE.y,
            ENGINE_SLAT_SIZE.z,
        ));

        // 2. LEFT ENGINE (4 Glowing Rectangular Slats)
        for &[x, y, z] in &LEFT_ENGINE_SLAT_POSITIONS {
            parent.spawn((
                EngineSlat,
                Mesh3d(slat_mesh.clone()),
                MeshMaterial3d(engine_slat_material.clone()),
                Transform::from_xyz(x, y, z),
            ));
        }

        // Left Engine Bay Lighting (Ambient plasma illumination on hull)
        parent.spawn((
            EngineBayLight { base_intensity: 28_000.0 },
            PointLight {
                color: Color::srgb(1.0, 0.55, 0.15),
                intensity: 4_200.0,
                range: 40.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(-18.45, -3.1, 79.2),
        ));

        // 3. RIGHT ENGINE (4 Glowing Rectangular Slats)
        for &[x, y, z] in &RIGHT_ENGINE_SLAT_POSITIONS {
            parent.spawn((
                EngineSlat,
                Mesh3d(slat_mesh.clone()),
                MeshMaterial3d(engine_slat_material.clone()),
                Transform::from_xyz(x, y, z),
            ));
        }

        // Right Engine Bay Lighting (Ambient plasma illumination on hull)
        parent.spawn((
            EngineBayLight { base_intensity: 28_000.0 },
            PointLight {
                color: Color::srgb(1.0, 0.55, 0.15),
                intensity: 4_200.0,
                range: 40.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(18.45, -3.1, 79.2),
        ));
    });
}

fn spawn_slalom_course(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let neon_colors = [
        Color::srgb(0.1, 0.85, 1.0), // High-vis Neon Cyan
        Color::srgb(0.3, 0.65, 1.0), // Electric Azure
        Color::srgb(0.95, 0.35, 1.0), // Fluorescent Purple
        Color::srgb(0.2, 1.0, 0.65), // Mint Green
        Color::srgb(1.0, 0.75, 0.2), // Runway Amber
    ];

    let neon_emissive = [
        LinearRgba::rgb(0.2, 3.2, 4.5),
        LinearRgba::rgb(0.4, 2.2, 4.8),
        LinearRgba::rgb(3.5, 0.5, 4.2),
        LinearRgba::rgb(0.3, 4.5, 1.5),
        LinearRgba::rgb(4.2, 2.5, 0.4),
    ];

    let ring_mesh = meshes.add(Torus {
        minor_radius: 0.85,
        major_radius: 12.0,
    });

    let runway_beacon_mesh = meshes.add(Sphere::new(0.45));

    // 15 glowing navigational gates forming an arc course in universal coordinates
    for i in 0..15 {
        let t = i as f64;
        let z = DEMO_SPAWN_POSITION.z - 60.0 - t * 90.0;
        let x = (t * 0.4).sin() * 50.0;
        let y = (t * 0.3).cos() * 25.0;

        let (gate_color, gate_emissive) = if i == 0 {
            // ALPHA GATE: High-visibility Runway Threshold Gateway (Emerald & Gold)
            (
                Color::srgb(0.2, 0.95, 0.45),
                LinearRgba::rgb(0.6, 6.0, 2.2), // Bright green runway threshold bloom
            )
        } else {
            (
                neon_colors[i % neon_colors.len()],
                neon_emissive[i % neon_emissive.len()],
            )
        };

        let mat = materials.add(StandardMaterial {
            base_color: gate_color,
            emissive: gate_emissive,
            metallic: 0.85,
            perceptual_roughness: 0.15,
            ..default()
        });

        let mut gate = commands.spawn((
            crate::celestial::UniverseEntity,
            UniversalPosition(DVec3::new(x, y, z)),
            Mesh3d(ring_mesh.clone()),
            MeshMaterial3d(mat),
            crate::celestial::RelativisticOpticsBody {
                base_color: LinearRgba::from(gate_color),
                base_emissive: gate_emissive,
                is_emissive_star: false,
                rest_temperature_k: 290.0,
            },
            Transform::from_xyz(x as f32, y as f32, z as f32)
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        ));

        // Add airport-like runway approach and edge lights studded around the perimeter
        gate.with_children(|parent| {
            let light_count = if i == 0 { 12 } else { 8 };
            for j in 0..light_count {
                let frac = j as f32 / light_count as f32;
                let angle = frac * std::f32::consts::TAU;
                let lx = angle.cos() * 12.0;
                let ly = angle.sin() * 12.0;

                let (base_emissive, freq) = if i == 0 {
                    // Airport runway threshold: alternating bright green beacons and white strobe flashers
                    if j % 2 == 0 {
                        (LinearRgba::rgb(0.4, 8.0, 2.0), 3.5)
                    } else {
                        (LinearRgba::rgb(6.0, 6.0, 7.5), 7.0) // Chasing white runway strobe
                    }
                } else {
                    (gate_emissive * 1.5, 3.0)
                };

                let beacon_mat = materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    emissive: base_emissive,
                    unlit: true,
                    ..default()
                });

                parent.spawn((
                    RunwayLight {
                        base_emissive,
                        phase: frac * std::f32::consts::TAU,
                        frequency: freq,
                    },
                    Mesh3d(runway_beacon_mesh.clone()),
                    MeshMaterial3d(beacon_mat),
                    Transform::from_xyz(lx, ly, 0.0),
                ));
            }
        });

        if i == 0 {
            gate.insert(crate::celestial::NavTarget {
                name: "Alpha Gate (Slalom)",
                classification: "Course Waypoint",
                radius_m: 25.0,
            });
        }
    }
}

fn spawn_spatial_beacons(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let beacon_mesh = meshes.add(Sphere::new(1.2));

    let coords_x: [f64; 5] = [-300.0, -150.0, 0.0, 150.0, 300.0];
    let coords_z: [f64; 5] = [-400.0, -200.0, 0.0, 200.0, 400.0];

    // Spawn 3D grid reference markers with universal positions
    for &x in &coords_x {
        for &z in &coords_z {
            if (x == 0.0 && z == 0.0) || (x.abs() < 50.0 && z < -50.0 && z > -1400.0) {
                continue;
            }

            let beacon_mat = materials.add(StandardMaterial {
                base_color: Color::BLACK,
                emissive: LinearRgba::rgb(2.5, 0.4, 0.1),
                unlit: true,
                ..default()
            });

            let upos_z = DEMO_SPAWN_POSITION.z + z;
            commands.spawn((
                crate::celestial::UniverseEntity,
                UniversalPosition(DVec3::new(x, -50.0, upos_z)),
                PulsingBeacon {
                    base_emissive: LinearRgba::rgb(2.5, 0.4, 0.1),
                    frequency: 2.5,
                },
                Mesh3d(beacon_mesh.clone()),
                MeshMaterial3d(beacon_mat),
                Transform::from_xyz(x as f32, -50.0, upos_z as f32),
            ));
        }
    }
}

fn spawn_space_station(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let station_metal = materials.add(StandardMaterial {
        base_color: Color::srgb(0.7, 0.72, 0.78),
        metallic: 0.9,
        perceptual_roughness: 0.2,
        ..default()
    });

    let solar_panel_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.05, 0.1, 0.3),
        metallic: 0.95,
        perceptual_roughness: 0.1,
        emissive: LinearRgba::rgb(0.02, 0.05, 0.15),
        ..default()
    });

    let station_upos = DEMO_SPAWN_POSITION + DVec3::new(160.0, 40.0, -220.0);

    let hub_entity = commands
        .spawn((
            crate::celestial::UniverseEntity,
            UniversalPosition(station_upos),
            crate::celestial::NavTarget {
                name: "Station Alpha",
                classification: "Orbital Habitat",
                radius_m: 80.0,
            },
            RotatingLandmark {
                rotation_axis: Vec3::Y,
                speed: 0.12,
            },
            crate::celestial::RelativisticOpticsBody {
                base_color: LinearRgba::new(0.7, 0.72, 0.78, 1.0),
                base_emissive: LinearRgba::rgb(0.0, 0.0, 0.0),
                is_emissive_star: false,
                rest_temperature_k: 290.0,
            },
            Transform::from_xyz(station_upos.x as f32, station_upos.y as f32, station_upos.z as f32),
            Visibility::default(),
        ))
        .id();

    commands.entity(hub_entity).with_children(|parent| {
        // Central Station Hub
        parent.spawn((
            Mesh3d(meshes.add(Cylinder::new(12.0, 35.0))),
            MeshMaterial3d(station_metal.clone()),
            Transform::IDENTITY,
        ));

        // Rotating Ring Habitat
        parent.spawn((
            Mesh3d(meshes.add(Torus {
                minor_radius: 2.8,
                major_radius: 40.0,
            })),
            MeshMaterial3d(station_metal.clone()),
            Transform::from_xyz(0.0, 0.0, 0.0).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        ));

        // Large Solar Array Left
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(50.0, 0.3, 12.0))),
            MeshMaterial3d(solar_panel_mat.clone()),
            Transform::from_xyz(-45.0, 0.0, 0.0),
        ));

        // Large Solar Array Right
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(50.0, 0.3, 12.0))),
            MeshMaterial3d(solar_panel_mat),
            Transform::from_xyz(45.0, 0.0, 0.0),
        ));
    });
}

fn spawn_asteroids(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let asteroid_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.32, 0.3),
        perceptual_roughness: 0.9,
        metallic: 0.15,
        ..default()
    });

    let asteroid_mesh = meshes.add(Sphere::new(1.0));

    // Scatter 20 asteroid clusters with universal coordinates
    for i in 0..20 {
        let angle = (i as f64) * 0.35;
        let radius = 120.0 + (i as f64 * 17.0) % 180.0;
        let x = angle.cos() * radius - 80.0;
        let z = angle.sin() * radius - 300.0 + DEMO_SPAWN_POSITION.z;
        let y = ((i as f64 * 31.0) % 90.0) - 45.0;

        let scale = 3.0 + ((i as f32 * 7.0) % 12.0);

        commands.spawn((
            crate::celestial::UniverseEntity,
            UniversalPosition(DVec3::new(x, y, z)),
            RotatingLandmark {
                rotation_axis: Vec3::new(
                    (i as f32 * 0.3).sin(),
                    (i as f32 * 0.5).cos(),
                    (i as f32 * 0.7).sin(),
                ).normalize(),
                speed: 0.05 + (i as f32 % 5.0) * 0.04,
            },
            crate::celestial::RelativisticOpticsBody {
                base_color: LinearRgba::new(0.35, 0.32, 0.3, 1.0),
                base_emissive: LinearRgba::rgb(0.0, 0.0, 0.0),
                is_emissive_star: false,
                rest_temperature_k: 180.0,
            },
            Mesh3d(asteroid_mesh.clone()),
            MeshMaterial3d(asteroid_mat.clone()),
            Transform::from_xyz(x as f32, y as f32, z as f32)
                .with_scale(Vec3::new(scale * 1.1, scale * 0.8, scale * 1.3)),
        ));
    }
}

pub fn rotate_landmarks_system(
    time: Res<Time>,
    pause: Res<SimulationPause>,
    config: Res<crate::relativity::RelativisticConfig>,
    mut query: Query<(&RotatingLandmark, &mut Transform)>,
) {
    if pause.0 {
        return;
    }
    let dt = time.delta_secs() * config.time_scale as f32;
    for (landmark, mut transform) in query.iter_mut() {
        let rot = Quat::from_axis_angle(landmark.rotation_axis, landmark.speed * dt);
        transform.rotation = (transform.rotation * rot).normalize();
    }
}

pub fn pulse_beacons_system(
    time: Res<Time>,
    pause: Res<SimulationPause>,
    beacon_query: Query<(&PulsingBeacon, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if pause.0 {
        return;
    }
    let t = time.elapsed_secs();
    for (beacon, mat_handle) in beacon_query.iter() {
        if let Some(mat) = materials.get_mut(mat_handle) {
            let pulse = (t * beacon.frequency).sin() * 0.5 + 0.5;
            let mult = 0.4 + pulse * 1.6;
            mat.emissive = LinearRgba::rgb(
                beacon.base_emissive.red * mult,
                beacon.base_emissive.green * mult,
                beacon.base_emissive.blue * mult,
            );
        }
    }
}

/// Chases and pulses the airport runway perimeter lights on the flight slalom gates
pub fn pulse_runway_lights_system(
    time: Res<Time>,
    pause: Res<SimulationPause>,
    query: Query<(&RunwayLight, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if pause.0 {
        return;
    }
    let t = time.elapsed_secs();
    for (light, mat_handle) in query.iter() {
        if let Some(mat) = materials.get_mut(mat_handle) {
            let wave = (t * light.frequency + light.phase).sin() * 0.5 + 0.5;
            let strobe = if wave > 0.85 { 4.5 } else { 0.35 + wave * 0.85 };
            mat.emissive = LinearRgba::rgb(
                light.base_emissive.red * strobe,
                light.base_emissive.green * strobe,
                light.base_emissive.blue * strobe,
            );
        }
    }
}

/// Dynamically aligns the primary DirectionalLight to shine from Sol towards the player's shuttle,
/// taking relativistic aberration, Doppler shift, and canopy visor transmittance into account.
pub fn sun_light_tracking_system(
    sol_query: Query<&UniversalPosition, (With<crate::celestial::CelestialBody>, Without<crate::celestial::KeplerOrbit>)>,
    shuttle_query: Query<(&UniversalPosition, Option<&RelativisticState>), With<Shuttle>>,
    occluders_query: Query<(&UniversalPosition, &crate::celestial::CelestialBody), (With<crate::celestial::KeplerOrbit>, Without<Shuttle>)>,
    visor: Res<crate::celestial::CanopyVisorState>,
    optics: Option<Res<crate::relativity::RelativisticOpticsConfig>>,
    mut light_query: Query<(&mut Transform, &mut DirectionalLight), (With<SunDirectionalLight>, Without<CosmicSearchlightDirectionalLight>)>,
    mut searchlight_query: Query<(&mut Transform, &mut DirectionalLight), (With<CosmicSearchlightDirectionalLight>, Without<SunDirectionalLight>)>,
    ambient_light: Option<ResMut<AmbientLight>>,
) {
    let Ok(sol_upos) = sol_query.get_single() else {
        return;
    };
    let Ok((shuttle_upos, rel_state_opt)) = shuttle_query.get_single() else {
        return;
    };
    let Ok((mut light_tf, mut light)) = light_query.get_single_mut() else {
        return;
    };

    let shuttle_to_sol = sol_upos.0 - shuttle_upos.0;
    if shuttle_to_sol.length_squared() > 1.0 {
        let sun_dir_rest = shuttle_to_sol.normalize();

        let aberration_on = optics.as_ref().map(|o| o.aberration_enabled).unwrap_or(false);
        let doppler_on = optics.as_ref().map(|o| o.doppler_enabled).unwrap_or(false);

        // 1. Relativistic Aberration:
        // Sol's apparent incoming direction transforms under relativistic velocity.
        // Sunlight and specular highlights on the shuttle match where Sol appears visually in the sky.
        let apparent_to_sol = if aberration_on {
            if let Some(rel) = rel_state_opt {
                if rel.rapidity.length_squared() > 1e-10 {
                    crate::relativity::aberrate_direction_from_rapidity(shuttle_to_sol, rel.rapidity)
                } else {
                    sun_dir_rest
                }
            } else {
                sun_dir_rest
            }
        } else {
            sun_dir_rest
        };

        // Solar rays arrive from Sol's apparent direction towards the shuttle (-apparent_to_sol)
        let sun_ray_dir = -apparent_to_sol.normalize().as_vec3();
        light_tf.translation = -sun_ray_dir * 150.0;
        *light_tf = light_tf.looking_to(sun_ray_dir, Vec3::Y);

        let visor_dim = visor.mode.transmittance().clamp(0.15, 1.0);

        // 2. Relativistic Doppler & Beaming for Sunlight:
        let (light_col, beaming) = if doppler_on {
            if let Some(rel) = rel_state_opt {
                if rel.beta > 0.001 {
                    let d = crate::relativity::relativistic_doppler_factor(
                        sun_dir_rest.as_vec3(),
                        rel.beta_vec.as_vec3(),
                        rel.gamma as f32,
                    );
                    let (c, b) = crate::relativity::blackbody_star_doppler(5778.0, d);
                    (c, b.clamp(0.01, 8.0))
                } else {
                    (LinearRgba::WHITE, 1.0)
                }
            } else {
                (LinearRgba::WHITE, 1.0)
            }
        } else {
            (LinearRgba::WHITE, 1.0)
        };

        light.color = Color::LinearRgba(light_col);
        let dist_to_sol = shuttle_to_sol.length();
        let au = 149_597_870_700.0f64;
        let proximity_factor = if dist_to_sol > 1.0 {
            let d = (dist_to_sol / au).max(0.005);
            // Perceptual human/camera adaptation curve: Stevens' power law / Weber-Fechner response (1 / d^0.35)
            // Models eye pupil dilation and sensor exposure adaptation across interplanetary distances.
            // At 0.005 AU (corona): 2.5x (11,250 lux)
            // At 1.0 AU (Earth): 1.0x (4,500 lux)
            // At 5.2 AU (Jupiter): 0.56x (2,520 lux) - vividly illuminated cloud belts
            // At 9.5 AU (Saturn): 0.45x (2,025 lux) - crisp rings and atmosphere
            // At 39.5 AU (Pluto): 0.28x (1,260 lux) - clearly visible icy nitrogen plains
            (1.0 / d.powf(0.35) as f32).clamp(0.25, 2.5)
        } else {
            1.0
        };

        // 3. Planetary Solar Eclipse / Shadow Occlusion:
        // When the shuttle is on the night side of a planet or moon, that body eclipses Sol.
        // Direct sunlight and specular hull reflections fade smoothly to zero in the planetary umbra.
        let mut occlusion_factor = 1.0f32;
        let ray_origin = shuttle_upos.0;
        let ray_dir = sun_dir_rest; // Unit vector from shuttle towards Sol

        for (body_upos, body) in occluders_query.iter() {
            let to_body = body_upos.0 - ray_origin;
            let proj = to_body.dot(ray_dir);

            // Only bodies between the shuttle and Sol can cast an eclipse shadow on the shuttle
            if proj > 0.0 && proj < dist_to_sol {
                let perp_dist_sq = to_body.length_squared() - proj * proj;
                let r = body.physical_radius;
                let penumbra = (r * 0.05).max(10_000.0);
                let outer_r = r + penumbra;

                if perp_dist_sq < outer_r * outer_r {
                    let perp_dist = perp_dist_sq.max(0.0).sqrt();
                    let shadow = if perp_dist <= r {
                        0.0f32
                    } else {
                        let t = ((perp_dist - r) / penumbra).clamp(0.0, 1.0) as f32;
                        t * t * (3.0 - 2.0 * t)
                    };
                    occlusion_factor = occlusion_factor.min(shadow);
                    if occlusion_factor <= 0.0 {
                        break;
                    }
                }
            }
        }

        light.illuminance = 4_500.0 * visor_dim * beaming * proximity_factor * occlusion_factor;

        // 4. Relativistic Cosmic Searchlight Directional Light & Ambient Illumination:
        // In the resting celestial frame, background starlight is subtle, isotropic, and neutral-white.
        // As beta exceeds ~0.40 and approaches 1.0, relativistic aberration compresses stars into a forward cone
        // and Doppler effect blueshifts their radiation.
        // Below beta = 0.50 (cone > 105°), starlight remains subtle and diffuse.
        // At beta = 0.80 (cone ~ 73°), starlight provides gentle, moody contouring (~200 lux) without specular blowout.
        // Only as beta approaches 0.96+ (cone < 30°) and 0.99+ (cone < 15°) does it intensify into a powerful beam.
        let searchlight_lux = if doppler_on && rel_state_opt.map_or(false, |r| r.beta > 0.40) {
            let rel = rel_state_opt.unwrap();
            let beta = rel.beta as f32;
            let flight_dir = if rel.velocity.length_squared() > 1e-6 {
                rel.velocity.normalize().as_vec3()
            } else {
                -Vec3::Z
            };

            // Incoming starlight arrives from the forward flight direction towards the craft
            let searchlight_ray_dir = -flight_dir;

            // Normalized relativistic compression factor u in [0.0, 1.0] from beta = 0.40 to 1.0
            let u = ((beta - 0.40) / 0.60).clamp(0.0, 1.0);
            // Power curve (u^8.0) ensures soft starlight (~230 lux) at beta = 0.80 and exponential surge toward c
            let beam_factor = u.powf(8.0);
            let lux = (beam_factor * 6_000.0 * visor_dim).clamp(0.0, 6_000.0);

            if let Ok((mut s_tf, mut s_light)) = searchlight_query.get_single_mut() {
                s_tf.translation = -searchlight_ray_dir * 150.0;
                *s_tf = s_tf.looking_to(searchlight_ray_dir, Vec3::Y);
                s_light.illuminance = lux;

                // Color transition matching the Doppler shift of the Milky Way star catalog:
                // At low beta: Average resting starlight (creamy diamond white, ~6,000 K)
                // At beta ~ 0.80: Pale diamond with subtle cyan tinge
                // At beta ~ 0.95: Electric cyan-blue
                // At beta ~ 1.00: Deep electric sapphire blue
                let r = (1.00 * (1.0 - u) + 0.18 * u).max(0.05);
                let g = (0.94 * (1.0 - u) + 0.48 * u).max(0.10);
                let b = 0.88 * (1.0 - u) + 1.85 * u;
                s_light.color = Color::LinearRgba(LinearRgba::new(r, g, b, 1.0));
            }

            lux
        } else {
            if let Ok((_, mut s_light)) = searchlight_query.get_single_mut() {
                s_light.illuminance = 0.0;
            }
            0.0
        };

        if let Some(mut ambient) = ambient_light {
            let base_ambient = LinearRgba::new(0.35, 0.45, 0.65, 1.0);
            let rest_ambient = 4.0;
            if doppler_on && rel_state_opt.map_or(false, |r| r.beta > 0.40) {
                let beta = rel_state_opt.unwrap().beta as f32;
                let u = ((beta - 0.40) / 0.60).clamp(0.0, 1.0);
                let color_t = u.powf(2.0);
                let ambient_blue = LinearRgba::new(
                    base_ambient.red * (1.0 - 0.45 * color_t),
                    base_ambient.green * (1.0 - 0.15 * color_t) + 0.10 * color_t,
                    base_ambient.blue * (1.0 + 0.6 * color_t),
                    1.0,
                );
                ambient.color = Color::LinearRgba(ambient_blue);
                // Ambient brightness scales gently with forward searchlight
                ambient.brightness = ((rest_ambient + searchlight_lux * 0.015) * visor_dim).clamp(1.0, 120.0);
            } else {
                ambient.color = Color::LinearRgba(base_ambient);
                ambient.brightness = rest_ambient * visor_dim;
            }
        }
    }
}

/// Dynamically updates the physical 3D canopy glass material of the shuttle
/// based on the active visor mode (Clear / Polarized / Solar Filter).
pub fn canopy_visor_glass_system(
    visor: Res<crate::celestial::CanopyVisorState>,
    glass_query: Query<&MeshMaterial3d<StandardMaterial>, With<CockpitCanopyGlass>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !visor.is_changed() {
        return;
    }
    for mat_handle in glass_query.iter() {
        let Some(mat) = materials.get_mut(&mat_handle.0) else {
            continue;
        };
        match visor.mode {
            crate::celestial::CanopyVisorMode::Clear => {
                mat.base_color = Color::srgba(0.04, 0.10, 0.22, 0.40);
                mat.emissive = LinearRgba::rgb(0.01, 0.03, 0.06);
            }
            crate::celestial::CanopyVisorMode::Polarized => {
                mat.base_color = Color::srgba(0.02, 0.04, 0.08, 0.85);
                mat.emissive = LinearRgba::rgb(0.005, 0.01, 0.02);
            }
            crate::celestial::CanopyVisorMode::SolarFilter => {
                mat.base_color = Color::srgba(0.20, 0.09, 0.02, 0.94);
                mat.emissive = LinearRgba::rgb(0.08, 0.03, 0.01);
            }
        }
    }
}

/// Marker component added to ship mesh entities whose PBR materials have been configured.
#[derive(Component)]
pub struct ShipMaterialConfigured;

/// Configures materials on the loaded Federal Corvette airframe:
/// - Maintains double-sided rendering (`double_sided = true`, `cull_mode = None`) so lateral side panels
///   and single-layer geometry remain fully opaque and don't punch transparent holes through the hull.
/// - Unbinds the metallic/roughness texture which had extreme 0.0/0.18 roughness values causing infinite mirror spikes.
/// - Sets a balanced satin-metallic spacecraft hull finish (perceptual_roughness = 0.45, metallic = 0.35, reflectance = 0.45)
///   providing smooth, visible solar specular highlights without blinding mirror glare.
/// - Strictly ensures only materials with active emissive textures glow, while setting materials without emissive
///   textures (such as Material 4: Sides) to LinearRgba::BLACK so they never render as glowing flat white panels.
pub fn configure_ship_materials_system(
    mut commands: Commands,
    ship_root_query: Query<Entity, With<ShipSceneRoot>>,
    children_query: Query<&Children>,
    unconfigured_meshes: Query<(Entity, &MeshMaterial3d<StandardMaterial>), Without<ShipMaterialConfigured>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for root in ship_root_query.iter() {
        let mut stack = vec![root];
        while let Some(entity) = stack.pop() {
            if let Ok(children) = children_query.get(entity) {
                for &child in children.iter() {
                    stack.push(child);
                }
            }

            if let Ok((mesh_entity, mat_handle)) = unconfigured_meshes.get(entity) {
                if let Some(mat) = materials.get_mut(&mat_handle.0) {
                    mat.double_sided = true;
                    mat.cull_mode = None;
                    mat.metallic_roughness_texture = None;
                    mat.perceptual_roughness = 0.45;
                    mat.metallic = 0.35;
                    mat.reflectance = 0.45;

                    // Only materials that actually have an emissive texture (running lights, cockpit HUDs)
                    // should have emissive illumination. Materials without an emissive texture (e.g. Sides)
                    // MUST be set to BLACK to prevent flat white emission.
                    if mat.emissive_texture.is_some() {
                        mat.emissive = LinearRgba::new(0.5, 0.5, 0.5, 1.0);
                    } else {
                        mat.emissive = LinearRgba::BLACK;
                    }
                }
                commands.entity(mesh_entity).insert(ShipMaterialConfigured);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sun_light_aberration_and_doppler_tracking() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(crate::relativity::RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig {
            aberration_enabled: true,
            doppler_enabled: true,
            beaming_enabled: true,
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(AmbientLight::default());

        // Sol at universal origin
        app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody { physical_radius: 696_340_000.0, mass_kg: 1.989e30 },
        ));

        // Shuttle at (100,000,000, 0, 0) moving fast along -Z at beta = 0.8
        let beta = 0.8f64;
        let zeta = beta.atanh();
        let rap = DVec3::new(0.0, 0.0, -zeta);
        let v = DVec3::new(0.0, 0.0, -beta * c);
        let gamma = 1.0 / (1.0 - beta * beta).sqrt();

        app.world_mut().spawn((
            Shuttle::default(),
            UniversalPosition(DVec3::new(100_000_000.0, 0.0, 0.0)),
            RelativisticState {
                velocity: v,
                rapidity: rap,
                beta,
                beta_vec: DVec3::new(0.0, 0.0, -beta),
                gamma,
                ..default()
            },
        ));

        let light_ent = app.world_mut().spawn((
            SunDirectionalLight,
            DirectionalLight::default(),
            Transform::default(),
        )).id();

        app.add_systems(Update, sun_light_tracking_system);
        app.update();

        let light_tf = app.world().get::<Transform>(light_ent).unwrap();
        let light = app.world().get::<DirectionalLight>(light_ent).unwrap();

        // Without aberration, Sol is at 90 deg (-X relative to shuttle).
        // With aberration at beta = 0.8 along -Z, Sol is aberrated forward towards -Z!
        let light_forward = light_tf.forward();
        // Since sunlight shines from Sol towards shuttle, its forward vector has a +Z component from aberration
        assert!(light_forward.z > 0.5, "Sunlight ray must shine towards +Z (illuminating front of craft) due to aberration, got {}", light_forward.z);
        assert!(light.illuminance > 0.0, "Light illuminance must be positive");
    }

    #[test]
    fn test_configure_ship_materials() {
        let mut app = App::new();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<Mesh>>();

        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let shiny_mat = materials.add(StandardMaterial {
            perceptual_roughness: 0.1,
            metallic: 0.9,
            reflectance: 0.8,
            double_sided: true,
            cull_mode: None,
            emissive: LinearRgba::WHITE,
            ..default()
        });

        // Spawn ship scene root with child mesh
        let child_mesh = app.world_mut().spawn(MeshMaterial3d(shiny_mat.clone())).id();

        app.world_mut().spawn(ShipSceneRoot).add_child(child_mesh);

        app.add_systems(Update, configure_ship_materials_system);
        app.update();

        // Check child now has ShipMaterialConfigured
        assert!(app.world().get::<ShipMaterialConfigured>(child_mesh).is_some());

        // Check material properties were corrected
        let mats = app.world().resource::<Assets<StandardMaterial>>();
        let mat = mats.get(&shiny_mat).unwrap();
        assert_eq!(mat.perceptual_roughness, 0.45);
        assert_eq!(mat.metallic, 0.35);
        assert_eq!(mat.reflectance, 0.45);
        assert!(mat.double_sided);
        assert_eq!(mat.cull_mode, None);
        assert!(mat.metallic_roughness_texture.is_none());
        assert_eq!(mat.emissive, LinearRgba::BLACK);
    }

    #[test]
    fn test_sun_light_planetary_eclipse_occlusion() {
        let mut app = App::new();
        app.insert_resource(crate::celestial::CanopyVisorState::default());

        // Sol at origin (0, 0, 0)
        app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody { physical_radius: 696_340_000.0, mass_kg: 1.989e30 },
        ));

        // Earth between Sol and Shuttle at X = 10_000_000.0 with radius 6_371_000.0
        app.world_mut().spawn((
            UniversalPosition(DVec3::new(10_000_000.0, 0.0, 0.0)),
            crate::celestial::CelestialBody { physical_radius: 6_371_000.0, mass_kg: 5.972e24 },
            crate::celestial::KeplerOrbit::default(),
        ));

        // Shuttle at X = 20_000_000.0 (directly behind Earth in umbra)
        let shuttle_ent = app.world_mut().spawn((
            Shuttle::default(),
            UniversalPosition(DVec3::new(20_000_000.0, 0.0, 0.0)),
        )).id();

        let light_ent = app.world_mut().spawn((
            SunDirectionalLight,
            DirectionalLight::default(),
            Transform::default(),
        )).id();

        app.add_systems(Update, sun_light_tracking_system);
        app.update();

        // Shuttle is in full umbra: sunlight illuminance MUST be 0.0
        let light = app.world().get::<DirectionalLight>(light_ent).unwrap();
        assert_eq!(light.illuminance, 0.0, "Sunlight illuminance must be 0 in complete planetary umbra");

        // Move shuttle well out of Earth's shadow (Y = 50_000_000.0)
        app.world_mut().get_mut::<UniversalPosition>(shuttle_ent).unwrap().0 = DVec3::new(20_000_000.0, 50_000_000.0, 0.0);
        app.update();

        let light = app.world().get::<DirectionalLight>(light_ent).unwrap();
        assert!(light.illuminance > 0.0, "Sunlight illuminance must be positive when line of sight to Sol is clear");
    }

    #[test]
    fn test_cosmic_searchlight_illumination_at_relativistic_speed() {
        let mut app = App::new();
        let c = 299_792_458.0;
        app.insert_resource(crate::relativity::RelativisticConfig { c, time_scale: 1.0, is_demo_mode: false });
        app.insert_resource(crate::relativity::RelativisticOpticsConfig {
            aberration_enabled: true,
            doppler_enabled: true,
            beaming_enabled: true,
        });
        app.insert_resource(crate::celestial::CanopyVisorState::default());
        app.insert_resource(AmbientLight::default());

        // Sol at universal origin
        app.world_mut().spawn((
            UniversalPosition(DVec3::ZERO),
            crate::celestial::CelestialBody { physical_radius: 696_340_000.0, mass_kg: 1.989e30 },
        ));

        // Shuttle at (0, 0, 100_000_000) moving at beta = 0.9 along -Z (forward flight)
        let beta = 0.9f64;
        let zeta = beta.atanh();
        let rap = DVec3::new(0.0, 0.0, -zeta);
        let v = DVec3::new(0.0, 0.0, -beta * c);
        let gamma = 1.0 / (1.0 - beta * beta).sqrt();

        app.world_mut().spawn((
            Shuttle::default(),
            UniversalPosition(DVec3::new(0.0, 0.0, 100_000_000.0)),
            RelativisticState {
                velocity: v,
                rapidity: rap,
                beta,
                beta_vec: DVec3::new(0.0, 0.0, -beta),
                gamma,
                ..default()
            },
        ));

        app.world_mut().spawn((
            SunDirectionalLight,
            DirectionalLight::default(),
            Transform::default(),
        ));

        let searchlight_ent = app.world_mut().spawn((
            CosmicSearchlightDirectionalLight,
            DirectionalLight::default(),
            Transform::default(),
        )).id();

        app.add_systems(Update, sun_light_tracking_system);
        app.update();

        let s_light = app.world().get::<DirectionalLight>(searchlight_ent).unwrap();
        let s_tf = app.world().get::<Transform>(searchlight_ent).unwrap();

        // At beta = 0.9, cosmic searchlight MUST be on with strong forward illuminance (> 1,000 lux)
        assert!(s_light.illuminance > 1_000.0, "Searchlight illuminance must exceed 1,000 lux at beta = 0.9, got {}", s_light.illuminance);

        // Rays arrive from forward flight direction (-Z) shining backwards towards craft (+Z)
        let fwd = s_tf.forward();
        assert!(fwd.z > 0.9, "Searchlight directional light must shine along +Z towards the front of the ship, got z: {}", fwd.z);

        // Ambient light must be boosted with blue chromaticity
        let ambient = app.world().resource::<AmbientLight>();
        assert!(ambient.brightness > 20.0, "Ambient starlight brightness must be boosted by relativistic beaming, got {}", ambient.brightness);

        // At low speed (beta = 0.15), searchlight must be completely OFF (0.0 lux, no premature pop-in)
        let mut rel_state = app.world_mut().query::<&mut RelativisticState>().single_mut(app.world_mut());
        rel_state.beta = 0.15;
        rel_state.velocity = DVec3::new(0.0, 0.0, -0.15 * c);
        app.update();

        let s_light = app.world().get::<DirectionalLight>(searchlight_ent).unwrap();
        assert_eq!(s_light.illuminance, 0.0, "Searchlight illuminance must be 0.0 lux at beta = 0.15");
    }
}
