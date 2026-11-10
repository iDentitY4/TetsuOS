#!/usr/bin/env python3
"""Boot jingle candidates from the sounds that go with a tetsubin: the Nambu furin (the
cast-iron wind chime from the same ironware tradition) and matsukaze, the tea ceremony's
"wind in the pines", the rushing of a kettle just before it boils.

A furin is a small, thick iron bell struck from inside by a clapper swung by the wind:
a hard metal click, a high fundamental that rings for seconds, a few weaker upper modes,
and a slow beat of a few Hz from the casting not being perfectly round.
"""
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from make_jingles import RATE, hz, pulse, tri, write  # noqa: E402
from make_jingles_iron import Piece, band_noise, crush  # noqa: E402

RNG = np.random.default_rng(1)

# ratio, loudness, ring seconds
FURIN = [(1.0, 1.0, 1.4), (2.68, 0.32, 0.45), (5.17, 0.13, 0.2), (8.4, 0.05, 0.1)]


def furin(note, dur, vol=1.0, beat=2.5, hardness=1.0):
    """One clapper strike. `beat` is the slow wobble in Hz between the bell's two halves."""
    f0 = hz(note) if isinstance(note, str) else note
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    out = np.zeros(n)
    for ratio, loud, ring in FURIN:
        loud *= hardness ** (ratio - 1)
        f = f0 * ratio
        if f > RATE / 2.2:
            continue
        for df, share in ((0.0, 0.6), (beat * ratio ** 0.5, 0.4)):
            out += np.sin(2 * np.pi * (f + df) * t + RNG.uniform(0, 6.28)) * loud * share \
                * np.exp(-t / ring)
    out *= np.minimum(1, t / 0.0004)
    click = band_noise(min(dur, 0.015), 7000, 2500, 0.0015)
    out[: len(click)] += click * 0.35 * hardness
    return out / np.abs(out).max() * vol


def breeze(strikes, note, dur, start=0.0, beat=2.5):
    """Clapper strikes as the wind swings it: (time, loudness) pairs."""
    p = np.zeros(int(dur * RATE))
    for t, vol in strikes:
        s = furin(note, dur - start - t, vol=vol, beat=beat, hardness=0.6 + 0.4 * vol)
        i = int((start + t) * RATE)
        p[i:i + len(s)] += s
    return p


def matsukaze(dur, swell_at=0.6, vol=1.0):
    """The kettle's rush before the boil: soft, breathy noise in a few bands, rising and
    falling unevenly, with the faint hum of the iron pieces in the kettle's floor."""
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    out = np.zeros(n)
    for centre, width, rate_, phase in ((420, 260, 0.9, 0.0), (900, 500, 1.4, 1.3),
                                        (1900, 900, 2.1, 2.2)):
        b = band_noise(dur, centre, width, 1e9)
        b /= np.abs(b).max()
        lfo = 0.65 + 0.35 * np.sin(2 * np.pi * rate_ * t + phase)
        out += b * lfo * (centre / 420) ** -0.6
    swell = np.clip(t / swell_at, 0, 1) ** 1.5
    hum = np.sin(2 * np.pi * 392 * t) * 0.05 * (0.6 + 0.4 * np.sin(2 * np.pi * 5.5 * t))
    out = (out / np.abs(out).max() + hum) * swell
    return out * vol


# ---- the candidates ------------------------------------------------------------------


def furin_single():
    """One clean strike of an iron wind chime, ringing out."""
    p = Piece(1.4)
    p.add(0.0, furin("A6", 1.4))
    return p.finish(room=0.15)


def furin_breeze():
    """A breeze catching the chime: chi-ri-rin, three uneven strikes, the last strongest."""
    p = Piece(1.5)
    p.add(0.0, breeze([(0.0, 0.55), (0.11, 0.4), (0.27, 1.0)], "A6", 1.5))
    return p.finish(room=0.15)


