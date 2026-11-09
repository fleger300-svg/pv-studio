//! Module commands: move, remove / restore, exact positions.

use super::{keys, ChangeSet, Command, CommandError, ModulePosition};
use crate::ids::ModuleId;
use crate::project::{ModuleState, Project};

pub(super) fn unique_sorted(ids: &[ModuleId]) -> Vec<ModuleId> {
    let mut v = ids.to_vec();
    v.sort_unstable();
    v.dedup();
    v
}

fn check_known(project: &Project, ids: &[ModuleId]) -> Result<(), CommandError> {
    let unknown = ids
        .iter()
        .filter(|id| project.module(**id).is_none())
        .count();
    if unknown > 0 {
        return Err(CommandError::new(keys::UNKNOWN_MODULE).with_number("count", unknown as f64));
    }
    Ok(())
}

/// Validates that every `(id, u, v)` keeps the module inside its roof face.
fn check_positions(
    project: &Project,
    targets: &[(ModuleId, f64, f64)],
) -> Result<(), CommandError> {
    let mut outside = 0usize;
    for &(id, u, v) in targets {
        if !u.is_finite() || !v.is_finite() {
            return Err(CommandError::new(keys::INVALID_NUMBER));
        }
        let m = &project.modules[id.index()];
        let (array, face) = project
            .module_context(m)
            .ok_or_else(|| CommandError::new(keys::UNKNOWN_MODULE).with_number("count", 1.0))?;
        if !Project::fits_on_roof(array, face, u, v) {
            outside += 1;
        }
    }
    if outside > 0 {
        return Err(CommandError::new(keys::OUT_OF_ROOF).with_number("count", outside as f64));
    }
    Ok(())
}

fn set_state(
    project: &mut Project,
    ids: &[ModuleId],
    from: ModuleState,
    to: ModuleState,
) -> ChangeSet {
    let mut changed = Vec::new();
    for &id in ids {
        let m = &mut project.modules[id.index()];
        if m.state == from {
            m.state = to;
            changed.push(id);
        }
    }
    ChangeSet::modules(changed)
}

/// Exact current positions of `ids` (the inverse of a move: `(u + du) − du ≠ u` in floating
/// point).
fn snapshot(project: &Project, ids: Vec<ModuleId>) -> Command {
    Command::SetModulePositions {
        positions: ids
            .into_iter()
            .map(|id| {
                let m = &project.modules[id.index()];
                ModulePosition { id, u: m.u, v: m.v }
            })
            .collect(),
    }
}

fn known(project: &Project, ids: &[ModuleId]) -> Vec<ModuleId> {
    unique_sorted(ids)
        .into_iter()
        .filter(|id| project.module(*id).is_some())
        .collect()
}

fn with_state(project: &Project, ids: &[ModuleId], state: ModuleState) -> Vec<ModuleId> {
    known(project, ids)
        .into_iter()
        .filter(|id| project.modules[id.index()].state == state)
        .collect()
}

pub(super) fn inverse_move(project: &Project, ids: &[ModuleId]) -> Command {
    snapshot(project, known(project, ids))
}

pub(super) fn inverse_set_positions(project: &Project, positions: &[ModulePosition]) -> Command {
    let ids: Vec<ModuleId> = positions.iter().map(|p| p.id).collect();
    snapshot(project, known(project, &ids))
}

pub(super) fn inverse_remove(project: &Project, ids: &[ModuleId]) -> Command {
    Command::RestoreModules {
        ids: with_state(project, ids, ModuleState::Active),
    }
}

pub(super) fn inverse_restore(project: &Project, ids: &[ModuleId]) -> Command {
    Command::RemoveModules {
        ids: with_state(project, ids, ModuleState::Removed),
    }
}

pub(super) fn apply_move(
    project: &mut Project,
    ids: &[ModuleId],
    du: f64,
    dv: f64,
) -> Result<ChangeSet, CommandError> {
    if !du.is_finite() || !dv.is_finite() {
        return Err(CommandError::new(keys::INVALID_NUMBER));
    }
    let ids = unique_sorted(ids);
    check_known(project, &ids)?;
    let targets: Vec<(ModuleId, f64, f64)> = ids
        .iter()
        .map(|&id| {
            let m = &project.modules[id.index()];
            (id, m.u + du, m.v + dv)
        })
        .collect();
    check_positions(project, &targets)?;
    for &(id, u, v) in &targets {
        let m = &mut project.modules[id.index()];
        m.u = u;
        m.v = v;
    }
    Ok(ChangeSet::modules(ids))
}

pub(super) fn apply_set_positions(
    project: &mut Project,
    positions: &[ModulePosition],
) -> Result<ChangeSet, CommandError> {
    let ids: Vec<ModuleId> = positions.iter().map(|p| p.id).collect();
    if unique_sorted(&ids).len() != ids.len() {
        return Err(CommandError::new(keys::MISMATCHED_LENGTHS));
    }
    check_known(project, &ids)?;
    let targets: Vec<(ModuleId, f64, f64)> = positions.iter().map(|p| (p.id, p.u, p.v)).collect();
    check_positions(project, &targets)?;
    for p in positions {
        let m = &mut project.modules[p.id.index()];
        m.u = p.u;
        m.v = p.v;
    }
    Ok(ChangeSet::modules(unique_sorted(&ids)))
}

pub(super) fn apply_state(
    project: &mut Project,
    ids: &[ModuleId],
    from: ModuleState,
    to: ModuleState,
) -> Result<ChangeSet, CommandError> {
    let ids = unique_sorted(ids);
    check_known(project, &ids)?;
    Ok(set_state(project, &ids, from, to))
}
