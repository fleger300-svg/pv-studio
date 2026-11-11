//! E4: project A (ref-a) rebuilt from an empty project file through the editing commands
//! only — roof generation, roof covering, one array per field laid out by the engine, one
//! module removed — then computed by the graph and compared with the K2 Base report roof by
//! roof: modules, rail cut lists, climbers, clamps, the bill of materials and the statics
//! of every zone. Undoing the whole rebuild returns to the empty project; redoing it and
//! reopening the file give the same results.
//!
//! Standing-seam clamps are the documented gap (K2 does not print the seam phase): they are
//! checked against the fixations they come from and their difference to K2 is printed.

mod common;

use std::collections::BTreeMap;

use common::*;
use pv_graph::{CancelToken, Graph, Instance};
use pv_layout::model::grid_spec;
use pv_layout::{default_array, plan_layout, Search};
use pv_model::component::presets;
use pv_model::{
    BlockParams, Building, BuildingId, ChangeSet, Command, EdgeRoof, ModuleState, Orientation,
    Project, RoofFace, RoofKind, RoofSpec, SupportGridKind, SupportGridSpec,
};
use pv_pipeline::nodes::bom::{FieldCountsNode, ProjectBom};
use pv_pipeline::{sync_changes, sync_project, FieldStatics};
use pv_store::{Author, ProjectStore};
use serde::Deserialize;

const SEAM_CLAMP: &str = "2003024";
const PITCH_DEG: f64 = 19.0;
const LENGTH_M: f64 = 26.18;
const DEPTH_M: f64 = 12.86;
const HIP_RUN_M: f64 = 6.56;
const RIDGE_M: f64 = 25.0;

#[derive(Deserialize)]
struct LayoutFile {
    rebuild: Rebuild,
    rails: Rails,
    expected_modules: BTreeMap<String, usize>,
}

#[derive(Deserialize)]
struct Rebuild {
    buildings: Vec<BuildingIn>,
    roofs: Vec<RoofIn>,
}

#[derive(Deserialize)]
struct BuildingIn {
    name: String,
    hip_end: String,
}

#[derive(Deserialize)]
struct RoofIn {
    roof: String,
    building: String,
    azimuth_deg: f64,
    hip: String,
    fields: Vec<FieldIn>,
}

#[derive(Deserialize)]
struct FieldIn {
    cols: u32,
    rows: u32,
    lower_left: [f64; 2],
    #[serde(default)]
    removed: Vec<[u32; 2]>,
}

#[derive(Deserialize)]
struct Rails {
    per_roof: BTreeMap<String, RoofRails>,
    seam_spacing_m: f64,
}

#[derive(Deserialize)]
struct RoofRails {
    top: Vec<(f64, usize)>,
    bottom_long: Vec<(f64, usize)>,
    bottom_short: Vec<(f64, usize)>,
    climbers: u32,
    clamps: u32,
}

#[derive(Deserialize)]
struct BomFile {
    roofs: Vec<Tab>,
    total: TabLines,
}

#[derive(Deserialize)]
struct Tab {
    id: String,
    lines: Vec<Line>,
}

#[derive(Deserialize)]
struct TabLines {
    lines: Vec<Line>,
}

#[derive(Deserialize)]
struct Line {
    article_no: String,
    quantity: u32,
}

fn layout_file() -> LayoutFile {
    serde_json::from_str(LAYOUT).unwrap()
}

fn bom_file() -> BomFile {
    serde_json::from_str(include_str!("../../../testdata/k2/reference_bom.json")).unwrap()
}

/// A building of project A as the user enters it: footprint, site, eave height and the roof
/// of each footprint edge (south, east, north, west).
fn building(name: &str, hip_end: &str) -> Building {
    let (site, height) = site_of("ref-a");
    assert!((height - RIDGE_M).abs() < 1e-9);
    let half = DEPTH_M / 2.0;
    let rise = half * PITCH_DEG.to_radians().tan();
    // The hipped end rises as high over the hip run: the ridge is 26.18 − 6.56 = 19.62 m.
    let hip = EdgeRoof::sloped((rise / HIP_RUN_M).atan().to_degrees());
    let side = EdgeRoof::sloped(PITCH_DEG);
    let (east, west) = match hip_end {
        "west" => (EdgeRoof::gable(), hip),
        "east" => (hip, EdgeRoof::gable()),
        other => panic!("hip end {other}"),
    };
    let footprint = pv_model::rectangle_footprint([LENGTH_M, DEPTH_M]);
    Building {
        id: BuildingId::new(),
        name: name.into(),
        origin: [0.0, if name == "A" { 0.0 } else { 30.0 }, 0.0],
        footprint,
        size_m: [LENGTH_M, DEPTH_M],
        eave_height_m: RIDGE_M - rise,
        ridge_height_m: RIDGE_M,
        roof_kind: RoofKind::Hipped,
        roof: Some(RoofSpec {
            edges: vec![side, east, side, west],
            overhang_m: 0.0,
        }),
        site: Some(site),
    }
}

