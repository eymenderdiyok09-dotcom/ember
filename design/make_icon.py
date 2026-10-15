"""Renders the Ember app icon (1024x1024 PNG) with no dependencies."""
import math, struct, sys, zlib

FLAME = [
    "......#.....",
    ".....##.....",
    "....###..#..",
    "....####.##.",
    "...#####.##.",
    "..##oo####..",
    "..#ooo#####.",
    ".##oyyo####.",
    ".#ooyyyoo###",
    ".#oyyyyyoo##",
    ".#oyyWWyyo##",
    "..#oyWWyyo#.",
    "..##oyyyo##.",
    "....######..",
]
COLORS = {"#": (232, 72, 28), "o": (255, 138, 36), "y": (255, 204, 64), "W": (255, 244, 200)}
SIZE, INSET, RADIUS, PX = 1024, 100, 185, 44

def rounded_alpha(x, y):
    lo, hi = INSET, SIZE - INSET
    cx = min(max(x, lo + RADIUS), hi - RADIUS)
    cy = min(max(y, lo + RADIUS), hi - RADIUS)
    d = math.hypot(x - cx, y - cy)
    return max(0.0, min(1.0, RADIUS - d + 0.5))

w, h = len(FLAME[0]) * PX, len(FLAME) * PX
ox, oy = (SIZE - w) // 2, (SIZE - h) // 2 + 20
rows = []
for y in range(SIZE):
    row = bytearray(b"\x00")
    for x in range(SIZE):
        a = rounded_alpha(x + 0.5, y + 0.5)
        if a <= 0:
            row += b"\x00\x00\x00\x00"
            continue
        t = (y - INSET) / (SIZE - 2 * INSET)
        r, g, b = 24 + 16 * t, 16 + 4 * t, 22 - 6 * t
        # ember glow behind the flame
        glow = max(0.0, 1 - math.hypot(x - SIZE / 2, y - SIZE * 0.56) / 420) ** 2
        r, g, b = r + 150 * glow, g + 55 * glow, b + 10 * glow
        # faint block grid
        if (x - INSET) % 64 < 2 or (y - INSET) % 64 < 2:
            r, g, b = r + 6, g + 5, b + 6
        fx, fy = (x - ox) // PX, (y - oy) // PX
        if 0 <= fx < len(FLAME[0]) and 0 <= fy < len(FLAME) and x >= ox and y >= oy:
            c = FLAME[fy][fx]
            if c in COLORS:
                r, g, b = COLORS[c]
                # bevel each pixel like a block
                lx, ly = (x - ox) % PX, (y - oy) % PX
                if lx < 5 or ly < 5:
                    r, g, b = r + 25, g + 25, b + 25
                elif lx >= PX - 5 or ly >= PX - 5:
                    r, g, b = r * 0.78, g * 0.78, b * 0.78
        row += bytes(int(max(0, min(255, v))) for v in (r, g, b, a * 255))
    rows.append(bytes(row))

def chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
