#!/usr/bin/env python3
"""Generate the Doit app icon (AppIcon.icns).

Renders a macOS-style rounded-square with a white checkmark (todo motif),
supersampled for smooth edges, writes AppIcon.icns next to this script using
only stdlib + system tools (sips, iconutil).

Usage: python3 gen_icon.py [out_dir]
"""

import math
import os
import struct
import subprocess
import sys
import tempfile
import zlib

OUT_DIR = os.path.dirname(os.path.abspath(__file__)) if len(sys.argv) < 2 else sys.argv[1]

SIZE = 1024            # master size in px
SS = 2                 # supersampling factor (samples per axis per pixel)
R = 0.228              # rounded-rect corner radius (fraction of width)
CHECK_T = 0.052        # check stroke half-thickness (fraction of width)
BLUE = (59, 130, 246)  # #3B82F6
WHITE = (255, 255, 255)


def rect_sdf(px: float, py: float) -> float:
    """Signed distance to a centered rounded square in normalized [-0.5, 0.5] space."""
    qx = abs(px) - (0.5 - R)
    qy = abs(py) - (0.5 - R)
    ax, ay = max(qx, 0.0), max(qy, 0.0)
    return math.hypot(ax, ay) + min(max(qx, qy), 0.0) - R


def seg_dist(px, py, ax, ay, bx, by):
    """Distance from (px,py) to segment A->B."""
    abx, aby = bx - ax, by - ay
    apx, apy = px - ax, py - ay
    denom = abx * abx + aby * aby
    t = (apx * abx + apy * aby) / denom if denom else 0.0
    t = max(0.0, min(1.0, t))
    cx, cy = ax + t * abx, ay + t * aby
    return math.hypot(px - cx, py - cy)


def check_sdf(px: float, py: float) -> float:
    """Signed distance to a check mark made of two segments, in the same
    centered [-0.5, 0.5] space as the rounded square."""
    # A ✓ from upper-left, dipping just below centre, rising to the right.
    d1 = seg_dist(px, py, -0.23, 0.03, -0.07, 0.19)
    d2 = seg_dist(px, py, -0.07, 0.19, 0.23, -0.16)
    return min(d1, d2) - CHECK_T


def render_pixel(x: int, y: int):
    """Average RGBA over SS*SS subsamples to anti-alias."""
    r = g = b = a = 0
    for i in range(SS):
        for j in range(SS):
            u = (x + (i + 0.5) / SS) / SIZE  # 0..1
            v = (y + (j + 0.5) / SS) / SIZE
            px = u - 0.5
            py = v - 0.5
            if rect_sdf(px, py) > 0:
                continue
            alpha = 1.0
            if check_sdf(px, py) <= 0:
                cr, cg, cb = WHITE
            else:
                cr, cg, cb = BLUE
            r += cr * alpha
            g += cg * alpha
            b += cb * alpha
            a += alpha * 255
    n = SS * SS
    return (round(r / n), round(g / n), round(b / n), round(a / n))


def write_png(path: str, w: int, h: int, pixels):
    def chunk(typ: bytes, data: bytes) -> bytes:
        out = struct.pack(">I", len(data)) + typ + data
        out += struct.pack(">I", zlib.crc32(typ + data) & 0xFFFFFFFF)
        return out

    raw = bytearray()
    row = bytearray()
    for idx, (rr, gg, bb, aa) in enumerate(pixels):
        if idx % w == 0:
            if row:
                raw += b"\x00" + bytes(row)
                row = bytearray()
        row += bytes((rr, gg, bb, aa))
    raw += b"\x00" + bytes(row)

    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", ihdr))
        f.write(chunk(b"IDAT", zlib.compress(bytes(raw), 9)))
        f.write(chunk(b"IEND", b""))


def main():
    print("rendering master...", flush=True)
    master = [(x, y, render_pixel(x, y)) for y in range(SIZE) for x in range(SIZE)]
    master_png = os.path.join(OUT_DIR, "icon_1024.png")
    write_png(master_png, SIZE, SIZE, [p[2] for p in master])
    print(f"wrote {master_png}")

    iconset = os.path.join(tempfile.mkdtemp(), "AppIcon.iconset")
    os.makedirs(iconset, exist_ok=True)

    def put(name, size):
        out = os.path.join(iconset, name)
        subprocess.run(["sips", "-z", str(size), str(size), master_png, "--out", out],
                       check=True, capture_output=True)

    put("icon_16x16.png", 16)
    put("icon_16x16@2x.png", 32)
    put("icon_32x32.png", 32)
    put("icon_32x32@2x.png", 64)
    put("icon_128x128.png", 128)
    put("icon_128x128@2x.png", 256)
    put("icon_256x256.png", 256)
    put("icon_256x256@2x.png", 512)
    put("icon_512x512.png", 512)
    put("icon_512x512@2x.png", 1024)

    icns = os.path.join(OUT_DIR, "AppIcon.icns")
    subprocess.run(["iconutil", "-c", "icns", iconset, "-o", icns], check=True)
    print(f"wrote {icns}")


if __name__ == "__main__":
    main()
