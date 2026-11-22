//! Both sensors, one stream: the dataset format for Phase 2.
//!
//! `tof_range` and `thermal_probe` each own the bus and print their own grid.
//! That is fine for aiming, but no two of their frames are ever from the same
//! moment, which makes them useless as training data for a model that is
//! meant to see a scene through both at once. This binary reads both on one
//! bus, pairs them, and prints them under a single frame number.
//!
//! # Pairing
//!
//! The VL53L5CX sets the pace, because it is the one with a data-ready flag.
//! When it has a frame, the AMG8833 is read **first** (~12 ms at 100 kHz),
//! then the ToF results (~100 ms — they are large). Reading the thermal frame
//! after the long ToF transfer would add that transfer to the skew for
//! nothing. The AMG updates internally at 10 Hz, so its frame is at most
//! ~100 ms older than the ToF's: well inside the time it takes a person to
//! sit down.
//!
//! 5 Hz rather than the 10 Hz `tof_range` uses: the ToF transfer alone eats
//! most of a 100 ms period at this bus speed, and inference only needs a
//! decision every 1–2 s.
//!
//! # Output
//!
//! Every frame, three blocks sharing one frame number:
//!
//! ```text
//! --- frame 12 --- tof t 4821 ms, 44/64 valid, 511..1864 mm, silicon 47 C
//!   <8 rows of raw distance, mm>   |map|
//! --- frame 12 --- status
//!   <8 rows of ULD target_status>  |map|
//! --- frame 12 --- thermal median 2525, range 2425..3000 c°C, die 2993
//!   <8 rows of temperature, c°C>   |map|
//! ```
//!
//! Distances are printed **raw**, for every zone, with the ULD's status
//! alongside — not collapsed to -1 the way `tof_range` does for display.
//! What an invalid zone should mean as model input is an open decision
//! (TODO.md), and in the current pose "no return" *is* the empty-chair
//! signature. Recording status and distance separately keeps every option
//! open; collapsing them here would make the decision before anyone has made
//! it, and the data would have to be collected again to undo it.
//!
//! Both grids are already in the world frame (see `orientation.rs`).
//!
//! `t` is the device's own clock in milliseconds since boot, so frame spacing
//! can be checked without trusting the host's pipe timing.
//!
//! Run with:
//! `cargo run --release --bin fused_stream | python3 ../tools/heatmap_viewer.py`

#![no_std]
#![no_main]

use defmt::{error, info, warn};
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::i2c::{I2c, Master};
use embassy_stm32::mode::Blocking;
use embassy_stm32::time::Hertz;
use embassy_time::{Duration, Instant, Timer};

use defmt_rtt as _;
use panic_probe as _;

use coworkos_firmware::amg8833::{ALT_ADDRESS, Amg8833, DEFAULT_ADDRESS, FrameRate, PIXELS};
use coworkos_firmware::clock;
use coworkos_firmware::vl53l5cx::bindings::{
    VL53L5CX_Configuration, VL53L5CX_ResultsData, vl53l5cx_check_data_ready, vl53l5cx_get_ranging_data,
    vl53l5cx_init, vl53l5cx_is_alive, vl53l5cx_set_ranging_frequency_hz, vl53l5cx_set_resolution,
    vl53l5cx_start_ranging,
};
use coworkos_firmware::vl53l5cx::platform::BusContext;
use coworkos_firmware::vl53l5cx::{self, ORIENTATION as TOF_ORIENTATION};

type Bus = I2c<'static, Blocking, Master>;

const TOF_ADDRESS_8BIT: u16 = 0x52;
const RESOLUTION_8X8: u8 = 64;
const RANGING_HZ: u8 = 5;

