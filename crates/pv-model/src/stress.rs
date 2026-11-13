//! Synthetic stress-test projects (performance prototype only — not a layout algorithm).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::geometry::Vec3;
use crate::ids::{ArrayId, BuildingId, ModuleId, ProjectId, RoofFaceId};
use crate::layout::{ArrayLayoutParams, CrossRailSpec, SupportGridKind, SupportGridSpec};
use crate::mounting::{
    AeroZoneSpec, BallastSpec, CpEntrySpec, CpSpec, MembraneKind, MountingSystem,
    PairCoefficientsSpec, WindZone,
};
use crate::project::{
    ArrayGrid, Building, ModuleInstance, ModuleState, Orientation, PanelArray, Project, RoofFace,
    RoofKind, MODULES_PER_STRING,
};
use crate::site::{SiteParams, TerrainCategory};

pub const MODULE_LENGTH_M: f64 = 1.762;
pub const MODULE_WIDTH_M: f64 = 1.134;
pub const MODULE_GAP_M: f64 = 0.020;
pub const EDGE_SETBACK_M: f64 = 0.300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum StressPreset {
    /// Three gable-roof barns, south + north faces at 30°, ~1,000 coplanar portrait modules.
    Residential1k,
    /// One 160 m × 140 m flat roof with 10° east-west racks, ~10,000 modules.
    Industrial10k,
}

impl StressPreset {
    /// Builds the synthetic project for this preset.
    #[must_use]
    pub fn build(self) -> Project {
        match self {
            StressPreset::Residential1k => residential_1k(),
            StressPreset::Industrial10k => industrial_10k(),
        }
    }
}

/// Orientation and size of a rectangular roof face.
struct FaceShape {
    tilt_deg: f64,
    azimuth_deg: f64,
    width_m: f64,
    height_m: f64,
}

struct Builder {
    project: Project,
}

impl Builder {
    fn new(name: &str) -> Self {
        Self {
            project: Project {
                id: ProjectId::new(),
                name: name.to_owned(),
                buildings: Vec::new(),
                roof_faces: Vec::new(),
                arrays: Vec::new(),
                modules: Vec::new(),
                pricing: crate::pricing::Pricing::default(),
                components: Vec::new(),
            },
        }
    }

    fn face(
        &mut self,
        building_id: BuildingId,
        name: &str,
        origin: Vec3,
        shape: FaceShape,
        support_grid: Option<SupportGridSpec>,
    ) -> RoofFaceId {
        let id = RoofFaceId::new();
        self.project.roof_faces.push(RoofFace {
            id,
            building_id,
            name: name.to_owned(),
            origin,
            tilt_deg: shape.tilt_deg,
            azimuth_deg: shape.azimuth_deg,
            width_m: shape.width_m,
            height_m: shape.height_m,
            outline: None,
            support_grid,
        });
        id
    }

    /// Adds an array and its modules at the given `(row, col, u, v)` positions.
    fn array(
        &mut self,
        mut array: PanelArray,
        mounting: &MountingSystem,
        cells: impl IntoIterator<Item = (u32, u32, f64, f64)>,
    ) {
        array.id = ArrayId::new();
        for (row, col, u, v) in cells {
            let id = self.project.modules.len() as u32;
            self.project.modules.push(ModuleInstance {
                id: ModuleId(id),
                array_id: array.id,
                row,
                col,
                u,
                v,
                state: ModuleState::Active,
                string_index: id / MODULES_PER_STRING,
            });
        }
        self.project.arrays.push(array);
        let index = self.project.arrays.len() - 1;
        crate::component::legacy::adopt_legacy_mounting(&mut self.project, index, mounting)
            .expect("stress presets convert to components");
    }
}

/// Number of modules of footprint `size` (with gaps) that fit in `extent` after setbacks.
fn fit(extent: f64, size: f64) -> u32 {
    ((extent - 2.0 * EDGE_SETBACK_M + MODULE_GAP_M) / (size + MODULE_GAP_M))
        .floor()
        .max(0.0) as u32
}

