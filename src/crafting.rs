use crate::item::{Item, MaterialType};
use crate::voxel::{BlockType, ItemStack};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ingredient {
    Exact(Item),
    AnyLog,
    AnyPickaxeHead,
}

impl Ingredient {
    fn matches(self, item: Item) -> bool {
        match self {
            Self::Exact(required) => required == item,
            Self::AnyLog => matches!(
                item,
                Item::Block(BlockType::OakWood | BlockType::MapleLog | BlockType::SpruceLog)
            ),
            Self::AnyPickaxeHead => matches!(item, Item::PickaxeHead(_)),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Exact(item) => item.name(),
            Self::AnyLog => "Any Log".to_string(),
            Self::AnyPickaxeHead => "Pickaxe Head".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeOutput {
    Exact(Item, u32),
    PickaxeFromHead,
}

impl RecipeOutput {
    pub fn label(self) -> String {
        match self {
            Self::Exact(item, _) => item.name(),
            Self::PickaxeFromHead => "Pickaxe".to_string(),
        }
    }

    pub fn count(self) -> u32 {
        match self {
            Self::Exact(_, count) => count,
            Self::PickaxeFromHead => 1,
        }
    }
}

impl Recipe {
    pub fn label(&self) -> String {
        let inputs = self
            .inputs
            .iter()
            .map(|(ingredient, count)| format!("{} x{}", ingredient.label(), count))
            .collect::<Vec<_>>()
            .join(" + ");
        format!("{inputs} -> {} x{}", self.output.label(), self.output.count())
    }

    pub fn label_for_inventory(&self, inventory: &[Option<ItemStack>]) -> String {
        let head = inventory.iter().flatten().find_map(|slot| match slot.item {
            Item::PickaxeHead(data) => Some(data),
            _ => None,
        });
        let inputs = self
            .inputs
            .iter()
            .map(|(ingredient, count)| {
                let name = match (ingredient, head) {
                    (Ingredient::AnyPickaxeHead, Some(data)) => {
                        Item::PickaxeHead(data).name()
                    }
                    _ => ingredient.label(),
                };
                format!("{name} x{count}")
            })
            .collect::<Vec<_>>()
            .join(" + ");
        let output = match (self.output, head) {
            (RecipeOutput::PickaxeFromHead, Some(data)) => Item::CraftedPickaxe(data).name(),
            _ => self.output.label(),
        };
        format!("{inputs} -> {output} x{}", self.output.count())
    }
}

pub struct Recipe {
    pub id: usize,
    pub inputs: Vec<(Ingredient, u32)>,
    pub output: RecipeOutput,
}

pub fn get_recipes() -> Vec<Recipe> {
    vec![
        Recipe {
            id: 0,
            inputs: vec![(Ingredient::AnyLog, 1)],
            output: RecipeOutput::Exact(Item::Material(MaterialType::Stick), 4),
        },
        Recipe {
            id: 1,
            inputs: vec![
                (Ingredient::AnyPickaxeHead, 1),
                (Ingredient::Exact(Item::Material(MaterialType::Stick)), 1),
            ],
            output: RecipeOutput::PickaxeFromHead,
        },
    ]
}

pub fn can_craft(recipe: &Recipe, inventory: &[Option<ItemStack>]) -> bool {
    recipe.inputs.iter().all(|(ingredient, required)| {
        inventory
            .iter()
            .flatten()
            .filter(|slot| ingredient.matches(slot.item))
            .map(|slot| slot.count.unwrap_or(1))
            .sum::<u32>()
            >= *required
    })
}

/// Consumes one recipe and returns its stateful output. For a pickaxe this
/// carries the cast head's composition and quality into the finished tool.
pub fn craft(recipe: &Recipe, inventory: &mut [Option<ItemStack>]) -> Option<ItemStack> {
    if !can_craft(recipe, inventory) {
        return None;
    }
    let head = inventory.iter().flatten().find_map(|slot| match slot.item {
        Item::PickaxeHead(data) => Some(data),
        _ => None,
    });

    for (ingredient, required) in &recipe.inputs {
        let mut remaining = *required;
        for slot in inventory.iter_mut() {
            if remaining == 0 {
                break;
            }
            let Some(stack) = slot else { continue };
            if !ingredient.matches(stack.item) {
                continue;
            }
            let count = stack.count.unwrap_or(1);
            if count > remaining {
                stack.count = Some(count - remaining);
                remaining = 0;
            } else {
                remaining -= count;
                *slot = None;
            }
        }
    }

    match recipe.output {
        RecipeOutput::Exact(item, count) => Some(ItemStack { item, count: Some(count) }),
        RecipeOutput::PickaxeFromHead => Some(ItemStack {
            item: Item::CraftedPickaxe(head?),
            count: Some(1),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chemistry::{CastIngotData, CastIngotKind};

    #[test]
    fn pickaxe_recipe_preserves_head_properties() {
        let head = CastIngotData {
            mass: 1_000,
            composition: [800, 200, 0, 0, 0, 0, 0, 0],
            quality_permille: 875,
            kind: CastIngotKind::Bronze,
        };
        let mut inventory = vec![
            Some(ItemStack { item: Item::PickaxeHead(head), count: Some(1) }),
            Some(ItemStack { item: Item::Material(MaterialType::Stick), count: Some(1) }),
        ];
        let recipe = get_recipes().remove(1);
        assert_eq!(
            recipe.label_for_inventory(&inventory),
            "Bronze Pickaxe Head x1 + Stick x1 -> Bronze Pickaxe x1"
        );
        let output = craft(&recipe, &mut inventory).unwrap();
        assert_eq!(output.item, Item::CraftedPickaxe(head));
        assert!(inventory.iter().all(Option::is_none));
    }
}
