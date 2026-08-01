use bevy::prelude::*;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use std::time::SystemTime;
use std::fs::File;
use std::io::Write;

use crate::camera::FreeCamera;
use crate::voxel::{ChunkGenerator, VoxelWorld};

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

    // The diagnostic can be unavailable for a frame depending on system order.
    // Falling back to Time keeps headless/CI benchmark runs from producing no file.
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|diagnostic| diagnostic.value())
        .unwrap_or_else(|| time.delta_secs_f64() * 1000.0);
    state.frame_times.push(frame_ms);
}

fn check_benchmark_end(
    state: Res<BenchmarkState>,
    time: Res<Time>,
    world: Option<Res<VoxelWorld>>,
    generator: Option<Res<ChunkGenerator>>,
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
        if sorted_times.is_empty() {
            sorted_times.push(time.delta_secs_f64() * 1000.0);
        }
        
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
            
            let chunks_loaded = world.as_ref().map(|w| w.chunks.len()).unwrap_or(0);
            let meshes_visible = world.as_ref().map(|w| w.generated_chunks.len()).unwrap_or(0);
            let (
                block_avg_ms,
                light_avg_ms,
                mesh_avg_ms,
                block_integrate_ms,
                light_integrate_ms,
                mesh_integrate_ms,
                max_pending_blocks,
                max_pending_lights,
                max_pending_meshes,
                visible_latency_avg_ms,
                visible_latency_p95_ms,
            ) = generator.as_ref().map_or(
                (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0, 0, 0, 0.0, 0.0),
                |g| {
                    let s = &g.stats;
                    let avg = |micros: u64, jobs: u64| {
                        if jobs == 0 { 0.0 } else { micros as f64 / jobs as f64 / 1000.0 }
                    };
                    let mut visible = s.visible_latency_micros.clone();
                    visible.sort_unstable();
                    let visible_avg = if visible.is_empty() {
                        0.0
                    } else {
                        visible.iter().sum::<u64>() as f64 / visible.len() as f64 / 1000.0
                    };
                    let visible_p95 = if visible.is_empty() {
                        0.0
                    } else {
                        let index = ((visible.len() - 1) as f64 * 0.95).round() as usize;
                        visible[index] as f64 / 1000.0
                    };
                    (
                        avg(s.block_work_micros, s.block_jobs),
                        avg(s.light_work_micros, s.light_jobs),
                        avg(s.mesh_work_micros, s.mesh_jobs),
                        s.block_integrate_micros as f64 / 1000.0,
                        s.light_integrate_micros as f64 / 1000.0,
                        s.mesh_integrate_micros as f64 / 1000.0,
                        s.max_pending_blocks,
                        s.max_pending_lights,
                        s.max_pending_meshes,
                        visible_avg,
                        visible_p95,
                    )
                },
            );
            
            let json_output = format!(
                concat!(
                    "{{\n",
                    "  \"duration_seconds\": {:.2},\n",
                    "  \"total_frames\": {},\n",
                    "  \"fps_average\": {:.2},\n",
                    "  \"fps_1_percent_low\": {:.2},\n",
                    "  \"fps_0_1_percent_low\": {:.2},\n",
                    "  \"chunks_loaded\": {},\n",
                    "  \"meshes_visible\": {},\n",
                    "  \"block_job_average_ms\": {:.3},\n",
                    "  \"light_job_average_ms\": {:.3},\n",
                    "  \"mesh_job_average_ms\": {:.3},\n",
                    "  \"block_integrate_total_ms\": {:.3},\n",
                    "  \"light_integrate_total_ms\": {:.3},\n",
                    "  \"mesh_integrate_total_ms\": {:.3},\n",
                    "  \"max_pending_blocks\": {},\n",
                    "  \"max_pending_lights\": {},\n",
                    "  \"max_pending_meshes\": {},\n",
                    "  \"visible_latency_average_ms\": {:.3},\n",
                    "  \"visible_latency_p95_ms\": {:.3}\n",
                    "}}\n"
                ),
                elapsed - 5.0,
                total_frames,
                avg_fps,
                fps_1_percent_low,
                fps_0_1_percent_low,
                chunks_loaded,
                meshes_visible,
                block_avg_ms,
                light_avg_ms,
                mesh_avg_ms,
                block_integrate_ms,
                light_integrate_ms,
                mesh_integrate_ms,
                max_pending_blocks,
                max_pending_lights,
                max_pending_meshes,
                visible_latency_avg_ms,
                visible_latency_p95_ms,
            );
            
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let filename = format!("benchmark_results_{}.json", timestamp);
            if let Ok(mut file) = File::create(&filename) {
                let _ = file.write_all(json_output.as_bytes());
                println!("Benchmark results saved to {}", filename);
                println!("{}", json_output);
            } else {
                eprintln!("Could not create benchmark result file: {}", filename);
                eprintln!("{}", json_output);
            }
        }
        
        app_exit_events.write(bevy::app::AppExit::Success);
    }
}
