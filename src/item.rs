use bevy::prelude::*;
use crate::voxel::{BlockType, ItemStack};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ToolType {
    Chisel,
    CopperWire,
    Pickaxe = 2,
    Axe = 3,
    Shovel = 4,
    SlagSkimmer = 5,
}

/// หมวดการขุด — จับคู่ tool กับบล็อกที่มันถนัด (ฝั่งบล็อกดู block_dig_class ใน voxel.rs)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DigClass {
    None,
    Pick,
    Axe,
    Shovel,
}

impl ToolType {
    /// เลขอยู่ใน save file / network wire แล้ว — เพิ่มได้แต่ต่อท้าย ห้ามสลับ
    pub fn to_u8(self) -> u8 {
        match self {
            ToolType::Chisel => 0,
            ToolType::CopperWire => 1,
            ToolType::Pickaxe => 2,
            ToolType::Axe => 3,
            ToolType::Shovel => 4,
            ToolType::SlagSkimmer => 5,
        }
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(ToolType::Chisel),
            1 => Some(ToolType::CopperWire),
            2 => Some(ToolType::Pickaxe),
            3 => Some(ToolType::Axe),
            4 => Some(ToolType::Shovel),
            5 => Some(ToolType::SlagSkimmer),
            _ => None,
        }
    }

    /// tool นี้ถนัดขุดบล็อกหมวดไหน (Chisel/CopperWire ไม่ใช่เครื่องมือขุด)
    pub fn dig_class(self) -> DigClass {
        match self {
            ToolType::Pickaxe => DigClass::Pick,
            ToolType::Axe => DigClass::Axe,
            ToolType::Shovel => DigClass::Shovel,
            ToolType::Chisel | ToolType::CopperWire | ToolType::SlagSkimmer => DigClass::None,
        }
    }

    /// ตัวคูณความเร็วเมื่อขุดบล็อกหมวดที่ตัวเองถนัด — จุดต่อยอด tier ในอนาคต
    /// (ไม้/หิน/เหล็ก = คืนค่าต่างกันตรงนี้ที่เดียว)
    pub fn dig_speed(self) -> f32 {
        match self {
            ToolType::Pickaxe | ToolType::Axe | ToolType::Shovel => 5.0,
            ToolType::Chisel | ToolType::CopperWire | ToolType::SlagSkimmer => 1.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MaterialType {
    Copper = 0,
    Iron = 1,
    Stick = 2,
    CopperIngot = 3,
    IronIngot = 4,
    Coal = 5,
    Slag = 6,
    Limestone = 7,
    BronzeIngot = 8,
    BrassIngot = 9,
    SteelIngot = 10,
    SlagAlloyIngot = 11,
    Tin = 12,
    Zinc = 13,
    EmptyGlassBottle = 14,
    SulfuricAcidBottle = 15,
}

impl MaterialType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(MaterialType::Copper),
            1 => Some(MaterialType::Iron),
            2 => Some(MaterialType::Stick),
            3 => Some(MaterialType::CopperIngot),
            4 => Some(MaterialType::IronIngot),
            5 => Some(MaterialType::Coal),
            6 => Some(MaterialType::Slag),
            7 => Some(MaterialType::Limestone),
            8 => Some(MaterialType::BronzeIngot),
            9 => Some(MaterialType::BrassIngot),
            10 => Some(MaterialType::SteelIngot),
            11 => Some(MaterialType::SlagAlloyIngot),
            12 => Some(MaterialType::Tin),
            13 => Some(MaterialType::Zinc),
            14 => Some(MaterialType::EmptyGlassBottle),
            15 => Some(MaterialType::SulfuricAcidBottle),
            _ => None,
        }
    }
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Item {
    Block(BlockType),
    Tool(ToolType),
    Material(MaterialType),
    Crucible(crate::chemistry::CrucibleData),
    CastIngot(crate::chemistry::CastIngotData),
    PickaxeHead(crate::chemistry::CastIngotData),
    CraftedPickaxe(crate::chemistry::CastIngotData),
}

pub struct FuelProperties {
    pub base_temp: f32,
    pub base_energy: f32, // How long it burns at 1.0x multiplier
}

impl Item {
    pub fn get_fuel_properties(&self) -> Option<FuelProperties> {
        match self {
            Item::Block(crate::voxel::BlockType::OakWood) => Some(FuelProperties { base_temp: 800.0, base_energy: 1000.0 }),
            Item::Block(crate::voxel::BlockType::Branch | crate::voxel::BlockType::MapleBranch) => Some(FuelProperties { base_temp: 600.0, base_energy: 300.0 }),
            Item::Block(crate::voxel::BlockType::MapleLog) => Some(FuelProperties { base_temp: 800.0, base_energy: 1000.0 }),
            Item::Material(crate::item::MaterialType::Stick) => Some(FuelProperties { base_temp: 600.0, base_energy: 300.0 }),
            Item::Material(crate::item::MaterialType::Coal) => Some(FuelProperties { base_temp: 1200.0, base_energy: 3000.0 }),
            // ... more fuels
            _ => None,
        }
    }

