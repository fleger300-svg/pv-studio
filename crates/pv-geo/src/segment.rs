use serde::{Deserialize, Serialize};

use crate::{Vec2, EPS};

/// Closed segment from `a` to `b`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub a: Vec2,
    pub b: Vec2,
}

impl Segment {
    #[must_use]
    pub const fn new(a: Vec2, b: Vec2) -> Self {
        Self { a, b }
    }

    #[must_use]
    pub fn length(&self) -> f64 {
        self.a.distance(self.b)
    }

    #[must_use]
    pub fn direction(&self) -> Vec2 {
        self.b - self.a
    }

    /// Closest point of the segment to `p`.
    #[must_use]
    pub fn closest_point(&self, p: Vec2) -> Vec2 {
        let d = self.direction();
        let len2 = d.dot(d);
        if len2 <= EPS * EPS {
            return self.a;
        }
        let t = ((p - self.a).dot(d) / len2).clamp(0.0, 1.0);
        self.a + d * t
    }

    #[must_use]
    pub fn distance_to_point(&self, p: Vec2) -> f64 {
        self.closest_point(p).distance(p)
    }

    /// Whether the two closed segments share at least one point (touching counts).
    #[must_use]
    pub fn intersects(&self, o: &Segment) -> bool {
        let o1 = orient(self.a, self.b, o.a);
        let o2 = orient(self.a, self.b, o.b);
        let o3 = orient(o.a, o.b, self.a);
        let o4 = orient(o.a, o.b, self.b);
        if o1 * o2 < 0 && o3 * o4 < 0 {
            return true;
        }
        (o1 == 0 && on_segment(self.a, self.b, o.a))
            || (o2 == 0 && on_segment(self.a, self.b, o.b))
            || (o3 == 0 && on_segment(o.a, o.b, self.a))
            || (o4 == 0 && on_segment(o.a, o.b, self.b))
    }

    /// Whether the segments cross at a point interior to both (touching at an end does not
    /// count, nor does collinear overlap). Used to detect self-intersecting polygons.
    #[must_use]
    pub fn crosses_properly(&self, o: &Segment) -> bool {
        let o1 = orient(self.a, self.b, o.a);
        let o2 = orient(self.a, self.b, o.b);
        let o3 = orient(o.a, o.b, self.a);
        let o4 = orient(o.a, o.b, self.b);
        o1 * o2 < 0 && o3 * o4 < 0
    }

    /// Minimum distance between two segments (0 if they intersect).
    #[must_use]
    pub fn distance_to_segment(&self, o: &Segment) -> f64 {
        if self.intersects(o) {
            return 0.0;
        }
        self.distance_to_point(o.a)
            .min(self.distance_to_point(o.b))
            .min(o.distance_to_point(self.a))
            .min(o.distance_to_point(self.b))
    }
}

/// Orientation of `c` relative to the directed line `a → b`: 1 left, −1 right, 0 collinear
/// (within [`EPS`] scaled by the segment length).
fn orient(a: Vec2, b: Vec2, c: Vec2) -> i8 {
    let cross = (b - a).cross(c - a);
    let scale = (b - a).length().max(1.0);
    if cross > EPS * scale {
        1
    } else if cross < -EPS * scale {
        -1
    } else {
        0
    }
}

fn on_segment(a: Vec2, b: Vec2, p: Vec2) -> bool {
    p.u >= a.u.min(b.u) - EPS
        && p.u <= a.u.max(b.u) + EPS
        && p.v >= a.v.min(b.v) - EPS
        && p.v <= a.v.max(b.v) + EPS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(a: (f64, f64), b: (f64, f64)) -> Segment {
        Segment::new(Vec2::new(a.0, a.1), Vec2::new(b.0, b.1))
    }

    #[test]
    fn point_distance() {
        let seg = s((0.0, 0.0), (10.0, 0.0));
        assert!((seg.distance_to_point(Vec2::new(5.0, 3.0)) - 3.0).abs() < 1e-12);
        assert!((seg.distance_to_point(Vec2::new(-4.0, 3.0)) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn intersections() {
        assert!(s((0.0, 0.0), (2.0, 2.0)).intersects(&s((0.0, 2.0), (2.0, 0.0))));
        assert!(s((0.0, 0.0), (2.0, 0.0)).intersects(&s((2.0, 0.0), (3.0, 1.0)))); // touching
        assert!(!s((0.0, 0.0), (2.0, 0.0)).crosses_properly(&s((2.0, 0.0), (3.0, 1.0))));
        assert!(!s((0.0, 0.0), (1.0, 0.0)).intersects(&s((0.0, 1.0), (1.0, 1.0))));
        assert!(
            (s((0.0, 0.0), (1.0, 0.0)).distance_to_segment(&s((0.0, 1.0), (1.0, 1.0))) - 1.0).abs()
                < 1e-12
        );
    }
}
