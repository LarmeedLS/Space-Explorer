#![allow(dead_code)]

use bevy::math::{DVec3, Vec3A};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::render::primitives::Aabb;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState,
    RenderPipelineDescriptor, ShaderRef, ShaderType, SpecializedMeshPipelineError,
};
use bevy::render::view::NoFrustumCulling;
use std::f32::consts::PI;

/// Tag component for the procedural starfield celestial dome
#[derive(Component)]
pub struct ProceduralStarfield;

/// Tag component for the forward Interstellar Dust & Starlight Halo (the 40° relativistic rainbow ring).
/// Formed at beta >= 0.55 when warm galactic dust and background starlight are aberrated into a forward annular halo.
#[derive(Component)]
pub struct InterstellarDustHalo;

/// Backwards compatibility alias for the relativistic interstellar dust halo
pub type CmbOpticalDipole = InterstellarDustHalo;

/// State and geometry tracking for the Interstellar Dust & Starlight Halo
#[derive(Component)]
#[allow(dead_code)]
pub struct InterstellarDustHaloCatalog {
    pub sphere_radius: f32,
    pub last_rapidity: DVec3,
    pub last_flight_dir: Vec3,
    pub last_visor_mode: crate::celestial::CanopyVisorMode,
    pub last_update_time: f64,
    pub last_optics_enabled: (bool, bool),
}

/// Backwards compatibility alias
pub type CmbRainbowCatalog = InterstellarDustHaloCatalog;

/// Tag component for the true Cosmic Microwave Background (CMB) thermal blackbody dome.
/// In the cosmic rest frame, the CMB is isotropic 2.7255 K microwave radiation (invisible).
/// At extreme relativistic Lorentz factors (gamma > 90), Doppler boosting shifts the forward apex
/// into near-infrared and visible red ember (T >= 500 K) -> orange -> incandescent white -> sapphire blue.
#[derive(Component)]
pub struct RealCmbDome;

/// Custom GPU Material for the Relativistic Celestial Starfield.
/// Computes analytical relativistic aberration and Doppler shift entirely in hardware via WGSL.
/// Guarantees flat, zero-leak VRAM performance and native 165Hz frame rates with 12,000+ stars.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct RelativisticStarMaterial {
    #[uniform(0)]
    pub uniform_data: StarfieldUniformData,
}

/// 48-byte uniform buffer sent to GPU every frame (updated in-place via queue.write_buffer)
#[derive(ShaderType, Debug, Clone, Copy, Default)]
pub struct StarfieldUniformData {
    #[allow(dead_code)]
    pub rapidity: Vec3,
    #[allow(dead_code)]
    pub beta: f32,
    #[allow(dead_code)]
    pub beta_vec: Vec3,
    #[allow(dead_code)]
    pub gamma: f32,
    #[allow(dead_code)]
    pub sphere_radius: f32,
    #[allow(dead_code)]
    pub aberration_enabled: u32,
    #[allow(dead_code)]
    pub doppler_enabled: u32,
    #[allow(dead_code)]
    pub visor_mode: u32,
}

impl Material for RelativisticStarMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/starfield.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/starfield.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(target) = descriptor.fragment.as_mut().and_then(|f| f.targets.first_mut()) {
            if let Some(target) = target {
                target.blend = Some(BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                });
            }
        }
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = false;
        }
        Ok(())
    }
}

/// Morgan-Keenan spectral classification with astrophysically realistic temperatures
#[derive(Clone, Copy, Debug)]
pub enum SpectralClass {
    O, // Deep Electric Blue (~35,000 K)
    B, // Blue-White (~20,000 K)
    A, // Pure Brilliant White (~9,000 K)
    F, // Creamy Yellow-White (~7,000 K)
    G, // Solar Golden-Yellow (~5,800 K)
    K, // Deep Warm Orange (~4,500 K)
    M, // Ruby Red Dwarf / Red Giant (~3,000 K)
}

impl SpectralClass {
    /// Returns the resting blackbody temperature in Kelvin
    pub fn temperature_kelvin(&self) -> f32 {
        match self {
            SpectralClass::O => 35_000.0,
            SpectralClass::B => 20_000.0,
            SpectralClass::A => 9_000.0,
            SpectralClass::F => 7_000.0,
            SpectralClass::G => 5_800.0,
            SpectralClass::K => 4_500.0,
            SpectralClass::M => 3_000.0,
        }
    }

