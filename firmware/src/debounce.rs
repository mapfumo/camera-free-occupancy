//! Turning per-frame classes into a state a building can act on.
//!
//! Rule: **the reported state changes only after N consecutive frames agree
//! on a new class** (N = 8, 1.6 s at 5 Hz). Nothing is reported until the
//! first class has held N frames.
//!
//! Chosen 2026-11-24 by simulating twelve candidate rules on the model's real
//! out-of-fold prediction streams (`ml/debounce.py`, NOTES "Choosing the
//! debounce rule"): over 2.9 h of steady sessions it makes no false
//! empty<->occupied change and one seated<->present episode, and follows real
//! changes in 1.4 s median, 2.6 s worst. Majority windows and smoothed
//! probabilities did no better; "N in a row" is the easiest to reason about.

pub const FRAMES_TO_CHANGE: u8 = 8;

pub struct Debouncer {
    state: Option<usize>,
    candidate: usize,
    run: u8,
}

impl Debouncer {
    pub const fn new() -> Self {
        Self { state: None, candidate: usize::MAX, run: 0 }
    }

    /// Feed one frame's class. Returns the new state when it changes
    /// (including the first report), otherwise `None`.
    pub fn update(&mut self, class: usize) -> Option<usize> {
        if class == self.candidate {
            self.run = self.run.saturating_add(1);
        } else {
            self.candidate = class;
            self.run = 1;
        }
        if self.run >= FRAMES_TO_CHANGE && self.state != Some(class) {
            self.state = Some(class);
            return self.state;
        }
        None
    }

    /// The current reported state; `None` until the first class has held.
    pub fn state(&self) -> Option<usize> {
        self.state
    }
}
