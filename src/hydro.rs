//! โครงข่ายแม่น้ำจริงจาก **flow accumulation** — ลำน้ำวิ่งตามทางลาดจริง (ไม่ใช่ noise เส้นเดียว)
//!
//! หลักการ: coarse grid (cell ~32 บล็อก) → hydrology height (แบ็คโบน+ภูเขา) → priority-flood
//! ถมแอ่ง → D8 flow direction → flow accumulation (นับ cell ต้นน้ำที่ไหลผ่าน) → cell ที่ accumulation
//! เกินเกณฑ์ = แม่น้ำ (accumulation มาก = สายใหญ่/กว้าง). ทุกอย่างเป็น pure function ของ seed
//! → host/client คำนวณเองได้เท่ากัน (ไม่ต้อง sync). infinite world = คำนวณเป็น tile + apron แล้ว cache.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::sync::{Arc, OnceLock, RwLock};

use bevy::math::Vec2;

use crate::voxel::{TerrainSampler, SEA_LEVEL};
use crate::NoiseParams;

#[cfg(test)]
mod regression_tests;

// ── ค่าจูน ──
/// ขนาด coarse cell (บล็อก)
const CELL: f64 = 32.0;
/// จำนวน cell ต่อด้านของ tile ชั้นใน (inner) และ apron รอบด้าน (จับ watershed ที่ไหลเข้าจากนอก tile)
const TILE: i32 = 48;
const APRON: i32 = 48;
const MAX_CACHED_TILES: usize = 64;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering as AtomicOrdering;

pub struct RiverConfig {
    pub threshold: AtomicU32,
    pub width_acc: AtomicU32,
    pub width_min: AtomicU32,
    pub width_max: AtomicU32,
    pub depth: AtomicU32,
    pub valley_margin: AtomicU32,
}

#[allow(dead_code)] // Setter API is kept for the planned world-generation tuning UI.
impl RiverConfig {
    pub const fn new() -> Self {
        Self {
            threshold: AtomicU32::new(120f32.to_bits()),
            width_acc: AtomicU32::new(1800f32.to_bits()),
            width_min: AtomicU32::new(2.5f32.to_bits()),
            width_max: AtomicU32::new(14f32.to_bits()),
            depth: AtomicU32::new(3f32.to_bits()),
            valley_margin: AtomicU32::new(16f32.to_bits()),
        }
    }

    pub fn threshold(&self) -> f32 { f32::from_bits(self.threshold.load(AtomicOrdering::Relaxed)) }
    pub fn width_acc(&self) -> f32 { f32::from_bits(self.width_acc.load(AtomicOrdering::Relaxed)) }
    pub fn width_min(&self) -> f32 { f32::from_bits(self.width_min.load(AtomicOrdering::Relaxed)) }
    pub fn width_max(&self) -> f32 { f32::from_bits(self.width_max.load(AtomicOrdering::Relaxed)) }
    pub fn depth(&self) -> f32 { f32::from_bits(self.depth.load(AtomicOrdering::Relaxed)) }
    pub fn valley_margin(&self) -> f32 { f32::from_bits(self.valley_margin.load(AtomicOrdering::Relaxed)) }

    pub fn set_threshold(&self, v: f32) { self.threshold.store(v.to_bits(), AtomicOrdering::Relaxed); invalidate_cache(); }
    pub fn set_width_acc(&self, v: f32) { self.width_acc.store(v.to_bits(), AtomicOrdering::Relaxed); invalidate_cache(); }
    pub fn set_width_min(&self, v: f32) { self.width_min.store(v.to_bits(), AtomicOrdering::Relaxed); invalidate_cache(); }
    pub fn set_width_max(&self, v: f32) { self.width_max.store(v.to_bits(), AtomicOrdering::Relaxed); invalidate_cache(); }
    pub fn set_depth(&self, v: f32) { self.depth.store(v.to_bits(), AtomicOrdering::Relaxed); invalidate_cache(); }
    pub fn set_valley_margin(&self, v: f32) { self.valley_margin.store(v.to_bits(), AtomicOrdering::Relaxed); invalidate_cache(); }
}

