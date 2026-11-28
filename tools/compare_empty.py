"""Is the sensor head still where the training data was recorded?

Compares an empty-room recording against the reference empty sessions
(sessions/*empty-marked*, chair on its marks) and, for scale, the
references against each other. Run after anything that could have moved
the head: a knock, the shield coming off, mounting the display.

Per recording, per zone, it takes the median over frames, then counts:

  ToF zones changed   validity flipped (valid in most frames in one, not the
                      other), or both valid and the median distance moved
                      more than 100 mm (about one zone's width at 1 m, the
                      resolution NOTES 2026-11-22 found for chair placement)
  thermal hot spots   pixels more than 1.0 C warmer, relative to each
                      frame's own median, than the reference: a heat source
                      in view (the display's backlight, if it is)
  tilt / roll         from the recordings' own IMU columns

The references-vs-references line is the bar: a new recording that changes
no more zones than the references do among themselves is the same view.

    python3 tools/compare_empty.py NEW.csv[.gz] [--ref sessions/X.csv.gz ...]

Record NEW with the chair on its marks and nobody in view, e.g. 60 s:
    cargo run --release --bin fused_stream \\
      | python3 tools/heatmap_viewer.py --no-display --record NEW.csv --label empty
"""

import argparse
import csv
import glob
import gzip
import itertools
import os
import statistics
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VALID = {5, 9}
DIST_MM = 100
HOT_C = 1.0


def load(path):
    op = gzip.open if path.endswith(".gz") else open
    with op(path, "rt") as f:
        rows = list(csv.DictReader(f))
    if not rows:
        sys.exit(f"{path}: no frames")
    valid = [[int(r[f"s{z:02d}"]) in VALID and int(r[f"d{z:02d}"]) > 0 for z in range(64)] for r in rows]
    dist = [[int(r[f"d{z:02d}"]) for z in range(64)] for r in rows]
    rel = []
    for r in rows:
        h = [int(r[f"h{z:02d}"]) for z in range(64)]
        m = statistics.median(h)
        rel.append([(v - m) / 100.0 for v in h])
    num = lambda k: statistics.median(float(r[k]) for r in rows if r.get(k) not in (None, "", "nan"))
    s = {
        "name": os.path.basename(path).replace(".csv.gz", "").replace(".csv", ""),
        "frames": len(rows),
        "valid": [sum(v[z] for v in valid) / len(rows) > 0.5 for z in range(64)],
        "dist": [statistics.median(d[z] for d, v in zip(dist, valid) if v[z]) if any(v[z] for v in valid) else None
                 for z in range(64)],
        "heat": [statistics.median(h[z] for h in rel) for z in range(64)],
    }
    try:
        s["tilt"], s["roll"] = num("tilt_deg"), num("roll_deg") % 360
    except statistics.StatisticsError:
        s["tilt"] = s["roll"] = None
    return s


def compare(a, b):
    changed = []
    for z in range(64):
        if a["valid"][z] != b["valid"][z]:
            changed.append(z)
        elif a["valid"][z] and abs(a["dist"][z] - b["dist"][z]) > DIST_MM:
            changed.append(z)
    hot = [z for z in range(64) if a["heat"][z] - b["heat"][z] > HOT_C]
    return changed, hot


def zones(zs):
    return " ".join(f"r{z // 8}c{z % 8}" for z in zs) or "-"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("new")
    ap.add_argument("--ref", nargs="*", help="reference sessions (default: sessions/*empty-marked*)")
    a = ap.parse_args()
    refs = a.ref or sorted(glob.glob(os.path.join(ROOT, "sessions", "*empty-marked*.csv.gz")))
    if not refs:
        sys.exit("no reference sessions")
    N = load(a.new)
    R = [r for r in (load(p) for p in refs) if r["name"] != N["name"]]

    print(f"new: {N['name']}, {N['frames']} frames, tilt {N['tilt']:.2f}, roll {N['roll']:.2f}")
    print("\nreferences among themselves (the bar):")
    base = []
    for x, y in itertools.combinations(R, 2):
        c, _ = compare(x, y)
        base.append(len(c))
    for r in R:
        print(f"  {r['name']:34s} tilt {r['tilt']:.2f} roll {r['roll']:.2f}")
    print(f"  ToF zones changed between reference pairs: min {min(base)}, median {statistics.median(base)}, "
          f"max {max(base)}" if base else "  (one reference: no bar)")

    print("\nnew vs each reference:")
    worst = 0
    for r in R:
        c, hot = compare(N, r)
        worst = max(worst, len(c))
        print(f"  {r['name']:34s} ToF changed {len(c):2d} [{zones(c)}]  hotter {len(hot)} [{zones(hot)}]")
    best = min(len(compare(N, r)[0]) for r in R)
    typical, most = (statistics.median(base), max(base)) if base else (0, 0)
    verdict = ("same view" if best <= typical else
               "borderline: within the references' worst spread" if best <= most else
               "VIEW HAS CHANGED")
    print(f"\nclosest reference: {best} zones changed; references among themselves: "
          f"typically {typical}, at most {most}. Verdict: {verdict}")


if __name__ == "__main__":
    main()
