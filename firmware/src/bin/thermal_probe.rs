//! Live 8×8 thermal imaging from the AMG8833.
//!
//! The thermal counterpart to `tof_range`. Same grid, same shape of output,
//! measuring an entirely different thing: radiated heat rather than distance.
//!
//! That difference is the reason both sensors are in this project. A person
//! sitting still is nearly invisible to the ToF once they stop moving relative
//! to the furniture — but they never stop radiating heat. Waving a hand in
//! front of this sensor should light up as clearly as it does in range,
//! **and** an unmoving hand should stay lit, which is the part the ToF cannot
//! do.
//!
//! # Reading the output
//!
//! Temperatures in centidegrees Celsius, so 2537 is 25.37 °C. The map bands
//! are relative to the thermistor reading — the sensor's own die temperature
//! standing in for ambient — because what matters for occupancy is "warmer
//! than the room", not an absolute threshold that drifts with the weather.
//!
//! Run with: `cargo run --release --bin thermal_probe`

#![no_std]
#![no_main]

use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_stm32::i2c::I2c;
use embassy_stm32::time::Hertz;
use embassy_time::{Duration, Timer};

use defmt_rtt as _;
use panic_probe as _;

use coworkos_firmware::amg8833::{Amg8833, DEFAULT_ADDRESS, FrameRate, PIXELS};
use coworkos_firmware::clock;

/// Degrees (in centidegrees) above ambient at which a zone is called warm.
/// A clothed person at a couple of metres reads a fraction of a degree above
/// room temperature; skin much closer reads several.
const WARM_STEP: i16 = 50; // 0.50 °C per band

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());

    info!("AMG8833 Grid-EYE — live 8x8 thermal");

    let mut cfg = embassy_stm32::i2c::Config::default();
    cfg.frequency = Hertz::khz(100);
    cfg.timeout = Duration::from_millis(100);
    let mut bus = I2c::new_blocking(p.I2C1, p.PB8, p.PB9, cfg);

    let mut sensor = Amg8833::new(DEFAULT_ADDRESS);

    // 10 Hz for a responsive bench test. Phase 4 will likely drop to 1 Hz:
    // inference runs every 1–2 s, and the slower rate is quieter.
    if let Err(e) = sensor.init(&mut bus, FrameRate::Fps10) {
        error!("AMG8833 init failed: {} — is it wired to 0x68?", e);
        stall().await;
    }

    // The datasheet asks for a settling period after reset before the array
    // reads true. Skipping it gives a first frame or two of nonsense, which is
    // exactly the kind of thing that gets mistaken for a driver bug.
    Timer::after(Duration::from_millis(100)).await;

    info!("initialised — wave a hand in front of the sensor");
    info!("");

    let mut frame = [0i16; PIXELS];
    let mut count: u32 = 0;

    loop {
        // No data-ready flag on this part, so the read cadence is ours to set.
        // 5 Hz against a 10 Hz sensor guarantees a fresh frame every time
        // without polling faster than it can produce them.
        Timer::after(Duration::from_millis(200)).await;

        let ambient = match sensor.read_thermistor(&mut bus) {
            Ok(t) => t,
            Err(e) => {
                error!("thermistor read failed: {}", e);
                stall().await;
            }
        };

        if let Err(e) = sensor.read_frame(&mut bus, &mut frame) {
            error!("frame read failed: {}", e);
            stall().await;
        }

        count = count.wrapping_add(1);

        // Every second frame — 2.5 Hz is about as fast as an 8-line grid can
        // be followed by eye.
        if count % 2 != 0 {
            continue;
        }

        let mut min = i16::MAX;
        let mut max = i16::MIN;
        for &pixel in frame.iter() {
            min = min.min(pixel);
            max = max.max(pixel);
        }

        info!(
            "--- frame {=u32} --- ambient {=i16} c°C, pixels {=i16}..{=i16} c°C (span {=i16})",
            count,
            ambient,
            min,
            max,
            max - min
        );

        for row in 0..8usize {
            let mut cells = [0i16; 8];
            let mut art = [b' '; 8];

            for col in 0..8usize {
                let value = frame[row * 8 + col];
                cells[col] = value;
                art[col] = shade(value - ambient);
            }

            let map = core::str::from_utf8(&art).unwrap_or("????????");

            info!(
                "  {=i16} {=i16} {=i16} {=i16} {=i16} {=i16} {=i16} {=i16}   |{=str}|",
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

/// One character per pixel, banded by how far above ambient it reads.
///
/// Relative rather than absolute on purpose: a 22 °C room and a 28 °C room
/// should produce the same picture of the same person, and only a difference
/// from ambient does that.
fn shade(above_ambient: i16) -> u8 {
    match above_ambient {
        d if d < -WARM_STEP => b'-',        // colder than ambient — a window
        d if d < WARM_STEP => b'.',         // room temperature
        d if d < WARM_STEP * 2 => b'+',     // faintly warm
        d if d < WARM_STEP * 4 => b'*',     // clearly warm — a person nearby
        d if d < WARM_STEP * 8 => b'%',     // hot — skin up close
        _ => b'#',                          // very hot — a mug, a soldering iron
    }
}

async fn stall() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
