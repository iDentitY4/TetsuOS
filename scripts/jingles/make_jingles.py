#!/usr/bin/env python3
"""Original 8-bit style boot jingle candidates for Tetsu OS.

A tiny chip synth: band-limited pulse waves (PolyBLEP), a 4-bit stepped triangle like
the NES's, LFSR noise, envelopes stepped to 16 volume levels, and a 60 Hz "frame" for
arpeggios and vibrato. Writes 48 kHz 16-bit stereo WAVs peaking at 11000, like the
current jingle.
"""
import sys
import wave
from pathlib import Path

import numpy as np

RATE = 48000
PEAK = 11000
NAMES = {"C": 0, "C#": 1, "D": 2, "D#": 3, "E": 4, "F": 5, "F#": 6, "G": 7, "G#": 8,
         "A": 9, "A#": 10, "B": 11}


def hz(note):
    if isinstance(note, (int, float)):
        return float(note)
    name, octave = note[:-1], int(note[-1])
    return 440.0 * 2 ** ((NAMES[name] + 12 * (octave + 1) - 69) / 12)


def env(n, attack=0.002, decay=0.15, sustain=0.6, release=0.03, steps=True):
    t = np.arange(n) / RATE
    e = np.where(t < attack, t / max(attack, 1e-9),
                 sustain + (1 - sustain) * np.exp(-(t - attack) / max(decay, 1e-9)))
    rel = int(release * RATE)
    if rel and n > rel:
        e[-rel:] *= np.linspace(1, 0, rel)
    if steps:  # 16 volume levels, like a 4-bit channel volume
        e = np.ceil(e * 15) / 15
    return e


def freq_curve(n, f, slide_to=None, vibrato=0.0, vib_delay=0.12, arp=None):
    t = np.arange(n) / RATE
    if arp:  # cycle through semitone offsets every 60 Hz frame
        frame = (t * 60).astype(int) % len(arp)
        fr = f * 2 ** (np.array(arp)[frame] / 12)
    elif slide_to is not None:
        fr = f * (hz(slide_to) / f) ** (t / t[-1])
    else:
        fr = np.full(n, f)
    if vibrato:
        depth = np.clip((t - vib_delay) / 0.1, 0, 1) * vibrato
        fr = fr * 2 ** (depth * np.sin(2 * np.pi * 6 * t) / 12)
    return fr


def polyblep(phase, dt):
    out = np.zeros_like(phase)
    a = phase < dt
    x = phase[a] / dt[a]
    out[a] = x + x - x * x - 1
    b = phase > 1 - dt
    x = (phase[b] - 1) / dt[b]
    out[b] = x * x + x + x + 1
    return out


def pulse(note, dur, duty=0.5, duty_to=None, vol=1.0, **kw):
    n = int(dur * RATE)
    e_kw = {k: kw.pop(k) for k in ("attack", "decay", "sustain", "release") if k in kw}
    fr = freq_curve(n, hz(note), **kw)
    dt = fr / RATE
    phase = np.cumsum(dt) % 1.0
    d = np.full(n, duty) if duty_to is None else np.linspace(duty, duty_to, n)
    naive = np.where(phase < d, 1.0, -1.0)
    shifted = (phase - d) % 1.0
    sig = naive + polyblep(phase, dt) - polyblep(shifted, dt)
    sig -= 2 * d - 1  # remove the offset a narrow pulse has, as the console's output does
    return sig * env(n, **e_kw) * vol


def tri(note, dur, vol=1.0, **kw):
    n = int(dur * RATE)
    e_kw = {k: kw.pop(k) for k in ("attack", "decay", "sustain", "release") if k in kw}
    e_kw.setdefault("sustain", 1.0)
    fr = freq_curve(n, hz(note), **kw)
    phase = np.cumsum(fr / RATE) % 1.0
    wave_ = 4 * np.abs(phase - 0.5) - 1
    wave_ = np.round(wave_ * 7.5) / 7.5  # 4-bit stepped triangle
    return wave_ * env(n, steps=False, **e_kw) * vol


def noise(dur, rate_hz=12000, vol=1.0, short=False, decay=0.05, sustain=0.0, release=0.01):
    n = int(dur * RATE)
    reg, bits = 1, []
    count = int(dur * rate_hz) + 2
    tap = 6 if short else 1
    for _ in range(count):
        bit = (reg ^ (reg >> tap)) & 1
        reg = (reg >> 1) | (bit << 14)
        bits.append(1.0 if reg & 1 else -1.0)
    idx = (np.arange(n) * rate_hz / RATE).astype(int)
    return np.array(bits)[idx] * env(n, decay=decay, sustain=sustain, release=release) * vol


