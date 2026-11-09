//! Mounting hardware of a field: blocks, rails, fixations (`pv-layout`).

use pv_graph::{Ctx, Derived, GraphError, Instance};
use pv_layout::model::{hardware, structure, ArrayHardware};
use pv_layout::Structure;
use pv_model::ModuleId;
use serde::Serialize;

use super::{array_of, kind};
use crate::error::layout_error;
use crate::inputs::{ArrayIn, FaceIn, ModulesIn};

kind!(
    /// Blocks, rails and fixations of a field; `None` when its system has no rail rules.
    Hardware,
    "array.hardware",
    Option<ArrayHardware>
);

impl Derived for Hardware {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<Option<ArrayHardware>, GraphError> {
        array_of(i)?;
        let array = ctx.get::<ArrayIn>(i)?;
        let face = ctx.get::<FaceIn>(Instance::Face(array.roof_face_id))?;
        let modules = ctx.get::<ModulesIn>(i)?;
        hardware(&face, &array, &modules).map_err(|e| layout_error(&e).into())
    }
}

/// Mounting blocks of a field and the module of each position.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldStructure {
    pub structure: Structure,
    /// Module of each structure position, in position order.
    pub module_ids: Vec<ModuleId>,
}

kind!(
    /// Mounting blocks of a field, rails or not (ballast sharing groups).
    StructureNode,
    "array.structure",
    FieldStructure
);

impl Derived for StructureNode {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<FieldStructure, GraphError> {
        array_of(i)?;
        let array = ctx.get::<ArrayIn>(i)?;
        let modules = ctx.get::<ModulesIn>(i)?;
        let (structure, module_ids) =
            structure(&array, &modules).map_err(|e| GraphError::from(layout_error(&e)))?;
        Ok(FieldStructure {
            structure,
            module_ids,
        })
    }
}
