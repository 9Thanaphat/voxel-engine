use bevy::prelude::*;

/// Outdoor air temperature in Celsius. This is the single boundary used by
/// heat-bearing gameplay objects. A future room/cell simulation can replace
/// this lookup without changing crucible or mold cooling.
pub fn outdoor_temperature_celsius(
    pos: Vec3,
    settings: &crate::GameSettings,
    weather: &crate::weather::Weather,
) -> f32 {
    let latitude = crate::biome::climate_lat(pos.z as f64);
    let climate = crate::biome::dynamic_temp(latitude, settings.day_of_year as f32)
        + settings.noise.temp_offset;

    // Map the existing climate scale (-0.9..0.7) to roughly -21..39 °C.
    let climate_c = 12.5 + climate as f32 * 37.5;
    let altitude_c =
        -(pos.y - crate::voxel::SEA_LEVEL as f32).max(0.0) * 0.0065;

    // Coldest shortly before sunrise, warmest around 14:00.
    let daily_c = ((settings.time_of_day - 8.0) / 24.0 * std::f32::consts::TAU).sin() * 4.0;
    let weather_c = match weather.kind {
        crate::weather::WeatherKind::Clear => 0.0,
        crate::weather::WeatherKind::Rain => -2.0 * weather.intensity,
        crate::weather::WeatherKind::Snow => -4.0 * weather.intensity,
    };

    climate_c + altitude_c + daily_c + weather_c
}

/// Newton cooling with thermal inertia from contained mass.
pub fn cool_toward_ambient(current_c: f32, ambient_c: f32, mass_g: u32, dt: f32) -> f32 {
    let inertia = 1.0 + mass_g as f32 / 1_000.0;
    let response = 1.0 - (-0.018 * dt.max(0.0) / inertia).exp();
    current_c + (ambient_c - current_c) * response
}

pub fn unpack_temperature(temp: u16, fraction: u16) -> f32 {
    temp as f32 + fraction as f32 / u16::MAX as f32
}

pub fn store_temperature(value: f32, temp: &mut u16, fraction: &mut u16) {
    let value = value.clamp(0.0, u16::MAX as f32);
    *temp = value.floor() as u16;
    *fraction = ((value.fract()) * u16::MAX as f32) as u16;
}

#[cfg(test)]
mod tests {
    use super::cool_toward_ambient;

    #[test]
    fn cooling_never_overshoots_and_more_mass_cools_slower() {
        let light = cool_toward_ambient(1_000.0, 10.0, 0, 10.0);
        let heavy = cool_toward_ambient(1_000.0, 10.0, 1_000, 10.0);
        assert!((10.0..1_000.0).contains(&light));
        assert!(heavy > light);
    }
}
