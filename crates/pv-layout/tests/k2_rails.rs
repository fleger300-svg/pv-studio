//! L4: rails, climbers and fixations against the K2 Base report of project A
//! (`testdata/k2/reference_layout.json`, section `rails`).

use std::collections::BTreeMap;

use pv_geo::Vec2;
use pv_layout::model::{grid_spec, layout_array, structure};
use pv_layout::{
    cross_rails, fixations, CrossRailParams, RailLayout, Search, SupportKind, SupportLines,
};
use pv_model::{
    ArrayGrid, ArrayId, ArrayLayoutParams, BlockParams, BuildingId, ModuleState, Orientation,
    PanelArray, RoofFace, RoofFaceId,
};
use serde::Deserialize;

const FIXTURE: &str = include_str!("../../../testdata/k2/reference_layout.json");

#[derive(Deserialize)]
struct Fixture {
    fields: Vec<FieldIn>,
    rails: RailsIn,
    roof_1_face: FaceIn,
}

#[derive(Deserialize)]
struct FieldIn {
    roof: String,
    cols: u32,
    rows: u32,
    removed: usize,
}

#[derive(Deserialize)]
struct RailsIn {
    per_roof: BTreeMap<String, RoofRails>,
    reduced_spacing_m: f64,
    seam_spacing_m: f64,
}

#[derive(Deserialize)]
struct RoofRails {
    top: Vec<(f64, usize)>,
    bottom_long: Vec<(f64, usize)>,
    bottom_short: Vec<(f64, usize)>,
    climbers: usize,
    clamps: usize,
}

#[derive(Deserialize)]
struct FaceIn {
    fields: Vec<PlacedField>,
}

#[derive(Deserialize)]
struct PlacedField {
    field: u32,
    cols: u32,
    rows: u32,
    lower_left: [f64; 2],
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("reference_layout.json")
}

fn face(width: f64, height: f64) -> RoofFace {
    RoofFace {
        id: RoofFaceId::new(),
        building_id: BuildingId::new(),
        name: "f".into(),
        origin: [0.0, 0.0, 0.0],
        tilt_deg: 19.0,
        azimuth_deg: 180.0,
        width_m: width,
        height_m: height,
        outline: None,
        support_grid: None,
    }
}

fn ref_a_array(f: &RoofFace, boundary: Vec<[f64; 2]>) -> PanelArray {
    PanelArray {
        id: ArrayId::new(),
        roof_face_id: f.id,
        module_length_m: 1.8,
        module_width_m: 1.134,
        orientation: Orientation::Landscape,
        tilt_on_roof_deg: 0.0,
        azimuth_offset_deg: 0.0,
        grid: ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0),
        module: None,
        mounting: None,
        layout: ArrayLayoutParams {
            boundary_uv: boundary,
            module_gap_u_m: 0.01,
            module_gap_v_m: 0.02,
            block: Some(BlockParams {
                max_cols: 2,
                max_rows: 2,
                gap_u_m: 0.08,
                gap_v_m: 0.13,
            }),
            ..ArrayLayoutParams::default()
        },
    }
}

/// Rails of one K2 field, laid out in its outline; K2's removed module (roof-1 field 2,
/// block 6) is the top module of the block's right column.
fn field_rails(field: &FieldIn) -> (RailLayout, Vec<pv_layout::Cell>) {
    let f = face(40.0, 12.0);
    let probe = grid_spec(&ref_a_array(&f, vec![]));
    let (w, l) = probe.extent(field.cols, field.rows);
    let (u0, v0) = (1.0, 1.0);
    let a = ref_a_array(
        &f,
        vec![
            [u0, v0],
            [u0 + w + 0.001, v0],
            [u0 + w + 0.001, v0 + l + 0.001],
            [u0, v0 + l + 0.001],
        ],
    );
    let layout = layout_array(&f, &a, Search::default()).unwrap();
    let mut modules = layout.module_instances(a.id, 0);
    if field.removed == 1 {
        modules
            .iter_mut()
            .find(|m| m.row == 3 && m.col == 3)
            .unwrap()
            .state = ModuleState::Removed;
    }
    let (s, ids) = structure(&a, &modules).unwrap();
    let cells: Vec<pv_layout::Cell> = ids
        .iter()
        .map(|id| {
            let m = modules.iter().find(|m| m.id == *id).unwrap();
            pv_layout::Cell {
                row: m.row,
                col: m.col,
                centre: Vec2::new(m.u, m.v),
            }
        })
        .collect();
    (
        cross_rails(
            &cells,
            &grid_spec(&a),
            &s,
            &CrossRailParams::k2_single_rail(),
        ),
        cells,
    )
}

