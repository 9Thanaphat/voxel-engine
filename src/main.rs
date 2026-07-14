use bevy::prelude::*;

mod camera;
mod voxel;
mod ui;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
            bevy::diagnostic::EntityCountDiagnosticsPlugin::default(),
        ))
        .add_systems(Startup, (
            voxel::setup_voxel,
            camera::setup_camera,
            ui::setup_ui
        ))
        .add_systems(
            Update,
            (
                camera::camera_movement_system,
                camera::camera_look_system,
                camera::cursor_grab_system,
                ui::update_coordinate_ui_system,
                ui::update_fps_text,
                voxel::voxel_raycast_system,
                voxel::world_generation_system,
                voxel::process_generated_chunks_system,
                voxel::chunk_unloading_system,
            ),
        )
        .run();
}
