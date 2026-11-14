#!/usr/bin/env python3
"""Black-box measurement runner.

    python3 measure.py <tag> <product> <label> [key=value ...]
    python3 measure.py list <product>

Renders the product through bin/probe with the given keys, analyses the
output, appends one JSON line (inputs + metrics) to logs/<tag>.jsonl and
prints the same record.  The record is the only thing that is kept; pass
keep=1 to keep the WAV under runs/<tag>/<label>.wav as well.

Extra keys handled here (everything else goes to the probe):
    keep=1   keep the rendered WAV
    tv=1     time-variance test: render a second time with a longer
             pre-roll and report how similar the two responses are
    full=1   print the full record instead of the short summary
"""
import json
import os
import struct
import subprocess
import sys
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
PROBE = os.environ.get("PROBE") or os.path.join(HERE, "..", "target", "release", "examples", "probe")
OCTAVES = [63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]


def read_wav(path):
    with open(path, "rb") as f:
        b = f.read()
    sr = struct.unpack_from("<I", b, 24)[0]
    x = np.frombuffer(b, dtype="<f4", offset=44).reshape(-1, 2).astype(np.float64)
    return sr, x[:, 0].copy(), x[:, 1].copy()


def db(v, floor=-200.0):
    v = float(v)
    return round(max(floor, 10.0 * np.log10(v)) if v > 0 else floor, 2)


def band(x, sr, lo, hi):
    # Zero-pad so the filter's ringing cannot wrap around into the tail.
    keep = len(x)
    n = 2 * keep
    X = np.fft.rfft(x, n)
    f = np.fft.rfftfreq(n, 1.0 / sr)
    m = np.zeros_like(f)
    inside = (f >= lo) & (f <= hi)
    m[inside] = 1.0
    # raised-cosine skirts a sixth of an octave wide
    for edge, sign in ((lo, -1), (hi, 1)):
        a, c = (edge * 2 ** (-1 / 6), edge) if sign < 0 else (edge, edge * 2 ** (1 / 6))
        sk = (f > a) & (f < c)
        ph = (f[sk] - a) / (c - a)
        m[sk] = 0.5 - 0.5 * np.cos(np.pi * ph) if sign < 0 else 0.5 + 0.5 * np.cos(np.pi * ph)
    return np.fft.irfft(X * m, n)[:keep]