fn histogram(lengths: impl Iterator<Item = f64>) -> BTreeMap<i64, usize> {
    let mut h = BTreeMap::new();
    for l in lengths {
        *h.entry((l * 1000.0).round() as i64).or_default() += 1;
    }
    h
}

#[test]
fn rails_and_climbers_match_k2_cut_lists() {
    let fx = fixture();
    let mut per_roof: BTreeMap<String, Vec<RailLayout>> = BTreeMap::new();
    for field in &fx.fields {
        let (rails, _) = field_rails(field);
        per_roof.entry(field.roof.clone()).or_default().push(rails);
    }
    for (roof, expected) in &fx.rails.per_roof {
        let layouts = &per_roof[roof];
        // Top rails: exact lengths (to the mm) and counts.
        let top = histogram(
            layouts
                .iter()
                .flat_map(|r| r.top.iter().map(|t| t.length())),
        );
        let want: BTreeMap<i64, usize> = expected
            .top
            .iter()
            .map(|&(l, n)| ((l * 1000.0).round() as i64, n))
            .collect();
        assert_eq!(top, want, "{roof}: top rails (mm → count)");

        // Bottom rails: counts of long (2 columns) and short (1 column) rails exact; base length
        // 3.060 / 1.250 m; K2's lengths are the base or lengthened by less than one seam spacing.
        let bottom: Vec<f64> = layouts
            .iter()
            .flat_map(|r| r.bottom.iter().map(|b| b.length()))
            .collect();
        let (long, short): (Vec<f64>, Vec<f64>) = bottom.iter().partition(|&&l| l > 2.0);
        let count = |v: &[(f64, usize)]| v.iter().map(|x| x.1).sum::<usize>();
        assert_eq!(
            long.len(),
            count(&expected.bottom_long),
            "{roof}: long bottom rails"
        );
        assert_eq!(
            short.len(),
            count(&expected.bottom_short),
            "{roof}: short bottom rails"
        );
        for (ours, k2, base) in [
            (&long, &expected.bottom_long, 3.060),
            (&short, &expected.bottom_short, 1.250),
        ] {
            assert!(
                ours.iter().all(|&l| (l - base).abs() < 0.0005),
                "{roof}: base length {base}"
            );
            for &(l, _) in k2.iter() {
                assert!(
                    l >= base - 0.0005 && l < base + fx.rails.seam_spacing_m,
                    "{roof}: K2 length {l} vs base {base}"
                );
            }
        }

        // Climbers: crossings of top and bottom rails.
        let climbers: usize = layouts.iter().map(|r| r.climbers.len()).sum();
        assert_eq!(climbers, expected.climbers, "{roof}: climber sets");

        // Modules per top rail → module clamps of the K2 material list (reference_bom.json):
        // middle clamps = Σ (modules − 1), end clamps = 2 per top rail.
        assert!(layouts.iter().all(|r| r.top_modules.len() == r.top.len()));
        let middle: u32 = layouts
            .iter()
            .flat_map(|r| r.top_modules.iter().map(|m| m - 1))
            .sum();
        let end = 2 * layouts.iter().map(|r| r.top.len()).sum::<usize>();
        let bom = &bom_tabs()[roof];
        assert_eq!(middle, bom["2004148"], "{roof}: middle clamps");
        assert_eq!(end as u32, bom["2004545"], "{roof}: end clamps");
    }
}

