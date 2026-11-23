//! The everlasting upgrade (EverlastingUpgradeItem): a backpack that carries it survives where it falls. Its item
//! entity can't be hurt (fire, explosions, cactus) and never despawns, and the placed backpack has bedrock's
//! explosion resistance and can't be destroyed by an entity. One per storage.
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use mod_api::{Compound, EntityRef, ItemStack, ParticleKey, ParticleOptions, ServerHost, Tag};

use crate::contents::{self, Nbt, UPGRADES};

/// What `Block.BEDROCK` resists (`BEDROCK_RESISTANCE`, 3600000).
pub const BEDROCK_RESISTANCE: f32 = 3_600_000.0;

/// Ticks of life an expiring item is given again (the hook caps one extension at 32766).
pub const EXTRA_LIFE: i32 = 32766;

/// Marks a dropped everlasting backpack's item entity.
pub const TAG: &str = "sophisticatedcore:everlasting";

/// Set once a dropped backpack has floated up; from then on it hangs in the air.
const FLOATED_TAG: &str = "sophisticatedcore:everlasting_floated";

/// What the bubble column's updraft adds each tick (`onInsideBubbleColumn(false)`), and its ceiling.
const RISE: f64 = 0.1;
const RISE_MAX: f64 = 1.8;

/// `ParticleTypes.HAPPY_VILLAGER`, requested once when the mod registers.
pub static SPARKLE: OnceLock<ParticleKey> = OnceLock::new();

/// A cheap source of noise for the sparkle's one-in-twenty chance and its offsets (0 to 1).
fn noise() -> f64 {
    static STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let mut x = STATE.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((x ^ (x >> 31)) >> 11) as f64 / (1u64 << 53) as f64
}

/// EverlastingBackpackItemEntity.tick: one tick in twenty a happy-villager sparkle near the item.
fn sparkle(host: &mut dyn ServerHost, entity: EntityRef) {
    let Some(key) = SPARKLE.get() else {
        return;
    };
    if noise() >= 0.05 {
        return;
    }
    let (Some(level), Some([x, y, z])) = (entity.level(&*host), entity.position(&*host)) else {
        return;
    };
    let at = [x + 0.5 - noise(), y + noise() * 0.5, z + 0.5 - noise()];
    level.send_particles(
        host,
        &ParticleOptions::simple(*key),
        None,
        at,
        [0.0, 0.0, 0.1],
        0.0,
        0,
    );
}

/// EverlastingBackpackItemEntity.tick: in water or lava, or at the world's floor (`isInWater` also holds one block
/// above `getMinBuildHeight`, so the void never takes it), the item rises; once it is out it stops and hangs without
/// gravity.
pub fn float(host: &mut dyn ServerHost, entity: EntityRef) {
    let Some(tags) = entity.tags(&*host) else {
        return;
    };
    if !tags.contains(TAG) {
        return;
    }
    let floated = tags.contains(FLOATED_TAG);
    sparkle(host, entity);
    let floor = entity
        .level(&*host)
        .and_then(|l| l.height(&*host))
        .zip(entity.position(&*host))
        .is_some_and(|((min_y, _), p)| p[1] < f64::from(min_y) + 1.0);
    let wet = floor
        || entity.is_in_water(&*host) == Some(true)
        || entity.is_in_lava(&*host) == Some(true);
    let Some([vx, vy, vz]) = entity.velocity(&*host) else {
        return;
    };
    if wet {
        entity.set_velocity(host, [vx, (vy + RISE).min(RISE_MAX), vz]);
        entity.add_tag(host, FLOATED_TAG);
    } else if floated {
        entity.set_no_gravity(host, true);
        entity.set_velocity(host, [0.0; 3]);
        entity.remove_tag(host, FLOATED_TAG);
    }
}

/// Whether the storage's custom data holds an everlasting upgrade in its upgrade slots.
pub fn in_data(data: &Compound) -> bool {
    let Some(Tag::Compound(inv)) = data.at(UPGRADES) else {
        return false;
    };
    let Some(Tag::List(items)) = inv.at("Items") else {
        return false;
    };
    items.iter().any(|t| match t {
        Tag::Compound(e) => {
            contents::get_str(e, "id").is_some_and(|id| id.ends_with(":everlasting_upgrade"))
        }
        _ => false,
    })
}

/// Whether a storage item carries the upgrade.
pub fn has(stack: &ItemStack) -> bool {
    in_data(&contents::custom_data(stack))
}

/// Whether a placed storage's saved block entity data (`{Items: [{Slot: 0, components, ..}]}`) holds a backpack that
/// carries it.
pub fn in_block_entity(data: &Compound) -> bool {
    let Some(Tag::List(items)) = data.at("Items") else {
        return false;
    };
    items.iter().any(|t| match t {
        Tag::Compound(e) => match e.at("components") {
            Some(Tag::Compound(c)) => matches!(
                c.at("minecraft:custom_data"),
                Some(Tag::Compound(d)) if in_data(d)
            ),
            _ => false,
        },
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upgrade_data(ids: &[&str]) -> Compound {
        let items = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                Tag::compound([
                    ("Slot", Tag::Int(i as i32)),
                    ("id", Tag::String((*id).into())),
                ])
            })
            .collect();
        Compound::from_iter([(
            UPGRADES.into(),
            Tag::compound([("Items", Tag::List(items))]),
        )])
    }

    #[test]
    fn finds_the_upgrade_among_others() {
        assert!(in_data(&upgrade_data(&[
            "sophisticatedcore:pickup_upgrade",
            "sophisticatedcore:everlasting_upgrade"
        ])));
        assert!(!in_data(&upgrade_data(&[
            "sophisticatedcore:pickup_upgrade"
        ])));
        assert!(!in_data(&Compound::new()));
    }

    #[test]
    fn finds_it_in_a_block_entity() {
        let stack = Tag::compound([(
            "components",
            Tag::compound([(
                "minecraft:custom_data",
                Tag::Compound(upgrade_data(&["sophisticatedcore:everlasting_upgrade"])),
            )]),
        )]);
        let data = Compound::from_iter([("Items".into(), Tag::List(vec![stack]))]);
        assert!(in_block_entity(&data));
        assert!(!in_block_entity(&Compound::new()));
    }
}
