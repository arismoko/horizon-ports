# Just Another Rotten Flesh to Leather

A content-only Sunrise port of [Just Another Rotten Flesh to Leather](https://gitlab.com/456Xander/JRFTL) by Alexander Daum (456Xander), based on Fabric 1.10.0 at `9b68bacbcd5ea7bdce0b3d2ab9b58f6f3fc94a72`. Ported by arismoko for Sunrise 26.3 Dev 2 (`ba1f24e4ad41246b558230aa665539dc768b1287`).

With standard coal, rotten flesh cooks in 200 ticks in a furnace or 100 ticks in a smoker. Each recipe turns one rotten flesh into one leather and awards 0.2 XP per item. Picking up rotten flesh unlocks both recipes in the recipe book.

Minecraft 26.3 stores the base cooking time in `cookingtime` and divides it by the fuel speed multiplier. The smoker recipe specifies 200 base ticks, so coal at 2× preserves Fabric 1.10.0’s 100 elapsed ticks. In the older format, the recipe specified the 100 ticks directly.

The upstream Fabric release carries leftover references to Prepared Flesh and hard-mode recipes, but does not register the item or ship those recipes. This port drops those inactive references and keeps both working recipes. It includes no upstream images or Java code.

## Licences

[`LICENSE`](LICENSE) preserves the upstream project's MIT licence, which is also declared in the Fabric mod metadata. [`LICENSE-FABRIC`](LICENSE-FABRIC) preserves the separate CC0 1.0 notice from the upstream Fabric directory.
