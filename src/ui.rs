use bevy::asset::{LoadState, UntypedAssetId};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use crate::camera::{CameraViewMode, SpaceCamera};
use crate::coords::UniversalPosition;
use crate::flight::{FpsTracker, Shuttle, SimulationPause};
use crate::relativity::{RelativisticConfig, RelativisticState, REAL_C};

#[derive(Component)]
pub struct TelemetryText;

#[derive(Component)]
pub struct FlightModeText;

#[derive(Component)]
pub struct SpacetimeClocksText;

#[derive(Component)]
pub struct NavTelemetryText;

/// UI Marker for the rolling target lock display text
#[derive(Component)]
pub struct NavTargetLockNameText;

/// Button component to cycle to previous navigation target
#[derive(Component)]
pub struct TargetPrevButton;

/// Button component to cycle to next navigation target
#[derive(Component)]
pub struct TargetNextButton;

#[derive(Component)]
pub struct FlightCrosshairUi;

#[derive(Component)]
pub struct CrosshairHoverText;

#[derive(Component)]
pub struct StartupCreditsToast {
    pub time_remaining: f32,
}

/// An individual asset being tracked for startup loading completion
#[derive(Clone, Debug)]
pub struct TrackedLoadingAsset {
    pub label: String,
    pub id: UntypedAssetId,
}

/// Global resource tracking startup asset loading, glTF instantiation, and the cinematic loading screen
#[derive(Resource)]
pub struct StartupLoadingTracker {
    pub tracked_assets: Vec<TrackedLoadingAsset>,
    pub is_loaded: bool,
    pub warm_up_frames: u32,
    pub opacity: f32,
    pub elapsed: f32,
    pub min_display_time: f32,
}

impl Default for StartupLoadingTracker {
    fn default() -> Self {
        Self {
            tracked_assets: Vec::new(),
            is_loaded: false,
            warm_up_frames: 0,
            opacity: 1.0,
            elapsed: 0.0,
            min_display_time: 0.40,
        }
    }
}

#[derive(Component)]
pub struct LoadingScreenRoot;

#[derive(Component)]
pub struct LoadingScreenPanel;

#[derive(Component)]
pub struct LoadingProgressBarFill;

#[derive(Component)]
pub struct LoadingStatusText;

#[derive(Component)]
pub struct LoadingPercentageText;

#[derive(Component)]
pub struct LoadingScreenText;

/// Dynamic fill bar for the Beta (v/c) velocity tape
#[derive(Component)]
pub struct BetaTapeFill;

/// Numerical label displaying live velocity beta and light-speed percentage
#[derive(Component)]
pub struct BetaTapeText;

/// Logarithmic progression fill bar for the Lorentz gamma factor
#[derive(Component)]
pub struct GammaTapeFill;

/// Cockpit visual alert badge for exponential time dilation thresholds
#[derive(Component)]
pub struct GammaAlertBadge;

/// Text inside the Lorentz gamma alert badge
#[derive(Component)]
pub struct GammaAlertText;

/// Aperture fill bar for the relativistic aberration searchlight cone
#[derive(Component)]
pub struct AberrationConeFill;

/// Text readout for the forward relativistic aberration cone opening angle
#[derive(Component)]
pub struct AberrationConeText;

/// Text status readout for the active Canopy Visor / Polarized Solar Filter
#[derive(Component)]
pub struct VisorStatusText;

/// Fullscreen Screen-Space Visor Tint Overlay
#[derive(Component)]
pub struct VisorScreenOverlay;

/// Active HUD display mode: Full telemetry, Minimal essential info, or completely Hidden
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
pub enum HudDisplayMode {
    /// Complete scientific telemetry panels, gauges, orbital radar, clocks, and controls bar
    #[default]
    Full,
    /// Compact, unobtrusive flight HUD showing only essential telemetry (speed, β/γ, altitude, target, clocks)
    Minimal,
    /// Pure game view — 100% of HUD panels, reticles, crosshair, and bars hidden
    Hidden,
}

impl HudDisplayMode {
    pub fn label(&self) -> &'static str {
        match self {
            HudDisplayMode::Full => "FULL",
            HudDisplayMode::Minimal => "MINIMAL",
            HudDisplayMode::Hidden => "HIDDEN",
        }
    }
}

/// Resource managing the active HUD display mode (Hotkeys: I to cycle, Shift+I to toggle Hidden)
#[derive(Resource, Debug, Clone)]
pub struct HudDisplayState {
    pub mode: HudDisplayMode,
    /// Remembers the last visible mode (Full or Minimal) so Shift+I restores it cleanly
    pub last_visible_mode: HudDisplayMode,
}

impl Default for HudDisplayState {
    fn default() -> Self {
        Self {
            mode: HudDisplayMode::Full,
            last_visible_mode: HudDisplayMode::Full,
        }
    }
}

impl HudDisplayState {
    /// Cycles Full -> Minimal -> Hidden -> Full
    pub fn cycle(&mut self) {
        self.mode = match self.mode {
            HudDisplayMode::Full => {
                self.last_visible_mode = HudDisplayMode::Minimal;
                HudDisplayMode::Minimal
            }
            HudDisplayMode::Minimal => {
                self.last_visible_mode = HudDisplayMode::Minimal;
                HudDisplayMode::Hidden
            }
            HudDisplayMode::Hidden => {
                self.last_visible_mode = HudDisplayMode::Full;
                HudDisplayMode::Full
            }
        };
    }

    /// Instantly toggles between Hidden and the last active visible mode (Full or Minimal)
    pub fn toggle_hidden(&mut self) {
        if self.mode == HudDisplayMode::Hidden {
            self.mode = match self.last_visible_mode {
                HudDisplayMode::Hidden => HudDisplayMode::Full,
                other => other,
            };
        } else {
            self.last_visible_mode = self.mode;
            self.mode = HudDisplayMode::Hidden;
        }
    }
}

/// Marker for HUD containers shown only in Full HUD mode (top full panels & bottom controls bar)
#[derive(Component)]
pub struct FullHudElement;

/// Root container for the compact Minimal HUD top bar
#[derive(Component)]
pub struct MinimalHudRoot;

/// Top-left compact kinematics text in Minimal HUD mode
#[derive(Component)]
pub struct MinimalKinematicsText;

/// Top-center compact target lock container in Minimal HUD mode
#[derive(Component)]
pub struct MinimalTargetBox;

/// Top-center compact target lock text in Minimal HUD mode
#[derive(Component)]
pub struct MinimalTargetText;

/// Top-right compact clocks & FPS text in Minimal HUD mode
#[derive(Component)]
pub struct MinimalMetricsText;

/// Resource managing visibility of the Cockpit MFD Settings & Help Overlay (Hotkey: F9)
#[derive(Resource, Debug, Clone, Default)]
pub struct SettingsOverlayState {
    pub visible: bool,
    pub just_closed: bool,
}

/// Root backdrop container for the Settings & Keybindings MFD Overlay
#[derive(Component)]
pub struct SettingsOverlayRoot;

/// Live simulation telemetry text entity inside the MFD Settings Overlay
#[derive(Component)]
pub struct SettingsOverlayTelemetryText;

/// Button entity in the Settings Overlay to switch universes
#[derive(Component)]
pub struct UniverseSwitchButton;

/// Button text entity in the Settings Overlay for universe mode
#[derive(Component)]
pub struct UniverseSwitchButtonText;

