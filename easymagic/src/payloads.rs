use crate::config::ServerConfig;
use mod_api::{ItemStack, PayloadId, ToClient, ToServer, Wire};
use std::sync::OnceLock;

pub const CHANNEL_SERVER_CONFIG: &str = "easymagic:server_config";
pub const CHANNEL_CLUES: &str = "easymagic:clues";
pub const CHANNEL_SHELF_QUERY: &str = "easymagic:shelf_query";
pub const CHANNEL_SHELF_ITEM: &str = "easymagic:shelf_item";

#[derive(Clone, Debug, PartialEq, Eq, Wire)]
pub struct EnchantClue {
    pub id: String,
    pub level: u32,
    pub curse: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Wire)]
pub struct CluesPayload {
    pub window: i32,
    pub costs: [i32; 3],
    pub seed: i32,
    pub rows: [Vec<EnchantClue>; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Wire)]
pub struct ShelfQueryPayload {
    pub pos: [i32; 3],
    pub slot: u8,
}

#[derive(Clone, Debug, PartialEq, Wire)]
pub struct ShelfItemPayload {
    pub pos: [i32; 3],
    pub slot: u8,
    pub stack: Option<ItemStack>,
}

pub struct Channels {
    pub server_config: ToClient<ServerConfig>,
    pub clues: ToClient<CluesPayload>,
    pub shelf_query: ToServer<ShelfQueryPayload>,
    pub shelf_item: ToClient<ShelfItemPayload>,
}

static CHANNELS: OnceLock<Channels> = OnceLock::new();

pub fn init_channels(channels: Channels) {
    let _ = CHANNELS.set(channels);
}

pub fn channels() -> Option<&'static Channels> {
    CHANNELS.get()
}

pub fn id_server_config() -> PayloadId {
    PayloadId(CHANNEL_SERVER_CONFIG.into())
}

pub fn id_clues() -> PayloadId {
    PayloadId(CHANNEL_CLUES.into())
}

pub fn id_shelf_query() -> PayloadId {
    PayloadId(CHANNEL_SHELF_QUERY.into())
}

pub fn id_shelf_item() -> PayloadId {
    PayloadId(CHANNEL_SHELF_ITEM.into())
}
