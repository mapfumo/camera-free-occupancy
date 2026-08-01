//! The VL53L5CX ULD platform layer, implemented in Rust.
//!
//! ST ships a `platform.c` template expecting these seven functions to be
//! written in C against a vendor HAL. We write them in Rust instead and let
//! the C driver call back in. That keeps the entire `unsafe` surface of the
//! sensor integration in this one file — three functions that touch I2C, one
//! that delays, one that swaps bytes — instead of spreading a second C
//! translation unit and its own HAL dependency through the build.
//!
//! # Address width
//!
//! The ULD's `VL53L5CX_DEFAULT_I2C_ADDRESS` is `0x52`. That is an **8-bit**
//! address: the 7-bit address `0x29` shifted left by one to leave room for the
//! read/write bit. ST's HAL takes 8-bit addresses, so their reference
//! `platform.c` passes the field straight through. Embassy takes 7-bit
//! addresses, so every function here shifts right by one.
//!
//! Get this wrong and the sensor simply never acknowledges — the ULD reports a
//! generic failure, and nothing in it points at the cause.
//!
//! # Transfer sizes
//!
//! `VL53L5CX_WrMulti` is not a small-payload path. Firmware download calls it
//! with `0x8000` bytes at a time, three times over, to push 86,017 bytes into
//! the sensor. That rules out staging the register address and payload into a
//! scratch buffer, so writes go out as two merged `Operation::Write`s — one
//! bus transaction, no stop between the address and the data.

use core::ffi::c_void;

use embassy_stm32::i2c::{I2c, Master};
use embassy_stm32::mode::Blocking;
use embassy_time::{Duration, block_for};
use embedded_hal::i2c::Operation;

use super::bindings::VL53L5CX_Platform;

/// The ULD's success code. Every platform function returns 0 for OK and
/// non-zero for failure; the driver ORs these together as it goes.
const STATUS_OK: u8 = 0;
/// Our generic failure code. The ULD only checks against zero, so the specific
/// value carries no meaning to it — it is here to be greppable in a log.
const STATUS_ERROR: u8 = 255;

/// Owns the I2C peripheral the sensor is reached through.
///
/// A pointer to this is stashed in `VL53L5CX_Platform::ctx` so the callbacks
/// can find their bus without a global. That indirection is what keeps a
/// second sensor on the same bus possible later without redesigning this.
pub struct BusContext {
    i2c: I2c<'static, Blocking, Master>,
}

impl BusContext {
    /// Take ownership of a configured blocking I2C peripheral.
    pub fn new(i2c: I2c<'static, Blocking, Master>) -> Self {
        Self { i2c }
    }

    /// Pointer to store in `VL53L5CX_Platform::ctx`.
    ///
    /// The caller must keep this `BusContext` alive and un-moved for as long
    /// as the ULD may call back — in practice, for the lifetime of the
    /// program. Moving it after handing out this pointer would leave the C
    /// side holding a dangling reference.
    pub fn as_ctx_ptr(&mut self) -> *mut c_void {
        self as *mut BusContext as *mut c_void
    }

    /// Direct access to the bus, for the AMG8833 and for bus scans.
    pub fn i2c(&mut self) -> &mut I2c<'static, Blocking, Master> {
        &mut self.i2c
    }
}

/// Recover the bus handle and 7-bit address from a platform pointer.
///
/// # Safety
///
/// `p` must be a valid `VL53L5CX_Platform` whose `ctx` was set from
/// [`BusContext::as_ctx_ptr`] on a still-live `BusContext`.
unsafe fn parts<'a>(
    p: *mut VL53L5CX_Platform,
) -> Option<(&'a mut I2c<'static, Blocking, Master>, u8)> {
    let platform = unsafe { p.as_mut()? };
    if platform.ctx.is_null() {
        return None;
    }
    let ctx = unsafe { &mut *(platform.ctx as *mut BusContext) };
    // 8-bit address from the ULD down to the 7-bit address Embassy wants.
    let address = (platform.address >> 1) as u8;
    Some((&mut ctx.i2c, address))
}

/// Read one byte from a 16-bit register address.
///
/// # Safety
///
/// Called by the ULD. `p_value` must point to a writable byte.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn VL53L5CX_RdByte(
    p_platform: *mut VL53L5CX_Platform,
    register_address: u16,
    p_value: *mut u8,
) -> u8 {
    let Some((i2c, address)) = (unsafe { parts(p_platform) }) else {
        return STATUS_ERROR;
    };
    if p_value.is_null() {
        return STATUS_ERROR;
    }

    let mut byte = [0u8; 1];
    match i2c.blocking_write_read(address, &register_address.to_be_bytes(), &mut byte) {
        Ok(()) => {
            unsafe { *p_value = byte[0] };
            STATUS_OK
        }
        Err(_) => STATUS_ERROR,
    }
}

