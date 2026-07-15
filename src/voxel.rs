use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};

use bevy::{
    asset::RenderAssetUsages,
    camera::primitives::Aabb,
    light::NotShadowCaster,
    prelude::*,
    render::{mesh::Indices, render_resource::PrimitiveTopology},
    tasks::AsyncComputeTaskPool,
};
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

#[derive(Component)]
pub struct Block;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BlockType {
    Air = 0,
    Dirt = 1,
    Grass = 2,
    Stone = 3,
    Water = 4,
    Wood = 5,
    Leaves = 6,
    Sand = 7,
    Glowstone = 8,
    LampRed = 9,
    LampGreen = 10,
    LampBlue = 11,
    Glass = 12,
    TallGrass = 13,
}

impl BlockType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => BlockType::Dirt,
            2 => BlockType::Grass,
            3 => BlockType::Stone,
            4 => BlockType::Water,
            5 => BlockType::Wood,
            6 => BlockType::Leaves,
            7 => BlockType::Sand,
            8 => BlockType::Glowstone,
            9 => BlockType::LampRed,
            10 => BlockType::LampGreen,
            11 => BlockType::LampBlue,
            12 => BlockType::Glass,
            13 => BlockType::TallGrass,
            _ => BlockType::Air,
        }
    }

    /// บล็อกที่ชนตัวผู้เล่นได้
    pub fn is_solid(self) -> bool {
        block_def(self).solid
    }

    /// บล็อกที่บังแสง/สร้างเงา AO (ตันและไม่โปร่งใส)
    pub fn occludes(self) -> bool {
        let def = block_def(self);
        def.solid && !def.transparent
    }
}

// --------------------------------------------------------
// Block Registry — property ทุกอย่างของบล็อกอยู่ตารางเดียว
// เพิ่มบล็อกใหม่: เพิ่ม variant ใน enum + arm ใน from_u8 + แถวในตารางนี้
// (index ของตาราง = id ของบล็อก ห้ามสลับลำดับ ไม่งั้น savefile เก่าพัง)
// --------------------------------------------------------

pub struct BlockDef {
    pub name: &'static str,
    /// สี fallback เมื่อไม่มี texture (และใช้เป็นสีใน preview mode)
    pub color: [f32; 4],
    pub solid: bool,
    /// มองทะลุได้ (น้ำ/กระจก/หญ้าสูง) — ไม่บังหน้าบล็อกข้างเคียง ไม่สร้างเงา AO
    pub transparent: bool,
    /// สีแสงของบล็อกเรืองแสง (None = บล็อกธรรมดา)
    pub emission: Option<[f32; 3]>,
    /// path ใต้ assets/ — ใส่ได้หลายลาย เกมจะสุ่มเลือกตามพิกัดบล็อก
    /// (deterministic) ให้ไม่ซ้ำกันเป็นแพทเทิร์น ไฟล์ไหนไม่มีจริงถูกข้าม
    pub tex_top: &'static [&'static str],
    pub tex_side: &'static [&'static str],
    pub tex_bottom: &'static [&'static str],
    /// sprite พู่ห้อยเอียงจากขอบบนของหน้าด้านข้าง (alpha cutout, สุ่มลายตามพิกัด)
    pub overlay_side: &'static [&'static str],
}

pub const BLOCK_DEFS: [BlockDef; 14] = [
    BlockDef { name: "Air", color: [1.0, 1.0, 1.0, 1.0], solid: false, transparent: true, emission: None,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Dirt", color: [0.4, 0.2, 0.0, 1.0], solid: true, transparent: false, emission: None,
        tex_top: &["textures/dirt.png"], tex_side: &["textures/dirt.png"], tex_bottom: &["textures/dirt.png"],
        overlay_side: &[] },
    BlockDef { name: "Grass", color: [0.2, 0.6, 0.2, 1.0], solid: true, transparent: false, emission: None,
        tex_top: &["textures/grass_top.png"],
        // ด้านข้างมี 3 ลาย สุ่มตามพิกัดให้ไม่ซ้ำกันเป็นแพทเทิร์น
        tex_side: &["textures/grass_side_1.png", "textures/grass_side_2.png", "textures/grass_side_3.png"],
        tex_bottom: &["textures/dirt.png"],
        // พู่หญ้าห้อยเอียงจากขอบบน — สุ่ม 3 ลายเช่นกัน
        overlay_side: &[
            "textures/grass_side_overlay_1.png",
            "textures/grass_side_overlay_2.png",
            "textures/grass_side_overlay_3.png",
        ] },
    BlockDef { name: "Stone", color: [0.5, 0.5, 0.5, 1.0], solid: true, transparent: false, emission: None,
        tex_top: &["textures/stone.png"], tex_side: &["textures/stone.png"], tex_bottom: &["textures/stone.png"],
        overlay_side: &[] },
    BlockDef { name: "Water", color: [0.1, 0.3, 0.8, 1.0], solid: false, transparent: true, emission: None,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Wood", color: [0.35, 0.25, 0.12, 1.0], solid: true, transparent: false, emission: None,
        tex_top: &["textures/wood_top.png"], tex_side: &["textures/wood_side.png"], tex_bottom: &["textures/wood_top.png"],
        overlay_side: &[] },
    BlockDef { name: "Leaves", color: [0.15, 0.45, 0.12, 1.0], solid: true, transparent: false, emission: None,
        tex_top: &["textures/leaves.png"], tex_side: &["textures/leaves.png"], tex_bottom: &["textures/leaves.png"],
        overlay_side: &[] },
    BlockDef { name: "Sand", color: [0.85, 0.80, 0.55, 1.0], solid: true, transparent: false, emission: None,
        tex_top: &["textures/sand.png"], tex_side: &["textures/sand.png"], tex_bottom: &["textures/sand.png"],
        overlay_side: &[] },
    BlockDef { name: "Glowstone", color: [1.0, 0.85, 0.55, 1.0], solid: true, transparent: false, emission: Some([1.0, 0.85, 0.55]),
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Red Lamp", color: [1.0, 0.15, 0.10, 1.0], solid: true, transparent: false, emission: Some([1.0, 0.15, 0.10]),
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Green Lamp", color: [0.15, 1.0, 0.20, 1.0], solid: true, transparent: false, emission: Some([0.15, 1.0, 0.20]),
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Blue Lamp", color: [0.15, 0.30, 1.0, 1.0], solid: true, transparent: false, emission: Some([0.15, 0.30, 1.0]),
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Glass", color: [0.80, 0.90, 1.0, 1.0], solid: true, transparent: true, emission: None,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Tall Grass", color: [0.25, 0.55, 0.53, 1.0], solid: false, transparent: true, emission: None,
        // ใช้ช่อง side เป็นรูป sprite ของกากบาท
        tex_top: &[], tex_side: &["textures/grass.png"], tex_bottom: &[], overlay_side: &[] },
];

pub fn block_def(block: BlockType) -> &'static BlockDef {
    &BLOCK_DEFS[block as usize]
}

pub fn block_name(block: BlockType) -> &'static str {
    block_def(block).name
}

pub fn block_color(block: BlockType) -> [f32; 4] {
    block_def(block).color
}

pub fn lamp_emission(block: BlockType) -> Option<Color> {
    block_def(block).emission.map(|c| Color::srgb(c[0], c[1], c[2]))
}

/// texture ที่ใช้ได้จริง (มีไฟล์บน disk) ต่อ (บล็อก, หน้า) — สร้างครั้งเดียวตอน setup
/// เข้าถึงได้จาก mesh task ทุก thread โดยไม่ต้องส่งผ่าน channel
static FACE_TEXTURES: OnceLock<Vec<[Vec<&'static str>; 6]>> = OnceLock::new();

fn face_texture_list(block: BlockType, face_id: usize) -> &'static [&'static str] {
    FACE_TEXTURES
        .get()
        .map(|table| table[block as usize][face_id].as_slice())
        .unwrap_or(&[])
}

/// hash พิกัดบล็อก → เลือกลาย texture แบบ deterministic (บล็อกเดิมลายเดิมเสมอ)
fn pos_hash(x: i32, y: i32, z: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x85EB_CA6B)
        ^ (y as u32).wrapping_mul(0xC2B2_AE35)
        ^ (z as u32).wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    h
}

/// index ของลายที่บล็อกตำแหน่งนี้ใช้ (0 เสมอถ้ามีลายเดียวหรือไม่มี)
pub fn texture_variant(block: BlockType, face_id: usize, wx: i32, wy: i32, wz: i32) -> u8 {
    let list = face_texture_list(block, face_id);
    if list.len() <= 1 {
        0
    } else {
        (pos_hash(wx, wy, wz) % list.len() as u32) as u8
    }
}

pub fn face_texture(block: BlockType, face_id: usize, variant: u8) -> Option<&'static str> {
    face_texture_list(block, face_id).get(variant as usize).copied()
}

/// overlay ด้านข้างที่ใช้ได้จริง (มีไฟล์บน disk) ต่อบล็อก
static SIDE_OVERLAYS: OnceLock<Vec<Vec<&'static str>>> = OnceLock::new();

/// เลือก sprite พู่ของหน้าด้านข้างนี้ (สุ่มลายตามพิกัด+ทิศ, deterministic)
fn side_overlay_pick(block: BlockType, face_id: usize, wx: i32, wy: i32, wz: i32) -> Option<&'static str> {
    let list = SIDE_OVERLAYS.get()?.get(block as usize)?.as_slice();
    if list.is_empty() {
        return None;
    }
    let idx = pos_hash(wx.wrapping_add(face_id as i32 * 7919), wy, wz) % list.len() as u32;
    Some(list[idx as usize])
}

pub const CHUNK_WIDTH: usize = 16;
pub const CHUNK_HEIGHT: usize = 512;
pub const CHUNK_VOLUME: usize = CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH;
pub const SEA_LEVEL: usize = 200;

pub struct ChunkData {
    pub blocks: Arc<[BlockType; CHUNK_VOLUME]>,
    pub num_vertices: usize,
    pub num_indices: usize,
}

impl ChunkData {
    pub fn get_index(x: usize, y: usize, z: usize) -> usize {
        x + y * CHUNK_WIDTH + z * CHUNK_WIDTH * CHUNK_HEIGHT
    }
}

#[derive(Resource, Default)]
pub struct VoxelWorld {
    pub chunks: HashMap<IVec2, ChunkData>,            // block data + สถิติ
    pub generated_chunks: HashMap<IVec2, Entity>,     // mesh entity (พื้นดิน vertex color)
    pub water_chunks: HashMap<IVec2, Entity>,         // mesh entity (น้ำ โปร่งใส)
    pub glass_chunks: HashMap<IVec2, Entity>,         // mesh entity (กระจก โปร่งใส)
    pub deco_chunks: HashMap<IVec2, Vec<Entity>>,     // mesh entity (ของประดับกากบาทและพู่หญ้า)
    pub glow_chunks: HashMap<IVec2, Vec<Entity>>,     // mesh entity (บล็อกเรืองแสง ต่อสี)
    pub textured_chunks: HashMap<IVec2, Vec<Entity>>, // mesh entity (บล็อกมี texture ต่อไฟล์)
    pub lamp_lights: HashMap<IVec2, Vec<Entity>>,     // PointLight ของบล็อกไฟใน chunk
    pub total_vertices: usize,
    pub total_indices: usize,
}