    pub fn is_fuel(&self) -> bool {
        self.get_fuel_properties().is_some()
    }
}

/// เข้ารหัส Item เป็น (kind, id) สำหรับเซฟลง disk / ส่งข้าม network — ไม่ derive serde
/// บน BlockType/Item ตรงๆ (ตาม convention เดิมของโปรเจกต์ที่ใช้ as u8/from_u8)
/// kind: 0 = Block, 1 = Tool
pub fn item_to_wire(item: Item) -> (u8, u8) {
    match item {
        Item::Block(b) => (0, b as u8),
        Item::Tool(t) => (1, t.to_u8()),
        Item::Material(m) => (2, m.to_u8()),
        Item::Crucible(_) => (3, 0),
        Item::CastIngot(data) => (4, data.kind.to_u8()),
        Item::PickaxeHead(data) => (5, data.kind.to_u8()),
        Item::CraftedPickaxe(data) => (6, data.kind.to_u8()),
    }
}

pub fn item_from_wire(kind: u8, id: u8) -> Option<Item> {
    match kind {
        0 => Some(Item::Block(BlockType::from_u8(id))),
        1 => ToolType::from_u8(id).map(Item::Tool),
        2 => MaterialType::from_u8(id).map(Item::Material),
        3 => Some(Item::Crucible(crate::chemistry::CrucibleData::default())), // Empty crucible if parsed from just u8
        // Compact held-item replication only needs a visual category. Inventory
        // serialization uses WireItemStack and preserves the full payload.
        4 => crate::chemistry::CastIngotKind::from_u8(id).map(|kind| {
            Item::CastIngot(crate::chemistry::CastIngotData {
                mass: 1_000,
                composition: [0; 8],
                quality_permille: 1_000,
                kind,
            })
        }),
        5 => crate::chemistry::CastIngotKind::from_u8(id).map(|kind| {
            let data = crate::chemistry::CastIngotData {
                mass: crate::chemistry::PICKAXE_HEAD_MASS_GRAMS,
                composition: [0; 8],
                quality_permille: 1_000,
                kind,
            };
            Item::PickaxeHead(data)
        }),
        6 => crate::chemistry::CastIngotKind::from_u8(id).map(|kind| {
            Item::CraftedPickaxe(crate::chemistry::CastIngotData {
                mass: crate::chemistry::PICKAXE_HEAD_MASS_GRAMS,
                composition: [0; 8],
                quality_permille: 1_000,
                kind,
            })
        }),
        _ => None,
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SimpleItem {
    pub kind: u8,
    pub id: u8,
    pub count: u32,
    #[serde(default)]
    pub cast_ingot: Option<crate::chemistry::CastIngotData>,
    #[serde(default)]
    pub cast_tool: Option<crate::chemistry::CastIngotData>,
}

impl SimpleItem {
    pub fn from_stack(s: ItemStack) -> Self {
        let (kind, id) = item_to_wire(s.item);
        let cast_ingot = match s.item {
            Item::CastIngot(data) => Some(data),
            _ => None,
        };
        let cast_tool = match s.item {
            Item::PickaxeHead(data) | Item::CraftedPickaxe(data) => Some(data),
            _ => None,
        };
        Self { kind, id, count: s.count.unwrap_or(1), cast_ingot, cast_tool }
    }

    pub fn to_stack(self) -> Option<ItemStack> {
        let item = match self.kind {
            3 => Item::Crucible(crate::chemistry::CrucibleData::default()), // SimpleItem doesn't hold nested crucibles
            4 => Item::CastIngot(self.cast_ingot?),
            5 => Item::PickaxeHead(self.cast_tool?),
            6 => Item::CraftedPickaxe(self.cast_tool?),
            _ => item_from_wire(self.kind, self.id)?
        };
        Some(crate::voxel::ItemStack { item, count: Some(self.count as u32) })
    }
}

/// รูปแบบ ItemStack บนสายส่ง (เซฟลง disk / ส่งข้าม network) — ตัวเดียวที่ derive serde
/// เก็บ BlockType/Item ให้ปลอดจาก serde ตาม convention เดิมของโปรเจกต์
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub struct WireItemStack {
    pub kind: u8,
    pub id: u8,
    pub count: Option<u32>,
    #[serde(default, alias = "pot")]
    pub crucible: Option<crate::chemistry::CrucibleData>,
    #[serde(default)]
    pub cast_ingot: Option<crate::chemistry::CastIngotData>,
    #[serde(default)]
    pub cast_tool: Option<crate::chemistry::CastIngotData>,
}

impl WireItemStack {
    pub fn from_stack(s: ItemStack) -> Self {
        let (kind, id) = item_to_wire(s.item);
        let crucible = match s.item {
            Item::Crucible(contents) => Some(contents),
            _ => None,
        };
        let cast_ingot = match s.item {
            Item::CastIngot(data) => Some(data),
            _ => None,
        };
        let cast_tool = match s.item {
            Item::PickaxeHead(data) | Item::CraftedPickaxe(data) => Some(data),
            _ => None,
        };
        Self { kind, id, count: s.count, crucible, cast_ingot, cast_tool }
    }

    pub fn to_stack(self) -> Option<ItemStack> {
        let item = match self.kind {
            3 => Item::Crucible(self.crucible.unwrap_or_default()),
            4 => Item::CastIngot(self.cast_ingot?),
            5 => Item::PickaxeHead(self.cast_tool?),
            6 => Item::CraftedPickaxe(self.cast_tool?),
            _ => item_from_wire(self.kind, self.id)?
        };
        Some(ItemStack { item, count: self.count })
    }
}

impl Item {
    /// เลิกใช้ตอน block picker แบบ egui ถูกแทนด้วยกริดไอคอน — เก็บไว้สำหรับ tooltip
    #[allow(dead_code)]
    pub fn name(&self) -> String {
        match self {
            Item::Block(b) => crate::voxel::block_name(*b),
            Item::Tool(ToolType::Chisel) => "Chisel",
            Item::Tool(ToolType::CopperWire) => "Copper Wire",
            Item::Tool(ToolType::Pickaxe) => "Pickaxe",
            Item::Tool(ToolType::Axe) => "Axe",
            Item::Tool(ToolType::Shovel) => "Shovel",
            Item::Tool(ToolType::SlagSkimmer) => "Slag Skimmer",
            Item::Material(MaterialType::Copper) => "Copper",
            Item::Material(MaterialType::Iron) => "Iron",
            Item::Material(MaterialType::Stick) => "Stick",
            Item::Material(MaterialType::CopperIngot) => "Copper Ingot",
            Item::Material(MaterialType::IronIngot) => "Iron Ingot",
            Item::Material(MaterialType::Coal) => "Coal",
            Item::Material(MaterialType::Slag) => "Slag",
            Item::Material(MaterialType::Limestone) => "Limestone",
            Item::Material(MaterialType::BronzeIngot) => "Bronze Ingot",
            Item::Material(MaterialType::BrassIngot) => "Brass Ingot",
            Item::Material(MaterialType::SteelIngot) => "Steel Ingot",
            Item::Material(MaterialType::SlagAlloyIngot) => "Slag Alloy Ingot",
            Item::Material(MaterialType::Tin) => "Tin",
            Item::Material(MaterialType::Zinc) => "Zinc",
            Item::Material(MaterialType::EmptyGlassBottle) => "Empty Glass Bottle",
            Item::Material(MaterialType::SulfuricAcidBottle) => "Sulfuric Acid Bottle",
            Item::Crucible(_) => "Crucible",
            Item::CastIngot(data) => return data.kind.name().to_string(),
            Item::PickaxeHead(data) => {
                return format!("{} Pickaxe Head", data.kind.material_name())
            }
            Item::CraftedPickaxe(data) => {
                return format!("{} Pickaxe", data.kind.material_name())
            }
        }
        .to_string()
    }

    /// เลิกใช้กับ UI icon แล้ว (แทนที่ด้วย icon_image ที่ render 3 มิติจริงต่อบล็อก) — ยังใช้กับ
    /// particle เศษบล็อกตอนทุบอยู่ (particles.rs) ซึ่งเป็นคนละ use case ไม่ต้องการความถูกต้องรายหน้า
    pub fn icon_texture(&self) -> Option<&'static str> {
        match self {
            Item::Block(b) => crate::voxel::hotbar_icon_texture(*b),
            Item::Tool(ToolType::Chisel) => Some("items/chisel.png"),
            Item::Tool(ToolType::CopperWire) => Some("items/copper_wire.png"),
            Item::Tool(ToolType::Pickaxe) => Some("items/pickaxe.png"),
            Item::Tool(ToolType::Axe) => Some("items/axe.png"),
            Item::Tool(ToolType::Shovel) => Some("items/shovel.png"),
            Item::Tool(ToolType::SlagSkimmer) => Some("items/slag_skimmer.png"),
            Item::Material(MaterialType::Copper) => Some("items/copper.png"),
            Item::Material(MaterialType::Iron) => Some("items/iron.png"),
            Item::Material(MaterialType::Stick) => Some("items/stick.png"),
            Item::Material(MaterialType::CopperIngot) => Some("items/copper_ingot.png"),
            Item::Material(MaterialType::IronIngot) => Some("items/iron_ingot.png"),
            Item::Material(MaterialType::Coal) => Some("items/coal.png"),
            Item::Material(MaterialType::Slag) => Some("items/slag.png"),
            Item::Material(MaterialType::Limestone) => Some("textures/limestone.png"),
            Item::Material(MaterialType::BronzeIngot) => Some("items/bronze_ingot.png"),
            Item::Material(MaterialType::BrassIngot) => Some("items/brass_ingot.png"),
            Item::Material(MaterialType::SteelIngot) => Some("items/steel_ingot.png"),
            Item::Material(MaterialType::SlagAlloyIngot) => Some("items/slag_alloy_ingot.png"),
            Item::Material(MaterialType::Tin) => Some("items/tin.png"),
            Item::Material(MaterialType::Zinc) => Some("items/zinc.png"),
            Item::Material(MaterialType::EmptyGlassBottle) => Some("items/glass_bottle.png"),
            Item::Material(MaterialType::SulfuricAcidBottle) => Some("items/sulfuric_acid_bottle.png"),
            Item::Crucible(_) => None,
            Item::CastIngot(_) => None,
            Item::PickaxeHead(_) | Item::CraftedPickaxe(_) => None,
        }
    }

    /// icon สำหรับ UI จริง — Block ใช้ภาพที่ render 3 มิติไว้แล้ว (ดู ItemIconCache/start_icon_bake),
    /// Tool ยังใช้ .png แบนเหมือนเดิม (ไม่ใช่บล็อก ไม่มีปัญหาเรื่องหน้าไม่เหมือนกัน)
    pub fn icon_image(
        &self,
        icons: &crate::voxel::ItemIconCache,
        asset_server: &AssetServer,
    ) -> Option<Handle<Image>> {
        let cache_key = match self {
            Item::Crucible(_) => Item::Block(crate::voxel::BlockType::Crucible),
            Item::CastIngot(data) => icons
                .0
                .keys()
                .copied()
                .find(|item| {
                    matches!(item, Item::CastIngot(cached) if cached.kind == data.kind)
                })
                .unwrap_or(*self),
            Item::PickaxeHead(data) => icons.0.keys().copied().find(|item| {
                matches!(item, Item::PickaxeHead(cached) if cached.kind == data.kind)
            }).unwrap_or(*self),
            Item::CraftedPickaxe(data) => icons.0.keys().copied().find(|item| {
                matches!(item, Item::CraftedPickaxe(cached) if cached.kind == data.kind)
            }).unwrap_or(*self),
            _ => *self,
        };

        // ลองหาใน cache ก่อน (บล็อกทั้งหมด และ Pickaxe จะมีภาพที่ render ไว้แล้ว)
        if let Some(handle) = icons.0.get(&cache_key) {
            return Some(handle.clone());
        }
        
        // ถ้าไม่มีใน cache ให้ fallback กลับไปใช้ icon_texture (พวกแผ่นแบน)
        self.icon_texture().map(|path| asset_server.load(path))
    }

    pub fn color(&self) -> [f32; 4] {
        match self {
            Item::Block(_) => [1.0, 1.0, 1.0, 1.0], // block ใช้ tint จาก voxel::hotbar_icon_texture() อีกทีถ้าเป็นใบไม้
            Item::Tool(_) | Item::Material(_) | Item::Crucible(_) | Item::CastIngot(_)
            | Item::PickaxeHead(_) | Item::CraftedPickaxe(_) => [1.0, 1.0, 1.0, 1.0],
        }
    }

    pub fn render_as_2d_sprite(&self) -> bool {
        match self {
            Item::Block(crate::voxel::BlockType::TallGrass) => true,
            Item::Tool(t) if tool_model_path(*t).is_none() => true,
            Item::Material(_) => true,
            Item::CastIngot(_) => false,
            Item::PickaxeHead(_) | Item::CraftedPickaxe(_) => false,
            _ => false,
        }
    }
}

// --------------------------------------------------------
// Item Drop System (โครงร่าง)
// --------------------------------------------------------

#[derive(Component)]
pub struct DroppedItem {
    pub item: Item,
    pub count: u32,
    pub velocity: Vec3,
    /// อายุ (วินาที) นับตั้งแต่ตก — เกิน DROP_LIFETIME แล้ว despawn กันของค้างถาวร
    pub age: f32,
}

/// ของที่ตกพื้นหายเองหลังกี่วินาที
const DROP_LIFETIME: f32 = 300.0;
/// ดีเลย์ก่อนเก็บของที่เพิ่งทิ้ง (วินาที) — กันทิ้งแล้วถูกดูดกลับทันที
const PICKUP_DELAY: f32 = 0.8;

/// ของที่ตกเป็น "แผ่นแบน" (item ที่ไม่ใช่บล็อก เช่น tool) — หันเข้ากล้องแบบ billboard
/// (บล็อกไม่มี marker นี้ → เป็นก้อน 3D หมุนรอบแกน Y ตามปกติ)
#[derive(Component)]
pub struct FlatSprite;

#[derive(Message)]
pub struct SpawnDroppedItemEvent {
    pub item: Item,
    pub pos: Vec3,
    pub velocity: Vec3,
}

pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnDroppedItemEvent>();
        app.init_resource::<HeldItemView>();
        app.add_systems(Update, (
            spawn_dropped_item_system,
            pickup_item_system,
            animate_dropped_items,
            billboard_flat_drops,
            update_held_item_view,
            animate_held_item_view.after(update_held_item_view),
        ).run_if(in_state(crate::GameState::InGame)));
        // กล้องอยู่ข้ามฉาก — ไม่เก็บ viewmodel ทิ้งจะค้างโชว์หน้าเมนู
        app.add_systems(OnExit(crate::GameState::InGame), clear_held_item_view);
    }
}

