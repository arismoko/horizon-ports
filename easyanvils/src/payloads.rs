use crate::config::ServerConfig;
use mod_api::{PayloadId, ToClient, ToServer, Wire};
use std::sync::OnceLock;

pub const CHANNEL_SERVER_CONFIG: &str = "easyanvils:server_config";
pub const CHANNEL_RENAME_ITEM: &str = "easyanvils:rename_item";

#[derive(Clone, Debug, PartialEq, Eq, Wire)]
pub struct RenameItemPayload {
    pub window: i32,
    pub name: String,
}

pub struct Channels {
    pub server_config: ToClient<ServerConfig>,
    pub rename_item: ToServer<RenameItemPayload>,
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

pub fn id_rename_item() -> PayloadId {
    PayloadId(CHANNEL_RENAME_ITEM.into())
}