/// Thermal map band width, centidegrees above the frame median. Same as
/// `thermal_probe`, so the two read alike.
const WARM_STEP: i16 = 50;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());

    info!("fused stream — VL53L5CX + AMG8833, one bus, paired frames");

    // PWR_EN high before anything touches the bus: an unpowered SATEL clamps
    // SDA/SCL and takes the AMG8833 down with it (see thermal_probe).
    let _power = Output::new(p.PB0, Level::High, Speed::Low);
    Timer::after(Duration::from_millis(100)).await;

    let mut cfg = embassy_stm32::i2c::Config::default();
    cfg.frequency = Hertz::khz(100);
    // Long enough for the ULD's firmware download, which is one big write.
    cfg.timeout = Duration::from_millis(1000);
    let mut bus = BusContext::new(I2c::new_blocking(p.I2C1, p.PB8, p.PB9, cfg));

    // One bus, two drivers. The ULD keeps a raw pointer to the BusContext and
    // reaches the I2C through it in its callbacks; the AMG driver wants
    // `&mut I2c`. Both are derived from this one pointer, and every use is a
    // blocking call that returns before the next begins, so the two never
    // hold the bus at once. Taking `&mut bus` again after this point would
    // invalidate the pointer the ULD holds — don't.
    let ctx: *mut BusContext = &mut bus;
    let amg_bus = || -> &mut Bus { unsafe { (*ctx).i2c() } };

    // ---- AMG8833 ----
    let amg_address = if probe(amg_bus(), DEFAULT_ADDRESS) {
        DEFAULT_ADDRESS
    } else if probe(amg_bus(), ALT_ADDRESS) {
        ALT_ADDRESS
    } else {
        error!("no AMG8833 at 0x{=u8:02x} or 0x{=u8:02x}", DEFAULT_ADDRESS, ALT_ADDRESS);
        stall().await;
    };
    let mut amg = Amg8833::new(amg_address);
    if let Err(e) = amg.init(amg_bus(), FrameRate::Fps10) {
        error!("AMG8833 init failed: {}", e);
        stall().await;
    }
    info!("AMG8833 at 0x{=u8:02x}", amg_address);

    // ---- VL53L5CX ----
    let mut dev: VL53L5CX_Configuration = unsafe { core::mem::zeroed() };
    dev.platform.address = TOF_ADDRESS_8BIT;
    dev.platform.ctx = ctx as *mut core::ffi::c_void;

    let mut alive: u8 = 0;
    if unsafe { vl53l5cx_is_alive(&mut dev, &mut alive) } != 0 || alive == 0 {
        error!("VL53L5CX not responding — check wiring and PWR_EN");
        stall().await;
    }
    info!("downloading VL53L5CX firmware...");
    if unsafe { vl53l5cx_init(&mut dev) } != 0 {
        error!("VL53L5CX init failed");
        stall().await;
    }
    // Resolution before frequency: the ULD checks the rate against the
    // current resolution's limit.
    if unsafe { vl53l5cx_set_resolution(&mut dev, RESOLUTION_8X8) } != 0
        || unsafe { vl53l5cx_set_ranging_frequency_hz(&mut dev, RANGING_HZ) } != 0
    {
        error!("VL53L5CX configuration failed");
        stall().await;
    }
    if unsafe { vl53l5cx_start_ranging(&mut dev) } != 0 {
        error!("VL53L5CX start_ranging failed");
        stall().await;
    }

    info!("streaming paired frames at {=u8} Hz", RANGING_HZ);
    info!("");

    let mut results: VL53L5CX_ResultsData = unsafe { core::mem::zeroed() };
    let mut thermal = [0i16; PIXELS];
    let mut frame: u32 = 0;

    loop {
        let mut ready: u8 = 0;
        if unsafe { vl53l5cx_check_data_ready(&mut dev, &mut ready) } != 0 {
            error!("check_data_ready failed");
            stall().await;
        }
        if ready == 0 {
            Timer::after(Duration::from_millis(5)).await;
            continue;
        }

        let t_ms = Instant::now().as_millis();

        // Thermal first — the short read — so the pair is as close in time
        // as this bus allows. A failed thermal read skips the pair rather
        // than stalling: one lost frame is better than a dead session, and a
        // ToF frame without its partner is not a paired observation.
        let die = amg.read_thermistor(amg_bus());
        let pixels = amg.read_frame(amg_bus(), &mut thermal);
        if unsafe { vl53l5cx_get_ranging_data(&mut dev, &mut results) } != 0 {
            error!("get_ranging_data failed");
            stall().await;
        }
        let die = match (die, pixels) {
            (Ok(d), Ok(())) => d,
            (Err(e), _) | (_, Err(e)) => {
                warn!("thermal read failed, pair skipped: {}", e);
                continue;
            }
        };

        frame = frame.wrapping_add(1);

        // Into the world frame — distance and status together, or a zone's
        // distance gets judged by its neighbour's status.
        let distance = TOF_ORIENTATION.apply(&results.distance_mm);
        let status = TOF_ORIENTATION.apply(&results.target_status);

        print_tof(frame, t_ms, &distance, &status, results.silicon_temp_degc);
        print_status(frame, &status);
        print_thermal(frame, &thermal, die);
    }
}