impl VoxelWorld {
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> BlockType {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return BlockType::Air;
        }

        let chunk_x = x.div_euclid(CHUNK_WIDTH as i32);
        let chunk_z = z.div_euclid(CHUNK_WIDTH as i32);

        if let Some(chunk) = self.chunks.get(&IVec2::new(chunk_x, chunk_z)) {
            let local_x = x.rem_euclid(CHUNK_WIDTH as i32) as usize;
            let local_y = y as usize;
            let local_z = z.rem_euclid(CHUNK_WIDTH as i32) as usize;
            chunk.blocks[ChunkData::get_index(local_x, local_y, local_z)]
        } else {
            BlockType::Air
        }
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block_type: BlockType) -> bool {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return false;
        }

        let chunk_x = x.div_euclid(CHUNK_WIDTH as i32);
        let chunk_z = z.div_euclid(CHUNK_WIDTH as i32);

        if let Some(chunk) = self.chunks.get_mut(&IVec2::new(chunk_x, chunk_z)) {
            let local_x = x.rem_euclid(CHUNK_WIDTH as i32) as usize;
            let local_y = y as usize;
            let local_z = z.rem_euclid(CHUNK_WIDTH as i32) as usize;

            let blocks_mut = Arc::make_mut(&mut chunk.blocks);
            blocks_mut[ChunkData::get_index(local_x, local_y, local_z)] = block_type;
            true
        } else {
            false
        }
    }
}

// --------------------------------------------------------
// Mesh building
// --------------------------------------------------------

pub const CUBE_POSITIONS: [[[f32; 3]; 4]; 6] = [
    // Top (Y+)
    [[0., 1., 1.], [1., 1., 1.], [1., 1., 0.], [0., 1., 0.]],
    // Bottom (Y-)
    [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]],
    // Right (X+)
    [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]],
    // Left (X-)
    [[0., 0., 1.], [0., 1., 1.], [0., 1., 0.], [0., 0., 0.]],
    // Forward (Z+)
    [[1., 0., 1.], [1., 1., 1.], [0., 1., 1.], [0., 0., 1.]],
    // Back (Z-)
    [[0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]],
];

pub const CUBE_NORMALS: [[f32; 3]; 6] = [
    [0., 1., 0.],
    [0., -1., 0.],
    [1., 0., 0.],
    [-1., 0., 0.],
    [0., 0., 1.],
    [0., 0., -1.],
];

pub const FACE_OFFSETS: [[i32; 3]; 6] = [
    [0, 1, 0],
    [0, -1, 0],
    [1, 0, 0],
    [-1, 0, 0],
    [0, 0, 1],
    [0, 0, -1],
];

// เงาประจำทิศแบบ Minecraft: บนสว่างสุด ล่างมืดสุด ด้านข้างลดหลั่นกัน
pub const FACE_SHADE: [f32; 6] = [1.0, 0.5, 0.8, 0.8, 0.6, 0.6];

// ความสว่างตามระดับ AO (0 = มุมอับสุด, 3 = โล่ง)
pub const AO_CURVE: [f32; 4] = [0.45, 0.65, 0.85, 1.0];

#[derive(Default)]
pub struct MeshBuf {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl MeshBuf {
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    fn push_quad(
        &mut self,
        verts: [[f32; 3]; 4],
        normal: [f32; 3],
        cols: [[f32; 4]; 4],
        uvs: [[f32; 2]; 4],
        flip: bool,
    ) {
        let vc = self.positions.len() as u32;
        for i in 0..4 {
            self.positions.push(verts[i]);
            self.normals.push(normal);
            self.colors.push(cols[i]);
            self.uvs.push(uvs[i]);
        }
        // flip = สลับ diagonal ของ quad (ใช้ตอน AO ไม่สมมาตร กัน interpolation เบี้ยว)
        if flip {
            self.indices.extend_from_slice(&[vc, vc + 1, vc + 3, vc + 1, vc + 2, vc + 3]);
        } else {
            self.indices.extend_from_slice(&[vc, vc + 1, vc + 2, vc, vc + 2, vc + 3]);
        }
    }

    pub fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

/// mesh ทั้งหมดของ chunk เดียว แยกตาม material ที่ต้องใช้
#[derive(Default)]
pub struct ChunkMeshSet {
    /// บล็อกที่ไม่มี texture — ใช้ vertex color
    pub solid: MeshBuf,
    /// น้ำ (material โปร่งใส)
    pub water: MeshBuf,
    /// กระจก (material โปร่งใสอีกระดับ)
    pub glass: MeshBuf,
    /// ของประดับ alpha cutout สองหน้า (หญ้าสูง, พู่หญ้าข้างบล็อก) แยกต่อ sprite
    pub deco: Vec<(&'static str, MeshBuf)>,
    /// บล็อกเรืองแสง แยกต่อชนิด (material emissive)
    pub glow: Vec<(BlockType, MeshBuf)>,
    /// บล็อกมี texture แยกต่อไฟล์ texture
    pub textured: Vec<(&'static str, MeshBuf)>,
}

impl ChunkMeshSet {
    pub fn total_vertices(&self) -> usize {
        self.solid.positions.len()
            + self.water.positions.len()
            + self.glass.positions.len()
            + self.deco.iter().map(|(_, b)| b.positions.len()).sum::<usize>()
            + self.glow.iter().map(|(_, b)| b.positions.len()).sum::<usize>()
            + self.textured.iter().map(|(_, b)| b.positions.len()).sum::<usize>()
    }

    pub fn total_indices(&self) -> usize {
        self.solid.indices.len()
            + self.water.indices.len()
            + self.glass.indices.len()
            + self.deco.iter().map(|(_, b)| b.indices.len()).sum::<usize>()
            + self.glow.iter().map(|(_, b)| b.indices.len()).sum::<usize>()
            + self.textured.iter().map(|(_, b)| b.indices.len()).sum::<usize>()
    }
}

/// หา/สร้าง buffer ของบล็อกเรืองแสงชนิดนั้นๆ
fn glow_buf(glow: &mut Vec<(BlockType, MeshBuf)>, block: BlockType) -> &mut MeshBuf {
    if let Some(i) = glow.iter().position(|(b, _)| *b == block) {
        &mut glow[i].1
    } else {
        glow.push((block, MeshBuf::default()));
        &mut glow.last_mut().unwrap().1
    }
}

/// หา/สร้าง buffer ของ texture นั้นๆ
fn texture_buf<'a>(bufs: &'a mut Vec<(&'static str, MeshBuf)>, tex: &'static str) -> &'a mut MeshBuf {
    if let Some(i) = bufs.iter().position(|(t, _)| *t == tex) {
        &mut bufs[i].1
    } else {
        bufs.push((tex, MeshBuf::default()));
        &mut bufs.last_mut().unwrap().1
    }
}

/// สร้าง mesh ของ chunk ด้วย greedy meshing:
/// หน้าที่อยู่ระนาบเดียวกัน ชนิดบล็อกเดียวกัน และ AO สม่ำเสมอเท่ากัน
/// จะถูกรวมเป็น quad ใหญ่อันเดียว ส่วนหน้าที่ AO ไล่เฉดภายใน quad
/// จะถูกวาดแยกทีละหน้าเพื่อรักษาเงาซอกมุม
///
/// ลำดับ neighbors: [+X, -X, +Z, -Z, +X+Z, +X-Z, -X+Z, -X-Z]
/// (แนวทแยงจำเป็นสำหรับ vertex AO ที่มุม chunk)
pub fn create_mesh_from_blocks(
    chunk_pos: IVec2,
    blocks: &[BlockType; CHUNK_VOLUME],
    neighbors: &[Arc<[BlockType; CHUNK_VOLUME]>; 8],
) -> ChunkMeshSet {
    let mut set = ChunkMeshSet::default();

    // พิกัดโลกของมุม chunk — ใช้ hash เลือกลาย texture ให้ต่อเนื่องข้าม chunk
    let world_base_x = chunk_pos.x * CHUNK_WIDTH as i32;
    let world_base_z = chunk_pos.y * CHUNK_WIDTH as i32;

    // อ่านบล็อกด้วยพิกัด local ที่ทะลุขอบ chunk ได้ (รวมแนวทแยง)
    let sample = |x: i32, y: i32, z: i32| -> BlockType {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return BlockType::Air;
        }
        let w = CHUNK_WIDTH as i32;
        let lx = x.rem_euclid(w) as usize;
        let lz = z.rem_euclid(w) as usize;
        let src: &[BlockType; CHUNK_VOLUME] = match (x.div_euclid(w), z.div_euclid(w)) {
            (0, 0) => blocks,
            (1, 0) => &neighbors[0],
            (-1, 0) => &neighbors[1],
            (0, 1) => &neighbors[2],
            (0, -1) => &neighbors[3],
            (1, 1) => &neighbors[4],
            (1, -1) => &neighbors[5],
            (-1, 1) => &neighbors[6],
            (-1, -1) => &neighbors[7],
            _ => return BlockType::Air,
        };
        src[ChunkData::get_index(lx, y as usize, lz)]
    };

    // Vertex AO: แต่ละมุมของหน้า เช็คบล็อกข้าง 2 + มุม 1 บนระนาบนอกหน้า
    let face_ao = |c: [i32; 3], face_id: usize| -> [u8; 4] {
        let norm = FACE_OFFSETS[face_id];
        let (a1, a2) = if norm[0] != 0 {
            (1, 2)
        } else if norm[1] != 0 {
            (0, 2)
        } else {
            (0, 1)
        };
        let base = [c[0] + norm[0], c[1] + norm[1], c[2] + norm[2]];
        let face_positions = CUBE_POSITIONS[face_id];

        let mut ao = [3u8; 4];
        for i in 0..4 {
            let vp = face_positions[i];
            let s1: i32 = if vp[a1] < 0.5 { -1 } else { 1 };
            let s2: i32 = if vp[a2] < 0.5 { -1 } else { 1 };

            let mut p1 = base;
            p1[a1] += s1;
            let mut p2 = base;
            p2[a2] += s2;
            let mut pc = base;
            pc[a1] += s1;
            pc[a2] += s2;

            let side1 = sample(p1[0], p1[1], p1[2]).occludes();
            let side2 = sample(p2[0], p2[1], p2[2]).occludes();
            let corner = sample(pc[0], pc[1], pc[2]).occludes();

            ao[i] = if side1 && side2 {
                0
            } else {
                3 - (side1 as u8 + side2 as u8 + corner as u8)
            };
        }
        ao
    };

