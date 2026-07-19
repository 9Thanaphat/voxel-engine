use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

use crate::voxel::{block_color, BlockType, MeshBuf, TerrainSampler, SEA_LEVEL};

// --------------------------------------------------------
// Distant terrain LOD (แนว Distant Horizons) — วาดภูมิประเทศไกลเกินระยะ
// chunk จริงด้วย heightfield หยาบ สังเคราะห์ตรงจาก height source (noise/DEM)
// โดยไม่ต้องมี chunk; รองรับทั้งสองโหมดโลกผ่าน HeightSource ตัวเดียว
// --------------------------------------------------------

/// วงแหวน LOD: (ขนาด cell เมตร, ขนาด tile บล็อก, รัศมีบล็อก)
/// tile ของวงละเอียดซ้อน "ใต้" chunk จริงด้วย (offset y ลบ) — ไม่มีรูตอน chunk โหลด/หาย
const LOD_RINGS: [(f32, i32, f32); 3] = [
    (8.0, 512, 2_500.0),
    (32.0, 2048, 10_000.0),
    (128.0, 8192, 33_000.0),
];
/// จมใต้ระดับจริงเล็กน้อย — chunk จริง/วงละเอียดกว่าวาดทับสนิท ไม่ z-fight
const LOD_Y_OFFSET: f32 = -1.5;
/// กระโปรงขอบ tile ห้อยลงเป็นค่าคงที่ (ไม่ใช่คูณ cell แบบเดิม) — เดิม
/// cell*1.5 ทำให้ ring นอก (cell 128ม.) ได้กระโปรงสูงถึง 192ม. กลายเป็นกำแพง
/// ตั้งตระหง่านโผล่ให้เห็นทั่วภูเขาจริงที่ลาดชัน (คือ "รอยแตก" ประหลาดที่เจอ)
/// ทั้งที่จุดประสงค์แค่ปิดรอยต่อบางๆ ระหว่าง tile/วงที่ความละเอียดต่างกัน
const SKIRT_DROP: f32 = 6.0;
const MAX_TASKS_IN_FLIGHT: usize = 6;
/// คาบเช็ค tile รอบกล้อง (วินาที)
const UPDATE_PERIOD: f32 = 0.5;

/// สเปกแหล่งความสูง — Copy ได้ ส่งเข้า task แล้วค่อยประกอบของจริงข้างใน
#[derive(Clone, Copy, PartialEq)]
pub enum HeightSourceSpec {
    Noise(crate::NoiseParams),
    Dem,
}

impl HeightSourceSpec {
    fn from_settings(settings: &crate::GameSettings) -> Self {
        match settings.terrain_source {
            crate::TerrainSource::Noise => Self::Noise(settings.noise),
            crate::TerrainSource::RealWorld => Self::Dem,
        }
    }
}

/// แหล่งความสูงจริงที่ใช้ใน task (โหมดไหนก็หน้าตาเดียวกันต่อ mesher)
enum HeightSource {
    Noise(TerrainSampler),
    Dem(&'static crate::dem::DemData),
}

impl HeightSource {
    fn build(spec: HeightSourceSpec) -> Option<Self> {
        match spec {
            HeightSourceSpec::Noise(params) => Some(Self::Noise(TerrainSampler::new(params))),
            HeightSourceSpec::Dem => crate::dem::dem().map(Self::Dem),
        }
    }

    /// ความสูงผิว (หน่วย y บล็อก)
    fn height(&self, wx: f64, wz: f64) -> f32 {
        match self {
            Self::Noise(s) => s.height(wx, wz) as f32,
            Self::Dem(d) => {
                crate::dem::DEM_SEA_LEVEL_Y as f32 + d.elevation_at_block(wx, wz)
            }
        }
    }

    fn sea_level(&self) -> f32 {
        match self {
            Self::Noise(_) => SEA_LEVEL as f32,
            Self::Dem(_) => crate::dem::DEM_SEA_LEVEL_Y as f32,
        }
    }

    /// cell นี้เป็นแถบทราย (ติดทะเล/desert) ไหม — ตัดสินว่าผิว+ด้านข้างใช้ palette
    /// ทราย หรือ หญ้า/ดิน ตามกติกา worldgen (voxel.rs TerrainSampler::surface_block)
    fn is_sandy(&self, wx: f64, wz: f64, h: f32) -> bool {
        match self {
            Self::Noise(s) => h <= SEA_LEVEL as f32 + 1.0 || s.is_desert(wx, wz),
            Self::Dem(_) => h <= crate::dem::DEM_SEA_LEVEL_Y as f32 + 1.0,
        }
    }