/// Quantities per article of each roof tab of the K2 material list.
fn bom_tabs() -> BTreeMap<String, BTreeMap<String, u32>> {
    #[derive(Deserialize)]
    struct Line {
        article_no: String,
        quantity: u32,
    }
    #[derive(Deserialize)]
    struct Tab {
        id: String,
        lines: Vec<Line>,
    }
    #[derive(Deserialize)]
    struct Bom {
        roofs: Vec<Tab>,
    }
    let bom: Bom =
        serde_json::from_str(include_str!("../../../testdata/k2/reference_bom.json")).unwrap();
    bom.roofs
        .into_iter()
        .map(|t| {
            (
                t.id,
                t.lines
                    .into_iter()
                    .map(|l| (l.article_no, l.quantity))
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn reduced_spacing_of_the_l_block() {
    let fx = fixture();
    let field = fx.fields.iter().find(|f| f.removed == 1).unwrap();
    let (rails, _) = field_rails(field);
    let mut spacings: Vec<i64> = rails
        .supports
        .iter()
        .map(|s| (s.spacing_m * 1000.0).round() as i64)
        .collect();
    spacings.sort_unstable();
    spacings.dedup();
    assert_eq!(
        spacings,
        vec![(fx.rails.reduced_spacing_m * 1000.0).round() as i64, 1000]
    );
    // The L block has 1 long and 3 short bottom rails (K2: « C E E E »).
    let mut per_block: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    for b in &rails.bottom {
        let e = per_block.entry(b.block).or_default();
        if b.length() > 2.0 {
            e.0 += 1
        } else {
            e.1 += 1
        }
    }
    assert!(per_block.values().any(|&x| x == (1, 3)));
}

/// Standing-seam clamps of roof-1, on its reconstructed geometry. The seam position is not
/// printed by K2: the count is computed for every seam phase, and the result is reported
/// against K2's 136 clamps.
#[test]
fn roof_1_clamps_over_seam_phases() {
    let fx = fixture();
    let k2 = fx.rails.per_roof["roof-1"].clamps;
    let f = face(26.18, 6.80);
    let mut results: Vec<(f64, usize, usize)> = Vec::new();
    let mut layouts = Vec::new();
    for pf in &fx.roof_1_face.fields {
        let mut a = ref_a_array(&f, vec![]);
        let spec = grid_spec(&a);
        a.grid = ArrayGrid {
            rows: pf.rows,
            cols: pf.cols,
            origin_u: pf.lower_left[0] + spec.footprint_u / 2.0,
            origin_v: pf.lower_left[1] + spec.footprint_v / 2.0,
            ..ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0)
        };
        let placed = pv_layout::relayout_fixed(&f, &a).unwrap();
        let mut modules = placed.module_instances(a.id, 0);
        if pf.field == 2 {
            modules
                .iter_mut()
                .find(|m| m.row == 3 && m.col == 3)
                .unwrap()
                .state = ModuleState::Removed;
        }
        let (s, ids) = structure(&a, &modules).unwrap();
        let cells: Vec<pv_layout::Cell> = ids
            .iter()
            .map(|id| {
                let m = modules.iter().find(|m| m.id == *id).unwrap();
                pv_layout::Cell {
                    row: m.row,
                    col: m.col,
                    centre: Vec2::new(m.u, m.v),
                }
            })
            .collect();
        layouts.push(cross_rails(
            &cells,
            &spec,
            &s,
            &CrossRailParams::k2_single_rail(),
        ));
    }
    for step in 0..60 {
        let phase = f64::from(step) * 0.01;
        let lines = SupportLines {
            kind: SupportKind::StandingSeam,
            origin_m: phase,
            spacing_m: fx.rails.seam_spacing_m,
        };
        let plain: usize = layouts
            .iter()
            .map(|l| fixations(l, &lines, 0.03, None, 1).count())
            .sum();
        let extended: usize = layouts
            .iter()
            .map(|l| fixations(l, &lines, 0.03, Some(0.485), 1).count())
            .sum();
        let ext_lens: Vec<f64> = layouts
            .iter()
            .flat_map(|l| {
                let fx = fixations(l, &lines, 0.03, Some(0.485), 1);
                l.bottom
                    .iter()
                    .zip(fx.bottom.clone())
                    .map(|(a, b)| b.length() - a.length())
                    .collect::<Vec<_>>()
            })
            .collect();
        let max_ext = ext_lens.iter().copied().fold(0.0_f64, f64::max);
        assert!(
            max_ext <= 0.6 - 0.485 + 0.03 + 1e-9,
            "lengthening {max_ext}"
        );
        results.push((phase, plain, extended));
    }
    println!(
        "phase → seams only / extended: {}",
        results
            .iter()
            .map(|r| format!("{:.2}:{}/{}", r.0, r.1, r.2))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let (min_p, max_p) = (
        results.iter().map(|r| r.1).min().unwrap(),
        results.iter().map(|r| r.1).max().unwrap(),
    );
    let (min_e, max_e) = (
        results.iter().map(|r| r.2).min().unwrap(),
        results.iter().map(|r| r.2).max().unwrap(),
    );
    let hits_p = results.iter().filter(|r| r.1 == k2).count();
    let hits_e = results.iter().filter(|r| r.2 == k2).count();
    println!("roof-1 clamps, K2 {k2}: seams only {min_p}..{max_p} ({hits_p}/60 phases = K2), with cantilever extension {min_e}..{max_e} ({hits_e}/60 phases = K2)");
    // Every bottom rail always gets at least two clamps.
    for l in &layouts {
        let fx0 = fixations(
            l,
            &SupportLines {
                kind: SupportKind::StandingSeam,
                origin_m: 0.0,
                spacing_m: 0.6,
            },
            0.03,
            None,
            1,
        );
        assert!(fx0.positions.iter().all(|p| p.len() >= 2));
    }
    assert!(
        min_p <= k2 + 20 && max_p + 20 >= k2,
        "K2's count must be within the range spanned by the seam phase"
    );
}

/// K2's clamp totals follow from its cut lists under the engine's rules:
/// - a base bottom rail (3.060 / 1.250 m) gets 5 / 2 clamps, or one more when it happens to
///   sit exactly on the seams (6 / — : a 1.25 m rail cannot hold 3 with 0.03 m edges);
/// - a lengthened rail (cantilever rule) ends with exactly one clamp more than a normal base
///   rail.
///
/// Predicted total = Σ base + Σ lengthened; K2 − predicted must then be the number of
/// seam-aligned base long rails, i.e. whole 2-row blocks (3 rails): 0, 3, 3, 0 here.
#[test]
fn k2_clamp_totals_follow_from_cut_lists() {
    let fx = fixture();
    let seams = |origin: f64| SupportLines {
        kind: SupportKind::StandingSeam,
        origin_m: origin,
        spacing_m: 0.6,
    };
    let rail = |len: f64| RailLayout {
        top: vec![],
        top_modules: vec![],
        bottom_modules: vec![],
        bottom: vec![pv_layout::Rail {
            block: 0,
            layer: pv_layout::RailLayer::Bottom,
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(len, 0.0),
        }],
        supports: vec![],
        climbers: vec![],
    };
    let max_cl = Some(0.485);
    // The engine's counts for one rail of each base length, over all seam phases.
    for (base, normal, aligned) in [(3.060, 5, 6), (1.250, 2, 2)] {
        let mut seen = std::collections::BTreeSet::new();
        for step in 0..600 {
            let fx1 = fixations(
                &rail(base),
                &seams(f64::from(step) * 0.001),
                0.03,
                max_cl,
                1,
            );
            let lengthened = fx1.bottom[0].length() > base + 1e-9;
            let n = fx1.count();
            if lengthened {
                // A lengthened rail ends with one clamp more than a normal base rail (a short
                // rail with a single seam under it is lengthened at both ends: 1 → 3), and
                // each end moves by at most 0.6 − 0.485 + 0.03 = 0.145 m.
                assert_eq!(n, normal + 1, "base {base}, phase {step} mm");
                let r = &fx1.bottom[0];
                assert!(-r.start.u <= 0.145 + 1e-9 && r.end.u - base <= 0.145 + 1e-9);
            } else {
                seen.insert(n);
            }
        }
        assert!(
            seen.iter().all(|&n| n == normal || n == aligned),
            "base {base}: {seen:?}"
        );
    }
    let mut aligned_rails = Vec::new();
    for (roof, r) in &fx.rails.per_roof {
        let mut predicted = 0;
        let mut base_long = 0;
        for (list, base, base_clamps) in [(&r.bottom_long, 3.060, 5), (&r.bottom_short, 1.250, 2)] {
            for &(len, n) in list.iter() {
                let lengthened = len > base + 0.0005;
                predicted += n * (base_clamps + usize::from(lengthened));
                if !lengthened && base > 2.0 {
                    base_long += n;
                }
            }
        }
        let extra = r
            .clamps
            .checked_sub(predicted)
            .unwrap_or_else(|| panic!("{roof}: K2 {} < predicted {predicted}", r.clamps));
        assert!(
            extra % 3 == 0 && extra <= base_long,
            "{roof}: K2 {} − predicted {predicted} = {extra}",
            r.clamps
        );
        aligned_rails.push((roof.clone(), predicted, extra));
    }
    println!("clamps predicted from K2 cut lists (+ seam-aligned base rails): {aligned_rails:?}");
    assert_eq!(
        aligned_rails.iter().map(|a| a.2).collect::<Vec<_>>(),
        vec![0, 3, 3, 0]
    );
}
