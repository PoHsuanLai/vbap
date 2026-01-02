//! Coordinate conversions and geometry utilities for VBAP.
//!
//! Uses `glam` for SIMD-optimized vector operations.

use glam::DVec3;

/// Convert spherical coordinates (azimuth, elevation in degrees) to Cartesian unit vector.
///
/// Convention:
/// - Azimuth 0° = front center (+Y axis)
/// - Azimuth 90° = left (+X axis)
/// - Azimuth -90° = right (-X axis)
/// - Elevation 0° = horizontal plane
/// - Elevation 90° = directly above (+Z axis)
#[inline]
pub fn spherical_to_cartesian(azimuth: f64, elevation: f64) -> DVec3 {
    let (azi_sin, azi_cos) = azimuth.to_radians().sin_cos();
    let (ele_sin, ele_cos) = elevation.to_radians().sin_cos();

    DVec3::new(
        ele_cos * azi_sin, // X: left-right
        ele_cos * azi_cos, // Y: front-back
        ele_sin,           // Z: up-down
    )
}

/// Convert Cartesian vector to spherical coordinates (azimuth, elevation in degrees).
///
/// Returns (azimuth, elevation) tuple.
#[inline]
pub fn cartesian_to_spherical(v: DVec3) -> (f64, f64) {
    let normalized = v.normalize_or_zero();
    if normalized == DVec3::ZERO {
        return (0.0, 0.0);
    }

    let elevation = normalized.z.asin().to_degrees();
    let azimuth = normalized.x.atan2(normalized.y).to_degrees();

    (azimuth, elevation)
}

/// Check if two great circle arcs intersect on a unit sphere.
///
/// Arc 1: from a1 to a2
/// Arc 2: from b1 to b2
///
/// Based on Pulkki's VBAP implementation.
#[inline]
pub(crate) fn lines_intersect(a1: DVec3, a2: DVec3, b1: DVec3, b2: DVec3) -> bool {
    // Normal vectors to the planes containing each arc
    let n1 = a1.cross(a2);
    let n2 = b1.cross(b2);

    // Line of intersection between the two planes
    let intersection = n1.cross(n2);

    let int_normalized = intersection.normalize_or_zero();
    if int_normalized == DVec3::ZERO {
        // Planes are parallel (arcs are on the same great circle)
        return false;
    }

    // Two potential intersection points (antipodal)
    let p1 = int_normalized;
    let p2 = -int_normalized;

    // Check if either intersection point lies on both arcs
    (point_on_arc(p1, a1, a2) && point_on_arc(p1, b1, b2))
        || (point_on_arc(p2, a1, a2) && point_on_arc(p2, b1, b2))
}

/// Check if point p lies on the arc from a to b (shorter path on great circle).
#[inline]
fn point_on_arc(p: DVec3, a: DVec3, b: DVec3) -> bool {
    let angle_ab = a.angle_between(b);
    let angle_ap = a.angle_between(p);
    let angle_pb = p.angle_between(b);

    // Point is on arc if sum of angles to endpoints equals the arc angle
    // (with some tolerance for floating point)
    (angle_ap + angle_pb - angle_ab).abs() < 1e-6
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_spherical_to_cartesian_front() {
        let v = spherical_to_cartesian(0.0, 0.0);
        assert_relative_eq!(v.x, 0.0, epsilon = 1e-10);
        assert_relative_eq!(v.y, 1.0, epsilon = 1e-10);
        assert_relative_eq!(v.z, 0.0, epsilon = 1e-10);
    }

    #[test]
    fn test_spherical_to_cartesian_left() {
        let v = spherical_to_cartesian(90.0, 0.0);
        assert_relative_eq!(v.x, 1.0, epsilon = 1e-10);
        assert_relative_eq!(v.y, 0.0, epsilon = 1e-10);
        assert_relative_eq!(v.z, 0.0, epsilon = 1e-10);
    }

    #[test]
    fn test_spherical_to_cartesian_up() {
        let v = spherical_to_cartesian(0.0, 90.0);
        assert_relative_eq!(v.x, 0.0, epsilon = 1e-10);
        assert_relative_eq!(v.y, 0.0, epsilon = 1e-10);
        assert_relative_eq!(v.z, 1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_cartesian_to_spherical_roundtrip() {
        for (azi, ele) in [
            (0.0, 0.0),
            (45.0, 0.0),
            (-45.0, 0.0),
            (90.0, 0.0),
            (0.0, 45.0),
            (45.0, 30.0),
        ] {
            let cart = spherical_to_cartesian(azi, ele);
            let (azi2, ele2) = cartesian_to_spherical(cart);
            assert_relative_eq!(azi, azi2, epsilon = 1e-9);
            assert_relative_eq!(ele, ele2, epsilon = 1e-9);
        }
    }
}
