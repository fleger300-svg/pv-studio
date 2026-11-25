//! Small JSON projection of a project for the front-end (modules travel as binary).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::geometry::RoofFrame;
use crate::ids::{BuildingId, ProjectId};
use crate::project::{
    Building, PanelArray, Project, RoofFace, MODULES_PER_STRING, MODULE_PEAK_POWER_WP,
    MODULE_THICKNESS_M,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoofFaceSummary {
    pub face: RoofFace,
    /// Precomputed orthonormal frame, so the front-end never re-derives roof geometry.
    pub frame: RoofFrame,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArraySummary {
    pub array: PanelArray,
    /// Index into `ProjectSummary::roof_faces`.
    pub roof_face_index: u32,
    /// Modules of this array have the contiguous ids `first_module_id .. first_module_id + module_count`.
    pub first_module_id: u32,
    pub module_count: u32,
    /// Module box size in its local frame (x, y, z), metres.
    pub module_size: [f64; 3],
    /// Half extents of the footprint on the roof plane along (u, v), metres.
    pub half_footprint: [f64; 2],
    /// Label of the module component (`None`: generic module).
    #[specta(optional)]
    pub module_label: Option<String>,
    /// Label of the mounting system component (`None`: none chosen).
    #[specta(optional)]
    pub mounting_label: Option<String>,
    /// Bar lengths the field can buy its rails in (the system's own first); empty without
    /// rails.
    pub rail_lengths_m: Vec<f64>,
}

/// One wall of a building: vertical, on footprint edge `start → end` (world plan, m), from
/// the ground up to the roof.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WallSummary {
    pub building_id: BuildingId,
    pub start: [f64; 2],
    pub end: [f64; 2],
    /// World height of the wall's foot, m.
    pub base_z: f64,
    /// Top of the wall, `(s, z)`: distance from `start` along the edge and world height,
    /// from `s = 0` to `s = length`.
    pub profile: Vec<[f64; 2]>,
}

/// Key figures of the project's weather year.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WeatherSummary {
    pub source: String,
    /// Annual global horizontal irradiation [kWh/m²].
    pub annual_ghi_kwh_m2: f64,
    pub mean_temp_c: f64,
    /// Measured diffuse irradiance available.
    pub has_diffuse: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub project_id: ProjectId,
    pub name: String,
    pub path: String,
    pub module_count: u32,
    pub active_module_count: u32,
    pub modules_per_string: u32,
    pub module_thickness_m: f64,
    /// Peak power of one module, Wp.
    pub module_peak_power_wp: f64,
    pub buildings: Vec<Building>,
    pub roof_faces: Vec<RoofFaceSummary>,
    pub arrays: Vec<ArraySummary>,
    /// Walls of every building (filled by the application with `pv_roof::walls`).
    pub walls: Vec<WallSummary>,
    /// Inverters, strings and electrical settings.
    pub wiring: crate::wiring::Wiring,
    #[specta(optional)]
    pub location: Option<crate::weather::GeoLocation>,
    /// The weather year, without its hourly values.
    #[specta(optional)]
    pub weather: Option<WeatherSummary>,
    /// Shading objects around the buildings (trees, neighbouring buildings, roof fixtures),
    /// for the 3-D view.
    pub objects: Vec<crate::weather::ShadingObject>,
    pub can_undo: bool,
    pub can_redo: bool,
}

impl ProjectSummary {
    /// Builds the summary. Assumes `project.validate()` succeeded (contiguous arrays).
    #[must_use]
    pub fn new(project: &Project, path: String, can_undo: bool, can_redo: bool) -> Self {
        let roof_faces = project
            .roof_faces
            .iter()
            .map(|f| RoofFaceSummary {
                face: f.clone(),
                frame: f.frame(),
            })
            .collect();
        let arrays = project
            .arrays
            .iter()
            .map(|a| {
                let mut ids = project
                    .modules
                    .iter()
                    .filter(|m| m.array_id == a.id)
                    .map(|m| m.id.0);
                let (first, count) = match ids.next() {
                    Some(first) => (first, 1 + ids.count() as u32),
                    None => (0, 0),
                };
                let (hu, hv) = a.half_footprint();
                let label = |id: Option<crate::ids::ComponentId>| {
                    id.and_then(|id| project.component(id))
                        .map(|s| s.component.label.clone())
                };
                ArraySummary {
                    array: a.clone(),
                    roof_face_index: project
                        .roof_faces
                        .iter()
                        .position(|f| f.id == a.roof_face_id)
                        .unwrap_or(0) as u32,
                    first_module_id: first,
                    module_count: count,
                    module_size: a.local_size(),
                    half_footprint: [hu, hv],
                    module_label: label(a.module),
                    mounting_label: label(a.mounting),
                    rail_lengths_m: project
                        .resolve_mounting(a)
                        .ok()
                        .flatten()
                        .map(|m| m.rail_lengths())
                        .unwrap_or_default(),
                }
            })
            .collect();
        Self {
            project_id: project.id,
            name: project.name.clone(),
            path,
            module_count: project.modules.len() as u32,
            active_module_count: project.active_count() as u32,
            modules_per_string: MODULES_PER_STRING,
            module_thickness_m: MODULE_THICKNESS_M,
            module_peak_power_wp: MODULE_PEAK_POWER_WP,
            buildings: project.buildings.clone(),
            roof_faces,
            arrays,
            walls: Vec::new(),
            wiring: project.wiring.clone(),
            location: project.site.location.clone(),
            weather: project.site.weather.as_ref().map(|w| WeatherSummary {
                source: w.source.clone(),
                annual_ghi_kwh_m2: w.annual_ghi_kwh_m2(),
                mean_temp_c: w.temp_c.iter().map(|&t| f64::from(t)).sum::<f64>()
                    / w.temp_c.len().max(1) as f64,
                has_diffuse: !w.dhi_w_m2.is_empty(),
            }),
            objects: project.site.objects.clone(),
            can_undo,
            can_redo,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::fixtures::small_project;

    #[test]
    fn summary_ranges() {
        let p = small_project();
        let s = ProjectSummary::new(&p, "x.pvproj".into(), false, false);
        assert_eq!(s.module_count, 6);
        assert_eq!(s.arrays[0].first_module_id, 0);
        assert_eq!(s.arrays[0].module_count, 6);
        assert_eq!(s.roof_faces[0].frame, p.roof_faces[0].frame());
    }
}
