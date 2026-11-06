//! Node status (the UI's « résultats à recalculer » badge), result events and counters.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::error::GraphError;
use crate::key::NodeKey;

/// State of a node at the current revision.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", content = "error", rename_all = "camelCase")]
pub enum NodeStatus {
    /// Never computed (derived) or not set (input).
    Missing,
    /// May be out of date: an upstream input changed since it was last verified.
    Stale,
    /// Up to date with a value.
    Fresh,
    /// Up to date, and the up-to-date result is an error.
    Failed(GraphError),
}

/// A watched node whose result changed, emitted after a pass (the `result:ready` event).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultReady {
    pub key: NodeKey,
    /// Revision the result is valid for.
    pub revision: u64,
    /// Fingerprint of the node's inputs (its identity and its dependencies' outputs): a
    /// result whose fingerprint has been superseded is ignored by the front end.
    pub input_fingerprint: u64,
    pub output_fingerprint: u64,
    pub failed: bool,
}

/// Counters, for tests and the performance HUD.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Computations per kind.
    pub computed: BTreeMap<&'static str, u64>,
    /// Stale nodes confirmed up to date without recomputation.
    pub verified: u64,
    /// Nodes marked stale by input changes.
    pub dirtied: u64,
}

impl Stats {
    #[must_use]
    pub fn computed(&self, kind: &str) -> u64 {
        self.computed.get(kind).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn total_computed(&self) -> u64 {
        self.computed.values().sum()
    }
}
