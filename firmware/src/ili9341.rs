//! ILI9341 240×320 TFT over SPI — write-only, just enough for the readout.
//!
//! Hand-written rather than pulled in as a crate: the controller needs a
//! dozen commands, and every one of them is visible here. Wiring and the
//! reasons for it: `docs/display_wiring.md`.
//!
//! - **Write-only.** MISO (SDO) is not connected; nothing is read back.
//! - **Software CS.** CS is an ordinary GPIO, low for each transaction, so a
//!   glitch on the long harness cannot leave the controller mid-command for
//!   longer than one transaction.
//! - **RESET optional.** The carrier may tie RESET high; then a software
//!   reset (0x01) does the same job at start-up.
//! - **Touch panel unused** (CLAUDE.md).
//!
//! Drawing goes through `embedded-graphics`. Filled areas and text with a
//! background colour stream as one window write; only stray single pixels
//! pay the per-pixel window cost.

use embassy_stm32::gpio::Output;
use embassy_stm32::mode::Blocking;
use embassy_stm32::spi::mode::Master;
use embassy_stm32::spi::Spi;
use embassy_time::{block_for, Duration};
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

/// Landscape: the long side across, the header pins on the left.
pub const WIDTH: u16 = 320;
pub const HEIGHT: u16 = 240;

mod cmd {
    pub const SWRESET: u8 = 0x01;
    pub const SLPOUT: u8 = 0x11;
    pub const GAMMASET: u8 = 0x26;
    pub const DISPON: u8 = 0x29;
    pub const CASET: u8 = 0x2A;
    pub const PASET: u8 = 0x2B;
    pub const RAMWR: u8 = 0x2C;
    pub const MADCTL: u8 = 0x36;
    pub const VSCRSADD: u8 = 0x37;
    pub const PIXFMT: u8 = 0x3A;
    pub const FRMCTR1: u8 = 0xB1;
    pub const DFUNCTR: u8 = 0xB6;
    pub const PWCTR1: u8 = 0xC0;
    pub const PWCTR2: u8 = 0xC1;
    pub const VMCTR1: u8 = 0xC5;
    pub const VMCTR2: u8 = 0xC7;
    pub const GMCTRP1: u8 = 0xE0;
    pub const GMCTRN1: u8 = 0xE1;
}

/// MADCTL: MV (row/column exchange → landscape) | BGR (this panel's colour
/// order). If the picture is mirrored once mounted, flip MX (0x40) or MY
/// (0x80) here — nothing else changes.
pub const MADCTL_LANDSCAPE: u8 = 0x20 | 0x08;

/// The widely used power-on sequence for this controller (the same values as
/// Adafruit's and most vendor examples): power, VCOM, frame rate, gamma.
/// Undocumented vendor registers (0xEF, 0xCF, …) are left out; the panel
/// runs without them.
const INIT: &[(u8, &[u8])] = &[
    (cmd::PWCTR1, &[0x23]),
    (cmd::PWCTR2, &[0x10]),
    (cmd::VMCTR1, &[0x3E, 0x28]),
    (cmd::VMCTR2, &[0x86]),
    (cmd::MADCTL, &[MADCTL_LANDSCAPE]),
    (cmd::VSCRSADD, &[0x00]),
    (cmd::PIXFMT, &[0x55]), // 16 bits per pixel, RGB565
    (cmd::FRMCTR1, &[0x00, 0x18]),
    (cmd::DFUNCTR, &[0x08, 0x82, 0x27]),
    (cmd::GAMMASET, &[0x01]),
    (
        cmd::GMCTRP1,
        &[0x0F, 0x31, 0x2B, 0x0C, 0x0E, 0x08, 0x4E, 0xF1, 0x37, 0x07, 0x10, 0x03, 0x0E, 0x09, 0x00],
    ),
    (
        cmd::GMCTRN1,
        &[0x00, 0x0E, 0x14, 0x03, 0x11, 0x07, 0x31, 0xC1, 0x48, 0x08, 0x0F, 0x0C, 0x31, 0x36, 0x0F],
    ),
];

/// Pixels per SPI write when streaming: 128 px = 256 bytes on the stack.
const CHUNK: usize = 128;

pub struct Ili9341<'d> {
    spi: Spi<'d, Blocking, Master>,
    cs: Output<'d>,
    dc: Output<'d>,
    rst: Option<Output<'d>>,
}

impl<'d> Ili9341<'d> {
    /// `cs` and `dc` must start high; `rst` (if wired) high.
    pub fn new(spi: Spi<'d, Blocking, Master>, cs: Output<'d>, dc: Output<'d>, rst: Option<Output<'d>>) -> Self {
        Self { spi, cs, dc, rst }
    }

    /// Reset and configure. Blocks ~300 ms (the controller's own waits).
    pub fn init(&mut self) {
        if let Some(rst) = self.rst.as_mut() {
            rst.set_low();
            block_for(Duration::from_millis(10));
            rst.set_high();
        } else {
            self.command(cmd::SWRESET, &[]);
        }
        block_for(Duration::from_millis(150));
        for (c, data) in INIT {
            self.command(*c, data);
        }
        self.command(cmd::SLPOUT, &[]);
        block_for(Duration::from_millis(120));
        self.command(cmd::DISPON, &[]);
        block_for(Duration::from_millis(20));
    }

