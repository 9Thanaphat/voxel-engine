//! สภาพอากาศ: ฝน/หิมะ/ครึ้ม — particle (hanabi) ลอยตามผู้เล่น + ปรับฟ้า/หมอก + เสียงฝน
//!
//! อากาศเป็น host-authoritative (client รับ sync ผ่าน network) เหมือน time_of_day
//! ผลต่อท้องฟ้า (เมฆ/ความมืด) อยู่ใน `sky::update_sky`, ผลต่อหมอกอยู่ใน `weather_fog_system`

use bevy::prelude::*;
use bevy_hanabi::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum WeatherKind {
    #[default]
    Clear,
    Rain,
    Snow,
}

#[derive(Resource)]
pub struct Weather {
    pub kind: WeatherKind,
    /// ความเข้มปัจจุบัน (fade เข้าหา target ให้เนียน) 0..1
    pub intensity: f32,
    pub target: f32,
}

impl Default for Weather {
    fn default() -> Self {
        Self { kind: WeatherKind::Clear, intensity: 0.0, target: 0.0 }
    }
}

impl Weather {
    pub fn set(&mut self, kind: WeatherKind, intensity: f32) {
        self.kind = kind;
        self.target = if kind == WeatherKind::Clear { 0.0 } else { intensity.clamp(0.0, 1.0) };
    }
    /// ความเข้มฝน (0 ถ้าไม่ใช่ฝน) — ไว้คุมเสียง/หมอก
    pub fn rain_amount(&self) -> f32 {
        if self.kind == WeatherKind::Rain { self.intensity } else { 0.0 }
    }
    /// ครึ้ม/มืดจากอากาศ (ฝนหรือหิมะ)
    pub fn overcast(&self) -> f32 {
        if self.kind == WeatherKind::Clear { 0.0 } else { self.intensity }
    }
}

#[derive(Component, Clone, Copy, PartialEq)]
enum Precip {
    Rain,
    Snow,
}

/// สถานะระบบเลือกอากาศอัตโนมัติ (host/single เท่านั้น) — re-roll นานๆ ครั้งตามภูมิอากาศ
/// ณ ตำแหน่งผู้เล่น. `/weather` แมนนวลใช้เลื่อน `next_roll_day` เพื่อค้างอากาศไว้ชั่วคราว
#[derive(Resource, Default)]
pub struct AutoWeather {
    /// วันเกมสัมบูรณ์ที่จะสุ่มอากาศใหม่ครั้งถัดไป (ดู [`absolute_game_day`])
    pub next_roll_day: f64,
    /// xorshift state (seed ตอน init จากเวลา)
    pub rng: u64,
    /// init เฟรมแรกแล้วหรือยัง
    pub started: bool,
}

impl AutoWeather {
    /// เลื่อนนัด re-roll ออกไปเมื่อผู้เล่นตั้งอากาศเอง (`/weather`/UI) — ค้างอากาศแมนนวลไว้ชั่วคราว
    /// แล้วปล่อยให้ auto กลับมาทำงานเอง
    pub fn hold_for_manual(&mut self, settings: &crate::GameSettings) {
        self.next_roll_day = absolute_game_day(settings) + MANUAL_HOLD_DAYS;
    }
}

/// ช่วงเวลาระหว่างเปลี่ยนอากาศ (วันเกม) — "นานๆ เปลี่ยนที" อากาศคงที่เป็นวันๆ
const MIN_INTERVAL_DAYS: f64 = 1.0;
const MAX_INTERVAL_DAYS: f64 = 3.0;
/// `/weather` แมนนวลค้างอากาศไว้กี่วันเกมก่อน auto กลับมาทำงาน
const MANUAL_HOLD_DAYS: f64 = 1.0;
/// อุณหภูมิ (หลัง lapse) ต่ำกว่านี้ = หิมะ ไม่งั้นฝน — ศูนย์สูตร dynamic_temp≈0.7 จึงไม่มีทางหิมะ
const SNOW_TEMP: f64 = -0.15;

/// วันเกมสัมบูรณ์ (โมโนโทนิก) จาก settings — ใช้เทียบ timer ของ auto-weather
pub fn absolute_game_day(s: &crate::GameSettings) -> f64 {
    s.year as f64 * crate::astro::DAYS_PER_YEAR as f64
        + s.day_of_year as f64
        + s.time_of_day as f64 / 24.0
}

/// xorshift64 — คืน u64 ถัดไป (state ต้องไม่เป็น 0)
fn next_u64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

/// สุ่ม 0.0..1.0
fn rand01(state: &mut u64) -> f64 {
    (next_u64(state) >> 11) as f64 / (1u64 << 53) as f64
}

/// สุ่มช่วงเวลาถึง re-roll ครั้งถัดไป (วันเกม)
fn rand_interval(state: &mut u64) -> f64 {
    MIN_INTERVAL_DAYS + rand01(state) * (MAX_INTERVAL_DAYS - MIN_INTERVAL_DAYS)
}

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Weather>()
            .init_resource::<AutoWeather>()
            .add_systems(Startup, setup_precip)
            .add_systems(
                Update,
                (fade_weather, follow_precip, drive_precip, weather_fog_system, auto_weather_system),
            );
    }
}

