//! A field's mounting system, snapshotted into the project: statics data and the parts it is
//! made of (articles and the rules giving their quantities).
//!
//! Architecture: components are copied into the project (« component_snapshot ») so that a
//! project recomputes the same way after the library changes. Until the component library
//! exists, the snapshot is filled from presets ([`MountingSystem::k2_single_rail`]); the
//! library will produce the same snapshots. Plain persisted data, mapped onto the
//! `pv-statics` types by `pv-pipeline`.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::article::Article;

/// Wind zone of a roof face (SN EN 1991-1-4, K2 Base nomenclature).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "camelCase")]
pub enum WindZone {
    Interior,
    Ridge,
    Verge,
    Eave,
    EaveCorner,
}

/// Cross-section and material of a rail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RailSectionSpec {
    pub name: String,
    pub area_cm2: f64,
    pub iy_cm4: f64,
    pub iz_cm4: f64,
    pub wy_cm3: f64,
    pub wz_cm3: f64,
    /// 0.2 % proof strength f₀ [MPa].
    pub f0_mpa: f64,
    pub gamma_m1: f64,
    pub e_mpa: f64,
    /// Admissible deflection under the characteristic combination [m].
    pub deflection_limit_m: f64,
}

impl RailSectionSpec {
    /// K2 SingleRail 36 (EN AW-6063 T66), as printed by K2 Base.
    #[must_use]
    pub fn k2_single_rail_36() -> Self {
        Self {
            name: "K2 SingleRail 36".into(),
            area_cm2: 2.850,
            iy_cm4: 4.02,
            iz_cm4: 6.37,
            wy_cm3: 2.14,
            wz_cm3: 3.09,
            f0_mpa: 200.0,
            gamma_m1: 1.1,
            e_mpa: 70_000.0,
            deflection_limit_m: 0.011_925,
        }
    }
}

/// Design resistance of one roof fixation [kN].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FixationSpec {
    pub name: String,
    pub uplift_kn: f64,
    pub pressure_kn: f64,
    pub parallel_kn: f64,
}

/// Rails and fixations of a cross-rail system (the rail *layout* rules are in
/// [`crate::CrossRailSpec`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RailStatics {
    pub top_rail: RailSectionSpec,
    pub bottom_rail: RailSectionSpec,
    /// `None`: fixations not checked (K2 standing-seam clamps: one on every seam).
    #[specta(optional)]
    pub fixation: Option<FixationSpec>,
}

/// Explicit pressure coefficients of one zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CpEntrySpec {
    pub zone: WindZone,
    pub cp_pressure: f64,
    pub cp_suction: f64,
}

/// Where the pressure coefficients come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum CpSpec {
    /// SN EN 1991-1-4 tables (by roof shape and pitch).
    Standard,
    /// Manufacturer wind-tunnel table.
    Manufacturer {
        manufacturer: String,
        product: String,
        document: String,
        pitch_min_deg: f64,
        pitch_max_deg: f64,
        entries: Vec<CpEntrySpec>,
    },
    /// Fixed by the engineer.
    Explicit {
        reference: String,
        entries: Vec<CpEntrySpec>,
    },
}

/// Wind-tunnel coefficients of an east-west module pair for one check.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PairCoefficientsSpec {
    pub cp_east: f64,
    pub cp_west: f64,
    pub k_slope: f64,
    pub k_direction: f64,
    pub k_parapet: f64,
    pub k_module_size: f64,
    pub k_row_spacing: f64,
    pub k_building_height: f64,
}

/// Coefficients of one zone: uplift check and sliding check.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AeroZoneSpec {
    pub zone: WindZone,
    pub uplift: PairCoefficientsSpec,
    pub sliding: PairCoefficientsSpec,
}

/// Roof membrane under a ballasted system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MembraneKind {
    Epdm,
    Bitumen,
    Pvc,
    Tpo,
    Gravel,
    Green,
}

/// Ballast data of an aerodynamic (east-west) system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BallastSpec {
    pub manufacturer: String,
    pub product: String,
    /// Wind-tunnel report, or the report the values were back-fitted from.
    pub document: String,
    pub module_tilt_deg: f64,
    pub entries: Vec<AeroZoneSpec>,
    /// Two modules and their mounting system, without ballast [kg].
    pub pair_own_mass_kg: f64,
    pub block_name: String,
    pub block_mass_kg: f64,
    pub membrane: MembraneKind,
    /// Design friction coefficient µ between system and roof [-].
    pub friction_coefficient: f64,
    pub friction_source: String,
}

