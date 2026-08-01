//! VL53L5CX 8×8 time-of-flight sensor.
//!
//! ST's Ultra Lite Driver does the work; this module is the seam between it
//! and Embassy. Two pieces:
//!
//! - [`bindings`] — bindgen's view of the ULD, generated at build time from
//!   `vendor/vl53l5cx/inc/vl53l5cx_api.h`.
//! - [`platform`] — the seven functions the ULD calls back into. This is the
//!   part that is specific to this board, and the part a port gets wrong.
//!
//! The unsafe surface is deliberately concentrated: three functions touch
//! I2C, one delays, one swaps bytes, and the rest is safe Rust calling into a
//! C library that has been shipping for years. That is the shape the project's
//! language rule calls for — a thin auditable shim under a large safe
//! application, rather than unsafe smeared through a hand-rewritten driver.

pub mod bindings;
pub mod platform;

pub use platform::BusContext;

/// Default 7-bit I2C address of the VL53L5CX.
///
/// The ULD's own `VL53L5CX_DEFAULT_I2C_ADDRESS` is `0x52`, which is the same
/// address in 8-bit form. See [`platform`] for why that distinction matters.
pub const DEFAULT_ADDRESS_7BIT: u8 = 0x29;
