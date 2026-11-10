//! A face in plan (building frame) → a `pv-model` [`RoofFace`]: tilt and azimuth from its
//! plane, the `(u, v)` frame of the model convention (`RoofFrame`), origin at the lower-left
//! corner of the bounding box in `(u, v)`, and an explicit outline unless the face is the
//! rectangle the layout engine assumes by default.

use pv_geo::Vec2;
use pv_model::{Building, RoofEdgeKind, RoofFace, RoofFaceId, RoofFrame, RoofOutlineSpec};

use crate::classify::face_z;
use crate::input::RoofInput;
use crate::PlanFace;

/// Corners of a face's bounding box closer than this (m) match it.
const CORNER_TOL: f64 = 1e-9;

/// Tilt and azimuth of a face: the edge's pitch, and the compass direction of its outward
/// normal (downslope). Flat faces: 0° facing south (the model's flat-roof convention).
#[must_use]
pub fn orientation(input: &RoofInput, face: &PlanFace) -> (f64, f64) {
    match face.edge {
        None => (0.0, 180.0),
        Some(e) => {
            let p = &input.planes[e];
            let down = -p.inward;
            // `+ 0.0` turns a −0° azimuth into 0°.
            let az = down.u.atan2(down.v).to_degrees().rem_euclid(360.0) + 0.0;
            (p.pitch_deg, az)
        }
    }
}

/// The face as a `RoofFace` (id and name to be filled by the caller).
#[must_use]
pub fn roof_face(
    building: &Building,
    input: &RoofInput,
    face: &PlanFace,
    kinds: &[RoofEdgeKind],
) -> RoofFace {
    let (tilt, azimuth) = orientation(input, face);
    let o = building.origin;
    let world = |p: Vec2| [o[0] + p.u, o[1] + p.v, o[2] + face_z(input, face, p)];
    let frame0 = RoofFrame::new([0.0; 3], tilt, azimuth);
    let local: Vec<(f64, f64)> = face
        .polygon
        .iter()
        .map(|&p| {
            let (u, v, _) = frame0.to_local(world(p));
            (u, v)
        })
        .collect();
    let umin = local.iter().map(|l| l.0).fold(f64::INFINITY, f64::min);
    let vmin = local.iter().map(|l| l.1).fold(f64::INFINITY, f64::min);
    let umax = local.iter().map(|l| l.0).fold(f64::NEG_INFINITY, f64::max);
    let vmax = local.iter().map(|l| l.1).fold(f64::NEG_INFINITY, f64::max);
    // The plane lies at a constant `w` in a frame built at the world origin.
    let w0 = face
        .polygon
        .iter()
        .map(|&p| frame0.to_local(world(p)).2)
        .sum::<f64>()
        / face.polygon.len() as f64;
    let origin = frame0.to_world(umin, vmin, w0);
    let (width, height) = (umax - umin, vmax - vmin);
    let uv: Vec<[f64; 2]> = local.iter().map(|&(u, v)| [u - umin, v - vmin]).collect();

    // The layout engine's default for a face without outline (see `RoofOutline::from_face`).
    let default_kinds = if tilt < 5.0 {
        [RoofEdgeKind::Parapet; 4]
    } else {
        [
            RoofEdgeKind::Eave,
            RoofEdgeKind::Verge,
            RoofEdgeKind::Ridge,
            RoofEdgeKind::Verge,
        ]
    };
    let corners = [[0.0, 0.0], [width, 0.0], [width, height], [0.0, height]];
    let is_default_rectangle = uv.len() == 4
        && (0..4).any(|start| {
            (0..4).all(|k| {
                let p = uv[(start + k) % 4];
                let c = corners[k];
                (p[0] - c[0]).abs() <= CORNER_TOL
                    && (p[1] - c[1]).abs() <= CORNER_TOL
                    && kinds[(start + k) % 4] == default_kinds[k]
            })
        });
    RoofFace {
        id: RoofFaceId(Default::default()),
        building_id: building.id,
        name: String::new(),
        origin,
        tilt_deg: tilt,
        azimuth_deg: azimuth,
        width_m: width,
        height_m: height,
        outline: (!is_default_rectangle).then(|| RoofOutlineSpec {
            vertices_uv: uv,
            edge_kinds: kinds.to_vec(),
            obstacles: Vec::new(),
        }),
        support_grid: None,
    }
}

/// English compass name of a face (the user renames it at will).
#[must_use]
pub fn compass_name(tilt_deg: f64, azimuth_deg: f64) -> String {
    if tilt_deg < 1e-9 {
        return "Flat roof".to_owned();
    }
    const NAMES: [&str; 8] = [
        "North",
        "North-east",
        "East",
        "South-east",
        "South",
        "South-west",
        "West",
        "North-west",
    ];
    let sector = ((azimuth_deg.rem_euclid(360.0) + 22.5) / 45.0).floor() as usize % 8;
    NAMES[sector].to_owned()
}
