//! Test projects built from the anonymised K2 references (`testdata/k2/`).
#![allow(dead_code)]

pub mod k2;

use pv_graph::{CancelToken, Graph, Instance};
use pv_layout::model::{grid_spec, relayout_fixed};
use pv_layout::{layout_east_west, Search};
use pv_model::component::legacy::adopt_legacy_mounting;
use pv_model::component::{presets, Currency, IsoDate, PriceEntry};
use pv_model::ComponentSnapshot;
use pv_model::{
    AeroZoneSpec, ArrayGrid, ArrayId, ArrayLayoutParams, BallastSpec, BlockParams, Building,
    BuildingId, ConsequenceClass, CpSpec, CrossRailSpec, MembraneKind, ModuleState, MountingSystem,
    Orientation, PairCoefficientsSpec, PanelArray, Project, ProjectId, RoofEdgeKind, RoofFace,
    RoofFaceId, RoofKind, RoofOutlineSpec, ServiceLifeModel, SiteParams, SnowExposure,
    SupportGridKind, SupportGridSpec, TerrainCategory, WindZone,
};
use serde::Deserialize;

pub const LAYOUT: &str = include_str!("../../../../testdata/k2/reference_layout.json");
pub const LOADS: &str = include_str!("../../../../testdata/k2/reference_loads.json");
pub const DESCENT: &str = include_str!("../../../../testdata/k2/reference_descent_ballast.json");

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
    site: pv_statics::SiteInput,
}

/// Site of a K2 load case, as stored in a project (reverse of `convert::site_input`).
pub fn site_of(case: &str) -> (SiteParams, f64) {
    let loads: LoadsFile = serde_json::from_str(LOADS).unwrap();
    let s = loads.cases.into_iter().find(|c| c.id == case).unwrap().site;
    let params = SiteParams {
        altitude_m: s.altitude_m,
        snow_altitude_correction_m: s.snow_altitude_correction_m,
        wind_reference_pressure_kn_m2: s.wind_reference_pressure_kn_m2,
        terrain: match s.terrain {
            pv_statics::TerrainCategory::II => TerrainCategory::II,
            pv_statics::TerrainCategory::IIa => TerrainCategory::IIa,
            pv_statics::TerrainCategory::III => TerrainCategory::III,
            pv_statics::TerrainCategory::IV => TerrainCategory::IV,
        },
        snow_exposure: match s.snow_exposure {
            pv_statics::SnowExposure::Windswept => SnowExposure::Windswept,
            pv_statics::SnowExposure::Normal => SnowExposure::Normal,
            pv_statics::SnowExposure::Sheltered => SnowExposure::Sheltered,
        },
        thermal_coefficient: s.thermal_coefficient,
        service_life_years: s.service_life_years,
        consequence_class: match s.consequence_class {
            pv_statics::ConsequenceClass::CC1 => ConsequenceClass::CC1,
            pv_statics::ConsequenceClass::CC2 => ConsequenceClass::CC2,
            pv_statics::ConsequenceClass::CC3 => ConsequenceClass::CC3,
        },
        service_life_model: match s.service_life_model {
            pv_statics::ServiceLifeModel::En1991 => ServiceLifeModel::En1991,
            pv_statics::ServiceLifeModel::Explicit { snow, wind } => {
                ServiceLifeModel::Explicit { snow, wind }
            }
        },
    };
    (params, s.building_height_m)
}

/// The base catalogue's K2 SingleRail system, its parts and the LONGi module, frozen without
/// prices (`Project::components` order: by id).
pub fn k2_components() -> Vec<ComponentSnapshot> {
    let mut all: Vec<ComponentSnapshot> = presets::k2_single_rail_parts()
        .into_iter()
        .chain([presets::lr7_54hvh_490m(), presets::k2_single_rail()])
        .map(|component| ComponentSnapshot {
            component,
            price: None,
        })
        .collect();
    all.sort_by_key(|s| s.component.id);
    all
}

