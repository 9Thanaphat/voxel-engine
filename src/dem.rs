use bevy::prelude::*;
use std::sync::OnceLock;

// --------------------------------------------------------
// DEM (Digital Elevation Model) — ภูมิประเทศโลกจริง 1 บล็อก = 1 เมตร
// ข้อมูล: Copernicus GLO-30 tile (GeoTIFF float32 เมตร) แปลงครั้งเดียวเป็น
// heightmap.r16 (u16 LE, 1 หน่วย = 0.1 ม.) + dem.json ผ่าน `--convert-dem`
// พิกัดเกม ↔ โลกจริง: ปักหมุดมุมซ้ายบนของ tile (origin_lat/lon) แล้วสเกลเมตรตรงๆ
// --------------------------------------------------------

/// ระดับน้ำทะเลจริง (elevation 0 ม.) อยู่ที่ y นี้ในเกม — เผื่อใต้ทะเล/ขุดลง
pub const DEM_SEA_LEVEL_Y: i32 = 64;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DemMeta {
    pub width: usize,
    pub height: usize,
    /// lat/lon ของพิกเซลมุมซ้ายบน (เหนือสุด-ตะวันตกสุด)
    pub origin_lat: f64,
    pub origin_lon: f64,
    /// ระยะจริงต่อพิกเซล (เมตร) — แกน x = ตะวันออก, z = ใต้
    pub meters_per_px_x: f64,
    pub meters_per_px_z: f64,
    /// เมตรต่อ 1 หน่วยของค่าใน r16 (0.1)
    pub value_scale: f32,
}

pub struct DemData {
    pub meta: DemMeta,
    /// ความสูง u16 (×value_scale = เมตร) เรียงแถวบน→ล่าง ในแถวซ้าย→ขวา
    pub samples: Vec<u16>,
}

impl DemData {
    /// ความสูง (เมตร) ที่พิกัดพิกเซลจำนวนเต็ม (clamp ขอบ)
    fn at(&self, px: i64, pz: i64) -> f32 {
        let x = px.clamp(0, self.meta.width as i64 - 1) as usize;
        let z = pz.clamp(0, self.meta.height as i64 - 1) as usize;
        self.samples[z * self.meta.width + x] as f32 * self.meta.value_scale
    }

    /// ความสูง (เมตร) ณ ตำแหน่งบล็อก (x=เมตรตะวันออกจาก origin, z=เมตรลงใต้)
    /// — bilinear ระหว่าง 4 จุดข้อมูลรอบๆ (ข้อมูลจริงห่างกัน ~30 ม.)
    pub fn elevation_at_block(&self, bx: f64, bz: f64) -> f32 {
        let px = bx / self.meta.meters_per_px_x;
        let pz = bz / self.meta.meters_per_px_z;
        let (x0, z0) = (px.floor(), pz.floor());
        let (fx, fz) = ((px - x0) as f32, (pz - z0) as f32);
        let (x0, z0) = (x0 as i64, z0 as i64);
        let h00 = self.at(x0, z0);
        let h10 = self.at(x0 + 1, z0);
        let h01 = self.at(x0, z0 + 1);
        let h11 = self.at(x0 + 1, z0 + 1);
        let top = h00 + (h10 - h00) * fx;
        let bot = h01 + (h11 - h01) * fx;
        top + (bot - top) * fz
    }

    /// จุดกึ่งกลาง tile ในพิกัดบล็อก — ใช้เป็นจุด spawn เริ่มต้น
    pub fn center_block(&self) -> (f64, f64) {
        (
            self.meta.width as f64 * self.meta.meters_per_px_x / 2.0,
            self.meta.height as f64 * self.meta.meters_per_px_z / 2.0,
        )
    }

    /// พิกัดบล็อก → lat/lon จริง (ไว้โชว์ GPS บน HUD)
    pub fn block_to_latlon(&self, bx: f64, bz: f64) -> (f64, f64) {
        let lat = self.meta.origin_lat - bz / 110_574.0;
        let lon = self.meta.origin_lon + bx / (111_320.0 * lat.to_radians().cos());
        (lat, lon)
    }

