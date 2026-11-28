//! Putting both 8×8 grids into one frame of reference: the world's.
//!
//! Each sensor reports its grid in its own order, which depends on its optics
//! and on how its breakout sits on the rig. Neither order means anything
//! downstream. What matters is that every consumer — the RTT print, the CSV,
//! the viewer, the model — sees index `row * 8 + col` as the same patch of the
//! room for *both* sensors. Otherwise Gate 3 concatenates two views of the
//! scene that disagree, which a dense net can partly learn around and a
//! convolutional one cannot.
//!
//! So the correction lives here, applied once, at the driver boundary. It is
//! deliberately not in the viewer: flipping only the display leaves recorded
//! data disagreeing with what was watched while recording it.
//!
//! # The world frame
//!
//! **Standing behind the sensor, looking out along its axis: row 0 is the top
//! of the view, row 7 the bottom; column 0 is on your left, column 7 on your
//! right.**
//!
//! Row 0 at the top is chosen to match how a grid is printed (first line
//! first) and how matplotlib's `imshow` draws by default, so neither the RTT
//! dump nor `tools/heatmap_viewer.py` needs its own correction.
//!
//! # Describing a sensor
//!
//! Any way a square grid can come out is one of eight: a possible transpose
//! (the breakout rotated 90° on the rig), then a possible flip of each axis.
//! An [`Orientation`] records which of those turn the sensor's raw grid into
//! the world frame. Fill it in from bench observation, never from reasoning
//! about the datasheet alone — the datasheet knows the lens, not the mount.
//!
//! No dependencies beyond `core`, on purpose: the crate cannot run `cargo
//! test` on the host (embassy-executor is Cortex-M only), but this file can be
//! tested on its own with `rustc --test`, and later reused by the C/FreeRTOS
//! build as the reference for the same mapping.

/// Zones per side.
pub const SIDE: usize = 8;
/// Zones per grid.
pub const ZONES: usize = SIDE * SIDE;

/// How to turn one sensor's raw grid into the world frame.
///
/// Steps apply in field order: transpose first, then flip rows, then flip
/// columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Orientation {
    /// Swap rows and columns. Needed when the breakout is mounted at 90° to
    /// how its grid reads out, and a top/bottom target moves along a *row*
    /// rather than a column.
    pub transpose: bool,
    /// Reverse the row order: raw row 0 becomes world row 7.
    pub flip_rows: bool,
    /// Reverse the column order: raw column 0 becomes world column 7.
    pub flip_cols: bool,
}

impl Orientation {
    /// The raw order is already the world order.
    pub const IDENTITY: Self = Self { transpose: false, flip_rows: false, flip_cols: false };

    /// Index into the *raw* grid that supplies world cell (`row`, `col`).
    ///
    /// Works backwards through the steps: undo the column flip, undo the row
    /// flip, undo the transpose. Each step is its own inverse, so "undo" is
    /// just "do again".
    pub const fn source_index(self, row: usize, col: usize) -> usize {
        let r = if self.flip_rows { SIDE - 1 - row } else { row };
        let c = if self.flip_cols { SIDE - 1 - col } else { col };
        if self.transpose { c * SIDE + r } else { r * SIDE + c }
    }

    /// Reorder a raw grid into the world frame.
    pub fn apply<T: Copy>(self, raw: &[T; ZONES]) -> [T; ZONES] {
        let mut world = *raw;
        if self == Self::IDENTITY {
            return world;
        }
        for row in 0..SIDE {
            for col in 0..SIDE {
                world[row * SIDE + col] = raw[self.source_index(row, col)];
            }
        }
        world
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid where each cell holds its own raw index, so a reordered grid
    /// shows exactly where each value came from.
    fn numbered() -> [u8; ZONES] {
        core::array::from_fn(|i| i as u8)
    }

    #[test]
    fn identity_changes_nothing() {
        assert_eq!(Orientation::IDENTITY.apply(&numbered()), numbered());
    }

    #[test]
    fn flip_rows_puts_raw_bottom_row_on_top() {
        let o = Orientation { flip_rows: true, ..Orientation::IDENTITY };
        let w = o.apply(&numbered());
        assert_eq!(&w[0..8], &[56, 57, 58, 59, 60, 61, 62, 63]);
        assert_eq!(&w[56..64], &[0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn flip_cols_mirrors_each_row() {
        let o = Orientation { flip_cols: true, ..Orientation::IDENTITY };
        let w = o.apply(&numbered());
        assert_eq!(&w[0..8], &[7, 6, 5, 4, 3, 2, 1, 0]);
    }

    #[test]
    fn transpose_swaps_axes() {
        let o = Orientation { transpose: true, ..Orientation::IDENTITY };
        let w = o.apply(&numbered());
        // World row 0 is raw column 0.
        assert_eq!(&w[0..8], &[0, 8, 16, 24, 32, 40, 48, 56]);
    }

    #[test]
    fn transpose_happens_before_flips() {
        // Transpose then flip rows is a 90° rotation: raw column 7 becomes
        // world row 0.
        let o = Orientation { transpose: true, flip_rows: true, flip_cols: false };
        let w = o.apply(&numbered());
        assert_eq!(&w[0..8], &[7, 15, 23, 31, 39, 47, 55, 63]);
    }

    #[test]
    fn all_eight_orientations_are_distinct_permutations() {
        let mut seen: [[u8; ZONES]; 8] = [[0; ZONES]; 8];
        for bits in 0..8u8 {
            let o = Orientation {
                transpose: bits & 1 != 0,
                flip_rows: bits & 2 != 0,
                flip_cols: bits & 4 != 0,
            };
            let w = o.apply(&numbered());
            // A permutation: every raw index appears exactly once.
            let mut sorted = w;
            sorted.sort_unstable();
            assert_eq!(sorted, numbered());
            for earlier in &seen[..bits as usize] {
                assert_ne!(*earlier, w);
            }
            seen[bits as usize] = w;
        }
    }
}

/// The mount's reference pose, as the on-board IMU reads it (`tilt_check`,
/// `fused_stream`'s `mount tilt` line; roll on 0–360°). `tilt_check` and
/// `align` show green within ±1° of it.
///
/// 45.4° / 167.2° since 2026-11-28 (NOTES, "Display on the panel…"): the
/// head moved during the display work and was kept where the model was
/// re-checked live. Every training session was recorded at 50.1° / 161.7°.
pub const MOUNT_TILT_DEG: f32 = 45.4;
pub const MOUNT_ROLL_DEG: f32 = 167.2;
