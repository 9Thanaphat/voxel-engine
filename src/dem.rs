use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

// --------------------------------------------------------
// DEM (Digital Elevation Model) — ภูมิประเทศโลกจริง 1 บล็อก = 1 เมตร
// ทั้งประเทศไทยแบบ streaming: แต่ละ tile = 1° Copernicus GLO-30 (3600×3600 px)
// เก็บเป็นไฟล์ assets/dem/tiles/N##E###.r16 ต่อ tile + manifest.json (รายชื่อ tile ที่มี)
//
// พิกัดโลก (global equirectangular เชิงเส้น, ครอบไทย): block (0,0) = มุมบนซ้าย
// (ORIGIN_LAT/LON) สเกล N-S คงที่, E-W ใช้ละติจูดอ้างอิง REF_LAT ให้ transform เป็น
// เชิงเส้นทั้งแผนที่ (E-W เพี้ยน ~3% ที่ขอบเหนือ/ใต้ — ยอมรับได้บนระนาบแบน)
//
// f32: ไทยกว้างสุด ~1.7M บล็อก (N-S) < 16.7M ที่ f32 แม่นระดับ <1 บล็อก → ไม่ต้อง
// floating origin (ต่างจากทั้งโลก 40M)
// --------------------------------------------------------

/// ระดับน้ำทะเลจริง (elevation 0 ม.) อยู่ที่ y นี้ในเกม
pub const DEM_SEA_LEVEL_Y: i32 = 64;

const ORIGIN_LAT: f64 = 21.0; // มุมบนของกล่องไทย
const ORIGIN_LON: f64 = 97.0; // มุมซ้ายของกล่องไทย
const REF_LAT: f64 = 13.0; // ละติจูดอ้างอิงสเกล E-W (กลางไทย)
const M_PER_DEG_LAT: f64 = 110_574.0;
fn m_per_deg_lon() -> f64 {
    111_320.0 * REF_LAT.to_radians().cos()
}

// ---- พิกัดโลก ↔ lat/lon (free function — global projection ไม่ผูก tile) ----

pub fn block_to_latlon(bx: f64, bz: f64) -> (f64, f64) {
    let lat = ORIGIN_LAT - bz / M_PER_DEG_LAT;
    let lon = ORIGIN_LON + bx / m_per_deg_lon();
    (lat, lon)
}

pub fn latlon_to_block(lat: f64, lon: f64) -> (f64, f64) {
    let bz = (ORIGIN_LAT - lat) * M_PER_DEG_LAT;
    let bx = (lon - ORIGIN_LON) * m_per_deg_lon();
    (bx, bz)
}

// ---- tile identity + ไฟล์ ----

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TileId {
    pub lat: i32, // floor latitude (ขอบล่างของ tile)
    pub lon: i32, // floor longitude (ขอบซ้าย)
}

