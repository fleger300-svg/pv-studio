//! Graph errors.

use pv_model::CommandError;
use serde::Serialize;

use crate::key::{KindId, NodeKey};

fn path_text(path: &[NodeKey]) -> String {
    path.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" -> ")
}

/// Why a node has no value.
///
/// Every variant except [`GraphError::Cancelled`] is a *result*: it is stored in the node,
/// fingerprinted and propagated to dependents like a value, so that fixing the cause (a new
/// input) recomputes exactly what failed.
#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GraphError {
    /// A domain failure (i18n key), raised by `origin` and propagated downstream.
    #[error("{origin} failed: {error}")]
    Failed {
        origin: NodeKey,
        error: CommandError,
    },
    /// A domain failure returned by a compute function; the graph turns it into `Failed`
    /// with the computing node as origin.
    #[error("compute failed: {0}")]
    Compute(CommandError),
    /// The pass was cancelled; never stored.
    #[error("cancelled")]
    Cancelled,
    #[error("dependency cycle through {}", path_text(.path))]
    Cycle { path: Vec<NodeKey> },
    #[error("input {0} is not set")]
    MissingInput(NodeKey),
    #[error("unknown node kind {0}")]
    UnknownKind(KindId),
    #[error("node kind {0} registered twice")]
    DuplicateKind(KindId),
    #[error("{0} is not an input")]
    NotAnInput(KindId),
    #[error("{0} is an input, it cannot be watched")]
    NotDerived(KindId),
    #[error("{0} holds a value of another type")]
    TypeMismatch(NodeKey),
    #[error("auto node {reader} reads on-demand node {dependency}")]
    AutoReadsOnDemand {
        reader: NodeKey,
        dependency: NodeKey,
    },
}

impl From<CommandError> for GraphError {
    fn from(e: CommandError) -> Self {
        GraphError::Compute(e)
    }
}

impl GraphError {
    /// The form stored in `node`: `Compute` gets its origin.
    pub(crate) fn stored_in(self, node: NodeKey) -> Self {
        match self {
            GraphError::Compute(error) => GraphError::Failed {
                origin: node,
                error,
            },
            e => e,
        }
    }
}
