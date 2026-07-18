use bevy::prelude::*;
use bevy_renet::netcode::{
    ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport, ServerAuthentication,
    ServerConfig,
};
use bevy_renet::renet::{ConnectionConfig, DefaultChannel, ServerEvent};
use bevy_renet::{RenetClient, RenetServer, RenetServerEvent};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::voxel::{BlockType, VoxelWorld, CHUNK_VOLUME, CHUNK_WIDTH};

pub const SERVER_PORT: u16 = 5000;
/// ต้องตรงกันทั้ง host และ client ไม่งั้น netcode ปฏิเสธการเชื่อมต่อ
pub const PROTOCOL_ID: u64 = 0xB10C_CAFE_0002;
/// id ตัวแทน host ในข้อความ PlayerPositions (client จริงใช้ id ที่ไม่ใช่ 0)
pub const HOST_PLAYER_ID: u64 = 0;

// ---------------------------------------------------------------------------
// Protocol
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub enum BlockEdit {
    SetBlock { pos: [i32; 3], block: u8 },
    /// ฝั่งรับต้อง convert_to_chiseled ก่อนถ้า block ยังไม่ใช่ Chiseled
    SetSubVoxel { pos: [i32; 3], sub: [u8; 3], val: u8 },
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum ServerMessage {
    Welcome {
        client_id: u64,
        /// ลำดับผู้เล่น (host = 1, client ตามลำดับ join = 2, 3, ...)
        player_number: u32,
        noise: crate::NoiseParams,
        spawn_pos: [f32; 3],
        time_of_day: f32,
    },
    ChunkData {
        chunk_pos: [i32; 2],
        blocks_rle: Vec<u8>,
        /// (block index ใน chunk, palette 4096 bytes)
        chiseled: Vec<(u32, Vec<u8>)>,
    },
    BlockEditBatch { edits: Vec<BlockEdit> },
    PlayerJoined { client_id: u64, player_number: u32 },
    PlayerLeft { client_id: u64 },
    PlayerPositions { players: Vec<(u64, [f32; 3], f32)> },
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum ClientMessage {
    RequestEdit { edits: Vec<BlockEdit> },
    Position { pos: [f32; 3], yaw: f32 },
}

pub fn encode<T: serde::Serialize>(msg: &T) -> Vec<u8> {
    bincode::serialize(msg).expect("network message serialization cannot fail")
}

pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    match bincode::deserialize(bytes) {
        Ok(v) => Some(v),
        Err(e) => {
            warn!("ทิ้ง network message ที่ decode ไม่ได้: {e}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// RLE encoding สำหรับ block array ทั้ง chunk (131072 bytes → ปกติ <5KB)
// รูปแบบ: (value: u8, run: u16 LE) ซ้ำจนครบ
// ---------------------------------------------------------------------------

pub fn rle_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut iter = data.iter().copied();
    let Some(mut current) = iter.next() else { return out };
    let mut run: u16 = 1;
    for b in iter {
        if b == current && run < u16::MAX {
            run += 1;
        } else {
            out.push(current);
            out.extend_from_slice(&run.to_le_bytes());
            current = b;
            run = 1;
        }
    }
    out.push(current);
    out.extend_from_slice(&run.to_le_bytes());
    out
}

/// คืน None ถ้าข้อมูลผิดรูปหรือความยาวรวมไม่เท่า CHUNK_VOLUME
pub fn rle_decode(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() % 3 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(CHUNK_VOLUME);
    for triple in data.chunks_exact(3) {
        let value = triple[0];
        let run = u16::from_le_bytes([triple[1], triple[2]]) as usize;
        if run == 0 || out.len() + run > CHUNK_VOLUME {
            return None;
        }
        out.resize(out.len() + run, value);
    }
    (out.len() == CHUNK_VOLUME).then_some(out)
}

// ---------------------------------------------------------------------------
// Resources / Components
// ---------------------------------------------------------------------------

/// สถานะหน้าจอ multiplayer menu + section ใน settings (address ที่พิมพ์, ข้อความ status)
#[derive(Resource, Default)]
pub struct MultiplayerUi {
    pub address: String,
    pub status: String,
}

/// IP:port ที่โชว์ใน UI ตอนเป็น host (คำนวณครั้งเดียวตอน start_host)
#[derive(Resource)]
pub struct LanInfo(pub String);

/// edit ขาออก: host → broadcast (exclude = client ที่ส่งมาเอง), client → RequestEdit
/// exclude เป็น None สำหรับ edit ที่เกิดในเครื่องเอง (host player หรือ fluid)
#[derive(Resource, Default)]
pub struct PendingNetEdits(pub VecDeque<(Option<u64>, BlockEdit)>);

/// edit ขาเข้าที่รอ apply + remesh
#[derive(Resource, Default)]
pub struct IncomingNetEdits(pub Vec<BlockEdit>);

/// chunk ที่ได้รับเต็มก้อนจาก host และโหลดอยู่แล้ว → รอ remesh
#[derive(Resource, Default)]
pub struct IncomingChunkRemesh(pub Vec<IVec2>);

/// สถานะฝั่ง host
#[derive(Resource, Default)]
pub struct HostSync {
    /// chunk ที่ต่างจาก generation (มี save file หรือ chiseled data)
    pub dirty: HashSet<IVec2>,
    /// คิวส่ง chunk ให้ client ที่เพิ่ง join (จำกัด 2 chunk/frame/client)
    pub chunk_send_queues: HashMap<u64, Vec<IVec2>>,
    /// เลขผู้เล่นที่แจกไปแล้ว (host เองคือ 1 ไม่อยู่ใน map นี้)
    pub player_numbers: HashMap<u64, u32>,
    /// เลขถัดไปที่จะแจก (เริ่ม 2, ไม่ reuse เลขของคนที่ออกไป)
    pub next_player_number: u32,
}

pub struct ReceivedChunk {
    /// block array ดิบ (decode จาก RLE แล้ว, ยาว CHUNK_VOLUME)
    pub blocks: Vec<u8>,
    pub chiseled: HashMap<usize, Box<[u8; 4096]>>,
}

/// สถานะฝั่ง client
#[derive(Resource, Default)]
pub struct ClientSync {
    pub my_id: u64,
    /// เลขผู้เล่นของเราเอง (จาก Welcome)
    pub my_number: u32,
    pub received_welcome: bool,
    /// ตั้งใจออกเอง (จาก pause menu) — watchdog จะพากลับ MainMenu เงียบๆ
    /// แทนที่จะเด้งไปหน้า multiplayer พร้อมข้อความ "หลุดจากเซิร์ฟเวอร์"
    pub leaving: bool,
    /// noise เดิมของผู้เล่น ไว้คืนค่าตอน disconnect
    pub prev_noise: Option<crate::NoiseParams>,
    /// chunk cache จาก host — อยู่ข้าม unload/reload โดยไม่แตะ disk
    pub full_chunks: HashMap<IVec2, ReceivedChunk>,
    /// edit ที่มาถึงก่อน chunk จะโหลด
    pub pending_edits: HashMap<IVec2, Vec<BlockEdit>>,
    /// chunk ที่ถูก net edit แก้หลังโหลด — ต้องเขียนกลับเข้า full_chunks ตอน unload
    pub edited: HashSet<IVec2>,
}

#[derive(Component)]
pub struct RemotePlayer {
    pub client_id: u64,
    /// 0 = ยังไม่รู้เลข (เจอจาก PlayerPositions ก่อน PlayerJoined จะมาถึง)
    pub player_number: u32,
    pub target_pos: Vec3,
    pub target_yaw: f32,
}

/// ป้ายชื่อ (UI text) ที่ตามหัว avatar ของ entity เป้าหมาย
#[derive(Component)]
pub struct NameTag(pub Entity);

/// timer ส่งตำแหน่ง 20Hz
#[derive(Resource)]
pub struct PositionSendTimer(pub Timer);

impl Default for PositionSendTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(0.05, TimerMode::Repeating))
    }
}

// ---------------------------------------------------------------------------
// Run conditions
// ---------------------------------------------------------------------------

pub fn is_networked(
    server: Option<Res<RenetServer>>,
    client: Option<Res<RenetClient>>,
) -> bool {
    server.is_some() || client.is_some()
}

/// fluid simulation รันได้เฉพาะ single player กับ host
pub fn is_not_client(client: Option<Res<RenetClient>>) -> bool {
    client.is_none()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// หา LAN IP ของเครื่องด้วยการ connect UDP socket ไป address ภายนอก
/// (ไม่มี packet ถูกส่งจริง — connect แค่เลือก route/interface)
pub fn local_lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}

/// client id ที่ไม่ซ้ำกันพอสำหรับ LAN — nanos ตั้งแต่ epoch, บังคับไม่เป็น 0
pub fn generate_client_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        | 1
}