pub fn setup_ui(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
) {
    let font_sans = asset_server.load("fonts/DejaVuSans.ttf");
    let font_mono = asset_server.load("fonts/DejaVuSansMono.ttf");

    // Fullscreen Screen-Space Visor Tint Overlay (sits behind HUD text)
    commands.spawn((
        VisorScreenOverlay,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
        ZIndex(-1),
    ));

    // Cockpit MFD Settings & Help Overlay Modal (Toggled by F9)
    commands.spawn((
        SettingsOverlayRoot,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            display: Display::None, // Hidden by default
            ..default()
        },
        BackgroundColor(Color::srgba(0.005, 0.012, 0.025, 0.78)),
        ZIndex(50),
    )).with_children(|backdrop| {
        backdrop.spawn((
            Node {
                width: Val::Px(940.0),
                height: Val::Px(670.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(16.0)),
                row_gap: Val::Px(10.0),
                border: UiRect::all(Val::Px(1.5)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.03, 0.07, 0.96)),
            BorderColor(Color::srgba(0.3, 0.8, 1.0, 0.75)),
        )).with_children(|panel| {
            // Header Bar
            panel.spawn((
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::bottom(Val::Px(6.0)),
                    border: UiRect::bottom(Val::Px(1.0)),
                    ..default()
                },
                BorderColor(Color::srgba(0.2, 0.6, 1.0, 0.40)),
            )).with_children(|header| {
                header.spawn((
                    Text::new("COCKPIT MULTI-FUNCTION DISPLAY (MFD) // FLIGHT MANUAL & DIAGNOSTICS"),
                    TextFont {
                        font: font_sans.clone(),
                        font_size: 13.5,
                        ..default()
                    },
                    TextColor(Color::srgb(0.35, 0.90, 1.0)),
                ));

                header.spawn((
                    Text::new("[PRESS F9 OR ESC TO CLOSE]"),
                    TextFont {
                        font: font_mono.clone(),
                        font_size: 11.5,
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.75, 0.2)),
                ));
            });

            // Two-Column Content Area
            panel.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(16.0),
                    ..default()
                },
            )).with_children(|row| {
                // Left Column: Live Simulation Telemetry & States
                row.spawn((
                    Node {
                        width: Val::Percent(50.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        padding: UiRect::all(Val::Px(10.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.01, 0.02, 0.05, 0.70)),
                    BorderColor(Color::srgba(0.2, 0.5, 0.8, 0.35)),
                )).with_children(|col| {
                    col.spawn((
                        Text::new("► LIVE SIMULATION TELEMETRY & STATES"),
                        TextFont {
                            font: font_sans.clone(),
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.4, 0.85, 1.0)),
                    ));

                    col.spawn((
                        SettingsOverlayTelemetryText,
                        Text::new("INITIALIZING MFD TELEMETRY..."),
                        TextFont {
                            font: font_mono.clone(),
                            font_size: 10.5,
                            ..default()
                        },
                        TextColor(Color::srgb(0.85, 0.92, 1.0)),
                    ));
                });

                // Right Column: Flight Controls & Commands Reference Manual
                row.spawn((
                    Node {
                        width: Val::Percent(50.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        padding: UiRect::all(Val::Px(10.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.01, 0.02, 0.05, 0.70)),
                    BorderColor(Color::srgba(0.2, 0.5, 0.8, 0.35)),
                )).with_children(|col| {
                    col.spawn((
                        Text::new("► FLIGHT CONTROLS & COMMAND MANUAL"),
                        TextFont {
                            font: font_sans.clone(),
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.80, 0.3)),
                    ));

                    col.spawn((
                        Text::new(
                            "FLIGHT & ATTITUDE:\n  [↑ / ↓] / Mouse         Pitch Down / Up\n  [← / →] / Mouse         Yaw Port / Starboard\n  [A / E]                 Roll Port / Starboard\n  [Z / Left-Click]        Main Plasma Thrusters\n  [S / Right-Click]       Reverse Retro-Thrusters\n  [Q / D]                 Lateral Strafe Port/Starboard\n  [Shift]                 Afterburner Boost Thruster\n  [X]                     Emergency Retro-Brake\n  [W]                     Toggle SAS Attitude Assist\n  [R / Num .]             Reset Ship to Runway\n\nTARGETING, AUTOPILOT & RADAR:\n  [T / Y]                 Cycle Target / Lock Nearest\n  [G / MMB]               Point & Select Target (Shift+G Clear)\n  [F]                     Auto-Aim Target Align\n  [H]                     Sci-Fi Warp Autopilot (Instant Drop)\n  [L]                     Lorentz Autopilot (Midcourse Flip)\n  [N]                     Toggle Orbital Radar Map (Shift+N Mode)\n\nOPTICS, HUD, TIME & SYSTEM:\n  [I / Shift+I]           Cycle HUD (Full/Minimal/Hide) / Hide UI\n  [U / F10]               Switch Universe (Demo ↔ 1:1 Solar System)\n  [K]                     Canopy Visor (Clear/Pol/H-α)\n  [M]                     Mute\n  [F1 / Return]           Pause\n  [F2]                    VSync\n  [Alt / F3]              Free-Look\n  [Num 8/2/4/6/5]         Numpad Look (8/2/4/6, 5 Center)\n  [F4 / F5]               Toggle Aberration / Doppler\n  [F6 / F7 / F8]          Slow-Mo / Time-Warp / Reset\n  [F9]                    Info and Controls Menu\n  [F11]                   Toggle Fullscreen (Borderless / Windowed)\n  [F12 / Shift+F12]       Texture Profile (8K/4K/2K) / Toggle Dynamic Streaming\n  [O / P]                 Adjust Light Constant c\n  [V] • [Scroll]          Camera Mode • Zoom\n  [Tab]                   Mouse Locking"
                        ),
                        TextFont {
                            font: font_mono.clone(),
                            font_size: 10.5,
                            ..default()
                        },
                        TextColor(Color::srgb(0.85, 0.90, 0.95)),
                    ));
                });
            });

            // Footer Status Line & Universe Switcher
            panel.spawn((
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::top(Val::Px(4.0)),
                    ..default()
                },
            )).with_children(|footer| {
                footer.spawn((
                    Text::new("STATUS: ALL SYSTEMS NOMINAL • SPACETIME SENSORS ONLINE"),
                    TextFont {
                        font: font_mono.clone(),
                        font_size: 10.5,
                        ..default()
                    },
                    TextColor(Color::srgb(0.3, 0.8, 0.5)),
                ));

                footer.spawn((
                    Button,
                    UniverseSwitchButton,
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BorderColor(Color::srgb(0.3, 0.8, 1.0)),
                    BackgroundColor(Color::srgba(0.05, 0.15, 0.25, 0.85)),
                )).with_children(|btn| {
                    btn.spawn((
                        UniverseSwitchButtonText,
                        Text::new("► SWITCH UNIVERSE [KEY U / F10]"),
                        TextFont {
                            font: font_mono.clone(),
                            font_size: 10.5,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.85, 0.2)),
                    ));
                });

                footer.spawn((
                    Text::new("SPACE EXPLORER // AVIONICS SUITE V1.0"),
                    TextFont {
                        font: font_mono.clone(),
                        font_size: 10.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.5, 0.7, 0.9, 0.6)),
                ));
            });
        });
    });

    // Initialize Orbital Radar texture buffer and state
    let radar_state = crate::radar::init_orbital_radar(&mut images);
    let radar_image_handle = radar_state.image_handle.clone();
    commands.insert_resource(radar_state);

    // Root full-screen UI container
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::SpaceBetween,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(24.0)),
            ..default()
        })
        .with_children(|root| {
            // Compact Minimal HUD Top Bar (Displayed only in HudDisplayMode::Minimal)
            root.spawn((
                MinimalHudRoot,
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexStart,
                    display: Display::None,
                    ..default()
                },
            ))
            .with_children(|min_bar| {
                // Top-Left Compact Kinematics Pill (Line 1: SPD/β/γ, Line 2: ALT & status tags)
                min_bar
                    .spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(7.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            min_width: Val::Px(310.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.02, 0.04, 0.08, 0.72)),
                        BorderColor(Color::srgba(0.2, 0.6, 1.0, 0.45)),
                    ))
                    .with_children(|pill| {
                        pill.spawn((
                            MinimalKinematicsText,
                            Text::new("SPD: 0.0 m/s (0.00% c)  •  γ: 1.00×"),
                            TextFont {
                                font: font_mono.clone(),
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.88, 0.95, 1.0)),
                        ));
                    });

                // Top-Center Compact Target Lock Pill (strictly centered, immune to left/right width changes)
                min_bar
                    .spawn(Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        right: Val::Px(0.0),
                        top: Val::Px(0.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::FlexStart,
                        ..default()
                    })
                    .with_children(|center_wrap| {
                        center_wrap
                            .spawn((
                                MinimalTargetBox,
                                Node {
                                    padding: UiRect::axes(Val::Px(12.0), Val::Px(7.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.02, 0.04, 0.08, 0.72)),
                                BorderColor(Color::srgba(0.2, 0.8, 0.6, 0.45)),
                            ))
                            .with_children(|pill| {
                                pill.spawn((
                                    MinimalTargetText,
                                    Text::new("LOCK: [NO TARGET]"),
                                    TextFont {
                                        font: font_mono.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.4, 1.0, 0.8)),
                                ));
                            });
                    });

                // Top-Right Compact Clocks & FPS Pill
                min_bar
                    .spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(7.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.02, 0.04, 0.08, 0.72)),
                        BorderColor(Color::srgba(0.2, 0.6, 1.0, 0.45)),
                    ))
                    .with_children(|pill| {
                        pill.spawn((
                            MinimalMetricsText,
                            Text::new("τ: 00:00  |  t: 00:00  •  60 FPS  •  [I] HUD"),
                            TextFont {
                                font: font_mono.clone(),
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.85, 0.92, 1.0)),
                        ));
                    });
            });

            // Top HUD Bar (Left: Speed/Beta/Gamma/Coords, Right: Dual Clocks/Settings/Orientation)
            root.spawn((
                FullHudElement,
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexStart,
                    ..default()
                },
            ))
            .with_children(|top_bar| {
                // Left Column: Relativistic Avionics Panel + Solar System Orbital Radar Map
                top_bar
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(10.0),
                        align_items: AlignItems::FlexStart,
                        ..default()
                    })
                    .with_children(|left_col| {
                        // Top Left Relativistic Telemetry Panel
                        left_col
                            .spawn((
                                Node {
                                    flex_direction: FlexDirection::Column,
                                    padding: UiRect::all(Val::Px(12.0)),
                                    row_gap: Val::Px(5.0),
                                    border: UiRect::all(Val::Px(1.0)),
                                    min_width: Val::Px(460.0),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.02, 0.04, 0.08, 0.85)),
                                BorderColor(Color::srgba(0.2, 0.6, 1.0, 0.5)),
                            ))
                            .with_children(|panel| {
                        panel.spawn((
                            Text::new("RELATIVISTIC AVIONICS & KINEMATICS"),
                            TextFont {
                                font: font_sans.clone(),
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.4, 0.8, 1.0)),
                        ));

                        // 1. Graphical Beta Velocity Tape
                        panel.spawn((
                            Node {
                                flex_direction: FlexDirection::Column,
                                width: Val::Percent(100.0),
                                row_gap: Val::Px(2.0),
                                margin: UiRect::vertical(Val::Px(2.0)),
                                ..default()
                            },
                        )).with_children(|gauge| {
                            gauge.spawn((
                                BetaTapeText,
                                Text::new("VELOCITY / c: β = 0.000000 (0.00% c)"),
                                TextFont {
                                    font: font_mono.clone(),
                                    font_size: 11.5,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.3, 0.85, 1.0)),
                            ));

                            // Outer Bar Track
                            gauge.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Px(10.0),
                                    border: UiRect::all(Val::Px(1.0)),
                                    overflow: Overflow::clip(),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.02, 0.05, 0.12, 0.95)),
                                BorderColor(Color::srgba(0.2, 0.5, 0.8, 0.6)),
                            )).with_children(|track| {
                                track.spawn((
                                    BetaTapeFill,
                                    Node {
                                        width: Val::Percent(0.0),
                                        height: Val::Percent(100.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.2, 0.75, 1.0)),
                                ));
                            });

                            // Scale Ticks (evenly spaced flex row across 100% width)
                            gauge.spawn(Node {
                                width: Val::Percent(100.0),
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                margin: UiRect::top(Val::Px(2.0)),
                                ..default()
                            }).with_children(|ticks| {
                                for label in ["0c", "0.25c", "0.50c", "0.75c", "1.0c"] {
                                    ticks.spawn((
                                        Text::new(label),
                                        TextFont {
                                            font: font_mono.clone(),
                                            font_size: 9.5,
                                            ..default()
                                        },
                                        TextColor(Color::srgba(0.5, 0.7, 0.9, 0.6)),
                                    ));
                                }
                            });
                        });

                        // 2. High-Precision Numerical Telemetry
                        panel.spawn((
                            TelemetryText,
                            Text::new("SPEED:     0.000 m/s\nCOORDS:    X: 0.0 m | Y: 0.0 m | Z: 0.0 m\nTHRUST:    IDLE\nACCEL:     0.0 m/s² (0.0 g) [INERTIAL]"),
                            TextFont {
                                font: font_mono.clone(),
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));

                        // 3. Lorentz Gamma Meter & Visual Dilation Alert Row
                        panel.spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                width: Val::Percent(100.0),
                                column_gap: Val::Px(8.0),
                                margin: UiRect::top(Val::Px(2.0)),
                                ..default()
                            },
                        )).with_children(|row| {
                            // Left: Gamma meter track
                            row.spawn((
                                Node {
                                    flex_direction: FlexDirection::Column,
                                    flex_grow: 1.0,
                                    row_gap: Val::Px(2.0),
                                    ..default()
                                },
                            )).with_children(|g_col| {
                                g_col.spawn((
                                    Text::new("LORENTZ γ PROGRESSION:"),
                                    TextFont {
                                        font: font_mono.clone(),
                                        font_size: 9.5,
                                        ..default()
                                    },
                                    TextColor(Color::srgba(0.5, 0.7, 0.9, 0.6)),
                                ));
                                g_col.spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        height: Val::Px(7.0),
                                        border: UiRect::all(Val::Px(1.0)),
                                        overflow: Overflow::clip(),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(0.02, 0.05, 0.12, 0.95)),
                                    BorderColor(Color::srgba(0.2, 0.5, 0.8, 0.5)),
                                )).with_children(|track| {
                                    track.spawn((
                                        GammaTapeFill,
                                        Node {
                                            width: Val::Percent(0.0),
                                            height: Val::Percent(100.0),
                                            ..default()
                                        },
                                        BackgroundColor(Color::srgb(0.25, 0.85, 0.45)),
                                    ));
                                });
                            });

                            // Right: Alert Badge
                            row.spawn((
                                GammaAlertBadge,
                                Node {
                                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    min_width: Val::Px(160.0),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.02, 0.08, 0.04, 0.85)),
                                BorderColor(Color::srgba(0.2, 0.8, 0.4, 0.6)),
                            )).with_children(|badge| {
                                badge.spawn((
                                    GammaAlertText,
                                    Text::new("γ: 1.000× [REST]"),
                                    TextFont {
                                        font: font_mono.clone(),
                                        font_size: 11.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.3, 0.9, 0.5)),
                                ));
                            });
                        });

                        panel.spawn((
                            FlightModeText,
                            Text::new("ROTATIONAL SAS: [ACTIVE (W to toggle)]"),
                            TextFont {
                                font: font_sans.clone(),
                                font_size: 11.5,
                                ..default()
                            },
                            TextColor(Color::srgb(0.2, 1.0, 0.4)),
                        ));

                        panel.spawn((
                            VisorStatusText,
                            Text::new("CANOPY VISOR: [CLEAR 100% (K to cycle)]"),
                            TextFont {
                                font: font_sans.clone(),
                                font_size: 11.5,
                                ..default()
                            },
                            TextColor(Color::srgb(0.3, 0.85, 1.0)),
                        ));
                    });

                // Solar System Orbital Radar Map (Docked in left column, flows dynamically below Avionics)
                crate::radar::setup_radar_ui(
                    &radar_image_handle,
                    &font_sans,
                    &font_mono,
                    left_col,
                );
            });

                // Top Center Targeting & Navigation Computer Panel
                top_bar
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Px(10.0)),
                            row_gap: Val::Px(4.0),
                            border: UiRect::all(Val::Px(1.0)),
                            width: Val::Px(520.0),
                            min_width: Val::Px(520.0),
                            max_width: Val::Px(520.0),
                            align_items: AlignItems::Center,
                            align_self: AlignSelf::FlexStart,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.02, 0.04, 0.08, 0.85)),
                        BorderColor(Color::srgba(0.2, 0.8, 0.6, 0.45)),
                    ))
                    .with_children(|panel| {
                        panel.spawn((
                            Text::new("TARGETING & NAVIGATION COMPUTER"),
                            TextFont {
                                font: font_sans.clone(),
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.3, 1.0, 0.6)),
                        ));

                        // Interactive Rolling Target Header: [ < ] LOCK: Target [Class] [ > ]
                        panel.spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::SpaceBetween,
                                column_gap: Val::Px(6.0),
                                width: Val::Percent(100.0),
                                margin: UiRect::vertical(Val::Px(1.0)),
                                ..default()
                            },
                        ))
                        .with_children(|row| {
                            // Previous Target Button
                            row.spawn((
                                Button,
                                TargetPrevButton,
                                Node {
                                    width: Val::Px(24.0),
                                    height: Val::Px(18.0),
                                    flex_shrink: 0.0,
                                    border: UiRect::all(Val::Px(1.0)),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.04, 0.12, 0.20, 0.70)),
                                BorderColor(Color::srgb(0.2, 0.6, 0.8)),
                                BorderRadius::all(Val::Px(3.0)),
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("<"),
                                    TextFont {
                                        font: font_mono.clone(),
                                        font_size: 11.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.4, 1.0, 0.8)),
                                ));
                            });

                            // Target Name and Classification Label (Centered between buttons)
                            row.spawn((
                                NavTargetLockNameText,
                                Text::new("LOCK:  [NO TARGET]"),
                                TextFont {
                                    font: font_mono.clone(),
                                    font_size: 12.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.4, 1.0, 0.8)),
                                TextLayout::new_with_justify(JustifyText::Center),
                                Node {
                                    flex_grow: 1.0,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                            ));

                            // Next Target Button
                            row.spawn((
                                Button,
                                TargetNextButton,
                                Node {
                                    width: Val::Px(24.0),
                                    height: Val::Px(18.0),
                                    flex_shrink: 0.0,
                                    border: UiRect::all(Val::Px(1.0)),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.04, 0.12, 0.20, 0.70)),
                                BorderColor(Color::srgb(0.2, 0.6, 0.8)),
                                BorderRadius::all(Val::Px(3.0)),
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new(">"),
                                    TextFont {
                                        font: font_mono.clone(),
                                        font_size: 11.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.4, 1.0, 0.8)),
                                ));
                            });
                        });

                        panel.spawn((
                            NavTelemetryText,
                            Text::new("RANGE: N/A\nRADIAL: N/A\n[T Cycle • Y Nearest • F Aim • H Warp • L Lorentz • G Clear]"),
                            TextFont {
                                font: font_mono.clone(),
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.85, 0.95, 1.0)),
                        ));
                    });

                // Top Right Spacetime Clocks & Settings Panel
                top_bar
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Px(12.0)),
                            row_gap: Val::Px(5.0),
                            border: UiRect::all(Val::Px(1.0)),
                            min_width: Val::Px(430.0),
                            align_items: AlignItems::FlexEnd,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.02, 0.04, 0.08, 0.85)),
                        BorderColor(Color::srgba(0.2, 0.6, 1.0, 0.5)),
                    ))
                    .with_children(|panel| {
                        panel.spawn((
                            Text::new("SPACETIME METRICS & RELATIVISTIC CONE"),
                            TextFont {
                                font: font_sans.clone(),
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.4, 0.8, 1.0)),
                        ));

                        panel.spawn((
                            SpacetimeClocksText,
                            Text::new("SHIP CLOCK (τ):      00:00.00\nUNIVERSE CLOCK (t):  00:00.00\nTIME DILATION:       1.000 : 1\nSPEED OF LIGHT (c):  5,000 m/s [DEMO]\nPITCH: +0° | YAW: +0° | ROLL: +0°"),
                            TextFont {
                                font: font_mono.clone(),
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.85, 0.9, 1.0)),
                        ));

                        // Relativistic Aberration Searchlight Cone Meter
                        panel.spawn((
                            Node {
                                flex_direction: FlexDirection::Column,
                                width: Val::Percent(100.0),
                                row_gap: Val::Px(2.0),
                                margin: UiRect::top(Val::Px(3.0)),
                                align_items: AlignItems::FlexEnd,
                                ..default()
                            },
                        )).with_children(|cone_box| {
                            cone_box.spawn((
                                AberrationConeText,
                                Text::new("ABERRATION: 180.0° [REST FRAME - WIDE VIEW]"),
                                TextFont {
                                    font: font_mono.clone(),
                                    font_size: 11.5,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.3, 0.85, 1.0)),
                            ));

                            // Graphical searchlight cone aperture track (centers from outside in)
                            cone_box.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Px(8.0),
                                    border: UiRect::all(Val::Px(1.0)),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    overflow: Overflow::clip(),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.02, 0.05, 0.12, 0.95)),
                                BorderColor(Color::srgba(0.2, 0.6, 1.0, 0.5)),
                            )).with_children(|track| {
                                track.spawn((
                                    AberrationConeFill,
                                    Node {
                                        width: Val::Percent(100.0),
                                        height: Val::Percent(100.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.2, 0.75, 1.0)),
                                ));
                            });

                            cone_box.spawn((
                                Text::new("FORWARD APERTURE: 180° (WIDE) ───► 10° (SEARCHLIGHT)"),
                                TextFont {
                                    font: font_mono.clone(),
                                    font_size: 9.5,
                                    ..default()
                                },
                                TextColor(Color::srgba(0.4, 0.7, 0.9, 0.6)),
                            ));
                        });
                    });
            });

            // Center Reticle / Crosshair with Hover Object Label
            root.spawn((
                FlightCrosshairUi,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Percent(50.0),
                    margin: UiRect {
                        left: Val::Px(-12.0),
                        top: Val::Px(-12.0),
                        ..default()
                    },
                    width: Val::Px(24.0),
                    height: Val::Px(24.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .with_children(|center| {
                center.spawn((
                    Text::new("+"),
                    TextFont {
                        font: font_sans.clone(),
                        font_size: 26.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.3, 0.85, 1.0, 0.75)),
                ));

                center.spawn((
                    CrosshairHoverText,
                    Text::new(""),
                    TextFont {
                        font: font_mono.clone(),
                        font_size: 11.5,
                        ..default()
                    },
                    TextLayout::new_with_justify(JustifyText::Center),
                    TextColor(Color::srgb(0.25, 1.0, 0.7)),
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(28.0),
                        left: Val::Px(-128.0), // Center 280px wide container under 24px crosshair
                        width: Val::Px(280.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                ));
            });

            // Startup Credits Toast (displays soundtrack and ScienceClic inspiration for 12 seconds)
            root.spawn((
                StartupCreditsToast { time_remaining: 12.0 },
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(56.0),
                    align_self: AlignSelf::Center,
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(7.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.01, 0.03, 0.07, 0.90)),
                BorderColor(Color::srgba(0.25, 0.75, 1.0, 0.5)),
            ))
            .with_children(|toast| {
                toast.spawn((
                    Text::new(
                        "♫ Soundtrack: \"ScienceClic (Musique mystérieuse)\" by Alessandro Roussel • Inspired by ScienceClic • Press [M] to Mute",
                    ),
                    TextFont {
                        font: font_sans.clone(),
                        font_size: 11.5,
                        ..default()
                    },
                    TextColor(Color::srgb(0.35, 0.88, 1.0)),
                ));
            });

            // Bottom Controls Bar with AZERTY Keymapping, Cruise Locks, and Performance shortcuts
            root.spawn((
                FullHudElement,
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(10.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.04, 0.08, 0.85)),
                BorderColor(Color::srgba(0.2, 0.6, 1.0, 0.4)),
            ))
            .with_children(|bar| {
                bar.spawn((
                    Text::new(
                        "[Z/L-Click] Thrust • [S/R-Click] Retro • [T/Y/G] Target • [F] Aim • [H] Warp AP • [L] Lorentz AP • [N] Radar • [I] HUD • [Shift+I] Hide UI • [K] Visor • [M] Mute • [W] SAS • [R] Reset • [U] Universe • [F1] Pause • [F4] Aberr • [F5] Doppler • [F6-F8] Time • [F9] Menu • [F11] Fullscreen • [F12] Quality",
                    ),
                    TextFont {
                        font: font_sans.clone(),
                        font_size: 11.5,
                        ..default()
                    },
                    TextColor(Color::srgb(0.7, 0.85, 1.0)),
                ));
            });
        });
}

