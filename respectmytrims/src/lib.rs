//! Respect My Trims for Sunrise: armour with a gold trim, or the snout pattern, counts as gold for
//! piglins, as Respect My Trims 1.4.0 (N, tnoctua) does in `PiglinAi.isWearingSafeArmor`.
use mod_api::{
    Hooks, Host, ItemPart, ItemStack, JavaString, MakesPiglinsNeutralEvent, Registrar,
    RegistrationContext, Tag,
};

/// The trim material and pattern that calm piglins (`TrimMaterials.GOLD`, `TrimPatterns.SNOUT`).
const GOLD: &str = "minecraft:gold";
const SNOUT: &str = "minecraft:snout";

/// Whether a `minecraft:trim` component names the gold material or the snout pattern. A material
/// or pattern written inline is no registry entry, so it is neither, as `Holder.is` answers.
pub fn calms_piglins(trim: &Tag) -> bool {
    let Tag::Compound(trim) = trim else {
        return false;
    };
    let named = |key: &str, id: &str| matches!(trim.get(&JavaString::from(key)), Some(Tag::String(s)) if s.to_string_lossy() == id);
    named("material", GOLD) || named("pattern", SNOUT)
}

fn trimmed_gold(stack: &ItemStack) -> bool {
    stack
        .components()
        .and_then(|c| c.as_nbt().get(&JavaString::from("minecraft:trim")))
        .is_some_and(calms_piglins)
}

struct GoldTrim;

#[mod_api::sunrise_part]
impl ItemPart for GoldTrim {
    fn makes_piglins_neutral(&self, _host: &dyn Host, event: &mut MakesPiglinsNeutralEvent<'_>) {
        if trimmed_gold(event.stack) {
            event.neutral = true;
        }
    }
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        r.item_part_tag("minecraft:trimmable_armor", GoldTrim);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mod_api::Compound;

    fn trim(material: Tag, pattern: Tag) -> Tag {
        let mut c = Compound::new();
        c.insert("material".into(), material);
        c.insert("pattern".into(), pattern);
        Tag::Compound(c)
    }

    #[test]
    fn gold_material_or_snout_pattern() {
        let s = |v: &str| Tag::String(v.into());
        assert!(calms_piglins(&trim(
            s("minecraft:gold"),
            s("minecraft:sentry")
        )));
        assert!(calms_piglins(&trim(
            s("minecraft:iron"),
            s("minecraft:snout")
        )));
        assert!(!calms_piglins(&trim(
            s("minecraft:iron"),
            s("minecraft:sentry")
        )));
        assert!(!calms_piglins(&trim(
            Tag::Compound(Compound::new()),
            s("minecraft:sentry")
        )));
    }
}