// --------------------------------------------------------
// First-person viewmodel — ของที่ถือลอยมุมขวาล่างจอแบบ Minecraft
// (เกาะเป็นลูกของ MainCamera; จูนตำแหน่ง/ขนาดที่ viewmodel_params)
// --------------------------------------------------------

#[derive(Resource, Default)]
pub struct HeldItemView {
    pub item: Option<Item>,
    pub entity: Option<Entity>,
    /// เวลาเหลือของ swing pulse (วินาที) — คลิกทุบ/วางแล้วเหวี่ยงหนึ่งจังหวะ
    pub swing: f32,
}

const VIEWMODEL_SWING_TIME: f32 = 0.25;

/// ขนาด + transform ประจำตัว viewmodel ของ item (สัมพัทธ์กับกล้อง)
fn viewmodel_params(item: Item) -> (f32, Transform) {
    let size = match item {
        Item::Block(_) => 0.3,
        Item::Tool(t) if tool_model_path(t).is_some() => 0.4,
        Item::Tool(_) => 0.35,
        Item::Material(_) => 0.25,
        Item::Crucible(_) => 0.35,
        Item::CastIngot(_) => 0.25,
        Item::PickaxeHead(_) => 0.3,
        Item::CraftedPickaxe(_) => 0.4,
    };
    let tf = Transform::from_translation(Vec3::new(0.4, -0.35, -0.7))
        .with_rotation(Quat::from_rotation_y(-0.5)); // เอียงเข้ากลางจอเล็กน้อย
    (size, tf)
}

