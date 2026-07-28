use bevy::prelude::*;
use crate::voxel::{VoxelWorld, CHUNK_HEIGHT, CHUNK_WIDTH};

pub fn furnace_tick_system(
    mut world: ResMut<VoxelWorld>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let mut furnace_positions = Vec::new();
    
    // Collect all furnace global positions
    for (chunk_pos, chunk) in world.chunks.iter() {
        for (&idx, _) in chunk.furnace_slots.iter() {
            let cx = chunk_pos.x * CHUNK_WIDTH as i32;
            let cz = chunk_pos.y * CHUNK_WIDTH as i32;
            let lx = idx % CHUNK_WIDTH;
            let ly = (idx / CHUNK_WIDTH) % CHUNK_HEIGHT;
            let lz = idx / (CHUNK_WIDTH * CHUNK_HEIGHT);
            let gx = cx + lx as i32;
            let gy = ly as i32;
            let gz = cz + lz as i32;
            furnace_positions.push((gx, gy, gz));
        }
    }

    for (gx, gy, gz) in furnace_positions {
        let mut air_mult: f32 = 1.0;
        
        let neighbors = [
            (gx+1, gy, gz), (gx-1, gy, gz),
            (gx, gy+1, gz), (gx, gy-1, gz),
            (gx, gy, gz+1), (gx, gy, gz-1),
        ];
        
        // Detect Air Multiplier
        for (nx, ny, nz) in neighbors {
            let _b = world.get_block(nx, ny, nz);
            // If we add Bellows later:
            // if b == BlockType::Bellows { air_mult = air_mult.max(1.5); }
        }

        if let Some(furnace) = world.get_furnace_data_mut(gx, gy, gz) {
            if furnace.air_boost_time > 0.0 {
                air_mult = air_mult.max(1.5);
                furnace.air_boost_time -= dt;
                if furnace.air_boost_time < 0.0 { furnace.air_boost_time = 0.0; }
            }
            furnace.air_multiplier = air_mult;
            
            // Consume active fuel
            if furnace.active_fuel_energy > 0.0 {
                // base burn rate: 100 energy per second
                let burn_rate = 100.0 * air_mult;
                furnace.active_fuel_energy -= burn_rate * dt;
                if furnace.active_fuel_energy < 0.0 {
                    furnace.active_fuel_energy = 0.0;
                }
            }
            
            // Try insert new fuel if empty
            if furnace.active_fuel_energy <= 0.0 {
                let mut fuel_empty = false;
                if let Some(ref mut fuel_slot) = furnace.slots[1] {
                    if let Some(props) = fuel_slot.item.get_fuel_properties() {
                        furnace.active_fuel_base_temp = props.base_temp;
                        furnace.active_fuel_energy = props.base_energy;
                        
                        let count = fuel_slot.count.unwrap_or(1);
                        if count > 1 {
                            fuel_slot.count = Some(count - 1);
                        } else {
                            fuel_empty = true;
                        }
                    }
                }
                if fuel_empty {
                    furnace.slots[1] = None;
                }
            }
            
            // Update Temp
            let target_temp = if furnace.active_fuel_energy > 0.0 {
                furnace.active_fuel_base_temp * furnace.air_multiplier
            } else {
                25.0 // Room temp
            };
            
            let lerp_speed = if target_temp > furnace.current_temp { 0.1 } else { 0.02 };
            furnace.current_temp += (target_temp - furnace.current_temp) * lerp_speed * dt;
            
            // Dynamic Smelting (Crucible)
            let current_furnace_temp = furnace.current_temp;
            if let Some(ref mut input_slot) = furnace.slots[0] {
                if let crate::item::Item::Crucible(ref mut crucible_data) = input_slot.item {
                    // Update crucible temperature to match furnace
                    let crucible_target = current_furnace_temp;
                    let crucible_lerp = 0.2; // Sped up from 0.02 so it doesn't take 2 minutes
                    let current_temp_f = crucible_data.temp as f32 + (crucible_data.temp_acc as f32 / 65535.0);
                    let mut new_temp_f = current_temp_f + (crucible_target - current_temp_f) * crucible_lerp * dt;
                    if crucible_target > current_temp_f && new_temp_f - current_temp_f < 10.0 * dt {
                        new_temp_f += 10.0 * dt; // Ensure min 10 degrees per sec heating
                        if new_temp_f > crucible_target { new_temp_f = crucible_target; }
                    }
                    crucible_data.temp = new_temp_f.floor() as u16;
                    crucible_data.temp_acc = ((new_temp_f - new_temp_f.floor()) * 65535.0) as u16;
                    
                    // 1. Calculate dynamic melting point from solid_contents
                    let mut total_mass = 0;
                    let mut weighted_temp_sum: f32 = 0.0;
                    
                    let mut has_solids = false;
                    for s in &crucible_data.solid_contents {
                        if let Some(item) = s {
                            has_solids = true;
                            if let Some(stack) = item.to_stack() {
                                if let Some(comp) = crate::chemistry::get_item_composition(&stack.item) {
                                    let count = stack.count.unwrap_or(1) as u32;
                                    for (element, mass) in comp {
                                        let total_item_mass = mass * count;
                                        total_mass += total_item_mass;
                                        weighted_temp_sum += (total_item_mass as f32) * (element.data().melting_point as f32);
                                    }
                                }
                            }
                        }
                    }
                    
                    if has_solids && total_mass > 0 {
                        let avg_melting_point = (weighted_temp_sum / (total_mass as f32)) as u16;
                        
                        // 2. If the crucible reaches the melting point, melt everything.
                        if crucible_data.temp >= avg_melting_point {
                            for i in 0..9 {
                                if let Some(item) = crucible_data.solid_contents[i] {
                                    if let Some(stack) = item.to_stack() {
                                        if let Some(comp) = crate::chemistry::get_item_composition(&stack.item) {
                                            let count = stack.count.unwrap_or(1) as u32;
                                            for (element, mass) in comp {
                                                crucible_data.liquid_mass[element as usize] += mass * count;
                                            }
                                        }
                                    }
                                }
                                crucible_data.solid_contents[i] = None;
                            }
                        }
                    }
                    
                    // 3. Flux Reaction (Limestone -> Slag)
                    let carbon = crucible_data.liquid_mass[crate::chemistry::Element::Carbon as usize];
                    let impurity = crucible_data.liquid_mass[crate::chemistry::Element::Impurity as usize];
                    if carbon > 0 && impurity > 0 {
                        // 1g of Carbon reacts with 3g of Impurity to form 4g of Slag
                        let reaction_amount = carbon.min(impurity / 3);
                        if reaction_amount > 0 {
                            crucible_data.liquid_mass[crate::chemistry::Element::Carbon as usize] -= reaction_amount;
                            crucible_data.liquid_mass[crate::chemistry::Element::Impurity as usize] -= reaction_amount * 3;
                            crucible_data.liquid_mass[crate::chemistry::Element::Slag as usize] += reaction_amount * 4;
                        }
                    }
                }
            }
        }
    }
}

