use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

/// ประเภทของขั้วไฟฟ้า
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortType {
    Input,
    Output,
    Bidirectional,
}

/// ปลายทางที่สายไฟไปเชื่อมต่อ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionTarget {
    pub block_pos: IVec3, // พิกัด Global ของบล็อกที่เป็นอุปกรณ์
    pub target_local_pos: IVec3, // พิกัด sub-voxel ภายในบล็อกนั้น (0..15)
}

/// ข้อมูลของขั้วไฟฟ้า 1 ขั้ว
#[derive(Debug, Clone)]
pub struct Terminal {
    pub local_pos: IVec3,
    pub port_type: PortType,
    pub connected_to: Vec<ConnectionTarget>,
    pub is_powered: bool,
}

impl Terminal {
    pub fn new(local_pos: IVec3, port_type: PortType) -> Self {
        Self {
            local_pos,
            port_type,
            connected_to: Vec::new(),
            is_powered: false,
        }
    }
}

/// ประเภทของ Logic ภายในอุปกรณ์
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceLogic {
    Source, // จ่ายไฟเสมอ (เช่น เครื่องกำเนิดไฟฟ้า, สวิตช์ที่เปิดอยู่)
    Switch { is_on: bool }, // จ่ายไฟก็ต่อเมื่อ is_on == true
    Relay,  // สายไฟหรือตัวนำไฟ (ถ้า Input มีไฟ Output จะมีไฟ)
    Lamp,   // อุปกรณ์รับไฟ (เปล่งแสงหรือทำงาน)
}

/// ข้อมูลอุปกรณ์ไฟฟ้าทั้งหมดในโลก (เก็บโดยใช้พิกัด Global IVec3 เป็น Key)
#[derive(Resource, Default)]
pub struct ElectricalGrid {
    pub devices: HashMap<IVec3, ElectricalDevice>,
}

pub struct ElectricalDevice {
    pub terminals: HashMap<IVec3, Terminal>,
    pub logic_type: DeviceLogic,
}

/// เก็บสถานะการลากสายไฟด้วยเมาส์
#[derive(Resource, Default)]
pub struct WiringState {
    pub is_dragging: bool,
    pub start_port: Option<ConnectionTarget>,
}

/// Event แจ้งเตือนให้ระบบประมวลผลไฟฟ้าใหม่ (รันเมื่อต่อสายไฟ หรือสับสวิตช์)
#[derive(Message)]
pub struct PowerTopologyChanged;

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct WireGizmo;

fn setup_wire_gizmos(mut config_store: ResMut<GizmoConfigStore>) {
    let (config, _) = config_store.config_mut::<WireGizmo>();
    config.line.width = 4.0;
}

/// ปลั๊กอินของระบบไฟฟ้า
pub struct ElectricityPlugin;

impl Plugin for ElectricityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ElectricalGrid>()
           .init_resource::<WiringState>()
           .add_message::<PowerTopologyChanged>()
           .init_gizmo_group::<WireGizmo>()
           .add_systems(Startup, setup_wire_gizmos)
           .add_systems(Update, (
               wiring_interaction_system,
               instant_power_update_system,
               draw_wires_system,
               block_fx_listener_system,
           ));
    }
}

