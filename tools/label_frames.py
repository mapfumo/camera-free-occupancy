"""Per-frame labelling for a recorded session.

A session-level label is wrong for part of some sessions: a walking-past
session is `present` only while the person is actually in the sensor's view.
This tool steps through a session's frames and records a label per frame.

    python3 tools/label_frames.py sessions/2026-11-22_22_present-marked.csv.gz \
        --video sessions/video/2026-11-22_22_present-marked.mkv

Shows, for the current frame: the ToF distance grid, the thermal grid (excess
over the frame median), and — with --video — the webcam frame at the same
instant. Below, a timeline of the whole session: the person signature from the
sensors, and the labels so far.

The question for every frame is "is someone in the *sensor's* view?" — not
"is someone in the room". The webcam sees the whole room; the sensor sees only
the desk and chair from above. Judge by the heatmaps; use the video to settle
doubt.

Keys:
  left / right        previous / next frame
  down / up           back / forward 1 s (5 frames)
  pgdn / pgup         back / forward 5 s
  home / end          first / last frame
  e  s  p  u          label empty / seated / present / unsure, then advance
  m                   mark: the next e/s/p/u labels the whole range from the
                      mark to here (inclusive), instead of one frame
  a                   auto-suggest: fill every *unlabelled* frame from the
                      sensor signature (person -> session class, else empty).
                      A starting point only — step through and correct it.
                      It never suggests `seated`, and after someone has sat
                      down the warm seat keeps it saying `present` in an
                      empty room for minutes — check every gap.
  w                   write labels
  q                   write and quit
  click timeline      jump to that frame

Output: <session>.labels.csv.gz beside the session — columns frame, host_s,
label. Blank label = not yet labelled; `unsure` frames are meant to be left
out of training. An existing labels file is loaded, so labelling can resume.

Video sync: the session's host_s is re-zeroed when it is trimmed, so the
`.sync.json` beside the video holds both the recorder's clock offset and the
trim point: video_s = host_s + trim_host_s + host_s_to_video_s.

--render N OUT.png draws frame N's view to a file without opening a window.
"""

import argparse
import csv
import gzip
import json
import os
import subprocess
import sys

import numpy as np

LABEL_KEYS = {"e": "empty", "s": "seated", "p": "present", "u": "unsure"}
LABEL_COLOR = {"empty": "#2a78d6", "seated": "#eb6834", "present": "#1baf7a",
               "unsure": "#8a8985", "": "#e4e3df"}
TOF_VALID = (5, 9)
NEAR_MM = 620          # same thresholds as tools/class_look.py
HOT_EXCESS_C = 1.5
VIDEO_W, VIDEO_H = 320, 240
VIDEO_FPS = 30
SURFACE = "#fcfcfb"


# ---------------------------------------------------------------- data

class Session:
    def __init__(self, path: str) -> None:
        self.path = path
        with gzip.open(path, "rt") if path.endswith(".gz") else open(path) as f:
            rows = list(csv.DictReader(f))
        if not rows:
            raise SystemExit(f"{path}: no frames")
        self.cls = rows[0]["label"].split("/")[0]
        self.frame = [int(r["frame"]) for r in rows]
        self.host_s = np.array([float(r["host_s"]) for r in rows])
        self.dist = np.array([[int(r[f"d{i:02d}"]) for i in range(64)] for r in rows])
        self.status = np.array([[int(r[f"s{i:02d}"]) for i in range(64)] for r in rows])
        heat = np.array([[int(r[f"h{i:02d}"]) for i in range(64)] for r in rows]) / 100
        self.excess = heat - np.median(heat, axis=1, keepdims=True)
        self.valid = np.isin(self.status, TOF_VALID)
        self.near = (self.valid & (self.dist < NEAR_MM)).sum(1)
        self.hot = (self.excess > HOT_EXCESS_C).sum(1)
        self.n = len(rows)

    @property
    def person(self) -> np.ndarray:
        return (self.near >= 3) | (self.hot >= 2)

    @property
    def labels_path(self) -> str:
        base = self.path[:-3] if self.path.endswith(".gz") else self.path
        return base[:-4] + ".labels.csv.gz"


def load_labels(session: Session) -> list:
    labels = [""] * session.n
    if os.path.exists(session.labels_path):
        index = {f: i for i, f in enumerate(session.frame)}
        with gzip.open(session.labels_path, "rt") as f:
            for r in csv.DictReader(f):
                i = index.get(int(r["frame"]))
                if i is not None:
                    labels[i] = r["label"]
    return labels


def save_labels(session: Session, labels: list) -> None:
    tmp = session.labels_path + ".tmp"
    with gzip.open(tmp, "wt", newline="") as f:
        w = csv.writer(f)
        w.writerow(["frame", "host_s", "label"])
        for fr, t, lab in zip(session.frame, session.host_s, labels):
            w.writerow([fr, f"{t:.3f}", lab])
    os.replace(tmp, session.labels_path)


