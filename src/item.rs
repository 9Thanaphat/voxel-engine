use bevy::prelude::*;
use crate::voxel::{BlockType, ItemStack};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolType {
    Chisel,
    CopperWire,
    Pickaxe,
    Axe,
    Shovel,
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
        }
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(ToolType::Chisel),
            1 => Some(ToolType::CopperWire),
            2 => Some(ToolType::Pickaxe),
            3 => Some(ToolType::Axe),
            4 => Some(ToolType::Shovel),
            _ => None,
        }
    }

    /// tool นี้ถนัดขุดบล็อกหมวดไหน (Chisel/CopperWire ไม่ใช่เครื่องมือขุด)
    pub fn dig_class(self) -> DigClass {
        match self {
            ToolType::Pickaxe => DigClass::Pick,
            ToolType::Axe => DigClass::Axe,
            ToolType::Shovel => DigClass::Shovel,
            ToolType::Chisel | ToolType::CopperWire => DigClass::None,
        }
    }

    /// ตัวคูณความเร็วเมื่อขุดบล็อกหมวดที่ตัวเองถนัด — จุดต่อยอด tier ในอนาคต
    /// (ไม้/หิน/เหล็ก = คืนค่าต่างกันตรงนี้ที่เดียว)
    pub fn dig_speed(self) -> f32 {
        match self {
            ToolType::Pickaxe | ToolType::Axe | ToolType::Shovel => 5.0,
            ToolType::Chisel | ToolType::CopperWire => 1.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Block(BlockType),
    Tool(ToolType),
}

/// เข้ารหัส Item เป็น (kind, id) สำหรับเซฟลง disk / ส่งข้าม network — ไม่ derive serde
/// บน BlockType/Item ตรงๆ (ตาม convention เดิมของโปรเจกต์ที่ใช้ as u8/from_u8)
/// kind: 0 = Block, 1 = Tool
pub fn item_to_wire(item: Item) -> (u8, u8) {
    match item {
        Item::Block(b) => (0, b as u8),
        Item::Tool(t) => (1, t.to_u8()),
    }
}

pub fn item_from_wire(kind: u8, id: u8) -> Option<Item> {
    match kind {
        0 => Some(Item::Block(BlockType::from_u8(id))),
        1 => ToolType::from_u8(id).map(Item::Tool),
        _ => None,
    }
}

/// รูปแบบ ItemStack บนสายส่ง (เซฟลง disk / ส่งข้าม network) — ตัวเดียวที่ derive serde
/// เก็บ BlockType/Item ให้ปลอดจาก serde ตาม convention เดิมของโปรเจกต์
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub struct WireItemStack {
    pub kind: u8,
    pub id: u8,
    pub count: Option<u32>,
}

impl WireItemStack {
    pub fn from_stack(s: ItemStack) -> Self {
        let (kind, id) = item_to_wire(s.item);
        Self { kind, id, count: s.count }
    }

    pub fn to_stack(self) -> Option<ItemStack> {
        item_from_wire(self.kind, self.id).map(|item| ItemStack { item, count: self.count })
    }
}

impl Item {
    /// เลิกใช้ตอน block picker แบบ egui ถูกแทนด้วยกริดไอคอน — เก็บไว้สำหรับ tooltip
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            Item::Block(b) => crate::voxel::block_name(*b),
            Item::Tool(ToolType::Chisel) => "Chisel",
            Item::Tool(ToolType::CopperWire) => "Copper Wire",
            Item::Tool(ToolType::Pickaxe) => "Pickaxe",
            Item::Tool(ToolType::Axe) => "Axe",
            Item::Tool(ToolType::Shovel) => "Shovel",
        }
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
        }
    }

    /// icon สำหรับ UI จริง — Block ใช้ภาพที่ render 3 มิติไว้แล้ว (ดู ItemIconCache/start_icon_bake),
    /// Tool ยังใช้ .png แบนเหมือนเดิม (ไม่ใช่บล็อก ไม่มีปัญหาเรื่องหน้าไม่เหมือนกัน)
    pub fn icon_image(
        &self,
        icons: &crate::voxel::ItemIconCache,
        asset_server: &AssetServer,
    ) -> Option<Handle<Image>> {
        match self {
            Item::Block(b) => icons.0.get(b).cloned(),
            // tool ใช้ .png แบนผ่าน icon_texture ทางเดียวกันทุกตัว
            Item::Tool(_) => self.icon_texture().map(|path| asset_server.load(path)),
        }
    }

    pub fn color(&self) -> [f32; 4] {
        match self {
            Item::Block(b) => crate::voxel::block_color(*b),
            Item::Tool(_) => [1.0, 1.0, 1.0, 1.0],
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
    campfire_assets: Res<crate::voxel::CampfireAssets>,
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
        ToolType::Pickaxe => "items/copper_pickaxe.gltf",
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
    campfire_assets: &crate::voxel::CampfireAssets,
    item: Item,
    size: f32,
    transform: Transform,
) -> Entity {
    use bevy::light::NotShadowCaster;
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
        // tool อื่น → แผ่นแบน icon png
        Item::Tool(_) => {
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
            commands.spawn((
                Mesh3d(meshes.add(Rectangle::new(size, size))),
                MeshMaterial3d(material),
                transform,
                NotShadowCaster,
            )).id()
        }
    }
}

fn spawn_dropped_item_system(
    mut commands: Commands,
    mut events: MessageReader<SpawnDroppedItemEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    block_mats: Res<crate::voxel::BlockMaterials>,
    campfire_assets: Res<crate::voxel::CampfireAssets>,
) {
    for ev in events.read() {
        // ขนาดของตกพื้น: บล็อกคิวบ์เล็ก, tool โมเดลจริงใหญ่หน่อย, แผ่นแบนกลางๆ
        let size = match ev.item {
            Item::Block(_) => 0.25,
            Item::Tool(t) if tool_model_path(t).is_some() => 0.5,
            Item::Tool(_) => 0.4,
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
