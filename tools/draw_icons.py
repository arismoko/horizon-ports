#!/usr/bin/env python3
"""Original registry icons (256x256) for ports whose upstream logo can't be redistributed.

Drawn here from scratch as 32x32 pixel art and scaled up 8x (nearest), so the art is ours to license with the
port. Usage: draw_icons.py OUTDIR  -> OUTDIR/<id>.png for each id below.
"""
import math
import sys
from pathlib import Path

from PIL import Image, ImageDraw

N = 32


def bg(top, bottom, r=5):
    """Rounded square with a vertical gradient."""
    im = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    px = im.load()
    for y in range(N):
        t = y / (N - 1)
        c = tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,)
        for x in range(N):
            # corner rounding
            cx = min(x, N - 1 - x)
            cy = min(y, N - 1 - y)
            if cx < r and cy < r and (r - cx - 0.5) ** 2 + (r - cy - 0.5) ** 2 > r * r:
                continue
            px[x, y] = c
    return im


def put(im, pts, col):
    px = im.load()
    for x, y in pts:
        if 0 <= x < N and 0 <= y < N:
            px[x, y] = col


def universalbonemeal():
    im = bg((54, 120, 62), (22, 60, 34))
    d = ImageDraw.Draw(im)
    # soil mound
    d.ellipse((3, 22, 28, 34), fill=(92, 62, 38, 255))
    d.ellipse((5, 23, 26, 31), fill=(116, 80, 50, 255))
    # sprouts growing out of the soil, three heights
    stem = (70, 170, 60, 255)
    leaf = (120, 214, 92, 255)
    for x0, h in ((9, 6), (16, 11), (23, 8)):
        for y in range(24 - h, 25):
            put(im, [(x0, y)], stem)
        top = 24 - h
        put(im, [(x0 - 1, top + 1), (x0 - 2, top), (x0 + 1, top + 2), (x0 + 2, top + 1)], leaf)
        put(im, [(x0 - 1, top), (x0 + 1, top + 1)], leaf)
    # a diagonal bone over the top left, white with a soft shadow
    bone = (244, 240, 226, 255)
    shade = (196, 188, 166, 255)
    for i in range(10):
        x, y = 6 + i, 15 - i
        put(im, [(x, y), (x + 1, y), (x, y + 1)], bone)
        put(im, [(x + 1, y + 1)], shade)
    for cx, cy in ((5, 16), (6, 17), (16, 5), (17, 6)):
        put(im, [(cx, cy), (cx - 1, cy), (cx, cy - 1), (cx + 1, cy), (cx, cy + 1)], bone)
    put(im, [(5, 18), (7, 18), (18, 7), (18, 5)], shade)
    # sparkles
    spark = (255, 252, 190, 255)
    for sx, sy in ((25, 5), (27, 13), (12, 20)):
        put(im, [(sx, sy), (sx - 1, sy), (sx + 1, sy), (sx, sy - 1), (sx, sy + 1)], spark)
    return im


