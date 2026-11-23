"""Gate 3 dataset: every session encoded, with the metadata the splits need.

The encoding itself lives in tools/encode.py (decided at Gate 2); this module
only gathers sessions and decides how they are split.

The split rule: **never split frames of one session across train and test.**
Frames 0.2 s apart are near-copies, so a frame-level split scores the model on
its own training data. The unit of evaluation here is a *round* — one daypart
on one day, recorded as a block with every class in it:

    2026-11-22 pm, 2026-11-22 eve, 2026-11-23 am, 2026-11-23 pm,
    2026-11-23 eve, 2026-11-24 am

Leave-one-round-out cross-validation holds out a whole round at a time, so
every score is on a day-and-daypart the model has never seen — chair placed
again, room at a different temperature, light different. That is the honest
version of "held-out data".

    from dataset import load_all, rounds
    data = load_all()                    # dict of arrays, one row per frame
    for name, train, test in rounds(data):
        ...
"""

import glob
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, os.path.join(ROOT, "tools"))
from encode import CLASSES, load_session  # noqa: E402

SESSIONS = os.path.join(ROOT, "sessions")


def load_all(sessions_dir=SESSIONS):
    """All labelled frames. Returns dict:
    X (n,3,8,8) float32, y (n,) int, session (n,) str, round (n,) str,
    chair (n,) str, frame (n,) int."""
    Xs, ys, sess, rnd, chair, frames = [], [], [], [], [], []
    for path in sorted(glob.glob(os.path.join(sessions_dir, "*.csv.gz"))):
        if path.endswith(".labels.csv.gz"):
            continue
        X, y, f = load_session(path)
        if len(y) == 0:
            continue
        name = os.path.basename(path)[: -len(".csv.gz")]
        date = name[:10]
        # label column is <class>/<chair>/<daypart>; read it from the file
        import csv
        import gzip
        with gzip.open(path, "rt") as fh:
            label = next(csv.DictReader(fh))["label"]
        _, ch, daypart = label.split("/")
        Xs.append(X)
        ys.append(y)
        n = len(y)
        sess += [name] * n
        rnd += [f"{date}_{daypart}"] * n
        chair += [ch] * n
        frames.append(f)
    return {
        "X": np.concatenate(Xs),
        "y": np.concatenate(ys),
        "session": np.array(sess),
        "round": np.array(rnd),
        "chair": np.array(chair),
        "frame": np.concatenate(frames),
    }


def rounds(data):
    """Leave-one-round-out folds: yields (round_name, train_mask, test_mask)."""
    for r in sorted(set(data["round"])):
        test = data["round"] == r
        yield r, ~test, test


def confusion(y_true, y_pred, k=len(CLASSES)):
    m = np.zeros((k, k), int)
    np.add.at(m, (y_true, y_pred), 1)
    return m


def report(m, title=""):
    """Confusion matrix as text: rows = true class, columns = predicted."""
    lines = [title] if title else []
    w = max(len(c) for c in CLASSES) + 2
    lines.append(" " * w + "".join(f"{c:>10s}" for c in CLASSES) + "    recall")
    for i, c in enumerate(CLASSES):
        row = m[i]
        rec = row[i] / row.sum() if row.sum() else float("nan")
        lines.append(f"{c:<{w}s}" + "".join(f"{v:10d}" for v in row) + f"    {rec:6.1%}")
    prec = [m[i, i] / m[:, i].sum() if m[:, i].sum() else float("nan") for i in range(len(CLASSES))]
    lines.append(f"{'precision':<{w}s}" + "".join(f"{p:10.1%}" for p in prec))
    acc = np.trace(m) / m.sum()
    bal = np.mean([m[i, i] / m[i].sum() for i in range(len(CLASSES))])
    lines.append(f"accuracy {acc:.1%}   balanced accuracy {bal:.1%}   frames {m.sum()}")
    return "\n".join(lines)


if __name__ == "__main__":
    d = load_all()
    print(f"{len(d['y'])} frames, {len(set(d['session']))} sessions")
    for r, _, te in rounds(d):
        counts = np.bincount(d["y"][te], minlength=len(CLASSES))
        print(f"  {r:16s} {len(set(d['session'][te])):3d} sessions  "
              + "  ".join(f"{c} {n:5d}" for c, n in zip(CLASSES, counts)))