    let axis_len = [CHUNK_WIDTH as i32, CHUNK_HEIGHT as i32, CHUNK_WIDTH as i32];

    for face_id in 0..6 {
        let norm = FACE_OFFSETS[face_id];
        let a = if norm[0] != 0 { 0 } else if norm[1] != 0 { 1 } else { 2 };
        let (ua, va) = match a {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let (la, lu, lv) = (axis_len[a], axis_len[ua], axis_len[va]);
        let midx = |ui: i32, vi: i32| (vi * lu + ui) as usize;

        // UV จากพิกัดบนระนาบของหน้า (1 บล็อก = 1 tile) — sampler แบบ Repeat
        // ทำให้ texture ปูซ้ำข้าม quad ที่ greedy merge แล้วได้เอง
        // แกน y กลับทิศให้หัว texture อยู่ด้านบนของบล็อก
        let face_uv = move |p: [f32; 3]| -> [f32; 2] {
            match a {
                1 => [p[0], p[2]],
                0 => [p[2], -p[1]],
                _ => [p[0], -p[1]],
            }
        };

        // mask ของ slice: Some((ชนิดบล็อก, ระดับ AO สม่ำเสมอ, ลาย texture)) = รอ merge
        // ลายอยู่ใน key ด้วย — หน้าที่ลายต่างกัน merge รวมกันไม่ได้
        let mut mask: Vec<Option<(BlockType, u8, u8)>> = vec![None; (lu * lv) as usize];

        for s in 0..la {
            for m in mask.iter_mut() {
                *m = None;
            }

            for vi in 0..lv {
                for ui in 0..lu {
                    let mut c = [0i32; 3];
                    c[a] = s;
                    c[ua] = ui;
                    c[va] = vi;

                    let block = blocks[ChunkData::get_index(c[0] as usize, c[1] as usize, c[2] as usize)];
                    // TallGrass ไม่ใช่ลูกบาศก์ — วาดแยกเป็นกากบาทท้ายฟังก์ชัน
                    if block == BlockType::Air || block == BlockType::TallGrass {
                        continue;
                    }

                    // เห็นหน้านี้เมื่อเพื่อนบ้านโปร่งใส (อากาศ/น้ำ/กระจก/หญ้าสูง)
                    // แต่บล็อกโปร่งใสชนิดเดียวกันติดกันไม่วาดหน้าใน (น้ำ-น้ำ, กระจก-กระจก)
                    let n = sample(c[0] + norm[0], c[1] + norm[1], c[2] + norm[2]);
                    let visible = n == BlockType::Air || (block_def(n).transparent && n != block);
                    if !visible {
                        continue;
                    }

                    // พู่ห้อยเอียง: ขอบบนแนบสันบล็อก ชายล่างยื่นออกตาม normal
                    // (เฉพาะหน้าด้านข้างของบล็อกที่มี overlay เช่นหญ้า)
                    if face_id >= 2 {
                        if let Some(overlay) = side_overlay_pick(
                            block,
                            face_id,
                            world_base_x + c[0],
                            c[1],
                            world_base_z + c[2],
                        ) {
                            const TILT: f32 = 0.3;
                            let mut verts = [[0f32; 3]; 4];
                            let mut uvs = [[0f32; 2]; 4];
                            for i in 0..4 {
                                let p = CUBE_POSITIONS[face_id][i];
                                let flat = [p[0] + c[0] as f32, p[1] + c[1] as f32, p[2] + c[2] as f32];
                                uvs[i] = face_uv(flat);
                                let mut v = flat;
                                if p[1] < 0.5 {
                                    v[0] += norm[0] as f32 * TILT;
                                    v[2] += norm[2] as f32 * TILT;
                                }
                                verts[i] = v;
                            }
                            // normal ชี้ขึ้น — รับแสงเหมือนพื้นหญ้าด้านบน
                            texture_buf(&mut set.deco, overlay)
                                .push_quad(verts, [0., 1., 0.], [[1.0; 4]; 4], uvs, false);
                        }
                    }

                    // บล็อกโปร่งใส (น้ำ/กระจก) และบล็อกเรืองแสงไม่คิด AO
                    let ao = if block_def(block).transparent || lamp_emission(block).is_some() {
                        [3u8; 4]
                    } else {
                        face_ao(c, face_id)
                    };

                    let variant = texture_variant(
                        block,
                        face_id,
                        world_base_x + c[0],
                        c[1],
                        world_base_z + c[2],
                    );

                    if ao[0] == ao[1] && ao[1] == ao[2] && ao[2] == ao[3] {
                        mask[midx(ui, vi)] = Some((block, ao[0], variant));
                    } else {
                        // AO ไล่เฉดภายในหน้า — merge ไม่ได้ วาดเดี่ยวพร้อม flip diagonal
                        let tex = face_texture(block, face_id, variant);
                        let base = if tex.is_some() { [1.0, 1.0, 1.0, 1.0] } else { block_color(block) };
                        let shade = FACE_SHADE[face_id];
                        let mut verts = [[0f32; 3]; 4];
                        let mut cols = [[0f32; 4]; 4];
                        let mut uvs = [[0f32; 2]; 4];
                        for i in 0..4 {
                            let p = CUBE_POSITIONS[face_id][i];
                            verts[i] = [p[0] + c[0] as f32, p[1] + c[1] as f32, p[2] + c[2] as f32];
                            let br = shade * AO_CURVE[ao[i] as usize];
                            cols[i] = [base[0] * br, base[1] * br, base[2] * br, base[3]];
                            uvs[i] = face_uv(verts[i]);
                        }
                        let flip = (ao[0] as u32 + ao[2] as u32) < (ao[1] as u32 + ao[3] as u32);
                        let buf = if let Some(t) = tex {
                            texture_buf(&mut set.textured, t)
                        } else {
                            &mut set.solid
                        };
                        buf.push_quad(verts, CUBE_NORMALS[face_id], cols, uvs, flip);
                    }
                }
            }

            // greedy merge: ขยายความกว้างก่อน แล้วขยายความสูงทั้งแถบ
            for vi in 0..lv {
                for ui in 0..lu {
                    let Some(key) = mask[midx(ui, vi)] else { continue };

                    let mut w = 1;
                    while ui + w < lu && mask[midx(ui + w, vi)] == Some(key) {
                        w += 1;
                    }
                    let mut h = 1;
                    'grow: while vi + h < lv {
                        for k in 0..w {
                            if mask[midx(ui + k, vi + h)] != Some(key) {
                                break 'grow;
                            }
                        }
                        h += 1;
                    }
                    for dv in 0..h {
                        for du in 0..w {
                            mask[midx(ui + du, vi + dv)] = None;
                        }
                    }

                    let (block, ao_level, variant) = key;
                    let is_water = block == BlockType::Water;
                    let is_glass = block == BlockType::Glass;
                    let is_lamp = lamp_emission(block).is_some();
                    let tex = if is_water || is_glass || is_lamp {
                        None
                    } else {
                        face_texture(block, face_id, variant)
                    };
                    let base = if tex.is_some() { [1.0, 1.0, 1.0, 1.0] } else { block_color(block) };
                    let br = FACE_SHADE[face_id] * AO_CURVE[ao_level as usize];
                    let col = [base[0] * br, base[1] * br, base[2] * br, base[3]];

                    let mut verts = [[0f32; 3]; 4];
                    let mut uvs = [[0f32; 2]; 4];
                    for i in 0..4 {
                        let p = CUBE_POSITIONS[face_id][i];
                        let mut out = [0f32; 3];
                        out[a] = s as f32 + p[a];
                        out[ua] = if p[ua] < 0.5 { ui as f32 } else { (ui + w) as f32 };
                        out[va] = if p[va] < 0.5 { vi as f32 } else { (vi + h) as f32 };
                        verts[i] = out;
                        uvs[i] = face_uv(out);
                    }

                    let buf = if is_water {
                        &mut set.water
                    } else if is_glass {
                        &mut set.glass
                    } else if is_lamp {
                        glow_buf(&mut set.glow, block)
                    } else if let Some(t) = tex {
                        texture_buf(&mut set.textured, t)
                    } else {
                        &mut set.solid
                    };
                    buf.push_quad(verts, CUBE_NORMALS[face_id], [col; 4], uvs, false);
                }
            }
        }
    }

    // ของประดับแบบกากบาท (Tall Grass): quad ทแยงสองแผ่น sprite alpha cutout
    // (วาดเมื่อมีไฟล์ sprite เท่านั้น) — normal ชี้ขึ้นให้โดนแสงเหมือนพื้นหญ้า
    if let Some(sprite) = face_texture(BlockType::TallGrass, 2, 0) {
        const CROSS_QUADS: [[[f32; 3]; 4]; 2] = [
            [[0., 0., 0.], [1., 0., 1.], [1., 1., 1.], [0., 1., 0.]],
            [[1., 0., 0.], [0., 0., 1.], [0., 1., 1.], [1., 1., 0.]],
        ];
        const CROSS_UVS: [[f32; 2]; 4] = [[0., 1.], [1., 1.], [1., 0.], [0., 0.]];

        for (i, block) in blocks.iter().enumerate() {
            if *block != BlockType::TallGrass {
                continue;
            }
            let x = (i % CHUNK_WIDTH) as f32;
            let y = ((i / CHUNK_WIDTH) % CHUNK_HEIGHT) as f32;
            let z = (i / (CHUNK_WIDTH * CHUNK_HEIGHT)) as f32;

            for quad in CROSS_QUADS {
                let mut verts = [[0f32; 3]; 4];
                for v in 0..4 {
                    verts[v] = [quad[v][0] + x, quad[v][1] + y, quad[v][2] + z];
                }
                texture_buf(&mut set.deco, sprite)
                    .push_quad(verts, [0., 1., 0.], [[1.0; 4]; 4], CROSS_UVS, false);
            }
        }
    }

    set
}

// --------------------------------------------------------
// Terrain generation
// --------------------------------------------------------

/// noise ทุกชั้นของ world gen — โหมด Full กับ Surface Preview ใช้ตัวเดียวกัน
/// เพื่อให้ terrain ที่เห็นตรงกันเป๊ะ
pub struct TerrainSampler {
    fbm: Fbm<Perlin>,
    temperature: Perlin,
    cave: Perlin,
    params: crate::NoiseParams,
}

impl TerrainSampler {
    pub fn new(params: crate::NoiseParams) -> Self {
        Self {
            fbm: Fbm::<Perlin>::new(1).set_octaves(params.octaves as usize),
            temperature: Perlin::new(2),
            cave: Perlin::new(3),
            params,
        }
    }

    pub fn height(&self, wx: f64, wz: f64) -> i32 {
        let n = self.fbm.get([wx * self.params.frequency, wz * self.params.frequency]);
        (SEA_LEVEL as f64 + n * self.params.amplitude).clamp(3.0, (CHUNK_HEIGHT - 1) as f64) as i32
    }