class Song:
    def __init__(self, seconds):
        self.buf = np.zeros(int(seconds * RATE))

    def add(self, t, sig):
        i = int(t * RATE)
        sig = sig[: max(0, len(self.buf) - i)]
        self.buf[i:i + len(sig)] += sig

    def kick(self, t, vol=0.9):
        # a fast downward triangle sweep plus a click of noise
        self.add(t, tri("A3", 0.12, vol=vol, slide_to="A1", decay=0.04, sustain=0.0))
        self.add(t, noise(0.02, 30000, vol=0.3, decay=0.01))

    def snare(self, t, vol=0.5):
        self.add(t, noise(0.14, 14000, vol=vol, decay=0.05))
        self.add(t, tri("E4", 0.05, vol=0.4, slide_to="B3", decay=0.02, sustain=0.0))

    def hat(self, t, vol=0.22):
        self.add(t, noise(0.04, 40000, vol=vol, decay=0.012))

    def crash(self, t, dur=0.8, vol=0.35):
        self.add(t, noise(dur, 36000, vol=vol, decay=0.25))

    def echo(self, delay, gain, repeats=2):
        out = self.buf.copy()
        d = int(delay * RATE)
        for r in range(1, repeats + 1):
            out[d * r:] += self.buf[: len(self.buf) - d * r] * gain ** r
        self.buf = out

    def finish(self, fade=0.08):
        f = int(fade * RATE)
        self.buf[-f:] *= np.linspace(1, 0, f)
        # a soft limiter, so a single drum hit doesn't set the loudness of the rest
        self.buf /= np.percentile(np.abs(self.buf), 99.5)
        self.buf = np.tanh(self.buf * 0.9)
        self.buf *= PEAK / np.abs(self.buf).max()
        return self.buf


def pulse_rush():
    """A two-octave arpeggio shooting up, then a C64-style cycling major chord."""
    s = Song(1.3)
    run = ["C5", "E5", "G5", "C6", "E6", "G6", "C7", "E7"]
    for i, note in enumerate(run):
        s.add(i * 0.035, pulse(note, 0.06, duty=0.25, vol=0.5, decay=0.03, sustain=0.3))
    t = len(run) * 0.035 + 0.02
    s.add(t, pulse("C6", 1.3 - t, duty=0.125, duty_to=0.45, vol=0.55, arp=[0, 4, 7, 12],
                   decay=0.4, sustain=0.25, release=0.3))
    s.add(t, pulse("G5", 1.3 - t, duty=0.5, vol=0.25, vibrato=0.3, decay=0.3,
                   sustain=0.3, release=0.3))
    s.add(t, tri("C4", 1.3 - t, vol=0.8, decay=0.5, sustain=0.3, release=0.3))
    s.add(0.0, tri("C3", 0.3, vol=0.6, slide_to="C4", decay=0.3, sustain=0.5))
    s.crash(t, 0.9, vol=0.15)
    return s.finish()


def turbo_start():
    """A rock riff at 170 bpm: drums, pumping bass and a lead that lands on A major."""
    s = Song(1.6)
    q = 60 / 170 / 4  # a sixteenth
    for i, step in enumerate("k.h.s.hkk.s.c"):
        t = i * q
        if step == "k":
            s.kick(t)
        elif step == "s":
            s.snare(t)
        elif step == "h":
            s.hat(t)
        elif step == "c":
            s.kick(t)
            s.crash(t, 0.9)
    bass = ["A3", "A3", "A4", "A3", "C4", "C4", "D4", "D4", "E4", "E4", "G4", "G#4"]
    for i, note in enumerate(bass):
        s.add(i * q, tri(note, q * 0.9, vol=0.7, release=0.01))
    s.add(12 * q, tri("A3", 0.6, vol=0.8, decay=0.3, sustain=0.4, release=0.2))
    lead = [("A4", 2), ("C5", 1), ("D5", 1), ("E5", 2), ("D5", 1), ("E5", 1),
            ("G5", 1), ("A5", 1), ("B5", 1), ("C#6", 1)]
    t = 0.0
    for note, steps in lead:
        s.add(t, pulse(note, steps * q * 0.95, duty=0.25, vol=0.45, decay=0.06, sustain=0.6,
                       release=0.01))
        t += steps * q
    for note, vol in (("A5", 0.45), ("E5", 0.3), ("C#5", 0.3)):
        s.add(t, pulse(note, 1.6 - t, duty=0.25 if note == "A5" else 0.5, vol=vol,
                       vibrato=0.25, vib_delay=0.15, decay=0.35, sustain=0.35, release=0.25))
    return s.finish()


