//! คำสั่งในแชท (ขึ้นต้นด้วย `/`)
//!
//! แชทที่ผู้ใช้ส่งเข้าคิว [`CommandQueue`] แล้ว [`run_commands`] drain ทีละบรรทัด
//! เพิ่มคำสั่งใหม่ = เพิ่ม arm ใน match ของ `dispatch` + 1 บรรทัดใน [`HELP`]
//!
//! สิทธิ์: คำสั่งที่กระทบโลกหรือคนอื่น (`/time`, `/setblock`) host เท่านั้น ตาม
//! สถาปัตยกรรม host-authoritative เดิม — client สั่งแล้วได้ error ไม่ส่งข้ามไปให้ host
//! ส่วน `/gamemode` กับ `/give` เป็น client-local อยู่แล้ว (ดูคอมเมนต์ GameMode ใน main.rs)

use bevy::prelude::*;

/// บรรทัดที่ผู้ใช้กด Enter ส่ง (ทั้งแชทธรรมดาและคำสั่ง) รอ run_commands จัดการ
#[derive(Resource, Default)]
pub struct CommandQueue(pub std::collections::VecDeque<String>);

const HELP: &[&str] = &[
    "/help - show this list",
    "/tp <x> <y> <z> - teleport to block coords",
    "/tp <lat> <lon> - teleport by GPS (real world only)",
    "/gamemode <creative|survival> - switch mode (this client only)",
    "/give <block|tool> [count] - put an item in the selected slot (tools: pickaxe, axe, shovel, chisel, wire)",
    "/setblock <x> <y> <z> <block> - place a block (host only)",
    "/time <0-24> - fast-forward to time of day (host only)",
    "/set day <1-365> - set day of year (host only)",
    "/date <0-364> - set day of year (0 = spring equinox, host only)",
    "/daynight <speed> - day-night cycle speed (1 = normal, 0 = frozen, host only)",
    "/weather <clear|rain|snow> [intensity] - set weather (host only)",
    "/seed - show the world seed",
    "/locate <volcano|hydrothermal> - find the nearest volcanic landmark",
    "/volcano <status|erupt|unrest|cooling|dormant> - inspect/control nearest volcano (host only)",
    "/chunkborders - toggle chunk boundary grid (debug, this client only)",
    "/waterflow - toggle water flow direction arrows (debug, this client only)",
    "/xray [on|off] - toggle terrain X-ray + fullbright (debug, this client only)",
    "/fog <clear|morning|dense> - change fog atmosphere (this client only)",
];

/// ทุกอย่างที่คำสั่งอาจต้องแตะ — รวมเป็น SystemParam ก้อนเดียวไม่ให้ signature บาน
#[derive(bevy::ecs::system::SystemParam)]
pub struct CommandWorld<'w, 's> {
    pub settings: ResMut<'w, crate::GameSettings>,
    pub fast_time: ResMut<'w, crate::voxel::TimeFastForward>,
    pub hotbar: ResMut<'w, crate::voxel::Hotbar>,
    pub pending: ResMut<'w, crate::network::PendingNetEdits>,
    pub incoming: ResMut<'w, crate::network::IncomingNetEdits>,
    pub camera: Query<'w, 's, &'static mut Transform, With<crate::camera::FreeCamera>>,
    pub weather: ResMut<'w, crate::weather::Weather>,
    pub auto_weather: ResMut<'w, crate::weather::AutoWeather>,
    pub fog: ResMut<'w, crate::camera::FogState>,
    pub voxel_world: ResMut<'w, crate::voxel::VoxelWorld>,
    pub volcanoes: ResMut<'w, crate::volcanism::VolcanoRegistry>,
    pub volcano_events: ResMut<'w, crate::network::PendingVolcanoEvents>,
}

