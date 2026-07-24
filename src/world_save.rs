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
    pub terrain_source: crate::TerrainSource,
    pub noise: crate::NoiseParams,
    pub render_distance: i32,
}

impl WorldGenPreset {
    pub fn from_settings(s: &crate::GameSettings) -> Self {
        Self {
            render_mode: s.render_mode,
            terrain_source: s.terrain_source,
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
}

pub fn save_player_and_electricity(
    grid: &crate::electricity::ElectricalGrid,
    transform: &bevy::prelude::Transform,
    camera: &crate::camera::FreeCamera,
    hotbar: &crate::voxel::Hotbar,
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
        };
        if let Ok(json) = serde_json::to_string_pretty(&player_data) {
            let _ = std::fs::write(dir.join("player.json"), json);
        }
}

pub fn auto_save_system(
    time: bevy::prelude::Res<bevy::prelude::Time>,
    mut timer: bevy::prelude::Local<f32>,
    grid: bevy::prelude::Res<crate::electricity::ElectricalGrid>,
    camera_q: bevy::prelude::Query<(&bevy::prelude::Transform, &crate::camera::FreeCamera)>,
    hotbar: bevy::prelude::Res<crate::voxel::Hotbar>,
    mut chat: bevy::prelude::ResMut<crate::ui::ChatState>,
) {
    *timer += time.delta_secs();
    if *timer >= 45.0 { // เซฟบ่อยขึ้น กัน crash เสียความคืบหน้ามาก
        *timer = 0.0;
        if let Ok((transform, camera)) = camera_q.single() {
            save_player_and_electricity(&grid, transform, camera, &hotbar);
            chat.push_system("Auto-saved game.");
        }
    }
}

pub fn save_on_exit_system(
    grid: bevy::prelude::Res<crate::electricity::ElectricalGrid>,
    camera_q: bevy::prelude::Query<(&bevy::prelude::Transform, &crate::camera::FreeCamera)>,
    hotbar: bevy::prelude::Res<crate::voxel::Hotbar>,
) {
    if let Ok((transform, camera)) = camera_q.single() {
        save_player_and_electricity(&grid, transform, camera, &hotbar);
    }
}

pub fn load_game_system(
    mut grid: bevy::prelude::ResMut<crate::electricity::ElectricalGrid>,
    mut camera_q: bevy::prelude::Query<(&mut bevy::prelude::Transform, &mut crate::camera::FreeCamera)>,
    mut hotbar: bevy::prelude::ResMut<crate::voxel::Hotbar>,
    mut topo_writer: bevy::prelude::MessageWriter<crate::electricity::PowerTopologyChanged>,
    mut world: bevy::prelude::ResMut<crate::voxel::VoxelWorld>,
) {
    let dir = crate::voxel::active_save_dir();
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
            }
        }
        // โครงกิ่งย้ายไปเก็บต่อ chunk (chunk_x_z.tree.bin) แล้ว — อ่าน JSON ก้อนเก่า
        // ครั้งเดียวเพื่อไม่ให้กิ่งที่ผู้เล่นเคยวางในโลกเดิมเสียโครงไป ข้อมูลจะย้ายเข้า
        // ไฟล์ต่อ chunk เองตอน chunk นั้นถูกเซฟครั้งถัดไป และไม่มีการเขียน JSON กลับอีก
        world.branch_network = crate::tree::BranchNetwork::load(&dir);
}
