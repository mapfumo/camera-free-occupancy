//! Model input encoding — the firmware twin of `tools/encode.py`.
//!
//! The model was trained on frames encoded by the Python function, so this
//! must produce the same 192 numbers from the same raw frame, or the model is
//! quietly scored on inputs it never saw. Decided 2026-11-23 (NOTES, "Invalid
//! ToF zones: the encoding"); any change here is a change there, and the
//! parity check (`tools/parity_check.py`) is what proves they still agree.
//!
//! Layout is channel-first, row-major in the world frame, exactly as the ONNX
//! model's `{1, 3, 8, 8}` input:
//!
//! | index    | channel          | value                                          |
//! |----------|------------------|------------------------------------------------|
//! | 0..64    | ToF valid mask   | 1.0 if status is 5 or 9 *and* distance > 0     |
//! | 64..128  | ToF distance     | valid: min(d, 2000) / 2000; invalid: 1.0 (far) |
//! | 128..192 | thermal excess   | °C above the frame's median pixel              |

use crate::vl53l5cx;

pub const ZONES: usize = 64;
pub const INPUTS: usize = 3 * ZONES;
pub const D_MAX_MM: f32 = 2000.0;

/// Encode one paired frame. `distance` and `status` are the world-frame ToF
/// grids, `thermal` the AMG8833 grid in centidegrees.
pub fn encode(
    distance: &[i16; ZONES],
    status: &[u8; ZONES],
    thermal: &[i16; ZONES],
    out: &mut [f32; INPUTS],
) {
    for z in 0..ZONES {
        let d = distance[z];
        let valid = vl53l5cx::is_valid(status[z]) && d > 0;
        out[z] = if valid { 1.0 } else { 0.0 };
        out[ZONES + z] = if valid {
            (d as f32).min(D_MAX_MM) / D_MAX_MM
        } else {
            1.0
        };
    }

    // numpy's median of an even count is the mean of the two middle values.
    let mut sorted = *thermal;
    sorted.sort_unstable();
    let median_c = (sorted[ZONES / 2 - 1] as f32 + sorted[ZONES / 2] as f32) / 200.0;
    for z in 0..ZONES {
        out[2 * ZONES + z] = thermal[z] as f32 / 100.0 - median_c;
    }
}
