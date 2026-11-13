//! Load descent, fixation counts and ballast against K2 Base reports (`testdata/k2/`).
//!
//! Acceptance rule (architecture doc, « Validation »): values derived from loads within ±3 %
//! of K2; decisions and quantities (fixation spacing, climbers, concrete blocks) with 0
//! deviation.

use std::fmt::Write as _;

use pv_statics::ballast::{
    aero_pair_ballast, AeroBallastTable, AeroPair, AeroZoneEntry, BallastProduct, PairCoefficients,
    RoofMembrane, RoofSurface,
};
use pv_statics::descent::{
    climber_count, descend, CrossRailSystem, FixationResistance, RailSection, SupportGrid,
    ZoneSurfaceLoads,
};
use pv_statics::{
    analyze_zones, wind, CpEntry, CpSource, DeadLoad, ExplicitCpTable, PartialFactors, RoofInput,
    RoofZone, SiteInput, ZoneLoadCase,
};
use serde::Deserialize;

const LOADS: &str = include_str!("../../../testdata/k2/reference_loads.json");
const FIXTURE: &str = include_str!("../../../testdata/k2/reference_descent_ballast.json");

#[derive(Deserialize)]
struct LoadsFile {
    cases: Vec<LoadCase>,
}

#[derive(Deserialize)]
struct LoadCase {
    id: String,
    site: SiteInput,
    roof: RoofInput,
    dead_load: Option<DeadLoad>,
}

#[derive(Deserialize)]
struct Fixture {
    load_tolerance: f64,
    descent: Vec<DescentCase>,
    climbers: Climbers,
    ballast: Vec<BallastCase>,
}

#[derive(Deserialize)]
struct DescentCase {
    id: String,
    loaded_area_m2: f64,
    dead_load: Option<DeadLoad>,
    #[serde(default)]
    factors_overrides: Option<FactorOverrides>,
    #[serde(default)]
    cp: Vec<CpEntry>,
    system: SystemIn,
    zones: Vec<DescentZone>,
}

#[derive(Deserialize)]
struct FactorOverrides {
    psi0_snow: Option<f64>,
}

#[derive(Deserialize)]
struct SystemIn {
    top_rail_tributary_width_m: f64,
    bottom_rail_spacing_m: f64,
    support_grid: SupportGrid,
    actual_fixation_spacing_m: Option<f64>,
    fixation: Option<FixationResistance>,
}

#[derive(Deserialize)]
struct Forces {
    uls: [f64; 4],
    sls: [f64; 4],
}

#[derive(Deserialize)]
struct DescentZone {
    zone: RoofZone,
    surface: Option<Forces>,
    fixation: Forces,
    top_bending: Option<f64>,
    top_deflection: Option<f64>,
    top_max_span_m: Option<f64>,
    bottom_bending: Option<f64>,
    bottom_deflection: Option<f64>,
    bottom_max_span_m: Option<f64>,
    fixation_utilisation: Option<f64>,
    fixation_spacing_m: f64,
}

#[derive(Deserialize)]
struct Climbers {
    bottom_rail_spacing_m: f64,
    roofs: Vec<ClimberRoof>,
    expected_total: usize,
}

#[derive(Deserialize)]
struct ClimberRoof {
    id: String,
    top_rails: Vec<(f64, usize)>,
    expected: usize,
}

#[derive(Deserialize)]
struct BallastCase {
    id: String,
    system: BallastSystem,
    zones: Vec<BallastZone>,
}

#[derive(Deserialize)]
struct BallastSystem {
    module_area_m2: f64,
    pair_own_mass_kg: f64,
    module_tilt_deg: f64,
    block_mass_kg: f64,
    friction_coefficient: f64,
}

#[derive(Deserialize)]
struct Coeffs {
    cp_east: f64,
    cp_west: f64,
    k: [f64; 6],
}

impl Coeffs {
    fn to_pair(&self) -> PairCoefficients {
        let [k_slope, k_direction, k_parapet, k_module_size, k_row_spacing, k_building_height] =
            self.k;
        PairCoefficients {
            cp_east: self.cp_east,
            cp_west: self.cp_west,
            k_slope,
            k_direction,
            k_parapet,
            k_module_size,
            k_row_spacing,
            k_building_height,
        }
    }
}

