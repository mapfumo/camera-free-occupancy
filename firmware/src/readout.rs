//! The on-device readout: what the ILI9341 shows.
//!
//! Gate 4's exit test is one person walking the field of view and reading
//! the result off the device, so the class has to be legible across a room:
//!
//! ```text
//!  ┌──────────────────────────────────────────┐
//!  │                                          │  band, coloured by class
//!  │              S E A T E D                 │  (labeller colours: blue
//!  │                                          │   empty, orange seated,
//!  ├────────────────────────┬─────────────────┤   green present)
//!  │  97%                   │  ▓▓▒▒░░  8×8    │
//!  │  confidence            │  ▓▓▓▒░░  ToF    │  near = dark
//!  │  OCCUPIED              │  ...            │
//!  └────────────────────────┴─────────────────┘
//! ```
//!
//! Every element redraws only when its value changes; the heatmap is the
//! only thing that changes every frame. SPI time per element (5 MHz) is
//! logged by `display_test`.

use core::fmt::Write as _;

use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::{MonoTextStyle, MonoTextStyleBuilder};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::{Baseline, Text};

use crate::ili9341::{Ili9341, Scaled, HEIGHT, WIDTH};
use crate::vl53l5cx;

pub const CLASSES: [&str; 3] = ["EMPTY", "SEATED", "PRESENT"];

const fn rgb(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new(r >> 3, g >> 2, b >> 3)
}

/// Same colours as the labeller's timeline band, so the two read alike.
const CLASS_COLOR: [Rgb565; 3] = [rgb(0x2a, 0x78, 0xd6), rgb(0xeb, 0x68, 0x34), rgb(0x1b, 0xaf, 0x7a)];
const WAITING: Rgb565 = rgb(0x60, 0x60, 0x60);
const BG: Rgb565 = Rgb565::BLACK;
const FG: Rgb565 = Rgb565::WHITE;
const DIM: Rgb565 = rgb(0x90, 0x90, 0x90);

const BAND_H: u32 = 104;
const BIG: u32 = 4; // class word: 40×80 px per character
const MID: u32 = 3; // confidence: 30×60

const MAP_CELL: u32 = 14;
const MAP_X: i32 = 204;
const MAP_Y: i32 = 116;
const D_MAX_MM: i32 = 2000;

pub struct Readout {
    /// Last colour drawn per heatmap cell; only changed cells are redrawn.
    cells: Option<[Rgb565; 64]>,
    class: Option<Option<usize>>,
    pct: Option<Option<u8>>,
    occupied: Option<Option<bool>>,
}

impl Readout {
    pub const fn new() -> Self {
        Self { cells: None, class: None, pct: None, occupied: None }
    }

    /// Blank the screen and draw the fixed labels.
    pub fn clear(&mut self, d: &mut Ili9341<'_>) {
        let _ = d.fill_solid(&Rectangle::new(Point::zero(), Size::new(WIDTH as u32, HEIGHT as u32)), BG);
        let small = text_style(DIM, BG);
        let _ = Text::with_baseline("confidence", Point::new(10, 180), small, Baseline::Top).draw(d);
        *self = Self::new();
    }

    /// The debounced state: `None` until the device has reported one.
    pub fn show_state(&mut self, d: &mut Ili9341<'_>, state: Option<usize>, occupied: Option<bool>) {
        if self.class != Some(state) {
            self.class = Some(state);
            let (word, color) = match state {
                Some(k) => (CLASSES[k], CLASS_COLOR[k]),
                None => ("WAIT", WAITING),
            };
            let band = Rectangle::new(Point::zero(), Size::new(WIDTH as u32, BAND_H));
            let _ = d.fill_solid(&band, color);
            let char_w = 10 * BIG;
            let x = (WIDTH as u32 - word.len() as u32 * char_w) / 2;
            let y = (BAND_H - 20 * BIG) / 2;
            let mut big = Scaled { inner: d, scale: BIG, origin: Point::new(x as i32, y as i32) };
            let _ = Text::with_baseline(word, Point::zero(), text_style(FG, color), Baseline::Top).draw(&mut big);
        }
        if self.occupied != Some(occupied) {
            self.occupied = Some(occupied);
            let (word, color) = match occupied {
                Some(true) => ("OCCUPIED", FG),
                Some(false) => ("VACANT  ", FG),
                None => ("        ", FG),
            };
            let _ = Text::with_baseline(word, Point::new(10, 208), text_style(color, BG), Baseline::Top).draw(d);
        }
    }