    /// biome ทะเลทราย (noise อุณหภูมิความถี่ต่ำ = ผืนใหญ่)
    pub fn is_desert(&self, wx: f64, wz: f64) -> bool {
        self.temperature.get([wx * 0.003, wz * 0.003]) > 0.5
    }

    pub fn is_cave(&self, wx: f64, y: i32, wz: f64) -> bool {
        self.cave.get([wx * 0.06, y as f64 * 0.06, wz * 0.06]) > 0.45
    }

    pub fn surface_block(&self, height: i32, desert: bool) -> BlockType {
        if desert || height <= SEA_LEVEL as i32 + 1 {
            BlockType::Sand
        } else {
            BlockType::Grass
        }
    }
}

fn generate_chunk_blocks(chunk_pos: IVec2, noise: crate::NoiseParams) -> Box<[BlockType; CHUNK_VOLUME]> {
    let sampler = TerrainSampler::new(noise);
    let mut blocks = Box::new([BlockType::Air; CHUNK_VOLUME]);

    let base_x = chunk_pos.x as f64 * CHUNK_WIDTH as f64;
    let base_z = chunk_pos.y as f64 * CHUNK_WIDTH as f64;

    let mut heights = [[0i32; CHUNK_WIDTH]; CHUNK_WIDTH];
    let mut desert = [[false; CHUNK_WIDTH]; CHUNK_WIDTH];

    for z in 0..CHUNK_WIDTH {
        for x in 0..CHUNK_WIDTH {
            let wx = base_x + x as f64;
            let wz = base_z + z as f64;
            heights[z][x] = sampler.height(wx, wz);
            desert[z][x] = sampler.is_desert(wx, wz);
        }
    }

    for z in 0..CHUNK_WIDTH {
        for x in 0..CHUNK_WIDTH {
            let wx = base_x + x as f64;
            let wz = base_z + z as f64;
            let h = heights[z][x];
            let is_desert = desert[z][x];
            let surface = sampler.surface_block(h, is_desert);

            for y in 0..CHUNK_HEIGHT {
                let yi = y as i32;
                let block = if yi < h - 3 {
                    BlockType::Stone
                } else if yi < h {
                    if is_desert { BlockType::Sand } else { BlockType::Dirt }
                } else if yi == h {
                    surface
                } else if yi <= SEA_LEVEL as i32 {
                    BlockType::Water
                } else {
                    break; // เหนือนี้เป็นอากาศทั้งหมด
                };

                // ถ้ำ: เจาะเฉพาะใต้ผิวลึกกว่า 4 บล็อก (ผิวโลก/ใต้ทะเลไม่ทะลุ)
                if block.is_solid() && yi < h - 4 && yi > 2 && sampler.is_cave(wx, yi, wz) {
                    continue;
                }
                blocks[ChunkData::get_index(x, y, z)] = block;
            }
        }
    }

    // ต้นไม้: ตำแหน่ง deterministic จากพิกัด chunk (xorshift) วางเฉพาะ
    // ในเขต 2..=13 ให้พุ่มใบไม่ล้ำออกนอก chunk
    let mut state: u64 = (chunk_pos.x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (chunk_pos.y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ 0x5851_F42D_4C95_7F2D;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    let tree_count = (next() % 3) as usize;
    for _ in 0..tree_count {
        let tx = 2 + (next() % 12) as usize;
        let tz = 2 + (next() % 12) as usize;
        let h = heights[tz][tx];
        if desert[tz][tx] || h <= SEA_LEVEL as i32 + 1 || h + 7 >= CHUNK_HEIGHT as i32 {
            continue;
        }

        for ty in (h + 1)..=(h + 5) {
            blocks[ChunkData::get_index(tx, ty as usize, tz)] = BlockType::Wood;
        }
        for ly in (h + 3)..=(h + 6) {
            let r: i32 = if ly <= h + 4 { 2 } else { 1 };
            for dz in -r..=r {
                for dx in -r..=r {
                    if r == 2 && dx.abs() == 2 && dz.abs() == 2 {
                        continue; // ตัดมุมพุ่ม
                    }
                    if ly == h + 6 && dx.abs() + dz.abs() > 1 {
                        continue; // ยอดเป็นกากบาท
                    }
                    let lx = (tx as i32 + dx) as usize;
                    let lz = (tz as i32 + dz) as usize;
                    let idx = ChunkData::get_index(lx, ly as usize, lz);
                    if blocks[idx] == BlockType::Air {
                        blocks[idx] = BlockType::Leaves;
                    }
                }
            }
        }
    }

    // หญ้าสูง: โปรยบนผิวหญ้า (ไม่ขึ้นในทะเลทราย/ใต้น้ำ)
    let tuft_count = (next() % 14) as usize;
    for _ in 0..tuft_count {
        let gx = (next() % CHUNK_WIDTH as u64) as usize;
        let gz = (next() % CHUNK_WIDTH as u64) as usize;
        let h = heights[gz][gx];
        if desert[gz][gx] || h <= SEA_LEVEL as i32 + 1 || h + 1 >= CHUNK_HEIGHT as i32 {
            continue;
        }
        let surface_idx = ChunkData::get_index(gx, h as usize, gz);
        let above_idx = ChunkData::get_index(gx, (h + 1) as usize, gz);
        if blocks[surface_idx] == BlockType::Grass && blocks[above_idx] == BlockType::Air {
            blocks[above_idx] = BlockType::TallGrass;
        }
    }

    blocks
}

// --------------------------------------------------------
// Save / Load (เก็บ chunk ที่ผู้เล่นแก้ไขลง disk)
// --------------------------------------------------------

/// root ของโปรเจกต์ — ไม่ใช้ path สัมพัทธ์ตรงๆ เพราะ working directory
/// เปลี่ยนได้ตามว่ารันจากไหน (เช่น cargo run จากใน src/)
pub fn project_root() -> std::path::PathBuf {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    if root.exists() {
        root.to_path_buf()
    } else {
        // ไบนารีถูกย้ายไปเครื่องอื่น — ใช้ working directory ตามเดิม
        std::path::PathBuf::from(".")
    }
}

fn chunk_save_path(chunk_pos: IVec2) -> std::path::PathBuf {
    project_root().join(format!("saves/chunk_{}_{}.bin", chunk_pos.x, chunk_pos.y))
}

pub fn save_chunk(chunk_pos: IVec2, blocks: &[BlockType; CHUNK_VOLUME]) {
    let _ = std::fs::create_dir_all(project_root().join("saves"));
    let bytes: Vec<u8> = blocks.iter().map(|b| *b as u8).collect();
    if let Err(e) = std::fs::write(chunk_save_path(chunk_pos), bytes) {
        warn!("save chunk {:?} failed: {}", chunk_pos, e);
    }
}

fn load_chunk(chunk_pos: IVec2) -> Option<Box<[BlockType; CHUNK_VOLUME]>> {
    let bytes = std::fs::read(chunk_save_path(chunk_pos)).ok()?;
    // ขนาดไม่ตรง = เซฟจากโลกคนละขนาด (เช่นช่วงที่ CHUNK_HEIGHT ถูกแก้) — ทิ้ง
    if bytes.len() != CHUNK_VOLUME {
        return None;
    }
    let mut blocks = Box::new([BlockType::Air; CHUNK_VOLUME]);
    for (i, b) in bytes.iter().enumerate() {
        blocks[i] = BlockType::from_u8(*b);
    }
    Some(blocks)
}

// --------------------------------------------------------
// Async Chunk Generation
// --------------------------------------------------------

pub struct ChunkBlockData {
    pub chunk_pos: IVec2,
    pub blocks: Arc<[BlockType; CHUNK_VOLUME]>,
    pub version: u32,
}

pub struct ChunkMeshData {
    pub chunk_pos: IVec2,
    pub set: ChunkMeshSet,
    pub version: u32,
}

#[derive(Resource)]
pub struct ChunkGenerator {
    pub sender_blocks: Mutex<Sender<ChunkBlockData>>,
    pub receiver_blocks: Mutex<Receiver<ChunkBlockData>>,
    pub sender_meshes: Mutex<Sender<ChunkMeshData>>,
    pub receiver_meshes: Mutex<Receiver<ChunkMeshData>>,
    pub generating_blocks: HashMap<IVec2, bool>,
    pub generating_meshes: HashMap<IVec2, bool>,
    /// เพิ่มทีละ 1 ทุกครั้งที่ล้างโลก — ผลจาก task รุ่นเก่าจะถูกทิ้ง
    pub version: u32,
}

impl Default for ChunkGenerator {
    fn default() -> Self {
        let (sb, rb) = mpsc::channel();
        let (sm, rm) = mpsc::channel();
        Self {
            sender_blocks: Mutex::new(sb),
            receiver_blocks: Mutex::new(rb),
            sender_meshes: Mutex::new(sm),
            receiver_meshes: Mutex::new(rm),
            generating_blocks: HashMap::new(),
            generating_meshes: HashMap::new(),
            version: 0,
        }
    }
}

pub fn spawn_block_generation_task(
    chunk_pos: IVec2,
    noise: crate::NoiseParams,
    version: u32,
    sender: Sender<ChunkBlockData>,
) {
    AsyncComputeTaskPool::get().spawn(async move {
        // ถ้ามีไฟล์เซฟ (ผู้เล่นเคยแก้ chunk นี้) ใช้ของเซฟแทนการ generate
        let blocks = load_chunk(chunk_pos).unwrap_or_else(|| generate_chunk_blocks(chunk_pos, noise));
        let _ = sender.send(ChunkBlockData {
            chunk_pos,
            blocks: Arc::from(*blocks),
            version,
        });
    }).detach();
}

pub fn spawn_mesh_generation_task(
    chunk_pos: IVec2,
    blocks: Arc<[BlockType; CHUNK_VOLUME]>,
    neighbors: [Arc<[BlockType; CHUNK_VOLUME]>; 8],
    version: u32,
    sender: Sender<ChunkMeshData>,
) {
    AsyncComputeTaskPool::get().spawn(async move {
        let set = create_mesh_from_blocks(chunk_pos, &blocks, &neighbors);
        let _ = sender.send(ChunkMeshData { chunk_pos, set, version });
    }).detach();
}

/// Preview mode: สร้าง mesh เฉพาะผิวโลกจาก noise ตรงๆ ต่อ column
/// (หน้าบน + ผนังด้านที่สูงกว่าเพื่อนบ้าน + ผิวน้ำ) — ไม่ต้องมี block volume
/// และไม่ขึ้นกับ chunk ข้างเคียง เพราะ sample noise ข้ามขอบได้เลย
pub fn spawn_surface_preview_task(
    chunk_pos: IVec2,
    noise: crate::NoiseParams,
    version: u32,
    sender: Sender<ChunkMeshData>,
) {
    AsyncComputeTaskPool::get().spawn(async move {
        let sampler = TerrainSampler::new(noise);
        let base_x = chunk_pos.x as f64 * CHUNK_WIDTH as f64;
        let base_z = chunk_pos.y as f64 * CHUNK_WIDTH as f64;

        let height_at = |x: i32, z: i32| -> i32 {
            sampler.height(base_x + x as f64, base_z + z as f64)
        };

        let mut solid = MeshBuf::default();
        let mut water = MeshBuf::default();

        // วาง quad ของหน้า face_id ที่ column (x, z) โดย map พิกัด y ของ
        // CUBE_POSITIONS (0/1) ไปเป็นช่วง y_lo..y_hi (ผนังสูงกี่บล็อกก็ quad เดียว)
        let push_face = |buf: &mut MeshBuf, face_id: usize, x: f32, z: f32, y_lo: f32, y_hi: f32, color: [f32; 4]| {
            let mut verts = [[0f32; 3]; 4];
            for i in 0..4 {
                let p = CUBE_POSITIONS[face_id][i];
                verts[i] = [p[0] + x, if p[1] < 0.5 { y_lo } else { y_hi }, p[2] + z];
            }
            buf.push_quad(verts, CUBE_NORMALS[face_id], [color; 4], [[0.0, 0.0]; 4], false);
        };

        let shaded = |block: BlockType, face_id: usize| -> [f32; 4] {
            let c = block_color(block);
            let s = FACE_SHADE[face_id];
            [c[0] * s, c[1] * s, c[2] * s, c[3]]
        };

        // ทิศข้าง: (dx, dz, face_id)
        let sides = [(1i32, 0i32, 2usize), (-1, 0, 3), (0, 1, 4), (0, -1, 5)];

        for z in 0..CHUNK_WIDTH as i32 {
            for x in 0..CHUNK_WIDTH as i32 {
                let h = height_at(x, z);
                let is_desert = sampler.is_desert(base_x + x as f64, base_z + z as f64);
                let top = sampler.surface_block(h, is_desert);
                let side = if is_desert { BlockType::Sand } else { BlockType::Dirt };

                // หน้าบนของบล็อกผิว (บล็อก y = h กินพื้นที่ถึง y = h + 1)
                push_face(&mut solid, 0, x as f32, z as f32, h as f32, (h + 1) as f32, shaded(top, 0));

                // ผนังด้านที่ column นี้สูงกว่าเพื่อนบ้าน
                for (dx, dz, face_id) in sides {
                    let hn = height_at(x + dx, z + dz);
                    if hn < h {
                        push_face(
                            &mut solid,
                            face_id,
                            x as f32,
                            z as f32,
                            (hn + 1) as f32,
                            (h + 1) as f32,
                            shaded(side, face_id),
                        );
                    }
                }

                // ผิวน้ำที่ระดับ SEA_LEVEL
                if h < SEA_LEVEL as i32 {
                    push_face(
                        &mut water,
                        0,
                        x as f32,
                        z as f32,
                        SEA_LEVEL as f32,
                        SEA_LEVEL as f32 + 1.0,
                        shaded(BlockType::Water, 0),
                    );
                }
            }
        }

        let _ = sender.send(ChunkMeshData {
            chunk_pos,
            set: ChunkMeshSet { solid, water, ..Default::default() },
            version,
        });
    }).detach();
}

// --------------------------------------------------------
// Setup & Systems
// --------------------------------------------------------

#[derive(Resource)]
pub struct ChunkMaterial(pub Handle<StandardMaterial>);

#[derive(Resource)]
pub struct WaterMaterial(pub Handle<StandardMaterial>);

/// material แบบ emissive ของบล็อกเรืองแสงแต่ละสี
#[derive(Resource)]
pub struct LampMaterials(pub HashMap<BlockType, Handle<StandardMaterial>>);

/// material ต่อไฟล์ texture (สร้างเฉพาะไฟล์ที่มีจริงตอนเปิดเกม)
#[derive(Resource)]
pub struct BlockMaterials(pub HashMap<&'static str, Handle<StandardMaterial>>);

#[derive(Resource)]
pub struct GlassMaterial(pub Handle<StandardMaterial>);

/// material ของของประดับ (alpha cutout สองหน้า) ต่อไฟล์ sprite
#[derive(Resource)]
pub struct DecoMaterials(pub HashMap<&'static str, Handle<StandardMaterial>>);

pub fn setup_voxel(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    // สร้างตาราง texture ต่อ (บล็อก, หน้า) — เอาเฉพาะไฟล์ที่มีจริงบน disk
    // ไฟล์ไหนไม่มี หน้านั้น fallback เป็น vertex color (เพิ่มรูปแล้วต้อง restart)
    let mut face_table: Vec<[Vec<&'static str>; 6]> = Vec::with_capacity(BLOCK_DEFS.len());
    let mut block_materials: HashMap<&'static str, Handle<StandardMaterial>> = HashMap::new();
    let mut missing: Vec<&'static str> = Vec::new();

    for def in BLOCK_DEFS.iter() {
        let mut faces: [Vec<&'static str>; 6] = Default::default();
        let per_face = [
            (0usize, def.tex_top),
            (1, def.tex_bottom),
            (2, def.tex_side),
            (3, def.tex_side),
            (4, def.tex_side),
            (5, def.tex_side),
        ];
        for (face_id, texs) in per_face {
            for path in texs {
                if project_root().join("assets").join(path).exists() {
                    faces[face_id].push(path);
                    block_materials.entry(path).or_insert_with(|| {
                        materials.add(StandardMaterial {
                            base_color: Color::WHITE,
                            base_color_texture: Some(asset_server.load(*path)),
                            perceptual_roughness: 1.0,
                            ..default()
                        })
                    });
                } else if !missing.contains(path) {
                    missing.push(path);
                }
            }
        }
        face_table.push(faces);
    }
    let _ = FACE_TEXTURES.set(face_table);
    commands.insert_resource(BlockMaterials(block_materials));

    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.insert_resource(ChunkMaterial(material));

    // สีน้ำมาจาก vertex color — material เป็นสีขาวโปร่งใสคูณทับ
    let water_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.55),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.15,
        ..default()
    });
    commands.insert_resource(WaterMaterial(water_material));

    // กระจก: โปร่งใสกว่าน้ำ ผิวเรียบสะท้อนแสง
    let glass_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.80, 0.90, 1.0, 0.30),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.08,
        ..default()
    });
    commands.insert_resource(GlassMaterial(glass_material));

    // sprite ของประดับ (หญ้าสูง + พู่หญ้าข้างบล็อก): alpha cutout + วาดสองหน้า
    // รวบรวมจาก overlay_side ของทุกบล็อก + sprite กากบาทของ Tall Grass
    let mut side_overlays: Vec<Vec<&'static str>> = Vec::with_capacity(BLOCK_DEFS.len());
    let mut deco_materials: HashMap<&'static str, Handle<StandardMaterial>> = HashMap::new();
    let mut cutout_sprites: Vec<&'static str> =
        BLOCK_DEFS[BlockType::TallGrass as usize].tex_side.to_vec();

    for def in BLOCK_DEFS.iter() {
        let mut overlays = Vec::new();
        for path in def.overlay_side {
            if project_root().join("assets").join(path).exists() {
                overlays.push(*path);
                cutout_sprites.push(*path);
            } else if !missing.contains(path) {
                missing.push(path);
            }
        }
        side_overlays.push(overlays);
    }
    for path in cutout_sprites {
        if !project_root().join("assets").join(path).exists() {
            continue;
        }
        deco_materials.entry(path).or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: Some(asset_server.load(path)),
                alpha_mode: AlphaMode::Mask(0.5),
                cull_mode: None,
                double_sided: true,
                perceptual_roughness: 1.0,
                ..default()
            })
        });
    }
    let _ = SIDE_OVERLAYS.set(side_overlays);
    commands.insert_resource(DecoMaterials(deco_materials));

    if !missing.is_empty() {
        info!(
            "textures not found (using vertex colors instead): {}",
            missing.join(", ")
        );
    }

    // บล็อกเรืองแสง: emissive เกิน 1.0 เพื่อให้ bloom ฟุ้ง
    let mut lamp_materials = HashMap::new();
    for block in [BlockType::Glowstone, BlockType::LampRed, BlockType::LampGreen, BlockType::LampBlue] {
        let color = lamp_emission(block).unwrap();
        lamp_materials.insert(block, materials.add(StandardMaterial {
            base_color: color,
            emissive: color.to_linear() * 4.0,
            ..default()
        }));
    }
    commands.insert_resource(LampMaterials(lamp_materials));

    commands.insert_resource(VoxelWorld::default());
    commands.insert_resource(ChunkGenerator::default());

    // Sun
    commands.spawn((
        Sun,
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        // ระยะเงาต้องครอบคลุมพื้นที่ที่มองเห็น แต่ยิ่งไกลยิ่งกิน GPU
        bevy::light::CascadeShadowConfigBuilder {
            maximum_distance: 300.0,
            ..default()
        }
        .build(),
        Transform::default().looking_to(Vec3::new(-0.4, -1.0, -0.6).normalize(), Vec3::Y),
    ));
}