/// Write one byte to a 16-bit register address.
///
/// # Safety
///
/// Called by the ULD.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn VL53L5CX_WrByte(
    p_platform: *mut VL53L5CX_Platform,
    register_address: u16,
    value: u8,
) -> u8 {
    let Some((i2c, address)) = (unsafe { parts(p_platform) }) else {
        return STATUS_ERROR;
    };

    let reg = register_address.to_be_bytes();
    match i2c.blocking_write(address, &[reg[0], reg[1], value]) {
        Ok(()) => STATUS_OK,
        Err(_) => STATUS_ERROR,
    }
}

/// Read a buffer starting at a 16-bit register address.
///
/// # Safety
///
/// Called by the ULD. `p_values` must be writable for `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn VL53L5CX_RdMulti(
    p_platform: *mut VL53L5CX_Platform,
    register_address: u16,
    p_values: *mut u8,
    size: u32,
) -> u8 {
    let Some((i2c, address)) = (unsafe { parts(p_platform) }) else {
        return STATUS_ERROR;
    };
    if size == 0 {
        return STATUS_OK;
    }
    if p_values.is_null() {
        return STATUS_ERROR;
    }

    let buffer = unsafe { core::slice::from_raw_parts_mut(p_values, size as usize) };
    match i2c.blocking_write_read(address, &register_address.to_be_bytes(), buffer) {
        Ok(()) => STATUS_OK,
        Err(_) => STATUS_ERROR,
    }
}

/// Write a buffer starting at a 16-bit register address.
///
/// Sent as two merged writes rather than one staged buffer: the firmware
/// download pushes 32 KB per call, which no scratch buffer here could hold.
/// Consecutive `Operation::Write`s are merged into a single bus transaction,
/// so the sensor sees the address and payload without a stop between them.
///
/// # Safety
///
/// Called by the ULD. `p_values` must be readable for `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn VL53L5CX_WrMulti(
    p_platform: *mut VL53L5CX_Platform,
    register_address: u16,
    p_values: *mut u8,
    size: u32,
) -> u8 {
    let Some((i2c, address)) = (unsafe { parts(p_platform) }) else {
        return STATUS_ERROR;
    };
    if size == 0 {
        return STATUS_OK;
    }
    if p_values.is_null() {
        return STATUS_ERROR;
    }

    let reg = register_address.to_be_bytes();
    let data = unsafe { core::slice::from_raw_parts(p_values, size as usize) };

    let mut operations = [Operation::Write(&reg), Operation::Write(data)];
    match i2c.blocking_transaction(address, &mut operations) {
        Ok(()) => STATUS_OK,
        Err(_) => STATUS_ERROR,
    }
}

/// Byte-swap each 32-bit word in the buffer.
///
/// Done bytewise rather than through `u32` loads: the ULD hands us pointers
/// into packed structures with no alignment guarantee, and an unaligned `u32`
/// access is a fault waiting for the one build where the optimiser stops being
/// forgiving. `size` is documented as always a multiple of 4; the remainder is
/// left untouched rather than trusted.
///
/// # Safety
///
/// Called by the ULD. `buffer` must be readable and writable for `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn VL53L5CX_SwapBuffer(buffer: *mut u8, size: u16) {
    if buffer.is_null() || size == 0 {
        return;
    }

    let bytes = unsafe { core::slice::from_raw_parts_mut(buffer, size as usize) };
    for word in bytes.chunks_exact_mut(4) {
        word.swap(0, 3);
        word.swap(1, 2);
    }
}

/// Block for a number of milliseconds.
///
/// Deliberately a busy-wait rather than an async timer: the ULD is
/// synchronous C and cannot await. The waits it asks for are short and occur
/// during initialisation, so stalling the executor here is acceptable — but it
/// is the reason sensor setup should not sit on a latency-critical path.
///
/// # Safety
///
/// Called by the ULD.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn VL53L5CX_WaitMs(_p_platform: *mut VL53L5CX_Platform, time_ms: u32) -> u8 {
    block_for(Duration::from_millis(time_ms as u64));
    STATUS_OK
}

/// Hardware reset — **not implemented**.
///
/// A real reset needs the sensor's PWREN or LPn line under GPIO control, and
/// nothing is wired for that yet. The ULD never calls this itself, so leaving
/// it unimplemented costs nothing.
///
/// It returns an error rather than success on purpose. Returning `STATUS_OK`
/// without resetting anything would be a silent lie to whichever future caller
/// reaches for it, and that caller would then be debugging the sensor instead
/// of this function.
///
/// # Safety
///
/// Called by the ULD.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn VL53L5CX_Reset_Sensor(_p_platform: *mut VL53L5CX_Platform) -> u8 {
    STATUS_ERROR
}
