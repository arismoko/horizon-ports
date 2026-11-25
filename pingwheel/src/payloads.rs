use mod_api::{ItemStack, PayloadId, ToClient, ToServer, Wire};
use std::sync::OnceLock;

pub const CHANNEL_PING_LOCATION_C2S: &str = "pingwheel:ping_location_c2s";
pub const CHANNEL_PING_LOCATION_S2C: &str = "pingwheel:ping_location_s2c";
pub const CHANNEL_UPDATE_CHANNEL_C2S: &str = "pingwheel:update_channel_c2s";

/// Client -> Server: request to broadcast a ping at `pos` (optionally tracking `entity_id`).
#[derive(Clone, Debug, PartialEq, Wire)]
pub struct PingLocationC2S {
    pub channel: String,
    pub pos: [f64; 3],
    pub entity_id: Option<u32>,
    pub sequence: i32,
}

/// Server -> Client: relayed ping with resolved author metadata and optional dropped item stack.
#[derive(Clone, Debug, PartialEq, Wire)]
pub struct PingLocationS2C {
    pub channel: String,
    pub pos: [f64; 3],
    pub entity_id: Option<u32>,
    pub sequence: i32,
    pub dimension: String,
    pub author: u128,
    pub author_name: String,
    pub team_color: Option<u32>,
    pub item_stack: Option<ItemStack>,
}

/// Client -> Server: notify the server of the player's active ping channel.
#[derive(Clone, Debug, PartialEq, Eq, Wire)]
pub struct UpdateChannelC2S {
    pub channel: String,
}

pub struct Channels {
    pub ping_location_c2s: ToServer<PingLocationC2S>,
    pub ping_location_s2c: ToClient<PingLocationS2C>,
    pub update_channel_c2s: ToServer<UpdateChannelC2S>,
}

static CHANNELS: OnceLock<Channels> = OnceLock::new();

pub fn init_channels(channels: Channels) {
    let _ = CHANNELS.set(channels);
}

pub fn channels() -> Option<&'static Channels> {
    CHANNELS.get()
}

pub fn id_ping_location_c2s() -> PayloadId {
    PayloadId(CHANNEL_PING_LOCATION_C2S.into())
}

pub fn id_ping_location_s2c() -> PayloadId {
    PayloadId(CHANNEL_PING_LOCATION_S2C.into())
}

pub fn id_update_channel_c2s() -> PayloadId {
    PayloadId(CHANNEL_UPDATE_CHANNEL_C2S.into())
}