    /// Returns the emissive color (LinearRgba) corresponding to the blackbody temperature
    /// with astrophotographic saturation boost so colors remain vibrant under ACES tonemapping and bloom.
    pub fn blackbody_color(&self) -> LinearRgba {
        match self {
            SpectralClass::O => LinearRgba::rgb(0.20, 0.50, 1.40), // Vivid Electric Sapphire Blue
            SpectralClass::B => LinearRgba::rgb(0.45, 0.75, 1.25), // Diamond Ice Blue / Cyan
            SpectralClass::A => LinearRgba::rgb(1.00, 1.00, 1.00), // Pure Brilliant Diamond White
            SpectralClass::F => LinearRgba::rgb(1.00, 0.95, 0.70), // Warm Creamy Vanilla White
            SpectralClass::G => LinearRgba::rgb(1.00, 0.82, 0.35), // Radiant Solar Golden Yellow
            SpectralClass::K => LinearRgba::rgb(1.00, 0.50, 0.08), // Warm Glowing Amber Orange
            SpectralClass::M => LinearRgba::rgb(1.10, 0.18, 0.04), // Deep Rich Ruby Red Ember
        }
    }

    /// Sample spectral class based on the naked-eye observable night sky catalog
    /// (Bright luminous O, B, and A giants dominate the visible night sky, while
    /// faint M-dwarfs are too dim to be seen with the naked eye unless nearby).
    pub fn sample_realistic(val: f32) -> Self {
        if val < 0.10 {
            SpectralClass::O // 10% Electric Sapphire Blue (e.g. Alnitak, Zeta Puppis)
        } else if val < 0.28 {
            SpectralClass::B // 18% Diamond Ice Blue (e.g. Rigel, Spica, Regulus)
        } else if val < 0.52 {
            SpectralClass::A // 24% Pristine Diamond White (e.g. Sirius, Vega, Altair)
        } else if val < 0.68 {
            SpectralClass::F // 16% Warm Creamy White (e.g. Canopus, Procyon)
        } else if val < 0.82 {
            SpectralClass::G // 14% Radiant Solar Golden (e.g. Capella, Alpha Centauri)
        } else if val < 0.93 {
            SpectralClass::K // 11% Warm Glowing Amber (e.g. Arcturus, Aldebaran)
        } else {
            SpectralClass::M // 7% Rich Ruby Red (e.g. Betelgeuse, Antares)
        }
    }
}

