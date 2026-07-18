use bevy::prelude::*;
use bevy_hanabi::prelude::*;

use crate::voxel::{self, BlockType};

// --------------------------------------------------------
// Particle effects (bevy_hanabi — GPU particles)
// - เศษบล็อกตอนทุบ: burst ใช้ texture ของบล็อกจริง (fallback สีบล็อก)
// - น้ำกระเซ็น: burst ตอน edit ที่เกี่ยวกับน้ำ
// - ประกายไฟ: ลูกไฟจิ๋วลอยจาก lamp/glowstone (เกาะ PointLight entity)
// --------------------------------------------------------

/// block edit จากมือผู้เล่น (ไว้ยิง particle) — ตั้งใจไม่ hook apply_block_edit
/// เพราะโดน bulk replay ตอน chunk โหลด/net sync จะพ่น particle มั่ว
#[derive(Message)]
pub struct BlockFx {
    pub pos: IVec3,
    pub placed: BlockType,
    pub replaced: BlockType,
}

/// ระเบิด TNT ที่จุด center — ยิงจาก tnt_detonation_system (host/single)
#[derive(Message)]
pub struct ExplosionFx {
    pub center: Vec3,
    /// เส้นทาง ray ทุกเส้นของระเบิด — ขับหน้าคลื่น shockwave
    pub rays: Vec<voxel::RaySeg>,
    /// พลังงานต่อ ray ตอนเริ่ม (หลังสเกลตามขนาดกอง) — ไว้ normalize ความเข้มคลื่น
    pub power: f32,
    /// nuke: flash นาน/แรงกว่า + เห็ดควัน + คลื่นเร็วกว่า
    pub is_nuke: bool,
}

/// material กลางของ shockwave: unlit + โปร่งใส สี/alpha มาจาก vertex color
/// (แพทเทิร์นเดียวกับ water/glass material — สี >1.0 ให้ bloom จับ)
#[derive(Resource)]
pub struct ShockwaveMaterial(Handle<StandardMaterial>);

/// คลื่นที่กำลังวิ่งอยู่ — หน้าคลื่น ณ เวลา t คือจุดบนแต่ละ ray ที่ระยะสะสม = SPEED×t
pub struct Shockwave {
    segs: Vec<voxel::RaySeg>,
    power: f32,
    age: f32,
    max_dist: f32,
    /// ความเร็วหน้าคลื่น (TNT 30 / nuke 60 — ตรงกับจังหวะ finalize บล็อก)
    speed: f32,
    entity: Entity,
    mesh: Handle<Mesh>,
}

#[derive(Resource, Default)]
pub struct ActiveShockwaves(Vec<Shockwave>);

/// ความเร็วหน้าคลื่น (บล็อก/วินาที)
const SHOCKWAVE_SPEED: f32 = 30.0;

/// แสงวาบของระเบิด — fade แล้ว despawn เอง (และกัน attach_lamp_sparkles มาเกาะ)
#[derive(Component)]
pub struct ExplosionFlash {
    age: f32,
    peak_intensity: f32,
    /// TNT ~0.45s / nuke นานหลายวินาทีแบบของจริง
    duration: f32,
    /// เพิ่ม AmbientLight ทั้งฉากตอนพีค (nuke — ทั้งโลกสว่างวาบ ไม่ใช่แค่จุดเดียว)
    ambient_boost: f32,
}

#[derive(Resource)]
pub struct ParticleAssets {
    debris: Handle<EffectAsset>,
    splash: Handle<EffectAsset>,
    sparkle: Handle<EffectAsset>,
    explosion: Handle<EffectAsset>,
    mushroom_column: Handle<EffectAsset>,
    mushroom_cap: Handle<EffectAsset>,
    /// texture ขาว 1x1 คู่กับ tint = สีบล็อก สำหรับบล็อกที่ยังไม่มี texture
    white: Handle<Image>,
}

/// effect ชั่วคราว (burst) — hanabi ไม่ despawn entity ให้เอง ต้องจับเวลาเก็บเอง
#[derive(Component)]
pub struct FxLifetime(pub Timer);

