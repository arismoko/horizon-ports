#!/usr/bin/env python3
"""Generate an original 64x64 pixel-art wooden plank bridge icon for mcwbridges.

No assets from the original jar are read or copied.
"""

import struct
import zlib
from pathlib import Path

W = H = 64


def write_png(path: Path, pixels):
    def chunk(tag, data):
        crc = zlib.crc32(tag + data) & 0xFFFFFFFF
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", crc)

    raw = bytearray()
    for y in range(H):
        raw.append(0)
        for x in range(W):
            raw.extend(pixels[y][x])

    ihdr = struct.pack(">IIBBBBB", W, H, 8, 6, 0, 0, 0)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    path.write_bytes(png)


def make_icon():
    # Simple 16x16 pixel-art wooden bridge over water, scaled 4x to 64x64
    sky_top = (118, 178, 232, 255)
    sky_bot = (168, 212, 245, 255)
    water_a = (46, 108, 184, 255)
    water_b = (62, 132, 206, 255)
    wood_hi = (196, 152, 92, 255)
    wood_md = (160, 118, 66, 255)
    wood_dk = (114, 80, 42, 255)
    post_dk = (88, 58, 28, 255)
    rope = (214, 192, 140, 255)
    lantern_glow = (255, 218, 92, 255)
    lantern_frame = (58, 58, 66, 255)

    grid = [[sky_top for _ in range(16)] for _ in range(16)]
    for y in range(16):
        t = y / 11.0
        if y < 12:
            c = tuple(
                int(sky_top[i] * (1 - t) + sky_bot[i] * t) if i < 3 else 255
                for i in range(4)
            )
            for x in range(16):
                grid[y][x] = c
        else:
            for x in range(16):
                grid[y][x] = water_b if (x + y) % 3 == 0 else water_a

    # Bridge piers going down into water
    for px in (2, 13):
        for y in range(9, 16):
            grid[y][px] = post_dk
            grid[y][px + (1 if px == 2 else -1)] = wood_dk

    # Arch under-beams
    for x in range(4, 12):
        if x in (4, 11):
            grid[11][x] = wood_dk
        elif x in (5, 10):
            grid[10][x] = wood_dk

    # Bridge deck (y=8..9)
    for x in range(16):
        grid[8][x] = wood_hi if x % 2 == 0 else wood_md
        grid[9][x] = wood_md if x % 2 == 0 else wood_dk

    # Handrail posts
    for px in (1, 5, 10, 14):
        for y in range(4, 8):
            grid[y][px] = post_dk if y == 4 else wood_dk

    # Top handrail / rope curve
    for x in range(16):
        if x in (1, 5, 10, 14):
            continue
        if x in (2, 4, 6, 9, 11, 13):
            grid[5][x] = rope
        elif x in (3, 7, 8, 12):
            grid[6][x] = rope
        else:
            grid[5][x] = wood_md

    # Two small bridge lanterns on center posts
    for lx in (5, 10):
        grid[3][lx] = lantern_frame
        grid[4][lx] = lantern_glow

    # Scale 4x to 64x64
    pixels = [[grid[y // 4][x // 4] for x in range(W)] for y in range(H)]
    return pixels


def main():
    out = Path(__file__).resolve().parent.parent / "icon.png"
    write_png(out, make_icon())
    print(f"Wrote {out} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
