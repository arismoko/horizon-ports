//! Obsidian Equipment Reworked's code: the chest loot it adds (ModLootTableModifiers), where its
//! items sit in the vanilla creative tabs (ModItems.registerModItems), and the obsidian upgrade
//! template's own tooltip text (ObsidianUpgradeTemplate). The items themselves are declarations
//! in `data/obsidianequipmentrework/sunrise/item/`.
use mod_api::serde_json::{Value, json};
use mod_api::text::TextContent;
use mod_api::{
    BuildCreativeModeTabContentsEvent, ClientHost, CreativeTabEditError, CreativeTabPlacement,
    CreativeTabVisibility, DataLoadContext, Hooks, Host, HostSide, Identifier, ItemStack,
    ItemTooltipEvent, LootTableLoadEvent, Text,
};

const NS: &str = "obsidianequipmentrework";
const TEMPLATE: &str = "obsidianequipmentrework:obsidian_upgrade_smithing_template";

/// The pools ModLootTableModifiers adds to a table, in the order it adds them: the horse armour
/// (0.18) to OBSIDIAN_HORSE_ARMOR_LOOT_TABLES, the template to OBSIDIAN_UPGRADE_TEMPLATE_LOOT_TABLES
/// at its chance, and the nautilus armour (0.004) to OBSIDIAN_NAUTILUS_ARMOR_LOOT_TABLES.
fn extra_pools(table: &str) -> Vec<(&'static str, f32)> {
    let mut pools = Vec::new();
    if matches!(
        table,
        "minecraft:chests/nether_bridge"
            | "minecraft:chests/bastion_treasure"
            | "minecraft:chests/ruined_portal"
    ) {
        pools.push(("obsidian_horse_armor", 0.18));
    }
    let template = match table {
        "minecraft:chests/bastion_treasure" => Some(0.20),
        "minecraft:chests/bastion_other"
        | "minecraft:chests/bastion_bridge"
        | "minecraft:chests/stronghold_corridor" => Some(0.13),
        "minecraft:chests/nether_bridge" | "minecraft:chests/ancient_city" => Some(0.15),
        "minecraft:chests/abandoned_mineshaft" => Some(0.11),
        "minecraft:chests/ruined_portal" => Some(0.10),
        _ => None,
    };
    if let Some(chance) = template {
        pools.push(("obsidian_upgrade_smithing_template", chance));
    }
    if matches!(
        table,
        "minecraft:chests/shipwreck_supply"
            | "minecraft:chests/underwater_ruin_small"
            | "minecraft:chests/underwater_ruin_big"
            | "minecraft:chests/buried_treasure"
    ) {
        pools.push(("obsidian_nautilus_armor", 0.004));
    }
    pools
}

/// `LootPool.builder().rolls(1).conditionally(RandomChance(chance)).with(ItemEntry(item))`, in
/// 26.3's form: a condition names its kind under `type`.
fn pool(item: &str, chance: f32) -> Value {
    json!({
        "rolls": 1,
        "entries": [{ "type": "minecraft:item", "name": format!("{NS}:{item}") }],
        "conditions": [{ "type": "minecraft:random_chance", "chance": chance }],
    })
}

/// ItemGroupEvents.modifyEntriesEvent(..).addAfter(anchor, items...): each tab, and the items put
/// after a vanilla item in it, in order.
fn tab_entries(tab: &str) -> &'static [(&'static str, &'static [&'static str])] {
    match tab {
        "minecraft:tools_and_utilities" => &[(
            "diamond_hoe",
            &[
                "obsidian_shovel",
                "obsidian_pickaxe",
                "obsidian_axe",
                "obsidian_hoe",
            ],
        )],
        "minecraft:ingredients" => &[(
            "netherite_upgrade_smithing_template",
            &["obsidian_upgrade_smithing_template"],
        )],
        "minecraft:combat" => &[
            ("diamond_sword", &["obsidian_sword"]),
            ("diamond_axe", &["obsidian_axe"]),
            ("diamond_spear", &["obsidian_spear"]),
            (
                "diamond_boots",
                &[
                    "obsidian_helmet",
                    "obsidian_chestplate",
                    "obsidian_leggings",
                    "obsidian_boots",
                ],
            ),
            ("diamond_horse_armor", &["obsidian_horse_armor"]),
            ("diamond_nautilus_armor", &["obsidian_nautilus_armor"]),
        ],
        _ => &[],
    }
}

fn stack(host: &dyn Host, id: &str) -> Option<ItemStack> {
    Some(ItemStack::of(host.item_key(&Identifier::parse(id).ok()?)?))
}