fn chunk_of(pos: IVec3) -> IVec2 {
    IVec2::new(
        pos.x.div_euclid(CHUNK_WIDTH as i32),
        pos.z.div_euclid(CHUNK_WIDTH as i32),
    )
}

fn edit_pos(edit: &BlockEdit) -> IVec3 {
    match edit {
        BlockEdit::SetBlock { pos, .. } | BlockEdit::SetSubVoxel { pos, .. } => IVec3::from_array(*pos),
    }
}

// ---------------------------------------------------------------------------
// Host / client lifecycle
// ---------------------------------------------------------------------------

/// marker: กด Close LAN แล้ว — ถอด resource เฟรมถัดไป ให้ transport มีเวลา
/// flush disconnect packet ไปหา client ก่อน (ถอดทันทีแพ็กเก็ตไม่ถูกส่ง)
#[derive(Resource)]
pub struct StopHostRequested;

pub fn start_host(commands: &mut Commands, world: &VoxelWorld, mp_ui: &mut MultiplayerUi) {
    let socket = match std::net::UdpSocket::bind(("0.0.0.0", SERVER_PORT)) {
        Ok(s) => s,
        Err(e) => {
            mp_ui.status = format!("เปิด host ไม่ได้ (port {SERVER_PORT}): {e}");
            return;
        }
    };
    let ip = local_lan_ip().unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let config = ServerConfig {
        current_time,
        max_clients: 3,
        protocol_id: PROTOCOL_ID,
        public_addresses: vec![std::net::SocketAddr::new(ip, SERVER_PORT)],
        authentication: ServerAuthentication::Unsecure,
    };
    let transport = match NetcodeServerTransport::new(config, socket) {
        Ok(t) => t,
        Err(e) => {
            mp_ui.status = format!("สร้าง server transport ไม่ได้: {e}");
            return;
        }
    };

    // chunk ที่ต่างจาก generation ล้วนๆ = มีไฟล์เซฟ หรือมี sub-voxel ใน memory
    let mut dirty: HashSet<IVec2> = HashSet::new();
    if let Ok(entries) = std::fs::read_dir(crate::voxel::project_root().join("saves")) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(rest) = name.strip_prefix("chunk_").and_then(|r| r.strip_suffix(".bin")) else { continue };
            let mut it = rest.splitn(2, '_');
            if let (Some(x), Some(z)) = (
                it.next().and_then(|v| v.parse::<i32>().ok()),
                it.next().and_then(|v| v.parse::<i32>().ok()),
            ) {
                dirty.insert(IVec2::new(x, z));
            }
        }
    }
    for (pos, chunk) in world.chunks.iter() {
        if !chunk.chiseled_blocks.is_empty() {
            dirty.insert(*pos);
        }
    }

    commands.insert_resource(RenetServer::new(ConnectionConfig::default()));
    commands.insert_resource(transport);
    commands.insert_resource(HostSync {
        dirty,
        chunk_send_queues: HashMap::new(),
        player_numbers: HashMap::new(),
        next_player_number: 2, // host คือ Player 1
    });
    commands.insert_resource(LanInfo(format!("{ip}:{SERVER_PORT}")));
    mp_ui.status.clear();
    info!("เปิด LAN host ที่ {ip}:{SERVER_PORT}");
}