/// A field as the user sets it up: K2 SingleRail with landscape 1800 × 1134 modules in
/// 2 × 2 blocks, its outline drawn on the face, no setback (the outline is the field).
fn field(face: &RoofFace, f: &FieldIn) -> pv_model::PanelArray {
    let mut a = default_array(face);
    a.module_length_m = 1.8;
    a.module_width_m = 1.134;
    a.orientation = Orientation::Landscape;
    a.layout.module_gap_u_m = 0.01;
    a.layout.module_gap_v_m = 0.02;
    a.layout.block = Some(BlockParams {
        max_cols: 2,
        max_rows: 2,
        gap_u_m: 0.08,
        gap_v_m: 0.13,
    });
    a.layout.edge_setback_m = 0.0;
    a.layout.rails = Some(k2_rails());
    a.module = Some(presets::LR7_54HVH_490M);
    a.mounting = Some(presets::K2_SINGLE_RAIL);
    let (w, l) = grid_spec(&a).extent(f.cols, f.rows);
    // K2 prints sizes to the centimetre: half a unit of slack, as when drawing the field.
    let ([u0, v0], w, l) = (f.lower_left, w + 0.005, l + 0.005);
    a.layout.boundary_uv = vec![[u0, v0], [u0 + w, v0], [u0 + w, v0 + l], [u0, v0 + l]];
    a
}

/// The face of `building` facing `azimuth`.
fn face_of(p: &Project, building: BuildingId, azimuth: f64) -> RoofFace {
    p.roof_faces
        .iter()
        .find(|f| {
            f.building_id == building
                && (f.tilt_deg - PITCH_DEG).abs() < 1e-6
                && (f.azimuth_deg - azimuth).abs() < 1e-6
        })
        .unwrap_or_else(|| panic!("no face at {azimuth}°"))
        .clone()
}

struct Rebuilt {
    store: ProjectStore,
    graph: Graph,
    /// Commands applied (each one an undo step).
    steps: usize,
    /// Roof id → face id and the array ids of its fields, in K2's field order.
    roofs: Vec<(String, RoofFace, Vec<pv_model::ArrayId>)>,
    _dir: tempfile::TempDir,
}

fn apply(r: &mut Rebuilt, cmd: &Command) -> ChangeSet {
    let c = r.store.apply(cmd, Author::User).unwrap();
    sync_changes(&mut r.graph, r.store.project(), &c).unwrap();
    r.graph.run_auto(&CancelToken::new()).unwrap();
    r.steps += 1;
    c
}

