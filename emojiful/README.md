# Emojiful for Horizon

`:short_name:` in chat shows the emoji (Twemoji).

A port of [Emojiful](https://github.com/InnovativeOnlineIndustries/Emojiful) 4.2.0 by HorizonStudio (Innovative Online Industries) to [Horizon](https://github.com/horizon-rust/horizon) 26.3, under the original's licence (MIT, in `LICENSE`). The emoji images are [Twemoji](https://github.com/jdecked/twemoji) (CC-BY 4.0, in `LICENSE-GRAPHICS`); see `NOTICE`.

This folder is the package source: one native mod with its emoji font (`horizon-mod.toml`, `src/`, `assets/`), on Horizon's mod API only. Release [`emojiful-v0.4.0`](https://github.com/lavoide/horizon-ports/releases/tag/emojiful-v0.4.0) holds `emojiful.hmod`, built from this folder with Horizon's exact-build SDK `dd67b76ff23e28c6d8e21859df600d31572f81a0` (Horizon Mixins `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`).

Earlier versions (a native `.hnmod` plus the `emojiful_twemoji` resource pack, on Horizon `fdc87ae`) are in this folder's history.
