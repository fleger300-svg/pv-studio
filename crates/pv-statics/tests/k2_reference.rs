//! Validation of the statics engine against K2 Base reports (`testdata/k2/`).
//!
//! Acceptance rule (architecture doc, « Validation »): loads within ±3 % of K2. Every value
//! compared is listed in the failure message so a regression is easy to locate.

use std::fmt::Write as _;

use pv_statics::{
    analyze_zones, snow, wind, CpSource, DeadLoad, PartialFactors, RoofInput, RoofZone, SiteInput,
    ZoneLoadCase,
};
use serde::Deserialize;

const FIXTURE: &str = include_str!("../../../testdata/k2/reference_loads.json");

#[derive(Deserialize)]
struct Fixture {
    load_tolerance: f64,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    site: SiteInput,
    roof: RoofInput,
    dead_load: Option<DeadLoad>,
    expected: Expected,
    #[serde(default)]
    zone_tables: Vec<ZoneTable>,
    #[serde(default)]
    roofs: Vec<Roof>,
}

#[derive(Deserialize, Default)]
struct Expected {
    s_k: Option<f64>,
    mu: Option<f64>,
    cos_alpha: Option<f64>,
    s_50: Option<f64>,
    f_s: Option<f64>,
    s_n: Option<f64>,
    q_p50: Option<f64>,
    f_w: Option<f64>,
    q_pn: Option<f64>,
    module_dead_kg_m2: Option<f64>,
    system_dead_kg_m2: Option<f64>,
}

#[derive(Deserialize)]
struct ZoneTable {
    area_m2: f64,
    zones: Vec<ZoneExpected>,
}

#[derive(Deserialize)]
struct ZoneExpected {
    zone: RoofZone,
    cp_pressure: Option<f64>,
    cp_suction: Option<f64>,
    w_pressure: Option<f64>,
    w_suction: Option<f64>,
    uls: Envelope,
    sls: Envelope,
}

#[derive(Deserialize)]
struct Envelope {
    pressure_perp: f64,
    pressure_par: f64,
    uplift_perp: f64,
    uplift_par: f64,
}

#[derive(Deserialize)]
struct Roof {
    id: String,
    fields: Vec<Field>,
}

#[derive(Deserialize)]
struct Field {
    field: u32,
    zones: Vec<RoofZone>,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("testdata/k2/reference_loads.json is valid")
}

/// Collects comparisons and fails once at the end with the full table.
struct Checker {
    tolerance: f64,
    report: String,
    failures: usize,
    count: usize,
}

impl Checker {
    fn new(tolerance: f64) -> Self {
        Self {
            tolerance,
            report: String::new(),
            failures: 0,
            count: 0,
        }
    }

    fn check(&mut self, what: &str, actual: f64, expected: Option<f64>) {
        let Some(expected) = expected else { return };
        self.count += 1;
        // Relative deviation; values printed by K2 with 3 decimals near zero would otherwise
        // be dominated by rounding, so an absolute floor of half a printed digit applies.
        let deviation = (actual - expected).abs();
        let allowed = (self.tolerance * expected.abs()).max(0.0005);
        let ok = deviation <= allowed;
        let rel = if expected == 0.0 {
            0.0
        } else {
            100.0 * (actual - expected) / expected.abs()
        };
        if !ok {
            self.failures += 1;
        }
        let _ = writeln!(
            self.report,
            "{} {what:<55} engine {actual:>9.4}  K2 {expected:>9.4}  {rel:>+6.2} %",
            if ok { "  ok " } else { "FAIL " }
        );
    }

    fn finish(self, name: &str) {
        println!("{name}: {} values compared\n{}", self.count, self.report);
        assert!(self.count > 0, "{name}: nothing compared");
        assert_eq!(
            self.failures,
            0,
            "{name}: {} values outside ±{:.0} %\n{}",
            self.failures,
            self.tolerance * 100.0,
            self.report
        );
    }
}

#[test]
fn snow_and_wind_match_k2_for_all_reference_cases() {
    let fx = fixture();
    let mut c = Checker::new(fx.load_tolerance);
    for case in &fx.cases {
        let id = &case.id;
        let s = snow::roof_snow(&case.site, case.roof.pitch_deg).unwrap();
        let w = wind::peak_pressure(&case.site).unwrap();
        let e = &case.expected;
        c.check(&format!("{id} s_k"), s.ground_load.value, e.s_k);
        c.check(&format!("{id} μ₁"), s.shape_coefficient.value, e.mu);
        c.check(&format!("{id} cos α"), s.slope_factor.value, e.cos_alpha);
        c.check(&format!("{id} s_50"), s.roof_load_50.value, e.s_50);
        c.check(&format!("{id} f_s"), s.service_life_factor.value, e.f_s);
        c.check(&format!("{id} s_n"), s.roof_load.value, e.s_n);
        c.check(&format!("{id} q_p,50"), w.peak_pressure_50.value, e.q_p50);
        c.check(&format!("{id} f_w"), w.service_life_factor.value, e.f_w);
        c.check(&format!("{id} q_p,n"), w.peak_pressure.value, e.q_pn);
    }
    c.finish("snow & wind");
}

