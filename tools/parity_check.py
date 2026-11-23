"""Does the board classify exactly what the PC would?

Reads a `fused_stream` log (the raw ToF/status/thermal grids *and* the
firmware's `infer frame N: ...` lines), re-encodes each raw frame with
tools/encode.py, runs the ONNX model on the PC, and compares the logits with
the ones the board printed.

This checks the whole on-device path at once: the Rust encoding
(firmware/src/encode.rs) against the Python one the model was trained on,
and ST's generated C on the Cortex-M33 against the original model. A
mismatch in either shows up as a logit difference.

    timeout 60 cargo run --release --bin fused_stream > run.log 2>&1   # in firmware/
    ml/.venv/bin/python tools/parity_check.py firmware/run.log

Pass: every frame the same class, and the largest logit difference small
(float32 rounding — around 1e-4 on logits of order 10).
"""

import os
import re
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from encode import CLASSES, encode  # noqa: E402
from heatmap_viewer import FusedFrame, assemble, parse_stream  # noqa: E402

MODEL = os.path.join(os.path.dirname(HERE), "ml", "results", "occupancy_cnn.onnx")
INFER = re.compile(
    r"infer frame (\d+): (\w+) .*\| logits (\S+) (\S+) (\S+) \| encode (\d+) us, run (\d+) us")


def main(path):
    import onnxruntime as ort

    lines = open(path, errors="replace").read().splitlines()
    frames = {f.number: f for f in assemble(parse_stream(iter(lines)))
              if isinstance(f, FusedFrame)}
    board = {}
    for line in lines:
        m = INFER.search(line)
        if m:
            board[int(m.group(1))] = (np.array([float(m.group(i)) for i in (3, 4, 5)]),
                                      int(m.group(6)), int(m.group(7)))
    common = sorted(set(frames) & set(board))
    if not common:
        sys.exit(f"no frames with both raw grids and an infer line in {path}")

    sess = ort.InferenceSession(MODEL)
    worst, disagree, run_us = 0.0, [], []
    for n in common:
        f = frames[n]
        x = encode(np.array([f.distance]), np.array([f.status]), np.array([f.thermal]))
        pc = sess.run(None, {"input": x})[0][0]
        dev, _, run = board[n]
        diff = float(np.abs(pc - dev).max())
        worst = max(worst, diff)
        run_us.append(run)
        if pc.argmax() != dev.argmax():
            disagree.append((n, CLASSES[int(pc.argmax())], CLASSES[int(dev.argmax())]))

    classes = np.bincount([int(board[n][0].argmax()) for n in common], minlength=3)
    print(f"{len(common)} frames compared ({', '.join(f'{c} {k}' for c, k in zip(CLASSES, classes))})")
    print(f"same class on board and PC: {len(common) - len(disagree)} / {len(common)}")
    print(f"largest logit difference:   {worst:.2e}")
    print(f"inference on the board:     {np.median(run_us) / 1000:.2f} ms median")
    for n, pc, dev in disagree[:10]:
        print(f"  frame {n}: PC {pc}, board {dev}")
    ok = not disagree and worst < 1e-2
    print("PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
