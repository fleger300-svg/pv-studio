//! `Batch`: several commands applied as one, all or nothing (the copilot's proposals and the
//! automatic layout go through it; one undo reverts the whole batch).

use super::{ChangeSet, Command, CommandError};
use crate::project::Project;

/// Applies the commands in order on a working copy; the project changes only if all succeed.
pub(super) fn apply_batch(
    project: &mut Project,
    commands: &[Command],
) -> Result<ChangeSet, CommandError> {
    let mut work = project.clone();
    let mut changes = ChangeSet::default();
    for c in commands {
        changes.merge(c.apply(&mut work)?);
    }
    *project = work;
    Ok(changes)
}

/// The inverses of the commands, each computed on the state it applies to, in reverse order.
pub(super) fn inverse_batch(project: &Project, commands: &[Command]) -> Command {
    let mut work = project.clone();
    let mut inverses = Vec::with_capacity(commands.len());
    for c in commands {
        inverses.push(c.inverse(&work));
        // A failing command fails the whole batch: its inverse is then never used.
        if c.apply(&mut work).is_err() {
            break;
        }
    }
    inverses.reverse();
    Command::Batch { commands: inverses }
}
