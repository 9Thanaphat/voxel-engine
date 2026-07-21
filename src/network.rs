use bevy::prelude::*;
use bevy_renet::netcode::{
    ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport, ServerAuthentication,
    ServerConfig,
};
use bevy_renet::renet::{ConnectionConfig, DefaultChannel, ServerEvent};
use bevy_renet::{RenetClient, RenetServer, RenetServerEvent};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::voxel::{VoxelWorld, CHUNK_VOLUME, CHUNK_WIDTH};


pub const SERVER_PORT: u16 = 5000;
/// ต้องตรงกันทั้ง host และ client ไม่งั้น netcode ปฏิเสธการเชื่อมต่อ
/// (0003: Position/PlayerPositions เพิ่มของที่ถือ — โครง encode เปลี่ยน)
pub const PROTOCOL_ID: u64 = 0xB10C_CAFE_0003;
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
    PlaceFacingBlock { pos: [i32; 3], block: u8, facing: u8 },
    SetContainerSlot { pos: [i32; 3], slot: u8, item: Option<crate::item::WireItemStack> },
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum ServerMessage {
    Welcome {
        client_id: u64,
        /// ลำดับผู้เล่น (host = 1, client ตามลำดับ join = 2, 3, ...)
        player_number: u32,
        noise: crate::NoiseParams,
        /// client generate chunk เองจากค่านี้ — RealWorld ต้องมีไฟล์ dem ตรงกัน
        terrain: crate::TerrainSource,
        spawn_pos: [f32; 3],
        time_of_day: f32,
        game_mode: crate::GameMode,
    },
    ChunkData {
        chunk_pos: [i32; 2],
        blocks_rle: Vec<u8>,
        /// (block index ใน chunk, palette 4096 bytes)
        chiseled: Vec<(u32, Vec<u8>)>,
        facings: Vec<(u32, u8)>,
        containers: Vec<(u32, u8, Vec<Option<crate::item::WireItemStack>>)>,
    },
    BlockEditBatch { edits: Vec<BlockEdit> },
    PlayerJoined { client_id: u64, player_number: u32 },
    PlayerLeft { client_id: u64 },
    /// (id, ตำแหน่งตา, yaw, ของที่ถือเป็น wire (kind,id) — None = มือเปล่า)
    PlayerPositions { players: Vec<(u64, [f32; 3], f32, Option<(u8, u8)>)> },
    Chat { from: u32, text: String },
    TimeOfDay { hours: f32 },
    Explosion(ExplosionWire),
    PlayerAction { client_id: u64, action: u8 },
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct WireRaySeg {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub energy: f32,
    pub dist0: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct ExplosionWire {
    pub center: [f32; 3],
    pub rays: Vec<WireRaySeg>,
    pub power: f32,
    pub is_nuke: bool,
}

impl ExplosionWire {
    pub fn new(center: bevy::math::Vec3, rays: &[crate::voxel::RaySeg], power: f32, is_nuke: bool) -> Self {
        Self {
            center: center.to_array(),
            rays: rays.iter().map(|r| WireRaySeg {
                a: r.a.to_array(),
                b: r.b.to_array(),
                energy: r.energy,
                dist0: r.dist0,
            }).collect(),
            power,
            is_nuke,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum ClientMessage {
    RequestEdit { edits: Vec<BlockEdit> },
    /// held = ของที่ถือเป็น wire (kind,id) — แนบมากับตำแหน่ง 20Hz ไม่มี message แยก
    Position { pos: [f32; 3], yaw: f32, held: Option<(u8, u8)> },
    Chat { text: String },
    Action { action: u8 },
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

#[derive(Resource, Default)]
pub struct PendingLocalActions(pub Vec<u8>);

#[derive(Resource, Default)]
pub struct PendingNetFx(pub Vec<ExplosionWire>);

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
    pub facings: HashMap<usize, u8>,
    pub chest_slots: HashMap<usize, Box<[Option<crate::voxel::ItemStack>; 27]>>,
    pub furnace_slots: HashMap<usize, Box<[Option<crate::voxel::ItemStack>; 3]>>,
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
    /// noise/terrain เดิมของผู้เล่น ไว้คืนค่าตอน disconnect
    pub prev_noise: Option<crate::NoiseParams>,
    pub prev_terrain: Option<crate::TerrainSource>,
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
    pub walk_phase: f32,
    pub mining_timer: f32,
    /// ของที่ผู้เล่นคนนี้ถืออยู่ (sync มากับ Position/PlayerPositions)
    pub held: Option<crate::item::Item>,
}

#[derive(Resource)]
pub struct PlayerModelAssets {
    pub player: Handle<WorldAsset>,
}

pub fn setup_player_model_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    use bevy::gltf::GltfAssetLabel;
    commands.insert_resource(PlayerModelAssets {
        player: asset_server.load(GltfAssetLabel::Scene(0).from_asset("model/player_model.gltf")),
    });
}

#[derive(Component, Default)]
pub struct PlayerRig {
    pub head: Option<Entity>,
    pub upper_arm_left: Option<Entity>,
    pub upper_arm_right: Option<Entity>,
    pub upper_leg_left: Option<Entity>,
    pub upper_leg_right: Option<Entity>,
    /// ภาพของที่ถืออยู่ในมือขวา (spawn โดย update_remote_held_items)
    pub held_entity: Option<Entity>,
    /// item ที่ held_entity แสดงอยู่ — ต่างจาก RemotePlayer.held เมื่อไหร่ = สลับของ
    pub held_item: Option<crate::item::Item>,
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

pub fn edit_pos(edit: &BlockEdit) -> IVec3 {
    match edit {
        BlockEdit::SetBlock { pos, .. } | BlockEdit::SetSubVoxel { pos, .. } | BlockEdit::PlaceFacingBlock { pos, .. } | BlockEdit::SetContainerSlot { pos, .. } => IVec3::from_array(*pos),
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
    // ต้องอ่านโฟลเดอร์เซฟของโลกที่เล่นอยู่จริง (saves_dem/ หรือ saves/<ชื่อโลก>/)
    // — เดิมอ่าน saves/ ตรงๆ host โลกอื่นเลยมองไม่เห็น chunk ที่เคยแก้บนดิสก์
    let mut dirty: HashSet<IVec2> = HashSet::new();
    if let Ok(entries) = std::fs::read_dir(crate::voxel::active_save_dir()) {
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
    current_terrain: crate::TerrainSource,
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
        prev_terrain: Some(current_terrain),
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
    mut local_actions: ResMut<PendingLocalActions>,
) {
    if server.is_none() && client.is_none() {
        for entity in &players {
            commands.entity(entity).despawn();
        }
        // single player ไม่มีใคร drain คิว action (ระบบส่งรันเฉพาะตอน networked)
        // — เคลียร์ทิ้งกันโตไม่จำกัด
        local_actions.0.clear();
    }
}

/// ของที่ avatar ถือในมือขวา — spawn/สลับตาม RemotePlayer.held ที่ sync มา
pub fn update_remote_held_items(
    mut commands: Commands,
    mut players: Query<(&RemotePlayer, &mut PlayerRig)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    block_mats: Res<crate::voxel::BlockMaterials>,
    campfire_assets: Res<crate::voxel::CampfireAssets>,
) {
    for (rp, mut rig) in players.iter_mut() {
        // รอ rig พร้อมก่อน (โมเดลเพิ่งโหลด) — เฟรมถัดไปค่อยเกาะ
        let Some(arm) = rig.upper_arm_right else { continue };
        if rig.held_item == rp.held {
            continue;
        }
        if let Some(entity) = rig.held_entity.take() {
            commands.entity(entity).despawn();
        }
        rig.held_item = rp.held;
        if let Some(item) = rp.held {
            let size = match item {
                crate::item::Item::Block(_) => 0.25,
                _ => 0.6,
            };
            // ตำแหน่ง/มุมเดียวกับ pickaxe ที่เคย hardcode ติดแขน (จูนแล้วว่าดูดี)
            let tf = Transform::from_xyz(0.0, -0.6, 0.4)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
            let entity = crate::item::spawn_item_visual(
                &mut commands, &mut meshes, &mut materials, &asset_server,
                &block_mats, &campfire_assets, item, size, tf,
            );
            commands.entity(arm).add_child(entity);
            rig.held_entity = Some(entity);
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
    models: &Res<PlayerModelAssets>,
    client_id: u64,
    player_number: u32,
) {
    let _hue = (client_id.wrapping_mul(2654435761) % 360) as f32;
    let avatar = commands.spawn((
        RemotePlayer {
            client_id,
            player_number,
            target_pos: Vec3::new(0.0, -1000.0, 0.0),
            target_yaw: 0.0,
            walk_phase: 0.0,
            mining_timer: 0.0,
            held: None,
        },
        PlayerRig::default(),
        WorldAssetRoot(models.player.clone()),
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
    camera_query: Query<(&Camera, &GlobalTransform), With<crate::camera::MainCamera>>,
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
    models: Res<PlayerModelAssets>,
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

            // spawn ตรงตำแหน่ง host จริง — เดิม hardcode (0,150,0) ซึ่งในโลก DEM
            // คือมุมแผนที่กลางทะเล ห่างจากตัว host หลายร้อยกิโลเมตร
            let spawn_pos = camera_query
                .iter()
                .next()
                .map(|t| t.translation)
                .unwrap_or(Vec3::new(0.0, 250.0, 0.0));
            let welcome = ServerMessage::Welcome {
                client_id,
                player_number,
                noise: settings.noise,
                terrain: settings.terrain_source,
                spawn_pos: spawn_pos.to_array(),
                time_of_day: settings.time_of_day,
                game_mode: settings.game_mode,
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
            spawn_remote_player(&mut commands, &models, client_id, player_number);
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
        BlockEdit::SetBlock { pos, block } => {
            (IVec3::from_array(*pos), *block <= 26)
        }
        BlockEdit::SetSubVoxel { pos, val, .. } => {
            (IVec3::from_array(*pos), *val <= 26)
        }
        BlockEdit::PlaceFacingBlock { pos, block, facing } => {
            (IVec3::from_array(*pos), *block <= 26 && *facing < 6)
        }
        BlockEdit::SetContainerSlot { pos, .. } => {
            (IVec3::from_array(*pos), true)
        }
    };
    let p = IVec3::from_array(pos.to_array());
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
    host_sync: Res<HostSync>,
    mut chat_ui: ResMut<crate::ui::ChatState>,
) {
    for client_id in server.clients_id() {
        while let Some(bytes) = server.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let Some(msg) = decode::<ClientMessage>(&bytes) else { continue };
            match msg {
                ClientMessage::RequestEdit { edits } => {
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
                // แชทจาก client: host เป็นคนกระจายให้ทุกคนรวมคนส่งเอง (client ไม่ขึ้น
                // จอตัวเองจนกว่าจะได้ echo กลับ — ลำดับข้อความจะได้ตรงกันทุกจอ ดู command.rs)
                ClientMessage::Chat { text } => {
                    let text = sanitize_chat(&text);
                    if text.is_empty() {
                        continue;
                    }
                    let from = host_sync.player_numbers.get(&client_id).copied().unwrap_or(0);
                    server.broadcast_message(
                        DefaultChannel::ReliableOrdered,
                        encode(&ServerMessage::Chat { from, text: text.clone() }),
                    );
                    chat_ui.push_player(from, text);
                }
                // ท่าทาง (ขุด) จาก client: เล่นบน avatar ฝั่ง host เอง + ส่งต่อให้ client อื่น
                ClientMessage::Action { action } => {
                    server.broadcast_message_except(
                        client_id,
                        DefaultChannel::ReliableOrdered,
                        encode(&ServerMessage::PlayerAction { client_id, action }),
                    );
                    if action == 0 {
                        for mut rp in remote_players.iter_mut() {
                            if rp.client_id == client_id {
                                rp.mining_timer = 0.5;
                            }
                        }
                    }
                }
                // Position มาทาง Unreliable เท่านั้น — โผล่ช่องนี้ = ผิดปกติ ทิ้ง
                ClientMessage::Position { .. } => {}
            }
        }
        while let Some(bytes) = server.receive_message(client_id, DefaultChannel::Unreliable) {
            let Some(ClientMessage::Position { pos, yaw, held }) = decode::<ClientMessage>(&bytes) else {
                continue;
            };
            for mut rp in remote_players.iter_mut() {
                if rp.client_id == client_id {
                    rp.target_pos = Vec3::from_array(pos);
                    rp.target_yaw = yaw;
                    rp.held = held.and_then(|(k, i)| crate::item::item_from_wire(k, i));
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
                    facings: Vec::new(),
                    containers: Vec::new(),
                }
            } else if let Some(bytes) = crate::voxel::load_chunk_bytes(chunk_pos) {
                // chunk ไม่ได้โหลดบน host แต่มีไฟล์เซฟ — ส่งจาก disk
                // (sub-voxel ไม่เคยถูกเซฟลง disk อยู่แล้ว — ข้อจำกัดเดิมของ save format)
                ServerMessage::ChunkData {
                    chunk_pos: chunk_pos.to_array(),
                    blocks_rle: rle_encode(&bytes),
                    chiseled: Vec::new(),
                    facings: Vec::new(),
                    containers: Vec::new(),
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
    hotbar: Res<crate::voxel::Hotbar>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    let mut players = Vec::new();
    if let Some((transform, cam)) = camera_query.iter().next() {
        let held = hotbar.slots[hotbar.selected].map(|s| crate::item::item_to_wire(s.item));
        players.push((HOST_PLAYER_ID, transform.translation.to_array(), cam.yaw, held));
    }
    for rp in &remote_players {
        let held = rp.held.map(crate::item::item_to_wire);
        players.push((rp.client_id, rp.target_pos.to_array(), rp.target_yaw, held));
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
    models: Res<PlayerModelAssets>,
    mut remote_players: Query<(Entity, &mut RemotePlayer)>,
    mut chat_ui: ResMut<crate::ui::ChatState>,
    mut fx_writer: MessageWriter<crate::particles::ExplosionFx>,
) {
    while let Some(bytes) = client.receive_message(DefaultChannel::ReliableOrdered) {
        match decode::<ServerMessage>(&bytes) {
            Some(ServerMessage::Welcome { client_id: _, player_number, noise, terrain, spawn_pos, time_of_day, game_mode: _ }) => {
                client_sync.my_number = player_number;
                settings.noise = noise;
                settings.terrain_source = terrain;
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
            Some(ServerMessage::ChunkData { chunk_pos, blocks_rle, chiseled, facings, containers }) => {
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
                client_sync.full_chunks.insert(pos, ReceivedChunk {
                    blocks,
                    chiseled: chiseled_map,
                    facings: facings.into_iter().map(|(i, f)| (i as usize, f)).collect(),
                    chest_slots: containers.iter().filter(|(_, k, _)| *k == 0).map(|(i, _, slots)| {
                        let mut arr = Box::new([None; 27]);
                        for (idx, slot) in slots.iter().enumerate().take(27) {
                            arr[idx] = (*slot).and_then(|s| s.to_stack());
                        }
                        (*i as usize, arr)
                    }).collect(),
                    furnace_slots: containers.iter().filter(|(_, k, _)| *k == 1).map(|(i, _, slots)| {
                        let mut arr = Box::new([None; 3]);
                        for (idx, slot) in slots.iter().enumerate().take(3) {
                            arr[idx] = (*slot).and_then(|s| s.to_stack());
                        }
                        (*i as usize, arr)
                    }).collect(),
                });
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
                        spawn_remote_player(&mut commands, &models, client_id, player_number);
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
            Some(ServerMessage::Chat { from, text }) => {
                chat_ui.push_player(from, text);
            }
            Some(ServerMessage::TimeOfDay { hours }) => {
                settings.time_of_day = hours;
            }
            Some(ServerMessage::Explosion(wire)) => {
                fx_writer.write(crate::particles::ExplosionFx {
                    center: Vec3::from_array(wire.center),
                    rays: wire
                        .rays
                        .iter()
                        .map(|s| crate::voxel::RaySeg {
                            a: Vec3::from_array(s.a),
                            b: Vec3::from_array(s.b),
                            energy: s.energy,
                            dist0: s.dist0,
                        })
                        .collect(),
                    power: wire.power,
                    is_nuke: wire.is_nuke,
                });
            }
            Some(ServerMessage::PlayerAction { client_id, action }) => {
                for (_, mut rp) in remote_players.iter_mut() {
                    if rp.client_id == client_id {
                        if action == 0 {
                            rp.mining_timer = 0.5;
                        }
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
        for (id, pos, yaw, held) in players {
            if id == client_sync.my_id {
                continue;
            }
            let mut found = false;
            for (_, mut rp) in remote_players.iter_mut() {
                if rp.client_id == id {
                    rp.target_pos = Vec3::from_array(pos);
                    rp.target_yaw = yaw;
                    rp.held = held.and_then(|(k, i)| crate::item::item_from_wire(k, i));
                    found = true;
                }
            }
            if !found {
                // ตำแหน่ง (unreliable) มาถึงก่อน PlayerJoined (reliable) ได้ —
                // spawn ไปก่อนด้วยเลข 0 ("Player ?") เดี๋ยว PlayerJoined ตามมาเติม
                spawn_remote_player(&mut commands, &models, id, 0);
            }
        }
    }
}

pub fn client_send_position(
    time: Res<Time>,
    mut timer: ResMut<PositionSendTimer>,
    mut client: ResMut<RenetClient>,
    camera_query: Query<(&Transform, &crate::camera::FreeCamera)>,
    hotbar: Res<crate::voxel::Hotbar>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    if let Some((transform, cam)) = camera_query.iter().next() {
        let held = hotbar.slots[hotbar.selected].map(|s| crate::item::item_to_wire(s.item));
        client.send_message(
            DefaultChannel::Unreliable,
            encode(&ClientMessage::Position {
                pos: transform.translation.to_array(),
                yaw: cam.yaw,
                held,
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

pub fn client_send_actions(mut client: ResMut<RenetClient>, mut pending: ResMut<PendingLocalActions>) {
    for action in pending.0.drain(..) {
        client.send_message(
            DefaultChannel::ReliableOrdered,
            encode(&ClientMessage::Action { action }),
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
        // ออกจากเกม → คืนค่า noise/terrain เดิม ล้างโลกของ host ทิ้ง
        if let Some(prev) = client_sync.prev_noise {
            settings.noise = prev;
        }
        if let Some(prev) = client_sync.prev_terrain {
            settings.terrain_source = prev;
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
        start_client(&mut commands, &mut mp_ui, settings.noise, settings.terrain_source);
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
            BlockEdit::PlaceFacingBlock { .. } => false,
            BlockEdit::SetContainerSlot { .. } => false,
        };
        let _affects_mesh = !matches!(edit, BlockEdit::SetContainerSlot { .. });
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

    // chunk ที่เพื่อนบ้านยังโหลดไม่ครบถูก skip — คืนเข้าคิวไว้ลองใหม่เฟรมหน้า
    // ไม่งั้น block data ใหม่แล้วแต่ mesh ค้างภาพเก่า (แพทเทิร์นเดียวกับคิวน้ำ
    // ใน fluid_simulation_system); lamp refresh เลื่อนตามไปรอบที่ remesh สำเร็จ
    let skipped = crate::voxel::remesh_chunks(&mut commands, &mut world, &mut mp, to_remesh);
    for cp in &skipped {
        edited_chunks.remove(cp);
    }
    chunk_remesh.0.extend(skipped);
    let skipped_water =
        crate::voxel::remesh_water_only(&mut commands, &mut world, &mut mp, water_remesh);
    // น้ำที่ skip เข้าคิวเดียวกัน (รอบหน้ากลายเป็น full remesh — แพงกว่านิดแต่ไม่หาย)
    chunk_remesh.0.extend(skipped_water);
    for cp in edited_chunks {
        crate::voxel::refresh_chunk_lamp_lights(&mut commands, &mut world, cp);
    }
}

pub fn host_broadcast_actions(
    mut server: ResMut<RenetServer>,
    mut pending: ResMut<PendingLocalActions>,
) {
    for action in pending.0.drain(..) {
        server.broadcast_message(
            DefaultChannel::ReliableOrdered,
            encode(&ServerMessage::PlayerAction { client_id: HOST_PLAYER_ID, action }),
        );
    }
}

pub fn player_rig_setup_system(
    mut players: Query<(Entity, &mut PlayerRig)>,
    children_query: Query<&Children>,
    name_query: Query<&Name>,
) {
    for (player_entity, mut rig) in players.iter_mut() {
        if rig.head.is_some()
            && rig.upper_arm_left.is_some()
            && rig.upper_arm_right.is_some()
            && rig.upper_leg_left.is_some()
            && rig.upper_leg_right.is_some()
        {
            continue; // Already fully setup
        }

        // BFS จากบนลงล่างจริงๆ (FIFO) + ยึด "ตัวแรกที่เจอ" — ใน glTF node กลุ่ม
        // (pivot หัวไหล่/สะโพก) กับ mesh ท่อนบนข้างในมัน "ชื่อเดียวกัน" ต้องจับ
        // node กลุ่มซึ่งเป็น ancestor เท่านั้น ถ้าจับ mesh จะหมุนแค่ครึ่งท่อน
        // (ท่อนล่างค้างนิ่ง — บั๊กเดิม: ใช้ Vec::pop เป็น DFS แล้วเขียนทับด้วยตัวหลัง)
        let mut queue = VecDeque::from([player_entity]);
        while let Some(entity) = queue.pop_front() {
            if let Ok(name) = name_query.get(entity) {
                match name.as_str() {
                    "head" => rig.head = rig.head.or(Some(entity)),
                    "upper_arm_right" => rig.upper_arm_right = rig.upper_arm_right.or(Some(entity)),
                    "upper_arm_left" => rig.upper_arm_left = rig.upper_arm_left.or(Some(entity)),
                    "upper_leg_right" => rig.upper_leg_right = rig.upper_leg_right.or(Some(entity)),
                    "upper_leg_left" => rig.upper_leg_left = rig.upper_leg_left.or(Some(entity)),
                    _ => {}
                }
            }
            if let Ok(children) = children_query.get(entity) {
                queue.extend(children.iter());
            }
        }
    }
}

/// อนิเมชัน avatar 4 สถานะ: idle (แขนขานิ่ง), walk (แกว่งสลับ), mine (แขนขวาทุบ),
/// mine+walk (ขา+แขนซ้ายเดินต่อ แขนขวาทุบทับ) — แขนขวาให้ท่าทุบชนะเสมอ
pub fn player_animation_system(
    time: Res<Time>,
    mut players: Query<(&mut RemotePlayer, &PlayerRig, &Transform)>,
    mut transforms: Query<&mut Transform, Without<RemotePlayer>>,
) {
    let dt = time.delta_secs();

    let set_rot_x = |transforms: &mut Query<&mut Transform, Without<RemotePlayer>>,
                         bone: Option<Entity>,
                         angle: f32| {
        if let Some(entity) = bone {
            if let Ok(mut tf) = transforms.get_mut(entity) {
                tf.rotation = Quat::from_rotation_x(angle);
            }
        }
    };

    for (mut player, rig, player_transform) in players.iter_mut() {
        // เป้าของ interpolation อยู่ระดับ "กลางตัว" (target_pos ที่ส่งกันคือระดับตา)
        // — บั๊กเดิมเทียบ translation กับ target_pos ตรงๆ เลยห่างกัน ~0.7 ตลอดกาล
        // ระยะไม่เคยต่ำกว่า threshold = ท่าเดินค้างแม้ยืนนิ่ง
        let target = player.target_pos
            - Vec3::Y * (crate::camera::EYE_HEIGHT - crate::camera::PLAYER_HEIGHT / 2.0);
        let delta = player_transform.translation - target;
        // ดูเฉพาะแนวราบ — ตก/กระโดดไม่ใช่ท่าเดิน; ยังไม่เคยได้ตำแหน่งจริงก็ไม่เดิน
        let walking = player.target_pos.y > -900.0
            && Vec2::new(delta.x, delta.z).length() > 0.05;

        if player.mining_timer > 0.0 {
            player.mining_timer -= dt;
        }
        let mining = player.mining_timer > 0.0;

        if walking {
            // wrap ไว้ไม่ให้ phase สะสมยาว (เดินนานๆ f32 เสียความละเอียด)
            player.walk_phase = (player.walk_phase + dt * 10.0) % std::f32::consts::TAU;
        } else {
            // idle: ลู่เข้าหาจุด sin=0 ที่ "ใกล้ที่สุด" (คูณของ π) — เดิมลาก
            // phase สะสมทั้งก้อนกลับ 0 แขนขาเลยสะบัดย้อนหลายรอบตอนหยุดเดิน
            let settle = (player.walk_phase / std::f32::consts::PI).round() * std::f32::consts::PI;
            player.walk_phase += (settle - player.walk_phase) * (dt * 5.0).min(1.0);
        }
        let walk_angle = player.walk_phase.sin() * 0.6;

        // ขา + แขนซ้าย: ตามท่าเดิน/idle เสมอ (เคสทุบระหว่างเดิน = ขายังเดินต่อ)
        set_rot_x(&mut transforms, rig.upper_leg_left, walk_angle);
        set_rot_x(&mut transforms, rig.upper_leg_right, -walk_angle);
        set_rot_x(&mut transforms, rig.upper_arm_left, -walk_angle);

        // แขนขวา: ท่าทุบทับทุกอย่าง ไม่ทุบค่อยตามเดิน/idle
        if mining {
            let mine_swing = (time.elapsed_secs() * 15.0).sin();
            let mine_angle = std::f32::consts::FRAC_PI_4
                - (mine_swing * 0.5 + 0.5) * std::f32::consts::FRAC_PI_2;
            set_rot_x(&mut transforms, rig.upper_arm_right, mine_angle);
        } else {
            set_rot_x(&mut transforms, rig.upper_arm_right, walk_angle);
        }
    }
}

pub fn host_broadcast_fx(
    mut server: ResMut<RenetServer>,
    mut pending: ResMut<PendingNetFx>,
) {
    for fx in pending.0.drain(..) {
        server.broadcast_message(
            DefaultChannel::ReliableOrdered,
            encode(&ServerMessage::Explosion(fx)),
        );
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

pub fn sanitize_chat(text: &str) -> String {
    text.trim().chars().take(100).collect()
}