/// ถอด resource ฝั่ง host หนึ่งเฟรมหลังกด Close LAN (ดู StopHostRequested)
pub fn stop_host_system(
    mut commands: Commands,
    requested: Option<Res<StopHostRequested>>,
) {
    if requested.is_none() {
        return;
    }
    commands.remove_resource::<StopHostRequested>();
    commands.remove_resource::<RenetServer>();
    commands.remove_resource::<NetcodeServerTransport>();
    commands.remove_resource::<HostSync>();
    commands.remove_resource::<LanInfo>();
}

pub fn start_client(
    commands: &mut Commands,
    mp_ui: &mut MultiplayerUi,
    current_noise: crate::NoiseParams,
) {
    let text = mp_ui.address.trim();
    let addr_text = if text.is_empty() {
        format!("127.0.0.1:{SERVER_PORT}")
    } else if text.contains(':') {
        text.to_string()
    } else {
        format!("{text}:{SERVER_PORT}")
    };
    let server_addr: std::net::SocketAddr = match addr_text.parse() {
        Ok(a) => a,
        Err(_) => {
            mp_ui.status = "IP ไม่ถูกต้อง (รูปแบบ: 192.168.1.10 หรือ 192.168.1.10:5000)".into();
            return;
        }
    };
    let socket = match std::net::UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => {
            mp_ui.status = format!("เปิด socket ไม่ได้: {e}");
            return;
        }
    };
    let client_id = generate_client_id();
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let auth = ClientAuthentication::Unsecure {
        protocol_id: PROTOCOL_ID,
        client_id,
        server_addr,
        user_data: None,
    };
    let transport = match NetcodeClientTransport::new(current_time, auth, socket) {
        Ok(t) => t,
        Err(e) => {
            mp_ui.status = format!("เชื่อมต่อไม่ได้: {e}");
            return;
        }
    };
    commands.insert_resource(RenetClient::new(ConnectionConfig::default()));
    commands.insert_resource(transport);
    commands.insert_resource(ClientSync {
        my_id: client_id,
        prev_noise: Some(current_noise),
        ..Default::default()
    });
    mp_ui.status = "กำลังเชื่อมต่อ...".into();
}

pub fn teardown_client(commands: &mut Commands) {
    commands.remove_resource::<RenetClient>();
    commands.remove_resource::<NetcodeClientTransport>();
    commands.remove_resource::<ClientSync>();
}

/// avatar ค้างอยู่หลังเลิก host/client — เก็บกวาดทิ้ง
pub fn cleanup_remote_players(
    mut commands: Commands,
    server: Option<Res<RenetServer>>,
    client: Option<Res<RenetClient>>,
    players: Query<Entity, With<RemotePlayer>>,
) {
    if server.is_none() && client.is_none() {
        for entity in &players {
            commands.entity(entity).despawn();
        }
    }
}

// ---------------------------------------------------------------------------
// Remote player avatars
// ---------------------------------------------------------------------------

fn nameplate_label(player_number: u32) -> String {
    if player_number == 0 {
        "Player ?".to_string()
    } else {
        format!("Player {player_number}")
    }
}

fn spawn_remote_player(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    client_id: u64,
    player_number: u32,
) {
    // สีตาม id ให้แยกผู้เล่นออกจากกัน (host id 0 ก็ได้สีของตัวเอง)
    let hue = (client_id.wrapping_mul(2654435761) % 360) as f32;
    let avatar = commands.spawn((
        RemotePlayer {
            client_id,
            player_number,
            // ซ่อนใต้โลกจนกว่าจะได้ตำแหน่งจริงครั้งแรก
            target_pos: Vec3::new(0.0, -1000.0, 0.0),
            target_yaw: 0.0,
        },
        Mesh3d(meshes.add(Cuboid::new(
            crate::camera::PLAYER_HALF * 2.0,
            crate::camera::PLAYER_HEIGHT,
            crate::camera::PLAYER_HALF * 2.0,
        ))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::hsl(hue, 0.75, 0.55),
            ..Default::default()
        })),
        Transform::from_xyz(0.0, -1000.0, 0.0),
    )).id();

    // ป้ายชื่อเป็น UI text แยก entity (UI อยู่คนละ hierarchy กับ 3D)
    // ตำแหน่งบนจออัปเดตทุกเฟรมใน nameplate_system
    commands.spawn((
        Text::new(nameplate_label(player_number)),
        TextFont {
            font_size: bevy::text::FontSize::Px(14.0),
            ..Default::default()
        },
        bevy::text::TextLayout::justify(bevy::text::Justify::Center),
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(-1000.0),
            top: Val::Px(-1000.0),
            width: Val::Px(120.0),
            ..Default::default()
        },
        Visibility::Hidden,
        NameTag(avatar),
    ));
}

