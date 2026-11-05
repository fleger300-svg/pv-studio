//! Minimal vector/quaternion maths and the roof-face local frame.
//!
//! Roof local coordinates `(u, v)`:
//! - `origin` is the world position of `(0, 0)`: the lower-left corner of the face when
//!   standing downslope and looking up the roof.
//! - `u` runs horizontally along the eave, to the right of that viewer.
//! - `v` runs up the slope (for a flat roof with azimuth 180°: north).
//! - `w` is the offset along the outward normal `n = u × v`.

use serde::{Deserialize, Serialize};
use specta::Type;

/// A 3-D vector in metres (world ENU unless stated otherwise).
pub type Vec3 = [f64; 3];

#[must_use]
pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[must_use]
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[must_use]
pub fn scale(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[must_use]
pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[must_use]
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Unit quaternion `(x, y, z, w)`, same component order as three.js.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct Quat {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Quat {
    pub const IDENTITY: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    /// Rotation of `angle_rad` around the unit `axis`.
    #[must_use]
    pub fn from_axis_angle(axis: Vec3, angle_rad: f64) -> Self {
        let (s, c) = (angle_rad / 2.0).sin_cos();
        Self {
            x: axis[0] * s,
            y: axis[1] * s,
            z: axis[2] * s,
            w: c,
        }
    }

    /// Rotation whose columns are the given orthonormal basis vectors (right-handed).
    #[must_use]
    pub fn from_basis(x: Vec3, y: Vec3, z: Vec3) -> Self {
        // Shepperd's method on the rotation matrix m[row][col] with columns x, y, z.
        let (m00, m01, m02) = (x[0], y[0], z[0]);
        let (m10, m11, m12) = (x[1], y[1], z[1]);
        let (m20, m21, m22) = (x[2], y[2], z[2]);
        let trace = m00 + m11 + m22;
        let q = if trace > 0.0 {
            let s = 0.5 / (trace + 1.0).sqrt();
            Self {
                w: 0.25 / s,
                x: (m21 - m12) * s,
                y: (m02 - m20) * s,
                z: (m10 - m01) * s,
            }
        } else if m00 > m11 && m00 > m22 {
            let s = 2.0 * (1.0 + m00 - m11 - m22).sqrt();
            Self {
                w: (m21 - m12) / s,
                x: 0.25 * s,
                y: (m01 + m10) / s,
                z: (m02 + m20) / s,
            }
        } else if m11 > m22 {
            let s = 2.0 * (1.0 + m11 - m00 - m22).sqrt();
            Self {
                w: (m02 - m20) / s,
                x: (m01 + m10) / s,
                y: 0.25 * s,
                z: (m12 + m21) / s,
            }
        } else {
            let s = 2.0 * (1.0 + m22 - m00 - m11).sqrt();
            Self {
                w: (m10 - m01) / s,
                x: (m02 + m20) / s,
                y: (m12 + m21) / s,
                z: 0.25 * s,
            }
        };
        q.normalized()
    }

    #[must_use]
    pub fn normalized(self) -> Self {
        let n = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        Self {
            x: self.x / n,
            y: self.y / n,
            z: self.z / n,
            w: self.w / n,
        }
    }

    /// Rotates vector `v`.
    #[must_use]
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let q = [self.x, self.y, self.z];
        let t = scale(cross(q, v), 2.0);
        add(add(v, scale(t, self.w)), cross(q, t))
    }
}

impl std::ops::Mul for Quat {
    type Output = Self;

    /// Hamilton product `self * r` (applies `r` first, then `self`).
    fn mul(self, r: Self) -> Self {
        Self {
            w: self.w * r.w - self.x * r.x - self.y * r.y - self.z * r.z,
            x: self.w * r.x + self.x * r.w + self.y * r.z - self.z * r.y,
            y: self.w * r.y - self.x * r.z + self.y * r.w + self.z * r.x,
            z: self.w * r.z + self.x * r.y - self.y * r.x + self.z * r.w,
        }
    }
}

/// Orthonormal frame of a planar roof face, derived from tilt and azimuth.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct RoofFrame {
    /// World position of roof `(u, v) = (0, 0)`.
    pub origin: Vec3,
    /// Unit vector of the `u` axis (horizontal, along the eave).
    pub axis_u: Vec3,
    /// Unit vector of the `v` axis (up the slope).
    pub axis_v: Vec3,
    /// Outward unit normal (`axis_u × axis_v`).
    pub normal: Vec3,
}

