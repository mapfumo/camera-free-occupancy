//! Yaw alignment: turn the sensor head until the ToF sees the training view.
//!
//! The accelerometer gives tilt and roll (`tilt_check`) but cannot see a turn
//! about the vertical axis. The scene can: the empty room, chair on its
//! marks, looks the same in every reference session. This compares the live
//! ToF view with that reference (`reference.rs`, the consensus of the 10
//! empty-marked sessions) and shows, on the ILI9341, at the pole:
//!
//! - **the number of zones that differ** — large, green at ≤ 13 (every
//!   reference session is within 13 of the consensus), orange ≤ 22, red above;
//! - **an 8×8 map**: green zone matches, red differs, dark both no-return;
//! - **the best column shift** — how many zone columns the live view would
//!   have to move to match best. Turn the head so this heads to 0;
//! - **tilt and roll**, since turning the head can disturb them.
//!
//! A zone differs exactly as in `tools/compare_empty.py`: validity flipped,
//! or both valid and more than 100 mm apart. The live side is a 1 s window
//! (5 frames): valid in ≥ 3, median of the valid distances.
//!
//! Chair on its marks, nobody in view. Run with:
//! `cargo run --release --bin align`

#![no_std]
#![no_main]

mod reference;

use core::fmt::Write as _;

use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::i2c::I2c;
use embassy_stm32::spi::{self, Spi};
use embassy_stm32::time::Hertz;
use embassy_time::{Duration, Timer};
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::{Baseline, Text};
use {defmt_rtt as _, panic_probe as _};

use coworkos_firmware::clock;
use coworkos_firmware::i2c_recovery;
use coworkos_firmware::ili9341::{Ili9341, Scaled, HEIGHT, WIDTH};
use coworkos_firmware::ism330dhcx::{self, Ism330dhcx};
use coworkos_firmware::vl53l5cx::bindings::{
    VL53L5CX_Configuration, VL53L5CX_ResultsData, vl53l5cx_check_data_ready, vl53l5cx_get_ranging_data,
    vl53l5cx_init, vl53l5cx_is_alive, vl53l5cx_set_ranging_frequency_hz, vl53l5cx_set_resolution,
    vl53l5cx_start_ranging,
};
use coworkos_firmware::vl53l5cx::platform::BusContext;
use coworkos_firmware::vl53l5cx::{self, ORIENTATION as TOF_ORIENTATION};

const WINDOW: usize = 5;
const DIST_MM: i32 = 100;
const GOOD: usize = 13;
const OK: usize = 22;

const GREEN: Rgb565 = Rgb565::new(3, 44, 15);
const ORANGE: Rgb565 = Rgb565::new(29, 26, 6);
const RED: Rgb565 = Rgb565::new(28, 8, 6);
const DARK: Rgb565 = Rgb565::new(5, 10, 5);
const DIM: Rgb565 = Rgb565::new(18, 36, 18);

