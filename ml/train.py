"""Gate 3: a small CNN on the 3x8x8 encoding, scored on unseen rounds.

Architecture — deliberately small, and position-aware:
  conv 3x3, 3->16, ReLU
  conv 3x3, 16->16, ReLU, max-pool 2  -> 16 x 4 x 4
  flatten (256) -> dense 32, ReLU -> dense 3
No global pooling: *where* the near returns and warmth sit (bottom rows vs
top) is the seated/present signal, so the classifier keeps the layout.
No flips or shifts as augmentation, for the same reason.

    ml/.venv/bin/python ml/train.py            # leave-one-round-out CV
    ml/.venv/bin/python ml/train.py --final    # train on everything, export ONNX

Training is fixed-recipe (epochs, learning rate, class weights chosen in
advance) so the held-out round is never used to tune anything.
"""

import argparse
import os
import sys
import time

import numpy as np
import torch
from torch import nn

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dataset import CLASSES, confusion, load_all, report, rounds  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "results")
EPOCHS = 15
BATCH = 256
LR = 2e-3
SEED = 0


class Net(nn.Module):
    def __init__(self, k=len(CLASSES)):
        super().__init__()
        self.features = nn.Sequential(
            nn.Conv2d(3, 16, 3, padding=1), nn.ReLU(),
            nn.Conv2d(16, 16, 3, padding=1), nn.ReLU(),
            nn.MaxPool2d(2),
        )
        self.head = nn.Sequential(
            nn.Flatten(), nn.Linear(16 * 4 * 4, 32), nn.ReLU(), nn.Linear(32, k),
        )

    def forward(self, x):
        return self.head(self.features(x))


def fit(X, y, seed=SEED):
    torch.manual_seed(seed)
    np.random.seed(seed)
    net = Net()
    counts = np.bincount(y, minlength=len(CLASSES))
    weight = torch.tensor(len(y) / (len(CLASSES) * counts), dtype=torch.float32)
    loss_fn = nn.CrossEntropyLoss(weight=weight)
    opt = torch.optim.Adam(net.parameters(), lr=LR)
    sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, EPOCHS)
    Xt, yt = torch.from_numpy(X), torch.from_numpy(y).long()
    for _ in range(EPOCHS):
        net.train()
        perm = torch.randperm(len(yt))
        for i in range(0, len(yt), BATCH):
            idx = perm[i : i + BATCH]
            opt.zero_grad()
            loss_fn(net(Xt[idx]), yt[idx]).backward()
            opt.step()
        sched.step()
    net.eval()
    return net


def predict(net, X):
    with torch.no_grad():
        return net(torch.from_numpy(X)).argmax(1).numpy()


def cross_validate(d, seed=SEED):
    pooled = np.zeros((len(CLASSES), len(CLASSES)), int)
    lines = [f"== CNN, leave-one-round-out, seed {seed}"]
    oof = np.full(len(d["y"]), -1)
    for r, tr, te in rounds(d):
        t0 = time.time()
        net = fit(d["X"][tr], d["y"][tr], seed=seed)
        pred = predict(net, d["X"][te])
        m = confusion(d["y"][te], pred)
        pooled += m
        bal = np.mean([m[i, i] / m[i].sum() for i in range(len(CLASSES))])
        lines.append(f"  held out {r:16s} accuracy {np.trace(m)/m.sum():6.1%}   "
                     f"balanced {bal:6.1%}   ({time.time()-t0:.0f} s)")
        # where the errors are, by session, for the write-up
        wrong = pred != d["y"][te]
        for s in sorted(set(d["session"][te])):
            ms = d["session"][te] == s
            if wrong[ms].mean() > 0.05:
                lines.append(f"      {s:34s} {wrong[ms].mean():6.1%} wrong of {ms.sum()}")
        oof[te] = pred
        print(f"  {r}: {np.trace(m)/m.sum():.1%}", flush=True)
    os.makedirs(os.path.join(HERE, "runs"), exist_ok=True)
    np.save(os.path.join(HERE, "runs", f"cnn_cv_pred_seed{seed}.npy"), oof)
    lines.append("")
    lines.append(report(pooled, "pooled over the six held-out rounds (rows true, columns predicted):"))
    return "\n".join(lines), pooled


def export(d):
    net = fit(d["X"], d["y"])
    n_params = sum(p.numel() for p in net.parameters())
    path = os.path.join(OUT, "occupancy_cnn.onnx")
    dummy = torch.zeros(1, 3, 8, 8)
    torch.onnx.export(net, dummy, path, input_names=["input"], output_names=["logits"],
                      opset_version=13, dynamo=False)
    # reference input/output pairs for stedgeai validate
    rng = np.random.default_rng(0)
    idx = rng.choice(len(d["y"]), 300, replace=False)
    xs = d["X"][idx]
    with torch.no_grad():
        ys = net(torch.from_numpy(xs)).numpy()
    np.save(os.path.join(OUT, "val_inputs.npy"), xs)
    np.save(os.path.join(OUT, "val_outputs.npy"), ys)
    return path, n_params


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--final", action="store_true")
    ap.add_argument("--seeds", type=int, default=1, help="repeat CV with seeds 0..N-1")
    a = ap.parse_args()
    torch.set_num_threads(8)
    d = load_all()
    os.makedirs(OUT, exist_ok=True)
    if a.final:
        path, n = export(d)
        print(f"exported {path}  ({n} parameters)")
    else:
        texts, stats = [], []
        for seed in range(a.seeds):
            text, m = cross_validate(d, seed)
            print(text)
            texts.append(text)
            acc = np.trace(m) / m.sum()
            bal = np.mean([m[i, i] / m[i].sum() for i in range(len(CLASSES))])
            stats.append((acc, bal, m[2, 1], m[0, 2]))
        if a.seeds > 1:
            s = np.array(stats, float)
            summary = [f"== {a.seeds} seeds, pooled leave-one-round-out",
                       "seed   accuracy  balanced  present->seated  empty->present"]
            summary += [f"{i:4d}   {r[0]:8.2%}  {r[1]:8.2%}  {int(r[2]):15d}  {int(r[3]):14d}"
                        for i, r in enumerate(s)]
            summary.append(f"mean   {s[:,0].mean():8.2%}  {s[:,1].mean():8.2%}  "
                           f"{s[:,2].mean():15.0f}  {s[:,3].mean():14.0f}")
            summary.append(f"range  {s[:,0].min():.2%}-{s[:,0].max():.2%} accuracy, "
                           f"{s[:,1].min():.2%}-{s[:,1].max():.2%} balanced")
            texts.append("\n".join(summary))
            print(texts[-1])
        name = "cnn_cv.txt" if a.seeds == 1 else "cnn_cv_seeds.txt"
        with open(os.path.join(OUT, name), "w") as f:
            f.write("\n\n".join(texts) + "\n")


if __name__ == "__main__":
    main()