impl TileId {
    /// ชื่อไฟล์แบบ Copernicus: N18E098 (มุมล่างซ้าย)
    fn file_stem(&self) -> String {
        let (ns, alat) = if self.lat < 0 { ('S', -self.lat) } else { ('N', self.lat) };
        let (ew, alon) = if self.lon < 0 { ('W', -self.lon) } else { ('E', self.lon) };
        format!("{ns}{alat:02}{ew}{alon:03}")
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DemManifest {
    pub tile_px: usize,   // 3600 (GLO-30 1°)
    pub value_scale: f32, // เมตรต่อ 1 หน่วยใน r16 (0.1)
    pub tiles: Vec<[i32; 2]>, // [lat, lon] ของ tile ที่มีไฟล์จริง
}

pub struct DemTile {
    samples: Vec<u16>, // tile_px × tile_px (บน→ล่าง, ซ้าย→ขวา)
    /// water mask จาก OSM — bitpacked 1 bit/px (ว่าง = ไม่มีไฟล์ .wmask = ไม่มีน้ำ)
    water: Vec<u8>,
}

/// อ่านไฟล์ tile (.r16 + .wmask ถ้ามี) — คืน None ถ้า .r16 หาย/ขนาดผิด
fn read_tile(px: usize, id: TileId) -> Option<DemTile> {
    let raw = std::fs::read(tiles_dir().join(format!("{}.r16", id.file_stem()))).ok()?;
    if raw.len() != px * px * 2 {
        return None;
    }
    let samples = raw.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
    // water mask (optional) — bitpacked, ขนาด ceil(px²/8)
    let water = std::fs::read(dem_dir().join("water").join(format!("{}.wmask", id.file_stem())))
        .ok()
        .filter(|w| w.len() == px * px / 8)
        .unwrap_or_default();
    Some(DemTile { samples, water })
}

enum TileSlot {
    Loading,
    Ready(Arc<DemTile>),
    Missing, // ไม่มีใน manifest (ทะเล) — นับเป็น "พร้อม" (= ระดับน้ำทะเล)
}

pub struct DemStreamer {
    manifest: DemManifest,
    available: std::collections::HashSet<TileId>,
    tiles: RwLock<HashMap<TileId, TileSlot>>,
}

static DEM: OnceLock<Option<DemStreamer>> = OnceLock::new();

fn dem_dir() -> std::path::PathBuf {
    crate::voxel::project_root().join("assets").join("dem")
}
fn tiles_dir() -> std::path::PathBuf {
    dem_dir().join("tiles")
}

/// streamer ของโลกจริง (None = ไม่มี manifest — ปุ่ม Real World disabled)
pub fn streamer() -> Option<&'static DemStreamer> {
    DEM.get_or_init(|| {
        let bytes = std::fs::read(dem_dir().join("manifest.json")).ok()?;
        let manifest: DemManifest = serde_json::from_slice(&bytes).ok()?;
        if manifest.tiles.is_empty() {
            return None;
        }
        let available = manifest.tiles.iter().map(|t| TileId { lat: t[0], lon: t[1] }).collect();
        info!("DEM manifest: {} tiles, {}px, scale {}", manifest.tiles.len(), manifest.tile_px, manifest.value_scale);
        Some(DemStreamer { manifest, available, tiles: RwLock::new(HashMap::new()) })
    })
    .as_ref()
}

impl DemStreamer {
    fn px(&self) -> i64 {
        self.manifest.tile_px as i64
    }

    pub fn center_block(&self) -> (f64, f64) {
        if let Some(t) = self.available.iter().next() {
            let lat = t.lat as f64 + 0.5;
            let lon = t.lon as f64 + 0.5;
            crate::dem::latlon_to_block(lat, lon)
        } else {
            (0.0, 0.0)
        }
    }

    /// lat/lon นี้อยู่ในกล่องที่มี tile ไหม (ไว้เช็ค teleport / bounds)
    pub fn has_tile_at(&self, lat: f64, lon: f64) -> bool {
        self.available.contains(&TileId { lat: lat.floor() as i32, lon: lon.floor() as i32 })
    }

    /// tile ที่ chunk/พื้นที่รอบพิกัดบล็อกนี้ต้องใช้ (เผื่อ bilinear ข้ามขอบ tile
    /// จึงคืน tile ที่ครอบ (bx±margin, bz±margin))
    pub fn tiles_covering(&self, bx0: f64, bz0: f64, bx1: f64, bz1: f64) -> Vec<TileId> {
        let (lat_a, lon_a) = block_to_latlon(bx0, bz1); // ล่างซ้าย → lat ต่ำ, lon ต่ำ
        let (lat_b, lon_b) = block_to_latlon(bx1, bz0); // บนขวา → lat สูง, lon สูง
        let mut out = Vec::new();
        for lat in lat_a.floor() as i32..=lat_b.floor() as i32 {
            for lon in lon_a.floor() as i32..=lon_b.floor() as i32 {
                out.push(TileId { lat, lon });
            }
        }
        out
    }

