//! PV Studio domain graph: the project wired through layout, wind zones, loads, load
//! descent and ballast on top of the generic engine `pv-graph`.
//!
//! Architecture: « Le calepinage est le nœud central : le modifier invalide la statique, la
//! nomenclature… », « Les zones de vent sont calculées pour le pan entier, puis attribuées à
//! chaque champ ». The graph computes from the in-memory `Project`; running it on a worker
//! thread and emitting `result:ready` is the application's job.
//!
//! # Nodes
//!
//! | Node | Instance | Reads | Typical cut-off |
//! |---|---|---|---|
//! | inputs ([`inputs`]) | building, face, array, project | the project | — |
//! | [`Outline`](nodes::geometry::Outline) | face | face | — |
//! | [`Roof`](nodes::geometry::Roof) | face | face, building | most face edits |
//! | [`ZoneGeom`](nodes::geometry::ZoneGeom) | face | outline, face, building | rarely changes |
//! | [`Site`](nodes::loads::Site) | building | site, building | — |
//! | [`Hardware`](nodes::layout::Hardware) | array | array, face, modules | — |
//! | [`StructureNode`](nodes::layout::StructureNode) | array | array, modules | — |
//! | [`ModuleZones`](nodes::zones::ModuleZones) | array | array, outline, zone geometry, modules | module moved inside its zone |
//! | [`ZoneSet`](nodes::zones::ZoneSet) | array | module zones | set of zones unchanged |
//! | [`ZoneLoads`](nodes::loads::ZoneLoads) | array | mounting, array, roof, site, zone set | — |
//! | [`Descent`](nodes::descent::Descent) | array | mounting, array, face, loads (+ hardware on rafters) | — |
//! | [`Ballast`](nodes::ballast::Ballast) | array | both halves of an east-west pair | — |
//! | [`FieldStatics`](nodes::summary::FieldStatics) | array (watched) | zone set, descent, ballast, hardware | decisions unchanged |
//!
//! The automatic layout is not a node: it is an on-demand action that issues commands; the
//! graph takes the laid-out modules as input.

pub mod convert;
pub mod error;
pub mod inputs;
pub mod nodes;
pub mod sync;

use pv_graph::{Graph, GraphError};

pub use nodes::summary::{FieldStatics, FieldStaticsResult, ZoneDecision};
pub use sync::{sync_changes, sync_project, SyncReport};

/// Registers every pipeline kind in `graph`.
///
/// # Errors
/// [`GraphError::DuplicateKind`] if called twice on the same graph.
pub fn register(graph: &mut Graph) -> Result<(), GraphError> {
    use inputs::*;
    use nodes::*;
    graph.register_input::<BuildingIn>()?;
    graph.register_input::<SiteIn>()?;
    graph.register_input::<FaceIn>()?;
    graph.register_input::<ArrayIn>()?;
    graph.register_input::<MountingIn>()?;
    graph.register_input::<ModulesIn>()?;
    graph.register_input::<ProjectIndexIn>()?;
    graph.register::<geometry::Outline>()?;
    graph.register::<geometry::Roof>()?;
    graph.register::<geometry::ZoneGeom>()?;
    graph.register::<loads::Site>()?;
    graph.register::<layout::Hardware>()?;
    graph.register::<layout::StructureNode>()?;
    graph.register::<zones::ModuleZones>()?;
    graph.register::<zones::ZoneSet>()?;
    graph.register::<loads::ZoneLoads>()?;
    graph.register::<descent::Descent>()?;
    graph.register::<ballast::Ballast>()?;
    graph.register::<summary::FieldStatics>()?;
    Ok(())
}

/// A graph with every pipeline kind registered.
#[must_use]
pub fn new_graph() -> Graph {
    let mut g = Graph::new();
    register(&mut g).expect("a new graph has no kinds yet");
    g
}
