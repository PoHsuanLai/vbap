//! Speaker configuration and builder.
//!
//! This module handles the construction of VBAP speaker configurations,
//! including the selection of valid speaker pairs (2D) or triplets (3D)
//! and the computation of inverse matrices for gain calculation.

use crate::error::{Result, VBAPError};
use crate::math::lines_intersect;
use crate::panner::VBAPanner;
use crate::presets;
use crate::speaker::Speaker;
use glam::{DMat2, DMat3, DVec2, DVec3};

/// Minimum angular distance between speakers to form a valid pair/triplet.
const MIN_PAIR_ANGLE: f64 = 0.0872665; // ~5 degrees in radians

/// Maximum angular distance for a speaker pair (prevents wrapping issues).
/// Approximately 175 degrees.
const MAX_PAIR_ANGLE: f64 = 3.0543; // π - 0.0873 radians

/// Minimum volume/side ratio for valid 3D triplets.
const MIN_VOL_P_SIDE_LGTH: f64 = 0.01;

/// Panning mode for VBAP computation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanningMode {
    /// 2D panning using speaker pairs (horizontal plane only).
    TwoD,
    /// 3D panning using speaker triplets (full sphere).
    ThreeD,
}

/// Dimension mode for builder configuration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dimension {
    /// Auto-detect based on speaker elevations.
    #[default]
    Auto,
    /// Force 2D panning (speaker pairs) even if speakers have elevation.
    Force2D,
    /// Force 3D panning (speaker triplets).
    Force3D,
}

/// Precomputed inverse matrix for gain computation.
#[derive(Clone, Copy, Debug)]
pub enum InverseMatrix {
    /// 2x2 matrix for 2D panning (speaker pairs).
    TwoD(DMat2),
    /// 3x3 matrix for 3D panning (speaker triplets).
    ThreeD(DMat3),
}

/// A speaker tuple (pair or triplet) with its precomputed inverse matrix.
#[derive(Clone, Debug)]
pub struct SpeakerTuple {
    /// Indices of speakers in this tuple (2 for 2D, 3 for 3D).
    pub speaker_indices: Vec<usize>,
    /// Inverse matrix for gain computation.
    pub inverse_matrix: InverseMatrix,
}

/// A fully configured speaker setup ready for VBAP computation.
#[derive(Clone, Debug)]
pub struct SpeakerConfig {
    /// All speakers in the configuration.
    speakers: Vec<Speaker>,
    /// Resolved panning mode.
    mode: PanningMode,
    /// Precomputed speaker tuples with inverse matrices.
    tuples: Vec<SpeakerTuple>,
}

impl SpeakerConfig {
    /// Get all speakers.
    #[inline]
    pub fn speakers(&self) -> &[Speaker] {
        &self.speakers
    }

    /// Get the number of speakers.
    #[inline]
    pub fn num_speakers(&self) -> usize {
        self.speakers.len()
    }

    /// Get the panning mode.
    #[inline]
    pub fn mode(&self) -> PanningMode {
        self.mode
    }

    /// Get the speaker tuples (pairs for 2D, triplets for 3D).
    #[inline]
    pub fn tuples(&self) -> &[SpeakerTuple] {
        &self.tuples
    }
}

/// Builder for constructing speaker configurations.
#[derive(Clone, Debug, Default)]
pub struct SpeakerConfigBuilder {
    speakers: Vec<(f64, f64)>, // (azimuth, elevation) pairs
    dimension: Dimension,
}

impl SpeakerConfigBuilder {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a speaker at the given position.
    ///
    /// # Arguments
    /// * `azimuth` - Horizontal angle in degrees (0° = front, 90° = left, -90° = right)
    /// * `elevation` - Vertical angle in degrees (0° = horizontal, 90° = above)
    pub fn add_speaker(mut self, azimuth: f64, elevation: f64) -> Self {
        self.speakers.push((azimuth, elevation));
        self
    }

    /// Add multiple speakers from an array of (azimuth, elevation) pairs.
    pub fn add_speakers(mut self, positions: &[(f64, f64)]) -> Self {
        self.speakers.extend_from_slice(positions);
        self
    }

