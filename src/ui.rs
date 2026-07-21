use bevy::prelude::*;
use crate::camera::FreeCamera;

#[derive(Component)]
pub struct CoordinateText;

#[derive(Component)]
pub struct FpsText;

#[derive(Component)]
pub struct BlockIdText;

#[derive(Component)]
pub struct ModeText;

#[derive(Component)]
pub struct InGameUi;

#[derive(Component)]
pub struct DebugMenuUi;

#[derive(Resource, Default)]
pub struct ShowDebugMenu(pub bool);

/// กรอบช่อง hotbar (index 0..9) — border เปลี่ยนสีตามช่องที่เลือก
#[derive(Component)]
pub struct HotbarSlotUi(pub usize);

/// icon ข้างในช่อง — เป็น ImageNode (บล็อกมี texture) หรือสี่เหลี่ยมสี (ไม่มี)
#[derive(Component)]
pub struct HotbarSlotIcon(pub usize);

/// เลขจำนวนมุมล่างขวาช่อง (Survival) — ว่างเมื่อ count = None (Creative ∞)
#[derive(Component)]
pub struct HotbarSlotCount(pub usize);

// --- หน้าต่างช่องเก็บของ (กด E) ---

/// รากของหน้าต่าง — ซ่อน/โชว์ทั้งก้อนด้วย Visibility
#[derive(Component)]
pub struct InventoryRoot;

/// แถบ palette (Creative เท่านั้น) — ซ่อนแยกจาก root ตามโหมด
#[derive(Component)]
pub struct InventoryPalette;

/// ช่องในหน้าต่าง — เก็บ index จริงใน `Hotbar.slots` (0..TOTAL_SLOTS)
#[derive(Component)]
pub struct InvSlotUi(pub usize);

#[derive(Component)]
pub struct InvSlotIcon(pub usize);

#[derive(Component)]
pub struct InvSlotCount(pub usize);

/// ช่องใน palette — index ใน `voxel::PLACEABLE_ITEMS`
#[derive(Component)]
pub struct PaletteSlotUi(pub usize);

/// icon ข้างในช่อง palette — แยก marker ไว้ให้ตั้ง texture ทีหลังใน Update (ดู bake_palette_icons)
#[derive(Component)]
pub struct PaletteSlotIcon(pub usize);

// --- กริดของ Chest/Furnace ที่เปิดอยู่ (แยกจาก InvSlotUi เพราะ backing array คนละตัว) ---

/// ครอบกริด container ทั้งก้อน — ซ่อน/โชว์ตาม `voxel::OpenContainer`
#[derive(Component)]
pub struct ContainerPanel;

/// ช่องในกริด container — index 0..27 (Furnace ใช้แค่ 0..3, ที่เหลือซ่อน)
#[derive(Component)]
pub struct ContainerSlotUi(pub usize);

#[derive(Component)]
pub struct ContainerSlotIcon(pub usize);

#[derive(Component)]
pub struct ContainerSlotCount(pub usize);

/// icon ของกองที่กำลังถืออยู่บนเมาส์
#[derive(Component)]
pub struct HeldStackIcon;

/// ป้ายชื่อ item เด้งเหนือ hotbar ตอนสลับช่อง (จางหายเอง) — ตัวครอบ (คุม Visibility)
#[derive(Component)]
pub struct HotbarItemNameRoot;

/// ตัว Text ข้างใน HotbarItemNameRoot
#[derive(Component)]
pub struct HotbarItemNameText;

/// บรรทัดชื่อ item ที่เมาส์ hover อยู่ ท้ายหน้าต่างช่องเก็บของ
#[derive(Component)]
pub struct InvHoverNameText;

/// กริด Furnace เฉพาะ (input/fuel → output) — สลับโชว์กับ ContainerPanel ตามชนิดกล่อง
#[derive(Component)]
pub struct FurnacePanel;

/// ตัวครอบ hotbar ล่างจอ (HUD) — ซ่อนตอนหน้าต่างช่องเก็บของเปิด (ในหน้าต่างมีแถว
/// hotbar ของตัวเองอยู่แล้ว โชว์คู่กันซ้ำซ้อน)
#[derive(Component)]
pub struct HudHotbarRoot;

/// สลับ node เข้า/ออกจาก layout — Visibility::Hidden อย่างเดียวยัง "กินพื้นที่" อยู่
/// (หน้าต่างเลยสูงโบ๋เพราะกริดที่ซ่อน) ต้อง Display::None ควบด้วยถึงจะ fit เนื้อหาจริง
fn set_shown(node: &mut Node, vis: &mut Visibility, show: bool) {
    let d = if show { Display::Flex } else { Display::None };
    if node.display != d {
        node.display = d;
    }
    let v = if show { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != v {
        *vis = v;
    }
}

/// กองที่ "ถืออยู่บนเมาส์" ระหว่างจัดของ — ต้องคืนเข้าช่อง/ทิ้งลงโลกตอนปิดหน้าต่าง
#[derive(Resource, Default)]
pub struct HeldStack(pub Option<crate::voxel::ItemStack>);

/// จอขาววาบตอนมองระเบิด — alpha ตาม ScreenFlash.intensity
#[derive(Component)]
pub struct ScreenFlashOverlay;

/// ความจ้าที่ค้างอยู่บนจอ (ตั้งโดยระบบ trigger ใน particles.rs, decay ที่นี่)
#[derive(Resource, Default)]
pub struct ScreenFlash {
    pub intensity: f32,
    /// อัตรา decay แบบ exponential ต่อวินาที (TNT เร็ว / nuke ช้า = ตาพร่านาน)
    pub decay: f32,
}

/// ช่องกรอก lat/lon สำหรับ teleport ในโลกจริง (โหมด RealWorld) + ข้อความสถานะ
#[derive(Resource, Default)]
pub struct TeleportUi {
    pub lat: String,
    pub lon: String,
    pub status: String,
}

// --------------------------------------------------------
// Chat
// --------------------------------------------------------

/// จำนวนบรรทัดที่แสดงบนจอ (log เก็บมากกว่านี้ ส่วนเกินเลื่อนขึ้นหาย)
pub const CHAT_VISIBLE_LINES: usize = 12;
/// เก็บย้อนหลังไว้เท่านี้ — เปิดแชทถึงจะเห็นเกิน CHAT_VISIBLE_LINES ไม่ได้อยู่ดี
/// แต่กันไว้เผื่อทำ scroll ทีหลัง
const CHAT_LOG_CAP: usize = 100;
/// ปิดแชทอยู่ บรรทัดที่เก่ากว่านี้ (วินาที) จะซ่อน — แบบ Minecraft
const CHAT_FADE_SECONDS: f32 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChatKind {
    /// คนพูด (รวมตัวเอง)
    Player,
    /// ระบบ/ผลของคำสั่ง
    System,
    /// คำสั่งผิดพลาด
    Error,
}

impl ChatKind {
    fn color(self) -> Color {
        match self {
            ChatKind::Player => Color::WHITE,
            ChatKind::System => Color::srgb(1.0, 0.85, 0.3),
            ChatKind::Error => Color::srgb(1.0, 0.4, 0.4),
        }
    }
}

pub struct ChatLine {
    pub text: String,
    pub kind: ChatKind,
    /// อายุตั้งแต่ถูกเพิ่ม (วินาที) — ใช้ตัดสินว่าจางไปหรือยังตอนปิดแชท
    pub age: f32,
}

#[derive(Resource, Default)]
pub struct ChatState {
    pub log: std::collections::VecDeque<ChatLine>,
    pub input: String,
    pub open: bool,
    /// คำสั่ง/ข้อความที่เคยส่ง — ลูกศรขึ้น/ลงเรียกกลับมา (ใหม่สุดอยู่ท้าย)
    pub history: Vec<String>,
    pub history_pos: Option<usize>,
    /// เพิ่งเปิดแชทเฟรมนี้ — ให้ TextEdit ขอ focus
    pub focus_requested: bool,
}

impl ChatState {
    fn push(&mut self, text: String, kind: ChatKind) {
        self.log.push_back(ChatLine { text, kind, age: 0.0 });
        while self.log.len() > CHAT_LOG_CAP {
            self.log.pop_front();
        }
    }

    /// ข้อความจากผู้เล่น — from = player_number (0 = ไม่รู้ว่าใคร)
    pub fn push_player(&mut self, from: u32, text: String) {
        let who = if from == 0 {
            "Player ?".to_string()
        } else {
            format!("Player {from}")
        };
        self.push(format!("<{who}> {text}"), ChatKind::Player);
    }

    pub fn push_system(&mut self, text: impl Into<String>) {
        self.push(text.into(), ChatKind::System);
    }

    pub fn push_error(&mut self, text: impl Into<String>) {
        self.push(text.into(), ChatKind::Error);
    }
}

/// บรรทัดที่ i ของ chat log บนจอ (0 = บนสุด) — อัปเดตในที่แบบเดียวกับ HotbarSlotUi
#[derive(Component)]
pub struct ChatLineUi(pub usize);

/// ช่องหนึ่งช่องในหน้าต่างช่องเก็บของ — สไตล์เดียวกับช่อง hotbar ล่างจอเป๊ะ
/// `idx` คือ index จริงใน `Hotbar.slots` ทำให้ระบบคลิก/วาดอ่านตรงเข้า array ได้เลย
fn spawn_inv_slot(
    parent: &mut bevy::ecs::relationship::RelatedSpawnerCommands<ChildOf>,
    idx: usize,
) {
    parent
        .spawn((
            Node {
                width: Val::Px(52.0),
                height: Val::Px(52.0),
                border: UiRect::all(Val::Px(3.0)),
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.7)),
            BorderColor::all(Color::srgba(0.2, 0.2, 0.2, 0.6)),
            Interaction::default(),
            InvSlotUi(idx),
        ))
        .with_children(|slot| {
            slot.spawn((
                Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
                BackgroundColor(Color::NONE),
                InvSlotIcon(idx),
            ));
            slot.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(2.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                Text::new(""),
                TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() },
                TextColor(Color::WHITE),
                InvSlotCount(idx),
            ));
        });
}

/// ช่องหนึ่งช่องในกริด Chest/Furnace ที่เปิดอยู่ — สไตล์เดียวกับ spawn_inv_slot เป๊ะ
/// ต่างกันแค่ marker (ChestSlotUi อ่าน container ที่เปิดอยู่ ไม่ใช่ Hotbar.slots ตรงๆ)
fn spawn_container_slot(
    parent: &mut bevy::ecs::relationship::RelatedSpawnerCommands<ChildOf>,
    idx: usize,
) {
    parent
        .spawn((
            Node {
                width: Val::Px(52.0),
                height: Val::Px(52.0),
                border: UiRect::all(Val::Px(3.0)),
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.7)),
            BorderColor::all(Color::srgba(0.2, 0.2, 0.2, 0.6)),
            Interaction::default(),
            ContainerSlotUi(idx),
        ))
        .with_children(|slot| {
            slot.spawn((
                Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
                BackgroundColor(Color::NONE),
                ContainerSlotIcon(idx),
            ));
            slot.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(2.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                Text::new(""),
                TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() },
                TextColor(Color::WHITE),
                ContainerSlotCount(idx),
            ));
        });
}

