use crate::item::{Item, MaterialType, SimpleItem};
use crate::voxel::BlockType;

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
    pub const ALL: [Self; 7] = [
        Self::Copper,
        Self::Tin,
        Self::Zinc,
        Self::Iron,
        Self::Carbon,
        Self::Impurity,
        Self::Slag,
    ];

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

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

/// Canonical smelting conversion. One inventory item is one 1,000 g furnace charge.
///
/// Current raw yields:
/// - Copper ore: 800 g copper + 200 g impurity
/// - Iron ore: 850 g iron + 150 g impurity
///
/// Keeping this conversion here ensures furnace logic, mold volume, and UI use the
/// same unit. Ingot molds hold one 1,000 g charge.
pub fn get_item_composition(item: &Item) -> Option<Vec<(Element, u32)>> {
    match item {
        Item::Block(BlockType::CopperOre) | Item::Material(MaterialType::Copper) => Some(vec![(Element::Copper, 800), (Element::Impurity, 200)]),
        Item::Block(BlockType::IronOre) | Item::Material(MaterialType::Iron) => Some(vec![(Element::Iron, 850), (Element::Impurity, 150)]),
        Item::Material(MaterialType::Coal) => Some(vec![(Element::Carbon, 950), (Element::Impurity, 50)]),
        Item::Material(MaterialType::CopperIngot) => Some(vec![(Element::Copper, 1000)]),
        Item::Material(MaterialType::IronIngot) => Some(vec![(Element::Iron, 1000)]),
        Item::Material(MaterialType::BronzeIngot) => {
            Some(vec![(Element::Copper, 880), (Element::Tin, 120)])
        }
        Item::Material(MaterialType::BrassIngot) => {
            Some(vec![(Element::Copper, 700), (Element::Zinc, 300)])
        }
        Item::Material(MaterialType::SteelIngot) => {
            Some(vec![(Element::Iron, 990), (Element::Carbon, 10)])
        }
        Item::Material(MaterialType::SlagAlloyIngot) => {
            Some(vec![(Element::Copper, 800), (Element::Impurity, 200)])
        }
        Item::CastIngot(data) => Some(
            data.composition
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, mass)| {
                    (mass > 0).then(|| Element::from_index(index).map(|element| (element, mass))).flatten()
                })
                .collect(),
        ),
        // Limestone (CaCO3) acts as a Flux, we represent it as Carbon + Flux property in the smelting logic
        // But for now, let's just make it react with Impurity directly in the smelting logic.
        // We'll give it a special composition or handle it manually.
        _ => None,
    }
}

