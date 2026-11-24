mod camera;
mod celestial;
mod coords;
mod flight;
mod navigation;
mod radar;
mod relativity;
mod stars;
mod ui;
mod world;

use bevy::core_pipeline::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};
use bevy::core_pipeline::fxaa::Fxaa;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::settings::{PowerPreference, RenderCreation, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::window::{
    CursorGrabMode, Monitor, MonitorSelection, PresentMode, PrimaryMonitor, PrimaryWindow,
    WindowMode, WindowPosition,
};

use camera::{
    app_exit_system, camera_follow_system, camera_toggle_system, canopy_visor_camera_system,
    cursor_grab_system, SpaceCamera,
};
use celestial::{
    axial_rotation_system, canopy_visor_input_system, celestial_optics_system,
    gpu_texture_quality_init_system, planet_distance_texture_streaming_system, planet_orbit_system,
    satellite_orbit_system, texture_quality_input_system, texture_quality_scaling_system,
    universe_switch_system, CanopyVisorState, DynamicTextureStreamingConfig, PlanetTextureRamCache,
    TextureQualityConfig, UniverseConfig,
};
use coords::floating_origin_system;
use flight::{
    fps_tracking_system, pause_toggle_system, shuttle_input_system, shuttle_time_system,
    update_engine_plumes, vsync_toggle_system, FpsTracker, SimulationPause,
};
use navigation::{
    navigation_targeting_system, setup_navigation_ui, update_navigation_reticle_system,
    update_target_screen_position_system, NavigationComputer,
};
use radar::update_orbital_radar_system;
use relativity::{relativistic_config_system, RelativisticConfig, RelativisticOpticsConfig};
use stars::{
    cmb_dipole_system, real_cmb_system, starfield_optics_system, starfield_tracking_system,
};
use ui::{
    canopy_visor_ui_system, crosshair_hover_system, fade_startup_credits_toast_system,
    hud_display_input_system, hud_display_update_system, setup_loading_screen, setup_ui,
    settings_overlay_input_system, target_rolling_button_system, universe_switch_button_system,
    update_loading_screen_system, update_relativistic_gauges_system, update_settings_overlay_system,
    update_ui_system, HudDisplayState, SettingsOverlayState, StartupLoadingTracker,
};
use world::{
    canopy_visor_glass_system, configure_ship_materials_system, pulse_beacons_system,
    pulse_runway_lights_system, rotate_landmarks_system, spawn_world, sun_light_tracking_system,
};

fn main() {
    // If launched directly without cargo run or BEVY_ASSET_ROOT, check if ./assets exists in the current working directory
    if std::env::var("BEVY_ASSET_ROOT").is_err() && std::env::var("CARGO_MANIFEST_DIR").is_err() {
        if std::path::Path::new("assets").is_dir() {
            std::env::set_var("BEVY_ASSET_ROOT", ".");
        }
    }

    App::new()
        // Default plugins with NVIDIA HighPerformance Vulkan preference and window settings
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Relativistic Space Simulator (Phase 4: Relativistic Visual Shaders & Optics)".into(),
                        mode: WindowMode::Windowed,
                        resolution: (1600.0_f32, 900.0_f32).into(),
                        position: WindowPosition::Automatic,
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(WgpuSettings {
                        power_preference: PowerPreference::HighPerformance,
                        ..default()
                    }),
                    ..default()
                }),
        )
        // Custom GPU Relativistic Starfield Material Plugin (hardware vertex aberration & Planck Doppler)
        .add_plugins(MaterialPlugin::<crate::stars::RelativisticStarMaterial>::default())
        // Space vacuum background color
        .insert_resource(ClearColor(Color::srgb(0.005, 0.007, 0.015)))
        // Subtle cosmos starlight ambient light
        .insert_resource(AmbientLight {
            color: Color::srgb(0.35, 0.45, 0.65),
            brightness: 4.0,
        })
        // Relativistic physics configuration
        .insert_resource(RelativisticConfig::default())
        // Relativistic optical shaders & visual deformation configuration
        .insert_resource(RelativisticOpticsConfig::default())
        // Universe scale & solar system sector configuration (Demo Sector vs 1:1 Realistic Scale)
        .insert_resource(UniverseConfig::default())
        // Simulation pause state
        .insert_resource(SimulationPause::default())
        // Frame rate and performance tracker
        .insert_resource(FpsTracker::default())
        // Navigation and target tracking computer
        .insert_resource(NavigationComputer::default())
        // Optical canopy visor and solar filter mode state
        .insert_resource(CanopyVisorState::default())
        // Cockpit MFD settings & help overlay modal state
        .insert_resource(SettingsOverlayState::default())
        // Active HUD display mode (Full / Minimal / Hidden)
        .insert_resource(HudDisplayState::default())
        // Audio and music settings (master volume and mute state)
        .insert_resource(AudioSettings::default())
        // Dynamic GPU adapter texture quality scaling & resolution cap
        .init_resource::<TextureQualityConfig>()
        // Dynamic distance-based planetary texture streaming / VRAM LOD
        .init_resource::<DynamicTextureStreamingConfig>()
        // In-memory CPU texture pyramid cache (auto-enabled on >= 24GB RAM)
        .init_resource::<PlanetTextureRamCache>()
        // Startup asset loading tracker & cinematic loading screen state
        .init_resource::<StartupLoadingTracker>()
        // Auto-fullscreen initialization state
        .init_resource::<StartupFullscreenState>()
        // Startup systems
        .add_systems(
            Startup,
            (
                setup_window_cursor,
                spawn_world,
                setup_ui,
                setup_navigation_ui,
                setup_loading_screen,
                setup_camera,
                setup_soundtrack,
            ),
        )
        // Flight, input, camera & visor controls
        .add_systems(
            Update,
            (
                update_loading_screen_system,
                auto_fullscreen_startup_system,
                gpu_texture_quality_init_system,
                texture_quality_scaling_system,
                texture_quality_input_system,
                planet_distance_texture_streaming_system,
                fullscreen_toggle_system,
                pause_toggle_system,
                vsync_toggle_system,
                fps_tracking_system,
                update_engine_plumes,
                camera_toggle_system,
                cursor_grab_system,
                (settings_overlay_input_system, app_exit_system).chain(),
                relativistic_config_system,
                canopy_visor_input_system,
                canopy_visor_camera_system,
                canopy_visor_glass_system,
                universe_switch_system,
                universe_switch_button_system,
            ),
        )
        .add_systems(Update, hud_display_input_system)
        // World animation, celestial dynamics, synchronized physics pipeline & UI updates
        .add_systems(
            Update,
            (
                rotate_landmarks_system,
                pulse_beacons_system,
                pulse_runway_lights_system,
                sun_light_tracking_system,
                configure_ship_materials_system,
                update_relativistic_gauges_system,
                canopy_visor_ui_system,
                target_rolling_button_system,
                update_settings_overlay_system,
                fade_startup_credits_toast_system,
                audio_mute_toggle_system,
                // Synchronized celestial axial rotation & orbital motion -> shuttle physics & collision -> targeting -> UI display:
                (
                    shuttle_time_system,
                    axial_rotation_system,
                    planet_orbit_system,
                    satellite_orbit_system,
                    shuttle_input_system,
                    navigation_targeting_system,
                    update_ui_system,
                    hud_display_update_system,
                    update_orbital_radar_system,
                )
                    .chain(),
            ),
        )
        // In PostUpdate: compute floating origin first, then align camera, then anchor and optically deform starfield,
        // and finally compute screen-space HUD targeting projections directly from the updated transforms
        .add_systems(
            PostUpdate,
            (
                floating_origin_system,
                camera_follow_system,
                starfield_tracking_system,
                starfield_optics_system,
                cmb_dipole_system,
                real_cmb_system,
                celestial_optics_system,
                update_target_screen_position_system,
                update_navigation_reticle_system,
                crosshair_hover_system,
            )
                .chain(),
        )
        .run();
}

