//! Speaker configuration and builder.
//!
//! This module handles the construction of VBAP speaker configurations,
//! including the selection of valid speaker pairs (2D) or triplets (3D)
//! and the computation of inverse matrices for gain calculation.

use alloc::vec;
use alloc::vec::Vec;

use crate::error::{Result, VBAPError};
use crate::math::convex_hull;
use crate::panner::VBAPanner;
use crate::presets;
use crate::speaker::Speaker;
use glam::{DMat2, DMat3, DVec2, DVec3};

/// Minimum angular distance between speakers to form a valid pair/triplet.
const MIN_PAIR_ANGLE: f64 = 0.0872665; // ~5 degrees in radians

/// Maximum angular distance for a speaker pair (prevents wrapping issues).
/// Approximately 175 degrees.
const MAX_PAIR_ANGLE: f64 = 3.0543; // π - 0.0873 radians

/// Tolerance for "this point lies outside that face's plane" during hull
/// construction.
const HULL_EPSILON: f64 = 1e-9;

/// A face whose supporting plane passes this close to the listener is
/// degenerate for VBAP: its basis matrix is singular.
///
/// This subsumes the volume/side-ratio threshold used previously. Pulkki (1997)
/// §6.2 reports that even a 5°/175°/175° triangle produces correct gains, so the
/// only geometry that must be rejected is the genuinely singular kind, which is
/// exactly what this test identifies.
const ORIGIN_PLANE_EPSILON: f64 = 1e-6;

/// Below this a triangle normal is numerically meaningless.
const NORMAL_EPSILON_SQ: f64 = 1e-24;

