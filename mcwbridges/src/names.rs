//! Registry IDs of every Macaw's Bridges block family, matching `BlockInit` in 3.1.2.

/// `Bridge_Block` (34 stone & balustrade bridges) — stone place sound.
pub const STONE_BRIDGES: &[&str] = &[
    "mcwbridges:andesite_bridge",
    "mcwbridges:balustrade_andesite_bridge",
    "mcwbridges:balustrade_blackstone_bridge",
    "mcwbridges:balustrade_bricks_bridge",
    "mcwbridges:balustrade_cobblestone_bridge",
    "mcwbridges:balustrade_deepslate_bricks_bridge",
    "mcwbridges:balustrade_deepslate_tiles_bridge",
    "mcwbridges:balustrade_diorite_bridge",
    "mcwbridges:balustrade_end_stone_bricks_bridge",
    "mcwbridges:balustrade_granite_bridge",
    "mcwbridges:balustrade_mossy_cobblestone_bridge",
    "mcwbridges:balustrade_mossy_stone_bricks_bridge",
    "mcwbridges:balustrade_mud_bricks_bridge",
    "mcwbridges:balustrade_nether_bricks_bridge",
    "mcwbridges:balustrade_orange_sandstone_bridge",
    "mcwbridges:balustrade_prismarine_bricks_bridge",
    "mcwbridges:balustrade_sandstone_bridge",
    "mcwbridges:balustrade_stone_bricks_bridge",
    "mcwbridges:blackstone_bridge",
    "mcwbridges:brick_bridge",
    "mcwbridges:cobblestone_bridge",
    "mcwbridges:deepslate_brick_bridge",
    "mcwbridges:deepslate_tile_bridge",
    "mcwbridges:diorite_bridge",
    "mcwbridges:end_stone_bricks_bridge",
    "mcwbridges:granite_bridge",
    "mcwbridges:mossy_cobblestone_bridge",
    "mcwbridges:mossy_stone_brick_bridge",
    "mcwbridges:mud_brick_bridge",
    "mcwbridges:nether_bricks_bridge",
    "mcwbridges:orange_sandstone_bridge",
    "mcwbridges:prismarine_bricks_bridge",
    "mcwbridges:sandstone_bridge",
    "mcwbridges:stone_brick_bridge",
];

/// `Log_Bridge` (14: 12 woods + asian_red + glass) — extends `Bridge_Block`, wood place sound.
pub const LOG_BRIDGES: &[&str] = &[
    "mcwbridges:acacia_log_bridge_middle",
    "mcwbridges:asian_red_bridge",
    "mcwbridges:birch_log_bridge_middle",
    "mcwbridges:cherry_log_bridge_middle",
    "mcwbridges:crimson_log_bridge_middle",
    "mcwbridges:dark_oak_log_bridge_middle",
    "mcwbridges:glass_bridge",
    "mcwbridges:jungle_log_bridge_middle",
    "mcwbridges:mangrove_log_bridge_middle",
    "mcwbridges:oak_log_bridge_middle",
    "mcwbridges:pale_oak_log_bridge_middle",
    "mcwbridges:poplar_log_bridge_middle",
    "mcwbridges:spruce_log_bridge_middle",
    "mcwbridges:warped_log_bridge_middle",
];

/// `Iron_Bridge` (1) — extends `Bridge_Block`, metal place sound.
pub const IRON_BRIDGES: &[&str] = &["mcwbridges:iron_bridge"];

/// `Bridge_Block_Rope` + `Bamboo_Bridge` (14: 12 rope woods + 2 bamboo) — wood place sound.
pub const ROPE_BRIDGES: &[&str] = &[
    "mcwbridges:bamboo_bridge",
    "mcwbridges:dry_bamboo_bridge",
    "mcwbridges:rope_acacia_bridge",
    "mcwbridges:rope_birch_bridge",
    "mcwbridges:rope_cherry_bridge",
    "mcwbridges:rope_crimson_bridge",
    "mcwbridges:rope_dark_oak_bridge",
    "mcwbridges:rope_jungle_bridge",
    "mcwbridges:rope_mangrove_bridge",
    "mcwbridges:rope_oak_bridge",
    "mcwbridges:rope_pale_oak_bridge",
    "mcwbridges:rope_poplar_bridge",
    "mcwbridges:rope_spruce_bridge",
    "mcwbridges:rope_warped_bridge",
];