pub fn composition_total_mass(item: &Item) -> Option<u32> {
    get_item_composition(item).map(|composition| {
        composition.into_iter().map(|(_, mass)| mass).sum()
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct CrucibleData {
    pub temp: u16, // Celsius
    pub temp_acc: u16, // Fractional part
    pub solid_contents: [Option<SimpleItem>; 9], // Solid items thrown in
    pub liquid_mass: [u32; 8], // Mass of each element in grams. Index corresponds to Element enum as usize
}

pub const INGOT_MOLD_CAPACITY_GRAMS: u32 = 1_000;
pub const CRUCIBLE_CAPACITY_GRAMS: u32 = 5_000;

impl CrucibleData {
    pub fn solid_mass(&self) -> u32 {
        self.solid_contents
            .iter()
            .flatten()
            .filter_map(|item| item.to_stack())
            .filter_map(|stack| {
                composition_total_mass(&stack.item)
                    .map(|mass| mass.saturating_mul(stack.count.unwrap_or(1)))
            })
            .sum()
    }

    pub fn liquid_mass_total(&self) -> u32 {
        self.liquid_mass.iter().copied().sum()
    }

    pub fn total_mass(&self) -> u32 {
        self.solid_mass().saturating_add(self.liquid_mass_total())
    }

    pub fn remaining_capacity(&self) -> u32 {
        CRUCIBLE_CAPACITY_GRAMS.saturating_sub(self.total_mass())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct IngotMoldData {
    pub temp: u16,
    pub temp_acc: u16,
    pub metal_mass: [u32; 8],
}

impl Default for IngotMoldData {
    fn default() -> Self {
        Self {
            temp: 25,
            temp_acc: 0,
            metal_mass: [0; 8],
        }
    }
}

impl IngotMoldData {
    pub fn total_mass(&self) -> u32 {
        self.metal_mass.iter().copied().sum()
    }

    pub fn remaining_capacity(&self) -> u32 {
        INGOT_MOLD_CAPACITY_GRAMS.saturating_sub(self.total_mass())
    }
}

pub const INGOT_SAFE_REMOVAL_TEMP_C: f32 = 200.0;
pub const MIN_CAST_INGOT_MASS_GRAMS: u32 = 100;

pub fn mold_ready_to_extract(mold: &IngotMoldData) -> bool {
    if mold.total_mass() < MIN_CAST_INGOT_MASS_GRAMS {
        return false;
    }
    let temp = crate::thermodynamics::unpack_temperature(mold.temp, mold.temp_acc);
    liquid_melting_point(&mold.metal_mass)
        .is_some_and(|melting_point| temp < melting_point as f32)
        && temp <= INGOT_SAFE_REMOVAL_TEMP_C
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, serde::Serialize, serde::Deserialize)]
pub enum CastIngotKind {
    Copper,
    Iron,
    Bronze,
    Brass,
    Steel,
    Mixed,
}

impl CastIngotKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Copper => "Cast Copper Ingot",
            Self::Iron => "Cast Iron Ingot",
            Self::Bronze => "Cast Bronze Ingot",
            Self::Brass => "Cast Brass Ingot",
            Self::Steel => "Cast Steel Ingot",
            Self::Mixed => "Mixed Metal Ingot",
        }
    }

    pub fn icon_path(self) -> &'static str {
        match self {
            Self::Copper => "items/copper_ingot.png",
            Self::Iron => "items/iron_ingot.png",
            Self::Bronze => "items/bronze_ingot.png",
            Self::Brass => "items/brass_ingot.png",
            Self::Steel => "items/steel_ingot.png",
            Self::Mixed => "items/slag_alloy_ingot.png",
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Self::Copper => 0,
            Self::Iron => 1,
            Self::Bronze => 2,
            Self::Brass => 3,
            Self::Steel => 4,
            Self::Mixed => 5,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Copper,
            1 => Self::Iron,
            2 => Self::Bronze,
            3 => Self::Brass,
            4 => Self::Steel,
            5 => Self::Mixed,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, serde::Serialize, serde::Deserialize)]
pub struct CastIngotData {
    pub mass: u32,
    pub composition: [u32; 8],
    /// 0..1000, displayed as 0..100.0%.
    pub quality_permille: u16,
    pub kind: CastIngotKind,
}

pub fn cast_ingot_from_mold(mold: &IngotMoldData) -> Option<CastIngotData> {
    if !mold_ready_to_extract(mold) {
        return None;
    }
    let total = mold.total_mass() as f32;
    let fraction = |element: Element| mold.metal_mass[element as usize] as f32 / total;
    let copper = fraction(Element::Copper);
    let tin = fraction(Element::Tin);
    let zinc = fraction(Element::Zinc);
    let iron = fraction(Element::Iron);
    let carbon = fraction(Element::Carbon);
    let waste = fraction(Element::Impurity) + fraction(Element::Slag);

    let kind = if copper >= 0.70 && (0.05..=0.30).contains(&tin) {
        CastIngotKind::Bronze
    } else if copper >= 0.55 && (0.10..=0.45).contains(&zinc) {
        CastIngotKind::Brass
    } else if iron >= 0.80 && (0.002..=0.05).contains(&carbon) {
        CastIngotKind::Steel
    } else if copper >= 0.80 {
        CastIngotKind::Copper
    } else if iron >= 0.80 {
        CastIngotKind::Iron
    } else {
        CastIngotKind::Mixed
    };
    let base_quality = match kind {
        CastIngotKind::Copper => copper,
        CastIngotKind::Iron => iron,
        CastIngotKind::Bronze => (copper + tin) * (1.0 - (tin - 0.12).abs()),
        CastIngotKind::Brass => (copper + zinc) * (1.0 - (zinc - 0.30).abs()),
        CastIngotKind::Steel => (iron + carbon) * (1.0 - (carbon - 0.01).abs() * 5.0),
        CastIngotKind::Mixed => (1.0 - waste) * 0.5,
    };
    Some(CastIngotData {
        mass: mold.total_mass(),
        composition: mold.metal_mass,
        quality_permille: (base_quality.clamp(0.0, 1.0) * 1_000.0).round() as u16,
        kind,
    })
}

pub fn liquid_melting_point(mass: &[u32; 8]) -> Option<u16> {
    let total: u64 = Element::ALL
        .iter()
        .map(|element| mass[*element as usize] as u64)
        .sum();
    if total == 0 {
        return None;
    }
    let weighted: u64 = Element::ALL
        .iter()
        .map(|element| mass[*element as usize] as u64 * element.data().melting_point as u64)
        .sum();
    Some(weighted.div_ceil(total).min(u16::MAX as u64) as u16)
}

/// Moves as much molten material as fits in the mold. Remainders are distributed
/// deterministically, so partial pours preserve both composition and total mass.
pub fn pour_crucible_into_mold(
    crucible: &mut CrucibleData,
    mold: &mut IngotMoldData,
) -> Result<u32, &'static str> {
    let source_total: u32 = crucible.liquid_mass.iter().copied().sum();
    if source_total == 0 {
        return Err("The crucible contains no liquid metal");
    }
    let melting_point =
        liquid_melting_point(&crucible.liquid_mass).ok_or("The crucible contains no liquid metal")?;
    if crucible.temp < melting_point {
        return Err("The metal is not hot enough to pour");
    }
    let transfer_total = source_total.min(mold.remaining_capacity());
    if transfer_total == 0 {
        return Err("The mold is full");
    }

    let mut transfer = [0u32; 8];
    let mut remainders = [(0u64, 0usize); 8];
    let mut assigned = 0u32;
    for (index, amount) in crucible.liquid_mass.iter().copied().enumerate() {
        let scaled = amount as u64 * transfer_total as u64;
        transfer[index] = (scaled / source_total as u64) as u32;
        remainders[index] = (scaled % source_total as u64, index);
        assigned += transfer[index];
    }
    remainders.sort_unstable_by(|a, b| b.cmp(a));
    for &(_, index) in remainders.iter().take((transfer_total - assigned) as usize) {
        transfer[index] += 1;
    }

    let old_mass = mold.total_mass();
    let blended_temp = ((mold.temp as u64 * old_mass as u64)
        + (crucible.temp as u64 * transfer_total as u64))
        / (old_mass + transfer_total) as u64;
    mold.temp = blended_temp.min(u16::MAX as u64) as u16;
    mold.temp_acc = crucible.temp_acc;
    for index in 0..8 {
        crucible.liquid_mass[index] -= transfer[index];
        mold.metal_mass[index] += transfer[index];
    }
    Ok(transfer_total)
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

#[cfg(test)]
mod tests {
    use super::{
        pour_crucible_into_mold, CrucibleData, Element, IngotMoldData,
        CRUCIBLE_CAPACITY_GRAMS, INGOT_MOLD_CAPACITY_GRAMS,
    };

    #[test]
    fn element_index_conversion_rejects_reserved_mass_slot() {
        for (index, expected) in Element::ALL.into_iter().enumerate() {
            assert_eq!(Element::from_index(index), Some(expected));
        }
        assert_eq!(Element::from_index(Element::ALL.len()), None);
        assert_eq!(Element::from_index(usize::MAX), None);
    }

    #[test]
    fn crucible_capacity_counts_solid_and_liquid_mass() {
        let mut crucible = CrucibleData::default();
        crucible.solid_contents[0] = Some(crate::item::SimpleItem::from_stack(
            crate::voxel::ItemStack {
                item: crate::item::Item::Block(crate::voxel::BlockType::CopperOre),
                count: Some(4),
            },
        ));
        crucible.liquid_mass[Element::Copper as usize] = 1_000;

        assert_eq!(crucible.solid_mass(), 4_000);
        assert_eq!(crucible.liquid_mass_total(), 1_000);
        assert_eq!(crucible.total_mass(), CRUCIBLE_CAPACITY_GRAMS);
        assert_eq!(crucible.remaining_capacity(), 0);
    }

    #[test]
    fn partial_pour_conserves_mass_and_composition() {
        let mut crucible = CrucibleData::default();
        crucible.temp = 1_600;
        crucible.liquid_mass[Element::Copper as usize] = 800;
        crucible.liquid_mass[Element::Tin as usize] = 200;
        let mut mold = IngotMoldData::default();
        mold.metal_mass[Element::Iron as usize] = 500;

        assert_eq!(pour_crucible_into_mold(&mut crucible, &mut mold), Ok(500));
        assert_eq!(mold.total_mass(), INGOT_MOLD_CAPACITY_GRAMS);
        assert_eq!(mold.metal_mass[Element::Copper as usize], 400);
        assert_eq!(mold.metal_mass[Element::Tin as usize], 100);
        assert_eq!(crucible.liquid_mass[Element::Copper as usize], 400);
        assert_eq!(crucible.liquid_mass[Element::Tin as usize], 100);
    }

    #[test]
    fn cold_metal_cannot_be_poured() {
        let mut crucible = CrucibleData::default();
        crucible.temp = 1_000;
        crucible.liquid_mass[Element::Copper as usize] = 1_000;
        let mut mold = IngotMoldData::default();
        assert!(pour_crucible_into_mold(&mut crucible, &mut mold).is_err());
        assert_eq!(mold.total_mass(), 0);
        assert_eq!(crucible.liquid_mass[Element::Copper as usize], 1_000);
    }

    #[test]
    fn full_cool_mold_extracts_a_matching_ingot() {
        let mut mold = IngotMoldData::default();
        mold.temp = 100;
        mold.metal_mass[Element::Copper as usize] = 900;
        mold.metal_mass[Element::Impurity as usize] = 100;
        let ingot = super::cast_ingot_from_mold(&mold).unwrap();
        assert_eq!(ingot.kind, super::CastIngotKind::Copper);
        assert_eq!(ingot.mass, 1_000);

        mold.metal_mass[Element::Copper as usize] = 700;
        assert_eq!(super::cast_ingot_from_mold(&mold).unwrap().mass, 800);
    }
}