/// Sets the list price (per piece, in currency units) of the frozen part `article_no`.
pub fn set_list_price(p: &mut Project, article_no: &str, list_price: f64) {
    let s = p
        .components
        .iter_mut()
        .find(|s| s.component.article_no == article_no)
        .unwrap_or_else(|| panic!("no component {article_no}"));
    s.price = Some(PriceEntry {
        component: s.component.id,
        supplier: s.component.manufacturer.clone(),
        list_price_cents: (list_price * 100.0).round() as i64,
        currency: Currency::Chf,
        valid_from: IsoDate::parse("2026-01-01").unwrap(),
        valid_to: None,
    });
}

pub fn k2_rails() -> CrossRailSpec {
    CrossRailSpec {
        layers: pv_model::RailLayers::Two,
        per_column: 2,
        top_inset_m: 0.40,
        top_overhang_m: 0.05,
        bottom_spacing_m: 1.0,
        min_support_overhang_m: 0.06,
        spacing_step_m: 0.01,
        bottom_inset_m: 0.275,
        fixation_edge_m: 0.03,
        max_cantilever_m: Some(0.485),
    }
}

/// Project A, roof 1: hipped 19°, double standing seam every 0.60 m, two K2 SingleRail
/// fields of landscape 1800 × 1134 modules in 2 × 2 blocks (field 2 has one module removed).
pub fn ref_a_roof1() -> Project {
    let layout: LayoutFile = serde_json::from_str(LAYOUT).unwrap();
    let (site, height) = site_of("ref-a");
    let building = Building {
        id: BuildingId::new(),
        name: "A".into(),
        origin: [0.0, 0.0, 0.0],
        footprint: pv_model::rectangle_footprint([26.18, 12.86]),
        size_m: [26.18, 12.86],
        eave_height_m: height - 2.0,
        ridge_height_m: height,
        roof_kind: RoofKind::Hipped,
        roof: None,
        site: Some(site),
    };
    let rf = &layout.roof_1_face;
    let face = RoofFace {
        id: RoofFaceId::new(),
        building_id: building.id,
        name: "roof-1".into(),
        origin: [0.0, 0.0, 0.0],
        tilt_deg: 19.0,
        azimuth_deg: 180.0,
        width_m: 26.18,
        height_m: 6.80,
        outline: Some(RoofOutlineSpec {
            vertices_uv: rf.vertices_uv.clone(),
            edge_kinds: rf.edge_kinds.clone(),
            obstacles: vec![],
        }),
        support_grid: Some(SupportGridSpec {
            kind: SupportGridKind::StandingSeam,
            origin_m: 0.0,
            spacing_m: 0.6,
        }),
    };
    let mut arrays = Vec::new();
    let mut modules = Vec::new();
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
                rails: Some(k2_rails()),
                ..ArrayLayoutParams::default()
            },
            module: Some(presets::LR7_54HVH_490M),
            mounting: Some(presets::K2_SINGLE_RAIL),
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
        let mut ms = placed.module_instances(a.id, modules.len() as u32);
        if pf.field == 2 {
            ms.iter_mut()
                .find(|m| m.row == 3 && m.col == 3)
                .unwrap()
                .state = ModuleState::Removed;
        }
        modules.extend(ms);
        arrays.push(a);
    }
    Project {
        id: ProjectId::new(),
        name: "ref-a roof 1".into(),
        buildings: vec![building],
        roof_faces: vec![face],
        arrays,
        modules,
        pricing: pv_model::Pricing::default(),
        components: k2_components(),
    }
}

fn pair(cp_east: f64, cp_west: f64, k: [f64; 6]) -> PairCoefficientsSpec {
    PairCoefficientsSpec {
        cp_east,
        cp_west,
        k_slope: k[0],
        k_direction: k[1],
        k_parapet: k[2],
        k_module_size: k[3],
        k_row_spacing: k[4],
        k_building_height: k[5],
    }
}

