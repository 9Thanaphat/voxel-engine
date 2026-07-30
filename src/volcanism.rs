//! Deterministic volcano and hydrothermal world-generation fields.
//!
//! The functions in this module are pure: the same world seed and coordinates
//! always produce the same landmark, regardless of chunk load order.

use bevy::{math::DVec2, prelude::*};

pub const VOLCANO_CELL_SIZE: i32 = 1_536;
pub const VOLCANO_CHANCE_PERCENT: u64 = 28;
pub const VOLCANO_MIN_RADIUS: f64 = 100.0;
pub const VOLCANO_MAX_RADIUS: f64 = 200.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolcanoDescriptor {
    pub id: u64,
    pub center: DVec2,
    pub radius: f64,
    pub height: f64,
    pub crater_radius: f64,
    pub seed: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VolcanoSample {
    pub descriptor: Option<VolcanoDescriptor>,
    /// Height added to the pre-volcano terrain.
    pub elevation: f64,
    /// 0 outside the cone, 1 at the centre.
    pub cone: f64,
    /// 0 outside the crater, 1 at the centre.
    pub crater: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HydrothermalSample {
    pub volcano: Option<VolcanoDescriptor>,
    /// Strength of altered ground/fumaroles.
    pub altered: f64,
    /// Horizontal pool mask, 0 outside and 1 at the pool centre.
    pub pool: f64,
    /// Number of blocks carved below the surrounding surface.
    pub pool_depth: i32,
}

#[inline]
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[inline]
fn cell_hash(seed: u32, cx: i32, cz: i32) -> u64 {
    mix64(
        seed as u64
            ^ (cx as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ (cz as i64 as u64).wrapping_mul(0xc2b2_ae3d_27d_4eb4f),
    )
}

#[inline]
fn unit(hash: u64) -> f64 {
    (hash >> 11) as f64 / ((1u64 << 53) as f64)
}

fn candidate(seed: u32, cx: i32, cz: i32) -> Option<VolcanoDescriptor> {
    let h = cell_hash(seed, cx, cz);
    if h % 100 >= VOLCANO_CHANCE_PERCENT {
        return None;
    }
    // Keep candidates away from cell edges. This makes the 3x3 lookup enough
    // even for the largest cone while retaining broad irregular spacing.
    let inset = VOLCANO_MAX_RADIUS + 32.0;
    let span = VOLCANO_CELL_SIZE as f64 - inset * 2.0;
    let x = cx as f64 * VOLCANO_CELL_SIZE as f64 + inset + unit(mix64(h ^ 1)) * span;
    let z = cz as f64 * VOLCANO_CELL_SIZE as f64 + inset + unit(mix64(h ^ 2)) * span;
    let radius = VOLCANO_MIN_RADIUS
        + unit(mix64(h ^ 3)) * (VOLCANO_MAX_RADIUS - VOLCANO_MIN_RADIUS);
    let height = 100.0 + unit(mix64(h ^ 4)) * 80.0;
    let crater_radius = radius * (0.13 + unit(mix64(h ^ 5)) * 0.07);
    Some(VolcanoDescriptor {
        id: h,
        center: DVec2::new(x, z),
        radius,
        height,
        crater_radius,
        seed: mix64(h ^ 0xa076_1d64_78bd_642f),
    })
}

/// Returns deterministic volcano candidates around a world position.
///
/// Terrain-specific callers should still filter these through
/// `TerrainSampler::volcano_sample`, because ocean candidates are suppressed
/// by the terrain generator.
pub fn volcanoes_nearby(
    seed: u32,
    wx: f64,
    wz: f64,
    cell_radius: i32,
) -> Vec<VolcanoDescriptor> {
    let cx = (wx.floor() as i32).div_euclid(VOLCANO_CELL_SIZE);
    let cz = (wz.floor() as i32).div_euclid(VOLCANO_CELL_SIZE);
    let mut result = Vec::new();
    for dz in -cell_radius..=cell_radius {
        for dx in -cell_radius..=cell_radius {
            if let Some(volcano) = candidate(seed, cx + dx, cz + dz) {
                result.push(volcano);
            }
        }
    }
    result
}

/// Centres of the sulfuric-acid pools associated with a volcano.
pub fn hydrothermal_pool_centers(volcano: VolcanoDescriptor) -> Vec<DVec2> {
    let cluster_count = 1 + (volcano.seed % 3) as usize;
    (0..cluster_count)
        .map(|i| {
            let h =
                mix64(volcano.seed ^ (i as u64 + 1).wrapping_mul(0xd6e8_feb8_6659_fd93));
            let angle = unit(h) * std::f64::consts::TAU;
            let flank_r =
                volcano.radius * (0.62 + unit(mix64(h ^ 1)) * 0.48);
            volcano.center + DVec2::new(angle.cos(), angle.sin()) * flank_r
        })
        .collect()
}

/// Volcano affecting a coordinate, including the outer altered-ground margin.
pub fn volcano_near(seed: u32, wx: f64, wz: f64) -> Option<VolcanoDescriptor> {
    let cx = (wx.floor() as i32).div_euclid(VOLCANO_CELL_SIZE);
    let cz = (wz.floor() as i32).div_euclid(VOLCANO_CELL_SIZE);
    let p = DVec2::new(wx, wz);
    let mut best: Option<(f64, VolcanoDescriptor)> = None;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let Some(v) = candidate(seed, cx + dx, cz + dz) else {
                continue;
            };
            let distance = p.distance(v.center);
            if distance <= v.radius * 1.3
                && best.is_none_or(|(best_distance, _)| distance < best_distance)
            {
                best = Some((distance, v));
            }
        }
    }
    best.map(|(_, v)| v)
}

/// Cone + crater height contribution.
pub fn sample_volcano(seed: u32, wx: f64, wz: f64) -> VolcanoSample {
    let Some(v) = volcano_near(seed, wx, wz) else {
        return VolcanoSample::default();
    };
    let p = DVec2::new(wx, wz);
    let d = p.distance(v.center);
    if d >= v.radius {
        return VolcanoSample { descriptor: Some(v), ..Default::default() };
    }

    let radial = (1.0 - d / v.radius).clamp(0.0, 1.0);
    let rough = {
        let angle = (wz - v.center.y).atan2(wx - v.center.x);
        let a = (angle * 7.0 + unit(v.seed) * 6.0).sin() * 0.035;
        let b = (angle * 13.0 + unit(mix64(v.seed)) * 6.0).sin() * 0.018;
        1.0 + (a + b) * (1.0 - radial)
    };
    let cone_height = v.height * radial.powf(1.22) * rough;
    let crater = (1.0 - d / v.crater_radius).clamp(0.0, 1.0);
    // Keep a high crater floor while cutting a readable rim into the summit.
    let crater_cut = v.height * 0.50 * crater.powf(1.35);
    VolcanoSample {
        descriptor: Some(v),
        elevation: (cone_height - crater_cut).max(0.0),
        cone: radial,
        crater,
    }
}

/// Hydrothermal pools occur on wet fractured flanks, not in the lava crater.
/// Humidity is supplied by the terrain sampler so the field follows climate.
pub fn sample_hydrothermal(
    seed: u32,
    wx: f64,
    wz: f64,
    humidity: f64,
) -> HydrothermalSample {
    if humidity < -0.20 {
        return HydrothermalSample::default();
    }
    let Some(v) = volcano_near(seed, wx, wz) else {
        return HydrothermalSample::default();
    };
    let p = DVec2::new(wx, wz);
    let d = p.distance(v.center);
    if d < v.radius * 0.52 || d > v.radius * 1.25 {
        return HydrothermalSample { volcano: Some(v), ..Default::default() };
    }

    let mut out = HydrothermalSample { volcano: Some(v), ..Default::default() };
    for (i, center) in hydrothermal_pool_centers(v).into_iter().enumerate() {
        let h = mix64(v.seed ^ (i as u64 + 1).wrapping_mul(0xd6e8_feb8_6659_fd93));
        let pool_radius = 4.0 + unit(mix64(h ^ 2)) * 8.0;
        let altered_radius = pool_radius * 2.4;
        let pd = p.distance(center);
        let altered = (1.0 - pd / altered_radius).clamp(0.0, 1.0);
        let pool = (1.0 - pd / pool_radius).clamp(0.0, 1.0);
        if altered > out.altered {
            out.altered = altered;
        }
        if pool > out.pool {
            out.pool = pool;
            out.pool_depth = (1.0 + pool * 3.0).floor() as i32;
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VolcanoState {
    Dormant,
    Unrest,
    Erupting,
    Cooling,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct VolcanoRuntime {
    pub id: u64,
    pub state: VolcanoState,
    pub seconds_in_state: f32,
    pub pulse_seconds: f32,
}

#[derive(Resource, Default, serde::Serialize, serde::Deserialize)]
pub struct VolcanoRegistry {
    pub active: std::collections::HashMap<u64, VolcanoRuntime>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_volcano(seed: u32) -> VolcanoDescriptor {
        for cz in -8..=8 {
            for cx in -8..=8 {
                if let Some(v) = candidate(seed, cx, cz) {
                    return v;
                }
            }
        }
        panic!("test seed should produce a volcano");
    }

    #[test]
    fn descriptors_are_deterministic() {
        let a = find_volcano(42);
        let b = find_volcano(42);
        assert_eq!(a, b);
        assert!((VOLCANO_MIN_RADIUS..=VOLCANO_MAX_RADIUS).contains(&a.radius));
    }

    #[test]
    fn cone_has_a_crater_below_the_rim() {
        let v = find_volcano(7);
        let centre = sample_volcano(7, v.center.x, v.center.y);
        let rim = sample_volcano(7, v.center.x + v.crater_radius, v.center.y);
        assert!(rim.elevation > centre.elevation);
        assert!(rim.elevation > 0.0);
    }

    #[test]
    fn hydrothermal_fields_need_groundwater() {
        let v = find_volcano(99);
        let p = v.center + DVec2::X * v.radius * 0.8;
        let dry = sample_hydrothermal(99, p.x, p.y, -0.5);
        assert_eq!(dry.pool, 0.0);
    }

    #[test]
    fn locate_candidates_include_the_nearby_volcano() {
        let v = find_volcano(91);
        let found = volcanoes_nearby(91, v.center.x, v.center.y, 1);
        assert!(found.iter().any(|candidate| candidate.id == v.id));
    }

    #[test]
    fn reported_hydrothermal_centers_are_pool_centers() {
        let v = find_volcano(117);
        for center in hydrothermal_pool_centers(v) {
            let sample = sample_hydrothermal(117, center.x, center.y, 1.0);
            assert!(sample.pool > 0.99);
        }
    }
}