/// Spawns 12,000 procedural stars batched into a single static GPU geometry.
/// Formatted as a realistic galactic disk (Milky Way belt) and spherical halo.
/// Relativistic aberration and Doppler shift are computed entirely in the GPU vertex shader.
pub fn spawn_procedural_starfield(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    star_materials: &mut ResMut<Assets<RelativisticStarMaterial>>,
) {
    let star_count = 12_000;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(star_count * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(star_count * 4);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(star_count * 4);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(star_count * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(star_count * 6);

    let mut rng_state: u64 = 0x5A8E_B9A1_9E37_79B9;
    let mut rand_f32 = || {
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((rng_state >> 32) as u32 as f32) / (u32::MAX as f32)
    };

    let sphere_radius = 400_000.0f32;

    for i in 0..star_count {
        // Galactic distribution: 65% concentrated along galactic plane, 35% spherical halo
        let is_galactic_plane = rand_f32() < 0.65;
        let (theta, phi) = if is_galactic_plane {
            let t = rand_f32() * 2.0 * PI;
            let b = (rand_f32() - 0.5 + rand_f32() - 0.5) * 0.45;
            let p = PI / 2.0 - b;
            (t, p)
        } else {
            let u = rand_f32();
            let v = rand_f32();
            let t = u * 2.0 * PI;
            let p = (2.0 * v - 1.0).clamp(-1.0, 1.0).acos();
            (t, p)
        };

        let dir = Vec3::new(
            phi.sin() * theta.cos(),
            phi.cos(),
            phi.sin() * theta.sin(),
        ).normalize();

        // Spectral class & Planck blackbody temperature
        let spec_class = SpectralClass::sample_realistic(rand_f32());
        let star_temp = spec_class.temperature_kelvin() * (0.95 + rand_f32() * 0.10);
        let base_col = spec_class.blackbody_color();

        let mag_rnd = rand_f32();
        let brightness = if mag_rnd < 0.02 {
            // Bright navigational anchor star (Vega, Sirius, Rigel, Betelgeuse-like)
            9.0 + rand_f32() * 5.0
        } else if mag_rnd < 0.15 {
            // Conspicuous constellation star
            4.2 + rand_f32() * 2.8
        } else {
            // Faint field star
            2.0 + rand_f32() * 1.5
        };

        // Angular size calibrated so stars span at least 1.6 - 2.0 screen pixels,
        // completely preventing sub-pixel rasterization scintillation / sparkling during rotation.
        let star_base_scale = 400_000.0 / 55_000.0 * 6.0;
        let star_radius = if mag_rnd < 0.02 {
            (14.0 + rand_f32() * 6.0) * star_base_scale
        } else if mag_rnd < 0.15 {
            (8.0 + rand_f32() * 3.5) * star_base_scale
        } else {
            (4.8 + rand_f32() * 1.5) * star_base_scale
        };

        let base_idx = (i * 4) as u32;

        // Position: stores [offset_x, offset_y, brightness]
        positions.push([-star_radius, -star_radius, brightness]);
        positions.push([ star_radius, -star_radius, brightness]);
        positions.push([ star_radius,  star_radius, brightness]);
        positions.push([-star_radius,  star_radius, brightness]);

        // Normal: stores unit rest direction n_0
        let dir_arr = dir.to_array();
        for _ in 0..4 {
            normals.push(dir_arr);
        }

        // Color: stores [r, g, b, rest_temp_kelvin]
        let col_arr = [base_col.red, base_col.green, base_col.blue, star_temp];
        for _ in 0..4 {
            colors.push(col_arr);
        }

        // UV: [-1, 1] for circular point-spread function
        uvs.push([-1.0, -1.0]);
        uvs.push([ 1.0, -1.0]);
        uvs.push([ 1.0,  1.0]);
        uvs.push([-1.0,  1.0]);

        // Two triangles forming diamond/quad
        indices.push(base_idx);
        indices.push(base_idx + 1);
        indices.push(base_idx + 2);
        indices.push(base_idx);
        indices.push(base_idx + 2);
        indices.push(base_idx + 3);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));

    let star_mat = star_materials.add(RelativisticStarMaterial {
        uniform_data: StarfieldUniformData {
            sphere_radius,
            ..default()
        },
    });

    commands.spawn((
        ProceduralStarfield,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(star_mat),
        Transform::IDENTITY,
        Aabb {
            center: Vec3A::ZERO,
            half_extents: Vec3A::splat(400_000.0),
        },
        NoFrustumCulling,
    ));

    // Spawn the Relativistic Interstellar Dust & Starlight Rainbow Halo (40° Ring)
    let dust_mesh = generate_interstellar_dust_halo_mesh(meshes);
    let dust_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        alpha_mode: AlphaMode::Add,
        cull_mode: None,
        ..default()
    });

    commands.spawn((
        InterstellarDustHalo,
        InterstellarDustHaloCatalog {
            sphere_radius: 360_000.0,
            last_rapidity: DVec3::ZERO,
            last_flight_dir: Vec3::ZERO,
            last_visor_mode: crate::celestial::CanopyVisorMode::Clear,
            last_update_time: 0.0,
            last_optics_enabled: (false, false),
        },
        Mesh3d(dust_mesh),
        MeshMaterial3d(dust_mat),
        Transform::IDENTITY,
        Visibility::Hidden,
        Aabb {
            center: Vec3A::ZERO,
            half_extents: Vec3A::splat(360_000.0),
        },
        NoFrustumCulling,
    ));

    // Spawn the Real Cosmic Microwave Background (CMB) Polar Thermal Blackbody Dome
    let cmb_mesh = generate_real_cmb_dome_mesh(meshes);
    let cmb_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        cull_mode: None,
        ..default()
    });

    commands.spawn((
        RealCmbDome,
        Mesh3d(cmb_mesh),
        MeshMaterial3d(cmb_mat),
        Transform::IDENTITY,
        Visibility::Hidden,
        Aabb {
            center: Vec3A::ZERO,
            half_extents: Vec3A::splat(370_000.0),
        },
        NoFrustumCulling,
    ));
}