/// D-Dome table of reference B (9 m): K2 prints the eave-corner and interior pairs; the
/// other edge zones take the eave-corner coefficients (conservative).
pub fn d_dome_ref_b_h9() -> BallastSpec {
    let k = [1.00, 0.90, 1.03, 1.03, 1.01, 1.00];
    let corner = (pair(-0.028, -0.324, k), pair(0.079, -0.273, k));
    let interior = (pair(-0.011, -0.182, k), pair(0.087, -0.246, k));
    let entry = |zone, (uplift, sliding)| AeroZoneSpec {
        zone,
        uplift,
        sliding,
    };
    BallastSpec {
        manufacturer: "K2 Systems".into(),
        product: "D-Dome 6.10".into(),
        document: "back-fitted from K2 Base report (ref-b-h9)".into(),
        module_tilt_deg: 8.6,
        entries: vec![
            entry(WindZone::Interior, interior),
            entry(WindZone::Ridge, corner),
            entry(WindZone::Verge, corner),
            entry(WindZone::Eave, corner),
            entry(WindZone::EaveCorner, corner),
        ],
        pair_own_mass_kg: 47.5,
        block_name: "Concrete block 11 kg".into(),
        block_mass_kg: 11.0,
        membrane: MembraneKind::Gravel,
        friction_coefficient: 0.7,
        friction_source: "K2 Base report".into(),
    }
}

/// Reference B (9 m): a flat roof with parapets and one D-Dome east-west system.
pub fn ref_b_east_west() -> Project {
    let (site, height) = site_of("ref-b-h9");
    let building = Building {
        id: BuildingId::new(),
        name: "B".into(),
        origin: [0.0, 0.0, 0.0],
        footprint: pv_model::rectangle_footprint([24.0, 12.0]),
        size_m: [24.0, 12.0],
        eave_height_m: height,
        ridge_height_m: height,
        roof_kind: RoofKind::Flat,
        roof: None,
        site: Some(site),
    };
    let face = RoofFace {
        id: RoofFaceId::new(),
        building_id: building.id,
        name: "flat".into(),
        origin: [0.0, 0.0, 0.0],
        tilt_deg: 0.0,
        azimuth_deg: 180.0,
        width_m: 24.0,
        height_m: 12.0,
        outline: Some(RoofOutlineSpec {
            vertices_uv: vec![[0.0, 0.0], [24.0, 0.0], [24.0, 12.0], [0.0, 12.0]],
            edge_kinds: vec![RoofEdgeKind::Parapet; 4],
            obstacles: vec![],
        }),
        support_grid: None,
    };
    let params = ArrayLayoutParams {
        boundary_uv: vec![[0.0, 0.0], [24.0, 0.0], [24.0, 12.0], [0.0, 12.0]],
        module_gap_u_m: 0.08,
        module_gap_v_m: 0.038,
        edge_setback_m: 0.6,
        row_pitch_m: Some(2.46),
        block: Some(BlockParams {
            max_cols: 4,
            max_rows: 3,
            gap_u_m: 0.10,
            gap_v_m: 0.10,
        }),
        ..ArrayLayoutParams::default()
    };
    let mounting = MountingSystem {
        name: "K2 D-Dome 6.10".into(),
        module_mass_kg: 21.5,
        system_mass_per_module_kg: 2.25,
        loaded_area_m2: 10.0,
        psi0_snow: None,
        cp: CpSpec::Standard,
        rails: None,
        ballast: Some(d_dome_ref_b_h9()),
        parts: Vec::new(),
    };
    let mk = |az: f64| PanelArray {
        id: ArrayId::new(),
        roof_face_id: face.id,
        module_length_m: 1.762,
        module_width_m: 1.134,
        orientation: Orientation::Landscape,
        tilt_on_roof_deg: 8.6,
        azimuth_offset_deg: az,
        grid: ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0),
        layout: params.clone(),
        module: None,
        mounting: None,
    };
    let (mut west, mut east) = (mk(90.0), mk(-90.0));
    west.layout.paired_with = Some(east.id);
    east.layout.paired_with = Some(west.id);
    let (lw, le) = layout_east_west(&face, &west, &east, Search::default()).unwrap();
    let mut modules = lw.module_instances(west.id, 0);
    modules.extend(le.module_instances(east.id, modules.len() as u32));
    west.grid = lw.grid;
    east.grid = le.grid;
    let mut p = Project {
        id: ProjectId::new(),
        name: "ref-b east-west".into(),
        buildings: vec![building],
        roof_faces: vec![face],
        arrays: vec![west, east],
        modules,
        pricing: pv_model::Pricing::default(),
        components: Vec::new(),
    };
    for i in 0..2 {
        adopt_legacy_mounting(&mut p, i, &mounting).unwrap();
    }
    p
}