pub fn update_ui_system(
    shuttle_query: Query<(&Shuttle, &RelativisticState, &UniversalPosition, &Transform)>,
    camera_query: Query<&SpaceCamera>,
    config: Res<RelativisticConfig>,
    optics: Res<crate::relativity::RelativisticOpticsConfig>,
    universe_config: Res<crate::celestial::UniverseConfig>,
    pause: Res<SimulationPause>,
    fps_tracker: Res<FpsTracker>,
    nav: Res<crate::navigation::NavigationComputer>,
    windows: Query<&Window, With<PrimaryWindow>>,
    monitors: Query<&bevy::window::Monitor, With<bevy::window::PrimaryMonitor>>,
    mut telemetry_query: Query<&mut Text, (With<TelemetryText>, Without<FlightModeText>, Without<SpacetimeClocksText>, Without<NavTelemetryText>, Without<NavTargetLockNameText>, Without<CrosshairHoverText>, Without<BetaTapeText>, Without<GammaAlertText>, Without<AberrationConeText>, Without<VisorStatusText>, Without<SettingsOverlayTelemetryText>, Without<UniverseSwitchButtonText>)>,
    mut flight_mode_query: Query<(&mut Text, &mut TextColor), (With<FlightModeText>, Without<TelemetryText>, Without<SpacetimeClocksText>, Without<NavTelemetryText>, Without<NavTargetLockNameText>, Without<CrosshairHoverText>, Without<BetaTapeText>, Without<GammaAlertText>, Without<AberrationConeText>, Without<VisorStatusText>, Without<SettingsOverlayTelemetryText>, Without<UniverseSwitchButtonText>)>,
    mut clocks_query: Query<&mut Text, (With<SpacetimeClocksText>, Without<TelemetryText>, Without<FlightModeText>, Without<NavTelemetryText>, Without<NavTargetLockNameText>, Without<CrosshairHoverText>, Without<BetaTapeText>, Without<GammaAlertText>, Without<AberrationConeText>, Without<VisorStatusText>, Without<SettingsOverlayTelemetryText>, Without<UniverseSwitchButtonText>)>,
    mut nav_query: Query<&mut Text, (With<NavTelemetryText>, Without<TelemetryText>, Without<FlightModeText>, Without<SpacetimeClocksText>, Without<NavTargetLockNameText>, Without<CrosshairHoverText>, Without<BetaTapeText>, Without<GammaAlertText>, Without<AberrationConeText>, Without<VisorStatusText>, Without<SettingsOverlayTelemetryText>, Without<UniverseSwitchButtonText>)>,
    mut nav_lock_query: Query<&mut Text, (With<NavTargetLockNameText>, Without<TelemetryText>, Without<FlightModeText>, Without<SpacetimeClocksText>, Without<NavTelemetryText>, Without<CrosshairHoverText>, Without<BetaTapeText>, Without<GammaAlertText>, Without<AberrationConeText>, Without<VisorStatusText>, Without<SettingsOverlayTelemetryText>, Without<UniverseSwitchButtonText>)>,
) {
    let Ok((shuttle, rel_state, upos, transform)) = shuttle_query.get_single() else {
        return;
    };
    let Ok(camera) = camera_query.get_single() else {
        return;
    };
    let Ok(window) = windows.get_single() else {
        return;
    };

    let is_cursor_locked = window.cursor_options.grab_mode == CursorGrabMode::Locked;
    let speed = rel_state.velocity.length();

    // 1. Update High-Precision Relativistic Telemetry Text
    for mut text in telemetry_query.iter_mut() {
        let thrust_state = if pause.0 {
            "SIMULATION PAUSED"
        } else if shuttle.landed_body.is_some() {
            "TOUCHDOWN - LANDED ON SURFACE [Space to Lift Off]"
        } else if shuttle.braking_active {
            if shuttle.brake_to_universal {
                "SOLAR RETRO BRAKE [Shift+X]"
            } else {
                "TARGET RETRO BRAKE [X]"
            }
        } else if shuttle.locked_thrust_dir > 0.0 {
            if shuttle.locked_boost {
                "AUTOPILOT: LOCKED IMPULSE BOOST [Ctrl+Shift+Z/L-Click] (Tap to cancel)"
            } else {
                "AUTOPILOT: LOCKED FORWARD CRUISE [Ctrl+Z/L-Click] (Tap to cancel)"
            }
        } else if shuttle.locked_thrust_dir < 0.0 {
            if shuttle.locked_boost {
                "AUTOPILOT: LOCKED REVERSE IMPULSE [Ctrl+Shift+S/R-Click] (Tap to cancel)"
            } else {
                "AUTOPILOT: LOCKED REVERSE CRUISE [Ctrl+S/R-Click] (Tap to cancel)"
            }
        } else if shuttle.boost_active && (shuttle.thruster_forward != 0.0 || shuttle.thruster_strafe.abs() > 0.0 || shuttle.thruster_vertical.abs() > 0.0) {
            if config.is_demo_mode {
                "AFTERBURNER [BOOST]"
            } else {
                "RELATIVISTIC IMPULSE DRIVE"
            }
        } else if shuttle.thruster_forward > 0.0 {
            "MAIN BURN [Z / L-Click]"
        } else if shuttle.thruster_forward < 0.0 {
            "RETRO BURN [S / R-Click]"
        } else if shuttle.thruster_strafe.abs() > 0.0 || shuttle.thruster_vertical.abs() > 0.0 {
            "RCS MANEUVERING"
        } else if rel_state.beta > 0.00001 {
            "RELATIVISTIC COASTING"
        } else {
            "STATIONARY AT REST"
        };

        // Format coordinates
        let (x_str, y_str, z_str) = if upos.0.length() >= 149_597_870_700.0 * 0.1 {
            let au = 149_597_870_700.0;
            (
                format!("{:>+7.3} AU", upos.0.x / au),
                format!("{:>+7.3} AU", upos.0.y / au),
                format!("{:>+7.3} AU", upos.0.z / au),
            )
        } else if upos.0.length() >= 10_000_000.0 {
            (
                format!("{:>+7.1}M km", upos.0.x / 1_000_000_000.0),
                format!("{:>+7.1}M km", upos.0.y / 1_000_000_000.0),
                format!("{:>+7.1}M km", upos.0.z / 1_000_000_000.0),
            )
        } else if upos.0.length() > 10000.0 {
            (
                format!("{:>+8.2} km", upos.0.x / 1000.0),
                format!("{:>+8.2} km", upos.0.y / 1000.0),
                format!("{:>+8.2} km", upos.0.z / 1000.0),
            )
        } else {
            (
                format!("{:>+8.1} m", upos.0.x),
                format!("{:>+8.1} m", upos.0.y),
                format!("{:>+8.1} m", upos.0.z),
            )
        };

        // High precision speed string (3 decimals when close to c, 2 decimals otherwise)
        let delta_c = (config.c - speed).max(0.0);
        let speed_formatted = if delta_c < 10.0 {
            format_number_with_commas_3(speed)
        } else {
            format_number_with_commas_2(speed)
        };

        // Delta to c readout
        let delta_str = if delta_c < 0.1 {
            format!("Δ to c:   -{:.4} m/s", delta_c)
        } else if delta_c < 10.0 {
            format!("Δ to c:   -{:.3} m/s", delta_c)
        } else if delta_c < 10_000.0 {
            format!("Δ to c:   -{:.1} m/s", delta_c)
        } else {
            format!("Δ to c:   -{} m/s", format_number_with_commas_2(delta_c))
        };

        // Speed line with km/s if in real mode
        let speed_line = if config.c >= 1_000_000.0 {
            format!("SPEED:    {} m/s\n          ({:.5} km/s)", speed_formatted, speed / 1000.0)
        } else {
            format!("SPEED:    {} m/s", speed_formatted)
        };

        let rel_speed_line = {
            let mut line = String::new();
            if nav.target_entity.is_some() {
                let rel_v_formatted = format_number_with_commas_2(nav.relative_speed);
                if nav.relative_speed >= 1_000.0 {
                    line.push_str(&format!("\nTGT REL:   {} m/s ({:.2} km/s)", rel_v_formatted, nav.relative_speed / 1_000.0));
                } else {
                    line.push_str(&format!("\nTGT REL:   {} m/s", rel_v_formatted));
                }
            }

            // Dual / Closest Object Altitude Meter logic:
            if shuttle.landed_body.is_some() {
                line.push_str("\nALT (SURF): 0 m [TOUCHDOWN]");
            } else if nav.target_entity.is_some() && nav.closest_body_entity == nav.target_entity {
                // Target is also the closest body: show single unified altitude
                if let Some(alt) = nav.altitude_m {
                    if alt < 1.0 {
                        line.push_str("\nALT (SURF): 0 m [TOUCHDOWN]");
                    } else {
                        line.push_str(&format!("\nALT (SURF): {}", crate::navigation::format_nav_distance(alt)));
                    }
                }
            } else if let (Some(ref c_name), Some(c_alt)) = (&nav.closest_body_name, nav.closest_altitude_m) {
                // Closest body is different from target (e.g. targeting Sun while orbiting Venus):
                if let Some(tgt_alt) = nav.altitude_m {
                    line.push_str(&format!("\nALT (TGT):  {}", crate::navigation::format_nav_distance(tgt_alt)));
                }
                if c_alt < 1.0 {
                    line.push_str(&format!("\nALT ({}): 0 m [TOUCHDOWN]", c_name.to_uppercase()));
                } else {
                    line.push_str(&format!("\nALT ({}): {}", c_name.to_uppercase(), crate::navigation::format_nav_distance(c_alt)));
                }
            } else if let Some(alt) = nav.altitude_m {
                if alt < 1.0 {
                    line.push_str("\nALT (SURF): 0 m [TOUCHDOWN]");
                } else {
                    line.push_str(&format!("\nALT (SURF): {}", crate::navigation::format_nav_distance(alt)));
                }
            }
            line
        };

        let ang_deg_s = shuttle.smoothed_angular_speed_deg;
        let ang_rot_s = ang_deg_s / 360.0;
        let ang_str = if pause.0 {
            "0.0°/s [FROZEN]".to_string()
        } else {
            format!("{:>5.1}°/s ({:.2} rot/s)", ang_deg_s, ang_rot_s)
        };

        let accel_str = if pause.0 {
            "0.0 m/s² (0.0 g) [FROZEN]".to_string()
        } else if shuttle.current_proper_accel <= 0.0 {
            "0.0 m/s² (0.0 g) [INERTIAL]".to_string()
        } else {
            let a = shuttle.current_proper_accel;
            let g = a / 9.80665;
            if a >= 1_000_000.0 {
                format!("{:.1}M m/s² ({:.2}M g)", a / 1_000_000.0, g / 1_000_000.0)
            } else if a >= 1_000.0 {
                format!("{:.1} km/s² ({:.1} g)", a / 1_000.0, g)
            } else {
                format!("{:.1} m/s² ({:.1} g)", a, g)
            }
        };

        let grav_mag = shuttle.current_grav_accel;
        let grav_g = grav_mag / 9.80665;
        let grav_str = if grav_mag >= 1_000_000.0 {
            format!("{:.1}M m/s² ({:.2}M g)", grav_mag / 1_000_000.0, grav_g / 1_000_000.0)
        } else if grav_mag >= 1_000.0 {
            format!("{:.1} km/s² ({:.1} g)", grav_mag / 1_000.0, grav_g)
        } else if grav_mag >= 0.01 {
            format!("{:.2} m/s² ({:.2} g)", grav_mag, grav_g)
        } else {
            "0.00 m/s² [FREE-FALL]".to_string()
        };

        let sector_label = format!("{} [U/F10]", universe_config.active_mode.label());

        **text = format!(
            "SECTOR:    {}\n{}{}\n{}\nANGULAR:   {}\nCOORDS:    X: {} | Y: {} | Z: {}\nTHRUST:    {}\nACCEL:     {}\nGRAVITY:   {}",
            sector_label,
            speed_line,
            rel_speed_line,
            delta_str,
            ang_str,
            x_str, y_str, z_str,
            thrust_state,
            accel_str,
            grav_str
        );
    }

    // 2. Update Flight Assist / Pause Text
    for (mut text, mut color) in flight_mode_query.iter_mut() {
        if pause.0 {
            **text = "SIMULATION: [PAUSED - Move Mouse to Orbit Camera • F1 to Resume]".to_string();
            *color = TextColor(Color::srgb(1.0, 0.85, 0.1));
        } else if shuttle.occlusion_warning_timer > 0.0 {
            let obs_name = nav.occluded_by_name.as_deref().unwrap_or("OBSTACLE");
            **text = format!("AUTOPILOT INHIBITED: LINE OF SIGHT BLOCKED BY [{}]! CLEAR OBSTACLE TO ENGAGE", obs_name.to_uppercase());
            *color = TextColor(Color::srgb(1.0, 0.40, 0.1)); // Bright Warning Orange
        } else if shuttle.autopilot_mode == crate::flight::AutopilotMode::Hyperdrive {
            let phase_str = match shuttle.autopilot_phase {
                crate::flight::AutopilotPhase::PreBraking => "PRE-STOPPING (X-BRAKE)",
                crate::flight::AutopilotPhase::Aligning => "ALIGNING TO VECTOR",
                crate::flight::AutopilotPhase::Accelerating => "WARP PROPULSION (BOOST)",
                crate::flight::AutopilotPhase::Cruising => "RELATIVISTIC CRUISE (0.99c)",
                crate::flight::AutopilotPhase::EmergencyBraking => "BRAKING (WARP DROP)",
                crate::flight::AutopilotPhase::TerminalApproach => "FINAL APPROACH",
                crate::flight::AutopilotPhase::Arrived => "ARRIVED (ORBIT INSERTED)",
                _ => "CRUISE",
            };
            **text = format!("AUTOPILOT [WARP/H]: {} [H to disengage]", phase_str);
            *color = TextColor(Color::srgb(0.92, 0.45, 1.0)); // Violet
        } else if shuttle.autopilot_mode == crate::flight::AutopilotMode::Lorentz {
            let phase_str = match shuttle.autopilot_phase {
                crate::flight::AutopilotPhase::PreBraking => "PRE-STOPPING (X-BRAKE)",
                crate::flight::AutopilotPhase::Aligning => "ALIGNING TO TARGET",
                crate::flight::AutopilotPhase::Accelerating => "PROPULSION ACCEL",
                crate::flight::AutopilotPhase::Flipping => "MIDCOURSE FLIP (180°)",
                crate::flight::AutopilotPhase::Decelerating => {
                    if shuttle.braking_active || shuttle.boost_active {
                        "RETRO-BRAKING BURN"
                    } else {
                        "RETRO-COASTING"
                    }
                }
                crate::flight::AutopilotPhase::TerminalApproach => "FINAL APPROACH",
                crate::flight::AutopilotPhase::Arrived => "ARRIVED (ORBIT INSERTED)",
                _ => "CRUISE",
            };
            **text = format!("AUTOPILOT [LORENTZ/L]: {} [L to disengage]", phase_str);
            *color = TextColor(Color::srgb(0.25, 0.90, 1.0)); // Cyan
        } else if shuttle.align_to_target || shuttle.autopilot_mode == crate::flight::AutopilotMode::AlignOnly {
            **text = format!("AUTOPILOT [AIM/F]: [TRACKING {} • F to disengage]", nav.target_name);
            *color = TextColor(Color::srgb(0.2, 0.95, 1.0));
        } else if shuttle.flight_assist {
            let free_str = if camera.look_yaw.abs() > 0.02 || camera.look_pitch.abs() > 0.02 || camera.free_look_active {
                " • [FREE-LOOK ACTIVE]"
            } else {
                ""
            };
            **text = format!("ROTATIONAL SAS: [ACTIVE (W to toggle)]{}", free_str);
            *color = TextColor(Color::srgb(0.2, 1.0, 0.4));
        } else {
            let free_str = if camera.look_yaw.abs() > 0.02 || camera.look_pitch.abs() > 0.02 || camera.free_look_active {
                " • [FREE-LOOK ACTIVE]"
            } else {
                ""
            };
            **text = format!("ROTATIONAL SAS: [OFF - RAW TUMBLE (W to toggle)]{}", free_str);
            *color = TextColor(Color::srgb(1.0, 0.6, 0.1));
        }
    }

    // 3. Update Dual Spacetime Clocks & Settings Text
    for mut text in clocks_query.iter_mut() {
        let (yaw, pitch, roll) = transform.rotation.to_euler(EulerRot::YXZ);

        let c_str = if (config.c - REAL_C).abs() < 1.0 {
            "299,792,458 m/s [REAL c]".to_string()
        } else {
            format!("{} m/s [DEMO c]", format_number_with_commas_2(config.c))
        };

        let view_str = match camera.mode {
            CameraViewMode::Cockpit => {
                if camera.look_yaw.abs() > 0.02 || camera.look_pitch.abs() > 0.02 {
                    "COCKPIT [LOOK]".to_string()
                } else {
                    "COCKPIT [V]".to_string()
                }
            }
            CameraViewMode::Chase => {
                if camera.look_yaw.abs() > 0.02 || camera.look_pitch.abs() > 0.02 {
                    format!("CHASE [ORBIT] ({:.0}m)", camera.chase_distance)
                } else {
                    format!("CHASE [V] ({:.0}m)", camera.chase_distance)
                }
            }
        };

        let cursor_str = if is_cursor_locked {
            format!("LOCKED [Tab] ({:.5})", shuttle.mouse_sensitivity.x)
        } else {
            "FREE".to_string()
        };

        let ship_time = format_timestamp(rel_state.proper_time);
        let universe_time = format_timestamp(rel_state.coordinate_time);

        let detected_hz = monitors
            .get_single()
            .ok()
            .and_then(|m| m.refresh_rate_millihertz)
            .map(|mhz| (mhz as f64 / 1000.0).round() as u32);

        let is_vsync = matches!(
            window.present_mode,
            bevy::window::PresentMode::AutoVsync
                | bevy::window::PresentMode::Fifo
                | bevy::window::PresentMode::FifoRelaxed
        );

        let vsync_str = if is_vsync {
            if let Some(hz) = detected_hz {
                format!("VSYNC: ON [{}Hz] (F2)", hz)
            } else {
                "VSYNC: ON [SYNCED] (F2)".to_string()
            }
        } else {
            "VSYNC: OFF [UNCAPPED] (F2)".to_string()
        };

        let aberr_str = if optics.aberration_enabled { "ON" } else { "OFF" };
        let fwd_doppler = if rel_state.beta > 0.001 {
            let fwd = transform.forward().as_vec3();
            let beta_vec = rel_state.beta_vec.as_vec3();
            crate::relativity::relativistic_doppler_factor(fwd, beta_vec, rel_state.gamma as f32)
        } else {
            1.0
        };

        let doppler_status = if !optics.doppler_enabled {
            "OFF (F5)".to_string()
        } else if fwd_doppler > 1.05 {
            format!("{:.2}x [BLUE] (F5)", fwd_doppler)
        } else if fwd_doppler < 0.95 {
            format!("{:.2}x [RED] (F5)", fwd_doppler)
        } else {
            "1.00x [REST] (F5)".to_string()
        };

        let time_scale_str = if (config.time_scale - 1.0).abs() < 0.01 {
            "1.00x (F6/F7)".to_string()
        } else {
            format!("{:.2}x [SLOW-MO] (F6/F7/F8)", config.time_scale)
        };

        **text = format!(
            "FPS: {:>5.1} ({:>4.1} ms) | {}\nSHIP CLOCK (τ):      {}\nUNIVERSE CLOCK (t):  {}\nTIME DILATION (γ):   {:>8.3} : 1\nTIME SCALE:          {}\nSPEED OF LIGHT (c):  {}\nABERRATION: [{} (F4)] | DOPPLER: {}\nVIEW: {} | MOUSE: {}\nPITCH: {:>+4.0}° | YAW: {:>+4.0}° | ROLL: {:>+4.0}°",
            fps_tracker.smoothed_fps,
            fps_tracker.smoothed_frame_time_ms,
            vsync_str,
            ship_time,
            universe_time,
            rel_state.gamma,
            time_scale_str,
            c_str,
            aberr_str,
            doppler_status,
            view_str,
            cursor_str,
            pitch.to_degrees(),
            yaw.to_degrees(),
            roll.to_degrees(),
        );
    }

    // 4. Update Targeting & Navigation Computer Text
    for mut text in nav_query.iter_mut() {
        if nav.target_entity.is_some() {
            let dist_str = crate::navigation::format_nav_distance(nav.distance_m);
            let alt_str = if shuttle.landed_body.is_some() && nav.closest_body_entity == nav.target_entity {
                " | ALT: 0 m [TOUCHDOWN]".to_string()
            } else if nav.closest_body_entity == nav.target_entity {
                if let Some(alt) = nav.altitude_m {
                    if alt < 1.0 {
                        " | ALT: 0 m [TOUCHDOWN]".to_string()
                    } else {
                        format!(" | ALT: {}", crate::navigation::format_nav_distance(alt))
                    }
                } else {
                    String::new()
                }
            } else {
                let tgt_part = if let Some(alt) = nav.altitude_m {
                    format!(" | ALT: {}", crate::navigation::format_nav_distance(alt))
                } else {
                    String::new()
                };
                let closest_part = if let (Some(ref c_name), Some(c_alt)) = (&nav.closest_body_name, nav.closest_altitude_m) {
                    if c_alt < 1.0 || shuttle.landed_body == nav.closest_body_entity {
                        format!("\nPROXIMITY: {} [0 m TOUCHDOWN]", c_name.to_uppercase())
                    } else {
                        format!("\nPROXIMITY: {} (ALT: {})", c_name.to_uppercase(), crate::navigation::format_nav_distance(c_alt))
                    }
                } else {
                    String::new()
                };
                format!("{}{}", tgt_part, closest_part)
            };

            let rel_speed_str = if nav.relative_speed >= 1_000.0 {
                format!("{:.2} km/s", nav.relative_speed / 1_000.0)
            } else {
                format!("{:.1} m/s", nav.relative_speed)
            };
            let tgt_speed_str = if nav.target_speed >= 1_000.0 {
                format!("{:.2} km/s", nav.target_speed / 1_000.0)
            } else {
                format!("{:.1} m/s", nav.target_speed)
            };

            let radial_str = if nav.closing {
                format!("{:+8.1} m/s [CLOSING] | LAT: {:>5.1} m/s", nav.radial_velocity, nav.lateral_velocity)
            } else if nav.radial_velocity.abs() < 1.0 {
                format!("    0.0 m/s [STATIONARY] | LAT: {:>5.1} m/s", nav.lateral_velocity)
            } else {
                format!("{:+8.1} m/s [OPENING] | LAT: {:>5.1} m/s", nav.radial_velocity, nav.lateral_velocity)
            };

            let eta_str = if let Some(eta) = nav.eta_seconds {
                let gamma = (rel_state.gamma as f64).max(1.0);
                let proper_eta = eta / gamma;
                format!(
                    "ETA (t): {:>12} [UNIV]\nETA (τ): {:>12} [SHIP]",
                    crate::navigation::format_eta(eta),
                    crate::navigation::format_eta(proper_eta)
                )
            } else {
                "ETA:     N/A".to_string()
            };

            let temp_str = if let (Some(t_rest), Some(t_perceived)) = (nav.rest_temperature_k, nav.perceived_temperature_k) {
                if (nav.doppler_factor - 1.0).abs() < 0.01 {
                    format!("TEMP:    {:.0} K [REST]", t_rest)
                } else if nav.doppler_factor > 1.0 {
                    format!("TEMP:    {:.0} K -> {:.0} K [{:.2}x BLUE]", t_rest, t_perceived, nav.doppler_factor)
                } else {
                    format!("TEMP:    {:.0} K -> {:.0} K [{:.2}x RED]", t_rest, t_perceived, nav.doppler_factor)
                }
            } else {
                "TEMP:    N/A".to_string()
            };

            let occluded_str = if let Some(ref obs) = nav.occluded_by_name {
                format!("\nWARN:    LINE OF SIGHT BLOCKED BY [{}]", obs.to_uppercase())
            } else {
                String::new()
            };

            **text = format!(
                "RANGE:   {} (BRG: {:>+4.0}°){}\nREL SPD: {:<14} | TGT: {}\nRADIAL:  {}\n{}\n{}{}",
                dist_str,
                nav.bearing_deg,
                alt_str,
                rel_speed_str,
                tgt_speed_str,
                radial_str,
                temp_str,
                eta_str,
                occluded_str
            );
        } else {
            let proximity_str = if let (Some(ref c_name), Some(c_alt)) = (&nav.closest_body_name, nav.closest_altitude_m) {
                if c_alt < 1.0 {
                    format!("PROXIMITY: {} [0 m TOUCHDOWN]\n", c_name.to_uppercase())
                } else {
                    format!("PROXIMITY: {} (ALT: {})\n", c_name.to_uppercase(), crate::navigation::format_nav_distance(c_alt))
                }
            } else {
                String::new()
            };
            **text = format!("{}RANGE:  N/A\nRADIAL: N/A\n[T Cycle • Y Nearest • F Aim • H Warp • L Lorentz • G Clear]", proximity_str);
        }
    }

    // 4b. Update Rolling Header Target Lock Label
    for mut text in nav_lock_query.iter_mut() {
        if nav.target_entity.is_some() {
            **text = format!("LOCK:  {} [{}]", nav.target_name, nav.target_class);
        } else {
            **text = "LOCK:  [NO TARGET]".to_string();
        }
    }
}

