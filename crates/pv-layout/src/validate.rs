//! Validation of module positions (commands): the architecture's « Rust valide (collisions,
//! zones de bord) ».
//!
//! For every moved module: it must be accepted by its array's buildable area (outline,
//! edge setbacks, obstacles), must not overlap any other active module of the face, and must
//! keep the required distance to the modules of the other arrays of the face. Failures are
//! `CommandError`s with an i18n key and ICU parameters, never prose.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use pv_geo::{OrientedRect, Vec2};
use pv_model::command::keys;
use pv_model::{ArrayId, CommandError, ModuleId, ModuleState, PanelArray, Project, RoofFaceId};

use crate::model::buildable_area;
use crate::setbacks::{BuildableArea, Rejection};

/// Footprint of a module of `array` centred at `(u, v)` (same as the 3D view).
#[must_use]
pub fn footprint(array: &PanelArray, u: f64, v: f64) -> OrientedRect {
    let (hu, hv) = array.half_footprint();
    OrientedRect::axis_aligned(Vec2::new(u, v), 2.0 * hu, 2.0 * hv)
}

fn rejection_error(r: &Rejection) -> CommandError {
    match r {
        Rejection::OutsideRoof => CommandError::new(keys::OUT_OF_ROOF).with_number("count", 1.0),
        Rejection::TooCloseToEdge {
            distance_m,
            required_m,
            ..
        } => CommandError::new(keys::LAYOUT_EDGE_SETBACK)
            .with_number("distance", round_cm(*distance_m))
            .with_number("required", *required_m),
        Rejection::OverlapsObstacle { name, .. } => {
            CommandError::new(keys::LAYOUT_OBSTACLE_OVERLAP).with_text("name", name)
        }
        Rejection::TooCloseToObstacle {
            name,
            distance_m,
            required_m,
            ..
        } => CommandError::new(keys::LAYOUT_OBSTACLE_CLEARANCE)
            .with_text("name", name)
            .with_number("distance", round_cm(*distance_m))
            .with_number("required", *required_m),
    }
}