pub static RIVER_CFG: RiverConfig = RiverConfig::new();

#[derive(Clone, Copy)]
struct RiverTuning {
    threshold: f32,
    width_acc: f32,
    width_min: f32,
    width_max: f32,
    depth: f32,
    valley_margin: f32,
}

impl RiverTuning {
    fn snapshot() -> Self {
        let width_min = RIVER_CFG.width_min().max(0.5);
        Self {
            threshold: RIVER_CFG.threshold().max(1.0),
            width_acc: RIVER_CFG.width_acc().max(1.0),
            width_min,
            width_max: RIVER_CFG.width_max().max(width_min),
            depth: RIVER_CFG.depth().max(0.5),
            valley_margin: RIVER_CFG.valley_margin().max(0.0),
        }
    }
}

/// สเกลความชัน→ความเร็ว และเพดาน (ปรับยากเพราะเกี่ยวพันกับหลายส่วน)
const FLOW_K: f32 = 6.0;
const FLOW_MAX: f32 = 1.0;

/// ข้อมูลแม่น้ำ ณ จุดหนึ่ง (คืนจาก [`river_at`])
#[derive(Clone, Copy)]
pub struct RiverPoint {
    /// 0..1 (แกนกลาง=1 จางออกริมฝั่งน้ำ)
    pub mask: f32,
    /// 0..1 (สำหรับหุบเขาแม่น้ำ, แกนกลาง=1 จางออกขอบหุบเขา)
    pub valley_mask: f32,
    /// ระดับผิวน้ำ (บล็อก)
    pub surface: f32,
    /// ความลึกร่อง (บล็อก)
    pub depth: f32,
    /// ทิศไหล (หน่วยเดียว, x=แกน X โลก, y=แกน Z โลก)
    pub flow: Vec2,
    /// ความเร็ว 0..1 (ชันกว่า=เร็วกว่า)
    pub speed: f32,
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

// ── global cache ──
struct State {
    params: Option<NoiseParams>,
    tiles: HashMap<(i32, i32), Arc<Tile>>,
    insertion_order: VecDeque<(i32, i32)>,
    generation: u64,
}

static HYDRO: OnceLock<RwLock<State>> = OnceLock::new();

fn state() -> &'static RwLock<State> {
    HYDRO.get_or_init(|| RwLock::new(State {
        params: None,
        tiles: HashMap::new(),
        insertion_order: VecDeque::new(),
        generation: 0,
    }))
}

fn clear_cache(state: &mut State) {
    state.tiles.clear();
    state.insertion_order.clear();
    state.generation = state.generation.wrapping_add(1);
}

fn invalidate_cache() {
    let mut state = state()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_cache(&mut state);
}

/// ตั้ง seed/noise ของโลก — ล้าง cache ถ้าเปลี่ยน (เรียกตอนสร้างโลก/เปลี่ยน seed)
pub fn configure(params: NoiseParams) {
    let mut s = state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    if s.params != Some(params) {
        s.params = Some(params);
        clear_cache(&mut s);
    }
}