    /// สีหน้าบนของ "บล็อกหยาบ" ให้ตรง palette กับ chunk ใกล้ๆ: หญ้า = สีเฉลี่ย
    /// texture (LOD_GRASS), ทราย = block_color(Sand) (ทรายไม่มี texture ใช้ค่าเดียว
    /// กับ mesher ใกล้อยู่แล้ว)
    fn top_color(&self, wx: f64, wz: f64, h: f32) -> [f32; 4] {
        if self.is_sandy(wx, wz, h) {
            block_color(BlockType::Sand)
        } else {
            LOD_GRASS
        }
    }
}

pub struct LodTileResult {
    ring: usize,
    coord: IVec2,
    version: u32,
    buf: MeshBuf,
}

#[derive(Resource)]
pub struct LodTiles {
    tiles: HashMap<(usize, IVec2), Entity>,
    pending: HashSet<(usize, IVec2)>,
    sender: Mutex<Sender<LodTileResult>>,
    receiver: Mutex<Receiver<LodTileResult>>,
    material: Handle<StandardMaterial>,
    /// bump ทุกครั้งที่ล้าง (สลับโลก/ปิดระบบ) — ผล task รุ่นเก่าถูกทิ้ง
    version: u32,
    timer: f32,
    /// spec ล่าสุดที่ใช้ — เปลี่ยน (จูน noise/สลับโหมด) = ล้างสร้างใหม่
    last_spec: Option<HeightSourceSpec>,
    /// นับรอบไว้ log สถานะห่างๆ (วินิจฉัย)
    passes: u32,
}

pub fn setup_lod(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    let (s, r) = mpsc::channel();
    commands.insert_resource(LodTiles {
        tiles: HashMap::new(),
        pending: HashSet::new(),
        sender: Mutex::new(s),
        receiver: Mutex::new(r),
        material: materials.add(StandardMaterial {
            base_color: Color::WHITE, // สีจาก vertex color
            perceptual_roughness: 1.0,
            ..default()
        }),
        version: 0,
        timer: 0.0,
        last_spec: None,
        passes: 0,
    });
}

/// ความหนา (เมตร) ของแถบดินใต้ผิวก่อนถึงหิน — เหมือนบล็อกหญ้านั่งบนดินบนหิน
/// ทำให้หน้าภูเขาไกลเป็นกำแพงดินคาดบน หินข้างล่าง (ไม่ใช่เทาล้วน)
const SOIL_BAND_M: f32 = 8.0;

// สีเฉลี่ยของ texture จริง (grass_top/dirt/stone.png) แปลงเป็น linear space —
// โลกใกล้ render บล็อกพวกนี้ด้วย texture (ไม่ใช่ block_color fallback) และ
// bevy ใช้ vertex color เป็น linear คูณกับ texture ที่ sample เป็น linear แล้ว
// เพราะฉะนั้น LOD ต้องใช้ค่า linear เฉลี่ยของ texture ถึงจะสีตรงกับบล็อกใกล้
// (grass_top จริงเป็นเขียวอมฟ้า ไม่ใช่ [0.2,0.6,0.2] ตาม registry) —
// วัดด้วยการเฉลี่ยพิกเซลไฟล์ png; sand/water ไม่มี texture (ใช้ block_color อยู่
// แล้วทั้งใกล้และ LOD จึงตรงกันเอง)
const LOD_GRASS: [f32; 4] = [0.053, 0.254, 0.241, 1.0];
const LOD_DIRT: [f32; 4] = [0.129, 0.044, 0.021, 1.0];
const LOD_STONE: [f32; 4] = [0.204, 0.204, 0.204, 1.0];
/// เงาประจำทิศ — ค่าตรงกับ FACE_SHADE ใน voxel.rs (top 1.0, +X/-X 0.8, +Z/-Z 0.6)
/// ให้ความสว่างหน้าบล็อก LOD เท่าบล็อกใกล้ตัว (บล็อกหยาบใช้ flat shade ต่อหน้า
/// ไม่ใช่ normal ต่อเนื่อง ไม่งั้นเสียคาแรกเตอร์ "บล็อก")
const SHADE_TOP: f32 = 1.0;
const SHADE_X: f32 = 0.8;
const SHADE_Z: f32 = 0.6;

/// สร้าง mesh ของ tile หนึ่งใบ (รันใน background task) — สไตล์ Distant Horizons
/// จริง: หนึ่งความสูงต่อหนึ่ง "บล็อกหยาบ" (เซลล์ = ก้อนเดียว ไม่ไล่เอียงเนียน)
/// พื้นบนแบน + กำแพงขั้นบันไดตรงรอยต่อที่สูงต่างกัน ให้เห็นเป็นขั้นบล็อกแบบ
/// เกมนี้ตอนมองใกล้ ไม่ใช่เนินโค้งมนแบบ terrain LOD ทั่วไป
fn build_tile(source: &HeightSource, ring: usize, coord: IVec2) -> MeshBuf {
    let (cell, tile_size, _) = LOD_RINGS[ring];
    let n = (tile_size as f32 / cell) as usize; // จำนวน cell ต่อด้าน
    let origin_x = coord.x as f64 * tile_size as f64;
    let origin_z = coord.y as f64 * tile_size as f64;
    let sea = source.sea_level();

    // ความสูงเดียวต่อ cell (สุ่มกลาง cell) — คือ "บล็อก" หนึ่งก้อนของ LOD วงนี้
    let mut hs = vec![0f32; n * n];
    for j in 0..n {
        for i in 0..n {
            let wx = origin_x + (i as f64 + 0.5) * cell as f64;
            let wz = origin_z + (j as f64 + 0.5) * cell as f64;
            hs[j * n + i] = source.height(wx, wz) + LOD_Y_OFFSET;
        }
    }
    let h_at = |i: usize, j: usize| hs[j * n + i];
    // แถบทราย/หญ้าต่อ cell (สำหรับเลือก palette ผิว+ด้านข้าง)
    let sandy_at = |i: usize, j: usize| {
        let wx = origin_x + (i as f64 + 0.5) * cell as f64;
        let wz = origin_z + (j as f64 + 0.5) * cell as f64;
        source.is_sandy(wx, wz, h_at(i, j))
    };

    let water = block_color(BlockType::Water);
    let mut buf = MeshBuf::default();

    for j in 0..n {
        for i in 0..n {
            let (x0, x1) = (i as f32 * cell, (i + 1) as f32 * cell);
            let (z0, z1) = (j as f32 * cell, (j + 1) as f32 * cell);
            let h = h_at(i, j);
            let wx = origin_x + (i as f64 + 0.5) * cell as f64;
            let wz = origin_z + (j as f64 + 0.5) * cell as f64;
            let col = shade(source.top_color(wx, wz, h), SHADE_TOP);

            // หน้าบนแบน — ลำดับจุดตรงกับ CUBE_POSITIONS หน้า Top ใน voxel.rs
            // (z1 ก่อน z0) ไม่งั้น winding กลับด้าน โดน backface cull มองจาก
            // บนไม่เห็น (เห็นทะลุ/พื้นหาย — บั๊ก "render สลับด้าน" ที่เจอมาก่อน)
            push_quad_flat(&mut buf, [[x0, h, z1], [x1, h, z1], [x1, h, z0], [x0, h, z0]], col);

            // ผิวน้ำแบนที่ sea level เมื่อ cell จมน้ำ
            if h < sea {
                let wy = sea + LOD_Y_OFFSET + 0.5;
                push_quad_flat(&mut buf, [[x0, wy, z1], [x1, wy, z1], [x1, wy, z0], [x0, wy, z0]], water);
            }

            // กำแพงขั้นบันไดตรงรอยต่อ +X และ +Z เท่านั้น (พอ — ขอบ -X/-Z ของ
            // เพื่อนบ้านจะโดนคลุมจากฝั่งนั้นเอง กันวาดซ้ำสองรอบ) — สี palette
            // ตาม cell ที่สูงกว่า (คือเจ้าของหน้ากำแพงนั้น): ดินคาดบน หินข้างล่าง
            if i + 1 < n {
                let hn = h_at(i + 1, j);
                if h != hn {
                    let taller = if h > hn { (i, j) } else { (i + 1, j) };
                    push_riser_layered(&mut buf, true, x1, z0, z1, h.min(hn), h.max(hn),
                        h > hn, SHADE_X, sandy_at(taller.0, taller.1));
                }
            }
            if j + 1 < n {
                let hn = h_at(i, j + 1);
                if h != hn {
                    let taller = if h > hn { (i, j) } else { (i, j + 1) };
                    push_riser_layered(&mut buf, false, z1, x0, x1, h.min(hn), h.max(hn),
                        h > hn, SHADE_Z, sandy_at(taller.0, taller.1));
                }
            }
        }
    }

    // กระโปรงขอบ tile: ห้อยลงจากขอบนอกสุดตามความสูงของ cell ริมนั้นๆ เอง —
    // ปิดรอยต่อระหว่าง ring/tile ที่ความละเอียดต่างกัน
    let drop = SKIRT_DROP;
    let t = tile_size as f32;
    for j in 0..n {
        let (z0, z1) = (j as f32 * cell, (j + 1) as f32 * cell);
        let h = h_at(0, j);
        push_riser_layered(&mut buf, true, 0.0, z0, z1, h - drop, h, false, SHADE_X, sandy_at(0, j));
        let h = h_at(n - 1, j);
        push_riser_layered(&mut buf, true, t, z0, z1, h - drop, h, true, SHADE_X, sandy_at(n - 1, j));
    }
    for i in 0..n {
        let (x0, x1) = (i as f32 * cell, (i + 1) as f32 * cell);
        let h = h_at(i, 0);
        push_riser_layered(&mut buf, false, 0.0, x0, x1, h - drop, h, false, SHADE_Z, sandy_at(i, 0));
        let h = h_at(i, n - 1);
        push_riser_layered(&mut buf, false, t, x0, x1, h - drop, h, true, SHADE_Z, sandy_at(i, n - 1));
    }

    buf
}

/// กำแพงตั้ง split เป็นดินคาดบน (SOIL_BAND_M ม.บนสุด) + หินข้างล่าง —
/// ให้อ่านเป็นบล็อกหญ้านั่งบนดินบนหินแบบโลกจริง (แถบทราย = ทรายแทนดิน+หิน)
fn push_riser_layered(
    buf: &mut MeshBuf,
    axis_x: bool,
    pos: f32,
    s0: f32,
    s1: f32,
    y_lo: f32,
    y_hi: f32,
    positive: bool,
    shade_f: f32,
    sandy: bool,
) {
    let soil = shade(if sandy { block_color(BlockType::Sand) } else { LOD_DIRT }, shade_f);
    let stone = shade(if sandy { block_color(BlockType::Sand) } else { LOD_STONE }, shade_f);
    let soil_bottom = (y_hi - SOIL_BAND_M).max(y_lo);
    if soil_bottom > y_lo {
        push_riser(buf, axis_x, pos, s0, s1, y_lo, soil_bottom, positive, stone);
    }
    push_riser(buf, axis_x, pos, s0, s1, soil_bottom, y_hi, positive, soil);
}

fn shade(col: [f32; 4], f: f32) -> [f32; 4] {
    [col[0] * f, col[1] * f, col[2] * f, col[3]]
}

/// กำแพงตั้งระหว่างพิกัด s0..s1 สูง y0..y1 — axis_x: true = กำแพงอยู่ที่ x คงที่
/// (pos), false = z คงที่; positive คือ normal ชี้ +แกนนั้น (winding ตรงกับ
/// CUBE_POSITIONS หน้า Right/Left/Forward/Backward ใน voxel.rs)
fn push_riser(
    buf: &mut MeshBuf,
    axis_x: bool,
    pos: f32,
    s0: f32,
    s1: f32,
    y0: f32,
    y1: f32,
    positive: bool,
    col: [f32; 4],
) {
    let verts = match (axis_x, positive) {
        (true, true) => [[pos, y0, s0], [pos, y1, s0], [pos, y1, s1], [pos, y0, s1]],
        (true, false) => [[pos, y0, s1], [pos, y1, s1], [pos, y1, s0], [pos, y0, s0]],
        (false, true) => [[s1, y0, pos], [s1, y1, pos], [s0, y1, pos], [s0, y0, pos]],
        (false, false) => [[s0, y0, pos], [s0, y1, pos], [s1, y1, pos], [s1, y0, pos]],
    };
    push_quad_flat(buf, verts, col);
}

/// normal จากมุม quad (flat) — บล็อกหยาบใช้ flat shading ล้วน ไม่มี normal ต่อเนื่อง
fn push_quad_flat(buf: &mut MeshBuf, verts: [[f32; 3]; 4], col: [f32; 4]) {
    let a = Vec3::from(verts[0]);
    let b = Vec3::from(verts[1]);
    let d = Vec3::from(verts[3]);
    let nrm = (b - a).cross(d - a).normalize_or_zero().to_array();
    let vc = buf.positions.len() as u32;
    for v in verts {
        buf.positions.push(v);
        buf.normals.push(nrm);
        buf.colors.push(col);
        buf.uvs.push([0.0, 0.0]);
    }
    buf.indices.extend_from_slice(&[vc, vc + 1, vc + 2, vc, vc + 2, vc + 3]);
}

#[cfg(test)]
mod debug_tests {
    use super::*;

