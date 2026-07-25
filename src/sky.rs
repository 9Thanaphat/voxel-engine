//! ท้องฟ้า procedural: skydome (ทรงกลมยักษ์ตามกล้อง) + shader ไล่สี + ดวงอาทิตย์ + ดาว
//!
//! เป็น custom `Material` ตัวแรกของโปรเจค (ที่เหลือเป็น StandardMaterial unlit)
//! ผูกกับระบบเวลาเดิม (`GameSettings::time_of_day` / `voxel::sun_tint`) ไม่สร้างเวลาใหม่
//! ค่าปรับแต่งทั้งหมดอยู่ใน [`SkySettings`] (ปรับ live ผ่าน dev menu + เซฟเป็น preset ได้)

use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use std::path::PathBuf;

/// รัศมี skydome — ต้องมากกว่าระยะภูเขา DEM ไกลสุด (~35 กม.) และน้อยกว่า far ของกล้อง (50 กม.)
/// depth (reversed-Z, depth_write ปิด) จะดันทรงกลมไปอยู่หลังทุกอย่างเสมอ
const SKY_RADIUS: f32 = 45_000.0;

// ============================================================================
// SkySettings — ค่าปรับแต่งท้องฟ้าทั้งหมด (ปรับ live + เซฟ preset เป็นไฟล์ได้)
// Default = หน้าตาที่จูนไว้ปัจจุบัน
// ============================================================================

#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkySettings {
    // สี gradient (linear rgb, เกิน 1 ได้เพื่อ Bloom)
    pub day_top: [f32; 3],
    pub day_horizon: [f32; 3],
    pub day_bottom: [f32; 3],
    pub night_top: [f32; 3],
    pub night_horizon: [f32; 3],
    pub night_bottom: [f32; 3],
    pub sunset_tint: [f32; 3],
    // ดวงอาทิตย์
    pub sun_size: f32,       // cos threshold (สูง = ดวงเล็ก)
    pub sun_brightness: f32, // ความสว่าง (>1 ให้ Bloom ฟุ้ง)
    // ดาว
    pub star_intensity: f32,
    pub star_density: f32, // 0..1 สูง = ดาวน้อย
    pub star_size_min: f32,
    pub star_size_max: f32,
    pub twinkle_rate_base: f32,
    pub twinkle_rate_range: f32,
    pub twinkle_amp: f32,
    // star trail
    pub trail_sensitivity: f32,
    pub trail_max: f32,
    // ทางช้างเผือก (0 = ปิด)
    pub milkyway_brightness: f32,
    // ดวงจันทร์
    pub moon_size: f32,       // รัศมีเชิงมุม
    pub moon_brightness: f32, // 0 = ปิด
    // เมฆ
    pub cloudiness: f32, // 0 = ฟ้าโล่ง .. 1 = เมฆเต็มฟ้า
    pub cloud_wind: f32, // ความเร็วเมฆลอย
}

impl Default for SkySettings {
    fn default() -> Self {
        Self {
            day_top: [0.18, 0.42, 0.86],
            day_horizon: [0.66, 0.80, 0.94],
            day_bottom: [0.52, 0.62, 0.72],
            night_top: [0.01, 0.012, 0.045],
            night_horizon: [0.04, 0.05, 0.10],
            night_bottom: [0.015, 0.02, 0.05],
            sunset_tint: [1.05, 0.45, 0.18],
            sun_size: 0.9995,
            sun_brightness: 4.5,
            star_intensity: 1.4,
            star_density: 0.975,
            star_size_min: 0.07,
            star_size_max: 0.22,
            twinkle_rate_base: 0.35,
            twinkle_rate_range: 0.7,
            twinkle_amp: 0.20,
            trail_sensitivity: 0.0012,
            trail_max: 0.10,
            milkyway_brightness: 0.22,
            moon_size: 0.0678,
            moon_brightness: 1.0,
            cloudiness: 0.40,
            cloud_wind: 0.010,
        }
    }
}

/// โฟลเดอร์เก็บ preset ท้องฟ้า (ไฟล์ json ละ preset) ใต้ root โปรเจค
pub fn presets_root() -> PathBuf {
    crate::voxel::project_root().join("sky_presets")
}

