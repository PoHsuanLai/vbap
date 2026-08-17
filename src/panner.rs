//! Core VBAP panner implementation.
//!
//! This module provides the main `VBAPanner` struct that computes
//! speaker gains for a given source position.

use alloc::vec;
use alloc::vec::Vec;

use crate::config::{InverseMatrix, PanningMode, SpeakerConfig, SpeakerConfigBuilder};
use crate::error::PanError;
use crate::math::spherical_to_cartesian;
use crate::speaker::Speaker;
use glam::DVec2;

/// How negative a gain factor may be before the direction counts as lying
/// outside every active region.
///
/// Pulkki (1997) §1.4 notes that limited numerical accuracy "may produce
/// slightly negative gain factors in some cases"; those are clamped to zero. A
/// factor more negative than this is not noise — it means the direction is
/// genuinely outside the arc, which only happens on layouts that do not
/// surround the listener.
const OUT_OF_ARC_TOLERANCE: f64 = 1e-6;

/// Vector Base Amplitude Panner.
///
/// Computes speaker gains for positioning sound sources in a multichannel
/// speaker setup using the VBAP algorithm.
///
/// # Example
///
/// ```
/// use vbap::VBAPanner;
///
/// let panner = VBAPanner::builder()
///     .stereo()
///     .build()
///     .unwrap();
///
/// // Pan a source 15 degrees to the left (alloc-free).
/// let mut gains = vec![0.0; panner.num_speakers()];
/// panner.compute_gains_into(15.0, 0.0, &mut gains);
/// assert_eq!(gains.len(), 2);
/// ```
#[derive(Clone, Debug)]
pub struct VBAPanner {
    config: SpeakerConfig,
}

impl VBAPanner {
    /// Create a new panner builder.
    ///
    /// This is the recommended way to construct a VBAPanner.
    pub fn builder() -> SpeakerConfigBuilder {
        SpeakerConfigBuilder::new()
    }

    /// Create a panner from an existing speaker configuration.
    pub fn new(config: SpeakerConfig) -> Self {
        Self { config }
    }

