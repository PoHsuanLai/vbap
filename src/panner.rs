//! Core VBAP panner implementation.
//!
//! This module provides the main `VBAPanner` struct that computes
//! speaker gains for a given source position.

use alloc::vec;
use alloc::vec::Vec;

use crate::config::{Bases, PanningMode, SpeakerConfig, SpeakerConfigBuilder};
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

/// Remembers which base was last selected, so a moving source can skip the
/// search when it has not left that base.
///
/// Held by the caller rather than the panner: it keeps [`VBAPanner`] shareable
/// across threads (`&VBAPanner` stays `Sync`), and gives each source its own
/// coherence instead of one shared slot thrashing between sources that are
/// moving in different directions.
///
/// A default or stale cursor is always safe — it only costs a full search.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PanCursor(u32);

/// The speakers that receive signal for one source direction, and their gains.
///
/// At most three entries are populated (two in 2D). Empty means the direction
/// lies outside the region the speakers span, so nothing should be played.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActiveGains {
    gains: [(u32, f64); 3],
    len: u8,
}

impl ActiveGains {
    /// Build from a base's speaker indices and its normalized gain factors.
    #[inline]
    fn new(speakers: &[u32], factors: &[f64]) -> Self {
        let mut out = ActiveGains::default();
        for (&speaker, &gain) in speakers.iter().zip(factors) {
            out.gains[out.len as usize] = (speaker, gain);
            out.len += 1;
        }
        out
    }

    /// Iterate over the `(speaker_index, gain)` pairs that are active.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (u32, f64)> + '_ {
        self.gains[..self.len as usize].iter().copied()
    }

    /// Number of active speakers (0, 2, or 3).
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Whether the direction lies outside the region the speakers span.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Add these gains into an output slice, leaving other entries alone.
    ///
    /// This is the operation a mixer wants: several sources accumulate into one
    /// buffer, so the buffer is cleared once per block rather than once per
    /// source.
    ///
    /// # Panics
    /// Panics only in debug builds if a speaker index is out of range.
    #[inline]
    pub fn accumulate_into(&self, out: &mut [f64]) {
        for (speaker, gain) in self.iter() {
            debug_assert!((speaker as usize) < out.len());
            if let Some(slot) = out.get_mut(speaker as usize) {
                *slot += gain;
            }
        }
    }
}

