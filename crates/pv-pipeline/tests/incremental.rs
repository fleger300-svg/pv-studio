//! Which nodes recompute after each kind of change (counted, exact), on project A roof 1.

mod common;

use common::*;
use pv_graph::{CancelToken, Graph, NodeStatus, Stats};
use pv_model::{ChangeSet, ModuleState, Project};
use pv_pipeline::nodes::zones::ModuleZones;
use pv_pipeline::{sync_changes, sync_project, FieldStatics};
use pv_statics::RoofZone;

fn pass(g: &mut Graph) -> Stats {
    g.reset_stats();
    g.run_auto(&CancelToken::new()).unwrap();
    g.stats()
}

/// `(kind, count)` of every kind computed in the pass.
fn computed(s: &Stats) -> Vec<(&'static str, u64)> {
    s.computed.iter().map(|(k, n)| (*k, *n)).collect()
}

fn module_in_zone(
    g: &mut Graph,
    p: &Project,
    field: usize,
    zone: RoofZone,
) -> Vec<pv_model::ModuleId> {
    g.get::<ModuleZones>(array(field, p))
        .unwrap()
        .iter()
        .filter(|m| m.zone == zone)
        .map(|m| m.module)
        .collect()
}

/// A module of field 1 whose sideways move changes no rail length: the upper module of a
/// 2-row block column, away from corner zones (the column's position is taken from the other
/// module; the assertions check that the BOM counters come out unchanged).
fn inner_module(g: &mut Graph, p: &Project) -> pv_model::ModuleId {
    let candidates: Vec<_> = [RoofZone::Eave, RoofZone::Ridge, RoofZone::Interior]
        .into_iter()
        .flat_map(|z| module_in_zone(g, p, 0, z))
        .collect();
    candidates
        .into_iter()
        .find(|id| {
            let m = &p.modules[id.0 as usize];
            m.row % 2 == 1
                && p.modules.iter().any(|o| {
                    o.array_id == m.array_id
                        && o.col == m.col
                        && o.row + 1 == m.row
                        && o.state == ModuleState::Active
                })
        })
        .expect("an inner module")
}

#[test]
fn moving_a_module_inside_its_zone_stops_at_the_zone_set() {
    let mut p = ref_a_roof1();
    let mut g = graph_for(&p);
    let id = inner_module(&mut g, &p);
    p.modules[id.0 as usize].u += 0.001;
    sync_changes(&mut g, &p, &ChangeSet::modules(vec![id])).unwrap();
    let s = pass(&mut g);
    // Hardware and module zones of field 1 only. The module stays in its zone: the module
    // zones are unchanged, so the zone set, loads and descent are confirmed without
    // recomputation (standing seam: the descent does not read the hardware). The summary
    // reads the hardware (counts). The BOM counters are recomputed and unchanged (no rail
    // length moved), so the project's bill of materials is not recomputed.
    assert_eq!(
        computed(&s),
        vec![
            ("array.bom_counts", 1),
            ("array.hardware", 1),
            ("array.module_zones", 1),
            ("array.statics", 1),
        ]
    );
}

#[test]
fn moving_a_rail_end_module_recomputes_the_bom() {
    // Along the slope, a module sets the end of its top rails: a 1 mm move lengthens them by
    // 1 mm, and the bill of materials follows.
    let mut p = ref_a_roof1();
    let mut g = graph_for(&p);
    let inner = inner_module(&mut g, &p);
    p.modules[inner.0 as usize].v += 0.001;
    sync_changes(&mut g, &p, &ChangeSet::modules(vec![inner])).unwrap();
    let s = pass(&mut g);
    assert_eq!(s.computed("array.bom_counts"), 1);
    assert_eq!(s.computed("project.bom"), 1);
}

#[test]
fn a_discount_recomputes_the_bom_only() {
    let mut p = ref_a_roof1();
    let mut g = graph_for(&p);
    p.pricing = pv_model::Pricing {
        discounts: vec![pv_model::ManufacturerDiscount {
            manufacturer: "K2 Systems".into(),
            rate: 0.35,
        }],
    };
    let changes = ChangeSet {
        module_ids: vec![],
        pricing: true,
    };
    sync_changes(&mut g, &p, &changes).unwrap();
    let s = pass(&mut g);
    assert_eq!(computed(&s), vec![("project.bom", 1)]);
}