/// ระบบประมวลผลไฟฟ้าแบบ Instant + ป้องกัน Infinite Loop (BFS)
pub fn instant_power_update_system(
    mut events: MessageReader<PowerTopologyChanged>,
    mut grid: ResMut<ElectricalGrid>,
    mut world: ResMut<crate::voxel::VoxelWorld>,
    mut commands: Commands,
    mut mp: crate::voxel::MeshingParams,
    campfire_assets: Res<crate::voxel::CampfireAssets>,
) {
    if events.is_empty() {
        return;
    }
    events.clear();

    // 1. รีเซ็ตสถานะ: ดับไฟขั้วทั้งหมดก่อนเริ่มคำนวณใหม่
    for device in grid.devices.values_mut() {
        for terminal in device.terminals.values_mut() {
            terminal.is_powered = false;
        }
    }

    // 2. เติมไฟจาก Source ทุกตัว
    let mut queue = std::collections::VecDeque::new();
    let mut visited = std::collections::HashSet::new();

    for (&block_pos, device) in grid.devices.iter() {
        let is_active_source = device.logic_type == DeviceLogic::Source || matches!(device.logic_type, DeviceLogic::Switch { is_on: true });
        if is_active_source {
            for terminal in device.terminals.values() {
                if terminal.port_type == PortType::Output || terminal.port_type == PortType::Bidirectional {
                    let target = ConnectionTarget {
                        block_pos,
                        target_local_pos: terminal.local_pos,
                    };
                    queue.push_back(target.clone());
                    visited.insert(target);
                }
            }
        }
    }

    // 3. ปล่อยกระแสไฟ (BFS) แผ่กระจายออกไปตามสายไฟ
    while let Some(current) = queue.pop_front() {
        let Some(device) = grid.devices.get_mut(&current.block_pos) else { continue };

        let external_hops;
        // อัปเดตขั้วปัจจุบันให้มีไฟ
        if let Some(terminal) = device.terminals.get_mut(&current.target_local_pos) {
            terminal.is_powered = true;
            external_hops = terminal.connected_to.clone();
        } else {
            continue;
        }

        let mut internal_outputs_to_trigger = Vec::new();
        
        // --- Logic ภายในอุปกรณ์ ---
        // ถ้าเป็น Relay หรือ Source กระแสไฟสามารถวิ่งทะลุข้ามขั้วอื่นๆ ในบล็อกเดียวกันได้
        if device.logic_type == DeviceLogic::Relay || device.logic_type == DeviceLogic::Source {
            for (other_pos, other_term) in device.terminals.iter() {
                // หาขั้วอื่นที่ไม่ใช่ขั้วที่เพิ่งวิ่งเข้ามา และเป็นขั้วที่จ่ายไฟออกได้
                if *other_pos != current.target_local_pos && 
                   (other_term.port_type == PortType::Output || other_term.port_type == PortType::Bidirectional) {
                    internal_outputs_to_trigger.push(*other_pos);
                }
            }
        }

        // เปิดไฟให้ขั้วภายใน และส่งกระแสไฟวิ่งไปตามสายไฟภายนอกที่เชื่อมอยู่
        for out_pos in internal_outputs_to_trigger {
            if let Some(out_term) = device.terminals.get_mut(&out_pos) {
                out_term.is_powered = true;
                // นำปลายทางที่ต่ออยู่กับ Output นี้ โยนเข้า Queue
                for next_hop in &out_term.connected_to {
                    if visited.insert(*next_hop) {
                        queue.push_back(*next_hop);
                    }
                }
            }
        }

        // ส่งกระแสไฟจากขั้วปัจจุบันวิ่งไปตามสายไฟภายนอกโดยตรง
        for next_hop in &external_hops {
            if visited.insert(*next_hop) {
                queue.push_back(*next_hop);
            }
        }
    }

    // 4. อัปเดต Block Visuals (เช่น Lamp ติดไฟ)
    for (&pos, device) in grid.devices.iter() {
        if device.logic_type == DeviceLogic::Lamp {
            let is_powered = device.terminals.values().any(|t| t.is_powered);
            let current = world.get_block(pos.x, pos.y, pos.z);
            use crate::voxel::BlockType;
            let mut changed = false;
            if is_powered && current == BlockType::SmartLamp {
                world.set_block(pos.x, pos.y, pos.z, BlockType::SmartLampOn);
                changed = true;
            } else if !is_powered && current == BlockType::SmartLampOn {
                world.set_block(pos.x, pos.y, pos.z, BlockType::SmartLamp);
                changed = true;
            }
            if changed {
                let affected = crate::voxel::edit_affected_chunks(pos);
                crate::voxel::remesh_chunks(&mut commands, &mut world, &mut mp, affected.clone());
                for chunk_pos in affected {
                    crate::voxel::refresh_chunk_lamp_lights(&mut commands, &mut world, chunk_pos);
                    crate::voxel::refresh_chunk_campfire_models(&mut commands, &mut world, chunk_pos, &campfire_assets);
                }
            }
        }
    }
}

