use super::*;

fn params() -> NoiseParams {
    NoiseParams {
        frequency: 0.015,
        amplitude: 40.0,
        octaves: 4,
        seed: 1,
        temp_offset: 0.0,
        ..Default::default()
    }
}

#[test]
fn centripetal_spline_keeps_segment_endpoints() {
    let p0 = Vec2::new(-1.0, 0.0);
    let p1 = Vec2::new(0.0, 0.0);
    let p2 = Vec2::new(1.0, 1.0);
    let p3 = Vec2::new(1.0, 2.0);
    assert!((catmull(p0, p1, p2, p3, 0.0) - p1).length() < 1e-5);
    assert!((catmull(p0, p1, p2, p3, 1.0) - p2).length() < 1e-5);
}

#[test]
fn adjacent_tiles_produce_compatible_overlap() {
    let tuning = RiverTuning::snapshot();
    let left = Tile::compute(0, 0, params(), tuning);
    let right = Tile::compute(1, 0, params(), tuning);
    let seam_x = TILE as f64 * CELL;
    let mut compared = 0;

    for z in 0..TILE {
        let wz = (z as f64 + 0.5) * CELL;
        for offset in [-8.0, 0.0, 8.0] {
            let (Some(a), Some(b)) = (
                left.query(seam_x + offset, wz),
                right.query(seam_x + offset, wz),
            ) else {
                continue;
            };
            compared += 1;
            assert!((a.surface - b.surface).abs() < 1.0);
            assert!(a.flow.dot(b.flow) > 0.8);
        }
    }

    assert!(compared > 0, "test seed must include a river at this seam");
}