def warp_up():
    """A pulse sweeping up three octaves, then sparkles with an echo and a bright chord."""
    s = Song(1.5)
    s.add(0.0, pulse("C4", 0.36, duty=0.5, duty_to=0.1, slide_to="C7", vol=0.5,
                     decay=1.0, sustain=1.0, release=0.01))
    s.add(0.0, noise(0.36, 8000, vol=0.08, decay=1.0, sustain=1.0))
    sparkle = ["G6", "D7", "B6", "E7", "C7", "G7"]
    for i, note in enumerate(sparkle):
        s.add(0.38 + i * 0.05, pulse(note, 0.08, duty=0.125, vol=0.3, decay=0.03, sustain=0.0))
    t = 0.7
    for note, vol in (("G6", 0.35), ("D6", 0.3), ("B5", 0.3)):
        s.add(t, pulse(note, 0.8, duty=0.25, vol=vol, vibrato=0.2, decay=0.25, sustain=0.3,
                       release=0.3))
    s.add(t, tri("G4", 0.8, vol=0.8, decay=0.4, sustain=0.3, release=0.3))
    s.echo(0.11, 0.4, repeats=3)
    return s.finish()


def hero_fanfare():
    """Three quick triplet hits and a held, heroic note, in thirds over a marching bass."""
    s = Song(1.7)
    tr = 0.075
    melody = [("D5", "B4", tr), ("D5", "B4", tr), ("D5", "B4", tr), ("G5", "D5", 0.3),
              ("F#5", "D5", tr * 2), ("G5", "E5", tr * 2)]
    t = 0.0
    for top, low, d in melody:
        s.add(t, pulse(top, d * 0.92, duty=0.5, vol=0.42, decay=0.08, sustain=0.65,
                       release=0.01))
        s.add(t, pulse(low, d * 0.92, duty=0.25, vol=0.3, decay=0.08, sustain=0.65,
                       release=0.01))
        t += d
    for note, low in (("G3", 0.0), ("D4", 3 * tr), ("C4", 3 * tr + 0.3), ("D4", t - 0.15)):
        s.add(low, tri(note, 0.14, vol=0.75, release=0.02))
    s.snare(0.0, 0.25)
    s.snare(tr, 0.2)
    s.snare(2 * tr, 0.25)
    s.kick(3 * tr)
    s.snare(t - 2 * tr * 2 + 0.0, 0.2)
    s.snare(t - 2 * tr, 0.3)
    s.add(t, pulse("B5", 1.7 - t, duty=0.5, vol=0.42, vibrato=0.3, vib_delay=0.1,
                   decay=0.4, sustain=0.45, release=0.35))
    s.add(t, pulse("G5", 1.7 - t, duty=0.25, vol=0.3, vibrato=0.3, vib_delay=0.1,
                   decay=0.4, sustain=0.45, release=0.35))
    s.add(t, tri("G3", 1.7 - t, vol=0.85, decay=0.6, sustain=0.4, release=0.35))
    s.kick(t)
    s.crash(t, 1.0, vol=0.25)
    return s.finish()


def level_up():
    """A ladder of rising triplet arpeggios, each a step higher, to a ringing top."""
    s = Song(1.4)
    steps = [["G5", "B5", "D6"], ["A5", "C#6", "E6"], ["B5", "D#6", "F#6"]]
    t = 0.0
    for group in steps:
        for note in group:
            s.add(t, pulse(note, 0.07, duty=0.25, vol=0.45, decay=0.04, sustain=0.4,
                           release=0.005))
            t += 0.055
        t += 0.03
    for i, note in enumerate(("G3", "A3", "B3")):
        s.add(i * 0.195, tri(note, 0.17, vol=0.7, release=0.02))
        s.hat(i * 0.195 + 0.11)
    s.add(t, pulse("C7", 1.4 - t, duty=0.125, vol=0.4, arp=[0, 0, 4, 4, 7, 7, 12, 12],
                   decay=0.3, sustain=0.2, release=0.3))
    s.add(t, pulse("G6", 1.4 - t, duty=0.5, vol=0.25, vibrato=0.25, decay=0.3, sustain=0.3,
                   release=0.3))
    s.add(t, tri("C4", 1.4 - t, vol=0.8, decay=0.4, sustain=0.35, release=0.3))
    s.crash(t, 0.8, vol=0.15)
    return s.finish()


