//! I2C bus recovery: free a target left holding SDA low.
//!
//! Every recording session ends with `timeout` killing probe-rs, which halts
//! the MCU wherever it is — sometimes in the middle of a read. A target that
//! was driving SDA low for a data bit keeps driving it: it is still waiting
//! for the clock edges that would finish the byte. The next run's first
//! transfer then sees a busy bus and the AMG8833 probe fails (three start-up
//! dropouts in two days, each gone on a retry; TODO.md, hardware loose ends).
//!
//! The standard fix (NXP UM10204 §3.1.16, "bus clear"): with the I2C
//! peripheral off, drive SCL by hand for up to nine pulses until the target
//! lets go of SDA, then send a STOP. A target that was idle ignores clocks
//! with no START, so doing this on every boot is harmless.
//!
//! Call it before `I2c::new_blocking` takes the pins. The pins are driven
//! open-drain, so nothing is ever pushed high against a target pulling low.

use embassy_stm32::Peri;
use embassy_stm32::gpio::{Flex, Pin, Speed};

/// What recovery found and did.
#[derive(Clone, Copy, defmt::Format)]
pub struct Recovery {
    /// SDA was low with SCL released: a target was holding the bus.
    pub sda_was_stuck: bool,
    /// SCL pulses it took for SDA to come back high (0 if it never was low).
    pub pulses: u8,
    /// Both lines high at the end: the bus is free.
    pub bus_free: bool,
}

/// Half an SCL period at 100 kHz. Busy-wait: this runs once at boot, before
/// anything else needs the CPU, and must not depend on the time driver.
fn half_period() {
    // 5 µs at the 160 MHz SYSCLK from `clock::config()`; slower is fine.
    cortex_m::asm::delay(800);
}

pub fn recover(scl: Peri<'_, impl Pin>, sda: Peri<'_, impl Pin>) -> Recovery {
    let mut scl = Flex::new(scl);
    let mut sda = Flex::new(sda);
    // Open-drain with input: set_high releases the line, set_low pulls it.
    scl.set_high();
    sda.set_high();
    scl.set_as_input_output(Speed::Low);
    sda.set_as_input_output(Speed::Low);
    half_period();

    let sda_was_stuck = sda.is_low();
    let mut pulses = 0;
    while sda.is_low() && pulses < 9 {
        scl.set_low();
        half_period();
        scl.set_high();
        half_period();
        pulses += 1;
    }

    // STOP: SDA low -> high while SCL is high. Leaves every target idle.
    scl.set_low();
    half_period();
    sda.set_low();
    half_period();
    scl.set_high();
    half_period();
    sda.set_high();
    half_period();

    Recovery {
        sda_was_stuck,
        pulses: if sda_was_stuck { pulses } else { 0 },
        bus_free: scl.is_high() && sda.is_high(),
    }
    // The Flex pins drop here and go back to analog; the I2C driver
    // reconfigures them as alternate-function open-drain.
}