pub fn setup_ui(mut commands: Commands) {
    // Crosshair
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        InGameUi,
        Visibility::Hidden,
    )).with_children(|parent| {
        // เส้นตั้ง
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(2.0),
                height: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.9)),
        ));
        // เส้นนอน
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(16.0),
                height: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.9)),
        ));
    });

    // จอขาววาบตอนมองระเบิด — ทับทุกอย่าง (GlobalZIndex สูง) เริ่มโปร่งใสสนิท
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
        bevy::ui::GlobalZIndex(100),
        ScreenFlashOverlay,
        InGameUi,
        Visibility::Hidden,
    ));

    // Chat log ซ้ายล่าง เหนือ hotbar (hotbar อยู่ bottom 10 สูง 52) — บรรทัดว่างไว้ก่อน
    // เติมข้อความ/สี/visibility โดย update_chat_ui
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(80.0),
            left: Val::Px(10.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            ..default()
        },
        InGameUi,
        Visibility::Hidden,
    )).with_children(|parent| {
        for i in 0..CHAT_VISIBLE_LINES {
            parent.spawn((
                Text::new(""),
                TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() },
                TextColor(Color::WHITE),
                // พื้นหลังจางๆ ให้อ่านออกบนท้องฟ้าสว่าง
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
                ChatLineUi(i),
            ));
        }
    });

    // Hotbar 9 ช่อง ล่างกลางจอ — icon/กรอบเติมโดย update_hotbar_ui เฟรมแรก
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(4.0),
            ..default()
        },
        InGameUi,
        HudHotbarRoot,
        Visibility::Hidden,
    )).with_children(|parent| {
        for i in 0..9 {
            parent.spawn((
                Node {
                    width: Val::Px(52.0),
                    height: Val::Px(52.0),
                    border: UiRect::all(Val::Px(3.0)),
                    padding: UiRect::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.7)),
                BorderColor::all(Color::srgba(0.2, 0.2, 0.2, 0.6)),
                HotbarSlotUi(i),
            )).with_children(|slot| {
                slot.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                    HotbarSlotIcon(i),
                ));
                // เลขจำนวน (Survival) มุมล่างขวา ทับบน icon
                slot.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        right: Val::Px(2.0),
                        bottom: Val::Px(0.0),
                        ..default()
                    },
                    Text::new(""),
                    TextFont {
                        font_size: bevy::text::FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    HotbarSlotCount(i),
                ));
            });
        }
    });

    // ป้ายชื่อ item เหนือ hotbar — เด้งตอนสลับช่อง แล้วจางหาย (hotbar_item_name_system)
    // ไม่ติด InGameUi เพราะ toggle_ingame_ui จะบังคับโชว์ตลอด — ระบบคุม Visibility เอง
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(72.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        HotbarItemNameRoot,
        Visibility::Hidden,
    )).with_children(|parent| {
        parent.spawn((
            Text::new(""),
            TextFont { font_size: bevy::text::FontSize::Px(18.0), ..default() },
            TextColor(Color::WHITE),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            HotbarItemNameText,
        ));
    });

    // หน้าต่างช่องเก็บของ (กด E) — pre-spawn ทั้งกริดแล้วอัปเดตในที่แบบเดียวกับ hotbar
    // ไม่ติด InGameUi เพราะ toggle_ingame_ui จะบังคับให้โผล่ตลอดเวลาที่อยู่ในเกม
    // ความมองเห็นคุมโดย update_inventory_ui ตาม InventoryOpen แทน
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(100),
            InventoryRoot,
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(16.0)),
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.08, 0.08, 0.11, 0.96)),
            ))
            .with_children(|panel| {
                // 0. กริด Chest ที่เปิดอยู่ (คลิกขวามือเปล่าใส่บล็อก) — ซ่อนโดย
                // ปริยาย โผล่เฉพาะตอน OpenContainer เป็น Chest (ดู update_container_ui)
                panel
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(4.0),
                            margin: UiRect::bottom(Val::Px(10.0)),
                            ..default()
                        },
                        ContainerPanel,
                        Visibility::Hidden,
                    ))
                    .with_children(|cont| {
                        for row in 0..3usize {
                            cont.spawn(Node { column_gap: Val::Px(4.0), ..default() })
                                .with_children(|r| {
                                    for col in 0..9usize {
                                        spawn_container_slot(r, row * 9 + col);
                                    }
                                });
                        }
                    });

                // 0b. กริด Furnace เฉพาะ: Input+Fuel ซ้าย → Output ขวา พร้อม label
                // (ช่อง 0/1/2 ซ้ำ index กับกริด Chest ได้ — โชว์ทีละ panel เท่านั้น
                // และ node ที่ซ่อนไม่รับ hover/คลิก จึงไม่ตีกัน; ยังไม่มีระบบเผาจริง
                // — จัดตามความหมายช่องใน voxel.rs ไว้ก่อน)
                let slot_label = |p: &mut bevy::ecs::relationship::RelatedSpawnerCommands<ChildOf>, text: &str| {
                    p.spawn((
                        Text::new(text),
                        TextFont { font_size: bevy::text::FontSize::Px(13.0), ..default() },
                        TextColor(Color::srgba(0.8, 0.8, 0.8, 0.9)),
                    ));
                };
                panel
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(14.0),
                            margin: UiRect::bottom(Val::Px(10.0)),
                            ..default()
                        },
                        FurnacePanel,
                        Visibility::Hidden,
                    ))
                    .with_children(|f| {
                        f.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(4.0),
                            ..default()
                        })
                        .with_children(|c| {
                            slot_label(c, "Input");
                            spawn_container_slot(c, 0);
                            slot_label(c, "Fuel");
                            spawn_container_slot(c, 1);
                        });
                        f.spawn((
                            Text::new(">"),
                            TextFont { font_size: bevy::text::FontSize::Px(30.0), ..default() },
                            TextColor(Color::srgba(1.0, 0.85, 0.2, 0.9)),
                        ));
                        f.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(4.0),
                            ..default()
                        })
                        .with_children(|c| {
                            slot_label(c, "Output");
                            spawn_container_slot(c, 2);
                        });
                    });

                // 1. palette ของ Creative — 10 คอลัมน์ x เท่าที่ PLACEABLE_ITEMS ต้องใช้ (ซ่อนใน Survival)
                // คำนวณจำนวนแถวจากความยาวจริงแทนที่จะ hardcode 2 แถว — ก่อนหน้านี้ hardcode ไว้ที่
                // 2x10=20 ช่อง ตอนเพิ่ม Furnace/Chest (ดัน PLACEABLE_ITEMS เป็น 22 ตัว) เลยตกกริดไปเงียบๆ
                panel
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(4.0),
                            margin: UiRect::bottom(Val::Px(10.0)),
                            ..default()
                        },
                        InventoryPalette,
                    ))
                    .with_children(|pal| {
                        let items = crate::voxel::PLACEABLE_ITEMS;
                        const PALETTE_COLS: usize = 10;
                        let palette_rows = items.len().div_ceil(PALETTE_COLS);
                        for row in 0..palette_rows {
                            pal.spawn(Node { column_gap: Val::Px(4.0), ..default() })
                                .with_children(|r| {
                                    for col in 0..PALETTE_COLS {
                                        let i = row * PALETTE_COLS + col;
                                        if items.get(i).is_none() { continue; }
                                        r.spawn((
                                            Node {
                                                width: Val::Px(52.0),
                                                height: Val::Px(52.0),
                                                border: UiRect::all(Val::Px(3.0)),
                                                padding: UiRect::all(Val::Px(4.0)),
                                                ..default()
                                            },
                                            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.7)),
                                            BorderColor::all(Color::srgba(0.2, 0.2, 0.2, 0.6)),
                                            Interaction::default(),
                                            PaletteSlotUi(i),
                                        ))
                                        .with_children(|slot| {
                                            // palette ไม่เปลี่ยนตลอดเกม แต่ห้ามใส่ icon ตอน spawn ตรงนี้:
                                            // setup_ui เป็น Startup system ลำดับไม่การันตีว่าจะรันหลัง
                                            // setup_voxel (ซึ่งเป็นคน init FACE_TEXTURES) — ใส่ตอนนี้
                                            // มีโอกาสได้ FACE_TEXTURES ว่างแล้วเห็นเป็นสีพื้นทั้งอัน
                                            // ให้ bake_palette_icons (Update, รันหลัง Startup เสร็จ
                                            // แน่นอน) มาใส่แทนทีหลัง — ดูคอมเมนต์เดียวกันใน update_hotbar_ui
                                            slot.spawn((
                                                Node {
                                                    width: Val::Percent(100.0),
                                                    height: Val::Percent(100.0),
                                                    ..default()
                                                },
                                                BackgroundColor(Color::NONE),
                                                PaletteSlotIcon(i),
                                            ));
                                        });
                                    }
                                });
                        }
                    });

                // 2. ช่องเก็บของ (index HOTBAR_SLOTS..TOTAL_SLOTS)
                for row in 0..crate::voxel::INV_ROWS {
                    panel
                        .spawn(Node { column_gap: Val::Px(4.0), ..default() })
                        .with_children(|r| {
                            for col in 0..crate::voxel::INV_COLS {
                                let idx = crate::voxel::HOTBAR_SLOTS + row * crate::voxel::INV_COLS + col;
                                spawn_inv_slot(r, idx);
                            }
                        });
                }

                // 3. เว้นช่องแล้วต่อด้วยแถบ hotbar (index 0..HOTBAR_SLOTS) ให้ลากของลงได้
                panel.spawn(Node { height: Val::Px(10.0), ..default() });
                panel
                    .spawn(Node { column_gap: Val::Px(4.0), ..default() })
                    .with_children(|r| {
                        for idx in 0..crate::voxel::HOTBAR_SLOTS {
                            spawn_inv_slot(r, idx);
                        }
                    });

                // 4. บรรทัดชื่อ item ที่ hover อยู่ (ย้ายไปเป็น UI ลอยแทน)
            });
        });

    // tooltip ชื่อ item ที่ hover อยู่ — ลอยตามเมาส์
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            padding: UiRect::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)),
        GlobalZIndex(102),
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() },
        TextColor(Color::srgb(1.0, 0.85, 0.2)),
        Visibility::Hidden,
        InvHoverNameText,
    ));

    // icon ของกองที่ถืออยู่ — ลอยตามเมาส์ ต้องอยู่นอกหน้าต่างเพื่อไม่ให้ layout ดันตำแหน่ง
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(40.0),
            height: Val::Px(40.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
        GlobalZIndex(101),
        HeldStackIcon,
        Visibility::Hidden,
    ));

    // Debug Menu (F3 Style Panel)
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(10.0),
                top: Val::Px(10.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(10.0)),
                row_gap: Val::Px(5.0),
                align_items: AlignItems::FlexEnd,
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.6)),
            InGameUi,
            DebugMenuUi,
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            // Performance Text
            parent.spawn((
                Text::new("FPS: 0, Time: 0ms, Entities: 0\nDraw Calls: 0, Polygons: 0, VRAM: 0 MB\nRAM: 0 MB"),
                TextFont {
                    font_size: bevy::text::FontSize::Px(16.0),
                    ..default()
                },
                bevy::text::TextLayout::justify(bevy::text::Justify::Right),
                TextColor(Color::WHITE),
                FpsText,
            ));

            // Coordinate Text
            parent.spawn((
                Text::new("X: 0.00, Y: 0.00, Z: 0.00"),
                TextFont {
                    font_size: bevy::text::FontSize::Px(16.0),
                    ..default()
                },
                bevy::text::TextLayout::justify(bevy::text::Justify::Right),
                TextColor(Color::WHITE),
                CoordinateText,
            ));

            // Block Target / Hotbar Text
            parent.spawn((
                Text::new("Block: None"),
                TextFont {
                    font_size: bevy::text::FontSize::Px(16.0),
                    ..default()
                },
                bevy::text::TextLayout::justify(bevy::text::Justify::Right),
                TextColor(Color::WHITE),
                BlockIdText,
            ));

            // Mode Text
            parent.spawn((
                Text::new("Mode: Normal"),
                TextFont {
                    font_size: bevy::text::FontSize::Px(16.0),
                    ..default()
                },
                bevy::text::TextLayout::justify(bevy::text::Justify::Right),
                TextColor(Color::WHITE),
                ModeText,
            ));
        });
}

/// ใส่ icon ให้ช่อง palette ครั้งเดียวหลังเฟรมแรก — palette ไม่เปลี่ยนตลอดเกมเลยไม่ต้อง
/// ทำซ้ำทุกเฟรมแบบ update_hotbar_ui แต่เหตุผลที่ต้องรอ Update (ไม่ใส่ตอน spawn ใน setup_ui)
/// เหมือนกัน: setup_ui/setup_voxel เป็น Startup ทั้งคู่ ลำดับกันเองไม่การันตี ส่วน Update
/// การันตีว่ารันหลัง Startup ทั้งหมดเสร็จเสมอ — FACE_TEXTURES จาก setup_voxel เลยพร้อมแน่นอน
/// ต้องรันหลัง voxel::start_icon_bake (register .after() ใน main.rs) — ตัวนั้นเป็นคนใส่
/// handle ของ ItemIconCache ให้ (ตัวรูปจริงจะโผล่ทีหลังเมื่อกล้อง render เสร็จ ไม่ต้องรอ)
pub fn bake_palette_icons(
    mut done: Local<bool>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    icons: Res<crate::voxel::ItemIconCache>,
    mut icon_query: Query<(Entity, &PaletteSlotIcon, &mut BackgroundColor)>,
) {
    if *done {
        return;
    }
    *done = true;
    for (entity, icon, mut bg) in &mut icon_query {
        let Some(item) = crate::voxel::PLACEABLE_ITEMS.get(icon.0).copied() else { continue };
        match item.icon_image(&icons, &asset_server) {
            Some(image) => {
                commands.entity(entity).insert(ImageNode::new(image));
                bg.0 = Color::NONE;
            }
            None => {
                let c = item.color();
                bg.0 = Color::srgba(c[0], c[1], c[2], c[3]);
            }
        }
    }
}

/// อัปเดตกรอบ+icon ของ hotbar — ทำงานเฉพาะตอน Hotbar เปลี่ยน (รวมเฟรมแรก)
/// เลี่ยงปัญหาลำดับ Startup: FACE_TEXTURES ถูก init ใน setup_voxel ซึ่งเสร็จ
/// ก่อน Update เฟรมแรกแน่นอน
pub fn update_hotbar_ui(
    hotbar: Res<crate::voxel::Hotbar>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    icons: Res<crate::voxel::ItemIconCache>,
    mut slot_query: Query<(&HotbarSlotUi, &mut BorderColor)>,
    mut icon_query: Query<(Entity, &HotbarSlotIcon, &mut BackgroundColor)>,
    mut count_query: Query<(&HotbarSlotCount, &mut Text)>,
) {
    if !hotbar.is_changed() {
        return;
    }

    // เลขจำนวน: โชว์เมื่อ count = Some(n) (Survival), ว่างเมื่อ None (Creative ∞)
    for (count, mut text) in &mut count_query {
        let s = match hotbar.slots[count.0].and_then(|st| st.count) {
            Some(n) => n.to_string(),
            None => String::new(),
        };
        if text.0 != s {
            text.0 = s;
        }
    }

    for (slot, mut border) in &mut slot_query {
        *border = if slot.0 == hotbar.selected {
            // ไฮไลต์สีเหลืองทองเวลาเลือก
            BorderColor::all(Color::srgb(1.0, 0.85, 0.2))
        } else {
            BorderColor::all(Color::srgba(0.2, 0.2, 0.2, 0.6))
        };
    }

    for (entity, icon, mut bg) in &mut icon_query {
        match hotbar.slots[icon.0] {
            Some(stack) => {
                if let Some(image) = stack.item.icon_image(&icons, &asset_server) {
                    commands.entity(entity).insert(ImageNode::new(image));
                    bg.0 = Color::NONE;
                } else {
                    commands.entity(entity).remove::<ImageNode>();
                    let c = stack.item.color();
                    bg.0 = Color::srgba(c[0], c[1], c[2], c[3]);
                }
            }
            None => {
                commands.entity(entity).remove::<ImageNode>();
                bg.0 = Color::NONE;
            }
        }
    }
}

// --------------------------------------------------------
// หน้าต่างช่องเก็บของ (กด E)
//
// กริดถูก pre-spawn ครบตั้งแต่ setup_ui แล้วอัปเดตในที่ — แพทเทิร์นเดียวกับ
// hotbar ล่างจอ (update_hotbar_ui) ต่างกันแค่ index ครอบทั้ง Hotbar.slots
// --------------------------------------------------------

/// ยัดกองลงช่วง `range` ของ slots — เติมกองเดิมที่ชนิดตรงกันก่อน แล้วค่อยลงช่องว่าง
/// คืนส่วนที่ยัดไม่ลง (None = ลงหมด) — ตรรกะเดียวกับ pickup_dropped_items ใน item.rs
fn insert_into(
    slots: &mut [Option<crate::voxel::ItemStack>],
    range: std::ops::Range<usize>,
    stack: crate::voxel::ItemStack,
) -> Option<crate::voxel::ItemStack> {
    let max = crate::voxel::max_stack(stack.item);
    let mut remaining = stack.count.unwrap_or(max);

    for i in range.clone() {
        if remaining == 0 {
            break;
        }
        if let Some(s) = slots[i].as_mut() {
            if s.item == stack.item {
                let cur = s.count.unwrap_or(max);
                let take = remaining.min(max.saturating_sub(cur));
                s.count = Some(cur + take);
                remaining -= take;
            }
        }
    }
    for i in range {
        if remaining == 0 {
            break;
        }
        if slots[i].is_none() {
            let take = remaining.min(max);
            slots[i] = Some(crate::voxel::ItemStack { item: stack.item, count: Some(take) });
            remaining -= take;
        }
    }

    (remaining > 0).then_some(crate::voxel::ItemStack {
        item: stack.item,
        count: Some(remaining),
    })
}

