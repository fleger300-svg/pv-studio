//! L2: module grid against the K2 Base report of project A (`testdata/k2/reference_layout.json`).
//!
//! - field sizes for every printed `cols × rows` field (1 cm, as printed),
//! - automatic filling of each field outline: module counts and block shapes, exact,
//! - roof-1 reconstructed from its printed dimensions: both fields fit, and the 18 printed
//!   distances to the roof edges are reproduced (0.1 m, truncated as K2 prints them).

use std::collections::BTreeMap;

use pv_layout::model::{grid_spec, layout_array, relayout_fixed};
use pv_layout::Search;
use pv_model::{
    ArrayGrid, ArrayId, ArrayLayoutParams, BlockParams, BuildingId, Orientation, PanelArray,
    RoofEdgeKind, RoofFace, RoofFaceId, RoofOutlineSpec,
};
use serde::Deserialize;

const FIXTURE: &str = include_str!("../../../testdata/k2/reference_layout.json");

#[derive(Deserialize)]
struct Fixture {
    module: ModuleIn,
    grid: GridIn,
    fields: Vec<FieldIn>,
    expected_modules: BTreeMap<String, usize>,
    roof_1_face: FaceIn,
}

#[derive(Deserialize)]
struct ModuleIn {
    length_m: f64,
    width_m: f64,
}

#[derive(Deserialize)]
struct GridIn {
    gap_u_m: f64,
    gap_v_m: f64,
    block: BlockIn,
}

#[derive(Deserialize, Clone, Copy)]
struct BlockIn {
    max_cols: u32,
    max_rows: u32,
    gap_u_m: f64,
    gap_v_m: f64,
}

impl BlockIn {
    fn params(self) -> BlockParams {
        BlockParams {
            max_cols: self.max_cols,
            max_rows: self.max_rows,
            gap_u_m: self.gap_u_m,
            gap_v_m: self.gap_v_m,
        }
    }
}

#[derive(Deserialize)]
struct FieldIn {
    roof: String,
    field: u32,
    cols: u32,
    rows: u32,
    width_m: f64,
    length_m: f64,
    blocks: Vec<[u32; 2]>,
    removed: usize,
}

#[derive(Deserialize)]
struct FaceIn {
    vertices_uv: Vec<[f64; 2]>,
    edge_kinds: Vec<RoofEdgeKind>,
    fields: Vec<PlacedField>,
}

#[derive(Deserialize)]
struct PlacedField {
    field: u32,
    cols: u32,
    rows: u32,
    lower_left: [f64; 2],
    blocks: Vec<BlockDistances>,
}

#[derive(Deserialize)]
struct BlockDistances {
    block: [u32; 2],
    k2_no: u32,
    hip: Option<f64>,
    ridge: Option<f64>,
    eave: Option<f64>,
    right: Option<f64>,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("reference_layout.json")
}

fn face(width: f64, height: f64, outline: Option<RoofOutlineSpec>) -> RoofFace {
    RoofFace {
        id: RoofFaceId::new(),
        building_id: BuildingId::new(),
        name: "test".into(),
        origin: [0.0, 0.0, 0.0],
        tilt_deg: 19.0,
        azimuth_deg: 180.0,
        width_m: width,
        height_m: height,
        outline,
        support_grid: None,
    }
}

fn array(fx: &Fixture, face: &RoofFace, boundary: Vec<[f64; 2]>) -> PanelArray {
    PanelArray {
        id: ArrayId::new(),
        roof_face_id: face.id,
        module_length_m: fx.module.length_m,
        module_width_m: fx.module.width_m,
        orientation: Orientation::Landscape,
        tilt_on_roof_deg: 0.0,
        azimuth_offset_deg: 0.0,
        grid: ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0),
        layout: ArrayLayoutParams {
            boundary_uv: boundary,
            module_gap_u_m: fx.grid.gap_u_m,
            module_gap_v_m: fx.grid.gap_v_m,
            block: Some(fx.grid.block.params()),
            ..ArrayLayoutParams::default()
        },
    }
}

/// Block shapes (cols × rows) present in a set of cells, as a sorted list.
fn block_shapes(cells: &[pv_layout::Cell], block: &BlockParams) -> Vec<[u32; 2]> {
    let mut blocks: BTreeMap<(u32, u32), (u32, u32, u32, u32)> = BTreeMap::new();
    for c in cells {
        let key = (c.col / block.max_cols, c.row / block.max_rows);
        let e = blocks.entry(key).or_insert((u32::MAX, 0, u32::MAX, 0));
        e.0 = e.0.min(c.col);
        e.1 = e.1.max(c.col);
        e.2 = e.2.min(c.row);
        e.3 = e.3.max(c.row);
    }
    let mut shapes: Vec<[u32; 2]> = blocks
        .values()
        .map(|&(c0, c1, r0, r1)| [c1 - c0 + 1, r1 - r0 + 1])
        .collect();
    shapes.sort_unstable();
    shapes
}