/// เศษบล็อก: กระเด็นออกรอบตัว + เด้งขึ้น แล้วโดน gravity ดึงตก
/// texture bind ต่อ instance ผ่าน EffectMaterial, tint ต่อ instance ผ่าน property
fn debris_effect() -> EffectAsset {
    let writer = ExprWriter::new();

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(0.3).expr(),
        dimension: ShapeDimension::Volume,
    };

    let dir = (writer.rand(VectorType::VEC3F) * writer.lit(2.0) - writer.lit(1.0)).normalized();
    let speed = writer.lit(1.0).uniform(writer.lit(2.5));
    let vel = dir * speed + writer.lit(Vec3::Y * 2.0);
    let init_vel = SetAttributeModifier::new(Attribute::VELOCITY, vel.expr());

    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
    let lifetime = writer.lit(0.45).uniform(writer.lit(0.9)).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

    // tint: ขาว = โชว์ texture ตรงๆ / สีบล็อก (คู่ texture ขาว) เมื่อไม่มี texture
    let tint = writer.add_property("tint", 0xFFFFFFFFu32.into());
    let init_color = SetAttributeModifier::new(Attribute::COLOR, writer.prop(tint).expr());

    // หมุนสุ่มต่อชิ้นให้ดูเป็นเศษ ไม่ใช่แถวสี่เหลี่ยมเรียงกัน
    let rotation = (writer.rand(ScalarType::Float) * writer.lit(std::f32::consts::TAU)).expr();
    let init_rotation = SetAttributeModifier::new(Attribute::F32_0, rotation);
    let rotation_attr = writer.attr(Attribute::F32_0).expr();

    let update_accel = AccelModifier::new(writer.lit(Vec3::Y * -14.0).expr());
    let update_drag = LinearDragModifier::new(writer.lit(1.5).expr());

    let texture_slot = writer.lit(0u32).expr();
    let mut module = writer.finish();
    module.add_texture_slot("color");

    // ขนาดคงที่แล้วหดหายช่วงท้ายอายุ
    let mut size = bevy_hanabi::Gradient::new();
    size.add_key(0.0, Vec3::splat(0.12));
    size.add_key(0.75, Vec3::splat(0.12));
    size.add_key(1.0, Vec3::ZERO);

    EffectAsset::new(64, SpawnerSettings::once(24.0.into()), module)
        .with_name("block_debris")
        .init(init_pos)
        .init(init_vel)
        .init(init_age)
        .init(init_lifetime)
        .init(init_color)
        .init(init_rotation)
        .update(update_accel)
        .update(update_drag)
        .render(ParticleTextureModifier {
            texture_slot,
            sample_mapping: ImageSampleMapping::Modulate,
        })
        .render(OrientModifier {
            mode: OrientMode::FaceCameraPosition,
            rotation: Some(rotation_attr),
        })
        .render(SizeOverLifetimeModifier {
            gradient: size,
            screen_space_size: false,
        })
}