/// Shift+คลิก: เด้งกองไปอีกฝั่ง (แถบล่างจอ ↔ ช่องเก็บของ)
fn quick_move(hotbar: &mut crate::voxel::Hotbar, idx: usize) {
    use crate::voxel::{HOTBAR_SLOTS, TOTAL_SLOTS};
    let Some(stack) = hotbar.slots[idx] else { return };
    let target = if idx < HOTBAR_SLOTS { HOTBAR_SLOTS..TOTAL_SLOTS } else { 0..HOTBAR_SLOTS };
    hotbar.slots[idx] = None;
    if let Some(left) = insert_into(&mut hotbar.slots, target, stack) {
        // อีกฝั่งเต็ม — คืนกลับช่องเดิม (ไม่ทำของหาย)
        hotbar.slots[idx] = Some(left);
    }
}

/// คลิกซ้าย: หยิบทั้งกอง / วางทั้งกอง / รวมกองถ้าชนิดเดียวกัน / สลับถ้าคนละชนิด
fn click_left(
    slot: &mut Option<crate::voxel::ItemStack>,
    held: &mut Option<crate::voxel::ItemStack>,
) {
    use crate::voxel::{max_stack, ItemStack};
    match (held.take(), *slot) {
        (None, Some(s)) => {
            *held = Some(s);
            *slot = None;
        }
        (Some(h), None) => *slot = Some(h),
        (Some(h), Some(s)) if s.item == h.item => {
            let max = max_stack(h.item);
            let (sc, hc) = (s.count.unwrap_or(max), h.count.unwrap_or(max));
            let moved = hc.min(max.saturating_sub(sc));
            *slot = Some(ItemStack { item: h.item, count: Some(sc + moved) });
            let left = hc - moved;
            *held = (left > 0).then_some(ItemStack { item: h.item, count: Some(left) });
        }
        (Some(h), Some(s)) => {
            *slot = Some(h);
            *held = Some(s);
        }
        (None, None) => {}
    }
}

/// คลิกขวา: มือว่าง = แบ่งครึ่งกองขึ้นมือ, ถือของอยู่ = วางทีละชิ้น
fn click_right(
    slot: &mut Option<crate::voxel::ItemStack>,
    held: &mut Option<crate::voxel::ItemStack>,
) {
    use crate::voxel::{max_stack, ItemStack};
    match (held.take(), *slot) {
        (None, Some(s)) => {
            let max = max_stack(s.item);
            let c = s.count.unwrap_or(max);
            let take = c.div_ceil(2);
            *held = Some(ItemStack { item: s.item, count: Some(take) });
            *slot = (c - take > 0).then_some(ItemStack { item: s.item, count: Some(c - take) });
        }
        (Some(h), None) => {
            let hc = h.count.unwrap_or(max_stack(h.item));
            *slot = Some(ItemStack { item: h.item, count: Some(1) });
            *held = (hc > 1).then_some(ItemStack { item: h.item, count: Some(hc - 1) });
        }
        (Some(h), Some(s)) if s.item == h.item => {
            let max = max_stack(h.item);
            let (sc, hc) = (s.count.unwrap_or(max), h.count.unwrap_or(max));
            if sc < max {
                *slot = Some(ItemStack { item: s.item, count: Some(sc + 1) });
                *held = (hc > 1).then_some(ItemStack { item: h.item, count: Some(hc - 1) });
            } else {
                *held = Some(h);
            }
        }
        (Some(h), Some(s)) => {
            // คลิกขวาบนของคนละชนิด = สลับ (เหมือนคลิกซ้าย)
            *slot = Some(h);
            *held = Some(s);
        }
        (None, None) => {}
    }
}

/// E เปิด/ปิดหน้าต่าง — ส่วน ESC อยู่ใน pause_menu_system ที่เดียว
pub fn inventory_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut open: ResMut<crate::voxel::InventoryOpen>,
    paused: Res<crate::Paused>,
    mut cursor_query: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
) {
    if paused.0 || !keyboard.just_pressed(KeyCode::KeyE) {
        return;
    }
    open.0 = !open.0;
    if open.0 {
        // ปล่อยเมาส์ให้คลิกช่องได้ — ล็อคกลับด้วยคลิกซ้ายตามเดิม (cursor_grab_system)
        if let Ok(mut cursor) = cursor_query.single_mut() {
            cursor.grab_mode = bevy::window::CursorGrabMode::None;
            cursor.visible = true;
        }
    }
}

/// จัดการตอนหน้าต่างปิด: ล็อคเมาส์กลับทันที + คืนของที่ค้างอยู่บนเมาส์
///
/// เฝ้าที่ตัว resource แทนที่จะผูกกับปุ่ม เพราะปิดได้จากหลายทาง (E, ESC ใน
/// pause_menu_system, ออกจากโลก) — ต้องรันหลัง cursor_grab_system ไม่งั้นการ
/// ปลดล็อคเมาส์ของ ESC ในระบบนั้นจะมาทับการล็อคกลับของเรา
pub fn inventory_close_system(
    open: Res<crate::voxel::InventoryOpen>,
    mut open_container: ResMut<crate::voxel::OpenContainer>,
    paused: Res<crate::Paused>,
    mut hotbar: ResMut<crate::voxel::Hotbar>,
    mut held: ResMut<HeldStack>,
    mut spawn_events: MessageWriter<crate::item::SpawnDroppedItemEvent>,
    camera_query: Query<&Transform, With<FreeCamera>>,
    mut cursor_query: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
) {
    if open.0 || !open.is_changed() {
        return;
    }
    open_container.0 = None;
    // pause menu เปิดอยู่ก็ต้องเห็นเมาส์ต่อ — ล็อคกลับเฉพาะตอนกลับไปคุมตัวละครจริงๆ
    if !paused.0 {
        if let Ok(mut cursor) = cursor_query.single_mut() {
            cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
            cursor.visible = false;
        }
    }
    let Some(stack) = held.0.take() else { return };
    let leftover = insert_into(&mut hotbar.slots, 0..crate::voxel::TOTAL_SLOTS, stack);
    // ช่องเต็มหมด — โยนลงโลกแทนที่จะกลืนทิ้ง (เส้นทางเดียวกับปุ่ม Q)
    if let (Some(left), Some(cam_tf)) = (leftover, camera_query.iter().next()) {
        let forward = cam_tf.forward().normalize();
        spawn_events.write(crate::item::SpawnDroppedItemEvent {
            item: left.item,
            pos: cam_tf.translation + forward * 0.5 - Vec3::Y * 0.2,
            velocity: forward * 5.0 + Vec3::Y * 3.0,
        });
    }
}

/// คลิกช่อง/palette เพื่อจัดของ
///
/// ไม่พึ่ง `Interaction::Pressed` เพราะ bevy_ui ตั้งค่านั้นจากปุ่มซ้ายอย่างเดียว
/// และลำดับ ui_focus_system เทียบกับระบบนี้ไม่การันตี — อ่านสถานะ hover คู่กับ
/// `ButtonInput<MouseButton>` เองแทน ได้ทั้งซ้ายและขวาด้วยตรรกะเดียว
pub fn inventory_click_system(
    open: Res<crate::voxel::InventoryOpen>,
    settings: Res<crate::GameSettings>,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut hotbar: ResMut<crate::voxel::Hotbar>,
    mut held: ResMut<HeldStack>,
    open_container: Res<crate::voxel::OpenContainer>,
    slot_query: Query<(&InvSlotUi, &Interaction)>,
    palette_query: Query<(&PaletteSlotUi, &Interaction)>,
) {
    if !open.0 {
        return;
    }
    let left = mouse.just_pressed(MouseButton::Left);
    let right = mouse.just_pressed(MouseButton::Right);
    if !left && !right {
        return;
    }
    let shift =
        keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    // palette: หยิบ stack เต็มขึ้นมือ (Creative เท่านั้น — Survival ซ่อนแถบนี้อยู่)
    if settings.game_mode == crate::GameMode::Creative && open_container.0.is_none() {
        for (pal, interaction) in &palette_query {
            if *interaction == Interaction::None {
                continue;
            }
            let Some(item) = crate::voxel::PLACEABLE_ITEMS.get(pal.0).copied() else { continue };
            held.0 = Some(crate::voxel::ItemStack {
                item,
                count: Some(crate::voxel::max_stack(item)),
            });
            return;
        }
    }

    for (slot, interaction) in &slot_query {
        if *interaction == Interaction::None {
            continue;
        }
        let idx = slot.0;
        if left && shift {
            quick_move(&mut hotbar, idx);
        } else if left {
            click_left(&mut hotbar.slots[idx], &mut held.0);
        } else {
            click_right(&mut hotbar.slots[idx], &mut held.0);
        }
        return;
    }
}