/// Relative spatial alignment of the relativistic searchlight beam relative to the observer's gaze.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RelativisticBeamAlignment {
    /// In forward line of sight (searchlight beam ahead)
    Forward { off_center_deg: f64 },
    /// In rear line of sight (searchlight beam behind, forward view is expansion void)
    Aft { off_center_deg: f64 },
    /// Lateral / drifting flight (searchlight beam on flank)
    Lateral { angle_deg: f64, flank: &'static str },
}

/// Computes the spatial alignment and angle of the relativistic aberration searchlight beam
/// relative to the observer camera's forward, right, and up axes.
pub fn calculate_relativistic_beam_alignment(
    cam_forward: DVec3,
    cam_right: DVec3,
    cam_up: DVec3,
    velocity: DVec3,
    beta: f64,
) -> RelativisticBeamAlignment {
    if beta < 0.005 || velocity.length_squared() < 1e-8 {
        return RelativisticBeamAlignment::Forward { off_center_deg: 0.0 };
    }
    let v_dir = velocity.normalize();
    let cos_look = cam_forward.dot(v_dir).clamp(-1.0, 1.0);
    let look_angle_deg = cos_look.acos().to_degrees();

    if look_angle_deg <= 45.0 {
        RelativisticBeamAlignment::Forward {
            off_center_deg: look_angle_deg,
        }
    } else if look_angle_deg >= 135.0 {
        RelativisticBeamAlignment::Aft {
            off_center_deg: look_angle_deg,
        }
    } else {
        let lat_dot = cam_right.dot(v_dir);
        let vert_dot = cam_up.dot(v_dir);
        let flank = if lat_dot.abs() >= vert_dot.abs() {
            if lat_dot > 0.0 {
                "STBD"
            } else {
                "PORT"
            }
        } else if vert_dot > 0.0 {
            "DORSAL"
        } else {
            "VENTRAL"
        };
        RelativisticBeamAlignment::Lateral {
            angle_deg: look_angle_deg,
            flank,
        }
    }
}