#[derive(Resource, Default)]
pub struct StartupFullscreenState {
    pub frame_count: u32,
    pub base_width: f32,
    pub base_height: f32,
    pub engaged: bool,
}

fn setup_window_cursor(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if let Ok(mut window) = windows.get_single_mut() {
        window.cursor_options.grab_mode = CursorGrabMode::Locked;
        window.cursor_options.visible = false;
    }
}

/// Starts windowed so KDE/KWin registers the windowed geometry properly,
/// then auto-engages Borderless Fullscreen cleanly without stretching or zooming.
fn auto_fullscreen_startup_system(
    mut state: ResMut<StartupFullscreenState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    monitors: Query<&Monitor, With<PrimaryMonitor>>,
) {
    if state.engaged {
        return;
    }

    state.frame_count += 1;

    // Record monitor-proportional windowed resolution (80% of screen)
    if state.base_width == 0.0 {
        if let Ok(monitor) = monitors.get_single() {
            let scale = if monitor.scale_factor > 0.0 { monitor.scale_factor } else { 1.0 };
            let logical_w = monitor.physical_width as f64 / scale;
            let logical_h = monitor.physical_height as f64 / scale;
            state.base_width = (logical_w * 0.80) as f32;
            state.base_height = (logical_h * 0.80) as f32;
        } else {
            state.base_width = 1600.0;
            state.base_height = 900.0;
        }
    }

    // After 2 frames of windowed registration, transition cleanly to Borderless Fullscreen
    if state.frame_count >= 2 {
        if let Ok(mut window) = windows.get_single_mut() {
            window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Primary);
            state.engaged = true;
            info!(
                "[Display] Windowed base recorded ({}x{} @ ~80% screen) -> Engaged Borderless Fullscreen",
                state.base_width as u32, state.base_height as u32
            );
        }
    }
}

