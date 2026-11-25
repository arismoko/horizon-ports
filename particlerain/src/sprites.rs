//! Procedural and resource-pack weather sprite generation for `register_sprite_sources`.
//!
//! Splits vanilla or resource-pack `textures/environment/rain.png` and `snow.png` into
//! `particlerain:particle/rain_0..15`, `heavy_rain_0..3`, `snow_0..15`, `heavy_snow_0..3`,
//! applies alpha boost and optional water-tint desaturation, and generates Bresenham
//! circle sprites `particlerain:particle/ripple_0..7`.

use crate::png::{RgbaImage, decode_png};
use mod_api::{Identifier, ResourceReader, SpriteSourceOutput};

/// Build all generated weather sprites for the `minecraft:particles` atlas.
pub fn generate_weather_sprites(
    resources: &dyn ResourceReader,
    water_tint: bool,
    ripple_res: u32,
    ripple_use_pack_res: bool,
) -> Vec<SpriteSourceOutput> {
    let mut outputs = Vec::with_capacity(48);

    // Rain sprites (split 16 + merge 4 heavy)
    let rain_img = resources
        .read("assets/minecraft/textures/environment/rain.png")
        .and_then(|bytes| decode_png(&bytes).ok());
    if let Some(mut img) = rain_img {
        if water_tint {
            desaturate(&mut img);
        }
        boost_alpha(&mut img);
        emit_split_and_merged(&img, "rain_", "heavy_rain_", 4, &mut outputs);
    } else {
        emit_fallback_rain(&mut outputs);
    }

    // Snow sprites (split 16 + merge 4 heavy)
    let snow_img = resources
        .read("assets/minecraft/textures/environment/snow.png")
        .and_then(|bytes| decode_png(&bytes).ok());
    if let Some(mut img) = snow_img {
        boost_alpha(&mut img);
        emit_split_and_merged(&img, "snow_", "heavy_snow_", 4, &mut outputs);
    } else {
        emit_fallback_snow(&mut outputs);
    }

    // Ripple sprites (8 concentric Bresenham circles)
    let resolved_ripple_res = resolve_ripple_resolution(resources, ripple_res, ripple_use_pack_res);
    for i in 0..8 {
        let ripple = generate_ripple(i, resolved_ripple_res);
        push_pixels(
            &mut outputs,
            &format!("particlerain:particle/ripple_{i}"),
            ripple,
        );
    }

    outputs
}

fn push_pixels(outputs: &mut Vec<SpriteSourceOutput>, id: &str, image: RgbaImage) {
    if let Ok(sprite) = Identifier::parse(id) {
        outputs.push(SpriteSourceOutput::Pixels {
            sprite,
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        });
    }
}

/// Desaturate an RGBA image using max-channel value (`TextureUtil.desaturate`).
pub fn desaturate(img: &mut RgbaImage) {
    for px in img.rgba.chunks_exact_mut(4) {
        let gray = px[0].max(px[1]).max(px[2]);
        px[0] = gray;
        px[1] = gray;
        px[2] = gray;
    }
}

/// Boost alpha proportionally so the maximum non-zero alpha approaches 255 (`TextureUtil.boostAlpha`).
pub fn boost_alpha(img: &mut RgbaImage) {
    let highest = img.rgba.chunks_exact(4).map(|px| px[3]).max().unwrap_or(0);
    if highest == 0 || highest == 255 {
        return;
    }
    let multiplier = 255u32 / (highest as u32);
    if multiplier <= 1 {
        return;
    }
    for px in img.rgba.chunks_exact_mut(4) {
        px[3] = ((px[3] as u32) * multiplier).min(255) as u8;
    }
}

fn emit_split_and_merged(
    sheet: &RgbaImage,
    prefix: &str,
    heavy_prefix: &str,
    columns: u32,
    outputs: &mut Vec<SpriteSourceOutput>,
) {
    let size = sheet.width / columns.max(1);
    if size == 0 {
        return;
    }
    let rows = sheet.height / size;
    let total = (rows * columns) as usize;
    if total == 0 {
        return;
    }

    for i in 0..16usize {
        let idx = i % total;
        let sprite = split_sprite(sheet, idx, columns, rows, size);
        push_pixels(
            outputs,
            &format!("particlerain:particle/{prefix}{i}"),
            sprite,
        );
    }

    for i in 0..4usize {
        let start = (i * 4) % total;
        let count = 4usize.min(total.saturating_sub(start)).max(1);
        let merged = merge_sprites(sheet, start, count, columns, rows, size);
        push_pixels(
            outputs,
            &format!("particlerain:particle/{heavy_prefix}{i}"),
            merged,
        );
    }
}

