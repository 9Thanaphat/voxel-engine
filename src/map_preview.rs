use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_egui::{egui, EguiContexts, EguiTextureHandle};
use egui::load::SizedTexture;
use crate::{GameSettings, GameState};
use crate::voxel::{TerrainSampler, SEA_LEVEL, BlockType};

/// ขนาด texture ที่ render จริง (พิกเซล) — top-down, 1 texel = `zoom` หน่วยโลก
const TEX_SIZE: usize = 256;
/// ขนาดภาพบนหน้าต่าง egui (points)
const DISPLAY_SIZE: f32 = 340.0;
/// ช่วง zoom (world-units ต่อ texel)
const ZOOM_MIN: f32 = 0.1;
const ZOOM_MAX: f32 = 1024.0;

/// โหมดระบายแผนที่
#[derive(Clone, Copy, PartialEq)]
pub enum MapMode {
    Biome,
    Height,
    /// ทิศ+ความเร็วการไหลของแม่น้ำ (hue = ทิศ, สว่าง = เร็ว)
    Flow,
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
    pub sampler: Option<(crate::NoiseParams, TerrainSampler)>,
}

impl Default for MapPreviewState {
    fn default() -> Self {
        Self {
            show_map: true,
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
        }
    }
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

/// หน้าต่าง egui "World Map (Dev)" — โผล่เฉพาะตอน dev_mode, ลอยข้าง Game Settings
fn dev_map_panel_system(
    mut contexts: EguiContexts,
    mut state: ResMut<MapPreviewState>,
    settings: Res<GameSettings>,
    mut images: ResMut<Assets<Image>>,
) {
    if !settings.dev_mode {
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

    let mut show_map = state.show_map;
    let mut changed = false;

    egui::Window::new("World Map (Dev)")
        .open(&mut show_map)
        .default_size([DISPLAY_SIZE + 20.0, DISPLAY_SIZE + 110.0])
        .resizable(false)
        .show(&ctx, |ui| {
            ui.horizontal(|ui| {
                changed |= ui.radio_value(&mut state.mode, MapMode::Biome, "Biome").changed();
                changed |= ui.radio_value(&mut state.mode, MapMode::Height, "Height").changed();
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

            changed |= ui
                .add(egui::Slider::new(&mut state.zoom, ZOOM_MIN..=ZOOM_MAX).logarithmic(true).text("Zoom"))
                .changed();

            ui.label(
                egui::RichText::new(format!(
                    "Center ({:.0}, {:.0})  ·  drag = pan, scroll = zoom",
                    state.center_x, state.center_z
                ))
                .small()
                .weak(),
            );

            // ภาพแผนที่ — ลาก/ซูมผ่าน response ของ egui เอง (ไม่ต้องมี Camera2d)
            let img = egui::Image::new(SizedTexture::new(tex_id, egui::vec2(DISPLAY_SIZE, DISPLAY_SIZE)))
                .sense(egui::Sense::drag());
            let resp = ui.add(img);

            let mut active = false;

            if resp.dragged() {
                let d = resp.drag_delta();
                if d != egui::Vec2::ZERO {
                    // world หน่วยต่อ 1 egui-point = zoom × (texel/point)
                    let world_per_pt = state.zoom as f64 * TEX_SIZE as f64 / DISPLAY_SIZE as f64;
                    state.center_x -= d.x as f64 * world_per_pt;
                    state.center_z -= d.y as f64 * world_per_pt;
                    changed = true;
                    active = true;
                }
            }

            if resp.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    if scroll > 0.0 {
                        state.zoom /= 1.1;
                    } else {
                        state.zoom *= 1.1;
                    }
                    state.zoom = state.zoom.clamp(ZOOM_MIN, ZOOM_MAX);
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
            if state.mode == MapMode::Flow && !state.interacting {
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
    if changed {
        state.needs_update = true;
    }

    if state.needs_update {
        state.needs_update = false;
        regenerate(&mut state, &settings, &mut images);
    }
}

/// เขียน texture ใหม่จาก sampler จริง — multithread ตามแถว, supersample ตอน zoom out
fn regenerate(state: &mut MapPreviewState, settings: &GameSettings, images: &mut Assets<Image>) {
    let Some(handle) = state.image.clone() else { return };

    // rebuild sampler เฉพาะเมื่อ params เปลี่ยน หรือถูกสั่ง (แก้ biome)
    if state.force_rebuild || state.sampler.as_ref().map_or(true, |(p, _)| *p != settings.noise) {
        state.sampler = Some((settings.noise, TerrainSampler::new(settings.noise)));
        state.force_rebuild = false;
    }
    // ให้โครงข่ายแม่น้ำใช้ seed เดียวกับ preview (ล้าง cache ถ้า seed เปลี่ยน)
    crate::hydro::configure(settings.noise);

    let width = TEX_SIZE;
    let height = TEX_SIZE;
    let zoom = state.zoom as f64;
    let mode = state.mode;
    let start_x = state.center_x - (width as f64 * zoom) / 2.0;
    let start_z = state.center_z - (height as f64 * zoom) / 2.0;
    // ระหว่างลาก render หยาบ (ทุก 4 texel); หยุดแล้วเต็ม + supersample แก้ aliasing ตอน zoom out
    let step = if state.interacting { 4 } else { 1 };
    let ss = if step == 1 { (zoom.round() as usize).clamp(1, 3) } else { 1 };

    let sampler = &state.sampler.as_ref().unwrap().1;

    let Some(mut image) = images.get_mut(&handle) else { return };
    let Some(data) = image.data.as_mut() else { return };

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, height.max(1));
    let rows_per = height.div_ceil(n_threads);
    let row_bytes = width * 4;

    std::thread::scope(|scope| {
        for (chunk_idx, chunk) in data.chunks_mut(row_bytes * rows_per).enumerate() {
            let y0 = chunk_idx * rows_per;
            scope.spawn(move || {
                let rows = chunk.len() / row_bytes;
                for row in 0..rows {
                    let y = y0 + row;
                    let wz = start_z + (y as f64 * zoom);
                    let mut x = 0;
                    while x < width {
                        let wx = start_x + (x as f64 * zoom);
                        let color = sample_texel(sampler, wx, wz, zoom, ss, mode);
                        let end = (x + step).min(width);
                        for xx in x..end {
                            let idx = (row * width + xx) * 4;
                            chunk[idx..idx + 4].copy_from_slice(&color);
                        }
                        x += step;
                    }
                }
            });
        }
    });
}

/// เฉลี่ย `ss×ss` จุดย่อยต่อ texel — ลด aliasing ตอน zoom out (1 texel ครอบพื้นที่โลกกว้าง)
#[inline]
fn sample_texel(sampler: &TerrainSampler, wx: f64, wz: f64, zoom: f64, ss: usize, mode: MapMode) -> [u8; 4] {
    if ss <= 1 {
        return sample_pixel(sampler, wx, wz, zoom, mode);
    }
    let inv = zoom / ss as f64;
    let mut acc = [0u32; 4];
    for sy in 0..ss {
        for sx in 0..ss {
            let px = sample_pixel(sampler, wx + sx as f64 * inv, wz + sy as f64 * inv, inv, mode);
            for i in 0..4 {
                acc[i] += px[i] as u32;
            }
        }
    }
    let n = (ss * ss) as u32;
    [(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8, (acc[3] / n) as u8]
}

/// สีน้ำ (ตื้น→ลึก) ตามระยะใต้ผิวน้ำ
#[inline]
fn water_color(depth_blocks: i32) -> [u8; 4] {
    let d = (depth_blocks.clamp(0, 48) as f32) / 48.0;
    let shallow = [70.0, 130.0, 190.0];
    let deep = [20.0, 45.0, 110.0];
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
    let (h, water) = sampler.column(wx, wz); // water = ผิวน้ำ (sea หรือแม่น้ำยกสูง)
    let underwater = h < water;

    match mode {
        MapMode::Flow => {
            // แม่น้ำ = ฟ้าน้ำเรียบ (ลูกศรทิศวาดทับด้วย egui painter); นอกนั้นจางให้ลูกศรเด่น
            if crate::hydro::river_at(wx, wz).is_some() {
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
        return water_color(water - h);
    }

    let col = sampler.column_biome(wx, wz);
    // block ผิวจริง (ชายหาด/หิมะยอดเขา คิดเหมือน gen เป๊ะ)
    let block = crate::voxel::surface_block_for(col, h, sea, sampler.snow_line());
    let mut base: [f32; 3] = match block {
        BlockType::Sand => [210.0, 190.0, 120.0],
        BlockType::Snow => [240.0, 240.0, 255.0],
        BlockType::SnowyGrass => [200.0, 220.0, 210.0],
        BlockType::Stone => [120.0, 120.0, 120.0],
        BlockType::Dirt => [133.0, 94.0, 66.0],
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
