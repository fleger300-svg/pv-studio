//! Project → graph inputs, in one call (tests, tools). The application splits the same
//! path across threads: [`crate::batch`] extraction on the command thread, application on
//! the compute thread.

use pv_graph::{Graph, GraphError};
use pv_model::{ChangeSet, Project};

pub use crate::batch::SyncReport;
use crate::batch::{apply_batch, batch_for_changes, batch_for_project};

/// Sets every input from `project`, forgets the entities that disappeared and watches the
/// statics of every field.
///
/// # Errors
/// Only if the pipeline kinds are not registered in `graph` (see [`crate::register`]).
pub fn sync_project(graph: &mut Graph, project: &Project) -> Result<SyncReport, GraphError> {
    apply_batch(graph, batch_for_project(project))
}

/// After a command: sets the inputs it changed (see [`batch_for_changes`]).
///
/// # Errors
/// Only if the pipeline kinds are not registered in `graph`.
pub fn sync_changes(
    graph: &mut Graph,
    project: &Project,
    changes: &ChangeSet,
) -> Result<SyncReport, GraphError> {
    apply_batch(graph, batch_for_changes(project, changes))
}