    /// One command byte (DC low) then its parameters (DC high), CS low for
    /// the whole transaction.
    fn command(&mut self, c: u8, data: &[u8]) {
        self.cs.set_low();
        self.dc.set_low();
        let _ = self.spi.blocking_write(&[c]);
        if !data.is_empty() {
            self.dc.set_high();
            let _ = self.spi.blocking_write(data);
        }
        self.cs.set_high();
    }

    /// Set the drawing window (inclusive corners) and start RAMWR; leaves CS
    /// low and DC high so pixel data can follow.
    fn begin_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) {
        self.command(cmd::CASET, &[(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8]);
        self.command(cmd::PASET, &[(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8]);
        self.cs.set_low();
        self.dc.set_low();
        let _ = self.spi.blocking_write(&[cmd::RAMWR]);
        self.dc.set_high();
    }

    fn end_window(&mut self) {
        self.cs.set_high();
    }

    /// Stream pixels into an open window, in chunks.
    fn stream<I: Iterator<Item = Rgb565>>(&mut self, pixels: I) {
        let mut buf = [0u8; CHUNK * 2];
        let mut n = 0;
        for c in pixels {
            let v: u16 = RawU16::from(c).into_inner();
            buf[n] = (v >> 8) as u8;
            buf[n + 1] = v as u8;
            n += 2;
            if n == buf.len() {
                let _ = self.spi.blocking_write(&buf);
                n = 0;
            }
        }
        if n > 0 {
            let _ = self.spi.blocking_write(&buf[..n]);
        }
    }

    /// The visible part of `area`, or None if it is entirely off screen.
    fn clip(area: &Rectangle) -> Option<Rectangle> {
        let screen = Rectangle::new(Point::zero(), Size::new(WIDTH as u32, HEIGHT as u32));
        let a = area.intersection(&screen);
        if a.size.width == 0 || a.size.height == 0 {
            None
        } else {
            Some(a)
        }
    }

    fn window_for(&mut self, a: &Rectangle) {
        let br = a.bottom_right().unwrap();
        self.begin_window(a.top_left.x as u16, a.top_left.y as u16, br.x as u16, br.y as u16);
    }
}

impl OriginDimensions for Ili9341<'_> {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}

impl DrawTarget for Ili9341<'_> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            if p.x >= 0 && p.y >= 0 && (p.x as u16) < WIDTH && (p.y as u16) < HEIGHT {
                self.begin_window(p.x as u16, p.y as u16, p.x as u16, p.y as u16);
                self.stream(core::iter::once(c));
                self.end_window();
            }
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        match Self::clip(area) {
            // Fully on screen: one window, one stream.
            Some(a) if a == *area => {
                self.window_for(&a);
                self.stream(colors.into_iter());
                self.end_window();
                Ok(())
            }
            // Partly on screen: fall back to per-pixel for the visible part.
            Some(_) => self.draw_iter(
                area.points()
                    .zip(colors)
                    .map(|(p, c)| Pixel(p, c)),
            ),
            None => Ok(()),
        }
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        if let Some(a) = Self::clip(area) {
            self.window_for(&a);
            self.stream(core::iter::repeat(color).take((a.size.width * a.size.height) as usize));
            self.end_window();
        }
        Ok(())
    }
}

/// Draws everything `scale` times larger: each source pixel becomes a
/// scale×scale block. Lets the 10×20 built-in font read across a room
/// without carrying a large font in flash. Text drawn through this must
/// have a background colour, so glyphs arrive as `fill_contiguous` and are
/// written one scaled row at a time.
pub struct Scaled<'a, 'd> {
    pub inner: &'a mut Ili9341<'d>,
    pub scale: u32,
    pub origin: Point,
}

/// Widest source row a glyph area can have (the built-in fonts are ≤ 10 px;
/// a text run's background box can be wider, so allow a whole line).
const MAX_SRC_ROW: usize = 64;

impl OriginDimensions for Scaled<'_, '_> {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32 / self.scale, HEIGHT as u32 / self.scale)
    }
}

impl DrawTarget for Scaled<'_, '_> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let s = self.scale;
        for Pixel(p, c) in pixels {
            let r = Rectangle::new(self.origin + p * s as i32, Size::new(s, s));
            self.inner.fill_solid(&r, c)?;
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let s = self.scale;
        let w = area.size.width as usize;
        let target = Rectangle::new(
            self.origin + area.top_left * s as i32,
            Size::new(area.size.width * s, area.size.height * s),
        );
        // Off-screen, partly off-screen or too wide to buffer: per pixel.
        if w > MAX_SRC_ROW || Ili9341::clip(&target) != Some(target) {
            return self.draw_iter(area.points().zip(colors).map(|(p, c)| Pixel(p, c)));
        }
        self.inner.window_for(&target);
        let mut row = [Rgb565::BLACK; MAX_SRC_ROW];
        let mut it = colors.into_iter();
        for _ in 0..area.size.height {
            for px in row.iter_mut().take(w) {
                *px = it.next().unwrap_or(Rgb565::BLACK);
            }
            for _ in 0..s {
                let line = row[..w].iter().flat_map(|&c| core::iter::repeat(c).take(s as usize));
                self.inner.stream(line);
            }
        }
        self.inner.end_window();
        Ok(())
    }
}
