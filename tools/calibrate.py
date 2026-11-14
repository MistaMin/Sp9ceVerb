#!/usr/bin/env python3
"""Fit a voicing's early gain, tail gain and tail side ratio to level targets
(wet energy of a unit impulse, C80, side-minus-mid). The targets are the design
brief recorded in docs/PROVENANCE.md."""
import os, subprocess, sys
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from measure import read_wav

# mode: (energy_db L, c80_db, side_minus_mid_db) at default settings
TARGET = {
    "room": (-7.63, 8.63, -24.82), "hall": (-4.66, 1.61, -14.75), "plate": (-2.19, -0.70, -21.07),
    "chamber": (0.11, -0.69, -22.04), "concert_hall": (-12.39, 11.05, -25.68),
    "random_hall": (-7.08, 1.94, -14.55), "vintage_plate": (-3.97, -0.59, -20.53),
}
TMP = os.path.join(HERE, "runs", "_cal")
os.makedirs(TMP, exist_ok=True)

def render(mode, e, t, s):
    wav = os.path.join(TMP, "x.wav")
    subprocess.run([os.path.join(HERE, "..", "target", "release", "examples", "probe"), mode, wav, "seconds=8",
                    f"cal_early={e}", f"cal_tail={t}", f"cal_side={s}"], check=True)
    sr, l, r = read_wav(wav)
    return sr, l, r

def metrics(sr, l, r):
    k = int(0.08 * sr)
    e = l * l + r * r
    E = 10 * np.log10((l * l).sum())
    c80 = 10 * np.log10(e[:k].sum() / e[k:].sum())
    m, sd = 0.5 * (l + r), 0.5 * (l - r)
    return E, c80, 10 * np.log10((sd * sd).sum() / (m * m).sum())

for mode, (tE, tC, tS) in TARGET.items():
    side = 0.3
    for _ in range(5):
        sr, fl, fr = render(mode, 0, 0, side)
        _, al, ar = render(mode, 1, 0, side)
        _, bl, br = render(mode, 0, 1, side)
        al, ar, bl, br = al - fl, ar - fr, bl - fl, br - fr
        best = None
        ge, gt = 0.2, 0.3
        for span in (1.0, 0.3, 0.1, 0.03, 0.01):   # coarse-to-fine log search
            for a in np.linspace(-span, span, 21):
                for b in np.linspace(-span, span, 21):
                    e_, t_ = ge * 10 ** a, gt * 10 ** b
                    E, c, _s = metrics(sr, fl + e_ * al + t_ * bl, fr + e_ * ar + t_ * br)
                    err = (E - tE) ** 2 + (c - tC) ** 2
                    if best is None or err < best[0]:
                        best = (err, e_, t_)
            ge, gt = best[1], best[2]
        E, c, s_db = metrics(sr, fl + ge * al + gt * bl, fr + ge * ar + gt * br)
        side = float(np.clip(side * 10 ** ((tS - s_db) / 20), 0.0, 1.5))
    print(f"{mode:14} early_gain: {ge:.4f}, tail_gain: {gt:.4f}, side: {side:.4f}   -> E {E:.2f} (target {tE})  C80 {c:.2f} ({tC})  side {s_db:.2f} ({tS})")
