//! Structural command: the modules of an array replaced (laying out a field). Module ids are
//! renumbered canonically afterwards (`Project::compact_modules`): the inverse restores the
//! previous modules, and the same canonical numbering gives back the same ids.

use super::{entities::unknown, keys, ChangeSet, Command, CommandError, ModuleSlot};
use crate::ids::{ArrayId, ModuleId};
use crate::project::{ModuleInstance, Project};

/// The modules of `array`, in id order, as slots.
pub(super) fn slots_of(project: &Project, array: ArrayId) -> Vec<ModuleSlot> {
    project
        .modules
        .iter()
        .filter(|m| m.array_id == array)
        .map(|m| ModuleSlot {
            row: m.row,
            col: m.col,
            u: m.u,
            v: m.v,
            state: m.state,
            string_index: m.string_index,
        })
        .collect()
}

pub(super) fn inverse_set_modules(project: &Project, array_id: ArrayId) -> Command {
    Command::SetArrayModules {
        array_id,
        modules: slots_of(project, array_id),
    }
}

pub(super) fn apply_set_modules(
    project: &mut Project,
    array_id: ArrayId,
    slots: &[ModuleSlot],
) -> Result<ChangeSet, CommandError> {
    let ai = project
        .array_index(array_id)
        .ok_or_else(|| unknown("array"))?;
    let array = &project.arrays[ai];
    let face = project
        .roof_face(array.roof_face_id)
        .ok_or_else(|| unknown("roofFace"))?;
    let mut outside = 0usize;
    for s in slots {
        if !s.u.is_finite() || !s.v.is_finite() {
            return Err(CommandError::new(keys::INVALID_NUMBER));
        }
        if !Project::fits_on_roof(array, face, s.u, s.v) {
            outside += 1;
        }
    }
    if outside > 0 {
        return Err(CommandError::new(keys::OUT_OF_ROOF).with_number("count", outside as f64));
    }
    project.modules.retain(|m| m.array_id != array_id);
    project.modules.extend(slots.iter().map(|s| ModuleInstance {
        id: ModuleId(0),
        array_id,
        row: s.row,
        col: s.col,
        u: s.u,
        v: s.v,
        state: s.state,
        string_index: s.string_index,
    }));
    project.compact_modules();
    Ok(ChangeSet {
        arrays: vec![array_id],
        structure: true,
        ..ChangeSet::default()
    })
}