#[derive(Deserialize)]
struct BallastZone {
    zone: RoofZone,
    uplift: Coeffs,
    sliding: Coeffs,
    expected: BallastExpected,
}

#[derive(Deserialize)]
struct BallastExpected {
    uplift_fz: f64,
    uplift_ballast_kg: f64,
    sliding_fz: f64,
    sliding_fx: f64,
    sliding_ballast_kg: f64,
    blocks: u32,
}

/// Collects comparisons and fails once with the full table.
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

    fn close(&mut self, what: &str, actual: f64, expected: Option<f64>) {
        let Some(expected) = expected else { return };
        self.count += 1;
        let ok = (actual - expected).abs() <= (self.tolerance * expected.abs()).max(0.0005);
        let rel = if expected == 0.0 {
            0.0
        } else {
            100.0 * (actual - expected) / expected.abs()
        };
        self.failures += usize::from(!ok);
        let _ = writeln!(
            self.report,
            "{} {what:<52} engine {actual:>9.4}  K2 {expected:>9.4}  {rel:>+6.2} %",
            if ok { "  ok " } else { "FAIL " }
        );
    }

    fn exact<T: PartialEq + std::fmt::Display>(&mut self, what: &str, actual: T, expected: T) {
        self.count += 1;
        let ok = actual == expected;
        self.failures += usize::from(!ok);
        let _ = writeln!(
            self.report,
            "{} {what:<52} engine {actual:>9}  K2 {expected:>9}  (exact)",
            if ok { "  ok " } else { "FAIL " }
        );
    }

    fn finish(self, name: &str) {
        println!("{name}: {} values compared\n{}", self.count, self.report);
        assert!(self.count > 0, "{name}: nothing compared");
        assert_eq!(
            self.failures, 0,
            "{name}: {} deviations\n{}",
            self.failures, self.report
        );
    }
}

fn load_cases() -> Vec<LoadCase> {
    serde_json::from_str::<LoadsFile>(LOADS)
        .expect("reference_loads.json")
        .cases
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("reference_descent_ballast.json")
}

