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
- [ ] **Check the thermal gap — convection half is already solved.**
      Mounting the board in portrait for the tilt puts the GridEYE *below* the
      SATEL, so the ToF's plume rises away from it rather than across it. That
      was the risky mechanism and it is gone for free. What remains is
      conduction through the shared protoboard, which is orientation-
      independent and much weaker. Still worth one soak: run the ToF for ten
      minutes in the final pose and check the GridEYE does not grow a fixed
      warm region. Note the part runs hotter than recorded — 42 °C within five
      frames, 51 °C at eight minutes and still climbing, so August's 44 °C was
      a short measurement, not a plateau.
- [ ] **Fix the tilt — people land in rows 0-1 only.** Every person-height
      return in every capture on 2026-09-26 landed in the top two rows: 16 of
      64 zones usable, the rest floor, door jamb and out-of-range space.
      Geometry says a person at 2 m spans ~44°, almost the full 45° field, so
      they should nearly fill the frame. Reduce the tilt until a person at
      working distance sits in the *middle* rows, then re-baseline the empty
      room — every number in NOTES is tied to the current pose.
- [ ] **Move the boom clear of the door jamb.** A static strip at 0.85-0.98 m
      runs down column 7 in every frame. Confirm it frees up.
- [ ] **Measure usable depth.** Detection is solid at ~1.5 m and absent at
      ~2 m, but whether that is the cone or the range is unknown. Walk out in
      half-metre steps and record where it is lost. That number decides how
      big a zone the product can honestly claim.
- [ ] **Redo the orientation characterisation for both sensors.** The board is
      portrait now; the hot-mug "entered from the bottom-left" finding is void
      and the SATEL's grid orientation was never established. Provisional
      evidence says row 0 is the top of the world view — confirm it.
- [ ] **Mark the ball head, clamp position and column extension.** A long
      column with an offset load on a ball joint, clamped to a desk people
      lean on, is the geometry-drift risk in physical form.
- [ ] **Combined stream — both grids, one output.** Deliberately *after* the
      rig: what this captures is Phase 2's dataset, and the geometry it records
      should be the final one. Building it against jumpers means recording a
      layout that is about to change.
- [x] ~~Aim the AMG8833 and confirm it sees a warm target.~~ Done — hot mug
      peaked at 71.5 °C against a ~21 °C room, appearing and disappearing
      cleanly. Entered from the bottom-left, which fixes its orientation.
- [ ] Tag `phase1-bringup` once the above is done.

## Before Gate 2 data collection

- [ ] **Allow settling time between classes when recording.** The AMG8833
      retains heat: after a mug was removed the whole frame still read ~1 °C
      high, with the die 1.25 °C up. Frames recorded straight after a person
      leaves still contain them, so back-to-back sessions would put the warm
      ghost of `sitting` into the `empty` set. Decide a settling period and
      write it into the collection procedure.
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
- [ ] Consider a `--fixed-scale` option. Auto-scaling per frame is right for
      aiming — maximum contrast on whatever is in view — but it makes colours
      incomparable between frames, which is the wrong default once frames are
      being compared rather than aimed.
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
      already building that integration.
- [ ] **ISM330DHCX (0x6b) — detect that the rig has been moved.** The
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
