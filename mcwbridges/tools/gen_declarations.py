#!/usr/bin/env python3
"""Generate Macaw's Bridges Sunrise block, item, and creative tab declarations.

Usage:
  python3 tools/gen_declarations.py

Produces our own Sunrise declaration files under `data/mcwbridges/sunrise/`:
- `block/*.json` (157 blocks)
- `item/pliers.json` (1 standalone item)
- `creative_tab/mcwbridges.json` (1 creative tab, 158 items)

Contains no assets or data copied from the original jar; textures, models,
blockstates, item definitions, lang, recipes, loot tables, tags, and
advancements are supplied by the player's jar via `[[jar]]`.
"""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SUNRISE_DIR = ROOT / "data" / "mcwbridges" / "sunrise"

WOODS = [
    ("oak", "minecraft:oak_planks"),
    ("birch", "minecraft:birch_planks"),
    ("acacia", "minecraft:acacia_planks"),
    ("spruce", "minecraft:spruce_planks"),
    ("jungle", "minecraft:jungle_planks"),
    ("dark_oak", "minecraft:dark_oak_planks"),
    ("crimson", "minecraft:crimson_planks"),
    ("warped", "minecraft:warped_planks"),
    ("mangrove", "minecraft:mangrove_planks"),
    ("cherry", "minecraft:cherry_planks"),
    ("pale_oak", "minecraft:pale_oak_planks"),
    ("poplar", "minecraft:poplar_planks"),
]

# Wood order for rail/pier/stair/rope differs slightly in the creative tab; we record exact tab order below.

STONES = [
    ("brick", "bricks", "brick", "minecraft:bricks"),
    ("sandstone", "sandstone", "sandstone", "minecraft:sandstone"),
    ("stone_brick", "stone_bricks", "stone", "minecraft:stone_bricks"),
    ("orange_sandstone", "orange_sandstone", "red_sandstone", "minecraft:red_sandstone"),
    ("blackstone", "blackstone", "blackstone", "minecraft:blackstone"),
    ("mossy_stone_brick", "mossy_stone_bricks", "mossy_stone", "minecraft:mossy_stone_bricks"),
    ("deepslate_brick", "deepslate_bricks", "deepslate_brick", "minecraft:deepslate_bricks"),
    ("deepslate_tile", "deepslate_tiles", "deepslate_tile", "minecraft:deepslate_tiles"),
    ("mud_brick", "mud_bricks", "mud_brick", "minecraft:mud_bricks"),
    ("cobblestone", "cobblestone", "cobblestone", "minecraft:cobblestone"),
    ("mossy_cobblestone", "mossy_cobblestone", "mossy_cobblestone", "minecraft:mossy_cobblestone"),
    ("andesite", "andesite", "andesite", "minecraft:andesite"),
    ("granite", "granite", "granite", "minecraft:granite"),
    ("diorite", "diorite", "diorite", "minecraft:diorite"),
    ("prismarine_bricks", "prismarine_bricks", "prismarine_bricks", "minecraft:prismarine_bricks"),
    ("nether_bricks", "nether_bricks", "nether_bricks", "minecraft:nether_bricks"),
    ("end_stone_bricks", "end_stone_bricks", "end_stone_bricks", "minecraft:end_stone_bricks"),
]

FUEL_300 = {
    "burn_time": 300,
    "speed_multiplier": "minecraft:cooking/speed_default",
}


def box(x1, y1, z1, x2, y2, z2):
    return [x1 / 16.0, y1 / 16.0, z1 / 16.0, x2 / 16.0, y2 / 16.0, z2 / 16.0]


def lore(key):
    return [{"translate": key, "color": "gray", "italic": False}]


# --- VoxelShape tables matching original Block.box(...) definitions ---

BASE_BOX = box(0.0, 0.0, 0.0, 15.99, 4.0, 15.99)
BASE_COLLISION_BOX = box(0.0, 0.0, 0.0, 15.99, 3.0, 15.99)