pub fn wiring_interaction_system(
    mut wiring_state: ResMut<WiringState>,
    mut grid: ResMut<ElectricalGrid>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    target: Res<crate::voxel::TargetedBlock>, // ดึงเป้าเล็งมาจากระบบ Voxel เดิม
    interaction_mode: Res<crate::voxel::InteractionMode>,
    mut fx_writer: MessageWriter<PowerTopologyChanged>,
) {
    // จำกัดให้ลากสายไฟได้เฉพาะตอนอยู่ในโหมด Wiring
    if *interaction_mode != crate::voxel::InteractionMode::Wiring {
        if wiring_state.is_dragging {
            wiring_state.is_dragging = false;
            wiring_state.start_port = None;
        }
        return;
    }

    if !mouse_input.just_pressed(MouseButton::Right) {
        return;
    }

    // ถ้าไม่ได้เล็งบล็อกอะไรอยู่เลย ให้ปล่อยสายไฟหลุดมือ
    let Some(hit) = target.0 else { 
        wiring_state.is_dragging = false;
        wiring_state.start_port = None;
        return; 
    };

    let block_pos = hit.pos;

    // 1. ตรวจสอบว่าบล็อกที่เล็งอยู่นั้นมีอุปกรณ์ไฟฟ้าหรือไม่
    // และถ้ามี ให้ดึงพิกัดขั้วแรกสุดมาเลย (ผู้เล่นจะได้ไม่ต้องเล็งเป้าให้ตรงขั้ว 100%)
    let target_local_pos = if let Some(device) = grid.devices.get(&block_pos) {
        if let Some((&local_pos, _)) = device.terminals.iter().next() {
            Some(local_pos)
        } else {
            None
        }
    } else {
        None
    };

    let Some(local_pos) = target_local_pos else {
        // ถ้ายกเลิกการลาก (คลิกมั่วไปโดนบล็อกธรรมดาที่ไม่มีขั้ว)
        if wiring_state.is_dragging {
            println!("Wiring cancelled.");
        }
        wiring_state.is_dragging = false;
        wiring_state.start_port = None;
        return;
    };

    let clicked_target = ConnectionTarget {
        block_pos,
        target_local_pos: local_pos,
    };

    // 2. State Machine การลากสาย
    if !wiring_state.is_dragging {
        // [State 1] เริ่มลากสายไฟจากขั้วนี้
        wiring_state.is_dragging = true;
        wiring_state.start_port = Some(clicked_target);
        println!("Started wiring from {:?}", clicked_target);
    } else {
        // [State 2] จบการลากสายไฟ (เชื่อมต่อ 2 ขั้ว)
        if let Some(start_target) = wiring_state.start_port {
            if start_target != clicked_target {
                // ผูกสายฝั่ง A ไป B
                if let Some(dev_a) = grid.devices.get_mut(&start_target.block_pos) {
                    if let Some(term_a) = dev_a.terminals.get_mut(&start_target.target_local_pos) {
                        term_a.connected_to.push(clicked_target);
                    }
                }
                
                // ผูกสายฝั่ง B กลับมา A (Undirected Graph)
                if let Some(dev_b) = grid.devices.get_mut(&clicked_target.block_pos) {
                    if let Some(term_b) = dev_b.terminals.get_mut(&clicked_target.target_local_pos) {
                        term_b.connected_to.push(start_target);
                    }
                }

                println!("Successfully connected wire!");
                
                // กระตุ้นให้ BFS ประมวลผลกราฟไฟฟ้าใหม่ทันที
                fx_writer.write(PowerTopologyChanged);
            }
        }
        
        
        // ต่อเสร็จแล้ว วางมือ
        wiring_state.is_dragging = false;
        wiring_state.start_port = None;
    }
}