/// Project A from an empty file, command by command; the graph follows incrementally.
fn rebuild() -> Rebuilt {
    let fx = layout_file();
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(
        &dir.path().join("atrium.pvproj"),
        Project::empty("Atrium (ref-a)"),
    )
    .unwrap();
    let mut graph = pv_pipeline::new_graph();
    sync_project(&mut graph, store.project()).unwrap();
    let mut r = Rebuilt {
        store,
        graph,
        steps: 0,
        roofs: Vec::new(),
        _dir: dir,
    };

    // 0. The module and the system, frozen from the component library (base catalogue).
    let library = pv_library::LibraryStore::open_in_memory().unwrap();
    let date = pv_model::component::IsoDate::parse("2026-11-11").unwrap();
    let snapshots = library
        .closure(&[presets::LR7_54HVH_490M, presets::K2_SINGLE_RAIL], &date)
        .unwrap();
    apply(&mut r, &Command::AdoptComponents { snapshots });

    // 1. Buildings: the roof generator gives each one its faces (one batch per building).
    let mut ids = BTreeMap::new();
    for b in &fx.rebuild.buildings {
        let new = building(&b.name, &b.hip_end);
        ids.insert(b.name.clone(), new.id);
        let cmd = pv_roof::plan::plan_roof(r.store.project(), &new).unwrap();
        apply(&mut r, &cmd);
    }

    for roof in &fx.rebuild.roofs {
        // 2. The PV face: renamed, standing seams every 0.60 m from its eave-left corner.
        let mut face = face_of(r.store.project(), ids[&roof.building], roof.azimuth_deg);
        face.name.clone_from(&roof.roof);
        face.support_grid = Some(SupportGridSpec {
            kind: SupportGridKind::StandingSeam,
            origin_m: 0.0,
            spacing_m: 0.6,
        });
        apply(
            &mut r,
            &Command::UpsertRoofFace {
                face: Box::new(face.clone()),
                at: None,
            },
        );
        // 3. One array per field, laid out by the engine inside the drawn outline.
        let mut arrays = Vec::new();
        for f in &roof.fields {
            let a = field(&face, f);
            let cmd = plan_layout(r.store.project(), &a, Search::default()).unwrap();
            apply(&mut r, &cmd);
            arrays.push(a.id);
        }
        r.roofs.push((roof.roof.clone(), face, arrays));
    }

    // 4. The modules K2 leaves out, removed by hand (after every layout: a relayout would
    //    put them back).
    for (roof, (_, _, arrays)) in fx.rebuild.roofs.iter().zip(r.roofs.clone()) {
        for (f, id) in roof.fields.iter().zip(arrays) {
            let ids: Vec<_> = r
                .store
                .project()
                .modules
                .iter()
                .filter(|m| m.array_id == id && f.removed.contains(&[m.row, m.col]))
                .map(|m| m.id)
                .collect();
            assert_eq!(ids.len(), f.removed.len(), "{}: removed modules", roof.roof);
            if !ids.is_empty() {
                apply(&mut r, &Command::RemoveModules { ids });
            }
        }
    }
    r
}

