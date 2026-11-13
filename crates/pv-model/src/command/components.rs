//! Component commands: freezing library components into the project, updating them (update
//! proposals), and assigning them to arrays.
//!
//! `Project::components` is kept sorted by id, so that every inverse restores the exact
//! original state.

use super::entities::unknown;
use super::{keys, ChangeSet, Command, CommandError};
use crate::component::{check_array_components, check_system, ComponentSnapshot, ComponentSpec};
use crate::ids::{ArrayId, ComponentId};
use crate::layout::SupportGridKind;
use crate::project::Project;

fn sort(components: &mut [ComponentSnapshot]) {
    components.sort_by_key(|s| s.component.id);
}

/// Checks every component of `components`, and every system against the others.
fn check_all(components: &[ComponentSnapshot]) -> Result<(), CommandError> {
    for s in components {
        s.component.validate()?;
        if matches!(s.component.spec, ComponentSpec::MountingSystem(_)) {
            check_system(&s.component, components)?;
        }
    }
    Ok(())
}

pub(super) fn inverse_adopt(project: &Project, snapshots: &[ComponentSnapshot]) -> Command {
    let mut ids: Vec<ComponentId> = snapshots
        .iter()
        .map(|s| s.component.id)
        .filter(|&id| project.component(id).is_none())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    Command::ForgetComponents { ids }
}

pub(super) fn apply_adopt(
    project: &mut Project,
    snapshots: &[ComponentSnapshot],
) -> Result<ChangeSet, CommandError> {
    let mut components = project.components.clone();
    let mut added = false;
    for s in snapshots {
        match components.iter().find(|c| c.component.id == s.component.id) {
            Some(existing) if existing == s => {}
            Some(_) => {
                return Err(CommandError::new(keys::COMPONENT_EXISTS)
                    .with_text("id", s.component.id.to_string()))
            }
            None => {
                components.push(s.clone());
                added = true;
            }
        }
    }
    if !added {
        return Ok(ChangeSet::default());
    }
    sort(&mut components);
    check_all(&components)?;
    project.components = components;
    Ok(ChangeSet {
        components: true,
        ..ChangeSet::default()
    })
}

pub(super) fn inverse_forget(project: &Project, ids: &[ComponentId]) -> Command {
    Command::AdoptComponents {
        snapshots: project
            .components
            .iter()
            .filter(|s| ids.contains(&s.component.id))
            .cloned()
            .collect(),
    }
}

pub(super) fn apply_forget(
    project: &mut Project,
    ids: &[ComponentId],
) -> Result<ChangeSet, CommandError> {
    let in_use = |id: ComponentId| {
        project
            .arrays
            .iter()
            .any(|a| a.module == Some(id) || a.mounting == Some(id))
            || project.wiring.inverters.iter().any(|i| i.component == id)
            || project.components.iter().any(|s| {
                !ids.contains(&s.component.id)
                    && matches!(&s.component.spec, ComponentSpec::MountingSystem(m) if m.part_ids().contains(&id))
            })
    };
    for &id in ids {
        if project.component(id).is_none() {
            return Err(unknown("component"));
        }
        if in_use(id) {
            return Err(CommandError::new(keys::ENTITY_REFERENCE).with_text("kind", "component"));
        }
    }
    let before = project.components.len();
    project
        .components
        .retain(|s| !ids.contains(&s.component.id));
    Ok(ChangeSet {
        components: project.components.len() != before,
        ..ChangeSet::default()
    })
}

pub(super) fn inverse_update(project: &Project, snapshots: &[ComponentSnapshot]) -> Command {
    Command::UpdateComponents {
        snapshots: snapshots
            .iter()
            .filter_map(|s| project.component(s.component.id).cloned())
            .collect(),
    }
}