/// ข้อมูลแม่น้ำที่พิกัดโลก (x,z) — None ถ้าไม่ใช่แม่น้ำ. deterministic ล้วน (cache ต่อ tile)
pub fn river_at(wx: f64, wz: f64) -> Option<RiverPoint> {
    let cx = (wx / CELL).floor() as i32;
    let cz = (wz / CELL).floor() as i32;
    let local_x = cx.rem_euclid(TILE);
    let local_z = cz.rem_euclid(TILE);
    let x_offsets: &[i32] = if local_x <= 1 {
        &[-1, 0]
    } else if local_x >= TILE - 2 {
        &[0, 1]
    } else {
        &[0]
    };
    let z_offsets: &[i32] = if local_z <= 1 {
        &[-1, 0]
    } else if local_z >= TILE - 2 {
        &[0, 1]
    } else {
        &[0]
    };

    let mut best: Option<RiverPoint> = None;
    for &tile_z in z_offsets {
        for &tile_x in x_offsets {
            let tile = tile_for_cell(cx + tile_x * TILE, cz + tile_z * TILE)?;
            let Some(candidate) = tile.query(wx, wz) else {
                continue;
            };
            let replace = best.is_none_or(|current| {
                candidate.mask > current.mask + 0.01
                    || ((candidate.mask - current.mask).abs() <= 0.01
                        && candidate.speed > current.speed)
            });
            if replace {
                best = Some(candidate);
            }
        }
    }
    best
}

fn tile_for_cell(cx: i32, cz: i32) -> Option<Arc<Tile>> {
    let tx = cx.div_euclid(TILE);
    let tz = cz.div_euclid(TILE);
    // fast path: มีใน cache แล้ว
    let (params, generation) = {
        let s = state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(t) = s.tiles.get(&(tx, tz)) {
            return Some(t.clone());
        }
        (s.params?, s.generation)
    };
    // คำนวณนอก lock (deterministic — สองเธรดคำนวณ tile เดียวกันได้ผลเท่ากัน)
    let tile = Arc::new(Tile::compute(tx, tz, params, RiverTuning::snapshot()));
    let mut s = state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    if s.generation != generation || s.params != Some(params) {
        drop(s);
        return tile_for_cell(cx, cz);
    }
    if let Some(existing) = s.tiles.get(&(tx, tz)) {
        return Some(existing.clone());
    }
    while s.tiles.len() >= MAX_CACHED_TILES {
        let Some(oldest) = s.insertion_order.pop_front() else { break };
        s.tiles.remove(&oldest);
    }
    s.insertion_order.push_back((tx, tz));
    s.tiles.insert((tx, tz), tile.clone());
    Some(tile)
}

// ── tile ──
struct Seg {
    a: Vec2,
    b: Vec2,
    surf_a: f32,
    surf_b: f32,
    width: f32,
    flow: Vec2,
    speed: f32,
    accumulation: f32,
}

struct Tile {
    segs: Vec<Seg>,
    tuning: RiverTuning,
    /// cell → index ของ segment ที่พาดผ่านบริเวณนั้น (spatial index)
    buckets: HashMap<(i32, i32), Vec<u32>>,
}

impl Tile {
    fn query(&self, wx: f64, wz: f64) -> Option<RiverPoint> {
        let cx = (wx / CELL).floor() as i32;
        let cz = (wz / CELL).floor() as i32;
        let p = Vec2::new(wx as f32, wz as f32);
        let mut best: Option<(f32, usize, f32)> = None; // (dist, seg idx, t)
        for dz in -1..=1 {
            for dx in -1..=1 {
                if let Some(list) = self.buckets.get(&(cx + dx, cz + dz)) {
                    for &si in list {
                        let s = &self.segs[si as usize];
                        let (dist, t) = dist_to_seg(p, s.a, s.b);
                        let better = best.map_or(true, |(best_dist, best_si, _)| {
                            dist + 0.5 < best_dist
                                || ((dist - best_dist).abs() <= 0.5
                                    && s.accumulation > self.segs[best_si].accumulation)
                        });
                        if dist < s.width + self.tuning.valley_margin && better {
                            best = Some((dist, si as usize, t));
                        }
                    }
                }
            }
        }
        let (dist, si, t) = best?;
        let s = &self.segs[si];
        Some(RiverPoint {
            mask: (1.0 - dist / s.width).clamp(0.0, 1.0),
            valley_mask: (1.0 - dist / (s.width + self.tuning.valley_margin)).clamp(0.0, 1.0),
            surface: lerp(s.surf_a, s.surf_b, t),
            depth: self.tuning.depth,
            flow: s.flow,
            speed: s.speed,
        })
    }

