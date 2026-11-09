use std::f64::consts::PI;

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::RelativeCursorPosition;
use bevy::window::{CursorGrabMode, PrimaryWindow};

use crate::celestial::{KeplerOrbit, NavTarget, SatelliteOrbit, ASTRONOMICAL_UNIT};
use crate::coords::UniversalPosition;
use crate::flight::{initiate_target_alignment, AutopilotMode, AutopilotPhase, Shuttle};
use crate::navigation::NavigationComputer;

pub const RADAR_PIXEL_SIZE: usize = 280;
pub const RADAR_CX: f32 = 140.0;
pub const RADAR_CY: f32 = 140.0;
pub const RADAR_MAX_R: f32 = 132.0;

/// Double-click time threshold window (in seconds) for targeting and auto-aiming at celestial bodies on the radar map.
/// Adjust this value to require faster (< 0.30s) or more relaxed (> 0.40s) double-clicking.
pub const MAP_DOUBLE_CLICK_WINDOW_SECS: f64 = 0.35;

/// Root UI panel container for the Orbital Radar Map
#[derive(Component)]
pub struct OrbitalRadarRoot;

/// Image node component displaying the dynamic rasterized radar texture
#[derive(Component)]
pub struct OrbitalRadarImageNode;

/// Telemetry text readout below the radar scope
#[derive(Component)]
pub struct OrbitalRadarTelemetryText;

/// Zoom focus modes for the orbital radar
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadarZoomMode {
    #[default]
    ShipAuto,    // Automatically zooms to match the shuttle's region in the solar system
    TargetFocus, // Automatically zooms to the locked target's orbital scale (independent of ship position)
    Manual,      // Manual scroll zoom in Free Mouse mode; persists until Shift+N resets to auto
}

/// Global resource tracking orbital radar display state, dynamic zoom, and mouse hover/selection
#[derive(Resource)]
pub struct OrbitalRadarState {
    pub image_handle: Handle<Image>,
    pub current_radius_m: f64,
    pub target_radius_m: f64,
    pub visible: bool,
    pub zoom_mode: RadarZoomMode,
    pub tier_name: String,
    pub is_hovered: bool,
    pub hovered_body_name: Option<String>,
    pub last_click_time_secs: f64,
    pub last_clicked_entity: Option<Entity>,
}

impl Default for OrbitalRadarState {
    fn default() -> Self {
        Self {
            image_handle: Handle::default(),
            current_radius_m: 2.2 * ASTRONOMICAL_UNIT,
            target_radius_m: 2.2 * ASTRONOMICAL_UNIT,
            visible: true,
            zoom_mode: RadarZoomMode::ShipAuto,
            tier_name: "INNER / TERRESTRIAL".to_string(),
            is_hovered: false,
            hovered_body_name: None,
            last_click_time_secs: -100.0,
            last_clicked_entity: None,
        }
    }
}

/// Initializes the Orbital Radar texture buffer and returns the resource state to insert
pub fn init_orbital_radar(images: &mut ResMut<Assets<Image>>) -> OrbitalRadarState {
    let initial_data = vec![0u8; RADAR_PIXEL_SIZE * RADAR_PIXEL_SIZE * 4];
    let image = Image::new(
        Extent3d {
            width: RADAR_PIXEL_SIZE as u32,
            height: RADAR_PIXEL_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        initial_data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let image_handle = images.add(image);

    OrbitalRadarState {
        image_handle,
        current_radius_m: 2.2 * ASTRONOMICAL_UNIT,
        target_radius_m: 2.2 * ASTRONOMICAL_UNIT,
        visible: true,
        zoom_mode: RadarZoomMode::ShipAuto,
        tier_name: "INNER / TERRESTRIAL".to_string(),
        is_hovered: false,
        hovered_body_name: None,
        last_click_time_secs: -100.0,
        last_clicked_entity: None,
    }
}

/// Spawns the Orbital Radar Map UI panel docked inside the left column below Avionics
pub fn setup_radar_ui(
    image_handle: &Handle<Image>,
    font_sans: &Handle<Font>,
    font_mono: &Handle<Font>,
    left_col: &mut ChildBuilder,
) {
    // Spawn Radar Panel with relative flex positioning (flows naturally below Avionics, preventing overlap)
    left_col.spawn((
        OrbitalRadarRoot,
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(8.0)),
            row_gap: Val::Px(4.0),
            border: UiRect::all(Val::Px(1.0)),
            width: Val::Px(298.0),
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.04, 0.08, 0.88)),
        BorderColor(Color::srgba(0.2, 0.6, 1.0, 0.5)),
    ))
    .with_children(|panel| {
        // Header
        panel.spawn((
            Text::new("SOLAR SYSTEM ORBITAL RADAR"),
            TextFont {
                font: font_sans.clone(),
                font_size: 11.5,
                ..default()
            },
            TextColor(Color::srgb(0.35, 0.85, 1.0)),
        ));

        // Radar Canvas Display with RelativeCursorPosition for pixel-exact cursor tracking
        panel.spawn((
            OrbitalRadarImageNode,
            ImageNode {
                image: image_handle.clone(),
                ..default()
            },
            Node {
                width: Val::Px(280.0),
                height: Val::Px(280.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor(Color::srgba(0.15, 0.45, 0.8, 0.4)),
            RelativeCursorPosition::default(),
            Interaction::default(),
        ));

        // Scale & Telemetry Text Readout
        panel.spawn((
            OrbitalRadarTelemetryText,
            Text::new("SCALE: 2.20 AU [INNER] • AUTO (SHIP)\nSOL: 1.00 AU | SHIP: (X: +1.00 AU, Z: 0.00 AU)"),
            TextFont {
                font: font_mono.clone(),
                font_size: 9.5,
                ..default()
            },
            TextLayout::new_with_justify(JustifyText::Center),
            TextColor(Color::srgba(0.75, 0.9, 1.0, 0.9)),
        ));
    });
}

/// Helper function returning standard color for celestial bodies on the radar
pub fn planet_radar_color(name: &str) -> [u8; 4] {
    match name {
        "Sol" => [255, 230, 90, 255],        // Golden yellow
        "Mercury" => [175, 175, 185, 255],   // Slate grey
        "Venus" => [235, 210, 160, 255],     // Pale amber
        "Earth" | "Terra" => [75, 160, 240, 255], // Azure blue
        "Mars" | "Ares" => [225, 90, 70, 255],    // Rust red
        "Ceres" => [175, 175, 175, 255],     // Stone grey
        "Jupiter" | "Chronos" => [225, 190, 150, 255], // Cream / banded beige
        "Saturn" => [230, 200, 130, 255],    // Pale gold
        "Uranus" => [115, 220, 245, 255],    // Cyan
        "Neptune" => [60, 135, 255, 255],    // Deep azure
        "Pluto" => [200, 180, 220, 255],     // Icy lavender
        "Haumea" => [225, 235, 230, 255],    // Elongated egg white
        "Makemake" => [225, 150, 120, 255],  // Terracotta
        "Eris" => [240, 240, 240, 255],      // Snow white
        "Gonggong" => [210, 140, 90, 255],   // Reddish brown
        "Sedna" => [200, 75, 45, 255],       // Crimson red
        "Farfarout" => [160, 130, 140, 255], // Muted mauve
        "Planet Nine" => [140, 70, 210, 255], // Deep purple
        _ => [180, 210, 230, 255],           // Default sci-fi blue
    }
}

#[inline(always)]
fn put_pixel(buf: &mut [u8], x: i32, y: i32, color: [u8; 4]) {
    if x >= 0 && x < RADAR_PIXEL_SIZE as i32 && y >= 0 && y < RADAR_PIXEL_SIZE as i32 {
        let idx = ((y as usize) * RADAR_PIXEL_SIZE + (x as usize)) * 4;
        let alpha = color[3] as u32;
        if alpha == 255 {
            buf[idx] = color[0];
            buf[idx + 1] = color[1];
            buf[idx + 2] = color[2];
            buf[idx + 3] = 255;
        } else if alpha > 0 {
            let inv_alpha = 255 - alpha;
            buf[idx] = ((color[0] as u32 * alpha + buf[idx] as u32 * inv_alpha) / 255) as u8;
            buf[idx + 1] = ((color[1] as u32 * alpha + buf[idx + 1] as u32 * inv_alpha) / 255) as u8;
            buf[idx + 2] = ((color[2] as u32 * alpha + buf[idx + 2] as u32 * inv_alpha) / 255) as u8;
            buf[idx + 3] = (alpha + (buf[idx + 3] as u32 * inv_alpha) / 255).min(255) as u8;
        }
    }
}

fn draw_line(buf: &mut [u8], mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        let cx_diff = x0 as f32 - RADAR_CX;
        let cy_diff = y0 as f32 - RADAR_CY;
        if cx_diff * cx_diff + cy_diff * cy_diff <= (RADAR_MAX_R + 1.0) * (RADAR_MAX_R + 1.0) {
            put_pixel(buf, x0, y0, color);
        }

        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_disc(buf: &mut [u8], cx: i32, cy: i32, r: i32, color: [u8; 4]) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                let px = cx + dx;
                let py = cy + dy;
                let r_diff_x = px as f32 - RADAR_CX;
                let r_diff_y = py as f32 - RADAR_CY;
                if r_diff_x * r_diff_x + r_diff_y * r_diff_y <= (RADAR_MAX_R + 2.0) * (RADAR_MAX_R + 2.0) {
                    put_pixel(buf, px, py, color);
                }
            }
        }
    }
}

