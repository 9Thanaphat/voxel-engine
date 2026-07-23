use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// thickness ของลำต้นที่งอกจากดิน (หน่วย 1/32 บล็อก — 16 = กว้างครึ่งบล็อก)
pub const TRUNK_THICKNESS: u8 = 16;
/// thickness ของกิ่งที่ตั้งเป็น root โดยไม่ได้งอกจากดิน (กิ่งลอย/กิ่งกำพร้าที่กู้คืน)
pub const LOOSE_THICKNESS: u8 = 8;
/// thickness บางสุด — กิ่งปลายสุดยังต้องหนาพอให้เห็น
pub const MIN_THICKNESS: u8 = 2;

/// เพื่อนบ้าน 26 ทิศ เรียงตามจำนวนแกนที่ขยับ: ตรง 6 → เฉียงขอบ 12 → เฉียงมุม 8
/// ลำดับคงที่สำคัญมาก เพราะใช้ tie-break ตอนเลือก parent — host กับ client
/// ต้องได้ผลเดียวกันเป๊ะ ไม่งั้นทรงกิ่งสองเครื่องจะไม่ตรงกัน
pub const NEIGHBOUR_DIRS: [IVec3; 26] = build_neighbour_dirs();

const fn build_neighbour_dirs() -> [IVec3; 26] {
    let mut out = [IVec3::ZERO; 26];
    let mut n = 0;
    let mut want = 1;
    while want <= 3 {
        let mut x = -1;
        while x <= 1 {
            let mut y = -1;
            while y <= 1 {
                let mut z = -1;
                while z <= 1 {
                    let axes = (x != 0) as i32 + (y != 0) as i32 + (z != 0) as i32;
                    if axes == want {
                        out[n] = IVec3::new(x, y, z);
                        n += 1;
                    }
                    z += 1;
                }
                y += 1;
            }
            x += 1;
        }
        want += 1;
    }
    out
}

