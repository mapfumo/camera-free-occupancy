# Project Plan: Camera-Free Occupancy Sensor (VL53L5CX + STM32U5 + Edge AI)

**Goal:** Build a presence/occupancy detector that classifies a room as
empty / occupied / people-count using an 8×8 Time-of-Flight sensor (no
camera, no images captured), running inference on-device, and feeding
the result into your existing BACnet/Modbus/LoRaWAN Grafana pipeline.

**Why this project:** It reuses skills you already have (Rust firmware,
IIoT gateway integration, Grafana dashboards) and adds edge AI on top,
rather than starting a new domain from scratch. End state is a working
demo you can blog about and, if validated, show to a building
automation contact.

---

## 0. Hardware & Software Checklist

**Hardware (you already have this):**
- [ ] B-U585I-IOT02A Discovery Kit (STM32U585, Cortex-M33)
- [ ] VL53L5CX-SATEL (8×8 zone ToF sensor)
- [ ] Jumper wires / breadboard (for I2C + interrupt line wiring)
- [ ] USB-C cable for programming/power
- [ ] HiLetgo ILI9341 TFT (SPI) — on-device readout, added in Phase 4
- [ ] A phone and a tripod — video ground truth in Phases 2 and 6
- [ ] (Owned, deliberately deferred) B-CAMS-OMV camera bundle — needed
      only for the vision-based comparison in the stretch list, which
      stays outside the six-month window. It is **not** the ground-truth
      camera; see Phase 2 step 4

**Software:**
- [ ] STM32CubeIDE or `probe-rs` + your existing Rust/Embassy toolchain
- [ ] STM32Cube AI Studio (replaces X-CUBE-AI) — free download from
      st.com
- [ ] ST's VL53L5CX ULD (Ultra Lite Driver) C API — available in
      STM32Cube expansion packages / on GitHub under STMicroelectronics
- [ ] Python 3 + `pandas`/`matplotlib` (or a spreadsheet) for offline
      data labeling and quick-look plots
- [ ] Your existing gateway stack (bacnet-gateway / wk11-unified-monitoring
      repo) — no changes needed yet, just have it running

