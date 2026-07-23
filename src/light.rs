//! แสงแบบ Minecraft: lightmap ต่อบล็อกที่ flood-fill ไว้ล่วงหน้า แล้วอบลง vertex color
//! ตอน mesh — ไม่มี shadow map เรียลไทม์เลย
//!
//! รอบนี้มีเฉพาะ **sky light** (แสงจากฟ้า) ส่วนไฟจากโคม/คบไฟยังเป็น `PointLight` แยก
//! (ดู `refresh_chunk_lamp_lights` ใน voxel.rs)
//!
//! ข้อจำกัดที่กำหนดการออกแบบทั้งหมด: `CHUNK_HEIGHT = 3072` (โลกจริง 1 บล็อก = 1 เมตร)
//! ถ้าเก็บ 1 byte ต่อบล็อกตรงๆ จะกิน 786 KB ต่อ chunk ซึ่งใช้ไม่ได้ จึง
//!   1. เก็บด้วย `Section` ชั้นละ 16 แบบเดียวกับบล็อก — ฟ้าเหนือผิวดินเป็น `Uniform(15)`
//!      ใต้ดินลึกเป็น `Uniform(0)` เก็บ 1 byte ต่อ 4096 cell
//!   2. คำนวณเฉพาะแถบรอบผิวดิน โดยหาความสูงของแต่ละคอลัมน์ก่อน ไม่ไล่ทั้ง 3072 ชั้น

use crate::voxel::{
    block_def, BlockType, CHUNK_HEIGHT, CHUNK_WIDTH, SECTIONS_PER_CHUNK, SECTION_H, SECTION_VOLUME,
};

/// ระดับแสงสูงสุด (กลางแจ้งตอนกลางวัน) — เท่า Minecraft
pub const MAX_LIGHT: u8 = 15;

/// แสงลดลงเท่าไหร่เมื่อผ่านบล็อกนี้ (นอกเหนือจาก -1 ต่อก้าวปกติ)
/// 15 = ทึบสนิทแสงไม่ผ่าน
pub fn light_opacity(block: BlockType) -> u8 {
    if block == BlockType::Air {
        return 0;
    }
    let def = block_def(block);
    if def.solid && !def.transparent {
        return MAX_LIGHT; // บล็อกตันทึบ
    }
    // โปร่งแสงแต่ไม่ใส: น้ำ/ใบไม้/กระจกสี — หรี่ลงหน่อยระหว่างทาง
    match block {
        BlockType::Glass => 0,
        BlockType::Leaves => 1,
        b if b.is_water() => 1,
        _ => 0, // หญ้าสูง กิ่งไม้ ฯลฯ แสงผ่านหมด
    }
}

/// บล็อกนี้ปล่อยให้ sky light ลงตรงๆ โดยไม่ลดระดับไหม (คอลัมน์ "เปิดฟ้า")
fn passes_sky_undimmed(block: BlockType) -> bool {
    light_opacity(block) == 0
}

#[derive(Clone, PartialEq)]
enum LightSection {
    Uniform(u8),
    Dense(Box<[u8; SECTION_VOLUME]>),
}

/// lightmap ของหนึ่ง chunk — โครงเดียวกับ `ChunkBlocks` เป๊ะ
///
/// PartialEq มีไว้ให้เทียบว่า "คำนวณใหม่แล้วได้ค่าเดิมไหม" — สำคัญมากต่อประสิทธิภาพ
/// เพราะ chunk ที่ unload แล้ว load กลับมาด้วยบล็อกชุดเดิมจะได้แสงเท่าเดิม ไม่ต้อง remesh
/// (ถ้าไม่เช็ค จะ remesh รัวทุกเฟรมจนภาพกระพริบ)
#[derive(Clone, PartialEq)]
pub struct ChunkLight {
    sections: Vec<LightSection>,
}

impl Default for ChunkLight {
    fn default() -> Self {
        Self::new_uniform(0)
    }
}

impl ChunkLight {
    pub fn new_uniform(level: u8) -> Self {
        Self { sections: vec![LightSection::Uniform(level); SECTIONS_PER_CHUNK] }
    }