    /// lat/lon จริง → พิกัดบล็อก (อินเวอร์สของ block_to_latlon — ไว้ teleport)
    pub fn latlon_to_block(&self, lat: f64, lon: f64) -> (f64, f64) {
        let bz = (self.meta.origin_lat - lat) * 110_574.0;
        let bx = (lon - self.meta.origin_lon) * 111_320.0 * lat.to_radians().cos();
        (bx, bz)
    }

    /// lat/lon อยู่ในขอบเขต tile นี้ไหม
    pub fn latlon_in_bounds(&self, lat: f64, lon: f64) -> bool {
        let (bx, bz) = self.latlon_to_block(lat, lon);
        bx >= 0.0
            && bz >= 0.0
            && bx <= self.meta.width as f64 * self.meta.meters_per_px_x
            && bz <= self.meta.height as f64 * self.meta.meters_per_px_z
    }
}

/// โหลดครั้งเดียว แชร์ให้ worldgen task ทุก thread (แพทเทิร์น FACE_TEXTURES)
static DEM: OnceLock<Option<DemData>> = OnceLock::new();

fn dem_dir() -> std::path::PathBuf {
    crate::voxel::project_root().join("assets").join("dem")
}

/// DEM ของโลกจริง (None = ไม่มีไฟล์ — ปุ่ม Real World จะ disabled)
pub fn dem() -> Option<&'static DemData> {
    DEM.get_or_init(|| {
        let meta_bytes = std::fs::read(dem_dir().join("dem.json")).ok()?;
        let meta: DemMeta = serde_json::from_slice(&meta_bytes).ok()?;
        let raw = std::fs::read(dem_dir().join("heightmap.r16")).ok()?;
        if raw.len() != meta.width * meta.height * 2 {
            warn!("heightmap.r16 ขนาดไม่ตรงกับ dem.json — ข้าม DEM");
            return None;
        }
        let samples: Vec<u16> = raw
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        info!(
            "DEM loaded: {}x{} px, origin {:.2}N {:.2}E, {:.1}x{:.1} m/px",
            meta.width, meta.height, meta.origin_lat, meta.origin_lon,
            meta.meters_per_px_x, meta.meters_per_px_z
        );
        Some(DemData { meta, samples })
    })
    .as_ref()
}

