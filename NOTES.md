# Notes

Running record of findings and decisions. Newest phase last.

---

## Phase 1 — Hardware bring-up

### I2C bus and pin assignment

**Status: CONFIRMED on hardware, 2026-08-01.** `cargo run --release --bin i2c_scan`
against ST-LINK V3 `002300453432511530343838`.

| Bus  | SCL | SDA | Routed to | Verdict |
|------|-----|-----|-----------|---------|
| I2C1 | PB8 | PB9 | ARDUINO Uno V3 header, D15 (SCL) / D14 (SDA) | **use this** |
| I2C2 | PH4 | PH5 | On-board MEMS sensors | occupied, leave alone |

The run also confirmed the clock tree independently: embassy reported
`sys: 160000000`, `hclk1/pclk1/pclk2/pclk3: 160000000` off HSI16, which is what
`clock.rs` intends.

I2C1 is the bus for both project sensors. It reaches the ARDUINO header, and
nothing on the board is fitted to it, so the VL53L5CX-SATEL and the AMG8833
can share it without contending with the on-board devices.

Sources agree on the pin assignment:

- Zephyr's board devicetree for `b_u585i_iot02a` — `i2c1` on PB8/PB9,
  `i2c2` on PH4/PH5
- ST's `stm32ai-nanoedge-datalogger`, which lists the same two buses with the
  same pins for this board
- ST's UM2839 (board user manual) and the MB1551 schematic are the primary
  authority; the ST download of both is still worth having on disk

### Scan result

**I2C2 — 7 devices.** Six were predicted; the seventh is accounted for below.

| Addr | Part | Function |
|------|------|----------|
| 0x10 | VEML6030 | ambient light |
| 0x1e | IIS2MDC | magnetometer |
| 0x56 | M24256 | EEPROM, memory array |
| 0x5d | LPS22HH | barometer |
| 0x5e | M24256 | EEPROM, identification page |
| 0x5f | HTS221 | humidity / temperature |
| 0x6b | ISM330DHCX | 6-axis IMU |

**I2C1 — 0 devices**, as expected with nothing wired to the header.

Because every predicted address on the control bus answered, the scanner
itself is proven, and the empty I2C1 result is a real reading rather than a
broken probe. That was the point of scanning both.

**On 0x5e:** not predicted, and it is the same chip as 0x56. `0x56` is
`1010_110` and `0x5e` is `1011_110` — identical A2/A1/A0 strapping, different
device-select nibble. That is the M24256's identification page answering
alongside its memory array, which is standard for the M24 family. Inferred
from the address bit pattern rather than read off the schematic, but the
pattern is hard to explain any other way. It is on a bus we are not using and
collides with nothing, so it needs no further action.

### Resolved: there is no on-board VL53L5CX

**Nothing answered at 0x29 on either bus.** ST's marketing copy mentions a
"time-of-flight and gesture-detection sensor" and ST's own datalogger lists a
VL53L5CX against this board, but the board in hand does not have one — Zephyr's
devicetree, which models no ToF node, was right.

So Phase 1 proceeds as originally planned: the VL53L5CX-SATEL gets wired to
I2C1 on the ARDUINO header. The zero-wiring bring-up shortcut is not available,
and the ULD will be brought up against the breakout with the wiring as a live
variable. Worth remembering when the driver first misbehaves: check the jumpers
before reading the ULD source.

### Why the scanner probes both buses

I2C2's population is known in advance, which makes it a control. If the scan
reports those six addresses, then the scan logic, clock configuration and bus
timing are all proven, and the I2C1 result can be trusted. Scanning only the
bus we care about would leave "found nothing" ambiguous between an empty bus
and a broken scanner.

### Pull-ups

The on-board bus has its own pull-ups fitted, so the scanner leaves the
internal ones off for I2C2.

**The ARDUINO header lines (PB8/PB9) also carry board-side pull-ups —
confirmed empirically 2026-08-01**, not from the schematic: with the header
completely empty, both lines hold high against the STM32's ~40 kΩ internal
pull-down, which only an external resistor well under that can do. (An earlier
draft of this note claimed the header had none; the bench said otherwise.)

Consequences:

- The internal pull-ups enabled in `i2c_scan` for I2C1 are redundant. Harmless
  at 100 kHz, but they can go.
- These plus the SATEL's own 2.2 kΩ will sit in parallel. Fine at 100 kHz;
  recheck the combined strength before raising the bus speed.
- `bus_check`'s "external pull-up present" verdict detects the **board's**
  resistors, so it cannot tell whether a sensor is attached or powered. What
  it can still do is find a shorted line, which is precisely what it did.

### Debugging record: the SDA short (2026-08-01)

First wiring of the SATEL: `i2c_scan` found nothing at 0x29. `bus_check`
showed SCL high but SDA clamped to ground so hard that neither the internal
pull-up nor the board's own pull-up could lift it. With the SATEL fully
disconnected, SDA read clean high — **the short travels with the SATEL
wiring**. The board was never the problem.

Lesson that cost an hour: a line reading high through the header proves only
that the *board's* pull-up is there. The first `bus_check` verdict called SCL
"healthy — sensor powered and grounded" off exactly that misreading, and it
was Tony disconnecting SCL and seeing no change that exposed it.

Fault-isolation procedure that works, for next time: run `pin_wiggle` (live
readback every 5 s), reconnect one wire at a time, and watch for the report
where `PB9 lifts` flips to false — the wire that kills it is the culprit.

**Continued — the short was not the wiring.** Evidence accumulated in order:

1. Rewired from scratch; scan still found nothing; `bus_check` showed the
   same SDA clamp. Same fault across two independent wirings.