/// `Bridge_Stairs` (46 variants).
pub const BRIDGE_STAIRS: &[&str] = &[
    "mcwbridges:acacia_log_bridge_stair",
    "mcwbridges:acacia_rope_bridge_stair",
    "mcwbridges:andesite_bridge_stair",
    "mcwbridges:asian_red_bridge_stair",
    "mcwbridges:bamboo_bridge_stair",
    "mcwbridges:birch_log_bridge_stair",
    "mcwbridges:birch_rope_bridge_stair",
    "mcwbridges:blackstone_bridge_stair",
    "mcwbridges:brick_bridge_stair",
    "mcwbridges:cherry_log_bridge_stair",
    "mcwbridges:cherry_rope_bridge_stair",
    "mcwbridges:cobblestone_bridge_stair",
    "mcwbridges:crimson_log_bridge_stair",
    "mcwbridges:crimson_rope_bridge_stair",
    "mcwbridges:dark_oak_log_bridge_stair",
    "mcwbridges:dark_oak_rope_bridge_stair",
    "mcwbridges:deepslate_brick_bridge_stair",
    "mcwbridges:deepslate_tile_bridge_stair",
    "mcwbridges:diorite_bridge_stair",
    "mcwbridges:dry_bamboo_bridge_stair",
    "mcwbridges:end_stone_bricks_bridge_stair",
    "mcwbridges:glass_bridge_stair",
    "mcwbridges:granite_bridge_stair",
    "mcwbridges:iron_bridge_stair",
    "mcwbridges:jungle_log_bridge_stair",
    "mcwbridges:jungle_rope_bridge_stair",
    "mcwbridges:mangrove_log_bridge_stair",
    "mcwbridges:mangrove_rope_bridge_stair",
    "mcwbridges:mossy_cobblestone_bridge_stair",
    "mcwbridges:mossy_stone_bridge_stair",
    "mcwbridges:mud_brick_bridge_stair",
    "mcwbridges:nether_bricks_bridge_stair",
    "mcwbridges:oak_log_bridge_stair",
    "mcwbridges:oak_rope_bridge_stair",
    "mcwbridges:pale_oak_log_bridge_stair",
    "mcwbridges:pale_oak_rope_bridge_stair",
    "mcwbridges:poplar_log_bridge_stair",
    "mcwbridges:poplar_rope_bridge_stair",
    "mcwbridges:prismarine_bricks_bridge_stair",
    "mcwbridges:red_sandstone_bridge_stair",
    "mcwbridges:sandstone_bridge_stair",
    "mcwbridges:spruce_log_bridge_stair",
    "mcwbridges:spruce_rope_bridge_stair",
    "mcwbridges:stone_brick_bridge_stair",
    "mcwbridges:warped_log_bridge_stair",
    "mcwbridges:warped_rope_bridge_stair",
];