/// หยดน้ำ: พุ่งขึ้นเป็นพุ่มแล้วตก จางหายไว
fn splash_effect() -> EffectAsset {
    let writer = ExprWriter::new();

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(0.25).expr(),
        dimension: ShapeDimension::Volume,
    };

    let dir = (writer.rand(VectorType::VEC3F) * writer.lit(2.0) - writer.lit(1.0)).normalized();
    let speed = writer.lit(0.5).uniform(writer.lit(1.5));
    let vel = dir * speed + writer.lit(Vec3::Y * 3.2);
    let init_vel = SetAttributeModifier::new(Attribute::VELOCITY, vel.expr());

    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
    let lifetime = writer.lit(0.35).uniform(writer.lit(0.7)).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

    let update_accel = AccelModifier::new(writer.lit(Vec3::Y * -12.0).expr());

    // สีน้ำโทนเดียวกับบล็อกน้ำ จางปลายอายุ
    let c = voxel::block_color(BlockType::Water);
    let mut color = bevy_hanabi::Gradient::new();
    color.add_key(0.0, Vec4::new(c[0] * 1.6, c[1] * 1.6, c[2] * 1.6, 0.9));
    color.add_key(1.0, Vec4::new(c[0], c[1], c[2], 0.0));

    let mut size = bevy_hanabi::Gradient::new();
    size.add_key(0.0, Vec3::splat(0.07));
    size.add_key(1.0, Vec3::splat(0.03));

    EffectAsset::new(64, SpawnerSettings::once(30.0.into()), writer.finish())
        .with_name("water_splash")
        .init(init_pos)
        .init(init_vel)
        .init(init_age)
        .init(init_lifetime)
        .update(update_accel)
        .render(ColorOverLifetimeModifier {
            gradient: color,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(SizeOverLifetimeModifier {
            gradient: size,
            screen_space_size: false,
        })
}

/// ประกายไฟรอบบล็อกเรืองแสง: จุดเล็กๆ ลอยขึ้นช้าๆ ต่อเนื่อง
/// สีตั้งต่อ instance ผ่าน property (คูณ >1 ให้ bloom จับ — กล้องเปิด Hdr อยู่แล้ว)
fn sparkle_effect() -> EffectAsset {
    let writer = ExprWriter::new();

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(0.45).expr(),
        dimension: ShapeDimension::Volume,
    };

    let vel = writer.lit(Vec3::Y * 0.15) + writer.lit(Vec3::Y * 0.2) * writer.rand(ScalarType::Float);
    let init_vel = SetAttributeModifier::new(Attribute::VELOCITY, vel.expr());

    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
    let lifetime = writer.lit(0.8).uniform(writer.lit(1.4)).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

    let tint = writer.add_property("tint", Vec4::new(3.0, 2.7, 1.5, 1.0).into());
    let init_color = SetAttributeModifier::new(Attribute::HDR_COLOR, writer.prop(tint).expr());

    let mut size = bevy_hanabi::Gradient::new();
    size.add_key(0.0, Vec3::splat(0.045));
    size.add_key(1.0, Vec3::ZERO);

    EffectAsset::new(16, SpawnerSettings::rate(3.0.into()), writer.finish())
        .with_name("lamp_sparkle")
        .init(init_pos)
        .init(init_vel)
        .init(init_age)
        .init(init_lifetime)
        .init(init_color)
        .render(SizeOverLifetimeModifier {
            gradient: size,
            screen_space_size: false,
        })
}

