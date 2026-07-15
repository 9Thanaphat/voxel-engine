use bevy::prelude::*;
use crate::camera::FreeCamera;

#[derive(Component)]
pub struct CoordinateText;

#[derive(Component)]
pub struct FpsText;

#[derive(Component)]
pub struct BlockIdText;

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
            "Block: {} | Place [1-0,-]: {}",
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
    mut settings: ResMut<crate::GameSettings>,
    mut regenerate: ResMut<crate::RegenerateWorld>,
    mut camera_query: Query<&mut crate::camera::FreeCamera>,
    mut wireframe_config: ResMut<bevy::pbr::wireframe::WireframeConfig>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    bevy_egui::egui::Window::new("Game Settings").show(ctx, |ui| {
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

        if regen {
            regenerate.0 = true;
        }

        ui.separator();

        ui.heading("Environment");
        ui.add(
            bevy_egui::egui::Slider::new(&mut settings.time_of_day, 0.0..=24.0)
                .text("Time of Day (h)"),
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
        ui.label("ESC: unlock mouse | F: fly/walk | 1-0,-,=,T: select block");
    });
}