pub fn run_commands(
    mut queue: ResMut<CommandQueue>,
    mut chat: ResMut<crate::ui::ChatState>,
    mut world: CommandWorld,
    mut server: Option<ResMut<bevy_renet::RenetServer>>,
    mut client: Option<ResMut<bevy_renet::RenetClient>>,
) {
    while let Some(line) = queue.0.pop_front() {
        if let Some(cmd) = line.strip_prefix('/') {
            dispatch(cmd, &mut chat, &mut world, server.as_deref_mut(), client.is_some());
        } else {
            send_chat(&line, &mut chat, server.as_deref_mut(), client.as_deref_mut());
        }
    }
}

/// แชทธรรมดา: host กระจายเอง, client ส่งให้ host กระจาย, single player ขึ้นจอตัวเอง
fn send_chat(
    text: &str,
    chat: &mut crate::ui::ChatState,
    server: Option<&mut bevy_renet::RenetServer>,
    client: Option<&mut bevy_renet::RenetClient>,
) {
    use crate::network::{encode, ClientMessage, ServerMessage};
    use bevy_renet::renet::DefaultChannel;

    if let Some(client) = client {
        // ไม่ขึ้นจอตัวเองตรงนี้ — รอ host ส่งกลับมา ลำดับข้อความจะได้ตรงกันทุกจอ
        client.send_message(
            DefaultChannel::ReliableOrdered,
            encode(&ClientMessage::Chat { text: text.to_string() }),
        );
    } else if let Some(server) = server {
        // host = Player 1
        server.broadcast_message(
            DefaultChannel::ReliableOrdered,
            encode(&ServerMessage::Chat { from: 1, text: text.to_string() }),
        );
        chat.push_player(1, text.to_string());
    } else {
        chat.push_player(1, text.to_string());
    }
}

fn dispatch(
    cmd: &str,
    chat: &mut crate::ui::ChatState,
    world: &mut CommandWorld,
    server: Option<&mut bevy_renet::RenetServer>,
    is_client: bool,
) {
    let mut parts = cmd.split_whitespace();
    let Some(name) = parts.next() else {
        chat.push_error("empty command - try /help");
        return;
    };
    let args: Vec<&str> = parts.collect();

    match name.to_ascii_lowercase().as_str() {
        "help" => {
            for line in HELP {
                chat.push_system(*line);
            }
        }
        "seed" => chat.push_system(format!("Seed: {}", world.settings.noise.seed)),
        "locate" => cmd_locate(&args, chat, world),
        "volcano" => cmd_volcano(&args, chat, world, is_client),
        "tp" => cmd_tp(&args, chat, world),
        "gamemode" => cmd_gamemode(&args, chat, world),
        "give" => cmd_give(&args, chat, world),
        "time" => cmd_time(&args, chat, world, is_client),
        "set" if args
            .first()
            .is_some_and(|arg| arg.eq_ignore_ascii_case("day")) =>
        {
            cmd_set_day(&args[1..], chat, world, server, is_client)
        }
        "setday" | "day" => cmd_set_day(&args, chat, world, server, is_client),
        "date" => cmd_date(&args, chat, world, server, is_client),
        "daynight" => cmd_daynight(&args, chat, world, is_client),
        "weather" => cmd_weather(&args, chat, world, server, is_client),
        "setblock" => cmd_setblock(&args, chat, world, is_client),
        "chunkborders" => {
            // debug local — สลับกริดขอบเขต chunk (ดู voxel::chunk_border_gizmo_system)
            world.settings.show_chunk_borders = !world.settings.show_chunk_borders;
            let state = if world.settings.show_chunk_borders { "on" } else { "off" };
            chat.push_system(format!("Chunk borders {state}"));
        }
        "waterflow" | "flowdebug" => {
            world.settings.show_water_flow = !world.settings.show_water_flow;
            let state = if world.settings.show_water_flow { "on" } else { "off" };
            chat.push_system(format!("Water flow debug arrows {state}"));
        }
        "xray" => cmd_xray(&args, chat, world),
        "fog" => cmd_fog(&args, chat, world),
        other => chat.push_error(format!("unknown command '{other}' - try /help")),
    }
}

