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

    // Performance Text
    commands.spawn((
        Text::new("FPS: 0, Time: 0ms, Entities: 0"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(5.0),
            right: Val::Px(5.0),
            ..default()
        },
        FpsText,
    ));

    // Coordinate Text
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(15.0),
                top: Val::Px(35.0), // moved down to give space if needed, wait actually coordinates was at top: 15
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("X: 0.00, Y: 0.00, Z: 0.00"),
                TextFont {
                    font_size: bevy::text::FontSize::Px(24.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                CoordinateText,
            ));
            
            parent.spawn((
                Text::new("Block: None"),
                TextFont {
                    font_size: bevy::text::FontSize::Px(24.0),
                    ..default()
                },
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

pub fn update_fps_text(
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    mut query: Query<&mut Text, With<FpsText>>,
) {
    for mut text in &mut query {
        let mut display_string = String::new();
        
        if let Some(fps) = diagnostics.get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS) {
            if let Some(value) = fps.smoothed() {
                display_string.push_str(&format!("FPS: {:.0}", value));
            }
        }
        
        if let Some(frame_time) = diagnostics.get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FRAME_TIME) {
            if let Some(value) = frame_time.smoothed() {
                if !display_string.is_empty() { display_string.push_str(", "); }
                display_string.push_str(&format!("Time: {:.1}ms", value));
            }
        }
        
        if let Some(entities) = diagnostics.get(&bevy::diagnostic::EntityCountDiagnosticsPlugin::ENTITY_COUNT) {
            if let Some(value) = entities.smoothed() {
                if !display_string.is_empty() { display_string.push_str(", "); }
                display_string.push_str(&format!("Entities: {}", value as u32));
            }
        }
        
        if !display_string.is_empty() {
            text.0 = display_string;
        }
    }
}