/// วางป้ายชื่อเหนือหัว avatar โดย project ตำแหน่งโลกลงจอทุกเฟรม
/// และเก็บกวาดป้ายที่ avatar หายไปแล้ว (despawn ตอน disconnect ฯลฯ)
pub fn nameplate_system(
    mut commands: Commands,
    camera_query: Query<(&Camera, &GlobalTransform), With<crate::camera::FreeCamera>>,
    players: Query<(&Transform, &RemotePlayer)>,
    mut tags: Query<(Entity, &NameTag, &mut Text, &mut Node, &mut Visibility)>,
) {
    let cam = camera_query.iter().next();
    for (entity, tag, mut text, mut node, mut vis) in &mut tags {
        let Ok((avatar_tf, rp)) = players.get(tag.0) else {
            commands.entity(entity).despawn();
            continue;
        };

        // เลขผู้เล่นอาจมาถึงช้ากว่า avatar (PlayerJoined ตามหลัง PlayerPositions)
        let label = nameplate_label(rp.player_number);
        if text.0 != label {
            text.0 = label;
        }

        // ยังไม่เคยได้ตำแหน่งจริง — ซ่อนไว้ก่อน
        if rp.target_pos.y < -900.0 {
            *vis = Visibility::Hidden;
            continue;
        }

        let Some((camera, cam_tf)) = cam else {
            *vis = Visibility::Hidden;
            continue;
        };
        let head = avatar_tf.translation + Vec3::Y * (crate::camera::PLAYER_HEIGHT / 2.0 + 0.4);
        match camera.world_to_viewport(cam_tf, head) {
            Ok(screen) => {
                node.left = Val::Px(screen.x - 60.0); // กึ่งกลาง node กว้าง 120px
                node.top = Val::Px(screen.y - 16.0);
                *vis = Visibility::Visible;
            }
            Err(_) => *vis = Visibility::Hidden, // อยู่หลังกล้อง/นอกจอ
        }
    }
}

pub fn interpolate_remote_players(
    time: Res<Time>,
    mut players: Query<(&mut Transform, &RemotePlayer)>,
) {
    let alpha = (10.0 * time.delta_secs()).min(1.0);
    for (mut transform, rp) in &mut players {
        // ตำแหน่งที่ส่งกันคือระดับตา (กล้อง) แต่ mesh กล่องมีจุดกลางที่ครึ่งความสูง
        let target = rp.target_pos
            - Vec3::Y * (crate::camera::EYE_HEIGHT - crate::camera::PLAYER_HEIGHT / 2.0);
        if transform.translation.y < -900.0 {
            transform.translation = target; // เฟรมแรก teleport ไม่ต้อง lerp ข้ามโลก
        } else {
            transform.translation = transform.translation.lerp(target, alpha);
        }
        let target_rot = Quat::from_rotation_y(rp.target_yaw);
        transform.rotation = transform.rotation.slerp(target_rot, alpha);
    }
}

// ---------------------------------------------------------------------------
// Host systems
// ---------------------------------------------------------------------------

/// observer: bevy_renet trigger RenetServerEvent ตอน client เชื่อม/หลุด
pub fn on_server_event(
    event: On<RenetServerEvent>,
    mut commands: Commands,
    server: Option<ResMut<RenetServer>>,
    host_sync: Option<ResMut<HostSync>>,
    settings: Res<crate::GameSettings>,
    camera_query: Query<&Transform, With<crate::camera::FreeCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    remote_players: Query<(Entity, &RemotePlayer)>,
) {
    let (Some(mut server), Some(mut host_sync)) = (server, host_sync) else { return };
    match &event.0 {
        ServerEvent::ClientConnected { client_id } => {
            let client_id = *client_id;
            // แจกเลขผู้เล่นตามลำดับ join (host = 1)
            let player_number = host_sync.next_player_number.max(2);
            host_sync.next_player_number = player_number + 1;
            host_sync.player_numbers.insert(client_id, player_number);
            info!("client {client_id} เข้าร่วมเกมเป็น Player {player_number}");

            let spawn_pos = camera_query
                .iter()
                .next()
                .map(|t| t.translation)
                .unwrap_or(Vec3::new(0.0, 250.0, 0.0));
            let welcome = ServerMessage::Welcome {
                client_id,
                player_number,
                noise: settings.noise,
                spawn_pos: spawn_pos.to_array(),
                time_of_day: settings.time_of_day,
            };
            server.send_message(client_id, DefaultChannel::ReliableOrdered, encode(&welcome));

            // แนะนำผู้เล่นที่อยู่ก่อนให้คนใหม่รู้จัก: host เอง + client คนอื่นๆ
            server.send_message(
                client_id,
                DefaultChannel::ReliableOrdered,
                encode(&ServerMessage::PlayerJoined {
                    client_id: HOST_PLAYER_ID,
                    player_number: 1,
                }),
            );
            for (&other_id, &other_number) in host_sync.player_numbers.iter() {
                if other_id != client_id {
                    server.send_message(
                        client_id,
                        DefaultChannel::ReliableOrdered,
                        encode(&ServerMessage::PlayerJoined {
                            client_id: other_id,
                            player_number: other_number,
                        }),
                    );
                }
            }

            let queue: Vec<IVec2> = host_sync.dirty.iter().copied().collect();
            host_sync.chunk_send_queues.insert(client_id, queue);

            server.broadcast_message_except(
                client_id,
                DefaultChannel::ReliableOrdered,
                encode(&ServerMessage::PlayerJoined { client_id, player_number }),
            );
            spawn_remote_player(&mut commands, &mut meshes, &mut materials, client_id, player_number);
        }
        ServerEvent::ClientDisconnected { client_id, reason } => {
            let client_id = *client_id;
            info!("client {client_id} หลุด: {reason}");
            host_sync.chunk_send_queues.remove(&client_id);
            host_sync.player_numbers.remove(&client_id);
            for (entity, rp) in &remote_players {
                if rp.client_id == client_id {
                    commands.entity(entity).despawn();
                }
            }
            server.broadcast_message(
                DefaultChannel::ReliableOrdered,
                encode(&ServerMessage::PlayerLeft { client_id }),
            );
        }
    }
}

