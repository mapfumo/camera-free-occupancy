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

### The wiring as built (2026-08-01, verified electrically)

Five wires, **each placed by reading the silkscreen label at both ends** —
see the debugging record below for why that rule is written in bold:

| Board label | SATEL label | Purpose |
|-------------|-------------|---------|
| `5V` | `AVDD` | analog supply — confirmed present and measured 5.0 V |
| `3V3` | `IOVDD` | I/O supply, 3.3 V |
| `GND` | `GND` | common ground |
| `D15 SCL` | `SCL` | I2C1 clock (PB8) |
| `D14` | `SDA` | I2C1 data (PB9) |

`INT`, `LPn`, `I2C_RST` are left unconnected: the breakout's own resistors
hold `LPn` high (comms enabled) and `I2C_RST` low (not reset). `INT` becomes
relevant when frame-ready moves from polling to interrupts later in Phase 1 —
any free header GPIO will do at that point; ST's reference uses A1/A2/A3.

**Board trap, the expensive kind:** the B-U585I-IOT02A's ARDUINO digital
header is not populated identically to a NUCLEO-64's — the first position is
empty and `SCL` starts one hole down. Position-counted wiring transplanted
from F446RE habit lands every wire one off. The full story is in the
debugging record.

---

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