fn residential_1k() -> Project {
    const LENGTH: f64 = 33.0; // along x (eave direction)
    const DEPTH: f64 = 19.6; // along y
    const EAVE: f64 = 6.0;
    const TILT: f64 = 30.0;
    const SPACING: f64 = 8.0;
    let half_depth = DEPTH / 2.0;
    let slope_len = half_depth / TILT.to_radians().cos();
    let ridge = EAVE + half_depth * TILT.to_radians().tan();

    let mut b = Builder::new("Residential 1k");
    for i in 0..3 {
        let cx = (f64::from(i) - 1.0) * (LENGTH + SPACING);
        let building = Building {
            id: BuildingId::new(),
            name: format!("Barn {}", i + 1),
            origin: [cx, 0.0, 0.0],
            footprint: crate::roof::rectangle_footprint([LENGTH, DEPTH]),
            size_m: [LENGTH, DEPTH],
            eave_height_m: EAVE,
            ridge_height_m: ridge,
            roof_kind: RoofKind::Gable,
            roof: None,
            site: Some(demo_site(TerrainCategory::III)),
        };
        let bid = building.id;
        b.project.buildings.push(building);

        // South face: origin at the west end of the south eave; u → east, v → up to ridge.
        let slope = |azimuth_deg| FaceShape {
            tilt_deg: TILT,
            azimuth_deg,
            width_m: LENGTH,
            height_m: slope_len,
        };
        let south = b.face(
            bid,
            "South",
            [cx - LENGTH / 2.0, -half_depth, EAVE],
            slope(180.0),
            Some(SEAMS),
        );
        // North face: origin at the east end of the north eave; u → west.
        let north = b.face(
            bid,
            "North",
            [cx + LENGTH / 2.0, half_depth, EAVE],
            slope(0.0),
            Some(SEAMS),
        );

        for face in [south, north] {
            let cols = fit(LENGTH, MODULE_WIDTH_M);
            let rows = fit(slope_len, MODULE_LENGTH_M);
            let pitch_u = MODULE_WIDTH_M + MODULE_GAP_M;
            let pitch_v = MODULE_LENGTH_M + MODULE_GAP_M;
            // Centre the block along the eave.
            let used_u = f64::from(cols) * pitch_u - MODULE_GAP_M;
            let origin_u = (LENGTH - used_u) / 2.0 + MODULE_WIDTH_M / 2.0;
            let origin_v = EDGE_SETBACK_M + MODULE_LENGTH_M / 2.0;
            let grid = ArrayGrid {
                rows,
                cols,
                origin_u,
                origin_v,
                pitch_u,
                pitch_v,
                block_cols: 0,
                block_rows: 0,
                block_extra_u: 0.0,
                block_extra_v: 0.0,
            };
            let array = PanelArray {
                id: ArrayId::new(),
                roof_face_id: face,
                module_length_m: MODULE_LENGTH_M,
                module_width_m: MODULE_WIDTH_M,
                orientation: Orientation::Portrait,
                tilt_on_roof_deg: 0.0,
                azimuth_offset_deg: 0.0,
                grid,
                module: None,
                mounting: None,
                layout: ArrayLayoutParams {
                    rails: Some(CrossRailSpec::k2_single_rail()),
                    ..ArrayLayoutParams::default()
                },
            };
            let cells = (0..rows).flat_map(move |r| {
                (0..cols).map(move |c| {
                    (
                        r,
                        c,
                        origin_u + f64::from(c) * pitch_u,
                        origin_v + f64::from(r) * pitch_v,
                    )
                })
            });
            b.array(array, &single_rail_on_gable(), cells);
        }
    }
    b.project
}

