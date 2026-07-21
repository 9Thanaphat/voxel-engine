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
