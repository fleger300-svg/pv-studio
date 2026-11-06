//! The graph: slots, revisions, dirty propagation, verification and recomputation.
//!
//! Storage is an arena: a key is interned once into an index, slots live in a `Vec`, edges
//! are index lists. A read costs one hash lookup (the interning); everything else —
//! verification, dirty propagation, passes — walks indices. (A first version keyed every
//! internal access by `NodeKey` and spent most of its time in cache misses: 2 µs per node.)

use std::collections::HashSet;
use std::sync::Arc;

use pv_model::fingerprint::combine;

use crate::cancel::CancelToken;
use crate::context::Ctx;
use crate::error::GraphError;
use crate::hash::{FxBuild, FxHashMap, FxHashSet};
use crate::key::{Instance, NodeKey};
use crate::kind::{Derived, Input, Kind, Policy};
use crate::registry::{ComputeFn, Entry, Registry};
use crate::status::{NodeStatus, ResultReady, Stats};
use crate::value::{downcast, Stored, Value};

/// Index of a slot in the arena.
pub(crate) type Idx = u32;

/// A stored result; the error is boxed so that slots stay small (they are walked in bulk).
type Memo = Result<Value, Arc<GraphError>>;

#[derive(Default)]
struct Counters {
    /// Computations per kind ordinal.
    computed: Vec<u64>,
    verified: u64,
    dirtied: u64,
}

/// Memo of one node (input or derived).
struct Slot {
    key: NodeKey,
    /// `None` for inputs.
    policy: Option<Policy>,
    compute: Option<Arc<ComputeFn>>,
    /// `None`: input not set, or derived node never computed.
    value: Option<Memo>,
    /// Registration ordinal of the kind (indexes the per-kind counters).
    ordinal: u16,
    /// Fingerprint of the node's identity.
    key_fp: u64,
    output_fp: u64,
    /// Identity + dependencies' output fingerprints (inputs: the value fingerprint).
    input_fp: u64,
    /// Dependencies read by the last computation, in read order (shared: verification
    /// iterates them while refreshing other slots).
    deps: Arc<[Idx]>,
    /// Reverse edges (unordered).
    dependents: Vec<Idx>,
    /// An upstream input changed since `verified_at`.
    dirty: bool,
    /// Revision at which the value was last confirmed up to date.
    verified_at: u64,
    /// Revision at which the output fingerprint last changed.
    changed_at: u64,
    /// Watched: the output fingerprint last reported (`Some(None)`: not reported yet).
    watched: Option<Option<u64>>,
    /// In the pending list of the next pass.
    pending: bool,
    /// Forgotten (its instance was deleted): no key, no value; kept while other nodes still
    /// list it as a dependency, so that they see a change and recompute.
    tomb: bool,
    /// Tomb in the free list, ready for reuse.
    freed: bool,
}

impl Slot {
    fn is_clean(&self) -> bool {
        !self.dirty && self.value.is_some()
    }
}

