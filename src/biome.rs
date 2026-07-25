//! ภูมิอากาศ pure ล้วน (ไม่พึ่ง BlockType/Bevy) เพื่อ unit-test ง่าย — คำนวณ อุณหภูมิ/ความชื้น/
//! ละติจูดภูมิอากาศ + จำแนก **เขตอุณหภูมิใหญ่** (ClimateZone). การเลือก sub-biome จริง (พื้น/ต้นไม้/
//! ความสูงภูเขา) เป็น data-driven อยู่ใน [`crate::biomegen`] เพราะต้องอิง BlockType + config ที่ปรับได้

/// เขตอุณหภูมิใหญ่ 4 เขต — แบ่งชั้นบนสุดของ biome (ในแต่ละเขตมี sub-biome อีกที ดู biomegen)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum ClimateZone {
    Cold, // หนาว
    Cool, // เย็น
    Warm, // อุ่น
    Hot,  // ร้อน
}

impl ClimateZone {
    pub const ALL: [ClimateZone; 4] =
        [ClimateZone::Cold, ClimateZone::Cool, ClimateZone::Warm, ClimateZone::Hot];
    pub fn label(self) -> &'static str {
        match self {
            ClimateZone::Cold => "Cold",
            ClimateZone::Cool => "Cool",
            ClimateZone::Warm => "Warm",
            ClimateZone::Hot => "Hot",
        }
    }
}

/// เกณฑ์อุณหภูมิแบ่งเขต (บน temperature_raw ที่นิ่งตามฤดู — worldgen ต้อง deterministic)
const ZONE_HOT: f64 = 0.35;
const ZONE_WARM: f64 = 0.05;
const ZONE_COOL: f64 = -0.25;

/// จำแนกเขตอุณหภูมิใหญ่จากอุณหภูมิ (−1..1)
pub fn zone_of(temp: f64) -> ClimateZone {
    if temp > ZONE_HOT {
        ClimateZone::Hot
    } else if temp > ZONE_WARM {
        ClimateZone::Warm
    } else if temp > ZONE_COOL {
        ClimateZone::Cool
    } else {
        ClimateZone::Cold
    }
}

/// อัตราเย็นลงต่อความสูง (ต่อบล็อก/เมตร) — ใช้ตัดสินหิมะตามความสูงใน auto-weather (ดู weather.rs)
const LAPSE_PER_M: f64 = 0.0004;
/// เกณฑ์ความชื้น: มากกว่านี้ = ชื้น (ใช้ใน foliage_color)
const WET: f64 = 0.0;

/// อุณหภูมิที่ปรับตามความสูงแล้ว (ยิ่งสูงยิ่งเย็น)
pub fn lapse_adjust(temp: f64, height_above_sea: i32) -> f64 {
    temp - LAPSE_PER_M * height_above_sea.max(0) as f64
}

/// อุณหภูมิฐาน (−1..1) จากละติจูดจริง — ใช้ตอน Real World แทน noise
/// เส้นศูนย์สูตร ~0.7 (เขตร้อน) → ขั้วโลก ~−0.7 (หนาว); ไทย ~15° ≈ 0.45
pub fn temp_from_latitude(lat_deg: f64) -> f64 {
    (0.7 - lat_deg.abs() / 55.0).clamp(-0.9, 0.7)
}

/// ละติจูดที่ spawn (z=0) และสเกล °/บล็อกตามแกนเหนือ-ใต้ (Z) — คุมว่าแถบ biome ไล่เร็วแค่ไหน
/// ย่อจากสเกลภูมิศาสตร์จริง (~0.0009°/บล็อก, หลักหมื่นบล็อกต่อแถบ) ให้เห็นแถบภายในระยะเดินไหว
/// ปรับ `LAT_PER_BLOCK` มากขึ้น = แถบยิ่งถี่ (เดินสั้นลงก็เปลี่ยน biome)
const CLIMATE_ORIGIN_LAT: f64 = 21.0;
const CLIMATE_LAT_PER_BLOCK: f64 = 0.006; // ~166 บล็อก/องศา
/// ละติจูดสูงสุด (ขั้วโลก) ที่คลื่นแกว่งไปถึงก่อนพับกลับ
const CLIMATE_POLE_LAT: f64 = 66.0;

