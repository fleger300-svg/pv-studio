use serde::{Deserialize, Serialize};

use crate::{OrientedRect, Segment, Vec2, EPS};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PolygonError {
    #[error("a polygon needs at least 3 distinct vertices, got {0}")]
    TooFewVertices(usize),
    #[error("vertex {0} is not a finite number")]
    NotFinite(usize),
    #[error("the polygon has zero area")]
    Degenerate,
    #[error("edges {0} and {1} cross: the polygon is not simple")]
    SelfIntersecting(usize, usize),
}

/// Simple polygon (convex or concave, no holes), stored counter-clockwise. Edge `i` goes from
/// vertex `i` to vertex `i + 1` (wrapping).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Polygon {
    vertices: Vec<Vec2>,
}

impl<'de> Deserialize<'de> for Polygon {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            vertices: Vec<Vec2>,
        }
        let raw = Raw::deserialize(d)?;
        Polygon::new(raw.vertices).map_err(serde::de::Error::custom)
    }
}

impl Polygon {
    /// Validates and normalises: drops repeated consecutive vertices and the closing vertex,
    /// reorders counter-clockwise, rejects degenerate and self-intersecting outlines.
    pub fn new(vertices: Vec<Vec2>) -> Result<Self, PolygonError> {
        Self::new_with_edge_map(vertices).map(|(p, _)| p)
    }

    /// Like [`Polygon::new`], also returning for each edge of the result the index of the
    /// input edge it comes from (input edge `i` joins input vertices `i` and `i + 1`), so
    /// that per-edge data (edge kinds, setbacks) survives normalisation.
    pub fn new_with_edge_map(vertices: Vec<Vec2>) -> Result<(Self, Vec<usize>), PolygonError> {
        if let Some(i) = vertices.iter().position(|p| !p.is_finite()) {
            return Err(PolygonError::NotFinite(i));
        }
        // Keep the first vertex of each run of coincident vertices; the run's last index is
        // the input edge leaving the kept vertex.
        let mut kept: Vec<Vec2> = Vec::new();
        let mut out_edge: Vec<usize> = Vec::new();
        for (i, &p) in vertices.iter().enumerate() {
            match kept.last() {
                Some(&last) if last.distance(p) <= EPS => {
                    *out_edge.last_mut().expect("parallel vectors") = i
                }
                _ => {
                    kept.push(p);
                    out_edge.push(i);
                }
            }
        }
        while kept.len() > 1 && kept[0].distance(kept[kept.len() - 1]) <= EPS {
            kept.pop();
            out_edge.pop();
        }
        if kept.len() < 3 {
            return Err(PolygonError::TooFewVertices(kept.len()));
        }
        let area = signed_area(&kept);
        if area.abs() <= EPS {
            return Err(PolygonError::Degenerate);
        }
        let m = kept.len();
        let edge_map: Vec<usize> = if area < 0.0 {
            kept.reverse();
            // New edge k joins old kept vertices m−1−k → m−2−k, i.e. the old edge leaving m−2−k.
            (0..m).map(|k| out_edge[(2 * m - 2 - k) % m]).collect()
        } else {
            out_edge
        };
        let p = Polygon { vertices: kept };
        p.check_simple()?;
        Ok((p, edge_map))
    }

    /// Axis-aligned rectangle [u0, u1] × [v0, v1].
    #[must_use]
    pub fn rectangle(u0: f64, v0: f64, u1: f64, v1: f64) -> Self {
        Polygon::new(vec![
            Vec2::new(u0, v0),
            Vec2::new(u1, v0),
            Vec2::new(u1, v1),
            Vec2::new(u0, v1),
        ])
        .expect("non-degenerate rectangle")
    }

    #[must_use]
    pub fn vertices(&self) -> &[Vec2] {
        &self.vertices
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    /// Always false: a valid polygon has at least 3 vertices.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    #[must_use]
    pub fn edge(&self, i: usize) -> Segment {
        let n = self.vertices.len();
        Segment::new(self.vertices[i % n], self.vertices[(i + 1) % n])
    }

    pub fn edges(&self) -> impl Iterator<Item = Segment> + '_ {
        (0..self.vertices.len()).map(|i| self.edge(i))
    }

    #[must_use]
    pub fn area(&self) -> f64 {
        signed_area(&self.vertices)
    }

    /// (min, max) corners of the bounding box.
    #[must_use]
    pub fn bounds(&self) -> (Vec2, Vec2) {
        self.vertices.iter().fold(
            (
                Vec2::new(f64::INFINITY, f64::INFINITY),
                Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
            ),
            |(lo, hi), p| {
                (
                    Vec2::new(lo.u.min(p.u), lo.v.min(p.v)),
                    Vec2::new(hi.u.max(p.u), hi.v.max(p.v)),
                )
            },
        )
    }

