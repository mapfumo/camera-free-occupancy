# TODO

Working list. Newest thinking wins — if something here contradicts
`occupancy-sensor-project-plan.md` or the roadmap, those are the authority and
this needs fixing.

Ordered by when it matters, not by size.

---

## Finish Gate 1 (Phase 1)

- [x] ~~Solder headers to the SATEL and build the sensor rig.~~ Built
      2026-08-03 on the ElectroCookie proto shield: AMG8833 on a socket strip,
      SATEL along the edge, both coplanar and facing the same way. Photo in
      `../images/shield.jpg`.
- [x] ~~Re-verify the rig on hardware after the break.~~ Done 2026-09-26.
      Rust 1.92, probe-rs 0.30, same ST-LINK serial; `i2c_scan`, `tof_range`
      and `thermal_probe` all good. AMG8833 is at 0x69, not 0x68 — expected
      (SparkFun straps AD0 high), but four documents said otherwise and are
      now fixed.
- [ ] **Use the heatmap viewer, not the RTT text dump.** Still the one tool
      built for this that has never been used in anger. Every aiming decision
      on 2026-09-26 was made by reading 8x8 integer grids in a terminal, which
      is exactly the slow path it was written to replace.
- [x] ~~Check the thermal gap.~~ **Passed 2026-11-22.** Twelve-minute
      soak from cold: ToF silicon 42 → ~47 °C, AMG die flat (−0.3 °C), no
      zone rose more than one LSB (+0.25 °C). See NOTES, "Thermal soak".
- [x] ~~Fix the tilt.~~ Set to **15° below horizontal** on 2026-09-26,
      derived rather than guessed: a 180 cm person at 2 m spans +4.0° to
      −39.7° against a 45° field, midpoint −17.9°, biased shallow to keep the
      head in frame. Valid zones went 24-26 → 38-42 of 64.
- [x] ~~Move the Philips fan out of the near field / boom clear of the door
      jamb.~~ Moot since 2026-11-22: rig remounted at the desk (back-left
      clamp, board on the boom, ~20° down, facing the chair). Keep "nothing
      within ~1 m of the field" as a precondition for every capture.
- [ ] **Measure usable depth.** Detection is solid at ~1.5 m and absent at
      ~2 m, but whether that is the cone or the range is unknown. Walk out in
      half-metre steps and record where it is lost. That number decides how
      big a zone the product can honestly claim.
- [x] ~~Orientation characterisation, AMG8833.~~ **Corrected 2026-11-22:
      the raw grid is already in the world frame** — row 0 at the top, column
      0 on your left, seen from behind. Verified by a live sweep in the viewer,
      both axes. September's "vertically inverted" was wrong; see NOTES,
      "AMG8833 orientation — corrected".
