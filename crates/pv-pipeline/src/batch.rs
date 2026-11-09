//! Input batches: what a command changed, extracted on the caller's thread (which owns the
//! project) and applied on the compute thread (which owns the graph).
//!
//! Extraction is proportional to the change: after a module command, only the module lists
//! of the touched arrays are copied.

use std::collections::BTreeSet;

use pv_graph::{Graph, GraphError, Instance};
use pv_model::{ArrayId, Building, ChangeSet, ModuleInstance, PanelArray, Project, RoofFace};

use crate::inputs::{
    ArrayIn, BuildingIn, FaceIn, ModulesIn, MountingIn, ProjectIndex, ProjectIndexIn, SiteIn,
};
use crate::nodes::summary::FieldStatics;

/// Inputs to set on the graph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputBatch {
    /// Full syncs only: entities missing from it are forgotten.
    pub index: Option<ProjectIndex>,
    pub buildings: Vec<Building>,
    pub faces: Vec<RoofFace>,
    pub arrays: Vec<PanelArray>,
    pub modules: Vec<(ArrayId, Vec<ModuleInstance>)>,
}

/// What applying a batch changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Inputs whose value changed.
    pub inputs_changed: usize,
    /// Entities forgotten (deleted from the project).
    pub forgotten: usize,
    /// Arrays among them (their results must disappear from the UI).
    pub forgotten_arrays: Vec<ArrayId>,
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

/// Every input of the project.
#[must_use]
pub fn batch_for_project(project: &Project) -> InputBatch {
    InputBatch {
        index: Some(ProjectIndex {
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
        }),
        buildings: project.buildings.clone(),
        faces: project.roof_faces.clone(),
        arrays: project.arrays.clone(),
        modules: project
            .arrays
            .iter()
            .map(|a| (a.id, modules_of(project, a.id)))
            .collect(),
    }
}

/// After a module command: the module lists of the arrays it touched.
#[must_use]
pub fn batch_for_changes(project: &Project, changes: &ChangeSet) -> InputBatch {
    let arrays: BTreeSet<ArrayId> = changes
        .module_ids
        .iter()
        .filter_map(|id| project.module(*id))
        .map(|m| m.array_id)
        .collect();
    InputBatch {
        modules: arrays
            .into_iter()
            .map(|a| (a, modules_of(project, a)))
            .collect(),
        ..InputBatch::default()
    }
}

/// Sets the batch's inputs (unchanged values are no-ops: same fingerprint), forgets deleted
/// entities on a full batch and watches the statics of the batch's arrays.
///
/// # Errors
/// Only if the pipeline kinds are not registered in `graph` (see [`crate::register`]).
pub fn apply_batch(graph: &mut Graph, batch: InputBatch) -> Result<SyncReport, GraphError> {
    let mut report = SyncReport::default();
    if let Some(index) = batch.index {
        let previous = graph
            .get::<ProjectIndexIn>(Instance::Project)
            .map(|p| p.instances())
            .unwrap_or_default();
        let current: BTreeSet<Instance> = index.instances().into_iter().collect();
        for gone in previous.into_iter().filter(|i| !current.contains(i)) {
            report.forgotten += 1;
            if let Instance::Array(a) = gone {
                report.forgotten_arrays.push(a);
            }
            graph.forget(gone);
        }
        report.inputs_changed +=
            usize::from(graph.set_input::<ProjectIndexIn>(Instance::Project, index)?);
    }
    let mut changed = 0;
    for mut b in batch.buildings {
        let i = Instance::Building(b.id);
        let site = b.site.take();
        changed += usize::from(graph.set_input::<BuildingIn>(i, b)?);
        changed += usize::from(graph.set_input::<SiteIn>(i, site)?);
    }
    for f in batch.faces {
        changed += usize::from(graph.set_input::<FaceIn>(Instance::Face(f.id), f)?);
    }
    for mut a in batch.arrays {
        let i = Instance::Array(a.id);
        let mounting = a.mounting.take();
        changed += usize::from(graph.set_input::<ArrayIn>(i, a)?);
        changed += usize::from(graph.set_input::<MountingIn>(i, mounting)?);
        graph.watch::<FieldStatics>(i)?;
    }
    for (a, modules) in batch.modules {
        changed += usize::from(graph.set_input::<ModulesIn>(Instance::Array(a), modules)?);
    }
    report.inputs_changed += changed;
    Ok(report)
}
