//! Live 8×8 ranging — the Phase 1 checkpoint.
//!
//! Brings the sensor all the way up and streams frames over RTT as a grid you
//! can read at a glance. Wave a hand in front of it and the covered zones
//! should drop to a few hundred millimetres while the rest hold at whatever
//! the far wall is.
//!
//! That is the whole test. The project plan asks for exactly this before any
//! data collection: *"wave your hand in front of the sensor and confirm the
//! printed grid changes sensibly"*. If the numbers move the way the room
//! does, Phase 1's sensing half is finished.
//!
//! # Reading the output
//!
//! Distances in millimetres, one row per sensor row. A zone with no valid
//! target prints `----` rather than a number — the sensor reports a status
//! per zone and only status 5 (and 9, with lower confidence) means "this
//! range is trustworthy". Printing invalid ranges as though they were real
//! is how a bad frame gets mistaken for a bad model later.
//!
//! Run with: `cargo run --release --bin tof_range`

#![no_std]
#![no_main]

use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::i2c::I2c;
use embassy_stm32::time::Hertz;
use embassy_time::{Duration, Timer};

use defmt_rtt as _;
use panic_probe as _;

use coworkos_firmware::clock;
use coworkos_firmware::vl53l5cx::bindings::{
    VL53L5CX_Configuration, VL53L5CX_ResultsData, vl53l5cx_check_data_ready, vl53l5cx_get_ranging_data,
    vl53l5cx_init, vl53l5cx_is_alive, vl53l5cx_set_ranging_frequency_hz, vl53l5cx_set_resolution,
    vl53l5cx_start_ranging,
};
use coworkos_firmware::vl53l5cx::platform::BusContext;

const DEFAULT_ADDRESS_8BIT: u16 = 0x52;

/// `VL53L5CX_RESOLUTION_8X8` — 64 zones. The ULD spells this as the zone
/// count rather than a grid dimension.
const RESOLUTION_8X8: u8 = 64;

/// Frames per second from the sensor. Inference will only run every 1–2 s,
/// but a faster stream makes hand-waving feel responsive while checking the
/// grid by eye. 8×8 caps at 15 Hz.
const RANGING_HZ: u8 = 10;

