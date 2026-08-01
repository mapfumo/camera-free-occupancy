//! Electrical check of the ARDUINO-header I2C lines, before blaming protocol.
//!
//! The scanner found nothing on I2C1 with the sensors wired. That is a
//! symptom with several possible causes — no power, a missing ground, swapped
//! SDA/SCL, `LPn` held low, or the pins simply not reaching the header — and
//! guessing between them is how an evening disappears.
//!
//! This separates *electrical* from *protocol* without touching the I2C
//! peripheral at all. It reads PB8/PB9 as plain GPIOs and asks one question:
//! **are the breakout's pull-up resistors there?**
//!
//! The VL53L5CX-SATEL fits 2.2 kΩ pull-ups to IOVDD. The STM32's internal
//! pull-down is on the order of 40 kΩ. So driving the internal pull-down
//! against a healthy external pull-up is a divider the pull-up wins by a wide
//! margin — the pin still reads high. With nothing connected, the internal
//! pull-down wins and the pin reads low.
//!
//! | Floating | With internal pull-down | Verdict |
//! |----------|------------------------|---------|
//! | high | high | external pull-up present — line is wired and powered |
//! | high | low  | no external pull-up — line not reaching the sensor |
//! | low  | low  | line held low — short to ground, or a device driving it |
//!
//! Both lines high in both tests means the wiring and supplies are good and
//! the fault is further up: `LPn`, `I2C_RST`, or the sensor itself.
//!
//! Run with: `cargo run --release --bin bus_check`

#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Flex, Pull};
use embassy_time::{Duration, Timer};

use defmt_rtt as _;
use panic_probe as _;

use coworkos_firmware::clock;

/// Settling time after changing pin configuration. Generous on purpose — an
/// RC settle against a 40 kΩ pull-down and a few tens of pF is microseconds,
/// so a millisecond removes any doubt without slowing anything that matters.
const SETTLE: Duration = Duration::from_millis(1);

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());

    info!("I2C1 electrical check — ARDUINO header");
    info!("  PB8 = D15 = SCL,  PB9 = D14 = SDA");
    info!("");

    let mut scl = Flex::new(p.PB8);
    let mut sda = Flex::new(p.PB9);

    // ---- floating: what is the bus idle level? ----
    scl.set_as_input(Pull::None);
    sda.set_as_input(Pull::None);
    Timer::after(SETTLE).await;
    let scl_float = scl.is_high();
    let sda_float = sda.is_high();

    // ---- against an internal pull-down: is there a real pull-up out there? ----
    scl.set_as_input(Pull::Down);
    sda.set_as_input(Pull::Down);
    Timer::after(SETTLE).await;
    let scl_pd = scl.is_high();
    let sda_pd = sda.is_high();

    // ---- against an internal pull-up: how hard is a low line being held? ----
    //
    // This tells a dead short from a resistor. The internal pull-up is around
    // 40 kΩ. A wire landing on GND clamps the pin at 0 V and stays low no
    // matter what. A wire landing on a net with its own pull-down — the
    // SATEL's I2C_RST carries 22 kΩ to ground — forms a divider instead, and
    // sits at roughly a third of the rail. Both read low, but only the short
    // is immovable, so the two faults need different fixes.
    scl.set_as_input(Pull::Up);
    sda.set_as_input(Pull::Up);
    Timer::after(SETTLE).await;
    let scl_pu = scl.is_high();
    let sda_pu = sda.is_high();

    // ---- same pull-up, half a second later: short, or capacitor? ----
    //
    // A line that lands on the decoupling capacitors of an unpowered supply
    // rail is indistinguishable from a short on a millisecond timescale — the
    // pin is charging microfarads through tens of kilohms, and τ is tens to
    // hundreds of milliseconds. Half a second is many time constants: a
    // capacitor has charged and reads high, a true short to copper is still
    // at 0 V. This is the difference between "SDA is in a GND pad" and "SDA
    // is in a supply pad whose rail is dead" — different wire, different fix.
    Timer::after(Duration::from_millis(500)).await;
    let scl_pu_slow = scl.is_high();
    let sda_pu_slow = sda.is_high();

    info!(
        "  with internal pull-up:  SCL={=bool}  SDA={=bool}   (after 500 ms: SCL={=bool}  SDA={=bool})",
        scl_pu, sda_pu, scl_pu_slow, sda_pu_slow
    );

    info!(
        "  SCL (PB8):  floating={=bool}  vs pull-down={=bool}  -> {=str}",
        scl_float,
        scl_pd,
        verdict(scl_float, scl_pd)
    );
    info!(
        "  SDA (PB9):  floating={=bool}  vs pull-down={=bool}  -> {=str}",
        sda_float,
        sda_pd,
        verdict(sda_float, sda_pd)
    );
    info!("");

    if scl_pd && sda_pd {
        info!("Both lines hold high against the internal pull-down.");
        info!("Neither line is shorted. NOTE: this board fits its own pull-ups");
        info!("on PB8/PB9 (confirmed empirically with the header empty), so a");
        info!("high here does NOT prove the sensor is present or powered —");
        info!("only a scan finding 0x29 proves that.");
    } else if !sda_float && scl_pd {
        // The asymmetric case: SCL healthy, SDA down. The wiring is mostly
        // right, so this is one wire in the wrong place rather than a
        // missing supply or ground.
        info!("SCL is healthy but SDA is held low — one wire, not the supply.");
        if sda_pu {
            info!("An internal pull-up lifts it, so the low is a resistor, not a short.");
            info!("SDA is very likely landing on a pulled-down net — I2C_RST is the");
            info!("neighbouring pad and carries 22k to ground. Check it is one pad over.");
        } else if sda_pu_slow {
            info!("The pull-up lifts it, but only after hundreds of milliseconds:");
            info!("that is a capacitor charging, not a short. The SDA wire is almost");
            info!("certainly in a SUPPLY pad (AVDD or IOVDD) of an UNPOWERED rail.");
            info!("Check where the SDA wire lands, and measure both supply pads.");
        } else {
            info!("An internal pull-up cannot lift it even after 500 ms: a hard");
            info!("short to copper. The SDA jumper is in a GND pad, or pinched.");
        }
    } else if !scl_float && !sda_float {
        info!("Both lines are stuck low.");
        info!("Suspect a short to ground, or SDA/SCL swapped onto a driven pin.");
    } else {
        info!("At least one line has no external pull-up behind it.");
        info!("That line is not reaching a powered sensor: check the jumper,");
        info!("the ground return, and that IOVDD is actually at 3.3 V.");
    }

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

fn verdict(floating: bool, with_pulldown: bool) -> &'static str {
    match (floating, with_pulldown) {
        (true, true) => "external pull-up present",
        (true, false) => "NO external pull-up — not reaching a powered sensor",
        (false, _) => "held LOW — short, or driven by something",
    }
}