/// ลูกระเบิด: แฟลชส้มจ้า (HDR ให้ bloom จับ) ขยายตัวกลายเป็นควันเทาลอยขึ้น
fn explosion_effect() -> EffectAsset {
    let writer = ExprWriter::new();

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(0.5).expr(),
        dimension: ShapeDimension::Volume,
    };

    let dir = (writer.rand(VectorType::VEC3F) * writer.lit(2.0) - writer.lit(1.0)).normalized();
    let speed = writer.lit(2.0).uniform(writer.lit(6.0));
    let vel = dir * speed + writer.lit(Vec3::Y * 1.5);
    let init_vel = SetAttributeModifier::new(Attribute::VELOCITY, vel.expr());

    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
    let lifetime = writer.lit(0.6).uniform(writer.lit(1.5)).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

    // ควันเบา: drag แรง + แรงลอยขึ้นอ่อนๆ แทน gravity
    let update_accel = AccelModifier::new(writer.lit(Vec3::Y * 1.0).expr());
    let update_drag = LinearDragModifier::new(writer.lit(2.5).expr());

    // แฟลชส้ม HDR ช่วงแรก → ควันเทา → จางหาย
    let mut color = bevy_hanabi::Gradient::new();
    color.add_key(0.0, Vec4::new(8.0, 4.0, 1.2, 1.0));
    color.add_key(0.12, Vec4::new(3.0, 1.4, 0.5, 1.0));
    color.add_key(0.35, Vec4::new(0.35, 0.33, 0.30, 0.85));
    color.add_key(1.0, Vec4::new(0.18, 0.18, 0.18, 0.0));

    // ลูกไฟขยายเป็นควันก้อนใหญ่ขึ้นเรื่อยๆ
    let mut size = bevy_hanabi::Gradient::new();
    size.add_key(0.0, Vec3::splat(0.35));
    size.add_key(0.3, Vec3::splat(0.7));
    size.add_key(1.0, Vec3::splat(1.1));

    EffectAsset::new(128, SpawnerSettings::once(70.0.into()), writer.finish())
        .with_name("tnt_explosion")
        .init(init_pos)
        .init(init_vel)
        .init(init_age)
        .init(init_lifetime)
        .update(update_accel)
        .update(update_drag)
        .render(ColorOverLifetimeModifier {
            gradient: color,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(SizeOverLifetimeModifier {
            gradient: size,
            screen_space_size: false,
        })
}

/// ลำต้นเห็ดควัน: ควันร้อนพวยพุ่งขึ้นจากจุดระเบิดเป็นคอลัมน์แคบ อายุยาว
fn mushroom_column_effect() -> EffectAsset {
    let writer = ExprWriter::new();

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(2.5).expr(),
        dimension: ShapeDimension::Volume,
    };

    // พุ่งขึ้นแรง + ส่ายข้างเล็กน้อย — ต้องไต่ถึงระดับหัวเห็ด (~85 บล็อก)
    let xz = (writer.rand(VectorType::VEC3F) * writer.lit(2.0) - writer.lit(1.0))
        * writer.lit(Vec3::new(2.0, 0.0, 2.0));
    let up = writer.lit(Vec3::Y * 26.0) + writer.lit(Vec3::Y * 14.0) * writer.rand(ScalarType::Float);
    let init_vel = SetAttributeModifier::new(Attribute::VELOCITY, (xz + up).expr());

    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
    let lifetime = writer.lit(12.0).uniform(writer.lit(20.0)).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

    let update_drag = LinearDragModifier::new(writer.lit(0.35).expr());

    // ร้อนจ้าช่วงสั้นๆ ที่โคน แล้วเป็นควันเทา จางตอนท้าย
    let mut color = bevy_hanabi::Gradient::new();
    color.add_key(0.0, Vec4::new(4.0, 2.0, 0.7, 0.95));
    color.add_key(0.15, Vec4::new(0.45, 0.42, 0.38, 0.9));
    color.add_key(0.8, Vec4::new(0.3, 0.3, 0.3, 0.7));
    color.add_key(1.0, Vec4::new(0.25, 0.25, 0.25, 0.0));

    let mut size = bevy_hanabi::Gradient::new();
    size.add_key(0.0, Vec3::splat(3.0));
    size.add_key(0.5, Vec3::splat(6.5));
    size.add_key(1.0, Vec3::splat(9.0));

    EffectAsset::new(2048, SpawnerSettings::once(800.0.into()), writer.finish())
        .with_name("mushroom_column")
        .init(init_pos)
        .init(init_vel)
        .init(init_age)
        .init(init_lifetime)
        .update(update_drag)
        .render(ColorOverLifetimeModifier {
            gradient: color,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(SizeOverLifetimeModifier {
            gradient: size,
            screen_space_size: false,
        })
}

/// หัวเห็ด: ก้อนควันหนาแผ่ออกด้านบน (เกิดสูงกว่าจุดระเบิด ~28 บล็อก)
/// alpha ไต่ขึ้นช้าๆ ให้รู้สึกว่าหัวเห็ด "ก่อตัว" หลังคอลัมน์พุ่งขึ้นไปถึง
fn mushroom_cap_effect() -> EffectAsset {
    let writer = ExprWriter::new();

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::Y * 85.0).expr(),
        radius: writer.lit(14.0).expr(),
        dimension: ShapeDimension::Volume,
    };

    // แผ่ออกแนวนอน + ลอยขึ้นช้า
    let out = (writer.rand(VectorType::VEC3F) * writer.lit(2.0) - writer.lit(1.0))
        * writer.lit(Vec3::new(5.5, 0.8, 5.5));
    let init_vel =
        SetAttributeModifier::new(Attribute::VELOCITY, (out + writer.lit(Vec3::Y * 2.5)).expr());

    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
    let lifetime = writer.lit(15.0).uniform(writer.lit(24.0)).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

    let update_drag = LinearDragModifier::new(writer.lit(0.5).expr());

    let mut color = bevy_hanabi::Gradient::new();
    color.add_key(0.0, Vec4::new(0.5, 0.45, 0.4, 0.0));
    color.add_key(0.2, Vec4::new(0.5, 0.45, 0.4, 0.85));
    color.add_key(0.8, Vec4::new(0.32, 0.32, 0.32, 0.7));
    color.add_key(1.0, Vec4::new(0.28, 0.28, 0.28, 0.0));

    let mut size = bevy_hanabi::Gradient::new();
    size.add_key(0.0, Vec3::splat(6.0));
    size.add_key(0.5, Vec3::splat(14.0));
    size.add_key(1.0, Vec3::splat(19.0));

    EffectAsset::new(2048, SpawnerSettings::once(900.0.into()), writer.finish())
        .with_name("mushroom_cap")
        .init(init_pos)
        .init(init_vel)
        .init(init_age)
        .init(init_lifetime)
        .update(update_drag)
        .render(ColorOverLifetimeModifier {
            gradient: color,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(SizeOverLifetimeModifier {
            gradient: size,
            screen_space_size: false,
        })
}

pub fn setup_particles(
    mut commands: Commands,
    mut effects: ResMut<Assets<EffectAsset>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(ShockwaveMaterial(materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        // ระบุ path เต็ม: ทั้ง bevy กับ bevy_hanabi prelude มี AlphaMode ชนกัน
        alpha_mode: bevy::prelude::AlphaMode::Blend,
        cull_mode: None, // billboard เห็นได้สองหน้า ไม่ต้องเป๊ะเรื่อง winding
        ..default()
    })));

    let white = images.add(Image::new_fill(
        bevy::render::render_resource::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        &[255, 255, 255, 255],
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    ));

    commands.insert_resource(ParticleAssets {
        debris: effects.add(debris_effect()),
        splash: effects.add(splash_effect()),
        sparkle: effects.add(sparkle_effect()),
        explosion: effects.add(explosion_effect()),
        mushroom_column: effects.add(mushroom_column_effect()),
        mushroom_cap: effects.add(mushroom_cap_effect()),
        white,
    });
}

