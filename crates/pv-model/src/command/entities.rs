//! Entity commands: buildings, roof faces and arrays added, replaced (« upsert ») or removed.
//!
//! A removal cascades: a building takes its faces, a face its arrays, an array its modules;
//! arrays paired with a removed one (east-west) are unpaired. The inverse of a removal is a
//! `Batch` restoring everything at its original position, so undo is exact.

use std::collections::BTreeSet;

use super::structure::slots_of;
use super::{keys, ChangeSet, Command, CommandError, ModuleSlot};
use crate::ids::{ArrayId, BuildingId, RoofFaceId};
use crate::project::{Building, PanelArray, Project, RoofFace};

pub(super) fn unknown(kind: &str) -> CommandError {
    CommandError::new(keys::ENTITY_UNKNOWN).with_text("kind", kind)
}

fn reference(kind: &str) -> CommandError {
    CommandError::new(keys::ENTITY_REFERENCE).with_text("kind", kind)
}

fn invalid(field: &str) -> CommandError {
    CommandError::new(keys::ENTITY_INVALID).with_text("field", field)
}

fn finite(field: &str, values: &[f64]) -> Result<(), CommandError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(invalid(field))
    }
}

fn positive(field: &str, value: f64) -> Result<(), CommandError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(invalid(field))
    }
}

fn angle(field: &str, value: f64) -> Result<(), CommandError> {
    if value.is_finite() && (0.0..90.0).contains(&value) {
        Ok(())
    } else {
        Err(invalid(field))
    }
}

fn validate_building(b: &Building) -> Result<(), CommandError> {
    finite("origin", &b.origin)?;
    positive("sizeM", b.size_m[0])?;
    positive("sizeM", b.size_m[1])?;
    finite("eaveHeightM", &[b.eave_height_m])?;
    finite("ridgeHeightM", &[b.ridge_height_m])?;
    if b.eave_height_m < 0.0 {
        return Err(invalid("eaveHeightM"));
    }
    if b.ridge_height_m < b.eave_height_m {
        return Err(invalid("ridgeHeightM"));
    }
    crate::roof::validate_footprint(&b.footprint).map_err(|e| invalid(e.field()))?;
    if let Some(spec) = &b.roof {
        crate::roof::validate_roof_spec(spec, &b.footprint).map_err(|e| invalid(e.field()))?;
    }
    Ok(())
}

fn validate_face(project: &Project, f: &RoofFace) -> Result<(), CommandError> {
    if !project.buildings.iter().any(|b| b.id == f.building_id) {
        return Err(reference("building"));
    }
    finite("origin", &f.origin)?;
    positive("widthM", f.width_m)?;
    positive("heightM", f.height_m)?;
    angle("tiltDeg", f.tilt_deg)?;
    finite("azimuthDeg", &[f.azimuth_deg])?;
    if let Some(o) = &f.outline {
        if !o.vertices_uv.iter().flatten().all(|v| v.is_finite()) {
            return Err(invalid("outline"));
        }
    }
    if let Some(g) = &f.support_grid {
        finite("supportGrid", &[g.origin_m])?;
        positive("supportGrid", g.spacing_m)?;
    }
    Ok(())
}

fn validate_array(project: &Project, a: &PanelArray) -> Result<(), CommandError> {
    if project.roof_face(a.roof_face_id).is_none() {
        return Err(reference("roofFace"));
    }
    positive("moduleLengthM", a.module_length_m)?;
    positive("moduleWidthM", a.module_width_m)?;
    angle("tiltOnRoofDeg", a.tilt_on_roof_deg)?;
    finite("azimuthOffsetDeg", &[a.azimuth_offset_deg])?;
    if let Some(p) = a.layout.paired_with {
        if p == a.id || project.array_index(p).is_none() {
            return Err(reference("array"));
        }
    }
    Ok(())
}

fn insert_at<T>(v: &mut Vec<T>, at: Option<u32>, item: T) {
    let i = at.map_or(v.len(), |a| (a as usize).min(v.len()));
    v.insert(i, item);
}

fn at(i: usize) -> Option<u32> {
    u32::try_from(i).ok()
}

// --- Upserts -----------------------------------------------------------------------------

pub(super) fn apply_upsert_building(
    project: &mut Project,
    b: &Building,
    pos: Option<u32>,
) -> Result<ChangeSet, CommandError> {
    validate_building(b)?;
    match project.buildings.iter().position(|x| x.id == b.id) {
        Some(i) => project.buildings[i] = b.clone(),
        None => insert_at(&mut project.buildings, pos, b.clone()),
    }
    Ok(ChangeSet {
        buildings: vec![b.id],
        ..ChangeSet::default()
    })
}