/// วาดช่องทั้งหมด + คุมความมองเห็นของหน้าต่าง (root ไม่ติด InGameUi จึงต้องคุมเอง)
pub fn update_inventory_ui(
    open: Res<crate::voxel::InventoryOpen>,
    open_container: Res<crate::voxel::OpenContainer>,
    hotbar: Res<crate::voxel::Hotbar>,
    settings: Res<crate::GameSettings>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    icons: Res<crate::voxel::ItemIconCache>,
    mut root_query: Query<
        &mut Visibility,
        (With<InventoryRoot>, Without<InventoryPalette>, Without<HudHotbarRoot>),
    >,
    mut palette_query: Query<(&mut Node, &mut Visibility), With<InventoryPalette>>,
    mut hud_hotbar_query: Query<
        &mut Visibility,
        (With<HudHotbarRoot>, Without<InventoryRoot>, Without<InventoryPalette>),
    >,
    mut slot_query: Query<(&InvSlotUi, &mut BorderColor)>,
    mut icon_query: Query<(Entity, &InvSlotIcon, &mut BackgroundColor), Without<HeldStackIcon>>,
    mut count_query: Query<(&InvSlotCount, &mut Text)>,
) {
    let show = open.0;
    for mut vis in &mut root_query {
        let want = if show { Visibility::Visible } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
    // hotbar ล่างจอซ้ำกับแถวในหน้าต่าง — ซ่อนไว้ตลอดที่หน้าต่างเปิด
    for mut vis in &mut hud_hotbar_query {
        let want = if show { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
    }
    if !show {
        return;
    }

    // palette ของ creative โชว์เฉพาะหน้าช่องเก็บของล้วนๆ (กด E) — ตอนเปิด
    // Chest/Furnace จอนั้นคือ "ย้ายของเข้าออกกล่อง" ไม่ใช่ที่หยิบของฟรี
    let show_palette = settings.game_mode == crate::GameMode::Creative
        && open_container.0.is_none();
    for (mut node, mut vis) in &mut palette_query {
        // Display::None ด้วย — ซ่อนแล้วต้องไม่เหลือช่องว่างค้าง (ดู set_shown)
        set_shown(&mut node, &mut vis, show_palette);
    }

    for (count, mut text) in &mut count_query {
        let s = match hotbar.slots[count.0].and_then(|st| st.count) {
            Some(n) => n.to_string(),
            None => String::new(),
        };
        if text.0 != s {
            text.0 = s;
        }
    }

    for (slot, mut border) in &mut slot_query {
        // ไฮไลต์ช่องที่ถืออยู่ในมือ (แถวล่างของหน้าต่าง = แถบเดียวกับล่างจอ)
        *border = if slot.0 == hotbar.selected {
            BorderColor::all(Color::srgb(1.0, 0.85, 0.2))
        } else {
            BorderColor::all(Color::srgba(0.2, 0.2, 0.2, 0.6))
        };
    }

    for (entity, icon, mut bg) in &mut icon_query {
        match hotbar.slots[icon.0] {
            Some(stack) => match stack.item.icon_image(&icons, &asset_server) {
                Some(image) => {
                    commands.entity(entity).insert(ImageNode::new(image));
                    bg.0 = Color::NONE;
                }
                None => {
                    commands.entity(entity).remove::<ImageNode>();
                    let c = stack.item.color();
                    bg.0 = Color::srgba(c[0], c[1], c[2], c[3]);
                }
            },
            None => {
                commands.entity(entity).remove::<ImageNode>();
                bg.0 = Color::NONE;
            }
        }
    }
}

/// วาดกริด Chest/Furnace ที่เปิดอยู่ + คุมความมองเห็น (โผล่/หายพร้อมหน้าต่างช่องเก็บของหลัก
/// เพราะ open_container เซ็ตคู่กับ InventoryOpen เสมอ — ดู block_interaction_system)
pub fn update_container_ui(
    open_container: Res<crate::voxel::OpenContainer>,
    world: Res<crate::voxel::VoxelWorld>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    icons: Res<crate::voxel::ItemIconCache>,
    mut chest_panel: Query<
        (&mut Node, &mut Visibility),
        (With<ContainerPanel>, Without<FurnacePanel>, Without<ContainerSlotUi>),
    >,
    mut furnace_panel: Query<
        (&mut Node, &mut Visibility),
        (With<FurnacePanel>, Without<ContainerPanel>, Without<ContainerSlotUi>),
    >,
    mut slot_vis_query: Query<
        (&ContainerSlotUi, &mut Visibility),
        (Without<ContainerPanel>, Without<FurnacePanel>),
    >,
    mut icon_query: Query<(Entity, &ContainerSlotIcon, &mut BackgroundColor), Without<HeldStackIcon>>,
    mut count_query: Query<(&ContainerSlotCount, &mut Text)>,
) {
    // แต่ละชนิดกล่องมี panel ของตัวเอง — โชว์อันเดียว อีกอันหลุดจาก layout ไปเลย
    // (set_shown ใช้ Display::None — Visibility เฉยๆ ยังกินความสูงหน้าต่างอยู่)
    let (show_chest, show_furnace) = match open_container.0 {
        None => (false, false),
        Some(oc) => {
            let is_chest = oc.kind == crate::voxel::BlockType::Chest;
            (is_chest, !is_chest)
        }
    };
    for (mut node, mut vis) in chest_panel.iter_mut() {
        set_shown(&mut node, &mut vis, show_chest);
    }
    for (mut node, mut vis) in furnace_panel.iter_mut() {
        set_shown(&mut node, &mut vis, show_furnace);
    }
    let Some(oc) = open_container.0 else { return };
    let is_chest = oc.kind == crate::voxel::BlockType::Chest;

    let capacity: usize = if is_chest { 27 } else { 3 };
    // เอามาเป็น array คงที่ก่อน — ช่อง 0..27 ใช้ร่วมกันทั้ง Chest(27)/Furnace(3, ที่เหลือ None)
    let mut slots: [Option<crate::voxel::ItemStack>; 27] = [None; 27];
    match oc.kind {
        crate::voxel::BlockType::Chest => {
            if let Some(s) = world.get_chest_slots(oc.pos.x, oc.pos.y, oc.pos.z) {
                slots = *s;
            }
        }
        crate::voxel::BlockType::Furnace => {
            if let Some(f) = world.get_furnace_slots(oc.pos.x, oc.pos.y, oc.pos.z) {
                slots[..3].copy_from_slice(f);
            }
        }
        _ => {}
    }

    for (slot, mut vis) in &mut slot_vis_query {
        let want = if slot.0 < capacity { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want { *vis = want; }
    }

    for (count, mut text) in &mut count_query {
        let s = match slots.get(count.0).copied().flatten().and_then(|s| s.count) {
            Some(n) => n.to_string(),
            None => String::new(),
        };
        if text.0 != s { text.0 = s; }
    }

    for (entity, icon, mut bg) in &mut icon_query {
        match slots.get(icon.0).copied().flatten() {
            Some(stack) => match stack.item.icon_image(&icons, &asset_server) {
                Some(image) => {
                    commands.entity(entity).insert(ImageNode::new(image));
                    bg.0 = Color::NONE;
                }
                None => {
                    commands.entity(entity).remove::<ImageNode>();
                    let c = stack.item.color();
                    bg.0 = Color::srgba(c[0], c[1], c[2], c[3]);
                }
            },
            None => {
                commands.entity(entity).remove::<ImageNode>();
                bg.0 = Color::NONE;
            }
        }
    }
}

/// คลิกช่องในกริด Chest/Furnace ที่เปิดอยู่ — เหมือน inventory_click_system แต่เขียนผ่าน
/// BlockEdit::SetContainerSlot (คงจุด apply เดียว + sync ข้าม network) แทนแก้ Hotbar ตรงๆ
/// ไม่ remesh/ปลุกน้ำ — ของใน container ไม่กระทบ geometry (ดู apply_incoming_net_edits ฝั่งรับ)
pub fn container_click_system(
    open_container: Res<crate::voxel::OpenContainer>,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut world: ResMut<crate::voxel::VoxelWorld>,
    mut held: ResMut<HeldStack>,
    mut hotbar: ResMut<crate::voxel::Hotbar>,
    slot_query: Query<(&ContainerSlotUi, &Interaction)>,
    (net_server, net_client, mut net_out): (
        Option<Res<bevy_renet::RenetServer>>,
        Option<Res<bevy_renet::RenetClient>>,
        ResMut<crate::network::PendingNetEdits>,
    ),
) {
    let Some(oc) = open_container.0 else { return };
    let left = mouse.just_pressed(MouseButton::Left);
    let right = mouse.just_pressed(MouseButton::Right);
    if !left && !right {
        return;
    }
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    let capacity: usize = if oc.kind == crate::voxel::BlockType::Chest { 27 } else { 3 };

    for (slot, interaction) in &slot_query {
        if *interaction == Interaction::None {
            continue;
        }
        let idx = slot.0;
        if idx >= capacity {
            continue;
        }

        let mut current = match oc.kind {
            crate::voxel::BlockType::Chest => {
                world.get_chest_slots(oc.pos.x, oc.pos.y, oc.pos.z).and_then(|s| s[idx])
            }
            crate::voxel::BlockType::Furnace => {
                world.get_furnace_slots(oc.pos.x, oc.pos.y, oc.pos.z).and_then(|s| s[idx])
            }
            _ => None,
        };

        if left && shift {
            // shift+คลิกซ้าย: เด้งทั้งกองไปช่องเก็บของผู้เล่น (ทางเดียว — ย้อนกลับใช้คลิกลากปกติ)
            if let Some(stack) = current {
                current = insert_into(&mut hotbar.slots, 0..crate::voxel::TOTAL_SLOTS, stack);
            }
        } else if left {
            click_left(&mut current, &mut held.0);
        } else {
            click_right(&mut current, &mut held.0);
        }

        let edit = crate::network::BlockEdit::SetContainerSlot {
            pos: [oc.pos.x, oc.pos.y, oc.pos.z],
            slot: idx as u8,
            item: current.map(crate::item::WireItemStack::from_stack),
        };
        crate::voxel::apply_block_edit(&mut world, &edit);
        if net_server.is_some() || net_client.is_some() {
            net_out.0.push_back((None, edit));
        }
        // เซฟทันที (host/single เท่านั้น — client ให้ host เป็นเจ้าของโลก)
        if net_client.is_none() {
            let cp = IVec2::new(
                oc.pos.x.div_euclid(crate::voxel::CHUNK_WIDTH as i32),
                oc.pos.z.div_euclid(crate::voxel::CHUNK_WIDTH as i32),
            );
            if let Some(chunk) = world.chunks.get(&cp) {
                crate::voxel::save_chunk_full(cp, chunk);
            }
        }
        return;
    }
}

/// icon ของกองที่ถืออยู่ ลอยตามเมาส์
pub fn update_held_icon(
    held: Res<HeldStack>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    icons: Res<crate::voxel::ItemIconCache>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut query: Query<
        (Entity, &mut Node, &mut Visibility, &mut BackgroundColor),
        With<HeldStackIcon>,
    >,
) {
    for (entity, mut node, mut vis, mut bg) in &mut query {
        let Some(stack) = held.0 else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
                commands.entity(entity).remove::<ImageNode>();
                bg.0 = Color::NONE;
            }
            continue;
        };
        *vis = Visibility::Visible;
        match stack.item.icon_image(&icons, &asset_server) {
            Some(image) => {
                commands.entity(entity).insert(ImageNode::new(image));
                bg.0 = Color::NONE;
            }
            None => {
                commands.entity(entity).remove::<ImageNode>();
                let c = stack.item.color();
                bg.0 = Color::srgba(c[0], c[1], c[2], c[3]);
            }
        }
        if let Some(pos) = windows.iter().next().and_then(|w| w.cursor_position()) {
            node.left = Val::Px(pos.x - 20.0);
            node.top = Val::Px(pos.y - 20.0);
        }
    }
}

/// decay ความจ้า + อัปเดต alpha ของ overlay (จ้าเกิน 1.0 = ขาวสนิทค้างไว้ก่อน)
/// ป้ายชื่อ item เด้งเหนือ hotbar ตอนสลับช่อง/ของในช่องเปลี่ยน แล้วจางหายใน 1.6 วิ
/// รันตลอด (ไม่ gate InGame) — timer จะพาป้ายซ่อนเองแม้ออกไปเมนูกลางคัน
pub fn hotbar_item_name_system(
    time: Res<Time>,
    hotbar: Res<crate::voxel::Hotbar>,
    inventory_open: Res<crate::voxel::InventoryOpen>,
    mut timer: Local<f32>,
    mut last: Local<Option<(usize, Option<crate::item::Item>)>>,
    mut root_query: Query<&mut Visibility, With<HotbarItemNameRoot>>,
    mut text_query: Query<&mut Text, With<HotbarItemNameText>>,
) {
    // หน้าต่างช่องเก็บของเปิดอยู่ — hotbar ล่างจอถูกซ่อน ป้ายก็ไม่ควรลอยเดี่ยวๆ
    if inventory_open.0 {
        *timer = 0.0;
    }
    let current = (hotbar.selected, hotbar.slots[hotbar.selected].map(|s| s.item));
    if *last != Some(current) {
        // เฟรมแรกของแอป (last = None) ไม่ต้องเด้ง — ตั้ง baseline เฉยๆ
        let first_run = last.is_none();
        *last = Some(current);
        if !first_run {
            if let Some(item) = current.1 {
                if let Ok(mut text) = text_query.single_mut() {
                    text.0 = item.name().to_string();
                }
                *timer = 1.6;
            } else {
                *timer = 0.0; // ช่องว่าง — ซ่อนเลย
            }
        }
    }
    if *timer > 0.0 {
        *timer -= time.delta_secs();
    }
    let want = if *timer > 0.0 { Visibility::Visible } else { Visibility::Hidden };
    if let Ok(mut vis) = root_query.single_mut() {
        if *vis != want { *vis = want; }
    }
}

/// โชว์ชื่อ item ใต้กริดตอนเมาส์ hover ช่องในหน้าต่าง E (ช่องเรา/palette/กล่อง)
pub fn inventory_hover_name_system(
    open: Res<crate::voxel::InventoryOpen>,
    hotbar: Res<crate::voxel::Hotbar>,
    open_container: Res<crate::voxel::OpenContainer>,
    world: Res<crate::voxel::VoxelWorld>,
    slot_query: Query<(&InvSlotUi, &Interaction)>,
    palette_query: Query<(&PaletteSlotUi, &Interaction)>,
    container_query: Query<(&ContainerSlotUi, &Interaction)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut hover_ui_query: Query<(&mut Text, &mut Node, &mut Visibility), With<InvHoverNameText>>,
) {
    if !open.0 {
        if let Ok((_, _, mut vis)) = hover_ui_query.single_mut() {
            if *vis != Visibility::Hidden { *vis = Visibility::Hidden; }
        }
        return;
    }
    let mut name: Option<&'static str> = None;
    for (slot, interaction) in &slot_query {
        if *interaction != Interaction::None {
            name = hotbar.slots[slot.0].map(|s| s.item.name());
            break;
        }
    }
    if name.is_none() {
        for (pal, interaction) in &palette_query {
            if *interaction != Interaction::None {
                name = crate::voxel::PLACEABLE_ITEMS.get(pal.0).map(|i| i.name());
                break;
            }
        }
    }
    if name.is_none() {
        if let Some(oc) = open_container.0 {
            for (slot, interaction) in &container_query {
                if *interaction == Interaction::None {
                    continue;
                }
                name = match oc.kind {
                    crate::voxel::BlockType::Chest => world
                        .get_chest_slots(oc.pos.x, oc.pos.y, oc.pos.z)
                        .and_then(|s| s.get(slot.0).copied().flatten())
                        .map(|s| s.item.name()),
                    crate::voxel::BlockType::Furnace => world
                        .get_furnace_slots(oc.pos.x, oc.pos.y, oc.pos.z)
                        .and_then(|s| s.get(slot.0).copied().flatten())
                        .map(|s| s.item.name()),
                    _ => None,
                };
                break;
            }
        }
    }
    if let Ok((mut text, mut node, mut vis)) = hover_ui_query.single_mut() {
        if let Some(want) = name {
            if text.0 != want {
                text.0 = want.to_string();
            }
            if *vis != Visibility::Visible {
                *vis = Visibility::Visible;
            }
            if let Some(pos) = windows.iter().next().and_then(|w| w.cursor_position()) {
                node.left = Val::Px(pos.x + 15.0);
                node.top = Val::Px(pos.y + 15.0);
            }
        } else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        }
    }
}

/// สอนปุ่มครั้งแรกที่เข้าโลก (ต่อการเปิดเกมหนึ่งครั้ง) — ผ่านแชทที่จางหายเอง
/// ไม่มีที่อื่นให้ผู้เล่นใหม่รู้ปุ่มเลยนอกจาก ESC → Options ที่ซ่อนอยู่
pub fn show_controls_hint(mut chat: ResMut<ChatState>, mut shown: Local<bool>) {
    if *shown {
        return;
    }
    *shown = true;
    chat.push_system("Controls: WASD move | Space jump | F fly/walk | E inventory | Q drop item");
    chat.push_system("T chat | F5 camera view | F3 debug info | ESC pause menu");
}

pub fn update_screen_flash(
    time: Res<Time>,
    mut flash: ResMut<ScreenFlash>,
    mut query: Query<&mut BackgroundColor, With<ScreenFlashOverlay>>,
) {
    if flash.intensity <= 0.0 {
        return;
    }
    flash.intensity *= (-flash.decay * time.delta_secs()).exp();
    if flash.intensity < 0.01 {
        flash.intensity = 0.0;
    }
    for mut bg in &mut query {
        bg.0 = Color::srgba(1.0, 1.0, 1.0, flash.intensity.clamp(0.0, 1.0));
    }
}

