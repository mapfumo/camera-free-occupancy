"""Model input encoding — the one place the ToF invalid-zone rule lives.

Decided 2026-11-23 (NOTES, "Invalid ToF zones: the encoding"). Each frame
becomes a 3x8x8 float32 array, row-major in the world frame of
sessions/README.md:

  channel 0  ToF valid mask   1.0 if status is 5 or 9 AND distance > 0 mm,
                              else 0.0. Status 255 (no target), 10, 4 and
                              the rest are invalid; so is a 0 mm reading,
                              whatever its status.
  channel 1  ToF distance     valid distance clipped to D_MAX_MM, scaled to
                              0..1. Invalid zones are filled with 1.0 (far):
                              smaller always means nearer, and "nothing came
                              back" never reads as "something at the glass".
  channel 2  thermal excess   each zone's °C above the frame's median zone.

Usage as a library:

    from encode import load_session
    X, y, frames = load_session("sessions/2026-11-23_10_present-marked.csv.gz")

Per-frame labels (<session>.labels.csv.gz) override the session class;
`unsure` and blank frames are dropped, and so is frame 1 (always
all-invalid). Run as a script to encode every session and print a summary:

    python3 tools/encode.py sessions
"""

import csv
import glob
import gzip
import os
import sys

import numpy as np

CLASSES = ("empty", "seated", "present")
VALID_STATUS = (5, 9)
D_MAX_MM = 2000.0


def encode(d_mm, status, h_cC):
    """Encode frames. Arrays of shape (n, 64): raw distance mm, ULD target
    status, AMG8833 centidegrees. Returns float32 (n, 3, 8, 8)."""
    d = np.asarray(d_mm, float)
    s = np.asarray(status)
    h = np.asarray(h_cC, float) / 100.0
    valid = np.isin(s, VALID_STATUS) & (d > 0)
    dist = np.where(valid, np.clip(d, 0, D_MAX_MM) / D_MAX_MM, 1.0)
    excess = h - np.median(h, axis=1, keepdims=True)
    x = np.stack([valid.astype(float), dist, excess], axis=1)
    return x.reshape(-1, 3, 8, 8).astype(np.float32)


def load_session(path):
    """Returns X (n,3,8,8), y (n,) class indices into CLASSES, frame numbers."""
    rows = [r for r in csv.DictReader(gzip.open(path, "rt")) if r["frame"] != "1"]
    session_cls = rows[0]["label"].split("/")[0]
    lab_path = path[: -len(".csv.gz")] + ".labels.csv.gz"
    if os.path.exists(lab_path):
        per = {r["frame"]: r["label"] for r in csv.DictReader(gzip.open(lab_path, "rt"))}
        cls = [per.get(r["frame"], "") for r in rows]
    else:
        cls = [session_cls] * len(rows)
    keep = [i for i, c in enumerate(cls) if c in CLASSES]
    rows = [rows[i] for i in keep]
    col = lambda p: [[int(r[f"{p}{i:02d}"]) for i in range(64)] for r in rows]
    X = encode(col("d"), col("s"), col("h"))
    y = np.array([CLASSES.index(cls[i]) for i in keep])
    frames = np.array([int(r["frame"]) for r in rows])
    return X, y, frames


if __name__ == "__main__":
    total = np.zeros(len(CLASSES), int)
    for p in sorted(glob.glob(os.path.join(sys.argv[1], "*.csv.gz"))):
        if p.endswith(".labels.csv.gz"):
            continue
        X, y, _ = load_session(p)
        counts = np.bincount(y, minlength=len(CLASSES))
        total += counts
        print(f"{os.path.basename(p)[:-7]:32s} {X.shape[0]:5d} frames  "
              + "  ".join(f"{c} {n:4d}" for c, n in zip(CLASSES, counts))
              + f"   valid {X[:, 0].mean():.0%}")
    print("total", "  ".join(f"{c} {n}" for c, n in zip(CLASSES, total)))