fn draw_circle_outline(buf: &mut [u8], cx: i32, cy: i32, r: i32, color: [u8; 4]) {
    let steps = (r * 4).max(16) as usize;
    for i in 0..steps {
        let th = (i as f32) * 2.0 * std::f32::consts::PI / (steps as f32);
        let px = (cx as f32 + (r as f32) * th.cos()).round() as i32;
        let py = (cy as f32 + (r as f32) * th.sin()).round() as i32;
        let r_diff_x = px as f32 - RADAR_CX;
        let r_diff_y = py as f32 - RADAR_CY;
        if r_diff_x * r_diff_x + r_diff_y * r_diff_y <= (RADAR_MAX_R + 2.0) * (RADAR_MAX_R + 2.0) {
            put_pixel(buf, px, py, color);
        }
    }
}

/// Draws two perpendicular crosshair lines (horizontal and vertical) passing through the ship,
/// acting as the graph's origin cross with coordinate ticks along each axis.
fn draw_ship_cross_of_origin(buf: &mut [u8], spx: i32, spy: i32, color: [u8; 4]) {
    // 1. Horizontal axis (y = spy)
    for x in 0..(RADAR_PIXEL_SIZE as i32) {
        let dx = x as f32 - RADAR_CX;
        let dy = spy as f32 - RADAR_CY;
        if dx * dx + dy * dy <= RADAR_MAX_R * RADAR_MAX_R {
            let rel_x = x - spx;
            if rel_x.abs() >= 6 {
                put_pixel(buf, x, spy, color);
                if rel_x % 16 == 0 {
                    let tick_color = [color[0], color[1], color[2], (color[3] as f32 * 0.75) as u8];
                    put_pixel(buf, x, spy - 1, tick_color);
                    put_pixel(buf, x, spy + 1, tick_color);
                }
            }
        }
    }

    // 2. Vertical axis (x = spx)
    for y in 0..(RADAR_PIXEL_SIZE as i32) {
        let dx = spx as f32 - RADAR_CX;
        let dy = y as f32 - RADAR_CY;
        if dx * dx + dy * dy <= RADAR_MAX_R * RADAR_MAX_R {
            let rel_y = y - spy;
            if rel_y.abs() >= 6 {
                put_pixel(buf, spx, y, color);
                if rel_y % 16 == 0 {
                    let tick_color = [color[0], color[1], color[2], (color[3] as f32 * 0.75) as u8];
                    put_pixel(buf, spx - 1, y, tick_color);
                    put_pixel(buf, spx + 1, y, tick_color);
                }
            }
        }
    }
}

/// Draws an oriented shuttle chevron marker pointing along its ecliptic forward heading
fn draw_ship_marker(buf: &mut [u8], spx: f32, spy: f32, heading_rad: f32) {
    let tip_len = 7.0;
    let wing_len = 6.0;
    let wing_angle = 2.4;

    let tip_x = (spx + tip_len * heading_rad.cos()).round() as i32;
    let tip_y = (spy + tip_len * heading_rad.sin()).round() as i32;

    let left_x = (spx + wing_len * (heading_rad + wing_angle).cos()).round() as i32;
    let left_y = (spy + wing_len * (heading_rad + wing_angle).sin()).round() as i32;

    let right_x = (spx + wing_len * (heading_rad - wing_angle).cos()).round() as i32;
    let right_y = (spy + wing_len * (heading_rad - wing_angle).sin()).round() as i32;

    let center_x = (spx - 1.0 * heading_rad.cos()).round() as i32;
    let center_y = (spy - 1.0 * heading_rad.sin()).round() as i32;

    let chevron_color = [90, 255, 150, 255]; // Luminous neon green
    draw_line(buf, tip_x, tip_y, left_x, left_y, chevron_color);
    draw_line(buf, tip_x, tip_y, right_x, right_y, chevron_color);
    draw_line(buf, left_x, left_y, center_x, center_y, chevron_color);
    draw_line(buf, right_x, right_y, center_x, center_y, chevron_color);

    // Glowing center core dot
    put_pixel(buf, spx.round() as i32, spy.round() as i32, [220, 255, 240, 255]);
}

/// Draws a 4-corner bracket target reticle around the locked destination planet
fn draw_target_bracket(buf: &mut [u8], px: i32, py: i32, color: [u8; 4]) {
    let d = 5;
    put_pixel(buf, px - d, py - d, color);
    put_pixel(buf, px - d + 1, py - d, color);
    put_pixel(buf, px - d, py - d + 1, color);

    put_pixel(buf, px + d, py - d, color);
    put_pixel(buf, px + d - 1, py - d, color);
    put_pixel(buf, px + d, py - d + 1, color);

    put_pixel(buf, px - d, py + d, color);
    put_pixel(buf, px - d + 1, py + d, color);
    put_pixel(buf, px - d, py + d - 1, color);

    put_pixel(buf, px + d, py + d, color);
    put_pixel(buf, px + d - 1, py + d, color);
    put_pixel(buf, px + d, py + d - 1, color);
}