def bridge_shapes(side_y_min, side_y_max, thickness, *, rope_variants=False):
    s0 = box(0.0, side_y_min, 16.0 - thickness, 16.0, side_y_max, 16.0)
    s90 = box(0.0, side_y_min, 0.0, thickness, side_y_max, 16.0)
    s180 = box(0.0, side_y_min, 0.0, 16.0, side_y_max, thickness)
    s270 = box(16.0 - thickness, side_y_min, 0.0, 16.0, side_y_max, 16.0)

    values = {
        "base": [BASE_BOX],
        "middle_ns": [BASE_BOX, s0, s180],
        "middle_ew": [BASE_BOX, s90, s270],
        "corner_ne": [BASE_BOX, s0, s90],
        "corner_nw": [BASE_BOX, s270, s0],
        "corner_se": [BASE_BOX, s180, s90],
        "corner_sw": [BASE_BOX, s180, s270],
        "side_n": [BASE_BOX, s180],
        "side_e": [BASE_BOX, s270],
        "side_s": [BASE_BOX, s0],
        "side_w": [BASE_BOX, s90],
    }
    if rope_variants:
        values.update({
            "middle_end_n": [BASE_BOX, s90, s270],
            "middle_end_e": [BASE_BOX, s0, s180],
            "middle_end_s": [BASE_BOX, s90, s270],
            "middle_end_w": [BASE_BOX, s0, s180],
            "end_n_left": [BASE_BOX, s270],
            "end_n_right": [BASE_BOX, s270],
            "end_e_left": [BASE_BOX, s90],
            "end_e_right": [BASE_BOX, s90],
            "end_s_left": [BASE_BOX, s0],
            "end_s_right": [BASE_BOX, s180],
            "end_w_left": [BASE_BOX, s180],
            "end_w_right": [BASE_BOX, s0],
            "base_toggled": [BASE_BOX],
        })
    return {"properties": ["connection"], "values": values}


def bridge_collision_shapes(*, rope_variants=False):
    table = bridge_shapes(2.0, 25.0, 1.0, rope_variants=rope_variants)
    if rope_variants:
        table["values"]["base_toggled"] = [BASE_COLLISION_BOX]
    return table


BRIDGE_BLOCK_OUTLINE = bridge_shapes(3.0, 16.0, 3.0)
LOG_BRIDGE_OUTLINE = bridge_shapes(2.0, 15.0, 2.0)
IRON_BRIDGE_OUTLINE = bridge_shapes(1.0, 16.0, 1.0)
ROPE_BRIDGE_OUTLINE = bridge_shapes(2.0, 16.0, 1.0, rope_variants=True)
BAMBOO_BRIDGE_OUTLINE = bridge_shapes(3.0, 16.0, 3.0, rope_variants=True)

BRIDGE_BLOCK_COLLISION = bridge_collision_shapes(rope_variants=False)
ROPE_BRIDGE_COLLISION = bridge_collision_shapes(rope_variants=True)

BRIDGE_CONNECTIONS = [
    "base",
    "middle_ns",
    "middle_ew",
    "corner_ne",
    "corner_nw",
    "corner_se",
    "corner_sw",
    "side_n",
    "side_e",
    "side_s",
    "side_w",
]

ROPE_CONNECTIONS = [
    "base",
    "middle_ns",
    "middle_ew",
    "middle_end_n",
    "middle_end_e",
    "middle_end_s",
    "middle_end_w",
    "corner_ne",
    "corner_nw",
    "corner_se",
    "corner_sw",
    "side_n",
    "side_e",
    "side_s",
    "side_w",
    "end_n_left",
    "end_n_right",
    "end_e_left",
    "end_e_right",
    "end_s_left",
    "end_s_right",
    "end_w_left",
    "end_w_right",
    "base_toggled",
]