    /// Set the dimension mode.
    pub fn dimension(mut self, dim: Dimension) -> Self {
        self.dimension = dim;
        self
    }

    // === Preset configurations ===

    /// Configure for standard stereo (L/R at ±30°).
    pub fn stereo(self) -> Self {
        self.add_speakers(presets::STEREO)
    }

    /// Configure for wide stereo (L/R at ±60°).
    pub fn stereo_wide(self) -> Self {
        self.add_speakers(presets::STEREO_WIDE)
    }

    /// Configure for LCR (Left-Center-Right).
    pub fn lcr(self) -> Self {
        self.add_speakers(presets::LCR)
    }

    /// Configure for quadraphonic (4.0).
    pub fn quad(self) -> Self {
        self.add_speakers(presets::QUAD)
    }

    /// Configure for 5.0/5.1 surround.
    pub fn surround_5_1(self) -> Self {
        self.add_speakers(presets::SURROUND_5_1)
    }

    /// Configure for 7.0/7.1 surround.
    pub fn surround_7_1(self) -> Self {
        self.add_speakers(presets::SURROUND_7_1)
    }

    /// Configure for Dolby Atmos 7.1.4.
    pub fn atmos_7_1_4(self) -> Self {
        self.add_speakers(presets::ATMOS_7_1_4)
    }

    /// Configure for Dolby Atmos 5.1.4.
    pub fn atmos_5_1_4(self) -> Self {
        self.add_speakers(presets::ATMOS_5_1_4)
    }

    /// Configure for hexagonal (6 speakers in ring).
    pub fn hexagon(self) -> Self {
        self.add_speakers(presets::HEXAGON)
    }

    /// Configure for octagonal (8 speakers in ring).
    pub fn octagon(self) -> Self {
        self.add_speakers(presets::OCTAGON)
    }

    /// Build a `VBAPanner` from this configuration.
    ///
    /// This is the primary build method and returns a ready-to-use panner.
    pub fn build(self) -> Result<VBAPanner> {
        Ok(VBAPanner::new(self.build_config()?))
    }

    /// Build only the speaker configuration (without creating a panner).
    ///
    /// This validates the configuration, selects valid speaker pairs/triplets,
    /// and computes the inverse matrices needed for VBAP.
    pub fn build_config(self) -> Result<SpeakerConfig> {
        let n = self.speakers.len();

        // Determine effective panning mode
        let has_elevation = self.speakers.iter().any(|(_, ele)| ele.abs() > 1e-6);
        let mode = match self.dimension {
            Dimension::Auto => {
                if has_elevation {
                    PanningMode::ThreeD
                } else {
                    PanningMode::TwoD
                }
            }
            Dimension::Force2D => PanningMode::TwoD,
            Dimension::Force3D => PanningMode::ThreeD,
        };

        // Check minimum speaker count
        let min_speakers = if mode == PanningMode::ThreeD { 3 } else { 2 };
        if n < min_speakers {
            return Err(VBAPError::InsufficientSpeakers {
                provided: n,
                required: min_speakers,
            });
        }

        // Create Speaker objects
        let speakers: Vec<Speaker> = self
            .speakers
            .into_iter()
            .enumerate()
            .map(|(id, (azi, ele))| Speaker::new(id, azi, ele))
            .collect();

        // Compute tuples based on mode
        let tuples = match mode {
            PanningMode::ThreeD => choose_speaker_triplets(&speakers)?,
            PanningMode::TwoD => choose_speaker_pairs(&speakers)?,
        };

        if tuples.is_empty() {
            return Err(VBAPError::InvalidConfiguration(
                "no valid speaker pairs/triplets could be formed".into(),
            ));
        }

        Ok(SpeakerConfig {
            speakers,
            mode,
            tuples,
        })
    }
}

