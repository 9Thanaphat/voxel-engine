//! World ที่ผู้เล่นสร้างเอง: โฟลเดอร์ละโลกใต้ `saves/` พร้อมไฟล์ metadata
//!
//! โครงสร้าง: `saves/<slug>/world.json` + `saves/<slug>/chunk_<x>_<z>.bin`
//! (chunk เขียนโดย `voxel::save_chunk` ผ่าน `voxel::active_save_dir`)
//!
//! โลกของ dev mode ยังใช้ `saves/` กับ `saves_dem/` ตรงๆ แบบเดิม — ไฟล์ chunk
//! ที่ root ของ `saves/` จึงไม่ถูกแตะ และ `list_worlds` ก็มองข้ามเพราะไม่มี world.json

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorldMeta {
    pub name: String,
    pub seed: u32,
    /// true = Survival, false = Creative (เก็บเป็น bool ให้ไฟล์เก่าอ่านง่าย)
    pub survival: bool,
    pub created_unix: u64,
}

impl WorldMeta {
    pub fn mode(&self) -> crate::GameMode {
        if self.survival {
            crate::GameMode::Survival
        } else {
            crate::GameMode::Creative
        }
    }
}

const META_FILE: &str = "world.json";

pub fn worlds_root() -> PathBuf {
    crate::voxel::project_root().join("saves")
}

// ============================================================================
// World-gen preset — เซฟค่า world gen (noise/terrain/render) เป็นไฟล์ json ไว้เรียกใช้ซ้ำ
// (คนละเรื่องกับ "โลก" ที่เซฟ chunk — อันนี้แค่ค่า generate) เก็บใน `worldgen_presets/`
// ============================================================================

#[derive(Serialize, Deserialize, Clone)]
pub struct WorldGenPreset {
    pub render_mode: crate::RenderMode,
    pub noise: crate::NoiseParams,
    pub render_distance: i32,
}

impl WorldGenPreset {
    pub fn from_settings(s: &crate::GameSettings) -> Self {
        Self {
            render_mode: s.render_mode,
            noise: s.noise,
            render_distance: s.render_distance,
        }
    }
}

pub fn worldgen_presets_root() -> PathBuf {
    crate::voxel::project_root().join("worldgen_presets")
}

pub fn list_worldgen_presets() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(worldgen_presets_root()) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

pub fn save_worldgen_preset(name: &str, preset: &WorldGenPreset) -> std::io::Result<()> {
    let root = worldgen_presets_root();
    std::fs::create_dir_all(&root)?;
    let json = serde_json::to_string_pretty(preset)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(root.join(format!("{}.json", slugify(name))), json)
}

pub fn load_worldgen_preset(name: &str) -> Option<WorldGenPreset> {
    let json = std::fs::read_to_string(worldgen_presets_root().join(format!("{}.json", slugify(name)))).ok()?;
    serde_json::from_str(&json).ok()
}

pub fn delete_worldgen_preset(name: &str) -> std::io::Result<()> {
    std::fs::remove_file(worldgen_presets_root().join(format!("{}.json", slugify(name))))
}

// ---- default preset: โลกใหม่ (Create World) ดึงค่า gen จากตัวนี้ ----
// เก็บแค่ "ชื่อ" preset ที่ตั้งเป็น default ในไฟล์ marker เดียว (single source of truth)
// ไม่มีนามสกุล .json → list_worldgen_presets มองข้าม ไม่โผล่เป็น preset

fn default_marker_path() -> PathBuf {
    worldgen_presets_root().join(".default")
}

/// ตั้ง preset ชื่อนี้เป็น default ที่โลกใหม่จะดึงค่า gen ไปใช้
pub fn set_default_worldgen(name: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(worldgen_presets_root())?;
    std::fs::write(default_marker_path(), name)
}

/// ชื่อ preset ที่เป็น default ตอนนี้ (ไว้ให้ UI โชว์ว่าอันไหน default) — None ถ้ายังไม่ตั้ง
pub fn default_worldgen_name() -> Option<String> {
    let s = std::fs::read_to_string(default_marker_path()).ok()?;
    let s = s.trim();
    if s.is_empty() { None } else { Some(s.to_string()) }
}

