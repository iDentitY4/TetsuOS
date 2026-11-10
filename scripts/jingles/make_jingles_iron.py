#!/usr/bin/env python3
"""Iron-themed boot jingle candidates: a drumstick on a tetsubin, gongs, an anvil, and
blends of those with the chip sounds from make_jingles.

Struck metal is made as modes: decaying sine pairs at inharmonic ratios, each pair a
fraction of a percent apart so it beats like a real casting. A drumstick is a softer,
slower contact than a spoon, so it takes the top modes down and adds a short wooden
knock. A small room reverb makes it sound like an object rather than a synth.
"""
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from make_jingles import RATE, PEAK, hz, pulse, tri, noise, write  # noqa: E402

RNG = np.random.default_rng(35)


def band_noise(dur, centre, width, decay):
    """Noise filtered to a band around `centre` Hz, decaying in `decay` seconds."""
    n = int(dur * RATE)
    spec = np.fft.rfft(RNG.standard_normal(n))
    f = np.fft.rfftfreq(n, 1 / RATE)
    spec *= np.exp(-(((f - centre) / width) ** 2))
    sig = np.fft.irfft(spec, n)
    sig /= np.abs(sig).max()
    return sig * np.exp(-np.arange(n) / RATE / decay)


def modes(f0, dur, partials, hardness=1.0, attack=0.0015, bloom=None, bend=0.0):
    """Struck metal. `partials` are (ratio, loudness, ring seconds). A lower `hardness`
    (a softer stick) quietens the higher modes. `bloom` delays the high modes' peak by that
    many seconds, as in a gong. `bend` glides the pitch by that fraction in 0.25 s."""
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    glide = 1 + bend * (1 - np.exp(-t / 0.08))
    out = np.zeros(n)
    for ratio, loud, ring in partials:
        loud *= min(1.0, hardness ** (ratio - 1)) if hardness < 1 else 1.0
        for detune, share in ((1.0, 0.7), (1.0 + RNG.uniform(0.002, 0.006), 0.3)):
            f = f0 * ratio * detune
            if f > RATE / 2.2:
                continue
            phase = 2 * np.pi * np.cumsum(f * glide) / RATE
            e = np.exp(-t / ring)
            if bloom and ratio > 2:
                e *= 1 - np.exp(-t / (bloom * min(ratio / 6, 1)))
            out += np.sin(phase + RNG.uniform(0, 6.28)) * loud * share * e
    out *= np.minimum(1, t / attack)
    return out


# A thick cast-iron kettle: bell-like ratios, the low ones ringing longest.
TETSUBIN = [(1.0, 1.0, 0.9), (1.51, 0.55, 0.6), (2.32, 0.5, 0.4), (2.98, 0.35, 0.3),
            (3.83, 0.25, 0.2), (4.94, 0.18, 0.14), (6.21, 0.12, 0.09)]
# Its lid and knob: small, high and short.
LID = [(1.0, 1.0, 0.12), (2.61, 0.6, 0.06), (4.42, 0.4, 0.04)]
# A gong: many close modes, with the highest blooming after the hit.
GONG = [(r, 1.0 / (1 + 0.35 * i), 1.6 / (1 + 0.25 * i))
        for i, r in enumerate([1.0, 1.47, 1.98, 2.24, 2.71, 3.02, 3.53, 3.97, 4.41, 4.88,
                               5.36, 5.92, 6.47, 7.1, 7.83, 8.6])]
# An anvil: a few bright, long-ringing modes.
ANVIL = [(1.0, 1.0, 0.7), (2.76, 0.8, 0.5), (5.40, 0.5, 0.3), (8.93, 0.25, 0.15)]


def knock(dur=0.03, vol=0.4):
    """The wooden end of a drumstick meeting the iron."""
    return band_noise(dur, 900, 500, 0.006) * vol


def drumstick(f0, dur, vol=1.0, hardness=0.75):
    sig = modes(f0, dur, TETSUBIN, hardness=hardness, attack=0.002)
    sig /= np.abs(sig).max()
    k = knock()
    sig[: len(k)] += k
    return sig * vol