/// The template's tooltip copies netherite's lines (it is declared on the netherite template);
/// ObsidianUpgradeTemplate's own "applies to" and "ingredients" texts take their place.
const TOOLTIP_KEYS: [(&str, &str); 2] = [
    (
        "item.minecraft.smithing_template.netherite_upgrade.applies_to",
        "item.obsidianequipmentrework.smithing_template.obsidian_upgrade.applies_to",
    ),
    (
        "item.minecraft.smithing_template.netherite_upgrade.ingredients",
        "item.obsidianequipmentrework.smithing_template.obsidian_upgrade.ingredients",
    ),
];

/// Renames the netherite translation keys anywhere in `text`; true when one was renamed.
fn rekey(text: &mut Text) -> bool {
    let mut changed = false;
    if let TextContent::Translatable { key, .. } = &mut text.content
        && let Some((_, ours)) = TOOLTIP_KEYS.iter().find(|(netherite, _)| key == netherite)
    {
        *key = (*ours).to_owned();
        changed = true;
    }
    for child in &mut text.children {
        changed |= rekey(child);
    }
    changed
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn loot_table_load(&self, _context: &dyn DataLoadContext, event: &mut LootTableLoadEvent<'_>) {
        let pools = extra_pools(event.id);
        if pools.is_empty() {
            return;
        }
        let Some(table) = event.table.as_object_mut() else {
            return;
        };
        if let Some(list) = table
            .entry("pools")
            .or_insert_with(|| json!([]))
            .as_array_mut()
        {
            list.extend(pools.into_iter().map(|(item, chance)| pool(item, chance)));
        }
    }

    fn build_creative_mode_tab_contents(
        &self,
        mut host: HostSide,
        event: &mut BuildCreativeModeTabContentsEvent<'_>,
    ) {
        let host: &dyn Host = match &mut host {
            HostSide::Server(host) => &**host,
            HostSide::Client(host) => &**host,
        };
        for (anchor, items) in tab_entries(event.tab) {
            let mut after = stack(host, &format!("minecraft:{anchor}"));
            for item in *items {
                let Some(ours) = stack(host, &format!("{NS}:{item}")) else {
                    continue;
                };
                let insert = |placement| {
                    event
                        .output
                        .insert(placement, ours.clone(), CreativeTabVisibility::Both)
                };
                // Fabric's addAfter appends to the end when the tab lacks the anchor.
                let placement = match &after {
                    Some(anchor) => CreativeTabPlacement::After(anchor.clone()),
                    None => CreativeTabPlacement::Last,
                };
                let placed = match insert(placement) {
                    Err(CreativeTabEditError::MissingAnchor(_)) => {
                        insert(CreativeTabPlacement::Last)
                    }
                    other => other,
                };
                if placed.is_ok() {
                    after = Some(ours);
                }
            }
        }
    }

    fn item_tooltip(&self, host: &dyn ClientHost, event: &mut ItemTooltipEvent<'_>) {
        if Identifier::parse(TEMPLATE)
            .ok()
            .and_then(|id| host.item_key(&id))
            != Some(event.stack.item())
        {
            return;
        }
        for index in 0..event.lines.len() {
            let Some(Ok((mut text, color))) = event.lines.get(index) else {
                continue;
            };
            if rekey(&mut text) {
                let _ = event.lines.set(index, text, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chest_pools_follow_the_original_tables() {
        assert_eq!(
            extra_pools("minecraft:chests/nether_bridge"),
            vec![
                ("obsidian_horse_armor", 0.18),
                ("obsidian_upgrade_smithing_template", 0.15)
            ]
        );
        assert_eq!(
            extra_pools("minecraft:chests/buried_treasure"),
            vec![("obsidian_nautilus_armor", 0.004)]
        );
        assert!(extra_pools("minecraft:chests/simple_dungeon").is_empty());
    }

    #[test]
    fn pool_holds_its_chance_as_a_pool_condition() {
        let pool = pool("obsidian_horse_armor", 0.18);
        assert_eq!(
            pool["entries"][0]["name"],
            "obsidianequipmentrework:obsidian_horse_armor"
        );
        assert_eq!(pool["conditions"][0]["type"], "minecraft:random_chance");
        assert_eq!(pool["rolls"], 1);
    }

    #[test]
    fn rekey_renames_the_netherite_lines_only() {
        let mut applies = Text::literal(" ");
        applies.children.push(Text::translate(
            "item.minecraft.smithing_template.netherite_upgrade.applies_to",
            Vec::new(),
        ));
        assert!(rekey(&mut applies));
        assert!(matches!(
            &applies.children[0].content,
            TextContent::Translatable { key, .. }
                if key == "item.obsidianequipmentrework.smithing_template.obsidian_upgrade.applies_to"
        ));
        let mut title = Text::translate("item.minecraft.smithing_template", Vec::new());
        assert!(!rekey(&mut title));
    }
}
