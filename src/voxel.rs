use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use bevy::{
    prelude::*,
    render::{mesh::Indices, render_resource::PrimitiveTopology},
    asset::RenderAssetUsages,
};
use std::collections::HashMap;
use noise::{NoiseFn, Perlin};

#[derive(Component)]
pub struct Block;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    Air,
    Dirt,
    Grass,
    Stone,
    Water,
}

pub const CHUNK_WIDTH: usize = 16;
pub const CHUNK_HEIGHT: usize = 512;
pub const CHUNK_VOLUME: usize = CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH;
pub const SEA_LEVEL: usize = 200;

pub struct ChunkData {
    pub blocks: Box<[BlockType; CHUNK_VOLUME]>,
}

impl ChunkData {
    pub fn get_index(x: usize, y: usize, z: usize) -> usize {
        x + y * CHUNK_WIDTH + z * CHUNK_WIDTH * CHUNK_HEIGHT
    }
}

#[derive(Resource, Default)]
pub struct VoxelWorld {
    pub chunks: HashMap<IVec2, ChunkData>,
    pub generated_chunks: HashMap<IVec2, Entity>,
}

impl VoxelWorld {
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> BlockType {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return BlockType::Air;
        }

        let chunk_x = x.div_euclid(CHUNK_WIDTH as i32);
        let chunk_z = z.div_euclid(CHUNK_WIDTH as i32);

        if let Some(chunk) = self.chunks.get(&IVec2::new(chunk_x, chunk_z)) {
            let local_x = x.rem_euclid(CHUNK_WIDTH as i32) as usize;
            let local_y = y as usize;
            let local_z = z.rem_euclid(CHUNK_WIDTH as i32) as usize;
            chunk.blocks[ChunkData::get_index(local_x, local_y, local_z)]
        } else {
            BlockType::Air
        }
    }
}

pub const CUBE_POSITIONS: [[[f32; 3]; 4]; 6] = [
    // Top (Y+)
    [[0., 1., 1.], [1., 1., 1.], [1., 1., 0.], [0., 1., 0.]],
    // Bottom (Y-)
    [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]],
    // Right (X+)
    [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]],
    // Left (X-)
    [[0., 0., 1.], [0., 1., 1.], [0., 1., 0.], [0., 0., 0.]],
    // Forward (Z+)
    [[1., 0., 1.], [1., 1., 1.], [0., 1., 1.], [0., 0., 1.]],
    // Back (Z-)
    [[0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]],
];

pub const CUBE_NORMALS: [[f32; 3]; 6] = [
    [0., 1., 0.],
    [0., -1., 0.],
    [1., 0., 0.],
    [-1., 0., 0.],
    [0., 0., 1.],
    [0., 0., -1.],
];

// --------------------------------------------------------
// Async Chunk Generation
// --------------------------------------------------------

pub struct ChunkMeshData {
    pub chunk_pos: IVec2,
    pub blocks: Box<[BlockType; CHUNK_VOLUME]>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

use std::sync::Mutex;

#[derive(Resource)]
pub struct ChunkGenerator {
    pub sender: Mutex<Sender<ChunkMeshData>>,
    pub receiver: Mutex<Receiver<ChunkMeshData>>,
    pub generating: HashMap<IVec2, bool>,
}

impl Default for ChunkGenerator {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender: Mutex::new(sender),
            receiver: Mutex::new(receiver),
            generating: HashMap::new(),
        }
    }
}