/// เกณฑ์รับ edit จาก client: ตำแหน่ง/ค่าต้องอยู่ในช่วง และ chunk ต้องโหลดอยู่บน host
/// (chunk ที่ host ยัง unload อยู่ = ปฏิเสธ — v1 ยอมรับข้อจำกัดนี้)
fn validate_edit(edit: &BlockEdit, world: &VoxelWorld) -> bool {
    let (pos, val_ok) = match edit {
        BlockEdit::SetBlock { pos, block } => (*pos, BlockType::from_u8(*block) as u8 == *block),
        BlockEdit::SetSubVoxel { pos, sub, val } => (
            *pos,
            sub.iter().all(|s| *s < 16) && BlockType::from_u8(*val) as u8 == *val,
        ),
    };
    let p = IVec3::from_array(pos);
    val_ok
        && p.y >= 0
        && p.y < crate::voxel::CHUNK_HEIGHT as i32
        && world.chunks.contains_key(&chunk_of(p))
}

pub fn host_receive_client_messages(
    mut server: ResMut<RenetServer>,
    mut incoming: ResMut<IncomingNetEdits>,
    mut pending: ResMut<PendingNetEdits>,
    world: Res<VoxelWorld>,
    mut remote_players: Query<&mut RemotePlayer>,
) {
    for client_id in server.clients_id() {
        while let Some(bytes) = server.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let Some(msg) = decode::<ClientMessage>(&bytes) else { continue };
            let ClientMessage::RequestEdit { edits } = msg else { continue };
            if edits.len() > 16 {
                warn!("client {client_id} ส่ง edit batch ใหญ่ผิดปกติ ({}) — ทิ้ง", edits.len());
                continue;
            }
            for edit in edits {
                if !validate_edit(&edit, &world) {
                    continue;
                }
                incoming.0.push(edit.clone());
                pending.0.push_back((Some(client_id), edit));
            }
        }
        while let Some(bytes) = server.receive_message(client_id, DefaultChannel::Unreliable) {
            let Some(ClientMessage::Position { pos, yaw }) = decode::<ClientMessage>(&bytes) else {
                continue;
            };
            for mut rp in remote_players.iter_mut() {
                if rp.client_id == client_id {
                    rp.target_pos = Vec3::from_array(pos);
                    rp.target_yaw = yaw;
                }
            }
        }
    }
}

/// ส่ง chunk ทั้งก้อนให้ client ทุกคน — ใช้ตอน nuke ที่แก้บล็อกเป็นแสน
/// (ยิงราย edit จะล้นท่อ reliable; RLE ทั้ง chunk ถูกกว่ามาก)
pub fn queue_chunk_to_all_clients(server: &RenetServer, host_sync: &mut HostSync, chunk: IVec2) {
    // ทำเครื่องหมาย dirty ให้คน join ทีหลังได้ chunk นี้ด้วย
    host_sync.dirty.insert(chunk);
    for client_id in server.clients_id() {
        host_sync.chunk_send_queues.entry(client_id).or_default().push(chunk);
    }
}

pub fn host_send_queued_chunks(
    mut server: ResMut<RenetServer>,
    mut host_sync: ResMut<HostSync>,
    world: Res<VoxelWorld>,
) {
    let mut finished: Vec<u64> = Vec::new();
    for (client_id, queue) in host_sync.chunk_send_queues.iter_mut() {
        // จำกัด 2 chunk/frame/client กัน reliable channel ล้น
        for _ in 0..2 {
            let Some(chunk_pos) = queue.pop() else {
                finished.push(*client_id);
                break;
            };
            let msg = if let Some(chunk) = world.chunks.get(&chunk_pos) {
                let bytes: Vec<u8> = chunk.blocks.iter_all().map(|b| b as u8).collect();
                ServerMessage::ChunkData {
                    chunk_pos: chunk_pos.to_array(),
                    blocks_rle: rle_encode(&bytes),
                    chiseled: chunk
                        .chiseled_blocks
                        .iter()
                        .map(|(k, v)| (*k as u32, v.to_vec()))
                        .collect(),
                }
            } else if let Some(bytes) = crate::voxel::load_chunk_bytes(chunk_pos) {
                // chunk ไม่ได้โหลดบน host แต่มีไฟล์เซฟ — ส่งจาก disk
                // (sub-voxel ไม่เคยถูกเซฟลง disk อยู่แล้ว — ข้อจำกัดเดิมของ save format)
                ServerMessage::ChunkData {
                    chunk_pos: chunk_pos.to_array(),
                    blocks_rle: rle_encode(&bytes),
                    chiseled: Vec::new(),
                }
            } else {
                continue; // dirty entry ที่ไม่มีข้อมูลแล้ว (เช่นไฟล์ถูกลบ)
            };
            server.send_message(*client_id, DefaultChannel::ReliableOrdered, encode(&msg));
        }
    }
    for id in finished {
        host_sync.chunk_send_queues.remove(&id);
    }
}

