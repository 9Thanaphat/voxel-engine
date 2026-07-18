use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

use crate::voxel::{MeshBuf, TerrainSampler, SEA_LEVEL};

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
/// กระโปรงขอบ tile ห้อยลง (คูณ cell) — ปิดรอยแยกระหว่างวง/ระหว่าง tile
const SKIRT_DROP_CELLS: f32 = 1.5;
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

    /// สีผิวระยะไกล: ทราย (ใกล้ทะเล/desert noise) / หญ้า / หินเมื่อชัน
    fn surface_color(&self, wx: f64, wz: f64, h: f32, slope: f32) -> [f32; 4] {
        if slope > 0.9 {
            return [0.42, 0.40, 0.38, 1.0]; // หน้าผา/เขาชัน
        }
        let sandy = match self {
            Self::Noise(s) => h <= SEA_LEVEL as f32 + 2.0 || s.is_desert(wx, wz),
            Self::Dem(_) => h <= crate::dem::DEM_SEA_LEVEL_Y as f32 + 2.0,
        };
        if sandy {
            [0.76, 0.69, 0.50, 1.0]
        } else {
            // หญ้า — สูงขึ้นซีด/เขียวเข้มต่ำ (ไล่เฉดตามความสูงให้ภูเขามีมิติ)
            let t = ((h - self.sea_level()) / 1500.0).clamp(0.0, 1.0);
            [0.24 + 0.14 * t, 0.46 - 0.10 * t, 0.20 + 0.08 * t, 1.0]
        }
    }
}

const WATER_COLOR: [f32; 4] = [0.10, 0.28, 0.52, 1.0];

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

/// สร้าง mesh ของ tile หนึ่งใบ (รันใน background task)
/// heightfield แบบ quad ต่อ cell (flat shaded — มองไกลสวยแบบ low-poly)
/// + ผิวน้ำที่ sea level + กระโปรงขอบกันรอยแยก
fn build_tile(source: &HeightSource, ring: usize, coord: IVec2) -> MeshBuf {
    let (cell, tile_size, _) = LOD_RINGS[ring];
    let n = (tile_size as f32 / cell) as usize; // จำนวน cell ต่อด้าน
    let origin_x = coord.x as f64 * tile_size as f64;
    let origin_z = coord.y as f64 * tile_size as f64;
    let sea = source.sea_level();

    // sample มุม cell (n+1)² จุด
    let mut hs = vec![0f32; (n + 1) * (n + 1)];
    for j in 0..=n {
        for i in 0..=n {
            hs[j * (n + 1) + i] = source.height(
                origin_x + i as f64 * cell as f64,
                origin_z + j as f64 * cell as f64,
            ) + LOD_Y_OFFSET;
        }
    }
    let h_at = |i: usize, j: usize| hs[j * (n + 1) + i];

    let mut buf = MeshBuf::default();
    let mut push_quad = |verts: [[f32; 3]; 4], col: [f32; 4]| {
        // normal จากมุม quad (flat) — พอสำหรับแสงระยะไกล
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
    };

    for j in 0..n {
        for i in 0..n {
            let (x0, x1) = (i as f32 * cell, (i + 1) as f32 * cell);
            let (z0, z1) = (j as f32 * cell, (j + 1) as f32 * cell);
            let (h00, h10, h01, h11) = (h_at(i, j), h_at(i + 1, j), h_at(i, j + 1), h_at(i + 1, j + 1));
            let h_avg = (h00 + h10 + h01 + h11) * 0.25;

            // ผิวดิน (ใต้น้ำก็วาด — เห็นก้นทะเลสีเข้มผ่านน้ำไม่ได้เพราะ opaque
            // แต่กันรูริมฝั่ง; ราคาถูกไม่คัดทิ้ง)
            let slope = ((h00 - h11).abs().max((h10 - h01).abs())) / cell;
            let wx = origin_x + (x0 + x1) as f64 * 0.5;
            let wz = origin_z + (z0 + z1) as f64 * 0.5;
            let col = source.surface_color(wx, wz, h_avg, slope);
            push_quad(
                [[x0, h00, z0], [x1, h10, z0], [x1, h11, z1], [x0, h01, z1]],
                col,
            );

            // ผิวน้ำแบนที่ sea level เมื่อ cell จมน้ำ
            if h_avg < sea {
                let wy = sea + LOD_Y_OFFSET + 0.5;
                push_quad(
                    [[x0, wy, z0], [x1, wy, z0], [x1, wy, z1], [x0, wy, z1]],
                    WATER_COLOR,
                );
            }
        }
    }

    // กระโปรงขอบ tile: ผนังดิ่งห้อยลงตามแนวขอบทั้ง 4 ด้าน ปิดรอยแยกระหว่างวง
    let drop = cell * SKIRT_DROP_CELLS;
    let edge_col = [0.35, 0.33, 0.31, 1.0];
    for k in 0..n {
        let (a, b) = (k as f32 * cell, (k + 1) as f32 * cell);
        let t = tile_size as f32;
        // ขอบ z=0, z=t, x=0, x=t
        let pairs: [([f32; 3], [f32; 3]); 4] = [
            ([a, h_at(k, 0), 0.0], [b, h_at(k + 1, 0), 0.0]),
            ([b, h_at(k + 1, n), t], [a, h_at(k, n), t]),
            ([0.0, h_at(0, n - k), (n - k) as f32 * cell], [0.0, h_at(0, n - k - 1), (n - k - 1) as f32 * cell]),
            ([t, h_at(n, k), a], [t, h_at(n, k + 1), b]),
        ];
        for (p0, p1) in pairs {
            push_quad(
                [p0, p1, [p1[0], p1[1] - drop, p1[2]], [p0[0], p0[1] - drop, p0[2]]],
                edge_col,
            );
        }
    }

    buf
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
                // วงนอกเว้นพื้นที่ที่วงในละเอียดกว่าคลุมอยู่ (กันวาดซ้อนหนา)
                if d <= radius && (ring == 0 || d + t * 0.71 > inner) {
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
