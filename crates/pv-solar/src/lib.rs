//! Yield simulation of a project over its typical weather year (hourly).
//!
//! Chain, per hour and per group of modules with one orientation and one inverter:
//! sun position (NOAA) → diffuse/beam split (measured DHI, else Erbs) → plane of array
//! (Hay & Davies, albedo) → incidence angle modifier (ASHRAE on beam, constant on diffuse) →
//! soiling → module power at the cell temperature (Faiman-type `U` value) with a low-light
//! correction → mismatch → DC cables → inverter efficiency and clipping at its AC power.
//! Modules in no string get a generic inverter (efficiency, no clipping).
//!
//! The losses are booked one after the other, so that their sum is the gap between the
//! nominal energy (STC efficiency × plane-of-array irradiation) and the AC energy, as in the
//! PV*SOL energy balance. No near shading yet (M3.2).

pub mod sun;

use std::collections::HashMap;

use pv_model::{ComponentSpec, InverterId, ModuleRef, ModuleState, Project, HOURS_PER_YEAR};
use serde::Serialize;
use specta::Type;

use crate::sun::{erbs_diffuse_fraction, extraterrestrial, hay_davies, incidence_deg, position};

/// Assumptions of the simulation (shown with the results).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SimulationAssumptions {
    pub albedo: f64,
    /// ASHRAE incidence angle modifier coefficient b₀.
    pub iam_b0: f64,
    /// Incidence modifier applied to sky and ground diffuse.
    pub iam_diffuse: f64,
    pub soiling_loss: f64,
    pub mismatch_loss: f64,
    pub dc_cable_loss: f64,
    /// Heat loss factor of modules on a pitched roof / of free-standing rows [W/(m²·K)].
    pub u_roof: f64,
    pub u_free: f64,
    /// Low-light coefficient: relative efficiency `1 + a·ln(G/1000)`.
    pub low_light_a: f64,
    /// Efficiency of the generic inverter (modules in no string).
    pub default_inverter_efficiency: f64,
}

impl Default for SimulationAssumptions {
    fn default() -> Self {
        Self {
            albedo: 0.2,
            iam_b0: 0.05,
            iam_diffuse: 0.95,
            soiling_loss: 0.02,
            mismatch_loss: 0.01,
            dc_cable_loss: 0.01,
            u_roof: 20.0,
            u_free: 29.0,
            low_light_a: 0.02,
            default_inverter_efficiency: 0.96,
        }
    }
}

/// Energy balance [kWh], each loss booked after the previous ones.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EnergyBalance {
    /// STC efficiency × plane-of-array irradiation.
    pub nominal_kwh: f64,
    pub iam_kwh: f64,
    pub soiling_kwh: f64,
    pub low_light_kwh: f64,
    pub temperature_kwh: f64,
    pub mismatch_kwh: f64,
    pub dc_cable_kwh: f64,
    pub inverter_kwh: f64,
    pub clipping_kwh: f64,
}

/// Results of the simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YieldReport {
    pub weather_source: String,
    pub peak_power_kwp: f64,
    /// Global horizontal irradiation [kWh/m²].
    pub ghi_kwh_m2: f64,
    /// Plane-of-array irradiation, weighted by peak power [kWh/m²].
    pub poa_kwh_m2: f64,
    pub ac_energy_kwh: f64,
    /// [kWh/kWp].
    pub specific_yield: f64,
    pub performance_ratio: f64,
    pub monthly_ac_kwh: Vec<f64>,
    pub balance: EnergyBalance,
    pub assumptions: SimulationAssumptions,
}

/// Why a project cannot be simulated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum NotSimulated {
    NoWeather,
    NoModules,
}

struct Group {
    tilt_deg: f64,
    azimuth_deg: f64,
    u: f64,
    modules: f64,
    pmax_w: f64,
    gamma: f64,
    inverter: Option<InverterId>,
}

struct Inverter {
    efficiency: f64,
    ac_max_w: f64,
}

const DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

fn month_of(hour: usize) -> usize {
    let mut day = hour / 24;
    for (m, &d) in DAYS.iter().enumerate() {
        if day < d as usize {
            return m;
        }
        day -= d as usize;
    }
    11
}