/// แปลงสีบล็อก -> u32 แบบ packed (ABGR ตาม pack4x8unorm ของ hanabi)
fn pack_color(c: [f32; 4]) -> u32 {
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
    (to_u8(c[3]) << 24) | (to_u8(c[2]) << 16) | (to_u8(c[1]) << 8) | to_u8(c[0])
}

pub fn spawn_block_fx(
    mut msgs: MessageReader<BlockFx>,
    mut commands: Commands,
    assets: Res<ParticleAssets>,
    asset_server: Res<AssetServer>,
) {
    for fx in msgs.read() {
        let center = fx.pos.as_vec3() + Vec3::splat(0.5);

        // edit แตะน้ำ (วางน้ำ/ทุบน้ำ/วางบล็อกทับน้ำ) — น้ำกระเซ็น
        if fx.placed.is_water() || fx.replaced.is_water() {
            commands.spawn((
                ParticleEffect::new(assets.splash.clone()),
                Transform::from_translation(center),
                FxLifetime(Timer::from_seconds(1.2, TimerMode::Once)),
            ));
        }

        // ทุบบล็อกจริงๆ (ไม่ใช่อากาศ/น้ำ) — เศษบล็อก
        if fx.replaced != BlockType::Air && !fx.replaced.is_water() {
            let (texture, tint) = match voxel::hotbar_icon_texture(fx.replaced) {
                Some(path) => (asset_server.load(path), 0xFFFF_FFFFu32),
                None => (assets.white.clone(), pack_color(voxel::block_color(fx.replaced))),
            };
            let mut props = EffectProperties::default();
            props.set("tint", tint.into());
            commands.spawn((
                ParticleEffect::new(assets.debris.clone()),
                EffectMaterial { images: vec![texture] },
                props,
                Transform::from_translation(center),
                FxLifetime(Timer::from_seconds(1.5, TimerMode::Once)),
            ));
        }
    }
}