/// Extract one square sprite from a vertical column sheet (`TextureUtil.getSpriteFromSheet`).
pub fn split_sprite(
    sheet: &RgbaImage,
    index: usize,
    columns: u32,
    rows: u32,
    size: u32,
) -> RgbaImage {
    let row = (index as u32) % rows.max(1);
    let col = ((index as u32) / rows.max(1)) % columns.max(1);
    let mut out = RgbaImage {
        width: size,
        height: size,
        rgba: vec![0; (size as usize) * (size as usize) * 4],
    };
    copy_rect(
        sheet,
        &mut out,
        col * size,
        row * size,
        size,
        size,
        true,
        true,
        false,
    );
    out
}

/// Merge `count` sprites from a sheet keeping highest-alpha pixels (`TextureUtil.mergeSpritesFromSheet`).
pub fn merge_sprites(
    sheet: &RgbaImage,
    start_index: usize,
    count: usize,
    columns: u32,
    rows: u32,
    size: u32,
) -> RgbaImage {
    let mut out = RgbaImage {
        width: size,
        height: size,
        rgba: vec![0; (size as usize) * (size as usize) * 4],
    };
    for i in start_index..start_index + count {
        let row = (i as u32) % rows.max(1);
        let col = ((i as u32) / rows.max(1)) % columns.max(1);
        copy_rect(
            sheet,
            &mut out,
            col * size,
            row * size,
            size,
            size,
            true,
            true,
            true,
        );
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn copy_rect(
    src: &RgbaImage,
    dst: &mut RgbaImage,
    src_x: u32,
    src_y: u32,
    w: u32,
    h: u32,
    swap_x: bool,
    swap_y: bool,
    highest_alpha_only: bool,
) {
    for y in 0..h {
        let sy = src_y + y;
        if sy >= src.height {
            continue;
        }
        let dy = if swap_y { h - 1 - y } else { y };
        for x in 0..w {
            let sx = src_x + x;
            if sx >= src.width {
                continue;
            }
            let dx = if swap_x { w - 1 - x } else { x };
            let si = ((sy * src.width + sx) as usize) * 4;
            let di = ((dy * dst.width + dx) as usize) * 4;
            if highest_alpha_only && src.rgba[si + 3] <= dst.rgba[di + 3] {
                continue;
            }
            dst.rgba[di..di + 4].copy_from_slice(&src.rgba[si..si + 4]);
        }
    }
}

fn resolve_ripple_resolution(
    resources: &dyn ResourceReader,
    configured: u32,
    use_pack_res: bool,
) -> u32 {
    if use_pack_res
        && let Some(bytes) = resources.read("assets/minecraft/textures/particle/big_smoke_0.png")
        && let Ok(img) = decode_png(&bytes)
        && img.width >= 4
    {
        return img.width.min(256);
    }
    configured.clamp(4, 256)
}

/// Generate a white Bresenham-circle ripple sprite (`TextureUtil.generateRipple`).
pub fn generate_ripple(frame: u32, size: u32) -> RgbaImage {
    let size = size.clamp(4, 256);
    let mut img = RgbaImage {
        width: size,
        height: size,
        rgba: vec![0; (size as usize) * (size as usize) * 4],
    };
    let radius = (((size as f32) / 2.0) / 8.0) * ((frame + 1) as f32);
    let r = radius.clamp(1.0, (size as f32) / 2.0 - 1.0) as i32;
    let cx = (size / 2) as i32;
    let cy = (size / 2) as i32;

    let mut x = 0i32;
    let mut y = r;
    let mut d = 3 - 2 * r;
    draw_circle_octants(&mut img, cx, cy, x, y);
    while y >= x {
        x += 1;
        if d > 0 {
            y -= 1;
            d += 4 * (x - y) + 10;
        } else {
            d += 4 * x + 6;
        }
        draw_circle_octants(&mut img, cx, cy, x, y);
    }
    img
}

fn draw_circle_octants(img: &mut RgbaImage, cx: i32, cy: i32, x: i32, y: i32) {
    let points = [
        (cx + x, cy + y),
        (cx - x, cy + y),
        (cx + x, cy - y),
        (cx - x, cy - y),
        (cx + y, cy + x),
        (cx - y, cy + x),
        (cx + y, cy - x),
        (cx - y, cy - x),
    ];
    for (px, py) in points {
        if px >= 0 && py >= 0 && (px as u32) < img.width && (py as u32) < img.height {
            let idx = (((py as u32) * img.width + (px as u32)) as usize) * 4;
            img.rgba[idx..idx + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
    }
}

fn emit_fallback_rain(outputs: &mut Vec<SpriteSourceOutput>) {
    let size = 16u32;
    for i in 0..16u32 {
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        let col = (3 + (i * 5) % 10) as usize;
        for y in 2..14usize {
            let idx = (y * (size as usize) + col) * 4;
            rgba[idx..idx + 4].copy_from_slice(&[255, 255, 255, 220]);
        }
        push_pixels(
            outputs,
            &format!("particlerain:particle/rain_{i}"),
            RgbaImage {
                width: size,
                height: size,
                rgba,
            },
        );
    }
    for i in 0..4u32 {
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        for col in [4usize, 10usize] {
            for y in 1..15usize {
                let idx = (y * (size as usize) + col) * 4;
                rgba[idx..idx + 4].copy_from_slice(&[255, 255, 255, 240]);
            }
        }
        push_pixels(
            outputs,
            &format!("particlerain:particle/heavy_rain_{i}"),
            RgbaImage {
                width: size,
                height: size,
                rgba,
            },
        );
    }
}

fn emit_fallback_snow(outputs: &mut Vec<SpriteSourceOutput>) {
    let size = 16u32;
    for i in 0..16u32 {
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        let cx = (4 + (i * 3) % 8) as usize;
        let cy = (4 + (i * 5) % 8) as usize;
        for dy in 0..2usize {
            for dx in 0..2usize {
                let idx = ((cy + dy) * (size as usize) + (cx + dx)) * 4;
                rgba[idx..idx + 4].copy_from_slice(&[255, 255, 255, 235]);
            }
        }
        push_pixels(
            outputs,
            &format!("particlerain:particle/snow_{i}"),
            RgbaImage {
                width: size,
                height: size,
                rgba,
            },
        );
    }
    for i in 0..4u32 {
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        for (cx, cy) in [(4usize, 4usize), (10usize, 10usize)] {
            for dy in 0..3usize {
                for dx in 0..3usize {
                    let idx = ((cy + dy) * (size as usize) + (cx + dx)) * 4;
                    rgba[idx..idx + 4].copy_from_slice(&[255, 255, 255, 245]);
                }
            }
        }
        push_pixels(
            outputs,
            &format!("particlerain:particle/heavy_snow_{i}"),
            RgbaImage {
                width: size,
                height: size,
                rgba,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desaturate_uses_max_channel() {
        let mut img = RgbaImage {
            width: 1,
            height: 1,
            rgba: vec![30, 120, 200, 180],
        };
        desaturate(&mut img);
        assert_eq!(img.rgba, vec![200, 200, 200, 180]);
    }

    #[test]
    fn boost_alpha_scales_low_alpha_sheet() {
        let mut img = RgbaImage {
            width: 2,
            height: 1,
            rgba: vec![255, 255, 255, 64, 255, 255, 255, 32],
        };
        boost_alpha(&mut img);
        assert_eq!(img.rgba[3], 192);
        assert_eq!(img.rgba[7], 96);
    }

    #[test]
    fn ripple_generates_nonempty_white_ring() {
        let img = generate_ripple(3, 16);
        assert_eq!(img.width, 16);
        assert_eq!(img.height, 16);
        let lit = img.rgba.chunks_exact(4).filter(|p| p[3] == 255).count();
        assert!(lit >= 8, "expected lit ring pixels, got {lit}");
    }

    #[test]
    fn merge_sprites_keeps_highest_alpha() {
        let sheet = RgbaImage {
            width: 2,
            height: 4,
            rgba: vec![
                // Tile 0 (row 0, col 0): 2x2
                10, 10, 10, 50, 10, 10, 10, 200, // row 0
                10, 10, 10, 50, 10, 10, 10, 50, // row 1
                // Tile 1 (row 1, col 0): 2x2
                90, 90, 90, 150, 90, 90, 90, 100, // row 2
                90, 90, 90, 20, 90, 90, 90, 250, // row 3
            ],
        };
        let merged = merge_sprites(&sheet, 0, 2, 1, 2, 2);
        let max_alpha = merged.rgba.chunks_exact(4).map(|p| p[3]).max().unwrap();
        assert_eq!(max_alpha, 250);
    }
}