def easymagic():
    im = bg((70, 40, 120), (24, 12, 52))
    d = ImageDraw.Draw(im)
    # table top (dark wood with red cloth) and an open book above it
    d.rectangle((5, 23, 26, 28), fill=(58, 30, 30, 255))
    d.rectangle((5, 22, 26, 23), fill=(150, 36, 48, 255))
    d.rectangle((7, 29, 9, 30), fill=(40, 20, 20, 255))
    d.rectangle((22, 29, 24, 30), fill=(40, 20, 20, 255))
    cover = (110, 60, 30, 255)
    page = (238, 226, 196, 255)
    line = (160, 140, 110, 255)
    # open book resting on the cloth: cover edge, two pages that rise towards the outer edges, the spine
    d.rectangle((7, 20, 24, 21), fill=cover)
    for x in range(8, 24):
        lift = abs(x - 15.5) // 3  # pages bow up away from the spine
        top = int(16 - lift)
        for y in range(top, 20):
            put(im, [(x, y)], page)
        if x % 2 == 0 and x not in (14, 15, 16, 17):
            put(im, [(x, top + 2)], line)
            put(im, [(x, top + 4)], line) if top + 4 < 20 else None
    d.line((15, 15, 15, 20), fill=(120, 96, 70, 255))
    d.line((16, 15, 16, 20), fill=(200, 184, 150, 255))
    # rune glyphs rising from the book in an arc
    glow = (150, 230, 255, 255)
    for k in range(7):
        a = math.pi * (0.1 + 0.8 * k / 6)
        x = round(15.5 - 10 * math.cos(a))
        y = round(13 - 8 * math.sin(a))
        put(im, [(x, y), (x + 1, y), (x, y + 1)], glow)
    # a reroll arrow (circular) at the top right
    arrow = (255, 214, 90, 255)
    for t in range(0, 300, 25):
        a = math.radians(t)
        put(im, [(round(25 + 3.5 * math.cos(a)), round(6 + 3.5 * math.sin(a)))], arrow)
    put(im, [(28, 3), (29, 4), (27, 4), (28, 5)], arrow)
    return im


def armorstatues():
    im = bg((70, 92, 130), (30, 40, 66))
    d = ImageDraw.Draw(im)
    wood = (150, 110, 64, 255)
    dark = (104, 74, 42, 255)
    # stand: base slab, pole, shoulder bar, arms raised in a pose
    d.rectangle((8, 27, 23, 28), fill=(140, 140, 146, 255))
    d.rectangle((8, 29, 23, 29), fill=(96, 96, 104, 255))
    d.rectangle((15, 9, 16, 26), fill=wood)
    d.rectangle((10, 12, 21, 13), fill=wood)
    d.line((10, 12, 6, 6), fill=dark)
    d.line((21, 12, 25, 7), fill=dark)
    d.rectangle((12, 20, 19, 21), fill=wood)
    # head
    d.rectangle((13, 4, 18, 9), fill=(196, 160, 112, 255))
    d.rectangle((13, 4, 18, 5), fill=(170, 136, 92, 255))
    # chestplate in gold over the shoulders
    gold = (236, 196, 72, 255)
    gshade = (184, 140, 40, 255)
    d.rectangle((11, 12, 20, 19), fill=gold)
    d.rectangle((14, 15, 17, 19), fill=gshade)
    d.rectangle((11, 12, 20, 12), fill=(255, 232, 140, 255))
    # pose marks
    for x, y in ((4, 4), (27, 5), (5, 9)):
        put(im, [(x, y)], (255, 255, 255, 200))
    return im


def blockrunner():
    im = bg((120, 176, 228), (70, 130, 70))
    d = ImageDraw.Draw(im)
    # a dirt path receding to the horizon
    d.polygon([(4, 31), (27, 31), (19, 16), (12, 16)], fill=(178, 142, 84, 255))
    d.polygon([(7, 31), (24, 31), (18, 18), (13, 18)], fill=(200, 164, 100, 255))
    d.rectangle((0, 15, 31, 16), fill=(84, 150, 70, 255))
    # speed lines
    for y, x0, x1 in ((7, 3, 11), (10, 1, 9), (13, 4, 10)):
        d.line((x0, y, x1, y), fill=(255, 255, 255, 230))
    # a boot mid-stride, top right
    boot = (120, 72, 40, 255)
    d.rectangle((18, 4, 22, 10), fill=boot)
    d.rectangle((18, 10, 27, 13), fill=boot)
    d.rectangle((18, 13, 27, 13), fill=(60, 36, 20, 255))
    d.rectangle((18, 4, 22, 5), fill=(150, 96, 56, 255))
    return im


ICONS = {"universalbonemeal": universalbonemeal, "easymagic": easymagic, "armorstatues": armorstatues,
         "blockrunner": blockrunner}

if __name__ == "__main__":
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for name, fn in ICONS.items():
        fn().resize((256, 256), Image.NEAREST).save(out / f"{name}.png")
        print(out / f"{name}.png")