    /// Compute speaker gains for a source at the given position.
    ///
    /// Allocates a fresh `Vec<f64>` on every call; not suitable for
    /// real-time audio threads. Prefer [`VBAPanner::compute_gains_into`]
    /// with a pre-allocated slice on hot paths.
    ///
    /// # Arguments
    /// * `azimuth` - Horizontal angle in degrees (0° = front, 90° = left, -90° = right)
    /// * `elevation` - Vertical angle in degrees (0° = horizontal, 90° = above)
    ///
    /// # Returns
    /// A vector of gains, one per speaker. Gains are normalized so that
    /// the sum of squared gains equals 1.0. Most gains will be 0.0,
    /// with only 2-3 speakers active (depending on 2D/3D mode).
    #[deprecated(
        since = "0.1.2",
        note = "allocates per call; use `compute_gains_into` with a pre-allocated slice"
    )]
    pub fn compute_gains(&self, azimuth: f64, elevation: f64) -> Vec<f64> {
        let mut gains = vec![0.0; self.config.num_speakers()];
        self.compute_gains_into(azimuth, elevation, &mut gains);
        gains
    }

    /// Compute speaker gains into a pre-allocated slice.
    ///
    /// This avoids allocation when called repeatedly, and is the intended entry
    /// point for audio threads: it allocates nothing, takes no locks, and — in
    /// release builds — cannot panic.
    ///
    /// # Real-time contract
    ///
    /// `gains.len()` must be at least [`num_speakers()`](Self::num_speakers).
    /// This is checked by a `debug_assert!`, so an undersized slice panics in
    /// debug builds and is a no-op in release. Prefer
    /// [`try_compute_gains_into`](Self::try_compute_gains_into) when the length
    /// is not statically known — unwinding out of an audio callback is
    /// undefined behaviour under most host ABIs.
    ///
    /// # Output
    ///
    /// Gains are normalized so that `Σg² = 1` — Pulkki's Eq. (10)/(19) with the
    /// paper's volume parameter `C` fixed at 1.0. Scale the result by `√C` for a
    /// different level. Only 2 (2D) or 3 (3D) gains are non-zero.
    ///
    /// Directions outside the region the speakers span yield all-zero gains, per
    /// Pulkki §3; see the crate-level docs on coverage.
    ///
    /// # Non-finite input
    ///
    /// A NaN or infinite `azimuth`/`elevation` yields all-zero gains rather than
    /// propagating NaN into the output. This is guaranteed, not incidental.
    /// Debug builds assert on non-finite input, since it indicates a caller bug.
    #[inline]
    pub fn compute_gains_into(&self, azimuth: f64, elevation: f64, gains: &mut [f64]) {
        debug_assert!(
            gains.len() >= self.config.num_speakers(),
            "gains slice too small: {} < {}",
            gains.len(),
            self.config.num_speakers()
        );
        debug_assert!(
            azimuth.is_finite() && elevation.is_finite(),
            "non-finite direction: azimuth={}, elevation={}",
            azimuth,
            elevation
        );

        if gains.len() < self.config.num_speakers() {
            return;
        }

        self.compute_gains_unchecked(azimuth, elevation, gains);
    }

    /// Compute speaker gains into a pre-allocated slice, reporting a slice that
    /// is too small instead of ignoring it.
    ///
    /// Identical to [`compute_gains_into`](Self::compute_gains_into) on success.
    /// The error type is `Copy` and carries no allocation, so this is safe to
    /// call from an audio thread.
    ///
    /// On error the contents of `gains` are left untouched: a partial write
    /// would silently drop a speaker's gain and produce a wrong mix.
    #[inline]
    pub fn try_compute_gains_into(
        &self,
        azimuth: f64,
        elevation: f64,
        gains: &mut [f64],
    ) -> core::result::Result<(), PanError> {
        let need = self.config.num_speakers();
        if gains.len() < need {
            return Err(PanError::BufferTooSmall {
                need: need as u32,
                got: gains.len() as u32,
            });
        }

        self.compute_gains_unchecked(azimuth, elevation, gains);
        Ok(())
    }

    /// Shared implementation. Caller guarantees `gains.len() >= num_speakers()`.
    #[inline]
    fn compute_gains_unchecked(&self, azimuth: f64, elevation: f64, gains: &mut [f64]) {
        // Zero out all gains
        gains.fill(0.0);

        let tuples = self.config.tuples();
        if tuples.is_empty() {
            return;
        }

        // Convert source direction to Cartesian
        let direction = spherical_to_cartesian(azimuth, elevation);

        // Find the best tuple (highest minimum gain)
        let mut best_tuple_idx = 0;
        let mut best_min_gain = f64::NEG_INFINITY;
        let mut best_gains = [0.0f64; 3];
        let mut best_len = 0usize;

        for (tuple_idx, tuple) in tuples.iter().enumerate() {
            // Compute candidate gains by multiplying direction with inverse matrix
            let (candidate_gains, len) = match tuple.inverse_matrix {
                InverseMatrix::ThreeD(mat) => {
                    let result = mat * direction;
                    ([result.x, result.y, result.z], 3)
                }
                InverseMatrix::TwoD(mat) => {
                    let dir_2d = DVec2::new(direction.x, direction.y);
                    let result = mat * dir_2d;
                    ([result.x, result.y, 0.0], 2)
                }
            };

            // Find minimum gain - we want the tuple where all gains are positive
            let min_gain = candidate_gains[..len]
                .iter()
                .copied()
                .reduce(f64::min)
                .unwrap_or(f64::NEG_INFINITY);

            if min_gain > best_min_gain {
                best_min_gain = min_gain;
                best_tuple_idx = tuple_idx;
                best_gains = candidate_gains;
                best_len = len;
            }
        }

        // A clearly negative factor means the direction lies outside every
        // active region: no combination of these speakers points there. Pulkki
        // (1997) §3: "the virtual source can not be positioned outside the
        // active arc or region." Leave the gains at zero rather than clamping
        // and renormalizing, which would rescale the surviving component back to
        // full level and produce a phantom the layout cannot actually render.
        //
        // Only closed layouts cover every direction; open ones (stereo, LCR, a
        // frontal array) fall outside behind the listener.
        if best_min_gain < -OUT_OF_ARC_TOLERANCE {
            return;
        }

        // Apply the winning gains
        let best_tuple = &tuples[best_tuple_idx];

        // Clamp the remaining slightly-negative factors to zero *before*
        // normalizing. Pulkki (1997) §1.4: "The negative factor must be set to
        // zero before normalization." These are numerical noise at an arc
        // boundary, so this is a no-op everywhere except within a hair of an
        // edge, where it keeps the surviving gains at full level.
        for gain in best_gains[..best_len].iter_mut() {
            *gain = gain.max(0.0);
        }

        // Normalize gains: sqrt(sum of squares) = 1
        let sum_sq: f64 = best_gains[..best_len].iter().map(|g| g * g).sum();
        let norm = if sum_sq > 1e-10 {
            1.0 / libm::sqrt(sum_sq)
        } else {
            0.0
        };

        for (&speaker_idx, &gain) in best_tuple
            .speaker_indices
            .iter()
            .zip(&best_gains[..best_len])
        {
            gains[speaker_idx] = gain * norm;
        }
    }

    /// Get the number of speakers in this configuration.
    #[inline]
    pub fn num_speakers(&self) -> usize {
        self.config.num_speakers()
    }

    /// Get the panning mode (2D or 3D).
    #[inline]
    pub fn mode(&self) -> PanningMode {
        self.config.mode()
    }

    /// Get all speakers in the configuration.
    #[inline]
    pub fn speakers(&self) -> &[Speaker] {
        self.config.speakers()
    }

    /// Get the underlying speaker configuration.
    #[inline]
    pub fn config(&self) -> &SpeakerConfig {
        &self.config
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_stereo_center() {
        let panner = VBAPanner::builder().stereo().build().unwrap();
        let gains = panner.compute_gains(0.0, 0.0);

        assert_eq!(gains.len(), 2);
        // Center pan should have equal gains
        assert_relative_eq!(gains[0], gains[1], epsilon = 0.01);
    }

    #[test]
    fn test_stereo_hard_left() {
        let panner = VBAPanner::builder().stereo().build().unwrap();
        // Stereo is at ±30°, so 30° should be hard left
        let gains = panner.compute_gains(30.0, 0.0);

        assert_eq!(gains.len(), 2);
        // Left speaker (index 0) should be louder
        assert!(gains[0] > gains[1]);
    }

    #[test]
    fn test_stereo_hard_right() {
        let panner = VBAPanner::builder().stereo().build().unwrap();
        let gains = panner.compute_gains(-30.0, 0.0);

        assert_eq!(gains.len(), 2);
        // Right speaker (index 1) should be louder
        assert!(gains[1] > gains[0]);
    }

    #[test]
    fn test_gains_normalized() {
        let panner = VBAPanner::builder().surround_5_1().build().unwrap();

        for azi in [-180, -90, -45, 0, 45, 90, 180] {
            let gains = panner.compute_gains(azi as f64, 0.0);
            let sum_sq: f64 = gains.iter().map(|g| g * g).sum();
            assert_relative_eq!(sum_sq, 1.0, epsilon = 0.01);
        }
    }

    #[test]
    fn test_gains_non_negative() {
        let panner = VBAPanner::builder().surround_7_1().build().unwrap();

        for azi in (-180..=180).step_by(15) {
            let gains = panner.compute_gains(azi as f64, 0.0);
            for g in &gains {
                assert!(*g >= 0.0, "gain {} at azi {} is negative", g, azi);
            }
        }
    }

    #[test]
    fn test_compute_gains_into() {
        let panner = VBAPanner::builder().stereo().build().unwrap();
        let mut gains = vec![0.0; 2];

        panner.compute_gains_into(15.0, 0.0, &mut gains);

        let sum_sq: f64 = gains.iter().map(|g| g * g).sum();
        assert_relative_eq!(sum_sq, 1.0, epsilon = 0.01);
    }

    #[test]
    fn test_3d_panning() {
        let panner = VBAPanner::builder().atmos_7_1_4().build().unwrap();

        assert_eq!(panner.mode(), PanningMode::ThreeD);
        assert_eq!(panner.num_speakers(), 11);

        // Elevated source should activate height speakers
        let gains = panner.compute_gains(45.0, 45.0);

        // At least one non-zero gain
        assert!(gains.iter().any(|&g| g > 0.0));
    }

    #[test]
    fn test_angle_wraparound() {
        let panner = VBAPanner::builder().surround_5_1().build().unwrap();

        // 450° should produce same gains as 90°
        let gains_90 = panner.compute_gains(90.0, 0.0);
        let gains_450 = panner.compute_gains(450.0, 0.0);
        for (a, b) in gains_90.iter().zip(gains_450.iter()) {
            assert_relative_eq!(a, b, epsilon = 1e-9);
        }
    }

    #[test]
    fn test_extreme_elevation() {
        let panner = VBAPanner::builder().atmos_7_1_4().build().unwrap();

        // Directly above — should still produce valid normalized gains
        let gains = panner.compute_gains(0.0, 90.0);
        assert!(gains.iter().all(|&g| g >= 0.0));

        let sum_sq: f64 = gains.iter().map(|g| g * g).sum();
        assert_relative_eq!(sum_sq, 1.0, epsilon = 0.01);
    }

    #[test]
    fn test_minimum_2d_speakers() {
        // Exactly 2 speakers — minimum for 2D
        let panner = VBAPanner::builder()
            .add_speaker(45.0, 0.0)
            .add_speaker(-45.0, 0.0)
            .build()
            .unwrap();

        assert_eq!(panner.num_speakers(), 2);
        let gains = panner.compute_gains(0.0, 0.0);
        assert_relative_eq!(gains[0], gains[1], epsilon = 0.01);
    }

    #[test]
    fn test_minimum_3d_speakers() {
        // Exactly 3 speakers with elevation — minimum for 3D
        let panner = VBAPanner::builder()
            .add_speaker(0.0, 0.0)
            .add_speaker(120.0, 0.0)
            .add_speaker(0.0, 90.0)
            .build()
            .unwrap();

        assert_eq!(panner.num_speakers(), 3);
        assert_eq!(panner.mode(), PanningMode::ThreeD);
        let gains = panner.compute_gains(0.0, 45.0);
        assert!(gains.iter().any(|&g| g > 0.0));
    }

    #[test]
    fn test_try_compute_gains_into_rejects_small_slice() {
        let panner = VBAPanner::builder().surround_5_1().build().unwrap();
        let mut gains = [0.0; 3];

        let err = panner
            .try_compute_gains_into(0.0, 0.0, &mut gains)
            .unwrap_err();
        assert_eq!(err, PanError::BufferTooSmall { need: 5, got: 3 });

        // The buffer must be left untouched rather than partially written.
        assert_eq!(gains, [0.0; 3]);
    }

    #[test]
    fn test_try_compute_gains_into_matches_infallible() {
        let panner = VBAPanner::builder().surround_5_1().build().unwrap();
        let mut a = vec![0.0; 5];
        let mut b = vec![0.0; 5];

        for azi in [-120.0, -30.0, 0.0, 45.0, 170.0] {
            panner.compute_gains_into(azi, 0.0, &mut a);
            panner.try_compute_gains_into(azi, 0.0, &mut b).unwrap();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn test_non_finite_input_yields_silence() {
        // Guaranteed behaviour: never leak NaN into the audio buffer.
        let panner = VBAPanner::builder().surround_5_1().build().unwrap();
        let mut gains = vec![0.0; 5];

        for (azi, ele) in [
            (f64::NAN, 0.0),
            (0.0, f64::NAN),
            (f64::NAN, f64::NAN),
            (f64::INFINITY, 0.0),
            (f64::NEG_INFINITY, 0.0),
            (0.0, f64::INFINITY),
        ] {
            gains.fill(1.0);
            panner.try_compute_gains_into(azi, ele, &mut gains).unwrap();
            assert!(
                gains.iter().all(|g| *g == 0.0),
                "non-finite input ({azi}, {ele}) produced {gains:?}"
            );
        }
    }

    #[test]
    fn test_center_speaker_audible_dead_ahead() {
        // Regression: the centre channel used to be orphaned by triplet
        // selection, so dialogue panned straight ahead came out of the left
        // speaker instead.
        let panner = VBAPanner::builder().atmos_7_1_4().build().unwrap();
        let mut gains = vec![0.0; panner.num_speakers()];
        panner.compute_gains_into(0.0, 0.0, &mut gains);

        assert_relative_eq!(gains[2], 1.0, epsilon = 1e-9);
        for (i, g) in gains.iter().enumerate() {
            if i != 2 {
                assert_relative_eq!(*g, 0.0, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn test_property_one_all_presets() {
        // Pulkki §3, property 1: a source in the same direction as a speaker
        // emanates from that speaker alone.
        let configs: [(&str, VBAPanner); 6] = [
            ("stereo", VBAPanner::builder().stereo().build().unwrap()),
            ("lcr", VBAPanner::builder().lcr().build().unwrap()),
            ("5.1", VBAPanner::builder().surround_5_1().build().unwrap()),
            ("7.1", VBAPanner::builder().surround_7_1().build().unwrap()),
            (
                "atmos_7_1_4",
                VBAPanner::builder().atmos_7_1_4().build().unwrap(),
            ),
            (
                "atmos_5_1_4",
                VBAPanner::builder().atmos_5_1_4().build().unwrap(),
            ),
        ];

        for (name, panner) in configs {
            let mut gains = vec![0.0; panner.num_speakers()];
            for (i, speaker) in panner.speakers().iter().enumerate() {
                panner.compute_gains_into(speaker.azimuth(), speaker.elevation(), &mut gains);
                assert_relative_eq!(gains[i], 1.0, epsilon = 1e-9);
                let leaked: f64 = gains
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| *k != i)
                    .map(|(_, g)| g.abs())
                    .sum();
                assert!(leaked < 1e-9, "{name}: speaker {i} leaked {leaked}");
            }
        }
    }

    #[test]
    fn test_open_layout_is_silent_outside_arc() {
        // Pulkki §3: a source cannot be positioned outside the active region.
        // An LCR rig spans -30..30 only, so the rear must be silent rather than
        // rendering a full-level phantom.
        let panner = VBAPanner::builder().lcr().build().unwrap();
        let mut gains = vec![0.0; 3];

        for azimuth in [-180.0, -120.0, -90.0, -45.0, -31.0, 31.0, 90.0, 150.0] {
            panner.compute_gains_into(azimuth, 0.0, &mut gains);
            assert!(
                gains.iter().all(|g| *g == 0.0),
                "azimuth {azimuth} should be outside the arc, got {gains:?}"
            );
        }

        // ...while everything inside the arc keeps unit power.
        let mut azimuth = -30.0;
        while azimuth <= 30.0 {
            panner.compute_gains_into(azimuth, 0.0, &mut gains);
            let sum_sq: f64 = gains.iter().map(|g| g * g).sum();
            assert_relative_eq!(sum_sq, 1.0, epsilon = 1e-9);
            azimuth += 0.5;
        }
    }

    #[test]
    fn test_stereo_forms_single_pair() {
        // Two speakers describe one arc; the modular wrap used to emit it twice.
        let panner = VBAPanner::builder().stereo().build().unwrap();
        assert_eq!(panner.config().tuples().len(), 1);
    }

    #[test]
    fn test_closed_layouts_cover_every_direction() {
        // A layout that surrounds the listener must render every azimuth.
        for panner in [
            VBAPanner::builder().surround_5_1().build().unwrap(),
            VBAPanner::builder().surround_7_1().build().unwrap(),
            VBAPanner::builder().quad().build().unwrap(),
            VBAPanner::builder().octagon().build().unwrap(),
        ] {
            let mut gains = vec![0.0; panner.num_speakers()];
            let mut azimuth = -180.0;
            while azimuth < 180.0 {
                panner.compute_gains_into(azimuth, 0.0, &mut gains);
                let sum_sq: f64 = gains.iter().map(|g| g * g).sum();
                assert_relative_eq!(sum_sq, 1.0, epsilon = 1e-9);
                azimuth += 0.5;
            }
        }
    }

    #[test]
    fn test_gain_continuity_2d() {
        // No audible jumps as a source sweeps through the covered region.
        let panner = VBAPanner::builder().lcr().build().unwrap();
        let mut prev = vec![0.0; 3];
        let mut cur = vec![0.0; 3];

        panner.compute_gains_into(-30.0, 0.0, &mut prev);
        let mut azimuth = -30.0;
        while azimuth <= 30.0 {
            panner.compute_gains_into(azimuth, 0.0, &mut cur);
            let delta: f64 = prev
                .iter()
                .zip(cur.iter())
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f64>()
                .sqrt();
            assert!(delta < 0.05, "jump of {delta} at azimuth {azimuth}");
            core::mem::swap(&mut prev, &mut cur);
            azimuth += 0.25;
        }
    }

    #[test]
    fn test_duplicate_speakers_error() {
        // Two speakers at the same position should fail to form valid pairs
        let result = VBAPanner::builder()
            .add_speaker(30.0, 0.0)
            .add_speaker(30.0, 0.0)
            .build();

        assert!(result.is_err());
    }
}