#[derive(Component)]
pub struct Sun;

/// หมุนดวงอาทิตย์ตาม time_of_day + ปรับความสว่าง/สีแดด, ambient, สีท้องฟ้า
pub fn update_sun_system(
    settings: Res<crate::GameSettings>,
    mut sun_query: Query<(&mut Transform, &mut DirectionalLight), With<Sun>>,
    mut ambient_query: Query<&mut AmbientLight>,
    mut clear_color: ResMut<ClearColor>,
) {
    let Ok((mut transform, mut light)) = sun_query.single_mut() else { return };

    // 6:00 ขึ้นขอบฟ้าทิศ +X, 12:00 กลางหัว, 18:00 ตกทิศ -X
    let hour_angle = (settings.time_of_day - 6.0) / 12.0 * std::f32::consts::PI;
    // เอียงแนวเหนือ-ใต้เล็กน้อย ให้เงาตอนเที่ยงไม่ตั้งฉากเป๊ะ
    let sun_dir = Vec3::new(hour_angle.cos(), hour_angle.sin(), 0.3).normalize();
    *transform = Transform::default().looking_to(-sun_dir, Vec3::Y);

    // ความสูงดวงอาทิตย์เหนือขอบฟ้า 0..1 (ต่ำกว่าขอบฟ้า = กลางคืน)
    let elevation = sun_dir.y.clamp(0.0, 1.0);
    light.illuminance = 10_000.0 * elevation.powf(0.7);

    // แดดอมส้มตอนใกล้ขอบฟ้า ขาวตอนกลางวัน
    let warm = 1.0 - (elevation * 2.0).min(1.0);
    light.color = Color::srgb(1.0, 1.0 - 0.3 * warm, 1.0 - 0.5 * warm);

    // กลางคืนเหลือ ambient สลัวๆ กลางวันสว่างเต็ม
    for mut ambient in ambient_query.iter_mut() {
        ambient.brightness = 80.0 + 320.0 * elevation;
    }

    // สีท้องฟ้า: กลางคืนน้ำเงินเข้ม -> กลางวันฟ้าสด
    let night = Vec3::new(0.02, 0.02, 0.06);
    let day = Vec3::new(0.35, 0.55, 0.90);
    let sky = night.lerp(day, elevation);
    clear_color.0 = Color::srgb(sky.x, sky.y, sky.z);
}

