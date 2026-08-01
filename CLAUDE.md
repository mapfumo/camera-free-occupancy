# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

CoworkOS is a privacy-preserving room occupancy detection system. It uses a VL53L5CX 8×8 Time-of-Flight sensor (no camera, no images) to classify a space as empty/occupied/count, running inference on-device, and feeds the result into the existing BACnet/Modbus/LoRaWAN Grafana pipeline.

**Hardware:**
- B-U585I-IOT02A Discovery Kit (STM32U585, Cortex-M33)
- VL53L5CX-SATEL (8×8 zone Time-of-Flight sensor, I2C) — distance + reflectance per zone
- SparkFun Grid-EYE AMG8833 (8×8 thermopile IR array, I2C) — radiated heat per pixel
- HiLetgo ILI9341 TFT (240×320, SPI) — on-device readout added in Phase 4: class, confidence, small live heatmap. **Display only — do not wire up the touch panel.**

**Software stack:**
- Rust firmware using Embassy async framework
- VL53L5CX ULD (C driver) linked via FFI (`cc` + `bindgen`)
- STM32Cube AI Studio — generates optimized C inference code, also linked via FFI
- Same Python/MQTT/InfluxDB/Grafana pipeline as the related projects below

## Rust Target & Toolchain

The STM32U585 is Cortex-M33 with TrustZone and FPU. The correct Rust target is **`thumbv8m.main-none-eabihf`** (not `thumbv7em` used by older STM32F4/STM32WL projects).

```toml
# .cargo/config.toml
[target.'cfg(all(target_arch = "arm", target_os = "none"))']
runner = "probe-rs run --chip STM32U585AIIx"

rustflags = [
  "-C", "link-arg=-Tlink.x",
  "-C", "link-arg=-Tdefmt.x",
]

[build]
target = "thumbv8m.main-none-eabihf"

[env]
DEFMT_LOG = "debug"
```

```bash
# Prerequisites (one-time)
rustup target add thumbv8m.main-none-eabihf
cargo install probe-rs-tools

# Build
cargo build --release

# Flash + run (streams defmt logs)
cargo run --release

# Flash specific board by probe ID
cargo run --release --probe <PROBE_ID>
```

List available probes: `probe-rs list`

## Architecture

```
VL53L5CX (8×8 ToF, I2C)  +  AMG8833 (8×8 thermal IR, I2C)
        ↓ FFI (C ULD driver)          ↓ Rust I2C driver
           STM32U585 Embassy firmware
        ↓ FFI (STM32Cube AI generated C)
  Edge AI inference (empty / occupied / count)
        ↓ MQTT / LoRaWAN
Existing gateway pipeline
        ↓
  InfluxDB → Grafana
```

**Key design decisions:**
- Inference runs every 1–2 seconds (not every frame) — sufficient for occupancy, saves power
- Both the VL53L5CX C driver and the AI-generated C code are linked via FFI, not rewritten in Rust
- Output integrates as a BACnet object (Binary Input for occupied/empty, or Analog Input for count), matching the pattern in `bacnet-gateway`
- Debounce/hysteresis for classification flapping is handled in firmware, not post-processed

## FFI Pattern

Both the sensor driver and the AI inference code are C libraries linked into the Rust firmware. Follow this pattern (same as the VL53L5CX ULD and Cube AI workflow):

```
build.rs          — runs cc::Build to compile vendor C sources
src/
  vl53l5cx.rs     — safe Rust wrapper around raw FFI bindings
  inference.rs    — safe Rust wrapper around AI-generated C
  main.rs         — Embassy tasks
vendor/
  vl53l5cx/       — ST ULD source (C)
  ai_model/       — STM32Cube AI generated C output
```

Use `bindgen` to generate `bindings.rs` from the C headers, and `cc` crate in `build.rs` to compile the vendor sources. See the `bacnet-gateway` repo for the pattern of calling a vendor C API from Embassy tasks.

## Data Model

New InfluxDB measurement to add to the existing `sensors` bucket:

| Measurement | Tags | Fields |
|------------|------|--------|
| `occupancy_sensor` | node, protocol=tof, ip | occupancy (0/1), people_count, confidence, uptime |