/// โหมดแปลงไฟล์: `voxel-game --convert-dem <ไฟล์ .tif> [<lat มุมบน> <lon มุมซ้าย>]`
/// ถ้าไม่ให้ lat/lon จะเดาจากชื่อไฟล์แบบ Copernicus (N18...E098 → บน = 19.0, ซ้าย = 98.0)
pub fn convert_dem_cli(tif_path: &str, lat_top: Option<f64>, lon_left: Option<f64>) {
    let (lat_top, lon_left) = match (lat_top, lon_left) {
        (Some(a), Some(b)) => (a, b),
        _ => match parse_copernicus_name(tif_path) {
            Some(v) => v,
            None => {
                eprintln!("บอก lat/lon มุมบนซ้ายไม่ได้จากชื่อไฟล์ — ใส่เป็น argument เพิ่ม");
                std::process::exit(1);
            }
        },
    };

    println!("อ่าน {tif_path} ...");
    let file = std::fs::File::open(tif_path).expect("เปิดไฟล์ tif ไม่ได้");
    let mut decoder = tiff::decoder::Decoder::new(std::io::BufReader::new(file))
        .expect("ไฟล์ไม่ใช่ TIFF ที่อ่านได้");
    let (width, height) = decoder.dimensions().expect("อ่านขนาดภาพไม่ได้");
    let (width, height) = (width as usize, height as usize);
    println!("ขนาด {width}x{height} px");

    let img = decoder.read_image().expect("decode ภาพไม่ได้ (compression ไม่รองรับ?)");
    let value_scale = 0.1f32;
    let samples: Vec<u16> = match img {
        tiff::decoder::DecodingResult::F32(v) => v
            .into_iter()
            // ค่าลบ (ทะเล/no-data) หนีบเป็น 0 — โซนเราไม่มีแผ่นดินต่ำกว่าทะเล
            .map(|m| (m.max(0.0) / value_scale).min(u16::MAX as f32) as u16)
            .collect(),
        tiff::decoder::DecodingResult::I16(v) => v
            .into_iter()
            .map(|m| ((m.max(0) as f32) / value_scale).min(u16::MAX as f32) as u16)
            .collect(),
        tiff::decoder::DecodingResult::U16(v) => v
            .into_iter()
            .map(|m| ((m as f32) / value_scale).min(u16::MAX as f32) as u16)
            .collect(),
        other => {
            eprintln!("รูปแบบข้อมูลใน TIFF ไม่รองรับ: {other:?}");
            std::process::exit(1);
        }
    };
    assert_eq!(samples.len(), width * height, "จำนวน sample ไม่ตรงขนาดภาพ");

    // เมตรต่อพิกเซลจากขนาดจริงของ tile 1° ณ ละติจูดนี้
    let center_lat = lat_top - 0.5;
    let meters_per_px_z = 110_574.0 / height as f64;
    let meters_per_px_x = 111_320.0 * center_lat.to_radians().cos() / width as f64;

    let meta = DemMeta {
        width,
        height,
        origin_lat: lat_top,
        origin_lon: lon_left,
        meters_per_px_x,
        meters_per_px_z,
        value_scale,
    };

    let out_dir = dem_dir();
    std::fs::create_dir_all(&out_dir).unwrap();
    let mut raw = Vec::with_capacity(samples.len() * 2);
    for s in &samples {
        raw.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(out_dir.join("heightmap.r16"), raw).unwrap();
    std::fs::write(out_dir.join("dem.json"), serde_json::to_vec_pretty(&meta).unwrap()).unwrap();

    let max_m = samples.iter().max().copied().unwrap_or(0) as f32 * value_scale;
    println!(
        "เสร็จ: assets/dem/heightmap.r16 + dem.json ({width}x{height}, ยอดสูงสุด {max_m:.0} ม., {:.1}x{:.1} ม./px)",
        meters_per_px_x, meters_per_px_z
    );
}

/// เดามุมบนซ้ายจากชื่อไฟล์สไตล์ Copernicus/SRTM (มี N18...E098 ฝังอยู่) → (19.0, 98.0)
/// ระวัง: ตัว N/E โผล่ในคำอื่นได้ (COPERNICUS, DEM) — นับเฉพาะตัวที่ตามด้วยตัวเลข
fn parse_copernicus_name(path: &str) -> Option<(f64, f64)> {
    let name = std::path::Path::new(path).file_name()?.to_str()?.to_uppercase();
    let bytes = name.as_bytes();
    let find_deg = |letters: [u8; 2]| -> Option<(u8, f64)> {
        for (i, &c) in bytes.iter().enumerate() {
            if (c == letters[0] || c == letters[1])
                && bytes.get(i + 1).is_some_and(|d| d.is_ascii_digit())
            {
                let digits: String = name[i + 1..]
                    .chars()
                    .take_while(|ch| ch.is_ascii_digit())
                    .collect();
                return digits.parse().ok().map(|v| (c, v));
            }
        }
        None
    };
    let (lat_c, lat) = find_deg([b'N', b'S'])?;
    let (lon_c, lon) = find_deg([b'E', b'W'])?;
    let lat_bottom = if lat_c == b'S' { -lat } else { lat };
    let lon_left = if lon_c == b'W' { -lon } else { lon };
    // ชื่อ tile บอกมุม "ล่าง" — มุมบน = +1°
    Some((lat_bottom + 1.0, lon_left))
}
