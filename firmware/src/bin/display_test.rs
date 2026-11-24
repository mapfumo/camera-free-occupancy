//! Bench test for the ILI9341 readout — no sensors, no model.
//!
//! Wiring (label to label, never by counted position — CLAUDE.md, CN13):
//! `docs/display_wiring.md`. In short: SCK D13 (PE13), SDI/MOSI D11 (PE15),
//! CS D10 (PE12), DC D8 (PC1), RESET D9 (PA8) or tied high on the carrier,
//! VCC 5 V, GND, LED tied to VCC on the carrier.
//!
//! What it does, in order:
//!
//! 1. **Colour fills** — red, green, blue, white, black, 1 s each. Wrong
//!    colours (red shows blue) mean the BGR bit in `MADCTL_LANDSCAPE`; a
//!    blank white screen means no data is arriving (SCK/MOSI/CS/DC); a dark
//!    screen means the backlight (LED/VCC).
//! 2. **Orientation** — a corner marker at top-left, text along the top. If
//!    mirrored once mounted, change MX/MY in `MADCTL_LANDSCAPE`.
//! 3. **The readout**, cycling WAIT → EMPTY → SEATED → PRESENT with a fake
//!    moving blob in the heatmap, forever. Each draw is timed and logged, so
//!    the SPI clock can be tuned against the 200 ms frame budget.
//!
//! Start at `SPI_MHZ = 5` with the full harness. Garbled pixels or stray
//! colours at a higher clock are the harness, not the code: go back down.
//!
//! Run with: `cargo run --release --bin display_test`

#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::spi::{self, Spi};
use embassy_stm32::time::Hertz;
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};
use {defmt_rtt as _, panic_probe as _};

use coworkos_firmware::clock;
use coworkos_firmware::ili9341::{Ili9341, HEIGHT, WIDTH};
use coworkos_firmware::readout::Readout;

/// SPI clock. The SPI kernel clock is 160 MHz and the divider is a power of
/// two, so the real clock is 160/2^n MHz and a request rounds *down*: asking
/// for 4 gives 2.5. Usable steps: 5 (start here over the ~30 cm harness),
/// 10, 20. Measured at 2.5 MHz: full-screen fill 499 ms — exactly the bit time.
const SPI_MHZ: u32 = 5;

/// Set false if RESET is tied high on the carrier instead of wired to D9.
const RESET_WIRED: bool = true;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());
    clock::enable_icache();
    info!("display test — ILI9341 on SPI1, {=u32} MHz", SPI_MHZ);

    let mut cfg = spi::Config::default();
    cfg.frequency = Hertz::mhz(SPI_MHZ);
    // Slower edges ring less on a long, unshielded harness; at a few MHz
    // the slowest setting is still far faster than needed.
    cfg.gpio_speed = Speed::Low;
    let spi = Spi::new_blocking_txonly(p.SPI1, p.PE13, p.PE15, cfg);
    let cs = Output::new(p.PE12, Level::High, Speed::Low);
    let dc = Output::new(p.PC1, Level::High, Speed::Low);
    let rst = if RESET_WIRED { Some(Output::new(p.PA8, Level::High, Speed::Low)) } else { None };

    let mut d = Ili9341::new(spi, cs, dc, rst);
    let t = Instant::now();
    d.init();
    info!("init {=u64} ms", t.elapsed().as_millis());

    // ---- 1. colour fills ----
    let screen = Rectangle::new(Point::zero(), Size::new(WIDTH as u32, HEIGHT as u32));
    for (name, c) in [
        ("red", Rgb565::RED),
        ("green", Rgb565::GREEN),
        ("blue", Rgb565::BLUE),
        ("white", Rgb565::WHITE),
        ("black", Rgb565::BLACK),
    ] {
        let t = Instant::now();
        let _ = d.fill_solid(&screen, c);
        info!("fill {=str}: {=u64} ms", name, t.elapsed().as_millis());
        Timer::after(Duration::from_secs(1)).await;
    }

    // ---- 2. orientation ----
    let _ = Rectangle::new(Point::zero(), Size::new(20, 20))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
        .draw(&mut d);
    let _ = Text::with_baseline(
        "top-left is yellow",
        Point::new(28, 0),
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
        Baseline::Top,
    )
    .draw(&mut d);
    info!("orientation marker drawn: yellow square should be top-left, text reading left to right");
    Timer::after(Duration::from_secs(3)).await;

    // ---- 3. the readout ----
    let mut r = Readout::new();
    r.clear(&mut d);
    let states: [(Option<usize>, Option<bool>, Option<f32>); 4] = [
        (None, None, None),
        (Some(0), Some(false), Some(0.99)),
        (Some(1), Some(true), Some(0.93)),
        (Some(2), Some(true), Some(0.81)),
    ];
    let mut frame: u32 = 0;
    loop {
        for (state, occ, conf) in states {
            let t = Instant::now();
            r.show_state(&mut d, state, occ);
            let t_state = t.elapsed().as_micros();
            let t = Instant::now();
            r.show_confidence(&mut d, conf);
            let t_conf = t.elapsed().as_micros();
            info!(
                "state {=str}: band+word {=u64} us, confidence {=u64} us",
                match state {
                    Some(k) => coworkos_firmware::readout::CLASSES[k],
                    None => "WAIT",
                },
                t_state,
                t_conf
            );
            // 3 s per state at the real 5 Hz frame rate, with a blob moving
            // across the heatmap so the per-frame cost is visible.
            for _ in 0..15 {
                let (dist, status) = fake_frame(frame, state);
                let t = Instant::now();
                r.show_heatmap(&mut d, &dist, &status);
                if frame % 25 == 0 {
                    info!("heatmap {=u64} us (budget 200000 per frame)", t.elapsed().as_micros());
                }
                frame += 1;
                Timer::after(Duration::from_millis(200)).await;
            }
        }
    }
}

/// A plausible ToF frame: a far floor, a near blob that drifts with time
/// when someone is "there", and two invalid zones at the far edge.
fn fake_frame(frame: u32, state: Option<usize>) -> ([i16; 64], [u8; 64]) {
    let mut dist = [1700i16; 64];
    let mut status = [5u8; 64];
    status[0] = 255;
    status[7] = 255;
    if matches!(state, Some(1) | Some(2)) {
        let cx = (frame / 3 % 6 + 1) as i32;
        let cy = if state == Some(1) { 5 } else { 3 };
        for y in 0..8i32 {
            for x in 0..8i32 {
                let r2 = (x - cx).pow(2) + (y - cy).pow(2);
                if r2 <= 2 {
                    dist[(y * 8 + x) as usize] = 600 + 150 * r2 as i16;
                }
            }
        }
    }
    (dist, status)
}