    #[inline]
    fn idx(x: usize, y_local: usize, z: usize) -> usize {
        x + y_local * CHUNK_WIDTH + z * CHUNK_WIDTH * SECTION_H
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        match &self.sections[y / SECTION_H] {
            LightSection::Uniform(l) => *l,
            LightSection::Dense(a) => a[Self::idx(x, y % SECTION_H, z)],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, level: u8) {
        let section = &mut self.sections[y / SECTION_H];
        if let LightSection::Uniform(l) = section {
            if *l == level {
                return;
            }
            *section = LightSection::Dense(Box::new([*l; SECTION_VOLUME]));
        }
        if let LightSection::Dense(a) = section {
            a[Self::idx(x, y % SECTION_H, z)] = level;
        }
    }

    /// ยุบ Dense ที่กลายเป็นค่าเดียวล้วนกลับเป็น Uniform — สำคัญมากกับคอลัมน์สูง 3072
    pub fn compact(&mut self) {
        for section in &mut self.sections {
            if let LightSection::Dense(a) = section {
                let first = a[0];
                if a.iter().all(|l| *l == first) {
                    *section = LightSection::Uniform(first);
                }
            }
        }
    }

    /// จำนวน section ที่ยัง Dense อยู่ — ใช้ในเทสกันการ regress กลับไปกิน 786 KB/chunk
    pub fn dense_sections(&self) -> usize {
        self.sections.iter().filter(|s| matches!(s, LightSection::Dense(_))).count()
    }
}

/// อ่านบล็อกด้วยพิกัด local ที่ทะลุขอบ chunk ได้ — ผู้เรียกส่ง closure ที่รู้จัก
/// เพื่อนบ้านมาให้ (mesher มี `sample` แบบเดียวกันอยู่แล้ว)
pub type BlockSampler<'a> = &'a dyn Fn(i32, i32, i32) -> BlockType;

/// คำนวณ sky light ของ chunk จากบล็อกของตัวเอง + เพื่อนบ้าน
///
/// อัลกอริทึม (แบบ Minecraft):
///   1. หาความสูงของคอลัมน์ = บล็อกบนสุดที่ไม่ปล่อยแสงผ่านเต็ม
///   2. ทุกอย่างเหนือความสูงนั้น = 15 (section ที่อยู่เหนือสุดยัง Uniform อยู่ ไม่เสียหน่วยความจำ)
///   3. BFS จากเซลล์ที่มีแสงไปทุกทิศ — **ลงตรงๆ ผ่านช่องเปิดไม่ลดระดับ** ส่วนไปข้าง/ขึ้น
///      ลด 1 ต่อก้าว ทำให้แสงไล่จางเข้าใต้ชายคา/ปากถ้ำแทนที่จะตัดขอบคม
/// `scan_top` = y สูงสุดที่ "อาจมีบล็อกที่ไม่ใช่อากาศ" ของ chunk นี้และเพื่อนบ้าน
/// ผู้เรียกหาได้ถูกๆ จาก `ChunkBlocks::y_bounds_non_air()` — ถ้าไม่ส่งมาจะต้องไล่สแกน
/// ทั้ง 3072 ชั้นต่อคอลัมน์ ซึ่งหนักเกินไปตอน stream chunk
pub fn compute_sky_light(sample: BlockSampler, scan_top: usize) -> ChunkLight {
    let w = CHUNK_WIDTH as i32;
    let scan_top = scan_top.min(CHUNK_HEIGHT - 1) as i32;

    // --- 1. ความสูงต่อคอลัมน์ (รวมขอบ ±1 เพื่อให้แสงจากเพื่อนบ้านไหลเข้ามาถูก) ---
    let mut height = [[0i32; CHUNK_WIDTH + 2]; CHUNK_WIDTH + 2];
    let mut max_h = 0i32;
    let mut min_h = CHUNK_HEIGHT as i32;
    for zi in 0..CHUNK_WIDTH + 2 {
        for xi in 0..CHUNK_WIDTH + 2 {
            let (x, z) = (xi as i32 - 1, zi as i32 - 1);
            let mut h = 0;
            for y in (0..=scan_top).rev() {
                if !passes_sky_undimmed(sample(x, y, z)) {
                    h = y;
                    break;
                }
            }
            height[zi][xi] = h;
            max_h = max_h.max(h);
            min_h = min_h.min(h);
        }
    }

    let mut light = ChunkLight::new_uniform(0);

    // --- 2. เหนือผิวดินสุดของ chunk = แสงเต็ม ---
    // เขียนเป็น Uniform ทั้ง section ตรงๆ ไม่ไล่ทีละ cell (คอลัมน์สูง 3072)
    let sky_from = (max_h + 1).max(0) as usize;
    let first_sky_section = sky_from.div_ceil(SECTION_H);
    for si in first_sky_section..SECTIONS_PER_CHUNK {
        light.sections[si] = LightSection::Uniform(MAX_LIGHT);
    }

    // --- 3. BFS เฉพาะแถบรอบผิวดิน ---
    // ล่างสุดที่ต้องคิด: ต่ำกว่าคอลัมน์ที่เตี้ยที่สุดลงไปอีก MAX_LIGHT ก้าว
    // (ลึกกว่านั้นแสงจากฟ้าไปไม่ถึงอยู่แล้ว)
    let band_lo = (min_h - MAX_LIGHT as i32 - 1).max(0);
    let band_hi = ((first_sky_section * SECTION_H) as i32 - 1).min(CHUNK_HEIGHT as i32 - 1);

    let mut queue: std::collections::VecDeque<(i32, i32, i32, u8)> = Default::default();

    // เมล็ดเริ่มต้น: คอลัมน์ที่เปิดฟ้า ไล่ลงมาจากเพดานแถบจนชนบล็อกทึบ
    for z in 0..w {
        for x in 0..w {
            let mut level = MAX_LIGHT;
            for y in (band_lo..=band_hi).rev() {
                let op = light_opacity(sample(x, y, z));
                if op >= MAX_LIGHT {
                    break; // ชนบล็อกทึบ — ใต้ลงไปมืดจนกว่า BFS ด้านข้างจะไหลเข้า
                }
                // ลงตรงๆ ผ่านช่องเปิดไม่ลดระดับ (ต่างจากไปด้านข้าง)
                level = level.saturating_sub(op);
                light.set(x as usize, y as usize, z as usize, level);
                queue.push_back((x, y, z, level));
                if level == 0 {
                    break;
                }
            }
        }
    }

    // แสงที่ไหลเข้ามาจาก chunk ข้างเคียงตรงขอบ — ประมาณจากความสูงคอลัมน์ของเพื่อนบ้าน
    // (ค่าจริงจะถูกแก้ให้ตรงตอน relight เพื่อนบ้าน แต่แค่นี้ก็ไม่เห็นตะเข็บแล้ว)
    for (dx, dz) in [(-1i32, 0i32), (w, 0), (0, -1), (0, w)] {
        let (bx, bz) = (dx, dz);
        for t in 0..w {
            let (x, z) = if bx == -1 || bx == w { (bx, t) } else { (t, bz) };
            let col_h = height[(z + 1) as usize][(x + 1) as usize];
            for y in (col_h + 1).max(band_lo)..=band_hi {
                queue.push_back((x, y, z, MAX_LIGHT));
            }
        }
    }

    // --- 4. กระจายด้านข้าง/ขึ้น ลดทีละ 1 ---
    const SIDE_DIRS: [(i32, i32, i32); 5] =
        [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0)];
    while let Some((x, y, z, level)) = queue.pop_front() {
        if level <= 1 {
            continue;
        }
        for (dx, dy, dz) in SIDE_DIRS {
            let (nx, ny, nz) = (x + dx, y + dy, z + dz);
            if nx < 0 || nx >= w || nz < 0 || nz >= w || ny < band_lo || ny > band_hi {
                continue;
            }
            let op = light_opacity(sample(nx, ny, nz));
            if op >= MAX_LIGHT {
                continue;
            }
            let next = level - 1 - op.min(level - 1);
            if next > light.get(nx as usize, ny as usize, nz as usize) {
                light.set(nx as usize, ny as usize, nz as usize, next);
                queue.push_back((nx, ny, nz, next));
            }
        }
    }