fn industrial_10k() -> Project {
    const WIDTH: f64 = 160.0; // x
    const DEPTH: f64 = 140.0; // y
    const HEIGHT: f64 = 10.0;
    const RACK_TILT: f64 = 10.0;
    const PAIRS: u32 = 70;
    const BLOCKS: u32 = 6;
    const ROWS_PER_BLOCK: u32 = 12;
    const AISLE_M: f64 = 1.5;

    let mut b = Builder::new("Industrial 10k");
    let building = Building {
        id: BuildingId::new(),
        name: "Hall".into(),
        origin: [0.0, 0.0, 0.0],
        footprint: crate::roof::rectangle_footprint([WIDTH, DEPTH]),
        size_m: [WIDTH, DEPTH],
        eave_height_m: HEIGHT,
        ridge_height_m: HEIGHT,
        roof_kind: RoofKind::Flat,
        roof: None,
        site: Some(demo_site(TerrainCategory::IV)),
    };
    let bid = building.id;
    b.project.buildings.push(building);
    let flat = FaceShape {
        tilt_deg: 0.0,
        azimuth_deg: 180.0,
        width_m: WIDTH,
        height_m: DEPTH,
    };
    let roof = b.face(
        bid,
        "Flat roof",
        [-WIDTH / 2.0, -DEPTH / 2.0, HEIGHT],
        flat,
        None,
    );

    // Landscape modules on E-W racks: long side runs north-south (along v).
    let foot_u = MODULE_WIDTH_M * RACK_TILT.to_radians().cos();
    let pair_pitch = 2.0 * foot_u + 2.0 * MODULE_GAP_M;
    let pitch_v = MODULE_LENGTH_M + MODULE_GAP_M;
    let rows = BLOCKS * ROWS_PER_BLOCK;
    let used_u = f64::from(PAIRS) * pair_pitch - MODULE_GAP_M;
    let used_v = f64::from(rows) * pitch_v - MODULE_GAP_M + f64::from(BLOCKS - 1) * AISLE_M;
    let start_u = (WIDTH - used_u) / 2.0;
    let start_v = (DEPTH - used_v) / 2.0;
    debug_assert!(start_u >= EDGE_SETBACK_M && start_v >= EDGE_SETBACK_M);

    let row_v = move |r: u32| {
        start_v
            + MODULE_LENGTH_M / 2.0
            + f64::from(r) * pitch_v
            + f64::from(r / ROWS_PER_BLOCK) * AISLE_M
    };
    // +90° → faces west (sits on the west side of each pair); −90° → faces east.
    for (offset, side) in [(90.0, 0.0), (-90.0, 1.0)] {
        let origin_u = start_u + foot_u / 2.0 + side * (foot_u + MODULE_GAP_M);
        let grid = ArrayGrid {
            rows,
            cols: PAIRS,
            origin_u,
            origin_v: row_v(0),
            pitch_u: pair_pitch,
            pitch_v,
            block_cols: 0,
            block_rows: 0,
            block_extra_u: 0.0,
            block_extra_v: 0.0,
        };
        let array = PanelArray {
            id: ArrayId::new(),
            roof_face_id: roof,
            module_length_m: MODULE_LENGTH_M,
            module_width_m: MODULE_WIDTH_M,
            orientation: Orientation::Landscape,
            tilt_on_roof_deg: RACK_TILT,
            azimuth_offset_deg: offset,
            grid,
            module: None,
            mounting: None,
            layout: ArrayLayoutParams::default(),
        };
        let cells = (0..rows).flat_map(move |r| {
            (0..PAIRS).map(move |c| (r, c, origin_u + f64::from(c) * pair_pitch, row_v(r)))
        });
        b.array(array, &d_dome(), cells);
    }
    // The two halves of one east-west system (ids are assigned by `Builder::array`).
    let (west, east) = (b.project.arrays[0].id, b.project.arrays[1].id);
    b.project.arrays[0].layout.paired_with = Some(east);
    b.project.arrays[1].layout.paired_with = Some(west);
    b.project
}

// --- Statics data of the presets (demo values, not for design) ----------------------------

/// Standing seams every 0.60 m.
const SEAMS: SupportGridSpec = SupportGridSpec {
    kind: SupportGridKind::StandingSeam,
    origin_m: 0.0,
    spacing_m: 0.6,
};

/// A Swiss Plateau order of magnitude: 500 m, q_p0 = 1.1 kN/m², 50 years, CC2.
fn demo_site(terrain: TerrainCategory) -> SiteParams {
    let mut s = SiteParams::new(500.0, 1.1);
    s.terrain = terrain;
    s
}

/// K2 SingleRail on the gable barns. The engine has no normative duopitch c_pe table yet:
/// the preset uses the duopitch coefficients printed by K2 Base for project C (interior and
/// ridge); the other edge zones take the ridge values.
fn single_rail_on_gable() -> MountingSystem {
    let entry = |zone, cp_pressure, cp_suction| CpEntrySpec {
        zone,
        cp_pressure,
        cp_suction,
    };
    MountingSystem {
        cp: CpSpec::Explicit {
            reference: "Stress preset (demo): K2 Base duopitch c_pe of project C, ridge values \
                        on the other edge zones"
                .into(),
            entries: vec![
                entry(WindZone::Interior, 0.2, -0.6),
                entry(WindZone::Ridge, 0.2, -1.0),
                entry(WindZone::Verge, 0.2, -1.0),
                entry(WindZone::Eave, 0.2, -1.0),
                entry(WindZone::EaveCorner, 0.2, -1.0),
            ],
        },
        ..MountingSystem::k2_single_rail()
    }
}