/// ล้างโลกทั้งหมดเพื่อ generate ใหม่ (ตอนเปลี่ยน render mode หรือค่า noise)
pub fn world_reset_system(
    mut commands: Commands,
    mut request: ResMut<crate::RegenerateWorld>,
    mut world: ResMut<VoxelWorld>,
    mut generator: ResMut<ChunkGenerator>,
) {
    if !request.0 {
        return;
    }
    request.0 = false;

    for (_, entity) in world.generated_chunks.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in world.water_chunks.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in world.glass_chunks.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entities) in world.deco_chunks.drain() {
        for entity in entities { commands.entity(entity).despawn(); }
    }
    for (_, entities) in world.glow_chunks.drain() {
        for entity in entities {
            commands.entity(entity).despawn();
        }
    }
    for (_, entities) in world.textured_chunks.drain() {
        for entity in entities {
            commands.entity(entity).despawn();
        }
    }
    for (_, entities) in world.lamp_lights.drain() {
        for entity in entities {
            commands.entity(entity).despawn();
        }
    }
    world.chunks.clear();
    world.total_vertices = 0;
    world.total_indices = 0;

    generator.generating_blocks.clear();
    generator.generating_meshes.clear();
    // ทำให้ผลจาก task ที่ยังค้างอยู่ใน pool กลายเป็นของเก่าและถูกทิ้ง
    generator.version += 1;
}

/// เพื่อนบ้าน 8 ทิศ ตามลำดับที่ create_mesh_from_blocks ต้องการ
fn chunk_neighbors(chunk_pos: IVec2) -> [IVec2; 8] {
    let (cx, cz) = (chunk_pos.x, chunk_pos.y);
    [
        IVec2::new(cx + 1, cz),     // +X
        IVec2::new(cx - 1, cz),     // -X
        IVec2::new(cx, cz + 1),     // +Z
        IVec2::new(cx, cz - 1),     // -Z
        IVec2::new(cx + 1, cz + 1), // +X+Z
        IVec2::new(cx + 1, cz - 1), // +X-Z
        IVec2::new(cx - 1, cz + 1), // -X+Z
        IVec2::new(cx - 1, cz - 1), // -X-Z
    ]
}

pub fn world_generation_system(
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    world: Res<VoxelWorld>,
    mut generator: ResMut<ChunkGenerator>,
    settings: Res<crate::GameSettings>,
    // cache offset เรียงจากใกล้ไปไกล (สร้างใหม่เมื่อ render distance เปลี่ยน)
    mut offsets_cache: Local<(i32, Vec<IVec2>)>,
) {
    let Some(camera_transform) = camera_query.iter().next() else { return };
    let cam_pos = camera_transform.translation;

    let center_chunk_x = cam_pos.x.div_euclid(CHUNK_WIDTH as f32) as i32;
    let center_chunk_z = cam_pos.z.div_euclid(CHUNK_WIDTH as f32) as i32;

    let render_distance = settings.render_distance;

    if offsets_cache.0 != render_distance || offsets_cache.1.is_empty() {
        let mut offsets = Vec::new();
        for dx in -render_distance..=render_distance {
            for dz in -render_distance..=render_distance {
                offsets.push(IVec2::new(dx, dz));
            }
        }
        offsets.sort_by_key(|o| o.x * o.x + o.y * o.y);
        *offsets_cache = (render_distance, offsets);
    }

    // จำกัดจำนวน task ต่อเฟรม: chunk ใกล้ตัวได้คิวก่อน และเฟรมไม่สะดุด
    let mut block_budget: usize = 6;
    let mut mesh_budget: usize = 8;

    for offset in offsets_cache.1.iter() {
        if block_budget == 0 && mesh_budget == 0 {
            break;
        }

        let cx = center_chunk_x + offset.x;
        let cz = center_chunk_z + offset.y;
        let chunk_pos = IVec2::new(cx, cz);

        // Preview: ข้ามการ gen block volume ไปสร้าง mesh ผิวโลกเลย
        if settings.render_mode == crate::RenderMode::SurfacePreview {
            if mesh_budget > 0
                && !world.generated_chunks.contains_key(&chunk_pos)
                && !generator.generating_meshes.contains_key(&chunk_pos)
            {
                generator.generating_meshes.insert(chunk_pos, true);
                let sender = generator.sender_meshes.lock().unwrap().clone();
                spawn_surface_preview_task(chunk_pos, settings.noise, generator.version, sender);
                mesh_budget -= 1;
            }
            continue;
        }

        // Phase 1: Block Generation
        if block_budget > 0
            && !world.chunks.contains_key(&chunk_pos)
            && !generator.generating_blocks.contains_key(&chunk_pos)
        {
            generator.generating_blocks.insert(chunk_pos, true);
            let sender = generator.sender_blocks.lock().unwrap().clone();
            spawn_block_generation_task(chunk_pos, settings.noise, generator.version, sender);
            block_budget -= 1;
        }

        // Phase 2: Mesh Generation
        if mesh_budget > 0
            && world.chunks.contains_key(&chunk_pos)
            && !world.generated_chunks.contains_key(&chunk_pos)
            && !generator.generating_meshes.contains_key(&chunk_pos)
        {
            // เพื่อนบ้านต้องมี block data ครบทั้ง 8 (รวมทแยง เพื่อ AO)
            let neighbors_pos = chunk_neighbors(chunk_pos);
            let all_neighbors_ready = neighbors_pos.iter().all(|p| world.chunks.contains_key(p));

            if all_neighbors_ready {
                generator.generating_meshes.insert(chunk_pos, true);

                let blocks = world.chunks.get(&chunk_pos).unwrap().blocks.clone();
                let neighbors = neighbors_pos.map(|p| world.chunks.get(&p).unwrap().blocks.clone());

                let sender = generator.sender_meshes.lock().unwrap().clone();
                spawn_mesh_generation_task(chunk_pos, blocks, neighbors, generator.version, sender);
                mesh_budget -= 1;
            }
        }
    }
}

/// สลับชุด mesh เรืองแสงของ chunk (entity เก่าทิ้ง สร้างใหม่ตาม buffer ที่ได้มา)
fn update_glow_entities(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    meshes: &mut Assets<Mesh>,
    lamp_materials: &LampMaterials,
    chunk_pos: IVec2,
    glow: Vec<(BlockType, MeshBuf)>,
    transform: Transform,
) {
    if let Some(old) = world.glow_chunks.remove(&chunk_pos) {
        for entity in old {
            commands.entity(entity).despawn();
        }
    }

    let mut entities = Vec::new();
    for (block, buf) in glow {
        if buf.is_empty() {
            continue;
        }
        let Some(material) = lamp_materials.0.get(&block) else { continue };
        let entity = commands.spawn((
            Mesh3d(meshes.add(buf.into_mesh())),
            MeshMaterial3d(material.clone()),
            transform,
            Block,
        )).id();
        entities.push(entity);
    }
    if !entities.is_empty() {
        world.glow_chunks.insert(chunk_pos, entities);
    }
}