pub fn spawn_explosion_fx(
    mut msgs: MessageReader<ExplosionFx>,
    mut commands: Commands,
    assets: Res<ParticleAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    shock_mat: Res<ShockwaveMaterial>,
    mut waves: ResMut<ActiveShockwaves>,
) {
    for fx in msgs.read() {
        // shockwave: mesh เปล่าก่อน — update_shockwaves เติมหน้าคลื่นทุกเฟรม
        if !fx.rays.is_empty() {
            let max_dist = fx
                .rays
                .iter()
                .map(|s| s.dist0 + s.a.distance(s.b))
                .fold(0.0f32, f32::max);
            let mesh = meshes.add(voxel::MeshBuf::default().into_mesh());
            let entity = commands
                .spawn((Mesh3d(mesh.clone()), MeshMaterial3d(shock_mat.0.clone())))
                .id();
            waves.0.push(Shockwave {
                segs: fx.rays.clone(),
                power: fx.power.max(0.1),
                age: 0.0,
                max_dist,
                speed: if fx.is_nuke { voxel::NUKE_WAVE_SPEED } else { SHOCKWAVE_SPEED },
                entity,
                mesh,
            });
        }

        // แสงวาบสาดรอบจุดระเบิด — แรง/ไกลตามขนาด (power ถูกสเกล N^⅓ มาแล้ว)
        // nuke: ขาวจ้า แรง×20 ไกล×หลายเท่า และค้างนานแบบแฟลชนิวเคลียร์จริง
        let scale = fx.power / 10.0;
        let (color, intensity, range, duration, ambient_boost) = if fx.is_nuke {
            (Color::srgb(1.0, 0.97, 0.9), 40_000_000.0 * scale, 90.0 + 40.0 * scale, 2.5, 25_000.0)
        } else {
            (Color::srgb(1.0, 0.75, 0.45), 2_000_000.0 * scale, 25.0 + 15.0 * scale, 0.45, 0.0)
        };
        commands.spawn((
            PointLight {
                color,
                intensity,
                range,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(fx.center),
            ExplosionFlash { age: 0.0, peak_intensity: intensity, duration, ambient_boost },
        ));

        // ลูกไฟ + ควัน
        commands.spawn((
            ParticleEffect::new(assets.explosion.clone()),
            Transform::from_translation(fx.center),
            FxLifetime(Timer::from_seconds(2.5, TimerMode::Once)),
        ));

        // nuke: เห็ดควัน (คอลัมน์ + หัวเห็ด — อายุยาว ลอยขึ้นช้าๆ)
        if fx.is_nuke {
            commands.spawn((
                ParticleEffect::new(assets.mushroom_column.clone()),
                Transform::from_translation(fx.center),
                FxLifetime(Timer::from_seconds(25.0, TimerMode::Once)),
            ));
            commands.spawn((
                ParticleEffect::new(assets.mushroom_cap.clone()),
                Transform::from_translation(fx.center),
                FxLifetime(Timer::from_seconds(30.0, TimerMode::Once)),
            ));
        }
        // เศษหินกระเด็นเสริมด้วย debris เดิม (tint เทา)
        let mut props = EffectProperties::default();
        props.set("tint", pack_color([0.45, 0.42, 0.40, 1.0]).into());
        commands.spawn((
            ParticleEffect::new(assets.debris.clone()),
            EffectMaterial { images: vec![assets.white.clone()] },
            props,
            Transform::from_translation(fx.center),
            FxLifetime(Timer::from_seconds(1.5, TimerMode::Once)),
        ));
    }
}

/// แสงจ้าเข้าตา: มองไปทางระเบิด + ไม่มีอะไรบัง = จอขาววาบแล้วค่อยๆ หาย
/// อ่าน ExplosionFx แยก reader จาก spawn_explosion_fx (message อ่านได้หลายระบบ)
pub fn trigger_screen_flash(
    mut msgs: MessageReader<ExplosionFx>,
    world: Res<voxel::VoxelWorld>,
    camera: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut flash: ResMut<crate::ui::ScreenFlash>,
) {
    let Ok(cam) = camera.single() else { return };
    for fx in msgs.read() {
        let to_center = fx.center - cam.translation;
        let dist = to_center.length().max(1.0);

        // มองตรง = โดนเต็ม, หันหลัง = เหลือแค่แสงฟุ้งรอบทิศนิดหน่อย
        let dot = cam.forward().dot(to_center / dist).max(0.0);
        let view = 0.15 + 0.85 * dot * dot;

        // มีบล็อกทึบบัง = ไม่โดนแฟลช (หลบหลังกำแพงช่วยได้จริง)
        if !voxel::line_of_sight(&world, cam.translation, fx.center) {
            continue;
        }

        let (base, falloff_dist, decay) = if fx.is_nuke {
            (1.8, 250.0, 0.9) // จ้าเกินสเกล = ขาวสนิทค้าง แล้วพร่านาน ~3-4 วิ
        } else {
            (0.4, 30.0, 4.5)
        };
        let intensity = base * view / (1.0 + dist / falloff_dist);
        if intensity > flash.intensity {
            flash.intensity = intensity;
            flash.decay = decay;
        }
    }
}

/// เดินหน้าคลื่นทุกเฟรม: จุดบนแต่ละ ray ที่ระยะสะสม = SPEED×age วาดเป็น
/// billboard quad ใน mesh เดียว (rebuild ต่อเฟรม — จิ๊บจ๊อยเทียบ chunk mesher)
/// คลื่นเลี้ยวตามการสะท้อนของ ray เอง: ในท่อจะวิ่งเป็นลำแล้วทะลักออกปลายท่อ
pub fn update_shockwaves(
    mut commands: Commands,
    time: Res<Time>,
    mut waves: ResMut<ActiveShockwaves>,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, With<crate::camera::FreeCamera>>,
) {
    if waves.0.is_empty() {
        return;
    }
    let Ok(cam) = camera.single() else { return };
    let cam_right = *cam.right();
    let cam_up = *cam.up();

    waves.0.retain_mut(|wave| {
        wave.age += time.delta_secs();
        let front = wave.age * wave.speed;
        if front > wave.max_dist {
            commands.entity(wave.entity).despawn();
            meshes.remove(&wave.mesh);
            return false;
        }

        let mut buf = voxel::MeshBuf::default();
        for seg in &wave.segs {
            let len = seg.a.distance(seg.b);
            if len <= 1e-4 || front < seg.dist0 || front >= seg.dist0 + len {
                continue;
            }
            let p = seg.a + (seg.b - seg.a) * ((front - seg.dist0) / len);
            let e = (seg.energy / wave.power).clamp(0.0, 1.0);
            // จุดแรงมาก = ใหญ่+สว่าง (สี >1 ให้ bloom), แรงน้อย = เล็กจาง
            let half = 0.22 + 0.28 * e;
            let col = [2.2, 1.4 + 0.8 * e, 0.7 + 0.5 * e, 0.15 + 0.45 * e];
            let r = cam_right * half;
            let u = cam_up * half;
            let vc = buf.positions.len() as u32;
            for corner in [p - r - u, p + r - u, p + r + u, p - r + u] {
                buf.positions.push(corner.to_array());
                buf.normals.push([0.0, 1.0, 0.0]); // unlit — normal ไม่มีผล
                buf.colors.push(col);
            }
            buf.uvs.extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
            buf.indices.extend_from_slice(&[vc, vc + 1, vc + 2, vc, vc + 2, vc + 3]);
        }
        if let Some(mut mesh) = meshes.get_mut(&wave.mesh) {
            *mesh = buf.into_mesh();
        }
        true
    });
}

/// แฟลชระเบิดหรี่ลงแบบ ease-out แล้วดับ + ยก ambient ทั้งฉากช่วง nuke flash
pub fn update_explosion_flash(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut ExplosionFlash, &mut PointLight)>,
    mut ambient: Query<&mut AmbientLight, With<crate::camera::FreeCamera>>,
) {
    let mut boost = 0.0f32;
    for (entity, mut flash, mut light) in &mut query {
        flash.age += time.delta_secs();
        if flash.age >= flash.duration {
            commands.entity(entity).despawn();
            continue;
        }
        let t = 1.0 - flash.age / flash.duration;
        light.intensity = flash.peak_intensity * t * t;
        boost += flash.ambient_boost * t * t;
    }
    if let Ok(mut amb) = ambient.single_mut() {
        // ฐาน 400 ตาม setup_camera (camera.rs) — แฟลชยกชั่วคราวแล้วคืนเอง
        let target = 400.0 + boost;
        if (amb.brightness - target).abs() > 0.5 {
            amb.brightness = target;
        }
    }
}

