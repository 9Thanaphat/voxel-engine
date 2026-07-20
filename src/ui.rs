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

/// อัปเดตกรอบ+icon ของ hotbar — ทำงานเฉพาะตอน Hotbar เปลี่ยน (รวมเฟรมแรก)
/// เลี่ยงปัญหาลำดับ Startup: FACE_TEXTURES ถูก init ใน setup_voxel ซึ่งเสร็จ
/// ก่อน Update เฟรมแรกแน่นอน
pub fn update_hotbar_ui(
    hotbar: Res<crate::voxel::Hotbar>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
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
                if let Some(tex) = stack.item.icon_texture() {
                    commands.entity(entity).insert(ImageNode::new(asset_server.load(tex)));
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

/// หน้าต่างเลือกบล็อกลงช่อง hotbar — E เปิด/ปิด, ESC ปิด (pause_menu_system
/// ต้องรันก่อนระบบนี้ ให้เห็นว่า picker ยังเปิดอยู่แล้วไม่เปิด pause ทับ)
pub fn block_picker_system(
    mut contexts: bevy_egui::EguiContexts,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut picker: ResMut<crate::voxel::BlockPickerOpen>,
    mut hotbar: ResMut<crate::voxel::Hotbar>,
    settings: Res<crate::GameSettings>,
    paused: Res<crate::Paused>,
    mut cursor_query: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
) {
    // Survival: summon บล็อกฟรีไม่ได้ — ปิด picker ทิ้ง (กัน E เปิด)
    if settings.game_mode != crate::GameMode::Creative {
        if picker.0 {
            picker.0 = false;
        }
        return;
    }
    if keyboard.just_pressed(KeyCode::KeyE) && !paused.0 {
        picker.0 = !picker.0;
        if picker.0 {
            // ปล่อยเมาส์ให้คลิกหน้าต่างได้ — ล็อคกลับด้วยคลิกซ้ายตามเดิม (cursor_grab_system)
            if let Ok(mut cursor) = cursor_query.single_mut() {
                cursor.grab_mode = bevy::window::CursorGrabMode::None;
                cursor.visible = true;
            }
        }
    }
    if picker.0 && keyboard.just_pressed(KeyCode::Escape) {
        picker.0 = false;
    }
    if !picker.0 {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone();

    bevy_egui::egui::Window::new("Block Picker")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(bevy_egui::egui::Align2::CENTER_CENTER, bevy_egui::egui::vec2(0.0, 0.0))
        .show(&ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(14.0);
                ui.heading(bevy_egui::egui::RichText::new("SELECT BLOCK").size(24.0).strong());
                ui.label(format!("ใส่ลงช่อง {} (กด 1-9 เปลี่ยนช่องได้)", hotbar.selected + 1));
                ui.add_space(10.0);
            });

            bevy_egui::egui::Grid::new("block_picker_grid").spacing([6.0, 6.0]).show(ui, |ui| {
                for (n, &item) in crate::voxel::PLACEABLE_ITEMS.iter().enumerate() {
                    let text = item.name();
                    let c = item.color();
                    let fill = bevy_egui::egui::Color32::from_rgb(
                        (c[0] * 255.0) as u8,
                        (c[1] * 255.0) as u8,
                        (c[2] * 255.0) as u8,
                    );
                    // ตัวหนังสือขาว/ดำตามความสว่างพื้นปุ่ม ให้อ่านออกทุกสี
                    let luma = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
                    let text_color = if luma > 0.5 {
                        bevy_egui::egui::Color32::BLACK
                    } else {
                        bevy_egui::egui::Color32::WHITE
                    };
                    let btn = bevy_egui::egui::Button::new(
                        bevy_egui::egui::RichText::new(text).color(text_color),
                    )
                    .fill(fill)
                    .min_size(bevy_egui::egui::vec2(96.0, 40.0));

                    if ui.add(btn).clicked() {
                        let sel = hotbar.selected;
                        hotbar.slots[sel] = Some(crate::voxel::ItemStack {
                            item,
                            count: Some(crate::voxel::max_stack(item)),
                        });
                    }
                    if n % 4 == 3 {
                        ui.end_row();
                    }
                }
            });

            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.label("E / ESC: ปิด");
                ui.add_space(6.0);
            });
        });
}

/// decay ความจ้า + อัปเดต alpha ของ overlay (จ้าเกิน 1.0 = ขาวสนิทค้างไว้ก่อน)
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
) {
    if keyboard.just_pressed(KeyCode::F3) {
        show_debug.0 = !show_debug.0;
    }
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
    crate::voxel::DEM_SAVE_DIR.store(
        source == crate::TerrainSource::RealWorld,
        std::sync::atomic::Ordering::Relaxed,
    );
}