def stair_shapes():
    bottom = box(0.0, 0.01, 0.0, 16.0, 8.0, 16.0)
    steps = {
        "north": [box(0.0, 8.0, 0.0, 8.0, 16.0, 16.0), bottom],
        "east": [box(0.0, 8.0, 0.0, 16.0, 16.0, 8.0), bottom],
        "south": [box(8.0, 8.0, 0.0, 16.0, 16.0, 16.0), bottom],
        "west": [box(0.0, 8.0, 8.0, 16.0, 16.0, 16.0), bottom],
    }
    e_side = box(0.0, 0.0, 0.0, 1.0, 32.0, 16.0)
    s_side = box(0.0, 0.0, 15.0, 16.0, 32.0, 16.0)
    w_side = box(15.0, 0.0, 0.0, 16.0, 32.0, 16.0)
    n_side = box(0.0, 0.0, 0.0, 16.0, 32.0, 1.0)

    outline = {"properties": ["facing"], "values": steps}
    collision_vals = {
        "north,base": [n_side, s_side, *steps["north"]],
        "north,double": steps["north"],
        "north,left": [s_side, *steps["north"]],
        "north,right": [n_side, *steps["north"]],
        "south,base": [n_side, s_side, *steps["south"]],
        "south,double": steps["south"],
        "south,left": [n_side, *steps["south"]],
        "south,right": [s_side, *steps["south"]],
        "east,base": [e_side, w_side, *steps["east"]],
        "east,double": steps["east"],
        "east,left": [e_side, *steps["east"]],
        "east,right": [w_side, *steps["east"]],
        "west,base": [e_side, w_side, *steps["west"]],
        "west,double": steps["west"],
        "west,left": [w_side, *steps["west"]],
        "west,right": [e_side, *steps["west"]],
    }
    collision = {"properties": ["facing", "connection"], "values": collision_vals}
    return outline, collision


STAIR_OUTLINE, STAIR_COLLISION = stair_shapes()
OCCLUSION_BOX = {"default": [box(0.0, 0.01, 0.0, 16.0, 16.0, 16.0)]}

TORCH_OUTLINE = {
    "properties": ["facing", "lightstate"],
    "values": {
        "east,bridge": [box(6.2, 0.0, 0.0, 9.8, 8.0, 2.8)],
        "south,bridge": [box(13.2, 0.0, 6.2, 16.0, 8.0, 9.8)],
        "west,bridge": [box(6.2, 0.0, 13.2, 9.8, 8.0, 16.0)],
        "north,bridge": [box(0.0, 0.0, 6.2, 2.8, 8.0, 9.8)],
        "east,stair": [box(6.2, 6.0, 0.0, 9.8, 16.0, 2.8)],
        "south,stair": [box(13.2, 6.0, 6.2, 16.0, 16.0, 9.8)],
        "west,stair": [box(6.2, 6.0, 13.2, 9.8, 16.0, 16.0)],
        "north,stair": [box(0.0, 6.0, 6.2, 2.8, 16.0, 9.8)],
    },
}

LANTERN_OUTLINE = {
    "properties": ["facing", "lightstate"],
    "values": {
        "east,bridge": [box(5.5, 0.0, 0.0, 10.5, 7.0, 4.0)],
        "south,bridge": [box(12.0, 0.0, 5.5, 16.0, 7.0, 10.5)],
        "west,bridge": [box(5.5, 0.0, 12.0, 10.5, 7.0, 16.0)],
        "north,bridge": [box(0.0, 0.0, 5.5, 4.0, 7.0, 10.5)],
        "east,stair": [box(5.5, 7.0, 0.0, 10.5, 16.0, 4.0)],
        "south,stair": [box(12.0, 7.0, 5.5, 16.0, 16.0, 10.5)],
        "west,stair": [box(5.5, 7.0, 12.0, 10.5, 16.0, 16.0)],
        "north,stair": [box(0.0, 7.0, 5.5, 4.0, 16.0, 10.5)],
    },
}


