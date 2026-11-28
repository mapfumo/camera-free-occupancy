"""Keep the last N seconds of a recorded session, re-zero its clock, gzip it.

Sessions are recorded a little long (start, walk out or sit down, settle)
and trimmed to a fixed length afterwards, so every session of a class is
the same size and none contains the setup. COLLECTION.md: empty 150 s,
seated / present 188 s (751 and 940 frames at 5 Hz, as the original
dataset).

    python3 tools/trim_session.py sessions/X.csv --seconds 150
    python3 tools/trim_session.py sessions/X.csv --seconds 188 --video sessions/video/X.mkv

- keeps rows whose host_s is within the last --seconds of the recording;
- re-zeroes host_s to the first kept frame;
- with --video, writes the original host_s of that frame into the video's
  .sync.json as "trim_host_s", so tools/label_frames.py still finds the
  matching video frame;
- writes X.csv.gz and removes X.csv.

--keep-all skips trimming (whole-session transition recordings, labelled
per frame) but still gzips.
"""

import argparse
import csv
import gzip
import json
import os
import sys


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("csv")
    ap.add_argument("--seconds", type=float)
    ap.add_argument("--keep-all", action="store_true")
    ap.add_argument("--video", help="the session's .mkv; its .sync.json gets trim_host_s")
    a = ap.parse_args()
    if not a.keep_all and not a.seconds:
        sys.exit("give --seconds or --keep-all")
    with open(a.csv, newline="") as f:
        r = csv.DictReader(f)
        fields, rows = r.fieldnames, list(r)
    if not rows:
        sys.exit("no frames")
    end = float(rows[-1]["host_s"])
    kept = rows if a.keep_all else [x for x in rows if float(x["host_s"]) >= end - a.seconds]
    t0 = float(kept[0]["host_s"])
    for x in kept:
        x["host_s"] = f"{float(x['host_s']) - t0:.3f}"
    out = a.csv + ".gz"
    with gzip.open(out, "wt", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fields)
        w.writeheader()
        w.writerows(kept)
    if a.video:
        sync = os.path.splitext(a.video)[0] + ".sync.json"
        with open(sync) as f:
            s = json.load(f)
        s["trim_host_s"] = s.get("trim_host_s", 0.0) + t0
        with open(sync, "w") as f:
            json.dump(s, f, indent=2)
    os.remove(a.csv)
    tilts = sorted(float(x["tilt_deg"]) for x in kept if x.get("tilt_deg") not in (None, "", "nan"))
    rolls = sorted(float(x["roll_deg"]) % 360 for x in kept if x.get("roll_deg") not in (None, "", "nan"))
    pose = f", tilt {tilts[len(tilts)//2]:.2f}, roll {rolls[len(rolls)//2]:.2f}" if tilts else ""
    print(f"{out}: kept {len(kept)} of {len(rows)} frames ({len(kept)/5:.0f} s){pose}")


if __name__ == "__main__":
    main()