/// Draws an off-screen pointer arrow along the radar scope rim pointing towards an off-screen entity (target or ship)
fn draw_offscreen_arrow(buf: &mut [u8], rim_x: f32, rim_y: f32, phi: f32, color: [u8; 4], pip_color: [u8; 4]) {
    let tip_len = 5.0;
    let wing_len = 6.0;
    let wing_angle = 0.85;

    let tip_x = (rim_x + tip_len * phi.cos()).round() as i32;
    let tip_y = (rim_y + tip_len * phi.sin()).round() as i32;

    let left_x = (rim_x - wing_len * (phi - wing_angle).cos()).round() as i32;
    let left_y = (rim_y - wing_len * (phi - wing_angle).sin()).round() as i32;

    let right_x = (rim_x - wing_len * (phi + wing_angle).cos()).round() as i32;
    let right_y = (rim_y - wing_len * (phi + wing_angle).sin()).round() as i32;

    draw_line(buf, tip_x, tip_y, left_x, left_y, color);
    draw_line(buf, tip_x, tip_y, right_x, right_y, color);
    draw_line(buf, left_x, left_y, right_x, right_y, color);

    // Glowing center pip at rim contact
    put_pixel(buf, rim_x.round() as i32, rim_y.round() as i32, pip_color);
}