pub fn toggle_ingame_ui(
    state: Res<State<crate::GameState>>,
    show_debug: Res<ShowDebugMenu>,
    mut query: Query<(Entity, &mut Visibility, Option<&DebugMenuUi>), With<InGameUi>>,
) {
    if state.is_changed() || state.is_added() || show_debug.is_changed() {
        let is_ingame = *state.get() == crate::GameState::InGame;
        for (_, mut vis, is_debug) in &mut query {
            if is_debug.is_some() {
                *vis = if is_ingame && show_debug.0 { Visibility::Inherited } else { Visibility::Hidden };
            } else {
                *vis = if is_ingame { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
}

pub fn handle_f3_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut show_debug: ResMut<ShowDebugMenu>,
    chat: Res<ChatState>,
) {
    if chat.open {
        return;
    }
    if keyboard.just_pressed(KeyCode::F3) {
        show_debug.0 = !show_debug.0;
    }
}

/// บอกว่าเฟรมนี้ egui กำลังรับ text input อยู่ไหม — ใช้กันคีย์ทะลุไป gameplay
/// (แก้บั๊กเดิมด้วย: พิมพ์ในช่อง GPS lat/lon แล้วเลข 1-9 ไปเปลี่ยนช่อง hotbar)
pub fn track_egui_typing(
    mut contexts: bevy_egui::EguiContexts,
    mut typing: ResMut<crate::EguiTyping>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let wants = ctx.egui_wants_keyboard_input();
    if typing.0 != wants {
        typing.0 = wants;
    }
}

/// T = เปิดแชทเปล่า, / = เปิดพร้อมเติม "/" ให้เลย (เหมือน Minecraft)
/// ปล่อยเมาส์ตอนเปิดตาม pattern ของ block picker
pub fn chat_open_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut chat: ResMut<ChatState>,
    mut cursor_query: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
    inventory: Res<crate::voxel::InventoryOpen>,
    paused: Res<crate::Paused>,
) {
    if chat.open || inventory.0 || paused.0 {
        return;
    }
    let slash = keyboard.just_pressed(KeyCode::Slash);
    if !keyboard.just_pressed(KeyCode::KeyT) && !slash {
        return;
    }
    chat.open = true;
    chat.focus_requested = true;
    chat.history_pos = None;
    chat.input = if slash { "/".to_string() } else { String::new() };
    if let Ok(mut cursor) = cursor_query.single_mut() {
        cursor.grab_mode = bevy::window::CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// เดินอายุบรรทัดแชทให้จางหายตอนปิดแชท
pub fn age_chat_lines(time: Res<Time>, mut chat: ResMut<ChatState>) {
    let dt = time.delta_secs();
    // bypass change detection: แตะทุกเฟรมอยู่แล้ว ไม่งั้น update_chat_ui รันรัวเปล่าๆ
    for line in chat.bypass_change_detection().log.iter_mut() {
        line.age += dt;
    }
}

/// เขียนบรรทัดล่าสุดลง Text node ที่ pre-spawn ไว้ (บนสุด = เก่าสุด)
pub fn update_chat_ui(
    chat: Res<ChatState>,
    mut query: Query<(&ChatLineUi, &mut Text, &mut TextColor, &mut Node)>,
) {
    // เลือกบรรทัดที่จะโชว์: เปิดแชท = ล่าสุดเต็มจอ, ปิด = เฉพาะที่ยังไม่จาง
    let visible: Vec<&ChatLine> = chat
        .log
        .iter()
        .rev()
        .filter(|l| chat.open || l.age < CHAT_FADE_SECONDS)
        .take(CHAT_VISIBLE_LINES)
        .collect();

    for (slot, mut text, mut color, mut node) in &mut query {
        // slot 0 อยู่บนสุด = บรรทัดเก่าสุดในชุดที่โชว์
        let idx = visible.len().checked_sub(slot.0 + 1);
        match idx.and_then(|i| visible.get(i)) {
            Some(line) => {
                if text.0 != line.text {
                    text.0 = line.text.clone();
                }
                color.0 = line.kind.color();
                node.display = Display::Flex;
            }
            None => {
                if !text.0.is_empty() {
                    text.0.clear();
                }
                // ซ่อนทั้ง node ไม่งั้นพื้นหลังดำของบรรทัดว่างยังค้างอยู่
                node.display = Display::None;
            }
        }
    }
}

/// ช่องพิมพ์แชท (egui) — Bevy UI ยังไม่มี text input ให้ใช้
pub fn chat_input_system(
    mut contexts: bevy_egui::EguiContexts,
    mut chat: ResMut<ChatState>,
    mut queue: ResMut<crate::command::CommandQueue>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if !chat.open {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone();

    // ESC ปิดแชททิ้งข้อความ — เช็คก่อน pause_menu_system จะได้ไม่เด้ง pause
    if keyboard.just_pressed(KeyCode::Escape) {
        chat.open = false;
        chat.input.clear();
        return;
    }

    // ลูกศรขึ้น/ลง = ย้อนประวัติที่เคยส่ง
    if keyboard.just_pressed(KeyCode::ArrowUp) && !chat.history.is_empty() {
        let pos = match chat.history_pos {
            Some(p) if p > 0 => p - 1,
            Some(p) => p,
            None => chat.history.len() - 1,
        };
        chat.history_pos = Some(pos);
        chat.input = chat.history[pos].clone();
    } else if keyboard.just_pressed(KeyCode::ArrowDown) {
        match chat.history_pos {
            Some(p) if p + 1 < chat.history.len() => {
                chat.history_pos = Some(p + 1);
                chat.input = chat.history[p + 1].clone();
            }
            Some(_) => {
                chat.history_pos = None;
                chat.input.clear();
            }
            None => {}
        }
    }

    let mut submitted = false;
    bevy_egui::egui::Area::new(bevy_egui::egui::Id::new("chat_input"))
        .anchor(bevy_egui::egui::Align2::LEFT_BOTTOM, bevy_egui::egui::vec2(10.0, -10.0))
        .order(bevy_egui::egui::Order::Foreground)
        .show(&ctx, |ui| {
            bevy_egui::egui::Frame::popup(ui.style()).show(ui, |ui| {
                let response = ui.add(
                    bevy_egui::egui::TextEdit::singleline(&mut chat.input)
                        .desired_width(520.0)
                        .hint_text("message or /command"),
                );
                if chat.focus_requested {
                    response.request_focus();
                    chat.focus_requested = false;
                }
                // idiom มาตรฐานของ egui: singleline เสีย focus ตอนกด Enter
                if response.lost_focus()
                    && ui.input(|i| i.key_pressed(bevy_egui::egui::Key::Enter))
                {
                    submitted = true;
                }
            });
        });

    if !submitted {
        return;
    }
    let text = crate::network::sanitize_chat(&chat.input);
    chat.input.clear();
    chat.open = false;
    chat.history_pos = None;
    if text.is_empty() {
        return;
    }
    if chat.history.last().map(|s| s.as_str()) != Some(text.as_str()) {
        chat.history.push(text.clone());
    }
    queue.0.push_back(text);
}

/// สลับชนิดโลก: ต่างจากเดิม = ล้างโลก generate ใหม่ + ชี้โฟลเดอร์เซฟให้ถูกโลก
pub fn select_terrain(
    settings: &mut crate::GameSettings,
    regenerate: &mut crate::RegenerateWorld,
    source: crate::TerrainSource,
) {
    if settings.terrain_source != source {
        settings.terrain_source = source;
        regenerate.0 = true;
        // ภูเขาจริง mesh หนักกว่าโลก noise หลายเท่า — เริ่มที่ระยะปลอดภัยก่อน
        // (ปรับเพิ่มเองได้ใน settings ตามไหวของการ์ด)
        if source == crate::TerrainSource::RealWorld && settings.render_distance > 6 {
            settings.render_distance = 6;
        }
    }
    crate::voxel::set_legacy_save_dir(source == crate::TerrainSource::RealWorld);
}

/// ทางเข้าเกมทางเดียวของทุกเมนู — รวมไว้ที่เดียวเพราะ terrain_source กับโฟลเดอร์เซฟ
/// ต้องตั้งคู่กันเสมอ (หลุด sync = เซฟข้ามโลกปนกัน)
#[allow(clippy::too_many_arguments)]
fn enter_world(
    settings: &mut crate::GameSettings,
    regenerate: &mut crate::RegenerateWorld,
    hotbar: &mut crate::voxel::Hotbar,
    next_state: &mut NextState<crate::GameState>,
    save_dir: Option<std::path::PathBuf>,
    seed: u32,
    mode: crate::GameMode,
    source: crate::TerrainSource,
    dev: bool,
) {
    settings.dev_mode = dev;
    settings.game_mode = mode;
    settings.noise.seed = seed;
    settings.terrain_source = source;

    match save_dir {
        Some(dir) => crate::voxel::set_active_save_dir(Some(dir)),
        None => crate::voxel::set_legacy_save_dir(source == crate::TerrainSource::RealWorld),
    }
    if source == crate::TerrainSource::RealWorld && settings.render_distance > 6 {
        // ภูเขาจริง mesh หนักกว่าโลก noise หลายเท่า — เริ่มที่ระยะปลอดภัยก่อน
        settings.render_distance = 6;
    }

    // สำคัญ: ล้างโลกใน memory จาก world ก่อนหน้า ไม่งั้น chunk เก่าค้างข้ามโลก
    regenerate.0 = true;
    *hotbar = crate::voxel::Hotbar::for_mode(mode);
    next_state.set(crate::GameState::InGame);
}

/// ธีม egui กลาง ตั้งครั้งเดียว: ฟอนต์ใหญ่ขึ้น + โทนสีเดียวกับ HUD (น้ำเงินเข้ม/ทอง)
/// — ค่า default ของ egui เป็นเทา debug-tool ไม่เข้ากับเกม
pub fn setup_egui_theme(mut contexts: bevy_egui::EguiContexts, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    *done = true;
    use bevy_egui::egui::{self, Color32, FontFamily, FontId, TextStyle};

    // egui 0.35: style แยกตาม light/dark theme — all_styles_mut ทาให้ทั้งคู่
    ctx.all_styles_mut(|style| {
        style.text_styles.insert(TextStyle::Heading, FontId::new(30.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Body, FontId::new(17.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Button, FontId::new(18.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Small, FontId::new(13.0, FontFamily::Proportional));
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);

        let panel = Color32::from_rgba_unmultiplied(18, 18, 28, 245);
        let widget = Color32::from_rgb(45, 45, 62);
        let widget_hover = Color32::from_rgb(64, 64, 88);
        let accent = Color32::from_rgb(255, 217, 51); // ทองเดียวกับกรอบ hotbar ที่เลือก

        let v = &mut style.visuals;
        v.window_fill = panel;
        v.panel_fill = panel;
        v.window_stroke = egui::Stroke::new(1.0, Color32::from_rgb(85, 85, 110));
        v.widgets.inactive.bg_fill = widget;
        v.widgets.inactive.weak_bg_fill = widget; // ปุ่มใช้ weak_bg_fill ใน egui รุ่นใหม่
        v.widgets.hovered.bg_fill = widget_hover;
        v.widgets.hovered.weak_bg_fill = widget_hover;
        v.widgets.active.bg_fill = widget_hover;
        v.widgets.active.weak_bg_fill = widget_hover;
        v.selection.bg_fill = Color32::from_rgba_unmultiplied(255, 217, 51, 70);
        v.selection.stroke = egui::Stroke::new(1.0, accent);
        v.hyperlink_color = accent;
    });
}

/// ฉากหลังทึบของหน้าเมนูนอกเกม — ไม่งั้นหน้าต่างเมนูลอยบนจอว่างเปล่า (โลกถูก unload แล้ว)
fn menu_backdrop(ctx: &bevy_egui::egui::Context) {
    use bevy_egui::egui;
    let screen = ctx.content_rect(); // egui 0.35: screen_rect เปลี่ยนชื่อเป็น content_rect
    egui::Area::new(egui::Id::new("menu_backdrop"))
        .order(egui::Order::Background)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.painter().rect_filled(
                screen,
                egui::CornerRadius::ZERO,
                egui::Color32::from_rgb(13, 15, 24),
            );
        });
}

pub fn main_menu_system(
    mut contexts: bevy_egui::EguiContexts,
    mut next_state: ResMut<NextState<crate::GameState>>,
    mut app_exit: MessageWriter<bevy::app::AppExit>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone(); // In egui 0.35 Context is easily cloned to avoid mutability issues
    menu_backdrop(&ctx);

    bevy_egui::egui::Window::new("Main Menu")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(bevy_egui::egui::Align2::CENTER_CENTER, bevy_egui::egui::vec2(0.0, 0.0))
        .show(&ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);

            ui.heading(
                bevy_egui::egui::RichText::new("VOXEL GAME")
                    .size(56.0)
                    .color(bevy_egui::egui::Color32::from_rgb(255, 217, 51))
                    .strong()
            );

            ui.add_space(50.0);

            let btn_size = bevy_egui::egui::vec2(220.0, 44.0);

            if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Singleplayer")).clicked() {
                next_state.set(crate::GameState::SinglePlayerMenu);
            }
            ui.add_space(10.0);

            if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Multiplayer")).clicked() {
                next_state.set(crate::GameState::MultiplayerMenu);
            }
            ui.add_space(10.0);

            // ออกทาง AppExit (ไม่ใช่ process::exit) — ให้ bevy ปิดตัวตามปกติ
            if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Quit")).clicked() {
                app_exit.write(bevy::app::AppExit::Success);
            }

            ui.add_space(24.0);

            // ทางเข้า dev (noise ด่วน / โลกจริง / สไลเดอร์ world gen) — ลดรูปเป็น
            // ลิงก์เล็กท้ายเมนู ไม่ปนกับปุ่มหลักให้ผู้เล่นทั่วไปงง
            if ui
                .add(bevy_egui::egui::Button::new(
                    bevy_egui::egui::RichText::new("dev mode").small().weak(),
                ).frame(false))
                .clicked()
            {
                next_state.set(crate::GameState::DevMenu);
            }

            ui.add_space(10.0);
        });
    });
}

/// รายการ world ที่โหลดจาก disk — cache ไว้ ไม่งั้นต้องอ่าน disk ทุกเฟรมที่วาดเมนู
#[derive(Resource, Default)]
pub struct WorldList(pub Vec<(std::path::PathBuf, crate::world_save::WorldMeta)>);

/// refresh ตอนเข้าหน้า SELECT WORLD (และหลังสร้าง/ลบ)
pub fn refresh_world_list(mut list: ResMut<WorldList>) {
    list.0 = crate::world_save::list_worlds();
}

/// ค่าที่ค้างอยู่ในฟอร์ม CREATE WORLD
#[derive(Resource)]
pub struct CreateWorldUi {
    pub name: String,
    pub seed: String,
    pub survival: bool,
    pub status: String,
}

impl Default for CreateWorldUi {
    fn default() -> Self {
        Self {
            name: "New World".into(),
            seed: String::new(),
            survival: false,
            status: String::new(),
        }
    }
}

/// เลือกโลกจากหน้า Multiplayer เพื่อเปิด host — เข้าเกมแล้วเปิด LAN อัตโนมัติ
/// (แก้ปัญหา "Open to LAN" ซ่อนลึกใน ESC → Options จนผู้เล่นใหม่หาไม่เจอ)
#[derive(Resource)]
pub struct HostIntent;