/// Simulates the project's AC yield over its weather year.
///
/// # Errors
/// [`NotSimulated`] without weather or without active modules with electrical data.
pub fn simulate(project: &Project, a: &SimulationAssumptions) -> Result<YieldReport, NotSimulated> {
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

    // Inverter of each module position (wiring).
    let mut inverter_of: HashMap<ModuleRef, InverterId> = HashMap::new();
    for s in &project.wiring.strings {
        for m in &s.modules {
            inverter_of.insert(*m, s.inverter);
        }
    }
    let mut inverters: HashMap<InverterId, Inverter> = HashMap::new();
    for unit in &project.wiring.inverters {
        if let Some(ComponentSpec::Inverter(i)) =
            project.component(unit.component).map(|c| &c.component.spec)
        {
            let eta = i
                .euro_efficiency_pct
                .or(i.max_efficiency_pct)
                .map_or(a.default_inverter_efficiency, |p| p / 100.0);
            inverters.insert(
                unit.id,
                Inverter {
                    efficiency: eta,
                    ac_max_w: i.ac.max_active_power_w.unwrap_or(i.ac.nominal_power_w),
                },
            );
        }
    }

    // Groups: (array, inverter) with the array's orientation and module data.
    let mut groups: Vec<Group> = Vec::new();
    let mut index: HashMap<(pv_model::ArrayId, Option<InverterId>), usize> = HashMap::new();
    for arr in &project.arrays {
        let Some(e) = arr
            .module
            .and_then(|id| project.component(id))
            .and_then(|s| match &s.component.spec {
                ComponentSpec::Module(m) => m.electrical,
                _ => None,
            })
        else {
            continue;
        };
        let Some(face) = project.roof_faces.iter().find(|f| f.id == arr.roof_face_id) else {
            continue;
        };
        let flat = face.tilt_deg.abs() < 1e-9;
        let tilt = if flat {
            arr.tilt_on_roof_deg
        } else {
            face.tilt_deg + arr.tilt_on_roof_deg
        };
        let azimuth = (face.azimuth_deg + arr.azimuth_offset_deg).rem_euclid(360.0);
        for m in project
            .modules
            .iter()
            .filter(|m| m.array_id == arr.id && m.state == ModuleState::Active)
        {
            let r = ModuleRef {
                array: arr.id,
                row: m.row,
                col: m.col,
            };
            let inv = inverter_of
                .get(&r)
                .copied()
                .filter(|i| inverters.contains_key(i));
            let k = *index.entry((arr.id, inv)).or_insert_with(|| {
                groups.push(Group {
                    tilt_deg: tilt,
                    azimuth_deg: azimuth,
                    u: if flat { a.u_free } else { a.u_roof },
                    modules: 0.0,
                    pmax_w: e.pmax_w,
                    gamma: e.temp_coeff_pmax_pct_per_k / 100.0,
                    inverter: inv,
                });
                groups.len() - 1
            });
            groups[k].modules += 1.0;
        }
    }
    let peak_w: f64 = groups.iter().map(|g| g.modules * g.pmax_w).sum();
    if peak_w <= 0.0 {
        return Err(NotSimulated::NoModules);
    }

    let mut b = EnergyBalance::default();
    let mut monthly = vec![0.0; 12];
    let mut ghi_sum = 0.0;
    let mut poa_weighted = 0.0;
    let mut dc_by_inverter: HashMap<Option<InverterId>, f64> = HashMap::new();
    for h in 0..HOURS_PER_YEAR {
        let ghi = f64::from(w.ghi_w_m2[h]);
        let temp = f64::from(w.temp_c[h]);
        ghi_sum += ghi;
        if ghi <= 0.0 {
            continue;
        }
        let day = (h / 24) as u32 + 1;
        let sun = position(lat, lon, day, (h % 24) as f64 + 0.5);
        let cos_z = sun.zenith_deg.to_radians().cos();
        let e0 = extraterrestrial(day);
        let dhi = if w.dhi_w_m2.is_empty() {
            let kt = if cos_z > 0.0174 {
                (ghi / (e0 * cos_z)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            ghi * erbs_diffuse_fraction(kt)
        } else {
            f64::from(w.dhi_w_m2[h]).min(ghi)
        };
        dc_by_inverter.clear();
        for g in &groups {
            let poa = hay_davies(&sun, ghi, dhi, e0, g.tilt_deg, g.azimuth_deg, a.albedo);
            let gpoa = poa.total();
            if gpoa <= 0.0 {
                continue;
            }
            let p_stc = g.modules * g.pmax_w;
            poa_weighted += gpoa * p_stc;
            let nominal = p_stc * gpoa / 1000.0;
            // Incidence angle modifier.
            let theta = incidence_deg(&sun, g.tilt_deg, g.azimuth_deg).to_radians();
            let iam_beam = if theta.cos() > 0.05 {
                (1.0 - a.iam_b0 * (1.0 / theta.cos() - 1.0)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let geff = poa.beam * iam_beam + (poa.sky_diffuse + poa.ground) * a.iam_diffuse;
            let after_iam = p_stc * geff / 1000.0;
            let g_soiled = geff * (1.0 - a.soiling_loss);
            let after_soiling = p_stc * g_soiled / 1000.0;
            let low_light = if g_soiled > 1.0 {
                (1.0 + a.low_light_a * (g_soiled / 1000.0).ln()).max(0.0)
            } else {
                0.0
            };
            let after_low_light = after_soiling * low_light;
            let t_cell = temp + gpoa * 0.9 * (1.0 - 0.2) / g.u;
            let after_temp = after_low_light * (1.0 + g.gamma * (t_cell - 25.0)).max(0.0);
            let after_mismatch = after_temp * (1.0 - a.mismatch_loss);
            let dc = after_mismatch * (1.0 - a.dc_cable_loss);
            b.nominal_kwh += nominal;
            b.iam_kwh += nominal - after_iam;
            b.soiling_kwh += after_iam - after_soiling;
            b.low_light_kwh += after_soiling - after_low_light;
            b.temperature_kwh += after_low_light - after_temp;
            b.mismatch_kwh += after_temp - after_mismatch;
            b.dc_cable_kwh += after_mismatch - dc;
            *dc_by_inverter.entry(g.inverter).or_default() += dc;
        }
        let month = month_of(h);
        for (inv, &dc) in &dc_by_inverter {
            let (eta, ac_max) = inv
                .and_then(|i| inverters.get(&i))
                .map_or((a.default_inverter_efficiency, f64::INFINITY), |i| {
                    (i.efficiency, i.ac_max_w)
                });
            let ac_raw = dc * eta;
            let ac = ac_raw.min(ac_max);
            b.inverter_kwh += dc - ac_raw;
            b.clipping_kwh += ac_raw - ac;
            monthly[month] += ac;
        }
    }
    // W·h → kWh.
    let k = 1.0 / 1000.0;
    for x in [
        &mut b.nominal_kwh,
        &mut b.iam_kwh,
        &mut b.soiling_kwh,
        &mut b.low_light_kwh,
        &mut b.temperature_kwh,
        &mut b.mismatch_kwh,
        &mut b.dc_cable_kwh,
        &mut b.inverter_kwh,
        &mut b.clipping_kwh,
    ] {
        *x *= k;
    }
    for m in &mut monthly {
        *m *= k;
    }
    let ac: f64 = monthly.iter().sum();
    let kwp = peak_w / 1000.0;
    Ok(YieldReport {
        weather_source: w.source.clone(),
        peak_power_kwp: kwp,
        ghi_kwh_m2: ghi_sum / 1000.0,
        poa_kwh_m2: poa_weighted / peak_w / 1000.0,
        ac_energy_kwh: ac,
        specific_yield: ac / kwp,
        performance_ratio: if b.nominal_kwh > 0.0 {
            ac / b.nominal_kwh
        } else {
            0.0
        },
        monthly_ac_kwh: monthly,
        balance: b,
        assumptions: *a,
    })
}