/// The dependency graph. See the crate documentation for the invariants.
pub struct Graph {
    registry: Registry,
    index: FxHashMap<NodeKey, Idx>,
    /// Live slots of each instance (for [`Graph::forget`]).
    by_instance: FxHashMap<Instance, Vec<Idx>>,
    slots: Vec<Slot>,
    /// Tombs no node depends on any more: reused by the next new keys.
    free: Vec<Idx>,
    revision: u64,
    /// Watched nodes a pass must look at: dirtied, never computed or not yet reported.
    pending: Vec<Idx>,
    /// Nodes being refreshed (cycle detection).
    active: Vec<Idx>,
    /// Recycled dependency buffers: a recomputation that reads the same dependencies as
    /// before (the common case) allocates nothing for them.
    dep_pool: Vec<Vec<Idx>>,
    counters: Counters,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: Registry::default(),
            index: FxHashMap::default(),
            by_instance: FxHashMap::default(),
            slots: Vec::new(),
            free: Vec::new(),
            revision: 1,
            pending: Vec::new(),
            active: Vec::new(),
            dep_pool: Vec::new(),
            counters: Counters::default(),
        }
    }

    // --- Registration --------------------------------------------------------------------

    /// # Errors
    /// [`GraphError::DuplicateKind`].
    pub fn register_input<I: Input>(&mut self) -> Result<(), GraphError> {
        self.registry.register_input::<I>()
    }

    /// # Errors
    /// [`GraphError::DuplicateKind`].
    pub fn register<D: Derived>(&mut self) -> Result<(), GraphError> {
        self.registry.register::<D>()
    }

    /// Index of `key`, creating its slot on first use.
    pub(crate) fn intern(&mut self, key: NodeKey) -> Result<Idx, GraphError> {
        if let Some(&idx) = self.index.get(&key) {
            return Ok(idx);
        }
        let (ordinal, entry) = self.registry.get(key.kind)?;
        let (policy, compute) = match entry {
            Entry::Input => (None, None),
            Entry::Derived { policy, compute } => (Some(*policy), Some(Arc::clone(compute))),
        };
        let slot = Slot {
            key,
            policy,
            compute,
            value: None,
            ordinal,
            key_fp: pv_model::fingerprint(&key),
            output_fp: 0,
            input_fp: 0,
            deps: Arc::from([]),
            dependents: Vec::new(),
            dirty: false,
            verified_at: 0,
            changed_at: 0,
            watched: None,
            pending: false,
            tomb: false,
            freed: false,
        };
        let idx = if let Some(idx) = self.free.pop() {
            self.slots[idx as usize] = slot;
            idx
        } else {
            self.slots.push(slot);
            Idx::try_from(self.slots.len() - 1).expect("fewer than 2^32 nodes")
        };
        self.index.insert(key, idx);
        self.by_instance.entry(key.instance).or_default().push(idx);
        Ok(idx)
    }

    fn slot(&self, idx: Idx) -> &Slot {
        &self.slots[idx as usize]
    }

    fn slot_mut(&mut self, idx: Idx) -> &mut Slot {
        &mut self.slots[idx as usize]
    }

    fn find(&self, key: NodeKey) -> Option<&Slot> {
        self.index.get(&key).map(|&i| self.slot(i))
    }

    /// Key of a live slot (`None` for a forgotten one, whose key may be recreated elsewhere).
    pub(crate) fn live_key(&self, idx: Idx) -> Option<NodeKey> {
        let s = self.slot(idx);
        (!s.tomb).then_some(s.key)
    }

    pub(crate) fn policy_of(&self, idx: Idx) -> Option<Policy> {
        self.slot(idx).policy
    }

    // --- Inputs --------------------------------------------------------------------------

    /// Sets an input. Returns `false` (and changes nothing, not even the revision) when the
    /// value has the same fingerprint as the current one; otherwise bumps the revision and
    /// marks every transitive dependent stale.
    ///
    /// # Errors
    /// [`GraphError::UnknownKind`], [`GraphError::NotAnInput`].
    pub fn set_input<I: Input>(
        &mut self,
        instance: Instance,
        value: I::Output,
    ) -> Result<bool, GraphError> {
        let fp = I::fingerprint(&value);
        self.set_input_value(I::key(instance), Arc::new(value), fp)
    }

    /// Removes an input (entity deleted); dependents become stale and will read
    /// [`GraphError::MissingInput`].
    ///
    /// # Errors
    /// [`GraphError::UnknownKind`], [`GraphError::NotAnInput`].
    pub fn remove_input<I: Input>(&mut self, instance: Instance) -> Result<bool, GraphError> {
        let key = I::key(instance);
        let idx = self.input_slot(key)?;
        if self.slot(idx).value.is_none() {
            return Ok(false);
        }
        self.revision += 1;
        let revision = self.revision;
        let slot = self.slot_mut(idx);
        slot.value = None;
        slot.output_fp = 0;
        slot.input_fp = 0;
        slot.changed_at = revision;
        slot.verified_at = revision;
        self.mark_dependents(idx);
        Ok(true)
    }

    fn input_slot(&mut self, key: NodeKey) -> Result<Idx, GraphError> {
        let idx = self.intern(key)?;
        if self.slot(idx).policy.is_some() {
            return Err(GraphError::NotAnInput(key.kind));
        }
        Ok(idx)
    }

    fn set_input_value(&mut self, key: NodeKey, value: Value, fp: u64) -> Result<bool, GraphError> {
        let idx = self.input_slot(key)?;
        let slot = self.slot(idx);
        if matches!(slot.value, Some(Ok(_))) && slot.output_fp == fp {
            return Ok(false);
        }
        self.revision += 1;
        let revision = self.revision;
        let slot = self.slot_mut(idx);
        slot.value = Some(Ok(value));
        slot.output_fp = fp;
        slot.input_fp = fp;
        slot.changed_at = revision;
        slot.verified_at = revision;
        self.mark_dependents(idx);
        Ok(true)
    }

    /// Marks the transitive dependents of `idx` dirty. Stops at nodes already dirty: by the
    /// dirty-closure invariant their dependents are dirty too.
    fn mark_dependents(&mut self, idx: Idx) {
        let mut stack: Vec<Idx> = self.slot(idx).dependents.clone();
        while let Some(i) = stack.pop() {
            let s = &mut self.slots[i as usize];
            if s.dirty {
                continue;
            }
            s.dirty = true;
            self.counters.dirtied += 1;
            stack.extend_from_slice(&s.dependents);
            if s.watched.is_some() && !s.pending {
                s.pending = true;
                self.pending.push(i);
            }
        }
    }

    /// Removes `dependent` from the reverse edges of `d`; a tomb nobody depends on any more
    /// goes to the free list.
    fn unlink(&mut self, d: Idx, dependent: Idx) {
        let s = &mut self.slots[d as usize];
        s.dependents.retain(|&x| x != dependent);
        if s.tomb && !s.freed && s.dependents.is_empty() {
            s.freed = true;
            self.free.push(d);
        }
    }

    /// Forgets every node of an entity that was deleted (a field, a face…): inputs and
    /// derived nodes alike. Nodes of *other* instances that read them become stale and, when
    /// recomputed, read [`GraphError::MissingInput`] (or whatever the recreated nodes give).
    /// The slots are reused once no node depends on them. Returns the number of nodes
    /// forgotten.
    pub fn forget(&mut self, instance: Instance) -> usize {
        let Some(idxs) = self.by_instance.remove(&instance) else {
            return 0;
        };
        self.revision += 1;
        let revision = self.revision;
        for &idx in &idxs {
            self.mark_dependents(idx);
        }
        let mut old_deps = Vec::new();
        for &idx in &idxs {
            let s = &mut self.slots[idx as usize];
            self.index.remove(&s.key);
            old_deps.push((idx, std::mem::replace(&mut s.deps, Arc::from([]))));
            s.tomb = true;
            s.value = None;
            s.policy = None;
            s.compute = None;
            s.watched = None;
            s.dirty = false;
            s.output_fp = 0;
            s.input_fp = 0;
            // Readers compare against this: they see a change and recompute.
            s.changed_at = revision;
            s.verified_at = revision;
        }
        for (idx, deps) in old_deps {
            for &d in deps.iter() {
                self.unlink(d, idx);
            }
        }
        for &idx in &idxs {
            let s = &mut self.slots[idx as usize];
            if !s.freed && s.dependents.is_empty() {
                s.freed = true;
                self.free.push(idx);
            }
        }
        idxs.len()
    }

    // --- Reading -------------------------------------------------------------------------

    /// Value of a node, computed if needed (whatever its policy).
    ///
    /// # Errors
    /// The node's stored error, or a misuse (unknown kind, cycle).
    pub fn get<K: Kind>(&mut self, instance: Instance) -> Result<Arc<K::Output>, GraphError> {
        self.get_with::<K>(instance, &CancelToken::new())
    }

    /// [`Graph::get`] with a cancellation token.
    ///
    /// # Errors
    /// As [`Graph::get`], plus [`GraphError::Cancelled`].
    pub fn get_with<K: Kind>(
        &mut self,
        instance: Instance,
        cancel: &CancelToken,
    ) -> Result<Arc<K::Output>, GraphError> {
        let key = K::key(instance);
        let idx = self.intern(key)?;
        self.refresh(idx, cancel)?;
        downcast(key, self.stored(idx)?)
    }

    pub(crate) fn stored(&self, idx: Idx) -> Stored {
        let s = self.slot(idx);
        match &s.value {
            Some(Ok(v)) => Ok(Arc::clone(v)),
            Some(Err(e)) => Err(GraphError::clone(e)),
            None => Err(GraphError::MissingInput(s.key)),
        }
    }

    /// Brings `idx` up to date. `Err` only for what is not a storable result: cancellation,
    /// or a cycle reaching a node being refreshed.
    pub(crate) fn refresh(&mut self, idx: Idx, cancel: &CancelToken) -> Result<(), GraphError> {
        if cancel.is_cancelled() {
            return Err(GraphError::Cancelled);
        }
        let s = self.slot(idx);
        if s.is_clean() {
            return Ok(());
        }
        let Some(policy) = s.policy else {
            return Ok(()); // input
        };
        if let Some(pos) = self.active.iter().position(|&a| a == idx) {
            let mut path: Vec<NodeKey> = self.active[pos..]
                .iter()
                .map(|&a| self.slot(a).key)
                .collect();
            path.push(self.slot(idx).key);
            return Err(GraphError::Cycle { path });
        }
        self.active.push(idx);
        let r = self.refresh_derived(idx, policy, cancel);
        self.active.pop();
        r
    }

    fn refresh_derived(
        &mut self,
        idx: Idx,
        policy: Policy,
        cancel: &CancelToken,
    ) -> Result<(), GraphError> {
        // The previous dependencies: verified first, then handed to the computation so that
        // repeated reads skip the key lookup (one shared clone for both).
        let s = self.slot(idx);
        let (key, prev, has_value, verified_at) =
            (s.key, Arc::clone(&s.deps), s.value.is_some(), s.verified_at);

        // 1. A stale node whose dependencies all kept their output is still valid.
        if has_value {
            let mut changed = false;
            for &d in prev.iter() {
                match self.refresh(d, cancel) {
                    Err(GraphError::Cancelled) => return Err(GraphError::Cancelled),
                    Err(_) => changed = true,
                    Ok(()) => changed = self.slot(d).changed_at > verified_at,
                }
                if changed {
                    break;
                }
            }
            if !changed {
                let revision = self.revision;
                let s = self.slot_mut(idx);
                s.dirty = false;
                s.verified_at = revision;
                self.counters.verified += 1;
                return Ok(());
            }
        }

        // 2. Recompute, recording the reads.
        let Some(compute) = self.slot(idx).compute.clone() else {
            return Ok(());
        };
        let buffer = self.dep_pool.pop().unwrap_or_default();
        let mut ctx = Ctx {
            graph: self,
            cancel,
            current: key,
            policy,
            deps: buffer,
            prev,
            seen: FxHashSet::default(),
        };
        let result = compute(&mut ctx, key.instance);
        let Ctx { deps, prev, .. } = ctx;
        if cancel.is_cancelled() || matches!(result, Err(GraphError::Cancelled)) {
            self.recycle(deps);
            return Err(GraphError::Cancelled);
        }
        let (value, output_fp) = match result {
            Ok((v, fp)) => (Ok(v), fp),
            Err(e) => {
                let e = e.stored_in(key);
                let fp = pv_model::fingerprint(&e);
                (Err(Arc::new(e)), fp)
            }
        };
        self.store(idx, value, output_fp, deps, &prev);
        let ordinal = usize::from(self.slot(idx).ordinal);
        if self.counters.computed.len() <= ordinal {
            self.counters.computed.resize(ordinal + 1, 0);
        }
        self.counters.computed[ordinal] += 1;
        Ok(())
    }

    /// `prev`: the dependencies before this computation (the slot still holds them).
    fn store(&mut self, idx: Idx, value: Memo, output_fp: u64, deps: Vec<Idx>, prev: &[Idx]) {
        if *prev == *deps {
            // Same reads as before (the common case): the slot keeps its list, no allocation.
            self.recycle(deps);
        } else {
            self.relink(idx, prev, &deps);
            self.slot_mut(idx).deps = deps.into();
        }
        let s = self.slot(idx);
        let input_fp = combine(
            std::iter::once(s.key_fp).chain(s.deps.iter().map(|&d| self.slot(d).output_fp)),
        );
        let revision = self.revision;
        let slot = self.slot_mut(idx);
        let changed = slot.value.is_none() || slot.output_fp != output_fp;
        slot.input_fp = input_fp;
        slot.value = Some(value);
        slot.output_fp = output_fp;
        slot.dirty = false;
        slot.verified_at = revision;
        if changed {
            slot.changed_at = revision;
        }
    }

    fn recycle(&mut self, mut buffer: Vec<Idx>) {
        // Bounded: only buffers of usual size, and as many as the recursion can hold.
        if buffer.capacity() <= 64 && self.dep_pool.len() < 64 {
            buffer.clear();
            self.dep_pool.push(buffer);
        }
    }

    /// Updates reverse edges when a node's dependencies change.
    fn relink(&mut self, idx: Idx, old: &[Idx], new: &[Idx]) {
        let (old_set, new_set): (FxHashSet<Idx>, FxHashSet<Idx>) =
            (old.iter().copied().collect(), new.iter().copied().collect());
        for &d in old.iter().filter(|d| !new_set.contains(d)) {
            self.unlink(d, idx);
        }
        for &d in new.iter().filter(|d| !old_set.contains(d)) {
            self.slot_mut(d).dependents.push(idx);
        }
    }

    // --- Passes --------------------------------------------------------------------------

    /// Watches a derived node: [`Graph::run_auto`] keeps it up to date (policy `Auto`) or
    /// reports it stale (policy `OnDemand`), and emits a [`ResultReady`] when it changes.
    ///
    /// # Errors
    /// [`GraphError::UnknownKind`], [`GraphError::NotDerived`].
    pub fn watch<D: Derived>(&mut self, instance: Instance) -> Result<(), GraphError> {
        self.watch_key(D::key(instance))
    }

    /// # Errors
    /// [`GraphError::UnknownKind`], [`GraphError::NotDerived`].
    pub fn watch_key(&mut self, key: NodeKey) -> Result<(), GraphError> {
        let idx = self.intern(key)?;
        let s = &mut self.slots[idx as usize];
        if s.policy.is_none() {
            return Err(GraphError::NotDerived(key.kind));
        }
        if s.watched.is_none() {
            s.watched = Some(None);
            if !s.pending {
                s.pending = true;
                self.pending.push(idx);
            }
        }
        Ok(())
    }

    pub fn unwatch_key(&mut self, key: NodeKey) -> bool {
        let Some(&idx) = self.index.get(&key) else {
            return false;
        };
        self.slot_mut(idx).watched.take().is_some()
    }

    /// Watched nodes, in key order.
    #[must_use]
    pub fn watched(&self) -> Vec<NodeKey> {
        let mut keys: Vec<NodeKey> = self
            .slots
            .iter()
            .filter(|s| s.watched.is_some())
            .map(|s| s.key)
            .collect();
        keys.sort_unstable();
        keys
    }

    /// One recompute pass over the watched nodes that need it, in creation order: `Auto`
    /// nodes are brought up to date; `OnDemand` nodes are cleared without computation when
    /// their dependencies kept their outputs, and stay stale otherwise. Returns the watched
    /// nodes whose result changed since they were last reported.
    ///
    /// # Errors
    /// [`GraphError::Cancelled`]: the pass stopped; nothing is reported, the graph stays
    /// consistent and the next pass resumes the work.
    pub fn run_auto(&mut self, cancel: &CancelToken) -> Result<Vec<ResultReady>, GraphError> {
        if cancel.is_cancelled() {
            return Err(GraphError::Cancelled);
        }
        self.pending.sort_unstable();
        let pending = self.pending.clone();
        for &idx in &pending {
            let s = self.slot(idx);
            if s.watched.is_some() && s.policy == Some(Policy::Auto) {
                if let Err(GraphError::Cancelled) = self.refresh(idx, cancel) {
                    return Err(GraphError::Cancelled);
                }
            }
        }
        for &idx in &pending {
            let s = self.slot(idx);
            if s.watched.is_some() && s.policy == Some(Policy::OnDemand) {
                self.try_clear(idx, cancel, &mut HashSet::default())?;
            }
        }
        let revision = self.revision;
        let mut ready = Vec::new();
        self.pending.clear();
        for idx in pending {
            let s = &mut self.slots[idx as usize];
            let Some(last) = s.watched else {
                s.pending = false; // unwatched meanwhile
                continue;
            };
            if !s.is_clean() {
                self.pending.push(idx); // on-demand, stale or never computed
                continue;
            }
            s.pending = false;
            // Against the last *reported* output: a result that changed and came back
            // between two passes is not re-emitted.
            if last != Some(s.output_fp) {
                s.watched = Some(Some(s.output_fp));
                ready.push(ResultReady {
                    key: s.key,
                    revision,
                    input_fingerprint: s.input_fp,
                    output_fingerprint: s.output_fp,
                    failed: matches!(s.value, Some(Err(_))),
                });
            }
        }
        Ok(ready)
    }

    /// Clears a stale on-demand node without computing it, when possible: `Auto`
    /// dependencies are refreshed, `OnDemand` ones cleared recursively. `Ok(false)` when it
    /// really needs a recomputation.
    fn try_clear(
        &mut self,
        idx: Idx,
        cancel: &CancelToken,
        visiting: &mut HashSet<Idx, FxBuild>,
    ) -> Result<bool, GraphError> {
        let s = self.slot(idx);
        match s.policy {
            None => return Ok(true),
            Some(Policy::Auto) => {
                return match self.refresh(idx, cancel) {
                    Err(GraphError::Cancelled) => Err(GraphError::Cancelled),
                    _ => Ok(true),
                };
            }
            Some(Policy::OnDemand) => {}
        }
        if s.value.is_none() {
            return Ok(false);
        }
        if !s.dirty {
            return Ok(true);
        }
        let (verified_at, deps) = (s.verified_at, Arc::clone(&s.deps));
        if !visiting.insert(idx) {
            return Ok(false);
        }
        for &d in deps.iter() {
            if !self.try_clear(d, cancel, visiting)? || self.slot(d).changed_at > verified_at {
                return Ok(false);
            }
        }
        let revision = self.revision;
        let s = self.slot_mut(idx);
        s.dirty = false;
        s.verified_at = revision;
        self.counters.verified += 1;
        Ok(true)
    }

    // --- Introspection -------------------------------------------------------------------

    /// Current revision; bumped by every input change.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn status(&self, key: NodeKey) -> NodeStatus {
        let Some(s) = self.find(key) else {
            return NodeStatus::Missing;
        };
        match &s.value {
            None => NodeStatus::Missing,
            Some(_) if s.dirty => NodeStatus::Stale,
            Some(Ok(_)) => NodeStatus::Fresh,
            Some(Err(e)) => NodeStatus::Failed(GraphError::clone(e)),
        }
    }

    #[must_use]
    pub fn status_of<K: Kind>(&self, instance: Instance) -> NodeStatus {
        self.status(K::key(instance))
    }

    /// Watched nodes that are not up to date (the « résultats à recalculer » badge), in key
    /// order.
    #[must_use]
    pub fn stale(&self) -> Vec<NodeKey> {
        self.watched()
            .into_iter()
            .filter(|k| matches!(self.status(*k), NodeStatus::Missing | NodeStatus::Stale))
            .collect()
    }

    /// Dependencies of the last computation, in read order.
    #[must_use]
    pub fn dependencies(&self, key: NodeKey) -> Vec<NodeKey> {
        self.find(key)
            .map(|s| s.deps.iter().map(|&d| self.slot(d).key).collect())
            .unwrap_or_default()
    }

    /// Dependents, in key order.
    #[must_use]
    pub fn dependents(&self, key: NodeKey) -> Vec<NodeKey> {
        let mut out: Vec<NodeKey> = self
            .find(key)
            .map(|s| s.dependents.iter().map(|&d| self.slot(d).key).collect())
            .unwrap_or_default();
        out.sort_unstable();
        out
    }

    /// Fingerprint of the inputs of the node's current value.
    #[must_use]
    pub fn input_fingerprint(&self, key: NodeKey) -> Option<u64> {
        self.find(key)
            .filter(|s| s.value.is_some())
            .map(|s| s.input_fp)
    }

    /// Live nodes (forgotten ones excluded).
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.index.len()
    }

    /// Slots allocated, live or awaiting reuse (to check that forgetting does not leak).
    #[must_use]
    pub fn slot_capacity(&self) -> usize {
        self.slots.len()
    }

    /// Counters since the graph was created or [`Graph::reset_stats`] was called.
    #[must_use]
    pub fn stats(&self) -> Stats {
        Stats {
            computed: self
                .registry
                .names()
                .iter()
                .zip(&self.counters.computed)
                .filter(|(_, &n)| n > 0)
                .map(|(&name, &n)| (name, n))
                .collect(),
            verified: self.counters.verified,
            dirtied: self.counters.dirtied,
        }
    }

    pub fn reset_stats(&mut self) {
        self.counters = Counters::default();
    }
}