def decay(x, sr):
    """Schroeder decay metrics of one signal."""
    e = x * x
    tot = e.sum()
    if tot <= 1e-30:
        return {"energy_db": -200.0}
    edc = np.cumsum(e[::-1])[::-1] / tot
    edc_db = 10 * np.log10(np.maximum(edc, 1e-30))
    t = np.arange(len(x)) / sr

    def slope(hi, lo):
        idx = np.where((edc_db <= hi) & (edc_db >= lo))[0]
        if len(idx) < 16 or edc_db[-1] > lo - 3 and idx[-1] >= len(x) - 2:
            return None
        p = np.polyfit(t[idx], edc_db[idx], 1)
        return round(float(-60.0 / p[0]), 4) if p[0] < 0 else None

    out = {
        "energy_db": db(tot),
        "edt": slope(0, -10),
        "t20": slope(-5, -25),
        "t30": slope(-5, -35),
        "t60_late": slope(-20, -50),
    }
    # how far the captured response actually decayed (truncation indicator)
    tail = e[-max(1, len(e) // 50):].mean()
    head = e.max()
    out["end_level_db"] = db(tail / head) if head > 0 else None
    return out


def echo_density(x, sr, times_ms):
    w = int(0.02 * sr)
    out = {}
    prof = []
    hop = w // 2
    for start in range(0, max(1, len(x) - w), hop):
        seg = x[start:start + w]
        sd = seg.std()
        if sd <= 1e-12:
            prof.append(0.0)
            continue
        prof.append(float((np.abs(seg) > sd).mean() / 0.3173))
    prof = np.array(prof)
    for ms in times_ms:
        i = int(ms / 1000 * sr / hop)
        out[str(ms)] = round(float(prof[i]), 3) if i < len(prof) else None
    mix = np.where(prof >= 0.9)[0]
    out["mixing_time_ms"] = round(float(mix[0] * hop / sr * 1000), 1) if len(mix) else None
    return out


def taps(x, sr, window_s=0.6, count=40, rel_db=-50.0):
    n = min(len(x), int(window_s * sr))
    a = np.abs(x[:n])
    pk = np.abs(x).max()
    if pk <= 0:
        return []
    thr = pk * 10 ** (rel_db / 20)
    idx = np.where((a[1:-1] >= a[:-2]) & (a[1:-1] > a[2:]) & (a[1:-1] > thr))[0] + 1
    if n > 0 and a[0] > thr and (n < 2 or a[0] > a[1]):
        idx = np.concatenate(([0], idx))
    idx = idx[np.argsort(-a[idx])][:count]
    idx.sort()
    return [[round(i / sr * 1000, 4), int(i), round(float(x[i]), 6)] for i in idx]


def third_octave(x, sr):
    n = len(x)
    P = np.abs(np.fft.rfft(x)) ** 2
    f = np.fft.rfftfreq(n, 1.0 / sr)
    out = {}
    fc = 25.0
    while fc < min(20000, sr / 2.2):
        lo, hi = fc * 2 ** (-1 / 6), fc * 2 ** (1 / 6)
        sel = (f >= lo) & (f < hi)
        out[str(int(round(fc)))] = db(P[sel].sum() / n) if sel.any() else None
        fc *= 2 ** (1 / 3)
    return out


def envelope(x, sr, frame_s=0.02, max_frames=400):
    w = int(frame_s * sr)
    k = min(len(x) // w, max_frames)
    return [db((x[i * w:(i + 1) * w] ** 2).mean()) for i in range(k)]


def corr(a, b):
    d = np.sqrt((a * a).sum() * (b * b).sum())
    return round(float((a * b).sum() / d), 4) if d > 1e-30 else None


def analytic(x):
    n = len(x)
    X = np.fft.fft(x)
    h = np.zeros(n)
    h[0] = 1
    h[1:(n + 1) // 2] = 2
    if n % 2 == 0:
        h[n // 2] = 1
    return np.fft.ifft(X * h)


def tone_metrics(l, sr, freq, dur):
    out = {}
    on = int(dur * sr)
    ss = l[max(0, on - int(0.1 * sr)):on]
    out["steady_rms_db"] = db((ss ** 2).mean()) if len(ss) else None
    tail = l[on + int(0.05 * sr):]
    if len(tail) < sr // 4:
        return out
    bw = band(tail, sr, freq * 0.8, freq * 1.25)
    z = analytic(bw)
    mag = np.abs(z)
    ref = mag[: sr // 10].max() if len(mag) > sr // 10 else mag.max()
    if ref <= 0:
        return out
    ok = np.where(mag > ref * 10 ** (-40 / 20))[0]
    end = int(ok[-1]) if len(ok) else 0
    end = min(end, len(z) - 1)
    if end < sr // 4:
        out["tail_too_short_for_modulation"] = True
        return out
    ph = np.unwrap(np.angle(z[:end]))
    inst = np.diff(ph) * sr / (2 * np.pi)
    # smooth to 5 ms to remove beating spikes
    k = max(1, int(0.005 * sr))
    inst = np.convolve(inst, np.ones(k) / k, mode="valid")[::k]
    wgt = mag[:end - 1][: len(inst) * k : k][: len(inst)]
    inst = inst[: len(wgt)]
    good = wgt > np.median(wgt) * 0.3
    cents = 1200 * np.log2(np.maximum(inst[good], 1.0) / freq)
    out["tail_freq_mean_cents"] = round(float(np.mean(cents)), 3)
    out["tail_freq_std_cents"] = round(float(np.std(cents)), 3)
    out["tail_freq_p2p_cents"] = round(float(np.percentile(cents, 98) - np.percentile(cents, 2)), 3)
    # amplitude-envelope modulation spectrum (detrended log envelope)
    step = k / sr
    env = 20 * np.log10(np.maximum(wgt, 1e-12))
    tt = np.arange(len(env)) * step
    env = env - np.polyval(np.polyfit(tt, env, 1), tt)
    if len(env) > 64:
        S = np.abs(np.fft.rfft(env * np.hanning(len(env))))
        fr = np.fft.rfftfreq(len(env), step)
        sel = (fr > 0.1) & (fr < 40)
        if sel.any():
            top = np.argsort(-S[sel])[:3]
            out["env_mod_peaks_hz"] = [round(float(fr[sel][i]), 3) for i in top]
        out["env_ripple_db_rms"] = round(float(env.std()), 3)
    # spectral spread of the tail around the tone
    P = np.abs(np.fft.rfft(tail[:end] * np.hanning(end))) ** 2
    fr = np.fft.rfftfreq(end, 1.0 / sr)
    sel = (fr > freq * 0.9) & (fr < freq * 1.1)
    if sel.any() and P[sel].sum() > 0:
        c = (fr[sel] * P[sel]).sum() / P[sel].sum()
        s = np.sqrt((((fr[sel] - c) ** 2) * P[sel]).sum() / P[sel].sum())
        out["tail_spectral_centroid_hz"] = round(float(c), 3)
        out["tail_spectral_spread_hz"] = round(float(s), 4)
    return out


def analyse(path, keys):
    sr, l, r = read_wav(path)
    excite = keys.get("excite", "impulse")
    m = {"sr": sr, "samples": len(l)}
    bad = int((~np.isfinite(l)).sum() + (~np.isfinite(r)).sum())
    m["non_finite_samples"] = bad
    if bad:
        l = np.nan_to_num(l)
        r = np.nan_to_num(r)
    m["peak"] = [round(float(np.abs(l).max()), 6), round(float(np.abs(r).max()), 6)]
    m["energy_db"] = [db((l * l).sum()), db((r * r).sum())]
    m["dc"] = [round(float(l.mean()), 7), round(float(r.mean()), 7)]
    mid, side = 0.5 * (l + r), 0.5 * (l - r)
    m["lr_corr"] = corr(l, r)
    k200 = int(0.2 * sr)
    m["lr_corr_late"] = corr(l[k200:], r[k200:])
    es, em = (side * side).sum(), (mid * mid).sum()
    m["side_minus_mid_db"] = round(db(es) - db(em), 2) if em > 0 and es > 0 else None
    m["env_db_20ms"] = envelope(mid if em > 0 else l, sr)

    if excite.startswith("impulse"):
        pk = max(np.abs(l).max(), np.abs(r).max())
        if pk > 0:
            first = np.where(np.maximum(np.abs(l), np.abs(r)) > pk * 1e-3)[0]
            m["first_arrival_ms"] = round(float(first[0] / sr * 1000), 4)
            m["peak_time_ms"] = [round(float(np.abs(l).argmax() / sr * 1000), 4),
                                 round(float(np.abs(r).argmax() / sr * 1000), 4)]
        m["taps_l"] = taps(l, sr)
        m["taps_r"] = taps(r, sr)
        m["decay_l"] = decay(l, sr)
        m["decay_r"] = decay(r, sr)
        m["bands"] = {}
        for fc in OCTAVES:
            if fc * 1.5 > sr / 2:
                continue
            m["bands"][str(fc)] = decay(band(l, sr, fc / 2 ** 0.5, fc * 2 ** 0.5), sr)
        m["echo_density_l"] = echo_density(l, sr, [10, 20, 40, 80, 160, 320, 640])
        m["third_octave_db_l"] = third_octave(l, sr)
        k80 = int(0.08 * sr)
        e = l * l + r * r
        if e.sum() > 0:
            m["c80_db"] = round(db(e[:k80].sum()) - db(e[k80:].sum()), 2) if e[k80:].sum() > 0 else None
            m["centre_time_ms"] = round(float((np.arange(len(e)) * e).sum() / e.sum() / sr * 1000), 3)
        m["early_spectrum_db"] = {str(fc): db((band(l[:k80], sr, fc / 2 ** 0.5, fc * 2 ** 0.5) ** 2).sum())
                                  for fc in OCTAVES if fc * 1.5 < sr / 2}
        m["late_spectrum_db"] = {str(fc): db((band(l[k200:], sr, fc / 2 ** 0.5, fc * 2 ** 0.5) ** 2).sum())
                                 for fc in OCTAVES if fc * 1.5 < sr / 2}
    elif excite == "tone":
        m["tone"] = tone_metrics(l, sr, float(keys.get("freq", 1000)), float(keys.get("dur", 0.5)))
    else:
        on = int(float(keys.get("dur", 0.5)) * sr)
        m["decay_after_stop_l"] = decay(l[on:], sr)
        m["bands_after_stop"] = {str(fc): decay(band(l[on:], sr, fc / 2 ** 0.5, fc * 2 ** 0.5), sr)
                                 for fc in OCTAVES if fc * 1.5 < sr / 2}
        m["third_octave_db_l"] = third_octave(l, sr)
    return m, (sr, l, r)


def run_probe(product, wav, keys):
    args = [PROBE, product, wav] + [f"{k}={v}" for k, v in keys.items()]
    p = subprocess.run(args, capture_output=True, text=True)
    if p.returncode != 0:
        sys.exit(f"probe failed: {p.stderr.strip()}")
    return p.stderr.strip()


def main():
    a = sys.argv[1:]
    if len(a) == 2 and a[0] == "list":
        sys.stdout.write(subprocess.run([PROBE, "list", a[1]], capture_output=True, text=True).stdout)
        return
    if len(a) < 3:
        sys.exit(__doc__)
    tag, product, label = a[0], a[1], a[2]
    keys = {}
    for kv in a[3:]:
        if "=" not in kv:
            sys.exit(f"bad argument {kv!r}")
        k, v = kv.split("=", 1)
        keys[k] = v
    keep = keys.pop("keep", "0") == "1"
    tv = keys.pop("tv", "0") == "1"
    full = keys.pop("full", "0") == "1"

    rundir = os.path.join(HERE, "runs", tag)
    os.makedirs(rundir, exist_ok=True)
    os.makedirs(os.path.join(HERE, "logs"), exist_ok=True)
    wav = os.path.join(rundir, f"{label}.wav")
    warn = run_probe(product, wav, keys)
    metrics, (sr, l, r) = analyse(wav, keys)

    if tv:
        k2 = dict(keys)
        k2["preroll"] = str(float(keys.get("preroll", 2)) + 0.3713)
        wav2 = os.path.join(rundir, f"{label}.tv.wav")
        run_probe(product, wav2, k2)
        _, l2, _r2 = read_wav(wav2)
        os.remove(wav2)
        res = {"whole": corr(l, l2)}
        for a_ms, b_ms in ((0, 50), (50, 200), (200, 500), (500, 1000), (1000, 2000), (2000, 4000)):
            i, j = int(a_ms / 1000 * sr), int(b_ms / 1000 * sr)
            if j <= len(l):
                res[f"{a_ms}-{b_ms}ms"] = corr(l[i:j], l2[i:j])
        metrics["time_variance_corr"] = res

    if not keep:
        os.remove(wav)
    rec = {
        "ts": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "tag": tag,
        "product": product,
        "label": label,
        "keys": keys,
        "metrics": metrics,
    }
    if warn:
        rec["probe_warning"] = warn
    with open(os.path.join(HERE, "logs", f"{tag}.jsonl"), "a") as f:
        f.write(json.dumps(rec, separators=(",", ":")) + "\n")

    if full:
        print(json.dumps(rec, indent=1))
        return
    s = {k: v for k, v in metrics.items()
         if k not in ("env_db_20ms", "third_octave_db_l", "taps_l", "taps_r", "bands", "bands_after_stop")}
    if "bands" in metrics:
        s["band_t30"] = {k: v.get("t30") for k, v in metrics["bands"].items()}
        s["band_edt"] = {k: v.get("edt") for k, v in metrics["bands"].items()}
        s["taps_l_first12"] = metrics["taps_l"][:12]
        s["taps_r_first12"] = metrics["taps_r"][:12]
    s["env_db_every_100ms"] = metrics["env_db_20ms"][::5][:40]
    if warn:
        s["probe_warning"] = warn
    print(json.dumps({"label": label, "keys": keys, "summary": s}))


if __name__ == "__main__":
    main()