    /// ขอโหลด tile (เรียกจาก main thread) — spawn task อ่านไฟล์เข้ามาใน cache
    pub fn request(&'static self, id: TileId) {
        {
            let r = self.tiles.read().unwrap();
            if r.contains_key(&id) {
                return; // มี state อยู่แล้ว (Loading/Ready/Missing)
            }
        }
        if !self.available.contains(&id) {
            self.tiles.write().unwrap().insert(id, TileSlot::Missing);
            return;
        }
        self.tiles.write().unwrap().insert(id, TileSlot::Loading);
        let px = self.manifest.tile_px;
        AsyncComputeTaskPool::get()
            .spawn(async move {
                let slot = match read_tile(px, id) {
                    Some(t) => TileSlot::Ready(Arc::new(t)),
                    None => TileSlot::Missing,
                };
                if let Some(Some(s)) = DEM.get() {
                    s.tiles.write().unwrap().insert(id, slot);
                }
            })
            .detach();
    }

    /// โหลด tile ที่ครอบพิกัดนี้แบบ blocking (อ่านไฟล์ตรงในเธรดนี้) — ใช้ตอน spawn
    /// ที่ต้องรู้ความสูงผิวทันที ไม่งั้น elevation คืน 0 (ทะเล) เพราะ tile ยังโหลด async
    pub fn load_blocking_at(&self, bx: f64, bz: f64) {
        for id in self.tiles_covering(bx, bz, bx, bz) {
            let already = matches!(
                self.tiles.read().unwrap().get(&id),
                Some(TileSlot::Ready(_)) | Some(TileSlot::Missing)
            );
            if already {
                continue;
            }
            let slot = if !self.available.contains(&id) {
                TileSlot::Missing
            } else {
                match read_tile(self.manifest.tile_px, id) {
                    Some(t) => TileSlot::Ready(Arc::new(t)),
                    None => TileSlot::Missing,
                }
            };
            self.tiles.write().unwrap().insert(id, slot);
        }
    }

    /// tile ทั้งหมดที่พื้นที่นี้ต้องใช้พร้อมหรือยัง (Ready หรือ Missing) — ถ้าไม่ ขอโหลด
    /// ให้ (return false) เรียกจาก main thread ก่อนสั่ง generate chunk
    pub fn ensure_ready(&'static self, bx0: f64, bz0: f64, bx1: f64, bz1: f64) -> bool {
        let mut all = true;
        for id in self.tiles_covering(bx0, bz0, bx1, bz1) {
            let ready = {
                let r = self.tiles.read().unwrap();
                matches!(r.get(&id), Some(TileSlot::Ready(_)) | Some(TileSlot::Missing))
            };
            if !ready {
                self.request(id);
                all = false;
            }
        }
        all
    }

    /// ความสูงพิกเซล global (col=ตะวันออก, row=ใต้จากขั้วโลกเหนือ) — None ถ้า tile
    /// ยังไม่ Ready (Missing/Loading → ให้ผู้เรียกใช้ 0 = ทะเล)
    fn pixel(&self, tiles: &HashMap<TileId, TileSlot>, gc: i64, gr: i64) -> Option<u16> {
        let px = self.px();
        let id = TileId {
            lon: gc.div_euclid(px) as i32,
            lat: 89 - gr.div_euclid(px) as i32,
        };
        match tiles.get(&id) {
            Some(TileSlot::Ready(t)) => {
                let lc = gc.rem_euclid(px) as usize;
                let lr = gr.rem_euclid(px) as usize;
                Some(t.samples[lr * self.manifest.tile_px + lc])
            }
            _ => None,
        }
    }

