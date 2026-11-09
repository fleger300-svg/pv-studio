//! Ear-clipping triangulation of simple polygons, and the exact intersection area of two
//! simple polygons (sum over triangle pairs of convex clips).

use crate::convex::{clip_convex, signed_area, HalfPlane};
use crate::{Vec2, EPS};

/// Triangles (vertex indices, counter-clockwise) of a simple counter-clockwise polygon.
/// O(n²) ear clipping; collinear vertices are fine.
#[must_use]
pub fn triangulate(poly: &[Vec2]) -> Vec<[usize; 3]> {
    let mut idx: Vec<usize> = (0..poly.len()).collect();
    let mut out = Vec::with_capacity(poly.len().saturating_sub(2));
    let mut guard = 0;
    while idx.len() > 3 && guard < 4 * poly.len() * poly.len() {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let (i0, i1, i2) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (poly[i0], poly[i1], poly[i2]);
            let turn = (b - a).cross(c - b);
            if turn < 0.0 {
                continue; // reflex
            }
            if turn <= EPS * EPS {
                // Collinear (or spike) vertex: drop it without a triangle.
                idx.remove(k);
                clipped = true;
                break;
            }
            let inside = idx.iter().any(|&j| {
                j != i0 && j != i1 && j != i2 && {
                    let p = poly[j];
                    (b - a).cross(p - a) >= 0.0
                        && (c - b).cross(p - b) >= 0.0
                        && (a - c).cross(p - c) >= 0.0
                        && p.distance(a) > EPS
                        && p.distance(b) > EPS
                        && p.distance(c) > EPS
                }
            });
            if !inside {
                out.push([i0, i1, i2]);
                idx.remove(k);
                clipped = true;
                break;
            }
        }
        if !clipped {
            break; // not simple: give up on the rest
        }
    }
    if idx.len() == 3 {
        let (a, b, c) = (poly[idx[0]], poly[idx[1]], poly[idx[2]]);
        if (b - a).cross(c - b) > EPS * EPS {
            out.push([idx[0], idx[1], idx[2]]);
        }
    }
    out
}

/// Area of the intersection of two simple counter-clockwise polygons.
#[must_use]
pub fn intersection_area(a: &[Vec2], b: &[Vec2]) -> f64 {
    let ta = triangulate(a);
    let tb = triangulate(b);
    let mut total = 0.0;
    for t in &ta {
        let tri_a = [a[t[0]], a[t[1]], a[t[2]]];
        for s in &tb {
            let tri_b = [b[s[0]], b[s[1]], b[s[2]]];
            let mut piece: Vec<Vec2> = tri_a.to_vec();
            for k in 0..3 {
                let (p, q) = (tri_b[k], tri_b[(k + 1) % 3]);
                let d = q - p;
                // Keep the left side of p → q: outward normal (d.v, −d.u).
                let normal = Vec2::new(d.v, -d.u);
                piece = clip_convex(
                    &piece,
                    HalfPlane {
                        normal,
                        offset: normal.dot(p),
                    },
                    0.0,
                );
                if piece.is_empty() {
                    break;
                }
            }
            total += signed_area(&piece).max(0.0);
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l_shape() -> Vec<Vec2> {
        [
            (0.0, 0.0),
            (4.0, 0.0),
            (4.0, 1.0),
            (1.0, 1.0),
            (1.0, 3.0),
            (0.0, 3.0),
        ]
        .iter()
        .map(|&(u, v)| Vec2::new(u, v))
        .collect()
    }

    #[test]
    fn triangulation_covers_the_area() {
        let l = l_shape();
        let tris = triangulate(&l);
        assert_eq!(tris.len(), 4);
        let sum: f64 = tris
            .iter()
            .map(|t| signed_area(&[l[t[0]], l[t[1]], l[t[2]]]))
            .sum();
        assert!((sum - 6.0).abs() < 1e-12);
    }

    #[test]
    fn intersection_of_an_l_and_a_square() {
        let sq = vec![
            Vec2::new(0.5, 0.5),
            Vec2::new(2.5, 0.5),
            Vec2::new(2.5, 2.5),
            Vec2::new(0.5, 2.5),
        ];
        // L ∩ square = [0.5,2.5]×[0.5,1] ∪ [0.5,1]×[1,2.5] = 1 + 0.75.
        assert!((intersection_area(&l_shape(), &sq) - 1.75).abs() < 1e-12);
        assert!((intersection_area(&l_shape(), &l_shape()) - 6.0).abs() < 1e-12);
    }

    /// Rounding put a crossing at t = 1 exactly and the clip lost the point (found by the
    /// roof generator: two rectangles sharing edges up to a few ULPs).
    #[test]
    fn intersection_is_symmetric_under_rounding() {
        let a = vec![
            Vec2::new(24.5, -9.8),
            Vec2::new(57.5, -9.799999999999997),
            Vec2::new(57.5, 3.552713678800501e-15),
            Vec2::new(24.5, 0.0),
        ];
        let b = vec![
            Vec2::new(24.5, -9.800000000000002),
            Vec2::new(57.5, -9.800000000000002),
            Vec2::new(57.5, 9.799999999999999),
            Vec2::new(24.499999999999996, 9.799999999999999),
        ];
        assert!((intersection_area(&a, &b) - 323.4).abs() < 1e-9);
        assert!((intersection_area(&b, &a) - 323.4).abs() < 1e-9);
    }
}
