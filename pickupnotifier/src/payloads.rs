//! Optional typed clientbound payloads for server-assisted pick-up notifications.

use mod_api::{ItemStack, PayloadId, ToClient, Wire};
use std::sync::OnceLock;

pub const CHANNEL_TAKE_ITEM: &str = "pickupnotifier:take_item";
pub const CHANNEL_TAKE_XP: &str = "pickupnotifier:take_xp";

/// Sent from server to client when a player collects an item stack or shot arrow.
#[derive(Clone, Debug, PartialEq, Wire)]
pub struct TakeItemPayload {
    pub stack: ItemStack,
    pub amount: u32,
    pub is_arrow: bool,
}

/// Sent from server to client when a player collects an experience orb.
#[derive(Clone, Debug, PartialEq, Eq, Wire)]
pub struct TakeXpPayload {
    pub value: u32,
    pub count: u32,
}

pub struct Channels {
    pub take_item: ToClient<TakeItemPayload>,
    pub take_xp: ToClient<TakeXpPayload>,
}

static CHANNELS: OnceLock<Channels> = OnceLock::new();

pub fn init_channels(channels: Channels) {
    let _ = CHANNELS.set(channels);
}

pub fn channels() -> Option<&'static Channels> {
    CHANNELS.get()
}

pub fn id_take_item() -> PayloadId {
    PayloadId(CHANNEL_TAKE_ITEM.into())
}

pub fn id_take_xp() -> PayloadId {
    PayloadId(CHANNEL_TAKE_XP.into())
}