def lid_rattle(f0, dur, vol=0.3, bounces=5, first_gap=0.05):
    """The lid jumping in its seat a few times after a hard hit, each bounce sooner."""
    n = int(dur * RATE)
    out = np.zeros(n)
    t, gap, loud = 0.012, first_gap, 1.0
    for _ in range(bounces):
        hit = modes(f0 * RNG.uniform(0.99, 1.01), 0.15, LID, attack=0.0005) * loud
        i = int(t * RATE)
        out[i:i + len(hit)] += hit[: max(0, n - i)]
        t += gap
        gap *= 0.7
        loud *= 0.6
    return out / max(np.abs(out).max(), 1e-9) * vol


def reverb(sig, mix=0.18, size=1.0):
    """A small Schroeder room: four combs in parallel, two allpasses after."""
    def comb(x, d, g):
        y = x.copy()
        for k in range(d, len(y), d):
            y[k:k + d] += g * y[k - d:k][: len(y[k:k + d])]
        return y

    def allpass(x, d, g):
        y = np.zeros_like(x)
        xd = np.concatenate([np.zeros(d), x[:-d]])
        y[:d] = -g * x[:d]
        for k in range(d, len(y), d):
            seg = slice(k, k + d)
            m = len(y[seg])
            y[seg] = -g * x[seg] + xd[seg] + g * y[k - d:k - d + m]
        return y

    wet = sum(comb(sig, int(ms * size * RATE / 1000), 0.72) for ms in (29.7, 37.1, 41.1, 43.7))
    wet = allpass(allpass(wet / 4, int(5.0 * RATE / 1000), 0.7), int(1.7 * RATE / 1000), 0.7)
    return sig * (1 - mix) + wet * mix


class Piece:
    def __init__(self, seconds):
        self.buf = np.zeros(int(seconds * RATE))

    def add(self, t, sig, vol=1.0):
        i = int(t * RATE)
        sig = sig[: max(0, len(self.buf) - i)]
        self.buf[i:i + len(sig)] += sig * vol

    def finish(self, room=0.18, fade=0.1):
        self.buf = reverb(self.buf, room)
        f = int(fade * RATE)
        self.buf[-f:] *= np.linspace(1, 0, f) ** 2
        # a gentle limit on the first transient only, so the ring stays loud enough
        self.buf /= np.percentile(np.abs(self.buf), 99.8)
        self.buf = np.tanh(self.buf * 0.8)
        self.buf *= PEAK / np.abs(self.buf).max()
        return self.buf


def crush(sig, rate=11025, bits=5):
    """An 8-bit sampler's version: held at a low rate and few levels, like a console's
    sample channel."""
    step = RATE / rate
    idx = (np.floor(np.arange(len(sig)) / step) * step).astype(int)
    held = sig[idx]
    levels = 2 ** (bits - 1)
    peak = np.abs(held).max()
    return np.round(held / peak * levels) / levels * peak


# ---- the candidates ------------------------------------------------------------------


def tetsubin_strike():
    """One firm drumstick hit on the kettle's body, the lid rattling in its seat after."""
    p = Piece(1.4)
    p.add(0.0, drumstick(hz("D5"), 1.4))
    p.add(0.0, lid_rattle(hz("A6"), 0.6, vol=0.22))
    return p.finish()


def tetsubin_beat():
    """A rhythm on the kettle: body, body, lid knob, then a full hit with the lid rattling."""
    p = Piece(1.5)
    beat = 0.17
    p.add(0.0, drumstick(hz("A4"), 0.4, vol=0.6))
    p.add(beat, drumstick(hz("A4"), 0.4, vol=0.55))
    p.add(2 * beat, modes(hz("E6"), 0.25, LID, attack=0.0005), vol=0.5)
    p.add(2 * beat, knock(vol=0.25))
    t = 3 * beat
    p.add(t, drumstick(hz("D5"), 1.5 - t, vol=1.0, hardness=0.85))
    p.add(t, drumstick(hz("A4"), 1.5 - t, vol=0.4))
    p.add(t, lid_rattle(hz("E6"), 0.5, vol=0.2))
    return p.finish()


def three_kettles():
    """Three kettles of rising size, G, C and E, then all three together, ringing."""
    p = Piece(1.6)
    beat = 0.2
    for i, note in enumerate(("G4", "C5", "E5")):
        p.add(i * beat, drumstick(hz(note), 0.6, vol=0.75))
    t = 3 * beat
    for note, vol in (("C5", 0.7), ("E5", 0.55), ("G5", 0.65)):
        p.add(t, drumstick(hz(note), 1.6 - t, vol=vol, hardness=0.85))
    return p.finish()