pub fn despawn_finished_fx(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut FxLifetime)>,
) {
    for (entity, mut life) in &mut query {
        if life.0.tick(time.delta()).is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// เกาะ sparkle ให้ PointLight ที่เพิ่ง spawn — ในเกม PointLight มีแต่ไฟ lamp
/// (refresh_chunk_lamp_lights) จึงใช้ Added<PointLight> ได้ตรงๆ; despawn ของ
/// parent เก็บ child ให้เองอยู่แล้ว ไม่ต้องแก้บัญชี lamp_lights
pub fn attach_lamp_sparkles(
    mut commands: Commands,
    assets: Res<ParticleAssets>,
    // เว้นแฟลชระเบิด — เป็น PointLight ชั่วคราว ไม่ใช่ lamp
    query: Query<(Entity, &PointLight), (Added<PointLight>, Without<ExplosionFlash>)>,
) {
    for (entity, light) in &query {
        let c = light.color.to_linear();
        let mut props = EffectProperties::default();
        props.set("tint", Vec4::new(c.red * 3.0, c.green * 3.0, c.blue * 3.0, 1.0).into());
        let child = commands
            .spawn((
                ParticleEffect::new(assets.sparkle.clone()),
                props,
                Transform::default(),
            ))
            .id();
        commands.entity(entity).add_child(child);
    }
}
