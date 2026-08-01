//! Phase 1, step 1 — confirm the I2C bus and pin assignment empirically.
//!
//! The project plan is explicit that pin numbers come from the board's actual
//! pinout rather than assumption. Documentation gets us a hypothesis; this
//! settles it against the hardware.
//!
//! ## Hypothesis under test
//!
//! | Bus  | SCL | SDA | Routed to                              |
//! |------|-----|-----|----------------------------------------|
//! | I2C1 | PB8 | PB9 | ARDUINO Uno V3 header — D15/D14, free  |
//! | I2C2 | PH4 | PH5 | On-board MEMS sensors                  |
//!
//! ## Why this scans both buses
//!
//! I2C2 is the control. Its population is known ahead of time, so if the scan
//! reports those six addresses then the scan logic, the clock setup, and the
//! timing are all proven correct — and whatever I2C1 reports afterwards can be
//! trusted. Scanning only the bus we care about would leave "found nothing"
//! ambiguous between an empty bus and a broken scanner.
//!
//! It also settles a question the datasheets disagree on: whether this board
//! carries an on-board VL53L5CX. If one is fitted on I2C2 it answers at 0x29,
//! and Phase 1 bring-up can be done with no wiring at all before the SATEL
//! breakout is introduced.
//!
//! ## Expected result
//!
//! - I2C2: 0x10, 0x1e, 0x56, 0x5d, 0x5f, 0x6b (six on-board devices)
//! - I2C1: nothing until the sensors are wired; then 0x29 and 0x68
//!
//! Run with: `cargo run --release --bin i2c_scan`

#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::i2c::{I2c, Master};
use embassy_stm32::mode::Blocking;
use embassy_stm32::time::Hertz;
use embassy_time::{Duration, Timer};

use defmt_rtt as _;
use panic_probe as _;

#[path = "../clock.rs"]
mod clock;

/// 0x00–0x07 and 0x78–0x7f are reserved by the I2C specification and are not
/// probed — addressing them is not merely useless but can put some devices
/// into a mode you did not intend.
const SCAN_START: u8 = 0x08;
const SCAN_END: u8 = 0x77;

/// Everything we expect to meet on this board, so the log names parts rather
/// than leaving bare numbers to be looked up by hand at the bench.
fn describe(addr: u8) -> &'static str {
    match addr {
        // Target sensors, on the ARDUINO header (I2C1)
        0x29 => "VL53L5CX 8x8 ToF  <-- target",
        0x68 => "AMG8833 Grid-EYE (AD0 low)  <-- target",
        0x69 => "AMG8833 Grid-EYE (AD0 high)",
        // On-board MEMS, expected on I2C2 — these are the control group
        0x10 => "VEML6030 ambient light (on-board)",
        0x1e => "IIS2MDC magnetometer (on-board)",
        0x56 => "M24256 EEPROM, memory array (on-board)",
        0x5e => "M24256 EEPROM, identification page (on-board)",
        0x5d => "LPS22HH barometer (on-board)",
        0x5f => "HTS221 humidity/temp (on-board)",
        0x6b => "ISM330DHCX IMU (on-board)",
        _ => "unidentified",
    }
}

/// Probe every valid 7-bit address with a one-byte read.
///
/// A device that is present acknowledges its address; an empty slot NACKs and
/// surfaces as `Error::Nack`. A one-byte read is used rather than a zero-length
/// write because zero-length transfers are a special case on this peripheral,
/// and a probe whose own validity is in question is worth nothing here.
fn scan(bus: &mut I2c<'_, Blocking, Master>) -> u32 {
    let mut found = 0;

    for addr in SCAN_START..=SCAN_END {
        let mut byte = [0u8; 1];
        if bus.blocking_read(addr, &mut byte).is_ok() {
            found += 1;
            info!("    0x{=u8:02x}  {=str}", addr, describe(addr));
        }
    }

    found
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());

    info!("I2C scan — B-U585I-IOT02A / STM32U585");
    info!("");

    // The on-board bus carries its own pull-ups, so the internal ones stay off.
    // A short timeout keeps a wedged bus from turning a 112-address sweep into
    // a two-minute wait.
    let mut onboard_cfg = embassy_stm32::i2c::Config::default();
    onboard_cfg.frequency = Hertz::khz(100);
    onboard_cfg.timeout = Duration::from_millis(50);

    // The ARDUINO header has no pull-ups fitted, so enable the internal ones.
    // They are weak (tens of kilohms) and are a fallback for probing a bare
    // header, not a substitute for the breakout's own resistors once the
    // sensors are wired — see NOTES.md.
    let mut header_cfg = onboard_cfg.clone();
    header_cfg.sda_pullup = true;
    header_cfg.scl_pullup = true;

    // ---- control group: on-board sensors, expected to be populated ----
    info!("I2C2  (PH4 SCL / PH5 SDA)  — on-board sensors");
    let mut i2c2 = I2c::new_blocking(p.I2C2, p.PH4, p.PH5, onboard_cfg);
    let found_onboard = scan(&mut i2c2);

    if found_onboard == 0 {
        defmt::error!(
            "    nothing found — the scan itself is suspect, not the bus. \
             Do not trust the I2C1 result below."
        );
    } else {
        info!("    {} device(s)", found_onboard);
    }
    info!("");

    // ---- the bus we intend to use: ARDUINO Uno V3 header ----
    info!("I2C1  (PB8 SCL / PB9 SDA)  — ARDUINO header, D15/D14");
    let mut i2c1 = I2c::new_blocking(p.I2C1, p.PB8, p.PB9, header_cfg);
    let found_header = scan(&mut i2c1);

    if found_header == 0 {
        info!("    no devices — expected until the sensors are wired");
    } else {
        info!("    {} device(s)", found_header);
    }

    info!("");
    info!("scan complete: I2C2 {}, I2C1 {}", found_onboard, found_header);

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
