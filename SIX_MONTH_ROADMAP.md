# Six-Month Roadmap: Network Engineering → Embedded Systems & Edge AI

**Window:** 2026-08-01 → 2027-02-01
**Availability:** every evening + all weekends (~30 h/week, ~750 h total)
**End state:** a portfolio, a public track record, and enough demonstrated
range across embedded Rust, C/C++, FreeRTOS, and on-device AI to be hired or
contracted as an embedded engineer.

This roadmap is the master document. `occupancy-sensor-project-plan.md` is
the detailed plan for the flagship project and remains the authority on its
seven phases; this file decides *when* those phases happen and what runs
alongside them.

**Hardware:** everything needed is already on the bench — B-U585I-IOT02A,
VL53L5CX-SATEL, AMG8833, breadboards, logic analyzer, W5500, gateway stack.
Nothing in this roadmap requires a purchase. See `HARDWARE_INVENTORY.md`.

---

## Shape: one flagship, two satellites

**Flagship — CoworkOS occupancy sensor (Edge AI).** The headline portfolio
piece. Camera-free occupancy detection: VL53L5CX ToF + AMG8833 thermal, fused
8×8 grids, on-device inference, into the existing BACnet/Grafana pipeline.
Runs across months 1–5. Detailed in `occupancy-sensor-project-plan.md`.

**Satellite A — FreeRTOS in C.** The same firmware, built a second time on
FreeRTOS in C, as a deliberate A/B against Embassy's async executor. This is
the highest-value satellite: FreeRTOS is the most-deployed embedded RTOS, so
it puts industry-standard C on the CV, and the measured comparison is the
single best piece of content in the six months. Runs months 1–4.

> **Use the native FreeRTOS API. Do not use the CMSIS-RTOS2 wrapper.**
> CubeMX will offer to generate the project wrapped in CMSIS-RTOS2 and it is
> the default — decline it. The wrapper hides what the kernel is actually
> doing behind a vendor-neutral veneer, which is precisely the thing you are
> here to learn. Interviews ask about `xQueueSendFromISR`, priority
> inheritance, and stack high-water marks; nobody asks about
> `osMessageQueuePut`. Write `xTaskCreate`, `xQueueSend`, `xSemaphoreTake`
> directly and read the kernel source when something surprises you.

The point of this satellite is not the API — that is a weekend's reading. It
is the failure modes Embassy never exposes you to: per-task stack sizing and
high-water marks, priority inversion and why a priority-inheriting mutex is
not a binary semaphore, `configMAX_SYSCALL_INTERRUPT_PRIORITY` and the NVIC
priority footgun, the `...FromISR` discipline, and static allocation. Meeting
these in C is what earns the right to argue for Rust from experience rather
than preference.

**Satellite B — a C++ build.** Deliberately unspecified. Chosen in month 3
once it's clear which gap is worth filling. Runs months 4–5.

**Deferred: KiCad.** Not in this program. Nothing here needs a custom PCB —
the Discovery kit and the two breakouts cover every phase. KiCad belongs to
the slower "future custom solutions" track and gets an optional entry point in
month 6 (see below), where a proven circuit gives it a real motivation
instead of a contrived one.

---

## Operating principles

**Language choice is a decision, not a default.** The rule: *is the `unsafe`
concentrated or smeared?* A thin, auditable FFI shim under a large safe
application is Rust working as designed — keep it. Unsafe smeared throughout
because you're fighting a vendor HAL that assumes C's ownership model means C
is the honest choice. Vendor-SDK-dominated work tends toward C; sensor drivers
and application logic tend toward Rust.

Write the reasoning down each time. A portfolio that shows *judgment* about
language choice is worth more than one that shows loyalty to a language.

**Rust is the differentiator, C is table stakes.** Most embedded jobs are C.
Being fluent in both, and articulate about when not to reach for Rust, is the
position worth occupying.

**Record during the debugging, not after.** Failures are the content and they
are unreconstructable once solved. Timebox editing hard or video production
will eat the build time it exists to document.

**Solve mechanical problems mechanically.** Flaky wiring and inconsistent
sensor geometry are real threats to the data, but they are bracket-and-
protoboard problems, not PCB problems. Don't reach for a fabricated board to
fix something a jig fixes in an evening.

**Scope discipline carries over from the project plan.** Don't start model
training before the Phase 2 heatmaps are visually distinguishable. Don't wire
in BACnet before live inference is stable.

---

## Month 1 — August 2026: Hardware bring-up & data collection

