//! Wiring check for the display harness, end to end, with a multimeter.
//!
//! Drives each display signal high on its own for 5 s, in turn, all others
//! low, and logs which one is high. Measure at the **display's** pins (black
//! probe on display GND): the named pin should read ~3.3 V during its turn
//! and ~0 V otherwise. That proves the MCU pin → ARDUINO label → Dupont
//! wire → display pin chain for each signal, and catches swapped wires.
//!
//! Run with: `cargo run --release --bin display_pins`

#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::{Duration, Timer};
use {defmt_rtt as _, panic_probe as _};

use coworkos_firmware::clock;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());
    let mut pins = [
        ("SCK   (D13, PE13)", Output::new(p.PE13, Level::Low, Speed::Low)),
        ("SDI   (D11, PE15)", Output::new(p.PE15, Level::Low, Speed::Low)),
        ("CS    (D10, PE12)", Output::new(p.PE12, Level::Low, Speed::Low)),
        ("DC    (D8,  PC1) ", Output::new(p.PC1, Level::Low, Speed::Low)),
        ("RESET (D9,  PA8) ", Output::new(p.PA8, Level::Low, Speed::Low)),
    ];
    info!("display pin check: each signal high for 5 s in turn; measure at the display");
    loop {
        for i in 0..pins.len() {
            for (j, (_, pin)) in pins.iter_mut().enumerate() {
                pin.set_level(if i == j { Level::High } else { Level::Low });
            }
            info!("HIGH now: {=str}", pins[i].0);
            Timer::after(Duration::from_secs(5)).await;
        }
        for (_, pin) in pins.iter_mut() {
            pin.set_low();
        }
        info!("all LOW for 5 s");
        Timer::after(Duration::from_secs(5)).await;
    }
}