impl RoofFrame {
    /// Builds the frame of a face whose downslope side faces compass `azimuth_deg` and which
    /// is tilted `tilt_deg` from horizontal.
    #[must_use]
    pub fn new(origin: Vec3, tilt_deg: f64, azimuth_deg: f64) -> Self {
        let (sa, ca) = azimuth_deg.to_radians().sin_cos();
        let (st, ct) = tilt_deg.to_radians().sin_cos();
        // Downslope horizontal direction h = (sin A, cos A, 0). Viewer looks up-slope (-h);
        // their right-hand side is (-cos A, sin A, 0).
        let axis_u = [-ca, sa, 0.0];
        let axis_v = [-sa * ct, -ca * ct, st];
        let normal = cross(axis_u, axis_v);
        Self {
            origin,
            axis_u,
            axis_v,
            normal,
        }
    }

    /// Roof `(u, v, w)` → world ENU.
    #[must_use]
    pub fn to_world(&self, u: f64, v: f64, w: f64) -> Vec3 {
        add(
            self.origin,
            add(
                add(scale(self.axis_u, u), scale(self.axis_v, v)),
                scale(self.normal, w),
            ),
        )
    }

    /// World ENU → roof `(u, v, w)`.
    #[must_use]
    pub fn to_local(&self, p: Vec3) -> (f64, f64, f64) {
        let d = sub(p, self.origin);
        (
            dot(d, self.axis_u),
            dot(d, self.axis_v),
            dot(d, self.normal),
        )
    }

    /// Rotation that maps local axes (x, y, z) onto (u, v, n).
    #[must_use]
    pub fn rotation(&self) -> Quat {
        Quat::from_basis(self.axis_u, self.axis_v, self.normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn south_roof_axes() {
        let f = RoofFrame::new([0.0; 3], 30.0, 180.0);
        assert!(close(f.axis_u, [1.0, 0.0, 0.0]), "u points east");
        let (s, c) = 30f64.to_radians().sin_cos();
        assert!(close(f.axis_v, [0.0, c, s]), "v rises northwards");
        assert!(close(f.normal, [0.0, -s, c]), "normal faces south and up");
    }

    #[test]
    fn north_roof_axes() {
        let f = RoofFrame::new([0.0; 3], 30.0, 0.0);
        assert!(close(f.axis_u, [-1.0, 0.0, 0.0]), "u points west");
        assert!(
            f.normal[1] > 0.0 && f.normal[2] > 0.0,
            "normal faces north and up"
        );
    }

    #[test]
    fn uv_world_round_trip() {
        let f = RoofFrame::new([12.5, -3.0, 6.0], 27.0, 213.0);
        for &(u, v, w) in &[(0.0, 0.0, 0.0), (3.2, 7.9, 0.1), (-1.0, 15.0, -0.4)] {
            let p = f.to_world(u, v, w);
            let (u2, v2, w2) = f.to_local(p);
            assert!((u - u2).abs() < 1e-9 && (v - v2).abs() < 1e-9 && (w - w2).abs() < 1e-9);
        }
    }

    #[test]
    fn frame_rotation_maps_axes() {
        let f = RoofFrame::new([0.0; 3], 35.0, 140.0);
        let q = f.rotation();
        assert!(close(q.rotate([1.0, 0.0, 0.0]), f.axis_u));
        assert!(close(q.rotate([0.0, 1.0, 0.0]), f.axis_v));
        assert!(close(q.rotate([0.0, 0.0, 1.0]), f.normal));
    }

    #[test]
    fn quaternion_product_composes() {
        let a = Quat::from_axis_angle([0.0, 0.0, 1.0], 0.7);
        let b = Quat::from_axis_angle([1.0, 0.0, 0.0], -0.3);
        let v = [0.3, -1.2, 2.0];
        assert!(close((a * b).rotate(v), a.rotate(b.rotate(v))));
    }
}
