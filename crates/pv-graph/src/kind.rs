//! Node kinds: inputs (set by commands) and derived nodes (computed from other nodes).

use serde::Serialize;

use crate::context::Ctx;
use crate::error::GraphError;
use crate::key::{Instance, KindId, NodeKey};

/// When a derived node is recomputed (architecture, « Règles d'invalidation »).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Policy {
    /// Recomputed by [`crate::Graph::run_auto`] (statics, ballast, BOM: < 100 ms).
    Auto,
    /// Only marked stale; recomputed when explicitly requested (simulation, auto-stringing).
    /// An `Auto` node may not read an `OnDemand` one.
    OnDemand,
}

/// What every node kind declares.
pub trait Kind: 'static {
    /// The value of the node. Shared as `Arc<Output>`, and sent to other threads.
    type Output: Send + Sync + 'static;
    /// Unique name of the kind within the graph.
    const NAME: &'static str;

    /// Fingerprint of a value, used for the early cut-off: a recomputed node whose output
    /// fingerprint is unchanged does not invalidate its dependents. Use
    /// [`serde_fingerprint`] unless a cheaper exact hash exists.
    fn fingerprint(output: &Self::Output) -> u64;

    #[must_use]
    fn key(instance: Instance) -> NodeKey {
        NodeKey::new(KindId(Self::NAME), instance)
    }
}

/// The default fingerprint of a serialisable value (FNV-1a of its JSON form).
#[must_use]
pub fn serde_fingerprint<T: Serialize + ?Sized>(value: &T) -> u64 {
    pv_model::fingerprint(value)
}

/// A value supplied from outside (project data), set with [`crate::Graph::set_input`].
pub trait Input: Kind {}

/// A value computed from other nodes. `compute` must be a pure function of what it reads
/// through the [`Ctx`]: the graph records those reads as the node's dependencies.
pub trait Derived: Kind {
    const POLICY: Policy = Policy::Auto;

    /// Computes the node. Errors from upstream nodes propagate with `?`; a domain failure is
    /// returned as `Err(command_error.into())` and becomes [`GraphError::Failed`] with this
    /// node as origin.
    fn compute(ctx: &mut Ctx<'_>, instance: Instance) -> Result<Self::Output, GraphError>;
}
