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
    Chiseled = 14,
    Water1 = 15,
    Water2 = 16,
    Water3 = 17,
    Water4 = 18,
    Water5 = 19,
    Water6 = 20,
    Water7 = 21,
    Water8 = 22,
    Tnt = 23,
    /// TNT ที่จุดชนวนแล้ว (นับถอยหลังใน ActiveTnt) — แยกชนิดเพื่อให้ sync
    /// ผ่าน SetBlock ธรรมดาได้ และ emission ทำให้ไฟ/ประกายติดผ่านระบบ lamp เดิม
    TntLit = 24,
    IronBlock = 25,
    Nuke = 26,
    /// Nuke ที่จุดชนวนแล้ว — แพทเทิร์นเดียวกับ TntLit (sync ผ่าน SetBlock + emission)
    NukeLit = 27,
    SwitchOff = 28,
    SmartLamp = 29,
    SmartLampOn = 30,
    SwitchOn = 31,
    Furnace = 32,
    Chest = 33,
    Campfire = 34,
    Branch = 35,
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
            14 => BlockType::Chiseled,
            15 => BlockType::Water1,
            16 => BlockType::Water2,
            17 => BlockType::Water3,
            18 => BlockType::Water4,
            19 => BlockType::Water5,
            20 => BlockType::Water6,
            21 => BlockType::Water7,
            22 => BlockType::Water8,
            23 => BlockType::Tnt,
            24 => BlockType::TntLit,
            25 => BlockType::IronBlock,
            26 => BlockType::Nuke,
            27 => BlockType::NukeLit,
            28 => BlockType::SwitchOff,
            29 => BlockType::SmartLamp,
            30 => BlockType::SmartLampOn,
            31 => BlockType::SwitchOn,
            32 => BlockType::Furnace,
            33 => BlockType::Chest,
            34 => BlockType::Campfire,
            35 => BlockType::Branch,
            _ => BlockType::Air,
        }
    }

    /// บล็อกที่ชนตัวผู้เล่นได้
    pub fn is_solid(self) -> bool {
        block_def(self).solid
    }

    pub fn is_water(self) -> bool {
        match self {
            BlockType::Water | BlockType::Water1 | BlockType::Water2 | BlockType::Water3 | BlockType::Water4 |
            BlockType::Water5 | BlockType::Water6 | BlockType::Water7 | BlockType::Water8 => true,
            _ => false,
        }
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
    /// ความแข็ง: พลังงานระเบิดที่ต้องจ่ายเพื่อทำลาย/ทะลุบล็อกนี้
    /// (น้ำ = ค่าดูดซับพลังงานต่อระดับ — น้ำไม่ถูกระเบิดทำลาย ปริมาตรต้อง conserve)
    pub hardness: f32,
    /// path ใต้ assets/ — ใส่ได้หลายลาย เกมจะสุ่มเลือกตามพิกัดบล็อก
    /// (deterministic) ให้ไม่ซ้ำกันเป็นแพทเทิร์น ไฟล์ไหนไม่มีจริงถูกข้าม
    pub tex_top: &'static [&'static str],
    pub tex_side: &'static [&'static str],
    pub tex_bottom: &'static [&'static str],
    /// sprite พู่ห้อยเอียงจากขอบบนของหน้าด้านข้าง (alpha cutout, สุ่มลายตามพิกัด)
    pub overlay_side: &'static [&'static str],
}

pub const BLOCK_DEFS: [BlockDef; 36] = [
    BlockDef { name: "Air", color: [1.0, 1.0, 1.0, 1.0], solid: false, transparent: true, emission: None, hardness: 0.0,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Dirt", color: [0.4, 0.2, 0.0, 1.0], solid: true, transparent: false, emission: None, hardness: 1.0,
        tex_top: &["textures/dirt.png"], tex_side: &["textures/dirt.png"], tex_bottom: &["textures/dirt.png"],
        overlay_side: &[] },
    BlockDef { name: "Grass", color: [0.2, 0.6, 0.2, 1.0], solid: true, transparent: false, emission: None, hardness: 1.2,
        tex_top: &["textures/grass_top.png"],
        tex_side: &["textures/grass_side.png"],
        tex_bottom: &["textures/dirt.png"],
        // พู่หญ้าห้อยเอียงจากขอบบน
        overlay_side: &["textures/grass_side_overlay.png"] },
    BlockDef { name: "Stone", color: [0.5, 0.5, 0.5, 1.0], solid: true, transparent: false, emission: None, hardness: 6.0,
        tex_top: &["textures/stone.png"], tex_side: &["textures/stone.png"], tex_bottom: &["textures/stone.png"],
        overlay_side: &[] },
    BlockDef { name: "Water", color: [0.25, 0.5, 0.85, 1.0], solid: false, transparent: true, emission: None, hardness: 3.2,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Wood", color: [0.4, 0.3, 0.2, 1.0], solid: true, transparent: false, emission: None, hardness: 3.0,
        tex_top: &["textures/wood_top.png"], tex_side: &["textures/wood_side.png"],
        tex_bottom: &["textures/wood_top.png"], overlay_side: &[] },
    // ใบไม้วาดเป็นแผ่น sprite ตัดกันแบบดาว 3 แกน (ดู generate_leaf_mesh_into) ไม่ใช่คิวบ์
    // — transparent:true เพื่อไม่ให้หน้าของบล็อกข้างเคียงถูก cull หายไปหลังพุ่มใบ
    // solid:true คงไว้ให้ยังเดินบนพุ่มได้เหมือนเดิม
    BlockDef { name: "Leaves", color: [0.1, 0.5, 0.1, 1.0], solid: true, transparent: true, emission: None, hardness: 0.3,
        tex_top: &["textures/leaves.png"], tex_side: &["textures/leaves.png"],
        tex_bottom: &["textures/leaves.png"], overlay_side: &[] },
    BlockDef { name: "Sand", color: [0.9, 0.8, 0.6, 1.0], solid: true, transparent: false, emission: None, hardness: 0.8,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Glowstone", color: [1.0, 0.9, 0.5, 1.0], solid: true, transparent: false, emission: Some([1.0, 0.9, 0.5]), hardness: 1.5,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "LampRed", color: [0.5, 0.1, 0.1, 1.0], solid: true, transparent: false, emission: Some([1.0, 0.2, 0.2]), hardness: 1.5,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "LampGreen", color: [0.1, 0.5, 0.1, 1.0], solid: true, transparent: false, emission: Some([0.2, 1.0, 0.2]), hardness: 1.5,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "LampBlue", color: [0.1, 0.1, 0.5, 1.0], solid: true, transparent: false, emission: Some([0.2, 0.2, 1.0]), hardness: 1.5,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Glass", color: [0.80, 0.90, 1.0, 1.0], solid: true, transparent: true, emission: None, hardness: 0.4,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Tall Grass", color: [0.25, 0.55, 0.53, 1.0], solid: false, transparent: true, emission: None, hardness: 0.05,
        // ใช้ช่อง side เป็นรูป sprite ของกากบาท
        tex_top: &[], tex_side: &["textures/grass.png"], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Chiseled", color: [1.0, 1.0, 1.0, 1.0], solid: false, transparent: true, emission: None, hardness: 1.0,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Water1", color: [0.25, 0.5, 0.85, 1.0], solid: false, transparent: true, emission: None, hardness: 0.4,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Water2", color: [0.25, 0.5, 0.85, 1.0], solid: false, transparent: true, emission: None, hardness: 0.8,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Water3", color: [0.25, 0.5, 0.85, 1.0], solid: false, transparent: true, emission: None, hardness: 1.2,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Water4", color: [0.25, 0.5, 0.85, 1.0], solid: false, transparent: true, emission: None, hardness: 1.6,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Water5", color: [0.25, 0.5, 0.85, 1.0], solid: false, transparent: true, emission: None, hardness: 2.0,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Water6", color: [0.25, 0.5, 0.85, 1.0], solid: false, transparent: true, emission: None, hardness: 2.4,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Water7", color: [0.25, 0.5, 0.85, 1.0], solid: false, transparent: true, emission: None, hardness: 2.8,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Water8", color: [0.25, 0.5, 0.85, 1.0], solid: false, transparent: true, emission: None, hardness: 3.2,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "TNT", color: [0.8, 0.2, 0.15, 1.0], solid: true, transparent: false, emission: None, hardness: 0.5,
        tex_top: &["textures/tnt.png"], tex_side: &["textures/tnt.png"], tex_bottom: &["textures/tnt.png"],
        overlay_side: &[] },
    // emission ทำให้ TntLit ได้ PointLight + ประกายไฟจากระบบ lamp/sparkle เดิมฟรี
    // hardness สูงมากกัน ray ระเบิดลูกอื่นทำลายก่อนถึงคิว chain
    BlockDef { name: "TNT (lit)", color: [1.0, 0.5, 0.3, 1.0], solid: true, transparent: false, emission: Some([1.5, 0.6, 0.2]), hardness: 99.0,
        tex_top: &["textures/tnt.png"], tex_side: &["textures/tnt.png"], tex_bottom: &["textures/tnt.png"],
        overlay_side: &[] },
    // ระเบิดทำลายไม่ได้ (ray สะท้อนอย่างเดียว) — วัสดุท่อปืนใหญ่ถาวร
    BlockDef { name: "Iron", color: [0.85, 0.85, 0.88, 1.0], solid: true, transparent: false, emission: None, hardness: 999.0,
        tex_top: &["textures/iron_block.png"], tex_side: &["textures/iron_block.png"], tex_bottom: &["textures/iron_block.png"],
        overlay_side: &[] },
    // hardness ต่ำโดยตั้งใจ (สมจริง): ระเบิดธรรมดาโดน = พังทิ้งเฉยๆ ไม่จุดนิวเคลียร์
    BlockDef { name: "Nuke", color: [0.75, 0.75, 0.3, 1.0], solid: true, transparent: false, emission: None, hardness: 2.0,
        tex_top: &["textures/nuke.png"], tex_side: &["textures/nuke.png"], tex_bottom: &["textures/nuke.png"],
        overlay_side: &[] },
    // จุดแล้ว: hardness สูงกันโดนคลื่นอื่นลบระหว่างรอ fuse, emission = ไฟเตือน
    BlockDef { name: "Nuke (armed)", color: [1.0, 0.7, 0.2, 1.0], solid: true, transparent: false, emission: Some([2.0, 0.9, 0.3]), hardness: 999.0,
        tex_top: &["textures/nuke.png"], tex_side: &["textures/nuke.png"], tex_bottom: &["textures/nuke.png"],
        overlay_side: &[] },
    BlockDef { name: "Switch (OFF)", color: [0.6, 0.6, 0.6, 1.0], solid: true, transparent: false, emission: None, hardness: 1.0,
        tex_top: &["textures/switch-off.png"], tex_side: &["textures/switch-off.png"], tex_bottom: &["textures/switch-off.png"], overlay_side: &[] },
    BlockDef { name: "SmartLamp (OFF)", color: [0.2, 0.2, 0.2, 1.0], solid: true, transparent: true, emission: None, hardness: 1.0,
        tex_top: &["textures/lamp-off.png"], tex_side: &["textures/lamp-off.png"], tex_bottom: &["textures/lamp-off.png"], overlay_side: &[] },
    BlockDef { name: "SmartLamp (ON)", color: [0.9, 0.9, 0.9, 1.0], solid: true, transparent: true, emission: Some([1.5, 1.5, 1.5]), hardness: 1.0,
        tex_top: &["textures/lamp-on.png"], tex_side: &["textures/lamp-on.png"], tex_bottom: &["textures/lamp-on.png"], overlay_side: &[] },
    BlockDef { name: "Switch (ON)", color: [0.3, 0.9, 0.3, 1.0], solid: true, transparent: false, emission: None, hardness: 1.0,
        tex_top: &["textures/switch-on.png"], tex_side: &["textures/switch-on.png"], tex_bottom: &["textures/switch-on.png"], overlay_side: &[] },
    // tex_side[0]=ด้านข้างธรรมดา, [1]=หน้า (facing_variant เลือกตาม facing ที่วางหันหาผู้เล่น)
    BlockDef { name: "Furnace", color: [0.4, 0.4, 0.4, 1.0], solid: true, transparent: false, emission: None, hardness: 3.5,
        tex_top: &["textures/furnace.png"], tex_side: &["textures/furnace.png", "textures/furnace_front.png"],
        tex_bottom: &["textures/furnace.png"], overlay_side: &[] },
    // tex_side[0]=ด้านข้าง, [1]=หน้า, [2]=หลัง (facing_variant เลือกตาม facing/facing^1)
    BlockDef { name: "Chest", color: [0.55, 0.35, 0.15, 1.0], solid: true, transparent: false, emission: None, hardness: 3.0,
        tex_top: &["textures/chest_top_bottom.png"],
        tex_side: &["textures/chest_side.png", "textures/chest_front.png", "textures/chest_back.png"],
        tex_bottom: &["textures/chest_top_bottom.png"], overlay_side: &[] },
    // ไม่มี texture แบนต่อหน้า — วาดด้วย glTF model จริง (assets/model/campfire.gltf) แทน
    // ทั้งคิวบ์ (ดู create_mesh_from_blocks ที่ข้าม Campfire ไปเหมือน TallGrass/Chiseled)
    // transparent:true กัน AO/หน้าเพื่อนบ้านถูกตัดทิ้งราวกับ Campfire เต็มช่อง (โมเดลไม่เต็มจริง)
    // solid:true ไว้คู่กับ block_collision_box (กล่องเล็กกว่าคิวบ์เต็ม ไม่ใช่ AABB เต็มช่อง)
    // emission ทำให้ได้ PointLight + particle ไฟฟรีผ่านระบบ lamp/sparkle เดิม (ดู refresh_chunk_lamp_lights)
    BlockDef { name: "Campfire", color: [0.35, 0.22, 0.12, 1.0], solid: true, transparent: true, emission: Some([1.4, 0.6, 0.15]), hardness: 0.4,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Branch", color: [0.4, 0.2, 0.0, 1.0], solid: true, transparent: true, emission: None, hardness: 2.0,
        tex_top: &["textures/wood_side.png"], tex_side: &["textures/wood_side.png"], tex_bottom: &["textures/wood_side.png"], overlay_side: &[] },
];

pub fn block_def(block: BlockType) -> &'static BlockDef {
    &BLOCK_DEFS[block as usize]
}

pub fn block_name(block: BlockType) -> &'static str {
    block_def(block).name
}

/// ตัดทุกอย่างที่ไม่ใช่ตัวอักษร/ตัวเลขออกแล้วเป็นตัวพิมพ์เล็ก —
/// ทำให้ "Tall Grass", "tall_grass", "TallGrass" กลายเป็นคีย์เดียวกัน
fn name_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// หา BlockType จากชื่อที่ผู้ใช้พิมพ์ (สำหรับ /give, /setblock)
/// รับได้ทั้งชื่อ variant (`IronBlock`, `iron_block`) และชื่อที่โชว์ใน UI (`Iron`, "Tall Grass")
pub fn block_from_name(input: &str) -> Option<BlockType> {
    let key = name_key(input);
    if key.is_empty() {
        return None;
    }
    (0..BLOCK_DEFS.len() as u8).map(BlockType::from_u8).find(|&bt| {
        // Debug ให้ชื่อ variant ตรงๆ (IronBlock) ส่วน BLOCK_DEFS ให้ชื่อโชว์ (Iron)
        name_key(&format!("{bt:?}")) == key || name_key(block_name(bt)) == key
    })
}

pub fn block_color(block: BlockType) -> [f32; 4] {
    block_def(block).color
}

pub fn block_hardness(block: BlockType) -> f32 {
    block_def(block).hardness
}

pub fn lamp_emission(block: BlockType) -> Option<Color> {
    block_def(block).emission.map(|c| Color::srgb(c[0], c[1], c[2]))
}

/// กล่อง collision จริงของบล็อก (มุมล่าง, มุมบน ภายในช่อง 1x1x1 ของตัวเอง) — ค่าเริ่มต้นคือ
/// คิวบ์เต็มช่องเดิมสำหรับบล็อกทุกชนิด ยกเว้นบล็อกที่ไม่ใช่คิวบ์เต็ม (เช่น Campfire) ที่ระบุ
/// กล่องเล็กกว่าจริงไว้เฉพาะที่นี่ — ไม่ต้องเพิ่ม field ใน BlockDef/BLOCK_DEFS ทั้งตาราง
pub fn block_collision_box(block: BlockType) -> (Vec3, Vec3) {
    match block {
        BlockType::Campfire => (Vec3::new(0.15, 0.0, 0.15), Vec3::new(0.85, 0.4, 0.85)),
        BlockType::Branch => (Vec3::new(0.25, 0.0, 0.25), Vec3::new(0.75, 1.0, 0.75)),
        _ => (Vec3::ZERO, Vec3::ONE),
    }
}

/// เหมือน block_collision_box แต่รู้ตำแหน่งด้วย — ใช้กับบล็อกที่รูปทรงขึ้นกับเพื่อนบ้าน
/// ตอนนี้คือ Branch: กล่องต้องวางตามทิศที่กิ่งเชื่อมจริง ไม่ใช่เสาตั้งตายตัว
/// (กิ่งแนวนอนจะได้ชนตรงกับที่ตาเห็น)
pub fn block_collision_box_at(world: &VoxelWorld, pos: IVec3, block: BlockType) -> (Vec3, Vec3) {
    if block != BlockType::Branch {
        return block_collision_box(block);
    }
    let Some(node) = world.branch_network.nodes.get(&pos) else {
        return block_collision_box(block);
    };

    // แกนกลางกว้างตาม thickness (ขั้นต่ำ 0.15 กันบางจนเดินทะลุ)
    let r = (node.thickness as f32 / 32.0).max(0.15);
    let mut min = Vec3::splat(0.5 - r);
    let mut max = Vec3::splat(0.5 + r);

    // ทุกด้านที่มีกิ่งต่อ ยืดกล่องออกไปจนสุดขอบช่อง
    let mut stretch = |d: IVec3| {
        if d.x > 0 { max.x = 1.0; } else if d.x < 0 { min.x = 0.0; }
        if d.y > 0 { max.y = 1.0; } else if d.y < 0 { min.y = 0.0; }
        if d.z > 0 { max.z = 1.0; } else if d.z < 0 { min.z = 0.0; }
    };
    if let Some(pp) = node.parent_pos {
        stretch(pp - pos);
    } else {
        stretch(IVec3::NEG_Y); // root: โคนหยั่งลงพื้นเหมือนที่ mesh วาด
    }
    for &cp in &node.children {
        stretch(cp - pos);
    }
    (min, max)
}

// --------------------------------------------------------
// ตารางการขุด (ระบบทุบบล็อก Survival) — แพทเทิร์นเดียวกับ block_collision_box:
// match function แยก ไม่เพิ่ม field ใน BLOCK_DEFS (field `hardness` เดิมคือความทน
// "ระเบิด" คนละความหมาย — Iron 999 = ระเบิดไม่พังแต่ขุดด้วย pickaxe ได้)
// --------------------------------------------------------

/// บล็อกนี้อยู่หมวดเครื่องมือไหน (ขุดด้วย tool หมวดตรงกัน = เร็วขึ้น dig_speed เท่า)
pub fn block_dig_class(block: BlockType) -> crate::item::DigClass {
    use crate::item::DigClass;
    match block {
        BlockType::Stone | BlockType::IronBlock | BlockType::Furnace
        | BlockType::Glowstone | BlockType::LampRed | BlockType::LampGreen | BlockType::LampBlue
        | BlockType::SmartLamp | BlockType::SmartLampOn
        | BlockType::SwitchOff | BlockType::SwitchOn => DigClass::Pick,
        BlockType::Wood | BlockType::Chest | BlockType::Tnt | BlockType::Nuke
        | BlockType::Campfire | BlockType::Branch => DigClass::Axe,
        BlockType::Dirt | BlockType::Grass | BlockType::Sand => DigClass::Shovel,
        _ => DigClass::None,
    }
}

/// เวลาขุดด้วยมือเปล่า (วินาที) — ปรับสมดุลเกมที่ตารางนี้ที่เดียว
pub fn block_dig_time(block: BlockType) -> f32 {
    match block {
        BlockType::TallGrass | BlockType::Campfire => 0.2,
        BlockType::Leaves => 0.35,
        BlockType::Glass => 0.5,
        BlockType::Sand => 0.75,
        BlockType::Dirt | BlockType::Tnt | BlockType::Nuke => 1.0,
        BlockType::Grass => 1.2,
        BlockType::Glowstone | BlockType::LampRed | BlockType::LampGreen | BlockType::LampBlue
        | BlockType::SmartLamp | BlockType::SmartLampOn
        | BlockType::SwitchOff | BlockType::SwitchOn => 1.5,
        BlockType::Wood | BlockType::Chest | BlockType::Branch => 3.0,
        BlockType::Furnace => 3.5,
        BlockType::Stone => 5.0,
        BlockType::IronBlock => 7.5,
        _ => 1.0,
    }
}

/// กติกา drop แบบ Minecraft: หมวด Pick (หิน/แร่) ต้องถือ pickaxe ตอนแตกถึงได้ของ
/// มือเปล่า/tool ผิดหมวดขุดได้ (ช้า) แต่บล็อกหายเปล่า — หมวดอื่นได้ของเสมอ
pub fn block_requires_tool(block: BlockType) -> bool {
    block_dig_class(block) == crate::item::DigClass::Pick
}

/// เวลาขุดจริงตามของที่ถืออยู่ (tool หมวดตรง = หาร dig_speed)
pub fn break_time(block: BlockType, held: Option<crate::item::ToolType>) -> f32 {
    let base = block_dig_time(block);
    match held {
        Some(tool) if tool.dig_class() == block_dig_class(block) => base / tool.dig_speed(),
        _ => base,
    }
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

/// เลือก texture variant ของ Furnace/Chest ตาม facing (หน้าหันหาผู้เล่นตอนวาง) แทน texture_variant
/// face_id ที่ใช้จริง (จาก FACE_OFFSETS) มีแค่ 2/3/4/5 เป็นด้านข้าง — บน/ล่าง (0/1) ใช้ variant 0 เสมอ
/// facing เก็บเป็น face_id ของหน้า "หน้า" ตรงๆ (2/3/4/5); หน้าตรงข้ามคือ facing ^ 1
pub fn facing_variant(block: BlockType, face_id: usize, facing: u8) -> u8 {
    if face_id < 2 {
        return 0;
    }
    let face_id = face_id as u8;
    match block {
        BlockType::Furnace => if face_id == facing { 1 } else { 0 },
        BlockType::Chest => {
            if face_id == facing { 1 } else if face_id == (facing ^ 1) { 2 } else { 0 }
        }
        _ => 0,
    }
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
/// 3072 = เผื่อภูมิประเทศจริง 1 บล็อก = 1 ม. (ดอยอินทนนท์ 2,565 ม. + ทะเล + ฟ้า)
/// — section storage ทำให้คอลัมน์สูงแบกได้ (ฟ้า/หินตันเก็บ 1 byte ต่อ 16 ชั้น)
pub const CHUNK_HEIGHT: usize = 3072;
pub const CHUNK_VOLUME: usize = CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH;
pub const SEA_LEVEL: usize = 200;

// --------------------------------------------------------
// Section storage — คอลัมน์ซอยเป็นชั้นละ 16 (แนว Minecraft):
// ชั้นที่เป็นชนิดเดียวล้วน (ฟ้าโล่ง/หินตัน) เก็บ 1 byte แทน 4KB
// โลกยัง key ด้วย IVec2 เหมือนเดิม — ไม่ใช่ 3D chunks
// --------------------------------------------------------

pub const SECTION_H: usize = 16;
pub const SECTION_VOLUME: usize = CHUNK_WIDTH * SECTION_H * CHUNK_WIDTH;
pub const SECTIONS_PER_CHUNK: usize = CHUNK_HEIGHT / SECTION_H;

#[derive(Clone)]
pub enum Section {
    /// ทั้ง 16×16×16 เป็นชนิดเดียว
    Uniform(BlockType),
    Dense(Box<[BlockType; SECTION_VOLUME]>),
}

impl Section {
    /// layout ภายใน section: x + y_local*W + z*W*SECTION_H
    #[inline]
    fn idx(x: usize, y_local: usize, z: usize) -> usize {
        x + y_local * CHUNK_WIDTH + z * CHUNK_WIDTH * SECTION_H
    }
}

#[derive(Clone)]
pub struct ChunkBlocks {
    sections: Vec<Section>,
}

impl ChunkBlocks {
    pub fn new_uniform(block: BlockType) -> Self {
        Self { sections: vec![Section::Uniform(block); SECTIONS_PER_CHUNK] }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> BlockType {
        match &self.sections[y / SECTION_H] {
            Section::Uniform(b) => *b,
            Section::Dense(a) => a[Section::idx(x, y % SECTION_H, z)],
        }
    }

    /// อ่านด้วย flat index แบบเดิม (x + y*W + z*W*H) — สำหรับโค้ดที่ยังคิดเป็น index
    #[inline]
    pub fn get_idx(&self, i: usize) -> BlockType {
        let x = i % CHUNK_WIDTH;
        let y = (i / CHUNK_WIDTH) % CHUNK_HEIGHT;
        let z = i / (CHUNK_WIDTH * CHUNK_HEIGHT);
        self.get(x, y, z)
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, block: BlockType) {
        let si = y / SECTION_H;
        let section = &mut self.sections[si];
        if let Section::Uniform(b) = section {
            if *b == block {
                return; // เขียนค่าเดิมลง uniform — ไม่ต้อง materialize
            }
            *section = Section::Dense(Box::new([*b; SECTION_VOLUME]));
        }
        if let Section::Dense(a) = section {
            a[Section::idx(x, y % SECTION_H, z)] = block;
        }
    }

    /// ยุบ Dense ที่กลายเป็นชนิดเดียวล้วนกลับเป็น Uniform (เรียกตอนเซฟ/หลัง gen)
    pub fn compact(&mut self) {
        for section in &mut self.sections {
            if let Section::Dense(a) = section {
                let first = a[0];
                if a.iter().all(|b| *b == first) {
                    *section = Section::Uniform(first);
                }
            }
        }
    }

    /// ไล่ทุกบล็อกตามลำดับ flat index เดิม (x เร็วสุด, แล้ว y, แล้ว z)
    /// — ให้ RLE network / โค้ด enumerate เดิมใช้แทน blocks.iter()
    pub fn iter_all(&self) -> impl Iterator<Item = BlockType> + '_ {
        (0..CHUNK_VOLUME).map(move |i| self.get_idx(i))
    }

    /// ช่วง section ที่ "อาจมีของ" (ไม่ใช่ Uniform(Air)) เป็นช่วง y inclusive —
    /// ให้ mesher/สแกนต่างๆ ข้ามฟ้าโล่งทั้งแถบ; None = ทั้งคอลัมน์เป็นอากาศ
    pub fn y_bounds_non_air(&self) -> Option<(usize, usize)> {
        let first = self.sections.iter().position(|s| !matches!(s, Section::Uniform(BlockType::Air)))?;
        let last = self.sections.iter().rposition(|s| !matches!(s, Section::Uniform(BlockType::Air)))?;
        Some((first * SECTION_H, last * SECTION_H + SECTION_H - 1))
    }

    /// section ตรง y นี้เป็น Uniform(Air) ไหม — fast path ให้ลูปสแกนกระโดดข้าม
    #[inline]
    pub fn section_is_air(&self, y: usize) -> bool {
        matches!(self.sections[y / SECTION_H], Section::Uniform(BlockType::Air))
    }

    /// เข้าถึง section ตรงๆ สำหรับลูปสแกนที่อยากได้ fast path ต่อ section
    pub fn sections_ref(&self) -> &[Section] {
        &self.sections
    }

    /// เรียก f(x, y, z, block) เฉพาะบล็อกที่ filter ผ่าน — section Uniform ที่
    /// filter ไม่ผ่านถูกข้ามทั้งก้อน 4096 cell (หัวใจของสแกนคอลัมน์สูงให้ยังถูก)
    pub fn for_each_matching(
        &self,
        filter: impl Fn(BlockType) -> bool,
        mut f: impl FnMut(usize, usize, usize, BlockType),
    ) {
        for (si, section) in self.sections.iter().enumerate() {
            match section {
                Section::Uniform(b) => {
                    if filter(*b) {
                        for z in 0..CHUNK_WIDTH {
                            for yl in 0..SECTION_H {
                                for x in 0..CHUNK_WIDTH {
                                    f(x, si * SECTION_H + yl, z, *b);
                                }
                            }
                        }
                    }
                }
                Section::Dense(a) => {
                    for z in 0..CHUNK_WIDTH {
                        for yl in 0..SECTION_H {
                            for x in 0..CHUNK_WIDTH {
                                let b = a[Section::idx(x, yl, z)];
                                if filter(b) {
                                    f(x, si * SECTION_H + yl, z, b);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// สร้างจาก byte ต่อบล็อกตามลำดับ flat เดิม (เส้นทาง network/แปลงของเก่า)
    pub fn from_dense_bytes(bytes: &[u8]) -> Self {
        let mut cb = Self::new_uniform(BlockType::Air);
        for (i, b) in bytes.iter().enumerate().take(CHUNK_VOLUME) {
            let block = BlockType::from_u8(*b);
            if block != BlockType::Air {
                let x = i % CHUNK_WIDTH;
                let y = (i / CHUNK_WIDTH) % CHUNK_HEIGHT;
                let z = i / (CHUNK_WIDTH * CHUNK_HEIGHT);
                cb.set(x, y, z, block);
            }
        }
        cb.compact();
        cb
    }

    // ---- save format v2: [b"CHK2"] แล้วต่อด้วย 192 sections:
    // tag 0 = Uniform ตามด้วย id 1 byte / tag 1 = Dense ตามด้วย 4096 bytes ----

    pub fn to_save_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + self.sections.len() * 8);
        out.extend_from_slice(b"CHK2");
        for section in &self.sections {
            match section {
                Section::Uniform(b) => {
                    out.push(0);
                    out.push(*b as u8);
                }
                Section::Dense(a) => {
                    out.push(1);
                    out.extend(a.iter().map(|b| *b as u8));
                }
            }
        }
        out
    }

    pub fn from_save_bytes(bytes: &[u8]) -> Option<Self> {
        let rest = bytes.strip_prefix(b"CHK2")?;
        let mut sections = Vec::with_capacity(SECTIONS_PER_CHUNK);
        let mut i = 0usize;
        for _ in 0..SECTIONS_PER_CHUNK {
            match rest.get(i)? {
                0 => {
                    sections.push(Section::Uniform(BlockType::from_u8(*rest.get(i + 1)?)));
                    i += 2;
                }
                1 => {
                    let data = rest.get(i + 1..i + 1 + SECTION_VOLUME)?;
                    let mut a = Box::new([BlockType::Air; SECTION_VOLUME]);
                    for (j, b) in data.iter().enumerate() {
                        a[j] = BlockType::from_u8(*b);
                    }
                    sections.push(Section::Dense(a));
                    i += 1 + SECTION_VOLUME;
                }
                _ => return None,
            }
        }
        Some(Self { sections })
    }
}

pub struct ChunkData {
    pub blocks: Arc<ChunkBlocks>,
    pub chiseled_blocks: HashMap<usize, Box<[u8; 4096]>>,
    /// หน้า "หน้า" ของ Furnace/Chest ต่อตำแหน่ง (เก็บเป็น face_id 2/3/4/5) — เหมือน chiseled_blocks
    /// ต่างกันที่อันนี้ต้องเซฟลง disk จริง (ดู save_chunk_full/load_chunk_aux)
    pub facings: HashMap<usize, u8>,
    /// ของในกล่อง Chest ต่อตำแหน่ง (27 ช่อง) — เซฟลง disk เหมือน facings
    pub chest_slots: HashMap<usize, Box<[Option<ItemStack>; 27]>>,
    /// ของในกล่อง Furnace ต่อตำแหน่ง (3 ช่อง: input/fuel/output — ยังไม่มี logic เผา)
    pub furnace_slots: HashMap<usize, Box<[Option<ItemStack>; 3]>>,
    pub num_vertices: usize,
    pub num_indices: usize,
    /// ช่วง y ที่มีน้ำ (inclusive) — grow-only ตอน set_block เขียนน้ำ,
    /// tighten ตอน rebuild mesh น้ำ; สถานะ "ไม่มีน้ำ" = min > max
    pub water_y_min: usize,
    pub water_y_max: usize,
    /// ส่วนแบ่งของ mesh น้ำใน num_vertices/num_indices —
    /// ให้เส้นทาง remesh เฉพาะน้ำอัปเดตยอดรวมแบบ delta ได้โดยไม่พัง
    pub num_water_vertices: usize,
    pub num_water_indices: usize,
    /// มีบล็อกถูกเขียนหลังโหลด — การขุด/วางเซฟทันทีอยู่แล้ว แต่ผลจาก fluid sim
    /// กับ TNT ที่ยังไหลอยู่ไม่เซฟรายเฟรม flag นี้ให้ตอนออกจากโลกเซฟเก็บให้ครบ
    pub dirty: bool,
    /// sky light ต่อบล็อก — คำนวณจากบล็อกล้วน ไม่เซฟลงดิสก์/ไม่ส่งข้าม network
    /// Arc เพราะ mesh task ต้องใช้ของ chunk นี้ + เพื่อนบ้านอีก 8 ตัว การ clone ต้องฟรี
    pub light: Arc<crate::light::ChunkLight>,
    /// ต้องคำนวณ light ใหม่ก่อน mesh รอบหน้า (บล็อกเปลี่ยน/เพิ่งโหลด)
    pub light_dirty: bool,
    /// bitmask ของเพื่อนบ้าน (ลำดับตาม chunk_neighbors) ที่ "ยังไม่โหลด" ตอนคำนวณแสงครั้งล่าสุด
    /// — ตอนนั้นถือว่าเป็นฟ้าโล่ง ค่าตรงขอบจึงเพี้ยน พอตัวจริงมาถึงค่อยปลุกคิดใหม่
    /// เฉพาะ chunk ที่รอตัวนั้นอยู่จริง (เดิมปลุกเพื่อนบ้านทั้ง 8 ทุกครั้งที่มี chunk ใหม่
    /// ซึ่งลาม remesh เป็น 9 chunk ต่อครั้ง = เฟรมตกและภาพกระพริบ)
    pub light_missing_neighbors: u8,
}

impl ChunkData {
    pub fn get_index(x: usize, y: usize, z: usize) -> usize {
        x + y * CHUNK_WIDTH + z * CHUNK_WIDTH * CHUNK_HEIGHT
    }
}

/// สแกนหาแถบ y ที่มีน้ำทั้ง chunk (ใช้ครั้งเดียวตอน insert) — ข้าม section
/// ที่เป็น Uniform ชนิดไม่ใช่น้ำได้ทั้งแถบ
pub fn scan_water_bounds(blocks: &ChunkBlocks) -> (usize, usize) {
    let mut min_y = CHUNK_HEIGHT;
    let mut max_y = 0usize;
    for (si, section) in blocks.sections_ref().iter().enumerate() {
        match section {
            Section::Uniform(b) => {
                if b.is_water() {
                    min_y = min_y.min(si * SECTION_H);
                    max_y = max_y.max(si * SECTION_H + SECTION_H - 1);
                }
            }
            Section::Dense(a) => {
                for y_local in 0..SECTION_H {
                    let y = si * SECTION_H + y_local;
                    'row: for z in 0..CHUNK_WIDTH {
                        for x in 0..CHUNK_WIDTH {
                            if a[Section::idx(x, y_local, z)].is_water() {
                                min_y = min_y.min(y);
                                max_y = max_y.max(y);
                                break 'row;
                            }
                        }
                    }
                }
            }
        }
    }
    (min_y, max_y)
}

#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum InteractionMode {
    #[default]
    Normal,
    SubVoxel,
    Wiring,
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
    pub campfire_models: HashMap<IVec2, Vec<Entity>>, // glTF scene entity ของ Campfire ใน chunk
    pub branch_network: crate::tree::BranchNetwork,
    pub pending_branch_remesh: std::collections::HashSet<IVec2>,
    /// กิ่งที่ parent หายไปแล้ว รอ host ทุบตามเป็นทอดๆ (ดู block_update_system)
    pub pending_branch_orphans: std::collections::HashSet<IVec3>,
    /// ใบที่อยู่ข้างกิ่งซึ่งเพิ่งหายไป รอเช็คว่ายังมีกิ่งค้ำอยู่ไหม (ดู leaf decay)
    /// เติมเฉพาะตอนกิ่งถูกทำลายจริง — ใบที่ผู้เล่นเอาไปสร้างบ้านไกลๆ จึงไม่ร่วงเอง
    pub pending_leaf_decay: std::collections::HashSet<IVec3>,
    /// chunk ที่ cascade กิ่งไปแก้บล็อกไว้ ต้องเขียนลงดิสก์ (ดู branch_remesh_system)
    pub pending_branch_save: std::collections::HashSet<IVec2>,
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
            chunk.blocks.get(local_x, local_y, local_z)
        } else {
            BlockType::Air
        }
    }

    pub fn get_chiseled_sub_voxel(&self, x: i32, y: i32, z: i32, sx: usize, sy: usize, sz: usize) -> u8 {
        if y < 0 || y >= CHUNK_HEIGHT as i32 { return 0; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        if let Some(chunk) = self.chunks.get(&IVec2::new(cx, cz)) {
            let idx = ChunkData::get_index(lx, y as usize, lz);
            if let Some(data) = chunk.chiseled_blocks.get(&idx) {
                return data[sx + sy * 16 + sz * 256];
            }
        }
        0
    }
    
    pub fn set_chiseled_sub_voxel(&mut self, x: i32, y: i32, z: i32, sx: usize, sy: usize, sz: usize, val: u8) {
        if y < 0 || y >= CHUNK_HEIGHT as i32 { return; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        if let Some(chunk) = self.chunks.get_mut(&IVec2::new(cx, cz)) {
            let idx = ChunkData::get_index(lx, y as usize, lz);
            let entry = chunk.chiseled_blocks.entry(idx).or_insert_with(|| Box::new([0; 4096]));
            entry[sx + sy * 16 + sz * 256] = val;
        }
    }

    pub fn convert_to_chiseled(&mut self, x: i32, y: i32, z: i32) {
        let block = self.get_block(x, y, z);
        if block == BlockType::Air || block == BlockType::Chiseled { return; }

        if y < 0 || y >= CHUNK_HEIGHT as i32 { return; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        if let Some(chunk) = self.chunks.get_mut(&IVec2::new(cx, cz)) {
            let idx = ChunkData::get_index(lx, y as usize, lz);
            Arc::make_mut(&mut chunk.blocks).set(lx, y as usize, lz, BlockType::Chiseled);
            let mut data = Box::new([0u8; 4096]);
            data.fill(block as u8);
            chunk.chiseled_blocks.insert(idx, data);
        }
        // Furnace/Chest ที่ถูกสกัดกลายเป็น Chiseled — facing/ของใน container เดิมไม่มีความหมายแล้ว
        self.clear_container_and_facing(x, y, z);
    }

    /// หน้า "หน้า" ของ Furnace/Chest ที่ตำแหน่งนี้ (face_id 2/3/4/5) — None ถ้าไม่มีข้อมูล
    /// คู่ getter ของ set_block_facing — ยังไม่มีจุดเรียกใช้ (meshing อ่าน chunk.facings ตรงๆ)
    /// เก็บไว้เผื่อ debug/F3 หรือ smelting logic ในอนาคตต้องรู้ facing
    #[allow(dead_code)]
    pub fn get_block_facing(&self, x: i32, y: i32, z: i32) -> Option<u8> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 { return None; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        self.chunks.get(&IVec2::new(cx, cz)).and_then(|chunk| {
            chunk.facings.get(&ChunkData::get_index(lx, y as usize, lz)).copied()
        })
    }

    pub fn set_block_facing(&mut self, x: i32, y: i32, z: i32, facing: u8) {
        if y < 0 || y >= CHUNK_HEIGHT as i32 { return; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        if let Some(chunk) = self.chunks.get_mut(&IVec2::new(cx, cz)) {
            let idx = ChunkData::get_index(lx, y as usize, lz);
            chunk.facings.insert(idx, facing);
        }
    }

    pub fn get_chest_slots(&self, x: i32, y: i32, z: i32) -> Option<&[Option<ItemStack>; 27]> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 { return None; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        self.chunks.get(&IVec2::new(cx, cz)).and_then(|chunk| {
            chunk.chest_slots.get(&ChunkData::get_index(lx, y as usize, lz)).map(|b| b.as_ref())
        })
    }

    pub fn get_furnace_slots(&self, x: i32, y: i32, z: i32) -> Option<&[Option<ItemStack>; 3]> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 { return None; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        self.chunks.get(&IVec2::new(cx, cz)).and_then(|chunk| {
            chunk.furnace_slots.get(&ChunkData::get_index(lx, y as usize, lz)).map(|b| b.as_ref())
        })
    }

    pub fn set_chest_slot(&mut self, x: i32, y: i32, z: i32, slot: usize, item: Option<ItemStack>) {
        if y < 0 || y >= CHUNK_HEIGHT as i32 || slot >= 27 { return; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        if let Some(chunk) = self.chunks.get_mut(&IVec2::new(cx, cz)) {
            let idx = ChunkData::get_index(lx, y as usize, lz);
            chunk.chest_slots.entry(idx).or_insert_with(|| Box::new([None; 27]))[slot] = item;
        }
    }

    pub fn set_furnace_slot(&mut self, x: i32, y: i32, z: i32, slot: usize, item: Option<ItemStack>) {
        if y < 0 || y >= CHUNK_HEIGHT as i32 || slot >= 3 { return; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        if let Some(chunk) = self.chunks.get_mut(&IVec2::new(cx, cz)) {
            let idx = ChunkData::get_index(lx, y as usize, lz);
            chunk.furnace_slots.entry(idx).or_insert_with(|| Box::new([None; 3]))[slot] = item;
        }
    }

    /// ล้าง facing + ของใน container ค้าง (เรียกก่อนเขียนทับ Furnace/Chest ด้วยบล็อกอื่น
    /// กัน entry ค้างใน map — ของใน container ที่ถูกทุบให้ break-drop ดึงออกไปเก็บ/ทิ้งก่อนเรียกฟังก์ชันนี้)
    pub fn clear_container_and_facing(&mut self, x: i32, y: i32, z: i32) {
        if y < 0 || y >= CHUNK_HEIGHT as i32 { return; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        if let Some(chunk) = self.chunks.get_mut(&IVec2::new(cx, cz)) {
            let idx = ChunkData::get_index(lx, y as usize, lz);
            chunk.facings.remove(&idx);
            chunk.chest_slots.remove(&idx);
            chunk.furnace_slots.remove(&idx);
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

            // make_mut ตอนนี้ clone แค่ Vec<Section> + section เดียวที่โดนเขียน (~4KB)
            // — เดิม clone ทั้งคอลัมน์ 128KB ต่อ write แรกหลัง share ให้ mesh task
            Arc::make_mut(&mut chunk.blocks).set(local_x, local_y, local_z, block_type);
            chunk.dirty = true;
            // ขยายแถบน้ำแบบ grow-only (tighten ทีเดียวตอน rebuild mesh น้ำ)
            if block_type.is_water() {
                chunk.water_y_min = chunk.water_y_min.min(local_y);
                chunk.water_y_max = chunk.water_y_max.max(local_y);
            }
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

/// ความสว่างตามระดับ sky light 0-15 — ไม่เชิงเส้นแบบ Minecraft (ระดับต่ำมืดเร็ว)
/// มีพื้น 0.05 ไม่ให้ดำสนิทจนมองไม่เห็นรูปทรงในถ้ำเลย
pub fn sky_curve(level: u8) -> f32 {
    let t = level.min(crate::light::MAX_LIGHT) as f32 / crate::light::MAX_LIGHT as f32;
    0.05 + 0.95 * t.powf(1.6)
}

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
/// ความมืดสูงสุดของสีน้ำลึก (0 = ไม่ไล่สี, 0.18 = ลึกสุดเหลือ 82% ความสว่าง)
/// ต่ำ = น้ำลึกไม่ทึบดำ มองทะลุเห็นพื้นได้
const WATER_DEPTH_DARKEN: f32 = 0.18;
/// ผิวน้ำเต็ม (source) ต่ำกว่าขอบบล็อกเล็กน้อยแบบ Minecraft — เฉพาะผิวบนสุดที่มีอากาศ
/// อยู่เหนือ (น้ำที่มีน้ำทับด้านบนยังเต็มความสูง เพราะ return ก่อนถึงตรงนี้)
const WATER_SURFACE_DROP: f32 = 0.1;
/// ความโปร่งของน้ำ อยู่ที่ **vertex alpha** ไม่ใช่ base_color ของ material —
/// material เป็น unlit + mesh มี vertex color ดังนั้น bevy ใช้ alpha จาก vertex
/// (ลด base_color.alpha ไม่มีผล) 0 = ใสสุด, 1 = ทึบตัน
const WATER_ALPHA: f32 = 0.4;
/// จำนวนชั้นน้ำที่นับว่า "ลึกสุด" สำหรับการไล่สี
const WATER_DEPTH_RANGE: i32 = 8;

/// ข้อมูลต่อมุมผิวน้ำ: (ระยะกดผิวลง, ความลึก normalize 0..1)
/// ใช้ร่วมกันทั้ง mesher เต็มและ create_water_mesh — logic มุมน้ำมีที่เดียว
/// - กดผิว: เฉลี่ยระดับน้ำจาก column 2x2 รอบมุม; column ที่เป็น "อากาศ" ร่วม
///   เฉลี่ยด้วยค่าจมสุด → ผิวลาดลงจูบพื้นตรงตลิ่ง/ขอบผา; solid ไม่ร่วมเฉลี่ย
///   → น้ำชนกำแพง/เขื่อนคงระดับ ไม่บุ๋ม
/// - ความลึก: เฉลี่ยจำนวนชั้นน้ำต่อเนื่องใต้มุม (ไว้ไล่สีน้ำลึกให้เข้ม)
fn water_corner_info(
    sample: &impl Fn(i32, i32, i32) -> BlockType,
    cx: i32,
    vy: i32,
    cz: i32,
) -> (f32, f32) {
    // มีน้ำชั้นบนติดมุม = จมใต้ผิว: เต็มความสูง + มืดสุด
    for dx in -1..=0 {
        for dz in -1..=0 {
            if sample(cx + dx, vy + 1, cz + dz).is_water() {
                return (0.0, 1.0);
            }
        }
    }
    let mut drop_sum = 0.0;
    let mut depth_sum = 0.0;
    let mut cnt = 0;
    for dx in -1..=0 {
        for dz in -1..=0 {
            let b = sample(cx + dx, vy, cz + dz);
            if b.is_water() {
                drop_sum += match b {
                    BlockType::Water7 => 0.125,
                    BlockType::Water6 => 0.25,
                    BlockType::Water5 => 0.375,
                    BlockType::Water4 => 0.50,
                    BlockType::Water3 => 0.625,
                    BlockType::Water2 => 0.75,
                    BlockType::Water1 => 0.875,
                    // น้ำเต็ม (source) — ผิวต่ำกว่าขอบบล็อกนิดเดียวแบบ Minecraft
                    _ => WATER_SURFACE_DROP,
                };
                let mut d = 0i32;
                while d < WATER_DEPTH_RANGE && sample(cx + dx, vy - d, cz + dz).is_water() {
                    d += 1;
                }
                depth_sum += d as f32;
                cnt += 1;
            } else if b == BlockType::Air {
                drop_sum += 1.0;
                cnt += 1;
            }
        }
    }
    if cnt > 0 {
        (
            drop_sum / cnt as f32,
            (depth_sum / cnt as f32 / WATER_DEPTH_RANGE as f32).min(1.0),
        )
    } else {
        (0.0, 0.0)
    }
}

pub fn create_mesh_from_blocks(
    chunk_pos: IVec2,
    blocks: &ChunkBlocks,
    neighbors: &[Arc<ChunkBlocks>; 8],
    chiseled_blocks: Option<&HashMap<usize, Box<[u8; 4096]>>>,
    facings: Option<&HashMap<usize, u8>>,
    branch_network: Option<&crate::tree::BranchNetwork>,
    light: Option<&LightNeighborhood>,
) -> ChunkMeshSet {
    // ต่อมุมผิวน้ำ: (ระยะกดผิวลง, ความลึกน้ำ normalize 0..1) — แชร์ข้ามหน้า/บล็อก
    let mut drop_cache: HashMap<(i32, i32, i32), (f32, f32)> = HashMap::with_capacity(1024);
    
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
        let src: &ChunkBlocks = match (x.div_euclid(w), z.div_euclid(w)) {
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
        src.get(lx, y as usize, lz)
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

    // แสงที่มุมของหน้า = เฉลี่ยแสงของ 4 เซลล์ฝั่งนอกหน้าที่ล้อมมุมนั้น (ชุดเดียวกับที่ AO
    // นับ) — เฉลี่ยเฉพาะเซลล์ที่แสงเข้าถึงได้ ไม่งั้นเนื้อบล็อกทึบ (แสง 0) จะดึงค่าลง
    // ทำให้ทุกมุมที่ติดผนังมืดผิดปกติ นี่คือ "smooth lighting" แบบ Minecraft
    let face_light = |c: [i32; 3], face_id: usize| -> [u8; 4] {
        let Some(lm) = light else { return [crate::light::MAX_LIGHT; 4] };
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

        let mut out = [0u8; 4];
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

            let mut sum = 0u32;
            let mut n = 0u32;
            for p in [base, p1, p2, pc] {
                if sample(p[0], p[1], p[2]).occludes() {
                    continue;
                }
                sum += lm.get(p[0], p[1], p[2]) as u32;
                n += 1;
            }
            // ทุกเซลล์รอบมุมทึบหมด (ซอกปิด) — ใช้ค่าของเซลล์ตรงหน้าไปตรงๆ
            out[i] = if n == 0 {
                lm.get(base[0], base[1], base[2])
            } else {
                (sum / n) as u8
            };
        }
        out
    };

    // สีของแผ่น sprite/กิ่ง ที่ไม่ได้ผ่านทาง AO ของหน้าคิวบ์ — ใช้แสงของช่องตัวเอง
    // (ถ้าไม่คูณ ใบไม้กับกิ่งจะสว่างเต็มแม้อยู่ในถ้ำ ลอยเด่นผิดที่ผิดทาง)
    let block_tint = |xi: i32, yi: i32, zi: i32| -> [f32; 4] {
        let level = light.map_or(crate::light::MAX_LIGHT, |lm| lm.get(xi, yi, zi));
        let b = sky_curve(level);
        [b, b, b, 1.0]
    };

    let axis_len = [CHUNK_WIDTH as i32, CHUNK_HEIGHT as i32, CHUNK_WIDTH as i32];

    // แถบ y ที่มีของจริง — ข้ามฟ้า Uniform(Air) ทั้งแถบ (หัวใจของคอลัมน์สูง:
    // หน้าของ chunk นี้เกิดจากบล็อกของ chunk นี้เท่านั้น จึงใช้ bounds ตัวเองพอ)
    let (y_lo, y_hi) = match blocks.y_bounds_non_air() {
        Some((lo, hi)) => (lo as i32, hi as i32),
        None => (0, -1), // อากาศล้วน — ทุกลูปกลายเป็นช่วงว่าง
    };

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

        // มิติไหนคือแกน y (index 1) ตัดช่วง loop ตามแถบ y — นอกแถบเป็นอากาศแน่นอน
        let dim_range = |dim: usize, len: i32| -> (i32, i32) {
            if dim == 1 { (y_lo, y_hi + 1) } else { (0, len) }
        };
        let (s0, s1) = dim_range(a, la);
        let (u0, u1) = dim_range(ua, lu);
        let (v0, v1) = dim_range(va, lv);

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

        // mask ของ slice: Some((ชนิดบล็อก, ระดับ AO, ลาย texture, ระดับ sky light)) = รอ merge
        // ลายกับแสงอยู่ใน key ด้วย — หน้าที่ลาย/ความสว่างต่างกัน merge รวมกันไม่ได้
        let mut mask: Vec<Option<(BlockType, u8, u8, u8)>> = vec![None; (lu * lv) as usize];

        for s in s0..s1 {
            // ล้างเฉพาะแถบที่ใช้ — นอกแถบไม่เคยถูกเขียน เป็น None ตลอด
            for vi in v0..v1 {
                for ui in u0..u1 {
                    mask[midx(ui, vi)] = None;
                }
            }

            for vi in v0..v1 {
                for ui in u0..u1 {
                    let mut c = [0i32; 3];
                    c[a] = s;
                    c[ua] = ui;
                    c[va] = vi;

                    let block = blocks.get(c[0] as usize, c[1] as usize, c[2] as usize);
                    // TallGrass ไม่ใช่ลูกบาศก์ — วาดแยกเป็นกากบาทท้ายฟังก์ชัน
                    // Chiseled ข้ามไปก่อน วาดแยกทีหลัง
                    // Branch เป็น Tapered Cylinder
                    // Leaves เป็นแผ่น sprite ตัดกันแบบดาว 3 แกน
                    if block == BlockType::Air || block == BlockType::TallGrass || block == BlockType::Chiseled || block == BlockType::Campfire || block == BlockType::SmartLamp || block == BlockType::SmartLampOn || block == BlockType::Branch || block == BlockType::Leaves {
                        continue;
                    }

                    // เห็นหน้านี้เมื่อเพื่อนบ้านโปร่งใส (อากาศ/น้ำ/กระจก/หญ้าสูง)
                    // แต่บล็อกโปร่งใสชนิดเดียวกันติดกันไม่วาดหน้าใน (น้ำ-น้ำ, กระจก-กระจก)
                    let n = sample(c[0] + norm[0], c[1] + norm[1], c[2] + norm[2]);
                    let visible = n == BlockType::Air || (block_def(n).transparent && n != block);
                    if !visible {
                        continue;
                    }
                    // น้ำติดน้ำไม่วาดหน้าระหว่างกันแม้ระดับต่าง — ผิวบนเป็น
                    // heightfield ต่อเนื่องอยู่แล้ว (มุมเฉลี่ยร่วมกัน) หน้าแทรก
                    // จะกลายเป็นแผ่นจมใต้ผิวซ้อน alpha เป็นเส้นเข้มดูสกปรก
                    // (ต้องแก้คู่กับ create_water_mesh เสมอ — parity test คุม)
                    if block.is_water() && n.is_water() {
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

                    let variant = if matches!(block, BlockType::Furnace | BlockType::Chest) {
                        let idx = ChunkData::get_index(c[0] as usize, c[1] as usize, c[2] as usize);
                        let facing = facings.and_then(|m| m.get(&idx)).copied().unwrap_or(4);
                        facing_variant(block, face_id, facing)
                    } else {
                        texture_variant(
                            block,
                            face_id,
                            world_base_x + c[0],
                            c[1],
                            world_base_z + c[2],
                        )
                    };

                    // บล็อกเรืองแสงสว่างเต็มเสมอ ไม่ขึ้นกับ sky light (ไม่งั้นโคมในถ้ำจะดำ)
                    let lit = if lamp_emission(block).is_some() {
                        [crate::light::MAX_LIGHT; 4]
                    } else {
                        face_light(c, face_id)
                    };

                    if !block.is_water()
                        && ao[0] == ao[1] && ao[1] == ao[2] && ao[2] == ao[3]
                        && lit[0] == lit[1] && lit[1] == lit[2] && lit[2] == lit[3]
                    {
                        mask[midx(ui, vi)] = Some((block, ao[0], variant, lit[0]));
                    } else {
                        // AO ไล่เฉดภายในหน้า — merge ไม่ได้ วาดเดี่ยวพร้อม flip diagonal
                        let tex = face_texture(block, face_id, variant);
                        let base = if tex.is_some() { [1.0, 1.0, 1.0, 1.0] } else { block_color(block) };
                        let shade = FACE_SHADE[face_id];
                        let mut verts = [[0f32; 3]; 4];
                        let mut cols = [[0f32; 4]; 4];
                        let mut uvs = [[0f32; 2]; 4];
                        let (vx, vy, vz) = (c[0], c[1], c[2]);
                        let is_w = block.is_water();
                        // ผิวน้ำเรียบแบบ Terraria: กดความสูง "รายมุม vertex" —
                        // แต่ละมุมเฉลี่ยระดับน้ำจาก column 2x2 ที่ล้อมมุมนั้นเอง
                        // มุมที่บล็อกข้างกันแชร์กันได้ค่าเดียวกัน (ผ่าน cache)
                        // ผิวน้ำจึงไล่ต่อเนื่องไม่เป็นขั้นบันได
                        let mut corner_drop = [0f32; 4];
                        let mut corner_depth = [0f32; 4];
                        // กดทุกหน้า (เดิมเว้น face_id 5 — ขอบบนด้าน Z- เลยโผล่
                        // เป็นครีบเหนือผิวที่ลาดลงไปแล้ว)
                        if is_w {
                            for i in 0..4 {
                                let p = CUBE_POSITIONS[face_id][i];
                                let (cx, cz) = (vx + p[0] as i32, vz + p[2] as i32);
                                let (d, dep) = *drop_cache.entry((cx, vy, cz)).or_insert_with(|| {
                                    water_corner_info(&sample, cx, vy, cz)
                                });
                                corner_drop[i] = d;
                                corner_depth[i] = dep;
                            }
                        }
                        for i in 0..4 {
                            let p = CUBE_POSITIONS[face_id][i];
                            verts[i] = [p[0] + vx as f32, p[1] + vy as f32, p[2] + vz as f32];
                            if is_w && p[1] > 0.5 { verts[i][1] -= corner_drop[i]; }
                            let br = shade * AO_CURVE[ao[i] as usize] * sky_curve(lit[i]);
                            // น้ำลึกสีเข้มกว่า (corner_depth = 0 สำหรับบล็อกอื่น)
                            let tint = 1.0 - WATER_DEPTH_DARKEN * corner_depth[i];
                            let a = if is_w { WATER_ALPHA } else { base[3] };
                            cols[i] = [base[0] * br * tint, base[1] * br * tint, base[2] * br * tint, a];
                            uvs[i] = face_uv(verts[i]);
                        }
                        let flip = (ao[0] as u32 + ao[2] as u32) < (ao[1] as u32 + ao[3] as u32);
                        let buf = if is_w {
                            &mut set.water
                        } else if let Some(t) = tex {
                            texture_buf(&mut set.textured, t)
                        } else {
                            &mut set.solid
                        };
                        buf.push_quad(verts, CUBE_NORMALS[face_id], cols, uvs, flip);
                    }
                }
            }

            // greedy merge: ขยายความกว้างก่อน แล้วขยายความสูงทั้งแถบ
            // (สแกนเฉพาะแถบ — นอกแถบเป็น None เสมอ)
            for vi in v0..v1 {
                for ui in u0..u1 {
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

                    let (block, ao_level, variant, light_level) = key;
                    let is_water = block.is_water();
                    let is_glass = block == BlockType::Glass;
                    let is_lamp = lamp_emission(block).is_some();
                    let tex = if is_water || is_glass || is_lamp {
                        None
                    } else {
                        face_texture(block, face_id, variant)
                    };
                    let base = if tex.is_some() { [1.0, 1.0, 1.0, 1.0] } else { block_color(block) };
                    let br = FACE_SHADE[face_id] * AO_CURVE[ao_level as usize] * sky_curve(light_level);
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

        blocks.for_each_matching(|b| b == BlockType::TallGrass, |xi, yi, zi, _| {
            let (x, y, z) = (xi as f32, yi as f32, zi as f32);
            let tint = block_tint(xi as i32, yi as i32, zi as i32);

            for quad in CROSS_QUADS {
                let mut verts = [[0f32; 3]; 4];
                for v in 0..4 {
                    verts[v] = [quad[v][0] + x, quad[v][1] + y, quad[v][2] + z];
                }
                texture_buf(&mut set.deco, sprite)
                    .push_quad(verts, [0., 1., 0.], [tint; 4], CROSS_UVS, false);
            }
        });
    }

    // ใบไม้ทรงดาว 3 แกน (แนว Better Leaves): แผ่น sprite ทแยงคู่ในทั้งสามระนาบ
    // พุ่มจึงฟูรอบทิศแทนที่จะเป็นก้อนเหลี่ยม
    if let Some(sprite) = face_texture(BlockType::Leaves, 2, 0) {
        blocks.for_each_matching(|b| b == BlockType::Leaves, |xi, yi, zi, _| {
            // ใบที่ถูกใบ/บล็อกทึบล้อมครบหกด้านมองไม่เห็นอยู่แล้ว — ข้ามไปเลย
            // (พุ่มหนาๆ ประหยัด quad ได้เยอะโดยหน้าตาไม่เปลี่ยน)
            let (cx, cy, cz) = (xi as i32, yi as i32, zi as i32);
            let hidden = FACE_OFFSETS.iter().all(|o| {
                let n = sample(cx + o[0], cy + o[1], cz + o[2]);
                n == BlockType::Leaves || !block_def(n).transparent && n != BlockType::Air
            });
            if hidden {
                return;
            }
            let tint = block_tint(cx, cy, cz);
            generate_leaf_mesh_into(&mut set, sprite, xi as f32, yi as f32, zi as f32, tint);
        });
    }

    if let Some(bn) = branch_network {
        blocks.for_each_matching(|b| b == BlockType::Branch, |xi, yi, zi, _| {
            let p = IVec3::new(chunk_pos.x * CHUNK_WIDTH as i32 + xi as i32, yi as i32, chunk_pos.y * CHUNK_WIDTH as i32 + zi as i32);
            let node = bn.nodes.get(&p);
            let thickness = node.map_or(crate::tree::LOOSE_THICKNESS, |n| n.thickness);

            // แต่ละทิศพก thickness ของ node ปลายทางมาด้วย — รอยต่อสองฝั่งจะได้คิด
            // รัศมีจากตัวเลขคู่เดียวกัน (ดู joint_radius) ผิวจึงต่อสนิทไม่เป็นขั้น
            let parent = node
                .and_then(|n| n.parent_pos)
                .map(|pp| (pp - p, bn.thickness_at(pp)));
            let mut children = Vec::new();
            if let Some(n) = node {
                for &cp in &n.children {
                    children.push((cp - p, bn.thickness_at(cp)));
                }
            }

            let tint = block_tint(xi as i32, yi as i32, zi as i32);
            generate_branch_mesh_into(&mut set, xi as f32, yi as f32, zi as f32, thickness, parent, &children, tint);
        });
    }

    if let Some(chiseled_map) = chiseled_blocks {
        blocks.for_each_matching(|b| b == BlockType::Chiseled, |x, y, z, _| {
            // map ของ sub-voxel ยัง key ด้วย flat index เดิม
            if let Some(chiseled_data) = chiseled_map.get(&ChunkData::get_index(x, y, z)) {
                generate_chiseled_mesh_into(&mut set, x as f32, y as f32, z as f32, chiseled_data);
            }
        });
    }

    set
}

/// รัศมีที่ระนาบรอยต่อของ node สองตัวที่ติดกัน — เฉลี่ย thickness ทั้งคู่ ทำให้ทั้ง
/// สองฝั่งคำนวณได้ค่าเดียวกันเสมอ ผิวจึงต่อสนิทไม่เป็นขั้นบันได
fn branch_joint_radius(t_self: u8, t_neighbor: u8) -> f32 {
    (t_self as f32 + t_neighbor as f32) * 0.5 / 32.0
}

/// ใบไม้ยื่นเลยขอบบล็อกออกไปเท่าไหร่ — ตัวที่ทำให้ขอบพุ่มดู "ฟู" แทนที่จะตัดตรง
/// เป็นเหลี่ยม ค่ามากไปพุ่มจะบวมทะลุกันเอง
const LEAF_OVERHANG: f32 = 0.15;

/// ใบไม้หนึ่งบล็อก = แผ่น sprite ทแยงคู่ในทั้งสามระนาบ (ดาว 3 แกน)
/// ไม่มีหน้าคิวบ์เลย พุ่มจึงโปร่งและฟูรอบทิศ มองจากใต้ต้นขึ้นไปเห็นเป็นหย่อมใบ
fn generate_leaf_mesh_into(
    set: &mut ChunkMeshSet,
    sprite: &'static str,
    bx: f32,
    by: f32,
    bz: f32,
    tint: [f32; 4],
) {
    const UVS: [[f32; 2]; 4] = [[0., 1.], [1., 1.], [1., 0.], [0., 0.]];
    let lo = -LEAF_OVERHANG;
    let hi = 1.0 + LEAF_OVERHANG;
    let mid = 0.5;

    // แต่ละระนาบมีแผ่นทแยงสองแผ่นตัดกัน — ระนาบตั้ง 2 ชุด (คร่อมแกน y)
    // และระนาบนอน 1 ชุด (คร่อมแกน x/z) รวม 6 แผ่น
    let quads: [[[f32; 3]; 4]; 6] = [
        // ทแยงในระนาบ XZ ตั้งขึ้น (เหมือนกากบาทของหญ้าสูง)
        [[lo, lo, lo], [hi, lo, hi], [hi, hi, hi], [lo, hi, lo]],
        [[hi, lo, lo], [lo, lo, hi], [lo, hi, hi], [hi, hi, lo]],
        // ทแยงในระนาบ XY (คร่อมแกน z ที่กึ่งกลาง)
        [[lo, lo, mid], [hi, lo, mid], [hi, hi, mid], [lo, hi, mid]],
        [[lo, hi, mid], [hi, hi, mid], [hi, lo, mid], [lo, lo, mid]],
        // ทแยงในระนาบ YZ (คร่อมแกน x ที่กึ่งกลาง)
        [[mid, lo, lo], [mid, lo, hi], [mid, hi, hi], [mid, hi, lo]],
        [[mid, lo, hi], [mid, lo, lo], [mid, hi, lo], [mid, hi, hi]],
    ];

    for quad in quads {
        let mut verts = [[0f32; 3]; 4];
        for v in 0..4 {
            verts[v] = [quad[v][0] + bx, quad[v][1] + by, quad[v][2] + bz];
        }
        // normal ชี้ขึ้นเหมือนหญ้าสูง — material เป็น double_sided อยู่แล้ว
        // ใบทุกแผ่นจึงรับแสงเท่ากันไม่ว่ามองจากทิศไหน ไม่มีแผ่นดำ
        texture_buf(&mut set.deco, sprite).push_quad(verts, [0., 1., 0.], [tint; 4], UVS, false);
    }
}

/// ช่วงของตัวต่อกิ่งวัดตามแกนของมันเอง: จากใจกลาง node ถึงระนาบรอยต่อ (ครึ่งทาง
/// ไปหาเพื่อนบ้าน) — node สองตัวที่ติดกันจึงปูเต็มระยะห่างพอดีโดยไม่เหลือคอคอด
fn extension_span(dir: IVec3) -> (f32, f32) {
    (0.0, dir.as_vec3().length() * 0.5)
}

/// แกนพิกัดของตัวต่อกิ่งไปทาง `dir` — คืน (u, n, w) โดย n คือทิศจริง ส่วน u/w คือ
/// แกนของหน้าตัด
///
/// u ถูกสร้างจาก **เส้นแกน** (canonical axis) ไม่ใช่จากทิศ เพื่อการันตีว่า node สองตัว
/// คนละฝั่งรอยต่อ (เห็นทิศเป็น d กับ -d) ได้หน้าตัดบิดรอบแกนเท่ากันเป๊ะ สี่เหลี่ยมจึงทับ
/// กันสนิท — Quat::from_rotation_arc บังเอิญให้ผลตรงกันในทั้ง 26 ทิศเหมือนกัน (มีเทส
/// ยืนยัน) แต่เป็นเรื่องบังเอิญจากความสมมาตร 90° ของหน้าตัดสี่เหลี่ยม ไม่ใช่การการันตี
/// (w พลิกเครื่องหมายตามทิศได้ เพราะหน้าตัดสมมาตร ±r — ตำแหน่งมุมยังตรงกัน
/// แต่ winding ยังชี้ออกนอกทั้งสองฝั่ง)
fn extension_basis(dir: IVec3) -> (Vec3, Vec3, Vec3) {
    let n = dir.as_vec3().normalize();
    // เส้นแกนเดียวกันต้องได้ canon ตัวเดียวกันไม่ว่ามองจากฝั่งไหน
    let canon_i = if (dir.x, dir.y, dir.z) < (0, 0, 0) { -dir } else { dir };
    let canon = canon_i.as_vec3().normalize();
    let helper = if canon.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let u = canon.cross(helper).normalize();
    // u × n = w — เรียงมือขวาแบบเดียวกับ (X, Y, Z) เดิม winding จึงไม่กลับด้าน
    let w = u.cross(n);
    (u, n, w)
}

/// ปลายกิ่งของ node แต่ละด้าน: ทิศ, รัศมีที่ขอบช่อง, ต่อกับ node จริงไหม
/// (ต่อกับ node จริง = ไม่ต้อง push ฝาปิด เพราะอีกฝั่งวาดต่อให้พอดี)
struct BranchEnd {
    dir: IVec3,
    radius: f32,
    joined: bool,
}

fn generate_branch_mesh_into(
    set: &mut ChunkMeshSet,
    bx: f32,
    by: f32,
    bz: f32,
    thickness: u8,
    parent: Option<(IVec3, Option<u8>)>,
    children: &[(IVec3, Option<u8>)],
    // ความสว่างของช่องที่กิ่งอยู่ (sky light) — ไม่งั้นกิ่งสว่างเต็มแม้ในถ้ำ/กลางคืน
    tint: [f32; 4],
) {
    let thickness_f = thickness as f32;
    let r_center = thickness_f / 32.0;
    // ไม่มี node จริงอีกฝั่ง: โคนบานออกนิด ปลายเรียวเข้า (ค่าเดิมก่อนแก้)
    let r_flare = (thickness_f + 2.0).min(16.0) / 32.0;
    let r_taper = (thickness_f - 2.0).max(2.0) / 32.0;

    let mut ends: Vec<BranchEnd> = Vec::with_capacity(children.len() + 1);
    // ไม่มี parent (root/กิ่งกำพร้า) = ถือว่าโคนชี้ลงตามแรงโน้มถ่วง
    let parent_dir = parent.map_or(IVec3::NEG_Y, |(d, _)| d);
    ends.push(match parent {
        Some((dir, Some(t))) => BranchEnd { dir, radius: branch_joint_radius(thickness, t), joined: true },
        _ => BranchEnd { dir: parent_dir, radius: r_flare, joined: false },
    });
    if children.is_empty() {
        // ปลายกิ่ง — เรียวต่อไปทางตรงข้ามโคน
        ends.push(BranchEnd { dir: -parent_dir, radius: r_taper, joined: false });
    } else {
        for &(dir, t) in children {
            ends.push(match t {
                Some(t) => BranchEnd { dir, radius: branch_joint_radius(thickness, t), joined: true },
                None => BranchEnd { dir, radius: r_taper, joined: false },
            });
        }
    }

    ends.retain(|e| e.dir != IVec3::ZERO);

    let tex = face_texture(BlockType::Branch, 2, 0).unwrap_or("textures/wood_side.png");

    // ไม่มีคิวบ์แกนกลางแล้ว — กิ่งประกอบจากแท่งเรียวที่ยิงออกจากใจกลาง node ล้วนๆ
    // แต่ละแท่งเป็นก้อนตันปิดครบทุกด้าน กิ่งทั้งเส้นจึงเป็นยูเนียนของก้อนตัน = ไม่มีรู
    //
    // คิวบ์เดิมสร้างปัญหาสองทาง: ถ้า cull หน้าที่มีกิ่งต่อ จะเหลือท่อเปิดเพราะแท่ง
    // เริ่มที่ใจกลางไม่ใช่ที่ผิวคิวบ์; ถ้าไม่ cull คิวบ์ก็โผล่เป็นก้อนเหลี่ยมคร่อมกิ่ง
    // ทุก node (เห็นชัดมากตอนกิ่งเฉียง เพราะคิวบ์วางตามแกนแต่กิ่งเอียง)

    let push_extension = |set: &mut ChunkMeshSet, dir: IVec3, r_end: f32, cap: bool| {
        // ยืดไปถึงจุดกึ่งกลางระหว่างศูนย์กลางบล็อกสองก้อน — แกนตรง 0.5, เฉียงขอบ ~0.71,
        // เฉียงมุม ~0.87 ทั้งสองฝั่งจึงบรรจบกันพอดีเพราะ branch_joint_radius สมมาตร
        // เริ่มจาก "ใจกลาง node" ไม่ใช่จากผิวคิวบ์ — ตัวต่อทุกเส้นของ node เดียวกันจึง
        // ซ้อนกันตรงกลางกลายเป็นดุมตัน และกิ่งเป็นแท่งต่อเนื่องเส้นเดียวจริงๆ
        //
        // เดิมเริ่มที่ผิวคิวบ์ ซึ่งพังหนักตอนกิ่งเฉียง: บล็อกที่ติดกันแบบเฉียงแตะกันแค่
        // "ขอบ" คิวบ์สองก้อนจึงไม่ชนกันเลย เหลือแค่คอเชื่อมบางๆ คั่นกลาง ภาพที่ได้
        // เป็นลูกปัดร้อยเชือกไม่ใช่กิ่งไม้
        let (min_y, max_y) = extension_span(dir);
        // หน้าตัดที่ใจกลางเท่าคิวบ์พอดี แล้วค่อยเรียวไปหา r_end ที่ระนาบรอยต่อ
        let r_start = r_center;
        let t = (r_center * 2.0).clamp(0.0, 1.0);

        // แกนอ้างอิงของหน้าตัดต้องขึ้นกับ "เส้นแกน" ไม่ใช่ทิศ — node สองตัวที่รอยต่อ
        // เดียวกันมองเห็นทิศตรงข้ามกัน (d กับ -d) ถ้าเอา d ไปสร้างแกนตรงๆ (เช่น
        // Quat::from_rotation_arc) หน้าตัดสองฝั่งจะบิดรอบแกนไม่เท่ากัน สี่เหลี่ยม
        // ไม่ทับกัน แล้วรอยต่อแตกเป็นรูโหว่ — ชัดมากตอนกิ่งเฉียง
        let (u, n, w) = extension_basis(dir);
        let at = |a: f32, y: f32, b: f32| -> [f32; 3] {
            let v = u * a + n * y + w * b;
            [v.x + bx + 0.5, v.y + by + 0.5, v.z + bz + 0.5]
        };
        let bot = [
            at(-r_start, min_y, -r_start), at( r_start, min_y, -r_start),
            at( r_start, min_y,  r_start), at(-r_start, min_y,  r_start),
        ];
        let top = [
            at(-r_end, max_y, -r_end), at( r_end, max_y, -r_end),
            at( r_end, max_y,  r_end), at(-r_end, max_y,  r_end),
        ];

        let mut push_face = |verts: [[f32; 3]; 4], normal: Vec3| {
            let uvs = [[0., 1.0 - t], [1., 1.0 - t], [1., 0.], [0., 0.]];
            texture_buf(&mut set.textured, tex)
                .push_quad(verts, [normal.x, normal.y, normal.z], [tint; 4], uvs, false);
        };

        push_face([bot[1], bot[0], top[0], top[1]], -w);
        push_face([bot[2], bot[1], top[1], top[2]], u);
        push_face([bot[3], bot[2], top[2], top[3]], w);
        push_face([bot[0], bot[3], top[3], top[0]], -u);
        // ฝาก้น — ปกติจมอยู่ในคิวบ์แกนกลางจึงมองไม่เห็น แต่ตอนกิ่งเฉียงหน้าตัดโผล่พ้น
        // คิวบ์ออกมา ถ้าไม่ปิดจะเห็นทะลุเข้าไปในกิ่ง (ด้านที่ "ล่องหน")
        push_face([bot[3], bot[2], bot[1], bot[0]], -n);
        // ฝาปิดปลาย — เว้นไว้เมื่อมี node จริงต่ออยู่ (อีกฝั่งวาดมาบรรจบพอดี ปิดซ้ำ
        // จะได้ quad ซ้อนกันสองแผ่นคาระนาบรอยต่อ)
        if cap {
            push_face([top[0], top[1], top[2], top[3]], n);
        }
    };

    for e in &ends {
        push_extension(set, e.dir, e.radius, !e.joined);
    }
}

fn generate_chiseled_mesh_into(
    set: &mut ChunkMeshSet,
    bx: f32,
    by: f32,
    bz: f32,
    data: &[u8; 4096]
) {
    let scale = 1.0 / 16.0;
    let get = |x: i32, y: i32, z: i32| -> u8 {
        if x < 0 || x > 15 || y < 0 || y > 15 || z < 0 || z > 15 {
            return 0;
        }
        data[x as usize + (y as usize) * 16 + (z as usize) * 256]
    };
    
    let face_uv = |face_id: usize, p: [f32; 3]| -> [f32; 2] {
        let norm = FACE_OFFSETS[face_id];
        let a = if norm[0] != 0 { 0 } else if norm[1] != 0 { 1 } else { 2 };
        match a {
            1 => [p[0], p[2]],
            0 => [p[2], -p[1]],
            _ => [p[0], -p[1]],
        }
    };

    for i in 0..4096 {
        let val = data[i];
        if val == 0 {
            continue;
        }
        
        let cx = (i % 16) as i32;
        let cy = ((i / 16) % 16) as i32;
        let cz = (i / 256) as i32;
        
        let (is_texture, color, block_type) = if val <= 127 {
            let bt = BlockType::from_u8(val);
            let col = block_def(bt).color;
            (true, [col[0], col[1], col[2], 1.0], bt)
        } else {
            // Palette mode 128-255: procedurally generate a hue based on value
            let hue = (val as f32 - 128.0) / 128.0;
            let rgb = Color::hsl(hue * 360.0, 0.8, 0.5).to_srgba();
            (false, [rgb.red, rgb.green, rgb.blue, 1.0], BlockType::Air)
        };

        for face_id in 0..6 {
            let norm = FACE_OFFSETS[face_id];
            let nx = cx + norm[0];
            let ny = cy + norm[1];
            let nz = cz + norm[2];
            
            if get(nx, ny, nz) == 0 {
                let mut verts = [[0f32; 3]; 4];
                let mut uvs = [[0f32; 2]; 4];
                let positions = CUBE_POSITIONS[face_id];
                
                for v in 0..4 {
                    let local_p = [
                        (cx as f32 + positions[v][0]) * scale,
                        (cy as f32 + positions[v][1]) * scale,
                        (cz as f32 + positions[v][2]) * scale,
                    ];
                    verts[v] = [
                        bx + local_p[0],
                        by + local_p[1],
                        bz + local_p[2],
                    ];
                    uvs[v] = face_uv(face_id, local_p);
                }
                
                let norm_f32 = [norm[0] as f32, norm[1] as f32, norm[2] as f32];
                
                if is_texture {
                    if let Some(path) = face_texture_list(block_type, face_id).first() {
                        // ถ้ามี texture ต้องใช้สีขาว (1.0) เพื่อไม่ให้สีไปปนกับสี texture 
                        // (เหมือนลอจิกใน create_mesh_from_blocks)
                        texture_buf(&mut set.textured, path).push_quad(verts, norm_f32, [[1.0, 1.0, 1.0, 1.0]; 4], uvs, false);
                    } else {
                        set.solid.push_quad(verts, norm_f32, [color; 4], [[0.0, 0.0]; 4], false);
                    }
                } else {
                    set.solid.push_quad(verts, norm_f32, [color; 4], [[0.0, 0.0]; 4], false);
                }
            }
        }
    }
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
            // seed เดียวคุมทุกชั้น — ให้ biome/ถ้ำเปลี่ยนตาม seed ด้วย ไม่ใช่แค่ความสูง
            fbm: Fbm::<Perlin>::new(params.seed).set_octaves(params.octaves as usize),
            temperature: Perlin::new(params.seed.wrapping_add(1)),
            cave: Perlin::new(params.seed.wrapping_add(2)),
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

    pub fn surface_block(&self, height: i32, desert: bool, sea_level: i32) -> BlockType {
        if desert || height <= sea_level + 1 {
            BlockType::Sand
        } else {
            BlockType::Grass
        }
    }
}

/// คืนบล็อกของ chunk + โครงกิ่งของต้นไม้ที่ปลูกไว้ (deterministic จากพิกัด chunk)
fn generate_chunk_blocks(
    chunk_pos: IVec2,
    noise: crate::NoiseParams,
    source: crate::TerrainSource,
) -> (ChunkBlocks, Vec<crate::tree::BranchRecord>) {
    let sampler = TerrainSampler::new(noise);
    // เริ่มเป็นอากาศ uniform ทั้งคอลัมน์ — เขียนเฉพาะที่มีของ ฟ้าไม่เคยถูก
    // materialize; compact() ท้ายฟังก์ชันยุบใต้ดิน/น้ำที่บังเอิญล้วนกลับเป็น 1 byte
    let mut blocks = ChunkBlocks::new_uniform(BlockType::Air);

    let base_x = chunk_pos.x as f64 * CHUNK_WIDTH as f64;
    let base_z = chunk_pos.y as f64 * CHUNK_WIDTH as f64;

    // โลกจริง: ความสูงจาก DEM (พิกัดบล็อก = เมตร), ทะเลจริงอยู่ y = DEM_SEA_LEVEL_Y
    // (ไม่มีไฟล์ dem → โลกอากาศล้วน; UI กัน RealWorld ไว้แล้วถ้าไฟล์ไม่มี)
    let dem_data = (source == crate::TerrainSource::RealWorld)
        .then(crate::dem::streamer)
        .flatten();
    let sea_level: i32 = if dem_data.is_some() {
        crate::dem::DEM_SEA_LEVEL_Y
    } else {
        SEA_LEVEL as i32
    };

    let mut heights = [[0i32; CHUNK_WIDTH]; CHUNK_WIDTH];
    let mut desert = [[false; CHUNK_WIDTH]; CHUNK_WIDTH];

    for z in 0..CHUNK_WIDTH {
        for x in 0..CHUNK_WIDTH {
            let wx = base_x + x as f64;
            let wz = base_z + z as f64;
            heights[z][x] = match dem_data {
                Some(d) => (crate::dem::DEM_SEA_LEVEL_Y as f32 + d.elevation_at_block(wx, wz))
                    .round()
                    .clamp(3.0, (CHUNK_HEIGHT - 16) as f32) as i32,
                None => sampler.height(wx, wz),
            };
            // โลกจริงไม่มี biome ทะเลทรายจาก noise (เชียงใหม่ไม่มีทะเลทราย)
            desert[z][x] = dem_data.is_none() && sampler.is_desert(wx, wz);
        }
    }

    for z in 0..CHUNK_WIDTH {
        for x in 0..CHUNK_WIDTH {
            let wx = base_x + x as f64;
            let wz = base_z + z as f64;
            let h = heights[z][x];
            let is_desert = desert[z][x];
            let surface = sampler.surface_block(h, is_desert, sea_level);
            // แม่น้ำ/ผืนน้ำจาก OSM mask (โลกจริง) — คอลัมน์นี้เป็นน้ำไหม
            let is_river = dem_data.is_some_and(|d| d.is_water_at_block(wx, wz));

            for y in 0..CHUNK_HEIGHT {
                let yi = y as i32;
                let block = if is_river && yi <= h && yi > h - 3 {
                    BlockType::Water // แม่น้ำ: น้ำ 3 บล็อกบนสุด (ท้องน้ำ = หินข้างล่าง)
                } else if yi < h - 3 {
                    BlockType::Stone
                } else if yi < h {
                    if is_desert { BlockType::Sand } else { BlockType::Dirt }
                } else if yi == h {
                    surface
                } else if yi <= sea_level {
                    BlockType::Water
                } else {
                    break; // เหนือนี้เป็นอากาศทั้งหมด
                };

                // ถ้ำ: เจาะเฉพาะแถบใต้ผิว 4..200 บล็อก — ภูเขาโลกจริงหนาเป็นพัน
                // เมตร ถ้าเจาะทั้งก้อน ผนังถ้ำที่มองไม่เห็นจะกิน VRAM เป็น GB
                // (โหมด noise ภูเขาบางกว่า 200 อยู่แล้ว พฤติกรรมแทบไม่เปลี่ยน)
                if block.is_solid()
                    && yi < h - 4
                    && yi > (h - 200).max(2)
                    && sampler.is_cave(wx, yi, wz)
                {
                    continue;
                }
                blocks.set(x, y, z, block);
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

    // ต้นไม้เป็นกิ่ง Branch ทั้งต้น — ลำต้นคือกิ่งหนาสุดเรียวขึ้นไปหายอด
    // จุดปลูกอยู่ในเขต 4..=11 เพื่อให้กิ่งที่แตกออกด้านข้างยังไม่ล้ำออกนอก chunk
    // (topology ของ chunk ต้องจบในตัวเอง — ดู chunk_records/merge_records)
    let mut branches: Vec<crate::tree::BranchRecord> = Vec::new();
    let tree_count = (next() % 3) as usize;
    for _ in 0..tree_count {
        let tx = 4 + (next() % 8) as i32;
        let tz = 4 + (next() % 8) as i32;
        let h = heights[tz as usize][tx as usize];
        let params = &TREE_PRESETS[ACTIVE_TREE_PRESET].1;
        // เผื่อความสูงลำต้นเต็มที่ + พุ่มยอด ให้ต้นสูงๆ (เช่น pine) ไม่ถูกตัดยอด
        let headroom = params.trunk_len.1 + params.limb_len.1 + 4;
        if desert[tz as usize][tx as usize] || h <= sea_level + 1 || h + headroom >= CHUNK_HEIGHT as i32 {
            continue;
        }
        grow_tree(&mut blocks, &mut branches, IVec3::new(tx, h + 1, tz), params, &mut next);
    }

    // หญ้าสูง: โปรยบนผิวหญ้า (ไม่ขึ้นในทะเลทราย/ใต้น้ำ)
    let tuft_count = (next() % 14) as usize;
    for _ in 0..tuft_count {
        let gx = (next() % CHUNK_WIDTH as u64) as usize;
        let gz = (next() % CHUNK_WIDTH as u64) as usize;
        let h = heights[gz][gx];
        if desert[gz][gx] || h <= sea_level + 1 || h + 1 >= CHUNK_HEIGHT as i32 {
            continue;
        }
        if blocks.get(gx, h as usize, gz) == BlockType::Grass
            && blocks.get(gx, (h + 1) as usize, gz) == BlockType::Air
        {
            blocks.set(gx, (h + 1) as usize, gz, BlockType::TallGrass);
        }
    }

    blocks.compact();

    // ตัวปั้นต้นไม้ทำงานบนพิกัด local ของ chunk (เพราะต้อง index blocks ตรงๆ) แต่
    // BranchNetwork ใช้พิกัด world ทั้งระบบ (mesh lookup, chunk_of, attach_branch_node)
    // — ถ้าลืมแปลง chunk อื่นนอกจาก (0,0) จะหา node ไม่เจอแล้ว fallback เป็นกิ่งผอม
    let origin = IVec3::new(
        chunk_pos.x * CHUNK_WIDTH as i32,
        0,
        chunk_pos.y * CHUNK_WIDTH as i32,
    );
    for r in &mut branches {
        r.pos = (IVec3::from_array(r.pos) + origin).to_array();
        r.parent = r.parent.map(|p| (IVec3::from_array(p) + origin).to_array());
    }

    (blocks, branches)
}

/// แปลงทิศทางต่อเนื่องเป็นก้าวเดียวในเพื่อนบ้าน 26 ทิศ — ตัวนี้แหละที่ทำให้เกิด
/// กิ่งเฉียง แทนที่จะเป็นบันไดตามแกน (mesh รองรับทิศเฉียงแล้ว ดู push_extension)
/// ไม่มีทางคืน (0,0,0): ถ้าทุกแกนต่ำกว่าเกณฑ์ จะบังคับใช้แกนที่แรงที่สุด
fn quantize_dir(dir: Vec3) -> IVec3 {
    let d = dir.normalize_or_zero();
    if d == Vec3::ZERO {
        return IVec3::Y;
    }
    let step = |c: f32| if c > 0.5 { 1 } else if c < -0.5 { -1 } else { 0 };
    let q = IVec3::new(step(d.x), step(d.y), step(d.z));
    if q != IVec3::ZERO {
        return q;
    }
    // ทุกแกนอยู่กลางๆ — เลือกแกนที่ค่าสัมบูรณ์มากสุด (tie-break x → y → z)
    let (ax, ay, az) = (d.x.abs(), d.y.abs(), d.z.abs());
    if ax >= ay && ax >= az {
        IVec3::new(d.x.signum() as i32, 0, 0)
    } else if ay >= az {
        IVec3::new(0, d.y.signum() as i32, 0)
    } else {
        IVec3::new(0, 0, d.z.signum() as i32)
    }
}

/// จุดนี้ยังอยู่ในกรอบ chunk (และช่วง y ที่เขียนได้) ไหม — กิ่งห้ามล้ำออกนอก chunk
/// เพราะโครงกิ่งถูกเซฟ/โหลด/ส่งข้าม network เป็นก้อนต่อ chunk
fn inside_chunk(p: IVec3) -> bool {
    p.x >= 0 && p.x < CHUNK_WIDTH as i32
        && p.z >= 0 && p.z < CHUNK_WIDTH as i32
        && p.y >= 0 && p.y < CHUNK_HEIGHT as i32
}

/// ทรงต้นไม้ทั้งหมดคุมจากที่นี่ที่เดียว — จูนตัวเลขแล้วเห็นผลทันทีโดยไม่ต้องแตะลอจิก
/// (ดู TREE_PRESETS สำหรับชุดค่าที่ใช้จริง และเทส dump_tree_previews สำหรับดูภาพเทียบ)
#[derive(Clone, Copy)]
pub struct TreeParams {
    /// ความยาวลำต้น (สุ่มในช่วง inclusive)
    pub trunk_len: (i32, i32),
    /// จำนวนชั้นการแตกกิ่ง (0 = ลำต้นล้วนไม่มีกิ่ง)
    pub max_depth: u32,
    /// จำนวนกิ่งที่แตกออกตรงยอดของแต่ละเส้น
    pub crown_forks: (i32, i32),
    /// โอกาสแตกกิ่งข้างต่อหนึ่งก้าวระหว่างเดิน (0 = แตกเฉพาะที่ยอดแบบไม้กวาด)
    /// ต้นไม้จริงแตกกิ่งตลอดความยาวลำต้น ไม่ใช่กระจุกที่ยอดจุดเดียว
    pub side_branch_chance: f32,
    /// ห้ามแตกกิ่งข้างในช่วงกี่ก้าวแรกของลำต้น (เว้นโคนให้โล่ง)
    pub bare_trunk: i32,
    /// ความยาวกิ่งชั้นแรก — ชั้นลึกกว่าสั้นลงทีละ 1
    pub limb_len: (i32, i32),
    /// ความเอียงออกจากแกนตั้งตอนแตกกิ่ง (0 = พุ่งขึ้นตรง, 1 = กางออกข้าง)
    pub tilt: f32,
    /// แรงส่ายรายก้าว — สูง = กิ่งคดเคี้ยว
    pub wobble: f32,
    /// แรงดึงขึ้นบนรายก้าว — สูง = กิ่งเชิดขึ้น, ต่ำ/ติดลบ = กิ่งทิ้งตัวลง
    pub climb: f32,
    /// สัดส่วน thickness ที่เหลือเมื่อเดินจนสุดกิ่งหนึ่งเส้น
    pub taper: f32,
    /// สัดส่วน thickness ที่เหลือทันทีหลังแตกกิ่ง — ตัวที่ทำให้ "ลำต้น vs กิ่ง" แยกกัน
    pub fork_drop: f32,
    /// รัศมีพุ่มใบที่ปลายกิ่งสุดท้าย และที่จุดแตกกิ่งระหว่างทาง
    pub leaf_tip: i32,
    pub leaf_fork: i32,
}

fn scale_thickness(t: u8, factor: f32) -> u8 {
    ((t as f32 * factor).round() as u8).max(crate::tree::MIN_THICKNESS)
}

/// ชุดทรงต้นไม้ที่เลือกใช้ได้ — ตัวแรกคือตัวที่ worldgen ใช้จริงตอนนี้
/// ดูภาพเทียบได้จากเทส `dump_tree_previews` (เขียนไฟล์ target/tree_previews.html)
pub const TREE_PRESETS: &[(&str, TreeParams)] = &[
    // ทรงร่ม: ลำต้นสั้น กิ่งกางออกกว้าง พุ่มใหญ่ — เงาเยอะ เดินลอดได้
    ("oak", TreeParams {
        trunk_len: (4, 6), max_depth: 2, crown_forks: (2, 3),
        side_branch_chance: 0.35, bare_trunk: 2, limb_len: (3, 5),
        tilt: 0.75, wobble: 0.6, climb: 0.25, taper: 0.8, fork_drop: 0.55,
        leaf_tip: 2, leaf_fork: 1,
    }),
    // ทรงกรวย: ลำต้นสูงชัดเจน กิ่งสั้นแตกถี่ตลอดลำต้น เอียงลงเล็กน้อย
    ("pine", TreeParams {
        trunk_len: (9, 12), max_depth: 1, crown_forks: (2, 3),
        side_branch_chance: 0.85, bare_trunk: 2, limb_len: (2, 3),
        tilt: 0.9, wobble: 0.25, climb: 0.05, taper: 0.65, fork_drop: 0.4,
        leaf_tip: 1, leaf_fork: 1,
    }),
    // เรียวสูง: ลำต้นสูงบาง กิ่งน้อยเชิดขึ้น พุ่มแคบ
    ("birch", TreeParams {
        trunk_len: (7, 10), max_depth: 2, crown_forks: (2, 2),
        side_branch_chance: 0.25, bare_trunk: 4, limb_len: (2, 4),
        tilt: 0.45, wobble: 0.35, climb: 0.45, taper: 0.75, fork_drop: 0.5,
        leaf_tip: 2, leaf_fork: 1,
    }),
    // บิดเบี้ยว: ลำต้นสั้นคด กิ่งเยอะแตกมั่ว ใบเป็นหย่อม — ต้นไม้แก่/ป่าดิบ
    ("gnarled", TreeParams {
        trunk_len: (3, 5), max_depth: 3, crown_forks: (2, 4),
        side_branch_chance: 0.5, bare_trunk: 1, limb_len: (2, 4),
        tilt: 0.85, wobble: 1.1, climb: 0.15, taper: 0.7, fork_drop: 0.6,
        leaf_tip: 2, leaf_fork: 1,
    }),
];

/// ทรงที่ worldgen ใช้อยู่ — เปลี่ยน index นี้เพื่อสลับทรงทั้งโลก
const ACTIVE_TREE_PRESET: usize = 0;

/// ปลูกต้นไม้ทั้งต้นที่ `base` — เขียนบล็อก Branch/Leaves ลง `blocks`
/// และสะสมโครง node ลง `records` (parent มาก่อนลูกเสมอ)
fn grow_tree(
    blocks: &mut ChunkBlocks,
    records: &mut Vec<crate::tree::BranchRecord>,
    base: IVec3,
    params: &TreeParams,
    next: &mut impl FnMut() -> u64,
) {
    if !inside_chunk(base) {
        return;
    }
    blocks.set(base.x as usize, base.y as usize, base.z as usize, BlockType::Branch);
    records.push(crate::tree::BranchRecord {
        pos: base.to_array(),
        parent: None,
        thickness: crate::tree::TRUNK_THICKNESS,
    });

    let trunk_len = pick_range(params.trunk_len, next);
    grow_limb(
        blocks, records, base, crate::tree::TRUNK_THICKNESS,
        Vec3::Y, trunk_len, 0, params, next,
    );
}

/// สุ่มจำนวนเต็มในช่วง inclusive แบบ deterministic
fn pick_range(range: (i32, i32), next: &mut impl FnMut() -> u64) -> i32 {
    let (lo, hi) = range;
    if hi <= lo {
        return lo;
    }
    lo + (next() % (hi - lo + 1) as u64) as i32
}

/// เดินกิ่งหนึ่งเส้นจาก `from` ไปทาง `dir` ยาว `len` ก้าว แล้วแตกกิ่งลูกต่อ
/// (`from` ต้องมี record อยู่แล้ว — ผู้เรียกเป็นคนวางบล็อกแรก)
#[allow(clippy::too_many_arguments)]
fn grow_limb(
    blocks: &mut ChunkBlocks,
    records: &mut Vec<crate::tree::BranchRecord>,
    from: IVec3,
    from_thickness: u8,
    dir: Vec3,
    len: i32,
    depth: u32,
    params: &TreeParams,
    next: &mut impl FnMut() -> u64,
) {
    // สุ่ม -0.5..0.5 จาก xorshift ตัวเดียวกับที่ใช้เลือกตำแหน่งต้นไม้ (deterministic)
    let jitter = |n: &mut dyn FnMut() -> u64| ((n() % 1000) as f32 / 1000.0) - 0.5;
    let chance = |n: &mut dyn FnMut() -> u64| (n() % 1000) as f32 / 1000.0;

    let mut cur = from;
    let mut thickness = from_thickness;
    let mut heading = dir.normalize_or_zero();
    // เรียวจาก from_thickness ลงไปหา tip_thickness เกลี่ยตลอดความยาวกิ่ง
    let tip_thickness = scale_thickness(from_thickness, params.taper);
    // จุดที่จะแตกกิ่งข้างระหว่างทาง เก็บไว้ทำทีหลังเพื่อไม่ให้ยืม blocks ซ้อนกัน
    let mut side_forks: Vec<(IVec3, u8)> = Vec::new();

    for i in 0..len {
        let step = quantize_dir(heading);
        let np = cur + step;
        if !inside_chunk(np) {
            break;
        }
        // ชนกิ่งที่มีอยู่แล้ว (กิ่งพี่น้อง/ต้นข้างๆ) — หยุดเส้นนี้ ไม่งั้นจะได้ record
        // สองใบที่ตำแหน่งเดียวกันแล้ว topology พัง
        if blocks.get(np.x as usize, np.y as usize, np.z as usize) == BlockType::Branch {
            break;
        }
        blocks.set(np.x as usize, np.y as usize, np.z as usize, BlockType::Branch);
        let f = (i + 1) as f32 / len as f32;
        thickness = (from_thickness as f32 + (tip_thickness as f32 - from_thickness as f32) * f)
            .round()
            .max(crate::tree::MIN_THICKNESS as f32) as u8;
        records.push(crate::tree::BranchRecord {
            pos: np.to_array(),
            parent: Some(cur.to_array()),
            thickness,
        });
        cur = np;

        // แตกกิ่งข้างระหว่างทาง — นี่คือตัวที่ทำให้ต้นไม้ไม่เป็นทรงไม้กวาด
        // (ถ้าแตกเฉพาะที่ยอด กิ่งทุกเส้นจะพุ่งออกจากจุดเดียวกันหมด)
        let past_bare = depth > 0 || i >= params.bare_trunk;
        if depth < params.max_depth && past_bare && chance(next) < params.side_branch_chance {
            side_forks.push((cur, thickness));
        }

        // ส่ายทุกก้าว + ดึงขึ้นบน กิ่งจึงโค้งแทนที่จะพุ่งตรงเป็นไม้บรรทัด
        heading = (heading
            + Vec3::new(jitter(next) * params.wobble, params.climb, jitter(next) * params.wobble))
            .normalize_or_zero();
        if heading == Vec3::ZERO {
            heading = Vec3::Y;
        }
    }

    let spread = |yaw: f32, tilt: f32| Vec3::new(yaw.cos() * tilt, 1.0 - tilt * 0.5, yaw.sin() * tilt);
    let child_len = (params.limb_len.0 - depth as i32).max(2);
    let child_range = (child_len, (params.limb_len.1 - depth as i32).max(child_len));

    // กิ่งข้างตลอดความยาว — เส้นละหนึ่งกิ่ง ทิศสุ่มรอบแกน
    for (at, t_here) in side_forks {
        let yaw = ((next() % 360) as f32).to_radians();
        let tilt = (params.tilt + jitter(next) * 0.3).clamp(0.1, 1.0);
        grow_limb(
            blocks, records, at, scale_thickness(t_here, params.fork_drop),
            spread(yaw, tilt), pick_range(child_range, next), depth + 1, params, next,
        );
        scatter_leaves(blocks, at, params.leaf_fork);
    }

    if depth >= params.max_depth {
        scatter_leaves(blocks, cur, params.leaf_tip);
        return;
    }

    // กิ่งกระจุกที่ยอด กระจายรอบแกนตั้งด้วยมุมเริ่มต้นสุ่ม
    let count = pick_range(params.crown_forks, next);
    let base_angle = ((next() % 360) as f32).to_radians();
    for i in 0..count {
        let yaw = base_angle + std::f32::consts::TAU * i as f32 / count.max(1) as f32;
        let tilt = (params.tilt + jitter(next) * 0.3).clamp(0.1, 1.0);
        // ตกฮวบตรงจุดแตกกิ่ง — กิ่งต้องดูเล็กกว่าลำต้นชัดเจนตั้งแต่บล็อกแรก
        grow_limb(
            blocks, records, cur, scale_thickness(thickness, params.fork_drop),
            spread(yaw, tilt), pick_range(child_range, next), depth + 1, params, next,
        );
    }
    scatter_leaves(blocks, cur, params.leaf_fork);
}

/// โปรยใบรอบจุดหนึ่ง (ไม่ทับบล็อกที่มีของอยู่แล้ว และไม่ล้ำออกนอก chunk)
fn scatter_leaves(blocks: &mut ChunkBlocks, center: IVec3, r: i32) {
    for dy in -r..=r {
        for dz in -r..=r {
            for dx in -r..=r {
                // ตัดมุมให้พุ่มกลมขึ้น ไม่เป็นกล่อง
                if dx.abs() + dy.abs() + dz.abs() > r + 1 {
                    continue;
                }
                let p = center + IVec3::new(dx, dy, dz);
                if !inside_chunk(p) {
                    continue;
                }
                if blocks.get(p.x as usize, p.y as usize, p.z as usize) == BlockType::Air {
                    blocks.set(p.x as usize, p.y as usize, p.z as usize, BlockType::Leaves);
                }
            }
        }
    }
}

/// สร้างเฉพาะ mesh น้ำของ chunk — คู่แฝดของเส้นทางน้ำใน create_mesh_from_blocks
/// (น้ำไม่เข้า greedy merge อยู่แล้ว จึงตัด machinery ทิ้งได้ทั้งหมด)
///
/// ต้อง**เป๊ะทุก byte**กับ set.water ของ mesher เต็ม (มี parity test คุม) —
/// ห้ามแก้ฝั่งเดียว: ลำดับ loop, predicate, quirk face_id != 5 ของ drop smoothing
/// ต้องตรงกันเสมอ
///
/// วนเฉพาะแถบ y [y_min, y_max] (superset ของน้ำจริง จาก metadata grow-only)
/// คืน (buffer, ช่วง y ที่เจอน้ำจริง) ไว้ tighten metadata — อิงการเจอ cell น้ำ
/// ไม่ใช่การมี face (น้ำจมไร้หน้าก็ยังต้องอยู่ใน band miếng khôngรูโผล่ตอน seam เปลี่ยน)
pub fn create_water_mesh(
    chunk_pos: IVec2,
    blocks: &ChunkBlocks,
    neighbors: &[Arc<ChunkBlocks>; 8],
    y_min: usize,
    y_max: usize,
) -> (MeshBuf, Option<(usize, usize)>) {
    let mut buf = MeshBuf::default();
    if y_min > y_max {
        return (buf, None);
    }
    let y_lo = y_min as i32;
    let y_hi = (y_max.min(CHUNK_HEIGHT - 1)) as i32;

    let mut drop_cache: HashMap<(i32, i32, i32), (f32, f32)> = HashMap::with_capacity(256);
    let mut observed: Option<(usize, usize)> = None;

    let world_base_x = chunk_pos.x * CHUNK_WIDTH as i32;
    let world_base_z = chunk_pos.y * CHUNK_WIDTH as i32;

    // เหมือน sample ใน create_mesh_from_blocks ทุกตัวอักษร
    let sample = |x: i32, y: i32, z: i32| -> BlockType {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return BlockType::Air;
        }
        let w = CHUNK_WIDTH as i32;
        let lx = x.rem_euclid(w) as usize;
        let lz = z.rem_euclid(w) as usize;
        let src: &ChunkBlocks = match (x.div_euclid(w), z.div_euclid(w)) {
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
        src.get(lx, y as usize, lz)
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

        let face_uv = move |p: [f32; 3]| -> [f32; 2] {
            match a {
                1 => [p[0], p[2]],
                0 => [p[2], -p[1]],
                _ => [p[0], -p[1]],
            }
        };

        // จำกัดเฉพาะตัวแปร loop ที่รับบทแกน y (a=0 → ui, a=1 → s, a=2 → vi)
        // การตัดช่วงไม่กระทบลำดับ emit เพราะ cell นอกแถบไม่มีน้ำให้วาดอยู่แล้ว
        let (s_lo, s_hi) = if a == 1 { (y_lo, y_hi) } else { (0, la - 1) };
        let (ui_lo, ui_hi) = if ua == 1 { (y_lo, y_hi) } else { (0, lu - 1) };
        let (vi_lo, vi_hi) = if va == 1 { (y_lo, y_hi) } else { (0, lv - 1) };

        for s in s_lo..=s_hi {
            for vi in vi_lo..=vi_hi {
                for ui in ui_lo..=ui_hi {
                    let mut c = [0i32; 3];
                    c[a] = s;
                    c[ua] = ui;
                    c[va] = vi;

                    let block = blocks.get(c[0] as usize, c[1] as usize, c[2] as usize);
                    if !block.is_water() {
                        continue;
                    }

                    // เจอน้ำ = อยู่ใน band จริง (นับก่อนเช็ค visibility)
                    let wy = c[1] as usize;
                    observed = Some(match observed {
                        Some((lo, hi)) => (lo.min(wy), hi.max(wy)),
                        None => (wy, wy),
                    });

                    let n = sample(c[0] + norm[0], c[1] + norm[1], c[2] + norm[2]);
                    let visible = n == BlockType::Air || (block_def(n).transparent && n != block);
                    if !visible {
                        continue;
                    }
                    // น้ำติดน้ำไม่วาดหน้าระหว่างกัน (ตรงกับ mesher เต็ม — parity test คุม)
                    if n.is_water() {
                        continue;
                    }

                    // ตรงกับ branch วาดเดี่ยวของ mesher เต็ม (น้ำ: ao คงที่ [3;4])
                    let variant = texture_variant(
                        block,
                        face_id,
                        world_base_x + c[0],
                        c[1],
                        world_base_z + c[2],
                    );
                    let tex = face_texture(block, face_id, variant);
                    let base = if tex.is_some() { [1.0, 1.0, 1.0, 1.0] } else { block_color(block) };
                    let shade = FACE_SHADE[face_id];
                    let ao = [3u8; 4];
                    let (vx, vy, vz) = (c[0], c[1], c[2]);

                    // กด/ไล่สีต่อมุมด้วย helper เดียวกับ mesher เต็ม (parity test คุม)
                    let mut corner_drop = [0f32; 4];
                    let mut corner_depth = [0f32; 4];
                    for i in 0..4 {
                        let p = CUBE_POSITIONS[face_id][i];
                        let (cx, cz) = (vx + p[0] as i32, vz + p[2] as i32);
                        let (d, dep) = *drop_cache.entry((cx, vy, cz)).or_insert_with(|| {
                            water_corner_info(&sample, cx, vy, cz)
                        });
                        corner_drop[i] = d;
                        corner_depth[i] = dep;
                    }

                    let mut verts = [[0f32; 3]; 4];
                    let mut cols = [[0f32; 4]; 4];
                    let mut uvs = [[0f32; 2]; 4];
                    for i in 0..4 {
                        let p = CUBE_POSITIONS[face_id][i];
                        verts[i] = [p[0] + vx as f32, p[1] + vy as f32, p[2] + vz as f32];
                        if p[1] > 0.5 { verts[i][1] -= corner_drop[i]; }
                        let br = shade * AO_CURVE[ao[i] as usize];
                        let tint = 1.0 - WATER_DEPTH_DARKEN * corner_depth[i];
                        // create_water_mesh วาดเฉพาะน้ำ → alpha ที่ vertex เสมอ (ดู WATER_ALPHA)
                        cols[i] = [base[0] * br * tint, base[1] * br * tint, base[2] * br * tint, WATER_ALPHA];
                        uvs[i] = face_uv(verts[i]);
                    }
                    let flip = (ao[0] as u32 + ao[2] as u32) < (ao[1] as u32 + ao[3] as u32);
                    buf.push_quad(verts, CUBE_NORMALS[face_id], cols, uvs, flip);
                }
            }
        }
    }

    (buf, observed)
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

/// โฟลเดอร์เซฟของโลกที่กำลังเล่น — ห้ามให้เซฟข้ามโลกโหลดปนกันเพราะพิกัด chunk ชนกันตรงๆ
/// เป็น global เพราะ chunk I/O ทำในงาน async ที่เข้าถึง resource ไม่ได้
/// `None` = ยังไม่ได้เลือกโลก ใช้ `saves/` ตาม default เดิม
static ACTIVE_SAVE_DIR: std::sync::RwLock<Option<std::path::PathBuf>> =
    std::sync::RwLock::new(None);

/// ตั้งโฟลเดอร์เซฟของโลกที่กำลังจะเข้า (โลกจากเมนู Singleplayer = saves/<slug>/)
pub fn set_active_save_dir(path: Option<std::path::PathBuf>) {
    if let Ok(mut guard) = ACTIVE_SAVE_DIR.write() {
        *guard = path;
    }
}

/// เส้นทาง dev mode: โลก noise ใช้ `saves/` โลกจริง (DEM) ใช้ `saves_dem/` แบบเดิม
pub fn set_legacy_save_dir(is_dem: bool) {
    let dir = if is_dem { "saves_dem" } else { "saves" };
    set_active_save_dir(Some(project_root().join(dir)));
}

/// โฟลเดอร์เซฟของโลกที่กำลังเล่น
pub fn active_save_dir() -> std::path::PathBuf {
    match ACTIVE_SAVE_DIR.read() {
        Ok(guard) => guard.clone().unwrap_or_else(|| project_root().join("saves")),
        Err(_) => project_root().join("saves"),
    }
}

fn chunk_save_path(chunk_pos: IVec2) -> std::path::PathBuf {
    active_save_dir().join(format!("chunk_{}_{}.bin", chunk_pos.x, chunk_pos.y))
}

/// ไฟล์เสริมของ chunk เก็บ facing + ของใน Chest/Furnace (แยกจาก .bin หลักเพื่อไม่แตะ
/// format บล็อกเดิมเลย — ไม่มีไฟล์นี้ = ไม่มี facing/container ใดๆ โหลดโลกเก่าได้ปกติ)
fn chunk_aux_path(chunk_pos: IVec2) -> std::path::PathBuf {
    active_save_dir().join(format!("chunk_{}_{}.aux.bin", chunk_pos.x, chunk_pos.y))
}

/// ไฟล์ที่สามของ chunk: โครงกิ่งไม้ (BranchNetwork เฉพาะส่วนของ chunk นี้)
/// แยกไฟล์ด้วยเหตุผลเดียวกับ .aux.bin — ChunkAux เป็น bincode แบบ positional
/// เพิ่ม field เข้าไปตรงๆ จะทำให้เซฟเก่าอ่านไม่ออกแล้ว facing/ของในหีบหายหมด
fn chunk_tree_path(chunk_pos: IVec2) -> std::path::PathBuf {
    active_save_dir().join(format!("chunk_{}_{}.tree.bin", chunk_pos.x, chunk_pos.y))
}

pub fn save_chunk_tree(chunk_pos: IVec2, records: &[crate::tree::BranchRecord]) {
    let path = chunk_tree_path(chunk_pos);
    if records.is_empty() {
        // ไม่มีกิ่งเหลือแล้ว (ทุบหมด) — ต้องลบไฟล์ ไม่งั้นโหลดครั้งหน้าจะฟื้นของเก่ากลับมา
        let _ = std::fs::remove_file(&path);
        return;
    }
    let Ok(body) = bincode::serialize(records) else { return };
    let mut bytes = Vec::with_capacity(body.len() + 5);
    bytes.extend_from_slice(b"TREE1");
    bytes.extend_from_slice(&body);
    if let Err(e) = std::fs::write(&path, bytes) {
        warn!("save chunk tree {:?} failed: {}", chunk_pos, e);
    }
}

pub fn load_chunk_tree(chunk_pos: IVec2) -> Vec<crate::tree::BranchRecord> {
    let Ok(bytes) = std::fs::read(chunk_tree_path(chunk_pos)) else { return Vec::new() };
    let Some(rest) = bytes.strip_prefix(b"TREE1") else { return Vec::new() };
    bincode::deserialize(rest).unwrap_or_default()
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct ChunkAux {
    facings: Vec<(u32, u8)>,
    chest: Vec<(u32, [Option<crate::item::WireItemStack>; 27])>,
    furnace: Vec<(u32, [Option<crate::item::WireItemStack>; 3])>,
}

pub fn save_chunk(chunk_pos: IVec2, blocks: &ChunkBlocks) {
    let _ = std::fs::create_dir_all(active_save_dir());
    // v2: compact ก่อนเซฟให้ section ที่ล้วนกลับกลายเป็น 1 byte (clone ถูก —
    // ส่วนใหญ่เป็น Uniform อยู่แล้ว)
    let mut compacted = blocks.clone();
    compacted.compact();
    if let Err(e) = std::fs::write(chunk_save_path(chunk_pos), compacted.to_save_bytes()) {
        warn!("save chunk {:?} failed: {}", chunk_pos, e);
    }
}

/// เซฟ chunk ครบชุด: บล็อก + .aux.bin (facing/container) + .tree.bin (โครงกิ่ง)
/// สะดวกกว่าเรียก save_chunk_full ตรงๆ เพราะดึง record กิ่งจาก network ให้เลย
pub fn save_loaded_chunk(world: &VoxelWorld, chunk_pos: IVec2) {
    if let Some(chunk) = world.chunks.get(&chunk_pos) {
        let records = world.branch_network.chunk_records(chunk_pos, CHUNK_WIDTH as i32);
        save_chunk_full(chunk_pos, chunk, &records);
    }
}

/// เหมือน save_chunk แต่เซฟไฟล์ .aux.bin (facing + container) และ .tree.bin ควบไปด้วย
pub fn save_chunk_full(chunk_pos: IVec2, chunk: &ChunkData, branches: &[crate::tree::BranchRecord]) {
    save_chunk(chunk_pos, &chunk.blocks);
    save_chunk_tree(chunk_pos, branches);
    let aux = ChunkAux {
        facings: chunk.facings.iter().map(|(&i, &f)| (i as u32, f)).collect(),
        chest: chunk.chest_slots.iter().map(|(&i, s)| {
            let mut wire = [None; 27];
            for (w, slot) in wire.iter_mut().zip(s.iter()) {
                *w = slot.map(crate::item::WireItemStack::from_stack);
            }
            (i as u32, wire)
        }).collect(),
        furnace: chunk.furnace_slots.iter().map(|(&i, s)| {
            let mut wire = [None; 3];
            for (w, slot) in wire.iter_mut().zip(s.iter()) {
                *w = slot.map(crate::item::WireItemStack::from_stack);
            }
            (i as u32, wire)
        }).collect(),
    };
    // chunk ไม่มี facing/container เลย — ไม่ต้องเขียนไฟล์ (และลบของเก่าถ้ามี กันค้าง)
    if aux.facings.is_empty() && aux.chest.is_empty() && aux.furnace.is_empty() {
        let _ = std::fs::remove_file(chunk_aux_path(chunk_pos));
        return;
    }
    match bincode::serialize(&aux) {
        Ok(bytes) => {
            let mut out = Vec::with_capacity(bytes.len() + 4);
            out.extend_from_slice(b"AUX1");
            out.extend_from_slice(&bytes);
            if let Err(e) = std::fs::write(chunk_aux_path(chunk_pos), out) {
                warn!("save chunk aux {:?} failed: {}", chunk_pos, e);
            }
        }
        Err(e) => warn!("encode chunk aux {:?} failed: {}", chunk_pos, e),
    }
}

/// อ่าน chunk จาก disk เป็น byte ต่อบล็อกแบบ flat (ให้ host ส่งต่อ client ผ่าน RLE)
pub fn load_chunk_bytes(chunk_pos: IVec2) -> Option<Vec<u8>> {
    let blocks = load_chunk(chunk_pos)?;
    Some(blocks.iter_all().map(|b| b as u8).collect())
}

fn load_chunk(chunk_pos: IVec2) -> Option<ChunkBlocks> {
    let bytes = std::fs::read(chunk_save_path(chunk_pos)).ok()?;
    // format ไม่ตรง (เซฟเก่าก่อนยุค section หรือคนละขนาดโลก) — ทิ้งตาม
    // ปรัชญาเดิม แล้ว generate ใหม่
    ChunkBlocks::from_save_bytes(&bytes)
}

/// แปลง facings map เป็นรูปแบบสายส่ง (network ChunkData / เซฟ) — ใช้ร่วมกันทั้งสองทาง
pub fn facings_to_wire(facings: &HashMap<usize, u8>) -> Vec<(u32, u8)> {
    facings.iter().map(|(&k, &v)| (k as u32, v)).collect()
}

/// แปลง chest+furnace slots เป็นรูปแบบสายส่งเดียวกัน (kind tag: 0=chest, 1=furnace)
pub fn containers_to_wire(
    chest: &HashMap<usize, Box<[Option<ItemStack>; 27]>>,
    furnace: &HashMap<usize, Box<[Option<ItemStack>; 3]>>,
) -> Vec<(u32, u8, Vec<Option<crate::item::WireItemStack>>)> {
    chest
        .iter()
        .map(|(&k, s)| {
            (
                k as u32,
                0u8,
                s.iter().map(|slot| slot.map(crate::item::WireItemStack::from_stack)).collect(),
            )
        })
        .chain(furnace.iter().map(|(&k, s)| {
            (
                k as u32,
                1u8,
                s.iter().map(|slot| slot.map(crate::item::WireItemStack::from_stack)).collect(),
            )
        }))
        .collect()
}

/// กลับด้าน facings_to_wire — ใช้ตอนรับ ServerMessage::ChunkData ฝั่ง client
pub fn wire_to_facings(wire: Vec<(u32, u8)>) -> HashMap<usize, u8> {
    wire.into_iter().map(|(k, v)| (k as usize, v)).collect()
}

/// กลับด้าน containers_to_wire — kind 0=chest(27)/1=furnace(3), ช่องอื่นทิ้ง (ข้อมูลเพี้ยน)
pub fn wire_to_containers(
    wire: Vec<(u32, u8, Vec<Option<crate::item::WireItemStack>>)>,
) -> (HashMap<usize, Box<[Option<ItemStack>; 27]>>, HashMap<usize, Box<[Option<ItemStack>; 3]>>) {
    let mut chest = HashMap::new();
    let mut furnace = HashMap::new();
    for (idx, kind, slots) in wire {
        let idx = idx as usize;
        match kind {
            0 if slots.len() == 27 => {
                let mut arr: Box<[Option<ItemStack>; 27]> = Box::new([None; 27]);
                for (dst, w) in arr.iter_mut().zip(slots) {
                    *dst = w.and_then(crate::item::WireItemStack::to_stack);
                }
                chest.insert(idx, arr);
            }
            1 if slots.len() == 3 => {
                let mut arr: Box<[Option<ItemStack>; 3]> = Box::new([None; 3]);
                for (dst, w) in arr.iter_mut().zip(slots) {
                    *dst = w.and_then(crate::item::WireItemStack::to_stack);
                }
                furnace.insert(idx, arr);
            }
            _ => {}
        }
    }
    (chest, furnace)
}

/// โหลด facing + container จากไฟล์ .aux.bin — ไม่มีไฟล์/decode ไม่ผ่าน = ว่างเปล่า
/// (ทั้งเซฟเก่าก่อนมีฟีเจอร์นี้ และ chunk ที่ไม่เคยมี Furnace/Chest)
pub fn load_chunk_aux(chunk_pos: IVec2) -> (HashMap<usize, u8>, HashMap<usize, Box<[Option<ItemStack>; 27]>>, HashMap<usize, Box<[Option<ItemStack>; 3]>>) {
    let empty = || (HashMap::new(), HashMap::new(), HashMap::new());
    let Ok(bytes) = std::fs::read(chunk_aux_path(chunk_pos)) else { return empty() };
    let Some(rest) = bytes.strip_prefix(b"AUX1") else { return empty() };
    let Ok(aux) = bincode::deserialize::<ChunkAux>(rest) else { return empty() };

    let facings = aux.facings.into_iter().map(|(i, f)| (i as usize, f)).collect();
    let chest = aux.chest.into_iter().map(|(i, wire)| {
        let mut slots: Box<[Option<ItemStack>; 27]> = Box::new([None; 27]);
        for (s, w) in slots.iter_mut().zip(wire.into_iter()) {
            *s = w.and_then(crate::item::WireItemStack::to_stack);
        }
        (i as usize, slots)
    }).collect();
    let furnace = aux.furnace.into_iter().map(|(i, wire)| {
        let mut slots: Box<[Option<ItemStack>; 3]> = Box::new([None; 3]);
        for (s, w) in slots.iter_mut().zip(wire.into_iter()) {
            *s = w.and_then(crate::item::WireItemStack::to_stack);
        }
        (i as usize, slots)
    }).collect();
    (facings, chest, furnace)
}

// --------------------------------------------------------
// Async Chunk Generation
// --------------------------------------------------------

pub struct ChunkBlockData {
    pub chunk_pos: IVec2,
    pub blocks: Arc<ChunkBlocks>,
    /// sub-voxel data ที่มากับ chunk (ตอนนี้ใช้เฉพาะ chunk ที่รับจาก network host)
    pub chiseled: HashMap<usize, Box<[u8; 4096]>>,
    /// facing ของ Furnace/Chest ต่อตำแหน่ง (จาก disk save หรือ network host)
    pub facings: HashMap<usize, u8>,
    pub chest_slots: HashMap<usize, Box<[Option<ItemStack>; 27]>>,
    pub furnace_slots: HashMap<usize, Box<[Option<ItemStack>; 3]>>,
    /// โครงกิ่งของ chunk นี้ — มาจาก .tree.bin ถ้าโหลดจาก disk หรือจากตัวปั้นต้นไม้
    /// ถ้าเป็น chunk ที่เพิ่ง generate (ดู spawn_block_generation_task)
    pub branches: Vec<crate::tree::BranchRecord>,
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
    source: crate::TerrainSource,
    version: u32,
    sender: Sender<ChunkBlockData>,
    use_disk_save: bool,
) {
    AsyncComputeTaskPool::get().spawn(async move {
        // ถ้ามีไฟล์เซฟ (ผู้เล่นเคยแก้ chunk นี้) ใช้ของเซฟแทนการ generate
        // — ยกเว้นตอนเป็น network client: save บนเครื่องเป็นโลก single player
        //   ของผู้เล่นเอง ห้ามเอามาปนกับโลกของ host
        let from_disk = use_disk_save.then(|| load_chunk(chunk_pos)).flatten();
        let loaded_from_disk = from_disk.is_some();
        let (facings, chest_slots, furnace_slots) = if use_disk_save && loaded_from_disk {
            load_chunk_aux(chunk_pos)
        } else {
            (HashMap::new(), HashMap::new(), HashMap::new())
        };
        // chunk ที่เคยเซฟ = โครงกิ่งอยู่ในไฟล์ (ผู้เล่นอาจทุบไปแล้ว ห้ามปั้นใหม่ทับ)
        // chunk ใหม่ = เอาโครงที่ตัวปั้นต้นไม้เพิ่งสร้าง ซึ่ง deterministic จาก seed
        let (blocks, branches) = match from_disk {
            Some(blocks) => {
                let trees = use_disk_save.then(|| load_chunk_tree(chunk_pos)).unwrap_or_default();
                (blocks, trees)
            }
            None => generate_chunk_blocks(chunk_pos, noise, source),
        };
        let _ = sender.send(ChunkBlockData {
            chunk_pos,
            blocks: Arc::new(blocks),
            chiseled: HashMap::new(),
            facings,
            chest_slots,
            furnace_slots,
            branches,
            version,
        });
    }).detach();
}

pub fn spawn_mesh_generation_task(
    chunk_pos: IVec2,
    blocks: Arc<ChunkBlocks>,
    neighbors: [Arc<ChunkBlocks>; 8],
    facings: HashMap<usize, u8>,
    // snapshot ของ branch node ในกรอบ chunk (async task แตะ resource ตรงๆ ไม่ได้) —
    // ถ้าไม่ส่งมา กิ่งจะถูกวาดด้วยค่า fallback แล้วเด้งรูปทรงตอน remesh ครั้งแรก
    branches: crate::tree::BranchNetwork,
    // lightmap ของ chunk + เพื่อนบ้าน (Arc ทั้งชุด clone ฟรี)
    light: LightNeighborhood,
    version: u32,
    sender: Sender<ChunkMeshData>,
) {
    AsyncComputeTaskPool::get().spawn(async move {
        let set = create_mesh_from_blocks(chunk_pos, &blocks, &neighbors, None, Some(&facings), Some(&branches), Some(&light));
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
                // preview เป็นเครื่องมือจูน noise — ใช้ทะเล noise เสมอ
                let top = sampler.surface_block(h, is_desert, SEA_LEVEL as i32);
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
                            // unlit: ความสว่างมาจาก vertex sky light ล้วน (แบบ Minecraft)
                            // ไม่ผ่าน PBR lighting — ไม่งั้นต้องมี DirectionalLight/ambient สูง
                            // และการเปลี่ยน ambient จะทำให้ทั้งฉาก re-extract → วูบ
                            unlit: true,
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
        unlit: true,
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.insert_resource(ChunkMaterial(material));

    // สีน้ำมาจาก vertex color — material เป็นสีขาวโปร่งใสคูณทับ
    let water_material = materials.add(StandardMaterial {
        // alpha จริงมาจาก vertex color (WATER_ALPHA) เพราะ unlit+vertex-colored ใช้ alpha
        // ของ vertex — ตั้ง base_color alpha 1.0 ไว้ ไม่งั้นเข้าใจผิดว่าคุมความโปร่งที่นี่
        base_color: Color::WHITE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        perceptual_roughness: 0.15,
        ..default()
    });
    commands.insert_resource(WaterMaterial(water_material));

    // กระจก: โปร่งใสกว่าน้ำ
    let glass_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.80, 0.90, 1.0, 0.30),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        perceptual_roughness: 0.08,
        ..default()
    });
    commands.insert_resource(GlassMaterial(glass_material));

    // sprite ของประดับ (หญ้าสูง + พู่หญ้าข้างบล็อก): alpha cutout + วาดสองหน้า
    // รวบรวมจาก overlay_side ของทุกบล็อก + sprite กากบาทของ Tall Grass
    let mut side_overlays: Vec<Vec<&'static str>> = Vec::with_capacity(BLOCK_DEFS.len());
    let mut deco_materials: HashMap<&'static str, Handle<StandardMaterial>> = HashMap::new();
    // sprite ที่วาดเป็นแผ่น alpha cutout ไม่ใช่หน้าคิวบ์ — Tall Grass และใบไม้
    // (ใบไม้วาดเป็นดาว 3 แกน ดู generate_leaf_mesh_into)
    let mut cutout_sprites: Vec<&'static str> =
        BLOCK_DEFS[BlockType::TallGrass as usize].tex_side.to_vec();
    cutout_sprites.extend_from_slice(BLOCK_DEFS[BlockType::Leaves as usize].tex_side);

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
                unlit: true,
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
    for i in 0..BLOCK_DEFS.len() {
        let block = BlockType::from_u8(i as u8);
        if let Some(color) = lamp_emission(block) {
            let def = block_def(block);
            let tex = def.tex_top.first().copied();
            
            let mut base_color = color;
            let mut handle = None;
            if let Some(path) = tex {
                if project_root().join("assets").join(path).exists() {
                    handle = Some(asset_server.load(path));
                    base_color = Color::WHITE;
                }
            }
            
            lamp_materials.insert(block, materials.add(StandardMaterial {
                base_color,
                base_color_texture: handle.clone(),
                emissive: color.to_linear() * 4.0,
                emissive_texture: handle,
                ..default()
            }));
        }
    }
    commands.insert_resource(LampMaterials(lamp_materials));

    commands.insert_resource(VoxelWorld::default());
    commands.insert_resource(ChunkGenerator::default());

    // ดวงอาทิตย์ไม่ใช่ DirectionalLight อีกต่อไป — ความสว่างมาจาก sky light ที่อบไว้ใน
    // vertex color คูณกับ base_color ของ material ที่ update_sun_system เขียนตามเวลา
    // (แบบ Minecraft) จึงไม่มี shadow map, ไม่มี N·L มาตีกับ lightmap และไม่ต้อง remesh
    // เวลาพระอาทิตย์เคลื่อน — entity นี้เหลือไว้เป็นที่เก็บสถานะเวลาอย่างเดียว
    commands.spawn((Sun, Transform::default()));
}

#[derive(Component)]
pub struct Sun;

/// ความยาววันจริง (วินาที) ที่เวลาในเกมเดินครบ 24 ชม. — 20 นาทีเท่า Minecraft
const GAME_DAY_SECONDS: f32 = 1200.0;

/// เดินเวลาของวันอัตโนมัติ (day-night cycle) — host/single เท่านั้น
/// client รับเวลาจาก host ผ่าน TimeOfDay message (ดู network.rs) จึงไม่เดินเอง
pub fn advance_time_system(
    time: Res<Time>,
    mut settings: ResMut<crate::GameSettings>,
    net_client: Option<Res<bevy_renet::RenetClient>>,
) {
    if net_client.is_some() {
        return;
    }
    let mut t = settings.time_of_day + time.delta_secs() * 24.0 / GAME_DAY_SECONDS;
    if t >= 24.0 {
        t -= 24.0;
    }
    settings.time_of_day = t;
}

/// ความแรงแดดตามเวลา — คูณกับ sky light ที่อบไว้ใน vertex color
/// แยกออกมาให้ระบบ tint material เรียกใช้ค่าเดียวกันทุกที่
pub fn sun_tint(time_of_day: f32) -> (f32, Color) {
    let hour_angle = (time_of_day - 6.0) / 12.0 * std::f32::consts::PI;
    let elevation = Vec3::new(hour_angle.cos(), hour_angle.sin(), 0.3)
        .normalize()
        .y
        .clamp(0.0, 1.0);
    // กลางคืนไม่ดำสนิท เหลือแสงจันทร์จางๆ ให้ยังเดินได้
    let strength = 0.12 + 0.88 * elevation.powf(0.7);
    // แดดอมส้มตอนใกล้ขอบฟ้า ขาวตอนกลางวัน
    let warm = 1.0 - (elevation * 2.0).min(1.0);
    let color = Color::srgb(
        strength,
        strength * (1.0 - 0.25 * warm),
        strength * (1.0 - 0.45 * warm),
    );
    (elevation, color)
}

/// อัปเดตเวลาของวัน: สีท้องฟ้า, ambient และ "ความแรงแดด" ที่คูณลง material ทุกตัว
/// ที่กินแสงจากฟ้า — ไม่มี DirectionalLight แล้ว ความสว่างมาจาก vertex light ล้วน
pub fn update_sun_system(
    settings: Res<crate::GameSettings>,
    mut ambient_query: Query<&mut AmbientLight>,
    mut clear_color: ResMut<ClearColor>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    chunk_mat: Option<Res<ChunkMaterial>>,
    block_mats: Option<Res<BlockMaterials>>,
    deco_mats: Option<Res<DecoMaterials>>,
    glass_mat: Option<Res<GlassMaterial>>,
    water_mat: Option<Res<WaterMaterial>>,
    lod: Option<Res<crate::lod::LodTiles>>,
    mut last: Local<Option<(f32, bool)>>,
) {
    // **Gate ทั้งฟังก์ชันบนสุด** — day-night ทำให้เวลาเดินทุกเฟรม ถ้าเขียน material/
    // clear_color ทุกเฟรม bevy จะ re-extract asset ใหม่ทุกเฟรม (เปลืองและเคยทำภาพวูบ)
    // จึงอัปเดตเฉพาะเมื่อเวลาขยับพอสังเกต (~1 วิจริง) หรือ material เพิ่งพร้อม —
    // day-night ยังลื่นพอเพราะ base_color เปลี่ยนทีละน้อยอยู่แล้ว
    let material_ready = chunk_mat.is_some();
    if let Some((last_time, last_ready)) = *last {
        if last_ready == material_ready && (settings.time_of_day - last_time).abs() < 0.02 {
            return;
        }
    }
    *last = Some((settings.time_of_day, material_ready));

    let (elevation, tint) = sun_tint(settings.time_of_day);

    // ambient เหลือแค่พื้นบางๆ — ถ้าสูงเท่าเดิม (80-400) ถ้ำจะไม่มืดเพราะ vertex light ถูกกลบ
    for mut ambient in ambient_query.iter_mut() {
        ambient.brightness = 8.0 + 40.0 * elevation;
    }

    // material ที่รับแสงจากฟ้า — คูณ tint เข้า base_color
    // (LampMaterials ไม่อยู่ในลิสต์: emissive ต้องสว่างเท่าเดิมตอนกลางคืน)
    let mut handles: Vec<&Handle<StandardMaterial>> = Vec::new();
    if let Some(m) = chunk_mat.as_ref() { handles.push(&m.0); }
    if let Some(m) = glass_mat.as_ref() { handles.push(&m.0); }
    if let Some(m) = water_mat.as_ref() { handles.push(&m.0); }
    if let Some(m) = block_mats.as_ref() { handles.extend(m.0.values()); }
    if let Some(m) = deco_mats.as_ref() { handles.extend(m.0.values()); }
    // ภูเขาระยะไกล (LOD) ต้องมืดตามด้วย ไม่งั้นกลางคืนขอบฟ้าสว่างค้างเป็นแถบ
    if let Some(l) = lod.as_ref() { handles.push(&l.material); }
    for handle in handles {
        if let Some(mut mat) = materials.get_mut(handle) {
            mat.base_color = tint;
        }
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
    mut pools: ResMut<ActivePools>,
    mut active_fluids: ResMut<ActiveFluids>,
) {
    if !request.0 {
        return;
    }
    request.0 = false;
    despawn_world(&mut commands, &mut world, &mut generator, &mut pools, &mut active_fluids);
}

/// ล้างโลกทั้งใบ: mesh entity ทุกชั้น + block data + งาน generate ที่ค้าง
/// (ใช้ร่วมกันระหว่าง regenerate กลางเกม กับตอนออกจากโลกกลับเมนู)
fn despawn_world(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    generator: &mut ChunkGenerator,
    pools: &mut ActivePools,
    active_fluids: &mut ActiveFluids,
) {
    // โลกกำลังจะหายทั้งใบ — สระ/น้ำที่ตื่นอยู่อ้างอิงบล็อกเก่า ทิ้งให้หมด
    pools.0.clear();
    active_fluids.0.clear();

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
    for (_, entities) in world.campfire_models.drain() {
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

/// ออกจากโลกกลับเมนูหลัก: เซฟ chunk ที่ค้าง แล้วล้างทุกอย่างที่มองเห็นได้
/// (ไม่งั้นโลกเดิมค้างเป็นฉากหลังเมนู และยังกิน frame ต่อไป)
#[allow(clippy::too_many_arguments)]
pub fn unload_world_on_exit(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    mut generator: ResMut<ChunkGenerator>,
    mut pools: ResMut<ActivePools>,
    mut active_fluids: ResMut<ActiveFluids>,
    mut active_tnt: ResMut<ActiveTnt>,
    mut nuke_jobs: ResMut<NukeJobs>,
    mut regenerate: ResMut<crate::RegenerateWorld>,
    dropped: Query<Entity, With<crate::item::DroppedItem>>,
) {
    // การขุด/วางเซฟทันทีอยู่แล้ว — ที่เหลือคือผลจาก fluid sim/ระเบิดที่ยังไหลอยู่
    // (เก็บ record กิ่งไว้ก่อน เพราะลูปข้างล่างยืม world.chunks แบบ mut อยู่)
    let dirty: Vec<IVec2> = world.chunks.iter().filter(|(_, c)| c.dirty).map(|(p, _)| *p).collect();
    let dirty_trees: Vec<Vec<crate::tree::BranchRecord>> = dirty
        .iter()
        .map(|p| world.branch_network.chunk_records(*p, CHUNK_WIDTH as i32))
        .collect();
    let saved = dirty.len();
    for (pos, records) in dirty.iter().zip(dirty_trees.iter()) {
        if let Some(chunk) = world.chunks.get_mut(pos) {
            save_chunk_full(*pos, chunk, records);
            chunk.dirty = false;
        }
    }
    if saved > 0 {
        info!("saved {saved} dirty chunks on world exit");
    }

    despawn_world(&mut commands, &mut world, &mut generator, &mut pools, &mut active_fluids);

    // ระเบิดที่ยังนับถอยหลัง/nuke ที่คำนวณค้างอยู่ อ้างถึงโลกที่เพิ่งหายไป
    active_tnt.0.clear();
    *nuke_jobs = NukeJobs::default();
    for entity in dropped.iter() {
        commands.entity(entity).despawn();
    }

    // โลกถูกล้างแล้ว — กันไม่ให้ regenerate ที่ค้างจากรอบก่อนไปทำงานตอนเข้าโลกหน้า
    regenerate.0 = false;
}

/// อ่านบล็อกด้วยพิกัด local ของ chunk ที่ทะลุขอบไปหาเพื่อนบ้านได้
/// — ลำดับ `neighbors` ต้องตรงกับ `chunk_neighbors()` และ `create_mesh_from_blocks`
fn neighbour_sample(
    blocks: &ChunkBlocks,
    neighbors: &[Arc<ChunkBlocks>; 8],
    x: i32,
    y: i32,
    z: i32,
) -> BlockType {
    if y < 0 || y >= CHUNK_HEIGHT as i32 {
        return BlockType::Air;
    }
    let w = CHUNK_WIDTH as i32;
    let lx = x.rem_euclid(w) as usize;
    let lz = z.rem_euclid(w) as usize;
    let src: &ChunkBlocks = match (x.div_euclid(w), z.div_euclid(w)) {
        (0, 0) => blocks,
        (1, 0) => &neighbors[0],
        (-1, 0) => &neighbors[1],
        (0, 1) => &neighbors[2],
        (0, -1) => &neighbors[3],
        (1, 1) => &neighbors[4],
        (1, -1) => &neighbors[5],
        (-1, 1) => &neighbors[6],
        _ => &neighbors[7],
    };
    src.get(lx, y as usize, lz)
}

/// คำนวณ sky light ของ chunk ใหม่ถ้ายัง dirty
///
/// คืน true **เฉพาะตอนค่าเปลี่ยนจริง** ไม่ใช่แค่ "คำนวณแล้ว" — chunk ที่ unload แล้ว
/// load กลับมาด้วยบล็อกชุดเดิมได้แสงเท่าเดิม ถ้าสั่ง remesh ทุกครั้งที่คำนวณ ภาพจะ
/// กระพริบรัวเพราะ mesh entity ถูก despawn/spawn ใหม่ทุกเฟรม
pub fn ensure_chunk_light(world: &mut VoxelWorld, chunk_pos: IVec2) -> bool {
    if !world.chunks.get(&chunk_pos).is_some_and(|c| c.light_dirty) {
        return false;
    }
    let blocks = world.chunks[&chunk_pos].blocks.clone();
    // เพื่อนบ้านที่ยังไม่โหลดถือเป็นฟ้าโล่ง — ห้ามบังคับให้ต้องครบ 8 ตัวก่อน ไม่งั้น
    // chunk ริมขอบ render distance จะคำนวณแสงไม่ได้เลย แล้วก็ mesh ไม่ได้ตามไปด้วย
    // (ค่าตรงขอบจะเพี้ยนนิดหน่อยจนกว่าเพื่อนบ้านจะมาถึง — ตอนนั้นถูกตีธง dirty ให้คิดใหม่)
    let empty: Arc<ChunkBlocks> = Arc::new(ChunkBlocks::new_uniform(BlockType::Air));
    let mut missing: u8 = 0;
    let neighbors: [Arc<ChunkBlocks>; 8] = {
        let positions = chunk_neighbors(chunk_pos);
        std::array::from_fn(|i| match world.chunks.get(&positions[i]) {
            Some(c) => c.blocks.clone(),
            None => {
                missing |= 1 << i;
                empty.clone()
            }
        })
    };

    // ไล่สแกนแค่ถึงยอดที่มีของจริงของทั้ง 9 chunk — ไม่งั้นต้องไล่ 3072 ชั้นต่อคอลัมน์
    let mut scan_top = blocks.y_bounds_non_air().map_or(0, |(_, hi)| hi);
    for n in &neighbors {
        scan_top = scan_top.max(n.y_bounds_non_air().map_or(0, |(_, hi)| hi));
    }

    let sampler = |x: i32, y: i32, z: i32| neighbour_sample(&blocks, &neighbors, x, y, z);
    let light = crate::light::compute_sky_light(&sampler, scan_top);

    let mut changed = false;
    if let Some(chunk) = world.chunks.get_mut(&chunk_pos) {
        changed = *chunk.light != light;
        if changed {
            chunk.light = Arc::new(light);
        }
        chunk.light_dirty = false;
        chunk.light_missing_neighbors = missing;
    }
    changed
}

/// lightmap ของ chunk + เพื่อนบ้าน 8 ทิศ สำหรับ mesher (ต้องอ่านข้ามขอบเพื่อให้แสง
/// ต่อเนื่องไม่เห็นตะเข็บระหว่าง chunk) — Arc ทั้งชุด clone แล้วส่งเข้า async task ได้ฟรี
#[derive(Clone)]
pub struct LightNeighborhood {
    pub own: Arc<crate::light::ChunkLight>,
    pub neighbors: [Arc<crate::light::ChunkLight>; 8],
}

impl LightNeighborhood {
    /// ระดับแสงที่พิกัด local (ทะลุขอบไปหาเพื่อนบ้านได้) — ลำดับเดียวกับ neighbour_sample
    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return crate::light::MAX_LIGHT;
        }
        let w = CHUNK_WIDTH as i32;
        let lx = x.rem_euclid(w) as usize;
        let lz = z.rem_euclid(w) as usize;
        let src = match (x.div_euclid(w), z.div_euclid(w)) {
            (0, 0) => &self.own,
            (1, 0) => &self.neighbors[0],
            (-1, 0) => &self.neighbors[1],
            (0, 1) => &self.neighbors[2],
            (0, -1) => &self.neighbors[3],
            (1, 1) => &self.neighbors[4],
            (1, -1) => &self.neighbors[5],
            (-1, 1) => &self.neighbors[6],
            _ => &self.neighbors[7],
        };
        src.get(lx, y as usize, lz)
    }
}

/// ประกอบ LightNeighborhood ของ chunk
///
/// คืน None ถ้าเพื่อนบ้านตัวใดยังโหลดไม่ครบ **หรือแสงยังไม่ได้คำนวณ** — ข้อหลังสำคัญ:
/// mesher อ่านแสงข้ามขอบเพื่อทำ smooth lighting ถ้าเพื่อนบ้านยังเป็นค่าเริ่มต้น (0 ทั้งก้อน)
/// ขอบ chunk จะกลายเป็นแถบมืดคาไว้ถาวรเพราะไม่มีอะไรมาสั่ง remesh ให้อีก
pub fn light_neighborhood(world: &VoxelWorld, chunk_pos: IVec2) -> Option<LightNeighborhood> {
    let own_chunk = world.chunks.get(&chunk_pos)?;
    if own_chunk.light_dirty {
        return None;
    }
    let own = own_chunk.light.clone();
    let positions = chunk_neighbors(chunk_pos);
    if !positions.iter().all(|p| world.chunks.get(p).is_some_and(|c| !c.light_dirty)) {
        return None;
    }
    Some(LightNeighborhood {
        own,
        neighbors: positions.map(|p| world.chunks[&p].light.clone()),
    })
}

/// จำนวน chunk ที่ยอมคำนวณแสงใหม่ต่อเฟรม — ต้องสูงพอจะไล่ทันตอนโหลดโลกครั้งแรก
/// (chunk ใหม่แต่ละก้อนตีธง dirty ใส่เพื่อนบ้านอีก 8 ตัว ถ้าไล่ไม่ทันจะไม่มี chunk ไหน
/// ผ่านเงื่อนไข mesh ได้เลย — เคยตั้งไว้ 2 แล้วเจอจอฟ้าเห็น mesh แค่ chunk เดียว)
const RELIGHT_BUDGET: usize = 32;

/// คำนวณ sky light ใหม่ให้ chunk ที่ dirty แล้วสั่ง remesh ตัวที่มี mesh อยู่แล้ว
/// (chunk ที่ยังไม่เคยมี mesh ไม่ต้องสั่ง — ระบบ generate จะ mesh ให้เองเมื่อแสงพร้อม)
pub fn relight_system(mut world: ResMut<VoxelWorld>) {
    let dirty: Vec<IVec2> = world
        .chunks
        .iter()
        .filter(|(_, c)| c.light_dirty)
        .map(|(p, _)| *p)
        .take(RELIGHT_BUDGET)
        .collect();

    for pos in dirty {
        if !ensure_chunk_light(&mut world, pos) {
            continue;
        }
        if world.generated_chunks.contains_key(&pos) {
            world.pending_branch_remesh.insert(pos);
        }
    }
}

/// เพื่อนบ้าน 8 ทิศ ตามลำดับที่ create_mesh_from_blocks ต้องการ
/// ดัชนีของทิศตรงข้ามใน `chunk_neighbors` — ถ้า N เป็นเพื่อนบ้านตัวที่ i ของเรา
/// เราก็เป็นเพื่อนบ้านตัวที่ `OPPOSITE_NEIGHBOR[i]` ของ N
/// (แกนตรงจับคู่ 0↔1, 2↔3 แต่ทแยงคือ 4↔7 และ 5↔6 — ไม่ใช่ i^1 อย่างที่ดูเผินๆ)
const OPPOSITE_NEIGHBOR: [usize; 8] = [1, 0, 3, 2, 7, 6, 5, 4];

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
    client_sync: Option<Res<crate::network::ClientSync>>,
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

        // โลกจริง: อย่า generate chunk จนกว่า DEM tile ที่ครอบมันจะโหลดเสร็จ
        // (ไม่งั้นได้ chunk ทะเลผิดๆ cache ค้าง) — ยังไม่พร้อม = ขอโหลด+ข้ามเฟรมนี้
        // (client ไม่ gen เอง รับจาก host — ข้าม guard)
        if settings.terrain_source == crate::TerrainSource::RealWorld
            && client_sync.is_none()
            && !world.chunks.contains_key(&chunk_pos)
        {
            if let Some(dem) = crate::dem::streamer() {
                let bx0 = (cx * CHUNK_WIDTH as i32) as f64;
                let bz0 = (cz * CHUNK_WIDTH as i32) as f64;
                if !dem.ensure_ready(bx0, bz0, bx0 + CHUNK_WIDTH as f64, bz0 + CHUNK_WIDTH as f64) {
                    continue; // tile ยังโหลดอยู่ — ลองใหม่เฟรมหน้า
                }
            }
        }

        // Phase 1: Block Generation
        if block_budget > 0
            && !world.chunks.contains_key(&chunk_pos)
            && !generator.generating_blocks.contains_key(&chunk_pos)
        {
            generator.generating_blocks.insert(chunk_pos, true);
            let sender = generator.sender_blocks.lock().unwrap().clone();
            // network client: chunk ที่ host ส่งมา (มี edit) ใช้แทนการ generate
            if let Some(received) = client_sync.as_ref().and_then(|cs| cs.full_chunks.get(&chunk_pos)) {
                let _ = sender.send(ChunkBlockData {
                    chunk_pos,
                    blocks: Arc::new(ChunkBlocks::from_dense_bytes(&received.blocks)),
                    chiseled: received.chiseled.clone(),
                    facings: received.facings.clone(),
                    chest_slots: received.chest_slots.clone(),
                    furnace_slots: received.furnace_slots.clone(),
                    branches: received.branches.clone(),
                    version: generator.version,
                });
            } else {
                spawn_block_generation_task(
                    chunk_pos, settings.noise, settings.terrain_source, generator.version, sender,
                    client_sync.is_none(),
                );
            }
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

            // แสงต้อง **final** ก่อน mesh ครั้งแรก — ไม่ใช่แค่คำนวณเสร็จ แต่ต้องคำนวณ
            // ตอนเพื่อนบ้านโหลดครบแล้ว (light_missing_neighbors == 0) ด้วย ไม่งั้น chunk
            // จะถูก mesh ด้วยแสงขอบเพี้ยน แล้วพอเพื่อนบ้านทยอยมา relight จะสั่ง remesh
            // ซ้ำทุกรอบ = solid mesh ถูก re-upload รัวๆ = ภาพกระพริบตอน stream
            // (chunk ริมขอบ render distance ไม่ผ่าน all_neighbors_ready อยู่แล้ว จึงไม่
            //  ค้างเป็นจอฟ้าเพราะเงื่อนไขนี้)
            let light_ready = world
                .chunks
                .get(&chunk_pos)
                .is_some_and(|c| !c.light_dirty && c.light_missing_neighbors == 0);

            if all_neighbors_ready && light_ready {
                let Some(light) = light_neighborhood(&world, chunk_pos) else { continue };
                generator.generating_meshes.insert(chunk_pos, true);

                let blocks = world.chunks.get(&chunk_pos).unwrap().blocks.clone();
                let neighbors = neighbors_pos.map(|p| world.chunks.get(&p).unwrap().blocks.clone());
                let facings = world.chunks.get(&chunk_pos).unwrap().facings.clone();
                let branches = world.branch_network.snapshot_for_chunk(chunk_pos, CHUNK_WIDTH as i32);

                let sender = generator.sender_meshes.lock().unwrap().clone();
                spawn_mesh_generation_task(chunk_pos, blocks, neighbors, facings, branches, light, generator.version, sender);
                mesh_budget -= 1;
            }
        }
    }
}

/// สลับชุด mesh เรืองแสงของ chunk (entity เก่าทิ้ง สร้างใหม่ตาม buffer ที่ได้มา)
/// สลับชุด mesh หลาย entity ต่อ chunk (deco/textured/glow) โดย **reuse entity เดิม
/// แล้วเขียนทับ mesh asset ในที่เดิม** แทนการ despawn+respawn
///
/// นี่คือหัวใจของการแก้ภาพกระพริบ: despawn+spawn ในเฟรมเดียวทำให้ entity ใหม่มี mesh
/// handle ที่ GPU ยังไม่ prepare ระหว่างที่ entity เก่าหายไปแล้ว = 1 เฟรมว่าง พอ relight
/// สั่ง remesh ซ้ำตอน chunk ข้างๆ โหลด ต้นไม้/ใบเลยวาบหาย (พื้นดินไม่เป็นเพราะสลับ
/// asset ในที่เดิมอยู่แล้ว) — reuse handle เดิม GPU มีข้อมูลเก่าให้วาดต่อระหว่างรอ upload
fn update_multi_mesh_entities(
    commands: &mut Commands,
    slots: &mut HashMap<IVec2, Vec<Entity>>,
    meshes: &mut Assets<Mesh>,
    mesh_query: &Query<&Mesh3d>,
    chunk_pos: IVec2,
    items: Vec<(Handle<StandardMaterial>, MeshBuf)>,
    transform: Transform,
    no_shadow: bool,
) {
    let old = slots.remove(&chunk_pos).unwrap_or_default();
    let mut old_iter = old.into_iter();
    let mut entities = Vec::new();

    for (material, buf) in items {
        if buf.is_empty() {
            continue;
        }
        let mesh = buf.into_mesh();
        if let Some(entity) = old_iter.next() {
            // reuse: เขียนทับ asset ผ่าน handle เดิมถ้ายังมี ไม่งั้นใส่ handle ใหม่
            if let Ok(mesh3d) = mesh_query.get(entity) {
                let _ = meshes.insert(mesh3d.0.id(), mesh);
                commands.entity(entity)
                    .insert(MeshMaterial3d(material))
                    .remove::<Aabb>();
            } else {
                commands.entity(entity)
                    .insert((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material)))
                    .remove::<Aabb>();
            }
            entities.push(entity);
        } else {
            let mut ec = commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material),
                transform,
                Block,
            ));
            if no_shadow {
                ec.insert(NotShadowCaster);
            }
            entities.push(ec.id());
        }
    }
    // entity เก่าที่เหลือเกิน (หน้าลดลง) — despawn ทิ้ง ไม่ทำให้กระพริบเพราะเป็นการหายจริง
    for entity in old_iter {
        commands.entity(entity).despawn();
    }
    if !entities.is_empty() {
        slots.insert(chunk_pos, entities);
    }
}

fn update_glow_entities(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    meshes: &mut Assets<Mesh>,
    mesh_query: &Query<&Mesh3d>,
    lamp_materials: &LampMaterials,
    chunk_pos: IVec2,
    glow: Vec<(BlockType, MeshBuf)>,
    transform: Transform,
) {
    let items: Vec<(Handle<StandardMaterial>, MeshBuf)> = glow
        .into_iter()
        .filter_map(|(block, buf)| lamp_materials.0.get(&block).map(|m| (m.clone(), buf)))
        .collect();
    update_multi_mesh_entities(
        commands, &mut world.glow_chunks, meshes, mesh_query, chunk_pos, items, transform, false,
    );
}

/// สลับชุด mesh แบบ deco ของ chunk (entity เก่าทิ้ง สร้างใหม่)
fn update_deco_entities(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    meshes: &mut Assets<Mesh>,
    deco_materials: &DecoMaterials,
    mesh_query: &Query<&Mesh3d>,
    chunk_pos: IVec2,
    deco: Vec<(&'static str, MeshBuf)>,
    transform: Transform,
) {
    // ของประดับ (ใบไม้/หญ้าสูง) ไม่ทอดเงา — เคยลองให้ใบทอดเงาแล้วพัง (shadow map resolve
    // รูโปร่งความถี่สูงไม่ไหว เป็นก้อนด่าง + เฟรมตก) จึงเลิก ใช้ sky lightmap แทน
    let items: Vec<(Handle<StandardMaterial>, MeshBuf)> = deco
        .into_iter()
        .filter_map(|(tex, buf)| deco_materials.0.get(tex).map(|m| (m.clone(), buf)))
        .collect();
    update_multi_mesh_entities(
        commands, &mut world.deco_chunks, meshes, mesh_query, chunk_pos, items, transform, true,
    );
}

/// สลับชุด mesh แบบมี texture ของ chunk (reuse entity เดิม ดู update_multi_mesh_entities)
fn update_textured_entities(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    meshes: &mut Assets<Mesh>,
    block_materials: &BlockMaterials,
    mesh_query: &Query<&Mesh3d>,
    chunk_pos: IVec2,
    textured: Vec<(&'static str, MeshBuf)>,
    transform: Transform,
) {
    let items: Vec<(Handle<StandardMaterial>, MeshBuf)> = textured
        .into_iter()
        .filter_map(|(tex, buf)| block_materials.0.get(tex).map(|m| (m.clone(), buf)))
        .collect();
    update_multi_mesh_entities(
        commands, &mut world.textured_chunks, meshes, mesh_query, chunk_pos, items, transform, false,
    );
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
pub fn refresh_chunk_lamp_lights(
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
    chunk.blocks.for_each_matching(|b| lamp_emission(b).is_some(), |x, y, z, block| {
        let Some(color) = lamp_emission(block) else { return };

        let y_offset = if block == BlockType::SmartLamp || block == BlockType::SmartLampOn {
            0.625 // ปรับตำแหน่ง PointLight ให้ตรงกับตำแหน่งหลอดไฟใน model
        } else {
            0.5
        };

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
                y as f32 + y_offset,
                base_z + z as f32 + 0.5,
            ),
        )).id();
        // Campfire ต้องได้ particle ไฟ ไม่ใช่ sparkle ทั่วไปของ lamp สี — แท็กไว้ให้
        // attach_campfire_flames จับแทน attach_lamp_sparkles (ดู particles.rs)
        if block == BlockType::Campfire {
            commands.entity(entity).insert(crate::particles::CampfireFlameSource);
        }
        lights.push(entity);
    });
    if !lights.is_empty() {
        world.lamp_lights.insert(chunk_pos, lights);
    }
}

/// glTF model ของ Campfire ต่อตำแหน่ง — เหมือน refresh_chunk_lamp_lights เป๊ะ (despawn ของเก่า
/// ทั้งชุดแล้วสแกน+spawn ใหม่) ต่างกันที่ spawn WorldAssetRoot (glTF scene) แทน PointLight
pub fn refresh_chunk_campfire_models(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    chunk_pos: IVec2,
    assets: &BlockModelAssets,
) {
    if let Some(old) = world.campfire_models.remove(&chunk_pos) {
        for entity in old {
            commands.entity(entity).despawn();
        }
    }

    let Some(chunk) = world.chunks.get(&chunk_pos) else { return };

    let base_x = (chunk_pos.x * CHUNK_WIDTH as i32) as f32;
    let base_z = (chunk_pos.y * CHUNK_WIDTH as i32) as f32;

    let mut models = Vec::new();
    chunk.blocks.for_each_matching(
        |b| b == BlockType::Campfire || b == BlockType::SmartLamp || b == BlockType::SmartLampOn, 
        |x, y, z, block| {
            let scene = if block == BlockType::Campfire {
                assets.campfire_scene.clone()
            } else {
                assets.light_bulb_scene.clone()
            };

            let rotation = if block == BlockType::SmartLamp || block == BlockType::SmartLampOn {
                let idx = ChunkData::get_index(x, y, z);
                let facing = chunk.facings.get(&idx).copied().unwrap_or(4);
                // 2 = +X, 3 = -X, 4 = +Z, 5 = -Z
                match facing {
                    2 => std::f32::consts::PI / 2.0,
                    3 => -std::f32::consts::PI / 2.0,
                    4 => 0.0,
                    5 => std::f32::consts::PI,
                    _ => 0.0,
                }
            } else {
                0.0
            };

            let entity = commands.spawn((
                WorldAssetRoot(scene),
                Transform::from_xyz(
                    base_x + x as f32 + 0.5,
                    y as f32,
                    base_z + z as f32 + 0.5,
                ).with_rotation(Quat::from_rotation_y(rotation)),
            )).id();
            models.push(entity);
        }
    );
    if !models.is_empty() {
        world.campfire_models.insert(chunk_pos, models);
    }
}

/// แคช Asset ของโมเดล 3D ต่างๆ ไว้ที่เดียวกัน
#[derive(Resource)]
pub struct BlockModelAssets {
    pub campfire_scene: Handle<WorldAsset>,
    pub light_bulb_scene: Handle<WorldAsset>,
}

pub fn setup_campfire_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(BlockModelAssets {
        campfire_scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset("model/campfire.gltf")),
        light_bulb_scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset("model/light_blub.gltf")),
    });
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
    mesh_query: Query<&Mesh3d>,
    mut client_sync: Option<ResMut<crate::network::ClientSync>>,
    mut active_fluids: ResMut<ActiveFluids>,
    mut active_tnt: ResMut<ActiveTnt>,
    campfire_assets: Res<BlockModelAssets>,
) {
    // Process Blocks
    let mut received_blocks = Vec::new();
    {
        let receiver = generator.receiver_blocks.lock().unwrap();
        while let Ok(block_data) = receiver.try_recv() {
            received_blocks.push(block_data);
            if received_blocks.len() >= 4 { break; }
        }
    }

    for block_data in received_blocks {
        // ผลจากโลกรุ่นเก่า (ก่อน reset) — ทิ้งไปเลย ห้ามแตะ generating maps
        // เพราะอาจมี task รุ่นใหม่ของ chunk เดียวกันกำลังทำงานอยู่
        if block_data.version != generator.version {
            continue;
        }
        let chunk_pos = block_data.chunk_pos;

        // TntLit/NukeLit ค้างจากเซฟ (จุดแล้วแต่ปิดเกมก่อนระเบิด) — re-arm fuse สั้นๆ
        // เฉพาะเจ้าของ simulation (host/single) เหมือน fluid
        if client_sync.is_none() {
            let base_x = chunk_pos.x * CHUNK_WIDTH as i32;
            let base_z = chunk_pos.y * CHUNK_WIDTH as i32;
            block_data.blocks.for_each_matching(
                |b| matches!(b, BlockType::TntLit | BlockType::NukeLit),
                |x, y, z, _| {
                    active_tnt.0.insert(
                        IVec3::new(base_x + x as i32, y as i32, base_z + z as i32),
                        Timer::from_seconds(1.0, TimerMode::Once),
                    );
                },
            );
        }

        // โครงกิ่งเข้า network ก่อน mesh task จะถูก spawn (คนละระบบ รันทีหลัง)
        // ไม่งั้นกิ่งจะถูกวาดด้วยค่า fallback แล้วเด้งรูปทรงตอน remesh ครั้งแรก
        world.branch_network.merge_records(&block_data.branches);

        let (water_y_min, water_y_max) = scan_water_bounds(&block_data.blocks);
        world.chunks.insert(chunk_pos, ChunkData {
            blocks: block_data.blocks,
            chiseled_blocks: block_data.chiseled,
            facings: block_data.facings,
            chest_slots: block_data.chest_slots,
            furnace_slots: block_data.furnace_slots,
            num_vertices: 0,
            num_indices: 0,
            water_y_min,
            water_y_max,
            num_water_vertices: 0,
            num_water_indices: 0,
            dirty: false,
            light: Default::default(),
            light_dirty: true,
            light_missing_neighbors: 0,
        });
        generator.generating_blocks.remove(&chunk_pos);

        // chunk ใหม่โผล่มา = แสงที่ขอบของเพื่อนบ้านอาจเปลี่ยน — แต่ปลุก**เฉพาะตัวที่
        // คำนวณแสงไปตอนที่ยังไม่เห็น chunk นี้**เท่านั้น ตัวที่คำนวณหลังจากนี้เห็นของจริง
        // อยู่แล้วไม่ต้องคิดใหม่ (เดิมปลุกทั้ง 8 ทุกครั้ง → remesh ลาม 9 chunk ต่อครั้ง)
        for (i, n) in chunk_neighbors(chunk_pos).into_iter().enumerate() {
            let opposite_bit = 1u8 << OPPOSITE_NEIGHBOR[i];
            if let Some(c) = world.chunks.get_mut(&n) {
                if c.light_missing_neighbors & opposite_bit != 0 {
                    c.light_dirty = true;
                }
            }
        }

        // network client: edit ที่มาถึงก่อน chunk โหลด — apply ก่อน mesh ถูกสร้าง
        if let Some(cs) = client_sync.as_mut() {
            if let Some(edits) = cs.pending_edits.remove(&chunk_pos) {
                for edit in edits {
                    apply_block_edit(&mut world, &edit);
                }
                cs.edited.insert(chunk_pos);
            }
        }

        // ปลุกน้ำริมตะเข็บกับเพื่อนบ้าน — เว้น client (host เป็นคนรัน fluid sim)
        if client_sync.is_none() {
            wake_seam_water(&world, chunk_pos, &mut active_fluids);
        }
    }

    // Process Meshes
    let mut received_meshes = Vec::new();
    {
        let receiver = generator.receiver_meshes.lock().unwrap();
        while let Ok(mesh_data) = receiver.try_recv() {
            received_meshes.push(mesh_data);
            if received_meshes.len() >= 4 { break; }
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
        let num_water_vertices = set.water.positions.len();
        let num_water_indices = set.water.indices.len();
        let ChunkMeshSet { solid, water, glass, deco, glow, textured } = set;

        // นับสถิติเฉพาะ chunk ที่มี block data อยู่จริง — mesh ที่มาถึงหลัง
        // chunk ถูก unload (หรือ mesh ของ preview mode) จะไม่ถูกนับ กันตัวเลขรั่ว
        if let Some(chunk_data) = world.chunks.get_mut(&chunk_pos) {
            chunk_data.num_vertices = num_vertices;
            chunk_data.num_indices = num_indices;
            chunk_data.num_water_vertices = num_water_vertices;
            chunk_data.num_water_indices = num_water_indices;
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

        update_deco_entities(&mut commands, &mut world, &mut meshes, &deco_material, &mesh_query, chunk_pos, deco, transform);

        update_glow_entities(&mut commands, &mut world, &mut meshes, &mesh_query, &lamp_materials, chunk_pos, glow, transform);
        update_textured_entities(&mut commands, &mut world, &mut meshes, &block_materials, &mesh_query, chunk_pos, textured, transform);
        refresh_chunk_lamp_lights(&mut commands, &mut world, chunk_pos);
        refresh_chunk_campfire_models(&mut commands, &mut world, chunk_pos, &campfire_assets);

        generator.generating_meshes.remove(&chunk_pos);
    }
}

pub fn chunk_unloading_system(
    mut commands: Commands,
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut world: ResMut<VoxelWorld>,
    settings: Res<crate::GameSettings>,
    mut client_sync: Option<ResMut<crate::network::ClientSync>>,
    mut pools: ResMut<ActivePools>,
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
        // chunk มีสระทับอยู่ — เลื่อน unload ออกไปก่อน ให้สระ flush สถานะ
        // สุดท้ายลงบล็อกให้เสร็จ (tick หน้า) แล้วค่อย unload รอบถัดไป
        if pools.mark_dying_overlapping(pos) {
            continue;
        }
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
        if let Some(entities) = world.campfire_models.remove(&pos) {
            for entity in entities {
                commands.entity(entity).despawn();
            }
        }
        // โครงกิ่งของ chunk นี้อยู่ในไฟล์แล้ว — ทิ้งออกจาก memory ไม่งั้น network
        // จะโตตามระยะที่ผู้เล่นเดินสำรวจไปเรื่อยๆ (เก็บสำเนาไว้ก่อนเผื่อ client cache)
        let branch_records = world.branch_network.chunk_records(pos, CHUNK_WIDTH as i32);
        world.branch_network.evict_chunk(pos, CHUNK_WIDTH as i32);

        if let Some(chunk_data) = world.chunks.remove(&pos) {
            world.total_vertices -= chunk_data.num_vertices;
            world.total_indices -= chunk_data.num_indices;

            // network client ห้ามเขียน disk — เก็บ chunk ที่มี edit กลับเข้า cache
            // ใน memory แทน ไม่งั้นเดินไกลแล้วกลับมา edit ของ host หาย
            if let Some(cs) = client_sync.as_mut() {
                if cs.edited.remove(&pos) || cs.full_chunks.contains_key(&pos) {
                    cs.full_chunks.insert(pos, crate::network::ReceivedChunk {
                        blocks: chunk_data.blocks.iter_all().map(|b| b as u8).collect(),
                        chiseled: chunk_data.chiseled_blocks.clone(),
                        facings: chunk_data.facings.clone(),
                        chest_slots: chunk_data.chest_slots.clone(),
                        furnace_slots: chunk_data.furnace_slots.clone(),
                        branches: branch_records,
                    });
                }
            }
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
    pub sub_pos: Option<IVec3>, // (0..15, 0..15, 0..15)
}

/// ผล raycast ของเฟรมนี้ — ให้ระบบอื่น (UI, interaction) อ่านต่อ
#[derive(Resource, Default)]
pub struct TargetedBlock(pub Option<TargetHit>);

// --------------------------------------------------------
// ระบบทุบบล็อก (Survival): กดค้างสะสม progress + รอยแตก 10 stage
// texture รอยแตกผู้ใช้วาดเองที่ assets/textures/breakblock/break1..10.png
// --------------------------------------------------------

/// บล็อกที่กำลังทุบอยู่ + progress 0..1 (Survival เท่านั้น — Creative แตกทันที)
#[derive(Resource, Default)]
pub struct BreakingProgress {
    pub target: Option<(IVec3, f32)>,
    /// นับถอยหลังส่ง Action::Mine ซ้ำระหว่างกดค้าง ให้ remote เห็นแขนแกว่งต่อเนื่อง
    pub action_cooldown: f32,
}

/// entity กล่องรอยแตก (ใบเดียว ครอบบล็อกที่กำลังทุบ) + material 10 stage
#[derive(Resource)]
pub struct BreakOverlay {
    pub entity: Entity,
    pub materials: Vec<Handle<StandardMaterial>>,
}

pub fn setup_break_overlay(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let mats: Vec<Handle<StandardMaterial>> = (1..=10)
        .map(|i| {
            materials.add(StandardMaterial {
                base_color_texture: Some(
                    asset_server.load(format!("textures/breakblock/break{i}.png")),
                ),
                // PNG พื้นโปร่งใส — เห็นเป็นรอยแตกวาดทับ texture บล็อกเดิม
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..Default::default()
            })
        })
        .collect();
    // ใหญ่กว่าบล็อกจริงนิดเดียว กัน z-fight กับหน้าบล็อก
    let entity = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.002, 1.002, 1.002))),
            MeshMaterial3d(mats[0].clone()),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
        ))
        .id();
    commands.insert_resource(BreakOverlay { entity, materials: mats });
}

/// วาง/ซ่อนกล่องรอยแตกตาม BreakingProgress + สลับ stage ตาม progress
pub fn update_break_overlay(
    breaking: Res<BreakingProgress>,
    overlay: Res<BreakOverlay>,
    mut query: Query<(&mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let Ok((mut tf, mut vis, mut mat)) = query.get_mut(overlay.entity) else { return };
    match breaking.target {
        Some((pos, progress)) => {
            tf.translation = pos.as_vec3() + Vec3::splat(0.5);
            let stage = ((progress * 10.0) as usize).min(9);
            if mat.0 != overlay.materials[stage] {
                mat.0 = overlay.materials[stage].clone();
            }
            *vis = Visibility::Visible;
        }
        None => *vis = Visibility::Hidden,
    }
}

/// ออกจากโลก — ล้าง progress ค้างและซ่อนกล่องรอยแตกทันที
/// (update_break_overlay รันเฉพาะ InGame — ปล่อยไว้กล่องค้างโชว์หลังเมนู)
pub fn clear_breaking_on_exit(
    mut breaking: ResMut<BreakingProgress>,
    overlay: Res<BreakOverlay>,
    mut vis_query: Query<&mut Visibility>,
) {
    breaking.target = None;
    if let Ok(mut vis) = vis_query.get_mut(overlay.entity) {
        *vis = Visibility::Hidden;
    }
}

/// บล็อกที่เลือกไว้สำหรับวาง — sync มาจากช่อง hotbar ที่เลือกอยู่
/// (ยังเป็น source of truth ของโค้ดวางบล็อก/network — Air = ช่องว่าง วางไม่ได้)
#[derive(Resource)]
pub struct SelectedBlock(pub BlockType);

impl Default for SelectedBlock {
    fn default() -> Self {
        Self(BlockType::Dirt)
    }
}

// --------------------------------------------------------
// Hotbar — 9 ช่องแบบ Minecraft
// โครงเป็น ItemStack มี count เผื่ออนาคตทำ survival (ตอนนี้ count = None
// คือ creative วางไม่จำกัด) — UI อยู่ ui.rs, ที่นี่คือ state + input
// --------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ItemStack {
    pub item: crate::item::Item,
    /// None = วางไม่จำกัด (creative) — survival ค่อยใส่จำนวนจริงแล้ว render เลขบนช่อง
    pub count: Option<u32>,
}

/// กว้างของกริด = จำนวนช่อง hotbar (ปรับแล้วต้อง rebuild — ไม่ใช่ค่า runtime)
pub const INV_COLS: usize = 9;
/// จำนวนแถวของช่องเก็บของ (ไม่นับแถว hotbar)
pub const INV_ROWS: usize = 3;
pub const HOTBAR_SLOTS: usize = INV_COLS;
pub const INV_SLOTS: usize = INV_COLS * INV_ROWS;
pub const TOTAL_SLOTS: usize = HOTBAR_SLOTS + INV_SLOTS;

/// ที่เก็บของผู้เล่นทั้งหมด — ชื่อยังเป็น Hotbar เพราะเป็นทั้ง state ของแถบล่างจอด้วย
///
/// layout ของ `slots`: **0..HOTBAR_SLOTS = แถบล่างจอ** (เรียงซ้าย→ขวา),
/// **HOTBAR_SLOTS..TOTAL_SLOTS = ช่องเก็บของ** (เรียงซ้าย→ขวา บน→ล่าง)
/// การเรียง hotbar ไว้ก่อนทำให้ระบบที่วนทุกช่อง (เก็บของ) เติมแถบล่างจออัตโนมัติก่อน
#[derive(Resource)]
pub struct Hotbar {
    pub slots: [Option<ItemStack>; TOTAL_SLOTS],
    /// index ช่องที่เลือกอยู่ (0..HOTBAR_SLOTS)
    pub selected: usize,
}

impl Default for Hotbar {
    fn default() -> Self {
        Self::creative()
    }
}

/// จำนวนสูงสุดต่อ stack (survival) — tool ไม่ stack (1 ชิ้น)
pub const MAX_STACK: u32 = 64;

pub fn max_stack(item: crate::item::Item) -> u32 {
    match item {
        crate::item::Item::Tool(_) => 1,
        _ => MAX_STACK,
    }
}

impl Hotbar {
    /// เริ่มด้วย palette เต็ม hotbar (จำนวนจริง = 1 stack) — โหมด Creative
    /// วางบล็อกไม่ลด count (build อิสระ) แต่ทิ้ง Q / เก็บ ปรับจำนวนได้จนหมด/เต็ม
    pub fn creative() -> Self {
        use crate::item::{Item, ToolType};
        const DEFAULTS: [Item; HOTBAR_SLOTS] = [
            Item::Tool(ToolType::Chisel),
            Item::Tool(ToolType::CopperWire),
            Item::Block(BlockType::Dirt),
            Item::Block(BlockType::Stone),
            Item::Block(BlockType::Wood),
            Item::Block(BlockType::Leaves),
            Item::Block(BlockType::Glass),
            Item::Block(BlockType::SmartLamp),
            Item::Block(BlockType::SwitchOff),
        ];
        // ช่องเก็บของเริ่มว่าง — Creative หยิบเพิ่มจาก palette ในหน้าต่าง E ได้ตลอด
        let mut slots = [None; TOTAL_SLOTS];
        for (slot, item) in slots.iter_mut().zip(DEFAULTS) {
            *slot = Some(ItemStack { item, count: Some(max_stack(item)) });
        }
        Self { slots, selected: 0 }
    }

    /// ช่องว่างทั้งหมด — โหมด Survival (เก็บของเอง)
    pub fn survival_empty() -> Self {
        Self { slots: [None; TOTAL_SLOTS], selected: 0 }
    }

    pub fn for_mode(mode: crate::GameMode) -> Self {
        match mode {
            crate::GameMode::Creative => Self::creative(),
            crate::GameMode::Survival => Self::survival_empty(),
        }
    }
}

/// หน้าต่างช่องเก็บของ (กด E) เปิดอยู่ไหม — ตอนเปิด block_interaction หยุดรับคลิก
/// และ ESC จะเป็นการปิดหน้าต่างแทนที่จะเด้ง pause menu
#[derive(Resource, Default)]
pub struct InventoryOpen(pub bool);

/// Chest/Furnace ที่เปิดค้างอยู่ตอนนี้ (คลิกขวามือเปล่าใส่บล็อก) — เปิดพร้อม
/// InventoryOpen เสมอ (ใช้ plumbing เดิมของหน้าต่างช่องเก็บของทั้งหมด: early-return ของ
/// block_interaction_system, ESC ปิดผ่าน pause_menu_system, ล็อค/ปลดล็อคเมาส์)
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct OpenContainerState {
    pub pos: IVec3,
    pub kind: BlockType,
}

#[derive(Resource, Default)]
pub struct OpenContainer(pub Option<OpenContainerState>);

/// ไอเทมทั้งหมดที่เลือกวางได้ (รายการในหน้าต่างกด E)
pub const PLACEABLE_ITEMS: [crate::item::Item; 27] = [
    crate::item::Item::Tool(crate::item::ToolType::Chisel),
    crate::item::Item::Tool(crate::item::ToolType::CopperWire),
    crate::item::Item::Tool(crate::item::ToolType::Pickaxe),
    crate::item::Item::Tool(crate::item::ToolType::Axe),
    crate::item::Item::Tool(crate::item::ToolType::Shovel),
    crate::item::Item::Block(BlockType::Dirt), crate::item::Item::Block(BlockType::Grass),
    crate::item::Item::Block(BlockType::Stone), crate::item::Item::Block(BlockType::Wood),
    crate::item::Item::Block(BlockType::Leaves), crate::item::Item::Block(BlockType::Sand),
    crate::item::Item::Block(BlockType::Water8), crate::item::Item::Block(BlockType::Glowstone),
    crate::item::Item::Block(BlockType::LampRed), crate::item::Item::Block(BlockType::LampGreen),
    crate::item::Item::Block(BlockType::LampBlue), crate::item::Item::Block(BlockType::Glass),
    crate::item::Item::Block(BlockType::TallGrass), crate::item::Item::Block(BlockType::Tnt),
    crate::item::Item::Block(BlockType::IronBlock), crate::item::Item::Block(BlockType::Nuke),
    crate::item::Item::Block(BlockType::SwitchOff), crate::item::Item::Block(BlockType::SmartLamp),
    crate::item::Item::Block(BlockType::Furnace), crate::item::Item::Block(BlockType::Chest),
    crate::item::Item::Block(BlockType::Campfire), crate::item::Item::Block(BlockType::Branch),
];

/// texture ที่ใช้เป็น icon บนช่อง hotbar — เอาหน้าข้างก่อน (grass เห็นเป็น
/// บล็อกหญ้าชัดกว่าหน้าบน) ไม่มีค่อย fallback หน้าบน / สีพื้นใน ui.rs
/// Furnace/Chest: ใช้ variant หน้า (facing_variant ที่ face_id คงที่=2) ให้เห็นหน้าเด่นแทนด้านข้างเฉยๆ
pub fn hotbar_icon_texture(block: BlockType) -> Option<&'static str> {
    match block {
        BlockType::Furnace | BlockType::Chest => {
            face_texture(block, 2, facing_variant(block, 2, 2)).or_else(|| face_texture(block, 0, 0))
        }
        _ => face_texture(block, 2, 0).or_else(|| face_texture(block, 0, 0)),
    }
}

/// สร้างโมเดลของบล็อก (ใช้ทั้งของที่ตกพื้นและฉากลับ render icon) — คิวบ์เล็ก 6 หน้าตรงตาม
/// texture จริงของบล็อกนั้น (ไม่ใช่ texture เดียวทาทั้งก้อน) ยกเว้น Campfire ที่ใช้ glTF scene จริง
/// คืน Entity หลัก (parent) — ผู้เรียกใส่ component เพิ่มเอง (DroppedItem ฯลฯ)
/// `layers`: แปะให้ parent + child ทุกตัวตรงๆ (ไม่พึ่ง inherit) กันฉากลับ render icon ปนกับโลกจริง
pub fn spawn_block_model(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    block_mats: &BlockMaterials,
    campfire_assets: &BlockModelAssets,
    block: BlockType,
    pos: Vec3,
    size: f32,
    layers: bevy::camera::visibility::RenderLayers,
) -> Entity {
    if block == BlockType::Campfire {
        return commands.spawn((
            WorldAssetRoot(campfire_assets.campfire_scene.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(size)),
            layers,
        )).id();
    }

    if block == BlockType::SmartLamp || block == BlockType::SmartLampOn {
        return commands.spawn((
            WorldAssetRoot(campfire_assets.light_bulb_scene.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(size)),
            layers,
        )).id();
    }

    const FACE_OFFSETS_F: [Vec3; 6] = [
        Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, -1.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 0.0, -1.0),
    ];
    let rotations = [
        Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        Quat::IDENTITY,
        Quat::from_rotation_y(std::f32::consts::PI),
    ];

    let parent = commands.spawn((Transform::from_translation(pos), Visibility::default(), layers.clone())).id();
    let half = size / 2.0;
    let quad = meshes.add(Rectangle::new(size, size));
    for face_id in 0..6usize {
        let variant = if matches!(block, BlockType::Furnace | BlockType::Chest) {
            facing_variant(block, face_id, 2)
        } else {
            0
        };
        let material = face_texture(block, face_id, variant)
            .and_then(|path| block_mats.0.get(path).cloned())
            .unwrap_or_else(|| {
                let c = block_color(block);
                materials.add(StandardMaterial {
                    base_color: Color::srgba(c[0], c[1], c[2], c[3]),
                    unlit: true,
                    ..default()
                })
            });
        let child = commands.spawn((
            Mesh3d(quad.clone()),
            MeshMaterial3d(material),
            Transform {
                translation: FACE_OFFSETS_F[face_id] * half,
                rotation: rotations[face_id],
                ..default()
            },
            layers.clone(),
        )).id();
        commands.entity(parent).add_child(child);
    }
    parent
}

/// icon แต่ละบล็อกที่ render เป็นภาพ 3 มิติจริงไว้แล้ว (ต่อ BlockType) — ตั้งครั้งเดียวตอนเกมเริ่ม
/// ไม่มี entry ของ Campfire ตั้งใจ (glTF scene ยังไม่ยืนยันว่า RenderLayers ทะลุเข้าไปในตัว scene
/// ลูกๆ ได้จริงใน Bevy 0.19 — Campfire เลยยังคงใช้ fallback สีพื้นเดิมไปก่อน กันเสี่ยง)
#[derive(Resource, Default)]
pub struct ItemIconCache(pub HashMap<crate::item::Item, Handle<Image>>);

/// entity ของฉากลับ render icon ที่รอ despawn (รอ 2-3 เฟรมให้กล้อง render จริงก่อนถึงจะทิ้งได้ —
/// spawn แล้ว despawn เฟรมเดียวกันจะโดน command buffer ตัดจบก่อนถึง render เลย ไม่ทันได้ render)
#[derive(Resource, Default)]
pub struct IconBakeState {
    cleanup: Vec<Entity>,
    frames_left: u32,
}

/// สร้างฉากลับ + กล้องเรนเดอร์ icon 3 มิติต่อบล็อกใน PLACEABLE_ITEMS (ครั้งเดียว) — ตั้ง
/// ImageIconCache ให้พร้อมใช้ทันที (ตัวรูปจะโผล่เองหลังกล้องเรนเดอร์จริงไม่กี่เฟรม ไม่ต้องรอ)
pub fn start_icon_bake(
    mut done: Local<bool>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut icons: ResMut<ItemIconCache>,
    mut bake_state: ResMut<IconBakeState>,
    block_mats: Res<BlockMaterials>,
    campfire_assets: Res<BlockModelAssets>,
    asset_server: Res<AssetServer>,
) {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

    if *done {
        return;
    }
    *done = true;

    let mut seen: std::collections::HashSet<crate::item::Item> = std::collections::HashSet::new();
    let mut layer: usize = 1; // layer 0 = ฉากเกมจริง เว้นไว้ไม่ใช้กับ icon
    for item in PLACEABLE_ITEMS {
        if !seen.insert(item) {
            continue;
        }

        // เฉพาะบล็อก (ที่ไม่ใช่หญ้าสูง) กับ Pickaxe ที่จะเรนเดอร์ 3D
        let is_pickaxe = matches!(item, crate::item::Item::Tool(crate::item::ToolType::Pickaxe));
        let is_block = match item {
            crate::item::Item::Block(crate::voxel::BlockType::TallGrass) => false,
            crate::item::Item::Block(_) => true,
            _ => false,
        };
        
        if !is_block && !is_pickaxe {
            continue;
        }

        let mut image = Image::new_fill(
            Extent3d { width: 128, height: 128, depth_or_array_layers: 1 },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        image.texture_descriptor.usage =
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
        let image_handle = images.add(image);
        icons.0.insert(item, image_handle.clone());

        let render_layer = bevy::camera::visibility::RenderLayers::layer(layer);
        layer += 1;

        let model = if let crate::item::Item::Block(block) = item {
            spawn_block_model(
                &mut commands, &mut meshes, &mut materials, &block_mats, &campfire_assets,
                block, Vec3::ZERO, 1.0, render_layer.clone(),
            )
        } else {
            use bevy::gltf::GltfAssetLabel;
            use bevy::light::NotShadowCaster;
            let path = crate::item::tool_model_path(crate::item::ToolType::Pickaxe).unwrap();
            let asset = asset_server.load(GltfAssetLabel::Scene(0).from_asset(path));
            commands.spawn((
                WorldAssetRoot(asset),
                Transform::from_translation(Vec3::ZERO).with_scale(Vec3::splat(1.0)),
                render_layer.clone(),
                NotShadowCaster,
            )).id()
        };
        bake_state.cleanup.push(model);

        let light = commands.spawn((
            PointLight { intensity: 200_000.0, range: 10.0, shadow_maps_enabled: false, ..default() },
            Transform::from_xyz(1.5, 2.0, 1.5),
            render_layer.clone(),
        )).id();
        bake_state.cleanup.push(light);

        let camera = commands.spawn((
            Camera3d::default(),
            Camera {
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            bevy::camera::RenderTarget::from(image_handle),
            Transform::from_xyz(1.4, 1.1, 1.4).looking_at(Vec3::ZERO, Vec3::Y),
            render_layer,
        )).id();
        bake_state.cleanup.push(camera);
    }
    bake_state.frames_left = 120; // รอ 2 วินาที (60fps) เพื่อให้ GLTF โหลดและ propagate RenderLayers ทัน
}

/// despawn ฉากลับ/กล้อง render icon ทิ้งหลังรอครบเฟรม (icon ไม่เปลี่ยนตลอดเกม render ครั้งเดียวพอ)
pub fn finish_icon_bake(mut commands: Commands, mut bake_state: ResMut<IconBakeState>) {
    if bake_state.frames_left == 0 {
        return;
    }
    bake_state.frames_left -= 1;
    if bake_state.frames_left == 0 {
        for e in bake_state.cleanup.drain(..) {
            commands.entity(e).despawn();
        }
    }
}

pub fn propagate_render_layers(
    mut commands: Commands,
    q_parents: Query<&bevy::camera::visibility::RenderLayers>,
    q_children: Query<(Entity, &ChildOf), Without<bevy::camera::visibility::RenderLayers>>,
) {
    for (entity, parent) in q_children.iter() {
        if let Ok(layers) = q_parents.get(parent.0) {
            commands.entity(entity).insert(layers.clone());
        }
    }
}

/// input ของ hotbar: 1-9 เลือกช่อง, scroll เลื่อนช่อง (วนรอบ), คลิกกลาง pick block
/// จบด้วย sync บล็อกของช่องที่เลือกลง SelectedBlock ให้ระบบวางบล็อกใช้ต่อ
pub fn hotbar_input_system(
    mut hotbar: ResMut<Hotbar>,
    settings: Res<crate::GameSettings>,
    mut selected: ResMut<SelectedBlock>,
    mut interaction_mode: ResMut<InteractionMode>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    target: Res<TargetedBlock>,
    mut q_egui: Query<&mut bevy_egui::EguiContext, With<bevy::window::PrimaryWindow>>,
    mut spawn_events: MessageWriter<crate::item::SpawnDroppedItemEvent>,
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
) {
    const SLOT_KEYS: [KeyCode; 9] = [
        KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3,
        KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6,
        KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
    ];
    for (i, key) in SLOT_KEYS.iter().enumerate() {
        if keyboard.just_pressed(*key) {
            hotbar.selected = i;
        }
    }

    // เมาส์อยู่บน egui = กำลังใช้เมนู — scroll/คลิกกลางเป็นของเมนู ไม่ใช่ hotbar
    let over_egui = q_egui.iter_mut().next().map_or(false, |mut ctx| {
        ctx.get_mut().egui_wants_pointer_input() || ctx.get_mut().is_pointer_over_egui()
    });

    let mut scroll = 0.0f32;
    for ev in wheel.read() {
        scroll += ev.y;
    }
    if scroll != 0.0 && !over_egui {
        let dir = if scroll < 0.0 { 1 } else { -1 }; // scroll ลง = ช่องถัดไปทางขวา
        hotbar.selected = (hotbar.selected as i32 + dir).rem_euclid(HOTBAR_SLOTS as i32) as usize;
    }

    // pick block: มีในแถบอยู่แล้วก็เลือกช่องนั้น ไม่งั้นใส่ทับช่องปัจจุบัน (แบบ Minecraft)
    if mouse.just_pressed(MouseButton::Middle) && !over_egui {
        if let Some(hit) = target.0 {
            // น้ำระดับไหนก็ตาม pick ได้เป็นน้ำเต็มบล็อก
            let block = if hit.block.is_water() { BlockType::Water8 } else { hit.block };
            if block != BlockType::Air {
                // ค้นเฉพาะแถบล่างจอ — pick ต้องได้ช่องที่ "เลือกได้" ไม่ใช่ช่องในกระเป๋า
                if let Some(i) = hotbar.slots[..HOTBAR_SLOTS].iter().position(|s| s.map(|s| s.item) == Some(crate::item::Item::Block(block))) {
                    hotbar.selected = i;
                } else if settings.game_mode == crate::GameMode::Creative {
                    // Creative เท่านั้น summon บล็อกใหม่เข้าช่องได้ (Survival ต้องหาเอง)
                    let sel = hotbar.selected;
                    let it = crate::item::Item::Block(block);
                    hotbar.slots[sel] = Some(ItemStack { item: it, count: Some(max_stack(it)) });
                }
            }
        }
    }

    // กด Q เพื่อทิ้งไอเทมจากมือ
    if keyboard.just_pressed(KeyCode::KeyQ) && !over_egui {
        let sel = hotbar.selected;
        if let Some(stack) = hotbar.slots[sel] {
            if let Some(cam_tf) = camera_query.iter().next() {
                let forward = cam_tf.forward();
                let spawn_pos = cam_tf.translation + forward.normalize() * 0.5 - Vec3::Y * 0.2;
                let velocity = forward.normalize() * 5.0 + Vec3::Y * 3.0; // พุ่งไปข้างหน้า + เด้งขึ้น
                spawn_events.write(crate::item::SpawnDroppedItemEvent {
                    item: stack.item,
                    pos: spawn_pos,
                    velocity,
                });
            }
            
            // หักของออกจากช่อง: count None = Creative ∞ (คงช่องไว้ ทิ้งได้เรื่อยๆ),
            // Some(c) = Survival (ลด 1, หมดแล้วช่องว่าง)
            if let Some(c) = stack.count {
                if c > 1 {
                    hotbar.slots[sel].as_mut().unwrap().count = Some(c - 1);
                } else {
                    hotbar.slots[sel] = None;
                }
            }
        }
    }

    let item = hotbar.slots[hotbar.selected].map(|s| s.item);
    let block = match item {
        Some(crate::item::Item::Block(b)) => b,
        _ => BlockType::Air,
    };
    if selected.0 != block {
        selected.0 = block;
    }

    let new_mode = match item {
        Some(crate::item::Item::Tool(crate::item::ToolType::Chisel)) => InteractionMode::SubVoxel,
        Some(crate::item::Item::Tool(crate::item::ToolType::CopperWire)) => InteractionMode::Wiring,
        _ => InteractionMode::Normal,
    };
    if *interaction_mode != new_mode {
        *interaction_mode = new_mode;
    }
}

pub fn voxel_raycast_system(
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    world: Res<VoxelWorld>,
    mut target: ResMut<TargetedBlock>,
    interaction_mode: Res<InteractionMode>,
    mut gizmos: Gizmos,
) {
    target.0 = None;

    let Some(camera_transform) = camera_query.iter().next() else { return };
    let origin = camera_transform.translation;
    let dir = camera_transform.forward().normalize();

    let max_dist = 6.0;

    if *interaction_mode == InteractionMode::SubVoxel || *interaction_mode == InteractionMode::Wiring {
        let max_steps = 600;
        let step = 0.01;
        let mut prev_macro = IVec3::new(origin.x.floor() as i32, origin.y.floor() as i32, origin.z.floor() as i32);
        let mut prev_sub = IVec3::new(
            ((origin.x - prev_macro.x as f32) * 16.0).floor().clamp(0.0, 15.0) as i32,
            ((origin.y - prev_macro.y as f32) * 16.0).floor().clamp(0.0, 15.0) as i32,
            ((origin.z - prev_macro.z as f32) * 16.0).floor().clamp(0.0, 15.0) as i32,
        );
        
        for i in 0..max_steps {
            let t = i as f32 * step;
            let p = origin + dir * t;
            
            let mx = p.x.floor() as i32;
            let my = p.y.floor() as i32;
            let mz = p.z.floor() as i32;
            let m_pos = IVec3::new(mx, my, mz);
            
            let block = world.get_block(mx, my, mz);
            
            if block != BlockType::Air
                && block != BlockType::TallGrass
                && !block.is_water()
            {
                let sx = ((p.x - mx as f32) * 16.0).floor().clamp(0.0, 15.0) as i32;
                let sy = ((p.y - my as f32) * 16.0).floor().clamp(0.0, 15.0) as i32;
                let sz = ((p.z - mz as f32) * 16.0).floor().clamp(0.0, 15.0) as i32;
                let s_pos = IVec3::new(sx, sy, sz);

                let is_solid = if block == BlockType::Chiseled {
                    world.get_chiseled_sub_voxel(mx, my, mz, sx as usize, sy as usize, sz as usize) > 0
                } else {
                    true
                };

                if is_solid {
                    let mut normal = IVec3::ZERO;
                    let dx = (mx * 16 + sx) - (prev_macro.x * 16 + prev_sub.x);
                    let dy = (my * 16 + sy) - (prev_macro.y * 16 + prev_sub.y);
                    let dz = (mz * 16 + sz) - (prev_macro.z * 16 + prev_sub.z);
                    
                    if dx != 0 { normal.x = -dx.signum(); }
                    else if dy != 0 { normal.y = -dy.signum(); }
                    else if dz != 0 { normal.z = -dz.signum(); }
                    else { normal.y = 1; }
                    
                    target.0 = Some(TargetHit {
                        pos: m_pos,
                        normal,
                        block,
                        sub_pos: Some(s_pos),
                    });

                    if *interaction_mode == InteractionMode::SubVoxel {
                        let min = Vec3::new(
                            mx as f32 + sx as f32 / 16.0,
                            my as f32 + sy as f32 / 16.0,
                            mz as f32 + sz as f32 / 16.0,
                        );
                        let max = min + Vec3::splat(1.0 / 16.0);
                        gizmos.cube(Transform::from_translation((min + max) * 0.5).with_scale(max - min), Color::BLACK);
                    }
                    
                    return;
                }
                
                prev_macro = m_pos;
                prev_sub = s_pos;
            } else {
                let sx = ((p.x - mx as f32) * 16.0).floor().clamp(0.0, 15.0) as i32;
                let sy = ((p.y - my as f32) * 16.0).floor().clamp(0.0, 15.0) as i32;
                let sz = ((p.z - mz as f32) * 16.0).floor().clamp(0.0, 15.0) as i32;
                prev_macro = m_pos;
                prev_sub = IVec3::new(sx, sy, sz);
            }
        }
        return;
    }

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
        if block != BlockType::Air && !block.is_water() {
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
        sub_pos: None,
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

// --------------------------------------------------------
// Shared edit/remesh helpers (ใช้ร่วมกันทั้ง block edit, fluid และ network)
// --------------------------------------------------------

/// มัดรวม resource ที่ต้องใช้ตอน remesh chunk แบบ synchronous
#[derive(bevy::ecs::system::SystemParam)]
pub struct MeshingParams<'w, 's> {
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub mesh_query: Query<'w, 's, &'static Mesh3d>,
    pub chunk_material: Res<'w, ChunkMaterial>,
    pub water_material: Res<'w, WaterMaterial>,
    pub glass_material: Res<'w, GlassMaterial>,
    pub deco_material: Res<'w, DecoMaterials>,
    pub lamp_materials: Res<'w, LampMaterials>,
    pub block_materials: Res<'w, BlockMaterials>,
}

/// ระยะที่ใบยังเกาะกิ่งอยู่ได้ (Chebyshev) — กว้างกว่ารัศมีพุ่มที่ scatter_leaves โปรย
/// ไว้เล็กน้อย ใบที่อยู่ในระยะนี้จากกิ่งใดก็ตามถือว่ายังมีที่ยึด
const LEAF_SUPPORT_RANGE: i32 = 3;

/// กิ่งที่ `p` หายไป — จ่อใบรอบๆ ไว้ให้ไปเช็คว่ายังมีกิ่งอื่นค้ำอยู่ไหม
pub fn queue_leaf_decay_around(world: &mut VoxelWorld, p: IVec3) {
    for d in crate::tree::NEIGHBOUR_DIRS {
        let n = p + d;
        if world.get_block(n.x, n.y, n.z) == BlockType::Leaves {
            world.pending_leaf_decay.insert(n);
        }
    }
}

/// ยังมีบล็อกกิ่งอยู่ในระยะเกาะของใบที่ `p` ไหม
fn leaf_has_support(world: &VoxelWorld, p: IVec3) -> bool {
    let r = LEAF_SUPPORT_RANGE;
    for dy in -r..=r {
        for dz in -r..=r {
            for dx in -r..=r {
                let q = p + IVec3::new(dx, dy, dz);
                if world.get_block(q.x, q.y, q.z) == BlockType::Branch {
                    return true;
                }
            }
        }
    }
    false
}

/// ผูก node ให้กิ่งที่เพิ่งเกิดที่ `p` — เลือก parent เป็นเพื่อนบ้านที่ thickness มากสุด
/// (ไล่ตามลำดับทิศคงที่เพื่อ tie-break ให้ host กับ client ได้ผลเดียวกันเสมอ)
pub fn attach_branch_node(world: &mut VoxelWorld, p: IVec3) {
    // node ค้างจาก desync/เซฟเก่า — ถอดทิ้งก่อน ไม่งั้นกิ่งใหม่จะสืบ parent/children ของเดิม
    if world.branch_network.nodes.contains_key(&p) {
        let orphans = world.branch_network.detach(p);
        world.pending_branch_orphans.extend(orphans);
    }

    let below = world.get_block(p.x, p.y - 1, p.z);
    if below == BlockType::Dirt || below == BlockType::Grass {
        world.branch_network.add_root(p, crate::tree::TRUNK_THICKNESS);
        return;
    }

    let mut parent: Option<(IVec3, u8)> = None;
    for dir in crate::tree::NEIGHBOUR_DIRS {
        let adj = p + dir;
        if world.get_block(adj.x, adj.y, adj.z) != BlockType::Branch {
            continue;
        }
        let Some(t) = world.branch_network.thickness_at(adj) else { continue };
        if parent.is_none_or(|(_, best)| t > best) {
            parent = Some((adj, t));
        }
    }

    match parent {
        Some((parent_pos, _)) => world.branch_network.add_branch(p, parent_pos),
        // ลอยเดี่ยวไม่ติดอะไรเลย — เป็น root ผอม ไม่ใช่ลำต้นอ้วนเหมือนเดิม
        None => world.branch_network.add_root(p, crate::tree::LOOSE_THICKNESS),
    }
}

/// จุด apply การแก้บล็อกจุดเดียว ใช้ทั้ง input ในเครื่องและ edit ที่มาจาก network
/// คืนตำแหน่งที่แก้สำเร็จ (None = chunk ยังไม่โหลด / นอกขอบเขต / ไม่มีอะไรให้แก้)
pub fn apply_block_edit(world: &mut VoxelWorld, edit: &crate::network::BlockEdit) -> Option<IVec3> {
    use crate::network::BlockEdit;
    match edit {
        BlockEdit::SetBlock { pos, block } => {
            let [x, y, z] = *pos;
            let new_block = BlockType::from_u8(*block);
            let old_block = world.get_block(x, y, z);
            // เขียนทับ Furnace/Chest ด้วยบล็อกอื่น (รวมทุบเป็น Air) — กัน facing/container ค้างใน map
            if new_block != old_block {
                world.clear_container_and_facing(x, y, z);
            }
            if world.set_block(x, y, z, new_block) {
                let p = IVec3::new(x, y, z);
                // บล็อกเปลี่ยน = แสงรอบๆ เปลี่ยน (เปิดช่องให้แดดลง/ปิดกั้นแสง)
                // — chunk ตัวเองบวกเพื่อนบ้านฝั่งที่ติดขอบ
                for cp in edit_affected_chunks(p) {
                    if let Some(c) = world.chunks.get_mut(&cp) {
                        c.light_dirty = true;
                    }
                }
                // node ของกิ่งเกิด/ตายที่นี่ที่เดียว — path นี้รันทั้ง host และ client
                // จาก edit ก้อนเดียวกัน สถานะ network สองฝั่งจึงตรงกันเสมอ
                // (set_block คืน true แค่บอกว่า chunk โหลดอยู่ ไม่ได้แปลว่าบล็อกเปลี่ยน —
                //  ต้องเทียบ old/new เอง ไม่งั้น edit ซ้ำจะ re-parent กิ่งเดิมแล้วกิ่งลูกร่วงฟรี)
                if new_block != old_block {
                    if new_block == BlockType::Branch {
                        attach_branch_node(world, p);
                    } else if old_block == BlockType::Branch {
                        let orphans = world.branch_network.detach(p);
                        world.pending_branch_orphans.extend(orphans);
                        queue_leaf_decay_around(world, p);
                    }
                }
                Some(p)
            } else {
                None
            }
        }
        BlockEdit::PlaceFacingBlock { pos, block, facing } => {
            let [x, y, z] = *pos;
            let bt = BlockType::from_u8(*block);
            if world.set_block(x, y, z, bt) {
                world.set_block_facing(x, y, z, *facing);
                Some(IVec3::new(x, y, z))
            } else {
                None
            }
        }
        BlockEdit::SetSubVoxel { pos, sub, val } => {
            let [x, y, z] = *pos;
            let current = world.get_block(x, y, z);
            if current != BlockType::Chiseled {
                if current == BlockType::Air {
                    // ทุบ sub-voxel ในอากาศ = ไม่มีอะไรให้ทำ (กัน desync สร้าง chiseled เปล่า)
                    if *val == 0 || !world.set_block(x, y, z, BlockType::Chiseled) {
                        return None;
                    }
                } else {
                    world.convert_to_chiseled(x, y, z);
                }
            }
            world.set_chiseled_sub_voxel(x, y, z, sub[0] as usize, sub[1] as usize, sub[2] as usize, *val);
            Some(IVec3::new(x, y, z))
        }
        BlockEdit::SetContainerSlot { pos, slot, item } => {
            let [x, y, z] = *pos;
            let stack = item.and_then(|w| w.to_stack());
            match world.get_block(x, y, z) {
                BlockType::Chest if (*slot as usize) < 27 => {
                    world.set_chest_slot(x, y, z, *slot as usize, stack);
                    Some(IVec3::new(x, y, z))
                }
                BlockType::Furnace if (*slot as usize) < 3 => {
                    world.set_furnace_slot(x, y, z, *slot as usize, stack);
                    Some(IVec3::new(x, y, z))
                }
                _ => None,
            }
        }
    }
}

/// chunk ที่โดนผลจากการแก้บล็อกที่ tp: ตัวเอง + เพื่อนบ้านถ้าแก้ตรงขอบ/มุม
/// (AO ของ chunk ข้างเคียงขึ้นกับบล็อกริมขอบ)
pub fn edit_affected_chunks(tp: IVec3) -> Vec<IVec2> {
    let edited_chunk = IVec2::new(
        tp.x.div_euclid(CHUNK_WIDTH as i32),
        tp.z.div_euclid(CHUNK_WIDTH as i32),
    );
    let mut chunks = vec![edited_chunk];
    let local_x = tp.x.rem_euclid(CHUNK_WIDTH as i32);
    let local_z = tp.z.rem_euclid(CHUNK_WIDTH as i32);
    let (cx, cz) = (edited_chunk.x, edited_chunk.y);

    let at_min_x = local_x == 0;
    let at_max_x = local_x == (CHUNK_WIDTH - 1) as i32;
    let at_min_z = local_z == 0;
    let at_max_z = local_z == (CHUNK_WIDTH - 1) as i32;

    if at_min_x { chunks.push(IVec2::new(cx - 1, cz)); }
    if at_max_x { chunks.push(IVec2::new(cx + 1, cz)); }
    if at_min_z { chunks.push(IVec2::new(cx, cz - 1)); }
    if at_max_z { chunks.push(IVec2::new(cx, cz + 1)); }

    if at_min_x && at_min_z { chunks.push(IVec2::new(cx - 1, cz - 1)); }
    if at_min_x && at_max_z { chunks.push(IVec2::new(cx - 1, cz + 1)); }
    if at_max_x && at_min_z { chunks.push(IVec2::new(cx + 1, cz - 1)); }
    if at_max_x && at_max_z { chunks.push(IVec2::new(cx + 1, cz + 1)); }

    chunks
}

/// remesh chunk แบบ synchronous (สลับ mesh asset ในที่เดิม ลดการ alloc)
/// คืนรายการ chunk ที่ยังทำไม่ได้เพราะเพื่อนบ้านยังไม่โหลด — ผู้เรียกตัดสินใจเองว่าจะ requeue ไหม
pub fn remesh_chunks(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    mp: &mut MeshingParams,
    chunk_positions: impl IntoIterator<Item = IVec2>,
) -> Vec<IVec2> {
    let mut skipped = Vec::new();
    for chunk_pos in chunk_positions {
        let neighbors_pos = chunk_neighbors(chunk_pos);
        if !neighbors_pos.iter().all(|p| world.chunks.contains_key(p)) {
            skipped.push(chunk_pos);
            continue;
        }
        let neighbors = neighbors_pos.map(|p| world.chunks.get(&p).unwrap().blocks.clone());
        // แสงต้องสดก่อน mesh — ทางนี้เป็น path แบบ sync (ตอนแก้บล็อก) จึงคำนวณตรงนี้เลย
        // ไม่ต้องรอ relight_system รอบหน้า ไม่งั้นบล็อกที่เพิ่งทุบจะสว่างช้าไปหนึ่งเฟรม
        // — ต้องคลุมเพื่อนบ้านด้วย เพราะ smooth lighting อ่านแสงข้ามขอบ
        ensure_chunk_light(world, chunk_pos);
        for n in neighbors_pos {
            ensure_chunk_light(world, n);
        }
        let light = light_neighborhood(world, chunk_pos);

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

            let s = create_mesh_from_blocks(chunk_pos, &chunk_data.blocks, &neighbors, Some(&chunk_data.chiseled_blocks), Some(&chunk_data.facings), Some(&world.branch_network), light.as_ref());
            chunk_data.num_vertices = s.total_vertices();
            chunk_data.num_indices = s.total_indices();
            chunk_data.num_water_vertices = s.water.positions.len();
            chunk_data.num_water_indices = s.water.indices.len();
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
            } else if let Ok(mesh3d) = mp.mesh_query.get(entity) {
                let _ = mp.meshes.insert(mesh3d.0.id(), solid.into_mesh());
                commands.entity(entity).remove::<Aabb>();
            } else {
                commands.entity(entity)
                    .insert((
                        Mesh3d(mp.meshes.add(solid.into_mesh())),
                        MeshMaterial3d(mp.chunk_material.0.clone()),
                    ))
                    .remove::<Aabb>();
            }
        }

        // น้ำ/กระจก/ของประดับ: สร้าง/เขียนทับ/ลบ ตามว่าเหลือหน้าไหม
        update_single_mesh_entity(commands, &mut world.water_chunks, &mut mp.meshes, &mp.mesh_query, &mp.water_material.0, chunk_pos, water, transform);
        update_single_mesh_entity(commands, &mut world.glass_chunks, &mut mp.meshes, &mp.mesh_query, &mp.glass_material.0, chunk_pos, glass, transform);
        update_deco_entities(commands, world, &mut mp.meshes, &mp.deco_material, &mp.mesh_query, chunk_pos, deco, transform);
        update_glow_entities(commands, world, &mut mp.meshes, &mp.mesh_query, &mp.lamp_materials, chunk_pos, glow, transform);
        update_textured_entities(commands, world, &mut mp.meshes, &mp.block_materials, &mp.mesh_query, chunk_pos, textured, transform);
    }
    skipped
}

/// remesh เฉพาะชั้นน้ำ (สลับ asset ในที่เดิม) — ถูกต้องเฉพาะเมื่อการเปลี่ยนแปลง
/// ทั้งหมดตั้งแต่ mesh ล่าสุดเป็น Air↔WaterN / WaterN↔WaterM เท่านั้น
/// (น้ำไม่ occlude AO และ visibility ของ solid มอง Air/น้ำเหมือนกัน —
/// ชั้นอื่นจึงไม่เปลี่ยนแม้แต่ byte เดียว มี parity test คุม)
/// fluid sim การันตีเงื่อนไขนี้เพราะเขียนบล็อกผ่าน vol_to_block เท่านั้น
/// คืนรายการ chunk ที่ remesh ไม่ได้เพราะเพื่อนบ้านยังไม่โหลด
pub fn remesh_water_only(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    mp: &mut MeshingParams,
    chunk_positions: impl IntoIterator<Item = IVec2>,
) -> Vec<IVec2> {
    let mut skipped = Vec::new();
    for chunk_pos in chunk_positions {
        let neighbors_pos = chunk_neighbors(chunk_pos);
        if !neighbors_pos.iter().all(|p| world.chunks.contains_key(p)) {
            skipped.push(chunk_pos);
            continue;
        }
        let neighbors = neighbors_pos.map(|p| world.chunks.get(&p).unwrap().blocks.clone());

        let transform = Transform::from_xyz(
            (chunk_pos.x * CHUNK_WIDTH as i32) as f32,
            0.0,
            (chunk_pos.y * CHUNK_WIDTH as i32) as f32,
        );

        let old_water_v;
        let old_water_i;
        let buf;
        {
            let Some(chunk_data) = world.chunks.get_mut(&chunk_pos) else { continue };
            // ไม่มีน้ำและ mesh น้ำก็ว่างอยู่แล้ว → ไม่มีอะไรให้ทำ
            if chunk_data.water_y_min > chunk_data.water_y_max && chunk_data.num_water_vertices == 0 {
                continue;
            }
            let (b, observed) = create_water_mesh(
                chunk_pos,
                &chunk_data.blocks,
                &neighbors,
                chunk_data.water_y_min,
                chunk_data.water_y_max,
            );
            // tighten แถบ y ตามน้ำที่เจอจริง (band เป็น grow-only ระหว่าง rebuild)
            match observed {
                Some((lo, hi)) => {
                    chunk_data.water_y_min = lo;
                    chunk_data.water_y_max = hi;
                }
                None => {
                    chunk_data.water_y_min = CHUNK_HEIGHT;
                    chunk_data.water_y_max = 0;
                }
            }
            old_water_v = chunk_data.num_water_vertices;
            old_water_i = chunk_data.num_water_indices;
            let nv = b.positions.len();
            let ni = b.indices.len();
            chunk_data.num_vertices = (chunk_data.num_vertices + nv) - old_water_v;
            chunk_data.num_indices = (chunk_data.num_indices + ni) - old_water_i;
            chunk_data.num_water_vertices = nv;
            chunk_data.num_water_indices = ni;
            buf = b;
        }
        world.total_vertices = (world.total_vertices + buf.positions.len()) - old_water_v;
        world.total_indices = (world.total_indices + buf.indices.len()) - old_water_i;

        update_single_mesh_entity(
            commands,
            &mut world.water_chunks,
            &mut mp.meshes,
            &mp.mesh_query,
            &mp.water_material.0,
            chunk_pos,
            buf,
            transform,
        );
    }
    skipped
}

pub fn block_interaction_system(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    target: Res<TargetedBlock>,
    selected: Res<SelectedBlock>,
    (mut inventory, mut open_container): (ResMut<InventoryOpen>, ResMut<OpenContainer>),
    interaction_mode: Res<InteractionMode>,
    (mouse_input, _keyboard): (Res<ButtonInput<MouseButton>>, Res<ButtonInput<KeyCode>>),
    mut mp: MeshingParams,
    (camera_query, mut cursor_query, mut q_egui): (
        Query<&Transform, With<crate::camera::FreeCamera>>,
        Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
        Query<&mut bevy_egui::EguiContext, With<bevy::window::PrimaryWindow>>,
    ),
    mut active_fluids: ResMut<ActiveFluids>,
    (net_server, net_client, mut net_out, mut local_actions): (
        Option<Res<bevy_renet::RenetServer>>,
        Option<Res<bevy_renet::RenetClient>>,
        ResMut<crate::network::PendingNetEdits>,
        ResMut<crate::network::PendingLocalActions>,
    ),
    mut pools: ResMut<ActivePools>,
    mut fx_writer: MessageWriter<crate::particles::BlockFx>,
    (settings, mut active_tnt, mut spawn_events, mut hotbar): (Res<crate::GameSettings>, ResMut<ActiveTnt>, MessageWriter<crate::item::SpawnDroppedItemEvent>, ResMut<Hotbar>),
    campfire_assets: Res<BlockModelAssets>,
    (time, mut breaking, mut block_updates): (Res<Time>, ResMut<BreakingProgress>, ResMut<PendingBlockUpdates>),
) {
    let survival = settings.game_mode == crate::GameMode::Survival;
    // หน้าต่างช่องเก็บของเปิดอยู่ — คลิกเป็นของหน้าต่าง ไม่ใช่การขุด/วาง
    if inventory.0 {
        breaking.target = None;
        return;
    }

    let Some(hit) = target.0 else {
        breaking.target = None;
        return;
    };

    let break_pressed = mouse_input.just_pressed(MouseButton::Left);
    let break_held = mouse_input.pressed(MouseButton::Left);
    let place_pressed = mouse_input.just_pressed(MouseButton::Right);
    // Survival โหมดปกติ = ทุบแบบกดค้างมี progress — Creative/Chisel/Wiring แตกทันทีเหมือนเดิม
    let hold_mining = survival && *interaction_mode == InteractionMode::Normal && break_held;
    if !hold_mining {
        breaking.target = None; // ปล่อยปุ่ม/สลับโหมด — progress หาย
    }
    if !break_pressed && !place_pressed && !hold_mining {
        return;
    }

    if hold_mining {
        // ท่าขุดส่งซ้ำเป็นจังหวะตลอดที่กดค้าง (ฝั่งรับตั้ง mining_timer 0.5s ต่อครั้ง
        // — 0.3s ทำให้แขน remote แกว่งต่อเนื่องไม่สะดุด)
        breaking.action_cooldown -= time.delta_secs();
        if break_pressed || breaking.action_cooldown <= 0.0 {
            local_actions.0.push(0); // 0 = Action::Mine
            breaking.action_cooldown = 0.3;
        }
    } else if break_pressed {
        local_actions.0.push(0); // 0 = Action::Mine
    }

    // คลิกบน egui = ใช้เมนูอยู่ ไม่ใช่เล่นเกม
    if let Some(mut egui_ctx) = q_egui.iter_mut().next() {
        if egui_ctx.get_mut().egui_wants_pointer_input() || egui_ctx.get_mut().is_pointer_over_egui() {
            breaking.target = None;
            return;
        }
    }

    use crate::network::BlockEdit;
    let mut edit: Option<BlockEdit> = None;
    // particle ของ edit นี้ (เก็บ block เก่าก่อน apply) — เฉพาะโหมด Normal
    let mut fx: Option<crate::particles::BlockFx> = None;

    if *interaction_mode == InteractionMode::SubVoxel {
        if let Some(sub_pos) = hit.sub_pos {
            if break_pressed {
                edit = Some(BlockEdit::SetSubVoxel {
                    pos: hit.pos.to_array(),
                    sub: [sub_pos.x as u8, sub_pos.y as u8, sub_pos.z as u8],
                    val: 0,
                });
            } else if place_pressed && selected.0 != BlockType::Air {
                let adj_sub = sub_pos + hit.normal;
                let (mut target_macro, mut target_sub) = (hit.pos, adj_sub);

                if target_sub.x < 0 { target_macro.x -= 1; target_sub.x = 15; }
                else if target_sub.x > 15 { target_macro.x += 1; target_sub.x = 0; }

                if target_sub.y < 0 { target_macro.y -= 1; target_sub.y = 15; }
                else if target_sub.y > 15 { target_macro.y += 1; target_sub.y = 0; }

                if target_sub.z < 0 { target_macro.z -= 1; target_sub.z = 15; }
                else if target_sub.z > 15 { target_macro.z += 1; target_sub.z = 0; }

                edit = Some(BlockEdit::SetSubVoxel {
                    pos: target_macro.to_array(),
                    sub: [target_sub.x as u8, target_sub.y as u8, target_sub.z as u8],
                    val: selected.0 as u8,
                });
            }
        }
    } else {
        if place_pressed && matches!(hit.block, BlockType::Tnt | BlockType::Nuke) {
            // คลิกขวาบล็อกระเบิด = จุดชนวน (ไม่ใช่วางบล็อก) — sync เป็น SetBlock ปกติ
            // fuse นับฝั่ง host/single เท่านั้น (client ส่ง edit ไป host เห็นแล้วนับเอง)
            let (lit, fuse) = if hit.block == BlockType::Tnt {
                (BlockType::TntLit, settings.tnt_fuse_seconds)
            } else {
                (BlockType::NukeLit, settings.nuke_fuse_seconds)
            };
            edit = Some(BlockEdit::SetBlock {
                pos: hit.pos.to_array(),
                block: lit as u8,
            });
            if net_client.is_none() {
                active_tnt.0.insert(hit.pos, Timer::from_seconds(fuse, TimerMode::Once));
            }
        } else if break_pressed || hold_mining {
            // ของที่ถืออยู่ — ใช้ทั้งคิดความเร็วขุดและกติกา drop (Survival)
            let held_tool = match hotbar.slots[hotbar.selected].map(|s| s.item) {
                Some(crate::item::Item::Tool(t)) => Some(t),
                _ => None,
            };
            // Survival: กดค้างสะสม progress ตามเวลาขุด ครบ 1.0 ค่อยแตกจริง
            // Creative: แตกทันทีเหมือนเดิม (done = true เลย)
            let done = if hold_mining {
                let total = break_time(hit.block, held_tool).max(0.05);
                let mut progress = match breaking.target {
                    Some((pos, p)) if pos == hit.pos => p,
                    _ => 0.0, // เพิ่งเริ่ม/เล็งบล็อกใหม่ — เริ่มนับศูนย์
                };
                progress += time.delta_secs() / total;
                if progress >= 1.0 {
                    breaking.target = None;
                    true
                } else {
                    breaking.target = Some((hit.pos, progress));
                    false
                }
            } else {
                true
            };

            if done {
            // เก็บของใน container ไว้ก่อน apply_block_edit ล้าง (clear_container_and_facing)
            // ดรอปเสมอทั้ง Creative/Survival — ต่างจากตัวบล็อกที่ดรอปเฉพาะ Survival เพราะ
            // ของที่เก็บไว้เป็นของผู้เล่นจริง ไม่ใช่บล็อกที่ build อิสระได้
            let mut container_drops: Vec<crate::item::Item> = Vec::new();
            match hit.block {
                BlockType::Chest => {
                    if let Some(slots) = world.get_chest_slots(hit.pos.x, hit.pos.y, hit.pos.z) {
                        container_drops.extend(slots.iter().filter_map(|s| s.map(|s| s.item)));
                    }
                }
                BlockType::Furnace => {
                    if let Some(slots) = world.get_furnace_slots(hit.pos.x, hit.pos.y, hit.pos.z) {
                        container_drops.extend(slots.iter().filter_map(|s| s.map(|s| s.item)));
                    }
                }
                _ => {}
            }

            edit = Some(BlockEdit::SetBlock {
                pos: hit.pos.to_array(),
                block: BlockType::Air as u8,
            });
            fx = Some(crate::particles::BlockFx {
                pos: hit.pos,
                placed: BlockType::Air,
                replaced: hit.block,
            });

            // ดรอปไอเทม (เฉพาะ Survival) — บล็อกหมวดหิน/แร่ต้องถือ pickaxe ตอนแตก
            // ถึงได้ของ (กติกา Minecraft) มือเปล่า/tool ผิดหมวด = บล็อกหายเปล่า
            let drops_item = !block_requires_tool(hit.block)
                || held_tool.is_some_and(|t| t.dig_class() == block_dig_class(hit.block));
            if survival && drops_item {
                spawn_events.write(crate::item::SpawnDroppedItemEvent {
                    item: crate::item::Item::Block(hit.block),
                    pos: hit.pos.as_vec3() + Vec3::new(0.5, 0.5, 0.5),
                    velocity: Vec3::new(
                        (fastrand::f32() - 0.5) * 4.0,
                        2.0 + fastrand::f32() * 3.0,
                        (fastrand::f32() - 0.5) * 4.0,
                    ),
                });
            }
            for item in container_drops {
                spawn_events.write(crate::item::SpawnDroppedItemEvent {
                    item,
                    pos: hit.pos.as_vec3() + Vec3::new(0.5, 0.5, 0.5),
                    velocity: Vec3::new(
                        (fastrand::f32() - 0.5) * 4.0,
                        2.0 + fastrand::f32() * 3.0,
                        (fastrand::f32() - 0.5) * 4.0,
                    ),
                });
            }
            }
        } else if place_pressed && selected.0 == BlockType::Air {
            // Interact! (กดคลิกขวาด้วยมือเปล่า)
            let current = world.get_block(hit.pos.x, hit.pos.y, hit.pos.z);
            if current == BlockType::SwitchOff {
                edit = Some(BlockEdit::SetBlock {
                    pos: hit.pos.to_array(),
                    block: BlockType::SwitchOn as u8,
                });
                fx = Some(crate::particles::BlockFx {
                    pos: hit.pos,
                    placed: BlockType::SwitchOn,
                    replaced: BlockType::SwitchOff,
                });
            } else if current == BlockType::SwitchOn {
                edit = Some(BlockEdit::SetBlock {
                    pos: hit.pos.to_array(),
                    block: BlockType::SwitchOff as u8,
                });
                fx = Some(crate::particles::BlockFx {
                    pos: hit.pos,
                    placed: BlockType::SwitchOff,
                    replaced: BlockType::SwitchOn,
                });
            } else if matches!(current, BlockType::Furnace | BlockType::Chest) {
                // เปิดกล่อง — ไม่ใช่การแก้บล็อก ใช้ plumbing เดียวกับหน้าต่างช่องเก็บของ (กด E)
                open_container.0 = Some(OpenContainerState { pos: hit.pos, kind: current });
                inventory.0 = true;
                if let Ok(mut cursor) = cursor_query.single_mut() {
                    cursor.grab_mode = bevy::window::CursorGrabMode::None;
                    cursor.visible = true;
                }
                return;
            }
        } else if place_pressed && selected.0 != BlockType::Air {
            let p = hit.pos + hit.normal;

            // Survival: ต้องมีของในช่องที่เลือกก่อนถึงวางได้ (count>0 หรือ None=∞)
            let mut blocked = survival
                && hotbar.slots[hotbar.selected]
                    .map(|s| s.count == Some(0))
                    .unwrap_or(true);
            if !blocked && selected.0.is_solid() {
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
            if !blocked && selected.0 == BlockType::TallGrass {
                let below = world.get_block(p.x, p.y - 1, p.z);
                if below != BlockType::Grass && below != BlockType::Dirt {
                    blocked = true;
                }
            }

            if !blocked {
                edit = Some(if matches!(selected.0, BlockType::Furnace | BlockType::Chest | BlockType::SmartLamp) {
                    // หน้า "หน้า" หันหาผู้เล่นเสมอ: เทียบแกน X/Z ที่ต่างจากศูนย์กลางบล็อกมากกว่า
                    let facing = camera_query.iter().next().map(|cam| {
                        let center = p.as_vec3() + Vec3::splat(0.5);
                        let d = cam.translation - center;
                        if d.x.abs() >= d.z.abs() {
                            if d.x >= 0.0 { 2u8 } else { 3u8 }
                        } else if d.z >= 0.0 { 4u8 } else { 5u8 }
                    }).unwrap_or(4);
                    BlockEdit::PlaceFacingBlock {
                        pos: p.to_array(),
                        block: selected.0 as u8,
                        facing,
                    }
                } else {
                    BlockEdit::SetBlock {
                        pos: p.to_array(),
                        block: selected.0 as u8,
                    }
                });
                fx = Some(crate::particles::BlockFx {
                    pos: p,
                    placed: selected.0,
                    replaced: world.get_block(p.x, p.y, p.z),
                });
                // Survival: หักจำนวนออกจากช่องที่เลือก (count None = ∞ ไม่หัก)
                if survival {
                    let sel = hotbar.selected;
                    if let Some(stack) = hotbar.slots[sel].as_mut() {
                        if let Some(c) = stack.count {
                            if c <= 1 {
                                hotbar.slots[sel] = None;
                            } else {
                                stack.count = Some(c - 1);
                            }
                        }
                    }
                }
            }
        }
    }

    let Some(edit) = edit else { return };
    let Some(tp) = apply_block_edit(&mut world, &edit) else { return };

    if let Some(fx) = fx {
        fx_writer.write(fx);
    }

    // แก้บล็อกแตะเขตสระ = โครงสร้างรอบน้ำเปลี่ยน บัญชีสระเชื่อไม่ได้แล้ว —
    // ทิ้งสระ (ถ้าน้ำยังขยับ เดี๋ยว form ใหม่ในรูปทรงใหม่เอง)
    pools.invalidate_touching(tp);

    // ส่งเข้า network: host เอาไป broadcast, client เอาไปส่ง RequestEdit หา host
    if net_server.is_some() || net_client.is_some() {
        net_out.0.push_back((None, edit));
    }

    // ปลุกน้ำให้ตื่น (ถ้าบล็อกถูกทุบหรือวาง บล็อกรอบๆ และตัวมันเองต้องเริ่มไหล)
    // — เว้น client: host เป็นคนรัน fluid sim แล้วส่ง delta กลับมา
    //   ถ้าปลุกไว้เฉยๆ set จะโตไม่หยุดเพราะไม่มีระบบมา drain
    if net_client.is_none() {
        active_fluids.0.insert(tp);
        block_updates.0.insert(tp);
        for dir in [IVec3::new(1,0,0), IVec3::new(-1,0,0), IVec3::new(0,1,0), IVec3::new(0,-1,0), IVec3::new(0,0,1), IVec3::new(0,0,-1)] {
            active_fluids.0.insert(tp + dir);
            block_updates.0.insert(tp + dir);
        }
    }

    // เซฟ chunk ที่แก้ลง disk ทันที — ยกเว้นตอนเป็น network client:
    // โลกนี้เป็นของ host, saves/ บนเครื่องเป็นโลก single player ของเราเอง
    let edited_chunk = IVec2::new(
        tp.x.div_euclid(CHUNK_WIDTH as i32),
        tp.z.div_euclid(CHUNK_WIDTH as i32),
    );
    if net_client.is_none() {
        save_loaded_chunk(&world, edited_chunk);
    }

    remesh_chunks(&mut commands, &mut world, &mut mp, edit_affected_chunks(tp));

    // บล็อกเปลี่ยนเฉพาะใน chunk ที่แก้ — อัปเดต PointLight/โมเดล Campfire เฉพาะตรงนั้น
    refresh_chunk_lamp_lights(&mut commands, &mut world, edited_chunk);
    refresh_chunk_campfire_models(&mut commands, &mut world, edited_chunk, &campfire_assets);
}


#[derive(Resource, Default)]
pub struct ActiveFluids(pub std::collections::HashSet<IVec3>);

#[derive(Resource, Default)]
pub struct PendingBlockUpdates(pub std::collections::HashSet<IVec3>);

// --------------------------------------------------------
// TNT / ระเบิด — โมเดล ray แบกพลังงาน + สะท้อนบนหน้าบล็อก
// จุดชนวน: คลิกขวาบล็อก Tnt → SetBlock เป็น TntLit (sync ผ่าน edit ปกติ)
// host/single เป็นเจ้าของ fuse+detonation แบบเดียวกับ fluid sim
// --------------------------------------------------------

/// TNT ที่จุดแล้ว รอระเบิด (เฉพาะฝั่งที่รัน simulation — client เห็นแค่บล็อก TntLit)
#[derive(Resource, Default)]
pub struct ActiveTnt(pub std::collections::HashMap<IVec3, Timer>);

/// จำนวน ray ต่อการระเบิด (fibonacci sphere)
const EXPLOSION_RAYS: usize = 400;
/// พลังงานที่เสียต่อ 1 บล็อกที่เดินผ่านในที่โล่ง — จำลองคลื่นกระจายตัว/เจือจาง
const EXPLOSION_AIR_FALLOFF: f32 = 0.25;
/// แรงตกต่อบล็อกตอนถูกบีบในที่แคบ (เพิ่งสะท้อนมาไม่เกิน CONFINE_WINDOW บล็อก)
/// — ในท่อคลื่นไม่ได้กระจาย พลังงานวิ่งไกลเกือบเต็ม แล้วค่อยตกปกติเมื่อพ้นท่อ
const EXPLOSION_CONFINED_FALLOFF: f32 = 0.05;
const EXPLOSION_CONFINE_WINDOW: u32 = 4;
/// สัดส่วนพลังงานที่เสียตอนสะท้อนแบบชนตั้งฉาก — ชนเฉียงเสียน้อยกว่าตามมุมตก
/// (เลียบผนังท่อแทบไม่เสีย = แรงระเบิดถูกบีบไปออกปลายท่อ)
const EXPLOSION_REFLECT_LOSS: f32 = 0.3;
/// งบการสะท้อนรวมต่อ ray นับตามมุมตก (ชนตรง = 1.0 เต็ม, เฉียงกริบ ≈ 0)
const EXPLOSION_BOUNCE_BUDGET: f32 = 6.0;
/// กันลูปยาวผิดปกติ (พลังงานหมดก่อนเสมอในทางปฏิบัติ)
const EXPLOSION_MAX_STEPS: usize = 400;

/// ท่อนหนึ่งของเส้นทาง ray (ตัดท่อนใหม่ทุกการสะท้อน) — ใช้ทั้ง debug และ shockwave
#[derive(Clone, Copy)]
pub struct RaySeg {
    pub a: Vec3,
    pub b: Vec3,
    /// พลังงานตอนต้น segment
    pub energy: f32,
    /// ระยะสะสมตามเส้นทาง ray ณ จุด a (นับจากจุดกำเนิด) — ไว้ขับหน้าคลื่น shockwave
    pub dist0: f32,
}

pub struct ExplosionResult {
    /// บล็อกที่ถูกทำลาย (ไม่รวมน้ำ — น้ำดูดซับอย่างเดียว ปริมาตร conserve)
    pub destroyed: std::collections::HashSet<IVec3>,
    /// Tnt/TntLit ลูกอื่นที่โดนแรงระเบิด → จุดต่อเป็นลูกโซ่
    pub chain: std::collections::HashSet<IVec3>,
    /// เส้นทาง ray ทุกเส้น — เก็บเสมอ (เล็กมาก) ใช้ขับ shockwave + debug
    pub rays: Vec<RaySeg>,
}

/// เส้น ray ของระเบิดล่าสุดค้างไว้ให้ดู (เปิดผ่าน checkbox Show TNT Rays)
#[derive(Resource, Default)]
pub struct ExplosionDebug {
    pub segments: Vec<RaySeg>,
    pub ttl: f32,
}

/// คำนวณผลระเบิดของกอง TNT ที่จุดพร้อมกัน (pure — ผู้เรียกเป็นคน apply)
/// - พลังต่อ ray โต ∝ N^⅓ (ฟิสิกส์จริง: รัศมีระเบิด ∝ มวล^⅓; N=1 เท่าระบบเดิมเป๊ะ)
/// - จุดกำเนิด ray วนตามบล็อกในกอง → รูปทรงกองกำหนดรูประเบิดเอง
///   (แถวยาว = ฟาดแนว, ก้อน = ทรงกลม, แผ่นแปะกำแพง = shaped charge)
pub fn explode(world: &VoxelWorld, cluster: &[IVec3], power: f32) -> ExplosionResult {
    let n = cluster.len().max(1);
    let energy = power * (n as f32).cbrt();
    let n_rays = (EXPLOSION_RAYS + 150 * (n - 1)).min(1600);
    explode_rays(&|x, y, z| world.get_block(x, y, z), cluster, energy, n_rays)
}

/// แกนกลางของการระเบิด — อ่านบล็อกผ่าน closure ให้รันได้ทั้งบน &VoxelWorld
/// (TNT, sync) และบน WorldSnapshot ใน background task (nuke, async)
pub fn explode_rays(
    sample: &dyn Fn(i32, i32, i32) -> BlockType,
    cluster: &[IVec3],
    energy: f32,
    n_rays: usize,
) -> ExplosionResult {
    let mut result = ExplosionResult {
        // seed ด้วยทั้งกอง: ray ทะลุผ่าน TNT ด้วยกันเอง (march เช็ค destroyed = โล่ง)
        // และ Air edits ของกองออกจาก destroyed ชุดเดียว
        destroyed: cluster.iter().copied().collect(),
        chain: Default::default(),
        rays: Vec::new(),
    };
    let n = cluster.len().max(1);
    let golden = std::f32::consts::PI * (1.0 + 5.0_f32.sqrt());
    for i in 0..n_rays {
        // fibonacci sphere: กระจายทิศสม่ำเสมอทั้งทรงกลม
        let k = i as f32 + 0.5;
        let phi = (1.0 - 2.0 * k / n_rays as f32).acos();
        let theta = golden * k;
        let dir = Vec3::new(
            phi.sin() * theta.cos(),
            phi.cos(),
            phi.sin() * theta.sin(),
        );
        let origin = cluster[i % n].as_vec3() + Vec3::splat(0.5);
        march_explosion_ray(sample, origin, dir, energy, &mut result);
    }
    result
}

/// เดิน ray 1 เส้นด้วย DDA (โครงเดียวกับ raycast เล็งบล็อก) สะสมผลใน result
/// - ทะลุได้: จ่าย hardness แล้ววิ่งต่อ
/// - ทะลุไม่ไหว: สะท้อน specular ตามแกนของหน้าที่ชน เสียพลังงานส่วนหนึ่ง
fn march_explosion_ray(
    sample: &dyn Fn(i32, i32, i32) -> BlockType,
    mut origin: Vec3,
    mut dir: Vec3,
    mut energy: f32,
    result: &mut ExplosionResult,
) {
    let mut bounce_used = 0.0f32;
    let mut steps = 0usize;
    // นับบล็อกตั้งแต่สะท้อนครั้งล่าสุด — น้อย = ยังถูกบีบในที่แคบ (เริ่มแบบที่โล่ง)
    let mut cells_since_bounce = u32::MAX;
    // segment ปัจจุบัน (ตัดใหม่ทุกครั้งที่สะท้อน) + ระยะสะสมตามเส้นทาง
    let mut seg_start = origin;
    let mut seg_energy = energy;
    let mut travelled = 0.0f32;

    'restart: loop {
        // DDA state จากจุดกำเนิด/ทิศปัจจุบัน (คำนวณใหม่ทุกครั้งหลังสะท้อน)
        let mut map = IVec3::new(
            origin.x.floor() as i32,
            origin.y.floor() as i32,
            origin.z.floor() as i32,
        );
        let delta = Vec3::new(
            if dir.x == 0.0 { f32::INFINITY } else { (1.0 / dir.x).abs() },
            if dir.y == 0.0 { f32::INFINITY } else { (1.0 / dir.y).abs() },
            if dir.z == 0.0 { f32::INFINITY } else { (1.0 / dir.z).abs() },
        );
        let step = IVec3::new(
            if dir.x < 0.0 { -1 } else { 1 },
            if dir.y < 0.0 { -1 } else { 1 },
            if dir.z < 0.0 { -1 } else { 1 },
        );
        let mut side_dist = Vec3::new(
            if dir.x < 0.0 { (origin.x - map.x as f32) * delta.x } else { (map.x as f32 + 1.0 - origin.x) * delta.x },
            if dir.y < 0.0 { (origin.y - map.y as f32) * delta.y } else { (map.y as f32 + 1.0 - origin.y) * delta.y },
            if dir.z < 0.0 { (origin.z - map.z as f32) * delta.z } else { (map.z as f32 + 1.0 - origin.z) * delta.z },
        );

        loop {
            steps += 1;

            // ก้าวเข้า cell ถัดไป — จำแกนที่ข้าม (หน้าที่ชน) กับระยะ ณ จุดข้าม
            let (axis, t_cross) = if side_dist.x < side_dist.y {
                if side_dist.x < side_dist.z { (0, side_dist.x) } else { (2, side_dist.z) }
            } else {
                if side_dist.y < side_dist.z { (1, side_dist.y) } else { (2, side_dist.z) }
            };
            match axis {
                0 => { side_dist.x += delta.x; map.x += step.x; }
                1 => { side_dist.y += delta.y; map.y += step.y; }
                _ => { side_dist.z += delta.z; map.z += step.z; }
            }

            if steps > EXPLOSION_MAX_STEPS {
                let end = origin + dir * t_cross;
                result.rays.push(RaySeg { a: seg_start, b: end, energy: seg_energy, dist0: travelled });
                return;
            }

            // แรงตกตามระยะทาง: ที่แคบ (เพิ่งสะท้อน) ตกช้ากว่าที่โล่งมาก
            let falloff = if cells_since_bounce < EXPLOSION_CONFINE_WINDOW {
                EXPLOSION_CONFINED_FALLOFF
            } else {
                EXPLOSION_AIR_FALLOFF
            };
            cells_since_bounce = cells_since_bounce.saturating_add(1);
            energy -= falloff;
            if energy <= 0.0 {
                let end = origin + dir * t_cross;
                result.rays.push(RaySeg { a: seg_start, b: end, energy: seg_energy, dist0: travelled });
                return;
            }

            if result.destroyed.contains(&map) {
                continue; // กองตัวเอง / บล็อกที่ ray อื่นทำลายไปแล้ว = โล่ง
            }
            let block = sample(map.x, map.y, map.z);
            match block {
                BlockType::Air => {}
                b if b.is_water() => {
                    // น้ำดูดซับตามระดับ แต่ไม่ถูกทำลาย (ปริมาตรต้อง conserve)
                    energy -= block_hardness(b);
                    if energy <= 0.0 {
                        let end = origin + dir * t_cross;
                        result.rays.push(RaySeg { a: seg_start, b: end, energy: seg_energy, dist0: travelled });
                        return;
                    }
                }
                BlockType::Tnt | BlockType::TntLit => {
                    result.chain.insert(map);
                    energy -= block_hardness(BlockType::Tnt);
                    if energy <= 0.0 {
                        let end = origin + dir * t_cross;
                        result.rays.push(RaySeg { a: seg_start, b: end, energy: seg_energy, dist0: travelled });
                        return;
                    }
                }
                b => {
                    let h = block_hardness(b);
                    if energy >= h {
                        energy -= h;
                        result.destroyed.insert(map);
                    } else {
                        // ทะลุไม่ไหว — สะท้อนออกจากหน้าที่ชน (นี่คือกลไกท่อ/ปืนใหญ่)
                        // มุมตกยิ่งตรง (|dir·normal| → 1) ยิ่งเสียพลังงาน/งบสะท้อนมาก
                        // เลียบผนังเฉียงๆ แทบไม่เสีย = แรงถูกบีบวิ่งไปออกปลายท่อ
                        let incidence = match axis {
                            0 => dir.x.abs(),
                            1 => dir.y.abs(),
                            _ => dir.z.abs(),
                        };
                        bounce_used += incidence;
                        energy *= 1.0 - EXPLOSION_REFLECT_LOSS * incidence;
                        let hit_point = origin + dir * t_cross;
                        result.rays.push(RaySeg {
                            a: seg_start,
                            b: hit_point,
                            energy: seg_energy,
                            dist0: travelled,
                        });
                        travelled += seg_start.distance(hit_point);
                        if bounce_used > EXPLOSION_BOUNCE_BUDGET {
                            return;
                        }
                        match axis {
                            0 => dir.x = -dir.x,
                            1 => dir.y = -dir.y,
                            _ => dir.z = -dir.z,
                        }
                        // ขยับออกจากผิวนิดเดียว กัน DDA รอบใหม่เข้า cell เดิมซ้ำ
                        origin = hit_point + dir * 1e-3;
                        seg_start = origin;
                        seg_energy = energy;
                        cells_since_bounce = 0;
                        continue 'restart;
                    }
                }
            }
        }
    }
}

/// นับถอยหลัง fuse แล้วระเบิด: ทำลายบล็อก + จุดลูกโซ่ + broadcast + remesh แบบ batch
/// (bookkeeping ชุดเดียวกับท้าย block_interaction_system แต่รวบเป็นชุดใหญ่)
pub fn tnt_detonation_system(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    time: Res<Time>,
    settings: Res<crate::GameSettings>,
    mut active_tnt: ResMut<ActiveTnt>,
    mut mp: MeshingParams,
    mut active_fluids: ResMut<ActiveFluids>,
    mut block_updates: ResMut<PendingBlockUpdates>,
    mut pools: ResMut<ActivePools>,
    (net_server, mut net_out, mut net_fx): (
        Option<Res<bevy_renet::RenetServer>>,
        ResMut<crate::network::PendingNetEdits>,
        ResMut<crate::network::PendingNetFx>,
    ),
    mut fx: MessageWriter<crate::particles::ExplosionFx>,
    mut debug: ResMut<ExplosionDebug>,
    jobs: Res<NukeJobs>,
    campfire_assets: Res<BlockModelAssets>,
) {
    if active_tnt.0.is_empty() {
        return;
    }
    let mut exploding: Vec<IVec3> = Vec::new();
    for (pos, timer) in active_tnt.0.iter_mut() {
        if timer.tick(time.delta()).is_finished() {
            exploding.push(*pos);
        }
    }
    if exploding.is_empty() {
        return;
    }

    use crate::network::BlockEdit;
    let mut edits: Vec<BlockEdit> = Vec::new();
    let mut chained: Vec<IVec3> = Vec::new();

    // กันลูกที่ถูกกลืนโดยระเบิดอื่นในเฟรมเดียวกันระเบิดซ้ำ (edits ถูก apply หลังคำนวณครบ)
    let mut consumed: std::collections::HashSet<IVec3> = Default::default();

    for center in exploding {
        active_tnt.0.remove(&center);
        if consumed.contains(&center) {
            continue;
        }
        let center_block = world.get_block(center.x, center.y, center.z);
        // nuke: แยกเส้นทาง — คำนวณใน background task แล้ว nuke_apply_system รับช่วง
        if center_block == BlockType::NukeLit {
            start_nuke(&world, center, &settings, &jobs);
            consumed.insert(center);
            continue;
        }
        // โดนทุบทิ้งระหว่างรอ fuse = ปลดชนวนแล้ว
        if center_block != BlockType::TntLit {
            continue;
        }

        // flood-fill กอง TNT ที่ต่อเนื่องกัน (6 ทิศ) — detonation wave วิ่งผ่านก้อน
        // ที่ติดกันแทบทันที = ระเบิดพร้อมกันเป็นลูกเดียว (cap กัน CPU/กองมหึมา)
        const CLUSTER_CAP: usize = 64;
        let mut cluster: Vec<IVec3> = vec![center];
        let mut seen: std::collections::HashSet<IVec3> = [center].into_iter().collect();
        let mut qi = 0;
        while qi < cluster.len() && cluster.len() < CLUSTER_CAP {
            let cur = cluster[qi];
            qi += 1;
            for d in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                let p = cur + d;
                if !seen.insert(p) || consumed.contains(&p) || cluster.len() >= CLUSTER_CAP {
                    continue;
                }
                if matches!(world.get_block(p.x, p.y, p.z), BlockType::Tnt | BlockType::TntLit) {
                    cluster.push(p);
                }
            }
        }
        for p in &cluster {
            consumed.insert(*p);
            // สมาชิกที่จุดไว้แล้วรอ fuse อยู่ — ถูกกลืนในลูกนี้แทน
            active_tnt.0.remove(p);
        }

        let mut result = explode(&world, &cluster, settings.tnt_power);
        let rays = std::mem::take(&mut result.rays);
        if settings.show_tnt_rays {
            debug.segments.extend(rays.iter().copied());
            debug.ttl = 8.0;
        }
        // destroyed ครอบทั้งกอง (seed ใน explode) — Air edits ชุดเดียวจบ
        for p in result.destroyed {
            edits.push(BlockEdit::SetBlock { pos: p.to_array(), block: BlockType::Air as u8 });
        }
        for p in result.chain {
            // จุดเฉพาะลูกที่ยังไม่ติดและยังไม่ถูกกลืน (TntLit อยู่ใน ActiveTnt แล้ว)
            if !consumed.contains(&p) && world.get_block(p.x, p.y, p.z) == BlockType::Tnt {
                edits.push(BlockEdit::SetBlock { pos: p.to_array(), block: BlockType::TntLit as u8 });
                chained.push(p);
            }
        }
        // เอฟเฟกต์ลูกเดียวที่กึ่งกลางมวลของกอง — rays ไปขับ shockwave ต่อ
        let com = cluster.iter().map(|p| p.as_vec3()).sum::<Vec3>() / cluster.len() as f32
            + Vec3::splat(0.5);
        let power = settings.tnt_power * (cluster.len() as f32).cbrt();
        // client ไม่รันระบบนี้ (gate is_not_client) — ต้องส่งเอฟเฟกต์ให้ ไม่งั้นเห็นแค่บล็อกหาย
        if net_server.is_some() {
            net_fx.0.push(crate::network::ExplosionWire::new(com, &rays, power, false));
        }
        fx.write(crate::particles::ExplosionFx {
            center: com,
            rays,
            power,
            is_nuke: false,
        });
    }

    let mut remesh: std::collections::HashSet<IVec2> = Default::default();
    let mut edited_chunks: std::collections::HashSet<IVec2> = Default::default();
    for edit in &edits {
        let Some(tp) = apply_block_edit(&mut world, edit) else { continue };
        pools.invalidate_touching(tp);
        active_fluids.0.insert(tp);
        block_updates.0.insert(tp);
        for d in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
            active_fluids.0.insert(tp + d);
            block_updates.0.insert(tp + d);
        }
        remesh.extend(edit_affected_chunks(tp));
        edited_chunks.insert(IVec2::new(
            tp.x.div_euclid(CHUNK_WIDTH as i32),
            tp.z.div_euclid(CHUNK_WIDTH as i32),
        ));
        if net_server.is_some() {
            net_out.0.push_back((None, edit.clone()));
        }
    }

    // ลูกโซ่: fuse สั้นสุ่มตามพิกัด (deterministic) ให้ระเบิดไล่จังหวะสวยๆ
    for p in chained {
        let fuse = 0.15 + (pos_hash(p.x, p.y, p.z) % 300) as f32 / 1000.0;
        active_tnt.0.insert(p, Timer::from_seconds(fuse, TimerMode::Once));
    }

    for cp in &edited_chunks {
        save_loaded_chunk(&world, *cp);
    }
    remesh_chunks(&mut commands, &mut world, &mut mp, remesh);
    for cp in edited_chunks {
        refresh_chunk_lamp_lights(&mut commands, &mut world, cp);
        refresh_chunk_campfire_models(&mut commands, &mut world, cp, &campfire_assets);
    }
}

// --------------------------------------------------------
// Nuke — yield ใหญ่: คำนวณบน snapshot ใน background task แล้วทยอย apply
// ตามหน้าคลื่นทีละ chunk (บล็อกหลายแสน + remesh ร้อย chunk ห้ามทำเฟรมเดียว)
// --------------------------------------------------------

/// ความเร็วหน้าคลื่น nuke (บล็อก/วิ) — เร็วกว่า TNT ให้ฟีลระเบิดใหญ่
pub const NUKE_WAVE_SPEED: f32 = 60.0;
/// เพดาน chunk ที่ finalize ต่อเฟรม — กัน spike (remesh chunk ละหลาย ms)
const NUKE_CHUNKS_PER_FRAME: usize = 2;
const NUKE_MAX_RAYS: usize = 16_000;

/// snapshot บล็อกรอบจุดระเบิด — clone แค่ Arc ต่อ chunk (ถูกมาก) ส่งเข้า task ได้
pub struct WorldSnapshot {
    chunks: std::collections::HashMap<IVec2, Arc<ChunkBlocks>>,
}

impl WorldSnapshot {
    /// คณิตเดียวกับ VoxelWorld::get_block (voxel.rs:359)
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> BlockType {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return BlockType::Air;
        }
        let cx = x.div_euclid(CHUNK_WIDTH as i32);
        let cz = z.div_euclid(CHUNK_WIDTH as i32);
        match self.chunks.get(&IVec2::new(cx, cz)) {
            Some(blocks) => {
                let lx = x.rem_euclid(CHUNK_WIDTH as i32) as usize;
                let lz = z.rem_euclid(CHUNK_WIDTH as i32) as usize;
                blocks.get(lx, y as usize, lz)
            }
            None => BlockType::Air,
        }
    }
}

pub struct NukeResult {
    pub center: IVec3,
    /// พลังต่อ ray (หลังสเกล yield^⅓) — ไว้ normalize เอฟเฟกต์
    pub energy: f32,
    pub result: ExplosionResult,
}

/// channel รับผลจาก task (แพทเทิร์นเดียวกับ ChunkGenerator)
#[derive(Resource)]
pub struct NukeJobs {
    pub sender: Mutex<Sender<NukeResult>>,
    pub receiver: Mutex<Receiver<NukeResult>>,
}

impl Default for NukeJobs {
    fn default() -> Self {
        let (s, r) = mpsc::channel();
        Self { sender: Mutex::new(s), receiver: Mutex::new(r) }
    }
}

/// งานทยอยลบบล็อก: chunk เรียงตามระยะไกลสุดจากศูนย์กลาง finalize เมื่อคลื่นผ่าน
pub struct NukeApply {
    front: f32,
    pending: std::collections::VecDeque<(f32, IVec2, Vec<IVec3>)>,
}

#[derive(Resource, Default)]
pub struct NukeApplication(pub Vec<NukeApply>);

/// spawn task คำนวณ nuke — สูตร scaling จริง: รัศมี ∝ yield^⅓ (Hopkinson–Cranz),
/// จำนวน ray ∝ พื้นผิวคลื่น ∝ yield^⅔
fn start_nuke(world: &VoxelWorld, center: IVec3, settings: &crate::GameSettings, jobs: &NukeJobs) {
    let y = settings.nuke_yield.max(1.0);
    let energy = settings.tnt_power * y.cbrt();
    let n_rays = ((EXPLOSION_RAYS as f32 * y.powf(2.0 / 3.0)) as usize)
        .clamp(EXPLOSION_RAYS, NUKE_MAX_RAYS);
    // รัศมีไกลสุดที่ ray ไปถึงได้ (พลังงานหมดพอดี) — snapshot เผื่อขอบ
    let reach = energy / EXPLOSION_AIR_FALLOFF + CHUNK_WIDTH as f32;

    let mut chunks = std::collections::HashMap::new();
    let c2 = Vec2::new(center.x as f32, center.z as f32);
    for (pos, chunk) in world.chunks.iter() {
        let cc = Vec2::new(
            (pos.x * CHUNK_WIDTH as i32 + CHUNK_WIDTH as i32 / 2) as f32,
            (pos.y * CHUNK_WIDTH as i32 + CHUNK_WIDTH as i32 / 2) as f32,
        );
        if cc.distance(c2) <= reach + CHUNK_WIDTH as f32 {
            chunks.insert(*pos, chunk.blocks.clone());
        }
    }
    let snapshot = WorldSnapshot { chunks };
    let sender = jobs.sender.lock().unwrap().clone();
    let cluster = vec![center];
    AsyncComputeTaskPool::get()
        .spawn(async move {
            let result =
                explode_rays(&|x, y, z| snapshot.get_block(x, y, z), &cluster, energy, n_rays);
            let _ = sender.send(NukeResult { center, energy, result });
        })
        .detach();
    info!("nuke: yield {y:.0} → energy/ray {energy:.1}, {n_rays} rays");
}

/// รับผลจาก task + เดินหน้าคลื่น finalize ทีละ chunk (host/single เท่านั้น)
pub fn nuke_apply_system(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    time: Res<Time>,
    settings: Res<crate::GameSettings>,
    jobs: Res<NukeJobs>,
    mut apps: ResMut<NukeApplication>,
    mut mp: MeshingParams,
    (mut active_fluids, mut _block_updates): (ResMut<ActiveFluids>, ResMut<PendingBlockUpdates>),
    mut pools: ResMut<ActivePools>,
    mut active_tnt: ResMut<ActiveTnt>,
    (net_server, mut host_sync, mut net_out, mut net_fx): (
        Option<Res<bevy_renet::RenetServer>>,
        Option<ResMut<crate::network::HostSync>>,
        ResMut<crate::network::PendingNetEdits>,
        ResMut<crate::network::PendingNetFx>,
    ),
    mut fx: MessageWriter<crate::particles::ExplosionFx>,
    mut debug: ResMut<ExplosionDebug>,
    campfire_assets: Res<BlockModelAssets>,
) {
    use crate::network::BlockEdit;

    // ---- รับผลจาก task ----
    loop {
        let res = { jobs.receiver.lock().unwrap().try_recv() };
        let Ok(res) = res else { break };
        let centerf = res.center.as_vec3() + Vec3::splat(0.5);

        // จัดกลุ่ม destroyed ตาม chunk พร้อมระยะไกลสุด เรียงใกล้→ไกล
        let mut by_chunk: std::collections::HashMap<IVec2, (f32, Vec<IVec3>)> =
            Default::default();
        for p in res.result.destroyed.iter() {
            let cp = IVec2::new(
                p.x.div_euclid(CHUNK_WIDTH as i32),
                p.z.div_euclid(CHUNK_WIDTH as i32),
            );
            let d = (p.as_vec3() + Vec3::splat(0.5)).distance(centerf);
            let e = by_chunk.entry(cp).or_insert((0.0, Vec::new()));
            e.0 = e.0.max(d);
            e.1.push(*p);
        }
        let mut pending: Vec<(f32, IVec2, Vec<IVec3>)> =
            by_chunk.into_iter().map(|(cp, (d, v))| (d, cp, v)).collect();
        pending.sort_by(|a, b| a.0.total_cmp(&b.0));

        // ลูกโซ่ TNT: จุดตอนนี้เลยด้วย fuse ตามระยะ — ตูมตอนคลื่นวิ่งไปถึงพอดี
        for p in res.result.chain.iter() {
            if world.get_block(p.x, p.y, p.z) != BlockType::Tnt {
                continue;
            }
            let edit = BlockEdit::SetBlock { pos: p.to_array(), block: BlockType::TntLit as u8 };
            if apply_block_edit(&mut world, &edit).is_some() {
                remesh_chunks(&mut commands, &mut world, &mut mp, edit_affected_chunks(*p));
                if net_server.is_some() {
                    net_out.0.push_back((None, edit));
                }
                let d = (p.as_vec3() + Vec3::splat(0.5)).distance(centerf);
                active_tnt
                    .0
                    .insert(*p, Timer::from_seconds(d / NUKE_WAVE_SPEED + 0.1, TimerMode::Once));
            }
        }

        // debug เอาเต็ม / shockwave subsample กัน mesh หน้าคลื่นบวมเกิน
        if settings.show_tnt_rays {
            debug.segments.extend(res.result.rays.iter().copied());
            debug.ttl = 10.0;
        }
        let fx_rays: Vec<RaySeg> = if res.result.rays.len() > 2000 {
            let stride = res.result.rays.len().div_ceil(2000);
            res.result.rays.iter().copied().step_by(stride).collect()
        } else {
            res.result.rays.clone()
        };
        if net_server.is_some() {
            net_fx.0.push(crate::network::ExplosionWire::new(
                centerf,
                &fx_rays,
                res.energy,
                true,
            ));
        }
        fx.write(crate::particles::ExplosionFx {
            center: centerf,
            rays: fx_rays,
            power: res.energy,
            is_nuke: true,
        });

        apps.0.push(NukeApply { front: 0.0, pending: pending.into() });
    }

    // ---- เดินหน้าคลื่น + finalize chunk (จำกัดต่อเฟรมกัน spike) ----
    if apps.0.is_empty() {
        return;
    }
    let mut budget = NUKE_CHUNKS_PER_FRAME;
    apps.0.retain_mut(|app| {
        app.front += NUKE_WAVE_SPEED * time.delta_secs();
        while budget > 0 {
            let Some((d, _, _)) = app.pending.front() else { break };
            if *d > app.front {
                break;
            }
            let (_, cp, blocks) = app.pending.pop_front().unwrap();
            budget -= 1;

            let mut remesh: std::collections::HashSet<IVec2> = Default::default();
            remesh.insert(cp);
            for p in &blocks {
                let edit = BlockEdit::SetBlock { pos: p.to_array(), block: BlockType::Air as u8 };
                if apply_block_edit(&mut world, &edit).is_none() {
                    continue;
                }
                // บล็อกริมขอบ chunk — เพื่อนบ้านต้อง remesh หน้าที่เพิ่งโผล่ด้วย
                let lx = p.x.rem_euclid(CHUNK_WIDTH as i32);
                let lz = p.z.rem_euclid(CHUNK_WIDTH as i32);
                if lx == 0 || lx == CHUNK_WIDTH as i32 - 1 || lz == 0 || lz == CHUNK_WIDTH as i32 - 1 {
                    remesh.extend(edit_affected_chunks(*p));
                }
                // ปลุกเฉพาะน้ำที่ติดหลุม (ปลุกทั้งหลุมแพงเปล่าๆ)
                for dv in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                    let n = *p + dv;
                    if world.get_block(n.x, n.y, n.z).is_water() {
                        active_fluids.0.insert(n);
                    }
                }
            }
            // pool แถวนี้เชื่อไม่ได้แล้ว — sample ห่างๆ พอ (pool ใหม่ form เองได้)
            for p in blocks.iter().step_by(32) {
                pools.invalidate_touching(*p);
            }

            save_loaded_chunk(&world, cp);
            remesh_chunks(&mut commands, &mut world, &mut mp, remesh);
            refresh_chunk_lamp_lights(&mut commands, &mut world, cp);
            refresh_chunk_campfire_models(&mut commands, &mut world, cp, &campfire_assets);
            // multiplayer: ส่ง chunk ทั้งก้อน (ราย edit เป็นแสนจะล้นท่อ reliable)
            if let (Some(server), Some(hs)) = (net_server.as_ref(), host_sync.as_mut()) {
                crate::network::queue_chunk_to_all_clients(server, hs, cp);
            }
        }
        !app.pending.is_empty()
    });
}

/// เข้าโลกจริงครั้งแรก (โลกว่างหรือกำลังจะ regenerate) — วางผู้เล่นกลาง tile
/// เหนือผิวจริง; client multiplayer ไม่ยุ่ง (host ส่ง spawn_pos มาใน Welcome แล้ว)
pub fn position_player_for_terrain(
    settings: Res<crate::GameSettings>,
    regen: Res<crate::RegenerateWorld>,
    world: Res<VoxelWorld>,
    client: Option<Res<bevy_renet::RenetClient>>,
    mut camera: Query<(&mut Transform, &mut crate::camera::FreeCamera)>,
) {
    if settings.terrain_source != crate::TerrainSource::RealWorld || client.is_some() {
        return;
    }
    if !regen.0 && !world.chunks.is_empty() {
        return; // กลับเข้าโลกเดิมที่ยังอยู่ใน memory — อยู่ที่เดิมต่อ
    }
    let Some(d) = crate::dem::streamer() else { return };
    // spawn ที่เชียงใหม่ (ดอยสุเทพ ~18.79N 98.98E) ถ้ามี tile นั้น; ไม่งั้น fallback
    // ไป tile แรกที่มี — deterministic ไม่สุ่มจุดทุกครั้งแบบ center_block เดิม
    let (cx, cz) = if d.has_tile_at(18.79, 98.98) {
        crate::dem::latlon_to_block(18.79, 98.98)
    } else {
        d.center_block()
    };
    // โหลด tile ตรงจุด spawn แบบ blocking ก่อน ไม่งั้น elevation คืน 0 (ทะเล)
    // เพราะ tile ยังโหลด async ไม่ทัน → ผู้เล่นโผล่ต่ำกว่าภูเขาจริง
    d.load_blocking_at(cx, cz);
    let h = crate::dem::DEM_SEA_LEVEL_Y as f32 + d.elevation_at_block(cx, cz);
    if let Some((mut t, mut cam)) = camera.iter_mut().next() {
        t.translation = Vec3::new(cx as f32, h + 20.0, cz as f32);
        // รีเซ็ตมุมมองเป็นระดับสายตา — เดิมสืบทอด pitch ก้มจาก setup_camera
        // (จูนไว้ให้โลก noise มองลงเห็นพื้นตอนเริ่ม) ทำให้โผล่มาก้มมองพื้นเกือบดิ่ง
        cam.yaw = 0.0;
        cam.pitch = 0.0;
        t.rotation = Quat::from_axis_angle(Vec3::Y, cam.yaw) * Quat::from_axis_angle(Vec3::X, cam.pitch);
        info!("spawn โลกจริง: บล็อก ({:.0}, {:.0}) ผิวสูง {:.0} ม.", cx, cz, h);
    }
}

/// มองเห็นกันไหม (ไม่มีบล็อกทึบขวาง) — ใช้คำนวณแสงจ้าเข้าตาตอนระเบิด
/// เดินแบบ sampling ทีละครึ่งบล็อกพอ (เรียกครั้งเดียวต่อการระเบิด ไม่ต้อง DDA เป๊ะ)
pub fn line_of_sight(world: &VoxelWorld, from: Vec3, to: Vec3) -> bool {
    let delta = to - from;
    let dist = delta.length();
    if dist < 1.0 {
        return true;
    }
    let dir = delta / dist;
    let steps = (dist * 2.0) as i32;
    for i in 1..steps {
        let p = from + dir * (i as f32 * 0.5);
        if world.get_block(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32).occludes() {
            return false;
        }
    }
    true
}

/// วาดเส้นทาง ray ของระเบิดล่าสุดค้างไว้ 8 วิ — สีบอกพลังงานตอนเริ่ม segment
/// (เหลืองสว่าง = แรงมาก, แดงมืด = ใกล้หมด) เห็นการสะท้อนในท่อชัดๆ
pub fn explosion_debug_system(
    time: Res<Time>,
    settings: Res<crate::GameSettings>,
    mut debug: ResMut<ExplosionDebug>,
    mut gizmos: Gizmos,
) {
    if debug.ttl <= 0.0 {
        return;
    }
    debug.ttl -= time.delta_secs();
    if debug.ttl <= 0.0 || !settings.show_tnt_rays {
        debug.segments.clear();
        debug.ttl = 0.0;
        return;
    }
    let max_e = settings.tnt_power.max(0.1);
    for seg in &debug.segments {
        let f = (seg.energy / max_e).clamp(0.0, 1.0);
        gizmos.line(seg.a, seg.b, Color::srgb(1.0, 0.1 + 0.9 * f, 0.05 + 0.35 * f));
    }
}

// --------------------------------------------------------
// Ephemeral pools — สระชั่วคราวสำหรับระบาย/เกลี่ยน้ำผืนใหญ่
// เกิดเฉพาะตอนน้ำผืนใหญ่กำลังขยับ ใช้บัญชีปริมาตรรวม + เลขระดับผิวตัวเดียว
// แทน simulate ราย cell; นิ่งเมื่อไรทิ้ง object ทันที (บล็อกในโลกคือ state จริง)
// conserve volume เป๊ะทุกหน่วย — ห้ามมี infinite source (ทิศทาง design โปรเจกต์)
// --------------------------------------------------------

/// จำนวน active cells ต่อ tick ที่เริ่มลอง form pool (ต่ำกว่านี้ cellular เอาอยู่)
const POOL_TRIGGER_ACTIVE: usize = 400;
/// เพดาน cells ต่อสระ — เกิน = ไม่ form (ทะเล/มหาสมุทรอยู่ cellular ตามเดิม)
const POOL_CELL_CAP: usize = 150_000;
const POOL_COLUMN_CAP: usize = 32_768;
/// สระเล็กกว่านี้ไม่คุ้มค่า overhead
const POOL_MIN_COLUMNS: usize = 16;
const MAX_POOLS: usize = 4;
/// tick ที่นิ่งสนิทติดกันก่อน retire (20 tick = ~2 วินาที)
const POOL_IDLE_TICKS: u32 = 20;
/// งบ set_block ต่อ tick รวมทุกสระ (คุมทั้ง CPU และปริมาณ delta บน network)
const POOL_SWEEP_BUDGET: usize = 2_048;
/// เพดานรายการจุดรั่วต่อสระ
const POOL_MAX_LEAKS: usize = 1_024;

/// run น้ำต่อเนื่องหนึ่งช่วงในคอลัมน์ (สระจับแค่ run เดียวต่อคอลัมน์ —
/// น้ำช่วงอื่นในคอลัมน์เดียวกัน เช่นแอ่งบนหิ้งถ้ำ ปล่อยให้ cellular ดูแล)
pub struct PoolColumn {
    pub y_bottom: i32,
    pub y_top: i32,
}

/// จุดเปลี่ยนความชันของฟังก์ชันความจุ cap(S) — ไว้ solve ระดับผิวจาก volume
/// แบบ O(log n) (cap เป็น piecewise linear ของระดับ S หน่วย 1/8 บล็อก)
struct CapSegment {
    s_start: i64,
    cap_start: u64,
    active: i64,
}

pub struct Pool {
    pub columns: HashMap<(i32, i32), PoolColumn>,
    pub column_order: Vec<(i32, i32)>,
    /// ปริมาตรจริงในบัญชี หน่วย 1/8 บล็อก — แหล่งความจริงเดียวระหว่างสระมีชีวิต
    pub volume: u64,
    /// ระดับผิวเป้าหมาย fixed-point (y*8 + เศษ)
    pub surface: i64,
    /// ระดับที่บล็อกในโลกสะท้อนอยู่ (sweep ไล่ให้เท่า surface ทีละ lap)
    pub applied_surface: i64,
    /// ระดับผิว ณ ตอนเริ่ม lap ปัจจุบัน — lap จบถึงตั้ง applied เป็นค่านี้
    /// (surface อาจขยับระหว่าง lap คอลัมน์ต้นๆ จะเขียนด้วยค่าเก่า)
    lap_surface: i64,
    pub sweep_cursor: usize,
    pub min: IVec3,
    pub max: IVec3,
    pub chunks: std::collections::HashSet<IVec2>,
    pub leaks: Vec<IVec3>,
    pub idle_ticks: u32,
    /// โดน invalidate — flush สถานะสุดท้ายแล้ว drop ใน tick ถัดไป
    pub dying: bool,
    /// absorption เพิ่ม volume นอก tick_pools — ต้อง recompute ผิวแม้ volume
    /// ไม่ต่างจากต้น tick
    volume_dirty: bool,
    segments: Vec<CapSegment>,
}

#[derive(Resource, Default)]
pub struct ActivePools(pub Vec<Pool>);

impl ActivePools {
    /// cell นี้เป็นสมาชิกสระไหนไหม — AABB ก่อน (ตัดเกือบทุก call) ค่อย lookup คอลัมน์
    pub fn member_of(&self, p: IVec3) -> Option<usize> {
        for (i, pool) in self.0.iter().enumerate() {
            if pool.dying { continue; }
            if p.x < pool.min.x || p.x > pool.max.x
                || p.y < pool.min.y || p.y > pool.max.y
                || p.z < pool.min.z || p.z > pool.max.z {
                continue;
            }
            if let Some(col) = pool.columns.get(&(p.x, p.z)) {
                if p.y >= col.y_bottom && p.y <= col.y_top {
                    return Some(i);
                }
            }
        }
        None
    }

    /// edit แตะเขตสระ (AABB พองขอบ 1) → ทิ้งสระ (โครงสร้างรอบน้ำเปลี่ยนแล้ว
    /// บัญชี capacity เชื่อไม่ได้อีก — ถ้าน้ำยังขยับเดี๋ยว form ใหม่เอง)
    pub fn invalidate_touching(&mut self, p: IVec3) {
        for pool in &mut self.0 {
            if p.x >= pool.min.x - 1 && p.x <= pool.max.x + 1
                && p.y >= pool.min.y - 1 && p.y <= pool.max.y + 1
                && p.z >= pool.min.z - 1 && p.z <= pool.max.z + 1 {
                pool.dying = true;
            }
        }
    }

    /// chunk นี้มีสระทับอยู่ไหม — ถ้ามี ตั้ง dying แล้วคืน true
    /// (ผู้เรียกควรเลื่อน unload chunk ออกไปก่อนจนสระ flush เสร็จ)
    pub fn mark_dying_overlapping(&mut self, cp: IVec2) -> bool {
        let mut any = false;
        for pool in &mut self.0 {
            if pool.chunks.contains(&cp) {
                pool.dying = true;
                any = true;
            }
        }
        any
    }
}

/// สร้างตารางความจุสะสมจากคอลัมน์ทั้งหมด (ครั้งเดียวตอน form)
fn build_cap_segments(columns: &HashMap<(i32, i32), PoolColumn>) -> Vec<CapSegment> {
    let mut events: Vec<(i64, i64)> = Vec::with_capacity(columns.len() * 2);
    for col in columns.values() {
        events.push((8 * col.y_bottom as i64, 1));
        events.push((8 * (col.y_top as i64 + 1), -1));
    }
    events.sort_unstable();
    let mut segs: Vec<CapSegment> = Vec::new();
    let mut active = 0i64;
    let mut cap = 0u64;
    let mut last_s = events.first().map(|e| e.0).unwrap_or(0);
    let mut idx = 0;
    while idx < events.len() {
        let s = events[idx].0;
        cap += (active.max(0) as u64) * ((s - last_s) as u64);
        while idx < events.len() && events[idx].0 == s {
            active += events[idx].1;
            idx += 1;
        }
        segs.push(CapSegment { s_start: s, cap_start: cap, active });
        last_s = s;
    }
    segs
}

/// ความจุรวมใต้ระดับ S
fn eval_cap(segs: &[CapSegment], s: i64) -> u64 {
    let i = segs.partition_point(|seg| seg.s_start <= s);
    if i == 0 {
        return 0;
    }
    let seg = &segs[i - 1];
    seg.cap_start + (seg.active.max(0) as u64) * ((s - seg.s_start).max(0) as u64)
}

/// ระดับผิวมากสุดที่ cap(S) <= volume (caller ต้องเช็ค volume เกินความจุรวมเอง)
fn surface_for_volume(segs: &[CapSegment], volume: u64) -> i64 {
    let i = segs.partition_point(|seg| seg.cap_start <= volume);
    if i == 0 {
        return segs.first().map(|s| s.s_start).unwrap_or(0);
    }
    let seg = &segs[i - 1];
    if seg.active <= 0 {
        return seg.s_start;
    }
    seg.s_start + ((volume - seg.cap_start) / seg.active as u64) as i64
}

/// พยายาม form สระจาก seed (ผิวน้ำลึกที่ settled) — คืน None ถ้าไม่เข้าเกณฑ์
/// เดินแบบ scanline ราย "คอลัมน์" ไม่ใช่ราย cell: หา run น้ำในคอลัมน์แล้วแผ่ 4 ทิศ
fn try_form_pool(seed: IVec3, world: &VoxelWorld, pools: &ActivePools) -> Option<Pool> {
    if !world.get_block(seed.x, seed.y, seed.z).is_water() || pools.member_of(seed).is_some() {
        return None;
    }

    let mut visited: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<(i32, i32, i32)> = std::collections::VecDeque::new();
    let mut columns: HashMap<(i32, i32), PoolColumn> = HashMap::new();
    let mut column_order: Vec<(i32, i32)> = Vec::new();
    let mut chunks: std::collections::HashSet<IVec2> = std::collections::HashSet::new();
    let mut leaks: Vec<IVec3> = Vec::new();
    let mut volume: u64 = 0;
    let mut cells: usize = 0;
    let mut min = seed;
    let mut max = seed;

    visited.insert((seed.x, seed.z));
    queue.push_back((seed.x, seed.z, seed.y));

    while let Some((x, z, y_hint)) = queue.pop_front() {
        // ส่วนของสระที่อยู่ใน chunk ที่ไม่โหลด = มองไม่เห็น นับบัญชีไม่ได้ — ยกเลิก
        let cp = IVec2::new(x.div_euclid(CHUNK_WIDTH as i32), z.div_euclid(CHUNK_WIDTH as i32));
        if !world.chunks.contains_key(&cp) {
            return None;
        }
        if !world.get_block(x, y_hint, z).is_water() {
            continue;
        }

        // run น้ำต่อเนื่องรอบ y_hint
        let mut y_bottom = y_hint;
        while y_bottom > 0 && world.get_block(x, y_bottom - 1, z).is_water() {
            y_bottom -= 1;
        }
        let mut y_top = y_hint;
        while y_top + 1 < CHUNK_HEIGHT as i32 && world.get_block(x, y_top + 1, z).is_water() {
            y_top += 1;
        }

        // ใต้ run เป็นอากาศ = คอลัมน์นี้คือน้ำที่กำลังร่วง (น้ำตก) —
        // ไม่รับเป็นสมาชิก แต่เป็นจุดรั่วของสระ ให้ cellular จัดการต่อ
        if y_bottom > 0 && world.get_block(x, y_bottom - 1, z) == BlockType::Air {
            leaks.push(IVec3::new(x, y_bottom, z));
            continue;
        }

        for y in y_bottom..=y_top {
            volume += get_water_vol(world.get_block(x, y, z)) as u64;
        }
        cells += (y_top - y_bottom + 1) as usize;
        if cells > POOL_CELL_CAP || columns.len() >= POOL_COLUMN_CAP {
            return None;
        }

        columns.insert((x, z), PoolColumn { y_bottom, y_top });
        column_order.push((x, z));
        chunks.insert(cp);
        min = min.min(IVec3::new(x, y_bottom, z));
        max = max.max(IVec3::new(x, y_top, z));

        // แผ่ 4 ทิศ: เชื่อมที่ y สูงสุดของ run ที่ฝั่งโน้นเป็นน้ำ
        // ระหว่างสแกนเก็บช่องอากาศข้างลำตัว (จุดรั่ว/ชายฝั่งใต้ระดับผิว)
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (x + dx, z + dz);
            let mut connect_y = None;
            for y in (y_bottom..=y_top).rev() {
                let nb = world.get_block(nx, y, nz);
                if nb.is_water() {
                    if connect_y.is_none() {
                        connect_y = Some(y);
                    }
                } else if nb == BlockType::Air && leaks.len() < POOL_MAX_LEAKS {
                    leaks.push(IVec3::new(nx, y, nz));
                }
            }
            if let Some(y) = connect_y {
                if visited.insert((nx, nz)) {
                    queue.push_back((nx, nz, y));
                }
            }
        }
    }

    if column_order.len() < POOL_MIN_COLUMNS {
        return None;
    }

    leaks.sort_unstable_by_key(|l| (l.x, l.y, l.z));
    leaks.dedup();

    let segments = build_cap_segments(&columns);
    let surface = surface_for_volume(&segments, volume);
    Some(Pool {
        columns,
        column_order,
        volume,
        surface,
        // เชื่อสถานะโลกตอน form ว่า ~ตรงกับ surface ที่คำนวณ (น้ำ settled อยู่แล้ว)
        // ถ้าคลาดเคลื่อนเล็กน้อย sweep รอบแรกจะเก็บให้เอง
        applied_surface: surface + 1, // บังคับ sweep ตรวจหนึ่ง lap แรกเสมอ
        lap_surface: surface,
        sweep_cursor: 0,
        min,
        max,
        chunks,
        leaks,
        idle_ticks: 0,
        dying: false,
        volume_dirty: false,
        segments,
    })
}

/// เขียนบล็อกน้ำในนามสระ: set_block + delta (host) + คิว remesh น้ำ + คืนสำเร็จไหม
fn pool_write(
    world: &mut VoxelWorld,
    pos: IVec3,
    block: BlockType,
    is_host: bool,
    net_out: &mut crate::network::PendingNetEdits,
    remesh_queue: &mut std::collections::HashSet<IVec2>,
) -> bool {
    if !world.set_block(pos.x, pos.y, pos.z, block) {
        return false;
    }
    if is_host {
        net_out.0.push_back((None, crate::network::BlockEdit::SetBlock {
            pos: pos.to_array(),
            block: block as u8,
        }));
    }
    remesh_queue.extend(edit_affected_chunks(pos));
    true
}

/// เขียนสถานะสุดท้ายของสระทั้งผืนตาม ledger (ตอน dying — ครั้งเดียว ไม่คิดงบ)
/// เศษ integer จากการ solve ระดับ (<8 ต่อสระ) เทเข้า cell แถวผิวไม่ให้น้ำหาย
fn flush_pool(
    pool: &Pool,
    world: &mut VoxelWorld,
    is_host: bool,
    net_out: &mut crate::network::PendingNetEdits,
    remesh_queue: &mut std::collections::HashSet<IVec2>,
    active_fluids: &mut ActiveFluids,
) {
    let mut leftover = pool.volume.saturating_sub(eval_cap(&pool.segments, pool.surface));
    let surface_cell_y = pool.surface.div_euclid(8);
    for &(cx, cz) in &pool.column_order {
        let col = &pool.columns[&(cx, cz)];
        for y in col.y_bottom..=col.y_top {
            let mut target = (pool.surface - 8 * y as i64).clamp(0, 8) as u8;
            if leftover > 0 && y as i64 == surface_cell_y && target < 8 {
                let add = leftover.min((8 - target) as u64);
                target += add as u8;
                leftover -= add;
            }
            let cur = world.get_block(cx, y, cz);
            // ห้ามทับ solid ที่ผู้เล่นเพิ่งวางแทรกเข้ามา
            if !(cur == BlockType::Air || cur.is_water()) {
                continue;
            }
            if get_water_vol(cur) != target {
                let p = IVec3::new(cx, y, cz);
                pool_write(world, p, vol_to_block(target), is_host, net_out, remesh_queue);
                // ปลุกให้ cellular รับช่วงต่อ — สระตายแล้วน้ำอาจยังต้องขยับ
                active_fluids.0.insert(p);
            }
        }
    }
    if leftover > 0 {
        warn!("pool flush เหลือเศษ {} หน่วย (ผิวเต็มพอดี) — ยอมทิ้ง", leftover);
    }
}

/// tick รายสระ: outflow ที่จุดรั่ว → recompute ระดับ → sweep เขียนแถบผิว → retire
fn tick_pools(
    pools: &mut ActivePools,
    world: &mut VoxelWorld,
    active_fluids: &mut ActiveFluids,
    remesh_queue: &mut std::collections::HashSet<IVec2>,
    net_out: &mut crate::network::PendingNetEdits,
    is_host: bool,
) {
    let mut budget = POOL_SWEEP_BUDGET;
    let mut i = 0;
    while i < pools.0.len() {
        if pools.0[i].dying {
            let pool = pools.0.swap_remove(i);
            info!(
                "pool ถูกทิ้ง: {} คอลัมน์ เหลือ {} หน่วย",
                pool.column_order.len(), pool.volume
            );
            flush_pool(&pool, world, is_host, net_out, remesh_queue, active_fluids);
            continue;
        }

        let pool = &mut pools.0[i];
        let vol_before = pool.volume;

        // --- ระบายออกที่จุดรั่ว (อัตราตามความลึกหัวน้ำ, เติมได้แค่ถึงระดับผิว) ---
        let mut li = 0;
        while li < pool.leaks.len() {
            let l = pool.leaks[li];
            if pool.volume == 0 {
                break;
            }
            let head = pool.surface - 8 * l.y as i64;
            let fill_cap = head.clamp(0, 8) as u8;
            let lb = world.get_block(l.x, l.y, l.z);
            let leak_open = lb == BlockType::Air || lb.is_water();
            if !leak_open || fill_cap == 0 {
                // โดนอุด (ผู้เล่นสร้างเขื่อนปิด) หรือลอยเหนือระดับผิวแล้ว — ตัดทิ้ง
                pool.leaks.swap_remove(li);
                continue;
            }
            let cur = get_water_vol(lb);
            if cur >= fill_cap {
                li += 1;
                continue;
            }
            let rate = (1 + head / 16).clamp(1, 8) as u8;
            let t = rate.min(fill_cap - cur).min(pool.volume.min(8) as u8);
            if t > 0 {
                let p_new = vol_to_block(cur + t);
                if pool_write(world, l, p_new, is_host, net_out, remesh_queue) {
                    pool.volume -= t as u64;
                    // ปลุก cellular รับน้ำที่จุดรั่วไปไหลต่อ
                    active_fluids.0.insert(l);
                    for dir in [
                        IVec3::new(1, 0, 0), IVec3::new(-1, 0, 0), IVec3::new(0, 1, 0),
                        IVec3::new(0, -1, 0), IVec3::new(0, 0, 1), IVec3::new(0, 0, -1),
                    ] {
                        active_fluids.0.insert(l + dir);
                    }
                } else {
                    pool.leaks.swap_remove(li);
                    continue;
                }
            }
            li += 1;
        }

        // --- ระดับผิวใหม่จากบัญชี ---
        if pool.volume != vol_before || pool.volume_dirty {
            pool.volume_dirty = false;
            let total_cap = pool.segments.last().map(|s| s.cap_start).unwrap_or(0);
            if pool.volume > total_cap {
                // น้ำจะล้นเกินขอบสระตอน form — สระเป็นตัวเร่งขาลง/เกลี่ยเท่านั้น
                // ขาขึ้นคืนให้ cellular แล้วค่อย form ใหม่ในขอบเขตใหม่
                pool.dying = true;
                i += 1;
                continue;
            }
            pool.surface = surface_for_volume(&pool.segments, pool.volume);
        }

        // --- sweep: ไล่เขียนบล็อกให้ตรง surface ทีละ lap ตามงบ ---
        if pool.applied_surface != pool.surface && budget > 0 {
            if pool.sweep_cursor == 0 {
                pool.lap_surface = pool.surface;
            }
            let total = pool.column_order.len();
            while budget > 0 {
                let (cx, cz) = pool.column_order[pool.sweep_cursor];
                let col = &pool.columns[&(cx, cz)];
                for y in (col.y_bottom..=col.y_top).rev() {
                    let target = (pool.lap_surface - 8 * y as i64).clamp(0, 8) as u8;
                    let cur = world.get_block(cx, y, cz);
                    if !(cur == BlockType::Air || cur.is_water()) {
                        continue; // solid แทรก — ไม่แตะ
                    }
                    if get_water_vol(cur) == target {
                        continue;
                    }
                    let p = IVec3::new(cx, y, cz);
                    if pool_write(world, p, vol_to_block(target), is_host, net_out, remesh_queue) {
                        budget = budget.saturating_sub(1);
                    }
                    if budget == 0 {
                        break;
                    }
                }
                if budget == 0 && pool.sweep_cursor != 0 {
                    break; // คอลัมน์นี้อาจยังไม่จบ — cursor ค้างไว้ทำต่อ tick หน้า
                }
                pool.sweep_cursor = (pool.sweep_cursor + 1) % total;
                if pool.sweep_cursor == 0 {
                    // ครบ lap — โลกตรงกับระดับ ณ ตอนเริ่ม lap แล้ว
                    pool.applied_surface = pool.lap_surface;
                    break;
                }
            }
        }

        // --- นิ่งครบกำหนด = retire เงียบๆ (บล็อกตรง ledger แล้ว ไม่ต้อง flush) ---
        let quiescent = pool.volume == vol_before && pool.applied_surface == pool.surface;
        if quiescent {
            pool.idle_ticks += 1;
        } else {
            pool.idle_ticks = 0;
        }
        if pool.idle_ticks >= POOL_IDLE_TICKS {
            info!(
                "pool retire: {} คอลัมน์ ปริมาตร {} หน่วย ผิว y*8={}",
                pool.column_order.len(), pool.volume, pool.surface
            );
            pools.0.swap_remove(i);
            continue;
        }
        i += 1;
    }
}

/// เพดาน remesh น้ำต่อ tick — เส้นทางเฉพาะน้ำถูกกว่าตัวเต็มมาก (สแกนแค่แถบ y
/// ที่มีน้ำ ไม่มี AO/greedy) เลยตั้งสูงกว่าเพดานเดิม 16 ได้สบาย
/// จำนวน chunk น้ำที่ remesh ต่อ "เฟรม" (ระบายคิวเรื่อยๆ ไม่ยิงก้อนใหญ่ต่อ tick
/// — เดิม 64/tick ทำเฟรมกระตุกเป็นจังหวะตอนน้ำทะลักลงหลุมระเบิด)
const WATER_REMESH_PER_FRAME: usize = 8;
/// งบ BFS หาทิศไหล (find_flow_dirs_finite) ต่อ tick — ตัวการหลักตอนน้ำท่วมหลุม:
/// active 20k cells × BFS ~130 cells = ล้าน lookups ต่อ tick; เกินงบใช้
/// การเทียบเพื่อนบ้านตรงๆ แทน (น้ำยังไหลตาม gradient แค่หาทางไกลไม่เก่งชั่วคราว)
const FLOW_BFS_BUDGET: usize = 1500;

fn queue_remesh(pos: IVec3, remesh_queue: &mut std::collections::HashSet<IVec2>) {
    // รวมเพื่อนบ้านเมื่อแตะขอบ chunk — ผิวน้ำเรียบ (drop smoothing) สุ่มมุมข้าม
    // seam ถ้าไม่ remesh ฝั่งโน้นด้วยมุมผิวจะค้างระดับเก่า (ของถูกลงแล้วทำได้)
    remesh_queue.extend(edit_affected_chunks(pos));
}

/// ปลุกน้ำตรงตะเข็บระหว่าง chunk ที่เพิ่งโหลดกับเพื่อนบ้านที่โหลดอยู่แล้ว —
/// น้ำที่เคยหลับเพราะปลายทางยังไม่โหลด (set_block ล้มเหลว) จะได้ไหลต่อ
/// ปลุกเฉพาะคู่ที่ไหลข้ามได้จริง: น้ำเจออากาศ หรือน้ำต่างระดับ ≥2
/// (ตะเข็บจาก generation ล้วนๆ เสมอกันพอดี เลยไม่ปลุกทะเลทั้งผืนโดยไม่จำเป็น)
pub fn wake_seam_water(world: &VoxelWorld, chunk_pos: IVec2, active_fluids: &mut ActiveFluids) {
    let Some(chunk) = world.chunks.get(&chunk_pos) else { return };
    let w = CHUNK_WIDTH;
    let base_x = chunk_pos.x * w as i32;
    let base_z = chunk_pos.y * w as i32;

    // (offset เพื่อนบ้าน, ฟังก์ชันแปลงตำแหน่งตามแนวขอบ i → (local เรา, local เขา))
    let sides: [(IVec2, fn(usize) -> ((usize, usize), (usize, usize))); 4] = [
        (IVec2::new(-1, 0), |i| ((0, i), (CHUNK_WIDTH - 1, i))),
        (IVec2::new(1, 0),  |i| ((CHUNK_WIDTH - 1, i), (0, i))),
        (IVec2::new(0, -1), |i| ((i, 0), (i, CHUNK_WIDTH - 1))),
        (IVec2::new(0, 1),  |i| ((i, CHUNK_WIDTH - 1), (i, 0))),
    ];

    for (offset, map_locals) in sides {
        let Some(neighbor) = world.chunks.get(&(chunk_pos + offset)) else { continue };
        let n_base_x = (chunk_pos.x + offset.x) * w as i32;
        let n_base_z = (chunk_pos.y + offset.y) * w as i32;

        for i in 0..w {
            let ((alx, alz), (blx, blz)) = map_locals(i);
            for y in 0..CHUNK_HEIGHT {
                // ทั้งสองฝั่ง section อากาศล้วน — ไม่มีน้ำให้ปลุกแน่ (เช็คถูกมาก)
                if chunk.blocks.section_is_air(y) && neighbor.blocks.section_is_air(y) {
                    continue;
                }
                let a = chunk.blocks.get(alx, y, alz);
                let b = neighbor.blocks.get(blx, y, blz);
                let (av, bv) = (get_water_vol(a), get_water_vol(b));

                // ฝั่งเราไหลไปหาเขาได้ไหม
                if a.is_water() && (b == BlockType::Air || (b.is_water() && bv + 1 < av)) {
                    active_fluids.0.insert(IVec3::new(base_x + alx as i32, y as i32, base_z + alz as i32));
                }
                // ฝั่งเขาไหลมาหาเราได้ไหม
                if b.is_water() && (a == BlockType::Air || (a.is_water() && av + 1 < bv)) {
                    active_fluids.0.insert(IVec3::new(n_base_x + blx as i32, y as i32, n_base_z + blz as i32));
                }
            }
        }
    }
}

fn vol_to_block(vol: u8) -> BlockType {
    match vol {
        8 => BlockType::Water8,
        7 => BlockType::Water7,
        6 => BlockType::Water6,
        5 => BlockType::Water5,
        4 => BlockType::Water4,
        3 => BlockType::Water3,
        2 => BlockType::Water2,
        1 => BlockType::Water1,
        _ => BlockType::Air,
    }
}

fn get_water_vol(block: BlockType) -> u8 {
    match block {
        BlockType::Water8 | BlockType::Water => 8,
        BlockType::Water7 => 7,
        BlockType::Water6 => 6,
        BlockType::Water5 => 5,
        BlockType::Water4 => 4,
        BlockType::Water3 => 3,
        BlockType::Water2 => 2,
        BlockType::Water1 => 1,
        _ => 0,
    }
}

/// รัศมี (บล็อก) ที่น้ำมองหาขอบผา/หลุมเพื่อไหลไปหา — ไกลขึ้น = น้ำฉลาดขึ้น
/// แต่ BFS แพงขึ้นเป็นกำลังสองของระยะ (8 → ~130 cells/ครั้ง ยังเบา)
const FLOW_SEARCH_DIST: i32 = 8;

fn find_flow_dirs_finite(pos: IVec3, world: &VoxelWorld, current_vol: u8) -> Vec<IVec3> {
    let horiz = [IVec3::new(1,0,0), IVec3::new(-1,0,0), IVec3::new(0,0,1), IVec3::new(0,0,-1)];
    let mut dirs = Vec::new();
    let mut min_dist = 100;
    
    let mut queue = std::collections::VecDeque::new();
    let mut visited = std::collections::HashSet::new();
    
    visited.insert(pos);
    
    for &dir in &horiz {
        let n_pos = pos + dir;
        let n_block = world.get_block(n_pos.x, n_pos.y, n_pos.z);
        if n_block.is_solid() { continue; }
        
        let n_vol = get_water_vol(n_block);
        if n_vol > current_vol { continue; }
        
        let b_pos = n_pos - IVec3::Y;
        let b_block = world.get_block(b_pos.x, b_pos.y, b_pos.z);
        let b_vol = get_water_vol(b_block);
        
        if b_block == BlockType::Air || (b_block.is_water() && b_vol < 8) {
            if 1 < min_dist {
                min_dist = 1;
                dirs.clear();
            }
            if 1 == min_dist {
                dirs.push(dir);
            }
        } else {
            queue.push_back((n_pos, 1, dir));
            visited.insert(n_pos);
        }
    }
    
    if !dirs.is_empty() { return dirs; }
    
    while let Some((curr, dist, first_dir)) = queue.pop_front() {
        if dist >= FLOW_SEARCH_DIST { continue; }
        
        for &dir in &horiz {
            let n_pos = curr + dir;
            if visited.contains(&n_pos) { continue; }
            let n_block = world.get_block(n_pos.x, n_pos.y, n_pos.z);
            if n_block.is_solid() { continue; }
            
            let n_vol = get_water_vol(n_block);
            if n_vol > current_vol { continue; }
            
            let b_pos = n_pos - IVec3::Y;
            let b_block = world.get_block(b_pos.x, b_pos.y, b_pos.z);
            let b_vol = get_water_vol(b_block);
            
            if b_block == BlockType::Air || (b_block.is_water() && b_vol < 8) {
                if dist + 1 < min_dist {
                    min_dist = dist + 1;
                    dirs.clear();
                }
                if dist + 1 == min_dist {
                    if !dirs.contains(&first_dir) {
                        dirs.push(first_dir);
                    }
                }
            } else {
                queue.push_back((n_pos, dist + 1, first_dir));
                visited.insert(n_pos);
            }
        }
    }
    
    dirs
}

pub fn fluid_simulation_system(
    mut active_fluids: ResMut<ActiveFluids>,
    mut remesh_queue: Local<std::collections::HashSet<IVec2>>,
    mut world: ResMut<VoxelWorld>,
    mut commands: Commands,
    mut mp: MeshingParams,
    net_server: Option<Res<bevy_renet::RenetServer>>,
    mut net_out: ResMut<crate::network::PendingNetEdits>,
    time: Res<Time>,
    settings: Res<crate::GameSettings>,
    mut pools: ResMut<ActivePools>,
    mut tick_accum: Local<f32>,
) {
    if active_fluids.0.is_empty() && remesh_queue.is_empty() && pools.0.is_empty() {
        return;
    }

    // ระบายคิว remesh น้ำทีละนิด "ทุกเฟรม" — งานเกลี่ยเรียบ ไม่ spike ตอน tick
    if !remesh_queue.is_empty() {
        let mut chunks = remesh_queue.drain().collect::<Vec<_>>();
        chunks.sort_by_key(|c| c.x * c.x + c.y * c.y);
        let overflow = chunks.split_off(chunks.len().min(WATER_REMESH_PER_FRAME));
        remesh_queue.extend(overflow);
        // chunk ที่เพื่อนบ้านยังไม่โหลด remesh ไม่ได้ — คืนเข้าคิวไว้ลองใหม่
        let skipped = remesh_water_only(&mut commands, &mut world, &mut mp, chunks);
        remesh_queue.extend(skipped);
    }

    // น้ำ simulate เป็นจังหวะคงที่ ไม่ใช่ทุกเฟรม — คาบปรับได้จาก settings UI
    // (ทุกเฟรมที่ 60fps น้ำจะแผ่ 60 บล็อก/วิ เร็วจนดูพัง แถม multiplayer
    // จะ broadcast delta ถี่เกินจน channel บวม)
    *tick_accum += time.delta_secs();
    if *tick_accum < settings.fluid_tick_seconds {
        return;
    }
    *tick_accum = 0.0;

    // ตอนเป็น host ทุกการเปลี่ยนบล็อกจากน้ำต้อง broadcast ให้ client
    // (client ไม่รันระบบนี้ — ดู run_if ใน main.rs)
    let is_host = net_server.is_some();

    let mut current_active: Vec<IVec3> = active_fluids.0.drain().collect();
    let mut next_active = std::collections::HashSet::new();

    // เกินงบ 20000 cells/tick ให้คืนเข้าคิวทำ tick หน้า — ห้ามทิ้ง
    // (เดิม take() แล้วทิ้งส่วนเกิน น้ำเลยแข็งค้างกลางทางเวลาไหลพร้อมกันเยอะๆ)
    if current_active.len() > 20000 {
        let overflow = current_active.split_off(20000);
        active_fluids.0.extend(overflow);
    }

    // seed สำหรับลอง form pool ปลายเฟรม (cell ผิวน้ำลึกที่นิ่ง)
    let mut pool_seed: Option<IVec3> = None;
    // งบ BFS หาทิศไหลของ tick นี้ — หมดแล้ว fallback เทียบเพื่อนบ้านตรงๆ
    let mut bfs_budget = FLOW_BFS_BUDGET;

    // Process fluids
    for pos in current_active.into_iter() {
        // cell สมาชิกสระ: ข้าม cellular ทั้งหมด — สระจัดการผ่านบัญชีรวมเอง
        if pools.member_of(pos).is_some() {
            continue;
        }
        let block = world.get_block(pos.x, pos.y, pos.z);

        // น้ำเป็น finite แท้ (conserve volume เสมอ ไม่มี infinite source) —
        // ตั้งใจเพื่อ gameplay สายเขื่อน/กักน้ำ: ตักออกระดับลดจริง เจาะบ่อน้ำหมดได้จริง
        if !block.is_water() { continue; }

        let vol = get_water_vol(block);
        let mut current_vol = vol;
        let mut moved = false;

        // Try to flow down first
        if pos.y > 0 {
            let b_pos = IVec3::new(pos.x, pos.y - 1, pos.z);
            // เทลงสระ: เข้าบัญชีรวมแทนการเขียนบล็อก (sweep ของสระจะสะท้อน
            // ระดับที่ขึ้นเอง) — conserve โดยโครงสร้าง
            if let Some(pi) = pools.member_of(b_pos) {
                let pool = &mut pools.0[pi];
                pool.volume += current_vol as u64;
                pool.volume_dirty = true;
                pool.idle_ticks = 0;
                current_vol = 0;
                moved = true;
            }
            let b_block = world.get_block(b_pos.x, b_pos.y, b_pos.z);
            if current_vol > 0 && (b_block == BlockType::Air || b_block.is_water()) {
                let b_vol = get_water_vol(b_block);
                if b_vol < 8 {
                    let transfer = current_vol.min(8 - b_vol);
                    let new_b_block = vol_to_block(b_vol + transfer);
                    // set สำเร็จเท่านั้นถึงหัก volume — chunk ปลายทางอาจยัง
                    // ไม่โหลด (get_block คืน Air หลอก) ไม่งั้นน้ำระเหยหายถาวร
                    if world.set_block(b_pos.x, b_pos.y, b_pos.z, new_b_block) {
                        current_vol -= transfer;
                        if is_host {
                            net_out.0.push_back((None, crate::network::BlockEdit::SetBlock {
                                pos: b_pos.to_array(), block: new_b_block as u8,
                            }));
                        }
                        queue_remesh(b_pos, &mut remesh_queue);
                        next_active.insert(b_pos);
                        moved = true;
                    }
                }
            }
        }

        // Spread horizontally ถ้ายังเหลือ volume และไม่ได้ไหลลงหมดไปแล้ว
        if current_vol == 1 && !moved {
            // หยดสุดท้าย: ปกตินอนเป็นคราบ แต่ถ้า BFS เจอที่ให้ตกในระยะค้นหา
            // ให้ย้ายทั้งหยดเดินตามทิศนั้น — เข้าใกล้จุดตกทุก tick เลยไม่ ping-pong
            // (ไม่มีทิศ = เป็นแอ่งจริง นอนนิ่งตามเดิม)
            // งบ BFS หมด = นอนรอ tick หน้า (โหลดหนักอยู่ — เดี๋ยวถึงคิว)
            let dirs = if bfs_budget > 0 {
                bfs_budget -= 1;
                find_flow_dirs_finite(pos, &world, current_vol)
            } else {
                next_active.insert(pos);
                Vec::new()
            };
            for dir in dirs {
                let n_pos = pos + dir;
                // เดินเข้าเขตสระ = ถูกดูดเข้าบัญชี
                if let Some(pi) = pools.member_of(n_pos) {
                    let pool = &mut pools.0[pi];
                    pool.volume += 1;
                    pool.volume_dirty = true;
                    pool.idle_ticks = 0;
                    current_vol = 0;
                    moved = true;
                    break;
                }
                let n_block = world.get_block(n_pos.x, n_pos.y, n_pos.z);
                if n_block.is_solid() { continue; }
                let n_vol = get_water_vol(n_block);
                if n_vol >= 8 { continue; }
                let new_t_block = vol_to_block(n_vol + 1);
                if world.set_block(n_pos.x, n_pos.y, n_pos.z, new_t_block) {
                    current_vol = 0;
                    if is_host {
                        net_out.0.push_back((None, crate::network::BlockEdit::SetBlock {
                            pos: n_pos.to_array(), block: new_t_block as u8,
                        }));
                    }
                    queue_remesh(n_pos, &mut remesh_queue);
                    next_active.insert(n_pos);
                    moved = true;
                    break;
                }
            }
        }
        if current_vol > 1 && !moved {
            // เกินงบ BFS → ใช้ 4 ทิศตรงๆ (เส้นทาง fallback เดิม gradient ยังพาไหลถูกทาง)
            let preferred_dirs = if bfs_budget > 0 {
                bfs_budget -= 1;
                find_flow_dirs_finite(pos, &world, current_vol)
            } else {
                Vec::new()
            };
            let check_dirs = if preferred_dirs.is_empty() {
                vec![IVec3::new(1, 0, 0), IVec3::new(-1, 0, 0), IVec3::new(0, 0, 1), IVec3::new(0, 0, -1)]
            } else {
                preferred_dirs
            };
            
            let mut neighbors = vec![];
            for dir in check_dirs {
                let n_pos = pos + dir;
                let n_block = world.get_block(n_pos.x, n_pos.y, n_pos.z);
                if n_block == BlockType::Air || n_block.is_water() {
                    let n_vol = get_water_vol(n_block);
                    // Use < current_vol - 1 to prevent ping-ponging!
                    if n_vol < current_vol - 1 {
                        neighbors.push((n_pos, n_vol));
                    }
                }
            }

            if !neighbors.is_empty() {
                neighbors.sort_by_key(|&(_, v)| v);
                let target = neighbors[0].0;
                let t_vol = neighbors[0].1;

                // เกลี่ยครึ่งหนึ่งของส่วนต่างแทนทีละ 1 — น้ำผิวบ่อไหลตามลงรู
                // ให้ทันตา ไม่ค้างเป็นสายนิ่งๆ (ยังนิ่งเมื่อเท่ากัน ไม่ ping-pong
                // เพราะเงื่อนไขเข้าลูปยังต้องต่าง ≥2 เหมือนเดิม)
                let transfer = (current_vol - t_vol) / 2;
                // เกลี่ยเข้าเขตสระ = ถูกดูดเข้าบัญชี
                if let Some(pi) = pools.member_of(target) {
                    let pool = &mut pools.0[pi];
                    pool.volume += transfer as u64;
                    pool.volume_dirty = true;
                    pool.idle_ticks = 0;
                    current_vol -= transfer;
                    moved = true;
                } else {
                let new_t_block = vol_to_block(t_vol + transfer);
                // เช็คผล set ก่อนหัก volume เหมือนตอนไหลลง
                if world.set_block(target.x, target.y, target.z, new_t_block) {
                    current_vol -= transfer;
                    if is_host {
                        net_out.0.push_back((None, crate::network::BlockEdit::SetBlock {
                            pos: target.to_array(), block: new_t_block as u8,
                        }));
                    }
                    queue_remesh(target, &mut remesh_queue);
                    next_active.insert(target);
                    moved = true;
                }
                } // else: ปลายทางไม่ใช่สระ
            }
        }

        let new_block = vol_to_block(current_vol);

        if new_block != block {
            world.set_block(pos.x, pos.y, pos.z, new_block);
            if is_host {
                net_out.0.push_back((None, crate::network::BlockEdit::SetBlock {
                    pos: pos.to_array(), block: new_block as u8,
                }));
            }
            queue_remesh(pos, &mut remesh_queue);
        }
        
        if moved {
            next_active.insert(pos);
            // volume ใน cell นี้เพิ่งว่างลง — น้ำข้างบน/ข้างๆ ที่หลับอยู่
            // อาจไหลเข้ามาแทนได้ ต้องปลุก ไม่งั้นเสาน้ำค้างลอยกลางอากาศ
            for dir in [
                IVec3::Y,
                IVec3::new(1, 0, 0), IVec3::new(-1, 0, 0),
                IVec3::new(0, 0, 1), IVec3::new(0, 0, -1),
            ] {
                next_active.insert(pos + dir);
            }
        } else if pool_seed.is_none()
            && current_vol == 8
            && world.get_block(pos.x, pos.y + 1, pos.z) == BlockType::Air
            && pos.y > 0
            && world.get_block(pos.x, pos.y - 1, pos.z).is_water()
        {
            // cell ผิวน้ำลึกที่นิ่งสนิท — ผู้ท้าชิงตำแหน่ง seed ของสระ
            pool_seed = Some(pos);
        }
    }

    active_fluids.0.extend(next_active);

    // --- Ephemeral pools ---
    // น้ำขยับพร้อมกันเยอะ = ผืนใหญ่กำลังเกลี่ย/ระบาย → ยกไปเข้าระบบสระ
    // (มากสุด 1 สระใหม่ต่อ tick — formation BFS มีค่าใช้จ่ายก้อนเดียวจบ)
    if active_fluids.0.len() > POOL_TRIGGER_ACTIVE && pools.0.len() < MAX_POOLS {
        if let Some(seed) = pool_seed {
            if let Some(pool) = try_form_pool(seed, &world, &pools) {
                info!(
                    "form pool: {} คอลัมน์ ปริมาตร {} หน่วย ผิว y*8={} จุดรั่ว {}",
                    pool.column_order.len(), pool.volume, pool.surface, pool.leaks.len()
                );
                pools.0.push(pool);
            }
        }
    }
    tick_pools(
        &mut pools, &mut world, &mut active_fluids,
        &mut remesh_queue, &mut net_out, is_host,
    );
    // remesh ชั้นน้ำถูกระบายรายเฟรมที่หัวฟังก์ชัน (ไม่ยิงก้อนใหญ่ท้าย tick แล้ว)
}

pub fn block_update_system(
    mut world: ResMut<VoxelWorld>,
    mut updates: ResMut<PendingBlockUpdates>,
    mut spawn_events: MessageWriter<crate::item::SpawnDroppedItemEvent>,
    mut net_out: ResMut<crate::network::PendingNetEdits>,
    mut pools: ResMut<ActivePools>,
    mut active_fluids: ResMut<ActiveFluids>,
    net_client: Option<Res<bevy_renet::RenetClient>>,
) {
    if net_client.is_some() {
        // client ไม่ตัดสินใจ cascade เอง — host จะส่ง SetBlock Air ตามมาให้ครบ
        // ซึ่งวิ่งผ่าน apply_block_edit → detach node ให้ถูกต้องอยู่แล้ว
        // (เคลียร์คิวทิ้งด้วย ไม่งั้น set โตไม่หยุดเพราะไม่มีใคร drain)
        world.pending_branch_orphans.clear();
        world.pending_leaf_decay.clear();
        updates.0.clear();
        return;
    }
    // เก็บออกมาก่อน เพราะระหว่างวนอาจต้องใส่ตำแหน่งใหม่กลับเข้าคิวรอบหน้า
    let pending: Vec<IVec3> = updates.0.drain().collect();
    for p in pending {
        let block = world.get_block(p.x, p.y, p.z);
        if block == BlockType::TallGrass {
            let below = world.get_block(p.x, p.y - 1, p.z);
            if below != BlockType::Grass && below != BlockType::Dirt {
                // ฐานหาย -> ทุบหญ้าทิ้ง
                world.set_block(p.x, p.y, p.z, BlockType::Air);
                spawn_events.write(crate::item::SpawnDroppedItemEvent {
                    item: crate::item::Item::Block(BlockType::TallGrass),
                    pos: p.as_vec3() + Vec3::new(0.5, 0.5, 0.5),
                    velocity: Vec3::new(
                        (fastrand::f32() - 0.5) * 4.0,
                        2.0 + fastrand::f32() * 3.0,
                        (fastrand::f32() - 0.5) * 4.0,
                    ),
                });
                net_out.0.push_back((None, crate::network::BlockEdit::SetBlock {
                    pos: p.to_array(),
                    block: BlockType::Air as u8,
                }));
            }
        }
        
        // บล็อกกิ่งที่ไม่มี node เลย (เซฟเก่าก่อนมีระบบนี้ / desync) — รับเลี้ยงเข้า
        // network แทนที่จะทำลายทิ้ง เพื่อไม่ให้สิ่งที่ผู้เล่นสร้างไว้หายไปดื้อๆ
        if block == BlockType::Branch && !world.branch_network.nodes.contains_key(&p) {
            attach_branch_node(&mut world, p);
            world.pending_branch_remesh.extend(edit_affected_chunks(p));
        }
    }

    // --- Cascade กิ่งที่ขาดที่ยึด ---
    // ขับด้วยคิว pending_branch_orphans ล้วนๆ (เติมตอน detach จริงเท่านั้น) —
    // ห้ามใช้ "parent ไม่อยู่ใน network" เป็นเงื่อนไข เพราะกิ่งที่พาดข้าม chunk จะมี
    // parent อยู่คนละ chunk ที่ยัง unload อยู่เป็นเรื่องปกติ
    let orphans: Vec<IVec3> = world.pending_branch_orphans.drain().collect();
    for o in orphans {
        // chunk ไม่ได้โหลด = node ถูก evict ไปแล้ว แก้บล็อกไม่ได้ ปล่อยผ่าน
        // (กิ่งจะค้างลอยจนกว่าผู้เล่นจะไปทุบเอง — ยอมได้ ดีกว่าคิวโตไม่หยุด)
        let ocp = crate::tree::chunk_of(o, CHUNK_WIDTH as i32);
        if !world.chunks.contains_key(&ocp) {
            continue;
        }
        if world.get_block(o.x, o.y, o.z) != BlockType::Branch {
            // บล็อกหายไปทางอื่นแล้ว (ระเบิด/ทับ) — เก็บ node ทิ้งแต่ยังต้องส่งลูกไปต่อคิว
            let next = world.branch_network.detach(o);
            world.pending_branch_orphans.extend(next);
            continue;
        }

        world.set_block(o.x, o.y, o.z, BlockType::Air);
        // ลูกของมันกลายเป็นกำพร้าต่อ → เข้าคิวรอบหน้า cascade จึงไหลลงทีละชั้นต่อเฟรม
        let next = world.branch_network.detach(o);
        world.pending_branch_orphans.extend(next);
        queue_leaf_decay_around(&mut world, o);

        spawn_events.write(crate::item::SpawnDroppedItemEvent {
            item: crate::item::Item::Block(BlockType::Branch),
            pos: o.as_vec3() + Vec3::splat(0.5),
            velocity: Vec3::new(
                (fastrand::f32() - 0.5) * 4.0,
                2.0 + fastrand::f32() * 3.0,
                (fastrand::f32() - 0.5) * 4.0,
            ),
        });
        net_out.0.push_back((None, crate::network::BlockEdit::SetBlock {
            pos: o.to_array(),
            block: BlockType::Air as u8,
        }));

        // ปลุกเพื่อนบ้านเหมือน edit ปกติ — หญ้าที่เกาะอยู่บนกิ่งจะได้ร่วง น้ำจะได้ไหลลงช่องว่าง
        pools.invalidate_touching(o);
        active_fluids.0.insert(o);
        updates.0.insert(o);
        for d in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
            active_fluids.0.insert(o + d);
            updates.0.insert(o + d);
        }

        world.pending_branch_save.insert(ocp);
        world.pending_branch_remesh.extend(edit_affected_chunks(o));
    }

    // --- ใบร่วงเมื่อกิ่งที่เกาะอยู่หายไป ---
    // จำกัดจำนวนต่อเฟรมเพราะการเช็คที่ยึดต้องสแกนกล่อง 7×7×7 ต่อใบหนึ่งใบ —
    // ตัดต้นใหญ่ทีเดียวมีใบหลายร้อย ถ้าทำรวดเดียวจะกระตุก (และการทยอยร่วงก็ดูดีกว่า)
    const LEAF_DECAY_PER_FRAME: usize = 48;
    let mut leaves: Vec<IVec3> = world.pending_leaf_decay.iter().copied().collect();
    leaves.sort_unstable_by_key(|p| p.to_array()); // deterministic ระหว่าง host/client
    leaves.truncate(LEAF_DECAY_PER_FRAME);
    for l in leaves {
        world.pending_leaf_decay.remove(&l);
        if world.get_block(l.x, l.y, l.z) != BlockType::Leaves {
            continue;
        }
        if leaf_has_support(&world, l) {
            continue;
        }
        world.set_block(l.x, l.y, l.z, BlockType::Air);
        // ใบข้างเคียงอาจขาดที่ยึดตามไปด้วย → การร่วงลามไปทั้งพุ่มเอง
        queue_leaf_decay_around(&mut world, l);
        net_out.0.push_back((None, crate::network::BlockEdit::SetBlock {
            pos: l.to_array(),
            block: BlockType::Air as u8,
        }));
        // ใบร่วงไม่ดรอปไอเทม — ตัดต้นเดียวมีใบเป็นร้อย จะกลายเป็นขยะเกลื่อนพื้น
        world.pending_branch_save.insert(crate::tree::chunk_of(l, CHUNK_WIDTH as i32));
        world.pending_branch_remesh.extend(edit_affected_chunks(l));
    }
}

/// Drain pending_branch_save/remesh → เขียน chunk ที่ branch cascade แก้ไว้ลงดิสก์
/// แล้ว remesh (ถ้าไม่เซฟ โหลดโลกใหม่กิ่งที่หักไปจะกลับมาแต่ node หายไปแล้ว = desync)
pub fn branch_remesh_system(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    mut mp: MeshingParams,
    net_client: Option<Res<bevy_renet::RenetClient>>,
) {
    if !world.pending_branch_save.is_empty() {
        let dirty: Vec<IVec2> = world.pending_branch_save.drain().collect();
        // client ไม่เขียนทับเซฟ single player ของตัวเอง (เหมือน path edit ปกติ)
        if net_client.is_none() {
            for cp in dirty {
                save_loaded_chunk(&world, cp);
            }
        }
    }
    if world.pending_branch_remesh.is_empty() {
        return;
    }
    // remesh ทางนี้เป็น sync บน main thread และวัดได้ ~5.5 ms ต่อ chunk — ทำได้เฟรมละ
    // ตัวเดียวเท่านั้น (เคยตั้ง 4 แล้ววัดได้ 22 ms/เฟรมค้างตลอดตอน stream = เพดาน 45fps
    // และเป็นต้นเหตุภาพกระพริบ)
    const REMESH_BUDGET: usize = 1;
    let chunks: Vec<IVec2> =
        world.pending_branch_remesh.iter().copied().take(REMESH_BUDGET).collect();
    for cp in &chunks {
        world.pending_branch_remesh.remove(cp);
    }
    // chunk ที่เพื่อนบ้านยังไม่ครบถูก skip — **ทิ้งไปเลย ห้ามใส่กลับคิว** เพราะ chunk
    // ริมขอบ render distance ไม่มีวันมีเพื่อนบ้านครบ จะวนอยู่ในคิวถาวรและโตขึ้นเรื่อยๆ
    // ตอนผู้เล่นเดิน (เคยเห็นค้างที่ 164 ตัว) — ถ้าเพื่อนบ้านมาถึงทีหลัง การ insert chunk
    // จะตีธง light_dirty ให้ แล้ว relight_system จะเข้าคิว remesh ให้เองอยู่แล้ว
    let _ = remesh_chunks(&mut commands, &mut world, &mut mp, chunks);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(blocks: &mut ChunkBlocks, x: usize, y: usize, z: usize, b: BlockType) {
        blocks.set(x, y, z, b);
    }

    /// VoxelWorld ที่มี chunk (0,0) เป็นอากาศล้วนหนึ่งก้อน — พอให้ set_block ทำงานได้
    fn world_with_one_chunk() -> VoxelWorld {
        let mut world = VoxelWorld::default();
        world.chunks.insert(IVec2::ZERO, ChunkData {
            blocks: Arc::new(ChunkBlocks::new_uniform(BlockType::Air)),
            chiseled_blocks: HashMap::new(),
            facings: HashMap::new(),
            chest_slots: HashMap::new(),
            furnace_slots: HashMap::new(),
            num_vertices: 0,
            num_indices: 0,
            water_y_min: 1,
            water_y_max: 0,
            num_water_vertices: 0,
            num_water_indices: 0,
            dirty: false,
            light: Default::default(),
            light_dirty: true,
            light_missing_neighbors: 0,
        });
        world
    }

    fn set_block_edit(world: &mut VoxelWorld, p: IVec3, block: BlockType) -> Option<IVec3> {
        apply_block_edit(world, &crate::network::BlockEdit::SetBlock {
            pos: p.to_array(),
            block: block as u8,
        })
    }

    /// ทุบกิ่งกลางต้น → ทุกกิ่งเหนือขึ้นไปต้องร่วงตามเป็นทอดๆ ส่วนตอที่ยังติดดินต้องอยู่
    /// (ทดสอบผ่าน App จริงเพื่อกันพลาดเรื่อง SystemParam/resource ที่ cargo check ไม่จับ)
    #[test]
    fn branch_break_cascades_up_the_tree() {
        let mut world = world_with_one_chunk();
        world.set_block(2, 0, 2, BlockType::Dirt);

        let stack: Vec<IVec3> = (1..=4).map(|y| IVec3::new(2, y, 2)).collect();
        for p in &stack {
            assert_eq!(set_block_edit(&mut world, *p, BlockType::Branch), Some(*p));
        }
        // ต่อกันเป็นสายเดียวและบางลงทีละ 2 ตามระยะจากดิน
        assert_eq!(world.branch_network.thickness_at(stack[0]), Some(crate::tree::TRUNK_THICKNESS));
        let mut expect = crate::tree::TRUNK_THICKNESS;
        for _ in 0..3 {
            expect = crate::tree::child_thickness(expect);
        }
        assert_eq!(world.branch_network.thickness_at(stack[3]), Some(expect));
        assert!(expect < crate::tree::TRUNK_THICKNESS, "ต้องเรียวลงจริง");

        // ทุบตัวที่สองจากล่าง
        set_block_edit(&mut world, stack[1], BlockType::Air);

        let mut app = app_with(world);
        // cascade ไหลชั้นละเฟรม — วนให้เกินความสูงต้นไม้
        for _ in 0..8 {
            app.update();
        }

        let world = app.world().resource::<VoxelWorld>();
        for p in &stack[1..] {
            assert_eq!(world.get_block(p.x, p.y, p.z), BlockType::Air, "{p} ต้องร่วงตาม");
            assert!(!world.branch_network.nodes.contains_key(p), "{p} ต้องไม่เหลือ node ค้าง");
        }
        assert_eq!(
            world.get_block(stack[0].x, stack[0].y, stack[0].z),
            BlockType::Branch,
            "ตอที่ยังติดดินต้องไม่ถูกทำลาย"
        );
        assert!(
            world.pending_branch_save.contains(&IVec2::ZERO),
            "chunk ที่ cascade แก้ต้องถูกจ่อเซฟลงดิสก์"
        );
    }

    /// ตัวต่อของ node สองตัวที่ติดกันต้องปูเต็มระยะห่างระหว่างศูนย์กลางพอดี ไม่เหลือ
    /// ช่วงว่างตรงกลาง — ช่วงว่างนั้นแหละที่เคยทำให้กิ่งเฉียงดูเป็นลูกปัดร้อยเชือก
    /// (คิวบ์สองก้อนที่ติดกันแบบเฉียงแตะกันแค่ขอบ ไม่ได้ชนกันจริง)
    #[test]
    fn extensions_of_adjacent_nodes_tile_the_whole_gap() {
        for dir in crate::tree::NEIGHBOUR_DIRS {
            let gap = dir.as_vec3().length();
            let (min_a, max_a) = extension_span(dir);
            let (min_b, max_b) = extension_span(-dir);
            assert_eq!(min_a, 0.0, "{dir:?}: ตัวต่อไม่ได้เริ่มจากใจกลาง node");
            assert_eq!(min_b, 0.0, "{dir:?}: ฝั่งตรงข้ามไม่ได้เริ่มจากใจกลาง node");
            assert!(max_a > 0.0, "{dir:?}: ตัวต่อยาวศูนย์");
            assert!(
                (max_a + max_b - gap).abs() < 1e-5,
                "{dir:?}: สองฝั่งรวมกัน {} แต่ระยะห่างจริง {gap} — เหลือคอคอด/ซ้อนเกิน",
                max_a + max_b
            );
        }
    }

    /// สไตล์ของหน้า preview — แยกออกมาเป็นค่าคงที่เพราะ CSS เต็มไปด้วยปีกกา
    /// ซึ่งต้องหนีอักขระถ้าอยู่ใน format!
    const TREE_PREVIEW_CSS: &str = r#"<style>
:root{
  --paper:#EDEFE8; --panel:#F7F8F3; --ink:#1A211C; --muted:#5D6A5C;
  --bark:#8A6A44; --leaf:#4F8046; --rule:#C6CCBD;
}
@media (prefers-color-scheme:dark){
  :root{ --paper:#141811; --panel:#1B2016; --ink:#E4E8DC; --muted:#94A08F;
         --bark:#B08856; --leaf:#6FA262; --rule:#333B2E; }
}
:root[data-theme="dark"]{ --paper:#141811; --panel:#1B2016; --ink:#E4E8DC; --muted:#94A08F;
  --bark:#B08856; --leaf:#6FA262; --rule:#333B2E; }
:root[data-theme="light"]{ --paper:#EDEFE8; --panel:#F7F8F3; --ink:#1A211C; --muted:#5D6A5C;
  --bark:#8A6A44; --leaf:#4F8046; --rule:#C6CCBD; }

body{ background:var(--paper); color:var(--ink); margin:0;
  padding:clamp(20px,4vw,56px); font:15px/1.6 ui-sans-serif,system-ui,sans-serif; }
h1{ font:italic 400 clamp(26px,4vw,38px)/1.15 Georgia,"Times New Roman",serif;
  margin:0 0 10px; text-wrap:balance; letter-spacing:-.01em; }
.lede{ max-width:64ch; color:var(--muted); margin:0 0 40px; }
code{ font:12.5px/1 ui-monospace,"Cascadia Mono",Consolas,monospace; color:var(--bark); }

.plate{ border-top:1px solid var(--rule); padding-top:20px; margin-bottom:44px; }
.plate header{ display:flex; align-items:baseline; gap:14px; flex-wrap:wrap; }
h2{ font:italic 400 22px/1.2 Georgia,"Times New Roman",serif; margin:0; }
.note{ margin:0; color:var(--muted); font-size:14px; }

.params{ display:flex; flex-wrap:wrap; gap:0 26px; margin:12px 0 0; }
.params div{ display:flex; align-items:baseline; gap:6px; }
.params dt{ font-size:11px; letter-spacing:.09em; text-transform:uppercase; color:var(--muted); }
.params dd{ margin:0; font:13px/1 ui-monospace,"Cascadia Mono",Consolas,monospace;
  font-variant-numeric:tabular-nums; }

.row{ display:flex; gap:10px; margin-top:16px; overflow-x:auto; padding-bottom:6px; }
svg{ background:var(--panel); border:1px solid var(--rule); border-radius:3px;
  width:158px; height:250px; flex:0 0 auto; }
.ground{ stroke:var(--rule); stroke-width:1.5; }
.leaf{ fill:var(--leaf); }
.branch{ stroke:var(--bark); stroke-linecap:round; }

.legend{ border-top:1px solid var(--rule); padding-top:20px; max-width:72ch; }
.legend dl{ margin:14px 0 0; display:grid; gap:10px; }
.legend div{ display:grid; grid-template-columns:88px 1fr; gap:14px; align-items:baseline; }
.legend dt{ font-size:11px; letter-spacing:.09em; text-transform:uppercase; color:var(--muted); }
.legend dd{ margin:0; }
@media (max-width:520px){ .legend div{ grid-template-columns:1fr; gap:2px; } }
</style>"#;

    /// เครื่องมือดูทรงต้นไม้: ปั้นต้นไม้จริงจากทุก preset แล้วเขียนภาพ SVG เทียบกัน
    /// ลง `target/tree_previews.html` (อยู่ใน target/ จึงไม่ปนกับ repo)
    /// — ใช้จูน TREE_PRESETS โดยไม่ต้องเปิดเกมทุกครั้ง
    #[test]
    fn dump_tree_previews() {
        // ฉายด้านข้าง (x, y) — กิ่งวาดเป็นเส้นจาก parent ไปลูก ความหนาตาม thickness
        fn svg_for(params: &TreeParams, seed: u64) -> String {
            let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = move || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            };
            let mut blocks = ChunkBlocks::new_uniform(BlockType::Air);
            let mut records = Vec::new();
            let base = IVec3::new(8, 40, 8);
            grow_tree(&mut blocks, &mut records, base, params, &mut next);

            let by_pos: HashMap<[i32; 3], u8> =
                records.iter().map(|r| (r.pos, r.thickness)).collect();
            let mut out = String::new();
            let scale = 9.0;
            let (w, h) = (16.0 * scale, 26.0 * scale);
            // y โลกชี้ขึ้น แต่ y ของ SVG ชี้ลง — พลิกและอิงโคนต้นเป็นเส้นพื้น
            let px = |p: [i32; 3]| {
                (
                    (p[0] as f32 + (p[2] as f32 - 8.0) * 0.35) * scale,
                    h - 2.0 * scale - (p[1] - base.y) as f32 * scale,
                )
            };
            // เผื่อขอบซ้าย/ขวา — กิ่งที่กางออกกว้างเลยกรอบ 16 บล็อกไปได้เล็กน้อย
            let pad = 3.0 * scale;
            out.push_str(&format!(
                r#"<svg viewBox="{vx} 0 {vw} {h}" preserveAspectRatio="xMidYMax meet">"#,
                vx = -pad,
                vw = w + pad * 2.0
            ));
            out.push_str(&format!(
                r#"<line x1="{x1}" y1="{gy}" x2="{x2}" y2="{gy}" class="ground"/>"#,
                x1 = -pad,
                x2 = w + pad,
                gy = h - 2.0 * scale
            ));
            // วาดจากหลังไปหน้า ของใกล้จึงทับของไกล และจางลงตามความลึกให้รู้สึกมีปริมาตร
            enum Item {
                Leaf(f32, f32),
                Branch(f32, f32, f32, f32, f32),
            }
            let mut items: Vec<(i32, Item)> = Vec::new();
            blocks.for_each_matching(|b| b == BlockType::Leaves, |x, y, z, _| {
                let (cx, cy) = px([x as i32, y as i32, z as i32]);
                items.push((z as i32, Item::Leaf(cx, cy)));
            });
            for r in &records {
                let Some(parent) = r.parent else { continue };
                let (x1, y1) = px(parent);
                let (x2, y2) = px(r.pos);
                let t = by_pos.get(&parent).copied().unwrap_or(r.thickness).max(r.thickness);
                items.push((
                    r.pos[2],
                    Item::Branch(x1, y1, x2, y2, t as f32 / 32.0 * 2.0 * scale),
                ));
            }
            items.sort_by_key(|(z, _)| *z);
            for (z, item) in &items {
                // z 0..15 → ไกลสุดจาง, ใกล้สุดทึบ
                let depth = (*z as f32 / 15.0).clamp(0.0, 1.0);
                match item {
                    Item::Leaf(cx, cy) => out.push_str(&format!(
                        r#"<circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}" class="leaf" opacity="{o:.2}"/>"#,
                        r = scale * 0.6,
                        o = 0.22 + depth * 0.3
                    )),
                    Item::Branch(x1, y1, x2, y2, sw) => out.push_str(&format!(
                        r#"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke-width="{sw:.2}" class="branch" opacity="{o:.2}"/>"#,
                        o = 0.55 + depth * 0.45
                    )),
                }
            }
            out.push_str("</svg>");
            out
        }

        // หน้าตาแบบ "แผ่นภาพคู่มือพรรณไม้" — แต่ละ preset คือหนึ่งชนิด วางเรียงบนเส้นพื้น
        // เดียวกันให้เทียบสัดส่วนกันได้ตรงๆ
        let mut html = String::new();
        html.push_str(TREE_PREVIEW_CSS);
        html.push_str(
            "<h1>พรรณไม้ที่เลือกได้</h1>\
             <p class=lede>ทุกต้นในหน้านี้ปั้นจาก generator ตัวจริงใน <code>TREE_PRESETS</code> \
             ฉายด้านข้างพร้อมเหลื่อมความลึกเล็กน้อย ความหนาของเส้น = <code>thickness</code> \
             ของ node นั้นจริงๆ แต่ละชนิดแสดง 6 เมล็ดสุ่มเพื่อให้เห็นความหลากหลายในชนิดเดียวกัน</p>",
        );
        for (name, params) in TREE_PRESETS {
            let note = match *name {
                "oak" => "ลำต้นสั้น กิ่งกางกว้าง พุ่มหนา — เดินลอดได้ เงาเยอะ",
                "pine" => "ลำต้นสูงเด่น กิ่งสั้นถี่ตลอดลำต้น ทรงกรวย",
                "birch" => "สูงเรียว กิ่งน้อยเชิดขึ้น พุ่มแคบ โปร่ง",
                _ => "ลำต้นคดสั้น กิ่งเยอะแตกมั่ว ใบเป็นหย่อม — ป่าดิบ/ไม้แก่",
            };
            html.push_str(&format!(
                "<section class=plate><header><h2>{name}</h2><p class=note>{note}</p></header>\
                 <dl class=params>\
                 <div><dt>ลำต้น</dt><dd>{}–{}</dd></div>\
                 <div><dt>ชั้นกิ่ง</dt><dd>{}</dd></div>\
                 <div><dt>กิ่งข้าง</dt><dd>{:.0}%</dd></div>\
                 <div><dt>กาง</dt><dd>{:.2}</dd></div>\
                 <div><dt>ส่าย</dt><dd>{:.2}</dd></div>\
                 <div><dt>เชิด</dt><dd>{:.2}</dd></div>\
                 </dl><div class=row>",
                params.trunk_len.0, params.trunk_len.1, params.max_depth,
                params.side_branch_chance * 100.0, params.tilt, params.wobble, params.climb
            ));
            for seed in 0..6u64 {
                html.push_str(&svg_for(params, seed * 7919 + 13));
            }
            html.push_str("</div></section>");
        }
        html.push_str(
            "<section class=legend><h2>ปุ่มที่หมุนได้</h2><dl>\
             <div><dt>ลำต้น</dt><dd>ความยาวก่อนถึงยอด — ยาว = ต้นสูงโปร่ง, สั้น = พุ่มเตี้ย</dd></div>\
             <div><dt>ชั้นกิ่ง</dt><dd>กิ่งแตกซ้อนได้กี่ชั้น — มาก = รกและ vertex เยอะ</dd></div>\
             <div><dt>กิ่งข้าง</dt><dd>โอกาสแตกกิ่งระหว่างทาง ไม่ใช่แตกที่ยอดจุดเดียว \
             ตัวนี้คือตัวที่กันไม่ให้ต้นไม้ออกมาเป็นทรงไม้กวาด</dd></div>\
             <div><dt>กาง</dt><dd>มุมที่กิ่งเบนออกจากแกนตั้ง — สูง = แผ่ออกข้าง, ต่ำ = พุ่งขึ้น</dd></div>\
             <div><dt>ส่าย</dt><dd>ความคดของกิ่งรายก้าว — สูง = บิดเบี้ยวเป็นธรรมชาติ, ต่ำ = ตรงเป๊ะ</dd></div>\
             <div><dt>เชิด</dt><dd>แรงดึงขึ้นบนรายก้าว — สูง = ปลายกิ่งชูขึ้น, ต่ำ = กิ่งทิ้งตัว</dd></div>\
             </dl></section>",
        );

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("tree_previews.html");
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        std::fs::write(&path, html).expect("เขียนไฟล์ preview ไม่ได้");
        assert!(path.exists());
    }

    /// pipeline เต็มของต้นไม้ที่ worldgen ปั้น: generate → record → merge เข้า network
    /// ต้องรักษา thickness ของลำต้นไว้ครบ ถ้าหลุดตรงไหน mesh จะ fallback เป็นกิ่งผอม
    #[test]
    fn generated_trunk_keeps_its_thickness_through_the_pipeline() {
        let noise = crate::NoiseParams { frequency: 0.01, amplitude: 24.0, octaves: 4, seed: 1337 };
        let mut checked = 0;
        for cx in 0..12 {
            for cz in 0..12 {
                let (blocks, records) =
                    generate_chunk_blocks(IVec2::new(cx, cz), noise, crate::TerrainSource::Noise);
                if records.is_empty() {
                    continue;
                }
                let mut net = crate::tree::BranchNetwork::default();
                net.merge_records(&records);

                let cp = IVec2::new(cx, cz);
                for r in &records {
                    let p = IVec3::from_array(r.pos);
                    // record ต้องเป็นพิกัด world — ถ้าเป็น local ทุกอันจะตกไปอยู่ chunk (0,0)
                    // แล้ว mesh (ซึ่ง lookup ด้วยพิกัด world) หา node ไม่เจอ
                    assert_eq!(
                        crate::tree::chunk_of(p, CHUNK_WIDTH as i32),
                        cp,
                        "record หลุดออกนอก chunk ตัวเอง — น่าจะลืมแปลง local → world"
                    );
                    let lx = p.x.rem_euclid(CHUNK_WIDTH as i32) as usize;
                    let lz = p.z.rem_euclid(CHUNK_WIDTH as i32) as usize;
                    assert_eq!(
                        blocks.get(lx, p.y as usize, lz),
                        BlockType::Branch,
                        "มี node แต่ไม่มีบล็อกกิ่งตรงนั้น"
                    );
                }
                for root in records.iter().filter(|r| r.parent.is_none()) {
                    let p = IVec3::from_array(root.pos);
                    assert_eq!(
                        net.thickness_at(p),
                        Some(crate::tree::TRUNK_THICKNESS),
                        "โคนลำต้นต้องหนาเต็มหลัง merge"
                    );
                }
                // บล็อกถัดขึ้นไปจากโคนต้องยังหนาเกือบเต็ม ไม่ใช่ตกไปเป็นกิ่งผอม
                let second = records.iter().find(|r| r.parent.is_some()).unwrap();
                assert!(
                    second.thickness >= crate::tree::TRUNK_THICKNESS - 2,
                    "ลำต้นเรียวเร็วเกินไป: {}",
                    second.thickness
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "ไม่เจอต้นไม้เลยใน 144 chunk — ตัวปั้นอาจไม่ทำงาน");
    }

    /// ขอบทุกเส้นของ mesh ต้องถูกใช้เป็นจำนวนคู่ — ขอบที่ถูกใช้หนเดียวคือขอบเปิด
    /// แปลว่ามีรู (ผิวซ้อนทับกันได้ นับเป็นเลขคู่ จึงยอมให้ solid หลายก้อนซ้อนกัน)
    fn open_edge_count(set: &ChunkMeshSet) -> usize {
        let key = |p: [f32; 3]| {
            [
                (p[0] * 4096.0).round() as i64,
                (p[1] * 4096.0).round() as i64,
                (p[2] * 4096.0).round() as i64,
            ]
        };
        let mut edges: HashMap<([i64; 3], [i64; 3]), usize> = HashMap::new();
        for (_, buf) in &set.textured {
            for tri in buf.indices.chunks(3) {
                for i in 0..3 {
                    let a = key(buf.positions[tri[i] as usize]);
                    let b = key(buf.positions[tri[(i + 1) % 3] as usize]);
                    let e = if a <= b { (a, b) } else { (b, a) };
                    *edges.entry(e).or_default() += 1;
                }
            }
        }
        edges.values().filter(|c| *c % 2 != 0).count()
    }

    /// node เดี่ยวที่ทุกปลายไม่มีเพื่อนบ้าน (ปลายทุกด้านมีฝาปิด) ต้องเป็นก้อนตัน
    /// ครอบทุกทรงที่เกิดจริง: ปลายกิ่ง, กิ่งตรง, กิ่งหักมุม, จุดแตกกิ่ง แกนตรงและเฉียง
    #[test]
    fn branch_mesh_has_no_holes() {
        let axis = IVec3::Y;
        let diag_edge = IVec3::new(1, 1, 0);
        let diag_corner = IVec3::new(1, 1, 1);
        let cases: Vec<(&str, Option<(IVec3, Option<u8>)>, Vec<(IVec3, Option<u8>)>)> = vec![
            ("ปลายกิ่งแนวแกน", Some((IVec3::NEG_Y, None)), vec![]),
            ("ปลายกิ่งแนวเฉียง", Some((-diag_edge, None)), vec![]),
            ("กิ่งตรงแนวแกน", Some((IVec3::NEG_Y, None)), vec![(axis, None)]),
            ("กิ่งตรงแนวเฉียง", Some((-diag_edge, None)), vec![(diag_edge, None)]),
            ("กิ่งหักมุมเฉียง", Some((IVec3::NEG_Y, None)), vec![(diag_edge, None)]),
            ("เฉียงมุมสามแกน", Some((-diag_corner, None)), vec![(diag_corner, None)]),
            (
                "จุดแตกกิ่งสามเส้น",
                Some((IVec3::NEG_Y, None)),
                vec![(axis, None), (diag_edge, None), (IVec3::new(-1, 1, 1), None)],
            ),
            ("รากไม่มี parent", None, vec![(axis, None)]),
        ];

        for thickness in [crate::tree::MIN_THICKNESS, 8, 13, crate::tree::TRUNK_THICKNESS] {
            for (name, parent, children) in &cases {
                let mut set = ChunkMeshSet::default();
                generate_branch_mesh_into(&mut set, 0.0, 0.0, 0.0, thickness, *parent, children, [1.0; 4]);
                assert_eq!(
                    open_edge_count(&set),
                    0,
                    "thickness {thickness} ทรง '{name}': mesh มีรู"
                );
            }
        }
    }

    /// **รอยต่อของ node สองตัวที่ติดกันต้องปิดสนิท** — ปลายฝั่ง joined จงใจไม่ปิดฝา
    /// เพราะอีกฝั่งต้องมาบรรจบพอดี ถ้าขอบสองฝั่งไม่ตรงกันเป๊ะจะเหลือขอบเปิด = รอยแยก
    /// ที่มองเห็นตรงข้อต่อ (อาการ "กิ่งเฉียงดูไม่ต่อกัน")
    #[test]
    fn adjacent_branch_nodes_seal_their_shared_joint() {
        for dir in crate::tree::NEIGHBOUR_DIRS {
            for (t_a, t_b) in [(16u8, 14u8), (13, 7), (9, 9), (4, 2), (2, 16)] {
                let mut set = ChunkMeshSet::default();
                // A: root (โคนปิดฝาเอง) มีลูกคือ B
                generate_branch_mesh_into(
                    &mut set, 0.0, 0.0, 0.0, t_a,
                    None, &[(dir, Some(t_b))], [1.0; 4],
                );
                // B: parent คือ A ไม่มีลูก (ปลายกิ่งปิดฝาเอง) — วางที่ออฟเซ็ต dir
                generate_branch_mesh_into(
                    &mut set, dir.x as f32, dir.y as f32, dir.z as f32, t_b,
                    Some((-dir, Some(t_a))), &[], [1.0; 4],
                );
                assert_eq!(
                    open_edge_count(&set),
                    0,
                    "ทิศ {dir:?} thickness {t_a}→{t_b}: รอยต่อไม่ปิดสนิท"
                );
            }
        }
    }

    /// หน้าตัดของตัวต่อสองฝั่งรอยต่อเดียวกันต้องทับกันสนิททุกทิศ (รวมเฉียง)
    /// ถ้าแกนหน้าตัดขึ้นกับทิศแทนที่จะขึ้นกับเส้นแกน สองฝั่งจะบิดคนละมุมแล้วรอยต่อแตก
    #[test]
    fn extension_cross_sections_match_across_a_joint() {
        let r = 0.3_f32;
        for dir in crate::tree::NEIGHBOUR_DIRS {
            let (u, n, w) = extension_basis(dir);
            // ตั้งฉากและเป็นหน่วยจริง
            assert!((u.length() - 1.0).abs() < 1e-4, "{dir:?}: u ไม่เป็นเวกเตอร์หน่วย");
            assert!((w.length() - 1.0).abs() < 1e-4, "{dir:?}: w ไม่เป็นเวกเตอร์หน่วย");
            assert!(u.dot(n).abs() < 1e-4, "{dir:?}: u ไม่ตั้งฉากกับทิศ");
            assert!(w.dot(n).abs() < 1e-4, "{dir:?}: w ไม่ตั้งฉากกับทิศ");
            assert!((u.cross(n) - w).length() < 1e-4, "{dir:?}: มือขวากลับด้าน (winding พัง)");

            let max_y = dir.as_vec3().length() * 0.5;
            let (u2, n2, w2) = extension_basis(-dir);

            // มุมหน้าตัดของฝั่งเรา (พิกัดโลกเทียบศูนย์กลางบล็อกตัวเอง)
            let ours: Vec<Vec3> = [(-r, -r), (r, -r), (r, r), (-r, r)]
                .iter()
                .map(|(a, b)| n * max_y + u * *a + w * *b)
                .collect();
            // มุมหน้าตัดของเพื่อนบ้าน แปลงมาอยู่บนระบบพิกัดเดียวกัน
            let base = dir.as_vec3();
            let theirs: Vec<Vec3> = [(-r, -r), (r, -r), (r, r), (-r, r)]
                .iter()
                .map(|(a, b)| base + n2 * max_y + u2 * *a + w2 * *b)
                .collect();

            for c in &ours {
                assert!(
                    theirs.iter().any(|t| (*t - *c).length() < 1e-4),
                    "{dir:?}: มุมหน้าตัด {c:?} ไม่มีคู่จากอีกฝั่ง — รอยต่อจะแตก"
                );
            }
        }
    }

    /// ลำต้นต้องหนากว่ากิ่งอย่างเห็นได้ชัด และเรียวลงตลอดความสูงแบบค่อยเป็นค่อยไป
    #[test]
    fn trunk_is_clearly_thicker_than_its_branches() {
        let mut state: u64 = 0x1234_5678_9ABC_DEF0;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut blocks = ChunkBlocks::new_uniform(BlockType::Air);
        let mut records = Vec::new();
        grow_tree(&mut blocks, &mut records, IVec3::new(8, 60, 8), &TREE_PRESETS[ACTIVE_TREE_PRESET].1, &mut next);

        // ลำต้น = สายที่สาวจาก root ขึ้นไปตรงๆ (record ชุดแรกก่อนแตกกิ่ง)
        let root_t = records[0].thickness;
        assert_eq!(root_t, crate::tree::TRUNK_THICKNESS);
        let trunk_bottom = records[1].thickness;
        assert!(
            trunk_bottom as f32 >= crate::tree::TRUNK_THICKNESS as f32 * 0.85,
            "โคนลำต้นเรียวเร็วเกินไป: {trunk_bottom}"
        );

        let thinnest = records.iter().map(|r| r.thickness).min().unwrap();
        assert!(
            (thinnest as f32) < root_t as f32 * 0.5,
            "ปลายกิ่งต้องเล็กกว่าลำต้นครึ่งหนึ่ง: {thinnest} vs {root_t}"
        );
    }

    /// quantize_dir ต้องคืนก้าวที่อยู่ในเพื่อนบ้าน 26 ทิศเสมอ และห้ามคืน (0,0,0)
    /// (ถ้าคืนศูนย์ กิ่งจะวนอยู่กับที่ และ mesh จะหารด้วยศูนย์ตอน normalize)
    #[test]
    fn quantize_dir_always_yields_a_real_step() {
        let mut cases = vec![
            Vec3::ZERO, Vec3::Y, Vec3::NEG_Y, Vec3::X, Vec3::Z,
            Vec3::new(1.0, 1.0, 1.0), Vec3::new(0.3, 0.31, 0.29), Vec3::new(-0.4, 0.45, 0.4),
        ];
        // ทิศกระจายรอบทรงกลมแบบ deterministic
        for i in 0..200 {
            let a = i as f32 * 0.31;
            cases.push(Vec3::new(a.cos(), (a * 0.7).sin(), a.sin()));
        }
        for d in cases {
            let q = quantize_dir(d);
            assert_ne!(q, IVec3::ZERO, "dir {d:?} ให้ก้าวศูนย์");
            assert!(
                crate::tree::NEIGHBOUR_DIRS.contains(&q),
                "dir {d:?} → {q:?} ไม่ใช่เพื่อนบ้าน 26 ทิศ"
            );
        }
    }

    /// ต้นไม้ที่ปั้นต้องอยู่ในกรอบ chunk ทั้งต้น และ topology ต้องเป็นต้นไม้จริง:
    /// root เดียว, ไม่มีตำแหน่งซ้ำ, parent มาก่อนลูกเสมอ, ทุกลิงก์เป็นเพื่อนบ้าน 26 ทิศ
    #[test]
    fn generated_tree_is_in_bounds_and_well_formed() {
        for seed in 0..64u64 {
            let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = move || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            };
            let mut blocks = ChunkBlocks::new_uniform(BlockType::Air);
            let mut records = Vec::new();
            grow_tree(&mut blocks, &mut records, IVec3::new(8, 60, 8), &TREE_PRESETS[ACTIVE_TREE_PRESET].1, &mut next);

            assert!(records.len() > 3, "seed {seed}: ต้นไม้เล็กเกินไป");
            let mut seen: std::collections::HashSet<IVec3> = Default::default();
            let mut roots = 0;
            for r in &records {
                let p = IVec3::from_array(r.pos);
                assert!(inside_chunk(p), "seed {seed}: {p} หลุดกรอบ chunk");
                assert_eq!(
                    blocks.get(p.x as usize, p.y as usize, p.z as usize),
                    BlockType::Branch,
                    "seed {seed}: {p} มี node แต่ไม่มีบล็อกกิ่ง"
                );
                assert!(seen.insert(p), "seed {seed}: {p} มี record ซ้ำ");
                match r.parent {
                    None => roots += 1,
                    Some(pp) => {
                        let pp = IVec3::from_array(pp);
                        assert!(seen.contains(&pp), "seed {seed}: parent มาหลังลูก");
                        assert_eq!((p - pp).abs().max_element(), 1, "seed {seed}: ลิงก์ข้ามช่อง");
                    }
                }
            }
            assert_eq!(roots, 1, "seed {seed}: ต้องมี root เดียว");
        }
    }

    /// record ต่อ chunk ต้อง round-trip ได้ครบ ทั้ง thickness และลิงก์ parent/children
    #[test]
    fn chunk_records_round_trip() {
        let mut world = world_with_one_chunk();
        world.set_block(2, 0, 2, BlockType::Dirt);
        for y in 1..=4 {
            let p = IVec3::new(2, y, 2);
            set_block_edit(&mut world, p, BlockType::Branch);
        }
        let records = world.branch_network.chunk_records(IVec2::ZERO, CHUNK_WIDTH as i32);
        assert_eq!(records.len(), 4);

        let mut restored = crate::tree::BranchNetwork::default();
        restored.merge_records(&records);
        assert_eq!(restored.nodes.len(), world.branch_network.nodes.len());
        for (pos, node) in &world.branch_network.nodes {
            let back = restored.nodes.get(pos).expect("node หาย");
            assert_eq!(back.parent_pos, node.parent_pos);
            assert_eq!(back.thickness, node.thickness);
            assert_eq!(back.children, node.children, "ลิงก์ลูกต้องประกอบกลับได้");
        }
        // เรียงแล้ว = ไฟล์เซฟ deterministic
        let again = world.branch_network.chunk_records(IVec2::ZERO, CHUNK_WIDTH as i32);
        assert_eq!(records, again);
    }

    /// evict ต้องเอา node ออกให้หมดและไม่ทิ้งลิงก์ค้างใน parent ที่ยังโหลดอยู่
    #[test]
    fn evict_chunk_clears_nodes_and_parent_links() {
        let mut bn = crate::tree::BranchNetwork::default();
        let here = IVec3::new(1, 5, 1);          // chunk (0,0)
        let neighbour = IVec3::new(-1, 5, 1);    // chunk (-1,0)
        bn.add_root(neighbour, crate::tree::TRUNK_THICKNESS);
        bn.merge_records(&[crate::tree::BranchRecord {
            pos: here.to_array(),
            parent: Some(neighbour.to_array()),
            thickness: 10,
        }]);
        assert!(bn.nodes[&neighbour].children.contains(&here));

        bn.evict_chunk(IVec2::ZERO, CHUNK_WIDTH as i32);
        assert!(!bn.nodes.contains_key(&here));
        assert!(bn.nodes.contains_key(&neighbour), "chunk อื่นต้องไม่โดนด้วย");
        assert!(
            !bn.nodes[&neighbour].children.contains(&here),
            "ลิงก์ค้างจะทำให้ mesh วาดกิ่งยื่นไปหาที่ที่ไม่มีอะไร"
        );
    }

    /// สร้าง App เปล่าที่มี resource ครบสำหรับรัน block_update_system
    fn app_with(world: VoxelWorld) -> App {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_message::<crate::item::SpawnDroppedItemEvent>()
            .insert_resource(world)
            .init_resource::<PendingBlockUpdates>()
            .init_resource::<crate::network::PendingNetEdits>()
            .init_resource::<ActivePools>()
            .init_resource::<ActiveFluids>()
            .add_systems(Update, block_update_system);
        app
    }

    /// โลก 3×3 chunk พื้นตันเรียบที่ y=0..=ground — เท่ากับที่ mesher ต้องการพอดี
    fn world_grid_with_ground(ground: usize) -> VoxelWorld {
        let mut world = VoxelWorld::default();
        for cz in -1..=1 {
            for cx in -1..=1 {
                let mut blocks = ChunkBlocks::new_uniform(BlockType::Air);
                for z in 0..CHUNK_WIDTH {
                    for x in 0..CHUNK_WIDTH {
                        for y in 0..=ground {
                            blocks.set(x, y, z, BlockType::Stone);
                        }
                    }
                }
                blocks.compact();
                world.chunks.insert(IVec2::new(cx, cz), ChunkData {
                    blocks: Arc::new(blocks),
                    chiseled_blocks: HashMap::new(),
                    facings: HashMap::new(),
                    chest_slots: HashMap::new(),
                    furnace_slots: HashMap::new(),
                    num_vertices: 0,
                    num_indices: 0,
                    water_y_min: 1,
                    water_y_max: 0,
                    num_water_vertices: 0,
                    num_water_indices: 0,
                    dirty: false,
                    light: Default::default(),
                    light_dirty: true,
                    light_missing_neighbors: 0,
                });
            }
        }
        world
    }

    /// เส้นทางจริงของแสง: ensure_chunk_light → light_neighborhood → อ่านค่าข้ามขอบ
    /// (การ map index เพื่อนบ้าน 8 ทิศเป็นจุดพลาดง่าย ถ้าสลับกันแสงตรงขอบจะเพี้ยน)
    #[test]
    fn chunk_light_pipeline_lights_the_surface_and_leaves_underground_dark() {
        let ground = 80usize;
        let mut world = world_grid_with_ground(ground);

        // relight_system ไล่ทำทุก chunk ที่ dirty — จำลองด้วยการทำครบทั้ง 3×3
        // **ทุกตัวต้องคำนวณได้ แม้ตัวริมที่เพื่อนบ้านไม่ครบ 8** ไม่งั้นจะไม่มี chunk ไหน
        // ผ่านเงื่อนไข mesh เลย (บั๊กจอฟ้า: เห็น mesh แค่ chunk เดียว)
        for cz in -1..=1 {
            for cx in -1..=1 {
                let p = IVec2::new(cx, cz);
                assert!(ensure_chunk_light(&mut world, p), "{p} ต้องคำนวณแสงได้");
                assert!(!world.chunks[&p].light_dirty, "{p} ต้องเคลียร์ flag หลังคำนวณ");
            }
        }

        // นี่คือเงื่อนไขที่ mesher ใช้ตัดสินว่าจะวาด chunk ได้ไหม — ถ้าเป็น None แปลว่า
        // chunk จะไม่ถูก mesh เลย (อาการจอฟ้า)
        let lm = light_neighborhood(&world, IVec2::ZERO)
            .expect("เพื่อนบ้านโหลดครบและคำนวณแสงแล้ว ต้อง mesh ได้");
        assert_eq!(lm.get(8, ground as i32 + 1, 8), crate::light::MAX_LIGHT, "เหนือพื้นต้องสว่างเต็ม");
        assert_eq!(lm.get(8, ground as i32 - 3, 8), 0, "ใต้ดินต้องมืด");

        // อ่านทะลุขอบไปหาเพื่อนบ้านทั้ง 8 ทิศต้องได้ค่าเดียวกัน (พื้นเรียบเหมือนกันหมด)
        // ถ้า index เพื่อนบ้านสลับกันจะอ่านไปโดน chunk ที่ยังไม่ได้คำนวณแล้วได้ 0
        for (dx, dz) in [(-1, 0), (16, 0), (0, -1), (0, 16), (-1, -1), (16, 16), (-1, 16), (16, -1)] {
            assert_eq!(
                lm.get(dx, ground as i32 + 1, dz),
                crate::light::MAX_LIGHT,
                "อ่านข้ามขอบไปทาง ({dx},{dz}) แล้วได้ค่าผิด"
            );
        }
    }

    /// เส้นโค้งความสว่างต้องไล่ขึ้นตามระดับแสง และไม่ดำสนิทที่ระดับ 0
    #[test]
    fn sky_curve_is_monotonic_with_a_floor() {
        for l in 1..=crate::light::MAX_LIGHT {
            assert!(sky_curve(l) > sky_curve(l - 1), "ระดับ {l} ต้องสว่างกว่าระดับก่อนหน้า");
        }
        assert!(sky_curve(0) > 0.0, "ระดับ 0 ต้องไม่ดำสนิท ไม่งั้นในถ้ำมองไม่เห็นรูปทรงเลย");
        assert_eq!(sky_curve(crate::light::MAX_LIGHT), 1.0);
    }

    /// ตัดต้นไม้แล้วใบต้องร่วงตาม ไม่ลอยค้างกลางอากาศ
    #[test]
    fn leaves_fall_after_the_branch_holding_them_is_gone() {
        let mut world = world_with_one_chunk();
        world.set_block(8, 0, 8, BlockType::Dirt);

        // ต้นเล็กๆ: กิ่งตั้ง 3 บล็อก + ใบครอบยอด
        let stack: Vec<IVec3> = (1..=3).map(|y| IVec3::new(8, y, 8)).collect();
        for p in &stack {
            set_block_edit(&mut world, *p, BlockType::Branch);
        }
        let mut leaves = Vec::new();
        for dy in 0..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let l = IVec3::new(8 + dx, 3 + dy, 8 + dz);
                    if world.get_block(l.x, l.y, l.z) == BlockType::Air {
                        world.set_block(l.x, l.y, l.z, BlockType::Leaves);
                        leaves.push(l);
                    }
                }
            }
        }
        assert!(!leaves.is_empty());

        // ทุบโคน → กิ่งบนร่วงตาม แล้วใบต้องร่วงตามอีกทอด
        set_block_edit(&mut world, stack[0], BlockType::Air);

        let mut app = app_with(world);
        for _ in 0..24 {
            app.update();
        }

        let world = app.world().resource::<VoxelWorld>();
        for l in &leaves {
            assert_eq!(
                world.get_block(l.x, l.y, l.z),
                BlockType::Air,
                "{l} ยังลอยค้างอยู่"
            );
        }
    }

    /// ใบที่ผู้เล่นเอาไปสร้างบ้านไกลจากต้นไม้ ห้ามร่วงเองเพราะมีการแก้บล็อกข้างๆ
    /// (คิว decay เติมเฉพาะตอนกิ่งถูกทำลายจริง ไม่ใช่ทุกครั้งที่บล็อกรอบๆ ขยับ)
    #[test]
    fn player_placed_leaves_far_from_trees_never_decay() {
        let mut world = world_with_one_chunk();
        let wall: Vec<IVec3> = (0..4).map(|i| IVec3::new(2 + i, 5, 2)).collect();
        for p in &wall {
            world.set_block(p.x, p.y, p.z, BlockType::Leaves);
        }
        // ขยับบล็อกติดกำแพงใบ — ไม่เกี่ยวกับกิ่งเลย
        set_block_edit(&mut world, IVec3::new(2, 4, 2), BlockType::Stone);
        set_block_edit(&mut world, IVec3::new(2, 4, 2), BlockType::Air);

        let mut app = app_with(world);
        for _ in 0..12 {
            app.update();
        }

        let world = app.world().resource::<VoxelWorld>();
        for p in &wall {
            assert_eq!(
                world.get_block(p.x, p.y, p.z),
                BlockType::Leaves,
                "{p} หายไปทั้งที่ไม่ได้เกี่ยวกับต้นไม้"
            );
        }
    }

    /// ใบที่ยังมีกิ่งอื่นค้ำอยู่ในระยะต้องอยู่ต่อ — ตัดกิ่งเดียวไม่ควรทำใบหายทั้งพุ่ม
    #[test]
    fn leaves_still_near_a_branch_survive() {
        let mut world = world_with_one_chunk();
        world.set_block(8, 0, 8, BlockType::Dirt);
        for y in 1..=4 {
            set_block_edit(&mut world, IVec3::new(8, y, 8), BlockType::Branch);
        }
        // กิ่งข้างยื่นออกไป แล้วมีใบเกาะที่ปลาย
        set_block_edit(&mut world, IVec3::new(9, 4, 8), BlockType::Branch);
        let leaf = IVec3::new(10, 4, 8);
        world.set_block(leaf.x, leaf.y, leaf.z, BlockType::Leaves);

        // ทุบเฉพาะกิ่งข้าง — ลำต้นยังอยู่และอยู่ในระยะเกาะของใบ
        set_block_edit(&mut world, IVec3::new(9, 4, 8), BlockType::Air);

        let mut app = app_with(world);
        for _ in 0..12 {
            app.update();
        }

        let world = app.world().resource::<VoxelWorld>();
        assert_eq!(
            world.get_block(leaf.x, leaf.y, leaf.z),
            BlockType::Leaves,
            "ใบยังอยู่ในระยะลำต้น ไม่ควรร่วง"
        );
    }

    /// วางกิ่งติดเพื่อนบ้านหลายตัว ต้องเลือกตัวที่หนาที่สุดเป็น parent
    /// ไม่ใช่ตัวแรกที่เจอตามลำดับทิศ (NEG_Y, Y, X, ...) และกิ่งลอยเดี่ยวต้องผอม
    #[test]
    fn branch_parent_pick_prefers_thickest_neighbour() {
        let mut world = world_with_one_chunk();

        // เพื่อนบ้านฝั่ง Y: ลอยกลางอากาศ = root ผอม (มาก่อนในลำดับทิศ)
        let thin = IVec3::new(5, 6, 5);
        set_block_edit(&mut world, thin, BlockType::Branch);
        assert_eq!(world.branch_network.thickness_at(thin), Some(crate::tree::LOOSE_THICKNESS));

        // เพื่อนบ้านฝั่ง X: งอกจากดิน = ลำต้นหนา (มาทีหลังในลำดับทิศ)
        world.set_block(6, 4, 5, BlockType::Dirt);
        let trunk = IVec3::new(6, 5, 5);
        set_block_edit(&mut world, trunk, BlockType::Branch);
        assert_eq!(world.branch_network.thickness_at(trunk), Some(crate::tree::TRUNK_THICKNESS));

        let joint = IVec3::new(5, 5, 5);
        set_block_edit(&mut world, joint, BlockType::Branch);
        assert_eq!(world.branch_network.nodes[&joint].parent_pos, Some(trunk));

        // ลอยเดี่ยวไม่ติดอะไรเลย — ต้องผอม ไม่ใช่อ้วนเท่าลำต้นเหมือนเดิม
        let loose = IVec3::new(12, 8, 12);
        set_block_edit(&mut world, loose, BlockType::Branch);
        assert_eq!(world.branch_network.thickness_at(loose), Some(crate::tree::LOOSE_THICKNESS));
    }

    /// คณิตบัญชีสระ: ความจุสะสม + solve ระดับผิวจากปริมาตร ต้อง invertible
    #[test]
    fn pool_surface_solve() {
        // สองคอลัมน์เท่ากัน y 0..=1 (จุคอลัมน์ละ 16 หน่วย)
        let mut cols: HashMap<(i32, i32), PoolColumn> = HashMap::new();
        cols.insert((0, 0), PoolColumn { y_bottom: 0, y_top: 1 });
        cols.insert((1, 0), PoolColumn { y_bottom: 0, y_top: 1 });
        let segs = build_cap_segments(&cols);
        assert_eq!(eval_cap(&segs, 0), 0);
        assert_eq!(eval_cap(&segs, 8), 16);
        assert_eq!(eval_cap(&segs, 16), 32);
        assert_eq!(surface_for_volume(&segs, 16), 8);
        assert_eq!(surface_for_volume(&segs, 20), 10);
        assert_eq!(surface_for_volume(&segs, 32), 16);

        // ก้นสระไม่เท่ากัน: A ลึก (y 0..=3), B ตื้น (y 2..=3)
        let mut cols2: HashMap<(i32, i32), PoolColumn> = HashMap::new();
        cols2.insert((0, 0), PoolColumn { y_bottom: 0, y_top: 3 });
        cols2.insert((1, 0), PoolColumn { y_bottom: 2, y_top: 3 });
        let segs2 = build_cap_segments(&cols2);
        // น้ำ 16 หน่วยพอดีเต็ม A ถึงระดับก้น B
        assert_eq!(surface_for_volume(&segs2, 16), 16);
        // เกินจากนั้นเกลี่ยสองคอลัมน์
        assert_eq!(surface_for_volume(&segs2, 18), 17);
        assert_eq!(eval_cap(&segs2, 17), 18);
        // เต็มสระ
        assert_eq!(surface_for_volume(&segs2, 48), 32);
        // roundtrip ทุกปริมาตร: cap(solve(v)) <= v เสมอ (เศษ < จำนวนคอลัมน์ active)
        for v in 0..=48u64 {
            let s = surface_for_volume(&segs2, v);
            assert!(eval_cap(&segs2, s) <= v, "cap(solve({v})) เกิน");
        }
    }

    /// anchor ความถูกต้องของเส้นทาง remesh เฉพาะน้ำ: buffer น้ำจาก
    /// create_water_mesh ต้องเป๊ะทุก byte กับ set.water ของ mesher เต็ม
    /// ครอบเคส: หลาย vol, น้ำ-กระจก, น้ำต่าง vol ติดกัน, น้ำจม, ขอบ chunk,
    /// เพื่อนบ้านตรง + ทแยง (drop smoothing ข้ามมุม)
    #[test]
    fn water_mesh_parity_with_full_mesher() {
        let mut main = ChunkBlocks::new_uniform(BlockType::Air);
        // พื้นหิน
        for z in 0..CHUNK_WIDTH {
            for x in 0..CHUNK_WIDTH {
                set(&mut main, x, 9, z, BlockType::Stone);
            }
        }
        // บ่อระดับผสม เต็มถึงขอบ chunk ทุกด้าน
        for z in 0..CHUNK_WIDTH {
            for x in 0..CHUNK_WIDTH {
                let b = match (x + z) % 5 {
                    0 => BlockType::Water8,
                    1 => BlockType::Water4,
                    2 => BlockType::Water1,
                    3 => BlockType::Air,
                    _ => BlockType::Water7,
                };
                set(&mut main, x, 10, z, b);
            }
        }
        // น้ำชั้นบน (มีน้ำจมข้างใต้)
        for x in 3..8 {
            set(&mut main, x, 11, 5, BlockType::Water8);
        }
        // กระจกแทรกในบ่อ (น้ำ-กระจกต้องวาดหน้า)
        set(&mut main, 6, 10, 6, BlockType::Glass);
        // เสาน้ำลอยโดด (เห็นครบทุกหน้า)
        set(&mut main, 2, 20, 2, BlockType::Water5);

        // เพื่อนบ้าน +X: น้ำ vol ต่างชิดขอบ (หน้าระหว่าง vol ต่างต้องวาด)
        let mut nx = ChunkBlocks::new_uniform(BlockType::Air);
        for z in 0..CHUNK_WIDTH {
            set(&mut nx, 0, 10, z, BlockType::Water6);
        }
        // เพื่อนบ้านทแยง +X+Z: น้ำที่มุม (ทดสอบ drop_cache ข้ามทแยง)
        let mut nxz = ChunkBlocks::new_uniform(BlockType::Air);
        set(&mut nxz, 0, 10, 0, BlockType::Water3);

        let air: Arc<ChunkBlocks> = Arc::new(ChunkBlocks::new_uniform(BlockType::Air));
        let neighbors: [Arc<ChunkBlocks>; 8] = [
            Arc::new(nx),
            air.clone(),
            air.clone(),
            air.clone(),
            Arc::new(nxz),
            air.clone(),
            air.clone(),
            air.clone(),
        ];

        let chunk_pos = IVec2::new(3, -2);
        let full = create_mesh_from_blocks(chunk_pos, &main, &neighbors, None, None, None, None);
        let (water, observed) = create_water_mesh(chunk_pos, &main, &neighbors, 0, CHUNK_HEIGHT - 1);

        assert!(!full.water.positions.is_empty(), "ฉากทดสอบต้องมีหน้าน้ำจริง");
        assert_eq!(water.positions, full.water.positions);
        assert_eq!(water.normals, full.water.normals);
        assert_eq!(water.colors, full.water.colors);
        assert_eq!(water.uvs, full.water.uvs);
        assert_eq!(water.indices, full.water.indices);
        assert_eq!(observed, Some((10, 20)));

        // แถบ y แคบ (superset ของน้ำจริง) ต้องให้ผลเหมือนสแกนทั้ง chunk
        let (banded, _) = create_water_mesh(chunk_pos, &main, &neighbors, 8, 24);
        assert_eq!(banded.positions, full.water.positions);
        assert_eq!(banded.indices, full.water.indices);
    }
}
