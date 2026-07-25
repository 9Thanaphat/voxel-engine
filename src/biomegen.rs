//! ระบบ biome แบบ **data-driven ลำดับชั้น** — เขตอุณหภูมิใหญ่ (ClimateZone, ดู biome.rs) ×
//! sub-biome (นิยามเป็นข้อมูลปรับได้ใน dev mode). แต่ละ sub-biome คุมพื้นผิว/พืช/**ความสูง terrain**
//!
//! เลือก sub-biome ในเขตด้วย **region noise แบ่งหย่อม** (freq ต่ำ = หย่อมใหญ่) ตาม weight —
//! deterministic จากตำแหน่ง+seed ล้วน จึง gen ตรงกันทั้ง host/client (worldgen sync)

use crate::biome::ClimateZone;
use crate::voxel::BlockType;
use noise::{NoiseFn, Perlin};
use serde::{Deserialize, Serialize};

/// ชนิดพืชที่ปลูกใน sub-biome (ต้นไม้กิ่ง/ต้นสนคิวบ์/ไม่มี)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TreeKind {
    None,
    Broadleaf,
    Spruce,
}

impl TreeKind {
    pub const ALL: [TreeKind; 3] = [TreeKind::None, TreeKind::Broadleaf, TreeKind::Spruce];
    pub fn label(self) -> &'static str {
        match self {
            TreeKind::None => "None",
            TreeKind::Broadleaf => "Broadleaf",
            TreeKind::Spruce => "Spruce",
        }
    }
}

/// นิยาม sub-biome หนึ่งตัว — ปรับได้ใน dev mode, เซฟลง biomes.json, sync ให้ client
#[derive(Clone, Serialize, Deserialize)]
pub struct BiomeDef {
    pub name: String,
    pub zone: ClimateZone,
    /// สัดส่วนพื้นที่ในเขต (region-noise pick) — มาก = หย่อมกว้าง
    pub weight: f32,
    /// ยกฐานความสูง (บล็อกเหนือ/ใต้ระดับน้ำ)
    pub height_offset: f32,
    /// ความสูงภูเขา — 0 = ราบเรียบ, มาก = ภูเขาสูง
    pub amplitude: f32,
    /// บล็อกผิว/ใต้ผิว (เก็บเป็น id ให้ serialize ง่าย ดู [`BlockType::from_u8`])
    pub surface_block: u8,
    pub subsurface_block: u8,
    pub tree: TreeKind,
    /// ความหนาแน่นต้นไม้ 0..1 (คูณกับจำนวนต้นสุ่มต่อ chunk)
    pub tree_density: f32,
}

impl BiomeDef {
    pub fn surface(&self) -> BlockType {
        BlockType::from_u8(self.surface_block)
    }
    pub fn subsurface(&self) -> BlockType {
        BlockType::from_u8(self.subsurface_block)
    }
}

/// ชุด biome ทั้งโลก (global config) — Resource + เซฟไฟล์ + sync ให้ client
#[derive(Clone, Serialize, Deserialize, bevy::prelude::Resource)]
pub struct BiomeConfig {
    pub biomes: Vec<BiomeDef>,
    /// ผิวเหนือระดับนี้ (บล็อกเหนือ sea) = คลุมหิมะ (ยอดเขาทุกเขต)
    pub snow_line: i32,
}

