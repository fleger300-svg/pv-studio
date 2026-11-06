//! Roof outline in the face's `(u, v)` frame: boundary polygon with typed edges, obstacles.

use pv_geo::{Polygon, PolygonError, Vec2};
use pv_model::{RoofFace, RoofOutlineSpec};
use serde::{Deserialize, Serialize};

use crate::LayoutError;

pub use pv_model::{ObstacleKind, RoofEdgeKind as EdgeKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Obstacle {
    pub name: String,
    pub kind: ObstacleKind,
    /// Footprint on the roof plane.
    pub footprint: Polygon,
    /// Height above the roof surface [m]; used later for near shading.
    #[serde(default)]
    pub height_m: f64,
    /// Clearance overriding the rule for this obstacle kind [m].
    #[serde(default)]
    pub clearance_m: Option<f64>,
}

/// Outline of one roof face.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoofOutline {
    boundary: Polygon,
    /// Kind of each boundary edge, aligned with `boundary.edge(i)`.
    edge_kinds: Vec<EdgeKind>,
    obstacles: Vec<Obstacle>,
}

impl RoofOutline {
    /// Outline from vertices (any orientation) and the kind of each input edge (edge `i`
    /// joins vertices `i` and `i + 1`).
    pub fn new(
        vertices: Vec<Vec2>,
        edge_kinds: Vec<EdgeKind>,
        obstacles: Vec<Obstacle>,
    ) -> Result<Self, LayoutError> {
        if edge_kinds.len() != vertices.len() {
            return Err(LayoutError::EdgeKindCount {
                vertices: vertices.len(),
                kinds: edge_kinds.len(),
            });
        }
        let (boundary, map) = Polygon::new_with_edge_map(vertices).map_err(LayoutError::Outline)?;
        let edge_kinds = map.iter().map(|&i| edge_kinds[i]).collect();
        let outline = Self {
            boundary,
            edge_kinds,
            obstacles: Vec::new(),
        };
        obstacles
            .into_iter()
            .try_fold(outline, |o, ob| o.with_obstacle(ob))
    }

    /// Rectangular pitched face `[0, width] × [0, height]` of the `pv-model` convention:
    /// eave along `v = 0`, ridge along `v = height`, the two sloped sides of the given kind
    /// (verge for a gable roof, hip for a hipped roof drawn as a rectangle).
    pub fn rectangle(width_m: f64, height_m: f64, sides: EdgeKind) -> Result<Self, LayoutError> {
        Self::new(
            vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(width_m, 0.0),
                Vec2::new(width_m, height_m),
                Vec2::new(0.0, height_m),
            ],
            vec![EdgeKind::Eave, sides, EdgeKind::Ridge, sides],
            Vec::new(),
        )
    }

    /// Flat roof `[0, width] × [0, depth]` bordered by parapets on all sides.
    pub fn flat(width_m: f64, depth_m: f64) -> Result<Self, LayoutError> {
        Self::new(
            vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(width_m, 0.0),
                Vec2::new(width_m, depth_m),
                Vec2::new(0.0, depth_m),
            ],
            vec![EdgeKind::Parapet; 4],
            Vec::new(),
        )
    }

    /// Outline of a `pv-model` face: its explicit outline when it has one, otherwise its
    /// rectangle (parapets all round below 5° of pitch, eave / verges / ridge above).
    pub fn from_face(face: &RoofFace) -> Result<Self, LayoutError> {
        match &face.outline {
            Some(spec) => Self::from_spec(spec),
            None if face.tilt_deg < 5.0 => Self::flat(face.width_m, face.height_m),
            None => Self::rectangle(face.width_m, face.height_m, EdgeKind::Verge),
        }
    }

    pub fn from_spec(spec: &RoofOutlineSpec) -> Result<Self, LayoutError> {
        let vertices = spec
            .vertices_uv
            .iter()
            .map(|&[u, v]| Vec2::new(u, v))
            .collect();
        let obstacles = spec
            .obstacles
            .iter()
            .map(|o| {
                let fp = o
                    .footprint_uv
                    .iter()
                    .map(|&[u, v]| Vec2::new(u, v))
                    .collect();
                Ok(Obstacle {
                    name: o.name.clone(),
                    kind: o.kind,
                    footprint: Polygon::new(fp)?,
                    height_m: o.height_m,
                    clearance_m: o.clearance_m,
                })
            })
            .collect::<Result<Vec<_>, LayoutError>>()?;
        Self::new(vertices, spec.edge_kinds.clone(), obstacles)
    }

    /// Adds an obstacle; it must lie at least partly on the roof.
    pub fn with_obstacle(mut self, obstacle: Obstacle) -> Result<Self, LayoutError> {
        let on_roof = obstacle
            .footprint
            .vertices()
            .iter()
            .any(|&v| self.boundary.contains_point(v))
            || self
                .boundary
                .vertices()
                .iter()
                .any(|&v| obstacle.footprint.contains_point(v));
        if !on_roof {
            return Err(LayoutError::ObstacleOffRoof {
                name: obstacle.name,
            });
        }
        self.obstacles.push(obstacle);
        Ok(self)
    }

    #[must_use]
    pub fn boundary(&self) -> &Polygon {
        &self.boundary
    }

    #[must_use]
    pub fn edge_kind(&self, edge: usize) -> EdgeKind {
        self.edge_kinds[edge]
    }

    #[must_use]
    pub fn edge_kinds(&self) -> &[EdgeKind] {
        &self.edge_kinds
    }

    #[must_use]
    pub fn obstacles(&self) -> &[Obstacle] {
        &self.obstacles
    }

    /// Roof area without obstacles [m²].
    #[must_use]
    pub fn area_m2(&self) -> f64 {
        self.boundary.area()
    }
}

