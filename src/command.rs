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
    "/time <0-24> - set time of day (host only)",
    "/daynight <speed> - day-night cycle speed (1 = normal, 0 = frozen, host only)",
    "/weather <clear|rain|snow> [intensity] - set weather (host only)",
    "/seed - show the world seed",
];

/// ทุกอย่างที่คำสั่งอาจต้องแตะ — รวมเป็น SystemParam ก้อนเดียวไม่ให้ signature บาน
#[derive(bevy::ecs::system::SystemParam)]
pub struct CommandWorld<'w, 's> {
    pub settings: ResMut<'w, crate::GameSettings>,
    pub hotbar: ResMut<'w, crate::voxel::Hotbar>,
    pub pending: ResMut<'w, crate::network::PendingNetEdits>,
    pub incoming: ResMut<'w, crate::network::IncomingNetEdits>,
    pub camera: Query<'w, 's, &'static mut Transform, With<crate::camera::FreeCamera>>,
    pub weather: ResMut<'w, crate::weather::Weather>,
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
        "tp" => cmd_tp(&args, chat, world),
        "gamemode" => cmd_gamemode(&args, chat, world),
        "give" => cmd_give(&args, chat, world),
        "time" => cmd_time(&args, chat, world, server, is_client),
        "daynight" => cmd_daynight(&args, chat, world, is_client),
        "weather" => cmd_weather(&args, chat, world, server, is_client),
        "setblock" => cmd_setblock(&args, chat, world, is_client),
        other => chat.push_error(format!("unknown command '{other}' - try /help")),
    }
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
        2 => {
            let (Ok(lat), Ok(lon)) = (args[0].parse::<f64>(), args[1].parse::<f64>()) else {
                chat.push_error("usage: /tp <lat> <lon> (numbers)");
                return;
            };
            let Some(dem) = crate::dem::streamer() else {
                chat.push_error("GPS teleport needs the real-world map");
                return;
            };
            if !dem.has_tile_at(lat, lon) {
                chat.push_error(format!("{lat:.4}, {lon:.4} is outside the loaded tiles"));
                return;
            }
            let (bx, bz) = crate::dem::latlon_to_block(lat, lon);
            // โหลด tile ปลายทางแบบ blocking ก่อน ไม่งั้นได้ความสูงระดับทะเล
            dem.load_blocking_at(bx, bz);
            let h = crate::dem::DEM_SEA_LEVEL_Y as f32 + dem.elevation_at_block(bx, bz);
            transform.translation = Vec3::new(bx as f32, h + 20.0, bz as f32);
            chat.push_system(format!("Teleported to {lat:.4}, {lon:.4} (surface {h:.0} m)"));
        }
        _ => chat.push_error("usage: /tp <x> <y> <z>  or  /tp <lat> <lon>"),
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
    // เปลี่ยนโหมดล้าง inventory ตาม behaviour เดิมของ settings radio
    *world.hotbar = crate::voxel::Hotbar::for_mode(mode);
    chat.push_system(format!("Game mode: {mode:?} (inventory reset)"));
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
    server: Option<&mut bevy_renet::RenetServer>,
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
    world.settings.time_of_day = hours;
    // เดิม time sync แค่ครั้งเดียวตอน Welcome — ต้อง broadcast เองถึงจะถึง client ที่ต่ออยู่
    if let Some(server) = server {
        server.broadcast_message(
            bevy_renet::renet::DefaultChannel::ReliableOrdered,
            crate::network::encode(&crate::network::ServerMessage::TimeOfDay { hours }),
        );
    }
    chat.push_system(format!("Time set to {hours:.1}"));
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
