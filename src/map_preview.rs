use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_egui::{egui, EguiContexts, EguiTextureHandle};
use egui::load::SizedTexture;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use crate::{GameSettings, GameState};
use crate::voxel::{TerrainSampler, SEA_LEVEL, BlockType};

/// ขนาด texture ที่ render จริง (พิกเซล) — top-down, 1 texel = `zoom` หน่วยโลก
const TEX_SIZE: usize = 256;
const TILE_SIZE: usize = 64;
const MAX_TILE_JOBS: usize = 2;
const MAX_CACHED_TILES: usize = 384;
/// ช่วง zoom (world-units ต่อ texel)
const ZOOM_MIN: f32 = 0.1;
const ZOOM_MAX: f32 = 4096.0;

/// โหมดระบายแผนที่
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapMode {
    Biome,
    Height,
    Continental,
    /// ทิศ+ความเร็วการไหลของแม่น้ำ (hue = ทิศ, สว่าง = เร็ว)
    Flow,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TileKey {
    mode: MapMode,
    zoom_level: i8,
    x: i64,
    z: i64,
}

struct TileResult {
    key: TileKey,
    noise: crate::NoiseParams,
    pixels: Vec<u8>,
}

/// สถานะ + cache ของ live map panel (Dev Mode) — ดูโลกภาพรวมพร้อมปรับ noise/biome
#[derive(Resource)]
pub struct MapPreviewState {
    /// เปิดหน้าต่างอยู่ไหม (ปุ่ม x บนหน้าต่าง egui)
    pub show_map: bool,
    pub mode: MapMode,
    pub zoom: f32,
    pub center_x: f64,
    pub center_z: f64,
    pub needs_update: bool,
    /// กำลังลาก/ซูมเฟรมนี้ไหม — ระหว่างนั้น render หยาบ (low-res) ให้ลื่น
    pub interacting: bool,
    /// สั่ง rebuild sampler รอบหน้า (กดหลังแก้ biome ในหน้าต่าง Game Settings)
    pub force_rebuild: bool,
    /// texture ปลายทาง (สร้าง lazy ครั้งแรก) + egui id ที่ register แล้ว
    pub image: Option<Handle<Image>>,
    pub tex_id: Option<egui::TextureId>,
    /// sampler cache — rebuild เฉพาะตอน noise params เปลี่ยน / force_rebuild
    pub sampler: Option<(crate::NoiseParams, Arc<TerrainSampler>)>,
    tile_cache: HashMap<TileKey, (Arc<Vec<u8>>, u64)>,
    pending_tiles: HashSet<TileKey>,
    tile_results: Arc<Mutex<VecDeque<TileResult>>>,
    active_tile_jobs: Arc<AtomicUsize>,
    cache_clock: u64,
    cache_noise: Option<crate::NoiseParams>,
}

impl Default for MapPreviewState {
    fn default() -> Self {
        Self {
            show_map: false,
            mode: MapMode::Biome,
            zoom: 1.0,
            center_x: 0.0,
            center_z: 0.0,
            needs_update: true,
            interacting: false,
            force_rebuild: false,
            image: None,
            tex_id: None,
            sampler: None,
            tile_cache: HashMap::new(),
            pending_tiles: HashSet::new(),
            tile_results: Arc::new(Mutex::new(VecDeque::new())),
            active_tile_jobs: Arc::new(AtomicUsize::new(0)),
            cache_clock: 0,
            cache_noise: None,
        }
    }
}

fn quantized_zoom(zoom: f32) -> (i8, f64) {
    let level = zoom.max(ZOOM_MIN).log2().round().clamp(-3.0, 12.0) as i8;
    (level, 2.0_f64.powi(level as i32))
}

pub struct MapPreviewPlugin;

impl Plugin for MapPreviewPlugin {
    fn build(&self, app: &mut App) {
        // ต้องอยู่ใน EguiPrimaryContextPass เหมือน UI egui อื่น ๆ (main.rs) —
        // ถ้าวาดหน้าต่างจาก Update มันจะแสดงได้แต่ "คลิกไม่ได้" (ไม่เข้า egui input pass)
        app.init_resource::<MapPreviewState>().add_systems(
            bevy_egui::EguiPrimaryContextPass,
            dev_map_panel_system.run_if(in_state(GameState::InGame)),
        );
    }
}

/// World map toggled with M in every game mode.
fn dev_map_panel_system(
    mut contexts: EguiContexts,
    mut state: ResMut<MapPreviewState>,
    settings: Res<GameSettings>,
    mut images: ResMut<Assets<Image>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    chat: Res<crate::ui::ChatState>,
    typing: Res<crate::EguiTyping>,
    inventory: Res<crate::voxel::InventoryOpen>,
    paused: Res<crate::Paused>,
    player: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    // เมนูอื่นที่ต้องใช้ cursor อยู่ — ตอนปิดแผนที่อย่าเผลอ re-lock ทับ
    let other_ui_wants_cursor = inventory.0 || paused.0 || chat.open;
    // gate เฉพาะ "การอ่านปุ่ม M" — พิมพ์ตัว m ในแชท/ช่อง command ต้องไม่ toggle แผนที่
    if keyboard.just_pressed(KeyCode::KeyM) && !chat.open && !typing.0 {
        state.show_map = !state.show_map;
        if state.show_map {
            if let Ok(transform) = player.single() {
                state.center_x = transform.translation.x as f64;
                state.center_z = transform.translation.z as f64;
            }
            state.needs_update = true;
        }
        if let Ok(mut cursor) = cursor.single_mut() {
            if state.show_map {
                cursor.grab_mode = CursorGrabMode::None;
                cursor.visible = true;
            } else if !other_ui_wants_cursor {
                // re-lock เฉพาะเมื่อไม่มีเมนูอื่นเปิดค้างที่ยังต้องใช้ cursor
                cursor.grab_mode = CursorGrabMode::Locked;
                cursor.visible = false;
            }
        }
    }
    if !state.show_map {
        return;
    }

    // สร้าง texture + register กับ egui ครั้งเดียว
    if state.image.is_none() {
        let size = Extent3d { width: TEX_SIZE as u32, height: TEX_SIZE as u32, depth_or_array_layers: 1 };
        let mut image = Image::new_fill(
            size,
            TextureDimension::D2,
            &[0, 0, 0, 255],
            TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::MAIN_WORLD | bevy::asset::RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor::nearest());
        let handle = images.add(image);
        let tex_id = contexts.add_image(EguiTextureHandle::Strong(handle.clone()));
        state.image = Some(handle);
        state.tex_id = Some(tex_id);
        state.needs_update = true;
    }

    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone();
    let tex_id = state.tex_id.unwrap();

    let was_open = state.show_map;
    let mut show_map = state.show_map;
    let mut changed = false;
    let viewport = ctx.content_rect();

    egui::Window::new("World Map")
        .open(&mut show_map)
        .fixed_pos(viewport.min)
        .fixed_size(viewport.size())
        .resizable(false)
        .collapsible(false)
        .movable(false)
        .show(&ctx, |ui| {
            ui.horizontal(|ui| {
                changed |= ui.radio_value(&mut state.mode, MapMode::Biome, "Biome").changed();
                changed |= ui.radio_value(&mut state.mode, MapMode::Height, "Height").changed();
                changed |= ui.radio_value(&mut state.mode, MapMode::Continental, "Plates").changed();
                changed |= ui.radio_value(&mut state.mode, MapMode::Flow, "Flow").changed();
                ui.separator();
                if ui.button("Refresh").on_hover_text("rebuild after editing biomes").clicked() {
                    state.force_rebuild = true;
                    changed = true;
                }
                if ui.button("Reset").clicked() {
                    state.center_x = 0.0;
                    state.center_z = 0.0;
                    state.zoom = 1.0;
                    changed = true;
                }
            });

            let zoom_changed = ui
                .add(egui::Slider::new(&mut state.zoom, ZOOM_MIN..=ZOOM_MAX).logarithmic(true).text("Zoom"))
                .changed();
            if zoom_changed {
                state.zoom = quantized_zoom(state.zoom).1 as f32;
                changed = true;
            }

            ui.label(
                egui::RichText::new(format!(
                    "Center ({:.0}, {:.0})  ·  drag = pan, scroll = zoom",
                    state.center_x, state.center_z
                ))
                .small()
                .weak(),
            );

            // ภาพแผนที่ — ลาก/ซูมผ่าน response ของ egui เอง (ไม่ต้องมี Camera2d)
            let map_side = ui.available_size().min_elem().max(128.0);
            let img = egui::Image::new(SizedTexture::new(tex_id, egui::vec2(map_side, map_side)))
                .sense(egui::Sense::drag());
            let horizontal_padding = ((ui.available_width() - map_side) * 0.5).max(0.0);
            let resp = ui
                .horizontal(|ui| {
                    ui.add_space(horizontal_padding);
                    ui.add_sized(egui::vec2(map_side, map_side), img)
                })
                .inner;

            // Chunk grid aligned to world coordinates. At distant zoom levels,
            // group chunks so the overlay remains readable instead of becoming
            // a solid field of lines.
            let rect = resp.rect;
            let span = TEX_SIZE as f64 * state.zoom as f64;
            let chunk = crate::voxel::CHUNK_WIDTH as f64;
            let mut stride_chunks = 1i32;
            while chunk * stride_chunks as f64 / span * (rect.width() as f64) < 7.0 {
                stride_chunks *= 2;
            }
            let grid_step = chunk * stride_chunks as f64;
            let min_x = state.center_x - span * 0.5;
            let min_z = state.center_z - span * 0.5;
            let first_x = (min_x / grid_step).floor() * grid_step;
            let first_z = (min_z / grid_step).floor() * grid_step;
            let painter = ui.painter_at(rect);
            let stroke = egui::Stroke::new(
                if stride_chunks == 1 { 1.0 } else { 1.4 },
                egui::Color32::from_rgba_unmultiplied(235, 225, 185, 105),
            );
            let mut gx = first_x;
            while gx <= min_x + span {
                let sx = rect.left() + ((gx - min_x) / span) as f32 * rect.width();
                painter.line_segment(
                    [egui::pos2(sx, rect.top()), egui::pos2(sx, rect.bottom())],
                    stroke,
                );
                gx += grid_step;
            }
            let mut gz = first_z;
            while gz <= min_z + span {
                let sy = rect.top() + ((gz - min_z) / span) as f32 * rect.height();
                painter.line_segment(
                    [egui::pos2(rect.left(), sy), egui::pos2(rect.right(), sy)],
                    stroke,
                );
                gz += grid_step;
            }
            painter.text(
                rect.left_top() + egui::vec2(6.0, 6.0),
                egui::Align2::LEFT_TOP,
                if stride_chunks == 1 {
                    "grid: 1 chunk".to_owned()
                } else {
                    format!("grid: {stride_chunks} chunks")
                },
                egui::FontId::monospace(11.0),
                egui::Color32::from_white_alpha(190),
            );

            let mut active = false;

            if resp.dragged() {
                // Per-frame pointer motion: `Response::drag_delta` is cumulative
                // over the whole gesture and caused unstable/ignored panning in
                // the fullscreen window.
                let d = ui.input(|input| input.pointer.delta());
                if d != egui::Vec2::ZERO {
                    // world หน่วยต่อ 1 egui-point = zoom × (texel/point)
                    let world_per_pt =
                        state.zoom as f64 * TEX_SIZE as f64 / resp.rect.width() as f64;
                    state.center_x -= d.x as f64 * world_per_pt;
                    state.center_z -= d.y as f64 * world_per_pt;
                    changed = true;
                    active = true;
                }
            }

            if resp.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let (level, _) = quantized_zoom(state.zoom);
                    let next_level = if scroll > 0.0 { level - 1 } else { level + 1 };
                    state.zoom = 2.0_f32
                        .powi(next_level.clamp(-3, 12) as i32)
                        .clamp(ZOOM_MIN, ZOOM_MAX);
                    changed = true;
                    active = true;
                }
            }

            // ระหว่างขยับ = low-res; พอหยุด = บังคับ full-res + supersample หนึ่งเฟรม
            if active {
                state.interacting = true;
            } else if state.interacting {
                state.interacting = false;
                changed = true;
            }

            // ── โหมด Flow: วาดลูกศรทิศการไหลของแม่น้ำทับบนภาพ (คมกว่าใส่ลง texture) ──
            if state.mode == MapMode::Flow && !state.interacting && state.zoom <= 16.0 {
                if let Some((_, sampler)) = state.sampler.as_ref() {
                    let rect = resp.rect;
                    let painter = ui.painter_at(rect);
                    let zoom = state.zoom as f64;
                    let (cx, cz) = (state.center_x, state.center_z);
                    let span = TEX_SIZE as f64 * zoom; // ระยะโลก (บล็อก) ที่ภาพครอบ
                    let stepp = 22.0f32; // ระยะห่างลูกศร (points)
                    let mut sy = rect.min.y + stepp * 0.5;
                    while sy < rect.max.y {
                        let mut sx = rect.min.x + stepp * 0.5;
                        while sx < rect.max.x {
                            let fx = (sx - rect.min.x) / rect.width();
                            let fy = (sy - rect.min.y) / rect.height();
                            let wx = cx + (fx as f64 - 0.5) * span;
                            let wz = cz + (fy as f64 - 0.5) * span;
                            if let Some((dir, speed)) = sampler.river_flow(wx, wz) {
                                let len = stepp * (0.55 + 0.45 * speed);
                                let v = egui::vec2(dir.x, dir.y) * len; // +x=ขวา, +y=ลง(=+Z)
                                let origin = egui::pos2(sx - v.x * 0.5, sy - v.y * 0.5);
                                let a = (110.0 + 145.0 * speed) as u8;
                                painter.arrow(
                                    origin,
                                    v,
                                    egui::Stroke::new(1.6, egui::Color32::from_rgba_unmultiplied(235, 245, 255, a)),
                                );
                            }
                            sx += stepp;
                        }
                        sy += stepp;
                    }
                }
            }
        });

    state.show_map = show_map;
    // ปิดด้วยปุ่ม X ของหน้าต่าง — re-lock เฉพาะเมื่อไม่มีเมนูอื่นเปิดค้าง
    if was_open && !state.show_map && !other_ui_wants_cursor {
        if let Ok(mut cursor) = cursor.single_mut() {
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        }
    }
    if changed {
        state.needs_update = true;
    }

    let tile_results_ready = state
        .tile_results
        .lock()
        .is_ok_and(|results| !results.is_empty());
    if state.needs_update
        || state.active_tile_jobs.load(Ordering::Relaxed) > 0
        || tile_results_ready
    {
        state.needs_update = false;
        regenerate(&mut state, &settings, &mut images);
    }
}

