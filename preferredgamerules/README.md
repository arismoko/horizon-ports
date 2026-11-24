# Preferred Gamerules for Sunrise

New worlds start with the game rules you prefer.

A port of [Preferred Gamerules](https://github.com/Estecka/mc-Preferred-Gamerules) by Estecka to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API. The 26.3 carry is by lavoide; this port moves it to Sunrise.

## How to use it

Write your preferences in `config/preferredgamerules.properties` in the game folder (the file is created empty the first time a world is created with the mod). Each line names a game rule with the prefix `gamerule.` and gives its value:

```properties
gamerule.minecraft:fire_damage=false
gamerule.minecraft:keep_inventory=true
```

Every world created afterwards starts with those values; worlds that already exist keep their own rules. Rules from other mods work the same way. An unknown rule is skipped; a value the rule refuses is reported in the log and the rule keeps its usual value; a line with no value makes the file unreadable, so none of it applies, as in the original.

## Differences from the original

- The file is `config/preferredgamerules.properties`, after this port's mod id, where the original's is `config/preferred-gamerules.properties`.
- The preferred values are set on a new world as it first opens, not as the rules' defaults: the create-world screen's Game Rules page doesn't show them, and a value picked there for a rule in the file is replaced. Sunrise lets no mod change a rule's default yet ([sunrise-org/sunrise#5306](https://github.com/sunrise-org/sunrise/issues/5306)).
- The original's in-game editor needs Mod Menu, a Fabric mod; preferences are edited in the file only.

## Building

With the Sunrise mod tool (`cargo sunrise-mod`, from the Sunrise SDK) and Rust:

```sh
cargo sunrise-mod check
cargo sunrise-mod build    # target/sunrise-mod/preferredgamerules.smod
```

`sunrise-mod.toml` pins the Sunrise build this version is made for.

## Licence and credits

Preferred Gamerules is by Estecka, under the MIT licence; `LICENSE` holds its text. This port contains no code from the original and is distributed under the same licence.
