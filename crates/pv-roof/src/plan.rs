//! Generated roof → explicit `Command::Batch` on the project.
//!
//! Generated faces are matched to the building's current faces by their overlap in plan
//! (intersection over union ≥ 50 %, best pairs first, one to one) among faces of compatible
//! orientation (both flat, or downslope directions within 45°: an array laid out for the
//! south must never land on a north face): a matched face keeps its
//! id — so the arrays on it stay attached —, its name, its covering (support grid) and its
//! obstacles (moved to the new `(u, v)` frame). Unmatched generated faces are added with new
//! ids; unmatched old faces are removed (with their arrays, undoably).

use pv_geo::{intersection_area, signed_area, Vec2};
use pv_model::{
    Building, Command, Project, RoofEdgeKind, RoofFace, RoofFaceId, RoofObstacle, RoofOutlineSpec,
};

use crate::{generate, RoofError};

/// Minimum intersection over union for a generated face to take over an existing one.
pub const MATCH_IOU: f64 = 0.5;

/// The face's polygon in world plan (counter-clockwise when its normal points up).
#[must_use]
pub fn world_plan(face: &RoofFace) -> Vec<Vec2> {
    let frame = face.frame();
    let uv: Vec<[f64; 2]> = match &face.outline {
        Some(o) => o.vertices_uv.clone(),
        None => vec![
            [0.0, 0.0],
            [face.width_m, 0.0],
            [face.width_m, face.height_m],
            [0.0, face.height_m],
        ],
    };
    let mut pts: Vec<Vec2> = uv
        .iter()
        .map(|&[u, v]| {
            let w = frame.to_world(u, v, 0.0);
            Vec2::new(w[0], w[1])
        })
        .collect();
    if signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    pts
}

/// Intersection over union of two faces in plan.
#[must_use]
pub fn iou(a: &RoofFace, b: &RoofFace) -> f64 {
    let (pa, pb) = (world_plan(a), world_plan(b));
    let inter = intersection_area(&pa, &pb);
    let union = signed_area(&pa).abs() + signed_area(&pb).abs() - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Whether two faces may be the same face: both flat, or both sloped with downslope
/// directions within 45°.
#[must_use]
pub fn compatible(a: &RoofFace, b: &RoofFace) -> bool {
    let flat = |f: &RoofFace| f.tilt_deg < 1.0;
    match (flat(a), flat(b)) {
        (true, true) => true,
        (false, false) => {
            let d = (a.azimuth_deg - b.azimuth_deg).rem_euclid(360.0);
            d.min(360.0 - d) <= 45.0
        }
        _ => false,
    }
}

/// One-to-one matching `generated index → old index`, best overlaps first.
#[must_use]
pub fn match_faces(generated: &[RoofFace], old: &[&RoofFace]) -> Vec<Option<usize>> {
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (g, gf) in generated.iter().enumerate() {
        for (o, of) in old.iter().enumerate() {
            if !compatible(gf, of) {
                continue;
            }
            let v = iou(gf, of);
            if v >= MATCH_IOU - 1e-9 {
                pairs.push((v, g, o));
            }
        }
    }
    pairs.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    let mut out = vec![None; generated.len()];
    let mut taken = vec![false; old.len()];
    for (_, g, o) in pairs {
        if out[g].is_none() && !taken[o] {
            out[g] = Some(o);
            taken[o] = true;
        }
    }
    out
}

/// Moves the old face's obstacles onto the new face's `(u, v)` frame.
fn carry_obstacles(old: &RoofFace, new: &mut RoofFace) {
    let obstacles = match &old.outline {
        Some(o) if !o.obstacles.is_empty() => &o.obstacles,
        _ => return,
    };
    let (from, to) = (old.frame(), new.frame());
    let moved: Vec<RoofObstacle> = obstacles
        .iter()
        .map(|ob| RoofObstacle {
            footprint_uv: ob
                .footprint_uv
                .iter()
                .map(|&[u, v]| {
                    let (nu, nv, _) = to.to_local(from.to_world(u, v, 0.0));
                    [nu, nv]
                })
                .collect(),
            ..ob.clone()
        })
        .collect();
    let outline = new.outline.get_or_insert_with(|| RoofOutlineSpec {
        // The default rectangle, made explicit to carry the obstacles.
        vertices_uv: vec![
            [0.0, 0.0],
            [new.width_m, 0.0],
            [new.width_m, new.height_m],
            [0.0, new.height_m],
        ],
        edge_kinds: if new.tilt_deg < 5.0 {
            vec![RoofEdgeKind::Parapet; 4]
        } else {
            vec![
                RoofEdgeKind::Eave,
                RoofEdgeKind::Verge,
                RoofEdgeKind::Ridge,
                RoofEdgeKind::Verge,
            ]
        },
        obstacles: Vec::new(),
    });
    outline.obstacles = moved;
}

/// The batch that gives `building` (new or changed, with a roof specification) its generated
/// roof in `project`. Applying it through the store validates and journals it like any
/// command; one undo reverts it.
///
/// # Errors
/// See [`RoofError`].
pub fn plan_roof(project: &Project, building: &Building) -> Result<Command, RoofError> {
    plan_roof_with(project, building, RoofFaceId::new)
}

/// [`plan_roof`] with an id source for new faces (deterministic tests).
///
/// # Errors
/// See [`RoofError`].
pub fn plan_roof_with(
    project: &Project,
    building: &Building,
    mut new_id: impl FnMut() -> RoofFaceId,
) -> Result<Command, RoofError> {
    let roof = generate(building)?;
    let b = roof.building(building);
    let old: Vec<&RoofFace> = project
        .roof_faces
        .iter()
        .filter(|f| f.building_id == b.id)
        .collect();
    let matches = match_faces(&roof.faces, &old);
    let mut commands = vec![Command::UpsertBuilding {
        building: Box::new(b),
        at: None,
    }];
    let mut kept = vec![false; old.len()];
    for (mut face, m) in roof.faces.into_iter().zip(matches) {
        match m {
            Some(o) => {
                kept[o] = true;
                let prev = old[o];
                face.id = prev.id;
                face.name.clone_from(&prev.name);
                face.support_grid.clone_from(&prev.support_grid);
                carry_obstacles(prev, &mut face);
                if &face != prev {
                    commands.push(Command::UpsertRoofFace {
                        face: Box::new(face),
                        at: None,
                    });
                }
            }
            None => {
                face.id = new_id();
                commands.push(Command::UpsertRoofFace {
                    face: Box::new(face),
                    at: None,
                });
            }
        }
    }
    for (o, f) in old.iter().enumerate() {
        if !kept[o] {
            commands.push(Command::RemoveRoofFace { id: f.id });
        }
    }
    Ok(Command::Batch { commands })
}
