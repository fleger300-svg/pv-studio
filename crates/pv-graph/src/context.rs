//! The context a compute function reads its dependencies through.

use std::sync::Arc;

use crate::cancel::CancelToken;
use crate::engine::{Graph, Idx};
use crate::error::GraphError;
use crate::hash::FxHashSet;
use crate::key::{Instance, NodeKey};
use crate::kind::{Kind, Policy};
use crate::value::downcast;

/// Below this many dependencies, duplicates are found by a linear scan (no allocation).
const LINEAR_DEDUP: usize = 16;

/// Handed to [`crate::Derived::compute`]. Every read is recorded as a dependency of the node
/// being computed, in read order; a node may read different nodes from one computation to
/// the next (dynamic dependencies).
pub struct Ctx<'g> {
    pub(crate) graph: &'g mut Graph,
    pub(crate) cancel: &'g CancelToken,
    pub(crate) current: NodeKey,
    pub(crate) policy: Policy,
    pub(crate) deps: Vec<Idx>,
    /// Dependencies of the previous computation: when the reads repeat (the common case),
    /// their indices are reused without hashing the keys.
    pub(crate) prev: Arc<[Idx]>,
    pub(crate) seen: FxHashSet<Idx>,
}

impl Ctx<'_> {
    fn record(&mut self, idx: Idx) {
        if self.deps.len() < LINEAR_DEDUP {
            if !self.deps.contains(&idx) {
                self.deps.push(idx);
            }
        } else {
            if self.seen.is_empty() {
                self.seen.extend(self.deps.iter().copied());
            }
            if self.seen.insert(idx) {
                self.deps.push(idx);
            }
        }
    }

    /// Value of another node (input or derived), brought up to date first.
    ///
    /// # Errors
    /// The stored error of that node (propagate it with `?`), [`GraphError::Cancelled`],
    /// a cycle, or a misuse (unknown kind, an auto node reading an on-demand one).
    pub fn get<K: Kind>(&mut self, instance: Instance) -> Result<Arc<K::Output>, GraphError> {
        let key = K::key(instance);
        let idx = match self.prev.get(self.deps.len()) {
            Some(&c) if self.graph.live_key(c) == Some(key) => c,
            _ => self.graph.intern(key)?,
        };
        self.record(idx);
        if self.policy == Policy::Auto && self.graph.policy_of(idx) == Some(Policy::OnDemand) {
            return Err(GraphError::AutoReadsOnDemand {
                reader: self.current,
                dependency: key,
            });
        }
        self.graph.refresh(idx, self.cancel)?;
        downcast(key, self.graph.stored(idx)?)
    }

    /// The node being computed.
    #[must_use]
    pub fn current(&self) -> NodeKey {
        self.current
    }

    /// For long computations: returns `Err(Cancelled)` once the pass is cancelled.
    ///
    /// # Errors
    /// [`GraphError::Cancelled`].
    pub fn check_cancelled(&self) -> Result<(), GraphError> {
        if self.cancel.is_cancelled() {
            Err(GraphError::Cancelled)
        } else {
            Ok(())
        }
    }

    #[must_use]
    pub fn cancel_token(&self) -> &CancelToken {
        self.cancel
    }
}
