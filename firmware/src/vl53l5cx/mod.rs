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

use crate::orientation::Orientation;

/// How this rig's VL53L5CX maps onto the world frame (see
/// [`crate::orientation`]). Apply it to every per-zone array taken out of
/// `VL53L5CX_ResultsData` — distance *and* status, or they stop lining up.
///
/// Established on hardware 2026-11-22. Vertical by sweeping a flat card in
/// the heatmap viewer: no transpose, raw row 0 already the top. Horizontal by
/// registration against the (verified) AMG8833 — a mug on a card held to one
/// side landed in columns 0–2 on both sensors only with the column flip, so
/// raw column 0 is on the *right* seen from behind. Columns flip, rows do not.
/// Judging left/right by eye gave contradictory answers; see NOTES.md.
///
/// Agrees with UM2884 §2.2, which says zone 0 sees the *top right* of the
/// scene: two independent sources, same answer. The 2026-09-26 A3-sheet hint
/// (raw row 0 nearer, so the top) also agrees.
///
/// Tied to the physical mount. Re-establish it if the breakout is ever
/// re-seated or the board rotated.
pub const ORIENTATION: Orientation = Orientation { transpose: false, flip_rows: false, flip_cols: true };

/// Default 7-bit I2C address of the VL53L5CX.
///
/// The ULD's own `VL53L5CX_DEFAULT_I2C_ADDRESS` is `0x52`, which is the same
/// address in 8-bit form. See [`platform`] for why that distinction matters.
pub const DEFAULT_ADDRESS_7BIT: u8 = 0x29;

/// Zone status meaning "range valid" — the one ST trusts fully.
pub const STATUS_VALID: u8 = 5;
/// Valid, but from a single detection; ST treats it as lower confidence.
pub const STATUS_VALID_WEAK: u8 = 9;

/// Whether a zone's distance can be trusted, from its `target_status`.
///
/// Every other status is some flavour of "no usable return" — out of range,
/// too little signal (dark surfaces, which this rig's black chair is), or a
/// wrapped/merged target — and the distance alongside it is not a
/// measurement. The host-side viewer applies the same rule; keep them in step.
pub const fn is_valid(status: u8) -> bool {
    status == STATUS_VALID || status == STATUS_VALID_WEAK
}
