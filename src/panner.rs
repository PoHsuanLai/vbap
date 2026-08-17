//! Core VBAP panner implementation.
//!
//! This module provides the main `VBAPanner` struct that computes
//! speaker gains for a given source position.

use crate::config::{Bases, PanningMode, SpeakerConfig, SpeakerConfigBuilder};
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
/// use vbap::{PanCursor, VBAPanner};
///
/// let panner = VBAPanner::builder()
///     .stereo()
///     .build()
///     .unwrap();
///
/// // Pan a source 15 degrees to the left.
/// let mut cursor = PanCursor::default();
/// let active = panner.compute_gains(15.0, 0.0, &mut cursor);
/// assert_eq!(active.len(), 2);
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

    /// Compute the gains for a source at the given direction.
    ///
    /// Returns only the two or three speakers that receive signal. Nothing here
    /// scales with the speaker count, nothing allocates, nothing locks, and in
    /// release builds nothing panics — so this is equally suited to an audio
    /// callback and to offline rendering.
    ///
    /// `cursor` carries the previously selected base. A moving source usually
    /// stays within the same base from one block to the next, so re-testing it
    /// first turns the search into a single hit in the common case. Give each
    /// concurrent source its own cursor; a stale or default cursor only costs
    /// speed, never correctness.
    ///
    /// # Output
    ///
    /// Gains are normalized so that `Σg² = 1` — Pulkki's Eq. (10)/(19) with the
    /// paper's volume parameter `C` fixed at 1.0. Scale by `√C` for a different
    /// level.
    ///
    /// The result is empty when the direction lies outside the region the
    /// speakers span, per Pulkki §3; see the crate-level docs on coverage. It is
    /// also empty for a NaN or infinite direction, which is guaranteed rather
    /// than incidental, so `NaN` can never reach an audio buffer. Debug builds
    /// assert on non-finite input, since it indicates a caller bug.
    ///
    /// Use [`ActiveGains::accumulate_into`] to sum into a channel-indexed
    /// buffer.
    ///
    /// ```
    /// use vbap::{PanCursor, VBAPanner};
    ///
    /// let panner = VBAPanner::builder().surround_5_1().build().unwrap();
    /// let mut cursor = PanCursor::default();
    /// let mut mix = vec![0.0; panner.num_speakers()];
    ///
    /// let active = panner.compute_gains(45.0, 0.0, &mut cursor);
    /// active.accumulate_into(&mut mix);
    /// ```
    #[inline]
    pub fn compute_gains(
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

        // A non-finite direction would make every comparison in `select_base`
        // false, leaving the first base selected with meaningless factors.
        // Reject it once here rather than per candidate.
        if !azimuth.is_finite() || !elevation.is_finite() {
            return ActiveGains::default();
        }

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
    use alloc::vec;
    use alloc::vec::Vec;

    /// Scatter the active gains into a dense per-speaker vector.
    ///
    /// Tests assert over whole layouts, so the dense form is the convenient
    /// shape here even though callers rarely need it.
    fn dense(panner: &VBAPanner, azimuth: f64, elevation: f64) -> Vec<f64> {
        let mut gains = vec![0.0; panner.num_speakers()];
        let mut cursor = PanCursor::default();
        panner
            .compute_gains(azimuth, elevation, &mut cursor)
            .accumulate_into(&mut gains);
        gains
    }
    use approx::assert_relative_eq;

    #[test]
    fn test_stereo_center() {
        let panner = VBAPanner::builder().stereo().build().unwrap();
        let gains = dense(&panner, 0.0, 0.0);

        assert_eq!(gains.len(), 2);
        // Center pan should have equal gains
        assert_relative_eq!(gains[0], gains[1], epsilon = 0.01);
    }

    #[test]
    fn test_stereo_hard_left() {
        let panner = VBAPanner::builder().stereo().build().unwrap();
        // Stereo is at ±30°, so 30° should be hard left
        let gains = dense(&panner, 30.0, 0.0);

        assert_eq!(gains.len(), 2);
        // Left speaker (index 0) should be louder
        assert!(gains[0] > gains[1]);
    }

    #[test]
    fn test_stereo_hard_right() {
        let panner = VBAPanner::builder().stereo().build().unwrap();
        let gains = dense(&panner, -30.0, 0.0);

        assert_eq!(gains.len(), 2);
        // Right speaker (index 1) should be louder
        assert!(gains[1] > gains[0]);
    }

    #[test]
    fn test_gains_normalized() {
        let panner = VBAPanner::builder().surround_5_1().build().unwrap();

        for azi in [-180, -90, -45, 0, 45, 90, 180] {
            let gains = dense(&panner, azi as f64, 0.0);
            let sum_sq: f64 = gains.iter().map(|g| g * g).sum();
            assert_relative_eq!(sum_sq, 1.0, epsilon = 0.01);
        }
    }

    #[test]
    fn test_gains_non_negative() {
        let panner = VBAPanner::builder().surround_7_1().build().unwrap();

        for azi in (-180..=180).step_by(15) {
            let gains = dense(&panner, azi as f64, 0.0);
            for g in &gains {
                assert!(*g >= 0.0, "gain {} at azi {} is negative", g, azi);
            }
        }
    }

    #[test]
    fn test_gains_are_power_normalized() {
        let panner = VBAPanner::builder().stereo().build().unwrap();
        let gains = dense(&panner, 15.0, 0.0);

        let sum_sq: f64 = gains.iter().map(|g| g * g).sum();
        assert_relative_eq!(sum_sq, 1.0, epsilon = 0.01);
    }

    #[test]
    fn test_3d_panning() {
        let panner = VBAPanner::builder().atmos_7_1_4().build().unwrap();

        assert_eq!(panner.mode(), PanningMode::ThreeD);
        assert_eq!(panner.num_speakers(), 11);

        // Elevated source should activate height speakers
        let gains = dense(&panner, 45.0, 45.0);

        // At least one non-zero gain
        assert!(gains.iter().any(|&g| g > 0.0));
    }

    #[test]
    fn test_angle_wraparound() {
        let panner = VBAPanner::builder().surround_5_1().build().unwrap();

        // 450° should produce same gains as 90°
        let gains_90 = dense(&panner, 90.0, 0.0);
        let gains_450 = dense(&panner, 450.0, 0.0);
        for (a, b) in gains_90.iter().zip(gains_450.iter()) {
            assert_relative_eq!(a, b, epsilon = 1e-9);
        }
    }

    #[test]
    fn test_extreme_elevation() {
        let panner = VBAPanner::builder().atmos_7_1_4().build().unwrap();

        // Directly above — should still produce valid normalized gains
        let gains = dense(&panner, 0.0, 90.0);
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
        let gains = dense(&panner, 0.0, 0.0);
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
        let gains = dense(&panner, 0.0, 45.0);
        assert!(gains.iter().any(|&g| g > 0.0));
    }

    // Because gains carry their own speaker index, there is no buffer-length
    // contract to violate: computing gains needs no output slice at all, and
    // `accumulate_into` writes only within whatever slice it is handed.
    #[test]
    #[cfg(not(debug_assertions))]
    fn test_accumulate_into_ignores_out_of_range_speakers() {
        let panner = VBAPanner::builder().surround_5_1().build().unwrap();
        let mut cursor = PanCursor::default();

        // A source at Ls (index 3) would write past a 2-element buffer.
        let active = panner.compute_gains(110.0, 0.0, &mut cursor);
        assert!(active.iter().any(|(speaker, _)| speaker >= 2));

        let mut undersized = [0.0; 2];
        active.accumulate_into(&mut undersized);
        assert_eq!(undersized, [0.0; 2], "wrote outside the provided slice");
    }

    // Non-finite input trips a `debug_assert!` by design — it means the caller
    // has a bug. This test pins the *release* contract instead: silence rather
    // than NaN reaching the audio buffer.
    #[test]
    #[cfg(not(debug_assertions))]
    fn test_non_finite_input_yields_silence() {
        let panner = VBAPanner::builder().surround_5_1().build().unwrap();
        let mut cursor = PanCursor::default();

        for (azi, ele) in [
            (f64::NAN, 0.0),
            (0.0, f64::NAN),
            (f64::NAN, f64::NAN),
            (f64::INFINITY, 0.0),
            (f64::NEG_INFINITY, 0.0),
            (0.0, f64::INFINITY),
        ] {
            let active = panner.compute_gains(azi, ele, &mut cursor);
            assert!(
                active.is_empty(),
                "non-finite input ({azi}, {ele}) produced {active:?}"
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
        gains = dense(&panner, 0.0, 0.0);

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
                gains = dense(&panner, speaker.azimuth(), speaker.elevation());
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
            gains = dense(&panner, azimuth, 0.0);
            assert!(
                gains.iter().all(|g| *g == 0.0),
                "azimuth {azimuth} should be outside the arc, got {gains:?}"
            );
        }

        // ...while everything inside the arc keeps unit power.
        let mut azimuth = -30.0;
        while azimuth <= 30.0 {
            gains = dense(&panner, azimuth, 0.0);
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
                gains = dense(&panner, azimuth, 0.0);
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

        prev = dense(&panner, -30.0, 0.0);
        let mut azimuth = -30.0;
        while azimuth <= 30.0 {
            cur = dense(&panner, azimuth, 0.0);
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
    fn test_gains_are_well_formed_everywhere() {
        // Across the sphere: never more than three active speakers, indices
        // always in range, and either unit power or nothing at all.
        for panner in [
            VBAPanner::builder().stereo().build().unwrap(),
            VBAPanner::builder().lcr().build().unwrap(),
            VBAPanner::builder().surround_7_1().build().unwrap(),
            VBAPanner::builder().atmos_7_1_4().build().unwrap(),
        ] {
            let n = panner.num_speakers() as u32;
            let mut cursor = PanCursor::default();

            for step in 0..720 {
                let azimuth = -180.0 + step as f64 * 0.5;
                for elevation in [-30.0, 0.0, 30.0, 60.0] {
                    let active = panner.compute_gains(azimuth, elevation, &mut cursor);

                    assert!(active.len() <= 3);
                    assert!(active.iter().all(|(speaker, _)| speaker < n));
                    assert!(active.iter().all(|(_, gain)| gain >= 0.0));

                    let sum_sq: f64 = active.iter().map(|(_, g)| g * g).sum();
                    if !active.is_empty() {
                        assert_relative_eq!(sum_sq, 1.0, epsilon = 1e-9);
                    }
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
        panner.compute_gains(150.0, 40.0, &mut reused);

        for step in 0..360 {
            let azimuth = -180.0 + step as f64;
            let a = panner.compute_gains(azimuth, 20.0, &mut PanCursor::default());
            let b = panner.compute_gains(azimuth, 20.0, &mut fresh);
            let c = panner.compute_gains(azimuth, 20.0, &mut reused);
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

        let a = panner.compute_gains(30.0, 0.0, &mut cursor);
        let b = panner.compute_gains(-30.0, 0.0, &mut cursor);
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