pub fn host_broadcast_edits(
    mut server: ResMut<RenetServer>,
    mut pending: ResMut<PendingNetEdits>,
    mut host_sync: ResMut<HostSync>,
) {
    if pending.0.is_empty() {
        return;
    }
    // แยกกลุ่มตามว่า edit นั้นต้องเว้นใคร (client ที่ส่งมาเอง apply ไปแล้ว)
    let mut groups: HashMap<Option<u64>, Vec<BlockEdit>> = HashMap::new();
    let cap = pending.0.len().min(2000);
    for (exclude, edit) in pending.0.drain(..cap) {
        host_sync.dirty.insert(chunk_of(edit_pos(&edit)));
        groups.entry(exclude).or_default().push(edit);
    }
    for (exclude, edits) in groups {
        let msg = encode(&ServerMessage::BlockEditBatch { edits });
        match exclude {
            Some(id) => server.broadcast_message_except(id, DefaultChannel::ReliableOrdered, msg),
            None => server.broadcast_message(DefaultChannel::ReliableOrdered, msg),
        }
    }
}

pub fn host_broadcast_positions(
    time: Res<Time>,
    mut timer: ResMut<PositionSendTimer>,
    mut server: ResMut<RenetServer>,
    camera_query: Query<(&Transform, &crate::camera::FreeCamera)>,
    remote_players: Query<&RemotePlayer>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    let mut players = Vec::new();
    if let Some((transform, cam)) = camera_query.iter().next() {
        players.push((HOST_PLAYER_ID, transform.translation.to_array(), cam.yaw));
    }
    for rp in &remote_players {
        players.push((rp.client_id, rp.target_pos.to_array(), rp.target_yaw));
    }
    server.broadcast_message(
        DefaultChannel::Unreliable,
        encode(&ServerMessage::PlayerPositions { players }),
    );
}

// ---------------------------------------------------------------------------
// Client systems
// ---------------------------------------------------------------------------

pub fn client_receive_messages(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    mut client_sync: ResMut<ClientSync>,
    mut settings: ResMut<crate::GameSettings>,
    mut regenerate: ResMut<crate::RegenerateWorld>,
    mut next_state: ResMut<NextState<crate::GameState>>,
    mut mp_ui: ResMut<MultiplayerUi>,
    mut incoming: ResMut<IncomingNetEdits>,
    mut chunk_remesh: ResMut<IncomingChunkRemesh>,
    mut world: ResMut<VoxelWorld>,
    mut camera_query: Query<&mut Transform, With<crate::camera::FreeCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut remote_players: Query<(Entity, &mut RemotePlayer)>,
) {
    while let Some(bytes) = client.receive_message(DefaultChannel::ReliableOrdered) {
        match decode::<ServerMessage>(&bytes) {
            Some(ServerMessage::Welcome { client_id: _, player_number, noise, spawn_pos, time_of_day }) => {
                client_sync.my_number = player_number;
                settings.noise = noise;
                settings.time_of_day = time_of_day;
                if let Some(mut transform) = camera_query.iter_mut().next() {
                    transform.translation = Vec3::from_array(spawn_pos);
                }
                // ล้างโลกเดิม (ถ้ามีจาก session ก่อน) แล้ว generate ใหม่ด้วย noise ของ host
                regenerate.0 = true;
                client_sync.received_welcome = true;
                info!("ได้รับ Welcome จาก host — เข้าเกม (noise/เวลา sync แล้ว)");
                mp_ui.status.clear();
                next_state.set(crate::GameState::InGame);
                // avatar ของ host และผู้เล่นคนอื่นจะมาถึงเป็น PlayerJoined ต่อจากนี้
            }
            Some(ServerMessage::ChunkData { chunk_pos, blocks_rle, chiseled }) => {
                let pos = IVec2::from_array(chunk_pos);
                let Some(blocks) = rle_decode(&blocks_rle) else {
                    warn!("ChunkData {pos:?} decode ไม่ผ่าน — ทิ้ง");
                    continue;
                };
                let chiseled_map: HashMap<usize, Box<[u8; 4096]>> = chiseled
                    .into_iter()
                    .filter_map(|(k, v)| {
                        let arr: Box<[u8; 4096]> = v.into_boxed_slice().try_into().ok()?;
                        Some((k as usize, arr))
                    })
                    .collect();

                // chunk โหลดอยู่แล้ว → เขียนทับทันที + remesh
                if let Some(chunk) = world.chunks.get_mut(&pos) {
                    chunk.blocks =
                        std::sync::Arc::new(crate::voxel::ChunkBlocks::from_dense_bytes(&blocks));
                    chunk.chiseled_blocks = chiseled_map.clone();
                    chunk_remesh.0.push(pos);
                }
                client_sync.full_chunks.insert(pos, ReceivedChunk { blocks, chiseled: chiseled_map });
            }
            Some(ServerMessage::BlockEditBatch { edits }) => incoming.0.extend(edits),
            Some(ServerMessage::PlayerJoined { client_id, player_number }) => {
                if client_id != client_sync.my_id {
                    // ถ้า avatar โผล่มาก่อนแล้ว (lazy spawn จาก PlayerPositions)
                    // แค่เติมเลขผู้เล่นให้ป้ายชื่อถูก
                    let mut found = false;
                    for (_, mut rp) in remote_players.iter_mut() {
                        if rp.client_id == client_id {
                            rp.player_number = player_number;
                            found = true;
                        }
                    }
                    if !found {
                        spawn_remote_player(&mut commands, &mut meshes, &mut materials, client_id, player_number);
                    }
                }
            }
            Some(ServerMessage::PlayerLeft { client_id }) => {
                for (entity, rp) in remote_players.iter() {
                    if rp.client_id == client_id {
                        commands.entity(entity).despawn();
                    }
                }
            }
            Some(ServerMessage::PlayerPositions { .. }) | None => {}
        }
    }

    while let Some(bytes) = client.receive_message(DefaultChannel::Unreliable) {
        let Some(ServerMessage::PlayerPositions { players }) = decode::<ServerMessage>(&bytes) else {
            continue;
        };
        for (id, pos, yaw) in players {
            if id == client_sync.my_id {
                continue;
            }
            let mut found = false;
            for (_, mut rp) in remote_players.iter_mut() {
                if rp.client_id == id {
                    rp.target_pos = Vec3::from_array(pos);
                    rp.target_yaw = yaw;
                    found = true;
                }
            }
            if !found {
                // ตำแหน่ง (unreliable) มาถึงก่อน PlayerJoined (reliable) ได้ —
                // spawn ไปก่อนด้วยเลข 0 ("Player ?") เดี๋ยว PlayerJoined ตามมาเติม
                spawn_remote_player(&mut commands, &mut meshes, &mut materials, id, 0);
            }
        }
    }
}