fn update_held_item_view(
    mut commands: Commands,
    mut view: ResMut<HeldItemView>,
    hotbar: Res<crate::voxel::Hotbar>,
    camera_query: Query<Entity, With<crate::camera::MainCamera>>,
    free_cam: Query<&crate::camera::FreeCamera>,
    mut vis_query: Query<&mut Visibility>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    block_mats: Res<crate::voxel::BlockMaterials>,
    campfire_assets: Res<crate::voxel::BlockModelAssets>,
) {
    let current = hotbar.slots[hotbar.selected].map(|s| s.item);
    if current != view.item {
        if let Some(entity) = view.entity.take() {
            commands.entity(entity).despawn();
        }
        view.item = current;
        if let Some(item) = current {
            if let Ok(camera) = camera_query.single() {
                let (size, tf) = viewmodel_params(item);
                let entity = spawn_item_visual(
                    &mut commands, &mut meshes, &mut materials, &asset_server,
                    &block_mats, &campfire_assets, item, size, tf,
                );
                commands.entity(camera).add_child(entity);
                view.entity = Some(entity);
            }
        }
    }

    // มุมมองบุคคลที่สาม (F5) — ของลอยหน้ากล้องจะบังจอ ซ่อนไว้
    if let Some(entity) = view.entity {
        if let (Ok(free), Ok(mut vis)) = (free_cam.single(), vis_query.get_mut(entity)) {
            *vis = if free.third_person { Visibility::Hidden } else { Visibility::Inherited };
        }
    }
}

