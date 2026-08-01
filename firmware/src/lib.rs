//! CoworkOS occupancy sensor firmware — shared library.
//!
//! The binaries in `src/bin/` are bring-up tools; `src/main.rs` is the
//! application. Everything they have in common lives here so that a tool and
//! the application cannot drift apart on something load-bearing like the clock
//! tree or the sensor driver.

#![no_std]

pub mod clock;
pub mod vl53l5cx;
