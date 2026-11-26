//! Secure Crops for Sunrise: three game rules, all off by default as in Secure Crops 2.0.1
//! (zekk051), decide what happens when a landing would trample farmland.
//!
//! - `securecrops:secure_farmland_and_crops`: with a crop on it, the farmland and the crop stay.
//! - `securecrops:secure_farmland_break_crops`: with a crop on it, the farmland stays and the
//!   crop breaks, dropping its items, with the trampling entity as the breaker.
//! - `securecrops:secure_farmland_if_empty`: with nothing on it, the farmland stays.
//!
//! "A crop" is what vanilla's `FarmlandBlock.shouldMaintainFarmland` reads: the block above is in
//! `#minecraft:maintains_farmland`. `farmland_trample` fires after vanilla's chance and checks,
//! where the original's mixin cancels `turnToBaseBlock`; fall damage happens either way.
use std::sync::OnceLock;

use mod_api::{
    BoolGameRuleKey, FarmlandTrampleEvent, Flow, Hooks, Host, LevelRef, Registrar,
    RegistrationContext, ServerHost, TagKey,
};

const CATEGORY: &str = "secure_crops";
const MAINTAINS_FARMLAND: &str = "minecraft:maintains_farmland";

struct Rules {
    and_crops: BoolGameRuleKey,
    break_crops: BoolGameRuleKey,
    if_empty: BoolGameRuleKey,
    maintains_farmland: TagKey,
}

static RULES: OnceLock<Rules> = OnceLock::new();

/// What a trample does under the rules, in the original's order of tests.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// Vanilla's trample.
    Trample,
    /// The farmland (and any crop) stays.
    Keep,
    /// The farmland stays and the crop above breaks.
    BreakCrop,
}

fn outcome(has_crop: bool, and_crops: bool, break_crops: bool, if_empty: bool) -> Outcome {
    if has_crop && and_crops {
        Outcome::Keep
    } else if has_crop && break_crops {
        Outcome::BreakCrop
    } else if !has_crop && if_empty {
        Outcome::Keep
    } else {
        Outcome::Trample
    }
}

fn rule(host: &dyn Host, level: LevelRef, key: BoolGameRuleKey) -> bool {
    level.game_rule_bool(host, key).unwrap_or(false)
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        let declared = (|| {
            r.game_rule_category(CATEGORY)?;
            let category = format!("securecrops:{CATEGORY}");
            Ok::<_, mod_api::GameRuleError>(Rules {
                and_crops: r.game_rule_bool("secure_farmland_and_crops", &category, false)?,
                break_crops: r.game_rule_bool("secure_farmland_break_crops", &category, false)?,
                if_empty: r.game_rule_bool("secure_farmland_if_empty", &category, false)?,
                maintains_farmland: r.tag_key(MAINTAINS_FARMLAND),
            })
        })();
        if let Ok(rules) = declared {
            let _ = RULES.set(rules);
        }
    }

    fn farmland_trample(
        &self,
        host: &mut dyn ServerHost,
        event: &mut FarmlandTrampleEvent,
    ) -> Flow {
        let Some(rules) = RULES.get() else {
            return Flow::Continue;
        };
        let Some(level) = event.entity.entity().level(&*host) else {
            return Flow::Continue;
        };
        let above = [event.pos.x, event.pos.y + 1, event.pos.z];
        let has_crop = level.block_state(&*host, above).is_some_and(|state| {
            host.tags()
                .block_in_tag(state.block(), rules.maintains_farmland)
                == Some(true)
        });
        match outcome(
            has_crop,
            rule(&*host, level, rules.and_crops),
            rule(&*host, level, rules.break_crops),
            rule(&*host, level, rules.if_empty),
        ) {
            Outcome::Trample => Flow::Continue,
            Outcome::Keep => Flow::Cancel,
            // A cancelling listener's writes now commit, so the crop breaks (and drops) while the
            // trample is cancelled, as the original's `breakBlock(pos.up(), true, entity)`.
            Outcome::BreakCrop => {
                let _ = level.destroy_block(
                    host,
                    above,
                    true,
                    None,
                    Some(event.entity.entity()),
                    false,
                );
                Flow::Cancel
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Outcome, outcome};

    #[test]
    fn rules_apply_in_the_original_order() {
        // All off: vanilla.
        assert_eq!(outcome(true, false, false, false), Outcome::Trample);
        assert_eq!(outcome(false, false, false, false), Outcome::Trample);
        // A crop: "and crops" wins over "break crops"; "if empty" doesn't apply.
        assert_eq!(outcome(true, true, true, true), Outcome::Keep);
        assert_eq!(outcome(true, false, true, false), Outcome::BreakCrop);
        assert_eq!(outcome(true, false, false, true), Outcome::Trample);
        // No crop: only "if empty" applies.
        assert_eq!(outcome(false, true, true, false), Outcome::Trample);
        assert_eq!(outcome(false, false, false, true), Outcome::Keep);
    }
}