def make_components(fuel=False, lore_key=None):
    comps = {}
    if fuel:
        comps["minecraft:cooking_fuel"] = FUEL_300
    if lore_key:
        comps["minecraft:lore"] = lore(lore_key)
    return comps


def bridge_block_decl(template, outline, *, fuel=False, lore_key="mcwbridges.bridges.desc"):
    decl = {
        "template": template,
        "properties": {"occludes": False},
        "shapes": outline,
        "collision_shapes": BRIDGE_BLOCK_COLLISION,
        "state_properties": {
            "connection": {"type": "enum", "values": BRIDGE_CONNECTIONS},
        },
        "default_state": {"connection": "base"},
    }
    comps = make_components(fuel=fuel, lore_key=lore_key)
    if comps:
        decl["components"] = comps
    return decl


def rope_bridge_decl(template, outline, *, fuel=True, lore_key="mcwbridges.bridges.desc"):
    decl = {
        "template": template,
        "properties": {"occludes": False},
        "shapes": outline,
        "collision_shapes": ROPE_BRIDGE_COLLISION,
        "state_properties": {
            "connection": {"type": "enum", "values": ROPE_CONNECTIONS},
            "facing": {"type": "enum", "values": ["north", "east"]},
        },
        "default_state": {"connection": "base", "facing": "north"},
    }
    comps = make_components(fuel=fuel, lore_key=lore_key)
    if comps:
        decl["components"] = comps
    return decl


def stair_decl(template, *, fuel=False, lore_key=None):
    decl = {
        "template": template,
        "properties": {"occludes": False},
        "shapes": STAIR_OUTLINE,
        "collision_shapes": STAIR_COLLISION,
        "state_properties": {
            "facing": {"type": "enum", "values": ["north", "south", "west", "east"]},
            "connection": {"type": "enum", "values": ["base", "double", "left", "right"]},
        },
        "default_state": {"facing": "north", "connection": "base"},
    }
    comps = make_components(fuel=fuel, lore_key=lore_key)
    if comps:
        decl["components"] = comps
    return decl


def support_decl(template, *, fuel=False):
    decl = {
        "template": template,
        "properties": {"occludes": False},
        "shapes": OCCLUSION_BOX,
        "state_properties": {
            "waterlogged": {"type": "boolean"},
        },
        "default_state": {"waterlogged": "false"},
    }
    comps = make_components(fuel=fuel)
    if comps:
        decl["components"] = comps
    return decl


def rail_decl(template):
    return {
        "template": template,
        "properties": {"occludes": False},
        "shapes": OCCLUSION_BOX,
        "state_properties": {
            "facing": {"type": "enum", "values": ["north", "south", "west", "east"]},
            "powered": {"type": "boolean"},
        },
        "default_state": {"facing": "north", "powered": "false"},
        "components": make_components(fuel=True),
    }


def light_decl(outline):
    # Original: BlockBehaviour.Properties.of().mapColor(MapColor.WOOD).strength(0.8f, 2.0f)
    #           .instabreak().sound(SoundType.WOOD).lightLevel(s -> 15)
    # Note: instabreak() sets hardness = 0.0f while keeping explosionResistance = 2.0f.
    return {
        "template": "minecraft:oak_planks",
        "properties": {
            "hardness": 0.0,
            "explosion_resistance": 2.0,
            "light": 15,
            "requires_tool": False,
            "map_color": 13,
            "sounds": "minecraft:oak_planks",
            "occludes": False,
        },
        "shapes": outline,
        "collision_shapes": {"default": []},
        "state_properties": {
            "facing": {"type": "enum", "values": ["north", "south", "west", "east"]},
            "lightstate": {"type": "enum", "values": ["bridge", "stair"]},
        },
        "default_state": {"facing": "north", "lightstate": "bridge"},
        "components": {
            "!minecraft:cooking_fuel": {},
            "minecraft:lore": lore("mcwbridges.light.desc"),
        },
    }


