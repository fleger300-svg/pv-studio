//! Convex-polygon tools for the roof generator: half-plane clipping, mitered offset, convex
//! hull and the minimum-area enclosing rectangle.

use crate::{OrientedRect, Vec2, EPS};

/// The closed half-plane `{ p : normal · p ≤ offset }`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HalfPlane {
    pub normal: Vec2,
    pub offset: f64,
}

impl HalfPlane {
    /// Signed value: ≤ 0 inside.
    #[must_use]
    pub fn eval(&self, p: Vec2) -> f64 {
        self.normal.dot(p) - self.offset
    }
}

/// Clips a convex polygon (any orientation) by a half-plane (Sutherland–Hodgman). Points
/// within `tol` of the boundary count as inside, so shared edges are kept exactly; the
/// result keeps the orientation and drops repeated vertices. Empty if nothing remains.
#[must_use]
pub fn clip_convex(poly: &[Vec2], h: HalfPlane, tol: f64) -> Vec<Vec2> {
    let n = poly.len();
    let mut out: Vec<Vec2> = Vec::with_capacity(n + 1);
    let push = |p: Vec2, out: &mut Vec<Vec2>| {
        if out.last().is_none_or(|q: &Vec2| q.distance(p) > EPS) {
            out.push(p);
        }
    };
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (fa, fb) = (h.eval(a), h.eval(b));
        let (ina, inb) = (fa <= tol, fb <= tol);
        if ina {
            push(a, &mut out);
        }
        if ina != inb {
            // The boundary point between a and b. Signs differ, so fa ≠ fb; t is clamped:
            // rounding may put it at 0 or 1, and the point must not be lost then.
            let t = (fa / (fa - fb)).clamp(0.0, 1.0);
            push(a + (b - a) * t, &mut out);
        }
    }
    while out.len() > 1 && out[0].distance(out[out.len() - 1]) <= EPS {
        out.pop();
    }
    if out.len() < 3 || signed_area(&out).abs() <= EPS {
        return Vec::new();
    }
    out
}

