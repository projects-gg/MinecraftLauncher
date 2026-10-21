"""Synthesizes the overlay's two interface sounds (our own, no samples) and encodes them for the game.

    python tools/build-sounds.py <work dir> src/main/resources/assets/projects_overlay/sounds/ui
    (needs ffmpeg with libvorbis on PATH)

- message: one soft mallet note (B5) that dies away in about a third of a second. Quiet by design: it
  comes with every new message.
- call: two notes a fifth apart (G5, then D6), the launcher's "success" interval, a little longer and
  brighter, for crew calls and "Birlikte oyna" invites.

A note is a few sine partials with their own decays (a vibraphone-like bar: the fundamental, a soft
octave and a quick fourth partial that gives the mallet's tap), a 3 ms rounded attack and no reverb, so
it stays clean under the game's own sounds.
"""
import math
import struct
import subprocess
import sys
import wave
from pathlib import Path

RATE = 44100

# (frequency ratio, amplitude, decay time constant as a fraction of the note's)
PARTIALS = [(1.0, 1.0, 1.0), (2.0, 0.16, 0.45), (4.0, 0.10, 0.16), (6.27, 0.025, 0.08)]


def note(buffer, start, freq, tau, amp):
    attack = int(0.003 * RATE)
    length = int(tau * 7 * RATE)
    for n in range(length):
        i = start + n
        if i >= len(buffer):
            break
        t = n / RATE
        env_a = 0.5 - 0.5 * math.cos(math.pi * n / attack) if n < attack else 1.0
        s = 0.0
        for ratio, a, decay in PARTIALS:
            s += a * math.exp(-t / (tau * decay)) * math.sin(2 * math.pi * freq * ratio * t)
        buffer[i] += amp * env_a * s


def render(notes, seconds, peak):
    buffer = [0.0] * int(seconds * RATE)
    for start, freq, tau, amp in notes:
        note(buffer, int(start * RATE), freq, tau, amp)
    top = max(abs(v) for v in buffer) or 1.0
    gain = peak / top
    fade = int(0.02 * RATE)
    for i in range(fade):
        buffer[-1 - i] *= i / fade
    return [v * gain for v in buffer]


def write_wav(path, samples):
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b"".join(struct.pack("<h", max(-32767, min(32767, int(v * 32767)))) for v in samples))


def main():
    work = Path(sys.argv[1])
    out = Path(sys.argv[2])
    work.mkdir(parents=True, exist_ok=True)
    out.mkdir(parents=True, exist_ok=True)
    sounds = {
        # -15 dBFS peak: one note, quiet.
        "message": render([(0.0, 987.77, 0.085, 1.0)], 0.55, 0.18),
        # -12 dBFS peak: two notes, the second a fifth up and held longer.
        "call": render([(0.0, 783.99, 0.07, 0.8), (0.12, 1174.66, 0.11, 1.0)], 0.95, 0.25),
    }
    for name, samples in sounds.items():
        wav = work / f"{name}.wav"
        write_wav(wav, samples)
        ogg = out / f"{name}.ogg"
        subprocess.run(
            ["ffmpeg", "-loglevel", "error", "-y", "-i", str(wav), "-ac", "1", "-c:a", "libvorbis", "-q:a", "5",
             "-map_metadata", "-1", str(ogg)],
            check=True,
        )
        print(f"{ogg.name}: {ogg.stat().st_size} bytes")


if __name__ == "__main__":
    main()