/// Below this a speaker basis matrix is singular.
const DET_EPSILON: f64 = 1e-10;

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
            let reason = if mode == PanningMode::ThreeD {
                "3D panning requires speakers that are not all coplanar with the \
                 listener; use Dimension::Force2D or add an elevated speaker"
            } else {
                "no valid speaker pairs could be formed; check for duplicate or \
                 antipodal speaker positions"
            };
            return Err(VBAPError::InvalidConfiguration(reason.into()));
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
            let azi1_rad = s1.azimuth() * (core::f64::consts::PI / 180.0);
            let azi2_rad = s2.azimuth() * (core::f64::consts::PI / 180.0);

            let (sin1, cos1) = libm::sincos(azi1_rad);
            let (sin2, cos2) = libm::sincos(azi2_rad);

            let mat = DMat2::from_cols(DVec2::new(sin1, cos1), DVec2::new(sin2, cos2));

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
/// The active triangles are the faces of the convex hull of the speaker
/// direction vectors, minus any face whose supporting plane passes through the
/// listener. Radially projected from the listener, hull faces tile the sphere
/// exactly once, which is precisely the non-intersecting arrangement Pulkki
/// (1997) §2.3 requires: "The active triangles of bases should not be
/// intersecting."
///
/// Faces lying in a plane through the origin are dropped because their basis
/// matrix is singular by construction — this is the flat "bottom" spanned by a
/// horizontal speaker ring, which every surround layout has.
fn choose_speaker_triplets(speakers: &[Speaker]) -> Result<Vec<SpeakerTuple>> {
    let n = speakers.len();
    if n < 3 {
        return Err(VBAPError::InsufficientSpeakers {
            provided: n,
            required: 3,
        });
    }

    let points: Vec<DVec3> = speakers.iter().map(|s| s.cartesian()).collect();

    // A hull needs four non-coplanar points; with exactly three speakers the
    // single triangle they span is the only possible base.
    let faces: Vec<[usize; 3]> = if n == 3 {
        vec![[0, 1, 2]]
    } else {
        convex_hull(&points, HULL_EPSILON)
    };

    let tuples = faces
        .into_iter()
        .filter_map(|[i, j, k]| {
            let (v1, v2, v3) = (points[i], points[j], points[k]);

            // Drop faces whose supporting plane contains the listener. These are
            // the degenerate great-circle faces produced by a coplanar speaker
            // ring; their radial projection would overlap the real faces.
            let normal = (v2 - v1).cross(v3 - v1);
            if normal.length_squared() < NORMAL_EPSILON_SQ {
                return None;
            }
            if normal.normalize().dot(v1).abs() <= ORIGIN_PLANE_EPSILON {
                return None;
            }

            let mat = DMat3::from_cols(v1, v2, v3);
            if mat.determinant().abs() < DET_EPSILON {
                return None;
            }

            Some(SpeakerTuple {
                speaker_indices: vec![i, j, k],
                inverse_matrix: InverseMatrix::ThreeD(mat.inverse()),
            })
        })
        .collect();

    Ok(tuples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::spherical_to_cartesian;

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

    /// Every 3D preset, as (name, positions).
    fn three_d_presets() -> [(&'static str, &'static [(f64, f64)]); 4] {
        [
            ("atmos_7_1_4", presets::ATMOS_7_1_4),
            ("atmos_5_1_4", presets::ATMOS_5_1_4),
            ("atmos_9_1_6", presets::ATMOS_9_1_6),
            ("auro_9_1", presets::AURO_9_1),
        ]
    }

    #[test]
    fn test_no_orphan_speakers_3d() {
        // Every speaker must belong to at least one triplet, or it can never
        // produce sound. The centre channel used to be orphaned in every Atmos
        // layout, which made dialogue silent.
        for (name, positions) in three_d_presets() {
            let config = SpeakerConfigBuilder::new()
                .add_speakers(positions)
                .build_config()
                .unwrap();

            for idx in 0..config.num_speakers() {
                let used = config
                    .tuples()
                    .iter()
                    .any(|t| t.speaker_indices.contains(&idx));
                assert!(used, "{name}: speaker {idx} appears in no triplet");
            }
        }
    }

    #[test]
    fn test_triplets_do_not_overlap() {
        // Pulkki §2.3: active triangles must not intersect. Overlapping regions
        // make the max-min selection tie on interior points, so the winner flips
        // on float noise as a source moves — an audible click.
        for (name, positions) in three_d_presets() {
            let config = SpeakerConfigBuilder::new()
                .add_speakers(positions)
                .build_config()
                .unwrap();

            let mut elevation = -80.0;
            while elevation <= 80.0 {
                let mut azimuth = -180.0;
                while azimuth < 180.0 {
                    let dir = spherical_to_cartesian(azimuth, elevation);
                    let interior = config
                        .tuples()
                        .iter()
                        .filter(|t| match &t.inverse_matrix {
                            InverseMatrix::ThreeD(inv) => {
                                let g = *inv * dir;
                                g.x > 1e-7 && g.y > 1e-7 && g.z > 1e-7
                            }
                            InverseMatrix::TwoD(_) => false,
                        })
                        .count();
                    assert!(
                        interior <= 1,
                        "{name}: direction ({azimuth}, {elevation}) is interior to \
                         {interior} triplets"
                    );
                    azimuth += 5.0;
                }
                elevation += 5.0;
            }
        }
    }

    #[test]
    fn test_all_coplanar_force3d_errors() {
        // A horizontal-only ring cannot span 3D. This must be a clean error
        // rather than a panic or a silently empty configuration.
        let result = SpeakerConfigBuilder::new()
            .surround_5_1()
            .dimension(Dimension::Force3D)
            .build_config();

        assert!(matches!(result, Err(VBAPError::InvalidConfiguration(_))));
    }

    #[test]
    fn test_minimal_3d_triplet() {
        // Three non-coplanar speakers form exactly one base.
        let config = SpeakerConfigBuilder::new()
            .add_speaker(0.0, 0.0)
            .add_speaker(120.0, 0.0)
            .add_speaker(0.0, 90.0)
            .build_config()
            .unwrap();

        assert_eq!(config.mode(), PanningMode::ThreeD);
        assert_eq!(config.tuples().len(), 1);
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
