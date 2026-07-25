//! การจำแนก biome จากภูมิอากาศ — pure ล้วน (ไม่พึ่ง BlockType/Bevy) เพื่อ unit-test ง่าย
//! และใช้ร่วมกันทั้ง worldgen (Noise + Real World) กับการย้อมสีหญ้า/ใบ
//!
//! 3 แกน: อุณหภูมิ (temp) × ความชื้น (humidity) × ความสูงเหนือระดับน้ำ (เมตร = บล็อก)
//! - ยิ่งสูงยิ่งหนาว (lapse rate) → ภูเขากลายเป็นเขตหนาว/alpine เอง
//! - โซนร้อน (temp สูง) ต่อให้หนาว/ฤดูหนาวก็ไม่เป็น tundra → "บางโซน winter ไม่ตกหิมะ"

/// biome ที่จำแนกได้ — signature ต่อโซน (พื้น/ต้นไม้/สี) map ใน voxel.rs
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Biome {
    Desert,          // ร้อน+แห้ง — ทราย ไม่มีต้นไม้
    Savanna,         // ร้อน/อุ่น+กึ่งแห้ง — หญ้าแห้ง ต้นไม้โปร่ง
    TropicalForest,  // ร้อน+ชื้น — เขียวจัดตลอดปี
    Plains,          // อุ่น+แห้ง — หญ้า ต้นไม้บาง
    TemperateForest, // อุ่น+ชื้น — ป่าผลัดใบ (โชว์ฤดู: ใบเปลี่ยนสี+หิมะหน้าหนาว)
    Taiga,           // หนาว+ชื้น — ป่าสน+หิมะ
    Tundra,          // หนาว+แห้ง — หิมะโล่ง
    Alpine,          // สูงมาก — หิน/ยอดโล่ง+หิมะ (ทุกอุณหภูมิ)
}

/// อัตราเย็นลงต่อความสูง (ต่อบล็อก/เมตร) — จริง ~6.5°C/กม.; แม็ปสเกล temp −1..1
/// โหมด noise ผิวสูง ~40 บล็อก → lapse แทบไม่มีผล (โลกลุ่ม); DEM ภูเขาจริงพันเมตร → เย็นชัด
const LAPSE_PER_M: f64 = 0.0004;
/// สูงเกินนี้ (เมตรเหนือระดับน้ำ) = Alpine เสมอ (เหนือแนวต้นไม้) — เห็นผลเฉพาะภูเขา DEM จริง
const ALPINE_M: i32 = 1800;
/// เกณฑ์อุณหภูมิ (หลัง lapse): ร้อน/หนาว; ช่วงกลาง = อุ่น
const HOT: f64 = 0.35;
const COLD: f64 = -0.30;
/// เกณฑ์ความชื้น: มากกว่านี้ = ชื้น
const WET: f64 = 0.0;
/// กึ่งแห้ง (savanna) อยู่ระหว่าง DRY_EDGE..WET ในเขตร้อน/อุ่น
const DRY_EDGE: f64 = -0.25;

/// อุณหภูมิที่ปรับตามความสูงแล้ว (ยิ่งสูงยิ่งเย็น)
pub fn lapse_adjust(temp: f64, height_above_sea: i32) -> f64 {
    temp - LAPSE_PER_M * height_above_sea.max(0) as f64
}

/// จำแนก biome จาก temp ดิบ + humidity + ความสูงเหนือระดับน้ำ (เมตร)
pub fn classify(temp: f64, humidity: f64, height_above_sea: i32) -> Biome {
    if height_above_sea >= ALPINE_M {
        return Biome::Alpine;
    }
    let t = lapse_adjust(temp, height_above_sea);
    if t < COLD {
        // หนาว: ชื้น=ป่าสน, แห้ง=ทุนดรา (ร้อนดิบก็เป็นหนาวได้ถ้าสูงมาก)
        if humidity > WET { Biome::Taiga } else { Biome::Tundra }
    } else if t > HOT {
        if humidity > WET {
            Biome::TropicalForest
        } else if humidity > DRY_EDGE {
            Biome::Savanna
        } else {
            Biome::Desert
        }
    } else {
        // อุ่น
        if humidity > WET {
            Biome::TemperateForest
        } else if humidity > DRY_EDGE {
            Biome::Plains
        } else {
            Biome::Savanna
        }
    }
}

/// อุณหภูมิฐาน (−1..1) จากละติจูดจริง — ใช้ตอน Real World แทน noise
/// เส้นศูนย์สูตร ~0.7 (เขตร้อน) → ขั้วโลก ~−0.7 (หนาว); ไทย ~15° ≈ 0.45
pub fn temp_from_latitude(lat_deg: f64) -> f64 {
    (0.7 - lat_deg.abs() / 55.0).clamp(-0.9, 0.7)
}

