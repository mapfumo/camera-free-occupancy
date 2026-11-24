"""Score a live run: the board's debounced state against hand labels.

Gate 4, stage 2. The board was running the whole firmware path (sensors ->
encode -> CNN -> 8-in-a-row debounce) and printed `state frame N: <class>
(changed)` whenever the reported state changed. The person labelled every
frame from the webcam (tools/label_frames.py). This compares the two.

    python3 tools/score_live.py trials/2026-11-24_live.log \
        trials/2026-11-24_live_full.labels.csv.gz

Reported:

  real changes   label changes where the new label then holds >= 1.6 s
                 (8 frames). Shorter events are, by design, not reportable;
                 they are listed separately. Lag = seconds from the label
                 change to the board reporting the new class.
  false changes  board changes to a class the person was not in at any point
                 in the 3 s before the change.
  occupancy      the same two, counting only empty <-> occupied (the BACnet
                 binary input).
  walk->seated   frames labelled present that the board reported seated.

Frames labelled `unsure` or blank match anything.
"""

import csv
import gzip
import re
import sys

HZ = 5
HOLD = 8           # frames a label must hold to count as a reportable change
LOOKBACK = 3 * HZ  # frames before a board change that may justify it
STATE = re.compile(r"state frame (\d+): (\w+) \(changed\)")
INFER = re.compile(r"infer frame (\d+): (\w+) ")


def main(log_path, labels_path):
    labels = {}
    with gzip.open(labels_path, "rt") as f:
        for r in csv.DictReader(f):
            labels[int(r["frame"])] = r["label"] if r["label"] not in ("", "unsure") else None
    changes, raw = [], {}
    for line in open(log_path, errors="replace"):
        m = STATE.search(line)
        if m:
            changes.append((int(m.group(1)), m.group(2)))
        m = INFER.search(line)
        if m:
            raw[int(m.group(1))] = m.group(2)
    frames = sorted(labels)
    first, last = frames[0], frames[-1]

    # reported state per frame
    rep, k, cur = {}, 0, None
    for n in frames:
        while k < len(changes) and changes[k][0] <= n:
            cur = changes[k][1]
            k += 1
        rep[n] = cur

    def occ(c):
        return None if c is None else (c != "empty")

    def report(name, lab, board):
        # real changes in the labels
        seq = [(n, lab(labels[n])) for n in frames if labels[n] is not None]
        real, short = [], []
        i = 0
        while i < len(seq):
            j = i
            while j < len(seq) and seq[j][1] == seq[i][1]:
                j += 1
            run = seq[j - 1][0] - seq[i][0] + 1
            if i > 0:
                (real if run >= HOLD else short).append((seq[i][0], seq[i][1], run))
            i = j
        lags = []
        for n, c, run in real:
            hit = next((m for m in frames if m >= n and board(rep[m]) == c), None)
            lag = None if hit is None or hit > n + run + 10 * HZ else (hit - n) / HZ
            lags.append((n, c, lag))
        # board changes
        prev, false = None, []
        for n, c in changes:
            c2 = board(c)
            if prev is not None and c2 == prev:
                continue
            if prev is not None:
                window = [lab(labels.get(m)) for m in range(n - LOOKBACK, n + 1)
                          if labels.get(m) is not None]
                if window and c2 not in window:
                    false.append((n, c))
            prev = c2
        print(f"--- {name} ---")
        print(f"real changes: {len(real)}   too short to report (< {HOLD / HZ:.1f} s): {len(short)}")
        for n, c, lag in lags:
            print(f"  frame {n:5d} -> {c!s:8s} lag {'MISSED' if lag is None else f'{lag:.1f} s'}")
        for n, c, run in short:
            print(f"  (short) frame {n:5d} -> {c!s:8s} {run / HZ:.1f} s")
        print(f"false changes: {len(false)}")
        for n, c in false:
            print(f"  frame {n:5d} board -> {c}")
        ok = [x[2] for x in lags if x[2] is not None]
        if ok:
            print(f"lag median {sorted(ok)[len(ok) // 2]:.1f} s, max {max(ok):.1f} s, "
                  f"missed {sum(x[2] is None for x in lags)}")
        return lags, false

    print(f"{len(frames)} frames ({(last - first + 1) / HZ / 60:.1f} min), "
          f"{len(changes)} board state changes")
    report("class (empty / seated / present)", lambda c: c, lambda c: c)
    report("occupancy (empty / occupied)", occ, occ)

    walk = [n for n in frames if labels[n] == "present" and rep[n] == "seated"]
    agree_rep = [rep[n] == labels[n] for n in frames if labels[n] and rep[n]]
    agree_raw = [raw.get(n) == labels[n] for n in frames if labels[n] and n in raw]
    print("--- per frame ---")
    print(f"labelled present, board reported seated: {len(walk)} frames ({len(walk) / HZ:.1f} s)")
    print(f"agreement with labels: debounced {sum(agree_rep) / len(agree_rep):.1%}, "
          f"raw model {sum(agree_raw) / len(agree_raw):.1%}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