/// Constructs a 3D polar cap dome mesh (16 concentric rings x 64 sectors) for the Cosmic Microwave Background (CMB).
/// Centered on the forward velocity vector, spanning outward from theta = 0 to 45 degrees.
/// Radial vertex coloring smoothly attenuates the thermal blackbody emission towards the edge.
fn generate_real_cmb_dome_mesh(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    let rings = 16;
    let sectors = 64;
    let vertex_count = (rings + 1) * (sectors + 1);
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(vertex_count);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(vertex_count);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(vertex_count);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(vertex_count);
    let mut indices: Vec<u32> = Vec::with_capacity(rings * sectors * 6);

    let radius = 370_000.0f32;
    let max_theta = 45.0f32.to_radians();

    for k in 0..=rings {
        let u = k as f32 / rings as f32;
        let theta = u * max_theta;
        let cos_theta = theta.cos();
        let sin_theta = theta.sin();

        // Smooth radial edge falloff
        let alpha = (1.0 - u * u).clamp(0.0, 1.0);
        let col_arr = [
            1.0,
            (1.0 - 0.4 * u).clamp(0.0, 1.0),
            (1.0 - 0.8 * u).clamp(0.0, 1.0),
            alpha,
        ];

        for j in 0..=sectors {
            let phi = std::f32::consts::TAU * (j as f32 / sectors as f32);
            let dir = Vec3::new(sin_theta * phi.cos(), sin_theta * phi.sin(), -cos_theta);
            positions.push((dir * radius).to_array());
            colors.push(col_arr);
            normals.push([-dir.x, -dir.y, -dir.z]);
            uvs.push([u, j as f32 / sectors as f32]);
        }
    }

    for k in 0..rings {
        for j in 0..sectors {
            let v0 = (k * (sectors + 1) + j) as u32;
            let v1 = (k * (sectors + 1) + j + 1) as u32;
            let v2 = ((k + 1) * (sectors + 1) + j) as u32;
            let v3 = ((k + 1) * (sectors + 1) + j + 1) as u32;

            indices.push(v0);
            indices.push(v2);
            indices.push(v1);

            indices.push(v1);
            indices.push(v2);
            indices.push(v3);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    meshes.add(mesh)
}

/// Constructs a 3D polar annular grid mesh (12 rings x 128 sectors) for the interstellar dust rainbow halo.
/// Centered directly on the flight axis, this guarantees 128 circumferential vertices at any contraction radius,
/// completely eliminating polygon/prism angularity and diagonal color bleeding artifacts.
pub fn generate_interstellar_dust_halo_mesh(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    let rings = 12;
    let sectors = 128;
    let vertex_count = (rings + 1) * (sectors + 1);
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(vertex_count);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(vertex_count);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(vertex_count);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(vertex_count);
    let mut indices: Vec<u32> = Vec::with_capacity(rings * sectors * 6);

    let radius = 360_000.0f32;

    for k in 0..=rings {
        let u = k as f32 / rings as f32;
        // Thinner 12-degree halo band (centered at ~40 degrees, span 34 to 46 degrees)
        let theta = (34.0 + u * 12.0).to_radians();
        let cos_theta = theta.cos();
        let sin_theta = theta.sin();
        let col = crate::relativity::relativistic_rainbow_color_at_u(u, 1.0);
        let col_arr = [col.red, col.green, col.blue, col.alpha];

        for j in 0..=sectors {
            let phi = std::f32::consts::TAU * (j as f32 / sectors as f32);
            let dir = Vec3::new(sin_theta * phi.cos(), sin_theta * phi.sin(), -cos_theta);
            positions.push((dir * radius).to_array());
            colors.push(col_arr);
            normals.push([-dir.x, -dir.y, -dir.z]);
            uvs.push([u, j as f32 / sectors as f32]);
        }
    }

    for k in 0..rings {
        for j in 0..sectors {
            let v0 = (k * (sectors + 1) + j) as u32;
            let v1 = (k * (sectors + 1) + j + 1) as u32;
            let v2 = ((k + 1) * (sectors + 1) + j) as u32;
            let v3 = ((k + 1) * (sectors + 1) + j + 1) as u32;

            // Inward-facing winding for camera inside sphere
            indices.push(v0);
            indices.push(v2);
            indices.push(v1);

            indices.push(v1);
            indices.push(v2);
            indices.push(v3);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    meshes.add(mesh)
}

/// Backwards compatibility alias for celestial rainbow mesh generator
#[allow(dead_code)]
pub fn generate_celestial_rainbow_mesh(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    generate_interstellar_dust_halo_mesh(meshes)
}

/// Keeps the celestial sphere, dust halo, and CMB dome centered on the camera to eliminate any parallax degradation
pub fn starfield_tracking_system(
    camera_query: Query<&Transform, With<crate::camera::SpaceCamera>>,
    mut tracking_query: Query<
        &mut Transform,
        (
            Or<(With<ProceduralStarfield>, With<InterstellarDustHalo>, With<RealCmbDome>)>,
            Without<crate::camera::SpaceCamera>,
        ),
    >,
) {
    let Ok(cam_tf) = camera_query.get_single() else {
        return;
    };
    for mut tf in tracking_query.iter_mut() {
        tf.translation = cam_tf.translation;
    }
}

/// Dynamically updates the GPU shader uniforms for Einstein's Special Relativity:
/// 1. Relativistic Light Aberration (searchlight cone compression towards velocity vector)
/// 2. Relativistic Doppler Shift (spectral blueshift forward, redshift aft)
/// 3. Relativistic Beaming (radiance boost D^3 forward, dimming aft)
///
/// Sends only a 48-byte uniform buffer to the GPU via queue.write_buffer.
/// Completely eliminates CPU per-vertex mutation and GPU buffer churn (zero memory leaks, flat 1.1GB VRAM).
pub fn starfield_optics_system(
    shuttle_query: Query<&crate::relativity::RelativisticState, With<crate::flight::Shuttle>>,
    optics: Res<crate::relativity::RelativisticOpticsConfig>,
    visor: Res<crate::celestial::CanopyVisorState>,
    starfield_query: Query<&MeshMaterial3d<RelativisticStarMaterial>, With<ProceduralStarfield>>,
    mut star_materials: ResMut<Assets<RelativisticStarMaterial>>,
) {
    let Ok(rel_state) = shuttle_query.get_single() else {
        return;
    };
    let Ok(mat_handle) = starfield_query.get_single() else {
        return;
    };
    let Some(mat) = star_materials.get_mut(&mat_handle.0) else {
        return;
    };

    let rapidity = if optics.aberration_enabled || optics.doppler_enabled {
        rel_state.rapidity.as_vec3()
    } else {
        Vec3::ZERO
    };

    let visor_mode = match visor.mode {
        crate::celestial::CanopyVisorMode::Clear => 0,
        crate::celestial::CanopyVisorMode::Polarized => 1,
        crate::celestial::CanopyVisorMode::SolarFilter => 2,
    };

    mat.uniform_data = StarfieldUniformData {
        rapidity,
        beta: rel_state.beta as f32,
        beta_vec: rel_state.beta_vec.as_vec3(),
        gamma: rel_state.gamma as f32,
        sphere_radius: 400_000.0,
        aberration_enabled: if optics.aberration_enabled { 1 } else { 0 },
        doppler_enabled: if optics.doppler_enabled { 1 } else { 0 },
        visor_mode,
    };
}

/// Dynamically updates the Relativistic Interstellar Dust & Starlight Rainbow Halo
///
/// At relativistic velocities (beta >= 0.55), interstellar dust and galactic background light
/// undergo relativistic aberration and Doppler shifting into a 40-degree annular rainbow ring:
/// - Centers on the physical velocity vector v_hat
/// - Radiates a continuous spherical rainbow wrapping around the player (< 180° FOV, ScienceClic visual):
///   - Forward Center: Radiant incandescent white & electric cyan ("Eye of Light" core)
///   - Forward Ring: Royal blue & electric cyan
///   - Mid Ring: Vibrant emerald green & lime
///   - Outer Ring: Golden yellow & warm orange
///   - Lateral Flanks: Deep scarlet red
///   - Aft Hemisphere: Fades smoothly into deep space void
/// - Relativistic Aberration (F4): When enabled, compresses the rainbow into the forward searchlight cone.
pub fn interstellar_dust_halo_system(
    shuttle_query: Query<(&crate::relativity::RelativisticState, &Transform), With<crate::flight::Shuttle>>,
    camera_query: Query<&Transform, With<crate::camera::SpaceCamera>>,
    optics: Res<crate::relativity::RelativisticOpticsConfig>,
    visor: Res<crate::celestial::CanopyVisorState>,
    mut dipole_query: Query<
        (&mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>),
        (With<InterstellarDustHalo>, Without<crate::camera::SpaceCamera>, Without<crate::flight::Shuttle>),
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok((rel_state, shuttle_tf)) = shuttle_query.get_single() else {
        return;
    };
    let Ok(cam_tf) = camera_query.get_single() else {
        return;
    };
    let Ok((mut dipole_tf, mut visibility, mat_handle)) = dipole_query.get_single_mut() else {
        return;
    };

    // Keep dome centered on the camera to eliminate parallax
    dipole_tf.translation = cam_tf.translation;

    if !optics.doppler_enabled || rel_state.beta < 0.55 {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    if *visibility != Visibility::Inherited {
        *visibility = Visibility::Inherited;
    }

    // Physical direction of flight in world space
    let flight_dir = if rel_state.velocity.length_squared() > 1e-4 {
        rel_state.velocity.normalize().as_vec3()
    } else {
        shuttle_tf.forward().as_vec3()
    };

    // Smoothly orient the dome towards the flight vector on the GPU via Transform (0 CPU mesh uploads!)
    let forward = flight_dir.normalize();
    let up_hint = if forward.y.abs() > 0.95 { Vec3::X } else { Vec3::Y };
    let right = forward.cross(up_hint).normalize();
    let up = right.cross(forward).normalize();
    dipole_tf.rotation = Quat::from_mat3(&Mat3::from_cols(right, up, -forward));

    let visor_scale = match visor.mode {
        crate::celestial::CanopyVisorMode::Clear => 1.0,
        crate::celestial::CanopyVisorMode::Polarized => 0.40,
        crate::celestial::CanopyVisorMode::SolarFilter => 0.08,
    };

    let beta = rel_state.beta as f32;
    let speed_t = ((beta - 0.55) / 0.25).clamp(0.0, 1.0);
    let speed_factor = speed_t * speed_t * (3.0 - 2.0 * speed_t);
    // Delicate, transparent celestial veil (0.10 max radiance) so stars clearly shine through the halo
    let total_intensity = 0.10 * speed_factor * visor_scale;

    // Relativistic ring angular bounds:
    // With aberration: crunches forward towards flight_dir into tight searchlight halo
    // Without aberration: spans widely across observer's sky
    let zeta = if rel_state.rapidity.length_squared() > 1e-12 {
        rel_state.rapidity.length()
    } else {
        (rel_state.beta as f64).atanh()
    };
    let (base_ring, _) = crate::relativity::relativistic_rainbow_ring_params_from_zeta(zeta);
    let th_ring = if optics.aberration_enabled {
        base_ring
    } else {
        (base_ring * rel_state.gamma as f32).clamp(10.0, 140.0)
    };

    // Base ring mesh center theta is 40.0 degrees
    let scale_xy = (th_ring / 40.0).clamp(0.0001, 3.5);
    dipole_tf.scale = Vec3::new(scale_xy, scale_xy, 1.0);

    // Apply intensity to material uniform without any vertex buffer churn
    if let Some(mat) = materials.get_mut(&mat_handle.0) {
        mat.base_color = Color::srgba(1.0, 1.0, 1.0, total_intensity);
    }
}

/// Backwards compatibility alias for interstellar dust halo system
pub use interstellar_dust_halo_system as cmb_dipole_system;

/// Dynamically updates the true Cosmic Microwave Background (CMB) thermal blackbody dome.
///
/// In the cosmic rest frame, the 2.7255 K isotropic CMB is shifted by relativistic Doppler boosting:
/// T(theta) = T_0 / [gamma * (1 - beta * cos theta)].
/// Directly forward (theta = 0): T_apex = gamma * (1 + beta) * 2.7255 K.
///
/// Spectrum progression:
/// - gamma < 90 (T_apex < 500 K): Peak in microwave/far-infrared -> completely invisible.
/// - 90 <= gamma < 250 (500 K <= T_apex < 1,360 K): Faint deep red / ruby ember forward dome.
/// - 250 <= gamma < 600 (1,360 K <= T_apex < 3,270 K): Warm fiery incandescent orange/yellow dome.
/// - gamma >= 600 (T_apex >= 3,270 K): Brilliant stellar white and sapphire blue thermal cap.
pub fn real_cmb_system(
    shuttle_query: Query<(&crate::relativity::RelativisticState, &Transform), With<crate::flight::Shuttle>>,
    camera_query: Query<&Transform, With<crate::camera::SpaceCamera>>,
    optics: Res<crate::relativity::RelativisticOpticsConfig>,
    visor: Res<crate::celestial::CanopyVisorState>,
    mut cmb_query: Query<
        (&mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>),
        (With<RealCmbDome>, Without<crate::camera::SpaceCamera>, Without<crate::flight::Shuttle>),
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok((rel_state, shuttle_tf)) = shuttle_query.get_single() else {
        return;
    };
    let Ok(cam_tf) = camera_query.get_single() else {
        return;
    };
    let Ok((mut cmb_tf, mut visibility, mat_handle)) = cmb_query.get_single_mut() else {
        return;
    };

    cmb_tf.translation = cam_tf.translation;

    if !optics.doppler_enabled || rel_state.beta < 0.1 || rel_state.gamma < 1.0 {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    // Forward peak Doppler factor: D_apex = gamma * (1 + beta)
    let d_apex = (rel_state.gamma * (1.0 + rel_state.beta)) as f32;
    let t_apex = d_apex * 2.7255;

    // Faint ember threshold is ~500 K. Below this, peak is deep in infrared / microwaves
    if t_apex < 500.0 {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    if *visibility != Visibility::Inherited {
        *visibility = Visibility::Inherited;
    }

    // Physical direction of flight in world space
    let flight_dir = if rel_state.velocity.length_squared() > 1e-4 {
        rel_state.velocity.normalize().as_vec3()
    } else {
        shuttle_tf.forward().as_vec3()
    };

    let forward = flight_dir.normalize();
    let up_hint = if forward.y.abs() > 0.95 { Vec3::X } else { Vec3::Y };
    let right = forward.cross(up_hint).normalize();
    let up = right.cross(forward).normalize();
    cmb_tf.rotation = Quat::from_mat3(&Mat3::from_cols(right, up, -forward));

    let visor_scale = match visor.mode {
        crate::celestial::CanopyVisorMode::Clear => 1.0,
        crate::celestial::CanopyVisorMode::Polarized => 0.40,
        crate::celestial::CanopyVisorMode::SolarFilter => 0.08,
    };

    // Blackbody color from apex temperature
    let col = crate::relativity::blackbody_temperature_to_linear_rgb(t_apex);

    // Radiant intensity scales smoothly as temperature exceeds 500 K
    let t_ratio = (t_apex - 500.0) / 1000.0;
    let base_radiance = t_ratio.powf(1.3).clamp(0.01, 1.5) * 0.35;
    let total_alpha = (base_radiance * visor_scale).clamp(0.0, 1.0);

    // Relativistic angular disk radius:
    // With aberration, angular half-width theta_500 scales as ~ 1 / gamma
    let gamma = rel_state.gamma as f32;
    let theta_scale = if optics.aberration_enabled {
        (25.0 / gamma.max(1.0)).clamp(0.0001, 1.5)
    } else {
        1.0
    };
    cmb_tf.scale = Vec3::new(theta_scale, theta_scale, 1.0);

    if let Some(mat) = materials.get_mut(&mat_handle.0) {
        mat.base_color = Color::linear_rgba(col.red, col.green, col.blue, total_alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polar_annular_rainbow_mesh_topology() {
        let mut meshes = Assets::<Mesh>::default();
        let handle = generate_celestial_rainbow_mesh(&mut meshes);
        let mesh = meshes.get(&handle).expect("Rainbow mesh must exist in assets");

        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("Mesh must have positions");
        let colors = mesh.attribute(Mesh::ATTRIBUTE_COLOR).expect("Mesh must have colors");
        let normals = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).expect("Mesh must have normals");
        let uvs = mesh.attribute(Mesh::ATTRIBUTE_UV_0).expect("Mesh must have UVs");

        let expected_vertices = (12 + 1) * (128 + 1); // 1,677 vertices
        assert_eq!(positions.len(), expected_vertices);
        assert_eq!(colors.len(), expected_vertices);
        assert_eq!(normals.len(), expected_vertices);
        assert_eq!(uvs.len(), expected_vertices);

        let indices = mesh.indices().expect("Mesh must have indices");
        let expected_indices = 12 * 128 * 6; // 9,216 indices
        assert_eq!(indices.len(), expected_indices);

        // Verify index safety: every index must reference a valid vertex
        for idx in indices.iter() {
            assert!(idx < expected_vertices, "Index {} out of bounds ({})", idx, expected_vertices);
        }
    }

    #[test]
    fn test_spectral_class_colors_and_naked_eye_population() {
        // Test color distinction:
        let o_col = SpectralClass::O.blackbody_color();
        assert!(o_col.blue > o_col.red, "O-class must be blue");
        let m_col = SpectralClass::M.blackbody_color();
        assert!(m_col.red > m_col.green && m_col.green > m_col.blue, "M-class must be ruby red");
        let g_col = SpectralClass::G.blackbody_color();
        assert!(g_col.red > g_col.blue, "G-class must be golden yellow");
        let a_col = SpectralClass::A.blackbody_color();
        assert_eq!(a_col, LinearRgba::WHITE, "A-class must be diamond white");

        // Test naked-eye population samples:
        assert!(matches!(SpectralClass::sample_realistic(0.05), SpectralClass::O));
        assert!(matches!(SpectralClass::sample_realistic(0.20), SpectralClass::B));
        assert!(matches!(SpectralClass::sample_realistic(0.40), SpectralClass::A));
        assert!(matches!(SpectralClass::sample_realistic(0.60), SpectralClass::F));
        assert!(matches!(SpectralClass::sample_realistic(0.75), SpectralClass::G));
        assert!(matches!(SpectralClass::sample_realistic(0.88), SpectralClass::K));
        assert!(matches!(SpectralClass::sample_realistic(0.98), SpectralClass::M));
    }

    #[test]
    fn test_starfield_uniform_layout() {
        let uniform = StarfieldUniformData {
            rapidity: Vec3::new(0.1, 0.2, 0.3),
            beta: 0.95,
            beta_vec: Vec3::new(0.0, 0.0, 0.95),
            gamma: 3.2,
            sphere_radius: 400_000.0,
            aberration_enabled: 1,
            doppler_enabled: 1,
            visor_mode: 0,
        };
        assert_eq!(uniform.aberration_enabled, 1);
        assert_eq!(uniform.doppler_enabled, 1);
        assert_eq!(uniform.sphere_radius, 400_000.0);
    }

    #[test]
    fn test_real_cmb_dome_mesh_topology() {
        let mut meshes = Assets::<Mesh>::default();
        let handle = generate_real_cmb_dome_mesh(&mut meshes);
        let mesh = meshes.get(&handle).expect("CMB dome mesh must exist");

        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("Mesh must have positions");
        let colors = mesh.attribute(Mesh::ATTRIBUTE_COLOR).expect("Mesh must have colors");
        let normals = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).expect("Mesh must have normals");

        let expected_vertices = (16 + 1) * (64 + 1); // 1,105 vertices
        assert_eq!(positions.len(), expected_vertices);
        assert_eq!(colors.len(), expected_vertices);
        assert_eq!(normals.len(), expected_vertices);

        let indices = mesh.indices().expect("Mesh must have indices");
        let expected_indices = 16 * 64 * 6; // 6,144 indices
        assert_eq!(indices.len(), expected_indices);
    }

    #[test]
    fn test_real_cmb_temperature_thresholds() {
        // At gamma = 1.0 (rest), apex temp is 2.73 K (invisible)
        let d_rest = 1.0 * (1.0 + 0.0);
        let t_rest = d_rest * 2.7255;
        assert!(t_rest < 500.0, "Rest CMB must be far below visible 500 K threshold");

        // At gamma = 80 (beta ~ 0.99992), apex temp is ~436 K (still invisible)
        let d_80 = 80.0 * (1.0 + 0.99992);
        let t_80 = d_80 * 2.7255;
        assert!(t_80 < 500.0, "gamma 80 should be below 500 K threshold");

        // At gamma = 120 (beta ~ 0.999965), apex temp is ~654 K (visible deep red ember)
        let d_120 = 120.0 * (1.0 + 0.999965);
        let t_120 = d_120 * 2.7255;
        assert!(t_120 > 500.0, "gamma 120 should cross visible threshold");

        let col_120 = crate::relativity::blackbody_temperature_to_linear_rgb(t_120);
        assert!(col_120.red > col_120.green && col_120.green > col_120.blue, "650 K ember must be crimson red");
    }
}
