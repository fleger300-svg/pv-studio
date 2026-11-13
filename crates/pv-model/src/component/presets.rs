//! Base catalogue: the components every library is seeded with.
//!
//! Ids are fixed (not random) so that every user's library, every project and every `.pvlib`
//! package refer to the same base components. Never change an id; change data by adding a
//! revision (bump `revision`), which the library turns into an update proposal.
//! No prices here: prices only enter a library by import.

use uuid::Uuid;

use super::{
    Component, ComponentSource, ComponentSpec, Descriptions, LengthRange, ModuleDetails,
    ModuleElectrical, ModuleSpec, MountingStructure, MountingSystemSpec, PartKind, PartRuleRef,
    PartSpec, SystemCompatibility, SystemStatics,
};
use crate::ids::ComponentId;
use crate::layout::{ArrayLayoutParams, CrossRailSpec, SupportGridKind};
use crate::mounting::{CpSpec, Driver, RailLayerKind, RailSectionSpec};

/// Fixed id of the base catalogue (UUID version 7 layout, sequence `n`).
const fn seed_id(n: u128) -> ComponentId {
    ComponentId(Uuid::from_u128(
        0x0193_0000_0000_7000_8000_0000_0000_0000 | n,
    ))
}

pub const LR7_54HVH_490M: ComponentId = seed_id(0x0001);
pub const K2_SEAM_CLAMP: ComponentId = seed_id(0x0101);
pub const K2_SINGLE_RAIL_36: ComponentId = seed_id(0x0102);
pub const K2_CLIMBER_36_50: ComponentId = seed_id(0x0103);
pub const K2_CLAMP_MC_25_40: ComponentId = seed_id(0x0104);
pub const K2_CLAMP_EC_25_40: ComponentId = seed_id(0x0105);
pub const K2_SINGLE_RAIL: ComponentId = seed_id(0x0201);
pub const K2_D_DOME: ComponentId = seed_id(0x0202);

const K2: &str = "K2 Systems";

fn component(
    id: ComponentId,
    manufacturer: &str,
    model: &str,
    no: &str,
    label: &str,
    spec: ComponentSpec,
) -> Component {
    Component {
        id,
        revision: 1,
        manufacturer: manufacturer.into(),
        model: model.into(),
        article_no: no.into(),
        label: label.into(),
        descriptions: Descriptions::default(),
        source: ComponentSource::Manual,
        verified: true,
        datasheet: None,
        spec,
    }
}

fn part(id: ComponentId, no: &str, label: &str, mass: f64, pack: u32, kind: PartKind) -> Component {
    let spec = ComponentSpec::MountingPart(PartSpec {
        unit_mass_kg: mass,
        pack_qty: pack,
        kind,
    });
    component(id, K2, label, no, label, spec)
}

/// LONGi Hi-MO 7 LR7-54HVH-490M (datasheet values, STC), the module of project A.
#[must_use]
pub fn lr7_54hvh_490m() -> Component {
    component(
        LR7_54HVH_490M,
        "LONGi",
        "LR7-54HVH-490M",
        "",
        "LONGi LR7-54HVH-490M",
        ComponentSpec::Module(ModuleSpec {
            length_m: 1.8,
            width_m: 1.134,
            thickness_m: 0.030,
            mass_kg: 21.6,
            electrical: Some(ModuleElectrical {
                pmax_w: 490.0,
                voc_v: 40.52,
                isc_a: 15.33,
                vmpp_v: 33.51,
                impp_a: 14.63,
                temp_coeff_pmax_pct_per_k: -0.26,
                temp_coeff_voc_pct_per_k: -0.20,
                temp_coeff_isc_pct_per_k: 0.05,
                max_system_voltage_v: 1500.0,
                bifaciality: None,
            }),
            details: Box::new(ModuleDetails {
                series: Some("Hi-MO X10".into()),
                bifacial: Some(false),
                nominal_operating_temp_c: Some(45.0),
                max_series_fuse_a: Some(25.0),
                warranty_product_years: Some(15.0),
                ..ModuleDetails::default()
            }),
        }),
    )
}

/// Parts of K2 SingleRail on standing seam (K2 Base material list of project A).
#[must_use]
pub fn k2_single_rail_parts() -> Vec<Component> {
    let clamps = LengthRange {
        min_m: 0.025,
        max_m: 0.040,
    };
    vec![
        part(
            K2_SEAM_CLAMP,
            "2003024",
            "Single Standing SeamClamp CF:x",
            0.145,
            50,
            PartKind::Fixation { capacity: None },
        ),
        part(
            K2_SINGLE_RAIL_36,
            "2004393",
            "SingleRail 36; 4.80 m",
            3.692,
            20,
            PartKind::Rail {
                stock_length_m: 4.8,
                section: RailSectionSpec::k2_single_rail_36(),
            },
        ),
        part(
            K2_CLIMBER_36_50,
            "2003145",
            "SingleRail Climber Set 36/50",
            0.067,
            10,
            PartKind::Climber,
        ),
        part(
            K2_CLAMP_MC_25_40,
            "2004148",
            "K2 Clamp MC 25-40 Black",
            0.0653,
            20,
            PartKind::Clamp {
                frame_height: clamps,
            },
        ),
        part(
            K2_CLAMP_EC_25_40,
            "2004545",
            "K2 Clamp EC 25-40 Black",
            0.0717,
            20,
            PartKind::Clamp {
                frame_height: clamps,
            },
        ),
    ]
}