/// Compose cached world-space tiles and queue only missing tiles in background.
fn regenerate(state: &mut MapPreviewState, settings: &GameSettings, images: &mut Assets<Image>) {
    let Some(handle) = state.image.clone() else { return };
    if state.force_rebuild || state.cache_noise != Some(settings.noise) {
        state.tile_cache.clear();
        state.pending_tiles.clear();
        if let Ok(mut results) = state.tile_results.lock() {
            results.clear();
        }
        state.cache_noise = Some(settings.noise);
        state.sampler = Some((settings.noise, Arc::new(TerrainSampler::new(settings.noise))));
        state.force_rebuild = false;
    }
    crate::hydro::configure(settings.noise);

    let mut completed = Vec::new();
    if let Ok(mut results) = state.tile_results.lock() {
        completed.extend(results.drain(..));
    }
    for result in completed {
        state.pending_tiles.remove(&result.key);
        if result.noise == settings.noise {
            state.cache_clock = state.cache_clock.wrapping_add(1);
            state.tile_cache.insert(result.key, (Arc::new(result.pixels), state.cache_clock));
        }
    }

    let (zoom_level, zoom) = quantized_zoom(state.zoom);
    state.zoom = zoom as f32;
    let mode = state.mode;
    let start_x = state.center_x - TEX_SIZE as f64 * zoom * 0.5;
    let start_z = state.center_z - TEX_SIZE as f64 * zoom * 0.5;
    let tile_world = TILE_SIZE as f64 * zoom;
    let min_tx = (start_x / tile_world).floor() as i64;
    let min_tz = (start_z / tile_world).floor() as i64;
    let max_tx = ((start_x + TEX_SIZE as f64 * zoom) / tile_world).floor() as i64;
    let max_tz = ((start_z + TEX_SIZE as f64 * zoom) / tile_world).floor() as i64;

    let center_tx = (state.center_x / tile_world).floor() as i64;
    let center_tz = (state.center_z / tile_world).floor() as i64;
    let mut visible_keys = Vec::new();
    for tz in min_tz..=max_tz {
        for tx in min_tx..=max_tx {
            visible_keys.push(TileKey { mode, zoom_level, x: tx, z: tz });
        }
    }
    visible_keys.sort_unstable_by_key(|key| {
        (key.x - center_tx).abs() + (key.z - center_tz).abs()
    });
    // sampler ที่ใช้ซ้ำ (สร้างครั้งเดียวต่อ noise ในบล็อก rebuild ข้างบน) — clone Arc
    // เข้าแต่ละ job แทนการสร้าง TerrainSampler ใหม่ทุก tile
    let sampler_arc = state.sampler.as_ref().map(|(_, s)| s.clone());
    for key in visible_keys {
            if state.tile_cache.contains_key(&key)
                || state.pending_tiles.contains(&key)
                || state.active_tile_jobs.load(Ordering::Relaxed) >= MAX_TILE_JOBS
            {
                continue;
            }
            let Some(sampler) = sampler_arc.clone() else { continue };
            state.pending_tiles.insert(key);
            state.active_tile_jobs.fetch_add(1, Ordering::Relaxed);
            let results = state.tile_results.clone();
            let active = state.active_tile_jobs.clone();
            let noise = settings.noise;
            // ใช้ OS thread แยก **ห้าม** ใช้ AsyncComputeTaskPool — pool นั้น world gen
            // (voxel.rs/lod.rs) ใช้อยู่ ถ้า map job หนักยึด worker หมด chunk gen จะไม่มี
            // worker เหลือ = world หยุด gen. OS thread ให้ OS แบ่งเวลา gen เดินต่อได้
            std::thread::spawn(move || {
                let mut pixels = vec![0u8; TILE_SIZE * TILE_SIZE * 4];
                let origin_x = key.x as f64 * TILE_SIZE as f64 * zoom;
                let origin_z = key.z as f64 * TILE_SIZE as f64 * zoom;
                for y in 0..TILE_SIZE {
                    for x in 0..TILE_SIZE {
                        let color = sample_pixel(
                            &sampler,
                            origin_x + x as f64 * zoom,
                            origin_z + y as f64 * zoom,
                            zoom,
                            key.mode,
                        );
                        let index = (y * TILE_SIZE + x) * 4;
                        pixels[index..index + 4].copy_from_slice(&color);
                    }
                }
                if let Ok(mut queue) = results.lock() {
                    queue.push_back(TileResult { key, noise, pixels });
                }
                active.fetch_sub(1, Ordering::Relaxed);
            });
    }

    let Some(mut image) = images.get_mut(&handle) else { return };
    let Some(data) = image.data.as_mut() else { return };
    for y in 0..TEX_SIZE {
        let wz = start_z + y as f64 * zoom;
        let tz = (wz / tile_world).floor() as i64;
        let local_z = ((wz - tz as f64 * tile_world) / zoom)
            .floor()
            .clamp(0.0, (TILE_SIZE - 1) as f64) as usize;
        for x in 0..TEX_SIZE {
            let wx = start_x + x as f64 * zoom;
            let tx = (wx / tile_world).floor() as i64;
            let local_x = ((wx - tx as f64 * tile_world) / zoom)
                .floor()
                .clamp(0.0, (TILE_SIZE - 1) as f64) as usize;
            let dst = (y * TEX_SIZE + x) * 4;
            let key = TileKey { mode, zoom_level, x: tx, z: tz };
            if let Some((pixels, used)) = state.tile_cache.get_mut(&key) {
                state.cache_clock = state.cache_clock.wrapping_add(1);
                *used = state.cache_clock;
                let src = (local_z * TILE_SIZE + local_x) * 4;
                data[dst..dst + 4].copy_from_slice(&pixels[src..src + 4]);
            } else {
                // While the exact level is loading, reuse a neighboring zoom
                // level at the same world coordinate instead of blanking the map.
                let mut fallback = None;
                for distance in 1..=4 {
                    for fallback_level in [zoom_level - distance, zoom_level + distance] {
                        if !(-3..=12).contains(&fallback_level) {
                            continue;
                        }
                        let fallback_zoom = 2.0_f64.powi(fallback_level as i32);
                        let fallback_world = TILE_SIZE as f64 * fallback_zoom;
                        let fallback_tx = (wx / fallback_world).floor() as i64;
                        let fallback_tz = (wz / fallback_world).floor() as i64;
                        let fallback_key = TileKey {
                            mode,
                            zoom_level: fallback_level,
                            x: fallback_tx,
                            z: fallback_tz,
                        };
                        if let Some((pixels, _)) = state.tile_cache.get(&fallback_key) {
                            let fx = ((wx - fallback_tx as f64 * fallback_world) / fallback_zoom)
                                .floor()
                                .clamp(0.0, (TILE_SIZE - 1) as f64) as usize;
                            let fz = ((wz - fallback_tz as f64 * fallback_world) / fallback_zoom)
                                .floor()
                                .clamp(0.0, (TILE_SIZE - 1) as f64) as usize;
                            let src = (fz * TILE_SIZE + fx) * 4;
                            fallback = Some([pixels[src], pixels[src + 1], pixels[src + 2], 255]);
                            break;
                        }
                    }
                    if fallback.is_some() {
                        break;
                    }
                }
                // ไม่มีทั้ง tile ตรงระดับและ fallback → เขียนสี "กำลังโหลด" เสมอ ห้ามปล่อยว่าง
                // (เดิมปล่อยว่าง = pixel เฟรมก่อนค้าง → pan/zoom ไกลแล้วภาพแตกปนของเก่า)
                let color = fallback.unwrap_or([26, 28, 34, 255]);
                data[dst..dst + 4].copy_from_slice(&color);
            }
        }
    }

    if state.tile_cache.len() > MAX_CACHED_TILES {
        let mut oldest: Vec<_> = state
            .tile_cache
            .iter()
            .map(|(key, (_, age))| (*key, *age))
            .collect();
        oldest.sort_unstable_by_key(|(_, age)| *age);
        let remove_count = state.tile_cache.len() - MAX_CACHED_TILES;
        for (key, _) in oldest.into_iter().take(remove_count) {
            state.tile_cache.remove(&key);
        }
    }
}

