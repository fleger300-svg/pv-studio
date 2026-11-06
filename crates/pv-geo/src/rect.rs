use serde::{Deserialize, Serialize};

use crate::{Polygon, Segment, Vec2};

/// Rectangle centred at `centre`, with half extents along its own axes, rotated
/// counter-clockwise by `angle_rad` from the roof `u` axis. A module footprint.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OrientedRect {
    pub centre: Vec2,
    pub half_u: f64,
    pub half_v: f64,
    #[serde(default)]
    pub angle_rad: f64,
}

impl OrientedRect {
    /// Axis-aligned rectangle from its centre and full size.
    #[must_use]
    pub fn axis_aligned(centre: Vec2, size_u: f64, size_v: f64) -> Self {
        Self {
            centre,
            half_u: size_u / 2.0,
            half_v: size_v / 2.0,
            angle_rad: 0.0,
        }
    }

    /// Axis-aligned rectangle from two opposite corners.
    #[must_use]
    pub fn from_corners(a: Vec2, b: Vec2) -> Self {
        let centre = (a + b) * 0.5;
        Self::axis_aligned(centre, (b.u - a.u).abs(), (b.v - a.v).abs())
    }

    #[must_use]
    pub fn rotated(mut self, angle_rad: f64) -> Self {
        self.angle_rad += angle_rad;
        self
    }

    /// Corners, counter-clockwise, starting at the local (−u, −v) corner.
    #[must_use]
    pub fn corners(&self) -> [Vec2; 4] {
        let (hu, hv) = (self.half_u, self.half_v);
        [
            Vec2::new(-hu, -hv),
            Vec2::new(hu, -hv),
            Vec2::new(hu, hv),
            Vec2::new(-hu, hv),
        ]
        .map(|p| self.centre + p.rotated(self.angle_rad))
    }

    #[must_use]
    pub fn edges(&self) -> [Segment; 4] {
        let c = self.corners();
        [
            Segment::new(c[0], c[1]),
            Segment::new(c[1], c[2]),
            Segment::new(c[2], c[3]),
            Segment::new(c[3], c[0]),
        ]
    }

    #[must_use]
    pub fn area(&self) -> f64 {
        4.0 * self.half_u * self.half_v
    }

    /// Whether `p` is inside or on the rectangle.
    #[must_use]
    pub fn contains_point(&self, p: Vec2) -> bool {
        let local = (p - self.centre).rotated(-self.angle_rad);
        local.u.abs() <= self.half_u + crate::EPS && local.v.abs() <= self.half_v + crate::EPS
    }

    /// Minimum distance from the rectangle to a segment (0 if they touch or overlap).
    #[must_use]
    pub fn distance_to_segment(&self, s: &Segment) -> f64 {
        if self.contains_point(s.a) || self.contains_point(s.b) {
            return 0.0;
        }
        self.edges()
            .iter()
            .map(|e| e.distance_to_segment(s))
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether the two rectangles overlap with positive area. Touching edges do not count:
    /// adjacent modules share an edge when the module gap is zero.
    #[must_use]
    pub fn overlaps(&self, o: &OrientedRect) -> bool {
        // Separating axis theorem on the 4 face normals, with a strict tolerance.
        let axes = [
            Vec2::new(1.0, 0.0).rotated(self.angle_rad),
            Vec2::new(0.0, 1.0).rotated(self.angle_rad),
            Vec2::new(1.0, 0.0).rotated(o.angle_rad),
            Vec2::new(0.0, 1.0).rotated(o.angle_rad),
        ];
        let (ca, cb) = (self.corners(), o.corners());
        axes.iter().all(|axis| {
            let (amin, amax) = project(&ca, *axis);
            let (bmin, bmax) = project(&cb, *axis);
            amax > bmin + crate::EPS && bmax > amin + crate::EPS
        })
    }

    #[must_use]
    pub fn to_polygon(&self) -> Polygon {
        Polygon::new(self.corners().to_vec())
            .expect("a non-degenerate rectangle is a valid polygon")
    }
}

fn project(points: &[Vec2], axis: Vec2) -> (f64, f64) {
    points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            let d = p.dot(axis);
            (lo.min(d), hi.max(d))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_and_rotation() {
        let r = OrientedRect::axis_aligned(Vec2::new(1.0, 1.0), 2.0, 1.0);
        assert_eq!(r.corners()[0], Vec2::new(0.0, 0.5));
        let q = r.rotated(std::f64::consts::FRAC_PI_2);
        let c = q.corners()[0];
        assert!((c.u - 1.5).abs() < 1e-12 && (c.v - 0.0).abs() < 1e-12);
    }

    #[test]
    fn overlap_excludes_touching() {
        let a = OrientedRect::axis_aligned(Vec2::new(0.5, 0.5), 1.0, 1.0);
        let b = OrientedRect::axis_aligned(Vec2::new(1.5, 0.5), 1.0, 1.0);
        let c = OrientedRect::axis_aligned(Vec2::new(1.4, 0.5), 1.0, 1.0);
        assert!(!a.overlaps(&b));
        assert!(a.overlaps(&c));
        // A diamond near the corner: axis-aligned boxes would overlap, the SAT says no.
        let d = OrientedRect::axis_aligned(Vec2::new(1.75, 1.75), 1.0, 1.0)
            .rotated(std::f64::consts::FRAC_PI_4);
        assert!(!a.overlaps(&d));
    }

    #[test]
    fn distance_to_segment() {
        let r = OrientedRect::axis_aligned(Vec2::new(0.0, 0.0), 2.0, 2.0);
        let s = Segment::new(Vec2::new(-5.0, 3.0), Vec2::new(5.0, 3.0));
        assert!((r.distance_to_segment(&s) - 2.0).abs() < 1e-12);
        let inside = Segment::new(Vec2::new(-0.1, 0.0), Vec2::new(0.1, 0.0));
        assert_eq!(r.distance_to_segment(&inside), 0.0);
    }
}
