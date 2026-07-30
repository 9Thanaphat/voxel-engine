use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FluidKind {
    Water,
    Lava,
    SulfuricAcid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FluidProperties {
    pub tick_interval: f32,
    pub viscosity: u8,
    pub damage_per_second: f32,
    pub emission: [u8; 3],
}

pub fn fluid_properties(kind: FluidKind) -> FluidProperties {
    match kind {
        FluidKind::Water => FluidProperties { tick_interval: 0.1, viscosity: 1, damage_per_second: 0.0, emission: [0, 0, 0] },
        FluidKind::Lava => FluidProperties { tick_interval: 0.5, viscosity: 4, damage_per_second: 30.0, emission: [15, 6, 1] },
        FluidKind::SulfuricAcid => FluidProperties { tick_interval: 0.25, viscosity: 2, damage_per_second: 12.0, emission: [0, 0, 0] },
    }
}

pub fn fluid_kind(block: BlockType) -> Option<FluidKind> {
    if block.is_water() {
        Some(FluidKind::Water)
    } else if block.is_lava() {
        Some(FluidKind::Lava)
    } else if block.is_acid() {
        Some(FluidKind::SulfuricAcid)
    } else {
        None
    }
}

fn volume(block: BlockType) -> u8 {
    match block {
        BlockType::LavaSource | BlockType::Lava8
        | BlockType::SulfuricAcidSource | BlockType::Acid8 => 8,
        BlockType::Lava7 | BlockType::Acid7 => 7,
        BlockType::Lava6 | BlockType::Acid6 => 6,
        BlockType::Lava5 | BlockType::Acid5 => 5,
        BlockType::Lava4 | BlockType::Acid4 => 4,
        BlockType::Lava3 | BlockType::Acid3 => 3,
        BlockType::Lava2 | BlockType::Acid2 => 2,
        BlockType::Lava1 | BlockType::Acid1 => 1,
        _ => 0,
    }
}

fn block_for(kind: FluidKind, volume: u8) -> BlockType {
    match (kind, volume.min(8)) {
        (_, 0) => BlockType::Air,
        (FluidKind::Lava, 1) => BlockType::Lava1,
        (FluidKind::Lava, 2) => BlockType::Lava2,
        (FluidKind::Lava, 3) => BlockType::Lava3,
        (FluidKind::Lava, 4) => BlockType::Lava4,
        (FluidKind::Lava, 5) => BlockType::Lava5,
        (FluidKind::Lava, 6) => BlockType::Lava6,
        (FluidKind::Lava, 7) => BlockType::Lava7,
        (FluidKind::Lava, _) => BlockType::Lava8,
        (FluidKind::SulfuricAcid, 1) => BlockType::Acid1,
        (FluidKind::SulfuricAcid, 2) => BlockType::Acid2,
        (FluidKind::SulfuricAcid, 3) => BlockType::Acid3,
        (FluidKind::SulfuricAcid, 4) => BlockType::Acid4,
        (FluidKind::SulfuricAcid, 5) => BlockType::Acid5,
        (FluidKind::SulfuricAcid, 6) => BlockType::Acid6,
        (FluidKind::SulfuricAcid, 7) => BlockType::Acid7,
        (FluidKind::SulfuricAcid, _) => BlockType::Acid8,
        (FluidKind::Water, volume) => vol_to_block(volume),
    }
}

pub fn take_one_unit(block: BlockType) -> BlockType {
    let Some(kind @ (FluidKind::Lava | FluidKind::SulfuricAcid)) = fluid_kind(block) else {
        return block;
    };
    block_for(kind, volume(block).saturating_sub(1))
}

fn write_block(
    world: &mut VoxelWorld,
    pos: IVec3,
    block: BlockType,
    is_host: bool,
    net_out: &mut crate::network::PendingNetEdits,
    remesh: &mut HashSet<IVec2>,
    modified_chunks: &mut HashSet<IVec2>,
) -> bool {
    if world.get_block(pos.x, pos.y, pos.z) == block {
        return true;
    }
    if !world.set_block(pos.x, pos.y, pos.z, block) {
        return false;
    }
    if is_host {
        net_out.0.push_back((None, crate::network::BlockEdit::SetBlock {
            pos: pos.to_array(),
            block: block as u8,
        }));
    }
    remesh.extend(edit_affected_chunks(pos));
    modified_chunks.insert(IVec2::new(
        pos.x.div_euclid(CHUNK_WIDTH as i32),
        pos.z.div_euclid(CHUNK_WIDTH as i32),
    ));
    true
}

/// Slow host-authoritative lava/acid simulation. Water keeps its specialised
/// pool solver; reactive fluids use this bounded cellular pass.
pub fn reactive_fluid_system(
    mut active: ResMut<ActiveReactiveFluids>,
    mut world: ResMut<VoxelWorld>,
    mut commands: Commands,
    mut mp: MeshingParams,
    net_server: Option<Res<bevy_renet::RenetServer>>,
    mut net_out: ResMut<crate::network::PendingNetEdits>,
    time: Res<Time>,
    mut tick_accum: Local<f32>,
    mut tick_index: Local<u64>,
) {
    *tick_accum += time.delta_secs();
    if *tick_accum < 0.25 || active.0.is_empty() {
        return;
    }
    *tick_accum = 0.0;
    *tick_index = tick_index.wrapping_add(1);
    let is_host = net_server.is_some();

    let mut cells: Vec<_> = active.0.drain().collect();
    cells.sort_unstable_by_key(|p| (p.y, p.x, p.z));
    if cells.len() > 256 {
        active.0.extend(cells.split_off(256));
    }

    let mut next = HashSet::new();
    let mut remesh = HashSet::new();
    let mut modified_chunks = HashSet::new();
    let horizontal = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];

    for pos in cells {
        let current = world.get_block(pos.x, pos.y, pos.z);
        let Some(kind @ (FluidKind::Lava | FluidKind::SulfuricAcid)) = fluid_kind(current) else {
            continue;
        };
        if kind == FluidKind::Lava && *tick_index % 2 != 0 {
            next.insert(pos);
            continue;
        }
        let source = matches!(current, BlockType::LavaSource | BlockType::SulfuricAcidSource);
        let mut amount = volume(current);
        let mut replaced = false;

        for dir in [IVec3::Y, IVec3::NEG_Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
            let np = pos + dir;
            let neighbor = world.get_block(np.x, np.y, np.z);
            match (kind, fluid_kind(neighbor), neighbor) {
                (FluidKind::Lava, Some(FluidKind::Water), _) => {
                    write_block(&mut world, pos, BlockType::Obsidian, is_host, &mut net_out, &mut remesh, &mut modified_chunks);
                    replaced = true;
                    break;
                }
                (FluidKind::SulfuricAcid, Some(FluidKind::Water), _) => {
                    write_block(&mut world, pos, vol_to_block(amount), is_host, &mut net_out, &mut remesh, &mut modified_chunks);
                    replaced = true;
                    break;
                }
                (FluidKind::SulfuricAcid, _, BlockType::Limestone) if amount > 0 => {
                    if write_block(&mut world, np, BlockType::Gypsum, is_host, &mut net_out, &mut remesh, &mut modified_chunks) && !source {
                        amount = amount.saturating_sub(1);
                    }
                }
                (FluidKind::SulfuricAcid, _, BlockType::TallGrass | BlockType::Leaves
                    | BlockType::MapleLeaves | BlockType::SpruceLeaves
                    | BlockType::OakWood | BlockType::MapleLog | BlockType::SpruceLog) => {
                    if write_block(&mut world, np, BlockType::Air, is_host, &mut net_out, &mut remesh, &mut modified_chunks) && !source {
                        amount = amount.saturating_sub(1);
                    }
                }
                (FluidKind::Lava, Some(FluidKind::SulfuricAcid), _) => {
                    write_block(&mut world, np, BlockType::Air, is_host, &mut net_out, &mut remesh, &mut modified_chunks);
                }
                _ => {}
            }
        }
        if replaced {
            continue;
        }

        let below = pos + IVec3::NEG_Y;
        let below_block = world.get_block(below.x, below.y, below.z);
        let below_amount = if fluid_kind(below_block) == Some(kind) { volume(below_block) } else { 0 };
        let mut moved = false;
        if (below_block == BlockType::Air || fluid_kind(below_block) == Some(kind)) && below_amount < 8 {
            let transfer = if source { 8 - below_amount } else { amount.min(8 - below_amount) };
            if transfer > 0 && write_block(
                &mut world, below, block_for(kind, below_amount + transfer), is_host,
                &mut net_out, &mut remesh, &mut modified_chunks,
            ) {
                if !source { amount -= transfer; }
                next.insert(below);
                moved = true;
            }
        }

        if amount > 1 {
            let start = ((pos.x as i64 * 31 + pos.z as i64 * 17 + *tick_index as i64)
                .unsigned_abs() as usize) % horizontal.len();
            for offset in 0..horizontal.len() {
                let np = pos + horizontal[(start + offset) % horizontal.len()];
                let neighbor = world.get_block(np.x, np.y, np.z);
                let neighbor_amount = if fluid_kind(neighbor) == Some(kind) { volume(neighbor) } else { 0 };
                if (neighbor == BlockType::Air || fluid_kind(neighbor) == Some(kind))
                    && neighbor_amount + 1 < amount
                    && write_block(
                        &mut world, np, block_for(kind, neighbor_amount + 1), is_host,
                        &mut net_out, &mut remesh, &mut modified_chunks,
                    )
                {
                    if !source { amount -= 1; }
                    next.insert(np);
                    moved = true;
                    break;
                }
            }
        }

        let desired = if source { current } else { block_for(kind, amount) };
        write_block(&mut world, pos, desired, is_host, &mut net_out, &mut remesh, &mut modified_chunks);
        if source || moved {
            next.insert(pos);
            for dir in [IVec3::Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
                next.insert(pos + dir);
            }
        }
    }
    active.0.extend(next);

    if !remesh.is_empty() {
        remesh_chunks(&mut commands, &mut world, &mut mp, None, remesh);
    }
    if net_server.is_none() {
        for chunk in modified_chunks {
            save_loaded_chunk(&world, chunk);
        }
    }
}

