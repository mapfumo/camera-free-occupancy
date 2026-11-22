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

## Before Gate 2 data collection

- [ ] **Gate 2 exit check — rerun `tools/class_look.py` after each round.**
      First run 2026-11-22 (round 1, one afternoon): classes separate in
      both sensors and in position. Exit when that still holds across
      several days and all three dayparts.

- [ ] **Per-frame labels for walking-past sessions — settle before Gate 3.**
      Session 12 (walking past) has a person in view in only ~54% of
      frames: 21 passes of 0.6–25 s with empty room between. Labelled
      `present` wholesale, the gaps would teach the model that an empty
      room is sometimes `present`. Future walking sessions record the webcam
      alongside (COLLECTION.md, "Ground truth"); session 12 gets per-frame
      labels before training, or only its in-view stretches are used. Not a
      detection heuristic — labelling with a hand-made rule just teaches the
      model the rule.

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
- [ ] **Decide what an invalid ToF zone means as model input.** Valid-zone
      count swings 0/64 to 64/64 with no fault — a zone reports nothing when
      nothing is in range, which at room scale is constant. This is a real
      observation about the room, not missing data, and encoding it as 0 mm
      asserts the exact opposite ("something is touching the sensor"). Decide
      before training, not after the confusion matrix disappoints.
      **Sharpened 2026-09-26:** a frame returned a literal `0` mm at r7 c0, so
      the driver already emits 0 as a value rather than a sentinel. "Invalid"
      is not one condition either — out-of-range, too-dark-to-return, and a
      spurious 0 are three different things and must not collapse into one
      number. Dark carpet makes the second case common here.
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
- [ ] **LoRaWAN backhaul in embedded C++17 (was Satellite B).** Deferred
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
- [ ] KiCad sensor shield — a 2-layer board carrying both breakouts on sockets
      with proper pull-ups and decoupling, no MCU. The roadmap gives this an
      optional post-Gate-8 entry point, where a proven circuit gives it an honest
      reason to exist.