def auto_suggest(session: Session, labels: list) -> int:
    filled = 0
    for i, person in enumerate(session.person):
        if not labels[i]:
            labels[i] = session.cls if person else "empty"
            filled += 1
    return filled


def load_video(path: str, video_times: np.ndarray):
    """One RGB frame per sensor frame, nearest in time. None where outside the video."""
    sync = json.load(open(path + ".sync.json"))
    t = video_times + sync.get("trim_host_s", 0.0) + sync["host_s_to_video_s"]
    want = np.round(t * VIDEO_FPS).astype(int)
    proc = subprocess.Popen(
        ["ffmpeg", "-v", "error", "-i", path, "-vf",
         f"fps={VIDEO_FPS},scale={VIDEO_W}:{VIDEO_H}", "-pix_fmt", "rgb24",
         "-f", "rawvideo", "-"], stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL)  # decoding stops early by design: broken pipe
    size = VIDEO_W * VIDEO_H * 3
    by_index = {}
    needed = set(int(w) for w in want if w >= 0)
    k = 0
    while True:
        buf = proc.stdout.read(size)
        if len(buf) < size:
            break
        if k in needed:
            by_index[k] = np.frombuffer(buf, np.uint8).reshape(VIDEO_H, VIDEO_W, 3)
        k += 1
        if k > max(needed, default=-1):
            break
    proc.stdout.close()
    proc.wait()
    if not by_index:
        raise SystemExit(f"{path}: no video frames decoded")
    return [by_index.get(int(w)) for w in want]


# ---------------------------------------------------------------- view