/// หน้าเลือก world ที่เคยสร้าง (เข้าจากปุ่ม Singleplayer หรือ Host a World)
pub fn singleplayer_menu_system(
    mut contexts: bevy_egui::EguiContexts,
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<crate::GameState>>,
    mut settings: ResMut<crate::GameSettings>,
    mut regenerate: ResMut<crate::RegenerateWorld>,
    mut hotbar: ResMut<crate::voxel::Hotbar>,
    mut list: ResMut<WorldList>,
    mut create_ui: ResMut<CreateWorldUi>,
    host_intent: Option<Res<HostIntent>>,
    // โลกที่รอยืนยันลบ (คลิก Delete ครั้งแรก) — กันลบถาวรด้วยคลิกเดียว/คลิกพลาด
    mut confirm_delete: Local<Option<usize>>,
) {
    let hosting = host_intent.is_some();
    // ESC = ถอยกลับ (เหมือนกดปุ่ม Back)
    if keyboard.just_pressed(KeyCode::Escape) {
        if confirm_delete.is_some() {
            *confirm_delete = None;
        } else {
            if hosting {
                commands.remove_resource::<HostIntent>();
                next_state.set(crate::GameState::MultiplayerMenu);
            } else {
                next_state.set(crate::GameState::MainMenu);
            }
            return;
        }
    }

    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone();
    menu_backdrop(&ctx);

    // เก็บ action ไว้ทำหลังปิด closure — ยืม list อยู่ระหว่างวน loop
    let mut play: Option<usize> = None;
    let mut delete: Option<usize> = None;

    bevy_egui::egui::Window::new("Select World")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(bevy_egui::egui::Align2::CENTER_CENTER, bevy_egui::egui::vec2(0.0, 0.0))
        .show(&ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                let title = if hosting { "SELECT WORLD TO HOST" } else { "SELECT WORLD" };
                ui.heading(bevy_egui::egui::RichText::new(title).size(32.0).strong());
                if hosting {
                    ui.label(
                        bevy_egui::egui::RichText::new("the world opens to LAN right after loading")
                            .small()
                            .weak(),
                    );
                }
                ui.add_space(20.0);
            });

            let btn_size = bevy_egui::egui::vec2(220.0, 44.0);

            if list.0.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.label(
                        bevy_egui::egui::RichText::new("No worlds yet - press Create New World to start")
                            .weak(),
                    );
                });
            } else {
                bevy_egui::egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for (i, (_, meta)) in list.0.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(bevy_egui::egui::RichText::new(&meta.name).strong());
                                    let mode = if meta.survival { "Survival" } else { "Creative" };
                                    ui.label(
                                        bevy_egui::egui::RichText::new(format!(
                                            "Seed: {} | {}",
                                            meta.seed, mode
                                        ))
                                        .small()
                                        .weak(),
                                    );
                                });
                                ui.with_layout(
                                    bevy_egui::egui::Layout::right_to_left(
                                        bevy_egui::egui::Align::Center,
                                    ),
                                    |ui| {
                                        // ปุ่มลบสีแดง แยกจาก Play ชัดๆ — และแค่ "ขอยืนยัน"
                                        // การลบจริงอยู่ในหน้าต่าง confirm ข้างล่าง
                                        if ui.button(
                                            bevy_egui::egui::RichText::new("Delete")
                                                .color(bevy_egui::egui::Color32::from_rgb(240, 90, 90)),
                                        ).clicked() {
                                            *confirm_delete = Some(i);
                                        }
                                        let play_label = if hosting { "Host" } else { "Play" };
                                        if ui.button(play_label).clicked() {
                                            play = Some(i);
                                        }
                                    },
                                );
                            });
                            ui.separator();
                        }
                    });
            }

            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Create New World")).clicked() {
                    *create_ui = CreateWorldUi::default();
                    next_state.set(crate::GameState::CreateWorldMenu);
                }
                ui.add_space(10.0);
                if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Back")).clicked() {
                    if hosting {
                        commands.remove_resource::<HostIntent>();
                        next_state.set(crate::GameState::MultiplayerMenu);
                    } else {
                        next_state.set(crate::GameState::MainMenu);
                    }
                }
                ui.add_space(20.0);
            });
        });

    // หน้าต่างยืนยันลบ — ทับกลางจอ ลบจริงเฉพาะกดปุ่มแดงในนี้เท่านั้น
    if let Some(i) = *confirm_delete {
        match list.0.get(i) {
            Some((_, meta)) => {
                let name = meta.name.clone();
                bevy_egui::egui::Window::new("Confirm Delete")
                    .title_bar(false)
                    .resizable(false)
                    .collapsible(false)
                    .anchor(bevy_egui::egui::Align2::CENTER_CENTER, bevy_egui::egui::vec2(0.0, 0.0))
                    .show(&ctx, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(12.0);
                            ui.label(
                                bevy_egui::egui::RichText::new(format!("Delete world '{name}'?"))
                                    .size(20.0)
                                    .strong(),
                            );
                            ui.label(
                                bevy_egui::egui::RichText::new("This cannot be undone")
                                    .small()
                                    .weak(),
                            );
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                if ui.add_sized(
                                    bevy_egui::egui::vec2(110.0, 36.0),
                                    bevy_egui::egui::Button::new(
                                        bevy_egui::egui::RichText::new("Delete")
                                            .color(bevy_egui::egui::Color32::from_rgb(255, 120, 120)),
                                    ),
                                ).clicked() {
                                    delete = Some(i);
                                    *confirm_delete = None;
                                }
                                if ui.add_sized(
                                    bevy_egui::egui::vec2(110.0, 36.0),
                                    bevy_egui::egui::Button::new("Cancel"),
                                ).clicked() {
                                    *confirm_delete = None;
                                }
                            });
                            ui.add_space(8.0);
                        });
                    });
            }
            None => *confirm_delete = None, // index เก่าหลัง list เปลี่ยน
        }
    }

    if let Some(i) = play {
        let (dir, meta) = list.0[i].clone();
        if hosting {
            // เข้าเกมแล้ว auto_host_system จะรอ VoxelWorld พร้อมก่อนค่อยเปิด LAN
            commands.remove_resource::<HostIntent>();
            commands.insert_resource(crate::network::AutoHostPending);
        }
        enter_world(
            &mut settings,
            &mut regenerate,
            &mut hotbar,
            &mut next_state,
            Some(dir),
            meta.seed,
            meta.mode(),
            crate::TerrainSource::Noise,
            false,
        );
    } else if let Some(i) = delete {
        if let Err(e) = crate::world_save::delete_world(&list.0[i].0) {
            warn!("delete world failed: {e}");
        } else {
            list.0.remove(i);
        }
    }
}

/// ฟอร์มสร้าง world ใหม่ — ชื่อ / seed / โหมด (โลกแบบ generate เท่านั้น)
pub fn create_world_menu_system(
    mut contexts: bevy_egui::EguiContexts,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<crate::GameState>>,
    mut settings: ResMut<crate::GameSettings>,
    mut regenerate: ResMut<crate::RegenerateWorld>,
    mut hotbar: ResMut<crate::voxel::Hotbar>,
    mut create_ui: ResMut<CreateWorldUi>,
    mut list: ResMut<WorldList>,
) {
    // ESC = ถอยกลับ (ทิ้งฟอร์ม)
    if keyboard.just_pressed(KeyCode::Escape) {
        create_ui.status.clear();
        next_state.set(crate::GameState::SinglePlayerMenu);
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone();
    menu_backdrop(&ctx);
    let mut created: Option<(std::path::PathBuf, crate::world_save::WorldMeta)> = None;

    bevy_egui::egui::Window::new("Create World")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(bevy_egui::egui::Align2::CENTER_CENTER, bevy_egui::egui::vec2(0.0, 0.0))
        .show(&ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.heading(bevy_egui::egui::RichText::new("CREATE WORLD").size(32.0).strong());
                ui.add_space(20.0);

                ui.label("World Name:");
                ui.add(
                    bevy_egui::egui::TextEdit::singleline(&mut create_ui.name)
                        .hint_text("world name")
                        .desired_width(220.0),
                );
                ui.add_space(10.0);

                ui.label("Seed:");
                ui.add(
                    bevy_egui::egui::TextEdit::singleline(&mut create_ui.seed)
                        .hint_text("leave empty = random")
                        .desired_width(220.0),
                );
                ui.label(
                    bevy_egui::egui::RichText::new("number or text both work")
                        .small()
                        .weak(),
                );
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    ui.selectable_value(&mut create_ui.survival, false, "Creative");
                    ui.selectable_value(&mut create_ui.survival, true, "Survival");
                });
                ui.add_space(20.0);

                let btn_size = bevy_egui::egui::vec2(200.0, 40.0);
                let name_ok = !create_ui.name.trim().is_empty();
                if ui
                    .add_enabled(
                        name_ok,
                        bevy_egui::egui::Button::new("Create").min_size(btn_size),
                    )
                    .clicked()
                {
                    let seed = crate::world_save::parse_seed(&create_ui.seed);
                    match crate::world_save::create_world(
                        create_ui.name.trim(),
                        seed,
                        create_ui.survival,
                    ) {
                        Ok(world) => created = Some(world),
                        Err(e) => create_ui.status = format!("Create world failed: {e}"),
                    }
                }
                if !create_ui.status.is_empty() {
                    ui.add_space(6.0);
                    ui.label(create_ui.status.clone());
                }
                ui.add_space(10.0);

                if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Back")).clicked() {
                    create_ui.status.clear();
                    next_state.set(crate::GameState::SinglePlayerMenu);
                }
                ui.add_space(20.0);
            });
        });

    if let Some((dir, meta)) = created {
        create_ui.status.clear();
        list.0.insert(0, (dir.clone(), meta.clone()));
        enter_world(
            &mut settings,
            &mut regenerate,
            &mut hotbar,
            &mut next_state,
            Some(dir),
            meta.seed,
            meta.mode(),
            crate::TerrainSource::Noise,
            false,
        );
    }
}

/// ทางเข้าแบบเดิมก่อนมีระบบ world — เข้าเกมไวๆ ไว้จูนค่า/เทส (เปิด Game Settings เต็ม)
pub fn dev_menu_system(
    mut contexts: bevy_egui::EguiContexts,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<crate::GameState>>,
    mut settings: ResMut<crate::GameSettings>,
    mut regenerate: ResMut<crate::RegenerateWorld>,
    mut hotbar: ResMut<crate::voxel::Hotbar>,
) {
    // ESC = ถอยกลับ
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(crate::GameState::MainMenu);
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone();
    menu_backdrop(&ctx);

    bevy_egui::egui::Window::new("Dev Mode")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(bevy_egui::egui::Align2::CENTER_CENTER, bevy_egui::egui::vec2(0.0, 0.0))
        .show(&ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.heading(bevy_egui::egui::RichText::new("DEV MODE").size(32.0).strong());
                ui.label(
                    bevy_egui::egui::RichText::new("saves to saves/ (noise) and saves_dem/ (real world)")
                        .small()
                        .weak(),
                );
                ui.add_space(20.0);

                let btn_size = bevy_egui::egui::vec2(200.0, 40.0);
                let mut mode = settings.game_mode;
                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    ui.selectable_value(&mut mode, crate::GameMode::Creative, "Creative");
                    ui.selectable_value(&mut mode, crate::GameMode::Survival, "Survival");
                });
                settings.game_mode = mode;
                ui.add_space(10.0);

                let seed = settings.noise.seed;
                if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Quick Start (Noise)")).clicked() {
                    enter_world(
                        &mut settings, &mut regenerate, &mut hotbar, &mut next_state,
                        None, seed, mode, crate::TerrainSource::Noise, true,
                    );
                }
                ui.add_space(10.0);

                // โลกจริง 1 บล็อก = 1 ม. — ต้องมีไฟล์ assets/dem/ (สร้างด้วย --convert-dem)
                let has_dem = crate::dem::streamer().is_some();
                let rw_btn = ui.add_enabled(
                    has_dem,
                    bevy_egui::egui::Button::new("Real World (Chiang Mai)").min_size(btn_size),
                );
                if !has_dem {
                    rw_btn.clone().on_disabled_hover_text("assets/dem/ not found - run --build-dem first");
                }
                if rw_btn.clicked() {
                    enter_world(
                        &mut settings, &mut regenerate, &mut hotbar, &mut next_state,
                        None, seed, mode, crate::TerrainSource::RealWorld, true,
                    );
                }
                ui.add_space(10.0);

                if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Back")).clicked() {
                    next_state.set(crate::GameState::MainMenu);
                }
                ui.add_space(20.0);
            });
        });
}

/// หน้าจอ join เกมผ่าน IP (เข้าจากปุ่ม Multiplayer ในเมนูหลัก)
pub fn multiplayer_menu_system(
    mut contexts: bevy_egui::EguiContexts,
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<crate::GameState>>,
    mut mp_ui: ResMut<crate::network::MultiplayerUi>,
    client: Option<Res<bevy_renet::RenetClient>>,
    settings: Res<crate::GameSettings>,
) {
    let connecting = client.is_some();
    // ESC = ถอยกลับ (ยกเลิกการเชื่อมต่อที่ค้างด้วย เหมือนกดปุ่ม Back)
    if keyboard.just_pressed(KeyCode::Escape) {
        if connecting {
            crate::network::teardown_client(&mut commands);
        }
        mp_ui.status.clear();
        next_state.set(crate::GameState::MainMenu);
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone();
    menu_backdrop(&ctx);

    bevy_egui::egui::Window::new("Multiplayer Menu")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(bevy_egui::egui::Align2::CENTER_CENTER, bevy_egui::egui::vec2(0.0, 0.0))
        .show(&ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.heading(bevy_egui::egui::RichText::new("MULTIPLAYER").size(32.0).strong());
                ui.add_space(20.0);

                let btn_size = bevy_egui::egui::vec2(220.0, 44.0);

                // เปิด host จากตรงนี้ได้เลย — เดิมต้องเข้าโลกก่อนแล้วไปหา
                // "Open to LAN" ใน ESC → Options ซึ่งไม่มีใครหาเจอ
                if ui
                    .add_enabled(
                        !connecting,
                        bevy_egui::egui::Button::new("Host a World").min_size(btn_size),
                    )
                    .clicked()
                {
                    commands.insert_resource(HostIntent);
                    next_state.set(crate::GameState::SinglePlayerMenu);
                }
                ui.label(
                    bevy_egui::egui::RichText::new("pick a world, it opens to LAN automatically")
                        .small()
                        .weak(),
                );

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(14.0);

                ui.label("Join with IP:");
                ui.add_enabled(
                    !connecting,
                    bevy_egui::egui::TextEdit::singleline(&mut mp_ui.address)
                        .hint_text("192.168.1.10 or 192.168.1.10:5000")
                        .desired_width(220.0),
                );
                ui.add_space(10.0);

                if ui.add_enabled(!connecting, bevy_egui::egui::Button::new("Join").min_size(btn_size)).clicked() {
                    crate::network::start_client(&mut commands, &mut mp_ui, settings.noise, settings.terrain_source);
                }
                if !mp_ui.status.is_empty() {
                    ui.add_space(6.0);
                    ui.label(mp_ui.status.clone());
                }
                ui.add_space(10.0);

                if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Back")).clicked() {
                    if connecting {
                        crate::network::teardown_client(&mut commands);
                    }
                    mp_ui.status.clear();
                    next_state.set(crate::GameState::MainMenu);
                }
                ui.add_space(20.0);
            });
        });
}

