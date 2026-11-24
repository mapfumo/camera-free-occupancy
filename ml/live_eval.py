"""Score models on the live scripted test — frames no model has trained on.

The live test (trials/, NOTES "Live scripted test") is a separate recording
with hand labels, never part of sessions/, so it is held out from every
model trained on sessions/. It also contains the posture the deployed model
gets wrong: standing bent over the keyboard (frames 1426-1511).

Compares, on those frames:
  deployed   ml/results/occupancy_cnn.onnx (what the board runs)
  without    retrained on sessions/ minus the named sessions (same recipe)
  with       retrained on all of sessions/

`without` vs `with` isolates what the named sessions add; `without` vs
`deployed` shows seed/recipe noise. Several seeds, since one is not evidence.

    ml/.venv/bin/python ml/live_eval.py 2026-11-24_13_present-marked [--seeds 3]
"""

import argparse
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dataset import CLASSES, ROOT, load_all  # noqa: E402
from encode import load_session  # noqa: E402
from train import fit, probabilities  # noqa: E402

TRIAL = os.path.join(ROOT, "trials", "2026-11-24_live_full.csv.gz")
BENT = (1426, 1511)  # standing bent over the keyboard, labelled present


def score(pred, y, frames):
    bent = (frames >= BENT[0]) & (frames <= BENT[1])
    per = [(pred[y == k] == k).mean() for k in range(len(CLASSES))]
    occ = ((pred != 0) == (y != 0)).mean()
    return (f"all {np.mean(pred == y):6.1%}  occupancy {occ:6.1%}  "
            + "  ".join(f"{c} {p:6.1%}" for c, p in zip(CLASSES, per))
            + f"  | bent-over {np.mean(pred[bent] == 2):6.1%} present")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("sessions", nargs="+", help="session names to test adding")
    ap.add_argument("--seeds", type=int, default=3)
    a = ap.parse_args()

    X, y, frames = load_session(TRIAL)
    d = load_all()
    missing = [s for s in a.sessions if s not in set(d["session"])]
    if missing:
        sys.exit(f"not in sessions/: {missing}")
    keep = ~np.isin(d["session"], a.sessions)
    print(f"live test: {len(y)} labelled frames "
          f"({', '.join(f'{c} {n}' for c, n in zip(CLASSES, np.bincount(y, minlength=3)))})")
    print(f"training frames: without {keep.sum()}, with {len(keep)} "
          f"(+{(~keep).sum()} from {', '.join(a.sessions)})\n")

    import onnxruntime as ort
    sess = ort.InferenceSession(os.path.join(ROOT, "ml", "results", "occupancy_cnn.onnx"))
    dep = np.array([sess.run(None, {"input": X[i:i + 1]})[0][0] for i in range(len(X))]).argmax(1)
    print(f"{'deployed':14s} {score(dep, y, frames)}")
    for seed in range(a.seeds):
        for name, m in (("without", keep), ("with", np.ones_like(keep))):
            net = fit(d["X"][m], d["y"][m], seed=seed)
            pred = probabilities(net, X).argmax(1)
            print(f"{name + f' s{seed}':14s} {score(pred, y, frames)}", flush=True)


if __name__ == "__main__":
    main()