/// Real-time updates for Phase 5 Relativistic Avionics:
/// 1. Dynamic Beta (v/c) tape gauge with multi-stage color transitions & 95%+ pulse
/// 2. Logarithmic Lorentz gamma meter and visual dilation alert badge
/// 3. Direction-aware Relativistic Aberration searchlight cone aperture & cosmic void indicator
pub fn update_relativistic_gauges_system(
    time: Res<Time>,
    shuttle_query: Query<&RelativisticState, With<Shuttle>>,
    camera_query: Query<&Transform, With<SpaceCamera>>,
    optics: Res<crate::relativity::RelativisticOpticsConfig>,
    mut beta_tape_query: Query<
        (&mut Node, &mut BackgroundColor),
        (
            With<BetaTapeFill>,
            Without<GammaTapeFill>,
            Without<AberrationConeFill>,
            Without<GammaAlertBadge>,
        ),
    >,
    mut beta_text_query: Query<
        (&mut Text, &mut TextColor),
        (
            With<BetaTapeText>,
            Without<TelemetryText>,
            Without<FlightModeText>,
            Without<SpacetimeClocksText>,
            Without<NavTelemetryText>,
            Without<NavTargetLockNameText>,
            Without<CrosshairHoverText>,
            Without<GammaAlertText>,
            Without<AberrationConeText>,
            Without<VisorStatusText>,
            Without<SettingsOverlayTelemetryText>,
        ),
    >,
    mut gamma_tape_query: Query<
        (&mut Node, &mut BackgroundColor),
        (
            With<GammaTapeFill>,
            Without<BetaTapeFill>,
            Without<AberrationConeFill>,
            Without<GammaAlertBadge>,
        ),
    >,
    mut gamma_badge_query: Query<
        (&mut BackgroundColor, &mut BorderColor),
        (
            With<GammaAlertBadge>,
            Without<BetaTapeFill>,
            Without<GammaTapeFill>,
            Without<AberrationConeFill>,
        ),
    >,
    mut gamma_alert_text_query: Query<
        (&mut Text, &mut TextColor),
        (
            With<GammaAlertText>,
            Without<TelemetryText>,
            Without<FlightModeText>,
            Without<SpacetimeClocksText>,
            Without<NavTelemetryText>,
            Without<NavTargetLockNameText>,
            Without<CrosshairHoverText>,
            Without<BetaTapeText>,
            Without<AberrationConeText>,
            Without<VisorStatusText>,
            Without<SettingsOverlayTelemetryText>,
        ),
    >,
    mut cone_fill_query: Query<
        (&mut Node, &mut BackgroundColor),
        (
            With<AberrationConeFill>,
            Without<BetaTapeFill>,
            Without<GammaTapeFill>,
            Without<GammaAlertBadge>,
        ),
    >,
    mut cone_text_query: Query<
        (&mut Text, &mut TextColor),
        (
            With<AberrationConeText>,
            Without<TelemetryText>,
            Without<FlightModeText>,
            Without<SpacetimeClocksText>,
            Without<NavTelemetryText>,
            Without<NavTargetLockNameText>,
            Without<CrosshairHoverText>,
            Without<BetaTapeText>,
            Without<GammaAlertText>,
            Without<VisorStatusText>,
            Without<SettingsOverlayTelemetryText>,
        ),
    >,
) {
    let Ok(rel_state) = shuttle_query.get_single() else {
        return;
    };

    let beta = rel_state.beta.clamp(0.0, 0.999999);
    let beta_pct = (beta * 100.0) as f32;

    // 1. Beta Velocity Progress Bar & Dynamic Color Transitions
    for (mut node, mut bg) in beta_tape_query.iter_mut() {
        node.width = Val::Percent(beta_pct);
        if beta < 0.50 {
            let t = (beta / 0.50) as f32;
            *bg = BackgroundColor(Color::srgb(
                0.2 + 0.3 * t,
                0.75 - 0.05 * t,
                1.0 - 0.2 * t,
            ));
        } else if beta < 0.85 {
            let t = ((beta - 0.50) / 0.35) as f32;
            *bg = BackgroundColor(Color::srgb(
                0.5 + 0.5 * t,
                0.7 - 0.3 * t,
                0.8 - 0.7 * t,
            ));
        } else {
            let pulse = if beta >= 0.95 {
                (time.elapsed_secs() * 8.0).sin().abs() * 0.25
            } else {
                0.0
            };
            *bg = BackgroundColor(Color::srgb(
                1.0,
                0.15 + pulse * 0.3,
                0.2 + pulse * 0.2,
            ));
        }
    }

    // 2. Beta Numeric Tape Text
    for (mut text, mut color) in beta_text_query.iter_mut() {
        **text = format!("VELOCITY / c: β = {:<8.6} ({:>5.2}% c)", beta, beta * 100.0);
        if beta < 0.50 {
            *color = TextColor(Color::srgb(0.3, 0.85, 1.0));
        } else if beta < 0.85 {
            *color = TextColor(Color::srgb(1.0, 0.75, 0.2));
        } else {
            *color = TextColor(Color::srgb(1.0, 0.3, 0.35));
        }
    }

    // 3. Lorentz Gamma Meter Bar (Logarithmic progression up to 16x+)
    let gamma = rel_state.gamma;
    let gamma_log = if gamma > 1.0 { (gamma - 1.0).log2() } else { 0.0 };
    let gamma_pct = ((gamma_log / 4.0).clamp(0.0, 1.0) * 100.0) as f32;

    for (mut node, mut bg) in gamma_tape_query.iter_mut() {
        node.width = Val::Percent(gamma_pct);
        if gamma < 1.25 {
            *bg = BackgroundColor(Color::srgb(0.25, 0.85, 0.45));
        } else if gamma < 2.50 {
            *bg = BackgroundColor(Color::srgb(0.2, 0.80, 1.0));
        } else if gamma < 5.00 {
            *bg = BackgroundColor(Color::srgb(1.0, 0.70, 0.15));
        } else {
            *bg = BackgroundColor(Color::srgb(1.0, 0.20, 0.30));
        }
    }

    // 4. Lorentz Gamma Alert Badge Container
    for (mut bg, mut border) in gamma_badge_query.iter_mut() {
        if gamma < 1.10 {
            *bg = BackgroundColor(Color::srgba(0.02, 0.08, 0.04, 0.85));
            *border = BorderColor(Color::srgba(0.2, 0.8, 0.4, 0.6));
        } else if gamma < 2.00 {
            *bg = BackgroundColor(Color::srgba(0.02, 0.06, 0.12, 0.85));
            *border = BorderColor(Color::srgba(0.2, 0.6, 1.0, 0.7));
        } else if gamma < 5.00 {
            *bg = BackgroundColor(Color::srgba(0.12, 0.08, 0.02, 0.85));
            *border = BorderColor(Color::srgba(1.0, 0.7, 0.2, 0.8));
        } else if gamma < 10.00 {
            *bg = BackgroundColor(Color::srgba(0.15, 0.04, 0.02, 0.90));
            *border = BorderColor(Color::srgba(1.0, 0.4, 0.1, 0.9));
        } else {
            let flash = (time.elapsed_secs() * 10.0).sin().abs() > 0.5;
            if flash {
                *bg = BackgroundColor(Color::srgba(0.35, 0.05, 0.05, 0.95));
                *border = BorderColor(Color::srgb(1.0, 0.2, 0.2));
            } else {
                *bg = BackgroundColor(Color::srgba(0.20, 0.02, 0.02, 0.90));
                *border = BorderColor(Color::srgba(0.9, 0.1, 0.1, 0.8));
            }
        }
    }

    // 5. Lorentz Gamma Alert Badge Text
    for (mut text, mut color) in gamma_alert_text_query.iter_mut() {
        if gamma < 1.10 {
            **text = format!("γ: {:>5.3}× [REST]", gamma);
            *color = TextColor(Color::srgb(0.3, 0.9, 0.5));
        } else if gamma < 2.00 {
            **text = format!("γ: {:>5.3}× [MODERATE DILATION]", gamma);
            *color = TextColor(Color::srgb(0.2, 0.85, 1.0));
        } else if gamma < 5.00 {
            **text = format!("γ: {:>5.2}× [HIGH DILATION >2×]", gamma);
            *color = TextColor(Color::srgb(1.0, 0.75, 0.2));
        } else if gamma < 10.00 {
            **text = format!("γ: {:>5.2}× [CRITICAL DILATION >5×]", gamma);
            *color = TextColor(Color::srgb(1.0, 0.45, 0.1));
        } else {
            **text = format!("γ: {:>5.1}× [TEMPORAL ANOMALY >10×]", gamma);
            *color = TextColor(Color::srgb(1.0, 0.2, 0.3));
        }
    }

    // 6. Direction-Aware Relativistic Aberration Searchlight & Aperture
    let (cam_fwd, cam_right, cam_up) = if let Some(cam_tf) = camera_query.iter().next() {
        (
            cam_tf.forward().as_dvec3(),
            cam_tf.right().as_dvec3(),
            cam_tf.up().as_dvec3(),
        )
    } else {
        (DVec3::NEG_Z, DVec3::X, DVec3::Y)
    };

    let beam_alignment = calculate_relativistic_beam_alignment(
        cam_fwd,
        cam_right,
        cam_up,
        rel_state.velocity,
        beta,
    );

    let theta_half_rad = beta.acos();
    let total_cone_deg = 2.0 * theta_half_rad.to_degrees();

    for (mut node, mut bg) in cone_fill_query.iter_mut() {
        if !optics.aberration_enabled {
            node.width = Val::Percent(100.0);
            *bg = BackgroundColor(Color::srgba(0.25, 0.35, 0.45, 0.5));
        } else if beta < 0.005 {
            node.width = Val::Percent(100.0);
            *bg = BackgroundColor(Color::srgb(0.2, 0.75, 1.0));
        } else {
            match beam_alignment {
                RelativisticBeamAlignment::Forward { .. } => {
                    let aperture_pct = ((total_cone_deg / 180.0).clamp(0.05, 1.0) * 100.0) as f32;
                    node.width = Val::Percent(aperture_pct);
                    if total_cone_deg > 120.0 {
                        *bg = BackgroundColor(Color::srgb(0.2, 0.75, 1.0));
                    } else if total_cone_deg > 60.0 {
                        *bg = BackgroundColor(Color::srgb(1.0, 0.7, 0.2));
                    } else {
                        *bg = BackgroundColor(Color::srgb(0.85, 0.25, 1.0));
                    }
                }
                RelativisticBeamAlignment::Aft { .. } => {
                    // Reverse flight: forward viewport is expanded into cosmic void
                    // Aperture bar fills to 100% in crimson void alert
                    node.width = Val::Percent(100.0);
                    *bg = BackgroundColor(Color::srgb(0.95, 0.2, 0.35));
                }
                RelativisticBeamAlignment::Lateral { .. } => {
                    // Drifting flight: show aperture width with distinct amber drift color
                    let aperture_pct = ((total_cone_deg / 180.0).clamp(0.05, 1.0) * 100.0) as f32;
                    node.width = Val::Percent(aperture_pct);
                    *bg = BackgroundColor(Color::srgb(1.0, 0.65, 0.15));
                }
            }
        }
    }

    // 7. Relativistic Aberration Cone Text Readout
    for (mut text, mut color) in cone_text_query.iter_mut() {
        if !optics.aberration_enabled {
            **text = "ABERRATION: [DISABLED (F4 to toggle)]".to_string();
            *color = TextColor(Color::srgba(0.6, 0.7, 0.8, 0.6));
        } else if beta < 0.005 {
            **text = "ABERRATION: 180.0° [REST FRAME - WIDE VIEW]".to_string();
            *color = TextColor(Color::srgb(0.3, 0.85, 1.0));
        } else {
            match beam_alignment {
                RelativisticBeamAlignment::Forward { off_center_deg } => {
                    if off_center_deg <= 15.0 {
                        if total_cone_deg > 120.0 {
                            **text = format!("ABERRATION: {:>5.1}° [FWD SEARCHLIGHT - IN FOV]", total_cone_deg);
                            *color = TextColor(Color::srgb(0.3, 0.85, 1.0));
                        } else if total_cone_deg > 60.0 {
                            **text = format!("ABERRATION: {:>5.1}° [FWD SEARCHLIGHT - COMPRESSION]", total_cone_deg);
                            *color = TextColor(Color::srgb(1.0, 0.75, 0.2));
                        } else {
                            **text = format!("ABERRATION: {:>5.1}° [FWD SEARCHLIGHT - BEAM]", total_cone_deg);
                            *color = TextColor(Color::srgb(0.9, 0.35, 1.0));
                        }
                    } else {
                        **text = format!("ABERRATION: {:>5.1}° [FWD SEARCHLIGHT - {:>2.0}° OFF-AXIS]", total_cone_deg, off_center_deg);
                        *color = TextColor(Color::srgb(0.35, 0.85, 1.0));
                    }
                }
                RelativisticBeamAlignment::Aft { .. } => {
                    **text = format!("ABERRATION: {:>5.1}° [AFT BEAM - FORWARD EXPANSION VOID]", total_cone_deg);
                    *color = TextColor(Color::srgb(1.0, 0.3, 0.45));
                }
                RelativisticBeamAlignment::Lateral { angle_deg, flank } => {
                    **text = format!("ABERRATION: {:>5.1}° [DRIFT: {:>2.0}° {}]", total_cone_deg, angle_deg, flank);
                    *color = TextColor(Color::srgb(1.0, 0.75, 0.2));
                }
            }
        }
    }
}

/// Real-time updates for Canopy Visor HUD badge and full-screen overlay tint
pub fn canopy_visor_ui_system(
    visor: Res<crate::celestial::CanopyVisorState>,
    mut text_query: Query<(&mut Text, &mut TextColor), With<VisorStatusText>>,
    mut overlay_query: Query<&mut BackgroundColor, With<VisorScreenOverlay>>,
) {
    for (mut text, mut color) in text_query.iter_mut() {
        **text = format!("CANOPY VISOR: [{} (K to cycle)]", visor.mode.label());
        *color = match visor.mode {
            crate::celestial::CanopyVisorMode::Clear => TextColor(Color::srgb(0.3, 0.85, 1.0)),
            crate::celestial::CanopyVisorMode::Polarized => TextColor(Color::srgb(1.0, 0.75, 0.2)),
            crate::celestial::CanopyVisorMode::SolarFilter => TextColor(Color::srgb(1.0, 0.45, 0.2)),
        };
    }

    for mut bg in overlay_query.iter_mut() {
        match visor.mode {
            crate::celestial::CanopyVisorMode::Clear => {
                *bg = BackgroundColor(Color::NONE);
            }
            crate::celestial::CanopyVisorMode::Polarized => {
                *bg = BackgroundColor(Color::srgba(0.01, 0.02, 0.05, 0.16));
            }
            crate::celestial::CanopyVisorMode::SolarFilter => {
                *bg = BackgroundColor(Color::srgba(0.12, 0.04, 0.01, 0.22));
            }
        }
    }
}

/// Listens for [I] (cycle Full -> Minimal -> Hidden) and [Shift+I] (instant toggle Hidden)
pub fn hud_display_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut hud_state: ResMut<HudDisplayState>,
) {
    if keyboard.just_pressed(KeyCode::KeyI) {
        let shift_pressed = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
        if shift_pressed {
            hud_state.toggle_hidden();
        } else {
            hud_state.cycle();
        }
    }
}

