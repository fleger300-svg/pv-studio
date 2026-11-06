//! Project → graph inputs.
//!
//! [`sync_project`] sets every input from the project (an unchanged value is a no-op: same
//! fingerprint), forgets the entities that disappeared and watches the statics of every
//! field. [`sync_changes`] is the fast path after a module command: only the module lists
//! of the touched arrays are set. Both lead to the same graph state.

use std::collections::BTreeSet;

use pv_graph::{Graph, GraphError, Instance};
use pv_model::{ArrayId, ChangeSet, ModuleInstance, Project};

use crate::inputs::{
    ArrayIn, BuildingIn, FaceIn, ModulesIn, MountingIn, ProjectIndex, ProjectIndexIn, SiteIn,
};
use crate::nodes::summary::FieldStatics;

/// What a sync changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Inputs whose value changed.
    pub inputs_changed: usize,
    /// Entities forgotten (deleted from the project).
    pub forgotten: usize,
}

fn modules_of(project: &Project, array: ArrayId) -> Vec<ModuleInstance> {
    // Modules of one array are contiguous (project invariant).
    project
        .modules
        .iter()
        .skip_while(|m| m.array_id != array)
        .take_while(|m| m.array_id == array)
        .cloned()
        .collect()
}

/// Sets every input from `project`.
///
/// # Errors
/// Only if the pipeline kinds are not registered in `graph` (see [`crate::register`]).
pub fn sync_project(graph: &mut Graph, project: &Project) -> Result<SyncReport, GraphError> {
    let mut report = SyncReport::default();
    let index = ProjectIndex {
        buildings: project.buildings.iter().map(|b| b.id).collect(),
        faces: project
            .roof_faces
            .iter()
            .map(|f| (f.id, f.building_id))
            .collect(),
        arrays: project
            .arrays
            .iter()
            .map(|a| (a.id, a.roof_face_id))
            .collect(),
    };
    let previous = graph
        .get::<ProjectIndexIn>(Instance::Project)
        .map(|p| p.instances())
        .unwrap_or_default();
    let current: BTreeSet<Instance> = index.instances().into_iter().collect();
    for gone in previous.into_iter().filter(|i| !current.contains(i)) {
        report.forgotten += 1;
        graph.forget(gone);
    }
    let mut set = |changed: bool| report.inputs_changed += usize::from(changed);
    set(graph.set_input::<ProjectIndexIn>(Instance::Project, index)?);

    for b in &project.buildings {
        let i = Instance::Building(b.id);
        let mut building = b.clone();
        let site = building.site.take();
        set(graph.set_input::<BuildingIn>(i, building)?);
        set(graph.set_input::<SiteIn>(i, site)?);
    }
    for f in &project.roof_faces {
        set(graph.set_input::<FaceIn>(Instance::Face(f.id), f.clone())?);
    }
    for a in &project.arrays {
        let i = Instance::Array(a.id);
        let mut array = a.clone();
        let mounting = array.mounting.take();
        set(graph.set_input::<ArrayIn>(i, array)?);
        set(graph.set_input::<MountingIn>(i, mounting)?);
        set(graph.set_input::<ModulesIn>(i, modules_of(project, a.id))?);
        graph.watch::<FieldStatics>(i)?;
    }
    Ok(report)
}

/// After a module command: sets the module lists of the arrays it touched.
///
/// # Errors
/// Only if the pipeline kinds are not registered in `graph`.
pub fn sync_changes(
    graph: &mut Graph,
    project: &Project,
    changes: &ChangeSet,
) -> Result<SyncReport, GraphError> {
    let arrays: BTreeSet<ArrayId> = changes
        .module_ids
        .iter()
        .filter_map(|id| project.module(*id))
        .map(|m| m.array_id)
        .collect();
    let mut report = SyncReport::default();
    for a in arrays {
        let changed = graph.set_input::<ModulesIn>(Instance::Array(a), modules_of(project, a))?;
        report.inputs_changed += usize::from(changed);
    }
    Ok(report)
}