2. `bus_check` grew a 500 ms re-sample to distinguish a short from the
   decoupling capacitance of an unpowered rail (both read "low" at 1 ms; only
   copper stays low at 500 ms). Verdict: still low — genuinely held down.
3. DMM at the SATEL pads, powered: AVDD 5.0 V, IOVDD 3.3 V, SCL idles 3.3 V,
   **SDA pad 0 V**. With the SATEL's own 2.2 kΩ pull-up to a live rail, that
   means something sinks >1.5 mA on that net continuously.
4. Jumper wires verified; SDA-to-GND continuity on the breakout, power off:
   **no beep**. Not copper, therefore active silicon.
5. Symptom then spread: SCL stopped reading as well. A slave can only hold
   SCL by clock-stretching from a wedged state — or the part is failing.
6. `I2C_RST` pulsed high (~1 s) per its purpose as the I2C-block reset:
   no recovery, scan still empty.

**Verdict: SATEL #1 sinks its own SDA (later SCL) with good rails, no copper
short, and does not respond to its designed reset. Suspect silicon.** It sank
SDA from its very first transaction, so it may have arrived damaged. Labeled
and kept for a post-mortem against a working baseline.

Next: pre-flight the harness (wire-tip voltages with no sensor attached:
AVDD 5 V, IOVDD 3.3 V, GND continuity, both signal tips ~3.3 V from the board
pull-ups), then SATEL #2 on five wires only — AVDD, IOVDD, GND, SDA, SCL;
INT/LPn/I2C_RST left to the breakout's own resistors. Solder a header strip
to #2 first: push-fit jumpers in castellated holes are the one connection a
meter cannot vouch for, and the rig wants a solid mount anyway.

**RESOLUTION — the sensors were innocent. So was every wire.**

Both SATELs showed the identical clamp on fresh jumper wires, which finally
proved the fault systematic rather than component-level. Tony then mapped the
U585 board's header hole by hole and found it: **the B-U585I-IOT02A's ARDUINO
digital header does not start where the NUCLEO-F446RE's does.** Its first
position is unpopulated and SCL begins one hole down. Wiring by position —
F446RE muscle memory from the bacnet-gateway builds — shifted every signal
wire by one, on every attempt, on both sensors, with any wire set. On a
shifted row some wire always finds a ground or a loaded net, which is where
the unliftable SDA clamp came from.

The earlier "suspect silicon" verdict on SATEL #1 is **withdrawn** — the
chip was reacting to a miswired harness, not failing. Both sensors are fine.

With every wire placed by reading the silkscreen label at each end —
never by counting positions — the bus reads clean, and stayed clean when the
5 V/AVDD wire landed, the step that had produced the clamp on every previous
attempt.

Lessons, in the order they were paid for:

1. **Wire label-to-label, never position-to-position.** Two boards sharing a
   connector standard can still differ in physical hole layout.
2. A pin reading high through a board-side pull-up proves nothing about what
   is (or is not) connected to it.
3. A powered chip with a broken harness can actively clamp a line in ways
   that mimic a copper short — no continuity beep, immune to reset — because
   its return current or a resistor-fed rail is involved. "No beep but 0 V
   under power" means active silicon, and active silicon means look at the
   harness before condemning the part.
4. The fastest debugging tools here were a 30-line GPIO test and a
   multimeter, alternated. Neither alone was sufficient.

### VL53L5CX-SATEL wiring

Authority: **AN5717 Rev 4**, *How to setup and run the VL53L5CX-SATEL using an
STM32 Nucleo64 board*, Table 1. Pin names on our board read off the silkscreen
directly (see `images/`); the breakout carries eight pads:

`INT · I2C_RST · SDA · SCL · LPn · AVDD · IOVDD · GND`

**The supplies are not interchangeable and neither is 3.3 V:**

| SATEL pin | Connect to | Notes |
|-----------|-----------|-------|
| `AVDD` | **+5 V** | analog supply, an **input** |
| `IOVDD` | **+3.3 V** | I/O supply, also an **input** — not a regulator output |
| `GND` | GND | |
| `SCL` | D15 → **PB8** | |
| `SDA` | D14 → **PB9** | |
| `LPn` | free GPIO, or tie to 3.3 V | must be high for I2C to answer |
| `I2C_RST` | free GPIO, or tie to GND | active high; low = not reset |
| `INT` | free GPIO | optional — poll for data-ready during bring-up |

Web sources disagreed on whether `IOVDD` was an input or a regulator output,
which is exactly the kind of thing not to resolve by majority vote — driving
3.3 V into a regulator output damages the part. AN5717 settles it: both are
inputs, and they come from two different rails.

Power sequencing from the VL53L5CX datasheet: **IOVDD must come up at the same
time as, or after, AVDD.** Powering IOVDD while AVDD is dead raises leakage.
Wiring both to the board's own rails satisfies this automatically, since they
come up together.

**Convenient coincidence:** ST's reference wiring is to a NUCLEO-F401RE, where
`SCL` = D15 = **PB8** and `SDA` = D14 = **PB9** — the same STM32 port pins our
U585 uses for its ARDUINO header I2C. That is the Arduino Uno V3 standard
mapping doing its job, and it means AN5717's I2C connections transfer to this
board unchanged.

**Do not break the board.** The SATEL is perforated so the sensor mini-PCB can
be snapped off for 3.3 V flying-lead use, which is a different supply
arrangement from the one above. Table 1 describes the intact board; keep it
intact.

### AMG8833 verified with a hot mug — PASSED, 2026-08-02

Both sensors are now confirmed against real targets. The thermal response:

