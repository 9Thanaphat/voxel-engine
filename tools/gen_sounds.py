#!/usr/bin/env python3
"""สังเคราะห์ไฟล์เสียง placeholder (.wav 16-bit mono) ลง assets/sounds/

เสียงไม่สวยหรอก แค่ให้ระบบเสียงเดินได้ทันทีโดยไม่ต้องไปหา asset —
พอได้เสียง CC0 จริง (Kenney.nl / freesound CC0) ก็วางทับชื่อเดิมได้เลย

รัน:  python tools/gen_sounds.py
"""
import math
import os
import random
import struct
import wave

RATE = 44100
OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "assets", "sounds")


def write_wav(name, samples):
    os.makedirs(OUT_DIR, exist_ok=True)
    path = os.path.join(OUT_DIR, name)
    # clamp + 16-bit
    frames = bytearray()
    for s in samples:
        v = max(-1.0, min(1.0, s))
        frames += struct.pack("<h", int(v * 32767))
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(bytes(frames))
    print(f"  {name}  ({len(samples)/RATE*1000:.0f} ms)")


def n(count):
    return int(RATE * count)


def env(i, total, attack=0.005, decay=None):
    """attack เชิงเส้นสั้นๆ + exp decay"""
    t = i / RATE
    a = min(1.0, t / attack) if attack > 0 else 1.0
    if decay is None:
        decay = (total / RATE)
    d = math.exp(-t / (decay * 0.35))
    return a * d


def lowpass(samples, cutoff):
    """one-pole lowpass, cutoff 0..1 (สูง = ผ่านสูง)"""
    y = 0.0
    out = []
    for x in samples:
        y += cutoff * (x - y)
        out.append(y)
    return out


def noise_burst(dur, cutoff, decay, hp=False):
    total = n(dur)
    raw = [random.uniform(-1, 1) for _ in range(total)]
    filt = lowpass(raw, cutoff)
    if hp:  # highpass = raw - lowpass
        filt = [raw[i] - filt[i] for i in range(total)]
    return [filt[i] * env(i, total, decay=decay) for i in range(total)]


def tone(freq, dur, decay, amp=1.0):
    total = n(dur)
    return [amp * math.sin(2 * math.pi * freq * i / RATE) * env(i, total, decay=decay)
            for i in range(total)]


def mix(*layers):
    m = max(len(l) for l in layers)
    out = [0.0] * m
    for l in layers:
        for i, v in enumerate(l):
            out[i] += v
    return out


def scale(samples, g):
    return [s * g for s in samples]


def build():
    random.seed(42)
    # ---- ทุบ/วางบล็อก (ใช้เสียงเดียวกันทั้งทุบและวาง) ----
    write_wav("dig_stone.wav", scale(noise_burst(0.13, 0.55, 0.09), 0.8))
    write_wav("dig_dirt.wav",  scale(noise_burst(0.15, 0.18, 0.11), 0.85))
    write_wav("dig_grass.wav", scale(noise_burst(0.10, 0.9, 0.07, hp=True), 0.5))
    write_wav("dig_wood.wav",  scale(mix(noise_burst(0.12, 0.4, 0.08),
                                         tone(180, 0.12, 0.05, 0.5)), 0.8))
    # แก้วแตก: noise สว่าง + tinkle หลายความถี่สูง
    glass = mix(scale(noise_burst(0.22, 0.95, 0.10, hp=True), 0.5),
                tone(2600, 0.20, 0.08, 0.25),
                tone(3700, 0.18, 0.06, 0.18),
                tone(5200, 0.15, 0.05, 0.12))
    write_wav("dig_glass.wav", scale(glass, 0.8))

    # ---- ก้าวเดิน (เบา สั้น) ----
    write_wav("step_stone.wav", scale(noise_burst(0.07, 0.5, 0.05), 0.35))
    write_wav("step_soft.wav",  scale(noise_burst(0.08, 0.2, 0.06), 0.30))
    write_wav("step_wood.wav",  scale(mix(noise_burst(0.06, 0.45, 0.04),
                                          tone(200, 0.06, 0.03, 0.3)), 0.32))

    # ---- อื่นๆ ----
    write_wav("land.wav", scale(mix(noise_burst(0.12, 0.25, 0.08),
                                    tone(90, 0.12, 0.06, 0.6)), 0.6))

    # splash: noise ลู่ต่ำ + amplitude modulation ให้เหมือนน้ำ
    total = n(0.40)
    raw = [random.uniform(-1, 1) for _ in range(total)]
    lp = lowpass(raw, 0.35)
    splash = []
    for i in range(total):
        wobble = 0.7 + 0.3 * math.sin(2 * math.pi * 18 * i / RATE)
        splash.append(lp[i] * env(i, total, attack=0.002, decay=0.14) * wobble)
    write_wav("splash.wav", scale(splash, 0.6))

    # rain_loop: noise กรอง lowpass ยาว 2 วิ ต่อ loop ได้ (amplitude เกือบคงที่ กันคลิกตรงรอยต่อ)
    total = n(2.0)
    raw = [random.uniform(-1, 1) for _ in range(total)]
    lp = lowpass(lowpass(raw, 0.25), 0.25)
    rain = []
    for i in range(total):
        # เฟดหัว-ท้ายนิดเดียวให้ต่อ loop เนียน
        edge = min(1.0, i / (RATE * 0.03), (total - i) / (RATE * 0.03))
        wob = 0.85 + 0.15 * math.sin(2 * math.pi * 7 * i / RATE)
        rain.append(lp[i] * edge * wob)
    write_wav("rain_loop.wav", scale(rain, 0.9))

    # explosion: rumble sine ต่ำ + noise burst หัว + clip เบาๆ
    total = n(0.85)
    boom = []
    for i in range(total):
        t = i / RATE
        low = math.sin(2 * math.pi * (70 - 30 * t) * t)  # sweep ลง
        nz = random.uniform(-1, 1) * math.exp(-t / 0.06)  # crack หัว
        e = math.exp(-t / 0.30)
        s = (low * 0.9 + nz * 0.8) * e
        boom.append(max(-1.0, min(1.0, s * 1.4)))  # clip = distortion
    write_wav("explosion.wav", scale(boom, 0.9))


if __name__ == "__main__":
    print("generating placeholder sounds ->", os.path.normpath(OUT_DIR))
    build()
    print("done.")