/// `Bridge_Support` (34 pier variants).
pub const BRIDGE_SUPPORTS: &[&str] = &[
    "mcwbridges:acacia_bridge_pier",
    "mcwbridges:andesite_bridge_pier",
    "mcwbridges:asian_red_bridge_pier",
    "mcwbridges:bamboo_bridge_pier",
    "mcwbridges:birch_bridge_pier",
    "mcwbridges:blackstone_bridge_pier",
    "mcwbridges:brick_bridge_pier",
    "mcwbridges:cherry_bridge_pier",
    "mcwbridges:cobblestone_bridge_pier",
    "mcwbridges:crimson_bridge_pier",
    "mcwbridges:dark_oak_bridge_pier",
    "mcwbridges:deepslate_brick_bridge_pier",
    "mcwbridges:deepslate_tile_bridge_pier",
    "mcwbridges:diorite_bridge_pier",
    "mcwbridges:dry_bamboo_bridge_pier",
    "mcwbridges:end_stone_bricks_bridge_pier",
    "mcwbridges:glass_bridge_pier",
    "mcwbridges:granite_bridge_pier",
    "mcwbridges:iron_bridge_pier",
    "mcwbridges:jungle_bridge_pier",
    "mcwbridges:mangrove_bridge_pier",
    "mcwbridges:mossy_cobblestone_bridge_pier",
    "mcwbridges:mossy_stone_bridge_pier",
    "mcwbridges:mud_brick_bridge_pier",
    "mcwbridges:nether_bricks_bridge_pier",
    "mcwbridges:oak_bridge_pier",
    "mcwbridges:pale_oak_bridge_pier",
    "mcwbridges:poplar_bridge_pier",
    "mcwbridges:prismarine_bricks_bridge_pier",
    "mcwbridges:red_sandstone_bridge_pier",
    "mcwbridges:sandstone_bridge_pier",
    "mcwbridges:spruce_bridge_pier",
    "mcwbridges:stone_bridge_pier",
    "mcwbridges:warped_bridge_pier",
];

/// `Rail_Bridge` (12 wood variants).
pub const RAIL_BRIDGES: &[&str] = &[
    "mcwbridges:acacia_rail_bridge",
    "mcwbridges:birch_rail_bridge",
    "mcwbridges:cherry_rail_bridge",
    "mcwbridges:crimson_rail_bridge",
    "mcwbridges:dark_oak_rail_bridge",
    "mcwbridges:jungle_rail_bridge",
    "mcwbridges:mangrove_rail_bridge",
    "mcwbridges:oak_rail_bridge",
    "mcwbridges:pale_oak_rail_bridge",
    "mcwbridges:poplar_rail_bridge",
    "mcwbridges:spruce_rail_bridge",
    "mcwbridges:warped_rail_bridge",
];

pub const BRIDGE_TORCH: &str = "mcwbridges:bridge_torch";
pub const BRIDGE_LANTERN: &str = "mcwbridges:bridge_lantern";
pub const PLIERS: &str = "mcwbridges:pliers";

pub fn is_bridge_block(id: &str) -> bool {
    STONE_BRIDGES.binary_search(&id).is_ok()
        || LOG_BRIDGES.binary_search(&id).is_ok()
        || IRON_BRIDGES.binary_search(&id).is_ok()
}

pub fn is_rope_bridge(id: &str) -> bool {
    ROPE_BRIDGES.binary_search(&id).is_ok()
}

pub fn is_bridge_stair(id: &str) -> bool {
    BRIDGE_STAIRS.binary_search(&id).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_sorted_unique(slice: &[&str]) {
        for pair in slice.windows(2) {
            assert!(pair[0] < pair[1], "not strictly sorted: {pair:?}");
        }
    }

    #[test]
    fn all_lists_sorted_and_total_157_blocks() {
        for list in [
            STONE_BRIDGES,
            LOG_BRIDGES,
            IRON_BRIDGES,
            ROPE_BRIDGES,
            BRIDGE_STAIRS,
            BRIDGE_SUPPORTS,
            RAIL_BRIDGES,
        ] {
            assert_sorted_unique(list);
        }
        let total = STONE_BRIDGES.len()
            + LOG_BRIDGES.len()
            + IRON_BRIDGES.len()
            + ROPE_BRIDGES.len()
            + BRIDGE_STAIRS.len()
            + BRIDGE_SUPPORTS.len()
            + RAIL_BRIDGES.len()
            + 2; // bridge_torch + bridge_lantern
        assert_eq!(total, 157);
    }
}