pub fn spawn_chunk_generation_task(chunk_pos: IVec2, sender: Sender<ChunkMeshData>) {
    thread::spawn(move || {
        let mut blocks = Box::new([BlockType::Air; CHUNK_VOLUME]);
        let perlin = Perlin::new(1);
        
        let cx = chunk_pos.x;
        let cz = chunk_pos.y;

        for z in 0..CHUNK_WIDTH {
            for x in 0..CHUNK_WIDTH {
                let world_x = cx as f64 * CHUNK_WIDTH as f64 + x as f64;
                let world_z = cz as f64 * CHUNK_WIDTH as f64 + z as f64;
                
                let noise_val = perlin.get([world_x * 0.015, world_z * 0.015]);
                let height = (SEA_LEVEL as f64 + noise_val * 40.0) as usize;

                for y in 0..CHUNK_HEIGHT {
                    let idx = ChunkData::get_index(x, y, z);
                    if y < height - 3 {
                        blocks[idx] = BlockType::Stone;
                    } else if y < height {
                        blocks[idx] = BlockType::Dirt;
                    } else if y == height {
                        blocks[idx] = BlockType::Grass;
                    } else if y <= SEA_LEVEL {
                        blocks[idx] = BlockType::Water;
                    }
                }
            }
        }

        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut normals: Vec<[f32; 3]> = Vec::new();
        let mut colors: Vec<[f32; 4]> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        let faces_offsets = [
            (0, 1, 0, 0),   // Top
            (0, -1, 0, 1),  // Bottom
            (1, 0, 0, 2),   // Right
            (-1, 0, 0, 3),  // Left
            (0, 0, 1, 4),   // Forward
            (0, 0, -1, 5),  // Back
        ];

        let mut vertex_count = 0;

        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_WIDTH {
                for x in 0..CHUNK_WIDTH {
                    let block = blocks[ChunkData::get_index(x, y, z)];
                    if block == BlockType::Air { continue; }

                    let color = match block {
                        BlockType::Dirt => [0.4, 0.2, 0.0, 1.0],
                        BlockType::Grass => [0.2, 0.6, 0.2, 1.0],
                        BlockType::Stone => [0.5, 0.5, 0.5, 1.0],
                        BlockType::Water => [0.1, 0.3, 0.8, 1.0],
                        _ => [1.0, 1.0, 1.0, 1.0],
                    };

                    for (dx, dy, dz, face_id) in faces_offsets.iter() {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        let nz = z as i32 + dz;

                        let neighbor = if nx < 0 || nx >= CHUNK_WIDTH as i32 || ny < 0 || ny >= CHUNK_HEIGHT as i32 || nz < 0 || nz >= CHUNK_WIDTH as i32 {
                            BlockType::Air // Border block, draw it
                        } else {
                            blocks[ChunkData::get_index(nx as usize, ny as usize, nz as usize)]
                        };

                        let mut should_draw = false;
                        if neighbor == BlockType::Air {
                            should_draw = true;
                        } else if neighbor == BlockType::Water && block != BlockType::Water {
                            should_draw = true;
                        }

                        if should_draw {
                            let face_positions = CUBE_POSITIONS[*face_id];
                            let face_normal = CUBE_NORMALS[*face_id];

                            for i in 0..4 {
                                positions.push([
                                    face_positions[i][0] + x as f32,
                                    face_positions[i][1] + y as f32,
                                    face_positions[i][2] + z as f32,
                                ]);
                                normals.push(face_normal);
                                colors.push(color);
                            }

                            indices.push(vertex_count);
                            indices.push(vertex_count + 1);
                            indices.push(vertex_count + 2);
                            indices.push(vertex_count);
                            indices.push(vertex_count + 2);
                            indices.push(vertex_count + 3);

                            vertex_count += 4;
                        }
                    }
                }
            }
        }

        let mesh_data = ChunkMeshData {
            chunk_pos,
            blocks,
            positions,
            normals,
            colors,
            indices,
        };

        // Ignore send errors if receiver is dropped
        let _ = sender.send(mesh_data);
    });
}


#[derive(Resource)]
pub struct ChunkMaterial(pub Handle<StandardMaterial>);

pub fn setup_voxel(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        ..default()
    });
    
    commands.insert_resource(ChunkMaterial(material));
    commands.insert_resource(VoxelWorld::default());
    commands.insert_resource(ChunkGenerator::default());

    // Light
    commands.spawn((
        PointLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, SEA_LEVEL as f32 + 50.0, 4.0),
    ));
}