    #[test]
    fn dem_elevation_sanity() {
        let Some(d) = crate::dem::dem() else {
            eprintln!("no dem loaded");
            return;
        };
        // จุดใกล้ spawn (52784, 55287) + จุดไกลออกไปตามรัศมี LOD แต่ละวง
        for (label, x, z) in [
            ("center", 52784.0, 55287.0),
            ("+2500", 52784.0 + 2500.0, 55287.0),
            ("+10000", 52784.0 + 10000.0, 55287.0),
            ("+33000", 52784.0 + 33000.0, 55287.0),
            ("-33000x", 52784.0 - 33000.0, 55287.0),
        ] {
            let elev = d.elevation_at_block(x, z);
            let y = crate::dem::DEM_SEA_LEVEL_Y as f32 + elev;
            eprintln!("{label}: elev={elev:.1}m  y={y:.1}");
        }

        // ตรงกับพิกัดมุมของ tile ring2 ตัวอย่างที่ log บอกว่า render (coord ~2..10)
        for coord_x in [2, 6, 10] {
            let origin_x = coord_x as f64 * 8192.0;
            let origin_z = 6.0 * 8192.0;
            let elev = d.elevation_at_block(origin_x, origin_z);
            eprintln!("tile coord x={coord_x}: origin=({origin_x},{origin_z}) elev={elev:.1}");
        }
    }

