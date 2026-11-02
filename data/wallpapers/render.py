#!/usr/bin/env python3
"""Render the preset wallpapers (original, procedural; Python 3 standard library only).

    python3 data/wallpapers/render.py            # all presets
    python3 data/wallpapers/render.py nord zen   # some of them

Writes <id>.png (2560x1440) next to this script and thumbs/<id>.png (480x270)
for the GUI preview cards. Every image is drawn from the shapes below, so the
output is the same on every run.
"""

import math
import os
import struct
import sys
import zlib
from multiprocessing import Pool

HERE = os.path.dirname(os.path.abspath(__file__))
FULL = (2560, 1440)
THUMB = (480, 270)


def rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i : i + 2], 16) / 255.0 for i in (0, 2, 4))


def mix(a, b, t):
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def smooth(e0, e1, x):
    t = min(max((x - e0) / (e1 - e0), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def gradient(stops, t):
    """Color at t (0..1) along [(pos, color), ...]."""
    if t <= stops[0][0]:
        return stops[0][1]
    for (p0, c0), (p1, c1) in zip(stops, stops[1:]):
        if t <= p1:
            return mix(c0, c1, smooth(p0, p1, t))
    return stops[-1][1]


# ---- styles ---------------------------------------------------------------
# Coordinates are resolution independent: u in 0..aspect, v in 0..1 (top-down).


def ridge(u, layer):
    """Height (v) of a ridge/wave line at u: a sum of sines."""
    v = layer["base"]
    for amp, freq, phase in layer["waves"]:
        v += amp * math.sin(u * freq + phase)
    return v


def render(spec, w, h):
    aspect = w / h
    sky = [(p, rgb(c)) for p, c in spec["sky"]]
    glows = [(gx * aspect, gy, r, rgb(c), s) for gx, gy, r, c, s in spec.get("glows", [])]
    layers = [dict(l, color=rgb(l["color"])) for l in spec.get("layers", [])]
    vignette = spec.get("vignette", 0.0)
    line = spec.get("line")
    light = spec.get("light", False)
    px = 1.0 / h  # one pixel in v units (for anti-aliasing)

    rows = []
    for y in range(h):
        v = (y + 0.5) / h
        base = gradient(sky, v)
        row = bytearray(w * 3)
        for x in range(w):
            u = (x + 0.5) / h
            r, g, b = base
            for gx, gy, rad, gc, s in glows:
                d2 = (u - gx) ** 2 + (v - gy) ** 2
                k = s * math.exp(-d2 / (rad * rad))
                if light:
                    # on a light sky a screen blend would vanish: tint instead
                    r += (gc[0] - r) * k
                    g += (gc[1] - g) * k
                    b += (gc[2] - b) * k
                else:
                    # screen blend keeps the glow soft
                    r = 1 - (1 - r) * (1 - gc[0] * k)
                    g = 1 - (1 - g) * (1 - gc[1] * k)
                    b = 1 - (1 - b) * (1 - gc[2] * k)
            for l in layers:
                top = ridge(u, l)
                cover = min(max((v - top) / px + 0.5, 0.0), 1.0)
                if cover > 0:
                    a = cover * l.get("alpha", 1.0)
                    # layers get a little darker towards the bottom
                    shade = 1 - l.get("fade", 0.0) * min(max(v - top, 0.0), 1.0)
                    c = l["color"]
                    r += (c[0] * shade - r) * a
                    g += (c[1] * shade - g) * a
                    b += (c[2] * shade - b) * a
            if line:
                lv, lc, la = line
                d = abs(v - lv) / px
                if d < 1.5:
                    k = la * (1 - d / 1.5) * smooth(0.0, 0.35, u / aspect) * smooth(1.0, 0.65, u / aspect)
                    c = rgb(lc)
                    r += (c[0] - r) * k
                    g += (c[1] - g) * k
                    b += (c[2] - b) * k
            if vignette:
                dx = u / aspect - 0.5
                dy = v - 0.5
                k = 1 - vignette * smooth(0.2, 0.75, math.sqrt(dx * dx + dy * dy))
                r, g, b = r * k, g * k, b * k
            i = x * 3
            row[i] = min(max(int(r * 255 + 0.5), 0), 255)
            row[i + 1] = min(max(int(g * 255 + 0.5), 0), 255)
            row[i + 2] = min(max(int(b * 255 + 0.5), 0), 255)
        rows.append(row)
    return rows


def write_png(path, rows, w, h):
    raw = bytearray()
    for row in rows:
        # filter 1 (Sub): smooth gradients become runs of tiny numbers
        raw.append(1)
        prev = b"\0\0\0" + bytes(row[:-3])
        raw += bytes((a - p) & 0xFF for a, p in zip(row, prev))

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    tmp = path + ".tmp"
    with open(tmp, "wb") as f:
        f.write(png)
    os.replace(tmp, path)


# ---- the wallpapers ---------------------------------------------------------
# sky: vertical gradient stops; glows: (x 0..1, y 0..1, radius, color, strength);
# layers: ridges/waves drawn back to front; line: (v, color, alpha).


def hills(colors, base, step, seed, fade=0.25):
    out = []
    for i, c in enumerate(colors):
        s = seed + i * 1.7
        out.append(
            {
                "color": c,
                "base": base + i * step,
                "fade": fade,
                "waves": [
                    (0.035 - i * 0.004, 2.1 + i * 0.6, s),
                    (0.018, 5.3 + i * 1.1, s * 2.3),
                    (0.006, 13.0 + i * 2.0, s * 0.7),
                ],
            }
        )
    return out


def waves(colors, base, step, seed, alpha=0.85):
    out = []
    for i, c in enumerate(colors):
        s = seed + i * 2.3
        out.append(
            {
                "color": c,
                "base": base + i * step,
                "alpha": alpha,
                "fade": 0.35,
                "waves": [(0.07 - i * 0.008, 1.6 + i * 0.25, s), (0.025, 3.7 + i * 0.4, s * 1.9)],
            }
        )
    return out


SPECS = {
    # calm blue: soft light over deep slate, gentle low waves
    "clean": {
        "sky": [(0.0, "#1b1e2b"), (0.6, "#252a3d"), (1.0, "#1b1e2b")],
        "glows": [(0.72, 0.30, 0.42, "#89b4fa", 0.35), (0.22, 0.85, 0.35, "#74c7ec", 0.18)],
        "layers": waves(["#2a3550", "#222b42", "#1a2033"], 0.68, 0.09, 0.4, 0.8),
        "vignette": 0.25,
    },
    # night city haze: blue and cyan glow behind dark skyline-like waves
    "tokyo-night": {
        "sky": [(0.0, "#0f1019"), (0.55, "#1a1b2e"), (1.0, "#16161e")],
        "glows": [(0.30, 0.55, 0.45, "#7aa2f7", 0.40), (0.78, 0.40, 0.32, "#2ac3de", 0.28)],
        "layers": waves(["#24283b", "#1f2335", "#16161e"], 0.62, 0.10, 2.1, 0.9),
        "vignette": 0.3,
    },
    # arctic: frost haze over polar night mountains
    "nord": {
        "sky": [(0.0, "#2e3440"), (0.55, "#4c566a"), (0.75, "#81a1c1")],
        "glows": [(0.60, 0.62, 0.40, "#88c0d0", 0.35)],
        "layers": hills(["#5e6f85", "#4c566a", "#3b4252", "#2e3440"], 0.55, 0.09, 1.3),
    },
    # retro sunset: amber horizon behind warm dark ridges
    "gruvbox": {
        "sky": [(0.0, "#1d2021"), (0.45, "#3c3836"), (0.7, "#d65d0e"), (0.8, "#fabd2f")],
        "glows": [(0.42, 0.68, 0.30, "#fe8019", 0.45)],
        "layers": hills(["#6b5440", "#4a3d33", "#32302f", "#1d2021"], 0.60, 0.08, 4.2),
        "vignette": 0.2,
    },
    # glass: soft, defocused frosted light blobs in icy blue and frost white
    "glass": {
        "sky": [(0.0, "#10161c"), (1.0, "#172029")],
        "glows": [
            (0.25, 0.30, 0.38, "#7cc4e4", 0.42),
            (0.70, 0.62, 0.45, "#4a8fb5", 0.38),
            (0.55, 0.15, 0.25, "#dcf0f8", 0.22),
            (0.15, 0.85, 0.28, "#3a7d96", 0.30),
        ],
        "vignette": 0.2,
    },
    # zen: almost nothing; a single faint horizon line
    "zen": {
        "sky": [(0.0, "#101113"), (1.0, "#16171a")],
        "glows": [(0.5, 0.62, 0.55, "#2a2c31", 0.6)],
        "line": (0.62, "#5c6068", 0.55),
        "vignette": 0.3,
    },
    # everforest: misty green valley
    "everforest": {
        "sky": [(0.0, "#232a2e"), (0.5, "#3d484d"), (0.7, "#7a8a6a")],
        "glows": [(0.35, 0.58, 0.38, "#a7c080", 0.35)],
        "layers": hills(["#56635f", "#425047", "#343f44", "#232a2e"], 0.52, 0.10, 2.7),
    },
    # rose pine: muted rose and gold waves on charcoal
    "rose-pine": {
        "sky": [(0.0, "#16151c"), (1.0, "#1f1d26")],
        "glows": [(0.25, 0.35, 0.40, "#ebbcba", 0.35), (0.80, 0.70, 0.35, "#f6c177", 0.20)],
        "layers": waves(["#3a2f38", "#2b2730", "#1c1b22"], 0.60, 0.11, 5.5, 0.8),
        "vignette": 0.25,
    },
    # kanagawa: layered ink-blue waves under an old-paper moon
    "kanagawa": {
        "sky": [(0.0, "#16161d"), (0.6, "#223249"), (1.0, "#1f1f28")],
        "glows": [(0.74, 0.26, 0.10, "#dcd7ba", 0.9), (0.74, 0.26, 0.30, "#7e9cd8", 0.25)],
        "layers": waves(["#2d4f67", "#223249", "#1f1f28"], 0.58, 0.11, 0.9, 0.9),
    },
    # solarized: teal and yellow light over deep sea
    "solarized": {
        "sky": [(0.0, "#002b36"), (1.0, "#073642")],
        "glows": [(0.30, 0.40, 0.42, "#2aa198", 0.35), (0.75, 0.75, 0.30, "#b58900", 0.30)],
        "layers": waves(["#0a4a58", "#063f4b", "#00313d"], 0.66, 0.09, 3.3, 0.85),
        "vignette": 0.2,
    },
    # latte: light, soft peach and sky blue
    "latte": {
        "light": True,
        "sky": [(0.0, "#f2f3f7"), (1.0, "#e6e9ef")],
        "glows": [(0.28, 0.32, 0.40, "#fbc4a0", 0.6), (0.75, 0.70, 0.45, "#b4cbf7", 0.6)],
        "layers": waves(["#d4dbe9", "#c3cbdc"], 0.72, 0.10, 1.9, 0.7),
    },
    # graphite: monochrome, one soft diagonal light
    "graphite": {
        "sky": [(0.0, "#0e0f11"), (1.0, "#18191c")],
        "glows": [(0.80, 0.10, 0.60, "#3a3d42", 0.7), (0.20, 0.95, 0.40, "#25272b", 0.5)],
        "layers": hills(["#1f2024", "#17181b"], 0.74, 0.10, 0.5, 0.1),
        "vignette": 0.25,
    },
}


def job(args):
    name, (w, h), path = args
    write_png(path, render(SPECS[name], w, h), w, h)
    return path


def main():
    names = sys.argv[1:] or list(SPECS)
    for n in names:
        if n not in SPECS:
            sys.exit(f"unknown wallpaper {n!r}; known: {', '.join(SPECS)}")
    os.makedirs(os.path.join(HERE, "thumbs"), exist_ok=True)
    jobs = []
    for n in names:
        jobs.append((n, FULL, os.path.join(HERE, f"{n}.png")))
        jobs.append((n, THUMB, os.path.join(HERE, "thumbs", f"{n}.png")))
    with Pool() as pool:
        for p in pool.imap_unordered(job, jobs):
            print(os.path.relpath(p, HERE))


if __name__ == "__main__":
    main()