/// Applies active HUD display mode visibility (Full / Minimal / Hidden) and updates Minimal HUD readouts
pub fn hud_display_update_system(
    hud_state: Res<HudDisplayState>,
    shuttle_query: Query<(&Shuttle, &RelativisticState), With<Shuttle>>,
    config: Res<RelativisticConfig>,
    pause: Res<SimulationPause>,
    fps_tracker: Res<FpsTracker>,
    nav: Res<crate::navigation::NavigationComputer>,
    mut full_hud_query: Query<&mut Node, (With<FullHudElement>, Without<MinimalHudRoot>, Without<MinimalTargetBox>, Without<StartupCreditsToast>)>,
    mut minimal_root_query: Query<&mut Node, (With<MinimalHudRoot>, Without<FullHudElement>, Without<MinimalTargetBox>, Without<StartupCreditsToast>)>,
    mut minimal_target_box_query: Query<&mut Node, (With<MinimalTargetBox>, Without<FullHudElement>, Without<MinimalHudRoot>, Without<StartupCreditsToast>)>,
    mut toast_query: Query<&mut Node, (With<StartupCreditsToast>, Without<FullHudElement>, Without<MinimalHudRoot>, Without<MinimalTargetBox>)>,
    mut min_kinematics_query: Query<&mut Text, (With<MinimalKinematicsText>, Without<MinimalTargetText>, Without<MinimalMetricsText>)>,
    mut min_target_query: Query<&mut Text, (With<MinimalTargetText>, Without<MinimalKinematicsText>, Without<MinimalMetricsText>)>,
    mut min_metrics_query: Query<&mut Text, (With<MinimalMetricsText>, Without<MinimalKinematicsText>, Without<MinimalTargetText>)>,
) {
    let full_display = if hud_state.mode == HudDisplayMode::Full {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in full_hud_query.iter_mut() {
        if node.display != full_display {
            node.display = full_display;
        }
    }

    let minimal_display = if hud_state.mode == HudDisplayMode::Minimal {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in minimal_root_query.iter_mut() {
        if node.display != minimal_display {
            node.display = minimal_display;
        }
    }

    let toast_display = if hud_state.mode == HudDisplayMode::Hidden {
        Display::None
    } else {
        Display::Flex
    };
    for mut node in toast_query.iter_mut() {
        if node.display != toast_display {
            node.display = toast_display;
        }
    }

    if hud_state.mode != HudDisplayMode::Minimal {
        return;
    }

    let Ok((shuttle, rel_state)) = shuttle_query.get_single() else {
        return;
    };

    // 1. Minimal Top-Left Kinematics Pill
    let speed = rel_state.velocity.length();
    let speed_str = if speed >= 10_000.0 {
        format!("{:.2} km/s", speed / 1_000.0)
    } else {
        format!("{:.1} m/s", speed)
    };

    let alt_part = if shuttle.landed_body.is_some() {
        "ALT: 0 m [TOUCHDOWN]".to_string()
    } else if let Some(c_alt) = nav.closest_altitude_m {
        if c_alt < 1.0 {
            "ALT: 0 m [TOUCHDOWN]".to_string()
        } else if c_alt < 50_000_000.0 {
            format!("ALT: {}", crate::navigation::format_nav_distance(c_alt))
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let mut status_tags: Vec<String> = Vec::new();
    if pause.0 {
        status_tags.push("[PAUSED]".to_string());
    } else if shuttle.autopilot_mode == crate::flight::AutopilotMode::Hyperdrive {
        status_tags.push("[WARP AP]".to_string());
    } else if shuttle.autopilot_mode == crate::flight::AutopilotMode::Lorentz {
        status_tags.push("[LORENTZ AP]".to_string());
    } else if shuttle.braking_active {
        status_tags.push("[BRAKE]".to_string());
    }
    if (config.time_scale - 1.0).abs() >= 0.01 {
        status_tags.push(format!("[{:.2}×]", config.time_scale));
    }
    if !shuttle.flight_assist {
        status_tags.push("[SAS OFF]".to_string());
    }

    let mut second_line_parts: Vec<String> = Vec::new();
    if !alt_part.is_empty() {
        second_line_parts.push(alt_part);
    }
    if !status_tags.is_empty() {
        second_line_parts.push(status_tags.join("  "));
    }

    let line1 = format!(
        "SPD: {} ({:.2}% c)  •  γ: {:.2}×",
        speed_str,
        rel_state.beta * 100.0,
        rel_state.gamma,
    );
    let full_kinematics = if second_line_parts.is_empty() {
        line1
    } else {
        format!("{}\n{}", line1, second_line_parts.join("  •  "))
    };

    for mut text in min_kinematics_query.iter_mut() {
        **text = full_kinematics.clone();
    }

    // 2. Minimal Top-Center Target Pill (only visible when a target is locked)
    let has_target = nav.target_entity.is_some();
    for mut node in minimal_target_box_query.iter_mut() {
        let target_display = if has_target { Display::Flex } else { Display::None };
        if node.display != target_display {
            node.display = target_display;
        }
    }
    if has_target {
        let dist_str = crate::navigation::format_nav_distance(nav.distance_m);
        let rel_spd_str = if nav.relative_speed >= 1_000.0 {
            format!("{:.2} km/s", nav.relative_speed / 1_000.0)
        } else {
            format!("{:.1} m/s", nav.relative_speed)
        };
        for mut text in min_target_query.iter_mut() {
            **text = format!(
                "{}  •  {}  •  REL: {}",
                nav.target_name.to_uppercase(),
                dist_str,
                rel_spd_str
            );
        }
    }

    // 3. Minimal Top-Right Clocks & FPS Pill
    let ship_clock = format_timestamp(rel_state.proper_time);
    let univ_clock = format_timestamp(rel_state.coordinate_time);
    for mut text in min_metrics_query.iter_mut() {
        **text = format!(
            "τ {}  |  t {}  •  {:.0} FPS  •  [I] HUD",
            ship_clock,
            univ_clock,
            fps_tracker.smoothed_fps
        );
    }
}

/// Listens for F9 to toggle the Cockpit MFD Settings & Help Overlay, and Escape to dismiss.
/// Automatically manages mouse cursor visibility and locking state.
pub fn settings_overlay_input_system(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut overlay: ResMut<SettingsOverlayState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    overlay.just_closed = false;

    if keyboard.just_pressed(KeyCode::F9) {
        overlay.visible = !overlay.visible;
        if let Ok(mut window) = windows.get_single_mut() {
            if overlay.visible {
                window.cursor_options.grab_mode = CursorGrabMode::None;
                window.cursor_options.visible = true;
            } else {
                window.cursor_options.grab_mode = CursorGrabMode::Locked;
                window.cursor_options.visible = false;
            }
        }
    } else if keyboard.just_pressed(KeyCode::Escape) && overlay.visible {
        overlay.visible = false;
        overlay.just_closed = true;
        keyboard.clear_just_pressed(KeyCode::Escape);
        if let Ok(mut window) = windows.get_single_mut() {
            window.cursor_options.grab_mode = CursorGrabMode::Locked;
            window.cursor_options.visible = false;
        }
    }
}

/// Updates live simulation telemetry and system states inside the Settings & Help MFD Overlay
pub fn update_settings_overlay_system(
    overlay: Res<SettingsOverlayState>,
    shuttle_query: Query<(&Shuttle, &RelativisticState, &UniversalPosition), With<Shuttle>>,
    config: Res<RelativisticConfig>,
    optics: Res<crate::relativity::RelativisticOpticsConfig>,
    universe_config: Res<crate::celestial::UniverseConfig>,
    visor: Res<crate::celestial::CanopyVisorState>,
    pause: Res<SimulationPause>,
    nav: Res<crate::navigation::NavigationComputer>,
    audio_settings: Res<crate::AudioSettings>,
    (hud_state, texture_quality): (Option<Res<HudDisplayState>>, Option<Res<crate::celestial::TextureQualityConfig>>),
    texture_streaming: Option<Res<crate::celestial::DynamicTextureStreamingConfig>>,
    ram_cache: Option<Res<crate::celestial::PlanetTextureRamCache>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut root_query: Query<&mut Node, With<SettingsOverlayRoot>>,
    mut telemetry_query: Query<&mut Text, (With<SettingsOverlayTelemetryText>, Without<UniverseSwitchButtonText>)>,
    mut button_text_query: Query<&mut Text, (With<UniverseSwitchButtonText>, Without<SettingsOverlayTelemetryText>)>,
) {
    // 1. Sync overlay modal backdrop visibility
    for mut node in root_query.iter_mut() {
        let target_display = if overlay.visible {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != target_display {
            node.display = target_display;
        }
    }

    if !overlay.visible {
        return;
    }

    let Ok((shuttle, rel_state, _upos)) = shuttle_query.get_single() else {
        return;
    };
    let Ok(window) = windows.get_single() else {
        return;
    };

    let target_name = if nav.target_entity.is_some() {
        nav.target_name.as_str()
    } else {
        "NONE (Press T/Y)"
    };

    let universe_str = match universe_config.active_mode {
        crate::celestial::UniverseMode::DemoSector => format!("{} [c = 5,000 m/s] (Key U / F10 to switch)", universe_config.active_mode.label()),
        crate::celestial::UniverseMode::RealisticSolarSystem => format!("{} [c = 299,792 km/s] (Key U / F10 to switch)", universe_config.active_mode.label()),
    };

    let c_str = if config.is_demo_mode {
        format!("{} m/s [DEMO (Key 1/2)]", format_number_with_commas_2(config.c))
    } else {
        format!("{} m/s [REAL VACUUM c]", format_number_with_commas_2(config.c))
    };

    let vsync_str = match window.present_mode {
        bevy::window::PresentMode::AutoVsync => "ACTIVE [SYNCED] (F2)",
        _ => "UNCAPPED [MAX FPS] (F2)",
    };

    let display_mode_str = match window.mode {
        bevy::window::WindowMode::BorderlessFullscreen(_) => "BORDERLESS FULLSCREEN (F11)",
        bevy::window::WindowMode::Fullscreen(_) | bevy::window::WindowMode::SizedFullscreen(_) => "EXCLUSIVE FULLSCREEN (F11)",
        bevy::window::WindowMode::Windowed => "WINDOWED (F11)",
    };

    let audio_str = if audio_settings.music_muted {
        "MUTED (M to unmute)".to_string()
    } else {
        format!("PLAYING [{:.0}% VOL] (M)", audio_settings.master_volume * 100.0)
    };

    let time_str = if pause.0 {
        "PAUSED [F1 to resume]".to_string()
    } else if (config.time_scale - 1.0).abs() < 0.01 {
        "1.00× [REAL-TIME] (F6/F7)".to_string()
    } else {
        format!("{:.2}× [TIME-WARP] (F6/F7/F8)", config.time_scale)
    };

    let (gpu_str, tex_str) = if let Some(tex_cfg) = texture_quality {
        let gpu_type = if tex_cfg.is_discrete_gpu {
            "dGPU"
        } else {
            "iGPU"
        };
        (
            format!("{} [{}]", tex_cfg.gpu_name, gpu_type),
            format!("{}px [{}]", tex_cfg.max_texture_dimension, tex_cfg.quality_label),
        )
    } else {
        ("PROBING ADAPTER...".to_string(), "8192px [DEFAULT]".to_string())
    };

    let streaming_str = if let Some(st) = texture_streaming {
        let ram_info = if let Some(ref rc) = ram_cache {
            if rc.enabled {
                format!(" | RAM Cache: ON ({:.1}G)", rc.total_ram_gb)
            } else {
                format!(" | RAM Cache: OFF ({:.1}G)", rc.total_ram_gb)
            }
        } else {
            String::new()
        };
        if st.enabled {
            format!("DYNAMIC PROXIMITY LOD [NEAR 8K ↔ DISTANT 2K]{} (Shift+F12)", ram_info)
        } else {
            format!("OFF [STATIC ALL-RESIDENT]{} (Shift+F12)", ram_info)
        }
    } else {
        "DYNAMIC PROXIMITY LOD (Shift+F12)".to_string()
    };

    let dust_str = if !optics.doppler_enabled {
        "DISABLED (F5)".to_string()
    } else if rel_state.beta >= 0.55 {
        format!("ACTIVE [40° RAINBOW HALO @ {:.1}% c]", rel_state.beta * 100.0)
    } else {
        "BELOW 0.55c THRESHOLD".to_string()
    };

    let cmb_str = if !optics.doppler_enabled {
        "DISABLED (F5)".to_string()
    } else {
        let (temp_k, _, _) = crate::relativity::cmb_dipole_forward_optics(rel_state.gamma as f32, rel_state.beta as f32);
        if temp_k >= 500.0 {
            format!("VISIBLE [{:.0} K FORWARD THERMAL DOME]", temp_k)
        } else {
            format!("INVISIBLE MICROWAVES [{:.1} K @ γ = {:.2}]", temp_k, rel_state.gamma)
        }
    };

    let grav_mag = shuttle.current_grav_accel;
    let grav_g = grav_mag / 9.80665;
    let grav_str = if grav_mag >= 1_000_000.0 {
        format!("{:.1}M m/s² ({:.2}M g)", grav_mag / 1_000_000.0, grav_g / 1_000_000.0)
    } else if grav_mag >= 1_000.0 {
        format!("{:.1} km/s² ({:.1} g)", grav_mag / 1_000.0, grav_g)
    } else if grav_mag >= 0.01 {
        format!("{:.2} m/s² ({:.2} g)", grav_mag, grav_g)
    } else {
        "0.00 m/s² [ZERO-G / FREE-FALL]".to_string()
    };

    for mut text in telemetry_query.iter_mut() {
        **text = format!(
            "ACTIVE UNIVERSE:     {}\n\
             SPEED OF LIGHT (c):   {}\n\
             SHIP SPEED (v):       {} m/s ({:.2} km/s)\n\
             VELOCITY RATIO (β):   {:.4}c ({:.2}% c)\n\
             LORENTZ FACTOR (γ):   {:.4}×\n\
             TIME ACCELERATION:    {}\n\
             SHIP PROPER TIME (τ): {:.2} s\n\
             COORDINATE TIME (t):  {:.2} s\n\
             LOCAL GRAVITY:        {}\n\n\
             GPU / ADAPTER:        {}\n\
             TEXTURE RES CAP:      {}\n\
             TEXTURE STREAMING:    {}\n\
             TEXTURE FILTERING:    16× ANISOTROPIC TRILINEAR MIPMAPS\n\
             ANTI-ALIASING:        FXAA ACTIVE [EDGE SMOOTHING]\n\
             DISPLAY MODE:         {}\n\
             RELATIVISTIC ABERR:   {}\n\
             DOPPLER FREQUENCY:    {}\n\
             INTERSTELLAR DUST:    {}\n\
             COSMIC MICROWAVE:     {}\n\
             BEAMING FACTOR:       {}\n\
             CANOPY VISOR FILTER:  {}\n\n\
             ROTATIONAL SAS:       {}\n\
             AUTOPILOT HOMING:     {}\n\
             NAVIGATION LOCK:      {}\n\
             HUD DISPLAY MODE:     {}\n\
             AUDIO SOUNDTRACK:     {}\n\
             DISPLAY REFRESH:      {}",
            universe_str,
            c_str,
            format_number_with_commas_2(rel_state.velocity.length()),
            rel_state.velocity.length() / 1_000.0,
            rel_state.beta,
            rel_state.beta * 100.0,
            rel_state.gamma,
            time_str,
            rel_state.proper_time,
            rel_state.coordinate_time,
            grav_str,
            gpu_str,
            format!("{} [F12]", tex_str),
            streaming_str,
            display_mode_str,
            if optics.aberration_enabled { "ENABLED (F4)" } else { "BYPASSED (F4)" },
            if optics.doppler_enabled { "ENABLED (F5)" } else { "BYPASSED (F5)" },
            dust_str,
            cmb_str,
            if optics.beaming_enabled { "ENABLED" } else { "DISABLED" },
            visor.mode.label(),
            if shuttle.flight_assist { "ACTIVE [DAMPED] (W)" } else { "INERTIAL [TUMBLING] (W)" },
            if shuttle.align_to_target { "ACTIVE CONTINUOUS (F)" } else { "STANDBY (F)" },
            target_name,
            format!("{} (I / Shift+I)", hud_state.as_ref().map_or("FULL", |h| h.mode.label())),
            audio_str,
            vsync_str,
        );
    }

    for mut btn_text in button_text_query.iter_mut() {
        **btn_text = match universe_config.active_mode {
            crate::celestial::UniverseMode::DemoSector => "► SWITCH TO 1:1 SOLAR SYSTEM [U / F10]".to_string(),
            crate::celestial::UniverseMode::RealisticSolarSystem => "► SWITCH TO DEMO SECTOR [U / F10]".to_string(),
        };
    }
}

/// Handles clicking the Universe Switch Button in the Cockpit MFD Settings Overlay
pub fn universe_switch_button_system(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (Changed<Interaction>, With<UniverseSwitchButton>),
    >,
    mut universe_config: ResMut<crate::celestial::UniverseConfig>,
) {
    for (interaction, mut bg, mut border) in interaction_query.iter_mut() {
        match *interaction {
            Interaction::Pressed => {
                universe_config.pending_switch = Some(universe_config.active_mode.next());
                *bg = BackgroundColor(Color::srgba(0.1, 0.4, 0.6, 0.95));
                *border = BorderColor(Color::srgb(1.0, 0.9, 0.3));
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(Color::srgba(0.08, 0.25, 0.40, 0.85));
                *border = BorderColor(Color::srgb(0.4, 0.9, 1.0));
            }
            Interaction::None => {
                *bg = BackgroundColor(Color::srgba(0.05, 0.15, 0.25, 0.85));
                *border = BorderColor(Color::srgb(0.3, 0.8, 1.0));
            }
        }
    }
}

/// Handles clicking and hovering the interactive rolling Target selection buttons (< and >) in the Targeting panel
pub fn target_rolling_button_system(
    mut button_query: Query<
        (
            &Interaction,
            &mut BackgroundColor,
            &mut BorderColor,
            Option<&TargetPrevButton>,
            Option<&TargetNextButton>,
        ),
        (
            Changed<Interaction>,
            Or<(With<TargetPrevButton>, With<TargetNextButton>)>,
        ),
    >,
    mut nav: ResMut<crate::navigation::NavigationComputer>,
) {
    for (interaction, mut bg, mut border, is_prev, is_next) in button_query.iter_mut() {
        match *interaction {
            Interaction::Pressed => {
                if is_prev.is_some() {
                    nav.cycle_target_offset -= 1;
                }
                if is_next.is_some() {
                    nav.cycle_target_offset += 1;
                }
                *bg = BackgroundColor(Color::srgba(0.1, 0.4, 0.6, 0.95));
                *border = BorderColor(Color::srgb(1.0, 0.9, 0.3));
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(Color::srgba(0.08, 0.25, 0.40, 0.85));
                *border = BorderColor(Color::srgb(0.4, 0.9, 1.0));
            }
            Interaction::None => {
                *bg = BackgroundColor(Color::srgba(0.04, 0.12, 0.20, 0.70));
                *border = BorderColor(Color::srgb(0.2, 0.6, 0.8));
            }
        }
    }
}

/// Robust floating point formatter with 2 decimals that correctly rounds without wrapping overflow
fn format_number_with_commas_2(n: f64) -> String {
    let formatted = format!("{:.2}", n);
    let mut parts = formatted.split('.');
    let int_str = parts.next().unwrap_or("0");
    let fract_str = parts.next().unwrap_or("00");

    let mut result = String::new();
    let len = int_str.len();
    for (i, c) in int_str.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    format!("{}.{}", result, fract_str)
}

/// Robust floating point formatter with 3 decimals
fn format_number_with_commas_3(n: f64) -> String {
    let formatted = format!("{:.3}", n);
    let mut parts = formatted.split('.');
    let int_str = parts.next().unwrap_or("0");
    let fract_str = parts.next().unwrap_or("000");

    let mut result = String::new();
    let len = int_str.len();
    for (i, c) in int_str.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    format!("{}.{}", result, fract_str)
}

fn format_timestamp(seconds: f64) -> String {
    let days = (seconds / 86400.0).floor() as u64;
    let hours = ((seconds % 86400.0) / 3600.0).floor() as u64;
    let mins = ((seconds % 3600.0) / 60.0).floor() as u64;
    let secs = (seconds % 60.0).floor() as u64;
    let centis = ((seconds % 1.0) * 100.0).floor() as u64;

    if days > 0 {
        format!("{}d {:02}:{:02}:{:02}.{:02}", days, hours, mins, secs, centis)
    } else if hours > 0 {
        format!("{:02}:{:02}:{:02}.{:02}", hours, mins, secs, centis)
    } else {
        format!("{:02}:{:02}.{:02}", mins, secs, centis)
    }
}

/// Dynamically manages flight crosshair visibility and identifies any navigation object under the crosshair
pub fn crosshair_hover_system(
    camera_query: Query<(&Camera, &Transform, &SpaceCamera)>,
    pause: Res<SimulationPause>,
    hud_state: Option<Res<HudDisplayState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    shuttle_query: Query<&UniversalPosition, With<Shuttle>>,
    targets_query: Query<(&crate::celestial::NavTarget, &UniversalPosition, &Transform)>,
    mut crosshair_query: Query<&mut Node, With<FlightCrosshairUi>>,
    mut hover_text_query: Query<&mut Text, With<CrosshairHoverText>>,
) {
    let Ok((camera, cam_tf, space_cam)) = camera_query.get_single() else {
        return;
    };
    let cam_global = GlobalTransform::from(*cam_tf);
    let Ok(mut crosshair_node) = crosshair_query.get_single_mut() else {
        return;
    };
    let Ok(mut hover_text) = hover_text_query.get_single_mut() else {
        return;
    };

    if hud_state.as_ref().map_or(false, |h| h.mode == HudDisplayMode::Hidden) {
        crosshair_node.display = Display::None;
        **hover_text = "".to_string();
        return;
    }

    let is_cockpit = space_cam.mode == CameraViewMode::Cockpit;
    let is_freelook = space_cam.free_look_active
        || space_cam.look_yaw.abs() > 0.02
        || space_cam.look_pitch.abs() > 0.02
        || pause.0;

    // Crosshair is active in first-person (cockpit) or during active free-look / pause
    let show_crosshair = is_cockpit || is_freelook;

    if !show_crosshair {
        crosshair_node.display = Display::None;
        **hover_text = "".to_string();
        return;
    }

    crosshair_node.display = Display::Flex;

    let Ok(window) = windows.get_single() else {
        return;
    };
    let Ok(shuttle_upos) = shuttle_query.get_single() else {
        return;
    };

    let screen_center = Vec2::new(window.width() * 0.5, window.height() * 0.5);
    let cam_forward = cam_global.forward().normalize();
    let cam_pos = cam_global.translation();

    let mut closest_target = None;
    let mut min_pixel_dist = f32::MAX;

    for (target, target_upos, target_tf) in targets_query.iter() {
        let to_target = target_tf.translation - cam_pos;
        let dist_render = to_target.length();
        if dist_render < 0.1 {
            continue;
        }

        let dir = to_target / dist_render;
        if cam_forward.dot(dir) <= 0.0 {
            // Behind camera
            continue;
        }

        if let Ok(viewport_pos) = camera.world_to_viewport(&cam_global, target_tf.translation) {
            let pixel_dist = (viewport_pos - screen_center).length();

            // Compute target's apparent pixel size + generous hover bracket
            let render_radius = target_tf.scale.x;
            let ang_radius = (render_radius / dist_render).clamp(0.0, 1.5);
            let pixel_radius = ang_radius * (window.height() / 1.05);
            let hover_tolerance = (pixel_radius + 36.0).max(42.0);

            if pixel_dist <= hover_tolerance && pixel_dist < min_pixel_dist {
                min_pixel_dist = pixel_dist;
                let true_dist = (target_upos.0 - shuttle_upos.0).length();
                closest_target = Some((target.name, true_dist));
            }
        }
    }

    if let Some((name, dist)) = closest_target {
        let dist_str = crate::navigation::format_nav_distance(dist);
        **hover_text = format!(">> {} <<\n[{}]", name.to_uppercase(), dist_str);
    } else {
        **hover_text = "".to_string();
    }
}

/// Smoothly fades out and despawns the startup credits toast after its display duration
pub fn fade_startup_credits_toast_system(
    time: Res<Time>,
    loading_tracker: Option<Res<StartupLoadingTracker>>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut StartupCreditsToast, &mut BackgroundColor, &mut BorderColor)>,
) {
    if loading_tracker.as_ref().map_or(false, |t| !t.is_loaded) {
        return; // Hold countdown until the loading screen finishes and fades out
    }
    let dt = time.delta_secs();
    for (entity, mut toast, mut bg, mut border) in query.iter_mut() {
        toast.time_remaining -= dt;
        if toast.time_remaining <= 0.0 {
            commands.entity(entity).despawn_recursive();
        } else if toast.time_remaining < 3.0 {
            let alpha = (toast.time_remaining / 3.0).clamp(0.0, 1.0);
            bg.0 = Color::srgba(0.01, 0.03, 0.07, 0.90 * alpha);
            border.0 = Color::srgba(0.25, 0.75, 1.0, 0.50 * alpha);
        }
    }
}

/// Spawns the fullscreen cinematic loading screen overlay at startup
pub fn setup_loading_screen(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font_sans = asset_server.load("fonts/DejaVuSans.ttf");
    let font_mono = asset_server.load("fonts/DejaVuSansMono.ttf");

    commands.spawn((
        LoadingScreenRoot,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.006, 0.010, 0.020, 1.0)),
        GlobalZIndex(9999),
    )).with_children(|root| {
        root.spawn((
            LoadingScreenPanel,
            Node {
                width: Val::Px(620.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(22.0)),
                row_gap: Val::Px(12.0),
                border: UiRect::all(Val::Px(1.5)),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.010, 0.020, 0.040, 0.98)),
            BorderColor(Color::srgba(0.25, 0.75, 1.0, 0.70)),
        )).with_children(|panel| {
            // Simulator Title
            panel.spawn((
                LoadingScreenText,
                Text::new("RELATIVISTIC SPACE SIMULATOR"),
                TextFont {
                    font: font_sans.clone(),
                    font_size: 16.5,
                    ..default()
                },
                TextColor(Color::srgb(0.35, 0.88, 1.0)),
            ));

            // Subtitle
            panel.spawn((
                LoadingScreenText,
                Text::new("ORBITAL DEPLOYMENT & TELEMETRY INITIALIZATION"),
                TextFont {
                    font: font_mono.clone(),
                    font_size: 10.5,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.72, 0.90)),
            ));

            // Mission info box
            panel.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(10.0)),
                    row_gap: Val::Px(4.0),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.005, 0.012, 0.025, 0.85)),
                BorderColor(Color::srgba(0.20, 0.50, 0.80, 0.40)),
            )).with_children(|info| {
                info.spawn((
                    LoadingScreenText,
                    Text::new("VESSEL: FEDERAL CORVETTE [TRUE SCALE 168M]"),
                    TextFont { font: font_mono.clone(), font_size: 9.5, ..default() },
                    TextColor(Color::srgb(0.35, 0.80, 0.95)),
                ));
                info.spawn((
                    LoadingScreenText,
                    Text::new("LOCATION: LOW EARTH ORBIT (3,500 KM ALTITUDE)"),
                    TextFont { font: font_mono.clone(), font_size: 9.5, ..default() },
                    TextColor(Color::srgb(0.35, 0.80, 0.95)),
                ));
                info.spawn((
                    LoadingScreenText,
                    Text::new("SYSTEMS: RELATIVISTIC ABERRATION & LORENTZ ENGINE READY"),
                    TextFont { font: font_mono.clone(), font_size: 9.5, ..default() },
                    TextColor(Color::srgb(0.40, 0.90, 0.65)),
                ));
            });

            // Progress Bar Outer Track
            panel.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(14.0),
                    border: UiRect::all(Val::Px(1.0)),
                    padding: UiRect::all(Val::Px(1.5)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.015, 0.04, 0.08, 0.90)),
                BorderColor(Color::srgba(0.25, 0.70, 1.0, 0.55)),
            )).with_children(|bar_outer| {
                bar_outer.spawn((
                    LoadingProgressBarFill,
                    Node {
                        width: Val::Percent(0.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.25, 0.85, 1.0)),
                ));
            });

            // Status Row
            panel.spawn((
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                },
            )).with_children(|row| {
                row.spawn((
                    LoadingStatusText,
                    LoadingScreenText,
                    Text::new("ACQUIRING ORBITAL TELEMETRY..."),
                    TextFont {
                        font: font_mono.clone(),
                        font_size: 10.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.70, 0.85, 1.0)),
                ));

                row.spawn((
                    LoadingPercentageText,
                    LoadingScreenText,
                    Text::new("0%"),
                    TextFont {
                        font: font_mono.clone(),
                        font_size: 10.5,
                        ..default()
                    },
                    TextColor(Color::srgb(0.35, 0.90, 1.0)),
                ));
            });
        });
    });
}