    light.compact();
    light
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ตัวช่วยสร้าง sampler จาก closure ที่รับพิกัด (นอกขอบ = อากาศ)
    fn sampler_of(f: impl Fn(i32, i32, i32) -> BlockType) -> impl Fn(i32, i32, i32) -> BlockType {
        move |x, y, z| {
            if y < 0 || y >= CHUNK_HEIGHT as i32 {
                BlockType::Air
            } else {
                f(x, y, z)
            }
        }
    }

    /// คอลัมน์อากาศล้วน = แสงเต็มทุกชั้น และต้องไม่กินหน่วยความจำ (Uniform ล้วน)
    #[test]
    fn open_sky_is_full_bright_and_costs_nothing() {
        let s = sampler_of(|_, _, _| BlockType::Air);
        let light = compute_sky_light(&s, CHUNK_HEIGHT - 1);
        assert_eq!(light.get(8, 0, 8), MAX_LIGHT);
        assert_eq!(light.get(8, CHUNK_HEIGHT - 1, 8), MAX_LIGHT);
        assert_eq!(
            light.dense_sections(),
            0,
            "ฟ้าโล่งต้องเป็น Uniform ล้วน ไม่งั้นคอลัมน์ 3072 ชั้นกิน 786 KB"
        );
    }

    /// พื้นตันเรียบ: เหนือพื้นสว่างเต็ม ใต้พื้นมืดสนิท
    #[test]
    fn solid_ground_blocks_sky_light() {
        let ground = 100;
        let s = sampler_of(move |_, y, _| {
            if y <= ground { BlockType::Stone } else { BlockType::Air }
        });
        let light = compute_sky_light(&s, CHUNK_HEIGHT - 1);
        assert_eq!(light.get(8, ground as usize + 1, 8), MAX_LIGHT, "เหนือพื้นต้องสว่างเต็ม");
        assert_eq!(light.get(8, ground as usize, 8), 0, "ในเนื้อหินต้องมืด");
        assert_eq!(light.get(8, ground as usize - 5, 8), 0, "ใต้ดินลึกต้องมืด");
    }