pub fn world_generation_system(
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    world: Res<VoxelWorld>,
    mut generator: ResMut<ChunkGenerator>,
) {
    let Some(camera_transform) = camera_query.iter().next() else { return };
    let cam_pos = camera_transform.translation;

    let center_chunk_x = cam_pos.x.div_euclid(CHUNK_WIDTH as f32) as i32;
    let center_chunk_z = cam_pos.z.div_euclid(CHUNK_WIDTH as f32) as i32;

    let render_distance = 8; 

    for dx in -render_distance..=render_distance {
        for dz in -render_distance..=render_distance {
            let cx = center_chunk_x + dx;
            let cz = center_chunk_z + dz;
            let chunk_pos = IVec2::new(cx, cz);

            if !world.generated_chunks.contains_key(&chunk_pos) && !generator.generating.contains_key(&chunk_pos) {
                // Mark as generating
                generator.generating.insert(chunk_pos, true);
                let sender = generator.sender.lock().unwrap().clone();
                spawn_chunk_generation_task(chunk_pos, sender);
            }
        }
    }
}

pub fn process_generated_chunks_system(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    mut generator: ResMut<ChunkGenerator>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunk_material: Res<ChunkMaterial>,
) {
    // Process all chunks that have finished generating this frame
    let mut received = Vec::new();
    {
        let receiver = generator.receiver.lock().unwrap();
        while let Ok(mesh_data) = receiver.try_recv() {
            received.push(mesh_data);
        }
    }

    for mesh_data in received {
        let chunk_pos = mesh_data.chunk_pos;
        
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, mesh_data.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, mesh_data.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, mesh_data.colors);
        mesh.insert_indices(Indices::U32(mesh_data.indices));

        world.chunks.insert(chunk_pos, ChunkData { blocks: mesh_data.blocks });
        
        let entity = commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(chunk_material.0.clone()),
            Transform::from_xyz((chunk_pos.x * CHUNK_WIDTH as i32) as f32, 0.0, (chunk_pos.y * CHUNK_WIDTH as i32) as f32),
            Block,
        )).id();
        
        world.generated_chunks.insert(chunk_pos, entity);
        generator.generating.remove(&chunk_pos);
    }
}

pub fn chunk_unloading_system(
    mut commands: Commands,
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut world: ResMut<VoxelWorld>,
) {
    let Some(camera_transform) = camera_query.iter().next() else { return };
    let cam_pos = camera_transform.translation;

    let center_chunk_x = cam_pos.x.div_euclid(CHUNK_WIDTH as f32) as i32;
    let center_chunk_z = cam_pos.z.div_euclid(CHUNK_WIDTH as f32) as i32;
    
    // Unload chunks that are outside render distance + 2
    let unload_distance = 8 + 2;

    let mut to_unload = Vec::new();

    for (&chunk_pos, &entity) in world.generated_chunks.iter() {
        if (chunk_pos.x - center_chunk_x).abs() > unload_distance || (chunk_pos.y - center_chunk_z).abs() > unload_distance {
            to_unload.push(chunk_pos);
            commands.entity(entity).despawn();
        }
    }

    for pos in to_unload {
        world.generated_chunks.remove(&pos);
        world.chunks.remove(&pos);
    }
}

// --------------------------------------------------------
// Raycast
// --------------------------------------------------------