/// ชื่อ preset ทั้งหมดที่เซฟไว้ (เรียงตามตัวอักษร) — ชื่อ = file stem
pub fn list_presets() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(presets_root()) else {
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

pub fn save_preset(name: &str, s: &SkySettings) -> std::io::Result<()> {
    let root = presets_root();
    std::fs::create_dir_all(&root)?;
    let json = serde_json::to_string_pretty(s)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(root.join(format!("{}.json", sanitize(name))), json)
}

pub fn load_preset(name: &str) -> Option<SkySettings> {
    let json = std::fs::read_to_string(presets_root().join(format!("{}.json", sanitize(name)))).ok()?;
    serde_json::from_str(&json).ok()
}

pub fn delete_preset(name: &str) -> std::io::Result<()> {
    std::fs::remove_file(presets_root().join(format!("{}.json", sanitize(name))))
}

/// กันอักขระที่ใช้ในชื่อไฟล์ไม่ได้ (ชื่อไทยกลายเป็น `_` — fallback "preset")
fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() { "preset".to_string() } else { s }
}

// ============================================================================
// Material / uniform
// ============================================================================

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SkyMaterial {
    #[uniform(0)]
    pub data: SkyUniform,
}

/// layout ต้องตรงกับ `SkyUniform` ใน `assets/shaders/sky.wgsl` (จัดเป็น vec4 ให้ std140 align)
#[derive(Clone, Copy, bevy::render::render_resource::ShaderType)]
#[repr(C)]
pub struct SkyUniform {
    pub sky_top: Vec4,
    pub sky_horizon: Vec4,
    pub sky_bottom: Vec4,
    /// rgb = สีดวงอาทิตย์ (>1 ได้ ให้ Bloom จับ)
    pub sun_color: Vec4,
    /// xyz = ทิศดวงอาทิตย์ (normalized), w = night_factor
    pub sun_dir_night: Vec4,
    /// x = ขนาดดวง (cos threshold), y = ความเข้มดาว, z = มุมหมุนโดม (hour_angle), w = ความยาว star trail
    pub params: Vec4,
    /// xyz = ทิศดวงจันทร์ (normalized), w = ความสว่างจันทร์ (0 กลางวัน .. 1 กลางคืน)
    pub moon_dir: Vec4,
    /// x = star density, y = size_min, z = size_max, w = twinkle_amp
    pub star_ctrl: Vec4,
    /// x = twinkle_rate_base, y = twinkle_rate_range, z = milkyway_brightness, w = moon_size
    pub star_ctrl2: Vec4,
    /// x = cloudiness (0..1), y = wind scroll, z = overcast/darken (0..1), w = สำรอง
    pub cloud_ctrl: Vec4,
    /// คอลัมน์เมทริกซ์ horizon→celestial (ประกอบ mat3 ใน shader) — map ทิศมอง→กรอบฟ้าคงที่
    /// ที่ดาว/ทางช้างเผือกติดตรึง ทำให้ขึ้น/ตก/เลื่อนตามฤดู+ละติจูดเอง (ดู [`crate::astro`])
    pub cel0: Vec4,
    pub cel1: Vec4,
    pub cel2: Vec4,
    /// xyz = ทิศใจกลางกาแล็กซี (celestial), w = ความสว่างเฟสจันทร์ 0..1 (ไว้กลบดาว/ทางช้างเผือก)
    pub gal_center: Vec4,
    /// xyz = ขั้วเหนือกาแล็กซี (celestial, ตั้งฉากระนาบทางช้างเผือก), w = สำรอง
    pub gal_pole: Vec4,
}

impl Material for SkyMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/sky.wgsl".into()
    }
    fn vertex_shader() -> ShaderRef {
        "shaders/sky.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // render ผิวในของทรงกลม (กล้องอยู่ข้างใน) — ไม่ cull
        descriptor.primitive.cull_mode = None;
        // ไม่เขียน depth: ทรงกลมอยู่ไกลสุด ผ่าน depth-test แต่ไม่บังภูมิประเทศที่ใกล้กว่า
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

/// เก็บ handle skydome ไว้ให้ระบบอื่นอัปเดต material/ตำแหน่ง
#[derive(Resource)]
pub struct SkyDome {
    pub material: Handle<SkyMaterial>,
    pub entity: Entity,
}

pub struct SkyPlugin;

/// เฟสการเลื่อนของเมฆที่สะสมทีละเฟรม (∝ cloud_wind × day_speed) — เก็บแยกจากนาฬิกาจริง
/// เพราะขับด้วยความเร็วรอบวัน ต้อง integrate เองให้ต่อเนื่องแม้ day_speed เปลี่ยนกลางคัน
/// (เหมือน time_of_day) ถ้าคูณ day_speed กับนาฬิกาจริงตรงๆ เมฆจะกระโดดตอนสไลด์ปรับความเร็ว
#[derive(Resource, Default)]
pub struct CloudPhase(pub f32);

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SkyMaterial>::default())
            .init_resource::<SkySettings>()
            .init_resource::<CloudPhase>()
            .add_systems(Startup, spawn_skydome)
            .add_systems(Update, (follow_camera, update_sky));
    }
}

