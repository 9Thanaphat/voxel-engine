use bevy::{
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use crate::voxel::{VoxelWorld, CHUNK_WIDTH};

// ขนาดตัวผู้เล่น (AABB) — ใช้ทั้ง collision และกันวางบล็อกทับตัว
pub const PLAYER_HALF: f32 = 0.3;
pub const PLAYER_HEIGHT: f32 = 1.8;
pub const EYE_HEIGHT: f32 = 1.62;

const GRAVITY: f32 = 28.0;
const JUMP_SPEED: f32 = 8.5;
const WALK_SPEED: f32 = 5.5;

#[derive(Component)]
pub struct FreeCamera {
    pub speed: f32,
    pub sensitivity: f32,
    pub pitch: f32,
    pub yaw: f32,
    /// true = บินอิสระ, false = เดิน (gravity + collision) — สลับด้วย F
    pub fly: bool,
    pub velocity_y: f32,
}

impl Default for FreeCamera {
    fn default() -> Self {
        Self {
            speed: 50.0,
            sensitivity: 0.002,
            pitch: 0.0,
            yaw: 0.0,
            fly: true,
            velocity_y: 0.0,
        }
    }
}

pub fn cursor_grab_system(
    mut cursor_query: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut q_egui: Query<&mut bevy_egui::EguiContext, With<PrimaryWindow>>,
    btn: Res<ButtonInput<MouseButton>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    if let Ok(mut cursor) = cursor_query.single_mut() {
        // คลิ๊กซ้ายเพื่อล็อคเมาส์ (เฉพาะตอนที่ไม่ได้คลิกโดน UI)
        if btn.just_pressed(MouseButton::Left) {
            let mut over_ui = false;
            if let Some(mut egui_ctx) = q_egui.iter_mut().next() {
                if egui_ctx.get_mut().egui_wants_pointer_input() || egui_ctx.get_mut().is_pointer_over_egui() {
                    over_ui = true;
                }
            }
            if !over_ui {
                cursor.grab_mode = CursorGrabMode::Locked;
                cursor.visible = false;
            }
        }

        // กด Esc เพื่อปลดล็อคเมาส์
        if key.just_pressed(KeyCode::Escape) {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
    }
}

/// player AABB (จากตำแหน่งเท้า) ชนบล็อกตันไหม
fn aabb_collides(world: &VoxelWorld, feet: Vec3) -> bool {
    let min = feet - Vec3::new(PLAYER_HALF, 0.0, PLAYER_HALF);
    let max = feet + Vec3::new(PLAYER_HALF, PLAYER_HEIGHT, PLAYER_HALF);

    let x0 = min.x.floor() as i32;
    let x1 = (max.x - 1e-4).floor() as i32;
    let y0 = min.y.floor() as i32;
    let y1 = (max.y - 1e-4).floor() as i32;
    let z0 = min.z.floor() as i32;
    let z1 = (max.z - 1e-4).floor() as i32;

    for bx in x0..=x1 {
        for by in y0..=y1 {
            for bz in z0..=z1 {
                if world.get_block(bx, by, bz).is_solid() {
                    return true;
                }
            }
        }
    }
    false
}

/// ขยับทีละแกนด้วย substep เล็กๆ หยุดเมื่อชน — คืน true ถ้าชน
fn move_axis(world: &VoxelWorld, feet: &mut Vec3, axis: usize, amount: f32) -> bool {
    let mut remaining = amount;
    while remaining.abs() > 1e-6 {
        let step = remaining.clamp(-0.05, 0.05);
        feet[axis] += step;
        if aabb_collides(world, *feet) {
            feet[axis] -= step;
            return true;
        }
        remaining -= step;
    }
    false
}

pub fn camera_movement_system(
    time: Res<Time>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    world: Res<VoxelWorld>,
    mut query: Query<(&mut FreeCamera, &mut Transform)>,
) {
    for (mut camera, mut transform) in query.iter_mut() {
        if keyboard_input.just_pressed(KeyCode::KeyF) {
            camera.fly = !camera.fly;
            camera.velocity_y = 0.0;
        }

        let mut direction = Vec3::ZERO;
        let forward = transform.forward();
        let right = transform.right();

        // เอาเฉพาะระนาบ XZ เพื่อไม่ให้กล้องบินขึ้นฟ้าเวลาเดินไปข้างหน้า
        let flat_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let flat_right = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

        if keyboard_input.pressed(KeyCode::KeyW) {
            direction += flat_forward;
        }
        if keyboard_input.pressed(KeyCode::KeyS) {
            direction -= flat_forward;
        }
        if keyboard_input.pressed(KeyCode::KeyD) {
            direction += flat_right;
        }
        if keyboard_input.pressed(KeyCode::KeyA) {
            direction -= flat_right;
        }

        if camera.fly {
            // บินขึ้น/ลง
            if keyboard_input.pressed(KeyCode::Space) {
                direction += Vec3::Y;
            }
            if keyboard_input.pressed(KeyCode::ShiftLeft) {
                direction -= Vec3::Y;
            }

            if direction != Vec3::ZERO {
                let displacement = direction.normalize() * camera.speed * time.delta_secs();
                transform.translation += displacement;
            }
        } else {
            let mut feet = transform.translation - Vec3::Y * EYE_HEIGHT;

            // chunk ใต้ตัวยังไม่โหลด — หยุดฟิสิกส์ไว้ก่อน กันตกทะลุโลก
            let chunk = IVec2::new(
                feet.x.div_euclid(CHUNK_WIDTH as f32) as i32,
                feet.z.div_euclid(CHUNK_WIDTH as f32) as i32,
            );
            if !world.chunks.contains_key(&chunk) {
                continue;
            }

            let dt = time.delta_secs();
            let horiz = if direction != Vec3::ZERO {
                direction.normalize() * WALK_SPEED * dt
            } else {
                Vec3::ZERO
            };

            camera.velocity_y = (camera.velocity_y - GRAVITY * dt).max(-50.0);

            move_axis(&world, &mut feet, 0, horiz.x);
            move_axis(&world, &mut feet, 2, horiz.z);
            let hit_y = move_axis(&world, &mut feet, 1, camera.velocity_y * dt);
            if hit_y {
                camera.velocity_y = 0.0;
            }

            // ยืนอยู่บนพื้นไหม (มีบล็อกชิดใต้เท้า)
            let grounded = {
                let mut probe = feet;
                probe.y -= 0.02;
                aabb_collides(&world, probe)
            };
            if grounded && keyboard_input.pressed(KeyCode::Space) {
                camera.velocity_y = JUMP_SPEED;
            }

            transform.translation = feet + Vec3::Y * EYE_HEIGHT;
        }
    }
}

pub fn camera_look_system(
    cursor_query: Query<&CursorOptions, With<PrimaryWindow>>,
    mut mouse_events: MessageReader<MouseMotion>,
    mut query: Query<(&mut FreeCamera, &mut Transform)>,
) {
    if let Ok(cursor) = cursor_query.single() {
        // ถ้าเมาส์ไม่ได้ถูกล็อคอยู่ ให้ข้ามไปไม่ต้องหมุนกล้อง
        if cursor.grab_mode == CursorGrabMode::None {
            return;
        }
    } else {
        return;
    }

    let mut delta = Vec2::ZERO;
    for event in mouse_events.read() {
        delta += event.delta;
    }

    if delta != Vec2::ZERO {
        for (mut camera, mut transform) in query.iter_mut() {
            camera.yaw -= delta.x * camera.sensitivity;
            camera.pitch -= delta.y * camera.sensitivity;

            // ล็อคมุมก้ม/เงย ไม่ให้กล้องตีลังกา
            camera.pitch = camera.pitch.clamp(
                -std::f32::consts::FRAC_PI_2 + 0.01,
                std::f32::consts::FRAC_PI_2 - 0.01,
            );

            // อัปเดต rotation
            transform.rotation = Quat::from_axis_angle(Vec3::Y, camera.yaw)
                * Quat::from_axis_angle(Vec3::X, camera.pitch);
        }
    }
}

pub fn setup_camera(mut commands: Commands) {
    let transform =
        Transform::from_xyz(-2.0, 250.0, -2.0).looking_at(Vec3::new(8.0, 200.0, 8.0), Vec3::Y);

    // ดึง yaw/pitch จาก rotation เริ่มต้น ให้ตรงกับสูตรใน camera_look_system
    // (Quat::from_axis_angle(Y, yaw) * Quat::from_axis_angle(X, pitch))
    // ไม่งั้นขยับเมาส์ครั้งแรกกล้องจะสะบัดกลับไปที่ yaw=0, pitch=0
    let (yaw, pitch, _roll) = transform.rotation.to_euler(EulerRot::YXZ);

    commands.spawn((
        Camera3d::default(),
        transform,
        // SSAO ไม่รองรับ MSAA (DepthPrepass/NormalPrepass ถูกใส่ให้เองผ่าน required components)
        bevy::pbr::ScreenSpaceAmbientOcclusion::default(),
        Msaa::Off,
        // ให้บล็อกเรืองแสง (emissive > 1.0) ฟุ้งแสง — Hdr ถูกใส่ให้อัตโนมัติ
        bevy::post_process::bloom::Bloom::NATURAL,
        // เพิ่ม ambient ให้ด้านที่ไม่โดนแดดไม่ดำสนิท (override GlobalAmbientLight)
        AmbientLight {
            color: Color::WHITE,
            brightness: 400.0,
            ..default()
        },
        FreeCamera {
            yaw,
            pitch,
            ..default()
        },
    ));
}