def neon_drive():
    """Synthwave-ish: octave-jumping bass, offbeat hats, PWM chord stabs, final lift."""
    s = Song(1.6)
    q = 60 / 150 / 4
    bass = ["E3", "E4"] * 4 + ["C3", "C4"] * 2 + ["D3", "D4"] * 2
    for i, note in enumerate(bass[:12]):
        s.add(i * q, pulse(note, q * 0.8, duty=0.5, vol=0.4, decay=0.05, sustain=0.5,
                           release=0.005))
        if i % 2:
            s.hat(i * q)
    for i in (0, 4, 8):
        s.kick(i * q)
    s.snare(4 * q, 0.35)
    s.snare(10 * q, 0.3)
    for i, chord in ((2, ("E5", "G5", "B5")), (6, ("E5", "G5", "B5")), (9, ("E5", "A5", "C6"))):
        for note in chord:
            s.add(i * q, pulse(note, q * 1.6, duty=0.15, duty_to=0.45, vol=0.22, decay=0.06,
                               sustain=0.3, release=0.02))
    t = 12 * q
    s.kick(t)
    s.crash(t, 0.9, vol=0.22)
    s.add(t, pulse("E4", 1.6 - t, duty=0.5, vol=0.45, decay=0.4, sustain=0.4, release=0.3))
    for note in ("D#5", "F#5", "B5", "E6"):
        s.add(t, pulse(note, 1.6 - t, duty=0.1, duty_to=0.5, vol=0.2, vibrato=0.15,
                       decay=0.4, sustain=0.35, release=0.3))
    s.echo(q * 3, 0.3, repeats=2)
    return s.finish()


def bubble_pop():
    """Playful: staccato blips that drop in pitch, a trill and a pop at the top."""
    s = Song(1.2)
    notes = ["C6", "D6", "E6", "G6", "A6", "C7", "A6", "C7"]
    for i, note in enumerate(notes):
        f = hz(note)
        s.add(i * 0.065, pulse(f * 1.06, 0.05, duty=0.125, slide_to=f * 0.94, vol=0.45,
                               decay=0.02, sustain=0.2, release=0.005))
    for i, note in enumerate(("C4", "G3", "A3", "E3")):
        s.add(i * 0.13, tri(note, 0.1, vol=0.65, release=0.02))
    t = len(notes) * 0.065
    for i in range(8):
        s.add(t + i * 0.03, pulse("D7" if i % 2 else "E7", 0.03, duty=0.25, vol=0.3,
                                  decay=1, sustain=1, release=0.003))
    t += 0.25
    s.add(t, pulse("C8", 0.12, duty=0.5, slide_to="C6", vol=0.35, decay=0.05, sustain=0.0))
    s.add(t, pulse("C6", 1.2 - t, duty=0.25, vol=0.35, arp=[0, 7, 12, 16], decay=0.2,
                   sustain=0.25, release=0.25))
    s.add(t, tri("C4", 1.2 - t, vol=0.7, decay=0.3, sustain=0.3, release=0.25))
    s.echo(0.09, 0.3, repeats=2)
    return s.finish()


def power_zap():
    """A laser zap, three hard chord stabs and a big final chord with a crash."""
    s = Song(1.6)
    s.add(0.0, pulse("C7", 0.2, duty=0.25, slide_to="C4", vol=0.4, decay=0.15, sustain=0.0))
    s.add(0.0, noise(0.2, 24000, vol=0.15, decay=0.08))
    stabs = [(0.24, ("D5", "F#5", "A5"), "D3"), (0.39, ("D5", "F#5", "A5"), "D3"),
             (0.54, ("E5", "G5", "C6"), "C3")]
    for t, chord, root in stabs:
        for note in chord:
            s.add(t, pulse(note, 0.1, duty=0.5, vol=0.25, decay=0.05, sustain=0.4,
                           release=0.01))
        s.add(t, tri(root.replace("3", "4"), 0.1, vol=0.75, release=0.01))
        s.kick(t)
    s.snare(0.69, 0.35)
    s.snare(0.765, 0.4)
    t = 0.84
    for note in ("D5", "F#5", "A5", "D6"):
        s.add(t, pulse(note, 1.6 - t, duty=0.5 if note == "D6" else 0.25, vol=0.25,
                       vibrato=0.2, vib_delay=0.15, decay=0.4, sustain=0.35, release=0.35))
    s.add(t, tri("D4", 1.6 - t, vol=0.85, decay=0.5, sustain=0.35, release=0.35))
    s.kick(t)
    s.crash(t, 0.9, vol=0.3)
    return s.finish()


JINGLES = [pulse_rush, turbo_start, warp_up, hero_fanfare, level_up, neon_drive,
           bubble_pop, power_zap]


def write(path, samples):
    data = np.repeat(samples.astype(np.int16), 2)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(data.tobytes())


if __name__ == "__main__":
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for i, fn in enumerate(JINGLES, 1):
        samples = fn()
        path = out / f"{i}-{fn.__name__.replace('_', '-')}.wav"
        write(path, samples)
        print(f"{path.name:24} {len(samples) / RATE:.2f} s  {fn.__doc__.strip()}")
