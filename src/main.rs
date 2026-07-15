use bevy::prelude::*;

mod camera;
mod voxel;
mod ui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// สร้าง block volume เต็ม chunk + AO — โหมดเล่นจริง
    Full,
    /// สร้างเฉพาะ mesh ผิวโลกจาก noise ตรงๆ — ไว้ preview ตอนจูนค่า world gen
    SurfacePreview,
}

#[derive(Clone, Copy, PartialEq)]
pub struct NoiseParams {
    pub frequency: f64,
    pub amplitude: f64,
    pub octaves: u32,
}

#[derive(Resource)]
pub struct GameSettings {
    pub render_distance: i32,
    pub render_mode: RenderMode,
    pub noise: NoiseParams,
    /// เวลาในเกม หน่วยชั่วโมง 0-24 (6 = พระอาทิตย์ขึ้น, 12 = เที่ยง, 18 = ตก)
    pub time_of_day: f32,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            render_distance: 8,
            render_mode: RenderMode::Full,
            noise: NoiseParams {
                frequency: 0.015,
                amplitude: 40.0,
                octaves: 4,
            },
            time_of_day: 10.0,
        }
    }
}

/// ตั้งเป็น true เพื่อล้างโลกแล้ว generate ใหม่ (ตอนเปลี่ยนโหมด/ค่า noise)
#[derive(Resource, Default)]
pub struct RegenerateWorld(pub bool);

fn main() {
    App::new()
        .init_resource::<GameSettings>()
        .init_resource::<RegenerateWorld>()
        .init_resource::<voxel::TargetedBlock>()
        .init_resource::<voxel::SelectedBlock>()
        .init_resource::<voxel::InteractionMode>()
        .add_plugins((
            // sampler แบบ nearest (พิกเซลคม) + repeat (จำเป็น: texture ต้องปูซ้ำ
            // ข้าม quad ที่ greedy meshing รวมแล้ว UV เกิน 1.0)
            DefaultPlugins.set(ImagePlugin {
                default_sampler: bevy::image::ImageSamplerDescriptor {
                    address_mode_u: bevy::image::ImageAddressMode::Repeat,
                    address_mode_v: bevy::image::ImageAddressMode::Repeat,
                    ..bevy::image::ImageSamplerDescriptor::nearest()
                },
            }),
            bevy::pbr::wireframe::WireframePlugin::default(),
            bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
            bevy::diagnostic::EntityCountDiagnosticsPlugin::default(),
            bevy_egui::EguiPlugin::default(),
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
                ui::update_block_target_text,
                ui::update_mode_text,
            ),
        )
        .add_systems(
            Update,
            (
                voxel::voxel_raycast_system,
                voxel::block_interaction_system,
                voxel::world_reset_system,
                voxel::world_generation_system,
                voxel::process_generated_chunks_system,
                voxel::chunk_unloading_system,
                voxel::update_sun_system,
            ),
        )
        .add_systems(
            bevy_egui::EguiPrimaryContextPass,
            ui::egui_settings_system,
        )
        .run();
}