/// ละติจูดภูมิอากาศจากพิกัด Z ของโลก — คุมการไล่แถบ biome (ร้อน↔หนาว, ชื้น↔แห้ง) และฤดู
/// เดิน +z (ลงใต้) เข้าเขตร้อน, −z (ขึ้นเหนือ) เข้าเขตหนาว. ใช้แทน dem::block_to_latlon
/// ในทุกจุดที่เป็นเรื่องภูมิอากาศ (ไม่ใช่การหา tile DEM จริง)
///
/// พับเป็น **คลื่นสามเหลี่ยม** ระหว่าง ±CLIMATE_POLE_LAT แทนการ clamp → เดินไกลไปทางเดียว
/// ภูมิอากาศจะวน ร้อน↔หนาว ไม่รู้จบ (ไม่มีขั้วโลกถาวรให้ตัน) ทิศ/สเกลใกล้ spawn เท่าเดิม
pub fn climate_lat(wz: f64) -> f64 {
    let travel = CLIMATE_ORIGIN_LAT - wz * CLIMATE_LAT_PER_BLOCK; // ละติจูด "ยังไม่พับ" (เชิงเส้น)
    let amp = CLIMATE_POLE_LAT;
    let period = 4.0 * amp; // 0→+66→0→−66→0 ครบรอบ
    let p = (travel + amp).rem_euclid(period); // 0..period
    if p < 2.0 * amp { p - amp } else { 3.0 * amp - p }
}

/// คำนวณอุณหภูมิ ณ เวลาปัจจุบัน (รวมผลของละติจูดและฤดูกาล)
pub fn dynamic_temp(lat_deg: f64, day_of_year: f32) -> f64 {
    let base_temp = temp_from_latitude(lat_deg);
    let solar_dec = crate::astro::solar_declination(day_of_year).to_degrees() as f64;
    let season_intensity = (lat_deg / 90.0).abs();
    let seasonal_offset = (solar_dec * lat_deg.signum() / 23.44) * season_intensity * 0.4;
    (base_temp + seasonal_offset).clamp(-0.9, 0.7)
}

/// ความชื้นฐานตามละติจูด (Hadley cell) — ชื้นศูนย์สูตร (ITCZ), แห้งแถบ ~30° (ความกดสูง),
/// ชื้นแถบอบอุ่น ~60°, แห้งขั้วโลก — คืน −1..1 เอาไปผสมกับ noise ใน worldgen (ดู `humidity_raw`)
pub fn humidity_band(lat_deg: f64) -> f64 {
    // cos: 0°→+1(ชื้น), 30°→−1(แห้ง), 60°→+1(ชื้น), 90°→−1(แห้ง)
    (lat_deg.abs() / 30.0 * std::f64::consts::PI).cos()
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
    fn zone_of_maps_temperature_tiers() {
        assert_eq!(zone_of(0.6), ClimateZone::Hot);
        assert_eq!(zone_of(0.2), ClimateZone::Warm);
        assert_eq!(zone_of(-0.1), ClimateZone::Cool);
        assert_eq!(zone_of(-0.5), ClimateZone::Cold);
        // ยิ่งอุ่นยิ่งเลื่อนไปทางร้อน (monotonic)
        assert!(zone_of(0.4) as u8 > zone_of(0.0) as u8);
    }

    #[test]
    fn lapse_cools_with_altitude() {
        // ยิ่งสูงยิ่งเย็น (ใช้ตัดสินหิมะบนภูเขาใน auto-weather)
        assert!(lapse_adjust(0.2, 1600) < 0.2 - 0.3, "1600 ม. ต้องเย็นลงชัด");
        assert_eq!(lapse_adjust(0.5, 0), 0.5);
    }

    #[test]
    fn climate_lat_waves_without_pole_deadend() {
        assert!((climate_lat(0.0) - 21.0).abs() < 1e-9, "spawn = 21°");
        // เดิน +z ~3500 บล็อก → เข้าใกล้ศูนย์สูตร (แถบไล่ภายในระยะเดินไหว ไม่ใช่หลักหมื่น)
        assert!(climate_lat(3500.0).abs() < 1.5, "+3500 บล็อก ควรใกล้ศูนย์สูตร");
        // อยู่ในช่วง ±66° เสมอ ไม่ตันขั้วโลก (พับกลับ)
        for z in [-5.0e5, -1234.0, 9.9e4, 5.0e5] {
            let l = climate_lat(z);
            assert!((-66.0..=66.0).contains(&l), "lat ต้องอยู่ใน ±66°");
        }
        // เป็นคาบ: เดินครบ 1 รอบ (4×66/0.006 = 44,000 บล็อก) กลับมาภูมิอากาศเดิม
        assert!((climate_lat(1234.0) - climate_lat(1234.0 + 44_000.0)).abs() < 1e-6, "ต้องวนเป็นคาบ");
    }

    #[test]
    fn humidity_band_wet_equator_dry_subtropics() {
        // ศูนย์สูตรชื้นกว่าแถบ 30°, และแถบอบอุ่น 60° ก็ชื้นกว่า 30° (โครง Hadley)
        assert!(humidity_band(0.0) > humidity_band(30.0), "ศูนย์สูตรต้องชื้นกว่า 30°");
        assert!(humidity_band(60.0) > humidity_band(30.0), "60° ต้องชื้นกว่า 30°");
        // สมมาตรซีกโลก
        assert!((humidity_band(20.0) - humidity_band(-20.0)).abs() < 1e-9);
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