fn nearest_generated_volcano(
    settings: &crate::GameSettings,
    origin: Vec3,
) -> Option<crate::volcanism::VolcanoDescriptor> {
    let sampler = crate::voxel::TerrainSampler::new(settings.noise);
    crate::volcanism::volcanoes_nearby(
        settings.noise.seed,
        origin.x as f64,
        origin.z as f64,
        32,
    )
    .into_iter()
    .filter(|volcano| {
        sampler
            .volcano_sample(volcano.center.x, volcano.center.y)
            .descriptor
            .is_some()
    })
    .min_by(|a, b| {
        let distance_sq = |volcano: &crate::volcanism::VolcanoDescriptor| {
            let dx = volcano.center.x - origin.x as f64;
            let dz = volcano.center.y - origin.z as f64;
            dx * dx + dz * dz
        };
        distance_sq(a).total_cmp(&distance_sq(b))
    })
}

fn volcano_state_name(state: crate::volcanism::VolcanoState) -> &'static str {
    match state {
        crate::volcanism::VolcanoState::Dormant => "dormant",
        crate::volcanism::VolcanoState::Unrest => "unrest",
        crate::volcanism::VolcanoState::Erupting => "erupting",
        crate::volcanism::VolcanoState::Cooling => "cooling",
    }
}

fn cmd_volcano(
    args: &[&str],
    chat: &mut crate::ui::ChatState,
    world: &mut CommandWorld,
    is_client: bool,
) {
    let Some(action) = args.first().map(|arg| arg.to_ascii_lowercase()) else {
        chat.push_error("usage: /volcano <status|erupt|unrest|cooling|dormant>");
        return;
    };
    if args.len() != 1 {
        chat.push_error("usage: /volcano <status|erupt|unrest|cooling|dormant>");
        return;
    }
    let Some(camera) = world.camera.iter().next() else {
        chat.push_error("no camera position available");
        return;
    };
    let origin = camera.translation;
    let Some(volcano) = nearest_generated_volcano(&world.settings, origin) else {
        chat.push_error("no volcano found within 50,000 blocks");
        return;
    };
    let dx = volcano.center.x - origin.x as f64;
    let dz = volcano.center.y - origin.z as f64;
    let distance = (dx * dx + dz * dz).sqrt().round() as i32;

    if action == "status" {
        let (state, seconds) = world
            .volcanoes
            .active
            .get(&volcano.id)
            .map_or((crate::volcanism::VolcanoState::Dormant, 0.0), |runtime| {
                (runtime.state, runtime.seconds_in_state)
            });
        chat.push_system(format!(
            "Nearest volcano is {} ({seconds:.0}s in state), {distance} blocks away at X={} Z={}",
            volcano_state_name(state),
            volcano.center.x.round() as i32,
            volcano.center.y.round() as i32,
        ));
        return;
    }
    if is_client {
        chat.push_error("/volcano state changes are host only");
        return;
    }

    let state = match action.as_str() {
        "erupt" | "erupting" => crate::volcanism::VolcanoState::Erupting,
        "unrest" => crate::volcanism::VolcanoState::Unrest,
        "cool" | "cooling" => crate::volcanism::VolcanoState::Cooling,
        "dormant" | "stop" => crate::volcanism::VolcanoState::Dormant,
        _ => {
            chat.push_error("usage: /volcano <status|erupt|unrest|cooling|dormant>");
            return;
        }
    };
    world.volcanoes.active.insert(
        volcano.id,
        crate::volcanism::VolcanoRuntime {
            id: volcano.id,
            state,
            seconds_in_state: 0.0,
            pulse_seconds: 0.0,
        },
    );
    world.volcano_events.0.push(crate::network::VolcanoEventWire {
        id: volcano.id,
        state,
        center: [volcano.center.x, volcano.center.y],
    });
    chat.push_system(format!(
        "Set nearest volcano to {} at X={} Z={} ({distance} blocks away)",
        volcano_state_name(state),
        volcano.center.x.round() as i32,
        volcano.center.y.round() as i32,
    ));
}