/// ท่าเหวี่ยงของ viewmodel: ขุดค้าง = แกว่งต่อเนื่อง (จังหวะเดียวกับแขน avatar),
/// คลิกทุบ/วางครั้งเดียว = pulse สั้น — ทำเป็น offset ทับ transform ประจำตัว
fn animate_held_item_view(
    time: Res<Time>,
    mut view: ResMut<HeldItemView>,
    breaking: Res<crate::voxel::BreakingProgress>,
    mouse: Res<ButtonInput<MouseButton>>,
    paused: Res<crate::Paused>,
    mut tf_query: Query<&mut Transform>,
) {
    let (Some(entity), Some(item)) = (view.entity, view.item) else { return };
    let Ok(mut tf) = tf_query.get_mut(entity) else { return };

    if !paused.0 && (mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right)) {
        view.swing = VIEWMODEL_SWING_TIME;
    }
    if view.swing > 0.0 {
        view.swing -= time.delta_secs();
    }

    // 0..1: ขุดค้างใช้คลื่น sin ต่อเนื่อง, pulse ใช้ครึ่งคลื่นเดียว (ขึ้นแล้วลง)
    let amount = if breaking.target.is_some() {
        (time.elapsed_secs() * 15.0).sin() * 0.5 + 0.5
    } else if view.swing > 0.0 {
        ((view.swing / VIEWMODEL_SWING_TIME) * std::f32::consts::PI).sin()
    } else {
        0.0
    };

    let (_, base) = viewmodel_params(item);
    *tf = base.mul_transform(
        Transform::from_rotation(Quat::from_rotation_x(-amount * 0.9))
            .with_translation(Vec3::new(0.0, -amount * 0.1, -amount * 0.15)),
    );
}

