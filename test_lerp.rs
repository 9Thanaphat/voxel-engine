fn main() {
    let mut furnace_temp = 25.0;
    let mut pot_temp_f = 25.0;
    let mut pot_temp_acc = 0;
    
    let dt = 1.0 / 60.0;
    let pot_lerp = 0.2;
    
    let mut pot_temp = 25;
    
    for frame in 0..(60 * 180) { // 3 minutes
        // Furnace heats up
        furnace_temp += (1200.0 - furnace_temp) * 0.1 * dt;
        
        // Pot heats up
        let pot_target = furnace_temp;
        let current_temp_f = pot_temp as f32 + (pot_temp_acc as f32 / 65535.0);
        let mut new_temp_f = current_temp_f + (pot_target - current_temp_f) * pot_lerp * dt;
        if pot_target > current_temp_f && new_temp_f - current_temp_f < 10.0 * dt {
            new_temp_f += 10.0 * dt;
            if new_temp_f > pot_target { new_temp_f = pot_target; }
        }
        pot_temp = new_temp_f.floor() as u16;
        pot_temp_acc = ((new_temp_f - new_temp_f.floor()) * 65535.0) as u16;
        
        if frame % (60 * 10) == 0 {
            println!("Time: {}s, Furnace: {:.1}, Pot: {}", frame / 60, furnace_temp, pot_temp);
        }
    }
}
