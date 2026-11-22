# Sessions — the Gate 2 dataset

Labelled recordings from `fused_stream`, one file per session, recorded
following [`../COLLECTION.md`](../COLLECTION.md).

## Files

`YYYY-MM-DD_NN_<class>-<chair>.csv.gz` — date, session number that day,
class and chair state. Compressed with `gzip` after recording; uncompressed
`.csv` files here are ignored by git, so a session can't be committed
before it's compressed.

```bash
gzip sessions/2026-11-23_01_empty-marked.csv
```

## Columns

One row per paired frame, 5 Hz.

| Column | Meaning |
|--------|---------|
| `host_s` | Seconds since recording started, host clock |
| `label` | `<class>/<chair>/<daypart>`, e.g. `empty/out/pm` |
| `frame` | Frame number from the device. Frame 1 is always all-invalid |
| `device_ms` | Milliseconds since boot, device clock — use this for spacing |
| `die_cC` | AMG8833 thermistor, centidegrees C |
| `tilt_deg` | Optical axis below horizontal, one IMU sample. Reference 49.9–50.2 |
| `roll_deg` | Rotation about the optical axis. Reference ~162 |
| `d00`–`d63` | VL53L5CX distance, mm, **raw** — not a measurement where invalid |
| `s00`–`s63` | VL53L5CX target status. 5 and 9 are valid; anything else is not |
| `h00`–`h63` | AMG8833 temperature, centidegrees C |

All three grids are row-major in the same world frame: index `rc` is row
`r`, column `c`, seen from behind the sensor looking out — row 0 at the top,
column 0 on the left. Both sensors' cells at the same index look at the same
patch of room (approximately: the ToF field is ~45°, the thermal ~60°).

## Reading

```python
import pandas as pd
df = pd.read_csv("sessions/2026-11-23_01_empty-marked.csv.gz")
cls = df["label"].str.split("/").str[0]
```

## Privacy

No wall-clock times are stored — only seconds since a session started, the
date in the file name and a coarse daypart (`am`, `pm`, `eve`). An 8×8
thermal and depth grid cannot identify anyone; the timestamps were the only
thing in a row that said anything about a person's routine.