/// Zone status meaning "range valid". 5 is the good one; 9 means valid but
/// with only one detection, which ST treats as lower confidence.
const STATUS_VALID: u8 = 5;
const STATUS_VALID_WEAK: u8 = 9;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());

    info!("VL53L5CX live ranging — 8x8");

    let _power = Output::new(p.PB0, Level::High, Speed::Low);
    Timer::after(Duration::from_millis(100)).await;

    let mut cfg = embassy_stm32::i2c::Config::default();
    cfg.frequency = Hertz::khz(100);
    cfg.timeout = Duration::from_millis(1000);
    let i2c = I2c::new_blocking(p.I2C1, p.PB8, p.PB9, cfg);

    let mut bus = BusContext::new(i2c);
    let mut dev: VL53L5CX_Configuration = unsafe { core::mem::zeroed() };
    dev.platform.address = DEFAULT_ADDRESS_8BIT;
    dev.platform.ctx = bus.as_ctx_ptr();

    // ---- bring-up ----
    let mut alive: u8 = 0;
    if unsafe { vl53l5cx_is_alive(&mut dev, &mut alive) } != 0 || alive == 0 {
        error!("sensor not responding — check wiring and PWR_EN");
        stall().await;
    }

    info!("downloading firmware...");
    if unsafe { vl53l5cx_init(&mut dev) } != 0 {
        error!("init failed");
        stall().await;
    }

    if unsafe { vl53l5cx_set_resolution(&mut dev, RESOLUTION_8X8) } != 0 {
        error!("set_resolution failed");
        stall().await;
    }

    // Order matters: the ULD validates the frequency against the current
    // resolution, so this has to come after set_resolution or the limit it
    // checks against is the 4x4 one.
    if unsafe { vl53l5cx_set_ranging_frequency_hz(&mut dev, RANGING_HZ) } != 0 {
        error!("set_ranging_frequency_hz failed");
        stall().await;
    }

    if unsafe { vl53l5cx_start_ranging(&mut dev) } != 0 {
        error!("start_ranging failed");
        stall().await;
    }

    info!("ranging at {=u8} Hz — wave a hand in front of the sensor", RANGING_HZ);
    info!("");

    // ~1.4 KB. Kept out of the poll loop so it is allocated once.
    let mut results: VL53L5CX_ResultsData = unsafe { core::mem::zeroed() };
    let mut frame: u32 = 0;

    loop {
        // Poll rather than use the INT line: the interrupt pin is deliberately
        // unwired for now, and at 10 Hz polling costs nothing. Worth revisiting
        // when the firmware moves to an async task.
        let mut ready: u8 = 0;
        if unsafe { vl53l5cx_check_data_ready(&mut dev, &mut ready) } != 0 {
            error!("check_data_ready failed");
            stall().await;
        }

        if ready == 0 {
            Timer::after(Duration::from_millis(5)).await;
            continue;
        }

        if unsafe { vl53l5cx_get_ranging_data(&mut dev, &mut results) } != 0 {
            error!("get_ranging_data failed");
            stall().await;
        }

        frame = frame.wrapping_add(1);

        // Print every fifth frame: 10 Hz of 8-line grids is unreadable, and
        // 2 Hz is about the fastest a person can actually follow.
        if frame % 5 != 0 {
            continue;
        }

        // Summary first: the fastest way to see whether anything changed at
        // all is the range of valid readings, without parsing 64 numbers.
        let mut min = i32::MAX;
        let mut max = i32::MIN;
        let mut valid = 0u32;
        for zone in 0..64usize {
            let status = results.target_status[zone];
            if status == STATUS_VALID || status == STATUS_VALID_WEAK {
                let d = results.distance_mm[zone] as i32;
                min = min.min(d);
                max = max.max(d);
                valid += 1;
            }
        }
        if valid == 0 {
            min = 0;
            max = 0;
        }

        info!(
            "--- frame {=u32} --- {=u32}/64 valid, {=i32}..{=i32} mm, silicon {=i8} C",
            frame, valid, min, max, results.silicon_temp_degc
        );

        for row in 0..8usize {
            let mut cells = [0i32; 8];
            let mut art = [b' '; 8];

            for col in 0..8usize {
                let zone = row * 8 + col;
                let status = results.target_status[zone];
                let valid_zone = status == STATUS_VALID || status == STATUS_VALID_WEAK;

                cells[col] = if valid_zone {
                    results.distance_mm[zone] as i32
                } else {
                    -1
                };
                art[col] = shade(cells[col]);
            }

            // `shade` only ever emits ASCII, so this cannot fail; the fallback
            // keeps the unsafe-free path rather than reaching for
            // from_utf8_unchecked to save a check that never runs.
            let map = core::str::from_utf8(&art).unwrap_or("????????");

            // Numbers and a coarse depth map on the same line. The map is what
            // makes a waving hand obvious; the numbers are what make it
            // trustworthy.
            info!(
                "  {=i32} {=i32} {=i32} {=i32} {=i32} {=i32} {=i32} {=i32}   |{=str}|",
                cells[0],
                cells[1],
                cells[2],
                cells[3],
                cells[4],
                cells[5],
                cells[6],
                cells[7],
                map
            );
        }
    }
}

/// One character per zone, banded by distance, so a frame reads as a picture.
///
/// Bands are chosen for a desk test rather than a room: a hand at arm's length
/// lands in the middle of the range, so waving one produces obvious movement
/// rather than saturating at one end. Phase 2 will want different bands for
/// room-scale mounting.
fn shade(distance_mm: i32) -> u8 {
    match distance_mm {
        d if d < 0 => b'.',    // no valid target
        0..=99 => b'#',        // touching or nearly
        100..=299 => b'%',     // close — a hand held up
        300..=599 => b'*',     // arm's length
        600..=1199 => b'+',    // across a desk
        1200..=2399 => b'-',   // across a small room
        _ => b' ',             // far wall or beyond
    }
}

/// Park forever, keeping PWR_EN asserted so the sensor state can be probed.
async fn stall() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