/// Choose valid speaker pairs for 2D VBAP and compute their inverse matrices.
///
/// Based on Ardour's `choose_speaker_pairs()` in vbap_speakers.cc.
fn choose_speaker_pairs(speakers: &[Speaker]) -> Result<Vec<SpeakerTuple>> {
    let n = speakers.len();
    if n < 2 {
        return Err(VBAPError::InsufficientSpeakers {
            provided: n,
            required: 2,
        });
    }

    // Sort speakers by azimuth
    let mut sorted_indices: Vec<usize> = (0..n).collect();
    sorted_indices.sort_by(|&a, &b| speakers[a].azimuth().total_cmp(&speakers[b].azimuth()));

    // Create pairs from adjacent speakers (in sorted order)
    let tuples = (0..n)
        .filter_map(|i| {
            let idx1 = sorted_indices[i];
            let idx2 = sorted_indices[(i + 1) % n];

            let s1 = &speakers[idx1];
            let s2 = &speakers[idx2];

            // Skip pairs that are too close or too far apart
            let angle = s1.cartesian().angle_between(s2.cartesian());
            if !(MIN_PAIR_ANGLE..=MAX_PAIR_ANGLE).contains(&angle) {
                return None;
            }

            // Compute 2x2 inverse matrix for this pair
            // Matrix columns are speaker direction vectors (sin/cos of azimuth)
            let azi1_rad = s1.azimuth().to_radians();
            let azi2_rad = s2.azimuth().to_radians();

            let mat = DMat2::from_cols(
                DVec2::new(azi1_rad.sin(), azi1_rad.cos()),
                DVec2::new(azi2_rad.sin(), azi2_rad.cos()),
            );

            if mat.determinant().abs() < 1e-10 {
                return None;
            }

            Some(SpeakerTuple {
                speaker_indices: vec![idx1, idx2],
                inverse_matrix: InverseMatrix::TwoD(mat.inverse()),
            })
        })
        .collect();

    Ok(tuples)
}

/// Choose valid speaker triplets for 3D VBAP and compute their inverse matrices.
///
/// Based on Ardour's `choose_speaker_triplets()` in vbap_speakers.cc.
/// This implements a convex hull-like algorithm to find valid triangular facets.
fn choose_speaker_triplets(speakers: &[Speaker]) -> Result<Vec<SpeakerTuple>> {
    let n = speakers.len();
    if n < 3 {
        return Err(VBAPError::InsufficientSpeakers {
            provided: n,
            required: 3,
        });
    }

    // Connection matrix: connections[i*n + j] = true if speakers i and j are connected
    let mut connections = vec![true; n * n];

    // First pass: find all potentially valid triplets
    let mut candidates: Vec<(usize, usize, usize, f64)> = Vec::new();

    for i in 0..n {
        for j in (i + 1)..n {
            for k in (j + 1)..n {
                let v1 = speakers[i].cartesian();
                let v2 = speakers[j].cartesian();
                let v3 = speakers[k].cartesian();

                // Calculate volume-to-perimeter ratio (filters degenerate triplets)
                let cross = v1.cross(v2);
                let vol = cross.dot(v3).abs();
                let side_sum = v1.angle_between(v2) + v1.angle_between(v3) + v2.angle_between(v3);

                if side_sum < 1e-10 {
                    continue;
                }

                let vol_p_side = vol / side_sum;

                if vol_p_side > MIN_VOL_P_SIDE_LGTH {
                    candidates.push((i, j, k, vol_p_side));
                }
            }
        }
    }

    // Build distance table for all speaker pairs, sorted by distance (shortest first)
    let mut distances: Vec<(usize, usize, f64)> = (0..n)
        .flat_map(|i| {
            ((i + 1)..n).map(move |j| {
                let dist = speakers[i]
                    .cartesian()
                    .angle_between(speakers[j].cartesian());
                (i, j, dist)
            })
        })
        .collect();
    distances.sort_by(|a, b| a.2.total_cmp(&b.2));

    // Remove crossing connections (longer lines that cross shorter ones)
    for (a, b, _) in &distances {
        let va = speakers[*a].cartesian();
        let vb = speakers[*b].cartesian();

        // Check all other connections
        for (c, d, _) in &distances {
            if a == c || a == d || b == c || b == d {
                continue;
            }

            if !connections[*c * n + *d] {
                continue;
            }

            let vc = speakers[*c].cartesian();
            let vd = speakers[*d].cartesian();

            if lines_intersect(va, vb, vc, vd) {
                // Remove the longer connection
                let dist_ab = va.angle_between(vb);
                let dist_cd = vc.angle_between(vd);

                if dist_cd > dist_ab {
                    connections[*c * n + *d] = false;
                    connections[*d * n + *c] = false;
                }
            }
        }
    }

    // Filter triplets based on remaining connections
    let mut tuples = Vec::new();

    for (i, j, k, _) in candidates {
        // Check if all three sides are still connected
        if !connections[i * n + j] || !connections[i * n + k] || !connections[j * n + k] {
            continue;
        }

        // Check if any other speaker is "inside" this triplet
        let v1 = speakers[i].cartesian();
        let v2 = speakers[j].cartesian();
        let v3 = speakers[k].cartesian();

        let has_interior_speaker = speakers.iter().enumerate().any(|(m, speaker)| {
            m != i && m != j && m != k && is_inside_triangle(speaker.cartesian(), v1, v2, v3)
        });

        if has_interior_speaker {
            continue;
        }

        // Compute 3x3 inverse matrix using glam
        // Matrix columns are the speaker direction vectors
        let mat = DMat3::from_cols(v1, v2, v3);

        if mat.determinant().abs() < 1e-10 {
            continue;
        }

        tuples.push(SpeakerTuple {
            speaker_indices: vec![i, j, k],
            inverse_matrix: InverseMatrix::ThreeD(mat.inverse()),
        });
    }

    Ok(tuples)
}

