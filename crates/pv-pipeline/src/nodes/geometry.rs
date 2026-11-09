//! Face geometry: outline, roof case for the coefficients, wind-zone geometry.

use pv_graph::{Ctx, Derived, GraphError, Instance};
use pv_layout::{EdgeKind, RoofOutline};
use pv_model::BuildingId;
use pv_statics::zones::ZoneGeometry;
use pv_statics::RoofInput;
use serde::Serialize;

use super::{face_of, kind};
use crate::convert::roof_input;
use crate::error::{layout_error, statics_error};
use crate::inputs::{BuildingIn, FaceIn};

kind!(
    /// Outline of a face (`pv-layout`).
    Outline,
    "face.outline",
    RoofOutline
);

impl Derived for Outline {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<RoofOutline, GraphError> {
        let face = ctx.get::<FaceIn>(i)?;
        RoofOutline::from_face(&face).map_err(|e| layout_error(&e).into())
    }
}

/// Roof shape and pitch of a face, and its building.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoofCase {
    pub building: BuildingId,
    pub roof: RoofInput,
}

kind!(
    /// Roof shape and pitch of a face (cut-off: most face edits keep them).
    Roof,
    "face.roof",
    RoofCase
);

impl Derived for Roof {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<RoofCase, GraphError> {
        let face = ctx.get::<FaceIn>(i)?;
        let building = ctx.get::<BuildingIn>(Instance::Building(face.building_id))?;
        Ok(RoofCase {
            building: face.building_id,
            roof: roof_input(&building, &face),
        })
    }
}

kind!(
    /// Wind-zone geometry of a face: e = min(b, 2h), with b the eave length (the dimension
    /// across the wind blowing onto the face) and h the ridge height.
    ZoneGeom,
    "face.zone_geometry",
    ZoneGeometry
);

impl Derived for ZoneGeom {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<ZoneGeometry, GraphError> {
        face_of(i)?;
        let outline = ctx.get::<Outline>(i)?;
        let face = ctx.get::<FaceIn>(i)?;
        let building = ctx.get::<BuildingIn>(Instance::Building(face.building_id))?;
        let eave: f64 = outline
            .boundary()
            .edges()
            .enumerate()
            .filter(|(k, _)| outline.edge_kind(*k) == EdgeKind::Eave)
            .map(|(_, e)| e.length())
            .sum();
        // No eave (flat roof with parapets): the longer side, which gives the wider (safer)
        // edge strips.
        let b = if eave > 0.0 {
            eave
        } else {
            face.width_m.max(face.height_m)
        };
        ZoneGeometry::new(b, building.ridge_height_m).map_err(|e| statics_error(&e).into())
    }
}