/// Validating deserialisation (edge kinds are re-aligned like in [`RoofOutline::new`]).
impl<'de> Deserialize<'de> for RoofOutline {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            vertices: Vec<Vec2>,
            edge_kinds: Vec<EdgeKind>,
            #[serde(default)]
            obstacles: Vec<Obstacle>,
        }
        let raw = Raw::deserialize(d)?;
        RoofOutline::new(raw.vertices, raw.edge_kinds, raw.obstacles)
            .map_err(serde::de::Error::custom)
    }
}

impl From<PolygonError> for LayoutError {
    fn from(e: PolygonError) -> Self {
        LayoutError::Outline(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_kinds_follow_normalisation() {
        // Drawn clockwise: eave is input edge 3 (from (6,0) back to (0,0)).
        let cw = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 4.0),
            Vec2::new(6.0, 4.0),
            Vec2::new(6.0, 0.0),
        ];
        let kinds = vec![
            EdgeKind::Verge,
            EdgeKind::Ridge,
            EdgeKind::Verge,
            EdgeKind::Eave,
        ];
        let roof = RoofOutline::new(cw, kinds, vec![]).unwrap();
        let eave = (0..4)
            .find(|&i| roof.edge_kind(i) == EdgeKind::Eave)
            .unwrap();
        let e = roof.boundary().edge(eave);
        assert_eq!((e.a.v, e.b.v), (0.0, 0.0));
        let ridge = (0..4)
            .find(|&i| roof.edge_kind(i) == EdgeKind::Ridge)
            .unwrap();
        assert_eq!(roof.boundary().edge(ridge).a.v, 4.0);
    }

    #[test]
    fn rejects_mismatched_kinds_and_off_roof_obstacle() {
        let v = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
        ];
        assert!(matches!(
            RoofOutline::new(v, vec![EdgeKind::Eave], vec![]),
            Err(LayoutError::EdgeKindCount {
                vertices: 3,
                kinds: 1
            })
        ));
        let roof = RoofOutline::rectangle(5.0, 3.0, EdgeKind::Verge).unwrap();
        let far = Obstacle {
            name: "far".into(),
            kind: ObstacleKind::Chimney,
            footprint: Polygon::rectangle(10.0, 10.0, 11.0, 11.0),
            height_m: 1.0,
            clearance_m: None,
        };
        assert!(matches!(
            roof.with_obstacle(far),
            Err(LayoutError::ObstacleOffRoof { .. })
        ));
    }
}
