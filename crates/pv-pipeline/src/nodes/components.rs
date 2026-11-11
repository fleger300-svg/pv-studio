//! Resolution of the library components: rebuilds, per array, the mounting system the engines
//! use from the frozen components. Engines downstream read this node as they read the old
//! hard-coded snapshot; an update of a component that does not change the resolved system
//! stops here (early cut-off). It reads the array's references only, never its layout.

use pv_graph::{Ctx, Derived, GraphError, Instance};
use pv_model::project::resolve_component_refs;
use pv_model::MountingSystem;

use super::{array_of, kind};
use crate::inputs::{ArrayComponentsIn, ComponentsIn};

kind!(
    /// Mounting system of an array, resolved from its components (`None`: none chosen).
    MountingNode,
    "array.mounting",
    Option<MountingSystem>
);

impl Derived for MountingNode {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<Option<MountingSystem>, GraphError> {
        array_of(i)?;
        let (module, mounting) = *ctx.get::<ArrayComponentsIn>(i)?;
        if mounting.is_none() {
            return Ok(None);
        }
        let components = ctx.get::<ComponentsIn>(Instance::Project)?;
        resolve_component_refs(&components, module, mounting).map_err(Into::into)
    }
}
