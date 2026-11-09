//! Persisted data (`pv-model`) → statics inputs (`pv-statics`).
//!
//! The persisted types mirror the engine's on purpose: the file format stays stable when the
//! engine evolves, and every mapping is written out here, in one place.

use pv_model::{
    Building, ConsequenceClass, CpEntrySpec, CpSpec, FixationSpec, MembraneKind, MountingSystem,
    PairCoefficientsSpec, PanelArray, RailSectionSpec, RoofFace, RoofKind, ServiceLifeModel,
    SiteParams, SnowExposure, SupportGridKind, TerrainCategory, WindZone,
};
use pv_statics::ballast::{
    AeroBallastTable, AeroPair, AeroZoneEntry, BallastProduct, PairCoefficients, RoofMembrane,
    RoofSurface,
};
use pv_statics::descent::{FixationResistance, RailSection, SupportGrid};
use pv_statics::{
    CpEntry, CpSource, DeadLoad, ExplicitCpTable, ManufacturerCpTable, PartialFactors, RoofInput,
    RoofShape, RoofZone, SiteInput,
};

/// Below this pitch a face is a flat roof for the pressure coefficients (EN 1991-1-4 §7.2.3).
pub const FLAT_ROOF_MAX_PITCH_DEG: f64 = 5.0;

#[must_use]
pub fn roof_zone(z: WindZone) -> RoofZone {
    match z {
        WindZone::Interior => RoofZone::Interior,
        WindZone::Ridge => RoofZone::Ridge,
        WindZone::Verge => RoofZone::Verge,
        WindZone::Eave => RoofZone::Eave,
        WindZone::EaveCorner => RoofZone::EaveCorner,
    }
}

#[must_use]
pub fn wind_zone(z: RoofZone) -> WindZone {
    match z {
        RoofZone::Interior => WindZone::Interior,
        RoofZone::Ridge => WindZone::Ridge,
        RoofZone::Verge => WindZone::Verge,
        RoofZone::Eave => WindZone::Eave,
        RoofZone::EaveCorner => WindZone::EaveCorner,
    }
}

/// Site of a building; the wind reference height is the ridge height.
#[must_use]
pub fn site_input(site: &SiteParams, building: &Building) -> SiteInput {
    SiteInput {
        altitude_m: site.altitude_m,
        snow_altitude_correction_m: site.snow_altitude_correction_m,
        wind_reference_pressure_kn_m2: site.wind_reference_pressure_kn_m2,
        terrain: match site.terrain {
            TerrainCategory::II => pv_statics::TerrainCategory::II,
            TerrainCategory::IIa => pv_statics::TerrainCategory::IIa,
            TerrainCategory::III => pv_statics::TerrainCategory::III,
            TerrainCategory::IV => pv_statics::TerrainCategory::IV,
        },
        building_height_m: building.ridge_height_m,
        snow_exposure: match site.snow_exposure {
            SnowExposure::Windswept => pv_statics::SnowExposure::Windswept,
            SnowExposure::Normal => pv_statics::SnowExposure::Normal,
            SnowExposure::Sheltered => pv_statics::SnowExposure::Sheltered,
        },
        thermal_coefficient: site.thermal_coefficient,
        service_life_years: site.service_life_years,
        consequence_class: consequence_class(site.consequence_class),
        service_life_model: match site.service_life_model {
            ServiceLifeModel::En1991 => pv_statics::ServiceLifeModel::En1991,
            ServiceLifeModel::Explicit { snow, wind } => {
                pv_statics::ServiceLifeModel::Explicit { snow, wind }
            }
        },
    }
}

#[must_use]
pub fn consequence_class(c: ConsequenceClass) -> pv_statics::ConsequenceClass {
    match c {
        ConsequenceClass::CC1 => pv_statics::ConsequenceClass::CC1,
        ConsequenceClass::CC2 => pv_statics::ConsequenceClass::CC2,
        ConsequenceClass::CC3 => pv_statics::ConsequenceClass::CC3,
    }
}

/// Roof shape and pitch of a face for the pressure coefficients.
#[must_use]
pub fn roof_input(building: &Building, face: &RoofFace) -> RoofInput {
    let shape = if face.tilt_deg < FLAT_ROOF_MAX_PITCH_DEG {
        RoofShape::Flat
    } else {
        match building.roof_kind {
            RoofKind::Flat => RoofShape::Flat,
            RoofKind::Gable => RoofShape::Duopitch,
            RoofKind::Hipped => RoofShape::Hipped,
            RoofKind::Monopitch => RoofShape::Monopitch,
        }
    };
    RoofInput {
        shape,
        pitch_deg: face.tilt_deg,
    }
}

fn cp_entry(e: &CpEntrySpec) -> CpEntry {
    CpEntry {
        zone: roof_zone(e.zone),
        cp_pressure: e.cp_pressure,
        cp_suction: e.cp_suction,
    }
}