pub(super) fn inverse_upsert_building(project: &Project, b: &Building) -> Command {
    match project.buildings.iter().position(|x| x.id == b.id) {
        Some(i) => Command::UpsertBuilding {
            building: Box::new(project.buildings[i].clone()),
            at: at(i),
        },
        None => Command::RemoveBuilding { id: b.id },
    }
}

pub(super) fn apply_upsert_face(
    project: &mut Project,
    f: &RoofFace,
    pos: Option<u32>,
) -> Result<ChangeSet, CommandError> {
    validate_face(project, f)?;
    match project.roof_faces.iter().position(|x| x.id == f.id) {
        Some(i) => project.roof_faces[i] = f.clone(),
        None => insert_at(&mut project.roof_faces, pos, f.clone()),
    }
    Ok(ChangeSet {
        faces: vec![f.id],
        ..ChangeSet::default()
    })
}

pub(super) fn inverse_upsert_face(project: &Project, f: &RoofFace) -> Command {
    match project.roof_faces.iter().position(|x| x.id == f.id) {
        Some(i) => Command::UpsertRoofFace {
            face: Box::new(project.roof_faces[i].clone()),
            at: at(i),
        },
        None => Command::RemoveRoofFace { id: f.id },
    }
}

pub(super) fn apply_upsert_array(
    project: &mut Project,
    a: &PanelArray,
    pos: Option<u32>,
) -> Result<ChangeSet, CommandError> {
    validate_array(project, a)?;
    match project.array_index(a.id) {
        Some(i) => project.arrays[i] = a.clone(),
        // A new array has no modules: the other blocks keep their relative order, so the
        // numbering stays canonical.
        None => insert_at(&mut project.arrays, pos, a.clone()),
    }
    Ok(ChangeSet {
        arrays: vec![a.id],
        ..ChangeSet::default()
    })
}

pub(super) fn inverse_upsert_array(project: &Project, a: &PanelArray) -> Command {
    match project.array_index(a.id) {
        Some(i) => Command::UpsertArray {
            array: Box::new(project.arrays[i].clone()),
            at: at(i),
        },
        None => Command::RemoveArray { id: a.id },
    }
}

// --- Removals ----------------------------------------------------------------------------

/// What a removal takes away (with original positions), and the arrays it unpairs.
struct Subtree {
    buildings: Vec<(usize, Building)>,
    faces: Vec<(usize, RoofFace)>,
    arrays: Vec<(usize, PanelArray)>,
    modules: Vec<(ArrayId, Vec<ModuleSlot>)>,
    /// Arrays outside the subtree paired with a removed one, as they were.
    partners: Vec<(usize, PanelArray)>,
}

enum Root {
    Building(BuildingId),
    Face(RoofFaceId),
    Array(ArrayId),
}

fn subtree(project: &Project, root: &Root) -> Option<Subtree> {
    let buildings: BTreeSet<BuildingId> = match root {
        Root::Building(id) => {
            project.buildings.iter().find(|b| b.id == *id)?;
            [*id].into()
        }
        _ => BTreeSet::new(),
    };
    let mut faces: BTreeSet<RoofFaceId> = project
        .roof_faces
        .iter()
        .filter(|f| buildings.contains(&f.building_id))
        .map(|f| f.id)
        .collect();
    if let Root::Face(id) = root {
        project.roof_face(*id)?;
        faces.insert(*id);
    }
    let mut arrays: BTreeSet<ArrayId> = project
        .arrays
        .iter()
        .filter(|a| faces.contains(&a.roof_face_id))
        .map(|a| a.id)
        .collect();
    if let Root::Array(id) = root {
        project.array_index(*id)?;
        arrays.insert(*id);
    }
    let pick = |n: usize, keep: &dyn Fn(usize) -> bool| -> Vec<usize> {
        (0..n).filter(|&i| keep(i)).collect()
    };
    Some(Subtree {
        buildings: pick(project.buildings.len(), &|i| {
            buildings.contains(&project.buildings[i].id)
        })
        .into_iter()
        .map(|i| (i, project.buildings[i].clone()))
        .collect(),
        faces: pick(project.roof_faces.len(), &|i| {
            faces.contains(&project.roof_faces[i].id)
        })
        .into_iter()
        .map(|i| (i, project.roof_faces[i].clone()))
        .collect(),
        arrays: pick(project.arrays.len(), &|i| {
            arrays.contains(&project.arrays[i].id)
        })
        .into_iter()
        .map(|i| (i, project.arrays[i].clone()))
        .collect(),
        modules: project
            .arrays
            .iter()
            .filter(|a| arrays.contains(&a.id))
            .map(|a| (a.id, slots_of(project, a.id)))
            .filter(|(_, s)| !s.is_empty())
            .collect(),
        partners: project
            .arrays
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                !arrays.contains(&a.id) && a.layout.paired_with.is_some_and(|p| arrays.contains(&p))
            })
            .map(|(i, a)| (i, a.clone()))
            .collect(),
    })
}

