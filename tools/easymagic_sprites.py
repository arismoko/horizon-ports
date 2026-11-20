#!/usr/bin/env python3
"""Original sprites for the Easy Magic port's re-roll button (native-next/easymagic/src/client.rs).

The first port encoded upstream's enchanting_table_reroll.png; its art isn't licensed for redistribution, so
these are drawn here from scratch (button frames 38x27, a 15x15 re-roll arrow, 12x12 XP and lapis markers) and
written into client.rs as runs (x, y, length, r, g, b, a), replacing the block between the "// Sprites" comment
and `fn sprite`. Usage: easymagic_sprites.py [client.rs] [--png DIR to also save previews].
"""
import math
import re
import sys
from pathlib import Path

from PIL import Image

T = (0, 0, 0, 0)


def frame(fill, light, dark, edge):
    w, h = 38, 27
    im = Image.new("RGBA", (w, h), T)
    px = im.load()
    for y in range(h):
        for x in range(w):
            corner = (x in (0, w - 1)) and (y in (0, h - 1))
            if corner:
                continue
            if x == 0 or y == 0 or x == w - 1 or y == h - 1:
                px[x, y] = edge
            elif x == 1 or y == 1:
                px[x, y] = light
            elif x == w - 2 or y == h - 2:
                px[x, y] = dark
            else:
                px[x, y] = fill
    return im


def arrow(col):
    """A circular re-roll arrow: three quarters of a ring, arrowhead at the top right."""
    im = Image.new("RGBA", (15, 15), T)
    px = im.load()
    cx = cy = 7.0
    for y in range(15):
        for x in range(15):
            r = math.hypot(x - cx, y - cy)
            a = math.degrees(math.atan2(y - cy, x - cx)) % 360
            if 4.0 <= r <= 6.2 and not (280 <= a <= 350):
                px[x, y] = col
    for x, y in ((9, 0), (10, 1), (11, 2), (12, 3), (13, 4), (10, 2), (11, 3), (12, 4), (11, 4), (10, 3),
                 (13, 3), (13, 2), (13, 1)):
        px[x, y] = col
    return im


def orb(rim, body, shine):
    im = Image.new("RGBA", (12, 12), T)
    px = im.load()
    for y in range(12):
        for x in range(12):
            r = math.hypot(x - 5.5, y - 5.5)
            if r <= 4.6:
                px[x, y] = body
            elif r <= 5.6:
                px[x, y] = rim
    for x, y in ((4, 3), (3, 4), (4, 4)):
        px[x, y] = shine
    return im


def gem(rim, body, shine):
    """Lapis marker: a faceted diamond."""
    im = Image.new("RGBA", (12, 12), T)
    px = im.load()
    for y in range(12):
        for x in range(12):
            d = abs(x - 5.5) + abs(y - 5.5)
            if d <= 4.0:
                px[x, y] = body
            elif d <= 5.0:
                px[x, y] = rim
    for x, y in ((5, 3), (4, 4), (5, 4)):
        px[x, y] = shine
    return im


def runs(im):
    out = []
    px = im.load()
    w, h = im.size
    for y in range(h):
        x = 0
        while x < w:
            c = px[x, y]
            if c[3] == 0:
                x += 1
                continue
            n = 1
            while x + n < w and px[x + n, y] == c:
                n += 1
            out.append((x, y, n) + tuple(c))
            x += n
    return out


SPRITES = {
    "FRAME_OFF": frame((58, 52, 44, 255), (66, 60, 50, 255), (48, 43, 36, 255), (28, 25, 21, 255)),
    "FRAME_ON": frame((150, 128, 98, 255), (206, 184, 146, 255), (98, 82, 62, 255), (40, 33, 25, 255)),
    "FRAME_HOVER": frame((150, 104, 170, 255), (214, 172, 230, 255), (98, 62, 116, 255), (44, 26, 52, 255)),
    "ICON_OFF": arrow((44, 40, 33, 255)),
    "ICON_ON": arrow((70, 58, 42, 255)),
    "ICON_HOVER": arrow((255, 236, 120, 255)),
    "XP_ORB": orb((40, 70, 20, 255), (120, 210, 70, 255), (220, 255, 170, 255)),
    "XP_ORB_OFF": orb((36, 44, 30, 255), (84, 112, 70, 255), (130, 150, 110, 255)),
    "LAPIS_ORB": gem((14, 26, 70, 255), (52, 96, 200, 255), (190, 210, 250, 255)),
    "LAPIS_ORB_OFF": gem((20, 26, 44, 255), (44, 64, 110, 255), (90, 106, 140, 255)),
}


def block():
    lines = [
        "// Sprites drawn for this port by tools/easymagic_sprites.py (not upstream's texture): each as runs of",
        "// same-coloured pixels, (x, y, length, r, g, b, a).",
        "type Runs = &'static [(u8, u8, u8, u8, u8, u8, u8)];",
    ]
    for name, im in SPRITES.items():
        body = ", ".join("(" + ", ".join(map(str, r)) + ")" for r in runs(im))
        lines.append(f"const {name}: Runs = &[{body}];")
    return "\n".join(lines) + "\n\n"


if __name__ == "__main__":
    args = sys.argv[1:]
    if "--png" in args:
        i = args.index("--png")
        d = Path(args[i + 1])
        del args[i:i + 2]
        d.mkdir(parents=True, exist_ok=True)
        for name, im in SPRITES.items():
            im.resize((im.width * 8, im.height * 8), Image.NEAREST).save(d / f"{name}.png")
    if args:
        p = Path(args[0])
        src = p.read_text()
        new, n = re.subn(r"// (Generated from easymagic|Sprites drawn).*?(?=\nfn sprite\()", block().rstrip("\n") + "\n",
                         src, count=1, flags=re.S)
        assert n == 1, "sprite block not found"
        p.write_text(new)
        print("rewrote", p)
