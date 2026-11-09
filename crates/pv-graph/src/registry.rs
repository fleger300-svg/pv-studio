//! Registered node kinds, type-erased.

use std::sync::Arc;

use crate::context::Ctx;
use crate::error::GraphError;
use crate::hash::FxHashMap;
use crate::key::{Instance, KindId};
use crate::kind::{Derived, Input, Kind, Policy};
use crate::value::Value;

/// A type-erased compute function: value and output fingerprint.
pub(crate) type ComputeFn =
    dyn Fn(&mut Ctx<'_>, Instance) -> Result<(Value, u64), GraphError> + Send + Sync;

#[derive(Clone)]
pub(crate) enum Entry {
    Input,
    Derived {
        policy: Policy,
        compute: Arc<ComputeFn>,
    },
}

/// Kinds in registration order (the ordinal indexes per-kind counters).
#[derive(Default)]
pub(crate) struct Registry {
    kinds: FxHashMap<KindId, (u16, Entry)>,
    names: Vec<&'static str>,
}

impl Registry {
    fn insert(&mut self, kind: KindId, entry: Entry) -> Result<(), GraphError> {
        if self.kinds.contains_key(&kind) {
            return Err(GraphError::DuplicateKind(kind));
        }
        let ordinal =
            u16::try_from(self.names.len()).map_err(|_| GraphError::DuplicateKind(kind))?;
        self.kinds.insert(kind, (ordinal, entry));
        self.names.push(kind.0);
        Ok(())
    }

    pub(crate) fn register_input<I: Input>(&mut self) -> Result<(), GraphError> {
        self.insert(KindId(I::NAME), Entry::Input)
    }

    pub(crate) fn register<D: Derived>(&mut self) -> Result<(), GraphError> {
        let compute: Arc<ComputeFn> = Arc::new(|ctx: &mut Ctx<'_>, instance: Instance| {
            let out = D::compute(ctx, instance)?;
            let fp = <D as Kind>::fingerprint(&out);
            Ok((Arc::new(out) as Value, fp))
        });
        self.insert(
            KindId(D::NAME),
            Entry::Derived {
                policy: D::POLICY,
                compute,
            },
        )
    }

    /// Ordinal and entry of a kind.
    pub(crate) fn get(&self, kind: KindId) -> Result<(u16, &Entry), GraphError> {
        self.kinds
            .get(&kind)
            .map(|(o, e)| (*o, e))
            .ok_or(GraphError::UnknownKind(kind))
    }

    pub(crate) fn names(&self) -> &[&'static str] {
        &self.names
    }
}
