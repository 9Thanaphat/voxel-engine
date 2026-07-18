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

/// กรอบช่อง hotbar (index 0..9) — border เปลี่ยนสีตามช่องที่เลือก
#[derive(Component)]
pub struct HotbarSlotUi(pub usize);

/// icon ข้างในช่อง — เป็น ImageNode (บล็อกมี texture) หรือสี่เหลี่ยมสี (ไม่มี)
#[derive(Component)]
pub struct HotbarSlotIcon(pub usize);

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
        parent.spawn((
            Node {
                width: Val::Px(10.0),
                height: Val::Px(10.0),
                ..default()
            },
            BackgroundColor(Color::WHITE),
        ));
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
        Visibility::Hidden,
    )).with_children(|parent| {
        for i in 0..9 {
            parent.spawn((
                Node {
                    width: Val::Px(48.0),
                    height: Val::Px(48.0),
                    border: UiRect::all(Val::Px(3.0)),
                    padding: UiRect::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                BorderColor::all(Color::srgba(0.3, 0.3, 0.3, 0.8)),
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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            InGameUi,
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
) {
    if !hotbar.is_changed() {
        return;
    }

    for (slot, mut border) in &mut slot_query {
        *border = if slot.0 == hotbar.selected {
            BorderColor::all(Color::WHITE)
        } else {
            BorderColor::all(Color::srgba(0.3, 0.3, 0.3, 0.8))
        };
    }

    for (entity, icon, mut bg) in &mut icon_query {
        match hotbar.slots[icon.0] {
            Some(stack) => {
                if let Some(tex) = crate::voxel::hotbar_icon_texture(stack.block) {
                    commands.entity(entity).insert(ImageNode::new(asset_server.load(tex)));
                    bg.0 = Color::NONE;
                } else {
                    commands.entity(entity).remove::<ImageNode>();
                    let c = crate::voxel::block_color(stack.block);
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
    paused: Res<crate::Paused>,
    mut cursor_query: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
) {
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
                for (n, &block) in crate::voxel::PLACEABLE_BLOCKS.iter().enumerate() {
                    let c = crate::voxel::block_color(block);
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
                        bevy_egui::egui::RichText::new(crate::voxel::block_name(block)).color(text_color),
                    )
                    .fill(fill)
                    .min_size(bevy_egui::egui::vec2(96.0, 40.0));

                    if ui.add(btn).clicked() {
                        let sel = hotbar.selected;
                        hotbar.slots[sel] =
                            Some(crate::voxel::ItemStack { block, count: None });
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

pub fn toggle_ingame_ui(
    state: Res<State<crate::GameState>>,
    mut query: Query<&mut Visibility, With<InGameUi>>,
) {
    if state.is_changed() || state.is_added() {
        let is_ingame = *state.get() == crate::GameState::InGame;
        for mut vis in &mut query {
            *vis = if is_ingame { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

pub fn main_menu_system(
    mut contexts: bevy_egui::EguiContexts,
    mut next_state: ResMut<NextState<crate::GameState>>,
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

            if ui.add_sized(btn_size, bevy_egui::egui::Button::new("Singleplayer")).clicked() {
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
                    crate::network::start_client(&mut commands, &mut mp_ui, settings.noise);
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
) {
    if let Ok(camera_transform) = camera_query.single() {
        if let Ok(mut text) = text_query.single_mut() {
            let pos = camera_transform.translation;
            // อัปเดตข้อความบนจอ
            text.0 = format!("X: {:.2}, Y: {:.2}, Z: {:.2}", pos.x, pos.y, pos.z);
        }
    }
}

pub fn update_mode_text(
    interaction_mode: Res<crate::voxel::InteractionMode>,
    mut text_query: Query<&mut Text, With<ModeText>>,
) {
    if interaction_mode.is_added() || interaction_mode.is_changed() {
        for mut text in &mut text_query {
            text.0 = format!("Mode: {:?}", *interaction_mode);
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
    mut wireframe_config: ResMut<bevy::pbr::wireframe::WireframeConfig>,
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

        // ตอนเล่น multiplayer ห้ามแตะ world gen — noise ที่ไม่ตรงกัน = desync ทันที
        ui.add_enabled_ui(!networked, |ui| {
        ui.heading("World Generation");
        ui.add(bevy_egui::egui::Slider::new(&mut settings.render_distance, 2..=32).text("Render Distance"));

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
            if ui.button("Clear Saved Edits").clicked() {
                let _ = std::fs::remove_dir_all(crate::voxel::project_root().join("saves"));
                regen = true;
            }
        });

        if regen && !networked {
            regenerate.0 = true;
        }
        }); // add_enabled_ui(!networked)

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

        ui.heading("Camera");
        if let Some(mut camera) = camera_query.iter_mut().next() {
            ui.add(bevy_egui::egui::Slider::new(&mut camera.speed, 10.0..=200.0).text("Fly Speed"));
            let mode = if camera.fly { "Fly" } else { "Walk" };
            ui.label(format!("Mode: {} (press F to toggle)", mode));
        }

        ui.separator();

        ui.heading("Debug");
        ui.checkbox(&mut wireframe_config.global, "Wireframe");

        ui.separator();
        ui.label("ESC: unlock mouse | F: fly/walk | 1-9/scroll: hotbar slot");
        ui.label("Middle click: pick block | E: block picker | T: subvoxel mode");
    });
}
