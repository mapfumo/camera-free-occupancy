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
- [ ] **Re-verify the rig on hardware after the break.** Built on 2026-08-03
      and not exercised since. Run `i2c_scan`, `tof_range`, `thermal_probe`
      and the heatmap viewer against it before trusting any frame it produces.
      Record the mounting height and angle in `NOTES.md` while doing it —
      that is the geometry the whole dataset inherits.
- [ ] **Confirm the thermal gap is sufficient.** The gap exists on the shield,
      but it was chosen by eye and never tested. Measured, not theoretical:
      the VL53L5CX climbed from 34 °C to 44 °C over a few minutes of ranging.
      Run the ToF continuously for ten minutes and watch whether the AMG8833
      grows a fixed bright corner. If it does, that corner appears in every
      class identically, quietly eating dynamic range instead of failing
      visibly — and it is far cheaper to find now than after a dataset exists.
      Thermal sensor upstream in any airflow.
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
- [ ] Video ground truth setup — phone on a tripod, visible clock, time-synced
      to the frame log. **Not** the B-CAMS-OMV; the job needs a camera and a
      clock, not a camera pipeline on the U585.

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
