//! Core VBAP panner implementation.
//!
//! This module provides the main `VBAPanner` struct that computes
//! speaker gains for a given source position.

use crate::config::{InverseMatrix, PanningMode, SpeakerConfig, SpeakerConfigBuilder};
use crate::math::spherical_to_cartesian;
use crate::speaker::Speaker;
use glam::DVec2;

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
/// // Pan a source 15 degrees to the left
/// let gains = panner.compute_gains(15.0, 0.0);
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
    /// # Arguments
    /// * `azimuth` - Horizontal angle in degrees (0° = front, 90° = left, -90° = right)
    /// * `elevation` - Vertical angle in degrees (0° = horizontal, 90° = above)
    ///
    /// # Returns
    /// A vector of gains, one per speaker. Gains are normalized so that
    /// the sum of squared gains equals 1.0. Most gains will be 0.0,
    /// with only 2-3 speakers active (depending on 2D/3D mode).
    pub fn compute_gains(&self, azimuth: f64, elevation: f64) -> Vec<f64> {
        let mut gains = vec![0.0; self.config.num_speakers()];
        self.compute_gains_into(azimuth, elevation, &mut gains);
        gains
    }

    /// Compute speaker gains into a pre-allocated slice.
    ///
    /// This avoids allocation when called repeatedly.
    ///
    /// # Panics
    /// Panics if `gains.len() < self.num_speakers()`.
    #[inline]
    pub fn compute_gains_into(&self, azimuth: f64, elevation: f64, gains: &mut [f64]) {
        assert!(
            gains.len() >= self.config.num_speakers(),
            "gains slice too small: {} < {}",
            gains.len(),
            self.config.num_speakers()
        );

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

        // Apply the winning gains
        let best_tuple = &tuples[best_tuple_idx];

        // Normalize gains: sqrt(sum of squares) = 1
        let sum_sq: f64 = best_gains[..best_len].iter().map(|g| g * g).sum();
        let norm = if sum_sq > 1e-10 {
            1.0 / sum_sq.sqrt()
        } else {
            0.0
        };

        for (&speaker_idx, &gain) in best_tuple
            .speaker_indices
            .iter()
            .zip(&best_gains[..best_len])
        {
            gains[speaker_idx] = (gain * norm).max(0.0);
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
}