    #[test]
    fn slope_distribution() {
        let Some(source) = HeightSource::build(HeightSourceSpec::Dem) else {
            eprintln!("no dem loaded");
            return;
        };
        // ตัวอย่างพื้นที่ 8192m รอบ spawn ที่ cell 32m (ring1) — ดูว่า slope ทั่วไป
        // ของภูเขาจริงเป็นเท่าไหร่ ไว้ตั้งเกณฑ์สีหินให้ตรงกับความชันจริง
        let (cell, n): (f32, usize) = (32.0, 256);
        let ox = 52784.0 - (n as f32 * cell) / 2.0;
        let oz = 55287.0 - (n as f32 * cell) / 2.0;
        let mut slopes = Vec::with_capacity(n * n);
        let h = |i: usize, j: usize| source.height(ox as f64 + i as f64 * cell as f64, oz as f64 + j as f64 * cell as f64);
        for j in 0..n {
            for i in 0..n {
                let (h00, h10, h01, h11) = (h(i, j), h(i + 1, j), h(i, j + 1), h(i + 1, j + 1));
                slopes.push(((h00 - h11).abs().max((h10 - h01).abs())) / cell);
            }
        }
        slopes.sort_by(|a, b| a.total_cmp(b));
        let pct = |p: f32| slopes[((slopes.len() - 1) as f32 * p) as usize];
        eprintln!(
            "slope stats over {}x{} cells @ {}m: p50={:.2} p75={:.2} p90={:.2} p95={:.2} p99={:.2} max={:.2}",
            n, n, cell, pct(0.5), pct(0.75), pct(0.9), pct(0.95), pct(0.99), slopes.last().unwrap()
        );
    }