pub fn client_send_position(
    time: Res<Time>,
    mut timer: ResMut<PositionSendTimer>,
    mut client: ResMut<RenetClient>,
    camera_query: Query<(&Transform, &crate::camera::FreeCamera)>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    if let Some((transform, cam)) = camera_query.iter().next() {
        client.send_message(
            DefaultChannel::Unreliable,
            encode(&ClientMessage::Position {
                pos: transform.translation.to_array(),
                yaw: cam.yaw,
            }),
        );
    }
}

pub fn client_send_edits(mut client: ResMut<RenetClient>, mut pending: ResMut<PendingNetEdits>) {
    if pending.0.is_empty() {
        return;
    }
    let edits: Vec<BlockEdit> = pending.0.drain(..).map(|(_, e)| e).collect();
    for batch in edits.chunks(16) {
        client.send_message(
            DefaultChannel::ReliableOrdered,
            encode(&ClientMessage::RequestEdit { edits: batch.to_vec() }),
        );
    }
}

pub fn client_connection_watchdog(
    mut commands: Commands,
    client: Res<RenetClient>,
    transport: Option<Res<NetcodeClientTransport>>,
    client_sync: Res<ClientSync>,
    mut settings: ResMut<crate::GameSettings>,
    mut regenerate: ResMut<crate::RegenerateWorld>,
    mut next_state: ResMut<NextState<crate::GameState>>,
    mut mp_ui: ResMut<MultiplayerUi>,
) {
    if !client.is_disconnected() {
        return;
    }
    let reason = transport
        .and_then(|t| t.disconnect_reason())
        .map(|r| format!("{r:?}"))
        .unwrap_or_else(|| "connection lost".into());

    if client_sync.received_welcome {
        // ออกจากเกม → คืนค่า noise เดิม ล้างโลกของ host ทิ้ง
        if let Some(prev) = client_sync.prev_noise {
            settings.noise = prev;
        }
        regenerate.0 = true;
        if client_sync.leaving {
            // ออกเองผ่าน pause menu — กลับเมนูหลักเงียบๆ
            mp_ui.status.clear();
            next_state.set(crate::GameState::MainMenu);
        } else {
            mp_ui.status = format!("หลุดจากเซิร์ฟเวอร์: {reason}");
            next_state.set(crate::GameState::MultiplayerMenu);
        }
    } else {
        mp_ui.status = format!("เชื่อมต่อไม่สำเร็จ: {reason}");
        next_state.set(crate::GameState::MultiplayerMenu);
    }
    teardown_client(&mut commands);
}

// ---------------------------------------------------------------------------
// Auto start จาก command line (ไว้ทดสอบ: --host หรือ --join <ip>)
// ---------------------------------------------------------------------------

/// รอ VoxelWorld พร้อมก่อนค่อยเปิด host (--host)
#[derive(Resource)]
pub struct AutoHostPending;

pub fn autostart_from_args(
    mut commands: Commands,
    mut mp_ui: ResMut<MultiplayerUi>,
    settings: Res<crate::GameSettings>,
    mut next_state: ResMut<NextState<crate::GameState>>,
) {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--host") {
        commands.insert_resource(AutoHostPending);
        next_state.set(crate::GameState::InGame);
    } else if let Some(i) = args.iter().position(|a| a == "--join") {
        mp_ui.address = args.get(i + 1).cloned().unwrap_or_else(|| "127.0.0.1".into());
        next_state.set(crate::GameState::MultiplayerMenu);
        start_client(&mut commands, &mut mp_ui, settings.noise);
    }
}

pub fn auto_host_system(
    mut commands: Commands,
    pending: Option<Res<AutoHostPending>>,
    world: Option<Res<VoxelWorld>>,
    mut mp_ui: ResMut<MultiplayerUi>,
) {
    if pending.is_none() {
        return;
    }
    let Some(world) = world else { return };
    commands.remove_resource::<AutoHostPending>();
    start_host(&mut commands, &world, &mut mp_ui);
}

// ---------------------------------------------------------------------------
// Shared: apply edits ที่มาจาก network
// ---------------------------------------------------------------------------