#[test]
fn zone_loads_match_k2_reference_a() {
    let fx = fixture();
    let case = fx
        .cases
        .iter()
        .find(|c| c.id == "ref-a")
        .expect("ref-a present");
    let dead = case.dead_load.expect("ref-a has dead load");
    let mut c = Checker::new(fx.load_tolerance);

    for table in &case.zone_tables {
        let input = ZoneLoadCase {
            site: case.site.clone(),
            roof: case.roof,
            dead_load: dead,
            factors: PartialFactors::sia_260(),
            cp_source: CpSource::Standard,
            zones: table.zones.iter().map(|z| z.zone).collect(),
            loaded_area_m2: table.area_m2,
        };
        let result = analyze_zones(&input).unwrap();
        if table.area_m2 == 10.0 {
            c.check(
                "ref-a m_M [kg/m²]",
                result.dead_load.module_mass_per_m2.value,
                case.expected.module_dead_kg_m2,
            );
            c.check(
                "ref-a m_S [kg/m²]",
                result.dead_load.system_mass_per_m2.value,
                case.expected.system_dead_kg_m2,
            );
        }
        for (zl, ex) in result.zones.iter().zip(&table.zones) {
            let tag = format!("A={:>5.2} {:?}", table.area_m2, ex.zone);
            c.check(
                &format!("{tag} c_pe,+"),
                zl.cp.pressure.value,
                ex.cp_pressure,
            );
            c.check(&format!("{tag} c_pe,−"), zl.cp.suction.value, ex.cp_suction);
            c.check(
                &format!("{tag} w_k,+"),
                zl.wind_pressure.value,
                ex.w_pressure,
            );
            c.check(&format!("{tag} w_k,−"), zl.wind_suction.value, ex.w_suction);
            for (state, env, exp) in [("ULS", &zl.uls, &ex.uls), ("SLS", &zl.sls, &ex.sls)] {
                c.check(
                    &format!("{tag} {state} pressure ⊥"),
                    env.pressure_perp.value,
                    Some(exp.pressure_perp),
                );
                c.check(
                    &format!("{tag} {state} pressure ∥"),
                    env.pressure_par.value,
                    Some(exp.pressure_par),
                );
                c.check(
                    &format!("{tag} {state} uplift ⊥"),
                    env.uplift_perp.value,
                    Some(exp.uplift_perp),
                );
                c.check(
                    &format!("{tag} {state} uplift ∥"),
                    env.uplift_par.value,
                    Some(exp.uplift_par),
                );
            }
        }
    }
    c.finish("ref-a zone loads");
}

/// Each of the 4 roofs × 2 fields computes the zones it touches, with the same values as the
/// zone table (same site, pitch and system on every roof).
#[test]
fn every_field_of_reference_a_is_computed() {
    let fx = fixture();
    let case = fx.cases.iter().find(|c| c.id == "ref-a").unwrap();
    let table = case.zone_tables.iter().find(|t| t.area_m2 == 10.0).unwrap();
    let mut c = Checker::new(fx.load_tolerance);
    for roof in &case.roofs {
        for field in &roof.fields {
            let input = ZoneLoadCase {
                site: case.site.clone(),
                roof: case.roof,
                dead_load: case.dead_load.unwrap(),
                factors: PartialFactors::sia_260(),
                cp_source: CpSource::Standard,
                zones: field.zones.clone(),
                loaded_area_m2: 10.0,
            };
            let result = analyze_zones(&input).unwrap();
            assert_eq!(result.zones.len(), field.zones.len());
            for zl in &result.zones {
                let ex = table.zones.iter().find(|z| z.zone == zl.zone).unwrap();
                let tag = format!("{} field {} {:?}", roof.id, field.field, zl.zone);
                c.check(
                    &format!("{tag} ULS uplift ⊥"),
                    zl.uls.uplift_perp.value,
                    Some(ex.uls.uplift_perp),
                );
                c.check(
                    &format!("{tag} ULS pressure ⊥"),
                    zl.uls.pressure_perp.value,
                    Some(ex.uls.pressure_perp),
                );
            }
        }
    }
    c.finish("ref-a fields");
}

/// ADR-004: results keep their formulas, references, inputs and assumptions.
#[test]
fn results_are_traceable() {
    let fx = fixture();
    let case = fx.cases.iter().find(|c| c.id == "ref-a").unwrap();
    let input = ZoneLoadCase {
        site: case.site.clone(),
        roof: case.roof,
        dead_load: case.dead_load.unwrap(),
        factors: PartialFactors::sia_260(),
        cp_source: CpSource::Standard,
        zones: vec![RoofZone::EaveCorner],
        loaded_area_m2: 10.0,
    };
    let r = analyze_zones(&input).unwrap();
    assert_eq!(r.disclaimer_key, pv_statics::DISCLAIMER_KEY);
    for t in [
        &r.snow.ground_load,
        &r.snow.roof_load_50,
        &r.wind.height_coefficient,
        &r.wind.peak_pressure_50,
    ] {
        assert!(
            !t.formula.is_empty() && t.reference.is_some() && !t.inputs.is_empty(),
            "{}",
            t.display()
        );
    }
    let z = &r.zones[0];
    assert!(z.uls.uplift_perp.formula.contains("Cc06"));
    assert!(z
        .cp
        .suction
        .assumptions
        .iter()
        .any(|a| a.key == "statics.assumption.cp.envelope"));
    assert!(z.cp.suction.inputs.iter().any(|i| i.symbol == "c_pe,−(L)"));

    // The fingerprint follows the inputs.
    let mut changed = input.clone();
    changed.site.building_height_m = 24.0;
    assert_ne!(
        r.input_fingerprint,
        analyze_zones(&changed).unwrap().input_fingerprint
    );
    assert_eq!(
        r.input_fingerprint,
        analyze_zones(&input).unwrap().input_fingerprint
    );
}