fn fullscreen_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    state: Res<StartupFullscreenState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if keyboard.just_pressed(KeyCode::F11) {
        if let Ok(mut window) = windows.get_single_mut() {
            window.mode = match window.mode {
                WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
                _ => {
                    let w = if state.base_width > 0.0 { state.base_width } else { 1600.0 };
                    let h = if state.base_height > 0.0 { state.base_height } else { 900.0 };
                    window.resolution.set(w, h);
                    window.position = WindowPosition::Centered(MonitorSelection::Primary);
                    WindowMode::Windowed
                }
            };
            info!("[Display] Fullscreen mode toggled via [F11]: {:?}", window.mode);
        }
    }
}

fn setup_camera(mut commands: Commands) {
    // 3D Flight Camera with HDR, Bloom, FXAA Anti-Aliasing, and deep-space astronomical far plane (500km)
    commands.spawn((
        Camera3d::default(),
        Camera {
            hdr: true,
            ..default()
        },
        Msaa::Off,
        Fxaa::default(),
        Projection::Perspective(PerspectiveProjection {
            near: 0.1,
            far: 1.0e16,
            fov: std::f32::consts::PI / 4.0,
            ..default()
        }),
        Tonemapping::AcesFitted,
        Bloom {
            intensity: 0.22,
            composite_mode: BloomCompositeMode::Additive,
            prefilter: BloomPrefilter {
                threshold: 1.8,
                threshold_softness: 0.35,
            },
            ..default()
        },
        SpaceCamera::default(),
        Transform::from_xyz(0.0, 3.5, 14.0).looking_at(Vec3::new(0.0, 0.0, -10.0), Vec3::Y),
    ));
}

#[derive(Component)]
pub struct BackgroundMusic;

#[derive(Resource)]
pub struct AudioSettings {
    pub music_muted: bool,
    pub master_volume: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            music_muted: false,
            master_volume: 0.25,
        }
    }
}

fn setup_soundtrack(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    audio_settings: Res<AudioSettings>,
) {
    let ogg_path = std::path::Path::new("assets/audio/scienceclic.ogg");
    let flac_path = std::path::Path::new("assets/audio/scienceclic.flac");

    let audio_track = if ogg_path.exists() {
        Some("audio/scienceclic.ogg")
    } else if flac_path.exists() {
        Some("audio/scienceclic.flac")
    } else {
        None
    };

    if let Some(track) = audio_track {
        commands.spawn((
            AudioPlayer::new(asset_server.load(track)),
            PlaybackSettings::LOOP.with_volume(bevy::audio::Volume::new(audio_settings.master_volume)),
            BackgroundMusic,
        ));
        info!("================================================================================");
        info!("♫ SPACE-EXPLORER SOUNDTRACK: ScienceClic (Musique mystérieuse)");
        info!("  Track: {}", track);
        info!("  Composer: Alessandro Roussel (Album: ScienceClic)");
        info!("  Inspiration: ScienceClic - \"Que verrait-on à la vitesse de la lumière ?\"");
        info!("  Video URL: https://www.youtube.com/watch?v=rrreYWD5_z8");
        info!("  Music URL: https://www.youtube.com/watch?v=DBqgHwd4QYo");
        info!("  Shortcut: Press [M] to toggle mute (supports AZERTY and QWERTY)");
        info!("================================================================================");
    } else {
        info!("No background audio track found at assets/audio/ (running without music).");
    }
}

fn audio_mute_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut audio_settings: ResMut<AudioSettings>,
    music_query: Query<&AudioSink, With<BackgroundMusic>>,
) {
    // KeyCode::KeyM: physical 'M' on QWERTY or ',' on AZERTY
    // KeyCode::Semicolon: physical 'M' on AZERTY (right of L)
    let m_pressed = keyboard.just_pressed(KeyCode::KeyM) || keyboard.just_pressed(KeyCode::Semicolon);
    if m_pressed {
        audio_settings.music_muted = !audio_settings.music_muted;
        for sink in music_query.iter() {
            if audio_settings.music_muted {
                sink.pause();
                info!("♫ Soundtrack paused (Muted via [M])");
            } else {
                sink.play();
                sink.set_volume(audio_settings.master_volume);
                info!("♫ Soundtrack resumed (Unmuted via [M])");
            }
        }
    }
}