fn round_cm(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Uniform spatial hash of footprints, for neighbour queries.
struct SpatialHash {
    cell: f64,
    bins: HashMap<(i64, i64), Vec<usize>>,
}

impl SpatialHash {
    fn new(cell: f64) -> Self {
        Self {
            cell,
            bins: HashMap::new(),
        }
    }

    fn range(&self, r: &OrientedRect, margin: f64) -> ((i64, i64), (i64, i64)) {
        let (lo, hi) = r.corners().iter().fold(
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
        );
        let k = |x: f64| (x / self.cell).floor() as i64;
        (
            (k(lo.u - margin), k(lo.v - margin)),
            (k(hi.u + margin), k(hi.v + margin)),
        )
    }

    fn insert(&mut self, r: &OrientedRect, idx: usize) {
        let ((u0, v0), (u1, v1)) = self.range(r, 0.0);
        for i in u0..=u1 {
            for j in v0..=v1 {
                self.bins.entry((i, j)).or_default().push(idx);
            }
        }
    }

    fn near(&self, r: &OrientedRect, margin: f64) -> BTreeSet<usize> {
        let ((u0, v0), (u1, v1)) = self.range(r, margin);
        let mut out = BTreeSet::new();
        for i in u0..=u1 {
            for j in v0..=v1 {
                if let Some(b) = self.bins.get(&(i, j)) {
                    out.extend(b.iter().copied());
                }
            }
        }
        out
    }
}

/// Validates the current positions of `moved` modules in `project` (call it on the project
/// state after the command, before committing it).
pub fn validate_module_positions(
    project: &Project,
    moved: &[ModuleId],
) -> Result<(), CommandError> {
    let moved_set: BTreeSet<ModuleId> = moved.iter().copied().collect();
    let arrays: BTreeMap<ArrayId, &PanelArray> = project.arrays.iter().map(|a| (a.id, a)).collect();
    let faces: BTreeSet<RoofFaceId> = project
        .modules
        .iter()
        .filter(|m| moved_set.contains(&m.id))
        .filter_map(|m| arrays.get(&m.array_id).map(|a| a.roof_face_id))
        .collect();

    for face_id in faces {
        let Some(face) = project.roof_face(face_id) else {
            continue;
        };
        // Buildable areas of the arrays of this face (cached).
        let mut areas: BTreeMap<ArrayId, BuildableArea> = BTreeMap::new();
        for a in project.arrays.iter().filter(|a| a.roof_face_id == face_id) {
            let area = buildable_area(face, a)
                .map_err(|_| CommandError::new(keys::LAYOUT_INVALID_OUTLINE))?;
            areas.insert(a.id, area);
        }
        // Active modules of the face and their footprints.
        let on_face: Vec<(ModuleId, ArrayId, OrientedRect)> = project
            .modules
            .iter()
            .filter(|m| m.state == ModuleState::Active)
            .filter_map(|m| {
                let a = arrays.get(&m.array_id)?;
                (a.roof_face_id == face_id).then(|| (m.id, m.array_id, footprint(a, m.u, m.v)))
            })
            .collect();
        let max_gap = areas
            .keys()
            .filter_map(|id| arrays.get(id))
            .map(|a| a.layout.min_distance_to_other_arrays_m)
            .fold(0.0, f64::max);
        let mut hash = SpatialHash::new(2.0);
        for (i, (_, _, r)) in on_face.iter().enumerate() {
            hash.insert(r, i);
        }

        let mut overlaps = 0usize;
        for (i, (id, array_id, rect)) in on_face.iter().enumerate() {
            if !moved_set.contains(id) {
                continue;
            }
            if let Some(r) = areas[array_id].check(rect) {
                return Err(rejection_error(&r));
            }
            let mut overlapping = false;
            for j in hash.near(rect, max_gap) {
                if j == i {
                    continue;
                }
                let (_, other_array, other) = &on_face[j];
                if rect.overlaps(other) {
                    overlapping = true;
                    continue;
                }
                let paired = arrays[array_id].layout.paired_with == Some(*other_array)
                    || arrays[other_array].layout.paired_with == Some(*array_id);
                if other_array != array_id && !paired {
                    let required = arrays[array_id]
                        .layout
                        .min_distance_to_other_arrays_m
                        .max(arrays[other_array].layout.min_distance_to_other_arrays_m);
                    if required > 0.0 {
                        let d = other
                            .edges()
                            .iter()
                            .map(|e| rect.distance_to_segment(e))
                            .fold(f64::INFINITY, f64::min);
                        if d + 1e-9 < required {
                            return Err(CommandError::new(keys::LAYOUT_ARRAY_DISTANCE)
                                .with_number("distance", round_cm(d))
                                .with_number("required", required));
                        }
                    }
                }
            }
            overlaps += usize::from(overlapping);
        }
        if overlaps > 0 {
            return Err(CommandError::new(keys::LAYOUT_MODULE_OVERLAP)
                .with_number("count", overlaps as f64));
        }
    }
    Ok(())
}

/// Every conflict of a face's current layout (all active modules), e.g. after the face or
/// an array's parameters changed: `(module, error)` pairs, at most one error per module.
#[must_use]
pub fn face_conflicts(project: &Project, face_id: RoofFaceId) -> Vec<(ModuleId, CommandError)> {
    let arrays: BTreeSet<ArrayId> = project
        .arrays
        .iter()
        .filter(|a| a.roof_face_id == face_id)
        .map(|a| a.id)
        .collect();
    project
        .modules
        .iter()
        .filter(|m| m.state == ModuleState::Active && arrays.contains(&m.array_id))
        .filter_map(|m| {
            validate_module_positions(project, &[m.id])
                .err()
                .map(|e| (m.id, e))
        })
        .collect()
}