/// K2 D-Dome 6.10 on gravel, coefficients back-fitted from the K2 Base report of reference
/// B (9 m): eave corner and interior as printed, the other edge zones take the eave-corner
/// values.
pub(crate) fn d_dome() -> MountingSystem {
    let k = |cp_east, cp_west| PairCoefficientsSpec {
        cp_east,
        cp_west,
        k_slope: 1.00,
        k_direction: 0.90,
        k_parapet: 1.03,
        k_module_size: 1.03,
        k_row_spacing: 1.01,
        k_building_height: 1.00,
    };
    let corner = |zone| AeroZoneSpec {
        zone,
        uplift: k(-0.028, -0.324),
        sliding: k(0.079, -0.273),
    };
    MountingSystem {
        name: "K2 D-Dome 6.10".into(),
        module_mass_kg: 21.5,
        system_mass_per_module_kg: 2.25,
        loaded_area_m2: 10.0,
        psi0_snow: None,
        cp: CpSpec::Standard,
        rails: None,
        ballast: Some(BallastSpec {
            manufacturer: "K2 Systems".into(),
            product: "D-Dome 6.10".into(),
            document: "Stress preset (demo): back-fitted from a K2 Base report".into(),
            module_tilt_deg: 8.6,
            entries: vec![
                AeroZoneSpec {
                    zone: WindZone::Interior,
                    uplift: k(-0.011, -0.182),
                    sliding: k(0.087, -0.246),
                },
                corner(WindZone::Ridge),
                corner(WindZone::Verge),
                corner(WindZone::Eave),
                corner(WindZone::EaveCorner),
            ],
            pair_own_mass_kg: 47.5,
            block_name: "Concrete block 11 kg".into(),
            block_mass_kg: 11.0,
            membrane: MembraneKind::Gravel,
            friction_coefficient: 0.7,
            friction_source: "K2 Base report".into(),
        }),
        // Concrete blocks are bought locally: no part in the K2 list.
        parts: Vec::new(),
        rail_options: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(p: &Project) {
        p.validate().expect("valid project");
        for m in &p.modules {
            let (a, f) = p.module_context(m).expect("context");
            assert!(
                Project::fits_on_roof(a, f, m.u, m.v),
                "module {} off roof",
                m.id.0
            );
            let (hu, hv) = a.half_footprint();
            // Setback respected.
            assert!(
                m.u - hu >= EDGE_SETBACK_M - 1e-9 && m.u + hu <= f.width_m - EDGE_SETBACK_M + 1e-9
            );
            assert!(
                m.v - hv >= EDGE_SETBACK_M - 1e-9 && m.v + hv <= f.height_m - EDGE_SETBACK_M + 1e-9
            );
        }
        // No two modules of the same roof face overlap.
        for f in &p.roof_faces {
            let mut boxes: Vec<(f64, f64, f64, f64)> = p
                .modules
                .iter()
                .filter_map(|m| {
                    let (a, face) = p.module_context(m)?;
                    (face.id == f.id).then(|| {
                        let (hu, hv) = a.half_footprint();
                        (m.u - hu, m.u + hu, m.v - hv, m.v + hv)
                    })
                })
                .collect();
            boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (i, a) in boxes.iter().enumerate() {
                for b in boxes[i + 1..].iter().take_while(|b| b.0 < a.1 - 1e-9) {
                    let overlap_v = a.2 < b.3 - 1e-9 && b.2 < a.3 - 1e-9;
                    assert!(!overlap_v, "overlap on face {}", f.name);
                }
            }
        }
    }

    #[test]
    fn residential_preset() {
        let p = StressPreset::Residential1k.build();
        check(&p);
        assert_eq!(p.modules.len(), 1008);
        assert_eq!(p.buildings.len(), 3);
        assert_eq!(p.roof_faces.len(), 6);
        assert_eq!(p.arrays.len(), 6);
    }

    #[test]
    fn industrial_preset() {
        let p = StressPreset::Industrial10k.build();
        check(&p);
        assert_eq!(p.modules.len(), 10_080);
        assert_eq!(p.arrays.len(), 2);
        assert_eq!(p.arrays[0].azimuth_offset_deg, 90.0);
        assert_eq!(p.arrays[1].azimuth_offset_deg, -90.0);
    }

    #[test]
    fn string_index_every_18_modules() {
        let p = StressPreset::Residential1k.build();
        assert_eq!(p.modules[17].string_index, 0);
        assert_eq!(p.modules[18].string_index, 1);
    }
}