/// สีหญ้า/ใบตาม biome แบบ Minecraft colormap — คืน **สีจริง** (absolute) ไม่ใช่ตัวคูณ
/// ใช้กับ texture แบบ grayscale (ลาย luminance): สีสุดท้าย = ค่านี้ × แสง
/// ชื้น/อุ่น = เขียวสด, แห้ง = เหลืองอมน้ำตาล (tan), หนาว = เขียวเข้มอมฟ้า
pub fn foliage_color(temp: f64, humidity: f64) -> [f32; 3] {
    let dry = ((WET - humidity) / 0.9).clamp(0.0, 1.0) as f32; // 0 ชื้น .. 1 แห้งจัด
    let cold = ((0.2 - temp) / 0.9).clamp(0.0, 1.0) as f32; // 0 อุ่น .. 1 หนาวจัด
    let lush: [f32; 3] = [0.34, 0.70, 0.24]; // เขียวสด (ป่าดิบ/ชื้นอุ่น)
    let tan: [f32; 3] = [0.68, 0.62, 0.20]; // เหลืองน้ำตาล (แห้ง/สะวันนา)
    let boreal: [f32; 3] = [0.34, 0.55, 0.40]; // เขียวเข้มอมฟ้า (หนาว)
    let mut c = [0.0f32; 3];
    for i in 0..3 {
        let warm = lush[i] + (tan[i] - lush[i]) * dry; // เขียว → tan ตามความแห้ง
        c[i] = warm + (boreal[i] - warm) * cold; // แล้ว → หนาว
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hot_dry_is_desert_and_never_tundra() {
        assert_eq!(classify(0.6, -0.5, 10), Biome::Desert);
        // ร้อนจัดที่พื้นราบ ไม่มีทางเป็นหนาว
        assert_ne!(classify(0.6, -0.5, 10), Biome::Tundra);
        assert_ne!(classify(0.6, 0.5, 10), Biome::Taiga);
    }

    #[test]
    fn climate_grid_maps_expected_biomes() {
        assert_eq!(classify(0.0, 0.5, 10), Biome::TemperateForest); // อุ่น+ชื้น
        assert_eq!(classify(0.0, -0.5, 10), Biome::Savanna); // อุ่น+แห้งจัด
        assert_eq!(classify(-0.5, 0.5, 10), Biome::Taiga); // หนาว+ชื้น
        assert_eq!(classify(-0.5, -0.5, 10), Biome::Tundra); // หนาว+แห้ง
        assert_eq!(classify(0.6, 0.5, 10), Biome::TropicalForest); // ร้อน+ชื้น
    }

    #[test]
    fn high_altitude_forces_alpine() {
        for (t, h) in [(0.6, -0.5), (0.0, 0.5), (-0.5, 0.5)] {
            assert_eq!(classify(t, h, 2000), Biome::Alpine, "สูง 2000 ต้องเป็น Alpine");
        }
    }

    #[test]
    fn lapse_cools_mountains() {
        // ฐานอุ่นแต่สูง 1600 ม. → เย็นลงจนเป็นเขตหนาว (ยังไม่ถึง Alpine 1800)
        assert!(lapse_adjust(0.2, 1600) < COLD, "1600 ม. ต้องเย็นกว่าเกณฑ์หนาว");
        assert_eq!(classify(0.2, 0.5, 1600), Biome::Taiga);
        // พื้นราบอุณหภูมิเดียวกันยังเป็นเขตอุ่น
        assert_eq!(classify(0.2, 0.5, 10), Biome::TemperateForest);
    }

    #[test]
    fn foliage_color_greens_wet_and_tans_dry() {
        let lush = foliage_color(0.3, 0.6); // ชื้นอุ่น = เขียวสด
        let dry = foliage_color(0.5, -0.7); // แห้งจัด = tan
        // เขียวสด: ช่องเขียว (G) เด่นสุด
        assert!(lush[1] > lush[0] && lush[1] > lush[2], "lush ต้องเขียวเด่น");
        // แห้ง: R เด่นขึ้น (อมเหลือง/น้ำตาล), B ตกลง
        assert!(dry[0] > lush[0], "แห้งต้อง R มากกว่า");
        assert!(dry[2] < lush[2], "แห้งต้อง B น้อยกว่า");
        // เป็นสีจริงในช่วง 0..1
        for c in dry.iter().chain(lush.iter()) {
            assert!(*c >= 0.0 && *c <= 1.0);
        }
    }
}
