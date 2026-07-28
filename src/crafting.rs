use crate::item::Item;
use crate::voxel::{BlockType, ItemStack};

pub struct Recipe {
    pub id: usize,
    pub inputs: Vec<(Item, u32)>,
    pub output: Item,
    pub output_count: u32,
}

pub fn get_recipes() -> Vec<Recipe> {
    vec![
        Recipe {
            id: 0,
            inputs: vec![(Item::Block(BlockType::OakWood), 1)],
            output: Item::Material(crate::item::MaterialType::Stick),
            output_count: 4,
        }
    ]
}

pub fn can_craft(recipe: &Recipe, inventory: &[Option<ItemStack>]) -> bool {
    for (req_item, req_count) in &recipe.inputs {
        let mut found = 0;
        for slot in inventory.iter().flatten() {
            if &slot.item == req_item {
                found += slot.count.unwrap_or(1);
            }
        }
        if found < *req_count {
            return false;
        }
    }
    true
}

pub fn consume_ingredients(recipe: &Recipe, inventory: &mut [Option<ItemStack>]) {
    for (req_item, req_count) in &recipe.inputs {
        let mut remaining = *req_count;
        for slot in inventory.iter_mut() {
            if remaining == 0 {
                break;
            }
            if let Some(s) = slot {
                if &s.item == req_item {
                    let count = s.count.unwrap_or(1);
                    if count > remaining {
                        s.count = Some(count - remaining);
                        remaining = 0;
                    } else {
                        remaining -= count;
                        *slot = None; // Slot empty
                    }
                }
            }
        }
    }
}
