//! Standard speaker configuration presets.
//!
//! All angles are in degrees with the convention:
//! - Azimuth 0° = front center
//! - Positive azimuth = left
//! - Negative azimuth = right
//! - Elevation 0° = horizontal plane
//! - Positive elevation = above

/// Stereo configuration: Left and Right at ±30°.
pub const STEREO: &[(f64, f64)] = &[
    (30.0, 0.0),  // L
    (-30.0, 0.0), // R
];

/// Wide stereo configuration: Left and Right at ±60°.
pub const STEREO_WIDE: &[(f64, f64)] = &[
    (60.0, 0.0),  // L
    (-60.0, 0.0), // R
];

/// LCR (Left-Center-Right) configuration.
pub const LCR: &[(f64, f64)] = &[
    (30.0, 0.0),  // L
    (0.0, 0.0),   // C
    (-30.0, 0.0), // R
];

/// Quadraphonic (4.0) configuration.
pub const QUAD: &[(f64, f64)] = &[
    (45.0, 0.0),   // FL
    (-45.0, 0.0),  // FR
    (135.0, 0.0),  // RL
    (-135.0, 0.0), // RR
];

/// 5.0 surround configuration (no LFE - LFE is not spatialized).
///
/// Based on ITU-R BS.775-1 recommendation.
pub const SURROUND_5_0: &[(f64, f64)] = &[
    (30.0, 0.0),   // L
    (-30.0, 0.0),  // R
    (0.0, 0.0),    // C
    (110.0, 0.0),  // Ls
    (-110.0, 0.0), // Rs
];

/// 5.1 surround configuration (5.0 layout, LFE handled separately).
///
/// Note: LFE is typically not used in VBAP as it's not spatialized.
/// This is the same as SURROUND_5_0.
pub const SURROUND_5_1: &[(f64, f64)] = SURROUND_5_0;

/// 7.0 surround configuration.
///
/// Adds side surrounds to the 5.0 layout.
pub const SURROUND_7_0: &[(f64, f64)] = &[
    (30.0, 0.0),   // L
    (-30.0, 0.0),  // R
    (0.0, 0.0),    // C
    (90.0, 0.0),   // Lss (Left Side Surround)
    (-90.0, 0.0),  // Rss (Right Side Surround)
    (150.0, 0.0),  // Lrs (Left Rear Surround)
    (-150.0, 0.0), // Rrs (Right Rear Surround)
];

/// 7.1 surround configuration (7.0 layout, LFE handled separately).
pub const SURROUND_7_1: &[(f64, f64)] = SURROUND_7_0;

/// Dolby Atmos 7.1.4 configuration.
///
/// 7.1 base layer plus 4 overhead speakers.
pub const ATMOS_7_1_4: &[(f64, f64)] = &[
    // Base layer (7.0)
    (30.0, 0.0),   // L
    (-30.0, 0.0),  // R
    (0.0, 0.0),    // C
    (90.0, 0.0),   // Lss
    (-90.0, 0.0),  // Rss
    (150.0, 0.0),  // Lrs
    (-150.0, 0.0), // Rrs
    // Height layer (4 overhead)
    (45.0, 45.0),   // Ltf (Left Top Front)
    (-45.0, 45.0),  // Rtf (Right Top Front)
    (135.0, 45.0),  // Ltr (Left Top Rear)
    (-135.0, 45.0), // Rtr (Right Top Rear)
];

/// Dolby Atmos 5.1.4 configuration.
///
/// 5.1 base layer plus 4 overhead speakers.
pub const ATMOS_5_1_4: &[(f64, f64)] = &[
    // Base layer (5.0)
    (30.0, 0.0),   // L
    (-30.0, 0.0),  // R
    (0.0, 0.0),    // C
    (110.0, 0.0),  // Ls
    (-110.0, 0.0), // Rs
    // Height layer (4 overhead)
    (45.0, 45.0),   // Ltf
    (-45.0, 45.0),  // Rtf
    (135.0, 45.0),  // Ltr
    (-135.0, 45.0), // Rtr
];

/// Dolby Atmos 9.1.6 configuration.
///
/// Extended 7.1 base with front wide speakers, plus 6 overhead.
pub const ATMOS_9_1_6: &[(f64, f64)] = &[
    // Base layer
    (30.0, 0.0),   // L
    (-30.0, 0.0),  // R
    (0.0, 0.0),    // C
    (60.0, 0.0),   // Lw (Left Wide)
    (-60.0, 0.0),  // Rw (Right Wide)
    (90.0, 0.0),   // Lss
    (-90.0, 0.0),  // Rss
    (150.0, 0.0),  // Lrs
    (-150.0, 0.0), // Rrs
    // Height layer (6 overhead)
    (30.0, 45.0),   // Ltf
    (-30.0, 45.0),  // Rtf
    (90.0, 45.0),   // Ltm (Left Top Middle)
    (-90.0, 45.0),  // Rtm (Right Top Middle)
    (150.0, 45.0),  // Ltr
    (-150.0, 45.0), // Rtr
];

/// Auro-3D 9.1 configuration.
///
/// 5.1 base plus 4 height speakers at 30° elevation.
pub const AURO_9_1: &[(f64, f64)] = &[
    // Base layer (5.0)
    (30.0, 0.0),   // L
    (-30.0, 0.0),  // R
    (0.0, 0.0),    // C
    (110.0, 0.0),  // Ls
    (-110.0, 0.0), // Rs
    // Height layer (30° elevation per Auro-3D spec)
    (30.0, 30.0),   // HL (Height Left)
    (-30.0, 30.0),  // HR (Height Right)
    (110.0, 30.0),  // HLs (Height Left Surround)
    (-110.0, 30.0), // HRs (Height Right Surround)
];

/// Hexagonal 2D configuration (6 speakers in a ring).
pub const HEXAGON: &[(f64, f64)] = &[
    (0.0, 0.0),
    (60.0, 0.0),
    (120.0, 0.0),
    (180.0, 0.0),
    (-120.0, 0.0),
    (-60.0, 0.0),
];

/// Octagonal 2D configuration (8 speakers in a ring).
pub const OCTAGON: &[(f64, f64)] = &[
    (0.0, 0.0),
    (45.0, 0.0),
    (90.0, 0.0),
    (135.0, 0.0),
    (180.0, 0.0),
    (-135.0, 0.0),
    (-90.0, 0.0),
    (-45.0, 0.0),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_lengths() {
        assert_eq!(STEREO.len(), 2);
        assert_eq!(LCR.len(), 3);
        assert_eq!(QUAD.len(), 4);
        assert_eq!(SURROUND_5_0.len(), 5);
        assert_eq!(SURROUND_7_0.len(), 7);
        assert_eq!(ATMOS_7_1_4.len(), 11);
    }

    #[test]
    fn test_atmos_has_elevation() {
        // Atmos configs should have speakers with non-zero elevation
        let has_elevated = ATMOS_7_1_4.iter().any(|(_, ele)| *ele != 0.0);
        assert!(has_elevated);
    }
}