const MAP_CELL: u32 = 22;
const MAP_X: i32 = 320 - 8 * MAP_CELL as i32 - 8;
const MAP_Y: i32 = 50;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());
    clock::enable_icache();
    info!("align — live ToF view vs the empty-room reference");

    // ---- display ----
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
    text(&mut d, 1, Point::new(8, 8), "ALIGN  zones off (<=13 good)", DIM);
    text(&mut d, 2, Point::new(8, 40), "WAIT", DIM);

    // ---- ToF on I2C1 ----
    let _power = Output::new(p.PB0, Level::High, Speed::Low);
    Timer::after(Duration::from_millis(100)).await;
    let (i2c1, mut scl, mut sda) = (p.I2C1, p.PB8, p.PB9);
    let _ = i2c_recovery::recover(scl.reborrow(), sda.reborrow());
    let mut cfg = embassy_stm32::i2c::Config::default();
    cfg.frequency = Hertz::khz(100);
    cfg.timeout = Duration::from_millis(1000);
    let mut bus = BusContext::new(I2c::new_blocking(i2c1, scl, sda, cfg));
    let ctx: *mut BusContext = &mut bus;

    let mut dev: VL53L5CX_Configuration = unsafe { core::mem::zeroed() };
    dev.platform.address = 0x52;
    dev.platform.ctx = ctx as *mut core::ffi::c_void;
    let mut alive = 0u8;
    if unsafe { vl53l5cx_is_alive(&mut dev, &mut alive) } != 0 || alive == 0 {
        fail(&mut d, "VL53L5CX not responding").await;
    }
    if unsafe { vl53l5cx_init(&mut dev) } != 0
        || unsafe { vl53l5cx_set_resolution(&mut dev, 64) } != 0
        || unsafe { vl53l5cx_set_ranging_frequency_hz(&mut dev, 5) } != 0
        || unsafe { vl53l5cx_start_ranging(&mut dev) } != 0
    {
        fail(&mut d, "VL53L5CX init failed").await;
    }

    // ---- IMU on I2C2 (optional) ----
    let mut imu_cfg = embassy_stm32::i2c::Config::default();
    imu_cfg.frequency = Hertz::khz(100);
    imu_cfg.timeout = Duration::from_millis(100);
    let mut imu_bus = I2c::new_blocking(p.I2C2, p.PH4, p.PH5, imu_cfg);
    let mut imu = Ism330dhcx::init(&mut imu_bus).ok();

    let mut results: VL53L5CX_ResultsData = unsafe { core::mem::zeroed() };
    let mut win_d = [[0i16; 64]; WINDOW];
    let mut win_v = [[false; 64]; WINDOW];
    let mut n = 0usize;
    let mut shown: Option<[u8; 64]> = None;

    loop {
        let mut ready = 0u8;
        if unsafe { vl53l5cx_check_data_ready(&mut dev, &mut ready) } != 0 {
            fail(&mut d, "check_data_ready failed").await;
        }
        if ready == 0 {
            Timer::after(Duration::from_millis(5)).await;
            continue;
        }
        if unsafe { vl53l5cx_get_ranging_data(&mut dev, &mut results) } != 0 {
            fail(&mut d, "get_ranging_data failed").await;
        }
        let dist = TOF_ORIENTATION.apply(&results.distance_mm);
        let status = TOF_ORIENTATION.apply(&results.target_status);
        let slot = n % WINDOW;
        for z in 0..64 {
            win_d[slot][z] = dist[z];
            win_v[slot][z] = vl53l5cx::is_valid(status[z]) && dist[z] > 0;
        }
        n += 1;
        if n < WINDOW || n % WINDOW != 0 {
            continue; // one update per second, from a full window
        }

        // Live view over the window.
        let mut valid = [false; 64];
        let mut med = [0i16; 64];
        for z in 0..64 {
            let mut v = [0i16; WINDOW];
            let mut k = 0;
            for w in 0..WINDOW {
                if win_v[w][z] {
                    v[k] = win_d[w][z];
                    k += 1;
                }
            }
            valid[z] = k * 2 > WINDOW;
            if k > 0 {
                v[..k].sort_unstable();
                med[z] = v[k / 2];
            }
        }

        // Zone states: 0 match, 1 differ, 2 both no-return.
        let mut cell = [0u8; 64];
        let mut off = 0usize;
        for z in 0..64 {
            cell[z] = if differs(valid[z], med[z], reference::VALID[z], reference::DIST_MM[z]) {
                off += 1;
                1
            } else if !valid[z] {
                2
            } else {
                0
            };
        }
        let shift = best_shift(&valid, &med);
        let tilt = imu.as_mut().and_then(|i| i.read_accel(&mut imu_bus).ok()).map(|a| {
            let t = ism330dhcx::tilt(a[0] as f32, a[1] as f32, a[2] as f32);
            (t.depression_deg, if t.roll_deg < 0.0 { t.roll_deg + 360.0 } else { t.roll_deg })
        });
        info!("align: {=usize} zones off, best shift {=i32} cols", off, shift);

        // Draw: count, shift, angles; map cells only where changed.
        let color = if off <= GOOD { GREEN } else if off <= OK { ORANGE } else { RED };
        let mut s = Line::new();
        let _ = write!(s, "{:>2}", off);
        text(&mut d, 5, Point::new(8, 40), s.as_str(), color);
        let mut s = Line::new();
        // Left column is 12 characters wide; the map takes the right.
        let _ = write!(s, "shift {:+} ", shift);
        text(&mut d, 1, Point::new(8, 148), s.as_str(), if shift == 0 { GREEN } else { ORANGE });
        let (t, r) = tilt.unwrap_or((0.0, 0.0));
        let near = |v: f32, w: f32| if (v - w).abs() <= 1.0 { GREEN } else { ORANGE };
        let mut s = Line::new();
        let _ = write!(s, "T {:5.1}", t);
        text(&mut d, 1, Point::new(8, 172), s.as_str(), near(t, 50.1));
        let mut s = Line::new();
        let _ = write!(s, "R {:5.1}", r);
        text(&mut d, 1, Point::new(8, 196), s.as_str(), near(r, 161.7));
        text(&mut d, 1, Point::new(8, 220), "50.1/161.7", DIM);
        for z in 0..64 {
            if shown.map_or(true, |p| p[z] != cell[z]) {
                let c = match cell[z] {
                    0 => GREEN,
                    1 => RED,
                    _ => DARK,
                };
                let x = MAP_X + (z % 8) as i32 * MAP_CELL as i32;
                let y = MAP_Y + (z / 8) as i32 * MAP_CELL as i32;
                let _ = d.fill_solid(
                    &Rectangle::new(Point::new(x + 1, y + 1), Size::new(MAP_CELL - 2, MAP_CELL - 2)),
                    c,
                );
            }
        }
        shown = Some(cell);
    }
}

