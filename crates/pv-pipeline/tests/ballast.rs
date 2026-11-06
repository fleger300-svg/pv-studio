//! East-west ballast through the graph (reference B, 9 m, D-Dome on gravel): pairs formed
//! across the two halves, K2's blocks for an isolated pair per zone, sharing over the
//! mounting blocks equal to the direct engine call.

mod common;

use common::*;
use pv_graph::GraphError;
use pv_model::command::keys;
use pv_pipeline::nodes::ballast::Ballast;
use pv_pipeline::nodes::layout::StructureNode;
use pv_pipeline::FieldStatics;
use pv_statics::ballast::share_ballast_on_structure;
use pv_statics::RoofZone;

#[test]
fn east_west_ballast_is_computed_once_per_pair() {
    let p = ref_b_east_west();
    let mut g = graph_for(&p);
    let (primary, second) = if p.arrays[0].id < p.arrays[1].id {
        (0, 1)
    } else {
        (1, 0)
    };
    let plan = g.get::<Ballast>(array(primary, &p)).unwrap();
    let plan = plan
        .as_ref()
        .as_ref()
        .expect("primary half carries the ballast");
    assert!(g.get::<Ballast>(array(second, &p)).unwrap().is_none());

    // Every pair found its partner in the other half.
    assert!(plan.pairs.len() > 10);
    assert!(plan.pairs.iter().all(|pr| pr.partner.is_some()));
    // An edge zone and the interior both occur.
    assert!(plan.pairs.iter().any(|pr| pr.zone == RoofZone::Interior));
    assert!(plan.pairs.iter().any(|pr| pr.zone == RoofZone::EaveCorner));

    // K2 decisions for an isolated pair: 4 blocks in the eave corner, 1 in the interior.
    for pr in &plan.pairs {
        match pr.zone {
            RoofZone::EaveCorner => assert_eq!(pr.blocks_alone, 4),
            RoofZone::Interior => assert_eq!(pr.blocks_alone, 1),
            _ => {}
        }
    }

    // Sharing over the mounting blocks: the direct engine call on the same data.
    let field = g.get::<StructureNode>(array(primary, &p)).unwrap();
    let required: Vec<f64> = plan.pairs.iter().map(|pr| pr.required_mass_kg).collect();
    let own = vec![47.5; required.len()];
    let direct = share_ballast_on_structure(&field.structure, &required, &own, 11.0).unwrap();
    assert_eq!(plan.groups, direct);
    assert_eq!(
        plan.total_blocks,
        direct.iter().map(|g| g.blocks).sum::<u32>()
    );
    // Linked pairs share: fewer blocks than every pair alone.
    let alone: u32 = plan.pairs.iter().map(|pr| pr.blocks_alone).sum();
    assert!(plan.total_blocks <= alone);

    let s = g.get::<FieldStatics>(array(primary, &p)).unwrap();
    assert_eq!(s.ballast_blocks, Some(plan.total_blocks));
    assert!(s.decisions.is_empty());
}

#[test]
fn an_unpaired_ballasted_field_is_an_error() {
    let mut p = ref_b_east_west();
    p.arrays[0].layout.paired_with = None;
    let mut g = graph_for(&p);
    match g.get::<Ballast>(array(0, &p)) {
        Err(GraphError::Failed { error, .. }) => {
            assert_eq!(error.key, keys::STATICS_EAST_WEST_UNPAIRED);
        }
        other => panic!("{other:?}"),
    }
}