// ==========================================
// Phase 3: Visuals (Rendering Wires)
// ==========================================

/// แปลงพิกัด ConnectionTarget ให้เป็นพิกัด 3D ในโลกจริง (World Space)
fn get_terminal_world_pos(target: &ConnectionTarget) -> Vec3 {
    let block = Vec3::new(
        target.block_pos.x as f32,
        target.block_pos.y as f32,
        target.block_pos.z as f32,
    );
    let sub = Vec3::new(
        target.target_local_pos.x as f32,
        target.target_local_pos.y as f32,
        target.target_local_pos.z as f32,
    );
    // Voxel ปกติขนาด 1.0, sub-voxel ขนาด 1/16 และเราชี้ไปที่กึ่งกลางของ sub-voxel
    block + (sub * (1.0 / 16.0)) + Vec3::splat(0.5 / 16.0)
}

/// วาดสายไฟตกท้องช้างสมจริง (พาราโบลาจำลอง Catenary Curve)
fn draw_catenary(gizmos: &mut Gizmos<WireGizmo>, p1: Vec3, p2: Vec3, color: Color) {
    let segments = 15;
    let dx = p2.x - p1.x;
    let dz = p2.z - p1.z;
    let horizontal_dist = (dx * dx + dz * dz).sqrt();
    
    // ความหย่อนแปรผันตามระยะทาง (ห่างมาก ยิ่งตกท้องช้างมาก)
    let sag = horizontal_dist * 0.15; 
    
    let mut prev_pos = p1;
    for i in 1..=segments {
        let t = i as f32 / segments as f32;
        let lerp_x = p1.x + dx * t;
        let lerp_z = p1.z + dz * t;
        
        // สมการโค้งตกท้องช้าง: y = 4 * sag * t * (t - 1)
        let drop = 4.0 * sag * t * (t - 1.0); 
        let lerp_y = p1.y + (p2.y - p1.y) * t + drop;
        
        let current_pos = Vec3::new(lerp_x, lerp_y, lerp_z);
        gizmos.line(prev_pos, current_pos, color);
        prev_pos = current_pos;
    }
}