    /// ความสูง (เมตร) ณ พิกัดบล็อก — bilinear ข้าม tile ได้ (อ่าน cache เฉยๆ ไม่ trigger
    /// โหลด ให้เรียกจาก worker task ได้ปลอดภัย); tile ยังไม่โหลด = ทะเล (0 ม.)
    pub fn elevation_at_block(&self, bx: f64, bz: f64) -> f32 {
        let (lat, lon) = block_to_latlon(bx, bz);
        let px = self.manifest.tile_px as f64;
        let fc = lon * px - 0.5;
        let fr = (90.0 - lat) * px - 0.5;
        let gc0 = fc.floor() as i64;
        let gr0 = fr.floor() as i64;
        let wc = (fc - gc0 as f64) as f32;
        let wr = (fr - gr0 as f64) as f32;
        let tiles = self.tiles.read().unwrap();
        let s = |gc: i64, gr: i64| {
            self.pixel(&tiles, gc, gr).unwrap_or(0) as f32 * self.manifest.value_scale
        };
        let h00 = s(gc0, gr0);
        let h10 = s(gc0 + 1, gr0);
        let h01 = s(gc0, gr0 + 1);
        let h11 = s(gc0 + 1, gr0 + 1);
        let top = h00 + (h10 - h00) * wc;
        let bot = h01 + (h11 - h01) * wc;
        top + (bot - top) * wr
    }

    /// พิกเซลนี้เป็นน้ำ (OSM mask) ไหม — nearest-pixel (ไม่ต้อง bilinear กับ mask)
    /// tile ยังไม่โหลด/ไม่มี wmask = ไม่ใช่น้ำ; อ่าน cache เฉยๆ ปลอดภัยจาก worker
    pub fn is_water_at_block(&self, bx: f64, bz: f64) -> bool {
        let (lat, lon) = block_to_latlon(bx, bz);
        let px = self.manifest.tile_px as f64;
        // pixel ที่ครอบ (ปัดลง ไม่ใช่ center-index เพราะ mask เป็น boolean)
        let gc = (lon * px).floor() as i64;
        let gr = ((90.0 - lat) * px).floor() as i64;
        let ipx = self.px();
        let id = TileId {
            lon: gc.div_euclid(ipx) as i32,
            lat: 89 - gr.div_euclid(ipx) as i32,
        };
        let tiles = self.tiles.read().unwrap();
        if let Some(TileSlot::Ready(t)) = tiles.get(&id) {
            if t.water.is_empty() {
                return false;
            }
            let lc = gc.rem_euclid(ipx) as usize;
            let lr = gr.rem_euclid(ipx) as usize;
            let bit = lr * self.manifest.tile_px + lc;
            return (t.water[bit / 8] >> (bit % 8)) & 1 == 1;
        }
        false
    }
}

/// ขอโหลด tile ในรัศมีรอบผู้เล่น (รวมระยะ LOD) เป็นระยะๆ — worldgen ก็ ensure_ready
/// เองต่อ chunk อยู่แล้ว แต่ระบบนี้ครอบระยะไกล (LOD) ที่ worldgen ไม่แตะ ให้ภูเขา
/// ไกลๆ มีข้อมูลโหลดมาด้วย (task เขียนผลเข้า RwLock cache เองผ่าน request)
pub fn dem_stream_system(
    settings: Res<crate::GameSettings>,
    camera: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut timer: Local<f32>,
    time: Res<Time>,
) {
    if settings.terrain_source != crate::TerrainSource::RealWorld {
        return;
    }
    // เช็ควินาทีละครั้งพอ (tile 1° ใหญ่มาก ไม่ต้องถี่)
    *timer += time.delta_secs();
    if *timer < 1.0 {
        return;
    }
    *timer = 0.0;
    let Some(dem) = streamer() else { return };
    let Ok(cam) = camera.single() else { return };
    let r = ((settings.lod_distance_chunks * crate::voxel::CHUNK_WIDTH as i32) as f64).min(40_000.0);
    let (px, pz) = (cam.translation.x as f64, cam.translation.z as f64);
    dem.ensure_ready(px - r, pz - r, px + r, pz + r);
}

// --------------------------------------------------------
// เครื่องมือ build DEM: ดาวน์โหลด + แปลง Copernicus tile เป็น r16 + manifest
// `voxel-game --build-dem <lat0> <lat1> <lon0> <lon1>`
// วนทุก tile 1° ในช่วง (inclusive) → มีไฟล์แล้วข้าม, ไม่มีก็ดาวน์โหลดจาก S3 → แปลง
// tile ที่ S3 ไม่มี (ทะเล) → ข้าม, ไม่ใส่ใน manifest
// --------------------------------------------------------

pub fn build_dem_cli(lat0: i32, lat1: i32, lon0: i32, lon1: i32) {
    std::fs::create_dir_all(tiles_dir()).unwrap();
    let mut tiles: Vec<[i32; 2]> = Vec::new();
    let tile_px = 3600usize;
    let value_scale = 0.1f32;

    for lat in lat0..=lat1 {
        for lon in lon0..=lon1 {
            let id = TileId { lat, lon };
            let out = tiles_dir().join(format!("{}.r16", id.file_stem()));
            if out.exists() && std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0) == (tile_px * tile_px * 2) as u64 {
                println!("{} มีแล้ว ข้าม", id.file_stem());
                tiles.push([lat, lon]);
                continue;
            }
            match download_and_convert(id, tile_px, value_scale) {
                Some(()) => {
                    println!("{} เสร็จ", id.file_stem());
                    tiles.push([lat, lon]);
                }
                None => println!("{} ไม่มีข้อมูล (ทะเล/นอกพื้นที่) ข้าม", id.file_stem()),
            }
        }
    }

