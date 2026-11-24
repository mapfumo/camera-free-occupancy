//! Turning per-frame classes into a state a building can act on.
//!
//! Two debouncers, each "N in a row" (N = 8, 1.6 s at 5 Hz):
//!
//! - **occupancy** — empty vs occupied. Any non-empty frame counts as
//!   occupied, so seated/present flips cannot delay it. This is what feeds
//!   the BACnet Binary Input.
//! - **class** — seated vs present, fed only non-empty frames and started
//!   afresh on each visit. Until it has held N frames the reported class is
//!   provisional: the more frequent of the two so far in this visit, `present`
//!   on a tie (a visit starts by walking in).
//!
//! Nothing is reported until occupancy has first held N frames.
//!
//! History, both 2026-11-24 (NOTES "Choosing the debounce rule", "Live
//! scripted test", "Two debouncers"): a single 8-in-a-row over all three
//! classes was chosen first, from twelve rules simulated on the model's
//! out-of-fold streams. The live test then showed its flaw: walking in and
//! sitting straight down flipped present/seated, each flip restarted the
//! count, and *occupied* took 4.2 s. Split in two with the same N, the live
//! replay reports occupied within 3.0 s, with no new false changes offline
//! or live. `ml/debounce.py` is the simulation; this is its `two(8, 8)`.

pub const FRAMES_TO_CHANGE: u8 = 8;

const EMPTY: usize = 0;
const SEATED: usize = 1;
const PRESENT: usize = 2;

pub struct Debouncer {
    /// Debounced occupancy; `None` until it has first held.
    occupied: Option<bool>,
    occ_candidate: bool,
    occ_run: u8,
    /// Debounced class within the current visit; `None` until it has held.
    class: Option<usize>,
    class_candidate: usize,
    class_run: u8,
    /// Seated / present frames in the current visit, for the provisional class.
    seated: u16,
    present: u16,
    /// The last state returned, to report changes only.
    reported: Option<usize>,
}

impl Debouncer {
    pub const fn new() -> Self {
        Self {
            occupied: None,
            occ_candidate: false,
            occ_run: 0,
            class: None,
            class_candidate: usize::MAX,
            class_run: 0,
            seated: 0,
            present: 0,
            reported: None,
        }
    }

    /// Feed one frame's class. Returns the new state when it changes
    /// (including the first report), otherwise `None`.
    pub fn update(&mut self, class: usize) -> Option<usize> {
        let occ = class != EMPTY;

        if occ == self.occ_candidate {
            self.occ_run = self.occ_run.saturating_add(1);
        } else {
            self.occ_candidate = occ;
            self.occ_run = 1;
        }
        if self.occ_run >= FRAMES_TO_CHANGE && self.occupied != Some(occ) {
            self.occupied = Some(occ);
        }

        // A visit is over once occupancy reports empty and this frame agrees:
        // forget its class so the next visit starts clean.
        if self.occupied != Some(true) && !occ {
            self.class = None;
            self.class_candidate = usize::MAX;
            self.class_run = 0;
            self.seated = 0;
            self.present = 0;
        }

        if occ {
            if class == SEATED {
                self.seated = self.seated.saturating_add(1);
            } else {
                self.present = self.present.saturating_add(1);
            }
            if class == self.class_candidate {
                self.class_run = self.class_run.saturating_add(1);
            } else {
                self.class_candidate = class;
                self.class_run = 1;
            }
            if self.class_run >= FRAMES_TO_CHANGE {
                self.class = Some(class);
            }
        }

        let now = match self.occupied {
            None => return None,
            Some(false) => EMPTY,
            Some(true) => self.class.unwrap_or(if self.seated > self.present {
                SEATED
            } else {
                PRESENT
            }),
        };
        if self.reported == Some(now) {
            return None;
        }
        self.reported = Some(now);
        self.reported
    }

    /// The current reported state; `None` until occupancy has first held.
    pub fn state(&self) -> Option<usize> {
        self.reported
    }

    /// Debounced occupancy alone — the BACnet Binary Input.
    pub fn occupied(&self) -> Option<bool> {
        self.occupied
    }
}
