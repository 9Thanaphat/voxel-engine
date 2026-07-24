//! ดาราศาสตร์เชิงเรขาคณิตสำหรับท้องฟ้า — pure ล้วน ไม่มี Bevy resource เพื่อ unit-test ได้
//!
//! กรอบพิกัด horizon ของเกม (เหมือนที่ shader/`sun_dir` เดิมใช้):
//!   X = ทิศตะวันออก, Y = ขึ้นบน (zenith), Z = ทิศใต้  (ทิศเหนือ = −Z)
//! ดวงอาทิตย์ขึ้น +X ตอน 6 โมง ตรงกับของเดิม (`Vec3(cos,sin,0.3)`) แต่ตอนนี้อิงฤดู/ละติจูดจริง
//!
//! กรอบพิกัด celestial (ทรงกลมฟ้าที่ดาวติดตรึง):
//!   X_c = วสันตวิษุวัต (RA 0, Dec 0), Y_c = RA 6h, Z_c = ขั้วฟ้าเหนือ (Dec +90°)
//!   จุด (RA α, Dec δ) → เวกเตอร์ (cosδ cosα, cosδ sinα, sinδ)

use bevy::math::{Mat3, Vec3};
use std::f32::consts::{PI, TAU};

/// ความเอียงแกนโลก (องศา) — คุมช่วงฤดู (±23.44°)
pub const AXIAL_TILT_DEG: f32 = 23.44;
/// จำนวนวันต่อปีในเกม (ปฏิทินอย่างง่าย)
pub const DAYS_PER_YEAR: f32 = 365.0;
/// เดือนจันทรคติแบบ sidereal (วัน) — ดวงจันทร์เดินรอบทรงกลมฟ้า
pub const LUNAR_SIDEREAL_DAYS: f32 = 27.32;
/// เดือนจันทรคติแบบ synodic (วัน) — รอบของเฟส (สัมพัทธ์ดวงอาทิตย์)
pub const LUNAR_SYNODIC_DAYS: f32 = 29.53;

fn axial_tilt() -> f32 {
    AXIAL_TILT_DEG.to_radians()
}

/// ลองจิจูดสุริยวิถี (ecliptic longitude) ของดวงอาทิตย์ — 0 ที่วสันตวิษุวัต (day 0)
fn sun_ecliptic_longitude(day_of_year: f32) -> f32 {
    TAU * day_of_year / DAYS_PER_YEAR
}

/// เดคลิเนชันดวงอาทิตย์ (เรเดียน) จากวันในปี — บวก = หน้าร้อนซีกเหนือ
pub fn solar_declination(day_of_year: f32) -> f32 {
    let lambda = sun_ecliptic_longitude(day_of_year);
    (axial_tilt().sin() * lambda.sin()).asin()
}

/// Right Ascension ของดวงอาทิตย์ (เรเดียน) — เดินตามสุริยวิถี ทำให้เกิด sidereal drift ของดาว
pub fn solar_right_ascension(day_of_year: f32) -> f32 {
    let lambda = sun_ecliptic_longitude(day_of_year);
    (axial_tilt().cos() * lambda.sin()).atan2(lambda.cos())
}

/// มุมชั่วโมงของดวงอาทิตย์ (เรเดียน) — 0 ตอนเที่ยง, บวก = บ่าย (ไปทางตะวันตก)
pub fn sun_hour_angle(time_of_day: f32) -> f32 {
    (time_of_day - 12.0) / 24.0 * TAU
}

/// Local Sidereal Time (เรเดียน) — RA ที่กำลังผ่านเมริเดียน
/// = RA ดวงอาทิตย์ + มุมชั่วโมงดวงอาทิตย์ → ดวงอาทิตย์คงเวลาสุริยะ (เที่ยง=ผ่านเมริเดียน)
/// ส่วนดาว RA คงที่เลยเลื่อนวันละ ~4 นาที (sidereal)
pub fn local_sidereal(time_of_day: f32, day_of_year: f32) -> f32 {
    solar_right_ascension(day_of_year) + sun_hour_angle(time_of_day)
}