fn setup_precip(mut commands: Commands, mut effects: ResMut<Assets<EffectAsset>>) {
    let rain = effects.add(rain_effect());
    let snow = effects.add(snow_effect());
    commands.spawn((ParticleEffect::new(rain), Transform::default(), Precip::Rain));
    commands.spawn((ParticleEffect::new(snow), Transform::default(), Precip::Snow));
}

fn rain_effect() -> EffectAsset {
    let writer = ExprWriter::new();
    // ดิสก์เหนือหัวผู้เล่น (center relative กับ emitter ที่ตามกล้อง)
    let init_pos = SetPositionCircleModifier {
        center: writer.lit(Vec3::new(0.0, 16.0, 0.0)).expr(),
        axis: writer.lit(Vec3::Y).expr(),
        radius: writer.lit(22.0).expr(),
        dimension: ShapeDimension::Volume,
    };
    // ตกเร็ว + เฉียงนิดหน่อย
    let hdrift = (writer.rand(VectorType::VEC3F) * writer.lit(Vec3::new(2.0, 0.0, 2.0))
        - writer.lit(Vec3::new(1.0, 0.0, 1.0)))
        * writer.lit(2.0);
    let vel = hdrift + writer.lit(Vec3::new(0.0, -26.0, 0.0));
    let init_vel = SetAttributeModifier::new(Attribute::VELOCITY, vel.expr());
    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
    let init_life = SetAttributeModifier::new(Attribute::LIFETIME, writer.lit(1.2).expr());

    let mut color = bevy_hanabi::Gradient::new();
    color.add_key(0.0, Vec4::new(0.62, 0.72, 0.95, 0.55));
    color.add_key(1.0, Vec4::new(0.62, 0.72, 0.95, 0.45));
    let mut size = bevy_hanabi::Gradient::new();
    size.add_key(0.0, Vec3::new(0.035, 0.55, 0.035)); // เส้นยาวแนวตั้ง (billboard สูง)

    EffectAsset::new(1600, SpawnerSettings::rate(520.0.into()), writer.finish())
        .with_name("rain")
        .init(init_pos)
        .init(init_vel)
        .init(init_age)
        .init(init_life)
        .render(ColorOverLifetimeModifier {
            gradient: color,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(SizeOverLifetimeModifier { gradient: size, screen_space_size: false })
}

fn snow_effect() -> EffectAsset {
    let writer = ExprWriter::new();
    let init_pos = SetPositionCircleModifier {
        center: writer.lit(Vec3::new(0.0, 16.0, 0.0)).expr(),
        axis: writer.lit(Vec3::Y).expr(),
        radius: writer.lit(22.0).expr(),
        dimension: ShapeDimension::Volume,
    };
    // ตกช้า ล่องลอย ปลิวแนวราบเบาๆ
    let hdrift = (writer.rand(VectorType::VEC3F) * writer.lit(Vec3::new(2.0, 0.0, 2.0))
        - writer.lit(Vec3::new(1.0, 0.0, 1.0)))
        * writer.lit(1.2);
    let vel = hdrift + writer.lit(Vec3::new(0.0, -2.4, 0.0));
    let init_vel = SetAttributeModifier::new(Attribute::VELOCITY, vel.expr());
    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
    let init_life = SetAttributeModifier::new(Attribute::LIFETIME, writer.lit(7.0).expr());

    let mut color = bevy_hanabi::Gradient::new();
    color.add_key(0.0, Vec4::new(0.95, 0.96, 1.0, 0.9));
    color.add_key(1.0, Vec4::new(0.95, 0.96, 1.0, 0.8));
    let mut size = bevy_hanabi::Gradient::new();
    size.add_key(0.0, Vec3::splat(0.11));

    EffectAsset::new(1400, SpawnerSettings::rate(150.0.into()), writer.finish())
        .with_name("snow")
        .init(init_pos)
        .init(init_vel)
        .init(init_age)
        .init(init_life)
        .render(ColorOverLifetimeModifier {
            gradient: color,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(SizeOverLifetimeModifier { gradient: size, screen_space_size: false })
}

/// ค่อยๆ ปรับ intensity เข้าหา target
fn fade_weather(time: Res<Time>, mut weather: ResMut<Weather>) {
    let d = time.delta_secs() * 0.35;
    if (weather.intensity - weather.target).abs() <= d {
        weather.intensity = weather.target;
    } else if weather.intensity < weather.target {
        weather.intensity += d;
    } else {
        weather.intensity -= d;
    }
}

/// emitter ตามกล้องหลัก (particle spawn รอบตัวผู้เล่นเสมอ)
fn follow_precip(
    cam: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    mut precip: Query<&mut Transform, With<Precip>>,
) {
    let Ok(cam_tf) = cam.single() else { return };
    let p = cam_tf.translation();
    for mut tf in &mut precip {
        tf.translation = p;
    }
}

/// เปิด/ปิด emitter ตามชนิดอากาศ
fn drive_precip(weather: Res<Weather>, mut q: Query<(&Precip, &mut EffectSpawner)>) {
    for (kind, mut spawner) in &mut q {
        let want = weather.intensity > 0.02
            && ((*kind == Precip::Rain && weather.kind == WeatherKind::Rain)
                || (*kind == Precip::Snow && weather.kind == WeatherKind::Snow));
        spawner.active = want;
    }
}

/// ตอนฝน/หิมะ: หมอกหนาขึ้น + เทา (ทับ update_sun_system เฉพาะตอน overcast>0)
fn weather_fog_system(
    weather: Res<Weather>,
    mut fog_query: Query<&mut bevy::pbr::DistanceFog>,
) {
    let oc = weather.overcast();
    if oc <= 0.001 {
        return; // อากาศใส — ปล่อย update_sun_system คุมหมอกตามเดิม
    }
    for mut fog in &mut fog_query {
        if let bevy::pbr::FogFalloff::Linear { start, end } = &mut fog.falloff {
            // ครึ้มมาก = มองเห็นใกล้ลง
            *start = 800.0 - 400.0 * oc;
            *end = 35_000.0 - 22_000.0 * oc;
        }
        let base = Vec3::new(fog.color.to_srgba().red, fog.color.to_srgba().green, fog.color.to_srgba().blue);
        let grey = Vec3::splat(0.62);
        let c = base.lerp(grey, oc * 0.7);
        fog.color = Color::srgb(c.x, c.y, c.z);
    }
}

/// สุ่มอากาศจากภูมิอากาศ ณ ตำแหน่ง: โอกาสตกอิงความชื้น (แห้ง→แทบไม่ตก),
/// หนาว/บนภูเขา (temp หลัง lapse ต่ำ) → หิมะ ไม่งั้นฝน
fn roll_weather(temp: f64, humidity: f64, alt_above_sea: f64, rng: &mut u64) -> (WeatherKind, f32) {
    // ชื้น (humidity สูง) = โอกาสตกสูง; ทะเลทราย humidity ต่ำ → ~0
    let p_precip = ((humidity + 0.35) * 0.7).clamp(0.02, 0.85);
    if rand01(rng) > p_precip {
        return (WeatherKind::Clear, 0.0);
    }
    let t = crate::biome::lapse_adjust(temp, alt_above_sea.max(0.0) as i32);
    let intensity = (0.35 + rand01(rng) * 0.6) as f32;
    if t < SNOW_TEMP {
        (WeatherKind::Snow, intensity)
    } else {
        (WeatherKind::Rain, intensity)
    }
}

/// เลือกอากาศอัตโนมัติ (host/single) — re-roll นานๆ ครั้งตามภูมิอากาศที่ตำแหน่งผู้เล่น
/// แล้ว broadcast ให้ client (host-authoritative เหมือน time). client รับผ่าน network จึงไม่รันเอง
fn auto_weather_system(
    time: Res<Time>,
    settings: Res<crate::GameSettings>,
    mut weather: ResMut<Weather>,
    mut auto: ResMut<AutoWeather>,
    net_client: Option<Res<bevy_renet::RenetClient>>,
    server: Option<ResMut<bevy_renet::RenetServer>>,
    cam: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
) {
    if net_client.is_some() {
        return; // client รับอากาศจาก host
    }
    let now = absolute_game_day(&settings);
    if !auto.started {
        // seed rng จากเวลาเดินเครื่อง (ให้ไม่ซ้ำทุกรอบเกม) แล้วตั้งนัด re-roll แรก
        auto.rng = (time.elapsed().as_nanos() as u64) | 1;
        auto.next_roll_day = now + rand_interval(&mut auto.rng);
        auto.started = true;
        return;
    }
    if now < auto.next_roll_day {
        return;
    }
    let Ok(cam_tf) = cam.single() else { return }; // ยังไม่มีกล้อง/โลก — รอ
    let p = cam_tf.translation();
    let (wx, wz) = (p.x as f64, p.z as f64);

    // ภูมิอากาศ ณ ตำแหน่ง — temp อิงละติจูด+ฤดู (dynamic_temp), humidity จาก sampler (มี band แล้ว)
    let lat = crate::biome::climate_lat(wz);
    let temp = crate::biome::dynamic_temp(lat, settings.day_of_year as f32);
    let sampler = crate::voxel::TerrainSampler::new(settings.noise);
    let humidity = sampler.humidity_raw(wx, wz);
    let alt = p.y as f64 - crate::voxel::SEA_LEVEL as f64;

    let (kind, intensity) = roll_weather(temp, humidity, alt, &mut auto.rng);
    weather.set(kind, intensity);
    if let Some(mut server) = server {
        server.broadcast_message(
            bevy_renet::renet::DefaultChannel::ReliableOrdered,
            crate::network::encode(&crate::network::ServerMessage::Weather {
                kind,
                intensity: weather.target,
            }),
        );
    }
    auto.next_roll_day = now + rand_interval(&mut auto.rng);
}