fn cmd_locate(
    args: &[&str],
    chat: &mut crate::ui::ChatState,
    world: &mut CommandWorld,
) {
    let Some(kind) = args.first().map(|arg| arg.to_ascii_lowercase()) else {
        chat.push_error("usage: /locate <volcano|hydrothermal>");
        return;
    };
    if args.len() != 1 || !matches!(kind.as_str(), "volcano" | "hydrothermal" | "acid") {
        chat.push_error("usage: /locate <volcano|hydrothermal>");
        return;
    }
    let Some(camera) = world.camera.iter().next() else {
        chat.push_error("no camera position available");
        return;
    };

    let origin = camera.translation;
    let sampler = crate::voxel::TerrainSampler::new(world.settings.noise);
    let candidates = crate::volcanism::volcanoes_nearby(
        world.settings.noise.seed,
        origin.x as f64,
        origin.z as f64,
        32,
    );

    let mut nearest: Option<(f64, f64, f64)> = None;
    for volcano in candidates {
        if sampler
            .volcano_sample(volcano.center.x, volcano.center.y)
            .descriptor
            .is_none()
        {
            continue;
        }
        let targets: Vec<_> = if kind == "volcano" {
            vec![volcano.center]
        } else {
            crate::volcanism::hydrothermal_pool_centers(volcano)
                .into_iter()
                .filter(|center| {
                    sampler
                        .hydrothermal_sample(center.x, center.y)
                        .pool
                        > 0.9
                })
                .collect()
        };
        for target in targets {
            let dx = target.x - origin.x as f64;
            let dz = target.y - origin.z as f64;
            let distance_sq = dx * dx + dz * dz;
            if nearest.is_none_or(|(best, _, _)| distance_sq < best) {
                nearest = Some((distance_sq, target.x, target.y));
            }
        }
    }

    let Some((distance_sq, x, z)) = nearest else {
        chat.push_error("no matching landmark found within 50,000 blocks");
        return;
    };
    let y = sampler.height(x, z) + 2;
    let label = if kind == "volcano" {
        "volcano"
    } else {
        "hydrothermal pool"
    };
    chat.push_system(format!(
        "Nearest {label}: X={} Y={y} Z={} ({} blocks) — /tp {} {y} {}",
        x.round() as i32,
        z.round() as i32,
        distance_sq.sqrt().round() as i32,
        x.round() as i32,
        z.round() as i32,
    ));
}

fn cmd_xray(args: &[&str], chat: &mut crate::ui::ChatState, world: &mut CommandWorld) {
    use std::sync::atomic::Ordering;

    let current = crate::voxel::DEBUG_XRAY_ENABLED.load(Ordering::Relaxed);
    let enabled = match args.first().map(|arg| arg.to_ascii_lowercase()) {
        None => !current,
        Some(value) if value == "on" || value == "true" || value == "1" => true,
        Some(value) if value == "off" || value == "false" || value == "0" => false,
        _ => {
            chat.push_error("usage: /xray [on|off]");
            return;
        }
    };
    if enabled == current {
        chat.push_system(format!("X-ray already {}", if enabled { "on" } else { "off" }));
        return;
    }

    crate::voxel::DEBUG_XRAY_ENABLED.store(enabled, Ordering::Relaxed);
    let loaded: Vec<IVec2> = world.voxel_world.chunks.keys().copied().collect();
    world.voxel_world.pending_branch_remesh.extend(loaded);
    chat.push_system(format!(
        "X-ray {} — remeshing loaded chunks",
        if enabled { "on (terrain hidden, fullbright)" } else { "off" }
    ));
}

/// 3 args = พิกัดบล็อก, 2 args = lat/lon (ใช้เส้นทางเดียวกับ GPS teleport ใน settings)
fn cmd_tp(args: &[&str], chat: &mut crate::ui::ChatState, world: &mut CommandWorld) {
    let Some(mut transform) = world.camera.iter_mut().next() else {
        chat.push_error("no camera to teleport");
        return;
    };
    match args.len() {
        3 => {
            let coords: Option<Vec<f32>> = args.iter().map(|a| a.parse::<f32>().ok()).collect();
            match coords {
                Some(c) => {
                    transform.translation = Vec3::new(c[0], c[1], c[2]);
                    chat.push_system(format!("Teleported to {:.0} {:.0} {:.0}", c[0], c[1], c[2]));
                }
                None => chat.push_error("usage: /tp <x> <y> <z> (numbers)"),
            }
        }
        _ => chat.push_error("usage: /tp <x> <y> <z>"),
    }
}

