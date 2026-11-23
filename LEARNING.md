# CoworkOS — How It Works, and Why

A living explanation of this project: what we're building, what each part does,
how the code fits together, and what went wrong along the way.

Written for someone comfortable with software and networks but newer to
embedded. It assumes no prior knowledge of Rust's embedded ecosystem, C
interop, or I2C. **It will be wrong in places as the project changes** — when
that happens, fix it here rather than starting a new document.

---

## Table of contents

1. [What we're building](#1-what-were-building)
2. [The two sensors](#2-the-two-sensors)
3. [The hardware](#3-the-hardware)
4. [I2C in five minutes](#4-i2c-in-five-minutes)
5. [How the firmware is organised](#5-how-the-firmware-is-organised)
6. [Embedded Rust: `no_std` and Embassy](#6-embedded-rust-no_std-and-embassy)
7. [The C interop story](#7-the-c-interop-story)
8. [The AMG8833: the same job in pure Rust](#8-the-amg8833-the-same-job-in-pure-rust)
9. [The diagnostic tools](#9-the-diagnostic-tools)
10. [Building the dataset (Gate 2)](#10-building-the-dataset-gate-2)
11. [From data to a model on the chip (Gate 3)](#11-from-data-to-a-model-on-the-chip-gate-3)
12. [Problems we hit, and what fixed them](#12-problems-we-hit-and-what-fixed-them)
13. [Glossary](#13-glossary)

---

## 1. What we're building

**A sensor that can tell whether a room is occupied, and roughly how many
people are in it, without a camera.**

Why that matters: buildings waste enormous energy heating, cooling and lighting
empty rooms. Occupancy sensing fixes that, but the obvious solution — a camera
with people-detection — creates a privacy problem, a compliance problem, and a
conversation with every employee in the building. A sensor that physically
cannot capture an image sidesteps all three.

The trick is that we use two sensors that each produce an **8×8 grid** of
numbers. Sixty-four values is enough to tell "empty room" from "someone
standing in the doorway", and far too little to identify anybody. You could not
reconstruct a face from it if you tried.

The finished system:

```
  two sensors  ->  microcontroller  ->  small AI model  ->  building network
   (8x8 each)      (runs the model)     (empty/occupied)    (BACnet, LoRaWAN)
```

Everything runs **on the device**. No images, no video, no cloud, nothing
leaving the box except a number saying how many people are in the room.

### The seven phases

| Phase | Goal | Status |
|-------|------|--------|
| 1 | Hardware bring-up — get raw data out of both sensors | nearly done |
| 2 | Data collection — labelled examples of each situation | next |
| 3 | Train a model | |
| 4 | Run the model on the device, live | |
| 5 | Feed results into the building network | |
| 6 | Run it for days, find where it breaks | |
| 7 | Write it up | |

---

## 2. The two sensors

Both produce an 8×8 grid. They measure completely different things, and the
whole design rests on that difference.

### VL53L5CX — "how far away is it?"

A **time-of-flight** sensor. It fires an invisible infrared laser pulse and
measures how long the reflection takes to come back. Light travels about 30 cm
per nanosecond, so this is a genuinely impressive piece of timing.

It does this for 64 zones at once, in an 8×8 pattern spread across its field of
view, giving you a **depth map**: 64 distances in millimetres.

```
   distances in mm, sensor pointed across a room

   1398 1450 1518 1602 1634 1710  322  293
   1438 1489 1590 1598 1681 1706   --   306     <- the two right columns
   1480 1533 1595 1663 1684   --  1507   --        are something close;
   1512 1563 1651 1716 1652 1523 1466 1365         the rest is the far wall
```

**Strength:** it works in complete darkness, and it tells you *where* things
are in space, which is what lets you count separate people.

**Weakness:** it only sees *shapes*. A person sitting motionless at a desk
becomes part of the furniture — geometrically they're just another lump in the
room, and there's no way to tell them from a chair.

### AMG8833 "Grid-EYE" — "how hot is it?"

A **thermopile array**. Sixty-four tiny sensors, each measuring infrared
radiation — heat — coming from one patch of the scene. Not a camera: it doesn't
see light, it senses warmth, the same way you can feel a radiator across a room
without touching it.

Output is 64 temperatures.

**Strength:** people are warm. A person radiates heat whether they're moving,
sitting still, or asleep. This is exactly the case the ToF fails.

**Weakness:** it has no idea about distance or shape. A warm patch could be a
person at 2 m or a laptop at 30 cm. And the difference between a clothed person
and a wall might only be 1–2 °C.

### Why both

|  | ToF | Thermal |
|--|-----|---------|
| Person walking | sees clearly | sees clearly |
| Person sitting still | **blends into furniture** | sees clearly |
| Warm laptop | correctly ignores | **might call it a person** |
| Total darkness | fine | fine |
| Where in the room | precise | vague |

Each sensor's blind spot is the other's strong suit. We give the model both
grids — 64 distances and 64 temperatures, 128 numbers — and let it learn that
"warm **and** person-shaped **and** person-sized" is a person, while "warm but
flat and small" is a laptop.

This is called **sensor fusion**, and it's why the project uses two cheap
sensors rather than one expensive one.

---

## 3. The hardware

### The board: B-U585I-IOT02A

An STM32U585 development board from ST. The chip is an **ARM Cortex-M33**
running at 160 MHz with 2 MB of flash and 786 KB of RAM.

For scale: that's roughly a mid-1990s desktop PC, on a chip the size of a
fingernail, using a fraction of a watt. It's more than enough to run a small
neural network — which is the entire premise of "edge AI".

The board has **Arduino-compatible headers**: rows of sockets around the edge
in the standard Arduino layout, so off-the-shelf add-on boards fit. We use them
to connect the two sensors.

### How things are wired

Both sensors sit on the **same two wires**. That's I2C, explained next.

```
 STM32U585
   PB8  ---- SCL (clock) ----+------------------+
   PB9  ---- SDA (data)  ----|--------------+   |
   PB0  ---- PWR_EN ---------|---------+    |   |
                             |         |    |   |
                        [VL53L5CX]   (pwr) [AMG8833]
                          0x29                0x69
```

`PB8` and `PB9` are pin names on the microcontroller. `0x29` and `0x69` are
addresses — more on those shortly.

`PWR_EN` is a switch: the VL53L5CX's breakout board has a small voltage
regulator on it that is **off by default**, and this pin turns it on. Forgetting
it cost us hours (see [§10](#10-problems-we-hit-and-what-fixed-them)).

---

## 4. I2C in five minutes

Almost everything in this project happens over I2C, and most of our bugs lived
there, so it's worth understanding properly.

**I2C is a two-wire bus that lets one controller talk to many devices.**

- **SCL** — the clock. The controller pulses this to mark off each bit.
- **SDA** — the data. One bit rides on each clock pulse.

If you know Ethernet, the useful comparison is that I2C is a *shared bus* like
old coaxial Ethernet, not switched point-to-point links. Everyone hears
everything; addressing sorts out who should care.

### Addresses

Every device has an address. The controller announces "I want to talk to `0x29`"
and only that device answers. Ours:

| Address | Device |
|---------|--------|
| `0x29` | VL53L5CX (ToF) |
| `0x69` | AMG8833 (thermal) |

That's how two sensors share two wires without conflict.

### The bit that trips everybody: 7-bit vs 8-bit addresses

An I2C address is **7 bits**. On the wire, the controller sends those 7 bits
plus 1 more bit saying "I'm reading" or "I'm writing" — 8 bits total.

This created two conventions, both common, both called "the address":

- **7-bit form**: the address alone. `0x29`.
- **8-bit form**: the address already shifted left, leaving room for the
  read/write bit. `0x52`.

`0x52` is `0x29 << 1`. Same device, two ways of writing it.

ST's driver uses the 8-bit form. Our Rust library uses the 7-bit form. Get this
wrong and **the sensor is completely silent**, with no error explaining why. We
handle it in exactly one place, in `platform.rs`:

```rust
let address = (platform.address >> 1) as u8;   // 0x52 -> 0x29
```

ST's own support forum has people losing days to this exact bug.

### Pull-up resistors

I2C devices can only pull the wires **down** to 0 V. Nothing drives them up.
So each wire needs a **pull-up resistor** — a resistor to the positive supply
that gently holds the line high whenever nobody is pulling it down.

Without pull-ups the bus doesn't work. With them, an idle bus reads high.

This becomes important because it means **a line reading high tells you a
pull-up exists — and nothing else.** We wasted real time forgetting that; see
[§10](#10-problems-we-hit-and-what-fixed-them).

---

## 5. How the firmware is organised

```
firmware/
  build.rs              runs at build time: compiles C, generates bindings
  src/
    lib.rs              shared library: everything reusable
    clock.rs            CPU speed configuration
    amg8833.rs          thermal sensor driver (pure Rust)
    vl53l5cx/
      mod.rs            module root
      bindings.rs       auto-generated Rust view of ST's C driver
      platform.rs       the C-to-Rust bridge (the interesting bit)
    main.rs             the application
    bin/                standalone tools, one per file
      i2c_scan.rs       who is on the bus?
      bus_check.rs      are the wires electrically sane?
      pin_wiggle.rs     which physical pin is this?
      find_pwren.rs     which pin turns the sensor on?
      tof_probe.rs      does the C driver work at all?
      fused_stream.rs   both sensors, paired — the dataset format
      tilt_check.rs     how far down is the rig pointing?
      tof_range.rs      live distance grid
      thermal_probe.rs  live temperature grid
  vendor/
    vl53l5cx/           ST's C driver, unmodified
```

### Why a library plus many small programs

A microcontroller runs **one program at a time**. There's no operating system
loading executables — you flash one binary and it owns the chip.

So each file in `src/bin/` is a **complete, separate program**. `cargo run
--bin i2c_scan` compiles that one and flashes it, replacing whatever was there.

The shared code lives in `lib.rs` and its modules, so every tool uses the same
clock configuration and the same drivers. That matters: a tool that ran the CPU
at a different speed than the application could pass while the application
fails, and you'd never suspect the tool.

---

## 6. Embedded Rust: `no_std` and Embassy

### `no_std` — no operating system underneath

Every file starts with:

```rust
#![no_std]
#![no_main]
```

`no_std` says: **don't use the standard library.** On a PC, Rust's standard
library gives you files, networking, threads, heap allocation — all of which
assume an OS. There is no OS here. There aren't even files.

What remains is `core`: the language itself. Integers, slices, `Option`,
`Result`, arithmetic, iterators. Anything needing memory allocation or the
outside world is gone.

Practical consequences you'll see throughout this codebase:

- **No `Vec`, no `String`, no `HashMap`.** Fixed-size arrays instead:
  `[i16; 64]`, not `Vec<i16>`.
- **No `println!`.** There's no terminal. We use `defmt` instead — see below.
- **No heap.** Everything is on the stack or in static memory. This is a
  feature: you cannot run out of memory at 3 a.m. if you never allocate.

`no_main` says we're not using the normal program entry point, because the
normal one assumes an OS started us. Instead the chip's reset vector jumps
straight into our code.

### Embassy — an async framework for microcontrollers

Embassy provides two things:

**1. Hardware drivers.** `embassy_stm32` knows this specific chip's registers.
Instead of writing `*(0x40005400 as *mut u32) = 0x1;` you write
`I2c::new_blocking(p.I2C1, p.PB8, p.PB9, cfg)`.

**2. An async runtime.** `async`/`await` without threads or an OS. When a task
awaits, Embassy runs another task; when nothing can run, the CPU sleeps. That
sleeping is the point — it's how a battery-powered sensor lasts months.

```rust
Timer::after(Duration::from_millis(200)).await;
```

On a PC that would block a thread. Here the CPU genuinely powers down most of
its circuitry and wakes on a hardware timer.

### `defmt` — printing without a terminal

There's no screen and no serial console by default. `defmt` is a logging system
where **the text never goes on the device at all.**

When you write:

```rust
info!("found AMG8833 at 0x{=u8:02x}", address);
```

the compiler stores that format string in a table, and the device transmits
only a small ID plus the raw value — a handful of bytes rather than a string.
`probe-rs` on the PC side looks up the ID and prints the message.

Why bother: string formatting is slow and strings eat flash. `defmt` gets rich
logging at almost no cost on the device.

---

## 7. The C interop story

This is the most conceptually interesting part of the project, and the reason
the roadmap treats it as a headline skill.

### The situation

ST sells the VL53L5CX. To make it work you must:

- upload **86,017 bytes of firmware** into the sensor at every startup
- speak an undocumented register protocol
- apply calibration tables

ST provides all of this as the **ULD** (Ultra Lite Driver) — thousands of lines
of C, tested over years, shipping in real products.

**We could rewrite it in Rust.** It would take weeks, and the result would be a
fresh crop of bugs in a problem that is already solved.

**So we don't.** We compile ST's C and call it from Rust. This is *normal* — it
is what almost all embedded Rust does with vendor code.

### The project's rule for choosing

The roadmap states it as: **is the `unsafe` concentrated, or smeared?**

Rust's `unsafe` keyword marks code where the compiler can't verify safety —
raw pointers, calling foreign functions. Calling C is always `unsafe`, because
the compiler cannot see inside C.

- **Concentrated** — a small, auditable boundary with safe Rust on top. Good.
  You can read the whole risky part in one sitting.
- **Smeared** — `unsafe` scattered through the codebase because you're fighting
  a C-shaped API everywhere. Bad. At that point C is the honest choice.

The VL53L5CX is the concentrated case: ~200 lines of boundary under an entire
application of safe Rust. The AMG8833, next section, needs no `unsafe` at all.

### Three tools do the work

**`cc` — compiles the C**

In `build.rs`, which runs on your PC before the Rust compiles:

```rust
cc::Build::new()
    .include(&inc)
    .file(src.join("vl53l5cx_api.c"))
    .compile("vl53l5cx_uld");
```

This invokes `arm-none-eabi-gcc` — a C compiler that produces ARM code — and
produces a **static library**, `libvl53l5cx_uld.a`: a bag of compiled machine
code with named entry points.

**`bindgen` — reads the C headers**

C headers declare functions but in C syntax, which Rust can't read. `bindgen`
parses them and writes the equivalent Rust declarations:

```c
uint8_t vl53l5cx_init(VL53L5CX_Configuration *p_dev);
```

becomes

```rust
pub fn vl53l5cx_init(p_dev: *mut VL53L5CX_Configuration) -> u8;
```

Same function, described in Rust. It generated 510 lines for us, automatically,
so the descriptions can't drift out of sync with the C.

**The linker — ties it together**

Rust compiles our code to machine code with unresolved references to
`vl53l5cx_init`. The C library contains a symbol with that name. The linker
matches them and patches in the address.

At the machine level **there is no such thing as a C function or a Rust
function** — just addresses and an agreed convention for passing arguments in
registers. That's why this works at all.

### The platform layer: the clever part

Here's the twist. ST's driver needs to *talk to the sensor*, but ST has no idea
what hardware you're using. So the driver doesn't contain any I2C code. Instead
it declares seven functions it promises to call, and requires **you** to supply
them:

```c
uint8_t VL53L5CX_RdByte(VL53L5CX_Platform *p, uint16_t reg, uint8_t *value);
uint8_t VL53L5CX_WrByte(VL53L5CX_Platform *p, uint16_t reg, uint8_t value);
uint8_t VL53L5CX_RdMulti(...);
uint8_t VL53L5CX_WrMulti(...);
void    VL53L5CX_SwapBuffer(...);
uint8_t VL53L5CX_WaitMs(...);
uint8_t VL53L5CX_Reset_Sensor(...);
```

ST expects these written in C. **We wrote them in Rust.**

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn VL53L5CX_RdByte(
    p_platform: *mut VL53L5CX_Platform,
    register_address: u16,
    p_value: *mut u8,
) -> u8 {
    // ... use Embassy's I2C to do the read ...
}
```

Two attributes make it work:

- **`extern "C"`** — use C's convention for passing arguments, so ST's compiled
  code and ours agree on where things go.
- **`no_mangle`** — keep the name exactly as written. Rust normally rewrites
  function names to encode types (so two `new` functions can coexist); the
  linker must find the literal string `VL53L5CX_RdByte`.

So the call chain is:

```
  our Rust  --calls-->  ST's C driver  --calls back-->  our Rust  --> hardware
```

C calling into Rust. This is why no `platform.c` exists in the tree — the
entire hardware-facing half of a C driver is Rust.

### Following one real call

When `tof_probe` asks whether the sensor is alive:

1. **Rust** calls `vl53l5cx_is_alive(&mut dev)`.
2. **Linker** already pointed that at ST's compiled C.
3. **ST's C** decides it needs register `0x00` and calls `VL53L5CX_RdByte`.
4. **Linker** pointed *that* at our Rust function.
5. **Our Rust** pulls the I2C peripheral out of the context pointer, shifts
   `0x52` to `0x29`, and performs the transfer with Embassy.
6. The byte travels back up the same chain.

Four language boundary crossings, and the result is `is_alive OK`.

### How Rust finds its way back: the context pointer

Step 5 hides something. When ST's C calls our function, how does our Rust code
know *which* I2C peripheral to use?

ST's design allows for it. The struct passed to every callback is one you
define:

```c
typedef struct {
    uint16_t  address;
    void     *ctx;      /* we added this */
} VL53L5CX_Platform;
```

`void *` is C for "a pointer to something, and I'm not going to say what." We
store a pointer to our Rust `BusContext` there, and cast it back on the way in:

```rust
let ctx = unsafe { &mut *(platform.ctx as *mut BusContext) };
```

This is *why* it's `unsafe`: the compiler is taking our word that the pointer
really is a live `BusContext`. If it were dangling or the wrong type, this
would be a serious bug — and this is one of only a handful of places where such
a bug is even possible.

The alternative is a global variable. The context pointer is better: it means
two sensors on one bus stays possible without redesigning anything.

---

## 8. The AMG8833: the same job in pure Rust

The thermal sensor needed a driver too, and here the calculation came out the
other way.

There is no firmware to upload, no calibration tables, no vendor library worth
having. There are about twenty registers and a 128-byte read. So `amg8833.rs`
is **plain safe Rust with no `unsafe` at all**:

```rust
pub fn read_frame(&mut self, bus: &mut Bus, out: &mut [i16; PIXELS]) -> Result<(), Error> {
    let mut raw = [0u8; PIXELS * 2];
    bus.blocking_write_read(self.address, &[REG_PIXEL_BASE], &mut raw)?;

    for (pixel, bytes) in out.iter_mut().zip(raw.chunks_exact(2)) {
        let word = u16::from_le_bytes([bytes[0], bytes[1]]);
        *pixel = quarter_degrees_to_centi(twelve_bit_twos_complement(word));
    }
    Ok(())
}
```

"Ask for 128 bytes starting at register `0x80`, then turn each pair into a
temperature."

**The two drivers side by side are the whole argument**, and having both in one
project with the reasoning written down is worth more than an opinion about
which language is better:

| | VL53L5CX | AMG8833 |
|--|----------|---------|
| Vendor code | thousands of lines of C | none needed |
| Our approach | wrap it | write it |
| `unsafe` | ~200 lines, one file | zero |
| Why | rewriting is weeks of new bugs | it's twenty registers |

### Number formats — where sensor bugs hide

Sensors pack numbers in ways chosen for silicon convenience, not yours. The
AMG8833 uses **two different schemes in the same chip**:

**Pixels: 12-bit two's complement.** The normal way computers store signed
numbers. The top bit means negative, and to widen it to 16 bits you copy that
bit leftwards:

```rust
fn twelve_bit_twos_complement(word: u16) -> i16 {
    let value = word & 0x0fff;
    if value & 0x0800 != 0 {
        (value | 0xf000) as i16   // negative: fill the top with 1s
    } else {
        value as i16
    }
}
```

**Thermistor: sign-and-magnitude.** A completely different scheme where one bit
is a "minus sign" flag and the rest is the size. `-5` is stored as "5, and
negative" rather than as a two's complement pattern.

Applying the wrong one gives plausible-looking wrong answers, which is the
worst kind.

### Why temperatures are integers

Everything is **centidegrees**: `2537` means 25.37 °C. The chip has hardware
floating point, so this isn't about speed.

It's about there being exactly one representation. These numbers get logged,
written to CSV, parsed by Python, fed to a model, and sent over a network. A
float can be `25.37`, `25.370000000000001`, or `2.537e1` depending on who
formats it. Integers can't drift like that, and "the CSV doesn't match the
device" is a real and miserable class of bug.

---

## 9. The diagnostic tools

Five of the eight programs in `src/bin/` exist only to answer questions during
bring-up. They earned their place — most of them were written *because* we were
stuck.

| Tool | Question it answers |
|------|--------------------|
| `i2c_scan` | Which devices are on the bus? |
| `bus_check` | Are the wires electrically healthy, or shorted? |
| `pin_wiggle` | Which physical socket is this MCU pin? |
| `find_pwren` | Which pin turns the sensor on? |
| `tof_probe` | Does ST's C driver actually work? |

Two ideas in them are worth stealing for future work.

**Always include a control.** `i2c_scan` sweeps *two* buses, not one. The second
bus has six sensors soldered to the board, so we know exactly what should
appear. If those six show up, the scanner is proven — and only then does "no
devices found" on the bus we care about mean an empty bus rather than a broken
scanner.

**Make failure messages say which layer failed.** `tof_probe` runs two tests:
a couple of register reads, then the 86 KB firmware upload. When the first
passed and the second failed, that immediately localised the bug to bulk
transfers — no guessing.

### The host-side viewer

`tools/heatmap_viewer.py` runs on the PC, not the device. It reads the
firmware's log output and draws it as a picture that updates live:

```bash
cd firmware
cargo run --release --bin tof_range | python3 ../tools/heatmap_viewer.py
```

Pipe `fused_stream` in instead and it draws both sensors side by side, from
readings taken together — which is what the model will actually see.

**Why it exists.** The project's biggest risk is a badly aimed sensor rig
where every recorded frame carries the same invisible defect — not discovered
until a model underperforms for reasons nobody can diagnose. Watching a live
heatmap *while positioning the rig* is the cheapest possible defence, and it
only helps if it exists before the rig is built.

It also records to CSV, which is Phase 2's data collection in its simplest
form:

```bash
cargo run --release --bin fused_stream \
  | python3 ../tools/heatmap_viewer.py --record session.csv --label empty
```

Each row is one moment seen by both sensors: 64 distances, 64 ToF status
codes and 64 temperatures. The distances are kept raw, with the status codes
beside them, rather than replacing "no reading" with a number. What "no
reading" should mean to a model is a real decision — in this rig it is
exactly what an empty black chair looks like — and recording it raw means
that decision can be made later without collecting the data again.

One label per session rather than annotating afterwards: far less error-prone,
and a session labelled wrongly can be thrown away whole instead of picked
apart.

**Two things in it worth understanding.**

*It parses text rather than a binary protocol.* The firmware already prints
readable grids for anyone watching the terminal, so consuming those adds no
firmware code and keeps one source of truth. That's the right trade now and
the wrong one later — a binary frame format would be faster and unambiguous.
The combined stream kept the text format: at 5 Hz it costs nothing, and
being able to read a capture by eye has already settled several arguments.

*Colour maps are perceptually uniform, never rainbow.* `jet` and `turbo` are
the reflex choice for sensor data and they actively mislead. Their lightness
isn't monotonic, so they manufacture bright and dark bands where the data is
perfectly smooth, and the eye reads those bands as edges that aren't in the
scene. `viridis` and `inferno` climb steadily from dark to light, so an
apparent boundary is a real one. They also survive colour-blindness and
greyscale printing, which matters when these plots reach the write-up.

Zones with no valid reading are drawn in a neutral grey that appears nowhere
on the ramp — "nothing within range this way" is a real observation, and
painting it as a distance would put a false hard edge in the picture.

**And one bug worth remembering.** The obvious way to rescale a matplotlib
image is `set_clim(vmin, vmax)`. With a colourbar attached, matplotlib 3.6
mangles it — asking for `vmin=815` produced `0.9`, and the plot renders with a
silently wrong scale rather than raising an error. Installing a fresh
`Normalize` via `set_norm` works correctly. This is exactly the class of bug
that survives review: no crash, no warning, just a picture that means
something other than what it appears to.

---

## 10. Building the dataset (Gate 2)

A model can only learn what the data shows it, and it will happily learn the
wrong thing if the data makes that easy. Most of Gate 2 was about making the
wrong things hard to learn.

### Three classes, set by where the sensor points

The board looks down at the chair at 49.9°. From there a standing person's
head is out of view, so "standing" and "walking past" look the same. That
gives three classes, not four:

| Class | Meaning |
|-------|---------|
| `empty` | Nobody in view |
| `seated` | One person in the chair |
| `present` | In view but not seated — standing, reaching, walking past |

### Sessions, rounds and labels

A **session** is one recording of 2.5–3.5 minutes at 5 frames a second, all
one situation: say, "chair pushed in, nobody there". A **round** is twelve
sessions recorded back to back in one daypart — five empty chair positions,
three sitting postures, three ways of being present, and a closing empty.
Six rounds over three days (`am`, `pm`, `eve`, each on two days) make the 70
sessions.

Usually the whole session has one label. Walking past is the exception — a
person is in view only part of the time — so those sessions were filmed with
a webcam and labelled frame by frame with `tools/label_frames.py`. The video
never leaves the PC; only the labels are committed.

### The rules each came from a measurement

- **Wait 10 minutes before recording `empty` after anyone sat down.** A seat
  stays warm for about 7 minutes. Without the wait, the model learns "warm
  seat = nobody here".
- **Move the chair on purpose.** Five chair states, so the model learns a
  chair's position is not a person.
- **Check the mount angle every frame.** The board's accelerometer logs tilt;
  if the rig gets knocked, the data changes meaning.
- **Watch what lines up with the label.** If every `empty` session happened in
  daylight and every `seated` one after dark, "dark" would predict "seated".
  The evening rounds were arranged so `empty` covers twilight *and* darkness.

### What the model actually sees: the encoding

Each frame becomes three 8×8 grids (`tools/encode.py`):

1. **ToF valid mask** — 1 where the distance reading is trustworthy (status 5
   or 9, and not 0 mm), else 0.
2. **ToF distance** — the valid distance, clipped to 2 m and scaled to 0–1.
   Where the reading is invalid, it's filled with 1.0 ("far").
3. **Thermal excess** — each pixel's temperature minus the frame's median, so
   a warm room and a cold room look alike and only *differences* matter.

Why a separate mask: "nothing came back" is real information here — the black
leather chair back returns nothing at all — but it must never look like "very
close". And one specific trap: about 1,300 readings claimed to be valid with
a distance of exactly 0 mm, almost all in one corner zone and almost all in
`empty` sessions. Fed in raw, that's a shortcut: "corner reads zero, so the
room is empty". The mask removes it.

The **Gate 2 exit test** was looking at the average grid per class
(`tools/class_look.py`): if a person can't see the difference, a model won't
find a real one.

---

## 11. From data to a model on the chip (Gate 3)

### What a model adds over a rule

Gate 2 already had a hand-written rule: "someone is here if ≥3 ToF zones are
nearer than 620 mm, or ≥2 thermal zones are 1.5 °C warmer than the room". It
gets empty-vs-occupied right 99.4% of the time. But it can't say *seated* or
*present* — leaning back puts your body in the top rows like standing does,
and no simple threshold untangles that. A model learns those combinations
from examples instead of being told them.

### Training and testing — and the trap

**Training** shows the model frames with their labels and adjusts its
internal numbers (the **weights**) so it gets more of them right. **Testing**
asks it about frames it has never seen. Only the test score means anything:
any model can memorise its training data.

The trap: frames 0.2 s apart are nearly identical. If you shuffle all 57,516
frames and hold out a random 20%, almost every test frame has a near-twin in
training. The model looks brilliant and has learned nothing about new
situations.

So the test unit here is a **round** — a whole daypart on a whole day:

```
round held out      trained on the other five
2026-11-22 pm   →   22 eve, 23 am, 23 pm, 23 eve, 24 am
2026-11-22 eve  →   22 pm,  23 am, 23 pm, 23 eve, 24 am
   …six times, one per round…
```

This is **leave-one-round-out cross-validation** (`ml/dataset.py`). Every
score is on a day and lighting the model never saw. The six test results are
added together into one confusion matrix.

### Baselines first

Before building anything clever, score something simple
(`ml/baseline.py`). If a simple method already wins, the clever one isn't
worth its cost.

- The Gate 2 rule: 99.4%, but empty-vs-occupied only.
- **Logistic regression** — the simplest real classifier: one weighted sum of
  all 192 inputs per class. **98.2%.** Its weakness: 500 `present` frames
  called `seated`.

### The network, layer by layer

A small **convolutional neural network** (CNN) — `ml/train.py`. A
convolution slides a 3×3 window over the grid and learns patterns like "warm
blob next to near returns", wherever they appear.

```
input              3 × 8 × 8      mask, distance, thermal
conv 3×3, 16       16 × 8 × 8     16 small pattern detectors      448 weights
ReLU                              keep positives, zero negatives
conv 3×3, 16       16 × 8 × 8     patterns of patterns          2,320 weights
ReLU
max-pool 2×2       16 × 4 × 4     keep the strongest per 2×2 block
flatten            256
dense 32           32             combine everything            8,224 weights
ReLU
dense 3            3              one score per class              99 weights
                                                        total  11,091
```

(Each conv's count is inputs × outputs × 9 + one bias per output: 3×16×9+16
= 448. Each dense layer's is inputs × outputs + outputs: 256×32+32 = 8,224.)

The three output scores are **logits**; the biggest one is the answer.

One deliberate choice: many image models end with "global pooling", which
throws away *where* things are. Here *where* is the whole point — warmth in
the bottom rows means seated, in the top rows means standing — so the layout
is kept all the way to the dense layers. For the same reason the training
images are never flipped or shifted.

### How it trains

- **Loss** — a number saying how wrong the model is on a batch of frames
  (cross-entropy). Training nudges every weight to make it smaller
  (**Adam** is the nudging method, the **learning rate** how big each nudge is).
- **Epoch** — one pass through all the training frames. 15 here.
- **Class weights** — there are twice as many `empty` frames as `present`, so
  mistakes on `present` count double. Otherwise the model can score well by
  favouring `empty`.
- **Seed** — the random starting values. Different seeds give slightly
  different models; checking three shows the result isn't luck.
- **Fixed in advance.** None of these were tuned by peeking at the test round
  — tuning on the test set is just a slower way of cheating.

Each fold trains in about 7 seconds on the CPU. A model this small doesn't
need the GPU.

### Reading a confusion matrix

Rows are the truth, columns the model's answer. The diagonal is correct.

```
              empty    seated   present    recall
empty         27131         0       250     99.1%
seated            1     16701        75     99.5%
present         123        80     13155     98.5%
precision     99.5%     99.5%     97.6%
```

- **Recall** (per row): of all real `present` frames, how many were caught.
- **Precision** (per column): of everything called `present`, how much really
  was.
- **Accuracy**: all correct ÷ all frames — 99.1%.
- **Balanced accuracy**: the average of the three recalls, so a big class
  can't hide a weak small one — 99.0%.

The matrix also says *where* to look. Here: zero empty-room frames called
occupied across the 40 empty sessions, and nearly all the errors at the
edges of walking passes, where the label itself is a judgement call.

### From PyTorch to C on the STM32

The model is trained in **PyTorch** on the PC. The chip can't run Python, so:

```
PyTorch model ──export──▶ ONNX file ──ST Edge AI──▶ C code + weight array
  (training)              (layers +                   (runs on the U585)
                           11,091 weights)
```

1. **ONNX** is a standard file format for trained networks: the list of
   layers and the learned weights, nothing else. `train.py --final` writes
   `ml/results/occupancy_cnn.onnx`.
2. **ST Edge AI Core** (`stedgeai`, installed on this PC at
   `/opt/ST/STEdgeAI/4.0/`) reads the ONNX file and generates C: the weights
   as a constant array in flash, and the network as calls into ST's
   pre-optimised Cortex-M33 library (its own convolution, ReLU, pooling and
   dense routines). No PyTorch code goes to the chip — the same maths, the
   same weights, computed by ST's code.

**STM32Cube AI Studio** is the graphical front end to the same engine. We used
the command line instead: it's scriptable and repeatable (`ml/st_check.sh`).

#### What `analyze` says

| | Value | Meaning |
|---|---:|---|
| Weights | 43.3 KiB (flash) | the 11,091 numbers × 4 bytes each |
| Runtime | ~6 KiB (flash) | ST's library code |
| Activations | 5.75 KiB (RAM) | scratch space for the layer outputs during one inference |
| MACC | 186,787 | multiply-accumulates per inference — the work done |

Out of 2 MiB flash and 786 KiB RAM. At 160 MHz that's a few milliseconds per
inference, and it runs once every 1–2 seconds.

**Quantisation** — storing weights as 8-bit integers instead of 32-bit floats
— would cut the 43 KiB to about 11 KiB and speed it up, at some risk to
accuracy. Not needed here, so not done.

#### What `validate` proves

`validate` compiles the generated C **on the PC**, feeds it real frames, and
compares its outputs with the original model's. On all 57,516 frames: **same
class every time**, output differences around 0.0000013 — rounding from
doing the same additions in a different order. So the 99.1% belongs to the C
model too. (ST can also run this check on the board itself; that, and
measuring the real speed, is Gate 4.)

### The trap still ahead

The C model is only as good as what it's fed. On the chip, Rust code has to
turn each raw frame into exactly the same three grids `tools/encode.py`
makes — same validity rule, same 2 m clip, same "far" fill, same median. A
small mismatch won't crash anything; it will just quietly make the model
worse. Gate 4's safeguard: log raw frames and the firmware's encoding of them
side by side, and check they match the Python encoding value for value.

### Doing it again

```bash
python3 -m venv ml/.venv
ml/.venv/bin/pip install torch scikit-learn onnx onnxruntime \
    --extra-index-url https://download.pytorch.org/whl/cpu

python3 ml/dataset.py                        # what's in each round
ml/.venv/bin/python ml/baseline.py           # the rule + logistic regression
ml/.venv/bin/python ml/train.py --seeds 3    # CNN, leave-one-round-out
ml/.venv/bin/python ml/train.py --final      # train on everything, export ONNX
ml/st_check.sh                               # analyze + validate, 300 frames
ml/st_check.sh --all                         # validate on every frame
```

Results land in `ml/results/`. The numbers and the reasoning behind every
choice are in `NOTES.md`, "Phase 3 — Model".

---

## 12. Problems we hit, and what fixed them

The genuinely instructive part. These are real, in order, with the reasoning
that eventually worked.

### 1. The header numbering doesn't match another board

**Cost: most of a day.** The worst one.

The sensor wouldn't respond. `bus_check` said the data line was shorted to
ground so hard nothing could lift it. We swapped sensors, rewired twice with
fresh wires, tested continuity, and concluded the sensor chip was damaged.

**It was none of that.** The Arduino header on this board (`CN13`) has its pins
in a different order than the NUCLEO-F446RE boards used on previous projects.
Wiring by *counting positions* — muscle memory from those builds — put the data
wire in a real ground pin. It genuinely *was* shorted to ground: we did it
ourselves, identically, every time.

**The fix:** read the label printed next to each hole. Not a diagram, not an
application note for a similar board, not a photograph.

**The lesson:** "SDA is on pin D14" and "which physical hole is D14" are
different facts. Documentation gives the first. Only the board gives the
second.

### 2. A pull-up tells you a pull-up exists

While chasing the above, `bus_check` reported the clock line "healthy — sensor
powered and grounded". Tony disconnected that wire entirely and it *still* said
healthy.

The board has its own pull-up resistors on those lines. The test was detecting
those, not the sensor. Every conclusion built on it was worthless.

**The lesson:** know what your test actually measures. It could prove a line
was *shorted*; it could never prove a line was *connected*.

### 3. The regulator was switched off

The sensor's breakout board has its own small voltage regulator, and a pin
called `PWR_EN` that turns it on. It defaults to **off**.

Result: correct voltages measured at the input pins, a clean bus, and total
silence. An unpowered sensor is indistinguishable from an absent one.

**The fix:** drive `PWR_EN` high in firmware before talking to the sensor.

### 4. An unpowered chip is not a neutral chip

The best one, and it explained several earlier mysteries.

`thermal_probe` couldn't find the AMG8833 at all, while `i2c_scan` — running
minutes earlier on the same wiring — found it fine. The only difference: the
scanner asserted `PWR_EN`, the thermal tool didn't (it doesn't use the ToF, so
why would it?).

With its regulator off, the unpowered VL53L5CX **clamped the shared bus through
its own input protection diodes**, breaking communication with a completely
different device.

**The lesson:** on a shared bus, a device with no power isn't sitting quietly —
it's a load. Every program that uses this bus now powers *everything* on it.

### 5. The 32 KB write that wasn't

`vl53l5cx_is_alive` passed; `vl53l5cx_init` failed. Because those are separate
tests, that immediately meant single-byte access worked and bulk transfers
didn't.

Firmware upload writes 32 KB in one call. Our first version tried to issue that
as one giant I2C transaction, relying on the driver library to split it
internally. It didn't work.

**The fix:** split it ourselves — 256 bytes at a time, advancing the register
address each chunk. The sensor auto-increments internally, so a run of small
writes lands identically to one huge one. This is what every other port of this
driver does; we should have looked first.

### 6. The protective film

First ranging test: every zone read 0–8 mm. The sensor was working perfectly and
faithfully reporting the distance to the protective sticker still on its lens.

**The lesson:** when data looks absurd, check the physical world before the
code.

### 7. Wrong reference for "warm"

The thermal display showed every pixel as "colder than ambient" and looked
broken. It wasn't: we were comparing against the sensor's own chip temperature,
and the chip self-heats a couple of degrees above the room it's watching. So
everything really was colder than it.

**The fix:** compare against the **median of the frame itself**. Half of any
realistic 8×8 view is background, so the median *is* the room — no calibration
needed, and it works the same in a cold room or a warm one.

### 8. The compiler catching a real bug

While checking the temperature conversions, `rustc` rejected a test constant:
`2047 * 25 = 51175`, which doesn't fit in a 16-bit integer.

That wasn't just a bad test — the driver had the same multiplication. In release
builds it would silently wrap around, turning a garbled reading into a
plausible-looking **negative temperature**.

**The fix:** do the arithmetic in 32 bits and saturate. Garbage now looks like
garbage instead of masquerading as data.

### 9. The bus that wouldn't let go

**Cost: three failed starts and one empty 13-minute recording.**

Every so often a recording wouldn't start: the firmware looked for the
thermal sensor, found nothing, and stopped. A rescan a minute later always
found it. The first guess was a loose wire — but everything is soldered on
the shield.

**The cause was how each recording ends.** A timer kills the PC-side
program, which halts the chip mid-whatever — sometimes halfway through
reading a byte from a sensor. On I2C the sensor drives the data line (SDA)
while it sends; halted mid-byte, it keeps holding SDA low, waiting for clock
pulses that never come. The next start sees a busy bus and gives up.

**The fix** (`firmware/src/i2c_recovery.rs`): before the I2C hardware takes
the pins, toggle the clock line by hand nine times so the sensor finishes its
byte and lets go, then send a STOP. Retry the probe up to three times. It
logs only when it acts — and the next morning it caught a stuck bus in the
act, confirming the theory.

**The lesson, twice over:** the first version stopped clocking as soon as SDA
read high. But a sensor mid-byte can put a 1 bit on the line and then pull it
low again for the next bit. "The line went high" is not the same as "the
sensor let go" — always send all nine pulses.

---

## 13. Glossary

**Activations** — the intermediate outputs of each layer during one
inference; they need RAM while the model runs.

**Baseline** — a deliberately simple method scored first, so a complex one has
to beat something.

**Bus clear** — toggling the I2C clock by hand to make a stuck device release
the data line.

**Confusion matrix** — table of true class (rows) against predicted class
(columns).

**Convolution (conv)** — a small window of learned weights slid across a grid,
detecting the same pattern anywhere in it.

**Cross-validation** — training and testing several times, holding out a
different part each time.

**Epoch** — one full pass of training over all the training data.

**Logits** — a model's raw per-class scores; the largest is the prediction.

**MACC** — multiply-accumulate operations; the standard measure of how much
work one inference takes.

**ONNX** — a standard file format for trained neural networks: layers plus
weights.

**Precision / recall** — of everything called class X, how much really was
(precision); of everything really X, how much was caught (recall).

**Quantisation** — storing a model's numbers as small integers (e.g. 8-bit)
instead of 32-bit floats, to save memory and time.

**ST Edge AI Core (`stedgeai`)** — ST's tool that turns a trained model into
optimised C for STM32 chips; STM32Cube AI Studio is its graphical front end.

**Weights** — the numbers a model learns during training.

**Bare metal** — no operating system. Your program is the only thing running.

**bindgen** — tool that reads C headers and generates Rust declarations.

**cc (crate)** — build-time tool that compiles C source into a library your
Rust links against.

**Cortex-M33** — the ARM processor core inside the STM32U585.

**defmt** — logging where the device sends IDs, not text, and the PC
reconstructs the message.

**Embassy** — async framework and hardware drivers for embedded Rust.

**FFI** (Foreign Function Interface) — mechanism for one language to call
another.

**Flash** — non-volatile memory holding the program. Survives power-off.

**I2C** — two-wire bus for connecting chips over short distances.

**Linker** — final build step that resolves names to addresses and produces one
binary.

**`no_mangle`** — keep this function's name exactly as written, so the linker
can find it.

**`no_std`** — build without the standard library, because there's no OS to
provide it.

**probe-rs** — tool that flashes the chip and streams logs over the debug
connection.

**Pull-up resistor** — resistor holding a signal line high when nothing pulls
it down. Required by I2C.

**RAM** — working memory. Lost on power-off.

**Register** — a numbered slot inside a chip. Writing configures, reading
retrieves data.

**RTT** (Real-Time Transfer) — SEGGER's protocol for shipping data over the
debug connection while the program runs.

**Sensor fusion** — combining several sensors so each covers the others' blind
spots.

**Thermopile** — sensor that measures heat radiation.

**Time-of-flight** — measuring distance by timing a light pulse's round trip.

**Two's complement** — standard binary encoding for signed integers.

**ULD** (Ultra Lite Driver) — ST's C driver for the VL53L5CX.

**`unsafe`** — Rust keyword marking code the compiler cannot verify. Not
"broken"; "you are responsible for this."

---

*Living document. When something here turns out to be wrong, or the code moves
on, edit it — the wrong turns are as much a part of the record as the fixes.*