/// ค่า gen ของ default preset — คืน None ถ้ายังไม่ตั้ง/preset ถูกลบไปแล้ว
pub fn load_default_worldgen() -> Option<WorldGenPreset> {
    default_worldgen_name().and_then(|n| load_worldgen_preset(&n))
}

/// ชื่อโฟลเดอร์จากชื่อโลก — กันอักขระที่ใช้ใน path ไม่ได้ (ชื่อไทยกลายเป็น `_` หมด
/// จึง fallback เป็น "world" แล้วให้ตัวเลขท้ายกันชนแทน ชื่อจริงอยู่ใน world.json)
fn slugify(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() { "world".to_string() } else { s }
}

/// อ่านทุกโลกใน `saves/` เรียงใหม่สุดก่อน — โฟลเดอร์ที่ไม่มี/พัง world.json ข้ามไป
pub fn list_worlds() -> Vec<(PathBuf, WorldMeta)> {
    let Ok(entries) = std::fs::read_dir(worlds_root()) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, WorldMeta)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|dir| {
            let bytes = std::fs::read(dir.join(META_FILE)).ok()?;
            let meta: WorldMeta = serde_json::from_slice(&bytes).ok()?;
            Some((dir, meta))
        })
        .collect();
    out.sort_by(|a, b| b.1.created_unix.cmp(&a.1.created_unix));
    out
}

pub fn create_world(name: &str, seed: u32, survival: bool) -> std::io::Result<(PathBuf, WorldMeta)> {
    let root = worlds_root();
    std::fs::create_dir_all(&root)?;

    // กันชนชื่อโฟลเดอร์: ต่อ -2, -3, ... จนกว่าจะว่าง
    let base = slugify(name);
    let mut dir = root.join(&base);
    let mut n = 2;
    while dir.exists() {
        dir = root.join(format!("{base}-{n}"));
        n += 1;
    }
    std::fs::create_dir_all(&dir)?;

    let meta = WorldMeta {
        name: name.to_string(),
        seed,
        survival,
        created_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };
    let json = serde_json::to_vec_pretty(&meta)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(dir.join(META_FILE), json)?;
    Ok((dir, meta))
}

/// ลบทั้งโฟลเดอร์โลก — ปฏิเสธ path ที่ไม่ได้อยู่ใต้ saves/ หรือไม่มี world.json
/// (กันลบ `saves/` ทั้งก้อนพร้อม chunk ของ dev world)
pub fn delete_world(dir: &Path) -> std::io::Result<()> {
    if !dir.join(META_FILE).is_file() || dir.parent() != Some(worlds_root().as_path()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a valid world folder",
        ));
    }
    std::fs::remove_dir_all(dir)
}

/// seed จากข้อความที่ผู้ใช้พิมพ์ — ว่าง = สุ่ม, ตัวเลข = ใช้ตรงๆ, อื่นๆ = hash
pub fn parse_seed(input: &str) -> u32 {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return fastrand::u32(..);
    }
    if let Ok(n) = trimmed.parse::<u32>() {
        return n;
    }
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    trimmed.hash(&mut hasher);
    hasher.finish() as u32
}

fn def_time_of_day() -> f32 { 10.0 }
fn def_day_of_year() -> u16 { 172 }

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PlayerSaveData {
    pub position: [f32; 3],
    pub pitch: f32,
    pub yaw: f32,
    pub fly: bool,
    pub velocity_y: f32,
    pub third_person: bool,
    pub hotbar_selected: usize,
    pub hotbar_items: Vec<Option<crate::item::WireItemStack>>,
    // เวลา/ปฏิทินต่อโลก — โลกกลับมาที่เวลา/ฤดูเดิม (serde default เผื่อ save เก่าไม่มีฟิลด์)
    #[serde(default = "def_time_of_day")]
    pub time_of_day: f32,
    #[serde(default = "def_day_of_year")]
    pub day_of_year: u16,
    #[serde(default)]
    pub year: u32,
}