    /// แสงลงตรงๆ ผ่านปล่องไม่ลดระดับ — ก้นปล่องลึกต้องยังสว่างเต็ม
    #[test]
    fn light_falls_down_a_shaft_without_dimming() {
        let ground = 200;
        let s = sampler_of(move |x, y, z| {
            if y > ground {
                BlockType::Air
            } else if x == 8 && z == 8 && y > ground - 40 {
                BlockType::Air // ปล่องตรงลงไป 40 บล็อก
            } else {
                BlockType::Stone
            }
        });
        let light = compute_sky_light(&s, CHUNK_HEIGHT - 1);
        assert_eq!(
            light.get(8, ground as usize - 30, 8),
            MAX_LIGHT,
            "แสงลงตรงๆ ในปล่องต้องไม่ลดระดับ"
        );
    }

    /// ใต้ชายคาแสงต้องไล่จางเข้าไปตามระยะ ไม่ใช่ตัดขอบคมหรือมืดทันที
    #[test]
    fn light_fades_gradually_under_an_overhang() {
        let floor = 100;
        let roof = 105;
        // หลังคาทึบคลุม x >= 4 ทั้งหมด เปิดฟ้าเฉพาะ x < 4
        let s = sampler_of(move |x, y, _| {
            if y <= floor {
                BlockType::Stone
            } else if y == roof && x >= 4 {
                BlockType::Stone
            } else {
                BlockType::Air
            }
        });
        let light = compute_sky_light(&s, CHUNK_HEIGHT - 1);
        let y = floor as usize + 1;
        let at = |x: usize| light.get(x, y, 8);

        assert_eq!(at(3), MAX_LIGHT, "นอกชายคาต้องสว่างเต็ม");
        // ยิ่งลึกเข้าไปใต้ชายคายิ่งมืด และต้องลดลงจริงทีละขั้น ไม่ใช่ดับทันที
        for x in 4..10 {
            assert!(at(x) < at(x - 1), "x={x} ต้องมืดกว่าตำแหน่งก่อนหน้า");
        }
        assert!(at(4) > 0, "ขอบชายคาต้องยังมีแสงเหลือ ไม่ใช่ดำสนิท");
    }

    /// โพรงปิดสนิทใต้ดินต้องมืดทั้งโพรง
    #[test]
    fn sealed_cave_is_pitch_black() {
        let ground = 300;
        let (cy, cx, cz) = (250usize, 8usize, 8usize);
        let s = sampler_of(move |x, y, z| {
            if y > ground {
                BlockType::Air
            } else if (y as usize).abs_diff(cy) <= 1
                && (x as usize).abs_diff(cx) <= 1
                && (z as usize).abs_diff(cz) <= 1
            {
                BlockType::Air // โพรง 3x3x3 ปิดสนิท
            } else {
                BlockType::Stone
            }
        });
        let light = compute_sky_light(&s, CHUNK_HEIGHT - 1);
        assert_eq!(light.get(cx, cy, cz), 0, "โพรงปิดต้องมืดสนิท");
    }

    /// กระจกใสไม่หรี่แสง แต่ใบไม้/น้ำหรี่
    #[test]
    fn glass_passes_light_but_leaves_dim_it() {
        assert_eq!(light_opacity(BlockType::Air), 0);
        assert_eq!(light_opacity(BlockType::Glass), 0);
        assert_eq!(light_opacity(BlockType::Stone), MAX_LIGHT);
        assert!(light_opacity(BlockType::Leaves) > 0 && light_opacity(BlockType::Leaves) < MAX_LIGHT);
    }
}