| Frame | Reading | What was happening |
|-------|---------|--------------------|
| 20 | span 475 | baseline, empty room |
| 22 | span 2475, `4625` bottom-left | mug entering the field of view |
| 28 | span 3850 | mug closer, hot region spreading |
| 96 | median 6675, peak 6900 | mug filling most of the view |
| 162 | span 650 | mug withdrawn, back to baseline |

Peak pixel 7150 c°C — 71.5 °C, right for hot tea. It appeared, grew and
disappeared, which is what rules out a stuck reading rather than the peak
value alone. The mug entered from the **bottom-left**, which fixes the
sensor's orientation relative to the scene.

**Finding: the sensor retains heat, and the whole image shifts with it.**

Baseline before the mug: median ~2075–2225, die 2331. Baseline twenty
seconds after removing it: median ~2200–2350, die 2456. The die gained
1.25 °C and every pixel rose with it.

A thermopile measures the difference between its own package and what it is
pointed at, so anything that warms the package shifts the entire frame. A mug
did it in under a minute; a person sitting nearby for an hour will do it more
slowly and more permanently.

Three consequences worth carrying into Phase 2 and 3:

1. **Absolute temperature thresholds will not work.** "Warmer than 25 °C
   means occupied" drifts with the sensor's own history, never mind the
   season. The frame-median reference already in `thermal_probe` sidesteps
   this by construction — half of any realistic frame is background, so the
   reference moves with the package. That design choice was made for a
   different reason and turns out to matter more than intended.
2. **Consecutive frames are not independent samples.** The sensor carries
   thermal state, so a frame recorded moments after a person leaves still
   contains them. Data collection needs settling time between classes, or
   the `empty` set will quietly contain the warm ghost of the previous
   session.
3. **This is a real signal, not only a nuisance.** A recently vacated chair
   stays warm. That is genuinely useful for occupancy — it distinguishes "was
   occupied a minute ago" from "empty all morning" — but only if it is
   deliberate rather than an accident in the training data.

### Phase 1 sensing checkpoint — PASSED, 2026-08-02

`tof_range` streaming 8×8 at 10 Hz, hand and object motion tracked correctly.
Representative frames from the first real run:

| Frame | Reading | Interpretation |
|-------|---------|----------------|
| 45 | `46..79 mm`, 57/64 valid, uniform | flat surface against the sensor |
| 190 | `481..1147 mm`, 54/64, smooth gradient | angled surface across the FoV |
| 210 | `293..1716 mm`, 60/64 | wall at ~1.5 m, object at 300 mm in one corner |
| 205 | `0/64 valid` | open space — nothing in range. Correct, not a fault |

**Two findings that matter for later phases:**

**1. Silicon temperature climbed 34 °C → 44 °C over a few minutes of ranging.**
The sensor reports this per frame (`silicon_temp_degc`) and it is a real,
self-generated hot spot roughly 20 °C above ambient. This is direct evidence
for the rig-design decision: the AMG8833 is a thermopile measuring radiated
heat, and mounting it hard against a part that sits at 44 °C would put a
permanent bright corner in its 8×8 grid — present in every class equally, so
it would quietly consume dynamic range rather than showing up as an obvious
fault. Leave a physical gap, and prefer having the thermal sensor upstream of
the ToF in any airflow.

