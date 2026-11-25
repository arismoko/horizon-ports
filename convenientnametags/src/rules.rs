//! The decisions of Convenient Name Tags, apart from the host: what a right-click on a mob does.

pub const NAME_TAG: &str = "minecraft:name_tag";
pub const SHEARS: &str = "minecraft:shears";
pub const PLAYER: &str = "minecraft:player";

/// What the click does before the mob's own interaction runs.
#[derive(Debug, PartialEq, Eq)]
pub enum Click {
    /// Not ours: vanilla goes on.
    Pass,
    /// `ShearsItemMixin.useOnEntity`: the name comes off as a tag and the shears take 1 damage.
    ShearName,
    /// `NameTagItemMixin.useOnEntity`: the tag names the mob already, so nothing is used up.
    KeepTag,
    /// A different name: the old name drops as a tag and vanilla renames the mob.
    DropOldName,
}

/// Classifies a click on a mob of `kind` with `held` in hand. Names compare whole (text and
/// style), as the original compares components.
pub fn classify<N: PartialEq>(
    kind: &str,
    held: &str,
    sneaking: bool,
    mob_name: Option<&N>,
    tag_name: Option<&N>,
) -> Click {
    let Some(mob_name) = mob_name else {
        return Click::Pass;
    };
    if kind == PLAYER {
        return Click::Pass;
    }
    if held == SHEARS && sneaking {
        return Click::ShearName;
    }
    match (held == NAME_TAG, tag_name) {
        (true, Some(tag)) if tag == mob_name => Click::KeepTag,
        (true, Some(_)) => Click::DropOldName,
        _ => Click::Pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COW: &str = "minecraft:cow";

    #[test]
    fn unnamed_mobs_are_left_alone() {
        assert_eq!(classify::<&str>(COW, SHEARS, true, None, None), Click::Pass);
        assert_eq!(
            classify(COW, NAME_TAG, false, None, Some(&"Bessie")),
            Click::Pass
        );
    }

    #[test]
    fn sneaking_with_shears_takes_the_name_off() {
        assert_eq!(
            classify::<&str>(COW, SHEARS, true, Some(&"Bessie"), None),
            Click::ShearName
        );
        assert_eq!(
            classify::<&str>(COW, SHEARS, false, Some(&"Bessie"), None),
            Click::Pass
        );
    }

    #[test]
    fn the_same_name_keeps_the_tag() {
        assert_eq!(
            classify(COW, NAME_TAG, false, Some(&"Bessie"), Some(&"Bessie")),
            Click::KeepTag
        );
    }

    #[test]
    fn another_name_drops_the_old_one() {
        assert_eq!(
            classify(COW, NAME_TAG, false, Some(&"Bessie"), Some(&"Daisy")),
            Click::DropOldName
        );
    }

    #[test]
    fn a_blank_tag_is_vanillas() {
        assert_eq!(
            classify::<&str>(COW, NAME_TAG, false, Some(&"Bessie"), None),
            Click::Pass
        );
    }

    #[test]
    fn players_are_never_touched() {
        assert_eq!(
            classify(PLAYER, SHEARS, true, Some(&"Steve"), None),
            Click::Pass
        );
    }

    #[test]
    fn other_items_pass() {
        assert_eq!(
            classify(
                COW,
                "minecraft:stick",
                true,
                Some(&"Bessie"),
                Some(&"Bessie")
            ),
            Click::Pass
        );
    }
}