/// Deviations of the rebuilt project from K2 (empty: the report is reproduced); the values
/// compared are counted.
fn deviations(
    g: &mut Graph,
    p: &Project,
    roofs: &[(String, RoofFace, Vec<pv_model::ArrayId>)],
) -> (usize, Vec<String>) {
    let fx = layout_file();
    let bom_ref = bom_file();
    let mut failures = Vec::new();
    let mut compared = 0;
    let bom = (*g.get::<ProjectBom>(Instance::Project).unwrap()).clone();
    let mm = |x: f64| (x * 1000.0).round() as i64;
    for (roof, face, arrays) in roofs {
        let fields: Vec<Instance> = arrays.iter().map(|&a| Instance::Array(a)).collect();

        // Modules.
        let modules = p
            .modules
            .iter()
            .filter(|m| arrays.contains(&m.array_id) && m.state == ModuleState::Active)
            .count();
        compared += 1;
        if Some(&modules) != fx.expected_modules.get(roof) {
            failures.push(format!(
                "{roof}: {modules} modules, K2 {:?}",
                fx.expected_modules.get(roof)
            ));
        }

        // Statics of every zone, climber sets (roof 1: every field has all five zones).
        let (n, f) = k2::roof_deviations(g, roof, &fields, roof == "roof-1");
        compared += n;
        failures.extend(f);

        // Rail cut lists: top rails exact; bottom rails as many long and short ones, each
        // the base length or lengthened by less than one seam spacing (seam phase unknown).
        let expected = &fx.rails.per_roof[roof];
        let mut top: BTreeMap<i64, usize> = BTreeMap::new();
        let mut bottom: Vec<f64> = Vec::new();
        let mut climbers = 0;
        let mut fixations = 0;
        for &i in &fields {
            let c = g.get::<FieldCountsNode>(i).unwrap();
            for t in &c.top_rails {
                *top.entry(mm(t.length_m)).or_default() += 1;
            }
            bottom.extend(&c.bottom_rails);
            climbers += c.climbers;
            fixations += g.get::<FieldStatics>(i).unwrap().fixations.unwrap_or(0);
        }
        let k2_top: BTreeMap<i64, usize> = expected.top.iter().map(|&(l, n)| (mm(l), n)).collect();
        compared += 1;
        if top != k2_top {
            failures.push(format!("{roof}: top rails {top:?}, K2 {k2_top:?}"));
        }
        let (long, short): (Vec<f64>, Vec<f64>) = bottom.iter().partition(|&&l| l > 2.0);
        let count = |v: &[(f64, usize)]| v.iter().map(|x| x.1).sum::<usize>();
        for (what, ours, k2, base) in [
            ("long", &long, &expected.bottom_long, 3.060),
            ("short", &short, &expected.bottom_short, 1.250),
        ] {
            compared += 1;
            if ours.len() != count(k2) {
                failures.push(format!(
                    "{roof}: {} {what} bottom rails, K2 {}",
                    ours.len(),
                    count(k2)
                ));
            }
            for &l in ours {
                compared += 1;
                if !(l >= base - 0.0005 && l < base + fx.rails.seam_spacing_m) {
                    failures.push(format!("{roof}: {what} bottom rail {l:.3} m"));
                }
            }
        }
        compared += 1;
        if climbers != expected.climbers {
            failures.push(format!(
                "{roof}: {climbers} climbers, K2 {}",
                expected.climbers
            ));
        }

        // Bill of materials, tab of the face: every line but the seam clamps exact.
        let tab = bom
            .faces
            .iter()
            .find(|t| t.face_id.as_deref() == Some(face.id.0.to_string().as_str()))
            .unwrap_or_else(|| panic!("{roof}: no BOM tab"));
        let k2_tab = bom_ref.roofs.iter().find(|t| &t.id == roof).unwrap();
        for l in &k2_tab.lines {
            let got = tab
                .lines
                .iter()
                .find(|x| x.article_no == l.article_no)
                .map_or(0, |x| x.quantity);
            if l.article_no == SEAM_CLAMP {
                compared += 1;
                if got != fixations {
                    failures.push(format!("{roof}: seam clamps {got} ≠ fixations {fixations}"));
                }
                eprintln!(
                    "{roof}: standing-seam clamps engine {got}, K2 {} (rails per_roof {}; documented gap)",
                    l.quantity, expected.clamps
                );
            } else {
                compared += 1;
                if got != l.quantity {
                    failures.push(format!(
                        "{roof} BOM {}: engine {got}, K2 {}",
                        l.article_no, l.quantity
                    ));
                }
            }
        }
    }
    // Project total: every line but the seam clamps exact (bars cut per layer, project-wide).
    for l in &bom_ref.total.lines {
        if l.article_no == SEAM_CLAMP {
            continue;
        }
        let got = bom
            .total
            .lines
            .iter()
            .find(|x| x.article_no == l.article_no)
            .map_or(0, |x| x.quantity);
        compared += 1;
        if got != l.quantity {
            failures.push(format!(
                "total BOM {}: engine {got}, K2 {}",
                l.article_no, l.quantity
            ));
        }
    }
    (compared, failures)
}

#[test]
fn atrium_rebuilt_from_scratch_matches_k2() {
    let mut r = rebuild();
    let p = r.store.project().clone();
    assert_eq!(p.buildings.len(), 2);
    assert_eq!(p.arrays.len(), 8);

    // The incremental graph agrees with a graph built from scratch.
    compare_with_full(&mut r.graph, &p).unwrap();

    // Each field sits where it was drawn: K2's lower-left corner, to the slack of the outline.
    let fx = layout_file();
    for (roof, (_, _, arrays)) in fx.rebuild.roofs.iter().zip(&r.roofs) {
        for (f, id) in roof.fields.iter().zip(arrays) {
            let a = p.arrays.iter().find(|a| a.id == *id).unwrap();
            let s = grid_spec(a);
            let corner = [
                a.grid.origin_u - s.footprint_u / 2.0,
                a.grid.origin_v - s.footprint_v / 2.0,
            ];
            assert_eq!(
                (a.grid.cols, a.grid.rows),
                (f.cols, f.rows),
                "{}",
                roof.roof
            );
            for k in 0..2 {
                assert!(
                    (corner[k] - f.lower_left[k]).abs() <= 0.005 + 1e-9,
                    "{}: field corner {corner:?}, drawn at {:?}",
                    roof.roof,
                    f.lower_left
                );
            }
        }
    }

    let (compared, failures) = deviations(&mut r.graph, &p, &r.roofs);
    eprintln!("{} commands, {compared} values compared with K2", r.steps);
    assert!(compared > 300, "{compared} values compared");
    assert!(
        failures.is_empty(),
        "{compared} values compared:\n{}",
        failures.join("\n")
    );
}