def build_blocks():
    blocks = {}

    # Iron bridge, pier, stair
    blocks["iron_bridge"] = bridge_block_decl("minecraft:iron_block", IRON_BRIDGE_OUTLINE)
    blocks["iron_bridge_pier"] = support_decl("minecraft:iron_block")
    blocks["iron_bridge_stair"] = stair_decl("minecraft:iron_block")

    # 12 wood families
    for wood, planks in WOODS:
        blocks[f"{wood}_log_bridge_middle"] = bridge_block_decl(
            planks, LOG_BRIDGE_OUTLINE, fuel=True
        )
        blocks[f"rope_{wood}_bridge"] = rope_bridge_decl(
            planks, ROPE_BRIDGE_OUTLINE, fuel=True
        )
        blocks[f"{wood}_rail_bridge"] = rail_decl(planks)
        # Original copies WARPED_PLANKS for mangrove_bridge_pier (exact parity with BlockInit).
        pier_template = "minecraft:warped_planks" if wood == "mangrove" else planks
        blocks[f"{wood}_bridge_pier"] = support_decl(pier_template, fuel=True)
        # Original uses FuelBlockItemWithInfo for cherry, pale_oak, and poplar bridge stairs.
        stair_lore = (
            "mcwbridges.bridges.desc"
            if wood in ("cherry", "pale_oak", "poplar")
            else None
        )
        blocks[f"{wood}_log_bridge_stair"] = stair_decl(
            planks, fuel=True, lore_key=stair_lore
        )
        blocks[f"{wood}_rope_bridge_stair"] = stair_decl(
            planks, fuel=True, lore_key=stair_lore
        )

    # 17 stone families (bridge, balustrade, pier, stair)
    for bridge_stem, balustrade_stem, pier_stair_stem, template in STONES:
        blocks[f"{bridge_stem}_bridge"] = bridge_block_decl(
            template, BRIDGE_BLOCK_OUTLINE
        )
        blocks[f"balustrade_{balustrade_stem}_bridge"] = bridge_block_decl(
            template, BRIDGE_BLOCK_OUTLINE
        )
        blocks[f"{pier_stair_stem}_bridge_pier"] = support_decl(template)
        stair_stem = "stone_brick" if pier_stair_stem == "stone" else pier_stair_stem
        blocks[f"{stair_stem}_bridge_stair"] = stair_decl(template)

    # Bamboo & dry bamboo
    for stem in ("bamboo", "dry_bamboo"):
        blocks[f"{stem}_bridge"] = rope_bridge_decl(
            "minecraft:bamboo_planks", BAMBOO_BRIDGE_OUTLINE, fuel=True
        )
        blocks[f"{stem}_bridge_pier"] = support_decl(
            "minecraft:bamboo_planks", fuel=True
        )
        blocks[f"{stem}_bridge_stair"] = stair_decl(
            "minecraft:bamboo_planks", fuel=True
        )

    # Asian red & glass
    blocks["asian_red_bridge"] = bridge_block_decl(
        "minecraft:mangrove_planks", LOG_BRIDGE_OUTLINE, fuel=True
    )
    blocks["asian_red_bridge_pier"] = support_decl("minecraft:oak_planks", fuel=True)
    blocks["asian_red_bridge_stair"] = stair_decl("minecraft:oak_planks", fuel=True)

    blocks["glass_bridge"] = bridge_block_decl(
        "minecraft:glass", LOG_BRIDGE_OUTLINE, fuel=False
    )
    blocks["glass_bridge_pier"] = support_decl("minecraft:glass", fuel=False)
    blocks["glass_bridge_stair"] = stair_decl("minecraft:glass", fuel=False)

    # Bridge torch & lantern
    blocks["bridge_torch"] = light_decl(TORCH_OUTLINE)
    blocks["bridge_lantern"] = light_decl(LANTERN_OUTLINE)

    assert len(blocks) == 157, f"expected 157 blocks, got {len(blocks)}"
    return blocks