def temple_gong():
    """A gong struck with a soft mallet: a dark hit that blooms brighter, then settles."""
    p = Piece(1.8)
    g = modes(hz("A3"), 1.8, GONG, hardness=0.8, attack=0.006, bloom=0.25)
    p.add(0.0, g / np.abs(g).max())
    p.add(0.0, band_noise(0.08, 300, 200, 0.02), vol=0.25)
    return p.finish(room=0.25)


def opera_gong():
    """A small gong whose pitch springs up after the hit, two quick ones and a big one."""
    p = Piece(1.5)
    for t, vol in ((0.0, 0.45), (0.16, 0.45)):
        g = modes(hz("D5"), 0.3, GONG[:8], attack=0.002, bend=0.06)
        p.add(t, g / np.abs(g).max(), vol=vol)
    g = modes(hz("A4"), 1.5 - 0.34, GONG[:10], attack=0.002, bend=0.09)
    p.add(0.34, g / np.abs(g).max())
    return p.finish()


def forge():
    """A smith's hammer on an anvil: two light taps, a heavy blow and sparks."""
    p = Piece(1.5)
    for t, vol in ((0.0, 0.4), (0.15, 0.45)):
        a = modes(hz("E6"), 0.35, ANVIL, attack=0.0004)
        p.add(t, a / np.abs(a).max(), vol=vol)
    t = 0.3
    a = modes(hz("B5"), 1.2, ANVIL, attack=0.0003)
    p.add(t, a / np.abs(a).max())
    a = modes(hz("E5"), 1.2, ANVIL, attack=0.0003)
    p.add(t, a / np.abs(a).max(), vol=0.5)
    for i in range(14):  # sparks: tiny bright clicks thinning out
        st = t + 0.02 + RNG.exponential(0.12)
        p.add(st, band_noise(0.01, RNG.uniform(5000, 9000), 1500, 0.002), vol=0.12)
    return p.finish()


def chip_kettle():
    """The chip sound and the kettle together: two pulse notes, then a kettle hit doubled
    by a pulse chord and a triangle bass."""
    p = Piece(1.5)
    for t, note in ((0.0, "G5"), (0.16, "C6")):
        p.add(t, pulse(note, 0.14, duty=0.25, vol=0.45, decay=0.1, sustain=0.5,
                       release=0.02))
        p.add(t, tri(note.replace("5", "3").replace("6", "4"), 0.14, vol=0.6, release=0.02))
    t = 0.32
    p.add(t, drumstick(hz("C5"), 1.5 - t, vol=1.0, hardness=0.9))
    p.add(t, lid_rattle(hz("G6"), 0.5, vol=0.15))
    for note in ("E6", "G6"):
        p.add(t, pulse(note, 1.5 - t, duty=0.25, vol=0.18, vibrato=0.2, vib_delay=0.15,
                       decay=0.4, sustain=0.3, release=0.3))
    p.add(t, tri("C4", 1.5 - t, vol=0.6, decay=0.5, sustain=0.4, release=0.3))
    return p.finish(room=0.12)


def pixel_gong():
    """A gong as an old console would play it from a sample: crunchy and low-fi, with a
    triangle bass note under it."""
    p = Piece(1.6)
    g = modes(hz("E4"), 1.6, GONG[:12], hardness=0.9, attack=0.002, bloom=0.12, bend=0.03)
    p.add(0.0, crush(g / np.abs(g).max(), 11025, 5))
    p.add(0.0, tri("E3", 1.0, vol=0.5, decay=0.4, sustain=0.2, release=0.3))
    p.add(0.0, noise(0.3, 20000, vol=0.15, decay=0.08))
    return p.finish(room=0.08)


JINGLES = [tetsubin_strike, tetsubin_beat, three_kettles, temple_gong, opera_gong, forge,
           chip_kettle, pixel_gong]

if __name__ == "__main__":
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for i, fn in enumerate(JINGLES, 17):
        samples = fn()
        path = out / f"{i}-{fn.__name__.replace('_', '-')}.wav"
        write(path, samples)
        rms = int(np.sqrt((samples ** 2).mean()))
        print(f"{path.name:24} {len(samples) / RATE:.2f} s rms {rms:5}  {fn.__doc__.split('.')[0]}")