/// สลับชุด mesh แบบ deco ของ chunk (entity เก่าทิ้ง สร้างใหม่)
fn update_deco_entities(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    meshes: &mut Assets<Mesh>,
    deco_materials: &DecoMaterials,
    chunk_pos: IVec2,
    deco: Vec<(&'static str, MeshBuf)>,
    transform: Transform,
) {
    if let Some(old) = world.deco_chunks.remove(&chunk_pos) {
        for entity in old {
            commands.entity(entity).despawn();
        }
    }

    let mut entities = Vec::new();
    for (tex, buf) in deco {
        if buf.is_empty() {
            continue;
        }
        let Some(material) = deco_materials.0.get(tex) else { continue };
        let entity = commands.spawn((
            Mesh3d(meshes.add(buf.into_mesh())),
            MeshMaterial3d(material.clone()),
            transform,
            NotShadowCaster,
            Block,
        )).id();
        entities.push(entity);
    }
    if !entities.is_empty() {
        world.deco_chunks.insert(chunk_pos, entities);
    }
}

/// สลับชุด mesh แบบมี texture ของ chunk (entity เก่าทิ้ง สร้างใหม่)
fn update_textured_entities(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    meshes: &mut Assets<Mesh>,
    block_materials: &BlockMaterials,
    chunk_pos: IVec2,
    textured: Vec<(&'static str, MeshBuf)>,
    transform: Transform,
) {
    if let Some(old) = world.textured_chunks.remove(&chunk_pos) {
        for entity in old {
            commands.entity(entity).despawn();
        }
    }

    let mut entities = Vec::new();
    for (tex, buf) in textured {
        if buf.is_empty() {
            continue;
        }
        let Some(material) = block_materials.0.get(tex) else { continue };
        let entity = commands.spawn((
            Mesh3d(meshes.add(buf.into_mesh())),
            MeshMaterial3d(material.clone()),
            transform,
            Block,
        )).id();
        entities.push(entity);
    }
    if !entities.is_empty() {
        world.textured_chunks.insert(chunk_pos, entities);
    }
}

/// อัปเดต mesh entity เดี่ยวของ chunk (น้ำ/กระจก/ของประดับ):
/// buffer ว่าง = ลบ entity, มีอยู่แล้ว = เขียนทับ asset เดิม, ยังไม่มี = สร้างใหม่
fn update_single_mesh_entity(
    commands: &mut Commands,
    map: &mut HashMap<IVec2, Entity>,
    meshes: &mut Assets<Mesh>,
    mesh_query: &Query<&Mesh3d>,
    material: &Handle<StandardMaterial>,
    chunk_pos: IVec2,
    buf: MeshBuf,
    transform: Transform,
) {
    if buf.is_empty() {
        if let Some(entity) = map.remove(&chunk_pos) {
            commands.entity(entity).despawn();
        }
    } else if let Some(&entity) = map.get(&chunk_pos) {
        if let Ok(mesh3d) = mesh_query.get(entity) {
            let _ = meshes.insert(mesh3d.0.id(), buf.into_mesh());
            commands.entity(entity).remove::<Aabb>();
        } else {
            commands.entity(entity)
                .insert(Mesh3d(meshes.add(buf.into_mesh())))
                .remove::<Aabb>();
        }
    } else {
        let entity = commands.spawn((
            Mesh3d(meshes.add(buf.into_mesh())),
            MeshMaterial3d(material.clone()),
            transform,
            NotShadowCaster,
            Block,
        )).id();
        map.insert(chunk_pos, entity);
    }
}

/// สแกน chunk แล้ว spawn PointLight ให้บล็อกไฟทุกก้อน — สีจากชนิดบล็อก
/// แสงจากหลายดวงผสมกันแบบ additive (แดง+น้ำเงิน = ม่วง) โดย renderer เอง
fn refresh_chunk_lamp_lights(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    chunk_pos: IVec2,
) {
    if let Some(old) = world.lamp_lights.remove(&chunk_pos) {
        for entity in old {
            commands.entity(entity).despawn();
        }
    }

    let Some(chunk) = world.chunks.get(&chunk_pos) else { return };

    let base_x = (chunk_pos.x * CHUNK_WIDTH as i32) as f32;
    let base_z = (chunk_pos.y * CHUNK_WIDTH as i32) as f32;

    let mut lights = Vec::new();
    for (i, block) in chunk.blocks.iter().enumerate() {
        let Some(color) = lamp_emission(*block) else { continue };
        // ถอดพิกัดกลับจาก index (x + y*W + z*W*H)
        let x = i % CHUNK_WIDTH;
        let y = (i / CHUNK_WIDTH) % CHUNK_HEIGHT;
        let z = i / (CHUNK_WIDTH * CHUNK_HEIGHT);

        let entity = commands.spawn((
            PointLight {
                color,
                intensity: 100_000.0,
                range: 14.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(
                base_x + x as f32 + 0.5,
                y as f32 + 0.5,
                base_z + z as f32 + 0.5,
            ),
        )).id();
        lights.push(entity);
    }
    if !lights.is_empty() {
        world.lamp_lights.insert(chunk_pos, lights);
    }
}

pub fn process_generated_chunks_system(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    mut generator: ResMut<ChunkGenerator>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunk_material: Res<ChunkMaterial>,
    water_material: Res<WaterMaterial>,
    glass_material: Res<GlassMaterial>,
    deco_material: Res<DecoMaterials>,
    lamp_materials: Res<LampMaterials>,
    block_materials: Res<BlockMaterials>,
) {
    // Process Blocks
    let mut received_blocks = Vec::new();
    {
        let receiver = generator.receiver_blocks.lock().unwrap();
        while let Ok(block_data) = receiver.try_recv() {
            received_blocks.push(block_data);
        }
    }

    for block_data in received_blocks {
        // ผลจากโลกรุ่นเก่า (ก่อน reset) — ทิ้งไปเลย ห้ามแตะ generating maps
        // เพราะอาจมี task รุ่นใหม่ของ chunk เดียวกันกำลังทำงานอยู่
        if block_data.version != generator.version {
            continue;
        }
        let chunk_pos = block_data.chunk_pos;
        world.chunks.insert(chunk_pos, ChunkData {
            blocks: block_data.blocks,
            num_vertices: 0,
            num_indices: 0,
        });
        generator.generating_blocks.remove(&chunk_pos);
    }

    // Process Meshes
    let mut received_meshes = Vec::new();
    {
        let receiver = generator.receiver_meshes.lock().unwrap();
        while let Ok(mesh_data) = receiver.try_recv() {
            received_meshes.push(mesh_data);
        }
    }

    for mesh_data in received_meshes {
        if mesh_data.version != generator.version {
            continue;
        }
        let ChunkMeshData { chunk_pos, set, .. } = mesh_data;
        let transform = Transform::from_xyz(
            (chunk_pos.x * CHUNK_WIDTH as i32) as f32,
            0.0,
            (chunk_pos.y * CHUNK_WIDTH as i32) as f32,
        );

        let num_vertices = set.total_vertices();
        let num_indices = set.total_indices();
        let ChunkMeshSet { solid, water, glass, deco, glow, textured } = set;

        // นับสถิติเฉพาะ chunk ที่มี block data อยู่จริง — mesh ที่มาถึงหลัง
        // chunk ถูก unload (หรือ mesh ของ preview mode) จะไม่ถูกนับ กันตัวเลขรั่ว
        if let Some(chunk_data) = world.chunks.get_mut(&chunk_pos) {
            chunk_data.num_vertices = num_vertices;
            chunk_data.num_indices = num_indices;
            world.total_vertices += num_vertices;
            world.total_indices += num_indices;
        }

        // ห้ามสร้าง mesh เปล่า (0 vertex) — กระตุ้นบั๊ก slab allocator ของ bevy 0.19
        // แต่ entity ต้องมีเสมอ เพราะ generated_chunks ใช้เป็น marker ว่า chunk เสร็จแล้ว
        let mut chunk_entity = commands.spawn((transform, Block));
        if !solid.is_empty() {
            chunk_entity.insert((
                Mesh3d(meshes.add(solid.into_mesh())),
                MeshMaterial3d(chunk_material.0.clone()),
            ));
        }
        let entity = chunk_entity.id();
        world.generated_chunks.insert(chunk_pos, entity);

        if !water.is_empty() {
            let water_entity = commands.spawn((
                Mesh3d(meshes.add(water.into_mesh())),
                MeshMaterial3d(water_material.0.clone()),
                transform,
                NotShadowCaster,
                Block,
            )).id();
            world.water_chunks.insert(chunk_pos, water_entity);
        }

        if !glass.is_empty() {
            let glass_entity = commands.spawn((
                Mesh3d(meshes.add(glass.into_mesh())),
                MeshMaterial3d(glass_material.0.clone()),
                transform,
                NotShadowCaster,
                Block,
            )).id();
            world.glass_chunks.insert(chunk_pos, glass_entity);
        }

        update_deco_entities(&mut commands, &mut world, &mut meshes, &deco_material, chunk_pos, deco, transform);

        update_glow_entities(&mut commands, &mut world, &mut meshes, &lamp_materials, chunk_pos, glow, transform);
        update_textured_entities(&mut commands, &mut world, &mut meshes, &block_materials, chunk_pos, textured, transform);
        refresh_chunk_lamp_lights(&mut commands, &mut world, chunk_pos);

        generator.generating_meshes.remove(&chunk_pos);
    }
}

pub fn chunk_unloading_system(
    mut commands: Commands,
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut world: ResMut<VoxelWorld>,
    settings: Res<crate::GameSettings>,
) {
    let Some(camera_transform) = camera_query.iter().next() else { return };
    let cam_pos = camera_transform.translation;

    let center_chunk_x = cam_pos.x.div_euclid(CHUNK_WIDTH as f32) as i32;
    let center_chunk_z = cam_pos.z.div_euclid(CHUNK_WIDTH as f32) as i32;

    // Unload chunks that are outside render distance + 2
    let unload_distance = settings.render_distance + 2;

    let is_out_of_range = |chunk_pos: IVec2| {
        (chunk_pos.x - center_chunk_x).abs() > unload_distance
            || (chunk_pos.y - center_chunk_z).abs() > unload_distance
    };

    // รวม chunk ที่มีแค่ block data (ยังไม่มี mesh) ด้วย ไม่งั้นวงนอกสุดของ
    // render distance จะค้างอยู่ใน world.chunks ตลอดกาล (memory leak)
    let mut to_unload: Vec<IVec2> = world.chunks.keys()
        .copied()
        .filter(|&pos| is_out_of_range(pos))
        .collect();
    to_unload.extend(
        world.generated_chunks.keys()
            .copied()
            .filter(|&pos| is_out_of_range(pos) && !world.chunks.contains_key(&pos))
    );

    for pos in to_unload {
        if let Some(entity) = world.generated_chunks.remove(&pos) {
            commands.entity(entity).despawn();
        }
        if let Some(entity) = world.water_chunks.remove(&pos) {
            commands.entity(entity).despawn();
        }
        if let Some(entity) = world.glass_chunks.remove(&pos) {
            commands.entity(entity).despawn();
        }
        if let Some(entities) = world.deco_chunks.remove(&pos) {
            for entity in entities { commands.entity(entity).despawn(); }
        }
        if let Some(entities) = world.glow_chunks.remove(&pos) {
            for entity in entities {
                commands.entity(entity).despawn();
            }
        }
        if let Some(entities) = world.textured_chunks.remove(&pos) {
            for entity in entities {
                commands.entity(entity).despawn();
            }
        }
        if let Some(entities) = world.lamp_lights.remove(&pos) {
            for entity in entities {
                commands.entity(entity).despawn();
            }
        }
        if let Some(chunk_data) = world.chunks.remove(&pos) {
            world.total_vertices -= chunk_data.num_vertices;
            world.total_indices -= chunk_data.num_indices;
        }
    }
}

// --------------------------------------------------------
// Raycast & Block Interaction
// --------------------------------------------------------

#[derive(Clone, Copy)]
pub struct TargetHit {
    pub pos: IVec3,
    pub normal: IVec3,
    pub block: BlockType,
}

/// ผล raycast ของเฟรมนี้ — ให้ระบบอื่น (UI, interaction) อ่านต่อ
#[derive(Resource, Default)]
pub struct TargetedBlock(pub Option<TargetHit>);

/// บล็อกที่เลือกไว้สำหรับวาง (กด 1-0 และ -)
#[derive(Resource)]
pub struct SelectedBlock(pub BlockType);

impl Default for SelectedBlock {
    fn default() -> Self {
        Self(BlockType::Dirt)
    }
}

pub fn voxel_raycast_system(
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    world: Res<VoxelWorld>,
    mut target: ResMut<TargetedBlock>,
    mut gizmos: Gizmos,
) {
    target.0 = None;

    let Some(camera_transform) = camera_query.iter().next() else { return };
    let origin = camera_transform.translation;
    let dir = camera_transform.forward().normalize();

    let max_dist = 6.0;

    let mut map_x = origin.x.floor() as i32;
    let mut map_y = origin.y.floor() as i32;
    let mut map_z = origin.z.floor() as i32;

    let delta_dist_x = if dir.x == 0.0 { f32::INFINITY } else { (1.0_f32 / dir.x).abs() };
    let delta_dist_y = if dir.y == 0.0 { f32::INFINITY } else { (1.0_f32 / dir.y).abs() };
    let delta_dist_z = if dir.z == 0.0 { f32::INFINITY } else { (1.0_f32 / dir.z).abs() };

    let step_x = if dir.x < 0.0 { -1 } else { 1 };
    let step_y = if dir.y < 0.0 { -1 } else { 1 };
    let step_z = if dir.z < 0.0 { -1 } else { 1 };

    let mut side_dist_x = if dir.x < 0.0 {
        (origin.x - map_x as f32) * delta_dist_x
    } else {
        (map_x as f32 + 1.0 - origin.x) * delta_dist_x
    };
    let mut side_dist_y = if dir.y < 0.0 {
        (origin.y - map_y as f32) * delta_dist_y
    } else {
        (map_y as f32 + 1.0 - origin.y) * delta_dist_y
    };
    let mut side_dist_z = if dir.z < 0.0 {
        (origin.z - map_z as f32) * delta_dist_z
    } else {
        (map_z as f32 + 1.0 - origin.z) * delta_dist_z
    };

    let mut hit = false;
    let mut side = 0; // 0 = x, 1 = y, 2 = z

    for _ in 0..50 {
        let dist = Vec3::new(map_x as f32 + 0.5, map_y as f32 + 0.5, map_z as f32 + 0.5).distance(origin);
        if dist > max_dist {
            break;
        }

        let block = world.get_block(map_x, map_y, map_z);
        if block != BlockType::Air {
            hit = true;
            break;
        }

        if side_dist_x < side_dist_y {
            if side_dist_x < side_dist_z {
                side_dist_x += delta_dist_x;
                map_x += step_x;
                side = 0;
            } else {
                side_dist_z += delta_dist_z;
                map_z += step_z;
                side = 2;
            }
        } else {
            if side_dist_y < side_dist_z {
                side_dist_y += delta_dist_y;
                map_y += step_y;
                side = 1;
            } else {
                side_dist_z += delta_dist_z;
                map_z += step_z;
                side = 2;
            }
        }
    }

    if !hit {
        return;
    }

    let mut normal = IVec3::ZERO;
    if side == 0 {
        normal.x = -step_x;
    } else if side == 1 {
        normal.y = -step_y;
    } else {
        normal.z = -step_z;
    }

    let block = world.get_block(map_x, map_y, map_z);
    target.0 = Some(TargetHit {
        pos: IVec3::new(map_x, map_y, map_z),
        normal,
        block,
    });

    // วาดกรอบหน้าที่เล็งอยู่
    let normal_f = normal.as_vec3();
    let mut face_idx = 0;
    for (i, n) in CUBE_NORMALS.iter().enumerate() {
        if Vec3::from_array(*n) == normal_f {
            face_idx = i;
            break;
        }
    }

    let positions = CUBE_POSITIONS[face_idx];
    let offset = normal_f * 0.01;
    let block_pos = Vec3::new(map_x as f32, map_y as f32, map_z as f32);

    let p0 = block_pos + Vec3::from_array(positions[0]) + offset;
    let p1 = block_pos + Vec3::from_array(positions[1]) + offset;
    let p2 = block_pos + Vec3::from_array(positions[2]) + offset;
    let p3 = block_pos + Vec3::from_array(positions[3]) + offset;

    let color = Color::BLACK;
    gizmos.line(p0, p1, color);
    gizmos.line(p1, p2, color);
    gizmos.line(p2, p3, color);
    gizmos.line(p3, p0, color);
}

pub fn block_interaction_system(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    target: Res<TargetedBlock>,
    mut selected: ResMut<SelectedBlock>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunk_material: Res<ChunkMaterial>,
    water_material: Res<WaterMaterial>,
    glass_material: Res<GlassMaterial>,
    deco_material: Res<DecoMaterials>,
    lamp_materials: Res<LampMaterials>,
    block_materials: Res<BlockMaterials>,
    mesh_query: Query<&Mesh3d>,
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut q_egui: Query<&mut bevy_egui::EguiContext, With<bevy::window::PrimaryWindow>>,
) {
    // Hotbar: กด 1-0, -, = และ T เลือกบล็อก
    const HOTBAR: [(KeyCode, BlockType); 13] = [
        (KeyCode::Digit1, BlockType::Dirt),
        (KeyCode::Digit2, BlockType::Grass),
        (KeyCode::Digit3, BlockType::Stone),
        (KeyCode::Digit4, BlockType::Wood),
        (KeyCode::Digit5, BlockType::Leaves),
        (KeyCode::Digit6, BlockType::Sand),
        (KeyCode::Digit7, BlockType::Water),
        (KeyCode::Digit8, BlockType::Glowstone),
        (KeyCode::Digit9, BlockType::LampRed),
        (KeyCode::Digit0, BlockType::LampGreen),
        (KeyCode::Minus, BlockType::LampBlue),
        (KeyCode::Equal, BlockType::Glass),
        (KeyCode::KeyT, BlockType::TallGrass),
    ];
    for (key, block) in HOTBAR {
        if keyboard.just_pressed(key) {
            selected.0 = block;
        }
    }

    let Some(hit) = target.0 else { return };

    let break_pressed = mouse_input.just_pressed(MouseButton::Left);
    let place_pressed = mouse_input.just_pressed(MouseButton::Right);
    if !break_pressed && !place_pressed {
        return;
    }

    // คลิกบน egui = ใช้เมนูอยู่ ไม่ใช่เล่นเกม
    if let Some(mut egui_ctx) = q_egui.iter_mut().next() {
        if egui_ctx.get_mut().egui_wants_pointer_input() || egui_ctx.get_mut().is_pointer_over_egui() {
            return;
        }
    }

    let mut target_pos = None;
    if break_pressed {
        if world.set_block(hit.pos.x, hit.pos.y, hit.pos.z, BlockType::Air) {
            target_pos = Some(hit.pos);
        }
    } else if place_pressed {
        let p = hit.pos + hit.normal;

        // กันวางบล็อกตันทับตำแหน่งผู้เล่น
        let mut blocked = false;
        if selected.0.is_solid() {
            if let Some(cam) = camera_query.iter().next() {
                let feet = cam.translation - Vec3::Y * crate::camera::EYE_HEIGHT;
                let pmin = feet - Vec3::new(crate::camera::PLAYER_HALF, 0.0, crate::camera::PLAYER_HALF);
                let pmax = feet + Vec3::new(crate::camera::PLAYER_HALF, crate::camera::PLAYER_HEIGHT, crate::camera::PLAYER_HALF);
                let bmin = p.as_vec3();
                let bmax = bmin + Vec3::ONE;
                blocked = pmin.x < bmax.x && pmax.x > bmin.x
                    && pmin.y < bmax.y && pmax.y > bmin.y
                    && pmin.z < bmax.z && pmax.z > bmin.z;
            }
        }

        if !blocked && world.set_block(p.x, p.y, p.z, selected.0) {
            target_pos = Some(p);
        }
    }

    let Some(tp) = target_pos else { return };

    // เซฟ chunk ที่แก้ลง disk ทันที
    let edited_chunk = IVec2::new(
        tp.x.div_euclid(CHUNK_WIDTH as i32),
        tp.z.div_euclid(CHUNK_WIDTH as i32),
    );
    if let Some(chunk) = world.chunks.get(&edited_chunk) {
        save_chunk(edited_chunk, &chunk.blocks);
    }

    // หา chunk ที่ต้อง remesh (ตัวเอง + เพื่อนบ้านถ้าแก้ตรงขอบ/มุม)
    let mut chunks_to_remesh = vec![edited_chunk];
    let local_x = tp.x.rem_euclid(CHUNK_WIDTH as i32);
    let local_z = tp.z.rem_euclid(CHUNK_WIDTH as i32);
    let (cx, cz) = (edited_chunk.x, edited_chunk.y);

    let at_min_x = local_x == 0;
    let at_max_x = local_x == (CHUNK_WIDTH - 1) as i32;
    let at_min_z = local_z == 0;
    let at_max_z = local_z == (CHUNK_WIDTH - 1) as i32;

    if at_min_x { chunks_to_remesh.push(IVec2::new(cx - 1, cz)); }
    if at_max_x { chunks_to_remesh.push(IVec2::new(cx + 1, cz)); }
    if at_min_z { chunks_to_remesh.push(IVec2::new(cx, cz - 1)); }
    if at_max_z { chunks_to_remesh.push(IVec2::new(cx, cz + 1)); }

    // มุม chunk: AO ของ chunk แนวทแยงก็โดนผลด้วย
    if at_min_x && at_min_z { chunks_to_remesh.push(IVec2::new(cx - 1, cz - 1)); }
    if at_min_x && at_max_z { chunks_to_remesh.push(IVec2::new(cx - 1, cz + 1)); }
    if at_max_x && at_min_z { chunks_to_remesh.push(IVec2::new(cx + 1, cz - 1)); }
    if at_max_x && at_max_z { chunks_to_remesh.push(IVec2::new(cx + 1, cz + 1)); }

    for chunk_pos in chunks_to_remesh {
        let neighbors_pos = chunk_neighbors(chunk_pos);
        if !neighbors_pos.iter().all(|p| world.chunks.contains_key(p)) {
            continue;
        }
        let neighbors = neighbors_pos.map(|p| world.chunks.get(&p).unwrap().blocks.clone());

        let transform = Transform::from_xyz(
            (chunk_pos.x * CHUNK_WIDTH as i32) as f32,
            0.0,
            (chunk_pos.y * CHUNK_WIDTH as i32) as f32,
        );

        let old_vertices;
        let old_indices;
        let set;
        {
            let Some(chunk_data) = world.chunks.get_mut(&chunk_pos) else { continue };
            old_vertices = chunk_data.num_vertices;
            old_indices = chunk_data.num_indices;

            let s = create_mesh_from_blocks(chunk_pos, &chunk_data.blocks, &neighbors);
            chunk_data.num_vertices = s.total_vertices();
            chunk_data.num_indices = s.total_indices();
            set = s;
        }

        world.total_vertices = (world.total_vertices + set.total_vertices()) - old_vertices;
        world.total_indices = (world.total_indices + set.total_indices()) - old_indices;
        let ChunkMeshSet { solid, water, glass, deco, glow, textured } = set;

        // สลับ mesh พื้นดิน: เขียนทับ asset เดิมผ่าน handle เดิมถ้าทำได้
        // (asset id คงที่ ไม่มี free/alloc ลดการกระตุ้นบั๊ก slab allocator)
        // และถอด Aabb ให้คำนวณใหม่ ไม่งั้นบล็อกที่วางสูงกว่ายอดเดิมโดน cull หาย
        if let Some(&entity) = world.generated_chunks.get(&chunk_pos) {
            if solid.is_empty() {
                commands.entity(entity).remove::<Mesh3d>().remove::<Aabb>();
            } else if let Ok(mesh3d) = mesh_query.get(entity) {
                let _ = meshes.insert(mesh3d.0.id(), solid.into_mesh());
                commands.entity(entity).remove::<Aabb>();
            } else {
                commands.entity(entity)
                    .insert((
                        Mesh3d(meshes.add(solid.into_mesh())),
                        MeshMaterial3d(chunk_material.0.clone()),
                    ))
                    .remove::<Aabb>();
            }
        }

        // น้ำ/กระจก/ของประดับ: สร้าง/เขียนทับ/ลบ ตามว่าเหลือหน้าไหม
        update_single_mesh_entity(&mut commands, &mut world.water_chunks, &mut meshes, &mesh_query, &water_material.0, chunk_pos, water, transform);
        update_single_mesh_entity(&mut commands, &mut world.glass_chunks, &mut meshes, &mesh_query, &glass_material.0, chunk_pos, glass, transform);
        update_deco_entities(&mut commands, &mut world, &mut meshes, &deco_material, chunk_pos, deco, transform);

        update_glow_entities(&mut commands, &mut world, &mut meshes, &lamp_materials, chunk_pos, glow, transform);
        update_textured_entities(&mut commands, &mut world, &mut meshes, &block_materials, chunk_pos, textured, transform);
    }

    // บล็อกเปลี่ยนเฉพาะใน chunk ที่แก้ — อัปเดต PointLight เฉพาะตรงนั้น
    refresh_chunk_lamp_lights(&mut commands, &mut world, edited_chunk);
}
