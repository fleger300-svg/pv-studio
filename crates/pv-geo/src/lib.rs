//! PV Studio 2D geometry.
//!
//! Everything here lives in the local `(u, v)` frame of a roof face (see `pv-model`):
//! `u` horizontal along the eave, `v` up the slope, metres. The crate is deliberately small
//! and dependency-free: points, segments, simple polygons (possibly concave) and oriented
//! rectangles, with the distance and containment predicates the layout engine needs.
//!
//! There is no general polygon offsetting: a setback is checked as the distance between a
//! module footprint and each roof edge, which is exact and deterministic.
//!
//! All predicates use the tolerance [`EPS`]: a module touching a boundary within 1 nm
//! counts as inside.

mod point;
mod polygon;
mod rect;
mod segment;

pub use point::Vec2;
pub use polygon::{Polygon, PolygonError};
pub use rect::OrientedRect;
pub use segment::Segment;

/// Geometric tolerance [m].
pub const EPS: f64 = 1e-9;
