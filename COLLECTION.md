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

`--label <class>/<chair>`. The class is always the part before the slash;
the part after records where the chair was, so chair placement can be
checked for later instead of guessed at.

| Chair | Meaning |
|-------|---------|
| `marked` | On all three marks: base spot, leg tape, seat edge aligned |
| `in` | Pushed in under the desk |
| `out` | Pulled back ~50 cm from the mark |
| `turned` | On the spot but swivelled ~45° |
| `away` | Out of view entirely |

Examples: `empty/marked`, `empty/out`, `seated/marked`, `present/away`.

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
  | python3 ../tools/heatmap_viewer.py --record ../sessions/2026-11-23_0930_empty-out.csv \
      --label empty/out
```

- **One CSV per session.** Name it `YYYY-MM-DD_HHMM_<class>-<chair>.csv`.
  The recorder refuses to append to a file with a different column layout.
- **1–2 minutes per session** (300–600 frames at 5 Hz). Many short
  sessions across days beat one long one.
- **Start, then get into position.** Discard the first ~15 s when training:
  it contains you walking out or sitting down. Frame 1 is always all-invalid.
- **Watch stderr.** It prints `mount tilt … at session start`. A
  `WARNING: mount tilt moved` line means the rig was knocked mid-session —
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
