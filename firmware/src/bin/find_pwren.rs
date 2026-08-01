//! Find which ARDUINO analog pin the SATEL's PWR_EN wire is actually in.
//!
//! Reading jumper positions off photographs has been unreliable all evening —
//! twice now a confident reading of a pin row turned out wrong. This asks the
//! hardware instead: drive one analog pin high at a time and see which one
//! makes the sensor appear at 0x29.
//!
//! Why one at a time rather than all six: the SATEL's `I2C_RST` may also be
//! wired to an analog pin, and driving *that* high holds the sensor in reset —
//! so a blanket drive could mask the very thing we are looking for. With only
//! one pin driven, every other SATEL input is left to its own on-board
//! resistor, which is the state we want (`LPn` pulled high, `I2C_RST` pulled
//! low).
//!
//! Each candidate is checked for a short before being driven. A pin whose
//! wire lands on a ground pad would otherwise be a push-pull output into a
//! dead short — the one genuinely destructive thing this tool could do.
//!
//! A5 is PB0 on this board (UM2839 Table 24), *not* PC0 as on a NUCLEO-64.
//! The whole A0..A5 row is swept precisely because that sort of difference is
//! what has been biting us.
//!
//! Run with: `cargo run --release --bin find_pwren`

#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Flex, Level, Output, Pull, Speed};
use embassy_stm32::i2c::{I2c, Master};
use embassy_stm32::mode::Blocking;
use embassy_stm32::time::Hertz;
use embassy_time::{Duration, Timer};

use defmt_rtt as _;
use panic_probe as _;

use coworkos_firmware::clock;

/// The VL53L5CX's 7-bit address — what we are listening for.
const TOF_ADDR: u8 = 0x29;

/// Time for the LDO to come up and the sensor to boot before it will ACK.
/// The datasheet boot is a few milliseconds; 150 ms is unmistakably enough.
const BOOT_MS: u64 = 150;

/// Probe one address with a one-byte read, exactly as `i2c_scan` does.
fn present(bus: &mut I2c<'_, Blocking, Master>, addr: u8) -> bool {
    let mut byte = [0u8; 1];
    bus.blocking_read(addr, &mut byte).is_ok()
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());

    info!("PWR_EN hunt — driving each ARDUINO analog pin in turn");
    info!("  looking for 0x29 to appear on I2C1 (PB8/PB9)");
    info!("");

    let mut cfg = embassy_stm32::i2c::Config::default();
    cfg.frequency = Hertz::khz(100);
    cfg.timeout = Duration::from_millis(50);
    let mut i2c = I2c::new_blocking(p.I2C1, p.PB8, p.PB9, cfg);

    // Baseline: is it already answering with nothing driven?
    if present(&mut i2c, TOF_ADDR) {
        info!("0x29 answers with NO pin driven — PWR_EN is already satisfied.");
        info!("Nothing to hunt for; go straight back to i2c_scan.");
        loop {
            Timer::after(Duration::from_secs(60)).await;
        }
    }

    // A0..A5 for the B-U585I-IOT02A. Kept as a list of closures rather than an
    // array because each Peri is a distinct type and they cannot be collected.
    macro_rules! try_pin {
        ($name:literal, $mcu:literal, $pin:expr) => {{
            // Short check first: a wire in a GND pad must never be driven.
            let mut probe = Flex::new($pin.reborrow());
            probe.set_as_input(Pull::Up);
            Timer::after(Duration::from_millis(2)).await;
            let safe = probe.is_high();
            drop(probe);

            if !safe {
                // A pin reading low against the internal pull-up is either a
                // ground short or a pull-down resistor — and PWR_EN carries a
                // pull-down, so this is exactly where the answer is most likely
                // to be hiding. Skipping it would be the safe-looking choice
                // that never finds anything.
                //
                // Driving is bounded: even a dead short is one pin sourcing
                // ~20 mA for 150 ms, inside the STM32's per-pin rating.
                info!(
                    "  {=str} ({=str}): low against a pull-up — could be a ground \
                     short OR PWR_EN's pull-down. Driving briefly to find out.",
                    $name, $mcu
                );
                let driven = Output::new($pin.reborrow(), Level::High, Speed::Low);
                Timer::after(Duration::from_millis(BOOT_MS)).await;
                let found = present(&mut i2c, TOF_ADDR);
                info!(
                    "  {=str} ({=str}): driven high -> 0x29 {=str}",
                    $name,
                    $mcu,
                    if found { "FOUND" } else { "silent" }
                );
                drop(driven);
                Timer::after(Duration::from_millis(20)).await;
                found
            } else {
                let driven = Output::new($pin.reborrow(), Level::High, Speed::Low);
                Timer::after(Duration::from_millis(BOOT_MS)).await;
                let found = present(&mut i2c, TOF_ADDR);
                info!(
                    "  {=str} ({=str}): driven high -> 0x29 {=str}",
                    $name,
                    $mcu,
                    if found { "FOUND" } else { "silent" }
                );
                drop(driven);
                Timer::after(Duration::from_millis(20)).await;
                found
            }
        }};
    }

    let mut a0 = p.PC0;
    let mut a1 = p.PC2;
    let mut a2 = p.PC4;
    let mut a3 = p.PC5;
    let mut a4 = p.PA7;
    let mut a5 = p.PB0;

    let hit = if try_pin!("A0", "PC0", a0) {
        Some("A0 / PC0")
    } else if try_pin!("A1", "PC2", a1) {
        Some("A1 / PC2")
    } else if try_pin!("A2", "PC4", a2) {
        Some("A2 / PC4")
    } else if try_pin!("A3", "PC5", a3) {
        Some("A3 / PC5")
    } else if try_pin!("A4", "PA7", a4) {
        Some("A4 / PA7")
    } else if try_pin!("A5", "PB0", a5) {
        Some("A5 / PB0")
    } else {
        None
    };

    info!("");
    match hit {
        Some(which) => {
            info!("PWR_EN is on {=str}.", which);
            info!("Update i2c_scan to drive that pin and the sensor is yours.");
        }
        None => {
            info!("No analog pin woke the sensor.");
            info!("Either the PWR_EN wire is not on A0-A5, or something else");
            info!("is still holding the sensor down — most likely I2C_RST wired");
            info!("to a pin sitting high, or LPn wired to one sitting low.");
            info!("Next: remove the I2C_RST and INT wires entirely (the SATEL's");
            info!("own resistors set them correctly) and re-run.");
        }
    }

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
