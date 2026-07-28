use bevy::prelude::*;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use std::time::SystemTime;
use std::fs::File;
use std::io::Write;

use crate::camera::FreeCamera;
use crate::voxel::VoxelWorld;

#[derive(Resource)]
pub struct BenchmarkState {
    pub start_time: f64,
    pub is_started: bool,
    pub frame_times: Vec<f64>,
    pub max_duration_seconds: f64,
}

impl Default for BenchmarkState {
    fn default() -> Self {
        Self {
            start_time: 0.0,
            is_started: false,
            frame_times: Vec::with_capacity(3600), // pre-allocate for 60 seconds at 60fps
            max_duration_seconds: 65.0, // 5s warmup + 60s benchmark
        }
    }
}

pub struct BenchmarkPlugin {
    pub enabled: bool,
}

impl Plugin for BenchmarkPlugin {
    fn build(&self, app: &mut App) {
        if !self.enabled {
            return;
        }
        app.init_resource::<BenchmarkState>()
           .add_systems(Startup, benchmark_trigger_ingame)
           .add_systems(Update, (
               benchmark_setup,
               benchmark_camera_movement,
               collect_metrics,
               check_benchmark_end,
           ).run_if(in_state(crate::GameState::InGame)));
    }
}

fn benchmark_trigger_ingame(mut next_state: ResMut<NextState<crate::GameState>>) {
    next_state.set(crate::GameState::InGame);
}

fn benchmark_setup(
    mut state: ResMut<BenchmarkState>,
    time: Res<Time>,
    mut initialized: Local<bool>,
) {
    if !*initialized {
        state.start_time = time.elapsed_secs_f64();
        state.is_started = true;
        *initialized = true;
    }
}

fn benchmark_camera_movement(
    mut query: Query<&mut Transform, With<FreeCamera>>,
    time: Res<Time>,
) {
    let speed = 60.0; // Increased speed to fly faster
    for mut transform in query.iter_mut() {
        // Force perfectly level rotation and fixed high Y
        transform.rotation = Quat::IDENTITY;
        transform.translation.y = 150.0;
        // Move forward (-Z)
        transform.translation += Vec3::NEG_Z * speed * time.delta_secs();
    }
}

fn collect_metrics(
    mut state: ResMut<BenchmarkState>,
    diagnostics: Res<DiagnosticsStore>,
    time: Res<Time>,
) {
    if !state.is_started {
        return;
    }
    // Skip the first 5 seconds (Warm-up phase) to ignore shader compilation and initial spawn stutter
    let elapsed = time.elapsed_secs_f64() - state.start_time;
    if elapsed < 5.0 {
        return;
    }

    if let Some(fps_diag) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME) {
        if let Some(value) = fps_diag.value() {
            // value is in milliseconds
            state.frame_times.push(value);
        }
    }
}

fn check_benchmark_end(
    state: Res<BenchmarkState>,
    time: Res<Time>,
    world: Option<Res<VoxelWorld>>,
    mut app_exit_events: MessageWriter<bevy::app::AppExit>,
) {
    if !state.is_started {
        return;
    }
    let elapsed = time.elapsed_secs_f64() - state.start_time;
    if elapsed >= state.max_duration_seconds {
        // Benchmark is done
        
        let mut sorted_times = state.frame_times.clone();
        sorted_times.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)); // Sort descending (worst frame times first)
        
        let total_frames = sorted_times.len();
        
        if total_frames > 0 {
            let avg_frame_time = sorted_times.iter().sum::<f64>() / total_frames as f64;
            let avg_fps = 1000.0 / avg_frame_time;
            
            // 1% Low is the average of the worst 1% of frames
            let one_percent_count = (total_frames as f64 * 0.01).ceil() as usize;
            let one_percent_count = one_percent_count.max(1);
            let one_percent_avg_time = sorted_times.iter().take(one_percent_count).sum::<f64>() / one_percent_count as f64;
            let fps_1_percent_low = 1000.0 / one_percent_avg_time;
            
            // 0.1% Low
            let zero_one_percent_count = (total_frames as f64 * 0.001).ceil() as usize;
            let zero_one_percent_count = zero_one_percent_count.max(1);
            let zero_one_percent_avg_time = sorted_times.iter().take(zero_one_percent_count).sum::<f64>() / zero_one_percent_count as f64;
            let fps_0_1_percent_low = 1000.0 / zero_one_percent_avg_time;
            
            let chunks_generated = world.map(|w| w.chunks.len()).unwrap_or(0);
            
            let json_output = format!(
                "{{\n  \"duration_seconds\": {:.2},\n  \"total_frames\": {},\n  \"fps_average\": {:.2},\n  \"fps_1_percent_low\": {:.2},\n  \"fps_0_1_percent_low\": {:.2},\n  \"chunks_generated\": {}\n}}\n",
                elapsed - 5.0, total_frames, avg_fps, fps_1_percent_low, fps_0_1_percent_low, chunks_generated
            );
            
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let filename = format!("benchmark_results_{}.json", timestamp);
            if let Ok(mut file) = File::create(&filename) {
                let _ = file.write_all(json_output.as_bytes());
                println!("Benchmark results saved to {}", filename);
            }
        }
        
        app_exit_events.write(bevy::app::AppExit::Success);
    }
}