**Flagship (Phases 1–2), ~80% of the month.**
- Confirm free I2C bus/pins from the B-U585I-IOT02A user manual — do not assume
- I2C bus scan finding `0x29` (VL53L5CX) and `0x68` (AMG8833)
- VL53L5CX ULD linked via `cc` + `bindgen`; write the platform layer
  (I2C read/write/wait hooks) by hand and understand it line by line — this is
  real C practice, not a black box to wrap and forget
- AMG8833 driver in pure Rust (almost no unsafe — Rust is correct here)
- Raw 8×8 frames streaming over RTT
- **Build the sensor rig before collecting any data.** Both sensors soldered to
  protoboard with short leads, rigidly fixed in known relative alignment, on a
  bracket that mounts repeatably at a realistic height and angle. This is not
  optional polish: if the geometry shifts between sessions, the "empty room"
  pattern shifts with it and you inject label noise straight into the training
  set — invisible until the model underperforms for reasons you can't diagnose.
- Log labeled CSV for 4 classes across several sessions and days
- **Checkpoint:** heatmaps in matplotlib are distinguishable by eye. If
  `empty` vs `one_person` look the same, fix the mounting — not the model.

**Satellite A (start), ~20% — this month is the toolchain, not the RTOS.**

Be honest about what "FreeRTOS blinky" actually costs: it is the entire C-side
toolchain arriving at once. ST's HAL, CubeMX or CMake with `arm-none-eabi-gcc`,
a different debug story than the defmt/RTT ergonomics you're used to (SEGGER
RTT works fine from C — set it up early), and only then FreeRTOS itself. The
HAL is arguably the larger of the two unfamiliar things.

- Get a plain C project building and flashing on the U585 — no RTOS yet
- SEGGER RTT or semihosting for output, so you are not debugging blind
- Then FreeRTOS: blinky, two tasks, a queue, a semaphore
- **Native FreeRTOS API only — decline CubeMX's CMSIS-RTOS2 wrapper.**
  The ARMv8-M port you want is the non-TrustZone one (`ARM_CM33_NTZ`);
  TrustZone adds a dimension you do not need yet.
- Separate repo. Nothing product-shaped.
- **Expectation:** finishing this month able to build and flash C on the
  U585 is success. Actual FreeRTOS fluency is a month-2 outcome.

**Content:** video 1 (why embedded, leaving network engineering, what the
6 months is); blog draft on linking a vendor C driver into Rust firmware.

---

## Month 2 — September 2026: Model & live inference

**Flagship (Phases 3–4).**
- STM32Cube AI Studio BYOD flow; start with the simplest thing that works —
  a 128-value input vector (both sensors concatenated) into a small dense net
  or shallow CNN
- Check the memory/latency estimate for the U585 *before* getting attached to
  an architecture
- **Checkpoint:** confusion matrix on held-out data, not training accuracy
- Export C, link via FFI, run inference every 1–2 s on live frames
- Debounce/hysteresis in firmware
- **Checkpoint:** walk in, walk out, walk back — tracks reality without flapping

**Satellite A (continue) — this is the real FreeRTOS fluency month.**
- Get the VL53L5CX ULD running under FreeRTOS in C — no Rust in the loop.
  You already wrote the platform layer in month 1, so this is mostly
  scheduling and buffer ownership.
- Deliberately exercise the things that bite: size the task stacks by
  measuring `uxTaskGetStackHighWaterMark` rather than guessing, turn on
  `configCHECK_FOR_STACK_OVERFLOW`, use a priority-inheriting mutex for the
  shared I2C bus, and read the frame-ready interrupt through a task
  notification or `xQueueSendFromISR` — not a polled flag.
- Check your NVIC priorities against `configMAX_SYSCALL_INTERRUPT_PRIORITY`
  before you call any `FromISR` API. Getting this wrong corrupts the kernel
  in ways that take days to find.
- Start the comparison notes now, while both designs are fresh.

**Content:** video on data collection + heatmaps (the most visual part of the
whole project — use it).

---

## Month 3 — October 2026: Gateway integration & FreeRTOS build-out

**Flagship (Phase 5).**
- MQTT topic → BACnet object (Binary Input occupied, Analog Input count,
  Analog Value confidence) → Grafana panel
- Matches the pattern in `bacnet-gateway` / `wk11-unified-monitoring`
- **Checkpoint:** occupancy events in Grafana, sane timestamps, no gaps

**Satellite A (build-out).**
- Both sensors reading under FreeRTOS; task decomposition settled (sensor
  task, inference task, telemetry task, queues between them)
- This is where the concurrency models genuinely diverge — note what each
  makes easy and what each makes awkward as you hit it

