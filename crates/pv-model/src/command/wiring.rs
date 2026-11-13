//! Wiring command: inverters, strings and electrical settings, replaced as a whole.

use super::{ChangeSet, CommandError};
use crate::project::Project;
use crate::wiring::Wiring;

pub(super) fn apply_set_wiring(
    project: &mut Project,
    wiring: &Wiring,
) -> Result<ChangeSet, CommandError> {
    wiring.validate(project)?;
    let changed = project.wiring != *wiring;
    if changed {
        project.wiring = wiring.clone();
    }
    Ok(ChangeSet {
        wiring: changed,
        ..ChangeSet::default()
    })
}
