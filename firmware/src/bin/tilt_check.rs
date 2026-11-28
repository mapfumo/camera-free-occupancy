//! Read the rig's mount angle from the on-board accelerometer.
//!
//! Averages one second of samples (~104) and prints, once a second, how far
//! the sensors' optical axis points below horizontal, the roll about that
//! axis, and how steady the reading was. Leave the rig untouched while it
//! runs: the spread line says whether it was.
//!
//! The same reading is drawn on the ILI9341 in large digits, so the mount
//! can be adjusted at the pole without watching a terminal: tilt and roll
//! (roll shown 0–360°, so the training pose reads 161.7, not a sign flip),
//! each green within ±1° of the training sessions' pose, orange otherwise.
//! No display attached: the log is unchanged.
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

use core::fmt::Write as _;

use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::spi::{self, Spi};
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::{Baseline, Text};

use coworkos_firmware::clock;
use coworkos_firmware::ili9341::{Ili9341, Scaled, HEIGHT, WIDTH};
use coworkos_firmware::ism330dhcx::{self, Ism330dhcx, MICRO_G_PER_LSB};

const SAMPLES: usize = 104;

/// The pose every training session was recorded in (NOTES.md; the live
/// trial's `mount tilt` line: 50.07° / 161.70°).
const TARGET_TILT: f32 = 50.1;
const TARGET_ROLL: f32 = 161.7;
const TOLERANCE_DEG: f32 = 1.0;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());
    info!("tilt check — ISM330DHCX on I2C2");

    let mut cfg = embassy_stm32::i2c::Config::default();
    cfg.frequency = Hertz::khz(100);
    cfg.timeout = Duration::from_millis(100);
    let mut bus = I2c::new_blocking(p.I2C2, p.PH4, p.PH5, cfg);

    let mut spi_cfg = spi::Config::default();
    spi_cfg.frequency = Hertz::mhz(5);
    spi_cfg.gpio_speed = Speed::Low;
    let mut d = Ili9341::new(
        Spi::new_blocking_txonly(p.SPI1, p.PE13, p.PE15, spi_cfg),
        Output::new(p.PE12, Level::High, Speed::Low),
        Output::new(p.PC1, Level::High, Speed::Low),
        Some(Output::new(p.PA8, Level::High, Speed::Low)),
    );
    d.init();
    let _ = d.fill_solid(&Rectangle::new(Point::zero(), Size::new(WIDTH as u32, HEIGHT as u32)), Rgb565::BLACK);
    let mut line: Line = Line::new();
    let _ = write!(line, "target {:.1} / {:.1}, +-{:.0}", TARGET_TILT, TARGET_ROLL, TOLERANCE_DEG);
    draw(&mut d, 1, Point::new(10, 200), line.as_str(), Rgb565::new(18, 36, 18));

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

        let roll360 = if t.roll_deg < 0.0 { t.roll_deg + 360.0 } else { t.roll_deg };
        for (i, (name, v, target)) in [("TILT", t.depression_deg, TARGET_TILT), ("ROLL", roll360, TARGET_ROLL)]
            .into_iter()
            .enumerate()
        {
            let ok = (v - target).abs() <= TOLERANCE_DEG;
            let color = if ok { Rgb565::new(3, 44, 15) } else { Rgb565::new(29, 26, 6) };
            let mut s: Line = Line::new();
            let _ = write!(s, "{} {:5.1}", name, v);
            draw(&mut d, 3, Point::new(10, 10 + i as i32 * 90), s.as_str(), color);
        }
    }
}

/// Text at `scale`× the 10×20 font, on black.
fn draw(d: &mut Ili9341<'_>, scale: u32, at: Point, text: &str, color: Rgb565) {
    let style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(color)
        .background_color(Rgb565::BLACK)
        .build();
    let mut big = Scaled { inner: d, scale, origin: at };
    let _ = Text::with_baseline(text, Point::zero(), style, Baseline::Top).draw(&mut big);
}

/// A fixed-capacity line of text for formatting without an allocator.
struct Line {
    buf: [u8; 40],
    len: usize,
}
impl Line {
    const fn new() -> Self {
        Self { buf: [0; 40], len: 0 }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}
impl core::fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        if self.len + b.len() > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[self.len..self.len + b.len()].copy_from_slice(b);
        self.len += b.len();
        Ok(())
    }
}
