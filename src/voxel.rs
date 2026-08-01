use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bevy::{
    asset::RenderAssetUsages,
    camera::primitives::Aabb,
    light::NotShadowCaster,
    prelude::*,
    render::{mesh::Indices, render_resource::PrimitiveTopology},
    tasks::AsyncComputeTaskPool,
};
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

mod block_edit;
mod reactive;
pub use reactive::{
    fluid_hazard_system, reactive_fluid_system, take_one_unit, volcano_lifecycle_system,
};

#[derive(Component)]
pub struct Block;

/// Client-local debug rendering mode. Meshing tasks may read this from worker threads.
pub static DEBUG_XRAY_ENABLED: AtomicBool = AtomicBool::new(false);

#[inline]
fn xray_hidden_block(block: BlockType) -> bool {
    matches!(block, BlockType::Dirt | BlockType::Grass | BlockType::Stone)
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BlockType {
    Air = 0,
    Dirt = 1,
    Grass = 2,
    Stone = 3,
    Water = 4,
    OakWood = 5,
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
    /// พื้นไบโอมหิมะ (บนหิมะ ข้างหญ้าคลุมหิมะ ล่างดิน)
    SnowyGrass = 36,
    /// บล็อกหิมะเต็ม — คลุมยอดเขา/หินโผล่
    Snow = 37,
    /// ลำต้นสน (คิวบ์ เหมือน Wood)
    SpruceLog = 38,
    /// ใบสน — วาดเป็น sprite ดาว 3 แกนเหมือน Leaves
    SpruceLeaves = 39,
    CopperOre = 40,
    IronOre = 41,
    SpruceLogDamaged1 = 42,
    SpruceLogDamaged2 = 43,
    Crucible = 44,
    IngotMold = 45,
    CastIngot = 46,
    PickaxeMold = 47,
    MapleLog = 48,
    MapleLeaves = 49,
    MapleBranch = 50,
    CoalOre = 51,
    TinOre = 52,
    ZincOre = 53,
    Limestone = 54,
    Basalt = 55,
    VolcanicAsh = 56,
    MagmaRock = 57,
    Obsidian = 58,
    AlteredRock = 59,
    SulfurOre = 60,
    Gypsum = 61,
    LavaSource = 62,
    Lava1 = 63,
    Lava2 = 64,
    Lava3 = 65,
    Lava4 = 66,
    Lava5 = 67,
    Lava6 = 68,
    Lava7 = 69,
    Lava8 = 70,
    SulfuricAcidSource = 71,
    Acid1 = 72,
    Acid2 = 73,
    Acid3 = 74,
    Acid4 = 75,
    Acid5 = 76,
    Acid6 = 77,
    Acid7 = 78,
    Acid8 = 79,
    /// น้ำแข็งทะเล — คลุมผิว Frozen Ocean (เขตหนาว), solid เดินได้
    Ice = 80,
    /// กรวด — ก้นทะเลกลางลึก + หาดเขตเย็น
    Gravel = 81,
    /// ดินเหนียว — ก้นทะเลลึก
    Clay = 82,
}

impl BlockType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => BlockType::Dirt,
            2 => BlockType::Grass,
            3 => BlockType::Stone,
            4 => BlockType::Water,
            5 => BlockType::OakWood,
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
            36 => BlockType::SnowyGrass,
            37 => BlockType::Snow,
            38 => BlockType::SpruceLog,
            39 => BlockType::SpruceLeaves,
            40 => BlockType::CopperOre,
            41 => BlockType::IronOre,
            42 => BlockType::SpruceLogDamaged1,
            43 => BlockType::SpruceLogDamaged2,
            44 => BlockType::Crucible,
            45 => BlockType::IngotMold,
            46 => BlockType::CastIngot,
            47 => BlockType::PickaxeMold,
            48 => BlockType::MapleLog,
            49 => BlockType::MapleLeaves,
            50 => BlockType::MapleBranch,
            51 => BlockType::CoalOre,
            52 => BlockType::TinOre,
            53 => BlockType::ZincOre,
            54 => BlockType::Limestone,
            55 => BlockType::Basalt,
            56 => BlockType::VolcanicAsh,
            57 => BlockType::MagmaRock,
            58 => BlockType::Obsidian,
            59 => BlockType::AlteredRock,
            60 => BlockType::SulfurOre,
            61 => BlockType::Gypsum,
            62 => BlockType::LavaSource,
            63 => BlockType::Lava1,
            64 => BlockType::Lava2,
            65 => BlockType::Lava3,
            66 => BlockType::Lava4,
            67 => BlockType::Lava5,
            68 => BlockType::Lava6,
            69 => BlockType::Lava7,
            70 => BlockType::Lava8,
            71 => BlockType::SulfuricAcidSource,
            72 => BlockType::Acid1,
            73 => BlockType::Acid2,
            74 => BlockType::Acid3,
            75 => BlockType::Acid4,
            76 => BlockType::Acid5,
            77 => BlockType::Acid6,
            78 => BlockType::Acid7,
            79 => BlockType::Acid8,
            80 => BlockType::Ice,
            81 => BlockType::Gravel,
            82 => BlockType::Clay,
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

    pub fn is_lava(self) -> bool {
        matches!(
            self,
            BlockType::LavaSource
                | BlockType::Lava1 | BlockType::Lava2 | BlockType::Lava3 | BlockType::Lava4
                | BlockType::Lava5 | BlockType::Lava6 | BlockType::Lava7 | BlockType::Lava8
        )
    }

    pub fn is_acid(self) -> bool {
        matches!(
            self,
            BlockType::SulfuricAcidSource
                | BlockType::Acid1 | BlockType::Acid2 | BlockType::Acid3 | BlockType::Acid4
                | BlockType::Acid5 | BlockType::Acid6 | BlockType::Acid7 | BlockType::Acid8
        )
    }

    pub fn is_fluid(self) -> bool {
        self.is_water() || self.is_lava() || self.is_acid()
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

pub const BLOCK_DEFS: [BlockDef; 83] = [
    BlockDef { name: "Air", color: [1.0, 1.0, 1.0, 1.0], solid: false, transparent: true, emission: None, hardness: 0.0,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Dirt", color: [0.4, 0.2, 0.0, 1.0], solid: true, transparent: false, emission: None, hardness: 1.0,
        tex_top: &["textures/dirt.png"], tex_side: &["textures/dirt.png"], tex_bottom: &["textures/dirt.png"],
        overlay_side: &[] },
    BlockDef { name: "Grass", color: [0.2, 0.6, 0.2, 1.0], solid: true, transparent: false, emission: None, hardness: 1.2,
        tex_top: &["textures/grass_top.png"],
        // หน้าข้างเป็นดินล้วน — สีเขียวมาจาก grass_side_overlay (กระโปรงหญ้าเต็มด้าน ย้อมตาม biome)
        tex_side: &["textures/dirt.png"],
        tex_bottom: &["textures/dirt.png"],
        overlay_side: &["textures/grass_side_overlay.png"] },
    BlockDef { name: "Stone", color: [0.5, 0.5, 0.5, 1.0], solid: true, transparent: false, emission: None, hardness: 6.0,
        tex_top: &["textures/stone.png"], tex_side: &["textures/stone.png"], tex_bottom: &["textures/stone.png"],
        overlay_side: &[] },
    BlockDef { name: "Water", color: [0.25, 0.5, 0.85, 1.0], solid: false, transparent: true, emission: None, hardness: 3.2,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Oak Wood", color: [0.4, 0.3, 0.2, 1.0], solid: true, transparent: false, emission: None, hardness: 3.0,
        tex_top: &["textures/oak_log_top.png"], tex_side: &["textures/oak_log_side.png"],
        tex_bottom: &["textures/oak_log_top.png"], overlay_side: &[] },
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
        tex_top: &["textures/oak_log_side.png"], tex_side: &["textures/oak_log_side.png"], tex_bottom: &["textures/oak_log_side.png"], overlay_side: &[] },
    // ไบโอมหิมะ — พื้นหญ้าคลุมหิมะ (บน=หิมะ ข้าง=หญ้าคลุมหิมะ ล่าง=ดิน) ไม่มีพู่หญ้า
    BlockDef { name: "Snowy Grass", color: [0.85, 0.9, 0.95, 1.0], solid: true, transparent: false, emission: None, hardness: 1.2,
        tex_top: &["textures/snow.png"], tex_side: &["textures/grass_side_snow.png"], tex_bottom: &["textures/dirt.png"],
        overlay_side: &[] },
    BlockDef { name: "Snow", color: [0.9, 0.95, 1.0, 1.0], solid: true, transparent: false, emission: None, hardness: 0.6,
        tex_top: &["textures/snow.png"], tex_side: &["textures/snow.png"], tex_bottom: &["textures/snow.png"], overlay_side: &[] },
    BlockDef { name: "Spruce Log", color: [0.35, 0.25, 0.18, 1.0], solid: true, transparent: false, emission: None, hardness: 3.0,
        tex_top: &["textures/spruce_log_top.png"], tex_side: &["textures/spruce_log_side.png"],
        tex_bottom: &["textures/spruce_log_top.png"], overlay_side: &[] },
    // ใบสน: วาดเป็น sprite ดาว 3 แกน (ดู generate_leaf_mesh_into) เหมือน Leaves — transparent:true
    BlockDef { name: "Spruce Leaves", color: [0.1, 0.35, 0.2, 1.0], solid: true, transparent: true, emission: None, hardness: 0.3,
        tex_top: &["textures/spruce_leaves.png"], tex_side: &["textures/spruce_leaves.png"],
        tex_bottom: &["textures/spruce_leaves.png"], overlay_side: &[] },
    BlockDef { name: "Copper Ore", color: [0.6, 0.4, 0.3, 1.0], solid: true, transparent: false, emission: None, hardness: 6.0,
        tex_top: &["textures/copper_ore.png"], tex_side: &["textures/copper_ore.png"], tex_bottom: &["textures/copper_ore.png"],
        overlay_side: &[] },
    BlockDef { name: "Iron Ore", color: [0.7, 0.6, 0.5, 1.0], solid: true, transparent: false, emission: None, hardness: 6.0,
        tex_top: &["textures/iron_ore.png"], tex_side: &["textures/iron_ore.png"], tex_bottom: &["textures/iron_ore.png"],
        overlay_side: &[] },
    BlockDef { name: "Spruce Log Damaged 1", color: [0.35, 0.22, 0.12, 1.0], solid: true, transparent: false, emission: None, hardness: 3.0,
        tex_top: &["textures/spruce_log_top.png"], tex_side: &["textures/spruce_log_damaged1.png"],
        tex_bottom: &["textures/spruce_log_top.png"], overlay_side: &[] },
    BlockDef { name: "Spruce Log Damaged 2", color: [0.35, 0.22, 0.12, 1.0], solid: true, transparent: false, emission: None, hardness: 3.0,
        tex_top: &["textures/spruce_log_top.png"], tex_side: &["textures/spruce_log_damaged2.png"],
        tex_bottom: &["textures/spruce_log_top.png"], overlay_side: &[] },
    BlockDef { name: "Crucible", color: [0.6, 0.3, 0.2, 1.0], solid: true, transparent: true, emission: None, hardness: 0.5,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Ingot Mold", color: [0.45, 0.28, 0.18, 1.0], solid: true, transparent: true, emission: None, hardness: 0.8,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Cast Ingot", color: [0.72, 0.42, 0.20, 1.0], solid: true, transparent: true, emission: None, hardness: 0.8,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Pickaxe Mold", color: [0.45, 0.28, 0.18, 1.0], solid: true, transparent: true, emission: None, hardness: 0.8,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Maple Log", color: [0.38, 0.27, 0.16, 1.0], solid: true, transparent: false, emission: None, hardness: 3.0,
        tex_top: &["textures/maple_log_top.png"], tex_side: &["textures/maple_log_side.png"],
        tex_bottom: &["textures/maple_log_top.png"], overlay_side: &[] },
    BlockDef { name: "Maple Leaves", color: [0.20, 0.52, 0.16, 1.0], solid: true, transparent: true, emission: None, hardness: 0.3,
        tex_top: &["textures/maple_leaves.png"], tex_side: &["textures/maple_leaves.png"],
        tex_bottom: &["textures/maple_leaves.png"], overlay_side: &[] },
    BlockDef { name: "Maple Branch", color: [0.38, 0.27, 0.16, 1.0], solid: true, transparent: true, emission: None, hardness: 2.0,
        tex_top: &["textures/maple_log_side.png"], tex_side: &["textures/maple_log_side.png"],
        tex_bottom: &["textures/maple_log_side.png"], overlay_side: &[] },
    BlockDef { name: "Coal Ore", color: [0.20, 0.20, 0.20, 1.0], solid: true, transparent: false, emission: None, hardness: 5.5,
        tex_top: &["textures/coal_ore.png"], tex_side: &["textures/coal_ore.png"],
        tex_bottom: &["textures/coal_ore.png"], overlay_side: &[] },
    BlockDef { name: "Tin Ore", color: [0.62, 0.66, 0.70, 1.0], solid: true, transparent: false, emission: None, hardness: 6.0,
        tex_top: &["textures/tin_ore.png"], tex_side: &["textures/tin_ore.png"],
        tex_bottom: &["textures/tin_ore.png"], overlay_side: &[] },
    BlockDef { name: "Zinc Ore", color: [0.52, 0.58, 0.55, 1.0], solid: true, transparent: false, emission: None, hardness: 6.0,
        tex_top: &["textures/zinc_ore.png"], tex_side: &["textures/zinc_ore.png"],
        tex_bottom: &["textures/zinc_ore.png"], overlay_side: &[] },
    BlockDef { name: "Limestone", color: [0.76, 0.75, 0.67, 1.0], solid: true, transparent: false, emission: None, hardness: 4.5,
        tex_top: &["textures/limestone.png"], tex_side: &["textures/limestone.png"],
        tex_bottom: &["textures/limestone.png"], overlay_side: &[] },
    BlockDef { name: "Basalt", color: [0.16, 0.17, 0.18, 1.0], solid: true, transparent: false, emission: None, hardness: 8.0,
        tex_top: &["textures/basalt.png"], tex_side: &["textures/basalt.png"], tex_bottom: &["textures/basalt.png"], overlay_side: &[] },
    BlockDef { name: "Volcanic Ash", color: [0.27, 0.25, 0.24, 1.0], solid: true, transparent: false, emission: None, hardness: 0.5,
        tex_top: &["textures/volcanic_ash.png"], tex_side: &["textures/volcanic_ash.png"], tex_bottom: &["textures/volcanic_ash.png"], overlay_side: &[] },
    BlockDef { name: "Magma Rock", color: [0.85, 0.22, 0.04, 1.0], solid: true, transparent: false, emission: Some([1.6, 0.35, 0.04]), hardness: 7.0,
        tex_top: &["textures/magma_rock.png"], tex_side: &["textures/magma_rock.png"], tex_bottom: &["textures/magma_rock.png"], overlay_side: &[] },
    BlockDef { name: "Obsidian", color: [0.09, 0.05, 0.13, 1.0], solid: true, transparent: false, emission: None, hardness: 18.0,
        tex_top: &["textures/obsidian.png"], tex_side: &["textures/obsidian.png"], tex_bottom: &["textures/obsidian.png"], overlay_side: &[] },
    BlockDef { name: "Altered Rock", color: [0.72, 0.69, 0.48, 1.0], solid: true, transparent: false, emission: None, hardness: 4.0,
        tex_top: &["textures/altered_rock.png"], tex_side: &["textures/altered_rock.png"], tex_bottom: &["textures/altered_rock.png"], overlay_side: &[] },
    BlockDef { name: "Sulfur Ore", color: [0.85, 0.78, 0.12, 1.0], solid: true, transparent: false, emission: None, hardness: 4.5,
        tex_top: &["textures/sulfur_ore.png"], tex_side: &["textures/sulfur_ore.png"], tex_bottom: &["textures/sulfur_ore.png"], overlay_side: &[] },
    BlockDef { name: "Gypsum", color: [0.88, 0.86, 0.78, 1.0], solid: true, transparent: false, emission: None, hardness: 2.5,
        tex_top: &["textures/gypsum.png"], tex_side: &["textures/gypsum.png"], tex_bottom: &["textures/gypsum.png"], overlay_side: &[] },
    BlockDef { name: "Lava Source", color: [1.0, 0.22, 0.02, 0.92], solid: false, transparent: true, emission: Some([2.0, 0.35, 0.03]), hardness: 3.2,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Lava1", color: [0.92, 0.10, 0.01, 0.92], solid: false, transparent: true, emission: Some([1.5, 0.25, 0.02]), hardness: 0.4,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Lava2", color: [0.94, 0.12, 0.01, 0.92], solid: false, transparent: true, emission: Some([1.5, 0.25, 0.02]), hardness: 0.8,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Lava3", color: [0.96, 0.14, 0.01, 0.92], solid: false, transparent: true, emission: Some([1.6, 0.28, 0.02]), hardness: 1.2,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Lava4", color: [0.98, 0.16, 0.01, 0.92], solid: false, transparent: true, emission: Some([1.7, 0.30, 0.02]), hardness: 1.6,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Lava5", color: [1.0, 0.18, 0.01, 0.92], solid: false, transparent: true, emission: Some([1.8, 0.32, 0.02]), hardness: 2.0,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Lava6", color: [1.0, 0.20, 0.01, 0.92], solid: false, transparent: true, emission: Some([1.9, 0.34, 0.02]), hardness: 2.4,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Lava7", color: [1.0, 0.22, 0.02, 0.92], solid: false, transparent: true, emission: Some([2.0, 0.36, 0.02]), hardness: 2.8,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Lava8", color: [1.0, 0.24, 0.02, 0.92], solid: false, transparent: true, emission: Some([2.0, 0.38, 0.03]), hardness: 3.2,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Sulfuric Acid Source", color: [0.58, 0.92, 0.12, 0.72], solid: false, transparent: true, emission: None, hardness: 3.2,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Acid1", color: [0.48, 0.84, 0.08, 0.72], solid: false, transparent: true, emission: None, hardness: 0.4,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Acid2", color: [0.50, 0.85, 0.08, 0.72], solid: false, transparent: true, emission: None, hardness: 0.8,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Acid3", color: [0.52, 0.86, 0.09, 0.72], solid: false, transparent: true, emission: None, hardness: 1.2,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Acid4", color: [0.54, 0.87, 0.09, 0.72], solid: false, transparent: true, emission: None, hardness: 1.6,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Acid5", color: [0.55, 0.88, 0.10, 0.72], solid: false, transparent: true, emission: None, hardness: 2.0,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Acid6", color: [0.56, 0.89, 0.10, 0.72], solid: false, transparent: true, emission: None, hardness: 2.4,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Acid7", color: [0.57, 0.90, 0.11, 0.72], solid: false, transparent: true, emission: None, hardness: 2.8,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Acid8", color: [0.58, 0.92, 0.12, 0.72], solid: false, transparent: true, emission: None, hardness: 3.2,
        tex_top: &[], tex_side: &[], tex_bottom: &[], overlay_side: &[] },
    BlockDef { name: "Ice", color: [0.72, 0.85, 0.95, 1.0], solid: true, transparent: false, emission: None, hardness: 2.5,
        tex_top: &["textures/ice.png"], tex_side: &["textures/ice.png"], tex_bottom: &["textures/ice.png"], overlay_side: &[] },
    BlockDef { name: "Gravel", color: [0.5, 0.48, 0.46, 1.0], solid: true, transparent: false, emission: None, hardness: 0.9,
        tex_top: &["textures/gravel.png"], tex_side: &["textures/gravel.png"], tex_bottom: &["textures/gravel.png"], overlay_side: &[] },
    BlockDef { name: "Clay", color: [0.62, 0.6, 0.58, 1.0], solid: true, transparent: false, emission: None, hardness: 0.9,
        tex_top: &["textures/clay.png"], tex_side: &["textures/clay.png"], tex_bottom: &["textures/clay.png"], overlay_side: &[] },
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

/// ไฟ "dynamic" = ควบคุมได้ด้วยระบบไฟฟ้า (เปิด/ปิด/หรี่/เปลี่ยนสีในอนาคต) → ใช้ PointLight จริง
/// (เปลี่ยนสด ไม่ต้อง remesh). ไฟ static (คบไฟ/glowstone/โคมสี/campfire) ใช้ baked overlay อย่างเดียว
pub fn is_dynamic_emitter(block: BlockType) -> bool {
    matches!(block, BlockType::SmartLamp | BlockType::SmartLampOn)
}

/// กล่อง collision จริงของบล็อก (มุมล่าง, มุมบน ภายในช่อง 1x1x1 ของตัวเอง) — ค่าเริ่มต้นคือ
/// คิวบ์เต็มช่องเดิมสำหรับบล็อกทุกชนิด ยกเว้นบล็อกที่ไม่ใช่คิวบ์เต็ม (เช่น Campfire) ที่ระบุ
/// กล่องเล็กกว่าจริงไว้เฉพาะที่นี่ — ไม่ต้องเพิ่ม field ใน BlockDef/BLOCK_DEFS ทั้งตาราง
pub fn block_collision_box(block: BlockType) -> (Vec3, Vec3) {
    match block {
        BlockType::Campfire => (Vec3::new(0.15, 0.0, 0.15), Vec3::new(0.85, 0.4, 0.85)),
        BlockType::Branch => (Vec3::new(0.25, 0.0, 0.25), Vec3::new(0.75, 1.0, 0.75)),
        BlockType::Crucible => (Vec3::new(0.15, 0.0, 0.15), Vec3::new(0.85, 0.7, 0.85)),
        BlockType::IngotMold | BlockType::PickaxeMold => (Vec3::ZERO, Vec3::new(1.0, 5.0 / 16.0, 1.0)),
        BlockType::CastIngot => (
            Vec3::new(3.0 / 16.0, 0.0, 5.0 / 16.0),
            Vec3::new(13.0 / 16.0, 3.0 / 16.0, 11.0 / 16.0),
        ),
        BlockType::SmartLamp | BlockType::SmartLampOn => (Vec3::new(0.25, 0.0, 0.25), Vec3::new(0.75, 0.6, 0.75)),
        _ => (Vec3::ZERO, Vec3::ONE),
    }
}

/// เหมือน block_collision_box แต่รู้ตำแหน่งด้วย — ใช้กับบล็อกที่รูปทรงขึ้นกับเพื่อนบ้าน
/// ตอนนี้คือ Branch: กล่องต้องวางตามทิศที่กิ่งเชื่อมจริง ไม่ใช่เสาตั้งตายตัว
/// (กิ่งแนวนอนจะได้ชนตรงกับที่ตาเห็น)
pub fn block_collision_box_at(world: &VoxelWorld, pos: IVec3, block: BlockType) -> (Vec3, Vec3) {
    if block == BlockType::CastIngot {
        let (min, mut max) = block_collision_box(block);
        let fill = world
            .placed_ingots
            .get(&pos)
            .map_or(1.0, |ingot| {
                ingot.mass as f32 / crate::chemistry::INGOT_MOLD_CAPACITY_GRAMS as f32
            })
            .clamp(0.1, 1.0);
        max.y *= fill;
        return (min, max);
    }
    if !matches!(block, BlockType::Branch | BlockType::MapleBranch) {
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

fn ray_aabb_hit(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<(f32, IVec3)> {
    let mut near = 0.0f32;
    let mut far = f32::INFINITY;
    let mut normal = IVec3::ZERO;
    for axis in 0..3 {
        if dir[axis].abs() < 1e-7 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let mut t1 = (min[axis] - origin[axis]) / dir[axis];
        let mut t2 = (max[axis] - origin[axis]) / dir[axis];
        let mut axis_normal = IVec3::ZERO;
        axis_normal[axis] = if dir[axis] > 0.0 { -1 } else { 1 };
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        if t1 > near {
            near = t1;
            normal = axis_normal;
        }
        far = far.min(t2);
        if far < near {
            return None;
        }
    }
    (far >= 0.0).then_some((near, normal))
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
        | BlockType::SwitchOff | BlockType::SwitchOn 
        | BlockType::Crucible | BlockType::IngotMold | BlockType::PickaxeMold | BlockType::CastIngot
        | BlockType::CopperOre | BlockType::IronOre | BlockType::CoalOre | BlockType::TinOre
        | BlockType::ZincOre | BlockType::Limestone | BlockType::Ice => DigClass::Pick,
        BlockType::OakWood | BlockType::MapleLog | BlockType::Chest | BlockType::Tnt | BlockType::Nuke
        | BlockType::Campfire | BlockType::Branch | BlockType::MapleBranch
        | BlockType::SpruceLog | BlockType::SpruceLogDamaged1 | BlockType::SpruceLogDamaged2 => DigClass::Axe,
        BlockType::Dirt | BlockType::Grass | BlockType::Sand
        | BlockType::SnowyGrass | BlockType::Snow
        | BlockType::Gravel | BlockType::Clay => DigClass::Shovel,
        _ => DigClass::None,
    }
}

/// เวลาขุดด้วยมือเปล่า (วินาที) — ปรับสมดุลเกมที่ตารางนี้ที่เดียว
pub fn block_dig_time(block: BlockType) -> f32 {
    match block {
        BlockType::TallGrass | BlockType::Campfire => 0.2,
        BlockType::Leaves | BlockType::MapleLeaves | BlockType::SpruceLeaves => 0.35,
        BlockType::Glass => 0.5,
        BlockType::Snow => 0.5,
        BlockType::Sand | BlockType::Gravel | BlockType::Clay => 0.75,
        BlockType::Ice => 1.0,
        BlockType::Dirt | BlockType::Tnt | BlockType::Nuke => 1.0,
        BlockType::Grass | BlockType::SnowyGrass => 1.2,
        BlockType::Glowstone | BlockType::LampRed | BlockType::LampGreen | BlockType::LampBlue
        | BlockType::SmartLamp | BlockType::SmartLampOn
        | BlockType::SwitchOff | BlockType::SwitchOn => 1.5,
        BlockType::OakWood | BlockType::MapleLog | BlockType::Chest
        | BlockType::Branch | BlockType::MapleBranch
        | BlockType::SpruceLog | BlockType::SpruceLogDamaged1 | BlockType::SpruceLogDamaged2 => 3.0,
        BlockType::Furnace => 3.5,
        BlockType::Stone | BlockType::CopperOre | BlockType::IronOre | BlockType::CoalOre
        | BlockType::TinOre | BlockType::ZincOre | BlockType::Limestone => 5.0,
        BlockType::IronBlock => 7.5,
        _ => 1.0,
    }
}

/// กติกา drop แบบ Minecraft: หมวด Pick (หิน/แร่) ต้องถือ pickaxe ตอนแตกถึงได้ของ
/// มือเปล่า/tool ผิดหมวดขุดได้ (ช้า) แต่บล็อกหายเปล่า — หมวดอื่นได้ของเสมอ
pub fn block_requires_tool(block: BlockType) -> bool {
    if block == BlockType::Crucible {
        return false;
    }
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

/// noise ของโลกที่กำลังเล่น — mesher อ่านเพื่อย้อมสีหญ้า/ใบตาม biome
/// (แนวเดียวกับ ACTIVE_SAVE_DIR — เลี่ยงร้อยสาย noise ผ่าน remesh caller ทั้งหมด)
static WORLDGEN_CLIMATE: std::sync::RwLock<Option<crate::NoiseParams>> =
    std::sync::RwLock::new(None);

/// ตั้ง climate ของโลกปัจจุบัน (เรียกตอน generate chunk — โลกเดียวค่าคงที่ เขียนซ้ำไม่เป็นไร)
pub fn set_worldgen_climate(noise: crate::NoiseParams) {
    if let Ok(mut g) = WORLDGEN_CLIMATE.write() {
        *g = Some(noise);
    }
}

/// ชุด biome ของโลกที่กำลัง gen — worldgen (height/surface/trees) อ่านจากที่นี่ (แนวเดียว
/// WORLDGEN_CLIMATE) เพราะ gen ทำในงาน async ที่เข้าถึง resource ไม่ได้. host/client ตั้งค่านี้
/// ให้ตรงกันก่อน regen → terrain ตรงกัน
static WORLDGEN_BIOMES: std::sync::RwLock<Option<Arc<crate::biomegen::BiomeConfig>>> =
    std::sync::RwLock::new(None);

/// ตั้งชุด biome ของโลกปัจจุบัน (เรียกตอนเข้าโลก/รับ config จาก host)
pub fn set_worldgen_biomes(cfg: crate::biomegen::BiomeConfig) {
    if let Ok(mut g) = WORLDGEN_BIOMES.write() {
        *g = Some(Arc::new(cfg));
    }
}

/// ชุด biome ปัจจุบัน (default ถ้ายังไม่ตั้ง) — TerrainSampler หยิบตอน new()
pub fn worldgen_biomes() -> Arc<crate::biomegen::BiomeConfig> {
    WORLDGEN_BIOMES
        .read()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_else(|| Arc::new(crate::biomegen::BiomeConfig::default()))
}

pub static CURRENT_DAY_OF_YEAR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(172);

/// ตัวคูณสีหญ้า/ใบตาม biome ที่กลาง chunk — biome เปลี่ยนช้า (~330 บล็อก) คงที่ต่อ chunk พอ
fn foliage_tint_for_chunk(chunk_pos: IVec2) -> [f32; 3] {
    let Some(noise) = WORLDGEN_CLIMATE.read().ok().and_then(|g| *g) else {
        return crate::biome::foliage_color(0.2, 0.3); // เขียวเขตอบอุ่น กัน grayscale เป็นขาว
    };
    let wx = chunk_pos.x as f64 * CHUNK_WIDTH as f64 + 8.0;
    let wz = chunk_pos.y as f64 * CHUNK_WIDTH as f64 + 8.0;
    let sampler = TerrainSampler::new(noise);

    let lat = crate::biome::climate_lat(wz);
    // เก็บเฉพาะสีภูมิอากาศฐานไว้ใน vertex; สีฤดูถูกผสมใน foliage shader
    // เพื่อให้เวลาเดินได้โดยไม่ต้อง remesh chunk
    let temp = crate::biome::temp_from_latitude(lat);
    crate::biome::foliage_color(temp, sampler.humidity_raw(wx, wz))
}

const TREE_SEASON_JITTER_DAYS: f32 = 21.0;
const FOLIAGE_FALLBACK_BUCKET: i32 = 8;

fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

pub(crate) fn seasonal_offset_from_key(key: IVec3, world_seed: u32) -> f32 {
    let mut h = world_seed as u64 ^ 0x6a09_e667_f3bc_c909;
    h = mix64(h ^ key.x as u32 as u64);
    h = mix64(h ^ (key.y as u32 as u64).rotate_left(21));
    h = mix64(h ^ (key.z as u32 as u64).rotate_left(42));
    let unit = (h >> 11) as f64 / ((1u64 << 53) - 1) as f64;
    ((unit as f32) * 2.0 - 1.0) * TREE_SEASON_JITTER_DAYS
}

fn root_for_branch(network: &crate::tree::BranchNetwork, start: IVec3) -> IVec3 {
    let mut current = start;
    // กิ่งจริงสั้นกว่านี้มาก; limit ป้องกันข้อมูลเซฟเสียแล้ว parent วนเป็นวง
    for _ in 0..512 {
        let Some(node) = network.nodes.get(&current) else { break };
        let Some(parent) = node.parent_pos else { break };
        current = parent;
    }
    current
}

fn seasonal_offset_for_leaf(
    network: Option<&crate::tree::BranchNetwork>,
    world_pos: IVec3,
    world_seed: u32,
) -> f32 {
    let mut best: Option<((i32, i32, [i32; 3]), IVec3)> = None;
    if let Some(network) = network {
        for dy in -LEAF_SUPPORT_RANGE..=LEAF_SUPPORT_RANGE {
            for dz in -LEAF_SUPPORT_RANGE..=LEAF_SUPPORT_RANGE {
                for dx in -LEAF_SUPPORT_RANGE..=LEAF_SUPPORT_RANGE {
                    let branch = world_pos + IVec3::new(dx, dy, dz);
                    if !network.nodes.contains_key(&branch) {
                        continue;
                    }
                    let cheb = dx.abs().max(dy.abs()).max(dz.abs());
                    let dist2 = dx * dx + dy * dy + dz * dz;
                    let candidate = ((cheb, dist2, branch.to_array()), branch);
                    if best.as_ref().is_none_or(|old| candidate.0 < old.0) {
                        best = Some(candidate);
                    }
                }
            }
        }
    }
    let key = best
        .map(|(_, branch)| root_for_branch(network.unwrap(), branch))
        .unwrap_or_else(|| {
            IVec3::new(
                world_pos.x.div_euclid(FOLIAGE_FALLBACK_BUCKET) * FOLIAGE_FALLBACK_BUCKET,
                0,
                world_pos.z.div_euclid(FOLIAGE_FALLBACK_BUCKET) * FOLIAGE_FALLBACK_BUCKET,
            )
        });
    seasonal_offset_from_key(key, world_seed)
}

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
/// 512 = โลก noise ทั่วไป (ทะเล y96 + ภูเขา + ฟ้า) — section storage เก็บฟ้า/หินตัน 1 byte/16 ชั้น
pub const CHUNK_HEIGHT: usize = 512;
pub const CHUNK_VOLUME: usize = CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH;
pub const SEA_LEVEL: usize = 96;
/// เจาะถ้ำเฉพาะแถบใต้ผิวลึกไม่เกินนี้ — is_cave (Perlin 3D) เป็นต้นทุนหลักของ gen
/// ลึกกว่านี้เป็นหินตันล้วน (ไม่มีใครเห็น) จึงข้ามการเช็คเพื่อความเร็ว
pub const CAVE_DEPTH: i32 = 64;

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

    /// Fill an inclusive vertical range while materializing each touched
    /// section at most once. Used by worldgen for the common solid-column path.
    pub fn fill_column_range(
        &mut self,
        x: usize,
        z: usize,
        y_start: usize,
        y_end: usize,
        block: BlockType,
    ) {
        if y_start > y_end || y_start >= CHUNK_HEIGHT {
            return;
        }
        let y_end = y_end.min(CHUNK_HEIGHT - 1);
        let first_section = y_start / SECTION_H;
        let last_section = y_end / SECTION_H;
        for si in first_section..=last_section {
            let lo = y_start.max(si * SECTION_H) - si * SECTION_H;
            let hi = y_end.min((si + 1) * SECTION_H - 1) - si * SECTION_H;
            let section = &mut self.sections[si];
            if let Section::Uniform(existing) = section {
                if *existing == block {
                    continue;
                }
                *section = Section::Dense(Box::new([*existing; SECTION_VOLUME]));
            }
            if let Section::Dense(values) = section {
                for yl in lo..=hi {
                    values[Section::idx(x, yl, z)] = block;
                }
            }
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

#[derive(Clone, Default)]
pub struct FurnaceData {
    pub slots: [Option<ItemStack>; 2],
    pub current_temp: f32,
    pub active_fuel_energy: f32,
    pub active_fuel_base_temp: f32,
    pub air_multiplier: f32,
    pub air_boost_time: f32,
    pub smelting_progress: f32,
}

pub struct ChunkData {
    pub blocks: Arc<ChunkBlocks>,
    pub chiseled_blocks: HashMap<usize, Box<[u8; 4096]>>,
    /// หน้า "หน้า" ของ Furnace/Chest ต่อตำแหน่ง (เก็บเป็น face_id 2/3/4/5) — เหมือน chiseled_blocks
    /// ต่างกันที่อันนี้ต้องเซฟลง disk จริง (ดู save_chunk_full/load_chunk_aux)
    pub facings: HashMap<usize, u8>,
    /// ของในกล่อง Chest ต่อตำแหน่ง (27 ช่อง) — เซฟลง disk เหมือน facings
    pub chest_slots: HashMap<usize, Box<[Option<ItemStack>; 27]>>,

    /// ข้อมูลในกล่อง Furnace ต่อตำแหน่ง (3 ช่อง + อุณหภูมิ)
    pub furnace_slots: HashMap<usize, Box<FurnaceData>>,
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
    /// block light สี (RGB) ต่อบล็อก (แสงจากโคม/คบไฟ) — แยกจาก sky เพื่อไม่โดน day/night หรี่
    pub block_light: Arc<crate::light::BlockLight>,
    /// ต้องคำนวณ light ใหม่ก่อน mesh รอบหน้า (บล็อกเปลี่ยน/เพิ่งโหลด)
    pub light_dirty: bool,
    /// Increments whenever blocks/neighbors invalidate this light snapshot.
    pub light_revision: u64,
    /// bitmask ของเพื่อนบ้าน (ลำดับตาม chunk_neighbors) ที่ "ยังไม่โหลด" ตอนคำนวณแสงครั้งล่าสุด
    /// — ตอนนั้นถือว่าเป็นฟ้าโล่ง ค่าตรงขอบจึงเพี้ยน พอตัวจริงมาถึงค่อยปลุกคิดใหม่
    /// เฉพาะ chunk ที่รอตัวนั้นอยู่จริง (เดิมปลุกเพื่อนบ้านทั้ง 8 ทุกครั้งที่มี chunk ใหม่
    /// ซึ่งลาม remesh เป็น 9 chunk ต่อครั้ง = เฟรมตกและภาพกระพริบ)
    pub light_missing_neighbors: u8,
    /// Cache ของตำแหน่งบล็อกที่เปล่งแสง (local coords) เพื่อไม่ต้องสแกนทั้ง chunk ทุกเฟรม
    pub emitters: std::collections::HashSet<IVec3>,
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
                if b.is_fluid() {
                    min_y = min_y.min(si * SECTION_H);
                    max_y = max_y.max(si * SECTION_H + SECTION_H - 1);
                }
            }
            Section::Dense(a) => {
                for y_local in 0..SECTION_H {
                    let y = si * SECTION_H + y_local;
                    'row: for z in 0..CHUNK_WIDTH {
                        for x in 0..CHUNK_WIDTH {
                            if a[Section::idx(x, y_local, z)].is_fluid() {
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
    pub block_light_chunks: HashMap<IVec2, Entity>,   // mesh entity (overlay แสงโคม additive)
    pub deco_chunks: HashMap<IVec2, Vec<Entity>>,     // mesh entity (ของประดับกากบาทและพู่หญ้า)
    pub seasonal_foliage_chunks: HashMap<IVec2, Entity>, // Oak leaves custom material
    pub maple_foliage_chunks: HashMap<IVec2, Entity>,
    pub glow_chunks: HashMap<IVec2, Vec<Entity>>,     // mesh entity (บล็อกเรืองแสง ต่อสี)
    pub textured_chunks: HashMap<IVec2, Vec<Entity>>, // mesh entity (บล็อกมี texture ต่อไฟล์)
    pub lamp_lights: HashMap<IVec2, Vec<Entity>>,     // PointLight ของบล็อกไฟใน chunk
    pub campfire_models: HashMap<IVec2, Vec<Entity>>, // glTF scene entity ของ Campfire ใน chunk
    pub branch_network: crate::tree::BranchNetwork,
    pub pending_branch_remesh: std::collections::HashSet<IVec2>,
    /// กิ่งที่ parent หายไปแล้ว รอ host ทุบตามเป็นทอดๆ (ดู block_update_system)
    pub pending_branch_orphans: std::collections::HashSet<IVec3>,
    /// ท่อนสน (SpruceLog) ที่ท่อนล่างหายไป รอ host เช็คว่ายังมีที่ยึดพื้นไหม —
    /// ถ้าไม่มีทุบตามเป็นทอดๆ ขึ้นไป (ต้นสนเป็นคิวบ์ ไม่มี BranchNetwork คุม)
    pub pending_spruce_orphans: std::collections::HashSet<IVec3>,
    /// ใบที่อยู่ข้างกิ่งซึ่งเพิ่งหายไป รอเช็คว่ายังมีกิ่งค้ำอยู่ไหม (ดู leaf decay)
    /// เติมเฉพาะตอนกิ่งถูกทำลายจริง — ใบที่ผู้เล่นเอาไปสร้างบ้านไกลๆ จึงไม่ร่วงเอง
    pub pending_leaf_decay: std::collections::HashSet<IVec3>,
    /// chunk ที่ cascade กิ่งไปแก้บล็อกไว้ ต้องเขียนลงดิสก์ (ดู branch_remesh_system)
    pub pending_branch_save: std::collections::HashSet<IVec2>,
    pub total_vertices: usize,
    pub total_indices: usize,
    pub crucibles: HashMap<IVec3, crate::chemistry::CrucibleData>,
    pub ingot_molds: HashMap<IVec3, crate::chemistry::IngotMoldData>,
    pub placed_ingots: HashMap<IVec3, crate::chemistry::CastIngotData>,
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

    pub fn get_furnace_slots(&self, x: i32, y: i32, z: i32) -> Option<&[Option<ItemStack>; 2]> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 { return None; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        self.chunks.get(&IVec2::new(cx, cz)).and_then(|chunk| {
            chunk.furnace_slots.get(&ChunkData::get_index(lx, y as usize, lz)).map(|b| &b.slots)
        })
    }

    pub fn get_furnace_data_mut(&mut self, x: i32, y: i32, z: i32) -> Option<&mut FurnaceData> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 { return None; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        self.chunks.get_mut(&IVec2::new(cx, cz)).and_then(|chunk| {
            chunk.furnace_slots.get_mut(&ChunkData::get_index(lx, y as usize, lz)).map(|b| &mut **b)
        })
    }

    pub fn get_furnace_data(&self, x: i32, y: i32, z: i32) -> Option<&FurnaceData> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 { return None; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        self.chunks.get(&IVec2::new(cx, cz)).and_then(|chunk| {
            chunk.furnace_slots.get(&ChunkData::get_index(lx, y as usize, lz)).map(|b| &**b)
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
        if y < 0 || y >= CHUNK_HEIGHT as i32 || slot >= 2 { return; }
        let (cx, lx) = (x.div_euclid(CHUNK_WIDTH as i32), x.rem_euclid(CHUNK_WIDTH as i32) as usize);
        let (cz, lz) = (z.div_euclid(CHUNK_WIDTH as i32), z.rem_euclid(CHUNK_WIDTH as i32) as usize);
        if let Some(chunk) = self.chunks.get_mut(&IVec2::new(cx, cz)) {
            let idx = ChunkData::get_index(lx, y as usize, lz);
            chunk.furnace_slots.entry(idx).or_insert_with(|| Box::new(FurnaceData::default())).slots[slot] = item;
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
        self.crucibles.remove(&IVec3::new(x, y, z));
        self.ingot_molds.remove(&IVec3::new(x, y, z));
        self.placed_ingots.remove(&IVec3::new(x, y, z));
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

            let old_block = chunk.blocks.get(local_x, local_y, local_z);
            if old_block != block_type {
                let old_is_emitter = crate::light::emitter_rgb(old_block) != [0,0,0];
                let new_is_emitter = crate::light::emitter_rgb(block_type) != [0,0,0];
                if old_is_emitter && !new_is_emitter {
                    chunk.emitters.remove(&IVec3::new(local_x as i32, local_y as i32, local_z as i32));
                } else if !old_is_emitter && new_is_emitter {
                    chunk.emitters.insert(IVec3::new(local_x as i32, local_y as i32, local_z as i32));
                }
            }

            // make_mut ตอนนี้ clone แค่ Vec<Section> + section เดียวที่โดนเขียน (~4KB)
            // — เดิม clone ทั้งคอลัมน์ 128KB ต่อ write แรกหลัง share ให้ mesh task
            Arc::make_mut(&mut chunk.blocks).set(local_x, local_y, local_z, block_type);
            chunk.dirty = true;
            // ขยายแถบน้ำแบบ grow-only (tighten ทีเดียวตอน rebuild mesh น้ำ)
            if block_type.is_fluid() {
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
    /// Secondary per-vertex data. Water uses [depth, shoreline], other meshes use zero.
    pub uv_b: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl MeshBuf {
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    pub(crate) fn push_quad(
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
            self.uv_b.push([0.0, 0.0]);
        }
        // flip = สลับ diagonal ของ quad (ใช้ตอน AO ไม่สมมาตร กัน interpolation เบี้ยว)
        if flip {
            self.indices.extend_from_slice(&[vc, vc + 1, vc + 3, vc + 1, vc + 2, vc + 3]);
        } else {
            self.indices.extend_from_slice(&[vc, vc + 1, vc + 2, vc, vc + 2, vc + 3]);
        }
    }

    fn push_water_quad(
        &mut self,
        verts: [[f32; 3]; 4],
        normal: [f32; 3],
        cols: [[f32; 4]; 4],
        flow: [[f32; 2]; 4],
        water_data: [[f32; 2]; 4],
        flip: bool,
    ) {
        let first = self.positions.len();
        self.push_quad(verts, normal, cols, flow, flip);
        self.uv_b[first..first + 4].copy_from_slice(&water_data);
    }

    pub fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.uv_b);
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
    /// ใบไม้ผลัดใบ ใช้ custom material เพื่อเปลี่ยนสีตามฤดูโดยไม่ remesh
    pub seasonal_foliage: MeshBuf,
    /// Maple leaves use the same seasonal shader with a separate texture/palette.
    pub maple_foliage: MeshBuf,
    /// บล็อกเรืองแสง แยกต่อชนิด (material emissive)
    pub glow: Vec<(BlockType, MeshBuf)>,
    /// บล็อกมี texture แยกต่อไฟล์ texture
    pub textured: Vec<(&'static str, MeshBuf)>,
    /// overlay แสงจากโคม (additive) — วาดบวกทับ ไม่โดน day/night tint = สว่างกลางคืน
    pub block_overlay: MeshBuf,
}

impl ChunkMeshSet {
    pub fn total_vertices(&self) -> usize {
        self.solid.positions.len()
            + self.water.positions.len()
            + self.glass.positions.len()
            + self.deco.iter().map(|(_, b)| b.positions.len()).sum::<usize>()
            + self.seasonal_foliage.positions.len()
            + self.maple_foliage.positions.len()
            + self.glow.iter().map(|(_, b)| b.positions.len()).sum::<usize>()
            + self.textured.iter().map(|(_, b)| b.positions.len()).sum::<usize>()
    }

    pub fn total_indices(&self) -> usize {
        self.solid.indices.len()
            + self.water.indices.len()
            + self.glass.indices.len()
            + self.deco.iter().map(|(_, b)| b.indices.len()).sum::<usize>()
            + self.seasonal_foliage.indices.len()
            + self.maple_foliage.indices.len()
            + self.glow.iter().map(|(_, b)| b.indices.len()).sum::<usize>()
            + self.textured.iter().map(|(_, b)| b.indices.len()).sum::<usize>()
    }
}

/// สีของ overlay แสงโคม (additive) จากระดับ block light RGB 0-15 ต่อ channel
fn block_glow_color(rgb: [u8; 3]) -> [f32; 4] {
    // โค้ง t² ต่อ channel ให้ขอบแสงจางเนียน ตรงกลางโคมเด่น
    let f = |v: u8| { let t = v as f32 / 15.0; t * t };
    [f(rgb[0]), f(rgb[1]), f(rgb[2]), 1.0]
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
fn get_water_surface(
    sample: &impl Fn(i32, i32, i32) -> BlockType,
    cx: i32,
    cz: i32,
    base_y: i32,
) -> Option<f32> {
    for y in (base_y - 2..=base_y + 2).rev() {
        let b = sample(cx, y, cz);
        if b.is_fluid() {
            let drop = match b {
                BlockType::Water7 => 0.125,
                BlockType::Water6 => 0.25,
                BlockType::Water5 => 0.375,
                BlockType::Water4 => 0.50,
                BlockType::Water3 => 0.625,
                BlockType::Water2 => 0.75,
                BlockType::Water1 => 0.875,
                // Lava keeps a thicker lip than water at the same simulated
                // volume, making its surface read as a viscous sheet.
                BlockType::Lava7 => 0.08,
                BlockType::Lava6 => 0.14,
                BlockType::Lava5 => 0.20,
                BlockType::Lava4 => 0.27,
                BlockType::Lava3 => 0.35,
                BlockType::Lava2 => 0.44,
                BlockType::Lava1 => 0.55,
                _ => WATER_SURFACE_DROP,
            };
            let drop = if sample(cx, y + 1, cz).is_fluid() { 0.0 } else { drop };
            return Some(y as f32 + 1.0 - drop);
        }
    }
    None
}

fn water_corner_info(
    sample: &impl Fn(i32, i32, i32) -> BlockType,
    cx: i32,
    vy: i32,
    cz: i32,
) -> (f32, f32, f32) {
    let mut sum_y = 0.0;
    let mut cnt = 0;
    let mut depth_sum = 0.0;
    let mut depth_cnt = 0;

    for dx in -1..=0 {
        for dz in -1..=0 {
            if let Some(sy) = get_water_surface(sample, cx + dx, cz + dz, vy) {
                sum_y += sy;
                cnt += 1;
            }
            
            // Depth calculation
            let b = sample(cx + dx, vy, cz + dz);
            if b.is_fluid() {
                let mut d = 0i32;
                while d < WATER_DEPTH_RANGE && sample(cx + dx, vy - d, cz + dz).is_fluid() {
                    d += 1;
                }
                depth_sum += d as f32;
                depth_cnt += 1;
            }
        }
    }

    if cnt > 0 {
        let avg_y = sum_y / cnt as f32;
        let corner_y = avg_y.max(vy as f32);
        let drop = (vy as f32 + 1.0) - corner_y;
        let depth = if depth_cnt > 0 { (depth_sum / depth_cnt as f32 / WATER_DEPTH_RANGE as f32).min(1.0) } else { 0.0 };
        let shoreline = 1.0 - cnt as f32 / 4.0;
        (drop, depth, shoreline)
    } else {
        (0.0, 0.0, 0.0)
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
    breaking_target: Option<(IVec3, f32)>,
) -> ChunkMeshSet {
    create_mesh_from_blocks_with_xray(
        chunk_pos,
        blocks,
        neighbors,
        chiseled_blocks,
        facings,
        branch_network,
        light,
        breaking_target,
        DEBUG_XRAY_ENABLED.load(Ordering::Relaxed),
    )
}

fn create_mesh_from_blocks_with_xray(
    chunk_pos: IVec2,
    blocks: &ChunkBlocks,
    neighbors: &[Arc<ChunkBlocks>; 8],
    chiseled_blocks: Option<&HashMap<usize, Box<[u8; 4096]>>>,
    facings: Option<&HashMap<usize, u8>>,
    branch_network: Option<&crate::tree::BranchNetwork>,
    light: Option<&LightNeighborhood>,
    breaking_target: Option<(IVec3, f32)>,
    xray: bool,
) -> ChunkMeshSet {
    // ต่อมุมผิวน้ำ: (ระยะกดผิวลง, ความลึกน้ำ normalize 0..1) — แชร์ข้ามหน้า/บล็อก
    let mut drop_cache: HashMap<(i32, i32, i32), (f32, f32, f32)> =
        HashMap::with_capacity(1024);
    
    let mut set = ChunkMeshSet::default();

    // พิกัดโลกของมุม chunk — ใช้ hash เลือกลาย texture ให้ต่อเนื่องข้าม chunk
    let world_base_x = chunk_pos.x * CHUNK_WIDTH as i32;
    let world_base_z = chunk_pos.y * CHUNK_WIDTH as i32;

    // สีหญ้า/ใบตาม biome (คูณเข้า vertex color ของหญ้าบน/พู่หญ้า/ใบผลัดใบ) — แบบ Minecraft
    let foliage = foliage_tint_for_chunk(chunk_pos);

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
        let block = src.get(lx, y as usize, lz);
        if xray && xray_hidden_block(block) {
            BlockType::Air
        } else {
            block
        }
    };

    // Vertex AO: แต่ละมุมของหน้า เช็คบล็อกข้าง 2 + มุม 1 บนระนาบนอกหน้า
    let face_ao = |c: [i32; 3], face_id: usize| -> [u8; 4] {
        if xray {
            return [3; 4];
        }
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
        if xray {
            return [crate::light::MAX_LIGHT; 4];
        }
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

    // block light สี (RGB) ที่หน้านี้ — sample เซลล์ตรงหน้า (ที่แสงโคมอยู่)
    let face_block = |c: [i32; 3], face_id: usize| -> [u8; 3] {
        if xray {
            return [0, 0, 0];
        }
        let Some(lm) = light else { return [0, 0, 0] };
        let n = FACE_OFFSETS[face_id];
        lm.get_block(c[0] + n[0], c[1] + n[1], c[2] + n[2])
    };

    // สีของแผ่น sprite/กิ่ง ที่ไม่ได้ผ่านทาง AO ของหน้าคิวบ์ — ใช้แสงของช่องตัวเอง
    // (ถ้าไม่คูณ ใบไม้กับกิ่งจะสว่างเต็มแม้อยู่ในถ้ำ ลอยเด่นผิดที่ผิดทาง)
    let block_tint = |xi: i32, yi: i32, zi: i32| -> [f32; 4] {
        if xray {
            return [1.0; 4];
        }
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
        let mut mask: Vec<Option<(BlockType, u8, u8, u8, [u8; 3])>> = vec![None; (lu * lv) as usize];

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
                    if xray && xray_hidden_block(block) {
                        continue;
                    }
                    // TallGrass ไม่ใช่ลูกบาศก์ — วาดแยกเป็นกากบาทท้ายฟังก์ชัน
                    // Chiseled ข้ามไปก่อน วาดแยกทีหลัง
                    // Branch เป็น Tapered Cylinder
                    // Leaves / Spruce Leaves เป็นแผ่น sprite ตัดกันแบบดาว 3 แกน
                    if block == BlockType::Air || block == BlockType::TallGrass || block == BlockType::Chiseled || block == BlockType::Campfire || block == BlockType::SmartLamp || block == BlockType::SmartLampOn || block == BlockType::Branch || block == BlockType::MapleBranch || block == BlockType::Leaves || block == BlockType::MapleLeaves || block == BlockType::SpruceLeaves || block == BlockType::Crucible || block == BlockType::IngotMold || block == BlockType::PickaxeMold || block == BlockType::CastIngot {
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
                    if block.is_fluid() && n.is_fluid() {
                        continue;
                    }
                    if block.is_fluid() {
                        // Cull internal side faces when adjacent to a downward ramp
                        if a != 1 && n == BlockType::Air {
                            if sample(c[0] + norm[0], c[1] - 1, c[2] + norm[2]).is_fluid() {
                                continue;
                            }
                        }
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
                            // สีเขียวตาม biome × แสงของช่องอากาศที่หน้านี้หันหา
                            // (กันหญ้าเรืองเขียวในถ้ำ/เงา — เทียบเท่าที่หน้าดินได้รับ)
                            let lb = block_tint(c[0] + norm[0], c[1] + norm[1], c[2] + norm[2])[0];
                            let ov = [foliage[0] * lb, foliage[1] * lb, foliage[2] * lb, 1.0];
                            texture_buf(&mut set.deco, overlay)
                                .push_quad(verts, [0., 1., 0.], [ov; 4], uvs, false);
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
                    // block light ของหน้านี้ (โคมไม่ต้อง overlay — หน้าตัวเองใช้ glow material อยู่แล้ว)
                    let block_level = if lamp_emission(block).is_some() { [0, 0, 0] } else { face_block(c, face_id) };

                    if !block.is_fluid()
                        && ao[0] == ao[1] && ao[1] == ao[2] && ao[2] == ao[3]
                        && lit[0] == lit[1] && lit[1] == lit[2] && lit[2] == lit[3]
                    {
                        mask[midx(ui, vi)] = Some((block, ao[0], variant, lit[0], block_level));
                    } else {
                        // AO ไล่เฉดภายในหน้า — merge ไม่ได้ วาดเดี่ยวพร้อม flip diagonal
                        let tex = face_texture(block, face_id, variant);
                        let base = if tex.is_some() { [1.0, 1.0, 1.0, 1.0] } else { block_color(block) };
                        let shade = FACE_SHADE[face_id];
                        let mut verts = [[0f32; 3]; 4];
                        let mut cols = [[0f32; 4]; 4];
                        let mut uvs = [[0f32; 2]; 4];
                        let mut water_data = [[0f32; 2]; 4];
                        let (vx, vy, vz) = (c[0], c[1], c[2]);
                        let is_w = block.is_fluid();
                        // หน้าหญ้าบนย้อมสีตาม biome (แบบ Minecraft) — path AO ไล่เฉด
                        let fol = if block == BlockType::Grass && face_id == 0 { foliage } else { [1.0; 3] };
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
                                let (d, dep, _) = *drop_cache.entry((cx, vy, cz)).or_insert_with(|| {
                                    water_corner_info(&sample, cx, vy, cz)
                                });
                                corner_drop[i] = d;
                                corner_depth[i] = dep;
                            }
                        }
                        let (flow_vec, speed) = if is_w {
                            crate::hydro::river_at((world_base_x + vx) as f64 + 0.5, (world_base_z + vz) as f64 + 0.5)
                                .map(|r| (r.flow, r.speed))
                                .unwrap_or((Vec2::ZERO, 0.0))
                        } else {
                            (Vec2::ZERO, 0.0)
                        };
                        let mut water_flow_uv = [flow_vec.x * speed, flow_vec.y * speed];
                        if is_w && face_id == 0 {
                            let slope_x = (corner_drop[1] + corner_drop[2] - corner_drop[0] - corner_drop[3]) * 0.5;
                            let slope_z = (corner_drop[0] + corner_drop[1] - corner_drop[2] - corner_drop[3]) * 0.5;
                            water_flow_uv[0] += slope_x * 2.0;
                            water_flow_uv[1] += slope_z * 2.0;
                        }

                        for i in 0..4 {
                            let p = CUBE_POSITIONS[face_id][i];
                            verts[i] = [p[0] + vx as f32, p[1] + vy as f32, p[2] + vz as f32];
                            if is_w && p[1] > 0.5 { verts[i][1] -= corner_drop[i]; }
                            let br = shade * AO_CURVE[ao[i] as usize] * sky_curve(lit[i]);
                            // น้ำลึกสีเข้มกว่า (corner_depth = 0 สำหรับบล็อกอื่น)
                            let tint = 1.0 - WATER_DEPTH_DARKEN * corner_depth[i];
                            let a = if is_w { WATER_ALPHA } else { base[3] };
                            cols[i] = [base[0] * br * tint * fol[0], base[1] * br * tint * fol[1], base[2] * br * tint * fol[2], a];
                            uvs[i] = if is_w { water_flow_uv } else { face_uv(verts[i]) };
                            if is_w {
                                let depth = corner_depth[i];
                                let p = CUBE_POSITIONS[face_id][i];
                                let (_, _, shoreline) = *drop_cache
                                    .get(&(vx + p[0] as i32, vy, vz + p[2] as i32))
                                    .expect("water corner was cached above");
                                water_data[i] = [
                                    depth,
                                    if block.is_lava() {
                                        -1.0 - shoreline
                                    } else {
                                        shoreline
                                    },
                                ];
                            }
                        }
                        let flip = (ao[0] as u32 + ao[2] as u32) < (ao[1] as u32 + ao[3] as u32);
                        let buf = if is_w {
                            &mut set.water
                        } else if let Some(t) = tex {
                            texture_buf(&mut set.textured, t)
                        } else {
                            &mut set.solid
                        };
                        if is_w {
                            buf.push_water_quad(
                                verts,
                                CUBE_NORMALS[face_id],
                                cols,
                                uvs,
                                water_data,
                                flip,
                            );
                        } else {
                            buf.push_quad(verts, CUBE_NORMALS[face_id], cols, uvs, flip);
                        }
                        if block_level != [0, 0, 0] && !is_w {
                            set.block_overlay.push_quad(verts, CUBE_NORMALS[face_id], [block_glow_color(block_level); 4], uvs, flip);
                        }
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

                    let (block, ao_level, variant, light_level, block_level) = key;
                    let is_water = block.is_fluid();
                    let is_glass = block == BlockType::Glass;
                    let is_lamp = lamp_emission(block).is_some();
                    let tex = if is_water || is_glass || is_lamp {
                        None
                    } else {
                        face_texture(block, face_id, variant)
                    };
                    let base = if tex.is_some() { [1.0, 1.0, 1.0, 1.0] } else { block_color(block) };
                    let br = FACE_SHADE[face_id] * AO_CURVE[ao_level as usize] * sky_curve(light_level);
                    // หน้าหญ้าบน (face 0) ย้อมสีตาม biome แบบ Minecraft (หน้าอื่น/บล็อกอื่นไม่แตะ)
                    let fol = if block == BlockType::Grass && face_id == 0 { foliage } else { [1.0; 3] };
                    let col = [base[0] * br * fol[0], base[1] * br * fol[1], base[2] * br * fol[2], base[3]];

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
                    if block_level != [0, 0, 0] && !is_water && !is_lamp {
                        set.block_overlay.push_quad(verts, CUBE_NORMALS[face_id], [block_glow_color(block_level); 4], uvs, false);
                    }
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
            let t = block_tint(xi as i32, yi as i32, zi as i32);
            let tint = [t[0] * foliage[0], t[1] * foliage[1], t[2] * foliage[2], t[3]];

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
    let world_seed = WORLDGEN_CLIMATE
        .read()
        .ok()
        .and_then(|g| *g)
        .map_or(0, |noise| noise.seed);
    for leaf in [
        BlockType::Leaves,
        BlockType::MapleLeaves,
        BlockType::SpruceLeaves,
    ] {
        let Some(sprite) = face_texture(leaf, 2, 0) else { continue };
        // ใบผลัดใบ (Leaves) ย้อมสีตาม biome; ใบสน (SpruceLeaves) เขียวคงที่ทุกโซน
        let seasonal = matches!(leaf, BlockType::Leaves | BlockType::MapleLeaves);
        let leaf_fol = if seasonal { foliage } else { [1.0; 3] };
        blocks.for_each_matching(|b| b == leaf, |xi, yi, zi, _| {
            // ใบที่ถูกใบ/บล็อกทึบล้อมครบหกด้านมองไม่เห็นอยู่แล้ว — ข้ามไปเลย
            // (พุ่มหนาๆ ประหยัด quad ได้เยอะโดยหน้าตาไม่เปลี่ยน)
            let (cx, cy, cz) = (xi as i32, yi as i32, zi as i32);
            let hidden = FACE_OFFSETS.iter().all(|o| {
                let n = sample(cx + o[0], cy + o[1], cz + o[2]);
                n == leaf || !block_def(n).transparent && n != BlockType::Air
            });
            if hidden {
                return;
            }
            let t = block_tint(cx, cy, cz);
            let seasonal_offset = if seasonal {
                let world_pos = IVec3::new(world_base_x + cx, cy, world_base_z + cz);
                seasonal_offset_for_leaf(branch_network, world_pos, world_seed)
            } else {
                0.0
            };
            // Oak: RGB = biome base, A = vertex light. Shader ใช้ texture alpha เอง
            let tint = if seasonal {
                [leaf_fol[0], leaf_fol[1], leaf_fol[2], t[0]]
            } else {
                [t[0], t[1], t[2], t[3]]
            };
            generate_leaf_mesh_into(
                &mut set, sprite, xi as f32, yi as f32, zi as f32,
                tint, seasonal, leaf == BlockType::MapleLeaves, seasonal_offset,
            );
        });
    }

    if let Some(bn) = branch_network {
        blocks.for_each_matching(
            |b| matches!(b, BlockType::Branch | BlockType::MapleBranch),
            |xi, yi, zi, branch| {
            let p = IVec3::new(chunk_pos.x * CHUNK_WIDTH as i32 + xi as i32, yi as i32, chunk_pos.y * CHUNK_WIDTH as i32 + zi as i32);
            let node = bn.nodes.get(&p);
            let thickness = node.map_or(crate::tree::LOOSE_THICKNESS, |n| n.thickness);
            let eff_thickness =
                effective_branch_thickness(p, thickness, breaking_target);

            // แต่ละทิศพก thickness ของ node ปลายทางมาด้วย — รอยต่อสองฝั่งจะได้คิด
            // รัศมีจากตัวเลขคู่เดียวกัน (ดู joint_radius) ผิวจึงต่อสนิทไม่เป็นขั้น
            let parent = node
                .and_then(|n| n.parent_pos)
                .map(|pp| {
                    (
                        pp - p,
                        bn.thickness_at(pp)
                            .map(|t| effective_branch_thickness(pp, t, breaking_target)),
                    )
                });
            let mut children = Vec::new();
            if let Some(n) = node {
                for &cp in &n.children {
                    children.push((
                        cp - p,
                        bn.thickness_at(cp)
                            .map(|t| effective_branch_thickness(cp, t, breaking_target)),
                    ));
                }
            }

            let tint = block_tint(xi as i32, yi as i32, zi as i32);
            generate_branch_mesh_with_type(
                &mut set, xi as f32, yi as f32, zi as f32, eff_thickness,
                parent, &children, tint, branch,
            );
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

fn effective_branch_thickness(
    pos: IVec3,
    original: u8,
    breaking_target: Option<(IVec3, f32)>,
) -> u8 {
    match breaking_target {
        Some((target, progress)) if pos == target => {
            (original as f32 * (1.0 - progress)).max(1.0) as u8
        }
        _ => original,
    }
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
    seasonal: bool,
    maple: bool,
    seasonal_offset: f32,
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
        if seasonal {
            let buf = if maple {
                &mut set.maple_foliage
            } else {
                &mut set.seasonal_foliage
            };
            let first = buf.positions.len();
            buf
                .push_quad(verts, [0., 1., 0.], [tint; 4], UVS, false);
            buf.uv_b[first..first + 4]
                .copy_from_slice(&[[seasonal_offset, 0.0]; 4]);
        } else {
            texture_buf(&mut set.deco, sprite)
                .push_quad(verts, [0., 1., 0.], [tint; 4], UVS, false);
        }
    }
}

/// ช่วงของตัวต่อกิ่งวัดตามแกนของมันเอง: จากใจกลาง node ถึงระนาบรอยต่อ (ครึ่งทาง
/// ไปหาเพื่อนบ้าน) — node สองตัวที่ติดกันจึงปูเต็มระยะห่างพอดีโดยไม่เหลือคอคอด
#[cfg(test)]
fn extension_span(dir: IVec3) -> (f32, f32) {
    (0.0, dir.as_vec3().length() * 0.5)
}

/// ให้ทุกแขนกินลึกย้อนเข้าไปในแกนกลางของ node เล็กน้อย พื้นที่ overlap นี้ทำหน้าที่
/// เป็น solid junction ที่ fork/bend และฝาด้านในจึงไม่โผล่เป็นรอยตัดตรงกลางกิ่ง
fn branch_extension_span(dir: IVec3, center_radius: f32) -> (f32, f32) {
    (-center_radius, dir.as_vec3().length() * 0.5)
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

#[cfg(test)]
fn generate_branch_mesh_into(
    set: &mut ChunkMeshSet,
    bx: f32,
    by: f32,
    bz: f32,
    thickness: u8,
    parent: Option<(IVec3, Option<u8>)>,
    children: &[(IVec3, Option<u8>)],
    tint: [f32; 4],
) {
    generate_branch_mesh_with_type(
        set,
        bx,
        by,
        bz,
        thickness,
        parent,
        children,
        tint,
        BlockType::Branch,
    );
}

fn generate_branch_mesh_with_type(
    set: &mut ChunkMeshSet,
    bx: f32,
    by: f32,
    bz: f32,
    thickness: u8,
    parent: Option<(IVec3, Option<u8>)>,
    children: &[(IVec3, Option<u8>)],
    // ความสว่างของช่องที่กิ่งอยู่ (sky light) — ไม่งั้นกิ่งสว่างเต็มแม้ในถ้ำ/กลางคืน
    tint: [f32; 4],
    branch: BlockType,
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

    let tex = face_texture(branch, 2, 0).unwrap_or("textures/oak_log_side.png");

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
        let (min_y, max_y) = branch_extension_span(dir, r_center);
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
    pub fbm: Fbm<Perlin>,
    temperature: Perlin,
    cave: Perlin,
    /// Shared vein mask plus a low-frequency selector replaces six independent
    /// 3D noise calls for every underground voxel.
    ore_field: Perlin,
    ore_kind: Perlin,
    humidity: Perlin,
    pub region: Perlin,
    /// domain warp (บิดพิกัดก่อน sample) → ชายฝั่ง/รอยต่อเป็นธรรมชาติ ไม่เป็นก้อนกลม
    warp_x: Perlin,
    warp_z: Perlin,
    /// ทวีป/มหาสมุทรสเกลใหญ่ (low-freq)
    continent: Perlin,
    continent_detail: Perlin,
    continent_warp_x: Perlin,
    continent_warp_z: Perlin,
    /// สนามความเป็นภูเขา (คุมว่าเทือกเขาอยู่แถบไหน)
    mountain: Perlin,
    /// ridged fbm สำหรับสันเขาแหลม
    ridge: Fbm<Perlin>,
    pub biomes: Arc<crate::biomegen::BiomeConfig>,
    params: crate::NoiseParams,
}

// ── ค่าจูนรูปทรง terrain (โครง; ยอด/ทะเลปรับได้ตรงนี้) ──
/// ระยะบิดพิกัด (บล็อก) + ความถี่ของ domain warp
const WARP_AMP: f64 = 45.0;
const WARP_FREQ: f64 = 0.004;
/// ความถี่ทวีป (ยิ่งต่ำ ทวีป/ทะเลยิ่งใหญ่) — ต่ำ = แผ่นดินใหญ่เป็นผืน ไม่แตกเป็นเกาะ
/// ความถี่สนามภูเขา + ความถี่สันเขา
const MOUNT_FIELD_FREQ: f64 = 0.0013;
const RIDGE_FREQ: f64 = 0.0035;
/// ความสูงสูงสุดที่เทือกเขาเพิ่มได้ (บล็อก) — ใช้ช่วงแนวตั้ง 512 ให้คุ้ม
const MOUNT_AMP: f64 = 155.0;
#[inline]
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Hermite smoothstep — 0 ต่ำกว่า edge0, 1 เหนือ edge1, ไล่นุ่มระหว่างกลาง
#[inline]
fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// map continentalness (−1..1) → offset ความสูง (บล็อก): ทะเลลึก → ที่ราบสูง.
/// ชายฝั่งแคบ+ชัน แล้วยกแผ่นดินขึ้น +22 → พื้นโผล่พ้นน้ำเป็นผืนตัน ไม่จมเป็นหย่อมเวลาโดน detail
#[inline]
fn continent_offset(c: f64) -> f64 {
    if c < -0.25 {
        lerp(-55.0, -12.0, (c + 1.0) / 0.75) // มหาสมุทร: ลึก → ไหล่ทวีป
    } else if c < 0.0 {
        lerp(-12.0, 28.0, (c + 0.25) / 0.25) // ชายฝั่งชัน: ข้าม 0 เร็ว โผล่พ้นน้ำเด็ดขาด
    } else {
        lerp(28.0, 82.0, c) // แผ่นดิน → ที่ราบสูง (พื้นตัน)
    }
}

/// biome ที่ column เดียว — Copy (ไม่มี String) เก็บใน array ต่อ chunk ได้เบา
#[derive(Clone, Copy)]
pub struct ColumnBiome {
    pub surface: BlockType,
    pub subsurface: BlockType,
    pub tree: crate::biomegen::TreeKind,
    pub tree_density: f32,
}

/// All expensive world-generation properties for one horizontal coordinate.
/// Chunk generation keeps these samples hot instead of independently repeating
/// terrain, volcanism, and hydrothermal queries for the same column.
#[derive(Clone, Copy)]
pub struct ColumnSample {
    pub height: i32,
    pub water_level: i32,
    pub biome: ColumnBiome,
    pub volcano: crate::volcanism::VolcanoSample,
    pub hydrothermal: crate::volcanism::HydrothermalSample,
}

impl TerrainSampler {
    pub fn new(params: crate::NoiseParams) -> Self {
        Self {
            // seed เดียวคุมทุกชั้น — ให้ biome/ถ้ำเปลี่ยนตาม seed ด้วย ไม่ใช่แค่ความสูง
            fbm: Fbm::<Perlin>::new(params.seed).set_octaves(params.octaves as usize),
            temperature: Perlin::new(params.seed.wrapping_add(1)),
            cave: Perlin::new(params.seed.wrapping_add(2)),
            ore_field: Perlin::new(params.seed.wrapping_add(20)),
            ore_kind: Perlin::new(params.seed.wrapping_add(21)),
            humidity: Perlin::new(params.seed.wrapping_add(3)),
            region: Perlin::new(params.seed.wrapping_add(4)),
            warp_x: Perlin::new(params.seed.wrapping_add(5)),
            warp_z: Perlin::new(params.seed.wrapping_add(6)),
            continent: Perlin::new(params.seed.wrapping_add(7)),
            continent_detail: Perlin::new(params.seed.wrapping_add(27)),
            continent_warp_x: Perlin::new(params.seed.wrapping_add(28)),
            continent_warp_z: Perlin::new(params.seed.wrapping_add(29)),
            mountain: Perlin::new(params.seed.wrapping_add(8)),
            ridge: Fbm::<Perlin>::new(params.seed.wrapping_add(9)).set_octaves(4),
            biomes: worldgen_biomes(),
            params,
        }
    }

    /// อุณหภูมิดิบ (−1..1) — ละติจูดภูมิอากาศ (แกน Z) + noise เฉพาะถิ่นเล็กน้อย. นิ่งตามฤดู
    /// (worldgen ต้อง deterministic) — ใช้จำแนกเขต biome
    pub fn temperature_raw(&self, wx: f64, wz: f64) -> f64 {
        let base_temp = crate::biome::temp_from_latitude(crate::biome::climate_lat(wz));
        // noise ความถี่ต่ำ (feature ~1,700 บล็อก) → รอยต่อเขตเป็นเส้นโค้งใหญ่ biome เป็นผืน
        // ต่อเนื่อง ไม่แตกเป็นหย่อมเล็กปน (เดิม 0.003 = ~330 บล็อก → taiga จุด ๆ กลาง plain)
        let noise = self.temperature.get([wx * 0.0006, wz * 0.0006]) * 0.15;
        (base_temp + noise).clamp(-1.0, 1.0)
    }

    /// ความชื้นดิบ (−1..1) — noise เฉพาะถิ่น ผสมกับแถบละติจูด (Hadley). ใช้ย้อมสีหญ้า/ใบ
    pub fn humidity_raw(&self, wx: f64, wz: f64) -> f64 {
        let noise = self.humidity.get([wx * 0.0025, wz * 0.0025]);
        let band = crate::biome::humidity_band(crate::biome::climate_lat(wz));
        (noise * 0.5 + band * 0.5).clamp(-1.0, 1.0)
    }

    /// บิดพิกัด (domain warp) ก่อน sample terrain/biome — ให้ชายฝั่ง/รอยต่อคดโค้งเป็นธรรมชาติ
    #[inline]
    fn warp(&self, wx: f64, wz: f64) -> (f64, f64) {
        let n = [wx * WARP_FREQ, wz * WARP_FREQ];
        (wx + WARP_AMP * self.warp_x.get(n), wz + WARP_AMP * self.warp_z.get(n))
    }

    /// offset ความสูงจากทวีป (บล็อก) ที่พิกัด warped แล้ว
    #[inline]
    fn continent_off(&self, px: f64, pz: f64) -> f64 {
        continent_offset(self.continental_at(px, pz).landness)
    }

    pub fn continental_sample(&self, wx: f64, wz: f64) -> ContinentalSample {
        let (px, pz) = self.warp(wx, wz);
        self.continental_at(px, pz)
    }

    fn continental_at(&self, px: f64, pz: f64) -> ContinentalSample {
        let scale = self.params.continent_scale.clamp(4_000.0, 80_000.0);
        let frequency = 1.0 / (scale * 0.75);
        let amplitude = scale * 0.18 * self.params.coast_roughness.clamp(0.0, 1.0);
        let qx = px + self.continent_warp_x.get([px * frequency, pz * frequency]) * amplitude;
        let qz = pz + self.continent_warp_z.get([px * frequency, pz * frequency]) * amplitude;
        let cx = (qx / scale).floor() as i64;
        let cz = (qz / scale).floor() as i64;
        let mut nearest = (f64::MAX, 0u64, 0.0f64);
        let mut second = (f64::MAX, 0u64, 0.0f64);
        let mut plate_samples = Vec::with_capacity(9);
        for dz in -1..=1 {
            for dx in -1..=1 {
                let gx = cx + dx;
                let gz = cz + dz;
                let id = world_hash(self.params.seed.wrapping_add(101), gx, gz);
                let sx = (gx as f64 + 0.15 + hash_unit(id) * 0.70) * scale;
                let sz = (gz as f64 + 0.15 + hash_unit(id.rotate_left(29)) * 0.70) * scale;
                let distance = (qx - sx).powi(2) + (qz - sz).powi(2);
                let plate = if hash_unit(id.rotate_left(47))
                    < self.params.land_ratio.clamp(0.1, 0.9)
                {
                    1.0
                } else {
                    -1.0
                };
                let candidate = (distance, id, plate);
                plate_samples.push((distance, plate));
                if distance < nearest.0 {
                    second = nearest;
                    nearest = candidate;
                } else if distance < second.0 {
                    second = candidate;
                }
            }
        }
        let d1 = nearest.0.sqrt();
        let d2 = second.0.sqrt();
        // A hard nearest-cell value jumps on Voronoi edges. Blending only the
        // nearest pair still jumps where the identity of the second cell changes
        // at a three-cell junction, so use a smooth weighted cellular field.
        let sigma = scale * 0.22;
        let weight_scale = 2.0 * sigma * sigma;
        let (plate_sum, weight_sum) =
            plate_samples
                .into_iter()
                .fold((0.0, 0.0), |(plate_sum, weight_sum), (distance, plate)| {
                    let weight = (-(distance - nearest.0) / weight_scale).exp();
                    (plate_sum + plate * weight, weight_sum + weight)
                });
        let plate = plate_sum / weight_sum.max(f64::EPSILON);
        let plate_boundary =
            (1.0 - ((d2 - d1) / (scale * 0.24)).clamp(0.0, 1.0)).powi(2);
        let macro_noise = self.continent.get([qx / scale, qz / scale]);
        let detail_noise =
            self.continent_detail.get([qx / (scale * 0.22), qz / (scale * 0.22)]);
        let bias = (self.params.land_ratio.clamp(0.1, 0.9) - 0.45) * 2.0;
        // ลดน้ำหนัก plate ลงเล็กน้อย + เพิ่ม detail noise ความถี่สูง → ขอบ land/ocean
        // แตกเป็นอ่าว/แหลม (fractal) แทนส่วนโค้งเซลล์เรียบ ๆ (ทวีปยังเกาะกลุ่มด้วย macro)
        let landness = (plate * 0.50
            + macro_noise * 0.34
            + detail_noise * 0.32 * self.params.coast_roughness.clamp(0.0, 1.0)
            + bias)
            .clamp(-1.0, 1.0);
        let ocean_depth = (-landness).clamp(0.0, 1.0);
        ContinentalSample {
            landness,
            ocean_depth,
            shelf: (1.0 - ocean_depth * 3.5).clamp(0.0, 1.0),
            plate_boundary,
            plate_id: nearest.1,
        }
    }

    /// ความเป็นภูเขา 0..1 (ก่อนคูณ land factor)
    #[inline]
    fn mount_field(&self, px: f64, pz: f64) -> f64 {
        let raw = self.mountain.get([px * MOUNT_FIELD_FREQ, pz * MOUNT_FIELD_FREQ]);
        // เอาเฉพาะยอดบนของสนามภูเขา → เทือกเขากระจุกเป็นหย่อม เหลือที่ราบเป็นส่วนใหญ่
        // (เดิม raw>0.15 ก็มีภูเขา = ครอบ ~40% ของแผ่นดิน → ขรุขระทั้งทวีป)
        let local = smoothstep(0.30, 0.62, raw);
        let tectonic = self.continental_at(px, pz).plate_boundary
            * self.params.tectonic_strength.clamp(0.0, 1.0);
        local.max(tectonic)
    }

    /// สันเขาแหลม 0..1 (ridged)
    #[inline]
    fn ridged(&self, px: f64, pz: f64) -> f64 {
        let r = self.ridge.get([px * RIDGE_FREQ, pz * RIDGE_FREQ]);
        let v = 1.0 - r.abs();
        v * v
    }

    /// ความสูงสำหรับ hydrology (แบ็คโบน + ภูเขา, ไม่มี fbm ละเอียด) — hydro.rs ใช้คำนวณโครงข่ายแม่น้ำ
    pub fn hydro_height(&self, wx: f64, wz: f64) -> f64 {
        let (px, pz) = self.warp(wx, wz);
        let cont = self.continent_off(px, pz);
        let (off, _amp) = crate::biomegen::terrain_at(
            &self.biomes,
            &self.region,
            |x, z| self.temperature_raw(x, z),
            px,
            pz,
        );
        let base = SEA_LEVEL as f64 + cont + off as f64;
        let land = ((cont + 8.0) / 28.0).clamp(0.0, 1.0);
        let terrain = base + self.mount_field(px, pz) * land * self.ridged(px, pz) * MOUNT_AMP;
        terrain + self.volcano_sample(wx, wz).elevation
    }

    /// sub-biome ที่ column นี้ (hard pick) — คุมพื้นผิว/พืช. beach/snow คิดตอนเติมบล็อก
    pub fn column_biome(&self, wx: f64, wz: f64) -> ColumnBiome {
        let (px, pz) = self.warp(wx, wz);
        let zone = crate::biome::zone_of(self.temperature_raw(px, pz));
        match crate::biomegen::select(&self.biomes, zone, &self.region, px, pz) {
            Some(b) => ColumnBiome {
                surface: b.surface(),
                subsurface: b.subsurface(),
                tree: b.tree,
                tree_density: b.tree_density,
            },
            None => ColumnBiome {
                surface: BlockType::Grass,
                subsurface: BlockType::Dirt,
                tree: crate::biomegen::TreeKind::None,
                tree_density: 0.0,
            },
        }
    }

    pub fn biome_name(&self, wx: f64, wz: f64) -> String {
        let (px, pz) = self.warp(wx, wz);
        let zone = crate::biome::zone_of(self.temperature_raw(px, pz));
        match crate::biomegen::select(&self.biomes, zone, &self.region, px, pz) {
            Some(b) => b.name.clone(),
            None => format!("{:?}", zone),
        }
    }

    /// ระดับ (บล็อกเหนือ sea) ที่ผิวเริ่มคลุมหิมะ (ยอดเขา) — จาก config
    pub fn snow_line(&self) -> i32 {
        self.biomes.snow_line
    }

    pub fn volcano_sample(&self, wx: f64, wz: f64) -> crate::volcanism::VolcanoSample {
        let (px, pz) = self.warp(wx, wz);
        let continental = self.continental_at(px, pz);
        if continental.landness < 0.08 {
            return crate::volcanism::VolcanoSample::default();
        }
        let sample = crate::volcanism::sample_volcano(self.params.seed, wx, wz);
        if let Some(descriptor) = sample.descriptor {
            let affinity = 0.20 + 0.80 * continental.plate_boundary
                * self.params.tectonic_strength.clamp(0.0, 1.0);
            if hash_unit(world_hash(
                self.params.seed.wrapping_add(303),
                descriptor.id as i64,
                0,
            )) > affinity
            {
                return crate::volcanism::VolcanoSample::default();
            }
        }
        sample
    }

    pub fn hydrothermal_sample(
        &self,
        wx: f64,
        wz: f64,
    ) -> crate::volcanism::HydrothermalSample {
        if self.volcano_sample(wx, wz).descriptor.is_none() {
            return crate::volcanism::HydrothermalSample::default();
        }
        crate::volcanism::sample_hydrothermal(
            self.params.seed,
            wx,
            wz,
            self.humidity_raw(wx, wz),
        )
    }

    /// Sample every column property used by block generation once. In
    /// particular, hydrothermal sampling reuses the volcano lookup instead of
    /// performing the same continental/volcano search a second time.
    pub fn column_sample(&self, wx: f64, wz: f64) -> ColumnSample {
        self.column_sample_with_river(wx, wz, crate::hydro::river_at(wx, wz))
    }

    /// Variant used by chunk generation after it has pinned a hydrology region.
    pub fn column_sample_with_river(
        &self,
        wx: f64,
        wz: f64,
        river: Option<crate::hydro::RiverPoint>,
    ) -> ColumnSample {
        let volcano = self.volcano_sample(wx, wz);
        let hydrothermal = if volcano.descriptor.is_none() {
            crate::volcanism::HydrothermalSample::default()
        } else {
            crate::volcanism::sample_hydrothermal(
                self.params.seed,
                wx,
                wz,
                self.humidity_raw(wx, wz),
            )
        };
        let (height, water_level) =
            self.column_with_features(wx, wz, volcano, hydrothermal, river);
        ColumnSample {
            height,
            water_level,
            biome: self.column_biome(wx, wz),
            volcano,
            hydrothermal,
        }
    }

    /// ความสูงผิว — หลายชั้น: domain warp → ทวีป(low-freq) + เนิน biome(fbm) + เทือกเขา ridged + carve แม่น้ำ
    pub fn height(&self, wx: f64, wz: f64) -> i32 {
        self.column(wx, wz).0
    }

    /// (ความสูงผิว, ระดับผิวน้ำ) ของคอลัมน์ — คำนวณ base ครั้งเดียว, carve แม่น้ำที่นี่
    pub fn column(&self, wx: f64, wz: f64) -> (i32, i32) {
        let (h, water) = self.column_raw(wx, wz);
        (h.clamp(3.0, (CHUNK_HEIGHT - 1) as f64) as i32, water)
    }

    fn column_with_features(
        &self,
        wx: f64,
        wz: f64,
        volcano: crate::volcanism::VolcanoSample,
        hydrothermal: crate::volcanism::HydrothermalSample,
        river: Option<crate::hydro::RiverPoint>,
    ) -> (i32, i32) {
        let (h, water) =
            self.column_raw_with_features(wx, wz, volcano, hydrothermal, river);
        (h.clamp(3.0, (CHUNK_HEIGHT - 1) as f64) as i32, water)
    }

    /// Keep river incision proportional to valley width. Without this cap, a
    /// coarse hydrology route can cut a narrow, nearly vertical wall through a
    /// mountain when its routed surface is far below the detailed terrain.
    #[inline]
    fn river_levels(terrain_h: f64, river: crate::hydro::RiverPoint) -> (f64, f64, f64) {
        const MIN_MAX_INCISION: f64 = 12.0;
        const MAX_SLOPE_PER_RADIUS: f64 = 1.25;
        const FULL_STRENGTH_INCISION: f64 = 8.0;
        const MAX_ROUTING_MISMATCH: f64 = 24.0;
        let max_incision =
            (river.valley_radius as f64 * MAX_SLOPE_PER_RADIUS).max(MIN_MAX_INCISION);
        let routed_surface = (river.surface as f64).min(terrain_h + 1.0);
        let requested_incision = (terrain_h - routed_surface).max(0.0);
        let terrain_fit =
            1.0 - smoothstep(FULL_STRENGTH_INCISION, MAX_ROUTING_MISMATCH, requested_incision);
        let local_surface = routed_surface.max(terrain_h - max_incision);
        (
            local_surface,
            local_surface - river.depth as f64,
            terrain_fit,
        )
    }

    fn column_raw(&self, wx: f64, wz: f64) -> (f64, i32) {
        let volcano = self.volcano_sample(wx, wz);
        let hydrothermal = if volcano.descriptor.is_none() {
            crate::volcanism::HydrothermalSample::default()
        } else {
            crate::volcanism::sample_hydrothermal(
                self.params.seed,
                wx,
                wz,
                self.humidity_raw(wx, wz),
            )
        };
        let river = crate::hydro::river_at(wx, wz);
        self.column_raw_with_features(wx, wz, volcano, hydrothermal, river)
    }

    fn column_raw_with_features(
        &self,
        wx: f64,
        wz: f64,
        volcano: crate::volcanism::VolcanoSample,
        hydrothermal: crate::volcanism::HydrothermalSample,
        river: Option<crate::hydro::RiverPoint>,
    ) -> (f64, i32) {
        let (px, pz) = self.warp(wx, wz);
        let cont = self.continent_off(px, pz);
        let (off, amp) = crate::biomegen::terrain_at(
            &self.biomes,
            &self.region,
            |x, z| self.temperature_raw(x, z),
            px,
            pz,
        );
        let base = SEA_LEVEL as f64 + cont + off as f64; // แบ็คโบน (= base_core)
        // ภูเขาโผล่เฉพาะบนแผ่นดิน (ไม่งอกกลางมหาสมุทร)
        let land = ((cont + 8.0) / 28.0).clamp(0.0, 1.0);
        let mount_f = self.mount_field(px, pz);
        // detail (เนินกลาง): เบาในที่ราบ แรงขึ้นในเขตภูเขา (roughness ตาม mount_f) และ
        // damp ใกล้ระดับน้ำ กันพื้นชายขอบจมเป็นบ่อจิ๋ว (base ต่ำ → detail แทบเป็นศูนย์)
        let rough = 0.35 + 0.65 * mount_f;
        let coast_damp = smoothstep(0.0, 14.0, base - SEA_LEVEL as f64);
        let detail = amp as f64
            * self.fbm.get([px * self.params.frequency, pz * self.params.frequency])
            * rough
            * coast_damp;
        let mut h = base + detail + mount_f * land * self.ridged(px, pz) * MOUNT_AMP;
        h += volcano.elevation;

        // แม่น้ำ: โครงข่ายจริงจาก hydro (flow accumulation) — carve หุบเขาก่อน แล้วค่อย carve แม่น้ำ
        let mut water = SEA_LEVEL as i32;
        if let Some(r) = river {
            // Hydrology owns the downhill profile, but never allow a coarse sample
            // to suspend water more than one block above the detailed local terrain.
            let (local_surface, bed, terrain_fit) = Self::river_levels(h, r);
            
            // 1. สร้างหุบเขา (Valley) เพื่อปรับระดับดินเดิม (h) ให้เข้าหาระดับผิวน้ำ (local_surface) อย่างนุ่มนวล
            // ป้องกันปัญหาน้ำลอยอยู่กลางอากาศ (Aqueduct) ถ้าระดับดินเดิมต่ำกว่าน้ำ (ยกเว้นอยู่ในทะเลอยู่แล้ว)
            let v = r.valley_mask as f64 * terrain_fit;
            let v_smooth = v * v * (3.0 - 2.0 * v);
            // Valleys may carve high terrain down, but must never raise low terrain
            // to meet a depression-filled routing surface.
            let valley_h = if h > local_surface {
                lerp(h, local_surface, v_smooth)
            } else {
                h
            };
            
            // 2. ขุดร่องแม่น้ำ (Channel) ลงไปหาระดับก้นแม่น้ำ (bed)
            let channel_mask = r.mask as f64 * terrain_fit;
            let channel_h = lerp(valley_h, bed, channel_mask);
            
            h = channel_h; // ใช้ค่าที่ปรับแล้ว (อาจจะสูงขึ้นหรือต่ำลงจาก h เดิม)
            
            // เติมน้ำเฉพาะจุดที่เป็นแม่น้ำจริงๆ (mask > 0)
            if channel_mask > 0.0 {
                water = water.max(local_surface.floor() as i32);
            }
        }
        if hydrothermal.pool > 0.0 {
            h -= hydrothermal.pool_depth as f64 * hydrothermal.pool.powf(0.7);
        }
        (h, water)
    }

    /// ทิศ+ความเร็วการไหลของแม่น้ำ (สำหรับ water wheel) — จากโครงข่าย hydro จริง; None ถ้าไม่ใช่แม่น้ำ
    pub fn river_flow(&self, wx: f64, wz: f64) -> Option<(Vec2, f32)> {
        crate::hydro::river_at(wx, wz).map(|r| (r.flow, r.speed))
    }

    /// ผิวเป็นทรายไหม (ทะเลทราย/ชายหาด) — ใช้ระบายสี LOD ระยะไกล
    pub fn surface_is_sand(&self, wx: f64, wz: f64) -> bool {
        self.column_biome(wx, wz).surface == BlockType::Sand
    }

    pub fn is_cave(&self, wx: f64, y: i32, wz: f64) -> bool {
        self.cave.get([wx * 0.06, y as f64 * 0.06, wz * 0.06]) > 0.45
    }
    
    /// Select at most one geology material with one common vein field and one
    /// mineral selector. Height bands retain the former availability ranges.
    #[inline]
    pub fn geology_block(&self, wx: f64, y: i32, wz: f64) -> Option<BlockType> {
        if !(2..100).contains(&y) {
            return None;
        }
        let p = [wx * 0.19, y as f64 * 0.19, wz * 0.19];
        if self.ore_field.get(p) <= 0.50 {
            return None;
        }
        let kind = self.ore_kind.get([wx * 0.055, y as f64 * 0.055, wz * 0.055]);
        let block = if y < 35 {
            if kind < -0.60 { BlockType::TinOre }
            else if kind < -0.25 { BlockType::ZincOre }
            else if kind < 0.05 { BlockType::IronOre }
            else if kind < 0.35 { BlockType::CopperOre }
            else if kind < 0.70 { BlockType::CoalOre }
            else { BlockType::Limestone }
        } else if y < 40 {
            if kind < -0.35 { BlockType::ZincOre }
            else if kind < 0.0 { BlockType::IronOre }
            else if kind < 0.35 { BlockType::CopperOre }
            else if kind < 0.70 { BlockType::CoalOre }
            else { BlockType::Limestone }
        } else if y < 60 {
            if kind < -0.20 { BlockType::ZincOre }
            else if kind < 0.25 { BlockType::CopperOre }
            else if kind < 0.70 { BlockType::CoalOre }
            else { BlockType::Limestone }
        } else if y < 80 {
            if kind < 0.05 { BlockType::CopperOre }
            else if kind < 0.65 { BlockType::CoalOre }
            else { BlockType::Limestone }
        } else if y < 90 {
            if kind < 0.45 { BlockType::CoalOre } else { BlockType::Limestone }
        } else {
            BlockType::CoalOre
        };
        Some(block)
    }
}

/// Finds stable inland terrain for a new player without assuming that world
/// origin is land. Candidates stay within 20 km and favor flat plate interiors.
pub fn safe_mainland_spawn(params: crate::NoiseParams) -> Vec3 {
    let sampler = TerrainSampler::new(params);
    let mut best = None::<(f64, f64, f64)>;
    let golden_angle = 2.399_963_229_728_653_f64;
    for i in 0..640 {
        let radius = 20_000.0 * ((i + 1) as f64 / 640.0).sqrt();
        let angle = i as f64 * golden_angle;
        let x = radius * angle.cos();
        let z = radius * angle.sin();
        let continental = sampler.continental_sample(x, z);
        if continental.landness < 0.22 || continental.plate_boundary > 0.55 {
            continue;
        }
        let h = sampler.hydro_height(x, z);
        let slope = [
            sampler.hydro_height(x + 10.0, z),
            sampler.hydro_height(x - 10.0, z),
            sampler.hydro_height(x, z + 10.0),
            sampler.hydro_height(x, z - 10.0),
        ]
        .into_iter()
        .map(|near| (near - h).abs())
        .fold(0.0, f64::max);
        if slope > 5.0 || sampler.volcano_sample(x, z).descriptor.is_some() {
            continue;
        }
        let score = continental.landness * 4.0 - slope * 0.35 - radius / 20_000.0;
        if best.is_none_or(|(old, _, _)| score > old) {
            best = Some((score, x, z));
        }
    }
    let (_, x, z) = best.unwrap_or((0.0, 0.0, 0.0));
    let y = sampler.hydro_height(x, z).max(SEA_LEVEL as f64 + 2.0) + 3.0;
    Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContinentalSample {
    pub landness: f64,
    pub ocean_depth: f64,
    pub shelf: f64,
    pub plate_boundary: f64,
    pub plate_id: u64,
}

#[inline]
fn world_hash(seed: u32, x: i64, z: i64) -> u64 {
    let mut v = seed as u64
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    v ^= v >> 30;
    v = v.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    v ^= v >> 27;
    v = v.wrapping_mul(0x94D0_49BB_1331_11EB);
    v ^ (v >> 31)
}

#[inline]
fn hash_unit(v: u64) -> f64 {
    (v >> 11) as f64 * (1.0 / ((1u64 << 53) as f64))
}

/// บล็อกผิวจริงตาม biome + ความสูง — ชายหาดใกล้น้ำเป็นทราย. **เลิกทำ elevation snow แล้ว**
/// (เดิมผิวเหนือ snow_line กลายเป็นหิมะทุกเขต — ผู้ใช้ไม่เอา) หิมะเหลือเฉพาะจาก biome หนาว
/// (Tundra/Snowy = `col.surface` เป็น Snow เอง)
pub fn surface_block_for(col: ColumnBiome, height: i32, sea_level: i32, _snow_line: i32) -> BlockType {
    if height <= sea_level + 1 {
        return BlockType::Sand; // ชายหาด/ก้นน้ำตื้น ทุก biome
    }
    col.surface
}

// ── ชายหาด ──
/// สูงเหนือน้ำไม่เกินนี้ (บล็อก) = อาจเป็นแถบหาด
const BEACH_MAX_ABOVE: i32 = 4;
/// ระยะเพื่อนบ้านที่วัดความชัน (บล็อก)
const BEACH_SLOPE_STEP: f64 = 3.0;
/// ต่างระดับถึงเพื่อนบ้าน <= นี้ = ลาดพอที่จะเป็นหาด (ชันกว่านี้=หน้าผา ไม่มีทราย)
const BEACH_MAX_SLOPE: i32 = 3;

/// บล็อกผิวชายฝั่ง — ชายฝั่ง **ลาด** ใกล้ระดับน้ำ → หาดกว้าง (วัสดุตามเขตอุณหภูมิ),
/// หน้าผา **ชัน** → ใช้ surface_block_for เดิม (หิน/หญ้า ไม่มีทราย). หาดกว้างเกิดเอง
/// เพราะบนพื้นลาด แถบความสูง [sea..sea+MAX_ABOVE] กินระยะแนวนอนมาก
pub fn coastal_surface(
    sampler: &TerrainSampler,
    wx: f64,
    wz: f64,
    col: ColumnBiome,
    height: i32,
    sea_level: i32,
    snow_line: i32,
) -> BlockType {
    if !(0..=BEACH_MAX_ABOVE).contains(&(height - sea_level)) {
        return surface_block_for(col, height, sea_level, snow_line);
    }
    let e = BEACH_SLOPE_STEP;
    let neighbors = [
        sampler.height(wx + e, wz),
        sampler.height(wx - e, wz),
        sampler.height(wx, wz + e),
        sampler.height(wx, wz - e),
    ];
    coastal_surface_from_neighbors(sampler, wx, wz, col, height, sea_level, snow_line, neighbors)
}

fn coastal_surface_from_neighbors(
    sampler: &TerrainSampler,
    wx: f64,
    wz: f64,
    col: ColumnBiome,
    height: i32,
    sea_level: i32,
    snow_line: i32,
    neighbors: [i32; 4],
) -> BlockType {
    let above = height - sea_level;
    if (0..=BEACH_MAX_ABOVE).contains(&above) {
        let max_diff = neighbors.iter()
        .map(|h| (h - height).abs())
        .max()
        .unwrap_or(0);
        if max_diff <= BEACH_MAX_SLOPE {
            return match crate::biome::zone_of(sampler.temperature_raw(wx, wz)) {
                crate::biome::ClimateZone::Hot | crate::biome::ClimateZone::Warm => BlockType::Sand,
                crate::biome::ClimateZone::Cool => BlockType::Gravel,
                crate::biome::ClimateZone::Cold => BlockType::Snow,
            };
        }
    }
    surface_block_for(col, height, sea_level, snow_line)
}

/// วัสดุก้นทะเล/ก้นน้ำ ตามความลึก (ตื้น=ทราย, กลาง=กรวด, ลึก=ดินเหนียว)
#[inline]
fn ocean_floor_block(depth: i32) -> BlockType {
    if depth <= 4 {
        BlockType::Sand
    } else if depth <= 12 {
        BlockType::Gravel
    } else {
        BlockType::Clay
    }
}

/// คืนบล็อกของ chunk + โครงกิ่งของต้นไม้ที่ปลูกไว้ (deterministic จากพิกัด chunk)
fn generate_chunk_blocks(
    chunk_pos: IVec2,
    noise: crate::NoiseParams,
) -> (ChunkBlocks, Vec<crate::tree::BranchRecord>) {
    let sampler = TerrainSampler::new(noise);
    // เริ่มเป็นอากาศ uniform ทั้งคอลัมน์ — เขียนเฉพาะที่มีของ ฟ้าไม่เคยถูก
    // materialize; compact() ท้ายฟังก์ชันยุบใต้ดิน/น้ำที่บังเอิญล้วนกลับเป็น 1 byte
    let mut blocks = ChunkBlocks::new_uniform(BlockType::Air);

    let base_x = chunk_pos.x as f64 * CHUNK_WIDTH as f64;
    let base_z = chunk_pos.y as f64 * CHUNK_WIDTH as f64;
    let sea_level: i32 = SEA_LEVEL as i32;
    let river_region = crate::hydro::region(
        base_x,
        base_z,
        base_x + CHUNK_WIDTH as f64 - 1.0,
        base_z + CHUNK_WIDTH as f64 - 1.0,
    );

    let mut heights = [[0i32; CHUNK_WIDTH]; CHUNK_WIDTH];
    let def_col = ColumnBiome {
        surface: BlockType::Grass,
        subsurface: BlockType::Dirt,
        tree: crate::biomegen::TreeKind::None,
        tree_density: 0.0,
    };
    let mut biomes = [[def_col; CHUNK_WIDTH]; CHUNK_WIDTH];
    let mut volcanoes = [[crate::volcanism::VolcanoSample::default(); CHUNK_WIDTH]; CHUNK_WIDTH];
    let mut hydrothermal = [[crate::volcanism::HydrothermalSample::default(); CHUNK_WIDTH]; CHUNK_WIDTH];
    // ระดับผิวน้ำต่อคอลัมน์ (sea ปกติ, ยกสูงเมื่อเป็นแม่น้ำบนที่สูง)
    let mut water_levels = [[sea_level; CHUNK_WIDTH]; CHUNK_WIDTH];
    let snow_line = sampler.snow_line();

    for z in 0..CHUNK_WIDTH {
        for x in 0..CHUNK_WIDTH {
            let wx = base_x + x as f64;
            let wz = base_z + z as f64;
            let river = river_region
                .as_ref()
                .and_then(|region| region.river_at(wx, wz));
            let sample = sampler.column_sample_with_river(wx, wz, river);
            heights[z][x] = sample.height;
            water_levels[z][x] = sample.water_level;
            biomes[z][x] = sample.biome;
            volcanoes[z][x] = sample.volcano;
            hydrothermal[z][x] = sample.hydrothermal;
        }
    }

    let mut height_cache: HashMap<(i32, i32), i32> = HashMap::with_capacity(CHUNK_WIDTH * CHUNK_WIDTH * 2);
    for z in 0..CHUNK_WIDTH {
        for x in 0..CHUNK_WIDTH {
            height_cache.insert((base_x as i32 + x as i32, base_z as i32 + z as i32), heights[z][x]);
        }
    }
    let mut cached_height = |x: i32, z: i32| -> i32 {
        *height_cache
            .entry((x, z))
            .or_insert_with(|| sampler.height(x as f64, z as f64))
    };

    for z in 0..CHUNK_WIDTH {
        for x in 0..CHUNK_WIDTH {
            let wx = base_x + x as f64;
            let wz = base_z + z as f64;
            let h = heights[z][x];
            let col = biomes[z][x];
            let water = water_levels[z][x];
            let volcano = volcanoes[z][x];
            let hydro = hydrothermal[z][x];
            let acid_level = h + hydro.pool_depth;
            
            // ก้นน้ำ (ทะเล/แม่น้ำ) = ทราย ไม่ใช่หญ้าใต้น้ำ; นอกนั้นตาม biome (ชายหาด/หิมะยอดเขา)
            let surface = if hydro.altered > 0.12 {
                if hydro.altered > 0.55
                    && sampler.region.get([wx * 0.13 + 19.0, wz * 0.13 - 7.0]) > 0.62
                {
                    BlockType::SulfurOre
                } else {
                    BlockType::AlteredRock
                }
            } else if volcano.cone > 0.0 {
                if volcano.crater > 0.45 {
                    BlockType::MagmaRock
                } else if volcano.cone < 0.48 {
                    BlockType::VolcanicAsh
                } else {
                    BlockType::Basalt
                }
            } else if h < water {
                ocean_floor_block(water - h) // ก้นทะเล: ตื้น=ทราย กลาง=กรวด ลึก=ดินเหนียว
            } else {
                if (0..=BEACH_MAX_ABOVE).contains(&(h - sea_level)) {
                    let sx = wx as i32;
                    let sz = wz as i32;
                    let step = BEACH_SLOPE_STEP as i32;
                    let neighbors = [
                        cached_height(sx + step, sz),
                        cached_height(sx - step, sz),
                        cached_height(sx, sz + step),
                        cached_height(sx, sz - step),
                    ];
                    coastal_surface_from_neighbors(
                        &sampler, wx, wz, col, h, sea_level, snow_line, neighbors,
                    )
                } else {
                    surface_block_for(col, h, sea_level, snow_line)
                }
            };
            // ทะเล/ผืนน้ำในเขตหนาว → ผิวน้ำเป็นน้ำแข็ง (sea ice)
            let frozen = h < water
                && crate::biome::zone_of(sampler.temperature_raw(wx, wz))
                    == crate::biome::ClimateZone::Cold;
            let subsurface = if volcano.cone > 0.0 {
                BlockType::Basalt
            } else if hydro.altered > 0.0 {
                BlockType::AlteredRock
            } else {
                col.subsurface
            };

            // Common terrain has no volcanic structures or acid pools. Build it
            // as vertical ranges, then overlay the comparatively small ore and
            // cave bands instead of branching over every y up to the surface.
            if volcano.descriptor.is_none() && hydro.pool <= 0.05 {
                if h >= 4 {
                    blocks.fill_column_range(x, z, 0, (h - 4) as usize, BlockType::Stone);
                    let ore_end = (h - 4).min(99);
                    if ore_end >= 2 {
                        for yi in 2..=ore_end {
                            if let Some(ore) = sampler.geology_block(wx, yi, wz) {
                                blocks.set(x, yi as usize, z, ore);
                            }
                        }
                    }
                }
                if h >= 3 {
                    blocks.fill_column_range(x, z, (h - 3) as usize, (h - 1) as usize, subsurface);
                }
                blocks.set(x, h as usize, z, surface);

                let cave_start = (h - CAVE_DEPTH).max(2) + 1;
                let cave_end = h - 5;
                if cave_start <= cave_end {
                    for yi in cave_start..=cave_end {
                        if sampler.is_cave(wx, yi, wz) {
                            blocks.set(x, yi as usize, z, BlockType::Air);
                        }
                    }
                }

                if water > h {
                    blocks.fill_column_range(
                        x,
                        z,
                        (h + 1) as usize,
                        water as usize,
                        BlockType::Water,
                    );
                    if frozen {
                        blocks.set(x, water as usize, z, BlockType::Ice);
                    }
                }
                continue;
            }

            let (volcano_distance, volcano_base, chamber_y) =
                if let Some(v) = volcano.descriptor {
                    let dx = wx - v.center.x;
                    let dz = wz - v.center.y;
                    let base_h = h - volcano.elevation.round() as i32;
                    ((dx * dx + dz * dz).sqrt(), base_h, base_h - 22)
                } else {
                    (f64::INFINITY, h, h - 22)
                };

            for y in 0..CHUNK_HEIGHT {
                let yi = y as i32;
                let chamber_distance_sq = if let Some(v) = volcano.descriptor {
                    let dx = wx - v.center.x;
                    let dz = wz - v.center.y;
                    let dy = (yi - chamber_y) as f64;
                    let chamber_radius = (v.radius * 0.09).clamp(12.0, 20.0);
                    (dx * dx + dz * dz + dy * dy, chamber_radius * chamber_radius)
                } else {
                    (f64::INFINITY, 0.0)
                };
                let in_conduit = volcano.descriptor.is_some()
                    && volcano_distance <= 4.0
                    && yi >= chamber_y
                    && yi <= h;
                let in_chamber = chamber_distance_sq.0 < chamber_distance_sq.1;
                let chamber_shell = chamber_distance_sq.0 < chamber_distance_sq.1 * 1.35;

                let block = if in_conduit || in_chamber {
                    BlockType::LavaSource
                } else if chamber_shell {
                    BlockType::MagmaRock
                } else if yi < h - 3 {
                    if volcano.cone > 0.0 && yi > volcano_base - 6 {
                        BlockType::Basalt
                    } else {
                        sampler.geology_block(wx, yi, wz).unwrap_or(BlockType::Stone)
                    }
                } else if yi < h {
                    subsurface
                } else if yi == h {
                    surface
                } else if hydro.pool > 0.05 && yi <= acid_level {
                    if hydro.pool > 0.82 && yi == h + 1 {
                        BlockType::SulfuricAcidSource
                    } else {
                        BlockType::Acid8
                    }
                } else if volcano.crater > 0.72 && yi == h + 1 {
                    BlockType::LavaSource
                } else if yi <= water {
                    if frozen && yi == water {
                        BlockType::Ice // แผ่นน้ำแข็งคลุมผิว (ใต้ลงไปยังเป็นน้ำ)
                    } else {
                        BlockType::Water
                    }
                } else {
                    break; // เหนือนี้เป็นอากาศทั้งหมด
                };

                // ถ้ำ: เจาะเฉพาะแถบใต้ผิวตื้นๆ (CAVE_DEPTH บล็อก) — is_cave เป็น Perlin 3 มิติ
                // ต้นทุนหลักของ gen; เจาะลึกกว่านี้ไม่มีใครเห็น แต่กิน CPU/VRAM มหาศาล
                if block.is_solid()
                    && yi < h - 4
                    && yi > (h - CAVE_DEPTH).max(2)
                    && !chamber_shell
                    && sampler.is_cave(wx, yi, wz)
                {
                    continue;
                }
                blocks.set(x, y, z, block);
            }
        }
    }

    // Decoration ใช้ world coordinate และจำลอง owner chunks รอบข้าง จึงปล่อยกิ่ง/ใบ
    // ข้ามรอยต่อได้ แต่แต่ละ target chunk จะรับเฉพาะ block/node ที่อยู่ในตัวเอง
    let branches = decorate_trees_for_chunk(chunk_pos, noise, &mut blocks);

    // หญ้าใช้ stream แยกจากต้นไม้ เพื่อให้จำนวน random calls ของทรงต้นไม้ไม่กระทบพื้น
    let mut state: u64 = tree_owner_seed(chunk_pos) ^ 0xD1B5_4A32_D192_ED03;
    let mut next = move || xorshift_next(&mut state);

    // หญ้าสูง: โปรยบนผิวหญ้า (เช็ค == Grass ด้านล่างกันไม่ให้ขึ้นบนทราย/หิมะอยู่แล้ว)
    let tuft_count = (next() % 14) as usize;
    for _ in 0..tuft_count {
        let gx = (next() % CHUNK_WIDTH as u64) as usize;
        let gz = (next() % CHUNK_WIDTH as u64) as usize;
        let h = heights[gz][gx];
        if h <= sea_level + 1 || h + 1 >= CHUNK_HEIGHT as i32 {
            continue;
        }
        if blocks.get(gx, h as usize, gz) == BlockType::Grass
            && blocks.get(gx, (h + 1) as usize, gz) == BlockType::Air
        {
            blocks.set(gx, (h + 1) as usize, gz, BlockType::TallGrass);
        }
    }

    blocks.compact();

    (blocks, branches)
}

const TREE_OWNER_RADIUS: i32 = 2;
const TREE_MAX_HORIZONTAL_REACH: i32 = 20;
const TREE_MIN_SPACING: i32 = 6;

fn tree_owner_seed(chunk_pos: IVec2) -> u64 {
    (chunk_pos.x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (chunk_pos.y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ 0x5851_F42D_4C95_7F2D
}

fn xorshift_next(state: &mut u64) -> u64 {
    if *state == 0 {
        *state = 0xA076_1D64_78BD_642F;
    }
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn tree_candidate_priority(owner: IVec2, tree_index: usize) -> u64 {
    let mut value =
        tree_owner_seed(owner) ^ (tree_index as u64 + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn horizontal_distance_to_chunk(p: IVec3, chunk_pos: IVec2) -> IVec2 {
    let min_x = chunk_pos.x * CHUNK_WIDTH as i32;
    let max_x = min_x + CHUNK_WIDTH as i32 - 1;
    let min_z = chunk_pos.y * CHUNK_WIDTH as i32;
    let max_z = min_z + CHUNK_WIDTH as i32 - 1;
    let dx = if p.x < min_x {
        min_x - p.x
    } else if p.x > max_x {
        p.x - max_x
    } else {
        0
    };
    let dz = if p.z < min_z {
        min_z - p.z
    } else if p.z > max_z {
        p.z - max_z
    } else {
        0
    };
    IVec2::new(dx, dz)
}

/// สร้างต้นไม้จาก owner chunk รอบเป้าหมายใน world space แล้ว clip ผลลง target chunk
/// แต่ละ candidate มี PRNG ของตัวเอง จึงได้รูปทรงเดิมไม่ว่า chunk ไหนจะเป็นผู้ขอ decoration
fn decorate_trees_for_chunk(
    target: IVec2,
    noise: crate::NoiseParams,
    target_blocks: &mut ChunkBlocks,
) -> Vec<crate::tree::BranchRecord> {
    let sampler = TerrainSampler::new(noise);
    let sea_level = SEA_LEVEL as i32;
    let snow_line = sampler.snow_line();
    let mut tree_blocks = WorldTreeBlocks::default();
    let mut records_by_pos: HashMap<IVec3, crate::tree::BranchRecord> = HashMap::new();
    let mut sample_cache: HashMap<(i32, i32), ColumnSample> = HashMap::new();
    // Candidate จะผ่านเมื่อมี priority สูงสุดในรัศมีขั้นต่ำของมันเอง กฎนี้ไม่ขึ้นกับ
    // ลำดับการ generate/load chunk และกันทั้งฐานซ้ำกับต้นที่ขึ้นชิดจนลำต้นซ้อนกัน
    let spacing_owner_radius = TREE_OWNER_RADIUS + 1;
    let mut spacing_candidates: Vec<(IVec3, u64)> = Vec::new();
    for owner_z in (target.y - spacing_owner_radius)..=(target.y + spacing_owner_radius) {
        for owner_x in (target.x - spacing_owner_radius)..=(target.x + spacing_owner_radius) {
            let owner = IVec2::new(owner_x, owner_z);
            let owner_seed = tree_owner_seed(owner);
            let mut count_state = owner_seed;
            let tree_count = (xorshift_next(&mut count_state) % 7) as usize;
            for tree_index in 0..tree_count {
                let mut state = owner_seed
                    ^ (tree_index as u64 + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93);
                let local_x = (xorshift_next(&mut state) % CHUNK_WIDTH as u64) as i32;
                let local_z = (xorshift_next(&mut state) % CHUNK_WIDTH as u64) as i32;
                let wx = owner.x * CHUNK_WIDTH as i32 + local_x;
                let wz = owner.y * CHUNK_WIDTH as i32 + local_z;
                let sample = *sample_cache
                    .entry((wx, wz))
                    .or_insert_with(|| sampler.column_sample(wx as f64, wz as f64));
                let (height, water_level) = (sample.height, sample.water_level);
                let col = sample.biome;
                let volcanic = sample.volcano.cone > 0.0
                    || sample.hydrothermal.altered > 0.08;
                let roll = (xorshift_next(&mut state) % 1000) as f32 / 1000.0;
                let surface = surface_block_for(col, height, sea_level, snow_line);
                let params = match col.tree {
                    crate::biomegen::TreeKind::Maple => &TREE_PRESETS[4].1,
                    _ => &TREE_PRESETS[ACTIVE_TREE_PRESET].1,
                };
                let headroom = match col.tree {
                    crate::biomegen::TreeKind::Spruce => 13,
                    _ => params.trunk_len.1 + params.limb_len.1 + 4,
                };
                if !volcanic
                    && col.tree != crate::biomegen::TreeKind::None
                    && roll < col.tree_density
                    && surface != BlockType::Snow
                    && height > water_level + 1
                    && height + headroom < CHUNK_HEIGHT as i32
                {
                    spacing_candidates.push((
                        IVec3::new(wx, height + 1, wz),
                        tree_candidate_priority(owner, tree_index),
                    ));
                }
            }
        }
    }

    for owner_z in (target.y - TREE_OWNER_RADIUS)..=(target.y + TREE_OWNER_RADIUS) {
        for owner_x in (target.x - TREE_OWNER_RADIUS)..=(target.x + TREE_OWNER_RADIUS) {
            let owner = IVec2::new(owner_x, owner_z);
            let owner_seed = tree_owner_seed(owner);
            let mut count_state = owner_seed;
            let tree_count = (xorshift_next(&mut count_state) % 7) as usize;
            for tree_index in 0..tree_count {
                let mut state = owner_seed
                    ^ (tree_index as u64 + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93);
                let mut next = || xorshift_next(&mut state);
                let local_x = (next() % CHUNK_WIDTH as u64) as i32;
                let local_z = (next() % CHUNK_WIDTH as u64) as i32;
                let wx = owner.x * CHUNK_WIDTH as i32 + local_x;
                let wz = owner.y * CHUNK_WIDTH as i32 + local_z;
                let sample = *sample_cache
                    .entry((wx, wz))
                    .or_insert_with(|| sampler.column_sample(wx as f64, wz as f64));
                let (height, water_level) = (sample.height, sample.water_level);
                let col = sample.biome;
                let volcanic = sample.volcano.cone > 0.0
                    || sample.hydrothermal.altered > 0.08;
                let base = IVec3::new(wx, height + 1, wz);
                let distance = horizontal_distance_to_chunk(base, target);
                if distance.x > TREE_MAX_HORIZONTAL_REACH
                    || distance.y > TREE_MAX_HORIZONTAL_REACH
                {
                    continue;
                }

                let roll = (next() % 1000) as f32 / 1000.0;
                let surface = surface_block_for(col, height, sea_level, snow_line);
                let params = match col.tree {
                    crate::biomegen::TreeKind::Maple => &TREE_PRESETS[4].1,
                    _ => &TREE_PRESETS[ACTIVE_TREE_PRESET].1,
                };
                let headroom = match col.tree {
                    crate::biomegen::TreeKind::Spruce => 13,
                    _ => params.trunk_len.1 + params.limb_len.1 + 4,
                };
                if volcanic
                    || col.tree == crate::biomegen::TreeKind::None
                    || roll >= col.tree_density
                    || surface == BlockType::Snow
                    || height <= water_level + 1
                    || height + headroom >= CHUNK_HEIGHT as i32
                {
                    continue;
                }
                let priority = tree_candidate_priority(owner, tree_index);
                let spacing_sq = TREE_MIN_SPACING * TREE_MIN_SPACING;
                if spacing_candidates.iter().any(|(other, other_priority)| {
                    *other_priority < priority
                        && (other.x - base.x).pow(2) + (other.z - base.z).pow(2) < spacing_sq
                }) {
                    continue;
                }

                let mut candidate_blocks = WorldTreeBlocks::default();
                let mut candidate_records = Vec::new();
                match col.tree {
                    crate::biomegen::TreeKind::Spruce => {
                        grow_spruce(&mut candidate_blocks, base, &mut next);
                    }
                    crate::biomegen::TreeKind::Maple => {
                        grow_tree(
                            &mut candidate_blocks,
                            &mut candidate_records,
                            base,
                            &TREE_PRESETS[4].1,
                            BlockType::MapleBranch,
                            BlockType::MapleLeaves,
                            &mut next,
                        );
                    }
                    crate::biomegen::TreeKind::Broadleaf => {
                        let maple = next() % 100 < 30;
                        grow_tree(
                            &mut candidate_blocks,
                            &mut candidate_records,
                            base,
                            if maple { &TREE_PRESETS[4].1 } else { params },
                            if maple {
                                BlockType::MapleBranch
                            } else {
                                BlockType::Branch
                            },
                            if maple {
                                BlockType::MapleLeaves
                            } else {
                                BlockType::Leaves
                            },
                            &mut next,
                        );
                    }
                    crate::biomegen::TreeKind::None => {}
                }
                let mut accepted_positions = HashSet::new();
                for (p, block) in candidate_blocks.blocks {
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        tree_blocks.blocks.entry(p)
                    {
                        entry.insert(block);
                        accepted_positions.insert(p);
                    }
                }
                for record in candidate_records {
                    let pos = IVec3::from_array(record.pos);
                    if accepted_positions.contains(&pos) {
                        records_by_pos.entry(pos).or_insert(record);
                    }
                }
            }
        }
    }

    let origin_x = target.x * CHUNK_WIDTH as i32;
    let origin_z = target.y * CHUNK_WIDTH as i32;
    for (p, block) in tree_blocks.blocks {
        if crate::tree::chunk_of(p, CHUNK_WIDTH as i32) != target {
            continue;
        }
        let local = IVec3::new(p.x - origin_x, p.y, p.z - origin_z);
        if matches!(
            block,
            BlockType::Leaves | BlockType::MapleLeaves | BlockType::SpruceLeaves
        ) && target_blocks.get(local.x as usize, local.y as usize, local.z as usize)
            != BlockType::Air
        {
            continue;
        }
        target_blocks.set(local.x as usize, local.y as usize, local.z as usize, block);
    }

    let mut records: Vec<_> = records_by_pos
        .into_values()
        .filter(|record| {
            crate::tree::chunk_of(IVec3::from_array(record.pos), CHUNK_WIDTH as i32) == target
        })
        .collect();
    records.sort_unstable_by_key(|record| record.pos);
    records
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

trait TreeBlockBuffer {
    fn contains(&self, p: IVec3) -> bool;
    fn block_at(&self, p: IVec3) -> BlockType;
    fn set_block(&mut self, p: IVec3, block: BlockType);
}

impl TreeBlockBuffer for ChunkBlocks {
    fn contains(&self, p: IVec3) -> bool {
        inside_chunk(p)
    }

    fn block_at(&self, p: IVec3) -> BlockType {
        self.get(p.x as usize, p.y as usize, p.z as usize)
    }

    fn set_block(&mut self, p: IVec3, block: BlockType) {
        self.set(p.x as usize, p.y as usize, p.z as usize, block);
    }
}

#[derive(Default)]
struct WorldTreeBlocks {
    blocks: HashMap<IVec3, BlockType>,
}

impl TreeBlockBuffer for WorldTreeBlocks {
    fn contains(&self, p: IVec3) -> bool {
        p.y >= 0 && p.y < CHUNK_HEIGHT as i32
    }

    fn block_at(&self, p: IVec3) -> BlockType {
        self.blocks.get(&p).copied().unwrap_or(BlockType::Air)
    }

    fn set_block(&mut self, p: IVec3, block: BlockType) {
        if self.contains(p) {
            self.blocks.insert(p, block);
        }
    }
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
    // Maple: taller than oak with a dense, rounded crown.
    ("maple", TreeParams {
        trunk_len: (5, 7), max_depth: 2, crown_forks: (3, 4),
        side_branch_chance: 0.42, bare_trunk: 3, limb_len: (3, 5),
        tilt: 0.68, wobble: 0.45, climb: 0.32, taper: 0.8, fork_drop: 0.58,
        leaf_tip: 2, leaf_fork: 2,
    }),
];

/// ทรงที่ worldgen ใช้อยู่ — เปลี่ยน index นี้เพื่อสลับทรงทั้งโลก
const ACTIVE_TREE_PRESET: usize = 0;

/// ปลูกต้นไม้ทั้งต้นที่ `base` — เขียนบล็อก Branch/Leaves ลง `blocks`
/// และสะสมโครง node ลง `records` (parent มาก่อนลูกเสมอ)
fn grow_tree(
    blocks: &mut impl TreeBlockBuffer,
    records: &mut Vec<crate::tree::BranchRecord>,
    base: IVec3,
    params: &TreeParams,
    branch: BlockType,
    leaves: BlockType,
    next: &mut impl FnMut() -> u64,
) {
    if !blocks.contains(base) {
        return;
    }
    blocks.set_block(base, branch);
    records.push(crate::tree::BranchRecord {
        pos: base.to_array(),
        parent: None,
        thickness: crate::tree::TRUNK_THICKNESS,
    });

    let trunk_len = pick_range(params.trunk_len, next);
    grow_limb(
        blocks, records, base, crate::tree::TRUNK_THICKNESS,
        Vec3::Y, trunk_len, 0, params, branch, leaves, next,
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
    blocks: &mut impl TreeBlockBuffer,
    records: &mut Vec<crate::tree::BranchRecord>,
    from: IVec3,
    from_thickness: u8,
    dir: Vec3,
    len: i32,
    depth: u32,
    params: &TreeParams,
    branch: BlockType,
    leaves: BlockType,
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
        if !blocks.contains(np) {
            break;
        }
        // ชนกิ่งที่มีอยู่แล้ว (กิ่งพี่น้อง/ต้นข้างๆ) — หยุดเส้นนี้ ไม่งั้นจะได้ record
        // สองใบที่ตำแหน่งเดียวกันแล้ว topology พัง
        if matches!(
            blocks.block_at(np),
            BlockType::Branch | BlockType::MapleBranch
        ) {
            break;
        }
        blocks.set_block(np, branch);
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
            spread(yaw, tilt), pick_range(child_range, next), depth + 1, params,
            branch, leaves, next,
        );
        scatter_leaves(blocks, at, params.leaf_fork, leaves);
    }

    if depth >= params.max_depth {
        scatter_leaves(blocks, cur, params.leaf_tip, leaves);
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
            spread(yaw, tilt), pick_range(child_range, next), depth + 1, params,
            branch, leaves, next,
        );
    }
    scatter_leaves(blocks, cur, params.leaf_fork, leaves);
}

/// โปรยใบรอบจุดหนึ่ง (ไม่ทับบล็อกที่มีของอยู่แล้ว และไม่ล้ำออกนอก chunk)
fn scatter_leaves(
    blocks: &mut impl TreeBlockBuffer,
    center: IVec3,
    r: i32,
    leaves: BlockType,
) {
    for dy in -r..=r {
        for dz in -r..=r {
            for dx in -r..=r {
                // ตัดมุมให้พุ่มกลมขึ้น ไม่เป็นกล่อง
                if dx.abs() + dy.abs() + dz.abs() > r + 1 {
                    continue;
                }
                let p = center + IVec3::new(dx, dy, dz);
                if !blocks.contains(p) {
                    continue;
                }
                if blocks.block_at(p) == BlockType::Air {
                    blocks.set_block(p, leaves);
                }
            }
        }
    }
}

/// ปลูกต้นสนแบบคิวบ์ (Minecraft classic) ที่ `base` — ลำต้น SpruceLog ตรง +
/// ใบ SpruceLeaves เป็นชั้นวงตัดมุมไล่ขึ้นเป็นทรงกรวย + ยอดแหลม
/// ไม่เข้า BranchNetwork (เป็นบล็อกธรรมดาที่เซฟลง chunk เอง)
fn grow_spruce(blocks: &mut impl TreeBlockBuffer, base: IVec3, next: &mut impl FnMut() -> u64) {
    if !blocks.contains(base) {
        return;
    }
    let trunk_h = 7 + (next() % 5) as i32; // 7..=11
    let top = base.y + trunk_h;

    // ใบ: เริ่มจากใต้ยอดลงมา เป็นชั้นๆ รัศมีสลับ 0,1,1,2,2,1,1,2,2,... ให้เป็นวงกรวยหยัก
    // (leaf_bottom อยู่เหนือโคน ~2 บล็อก ให้เห็นลำต้นเปลือย)
    let leaf_bottom = base.y + 3;
    for y in leaf_bottom..=top {
        // ระยะจากยอดลงมา — ยอดสุดรัศมี 0 ค่อยกว้างลงล่าง แต่หยักเป็นชั้น
        let from_top = top - y;
        let r = match from_top {
            0 => 0,
            n if n % 2 == 1 => 1,
            _ => 2,
        };
        place_leaf_disk(blocks, IVec3::new(base.x, y, base.z), r);
    }
    // ยอดแหลมเดี่ยวเหนือชั้นบนสุด
    let tip = IVec3::new(base.x, top + 1, base.z);
    if blocks.contains(tip) && blocks.block_at(tip) == BlockType::Air {
        blocks.set_block(tip, BlockType::SpruceLeaves);
    }

    // ลำต้น: ทับใบด้วย log (วางหลังใบเพื่อให้ log ชนะตรงแกนกลาง)
    for i in 0..trunk_h {
        let p = IVec3::new(base.x, base.y + i, base.z);
        if blocks.contains(p) {
            blocks.set_block(p, BlockType::SpruceLog);
        }
    }
}

/// วางแผ่นใบสนวงกลม (สี่เหลี่ยมตัดมุม) รัศมี r ที่ระดับ y เดียว — ไม่ทับของเดิม/ไม่ล้ำ chunk
fn place_leaf_disk(blocks: &mut impl TreeBlockBuffer, center: IVec3, r: i32) {
    for dz in -r..=r {
        for dx in -r..=r {
            // ตัดมุมเมื่อ r>=2 ให้วงกลมขึ้น (r<=1 เก็บครบ)
            if r >= 2 && dx.abs() == r && dz.abs() == r {
                continue;
            }
            let p = center + IVec3::new(dx, 0, dz);
            if !blocks.contains(p) {
                continue;
            }
            if blocks.block_at(p) == BlockType::Air {
                blocks.set_block(p, BlockType::SpruceLeaves);
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

    let mut drop_cache: HashMap<(i32, i32, i32), (f32, f32, f32)> =
        HashMap::with_capacity(256);
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

        let _face_uv = move |p: [f32; 3]| -> [f32; 2] {
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
                    if !block.is_fluid() {
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
                    if n.is_fluid() {
                        continue;
                    }
                    
                    // Cull internal side faces when adjacent to a downward ramp
                    if a != 1 && n == BlockType::Air {
                        if sample(c[0] + norm[0], c[1] - 1, c[2] + norm[2]).is_fluid() {
                            continue;
                        }
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
                        let (d, dep, _) = *drop_cache.entry((cx, vy, cz)).or_insert_with(|| {
                            water_corner_info(&sample, cx, vy, cz)
                        });
                        corner_drop[i] = d;
                        corner_depth[i] = dep;
                    }

                    let mut verts = [[0f32; 3]; 4];
                    let mut cols = [[0f32; 4]; 4];
                    let mut uvs = [[0f32; 2]; 4];
                    let mut water_data = [[0f32; 2]; 4];
                    let (flow_vec, speed) = crate::hydro::river_at((world_base_x + vx) as f64 + 0.5, (world_base_z + vz) as f64 + 0.5)
                        .map(|r| (r.flow, r.speed))
                        .unwrap_or((Vec2::ZERO, 0.0));
                    let mut water_flow_uv = [flow_vec.x * speed, flow_vec.y * speed];
                    if face_id == 0 {
                        let slope_x = (corner_drop[1] + corner_drop[2] - corner_drop[0] - corner_drop[3]) * 0.5;
                        let slope_z = (corner_drop[0] + corner_drop[1] - corner_drop[2] - corner_drop[3]) * 0.5;
                        water_flow_uv[0] += slope_x * 2.0;
                        water_flow_uv[1] += slope_z * 2.0;
                    }
                    for i in 0..4 {
                        let p = CUBE_POSITIONS[face_id][i];
                        verts[i] = [p[0] + vx as f32, p[1] + vy as f32, p[2] + vz as f32];
                        if p[1] > 0.5 { verts[i][1] -= corner_drop[i]; }
                        let br = shade * AO_CURVE[ao[i] as usize];
                        let tint = 1.0 - WATER_DEPTH_DARKEN * corner_depth[i];
                        // create_water_mesh วาดเฉพาะน้ำ → alpha ที่ vertex เสมอ (ดู WATER_ALPHA)
                        cols[i] = [base[0] * br * tint, base[1] * br * tint, base[2] * br * tint, WATER_ALPHA];
                        uvs[i] = water_flow_uv;
                        let depth = corner_depth[i];
                        let p = CUBE_POSITIONS[face_id][i];
                        let (_, _, shoreline) = *drop_cache
                            .get(&(vx + p[0] as i32, vy, vz + p[2] as i32))
                            .expect("water corner was cached above");
                        water_data[i] = [
                            depth,
                            if block.is_lava() {
                                -1.0 - shoreline
                            } else {
                                shoreline
                            },
                        ];
                    }
                    let flip = (ao[0] as u32 + ao[2] as u32) < (ao[1] as u32 + ao[3] as u32);
                    buf.push_water_quad(
                        verts,
                        CUBE_NORMALS[face_id],
                        cols,
                        uvs,
                        water_data,
                        flip,
                    );
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

/// เส้นทาง dev mode (Quick Start): ใช้โฟลเดอร์ `saves/` ที่ root
pub fn set_legacy_save_dir() {
    set_active_save_dir(Some(project_root().join("saves")));
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
pub struct WireFurnaceData {
    pub slots: [Option<crate::item::WireItemStack>; 2],
    pub current_temp: f32,
    pub active_fuel_energy: f32,
    pub active_fuel_base_temp: f32,
    pub air_multiplier: f32,
    pub air_boost_time: f32,
    pub smelting_progress: f32,
}

impl WireFurnaceData {
    pub fn from_data(data: &FurnaceData) -> Self {
        let mut slots = [None; 2];
        for (w, s) in slots.iter_mut().zip(data.slots.iter()) {
            *w = s.map(crate::item::WireItemStack::from_stack);
        }
        Self {
            slots,
            current_temp: data.current_temp,
            active_fuel_energy: data.active_fuel_energy,
            active_fuel_base_temp: data.active_fuel_base_temp,
            air_multiplier: data.air_multiplier,
            air_boost_time: data.air_boost_time,
            smelting_progress: data.smelting_progress,
        }
    }

    pub fn to_data(self) -> FurnaceData {
        let mut slots = [None; 2];
        for (w, s) in slots.iter_mut().zip(self.slots.iter()) {
            *w = s.and_then(|w| w.to_stack());
        }
        FurnaceData {
            slots,
            current_temp: self.current_temp,
            active_fuel_energy: self.active_fuel_energy,
            active_fuel_base_temp: self.active_fuel_base_temp,
            air_multiplier: self.air_multiplier,
            air_boost_time: self.air_boost_time,
            smelting_progress: self.smelting_progress,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct ChunkAux {
    facings: Vec<(u32, u8)>,
    chest: Vec<(u32, [Option<crate::item::WireItemStack>; 27])>,
    furnace: Vec<(u32, WireFurnaceData)>,
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
            (i as u32, WireFurnaceData::from_data(s))
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
#[allow(dead_code)] // Legacy wire helper retained until the chunk protocol migration is complete.
pub fn facings_to_wire(facings: &HashMap<usize, u8>) -> Vec<(u32, u8)> {
    facings.iter().map(|(&k, &v)| (k as u32, v)).collect()
}

/// แปลง chest+furnace slots เป็นรูปแบบสายส่งเดียวกัน (kind tag: 0=chest, 1=furnace)
#[allow(dead_code)] // Legacy wire helper retained until the chunk protocol migration is complete.
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
#[allow(dead_code)] // Legacy wire helper retained until the chunk protocol migration is complete.
pub fn wire_to_facings(wire: Vec<(u32, u8)>) -> HashMap<usize, u8> {
    wire.into_iter().map(|(k, v)| (k as usize, v)).collect()
}

/// กลับด้าน containers_to_wire — kind 0=chest(27)/1=furnace(3), ช่องอื่นทิ้ง (ข้อมูลเพี้ยน)
#[allow(dead_code)] // Legacy wire helper retained until the chunk protocol migration is complete.
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
pub fn load_chunk_aux(chunk_pos: IVec2) -> (HashMap<usize, u8>, HashMap<usize, Box<[Option<ItemStack>; 27]>>, HashMap<usize, Box<FurnaceData>>) {
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
        (i as usize, Box::new(wire.to_data()))
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
    pub furnace_slots: HashMap<usize, Box<FurnaceData>>,
    /// โครงกิ่งของ chunk นี้ — มาจาก .tree.bin ถ้าโหลดจาก disk หรือจากตัวปั้นต้นไม้
    /// ถ้าเป็น chunk ที่เพิ่ง generate (ดู spawn_block_generation_task)
    pub branches: Vec<crate::tree::BranchRecord>,
    /// Metadata calculated on the worker while the block volume is hot in cache.
    pub water_bounds: (usize, usize),
    pub emitters: HashSet<IVec3>,
    pub work_micros: u64,
    pub version: u32,
}

pub struct ChunkMeshData {
    pub chunk_pos: IVec2,
    pub set: ChunkMeshSet,
    pub work_micros: u64,
    pub version: u32,
}

pub struct ChunkLightData {
    pub chunk_pos: IVec2,
    pub light: Arc<crate::light::ChunkLight>,
    pub block_light: Arc<crate::light::BlockLight>,
    pub missing_neighbors: u8,
    pub revision: u64,
    pub work_micros: u64,
    pub version: u32,
}

#[derive(Resource, Default)]
pub struct ChunkPipelineStats {
    pub block_jobs: u64,
    pub block_work_micros: u64,
    pub light_jobs: u64,
    pub light_work_micros: u64,
    pub mesh_jobs: u64,
    pub mesh_work_micros: u64,
    pub block_integrate_micros: u64,
    pub light_integrate_micros: u64,
    pub mesh_integrate_micros: u64,
    pub max_pending_blocks: usize,
    pub max_pending_lights: usize,
    pub max_pending_meshes: usize,
    /// End-to-end request-to-visible samples, capped to bound long-session memory.
    pub visible_latency_micros: Vec<u64>,
}

#[derive(Resource)]
pub struct ChunkGenerator {
    pub sender_blocks: Mutex<Sender<ChunkBlockData>>,
    pub receiver_blocks: Mutex<Receiver<ChunkBlockData>>,
    pub sender_meshes: Mutex<Sender<ChunkMeshData>>,
    pub receiver_meshes: Mutex<Receiver<ChunkMeshData>>,
    pub sender_lights: Mutex<Sender<ChunkLightData>>,
    pub receiver_lights: Mutex<Receiver<ChunkLightData>>,
    pub generating_blocks: HashMap<IVec2, bool>,
    pub generating_meshes: HashMap<IVec2, bool>,
    pub generating_lights: HashMap<IVec2, u64>,
    pub requested_at: HashMap<IVec2, Instant>,
    /// Completed jobs wait here so the main thread can apply the nearest chunks first.
    pub pending_blocks: Vec<ChunkBlockData>,
    pub pending_meshes: Vec<ChunkMeshData>,
    pub pending_lights: Vec<ChunkLightData>,
    pub stats: ChunkPipelineStats,
    pub stream_center: IVec2,
    pub stream_keep_distance: i32,
    /// เพิ่มทีละ 1 ทุกครั้งที่ล้างโลก — ผลจาก task รุ่นเก่าจะถูกทิ้ง
    pub version: u32,
}

impl Default for ChunkGenerator {
    fn default() -> Self {
        let (sb, rb) = mpsc::channel();
        let (sm, rm) = mpsc::channel();
        let (sl, rl) = mpsc::channel();
        Self {
            sender_blocks: Mutex::new(sb),
            receiver_blocks: Mutex::new(rb),
            sender_meshes: Mutex::new(sm),
            receiver_meshes: Mutex::new(rm),
            sender_lights: Mutex::new(sl),
            receiver_lights: Mutex::new(rl),
            generating_blocks: HashMap::new(),
            generating_meshes: HashMap::new(),
            generating_lights: HashMap::new(),
            requested_at: HashMap::new(),
            pending_blocks: Vec::new(),
            pending_meshes: Vec::new(),
            pending_lights: Vec::new(),
            stats: ChunkPipelineStats::default(),
            stream_center: IVec2::ZERO,
            stream_keep_distance: 10,
            version: 0,
        }
    }
}

fn light_job_chunk(
    blocks: Arc<ChunkBlocks>,
    emitters: HashSet<IVec3>,
    dirty: bool,
) -> ChunkData {
    ChunkData {
        blocks,
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
        block_light: Default::default(),
        light_dirty: dirty,
        light_revision: 1,
        light_missing_neighbors: 0,
        emitters,
    }
}

fn spawn_light_generation_task(
    chunk_pos: IVec2,
    blocks: Arc<ChunkBlocks>,
    emitters: HashSet<IVec3>,
    neighbors: [Option<(Arc<ChunkBlocks>, HashSet<IVec3>)>; 8],
    revision: u64,
    version: u32,
    sender: Sender<ChunkLightData>,
) {
    AsyncComputeTaskPool::get()
        .spawn(async move {
            let started = Instant::now();
            // Reuse the canonical synchronous light implementation against an
            // isolated snapshot. No ECS resource is touched from this worker.
            let mut snapshot = VoxelWorld::default();
            snapshot
                .chunks
                .insert(chunk_pos, light_job_chunk(blocks, emitters, true));
            for (i, neighbor) in neighbors.into_iter().enumerate() {
                if let Some((blocks, emitters)) = neighbor {
                    snapshot.chunks.insert(
                        chunk_neighbors(chunk_pos)[i],
                        light_job_chunk(blocks, emitters, false),
                    );
                }
            }
            ensure_chunk_light(&mut snapshot, chunk_pos);
            if let Some(chunk) = snapshot.chunks.remove(&chunk_pos) {
                let _ = sender.send(ChunkLightData {
                    chunk_pos,
                    light: chunk.light,
                    block_light: chunk.block_light,
                    missing_neighbors: chunk.light_missing_neighbors,
                    revision,
                    work_micros: started.elapsed().as_micros() as u64,
                    version,
                });
            }
        })
        .detach();
}

fn chunk_metadata(blocks: &ChunkBlocks) -> ((usize, usize), HashSet<IVec3>) {
    let water_bounds = scan_water_bounds(blocks);
    let mut emitters = HashSet::new();
    blocks.for_each_matching(
        |b| crate::light::emitter_rgb(b) != [0, 0, 0],
        |x, y, z, _| {
            emitters.insert(IVec3::new(x as i32, y as i32, z as i32));
        },
    );
    (water_bounds, emitters)
}

pub fn spawn_block_generation_task(
    chunk_pos: IVec2,
    noise: crate::NoiseParams,
    version: u32,
    sender: Sender<ChunkBlockData>,
    use_disk_save: bool,
) {
    AsyncComputeTaskPool::get().spawn(async move {
        let started = Instant::now();
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
            None => generate_chunk_blocks(chunk_pos, noise),
        };
        let (water_bounds, emitters) = chunk_metadata(&blocks);
        let _ = sender.send(ChunkBlockData {
            chunk_pos,
            blocks: Arc::new(blocks),
            chiseled: HashMap::new(),
            facings,
            chest_slots,
            furnace_slots,
            branches,
            water_bounds,
            emitters,
            work_micros: started.elapsed().as_micros() as u64,
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
    breaking_target: Option<(IVec3, f32)>,
    sender: Sender<ChunkMeshData>,
) {
    AsyncComputeTaskPool::get().spawn(async move {
        let started = Instant::now();
        let set = create_mesh_from_blocks(chunk_pos, &blocks, &neighbors, None, Some(&facings), Some(&branches), Some(&light), breaking_target);
        let _ = sender.send(ChunkMeshData {
            chunk_pos,
            set,
            work_micros: started.elapsed().as_micros() as u64,
            version,
        });
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
        let started = Instant::now();
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
                // preview เป็นเครื่องมือจูน noise — ใช้ทะเล noise เสมอ + biome จาก noise
                let col = sampler.column_biome(base_x + x as f64, base_z + z as f64);
                let top = surface_block_for(col, h, SEA_LEVEL as i32, sampler.snow_line());
                let side = col.subsurface;

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
            work_micros: started.elapsed().as_micros() as u64,
            version,
        });
    }).detach();
}

// --------------------------------------------------------
// Setup & Systems
// --------------------------------------------------------

#[derive(Resource)]
pub struct ChunkMaterial(pub Handle<StandardMaterial>);

/// material ของ overlay แสงโคม — additive (บวกสีลง framebuffer) unlit ไม่มี day tint
#[derive(Resource)]
pub struct BlockLightMaterial(pub Handle<StandardMaterial>);

#[derive(Asset, TypePath, bevy::render::render_resource::AsBindGroup, Debug, Clone, Default)]
pub struct CustomWaterMaterial {
    #[uniform(0)]
    pub uniforms: WaterUniforms,
}

#[derive(bevy::render::render_resource::ShaderType, Debug, Clone, Default)]
pub struct WaterUniforms {
    pub color: LinearRgba,
    pub shallow_color: LinearRgba,
    pub deep_color: LinearRgba,
    pub reflection_color: LinearRgba,
    pub sun_dir: Vec3,
    /// x: flow speed, y: wave strength, z: foam strength, w: reflection strength
    pub tuning: Vec4,
}

impl bevy::pbr::Material for CustomWaterMaterial {
    fn fragment_shader() -> bevy::shader::ShaderRef {
        "shaders/water.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

#[derive(Resource)]
pub struct WaterMaterial(pub Handle<CustomWaterMaterial>);

#[derive(Asset, TypePath, bevy::render::render_resource::AsBindGroup, Debug, Clone)]
pub struct SeasonalFoliageMaterial {
    #[uniform(0)]
    pub uniforms: SeasonalFoliageUniforms,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Handle<Image>,
}

#[derive(bevy::render::render_resource::ShaderType, Debug, Clone, Default)]
pub struct SeasonalFoliageUniforms {
    /// rgb = day/night tint, a unused
    pub daylight: LinearRgba,
    /// x = day_of_year. ที่เหลือสงวนไว้สำหรับ tuning ในอนาคต
    pub tuning: Vec4,
}

impl bevy::pbr::Material for SeasonalFoliageMaterial {
    fn fragment_shader() -> bevy::shader::ShaderRef {
        "shaders/seasonal_foliage.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Mask(0.5)
    }

}

#[derive(Resource)]
pub struct SeasonalFoliageMaterialHandles {
    pub oak: Handle<SeasonalFoliageMaterial>,
    pub maple: Handle<SeasonalFoliageMaterial>,
}

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

#[derive(bevy::ecs::system::SystemParam)]
pub struct ChunkRenderMaterials<'w> {
    pub chunk: Res<'w, ChunkMaterial>,
    pub block_light: Res<'w, BlockLightMaterial>,
    pub water: Res<'w, WaterMaterial>,
    pub glass: Res<'w, GlassMaterial>,
    pub deco: Res<'w, DecoMaterials>,
    pub foliage: Res<'w, SeasonalFoliageMaterialHandles>,
    pub lamps: Res<'w, LampMaterials>,
    pub blocks: Res<'w, BlockMaterials>,
}

pub fn setup_voxel(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut custom_water_materials: ResMut<Assets<CustomWaterMaterial>>,
    mut foliage_materials: ResMut<Assets<SeasonalFoliageMaterial>>,
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

    // overlay แสงโคม: additive blend (Add) — บวกสีทับ terrain, unlit, ไม่โดน day/night tint
    let block_light_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        alpha_mode: AlphaMode::Add,
        unlit: true,
        perceptual_roughness: 1.0,
        depth_bias: 0.5, // ดันมาข้างหน้านิด กัน z-fight กับหน้า terrain เดียวกัน
        ..default()
    });
    commands.insert_resource(BlockLightMaterial(block_light_material));

    // สีน้ำมาจาก vertex color — material เป็น custom material ที่เลื่อนคลื่น
    let water_material = custom_water_materials.add(CustomWaterMaterial {
        uniforms: WaterUniforms {
            color: LinearRgba::WHITE,
            shallow_color: LinearRgba::new(0.20, 0.72, 0.72, 0.28),
            deep_color: LinearRgba::new(0.015, 0.16, 0.30, 0.72),
            reflection_color: LinearRgba::new(0.38, 0.62, 0.88, 1.0),
            sun_dir: Vec3::Y,
            tuning: Vec4::new(0.75, 0.16, 0.85, 0.72),
        }
    });
    commands.insert_resource(WaterMaterial(water_material));

    let foliage_material = foliage_materials.add(SeasonalFoliageMaterial {
        uniforms: SeasonalFoliageUniforms {
            daylight: LinearRgba::WHITE,
            tuning: Vec4::new(
                CURRENT_DAY_OF_YEAR.load(std::sync::atomic::Ordering::Relaxed) as f32,
                0.0,
                0.0,
                0.0,
            ),
        },
        texture: asset_server.load("textures/leaves.png"),
    });
    let maple_foliage_material = foliage_materials.add(SeasonalFoliageMaterial {
        uniforms: SeasonalFoliageUniforms {
            daylight: LinearRgba::WHITE,
            // y = palette species (0 oak, 1 maple)
            tuning: Vec4::new(
                CURRENT_DAY_OF_YEAR.load(std::sync::atomic::Ordering::Relaxed) as f32,
                1.0,
                0.0,
                0.0,
            ),
        },
        texture: asset_server.load("textures/maple_leaves.png"),
    });
    commands.insert_resource(SeasonalFoliageMaterialHandles {
        oak: foliage_material,
        maple: maple_foliage_material,
    });

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
    cutout_sprites.extend_from_slice(BLOCK_DEFS[BlockType::SpruceLeaves as usize].tex_side);

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
const TIME_FAST_FORWARD_SPEED: f32 = 120.0;

#[derive(Resource, Default, Debug)]
pub struct TimeFastForward {
    pub target_hour: Option<f32>,
    sync_elapsed: f32,
}

impl TimeFastForward {
    pub fn start(&mut self, target_hour: f32) {
        self.target_hour = Some(target_hour.rem_euclid(24.0));
        self.sync_elapsed = 0.0;
    }
}

fn advance_calendar(settings: &mut crate::GameSettings, delta_hours: f32) {
    let raw = settings.time_of_day + delta_hours;
    let days_passed = (raw / 24.0).floor() as i64;
    if days_passed != 0 {
        let total = settings.day_of_year as i64 + days_passed;
        let dpy = crate::astro::DAYS_PER_YEAR as i64;
        settings.year = (settings.year as i64 + total.div_euclid(dpy)).max(0) as u32;
        settings.day_of_year = total.rem_euclid(dpy) as u16;
        CURRENT_DAY_OF_YEAR.store(settings.day_of_year as u32, std::sync::atomic::Ordering::Relaxed);
    }
    settings.time_of_day = raw.rem_euclid(24.0);
}

fn fast_forward_step(current: f32, target: f32, max_step: f32) -> (f32, bool) {
    let remaining = (target - current).rem_euclid(24.0);
    if remaining <= 0.001 {
        (0.0, true)
    } else if remaining <= max_step {
        (remaining, true)
    } else {
        (max_step, false)
    }
}

/// เดินเวลาของวันอัตโนมัติ (day-night cycle) — host/single เท่านั้น
/// client รับเวลาจาก host ผ่าน TimeOfDay message (ดู network.rs) จึงไม่เดินเอง
pub fn advance_time_system(
    time: Res<Time>,
    mut settings: ResMut<crate::GameSettings>,
    mut fast_forward: ResMut<TimeFastForward>,
    net_client: Option<Res<bevy_renet::RenetClient>>,
    mut server: Option<ResMut<bevy_renet::RenetServer>>,
) {
    if net_client.is_some() {
        return;
    }
    if let Some(target) = fast_forward.target_hour {
        let max_step =
            time.delta_secs() * 24.0 / GAME_DAY_SECONDS * TIME_FAST_FORWARD_SPEED;
        let (step, arrived) = fast_forward_step(settings.time_of_day, target, max_step);
        advance_calendar(&mut settings, step);
        if arrived {
            settings.time_of_day = target;
            fast_forward.target_hour = None;
        }

        fast_forward.sync_elapsed += time.delta_secs();
        if arrived || fast_forward.sync_elapsed >= 0.1 {
            crate::command::broadcast_time(server.as_deref_mut(), &settings);
            fast_forward.sync_elapsed = 0.0;
        }
        return;
    }

    // day_speed = ตัวคูณความเร็วรอบวัน (0 = หยุดเวลานิ่ง) ปรับผ่าน /daynight
    let delta = time.delta_secs() * 24.0 / GAME_DAY_SECONDS * settings.day_speed;
    advance_calendar(&mut settings, delta);
}

/// ความแรงแดดตามเวลา — คูณกับ sky light ที่อบไว้ใน vertex color
/// แยกออกมาให้ระบบ tint material เรียกใช้ค่าเดียวกันทุกที่
pub fn sun_tint(time_of_day: f32, day_of_year: u16, latitude_deg: f32) -> (f32, Color) {
    // ทิศดวงอาทิตย์อิงดาราศาสตร์จริง (ฤดู+ละติจูด) — ตัวเดียวกับที่ท้องฟ้า (sky_uniform) ใช้
    let elevation = crate::astro::sun_direction(
        time_of_day,
        day_of_year as f32,
        latitude_deg.to_radians(),
    )
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
    mut fog_query: Query<&mut bevy::pbr::DistanceFog>,
    mut clear_color: ResMut<ClearColor>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut custom_water_materials: ResMut<Assets<CustomWaterMaterial>>,
    mut foliage_materials: ResMut<Assets<SeasonalFoliageMaterial>>,
    chunk_mat: Option<Res<ChunkMaterial>>,
    block_mats: Option<Res<BlockMaterials>>,
    deco_mats: Option<Res<DecoMaterials>>,
    glass_mat: Option<Res<GlassMaterial>>,
    water_mat: Option<Res<WaterMaterial>>,
    foliage_mat: Option<Res<SeasonalFoliageMaterialHandles>>,
    lod: Option<Res<crate::lod::LodTiles>>,
    mut last: Local<Option<(f32, u16, bool, bool)>>,
) {
    // **Gate ทั้งฟังก์ชันบนสุด** — day-night ทำให้เวลาเดินทุกเฟรม ถ้าเขียน material/
    // clear_color ทุกเฟรม bevy จะ re-extract asset ใหม่ทุกเฟรม (เปลืองและเคยทำภาพวูบ)
    // จึงอัปเดตเฉพาะเมื่อเวลาขยับพอสังเกต (~1 วิจริง) หรือ material เพิ่งพร้อม —
    // day-night ยังลื่นพอเพราะ base_color เปลี่ยนทีละน้อยอยู่แล้ว
    let material_ready = chunk_mat.is_some();
    let xray = DEBUG_XRAY_ENABLED.load(Ordering::Relaxed);
    if let Some((last_time, last_day, last_ready, last_xray)) = *last {
        if last_ready == material_ready
            && last_day == settings.day_of_year
            && last_xray == xray
            && (settings.time_of_day - last_time).abs() < 0.02
        {
            return;
        }
    }
    *last = Some((settings.time_of_day, settings.day_of_year, material_ready, xray));

    let (elevation, tint) = sun_tint(settings.time_of_day, settings.day_of_year, settings.latitude_deg);
    let render_tint = if xray { Color::WHITE } else { tint };

    // ambient เหลือแค่พื้นบางๆ — ถ้าสูงเท่าเดิม (80-400) ถ้ำจะไม่มืดเพราะ vertex light ถูกกลบ
    for mut ambient in ambient_query.iter_mut() {
        ambient.brightness = if xray { 1000.0 } else { 8.0 + 40.0 * elevation };
    }

    // material ที่รับแสงจากฟ้า — คูณ tint เข้า base_color
    // (LampMaterials ไม่อยู่ในลิสต์: emissive ต้องสว่างเท่าเดิมตอนกลางคืน)
    let mut handles: Vec<&Handle<StandardMaterial>> = Vec::new();
    if let Some(m) = chunk_mat.as_ref() { handles.push(&m.0); }
    if let Some(m) = glass_mat.as_ref() { handles.push(&m.0); }
    if let Some(m) = block_mats.as_ref() { handles.extend(m.0.values()); }
    if let Some(m) = deco_mats.as_ref() { handles.extend(m.0.values()); }
    // ภูเขาระยะไกล (LOD) ต้องมืดตามด้วย ไม่งั้นกลางคืนขอบฟ้าสว่างค้างเป็นแถบ
    if let Some(l) = lod.as_ref() { handles.push(&l.material); }
    for handle in handles {
        if let Some(mut mat) = materials.get_mut(handle) {
            mat.base_color = render_tint;
        }
    }

    let sun_dir = crate::astro::sun_direction(
        settings.time_of_day,
        settings.day_of_year as f32,
        settings.latitude_deg.to_radians(),
    );
    let night = Vec3::new(0.02, 0.02, 0.06);
    let day = Vec3::new(0.35, 0.55, 0.90);
    let sky = night.lerp(day, elevation);

    if let Some(m) = water_mat.as_ref() {
        if let Some(mut mat) = custom_water_materials.get_mut(&m.0) {
            mat.uniforms.color = render_tint.into();
            mat.uniforms.sun_dir = sun_dir;
            mat.uniforms.reflection_color = LinearRgba::new(sky.x, sky.y, sky.z, 1.0);
        }
    }
    if let Some(m) = foliage_mat.as_ref() {
        for handle in [&m.oak, &m.maple] {
            if let Some(mut mat) = foliage_materials.get_mut(handle) {
                mat.uniforms.daylight = render_tint.into();
                mat.uniforms.tuning.x = settings.day_of_year as f32;
            }
        }
    }

    // สีท้องฟ้า (fallback ก่อน skydome พร้อม / ส่วนที่ skydome ไม่ครอบ)
    clear_color.0 = Color::srgb(sky.x, sky.y, sky.z);

    // หมอกระยะไกล: ให้กลืนกับสีขอบฟ้าของ skydome ทุกช่วงเวลา
    // (ไม่งั้นกลางคืนภูเขา DEM ไกลจะยังจางเป็นสีฟ้าตายตัว ไม่เข้ากับฟ้ามืด)
    let fog_day = Vec3::new(0.66, 0.80, 0.94);
    let fog_night = Vec3::new(0.04, 0.05, 0.10);
    let fog = fog_night.lerp(fog_day, elevation);
    for mut df in fog_query.iter_mut() {
        df.color = Color::srgb(fog.x, fog.y, fog.z);
    }
}

/// ล้างโลกทั้งหมดเพื่อ generate ใหม่ (ตอนเปลี่ยน render mode หรือค่า noise)
pub fn world_reset_system(
    mut commands: Commands,
    mut request: ResMut<crate::RegenerateWorld>,
    mut world: ResMut<VoxelWorld>,
    mut generator: ResMut<ChunkGenerator>,
    mut pools: ResMut<ActivePools>,
    mut active_fluids: ResMut<ActiveFluids>,
    mut active_reactive_fluids: ResMut<ActiveReactiveFluids>,
) {
    if !request.0 {
        return;
    }
    request.0 = false;
    despawn_world(
        &mut commands,
        &mut world,
        &mut generator,
        &mut pools,
        &mut active_fluids,
        &mut active_reactive_fluids,
    );
}

/// ล้างโลกทั้งใบ: mesh entity ทุกชั้น + block data + งาน generate ที่ค้าง
/// (ใช้ร่วมกันระหว่าง regenerate กลางเกม กับตอนออกจากโลกกลับเมนู)
fn despawn_world(
    commands: &mut Commands,
    world: &mut VoxelWorld,
    generator: &mut ChunkGenerator,
    pools: &mut ActivePools,
    active_fluids: &mut ActiveFluids,
    active_reactive_fluids: &mut ActiveReactiveFluids,
) {
    // โลกกำลังจะหายทั้งใบ — สระ/น้ำที่ตื่นอยู่อ้างอิงบล็อกเก่า ทิ้งให้หมด
    pools.0.clear();
    active_fluids.0.clear();
    active_reactive_fluids.0.clear();

    for (_, entity) in world.generated_chunks.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in world.water_chunks.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in world.glass_chunks.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in world.block_light_chunks.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entities) in world.deco_chunks.drain() {
        for entity in entities { commands.entity(entity).despawn(); }
    }
    for (_, entity) in world.seasonal_foliage_chunks.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in world.maple_foliage_chunks.drain() {
        commands.entity(entity).despawn();
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
    generator.generating_lights.clear();
    generator.requested_at.clear();
    generator.pending_blocks.clear();
    generator.pending_meshes.clear();
    generator.pending_lights.clear();
    generator.stats = ChunkPipelineStats::default();
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
    mut active_reactive_fluids: ResMut<ActiveReactiveFluids>,
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

    despawn_world(
        &mut commands,
        &mut world,
        &mut generator,
        &mut pools,
        &mut active_fluids,
        &mut active_reactive_fluids,
    );

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
    let own_chunk = &world.chunks[&chunk_pos];
    let blocks = own_chunk.blocks.clone();
    let own_emitters = own_chunk.emitters.clone();
    
    // เพื่อนบ้านที่ยังไม่โหลดถือเป็นฟ้าโล่ง — ห้ามบังคับให้ต้องครบ 8 ตัวก่อน ไม่งั้น
    // chunk ริมขอบ render distance จะคำนวณแสงไม่ได้เลย แล้วก็ mesh ไม่ได้ตามไปด้วย
    // (ค่าตรงขอบจะเพี้ยนนิดหน่อยจนกว่าเพื่อนบ้านจะมาถึง — ตอนนั้นถูกตีธง dirty ให้คิดใหม่)
    let empty: Arc<ChunkBlocks> = Arc::new(ChunkBlocks::new_uniform(BlockType::Air));
    let mut missing: u8 = 0;
    
    let mut neighbor_emitters: [std::collections::HashSet<IVec3>; 8] = Default::default();
    let neighbors: [Arc<ChunkBlocks>; 8] = {
        let positions = chunk_neighbors(chunk_pos);
        std::array::from_fn(|i| match world.chunks.get(&positions[i]) {
            Some(c) => {
                neighbor_emitters[i] = c.emitters.clone();
                c.blocks.clone()
            },
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
    let t0 = std::time::Instant::now();
    let light = crate::light::compute_sky_light(&sampler, scan_top);
    let t_sky = t0.elapsed();

    // block light (โคม/คบไฟ) — จำกัดแถบ y แค่ ~280 บล็อกใต้ยอด (ผิว + ถ้ำ 200m + margin)
    // ไม่งั้น halo scan ×ทั้งคอลัมน์ (0..ผิว) จะช้ามากตอนโหลด; ใต้นั้นเป็นหินตันไม่มีโคม/อากาศ
    let blk_top = blocks.y_bounds_non_air().map_or(0, |(_, hi)| hi).max(scan_top);

    // รวบรวม seed ของ emitter (โคม/คบไฟ) จาก chunk หลัก + เพื่อนบ้าน 8 ทิศ ในระยะ halo ±15
    let w = CHUNK_WIDTH as i32;
    let r = crate::light::MAX_LIGHT as i32; // 15 = ระยะไกลสุดที่แสงโคมเดินถึง
    let blk_lo = blk_top.saturating_sub(280);
    let mut seeds: Vec<([i32; 3], [u8; 3])> = Vec::new();

    let t1 = std::time::Instant::now();
    // chunk หลัก (พิกัด local 0..w)
    for pos in own_emitters {
        if pos.y >= blk_lo as i32 && pos.y <= blk_top as i32 {
            let b = blocks.get(pos.x as usize, pos.y as usize, pos.z as usize);
            seeds.push(([pos.x, pos.y, pos.z], crate::light::emitter_rgb(b)));
        }
    }

    // เพื่อนบ้าน 8 ทิศ — แปลงเป็นพิกัด local ของ chunk หลัก แล้วกรองเฉพาะใน halo ±r
    let offsets: [(i32, i32); 8] = [(1,0), (-1,0), (0,1), (0,-1), (1,1), (1,-1), (-1,1), (-1,-1)];
    for (ni, nb_emitters) in neighbor_emitters.into_iter().enumerate() {
        let (dx, dz) = offsets[ni];
        let nb_blocks = &neighbors[ni];
        for pos in nb_emitters {
            let lx = pos.x + dx * w;
            let lz = pos.z + dz * w;
            // เฉพาะในระยะ halo ±r และแถบ y ที่คำนวณ
            if lx >= -r && lx < w + r && lz >= -r && lz < w + r
                && pos.y >= blk_lo as i32 && pos.y <= blk_top as i32
            {
                let b = nb_blocks.get(pos.x as usize, pos.y as usize, pos.z as usize);
                seeds.push(([lx, pos.y, lz], crate::light::emitter_rgb(b)));
            }
        }
    }
    let t_scan = t1.elapsed();

    let t2 = std::time::Instant::now();
    let block_light = crate::light::compute_block_light(&sampler, blk_lo, blk_top, &seeds);
    let t_block = t2.elapsed();
    
    if (t_sky + t_scan + t_block).as_millis() > 2 {
        println!("  ensure_chunk_light: sky={:?}, scan={:?}, block={:?}", t_sky, t_scan, t_block);
    }

    let mut changed = false;
    if let Some(chunk) = world.chunks.get_mut(&chunk_pos) {
        changed = *chunk.light != light || *chunk.block_light != block_light;
        if changed {
            chunk.light = Arc::new(light);
            chunk.block_light = Arc::new(block_light);
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
    pub block_own: Arc<crate::light::BlockLight>,
    pub block_neighbors: [Arc<crate::light::BlockLight>; 8],
}

impl LightNeighborhood {
    /// เลือก ChunkLight ของ chunk ที่พิกัด (x,z) ตกลงไป (own/neighbor) — คืน src + local x,z
    #[inline]
    fn pick<'a>(
        own: &'a Arc<crate::light::ChunkLight>,
        neighbors: &'a [Arc<crate::light::ChunkLight>; 8],
        x: i32,
        z: i32,
    ) -> (&'a Arc<crate::light::ChunkLight>, usize, usize) {
        let w = CHUNK_WIDTH as i32;
        let (lx, lz) = (x.rem_euclid(w) as usize, z.rem_euclid(w) as usize);
        let src = match (x.div_euclid(w), z.div_euclid(w)) {
            (0, 0) => own,
            (1, 0) => &neighbors[0],
            (-1, 0) => &neighbors[1],
            (0, 1) => &neighbors[2],
            (0, -1) => &neighbors[3],
            (1, 1) => &neighbors[4],
            (1, -1) => &neighbors[5],
            (-1, 1) => &neighbors[6],
            _ => &neighbors[7],
        };
        (src, lx, lz)
    }

    /// sky light ที่พิกัด local (ทะลุขอบไปหาเพื่อนบ้านได้)
    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return crate::light::MAX_LIGHT;
        }
        let (src, lx, lz) = Self::pick(&self.own, &self.neighbors, x, z);
        src.get(lx, y as usize, lz)
    }

    /// block light สี (RGB) ที่พิกัด local — นอกช่วง y = [0,0,0] (ไม่มีแสงโคมนอกโลก)
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> [u8; 3] {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return [0, 0, 0];
        }
        let w = CHUNK_WIDTH as i32;
        let (lx, lz) = (x.rem_euclid(w) as usize, z.rem_euclid(w) as usize);
        let src = match (x.div_euclid(w), z.div_euclid(w)) {
            (0, 0) => &self.block_own,
            (1, 0) => &self.block_neighbors[0],
            (-1, 0) => &self.block_neighbors[1],
            (0, 1) => &self.block_neighbors[2],
            (0, -1) => &self.block_neighbors[3],
            (1, 1) => &self.block_neighbors[4],
            (1, -1) => &self.block_neighbors[5],
            (-1, 1) => &self.block_neighbors[6],
            _ => &self.block_neighbors[7],
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
        block_own: own_chunk.block_light.clone(),
        block_neighbors: positions.map(|p| world.chunks[&p].block_light.clone()),
    })
}

/// คำนวณ sky light ใหม่ให้ chunk ที่ dirty แล้วสั่ง remesh ตัวที่มี mesh อยู่แล้ว
/// (chunk ที่ยังไม่เคยมี mesh ไม่ต้องสั่ง — ระบบ generate จะ mesh ให้เองเมื่อแสงพร้อม)
#[inline]
fn chunk_distance_squared(center: IVec2, pos: IVec2) -> i32 {
    let d = pos - center;
    d.x.saturating_mul(d.x) + d.y.saturating_mul(d.y)
}

#[inline]
fn light_result_is_current(
    world_version: u32,
    chunk_revision: u64,
    result_version: u32,
    result_revision: u64,
) -> bool {
    world_version == result_version && chunk_revision == result_revision
}

pub fn relight_system(
    mut world: ResMut<VoxelWorld>,
    mut generator: ResMut<ChunkGenerator>,
) {
    let center = generator.stream_center;
    let distance2 = |p: IVec2| chunk_distance_squared(center, p);

    let mut drained = Vec::new();
    {
        let receiver = generator
            .receiver_lights
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while let Ok(data) = receiver.try_recv() {
            drained.push(data);
        }
    }
    generator.stats.light_jobs += drained.len() as u64;
    generator.stats.light_work_micros += drained.iter().map(|d| d.work_micros).sum::<u64>();
    generator.pending_lights.extend(drained);
    generator.stats.max_pending_lights =
        generator.stats.max_pending_lights.max(generator.pending_lights.len());
    generator
        .pending_lights
        .sort_unstable_by_key(|data| std::cmp::Reverse(distance2(data.chunk_pos)));

    let apply_started = Instant::now();
    let mut applied = 0usize;
    while applied < 8
        && (applied == 0 || apply_started.elapsed() < Duration::from_millis(1))
    {
        let Some(data) = generator.pending_lights.pop() else {
            break;
        };
        if data.version != generator.version {
            continue;
        }
        if generator.generating_lights.get(&data.chunk_pos) == Some(&data.revision) {
            generator.generating_lights.remove(&data.chunk_pos);
        }
        // Neighbor availability may change while the async light job is running.
        // Never apply a result whose snapshot no longer matches the world: doing so
        // can restore a stale `missing_neighbors` bit after that neighbor's arrival
        // event already ran, leaving this chunk permanently ineligible for meshing.
        let current_missing_neighbors = chunk_neighbors(data.chunk_pos)
            .iter()
            .enumerate()
            .fold(0u8, |mask, (index, pos)| {
                if world.chunks.contains_key(pos) {
                    mask
                } else {
                    mask | (1 << index)
                }
            });
        if current_missing_neighbors != data.missing_neighbors {
            if let Some(chunk) = world.chunks.get_mut(&data.chunk_pos) {
                chunk.light_dirty = true;
                chunk.light_revision = chunk.light_revision.wrapping_add(1);
            }
            continue;
        }
        let Some(chunk) = world.chunks.get_mut(&data.chunk_pos) else {
            continue;
        };
        if !light_result_is_current(
            generator.version,
            chunk.light_revision,
            data.version,
            data.revision,
        ) {
            continue;
        }

        let changed = *chunk.light != *data.light || *chunk.block_light != *data.block_light;
        chunk.light = data.light;
        chunk.block_light = data.block_light;
        chunk.light_dirty = false;
        chunk.light_missing_neighbors = data.missing_neighbors;
        if changed && world.generated_chunks.contains_key(&data.chunk_pos) {
            world.pending_branch_remesh.insert(data.chunk_pos);
        }
        applied += 1;
    }
    generator.stats.light_integrate_micros += apply_started.elapsed().as_micros() as u64;

    let workers = std::thread::available_parallelism().map_or(1, |n| n.get());
    let block_slots = workers.div_ceil(2).max(1);
    let mesh_slots = (workers / 4).max(1);
    let light_slots = workers.saturating_sub(block_slots + mesh_slots).max(1);
    let total_inflight = generator.generating_blocks.len()
        + generator.generating_meshes.len()
        + generator.generating_lights.len();
    let available = light_slots
        .saturating_sub(generator.generating_lights.len())
        .min(workers.saturating_sub(total_inflight));
    if available == 0 {
        return;
    }

    let mut dirty: Vec<IVec2> = world
        .chunks
        .iter()
        .filter(|(pos, chunk)| {
            chunk.light_dirty
                && !generator.generating_lights.contains_key(pos)
                // First meshes already require all eight neighbors for AO. A
                // provisional light pass before then can never become visible
                // and only gets invalidated as those neighbors arrive.
                && chunk_neighbors(**pos).iter().all(|neighbor| world.chunks.contains_key(neighbor))
        })
        .map(|(pos, _)| *pos)
        .collect();
    dirty.sort_unstable_by_key(|pos| distance2(*pos));

    for chunk_pos in dirty.into_iter().take(available) {
        let Some(own) = world.chunks.get(&chunk_pos) else {
            continue;
        };
        let blocks = own.blocks.clone();
        let emitters = own.emitters.clone();
        let revision = own.light_revision;
        let neighbors = chunk_neighbors(chunk_pos).map(|pos| {
            world
                .chunks
                .get(&pos)
                .map(|chunk| (chunk.blocks.clone(), chunk.emitters.clone()))
        });
        let sender = generator
            .sender_lights
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        generator.generating_lights.insert(chunk_pos, revision);
        spawn_light_generation_task(
            chunk_pos,
            blocks,
            emitters,
            neighbors,
            revision,
            generator.version,
            sender,
        );
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
    breaking: Option<Res<BreakingProgress>>,
    // cache offset เรียงจากใกล้ไปไกล (สร้างใหม่เมื่อ render distance เปลี่ยน)
    mut offsets_cache: Local<(i32, Vec<IVec2>)>,
    mut configured_noise: Local<Option<crate::NoiseParams>>,
) {
    let Some(camera_transform) = camera_query.iter().next() else { return };
    let cam_pos = camera_transform.translation;

    let center_chunk_x = cam_pos.x.div_euclid(CHUNK_WIDTH as f32) as i32;
    let center_chunk_z = cam_pos.z.div_euclid(CHUNK_WIDTH as f32) as i32;

    let render_distance = settings.render_distance;
    generator.stream_center = IVec2::new(center_chunk_x, center_chunk_z);
    generator.stream_keep_distance = render_distance + 2;
    if *configured_noise != Some(settings.noise) {
        set_worldgen_climate(settings.noise);
        crate::hydro::configure(settings.noise);
        *configured_noise = Some(settings.noise);
    }

    if offsets_cache.0 != render_distance || offsets_cache.1.is_empty() {
        // โหลดเป็นวงกลม (รัศมี = render_distance) แทนสี่เหลี่ยม — มุมไม่โหลด ขอบฟ้ากลม
        let r2 = render_distance * render_distance;
        let mut offsets = Vec::new();
        for dx in -render_distance..=render_distance {
            for dz in -render_distance..=render_distance {
                if dx * dx + dz * dz <= r2 {
                    offsets.push(IVec2::new(dx, dz));
                }
            }
        }
        offsets.sort_by_key(|o| o.x * o.x + o.y * o.y);
        *offsets_cache = (render_distance, offsets);
    }

    // จำกัดจำนวน task ต่อเฟรม: chunk ใกล้ตัวได้คิวก่อน และเฟรมไม่สะดุด
    let mut block_budget: usize = 6;
    let mut mesh_budget: usize = 8;
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get());
    // Keep the compute pool bounded across the three heavy stages. With the old
    // 2×/1×/1× limits, distant queued jobs could occupy roughly four times the
    // available workers and delay chunks nearest to the player.
    let max_block_jobs = workers.div_ceil(2).max(1);
    let max_mesh_jobs = (workers / 4).max(1);

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
                && generator.generating_meshes.len() < max_mesh_jobs
                && generator.generating_blocks.len()
                    + generator.generating_meshes.len()
                    + generator.generating_lights.len()
                    < workers
                && !world.generated_chunks.contains_key(&chunk_pos)
                && !generator.generating_meshes.contains_key(&chunk_pos)
            {
                generator.requested_at.entry(chunk_pos).or_insert_with(Instant::now);
                generator.generating_meshes.insert(chunk_pos, true);
                let sender = generator
                    .sender_meshes
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                spawn_surface_preview_task(chunk_pos, settings.noise, generator.version, sender);
                mesh_budget -= 1;
            }
            continue;
        }

        // Phase 1: Block Generation
        if block_budget > 0
            && generator.generating_blocks.len() < max_block_jobs
            && generator.generating_blocks.len()
                + generator.generating_meshes.len()
                + generator.generating_lights.len()
                < workers
            && !world.chunks.contains_key(&chunk_pos)
            && !generator.generating_blocks.contains_key(&chunk_pos)
        {
            generator.requested_at.entry(chunk_pos).or_insert_with(Instant::now);
            generator.generating_blocks.insert(chunk_pos, true);
            let sender = generator
                .sender_blocks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            // network client: chunk ที่ host ส่งมา (มี edit) ใช้แทนการ generate
            if let Some(received) = client_sync.as_ref().and_then(|cs| cs.full_chunks.get(&chunk_pos)) {
                let blocks = ChunkBlocks::from_dense_bytes(&received.blocks);
                let (water_bounds, emitters) = chunk_metadata(&blocks);
                let _ = sender.send(ChunkBlockData {
                    chunk_pos,
                    blocks: Arc::new(blocks),
                    chiseled: received.chiseled.clone(),
                    facings: received.facings.clone(),
                    chest_slots: received.chest_slots.clone(),
                    furnace_slots: received.furnace_slots.clone(),
                    branches: received.branches.clone(),
                    water_bounds,
                    emitters,
                    work_micros: 0,
                    version: generator.version,
                });
            } else {
                spawn_block_generation_task(
                    chunk_pos, settings.noise, generator.version, sender,
                    client_sync.is_none(),
                );
            }
            block_budget -= 1;
        }

        // Phase 2: Mesh Generation
        if mesh_budget > 0
            && generator.generating_meshes.len() < max_mesh_jobs
            && generator.generating_blocks.len()
                + generator.generating_meshes.len()
                + generator.generating_lights.len()
                < workers
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

                let sender = generator
                    .sender_meshes
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                let breaking_target = breaking.as_ref().and_then(|b| b.target);
                spawn_mesh_generation_task(chunk_pos, blocks, neighbors, facings, branches, light, generator.version, breaking_target, sender);
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
fn update_single_mesh_entity<M: bevy::pbr::Material>(
    commands: &mut Commands,
    map: &mut HashMap<IVec2, Entity>,
    meshes: &mut Assets<Mesh>,
    mesh_query: &Query<&Mesh3d>,
    material: &Handle<M>,
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
    // spawn PointLight จริง **เฉพาะไฟ dynamic** (SmartLamp/ไฟฟ้า) — ไฟ static ใช้ baked overlay
    // อย่างเดียว (คบไฟ/glowstone/campfire เป็นร้อยดวงเลยไม่ทำ clustering เต็ม/กระพริบอีก)
    chunk.blocks.for_each_matching(
        |b| lamp_emission(b).is_some() && is_dynamic_emitter(b),
        |x, y, z, block| {
            let Some(color) = lamp_emission(block) else { return };
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
                    y as f32 + 0.625, // ตรงกับหลอดไฟใน model ของ SmartLamp
                    base_z + z as f32 + 0.5,
                ),
            )).id();
            lights.push(entity);
        },
    );
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
        |b| matches!(b, BlockType::Campfire | BlockType::SmartLamp | BlockType::SmartLampOn | BlockType::Crucible | BlockType::IngotMold | BlockType::PickaxeMold | BlockType::CastIngot),
        |x, y, z, block| {
            let scene = if block == BlockType::Campfire {
                assets.campfire_scene.clone()
            } else if block == BlockType::Crucible {
                assets.crucible_scene.clone()
            } else if block == BlockType::IngotMold {
                assets.ingot_mold_scene.clone()
            } else if block == BlockType::PickaxeMold {
                assets.pickaxe_mold_scene.clone()
            } else if block == BlockType::CastIngot {
                assets.ingot_scene.clone()
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

            let mut transform = Transform::from_xyz(
                base_x + x as f32 + 0.5,
                y as f32,
                base_z + z as f32 + 0.5,
            ).with_rotation(Quat::from_rotation_y(rotation));
            if block == BlockType::CastIngot {
                let pos = IVec3::new(
                    chunk_pos.x * CHUNK_WIDTH as i32 + x as i32,
                    y as i32,
                    chunk_pos.y * CHUNK_WIDTH as i32 + z as i32,
                );
                let fill = world
                    .placed_ingots
                    .get(&pos)
                    .map_or(1.0, |ingot| {
                        ingot.mass as f32 / crate::chemistry::INGOT_MOLD_CAPACITY_GRAMS as f32
                    })
                    .clamp(0.1, 1.0);
                transform.scale.y = fill;
            }
            let mut entity_commands = commands.spawn((
                WorldAssetRoot(scene),
                transform,
            ));
            if block == BlockType::CastIngot {
                let pos = IVec3::new(
                    chunk_pos.x * CHUNK_WIDTH as i32 + x as i32,
                    y as i32,
                    chunk_pos.y * CHUNK_WIDTH as i32 + z as i32,
                );
                let kind_index = world
                    .placed_ingots
                    .get(&pos)
                    .map_or(crate::chemistry::CastIngotKind::Mixed.to_u8(), |data| {
                        data.kind.to_u8()
                    }) as usize;
                entity_commands.insert(CastIngotMaterialOverride(
                    assets.cast_ingot_materials[kind_index].clone(),
                ));
            }
            let entity = entity_commands.id();
            // เปลวไฟ campfire เกาะโมเดล (ไม่พึ่ง PointLight แล้ว — campfire เป็นไฟ static)
            if block == BlockType::Campfire {
                commands.entity(entity).insert(crate::particles::CampfireFlameSource);
            }
            models.push(entity);
            if matches!(block, BlockType::IngotMold | BlockType::PickaxeMold | BlockType::Crucible) {
                let pos = IVec3::new(
                    chunk_pos.x * CHUNK_WIDTH as i32 + x as i32,
                    y as i32,
                    chunk_pos.y * CHUNK_WIDTH as i32 + z as i32,
                );
                let (mesh, kind) = if block == BlockType::Crucible {
                    (assets.crucible_fill_mesh.clone(), MetalFillKind::Crucible)
                } else if block == BlockType::PickaxeMold {
                    (
                        assets.pickaxe_fill_mesh.clone(),
                        MetalFillKind::PickaxeMold,
                    )
                } else {
                    (assets.ingot_fill_mesh.clone(), MetalFillKind::IngotMold)
                };
                let fill = commands.spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(assets.ingot_fill_materials[0].clone()),
                    Transform::from_xyz(pos.x as f32 + 0.5, pos.y as f32, pos.z as f32 + 0.5),
                    Visibility::Hidden,
                    MetalFill { pos, kind },
                )).id();
                models.push(fill);
            }
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
    pub crucible_scene: Handle<WorldAsset>,
    pub ingot_mold_scene: Handle<WorldAsset>,
    pub pickaxe_mold_scene: Handle<WorldAsset>,
    pub ingot_scene: Handle<WorldAsset>,
    pub ingot_fill_mesh: Handle<Mesh>,
    pub pickaxe_fill_mesh: Handle<Mesh>,
    pub crucible_fill_mesh: Handle<Mesh>,
    pub ingot_fill_materials: Vec<Handle<StandardMaterial>>,
    pub cast_ingot_materials: Vec<Handle<StandardMaterial>>,
}

#[derive(Component, Clone)]
pub struct CastIngotMaterialOverride(pub Handle<StandardMaterial>);

#[derive(Component, Clone)]
pub struct NamedMaterialOverride {
    pub node_name: &'static str,
    pub material: Handle<StandardMaterial>,
}

#[derive(Component, Clone)]
pub struct HiddenModelNode(pub &'static str);

#[derive(Clone, Copy)]
pub enum MetalFillKind {
    Crucible,
    IngotMold,
    PickaxeMold,
}

#[derive(Component)]
pub struct MetalFill {
    pub pos: IVec3,
    pub kind: MetalFillKind,
}

pub fn setup_campfire_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    const THERMAL_LEVELS: usize = 33;
    let mut noise_pixels = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32u32 {
        for x in 0..32u32 {
            let mut hash = x.wrapping_mul(0x9E37_79B9)
                ^ y.wrapping_mul(0x85EB_CA6B)
                ^ (x + y * 32).wrapping_mul(0xC2B2_AE35);
            hash ^= hash >> 16;
            hash = hash.wrapping_mul(0x7FEB_352D);
            hash ^= hash >> 15;
            let fine = (hash & 31) as u8;
            let coarse = (((x / 4) * 13 + (y / 4) * 7) & 15) as u8;
            let value = 205u8.saturating_add(fine).saturating_add(coarse);
            noise_pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let metal_noise = images.add(Image::new(
        bevy::render::render_resource::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        noise_pixels,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    ));
    let ingot_fill_materials = (0..THERMAL_LEVELS)
        .map(|level| {
            let heat = level as f32 / (THERMAL_LEVELS - 1) as f32;
            let hot_color = if heat < 0.55 {
                let t = heat / 0.55;
                [0.30 + 0.65 * t, 0.08 + 0.20 * t, 0.04]
            } else {
                let t = (heat - 0.55) / 0.45;
                [0.95 + 0.05 * t, 0.28 + 0.62 * t, 0.04 + 0.38 * t]
            };
            let emission = heat.powf(1.6) * 12.0;
            materials.add(StandardMaterial {
                base_color: Color::srgb(hot_color[0], hot_color[1], hot_color[2]),
                base_color_texture: Some(metal_noise.clone()),
                emissive: LinearRgba::rgb(
                    hot_color[0] * emission,
                    hot_color[1] * emission,
                    hot_color[2] * emission,
                ),
                emissive_texture: Some(metal_noise.clone()),
                metallic: 0.9,
                perceptual_roughness: 0.32,
                ..default()
            })
        })
        .collect();
    let cast_ingot_materials = [
        Color::srgb(0.72, 0.30, 0.12), // copper
        Color::srgb(0.42, 0.45, 0.48), // iron
        Color::srgb(0.58, 0.34, 0.12), // bronze
        Color::srgb(0.72, 0.57, 0.16), // brass
        Color::srgb(0.32, 0.36, 0.40), // steel
        Color::srgb(0.38, 0.28, 0.24), // mixed/impure
    ]
    .into_iter()
    .map(|base_color| {
        materials.add(StandardMaterial {
            base_color,
            base_color_texture: Some(metal_noise.clone()),
            metallic: 0.85,
            perceptual_roughness: 0.34,
            ..default()
        })
    })
    .collect();
    commands.insert_resource(BlockModelAssets {
        campfire_scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset("model/campfire.gltf")),
        light_bulb_scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset("model/light_blub.gltf")),
        crucible_scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset("model/crucible.gltf")),
        ingot_mold_scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset("model/ingot_mold.gltf")),
        pickaxe_mold_scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset("model/pickaxe_mold.gltf")),
        ingot_scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset("model/ingot.gltf")),
        ingot_fill_mesh: meshes.add(Cuboid::new(9.8 / 16.0, 1.0, 5.8 / 16.0)),
        pickaxe_fill_mesh: meshes.add(Cuboid::new(15.0 / 16.0, 1.0, 15.0 / 16.0)),
        crucible_fill_mesh: meshes.add(Cuboid::new(4.4 / 16.0, 1.0, 4.4 / 16.0)),
        ingot_fill_materials,
        cast_ingot_materials,
    });
}

pub fn apply_cast_ingot_materials(
    roots: Query<(Entity, &CastIngotMaterialOverride)>,
    children: Query<&Children>,
    mut mesh_materials: Query<&mut MeshMaterial3d<StandardMaterial>, With<Mesh3d>>,
) {
    for (root, material) in &roots {
        let mut pending = vec![root];
        while let Some(entity) = pending.pop() {
            if let Ok(mut mesh_material) = mesh_materials.get_mut(entity) {
                mesh_material.0 = material.0.clone();
            }
            if let Ok(entity_children) = children.get(entity) {
                pending.extend(entity_children.iter());
            }
        }
    }
}

pub fn apply_named_materials(
    roots: Query<(Entity, &NamedMaterialOverride)>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut mesh_materials: Query<&mut MeshMaterial3d<StandardMaterial>, With<Mesh3d>>,
) {
    for (root, override_data) in &roots {
        let mut pending = vec![(root, false)];
        while let Some((entity, inherited_match)) = pending.pop() {
            let matches_name = inherited_match
                || names.get(entity).is_ok_and(|name| name.as_str() == override_data.node_name);
            if matches_name {
                if let Ok(mut mesh_material) = mesh_materials.get_mut(entity) {
                    mesh_material.0 = override_data.material.clone();
                }
            }
            if let Ok(entity_children) = children.get(entity) {
                pending.extend(entity_children.iter().map(|child| (child, matches_name)));
            }
        }
    }
}

pub fn hide_named_model_nodes(
    roots: Query<(Entity, &HiddenModelNode)>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut visibility: Query<&mut Visibility>,
) {
    for (root, hidden) in &roots {
        let mut pending = vec![root];
        while let Some(entity) = pending.pop() {
            if names.get(entity).is_ok_and(|name| name.as_str() == hidden.0) {
                if let Ok(mut node_visibility) = visibility.get_mut(entity) {
                    *node_visibility = Visibility::Hidden;
                }
                continue;
            }
            if let Ok(entity_children) = children.get(entity) {
                pending.extend(entity_children.iter());
            }
        }
    }
}

pub fn update_ingot_mold_fill_system(
    world: Res<VoxelWorld>,
    assets: Res<BlockModelAssets>,
    mut fills: Query<(
        &MetalFill,
        &mut Transform,
        &mut Visibility,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    for (fill, mut transform, mut visibility, mut material) in &mut fills {
        let (mass, capacity, temperature, bottom, max_height) = match fill.kind {
            MetalFillKind::IngotMold => {
                let Some(data) = world.ingot_molds.get(&fill.pos) else {
                    *visibility = Visibility::Hidden;
                    continue;
                };
                (
                    data.total_mass(),
                    crate::chemistry::INGOT_MOLD_CAPACITY_GRAMS,
                    crate::thermodynamics::unpack_temperature(data.temp, data.temp_acc),
                    1.70 / 16.0,
                    3.10 / 16.0,
                )
            }
            MetalFillKind::PickaxeMold => {
                let Some(data) = world.ingot_molds.get(&fill.pos) else {
                    *visibility = Visibility::Hidden;
                    continue;
                };
                (
                    data.total_mass(),
                    crate::chemistry::PICKAXE_HEAD_MASS_GRAMS,
                    crate::thermodynamics::unpack_temperature(data.temp, data.temp_acc),
                    1.25 / 16.0,
                    1.75 / 16.0,
                )
            }
            MetalFillKind::Crucible => {
                let Some(data) = world.crucibles.get(&fill.pos) else {
                    *visibility = Visibility::Hidden;
                    continue;
                };
                (
                    data.liquid_mass.iter().copied().sum(),
                    crate::chemistry::CRUCIBLE_CAPACITY_GRAMS,
                    crate::thermodynamics::unpack_temperature(data.temp, data.temp_acc),
                    0.65 / 16.0,
                    4.35 / 16.0,
                )
            }
        };
        let ratio = mass as f32 / capacity as f32;
        if ratio <= 0.0 {
            *visibility = Visibility::Hidden;
            continue;
        }
        let height = max_height * ratio.clamp(0.0, 1.0);
        transform.translation.y = fill.pos.y as f32 + bottom + height * 0.5;
        transform.scale.y = height;
        // Below 100 C the fill is fully non-emissive. Above that point it
        // gradually brightens up to the hottest material preset.
        let heat = ((temperature - 100.0) / (1_600.0 - 100.0)).clamp(0.0, 1.0);
        let level = (heat * (assets.ingot_fill_materials.len() - 1) as f32).round() as usize;
        material.0 = assets.ingot_fill_materials[level].clone();
        *visibility = Visibility::Inherited;
    }
}

pub fn process_generated_chunks_system(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    mut generator: ResMut<ChunkGenerator>,
    mut meshes: ResMut<Assets<Mesh>>,
    render_materials: ChunkRenderMaterials,
    mesh_query: Query<&Mesh3d>,
    mut client_sync: Option<ResMut<crate::network::ClientSync>>,
    (mut active_fluids, mut active_reactive_fluids): (
        ResMut<ActiveFluids>,
        ResMut<ActiveReactiveFluids>,
    ),
    mut active_tnt: ResMut<ActiveTnt>,
    campfire_assets: Res<BlockModelAssets>,
) {
    let center = generator.stream_center;
    let distance2 = |p: IVec2| chunk_distance_squared(center, p);
    let keep_distance = generator.stream_keep_distance;
    let keep_distance2 = keep_distance.saturating_mul(keep_distance);

    // Drain all completed jobs first. They are cheap to hold and sorting them here
    // prevents a far-away slow job from deciding what becomes visible next.
    let mut drained_blocks = Vec::new();
    {
        let receiver = generator
            .receiver_blocks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while let Ok(block_data) = receiver.try_recv() {
            drained_blocks.push(block_data);
        }
    }
    generator.stats.block_jobs += drained_blocks.len() as u64;
    generator.stats.block_work_micros += drained_blocks
        .iter()
        .map(|data| data.work_micros)
        .sum::<u64>();
    generator.pending_blocks.extend(drained_blocks);
    generator.stats.max_pending_blocks =
        generator.stats.max_pending_blocks.max(generator.pending_blocks.len());
    generator
        .pending_blocks
        .sort_unstable_by_key(|data| std::cmp::Reverse(distance2(data.chunk_pos)));

    let block_started = Instant::now();
    let mut applied_blocks = 0usize;
    while applied_blocks < 8
        && (applied_blocks == 0 || block_started.elapsed() < Duration::from_micros(1_500))
    {
        let Some(block_data) = generator.pending_blocks.pop() else {
            break;
        };
        // ผลจากโลกรุ่นเก่า (ก่อน reset) — ทิ้งไปเลย ห้ามแตะ generating maps
        // เพราะอาจมี task รุ่นใหม่ของ chunk เดียวกันกำลังทำงานอยู่
        if block_data.version != generator.version {
            continue;
        }
        let chunk_pos = block_data.chunk_pos;
        if distance2(chunk_pos) > keep_distance2 {
            generator.generating_blocks.remove(&chunk_pos);
            generator.requested_at.remove(&chunk_pos);
            continue;
        }

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

        let (water_y_min, water_y_max) = block_data.water_bounds;

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
            light: Default::default(), block_light: Default::default(),
            light_dirty: true,
            light_revision: 1,
            light_missing_neighbors: 0,
            emitters: block_data.emitters,
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
                    c.light_revision = c.light_revision.wrapping_add(1);
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
            if let Some(chunk) = world.chunks.get(&chunk_pos) {
                let base_x = chunk_pos.x * CHUNK_WIDTH as i32;
                let base_z = chunk_pos.y * CHUNK_WIDTH as i32;
                chunk.blocks.for_each_matching(
                    |b| b.is_lava() || b.is_acid(),
                    |x, y, z, _| {
                        active_reactive_fluids.0.insert(IVec3::new(
                            base_x + x as i32,
                            y as i32,
                            base_z + z as i32,
                        ));
                    },
                );
            }
        }
        applied_blocks += 1;
    }
    generator.stats.block_integrate_micros += block_started.elapsed().as_micros() as u64;

    // Process Meshes
    let mut drained_meshes = Vec::new();
    {
        let receiver = generator
            .receiver_meshes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while let Ok(mesh_data) = receiver.try_recv() {
            drained_meshes.push(mesh_data);
        }
    }
    generator.stats.mesh_jobs += drained_meshes.len() as u64;
    generator.stats.mesh_work_micros += drained_meshes
        .iter()
        .map(|data| data.work_micros)
        .sum::<u64>();
    generator.pending_meshes.extend(drained_meshes);
    generator.stats.max_pending_meshes =
        generator.stats.max_pending_meshes.max(generator.pending_meshes.len());
    generator
        .pending_meshes
        .sort_unstable_by_key(|data| std::cmp::Reverse(distance2(data.chunk_pos)));

    let mesh_started = Instant::now();
    let mut applied_meshes = 0usize;
    while applied_meshes < 2
        && (applied_meshes == 0 || mesh_started.elapsed() < Duration::from_millis(3))
    {
        let Some(mesh_data) = generator.pending_meshes.pop() else {
            break;
        };
        if mesh_data.version != generator.version {
            continue;
        }
        let ChunkMeshData { chunk_pos, set, .. } = mesh_data;
        if distance2(chunk_pos) > keep_distance2 {
            generator.generating_meshes.remove(&chunk_pos);
            generator.requested_at.remove(&chunk_pos);
            continue;
        }
        let transform = Transform::from_xyz(
            (chunk_pos.x * CHUNK_WIDTH as i32) as f32,
            0.0,
            (chunk_pos.y * CHUNK_WIDTH as i32) as f32,
        );

        let num_vertices = set.total_vertices();
        let num_indices = set.total_indices();
        let num_water_vertices = set.water.positions.len();
        let num_water_indices = set.water.indices.len();
        let ChunkMeshSet {
            solid, water, glass, deco, seasonal_foliage, maple_foliage,
            glow, textured, block_overlay,
        } = set;

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
                MeshMaterial3d(render_materials.chunk.0.clone()),
            ));
        }
        let entity = chunk_entity.id();
        world.generated_chunks.insert(chunk_pos, entity);
        if let Some(requested_at) = generator.requested_at.remove(&chunk_pos) {
            if generator.stats.visible_latency_micros.len() >= 4096 {
                generator.stats.visible_latency_micros.remove(0);
            }
            generator
                .stats
                .visible_latency_micros
                .push(requested_at.elapsed().as_micros() as u64);
        }
        // overlay แสงโคม — entity แยก (track ใน block_light_chunks) แบบเดียวกับน้ำ/กระจก
        update_single_mesh_entity(&mut commands, &mut world.block_light_chunks, &mut meshes, &mesh_query, &render_materials.block_light.0, chunk_pos, block_overlay, transform);

        if !water.is_empty() {
            let water_entity = commands.spawn((
                Mesh3d(meshes.add(water.into_mesh())),
                MeshMaterial3d(render_materials.water.0.clone()),
                transform,
                NotShadowCaster,
                Block,
            )).id();
            world.water_chunks.insert(chunk_pos, water_entity);
        }

        if !glass.is_empty() {
            let glass_entity = commands.spawn((
                Mesh3d(meshes.add(glass.into_mesh())),
                MeshMaterial3d(render_materials.glass.0.clone()),
                transform,
                NotShadowCaster,
                Block,
            )).id();
            world.glass_chunks.insert(chunk_pos, glass_entity);
        }

        update_deco_entities(&mut commands, &mut world, &mut meshes, &render_materials.deco, &mesh_query, chunk_pos, deco, transform);
        update_single_mesh_entity(
            &mut commands,
            &mut world.seasonal_foliage_chunks,
            &mut meshes,
            &mesh_query,
            &render_materials.foliage.oak,
            chunk_pos,
            seasonal_foliage,
            transform,
        );
        update_single_mesh_entity(
            &mut commands,
            &mut world.maple_foliage_chunks,
            &mut meshes,
            &mesh_query,
            &render_materials.foliage.maple,
            chunk_pos,
            maple_foliage,
            transform,
        );

        update_glow_entities(&mut commands, &mut world, &mut meshes, &mesh_query, &render_materials.lamps, chunk_pos, glow, transform);
        update_textured_entities(&mut commands, &mut world, &mut meshes, &render_materials.blocks, &mesh_query, chunk_pos, textured, transform);
        refresh_chunk_lamp_lights(&mut commands, &mut world, chunk_pos);
        refresh_chunk_campfire_models(&mut commands, &mut world, chunk_pos, &campfire_assets);

        generator.generating_meshes.remove(&chunk_pos);
        applied_meshes += 1;
    }
    generator.stats.mesh_integrate_micros += mesh_started.elapsed().as_micros() as u64;
}

pub fn chunk_unloading_system(
    mut commands: Commands,
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut world: ResMut<VoxelWorld>,
    settings: Res<crate::GameSettings>,
    mut client_sync: Option<ResMut<crate::network::ClientSync>>,
    mut pools: ResMut<ActivePools>,
    time: Res<Time>,
    mut unload_credit: Local<f32>,
) {
    let Some(camera_transform) = camera_query.iter().next() else { return };
    let cam_pos = camera_transform.translation;

    let center_chunk_x = cam_pos.x.div_euclid(CHUNK_WIDTH as f32) as i32;
    let center_chunk_z = cam_pos.z.div_euclid(CHUNK_WIDTH as f32) as i32;

    // Unload นอกวงกลมรัศมี render distance + 2 (margin กัน load/unload กระพริบที่ขอบ)
    let unload_distance = settings.render_distance + 2;
    let unload_r2 = unload_distance * unload_distance;

    let is_out_of_range = |chunk_pos: IVec2| {
        let dx = chunk_pos.x - center_chunk_x;
        let dz = chunk_pos.y - center_chunk_z;
        dx * dx + dz * dz > unload_r2
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

    // Budget by elapsed time instead of frames. A fixed 2 chunks/frame falls further
    // behind as FPS drops, which retains more meshes and creates a feedback loop.
    // Keep a per-frame cap so a hitch cannot cause one very large despawn burst.
    const UNLOAD_CHUNKS_PER_SECOND: f32 = 192.0;
    const MAX_UNLOADS_PER_FRAME: usize = 8;
    *unload_credit =
        (*unload_credit + time.delta_secs() * UNLOAD_CHUNKS_PER_SECOND).min(32.0);
    let unload_budget = (*unload_credit as usize).min(MAX_UNLOADS_PER_FRAME);
    if unload_budget == 0 {
        return;
    }

    // HashMap iteration order is arbitrary. Prefer the farthest chunks and only
    // consume budget after a chunk is actually unloaded, so a pool waiting to
    // flush cannot starve every other candidate.
    to_unload.sort_unstable_by_key(|pos| {
        let dx = pos.x - center_chunk_x;
        let dz = pos.y - center_chunk_z;
        std::cmp::Reverse(dx * dx + dz * dz)
    });

    let mut unloaded = 0;
    for pos in to_unload {
        if unloaded >= unload_budget {
            break;
        }
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
        if let Some(entity) = world.block_light_chunks.remove(&pos) {
            commands.entity(entity).despawn();
        }
        if let Some(entities) = world.deco_chunks.remove(&pos) {
            for entity in entities { commands.entity(entity).despawn(); }
        }
        if let Some(entity) = world.seasonal_foliage_chunks.remove(&pos) {
            commands.entity(entity).despawn();
        }
        if let Some(entity) = world.maple_foliage_chunks.remove(&pos) {
            commands.entity(entity).despawn();
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
        unloaded += 1;
    }
    *unload_credit -= unloaded as f32;
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
        Self::survival_empty()
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

    pub fn try_add_item(&mut self, item: crate::item::Item, count: u32) -> bool {
        let max = max_stack(item);
        if let Some(stack) = self.slots.iter_mut().flatten().find(|stack| {
            stack.item == item && stack.count.unwrap_or(max) < max
        }) {
            let current = stack.count.unwrap_or(max);
            if current + count <= max {
                stack.count = Some(current + count);
                return true;
            }
        }
        if count <= max {
            if let Some(slot) = self.slots.iter_mut().find(|slot| slot.is_none()) {
                *slot = Some(ItemStack { item, count: Some(count) });
                return true;
            }
        }
        false
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
pub const PLACEABLE_ITEMS: [crate::item::Item; 75] = [
    crate::item::Item::Tool(crate::item::ToolType::Chisel),
    crate::item::Item::Tool(crate::item::ToolType::CopperWire),
    crate::item::Item::Tool(crate::item::ToolType::Pickaxe),
    crate::item::Item::Tool(crate::item::ToolType::Axe),
    crate::item::Item::Tool(crate::item::ToolType::Shovel),
    crate::item::Item::PickaxeHead(crate::chemistry::CastIngotData {
        mass: 1_000, composition: [1_000, 0, 0, 0, 0, 0, 0, 0],
        quality_permille: 1_000, kind: crate::chemistry::CastIngotKind::Copper,
    }),
    crate::item::Item::CraftedPickaxe(crate::chemistry::CastIngotData {
        mass: 1_000, composition: [1_000, 0, 0, 0, 0, 0, 0, 0],
        quality_permille: 1_000, kind: crate::chemistry::CastIngotKind::Copper,
    }),
    crate::item::Item::Material(crate::item::MaterialType::Copper),
    crate::item::Item::Material(crate::item::MaterialType::Iron),
    crate::item::Item::CastIngot(crate::chemistry::CastIngotData {
        mass: 1_000, composition: [1_000, 0, 0, 0, 0, 0, 0, 0],
        quality_permille: 1_000, kind: crate::chemistry::CastIngotKind::Copper,
    }),
    crate::item::Item::CastIngot(crate::chemistry::CastIngotData {
        mass: 1_000, composition: [0, 0, 0, 1_000, 0, 0, 0, 0],
        quality_permille: 1_000, kind: crate::chemistry::CastIngotKind::Iron,
    }),
    crate::item::Item::Material(crate::item::MaterialType::Coal),
    crate::item::Item::Material(crate::item::MaterialType::Stick),
    crate::item::Item::Material(crate::item::MaterialType::Slag),
    crate::item::Item::Material(crate::item::MaterialType::Limestone),
    crate::item::Item::Material(crate::item::MaterialType::Tin),
    crate::item::Item::Material(crate::item::MaterialType::Zinc),
    crate::item::Item::Material(crate::item::MaterialType::EmptyGlassBottle),
    crate::item::Item::Material(crate::item::MaterialType::SulfuricAcidBottle),
    crate::item::Item::CastIngot(crate::chemistry::CastIngotData {
        mass: 1_000, composition: [880, 120, 0, 0, 0, 0, 0, 0],
        quality_permille: 1_000, kind: crate::chemistry::CastIngotKind::Bronze,
    }),
    crate::item::Item::CastIngot(crate::chemistry::CastIngotData {
        mass: 1_000, composition: [700, 0, 300, 0, 0, 0, 0, 0],
        quality_permille: 1_000, kind: crate::chemistry::CastIngotKind::Brass,
    }),
    crate::item::Item::CastIngot(crate::chemistry::CastIngotData {
        mass: 1_000, composition: [0, 0, 0, 990, 10, 0, 0, 0],
        quality_permille: 1_000, kind: crate::chemistry::CastIngotKind::Steel,
    }),
    crate::item::Item::CastIngot(crate::chemistry::CastIngotData {
        mass: 1_000, composition: [800, 0, 0, 0, 0, 200, 0, 0],
        quality_permille: 400, kind: crate::chemistry::CastIngotKind::Mixed,
    }),
    crate::item::Item::Block(BlockType::Dirt), crate::item::Item::Block(BlockType::Grass),
    crate::item::Item::Block(BlockType::Stone), crate::item::Item::Block(BlockType::OakWood),
    crate::item::Item::Block(BlockType::Leaves), crate::item::Item::Block(BlockType::Sand),
    crate::item::Item::Block(BlockType::Water8), crate::item::Item::Block(BlockType::Glowstone),
    crate::item::Item::Block(BlockType::LampRed), crate::item::Item::Block(BlockType::LampGreen),
    crate::item::Item::Block(BlockType::LampBlue), crate::item::Item::Block(BlockType::Glass),
    crate::item::Item::Block(BlockType::TallGrass), crate::item::Item::Block(BlockType::Tnt),
    crate::item::Item::Block(BlockType::IronBlock), crate::item::Item::Block(BlockType::Nuke),
    crate::item::Item::Block(BlockType::SwitchOff), crate::item::Item::Block(BlockType::SmartLamp),
    crate::item::Item::Block(BlockType::Furnace), crate::item::Item::Block(BlockType::Chest),
    crate::item::Item::Block(BlockType::Campfire), crate::item::Item::Block(BlockType::Branch),
    crate::item::Item::Block(BlockType::SnowyGrass), crate::item::Item::Block(BlockType::Snow),
    crate::item::Item::Block(BlockType::SpruceLog), crate::item::Item::Block(BlockType::SpruceLogDamaged1), crate::item::Item::Block(BlockType::SpruceLogDamaged2), crate::item::Item::Block(BlockType::SpruceLeaves),
    crate::item::Item::Block(BlockType::MapleLog), crate::item::Item::Block(BlockType::MapleLeaves),
    crate::item::Item::Block(BlockType::MapleBranch),
    crate::item::Item::Block(BlockType::CopperOre), crate::item::Item::Block(BlockType::IronOre),
    crate::item::Item::Block(BlockType::CoalOre), crate::item::Item::Block(BlockType::TinOre),
    crate::item::Item::Block(BlockType::ZincOre), crate::item::Item::Block(BlockType::Limestone),
    crate::item::Item::Block(BlockType::Crucible),
    crate::item::Item::Block(BlockType::IngotMold),
    crate::item::Item::Block(BlockType::PickaxeMold),
    crate::item::Item::Block(BlockType::Basalt),
    crate::item::Item::Block(BlockType::VolcanicAsh),
    crate::item::Item::Block(BlockType::MagmaRock),
    crate::item::Item::Block(BlockType::Obsidian),
    crate::item::Item::Block(BlockType::AlteredRock),
    crate::item::Item::Block(BlockType::SulfurOre),
    crate::item::Item::Block(BlockType::Gypsum),
    crate::item::Item::Block(BlockType::LavaSource),
    crate::item::Item::Block(BlockType::SulfuricAcidSource),
    crate::item::Item::Block(BlockType::Ice),
    crate::item::Item::Block(BlockType::Gravel),
    crate::item::Item::Block(BlockType::Clay),
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

    if block == BlockType::Crucible {
        return commands.spawn((
            WorldAssetRoot(campfire_assets.crucible_scene.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(size)),
            layers,
        )).id();
    }

    if block == BlockType::IngotMold {
        return commands.spawn((
            WorldAssetRoot(campfire_assets.ingot_mold_scene.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(size)),
            layers,
        )).id();
    }

    if block == BlockType::PickaxeMold {
        return commands.spawn((
            WorldAssetRoot(campfire_assets.pickaxe_mold_scene.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(size)),
            layers,
        )).id();
    }

    if block == BlockType::CastIngot {
        return commands.spawn((
            WorldAssetRoot(campfire_assets.ingot_scene.clone()),
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
        let is_pickaxe = matches!(
            item,
            crate::item::Item::Tool(crate::item::ToolType::Pickaxe)
                | crate::item::Item::CraftedPickaxe(_)
        );
        let is_cast_metal = matches!(
            item,
            crate::item::Item::CastIngot(_) | crate::item::Item::PickaxeHead(_)
        );
        let is_block = match item {
            crate::item::Item::Block(crate::voxel::BlockType::TallGrass) => false,
            crate::item::Item::Block(_) => true,
            _ => false,
        };
        
        if !is_block && !is_pickaxe && !is_cast_metal {
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
            let entity = crate::item::spawn_item_visual(
                &mut commands,
                &mut meshes,
                &mut materials,
                &asset_server,
                &block_mats,
                &campfire_assets,
                item,
                1.0,
                Transform::from_translation(Vec3::ZERO),
            );
            commands.entity(entity).insert(render_layer.clone());
            entity
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
            commands.entity(entity).try_insert(layers.clone());
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
        Some(crate::item::Item::Crucible(_)) => BlockType::Crucible,
        Some(crate::item::Item::CastIngot(_)) => BlockType::CastIngot,
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
    let mut precise_normal = None;

    for _ in 0..50 {
        let dist = Vec3::new(map_x as f32 + 0.5, map_y as f32 + 0.5, map_z as f32 + 0.5).distance(origin);
        if dist > max_dist {
            break;
        }

        let block = world.get_block(map_x, map_y, map_z);
        if block != BlockType::Air && !block.is_water() {
            let block_pos = IVec3::new(map_x, map_y, map_z);
            let (local_min, local_max) = block_collision_box_at(&world, block_pos, block);
            let base = block_pos.as_vec3();
            if let Some((distance, normal)) =
                ray_aabb_hit(origin, dir, base + local_min, base + local_max)
            {
                if distance <= max_dist {
                    hit = true;
                    precise_normal = Some(normal);
                    break;
                }
            }
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

    let mut normal = precise_normal.unwrap_or(IVec3::ZERO);
    if normal == IVec3::ZERO {
        if side == 0 {
            normal.x = -step_x;
        } else if side == 1 {
            normal.y = -step_y;
        } else {
            normal.z = -step_z;
        }
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

    let (c_min, c_max) = block_collision_box_at(&world, IVec3::new(map_x, map_y, map_z), block);
    let transform_pos = |p: [f32; 3]| -> Vec3 {
        Vec3::new(
            if p[0] == 0.0 { c_min.x } else { c_max.x },
            if p[1] == 0.0 { c_min.y } else { c_max.y },
            if p[2] == 0.0 { c_min.z } else { c_max.z },
        )
    };

    let p0 = block_pos + transform_pos(positions[0]) + offset;
    let p1 = block_pos + transform_pos(positions[1]) + offset;
    let p2 = block_pos + transform_pos(positions[2]) + offset;
    let p3 = block_pos + transform_pos(positions[3]) + offset;

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
    pub foliage_material: Res<'w, SeasonalFoliageMaterialHandles>,
    pub lamp_materials: Res<'w, LampMaterials>,
    pub block_materials: Res<'w, BlockMaterials>,
    pub block_light_material: Res<'w, BlockLightMaterial>,
}

/// ระยะที่ใบยังเกาะกิ่งอยู่ได้ (Chebyshev) — กว้างกว่ารัศมีพุ่มที่ scatter_leaves โปรย
/// ไว้เล็กน้อย ใบที่อยู่ในระยะนี้จากกิ่งใดก็ตามถือว่ายังมีที่ยึด
const LEAF_SUPPORT_RANGE: i32 = 3;

/// กิ่ง/ท่อนสนที่ `p` หายไป — จ่อใบรอบๆ ไว้ให้ไปเช็คว่ายังมีที่ยึดอยู่ไหม
/// (ทั้งใบไม้ปกติ Leaves และใบสน SpruceLeaves)
pub fn queue_leaf_decay_around(world: &mut VoxelWorld, p: IVec3) {
    for d in crate::tree::NEIGHBOUR_DIRS {
        let n = p + d;
        let b = world.get_block(n.x, n.y, n.z);
        if matches!(b, BlockType::Leaves | BlockType::MapleLeaves | BlockType::SpruceLeaves) {
            world.pending_leaf_decay.insert(n);
        }
    }
}

/// บล็อกที่ค้ำใบชนิดนี้ให้ไม่ร่วง — ใบไม้เกาะกิ่ง Branch, ใบสนเกาะท่อน SpruceLog
fn leaf_support_block(leaf: BlockType) -> BlockType {
    match leaf {
        BlockType::SpruceLeaves => BlockType::SpruceLog,
        BlockType::MapleLeaves => BlockType::MapleBranch,
        _ => BlockType::Branch,
    }
}

/// ยังมีบล็อกที่ค้ำ (`support`) อยู่ในระยะเกาะของใบที่ `p` ไหม
fn leaf_has_support(world: &VoxelWorld, p: IVec3, support: BlockType) -> bool {
    let r = LEAF_SUPPORT_RANGE;
    for dy in -r..=r {
        for dz in -r..=r {
            for dx in -r..=r {
                let q = p + IVec3::new(dx, dy, dz);
                if world.get_block(q.x, q.y, q.z) == support {
                    return true;
                }
            }
        }
    }
    false
}

/// ท่อนสนที่ `o` ยังมีที่ยึดพื้นไหม — ยึดได้ถ้าท่อนล่างเป็น SpruceLog (ไล่ลงถึงพื้น)
/// หรือเป็นบล็อกพื้นแข็ง (ดิน/หญ้า/หิน ฯลฯ ที่ไม่ใช่ใบ/กิ่ง)
fn spruce_log_supported(world: &VoxelWorld, o: IVec3) -> bool {
    let below = world.get_block(o.x, o.y - 1, o.z);
    below == BlockType::SpruceLog
        || (block_def(below).solid
            && !matches!(
                below,
                BlockType::Leaves | BlockType::MapleLeaves | BlockType::SpruceLeaves
                    | BlockType::Branch | BlockType::MapleBranch
            ))
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
        if !matches!(
            world.get_block(adj.x, adj.y, adj.z),
            BlockType::Branch | BlockType::MapleBranch
        ) {
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
                // บล็อกเปลี่ยน = แสงรอบๆ อาจเปลี่ยน
                // sky light: กระทบแค่ chunk ที่ติดขอบ (edit_affected_chunks)
                // block light: โคมมีระยะ 15 = ข้ามขอบ chunk ได้ — ปลุกเพื่อนบ้านทั้ง 8 ทิศ
                let emitter_changed =
                    crate::light::emitter_rgb(old_block) != [0, 0, 0]
                    || crate::light::emitter_rgb(new_block) != [0, 0, 0];
                let affected = if emitter_changed {
                    let edited_chunk = IVec2::new(
                        x.div_euclid(CHUNK_WIDTH as i32),
                        z.div_euclid(CHUNK_WIDTH as i32),
                    );
                    let mut all = vec![edited_chunk];
                    all.extend(chunk_neighbors(edited_chunk));
                    all
                } else {
                    edit_affected_chunks(p)
                };
                for cp in affected {
                    if let Some(c) = world.chunks.get_mut(&cp) {
                        c.light_dirty = true;
                        c.light_revision = c.light_revision.wrapping_add(1);
                    }
                }
                // node ของกิ่งเกิด/ตายที่นี่ที่เดียว — path นี้รันทั้ง host และ client
                // จาก edit ก้อนเดียวกัน สถานะ network สองฝั่งจึงตรงกันเสมอ
                // (set_block คืน true แค่บอกว่า chunk โหลดอยู่ ไม่ได้แปลว่าบล็อกเปลี่ยน —
                //  ต้องเทียบ old/new เอง ไม่งั้น edit ซ้ำจะ re-parent กิ่งเดิมแล้วกิ่งลูกร่วงฟรี)
                if new_block != old_block {
                    if matches!(new_block, BlockType::Branch | BlockType::MapleBranch) {
                        attach_branch_node(world, p);
                    } else if new_block == BlockType::Crucible {
                        world.crucibles.insert(p, crate::chemistry::CrucibleData::default());
                    } else if matches!(new_block, BlockType::IngotMold | BlockType::PickaxeMold) {
                        world.ingot_molds.insert(p, crate::chemistry::IngotMoldData::default());
                    } else if matches!(old_block, BlockType::Branch | BlockType::MapleBranch) {
                        let orphans = world.branch_network.detach(p);
                        world.pending_branch_orphans.extend(orphans);
                        queue_leaf_decay_around(world, p);
                    } else if (old_block == BlockType::SpruceLog || old_block == BlockType::SpruceLogDamaged1 || old_block == BlockType::SpruceLogDamaged2) && new_block == BlockType::Air {
                        // ท่อนบนขาดที่ยึด → เข้าคิว cascade + ใบรอบๆ อาจร่วงตาม
                        world.pending_spruce_orphans.insert(p + IVec3::Y);
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
            let old_block = world.get_block(x, y, z);
            if world.set_block(x, y, z, bt) {
                world.set_block_facing(x, y, z, *facing);
                let p = IVec3::new(x, y, z);
                let emitter_changed =
                    crate::light::emitter_rgb(old_block) != [0, 0, 0]
                    || crate::light::emitter_rgb(bt) != [0, 0, 0];
                let affected = if emitter_changed {
                    let edited_chunk = IVec2::new(
                        x.div_euclid(CHUNK_WIDTH as i32),
                        z.div_euclid(CHUNK_WIDTH as i32),
                    );
                    let mut all = vec![edited_chunk];
                    all.extend(chunk_neighbors(edited_chunk));
                    all
                } else {
                    edit_affected_chunks(p)
                };
                for cp in affected {
                    if let Some(c) = world.chunks.get_mut(&cp) {
                        c.light_dirty = true;
                        c.light_revision = c.light_revision.wrapping_add(1);
                    }
                }
                Some(p)
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
        BlockEdit::PlaceContainerBlock { pos, block, contents, crucible_data } => {
            block_edit::place_container(world, *pos, *block, contents, *crucible_data)
        }
        BlockEdit::SetContainerSlot { pos, slot, item } => {
            block_edit::set_container_slot(world, *pos, *slot, *item)
        }
        BlockEdit::AddFurnaceAir { pos } => {
            block_edit::add_furnace_air(world, *pos)
        }
        BlockEdit::SetIngotMold { pos, data } => {
            let p = IVec3::from_array(*pos);
            if !matches!(world.get_block(p.x, p.y, p.z), BlockType::IngotMold | BlockType::PickaxeMold)
                || data.total_mass() > crate::chemistry::INGOT_MOLD_CAPACITY_GRAMS
            {
                return None;
            }
            world.ingot_molds.insert(p, *data);
            Some(p)
        }
        BlockEdit::TakeIngotMold { pos } => {
            let p = IVec3::from_array(*pos);
            let mold = world.ingot_molds.get(&p)?;
            let block = world.get_block(p.x, p.y, p.z);
            let ready = if block == BlockType::PickaxeMold {
                crate::chemistry::cast_pickaxe_head_from_mold(mold).is_some()
            } else {
                crate::chemistry::mold_ready_to_extract(mold)
            };
            if !matches!(block, BlockType::IngotMold | BlockType::PickaxeMold) || !ready
            {
                return None;
            }
            world.ingot_molds.insert(p, crate::chemistry::IngotMoldData::default());
            Some(p)
        }
        BlockEdit::PlaceCastIngot { pos, data } => {
            let p = IVec3::from_array(*pos);
            if data.mass == 0
                || data.mass > crate::chemistry::INGOT_MOLD_CAPACITY_GRAMS
                || data.composition.iter().copied().sum::<u32>() != data.mass
                || world.get_block(p.x, p.y, p.z) != BlockType::Air
                || !world.set_block(p.x, p.y, p.z, BlockType::CastIngot)
            {
                return None;
            }
            world.placed_ingots.insert(p, *data);
            Some(p)
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
    breaking_target: Option<(IVec3, f32)>,
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

            let s = create_mesh_from_blocks(chunk_pos, &chunk_data.blocks, &neighbors, Some(&chunk_data.chiseled_blocks), Some(&chunk_data.facings), Some(&world.branch_network), light.as_ref(), breaking_target);
            chunk_data.num_vertices = s.total_vertices();
            chunk_data.num_indices = s.total_indices();
            chunk_data.num_water_vertices = s.water.positions.len();
            chunk_data.num_water_indices = s.water.indices.len();
            set = s;
        }

        world.total_vertices = (world.total_vertices + set.total_vertices()) - old_vertices;
        world.total_indices = (world.total_indices + set.total_indices()) - old_indices;
        let ChunkMeshSet {
            solid, water, glass, deco, seasonal_foliage, maple_foliage,
            glow, textured, block_overlay,
        } = set;

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
        update_single_mesh_entity(
            commands,
            &mut world.seasonal_foliage_chunks,
            &mut mp.meshes,
            &mp.mesh_query,
            &mp.foliage_material.oak,
            chunk_pos,
            seasonal_foliage,
            transform,
        );
        update_single_mesh_entity(
            commands,
            &mut world.maple_foliage_chunks,
            &mut mp.meshes,
            &mp.mesh_query,
            &mp.foliage_material.maple,
            chunk_pos,
            maple_foliage,
            transform,
        );
        update_glow_entities(commands, world, &mut mp.meshes, &mp.mesh_query, &mp.lamp_materials, chunk_pos, glow, transform);
        update_textured_entities(commands, world, &mut mp.meshes, &mp.block_materials, &mp.mesh_query, chunk_pos, textured, transform);
        update_single_mesh_entity(commands, &mut world.block_light_chunks, &mut mp.meshes, &mp.mesh_query, &mp.block_light_material.0, chunk_pos, block_overlay, transform);
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
    (mut active_fluids, mut active_reactive_fluids): (
        ResMut<ActiveFluids>,
        ResMut<ActiveReactiveFluids>,
    ),
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
        if place_pressed
            && hit.block.is_acid()
            && hotbar.slots[hotbar.selected].is_some_and(|stack| {
                stack.item == crate::item::Item::Material(crate::item::MaterialType::EmptyGlassBottle)
            })
        {
            let selected_slot = hotbar.selected;
            let stack = hotbar.slots[selected_slot].expect("checked above");
            let acid_bottle = crate::item::Item::Material(
                crate::item::MaterialType::SulfuricAcidBottle,
            );
            let stored = match stack.count {
                None => {
                    hotbar.slots[selected_slot] = Some(ItemStack {
                        item: acid_bottle,
                        count: None,
                    });
                    true
                }
                Some(1) => {
                    hotbar.slots[selected_slot] = Some(ItemStack {
                        item: acid_bottle,
                        count: Some(1),
                    });
                    true
                }
                Some(count) if hotbar.try_add_item(acid_bottle, 1) => {
                    hotbar.slots[selected_slot] = Some(ItemStack {
                        item: stack.item,
                        count: Some(count - 1),
                    });
                    true
                }
                _ => false,
            };
            if stored {
                let remaining = take_one_unit(hit.block);
                edit = Some(BlockEdit::SetBlock {
                    pos: hit.pos.to_array(),
                    block: remaining as u8,
                });
                active_reactive_fluids.0.insert(hit.pos);
            }
        } else if place_pressed && matches!(hit.block, BlockType::Tnt | BlockType::Nuke) {
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
        } else if place_pressed
            && matches!(hit.block, BlockType::IngotMold | BlockType::PickaxeMold)
            && hotbar.slots[hotbar.selected]
                .is_some_and(|stack| matches!(stack.item, crate::item::Item::Crucible(_)))
        {
            let selected_slot = hotbar.selected;
            if let Some(stack) = hotbar.slots[selected_slot].as_mut() {
                if let crate::item::Item::Crucible(mut crucible) = stack.item {
                    let mold = world.ingot_molds.entry(hit.pos).or_default();
                    if crate::chemistry::pour_crucible_into_mold(&mut crucible, mold).is_ok() {
                        stack.item = crate::item::Item::Crucible(crucible);
                        edit = Some(BlockEdit::SetIngotMold {
                            pos: hit.pos.to_array(),
                            data: *mold,
                        });
                    }
                }
            }
        } else if break_pressed || hold_mining {
            // ของที่ถืออยู่ — ใช้ทั้งคิดความเร็วขุดและกติกา drop (Survival)
            let held_tool = match hotbar.slots[hotbar.selected].map(|s| s.item) {
                Some(crate::item::Item::Tool(t)) => Some(t),
                Some(crate::item::Item::CraftedPickaxe(_)) => Some(crate::item::ToolType::Pickaxe),
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
                    if matches!(hit.block, BlockType::Branch | BlockType::MapleBranch) {
                        world.pending_branch_remesh.extend(edit_affected_chunks(hit.pos));
                    } else if hit.block == BlockType::SpruceLog && progress > 0.33 {
                        let e = BlockEdit::SetBlock { pos: hit.pos.to_array(), block: BlockType::SpruceLogDamaged1 as u8 };
                        if let Some(tp) = apply_block_edit(&mut world, &e) {
                            remesh_chunks(&mut commands, &mut world, &mut mp, None, edit_affected_chunks(tp));
                            if net_server.is_some() || net_client.is_some() {
                                net_out.0.push_back((None, e));
                            }
                        }
                    } else if hit.block == BlockType::SpruceLogDamaged1 && progress > 0.66 {
                        let e = BlockEdit::SetBlock { pos: hit.pos.to_array(), block: BlockType::SpruceLogDamaged2 as u8 };
                        if let Some(tp) = apply_block_edit(&mut world, &e) {
                            remesh_chunks(&mut commands, &mut world, &mut mp, None, edit_affected_chunks(tp));
                            if net_server.is_some() || net_client.is_some() {
                                net_out.0.push_back((None, e));
                            }
                        }
                    }
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
            // Crucible is a stateful vessel: always return it with its contents in both
            // Creative and Survival. Losing it in Creative also destroys molten metal.
            if hit.block == BlockType::Crucible {
                let crucible_data = world.crucibles.get(&hit.pos).copied().unwrap_or_default();
                spawn_events.write(crate::item::SpawnDroppedItemEvent {
                    item: crate::item::Item::Crucible(crucible_data),
                    pos: hit.pos.as_vec3() + Vec3::new(0.5, 0.5, 0.5),
                    velocity: Vec3::new(
                        (fastrand::f32() - 0.5) * 4.0,
                        2.0 + fastrand::f32() * 3.0,
                        (fastrand::f32() - 0.5) * 4.0,
                    ),
                });
            } else if hit.block == BlockType::CastIngot {
                if let Some(ingot) = world.placed_ingots.get(&hit.pos).copied() {
                    spawn_events.write(crate::item::SpawnDroppedItemEvent {
                        item: crate::item::Item::CastIngot(ingot),
                        pos: hit.pos.as_vec3() + Vec3::new(0.5, 0.3, 0.5),
                        velocity: Vec3::new(
                            (fastrand::f32() - 0.5) * 4.0,
                            2.0 + fastrand::f32() * 3.0,
                            (fastrand::f32() - 0.5) * 4.0,
                        ),
                    });
                }
            } else if survival && drops_item {
                let dropped_item = match hit.block {
                    BlockType::CopperOre => crate::item::Item::Material(crate::item::MaterialType::Copper),
                    BlockType::IronOre => crate::item::Item::Material(crate::item::MaterialType::Iron),
                    BlockType::CoalOre => crate::item::Item::Material(crate::item::MaterialType::Coal),
                    BlockType::TinOre => crate::item::Item::Material(crate::item::MaterialType::Tin),
                    BlockType::ZincOre => crate::item::Item::Material(crate::item::MaterialType::Zinc),
                    BlockType::Limestone => crate::item::Item::Material(crate::item::MaterialType::Limestone),
                    BlockType::SpruceLogDamaged1 | BlockType::SpruceLogDamaged2 => crate::item::Item::Block(BlockType::SpruceLog),
                    _ => crate::item::Item::Block(hit.block),
                };
                spawn_events.write(crate::item::SpawnDroppedItemEvent {
                    item: dropped_item,
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
            if matches!(current, BlockType::IngotMold | BlockType::PickaxeMold) {
                let cast = world.ingot_molds.get(&hit.pos).and_then(|mold| {
                    if current == BlockType::PickaxeMold {
                        crate::chemistry::cast_pickaxe_head_from_mold(mold)
                            .map(crate::item::Item::PickaxeHead)
                    } else {
                        crate::chemistry::cast_ingot_from_mold(mold)
                            .map(crate::item::Item::CastIngot)
                    }
                });
                if let Some(item) = cast {
                    spawn_events.write(crate::item::SpawnDroppedItemEvent {
                        item,
                        pos: hit.pos.as_vec3() + Vec3::new(0.5, 0.6, 0.5),
                        velocity: Vec3::new(0.0, 2.0, 0.0),
                    });
                    edit = Some(BlockEdit::TakeIngotMold {
                        pos: hit.pos.to_array(),
                    });
                }
            } else if current == BlockType::SwitchOff {
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
            } else if matches!(current, BlockType::Furnace | BlockType::Chest | BlockType::Crucible) {
                if current == BlockType::Crucible {
                    if let Some(crate::item::Item::Tool(crate::item::ToolType::SlagSkimmer)) = hotbar.slots[hotbar.selected].map(|s| s.item) {
                        if let Some(crucible) = world.crucibles.get_mut(&hit.pos) {
                            let slag = crucible.liquid_mass[crate::chemistry::Element::Slag as usize];
                            if slag > 0 {
                                crucible.liquid_mass[crate::chemistry::Element::Slag as usize] = 0;
                                spawn_events.write(crate::item::SpawnDroppedItemEvent {
                                    item: crate::item::Item::Material(crate::item::MaterialType::Slag),
                                    pos: hit.pos.as_vec3() + Vec3::new(0.5, 1.0, 0.5),
                                    velocity: Vec3::new(0.0, 2.0, 0.0),
                                });
                            }
                        }
                        return;
                    }
                }
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
                    let (local_min, local_max) = block_collision_box(selected.0);
                    let bmin = p.as_vec3() + local_min;
                    let bmax = p.as_vec3() + local_max;
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
                } else if selected.0 == BlockType::Crucible {
                    let mut crucible_data_opt = None;
                    if let Some(crate::item::Item::Crucible(crucible)) = hotbar.slots[hotbar.selected].as_ref().map(|s| s.item) {
                        crucible_data_opt = Some(crucible);
                    }
                    BlockEdit::PlaceContainerBlock {
                        pos: p.to_array(),
                        block: selected.0 as u8,
                        contents: vec![],
                        crucible_data: crucible_data_opt,
                    }
                } else if selected.0 == BlockType::CastIngot {
                    let Some(crate::item::Item::CastIngot(data)) =
                        hotbar.slots[hotbar.selected].map(|stack| stack.item)
                    else {
                        return;
                    };
                    BlockEdit::PlaceCastIngot {
                        pos: p.to_array(),
                        data,
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
        active_reactive_fluids.0.insert(tp);
        block_updates.0.insert(tp);
        for dir in [IVec3::new(1,0,0), IVec3::new(-1,0,0), IVec3::new(0,1,0), IVec3::new(0,-1,0), IVec3::new(0,0,1), IVec3::new(0,0,-1)] {
            active_fluids.0.insert(tp + dir);
            active_reactive_fluids.0.insert(tp + dir);
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

    remesh_chunks(&mut commands, &mut world, &mut mp, None, edit_affected_chunks(tp));

    // บล็อกเปลี่ยนเฉพาะใน chunk ที่แก้ — อัปเดต PointLight/โมเดล Campfire เฉพาะตรงนั้น
    refresh_chunk_lamp_lights(&mut commands, &mut world, edited_chunk);
    refresh_chunk_campfire_models(&mut commands, &mut world, edited_chunk, &campfire_assets);
}


#[derive(Resource, Default)]
pub struct ActiveFluids(pub std::collections::HashSet<IVec3>);

#[derive(Resource, Default)]
pub struct ActiveReactiveFluids(pub std::collections::HashSet<IVec3>);

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
    remesh_chunks(&mut commands, &mut world, &mut mp, None, remesh);
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
    let sender = jobs
        .sender
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
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
        let res = {
            jobs.receiver
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .try_recv()
        };
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
                remesh_chunks(&mut commands, &mut world, &mut mp, None, edit_affected_chunks(*p));
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
            remesh_chunks(&mut commands, &mut world, &mut mp, None, remesh);
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

/// debug: วาดกริดขอบเขต chunk รอบตัวผู้เล่น (สลับด้วย /chunkborders) —
/// เส้นตั้งที่จุดตัดกริด (chunk ปัจจุบัน = เหลือง, รอบข้าง = ฟ้า) + ฝากริดบน/ล่าง
pub fn chunk_border_gizmo_system(
    settings: Res<crate::GameSettings>,
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut gizmos: Gizmos,
) {
    if !settings.show_chunk_borders {
        return;
    }
    let Some(t) = camera_query.iter().next() else { return };
    let p = t.translation;
    let w = CHUNK_WIDTH as f32;
    let cx = (p.x / w).floor() as i32;
    let cz = (p.z / w).floor() as i32;
    let r = 2; // รัศมี chunk รอบตัว
    let y_lo = (p.y - 48.0).max(0.0);
    let y_hi = (p.y + 48.0).min(CHUNK_HEIGHT as f32);
    let cur = Color::srgb(1.0, 0.9, 0.2); // chunk ที่ยืนอยู่ = เหลือง
    let grid = Color::srgba(0.3, 0.75, 1.0, 0.6); // รอบข้าง = ฟ้า

    // เส้นตั้งที่จุดตัดของกริด chunk
    for gx in (cx - r)..=(cx + r + 1) {
        for gz in (cz - r)..=(cz + r + 1) {
            let x = gx as f32 * w;
            let z = gz as f32 * w;
            let is_current_corner = (gx == cx || gx == cx + 1) && (gz == cz || gz == cz + 1);
            let c = if is_current_corner { cur } else { grid };
            gizmos.line(Vec3::new(x, y_lo, z), Vec3::new(x, y_hi, z), c);
        }
    }

    // ฝากริดแนวนอนบน/ล่าง ให้เห็นเป็นช่อง chunk ชัด
    let x_min = (cx - r) as f32 * w;
    let x_max = (cx + r + 1) as f32 * w;
    let z_min = (cz - r) as f32 * w;
    let z_max = (cz + r + 1) as f32 * w;
    for &y in &[y_lo, y_hi] {
        for gx in (cx - r)..=(cx + r + 1) {
            let x = gx as f32 * w;
            gizmos.line(Vec3::new(x, y, z_min), Vec3::new(x, y, z_max), grid);
        }
        for gz in (cz - r)..=(cz + r + 1) {
            let z = gz as f32 * w;
            gizmos.line(Vec3::new(x_min, y, z), Vec3::new(x_max, y, z), grid);
        }
    }
}

/// debug: วาดลูกศร 3D แสดงทิศทางและความเร็วการไหลของน้ำรอบตัวผู้เล่น (สลับด้วย /waterflow)
pub fn water_flow_gizmo_system(
    settings: Res<crate::GameSettings>,
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    world: Res<VoxelWorld>,
    mut gizmos: Gizmos,
) {
    if !settings.show_water_flow {
        return;
    }
    let Some(t) = camera_query.iter().next() else { return };
    let p = t.translation;
    let px = p.x.floor() as i32;
    let py = p.y.floor() as i32;
    let pz = p.z.floor() as i32;
    let radius = 24;
    let debug_color = Color::srgb(1.0, 0.15, 0.15); // Red

    for dx in -radius..=radius {
        for dz in -radius..=radius {
            let wx = px + dx;
            let wz = pz + dz;
            let (flow, speed) = crate::hydro::river_at(wx as f64 + 0.5, wz as f64 + 0.5)
                .map(|r| (r.flow, r.speed))
                .unwrap_or((Vec2::ZERO, 0.0));

            let mut water_y = None;
            for y in (py - 20).max(1)..(py + 20).min(CHUNK_HEIGHT as i32 - 1) {
                let b = world.get_block(wx, y, wz);
                let above = world.get_block(wx, y + 1, wz);
                if b.is_water() && !above.is_water() {
                    water_y = Some(y);
                    break;
                }
            }

            let wy_i = match water_y {
                Some(y) => y,
                None => continue,
            };

            let sample = |x, y, z| world.get_block(x, y, z);
            let d0 = water_corner_info(&sample, wx, wy_i, wz + 1).0;
            let d1 = water_corner_info(&sample, wx + 1, wy_i, wz + 1).0;
            let d2 = water_corner_info(&sample, wx + 1, wy_i, wz).0;
            let d3 = water_corner_info(&sample, wx, wy_i, wz).0;

            let slope_x = (d1 + d2 - d0 - d3) * 0.5;
            let slope_z = (d0 + d1 - d2 - d3) * 0.5;

            let final_flow_x = flow.x * speed + slope_x * 2.0;
            let final_flow_z = flow.y * speed + slope_z * 2.0;
            let final_speed = (final_flow_x * final_flow_x + final_flow_z * final_flow_z).sqrt();

            if final_speed <= 0.001 {
                continue;
            }

            let slope_mag = (slope_x * slope_x + slope_z * slope_z).sqrt();
            let tilt_y = -slope_mag * 1.2; // Point downwards on slopes
            
            let dir = Vec3::new(final_flow_x / final_speed, tilt_y, final_flow_z / final_speed).normalize();

            // Calculate exact visual center height for the arrow origin
            let avg_drop = (d0 + d1 + d2 + d3) * 0.25;
            let surface_y = wy_i as f32 + 1.0 - avg_drop + 0.05;

            let start = Vec3::new(wx as f32 + 0.5, surface_y, wz as f32 + 0.5);
            let arrow_len = (0.4 + final_speed * 0.2).min(1.2);
            let end = start + dir * arrow_len;

            gizmos.line(start, end, debug_color);
            // Construct arrow head
            let right = dir.cross(Vec3::Y).normalize_or_zero();
            let up_local = right.cross(dir).normalize_or_zero();
            let head_back = end - dir * 0.25;
            gizmos.line(end, head_back + right * 0.15 + up_local * 0.1, debug_color);
            gizmos.line(end, head_back - right * 0.15 + up_local * 0.1, debug_color);
        }
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
/// คอลัมน์นี้เป็นแม่น้ำ gen ไหม — น้ำแม่น้ำเป็น scenery นิ่งถาวร fluid sim ไม่แตะ (ไม่งั้นไหลลงทะเลจนแห้ง
/// เพราะ finite ไม่มี source). ตรวจจาก hydro network ตรง ๆ (ไม่ต้องมี block type แยก)
#[inline]
pub fn is_river_column(x: i32, z: i32) -> bool {
    crate::hydro::river_at(x as f64 + 0.5, z as f64 + 0.5).is_some()
}

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

                // ฝั่งเราไหลไปหาเขาได้ไหม (เว้นน้ำแม่น้ำ = นิ่งถาวร)
                if a.is_water() && (b == BlockType::Air || (b.is_water() && bv + 1 < av)) {
                    let (wx, wz) = (base_x + alx as i32, base_z + alz as i32);
                    if !is_river_column(wx, wz) {
                        active_fluids.0.insert(IVec3::new(wx, y as i32, wz));
                    }
                }
                // ฝั่งเขาไหลมาหาเราได้ไหม
                if b.is_water() && (a == BlockType::Air || (a.is_water() && av + 1 < bv)) {
                    let (wx, wz) = (n_base_x + blx as i32, n_base_z + blz as i32);
                    if !is_river_column(wx, wz) {
                        active_fluids.0.insert(IVec3::new(wx, y as i32, wz));
                    }
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
        // น้ำแม่น้ำ gen = นิ่งถาวร ไม่ simulate (ปล่อยให้ไหลจะแห้งเพราะ finite ไม่มี source)
        if is_river_column(pos.x, pos.z) {
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
        world.pending_spruce_orphans.clear();
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
        if matches!(block, BlockType::Branch | BlockType::MapleBranch)
            && !world.branch_network.nodes.contains_key(&p)
        {
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
        let orphan_block = world.get_block(o.x, o.y, o.z);
        if !matches!(orphan_block, BlockType::Branch | BlockType::MapleBranch) {
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
            item: crate::item::Item::Block(orphan_block),
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

    // --- Cascade ท่อนสนที่ขาดที่ยึด (คิวบ์ล้วน ไม่มี BranchNetwork คุม) ---
    // ท่อนที่ท่อนล่างหายไปแล้วไม่มีพื้นค้ำ → ทุบทิ้ง ดรอปเป็นไอเทม แล้วส่งท่อนบนต่อคิว
    // → ต้นล้มทีละท่อนขึ้นไปเหมือนต้นกิ่ง (ตัดโคน = ล้มทั้งต้น)
    let spruce_orphans: Vec<IVec3> = world.pending_spruce_orphans.drain().collect();
    for o in spruce_orphans {
        let ocp = crate::tree::chunk_of(o, CHUNK_WIDTH as i32);
        if !world.chunks.contains_key(&ocp) {
            continue; // chunk ไม่โหลด — ปล่อยค้าง ผู้เล่นไปทุบเอง
        }
        if world.get_block(o.x, o.y, o.z) != BlockType::SpruceLog {
            continue; // ท่อนหายไปทางอื่นแล้ว
        }
        if spruce_log_supported(&world, o) {
            continue; // ยังมีพื้น/ท่อนล่างค้ำ ไม่ล้ม
        }
        world.set_block(o.x, o.y, o.z, BlockType::Air);
        world.pending_spruce_orphans.insert(o + IVec3::Y); // ท่อนถัดขึ้นไปเช็คต่อ
        queue_leaf_decay_around(&mut world, o);
        spawn_events.write(crate::item::SpawnDroppedItemEvent {
            item: crate::item::Item::Block(BlockType::SpruceLog),
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

    // --- ใบร่วงเมื่อกิ่ง/ท่อนที่เกาะอยู่หายไป (ทั้ง Leaves และ SpruceLeaves) ---
    // จำกัดจำนวนต่อเฟรมเพราะการเช็คที่ยึดต้องสแกนกล่อง 7×7×7 ต่อใบหนึ่งใบ —
    // ตัดต้นใหญ่ทีเดียวมีใบหลายร้อย ถ้าทำรวดเดียวจะกระตุก (และการทยอยร่วงก็ดูดีกว่า)
    const LEAF_DECAY_PER_FRAME: usize = 48;
    let mut leaves: Vec<IVec3> = world.pending_leaf_decay.iter().copied().collect();
    leaves.sort_unstable_by_key(|p| p.to_array()); // deterministic ระหว่าง host/client
    leaves.truncate(LEAF_DECAY_PER_FRAME);
    for l in leaves {
        world.pending_leaf_decay.remove(&l);
        let leaf = world.get_block(l.x, l.y, l.z);
        if !matches!(
            leaf,
            BlockType::Leaves | BlockType::MapleLeaves | BlockType::SpruceLeaves
        ) {
            continue;
        }
        if leaf_has_support(&world, l, leaf_support_block(leaf)) {
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
    let _ = remesh_chunks(&mut commands, &mut world, &mut mp, None, chunks);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_range_fill_crosses_sections_without_touching_neighbors() {
        let mut blocks = ChunkBlocks::new_uniform(BlockType::Air);
        blocks.fill_column_range(3, 5, 14, 34, BlockType::Stone);
        for y in 0..48 {
            let expected = if (14..=34).contains(&y) {
                BlockType::Stone
            } else {
                BlockType::Air
            };
            assert_eq!(blocks.get(3, y, 5), expected);
            assert_eq!(blocks.get(4, y, 5), BlockType::Air);
        }
    }

    #[test]
    fn geology_is_deterministic_and_respects_height_bands() {
        let sampler = TerrainSampler::new(crate::NoiseParams::default());
        for y in 0..CHUNK_HEIGHT as i32 {
            let a = sampler.geology_block(41.0, y, -73.0);
            let b = sampler.geology_block(41.0, y, -73.0);
            assert_eq!(a, b);
            if y >= 100 || y < 2 {
                assert_eq!(a, None);
            }
            if y >= 90 {
                assert!(!matches!(a, Some(block) if block != BlockType::CoalOre));
            }
        }
    }

    #[test]
    fn fast_forward_moves_forward_and_wraps_midnight() {
        assert_eq!(fast_forward_step(18.0, 6.0, 2.0), (2.0, false));
        assert_eq!(fast_forward_step(23.5, 0.5, 2.0), (1.0, true));
        assert_eq!(fast_forward_step(6.0, 6.0, 2.0), (0.0, true));
    }

    #[test]
    fn advancing_calendar_carries_into_the_next_year() {
        let mut settings = crate::GameSettings::default();
        settings.time_of_day = 23.0;
        settings.day_of_year = 364;
        settings.year = 7;

        advance_calendar(&mut settings, 2.0);

        assert_eq!(settings.time_of_day, 1.0);
        assert_eq!(settings.day_of_year, 0);
        assert_eq!(settings.year, 8);
    }

    #[test]
    fn seasonal_tree_offset_is_stable_bounded_and_root_based() {
        let root = IVec3::new(12, 64, -7);
        let child = IVec3::new(12, 65, -7);
        let mut network = crate::tree::BranchNetwork::default();
        network.add_root(root, crate::tree::TRUNK_THICKNESS);
        network.add_branch(child, root);

        let leaf_a = child + IVec3::X;
        let leaf_b = child + IVec3::Z;
        let a = seasonal_offset_for_leaf(Some(&network), leaf_a, 1234);
        let b = seasonal_offset_for_leaf(Some(&network), leaf_b, 1234);
        assert_eq!(a, b, "ใบของ root เดียวกันต้องเปลี่ยนสีพร้อมกัน");
        assert!(a.abs() <= TREE_SEASON_JITTER_DAYS);
        assert_eq!(a, seasonal_offset_for_leaf(Some(&network), leaf_a, 1234));
        assert_ne!(a, seasonal_offset_for_leaf(Some(&network), leaf_a, 5678));
    }

    #[test]
    fn unsupported_leaves_share_spatial_fallback_bucket() {
        let a = seasonal_offset_for_leaf(None, IVec3::new(17, 70, -15), 9);
        let b = seasonal_offset_for_leaf(None, IVec3::new(22, 74, -10), 9);
        assert_eq!(a, b);
    }

    #[test]
    fn oak_leaf_mesh_is_routed_to_seasonal_buffer() {
        let mut set = ChunkMeshSet::default();
        generate_leaf_mesh_into(
            &mut set,
            "textures/leaves.png",
            1.0,
            2.0,
            3.0,
            [0.3, 0.6, 0.2, 0.8],
            true,
            false,
            12.5,
        );
        assert_eq!(set.seasonal_foliage.positions.len(), 24);
        assert!(set.deco.is_empty());
        assert!(set.seasonal_foliage.uv_b.iter().all(|uv| uv[0] == 12.5));
    }

    #[test]
    fn maple_leaf_mesh_is_routed_to_its_own_seasonal_buffer() {
        let mut set = ChunkMeshSet::default();
        generate_leaf_mesh_into(
            &mut set,
            "textures/maple_leaves.png",
            1.0,
            2.0,
            3.0,
            [0.3, 0.6, 0.2, 0.8],
            true,
            true,
            -7.0,
        );
        assert_eq!(set.maple_foliage.positions.len(), 24);
        assert!(set.seasonal_foliage.is_empty());
        assert!(set.maple_foliage.uv_b.iter().all(|uv| uv[0] == -7.0));
    }

    #[test]
    fn completed_chunk_priority_pops_nearest_first() {
        let center = IVec2::new(10, -4);
        let mut positions = vec![
            IVec2::new(20, -4),
            IVec2::new(11, -4),
            IVec2::new(13, -4),
        ];
        positions.sort_unstable_by_key(|pos| {
            std::cmp::Reverse(chunk_distance_squared(center, *pos))
        });
        assert_eq!(positions.pop(), Some(IVec2::new(11, -4)));
        assert_eq!(positions.pop(), Some(IVec2::new(13, -4)));
    }

    #[test]
    fn river_incision_is_limited_by_valley_width() {
        let river = crate::hydro::RiverPoint {
            mask: 1.0,
            valley_mask: 1.0,
            surface: 40.0,
            depth: 3.0,
            valley_radius: 20.0,
            flow: Vec2::X,
            speed: 1.0,
        };
        let (surface, bed, terrain_fit) = TerrainSampler::river_levels(140.0, river);

        assert_eq!(surface, 115.0);
        assert_eq!(bed, 112.0);
        assert_eq!(terrain_fit, 0.0);

        let (_, _, terrain_fit) = TerrainSampler::river_levels(45.0, river);
        assert_eq!(terrain_fit, 1.0);
    }

    #[test]
    fn xray_hides_base_terrain_and_exposes_an_enclosed_ore_block() {
        let mut blocks = ChunkBlocks::new_uniform(BlockType::Air);
        for z in 7..=9 {
            for y in 63..=65 {
                for x in 7..=9 {
                    blocks.set(x, y, z, BlockType::Stone);
                }
            }
        }
        blocks.set(8, 64, 8, BlockType::CopperOre);
        let neighbors = std::array::from_fn(|_| {
            Arc::new(ChunkBlocks::new_uniform(BlockType::Air))
        });

        let normal = create_mesh_from_blocks_with_xray(
            IVec2::ZERO,
            &blocks,
            &neighbors,
            None,
            None,
            None,
            None,
            None,
            false,
        );
        assert!(normal.total_vertices() > 0, "ผิวนอกของก้อนหินต้องมี mesh ในโหมดปกติ");

        let xray = create_mesh_from_blocks_with_xray(
            IVec2::ZERO,
            &blocks,
            &neighbors,
            None,
            None,
            None,
            None,
            None,
            true,
        );
        assert_eq!(xray.total_vertices(), 24, "X-ray ต้องสร้างครบหกหน้าของแร่หนึ่งบล็อก");
        assert_eq!(xray.total_indices(), 36);
    }

    #[test]
    fn stale_light_results_are_rejected() {
        assert!(light_result_is_current(7, 12, 7, 12));
        assert!(!light_result_is_current(8, 12, 7, 12));
        assert!(!light_result_is_current(7, 13, 7, 12));
    }

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
            light: Default::default(), block_light: Default::default(),
            light_dirty: true,
            light_revision: 1,
            light_missing_neighbors: 0,
            emitters: Default::default(),
        });
        world
    }

    fn set_block_edit(world: &mut VoxelWorld, p: IVec3, block: BlockType) -> Option<IVec3> {
        apply_block_edit(world, &crate::network::BlockEdit::SetBlock {
            pos: p.to_array(),
            block: block as u8,
        })
    }

    fn wire_block(block: BlockType, count: u32) -> crate::item::WireItemStack {
        crate::item::WireItemStack::from_stack(ItemStack {
            item: crate::item::Item::Block(block),
            count: Some(count),
        })
    }

    #[test]
    fn container_payloads_apply_with_their_real_capacities() {
        let mut world = world_with_one_chunk();

        let chest_pos = IVec3::new(1, 1, 1);
        let chest_edit = crate::network::BlockEdit::PlaceContainerBlock {
            pos: chest_pos.to_array(),
            block: BlockType::Chest as u8,
            contents: vec![Some(wire_block(BlockType::Dirt, 3)); 27],
            crucible_data: None,
        };
        assert_eq!(apply_block_edit(&mut world, &chest_edit), Some(chest_pos));
        let chest = world
            .get_chest_slots(chest_pos.x, chest_pos.y, chest_pos.z)
            .expect("chest payload should create its storage");
        assert_eq!(chest.len(), 27);
        assert_eq!(chest[26].expect("last chest slot").count, Some(3));

        let furnace_pos = IVec3::new(2, 1, 1);
        let furnace_edit = crate::network::BlockEdit::PlaceContainerBlock {
            pos: furnace_pos.to_array(),
            block: BlockType::Furnace as u8,
            contents: vec![
                Some(wire_block(BlockType::CopperOre, 1)),
                Some(wire_block(BlockType::OakWood, 2)),
            ],
            crucible_data: None,
        };
        assert_eq!(
            apply_block_edit(&mut world, &furnace_edit),
            Some(furnace_pos)
        );
        let furnace = world
            .get_furnace_slots(furnace_pos.x, furnace_pos.y, furnace_pos.z)
            .expect("furnace payload should create its storage");
        assert_eq!(furnace.len(), 2);
        assert_eq!(furnace[1].expect("fuel slot").count, Some(2));
    }

    #[test]
    fn invalid_container_payloads_are_rejected_without_mutating_the_world() {
        let mut world = world_with_one_chunk();
        let pos = IVec3::new(1, 1, 1);
        let oversized = crate::network::BlockEdit::PlaceContainerBlock {
            pos: pos.to_array(),
            block: BlockType::Furnace as u8,
            contents: vec![None; 3],
            crucible_data: None,
        };
        assert_eq!(apply_block_edit(&mut world, &oversized), None);
        assert_eq!(world.get_block(pos.x, pos.y, pos.z), BlockType::Air);

        assert_eq!(
            apply_block_edit(
                &mut world,
                &crate::network::BlockEdit::SetBlock {
                    pos: pos.to_array(),
                    block: BlockType::Furnace as u8,
                },
            ),
            Some(pos)
        );
        let out_of_range_slot = crate::network::BlockEdit::SetContainerSlot {
            pos: pos.to_array(),
            slot: 2,
            item: Some(wire_block(BlockType::Dirt, 1)),
        };
        assert_eq!(apply_block_edit(&mut world, &out_of_range_slot), None);
        assert!(world
            .get_furnace_slots(pos.x, pos.y, pos.z)
            .is_none());
    }

    #[test]
    fn placed_crucible_preserves_crucible_data() {
        let mut world = world_with_one_chunk();
        let pos = IVec3::new(1, 1, 1);
        let mut expected = crate::chemistry::CrucibleData::default();
        expected.temp = 875;
        expected.liquid_mass[crate::chemistry::Element::Copper as usize] = 500;
        let edit = crate::network::BlockEdit::PlaceContainerBlock {
            pos: pos.to_array(),
            block: BlockType::Crucible as u8,
            contents: Vec::new(),
            crucible_data: Some(expected),
        };

        assert_eq!(apply_block_edit(&mut world, &edit), Some(pos));
        assert_eq!(world.crucibles.get(&pos), Some(&expected));
    }

    #[test]
    fn placed_cast_ingot_preserves_mass_and_composition() {
        let mut world = world_with_one_chunk();
        let pos = IVec3::new(1, 1, 1);
        let mut composition = [0; 8];
        composition[crate::chemistry::Element::Copper as usize] = 640;
        composition[crate::chemistry::Element::Tin as usize] = 160;
        let expected = crate::chemistry::CastIngotData {
            mass: 800,
            composition,
            quality_permille: 920,
            kind: crate::chemistry::CastIngotKind::Bronze,
        };
        let edit = crate::network::BlockEdit::PlaceCastIngot {
            pos: pos.to_array(),
            data: expected,
        };

        assert_eq!(apply_block_edit(&mut world, &edit), Some(pos));
        assert_eq!(world.get_block(pos.x, pos.y, pos.z), BlockType::CastIngot);
        assert_eq!(world.placed_ingots.get(&pos), Some(&expected));

        assert_eq!(
            apply_block_edit(
                &mut world,
                &crate::network::BlockEdit::SetBlock {
                    pos: pos.to_array(),
                    block: BlockType::Air as u8,
                },
            ),
            Some(pos)
        );
        assert!(!world.placed_ingots.contains_key(&pos));
    }

    #[test]
    fn cast_ingot_raycast_uses_model_sized_bounds() {
        let (min, max) = block_collision_box(BlockType::CastIngot);
        let base = Vec3::new(4.0, 2.0, 6.0);

        assert!(ray_aabb_hit(
            base + Vec3::new(0.05, 0.1, -1.0),
            Vec3::Z,
            base + min,
            base + max,
        )
        .is_none());

        let (_, normal) = ray_aabb_hit(
            base + Vec3::new(0.5, 1.0, 0.5),
            Vec3::NEG_Y,
            base + min,
            base + max,
        )
        .unwrap();
        assert_eq!(normal, IVec3::Y);
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

    /// ตัดโคนต้นสน → ท่อนทั้งต้นต้องล้มไล่ขึ้นไป และใบต้องร่วงตาม ไม่ลอยค้าง
    #[test]
    fn spruce_fells_whole_trunk_and_leaves_when_base_is_cut() {
        let mut world = world_with_one_chunk();
        world.set_block(8, 0, 8, BlockType::Grass);

        // ลำต้น SpruceLog 6 ท่อน ตั้งบนพื้นที่ y=1..=6
        let trunk: Vec<IVec3> = (1..=6).map(|y| IVec3::new(8, y, 8)).collect();
        for p in &trunk {
            world.set_block(p.x, p.y, p.z, BlockType::SpruceLog);
        }
        // ใบสนครอบยอด (แผ่นวงรอบท่อนบนๆ)
        let mut leaves = Vec::new();
        for y in 4..=7 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let l = IVec3::new(8 + dx, y, 8 + dz);
                    if world.get_block(l.x, l.y, l.z) == BlockType::Air {
                        world.set_block(l.x, l.y, l.z, BlockType::SpruceLeaves);
                        leaves.push(l);
                    }
                }
            }
        }
        assert!(!leaves.is_empty());

        // ตัดท่อนล่างสุด → ทั้งต้นต้องล้ม
        set_block_edit(&mut world, trunk[0], BlockType::Air);

        let mut app = app_with(world);
        for _ in 0..40 {
            app.update();
        }

        let world = app.world().resource::<VoxelWorld>();
        for p in &trunk {
            assert_eq!(world.get_block(p.x, p.y, p.z), BlockType::Air, "ท่อน {p} ต้องล้ม");
        }
        for l in &leaves {
            assert_eq!(world.get_block(l.x, l.y, l.z), BlockType::Air, "ใบ {l} ยังลอยค้าง");
        }
    }

    /// ต้นสนที่ปลูกจริงด้วย grow_spruce แล้วตัดโคน → ต้องไม่เหลือ log/leaf ลอยค้างเลย
    #[test]
    fn generated_spruce_fully_fells_from_base() {
        let mut world = world_with_one_chunk();
        world.set_block(8, 0, 8, BlockType::Grass);

        // ปลูกต้นสนจริงลง ChunkBlocks ของ chunk (0,0) — base ที่ y=1 บนพื้น y=0
        let mut blocks = (*world.chunks[&IVec2::ZERO].blocks).clone();
        let mut seed: u64 = 12345;
        let mut next = || { seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1); seed >> 33 };
        grow_spruce(&mut blocks, IVec3::new(8, 1, 8), &mut next);
        blocks.compact();
        world.chunks.get_mut(&IVec2::ZERO).unwrap().blocks = Arc::new(blocks);

        // เก็บทุกตำแหน่งของ log/leaf ไว้เช็คทีหลัง
        let mut logs = Vec::new();
        let mut leaves = Vec::new();
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_WIDTH {
                for x in 0..CHUNK_WIDTH {
                    match world.get_block(x as i32, y as i32, z as i32) {
                        BlockType::SpruceLog => logs.push(IVec3::new(x as i32, y as i32, z as i32)),
                        BlockType::SpruceLeaves => leaves.push(IVec3::new(x as i32, y as i32, z as i32)),
                        _ => {}
                    }
                }
            }
        }
        assert!(logs.len() >= 7, "ควรมีลำต้นหลายท่อน ได้ {}", logs.len());
        assert!(!leaves.is_empty());

        // ตัดท่อนล่างสุด
        let base = *logs.iter().min_by_key(|p| p.y).unwrap();
        set_block_edit(&mut world, base, BlockType::Air);

        let mut app = app_with(world);
        for _ in 0..80 {
            app.update();
        }

        let world = app.world().resource::<VoxelWorld>();
        let remaining_logs: Vec<_> = logs.iter().filter(|p| world.get_block(p.x, p.y, p.z) == BlockType::SpruceLog).collect();
        let remaining_leaves: Vec<_> = leaves.iter().filter(|p| world.get_block(p.x, p.y, p.z) == BlockType::SpruceLeaves).collect();
        assert!(remaining_logs.is_empty(), "ยังเหลือท่อนลอยค้าง: {:?}", remaining_logs);
        assert!(remaining_leaves.is_empty(), "ยังเหลือใบลอยค้าง {} ใบ เช่น {:?}", remaining_leaves.len(), remaining_leaves.first());
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
            grow_tree(
                &mut blocks, &mut records, base, params,
                BlockType::Branch, BlockType::Leaves, &mut next,
            );

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
        let noise = crate::NoiseParams { frequency: 0.01, amplitude: 24.0, octaves: 4, seed: 1337, temp_offset: 0.0, ..Default::default() };
        let mut checked = 0;
        for cx in 0..12 {
            for cz in 0..12 {
                let (blocks, records) =
                    generate_chunk_blocks(IVec2::new(cx, cz), noise);
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
                    assert!(
                        matches!(
                            blocks.get(lx, p.y as usize, lz),
                            BlockType::Branch | BlockType::MapleBranch
                        ),
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
                if let Some(root) = records.iter().find(|r| r.parent.is_none()) {
                    let second = records
                        .iter()
                        .find(|r| r.parent == Some(root.pos))
                        .expect("root ที่อยู่ใน chunk ต้องมีลำต้นช่วงถัดไป");
                    assert!(
                        second.thickness >= crate::tree::TRUNK_THICKNESS - 2,
                        "ลำต้นเรียวเร็วเกินไป: {}",
                        second.thickness
                    );
                }
                checked += 1;
            }
        }
        assert!(checked > 0, "ไม่เจอต้นไม้เลยใน 144 chunk — ตัวปั้นอาจไม่ทำงาน");
    }

    #[test]
    fn world_tree_buffer_keeps_foliage_beyond_an_owner_chunk_edge() {
        let mut blocks = WorldTreeBlocks::default();
        scatter_leaves(
            &mut blocks,
            IVec3::new(CHUNK_WIDTH as i32 - 1, 60, 8),
            2,
            BlockType::Leaves,
        );
        assert_eq!(
            blocks.block_at(IVec3::new(CHUNK_WIDTH as i32 + 1, 60, 8)),
            BlockType::Leaves,
            "พุ่ม oak/maple ต้องไม่ถูก clip ที่ขอบ owner chunk"
        );

        let mut state = 12345;
        let mut next = || xorshift_next(&mut state);
        grow_spruce(
            &mut blocks,
            IVec3::new(CHUNK_WIDTH as i32 - 1, 40, 8),
            &mut next,
        );
        assert!(
            blocks.blocks.keys().any(|p| p.x >= CHUNK_WIDTH as i32),
            "ใบ spruce ต้องเขียนข้าม owner chunk ได้"
        );
    }

    #[test]
    fn generated_branch_topology_connects_across_chunks_deterministically() {
        let noise = crate::NoiseParams {
            frequency: 0.01,
            amplitude: 24.0,
            octaves: 4,
            seed: 1337,
            temp_offset: 0.0,
            ..Default::default()
        };
        let mut crossing = None;
        for cz in -8..=8 {
            for cx in -8..=8 {
                let cp = IVec2::new(cx, cz);
                let (_, records) = generate_chunk_blocks(cp, noise);
                if let Some(record) = records.iter().find(|record| {
                    record.parent.is_some_and(|parent| {
                        crate::tree::chunk_of(
                            IVec3::from_array(parent),
                            CHUNK_WIDTH as i32,
                        ) != cp
                    })
                }) {
                    crossing = Some((cp, *record));
                    break;
                }
            }
            if crossing.is_some() {
                break;
            }
        }

        let (child_chunk, child) =
            crossing.expect("ควรพบกิ่งที่ parent อยู่ข้ามรอยต่อ chunk");
        let parent = IVec3::from_array(child.parent.unwrap());
        let parent_chunk = crate::tree::chunk_of(parent, CHUNK_WIDTH as i32);
        let (_, parent_records) = generate_chunk_blocks(parent_chunk, noise);
        assert!(
            parent_records
                .iter()
                .any(|record| record.pos == parent.to_array()),
            "chunk ของ parent ต้องสร้าง node ปลายอีกฝั่งของรอยต่อ"
        );

        let (_, child_records_again) = generate_chunk_blocks(child_chunk, noise);
        assert_eq!(
            generate_chunk_blocks(child_chunk, noise).1,
            child_records_again,
            "ผล topology ต้องไม่ขึ้นกับลำดับการโหลด chunk"
        );
        assert!(
            child_records_again.iter().any(|record| *record == child),
            "กิ่งข้าม chunk ต้องได้ตำแหน่งและ parent เดิมทุกครั้ง"
        );
    }

    #[test]
    fn generated_broadleaf_roots_respect_minimum_spacing_across_chunks() {
        let noise = crate::NoiseParams {
            frequency: 0.01,
            amplitude: 24.0,
            octaves: 4,
            seed: 1337,
            temp_offset: 0.0,
            ..Default::default()
        };
        let mut roots = Vec::new();
        for cz in -6..=6 {
            for cx in -6..=6 {
                let (_, records) = generate_chunk_blocks(IVec2::new(cx, cz), noise);
                roots.extend(
                    records
                        .into_iter()
                        .filter(|record| record.parent.is_none())
                        .map(|record| IVec3::from_array(record.pos)),
                );
            }
        }
        for (index, root) in roots.iter().enumerate() {
            for other in &roots[index + 1..] {
                let distance_sq =
                    (root.x - other.x).pow(2) + (root.z - other.z).pow(2);
                assert!(
                    distance_sq >= TREE_MIN_SPACING * TREE_MIN_SPACING,
                    "ฐานต้นไม้ซ้อน/ชิดเกินไป: {root} กับ {other}"
                );
            }
        }
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

    #[test]
    fn breaking_branch_uses_the_same_effective_joint_radius_from_both_sides() {
        let a = IVec3::new(3, 8, 5);
        let b = a + IVec3::Y;
        let breaking = Some((a, 0.5));
        let effective_a = effective_branch_thickness(a, 16, breaking);
        let effective_b = effective_branch_thickness(b, 12, breaking);
        assert_eq!(effective_a, 8);
        assert_eq!(effective_b, 12);
        assert_eq!(
            branch_joint_radius(effective_a, effective_b),
            branch_joint_radius(effective_b, effective_a),
            "สอง node ต้องใช้ thickness หลัง mining ชุดเดียวกันที่รอยต่อ"
        );
    }

    #[test]
    fn branch_extensions_overlap_inside_a_bend_or_fork() {
        for dir in crate::tree::NEIGHBOUR_DIRS {
            let radius = 0.25;
            let (min_y, max_y) = branch_extension_span(dir, radius);
            assert!(min_y < 0.0, "แขนกิ่งต้องฝังย้อนเข้า junction");
            assert_eq!(min_y, -radius);
            assert_eq!(max_y, dir.as_vec3().length() * 0.5);
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
        grow_tree(
            &mut blocks, &mut records, IVec3::new(8, 60, 8),
            &TREE_PRESETS[ACTIVE_TREE_PRESET].1,
            BlockType::Branch, BlockType::Leaves, &mut next,
        );

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
            grow_tree(
                &mut blocks, &mut records, IVec3::new(8, 60, 8),
                &TREE_PRESETS[ACTIVE_TREE_PRESET].1,
                BlockType::Branch, BlockType::Leaves, &mut next,
            );

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

    /// worldgen ต้องได้เขตอุณหภูมิ + ผิว biome หลากหลายเมื่อกวาดพื้นที่กว้าง (เหนือ-ใต้ข้ามเขต)
    #[test]
    fn worldgen_produces_varied_biomes() {
        let noise = crate::NoiseParams { frequency: 0.01, amplitude: 40.0, octaves: 4, seed: 4242, temp_offset: 0.0, ..Default::default() };
        let sampler = TerrainSampler::new(noise);
        let mut zones: std::collections::HashSet<crate::biome::ClimateZone> = Default::default();
        let mut surfaces: std::collections::HashSet<BlockType> = Default::default();
        for cx in 0..48i32 {
            for cz in 0..64i32 {
                // Macro continents are tens of kilometres wide; cover several
                // plates instead of sampling only one local coastline.
                let wx = (cx * 4_000) as f64;
                let wz = (cz * 1_200) as f64; // ก้าวใหญ่แกน Z ให้ข้ามหลายเขตอุณหภูมิ (แถบกว้างขึ้น ต้องกวาดไกลขึ้น)
                let h = sampler.height(wx, wz);
                zones.insert(crate::biome::zone_of(sampler.temperature_raw(wx, wz)));
                let col = sampler.column_biome(wx, wz);
                let s = surface_block_for(col, h, SEA_LEVEL as i32, sampler.snow_line());
                assert_ne!(s, BlockType::Air, "พื้นต้องเป็นบล็อกจริง");
                surfaces.insert(s);
            }
        }
        assert!(zones.len() >= 3, "ควรเจอเขตอุณหภูมิหลากหลาย (>=3) ได้ {}: {:?}", zones.len(), zones);
        assert!(surfaces.len() >= 3, "ควรเจอผิวหลากหลาย (>=3): {:?}", surfaces);
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
                    light: Default::default(), block_light: Default::default(),
                    light_dirty: true,
                    light_revision: 1,
                    light_missing_neighbors: 0,
                    emitters: Default::default(),
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
        let full = create_mesh_from_blocks(
            chunk_pos,
            &main,
            &neighbors,
            None,
            None,
            None,
            None,
            None,
        );
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

    #[test]
    fn continental_sampler_is_deterministic_and_targets_land_ratio() {
        for seed in [1, 17, 4242] {
            let params = crate::NoiseParams { seed, ..Default::default() };
            let a = TerrainSampler::new(params);
            let b = TerrainSampler::new(params);
            let mut land = 0usize;
            let mut total = 0usize;
            for z in -24..=24 {
                for x in -24..=24 {
                    let wx = x as f64 * 8_000.0 + 137.0;
                    let wz = z as f64 * 8_000.0 - 271.0;
                    let sample = a.continental_sample(wx, wz);
                    assert_eq!(sample, b.continental_sample(wx, wz));
                    land += usize::from(sample.landness >= 0.0);
                    total += 1;
                }
            }
            let ratio = land as f64 / total as f64;
            assert!((0.40..=0.50).contains(&ratio), "seed {seed}: land ratio {ratio}");
        }
    }

    #[test]
    fn continental_landness_is_continuous_across_voronoi_edges() {
        let sampler = TerrainSampler::new(crate::NoiseParams {
            seed: 4242,
            continent_scale: 8_000.0,
            ..Default::default()
        });
        let step = 64.0;
        let mut max_jump = 0.0f64;
        for z in -160..=160 {
            for x in -160..=160 {
                let wx = x as f64 * step;
                let wz = z as f64 * step;
                let here = sampler.continental_sample(wx, wz).landness;
                let right = sampler.continental_sample(wx + step, wz).landness;
                let down = sampler.continental_sample(wx, wz + step).landness;
                max_jump = max_jump.max((here - right).abs()).max((here - down).abs());
            }
        }

        assert!(
            max_jump < 0.15,
            "continentalness jumped {max_jump} across one block-scale sample"
        );
    }

    #[test]
    fn safe_spawn_is_inland_and_within_search_radius() {
        let params = crate::NoiseParams { seed: 90210, ..Default::default() };
        let sampler = TerrainSampler::new(params);
        let spawn = safe_mainland_spawn(params);
        let continental = sampler.continental_sample(spawn.x as f64, spawn.z as f64);
        assert!(continental.landness >= 0.22);
        assert!(spawn.x.hypot(spawn.z) <= 20_001.0);
        assert!(spawn.y > SEA_LEVEL as f32);
        assert!(sampler.volcano_sample(spawn.x as f64, spawn.z as f64).descriptor.is_none());
    }
}
