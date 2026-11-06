//! Setbacks: minimum distances from roof edges and obstacles, giving the buildable area.
//!
//! The buildable area is never computed as an offset polygon. A module footprint is accepted
//! when it lies inside the roof outline, keeps the required distance from every edge
//! (distance depends on the edge kind) and from every obstacle, and does not overlap an
//! obstacle. This is exact for concave outlines and needs no polygon offsetting.

use std::collections::HashMap;

use pv_geo::{OrientedRect, EPS};
use serde::{Deserialize, Serialize};

use crate::roof::{EdgeKind, ObstacleKind, RoofOutline};

/// Distances to keep, by edge kind and obstacle kind [m].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetbackRules {
    /// Distance for edges whose kind is not listed in `by_edge`.
    pub default_edge_m: f64,
    #[serde(default)]
    pub by_edge: HashMap<EdgeKind, f64>,
    /// Clearance for obstacles whose kind is not listed in `by_obstacle`.
    pub default_obstacle_m: f64,
    #[serde(default)]
    pub by_obstacle: HashMap<ObstacleKind, f64>,
}

impl SetbackRules {
    /// Same distance on every edge and around every obstacle.
    #[must_use]
    pub fn uniform(edge_m: f64, obstacle_m: f64) -> Self {
        Self {
            default_edge_m: edge_m,
            by_edge: HashMap::new(),
            default_obstacle_m: obstacle_m,
            by_obstacle: HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_edge(mut self, kind: EdgeKind, m: f64) -> Self {
        self.by_edge.insert(kind, m);
        self
    }

    #[must_use]
    pub fn with_obstacle(mut self, kind: ObstacleKind, m: f64) -> Self {
        self.by_obstacle.insert(kind, m);
        self
    }

    #[must_use]
    pub fn edge(&self, kind: EdgeKind) -> f64 {
        self.by_edge
            .get(&kind)
            .copied()
            .unwrap_or(self.default_edge_m)
    }

    #[must_use]
    pub fn obstacle(&self, kind: ObstacleKind) -> f64 {
        self.by_obstacle
            .get(&kind)
            .copied()
            .unwrap_or(self.default_obstacle_m)
    }
}

/// Why a footprint was rejected. Each variant maps to an i18n message key so that command
/// validation can tell the user what to fix.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Rejection {
    OutsideRoof,
    TooCloseToEdge {
        edge: usize,
        kind: EdgeKind,
        distance_m: f64,
        required_m: f64,
    },
    OverlapsObstacle {
        obstacle: usize,
        name: String,
    },
    TooCloseToObstacle {
        obstacle: usize,
        name: String,
        distance_m: f64,
        required_m: f64,
    },
}

impl Rejection {
    #[must_use]
    pub fn message_key(&self) -> &'static str {
        match self {
            Rejection::OutsideRoof => "layout.reject.outside_roof",
            Rejection::TooCloseToEdge { .. } => "layout.reject.edge_setback",
            Rejection::OverlapsObstacle { .. } => "layout.reject.obstacle_overlap",
            Rejection::TooCloseToObstacle { .. } => "layout.reject.obstacle_clearance",
        }
    }
}

/// Roof outline with its setbacks resolved per edge and per obstacle.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuildableArea {
    roof: RoofOutline,
    edge_setbacks: Vec<f64>,
    obstacle_clearances: Vec<f64>,
}

impl BuildableArea {
    #[must_use]
    pub fn new(roof: RoofOutline, rules: &SetbackRules) -> Self {
        let edge_setbacks = roof.edge_kinds().iter().map(|&k| rules.edge(k)).collect();
        let obstacle_clearances = roof
            .obstacles()
            .iter()
            .map(|o| o.clearance_m.unwrap_or_else(|| rules.obstacle(o.kind)))
            .collect();
        Self {
            roof,
            edge_setbacks,
            obstacle_clearances,
        }
    }

    #[must_use]
    pub fn roof(&self) -> &RoofOutline {
        &self.roof
    }

    #[must_use]
    pub fn edge_setback(&self, edge: usize) -> f64 {
        self.edge_setbacks[edge]
    }

    /// First reason why the footprint cannot be placed, or `None` if it fits.
    #[must_use]
    pub fn check(&self, footprint: &OrientedRect) -> Option<Rejection> {
        let boundary = self.roof.boundary();
        if !boundary.contains_rect(footprint) {
            return Some(Rejection::OutsideRoof);
        }
        for (i, edge) in boundary.edges().enumerate() {
            let required = self.edge_setbacks[i];
            if required <= 0.0 {
                continue;
            }
            let d = footprint.distance_to_segment(&edge);
            if d + EPS < required {
                return Some(Rejection::TooCloseToEdge {
                    edge: i,
                    kind: self.roof.edge_kind(i),
                    distance_m: d,
                    required_m: required,
                });
            }
        }
        for (i, ob) in self.roof.obstacles().iter().enumerate() {
            if ob.footprint.overlaps_rect(footprint) {
                return Some(Rejection::OverlapsObstacle {
                    obstacle: i,
                    name: ob.name.clone(),
                });
            }
            let required = self.obstacle_clearances[i];
            if required > 0.0 {
                let d = ob.footprint.distance_to_rect(footprint);
                if d + EPS < required {
                    return Some(Rejection::TooCloseToObstacle {
                        obstacle: i,
                        name: ob.name.clone(),
                        distance_m: d,
                        required_m: required,
                    });
                }
            }
        }
        None
    }

    #[must_use]
    pub fn accepts(&self, footprint: &OrientedRect) -> bool {
        self.check(footprint).is_none()
    }
}