pub fn main_menu_system(
    mut contexts: bevy_egui::EguiContexts,
    mut next_state: ResMut<NextState<crate::GameState>>,
    mut settings: ResMut<crate::GameSettings>,
    mut regenerate: ResMut<crate::RegenerateWorld>,
    mut hotbar: ResMut<crate::voxel::Hotbar>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone(); // In egui 0.35 Context is easily cloned to avoid mutability issues

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
                    .size(50.0)
                    .strong()
            );
            
            ui.add_space(50.0);

            let btn_size = bevy_egui::egui::vec2(200.0, 40.0);

            // เลือกโหมดเล่นก่อนเริ่ม — มีผลตอนกดปุ่มเริ่มเกม (rebuild hotbar)
            ui.horizontal(|ui| {
                ui.label("Mode:");
                ui.selectable_value(&mut settings.game_mode, crate::GameMode::Creative, "Creative");
                ui.selectable_value(&mut settings.game_mode, crate::GameMode::Survival, "Survival");
            });
            ui.add_space(10.0);

            if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Singleplayer")).clicked() {
                select_terrain(&mut settings, &mut regenerate, crate::TerrainSource::Noise);
                *hotbar = crate::voxel::Hotbar::for_mode(settings.game_mode);
                next_state.set(crate::GameState::InGame);
            }
            ui.add_space(10.0);

            // โลกจริง 1 บล็อก = 1 ม. — ต้องมีไฟล์ assets/dem/ (สร้างด้วย --convert-dem)
            let has_dem = crate::dem::streamer().is_some();
            let rw_btn = ui.add_enabled(
                has_dem,
                bevy_egui::egui::Button::new("Real World (Chiang Mai)").min_size(btn_size),
            );
            if !has_dem {
                rw_btn.clone().on_disabled_hover_text("ไม่พบ assets/dem/ — รัน --convert-dem ก่อน");
            }
            if rw_btn.clicked() {
                select_terrain(&mut settings, &mut regenerate, crate::TerrainSource::RealWorld);
                *hotbar = crate::voxel::Hotbar::for_mode(settings.game_mode);
                next_state.set(crate::GameState::InGame);
            }
            ui.add_space(10.0);
            
            if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Multiplayer")).clicked() {
                next_state.set(crate::GameState::MultiplayerMenu);
            }
            ui.add_space(10.0);
            
            if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Option")).clicked() {
                // Not implemented
            }
            ui.add_space(10.0);
            
            if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Quit")).clicked() {
                std::process::exit(0);
            }
            
            ui.add_space(20.0);
        });
    });
}

/// หน้าจอ join เกมผ่าน IP (เข้าจากปุ่ม Multiplayer ในเมนูหลัก)
pub fn multiplayer_menu_system(
    mut contexts: bevy_egui::EguiContexts,
    mut commands: Commands,
    mut next_state: ResMut<NextState<crate::GameState>>,
    mut mp_ui: ResMut<crate::network::MultiplayerUi>,
    client: Option<Res<bevy_renet::RenetClient>>,
    settings: Res<crate::GameSettings>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone();
    let connecting = client.is_some();

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

                ui.label("Server IP:");
                ui.add_enabled(
                    !connecting,
                    bevy_egui::egui::TextEdit::singleline(&mut mp_ui.address)
                        .hint_text("192.168.1.10 หรือ 192.168.1.10:5000")
                        .desired_width(220.0),
                );
                ui.add_space(10.0);

                let btn_size = bevy_egui::egui::vec2(200.0, 40.0);
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
    picker: Res<crate::voxel::BlockPickerOpen>,
) {
    // picker เปิดอยู่ ESC เป็นการปิด picker (block_picker_system รันถัดไปจัดการ)
    if keyboard.just_pressed(KeyCode::Escape) && !picker.0 {
        paused.0 = !paused.0;
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
                }
                ui.add_space(10.0);

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
                        // single player: โลกยังอยู่ใน memory กลับมาเล่นต่อได้เลย
                        // (chunk ที่แก้เซฟลง disk ตลอดอยู่แล้ว)
                        next_state.set(crate::GameState::MainMenu);
                    }
                }
                ui.add_space(10.0);

                if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Quit Game")).clicked() {
                    std::process::exit(0);
                }

                ui.add_space(20.0);
            });
        });
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

pub fn egui_settings_system(
    mut contexts: bevy_egui::EguiContexts,
    mut commands: Commands,
    mut settings: ResMut<crate::GameSettings>,
    mut regenerate: ResMut<crate::RegenerateWorld>,
    mut camera_query: Query<&mut crate::camera::FreeCamera>,
    mut proj_query: Query<&mut Projection, With<crate::camera::FreeCamera>>,
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
) {
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
            bevy_egui::egui::RichText::new("เปลี่ยนโหมดจะล้าง inventory")
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
            // (เฉพาะโฟลเดอร์ของโลกที่กำลังเล่น — โลกอีกชนิดไม่โดน)
            if ui.button("Clear Saved Edits").clicked() {
                let _ = std::fs::remove_dir_all(crate::voxel::active_save_dir());
                regen = true;
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
                            teleport.status = format!("ไปที่ {:.4}, {:.4} (ผิวสูง {:.0} ม.)", lat, lon, h);
                        }
                        (Ok(lat), Ok(lon)) => {
                            teleport.status = format!("{:.4}, {:.4} อยู่นอกขอบเขต tile นี้", lat, lon);
                        }
                        _ => teleport.status = "กรอก lat/lon เป็นตัวเลข (เช่น 18.5885, 98.4867)".into(),
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
        ui.label("ESC: unlock mouse | F: fly/walk | 1-9/scroll: hotbar slot");
        ui.label("Middle click: pick block | E: block picker | T: subvoxel mode");
    });
}
