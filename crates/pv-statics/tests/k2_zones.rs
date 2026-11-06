//! Wind zones of the whole face assigned to each field (architecture: « calculées pour le pan
//! entier, puis attribuées à chaque champ selon sa position »), against K2 project A roof-1.
//!
//! Geometry: `testdata/k2/reference_layout.json` (`roof_1_face`); zone sets per field:
//! `testdata/k2/reference_loads.json` (`ref-a`, `roofs[roof-1]`).

use pv_geo::Vec2;
use pv_layout::model::{grid_spec, relayout_fixed};
use pv_layout::RoofOutline;
use pv_model::{
    ArrayGrid, ArrayId, ArrayLayoutParams, BlockParams, BuildingId, Orientation, PanelArray,
    RoofEdgeKind, RoofFace, RoofFaceId, RoofOutlineSpec,
};
use pv_statics::zones::{field_zones, ZoneGeometry};
use pv_statics::RoofZone;
use serde::Deserialize;

const LAYOUT: &str = include_str!("../../../testdata/k2/reference_layout.json");
const LOADS: &str = include_str!("../../../testdata/k2/reference_loads.json");

#[derive(Deserialize)]
struct LayoutFile {
    roof_1_face: FaceIn,
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
}

#[derive(Deserialize)]
struct LoadsFile {
    cases: Vec<LoadCase>,
}

#[derive(Deserialize)]
struct LoadCase {
    id: String,
    #[serde(default)]
    roofs: Vec<Roof>,
    site: Site,
}

#[derive(Deserialize)]
struct Site {
    building_height_m: f64,
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

fn sorted(mut z: Vec<RoofZone>) -> Vec<RoofZone> {
    z.sort_by_key(|z| RoofZone::ALL.iter().position(|a| a == z));
    z.dedup();
    z
}

fn roof_1_field_centres() -> (RoofOutline, Vec<(u32, Vec<Vec2>)>, f64) {
    let layout: LayoutFile = serde_json::from_str(LAYOUT).unwrap();
    let loads: LoadsFile = serde_json::from_str(LOADS).unwrap();
    let rf = &layout.roof_1_face;
    let spec = RoofOutlineSpec {
        vertices_uv: rf.vertices_uv.clone(),
        edge_kinds: rf.edge_kinds.clone(),
        obstacles: vec![],
    };
    let face = RoofFace {
        id: RoofFaceId::new(),
        building_id: BuildingId::new(),
        name: "roof-1".into(),
        origin: [0.0, 0.0, 0.0],
        tilt_deg: 19.0,
        azimuth_deg: 180.0,
        width_m: 26.18,
        height_m: 6.80,
        outline: Some(spec.clone()),
        support_grid: None,
    };
    let mut fields = Vec::new();
    for pf in &rf.fields {
        let mut a = PanelArray {
            id: ArrayId::new(),
            roof_face_id: face.id,
            module_length_m: 1.8,
            module_width_m: 1.134,
            orientation: Orientation::Landscape,
            tilt_on_roof_deg: 0.0,
            azimuth_offset_deg: 0.0,
            grid: ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0),
            layout: ArrayLayoutParams {
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
        };
        let s = grid_spec(&a);
        a.grid = ArrayGrid {
            rows: pf.rows,
            cols: pf.cols,
            origin_u: pf.lower_left[0] + s.footprint_u / 2.0,
            origin_v: pf.lower_left[1] + s.footprint_v / 2.0,
            ..ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0)
        };
        let placed = relayout_fixed(&face, &a).unwrap();
        fields.push((pf.field, placed.cells.iter().map(|c| c.centre).collect()));
    }
    let h = loads
        .cases
        .iter()
        .find(|c| c.id == "ref-a")
        .unwrap()
        .site
        .building_height_m;
    (RoofOutline::from_spec(&spec).unwrap(), fields, h)
}

#[test]
fn roof_1_fields_get_k2_zones() {
    let loads: LoadsFile = serde_json::from_str(LOADS).unwrap();
    let k2 = &loads
        .cases
        .iter()
        .find(|c| c.id == "ref-a")
        .unwrap()
        .roofs
        .iter()
        .find(|r| r.id == "roof-1")
        .unwrap()
        .fields;
    let (roof, fields, h) = roof_1_field_centres();
    // Wind onto the long face: b = eave length 26.18 m; e = min(b, 2h) = 26.18 m.
    let g = ZoneGeometry::new(26.18, h).unwrap();
    assert!((g.e.value - 26.18).abs() < 1e-12);
    for (field, centres) in &fields {
        let ours = field_zones(&roof, &g, centres);
        let theirs = sorted(k2.iter().find(|f| f.field == *field).unwrap().zones.clone());
        assert_eq!(ours, theirs, "roof-1 field {field}");
    }
    // Discrimination: with e from the building depth (12.86 m), field 1 loses its ridge zone.
    let narrow = ZoneGeometry::new(12.86, h).unwrap();
    let f1 = field_zones(&roof, &narrow, &fields[0].1);
    assert!(!f1.contains(&RoofZone::Ridge), "{f1:?}");
}