/// Snapshot of a field's mounting system: statics data and parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MountingSystem {
    /// Product name shown in reports.
    pub name: String,
    /// Mass of one module [kg] (module datasheet).
    pub module_mass_kg: f64,
    /// Mounting system mass per module, without ballast [kg].
    pub system_mass_per_module_kg: f64,
    /// Loaded area for the pressure coefficients [m²] (K2 sizes the system with 10 m²).
    pub loaded_area_m2: f64,
    /// Combination factor ψ₀ for snow, when the manufacturer's tool deviates from SIA 260.
    #[specta(optional)]
    pub psi0_snow: Option<f64>,
    pub cp: CpSpec,
    /// Cross-rail system (coplanar fields).
    #[specta(optional)]
    pub rails: Option<RailStatics>,
    /// Aerodynamic ballasted system (east-west on flat roofs).
    #[specta(optional)]
    pub ballast: Option<BallastSpec>,
    /// Parts of the system and the rules giving their quantities (bill of materials).
    pub parts: Vec<PartRule>,
}

impl MountingSystem {
    /// K2 SingleRail cross system with 2 × SingleRail 36, standard coefficients, standing
    /// seam clamps not checked; masses of project A (module 21.6 kg, system 4.3 kg/module).
    #[must_use]
    pub fn k2_single_rail() -> Self {
        Self {
            name: "K2 SingleRail".into(),
            module_mass_kg: 21.6,
            system_mass_per_module_kg: 4.3,
            loaded_area_m2: 10.0,
            psi0_snow: None,
            cp: CpSpec::Standard,
            rails: Some(RailStatics {
                top_rail: RailSectionSpec::k2_single_rail_36(),
                bottom_rail: RailSectionSpec::k2_single_rail_36(),
                fixation: None,
            }),
            ballast: None,
            parts: k2_single_rail_parts(),
        }
    }
}

/// Layer of a cross-rail system: rails of different layers are never cut from the same bar
/// (K2 Base cuts top and bottom rails separately).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "camelCase")]
pub enum RailLayerKind {
    Top,
    Bottom,
}

/// What a part's quantity is counted from (closed set: the bill-of-materials engine knows how
/// to count each of them on a laid-out field).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Driver {
    /// One per roof fixation (standing-seam clamp, hanger bolt…).
    Fixation,
    /// One per crossing of a top and a bottom rail.
    Climber,
    /// Two per top rail (end clamps).
    TopRailEnd,
    /// Modules − 1 per top rail (middle clamps).
    TopRailInnerJoint,
    /// One per active module.
    Module,
    /// One per ballast block.
    BallastBlock,
    /// Stock bars the rail pieces of `layer` are cut from.
    RailBars {
        layer: RailLayerKind,
        stock_length_m: f64,
        /// Saw cut lost between two pieces of a bar [m].
        kerf_m: f64,
    },
}

/// One part of a system: an article, counted by `driver`, `per` pieces each time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PartRule {
    pub article: Article,
    pub driver: Driver,
    /// Pieces per counted item (ignored for rail bars).
    pub per: u32,
}

/// Parts of the K2 SingleRail cross system on standing seam, as in the K2 Base material list.
/// List prices are left at 0: prices come from the component library, not from code.
fn k2_single_rail_parts() -> Vec<PartRule> {
    let article = |no: &str, label: &str, mass: f64, pack: u32| Article {
        article_no: no.into(),
        manufacturer: "K2 Systems".into(),
        label: label.into(),
        descriptions: std::collections::BTreeMap::new(),
        unit_mass_kg: mass,
        pack_qty: pack,
        list_price: 0.0,
        currency: "CHF".into(),
    };
    let rule = |article, driver| PartRule {
        article,
        driver,
        per: 1,
    };
    let rail = || article("2004393", "SingleRail 36; 4.80 m", 3.692, 20);
    let bars = |layer| Driver::RailBars {
        layer,
        stock_length_m: 4.8,
        kerf_m: 0.0,
    };
    vec![
        rule(
            article("2003024", "Single Standing SeamClamp CF:x", 0.145, 50),
            Driver::Fixation,
        ),
        rule(rail(), bars(RailLayerKind::Top)),
        rule(rail(), bars(RailLayerKind::Bottom)),
        rule(
            article("2003145", "SingleRail Climber Set 36/50", 0.067, 10),
            Driver::Climber,
        ),
        rule(
            article("2004148", "K2 Clamp MC 25-40 Black", 0.0653, 20),
            Driver::TopRailInnerJoint,
        ),
        rule(
            article("2004545", "K2 Clamp EC 25-40 Black", 0.0717, 20),
            Driver::TopRailEnd,
        ),
    ]
}