/// Graph synced on `project`, first pass done.
pub fn graph_for(project: &Project) -> Graph {
    let mut g = pv_pipeline::new_graph();
    pv_pipeline::sync_project(&mut g, project).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    g
}

pub fn array(i: usize, p: &Project) -> Instance {
    Instance::Array(p.arrays[i].id)
}

/// Every array-level node of `inc` against a graph built from scratch on `p`: same value or
/// same error, compared through their serialised form. `Err` lists the differences.
pub fn compare_with_full(inc: &mut Graph, p: &Project) -> Result<(), String> {
    use pv_pipeline::nodes::{ballast::Ballast, descent::Descent, layout::Hardware};
    use pv_pipeline::nodes::{loads::ZoneLoads, zones::ModuleZones};
    use pv_pipeline::FieldStatics;

    fn json<K: pv_graph::Kind>(g: &mut Graph, i: Instance) -> serde_json::Value
    where
        K::Output: serde::Serialize,
    {
        match g.get::<K>(i) {
            Ok(v) => serde_json::to_value(&*v).unwrap(),
            Err(e) => serde_json::json!({ "error": e }),
        }
    }
    let mut full = pv_pipeline::new_graph();
    pv_pipeline::sync_project(&mut full, p).unwrap();
    full.run_auto(&CancelToken::new()).unwrap();
    let mut diffs = Vec::new();
    for a in &p.arrays {
        let i = Instance::Array(a.id);
        for (name, ours, theirs) in [
            (
                "statics",
                json::<FieldStatics>(inc, i),
                json::<FieldStatics>(&mut full, i),
            ),
            (
                "hardware",
                json::<Hardware>(inc, i),
                json::<Hardware>(&mut full, i),
            ),
            (
                "zones",
                json::<ModuleZones>(inc, i),
                json::<ModuleZones>(&mut full, i),
            ),
            (
                "loads",
                json::<ZoneLoads>(inc, i),
                json::<ZoneLoads>(&mut full, i),
            ),
            (
                "descent",
                json::<Descent>(inc, i),
                json::<Descent>(&mut full, i),
            ),
            (
                "ballast",
                json::<Ballast>(inc, i),
                json::<Ballast>(&mut full, i),
            ),
        ] {
            if ours != theirs {
                diffs.push(format!("array {} {name} differs", a.id.0));
            }
        }
    }
    {
        use pv_pipeline::nodes::bom::ProjectBom;
        let project = Instance::Project;
        if json::<ProjectBom>(inc, project) != json::<ProjectBom>(&mut full, project) {
            diffs.push("project BOM differs".into());
        }
    }
    if diffs.is_empty() {
        Ok(())
    } else {
        Err(diffs.join("\n"))
    }
}

/// A project file for `p` in `dir` (real commands: validation, journal, undo / redo).
pub fn store_for(dir: &tempfile::TempDir, name: &str, p: &Project) -> pv_store::ProjectStore {
    pv_store::ProjectStore::create(&dir.path().join(format!("{name}.pvproj")), p.clone()).unwrap()
}

/// SplitMix64: small and deterministic.
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// Uniform in `[-a, a]`.
    pub fn signed(&mut self, a: f64) -> f64 {
        (self.below(2001) as f64 / 1000.0 - 1.0) * a
    }
}
