//! Read the rig's mount angle from the on-board accelerometer.
//!
//! Averages one second of samples (~104) and prints, once a second, how far
//! the sensors' optical axis points below horizontal, the roll about that
//! axis, and how steady the reading was. Leave the rig untouched while it
//! runs: the spread line says whether it was.
//!
//! Run with: `cargo run --release --bin tilt_check`

#![no_std]
#![no_main]

use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_stm32::i2c::I2c;
use embassy_stm32::time::Hertz;
use embassy_time::{Duration, Timer};

use defmt_rtt as _;
use panic_probe as _;

use coworkos_firmware::clock;
use coworkos_firmware::ism330dhcx::{self, Ism330dhcx, MICRO_G_PER_LSB};

const SAMPLES: usize = 104;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());
    info!("tilt check — ISM330DHCX on I2C2");

    let mut cfg = embassy_stm32::i2c::Config::default();
    cfg.frequency = Hertz::khz(100);
    cfg.timeout = Duration::from_millis(100);
    let mut bus = I2c::new_blocking(p.I2C2, p.PH4, p.PH5, cfg);

    let mut imu = match Ism330dhcx::init(&mut bus) {
        Ok(imu) => imu,
        Err(e) => {
            error!("IMU init failed: {}", e);
            loop {
                Timer::after(Duration::from_secs(60)).await;
            }
        }
    };
    // First samples after power-up are not settled (datasheet turn-on time).
    Timer::after(Duration::from_millis(100)).await;
    info!("averaging {=usize} samples per reading; keep the rig still", SAMPLES);

    loop {
        let mut sum = [0i64; 3];
        let mut min_z = i16::MAX;
        let mut max_z = i16::MIN;
        let mut taken = 0;
        while taken < SAMPLES {
            match imu.accel_ready(&mut bus) {
                Ok(true) => {}
                Ok(false) => {
                    Timer::after(Duration::from_millis(2)).await;
                    continue;
                }
                Err(e) => {
                    error!("status read failed: {}", e);
                    Timer::after(Duration::from_millis(100)).await;
                    continue;
                }
            }
            match imu.read_accel(&mut bus) {
                Ok(a) => {
                    for i in 0..3 {
                        sum[i] += a[i] as i64;
                    }
                    min_z = min_z.min(a[2]);
                    max_z = max_z.max(a[2]);
                    taken += 1;
                }
                Err(e) => error!("accel read failed: {}", e),
            }
        }

        // Mean in milli-g, kept integer for printing; float only for angles.
        let mg = |s: i64| (s * MICRO_G_PER_LSB as i64 / SAMPLES as i64 / 1000) as i32;
        let (x, y, z) = (mg(sum[0]), mg(sum[1]), mg(sum[2]));
        let t = ism330dhcx::tilt(x as f32, y as f32, z as f32);
        let spread_mg = (max_z as i32 - min_z as i32) * MICRO_G_PER_LSB / 1000;

        info!(
            "tilt {=f32} deg below horizontal, roll {=f32} deg, |g| {=f32}  (x {=i32} y {=i32} z {=i32} mg, z spread {=i32} mg)",
            t.depression_deg,
            t.roll_deg,
            t.magnitude / 1000.0,
            x,
            y,
            z,
            spread_mg
        );
    }
}