/// thickness ของกิ่งลูกที่ผู้เล่นวางต่อจาก parent — ลดทีละ 1 หน่วย (1/32 บล็อก)
///
/// ขนาดของ "ขั้น" ที่ตาเห็นตรงรอยต่อคือ (t_parent - t_child)/64 บล็อก ดังนั้นลดทีละ 1
/// = ขั้นราว 1.5% ของบล็อก แทบมองไม่เห็น ส่วนลดทีละ 2 (ของเดิม) เห็นเป็นวงแหวนชัด
/// ทุกบล็อก — นี่คือเหตุผลที่กิ่งที่วางเองต่อกันแล้วดูไม่เนียน
pub fn child_thickness(parent: u8) -> u8 {
    parent.saturating_sub(1).max(MIN_THICKNESS)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BranchNode {
    pub parent_pos: Option<IVec3>,
    pub thickness: u8,
    pub children: HashSet<IVec3>,
}

#[derive(Resource, Default, Serialize, Deserialize, Clone)]
pub struct BranchNetwork {
    pub nodes: HashMap<IVec3, BranchNode>,
}

/// รูปแบบที่เก็บลงไฟล์ต่อ chunk — พก thickness มาด้วยจะได้ไม่ต้องคำนวณใหม่ตอนโหลด
/// ทำให้ลำดับ record ไม่สำคัญ และกิ่งที่ parent อยู่คนละ chunk ก็ไม่เพี้ยน
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct BranchRecord {
    pub pos: [i32; 3],
    pub parent: Option<[i32; 3]>,
    pub thickness: u8,
}

/// ตำแหน่งนี้อยู่ในกรอบ chunk ไหน (แกน y ไม่เกี่ยว — chunk เป็นคอลัมน์เต็มความสูง)
pub fn chunk_of(pos: IVec3, width: i32) -> IVec2 {
    IVec2::new(pos.x.div_euclid(width), pos.z.div_euclid(width))
}

impl BranchNetwork {
    /// เพิ่ม root node (ไม่มี parent) — thickness กำหนดเองตามที่มา
    /// (งอกจากดิน = TRUNK_THICKNESS, กิ่งลอย = LOOSE_THICKNESS)
    pub fn add_root(&mut self, pos: IVec3, thickness: u8) {
        self.nodes.insert(pos, BranchNode {
            parent_pos: None,
            thickness,
            children: HashSet::new(),
        });
    }

    /// เพิ่ม branch ต่อจาก parent — thickness ลดลงตาม parent
    /// ถ้า parent ไม่มีอยู่จริงใน network ให้ตั้งเป็น root ลอยแทนการผูก parent ผี
    pub fn add_branch(&mut self, pos: IVec3, parent_pos: IVec3) {
        let Some(parent_thickness) = self.nodes.get(&parent_pos).map(|n| n.thickness) else {
            self.add_root(pos, LOOSE_THICKNESS);
            return;
        };
        let new_thickness = child_thickness(parent_thickness);

        self.nodes.insert(pos, BranchNode {
            parent_pos: Some(parent_pos),
            thickness: new_thickness,
            children: HashSet::new(),
        });

        if let Some(parent) = self.nodes.get_mut(&parent_pos) {
            parent.children.insert(pos);
        }
    }

    /// ลบ node ตัวเดียวออกจาก network (ถอดออกจาก parent.children ด้วย)
    /// คืนลูกที่กลายเป็นกำพร้า — ผู้เรียกเป็นคนตัดสินใจว่าจะ cascade ต่อยังไง
    pub fn detach(&mut self, pos: IVec3) -> Vec<IVec3> {
        let Some(node) = self.nodes.remove(&pos) else {
            return Vec::new();
        };
        if let Some(parent_pos) = node.parent_pos {
            if let Some(parent) = self.nodes.get_mut(&parent_pos) {
                parent.children.remove(&pos);
            }
        }
        node.children.into_iter().collect()
    }

    pub fn thickness_at(&self, pos: IVec3) -> Option<u8> {
        self.nodes.get(&pos).map(|n| n.thickness)
    }

    /// node นี้ยังมี parent ที่ยังอยู่ใน network ไหม (root ถือว่าผ่านเสมอ)
    pub fn is_supported(&self, pos: IVec3) -> bool {
        match self.nodes.get(&pos) {
            Some(node) => match node.parent_pos {
                Some(pp) => self.nodes.contains_key(&pp),
                None => true,
            },
            None => false,
        }
    }

    /// node ทั้งหมดของ chunk นี้ในรูปแบบที่พร้อมเขียนลงไฟล์
    pub fn chunk_records(&self, chunk_pos: IVec2, width: i32) -> Vec<BranchRecord> {
        let mut out: Vec<BranchRecord> = self
            .nodes
            .iter()
            .filter(|(p, _)| chunk_of(**p, width) == chunk_pos)
            .map(|(p, n)| BranchRecord {
                pos: p.to_array(),
                parent: n.parent_pos.map(|pp| pp.to_array()),
                thickness: n.thickness,
            })
            .collect();
        // HashMap ไม่การันตีลำดับ — เรียงให้ไฟล์เซฟ deterministic (diff/เทียบง่าย)
        out.sort_unstable_by_key(|r| r.pos);
        out
    }

    /// ใส่ node จากไฟล์/worldgen กลับเข้า network แล้วเชื่อม children ทั้งสองทาง
    /// (ทั้งกับ parent ที่โหลดอยู่ก่อน และกับลูกที่โหลดอยู่ก่อนแต่ parent เพิ่งมาถึง)
    pub fn merge_records(&mut self, records: &[BranchRecord]) {
        for r in records {
            let pos = IVec3::from_array(r.pos);
            self.nodes.insert(pos, BranchNode {
                parent_pos: r.parent.map(IVec3::from_array),
                thickness: r.thickness,
                children: HashSet::new(),
            });
        }
        // เชื่อมลิงก์หลังใส่ครบ — record ตัวหลังอาจเป็น parent ของตัวก่อนหน้าก็ได้
        for r in records {
            let pos = IVec3::from_array(r.pos);
            if let Some(pp) = r.parent.map(IVec3::from_array) {
                if let Some(parent) = self.nodes.get_mut(&pp) {
                    parent.children.insert(pos);
                }
            }
            // ลูกที่โหลดมาก่อนหน้าและชี้มาที่ node นี้ (เช่นกิ่งพาดข้าม chunk)
            let adopted: Vec<IVec3> = NEIGHBOUR_DIRS
                .iter()
                .map(|d| pos + *d)
                .filter(|c| self.nodes.get(c).is_some_and(|n| n.parent_pos == Some(pos)))
                .collect();
            if let Some(node) = self.nodes.get_mut(&pos) {
                node.children.extend(adopted);
            }
        }
    }

    /// ทิ้ง node ของ chunk ที่ unload ออกจากหน่วยความจำ (ข้อมูลอยู่ในไฟล์ chunk แล้ว)
    /// — ต้องถอดออกจาก children ของ parent ที่ยังโหลดอยู่ด้วย ไม่งั้น mesh จะวาดกิ่ง
    /// ยื่นไปหาที่ที่ไม่มีอะไรแล้ว
    pub fn evict_chunk(&mut self, chunk_pos: IVec2, width: i32) {
        let gone: Vec<IVec3> = self
            .nodes
            .keys()
            .filter(|p| chunk_of(**p, width) == chunk_pos)
            .copied()
            .collect();
        // ลูกที่อยู่คนละ chunk จะกลายเป็นกิ่งกำพร้าชั่วคราว — ไม่เป็นไร เพราะ cascade
        // ข้าม node ที่ chunk ของ parent ไม่ได้โหลดอยู่
        for pos in &gone {
            if let Some(node) = self.nodes.remove(pos) {
                if let Some(pp) = node.parent_pos {
                    if let Some(parent) = self.nodes.get_mut(&pp) {
                        parent.children.remove(pos);
                    }
                }
            }
        }
    }

    /// สำเนา node เฉพาะในกรอบ chunk + ขอบ 1 บล็อกในแกน x/z
    /// (mesh ต้องรู้ thickness ของ node เพื่อนบ้านที่อยู่ข้าม chunk เพื่อคำนวณรอยต่อ)
    /// ใช้ส่งเข้า async mesh task ที่แตะ resource ตรงๆ ไม่ได้
    pub fn snapshot_for_chunk(&self, chunk_pos: IVec2, width: i32) -> Self {
        let min_x = chunk_pos.x * width - 1;
        let max_x = chunk_pos.x * width + width;
        let min_z = chunk_pos.y * width - 1;
        let max_z = chunk_pos.y * width + width;

        let nodes = self
            .nodes
            .iter()
            .filter(|(p, _)| p.x >= min_x && p.x <= max_x && p.z >= min_z && p.z <= max_z)
            .map(|(p, n)| (*p, n.clone()))
            .collect();
        Self { nodes }
    }

    /// เซฟ BranchNetwork ลง JSON file
    pub fn save(&self, dir: &std::path::Path) {
        let path = dir.join("branch_network.json");
        match serde_json::to_string(self) {
            Ok(json) => {
                let _ = std::fs::create_dir_all(dir);
                if let Err(e) = std::fs::write(&path, json) {
                    bevy::log::warn!("save branch_network failed: {}", e);
                }
            }
            Err(e) => bevy::log::warn!("encode branch_network failed: {}", e),
        }
    }

    /// โหลด BranchNetwork จาก JSON file (คืน Default ถ้าไม่มีไฟล์)
    pub fn load(dir: &std::path::Path) -> Self {
        let path = dir.join("branch_network.json");
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// detach ลบแค่ตัวเอง ถอดจาก parent และคืนลูกที่กำพร้า (ไม่ลบลูกทิ้งเอง)
    #[test]
    fn detach_orphans_children_only() {
        let (root, mid, tip) = (IVec3::new(0, 0, 0), IVec3::new(0, 1, 0), IVec3::new(0, 2, 0));
        let mut bn = BranchNetwork::default();
        bn.add_root(root, TRUNK_THICKNESS);
        bn.add_branch(mid, root);
        bn.add_branch(tip, mid);

        let orphans = bn.detach(mid);
        assert_eq!(orphans, vec![tip]);
        assert!(!bn.nodes.contains_key(&mid));
        assert!(bn.nodes.contains_key(&tip), "ลูกต้องยังอยู่ให้ผู้เรียก cascade เอง");
        assert!(!bn.nodes[&root].children.contains(&mid), "parent ต้องไม่เหลือลิงก์ค้าง");
        assert!(!bn.is_supported(tip), "ลูกกำพร้าต้องนับว่าไม่มีที่ยึด");
    }

    /// thickness บางลงทีละ 2 ตามระยะจากลำต้น และ parent ผีกลายเป็น root ลอย
    #[test]
    fn thickness_tapers_and_ghost_parent_becomes_root() {
        let (root, mid) = (IVec3::new(0, 0, 0), IVec3::new(0, 1, 0));
        let mut bn = BranchNetwork::default();
        bn.add_root(root, TRUNK_THICKNESS);
        bn.add_branch(mid, root);
        assert_eq!(bn.thickness_at(mid), Some(child_thickness(TRUNK_THICKNESS)));
        assert!(bn.thickness_at(mid) < Some(TRUNK_THICKNESS), "ต้องเรียวลงจริง");

        let ghost = IVec3::new(9, 9, 9);
        let hanging = IVec3::new(9, 10, 9);
        bn.add_branch(hanging, ghost);
        assert_eq!(bn.thickness_at(hanging), Some(LOOSE_THICKNESS));
        assert!(bn.is_supported(hanging), "root ลอยถือว่ามีที่ยึดในตัวเอง");
    }

    /// snapshot เอาเฉพาะ node ในกรอบ chunk + ขอบ 1 บล็อก
    #[test]
    fn snapshot_covers_chunk_plus_margin() {
        let mut bn = BranchNetwork::default();
        let inside = IVec3::new(5, 70, 5);
        let margin = IVec3::new(-1, 70, 5);
        let outside = IVec3::new(-2, 70, 5);
        bn.add_root(inside, TRUNK_THICKNESS);
        bn.add_root(margin, TRUNK_THICKNESS);
        bn.add_root(outside, TRUNK_THICKNESS);

        let snap = bn.snapshot_for_chunk(IVec2::new(0, 0), 16);
        assert!(snap.nodes.contains_key(&inside));
        assert!(snap.nodes.contains_key(&margin));
        assert!(!snap.nodes.contains_key(&outside));
    }
}
