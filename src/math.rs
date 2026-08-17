//! Coordinate conversions and geometry utilities for VBAP.
//!
//! Uses `glam` for SIMD-optimized vector operations.

use alloc::vec::Vec;
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
    let azi_rad = azimuth * (core::f64::consts::PI / 180.0);
    let ele_rad = elevation * (core::f64::consts::PI / 180.0);
    let (azi_sin, azi_cos) = libm::sincos(azi_rad);
    let (ele_sin, ele_cos) = libm::sincos(ele_rad);

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

    let elevation = libm::asin(normalized.z) * (180.0 / core::f64::consts::PI);
    let azimuth = libm::atan2(normalized.x, normalized.y) * (180.0 / core::f64::consts::PI);

    (azimuth, elevation)
}

/// Compute the convex hull of a set of 3D points as outward-oriented triangles.
///
/// Uses incremental (horizon-edge) construction. The face set stays a closed
/// manifold at every step, which is what keeps coplanar clusters — such as the
/// horizontal speaker ring present in every surround layout — from producing
/// contradictory, mutually overlapping faces.
///
/// Faces are oriented so their normals point away from the hull interior.
///
/// Returns an empty `Vec` when the points are coplanar, since no 3D hull exists.
pub(crate) fn convex_hull(pts: &[DVec3], eps: f64) -> Vec<[usize; 3]> {
    let n = pts.len();
    if n < 4 {
        return Vec::new();
    }

    // --- Seed a non-degenerate tetrahedron -------------------------------
    // Two most distant points.
    let (mut i0, mut i1, mut best) = (0usize, 1usize, -1.0f64);
    for a in 0..n {
        for b in (a + 1)..n {
            let d = pts[a].distance_squared(pts[b]);
            if d > best {
                best = d;
                i0 = a;
                i1 = b;
            }
        }
    }
    if best <= eps * eps {
        return Vec::new(); // all points coincident
    }

    // Point furthest off the line i0-i1.
    let axis = pts[i1] - pts[i0];
    let (mut i2, mut best_area) = (usize::MAX, eps);
    for c in 0..n {
        if c == i0 || c == i1 {
            continue;
        }
        let area = axis.cross(pts[c] - pts[i0]).length();
        if area > best_area {
            best_area = area;
            i2 = c;
        }
    }
    if i2 == usize::MAX {
        return Vec::new(); // all points collinear
    }

    // Point furthest off the plane i0-i1-i2.
    let normal = axis.cross(pts[i2] - pts[i0]).normalize();
    let (mut i3, mut best_dist) = (usize::MAX, eps);
    for d in 0..n {
        if d == i0 || d == i1 || d == i2 {
            continue;
        }
        let dist = normal.dot(pts[d] - pts[i0]).abs();
        if dist > best_dist {
            best_dist = dist;
            i3 = d;
        }
    }
    if i3 == usize::MAX {
        return Vec::new(); // all points coplanar — no 3D hull
    }

    // Orient the seed faces outward relative to the tetrahedron centroid.
    let centroid = (pts[i0] + pts[i1] + pts[i2] + pts[i3]) / 4.0;
    let mut faces: Vec<[usize; 3]> = Vec::with_capacity(4 * n);
    for face in [[i0, i1, i2], [i0, i1, i3], [i0, i2, i3], [i1, i2, i3]] {
        faces.push(orient_outward(face, pts, centroid));
    }

    // --- Add the remaining points ----------------------------------------
    let mut visible: Vec<usize> = Vec::new();
    let mut horizon: Vec<(usize, usize)> = Vec::new();

    for (p, &point) in pts.iter().enumerate() {
        if p == i0 || p == i1 || p == i2 || p == i3 {
            continue;
        }

        // Faces this point can "see" from outside.
        visible.clear();
        for (f, face) in faces.iter().enumerate() {
            let v0 = pts[face[0]];
            let nrm = (pts[face[1]] - v0).cross(pts[face[2]] - v0);
            if nrm.dot(point - v0) > eps {
                visible.push(f);
            }
        }
        if visible.is_empty() {
            continue; // inside the hull
        }

        // Horizon = directed edges of visible faces whose reverse is not also
        // an edge of a visible face. This is what preserves manifoldness.
        horizon.clear();
        for &f in &visible {
            let [a, b, c] = faces[f];
            for edge in [(a, b), (b, c), (c, a)] {
                let shared = visible.iter().any(|&g| {
                    g != f && {
                        let [x, y, z] = faces[g];
                        [(x, y), (y, z), (z, x)].contains(&(edge.1, edge.0))
                    }
                });
                if !shared {
                    horizon.push(edge);
                }
            }
        }

        // Drop visible faces (descending, so indices stay valid) and cone the
        // horizon to the new point.
        visible.sort_unstable_by(|a, b| b.cmp(a));
        for &f in &visible {
            faces.swap_remove(f);
        }
        for &(a, b) in &horizon {
            faces.push([a, b, p]);
        }
    }

    faces
}

/// Orient a triangle so its normal points away from `interior`.
#[inline]
fn orient_outward(face: [usize; 3], pts: &[DVec3], interior: DVec3) -> [usize; 3] {
    let [a, b, c] = face;
    let normal = (pts[b] - pts[a]).cross(pts[c] - pts[a]);
    if normal.dot(pts[a] - interior) < 0.0 {
        [a, c, b]
    } else {
        face
    }
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

    #[test]
    fn test_spherical_to_cartesian_down() {
        let v = spherical_to_cartesian(0.0, -90.0);
        assert_relative_eq!(v.x, 0.0, epsilon = 1e-10);
        assert_relative_eq!(v.y, 0.0, epsilon = 1e-10);
        assert_relative_eq!(v.z, -1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_spherical_to_cartesian_rear() {
        let v = spherical_to_cartesian(180.0, 0.0);
        assert_relative_eq!(v.x, 0.0, epsilon = 1e-10);
        assert_relative_eq!(v.y, -1.0, epsilon = 1e-10);
        assert_relative_eq!(v.z, 0.0, epsilon = 1e-10);
    }

    #[test]
    fn test_angle_wraparound() {
        // 450° should produce the same result as 90°
        let v_90 = spherical_to_cartesian(90.0, 0.0);
        let v_450 = spherical_to_cartesian(450.0, 0.0);
        assert_relative_eq!(v_90.x, v_450.x, epsilon = 1e-10);
        assert_relative_eq!(v_90.y, v_450.y, epsilon = 1e-10);
        assert_relative_eq!(v_90.z, v_450.z, epsilon = 1e-10);

        // -270° should also equal 90°
        let v_neg270 = spherical_to_cartesian(-270.0, 0.0);
        assert_relative_eq!(v_90.x, v_neg270.x, epsilon = 1e-10);
        assert_relative_eq!(v_90.y, v_neg270.y, epsilon = 1e-10);
    }

    #[test]
    fn test_cartesian_to_spherical_poles() {
        // Directly above
        let (_, ele) = cartesian_to_spherical(DVec3::new(0.0, 0.0, 1.0));
        assert_relative_eq!(ele, 90.0, epsilon = 1e-9);

        // Directly below
        let (_, ele) = cartesian_to_spherical(DVec3::new(0.0, 0.0, -1.0));
        assert_relative_eq!(ele, -90.0, epsilon = 1e-9);

        // Zero vector
        let (azi, ele) = cartesian_to_spherical(DVec3::ZERO);
        assert_relative_eq!(azi, 0.0, epsilon = 1e-9);
        assert_relative_eq!(ele, 0.0, epsilon = 1e-9);
    }
}
