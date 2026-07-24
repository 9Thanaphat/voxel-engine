//! ระบบเสียง SFX — event-driven จาก `BlockFx`/`ExplosionFx` ที่มีอยู่แล้ว + footstep poll
//!
//! ไฟล์เสียงอยู่ `assets/sounds/` (placeholder เจนจาก `tools/gen_sounds.py`)
//! ไฟล์ไหนไม่มีก็โหลดเป็น `None` = เงียบ ไม่ crash — เปลี่ยนเป็นเสียง CC0 จริงได้โดยวางทับชื่อเดิม

use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;

use crate::particles::{BlockFx, ExplosionFx};
use crate::voxel::{BlockType, VoxelWorld};

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_sounds).add_systems(
            Update,
            (block_sound_system, explosion_sound_system, footstep_system, rain_audio_system),
        );
    }
}

/// handle ของเสียงแต่ละอัน (None = ไม่มีไฟล์ = เงียบ)
#[derive(Resource, Default)]
pub struct SoundBank {
    dig_stone: Option<Handle<AudioSource>>,
    dig_dirt: Option<Handle<AudioSource>>,
    dig_grass: Option<Handle<AudioSource>>,
    dig_wood: Option<Handle<AudioSource>>,
    dig_glass: Option<Handle<AudioSource>>,
    step_stone: Option<Handle<AudioSource>>,
    step_soft: Option<Handle<AudioSource>>,
    step_wood: Option<Handle<AudioSource>>,
    splash: Option<Handle<AudioSource>>,
    explosion: Option<Handle<AudioSource>>,
    rain_loop: Option<Handle<AudioSource>>,
}

/// marker ของ entity เสียงฝน loop (คุม volume ตาม weather)
#[derive(Component)]
struct RainLoopSink;

fn load_sounds(asset_server: Res<AssetServer>, mut commands: Commands) {
    // โหลดเฉพาะไฟล์ที่มีจริง — กัน warning/พฤติกรรมแปลกตอนไฟล์หาย
    let sounds_dir = crate::voxel::project_root().join("assets").join("sounds");
    let load = |name: &str| -> Option<Handle<AudioSource>> {
        if sounds_dir.join(name).exists() {
            Some(asset_server.load(format!("sounds/{name}")))
        } else {
            None
        }
    };
    commands.insert_resource(SoundBank {
        dig_stone: load("dig_stone.wav"),
        dig_dirt: load("dig_dirt.wav"),
        dig_grass: load("dig_grass.wav"),
        dig_wood: load("dig_wood.wav"),
        dig_glass: load("dig_glass.wav"),
        step_stone: load("step_stone.wav"),
        step_soft: load("step_soft.wav"),
        step_wood: load("step_wood.wav"),
        splash: load("splash.wav"),
        explosion: load("explosion.wav"),
        rain_loop: load("rain_loop.wav"),
    });
}

/// เสียงฝน loop ตัวเดียว คุม volume ตามความเข้มฝน (0 = เงียบ)
fn rain_audio_system(
    weather: Res<crate::weather::Weather>,
    bank: Res<SoundBank>,
    mut commands: Commands,
    mut sinks: Query<&mut AudioSink, With<RainLoopSink>>,
    mut spawned: Local<bool>,
) {
    let Some(h) = &bank.rain_loop else { return };
    if !*spawned {
        *spawned = true;
        commands.spawn((
            AudioPlayer(h.clone()),
            PlaybackSettings {
                volume: Volume::Linear(0.0),
                ..PlaybackSettings::LOOP
            },
            RainLoopSink,
        ));
    }
    let target = weather.rain_amount() * 0.5;
    for mut sink in &mut sinks {
        sink.set_volume(Volume::Linear(target));
    }
}

/// หมวดวัสดุของบล็อก → เลือกเสียง
#[derive(Clone, Copy)]
enum Mat {
    Stone,
    Dirt,
    Grass,
    Wood,
    Glass,
}

fn block_mat(b: BlockType) -> Mat {
    use BlockType::*;
    match b {
        Glass => Mat::Glass,
        Wood | Chest | Tnt | TntLit => Mat::Wood,
        Dirt | Sand => Mat::Dirt,
        Grass | Leaves | TallGrass | Branch | Campfire => Mat::Grass,
        // ที่เหลือ (Stone/IronBlock/Glowstone/Lamp*/Chiseled/Furnace/Switch*/SmartLamp*/Nuke*) = หิน
        _ => Mat::Stone,
    }
}

