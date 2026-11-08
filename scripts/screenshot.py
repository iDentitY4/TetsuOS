#!/usr/bin/env python3
"""Saves the device's 640x480 XRGB framebuffer as a PNG.

usage: scripts/screenshot.py FILE.png
"""
import struct, subprocess, sys, zlib
SSH = ["ssh", "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null",
       "-o", "LogLevel=ERROR", "root@10.35.35.1"]
w, h = 640, 480
raw = subprocess.run(SSH + ["head -c %d /dev/fb0" % (w * h * 4)], capture_output=True, check=True).stdout
rows = []
for y in range(h):
    line = raw[y * w * 4:(y + 1) * w * 4]
    rgb = bytearray(w * 3)
    rgb[0::3], rgb[1::3], rgb[2::3] = line[2::4], line[1::4], line[0::4]
    rows.append(b"\0" + bytes(rgb))
def chunk(t, d):
    return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
