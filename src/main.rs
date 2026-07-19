use bevy::prelude::*;

mod camera;
mod voxel;
mod ui;
mod network;
mod particles;
mod dem;
mod electricity;
mod lod;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// สร้าง block volume เต็ม chunk + AO — โหมดเล่นจริง
    Full,
    /// สร้างเฉพาะ mesh ผิวโลกจาก noise ตรงๆ — ไว้ preview ตอนจูนค่า world gen
    SurfacePreview,
}

/// แหล่งภูมิประเทศ: noise เดิม หรือโลกจริงจาก DEM (1 บล็อก = 1 ม.)
/// — serialize ได้เพราะต้อง sync ให้ client ตอน join (client generate chunk เอง)
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum TerrainSource {
    Noise,
    RealWorld,
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NoiseParams {
    pub frequency: f64,
    pub amplitude: f64,
    pub octaves: u32,
}

#[derive(Resource)]
pub struct GameSettings {
    pub render_distance: i32,
    pub render_mode: RenderMode,
    pub terrain_source: TerrainSource,
    pub noise: NoiseParams,
    /// เวลาในเกม หน่วยชั่วโมง 0-24 (6 = พระอาทิตย์ขึ้น, 12 = เที่ยง, 18 = ตก)
    pub time_of_day: f32,
    /// คาบ tick ของ fluid sim (วินาที) — น้อย = น้ำไหลเร็ว, มาก = ช้า/เบาเครื่อง
    pub fluid_tick_seconds: f32,
    /// พลังงานต่อ ray ของระเบิด TNT (มีผลฝั่ง host/single — ผล broadcast เป็น edit)
    pub tnt_power: f32,
    /// เวลานับถอยหลังหลังจุดชนวน TNT (วินาที)
    pub tnt_fuse_seconds: f32,
    /// debug: วาดเส้น ray ของระเบิดค้างไว้ให้ดู
    pub show_tnt_rays: bool,
    /// ขนาด nuke หน่วย "บล็อก TNT เทียบเท่า" — รัศมี ∝ yield^⅓ ตามสูตรจริง
    pub nuke_yield: f32,
    pub nuke_fuse_seconds: f32,
    /// ภูมิประเทศระยะไกล (LOD แบบ Distant Horizons)
    pub lod_enabled: bool,
    pub lod_distance_m: f32,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            render_distance: 8,
            render_mode: RenderMode::Full,
            terrain_source: TerrainSource::Noise,
            noise: NoiseParams {
                frequency: 0.015,
                amplitude: 40.0,
                octaves: 4,
            },
            time_of_day: 10.0,
            fluid_tick_seconds: 0.1,
            tnt_power: 10.0,
            tnt_fuse_seconds: 2.0,
            show_tnt_rays: false,
            nuke_yield: 500.0,
            nuke_fuse_seconds: 5.0,
            lod_enabled: true,
            lod_distance_m: 33_000.0,
        }
    }
}

/// ตั้งเป็น true เพื่อล้างโลกแล้ว generate ใหม่ (ตอนเปลี่ยนโหมด/ค่า noise)
#[derive(Resource, Default)]
pub struct RegenerateWorld(pub bool);

/// เปิด pause menu อยู่ไหม (ESC ในเกม) — โลกยังเดินต่อ แค่ล็อค input ผู้เล่น
#[derive(Resource, Default)]
pub struct Paused(pub bool);

fn unpaused(paused: Res<Paused>) -> bool {
    !paused.0
}

fn reset_paused(mut paused: ResMut<Paused>) {
    paused.0 = false;
}

/// `--realworld`: เข้าโลก DEM ตั้งแต่เริ่ม (ใช้คู่ --host ไว้เทสอัตโนมัติ)
fn apply_cli_world_flags(mut settings: ResMut<GameSettings>) {
    if std::env::args().any(|a| a == "--realworld") {
        settings.terrain_source = TerrainSource::RealWorld;
        voxel::DEM_SAVE_DIR.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq, Debug, Hash, States)]
