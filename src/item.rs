use bevy::prelude::*;
use crate::voxel::BlockType;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolType {
    Chisel,
    CopperWire,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Block(BlockType),
    Tool(ToolType),
}

impl Item {
    pub fn name(&self) -> &'static str {
        match self {
            Item::Block(b) => crate::voxel::block_name(*b),
            Item::Tool(ToolType::Chisel) => "Chisel",
            Item::Tool(ToolType::CopperWire) => "Copper Wire",
        }
    }

    pub fn icon_texture(&self) -> Option<&'static str> {
        match self {
            Item::Block(b) => crate::voxel::hotbar_icon_texture(*b),
            Item::Tool(ToolType::Chisel) => Some("items/chisel.png"),
            Item::Tool(ToolType::CopperWire) => Some("items/copper_wire.png"),
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
        app.add_systems(Update, (
            spawn_dropped_item_system,
            pickup_item_system,
            animate_dropped_items,
            billboard_flat_drops,
        ).run_if(in_state(crate::GameState::InGame)));
    }
}

fn spawn_dropped_item_system(
    mut commands: Commands,
    mut events: MessageReader<SpawnDroppedItemEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    block_mats: Res<crate::voxel::BlockMaterials>,
    // mesh ใช้ร่วมทุก drop — สร้างครั้งเดียว (กันสร้าง mesh ใหม่ทุกก้อน)
    mut cube: Local<Option<Handle<Mesh>>>,
    mut quad: Local<Option<Handle<Mesh>>>,
    // material ของ item แบน (tool) cache ตาม path — กันสร้าง material ซ้ำ
    mut flat_mats: Local<std::collections::HashMap<&'static str, Handle<StandardMaterial>>>,
) {
    for ev in events.read() {
        match ev.item {
            // บล็อก → ก้อน 3D ใช้ texture ของบล็อกจริง (key เดียวกับ hotbar icon);
            // บล็อกไม่มี texture → fallback สี่เหลี่ยมสี unlit
            Item::Block(_) => {
                let mesh = cube
                    .get_or_insert_with(|| meshes.add(Cuboid::new(0.25, 0.25, 0.25)))
                    .clone();
                let material = ev
                    .item
                    .icon_texture()
                    .and_then(|path| block_mats.0.get(&path).cloned())
                    .unwrap_or_else(|| {
                        let c = ev.item.color();
                        materials.add(StandardMaterial {
                            base_color: Color::srgba(c[0], c[1], c[2], c[3]),
                            unlit: true,
                            ..default()
                        })
                    });
                commands.spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Transform::from_translation(ev.pos),
                    DroppedItem { item: ev.item, count: 1, velocity: ev.velocity, age: 0.0 },
                ));
            }
            // item ที่ไม่ใช่บล็อก (tool) → แผ่นแบนหันเข้ากล้อง (billboard) ใช้รูป items/*.png
            Item::Tool(_) => {
                let mesh = quad
                    .get_or_insert_with(|| meshes.add(Rectangle::new(0.4, 0.4)))
                    .clone();
                let material = match ev.item.icon_texture() {
                    Some(path) => flat_mats
                        .entry(path)
                        .or_insert_with(|| {
                            materials.add(StandardMaterial {
                                base_color: Color::WHITE,
                                base_color_texture: Some(asset_server.load(path)),
                                alpha_mode: AlphaMode::Blend, // PNG โปร่งใส
                                unlit: true,
                                cull_mode: None, // เห็นทั้งสองด้าน
                                ..default()
                            })
                        })
                        .clone(),
                    None => {
                        let c = ev.item.color();
                        materials.add(StandardMaterial {
                            base_color: Color::srgba(c[0], c[1], c[2], c[3]),
                            unlit: true,
                            cull_mode: None,
                            ..default()
                        })
                    }
                };
                commands.spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Transform::from_translation(ev.pos),
                    DroppedItem { item: ev.item, count: 1, velocity: ev.velocity, age: 0.0 },
                    FlatSprite,
                ));
            }
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
