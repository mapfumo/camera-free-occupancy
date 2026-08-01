//! Panasonic AMG8833 "Grid-EYE" 8×8 thermopile array.
//!
//! Deliberately the opposite of the VL53L5CX integration next door. That one
//! wraps ST's Ultra Lite Driver — tens of thousands of lines of C, an 86 KB
//! firmware blob to upload, calibration tables — behind a thin `unsafe` shim,
//! because rewriting it in Rust would take weeks and produce a fresh set of
//! bugs in code that already works.
//!
//! This is twenty registers and a 128-byte read. There is no vendor library
//! worth linking, so it is plain safe Rust with no `unsafe` at all. Same
//! project, same bus, opposite call — which is the point. The rule the roadmap
//! sets is *is the unsafe concentrated or smeared?* Here there is none to
//! concentrate, so C would buy nothing.
//!
//! # Units
//!
//! Everything comes out as **centidegrees Celsius** (`i16`): 2537 is 25.37 °C.
//! Integers rather than floats not because the M33 lacks an FPU — it has one —
//! but because these values are logged, transported and fed to a model, and a
//! fixed-point integer has exactly one representation. Float formatting
//! differences between the firmware and the Python side are a real source of
//! "the CSV doesn't match the device" confusion.
//!
//! # Two encodings, and a disagreement worth knowing about
//!
//! The pixel registers and the thermistor register do **not** share a number
//! format, which is the easiest thing to get wrong here:
//!
//! - **Pixels** are 12-bit two's complement, 0.25 °C per LSB.
//! - **Thermistor** is 12-bit sign-and-magnitude — bit 11 is a sign flag, not
//!   part of the value — at 0.0625 °C per LSB.
//!
//! Note that Adafruit's widely-used AMG88xx driver treats the *pixels* as
//! sign-and-magnitude too, which contradicts the datasheet. Above 0 °C the two
//! interpretations are identical, so for indoor occupancy the distinction
//! never shows. This implementation follows the datasheet; the sub-zero path
//! is therefore untested, and if this sensor ever ends up outdoors that is the
//! first thing to check.

use embassy_stm32::i2c::{Error as I2cError, I2c, Master};
use embassy_stm32::mode::Blocking;

/// Number of pixels in the array.
pub const PIXELS: usize = 64;

/// I2C address with the AD_SELECT pin low — the SparkFun board's default.
pub const DEFAULT_ADDRESS: u8 = 0x68;
/// I2C address with AD_SELECT tied high.
pub const ALT_ADDRESS: u8 = 0x69;

// ---- register map ---------------------------------------------------------

const REG_POWER_CONTROL: u8 = 0x00;
const REG_RESET: u8 = 0x01;
const REG_FRAME_RATE: u8 = 0x02;
const REG_INT_CONTROL: u8 = 0x03;
const REG_THERMISTOR_LOW: u8 = 0x0e;
/// Pixel data starts here: 64 pixels × 2 bytes, low byte first, auto-incrementing.
const REG_PIXEL_BASE: u8 = 0x80;

const POWER_NORMAL: u8 = 0x00;
/// Clears every register back to its power-on state, including the interrupt
/// table. `0x30` — a "flag reset" — only clears status bits.
const RESET_INITIAL: u8 = 0x3f;
const INT_DISABLED: u8 = 0x00;

/// Errors from talking to the sensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum Error {
    /// The I2C transfer failed — wrong address, no ACK, bus fault.
    Bus,
}

impl From<I2cError> for Error {
    fn from(_: I2cError) -> Self {
        Error::Bus
    }
}

/// Frame rate. 10 Hz is the sensor's maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum FrameRate {
    /// 10 frames per second.
    Fps10,
    /// 1 frame per second. Lower noise, and a fair match for occupancy, where
    /// inference only runs every 1–2 s anyway.
    Fps1,
}

impl FrameRate {
    fn bits(self) -> u8 {
        match self {
            FrameRate::Fps10 => 0x00,
            FrameRate::Fps1 => 0x01,
        }
    }
}

type Bus = I2c<'static, Blocking, Master>;

/// A Grid-EYE on a shared I2C bus.
///
/// Holds no peripheral of its own — the bus is passed in per call. The
/// VL53L5CX's ULD owns the `I2c` through `BusContext`, and this sensor shares
/// the same wires, so making that sharing explicit at every call site is
/// honest about what is going on. It also leaves the door open for the two
/// sensors to end up behind a mutex when the firmware becomes multi-task.
pub struct Amg8833 {
    address: u8,
}

impl Amg8833 {
    /// Create a handle for a sensor at the given 7-bit address.
    ///
    /// No bus traffic happens here; call [`init`](Self::init) next.
    pub fn new(address: u8) -> Self {
        Self { address }
    }

    /// Wake the sensor, reset it, disable interrupts, and set the frame rate.
    ///
    /// The reset is deliberate rather than defensive: on a warm restart — a
    /// debugger reflash, which is most restarts during development — the
    /// sensor keeps whatever state the previous run left, including a
    /// configured interrupt table. Starting from a known configuration means
    /// the firmware behaves the same whether the board was just power-cycled
    /// or just reflashed.
    pub fn init(&mut self, bus: &mut Bus, rate: FrameRate) -> Result<(), Error> {
        self.write(bus, REG_POWER_CONTROL, POWER_NORMAL)?;
        self.write(bus, REG_RESET, RESET_INITIAL)?;
        self.write(bus, REG_INT_CONTROL, INT_DISABLED)?;
        self.write(bus, REG_FRAME_RATE, rate.bits())?;
        Ok(())
    }

