#!/usr/bin/env python3
"""Writes PNG test images for png.rs with a hand-made writer, and the RGB they decode to.

usage: testdata/make.py testdata
"""
import random, struct, sys, zlib, os
out = sys.argv[1]
rng = random.Random(7)

def chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

def paeth(a, b, c):
    p = a + b - c; pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    return a if pa <= pb and pa <= pc else (b if pb <= pc else c)

def filtered(lines, bpp):
    """Each line with a random filter of the five."""
    data = bytearray(); prev = bytes(len(lines[0])) if lines else b""
    for line in lines:
        f = rng.randrange(5); row = bytearray([f])
        for x, v in enumerate(line):
            a = line[x - bpp] if x >= bpp else 0; b = prev[x]; c = prev[x - bpp] if x >= bpp else 0
            pred = [0, a, b, (a + b) // 2, paeth(a, b, c)][f]
            row.append((v - pred) & 0xFF)
        data += row; prev = line
    return bytes(data)

def pack(samples, depth):
    """Packs one line of samples at a bit depth."""
    if depth == 8: return bytes(samples)
    if depth == 16: return b"".join(struct.pack(">H", s) for s in samples)
    bits = "".join(format(s, "0%db" % depth) for s in samples)
    bits += "0" * (-len(bits) % 8)
    return bytes(int(bits[i:i + 8], 2) for i in range(0, len(bits), 8))

def write(name, w, h, color, depth, interlace=False, compress=None):
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color]
    maxv = (1 << depth) - 1
    palette = [(rng.randrange(256), rng.randrange(256), rng.randrange(256)) for _ in range(min(256, maxv + 1))]
    px = [[[rng.randrange(maxv + 1) for _ in range(channels)] for _ in range(w)] for _ in range(h)]
    def rgb(p):
        if color == 3: return palette[p[0]]
        top = (lambda v: v >> 8) if depth == 16 else ((lambda v: v) if depth == 8 else (lambda v: v * 255 // maxv))
        if color in (0, 4): g = top(p[0]); return (g, g, g)
        return (top(p[0]), top(p[1]), top(p[2]))
    bpp = max(1, channels * depth // 8)
    def encode(sub):
        lines = [pack([s for p in row for s in p], depth) for row in sub]
        return filtered(lines, bpp) if lines and lines[0] else b""
    if interlace:
        raw = b""
        for x0, y0, dx, dy in [(0,0,8,8),(4,0,8,8),(0,4,4,8),(2,0,4,4),(0,2,2,4),(1,0,2,2),(0,1,1,2)]:
            sub = [[px[y][x] for x in range(x0, w, dx)] for y in range(y0, h, dy)]
            if sub and sub[0]: raw += encode(sub)
    else:
        raw = encode(px)
    if compress == "stored": data = zlib.compress(raw, 0)
    elif compress == "fixed":
        c = zlib.compressobj(9, zlib.DEFLATED, 15, 9, zlib.Z_FIXED); data = c.compress(raw) + c.flush()
    else: data = zlib.compress(raw, 9)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, depth, color, 0, 0, int(interlace)))
    if color == 3: png += chunk(b"PLTE", bytes(v for c in palette for v in c))
    # Split the data over two IDAT chunks, as encoders may.
    half = len(data) // 2
    png += chunk(b"tEXt", b"Comment\x00test") + chunk(b"IDAT", data[:half]) + chunk(b"IDAT", data[half:]) + chunk(b"IEND", b"")
    open(os.path.join(out, name + ".png"), "wb").write(png)
    open(os.path.join(out, name + ".rgb"), "wb").write(bytes(v for row in px for p in row for v in rgb(p)))

write("rgb", 37, 23, 2, 8)
write("rgba", 19, 17, 6, 8)
write("palette", 21, 13, 3, 8)
write("palette4", 13, 9, 3, 4)
write("gray1", 29, 7, 0, 1)
write("gray16", 11, 5, 0, 16)
write("graya", 9, 9, 4, 8)
write("rgb16", 7, 6, 2, 16)
write("interlaced", 33, 27, 2, 8, interlace=True)
write("interlaced_small", 3, 2, 3, 2, interlace=True)
write("stored", 40, 30, 2, 8, compress="stored")
write("fixed", 40, 30, 2, 8, compress="fixed")
