//! The K2 Base checks of project A, on whatever graph state is given: wind zones per field
//! (roof 1), fixation forces and rail checks per zone (±3 %), fixation spacing and climber
//! sets per roof (0 deviation).

use pv_graph::{Graph, Instance};
use pv_model::Project;
use pv_pipeline::nodes::descent::Descent;
use pv_pipeline::nodes::zones::ZoneSet;
use pv_pipeline::FieldStatics;
use pv_statics::RoofZone;
use serde::Deserialize;

use super::{array, DESCENT};

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

/// Values compared and deviations from K2 for roof 1 (fields 0 and 1 of `p`).
pub fn roof1_deviations(g: &mut Graph, p: &Project) -> (usize, Vec<String>) {
    roof_deviations(g, "roof-1", &[array(0, p), array(1, p)], true)
}

/// Values compared and deviations from K2 for the fields of one roof (empty: the graph
/// reproduces the report). Every zone a field has is checked against K2's values for that
/// zone; `all_zones`: K2 gives each field all five zones (printed for roof 1).
pub fn roof_deviations(
    g: &mut Graph,
    roof: &str,
    fields: &[Instance],
    all_zones: bool,
) -> (usize, Vec<String>) {
    let fx: DescentFile = serde_json::from_str(DESCENT).unwrap();
    let k2 = fx.descent.iter().find(|c| c.id == "ref-a").unwrap();
    let tol = fx.load_tolerance;
    let mut failures = Vec::new();
    let mut compared = 0;
    let mut close = |what: String, actual: f64, expected: f64, failures: &mut Vec<String>| {
        compared += 1;
        if (actual - expected).abs() > (tol * expected.abs()).max(0.0005) {
            failures.push(format!("{what}: engine {actual:.4}, K2 {expected:.4}"));
        }
    };
    let all = RoofZone::ALL.to_vec();
    for (f, &a) in fields.iter().enumerate() {
        let zones = g.get::<ZoneSet>(a).unwrap();
        if all_zones && *zones != all {
            failures.push(format!("{roof} field {} zones {:?}", f + 1, zones));
        }
        let descent = g.get::<Descent>(a).unwrap();
        let descent = descent.as_ref().as_ref().expect("cross-rail field");
        for d in descent {
            let ex = k2.zones.iter().find(|z| z.zone == d.zone).unwrap();
            let tag = format!("{roof} field {} {:?}", f + 1, d.zone);
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
            for (k, (&v, &e)) in got
                .iter()
                .zip(ex.fixation.uls.iter().chain(&ex.fixation.sls))
                .enumerate()
            {
                close(format!("{tag} F[{k}]"), v, e, &mut failures);
            }
            let r = &d.result;
            for (name, v, exp) in [
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
                    close(format!("{tag} {name}"), v, e, &mut failures);
                }
            }
            // Decision: exact, in millimetres.
            let mm = |x: f64| (x * 1000.0).round() as i64;
            if mm(r.fixation_spacing.value) != mm(ex.fixation_spacing_m) {
                failures.push(format!(
                    "{tag} fixation spacing {} mm, K2 {} mm",
                    mm(r.fixation_spacing.value),
                    mm(ex.fixation_spacing_m)
                ));
            }
        }
    }
    // Climber sets of the roof: exact.
    let climbers: usize = fields
        .iter()
        .map(|&a| {
            g.get::<FieldStatics>(a)
                .unwrap()
                .climbers
                .map_or(0, |n| n as usize)
        })
        .sum();
    let expected = fx
        .climbers
        .roofs
        .iter()
        .find(|r| r.id == roof)
        .unwrap()
        .expected;
    if climbers != expected {
        failures.push(format!("{roof}: climber sets {climbers}, K2 {expected}"));
    }
    (compared, failures)
}