    /// Whether `p` is inside or on the boundary (within [`EPS`]).
    #[must_use]
    pub fn contains_point(&self, p: Vec2) -> bool {
        if self.edges().any(|e| e.distance_to_point(p) <= EPS) {
            return true;
        }
        // Even-odd ray casting along +u.
        let mut inside = false;
        let n = self.vertices.len();
        let mut j = n - 1;
        for i in 0..n {
            let (a, b) = (self.vertices[i], self.vertices[j]);
            if (a.v > p.v) != (b.v > p.v) {
                let u_cross = a.u + (p.v - a.v) / (b.v - a.v) * (b.u - a.u);
                if p.u < u_cross {
                    inside = !inside;
                }
            }
            j = i;
        }
        inside
    }

    /// Distance from `p` to the boundary.
    #[must_use]
    pub fn distance_to_boundary(&self, p: Vec2) -> f64 {
        self.edges()
            .map(|e| e.distance_to_point(p))
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether the rectangle lies entirely inside the polygon (boundary contact allowed).
    /// For a concave polygon, corners inside is not enough: no polygon edge may enter the
    /// rectangle's interior either.
    #[must_use]
    pub fn contains_rect(&self, r: &OrientedRect) -> bool {
        if !r.corners().iter().all(|&c| self.contains_point(c)) {
            return false;
        }
        // A reflex vertex strictly inside the rectangle means the outline cuts into it.
        let shrunk = OrientedRect {
            half_u: r.half_u - 10.0 * EPS,
            half_v: r.half_v - 10.0 * EPS,
            ..*r
        };
        if shrunk.half_u <= 0.0 || shrunk.half_v <= 0.0 {
            return true;
        }
        if self.vertices.iter().any(|&v| shrunk.contains_point(v)) {
            return false;
        }
        // An edge crossing through the interior without a vertex inside it.
        !self
            .edges()
            .any(|e| shrunk.edges().iter().any(|re| re.crosses_properly(&e)))
    }

    /// Whether the rectangle and the polygon overlap with positive area.
    #[must_use]
    pub fn overlaps_rect(&self, r: &OrientedRect) -> bool {
        let shrunk = OrientedRect {
            half_u: r.half_u - 10.0 * EPS,
            half_v: r.half_v - 10.0 * EPS,
            ..*r
        };
        if shrunk.half_u <= 0.0 || shrunk.half_v <= 0.0 {
            return false;
        }
        if shrunk
            .corners()
            .iter()
            .any(|&c| self.contains_point(c) && self.distance_to_boundary(c) > EPS)
        {
            return true;
        }
        if self.vertices.iter().any(|&v| shrunk.contains_point(v)) {
            return true;
        }
        self.edges()
            .any(|e| shrunk.edges().iter().any(|re| re.crosses_properly(&e)))
    }

    /// Minimum distance between the rectangle and the polygon boundary.
    #[must_use]
    pub fn distance_to_rect(&self, r: &OrientedRect) -> f64 {
        self.edges()
            .map(|e| r.distance_to_segment(&e))
            .fold(f64::INFINITY, f64::min)
    }

    fn check_simple(&self) -> Result<(), PolygonError> {
        let n = self.vertices.len();
        for i in 0..n {
            for j in (i + 1)..n {
                let adjacent = j == i + 1 || (i == 0 && j == n - 1);
                let (ei, ej) = (self.edge(i), self.edge(j));
                let bad = if adjacent {
                    // Adjacent edges folding back onto each other.
                    let shared = if j == i + 1 { ei.b } else { ei.a };
                    let other_i = if j == i + 1 { ei.a } else { ei.b };
                    let other_j = if j == i + 1 { ej.b } else { ej.a };
                    let (di, dj) = (other_i - shared, other_j - shared);
                    di.cross(dj).abs() <= EPS * di.length().max(1.0) && di.dot(dj) > 0.0
                } else {
                    ei.intersects(&ej)
                };
                if bad {
                    return Err(PolygonError::SelfIntersecting(i, j));
                }
            }
        }
        Ok(())
    }
}

fn signed_area(v: &[Vec2]) -> f64 {
    let n = v.len();
    (0..n).map(|i| v[i].cross(v[(i + 1) % n])).sum::<f64>() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(u: f64, v: f64) -> Vec2 {
        Vec2::new(u, v)
    }

    /// L-shape: 10 × 6 with the top-right 4 × 3 removed.
    fn l_shape() -> Polygon {
        Polygon::new(vec![
            p(0.0, 0.0),
            p(10.0, 0.0),
            p(10.0, 3.0),
            p(6.0, 3.0),
            p(6.0, 6.0),
            p(0.0, 6.0),
        ])
        .unwrap()
    }

    #[test]
    fn normalises_orientation_and_maps_edges() {
        // Clockwise, with a duplicate vertex and the closing vertex.
        let cw = vec![
            p(0.0, 0.0),
            p(0.0, 2.0),
            p(0.0, 2.0),
            p(3.0, 2.0),
            p(3.0, 0.0),
            p(0.0, 0.0),
        ];
        let (poly, map) = Polygon::new_with_edge_map(cw.clone()).unwrap();
        assert_eq!(poly.len(), 4);
        assert!((poly.area() - 6.0).abs() < 1e-12);
        // Every result edge is geometrically its input edge, reversed.
        for (k, &i) in map.iter().enumerate() {
            let e = poly.edge(k);
            assert_eq!((e.a, e.b), (cw[i + 1], cw[i]), "edge {k} ← input edge {i}");
        }
        let ccw = vec![p(0.0, 0.0), p(3.0, 0.0), p(3.0, 2.0), p(0.0, 2.0)];
        assert_eq!(Polygon::new_with_edge_map(ccw).unwrap().1, vec![0, 1, 2, 3]);
    }

    #[test]
    fn rejects_invalid_outlines() {
        assert_eq!(
            Polygon::new(vec![p(0.0, 0.0), p(1.0, 0.0)]),
            Err(PolygonError::TooFewVertices(2))
        );
        assert_eq!(
            Polygon::new(vec![p(0.0, 0.0), p(1.0, 0.0), p(2.0, 0.0)]),
            Err(PolygonError::Degenerate)
        );
        // Asymmetric bow-tie (a symmetric one has zero area and is rejected as degenerate).
        let bowtie = vec![p(0.0, 0.0), p(2.0, 2.0), p(2.0, 0.0), p(0.0, 3.0)];
        assert!(matches!(
            Polygon::new(bowtie),
            Err(PolygonError::SelfIntersecting(..))
        ));
        assert!(matches!(
            Polygon::new(vec![p(0.0, 0.0), p(f64::NAN, 0.0), p(1.0, 1.0)]),
            Err(PolygonError::NotFinite(1))
        ));
    }

    #[test]
    fn point_containment_concave() {
        let l = l_shape();
        assert!(l.contains_point(p(2.0, 5.0)));
        assert!(l.contains_point(p(8.0, 1.0)));
        assert!(!l.contains_point(p(8.0, 5.0))); // in the notch
        assert!(l.contains_point(p(6.0, 4.0))); // on the boundary
        assert!((l.area() - 48.0).abs() < 1e-12);
    }

    #[test]
    fn rect_containment_detects_notch() {
        let l = l_shape();
        // Corners all inside the L, but the reflex corner (6, 3) is inside the rectangle.
        let straddling = OrientedRect::from_corners(p(5.0, 2.0), p(7.0, 4.0));
        assert!(
            !straddling.corners().iter().all(|&c| l.contains_point(c))
                || !l.contains_rect(&straddling)
        );
        assert!(!l.contains_rect(&straddling));
        assert!(l.contains_rect(&OrientedRect::from_corners(p(0.0, 0.0), p(10.0, 3.0))));
        assert!(l.contains_rect(&OrientedRect::from_corners(p(1.0, 3.5), p(5.0, 5.5))));
        // Corners inside both arms, edge crossing the notch without any vertex inside.
        let wide = OrientedRect::from_corners(p(5.0, 2.9), p(9.0, 3.1));
        assert!(!l.contains_rect(&wide));
    }

    #[test]
    fn overlap_and_distance() {
        let chimney = Polygon::rectangle(4.0, 4.0, 5.0, 5.0);
        let module = OrientedRect::from_corners(p(2.0, 4.2), p(3.8, 5.0));
        assert!(!chimney.overlaps_rect(&module));
        assert!((chimney.distance_to_rect(&module) - 0.2).abs() < 1e-9);
        let hit = OrientedRect::from_corners(p(3.0, 4.5), p(4.5, 6.0));
        assert!(chimney.overlaps_rect(&hit));
        // Obstacle fully inside a big rectangle.
        let big = OrientedRect::from_corners(p(0.0, 0.0), p(10.0, 10.0));
        assert!(chimney.overlaps_rect(&big));
        // Touching only.
        let touching = OrientedRect::from_corners(p(5.0, 4.0), p(6.0, 5.0));
        assert!(!chimney.overlaps_rect(&touching));
    }

    #[test]
    fn deserialisation_validates() {
        let ok: Polygon =
            serde_json::from_str(r#"{"vertices":[{"u":0,"v":0},{"u":1,"v":0},{"u":1,"v":1}]}"#)
                .unwrap();
        assert_eq!(ok.len(), 3);
        let bad = serde_json::from_str::<Polygon>(r#"{"vertices":[{"u":0,"v":0},{"u":1,"v":0}]}"#);
        assert!(bad.is_err());
    }
}