**Read before wiring anything:**
- [ ] B-U585I-IOT02A User Manual — confirm which I2C bus and pins are
      free on the Arduino Uno V3 / expansion connector (do **not**
      assume pin numbers — different discovery kits route I2C
      differently, so check the board's actual pinout diagram)
- [ ] VL53L5CX-SATEL datasheet — confirm which pins are I2C SDA/SCL,
      interrupt (INT), power-enable (PWREN), and LPn (if used)

---

## Phase 1 — Hardware Bring-Up (Est. 1–2 evenings)

**Objective:** Get raw distance/reflectance data streaming off the
sensor and printed over UART or your existing debug channel. No AI
yet — just prove the physical link works.

1. Wire VL53L5CX-SATEL to the B-U585I-IOT02A:
   - SDA/SCL → the discovery board's I2C bus (confirm which one is
     unused/free per the user manual)
   - INT → any free GPIO (you'll poll or interrupt on "new data ready")
   - VCC/GND → 3.3V rail on the discovery board
2. Bring up I2C on the STM32U5 in your Rust firmware (Embassy has I2C
   HAL support — reuse the same patterns from your LoRaWAN sensor
   nodes).
3. Port or call the VL53L5CX ULD driver (it's C — you can either use
   `cc`/`bindgen` to link it into your Rust firmware, similar to how
   you'd bind any vendor C HAL, or write a thin C shim and call it via
   FFI). This is more wiring than novel logic — budget time for it but
   don't expect it to be conceptually hard.
4. Read one full 8×8 frame (64 distance values in mm, plus a
   reflectance/signal-strength value per zone) and print it over UART.
5. **Checkpoint:** wave your hand in front of the sensor and confirm
   the printed grid changes sensibly (closer object → smaller distance
   values in the zones it covers).

---

## Phase 2 — Data Collection (Est. 1 weekend)

**Objective:** Capture labeled 8×8 frames for a small set of classes,
so you have something to train/threshold against.

1. Pick 3–4 classes to start simple, e.g.:
   - `empty` — no one in the room
   - `one_person_sitting` (at a desk, typical of the sensor's mount
     height/angle)
   - `one_person_standing`
   - `walking_through` (person crossing the field of view)
2. Mount the sensor at a realistic install height/angle (think:
   corner-of-room or above-a-doorway, like a real HVAC occupancy
   sensor would be mounted).
3. Log frames to a CSV over UART or straight to your InfluxDB (reuse
   the ingestion pattern from your LoRaWAN nodes) — capture at least
   1–2 minutes per class, several sessions, ideally across a couple of
   different days/lighting conditions (ToF is largely light-independent,
   but do it anyway for robustness).
4. Record video ground truth alongside each session — a phone on a
   tripod with a visible clock in frame, time-synced to the frame log.
   Hand-labeled sessions drift: you remember a run as "sitting" and you
   are occasionally wrong, and a mislabeled session is indistinguishable
   from a bad model three weeks later. This is a **development tool and
   never part of the deployed system** — say that explicitly in the
   write-up, because the whole premise is camera-free sensing and a
   reader will notice the camera otherwise. Note that this is a phone,
   not the B-CAMS-OMV: the job needs a camera and a clock, not a camera
   pipeline running on the U585.
5. Label each session file with its class in the filename or a column.
6. **Checkpoint:** load the CSVs in Python, plot a few frames as
   heatmaps (`matplotlib.pyplot.imshow`), and visually confirm the
   classes look distinguishable by eye. If `empty` vs `one_person`
   don't look different in the heatmap, fix sensor placement before
   going further — don't try to fix a bad sensor angle with a fancier
   model.

---

## Phase 3 — Model Design & Training (Est. 1 weekend)

**Objective:** Turn labeled frames into a small classifier you can run
on the STM32U5, using STM32Cube AI Studio.

1. Start with the simplest thing that could work: treat each 8×8 frame
   as a 64-value input vector (or a single-channel 8×8 "image") and
   train a small classifier — a shallow CNN or even a small dense
   network is plenty for this problem; you don't need anything close
   to STM32N6/NPU-scale models.
2. Use STM32Cube AI Studio's **Bring Your Own Data (BYOD)** flow:
   - Import your labeled CSV/frame dataset
   - Let it help you pick/train a small model, or bring a model you
     trained yourself in Keras/PyTorch and import it (BYOM path) if
     you'd rather train outside the tool
3. Check the tool's memory/latency estimate for the STM32U585 target
   before you get attached to an architecture — if it doesn't fit in
   RAM/flash comfortably, shrink the model (fewer filters/layers) rather
   than switching boards.
4. Export the optimized C code for your target (STM32U5).
5. **Checkpoint:** Validate accuracy on a held-out slice of your own
   data (data the model didn't train on) — don't trust training
   accuracy alone. Confusion matrix between your classes is more
   useful than a single accuracy number.

---

## Phase 4 — Firmware Integration (Est. 1 weekend)

**Objective:** Run inference on-device, continuously, from live sensor
frames — not just on saved CSVs.

1. Link the generated C inference code into your firmware (same FFI
   approach as the VL53L5CX driver in Phase 1).
2. Feed each new 8×8 frame from the sensor straight into the model
   instead of to CSV logging.
3. Run inference on a sensible cadence (e.g., every 1–2 seconds is
   plenty for occupancy — no need to run it every frame).
4. Output the classification result (e.g., `occupied` / `empty` / a
   people-count) over your existing telemetry channel — UART, MQTT, or
   whatever your LoRaWAN/gateway nodes already use.
5. Add the on-device readout on the ILI9341 (SPI): class and confidence
   large enough to read across the room, plus a small live heatmap. This
   is instrumentation for the checkpoint below rather than polish — the
   test asks you to walk the field of view, and reading the result off a
   tethered laptop makes that a two-person job. **Display only; leave the
   touch panel unused.** Scoped that way it is an evening's work; allowed
   to become a UI it will consume the phase.
6. **Checkpoint:** stand in front of the sensor, walk away, walk back —
   confirm the live classification output tracks reality with a
   reasonable delay and without obvious flapping (if it flickers
   rapidly between classes, add simple debouncing/hysteresis in
   firmware before blaming the model).

---

## Phase 5 — Gateway & Dashboard Integration (Est. 1 evening–1 weekend)

**Objective:** Make this sensor show up exactly like your other IIoT
sensors, in the same pipeline.

1. Add an MQTT (or whatever transport you're using) topic/message for
   `occupancy_status` from this node, matching the pattern used by
   your other LoRaWAN/Modbus sensors feeding `bacnet-gateway` /
   `wk11-unified-monitoring`.
2. Map the occupancy value to a BACnet object (e.g., a Binary Input for
   occupied/empty, or an Analog Input for a people-count) using the
   same approach you used for your other sensor types in that repo.
3. Add a panel to your existing Grafana dashboard showing occupancy
   over time, alongside your other sensor data.
4. **Checkpoint:** occupancy events show up in Grafana with reasonable
   timestamps and no gaps — treat this the same way you validated your
   LoRaWAN walk-test data.

---

## Phase 6 — Real-World Validation (Est. 2–3 days spread out)

**Objective:** Find out where it breaks before you call it done.

1. Run it in a real room (your office/study) for a few days, logging
   continuously.
2. Deliberately test edge cases:
   - Two people close together (does it merge into one blob?)
   - Someone sitting very still for a long time (does the model still
     detect them, or does it assume "no motion = empty"? — decide if
     you want motion-based or presence-based detection and design for
     that intentionally)
   - Pets, if you have any (false positives?)
   - Someone standing at the edge of the field of view
3. Keep the video ground truth running for at least one full day of the
   continuous run. When the log says `occupied` at 14:32 and you believe
   the room was empty, the recording is the whole difference between a
   documented limitation and a mystery you cannot explain in the post.
4. Write down false positives/negatives you find — this becomes the
   "known limitations" section of your write-up, which is more
   credible than pretending it's perfect.

---

## Phase 7 — Write It Up

**Objective:** Turn this into a blog post in the same style as your
LoRaWAN walk-test and BACnet gateway posts — real data, real bugs,
what you'd do differently.

Suggested structure (matches your existing blog voice):
1. The problem — why camera-free occupancy sensing matters (privacy,
   simpler compliance, works in the dark)
2. The setup — sensor + board + why VL53L5CX over a camera
3. Data collection — show a heatmap or two
4. Model + results — confusion matrix, what worked / didn't
5. Integration into your existing gateway — tie it back to your BACnet
   work explicitly, since that's your differentiator
6. What broke, what you'd change next time

---

## Timeline

Superseded by `SIX_MONTH_ROADMAP.md`, which schedules these phases across
2026-08-01 → 2027-02-01 alongside the FreeRTOS, KiCad, and C++ work. This
project is the flagship of that roadmap, not a standalone 7-week sprint.

| Month | Phases here |
|-------|-------------|
| 1 — Aug 2026 | Phase 1 (bring-up) + Phase 2 (data collection) |
| 2 — Sep 2026 | Phase 3 (model) + Phase 4 (live inference) |
| 3 — Oct 2026 | Phase 5 (gateway/dashboard) |
| 4 — Nov 2026 | — (FreeRTOS variant completed and measured) |
| 5 — Dec 2026 | Phase 6 (real-world validation) |
| 6 — Jan 2027 | Phase 7 (write-up) |

The per-phase effort estimates above still hold; the roadmap adds slack
around them rather than expanding the work. Note that the phase estimates
assume the original side-project pace — at evenings-plus-weekends they will
finish faster, and that headroom is what the satellite projects consume.

---

## Stretch Goals (only after the above works end-to-end)

- Battery-power the node and measure real power draw / battery life
  estimate (relevant if this ever becomes a product — mains-powered
  vs. battery-powered occupancy sensors are a real market distinction)
- Add a LoRaWAN backhaul variant on your STM32WL55 nodes for
  installations where running Ethernet/PoE isn't practical
- Compare against the B-CAMS-OMV camera-based approach on accuracy vs.
  privacy vs. power trade-offs, and write that comparison up — it's a
  genuinely useful, differentiated post since most tutorials only cover
  one modality
- Multi-zone deployment: two or three of these nodes across a floor,
  aggregated in Grafana into a single "floor occupancy" view

---

## Notes on Scope Discipline

- Resist the urge to jump straight to a fancy model in Phase 3 before
  Phase 2's data is confirmed to be visually distinguishable — a
  perfect model on bad data will not save this project.
- Resist the urge to add BACnet integration (Phase 5) before Phase 4's
  live inference is proven stable — get the sensor right before wiring
  it into the bigger pipeline.
- If any phase takes more than double its estimate, that's useful
  information for the blog post ("this took longer than expected
  because...") — don't hide it, it's the most useful part of your
  existing posts.
