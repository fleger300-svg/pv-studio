//! Project A, roof 1, built as a `Project` and computed through the graph, against the
//! K2 Base report: wind zones per field, fixation forces and rail checks per zone (±3 %),
//! fixation spacing and climber sets (0 deviation).

mod common;

use common::*;
use pv_graph::Instance;
use pv_model::WindZone;
use pv_pipeline::nodes::descent::Descent;
use pv_pipeline::nodes::zones::ZoneSet;
use pv_pipeline::FieldStatics;
use pv_statics::RoofZone;
use serde::Deserialize;

#[derive(Deserialize)]
struct DescentFile {
    load_tolerance: f64,
    descent: Vec<DescentCase>,
    climbers: Climbers,
}

#[derive(Deserialize)]
struct DescentCase {
    id: String,
    zones: Vec<ZoneIn>,
}

#[derive(Deserialize)]
struct Forces {
    uls: [f64; 4],
    sls: [f64; 4],
}

#[derive(Deserialize)]
struct ZoneIn {
    zone: RoofZone,
    fixation: Forces,
    top_bending: Option<f64>,
    top_max_span_m: Option<f64>,
    bottom_bending: Option<f64>,
    fixation_spacing_m: f64,
}

#[derive(Deserialize)]
struct Climbers {
    roofs: Vec<ClimberRoof>,
}

#[derive(Deserialize)]
struct ClimberRoof {
    id: String,
    expected: usize,
}

fn close(what: &str, actual: f64, expected: f64, tol: f64, failures: &mut Vec<String>) {
    if (actual - expected).abs() > (tol * expected.abs()).max(0.0005) {
        failures.push(format!("{what}: engine {actual:.4}, K2 {expected:.4}"));
    }
}

#[test]
fn roof_1_through_the_graph_matches_k2() {
    let p = ref_a_roof1();
    let mut g = graph_for(&p);
    let fx: DescentFile = serde_json::from_str(DESCENT).unwrap();
    let k2 = fx.descent.iter().find(|c| c.id == "ref-a").unwrap();
    let all = [
        RoofZone::Interior,
        RoofZone::Ridge,
        RoofZone::Verge,
        RoofZone::Eave,
        RoofZone::EaveCorner,
    ];
    let mut failures = Vec::new();
    let mut compared = 0;
    for f in 0..2 {
        let a = array(f, &p);
        // Wind zones: K2 gives both fields of roof 1 all five zones.
        assert_eq!(*g.get::<ZoneSet>(a).unwrap(), all, "field {}", f + 1);

        let descent = g.get::<Descent>(a).unwrap();
        let descent = descent.as_ref().as_ref().expect("cross-rail field");
        for d in descent {
            let ex = k2.zones.iter().find(|z| z.zone == d.zone).unwrap();
            let tag = format!("field {} {:?}", f + 1, d.zone);
            let ff = &d.result.fixation_forces;
            let got = [
                ff.uls_pressure_perp,
                ff.uls_pressure_par,
                ff.uls_uplift_perp,
                ff.uls_uplift_par,
                ff.sls_pressure_perp,
                ff.sls_pressure_par,
                ff.sls_uplift_perp,
                ff.sls_uplift_par,
            ];
            for (k, (&g, &e)) in got
                .iter()
                .zip(ex.fixation.uls.iter().chain(&ex.fixation.sls))
                .enumerate()
            {
                close(
                    &format!("{tag} F[{k}]"),
                    g,
                    e,
                    fx.load_tolerance,
                    &mut failures,
                );
                compared += 1;
            }
            let r = &d.result;
            for (name, got, exp) in [
                (
                    "top η_M",
                    r.top_rail.bending_utilisation.value,
                    ex.top_bending,
                ),
                ("top L_max", r.top_rail.max_span.value, ex.top_max_span_m),
                (
                    "bottom η_M",
                    r.bottom_rail.bending_utilisation.value,
                    ex.bottom_bending,
                ),
            ] {
                if let Some(e) = exp {
                    close(
                        &format!("{tag} {name}"),
                        got,
                        e,
                        fx.load_tolerance,
                        &mut failures,
                    );
                    compared += 1;
                }
            }
            // Decision: exact, in millimetres.
            assert_eq!(
                (r.fixation_spacing.value * 1000.0).round() as i64,
                (ex.fixation_spacing_m * 1000.0).round() as i64,
                "{tag} fixation spacing"
            );
        }
    }
    assert!(compared > 80, "{compared} values compared");
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    // Climber sets of the roof: exact.
    let climbers: usize = (0..2)
        .map(|f| {
            g.get::<FieldStatics>(array(f, &p))
                .unwrap()
                .climbers
                .map(|n| n as usize)
                .unwrap()
        })
        .sum();
    let expected = fx
        .climbers
        .roofs
        .iter()
        .find(|r| r.id == "roof-1")
        .unwrap()
        .expected;
    assert_eq!(climbers, expected);
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
    assert_eq!(g.watched().len(), 2);
    let _ = Instance::Project;
}