    /// Model probability of the reported class, 0.0–1.0, shown as a whole
    /// percentage. Redraws only when the shown number changes.
    pub fn show_confidence(&mut self, d: &mut Ili9341<'_>, p: Option<f32>) {
        let pct = p.map(|p| (p * 100.0 + 0.5).clamp(0.0, 100.0) as u8);
        if self.pct == Some(pct) {
            return;
        }
        self.pct = Some(pct);
        let mut s: heapless_str::Str<8> = heapless_str::Str::new();
        match pct {
            Some(v) => {
                let _ = write!(s, "{:>3}%", v);
            }
            None => {
                let _ = write!(s, "  --");
            }
        }
        let mut mid = Scaled { inner: d, scale: MID, origin: Point::new(4, 112) };
        let _ = Text::with_baseline(s.as_str(), Point::zero(), text_style(FG, BG), Baseline::Top).draw(&mut mid);
    }

    /// The 8×8 ToF distance grid, world frame (row 0 = far edge of the
    /// view). Near is dark, far is pale, no return is grey — as the viewer.
    pub fn show_heatmap(&mut self, d: &mut Ili9341<'_>, distance: &[i16; 64], status: &[u8; 64]) {
        // Only cells whose colour changed are redrawn: a person moves a few
        // cells per frame, so this is a few small windows instead of the
        // whole 112×112 map (25 KB, ~45 ms at 5 MHz) every 200 ms frame.
        let prev = self.cells;
        let mut now = [Rgb565::BLACK; 64];
        for z in 0..64 {
            now[z] = cell_color(distance[z], status[z]);
            if prev.map_or(true, |p| p[z] != now[z]) {
                let x = MAP_X + (z % 8) as i32 * MAP_CELL as i32;
                let y = MAP_Y + (z / 8) as i32 * MAP_CELL as i32;
                let _ = d.fill_solid(&Rectangle::new(Point::new(x, y), Size::new(MAP_CELL, MAP_CELL)), now[z]);
            }
        }
        self.cells = Some(now);
    }
}

fn cell_color(mm: i16, status: u8) -> Rgb565 {
    if !(vl53l5cx::is_valid(status) && mm > 0) {
        return rgb(0x40, 0x40, 0x40);
    }
    // near (0) → dark orange, far (2 m+) → pale
    // 16 steps: millimetre noise must not repaint every cell every frame.
    let t = (mm as i32).clamp(0, D_MAX_MM) / (D_MAX_MM / 16) * (D_MAX_MM / 16);
    let lerp = |a: i32, b: i32| (a + (b - a) * t / D_MAX_MM) as u8;
    rgb(lerp(0x82, 0xf5), lerp(0x28, 0xdc), lerp(0x0a, 0xc8))
}

fn text_style(fg: Rgb565, bg: Rgb565) -> MonoTextStyle<'static, Rgb565> {
    MonoTextStyleBuilder::new().font(&FONT_10X20).text_color(fg).background_color(bg).build()
}

/// A tiny fixed-capacity string for formatting without an allocator.
mod heapless_str {
    pub struct Str<const N: usize> {
        buf: [u8; N],
        len: usize,
    }
    impl<const N: usize> Str<N> {
        pub const fn new() -> Self {
            Self { buf: [0; N], len: 0 }
        }
        pub fn as_str(&self) -> &str {
            core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
        }
    }
    impl<const N: usize> core::fmt::Write for Str<N> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let b = s.as_bytes();
            if self.len + b.len() > N {
                return Err(core::fmt::Error);
            }
            self.buf[self.len..self.len + b.len()].copy_from_slice(b);
            self.len += b.len();
            Ok(())
        }
    }
}
