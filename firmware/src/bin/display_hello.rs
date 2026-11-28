//! Simplest possible display check: draw one static screen and stop.
//!
//! Black background, a white border, "HELLO" in big white letters, a red,
//! green and blue square. Nothing changes after it is drawn, so any
//! flashing or flicker afterwards is the backlight or power, not the SPI.
//!
//! Wiring as `docs/display_wiring.md`. Run with:
//! `cargo run --release --bin display_hello`

#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::spi::{self, Spi};
use embassy_stm32::time::Hertz;
use embassy_time::{Duration, Timer};
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};
use {defmt_rtt as _, panic_probe as _};

use coworkos_firmware::clock;
use coworkos_firmware::ili9341::{Ili9341, Scaled, HEIGHT, WIDTH};

/// Half the bench test's clock, to rule the 40 cm wires out.
const SPI_KHZ: u32 = 2500;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock::config());
    clock::enable_icache();
    info!("display hello — SPI1 at {=u32} kHz", SPI_KHZ);

    let mut cfg = spi::Config::default();
    cfg.frequency = Hertz::khz(SPI_KHZ);
    cfg.gpio_speed = Speed::Low;
    let spi = Spi::new_blocking_txonly(p.SPI1, p.PE13, p.PE15, cfg);
    let cs = Output::new(p.PE12, Level::High, Speed::Low);
    let dc = Output::new(p.PC1, Level::High, Speed::Low);
    let rst = Some(Output::new(p.PA8, Level::High, Speed::Low));

    let mut d = Ili9341::new(spi, cs, dc, rst);
    d.init();

    let screen = Rectangle::new(Point::zero(), Size::new(WIDTH as u32, HEIGHT as u32));
    let _ = d.fill_solid(&screen, Rgb565::BLACK);
    let _ = screen.into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 4)).draw(&mut d);

    let style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb565::WHITE)
        .background_color(Rgb565::BLACK)
        .build();
    // "HELLO" at 5x: 50x100 px per character, 250 px wide, centred.
    let mut big = Scaled { inner: &mut d, scale: 5, origin: Point::new(35, 30) };
    let _ = Text::with_baseline("HELLO", Point::zero(), style, Baseline::Top).draw(&mut big);

    for (i, c) in [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE].into_iter().enumerate() {
        let _ = Rectangle::new(Point::new(70 + i as i32 * 65, 160), Size::new(50, 50))
            .into_styled(PrimitiveStyle::with_fill(c))
            .draw(&mut d);
    }
    info!("drawn: HELLO in white, then red / green / blue squares left to right");

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