fn clear_held_item_view(mut commands: Commands, mut view: ResMut<HeldItemView>) {
    if let Some(entity) = view.entity.take() {
        commands.entity(entity).despawn();
    }
    view.item = None;
}

/// โมเดล 3D ของ tool (ใต้ assets/) — ตรวจไฟล์จริงก่อน: ยังไม่ได้ export
/// มา = คืน None ให้ fallback เป็นแผ่นแบน (กันของล่องหนตอนโมเดลยังไม่มา)
pub fn tool_model_path(tool: ToolType) -> Option<&'static str> {
    let path = match tool {
        ToolType::Pickaxe => "model/pickaxe.gltf",
        _ => return None,
    };
    crate::voxel::project_root()
        .join("assets")
        .join(path)
        .exists()
        .then_some(path)
}

/// spawn ภาพของ item หนึ่งชิ้น — ใช้ร่วมกันทั้งของตกพื้น, viewmodel มือตัวเอง,
/// และมือ avatar ผู้เล่นอื่น คืน entity หลักพร้อม `transform` ที่ให้มา
/// (`size`: บล็อก = ขนาดคิวบ์, glTF = scale คูณเข้า transform, แผ่นแบน = ด้านของ quad)
pub fn spawn_item_visual(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
    block_mats: &crate::voxel::BlockMaterials,
    campfire_assets: &crate::voxel::BlockModelAssets,
    item: Item,
    size: f32,
    transform: Transform,
) -> Entity {
    use bevy::light::NotShadowCaster;
    
    if item.render_as_2d_sprite() {
        let material = match item.icon_texture() {
            Some(path) => materials.add(StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: Some(asset_server.load(path)),
                alpha_mode: AlphaMode::Blend, // PNG โปร่งใส
                unlit: true,
                cull_mode: None, // เห็นทั้งสองด้าน
                ..default()
            }),
            None => {
                let c = item.color();
                materials.add(StandardMaterial {
                    base_color: Color::srgba(c[0], c[1], c[2], c[3]),
                    unlit: true,
                    cull_mode: None,
                    ..default()
                })
            }
        };
        return commands.spawn((
            Mesh3d(meshes.add(Rectangle::new(size, size))),
            MeshMaterial3d(material),
            transform,
            NotShadowCaster,
        )).id();
    }

    match item {
        // บล็อก → คิวบ์จิ๋ว 6 หน้า texture ถูกต้อง (ขนาด bake ในตัว mesh ไม่ใช่ scale)
        Item::Block(block) => {
            let entity = crate::voxel::spawn_block_model(
                commands, meshes, materials, block_mats, campfire_assets,
                block, Vec3::ZERO, size, bevy::camera::visibility::RenderLayers::default(),
            );
            commands.entity(entity).insert(transform);
            entity
        }
        Item::Crucible(_) => {
            let entity = crate::voxel::spawn_block_model(
                commands, meshes, materials, block_mats, campfire_assets,
                crate::voxel::BlockType::Crucible, Vec3::ZERO, size, bevy::camera::visibility::RenderLayers::default(),
            );
            commands.entity(entity).insert(transform);
            entity
        }
        Item::CastIngot(data) => commands
            .spawn((
                WorldAssetRoot(campfire_assets.ingot_scene.clone()),
                transform.with_scale(transform.scale * size),
                crate::voxel::CastIngotMaterialOverride(
                    campfire_assets.cast_ingot_materials[data.kind.to_u8() as usize].clone(),
                ),
                NotShadowCaster,
            ))
            .id(),
        Item::PickaxeHead(data) => commands
            .spawn((
                WorldAssetRoot(asset_server.load(
                    bevy::gltf::GltfAssetLabel::Scene(0).from_asset("model/pickaxe_head.gltf"),
                )),
                transform.with_scale(transform.scale * size),
                crate::voxel::NamedMaterialOverride {
                    node_name: "Head",
                    material: campfire_assets.cast_ingot_materials[data.kind.to_u8() as usize]
                        .clone(),
                },
                crate::voxel::HiddenModelNode("Handle"),
                NotShadowCaster,
            ))
            .id(),
        Item::CraftedPickaxe(data) => commands
            .spawn((
                WorldAssetRoot(asset_server.load(
                    bevy::gltf::GltfAssetLabel::Scene(0).from_asset("model/pickaxe.gltf"),
                )),
                transform.with_scale(transform.scale * size),
                crate::voxel::NamedMaterialOverride {
                    node_name: "Head",
                    material: campfire_assets.cast_ingot_materials[data.kind.to_u8() as usize].clone(),
                },
                NotShadowCaster,
            ))
            .id(),
        // tool ที่มีโมเดล 3D
        Item::Tool(tool) if tool_model_path(tool).is_some() => {
            use bevy::gltf::GltfAssetLabel;
            let path = tool_model_path(tool).unwrap();
            commands.spawn((
                WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(path))),
                transform.with_scale(transform.scale * size),
                NotShadowCaster,
            )).id()
        }
        // tool อื่นและ TallGrass ถูกจัดการใน render_as_2d_sprite() ข้างบนแล้ว
        _ => unreachable!(),
    }
}