/// แปลง (มุมชั่วโมง H, เดคลิเนชัน δ) → เวกเตอร์ทิศในกรอบ horizon ที่ละติจูด φ
/// (E=X, Up=Y, S=Z) — ดวงอาทิตย์วิษุวัตขึ้นตรง +X ตอน H=−90°
pub fn hadec_to_horizon(hour_angle: f32, dec: f32, latitude: f32) -> Vec3 {
    let (sh, ch) = hour_angle.sin_cos();
    let (sd, cd) = dec.sin_cos();
    let (sp, cp) = latitude.sin_cos();
    Vec3::new(
        -cd * sh,              // ตะวันออก (+X): H=−90° (เช้า) → +1
        cd * ch * cp + sd * sp, // ขึ้นบน (+Y): H=0,δ=φ → 1 (ผ่านจุดเหนือหัว)
        cd * ch * sp - sd * cp, // ใต้ (+Z)
    )
}

/// (RA, Dec) → เวกเตอร์ในกรอบ celestial (ดาวติดตรึงที่นี่)
pub fn radec_to_cel(ra: f32, dec: f32) -> Vec3 {
    let (sa, ca) = ra.sin_cos();
    let (sd, cd) = dec.sin_cos();
    Vec3::new(cd * ca, cd * sa, sd)
}

/// ทิศดวงอาทิตย์ในกรอบ horizon — ใช้ร่วมกันทั้งแสงโลก (sun_tint) และท้องฟ้า (sky_uniform)
pub fn sun_direction(time_of_day: f32, day_of_year: f32, latitude: f32) -> Vec3 {
    hadec_to_horizon(
        sun_hour_angle(time_of_day),
        solar_declination(day_of_year),
        latitude,
    )
}

/// ลองจิจูดสุริยวิถีของดวงจันทร์ (เรเดียน) — เดินเร็วกว่าดวงอาทิตย์ (รอบ sidereal ~27.32 วัน)
fn moon_ecliptic_longitude(day_of_year: f32) -> f32 {
    TAU * day_of_year / LUNAR_SIDEREAL_DAYS
}

/// ทิศดวงจันทร์ในกรอบ horizon (โมเดลย่อ: อยู่บนสุริยวิถี ไม่คิด inclination/eccentricity)
pub fn moon_direction(time_of_day: f32, day_of_year: f32, latitude: f32) -> Vec3 {
    let lambda = moon_ecliptic_longitude(day_of_year);
    let dec = (axial_tilt().sin() * lambda.sin()).asin();
    let ra = (axial_tilt().cos() * lambda.sin()).atan2(lambda.cos());
    let hour_angle = local_sidereal(time_of_day, day_of_year) - ra;
    hadec_to_horizon(hour_angle, dec, latitude)
}

/// สัดส่วนสว่างของดวงจันทร์ 0..1 (0 = จันทร์ดับ, 1 = เพ็ญ) จาก elongation สัมพัทธ์ดวงอาทิตย์
pub fn moon_illumination(day_of_year: f32) -> f32 {
    let elong = TAU * day_of_year / LUNAR_SYNODIC_DAYS;
    (1.0 - elong.cos()) * 0.5
}

/// เมทริกซ์หมุน celestial → horizon ที่ละติจูด φ และ LST (คอลัมน์ = ภาพของแกน celestial)
/// horizon_dir = M · v_cel  — M ตั้งฉาก (orthonormal)
pub fn equatorial_to_horizon(latitude: f32, lst: f32) -> Mat3 {
    let col_x = hadec_to_horizon(lst, 0.0, latitude); // ภาพของ X_c (RA0,Dec0)
    let col_y = hadec_to_horizon(lst - PI / 2.0, 0.0, latitude); // ภาพของ Y_c (RA6h)
    let col_z = hadec_to_horizon(0.0, PI / 2.0, latitude); // ภาพของ Z_c (ขั้วฟ้าเหนือ)
    Mat3::from_cols(col_x, col_y, col_z)
}

/// เมทริกซ์หมุน horizon → celestial (ทรานสโพสของ [`equatorial_to_horizon`]) —
/// ส่งให้ shader ใช้ map ทิศที่มอง → กรอบฟ้าคงที่ เพื่อ lookup สนามดาว/ทางช้างเผือก
pub fn horizon_to_equatorial(latitude: f32, lst: f32) -> Mat3 {
    equatorial_to_horizon(latitude, lst).transpose()
}

/// ชื่อฤดู (ซีกโลกเหนือ) จากวันในปี — day 0 = วสันตวิษุวัต, อายัน/วิษุวัตทุก ~91 วัน
pub fn season_name(day_of_year: u16) -> &'static str {
    // ข้อความโชว์บน HUD (ฟอนต์เกมไม่มี glyph ไทย + HUD ที่เหลือเป็นอังกฤษ) จึงใช้อังกฤษ
    let d = day_of_year as f32 / DAYS_PER_YEAR; // 0..1
    match d {
        x if x < 0.25 => "Spring",
        x if x < 0.50 => "Summer",
        x if x < 0.75 => "Autumn",
        _ => "Winter",
    }
}