/// K2 SingleRail cross system (2 × SingleRail 36, standard coefficients, standing-seam clamps
/// not checked): resolves to [`crate::MountingSystem::k2_single_rail`].
#[must_use]
pub fn k2_single_rail() -> Component {
    let bars = |layer| Driver::RailBars {
        layer,
        stock_length_m: 4.8,
        kerf_m: 0.0,
    };
    let rule = |part, driver| PartRuleRef {
        part,
        driver,
        per: 1,
    };
    component(
        K2_SINGLE_RAIL,
        K2,
        "SingleRail",
        "",
        "K2 SingleRail",
        ComponentSpec::MountingSystem(Box::new(MountingSystemSpec {
            structure: MountingStructure::CrossRail {
                top_rail: K2_SINGLE_RAIL_36,
                bottom_rail: K2_SINGLE_RAIL_36,
                checked_fixation: None,
            },
            defaults: ArrayLayoutParams {
                rails: Some(CrossRailSpec::k2_single_rail()),
                ..ArrayLayoutParams::default()
            },
            statics: SystemStatics {
                system_mass_per_module_kg: 4.3,
                loaded_area_m2: 10.0,
                psi0_snow: None,
                cp: CpSpec::Standard,
            },
            parts: vec![
                rule(K2_SEAM_CLAMP, Driver::Fixation),
                rule(K2_SINGLE_RAIL_36, bars(RailLayerKind::Top)),
                rule(K2_SINGLE_RAIL_36, bars(RailLayerKind::Bottom)),
                rule(K2_CLIMBER_36_50, Driver::Climber),
                rule(K2_CLAMP_MC_25_40, Driver::TopRailInnerJoint),
                rule(K2_CLAMP_EC_25_40, Driver::TopRailEnd),
            ],
            compatibility: SystemCompatibility {
                supports: vec![SupportGridKind::StandingSeam],
                module_length: None,
                module_width: None,
            },
            approval: None,
        })),
    )
}

/// K2 D-Dome 6.10 east-west ballasted system on gravel (coefficients of the stress preset,
/// back-fitted from a K2 Base report). Concrete blocks are bought locally: no part.
#[must_use]
pub fn k2_d_dome() -> Component {
    let reference = crate::stress::d_dome();
    component(
        K2_D_DOME,
        K2,
        "D-Dome 6.10",
        "",
        "K2 D-Dome 6.10",
        ComponentSpec::MountingSystem(Box::new(MountingSystemSpec {
            structure: MountingStructure::Ballasted {
                ballast: reference.ballast.expect("D-Dome is ballasted"),
            },
            defaults: ArrayLayoutParams::default(),
            statics: SystemStatics {
                system_mass_per_module_kg: reference.system_mass_per_module_kg,
                loaded_area_m2: reference.loaded_area_m2,
                psi0_snow: reference.psi0_snow,
                cp: reference.cp,
            },
            parts: Vec::new(),
            compatibility: SystemCompatibility::default(),
            approval: None,
        })),
    )
}

/// The whole base catalogue, parts before the systems that use them.
#[must_use]
pub fn all() -> Vec<Component> {
    let mut all = vec![lr7_54hvh_490m()];
    all.extend(k2_single_rail_parts());
    all.push(k2_single_rail());
    all.push(k2_d_dome());
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{resolve_mounting, ComponentSnapshot};
    use crate::MountingSystem;

    fn snapshots() -> Vec<ComponentSnapshot> {
        all()
            .into_iter()
            .map(|component| ComponentSnapshot {
                component,
                price: None,
            })
            .collect()
    }

    #[test]
    fn base_catalogue_is_valid_and_unique() {
        let all = all();
        for c in &all {
            c.validate()
                .unwrap_or_else(|e| panic!("{}: {e:?}", c.label));
        }
        let mut ids: Vec<_> = all.iter().map(|c| c.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        assert_eq!(LR7_54HVH_490M.0.get_version_num(), 7);
    }

    #[test]
    fn systems_resolve_to_the_engine_presets() {
        let snaps = snapshots();
        let module = lr7_54hvh_490m();
        let single =
            resolve_mounting(&k2_single_rail(), &module, snaps.as_slice()).expect("single rail");
        assert_eq!(single, MountingSystem::k2_single_rail());
        let dome = resolve_mounting(&k2_d_dome(), &module, snaps.as_slice()).expect("d-dome");
        assert_eq!(
            dome,
            MountingSystem {
                module_mass_kg: 21.6,
                ..crate::stress::d_dome()
            }
        );
    }
}