pub fn crucible_cooling_system(
    mut world: ResMut<VoxelWorld>,
    mut hotbar: ResMut<crate::voxel::Hotbar>,
    settings: Res<crate::GameSettings>,
    weather: Res<crate::weather::Weather>,
    camera: Query<&Transform, With<crate::camera::FreeCamera>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();

    for (pos, crucible) in world.crucibles.iter_mut() {
        let ambient = crate::thermodynamics::outdoor_temperature_celsius(
            pos.as_vec3() + Vec3::splat(0.5),
            &settings,
            &weather,
        );
        cool_crucible(crucible, ambient, dt);
    }

    for (pos, mold) in world.ingot_molds.iter_mut() {
        let ambient = crate::thermodynamics::outdoor_temperature_celsius(
            pos.as_vec3() + Vec3::splat(0.5),
            &settings,
            &weather,
        );
        let current = crate::thermodynamics::unpack_temperature(mold.temp, mold.temp_acc);
        let cooled =
            crate::thermodynamics::cool_toward_ambient(current, ambient, mold.total_mass(), dt);
        crate::thermodynamics::store_temperature(
            cooled,
            &mut mold.temp,
            &mut mold.temp_acc,
        );
    }

    let player_pos = camera
        .iter()
        .next()
        .map_or(Vec3::new(0.0, crate::voxel::SEA_LEVEL as f32, 0.0), |tf| tf.translation);
    let player_ambient =
        crate::thermodynamics::outdoor_temperature_celsius(player_pos, &settings, &weather);
    for stack in hotbar.slots.iter_mut().flatten() {
        if let crate::item::Item::Crucible(ref mut crucible) = stack.item {
            cool_crucible(crucible, player_ambient, dt);
        }
    }
}

fn crucible_mass(crucible: &crate::chemistry::CrucibleData) -> u32 {
    crucible.total_mass()
}

fn cool_crucible(crucible: &mut crate::chemistry::CrucibleData, ambient: f32, dt: f32) {
    let current = crate::thermodynamics::unpack_temperature(crucible.temp, crucible.temp_acc);
    let cooled = crate::thermodynamics::cool_toward_ambient(
        current,
        ambient,
        crucible_mass(crucible),
        dt,
    );
    crate::thermodynamics::store_temperature(
        cooled,
        &mut crucible.temp,
        &mut crucible.temp_acc,
    );
}