fn cmd_gamemode(args: &[&str], chat: &mut crate::ui::ChatState, world: &mut CommandWorld) {
    let mode = match args.first().map(|a| a.to_ascii_lowercase()) {
        Some(m) if m == "creative" || m == "c" => crate::GameMode::Creative,
        Some(m) if m == "survival" || m == "s" => crate::GameMode::Survival,
        _ => {
            chat.push_error("usage: /gamemode <creative|survival>");
            return;
        }
    };
    world.settings.game_mode = mode;
    chat.push_system(format!("Game mode: {mode:?}"));
}

fn cmd_fog(args: &[&str], chat: &mut crate::ui::ChatState, world: &mut CommandWorld) {
    let Some(mode) = args.first().map(|a| a.to_ascii_lowercase()) else {
        chat.push_error("usage: /fog <clear|morning|dense>");
        return;
    };
    
    match mode.as_str() {
        "clear" => {
            world.fog.target_color = Srgba::new(0.72, 0.80, 0.90, 1.0);
            world.fog.auto_distance = true;
            chat.push_system("Fog: Clear atmosphere (matches render distance)");
        }
        "morning" => {
            world.fog.target_color = Srgba::new(0.85, 0.90, 0.95, 1.0);
            world.fog.target_start = 10.0;
            world.fog.target_end = 200.0;
            world.fog.auto_distance = false;
            chat.push_system("Fog: Morning mist (medium range)");
        }
        "dense" => {
            world.fog.target_color = Srgba::new(0.60, 0.65, 0.70, 1.0);
            world.fog.target_start = 0.0;
            world.fog.target_end = 30.0;
            world.fog.auto_distance = false;
            chat.push_system("Fog: Dense Silent-Hill (close range)");
        }
        _ => chat.push_error("usage: /fog <clear|morning|dense>"),
    }
}

/// ชื่อ tool ที่ /give รับ — แยกจาก block_from_name เพราะ tool ไม่อยู่ใน BLOCK_DEFS
fn tool_from_name(name: &str) -> Option<crate::item::ToolType> {
    use crate::item::ToolType;
    match name.to_ascii_lowercase().as_str() {
        "pickaxe" | "pick" => Some(ToolType::Pickaxe),
        "axe" => Some(ToolType::Axe),
        "shovel" => Some(ToolType::Shovel),
        "chisel" => Some(ToolType::Chisel),
        "wire" | "copper_wire" | "copperwire" => Some(ToolType::CopperWire),
        _ => None,
    }
}

fn cmd_give(args: &[&str], chat: &mut crate::ui::ChatState, world: &mut CommandWorld) {
    let Some(name) = args.first() else {
        chat.push_error("usage: /give <block|tool> [count]");
        return;
    };
    // เช็คชื่อ tool ก่อน ไม่เจอค่อยลองเป็นบล็อก
    let item = if let Some(tool) = tool_from_name(name) {
        crate::item::Item::Tool(tool)
    } else if let Some(block) = crate::voxel::block_from_name(name) {
        crate::item::Item::Block(block)
    } else {
        chat.push_error(format!("unknown block or tool '{name}'"));
        return;
    };
    let max = crate::voxel::max_stack(item);
    let count = match args.get(1) {
        Some(c) => match c.parse::<u32>() {
            Ok(n) if n > 0 => n.min(max),
            _ => {
                chat.push_error(format!("count must be 1-{max}"));
                return;
            }
        },
        None => max,
    };
    let sel = world.hotbar.selected;
    world.hotbar.slots[sel] = Some(crate::voxel::ItemStack { item, count: Some(count) });
    chat.push_system(format!("Gave {count} x {} to slot {}", item.name(), sel + 1));
}

