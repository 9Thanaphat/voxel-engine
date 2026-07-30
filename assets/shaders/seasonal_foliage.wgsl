#import bevy_pbr::forward_io::VertexOutput

struct SeasonalFoliageUniforms {
    daylight: vec4<f32>,
    tuning: vec4<f32>,
};

@group(3) @binding(0)
var<uniform> material: SeasonalFoliageUniforms;
@group(3) @binding(1)
var foliage_texture: texture_2d<f32>;
@group(3) @binding(2)
var foliage_sampler: sampler;

fn oak_palette(day: f32) -> vec3<f32> {
    let spring = vec3<f32>(0.471, 0.722, 0.290);
    let summer = vec3<f32>(0.247, 0.561, 0.212);
    // Yellow-gold rather than orange-brown.
    let autumn = vec3<f32>(0.960, 0.760, 0.080);
    let winter = vec3<f32>(0.435, 0.333, 0.216);
    let d = day - floor(day / 365.0) * 365.0;

    if d < 91.0 {
        return spring;
    } else if d < 182.0 {
        return summer;
    } else if d < 274.0 {
        return autumn;
    } else {
        return winter;
    }
}

fn maple_palette(day: f32, variation: f32) -> vec3<f32> {
    let spring = vec3<f32>(0.560, 0.760, 0.250);
    let summer = vec3<f32>(0.200, 0.520, 0.160);
    let winter = vec3<f32>(0.380, 0.220, 0.130);
    let d = day - floor(day / 365.0) * 365.0;

    if d < 91.0 {
        return spring;
    } else if d < 182.0 {
        return summer;
    } else if d < 274.0 {
        // Each tree gets one full autumn color: gold, orange, or red.
        if variation < 0.333 {
            return vec3<f32>(0.980, 0.720, 0.060);
        } else if variation < 0.666 {
            return vec3<f32>(0.950, 0.330, 0.060);
        }
        return vec3<f32>(0.820, 0.080, 0.045);
    }
    return winter;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(foliage_texture, foliage_sampler, in.uv);
    if texel.a < 0.5 {
        discard;
    }
    let effective_day = material.tuning.x + in.uv_b.x;
    let variation = fract(sin(in.uv_b.x * 12.9898) * 43758.5453);
    var seasonal = oak_palette(effective_day);
    if material.tuning.y > 0.5 {
        seasonal = maple_palette(effective_day, variation);
    }
    // Oak foliage uses the seasonal palette directly; biome green must not tint it.
    let foliage = seasonal;
    let rgb = texel.rgb * foliage * in.color.a * material.daylight.rgb;
    return vec4<f32>(rgb, texel.a);
}