pub fn voxel_raycast_system(
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    world: Res<VoxelWorld>,
    mut gizmos: Gizmos,
    mut block_id_text: Query<&mut Text, With<crate::ui::BlockIdText>>,
) {
    let Some(camera_transform) = camera_query.iter().next() else { return };
    let origin = camera_transform.translation;
    let dir = camera_transform.forward().normalize();

    let max_dist = 6.0;
    
    let mut map_x = origin.x.floor() as i32;
    let mut map_y = origin.y.floor() as i32;
    let mut map_z = origin.z.floor() as i32;

    let delta_dist_x = if dir.x == 0.0 { f32::INFINITY } else { (1.0_f32 / dir.x).abs() };
    let delta_dist_y = if dir.y == 0.0 { f32::INFINITY } else { (1.0_f32 / dir.y).abs() };
    let delta_dist_z = if dir.z == 0.0 { f32::INFINITY } else { (1.0_f32 / dir.z).abs() };

    let step_x = if dir.x < 0.0 { -1 } else { 1 };
    let step_y = if dir.y < 0.0 { -1 } else { 1 };
    let step_z = if dir.z < 0.0 { -1 } else { 1 };

    let mut side_dist_x = if dir.x < 0.0 {
        (origin.x - map_x as f32) * delta_dist_x
    } else {
        (map_x as f32 + 1.0 - origin.x) * delta_dist_x
    };
    let mut side_dist_y = if dir.y < 0.0 {
        (origin.y - map_y as f32) * delta_dist_y
    } else {
        (map_y as f32 + 1.0 - origin.y) * delta_dist_y
    };
    let mut side_dist_z = if dir.z < 0.0 {
        (origin.z - map_z as f32) * delta_dist_z
    } else {
        (map_z as f32 + 1.0 - origin.z) * delta_dist_z
    };

    let mut hit = false;
    let mut side = 0; // 0 for x, 1 for y, 2 for z

    for _ in 0..50 {
        let dist = Vec3::new(map_x as f32 + 0.5, map_y as f32 + 0.5, map_z as f32 + 0.5).distance(origin);
        if dist > max_dist {
            break;
        }

        let block = world.get_block(map_x, map_y, map_z);
        if block != BlockType::Air {
            hit = true;
            break;
        }

        if side_dist_x < side_dist_y {
            if side_dist_x < side_dist_z {
                side_dist_x += delta_dist_x;
                map_x += step_x;
                side = 0;
            } else {
                side_dist_z += delta_dist_z;
                map_z += step_z;
                side = 2;
            }
        } else {
            if side_dist_y < side_dist_z {
                side_dist_y += delta_dist_y;
                map_y += step_y;
                side = 1;
            } else {
                side_dist_z += delta_dist_z;
                map_z += step_z;
                side = 2;
            }
        }
    }

    if let Some(mut text) = block_id_text.iter_mut().next() {
        if hit {
            let block = world.get_block(map_x, map_y, map_z);
            let name = match block {
                BlockType::Dirt => "Dirt",
                BlockType::Grass => "Grass",
                BlockType::Stone => "Stone",
                BlockType::Water => "Water",
                _ => "Unknown",
            };
            text.0 = format!("Block: {}", name);
            
            let mut normal = Vec3::ZERO;
            if side == 0 {
                normal.x = -step_x as f32;
            } else if side == 1 {
                normal.y = -step_y as f32;
            } else {
                normal.z = -step_z as f32;
            }
            
            let mut face_idx = 0;
            for (i, n) in CUBE_NORMALS.iter().enumerate() {
                if Vec3::from_array(*n) == normal {
                    face_idx = i;
                    break;
                }
            }

            let positions = CUBE_POSITIONS[face_idx];
            let offset = normal * 0.01;
            let block_pos = Vec3::new(map_x as f32, map_y as f32, map_z as f32);
            
            let p0 = block_pos + Vec3::from_array(positions[0]) + offset;
            let p1 = block_pos + Vec3::from_array(positions[1]) + offset;
            let p2 = block_pos + Vec3::from_array(positions[2]) + offset;
            let p3 = block_pos + Vec3::from_array(positions[3]) + offset;
            
            let color = Color::BLACK;
            gizmos.line(p0, p1, color);
            gizmos.line(p1, p2, color);
            gizmos.line(p2, p3, color);
            gizmos.line(p3, p0, color);

        } else {
            text.0 = "Block: None".to_string();
        }
    }
}
