# Collection procedure — Gate 2

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
   it should read **49.9–50.2°** down, roll ~162°. Anything else: the rig
   has moved; stop and re-check aim before recording.

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
through with the arrow keys and correct it: `e`/`p` label a frame, `m`
then `e`/`p` labels a whole range, `w` saves. The question is always **"is
someone in the sensor's view?"**, not "is someone in the room": the webcam
sees the whole room, the sensor only the desk and chair from above, so a
person the camera sees clearly can still be an `empty` frame. Judge by the
heatmaps; use the video to settle doubt (2026-11-22, "Evening round; video
sync confirmed"). Commit the `.labels.csv.gz`; never the video.