/// ใจกลางกาแล็กซี (Sagittarius, RA 17h45.6m, Dec −28.94°) ในกรอบ celestial
pub fn galactic_center_cel() -> Vec3 {
    radec_to_cel(TAU * 17.76 / 24.0, (-28.94_f32).to_radians())
}

/// ขั้วเหนือกาแล็กซี (RA 12h51.4m, Dec +27.13°) ในกรอบ celestial — ตั้งฉากระนาบทางช้างเผือก
pub fn galactic_pole_cel() -> Vec3 {
    radec_to_cel(TAU * 12.857 / 24.0, (27.13_f32).to_radians())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    /// วสันตวิษุวัต (day 0, δ≈0): ดวงอาทิตย์ขึ้นตรงทิศตะวันออก, ตกทิศตะวันตก, กลางวัน 12 ชม.
    #[test]
    fn equinox_sun_rises_east_sets_west() {
        let lat = 15.0_f32.to_radians();
        let sunrise = sun_direction(6.0, 0.0, lat);
        assert!(sunrise.x > 0.95, "6โมงต้องอยู่ตะวันออก (+X) ได้ {}", sunrise.x);
        assert!(approx(sunrise.y, 0.0, 0.02), "วิษุวัตขึ้นตรงขอบฟ้าพอดี y={}", sunrise.y);

        let sunset = sun_direction(18.0, 0.0, lat);
        assert!(sunset.x < -0.95, "6โมงเย็นต้องอยู่ตะวันตก (−X) ได้ {}", sunset.x);
        assert!(approx(sunset.y, 0.0, 0.02), "วิษุวัตตกตรงขอบฟ้าพอดี y={}", sunset.y);

        let noon = sun_direction(12.0, 0.0, lat);
        assert!(noon.y > 0.9, "เที่ยงต้องเกือบเหนือหัว y={}", noon.y);
        assert!(noon.z > 0.0, "ซีกเหนือ เที่ยงดวงเยื้องไปทางใต้ (+Z) z={}", noon.z);
    }

    /// ความสูงดวงอาทิตย์เที่ยง: ครีษมายัน = cos(φ−tilt), เหมายัน = cos(φ+tilt)
    #[test]
    fn solstice_noon_altitudes() {
        let lat = 15.0_f32.to_radians();
        let tilt = axial_tilt();

        // ครีษมายัน ≈ day 91.25 (λ=π/2, δ=+tilt)
        let summer = sun_direction(12.0, DAYS_PER_YEAR * 0.25, lat);
        assert!(approx(summer.y, (lat - tilt).cos(), 0.01),
            "ครีษมายันเที่ยง y ควร={} ได้={}", (lat - tilt).cos(), summer.y);

        // เหมายัน ≈ day 273.75 (λ=3π/2, δ=−tilt)
        let winter = sun_direction(12.0, DAYS_PER_YEAR * 0.75, lat);
        assert!(approx(winter.y, (lat + tilt).cos(), 0.01),
            "เหมายันเที่ยง y ควร={} ได้={}", (lat + tilt).cos(), winter.y);

        assert!(summer.y > winter.y, "หน้าร้อนดวงต้องสูงกว่าหน้าหนาว");
    }

    /// เมทริกซ์ celestial→horizon ตรงกับ hadec_to_horizon สำหรับจุดใดๆ + ตั้งฉาก
    #[test]
    fn matrix_matches_hadec_and_orthonormal() {
        let lat = 32.0_f32.to_radians();
        let lst = 1.3_f32;
        let m = equatorial_to_horizon(lat, lst);

        // orthonormal: Mᵀ·M ≈ I
        let prod = m.transpose() * m;
        let id = Mat3::IDENTITY;
        for i in 0..3 {
            for j in 0..3 {
                assert!(approx(prod.col(i)[j], id.col(i)[j], 1e-4),
                    "เมทริกซ์ต้องตั้งฉาก ({i},{j})");
            }
        }

        // ตรงกับ hadec_to_horizon: M·v_cel(α,δ) == hadec_to_horizon(lst−α, δ)
        for (ra, dec) in [(0.5_f32, 0.3_f32), (2.1, -0.7), (4.8, 0.9), (5.9, -0.2)] {
            let via_matrix = m * radec_to_cel(ra, dec);
            let via_hadec = hadec_to_horizon(lst - ra, dec, lat);
            assert!((via_matrix - via_hadec).length() < 1e-4,
                "RA={ra} Dec={dec}: matrix {via_matrix:?} != hadec {via_hadec:?}");
        }
    }

    /// ขั้วฟ้าเหนืออยู่สูงจากขอบฟ้า = ละติจูด, อยู่ทางทิศเหนือ (−Z)
    #[test]
    fn celestial_pole_altitude_equals_latitude() {
        for deg in [0.0_f32, 15.0, 45.0, 66.0] {
            let lat = deg.to_radians();
            let m = equatorial_to_horizon(lat, 2.0);
            let pole = m * Vec3::Z; // Z_c = ขั้วฟ้าเหนือ
            assert!(approx(pole.y.asin(), lat, 1e-4),
                "ขั้วฟ้าต้องสูง={deg}° ได้={}°", pole.y.asin().to_degrees());
            assert!(deg < 1.0 || pole.z < 0.0, "ขั้วฟ้าต้องอยู่ทางเหนือ (−Z)");
        }
    }

    /// ดาว RA คงที่ผ่านเมริเดียน (culminate) เร็วขึ้น ~4 นาที/วัน (sidereal drift)
    #[test]
    fn stars_drift_four_minutes_per_day() {
        // เวลาที่ดาว RA=α ผ่านเมริเดียน: LST=α → sun_RA(day)+(t−12)/24·τ = α
        let culmination_time = |day: f32, ra: f32| -> f32 {
            let ha = ra - solar_right_ascension(day); // = sun_hour_angle → (t−12)/24·τ
            12.0 + ha / TAU * 24.0
        };
        let ra = 1.0_f32;
        // รายวันแกว่ง 3.6–4.3 นาทีตามความเร็ว RA ดวงอาทิตย์ (ช้าช่วงวิษุวัต เร็วช่วงอายัน)
        let t0 = culmination_time(100.0, ra);
        let t1 = culmination_time(101.0, ra);
        let drift_min = (t0 - t1) * 60.0; // เร็วขึ้น = t ลดลง
        assert!(drift_min > 3.4 && drift_min < 4.6,
            "ดาวควรผ่านเมริเดียนเร็วขึ้น ~4 นาที/วัน ได้ {drift_min}");

        // ค่าเฉลี่ยทั้งปีต้องเท่ากับ 1 รอบเต็ม/365 วัน = 3.945 นาที/วัน (sidereal แท้)
        let avg_min = 24.0 * 60.0 / DAYS_PER_YEAR;
        assert!(approx(avg_min, 3.945, 0.01), "เฉลี่ยทั้งปีควร 3.945 นาที ได้ {avg_min}");
    }

    /// ใจกลางทางช้างเผือกขึ้นสูงกว่าตอนเที่ยงคืนหน้าร้อน เทียบหน้าหนาว (ซีกโลกเหนือ)
    #[test]
    fn galactic_center_higher_on_summer_midnight() {
        let lat = 15.0_f32.to_radians();
        let gc = galactic_center_cel();
        let alt_at = |day: f32| -> f32 {
            let m = equatorial_to_horizon(lat, local_sidereal(0.0, day)); // เที่ยงคืน
            (m * gc).y
        };
        // ใจกลางกาแล็กซี (Sagittarius) เด่นคืนหน้าร้อน (มิ.ย.-ส.ค.)
        let summer = alt_at(DAYS_PER_YEAR * 0.42); // ~ปลาย พ.ค./มิ.ย.
        let winter = alt_at(DAYS_PER_YEAR * 0.92); // ~ธ.ค.
        assert!(summer > winter,
            "ใจกลางกาแล็กซีต้องสูงกว่าคืนหน้าร้อน: ร้อน={summer} หนาว={winter}");
    }

    /// เฟสจันทร์: day 0 ดับ, ครึ่ง synodic เพ็ญ
    #[test]
    fn moon_phase_cycles() {
        assert!(approx(moon_illumination(0.0), 0.0, 0.01), "day0 ต้องจันทร์ดับ");
        assert!(approx(moon_illumination(LUNAR_SYNODIC_DAYS * 0.5), 1.0, 0.01),
            "ครึ่งรอบต้องเพ็ญ");
    }
}