#[test]
fn load_descent_matches_k2() {
    let fx = fixture();
    let loads = load_cases();
    let mut c = Checker::new(fx.load_tolerance);
    for case in &fx.descent {
        let lc = loads
            .iter()
            .find(|l| l.id == case.id)
            .expect("site data for descent case");
        let mut factors = PartialFactors::sia_260_for(lc.site.consequence_class);
        if let Some(psi) = case.factors_overrides.as_ref().and_then(|o| o.psi0_snow) {
            factors.psi0_snow = psi;
        }
        let cp_source = if case.cp.is_empty() {
            CpSource::Standard
        } else {
            CpSource::Explicit(ExplicitCpTable {
                reference: "c_pe as printed by K2 Base".into(),
                entries: case.cp.clone(),
            })
        };
        let input = ZoneLoadCase {
            site: lc.site.clone(),
            roof: lc.roof,
            dead_load: case.dead_load.or(lc.dead_load).expect("dead load"),
            factors,
            cp_source,
            zones: case.zones.iter().map(|z| z.zone).collect(),
            loaded_area_m2: case.loaded_area_m2,
        };
        let result = analyze_zones(&input).unwrap();
        let system = CrossRailSystem {
            top_rail: RailSection::k2_single_rail_36(),
            bottom_rail: RailSection::k2_single_rail_36(),
            top_rail_tributary_width_m: case.system.top_rail_tributary_width_m,
            bottom_rail_spacing_m: case.system.bottom_rail_spacing_m,
            support_grid: case.system.support_grid,
            actual_fixation_spacing_m: case.system.actual_fixation_spacing_m,
            fixation: case.system.fixation.clone(),
            single_layer: false,
        };
        for (zl, ex) in result.zones.iter().zip(&case.zones) {
            let tag = format!("{} {:?}", case.id, ex.zone);
            let surface = ZoneSurfaceLoads::from_envelopes(&zl.uls, &zl.sls);
            if let Some(s) = &ex.surface {
                let got = [
                    surface.uls_pressure_perp,
                    surface.uls_pressure_par,
                    surface.uls_uplift_perp,
                    surface.uls_uplift_par,
                    surface.sls_pressure_perp,
                    surface.sls_pressure_par,
                    surface.sls_uplift_perp,
                    surface.sls_uplift_par,
                ];
                let names = [
                    "ULS p⊥", "ULS p∥", "ULS u⊥", "ULS u∥", "SLS p⊥", "SLS p∥", "SLS u⊥", "SLS u∥",
                ];
                for (i, &v) in s.uls.iter().chain(&s.sls).enumerate() {
                    c.close(&format!("{tag} surface {}", names[i]), got[i], Some(v));
                }
            }
            let d = descend(&system, &surface).unwrap();
            let f = &d.fixation_forces;
            let got = [
                f.uls_pressure_perp,
                f.uls_pressure_par,
                f.uls_uplift_perp,
                f.uls_uplift_par,
                f.sls_pressure_perp,
                f.sls_pressure_par,
                f.sls_uplift_perp,
                f.sls_uplift_par,
            ];
            let names = [
                "ULS F⊥ pressure",
                "ULS F∥ pressure",
                "ULS F⊥ uplift",
                "ULS F∥ uplift",
                "SLS F⊥ pressure",
                "SLS F∥ pressure",
                "SLS F⊥ uplift",
                "SLS F∥ uplift",
            ];
            for (i, &v) in ex.fixation.uls.iter().chain(&ex.fixation.sls).enumerate() {
                c.close(&format!("{tag} {}", names[i]), got[i], Some(v));
            }
            c.close(
                &format!("{tag} top rail η_M"),
                d.top_rail.as_ref().unwrap().bending_utilisation.value,
                ex.top_bending,
            );
            c.close(
                &format!("{tag} top rail η_δ"),
                d.top_rail.as_ref().unwrap().deflection_utilisation.value,
                ex.top_deflection,
            );
            c.close(
                &format!("{tag} top rail L_max [m]"),
                d.top_rail.as_ref().unwrap().max_span.value,
                ex.top_max_span_m,
            );
            c.close(
                &format!("{tag} bottom rail η_M"),
                d.bottom_rail.bending_utilisation.value,
                ex.bottom_bending,
            );
            c.close(
                &format!("{tag} bottom rail η_δ"),
                d.bottom_rail.deflection_utilisation.value,
                ex.bottom_deflection,
            );
            c.close(
                &format!("{tag} bottom rail L_max [m]"),
                d.bottom_rail.max_span.value,
                ex.bottom_max_span_m,
            );
            if let Some(fc) = &d.fixation_check {
                c.close(
                    &format!("{tag} fixation η⊥"),
                    fc.perpendicular_utilisation.value,
                    ex.fixation_utilisation,
                );
            }
            // Decision: spacing on the support grid, in mm, exact.
            c.exact(
                &format!("{tag} fixation spacing [mm]"),
                (d.fixation_spacing.value * 1000.0).round() as i64,
                (ex.fixation_spacing_m * 1000.0).round() as i64,
            );
        }
    }
    c.finish("load descent");
}

#[test]
fn climber_counts_match_k2_exactly() {
    let fx = fixture();
    let mut c = Checker::new(fx.load_tolerance);
    let mut total = 0;
    for roof in &fx.climbers.roofs {
        let lengths: Vec<f64> = roof
            .top_rails
            .iter()
            .flat_map(|&(len, n)| std::iter::repeat_n(len, n))
            .collect();
        let n = climber_count(&lengths, fx.climbers.bottom_rail_spacing_m);
        total += n;
        c.exact(&format!("ref-a {} climber sets", roof.id), n, roof.expected);
    }
    c.exact(
        "ref-a total climber sets",
        total,
        fx.climbers.expected_total,
    );
    c.finish("climbers");
}