fn spawn_dropped_item_system(
    mut commands: Commands,
    mut events: MessageReader<SpawnDroppedItemEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    block_mats: Res<crate::voxel::BlockMaterials>,
    campfire_assets: Res<crate::voxel::BlockModelAssets>,
) {
    for ev in events.read() {
        // ขนาดของตกพื้น: บล็อกคิวบ์เล็ก, tool โมเดลจริงใหญ่หน่อย, แผ่นแบนกลางๆ
        let size = match ev.item {
            Item::Block(_) => 0.4,
            Item::Tool(t) if tool_model_path(t).is_some() => 0.6,
            Item::Tool(_) => 0.5,
            Item::Material(_) => 0.3,
            Item::Crucible(_) => 0.4,
            Item::CastIngot(_) => 0.3,
            Item::PickaxeHead(_) => 0.4,
            Item::CraftedPickaxe(_) => 0.6,
        };
        let entity = spawn_item_visual(
            &mut commands, &mut meshes, &mut materials, &asset_server,
            &block_mats, &campfire_assets,
            ev.item, size, Transform::from_translation(ev.pos),
        );
        commands.entity(entity).insert(
            DroppedItem { item: ev.item, count: 1, velocity: ev.velocity, age: 0.0 },
        );
        // เฉพาะแผ่นแบนที่ต้องหันเข้ากล้อง (โมเดล 3D หมุนรอบตัวเองใน animate_dropped_items)
        if matches!(ev.item, Item::Tool(t) if tool_model_path(t).is_none()) {
            commands.entity(entity).insert(FlatSprite);
        }
    }
}

