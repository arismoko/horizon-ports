# Farmer's Delight for Horizon

Farmer's Delight: stove, cooking pot, skillet, cutting board, crops, rich soil and 170 foods.

A port of [Farmer's Delight](https://github.com/MehVahdJukaar/FarmersDelight) 1.20.1-2.4.1+refabricated by vectorwing, MehVahdJukaar, MerchantPug to [Horizon](https://github.com/horizon-rust/horizon) 26.3, under the original's licence (MIT, in `LICENSE`; see `NOTICE`).

This folder is the package source: one native mod with its content (`horizon-mod.toml`, `src/`, `data/`, `assets/`). The rules are written on Horizon's mod API only (no game module, no mixins). Release [`farmersdelight-v0.5.0`](https://github.com/lavoide/horizon-ports/releases/tag/farmersdelight-v0.5.0) holds `farmersdelight.hmod`, built from this folder with Horizon's exact-build SDK `dd67b76ff23e28c6d8e21859df600d31572f81a0` (Horizon Mixins `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`).

Build: `cargo horizon-mod build` (cargo-horizon-mod at dd67b76), or `horizon-launcher native-build <folder>` against Nightly dd67b76.

Earlier versions (0.3.1 native + 2.4.1-horizon.3 content, two packages on Horizon `fdc87ae`) are in this folder's history.