#[test]
fn ballast_matches_k2() {
    let fx = fixture();
    let loads = load_cases();
    let mut c = Checker::new(fx.load_tolerance);
    for case in &fx.ballast {
        let lc = loads
            .iter()
            .find(|l| l.id == case.id)
            .expect("site data for ballast case");
        let qp = wind::peak_pressure(&lc.site).unwrap().peak_pressure.value;
        let factors = PartialFactors::sia_260_for(lc.site.consequence_class);
        let k_fi = lc.site.consequence_class.importance_factor();
        let s = &case.system;
        let table = AeroBallastTable {
            manufacturer: "K2 Systems".into(),
            product: "D-Dome 6.10".into(),
            document: format!("back-fitted from K2 Base report ({})", case.id),
            module_tilt_deg: s.module_tilt_deg,
            entries: case
                .zones
                .iter()
                .map(|z| AeroZoneEntry {
                    zone: z.zone,
                    uplift: z.uplift.to_pair(),
                    sliding: z.sliding.to_pair(),
                })
                .collect(),
        };
        let pair = AeroPair {
            module_area_m2: s.module_area_m2,
            own_mass_kg: s.pair_own_mass_kg,
        };
        let surface = RoofSurface {
            membrane: RoofMembrane::Gravel,
            friction_coefficient: s.friction_coefficient,
            source: "K2 Base report".into(),
        };
        let block = BallastProduct {
            name: "Concrete block 11 kg".into(),
            unit_mass_kg: s.block_mass_kg,
        };
        for z in &case.zones {
            let tag = format!("{} {:?}", case.id, z.zone);
            let r = aero_pair_ballast(&table, z.zone, qp, &pair, &surface, &factors, k_fi, &block)
                .unwrap();
            let e = &z.expected;
            c.close(
                &format!("{tag} uplift F_z [kN]"),
                r.uplift_forces.uplift.value,
                Some(e.uplift_fz),
            );
            c.close(
                &format!("{tag} sliding F_z [kN]"),
                r.sliding_forces.uplift.value,
                Some(e.sliding_fz),
            );
            c.close(
                &format!("{tag} sliding F_x [kN]"),
                r.sliding_forces.horizontal.value,
                Some(e.sliding_fx),
            );
            // Required total mass (own + ballast) is the load-derived quantity; where K2 needs
            // no ballast the engine must not need any either.
            for (name, res, k2) in [
                ("uplift", &r.uplift, e.uplift_ballast_kg),
                ("sliding", &r.sliding, e.sliding_ballast_kg),
            ] {
                if k2 > 0.0 {
                    c.close(
                        &format!("{tag} {name} m_DL + m_B [kg]"),
                        res.required_mass.value,
                        Some(s.pair_own_mass_kg + k2),
                    );
                } else {
                    c.exact(
                        &format!("{tag} {name} ballast needed"),
                        res.ballast_mass.value > 0.0,
                        false,
                    );
                }
            }
            c.exact(&format!("{tag} concrete blocks"), r.blocks, e.blocks);
        }
    }
    c.finish("ballast");
}

/// Ballast sharing over the mounting blocks laid out by `pv-layout`: linked modules share,
/// an expansion joint splits the group (architecture: « les joints de dilatation coupent ces
/// liaisons »).
#[test]
fn ballast_is_shared_within_layout_blocks() {
    use pv_layout::{build_structure, Cell, GridSpec, JointRules};
    use pv_statics::ballast::share_ballast_on_structure;

    let spec = GridSpec {
        footprint_u: 1.134,
        footprint_v: 1.762,
        gap_u: 0.02,
        gap_v: 0.02,
        block: None,
        row_pitch_v: None,
        anchor: pv_model::LayoutAnchor::LowerLeft,
    };
    let cells: Vec<Cell> = (0..6)
        .map(|c| Cell {
            row: 0,
            col: c,
            centre: pv_geo::Vec2::new(spec.col_offset(c), 0.0),
        })
        .collect();
    // Edge modules need ballast, the middle ones have a surplus.
    let required = [60.0, 20.0, 20.0, 20.0, 20.0, 60.0];
    let own = [25.0; 6];

    // One block: total deficit 2·35 − 4·5 = 50 kg → 5 blocks of 11 kg.
    let one = build_structure(&cells, &spec, &JointRules::default()).unwrap();
    let groups = share_ballast_on_structure(&one, &required, &own, 11.0).unwrap();
    assert_eq!(groups.len(), 1);
    assert!((groups[0].ballast_mass_kg - 50.0).abs() < 1e-9);
    assert_eq!(groups[0].blocks, 5);

    // A joint in the middle: two groups of 35 − 2·5 = 25 kg → 3 blocks each.
    let rules = JointRules {
        max_length_u: Some(3.5),
        ..JointRules::default()
    };
    let split = build_structure(&cells, &spec, &rules).unwrap();
    assert_eq!(split.blocks.len(), 2);
    let groups = share_ballast_on_structure(&split, &required, &own, 11.0).unwrap();
    assert_eq!(groups.len(), 2);
    assert!(groups
        .iter()
        .all(|g| (g.ballast_mass_kg - 25.0).abs() < 1e-9 && g.blocks == 3));
}
