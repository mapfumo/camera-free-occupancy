//! CoworkOS occupancy sensor firmware — shared library.
//!
//! The binaries in `src/bin/` are bring-up tools; `src/main.rs` is the
//! application. Everything they have in common lives here so that a tool and
//! the application cannot drift apart on something load-bearing like the clock
//! tree or the sensor driver.

#![no_std]

pub mod amg8833;
pub mod clock;
pub mod debounce;
pub mod encode;
pub mod ili9341;
pub mod i2c_recovery;
pub mod inference;
pub mod ism330dhcx;
pub mod orientation;
pub mod readout;
pub mod vl53l5cx;
