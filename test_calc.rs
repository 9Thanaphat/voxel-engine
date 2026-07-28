// Simple rust script to test melting point calculation
fn main() {
    let mass_copper = 800.0 * 64.0;
    let mass_impurity = 200.0 * 64.0;
    let total_mass = mass_copper + mass_impurity;
    
    let weighted_temp_sum = (mass_copper * 1085.0) + (mass_impurity * 800.0);
    let avg_melting_point = (weighted_temp_sum / total_mass) as u16;
    
    println!("Total Mass: {}", total_mass);
    println!("Weighted Temp Sum: {}", weighted_temp_sum);
    println!("Avg Melting Point: {}", avg_melting_point);
}