/// สีทะเลตามความลึก (ตื้น→ลึก) + อุณหภูมิ: อุ่น=เทอร์คอยส์, เย็น=น้ำเงิน, หนาว=น้ำเงินเข้ม/
/// น้ำแข็ง — ให้ตรงกับ ocean biome ในเกม (เกณฑ์เขต warm>0.05, cool>-0.25 ตาม biome.rs)
#[inline]
fn ocean_color(depth_blocks: i32, temp: f64) -> [u8; 4] {
    // เขตหนาว น้ำตื้น = แผ่นน้ำแข็ง (ให้ตรงกับ sea ice ที่ gen)
    if temp < -0.25 && depth_blocks <= 2 {
        return [205, 224, 236, 255];
    }
    let d = (depth_blocks.clamp(0, 48) as f32) / 48.0;
    let (shallow, deep) = if temp > 0.05 {
        ([90.0, 200.0, 190.0], [10.0, 70.0, 120.0]) // อุ่น: เทอร์คอยส์ → teal
    } else if temp > -0.25 {
        ([70.0, 130.0, 190.0], [20.0, 45.0, 110.0]) // เย็น: ฟ้า → น้ำเงิน
    } else {
        ([50.0, 90.0, 150.0], [8.0, 20.0, 70.0]) // หนาว: น้ำเงินเข้ม
    };
    [
        (shallow[0] + (deep[0] - shallow[0]) * d) as u8,
        (shallow[1] + (deep[1] - shallow[1]) * d) as u8,
        (shallow[2] + (deep[2] - shallow[2]) * d) as u8,
        255,
    ]
}