BACnet objects to expose from this device:
- Binary Input 0: Occupied (true/false)
- Analog Input 0: People count
- Analog Value 0: Inference confidence (0.0–1.0)

## Related Projects (reuse patterns from these)

| Repo | Relevant for |
|------|-------------|
| `~/dev/4-month-plan/bacnet-gateway` | Embassy firmware structure, BACnet object mapping, W5500 Ethernet, I2C driver pattern, `build.rs` for C FFI |
| `~/dev/4-month-plan/wk11-unified-monitoring` | Docker Compose stack, InfluxDB bucket/measurement pattern, Grafana dashboard provisioning, `mqtt_to_influx.py` |

Both repos have their own CLAUDE.md with firmware commands, probe IDs, pin assignments, and troubleshooting.

## Development Phases

The project follows `occupancy-sensor-project-plan.md` at the repo root:

1. **Hardware bring-up** — I2C + VL53L5CX ULD via FFI, raw 8×8 frame over UART
2. **Data collection** — labeled CSV frames per class (empty, sitting, standing, walking)
3. **Model training** — STM32Cube AI Studio BYOD flow, validate on held-out data
4. **Firmware integration** — live inference every 1–2s, output over telemetry, plus the ILI9341 readout
5. **Gateway integration** — MQTT topic → BACnet object → Grafana panel
6. **Real-world validation** — multi-day run, edge cases
7. **Write-up** — blog post

Do not add BACnet/gateway integration (Phase 5) before Phase 4 live inference is stable. Do not start model training (Phase 3) before Phase 2 data is visually distinguishable as heatmaps.

## Sensor Notes

**VL53L5CX (ToF):**
- Output: 64 distance values (mm) + 64 signal strength values per frame
- Requires ST's ULD C driver linked via FFI — see Phase 1 in `occupancy-sensor-project-plan.md`
- Lighting-independent; good for position/count
- **Wiring (see NOTES.md for the full story):** I2C1 on PB8 (SCL) / PB9 (SDA) via the ARDUINO header. Six wires to the SATEL **carrier hole row** (not the mini-PCB castellations): `AVDD`←5V, `IOVDD`←3.3V, **`PWR_EN`←`A5` hole (LDO enable — defaults off; firmware drives A5 = PB0 high, per UM2839 Table 24; a NUCLEO-64's A5 is PC0, not this board's)**, `GND`, `SCL`←**`CN13` end pin**, `SDA`←**`CN13` second pin**. Leave `INT`/`LPn`/`I2C_RST` unconnected (on-board resistors). **The ARDUINO digital header is `CN13` and its order runs opposite to a NUCLEO-64's: SCL is the end pin (top right, board label-up), SDA next to it.** Assuming NUCLEO ordering here cost an entire bench session — it lands SDA in a real GND pin, which presents as an unfixable hard short. The mini-PCB castellation `AVDD` is the raw 3.3 V chip rail — never 5 V there. **Wire label-to-label, never by counted position** — this board's ARDUINO header has an unpopulated first position, unlike a NUCLEO-64, and position-counting cost a full bench session.
- ULD address convention is 8-bit (`0x52`); Embassy wants 7-bit (`0x29`). The platform layer in `firmware/src/vl53l5cx/platform.rs` shifts — do not "fix" it.

**AMG8833 (Grid-EYE thermal IR):**
- Output: 64 temperature readings (°C) per frame, 8×8 grid, ~10 Hz max
- I2C address: 0x68 (default) or 0x69 (AD0 pin high) — both sensors can share one I2C bus
- Detects radiated heat regardless of motion — complements ToF for detecting still occupants
- A pure Rust I2C driver is straightforward; no vendor C library needed

**Multi-modal fusion:**
- Both sensors produce 8×8 grids — natural to concatenate into a 128-value input vector (or two-channel 8×8 "image") for a single model
- The AMG8833 compensates for the ToF's weakness on still/motionless occupants
- Confirm both produce visually distinct class heatmaps (Phase 2) before committing to a fused model