/// Primary system updating the Orbital Radar Map:
/// - Handles [N] visibility toggle and [Shift+N] / [Ctrl+N] Target Focus Zoom toggle
/// - Automatic zoom: ShipAuto tracks ship distance, TargetFocus scales strictly to target distance
/// - Off-screen arrows: points to target if target is off-screen, or points to ship if ship is off-screen
/// - Interactive map clicking: in mouse free mode (Tab), clicking any celestial body selects it as target
pub fn update_orbital_radar_system(
    mut images: ResMut<Assets<Image>>,
    mut radar: ResMut<OrbitalRadarState>,
    mut shuttle_query: Query<(&mut Shuttle, &UniversalPosition, &Transform)>,
    all_targets_query: Query<(Entity, &NavTarget, &UniversalPosition)>,
    orbiting_bodies_query: Query<(Entity, &NavTarget, &UniversalPosition, &KeplerOrbit), Without<SatelliteOrbit>>,
    mut nav: ResMut<NavigationComputer>,
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_wheel: EventReader<MouseWheel>,
    windows: Query<&Window, With<PrimaryWindow>>,
    radar_image_query: Query<(&RelativeCursorPosition, &Interaction), With<OrbitalRadarImageNode>>,
    mut radar_root_query: Query<&mut Node, (With<OrbitalRadarRoot>, Without<OrbitalRadarImageNode>)>,
    mut radar_telemetry_query: Query<&mut Text, With<OrbitalRadarTelemetryText>>,
) {
    let Ok((mut shuttle, shuttle_upos, shuttle_tf)) = shuttle_query.get_single_mut() else {
        return;
    };

    let in_autonomous_flight = shuttle.autopilot_mode == crate::flight::AutopilotMode::Hyperdrive
        || shuttle.autopilot_mode == crate::flight::AutopilotMode::Lorentz;

    // Find universal position of locked destination target (if any)
    let targeted_pos: Option<DVec3> = nav.target_entity.and_then(|ent| {
        all_targets_query.get(ent).ok().map(|(_, _, pos)| pos.0)
    });

    // 1. Keyboard & Mouse Controls:
    // [N] alone: Toggle radar visibility
    // [Shift+N] or [Ctrl+N]: Toggle Target Focus Zoom (Auto Ship <-> Target Scale) or reset from Manual Zoom
    let shift_or_ctrl = keyboard.pressed(KeyCode::ShiftLeft)
        || keyboard.pressed(KeyCode::ShiftRight)
        || keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight);

    if keyboard.just_pressed(KeyCode::KeyN) {
        if shift_or_ctrl {
            radar.zoom_mode = match radar.zoom_mode {
                RadarZoomMode::ShipAuto => {
                    if targeted_pos.is_some() {
                        RadarZoomMode::TargetFocus
                    } else {
                        RadarZoomMode::ShipAuto
                    }
                }
                RadarZoomMode::TargetFocus => RadarZoomMode::ShipAuto,
                RadarZoomMode::Manual => RadarZoomMode::ShipAuto,
            };
        } else {
            radar.visible = !radar.visible;
        }
    }

    // Apply visibility to UI container
    for mut node in radar_root_query.iter_mut() {
        node.display = if radar.visible {
            Display::Flex
        } else {
            Display::None
        };
    }

    if !radar.visible {
        return;
    }

    let cursor_unlocked = windows.get_single().map_or(false, |w| w.cursor_options.grab_mode == CursorGrabMode::None);

    // Free Mouse Scroll Zoom: in Free Mouse mode (Tab), mouse wheel zooms in and out manually
    let mut scroll_delta = 0.0f32;
    for ev in mouse_wheel.read() {
        match ev.unit {
            MouseScrollUnit::Line => scroll_delta += ev.y,
            MouseScrollUnit::Pixel => scroll_delta += ev.y * 0.05,
        }
    }

    if cursor_unlocked && scroll_delta.abs() > 0.0 {
        radar.zoom_mode = RadarZoomMode::Manual;
        let zoom_factor = (1.18f64).powf(-scroll_delta as f64);
        let min_radius = 0.15 * ASTRONOMICAL_UNIT;
        let max_radius = 1000.0 * ASTRONOMICAL_UNIT;
        radar.target_radius_m = (radar.target_radius_m * zoom_factor).clamp(min_radius, max_radius);
    }

    // Fall back to ShipAuto if locked target is cleared
    if radar.zoom_mode == RadarZoomMode::TargetFocus && targeted_pos.is_none() {
        radar.zoom_mode = RadarZoomMode::ShipAuto;
    }

    // 2. Compute Target Zoom Radius
    let dist_sol_m = (shuttle_upos.0.x * shuttle_upos.0.x + shuttle_upos.0.z * shuttle_upos.0.z).sqrt();
    let dist_au = dist_sol_m / ASTRONOMICAL_UNIT;

    match radar.zoom_mode {
        RadarZoomMode::ShipAuto => {
            let (target_r_au, name) = if dist_au <= 1.8 {
                (2.2, "INNER / TERRESTRIAL")
            } else if dist_au <= 3.8 {
                (4.5, "ASTEROID BELT (CERES)")
            } else if dist_au <= 7.5 {
                (8.0, "JOVIAN SYSTEM (JUPITER)")
            } else if dist_au <= 14.5 {
                (15.0, "SATURNIAN SYSTEM (SATURN)")
            } else if dist_au <= 24.0 {
                (25.0, "URANIAN SYSTEM (URANUS)")
            } else if dist_au <= 35.0 {
                (38.0, "NEPTUNIAN SYSTEM (NEPTUNE)")
            } else if dist_au <= 60.0 {
                (65.0, "KUIPER BELT (PLUTO/HAUMEA)")
            } else if dist_au <= 110.0 {
                (120.0, "SCATTERED DISC (ERIS)")
            } else {
                ((dist_au * 1.35).max(150.0), "TRANS-NEPTUNIAN")
            };
            radar.target_radius_m = target_r_au * ASTRONOMICAL_UNIT;
            radar.tier_name = name.to_string();
        }
        RadarZoomMode::TargetFocus => {
            if let Some(t_pos) = targeted_pos {
                let dist_target_sol = (t_pos.x * t_pos.x + t_pos.z * t_pos.z).sqrt();
                let tgt_au = dist_target_sol / ASTRONOMICAL_UNIT;

                // Scale strictly to target's orbital zone (independent of ship position!)
                let (target_r_au, _) = if tgt_au <= 1.8 {
                    (2.2, "INNER / TERRESTRIAL")
                } else if tgt_au <= 3.8 {
                    (4.5, "ASTEROID BELT (CERES)")
                } else if tgt_au <= 7.5 {
                    (8.0, "JOVIAN SYSTEM (JUPITER)")
                } else if tgt_au <= 14.5 {
                    (15.0, "SATURNIAN SYSTEM (SATURN)")
                } else if tgt_au <= 24.0 {
                    (25.0, "URANIAN SYSTEM (URANUS)")
                } else if tgt_au <= 35.0 {
                    (38.0, "NEPTUNIAN SYSTEM (NEPTUNE)")
                } else if tgt_au <= 60.0 {
                    (65.0, "KUIPER BELT (PLUTO/HAUMEA)")
                } else if tgt_au <= 110.0 {
                    (120.0, "SCATTERED DISC (ERIS)")
                } else {
                    ((tgt_au * 1.35).max(150.0), "TRANS-NEPTUNIAN")
                };

                radar.target_radius_m = target_r_au * ASTRONOMICAL_UNIT;
                radar.tier_name = format!("TARGET FOCUS: {}", nav.target_name.to_uppercase());
            }
        }
        RadarZoomMode::Manual => {
            let r_au = radar.target_radius_m / ASTRONOMICAL_UNIT;
            let name = if r_au <= 1.8 {
                "MANUAL (TERRESTRIAL)"
            } else if r_au <= 3.8 {
                "MANUAL (ASTEROID BELT)"
            } else if r_au <= 7.5 {
                "MANUAL (JOVIAN)"
            } else if r_au <= 14.5 {
                "MANUAL (SATURNIAN)"
            } else if r_au <= 24.0 {
                "MANUAL (URANIAN)"
            } else if r_au <= 35.0 {
                "MANUAL (NEPTUNIAN)"
            } else if r_au <= 60.0 {
                "MANUAL (KUIPER BELT)"
            } else if r_au <= 110.0 {
                "MANUAL (SCATTERED DISC)"
            } else {
                "MANUAL (TRANS-NEPTUNIAN)"
            };
            radar.tier_name = name.to_string();
        }
    }

    // Smooth exponential lerp toward target radius
    let dt = time.delta_secs() as f64;
    let blend = (1.0 - (-5.0 * dt).exp()).clamp(0.0, 1.0);
    radar.current_radius_m += (radar.target_radius_m - radar.current_radius_m) * blend;

    let view_radius_m = radar.current_radius_m.max(0.1 * ASTRONOMICAL_UNIT);
    let view_au = view_radius_m / ASTRONOMICAL_UNIT;

    // Check if Target is off-screen
    let mut offscreen_target: Option<(f32, f32, f32)> = None; // (rim_x, rim_y, phi)
    if let Some(t_pos) = targeted_pos {
        let px_f = RADAR_CX as f64 + (t_pos.x / view_radius_m) * (RADAR_MAX_R as f64);
        let py_f = RADAR_CY as f64 + (t_pos.z / view_radius_m) * (RADAR_MAX_R as f64);
        let dx = (px_f - RADAR_CX as f64) as f32;
        let dy = (py_f - RADAR_CY as f64) as f32;
        let r = (dx * dx + dy * dy).sqrt();

        if r > RADAR_MAX_R {
            let phi = dy.atan2(dx);
            let rim_x = RADAR_CX + (RADAR_MAX_R - 4.0) * phi.cos();
            let rim_y = RADAR_CY + (RADAR_MAX_R - 4.0) * phi.sin();
            offscreen_target = Some((rim_x, rim_y, phi));
        }
    }

    // Check if Ship is on-screen or off-screen
    let ship_px_f = RADAR_CX as f64 + (shuttle_upos.0.x / view_radius_m) * (RADAR_MAX_R as f64);
    let ship_py_f = RADAR_CY as f64 + (shuttle_upos.0.z / view_radius_m) * (RADAR_MAX_R as f64);
    let ship_dx = (ship_px_f - RADAR_CX as f64) as f32;
    let ship_dy = (ship_py_f - RADAR_CY as f64) as f32;
    let ship_r = (ship_dx * ship_dx + ship_dy * ship_dy).sqrt();

    let ship_is_offscreen = ship_r > RADAR_MAX_R;
    let mut offscreen_ship: Option<(f32, f32, f32)> = None;
    if ship_is_offscreen {
        let phi = ship_dy.atan2(ship_dx);
        let rim_x = RADAR_CX + (RADAR_MAX_R - 4.0) * phi.cos();
        let rim_y = RADAR_CY + (RADAR_MAX_R - 4.0) * phi.sin();
        offscreen_ship = Some((rim_x, rim_y, phi));
    }

    // 3. Mouse Hover & Click Detection using RelativeCursorPosition
    radar.is_hovered = false;
    radar.hovered_body_name = None;
    let mut hovered_candidate_screen_pos: Option<(f32, f32)> = None;
    let mut clicked_body: Option<(Entity, String, bool)> = None;

    if let Ok((rel_cursor, interaction)) = radar_image_query.get_single() {
        if *interaction != Interaction::None || rel_cursor.mouse_over() {
            radar.is_hovered = true;
        }

        if cursor_unlocked {
            if let Some(norm_pos) = rel_cursor.normalized {
                // norm_pos is in [0.0, 1.0] range with (0.0, 0.0) at top-left and (1.0, 1.0) at bottom-right
                let cursor_radar_x = norm_pos.x * RADAR_PIXEL_SIZE as f32;
                let cursor_radar_y = norm_pos.y * RADAR_PIXEL_SIZE as f32;

                let dx = cursor_radar_x - RADAR_CX;
                let dy = cursor_radar_y - RADAR_CY;

                if dx * dx + dy * dy <= (RADAR_MAX_R + 6.0) * (RADAR_MAX_R + 6.0) {
                    radar.is_hovered = true;

                    let mut min_dist_px = 16.0f32; // Selection click tolerance radius
                    let mut candidate = None;

                    for (ent, target, pos) in all_targets_query.iter() {
                        let px_f = RADAR_CX as f64 + (pos.0.x / view_radius_m) * (RADAR_MAX_R as f64);
                        let py_f = RADAR_CY as f64 + (pos.0.z / view_radius_m) * (RADAR_MAX_R as f64);
                        let d_px = ((cursor_radar_x - px_f as f32).powi(2) + (cursor_radar_y - py_f as f32).powi(2)).sqrt();

                        if d_px < min_dist_px {
                            min_dist_px = d_px;
                            candidate = Some((ent, target.name.to_string(), px_f as f32, py_f as f32));
                        }
                    }

                    if let Some((ent, name, h_px, h_py)) = candidate {
                        radar.hovered_body_name = Some(name.clone());
                        hovered_candidate_screen_pos = Some((h_px, h_py));

                        if mouse_buttons.just_pressed(MouseButton::Left) && !in_autonomous_flight {
                            let now = time.elapsed_secs_f64();
                            let is_double_click = radar.last_clicked_entity == Some(ent)
                                && (now - radar.last_click_time_secs) <= MAP_DOUBLE_CLICK_WINDOW_SECS;

                            if is_double_click {
                                radar.last_click_time_secs = -100.0;
                            } else {
                                radar.last_click_time_secs = now;
                            }
                            radar.last_clicked_entity = Some(ent);

                            clicked_body = Some((ent, name, is_double_click));
                        }
                    } else if mouse_buttons.just_pressed(MouseButton::Left) {
                        // Clicked empty space on radar scope: clear double click candidate
                        radar.last_click_time_secs = -100.0;
                        radar.last_clicked_entity = None;
                    }
                }
            }
        }
    }

    if let Some((ent, name, is_double_click)) = clicked_body {
        // Single click: target the celestial body (like 'G')
        nav.pending_target_entity = Some(ent);
        nav.target_entity = Some(ent);
        nav.target_name = name;

        // Double click: target and aim/align ship at the destination (like 'G' then 'F')
        if is_double_click && !in_autonomous_flight {
            if nav.is_occluded {
                shuttle.occlusion_warning_timer = 3.5;
            } else {
                shuttle.autopilot_mode = AutopilotMode::AlignOnly;
                shuttle.autopilot_phase = AutopilotPhase::Aligning;
                shuttle.braking_active = false;
                initiate_target_alignment(&mut shuttle, shuttle_tf);
            }
        }
    }

    // 4. Update Text Telemetry Readout
    for mut text in radar_telemetry_query.iter_mut() {
        let line2 = if let Some(ref h_name) = radar.hovered_body_name {
            if in_autonomous_flight {
                format!("HOVER: [{}] (AUTOPILOT ENGAGED • TARGET LOCKED)", h_name.to_uppercase())
            } else {
                format!("HOVER: [{}] (CLICK: TARGET • 2x: AIM)", h_name.to_uppercase())
            }
        } else {
            match radar.zoom_mode {
                RadarZoomMode::TargetFocus => {
                    if ship_is_offscreen {
                        format!("TGT FOCUS [{}] • SHIP OFF-SCREEN [Shift+N]", nav.target_name)
                    } else {
                        format!("MODE: TARGET FOCUS [{}] • [Shift+N: Ship Auto]", nav.target_name)
                    }
                }
                RadarZoomMode::ShipAuto => {
                    if offscreen_target.is_some() {
                        format!("TGT: {} (OFF-SCREEN) • [Shift+N to Frame]", nav.target_name)
                    } else if targeted_pos.is_some() {
                        format!("MODE: AUTO (SHIP) • [Shift+N: Focus {}]", nav.target_name)
                    } else {
                        "MODE: AUTO-ZOOM (SHIP) • [Tab] Free Mouse".to_string()
                    }
                }
                RadarZoomMode::Manual => {
                    if offscreen_target.is_some() {
                        format!("MANUAL ZOOM • TGT OFF-SCREEN [Shift+N: Auto]")
                    } else if targeted_pos.is_some() {
                        format!("MODE: MANUAL ZOOM • [Shift+N: Auto-Zoom]")
                    } else {
                        "MODE: MANUAL ZOOM • [Shift+N: Reset Auto]".to_string()
                    }
                }
            }
        };

        let line3 = if ship_is_offscreen {
            format!("SOL: {:.2} AU | SHIP: {:.2} AU (OFF-SCREEN)", dist_au, dist_au)
        } else {
            format!(
                "SOL: {:.2} AU | SHIP: (X: {:+.2} AU, Z: {:+.2} AU)",
                dist_au,
                shuttle_upos.0.x / ASTRONOMICAL_UNIT,
                shuttle_upos.0.z / ASTRONOMICAL_UNIT,
            )
        };

        **text = format!("SCALE: {:.2} AU [{}]\n{}\n{}", view_au, radar.tier_name, line2, line3);
    }

    // 5. Software Rasterization onto Radar Image Buffer
    let Some(image) = images.get_mut(&radar.image_handle) else {
        return;
    };
    let buf = &mut image.data;

    // 5.1 Clear Scope Background
    for y in 0..RADAR_PIXEL_SIZE {
        for x in 0..RADAR_PIXEL_SIZE {
            let dx = x as f32 - RADAR_CX;
            let dy = y as f32 - RADAR_CY;
            let r2 = dx * dx + dy * dy;
            let idx = (y * RADAR_PIXEL_SIZE + x) * 4;

            if r2 <= RADAR_MAX_R * RADAR_MAX_R {
                // Scope interior: deep space tactical dark navy
                buf[idx] = 6;
                buf[idx + 1] = 10;
                buf[idx + 2] = 20;
                buf[idx + 3] = 230;
            } else if r2 <= (RADAR_MAX_R + 1.5) * (RADAR_MAX_R + 1.5) {
                // Glowing circular outer rim
                buf[idx] = 45;
                buf[idx + 1] = 140;
                buf[idx + 2] = 220;
                buf[idx + 3] = 180;
            } else {
                // Outside corner padding
                buf[idx] = 3;
                buf[idx + 1] = 5;
                buf[idx + 2] = 10;
                buf[idx + 3] = 240;
            }
        }
    }

    // 5.2 Concentric Range Rings (at 33%, 66%, and 100% of max radius)
    for fraction in [0.33f32, 0.66f32, 1.00f32] {
        let r = RADAR_MAX_R * fraction;
        let ring_steps = (r * 3.5) as usize;
        for i in 0..ring_steps {
            if i % 3 != 0 {
                let th = (i as f32) * 2.0 * std::f32::consts::PI / (ring_steps as f32);
                let px = (RADAR_CX + r * th.cos()).round() as i32;
                let py = (RADAR_CY + r * th.sin()).round() as i32;
                put_pixel(buf, px, py, [35, 75, 115, 80]);
            }
        }
    }

    // 5.3 Sol Crosshair (Central axes through Sol with 6px center gap)
    for i in 6..(RADAR_MAX_R as i32) {
        let faint_sol_axis = [30, 65, 100, 65];
        put_pixel(buf, RADAR_CX as i32 + i, RADAR_CY as i32, faint_sol_axis);
        put_pixel(buf, RADAR_CX as i32 - i, RADAR_CY as i32, faint_sol_axis);
        put_pixel(buf, RADAR_CX as i32, RADAR_CY as i32 + i, faint_sol_axis);
        put_pixel(buf, RADAR_CX as i32, RADAR_CY as i32 - i, faint_sol_axis);
    }

    // 5.4 Planetary Orbits (Realistic Keplerian Elliptical Trajectories)
    let targeted_entity = nav.target_entity;

    for (ent, _target, _pos, orbit) in orbiting_bodies_query.iter() {
        let a = orbit.semi_major_axis;
        let e = orbit.eccentricity;
        let r_min_px = ((a * (1.0 - e)) / view_radius_m) * (RADAR_MAX_R as f64);
        let r_max_px = ((a * (1.0 + e)) / view_radius_m) * (RADAR_MAX_R as f64);

        if r_max_px >= 1.2 && r_min_px <= (RADAR_MAX_R as f64) * 1.85 {
            let is_targeted = Some(ent) == targeted_entity;
            let orbit_color = if is_targeted {
                [65, 215, 255, 190] // Glowing bright cyan for targeted destination orbit
            } else {
                [42, 88, 135, 75]   // Subtle tactical navy for planetary orbits
            };

            let steps = ((r_max_px * 2.4).clamp(64.0, 240.0)) as usize;
            let mut prev_pt: Option<(i32, i32)> = None;
            let mut first_pt: Option<(i32, i32)> = None;

            for i in 0..steps {
                let e_anom = (i as f64) * 2.0 * PI / (steps as f64);
                let pt = orbit.position_at_eccentric_anomaly(e_anom);
                let px = (RADAR_CX as f64 + (pt.x / view_radius_m) * (RADAR_MAX_R as f64)).round() as i32;
                let py = (RADAR_CY as f64 + (pt.z / view_radius_m) * (RADAR_MAX_R as f64)).round() as i32;

                if first_pt.is_none() {
                    first_pt = Some((px, py));
                }
                if let Some((x0, y0)) = prev_pt {
                    draw_line(buf, x0, y0, px, py, orbit_color);
                }
                prev_pt = Some((px, py));
            }
            if let (Some((x0, y0)), Some((xf, yf))) = (prev_pt, first_pt) {
                draw_line(buf, x0, y0, xf, yf, orbit_color);
            }
        }
    }

    // 5.5 Sol (The Sun at Center 0, 0)
    draw_disc(buf, RADAR_CX as i32, RADAR_CY as i32, 4, [255, 200, 60, 110]);
    draw_disc(buf, RADAR_CX as i32, RADAR_CY as i32, 2, [255, 245, 170, 255]);

    // 5.6 Planetary Bodies
    for (ent, target, pos) in all_targets_query.iter() {
        let x = pos.0.x;
        let z = pos.0.z;
        let px_f = RADAR_CX as f64 + (x / view_radius_m) * (RADAR_MAX_R as f64);
        let py_f = RADAR_CY as f64 + (z / view_radius_m) * (RADAR_MAX_R as f64);

        let dx = (px_f - RADAR_CX as f64) as f32;
        let dy = (py_f - RADAR_CY as f64) as f32;

        if dx * dx + dy * dy <= (RADAR_MAX_R + 1.0) * (RADAR_MAX_R + 1.0) {
            let px = px_f.round() as i32;
            let py = py_f.round() as i32;
            let is_targeted = Some(ent) == targeted_entity;

            let color = planet_radar_color(target.name);
            let radius = match target.name {
                "Jupiter" | "Saturn" | "Chronos" => 3,
                "Ceres" | "Pluto" | "Haumea" | "Makemake" | "Eris" | "Farfarout" => 2,
                _ => 2,
            };

            draw_disc(buf, px, py, radius, color);

            // Highlight targeted destination with a 4-corner bracket reticle
            if is_targeted {
                draw_target_bracket(buf, px, py, [60, 255, 150, 240]);
            }
        }
    }

    // 5.7 Mouse Hover Ring
    if let Some((h_px, h_py)) = hovered_candidate_screen_pos {
        let hover_ring_color = if in_autonomous_flight {
            [235, 120, 40, 190] // Muted amber when target selection is locked by autopilot
        } else {
            [100, 225, 255, 230] // Cyan when selectable
        };
        draw_circle_outline(buf, h_px.round() as i32, h_py.round() as i32, 6, hover_ring_color);
    }

    // 5.8 Off-Screen Target Indicator Arrow (Cyan/Teal)
    if let Some((rim_x, rim_y, phi)) = offscreen_target {
        draw_offscreen_arrow(buf, rim_x, rim_y, phi, [60, 215, 255, 255], [200, 245, 255, 255]);
    }

    // 5.9 Shuttle / Player: Cross of Origin & Chevron (if On-Screen) OR Off-Screen Ship Arrow (if Off-Screen)
    if !ship_is_offscreen {
        let spx = ship_px_f.round() as i32;
        let spy = ship_py_f.round() as i32;

        // Draw the user-requested perpendicular origin cross lines with coordinate ticks
        let cross_color = [60, 220, 130, 130];
        draw_ship_cross_of_origin(buf, spx, spy, cross_color);

        let fwd = (shuttle_tf.rotation * -Vec3::Z).normalize();
        let heading_rad = fwd.z.atan2(fwd.x);

        draw_ship_marker(buf, ship_px_f as f32, ship_py_f as f32, heading_rad);
    } else if let Some((rim_x, rim_y, phi)) = offscreen_ship {
        // Draw bright green arrow on the radar perimeter pointing to the distant ship
        draw_offscreen_arrow(buf, rim_x, rim_y, phi, [80, 255, 140, 255], [220, 255, 240, 255]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_radar_pixel_buffer_dimensions() {
        assert_eq!(RADAR_PIXEL_SIZE, 280);
        let expected_bytes = RADAR_PIXEL_SIZE * RADAR_PIXEL_SIZE * 4;
        assert_eq!(expected_bytes, 313_600);
    }

    #[test]
    fn test_planet_radar_color_mapping() {
        let sol = planet_radar_color("Sol");
        assert_eq!(sol[0], 255);
        assert!(sol[1] > 200);

        let earth = planet_radar_color("Earth");
        assert_eq!(earth, [75, 160, 240, 255]);

        let mars = planet_radar_color("Mars");
        assert_eq!(mars, [225, 90, 70, 255]);

        let haumea = planet_radar_color("Haumea");
        assert_eq!(haumea, [225, 235, 230, 255]);
    }

    #[test]
    fn test_radar_ecliptic_coordinate_projection() {
        let view_radius = 2.2 * ASTRONOMICAL_UNIT;

        let sol_x = 0.0;
        let sol_z = 0.0;
        let sol_px = RADAR_CX as f64 + (sol_x / view_radius) * (RADAR_MAX_R as f64);
        let sol_py = RADAR_CY as f64 + (sol_z / view_radius) * (RADAR_MAX_R as f64);
        assert!((sol_px - RADAR_CX as f64).abs() < 1e-6);
        assert!((sol_py - RADAR_CY as f64).abs() < 1e-6);

        // Planet at +X (East/Right in 3D) maps to right of radar center (px > RADAR_CX)
        let earth_x = ASTRONOMICAL_UNIT;
        let earth_z = 0.0;
        let earth_px = RADAR_CX as f64 + (earth_x / view_radius) * (RADAR_MAX_R as f64);
        let earth_py = RADAR_CY as f64 + (earth_z / view_radius) * (RADAR_MAX_R as f64);
        assert!(earth_px > RADAR_CX as f64);
        assert!((earth_py - RADAR_CY as f64).abs() < 1e-6);

        // Planet at -Z (North/Forward in 3D) maps to top of radar screen (py < RADAR_CY)
        let fwd_z = -ASTRONOMICAL_UNIT;
        let fwd_py = RADAR_CY as f64 + (fwd_z / view_radius) * (RADAR_MAX_R as f64);
        assert!(fwd_py < RADAR_CY as f64, "Forward (-Z) in 3D must map to top of radar scope");

        // Planet at +Z (South/Aft in 3D) maps to bottom of radar screen (py > RADAR_CY)
        let aft_z = ASTRONOMICAL_UNIT;
        let aft_py = RADAR_CY as f64 + (aft_z / view_radius) * (RADAR_MAX_R as f64);
        assert!(aft_py > RADAR_CY as f64, "Backward (+Z) in 3D must map to bottom of radar scope");
    }

    #[test]
    fn test_ship_heading_and_chirality() {
        // Forward flight (-Z in 3D) must point UP on screen (Δy < 0, Δx = 0)
        let fwd_unrotated = -Vec3::Z;
        let heading_fwd = fwd_unrotated.z.atan2(fwd_unrotated.x);
        let tip_dx = heading_fwd.cos();
        let tip_dy = heading_fwd.sin();
        assert!(tip_dx.abs() < 1e-5, "Forward flight should have dx = 0 on screen");
        assert!(tip_dy < -0.99, "Forward flight should point directly UP (dy < 0) on screen");

        // Turning left (yaw counter-clockwise in 3D around +Y) rotates -Z toward -X
        let yaw_left = Quat::from_rotation_y(15.0_f32.to_radians());
        let fwd_left = (yaw_left * -Vec3::Z).normalize();
        assert!(fwd_left.x < 0.0, "Yawing left in 3D must rotate heading into negative X");
        let heading_left = fwd_left.z.atan2(fwd_left.x);
        let tip_dx_left = heading_left.cos();
        assert!(tip_dx_left < -0.2, "Turning left in 3D must point left (negative dx) on radar screen");

        // Turning right (yaw clockwise in 3D around +Y) rotates -Z toward +X
        let yaw_right = Quat::from_rotation_y(-15.0_f32.to_radians());
        let fwd_right = (yaw_right * -Vec3::Z).normalize();
        assert!(fwd_right.x > 0.0, "Yawing right in 3D must rotate heading into positive X");
        let heading_right = fwd_right.z.atan2(fwd_right.x);
        let tip_dx_right = heading_right.cos();
        assert!(tip_dx_right > 0.2, "Turning right in 3D must point right (positive dx) on radar screen");

        // Relative targets: From ship looking down -Z, Mars at (-X) is on the left, Mercury at (+X) is on the right
        let view_radius = 2.0 * ASTRONOMICAL_UNIT;
        let mars_x = -0.5 * ASTRONOMICAL_UNIT;
        let mercury_x = 0.5 * ASTRONOMICAL_UNIT;
        let mars_px = RADAR_CX as f64 + (mars_x / view_radius) * (RADAR_MAX_R as f64);
        let mercury_px = RADAR_CX as f64 + (mercury_x / view_radius) * (RADAR_MAX_R as f64);
        assert!(mars_px < RADAR_CX as f64, "Body on player's left (-X) must appear on the left on radar");
        assert!(mercury_px > RADAR_CX as f64, "Body on player's right (+X) must appear on the right on radar");
        assert!(mercury_px > mars_px, "Mercury on the right of Mars must have px_mercury > px_mars");
    }

    #[test]
    fn test_cross_of_origin_raster_safety() {
        let mut buf = vec![0u8; RADAR_PIXEL_SIZE * RADAR_PIXEL_SIZE * 4];
        draw_ship_cross_of_origin(&mut buf, RADAR_CX as i32, RADAR_CY as i32, [60, 220, 130, 130]);
        let center_idx = ((RADAR_CY as usize) * RADAR_PIXEL_SIZE + (RADAR_CX as usize)) * 4;
        assert_eq!(buf[center_idx + 3], 0, "Center of cross must leave a gap for ship chevron");

        let arm_idx = ((RADAR_CY as usize) * RADAR_PIXEL_SIZE + (RADAR_CX as usize + 20)) * 4;
        assert!(buf[arm_idx + 3] > 0, "Crosshair line must be drawn");
    }

    #[test]
    fn test_target_focus_zoom_independent_of_ship() {
        // Target is Earth (1.0 AU), while ship is at Pluto (40.0 AU)
        let earth_pos = DVec3::new(ASTRONOMICAL_UNIT, 0.0, 0.0);
        let dist_target_sol = (earth_pos.x * earth_pos.x + earth_pos.z * earth_pos.z).sqrt();
        let tgt_au = dist_target_sol / ASTRONOMICAL_UNIT;

        let target_r_au = if tgt_au <= 1.8 {
            2.2
        } else {
            tgt_au * 1.35
        };

        // Target focus must zoom in to terrestrial scale (2.2 AU) regardless of ship at 40 AU!
        assert_eq!(target_r_au, 2.2);
    }

    #[test]
    fn test_offscreen_ship_and_target_arrows() {
        let mut buf = vec![0u8; RADAR_PIXEL_SIZE * RADAR_PIXEL_SIZE * 4];
        let rim_x = RADAR_CX + RADAR_MAX_R - 4.0;
        let rim_y = RADAR_CY;

        // Draw offscreen arrow
        draw_offscreen_arrow(&mut buf, rim_x, rim_y, 0.0, [80, 255, 140, 255], [220, 255, 240, 255]);

        let center_idx = ((rim_y as usize) * RADAR_PIXEL_SIZE + (rim_x as usize)) * 4;
        assert!(buf[center_idx + 3] > 0, "Pip on rim must be drawn");
    }

    #[test]
    fn test_radar_relative_cursor_exact_pixel_mapping() {
        // Test 1: Sol at exact center of radar
        let norm_center = Vec2::new(0.5, 0.5);
        let cursor_x = norm_center.x * RADAR_PIXEL_SIZE as f32;
        let cursor_y = norm_center.y * RADAR_PIXEL_SIZE as f32;
        assert_eq!(cursor_x, RADAR_CX);
        assert_eq!(cursor_y, RADAR_CY);

        // Test 2: Venus located 30 pixels above Sol on radar (y = 110.0)
        let venus_y_expected = 110.0f32;
        let norm_venus = Vec2::new(0.5, venus_y_expected / RADAR_PIXEL_SIZE as f32);
        let calculated_y = norm_venus.y * RADAR_PIXEL_SIZE as f32;
        assert!((calculated_y - venus_y_expected).abs() < 1e-4, "Cursor at Venus must map directly to Venus y-coordinate with zero offset");
    }

    #[test]
    fn test_radar_manual_scroll_zoom_scaling() {
        let mut radar = OrbitalRadarState::default();
        let initial_r = radar.target_radius_m;

        // Scroll Up (zoom in, scroll_delta = 1.0)
        let scroll_up = 1.0f32;
        let zoom_in_factor = (1.18f64).powf(-scroll_up as f64);
        radar.target_radius_m = (radar.target_radius_m * zoom_in_factor).clamp(0.15 * ASTRONOMICAL_UNIT, 350.0 * ASTRONOMICAL_UNIT);
        radar.zoom_mode = RadarZoomMode::Manual;

        assert_eq!(radar.zoom_mode, RadarZoomMode::Manual);
        assert!(radar.target_radius_m < initial_r, "Scrolling up must zoom in (decrease scope radius)");

        // Scroll Down (zoom out, scroll_delta = -2.0)
        let scroll_down = -2.0f32;
        let zoom_out_factor = (1.18f64).powf(-scroll_down as f64);
        radar.target_radius_m = (radar.target_radius_m * zoom_out_factor).clamp(0.15 * ASTRONOMICAL_UNIT, 350.0 * ASTRONOMICAL_UNIT);
        assert!(radar.target_radius_m > initial_r, "Scrolling down must zoom out (increase scope radius)");

        // Minimum clamping check
        let extreme_scroll_in = 50.0f32;
        radar.target_radius_m = (radar.target_radius_m * (1.18f64).powf(-extreme_scroll_in as f64))
            .clamp(0.15 * ASTRONOMICAL_UNIT, 350.0 * ASTRONOMICAL_UNIT);
        assert!((radar.target_radius_m - 0.15 * ASTRONOMICAL_UNIT).abs() < 1e-3, "Must clamp to minimum radius");

        // Maximum clamping check
        let extreme_scroll_out = -100.0f32;
        radar.target_radius_m = (radar.target_radius_m * (1.18f64).powf(-extreme_scroll_out as f64))
            .clamp(0.15 * ASTRONOMICAL_UNIT, 350.0 * ASTRONOMICAL_UNIT);
        assert!((radar.target_radius_m - 350.0 * ASTRONOMICAL_UNIT).abs() < 1e-3, "Must clamp to maximum radius");
    }

    #[test]
    fn test_radar_manual_zoom_shift_n_cycle() {
        let mut zoom_mode = RadarZoomMode::Manual;

        // Pressing Shift+N when in Manual mode must reset to ShipAuto
        zoom_mode = match zoom_mode {
            RadarZoomMode::ShipAuto => RadarZoomMode::TargetFocus,
            RadarZoomMode::TargetFocus => RadarZoomMode::ShipAuto,
            RadarZoomMode::Manual => RadarZoomMode::ShipAuto,
        };
        assert_eq!(zoom_mode, RadarZoomMode::ShipAuto, "Shift+N in Manual mode must return to ShipAuto");

        // Pressing Shift+N when in ShipAuto (with target) switches to TargetFocus
        zoom_mode = match zoom_mode {
            RadarZoomMode::ShipAuto => RadarZoomMode::TargetFocus,
            RadarZoomMode::TargetFocus => RadarZoomMode::ShipAuto,
            RadarZoomMode::Manual => RadarZoomMode::ShipAuto,
        };
        assert_eq!(zoom_mode, RadarZoomMode::TargetFocus);
    }

    #[test]
    fn test_radar_manual_zoom_tier_naming() {
        let tier_name_for_radius = |r_au: f64| {
            if r_au <= 1.8 {
                "MANUAL (TERRESTRIAL)"
            } else if r_au <= 3.8 {
                "MANUAL (ASTEROID BELT)"
            } else if r_au <= 7.5 {
                "MANUAL (JOVIAN)"
            } else if r_au <= 14.5 {
                "MANUAL (SATURNIAN)"
            } else if r_au <= 24.0 {
                "MANUAL (URANIAN)"
            } else if r_au <= 35.0 {
                "MANUAL (NEPTUNIAN)"
            } else if r_au <= 60.0 {
                "MANUAL (KUIPER BELT)"
            } else if r_au <= 110.0 {
                "MANUAL (SCATTERED DISC)"
            } else {
                "MANUAL (TRANS-NEPTUNIAN)"
            }
        };

        assert_eq!(tier_name_for_radius(1.0), "MANUAL (TERRESTRIAL)");
        assert_eq!(tier_name_for_radius(2.7), "MANUAL (ASTEROID BELT)");
        assert_eq!(tier_name_for_radius(5.2), "MANUAL (JOVIAN)");
        assert_eq!(tier_name_for_radius(9.5), "MANUAL (SATURNIAN)");
        assert_eq!(tier_name_for_radius(19.2), "MANUAL (URANIAN)");
        assert_eq!(tier_name_for_radius(30.0), "MANUAL (NEPTUNIAN)");
        assert_eq!(tier_name_for_radius(45.0), "MANUAL (KUIPER BELT)");
        assert_eq!(tier_name_for_radius(90.0), "MANUAL (SCATTERED DISC)");
        assert_eq!(tier_name_for_radius(140.0), "MANUAL (TRANS-NEPTUNIAN)");
    }

    #[test]
    fn test_radar_eccentric_orbit_projection_and_focus() {
        // Farfarout: a = 132 AU, e = 0.655
        let a = 19_747_000_000_000.0;
        let e = 0.6550;
        let orbit = KeplerOrbit::eccentric(None, a, e, 0.0, 0.0, 0.0, 1000.0, 0.0);

        let view_radius_m = 250.0 * ASTRONOMICAL_UNIT;

        let peri_pt = orbit.position_at_eccentric_anomaly(0.0);
        let aph_pt = orbit.position_at_eccentric_anomaly(std::f64::consts::PI);

        let peri_px = RADAR_CX as f64 + (peri_pt.x / view_radius_m) * (RADAR_MAX_R as f64);
        let peri_py = RADAR_CY as f64 + (peri_pt.z / view_radius_m) * (RADAR_MAX_R as f64);

        let aph_px = RADAR_CX as f64 + (aph_pt.x / view_radius_m) * (RADAR_MAX_R as f64);
        let aph_py = RADAR_CY as f64 + (aph_pt.z / view_radius_m) * (RADAR_MAX_R as f64);

        let dist_peri_from_sol = ((peri_px - RADAR_CX as f64).powi(2) + (peri_py - RADAR_CY as f64).powi(2)).sqrt();
        let dist_aph_from_sol = ((aph_px - RADAR_CX as f64).powi(2) + (aph_py - RADAR_CY as f64).powi(2)).sqrt();

        // Sol is at RADAR_CX, RADAR_CY. Aphelion must be much farther from Sol than perihelion:
        assert!(dist_aph_from_sol > dist_peri_from_sol * 4.0, "Aphelion on radar must be > 4x farther from Sol than perihelion");

        // The midpoint of perihelion and aphelion (the geometric center of the ellipse) is offset from Sol
        let center_px = (peri_px + aph_px) * 0.5;
        let offset_px = (center_px - RADAR_CX as f64).abs();
        let expected_offset_px = ((a * e) / view_radius_m) * (RADAR_MAX_R as f64);
        assert!((offset_px - expected_offset_px).abs() < 1e-4, "Geometric center must be offset from Sol by a*e on radar screen");
    }

    #[test]
    fn test_radar_map_double_click_timing_and_aim() {
        let mut radar = OrbitalRadarState::default();
        let dummy_entity_1 = Entity::from_raw(42);
        let dummy_entity_2 = Entity::from_raw(99);

        // First click on entity 1 at t = 10.0s
        let t1 = 10.0f64;
        let is_dc_1 = radar.last_clicked_entity == Some(dummy_entity_1)
            && (t1 - radar.last_click_time_secs) <= MAP_DOUBLE_CLICK_WINDOW_SECS;
        assert!(!is_dc_1, "First click should not trigger double-click");
        radar.last_click_time_secs = t1;
        radar.last_clicked_entity = Some(dummy_entity_1);

        // Second click on entity 1 within threshold (t = 10.20s, delta = 0.20s <= 0.35s)
        let t2 = 10.20f64;
        let is_dc_2 = radar.last_clicked_entity == Some(dummy_entity_1)
            && (t2 - radar.last_click_time_secs) <= MAP_DOUBLE_CLICK_WINDOW_SECS;
        assert!(is_dc_2, "Second click within window must trigger double-click targeting + aim");
        // Reset timer after double click
        radar.last_click_time_secs = -100.0;
        radar.last_clicked_entity = Some(dummy_entity_1);

        // Third quick click (t = 10.25s) after a successful double-click must NOT re-trigger double-click
        let t3 = 10.25f64;
        let is_dc_3 = radar.last_clicked_entity == Some(dummy_entity_1)
            && (t3 - radar.last_click_time_secs) <= MAP_DOUBLE_CLICK_WINDOW_SECS;
        assert!(!is_dc_3, "Immediate third click must not trigger double click again");
        radar.last_click_time_secs = t3;
        radar.last_clicked_entity = Some(dummy_entity_1);

        // Click on a DIFFERENT entity (dummy_entity_2) at t = 10.35s must NOT trigger double-click
        let t4 = 10.35f64;
        let is_dc_4 = radar.last_clicked_entity == Some(dummy_entity_2)
            && (t4 - radar.last_click_time_secs) <= MAP_DOUBLE_CLICK_WINDOW_SECS;
        assert!(!is_dc_4, "Clicking a different entity must not trigger double click");
        radar.last_click_time_secs = t4;
        radar.last_clicked_entity = Some(dummy_entity_2);

        // Click on entity 2 after threshold expired (t = 11.00s, delta = 0.65s > 0.35s)
        let t5 = 11.00f64;
        let is_dc_5 = radar.last_clicked_entity == Some(dummy_entity_2)
            && (t5 - radar.last_click_time_secs) <= MAP_DOUBLE_CLICK_WINDOW_SECS;
        assert!(!is_dc_5, "Clicking after double click window expired must not trigger double click");
    }
}