class Labeller:
    def __init__(self, session: Session, labels: list, video=None) -> None:
        import matplotlib.pyplot as plt
        from matplotlib.colors import LinearSegmentedColormap, TwoSlopeNorm

        self.plt = plt
        # matplotlib's own shortcuts collide with ours ('s' opens save-figure,
        # 'p' pans, arrows step view history, 'q' closes) — switch them all off.
        for key in [k for k in plt.rcParams if k.startswith("keymap.")]:
            plt.rcParams[key] = []
        self.s, self.labels, self.video = session, labels, video
        self.i, self.mark, self.dirty = 0, None, False

        dist_cm = LinearSegmentedColormap.from_list(
            "near", ["#7a2e0e", "#c2501f", "#eb6834", "#f3a37f", "#fbe0d2"])
        dist_cm.set_bad("#e4e3df")
        heat_cm = LinearSegmentedColormap.from_list(
            "div", ["#1c5cab", "#86b6ef", "#f0efec", "#f19a96", "#b8302f"])

        ncols = 3 if video is not None else 2
        self.fig = plt.figure(figsize=(4 * ncols, 6.2), facecolor=SURFACE)
        gs = self.fig.add_gridspec(2, ncols, height_ratios=[4, 1.3], hspace=0.35)
        self.ax_d = self.fig.add_subplot(gs[0, 0])
        self.ax_h = self.fig.add_subplot(gs[0, 1])
        self.ax_v = self.fig.add_subplot(gs[0, 2]) if video is not None else None
        self.ax_t = self.fig.add_subplot(gs[1, :])
        self.ax_t.set_facecolor(SURFACE)

        self.im_d = self.ax_d.imshow(np.zeros((8, 8)), cmap=dist_cm, vmin=300, vmax=1800)
        self.im_h = self.ax_h.imshow(np.zeros((8, 8)), cmap=heat_cm,
                                     norm=TwoSlopeNorm(0, -3, 3))
        self.ax_d.set_title("ToF distance (dark = near; grey = no return)", fontsize=9)
        self.ax_h.set_title("Thermal °C above frame median", fontsize=9)
        for ax in (self.ax_d, self.ax_h):
            ax.set_xticks([]); ax.set_yticks([])
        if self.ax_v is not None:
            self.im_v = self.ax_v.imshow(np.zeros((VIDEO_H, VIDEO_W, 3), np.uint8))
            self.ax_v.set_title("Webcam (whole room — sensor sees only the desk)", fontsize=9)
            self.ax_v.set_xticks([]); self.ax_v.set_yticks([])

        # timeline: signature on top, label band below
        t = np.arange(session.n)
        self.ax_t.plot(t, session.near, color="#52514e", lw=1, label="ToF zones < 620 mm")
        self.ax_t.plot(t, session.hot, color="#b8302f", lw=1, label="thermal zones > +1.5 °C")
        top = max(session.near.max(), session.hot.max(), 5)
        self.band_y = -0.18 * top
        self.band = self.ax_t.scatter(t, np.full(session.n, self.band_y), marker="s",
                                      s=14, linewidths=0, c=self._band_colors())
        self.cursor = self.ax_t.axvline(0, color="#0b0b0b", lw=1)
        self.mark_line = self.ax_t.axvline(0, color="#0b0b0b", lw=1, ls="--", visible=False)
        self.ax_t.set_xlim(-1, session.n)
        self.ax_t.set_ylim(self.band_y * 1.8, top * 1.08)
        self.ax_t.set_yticks([])
        self.ax_t.set_xlabel("frame (5 per second)   band: blue empty · orange seated · "
                             "green present · grey unsure/unlabelled", fontsize=8)
        self.ax_t.legend(loc="upper right", fontsize=7, frameon=False)
        for sp in ("top", "right", "left"):
            self.ax_t.spines[sp].set_visible(False)

        self.fig.canvas.mpl_connect("key_press_event", self.on_key)
        self.fig.canvas.mpl_connect("button_press_event", self.on_click)
        self.draw()

    def _band_colors(self):
        return [LABEL_COLOR.get(lab, "#e4e3df") for lab in self.labels]

    def draw(self) -> None:
        s, i = self.s, self.i
        d = np.ma.masked_where(~s.valid[i], s.dist[i]).reshape(8, 8)
        self.im_d.set_data(d)
        self.im_h.set_data(s.excess[i].reshape(8, 8))
        if self.ax_v is not None:
            frame = self.video[i]
            self.im_v.set_data(frame if frame is not None
                               else np.zeros((VIDEO_H, VIDEO_W, 3), np.uint8))
        self.cursor.set_xdata([i, i])
        self.mark_line.set_visible(self.mark is not None)
        if self.mark is not None:
            self.mark_line.set_xdata([self.mark, self.mark])
        self.band.set_color(self._band_colors())
        done = sum(1 for lab in self.labels if lab)
        lab = self.labels[i] or "—"
        mark = f"   mark at {self.mark}" if self.mark is not None else ""
        self.fig.suptitle(
            f"{os.path.basename(s.path)}   frame {i + 1}/{s.n} (#{s.frame[i]})  "
            f"t {s.host_s[i]:6.1f} s   label: {lab}   "
            f"sensor: near {s.near[i]}, hot {s.hot[i]}   labelled {done}/{s.n}{mark}"
            + ("   *unsaved*" if self.dirty else ""), fontsize=9)
        self.fig.canvas.draw_idle()

    def go(self, i: int) -> None:
        self.i = int(np.clip(i, 0, self.s.n - 1))

    def on_key(self, ev) -> None:
        k = ev.key
        moves = {"right": 1, "left": -1, "up": 5, "down": -5,
                 "pageup": 25, "pagedown": -25}
        if k in moves:
            self.go(self.i + moves[k])
        elif k == "home":
            self.go(0)
        elif k == "end":
            self.go(self.s.n - 1)
        elif k in LABEL_KEYS:
            lab = LABEL_KEYS[k]
            if self.mark is not None:
                a, b = sorted((self.mark, self.i))
                for j in range(a, b + 1):
                    self.labels[j] = lab
                self.mark = None
            else:
                self.labels[self.i] = lab
                self.go(self.i + 1)
            self.dirty = True
        elif k == "m":
            self.mark = None if self.mark == self.i else self.i
        elif k == "a":
            if auto_suggest(self.s, self.labels):
                self.dirty = True
        elif k == "w":
            self.save()
        elif k == "q":
            self.save()
            self.plt.close(self.fig)
            return
        self.draw()

    def on_click(self, ev) -> None:
        if ev.inaxes is self.ax_t and ev.xdata is not None:
            self.go(round(ev.xdata))
            self.draw()

    def save(self) -> None:
        save_labels(self.s, self.labels)
        self.dirty = False
        print(f"wrote {self.s.labels_path}", file=sys.stderr)


# ---------------------------------------------------------------- main

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("session", help="sessions/<name>.csv.gz")
    ap.add_argument("--video", help="the session's .mkv (needs its .sync.json)")
    ap.add_argument("--render", nargs=2, metavar=("N", "OUT_PNG"),
                    help="draw frame N (1-based) to a PNG and exit")
    args = ap.parse_args()

    if args.render:
        import matplotlib
        matplotlib.use("Agg")
    session = Session(args.session)
    labels = load_labels(session)
    video = load_video(args.video, session.host_s) if args.video else None
    if video is not None:
        missing = sum(v is None for v in video)
        if missing:
            print(f"note: {missing} frames fall outside the video", file=sys.stderr)

    lab = Labeller(session, labels, video)
    if args.render:
        lab.go(int(args.render[0]) - 1)
        lab.draw()
        lab.fig.savefig(args.render[1], dpi=100, facecolor=SURFACE)
        return 0
    print(__doc__.split("Keys:")[1].split("Output:")[0], file=sys.stderr)
    lab.plt.show()
    if lab.dirty:
        lab.save()
    return 0


if __name__ == "__main__":
    sys.exit(main())
