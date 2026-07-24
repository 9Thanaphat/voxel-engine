use bevy::prelude::*;

mod camera;
mod voxel;
mod ui;
mod network;
mod particles;
mod dem;
mod electricity;
mod lod;
mod item;
mod world_save;
mod command;
pub mod light;
pub mod tree;
mod sky;
mod audio;
mod weather;

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// โหมดเล่น: Creative = palette วาง/ขุดไม่จำกัด, Survival = นับจำนวนจริง เก็บ/หัก stack
/// (เป็นค่า client-local — inventory ไม่ sync ข้าม network)
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum GameMode {
    Creative,
    Survival,
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NoiseParams {
    pub frequency: f64,
    pub amplitude: f64,
    pub octaves: u32,
    /// seed ของ world gen — คุมทั้งความสูง biome และถ้ำ (ดู `TerrainSampler::new`)
    pub seed: u32,
}

#[derive(Resource)]
pub struct GameSettings {
    pub render_distance: i32,
    pub render_mode: RenderMode,
    pub terrain_source: TerrainSource,
    /// โหมดเล่น Creative/Survival (ดู [`GameMode`]) — เปลี่ยนแล้ว rebuild Hotbar
    pub game_mode: GameMode,
    pub noise: NoiseParams,
    /// เวลาในเกม หน่วยชั่วโมง 0-24 (6 = พระอาทิตย์ขึ้น, 12 = เที่ยง, 18 = ตก)
    pub time_of_day: f32,
    /// ตัวคูณความเร็วรอบวัน-คืน (1.0 = ปกติ ~20 นาที/วัน, 0 = หยุดเวลานิ่ง) — ปรับด้วย /daynight
    pub day_speed: f32,
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
    /// ระยะ LOD หน่วย "chunk" (เท่ากับ render distance) — × CHUNK_WIDTH = บล็อก/เมตร
    pub lod_distance_chunks: i32,
    /// เข้าเกมผ่านเมนู Dev Mode — เปิดหน้าต่าง Game Settings เต็ม (สไลเดอร์ noise,
    /// Regenerate, wireframe ฯลฯ) ส่วนโลกปกติเห็นแค่ Options พื้นฐานตอนกด ESC
    pub dev_mode: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            render_distance: 8,
            render_mode: RenderMode::Full,
            terrain_source: TerrainSource::Noise,
            game_mode: GameMode::Creative,
            noise: NoiseParams {
                frequency: 0.015,
                amplitude: 40.0,
                octaves: 4,
                seed: 1,
            },
            time_of_day: 10.0,
            day_speed: 1.0,
            fluid_tick_seconds: 0.1,
            tnt_power: 10.0,
            tnt_fuse_seconds: 2.0,
            show_tnt_rays: false,
            nuke_yield: 500.0,
            nuke_fuse_seconds: 5.0,
            lod_enabled: true,
            lod_distance_chunks: 2048, // ×16 ≈ 33 กม.
            dev_mode: false,
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

/// egui กำลังรับ text input อยู่ไหม (ช่องแชท, IP, GPS lat/lon)
/// ตั้งทุกเฟรมโดย [`ui::track_egui_typing`]
#[derive(Resource, Default)]
pub struct EguiTyping(pub bool);

/// คีย์บอร์ดว่างให้ gameplay ใช้ไหม — กันตัวอักษรที่พิมพ์ในช่องข้อความ
/// ทะลุไปเปลี่ยนช่อง hotbar / ขยับกล้อง
fn keyboard_free(chat: Res<ui::ChatState>, typing: Res<EguiTyping>) -> bool {
    !chat.open && !typing.0
}

fn reset_paused(
    mut paused: ResMut<Paused>,
    mut show_options: ResMut<ui::ShowOptions>,
    mut inventory: ResMut<voxel::InventoryOpen>,
    mut open_container: ResMut<voxel::OpenContainer>,
) {
    paused.0 = false;
    show_options.0 = false;
    inventory.0 = false;
    open_container.0 = None;
}

/// หน้าต่างช่องเก็บของปิดอยู่ — คลิก/เลข 1-9/scroll เป็นของโลก ไม่ใช่ของหน้าต่าง
fn inventory_closed(open: Res<voxel::InventoryOpen>) -> bool {
    !open.0
}

/// `--realworld`: เข้าโลก DEM ตั้งแต่เริ่ม (ใช้คู่ --host ไว้เทสอัตโนมัติ)
fn apply_cli_world_flags(mut settings: ResMut<GameSettings>) {
    if std::env::args().any(|a| a == "--realworld") {
        settings.terrain_source = TerrainSource::RealWorld;
        settings.dev_mode = true;
        voxel::set_legacy_save_dir(true);
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq, Debug, Hash, States)]
pub enum GameState {
    #[default]
    MainMenu,
    /// รายการ world ที่เคยสร้าง + ปุ่มสร้างใหม่
    SinglePlayerMenu,
    /// ฟอร์มตั้งชื่อ/seed/mode ของ world ใหม่
    CreateWorldMenu,
    /// ทางเข้าเดิม (Quick Start noise / Real World) สำหรับจูนค่าและเทส
    DevMenu,
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

fn setup_cluster_settings(mut settings: ResMut<bevy_light::cluster::GlobalClusterSettings>) {
    // ปิด GPU clustering ทั้งระบบ ให้ bevy fallback เป็น CPU clustering —
    // เส้นทาง GPU readback ของมัน (prepare_clusters_for_gpu_clustering) คือจุดที่
    // DeviceLost บน RX 570 + driver 22.20 ตอนเปิดเกมสองหน้าต่าง (host+client)
    // ยืนยันจาก smoke test 2026-07-21: ตายซ้ำได้ทุกครั้งภายในไม่กี่วิหลัง join
    settings.gpu_clustering = None;
}

/// panic hook: เขียน crash_log.txt ให้ชัดว่าเกิดอะไร (ส่วนใหญ่คือ GPU DeviceLost)
/// แล้วเรียก hook เดิมต่อ (ยังได้ backtrace บน stderr) — เกมจะไม่เด้งแบบเงียบอีก
fn install_crash_handler() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_string());
        let loc = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let report = format!(
            "\n===== CRASH (unix {secs}) =====\n{msg}\nat {loc}\n\
             อาจเป็น GPU DeviceLost (การ์ดจอหมด VRAM) — ถ้าเกิดตอน render distance สูง\n\
             ให้ลด Render Distance ใน Settings (LOD เห็นไกลได้โดยไม่ต้องโหลด chunk เยอะ)\n",
        );
        use std::io::Write;
        let path = crate::voxel::project_root().join("crash_log.txt");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(report.as_bytes());
        }
        default(info);
    }));
}

