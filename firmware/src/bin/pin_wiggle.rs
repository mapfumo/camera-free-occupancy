//! Prove which header pads PB8 and PB9 actually reach, with a logic analyzer.
//!
//! `bus_check` hit the limit of what firmware alone can see: it can tell that
//! SDA is clamped low and that *something* pulls SCL high, but it cannot tell
//! whether the high comes from the SATEL or from pull-ups on the Discovery
//! board itself — and it cannot tell whether PB8/PB9 reach the ARDUINO header
//! at all. That routing claim came from documentation, and this binary is how
//! it stops being a claim.
//!
//! Each pin toggles at its own frequency, so one analyzer channel is enough
//! to identify which MCU pin it is looking at:
//!
//! - **PB8 — expected at header D15 (SCL): 5 Hz**
//! - **PB9 — expected at header D14 (SDA): 1 Hz**
//!
//! Clip the analyzer (or a multimeter — 50% duty reads about half the rail)
//! to D15 and D14. A square wave at the right frequency on the right pad
//! proves the routing. A flat line on D14 while PB9 reads back low here is
//! the short again; a flat line while PB9 reads back high means PB9 simply
//! is not that pad.
//!
//! Both pins drive **open-drain** with the internal pull-up. If PB9 is truly
//! shorted to ground, a push-pull high would dump current into the short;
//! open-drain only ever drives low, which into a ground short is no current
//! at all. The internal pull-up means even a completely unwired pin still
//! shows the toggle.
//!
//! The readback lines printed every few seconds are the same evidence from
//! the firmware side: a released open-drain pin that reads low is being held
//! down by something external.
//!
//! Run with: `cargo run --release --bin pin_wiggle`

#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Flex, Pull, Speed};
use embassy_time::{Duration, Timer};

use defmt_rtt as _;
use panic_probe as _;

use coworkos_firmware::clock;

/// Base tick. PB8 toggles every tick (5 Hz square wave), PB9 every fifth
/// tick (1 Hz). Slow on purpose: visible on any analyzer setting and on a
/// multimeter's averaging.
const HALF_PERIOD_PB8: Duration = Duration::from_millis(100);
const TICKS_PER_PB9_TOGGLE: u32 = 5;
const TICKS_PER_REPORT: u32 = 50; // every 5 s

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());

    info!("pin wiggle — identify PB8/PB9 at the ARDUINO header");
    info!("  PB8: 5 Hz, expected at D15 (SCL)");
    info!("  PB9: 1 Hz, expected at D14 (SDA)");
    info!("  open-drain + internal pull-up; safe against a grounded line");

    let mut pb8 = Flex::new(p.PB8);
    let mut pb9 = Flex::new(p.PB9);
    pb8.set_as_input_output_pull(Speed::Low, Pull::Up);
    pb9.set_as_input_output_pull(Speed::Low, Pull::Up);
    pb8.set_high();
    pb9.set_high();

    let mut tick: u32 = 0;
    // Whether each pin has read high at any point while released this
    // reporting window. A pin that never manages it is being held low.
    let mut pb8_lifted = false;
    let mut pb9_lifted = false;

    loop {
        Timer::after(HALF_PERIOD_PB8).await;
        tick = tick.wrapping_add(1);

        pb8.toggle();
        if tick % TICKS_PER_PB9_TOGGLE == 0 {
            pb9.toggle();
        }

        // Sample the readback only while released — a driven-low open-drain
        // pin reading low proves nothing.
        if pb8.is_set_high() && pb8.is_high() {
            pb8_lifted = true;
        }
        if pb9.is_set_high() && pb9.is_high() {
            pb9_lifted = true;
        }

        if tick % TICKS_PER_REPORT == 0 {
            info!(
                "released readback — PB8 lifts: {=bool}, PB9 lifts: {=bool}",
                pb8_lifted, pb9_lifted
            );
            pb8_lifted = false;
            pb9_lifted = false;
        }
    }
}