/// Pick the base whose smallest gain factor is largest, and normalize it.
///
/// Returns the winning index and its normalized factors, or `None` when the
/// direction falls outside every base.
///
/// Two shortcuts keep this off the critical path. The cursor's base is tried
/// first, and a base whose factors are all non-negative wins immediately: active
/// regions do not overlap, so no other base can contain the direction. Both are
/// exact — they select the same base a full scan would.
#[inline]
fn select_base<B, F>(
    bases: &[B],
    cursor: &mut PanCursor,
    mut factors_of: F,
) -> (Option<usize>, [f64; 3])
where
    F: FnMut(&B) -> [f64; 3],
{
    if bases.is_empty() {
        return (None, [0.0; 3]);
    }

    let mut best_idx = 0usize;
    let mut best_min = f64::NEG_INFINITY;
    let mut best_factors = [0.0f64; 3];
    let mut found = false;

    // Try the previously selected base first.
    let start = (cursor.0 as usize).min(bases.len() - 1);
    let order = core::iter::once(start).chain((0..bases.len()).filter(|&i| i != start));

    for idx in order {
        let factors = factors_of(&bases[idx]);
        let min = factors[0].min(factors[1]).min(factors[2]);

        if min > best_min {
            best_min = min;
            best_idx = idx;
            best_factors = factors;
            found = true;
        }

        // All factors non-negative: this base contains the direction, and
        // regions do not overlap, so no later base can beat it.
        if min >= 0.0 {
            break;
        }
    }

    // A clearly negative factor means the direction lies outside every active
    // region. Pulkki (1997) §3: "the virtual source can not be positioned
    // outside the active arc or region." Returning nothing keeps the output
    // silent instead of renormalizing a surviving component up to full level
    // and rendering a phantom the layout cannot produce.
    if !found || best_min < -OUT_OF_ARC_TOLERANCE {
        return (None, [0.0; 3]);
    }

    cursor.0 = best_idx as u32;

    // Clamp the remaining slightly-negative factors *before* normalizing, per
    // Pulkki §1.4: "The negative factor must be set to zero before
    // normalization." These are numerical noise at an arc boundary.
    //
    // The 2D path pads its unused third slot with +inf so it cannot become the
    // minimum above; drop it here so it never reaches the sum of squares.
    for factor in best_factors.iter_mut() {
        *factor = if factor.is_finite() {
            factor.max(0.0)
        } else {
            0.0
        };
    }

    // Normalize so that the sum of squared gains is 1 (Eq. 10/19 with C = 1).
    let sum_sq = best_factors.iter().map(|g| g * g).sum::<f64>();
    let norm = if sum_sq > 1e-10 {
        1.0 / libm::sqrt(sum_sq)
    } else {
        0.0
    };
    for factor in best_factors.iter_mut() {
        *factor *= norm;
    }

    (Some(best_idx), best_factors)
}

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

    /// Compute only the speakers that actually receive signal.
    ///
    /// This is the cheapest way to drive a large layout: it does no work
    /// proportional to the speaker count, where
    /// [`compute_gains_into`](Self::compute_gains_into) must zero the whole
    /// output slice on every call even though at most three entries change.
    ///
    /// `cursor` carries the previously selected base. A moving source usually
    /// stays within the same base from one block to the next, so re-testing it
    /// first turns the search into a single hit in the common case. Give each
    /// concurrent source its own cursor; a stale or default cursor only costs
    /// speed, never correctness.
    ///
    /// ```
    /// use vbap::{PanCursor, VBAPanner};
    ///
    /// let panner = VBAPanner::builder().surround_5_1().build().unwrap();
    /// let mut cursor = PanCursor::default();
    /// let mut mix = vec![0.0; panner.num_speakers()];
    ///
    /// let active = panner.compute_active_gains(45.0, 0.0, &mut cursor);
    /// active.accumulate_into(&mut mix);
    /// ```
    #[inline]
    pub fn compute_active_gains(
        &self,
        azimuth: f64,
        elevation: f64,
        cursor: &mut PanCursor,
    ) -> ActiveGains {
        debug_assert!(
            azimuth.is_finite() && elevation.is_finite(),
            "non-finite direction: azimuth={}, elevation={}",
            azimuth,
            elevation
        );

        let direction = spherical_to_cartesian(azimuth, elevation);

        match &self.config.bases {
            Bases::Two(pairs) => {
                let dir = DVec2::new(direction.x, direction.y);
                // The unused third slot is +inf so it never becomes the minimum.
                let (best, factors) = select_base(pairs, cursor, |pair| {
                    let g = pair.inverse * dir;
                    [g.x, g.y, f64::INFINITY]
                });
                match best {
                    Some(i) => ActiveGains::new(&pairs[i].speakers, &factors[..2]),
                    None => ActiveGains::default(),
                }
            }
            Bases::Three(triplets) => {
                let (best, factors) = select_base(triplets, cursor, |triplet| {
                    let g = triplet.inverse * direction;
                    [g.x, g.y, g.z]
                });
                match best {
                    Some(i) => ActiveGains::new(&triplets[i].speakers, &factors),
                    None => ActiveGains::default(),
                }
            }
        }
    }

    /// Shared implementation. Caller guarantees `gains.len() >= num_speakers()`.
    #[inline]
    fn compute_gains_unchecked(&self, azimuth: f64, elevation: f64, gains: &mut [f64]) {
        gains.fill(0.0);
        let mut cursor = PanCursor::default();
        self.compute_active_gains(azimuth, elevation, &mut cursor)
            .accumulate_into(gains);
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

    // Non-finite input trips a `debug_assert!` by design — it means the caller
    // has a bug. This test pins the *release* contract instead: silence rather
    // than NaN reaching the audio buffer.
    #[test]
    #[cfg(not(debug_assertions))]
    fn test_non_finite_input_yields_silence() {
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
        assert_eq!(panner.config().num_bases(), 1);
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
    fn test_active_gains_match_dense_output() {
        // The sparse and dense paths must agree exactly, including which
        // directions produce nothing at all.
        for panner in [
            VBAPanner::builder().stereo().build().unwrap(),
            VBAPanner::builder().lcr().build().unwrap(),
            VBAPanner::builder().surround_7_1().build().unwrap(),
            VBAPanner::builder().atmos_7_1_4().build().unwrap(),
        ] {
            let n = panner.num_speakers();
            let mut dense = vec![0.0; n];
            let mut sparse = vec![0.0; n];
            let mut cursor = PanCursor::default();

            for step in 0..720 {
                let azimuth = -180.0 + step as f64 * 0.5;
                for elevation in [-30.0, 0.0, 30.0, 60.0] {
                    panner.compute_gains_into(azimuth, elevation, &mut dense);

                    sparse.fill(0.0);
                    let active = panner.compute_active_gains(azimuth, elevation, &mut cursor);
                    active.accumulate_into(&mut sparse);

                    assert_eq!(
                        dense, sparse,
                        "mismatch at azimuth {azimuth}, elevation {elevation}"
                    );
                    assert!(active.len() <= 3);
                }
            }
        }
    }

    #[test]
    fn test_cursor_does_not_change_results() {
        // A stale cursor may only cost time, never correctness.
        let panner = VBAPanner::builder().atmos_7_1_4().build().unwrap();
        let mut fresh = PanCursor::default();
        let mut reused = PanCursor::default();

        // Drive `reused` somewhere unrelated first.
        panner.compute_active_gains(150.0, 40.0, &mut reused);

        for step in 0..360 {
            let azimuth = -180.0 + step as f64;
            let a = panner.compute_active_gains(azimuth, 20.0, &mut PanCursor::default());
            let b = panner.compute_active_gains(azimuth, 20.0, &mut fresh);
            let c = panner.compute_active_gains(azimuth, 20.0, &mut reused);
            assert_eq!(a, b);
            assert_eq!(a, c);
        }
    }

    #[test]
    fn test_accumulate_into_sums_sources() {
        // Several sources mix into one buffer without clearing between them.
        let panner = VBAPanner::builder().surround_5_1().build().unwrap();
        let mut cursor = PanCursor::default();
        let mut mix = vec![0.0; panner.num_speakers()];

        let a = panner.compute_active_gains(30.0, 0.0, &mut cursor);
        let b = panner.compute_active_gains(-30.0, 0.0, &mut cursor);
        a.accumulate_into(&mut mix);
        b.accumulate_into(&mut mix);

        // Both sources sit exactly on a speaker, so each contributes 1.0.
        assert_relative_eq!(mix[0], 1.0, epsilon = 1e-9);
        assert_relative_eq!(mix[1], 1.0, epsilon = 1e-9);
    }

    #[test]
    fn test_panner_is_send_and_sync() {
        // Sharing one panner across voices/threads must keep working.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VBAPanner>();
        assert_send_sync::<PanCursor>();
        assert_send_sync::<ActiveGains>();
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