pub fn save_player_and_electricity(
    grid: &crate::electricity::ElectricalGrid,
    transform: &bevy::prelude::Transform,
    camera: &crate::camera::FreeCamera,
    hotbar: &crate::voxel::Hotbar,
    settings: &crate::GameSettings,
    world: &crate::voxel::VoxelWorld,
) {
    let dir = crate::voxel::active_save_dir();
        if let Ok(bytes) = bincode::serialize(grid) {
            let _ = std::fs::write(dir.join("electricity.bin"), bytes);
        }
        let items: Vec<_> = hotbar.slots.iter().map(|s| s.map(crate::item::WireItemStack::from_stack)).collect();
        let player_data = PlayerSaveData {
            position: transform.translation.into(),
            pitch: camera.pitch,
            yaw: camera.yaw,
            fly: camera.fly,
            velocity_y: camera.velocity_y,
            third_person: camera.third_person,
            hotbar_selected: hotbar.selected,
            hotbar_items: items,
            time_of_day: settings.time_of_day,
            day_of_year: settings.day_of_year,
            year: settings.year,
        };
        if let Ok(json) = serde_json::to_string_pretty(&player_data) {
            let _ = std::fs::write(dir.join("player.json"), json);
        }
        if let Ok(bytes) = bincode::serialize(&(
            world.crucibles.clone(),
            world.ingot_molds.clone(),
            world.placed_ingots.clone(),
        )) {
            let _ = std::fs::write(dir.join("metallurgy.bin"), bytes);
        }
}

// ============================================================================
// UserPrefs — ค่าตั้งค่ารวม (ต่อเครื่อง ไม่ผูกโลก) เซฟเป็น settings.json ที่ root โปรเจกต์
// โหลดตอนเปิดเกม, เซฟตอน auto-save/ออกเกม → ปรับใน Options แล้วจำข้ามรอบเล่น
// ============================================================================

#[derive(Serialize, Deserialize, Clone, bevy::prelude::Resource)]
pub struct UserPrefs {
    pub render_distance: i32,
    pub render_mode: crate::RenderMode,
    pub lod_enabled: bool,
    pub lod_distance_chunks: i32,
    pub fluid_tick_seconds: f32,
    pub day_speed: f32,
    pub latitude_deg: f32,
    pub fov_deg: f32,
    pub fly_speed: f32,
}

impl Default for UserPrefs {
    fn default() -> Self {
        let s = crate::GameSettings::default();
        Self {
            render_distance: s.render_distance,
            render_mode: s.render_mode,
            lod_enabled: s.lod_enabled,
            lod_distance_chunks: s.lod_distance_chunks,
            fluid_tick_seconds: s.fluid_tick_seconds,
            day_speed: s.day_speed,
            latitude_deg: s.latitude_deg,
            fov_deg: 70.0,   // ตรงกับ default FOV (camera.rs)
            fly_speed: 50.0, // ตรงกับ FreeCamera::default().speed
        }
    }
}

impl UserPrefs {
    /// เขียนค่าที่จำได้ลง GameSettings (fov/fly ไปที่กล้อง ดู [`apply_camera_prefs_system`])
    pub fn apply_to_settings(&self, s: &mut crate::GameSettings) {
        s.render_distance = self.render_distance;
        s.render_mode = self.render_mode;
        s.lod_enabled = self.lod_enabled;
        s.lod_distance_chunks = self.lod_distance_chunks;
        s.fluid_tick_seconds = self.fluid_tick_seconds;
        s.day_speed = self.day_speed;
        s.latitude_deg = self.latitude_deg;
    }
}

fn user_prefs_path() -> PathBuf {
    crate::voxel::project_root().join("settings.json")
}

// ---- BiomeConfig (data-driven biomes) — global, เซฟ biomes.json, sync ให้ client ----

fn biome_config_path() -> PathBuf {
    crate::voxel::project_root().join("biomes.json")
}

/// โหลดชุด biome จาก biomes.json (คืน default ถ้าไม่มี/พัง) — เรียกตอน main() ก่อน insert resource
pub fn load_biome_config() -> crate::biomegen::BiomeConfig {
    std::fs::read_to_string(biome_config_path())
        .ok()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

/// เซฟชุด biome ปัจจุบันลง biomes.json (เรียกตอนแก้ใน dev UI)
pub fn save_biome_config(cfg: &crate::biomegen::BiomeConfig) {
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(biome_config_path(), json);
    }
}

