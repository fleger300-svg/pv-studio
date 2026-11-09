//! PV Studio dependency graph.
//!
//! Architecture: « Un graphe de dépendances décide quoi recalculer après chaque commande »,
//! « Une modification n'invalide que ce qui en dépend ». This crate is the generic engine:
//! it knows nothing about modules or SIA 261; the domain nodes (layout, zones, loads,
//! ballast, BOM) are plugged in on top of it.
//!
//! # Model
//!
//! - **Inputs** ([`Input`]) hold project data and are set by commands
//!   ([`Graph::set_input`]); every change bumps the [revision](Graph::revision).
//! - **Derived nodes** ([`Derived`]) are pure functions of what they read through their
//!   [`Ctx`]. The reads are recorded as the node's dependencies at each computation, so
//!   they may differ from one computation to the next (dynamic dependencies, in the manner
//!   of `salsa`).
//! - A node is identified by a [`NodeKey`]: its kind and the [`Instance`] it is computed for.
//!
//! # Lifecycle of a change
//!
//! 1. `set_input` marks every transitive dependent *dirty* (push).
//! 2. A pass ([`Graph::run_auto`]) or a read ([`Graph::get`]) *pulls*: a dirty node first
//!    refreshes its dependencies in read order; if none of them changed its output since
//!    the node was last verified, the node is confirmed without recomputation.
//! 3. **Early cut-off**: a recomputed node whose output fingerprint is unchanged keeps its
//!    `changed_at`, so its dependents are confirmed without recomputation.
//!
//! # Invariants
//!
//! 1. *Dirty closure*: if a node is dirty, every one of its dependents is dirty.
//! 2. A clean derived node holds what its compute function would return now.
//! 3. `changed_at` only moves when the output fingerprint changes.
//! 4. [`GraphError::Cancelled`] is never stored: a cancelled pass leaves every node either
//!    clean and correct, or dirty.
//! 5. Errors are results: a failing node stores its error (with its origin), dependents
//!    propagate it, and a later input change recomputes exactly what failed.
//! 6. Determinism: the same sequence of calls computes the same nodes in the same order
//!    (watched nodes in creation order, dependencies in read order; no hash-map iteration
//!    decides an order).
//!
//! Refreshing recurses along dependencies: the stack depth is the depth of the graph (a
//! dozen levels in PV Studio), not its size. Computation is sequential (decision G1: a
//! parallel pass only if the G4 measurements call for it).

pub mod cancel;
pub mod context;
pub mod engine;
pub mod error;
mod hash;
pub mod key;
pub mod kind;
mod registry;
pub mod status;
pub mod value;

pub use cancel::CancelToken;
pub use context::Ctx;
pub use engine::Graph;
pub use error::GraphError;
pub use key::{Instance, KindId, NodeKey};
pub use kind::{serde_fingerprint, Derived, Input, Kind, Policy};
pub use status::{NodeStatus, ResultReady, Stats};
pub use value::{Stored, Value};