    // รวมกับ manifest เดิม (ถ้ามี) แล้วเขียนใหม่
    let mut set: std::collections::BTreeSet<[i32; 2]> = tiles.into_iter().collect();
    if let Ok(bytes) = std::fs::read(dem_dir().join("manifest.json")) {
        if let Ok(m) = serde_json::from_slice::<DemManifest>(&bytes) {
            set.extend(m.tiles);
        }
    }
    let manifest = DemManifest { tile_px, value_scale, tiles: set.into_iter().collect() };
    std::fs::write(dem_dir().join("manifest.json"), serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    println!("เขียน manifest: {} tiles ทั้งหมด", manifest.tiles.len());
}

/// ดาวน์โหลด GeoTIFF tile จาก Copernicus S3 แล้วแปลงเป็น r16 — None ถ้า S3 ไม่มี tile นี้
fn download_and_convert(id: TileId, tile_px: usize, value_scale: f32) -> Option<()> {
    let stem = id.file_stem(); // N18E098
    // path S3 ของ Copernicus GLO-30: Copernicus_DSM_COG_10_N18_00_E098_00_DEM/...DEM.tif
    let (ns, alat) = if id.lat < 0 { ("S", -id.lat) } else { ("N", id.lat) };
    let (ew, alon) = if id.lon < 0 { ("W", -id.lon) } else { ("E", id.lon) };
    let dir = format!("Copernicus_DSM_COG_10_{ns}{alat:02}_00_{ew}{alon:03}_00_DEM");
    let url = format!("https://copernicus-dem-30m.s3.amazonaws.com/{dir}/{dir}.tif");
    let tmp = tiles_dir().join(format!("{stem}.tif.tmp"));

    println!("ดาวน์โหลด {stem} ...");
    let status = std::process::Command::new("curl")
        .args(["-sfL", "--max-time", "300", "-o"])
        .arg(&tmp)
        .arg(&url)
        .status();
    match status {
        Ok(s) if s.success() => {}
        _ => {
            let _ = std::fs::remove_file(&tmp);
            return None; // 404 = ทะเล ไม่มี tile
        }
    }

    let result = convert_tif_to_r16(&tmp, id, tile_px, value_scale);
    let _ = std::fs::remove_file(&tmp);
    result
}

fn convert_tif_to_r16(tif: &std::path::Path, id: TileId, tile_px: usize, value_scale: f32) -> Option<()> {
    let file = std::fs::File::open(tif).ok()?;
    let mut decoder = tiff::decoder::Decoder::new(std::io::BufReader::new(file)).ok()?;
    let (w, h) = decoder.dimensions().ok()?;
    if w as usize != tile_px || h as usize != tile_px {
        eprintln!("  ขนาด {}×{} ไม่ตรง {} — ข้าม", w, h, tile_px);
        return None;
    }
    let img = decoder.read_image().ok()?;
    let samples: Vec<u16> = match img {
        tiff::decoder::DecodingResult::F32(v) => {
            v.into_iter().map(|m| (m.max(0.0) / value_scale).min(u16::MAX as f32) as u16).collect()
        }
        tiff::decoder::DecodingResult::I16(v) => {
            v.into_iter().map(|m| ((m.max(0) as f32) / value_scale).min(u16::MAX as f32) as u16).collect()
        }
        tiff::decoder::DecodingResult::U16(v) => {
            v.into_iter().map(|m| ((m as f32) / value_scale).min(u16::MAX as f32) as u16).collect()
        }
        _ => return None,
    };
    if samples.len() != tile_px * tile_px {
        return None;
    }
    let mut raw = Vec::with_capacity(samples.len() * 2);
    for s in &samples {
        raw.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(tiles_dir().join(format!("{}.r16", id.file_stem())), raw).ok()?;
    Some(())
}

// --------------------------------------------------------
// เครื่องมือ build water mask จาก OSM: `--build-water <lat0> <lat1> <lon0> <lon1>`
// ต่อ tile 1°: ดึงน้ำจาก Overpass → rasterize เข้า grid 3600² → .wmask (bitpacked)
// --------------------------------------------------------

fn water_dir() -> std::path::PathBuf {
    dem_dir().join("water")
}

pub fn build_water_cli(lat0: i32, lat1: i32, lon0: i32, lon1: i32) {
    std::fs::create_dir_all(water_dir()).unwrap();
    let tile_px = 3600usize;
    for lat in lat0..=lat1 {
        for lon in lon0..=lon1 {
            let id = TileId { lat, lon };
            // มี .wmask แล้วข้าม (รันซ้ำเพื่อเก็บ tile ที่ยังขาดได้)
            if water_dir().join(format!("{}.wmask", id.file_stem())).exists() {
                println!("{} มีแล้ว ข้าม", id.file_stem());
                continue;
            }
            // retry — Overpass public API rate-limit ถ้ายิงถี่ (429/timeout)
            let mut done = false;
            for attempt in 1..=4 {
                if let Some(n) = build_water_tile(id, tile_px) {
                    println!("{} เสร็จ ({} water pixels)", id.file_stem(), n);
                    done = true;
                    break;
                }
                println!("{} ลองใหม่ ({}/4) — รอ Overpass", id.file_stem(), attempt);
                std::thread::sleep(std::time::Duration::from_secs(20));
            }
            if !done {
                println!("{} ข้าม (ดึง/parse ไม่ได้หลัง retry)", id.file_stem());
            }
            // เว้นจังหวะระหว่าง tile กัน rate-limit
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    }
    println!("เสร็จทั้งหมด");
}

/// ดึง OSM + rasterize water mask ของ tile หนึ่ง → คืนจำนวนพิกเซลน้ำ (None ถ้าพัง)
fn build_water_tile(id: TileId, tile_px: usize) -> Option<usize> {
    let (s, w, n, e) = (id.lat, id.lon, id.lat + 1, id.lon + 1);
    let query = format!(
        "[out:json][timeout:180];\
         (way[\"natural\"=\"water\"]({s},{w},{n},{e});\
          way[\"water\"]({s},{w},{n},{e});\
          way[\"waterway\"~\"^(river|canal|stream|drain|ditch|riverbank)$\"]({s},{w},{n},{e}););\
         out geom;"
    );
    let tmp = water_dir().join(format!("{}.json.tmp", id.file_stem()));
    println!("ดึง OSM {} ...", id.file_stem());
    let status = std::process::Command::new("curl")
        .args(["-s", "--max-time", "200", "-o"])
        .arg(&tmp)
        .args(["--data", &query, "https://overpass-api.de/api/interpreter"])
        .status();
    if !matches!(status, Ok(s) if s.success()) {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    let bytes = std::fs::read(&tmp).ok()?;
    let _ = std::fs::remove_file(&tmp);
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let elements = json["elements"].as_array()?;

    let mut mask = vec![0u8; tile_px * tile_px / 8];
    let set = |mask: &mut [u8], px: i64, py: i64| {
        if px >= 0 && py >= 0 && (px as usize) < tile_px && (py as usize) < tile_px {
            let bit = py as usize * tile_px + px as usize;
            mask[bit / 8] |= 1 << (bit % 8);
        }
    };
    // lat/lon → พิกเซล tile (px=ตะวันออก, py=ใต้จากขอบบน)
    let to_px = |lat: f64, lon: f64| -> (f64, f64) {
        ((lon - id.lon as f64) * tile_px as f64, (id.lat as f64 + 1.0 - lat) * tile_px as f64)
    };

    for el in elements {
        if el["type"] != "way" {
            continue;
        }
        let tags = &el["tags"];
        let Some(geom) = el["geometry"].as_array() else { continue };
        let pts: Vec<(f64, f64)> = geom
            .iter()
            .filter_map(|g| Some(to_px(g["lat"].as_f64()?, g["lon"].as_f64()?)))
            .collect();
        if pts.len() < 2 {
            continue;
        }
        let tag = |k: &str| tags[k].as_str();
        let is_area = tag("natural") == Some("water")
            || tags["water"].is_string()
            || matches!(tag("waterway"), Some("riverbank") | Some("dock"));
        if is_area {
            fill_polygon(&mut mask, tile_px, &pts, &set);
        } else if let Some(ww) = tag("waterway") {
            let width_m = tags["width"]
                .as_str()
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(match ww {
                    "river" => 40.0,
                    "canal" => 10.0,
                    "stream" => 4.0,
                    _ => 2.0, // drain/ditch
                });
            let r_px = (width_m / 2.0 / 30.0).max(1.0) as i64;
            draw_thick_line(&mut mask, &pts, r_px, &set);
        }
    }

    let count = mask.iter().map(|b| b.count_ones() as usize).sum();
    std::fs::write(water_dir().join(format!("{}.wmask", id.file_stem())), &mask).ok()?;
    Some(count)
}

/// เติมรูปหลายเหลี่ยม (even-odd scanline) — pts เป็นพิกัดพิกเซล
fn fill_polygon(mask: &mut [u8], tile_px: usize, pts: &[(f64, f64)], set: &impl Fn(&mut [u8], i64, i64)) {
    let (mut y_min, mut y_max) = (f64::MAX, f64::MIN);
    for &(_, y) in pts {
        y_min = y_min.min(y);
        y_max = y_max.max(y);
    }
    let y0 = (y_min.floor() as i64).max(0);
    let y1 = (y_max.ceil() as i64).min(tile_px as i64 - 1);
    for py in y0..=y1 {
        let yc = py as f64 + 0.5;
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..pts.len() {
            let (x0, ya) = pts[i];
            let (x1, yb) = pts[(i + 1) % pts.len()];
            if (ya <= yc && yb > yc) || (yb <= yc && ya > yc) {
                xs.push(x0 + (yc - ya) / (yb - ya) * (x1 - x0));
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        for pair in xs.chunks_exact(2) {
            let (xa, xb) = (pair[0].floor() as i64, pair[1].ceil() as i64);
            for px in xa..=xb {
                set(mask, px, py);
            }
        }
    }
}

/// วาดเส้นหนา (stamp วงกลมรัศมี r ตามแนวเส้น) — pts เป็นพิกัดพิกเซล
fn draw_thick_line(mask: &mut [u8], pts: &[(f64, f64)], r: i64, set: &impl Fn(&mut [u8], i64, i64)) {
    let stamp = |mask: &mut [u8], cx: i64, cy: i64| {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    set(mask, cx + dx, cy + dy);
                }
            }
        }
    };
    for seg in pts.windows(2) {
        let (x0, y0) = seg[0];
        let (x1, y1) = seg[1];
        let len = ((x1 - x0).hypot(y1 - y0)).ceil().max(1.0);
        for k in 0..=len as i64 {
            let t = k as f64 / len;
            stamp(mask, (x0 + (x1 - x0) * t).round() as i64, (y0 + (y1 - y0) * t).round() as i64);
        }
    }
}
