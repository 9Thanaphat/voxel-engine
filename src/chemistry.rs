use crate::item::{Item, MaterialType, SimpleItem};
use crate::voxel::BlockType;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum Element {
    Copper,
    Tin,
    Zinc,
    Iron,
    Carbon,
    Impurity, // Things that create slag
    Slag,     // Skimmed waste
}

pub struct ElementData {
    pub melting_point: u16, // Celsius
    pub name: &'static str,
}

impl Element {
    pub fn data(&self) -> ElementData {
        match self {
            Element::Copper => ElementData { melting_point: 1085, name: "Copper" },
            Element::Tin => ElementData { melting_point: 232, name: "Tin" },
            Element::Zinc => ElementData { melting_point: 420, name: "Zinc" },
            Element::Iron => ElementData { melting_point: 1538, name: "Iron" },
            Element::Carbon => ElementData { melting_point: 3550, name: "Carbon" },
            Element::Impurity => ElementData { melting_point: 800, name: "Impurity" },
            Element::Slag => ElementData { melting_point: 1200, name: "Slag" },
        }
    }
}

/// Returns the elemental composition of an item in grams (u32).
/// This assumes a standard "ore block" weighs about 1000g for calculation simplicity.
pub fn get_item_composition(item: &Item) -> Option<Vec<(Element, u32)>> {
    match item {
        Item::Block(BlockType::CopperOre) | Item::Material(MaterialType::Copper) => Some(vec![(Element::Copper, 800), (Element::Impurity, 200)]),
        Item::Block(BlockType::IronOre) | Item::Material(MaterialType::Iron) => Some(vec![(Element::Iron, 850), (Element::Impurity, 150)]),
        Item::Material(MaterialType::Coal) => Some(vec![(Element::Carbon, 950), (Element::Impurity, 50)]),
        Item::Material(MaterialType::CopperIngot) => Some(vec![(Element::Copper, 1000)]),
        Item::Material(MaterialType::IronIngot) => Some(vec![(Element::Iron, 1000)]),
        // Limestone (CaCO3) acts as a Flux, we represent it as Carbon + Flux property in the smelting logic
        // But for now, let's just make it react with Impurity directly in the smelting logic.
        // We'll give it a special composition or handle it manually.
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct CrucibleData {
    pub temp: u16, // Celsius
    pub temp_acc: u16, // Fractional part
    pub solid_contents: [Option<SimpleItem>; 9], // Solid items thrown in
    pub liquid_mass: [u32; 8], // Mass of each element in grams. Index corresponds to Element enum as usize
}

impl Default for CrucibleData {
    fn default() -> Self {
        Self {
            temp: 25,
            temp_acc: 0,
            solid_contents: [None; 9],
            liquid_mass: [0; 8],
        }
    }
}
