#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::globals
#import bevy_pbr::mesh_view_bindings::view

struct WaterUniforms {
    color: vec4<f32>,
    sun_dir: vec3<f32>,
    _padding: f32,
};

@group(3) @binding(0)
var<uniform> material: WaterUniforms;

// Hash function for Voronoi
fn hash22(p: vec2<f32>) -> vec2<f32> {
    var p3 = fract(vec3<f32>(p.xyx) * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.xx + p3.yz) * p3.zy);
}

// Voronoi noise for organic ripples
fn voronoi(x: vec2<f32>) -> f32 {
    let p = floor(x);
    let f = fract(x);
    var res = 8.0;
    for(var j: i32 = -1; j <= 1; j = j + 1) {
        for(var i: i32 = -1; i <= 1; i = i + 1) {
            let b = vec2<f32>(f32(i), f32(j));
            let r = vec2<f32>(b) - f + hash22(p + b);
            let d = dot(r, r);
            res = min(res, d);
        }
    }
    return sqrt(res);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let flow = in.uv;
    var speed = length(flow);
    let time = globals.time;
    let world_pos = in.world_position.xyz;

    // Detect Waterfall Mode (vertical faces)
    let is_waterfall = abs(in.world_normal.y) < 0.5;
    
    // Base normal pointing out of the surface
    var normal = in.world_normal;
    var wave_intensity = 0.12;
    var foam_factor = 0.0;
    var combined_ripple = 0.0;
    
    // 1. Procedural Normal (Ripples)
    if speed > 0.001 {
        var dir1: vec2<f32>;
        var dir2: vec2<f32>;
        var wave_pos: vec2<f32>;
        
        if is_waterfall {
            // Waterfall: Flow down the Y axis mixed with X or Z
            let down_dir = vec2<f32>(0.0, -1.0);
            dir1 = down_dir;
            dir2 = normalize(vec2<f32>(0.2, -1.0)); // slight diagonal
            wave_pos = vec2<f32>(world_pos.x + world_pos.z, world_pos.y);
            speed *= 2.0; 
            wave_intensity = 0.25; 
        } else {
            // Horizontal Flow (River/Lake)
            dir1 = normalize(flow);
            dir2 = normalize(vec2<f32>(dir1.x * 0.8 + dir1.y * 0.6, -dir1.x * 0.6 + dir1.y * 0.8));
            wave_pos = world_pos.xz;
        }

        // Use Voronoi for organic ripples instead of cos()
        let uv1 = wave_pos * 2.0 - dir1 * (time * speed * 0.8);
        let uv2 = wave_pos * 3.5 - dir2 * (time * speed * 1.2) + vec2<f32>(time * 0.2);
        
        let v1 = voronoi(uv1);
        let v2 = voronoi(uv2);
        
        combined_ripple = 1.0 - ((v1 + v2) * 0.5); // 0.0 (dark) to 1.0 (bright ridges)
        
        let perturb_u = ((1.0 - v1) * 2.0 - 1.0) * wave_intensity;
        let perturb_v = ((1.0 - v2) * 2.0 - 1.0) * wave_intensity;

        // Perturb normal using tangent space approximation
        var tangent: vec3<f32>;
        var bitangent: vec3<f32>;
        if is_waterfall {
            tangent = vec3<f32>(0.0, 1.0, 0.0);
            bitangent = cross(normal, tangent);
        } else {
            tangent = vec3<f32>(1.0, 0.0, 0.0);
            bitangent = vec3<f32>(0.0, 0.0, 1.0);
        }
        
        normal = normalize(normal + tangent * perturb_u + bitangent * perturb_v);
    } else {
        // Still water (puddles / lakes without flow)
        let wave_pos = world_pos.xz;
        let uv1 = wave_pos * 1.5 - vec2<f32>(time * 0.05);
        let uv2 = wave_pos * 2.5 + vec2<f32>(time * 0.08);
        let v1 = voronoi(uv1);
        let v2 = voronoi(uv2);
        combined_ripple = 1.0 - ((v1 + v2) * 0.5);
        
        let perturb_u = ((1.0 - v1) * 2.0 - 1.0) * 0.03;
        let perturb_v = ((1.0 - v2) * 2.0 - 1.0) * 0.03;
        
        let tangent = vec3<f32>(1.0, 0.0, 0.0);
        let bitangent = vec3<f32>(0.0, 0.0, 1.0);
        normal = normalize(normal + tangent * perturb_u + bitangent * perturb_v);
    }

    // 2. View and Lighting Setup
    let view_dir = normalize(view.world_position.xyz - in.world_position.xyz);
    let sun_dir = normalize(material.sun_dir);

    // 3. Fresnel Effect
    let n_dot_v = max(dot(normal, view_dir), 0.0);
    let fresnel_power = select(2.5, 1.5, is_waterfall);
    let fresnel = pow(1.0 - n_dot_v, fresnel_power);

    // 4. Specular Highlight & Sun Glints
    let half_vector = normalize(sun_dir + view_dir);
    let n_dot_h = max(dot(normal, half_vector), 0.0);
    
    // Only show specular if the sun is above horizon
    let sun_intensity = smoothstep(-0.05, 0.1, sun_dir.y);
    
    // Break up the specular highlight using the ripple noise to create sparkling glints
    let glint = smoothstep(0.65, 1.0, combined_ripple);
    let specular = pow(n_dot_h, 250.0) * 2.5 * sun_intensity * (0.3 + 0.7 * glint);

    // 5. Final Color Composition
    // Darken base color slightly when looking straight down (depth illusion)
    let depth_darken = mix(0.7, 1.0, n_dot_v);
    var base_color = vec4<f32>(in.color.rgb * material.color.rgb * depth_darken, in.color.a * material.color.a);
    
    // Mix sky reflection (fresnel)
    let reflection_color = vec3<f32>(0.4, 0.65, 0.9) * material.color.rgb; // Sky tint
    let out_rgb = mix(base_color.rgb, reflection_color, fresnel * 0.7) + vec3<f32>(specular);
    
    // Alpha blending
    let base_alpha = select(base_color.a, base_color.a + 0.4, is_waterfall);
    let out_alpha = clamp(base_alpha + fresnel * 0.5, 0.0, 1.0);

    return vec4<f32>(out_rgb, out_alpha);
}