**Satellite B (choose).**
- Pick the C++ project now, based on which gap is most visible after three
  months. Decide it in month 3 so month 4 can start clean.

**Content:** blog post 1 published — the full Edge AI story end to end. This is
the flagship post; give it the real data and the real bugs.

---

## Month 4 — November 2026: FreeRTOS variant complete & measured

**Satellite A (main effort, finish it).**
- Full FreeRTOS/C implementation of the occupancy firmware: same sensors, same
  inference, same telemetry output — different concurrency model throughout
- **Measure both properly.** Binary size, static and peak RAM, inference
  latency, jitter on the 1–2 s cadence, behaviour under a stalled I2C
  transaction. Numbers, not impressions — this is what makes the write-up
  credible.
- Record where each model bit you: Embassy's borrow-checker friction around
  shared peripherals versus FreeRTOS's manual synchronisation and stack sizing

**Satellite B (start).**

**Content:** video — the same firmware written twice, and what that teaches.

---

## Month 5 — December 2026: Validation & depth

**Flagship (Phase 6).**
- Multi-day continuous run in a real room
- Deliberate edge cases: two people close together, someone sitting very still
  for hours, pets, someone at the edge of the field of view
- Decide explicitly whether this is motion-based or presence-based detection
  and design for it — don't let it be an accident
- Write down every false positive and negative. This becomes the "known
  limitations" section, which is more credible than claiming it's perfect.

**Satellite B (build).**

**Content:** blog post 2 — RTOS vs async executors, backed by the month-4
measurements. This is the post most likely to get read by people who hire.

---

## Month 6 — January 2027: Consolidate & go to market

- Phase 7 write-up
- README and documentation quality pass across every public repo — this is
  what a hiring manager actually opens
- Portfolio page on mapfumo.net pulling the six months together
- CV rework around the embedded/Edge AI positioning
- Outreach: building automation contacts, contract leads, applications
- **Buffer.** Something above will overrun. Assume it and leave the room.

**Optional, only if the buffer is genuinely spare — the KiCad entry point.**
Lay out a sensor shield in KiCad: a 2-layer board carrying both breakouts on
sockets with proper I2C pull-ups and decoupling, no MCU, everything
hand-solderable. By this point the circuit is proven and the board has an
honest reason to exist. Treat it as the first step of the slower
custom-hardware track, not a deadline item — if month 6 is busy, it moves
past the window with no cost to the program.

---

## Deliverables at the end

| # | Deliverable | Language | Repo |
|---|-------------|----------|------|
| 1 | Occupancy sensor firmware, Embassy | Rust + C FFI | CoworkOS |
| 2 | Occupancy sensor firmware, FreeRTOS | C | new |
| 3 | Measured comparison of the two | — | write-up |
| 4 | C++ project | C++ | new |
| 5 | Gateway/BACnet/Grafana integration | Python | existing stack |
| 6 | 3+ blog posts | — | mapfumo.net |
| 7 | 6+ videos | — | YouTube |

---

## Content calendar

Videos are a personal vlog — for future reference, and for anyone else
migrating from network engineering into embedded. That angle *is* the
differentiator: there are many embedded channels, but few by someone who
already understands protocols, IP stacks, and industrial networks from the
operator side. Lean into it rather than competing on tutorial polish.

| Month | Video | Blog |
|-------|-------|------|
| 1 | Why embedded / the plan | (draft) vendor C driver into Rust |
| 2 | Data collection + heatmaps | — |
| 3 | Edge AI on a microcontroller | **Post 1:** full Edge AI build |
| 4 | The same firmware written twice | — |
| 5 | Measuring both: RAM, latency, jitter | **Post 2:** FreeRTOS vs Embassy, measured |
| 6 | Six months in review | **Post 3:** what broke, what I'd change |

---

## Risks

**Model doesn't separate the classes.** Most likely cause is sensor placement,
not architecture. The Phase 2 checkpoint exists to catch this early — respect
it, and build the sensor rig before collecting data rather than after.

**The FreeRTOS variant expands without limit.** It only has to go far enough to
support an honest comparison — sensors in, inference running, telemetry out.
It does not need the BACnet integration or the full validation run. Decide the
stopping line in month 3 and hold it.

**Video production eats build time.** The failure mode is spending a weekend
editing instead of building. Cap it: rough cuts, minimal editing, ship it.

**Scope creep from enthusiasm.** Stretch goals (battery power, LoRaWAN
backhaul, multi-node floor view, camera comparison, custom PCB) stay stretch
goals until the end-to-end system works. They are listed in the project plan;
leave them there.