fn cmd_time(
    args: &[&str],
    chat: &mut crate::ui::ChatState,
    world: &mut CommandWorld,
    is_client: bool,
) {
    if is_client {
        chat.push_error("/time is host only");
        return;
    }
    let Some(hours) = args.first().and_then(|a| a.parse::<f32>().ok()) else {
        chat.push_error("usage: /time <0-24>");
        return;
    };
    if !(0.0..=24.0).contains(&hours) {
        chat.push_error("time must be between 0 and 24");
        return;
    }
    let target = hours.rem_euclid(24.0);
    world.fast_time.start(target);
    chat.push_system(format!("Fast-forwarding to {target:.1}"));
}

/// broadcast เวลา+ปฏิทินปัจจุบันให้ client ที่ต่ออยู่ (time sync ไม่มี periodic — ต้องยิงเอง)
pub(crate) fn broadcast_time(
    server: Option<&mut bevy_renet::RenetServer>,
    settings: &crate::GameSettings,
) {
    if let Some(server) = server {
        server.broadcast_message(
            bevy_renet::renet::DefaultChannel::ReliableOrdered,
            crate::network::encode(&crate::network::ServerMessage::TimeOfDay {
                hours: settings.time_of_day,
                day_of_year: settings.day_of_year,
                year: settings.year,
            }),
        );
    }
}

fn cmd_set_day(
    args: &[&str],
    chat: &mut crate::ui::ChatState,
    world: &mut CommandWorld,
    server: Option<&mut bevy_renet::RenetServer>,
    is_client: bool,
) {
    if is_client {
        chat.push_error("/set day is host only");
        return;
    }
    let Some(day) = args
        .first()
        .and_then(|value| value.parse::<u16>().ok())
        .and_then(one_based_day_to_internal)
    else {
        chat.push_error("usage: /set day <1-365>");
        return;
    };

    world.settings.day_of_year = day;
    crate::voxel::CURRENT_DAY_OF_YEAR
        .store(day as u32, std::sync::atomic::Ordering::Relaxed);
    broadcast_time(server, &world.settings);
    chat.push_system(format!("Day set to {}", day + 1));
}

#[cfg(test)]
mod tests {
    use super::one_based_day_to_internal;

    #[test]
    fn set_day_uses_player_facing_one_based_days() {
        assert_eq!(one_based_day_to_internal(1), Some(0));
        assert_eq!(one_based_day_to_internal(365), Some(364));
        assert_eq!(one_based_day_to_internal(0), None);
        assert_eq!(one_based_day_to_internal(366), None);
    }
}

fn one_based_day_to_internal(day: u16) -> Option<u16> {
    (1..=365).contains(&day).then(|| day - 1)
}

/// ตั้งวันในปี 0..364 (0 = วสันตวิษุวัต) — คุมฤดู/ท้องฟ้ากลางคืน host only เหมือน /time
fn cmd_date(
    args: &[&str],
    chat: &mut crate::ui::ChatState,
    world: &mut CommandWorld,
    server: Option<&mut bevy_renet::RenetServer>,
    is_client: bool,
) {
    if is_client {
        chat.push_error("/date is host only");
        return;
    }
    let dpy = crate::astro::DAYS_PER_YEAR as i32;
    let Some(day) = args.first().and_then(|a| a.parse::<i32>().ok()) else {
        chat.push_error("usage: /date <0-364>  (0 = spring equinox)");
        return;
    };
    if !(0..dpy).contains(&day) {
        chat.push_error(format!("date must be between 0 and {}", dpy - 1));
        return;
    }
    world.settings.day_of_year = day as u16;
    crate::voxel::CURRENT_DAY_OF_YEAR
        .store(day as u32, std::sync::atomic::Ordering::Relaxed);
    broadcast_time(server, &world.settings);
    chat.push_system(format!("Date set to day {day} of the year"));
}