def two_furin():
    """Two chimes of different sizes on one eave, answering each other, then together."""
    p = Piece(1.6)
    p.add(0.0, breeze([(0.0, 0.7), (0.3, 0.9)], "E6", 1.6, beat=2.0))
    p.add(0.0, breeze([(0.15, 0.7), (0.32, 1.0)], "B6", 1.6, beat=3.0))
    return p.finish(room=0.18)


def furin_gust():
    """A gust through a row of chimes tuned to a Japanese pentatonic, D E G A B, climbing."""
    p = Piece(1.6)
    notes = ["D6", "E6", "G6", "A6", "B6", "D7"]
    for i, note in enumerate(notes):
        t = i * 0.07 + RNG.uniform(-0.01, 0.01)
        p.add(t, furin(note, 1.6 - t, vol=0.5 + 0.08 * i, beat=RNG.uniform(1.8, 3.2)))
    p.add(0.5, furin("D7", 1.1, vol=0.6, hardness=0.7))
    return p.finish(room=0.2)


def matsukaze_to_furin():
    """The kettle's rush rising like wind in the pines, then the chime answers it."""
    p = Piece(1.7)
    p.add(0.0, matsukaze(0.75, swell_at=0.55) * np.r_[np.ones(int(0.55 * RATE)),
                                                       np.linspace(1, 0, int(0.2 * RATE))],
          vol=0.35)
    p.add(0.5, breeze([(0.0, 0.6), (0.14, 1.0)], "G6", 1.2))
    return p.finish(room=0.18)


def kettle_and_furin():
    """A drumstick on the kettle, its lid rattling, and the chime ringing above it."""
    from make_jingles_iron import drumstick, lid_rattle
    p = Piece(1.5)
    p.add(0.0, drumstick(hz("D5"), 1.5), vol=0.6)
    p.add(0.0, lid_rattle(hz("A6"), 0.4, vol=0.12))
    p.add(0.18, breeze([(0.0, 0.7), (0.12, 1.0)], "A6", 1.32))
    return p.finish(room=0.15)


def chip_furin():
    """Three chip notes in the same pentatonic, D, E and A, landing on the chime."""
    p = Piece(1.5)
    for i, (note, bass) in enumerate((("D6", "D4"), ("E6", "E4"), ("A6", "A3"))):
        t = i * 0.13
        p.add(t, pulse(note, 0.11, duty=0.25, vol=0.4, decay=0.08, sustain=0.5,
                       release=0.02))
        p.add(t, tri(bass, 0.11, vol=0.55, release=0.02))
    t = 0.39
    p.add(t, breeze([(0.0, 1.0), (0.2, 0.45)], "D7", 1.5 - t))
    p.add(t, furin("A6", 1.5 - t, vol=0.55))
    p.add(t, tri("D4", 1.5 - t, vol=0.4, decay=0.5, sustain=0.3, release=0.3))
    return p.finish(room=0.15)


def pixel_furin():
    """The chime in a breeze as an old console's sample: crunchy, still ringing."""
    p = Piece(1.5)
    sig = breeze([(0.0, 0.55), (0.11, 0.4), (0.27, 1.0)], "E6", 1.5)
    p.add(0.0, crush(sig, 16000, 5))
    return p.finish(room=0.08)


JINGLES = [furin_single, furin_breeze, two_furin, furin_gust, matsukaze_to_furin,
           kettle_and_furin, chip_furin, pixel_furin]

if __name__ == "__main__":
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for i, fn in enumerate(JINGLES, 25):
        samples = fn()
        path = out / f"{i}-{fn.__name__.replace('_', '-')}.wav"
        write(path, samples)
        a = samples
        q = len(a) // 4
        print(f"{path.name:26} {len(a) / RATE:.2f} s dc {int(a.mean()):4} rms",
              [int(np.sqrt((a[k * q:(k + 1) * q] ** 2).mean())) for k in range(4)],
              fn.__doc__.split('.')[0][:60])