fn dig_sound(bank: &SoundBank, m: Mat) -> &Option<Handle<AudioSource>> {
    match m {
        Mat::Stone => &bank.dig_stone,
        Mat::Dirt => &bank.dig_dirt,
        Mat::Grass => &bank.dig_grass,
        Mat::Wood => &bank.dig_wood,
        Mat::Glass => &bank.dig_glass,
    }
}

fn step_sound(bank: &SoundBank, m: Mat) -> &Option<Handle<AudioSource>> {
    match m {
        Mat::Stone | Mat::Glass => &bank.step_stone,
        Mat::Wood => &bank.step_wood,
        Mat::Dirt | Mat::Grass => &bank.step_soft,
    }
}

/// เล่นเสียง one-shot + สุ่ม volume/speed เล็กน้อยให้ไม่จำเจ (despawn เองเมื่อจบ)
fn play(commands: &mut Commands, handle: &Option<Handle<AudioSource>>, base_vol: f32) {
    let Some(h) = handle else { return };
    let vol = base_vol * (0.85 + fastrand::f32() * 0.3);
    let speed = 0.92 + fastrand::f32() * 0.16;
    commands.spawn((
        AudioPlayer(h.clone()),
        PlaybackSettings {
            volume: Volume::Linear(vol),
            speed,
            ..PlaybackSettings::DESPAWN
        },
    ));
}

/// ทุบ/วางบล็อก → เสียงตามหมวดวัสดุ; เกี่ยวกับน้ำ → splash
fn block_sound_system(
    mut ev: MessageReader<BlockFx>,
    bank: Res<SoundBank>,
    mut commands: Commands,
) {
    for fx in ev.read() {
        if fx.placed.is_water() || fx.replaced.is_water() {
            play(&mut commands, &bank.splash, 0.5);
            continue;
        }
        // placed==Air = ทุบ (เสียงตามของที่หายไป), ไม่งั้น = วาง (เบากว่า)
        let (block, vol) = if fx.placed == BlockType::Air {
            (fx.replaced, 0.7)
        } else {
            (fx.placed, 0.5)
        };
        if block == BlockType::Air {
            continue;
        }
        play(&mut commands, dig_sound(&bank, block_mat(block)), vol);
    }
}

fn explosion_sound_system(
    mut ev: MessageReader<ExplosionFx>,
    bank: Res<SoundBank>,
    mut commands: Commands,
) {
    for _ in ev.read() {
        play(&mut commands, &bank.explosion, 1.0);
    }
}

/// เสียงก้าวเดิน — poll ตำแหน่งผู้เล่น: เดินบนพื้นตัน = step ตามหมวด, ลงน้ำครั้งแรก = splash
fn footstep_system(
    world: Res<VoxelWorld>,
    bank: Res<SoundBank>,
    query: Query<(&crate::camera::FreeCamera, &Transform)>,
    mut commands: Commands,
    mut prev_pos: Local<Option<Vec3>>,
    mut accum: Local<f32>,
    mut prev_in_water: Local<bool>,
) {
    let Ok((cam, tf)) = query.single() else {
        *prev_pos = None;
        return;
    };
    let pos = tf.translation;
    let Some(prev) = prev_pos.replace(pos) else {
        return; // เฟรมแรก ยังไม่มีตำแหน่งก่อนหน้า
    };

    // บินอยู่ = ไม่มีเสียงก้าว
    if cam.fly {
        *accum = 0.0;
        return;
    }

    let feet = pos - Vec3::Y * crate::camera::EYE_HEIGHT;
    let head_b = world.get_block(pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32);
    let feet_b = world.get_block(feet.x.floor() as i32, (feet.y + 0.1).floor() as i32, feet.z.floor() as i32);
    let in_water = head_b.is_water() || feet_b.is_water();

    // เข้าน้ำครั้งแรก = splash (transition dry -> wet)
    if in_water && !*prev_in_water {
        play(&mut commands, &bank.splash, 0.45);
    }
    *prev_in_water = in_water;
    if in_water {
        *accum = 0.0;
        return;
    }

    // ยืนบนพื้นตันไหม (บล็อกใต้เท้า)
    let below = world.get_block(feet.x.floor() as i32, (feet.y - 0.1).floor() as i32, feet.z.floor() as i32);
    if !below.is_solid() {
        *accum = 0.0;
        return;
    }

    // สะสมระยะเดินแนวราบ ครบ 1 ช่วงก้าวค่อยเล่นเสียง (sync กับการเคลื่อนที่จริง)
    let step_dist = Vec3::new(pos.x - prev.x, 0.0, pos.z - prev.z).length();
    *accum += step_dist;
    if *accum >= 2.0 {
        *accum = 0.0;
        play(&mut commands, step_sound(&bank, block_mat(below)), 0.5);
    }
}