/// Pause menu: ESC ในเกมเปิด/ปิด — โลกเดินต่อ แค่หยุด input ผู้เล่น (ดู run_if ใน main.rs)
pub fn pause_menu_system(
    mut contexts: bevy_egui::EguiContexts,
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut paused: ResMut<crate::Paused>,
    mut next_state: ResMut<NextState<crate::GameState>>,
    mut server: Option<ResMut<bevy_renet::RenetServer>>,
    mut client: Option<ResMut<bevy_renet::RenetClient>>,
    mut client_sync: Option<ResMut<crate::network::ClientSync>>,
    mut inventory: ResMut<crate::voxel::InventoryOpen>,
    settings: Res<crate::GameSettings>,
    mut show_options: ResMut<ShowOptions>,
    chat: Res<ChatState>,
) {
    // แชทเปิดอยู่ ESC เป็นการปิดแชท (chat_input_system รันก่อนหน้าจัดการไปแล้ว)
    if chat.open {
        return;
    }
    // ESC จัดการที่เดียวตรงนี้ ไม่แบ่งไปอยู่คนละ schedule กับหน้าต่างช่องเก็บของ
    // (ไม่งั้นลำดับ Update เทียบ EguiPrimaryContextPass จะกำหนดว่า pause เด้งทับหรือไม่)
    if keyboard.just_pressed(KeyCode::Escape) {
        if inventory.0 {
            inventory.0 = false;
        } else {
            paused.0 = !paused.0;
        }
    }
    if !paused.0 {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone();

    bevy_egui::egui::Window::new("Pause Menu")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(bevy_egui::egui::Align2::CENTER_CENTER, bevy_egui::egui::vec2(0.0, 0.0))
        .show(&ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.heading(bevy_egui::egui::RichText::new("PAUSED").size(32.0).strong());
                ui.add_space(20.0);

                let btn_size = bevy_egui::egui::vec2(200.0, 40.0);

                if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Back to Game")).clicked() {
                    paused.0 = false;
                    show_options.0 = false;
                }
                ui.add_space(10.0);

                // dev mode มีหน้าต่าง Game Settings ลอยอยู่แล้ว ไม่ต้องมีปุ่มนี้
                if !settings.dev_mode {
                    if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Options")).clicked() {
                        show_options.0 = !show_options.0;
                    }
                    ui.add_space(10.0);
                }

                if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Back to Main Menu")).clicked() {
                    paused.0 = false;
                    if let Some(server) = server.as_mut() {
                        // ปิด host: เตะทุกคนออกก่อน แล้วถอด resource เฟรมถัดไป
                        // (ให้ disconnect packet ได้ flush — ดู StopHostRequested)
                        server.disconnect_all();
                        commands.insert_resource(crate::network::StopHostRequested);
                        next_state.set(crate::GameState::MainMenu);
                    } else if let (Some(client), Some(cs)) = (client.as_mut(), client_sync.as_mut()) {
                        // ออกจากเซิร์ฟเวอร์แบบตั้งใจ — watchdog เห็น disconnect แล้ว
                        // จัดการคืน noise + ล้างโลก + กลับ MainMenu ให้ (flag leaving)
                        cs.leaving = true;
                        client.disconnect();
                    } else {
                        // single player: OnExit(InGame) เซฟ chunk ที่ค้างแล้วล้างโลกทิ้ง
                        // (ดู voxel::unload_world_on_exit)
                        next_state.set(crate::GameState::MainMenu);
                    }
                }
                ui.add_space(10.0);

                if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Quit Game")).clicked() {
                    // ห้าม process::exit ตรงๆ — ข้าม unload_world_on_exit แล้ว chunk
                    // ที่ dirty (น้ำ/ระเบิดเพิ่งแก้) หายเงียบๆ ให้กลับ MainMenu ก่อน
                    // (OnExit เซฟให้) แล้ว quit_after_save ค่อยปิดโปรแกรม
                    paused.0 = false;
                    if let Some(server) = server.as_mut() {
                        server.disconnect_all();
                        commands.insert_resource(crate::network::StopHostRequested);
                    } else if let (Some(client), Some(cs)) = (client.as_mut(), client_sync.as_mut()) {
                        cs.leaving = true;
                        client.disconnect();
                    }
                    commands.insert_resource(QuitAfterSave);
                    next_state.set(crate::GameState::MainMenu);
                }

                ui.add_space(20.0);
            });
        });
}

/// กด Quit จากใน pause menu — รอให้ออกจากโลกเสร็จ (OnExit เซฟ chunk ค้างแล้ว)
/// ค่อยปิดโปรแกรมจริง
#[derive(Resource)]
pub struct QuitAfterSave;

pub fn quit_after_save(
    requested: Option<Res<QuitAfterSave>>,
    state: Res<State<crate::GameState>>,
    mut app_exit: MessageWriter<bevy::app::AppExit>,
) {
    if requested.is_some() && *state.get() == crate::GameState::MainMenu {
        app_exit.write(bevy::app::AppExit::Success);
    }
}

pub fn update_coordinate_ui_system(
    camera_query: Query<&Transform, With<FreeCamera>>,
    mut text_query: Query<&mut Text, With<CoordinateText>>,
    settings: Res<crate::GameSettings>,
) {
    if let Ok(camera_transform) = camera_query.single() {
        if let Ok(mut text) = text_query.single_mut() {
            let pos = camera_transform.translation;
            // อัปเดตข้อความบนจอ
            text.0 = format!("X: {:.2}, Y: {:.2}, Z: {:.2}", pos.x, pos.y, pos.z);
            // โลกจริง: โชว์พิกัด GPS + ความสูงจากระดับน้ำทะเลจริง (เทียบแผนที่ได้เลย)
            if settings.terrain_source == crate::TerrainSource::RealWorld
                && crate::dem::streamer().is_some()
            {
                {
                    let (lat, lon) = crate::dem::block_to_latlon(pos.x as f64, pos.z as f64);
                    let elev = pos.y - crate::dem::DEM_SEA_LEVEL_Y as f32;
                    text.0.push_str(&format!(
                        "\nGPS: {:.5}°N {:.5}°E  elev {:.0} m",
                        lat, lon, elev
                    ));
                }
            }
        }
    }
}

pub fn update_mode_text(
    interaction_mode: Res<crate::voxel::InteractionMode>,
    settings: Res<crate::GameSettings>,
    mut text_query: Query<&mut Text, With<ModeText>>,
) {
    if interaction_mode.is_added() || interaction_mode.is_changed()
        || settings.is_added() || settings.is_changed()
    {
        for mut text in &mut text_query {
            text.0 = format!("Mode: {:?} | {:?}", *interaction_mode, settings.game_mode);
        }
    }
}

/// แสดงบล็อกที่เล็งอยู่ + บล็อกที่เลือกไว้วาง (จาก resource ที่ระบบ voxel เขียน)
pub fn update_block_target_text(
    target: Res<crate::voxel::TargetedBlock>,
    selected: Res<crate::voxel::SelectedBlock>,
    mut text_query: Query<&mut Text, With<BlockIdText>>,
) {
    if let Ok(mut text) = text_query.single_mut() {
        let looking_at = match target.0 {
            Some(hit) => crate::voxel::block_name(hit.block),
            None => "None",
        };
        text.0 = format!(
            "Block: {} | Place [1-9]: {}",
            looking_at,
            crate::voxel::block_name(selected.0)
        );
    }
}

pub fn update_fps_text(
    time: Res<Time>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    world: Res<crate::voxel::VoxelWorld>,
    mut query: Query<&mut Text, With<FpsText>>,
    mut sys_info: Local<sysinfo::System>,
    mut refresh_timer: Local<f32>,
    mut ram_usage_mb: Local<f64>,
) {
    // sysinfo เป็น syscall ที่แพง — refresh แค่วินาทีละครั้งพอ
    *refresh_timer -= time.delta_secs();
    if *refresh_timer <= 0.0 {
        *refresh_timer = 1.0;

        sys_info.refresh_memory();
        let pid = sysinfo::Pid::from_u32(std::process::id());
        sys_info.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            true,
            sysinfo::ProcessRefreshKind::nothing().with_memory(),
        );
        if let Some(process) = sys_info.process(pid) {
            *ram_usage_mb = process.memory() as f64 / 1_048_576.0;
        }
    }

    for mut text in &mut query {
        let mut display_string = String::new();

        if let Some(fps) = diagnostics.get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS) {
            if let Some(value) = fps.smoothed() {
                display_string.push_str(&format!("FPS: {:>3.0}", value));
            }
        }

        if let Some(frame_time) = diagnostics.get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FRAME_TIME) {
            if let Some(value) = frame_time.smoothed() {
                if !display_string.is_empty() { display_string.push_str(", "); }
                display_string.push_str(&format!("Time: {:>4.1}ms", value));
            }
        }

        if let Some(entities) = diagnostics.get(&bevy::diagnostic::EntityCountDiagnosticsPlugin::ENTITY_COUNT) {
            if let Some(value) = entities.smoothed() {
                if !display_string.is_empty() { display_string.push_str(", "); }
                display_string.push_str(&format!("Entities: {:>4}", value as u32));
            }
        }

        // Draw Calls (Chunks Rendered)
        let draw_calls = world.generated_chunks.len() + world.water_chunks.len();
        display_string.push_str(&format!("\nDraw Calls: {:>4}", draw_calls));

        // Polygons (Triangles)
        let polygons = world.total_indices / 3;
        display_string.push_str(&format!(", Polygons: {:>7}", polygons));

        // VRAM Usage Estimate (40 bytes per vertex + 4 bytes per index)
        let vram_bytes = (world.total_vertices * 40) + (world.total_indices * 4);
        let vram_mb = vram_bytes as f64 / 1_048_576.0;
        display_string.push_str(&format!(", VRAM: {:>5.1} MB", vram_mb));

        // RAM Usage
        display_string.push_str(&format!("\nRAM: {:>5.1} MB", *ram_usage_mb));

        if !display_string.is_empty() {
            text.0 = display_string;
        }
    }
}

/// เปิดหน้าต่าง Options อยู่ไหม (กดจาก pause menu — เฉพาะโลกที่ไม่ใช่ dev mode)
#[derive(Resource, Default)]
pub struct ShowOptions(pub bool);

/// Options พื้นฐานสำหรับโลกปกติ — เอาเฉพาะที่ผู้เล่นควรได้แตะ
/// ของสาย dev (สไลเดอร์ noise, Regenerate, Render Mode, GPS, TNT tuning,
/// wireframe) อยู่ใน [`egui_settings_system`] ซึ่งขึ้นเฉพาะ dev mode
pub fn options_menu_system(
    mut contexts: bevy_egui::EguiContexts,
    mut commands: Commands,
    mut settings: ResMut<crate::GameSettings>,
    mut regenerate: ResMut<crate::RegenerateWorld>,
    mut show_options: ResMut<ShowOptions>,
    mut camera_query: Query<&mut crate::camera::FreeCamera>,
    mut proj_query: Query<&mut Projection, With<crate::camera::MainCamera>>,
    (mut server, mut client, lan_info, world, mut mp_ui): (
        Option<ResMut<bevy_renet::RenetServer>>,
        Option<ResMut<bevy_renet::RenetClient>>,
        Option<Res<crate::network::LanInfo>>,
        Res<crate::voxel::VoxelWorld>,
        ResMut<crate::network::MultiplayerUi>,
    ),
    // สถานะรอยืนยันของปุ่มลบเซฟ — ปุ่มอันตราย ห้ามลบด้วยคลิกเดียว
    mut confirm_clear: Local<bool>,
) {
    if settings.dev_mode || !show_options.0 {
        *confirm_clear = false;
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone();
    let networked = server.is_some() || client.is_some();

    bevy_egui::egui::Window::new("Options")
        .resizable(false)
        .collapsible(false)
        .anchor(bevy_egui::egui::Align2::CENTER_CENTER, bevy_egui::egui::vec2(0.0, 0.0))
        .show(&ctx, |ui| {
            ui.heading("Multiplayer");
            if let Some(server) = server.as_mut() {
                let addr = lan_info.as_ref().map(|l| l.0.clone()).unwrap_or_default();
                ui.label(format!("Hosting on {addr}"));
                ui.label(format!("Players: {}", server.connected_clients() + 1));
                ui.horizontal(|ui| {
                    if ui.button("Copy IP").clicked() {
                        ui.ctx().copy_text(addr.clone());
                    }
                    if ui.button("Close LAN").clicked() {
                        server.disconnect_all();
                        commands.insert_resource(crate::network::StopHostRequested);
                    }
                });
            } else if let Some(client) = client.as_mut() {
                ui.label("Connected to host");
                if ui.button("Disconnect").clicked() {
                    client.disconnect();
                }
            } else {
                if ui.button("Open to LAN").clicked() {
                    crate::network::start_host(&mut commands, &world, &mut mp_ui);
                }
                if !mp_ui.status.is_empty() {
                    ui.label(mp_ui.status.clone());
                }
            }

            ui.separator();

            ui.heading("World");
            ui.add(
                bevy_egui::egui::Slider::new(&mut settings.render_distance, 2..=32)
                    .text("Render Distance"),
            );
            // chunk ที่เซฟไว้ override การ generate เสมอ — ปุ่มนี้คืนโลกกลับเป็นตอนสร้าง
            // (เฉพาะโฟลเดอร์ของโลกนี้) ตอนต่อ network ห้ามแตะ = desync
            // สองจังหวะ: คลิกแรกแค่ขอยืนยัน ลบจริงต้องกดปุ่มแดง
            ui.add_enabled_ui(!networked, |ui| {
                if !*confirm_clear {
                    if ui.button("Clear Saved Edits...").clicked() {
                        *confirm_clear = true;
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Delete all saved edits in this world?");
                        if ui.button(
                            bevy_egui::egui::RichText::new("Yes, clear")
                                .color(bevy_egui::egui::Color32::from_rgb(255, 120, 120)),
                        ).clicked() {
                            let dir = crate::voxel::active_save_dir();
                            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                                // เก็บ world.json ไว้ ลบเฉพาะ chunk
                                if entry.path().extension().is_some_and(|e| e == "bin") {
                                    let _ = std::fs::remove_file(entry.path());
                                }
                            }
                            regenerate.0 = true;
                            *confirm_clear = false;
                        }
                        if ui.button("Cancel").clicked() {
                            *confirm_clear = false;
                        }
                    });
                }
            });

            ui.separator();

            ui.heading("Environment");
            ui.add(
                bevy_egui::egui::Slider::new(&mut settings.time_of_day, 0.0..=24.0)
                    .text("Time of Day (h)"),
            );
            // มีผลเฉพาะ single player/host (client รับผลจาก host อยู่แล้ว)
            ui.add(
                bevy_egui::egui::Slider::new(&mut settings.fluid_tick_seconds, 0.02..=1.0)
                    .logarithmic(true)
                    .text("Water Tick (s)"),
            );

            ui.separator();

            ui.heading("Graphics");
            ui.checkbox(&mut settings.lod_enabled, "Distant Terrain (LOD)");
            ui.add(
                bevy_egui::egui::Slider::new(&mut settings.lod_distance_m, 2_000.0..=35_000.0)
                    .logarithmic(true)
                    .text("LOD Distance (m)"),
            );
            // FOV: แก้ที่ Projection ของกล้องตรงๆ ไม่เก็บซ้ำใน GameSettings
            if let Some(mut projection) = proj_query.iter_mut().next() {
                if let Projection::Perspective(p) = &mut *projection {
                    let mut fov_deg = p.fov.to_degrees();
                    if ui
                        .add(bevy_egui::egui::Slider::new(&mut fov_deg, 30.0..=110.0).text("FOV (deg)"))
                        .changed()
                    {
                        p.fov = fov_deg.to_radians();
                    }
                }
            }

            ui.separator();

            ui.heading("Camera");
            if let Some(mut camera) = camera_query.iter_mut().next() {
                ui.add(bevy_egui::egui::Slider::new(&mut camera.speed, 10.0..=200.0).text("Fly Speed"));
                let mode = if camera.fly { "Fly" } else { "Walk" };
                ui.label(format!("Mode: {} (press F to toggle)", mode));
            }

            ui.separator();

            ui.label("ESC: pause | F: fly/walk | F5: 3rd person | F3: debug | 1-9/scroll: hotbar");
            ui.label("E: inventory | Q: drop item | T or /: chat | middle click: pick block | hold Chisel: sub-voxel");

            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                if ui
                    .add_sized(
                        bevy_egui::egui::vec2(200.0, 32.0),
                        bevy_egui::egui::Button::new("Close"),
                    )
                    .clicked()
                {
                    show_options.0 = false;
                }
            });
        });
}