#[must_use]
pub fn cp_source(cp: &CpSpec) -> CpSource {
    match cp {
        CpSpec::Standard => CpSource::Standard,
        CpSpec::Manufacturer {
            manufacturer,
            product,
            document,
            pitch_min_deg,
            pitch_max_deg,
            entries,
        } => CpSource::Manufacturer(ManufacturerCpTable {
            manufacturer: manufacturer.clone(),
            product: product.clone(),
            document: document.clone(),
            pitch_range_deg: (*pitch_min_deg, *pitch_max_deg),
            entries: entries.iter().map(cp_entry).collect(),
        }),
        CpSpec::Explicit { reference, entries } => CpSource::Explicit(ExplicitCpTable {
            reference: reference.clone(),
            entries: entries.iter().map(cp_entry).collect(),
        }),
    }
}

/// Dead load: module area from the array's module dimensions.
#[must_use]
pub fn dead_load(m: &MountingSystem, array: &PanelArray) -> DeadLoad {
    DeadLoad {
        module_mass_kg: m.module_mass_kg,
        system_mass_per_module_kg: m.system_mass_per_module_kg,
        module_area_m2: array.module_length_m * array.module_width_m,
    }
}

/// SIA 260 factors of the site's consequence class, with the snapshot's ψ₀,S override.
#[must_use]
pub fn factors(site: &SiteInput, m: &MountingSystem) -> PartialFactors {
    let mut f = PartialFactors::sia_260_for(site.consequence_class);
    if let Some(psi) = m.psi0_snow {
        f.psi0_snow = psi;
    }
    f
}

#[must_use]
pub fn rail_section(r: &RailSectionSpec) -> RailSection {
    RailSection {
        name: r.name.clone(),
        area_cm2: r.area_cm2,
        iy_cm4: r.iy_cm4,
        iz_cm4: r.iz_cm4,
        wy_cm3: r.wy_cm3,
        wz_cm3: r.wz_cm3,
        f0_mpa: r.f0_mpa,
        gamma_m1: r.gamma_m1,
        e_mpa: r.e_mpa,
        deflection_limit_m: r.deflection_limit_m,
    }
}

#[must_use]
pub fn fixation(f: &FixationSpec) -> FixationResistance {
    FixationResistance {
        name: f.name.clone(),
        uplift_kn: f.uplift_kn,
        pressure_kn: f.pressure_kn,
        parallel_kn: f.parallel_kn,
    }
}

/// Support grid of the statics (seam or rafter spacing) from the face covering.
#[must_use]
pub fn support_grid(face: &RoofFace) -> Option<SupportGrid> {
    face.support_grid.map(|g| match g.kind {
        SupportGridKind::StandingSeam => SupportGrid::StandingSeam {
            seam_spacing_m: g.spacing_m,
        },
        SupportGridKind::Rafters => SupportGrid::Rafters {
            spacing_m: g.spacing_m,
        },
    })
}

fn pair_coefficients(c: &PairCoefficientsSpec) -> PairCoefficients {
    PairCoefficients {
        cp_east: c.cp_east,
        cp_west: c.cp_west,
        k_slope: c.k_slope,
        k_direction: c.k_direction,
        k_parapet: c.k_parapet,
        k_module_size: c.k_module_size,
        k_row_spacing: c.k_row_spacing,
        k_building_height: c.k_building_height,
    }
}

/// Aerodynamic table, module pair, roof surface and block of a ballasted system.
#[must_use]
pub fn ballast_inputs(
    b: &pv_model::BallastSpec,
    array: &PanelArray,
) -> (AeroBallastTable, AeroPair, RoofSurface, BallastProduct) {
    (
        AeroBallastTable {
            manufacturer: b.manufacturer.clone(),
            product: b.product.clone(),
            document: b.document.clone(),
            module_tilt_deg: b.module_tilt_deg,
            entries: b
                .entries
                .iter()
                .map(|e| AeroZoneEntry {
                    zone: roof_zone(e.zone),
                    uplift: pair_coefficients(&e.uplift),
                    sliding: pair_coefficients(&e.sliding),
                })
                .collect(),
        },
        AeroPair {
            module_area_m2: array.module_length_m * array.module_width_m,
            own_mass_kg: b.pair_own_mass_kg,
        },
        RoofSurface {
            membrane: match b.membrane {
                MembraneKind::Epdm => RoofMembrane::Epdm,
                MembraneKind::Bitumen => RoofMembrane::Bitumen,
                MembraneKind::Pvc => RoofMembrane::Pvc,
                MembraneKind::Tpo => RoofMembrane::Tpo,
                MembraneKind::Gravel => RoofMembrane::Gravel,
                MembraneKind::Green => RoofMembrane::Green,
            },
            friction_coefficient: b.friction_coefficient,
            source: b.friction_source.clone(),
        },
        BallastProduct {
            name: b.block_name.clone(),
            unit_mass_kg: b.block_mass_kg,
        },
    )
}