fn root_kind(root: &Root) -> &'static str {
    match root {
        Root::Building(_) => "building",
        Root::Face(_) => "roofFace",
        Root::Array(_) => "array",
    }
}

fn apply_remove(project: &mut Project, root: &Root) -> Result<ChangeSet, CommandError> {
    let sub = subtree(project, root).ok_or_else(|| unknown(root_kind(root)))?;
    let buildings: BTreeSet<BuildingId> = sub.buildings.iter().map(|(_, b)| b.id).collect();
    let faces: BTreeSet<RoofFaceId> = sub.faces.iter().map(|(_, f)| f.id).collect();
    let arrays: BTreeSet<ArrayId> = sub.arrays.iter().map(|(_, a)| a.id).collect();
    let before = project.modules.len();
    project.modules.retain(|m| !arrays.contains(&m.array_id));
    let structure = project.modules.len() != before;
    project.arrays.retain(|a| !arrays.contains(&a.id));
    project.roof_faces.retain(|f| !faces.contains(&f.id));
    project.buildings.retain(|b| !buildings.contains(&b.id));
    for (_, p) in &sub.partners {
        if let Some(i) = project.array_index(p.id) {
            project.arrays[i].layout.paired_with = None;
        }
    }
    if structure {
        project.compact_modules();
    }
    Ok(ChangeSet {
        buildings: buildings.into_iter().collect(),
        faces: faces.into_iter().collect(),
        arrays: arrays
            .into_iter()
            .chain(sub.partners.iter().map(|(_, a)| a.id))
            .collect(),
        structure,
        ..ChangeSet::default()
    })
}

fn inverse_remove(project: &Project, root: &Root) -> Command {
    let Some(sub) = subtree(project, root) else {
        return Command::Batch { commands: vec![] };
    };
    let mut commands = Vec::new();
    // Ascending original positions: each insertion lands where it was.
    for (i, b) in sub.buildings {
        commands.push(Command::UpsertBuilding {
            building: Box::new(b),
            at: at(i),
        });
    }
    for (i, f) in sub.faces {
        commands.push(Command::UpsertRoofFace {
            face: Box::new(f),
            at: at(i),
        });
    }
    // Arrays first unpaired (two removed arrays may be paired with each other), then paired
    // again in place once all exist.
    let mut repair = Vec::new();
    for (i, a) in sub.arrays {
        let mut bare = a.clone();
        if bare.layout.paired_with.take().is_some() {
            repair.push(Command::UpsertArray {
                array: Box::new(a),
                at: at(i),
            });
        }
        commands.push(Command::UpsertArray {
            array: Box::new(bare),
            at: at(i),
        });
    }
    commands.extend(repair);
    for (i, a) in sub.partners {
        commands.push(Command::UpsertArray {
            array: Box::new(a),
            at: at(i),
        });
    }
    for (array_id, modules) in sub.modules {
        commands.push(Command::SetArrayModules { array_id, modules });
    }
    Command::Batch { commands }
}

pub(super) fn apply_remove_building(
    project: &mut Project,
    id: BuildingId,
) -> Result<ChangeSet, CommandError> {
    apply_remove(project, &Root::Building(id))
}

pub(super) fn inverse_remove_building(project: &Project, id: BuildingId) -> Command {
    inverse_remove(project, &Root::Building(id))
}

pub(super) fn apply_remove_face(
    project: &mut Project,
    id: RoofFaceId,
) -> Result<ChangeSet, CommandError> {
    apply_remove(project, &Root::Face(id))
}

pub(super) fn inverse_remove_face(project: &Project, id: RoofFaceId) -> Command {
    inverse_remove(project, &Root::Face(id))
}

pub(super) fn apply_remove_array(
    project: &mut Project,
    id: ArrayId,
) -> Result<ChangeSet, CommandError> {
    apply_remove(project, &Root::Array(id))
}

pub(super) fn inverse_remove_array(project: &Project, id: ArrayId) -> Command {
    inverse_remove(project, &Root::Array(id))
}
