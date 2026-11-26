//! Shading of each module over the weather year: the share of its plane-of-array irradiation
//! lost to the far horizon and to the near shading, with the same chain as [`crate::simulate`]
//! (horizon on the irradiance first, then the near shading of the beam, circumsolar and sky
//! diffuse). The irradiance-weighted mean over the modules of an array is the shading booked
//! by the simulation; the electrical effect of partial shading (bypass diodes, strings) is
//! not split per module and is left out.

use pv_model::{ModuleId, ModuleRef, ModuleState, Project, HOURS_PER_YEAR};
use serde::Serialize;
use specta::Type;

use crate::sun::{extraterrestrial, hay_davies, position};
use crate::{diffuse_horizontal, month_of, orientation, shade, NotSimulated};

/// Shading of one module over the year, as shares of its plane-of-array irradiation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModuleShading {
    pub module_id: ModuleId,
    /// Far horizon (relief).
    pub horizon: f64,
    /// Near shading (buildings, roof obstacles, surroundings, module rows).
    pub near: f64,
}

/// Shading of every active module of a project.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShadingMap {
    pub modules: Vec<ModuleShading>,
    /// Means over the modules, weighted by their irradiation.
    pub horizon: f64,
    pub near: f64,
}

/// Running sums of one module [W/m²·h].
#[derive(Clone, Copy, Default)]
struct Sums {
    poa: f64,
    horizon: f64,
    near: f64,
}

/// Shading of each active module of `project` over its weather year.
///
/// # Errors
/// [`NotSimulated`] without weather or without active modules on a roof face.
pub fn module_shading(project: &Project, albedo: f64) -> Result<ShadingMap, NotSimulated> {
    let w = project
        .site
        .weather
        .as_ref()
        .ok_or(NotSimulated::NoWeather)?;
    let (lat, lon) = project
        .site
        .location
        .as_ref()
        .map_or((w.lat_deg, w.lon_deg), |l| (l.lat_deg, l.lon_deg));
    let mut shade = shade::ShadeModel::new(project);

    // Per array: orientation and its modules (id, index in the shade masks).
    struct Array {
        id: pv_model::ArrayId,
        tilt: f64,
        azimuth: f64,
        modules: Vec<(ModuleId, usize)>,
    }
    let mut arrays: Vec<Array> = Vec::new();
    for arr in &project.arrays {
        let Some((tilt, azimuth, _)) = orientation(project, arr) else {
            continue;
        };
        let modules: Vec<(ModuleId, usize)> = project
            .modules
            .iter()
            .filter(|m| m.array_id == arr.id && m.state == ModuleState::Active)
            .filter_map(|m| {
                let r = ModuleRef {
                    array: arr.id,
                    row: m.row,
                    col: m.col,
                };
                Some((m.id, shade.module_index(r)?))
            })
            .collect();
        if !modules.is_empty() {
            arrays.push(Array {
                id: arr.id,
                tilt,
                azimuth,
                modules,
            });
        }
    }
    if arrays.is_empty() {
        return Err(NotSimulated::NoModules);
    }

    let mut sums: Vec<Vec<Sums>> = arrays
        .iter()
        .map(|a| vec![Sums::default(); a.modules.len()])
        .collect();
    for h in 0..HOURS_PER_YEAR {
        let ghi = f64::from(w.ghi_w_m2[h]);
        if ghi <= 0.0 {
            continue;
        }
        let day = (h / 24) as u32 + 1;
        let sun = position(lat, lon, day, (h % 24) as f64 + 0.5);
        let e0 = extraterrestrial(day);
        let dhi = diffuse_horizontal(w, h, ghi, &sun, e0);
        let month = month_of(h);
        let elevation = 90.0 - sun.zenith_deg;
        let bin = (elevation > 0.0).then(|| shade::bin_of(sun.azimuth_deg, elevation));
        let behind = elevation > 0.0 && shade.below_horizon(sun.azimuth_deg, elevation);
        for (a, s) in arrays.iter().zip(&mut sums) {
            let poa = hay_davies(&sun, ghi, dhi, e0, a.tilt, a.azimuth, albedo);
            let gpoa = poa.total();
            if gpoa <= 0.0 {
                continue;
            }
            let iso = poa.sky_diffuse - poa.circumsolar;
            let direct = if behind {
                0.0
            } else {
                poa.beam + poa.circumsolar
            };
            let h_iso = iso * shade.horizon_view(a.id);
            let horizon = gpoa - (direct + h_iso + poa.ground);
            for (&(_, i), m) in a.modules.iter().zip(s.iter_mut()) {
                let lit = match bin {
                    Some(bin) if direct > 0.0 => 1.0 - shade.module_beam_shade(a.id, bin, month, i),
                    _ => 1.0,
                };
                let view = shade.module_diffuse_factor(a.id, i);
                m.poa += gpoa;
                m.horizon += horizon;
                m.near += direct * (1.0 - lit) + h_iso * (1.0 - view);
            }
        }
    }

    let mut modules = Vec::new();
    let (mut poa, mut horizon, mut near) = (0.0, 0.0, 0.0);
    for (a, s) in arrays.iter().zip(&sums) {
        for (&(id, _), m) in a.modules.iter().zip(s) {
            let share = |x: f64| if m.poa > 0.0 { x / m.poa } else { 0.0 };
            modules.push(ModuleShading {
                module_id: id,
                horizon: share(m.horizon),
                near: share(m.near),
            });
            poa += m.poa;
            horizon += m.horizon;
            near += m.near;
        }
    }
    modules.sort_by_key(|m| m.module_id);
    let share = |x: f64| if poa > 0.0 { x / poa } else { 0.0 };
    Ok(ShadingMap {
        modules,
        horizon: share(horizon),
        near: share(near),
    })
}