#[test]
fn field_sizes_match_k2() {
    let fx = fixture();
    let f = face(30.0, 10.0, None);
    let spec = grid_spec(&array(&fx, &f, vec![]));
    for field in &fx.fields {
        let (w, l) = spec.extent(field.cols, field.rows);
        let tag = format!(
            "{} field {} ({}×{})",
            field.roof, field.field, field.cols, field.rows
        );
        assert!(
            (w - field.width_m).abs() < 0.005 + 1e-9,
            "{tag}: width {w:.4} vs K2 {}",
            field.width_m
        );
        assert!(
            (l - field.length_m).abs() < 0.005 + 1e-9,
            "{tag}: length {l:.4} vs K2 {}",
            field.length_m
        );
    }
}

#[test]
fn filling_each_field_reproduces_k2_counts_and_blocks() {
    let fx = fixture();
    let mut per_roof: BTreeMap<String, usize> = BTreeMap::new();
    for field in &fx.fields {
        // The field outline as drawn in K2: printed size plus half a printed unit (K2 rounds
        // to 1 cm: 3.552 m prints as 3.55 m), at an arbitrary place on a large face.
        let f = face(30.0, 10.0, None);
        let (u0, v0) = (2.0, 1.0);
        let (w, l) = (field.width_m + 0.005, field.length_m + 0.005);
        let boundary = vec![[u0, v0], [u0 + w, v0], [u0 + w, v0 + l], [u0, v0 + l]];
        let a = array(&fx, &f, boundary);
        let layout = layout_array(&f, &a, Search::default()).unwrap();
        let tag = format!("{} field {}", field.roof, field.field);
        assert_eq!(
            layout.cells.len() as u32,
            field.cols * field.rows,
            "{tag}: module count"
        );
        assert_eq!(
            (layout.grid.cols, layout.grid.rows),
            (field.cols, field.rows),
            "{tag}: grid size"
        );
        let mut expected = field.blocks.clone();
        expected.sort_unstable();
        assert_eq!(
            block_shapes(&layout.cells, &fx.grid.block.params()),
            expected,
            "{tag}: block shapes"
        );
        // K2 removed modules by hand (state Removed in our model).
        *per_roof.entry(field.roof.clone()).or_default() += layout.cells.len() - field.removed;
    }
    let total: usize = per_roof.values().sum();
    for (roof, n) in &per_roof {
        assert_eq!(Some(n), fx.expected_modules.get(roof), "{roof}: modules");
    }
    assert_eq!(Some(&total), fx.expected_modules.get("total"));
}

#[test]
fn roof_1_reconstruction_reproduces_k2_edge_distances() {
    let fx = fixture();
    let rf = &fx.roof_1_face;
    let outline = RoofOutlineSpec {
        vertices_uv: rf.vertices_uv.clone(),
        edge_kinds: rf.edge_kinds.clone(),
        obstacles: vec![],
    };
    let f = face(26.18, 6.80, Some(outline));
    let [_, _, [_, ridge_v], [hip_u_top, _]] = [
        rf.vertices_uv[0],
        rf.vertices_uv[1],
        rf.vertices_uv[2],
        rf.vertices_uv[3],
    ];
    let right_u = rf.vertices_uv[1][0];
    let hip_slope = hip_u_top / ridge_v; // u of the hip per metre of v
    let trunc = |x: f64| (x * 10.0 + 1e-9).floor() / 10.0;

    let mut compared = 0;
    for pf in &rf.fields {
        let mut a = array(&fx, &f, vec![]);
        let spec = grid_spec(&a);
        a.grid = ArrayGrid {
            rows: pf.rows,
            cols: pf.cols,
            origin_u: pf.lower_left[0] + spec.footprint_u / 2.0,
            origin_v: pf.lower_left[1] + spec.footprint_v / 2.0,
            ..ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0)
        };
        let placed = relayout_fixed(&f, &a).unwrap();
        assert_eq!(
            placed.cells.len() as u32,
            pf.cols * pf.rows,
            "field {}: every module fits the face",
            pf.field
        );

        let b = fx.grid.block.params();
        for bd in &pf.blocks {
            let cells: Vec<_> = placed
                .cells
                .iter()
                .filter(|c| [c.col / b.max_cols, c.row / b.max_rows] == bd.block)
                .collect();
            assert!(!cells.is_empty(), "block {}", bd.k2_no);
            let (hu, hv) = (spec.footprint_u / 2.0, spec.footprint_v / 2.0);
            let left = cells
                .iter()
                .map(|c| c.centre.u - hu)
                .fold(f64::INFINITY, f64::min);
            let right = cells
                .iter()
                .map(|c| c.centre.u + hu)
                .fold(f64::NEG_INFINITY, f64::max);
            let bottom = cells
                .iter()
                .map(|c| c.centre.v - hv)
                .fold(f64::INFINITY, f64::min);
            let top = cells
                .iter()
                .map(|c| c.centre.v + hv)
                .fold(f64::NEG_INFINITY, f64::max);
            let ours = [
                ("hip", left - hip_slope * bottom, bd.hip),
                ("ridge", ridge_v - top, bd.ridge),
                ("eave", bottom, bd.eave),
                ("right", right_u - right, bd.right),
            ];
            for (name, value, k2) in ours {
                if let Some(k2) = k2 {
                    compared += 1;
                    assert!(
                        (trunc(value) - k2).abs() < 1e-9,
                        "field {} block {} {name}: engine {value:.3} → {:.1}, K2 {k2}",
                        pf.field,
                        bd.k2_no,
                        trunc(value)
                    );
                }
            }
        }
    }
    assert_eq!(compared, 18);
}