fn print_tof(frame: u32, t_ms: u64, distance: &[i16; 64], status: &[u8; 64], silicon: i8) {
    let mut min = i16::MAX;
    let mut max = i16::MIN;
    let mut valid = 0u32;
    for zone in 0..64 {
        if vl53l5cx::is_valid(status[zone]) {
            min = min.min(distance[zone]);
            max = max.max(distance[zone]);
            valid += 1;
        }
    }
    if valid == 0 {
        min = 0;
        max = 0;
    }
    info!(
        "--- frame {=u32} --- tof t {=u64} ms, {=u32}/64 valid, {=i16}..{=i16} mm, silicon {=i8} C",
        frame, t_ms, valid, min, max, silicon
    );
    for row in 0..8 {
        let mut art = [b' '; 8];
        let base = row * 8;
        for col in 0..8 {
            art[col] = if vl53l5cx::is_valid(status[base + col]) {
                tof_shade(distance[base + col])
            } else {
                b'.'
            };
        }
        let d = &distance[base..base + 8];
        info!(
            "  {=i16} {=i16} {=i16} {=i16} {=i16} {=i16} {=i16} {=i16}   |{=str}|",
            d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7],
            core::str::from_utf8(&art).unwrap_or("????????")
        );
    }
}

fn print_status(frame: u32, status: &[u8; 64]) {
    info!("--- frame {=u32} --- status", frame);
    for row in 0..8 {
        let mut art = [b' '; 8];
        let base = row * 8;
        for col in 0..8 {
            art[col] = match status[base + col] {
                vl53l5cx::STATUS_VALID => b'v',
                vl53l5cx::STATUS_VALID_WEAK => b'w',
                _ => b'.',
            };
        }
        let s = &status[base..base + 8];
        info!(
            "  {=u8} {=u8} {=u8} {=u8} {=u8} {=u8} {=u8} {=u8}   |{=str}|",
            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
            core::str::from_utf8(&art).unwrap_or("????????")
        );
    }
}

fn print_thermal(frame: u32, pixels: &[i16; PIXELS], die: i16) {
    let mut sorted = *pixels;
    sorted.sort_unstable();
    let median = sorted[PIXELS / 2];
    let (min, max) = (sorted[0], sorted[PIXELS - 1]);
    info!(
        "--- frame {=u32} --- thermal median {=i16}, range {=i16}..{=i16} c°C, die {=i16}",
        frame, median, min, max, die
    );
    for row in 0..8 {
        let mut art = [b' '; 8];
        let base = row * 8;
        for col in 0..8 {
            art[col] = thermal_shade(pixels[base + col] - median);
        }
        let v = &pixels[base..base + 8];
        info!(
            "  {=i16} {=i16} {=i16} {=i16} {=i16} {=i16} {=i16} {=i16}   |{=str}|",
            v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7],
            core::str::from_utf8(&art).unwrap_or("????????")
        );
    }
}

/// Same bands as `tof_range`.
fn tof_shade(distance_mm: i16) -> u8 {
    match distance_mm {
        d if d < 100 => b'#',
        100..=299 => b'%',
        300..=599 => b'*',
        600..=1199 => b'+',
        1200..=2399 => b'-',
        _ => b' ',
    }
}

/// Same bands as `thermal_probe`, relative to the frame median.
fn thermal_shade(above_median: i16) -> u8 {
    match above_median {
        d if d < -WARM_STEP => b'-',
        d if d < WARM_STEP => b'.',
        d if d < WARM_STEP * 2 => b'+',
        d if d < WARM_STEP * 4 => b'*',
        d if d < WARM_STEP * 8 => b'%',
        _ => b'#',
    }
}

fn probe(bus: &mut Bus, address: u8) -> bool {
    let mut byte = [0u8; 1];
    bus.blocking_read(address, &mut byte).is_ok()
}

async fn stall() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
