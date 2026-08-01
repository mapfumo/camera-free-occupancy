//! Generated bindings to ST's VL53L5CX ULD.
//!
//! Produced by bindgen in `build.rs` from `vendor/vl53l5cx/inc/vl53l5cx_api.h`.
//! Do not edit — regenerate by touching the header or the vendor sources.
//!
//! The platform callbacks are blocklisted here on purpose: they are defined on
//! the Rust side in [`super::platform`], and having bindgen also declare them
//! would mean the same symbol arriving from two directions.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/vl53l5cx_bindings.rs"));