    /// Read all 64 pixels, in centidegrees Celsius.
    ///
    /// Index `row * 8 + col`, matching the VL53L5CX's zone ordering so the two
    /// grids can be concatenated without either one being transposed.
    ///
    /// The whole array is read in a single transaction. The sensor has no
    /// mechanism to latch a frame, so a read that straddles an internal update
    /// can in principle mix two frames. At 10 Hz against a ~3 ms transfer the
    /// window is small, and for occupancy — where a person moves centimetres
    /// per frame — a torn frame is indistinguishable from a real one. Worth
    /// remembering if the data ever looks strange in a way nothing else
    /// explains.
    pub fn read_frame(&mut self, bus: &mut Bus, out: &mut [i16; PIXELS]) -> Result<(), Error> {
        let mut raw = [0u8; PIXELS * 2];
        bus.blocking_write_read(self.address, &[REG_PIXEL_BASE], &mut raw)?;

        for (pixel, bytes) in out.iter_mut().zip(raw.chunks_exact(2)) {
            // Little-endian: the low byte sits at the lower register address.
            let word = u16::from_le_bytes([bytes[0], bytes[1]]);
            *pixel = quarter_degrees_to_centi(twelve_bit_twos_complement(word));
        }

        Ok(())
    }

    /// Read the on-chip thermistor, in centidegrees Celsius.
    ///
    /// This is the sensor's own die temperature, not part of the image. Useful
    /// as a reference point: the pixel array measures radiated heat, so
    /// knowing the ambient the sensor itself sits at is what makes "warm
    /// relative to the room" meaningful rather than an absolute threshold that
    /// drifts with the seasons.
    pub fn read_thermistor(&mut self, bus: &mut Bus) -> Result<i16, Error> {
        let mut raw = [0u8; 2];
        bus.blocking_write_read(self.address, &[REG_THERMISTOR_LOW], &mut raw)?;

        let word = u16::from_le_bytes([raw[0], raw[1]]);
        // Sign-and-magnitude here, unlike the pixels — see the module docs.
        let magnitude = (word & 0x07ff) as i32;
        let signed = if word & 0x0800 != 0 { -magnitude } else { magnitude };

        // 0.0625 °C per LSB -> centidegrees. Done in i32 so the multiply
        // cannot overflow before the divide brings it back into range.
        Ok(((signed * 625) / 100) as i16)
    }

    fn write(&mut self, bus: &mut Bus, register: u8, value: u8) -> Result<(), Error> {
        bus.blocking_write(self.address, &[register, value])?;
        Ok(())
    }
}

/// Convert the sensor's 0.25 °C units to centidegrees, saturating.
///
/// The multiply genuinely can overflow `i16`: the largest 12-bit value is
/// 2047, and 2047 × 25 is 51175 against an `i16` ceiling of 32767. In release
/// builds that wraps silently to a large *negative* number — a garbled read
/// would arrive downstream as a plausible-looking sub-zero temperature rather
/// than obvious nonsense.
///
/// Saturating instead of wrapping keeps a corrupted read looking corrupted.
/// The clamp only engages far outside the sensor's specified −20…80 °C range
/// (−2000…8000 centidegrees), so it never touches a real measurement.
fn quarter_degrees_to_centi(quarters: i16) -> i16 {
    let centi = quarters as i32 * 25;
    centi.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// Sign-extend a 12-bit two's complement value held in the low bits of a word.
fn twelve_bit_twos_complement(word: u16) -> i16 {
    let value = word & 0x0fff;
    if value & 0x0800 != 0 {
        // Negative: set the top four bits so the i16 carries the right sign.
        (value | 0xf000) as i16
    } else {
        value as i16
    }
}

#[cfg(test)]
mod tests {
    // NOTE: these do not run today. The crate depends on embassy-executor,
    // which is Cortex-M only, so `cargo test` cannot build it for the host.
    // They are kept because the conversions below are the error-prone part of
    // this driver and because splitting the sensor drivers into a
    // dependency-free crate — likely when the FreeRTOS comparison needs the
    // same maths in C — makes them runnable as they stand.
    //
    // Until then the same cases are verified out-of-tree; the overflow case
    // in particular was a real bug, caught because 2047 * 25 does not fit an
    // i16.
    use super::*;

    #[test]
    fn positive_pixel_values_pass_through() {
        assert_eq!(twelve_bit_twos_complement(0x0000), 0);
        assert_eq!(twelve_bit_twos_complement(0x0064), 100); // 25.00 C
        assert_eq!(twelve_bit_twos_complement(0x07ff), 2047); // largest positive
    }

    #[test]
    fn negative_pixel_values_sign_extend() {
        assert_eq!(twelve_bit_twos_complement(0x0fff), -1);
        assert_eq!(twelve_bit_twos_complement(0x0800), -2048); // most negative
    }

    #[test]
    fn bits_above_twelve_are_ignored() {
        // The sensor never sets them, but a garbled read must not become a
        // wildly wrong temperature.
        assert_eq!(twelve_bit_twos_complement(0xf064), 100);
    }

    #[test]
    fn realistic_temperatures_convert_exactly() {
        assert_eq!(quarter_degrees_to_centi(100), 2500); // 25.00 C
        assert_eq!(quarter_degrees_to_centi(320), 8000); // 80 C, top of spec
        assert_eq!(quarter_degrees_to_centi(-200), -5000); // -50 C
    }

    #[test]
    fn out_of_range_values_saturate_rather_than_wrap() {
        // 2047 * 25 = 51175, which does not fit an i16. Wrapping would turn a
        // garbled read into a plausible-looking negative temperature; the
        // clamp keeps nonsense looking like nonsense.
        assert_eq!(quarter_degrees_to_centi(2047), i16::MAX);
        assert_eq!(quarter_degrees_to_centi(-2048), i16::MIN);
    }
}
