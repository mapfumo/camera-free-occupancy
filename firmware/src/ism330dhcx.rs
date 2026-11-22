//! ISM330DHCX accelerometer — the rig measuring its own angle.
//!
//! The IMU is soldered to the B-U585I-IOT02A on I2C2 (PH4/PH5, `0x6b`), a
//! separate bus from the two occupancy sensors, so nothing here can disturb
//! them. Only the accelerometer is used: at rest it measures gravity, and
//! gravity's direction in the board's frame *is* the mount angle.
//!
//! Two jobs, one reading:
//!
//! - **Mount angle.** Measured directly, to a fraction of a degree, instead of
//!   inferred from where a mug lands in an 8×8 grid — which in this room
//!   failed, because furniture hides the floor and warm seats mimic targets.
//! - **Drift detection.** Record the resting angle at the start of each
//!   collection session; a session that disagrees has been knocked, and its
//!   frames carry a different "empty room" than the rest. Silent label noise
//!   becomes a loud number.
//!
//! # Geometry
//!
//! At rest the accelerometer reads +1 g along whichever direction is *up*.
//! On this rig the sensors look along the IMU's **−Z**, so the optical axis
//! sits `asin(a_z / |a|)` below horizontal — independent of how the board is
//! turned within its own plane.
//!
//! The sign was established on hardware (2026-11-22), not assumed: the first
//! version took +Z and reported the rig pointing 50° *up*, which cannot be
//! true of a sensor that sees a seated person below it. Corrected, the
//! seated head's measured ToF range (0.52–0.57 m) agrees with the geometry. The in-plane
//! components give the roll about that axis, useful as a drift reference even
//! though its zero depends on the mount.

use embassy_stm32::i2c::{Error, I2c, Master};
use embassy_stm32::mode::Blocking;

type Bus = I2c<'static, Blocking, Master>;

/// Fixed by the board: SA0 is strapped high.
pub const ADDRESS: u8 = 0x6b;

const REG_WHO_AM_I: u8 = 0x0f;
const WHO_AM_I_VALUE: u8 = 0x6b;
const REG_CTRL1_XL: u8 = 0x10;
const REG_CTRL3_C: u8 = 0x12;
const REG_STATUS: u8 = 0x1e;
const REG_OUTX_L_A: u8 = 0x28;

/// 104 Hz output, ±2 g: the finest scale, and gravity never exceeds it at
/// rest.
const CTRL1_XL_104HZ_2G: u8 = 0b0100_0000;
/// Block data update (a sample's high and low bytes never come from two
/// different conversions) plus register auto-increment for burst reads.
const CTRL3_C_BDU_IF_INC: u8 = 0b0100_0100;

/// ±2 g full scale: 0.061 mg per LSB (datasheet Table 2).
pub const MICRO_G_PER_LSB: i32 = 61;

#[derive(Debug, defmt::Format)]
pub enum ImuError {
    Bus(Error),
    /// Something answered at the address but is not this part.
    WrongDevice(u8),
}

impl From<Error> for ImuError {
    fn from(e: Error) -> Self {
        ImuError::Bus(e)
    }
}

pub struct Ism330dhcx;

impl Ism330dhcx {
    /// Check identity and start the accelerometer. The gyro stays powered
    /// down — it measures rotation rate, which at rest is noise.
    pub fn init(bus: &mut Bus) -> Result<Self, ImuError> {
        let mut id = [0u8; 1];
        bus.blocking_write_read(ADDRESS, &[REG_WHO_AM_I], &mut id)?;
        if id[0] != WHO_AM_I_VALUE {
            return Err(ImuError::WrongDevice(id[0]));
        }
        bus.blocking_write(ADDRESS, &[REG_CTRL3_C, CTRL3_C_BDU_IF_INC])?;
        bus.blocking_write(ADDRESS, &[REG_CTRL1_XL, CTRL1_XL_104HZ_2G])?;
        Ok(Self)
    }

    /// Whether a new accelerometer sample is waiting (STATUS_REG.XLDA).
    pub fn accel_ready(&mut self, bus: &mut Bus) -> Result<bool, ImuError> {
        let mut status = [0u8; 1];
        bus.blocking_write_read(ADDRESS, &[REG_STATUS], &mut status)?;
        Ok(status[0] & 0x01 != 0)
    }

    /// One raw sample, X/Y/Z in LSB. Multiply by [`MICRO_G_PER_LSB`] for µg.
    pub fn read_accel(&mut self, bus: &mut Bus) -> Result<[i16; 3], ImuError> {
        let mut raw = [0u8; 6];
        bus.blocking_write_read(ADDRESS, &[REG_OUTX_L_A], &mut raw)?;
        Ok([
            i16::from_le_bytes([raw[0], raw[1]]),
            i16::from_le_bytes([raw[2], raw[3]]),
            i16::from_le_bytes([raw[4], raw[5]]),
        ])
    }
}

/// The mount angle from an averaged gravity reading, in any consistent unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tilt {
    /// How far the optical axis points below horizontal, degrees. Negative
    /// means it points up.
    pub depression_deg: f32,
    /// Rotation about the optical axis, degrees from the board's +Y axis.
    /// Its zero is arbitrary for a given mount; its *change* is what matters.
    pub roll_deg: f32,
    /// Magnitude of the measured vector relative to 1 g, if given in g.
    /// Far from 1 means the board was moving, or the reading is wrong.
    pub magnitude: f32,
}

/// Compute [`Tilt`] from a gravity vector in the IMU frame.
pub fn tilt(x: f32, y: f32, z: f32) -> Tilt {
    let magnitude = libm::sqrtf(x * x + y * y + z * z);
    let depression_deg = if magnitude > 0.0 {
        libm::asinf((z / magnitude).clamp(-1.0, 1.0)).to_degrees()
    } else {
        0.0
    };
    Tilt {
        depression_deg,
        roll_deg: libm::atan2f(x, y).to_degrees(),
        magnitude,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn upright_board_looks_level() {
        // Board vertical, +Y up: gravity reads +1 g on Y, nothing on Z.
        let t = tilt(0.0, 1.0, 0.0);
        assert!(close(t.depression_deg, 0.0));
        assert!(close(t.magnitude, 1.0));
    }

    #[test]
    fn sensors_facing_the_floor_look_straight_down() {
        // Optical axis is -Z, so pointing it at the floor puts +Z up.
        assert!(close(tilt(0.0, 0.0, 1.0).depression_deg, 90.0));
    }

    #[test]
    fn thirty_degrees_down() {
        // Tipped forward 30°: up has a +Z component of sin 30°.
        let (s, c) = (0.5f32, libm::sqrtf(3.0) / 2.0);
        assert!(close(tilt(0.0, c, s).depression_deg, 30.0));
    }

    #[test]
    fn this_rig_on_2026_11_22() {
        // The reading that established the sign: about 50° down.
        let t = tilt(200.0, -623.0, 776.0);
        assert!((t.depression_deg - 49.86).abs() < 0.05);
    }

    #[test]
    fn depression_ignores_rotation_in_plane() {
        // Same 30° tip, board turned 90° in its own plane (portrait vs
        // landscape): depression unchanged, roll moves.
        let (s, c) = (0.5f32, libm::sqrtf(3.0) / 2.0);
        let a = tilt(0.0, c, s);
        let b = tilt(c, 0.0, s);
        assert!(close(a.depression_deg, b.depression_deg));
        assert!(close(b.roll_deg - a.roll_deg, 90.0));
    }
}