#[test]
fn generated_faces_are_k2_trapezoids() {
    let fx = layout_file();
    let r = rebuild();
    let (l, run) = (LENGTH_M, HIP_RUN_M);
    for (roof, (id, face, _)) in fx.rebuild.roofs.iter().zip(&r.roofs) {
        assert_eq!(&roof.roof, id);
        let h = face.height_m;
        assert!((h - 6.80).abs() < 0.005, "{id}: slope height {h}");
        assert!(
            (face.width_m - l).abs() < 1e-6,
            "{id}: eave {}",
            face.width_m
        );
        let expected = match roof.hip.as_str() {
            "left" => [[0.0, 0.0], [l, 0.0], [l, h], [run, h]],
            "right" => [[0.0, 0.0], [l, 0.0], [l - run, h], [0.0, h]],
            other => panic!("hip {other}"),
        };
        let outline = face.outline.as_ref().expect("a trapezoid has an outline");
        assert_eq!(outline.vertices_uv.len(), 4, "{id}");
        for e in expected {
            assert!(
                outline
                    .vertices_uv
                    .iter()
                    .any(|v| (v[0] - e[0]).abs() < 1e-6 && (v[1] - e[1]).abs() < 1e-6),
                "{id}: no vertex at {e:?} in {:?}",
                outline.vertices_uv
            );
        }
        let mut kinds = outline.edge_kinds.clone();
        kinds.sort_by_key(|k| format!("{k:?}"));
        assert_eq!(
            kinds,
            [
                pv_model::RoofEdgeKind::Eave,
                pv_model::RoofEdgeKind::Hip,
                pv_model::RoofEdgeKind::Ridge,
                pv_model::RoofEdgeKind::Verge
            ],
            "{id}"
        );
    }
    // Ridge at 25.00 m: the wind reference height of K2.
    for b in &r.store.project().buildings {
        assert!(
            (b.ridge_height_m - RIDGE_M).abs() < 1e-9,
            "{}",
            b.ridge_height_m
        );
        assert_eq!(b.roof_kind, RoofKind::Hipped);
    }
}

#[test]
fn the_whole_rebuild_is_undone_redone_and_reopened() {
    let mut r = rebuild();
    let built = serde_json::to_string(r.store.project()).unwrap();
    let bom = (*r.graph.get::<ProjectBom>(Instance::Project).unwrap()).clone();
    let steps = r.steps;
    assert!(steps >= 15, "{steps} commands");

    // Every command undone: back to the empty project, the graph following.
    for _ in 0..steps {
        let c = r.store.undo().unwrap();
        sync_changes(&mut r.graph, r.store.project(), &c).unwrap();
        r.graph.run_auto(&CancelToken::new()).unwrap();
    }
    let p = r.store.project();
    assert!(p.buildings.is_empty() && p.roof_faces.is_empty());
    assert!(p.arrays.is_empty() && p.modules.is_empty());
    assert!(r.store.undo().is_err(), "nothing left to undo");

    // Every command redone: the same project, bit for bit, and the same results.
    for _ in 0..steps {
        let c = r.store.redo().unwrap();
        sync_changes(&mut r.graph, r.store.project(), &c).unwrap();
        r.graph.run_auto(&CancelToken::new()).unwrap();
    }
    assert_eq!(serde_json::to_string(r.store.project()).unwrap(), built);
    compare_with_full(&mut r.graph, r.store.project()).unwrap();
    assert_eq!(*r.graph.get::<ProjectBom>(Instance::Project).unwrap(), bom);
    let p = r.store.project().clone();
    let (_, failures) = deviations(&mut r.graph, &p, &r.roofs);
    assert!(failures.is_empty(), "after redo:\n{}", failures.join("\n"));

    // Reopening the file: the journal replays to the same project.
    let path = r.store.path().to_path_buf();
    drop(r.store);
    let reopened = ProjectStore::open(&path).unwrap();
    assert_eq!(serde_json::to_string(reopened.project()).unwrap(), built);
    let mut fresh = pv_pipeline::new_graph();
    sync_project(&mut fresh, reopened.project()).unwrap();
    let (_, failures) = deviations(&mut fresh, reopened.project(), &r.roofs);
    assert!(
        failures.is_empty(),
        "after reopening:\n{}",
        failures.join("\n")
    );
}