impl Default for BiomeConfig {
    fn default() -> Self {
        // ค่าเริ่มต้น: seed จาก biome เดิม + variant ราบ/ป่า/ภูเขา ต่อเขต ให้ผู้ใช้ต่อยอด
        let g = BlockType::Grass as u8;
        let d = BlockType::Dirt as u8;
        let sand = BlockType::Sand as u8;
        let stone = BlockType::Stone as u8;
        let snow = BlockType::Snow as u8;
        let sgrass = BlockType::SnowyGrass as u8;
        use ClimateZone::*;
        use TreeKind::*;
        let mk = |name: &str, zone, weight, height_offset, amplitude, surface_block, subsurface_block, tree, tree_density| BiomeDef {
            name: name.to_string(), zone, weight, height_offset, amplitude,
            surface_block, subsurface_block, tree, tree_density,
        };
        Self {
            // ยอดเขาใหญ่มาจาก MOUNT_AMP (ridged) ใน voxel.rs แล้ว — amplitude ที่นี่คือความขรุขระกลาง
            snow_line: 92,
            biomes: vec![
                // ── ร้อน ── (offset/amplitude หน่วยบล็อก — ผิวยิ่งสูง ใต้ดินยิ่งลึก/gen ช้า)
                mk("Desert", Hot, 1.0, 3.0, 6.0, sand, sand, None, 0.0),
                mk("Savanna", Hot, 1.0, 4.0, 8.0, g, d, Broadleaf, 0.15),
                mk("Tropical Forest", Hot, 1.0, 5.0, 14.0, g, d, Broadleaf, 0.7),
                // ── อุ่น ──
                mk("Plains", Warm, 1.2, 4.0, 6.0, g, d, Broadleaf, 0.1),
                mk("Temperate Forest", Warm, 1.0, 5.0, 16.0, g, d, Broadleaf, 0.6),
                mk("Mountains", Warm, 0.6, 10.0, 22.0, g, stone, Broadleaf, 0.2),
                // ── เย็น ──
                mk("Taiga", Cool, 1.0, 4.0, 10.0, sgrass, d, Spruce, 0.5),
                mk("Conifer Mountains", Cool, 0.7, 10.0, 22.0, sgrass, stone, Spruce, 0.3),
                // ── หนาว ──
                mk("Tundra", Cold, 1.0, 3.0, 6.0, snow, d, None, 0.0),
                mk("Snowy Mountains", Cold, 0.7, 12.0, 24.0, snow, stone, None, 0.0),
            ],
        }
    }
}

impl BiomeConfig {
    /// biome ตัวแรกในเขต (fallback ถ้า weight รวม = 0)
    fn first_in(&self, zone: ClimateZone) -> Option<&BiomeDef> {
        self.biomes.iter().find(|b| b.zone == zone)
    }
}

/// region noise → 0..1 (หย่อมใหญ่ ~หลายร้อยบล็อกต่อหย่อม)
fn region_value(region: &Perlin, wx: f64, wz: f64) -> f64 {
    ((region.get([wx * 0.0005, wz * 0.0005]) + 1.0) * 0.5).clamp(0.0, 1.0)
}

/// เลือก sub-biome ในเขตด้วย weight (deterministic) — ไม่ alloc
pub fn select<'a>(
    cfg: &'a BiomeConfig,
    zone: ClimateZone,
    region: &Perlin,
    wx: f64,
    wz: f64,
) -> Option<&'a BiomeDef> {
    let first = cfg.first_in(zone)?;
    let total: f32 = cfg.biomes.iter().filter(|b| b.zone == zone).map(|b| b.weight.max(0.0)).sum();
    if total <= 0.0 {
        return Some(first);
    }
    let mut r = region_value(region, wx, wz) as f32 * total;
    for b in cfg.biomes.iter().filter(|b| b.zone == zone) {
        let w = b.weight.max(0.0);
        if r < w {
            return Some(b);
        }
        r -= w;
    }
    Some(first)
}

/// ความสูง terrain (offset, amplitude) แบบ **blend ขอบ** — เฉลี่ยจาก 5 จุด (กลาง+4 รอบ)
/// เพื่อให้ขอบ biome ราบ↔ภูเขา ลาดเนียน ไม่เป็นหน้าผา. `temp_at` = อุณหภูมิ ณ ตำแหน่ง (นิ่งตามฤดู)
pub fn terrain_at(
    cfg: &BiomeConfig,
    region: &Perlin,
    temp_at: impl Fn(f64, f64) -> f64,
    wx: f64,
    wz: f64,
) -> (f32, f32) {
    const R: f64 = 32.0;
    let pts = [(0.0, 0.0), (R, 0.0), (-R, 0.0), (0.0, R), (0.0, -R)];
    let (mut off, mut amp, mut n) = (0.0f32, 0.0f32, 0.0f32);
    for (dx, dz) in pts {
        let (x, z) = (wx + dx, wz + dz);
        let zone = crate::biome::zone_of(temp_at(x, z));
        if let Some(b) = select(cfg, zone, region, x, z) {
            off += b.height_offset;
            amp += b.amplitude;
            n += 1.0;
        }
    }
    if n > 0.0 {
        (off / n, amp / n)
    } else {
        (4.0, 10.0) // ไม่มี biome ในเขต (ผู้ใช้ลบหมด) — ราบ default
    }
}