pub enum GameState {
    #[default]
    MainMenu,
    MultiplayerMenu,
    InGame,
}

/// หาโฟลเดอร์ assets ให้เจอไม่ว่าจะรันแบบไหน:
/// - dev (cargo run หรือรัน exe ตรงๆ ในเครื่องนี้): assets ที่ root โปรเจกต์
/// - แจกจ่าย (ก๊อป exe + assets ไปเครื่องอื่น): assets ข้างๆ ตัว exe
/// ถ้าไม่ตั้งเอง bevy จะหาข้างๆ exe เท่านั้นตอนรันตรงๆ — texture หายหมด
fn asset_root() -> String {
    let dev = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    if dev.exists() {
        return dev.to_string_lossy().into_owned();
    }
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        let beside_exe = exe_dir.join("assets");
        if beside_exe.exists() {
            return beside_exe.to_string_lossy().into_owned();
        }
    }
    "assets".to_string()
}

fn main() {
    // โหมดโหลด DEM: `voxel-game --build-dem <lat0> <lat1> <lon0> <lon1>` แล้วจบ
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--build-dem") {
        let lat0 = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let lat1 = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(0);
        let lon0 = args.get(i + 3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let lon1 = args.get(i + 4).and_then(|s| s.parse().ok()).unwrap_or(0);
        dem::build_dem_cli(lat0, lat1, lon0, lon1);
        return;
    }

    App::new()
        .init_resource::<GameSettings>()
        .init_resource::<RegenerateWorld>()
        .init_resource::<voxel::TargetedBlock>()
        .init_resource::<voxel::SelectedBlock>()
        .init_resource::<voxel::Hotbar>()
        .init_resource::<voxel::BlockPickerOpen>()
        .init_resource::<voxel::InteractionMode>()
        .init_resource::<voxel::ActiveFluids>()
        .init_resource::<voxel::ActiveTnt>()
        .init_resource::<voxel::ExplosionDebug>()
        .init_resource::<particles::ActiveShockwaves>()
        .init_resource::<voxel::NukeJobs>()
        .init_resource::<voxel::NukeApplication>()
        .init_resource::<ui::ScreenFlash>()
        .init_resource::<ui::TeleportUi>()
        .init_resource::<ui::ShowDebugMenu>()
        .init_resource::<voxel::ActivePools>()
        .init_resource::<Paused>()
        .init_resource::<network::MultiplayerUi>()
        .init_resource::<network::PendingNetEdits>()
        .init_resource::<network::IncomingNetEdits>()
        .init_resource::<network::IncomingChunkRemesh>()
        .init_resource::<network::PositionSendTimer>()
        .add_plugins((
            DefaultPlugins.set(ImagePlugin {
                default_sampler: bevy::image::ImageSamplerDescriptor {
                    address_mode_u: bevy::image::ImageAddressMode::Repeat,
                    address_mode_v: bevy::image::ImageAddressMode::Repeat,
                    ..bevy::image::ImageSamplerDescriptor::nearest()
                },
            }).set(bevy::asset::AssetPlugin {
                file_path: asset_root(),
                ..Default::default()
            }),
            bevy::pbr::wireframe::WireframePlugin::default(),
            bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
            bevy::diagnostic::EntityCountDiagnosticsPlugin::default(),
            bevy_egui::EguiPlugin::default(),
            electricity::ElectricityPlugin,
            // renet: plugins ทำงานเฉพาะตอนมี RenetServer/RenetClient resource
            bevy_renet::RenetServerPlugin,
            bevy_renet::RenetClientPlugin,
            bevy_renet::netcode::NetcodeServerPlugin,
            bevy_renet::netcode::NetcodeClientPlugin,
            bevy_hanabi::HanabiPlugin,
        ))
        .init_state::<GameState>()
        .add_message::<particles::BlockFx>()
        .add_message::<particles::ExplosionFx>()
        .add_systems(Startup, (
            voxel::setup_voxel,
            camera::setup_camera,
            ui::setup_ui,
            particles::setup_particles,
            lod::setup_lod,
        ))
        .add_systems(Update, (
            particles::spawn_block_fx,
            particles::spawn_explosion_fx,
            particles::update_shockwaves.after(particles::spawn_explosion_fx),
            particles::update_explosion_flash,
            particles::update_plasma_dome,
            particles::update_wilson_cloud,
            particles::trigger_screen_flash,
            ui::update_screen_flash.after(particles::trigger_screen_flash),
            particles::despawn_finished_fx,
            particles::attach_lamp_sparkles,
        ))
        .add_systems(
            Update,
            (
                // input ผู้เล่นหยุดตอน pause แต่ HUD text อัปเดตต่อ
                (
                    camera::camera_movement_system,
                    camera::camera_look_system,
                    camera::cursor_grab_system,
                    voxel::hotbar_input_system,
                ).run_if(unpaused),
                ui::update_hotbar_ui,
                ui::update_coordinate_ui_system,
                ui::update_fps_text,
                ui::update_block_target_text,
                ui::update_mode_text,
            ).run_if(in_state(GameState::InGame)),
        )
        .add_systems(
            OnEnter(GameState::InGame),
            (reset_paused, voxel::position_player_for_terrain),
        )
        .add_systems(
            Update,
            (
                voxel::voxel_raycast_system,
                voxel::block_interaction_system.run_if(unpaused),
                voxel::world_reset_system,
                voxel::world_generation_system,
                voxel::process_generated_chunks_system,
                voxel::chunk_unloading_system,
                voxel::update_sun_system,
                // น้ำ simulate เฉพาะ single player กับ host — client รับ delta จาก host แทน
                voxel::fluid_simulation_system.run_if(network::is_not_client),
                // TNT fuse/ระเบิดก็เป็นของ host/single เช่นกัน (ผล broadcast เป็น edit)
                voxel::tnt_detonation_system.run_if(network::is_not_client),
                voxel::nuke_apply_system.run_if(network::is_not_client),
                voxel::explosion_debug_system,
                lod::update_lod_tiles,
                dem::dem_stream_system,
            ).run_if(in_state(GameState::InGame)),
        )
        // ---- Networking ----
        .add_observer(network::on_server_event)
        .add_systems(
            Update,
            (
                network::host_receive_client_messages,
                network::host_send_queued_chunks,
                network::host_broadcast_edits.after(network::host_receive_client_messages),
                network::host_broadcast_positions,
            ).run_if(resource_exists::<bevy_renet::RenetServer>),
        )
        .add_systems(
            Update,
            (
                network::client_receive_messages,
                network::client_send_edits,
                network::client_send_position,
                network::client_connection_watchdog,
            ).run_if(resource_exists::<bevy_renet::RenetClient>),
        )
        .add_systems(
            Update,
            (
                network::apply_incoming_net_edits
                    .after(network::client_receive_messages)
                    .after(network::host_receive_client_messages)
                    .before(voxel::fluid_simulation_system),
                network::interpolate_remote_players,
            ).run_if(network::is_networked),
        )
        .add_systems(Update, (
            network::cleanup_remote_players,
            network::stop_host_system,
            network::auto_host_system,
            network::nameplate_system,
        ))
        .add_systems(Startup, apply_cli_world_flags.before(network::autostart_from_args))
        .add_systems(Startup, network::autostart_from_args)
        .add_systems(
            bevy_egui::EguiPrimaryContextPass,
            (
                ui::egui_settings_system.run_if(in_state(GameState::InGame)),
                // pause ต้องรันก่อน picker: ESC ตอน picker เปิด = ปิด picker ไม่ใช่เปิด pause
                ui::pause_menu_system
                    .run_if(in_state(GameState::InGame))
                    .before(ui::block_picker_system),
                ui::block_picker_system.run_if(in_state(GameState::InGame)),
                ui::main_menu_system.run_if(in_state(GameState::MainMenu)),
                ui::multiplayer_menu_system.run_if(in_state(GameState::MultiplayerMenu)),
            ),
        )
        .add_systems(
            Update,
            (ui::toggle_ingame_ui, ui::handle_f3_system),
        )
        .run();
}
