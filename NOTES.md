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

The ARDUINO header has none. The scanner enables the STM32's internal pull-ups
on I2C1 so that probing a bare header gives clean NACKs rather than noise.
**These are a bring-up convenience, not the real design.** They are tens of
kilohms, which is weak for I2C; the breakout boards' own pull-ups are what the
final wiring should rely on. If both end up in parallel that is fine at
100 kHz, but revisit it before raising the bus speed.

### Still to confirm before wiring

- VL53L5CX-SATEL pin-out: which header pins are SDA, SCL, INT, LPn, PWREN,
  and what each needs tying to. **Not yet read** — needs the SATEL user
  manual from ST.
- A free GPIO for the ToF interrupt line. Not needed for the scan; the ULD
  can be polled for data-ready during bring-up and moved to an interrupt once
  frames are flowing.

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