CREATIVE_TAB_ITEMS = [
    "pliers",
    "bridge_torch",
    "bridge_lantern",
    "iron_bridge",
    "oak_log_bridge_middle",
    "birch_log_bridge_middle",
    "acacia_log_bridge_middle",
    "spruce_log_bridge_middle",
    "jungle_log_bridge_middle",
    "dark_oak_log_bridge_middle",
    "crimson_log_bridge_middle",
    "warped_log_bridge_middle",
    "mangrove_log_bridge_middle",
    "cherry_log_bridge_middle",
    "pale_oak_log_bridge_middle",
    "poplar_log_bridge_middle",
    "rope_oak_bridge",
    "rope_birch_bridge",
    "rope_spruce_bridge",
    "rope_jungle_bridge",
    "rope_acacia_bridge",
    "rope_dark_oak_bridge",
    "rope_crimson_bridge",
    "rope_warped_bridge",
    "rope_mangrove_bridge",
    "rope_cherry_bridge",
    "rope_pale_oak_bridge",
    "rope_poplar_bridge",
    "brick_bridge",
    "sandstone_bridge",
    "stone_brick_bridge",
    "orange_sandstone_bridge",
    "blackstone_bridge",
    "mossy_stone_brick_bridge",
    "deepslate_brick_bridge",
    "deepslate_tile_bridge",
    "mud_brick_bridge",
    "cobblestone_bridge",
    "mossy_cobblestone_bridge",
    "andesite_bridge",
    "granite_bridge",
    "diorite_bridge",
    "prismarine_bricks_bridge",
    "nether_bricks_bridge",
    "end_stone_bricks_bridge",
    "oak_rail_bridge",
    "spruce_rail_bridge",
    "birch_rail_bridge",
    "jungle_rail_bridge",
    "acacia_rail_bridge",
    "dark_oak_rail_bridge",
    "crimson_rail_bridge",
    "warped_rail_bridge",
    "mangrove_rail_bridge",
    "cherry_rail_bridge",
    "pale_oak_rail_bridge",
    "poplar_rail_bridge",
    "bamboo_bridge",
    "dry_bamboo_bridge",
    "asian_red_bridge",
    "glass_bridge",
    "balustrade_stone_bricks_bridge",
    "balustrade_sandstone_bridge",
    "balustrade_bricks_bridge",
    "balustrade_orange_sandstone_bridge",
    "balustrade_blackstone_bridge",
    "balustrade_mossy_stone_bricks_bridge",
    "balustrade_deepslate_bricks_bridge",
    "balustrade_deepslate_tiles_bridge",
    "balustrade_mud_bricks_bridge",
    "balustrade_cobblestone_bridge",
    "balustrade_mossy_cobblestone_bridge",
    "balustrade_andesite_bridge",
    "balustrade_granite_bridge",
    "balustrade_diorite_bridge",
    "balustrade_prismarine_bricks_bridge",
    "balustrade_nether_bricks_bridge",
    "balustrade_end_stone_bricks_bridge",
    "iron_bridge_pier",
    "oak_bridge_pier",
    "spruce_bridge_pier",
    "birch_bridge_pier",
    "jungle_bridge_pier",
    "acacia_bridge_pier",
    "dark_oak_bridge_pier",
    "crimson_bridge_pier",
    "warped_bridge_pier",
    "mangrove_bridge_pier",
    "cherry_bridge_pier",
    "pale_oak_bridge_pier",
    "poplar_bridge_pier",
    "asian_red_bridge_pier",
    "glass_bridge_pier",
    "brick_bridge_pier",
    "sandstone_bridge_pier",
    "stone_bridge_pier",
    "red_sandstone_bridge_pier",
    "blackstone_bridge_pier",
    "mossy_stone_bridge_pier",
    "deepslate_brick_bridge_pier",
    "deepslate_tile_bridge_pier",
    "mud_brick_bridge_pier",
    "bamboo_bridge_pier",
    "dry_bamboo_bridge_pier",
    "cobblestone_bridge_pier",
    "mossy_cobblestone_bridge_pier",
    "andesite_bridge_pier",
    "granite_bridge_pier",
    "diorite_bridge_pier",
    "prismarine_bricks_bridge_pier",
    "nether_bricks_bridge_pier",
    "end_stone_bricks_bridge_pier",
    "iron_bridge_stair",
    "oak_log_bridge_stair",
    "spruce_log_bridge_stair",
    "birch_log_bridge_stair",
    "jungle_log_bridge_stair",
    "acacia_log_bridge_stair",
    "dark_oak_log_bridge_stair",
    "crimson_log_bridge_stair",
    "warped_log_bridge_stair",
    "mangrove_log_bridge_stair",
    "cherry_log_bridge_stair",
    "pale_oak_log_bridge_stair",
    "poplar_log_bridge_stair",
    "oak_rope_bridge_stair",
    "spruce_rope_bridge_stair",
    "birch_rope_bridge_stair",
    "jungle_rope_bridge_stair",
    "acacia_rope_bridge_stair",
    "dark_oak_rope_bridge_stair",
    "crimson_rope_bridge_stair",
    "warped_rope_bridge_stair",
    "mangrove_rope_bridge_stair",
    "cherry_rope_bridge_stair",
    "pale_oak_rope_bridge_stair",
    "poplar_rope_bridge_stair",
    "stone_brick_bridge_stair",
    "sandstone_bridge_stair",
    "asian_red_bridge_stair",
    "glass_bridge_stair",
    "brick_bridge_stair",
    "red_sandstone_bridge_stair",
    "blackstone_bridge_stair",
    "mossy_stone_bridge_stair",
    "deepslate_brick_bridge_stair",
    "deepslate_tile_bridge_stair",
    "mud_brick_bridge_stair",
    "bamboo_bridge_stair",
    "dry_bamboo_bridge_stair",
    "cobblestone_bridge_stair",
    "mossy_cobblestone_bridge_stair",
    "andesite_bridge_stair",
    "granite_bridge_stair",
    "diorite_bridge_stair",
    "prismarine_bricks_bridge_stair",
    "nether_bricks_bridge_stair",
    "end_stone_bricks_bridge_stair",
]


