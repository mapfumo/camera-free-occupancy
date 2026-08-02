# TODO

Working list. Newest thinking wins — if something here contradicts
`occupancy-sensor-project-plan.md` or the roadmap, those are the authority and
this needs fixing.

Ordered by when it matters, not by size.

---

## Finish Phase 1

- [ ] **Solder headers to the SATEL and build the sensor rig.** Waiting on the
      ElectroCookie proto shields. Sockets on the shield, pins on the sensors,
      so both stay removable — there is only one AMG8833 and desoldering a ToF
      is how ToF sensors die.
- [ ] **Leave a physical gap between the two sensors on the shield.** Measured,
      not theoretical: the VL53L5CX climbed from 34 °C to 44 °C over a few
      minutes of ranging. A thermopile mounted against that gains a permanent
      bright corner in every frame, identical across all classes, quietly
      eating dynamic range instead of failing visibly. Thermal sensor upstream
      in any airflow.
- [ ] **Combined stream — both grids, one output.** Deliberately *after* the
      rig: what this captures is Phase 2's dataset, and the geometry it records
      should be the final one. Building it against jumpers means recording a
      layout that is about to change.
- [ ] **Aim the AMG8833 and confirm it sees a warm target.** As of the last
      run it reported a uniform room with nothing warm ever entering the field
      of view — the sensor works, but it has not yet been pointed at anything.
- [ ] Tag `phase1-bringup` once the above is done.

## Before Phase 2 data collection

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
- [ ] Host-side RTT heatmap viewer, per the roadmap's month-1 entry. Build it
      *before* the rig, so alignment is something watched live rather than
      discovered in matplotlib the next day.
- [ ] Video ground truth setup — phone on a tripod, visible clock, time-synced
      to the frame log. **Not** the B-CAMS-OMV; the job needs a camera and a
      clock, not a camera pipeline on the U585.

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
- [ ] Battery power and a real power-draw measurement. Mains vs battery is a
      genuine market distinction for occupancy sensors.
- [ ] Multi-zone deployment: two or three nodes across a floor, aggregated in
      Grafana into one view. Needs the second SATEL and
      `vl53l5cx_set_i2c_address()` plus LPn control if two ever share a bus.
- [ ] KiCad sensor shield — a 2-layer board carrying both breakouts on sockets
      with proper pull-ups and decoupling, no MCU. The roadmap gives this an
      optional month-6 entry point, where a proven circuit gives it an honest
      reason to exist.
