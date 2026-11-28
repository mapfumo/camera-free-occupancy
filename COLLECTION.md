# Collection procedure — Gate 2

> **2026-11-28: the dataset is being re-recorded in a new pose.** See
> [The redo](#the-redo-in-the-current-pose--from-2026-11-28) at the end for
> the round list. Everything else below still applies.

How to record a labelled session. Every rule here came out of a measurement;
the reasons are in [`NOTES.md`](NOTES.md) under the date given.

## Classes

| Class | Meaning |
|-------|---------|
| `empty` | Nobody in view |
| `seated` | One person in the chair at the desk |
| `present` | Someone in view but not seated: standing at the desk, reaching across it, walking past |

Standing and walking are one class because at the rig's 49.9° tilt a
standing head is out of frame (2026-11-22, "Class set narrowed").

## Labels

`--label <class>/<chair>/<daypart>`. The class is always the part before
the first slash. The chair state records where the chair was, so its effect
can be checked for later instead of guessed at; the daypart (`am`, `pm`,
`eve`) lets the time-of-day spread be checked without storing clock times.

| Chair | Meaning |
|-------|---------|
| `marked` | On all three marks: base spot, leg tape, seat edge aligned |
| `in` | Pushed in under the desk |
| `out` | Pulled back ~50 cm from the mark |
| `turned` | On the spot but swivelled ~45° |
| `away` | Out of view entirely |

Examples: `empty/marked/am`, `empty/out/pm`, `seated/marked/pm`,
`present/away/eve`.

## Before every session

1. **Lights in the session's standard state.** Pick one and hold it for
   every capture, for every class. A lamp is both a warm object for the
   thermal sensor and correlated with someone being there — the shortcut
   the light sensor was excluded to avoid. (2026-09-26)
2. **Nothing warm on the desk or chair.** No mug, laptop or heater in view.
   A hot mug in the bottom rows reads exactly like a person.
   (2026-11-22, "Re-aim")
3. **10 minutes empty before any `empty` recording** if anyone has sat in
   the chair. The seat holds a fixed +0.5 °C patch for 7 minutes after a
   10-minute sit; a model will learn it as "someone just left".
   (2026-11-22, "Settling time")
4. **Don't touch the rig.** The session prints its mount angle at start —
   it should be within ±1° of the reference pose (`orientation::MOUNT_*`;
   see "The redo" below for the current values; the original dataset was
   49.9–50.2° down, roll ~162°). Anything else: the rig has moved; stop,
   run `tilt_check` and `align`, and re-set it before recording.

## Recording

```bash
cd firmware
cargo run --release --bin fused_stream \
  | python3 ../tools/heatmap_viewer.py --record ../sessions/2026-11-23_01_empty-out.csv \
      --label empty/out/am
gzip ../sessions/2026-11-23_01_empty-out.csv     # after the session
```

- **One CSV per session.** Name it `YYYY-MM-DD_NN_<class>-<chair>.csv` —
  date and that day's session number, no clock time. Gzip it when the
  session ends; git only accepts the `.csv.gz`. Columns are documented in
  [`sessions/README.md`](sessions/README.md).
- The recorder refuses to append to a file with a different column layout,
  and stores only seconds since the session started — never wall-clock
  time, since the dataset is published.
- **1–2 minutes per session** (300–600 frames at 5 Hz). Many short
  sessions across days beat one long one.
- **Start, then get into position.** Discard the first ~15 s when training:
  it contains you walking out or sitting down. Frame 1 is always all-invalid.
- **Watch stderr.** It prints `mount tilt … at session start`. A
  `WARNING: mount tilt moved` line means the rig moved and *stayed* moved
  (a 5-second median, so footsteps shaking the boom don't trigger it) —
  discard everything from that frame on.

## Vary the chair on purpose

A chair returned to its marks still lands ~10 cm differently each time —
measured as a one-column shift in the ToF (2026-11-22, "Chair marks").
Real chairs move far more than that. So rather than pretend it is fixed:

- **`empty`:** cycle through `marked`, `in`, `out`, `turned`, `away` across
  sessions. The model has to learn that a chair's position is not a person.
- **`seated`:** sit where you naturally would; label `seated/marked` if you
  started from the marks, otherwise the nearest description.
- **`present`:** vary it too, and vary *what* you do — stand at the desk,
  reach across, walk past on either side.

Aim for every class to appear with more than one chair state. A class that
only ever has the chair `away` lets the model cheat on the chair.

## Things that must not track the class

Each of these, if it differs between classes, becomes what the model learns:

- **Distance.** Don't always record one class nearer the sensor than
  another — apparent temperature falls with range. (2026-09-26)
- **Lighting.** Held constant, above.
- **Time of day / sunlight.** Sunlit patches are warm and drift; spread
  every class across different times and weather. (2026-09-26)
- **Chair state.** Spread, above.

## Ground truth

The webcam on the monitor is aimed at the chair and host-timestamped; run it
alongside a session when a label is in any doubt. Not needed for clean
single-class sessions.

**Required for walking-past sessions.** A person walking past is in view only
part of the time — about half the frames in session 12 show an empty room.
A session-level `present` label is wrong for those frames, so walking
sessions need per-frame labels, and the video is where they come from.

Add `--video` to the recording command:

```bash
cargo run --release --bin fused_stream \
  | python3 ../tools/heatmap_viewer.py --record ../sessions/2026-11-23_04_present-marked.csv \
      --label present/marked/am --video ../sessions/video/2026-11-23_04_present-marked.mkv
```

The video starts and stops with the recording and writes a `.sync.json`
beside it holding one number, the offset between the two clocks
(`video_s = host_s + host_s_to_video_s`, typically about −0.34 s). **Video
is local only:** `sessions/video/` is git-ignored, and only labels derived
from it are committed. The project is camera-free; the footage is a
labelling aid, not data.

**When trimming a video session**, add the original `host_s` of the first
kept frame to the `.sync.json` as `"trim_host_s"` — the committed file
re-zeroes `host_s`, and the labelling tool needs the trim point to find the
matching video frame.

### Labelling frames

```bash
python3 tools/label_frames.py sessions/2026-11-22_22_present-marked.csv.gz \
    --video sessions/video/2026-11-22_22_present-marked.mkv
```

Press `a` to pre-fill every frame from the sensor signature, then step
through with the arrow keys and correct it. The pre-fill only knows
`present` or `empty`, and it reads a seat warmed by someone who has just
left as `present` — in any session that includes sitting, both are wrong
until fixed (2026-11-22, session 23). Correct it: `e`/`p` label a frame, `m`
then `e`/`p` labels a whole range, `w` saves. The question is always **"is
someone in the sensor's view?"**, not "is someone in the room": the webcam
sees the whole room, the sensor only the desk and chair from above, so a
person the camera sees clearly can still be an `empty` frame. Judge by the
heatmaps; use the video to settle doubt (2026-11-22, "Evening round; video
sync confirmed"). Commit the `.labels.csv.gz`; never the video.

## The redo in the current pose — from 2026-11-28

**Why.** The head moved (shield work, then a bump) and the deployed model
misread a sit as `present` for 30 s at the new pose (NOTES, 2026-11-28).
Rather than buy a precision mount, the whole dataset is recorded again in
the current setup, and the limitation (a non-rigid mount, a model
measurably sensitive to a few degrees of pose) is documented. The original
70 sessions stay in `sessions/`; whether adding them to training helps is
tested, not assumed. New-pose sessions are the ones dated 2026-11-28 and
later.

**Before the first session (once):**

1. Move the stand so nobody can walk into the board.
2. Set the pose with `tilt_check` (angles on the LCD), lock it, then
   `align` for yaw. Record the values in `orientation::MOUNT_*` and here.
3. One `empty-marked` session first: the new pose's empty reference.

**Before every session:** a glance at the start-of-session `mount tilt`
line (or `tilt_check`): within ±1° of the reference. If not, stop and
re-set; don't record into a moved pose.

**Reference pose:** _to be filled in when set_.

**One round = 12 sessions, ~3 min each** (empty 2.5 min, others 3 min),
about 35 min of recording plus the 10-minute empty wait after any sitting.
Sessions stay short on purpose: frames within one are near-copies, so
variety comes from more sessions, not longer ones.

| # | Session | Label | Video |
|---|---|---|---|
| 1 | Empty, chair on its marks | `empty/marked/<dp>` | |
| 2 | Empty, chair pulled out | `empty/out/<dp>` | |
| 3 | Empty, chair turned | `empty/turned/<dp>` | |
| 4 | Empty, chair away | `empty/away/<dp>` | |
| 5 | Seated, normal | `seated/marked/<dp>` | |
| 6 | Seated, hunched over the desk | `seated/marked/<dp>` | |
| 7 | Seated, leaning back | `seated/marked/<dp>` | |
| 8 | Standing at the desk, reaching across | `present/out/<dp>` | |
| 9 | Standing bent over the keyboard | `present/marked/<dp>` | |
| 10 | Walking past, both sides | `present/marked/<dp>` | **yes**, per-frame labels |
| 11 | Walking past, again | `present/away/<dp>` | **yes**, per-frame labels |
| 12 | Transitions: walk in → sit → stand → bent over → sit → leave, repeated | `present/marked/<dp>` | **yes**, per-frame labels incl. `seated` |

Do the empty sessions (1–4) first, before anyone has sat, so the 10-minute
wait is only needed if the order changes. Sessions 10–12 need the labeller
afterwards (`tools/label_frames.py`); session 12 is the one the old dataset
lacked — sitting down and standing up, where the live tests failed.

**Six rounds:** `eve`, `am`, `pm` on two different days, as before. Start:
2026-11-28 `eve`.

**Then:** retrain on the new rounds (leave-one-round-out), and on new + old
together; score both on held-out new rounds and the live test in `trials/`;
deploy the better one; rerun the Gate 4 exit test.