fn main() {
    install_crash_handler();
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
    // โหมด water mask จาก OSM: `voxel-game --build-water <lat0> <lat1> <lon0> <lon1>`
    if let Some(i) = args.iter().position(|a| a == "--build-water") {
        let lat0 = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let lat1 = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(0);
        let lon0 = args.get(i + 3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let lon1 = args.get(i + 4).and_then(|s| s.parse().ok()).unwrap_or(0);
        dem::build_water_cli(lat0, lat1, lon0, lon1);
        return;
    }

    App::new()
        .init_resource::<GameSettings>()
        .init_resource::<RegenerateWorld>()
        .init_resource::<voxel::TargetedBlock>()
        .init_resource::<voxel::SelectedBlock>()
        .init_resource::<voxel::Hotbar>()
        .init_resource::<voxel::InventoryOpen>()
        .init_resource::<voxel::OpenContainer>()
        .init_resource::<voxel::ItemIconCache>()
        .init_resource::<voxel::IconBakeState>()
        .init_resource::<ui::HeldStack>()
        .init_resource::<voxel::InteractionMode>()
        .init_resource::<voxel::ActiveFluids>()
        .init_resource::<voxel::PendingBlockUpdates>()
        .init_resource::<voxel::ActivePools>()
        .init_resource::<voxel::ActiveTnt>()
        .init_resource::<voxel::ExplosionDebug>()
        .init_resource::<particles::ActiveShockwaves>()
        .init_resource::<voxel::NukeJobs>()
        .init_resource::<voxel::NukeApplication>()
        .init_resource::<ui::ScreenFlash>()
        .init_resource::<ui::TeleportUi>()
        .init_resource::<ui::ShowDebugMenu>()
        .init_resource::<ui::HudHidden>()
        .init_resource::<ui::ShowOptions>()
        .init_resource::<ui::ChatState>()
        .init_resource::<command::CommandQueue>()
        .init_resource::<EguiTyping>()
        .init_resource::<ui::WorldList>()
        .init_resource::<ui::CreateWorldUi>()
        .init_resource::<voxel::ActivePools>()
        .init_resource::<voxel::BreakingProgress>()
        .init_resource::<Paused>()
        .init_resource::<network::MultiplayerUi>()
        .init_resource::<network::PendingNetEdits>()
        .init_resource::<network::PendingLocalActions>()
        .init_resource::<network::PendingNetFx>()
        .init_resource::<network::IncomingNetEdits>()
        .init_resource::<network::IncomingChunkRemesh>()
        .init_resource::<network::PositionSendTimer>()
        .init_resource::<tree::BranchNetwork>()
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
            item::ItemPlugin,
            sky::SkyPlugin,
            audio::AudioPlugin,
            weather::WeatherPlugin,
        ))
        .init_state::<GameState>()
        .add_message::<particles::BlockFx>()
        .add_message::<particles::ExplosionFx>()
        .add_systems(Startup, (
            setup_cluster_settings,
            voxel::setup_voxel,
            voxel::setup_campfire_assets,
            voxel::setup_break_overlay,
            network::setup_player_model_assets,
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
            particles::attach_campfire_flames,
        ))
        .add_systems(
            Update,
            (
                // input ผู้เล่นหยุดตอน pause แต่ HUD text อัปเดตต่อ
                (
                    camera::cursor_grab_system,
                    voxel::hotbar_input_system,
                ).run_if(unpaused).run_if(keyboard_free).run_if(inventory_closed),
                // ไม่ gate: แรงโน้มถ่วงต้องเดินต่อตอน pause/พิมพ์แชท ไม่งั้นค้างกลางอากาศ
                // (ระบบเช็ค Paused/ChatState/EguiTyping เองเพื่อหยุดแค่การอ่านปุ่ม)
                camera::camera_movement_system,
                // mouse-look ปิดตัวเองอยู่แล้วเมื่อ cursor ไม่ถูก grab (camera.rs)
                camera::camera_look_system.run_if(unpaused),
                ui::chat_open_system.run_if(keyboard_free),
                // E ตอนพิมพ์แชทต้องลงช่องข้อความ ไม่ใช่เปิดช่องเก็บของ
                ui::inventory_toggle_system.run_if(keyboard_free),
                ui::inventory_click_system,
                ui::container_click_system,
                // ต้องหลัง cursor_grab_system: ESC ในระบบนั้นปลดล็อคเมาส์ทุกครั้ง
                // ปิด inventory ด้วย ESC แล้วต้องได้เมาส์ล็อคกลับ ไม่ใช่ค้างเป็นลูกศร
                ui::inventory_close_system.after(camera::cursor_grab_system),
                ui::inventory_hover_name_system,
                ui::update_inventory_ui.after(voxel::start_icon_bake),
                ui::update_container_ui.after(voxel::start_icon_bake),
                ui::update_held_icon.after(voxel::start_icon_bake),
                ui::age_chat_lines,
                ui::update_chat_ui,
                ui::update_underwater_overlay,
                command::run_commands,
                ui::update_hotbar_ui.after(voxel::start_icon_bake),
                ui::bake_palette_icons.after(voxel::start_icon_bake),
                // รวมเป็น tuple ย่อย — tuple ระบบของ bevy จำกัด 20 ตัวต่อชั้น
                (
                    ui::update_coordinate_ui_system,
                    ui::update_fps_text,
                    ui::update_block_target_text,
                    ui::update_mode_text,
                ),
            ).run_if(in_state(GameState::InGame)),
        )
        .add_systems(
            OnEnter(GameState::InGame),
            (reset_paused, voxel::position_player_for_terrain, world_save::load_game_system.after(voxel::position_player_for_terrain), ui::show_controls_hint),
        )
        // ออกจากโลก: เซฟที่ค้าง + ล้างโลกทิ้ง ไม่งั้นค้างเป็นฉากหลังเมนูหลัก
        .add_systems(
            OnExit(GameState::InGame),
            (world_save::save_on_exit_system, voxel::unload_world_on_exit, lod::clear_lod_on_exit, voxel::clear_breaking_on_exit),
        )
        .add_systems(
            Update,
            (
                voxel::voxel_raycast_system,
                voxel::block_interaction_system.run_if(unpaused),
                voxel::update_break_overlay.after(voxel::block_interaction_system),
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
                lod::hide_near_overlay,
                dem::dem_stream_system,
                voxel::start_icon_bake,
                voxel::finish_icon_bake,
                voxel::propagate_render_layers,
                world_save::auto_save_system,
                // ทูเพิลของ add_systems รับได้สูงสุด 20 ตัว — ที่เกินจัดเป็นกลุ่มซ้อน
                (
                    voxel::block_update_system,
                    // แสงต้องคำนวณก่อน chunk ถูก mesh ครั้งแรก ไม่งั้นจะ mesh ตอนยังมืด
                    voxel::relight_system.before(voxel::world_generation_system),
                    voxel::branch_remesh_system.after(voxel::block_update_system),
                    voxel::advance_time_system.before(voxel::update_sun_system),
                ),
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
                network::host_broadcast_fx,
                network::host_broadcast_actions,
                network::host_broadcast_positions,
            ).run_if(resource_exists::<bevy_renet::RenetServer>),
        )
        .add_systems(
            Update,
            (
                network::client_receive_messages,
                network::client_send_edits,
                network::client_send_actions,
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
            network::player_rig_setup_system,
            network::player_animation_system,
            network::update_remote_held_items.after(network::player_rig_setup_system),
        ))
        .add_systems(Startup, apply_cli_world_flags.before(network::autostart_from_args))
        .add_systems(Startup, network::autostart_from_args)
        .add_systems(
            bevy_egui::EguiPrimaryContextPass,
            (
                ui::setup_egui_theme,
                ui::track_egui_typing,
                ui::egui_settings_system.run_if(in_state(GameState::InGame)),
                ui::options_menu_system.run_if(in_state(GameState::InGame)),
                // chat ต้องรันก่อน pause: ESC ตอนแชทเปิด = ปิดแชท ไม่ใช่เปิด pause
                ui::chat_input_system
                    .run_if(in_state(GameState::InGame))
                    .before(ui::pause_menu_system),
                ui::pause_menu_system.run_if(in_state(GameState::InGame)),
                ui::main_menu_system.run_if(in_state(GameState::MainMenu)),
                ui::singleplayer_menu_system.run_if(in_state(GameState::SinglePlayerMenu)),
                ui::create_world_menu_system.run_if(in_state(GameState::CreateWorldMenu)),
                ui::dev_menu_system.run_if(in_state(GameState::DevMenu)),
                ui::multiplayer_menu_system.run_if(in_state(GameState::MultiplayerMenu)),
            ),
        )
        .add_systems(OnEnter(GameState::SinglePlayerMenu), ui::refresh_world_list)
        .add_systems(
            Update,
            (
                ui::toggle_ingame_ui,
                ui::handle_f3_system,
                ui::handle_f1_system,
                ui::handle_f2_screenshot,
                ui::hotbar_item_name_system,
                ui::quit_after_save,
            ),
        )
        .run();
}
