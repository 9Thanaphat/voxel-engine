use bevy::{
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

#[derive(Component)]
pub struct FreeCamera {
    pub speed: f32,
    pub sensitivity: f32,
    pub pitch: f32,
    pub yaw: f32,
}

impl Default for FreeCamera {
    fn default() -> Self {
        Self {
            speed: 50.0,
            sensitivity: 0.002,
            pitch: 0.0,
            yaw: 0.0,
        }
    }
}

pub fn cursor_grab_system(
    mut cursor_query: Query<&mut CursorOptions, With<PrimaryWindow>>,
    btn: Res<ButtonInput<MouseButton>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    if let Ok(mut cursor) = cursor_query.single_mut() {
        // คลิ๊กซ้ายเพื่อล็อคเมาส์
        if btn.just_pressed(MouseButton::Left) {
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        }

        // กด Esc เพื่อปลดล็อคเมาส์
        if key.just_pressed(KeyCode::Escape) {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
    }
}

pub fn camera_movement_system(
    time: Res<Time>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&FreeCamera, &mut Transform)>,
) {
    for (camera, mut transform) in query.iter_mut() {
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
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-2.0, 250.0, -2.0).looking_at(Vec3::new(8.0, 200.0, 8.0), Vec3::Y),
        FreeCamera::default(),
    ));
}