    #[test]
    fn ring_overlap_check() {
        // จำลอง desired-set logic แบบเดียวกับ update_lod_tiles ที่ camera x=54574,z=52250
        let cam = Vec2::new(54574.0, 52250.0);
        let mut ring_hits: std::collections::HashMap<(i32, i32), Vec<usize>> = Default::default();
        for (ring, (_cell, tile_size, radius)) in LOD_RINGS.iter().enumerate() {
            let radius = *radius;
            let inner = if ring == 0 { 0.0 } else { LOD_RINGS[ring - 1].2 };
            let t = *tile_size as f32;
            let (min_tx, max_tx) = (((cam.x - radius) / t).floor() as i32, ((cam.x + radius) / t).floor() as i32);
            let (min_tz, max_tz) = (((cam.y - radius) / t).floor() as i32, ((cam.y + radius) / t).floor() as i32);
            for tz in min_tz..=max_tz {
                for tx in min_tx..=max_tx {
                    let center = Vec2::new((tx as f32 + 0.5) * t, (tz as f32 + 0.5) * t);
                    let d = center.distance(cam);
                    if d <= radius && (ring == 0 || d > inner) {
                        ring_hits.entry((tx, tz)).or_default().push(ring);
                    }
                }
            }
        }
        let overlaps = ring_hits.values().filter(|v| v.len() > 1).count();
        eprintln!("cells with >1 ring drawing (different tile grids so this underscounts, but any >0 within same normalized space is bad): {overlaps} / {}", ring_hits.len());
        // นับพื้นที่ที่ทั้งสองวงคาดว่าจะวาดจริง (สุ่มจุดในดิสก์แล้วเช็คว่ากี่วง cover)
        let mut double_covered_samples = 0;
        let mut total_samples = 0;
        for i in -400..400 {
            for j in -400..400 {
                let p = cam + Vec2::new(i as f32 * 100.0, j as f32 * 100.0);
                let mut covers = 0;
                for (ring, (_cell, tile_size, radius)) in LOD_RINGS.iter().enumerate() {
                    let t = *tile_size as f32;
                    let inner = if ring == 0 { 0.0 } else { LOD_RINGS[ring - 1].2 };
                    let tx = (p.x / t).floor();
                    let tz = (p.y / t).floor();
                    let center = Vec2::new((tx + 0.5) * t, (tz + 0.5) * t);
                    let d = center.distance(cam);
                    if d <= *radius && (ring == 0 || d > inner) {
                        covers += 1;
                    }
                }
                total_samples += 1;
                if covers > 1 {
                    double_covered_samples += 1;
                }
            }
        }
        let pct = 100.0 * double_covered_samples as f32 / total_samples as f32;
        eprintln!("sampled disk points covered by >1 ring: {double_covered_samples}/{total_samples} ({pct:.1}%)");
        // แบ่งตามระยะศูนย์กลาง tile ล้วนๆ — overlap ที่เหลือคือ tile สี่เหลี่ยม
        // คร่อมเส้นแบ่งวงแบบวงกลมพอดี (หลีกเลี่ยงไม่ได้เว้นแต่เช็คด้วย bounding
        // box แทน) เหลือแค่แถบบางๆ ตรงรอยต่อ 2 วง ไม่ใช่พื้นที่กว้างแบบเดิม (~4-14%)
        assert!(pct < 2.0, "ring overlap สูงผิดปกติ: {pct:.1}%");
    }
}

/// จัดการ tile รอบกล้อง: spawn task ที่ขาด, รับผลมาเป็น entity, เก็บที่หลุดระยะ
pub fn update_lod_tiles(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<crate::GameSettings>,
    regenerate: Res<crate::RegenerateWorld>,
    mut lod: ResMut<LodTiles>,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, With<crate::camera::FreeCamera>>,
) {
    let lod = &mut *lod;

    // ---- รับผลจาก task (ทุกเฟรม — ผลมาถึงแล้วโชว์เลย) ----
    loop {
        let res = { lod.receiver.lock().unwrap().try_recv() };
        let Ok(res) = res else { break };
        let key = (res.ring, res.coord);
        lod.pending.remove(&key);
        if res.version != lod.version || lod.tiles.contains_key(&key) {
            continue; // ผลรุ่นเก่า (สลับโลกไปแล้ว) — ทิ้ง
        }
        let (_, tile_size, _) = LOD_RINGS[res.ring];
        let entity = commands
            .spawn((
                Mesh3d(meshes.add(res.buf.into_mesh())),
                MeshMaterial3d(lod.material.clone()),
                Transform::from_xyz(
                    (res.coord.x * tile_size) as f32,
                    0.0,
                    (res.coord.y * tile_size) as f32,
                ),
            ))
            .id();
        lod.tiles.insert(key, entity);
    }

    // ---- เช็คชุด tile ที่ควรมี (เป็นคาบ ไม่ใช่ทุกเฟรม) ----
    lod.timer += time.delta_secs();
    if lod.timer < UPDATE_PERIOD {
        return;
    }
    lod.timer = 0.0;
    lod.passes += 1;
    if lod.passes % 10 == 0 {
        info!("LOD: {} tiles, {} pending", lod.tiles.len(), lod.pending.len());
    }

    let spec = HeightSourceSpec::from_settings(&settings);
    let clear_all = !settings.lod_enabled
        || regenerate.0
        || lod.last_spec.is_some_and(|s| s != spec);
    if clear_all {
        for (_, e) in lod.tiles.drain() {
            commands.entity(e).despawn();
        }
        lod.pending.clear();
        lod.version = lod.version.wrapping_add(1);
        lod.last_spec = Some(spec);
        if !settings.lod_enabled {
            return;
        }
        // regenerate เฟรมนี้: รอรอบหน้าให้โลก/ค่านิ่งก่อนค่อยสร้างใหม่
        if regenerate.0 {
            return;
        }
    }
    lod.last_spec = Some(spec);

    let Ok(cam) = camera.single() else { return };
    let cam_pos = cam.translation;

    // ชุดที่ควรมี: ต่อวง เอา tile ที่ "ศูนย์กลางไกลกว่าวงใน" และอยู่ในรัศมีวง/สไลเดอร์
    let mut desired: HashSet<(usize, IVec2)> = HashSet::new();
    for (ring, (_cell, tile_size, radius)) in LOD_RINGS.iter().enumerate() {
        let radius = radius.min(settings.lod_distance_m);
        if radius <= 0.0 {
            continue;
        }
        let inner = if ring == 0 { 0.0 } else { LOD_RINGS[ring - 1].2.min(settings.lod_distance_m) };
        let t = *tile_size as f32;
        let (min_tx, max_tx) = (
            ((cam_pos.x - radius) / t).floor() as i32,
            ((cam_pos.x + radius) / t).floor() as i32,
        );
        let (min_tz, max_tz) = (
            ((cam_pos.z - radius) / t).floor() as i32,
            ((cam_pos.z + radius) / t).floor() as i32,
        );
        for tz in min_tz..=max_tz {
            for tx in min_tx..=max_tx {
                let center = Vec2::new((tx as f32 + 0.5) * t, (tz as f32 + 0.5) * t);
                let d = center.distance(Vec2::new(cam_pos.x, cam_pos.z));
                // แบ่งวงตามระยะศูนย์กลาง tile ล้วนๆ (ไม่บวก half-diagonal ของ tile
                // เข้าไปอีก — เดิมบวกทำให้วงหยาบเริ่มวาดล้ำเข้าเขตวงละเอียดเป็นแถบ
                // กว้างหลักพันบล็อก สอง mesh ซ้อนทับกันแล้ว z-fight เป็นเส้นรอยแตก
                // ที่เห็น) เหลือรอยต่อแค่ระดับกริด ~ครึ่ง tile ให้กระโปรงขอบปิดพอ
                if d <= radius && (ring == 0 || d > inner) {
                    desired.insert((ring, IVec2::new(tx, tz)));
                }
            }
        }
    }

    // เก็บที่หลุดระยะ
    let stale: Vec<_> = lod.tiles.keys().filter(|k| !desired.contains(k)).copied().collect();
    for k in stale {
        if let Some(e) = lod.tiles.remove(&k) {
            commands.entity(e).despawn();
        }
    }

    // spawn task ที่ขาด (จำกัดงานคงค้าง)
    for key in desired {
        if lod.tiles.contains_key(&key)
            || lod.pending.contains(&key)
            || lod.pending.len() >= MAX_TASKS_IN_FLIGHT
        {
            continue;
        }
        lod.pending.insert(key);
        let sender = lod.sender.lock().unwrap().clone();
        let version = lod.version;
        let (ring, coord) = key;
        AsyncComputeTaskPool::get()
            .spawn(async move {
                if let Some(source) = HeightSource::build(spec) {
                    let buf = build_tile(&source, ring, coord);
                    let _ = sender.send(LodTileResult { ring, coord, version, buf });
                }
            })
            .detach();
    }
}
