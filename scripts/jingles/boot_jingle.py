#!/usr/bin/env python3
"""Writes the boot jingle, 29-matsukaze-to-furin, as the raw samples the launcher plays:
48 kHz, 16-bit little-endian, mono.

The candidates share random generators, so it is made after the ones before it in
make_jingles_furin, as when the user chose it, to come out the same to the sample.

Usage: boot_jingle.py src/launcher/sounds/boot-jingle.s16  (needs numpy)
"""
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import make_jingles_furin as furin  # noqa: E402

for jingle in furin.JINGLES:
    samples = jingle()
    if jingle is furin.matsukaze_to_furin:
        break
Path(sys.argv[1]).write_bytes(samples.astype("<i2").tobytes())
print(f"{sys.argv[1]}: {len(samples) / furin.RATE:.2f} s")
