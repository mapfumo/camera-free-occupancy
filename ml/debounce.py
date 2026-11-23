"""Gate 4, stage 1: choose the debounce rule from data, not by guessing.

The CNN classifies every frame (5 Hz). A building wants a *state* that
changes when reality changes and not otherwise. This simulates candidate
rules on real prediction streams: the out-of-fold probabilities from the
leave-one-round-out run (ml/train.py), i.e. what the model said about each
frame when it had never seen that frame's round. Within a session the frames
are in time order, so each session is a genuine stream.

Three tests per rule:

  steady       Whole-session-labelled sessions (nothing changes in them).
               Every reported state change is a false one: flapping.
  transitions  Within each round, session streams chained end to end —
               empty -> seated -> present -> seated -> empty, and so on —
               so the class changes for real at the joins. Measures lag
               (seconds from the change to the new state being reported
               and held) and extra changes around each join.
  walking      The per-frame-labelled walking/sit-stand sessions: agreement
               with the labels, and frames of walking past reported `seated`.

"false occ" counts only empty <-> occupied changes — the BACnet binary input
a building acts on; "false class" counts every change, seated <-> present
included. Both skip the first 3 s, before the device would report anything.

Pass bar, fixed before looking (NOTES, Phase 4): no false change in steady
data, every real change within 3 s, walking past never reported `seated`.

    ml/.venv/bin/python ml/debounce.py
"""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dataset import CLASSES, load_all  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
HZ = 5
EMPTY, SEATED, PRESENT = 0, 1, 2
WARMUP = 3 * HZ  # the device reports nothing until its window has filled


# ---------------------------------------------------------------- rules ----
# Each rule maps a (n, 3) probability stream to a reported state per frame.

def raw(p):
    return p.argmax(1)


def majority(w):
    def rule(p):
        c = p.argmax(1)
        out = np.empty(len(c), int)
        state = c[0]
        for i in range(len(c)):
            votes = np.bincount(c[max(0, i - w + 1): i + 1], minlength=3)
            best = votes.argmax()
            if votes[best] > votes[state]:  # ties keep the current state
                state = best
            out[i] = state
        return out
    rule.__name__ = f"majority {w / HZ:.0f} s" if w % HZ == 0 else f"majority {w} fr"
    return rule


def streak(n):
    def rule(p):
        c = p.argmax(1)
        out = np.empty(len(c), int)
        state, run, cand = c[0], 0, c[0]
        for i, x in enumerate(c):
            run = run + 1 if x == cand else 1
            cand = x
            if cand != state and run >= n:
                state = cand
            out[i] = state
        return out
    rule.__name__ = f"{n} in a row ({n / HZ:.1f} s)"
    return rule


def ema(alpha, margin=0.0):
    def rule(p):
        out = np.empty(len(p), int)
        e = p[0].copy()
        state = int(e.argmax())
        for i in range(len(p)):
            e = alpha * p[i] + (1 - alpha) * e
            best = int(e.argmax())
            if best != state and e[best] > e[state] + margin:
                state = best
            out[i] = state
        return out
    rule.__name__ = f"EMA a={alpha}" + (f" +{margin}" if margin else "")
    return rule


RULES = [raw, majority(5), majority(10), majority(15), streak(3), streak(5),
         streak(8), streak(10), ema(0.3), ema(0.2), ema(0.2, 0.2), ema(0.1)]


# ---------------------------------------------------------------- data -----

def streams(d, prob):
    """Per session: (name, round, labels, probs) in frame order."""
    out = []
    for s in sorted(set(d["session"])):
        m = np.where(d["session"] == s)[0]
        m = m[np.argsort(d["frame"][m])]
        out.append((s, d["round"][m[0]], d["y"][m], prob[m]))
    return out