/// โหลด settings.json (คืน default ถ้าไม่มี/พัง) — เรียกตอน main() ก่อน insert GameSettings
pub fn load_user_prefs() -> UserPrefs {
    std::fs::read_to_string(user_prefs_path())
        .ok()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

/// เซฟค่า global ปัจจุบันลง settings.json (fov/fly มาจากกล้อง)
pub fn save_user_prefs(settings: &crate::GameSettings, fov_deg: f32, fly_speed: f32) {
    let prefs = UserPrefs {
        render_distance: settings.render_distance,
        render_mode: settings.render_mode,
        lod_enabled: settings.lod_enabled,
        lod_distance_chunks: settings.lod_distance_chunks,
        fluid_tick_seconds: settings.fluid_tick_seconds,
        day_speed: settings.day_speed,
        latitude_deg: settings.latitude_deg,
        fov_deg,
        fly_speed,
    };
    if let Ok(json) = serde_json::to_string_pretty(&prefs) {
        let _ = std::fs::write(user_prefs_path(), json);
    }
}

/// ใช้ค่า fov/fly ที่จำไว้กับกล้องครั้งแรกที่พร้อม (รันจนกว่าจะ apply สำเร็จ)
pub fn apply_camera_prefs_system(
    prefs: bevy::prelude::Res<UserPrefs>,
    mut done: bevy::prelude::Local<bool>,
    mut cam_q: bevy::prelude::Query<&mut crate::camera::FreeCamera>,
    mut proj_q: bevy::prelude::Query<&mut bevy::prelude::Projection, bevy::prelude::With<crate::camera::MainCamera>>,
) {
    if *done {
        return;
    }
    let mut applied = false;
    if let Ok(mut cam) = cam_q.single_mut() {
        cam.speed = prefs.fly_speed;
        applied = true;
    }
    if let Ok(mut proj) = proj_q.single_mut() {
        if let bevy::prelude::Projection::Perspective(p) = &mut *proj {
            p.fov = prefs.fov_deg.to_radians();
            applied = true;
        }
    }
    if applied {
        *done = true;
    }
}

/// ดึง FOV (องศา) + fly speed ปัจจุบันจากกล้อง เพื่อประกอบ save (ใช้ใน auto-save/on-exit)
fn camera_fov_speed(
    cam_q: &bevy::prelude::Query<(&bevy::prelude::Transform, &crate::camera::FreeCamera)>,
    proj_q: &bevy::prelude::Query<&bevy::prelude::Projection, bevy::prelude::With<crate::camera::MainCamera>>,
) -> (f32, f32) {
    let fly = cam_q.single().map(|(_, c)| c.speed).unwrap_or(50.0);
    let fov = proj_q
        .single()
        .ok()
        .and_then(|p| match p {
            bevy::prelude::Projection::Perspective(pp) => Some(pp.fov.to_degrees()),
            _ => None,
        })
        .unwrap_or(70.0);
    (fov, fly)
}

pub fn auto_save_system(
    time: bevy::prelude::Res<bevy::prelude::Time>,
    mut timer: bevy::prelude::Local<f32>,
    grid: bevy::prelude::Res<crate::electricity::ElectricalGrid>,
    camera_q: bevy::prelude::Query<(&bevy::prelude::Transform, &crate::camera::FreeCamera)>,
    proj_q: bevy::prelude::Query<&bevy::prelude::Projection, bevy::prelude::With<crate::camera::MainCamera>>,
    hotbar: bevy::prelude::Res<crate::voxel::Hotbar>,
    settings: bevy::prelude::Res<crate::GameSettings>,
    world: bevy::prelude::Res<crate::voxel::VoxelWorld>,
    mut chat: bevy::prelude::ResMut<crate::ui::ChatState>,
) {
    *timer += time.delta_secs();
    if *timer >= 45.0 { // เซฟบ่อยขึ้น กัน crash เสียความคืบหน้ามาก
        *timer = 0.0;
        if let Ok((transform, camera)) = camera_q.single() {
            save_player_and_electricity(&grid, transform, camera, &hotbar, &settings, &world);
            let (fov, fly) = camera_fov_speed(&camera_q, &proj_q);
            save_user_prefs(&settings, fov, fly);
            chat.push_system("Auto-saved game.");
        }
    }
}

pub fn save_on_exit_system(
    grid: bevy::prelude::Res<crate::electricity::ElectricalGrid>,
    camera_q: bevy::prelude::Query<(&bevy::prelude::Transform, &crate::camera::FreeCamera)>,
    proj_q: bevy::prelude::Query<&bevy::prelude::Projection, bevy::prelude::With<crate::camera::MainCamera>>,
    hotbar: bevy::prelude::Res<crate::voxel::Hotbar>,
    settings: bevy::prelude::Res<crate::GameSettings>,
    world: bevy::prelude::Res<crate::voxel::VoxelWorld>,
) {
    if let Ok((transform, camera)) = camera_q.single() {
        save_player_and_electricity(&grid, transform, camera, &hotbar, &settings, &world);
        let (fov, fly) = camera_fov_speed(&camera_q, &proj_q);
        save_user_prefs(&settings, fov, fly);
    }
}

pub fn load_game_system(
    mut grid: bevy::prelude::ResMut<crate::electricity::ElectricalGrid>,
    mut camera_q: bevy::prelude::Query<(&mut bevy::prelude::Transform, &mut crate::camera::FreeCamera)>,
    mut hotbar: bevy::prelude::ResMut<crate::voxel::Hotbar>,
    mut topo_writer: bevy::prelude::MessageWriter<crate::electricity::PowerTopologyChanged>,
    mut world: bevy::prelude::ResMut<crate::voxel::VoxelWorld>,
    mut settings: bevy::prelude::ResMut<crate::GameSettings>,
    net_client: Option<bevy::prelude::Res<bevy_renet::RenetClient>>,
) {
    let dir = crate::voxel::active_save_dir();
        if let Ok(bytes) = std::fs::read(dir.join("metallurgy.bin")) {
            type MetallurgySave = (
                std::collections::HashMap<bevy::prelude::IVec3, crate::chemistry::CrucibleData>,
                std::collections::HashMap<bevy::prelude::IVec3, crate::chemistry::IngotMoldData>,
                std::collections::HashMap<bevy::prelude::IVec3, crate::chemistry::CastIngotData>,
            );
            if let Ok((crucibles, molds, ingots)) = bincode::deserialize::<MetallurgySave>(&bytes) {
                world.crucibles = crucibles;
                world.ingot_molds = molds;
                world.placed_ingots = ingots;
            }
        }
        if let Ok(bytes) = std::fs::read(dir.join("electricity.bin")) {
            if let Ok(loaded_grid) = bincode::deserialize(&bytes) {
                *grid = loaded_grid;
                topo_writer.write(crate::electricity::PowerTopologyChanged);
            }
        }
        if let Ok(json) = std::fs::read_to_string(dir.join("player.json")) {
            if let Ok(data) = serde_json::from_str::<PlayerSaveData>(&json) {
                if let Ok((mut transform, mut camera)) = camera_q.single_mut() {
                    transform.translation = bevy::prelude::Vec3::from(data.position);
                    camera.pitch = data.pitch;
                    camera.yaw = data.yaw;
                    camera.fly = data.fly;
                    camera.velocity_y = data.velocity_y;
                    camera.third_person = data.third_person;
                    
                    use bevy::prelude::*;
                    transform.rotation = Quat::from_axis_angle(Vec3::Y, camera.yaw) * Quat::from_axis_angle(Vec3::X, camera.pitch);
                }
                
                hotbar.selected = data.hotbar_selected;
                for (i, wire_item) in data.hotbar_items.into_iter().enumerate() {
                    if i < hotbar.slots.len() {
                        hotbar.slots[i] = wire_item.and_then(|w| w.to_stack());
                    }
                }
                // เวลา/ปฏิทินต่อโลก — client รับจาก host (Welcome) จึงไม่เขียนทับ
                if net_client.is_none() {
                    settings.time_of_day = data.time_of_day;
                    settings.day_of_year = data.day_of_year;
                    settings.year = data.year;
                }
            }
        }
        // โครงกิ่งย้ายไปเก็บต่อ chunk (chunk_x_z.tree.bin) แล้ว — อ่าน JSON ก้อนเก่า
        // ครั้งเดียวเพื่อไม่ให้กิ่งที่ผู้เล่นเคยวางในโลกเดิมเสียโครงไป ข้อมูลจะย้ายเข้า
        // ไฟล์ต่อ chunk เองตอน chunk นั้นถูกเซฟครั้งถัดไป และไม่มีการเขียน JSON กลับอีก
        world.branch_network = crate::tree::BranchNetwork::load(&dir);
}