pub(super) fn apply_update(
    project: &mut Project,
    snapshots: &[ComponentSnapshot],
) -> Result<ChangeSet, CommandError> {
    let mut components = project.components.clone();
    let mut changed = false;
    for s in snapshots {
        let Some(slot) = components
            .iter_mut()
            .find(|c| c.component.id == s.component.id)
        else {
            return Err(unknown("component"));
        };
        if slot != s {
            *slot = s.clone();
            changed = true;
        }
    }
    if !changed {
        return Ok(ChangeSet::default());
    }
    check_all(&components)?;
    // Arrays keep copies of their module's dimensions.
    let mut arrays: Vec<ArrayId> = Vec::new();
    let mut new_arrays = project.arrays.clone();
    for a in &mut new_arrays {
        let module = a
            .module
            .and_then(|id| components.iter().find(|c| c.component.id == id));
        if let Some(ComponentSpec::Module(m)) = module.map(|c| &c.component.spec) {
            if a.module_length_m != m.length_m || a.module_width_m != m.width_m {
                a.module_length_m = m.length_m;
                a.module_width_m = m.width_m;
                arrays.push(a.id);
            }
        }
    }
    arrays.sort_unstable();
    project.components = components;
    project.arrays = new_arrays;
    Ok(ChangeSet {
        components: true,
        arrays,
        ..ChangeSet::default()
    })
}

pub(super) fn inverse_set_array(project: &Project, array_id: ArrayId) -> Command {
    match project.array_index(array_id) {
        Some(i) => Command::UpsertArray {
            array: Box::new(project.arrays[i].clone()),
            at: u32::try_from(i).ok(),
        },
        // Applying fails on an unknown array: nothing to undo.
        None => Command::Batch {
            commands: Vec::new(),
        },
    }
}

pub(super) fn apply_set_array(
    project: &mut Project,
    array_id: ArrayId,
    module: Option<ComponentId>,
    mounting: Option<ComponentId>,
) -> Result<ChangeSet, CommandError> {
    let i = project
        .array_index(array_id)
        .ok_or_else(|| unknown("array"))?;
    let mut array = project.arrays[i].clone();
    if let Some(id) = module {
        let c = &project
            .component(id)
            .ok_or_else(|| {
                CommandError::new(keys::COMPONENT_UNKNOWN).with_text("id", id.to_string())
            })?
            .component;
        let ComponentSpec::Module(m) = &c.spec else {
            return Err(c
                .error(keys::COMPONENT_WRONG_FAMILY)
                .with_text("expected", "module"));
        };
        array.module_length_m = m.length_m;
        array.module_width_m = m.width_m;
    }
    if let Some(system_id) = mounting {
        let Some(module_id) = module else {
            return Err(CommandError::new(keys::COMPONENT_UNKNOWN).with_text("id", "module"));
        };
        let system = &project
            .component(system_id)
            .ok_or_else(|| {
                CommandError::new(keys::COMPONENT_UNKNOWN).with_text("id", system_id.to_string())
            })?
            .component;
        let module = &project
            .component(module_id)
            .ok_or_else(|| unknown("component"))?
            .component;
        let support: Option<SupportGridKind> = project
            .roof_faces
            .iter()
            .find(|f| f.id == array.roof_face_id)
            .and_then(|f| f.support_grid.map(|g| g.kind));
        if let Some(issue) =
            check_array_components(module, system, support, project.components.as_slice())
                .into_iter()
                .next()
        {
            return Err(issue);
        }
        // A newly chosen system brings its rail geometry.
        if array.mounting != Some(system_id) {
            if let ComponentSpec::MountingSystem(spec) = &system.spec {
                if spec.defaults.rails.is_some() {
                    array.layout.rails = spec.defaults.rails;
                }
            }
        }
    }
    array.module = module;
    array.mounting = mounting;
    let changed = array != project.arrays[i];
    project.arrays[i] = array;
    Ok(ChangeSet {
        arrays: if changed { vec![array_id] } else { Vec::new() },
        ..ChangeSet::default()
    })
}