fn pickup_item_system(
    mut commands: Commands,
    player_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut item_query: Query<(Entity, &Transform, &mut DroppedItem)>,
    mut hotbar: ResMut<crate::voxel::Hotbar>,
) {
    // นับจำนวนเข้าช่องทั้งสองโหมด (creative วางไม่ลด count แต่ทิ้ง/เก็บนับจริง)
    let Some(player_tf) = player_query.iter().next() else { return };
    let player_pos = player_tf.translation;

    for (entity, item_tf, mut dropped) in item_query.iter_mut() {
        // ดีเลย์ก่อนเก็บ — กันของที่เพิ่งทิ้งถูกดูดกลับทันที
        if dropped.age < PICKUP_DELAY {
            continue;
        }
        if item_tf.translation.distance(player_pos) >= 2.0 {
            continue;
        }
        let max = crate::voxel::max_stack(dropped.item);
        let mut remaining = dropped.count;

        // 1. เติมช่องที่ชนิดตรงกันและยังไม่เต็ม
        for slot in hotbar.slots.iter_mut() {
            if remaining == 0 {
                break;
            }
            if let Some(stack) = slot {
                if stack.item == dropped.item {
                    let cur = stack.count.unwrap_or(max);
                    let space = max.saturating_sub(cur);
                    let take = space.min(remaining);
                    stack.count = Some(cur + take);
                    remaining -= take;
                }
            }
        }
        // 2. ที่เหลือลงช่องว่าง (ทีละ stack)
        for slot in hotbar.slots.iter_mut() {
            if remaining == 0 {
                break;
            }
            if slot.is_none() {
                let take = remaining.min(max);
                *slot = Some(crate::voxel::ItemStack {
                    item: dropped.item,
                    count: Some(take),
                });
                remaining -= take;
            }
        }

        if remaining == 0 {
            commands.entity(entity).despawn();
        } else if remaining != dropped.count {
            // เก็บได้บางส่วน — ปรับจำนวนที่เหลือค้างบนพื้นไว้เก็บต่อ
            dropped.count = remaining;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Item, SimpleItem, WireItemStack};
    use crate::chemistry::{CastIngotData, CastIngotKind, Element};
    use crate::voxel::ItemStack;

    fn sample_ingot() -> CastIngotData {
        let mut composition = [0; 8];
        composition[Element::Copper as usize] = 640;
        composition[Element::Tin as usize] = 160;
        CastIngotData {
            mass: 800,
            composition,
            quality_permille: 920,
            kind: CastIngotKind::Bronze,
        }
    }

    #[test]
    fn cast_ingot_round_trips_through_inventory_wire() {
        let original = ItemStack {
            item: Item::CastIngot(sample_ingot()),
            count: Some(2),
        };
        let restored = WireItemStack::from_stack(original).to_stack().unwrap();
        assert!(restored == original);
    }

    #[test]
    fn crafted_pickaxe_round_trips_through_inventory_wire() {
        let head = sample_ingot();
        let stack = ItemStack {
            item: Item::CraftedPickaxe(head),
            count: Some(1),
        };
        let decoded = WireItemStack::from_stack(stack).to_stack().unwrap();
        assert_eq!(decoded.item, stack.item);
    }

    #[test]
    fn cast_ingot_round_trips_through_crucible_solid_item() {
        let original = ItemStack {
            item: Item::CastIngot(sample_ingot()),
            count: Some(1),
        };
        let restored = SimpleItem::from_stack(original).to_stack().unwrap();
        assert!(restored == original);
    }
}

fn animate_dropped_items(
    mut commands: Commands,
    time: Res<Time>,
    world: Res<crate::voxel::VoxelWorld>,
    mut query: Query<(Entity, &mut Transform, &mut DroppedItem, Option<&FlatSprite>)>,
) {
    let dt = time.delta_secs();
    for (entity, mut tf, mut dropped, flat) in query.iter_mut() {
        dropped.age += dt;
        if dropped.age >= DROP_LIFETIME {
            commands.entity(entity).despawn();
            continue;
        }
        dropped.velocity.y -= 15.0 * dt; // gravity
        
        let mut new_pos = tf.translation + dropped.velocity * dt;
        
        // Simple floor collision (เช็คเฉพาะจุด center ด้านล่างคร่าวๆ)
        let block_x = new_pos.x.floor() as i32;
        let block_y = (new_pos.y - 0.1).floor() as i32;
        let block_z = new_pos.z.floor() as i32;
        let block = world.get_block(block_x, block_y, block_z);
        
        if block != crate::voxel::BlockType::Air {
            // ชนพื้น
            new_pos.y = (block_y + 1) as f32 + 0.1; 
            dropped.velocity.y = 0.0;
            // friction
            dropped.velocity.x *= (1.0 - 5.0 * dt).max(0.0);
            dropped.velocity.z *= (1.0 - 5.0 * dt).max(0.0);
        }
        
        tf.translation = new_pos;
        // ก้อนบล็อกหมุนรอบ Y; item แบนไม่หมุน (billboard_flat_drops คุมการหันเอง)
        if flat.is_none() {
            tf.rotate_y(2.0 * dt);
        }

        // ลอยขึ้นลงเบาๆ เมื่ออยู่บนพื้น
        if dropped.velocity.y == 0.0 {
            tf.translation.y += (time.elapsed_secs() * 3.0).sin() * 0.2 * dt;
        }
    }
}

/// item แบน (FlatSprite) หันหน้าเข้ากล้องเสมอ — yaw ตามกล้อง คงตั้งตรง (up = Y)
fn billboard_flat_drops(
    camera: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut drops: Query<&mut Transform, (With<FlatSprite>, Without<crate::camera::FreeCamera>)>,
) {
    let Ok(cam) = camera.single() else { return };
    let cam_pos = cam.translation;
    for mut tf in drops.iter_mut() {
        let mut dir = cam_pos - tf.translation;
        dir.y = 0.0; // yaw อย่างเดียว ให้แผ่นตั้งตรงเสมอ
        if dir.length_squared() > 1e-6 {
            // Rectangle หน้า +Z → หมุนรอบ Y ให้ +Z ชี้เข้ากล้อง
            tf.rotation = Quat::from_rotation_y(dir.x.atan2(dir.z));
        }
    }
}