pub fn apply_incoming_net_edits(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    mut incoming: ResMut<IncomingNetEdits>,
    mut chunk_remesh: ResMut<IncomingChunkRemesh>,
    mut client_sync: Option<ResMut<ClientSync>>,
    server: Option<Res<RenetServer>>,
    mut active_fluids: ResMut<crate::voxel::ActiveFluids>,
    mut pools: ResMut<crate::voxel::ActivePools>,
    mut mp: crate::voxel::MeshingParams,
    (mut active_tnt, settings): (ResMut<crate::voxel::ActiveTnt>, Res<crate::GameSettings>),
) {
    if incoming.0.is_empty() && chunk_remesh.0.is_empty() {
        return;
    }
    let is_host = server.is_some();
    let mut to_remesh: HashSet<IVec2> = HashSet::new();
    // edit ที่ทั้งก่อน/หลังเป็นน้ำหรืออากาศ (delta จาก fluid sim ของ host เกือบทั้งหมด)
    // เข้าเส้นทาง remesh เฉพาะชั้นน้ำที่ถูกกว่ามาก
    let mut water_remesh: HashSet<IVec2> = HashSet::new();
    let mut edited_chunks: HashSet<IVec2> = HashSet::new();
    let water_like = |b: crate::voxel::BlockType| b == crate::voxel::BlockType::Air || b.is_water();

    for edit in incoming.0.drain(..) {
        let cp = chunk_of(edit_pos(&edit));
        if !world.chunks.contains_key(&cp) {
            // chunk ยังไม่โหลด — client เก็บไว้ apply ตอน chunk มาถึง, host ทิ้ง
            // (host validate แล้วว่า chunk โหลดอยู่ — มาถึงตรงนี้ไม่ได้นอกจาก unload พอดี)
            if let Some(cs) = client_sync.as_mut() {
                cs.pending_edits.entry(cp).or_default().push(edit);
            }
            continue;
        }
        // ต้องดู block เก่าก่อน apply ถึงจะรู้ว่าเป็น edit น้ำล้วนไหม
        let is_water_edit = match &edit {
            BlockEdit::SetBlock { pos, block } => {
                water_like(world.get_block(pos[0], pos[1], pos[2]))
                    && water_like(crate::voxel::BlockType::from_u8(*block))
            }
            BlockEdit::SetSubVoxel { .. } => false,
        };
        let Some(tp) = crate::voxel::apply_block_edit(&mut world, &edit) else { continue };
        // edit จาก client แตะเขตสระ (host เท่านั้นที่มีสระ — client list ว่างเสมอ)
        pools.invalidate_touching(tp);
        if is_water_edit {
            water_remesh.extend(crate::voxel::edit_affected_chunks(tp));
        } else {
            to_remesh.extend(crate::voxel::edit_affected_chunks(tp));
            // lamp light อัปเดตเฉพาะ edit ที่ไม่ใช่น้ำ (น้ำไม่มีทางแตะบล็อกไฟ)
            edited_chunks.insert(cp);
        }
        if let Some(cs) = client_sync.as_mut() {
            cs.edited.insert(cp);
        }
        if is_host {
            // client จุดชนวน TNT/Nuke (ส่ง SetBlock *Lit มา) — host เป็นคนนับ fuse
            if let BlockEdit::SetBlock { block, .. } = &edit {
                match crate::voxel::BlockType::from_u8(*block) {
                    crate::voxel::BlockType::TntLit => {
                        active_tnt.0.insert(
                            tp,
                            Timer::from_seconds(settings.tnt_fuse_seconds, TimerMode::Once),
                        );
                    }
                    crate::voxel::BlockType::NukeLit => {
                        active_tnt.0.insert(
                            tp,
                            Timer::from_seconds(settings.nuke_fuse_seconds, TimerMode::Once),
                        );
                    }
                    _ => {}
                }
            }
            // host เป็นเจ้าของ simulation: ปลุกน้ำ + เซฟเหมือน edit ในเครื่อง
            active_fluids.0.insert(tp);
            for dir in [
                IVec3::new(1, 0, 0), IVec3::new(-1, 0, 0), IVec3::new(0, 1, 0),
                IVec3::new(0, -1, 0), IVec3::new(0, 0, 1), IVec3::new(0, 0, -1),
            ] {
                active_fluids.0.insert(tp + dir);
            }
            if let Some(chunk) = world.chunks.get(&cp) {
                crate::voxel::save_chunk(cp, &chunk.blocks);
            }
        }
    }

    // chunk เต็มก้อนที่เพิ่งรับจาก host และโหลดอยู่แล้ว — full เสมอ
    for pos in chunk_remesh.0.drain(..) {
        to_remesh.insert(pos);
        edited_chunks.insert(pos);
    }

    // full remesh ครอบชั้นน้ำอยู่แล้ว — อย่าทำ chunk เดิมซ้ำสองรอบ
    water_remesh.retain(|c| !to_remesh.contains(c));

    crate::voxel::remesh_chunks(&mut commands, &mut world, &mut mp, to_remesh);
    let _ = crate::voxel::remesh_water_only(&mut commands, &mut world, &mut mp, water_remesh);
    for cp in edited_chunks {
        crate::voxel::refresh_chunk_lamp_lights(&mut commands, &mut world, cp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rle_roundtrip_uniform() {
        let data = vec![7u8; CHUNK_VOLUME];
        let encoded = rle_encode(&data);
        // run length เป็น u16 — uniform ทั้งคอลัมน์ใช้ ~CHUNK_VOLUME/65535 runs
        let max_runs = CHUNK_VOLUME / (u16::MAX as usize) + 2;
        assert!(encoded.len() <= max_runs * 4, "encoded {} bytes", encoded.len());
        assert_eq!(rle_decode(&encoded).unwrap(), data);
    }

    #[test]
    fn rle_roundtrip_mixed() {
        let mut data = vec![0u8; CHUNK_VOLUME];
        for (i, b) in data.iter_mut().enumerate() {
            *b = ((i / 3) % 23) as u8;
        }
        let encoded = rle_encode(&data);
        assert_eq!(rle_decode(&encoded).unwrap(), data);
    }

    #[test]
    fn rle_decode_rejects_bad_input() {
        assert!(rle_decode(&[1, 2]).is_none()); // ไม่หาร 3 ลงตัว
        assert!(rle_decode(&[5, 0, 0]).is_none()); // run = 0
        assert!(rle_decode(&rle_encode(&[1u8; 10])).is_none()); // ยาวไม่ถึง CHUNK_VOLUME
    }
}