def write_json(path: Path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def main():
    blocks = build_blocks()
    block_dir = SUNRISE_DIR / "block"
    item_dir = SUNRISE_DIR / "item"
    tab_dir = SUNRISE_DIR / "creative_tab"

    # Clean stale generated files if any
    for d in (block_dir, item_dir, tab_dir):
        if d.exists():
            for f in d.glob("*.json"):
                f.unlink()

    for bid, decl in sorted(blocks.items()):
        write_json(block_dir / f"{bid}.json", decl)

    # Pliers standalone item: plain non-fuel item with tooltip lore
    write_json(
        item_dir / "pliers.json",
        {
            "template": "minecraft:iron_ingot",
            "components": {
                "minecraft:lore": lore("mcwbridges.pliers.desc"),
            },
        },
    )

    assert len(CREATIVE_TAB_ITEMS) == 158
    assert set(CREATIVE_TAB_ITEMS) == set(blocks.keys()) | {"pliers"}
    write_json(
        tab_dir / "mcwbridges.json",
        {
            "title": "itemGroup.mcwbridges",
            "icon": "mcwbridges:iron_bridge",
            "items": [f"mcwbridges:{name}" for name in CREATIVE_TAB_ITEMS],
        },
    )
    print(f"Generated {len(blocks)} blocks, 1 item, 1 creative tab ({len(CREATIVE_TAB_ITEMS)} entries)")


if __name__ == "__main__":
    main()