- [x] ~~Establish the ToF orientation and check registration.~~ Done
      2026-11-22. ToF is `flip_cols` only — raw row 0 is the top, raw column
      0 is on the right, matching UM2884 §2.2. Registration settled with a
      hand ~17 cm in front of the board on the pole side: both sensors put
      it in the high columns, which is the pole side seen from behind. See
      NOTES, "Re-aim, and registration settled properly". The procedure
      below is kept for the next remount.
      **Procedure** (world frame and mechanism in
      `firmware/src/orientation.rs`). Run `tof_range` into the heatmap viewer,
      stand *behind* the sensor's line of sight to judge left/right, and move
      the target one way at a time against open background at ~1.2 m:
      1. Raise the target. If the blob moves along a **row** rather than a
         column, set `transpose: true` and redo steps 2–3 after reflashing.
      2. Raised target heads toward screen **bottom** → `flip_rows: true`.
      3. Target moved to *your* left (seen from behind) heads toward screen
         **right** → `flip_cols: true`.
      4. Set the fields in `vl53l5cx::ORIENTATION`, reflash, confirm the
         blob now moves with the target.
      5. Run `thermal_probe` with the same target: both blobs should land on
         the same cells. That is the registration check.
      **Left/right is the step that goes wrong.** Holding a target out in
      front means facing the sensor, which mirrors "your left". On
      2026-11-22 it was reported reversed both with and without the flip.
      Skip step 3's judgement entirely: hold an open hand ~20 cm in front of
      the board on the pole side, standing on that same side, and check both
      sensors put it in the high columns. Never let the holder's body sit on
      the opposite side of the target — it is the strongest thermal source in
      the room and will "agree" with a wrong flip.
      For reference, UM2884 §2.2 says the lens flips both ways (zone 0 sees
      the scene's top right); how the breakout is mounted decides the rest.
- [x] ~~The ToF can't see a seated subject in the current pose.~~
      Resolved by the ~20° re-tilt (2026-11-22): seated vs empty now
      changes ~22 ToF zones, mostly invalid → ~0.5 m, because the empty
      black chair back returns nothing. See NOTES, "After the ~20°
      re-tilt".
- [x] ~~Fix the chair for collection.~~ Marked 2026-11-22, then tested:
      marks return it within ~10 cm, which the ToF resolves as a one-column
      shift. **Superseded: the chair is varied on purpose and its state
      recorded in the label** — see [`COLLECTION.md`](COLLECTION.md).
- [x] ~~Correct orientation in the driver, not the viewer.~~ Mechanism in
      place 2026-11-22: `orientation.rs`, applied in `Amg8833::read_frame`
      and in `tof_range` (distance and status together). AMG8833 confirmed
      identity on hardware the same day; ToF still identity pending the
      procedure above. `firmware/session.csv` (2026-08-02) was recorded in
      the old landscape pose; don't train on it.
- [ ] **Look into today's hardware check (2026-11-22).** Both sensors found
      and streaming after the break. Two loose ends: (1) the empty scene now
      gives only 5-8 valid ToF zones against 38-42 in September — the board
      was remounted on the stand that morning, so the aim has changed;
      re-check tilt and near-field clutter before reading anything into it; (2) one `tof_range` start failed
      `is_alive`, while a rescan straight after found 0x29 fine. Suspect
      a loose jumper; press on the SATEL wires while rescanning if it recurs.
      **Recurred 2026-11-23, other sensor:** the first attempt at morning
      session 07 (seated, leaning in to the desk) stopped at start with
      `no AMG8833 at 0x68 or 0x69`; `i2c_scan` straight after found both
      0x29 and 0x69, and the retry ran clean. Two start-up dropouts, one per
      sensor, both gone on retry: suspect the shared SCL/SDA jumpers or a
      breadboard contact rather than either breakout. Reseat before the
      Phase 6 multi-day run, where nobody is there to retry.
      **Third dropout, 2026-11-23 afternoon — and there are no jumpers:**
      the sensors sit on an Arduino-compatible shield, so "loose wire" is
      unlikely. First attempt at session 13 (`empty/in/pm`, nobody near the
      rig) failed `no AMG8833 at 0x68 or 0x69`; `i2c_scan` straight after
      found both; retry clean. New suspect: every session ends with
      `timeout` killing probe-rs mid-stream, which can leave a sensor
      mid-transfer holding SDA low, so the next start's probe fails. Not
      confirmed. Fix candidates: I2C bus recovery (9 SCL pulses + STOP)
      before probing, and retry the probe a few times before giving up.
      Also: on that failure `fused_stream` halts but probe-rs keeps running,
      so the whole 815 s timed out with zero frames — check for frames ~30 s
      after every launch.
      **Fix in, 2026-11-23:** `src/i2c_recovery.rs` (bus clear: up to 9
      SCL pulses until SDA releases, then STOP) runs before the I2C driver
      takes PB8/PB9, and `fused_stream` probes the AMG8833 up to 3 times,
      recovering the bus before each. It logs only when it acts —
      `I2C1 bus recovery (attempt n): sda_was_stuck …` or `AMG8833 found on
      attempt n` — so the next dropout tells us whether the stuck-SDA
      theory was right. Four timeout-killed restarts in a row: all clean,
      no recovery needed. Still open: port to `main.rs` for Phase 4, and
      make a give-up exit probe-rs instead of stalling.
      **Confirmed 2026-11-24**, start of morning session 03: SDA stuck low
      after session 02's timeout kill. Attempt 1 stopped clocking at the
      first SDA high (1 pulse) and the bus stayed busy — a 1 bit mid-byte,
      not a release; attempt 2 freed it (6 pulses) and the AMG answered.
      Fixed the same morning: always send all nine pulses when SDA is
      stuck. Ten varied-time timeout kills after the fix: all clean, none
      stuck, so the nine-pulse path is still unexercised on hardware —
      watch the log for `pulses` on the next recovery.
      **Separate fault, same morning:** session 09 (reaching across the
      desk) lost the ST-LINK at 144 s — kernel `error -71`, USB
      re-enumerated a second later. A host-side cable/connector glitch, not
      the sensor bus; the AMG dropout before it left no USB trace. Route the
      USB cable so reaching across the desk can't tug it, and for Phase 6
      don't depend on a debug probe staying attached for days.
- [ ] **Build a dual-modality target.** A hot water bottle taped behind a
      piece of card: a flat surface at a known distance for the ToF and a
      large warm area for the thermal, in the same place at the same instant.
      Makes grid agreement directly readable instead of inferred, and removes
      the two-probes-two-placements error that made today's column mapping
      ambiguous. Large area and moderate temperature beat small and hot —
      see NOTES on why a 450 °C soldering iron is a *worse* probe than a mug.
- [x] ~~Measure the mount angle in the new pose.~~ **49.9° below
      horizontal**, 2026-11-22, from the on-board accelerometer
      (`tilt_check`). The tape-measure method failed here — furniture hides
      the floor and the warm seat mimics the target. See NOTES, "Mount angle,
      measured by the board itself".
- [x] ~~Decide the class set for this pose.~~ **`empty` / `seated` /
      `present`**, decided 2026-11-22. A standing head is out of frame at
      49.9° down, so standing and walking merge into `present`. Product
      claim: workstation occupancy. See the project plan, Phase 2.
- [x] ~~Mark the ball head, clamp position and column extension.~~ Taped
      and marked 2026-11-22. `tilt_check` reads 49.9° down / roll 162° as the
      reference, so drift is now measurable rather than visual.
- [x] ~~Combined stream — both grids, one output.~~ Done 2026-11-22:
      `fused_stream` reads both on one bus at 5 Hz, thermal first then ToF,
      pairs them under one frame number with the device clock. Raw ToF
      distance *and* status are recorded, not collapsed to -1, so the
      invalid-zone decision stays open until training. The viewer shows both
      side by side, and `--record` writes one CSV row per pair
      (`d00..d63`, `s00..s63`, `h00..h63`). The first frame after start is
      all-invalid; drop `frame == 1`.
- [x] ~~Aim the AMG8833 and confirm it sees a warm target.~~ Done — hot mug
      peaked at 71.5 °C against a ~21 °C room, appearing and disappearing
      cleanly. Entered from the bottom-left, which fixes its orientation.
- [x] ~~Tag `phase1-bringup`.~~ Tagged 2026-11-22. Gate 1 closed.

## Gate 3 — Model

- [x] ~~Leave-one-round-out evaluation, baselines, first CNN.~~ 2026-11-24:
      CNN 99.1% (balanced 99.0%) on unseen rounds vs logistic regression
      98.2%; 0 false "occupied" on 24,772 empty-session frames. Fits the
      U585 at ~49 KiB flash / 5.75 KiB RAM; C model validated against
      PyTorch. NOTES, "First model, scored on unseen rounds".
- [x] ~~Seed check, then close Gate 3.~~ **Closed 2026-11-24**, tagged
      `phase3-model`: 99.08–99.10% over three seeds. Temporal input not
      pursued — Gate 4's debounce handles the pass-edge frames.
## Gate 4 — Live inference

- [x] ~~Generate the C model, link it, run it on the board.~~ 2026-11-24:
      `fused_stream` classifies every frame; parity with the PC 106/106,
      max logit diff 1e-5; 8.2 ms per inference with ICACHE on (18.5 off).
- [x] ~~Debounce rule.~~ 2026-11-24: 8 in a row (1.6 s), chosen by
      simulation on real prediction streams; 0 false empty↔occupied changes
      in 2.9 h, lag ≤ 2.6 s. NOTES, "Choosing the debounce rule".
- [x] ~~Live scripted test~~ (stage 2). 2026-11-24, 10.8 min: 0 false
      empty↔occupied changes, 12/12 caught, walk-pasts never `seated`;
      parity 3,255/3,255 all classes. One real error: ~16 s `seated` while
      standing bent over the keyboard (model, not debounce). Two entries
      3.4 / 4.2 s. NOTES, "Live scripted test".
- [x] ~~Two debouncers.~~ 2026-11-24: occupancy and class each 8 in a row;
      live worst entry 4.2 → 3.0 s, no new false changes offline or live.
      Rust matches the Python simulation on all 3,255 live frames.
      NOTES, "Two debouncers".
- [x] ~~Record `present` bent over the desk / keyboard, retrain.~~
      2026-11-24: session 13; on the live test the bent-over stretch goes
      0% → 86–99% `present`; seated recall −0.5 pt. Deployed (v2), parity
      on the board. NOTES, "Retraining for the bent-over posture".
- [ ] Second bent-over session in a pm/eve round (CV can then test the
      fix on an unseen round), plus hunched sitting to recover seated recall.
- [x] ~~**ILI9341 readout**~~ — 2026-11-28: on the panel over 40 cm Dupont
      leads at 5 MHz, mounted top corner of the pole facing the door, and
      drawn by `fused_stream` (state, occupied/vacant, confidence, heatmap).
      One slow frame per state change (212–220 ms), the rest < 50 ms. Two
      power faults on the way (display VCC open; AMG8833 3V3 at a standoff,
      now a soldered wire). NOTES, "Display on the panel…".
- [x] ~~Re-check tilt and empty scene after mounting.~~ 2026-11-28: the
      shield coming off had moved the head (56.6° / 184°, then ~10° of yaw).
      Realigned with `tilt_check` on the LCD and the new `align` mode; the
      pose kept is **45.4° / 167.2°** (`orientation::MOUNT_*`), and the
      model was re-checked live in it: stand + bend-over 17.6 s `present`
      without a break, seated / empty steady at p 0.96–0.99.
- [ ] **Gate 4 exit test**: walk in, out, back, reading the display from the
      door; then tag `phase4-live`. 2026-11-28: failed after a bump moved
      the head (a sit read `present` for ~30 s). NOTES, "Exit test fails…".
- [ ] **Re-record the dataset in the current pose** (`COLLECTION.md`,
      "The redo"): move the stand out of the walking path, set and record
      the pose, one empty reference, then six rounds of 12 × ~3 min
      (eve / am / pm × 2 days), starting 2026-11-28 eve. Label sessions
      10–12 per frame from video.
- [ ] Retrain on new rounds, and on new + old; score on held-out new rounds
      and `trials/`; deploy the better; rerun the exit test.
- [ ] Document the limitation in README: non-rigid mount; model sensitive
      to a few degrees of pose; a real deployment needs a fixed mount.
- [ ] Lock the harness plug: hot glue across the shield-end Dupont housings,
      or one 1×8 housing, labelled. Twisted/loose Dupont ends cost two
      bench rounds on 2026-11-28.
- [ ] Port bus recovery + ICACHE + the readout into `main.rs` when the
      application binary takes over from `fused_stream`.
- [ ] Optional: display SPI 5 → 10 MHz to make the state-change frame fit
      the 200 ms period, now the harness is proven.
- [ ] Note: the on-board LPS22HH barometer (I2C2, 0x5d) missed one scan on
      2026-11-28. Unused by this project; check again if it recurs.

## Before Gate 2 data collection

- [x] ~~Gate 2 exit check.~~ **Passed 2026-11-24.** 70 sessions,
      2026-11-22 to 24; every class in every daypart on two days (`am`
      23 + 24, `pm` 22 + 23, `eve` 22 + 23). Classes separate by eye in
      both sensors (`figures/gate2/2026-11-24_two_days_grids.png`);
      person-signature share empty 0–1.9%, seated 100%, present
      97.9–99.2% by daypart. NOTES, "Second morning round; Gate 2 closes".
      Tagged `phase2-data`.

- [x] ~~Per-frame labels for walking-past sessions.~~ **Done 2026-11-24**:
      seven sessions with per-frame labels from video — six walking past
      (2026-11-22 22, 24; 2026-11-23 10, 22, 34; 2026-11-24 11) and the
      mixed sit-and-stand session 2026-11-22 23 — every daypart covered.
      Session 12 (walking past) has a person in view in only ~54% of
      frames: 21 passes of 0.6–25 s with empty room between. Labelled
      `present` wholesale, the gaps would teach the model that an empty
      room is sometimes `present`. Tool built 2026-11-22:
      `tools/label_frames.py` (COLLECTION.md, "Labelling frames"); video
      sync confirmed end to end on session 22. Session 22 labelled
      2026-11-22 (541 present / 197 empty / 1 unsure; thermal-only
      `present` frames spot-checked against video — person at the edge of
      the thermal's wider field, not residual warmth). Session 12 (no
      video) dropped rather than labelled from heatmaps; replaced by 24
      (walking past, video, labelled). Session 23 — sat down and got up
      repeatedly — kept as a mixed session with per-frame labels.
      Afternoon session with video recorded 2026-11-23 (22). Not a
      detection heuristic: the auto-suggest is a starting point, and every
      frame is checked by eye, or the model just learns the rule.

The procedure itself is [`COLLECTION.md`](COLLECTION.md). The items below
are where its rules came from.

- [x] ~~Allow settling time between classes when recording.~~
      **Measured 2026-11-22: wait 10 minutes empty before recording `empty`**
      after anyone has sat in the chair. After a 10.6-minute sit the seat
      zone reached the noise floor (+0.25 °C) at 7 minutes; 10 gives margin
      for longer sits. See NOTES, "Settling time after sitting".
- [x] ~~Mark the chair's rotation, not just its position.~~ Done
      2026-11-22: tape along one base leg, plus a seat-edge mark lined up
      with a partner mark. Returning the chair is now: base on the spot, leg
      on its tape, seat marks aligned. (Before this, a return changed ~5 ToF
      zones beyond noise.)
- [ ] **Site the zone so subjects have depth behind them.** Measured
      2026-09-26: a person at 180 cm standing against a wall that reads
      1.71-1.98 m is *invisible* to the ToF — ≤5 cm of shift, inside the
      frame-to-frame noise. A ToF needs the subject to be nearer than
      something. A doorway with a wall at arm's reach is the worst case
      available. Aim across open space with the background well beyond the
      subject, and never assume the ToF will see a stationary person against
      a wall.
- [ ] **Do not correlate range with class.** Apparent temperature falls with
      distance because of zone averaging — a person peaked at 25.0 °C at
      1.8 m against a 21.0 °C background, roughly 40% zone fill, where real
      skin and clothing are ~31 °C. If one class is habitually collected
      nearer than another, the model will learn range, not posture.
- [ ] **Hold lighting constant across every capture.** A fluorescent lamp
      switched on mid-session contaminated a comparison on 2026-09-26. The
      VL53L5CX supplies its own 940 nm illumination and works in darkness —
      ambient IR only adds shot noise and *reduces* range, so switching a
      light on to help it see works against you. A lamp is also a warm object
      for the GridEYE that is on when people are present: the same shortcut
      the VEML6030 was excluded to avoid. Pick a state and hold it, or record
      it per session and balance it across classes — never let it track the
      class. The photographic lamp is on exactly for the sessions that produce
      video ground truth, so the rule applies there too.
- [ ] **Sunlight is an uncontrolled thermal source.** Sunlit patches are warm,
      drift through the day, and correlate with time rather than occupancy.
      Collect across different times and weather; record as a known
      limitation.
- [ ] **Decide whether thermal persistence is a feature.** A recently vacated
      chair stays warm, which genuinely separates "occupied a minute ago" from
      "empty all morning". Worth exploiting deliberately — but only
      deliberately, never as an accident in the training data.
- [x] ~~Decide what an invalid ToF zone means as model input.~~
      **Decided 2026-11-23:** two ToF channels — a valid mask (status 5
      or 9 *and* distance > 0) and distance clipped to 2 m with invalid
      zones filled as far — plus the thermal excess: a 3×8×8 input. The
      0 mm readings turned out to be a class-correlated artifact in one
      corner zone. Rule lives in `tools/encode.py`; reasons in NOTES,
      "Invalid ToF zones: the encoding".
- [ ] **Retune the thermal display bands for room distance.** Current bands
      were chosen for a desk test. A clothed person at 2–3 m may sit only 1–2 °C
      above background — the same order as the sensor's noise floor. Does not
      affect the model, which sees raw values, but the human-readable map will
      look under-sensitive when the rig is mounted properly.
- [x] ~~Host-side RTT heatmap viewer.~~ Done — `tools/heatmap_viewer.py`.
      Live view plus CSV recording, both sensors auto-detected. Verified
      against captured logs and against live hardware.
- [ ] **Use it while re-aiming the rig.** The whole reason it was built
      before the rig rather than after.
- [ ] **Implement `--fixed-scale`** — promoted from "consider" on 2026-09-26.
      Auto-scaling per frame is right for aiming, but it made the ToF sweep
      unreadable: in a cluttered scene the colour mapping rescales every frame
      and everything appears to jump. Wrong default the moment frames are
      being compared rather than aimed at.
- [ ] Video ground truth — **use the webcam already on top of the monitor.**
      It is aimed at the chair, host-timestamped, and needs no rigging, so it
      replaces the phone-on-a-tripod plan. **Not** the B-CAMS-OMV; the job
      needs a camera and a clock, not a camera pipeline on the U585.

## Use the board's own sensors

Both of these are already soldered to the B-U585I-IOT02A, on I2C2 (PH4/PH5),
which is a separate bus from the one our two sensors share. Nothing needs
wiring and neither can disturb the sensors we care about.

- [ ] **HTS221 (0x5f) — true ambient temperature.** The AMG8833 measures heat
      *relative* to ambient, and `thermal_probe` currently approximates ambient
      from the frame median. That works and needs no calibration, but it fails
      in the one case that matters: a frame where a person fills most of the
      view has a median that is partly person, so the reference drifts toward
      the thing being measured. A real room temperature reading does not.
      Second reason: a device reporting occupancy *and* temperature/humidity
      over BACnet is what a real building sensor looks like, and Phase 5 is
      already building that integration. **Third, now measured:** between two
      captures minutes apart on 2026-09-26 the die moved 0.19 °C and the frame
      median 0.13 °C, putting up to ±1.25 °C of drift on off-target zones
      against a peak signal of +3.5 °C. Slow ambient drift currently rides on
      every reading and can masquerade as signal at only ~3:1.
- [x] **ISM330DHCX (0x6b) — detect that the rig has been moved.** Done
      2026-11-22. `fused_stream` prints a 1 s averaged `mount tilt` at start
      and a sample on every frame; the CSV carries `tilt_deg`/`roll_deg`;
      the viewer warns if the tilt moves >1° from the session's start.
      Reference for this mount: 49.9–50.2° down, roll 162°. Compare each
      session's start against it. Original reasoning follows: the
      roadmap's biggest stated risk is that sensor geometry shifts between
      sessions and injects label noise that stays invisible until the model
      underperforms for reasons nobody can diagnose. An accelerometer already
      on the board can catch exactly that: record the resting orientation, and
      flag any session whose orientation does not match. It turns the
      programme's worst failure mode from silent into loud, for very little
      work. **Promoted 2026-09-26 from prudent to load-bearing:** with
      multi-week gaps in the schedule the rig will be boxed, moved or knocked
      between collection sessions, so drift goes from possible to likely.
      Do it *before* Gate 2 collects anything.

### Deliberately not using

- **VEML6030 (0x10, ambient light).** Lights-on correlates strongly with
  occupancy, so a model given this feature will likely learn "lights on =
  occupied" and stop attending to the sensors — scoring well while having
  learned nothing about the actual problem. If it is ever tried, it belongs
  *after* the fused model works, evaluated specifically for whether it improved
  anything or just found a shortcut.
- **The on-board microphones.** Audio occupancy detection works, and is a
  larger privacy problem than the camera this project exists to avoid. Using
  them would demolish the premise.
- **LPS22HH (0x5d, pressure).** Responds to HVAC and doors opening, so not
  meaningless, but likely noise. Free to add later if a use appears.

## Firmware improvements, not urgent

- [ ] **Move frame-ready from polling to the INT line.** The interrupt pin is
      currently unwired and both sensors are polled. Fine at 10 Hz; worth doing
      when the firmware becomes a proper multi-task design, and it is a
      prerequisite for meaningful sleep between frames.
- [ ] **Raise the I2C bus above 100 kHz.** Sensor firmware download is 86 KB
      and takes several seconds at the current speed. 400 kHz is well within
      spec for both parts. Check the combined pull-up strength first — the
      board's own resistors and the SATEL's 2.2 kΩ are in parallel.
- [ ] **Split the sensor drivers into a dependency-free crate.** The unit tests
      in `amg8833.rs` cannot run today because the crate pulls in
      embassy-executor, which is Cortex-M only. A split makes them runnable
      unchanged — and the FreeRTOS satellite will want the same conversion
      maths in C, so there is a second reason.
- [ ] Consider firmware-controlled power cycling of the sensor now that
      `PWR_EN` is on a GPIO. Useful for unattended recovery during the Phase 6
      multi-day run.

## Housekeeping

- [ ] `README.md` and `TROUBLESHOOTING.md` — the roadmap says every repo
      carries them; this one has `NOTES.md` and `LEARNING.md` but neither of
      those.
- [ ] Decide whether `images-wiring-cn13.jpg` belongs in the repo root, or in
      an `images/` directory, or not tracked at all.
- [ ] Keep `LEARNING.md` current as the code moves. It is the Phase 7 write-up's
      raw material, and stale explanations are worse than none.

---

## Deferred past the six-month window

Recorded so they stop being re-litigated. Each is a real idea with a real
reason for not being now.

- [ ] **Camera vs ToF comparison — accuracy, privacy, power.** The
      B-CAMS-OMV is on the bench. This is a genuinely differentiated post,
      because most write-ups only cover one modality and this one would have
      measurements on both. It stays out of the programme because it is a
      second model and a second data collection campaign — satellite-sized
      work — and because it spends the middle of the programme arguing against
      the premise the programme is built on. **Pick it up once the camera-free
      system is finished and defensible**, when the comparison has a working
      baseline to compare against and the argument lands as "I built both"
      rather than "I hedged".
- [ ] **LoRaWAN backhaul — reinstated 2026-11-24 as Satellite B**, after
      Gate 5 (roadmap, "Satellite B sub-gates", B1–B2), framed as the
      deployment story and business case rather than a C++ exercise;
      language decided at B1. History: deferred
      2026-09-26. An STM32WL55JC1 node carrying the occupancy result to the
      RAK7268V2 over AU915, with the "pull the Ethernet cable and the data
      keeps arriving" demo. Cut because the AU915 config is meant to be
      lifted rather than re-derived, leaving C++ RAII over a vendor HAL as
      the only real learning — the smallest delta over the C in
      `../freertos-u585/`. Repo and notes kept in `../lorawan-occupancy/`.
- [ ] Battery power and a real power-draw measurement. Mains vs battery is a
      genuine market distinction for occupancy sensors.
- [ ] Multi-zone deployment: two or three nodes across a floor, aggregated in
      Grafana into one view. Needs the second SATEL and
      `vl53l5cx_set_i2c_address()` plus LPn control if two ever share a bus.
- [x] ~~KiCad sensor shield.~~ **Dropped 2026-11-24** with KiCad itself
      (roadmap, "Deferred"): time goes to the Edge AI and LoRaWAN business
      cases and FreeRTOS instead. The soldered proto shield stays.
