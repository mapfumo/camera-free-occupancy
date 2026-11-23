"""Gate 3 baselines — the scores a neural network has to beat to earn its place.

1. The Gate 2 signature rule, as a classifier. "Person in view" if ≥3 ToF
   zones are nearer than 620 mm or ≥2 thermal zones are >1.5 °C above the
   frame median (tools/class_look.py). It only knows empty vs occupied, so
   it is scored on that binary question.
2. Logistic regression on the 192 encoded inputs (tools/encode.py), three
   classes, class-balanced. A linear model: if it already separates the
   classes on unseen rounds, the CNN has very little to add.

Both are scored by leave-one-round-out (ml/dataset.py): train on five rounds,
test on the sixth, pool the six test sets into one confusion matrix.

    ml/.venv/bin/python ml/baseline.py
"""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dataset import CLASSES, confusion, load_all, report, rounds  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "results")


def signature_rule(X):
    valid, dist, excess = X[:, 0].reshape(-1, 64), X[:, 1].reshape(-1, 64), X[:, 2].reshape(-1, 64)
    near = ((valid > 0) & (dist * 2000 < 620)).sum(1)
    hot = (excess > 1.5).sum(1)
    return ((near >= 3) | (hot >= 2)).astype(int)


def main():
    d = load_all()
    X, y = d["X"], d["y"]
    lines = []

    # 1. rule, binary
    occ_true = (y != 0).astype(int)
    occ_pred = signature_rule(X)
    m = np.zeros((2, 2), int)
    np.add.at(m, (occ_true, occ_pred), 1)
    lines.append("== Signature rule (no training), empty vs occupied, all frames")
    lines.append(f"             pred empty  pred occupied")
    lines.append(f"empty        {m[0,0]:10d}  {m[0,1]:13d}    recall {m[0,0]/m[0].sum():.1%}")
    lines.append(f"occupied     {m[1,0]:10d}  {m[1,1]:13d}    recall {m[1,1]/m[1].sum():.1%}")
    lines.append(f"accuracy {np.trace(m)/m.sum():.1%}")
    lines.append("")

    # 2. logistic regression, leave-one-round-out
    from sklearn.linear_model import LogisticRegression
    from sklearn.preprocessing import StandardScaler
    from sklearn.pipeline import make_pipeline

    flat = X.reshape(len(X), -1)
    pooled = np.zeros((len(CLASSES), len(CLASSES)), int)
    lines.append("== Logistic regression, 192 inputs, leave-one-round-out")
    for r, tr, te in rounds(d):
        clf = make_pipeline(StandardScaler(),
                            LogisticRegression(max_iter=2000, class_weight="balanced", C=0.1))
        clf.fit(flat[tr], y[tr])
        m = confusion(y[te], clf.predict(flat[te]))
        pooled += m
        bal = np.mean([m[i, i] / m[i].sum() for i in range(len(CLASSES))])
        lines.append(f"  held out {r:16s} accuracy {np.trace(m)/m.sum():6.1%}   balanced {bal:6.1%}")
    lines.append("")
    lines.append(report(pooled, "pooled over the six held-out rounds (rows true, columns predicted):"))

    text = "\n".join(lines)
    print(text)
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "baseline.txt"), "w") as f:
        f.write(text + "\n")


if __name__ == "__main__":
    main()