fn differs(v: bool, mm: i16, rv: bool, rmm: i16) -> bool {
    v != rv || (v && (mm as i32 - rmm as i32).abs() > DIST_MM)
}

/// The column shift (−3..=3) of the live view that best matches the
/// reference, by fraction of overlapping zones that agree. 0 = aligned.
fn best_shift(valid: &[bool; 64], med: &[i16; 64]) -> i32 {
    let mut best = (f32::MAX, 0i32);
    for dx in -3i32..=3 {
        let (mut bad, mut all) = (0u32, 0u32);
        for r in 0..8 {
            for c in 0..8i32 {
                let cc = c + dx;
                if !(0..8).contains(&cc) {
                    continue;
                }
                let (zr, zl) = (r * 8 + c as usize, r * 8 + cc as usize);
                all += 1;
                if differs(valid[zl], med[zl], reference::VALID[zr], reference::DIST_MM[zr]) {
                    bad += 1;
                }
            }
        }
        let f = bad as f32 / all as f32;
        // Prefer no shift on a tie, then the smaller one.
        if f < best.0 - 1e-6 {
            best = (f, dx);
        }
    }
    best.1
}

fn text(d: &mut Ili9341<'_>, scale: u32, at: Point, s: &str, color: Rgb565) {
    let style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(color)
        .background_color(Rgb565::BLACK)
        .build();
    if scale == 1 {
        let _ = Text::with_baseline(s, at, style, Baseline::Top).draw(d);
    } else {
        let mut big = Scaled { inner: d, scale, origin: at };
        let _ = Text::with_baseline(s, Point::zero(), style, Baseline::Top).draw(&mut big);
    }
}

async fn fail(d: &mut Ili9341<'_>, msg: &str) -> ! {
    error!("{=str}", msg);
    text(d, 1, Point::new(8, 220), msg, RED);
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

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
