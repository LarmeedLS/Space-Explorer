#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
}

struct StarfieldUniform {
    rapidity: vec3<f32>,
    beta: f32,
    beta_vec: vec3<f32>,
    gamma: f32,
    sphere_radius: f32,
    aberration_enabled: u32,
    doppler_enabled: u32,
    visor_mode: u32,
};

@group(2) @binding(0) var<uniform> uniforms: StarfieldUniform;

struct StarVertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>, // x: offset_x, y: offset_y, z: brightness
    @location(1) normal: vec3<f32>,   // rest unit direction n_0
    @location(2) uv: vec2<f32>,       // billboard quad uv [-1, 1]
    @location(5) color: vec4<f32>,    // rgb: rest blackbody color, a: rest temperature in Kelvin
};

struct StarVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

fn to_linear(c: f32) -> f32 {
    if (c <= 0.04045) {
        return c / 12.92;
    } else {
        return pow((c + 0.055) / 1.055, 2.4);
    }
}

// Analytical Planckian blackbody chromaticity fit with astrophotographic saturation boost
fn blackbody_temperature_to_linear_rgb(temp_kelvin: f32) -> vec3<f32> {
    if (temp_kelvin <= 300.0) {
        return vec3<f32>(0.0);
    }

    let t = clamp(temp_kelvin / 100.0, 3.0, 1000.0);

    var r_srgb = 1.0;
    if (t > 66.0) {
        r_srgb = clamp(329.698727446 * pow(t - 60.0, -0.1332047592) / 255.0, 0.0, 1.0);
    }

    var g_srgb = 0.0;
    if (t <= 66.0) {
        g_srgb = clamp((99.4708025861 * log(t) - 161.1195681661) / 255.0, 0.0, 1.0);
    } else {
        g_srgb = clamp((288.1221695283 * pow(t - 60.0, -0.0755148492)) / 255.0, 0.0, 1.0);
    }

    var b_srgb = 1.0;
    if (t <= 19.0) {
        b_srgb = 0.0;
    } else if (t < 66.0) {
        b_srgb = clamp((138.5177312231 * log(t - 10.0) - 305.0447927307) / 255.0, 0.0, 1.0);
    }

    var infrared_fade = 1.0;
    if (temp_kelvin < 1800.0) {
        let x = clamp(temp_kelvin / 1800.0, 0.0, 1.0);
        infrared_fade = x * x * (3.0 - 2.0 * x);
    }

    let r_lin = to_linear(r_srgb);
    let g_lin = to_linear(g_srgb);
    let b_lin = to_linear(b_srgb);

    // Astrophotographic saturation boost: prevents stars from washing out to pure white under bloom and tonemapping
    let y = 0.2126 * r_lin + 0.7152 * g_lin + 0.0722 * b_lin;
    let sat = 2.0;
    let r_sat = max(0.0, y + sat * (r_lin - y));
    let g_sat = max(0.0, y + sat * (g_lin - y));
    let b_sat = max(0.0, y + sat * (b_lin - y));
    let peak = max(r_sat, max(g_sat, max(b_sat, 1e-4)));
    let peak_orig = max(r_lin, max(g_lin, b_lin));
    let norm = peak_orig / peak;

    return vec3<f32>(r_sat * norm * infrared_fade, g_sat * norm * infrared_fade, b_sat * norm * infrared_fade);
}

// Analytical relativistic aberration using rapidity half-angle formulation:
// tan(theta'/2) = exp(-||zeta||) * tan(theta/2)
fn aberrate_direction_from_rapidity(dir: vec3<f32>, rapidity: vec3<f32>) -> vec3<f32> {
    let zeta = length(rapidity);
    if (zeta < 1e-6) {
        return normalize(dir);
    }

    let beta_hat = rapidity / zeta;
    let cos_theta = clamp(dot(dir, beta_hat), -1.0, 1.0);
    let theta = acos(cos_theta);

    if (theta < 1e-5) {
        return beta_hat;
    }
    if (theta > 3.14159265 - 1e-5) {
        return -beta_hat;
    }

    let half_theta = theta * 0.5;
    let exp_minus_zeta = exp(-zeta);
    let tan_half_prime = exp_minus_zeta * tan(half_theta);
    let theta_prime = 2.0 * atan(tan_half_prime);

    let n_perp = dir - beta_hat * cos_theta;
    let len_perp = length(n_perp);
    var u_perp = vec3<f32>(0.0);
    if (len_perp > 1e-6) {
        u_perp = n_perp / len_perp;
    }

    return normalize(beta_hat * cos(theta_prime) + u_perp * sin(theta_prime));
}

@vertex
fn vertex(in: StarVertexInput) -> StarVertexOutput {
    var out: StarVertexOutput;
    out.uv = in.uv;

    let n0 = normalize(in.normal);
    var obs_dir = n0;
    if (uniforms.aberration_enabled != 0u && length(uniforms.rapidity) > 1e-6) {
        obs_dir = aberrate_direction_from_rapidity(n0, uniforms.rapidity);
    }

    // Billboarding orientation perpendicular to observed ray direction
    var up_hint = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(obs_dir.y) > 0.95) {
        up_hint = vec3<f32>(1.0, 0.0, 0.0);
    }
    let right_vec = normalize(cross(obs_dir, up_hint));
    let up_vec = cross(right_vec, obs_dir);

    // Quad vertex offset: in.position.x = offset_x, in.position.y = offset_y, in.position.z = brightness
    let star_center = obs_dir * uniforms.sphere_radius;
    let local_pos = star_center + right_vec * in.position.x + up_vec * in.position.y;

    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    let world_pos = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(local_pos, 1.0));
    out.clip_position = position_world_to_clip(world_pos.xyz);
    // Pin starfield depth to the absolute infinite far plane (Z = 0.0 in Reversed-Z depth buffer)
    // so all physical 1:1 celestial bodies (even at 100 AU) strictly occlude background stars.
    out.clip_position.z = 0.0;

    // Relativistic Doppler shift, thermal blackbody radiation & relativistic beaming
    var shifted_col = in.color.rgb;
    var beaming = 1.0;
    let rest_temp = in.color.a;
    let brightness = in.position.z;

    if (uniforms.doppler_enabled != 0u && uniforms.beta > 1e-5) {
        let cos_angle = dot(obs_dir, uniforms.beta_vec);
        let denom = uniforms.gamma * (1.0 - cos_angle);
        var doppler = 1.0;
        if (abs(denom) > 1e-6) {
            doppler = clamp(1.0 / denom, 0.001, 20.0);
        }
        let shifted_temp = doppler * rest_temp;
        shifted_col = blackbody_temperature_to_linear_rgb(shifted_temp);
        beaming = clamp(doppler * doppler * doppler, 0.0, 10.0);
    }

    var visor_tint = 1.0;
    if (uniforms.visor_mode == 1u) {
        visor_tint = 0.85;
    } else if (uniforms.visor_mode == 2u) {
        visor_tint = 0.50;
    }

    let rgb = min(shifted_col * (beaming * brightness * visor_tint), vec3<f32>(60.0));
    out.color = vec4<f32>(rgb, 1.0);

    return out;
}

@fragment
fn fragment(in: StarVertexOutput) -> @location(0) vec4<f32> {
    // Smooth circular point-spread function (optical starlight point source)
    let dist_sq = dot(in.uv, in.uv);
    if (dist_sq > 1.0) {
        discard;
    }
    let falloff = exp(-1.5 * dist_sq) * (1.0 - dist_sq);
    return vec4<f32>(in.color.rgb * falloff, 1.0);
}