def chains(ss):
    """Per round, chain whole-session streams so the class changes at joins."""
    plan = [("empty", "marked"), ("seated", None), ("present", "out"), ("seated", None),
            ("present", "marked"), ("empty", "away"), ("seated", None), ("empty", "in")]
    out = []
    for r in sorted({x[1] for x in ss}):
        pool = [x for x in ss if x[1] == r and not os.path.exists(
            os.path.join(os.path.dirname(HERE), "sessions", f"{x[0]}.labels.csv.gz"))]
        used, parts = set(), []
        for cls, chair in plan:
            for x in pool:
                name = x[0]
                if name in used or f"_{cls}-" not in name:
                    continue
                if chair and not name.endswith(f"-{chair}"):
                    continue
                used.add(name)
                parts.append(x)
                break
        if len(parts) >= 4:
            out.append((r, np.concatenate([x[2] for x in parts]),
                        np.concatenate([x[3] for x in parts])))
    return out


# ------------------------------------------------------------- metrics -----

def changes(states):
    return int((np.diff(states) != 0).sum())


def transition_stats(y, rep):
    """Lag to each real change, and extra state changes beyond one per join."""
    joins = np.where(np.diff(y) != 0)[0] + 1
    bounds = np.r_[joins, len(y)]
    lags, extra = [], 0
    for j, end in zip(joins, bounds[1:]):
        new = y[j]
        seg = rep[j:end]
        # first frame from which the report equals the new class for 3 s
        held = None
        for k in range(len(seg)):
            if seg[k] == new and (seg[k: k + 3 * HZ] == new).all():
                held = k
                break
        lags.append(np.inf if held is None else held / HZ)
        lo = max(j - 3 * HZ, 0)
        extra += max(0, changes(rep[lo: min(end, j + 6 * HZ)]) - 1)
    return lags, extra


def episodes(y, cls, min_len):
    """Runs of `cls` at least min_len frames long: (start, end)."""
    out, i = [], 0
    while i < len(y):
        if y[i] == cls:
            j = i
            while j < len(y) and y[j] == cls:
                j += 1
            if j - i >= min_len:
                out.append((i, j))
            i = j
        else:
            i += 1
    return out


def main():
    d = load_all()
    prob = np.load(os.path.join(HERE, "runs", "cnn_cv_prob_seed0.npy"))
    ss = streams(d, prob)
    labelled = {x[0] for x in ss if os.path.exists(
        os.path.join(os.path.dirname(HERE), "sessions", f"{x[0]}.labels.csv.gz"))}
    steady = [x for x in ss if x[0] not in labelled]
    walking = [x for x in ss if x[0] in labelled]
    ch = chains(ss)
    steady_hours = sum(len(x[2]) for x in steady) / HZ / 3600

    lines = [f"steady: {len(steady)} sessions, {steady_hours:.2f} h   "
             f"transitions: {len(ch)} chained rounds, "
             f"{sum(int((np.diff(c[1]) != 0).sum()) for c in ch)} real changes   "
             f"walking: {len(walking)} sessions",
             "",
             f"{'rule':22s} {'false occ':>9s} {'false class':>11s} {'lag median':>10s} {'lag max':>8s} "
             f"{'extra':>6s} {'walk agree':>10s} {'walk->seated':>12s}"]
    for rule in RULES:
        flaps = sum(changes(rule(x[3])[WARMUP:]) for x in steady)
        # the building's question: empty vs occupied, ignoring seated/present
        occ_flaps = sum(changes((rule(x[3])[WARMUP:] != EMPTY).astype(int)) for x in steady)
        lags, extra = [], 0
        for _, y, p in ch:
            l, e = transition_stats(y, rule(p))
            lags += l
            extra += e
        agree, n, to_seated = 0, 0, 0
        for _, _, y, p in walking:
            rep = rule(p)
            agree += (rep == y).sum()
            n += len(y)
        for name, _, y, p in walking:
            if name.endswith("2026-11-22_23_present-marked"):
                continue  # the sit-and-stand session really does contain seated
            to_seated += (rule(p) == SEATED).sum()
        lags = np.array(lags)
        lines.append(
            f"{rule.__name__:22s} {occ_flaps:9d} {flaps:11d} {np.median(lags):9.1f}s {lags.max():7.1f}s "
            f"{extra:6d} {agree / n:10.1%} {to_seated:12d}")
    text = "\n".join(lines)
    print(text)
    with open(os.path.join(HERE, "results", "debounce.txt"), "w") as f:
        f.write(text + "\n")


if __name__ == "__main__":
    main()
