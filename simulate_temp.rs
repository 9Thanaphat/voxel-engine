fn main() {
    let mut furnace_current_temp = 25.0;
    let mut pot_temp_f = 25.0;
    
    let target_temp = 1200.0; // Coal
    let dt = 0.016; // 60 FPS
    
    for frame in 0..10000 {
        let lerp_speed = if target_temp > furnace_current_temp { 0.1 } else { 0.02 };
        furnace_current_temp += (target_temp - furnace_current_temp) * lerp_speed * dt;
        
        let pot_target = furnace_current_temp;
        let pot_lerp = 0.02; // Original lerp
        
        let mut new_temp_f = pot_temp_f + (pot_target - pot_temp_f) * pot_lerp * dt;
        if pot_target > pot_temp_f && new_temp_f - pot_temp_f < 2.0 * dt {
            new_temp_f += 2.0 * dt;
            if new_temp_f > pot_target { new_temp_f = pot_target; }
        }
        pot_temp_f = new_temp_f;
        
        let pot_temp_u16 = pot_temp_f.floor() as u16;
        let furnace_temp_u16 = furnace_current_temp.floor() as u16;
        
        if pot_temp_u16 > furnace_temp_u16 {
            println!("Frame {}: Pot {} > Furnace {}", frame, pot_temp_u16, furnace_temp_u16);
            break;
        }
    }
    println!("Simulation ended.");
}