    fn compute(tx: i32, tz: i32, params: NoiseParams, tuning: RiverTuning) -> Tile {
        let sampler = TerrainSampler::new(params);
        let n = (TILE + 2 * APRON) as usize;
        let min_cx = tx * TILE - APRON;
        let min_cz = tz * TILE - APRON;
        let sea = SEA_LEVEL as f32;
        let idx = |x: usize, z: usize| z * n + x;
        let cell_world = |x: usize, z: usize| -> (f64, f64) {
            let cx = min_cx + x as i32;
            let cz = min_cz + z as i32;
            ((cx as f64 + 0.5) * CELL, (cz as f64 + 0.5) * CELL)
        };

        // 1) hydrology height ต่อ cell
        let mut h = vec![0f32; n * n];
        for z in 0..n {
            for x in 0..n {
                let (wx, wz) = cell_world(x, z);
                h[idx(x, z)] = sampler.hydro_height(wx, wz) as f32;
            }
        }

        // 2) priority-flood ถมแอ่ง (Barnes ε) — ทุก cell ระบายออกขอบได้ ไม่มี sink ค้าง
        let mut filled = h.clone();
        let mut visited = vec![false; n * n];
        let mut heap: BinaryHeap<MinF> = BinaryHeap::new();
        for z in 0..n {
            for x in 0..n {
                if x == 0 || z == 0 || x == n - 1 || z == n - 1 {
                    let i = idx(x, z);
                    visited[i] = true;
                    heap.push(MinF(filled[i], i));
                }
            }
        }
        const EPS: f32 = 0.001;
        const NEIGH: [(i32, i32); 8] =
            [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
        while let Some(MinF(hc, ci)) = heap.pop() {
            let cx = ci % n;
            let cz = ci / n;
            for (dx, dz) in NEIGH {
                let nx = cx as i32 + dx;
                let nz = cz as i32 + dz;
                if nx < 0 || nz < 0 || nx >= n as i32 || nz >= n as i32 {
                    continue;
                }
                let ni = idx(nx as usize, nz as usize);
                if visited[ni] {
                    continue;
                }
                visited[ni] = true;
                let nh = filled[ni].max(hc + EPS);
                filled[ni] = nh;
                heap.push(MinF(nh, ni));
            }
        }

        // 3) D8 downstream (บน filled) — cell ในไหลไป neighbor ต่ำสุด; ทะเล/ขอบ = terminator
        let mut down = vec![usize::MAX; n * n];
        for z in 1..n - 1 {
            for x in 1..n - 1 {
                let ci = idx(x, z);
                if h[ci] <= sea {
                    continue; // ทะเล = ปลายทาง
                }
                let mut bh = filled[ci];
                let mut bd = usize::MAX;
                for (dx, dz) in NEIGH {
                    let ni = idx((x as i32 + dx) as usize, (z as i32 + dz) as usize);
                    if filled[ni] < bh {
                        bh = filled[ni];
                        bd = ni;
                    }
                }
                down[ci] = bd;
            }
        }

        // 4) flow accumulation — ประมวลจากสูง→ต่ำ (ต้นน้ำก่อน) บวกลง downstream
        let mut acc = vec![1f32; n * n];
        let mut surface = h.clone();
        let mut order: Vec<usize> = (0..n * n).collect();
        order.sort_by(|&a, &b| filled[b].total_cmp(&filled[a]));
        for &ci in &order {
            let d = down[ci];
            if d != usize::MAX {
                acc[d] += acc[ci];
                // Routing uses the depression-filled elevation, but visible water
                // follows a carved profile that never rises above the source terrain.
                surface[d] = surface[d].min(surface[ci] - 0.02);
            }
        }

        // 5) upstream หลัก (สาขาที่ acc มากสุดที่ไหลเข้า cell นี้) — ใช้เป็น tangent ของเส้นโค้ง
        let mut up = vec![usize::MAX; n * n];
        for c in 0..n * n {
            let d = down[c];
            if d != usize::MAX
                && acc[c] >= tuning.threshold
                && (up[d] == usize::MAX || acc[c] > acc[up[d]])
            {
                up[d] = c;
            }
        }

        // 6) สร้าง segments แบบ **smooth (Catmull-Rom ผ่านจุดกลาง cell)** — ลำน้ำโค้ง ไม่หักมุม D8
        let center = |i: usize| -> Vec2 {
            let (wx, wz) = cell_world(i % n, i / n);
            Vec2::new(wx as f32, wz as f32)
        };
        let width_of = |a: f32| {
            lerp(
                tuning.width_min,
                tuning.width_max,
                (a - tuning.threshold) / tuning.width_acc,
            )
        };

        let mut segs: Vec<Seg> = Vec::new();
        let mut buckets: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        let lo = APRON - 1;
        let hi = APRON + TILE; // local idx ของ inner cells = [APRON, APRON+TILE)
        const SUB: usize = 4; // จำนวนช่วงย่อยต่อ cell (มากขึ้น=โค้งเนียนขึ้น)
        for z in 0..n {
            for x in 0..n {
                let (lx, lz) = (x as i32, z as i32);
                if lx < lo || lx > hi || lz < lo || lz > hi {
                    continue;
                }
                let ci = idx(x, z);
                if h[ci] <= sea + 1.0 || acc[ci] < tuning.threshold {
                    continue;
                }
                let d = down[ci];
                if d == usize::MAX {
                    continue;
                }
                // 4 จุดคุมเส้นโค้ง: up → นี่ → down → down-down (ปลายที่ขาดใช้ extrapolate)
                let p1 = center(ci);
                let p2 = center(d);
                let p0 = if up[ci] != usize::MAX { center(up[ci]) } else { p1 * 2.0 - p2 };
                let dd = down[d];
                let p3 = if dd != usize::MAX { center(dd) } else { p2 * 2.0 - p1 };

                let (surf1, surf2) = (surface[ci], surface[d]);
                let (w1, w2) = (width_of(acc[ci]), width_of(acc[d]));
                let total_len = (p2 - p1).length().max(0.001);
                let speed = (((surf1 - surf2).max(0.0) / total_len) * FLOW_K).clamp(0.0, FLOW_MAX);

                let mut prev = catmull(p0, p1, p2, p3, 0.0);
                for s in 1..=SUB {
                    let (t0, t1) = ((s - 1) as f32 / SUB as f32, s as f32 / SUB as f32);
                    let cur = catmull(p0, p1, p2, p3, t1);
                    let dir = cur - prev;
                    let len = dir.length().max(0.001);
                    let width = lerp(w1, w2, (t0 + t1) * 0.5);
                    let si = segs.len() as u32;
                    segs.push(Seg {
                        a: prev,
                        b: cur,
                        surf_a: lerp(surf1, surf2, t0),
                        surf_b: lerp(surf1, surf2, t1),
                        width,
                        flow: dir / len,
                        speed,
                        accumulation: lerp(acc[ci], acc[d], (t0 + t1) * 0.5),
                    });
                    // bucket ลงทุก cell ที่ bbox (ขยายด้วย width+valley) พาดถึง
                    let wr =
                        ((width + tuning.valley_margin) as f64 / CELL).ceil() as i32 + 1;
                    let cxmin = (prev.x.min(cur.x) as f64 / CELL).floor() as i32 - wr;
                    let cxmax = (prev.x.max(cur.x) as f64 / CELL).floor() as i32 + wr;
                    let czmin = (prev.y.min(cur.y) as f64 / CELL).floor() as i32 - wr;
                    let czmax = (prev.y.max(cur.y) as f64 / CELL).floor() as i32 + wr;
                    for cz2 in czmin..=czmax {
                        for cx2 in cxmin..=cxmax {
                            buckets.entry((cx2, cz2)).or_default().push(si);
                        }
                    }
                    prev = cur;
                }
            }
        }

        Tile {
            segs,
            tuning,
            buckets,
        }
    }
}

/// min-heap wrapper สำหรับ f32 (pop คืนค่าน้อยสุด) — BinaryHeap เป็น max-heap เลยกลับ cmp
#[derive(PartialEq)]
struct MinF(f32, usize);
impl Eq for MinF {}
impl PartialOrd for MinF {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for MinF {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
    }
}

/// Catmull-Rom spline (uniform) ผ่าน p1→p2 โดยมี p0/p3 เป็น tangent — คืนจุดที่ t (0..1)
#[inline]
fn catmull(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    fn knot(a: Vec2, b: Vec2, previous: f32) -> f32 {
        previous + (b - a).length().sqrt().max(1e-3)
    }
    fn blend(a: Vec2, b: Vec2, ta: f32, tb: f32, t: f32) -> Vec2 {
        let span = (tb - ta).max(1e-6);
        a * ((tb - t) / span) + b * ((t - ta) / span)
    }

    let t0 = 0.0;
    let t1 = knot(p0, p1, t0);
    let t2 = knot(p1, p2, t1);
    let t3 = knot(p2, p3, t2);
    let u = lerp(t1, t2, t);
    let a1 = blend(p0, p1, t0, t1, u);
    let a2 = blend(p1, p2, t1, t2, u);
    let a3 = blend(p2, p3, t2, t3, u);
    let b1 = blend(a1, a2, t0, t2, u);
    let b2 = blend(a2, a3, t1, t3, u);
    blend(b1, b2, t1, t2, u)
}

/// ระยะจากจุด p ถึงเซกเมนต์ a→b + พารามิเตอร์ t (0..1) ของจุดที่ใกล้สุด
#[inline]
fn dist_to_seg(p: Vec2, a: Vec2, b: Vec2) -> (f32, f32) {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 <= 1e-6 { 0.0 } else { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) };
    ((p - (a + ab * t)).length(), t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> NoiseParams {
        NoiseParams { frequency: 0.015, amplitude: 40.0, octaves: 4, seed: 1, temp_offset: 0.0 }
    }

    #[test]
    fn tile_has_rivers_and_flows_downhill() {
        let tile = Tile::compute(0, 0, params(), RiverTuning::snapshot());
        assert!(!tile.segs.is_empty(), "ควรมีแม่น้ำอย่างน้อยหนึ่งสายใน tile seed 1");
        // ทุกเซกเมนต์ต้องไหลลง (ผิวน้ำต้นทาง ≥ ปลายทาง) และมีทิศเป็นเวกเตอร์หนึ่งหน่วย
        for s in &tile.segs {
            assert!(s.surf_a >= s.surf_b - 0.01, "แม่น้ำต้องไหลลง (surf_a≥surf_b)");
            assert!((s.flow.length() - 1.0).abs() < 1e-3, "flow ต้องเป็นเวกเตอร์หนึ่งหน่วย");
            assert!(s.width >= RIVER_CFG.width_min() && s.width <= RIVER_CFG.width_max());
        }
    }

    #[test]
    fn river_query_returns_flow_on_channel() {
        configure(params());
        let tile = Tile::compute(0, 0, params(), RiverTuning::snapshot());
        // หยิบเซกเมนต์แรก แล้ว query ที่จุดกึ่งกลาง — ต้องเจอแม่น้ำ ทิศตรงกับเซกเมนต์
        let s = &tile.segs[0];
        let mid = (s.a + s.b) * 0.5;
        let r = tile.query(mid.x as f64, mid.y as f64).expect("กลางลำน้ำต้องเป็นแม่น้ำ");
        assert!(r.mask > 0.0);
        assert!(r.flow.dot(s.flow) > 0.9, "ทิศ query ต้องตรงกับเซกเมนต์");
    }
}
