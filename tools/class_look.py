"""Gate 2 exit check: do empty / seated / present look different?

Usage:  python3 tools/class_look.py sessions figures/gate2/<name>
Writes <name>_grids.png and <name>_scatter.png and prints per-session numbers.
Rerun after each collection round; the exit criterion is that the classes are
separable by eye as heatmaps.

Figure 1 — per-class average grids, shared scales down each column:
  ToF: fraction of frames with a valid return; median distance where valid.
  Thermal: mean excess over each frame's median (diverging, gray = 0).
Figure 2 — every frame as two numbers (ToF zones under 620 mm, thermal zones
over +1.5 °C), coloured by class, to see overlap frame by frame.
"""

import csv
import glob
import gzip
import os
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.colors import LinearSegmentedColormap, TwoSlopeNorm

SESS = sys.argv[1]
OUT = sys.argv[2]

SURFACE = "#fcfcfb"
INK, INK2, MUTED = "#0b0b0b", "#52514e", "#8a8985"
CLASS_COLOR = {"empty": "#2a78d6", "seated": "#eb6834", "present": "#1baf7a"}
CLASS_MARK = {"empty": "o", "seated": "s", "present": "^"}
CLASSES = ["empty", "seated", "present"]

blue = LinearSegmentedColormap.from_list(
    "blue", ["#cde2fb", "#86b6ef", "#3987e5", "#1c5cab", "#0d366b"])
# Distance: near = dark, so a person reads as the strong mark.
orange_r = LinearSegmentedColormap.from_list(
    "orange_r", ["#7a2e0e", "#c2501f", "#eb6834", "#f3a37f", "#fbe0d2"])
diverging = LinearSegmentedColormap.from_list(
    "div", ["#1c5cab", "#86b6ef", "#f0efec", "#f19a96", "#b8302f"])
for cm in (blue, orange_r, diverging):
    cm.set_bad("#e4e3df")

frames = {c: [] for c in CLASSES}
per_session = []
dayparts, dates = set(), set()
by_daypart = {}
for path in sorted(glob.glob(os.path.join(SESS, "*.csv.gz"))):
    if path.endswith(".labels.csv.gz"):
        continue
    rows = list(csv.DictReader(gzip.open(path, "rt")))
    cls = rows[0]["label"].split("/")[0]
    daypart = rows[0]["label"].split("/")[-1]
    dayparts.add(daypart)
    dates.add(os.path.basename(path)[:10])
    d = np.array([[int(r[f"d{i:02d}"]) for i in range(64)] for r in rows], float)
    s = np.array([[int(r[f"s{i:02d}"]) for i in range(64)] for r in rows])
    h = np.array([[int(r[f"h{i:02d}"]) for i in range(64)] for r in rows], float) / 100
    valid = np.isin(s, (5, 9)) & (d > 0)  # same rule as tools/encode.py
    excess = h - np.median(h, axis=1, keepdims=True)
    near = (valid & (d < 620)).sum(1)
    hot = (excess > 1.5).sum(1)
    # Per-frame labels override the session class where they exist
    # (sessions/README.md); unsure and blank frames are left out.
    lab_path = path[:-len(".csv.gz")] + ".labels.csv.gz"
    if os.path.exists(lab_path):
        lab = {r["frame"]: r["label"]
               for r in csv.DictReader(gzip.open(lab_path, "rt"))}
        per_frame = np.array([lab.get(r["frame"], "") for r in rows])
    else:
        per_frame = np.full(len(rows), cls)
    for c in CLASSES:
        m = per_frame == c
        if m.any():
            frames[c].append((d[m], valid[m], excess[m], near[m], hot[m]))
            by_daypart.setdefault((c, daypart), []).append((near[m], hot[m]))
    tag = os.path.basename(path)[11:-7] + ("*" if os.path.exists(lab_path) else "")
    per_session.append((tag, cls, near, hot))

# ---------- figure 1: class averages ----------
plt.rcParams.update({"font.size": 9, "text.color": INK, "axes.labelcolor": INK2,
                     "xtick.color": MUTED, "ytick.color": MUTED})
fig, axes = plt.subplots(3, 3, figsize=(10.5, 10), facecolor=SURFACE)
cols = ["ToF: share of frames with a return",
        "ToF: median distance where valid (mm)",
        "Thermal: mean °C above frame median"]