/// Check if point p is inside the spherical triangle defined by v1, v2, v3.
fn is_inside_triangle(p: DVec3, v1: DVec3, v2: DVec3, v3: DVec3) -> bool {
    // Use barycentric-like approach on the sphere
    // Point is inside if it's on the same side of all three edges

    let n1 = v1.cross(v2);
    let n2 = v2.cross(v3);
    let n3 = v3.cross(v1);

    let d1 = p.dot(n1);
    let d2 = p.dot(n2);
    let d3 = p.dot(n3);

    // All same sign means inside (or on edge)
    (d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0) || (d1 <= 0.0 && d2 <= 0.0 && d3 <= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_stereo() {
        let config = SpeakerConfigBuilder::new().stereo().build_config().unwrap();

        assert_eq!(config.num_speakers(), 2);
        assert_eq!(config.mode(), PanningMode::TwoD);
        assert!(!config.tuples().is_empty());
    }

    #[test]
    fn test_build_surround_5_1() {
        let config = SpeakerConfigBuilder::new()
            .surround_5_1()
            .build_config()
            .unwrap();

        assert_eq!(config.num_speakers(), 5);
        assert_eq!(config.mode(), PanningMode::TwoD);
    }

    #[test]
    fn test_build_atmos() {
        let config = SpeakerConfigBuilder::new()
            .atmos_7_1_4()
            .build_config()
            .unwrap();

        assert_eq!(config.num_speakers(), 11);
        assert_eq!(config.mode(), PanningMode::ThreeD); // Auto-detected from elevation
    }

    #[test]
    fn test_force_2d() {
        let config = SpeakerConfigBuilder::new()
            .atmos_7_1_4()
            .dimension(Dimension::Force2D)
            .build_config()
            .unwrap();

        assert_eq!(config.mode(), PanningMode::TwoD);
    }

    #[test]
    fn test_insufficient_speakers() {
        let result = SpeakerConfigBuilder::new()
            .add_speaker(0.0, 0.0)
            .build_config();

        assert!(matches!(
            result,
            Err(VBAPError::InsufficientSpeakers { provided: 1, .. })
        ));
    }

    #[test]
    fn test_custom_speakers() {
        let config = SpeakerConfigBuilder::new()
            .add_speaker(30.0, 0.0)
            .add_speaker(-30.0, 0.0)
            .add_speaker(0.0, 0.0)
            .build_config()
            .unwrap();

        assert_eq!(config.num_speakers(), 3);
    }
}