/// สีของ texel หนึ่งจุด — ยึด terrain จริง (`column` = ความสูง+ผิวน้ำ) ให้ตรงกับโลกที่ gen จริง
/// `world_step` = ระยะโลกต่อ texel (ใช้หาความชันเพื่อ hillshade)
#[inline]
fn sample_pixel(sampler: &TerrainSampler, wx: f64, wz: f64, world_step: f64, mode: MapMode) -> [u8; 4] {
    let sea = SEA_LEVEL as i32;
    let far_preview = world_step >= 16.0;
    let (h, water) = if mode == MapMode::Continental {
        (sea, sea)
    } else if far_preview {
        (sampler.hydro_height(wx, wz).round() as i32, sea)
    } else {
        sampler.column(wx, wz)
    }; // far zoom skips flow-accumulation tiles, which are too expensive per pixel
    let underwater = h < water;

    match mode {
        MapMode::Continental => {
            let c = sampler.continental_sample(wx, wz);
            if c.plate_boundary > 0.35 {
                let glow = (120.0 + c.plate_boundary * 135.0) as u8;
                return [glow, 70, 45, 255];
            }
            if c.landness >= 0.0 {
                let v = (80.0 + c.landness * 150.0) as u8;
                return [55, v, 65, 255];
            }
            let depth = (c.ocean_depth * 150.0) as u8;
            return [20, 85u8.saturating_sub(depth / 3), 170u8.saturating_sub(depth), 255];
        }
        MapMode::Flow => {
            // แม่น้ำ = ฟ้าน้ำเรียบ (ลูกศรทิศวาดทับด้วย egui painter); นอกนั้นจางให้ลูกศรเด่น
            if !far_preview && crate::hydro::river_at(wx, wz).is_some() {
                return [40, 90, 160, 255];
            }
            if underwater {
                return [16, 24, 42, 255]; // ทะเล
            }
            let t = ((h - sea).clamp(0, 140) as f32 / 140.0) * 80.0;
            let g = (35.0 + t) as u8;
            return [g, g, g, 255]; // บก = เทาจาง
        }
        MapMode::Height => {
            if underwater {
                let depth = ((water - h).clamp(0, 40) as f32) / 40.0;
                let v = (0.45 - depth * 0.30) * 255.0;
                return [(v * 0.35) as u8, (v * 0.55) as u8, v as u8, 255];
            }
            let t = ((h - sea) as f32 / 160.0).clamp(0.0, 1.0);
            let c = (60.0 + t * 195.0) as u8;
            return [c, c, c, 255];
        }
        MapMode::Biome => {}
    }

    // ── Biome Colors ──
    if underwater {
        return ocean_color(water - h, sampler.temperature_raw(wx, wz));
    }

    let volcano = sampler.volcano_sample(wx, wz);
    let hydrothermal = sampler.hydrothermal_sample(wx, wz);
    if hydrothermal.pool > 0.05 {
        let c = crate::voxel::block_color(BlockType::SulfuricAcidSource);
        return [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255];
    }
    let col = sampler.column_biome(wx, wz);
    // block ผิวจริง (ชายหาด/หิมะยอดเขา คิดเหมือน gen เป๊ะ)
    let block = if hydrothermal.altered > 0.12 {
        BlockType::AlteredRock
    } else if volcano.crater > 0.45 {
        BlockType::MagmaRock
    } else if volcano.cone > 0.48 {
        BlockType::Basalt
    } else if volcano.cone > 0.0 {
        BlockType::VolcanicAsh
    } else {
        crate::voxel::coastal_surface(sampler, wx, wz, col, h, sea, sampler.snow_line())
    };
    let mut base: [f32; 3] = match block {
        BlockType::Sand => [210.0, 190.0, 120.0],
        BlockType::Snow => [240.0, 240.0, 255.0],
        BlockType::SnowyGrass => [200.0, 220.0, 210.0],
        BlockType::Stone => [120.0, 120.0, 120.0],
        BlockType::Gravel => [125.0, 120.0, 114.0],
        BlockType::Clay => [150.0, 144.0, 138.0],
        BlockType::Dirt => [133.0, 94.0, 66.0],
        BlockType::Basalt => [42.0, 44.0, 46.0],
        BlockType::VolcanicAsh => [72.0, 67.0, 64.0],
        BlockType::MagmaRock => [215.0, 55.0, 8.0],
        BlockType::AlteredRock => [184.0, 176.0, 122.0],
        _ => {
            let temp = sampler.temperature_raw(wx, wz);
            let hum = sampler.humidity_raw(wx, wz);
            let fol = crate::biome::foliage_color(temp, hum);
            [fol[0] * 255.0, fol[1] * 255.0, fol[2] * 255.0]
        }
    };

    // Hillshade จากความชันจริง (เทียบเพื่อนบ้าน 1 texel ทาง +X / +Z) — แสงจากมุมบนซ้าย
    let hx = sampler.height(wx + world_step, wz);
    let hz = sampler.height(wx, wz + world_step);
    let slope = ((h - hx) + (h - hz)) as f32 * 0.06;
    let shade = (1.0 + slope).clamp(0.6, 1.4);
    for c in &mut base {
        *c = (*c * shade).clamp(0.0, 255.0);
    }
    [base[0] as u8, base[1] as u8, base[2] as u8, 255]
}