#[test]
fn removing_the_ridge_modules_changes_the_loads_of_that_field_only() {
    let mut p = ref_a_roof1();
    let mut g = graph_for(&p);
    let ridge = module_in_zone(&mut g, &p, 0, RoofZone::Ridge);
    assert!(!ridge.is_empty());
    for id in &ridge {
        p.modules[id.0 as usize].state = ModuleState::Removed;
    }
    sync_changes(&mut g, &p, &ChangeSet::modules(ridge)).unwrap();
    let s = pass(&mut g);
    assert_eq!(
        computed(&s),
        vec![
            ("array.bom_counts", 1),
            ("array.descent", 1),
            ("array.hardware", 1),
            ("array.module_zones", 1),
            ("array.statics", 1),
            ("array.zone_loads", 1),
            ("array.zone_set", 1),
            ("project.bom", 1),
        ]
    );
    let f1 = g.get::<FieldStatics>(array(0, &p)).unwrap();
    assert_eq!(f1.decisions.len(), 4);
    assert!(!f1.zones.contains(&pv_model::WindZone::Ridge));
}

#[test]
fn a_site_change_recomputes_the_loads_not_the_layout() {
    let mut p = ref_a_roof1();
    let mut g = graph_for(&p);
    if let Some(site) = p.buildings[0].site.as_mut() {
        site.altitude_m += 300.0;
    }
    let r = sync_project(&mut g, &p).unwrap();
    assert_eq!(r.inputs_changed, 1);
    let s = pass(&mut g);
    assert_eq!(
        computed(&s),
        vec![
            ("array.descent", 2),
            ("array.statics", 2),
            ("array.zone_loads", 2),
            ("building.site", 1),
        ]
    );
}

#[test]
fn a_seam_spacing_change_recomputes_hardware_and_descent() {
    let mut p = ref_a_roof1();
    let mut g = graph_for(&p);
    if let Some(grid) = p.roof_faces[0].support_grid.as_mut() {
        grid.spacing_m = 0.5;
    }
    sync_project(&mut g, &p).unwrap();
    let s = pass(&mut g);
    // Outline, roof case and zone geometry are recomputed and cut off; zones and loads are
    // only confirmed. The BOM follows the new fixation count.
    assert_eq!(
        computed(&s),
        vec![
            ("array.bom_counts", 2),
            ("array.descent", 2),
            ("array.hardware", 2),
            ("array.statics", 2),
            ("face.outline", 1),
            ("face.roof", 1),
            ("face.zone_geometry", 1),
            ("project.bom", 1),
        ]
    );
    let f1 = g.get::<FieldStatics>(array(0, &p)).unwrap();
    assert!(f1
        .decisions
        .iter()
        .all(|d| (d.fixation_spacing_m - 0.5).abs() < 1e-9));
}

#[test]
fn a_mounting_change_recomputes_that_field() {
    // The BOM counters read the mounting (parts): recomputed, unchanged, cut off.
    let mut p = ref_a_roof1();
    let mut g = graph_for(&p);
    if let Some(m) = p.arrays[1].mounting.as_mut() {
        m.system_mass_per_module_kg += 1.0;
    }
    sync_project(&mut g, &p).unwrap();
    let s = pass(&mut g);
    assert_eq!(
        computed(&s),
        vec![
            ("array.ballast", 1),
            ("array.bom_counts", 1),
            ("array.descent", 1),
            ("array.statics", 1),
            ("array.zone_loads", 1),
        ]
    );
}

#[test]
fn deleting_a_field_forgets_its_nodes() {
    let mut p = ref_a_roof1();
    let mut g = graph_for(&p);
    let gone = p.arrays.pop().unwrap();
    p.modules.retain(|m| m.array_id != gone.id);
    let live = g.node_count();
    let r = sync_project(&mut g, &p).unwrap();
    assert_eq!(r.forgotten, 1);
    assert!(g.node_count() < live);
    let s = pass(&mut g);
    // Only the bill of materials, which lost a field.
    assert_eq!(computed(&s), vec![("project.bom", 1)]);
    assert_eq!(g.watched().len(), 2); // the remaining field, the BOM
    assert_eq!(
        g.status_of::<FieldStatics>(pv_graph::Instance::Array(gone.id)),
        NodeStatus::Missing
    );
}
