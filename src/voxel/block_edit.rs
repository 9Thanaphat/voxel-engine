use bevy::prelude::IVec3;

use super::{edit_affected_chunks, BlockType, ItemStack, VoxelWorld};

fn decode_contents(
    contents: &[Option<crate::item::WireItemStack>],
) -> Option<Vec<Option<ItemStack>>> {
    contents
        .iter()
        .map(|item| match item {
            Some(item) => item.to_stack().map(Some),
            None => Some(None),
        })
        .collect()
}

pub(super) fn place_container(
    world: &mut VoxelWorld,
    pos: [i32; 3],
    block: u8,
    contents: &[Option<crate::item::WireItemStack>],
    crucible_data: Option<crate::chemistry::CrucibleData>,
) -> Option<IVec3> {
    let [x, y, z] = pos;
    let block = BlockType::from_u8(block);
    let decoded_contents = match block {
        BlockType::Chest if contents.len() <= 27 && crucible_data.is_none() => {
            decode_contents(contents)
        }
        BlockType::Furnace if contents.len() <= 2 && crucible_data.is_none() => {
            decode_contents(contents)
        }
        BlockType::Crucible if contents.is_empty() => Some(Vec::new()),
        _ => None,
    }?;

    if !world.set_block(x, y, z, block) {
        return None;
    }

    let pos = IVec3::new(x, y, z);
    match block {
        BlockType::Chest => {
            for (slot, item) in decoded_contents.into_iter().enumerate() {
                world.set_chest_slot(x, y, z, slot, item);
            }
        }
        BlockType::Furnace => {
            for (slot, item) in decoded_contents.into_iter().enumerate() {
                world.set_furnace_slot(x, y, z, slot, item);
            }
        }
        BlockType::Crucible => {
            world.crucibles.insert(pos, crucible_data.unwrap_or_default());
        }
        _ => unreachable!("container type was validated before mutating the world"),
    }

    for chunk_pos in edit_affected_chunks(pos) {
        if let Some(chunk) = world.chunks.get_mut(&chunk_pos) {
            chunk.light_dirty = true;
        }
    }
    Some(pos)
}

pub(super) fn set_container_slot(
    world: &mut VoxelWorld,
    pos: [i32; 3],
    slot: u8,
    item: Option<crate::item::WireItemStack>,
) -> Option<IVec3> {
    let [x, y, z] = pos;
    let stack = item.and_then(crate::item::WireItemStack::to_stack);
    match world.get_block(x, y, z) {
        BlockType::Chest if slot < 27 => {
            world.set_chest_slot(x, y, z, slot as usize, stack);
        }
        BlockType::Furnace if slot < 2 => {
            world.set_furnace_slot(x, y, z, slot as usize, stack);
        }
        BlockType::Crucible if slot < 9 => {
            let data = world.crucibles.entry(IVec3::new(x, y, z)).or_default();
            let old = data.solid_contents[slot as usize];
            data.solid_contents[slot as usize] = stack.map(crate::item::SimpleItem::from_stack);
            if data.total_mass() > crate::chemistry::CRUCIBLE_CAPACITY_GRAMS {
                data.solid_contents[slot as usize] = old;
                return None;
            }
        }
        _ => return None,
    }
    Some(IVec3::new(x, y, z))
}

pub(super) fn add_furnace_air(world: &mut VoxelWorld, pos: [i32; 3]) -> Option<IVec3> {
    if let Some(furnace) = world.get_furnace_data_mut(pos[0], pos[1], pos[2]) {
        furnace.air_boost_time = (furnace.air_boost_time + 2.0).min(10.0);
    }
    None
}
