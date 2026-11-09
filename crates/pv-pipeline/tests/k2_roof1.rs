//! Project A, roof 1, built as a `Project` and computed through the graph, against the
//! K2 Base report: wind zones per field, fixation forces and rail checks per zone (±3 %),
//! fixation spacing and climber sets (0 deviation).

mod common;

use common::*;
use pv_graph::Instance;
use pv_model::WindZone;
use pv_pipeline::FieldStatics;

#[test]
fn roof_1_through_the_graph_matches_k2() {
    let p = ref_a_roof1();
    let mut g = graph_for(&p);
    let (compared, failures) = k2::roof1_deviations(&mut g, &p);
    assert!(compared > 80, "{compared} values compared");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn field_statics_summarise_the_decisions() {
    let p = ref_a_roof1();
    let mut g = graph_for(&p);
    for f in 0..2 {
        let s = g.get::<FieldStatics>(array(f, &p)).unwrap();
        assert_eq!(s.zones.len(), 5);
        assert_eq!(s.decisions.len(), 5);
        assert!(s
            .decisions
            .iter()
            .all(|d| (d.fixation_spacing_m - 0.6).abs() < 1e-9));
        assert!(s.fixations.unwrap() > 0);
        assert_eq!(s.ballast_blocks, None);
        assert!(s.decisions.iter().any(|d| d.zone == WindZone::EaveCorner));
    }
    assert!(g.stale().is_empty());
    assert_eq!(g.watched().len(), 3); // two fields and the bill of materials
    let _ = Instance::Project;
}
