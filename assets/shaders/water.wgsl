#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::globals
#import bevy_pbr::mesh_view_bindings::view

struct WaterUniforms {
    color: vec4<f32>,
    shallow_color: vec4<f32>,
    deep_color: vec4<f32>,
    reflection_color: vec4<f32>,
    sun_dir: vec3<f32>,
    tuning: vec4<f32>,
};

@group(3) @binding(0)
var<uniform> material: WaterUniforms;

fn hash21(p: vec2<f32>) -> f32 {
    let p3 = fract(vec3<f32>(p.xyx) * 0.1031);
    let q = p3 + dot(p3, p3.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash21(i), hash21(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash21(i + vec2<f32>(0.0, 1.0)), hash21(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y
    );
}

fn detail_noise(p: vec2<f32>) -> f32 {
    return value_noise(p) * 0.65 + value_noise(p * 2.07 + 17.3) * 0.35;
}

// Two repeating phases cross-fade halfway through the cycle. This keeps flow
// moving indefinitely without the long texture stretch and visible snap.
fn flowed_noise(p: vec2<f32>, direction: vec2<f32>, phase: f32) -> f32 {
    let a = fract(phase);
    let b = fract(phase + 0.5);
    let blend = abs(a * 2.0 - 1.0);
    let na = detail_noise(p - direction * a);
    let nb = detail_noise(p - direction * b + vec2<f32>(11.7, -4.2));
    return mix(na, nb, blend);
}

fn safe_normalize(v: vec2<f32>, fallback: vec2<f32>) -> vec2<f32> {
    let len = length(v);
    return select(fallback, v / len, len > 0.0001);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let time = globals.time;
    let world_pos = in.world_position.xyz;
    let geometric_normal = normalize(in.world_normal);
    let is_waterfall = abs(geometric_normal.y) < 0.5;

    let raw_speed = length(in.uv);
    let flow_speed = clamp(sqrt(raw_speed) * material.tuning.x, 0.0, 2.4);
    let river_dir = safe_normalize(in.uv, vec2<f32>(0.72, 0.69));
    let phase = time * (0.08 + flow_speed * 0.42);

    var surface_p: vec2<f32>;
    var direction: vec2<f32>;
    var tangent: vec3<f32>;
    var bitangent: vec3<f32>;
    if is_waterfall {
        surface_p = vec2<f32>(world_pos.x + world_pos.z, world_pos.y);
        direction = vec2<f32>(river_dir.x * 0.18, -1.0);
        tangent = normalize(vec3<f32>(geometric_normal.z, 0.0, -geometric_normal.x));
        bitangent = vec3<f32>(0.0, 1.0, 0.0);
    } else {
        surface_p = vec2<f32>(
            dot(world_pos.xz, river_dir),
            dot(world_pos.xz, vec2<f32>(-river_dir.y, river_dir.x))
        );
        direction = vec2<f32>(1.0, 0.0);
        tangent = normalize(vec3<f32>(river_dir.x, 0.0, river_dir.y));
        bitangent = normalize(cross(geometric_normal, tangent));
    }

    let stretch = select(vec2<f32>(1.35, 4.2), vec2<f32>(0.85, 5.5), is_waterfall);
    let p = surface_p * stretch;
    let main_wave = flowed_noise(p, direction * (2.1 + flow_speed), phase);
    let detail_wave = flowed_noise(
        p * 1.9 + vec2<f32>(3.1, 8.7),
        safe_normalize(direction + vec2<f32>(0.17, 0.11), direction) * (1.2 + flow_speed),
        phase * 1.37
    );

    let eps = 0.055;
    let wave_x = flowed_noise(p + vec2<f32>(eps, 0.0), direction * (2.1 + flow_speed), phase);
    let wave_y = flowed_noise(p + vec2<f32>(0.0, eps), direction * (2.1 + flow_speed), phase);
    let wave_strength = material.tuning.y * select(1.0, 1.55, is_waterfall);
    let gradient = vec2<f32>(wave_x - main_wave, wave_y - main_wave) / eps;
    let normal = normalize(
        geometric_normal - tangent * gradient.x * wave_strength - bitangent * gradient.y * wave_strength
    );

    let depth = clamp(in.uv_b.x, 0.0, 1.0);
    let shoreline = clamp(in.uv_b.y, 0.0, 1.0);
    let view_dir = normalize(view.world_position.xyz - world_pos);
    let n_dot_v = clamp(dot(normal, view_dir), 0.0, 1.0);
    let fresnel = pow(1.0 - n_dot_v, select(4.2, 2.2, is_waterfall));

    // The moving light/dark variation is a cheap refraction/absorption cue.
    // It intentionally avoids sampling scene color, keeping this compatible
    // with Bevy's transparent forward pass.
    let caustic = smoothstep(0.50, 0.88, main_wave * 0.65 + detail_wave * 0.35);
    let absorption = smoothstep(0.02, 0.92, depth);
    var water_rgb = mix(material.shallow_color.rgb, material.deep_color.rgb, absorption);
    water_rgb *= in.color.rgb * material.color.rgb;
    water_rgb += material.shallow_color.rgb * caustic * (1.0 - depth) * 0.18;

    let fast_foam = smoothstep(0.65, 1.65, flow_speed);
    let broken_wave = smoothstep(0.60, 0.86, main_wave * 0.55 + detail_wave * 0.45);
    let edge_foam = shoreline * smoothstep(0.38, 0.72, detail_wave);
    let fall_foam = select(0.0, 0.35 + fast_foam * 0.45, is_waterfall);
    let foam = clamp((edge_foam + fast_foam * broken_wave * 0.55 + fall_foam * broken_wave) * material.tuning.z, 0.0, 1.0);

    let sun_dir = normalize(material.sun_dir);
    let half_vector = normalize(sun_dir + view_dir);
    let sun_visible = smoothstep(-0.08, 0.12, sun_dir.y);
    let glint_mask = smoothstep(0.62, 0.91, detail_wave);
    let specular = pow(max(dot(normal, half_vector), 0.0), 180.0)
        * sun_visible * (0.35 + glint_mask * 2.2);

    let reflected = material.reflection_color.rgb * material.color.rgb;
    var out_rgb = mix(water_rgb, reflected, fresnel * material.tuning.w);
    out_rgb = mix(out_rgb, vec3<f32>(0.88, 0.96, 1.0), foam * 0.82);
    out_rgb += vec3<f32>(specular);

    let depth_alpha = mix(material.shallow_color.a, material.deep_color.a, absorption);
    let fall_alpha = select(0.0, 0.18, is_waterfall);
    let out_alpha = clamp(depth_alpha * in.color.a / 0.4 + fresnel * 0.22 + foam * 0.28 + fall_alpha, 0.08, 0.94);
    return vec4<f32>(out_rgb, out_alpha);
}