ims = [None] * 3
for r, cls in enumerate(CLASSES):
    d = np.concatenate([f[0] for f in frames[cls]])
    v = np.concatenate([f[1] for f in frames[cls]])
    e = np.concatenate([f[2] for f in frames[cls]])
    share = v.mean(0).reshape(8, 8)
    dist = np.full(64, np.nan)
    for i in range(64):
        if v[:, i].mean() > 0.5:
            dist[i] = np.median(d[v[:, i], i])
    grids = [share, dist.reshape(8, 8), e.mean(0).reshape(8, 8)]
    for c, (g, cm) in enumerate(zip(grids, (blue, orange_r, diverging))):
        ax = axes[r, c]
        ax.set_facecolor(SURFACE)
        if c == 0:
            im = ax.imshow(g, cmap=cm, vmin=0, vmax=1)
        elif c == 1:
            im = ax.imshow(np.ma.masked_invalid(g), cmap=cm, vmin=300, vmax=1800)
        else:
            im = ax.imshow(g, cmap=cm, norm=TwoSlopeNorm(0, -3, 3))
        ims[c] = im
        for y in range(8):
            for x in range(8):
                val = g[y, x]
                if np.isnan(val):
                    continue
                txt = f"{val:.1f}" if c == 2 else (f"{val:.0%}"[:-1] if c == 0 else f"{val / 1000:.1f}")
                ax.text(x, y, txt, ha="center", va="center", fontsize=6.5, color=INK2)
        ax.set_xticks(range(8)); ax.set_yticks(range(8))
        ax.tick_params(length=0, labelsize=7)
        for sp in ax.spines.values():
            sp.set_visible(False)
        if r == 0:
            ax.set_title(cols[c], fontsize=9.5, color=INK, pad=8)
        if c == 0:
            n = sum(len(f[3]) for f in frames[cls])
            ax.set_ylabel(f"{cls}\n{len(frames[cls])} sessions · {n} frames",
                          fontsize=10, color=INK, labelpad=8)
for c in range(3):
    cb = fig.colorbar(ims[c], ax=axes[:, c], orientation="horizontal",
                      fraction=0.025, pad=0.04, aspect=30)
    cb.outline.set_visible(False)
    cb.ax.tick_params(labelsize=7, colors=MUTED, length=0)
fig.suptitle(f"Per-class average grids — {len(per_session)} sessions, "
             f"{len(dates)} day(s) ({', '.join(sorted(dates))}), "
             f"daypart {'/'.join(sorted(dayparts))}.  "
             "Seen from behind the sensor; grey = return in under half the frames.",
             fontsize=10, color=INK2, y=0.995)
fig.savefig(OUT + "_grids.png", dpi=110, facecolor=SURFACE, bbox_inches="tight")

# ---------- figure 2: frame-level scatter ----------
rng = np.random.default_rng(0)
fig, ax = plt.subplots(figsize=(8.5, 6), facecolor=SURFACE)
ax.set_facecolor(SURFACE)
for cls in CLASSES:
    near = np.concatenate([f[3] for f in frames[cls]])
    hot = np.concatenate([f[4] for f in frames[cls]])
    ax.scatter(near + rng.uniform(-.35, .35, near.size),
               hot + rng.uniform(-.35, .35, hot.size),
               s=10, marker=CLASS_MARK[cls], color=CLASS_COLOR[cls], alpha=.25,
               linewidths=0, label=f"{cls} ({near.size} frames)")
    # direct label at the class's median point
    ax.annotate(cls, (np.median(near), np.median(hot)), xytext=(8, 8),
                textcoords="offset points", fontsize=10, color=INK, weight="bold")
ax.set_xlabel("ToF zones nearer than 620 mm")
ax.set_ylabel("thermal zones more than +1.5 °C above frame median")
ax.grid(color="#e4e3df", linewidth=.6)
ax.set_axisbelow(True)
for sp in ("top", "right"):
    ax.spines[sp].set_visible(False)
for sp in ("left", "bottom"):
    ax.spines[sp].set_color("#d6d5d0")
leg = ax.legend(frameon=False, loc="upper left", markerscale=1.8)
for lh in getattr(leg, "legend_handles", None) or leg.legendHandles:
    lh.set_alpha(1)
ax.set_title("Every frame as two numbers (jittered). Walking-past frames with "
             "nobody in view sit at the origin with empty.", fontsize=9.5, color=INK2)
fig.savefig(OUT + "_scatter.png", dpi=110, facecolor=SURFACE, bbox_inches="tight")

# ---------- numbers ----------
print("per session: share of frames with a person signature (near>=3 or hot>=2)")
for name, cls, near, hot in per_session:
    sig = ((near >= 3) | (hot >= 2)).mean()
    print(f"  {name:22s} {cls:8s} {sig:6.0%}   near median {np.median(near):4.0f}   hot median {np.median(hot):4.0f}")

print("\nper class and daypart (frame-level labels): person-signature share, near / hot medians")
for cls in CLASSES:
    for dp in ("am", "pm", "eve"):
        parts = by_daypart.get((cls, dp))
        if not parts:
            continue
        near = np.concatenate([p[0] for p in parts]); hot = np.concatenate([p[1] for p in parts])
        sig = ((near >= 3) | (hot >= 2)).mean()
        print(f"  {cls:8s} {dp:4s} {near.size:6d} frames   {sig:6.1%}   near {np.median(near):3.0f}   hot {np.median(hot):3.0f}")