/// Diagnostic dump — เรนเดอร์แผนที่โลกจริง (ผ่าน sample_pixel ตัวเดียวกับเกม) เป็น PNG
/// ให้เปิดดูวิเคราะห์ความสมจริงนอกเกม. รันด้วย:
///   cargo test dump_world_maps -- --ignored --nocapture
#[cfg(test)]
mod dump {
    use super::*;

    /// โฟลเดอร์ปลายทาง — override ได้ด้วย env `WORLDMAP_DUMP_DIR`, ไม่งั้นลง temp dir
    fn dump_dir() -> std::path::PathBuf {
        std::env::var_os("WORLDMAP_DUMP_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    }

    fn render(
        sampler: &TerrainSampler,
        mode: MapMode,
        cx: f64,
        cz: f64,
        zoom: f64,
        size: usize,
    ) -> Vec<u8> {
        let mut buf = vec![0u8; size * size * 4];
        let start_x = cx - size as f64 * zoom * 0.5;
        let start_z = cz - size as f64 * zoom * 0.5;
        for y in 0..size {
            for x in 0..size {
                let wx = start_x + x as f64 * zoom;
                let wz = start_z + y as f64 * zoom;
                let c = sample_pixel(sampler, wx, wz, zoom, mode);
                let i = (y * size + x) * 4;
                buf[i..i + 4].copy_from_slice(&c);
            }
        }
        buf
    }

    fn save(buf: &[u8], size: usize, name: &str) {
        let path = dump_dir().join(name);
        image::save_buffer(&path, buf, size as u32, size as u32, image::ExtendedColorType::Rgba8)
            .unwrap();
        eprintln!("wrote {}", path.display());
    }

    #[test]
    #[ignore]
    fn dump_world_maps() {
        let noise = crate::NoiseParams::default();
        crate::hydro::configure(noise);
        let sampler = TerrainSampler::new(noise);
        let size = 512;

        // มุมกว้าง ~33 กม. (512 texel × 64 บล็อก) — ดูโครงทวีป/มหาสมุทร/แถบ biome/แผ่นเปลือกโลก
        save(&render(&sampler, MapMode::Biome, 0.0, 0.0, 64.0, size), size, "world_biome_macro.png");
        save(&render(&sampler, MapMode::Height, 0.0, 0.0, 64.0, size), size, "world_height_macro.png");
        save(&render(&sampler, MapMode::Continental, 0.0, 0.0, 64.0, size), size, "world_plates_macro.png");

        // เช็คว่าไม่ fluke — biome macro ของ seed อื่น
        for seed in [2u32, 7, 42] {
            let mut n2 = noise;
            n2.seed = seed;
            crate::hydro::configure(n2);
            let s2 = TerrainSampler::new(n2);
            save(&render(&s2, MapMode::Biome, 0.0, 0.0, 64.0, size), size, &format!("world_biome_seed{seed}.png"));
            save(&render(&s2, MapMode::Height, 0.0, 0.0, 64.0, size), size, &format!("world_height_seed{seed}.png"));
        }
        crate::hydro::configure(noise);

        // หาจุดบนแผ่นดินที่ค่อนข้างราบ (relief ต่ำ) ไว้ทำภาพระยะใกล้ (โชว์ที่ราบ + แม่น้ำ)
        let relief = |x: f64, z: f64| {
            let hs = [
                sampler.height(x, z),
                sampler.height(x + 120.0, z),
                sampler.height(x - 120.0, z),
                sampler.height(x, z + 120.0),
                sampler.height(x, z - 120.0),
            ];
            let (mut lo, mut hi) = (i32::MAX, i32::MIN);
            for h in hs {
                lo = lo.min(h);
                hi = hi.max(h);
            }
            hi - lo
        };
        let mut center = (0.0, 0.0);
        'outer: for r in 1..80 {
            for a in 0..16 {
                let ang = a as f64 / 16.0 * std::f64::consts::TAU;
                let d = r as f64 * 300.0;
                let (x, z) = (d * ang.cos(), d * ang.sin());
                if sampler.continental_sample(x, z).landness > 0.30 && relief(x, z) < 18 {
                    center = (x, z);
                    break 'outer;
                }
            }
        }
        eprintln!("close-up center = {:?}", center);
        // ~3 กม. (512 × 6) — โครงข่ายแม่น้ำ + รายละเอียดผิว
        save(&render(&sampler, MapMode::Biome, center.0, center.1, 6.0, size), size, "world_biome_close.png");
        save(&render(&sampler, MapMode::Flow, center.0, center.1, 6.0, size), size, "world_flow_close.png");
        save(&render(&sampler, MapMode::Height, center.0, center.1, 6.0, size), size, "world_height_close.png");
    }

    // ── ตัวสร้าง texture placeholder สำหรับบล็อกใหม่ (ice/gravel/clay) ──
    // รันครั้งเดียว: cargo test --bin voxel-game gen_block_textures -- --ignored
    // เขียน PNG 16×16 ลง assets/textures/ (ผู้ใช้วาดทับทีหลังได้)
    fn px_hash(x: u32, y: u32, salt: u32) -> u32 {
        let mut v = x
            .wrapping_mul(374_761_393)
            .wrapping_add(y.wrapping_mul(668_265_263))
            .wrapping_add(salt.wrapping_mul(2_246_822_519));
        v ^= v >> 13;
        v = v.wrapping_mul(1_274_126_177);
        v ^ (v >> 16)
    }
    fn unit(x: u32, y: u32, salt: u32) -> f32 {
        (px_hash(x, y, salt) & 0xffff) as f32 / 65535.0
    }

    fn save_texture(name: &str, salt: u32, f: impl Fn(u32, u32) -> [u8; 3]) {
        const N: u32 = 16;
        let mut buf = vec![0u8; (N * N * 4) as usize];
        for y in 0..N {
            for x in 0..N {
                let c = f(x, y);
                let i = ((y * N + x) * 4) as usize;
                buf[i] = c[0];
                buf[i + 1] = c[1];
                buf[i + 2] = c[2];
                buf[i + 3] = 255;
            }
        }
        let _ = salt;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/textures")
            .join(name);
        image::save_buffer(&path, &buf, N, N, image::ExtendedColorType::Rgba8).unwrap();
        eprintln!("wrote {}", path.display());
    }

    #[test]
    #[ignore]
    fn gen_block_textures() {
        // Ice: ฟ้าอมเขียวอ่อน + noise เบา + รอยแตกจางแนวทแยง
        save_texture("ice.png", 11, |x, y| {
            let n = unit(x, y, 11) * 16.0 - 8.0;
            let crack = if (x + y) % 6 == 0 || (x + 3 * y) % 11 == 0 { -22.0 } else { 0.0 };
            let base = [176.0, 212.0, 232.0];
            [
                (base[0] + n + crack).clamp(0.0, 255.0) as u8,
                (base[1] + n + crack).clamp(0.0, 255.0) as u8,
                (base[2] + n + crack * 0.5).clamp(0.0, 255.0) as u8,
            ]
        });
        // Gravel: กรวดเทาคละก้อน (สุ่มความสว่างต่อ pixel) + จุดเข้มเป็นร่อง
        save_texture("gravel.png", 22, |x, y| {
            let u = unit(x, y, 22);
            let g = 92.0 + u * 78.0; // 92..170 คละก้อน
            let grout = if unit(x, y, 99) < 0.12 { -34.0 } else { 0.0 };
            [
                (g + grout).clamp(0.0, 255.0) as u8,
                (g * 0.96 + grout).clamp(0.0, 255.0) as u8,
                (g * 0.9 + grout).clamp(0.0, 255.0) as u8,
            ]
        });
        // Clay: เทา-น้ำตาลเนียน + แถบแนวนอนจาง + noise เบา
        save_texture("clay.png", 33, |x, y| {
            let n = unit(x, y, 33) * 10.0 - 5.0;
            let band = if y % 4 == 0 { -8.0 } else { 0.0 };
            let base = [152.0, 145.0, 138.0];
            [
                (base[0] + n + band).clamp(0.0, 255.0) as u8,
                (base[1] + n + band).clamp(0.0, 255.0) as u8,
                (base[2] + n + band).clamp(0.0, 255.0) as u8,
            ]
        });
    }
}