**2. Valid-zone count swings enormously — 0/64 to 64/64 — with no fault.**
A zone reports no valid target when nothing is in range, which for
room-scale mounting will be common (ceilings, doorways, open space beyond the
sensor's reach). Phase 3 has to decide explicitly what an invalid zone means
as model input. It is *not* missing data to be imputed away: "nothing within
range in this direction" is a real, informative observation about the room.
Encoding it as 0 mm would be actively misleading — that reads as "something
touching the sensor", the exact opposite. Worth deciding before training, not
after the confusion matrix disappoints.

### FIRST LIGHT — 0x29 answered, 2026-08-02

```
I2C1  (PB8 SCL / PB9 SDA)  — ARDUINO header, D15/D14
    0x29  VL53L5CX 8x8 ToF  <-- target
    1 device(s)
```

**Root cause of the entire saga: the ARDUINO digital header is `CN13`, and
its pin order runs opposite to the assumption.** `SCL` is the **end pin**
(top right as the board sits label-up), with `SDA` immediately beside it.
Every wiring attempt before this placed those two by the ordering carried
over from the NUCLEO-F446RE, and landed them elsewhere on the row.

That single fact explains every symptom we chased for hours:

- SDA "hard shorted to ground" — the SDA wire was in a genuine GND pin of
  CN13, which is exactly what a hard short looks like, because it *was* one.
- SCL "healthy" — PB8 was reading the board's own pull-up while the SCL wire
  sat in an unrelated hole.
- Immune to `I2C_RST`, unchanged by swapping sensors, reproducible across
  three complete rewirings — because every rewiring reproduced the same
  wrong ordering.
- `find_pwren` finding nothing on A0–A5 — the sensor could not answer
  regardless of `PWR_EN`, because its data line was grounded.

Both SATELs are fine. Neither was ever damaged.

**The lesson, now paid for in full: on an unfamiliar board, establish pin
positions from the board itself before wiring anything.** Not from an
application note written for a different board, not from a devicetree, not
from a photograph — all three were consulted, and all three were consistent
with a wiring that did not work, because they described *which signal is on
which pin name* and never *which physical hole that pin name occupies*.

### The wiring (2026-08-01 — revised after the PWR_EN discovery)

**The SATEL is two boards in one, and they take different supplies.** The
carrier section has a labeled hole row and an LDO (`U1`); the perforated
mini-PCB carries the sensor and its own castellation labels. The same name
means different things on each:

- **Carrier `AVDD` hole = LDO input, takes 5 V.** The LDO drops it to the
  3.3 V the chip wants. **`PWR_EN` enables the LDO, defaults OFF, and must be
  tied high** — it is a board-control pin, not a sensor pin. AN5717 Table 1
  wires it to A5 and ST's example code drives it high.
- **Mini-PCB castellation `AVDD` = the raw chip rail, 3.3 V maximum.**
  AN5717 calls the snapped mini-PCB "a 3.3 V supply application". 5 V here
  is out of spec for the bare sensor.

**Correct hookup for the intact board — six wires at the carrier holes,
each placed by reading the silkscreen label at both ends:**

| Board label | SATEL carrier label | Purpose |
|-------------|--------------------|---------|
| `5V` | `AVDD` | LDO input |
| `3V3` | `IOVDD` | I/O rail |
| `A5` | `PWR_EN` | **LDO enable — without this the sensor is dark.** A5 = **PB0** on this board (UM2839 Table 24 + Zephyr DTS agree; a NUCLEO-64's A5 is PC0). Driven high by firmware, which also buys a programmatic hard power-cycle of the sensor later |
| `GND` | `GND` | common ground |
| `CN13` **end pin** | `SCL` | I2C1 clock (PB8). **The end of the row — top right, board label-up.** Not where NUCLEO-64 habit puts it |
| `CN13` **second pin** | `SDA` | I2C1 data (PB9). Immediately beside SCL |

`INT`, `LPn`, `I2C_RST` stay unconnected: the breakout's resistors hold
`LPn` high (comms enabled) and `I2C_RST` low (not reset). `INT` becomes
relevant when frame-ready moves from polling to interrupts later in Phase 1 —
any free header GPIO will do; ST's reference uses A1/A2/A3.

An earlier revision of this section specified five wires by castellation
label, omitting `PWR_EN` (the castellations don't have one) and sending 5 V
to what may have been the raw rail. Symptom of the missing `PWR_EN`:
perfectly clean bus, correct voltages at the input holes, and total silence
at 0x29 — a regulator that is off looks exactly like a sensor that is
absent.

**Board trap, the expensive kind:** the B-U585I-IOT02A's ARDUINO digital
header is not populated identically to a NUCLEO-64's — the first position is
empty and `SCL` starts one hole down. Position-counted wiring transplanted
from F446RE habit lands every wire one off. The full story is in the
debugging record.

---

### Re-verification after the break — 2026-09-26

The rig was built on 2026-08-03 and then not touched for eight weeks. Re-checked
before trusting anything it produces. Toolchain: Rust 1.92.0, probe-rs 0.30.0,
ST-LINK V3 `002300453432511530343838` — same probe as the August work.

Everything still works. Firmware builds clean, both buses enumerate as before:
I2C2 shows the seven on-board devices, I2C1 shows `0x29` and `0x69`.

**The AMG8833 is at 0x69, not 0x68.** This is not a regression and not a change
— the SparkFun breakout straps AD_SELECT high, and `thermal_probe` has probed
both addresses since August precisely because of it. What was wrong was the
*documentation*: the roadmap, `CLAUDE.md` and `i2c_scan`'s own doc comment all
named 0x68, and the scanner printed `<-- target` against 0x68 while the real
sensor sat unmarked at 0x69. Knowledge that lives only in a source comment is
knowledge that gets contradicted by the summary someone actually reads. Fixed
in all four places.

**Baseline before any ranging:** scene median 22.75 °C, span 2.3 °C,
die 27.6 °C. The frame is not flat — the right-hand columns run roughly 1 °C
warmer than the left. Worth knowing that this gradient exists *before*
attributing any later one to the ToF.

**VL53L5CX silicon reached 42 °C within five frames of ranging**, against the
34 → 44 °C range recorded in August — then kept climbing: 48 °C at ~5 min,
51 °C at ~8 min, still rising when the soak ended. August's 44 °C was not a
plateau, it was a shorter measurement. Two things follow. The steady-state
temperature is higher than recorded and not yet established, and the shield
is a plausible cause — soldered to protoboard the part has less room to dump
heat than it had on jumpers in open air. Treat 44 °C as a lower bound, not a
maximum, anywhere it is quoted.

### Mount geometry, and a reflectance theory that did not survive — 2026-09-26

A long bench session spent deciding where the sensor actually goes. Worth
recording in full because most of it is reasoning that would otherwise be
re-run from scratch, and because the headline hypothesis turned out to be
wrong — which is the part most likely to be repeated if it is not written
down.

#### The mount

Available gear: a Neewer desk-clamp stand with a boom, reaching **166 cm**
maximum, and a 1 m tripod. Nothing that reaches a ceiling.

Operator height is 180 cm, so **the sensor sits below head height**. That one
fact decides the product. Overhead mounting is out: you cannot stand under a
sensor at 166 cm, and from directly above, a seated head at ~130 cm is only
36 cm away, where the ToF's field spans barely 33 cm. So this is a
**wall-height, cross-room sensor**, not a ceiling one.

That is not a consolation. A sensor that works at wall height installs
anywhere; one that needs ceiling access does not — the same retrofit argument
the roadmap makes about cabling applies to mounting.

Orientations considered and rejected:

- **Facing up.** Puts ceiling luminaires in the thermal field. They are hot,
  large in a 60° field, and switch on and off *with occupancy* — precisely the
  shortcut the VEML6030 was excluded to avoid, but hidden inside 64 thermal
  pixels where it cannot be attributed. Also collects dust and sees desk
  clutter. Rejected outright.
- **Straight down.** Correct geometry in principle — no occlusion, floor as a
  constant baseline, head height maps to distance, and it makes `sitting` vs
  `standing` nearly free. Impossible at this height for the reason above.
- **Tilted down, facing out.** What we settled on.

#### Sensor arrangement — the thermal question answered itself

Mounting the board in portrait for the tilt happens to put the **GridEYE below
the SATEL**. The ToF's plume therefore rises away from the thermopile instead
of across it, which is the arrangement the earlier TODO wanted and would
otherwise have needed shield rework. Convection is no longer the concern;
only conduction through the shared protoboard remains, which is a much weaker
effect and still unmeasured.

Rotating to portrait **voids the earlier orientation characterisation**. The
hot-mug finding that a target "entered from the bottom-left" no longer holds,
and the SATEL's grid orientation was never established at all. Both need
redoing in the final pose. One provisional data point arrived for free: with a
vertical target (A3 sheet held at chest height), **row 0 reads nearer than
row 1**. On a flat vertical surface the closest point is the one level with the
sensor and points below are farther, so **row 0 is the top of the world view**.
Confirm with the mug before relying on it.

#### What the ranging actually showed

Empty room, final pose, typical frame:

```
 -1    -1    -1   1893  1743   -1    -1    -1
 -1    -1   1940   -1    -1    -1    -1    -1
 -1    -1   2298   -1    -1    -1    -1    -1
 -1    -1    -1    -1    -1    -1    -1   856
 -1   2034   -1    -1    -1    -1    -1   889
1845  1909   -1    -1    -1    -1   1004  916
1719  1778  1792  1276  1244   -1   1073  971
1599  1652  1656  1198  1208  1208  1156  984
```

- **A persistent strip down column 7 at 0.85–0.98 m.** Static, one column wide,
  nearest at the top — the signature of a flat surface at the frame edge. The
  assembly is mounted hard against the door jamb; most likely that. Move the
  boom clear of it.
- **Only 24–26 of 64 zones valid.** Three-quarters of the array is pointed at
  space beyond useful range.
- An earlier pose over the desk produced 0.98 m returns across the bottom rows.
  Useful invariant for diagnosing these: **from a sensor at height h, the slant
  range to the floor can never be less than h.** Anything reading below 1.66 m
  here is an object, not the floor.

#### The reflectance test — hypothesis rejected

A person standing ~2 m away (near the shoe rack) produced **no detectable
change at all**: 22–25 valid zones against 24–26 empty, same pattern
everywhere. An A3 white sheet in roughly the same area showed up clearly.
Initial reading: the ToF cannot see dark clothing or dark skin at 940 nm,
which would have been a strong argument for sensor fusion.

**That reading was wrong.** Two confounds, found by making the test stricter:

1. **Target size, not surface.** At 45° across 8 zones, each zone subtends
   ~5.6°, so **one zone covers roughly 15 cm at 1.5 m**. A bare forearm
   (~10 cm) underfills a single zone and returns weakly and intermittently.
   The A3 sheet (30 × 42 cm) fills two to three zones completely. That alone
   explains the difference with no reflectance effect whatsoever.
2. **The spot moved between captures**, and a fluorescent light was switched
   on partway through, changing ambient IR as well as the target.

The size-matched, fixed-lighting repeat settles it. Same position, same pose,
light off, A3 held flat against the chest and then dropped:

| zone  | empty | paper on chest | person, no paper |
|-------|-------|----------------|------------------|
| r0 c3 | 1893  | 1478           | **1510**         |
| r0 c4 | 1743  | 1403           | **1450**         |
| r0 c5 | —     | —              | **1488**         |
| r1 c3 | —     | 1663           | **1731**         |
| r1 c4 | —     | 1617           | **1624**         |

**The unaided person returns more zones than the paper, not fewer.** Grey
clothing and dark skin are both perfectly adequate 940 nm targets. Melanin
absorption falls off steeply above ~700 nm, so skin-tone variation in the near
infrared is far smaller than in visible light — a genuine advantage of active
NIR sensing over camera-based approaches, and worth stating in the write-up.
Visible colour is in any case a poor predictor of NIR reflectance: carbon-black
absorbs across both, while many dyes that look dark are bright at 940 nm.

**Consequence: the earlier null was position, not surface.** At ~2 m the
subject was outside the usable cone. And the fusion argument this looked like
it was going to supply **does not exist** — do not write it up. If fusion needs
justifying, it has to be on something real, most likely stationary occupants,
where the thermopile sees what the ToF cannot.

#### Confirmed problem: only two rows are usable

Every person-height return, in every capture, landed in **rows 0–1**. Sixteen
of sixty-four zones. The remainder is near floor, the door-jamb strip, and
out-of-range space.

Geometry says a person at 2 m spans ~44° from head to feet, almost exactly the
sensor's 45° field — they should nearly fill the frame. Landing on the top two
rows means the array is aimed far too steeply, at floor much closer in.
**Aiming criterion: adjust the tilt until a person at working distance sits in
the middle rows, not the top edge.** Training on 16 usable zones would sink
Gate 3.

#### Resolution ceiling

Zone size is roughly **10% of range**. A 45 cm torso covers about 3 zones at
1.5 m, 1.5 zones at 3 m, and a single zone at 4.5 m. Past ~3 m a person stops
having a shape and becomes one pixel, so anything posture-based — which is
what separates `sitting` from `standing` — has a hard ceiling around there.
This argues for a deliberately close sensing zone and supports framing the
product as **workstation occupancy** rather than room-scale coverage.

#### Data quality note: a literal 0 mm reading

One frame returned **0 mm** at r7 c0. Zero is physically meaningless — nothing
is touching the cover glass. The driver is emitting 0 as a value rather than a
sentinel, so unfiltered it would assert the exact opposite of the truth. This
sharpens the open TODO on invalid-zone encoding: "invalid" is not one
condition. Out-of-range, too-dark-to-return, and a spurious 0 are three
different things and should not collapse into one number. Decide before any
CSV is recorded.

#### Lighting must be a controlled variable

A fluorescent lamp was switched on mid-session and contaminated a comparison.
Three reasons it matters:

- The VL53L5CX supplies its own 940 nm illumination and works in darkness.
  Ambient IR only adds shot noise and **reduces** effective range — switching a
  light on to help it see works against you.
- A lamp is a warm object for the GridEYE, and it is on when people are
  present. Same shortcut family as the ceiling lights.
- It changed between two captures that were being compared.

**Rule for Gate 2: pick a lighting state and hold it for every capture, or
record it per session and balance it across classes. Never let it track the
class.** The photographic lamp on the stand is for filming — which means it is
on exactly for the sessions that produce video ground truth. Same rule applies.

#### Open

- Reduce the tilt until people land in the middle rows; re-baseline the empty
  room afterwards, since every number above is tied to the current pose.
- Move the boom clear of the door jamb and confirm column 7 frees up.
- **Measure usable depth:** walk out in half-metre steps and record where
  detection is lost. Detection is solid at ~1.5 m and absent at ~2 m, but
  whether that is the cone or the range is not yet known. That number decides
  the size of the zone the product can honestly claim.
- Redo the hot-mug orientation characterisation for both sensors in the final
  pose, and confirm row 0 is the top.
- Mark the ball head, clamp position and column extension. A long column with
  an offset load on a ball joint, clamped to a desk people lean on, is the
  geometry-drift risk in physical form.
- The sunlit window is an uncontrolled thermal source: sunlit patches are warm,
  move through the day, and correlate with time rather than occupancy. Collect
  across times and weather; record as a known limitation.

### Fusion demonstrated: ToF blind, thermal clear — 2026-09-26

The best measurement of the session, and the one worth building the write-up
around. Four captures, same pose, same lighting, minutes apart, each with its
control.

**Setup.** Sensor at 166 cm on the boom, tilted **15° below horizontal**
(reduced from ~34° earlier in the session — see the derivation below).
Subject 180 cm tall, standing 180 cm from the sensor in a doorway. A Philips
tower fan, not running, at 130 cm. Room lighting off and unchanged throughout.

#### Setting the tilt, derived rather than guessed

A 180 cm person at 2 m, seen from 166 cm, spans from **+4.0°** (head, 14 cm
above the sensor) to **−39.7°** (feet, 166 cm below): 43.7° of vertical angle
against a 45° field. They should almost exactly fill the frame, and the
midpoint of that span is **−17.9°**. 15° was chosen to bias slightly toward
keeping the head in frame at the cost of the feet.

Sanity check in the other direction: at 75° below horizontal the field spans
52.5°–97.5° depression and a person at 2 m is entirely outside it. The angle
is measured **from horizontal**; the board ends up nearly upright, leaning
back slightly. At 15° the axis drops 27 cm per metre, so from 166 cm the aim
point is 112 cm high at 2 m.

The change did what it was supposed to: valid zones went from 24–26 to
**38–42 of 64**.

#### ToF: the subject is invisible

Empty and occupied frames are identical to within tens of millimetres.

| zone | empty | subject at 1.8 m |
|------|-------|------------------|
| r2 c1 | 1811 | 1822 |
| r3 c1 | 1756 | 1713 |
| r4 c1 | 1714 | 1664 |
| valid zones | 39–42 | 38–41 |

**Cause: no depth contrast.** The subject is 180 cm from the sensor and the
wall behind them already reads **1.71–1.98 m** in those same zones. A ToF
measures distance; a person standing against a background at their own range
has almost nothing to be nearer *than*. The residual shift is ≤5 cm, which is
inside the frame-to-frame variation.

This is a fundamental property of the sensor, not a defect and not a tuning
problem. It is also not reflectance — that hypothesis was tested and rejected
separately (see the previous section).

#### Thermal: the subject is obvious

Difference map, subject minus empty, in centi-°C:

```
col:      1     2     3     4     5
row0:   +25  +150  +350  +325   +50
row1:     0  +250  +200  +200   -25
row2:   -25  +125  +250   -25   -50
row3:     0  +100  +200     0   -75
```

A contiguous blob across columns 2–4, rows 0–3, peaking at **+3.5 °C** over a
21.0 °C background. Frame span doubles from 200–250 c°C empty to 475–550 c°C
occupied.

#### The asymmetry is the whole argument

- The **fan** dominates columns 3–7 of the ToF frame and is **completely
  invisible** to the thermal sensor — it sits at ambient, so thermally it does
  not exist.
- The **person** is invisible to the ToF and unmistakable in the thermal.

One frame pair, both failure modes, in opposite directions. Each sensor is
blind to exactly what the other sees clearly. That is a demonstrated reason to
fuse them, and it is far stronger than the usual assertion that two modalities
are better than one. **This pair belongs in the blog post.**

#### Caveats, recorded honestly

**Zone averaging dilutes temperature.** The subject peaks at 25.0 °C, not the
~31 °C of real skin and clothing. At 1.8 m each thermal zone covers about
24 cm, so a zone containing part of a person and part of a wall reports the
mixture — roughly 40% fill on the hottest zone. Absolute temperature
thresholds will therefore be unreliable at distance, which argues for a
learned model over a fixed threshold. It also means apparent temperature falls
with range, so range and class must not be correlated in the training set.

**Global drift between captures.** Off-target zones moved by up to ±125 c°C
between the two frames, while the die temperature moved 0.19 °C and the frame
median 0.13 °C. The peak signal is therefore only about **3× the background
drift** — a real result, but not a comfortable margin. This is direct evidence
for the HTS221 ambient-reference item: slow ambient drift currently rides on
top of every reading and can masquerade as signal.

#### Consequences for the collection procedure

- **Site the sensing zone so subjects have depth behind them.** A doorway with
  a wall at arm's reach is the worst case available, which made it a useful
  accident. Aim across open space so the background is well beyond the subject.
- **Keep clutter out of the near field.** The fan is a metre-tall, highly
  reflective column at 130 cm occupying more than half the ToF array. Move it,
  and treat "nothing within ~1 m of the field" as a precondition for any
  capture.
- The ToF cannot be the sole presence signal at these ranges. Any class
  definition that assumes it will see a stationary person against a wall is
  wrong.

### Thermal dilution, and why peak temperature cannot classify — 2026-09-26

A hot mug of coffee turns out to be the most instructive target of the day.

#### Dilution is quantitative and it dominates at range

Mug held at 1.8 m: a single dominant zone at r1c4, **+2.75 to +3.0 °C** over a
21.0 °C background, with +1.0 to +1.5 °C spill into two neighbours.

At 1.8 m an AMG zone covers ~23.6 cm, so ~557 cm² of scene. A mug presents
roughly 8 x 10 cm to the sensor — about **14% fill**. Working back from
+3.0 °C gives an outer surface near **42 °C**, which is right for ceramic
with hot coffee inside: the wall is far cooler than the liquid.

Compare the bench test of 2026-08-02, where the same class of object read
**71.5 °C** at desk distance and filled most of a zone. Same object, same
physics: 71.5 °C at ~25 cm, +3 °C at 180 cm. **Apparent temperature is a
function of range**, and at room distances dilution dominates everything else.

#### Peak temperature does not discriminate — extent does

| target | peak Δ | zones |
|--------|--------|-------|
| person at 1.8 m | +3.5 °C | 6–8 |
| coffee mug at 1.8 m | +3.0 °C | 1–2 |

A classifier thresholding on the hottest pixel calls a hot drink an occupant,
with near-identical confidence. The discriminator is **spatial extent**, about
4:1 between them. The 8x8 structure is not a refinement over a single-pixel
thermopile — it is the entire reason this works.

Consequences:

- **No fixed temperature thresholds.** They cannot survive varying range.
- **Range must not correlate with class** in the training set, or the model
  learns distance instead of posture.
- **Named false-positive class for the limitations section:** hot drinks,
  laptop vents, radiators, a recently vacated chair. Small hot things look
  like distant people to anything measuring temperature alone.

#### Three objects, three signatures

- **Fan** (130 cm) — dominates columns 3–7 of the ToF, thermally absent
  entirely; it sits at ambient.
- **Person** (180 cm) — invisible to the ToF against a wall at their own
  range, thermally broad and obvious.
- **Mug** (180 cm) — thermally bright, spatially tiny, ToF-negligible.

No single sensor and no single statistic separates all three. This is the
strongest form of the fusion argument available, and every leg of it is now
measured rather than asserted.

#### Orientation mapping — first position, and it already found something

Protocol correction worth recording: the probe must be **alone** in the frame.
A first attempt with the mug held near the subject's head was uninterpretable,
because the person is a 6–8 zone target and the mug is a 1–2 zone one — the
small source vanishes inside the large one, and the frames changed between
captures from movement.

First clean position — mug on a stool, centre, 180 cm out, **48 cm high**,
subject out of view — lands at **r2 c3**, +1.25 to +1.5 °C.

That **does not match prediction**. The mug is 118 cm below the sensor at
180 cm out, so 33.2° below horizontal; with the axis nominally at 15° that is
18.2° below the axis, which at 7.5° per row should be the **seventh row from
the top**. It landed in the third.

Two candidate explanations, not yet separated:

1. **The AMG grid is vertically flipped** — displayed row 0 is the *bottom* of
   the world view. That alone moves the prediction to displayed row 1, one row
   from what was seen.
2. **The tilt is not 15°.** A flipped grid plus a true tilt near 22° predicts
   r2 exactly. That would mean the ball head crept between setting the angle
   and the capture — the drift risk already on the list, showing up in data.

Note this **contradicts the provisional ToF finding** that row 0 is the top.
There is no reason two independently mounted breakouts should agree, and
every reason to establish each separately.

Signal strength caveat: the mug had cooled from +3.0 °C to +1.25 °C over about
twenty minutes, which is at the inter-capture drift floor. Reheat between
positions or the mapping will be ambiguous where it most needs to be sharp.

### AMG8833 orientation map — resolved, 2026-09-26

Resolves the ambiguity left open in the previous section. Established by
moving a boiling-water mug to known positions and, for the direction
questions, by sweeping it live in `tools/heatmap_viewer.py` — which answered
in seconds what capture-and-analyse had spent eight round trips failing to
pin down.

> **Standing behind the sensor and looking out along its axis: column 0 is on
> your left and column 7 on your right. Row 0 is at the bottom of the view and
> row 7 at the top.**

So the grid is **horizontally normal and vertically inverted** in the sensor's
own frame.

Evidence:

- **Vertical.** Mug at 180 cm out, centre: 48 cm high → r2; raised to 120 cm
  → r4. Higher in the world gives a *higher* row index. Predicted 2.5 rows of
  movement, observed 2 — within single-zone quantisation.
- **Horizontal.** Facing the sensor and moving the mug to the right moves the
  blob *left* on screen. The subject's right is the sensor's left, so
  sensor-left maps to low column numbers. Consistent with the earlier capture
  where a 50 cm lateral move ran c3 → c5.

#### Bonus: the sensor measured its own mounting angle

With the row map known, each mug position gives an independent estimate of the
optical axis:

- mug at 120 cm sits 14.3° below horizontal, lands +3.75° above axis → **18.1°**
- mug at 48 cm sits 33.2° below horizontal, lands 11.25° below axis → **21.9°**

So the mount is at roughly **20°, not the 15° that was set**. The 4° spread is
single-zone quantisation. Either the ball head crept after the angle was set,
or 15° was never achieved — the drift risk already on the list, showing up in
data within an hour of being flagged.

Worth keeping as a procedure: **two known targets at different heights
recover the mount angle**, with no inclinometer and no trusting the ball head.

#### Consequence: everything watched today was upside down

Every heatmap in this session rendered with the floor at the top. **Fix it in
the driver, not the viewer** — one place, so the CSV, the model input and the
live display all agree with the world. Flipping only the viewer leaves
recorded data disagreeing with what was watched while recording it, which is
the worst of the three options.

#### Still open, and it is the one that costs: ToF registration

The ToF's orientation is **not** established. The only evidence — A3 sheet
held against the chest, where r0 read nearer than r1, which on a vertical
surface means r0 is higher — points to **row 0 being the top**, the opposite
of the AMG.

If that holds, the two grids are vertically opposed. Gate 3 concatenates them
into one 128-value vector, so the model would be handed two mirror-image views
of the same scene: recoverable by a dense net at the cost of capacity, and
actively wrong for anything convolutional. Establish it before a single frame
is recorded.

### Choosing a thermal probe: area beats temperature — 2026-09-26

A 450 °C miniature soldering iron was considered as a stronger probe than the
mug and rejected on the arithmetic, which follows directly from the dilution
result.

| probe | radiating area | excess over ambient | zone fill at 1.8 m | predicted Δ |
|-------|----------------|---------------------|--------------------|-------------|
| mug | ~80 cm² | ~21 °C | 14% | **+3.0 °C** (observed) |
| iron tip | ~0.3 cm² | ~430 °C | 0.05% | +0.2 °C |
| iron barrel | ~5 cm² | ~180 °C | 0.9% | +1.6 °C |

The mug wins because it is **large**, not because it is hot. At room distances
the zone-fill fraction dominates everything else.

Second reason to reject it: the AMG8833 measures roughly **0–80 °C**. Close
enough to fill a zone the iron saturates and the reading is meaningless; far
enough not to saturate it is diluted into the noise. Bad at both ends.

**What a good probe looks like:** large area, moderate temperature, high
emissivity. A hot water bottle is close to ideal — big, holds temperature for
an hour so no reheating between positions, and fabric or rubber is near
emissivity 1.

**Better still, one target both sensors can see.** Mapping the thermal grid
with a mug and the ToF grid with a sheet means two probes, two placements and
two sets of positioning error before the two maps can even be compared. A hot
water bottle taped behind a piece of card gives the ToF a flat surface at a
known distance *and* the thermal a warm area in the same place — so whether
the two grids agree becomes directly readable rather than inferred. Build one
before resuming.

#### Why the ToF sweep was unreadable

Attempting the same sweep on the ToF with the A3 sheet gave a blob "all over
the place". Two causes, both fixable:

- **Scene clutter.** The fan still occupies half the array at 130 cm, and the
  sheet competes with walls and floor. Hold the target clearly nearer than its
  background — at 1.2 m against a 1.8–2.0 m wall there is 60–80 cm of
  separation and it stands out unambiguously.
- **Per-frame auto-scaling in the viewer.** The colour mapping rescales every
  frame, so in a busy scene everything appears to jump. This is the
  `--fixed-scale` item already in TODO: auto-scaling is right for aiming at a
  single target and wrong the moment frames are being compared. This session
  is the argument for implementing it.

## Vendor driver

VL53L5CX ULD **2.0.1** is on disk at
`~/dev/6-month-plan/downloads/VL53L5CX_ULD_driver_2.0.1/`, alongside UM2884
(the ULD guide).

Layout that matters:

- `VL53L5CX_ULD_API/inc` + `src` — the driver proper. This is the vendor code
  that gets copied into `firmware/vendor/vl53l5cx/` and compiled by `cc`.
- `Platform/platform.h` — the contract we implement. Six mandatory functions
  and one optional:

  | Function | Role |
  |----------|------|
  | `VL53L5CX_RdByte` | read one byte from a 16-bit register address |
  | `VL53L5CX_WrByte` | write one byte |
  | `VL53L5CX_RdMulti` | read a buffer |
  | `VL53L5CX_WrMulti` | write a buffer |
  | `VL53L5CX_SwapBuffer` | byte-swap a buffer, size always a multiple of 4 |
  | `VL53L5CX_WaitMs` | delay |
  | `VL53L5CX_Reset_Sensor` | optional; hardware reset, unused by the API |

  Only the I2C pair and the delay touch hardware, which is what makes this a
  good fit for a thin FFI shim over Embassy's I2C — the `unsafe` is
  concentrated in a handful of functions rather than smeared through a HAL.

- `Examples/Example_1_ranging_basic.c` — the reference call sequence to follow
  for the first frame.
- `CubeIDE_F401RE_Example/` — a full CubeIDE project for an F401RE. Not our
  target and not our toolchain, but `Core/Src/platform.c` is a worked
  implementation of the platform layer against ST's HAL and is worth reading
  before writing ours.

`VL53L5CX_NB_TARGET_PER_ZONE` defaults to 1 in `platform.h`. That is the right
value for occupancy and keeps the RAM footprint and I2C traffic down; the
`VL53L5CX_DISABLE_*` macros in the same header are the lever to pull if the
frame transfer needs trimming later.