pub fn egui_settings_system(
    mut contexts: bevy_egui::EguiContexts,
    mut commands: Commands,
    mut settings: ResMut<crate::GameSettings>,
    mut regenerate: ResMut<crate::RegenerateWorld>,
    mut camera_query: Query<&mut crate::camera::FreeCamera>,
    mut proj_query: Query<&mut Projection, With<crate::camera::MainCamera>>,
    mut cam_transform: Query<&mut Transform, With<crate::camera::FreeCamera>>,
    mut wireframe_config: ResMut<bevy::pbr::wireframe::WireframeConfig>,
    mut teleport: ResMut<TeleportUi>,
    mut hotbar: ResMut<crate::voxel::Hotbar>,
    (mut server, mut client, lan_info, world, mut mp_ui): (
        Option<ResMut<bevy_renet::RenetServer>>,
        Option<ResMut<bevy_renet::RenetClient>>,
        Option<Res<crate::network::LanInfo>>,
        Res<crate::voxel::VoxelWorld>,
        ResMut<crate::network::MultiplayerUi>,
    ),
    mut confirm_clear: Local<bool>,
) {
    // โลกปกติใช้ Options พื้นฐานจาก pause menu แทน (ดู [`options_menu_system`])
    if !settings.dev_mode {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let networked = server.is_some() || client.is_some();

    bevy_egui::egui::Window::new("Game Settings").show(ctx, |ui| {
        ui.heading("Multiplayer");
        if let Some(server) = server.as_mut() {
            let addr = lan_info.as_ref().map(|l| l.0.clone()).unwrap_or_default();
            ui.label(format!("Hosting on {addr}"));
            ui.label(format!("Players: {}", server.connected_clients() + 1));
            ui.horizontal(|ui| {
                if ui.button("Copy IP").clicked() {
                    ui.ctx().copy_text(addr.clone());
                }
                if ui.button("Close LAN").clicked() {
                    server.disconnect_all();
                    // ถอด resource เฟรมหน้า ให้ disconnect packet ได้ flush ก่อน
                    commands.insert_resource(crate::network::StopHostRequested);
                }
            });
        } else if let Some(client) = client.as_mut() {
            ui.label("Connected to host");
            if ui.button("Disconnect").clicked() {
                // watchdog เห็น is_disconnected แล้วจัดการคืนค่า + กลับเมนูให้เอง
                client.disconnect();
            }
        } else {
            if ui.button("Open to LAN").clicked() {
                crate::network::start_host(&mut commands, &world, &mut mp_ui);
            }
            if !mp_ui.status.is_empty() {
                ui.label(mp_ui.status.clone());
            }
        }

        ui.separator();

        // โหมดเล่น — client-local, สลับได้ทุกเมื่อ (rebuild inventory ตามโหมด)
        ui.heading("Game Mode");
        let mut mode = settings.game_mode;
        ui.horizontal(|ui| {
            ui.radio_value(&mut mode, crate::GameMode::Creative, "Creative");
            ui.radio_value(&mut mode, crate::GameMode::Survival, "Survival");
        });
        if mode != settings.game_mode {
            settings.game_mode = mode;
            *hotbar = crate::voxel::Hotbar::for_mode(mode);
        }
        ui.label(
            bevy_egui::egui::RichText::new("switching mode clears inventory")
                .small()
                .weak(),
        );

        ui.separator();

        // ตอนเล่น multiplayer ห้ามแตะ world gen — noise ที่ไม่ตรงกัน = desync ทันที
        ui.add_enabled_ui(!networked, |ui| {
        ui.heading("World Generation");
        ui.add(bevy_egui::egui::Slider::new(&mut settings.render_distance, 2..=32).text("Render Distance"));

        // สลับชนิดโลกกลางเกมได้ (ล้างโลกใหม่ + สลับโฟลเดอร์เซฟให้เอง)
        let mut src = settings.terrain_source;
        ui.horizontal(|ui| {
            ui.label("Terrain:");
            ui.radio_value(&mut src, crate::TerrainSource::Noise, "Noise");
            ui.add_enabled_ui(crate::dem::streamer().is_some(), |ui| {
                ui.radio_value(&mut src, crate::TerrainSource::RealWorld, "Real World");
            });
        });
        if src != settings.terrain_source {
            select_terrain(&mut settings, &mut regenerate, src);
        }

        let mut regen = false;

        ui.horizontal(|ui| {
            ui.label("Render Mode:");
            regen |= ui.radio_value(&mut settings.render_mode, crate::RenderMode::Full, "Full").changed();
            regen |= ui.radio_value(&mut settings.render_mode, crate::RenderMode::SurfacePreview, "Surface Preview").changed();
        });

        // ใช้ drag_stopped ไม่ใช่ changed — ไม่งั้นจะ regen รัวๆ ระหว่างลาก
        regen |= ui.add(
            bevy_egui::egui::Slider::new(&mut settings.noise.frequency, 0.001..=0.1)
                .logarithmic(true)
                .text("Noise Frequency"),
        ).drag_stopped();
        regen |= ui.add(
            bevy_egui::egui::Slider::new(&mut settings.noise.amplitude, 5.0..=150.0)
                .text("Noise Amplitude"),
        ).drag_stopped();
        regen |= ui.add(
            bevy_egui::egui::Slider::new(&mut settings.noise.octaves, 1..=8)
                .text("Noise Octaves"),
        ).drag_stopped();

        ui.horizontal(|ui| {
            if ui.button("Regenerate World").clicked() {
                regen = true;
            }
            // chunk ที่เซฟไว้จะ override การ generate เสมอ — ปุ่มนี้ล้างเซฟทิ้ง
            // (เฉพาะไฟล์ chunk ของโลกที่กำลังเล่น — world.json/โลกอื่นไม่โดน)
            // สองจังหวะกันคลิกพลาด เหมือน options_menu_system
            if !*confirm_clear {
                if ui.button("Clear Saved Edits...").clicked() {
                    *confirm_clear = true;
                }
            } else {
                if ui.button(
                    bevy_egui::egui::RichText::new("Yes, clear")
                        .color(bevy_egui::egui::Color32::from_rgb(255, 120, 120)),
                ).clicked() {
                    let dir = crate::voxel::active_save_dir();
                    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                        if entry.path().extension().is_some_and(|e| e == "bin") {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                    regen = true;
                    *confirm_clear = false;
                }
                if ui.button("Cancel").clicked() {
                    *confirm_clear = false;
                }
            }
        });

        if regen && !networked {
            regenerate.0 = true;
        }
        }); // add_enabled_ui(!networked)

        // Teleport ด้วยพิกัดจริง — เฉพาะโลกจริง (มีไฟล์ dem) ก๊อป lat/lon จาก
        // Google Maps มาวางแล้วไปโผล่ที่นั่นในเกมได้เลย
        if settings.terrain_source == crate::TerrainSource::RealWorld {
            if let Some(dem) = crate::dem::streamer() {
                ui.separator();
                ui.heading("Teleport (GPS)");
                ui.horizontal(|ui| {
                    ui.label("Lat:");
                    ui.add(bevy_egui::egui::TextEdit::singleline(&mut teleport.lat).desired_width(90.0));
                    ui.label("Lon:");
                    ui.add(bevy_egui::egui::TextEdit::singleline(&mut teleport.lon).desired_width(90.0));
                });
                if ui.button("Go").clicked() {
                    match (teleport.lat.trim().parse::<f64>(), teleport.lon.trim().parse::<f64>()) {
                        (Ok(lat), Ok(lon)) if dem.has_tile_at(lat, lon) => {
                            let (bx, bz) = crate::dem::latlon_to_block(lat, lon);
                            // โหลด tile ปลายทาง blocking ก่อน จะได้ความสูงถูก (ไม่ใช่ทะเล)
                            dem.load_blocking_at(bx, bz);
                            let h = crate::dem::DEM_SEA_LEVEL_Y as f32 + dem.elevation_at_block(bx, bz);
                            if let Some(mut t) = cam_transform.iter_mut().next() {
                                t.translation = Vec3::new(bx as f32, h + 20.0, bz as f32);
                            }
                            teleport.status = format!("moved to {:.4}, {:.4} (surface {:.0} m)", lat, lon, h);
                        }
                        (Ok(lat), Ok(lon)) => {
                            teleport.status = format!("{:.4}, {:.4} is outside this tile", lat, lon);
                        }
                        _ => teleport.status = "enter lat/lon as numbers (e.g. 18.5885, 98.4867)".into(),
                    }
                }
                if !teleport.status.is_empty() {
                    ui.label(teleport.status.clone());
                }
            }
        }

        ui.separator();

        ui.heading("Environment");
        ui.add(
            bevy_egui::egui::Slider::new(&mut settings.time_of_day, 0.0..=24.0)
                .text("Time of Day (h)"),
        );
        // ความเร็วน้ำ: คาบ tick ของ fluid sim — มีผลเฉพาะ single player/host
        // (client รับผลจาก host อยู่แล้ว ปรับไปก็ไม่เปลี่ยนอะไร)
        ui.add(
            bevy_egui::egui::Slider::new(&mut settings.fluid_tick_seconds, 0.02..=1.0)
                .logarithmic(true)
                .text("Water Tick (s)"),
        );

        ui.separator();

        ui.heading("Explosion");
        // มีผลเฉพาะ host/single (client ส่งจุดชนวนไปให้ host คำนวณ)
        ui.add(bevy_egui::egui::Slider::new(&mut settings.tnt_power, 4.0..=25.0).text("TNT Power"));
        ui.add(bevy_egui::egui::Slider::new(&mut settings.tnt_fuse_seconds, 0.5..=5.0).text("TNT Fuse (s)"));
        ui.add(
            bevy_egui::egui::Slider::new(&mut settings.nuke_yield, 100.0..=4000.0)
                .logarithmic(true)
                .text("Nuke Yield (TNT)"),
        );
        // รัศมีโดยประมาณจากสูตร: ray วิ่งได้ไกลสุด = พลังงาน/แรงตกต่อบล็อก
        let nuke_reach = settings.tnt_power * settings.nuke_yield.cbrt() / 0.25;
        ui.label(format!("  ≈ blast reach {:.0} blocks", nuke_reach));
        ui.add(bevy_egui::egui::Slider::new(&mut settings.nuke_fuse_seconds, 1.0..=15.0).text("Nuke Fuse (s)"));

        ui.separator();

        ui.heading("Distant Terrain");
        ui.checkbox(&mut settings.lod_enabled, "LOD (Distant Horizons style)");
        ui.add(
            bevy_egui::egui::Slider::new(&mut settings.lod_distance_m, 2_000.0..=35_000.0)
                .logarithmic(true)
                .text("LOD Distance (m)"),
        );

        ui.separator();

        ui.heading("Camera");
        if let Some(mut camera) = camera_query.iter_mut().next() {
            ui.add(bevy_egui::egui::Slider::new(&mut camera.speed, 10.0..=200.0).text("Fly Speed"));
            let mode = if camera.fly { "Fly" } else { "Walk" };
            ui.label(format!("Mode: {} (press F to toggle)", mode));
        }
        // FOV: แก้ที่ Projection ของกล้องตรงๆ ไม่เก็บซ้ำใน GameSettings
        if let Some(mut projection) = proj_query.iter_mut().next() {
            if let Projection::Perspective(p) = &mut *projection {
                let mut fov_deg = p.fov.to_degrees();
                if ui.add(bevy_egui::egui::Slider::new(&mut fov_deg, 30.0..=110.0).text("FOV (deg)")).changed() {
                    p.fov = fov_deg.to_radians();
                }
            }
        }

        ui.separator();

        ui.heading("Debug");
        ui.checkbox(&mut wireframe_config.global, "Wireframe");
        // มีผลเฉพาะ host/single (ray คำนวณที่นั่น) — เปิดก่อนจุด TNT
        ui.checkbox(&mut settings.show_tnt_rays, "Show TNT Rays");

        ui.separator();
        ui.label("ESC: pause | F: fly/walk | F5: 3rd person | F3: debug | 1-9/scroll: hotbar");
        ui.label("E: inventory | Q: drop item | T or /: chat | middle click: pick block | hold Chisel: sub-voxel");
    });
}