/// Twice the signed area (> 0 counter-clockwise).
#[must_use]
pub fn signed_area(poly: &[Vec2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| poly[i].cross(poly[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

/// Whether a counter-clockwise polygon is convex (collinear vertices allowed).
#[must_use]
pub fn is_convex(poly: &[Vec2]) -> bool {
    let n = poly.len();
    (0..n).all(|i| {
        let (a, b, c) = (poly[i], poly[(i + 1) % n], poly[(i + 2) % n]);
        (b - a).cross(c - b) >= -1e-12 * (b - a).length() * (c - b).length()
    })
}

/// Unit outward normal of the edge `a → b` of a counter-clockwise polygon.
#[must_use]
pub fn outward_normal(a: Vec2, b: Vec2) -> Vec2 {
    let d = b - a;
    let l = d.length();
    Vec2::new(d.v / l, -d.u / l)
}

/// Moves every edge of a counter-clockwise polygon outward by `d` (inward if negative) and
/// joins the moved lines (mitered corners). Collinear neighbours move their shared vertex
/// along the common normal. The result may self-intersect when `|d|` is large compared with
/// short edges of a concave polygon; callers validate it.
#[must_use]
pub fn offset_mitered(poly: &[Vec2], d: f64) -> Vec<Vec2> {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let prev = poly[(i + n - 1) % n];
            let (p, next) = (poly[i], poly[(i + 1) % n]);
            let (n0, n1) = (outward_normal(prev, p), outward_normal(p, next));
            let denom = 1.0 + n0.dot(n1);
            if denom < 1e-12 {
                // A spike (edges folding back): no finite miter.
                return p + n1 * d;
            }
            // The miter point m = p + k (n0 + n1) satisfies m·n0 = p·n0 + d and m·n1 = p·n1 + d.
            p + (n0 + n1) * (d / denom)
        })
        .collect()
}

/// Convex hull, counter-clockwise, without collinear points (Andrew's monotone chain).
#[must_use]
pub fn convex_hull(points: &[Vec2]) -> Vec<Vec2> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a.u.total_cmp(&b.u).then(a.v.total_cmp(&b.v)));
    pts.dedup_by(|a, b| a.distance(*b) <= EPS);
    if pts.len() < 3 {
        return pts;
    }
    let mut hull: Vec<Vec2> = Vec::with_capacity(2 * pts.len());
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &Vec2>> = if pass == 0 {
            Box::new(pts.iter())
        } else {
            Box::new(pts.iter().rev())
        };
        for &p in iter {
            while hull.len() >= start + 2 {
                let (a, b) = (hull[hull.len() - 2], hull[hull.len() - 1]);
                if (b - a).cross(p - b) <= EPS * EPS {
                    hull.pop();
                } else {
                    break;
                }
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Minimum-area rectangle enclosing the points (rotating calipers over the hull edges), with
/// its long side along its `u` axis (`half_u ≥ half_v`).
#[must_use]
pub fn min_area_rect(points: &[Vec2]) -> Option<OrientedRect> {
    let hull = convex_hull(points);
    if hull.len() < 3 {
        return None;
    }
    let n = hull.len();
    let mut best: Option<(f64, OrientedRect)> = None;
    for i in 0..n {
        let d = hull[(i + 1) % n] - hull[i];
        let ex = d * (1.0 / d.length());
        let ey = Vec2::new(-ex.v, ex.u);
        let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for &p in &hull {
            let (x, y) = (p.dot(ex), p.dot(ey));
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
        }
        let area = (x1 - x0) * (y1 - y0);
        if best.as_ref().is_none_or(|(a, _)| area < *a - EPS) {
            let centre = ex * ((x0 + x1) / 2.0) + ey * ((y0 + y1) / 2.0);
            let (hx, hy) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
            let angle = ex.v.atan2(ex.u);
            let rect = if hx >= hy {
                OrientedRect {
                    centre,
                    half_u: hx,
                    half_v: hy,
                    angle_rad: angle,
                }
            } else {
                OrientedRect {
                    centre,
                    half_u: hy,
                    half_v: hx,
                    angle_rad: angle + std::f64::consts::FRAC_PI_2,
                }
            };
            best = Some((area, rect));
        }
    }
    best.map(|(_, r)| r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Vec2> {
        vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(0.0, 2.0),
        ]
    }

    #[test]
    fn clip_halves_a_square() {
        let h = HalfPlane {
            normal: Vec2::new(1.0, 0.0),
            offset: 1.0,
        };
        let c = clip_convex(&square(), h, 1e-12);
        assert!((signed_area(&c) - 2.0).abs() < 1e-12);
        // Through a vertex diagonal: a triangle.
        let d = HalfPlane {
            normal: Vec2::new(1.0, 1.0),
            offset: 2.0,
        };
        let t = clip_convex(&square(), d, 1e-12);
        assert_eq!(t.len(), 3);
        assert!((signed_area(&t) - 2.0).abs() < 1e-12);
        // Everything outside, or a mere touch: empty.
        let out = HalfPlane {
            normal: Vec2::new(1.0, 0.0),
            offset: 0.0,
        };
        assert!(clip_convex(&square(), out, 1e-12).is_empty());
    }

    #[test]
    fn offset_of_a_square_and_a_triangle() {
        let o = offset_mitered(&square(), 0.5);
        assert!((signed_area(&o) - 9.0).abs() < 1e-12);
        let tri = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(4.0, 0.0),
            Vec2::new(0.0, 3.0),
        ];
        let back = offset_mitered(&offset_mitered(&tri, 0.3), -0.3);
        for (a, b) in tri.iter().zip(&back) {
            assert!(a.distance(*b) < 1e-12);
        }
    }

    #[test]
    fn hull_and_min_rect_of_a_rotated_rectangle() {
        let a = 0.3_f64;
        let r = |x: f64, y: f64| Vec2::new(x, y).rotated(a);
        let pts = vec![
            r(0.0, 0.0),
            r(6.0, 0.0),
            r(6.0, 2.0),
            r(0.0, 2.0),
            r(3.0, 1.0),
            r(6.0, 1.0),
        ];
        assert_eq!(convex_hull(&pts).len(), 4);
        let m = min_area_rect(&pts).expect("rect");
        assert!((m.half_u - 3.0).abs() < 1e-9 && (m.half_v - 1.0).abs() < 1e-9);
        let dir = Vec2::new(m.angle_rad.cos(), m.angle_rad.sin());
        assert!(
            dir.cross(Vec2::new(a.cos(), a.sin())).abs() < 1e-9,
            "long side along the rectangle"
        );
        assert!(is_convex(&convex_hull(&pts)));
    }
}
