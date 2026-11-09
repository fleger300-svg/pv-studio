//! Statics data of a field's mounting system: a snapshot frozen into the project.
//!
//! Architecture: components are copied into the project (« component_snapshot ») so that a
//! project recomputes the same way after the library changes. Until the component library
//! exists, the snapshot is filled from presets ([`MountingStatics::k2_single_rail`]); the
//! library will produce the same snapshots. Plain persisted data, mapped onto the
//! `pv-statics` types by `pv-pipeline`.

use serde::{Deserialize, Serialize};
use specta::Type;

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

/// Statics snapshot of a field's mounting system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MountingStatics {
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
}

impl MountingStatics {
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
        }
    }
}