/// Dynamically tracks asset loading, GLTF mesh instantiation, updates progress bar, and smoothly fades out
pub fn update_loading_screen_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    mut tracker: ResMut<StartupLoadingTracker>,
    ship_scene_query: Query<&Children, With<crate::world::ShipSceneRoot>>,
    mut root_query: Query<(Entity, &mut BackgroundColor), With<LoadingScreenRoot>>,
    mut panel_query: Query<(&mut BackgroundColor, &mut BorderColor), (With<LoadingScreenPanel>, Without<LoadingScreenRoot>)>,
    mut bar_fill_query: Query<(&mut Node, &mut BackgroundColor), (With<LoadingProgressBarFill>, Without<LoadingScreenRoot>, Without<LoadingScreenPanel>)>,
    mut status_text_query: Query<&mut Text, (With<LoadingStatusText>, Without<LoadingPercentageText>)>,
    mut pct_text_query: Query<&mut Text, (With<LoadingPercentageText>, Without<LoadingStatusText>)>,
    mut text_color_query: Query<&mut TextColor, With<LoadingScreenText>>,
) {
    let Ok((root_entity, mut root_bg)) = root_query.get_single_mut() else {
        return; // Already despawned once loading was complete
    };

    let dt = time.delta_secs();
    tracker.elapsed += dt;

    // 1. Tally loaded assets
    let total_assets = tracker.tracked_assets.len();
    let mut loaded_count = 0usize;
    let mut active_asset_name: Option<String> = None;

    for asset in &tracker.tracked_assets {
        let state = asset_server.load_state(asset.id);
        if matches!(state, LoadState::Loaded) || matches!(state, LoadState::Failed(_)) {
            loaded_count += 1;
        } else if active_asset_name.is_none() {
            active_asset_name = Some(asset.label.clone());
        }
    }

    // 2. Check if Federal Corvette GLTF airframe has instantiated child meshes
    let ship_mesh_instantiated = ship_scene_query.iter().any(|children| !children.is_empty());

    // 3. Compute overall progress:
    let total_steps = total_assets + 1;
    let completed_steps = loaded_count + if ship_mesh_instantiated { 1 } else { 0 };
    let pct = if total_steps > 0 {
        ((completed_steps as f32 / total_steps as f32) * 100.0).clamp(0.0, 100.0)
    } else {
        100.0
    };

    // 4. Update Status and Percentage Texts
    let all_assets_ready = loaded_count == total_assets;
    let is_all_ready = (all_assets_ready && ship_mesh_instantiated && tracker.elapsed >= tracker.min_display_time)
        || tracker.elapsed >= 6.0; // 6-second fallback to prevent any permanent hang

    for mut text in status_text_query.iter_mut() {
        if is_all_ready {
            **text = "ALL SYSTEMS NOMINAL // WARPING COCKPIT ONLINE".to_string();
        } else if !all_assets_ready {
            if let Some(ref name) = active_asset_name {
                **text = format!("LOADING: {} ({}/{})", name.to_uppercase(), loaded_count, total_assets);
            } else {
                **text = format!("UPLOADING TEXTURES TO GPU ({}/{})", loaded_count, total_assets);
            }
        } else if !ship_mesh_instantiated {
            **text = "ASSEMBLING CORVETTE AIRFRAME MESH...".to_string();
        }
    }

    for mut text in pct_text_query.iter_mut() {
        **text = format!("{:.0}%", pct);
    }

    // 5. Update Progress Bar Width and Color
    for (mut node, mut bg) in bar_fill_query.iter_mut() {
        node.width = Val::Percent(pct);
        if is_all_ready {
            bg.0 = Color::srgb(0.35, 1.0, 0.65); // Radiant emerald/cyan when ready
        }
    }

    // 6. Handle Warm-up & Smooth Fade-Out
    if is_all_ready {
        if tracker.warm_up_frames < 3 {
            tracker.warm_up_frames += 1;
        } else {
            tracker.is_loaded = true;
        }
    }

    if tracker.is_loaded {
        tracker.opacity = (tracker.opacity - dt * 2.5).max(0.0);

        if tracker.opacity <= 0.0 {
            commands.entity(root_entity).despawn_recursive();
            return;
        }

        // Apply smooth fading to all loading screen elements
        root_bg.0 = Color::srgba(0.006, 0.010, 0.020, tracker.opacity);

        for (mut p_bg, mut p_border) in panel_query.iter_mut() {
            p_bg.0 = Color::srgba(0.010, 0.020, 0.040, 0.98 * tracker.opacity);
            p_border.0 = Color::srgba(0.25, 0.75, 1.0, 0.70 * tracker.opacity);
        }

        for mut text_color in text_color_query.iter_mut() {
            text_color.0 = match text_color.0 {
                Color::Srgba(c) => Color::srgba(c.red, c.green, c.blue, tracker.opacity),
                Color::LinearRgba(c) => Color::srgba(c.red, c.green, c.blue, tracker.opacity),
                other => other,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beta_progress_and_color_math() {
        let beta_low = 0.25;
        let beta_pct = (beta_low * 100.0) as f32;
        assert!((beta_pct - 25.0).abs() < 1e-4);

        let beta_med = 0.65;
        let beta_pct_med = (beta_med * 100.0) as f32;
        assert!((beta_pct_med - 65.0).abs() < 1e-4);

        let beta_high = 0.99;
        let beta_pct_high = (beta_high * 100.0) as f32;
        assert!((beta_pct_high - 99.0).abs() < 1e-4);
    }

    #[test]
    fn test_gamma_dilation_alert_thresholds() {
        // γ = 1.0 (Rest)
        let g1 = 1.000;
        assert!(g1 < 1.10);

        // γ = 1.5 (Moderate)
        let g2 = 1.500;
        assert!(g2 >= 1.10 && g2 < 2.00);

        // γ = 3.5 (High dilation >2x)
        let g3 = 3.500;
        assert!(g3 >= 2.00 && g3 < 5.00);

        // γ = 7.5 (Critical dilation >5x)
        let g4 = 7.500;
        assert!(g4 >= 5.00 && g4 < 10.00);

        // γ = 15.0 (Temporal anomaly >10x)
        let g5 = 15.000;
        assert!(g5 >= 10.00);
    }

    #[test]
    fn test_aberration_cone_aperture_math() {
        // At rest (β = 0.0), full hemisphere opening = 180°
        let beta_0: f64 = 0.0;
        let total_cone_0 = 2.0 * beta_0.acos().to_degrees();
        assert!((total_cone_0 - 180.0).abs() < 1e-3);

        // At β = 0.5, total cone = 120°
        let beta_half: f64 = 0.5;
        let total_cone_half = 2.0 * beta_half.acos().to_degrees();
        assert!((total_cone_half - 120.0).abs() < 1e-3);

        // At β = 0.8660254 (cos(30°)), total cone = 60°
        let beta_sqrt3_2: f64 = 3.0_f64.sqrt() / 2.0;
        let total_cone_60 = 2.0 * beta_sqrt3_2.acos().to_degrees();
        assert!((total_cone_60 - 60.0).abs() < 1e-3);

        // At β = 0.99, cone is narrow searchlight beam (< 20°)
        let beta_99: f64 = 0.99;
        let total_cone_99 = 2.0 * beta_99.acos().to_degrees();
        assert!(total_cone_99 < 20.0);
    }

    #[test]
    fn test_calculate_relativistic_beam_alignment() {
        use super::*;

        let cam_fwd = DVec3::NEG_Z;
        let cam_right = DVec3::X;
        let cam_up = DVec3::Y;

        // 1. Rest / low speed
        let align_rest = calculate_relativistic_beam_alignment(cam_fwd, cam_right, cam_up, DVec3::ZERO, 0.0);
        assert_eq!(align_rest, RelativisticBeamAlignment::Forward { off_center_deg: 0.0 });

        // 2. Moving forward along camera look vector
        let v_fwd = DVec3::NEG_Z * 4000.0;
        let align_fwd = calculate_relativistic_beam_alignment(cam_fwd, cam_right, cam_up, v_fwd, 0.8);
        match align_fwd {
            RelativisticBeamAlignment::Forward { off_center_deg } => {
                assert!(off_center_deg < 1.0);
            }
            _ => panic!("Expected Forward alignment"),
        }

        // 3. Moving backward (reverse thrust)
        let v_aft = DVec3::Z * 4000.0;
        let align_aft = calculate_relativistic_beam_alignment(cam_fwd, cam_right, cam_up, v_aft, 0.8);
        match align_aft {
            RelativisticBeamAlignment::Aft { off_center_deg } => {
                assert!((off_center_deg - 180.0).abs() < 1.0);
            }
            _ => panic!("Expected Aft alignment"),
        }

        // 4. Strafing left (PORT)
        let v_port = DVec3::NEG_X * 4000.0;
        let align_port = calculate_relativistic_beam_alignment(cam_fwd, cam_right, cam_up, v_port, 0.8);
        match align_port {
            RelativisticBeamAlignment::Lateral { angle_deg, flank } => {
                assert!((angle_deg - 90.0).abs() < 1.0);
                assert_eq!(flank, "PORT");
            }
            _ => panic!("Expected Lateral PORT alignment"),
        }

        // 5. Strafing right (STBD)
        let v_stbd = DVec3::X * 4000.0;
        let align_stbd = calculate_relativistic_beam_alignment(cam_fwd, cam_right, cam_up, v_stbd, 0.8);
        match align_stbd {
            RelativisticBeamAlignment::Lateral { angle_deg, flank } => {
                assert!((angle_deg - 90.0).abs() < 1.0);
                assert_eq!(flank, "STBD");
            }
            _ => panic!("Expected Lateral STBD alignment"),
        }

        // 6. Thrusting upward (DORSAL)
        let v_dorsal = DVec3::Y * 4000.0;
        let align_dorsal = calculate_relativistic_beam_alignment(cam_fwd, cam_right, cam_up, v_dorsal, 0.8);
        match align_dorsal {
            RelativisticBeamAlignment::Lateral { angle_deg, flank } => {
                assert!((angle_deg - 90.0).abs() < 1.0);
                assert_eq!(flank, "DORSAL");
            }
            _ => panic!("Expected Lateral DORSAL alignment"),
        }
    }

    #[test]
    fn test_settings_overlay_default_and_toggle() {
        let mut overlay = SettingsOverlayState::default();
        assert!(!overlay.visible);
        assert!(!overlay.just_closed);
        overlay.visible = !overlay.visible;
        assert!(overlay.visible);
        overlay.visible = !overlay.visible;
        assert!(!overlay.visible);
    }

    #[test]
    fn test_acceleration_and_g_force_formatting() {
        let format_accel = |a: f64| -> String {
            let g = a / 9.80665;
            if a <= 0.0 {
                "0.0 m/s² (0.0 g) [INERTIAL]".to_string()
            } else if a >= 1_000_000.0 {
                format!("{:.1}M m/s² ({:.2}M g)", a / 1_000_000.0, g / 1_000_000.0)
            } else if a >= 1_000.0 {
                format!("{:.1} km/s² ({:.1} g)", a / 1_000.0, g)
            } else {
                format!("{:.1} m/s² ({:.1} g)", a, g)
            }
        };

        assert_eq!(format_accel(0.0), "0.0 m/s² (0.0 g) [INERTIAL]");
        assert_eq!(format_accel(120.0), "120.0 m/s² (12.2 g)");
        assert_eq!(format_accel(600.0), "600.0 m/s² (61.2 g)");
        assert_eq!(format_accel(2_000.0), "2.0 km/s² (203.9 g)");
        assert_eq!(format_accel(25_000_000.0), "25.0M m/s² (2.55M g)");
    }

    #[test]
    fn test_settings_overlay_escape_dismiss_flag() {
        let mut overlay = SettingsOverlayState {
            visible: true,
            just_closed: false,
        };
        // Simulating Escape key press handling:
        overlay.visible = false;
        overlay.just_closed = true;

        assert!(!overlay.visible);
        assert!(overlay.just_closed);
    }

    #[test]
    fn test_startup_loading_tracker_progress_computation() {
        let mut tracker = StartupLoadingTracker::default();
        assert!(!tracker.is_loaded);
        assert_eq!(tracker.opacity, 1.0);

        // Empty tracker defaults to 100%
        let total_steps = tracker.tracked_assets.len() + 1;
        let completed_steps = 0usize;
        let pct = (completed_steps as f32 / total_steps as f32) * 100.0;
        assert_eq!(pct, 0.0);

        // 3 dummy assets + 1 ship mesh step = 4 total steps
        let dummy_id = AssetId::<Image>::default().untyped();
        tracker.tracked_assets.push(TrackedLoadingAsset {
            label: "Earth Map".to_string(),
            id: dummy_id,
        });
        tracker.tracked_assets.push(TrackedLoadingAsset {
            label: "Moon Map".to_string(),
            id: dummy_id,
        });
        tracker.tracked_assets.push(TrackedLoadingAsset {
            label: "Sun Map".to_string(),
            id: dummy_id,
        });

        let total = tracker.tracked_assets.len() + 1; // 3 + 1 = 4
        assert_eq!(total, 4);

        // 2 assets loaded, ship mesh not loaded -> 2/4 = 50%
        let completed = 2usize;
        let pct = (completed as f32 / total as f32) * 100.0;
        assert_eq!(pct, 50.0);

        // 3 assets loaded, ship mesh loaded -> 4/4 = 100%
        let completed_all = 4usize;
        let pct_all = (completed_all as f32 / total as f32) * 100.0;
        assert_eq!(pct_all, 100.0);
    }

    #[test]
    fn test_startup_loading_completion_and_fade() {
        let mut tracker = StartupLoadingTracker::default();
        tracker.is_loaded = true;
        let dt = 0.1f32; // 100ms step
        let fade_speed = 2.5f32; // fades over 0.4s

        tracker.opacity = (tracker.opacity - dt * fade_speed).max(0.0);
        assert!((tracker.opacity - 0.75).abs() < 1e-4);

        // After 4 steps (0.4s total), opacity reaches 0.0
        tracker.opacity = (tracker.opacity - 0.3 * fade_speed).max(0.0);
        assert_eq!(tracker.opacity, 0.0);
    }

    #[test]
    fn test_hud_display_mode_cycling_and_hide_toggle() {
        let mut state = HudDisplayState::default();
        assert_eq!(state.mode, HudDisplayMode::Full);

        // I cycles Full -> Minimal -> Hidden -> Full
        state.cycle();
        assert_eq!(state.mode, HudDisplayMode::Minimal);
        state.cycle();
        assert_eq!(state.mode, HudDisplayMode::Hidden);
        state.cycle();
        assert_eq!(state.mode, HudDisplayMode::Full);

        // Shift+I toggles Full <-> Hidden
        state.toggle_hidden();
        assert_eq!(state.mode, HudDisplayMode::Hidden);
        state.toggle_hidden();
        assert_eq!(state.mode, HudDisplayMode::Full);

        // Switch to Minimal, then Shift+I toggles Minimal <-> Hidden
        state.cycle();
        assert_eq!(state.mode, HudDisplayMode::Minimal);
        state.toggle_hidden();
        assert_eq!(state.mode, HudDisplayMode::Hidden);
        state.toggle_hidden();
        assert_eq!(state.mode, HudDisplayMode::Minimal);
    }

    #[test]
    fn test_hud_display_visibility_transitions() {
        let mut app = App::new();
        app.insert_resource(HudDisplayState::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(RelativisticConfig::default());
        app.insert_resource(SimulationPause(false));
        app.insert_resource(FpsTracker::default());
        app.insert_resource(crate::navigation::NavigationComputer::default());

        app.world_mut().spawn((Shuttle::default(), RelativisticState::default()));
        let full_ent = app.world_mut().spawn((FullHudElement, Node::default())).id();
        let min_root_ent = app.world_mut().spawn((MinimalHudRoot, Node { display: Display::None, ..default() })).id();
        let min_tgt_box_ent = app.world_mut().spawn((MinimalTargetBox, Node::default())).id();
        let toast_ent = app.world_mut().spawn((StartupCreditsToast { time_remaining: 10.0 }, Node::default())).id();
        app.world_mut().spawn((MinimalKinematicsText, Text::new("")));
        app.world_mut().spawn((MinimalTargetText, Text::new("")));
        app.world_mut().spawn((MinimalMetricsText, Text::new("")));

        app.add_systems(Update, (hud_display_input_system, hud_display_update_system).chain());

        // 1. Default Full mode
        app.update();
        assert_eq!(app.world().get::<Node>(full_ent).unwrap().display, Display::Flex);
        assert_eq!(app.world().get::<Node>(min_root_ent).unwrap().display, Display::None);
        assert_eq!(app.world().get::<Node>(toast_ent).unwrap().display, Display::Flex);

        // 2. Press I -> Minimal mode (Full hidden, Minimal visible, Target box hidden when no target)
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyI);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear_just_pressed(KeyCode::KeyI);
        assert_eq!(app.world().resource::<HudDisplayState>().mode, HudDisplayMode::Minimal);
        assert_eq!(app.world().get::<Node>(full_ent).unwrap().display, Display::None);
        assert_eq!(app.world().get::<Node>(min_root_ent).unwrap().display, Display::Flex);
        assert_eq!(app.world().get::<Node>(min_tgt_box_ent).unwrap().display, Display::None);

        // 3. Press I again -> Hidden mode (Full, Minimal, and Toast all hidden)
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::KeyI);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyI);
        app.update();
        assert_eq!(app.world().resource::<HudDisplayState>().mode, HudDisplayMode::Hidden);
        assert_eq!(app.world().get::<Node>(full_ent).unwrap().display, Display::None);
        assert_eq!(app.world().get::<Node>(min_root_ent).unwrap().display, Display::None);
        assert_eq!(app.world().get::<Node>(toast_ent).unwrap().display, Display::None);

        // 4. Press Shift+I -> restores Minimal mode (the last visible mode)
        {
            let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            kb.release(KeyCode::KeyI);
            kb.press(KeyCode::ShiftLeft);
            kb.press(KeyCode::KeyI);
        }
        app.update();
        assert_eq!(app.world().resource::<HudDisplayState>().mode, HudDisplayMode::Minimal);
        assert_eq!(app.world().get::<Node>(min_root_ent).unwrap().display, Display::Flex);
    }
}