/// ปรับความเร็วรอบวัน-คืน — host only เพราะมีแต่ host/single ที่เดินเวลาเอง
/// (client รับเวลาจาก host ผ่าน sync ไม่ได้เดินเอง จึงตั้งเองไม่มีผล)
fn cmd_daynight(
    args: &[&str],
    chat: &mut crate::ui::ChatState,
    world: &mut CommandWorld,
    is_client: bool,
) {
    if is_client {
        chat.push_error("/daynight is host only");
        return;
    }
    let Some(speed) = args.first().and_then(|a| a.parse::<f32>().ok()) else {
        chat.push_error("usage: /daynight <speed>  (1 = normal, 2 = twice as fast, 0 = frozen)");
        return;
    };
    if !(0.0..=1000.0).contains(&speed) {
        chat.push_error("speed must be between 0 and 1000");
        return;
    }
    world.settings.day_speed = speed;
    if speed == 0.0 {
        chat.push_system("Day-night cycle frozen".to_string());
    } else {
        // GAME_DAY_SECONDS = 1200 วิ (20 นาที) ที่ speed 1.0
        let minutes = 1200.0 / speed / 60.0;
        chat.push_system(format!("Day-night speed x{speed} ({minutes:.1} min per day)"));
    }
}

fn cmd_weather(
    args: &[&str],
    chat: &mut crate::ui::ChatState,
    world: &mut CommandWorld,
    server: Option<&mut bevy_renet::RenetServer>,
    is_client: bool,
) {
    use crate::weather::WeatherKind;
    if is_client {
        chat.push_error("/weather is host only");
        return;
    }
    let kind = match args.first().map(|a| a.to_ascii_lowercase()).as_deref() {
        Some("clear") => WeatherKind::Clear,
        Some("rain") => WeatherKind::Rain,
        Some("snow") => WeatherKind::Snow,
        _ => {
            chat.push_error("usage: /weather <clear|rain|snow> [intensity 0..1]");
            return;
        }
    };
    let intensity = args
        .get(1)
        .and_then(|a| a.parse::<f32>().ok())
        .unwrap_or(0.8)
        .clamp(0.0, 1.0);
    world.weather.set(kind, intensity);
    // ค้างอากาศแมนนวลไว้ ~1 วันเกม แล้ว auto-weather ค่อยกลับมา re-roll ต่อ
    world.auto_weather.hold_for_manual(&world.settings);
    // broadcast ให้ client (host-authoritative เหมือน /time)
    if let Some(server) = server {
        let target = world.weather.target;
        server.broadcast_message(
            bevy_renet::renet::DefaultChannel::ReliableOrdered,
            crate::network::encode(&crate::network::ServerMessage::Weather { kind, intensity: target }),
        );
    }
    chat.push_system(format!("Weather: {kind:?} ({intensity:.1})"));
}

fn cmd_setblock(
    args: &[&str],
    chat: &mut crate::ui::ChatState,
    world: &mut CommandWorld,
    is_client: bool,
) {
    if is_client {
        chat.push_error("/setblock is host only");
        return;
    }
    if args.len() != 4 {
        chat.push_error("usage: /setblock <x> <y> <z> <block>");
        return;
    }
    let coords: Option<Vec<i32>> = args[..3].iter().map(|a| a.parse::<i32>().ok()).collect();
    let Some(c) = coords else {
        chat.push_error("coordinates must be whole numbers");
        return;
    };
    let Some(block) = crate::voxel::block_from_name(args[3]) else {
        chat.push_error(format!("unknown block '{}'", args[3]));
        return;
    };

    // ไหลผ่าน pipeline เดิม: incoming = ทาถึงโลกเรา, pending = broadcast ให้ client
    // (แบบเดียวกับที่ block_interaction_system ทำ)
    let edit = crate::network::BlockEdit::SetBlock {
        pos: [c[0], c[1], c[2]],
        block: block as u8,
    };
    world.incoming.0.push(edit.clone());
    world.pending.0.push_back((None, edit));
    chat.push_system(format!(
        "Set {} {} {} to {}",
        c[0],
        c[1],
        c[2],
        crate::voxel::block_name(block)
    ));
}