fn spawn_skydome(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyMaterial>>,
) {
    let mesh = meshes.add(Sphere::new(1.0).mesh().uv(32, 18));
    let material = materials.add(SkyMaterial {
        // เริ่มที่เที่ยงวันด้วยค่า default เดี๋ยว update_sky เขียนทับ
        data: sky_uniform(12.0, 172, 15.0, 1.0, &SkySettings::default()),
    });
    let entity = commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform::from_scale(Vec3::splat(SKY_RADIUS)),
        ))
        .id();
    commands.insert_resource(SkyDome { material, entity });
}

/// ให้ skydome ตามกล้องหลักตลอด (world-space) — จะได้ดูไกลคงที่ ไม่มีวันเดินทะลุขอบ
fn follow_camera(
    dome: Option<Res<SkyDome>>,
    cam: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    mut tf: Query<&mut Transform>,
) {
    let Some(dome) = dome else { return };
    let Ok(cam_tf) = cam.single() else { return };
    if let Ok(mut t) = tf.get_mut(dome.entity) {
        t.translation = cam_tf.translation();
    }
}

/// สร้างค่า uniform จากเวลา + วัน + ละติจูด + ค่าปรับแต่ง — ทิศดวงอาทิตย์/จันทร์/ดาว อิงดาราศาสตร์จริง
/// (`crate::astro`) ดวงอาทิตย์ตัวเดียวกับ `voxel::sun_tint`; day_speed คุมความยาว star trail
fn sky_uniform(time_of_day: f32, day_of_year: u16, latitude_deg: f32, day_speed: f32, sky: &SkySettings) -> SkyUniform {
    let lat = latitude_deg.to_radians();
    let day = day_of_year as f32;
    let sun_dir = crate::astro::sun_direction(time_of_day, day, lat);
    let elevation = sun_dir.y.clamp(0.0, 1.0);

    // 0 กลางวัน -> 1 กลางคืน (โค้งให้พลบค่ำยังไม่มืดเร็วเกิน)
    let night = 1.0 - elevation.powf(0.5);

    // พาเลตกลางวัน/กลางคืน — lerp ตาม elevation
    let top = Vec3::from_array(sky.night_top).lerp(Vec3::from_array(sky.day_top), elevation);
    let mut hor = Vec3::from_array(sky.night_horizon).lerp(Vec3::from_array(sky.day_horizon), elevation);
    let bot = Vec3::from_array(sky.night_bottom).lerp(Vec3::from_array(sky.day_bottom), elevation);

    // แต้มส้มที่ขอบฟ้าตอนดวงอาทิตย์ใกล้ขอบ (พระอาทิตย์ขึ้น/ตก)
    let sunset = (1.0 - (elevation * 3.5).min(1.0)) * (sun_dir.y + 0.15).clamp(0.0, 1.0);
    hor = hor.lerp(Vec3::from_array(sky.sunset_tint), sunset * 0.7);

    // สีดวงอาทิตย์: ขาวตอนสูง อมส้มตอนต่ำ, ดับใต้ขอบฟ้า, เกิน 1 เพื่อ Bloom
    let visible = smoothstep(-0.04, 0.10, sun_dir.y);
    let warm = 1.0 - (elevation * 2.0).min(1.0);
    let sun_rgb = Vec3::new(1.0, 1.0 - 0.35 * warm, 1.0 - 0.65 * warm) * (sky.sun_brightness * visible);

    // ดาวจางลงจนหายตอนใกล้กลางวัน
    let star_intensity = ((night - 0.35) / 0.65).clamp(0.0, 1.0) * sky.star_intensity;

    // ความยาว star trail (เรเดียน) ∝ day_speed
    let trail_span = (day_speed * sky.trail_sensitivity).clamp(0.0, sky.trail_max);

    // ดวงจันทร์: ตำแหน่ง+เฟสอิงดาราศาสตร์ (ขึ้นช้าลง ~50 นาที/วัน, เพ็ญ/ดับตามรอบ synodic)
    let moon_dir = crate::astro::moon_direction(time_of_day, day, lat);
    let moon_vis = smoothstep(-0.05, 0.18, moon_dir.y) * sky.moon_brightness;
    let moon_illum = crate::astro::moon_illumination(day);

    // เมทริกซ์ horizon→celestial + ทิศกาแล็กซี (กรอบฟ้าคงที่) สำหรับ lookup ดาว/ทางช้างเผือกใน shader
    let lst = crate::astro::local_sidereal(time_of_day, day);
    let m = crate::astro::horizon_to_equatorial(lat, lst);
    let gc = crate::astro::galactic_center_cel();
    let gp = crate::astro::galactic_pole_cel();

    SkyUniform {
        sky_top: top.extend(1.0),
        sky_horizon: hor.extend(1.0),
        sky_bottom: bot.extend(1.0),
        sun_color: sun_rgb.extend(1.0),
        sun_dir_night: sun_dir.extend(night),
        // z: เดิมเป็น hour_angle หมุนโดมดาว — ตอนนี้ใช้เมทริกซ์ celestial แทน (z สำรอง)
        params: Vec4::new(sky.sun_size, star_intensity, 0.0, trail_span),
        moon_dir: moon_dir.extend(moon_vis),
        star_ctrl: Vec4::new(sky.star_density, sky.star_size_min, sky.star_size_max, sky.twinkle_amp),
        star_ctrl2: Vec4::new(
            sky.twinkle_rate_base,
            sky.twinkle_rate_range,
            sky.milkyway_brightness,
            sky.moon_size,
        ),
        cloud_ctrl: Vec4::new(sky.cloudiness, sky.cloud_wind, 0.0, 0.0),
        cel0: m.x_axis.extend(0.0),
        cel1: m.y_axis.extend(0.0),
        cel2: m.z_axis.extend(0.0),
        gal_center: gc.extend(moon_illum),
        gal_pole: gp.extend(0.0),
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// อัปเดต uniform ของ skydome ทุกเฟรม — sky มี material เดียว การเขียนทุกเฟรมเบามาก
/// (ต่างจาก update_sun_system ที่มี material หลายตัวเลยต้อง gate) และ**ต้องทุกเฟรม**เพื่อให้
/// มุมหมุนโดมดาว (params.z) ลื่น ไม่งั้นดาวจะกระโดดเป็นสเต็ปทุกวินาทีเหมือนสลับที่
fn update_sky(
    settings: Res<crate::GameSettings>,
    sky: Res<SkySettings>,
    weather: Option<Res<crate::weather::Weather>>,
    dome: Option<Res<SkyDome>>,
    time: Res<Time>,
    mut cloud_phase: ResMut<CloudPhase>,
    mut materials: ResMut<Assets<SkyMaterial>>,
) {
    // เดินเฟสเมฆตามความเร็วรอบวัน: day_speed=1 → เท่าเดิม, เร่งวันเมฆเร็วตาม, หยุดเวลาเมฆหยุด
    cloud_phase.0 += time.delta_secs() * sky.cloud_wind * settings.day_speed;

    let Some(dome) = dome else { return };
    if let Some(mut mat) = materials.get_mut(&dome.material) {
        let mut data = sky_uniform(settings.time_of_day, settings.day_of_year, settings.latitude_deg, settings.day_speed, &sky);
        // เฟสสะสมไปที่ cloud_ctrl.y (shader ใช้เป็น t) แทนค่า cloud_wind ดิบ
        data.cloud_ctrl.y = cloud_phase.0;
        // อากาศครึ้ม: ดันเมฆให้คลุมเต็มฟ้า + เทาลง
        if let Some(w) = weather {
            let oc = w.overcast();
            if oc > 0.0 {
                data.cloud_ctrl.x = data.cloud_ctrl.x.max(0.45 + 0.55 * oc);
                data.cloud_ctrl.z = 0.75 * oc;
            }
        }
        mat.data = data;
    }
}

