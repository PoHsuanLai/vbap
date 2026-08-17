//! Speaker position representation.

use crate::math::spherical_to_cartesian;
use glam::DVec3;

/// Wrap an angle in degrees into the half-open range `[-180, 180)`.
///
/// Exact for angles already in range, so ordinary speaker positions are
/// returned bit-identical.
#[inline]
pub(crate) fn wrap_azimuth(azimuth: f64) -> f64 {
    if (-180.0..180.0).contains(&azimuth) {
        return azimuth;
    }
    let wrapped = libm::fmod(azimuth + 180.0, 360.0);
    if wrapped < 0.0 {
        wrapped + 180.0
    } else {
        wrapped - 180.0
    }
}

/// A speaker at a specific position in 3D space.
///
/// Positions are defined using spherical coordinates (azimuth, elevation)
/// and automatically converted to Cartesian for internal calculations.
#[derive(Clone, Debug)]
pub struct Speaker {
    /// Speaker index/ID.
    id: usize,

    /// Azimuth angle in degrees.
    /// - 0° = front center
    /// - 90° = left
    /// - -90° = right
    /// - 180° = rear center
    azimuth: f64,

    /// Elevation angle in degrees.
    /// - 0° = horizontal plane
    /// - 90° = directly above
    /// - -90° = directly below
    elevation: f64,

    /// Distance from listening position (default 1.0).
    distance: f64,

    /// Cached Cartesian coordinates (unit vector on sphere).
    cartesian: DVec3,
}

impl Speaker {
    /// Create a new speaker at the given azimuth and elevation (in degrees).
    ///
    /// Distance defaults to 1.0 (unit sphere).
    pub fn new(id: usize, azimuth: f64, elevation: f64) -> Self {
        Self::with_distance(id, azimuth, elevation, 1.0)
    }

    /// Create a new speaker with a specific distance from the listening position.
    ///
    /// The azimuth is wrapped into `[-180, 180)`. Pair selection sorts speakers
    /// by azimuth to find adjacent neighbours, so an unwrapped angle (e.g. 370°
    /// meaning the same direction as 10°) would otherwise sort to the wrong
    /// place and destroy the adjacency structure.
    pub fn with_distance(id: usize, azimuth: f64, elevation: f64, distance: f64) -> Self {
        let azimuth = wrap_azimuth(azimuth);
        let cartesian = spherical_to_cartesian(azimuth, elevation);
        Self {
            id,
            azimuth,
            elevation,
            distance,
            cartesian,
        }
    }

    /// Get the speaker's ID.
    #[inline]
    pub fn id(&self) -> usize {
        self.id
    }

    /// Get the azimuth angle in degrees.
    #[inline]
    pub fn azimuth(&self) -> f64 {
        self.azimuth
    }

    /// Get the elevation angle in degrees.
    #[inline]
    pub fn elevation(&self) -> f64 {
        self.elevation
    }

    /// Get the distance from listening position.
    #[inline]
    pub fn distance(&self) -> f64 {
        self.distance
    }

    /// Get the Cartesian unit vector pointing to this speaker.
    #[inline]
    pub fn cartesian(&self) -> DVec3 {
        self.cartesian
    }

    /// Check if this speaker is in the horizontal plane (elevation ≈ 0).
    #[inline]
    pub fn is_horizontal(&self) -> bool {
        self.elevation.abs() < 1e-6
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_speaker_front_center() {
        let speaker = Speaker::new(0, 0.0, 0.0);
        assert_relative_eq!(speaker.cartesian().x, 0.0, epsilon = 1e-10);
        assert_relative_eq!(speaker.cartesian().y, 1.0, epsilon = 1e-10);
        assert_relative_eq!(speaker.cartesian().z, 0.0, epsilon = 1e-10);
    }

    #[test]
    fn test_speaker_left() {
        let speaker = Speaker::new(0, 90.0, 0.0);
        assert_relative_eq!(speaker.cartesian().x, 1.0, epsilon = 1e-10);
        assert_relative_eq!(speaker.cartesian().y, 0.0, epsilon = 1e-10);
    }

    #[test]
    fn test_wrap_azimuth() {
        // In-range values are returned untouched.
        for a in [-180.0, -90.0, 0.0, 30.0, 179.9] {
            assert_eq!(wrap_azimuth(a), a);
        }
        // Out-of-range values wrap to the equivalent direction.
        assert_relative_eq!(wrap_azimuth(370.0), 10.0, epsilon = 1e-12);
        assert_relative_eq!(wrap_azimuth(-190.0), 170.0, epsilon = 1e-12);
        assert_relative_eq!(wrap_azimuth(190.0), -170.0, epsilon = 1e-12);
        assert_relative_eq!(wrap_azimuth(720.0), 0.0, epsilon = 1e-12);
        // 180 is the exclusive end of the range and folds to -180.
        assert_relative_eq!(wrap_azimuth(180.0), -180.0, epsilon = 1e-12);
    }

    #[test]
    fn test_speaker_azimuth_is_normalized() {
        // An unwrapped angle must land on the same direction as its equivalent,
        // otherwise azimuth sorting in pair selection breaks.
        let wrapped = Speaker::new(0, 370.0, 0.0);
        let plain = Speaker::new(1, 10.0, 0.0);
        assert_relative_eq!(wrapped.azimuth(), plain.azimuth(), epsilon = 1e-12);
        assert_relative_eq!(wrapped.cartesian().x, plain.cartesian().x, epsilon = 1e-12);
        assert_relative_eq!(wrapped.cartesian().y, plain.cartesian().y, epsilon = 1e-12);
    }

    #[test]
    fn test_speaker_is_horizontal() {
        let horizontal = Speaker::new(0, 45.0, 0.0);
        let elevated = Speaker::new(1, 45.0, 30.0);

        assert!(horizontal.is_horizontal());
        assert!(!elevated.is_horizontal());
    }
}