pub fn draw_wires_system(
    mut gizmos: Gizmos<WireGizmo>,
    grid: Res<ElectricalGrid>,
    wiring_state: Res<WiringState>,
    target: Res<crate::voxel::TargetedBlock>,
    camera_query: Query<&GlobalTransform, With<crate::camera::FreeCamera>>,
) {
    // 1. วาดสายไฟทั้งหมดในระบบ
    // ใช้ HashSet กันการวาดสายไฟเส้นเดิมซ้ำ 2 รอบ (เพราะเป็นกราฟแบบ Undirected)
    let mut drawn_wires: HashSet<(ConnectionTarget, ConnectionTarget)> = HashSet::new();

    for (&block_pos, device) in grid.devices.iter() {
        for (&local_pos, terminal) in device.terminals.iter() {
            let start_target = ConnectionTarget { block_pos, target_local_pos: local_pos };
            let start_world = get_terminal_world_pos(&start_target);
            
            // สีส้มจ้าถ้ามีไฟ, สีเทาเข้มถ้าไม่มีไฟ
            let color = if terminal.is_powered { 
                Color::srgb(1.5, 0.7, 0.1) 
            } else { 
                Color::srgb(0.15, 0.15, 0.15) 
            };

            for next_hop in &terminal.connected_to {
                // สร้าง Key ที่ไม่สนใจลำดับ (A->B หรือ B->A ก็คือเส้นเดียวกัน)
                let mut pair = (start_target, *next_hop);
                
                // เปรียบเทียบเพื่อสลับที่ให้ Key เหมือนกันเสมอ
                if pair.0.block_pos.x > pair.1.block_pos.x || 
                   (pair.0.block_pos.x == pair.1.block_pos.x && pair.0.target_local_pos.x > pair.1.target_local_pos.x) {
                    pair = (pair.1, pair.0);
                }
                
                // ถ้ายัดลง HashSet ไม่ได้แปลว่าวาดไปแล้ว
                if drawn_wires.insert(pair) {
                    let end_world = get_terminal_world_pos(next_hop);
                    draw_catenary(&mut gizmos, start_world, end_world, color);
                }
            }
        }
    }

    // 2. วาดเส้นประ (Preview) ขณะที่ผู้เล่นกำลังลากสายไฟ
    if wiring_state.is_dragging {
        if let Some(start_target) = wiring_state.start_port {
            let start_world = get_terminal_world_pos(&start_target);
            
            let end_world = if let Some(hit) = target.0 {
                // ถ้ายกเป้าชี้ไปที่บล็อก ให้ยึดพิกัดเป้าเล็งเป็นปลายสาย
                let t_block = hit.pos;
                let t_sub = hit.sub_pos.unwrap_or(IVec3::ZERO);
                get_terminal_world_pos(&ConnectionTarget { block_pos: t_block, target_local_pos: t_sub })
            } else {
                // ถ้าชี้ขึ้นฟ้า เอาพิกัดลอยๆ ห่างกล้องไป 2 เมตร
                if let Some(cam_transform) = camera_query.iter().next() {
                    cam_transform.translation() + cam_transform.forward() * 2.0
                } else {
                    start_world
                }
            };

            // วาดสายไฟกำลังลาก (สีเหลืองอ่อน)
            draw_catenary(&mut gizmos, start_world, end_world, Color::srgba(1.0, 1.0, 0.0, 0.5));
        }
    }
}

pub fn block_fx_listener_system(
    mut events: MessageReader<crate::particles::BlockFx>,
    mut grid: ResMut<ElectricalGrid>,
    mut topo_writer: MessageWriter<PowerTopologyChanged>,
) {
    use crate::voxel::BlockType::*;
    let mut changed = false;
    for fx in events.read() {
        let pos = fx.pos;
        let is_now = matches!(fx.placed, SwitchOff | SwitchOn | SmartLamp | SmartLampOn);
        let was_electrical = matches!(fx.replaced, SwitchOff | SwitchOn | SmartLamp | SmartLampOn);
        
        if !is_now {
            if grid.devices.remove(&pos).is_some() {
                // ต้องไปทำลายสายของชาวบ้านที่ลากมาหาบล็อกนี้ด้วย
                for device in grid.devices.values_mut() {
                    for terminal in device.terminals.values_mut() {
                        terminal.connected_to.retain(|t| t.block_pos != pos);
                    }
                }
                changed = true;
            }
        } else {
            let logic_type = match fx.placed {
                SwitchOff => DeviceLogic::Switch { is_on: false },
                SwitchOn => DeviceLogic::Switch { is_on: true },
                SmartLamp | SmartLampOn => DeviceLogic::Lamp,
                _ => unreachable!(),
            };
            
            if was_electrical && grid.devices.contains_key(&pos) {
                let device = grid.devices.get_mut(&pos).unwrap();
                if device.logic_type != logic_type {
                    device.logic_type = logic_type;
                    changed = true;
                }
            } else {
                let mut device = ElectricalDevice {
                    terminals: HashMap::new(),
                    logic_type,
                };
                let port_type = match fx.placed {
                    SwitchOff | SwitchOn => PortType::Output,
                    SmartLamp | SmartLampOn => PortType::Input,
                    _ => unreachable!(),
                };
                device.terminals.insert(IVec3::new(8, 8, 8), Terminal::new(IVec3::new(8, 8, 8), port_type));
                grid.devices.insert(pos, device);
                changed = true;
            }
        }
    }
    
    if changed {
        topo_writer.write(PowerTopologyChanged);
    }
}