pub fn fluid_hazard_system(
    time: Res<Time>,
    world: Res<VoxelWorld>,
    settings: Res<crate::GameSettings>,
    mut player: Query<(&Transform, &mut crate::camera::PlayerStats), With<crate::camera::FreeCamera>>,
) {
    if settings.game_mode != crate::GameMode::Survival {
        return;
    }
    for (transform, mut stats) in &mut player {
        let feet = transform.translation - Vec3::Y * 1.5;
        let positions = [
            feet.floor().as_ivec3(),
            (feet + Vec3::Y).floor().as_ivec3(),
        ];
        let damage = positions.into_iter().fold(0.0f32, |damage, p| {
            let block = world.get_block(p.x, p.y, p.z);
            let dps = fluid_kind(block)
                .map(fluid_properties)
                .map_or(0.0, |properties| properties.damage_per_second);
            let magma = if block == BlockType::MagmaRock { 6.0 } else { 0.0 };
            damage.max(dps.max(magma))
        });
        stats.health = (stats.health - damage * time.delta_secs()).max(0.0);
    }
}

pub fn volcano_lifecycle_system(
    time: Res<Time>,
    settings: Res<crate::GameSettings>,
    camera: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut registry: ResMut<crate::volcanism::VolcanoRegistry>,
    mut pending_events: ResMut<crate::network::PendingVolcanoEvents>,
    mut active_fluids: ResMut<ActiveReactiveFluids>,
    mut world: ResMut<VoxelWorld>,
    mut commands: Commands,
    mut mp: MeshingParams,
    net_server: Option<Res<bevy_renet::RenetServer>>,
    mut net_out: ResMut<crate::network::PendingNetEdits>,
    mut fx_writer: MessageWriter<crate::particles::ExplosionFx>,
    mut net_fx: ResMut<crate::network::PendingNetFx>,
) {
    let Ok(transform) = camera.single() else { return };
    let Some(descriptor) = crate::volcanism::volcano_near(
        settings.noise.seed,
        transform.translation.x as f64,
        transform.translation.z as f64,
    ) else {
        return;
    };

    let runtime = registry.active.entry(descriptor.id).or_insert(
        crate::volcanism::VolcanoRuntime {
            id: descriptor.id,
            state: crate::volcanism::VolcanoState::Dormant,
            seconds_in_state: 0.0,
            pulse_seconds: 0.0,
        },
    );
    runtime.seconds_in_state += time.delta_secs();
    runtime.pulse_seconds += time.delta_secs();

    let duration = match runtime.state {
        crate::volcanism::VolcanoState::Dormant => 600.0 + (runtime.id % 600) as f32,
        crate::volcanism::VolcanoState::Unrest => 90.0,
        crate::volcanism::VolcanoState::Erupting => 120.0,
        crate::volcanism::VolcanoState::Cooling => 180.0,
    };
    if runtime.seconds_in_state >= duration {
        runtime.state = match runtime.state {
            crate::volcanism::VolcanoState::Dormant => crate::volcanism::VolcanoState::Unrest,
            crate::volcanism::VolcanoState::Unrest => crate::volcanism::VolcanoState::Erupting,
            crate::volcanism::VolcanoState::Erupting => crate::volcanism::VolcanoState::Cooling,
            crate::volcanism::VolcanoState::Cooling => crate::volcanism::VolcanoState::Dormant,
        };
        runtime.seconds_in_state = 0.0;
        runtime.pulse_seconds = 0.0;
        pending_events.0.push(crate::network::VolcanoEventWire {
            id: runtime.id,
            state: runtime.state,
            center: [descriptor.center.x, descriptor.center.y],
        });
    }

    if runtime.state != crate::volcanism::VolcanoState::Erupting || runtime.pulse_seconds < 1.0 {
        return;
    }
    runtime.pulse_seconds = 0.0;
    let pulse = runtime.seconds_in_state.floor() as i32;
    let is_host = net_server.is_some();
    let center_x = descriptor.center.x.round() as i32;
    let center_z = descriptor.center.y.round() as i32;
    let mut remesh = HashSet::new();
    let mut modified_chunks = HashSet::new();
    let mut highest_y = 0;

    // Wake the generated conduit and crater lake. Source cells then feed the
    // bounded reactive-fluid solver.
    for dz in -6..=6 {
        for dx in -6..=6 {
            if dx * dx + dz * dz > 36 {
                continue;
            }
            let x = center_x + dx;
            let z = center_z + dz;
            for y in (1..CHUNK_HEIGHT as i32).rev() {
                let block = world.get_block(x, y, z);
                if block != BlockType::Air {
                    highest_y = highest_y.max(y);
                    if block.is_lava() {
                        active_fluids.0.insert(IVec3::new(x, y, z));
                    } else if dx * dx + dz * dz <= 9 {
                        write_block(
                            &mut world,
                            IVec3::new(x, y + 1, z),
                            BlockType::LavaSource,
                            is_host,
                            &mut net_out,
                            &mut remesh,
                            &mut modified_chunks,
                        );
                        active_fluids.0.insert(IVec3::new(x, y + 1, z));
                    }
                    break;
                }
            }
        }
    }

    // Persistent ash fall in deterministic down-wind arcs.
    for i in 0..8 {
        let angle = (pulse as f32 * 0.37 + i as f32 * 0.79) as f64;
        let distance = descriptor.radius * (0.35 + (i as f64 / 8.0) * 0.55);
        let x = center_x + (angle.cos() * distance) as i32;
        let z = center_z + (angle.sin() * distance) as i32;
        for y in (1..CHUNK_HEIGHT as i32 - 1).rev() {
            let block = world.get_block(x, y, z);
            if block.is_solid() {
                let above = IVec3::new(x, y + 1, z);
                if world.get_block(above.x, above.y, above.z) == BlockType::Air {
                    write_block(
                        &mut world,
                        above,
                        BlockType::VolcanicAsh,
                        is_host,
                        &mut net_out,
                        &mut remesh,
                        &mut modified_chunks,
                    );
                }
                break;
            }
        }
    }

    if pulse % 5 == 0 && highest_y > 0 {
        let center = Vec3::new(center_x as f32 + 0.5, highest_y as f32 + 2.0, center_z as f32 + 0.5);
        fx_writer.write(crate::particles::ExplosionFx {
            center,
            rays: Vec::new(),
            power: 16.0,
            is_nuke: false,
        });
        if is_host {
            net_fx.0.push(crate::network::ExplosionWire::new(center, &[], 16.0, false));
        }
    }

    if !remesh.is_empty() {
        remesh_chunks(&mut commands, &mut world, &mut mp, None, remesh);
    }
    if !is_host {
        for chunk in modified_chunks {
            save_loaded_chunk(&world, chunk);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reactive_levels_round_trip() {
        for level in 1..=8 {
            assert_eq!(volume(block_for(FluidKind::Lava, level)), level);
            assert_eq!(volume(block_for(FluidKind::SulfuricAcid, level)), level);
        }
    }
}
