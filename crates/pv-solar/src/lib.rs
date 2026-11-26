//! Yield simulation of a project over its typical weather year (hourly).
//!
//! Chain, per hour and per group of modules with one orientation and one inverter:
//! sun position (NOAA) → diffuse/beam split (measured DHI, else Erbs) → plane of array
//! (Hay & Davies, albedo) → incidence angle modifier (ASHRAE on beam, constant on diffuse) →
//! soiling → module power at the cell temperature (Faiman-type `U` value) with a low-light
//! correction → mismatch → electrical mismatch on each tracker ([`parallel`]: partial
//! shading of the strings, unequal strings in parallel) → DC cables → inverter efficiency and
//! clipping at its AC power. Module optimizers are not modelled (computed as without them).
//! Modules in no string get a generic inverter (efficiency, no clipping).
//!
//! The losses are booked one after the other, so that their sum is the gap between the
//! nominal energy (STC efficiency × plane-of-array irradiation) and the AC energy, as in the
//! PV*SOL energy balance. Near shading by the project's own buildings, roof obstacles and
//! tilted module rows ([`shade`]) is booked first, on the plane-of-array irradiance, in
//! proportion to the shaded area; its electrical effect comes with the mismatch.

pub mod module_shading;
pub mod parallel;
pub mod roof_shading;
pub mod shade;
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
            // Calibrated on PV*SOL over the 119 completely imported projects of the corpus
            // (same weather, M10): median reflection gap 0.0 pt, temperature −0.05 pt, low
            // light 0.0 pt (`iam_b0`, `iam_diffuse`, `u_roof`, `low_light_a`).
            iam_b0: 0.03,
            iam_diffuse: 0.97,
            soiling_loss: 0.02,
            mismatch_loss: 0.01,
            dc_cable_loss: 0.01,
            u_roof: 27.5,
            u_free: 29.0,
            low_light_a: 0.0275,
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
    /// Far horizon (relief).
    pub horizon_kwh: f64,
    /// Near shading (buildings, roof obstacles).
    pub shading_kwh: f64,
    pub iam_kwh: f64,
    pub soiling_kwh: f64,
    pub low_light_kwh: f64,
    pub temperature_kwh: f64,
    pub mismatch_kwh: f64,
    /// Electrical mismatch on the trackers beyond the linear shading loss: partially shaded
    /// strings (bypass diodes, current limited by the shaded substrings) and unequal strings
    /// in parallel, held at one voltage.
    pub wiring_mismatch_kwh: f64,
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
    array: pv_model::ArrayId,
    tilt_deg: f64,
    azimuth_deg: f64,
    u: f64,
    modules: f64,
    pmax_w: f64,
    gamma: f64,
    inverter: Option<InverterId>,
    /// Module voltages and currents at STC, `Voc` temperature coefficient [1/K].
    vmpp_v: f64,
    voc_v: f64,
    isc_per_impp: f64,
    beta_voc: f64,
}

/// The modules of one string, as `(group index, module count)`.
type StringGroups = Vec<(usize, f64)>;

/// One group's modules this hour: power per module after the mismatch [W], voltage factor.
#[derive(Clone, Copy, Default)]
struct GroupHour {
    p_module_w: f64,
    voltage_factor: f64,
    /// Power of one module wholly in the sun / wholly in the beam's shade; both 0 when the
    /// group has no shade this hour.
    p_sun_w: f64,
    p_shade_w: f64,
}

/// A string's modules: `(group index, array, index in the array's shade masks)`.
type StringModules = Vec<(usize, pv_model::ArrayId, usize)>;

/// One tracker: its strings as module counts per group (unshaded hours) and as modules
/// (hours with partial shading).
struct Tracker {
    inverter: InverterId,
    counts: Vec<StringGroups>,
    modules: Vec<StringModules>,
}

/// Bits of the two halves of strip `s` in a shade mask (bands 0–1 and 2–3).
fn halves(mask: u16, s: usize) -> (bool, bool) {
    let base = s * shade::BANDS;
    (mask & (0b0011 << base) != 0, mask & (0b1100 << base) != 0)
}

/// Relative change of the module voltages with the effective irradiance `g` [W/m²] (the
/// thermal-voltage term `n·k·T/q·ln(G/1000)` over `Voc`, ≈ 0.04 for crystalline cells) and
/// the cell temperature.
fn voltage_factor(g: f64, t_cell: f64, beta_voc: f64) -> f64 {
    (1.0 + beta_voc * (t_cell - 25.0) + 0.04 * (g.max(10.0) / 1000.0).ln()).max(0.2)
}

/// Loss [W] of a tracker with partially shaded modules this hour, beyond the linear loss of
/// the shaded area already booked: bypass diodes and the string current limited by its
/// shaded substrings, plus unequal strings in parallel. `None` when none of its modules is
/// partly in the beam's shade (the unshaded path applies).
fn partial_shading_loss(
    t: &Tracker,
    bin: (u16, u16),
    month: usize,
    groups: &[Group],
    hour: &[GroupHour],
    shade: &mut shade::ShadeModel,
) -> Option<f64> {
    const SUBSTRINGS: f64 = shade::STRIPS as f64;
    fn count(counts: &mut Vec<((usize, u32), f64)>, key: (usize, u32)) {
        match counts.iter_mut().find(|c| c.0 == key) {
            Some(c) => c.1 += 1.0,
            None => counts.push((key, 1.0)),
        }
    }
    // Substrings by (group, shaded halves; 3: a group without shade this hour).
    let blocks = |counts: &[((usize, u32), f64)]| -> Vec<parallel::Block> {
        counts
            .iter()
            .map(|&((k, n), count)| {
                let gh = hour[k];
                let (p_full, r) = if n >= 3 {
                    (gh.p_module_w, 1.0)
                } else {
                    (gh.p_sun_w, gh.p_shade_w / gh.p_sun_w)
                };
                // Each shaded half of a half-cell substring carries the shaded current.
                let c = 1.0 - f64::from(n % 3) * (1.0 - r) / 2.0;
                parallel::Block {
                    count,
                    point: substring(&groups[k], gh.voltage_factor, p_full, c),
                }
            })
            .collect()
    };
    let mut any = false;
    let mut pmax_of_group: Vec<Option<f64>> = vec![None; groups.len()];
    let (mut p_ref, mut p_clear, mut p_linear) = (0.0, 0.0, 0.0);
    let mut shaded_strings: Vec<Vec<parallel::Block>> = Vec::with_capacity(t.modules.len());
    let mut clear_strings: Vec<Vec<parallel::Block>> = Vec::with_capacity(t.modules.len());
    for s in &t.modules {
        let mut counts: Vec<((usize, u32), f64)> = Vec::new();
        let mut clear: Vec<((usize, u32), f64)> = Vec::new();
        for &(k, array, i) in s {
            let gh = hour[k];
            let lit = gh.p_sun_w > gh.p_shade_w;
            let (p_full, r, mask) = if lit {
                let mask = shade.module_mask(array, bin, month, i);
                (gh.p_sun_w, gh.p_shade_w / gh.p_sun_w, mask)
            } else {
                (gh.p_module_w, 1.0, 0)
            };
            if p_full <= 0.0 {
                continue;
            }
            any |= mask != 0;
            let shaded = f64::from(mask.count_ones()) / shade::POINTS as f64;
            let linear = 1.0 - shaded * (1.0 - r);
            p_linear += p_full * linear;
            let g = &groups[k];
            let pmax = *pmax_of_group[k].get_or_insert_with(|| {
                parallel::block_pmax(&substring(g, gh.voltage_factor, p_full, 1.0)) / p_full
            });
            p_ref += SUBSTRINGS * pmax * p_full * linear;
            p_clear += SUBSTRINGS * pmax * p_full;
            let base = if lit { 0 } else { 3 };
            for strip in 0..shade::STRIPS {
                let (a, b) = halves(mask, strip);
                count(&mut counts, (k, u32::from(a) + u32::from(b) + base));
                count(&mut clear, (k, base));
            }
        }
        shaded_strings.push(blocks(&counts));
        clear_strings.push(blocks(&clear));
    }
    if !any || p_ref <= 0.0 {
        return None;
    }
    // The electrical effect of the shade alone: the tracker's power with the shade over its
    // power without, against the linear expectation. (Strings mixing orientations lose as
    // much in both and are not counted, as in PV*SOL; unequal strings in parallel are
    // counted apart.)
    let clear = parallel::tracker_power(&clear_strings);
    if clear <= 0.0 {
        return None;
    }
    let ratio = parallel::tracker_power(&shaded_strings) / clear / (p_ref / p_clear);
    Some((1.0 - ratio).clamp(0.0, 1.0) * p_linear)
}

/// One bypass-diode substring of a module of group `g` that gives `p_module_w` in full light,
/// carrying the share `c` of its light.
fn substring(g: &Group, voltage_factor: f64, p_module_w: f64, c: f64) -> parallel::StringPoint {
    const SUBSTRINGS: f64 = shade::STRIPS as f64;
    let vmpp = g.vmpp_v * voltage_factor;
    let impp = c * p_module_w / vmpp;
    parallel::StringPoint {
        vmpp_v: vmpp / SUBSTRINGS,
        impp_a: impp,
        voc_v: g.voc_v * voltage_factor / SUBSTRINGS,
        isc_a: impp * g.isc_per_impp,
    }
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

/// Tilt, azimuth of the modules of `arr` and whether they stand on a flat roof; `None` when
/// its roof face is missing.
fn orientation(project: &Project, arr: &pv_model::PanelArray) -> Option<(f64, f64, bool)> {
    let face = project
        .roof_faces
        .iter()
        .find(|f| f.id == arr.roof_face_id)?;
    let flat = face.tilt_deg.abs() < 1e-9;
    let tilt = if flat {
        arr.tilt_on_roof_deg
    } else {
        face.tilt_deg + arr.tilt_on_roof_deg
    };
    let azimuth = (face.azimuth_deg + arr.azimuth_offset_deg).rem_euclid(360.0);
    Some((tilt, azimuth, flat))
}

/// Diffuse horizontal irradiance of hour `h` [W/m²]: measured, else split from `ghi` (Erbs).
fn diffuse_horizontal(
    w: &pv_model::WeatherYear,
    h: usize,
    ghi: f64,
    sun: &sun::SunPosition,
    e0: f64,
) -> f64 {
    if w.dhi_w_m2.is_empty() {
        let cos_z = sun.zenith_deg.to_radians().cos();
        let kt = if cos_z > 0.0174 {
            (ghi / (e0 * cos_z)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        ghi * erbs_diffuse_fraction(kt)
    } else {
        f64::from(w.dhi_w_m2[h]).min(ghi)
    }
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
    let mut group_of: HashMap<ModuleRef, usize> = HashMap::new();
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
        let Some((tilt, azimuth, flat)) = orientation(project, arr) else {
            continue;
        };
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
                    array: arr.id,
                    tilt_deg: tilt,
                    azimuth_deg: azimuth,
                    u: if flat { a.u_free } else { a.u_roof },
                    modules: 0.0,
                    pmax_w: e.pmax_w,
                    gamma: e.temp_coeff_pmax_pct_per_k / 100.0,
                    inverter: inv,
                    vmpp_v: e.vmpp_v,
                    voc_v: e.voc_v,
                    isc_per_impp: e.isc_a / e.impp_a,
                    beta_voc: e.temp_coeff_voc_pct_per_k / 100.0,
                });
                groups.len() - 1
            });
            groups[k].modules += 1.0;
            group_of.insert(r, k);
        }
    }
    let peak_w: f64 = groups.iter().map(|g| g.modules * g.pmax_w).sum();
    if peak_w <= 0.0 {
        return Err(NotSimulated::NoModules);
    }

    let mut shade = shade::ShadeModel::new(project);

    // Trackers: each string as module counts per group and as modules.
    let mut trackers: Vec<Tracker> = Vec::new();
    let mut tracker_of: HashMap<(InverterId, u32), usize> = HashMap::new();
    for s in &project.wiring.strings {
        if !inverters.contains_key(&s.inverter) {
            continue;
        }
        let mut counts: StringGroups = Vec::new();
        let mut modules: StringModules = Vec::new();
        for m in &s.modules {
            let (Some(&k), Some(i)) = (group_of.get(m), shade.module_index(*m)) else {
                continue;
            };
            modules.push((k, m.array, i));
            match counts.iter_mut().find(|c| c.0 == k) {
                Some(c) => c.1 += 1.0,
                None => counts.push((k, 1.0)),
            }
        }
        if counts.is_empty() {
            continue;
        }
        // (In wiring order, so that the sums do not depend on a map's order.)
        let t = *tracker_of
            .entry((s.inverter, s.tracker))
            .or_insert_with(|| {
                trackers.push(Tracker {
                    inverter: s.inverter,
                    counts: Vec::new(),
                    modules: Vec::new(),
                });
                trackers.len() - 1
            });
        trackers[t].counts.push(counts);
        trackers[t].modules.push(modules);
    }
    let mut hour_of_group = vec![GroupHour::default(); groups.len()];
    let mut points: Vec<parallel::StringPoint> = Vec::new();
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
        let e0 = extraterrestrial(day);
        let dhi = diffuse_horizontal(w, h, ghi, &sun, e0);
        let month = month_of(h);
        dc_by_inverter.clear();
        hour_of_group.fill(GroupHour::default());
        for (k, g) in groups.iter().enumerate() {
            let poa = hay_davies(&sun, ghi, dhi, e0, g.tilt_deg, g.azimuth_deg, a.albedo);
            let gpoa = poa.total();
            if gpoa <= 0.0 {
                continue;
            }
            let p_stc = g.modules * g.pmax_w;
            poa_weighted += gpoa * p_stc;
            let nominal = p_stc * gpoa / 1000.0;
            let elevation = 90.0 - sun.zenith_deg;
            let s_beam = if elevation > 0.0 {
                shade.beam_shade(g.array, shade::bin_of(sun.azimuth_deg, elevation), month)
            } else {
                0.0
            };
            let d_view = shade.diffuse_factor(g.array);
            // Far horizon first (on the irradiance), then the near shading.
            let behind = elevation > 0.0 && shade.below_horizon(sun.azimuth_deg, elevation);
            // The circumsolar diffuse comes from around the sun: shaded like the beam.
            let iso = poa.sky_diffuse - poa.circumsolar;
            let (h_beam, h_circ) = if behind {
                (0.0, 0.0)
            } else {
                (poa.beam, poa.circumsolar)
            };
            let h_iso = iso * shade.horizon_view(g.array);
            let after_horizon = p_stc * (h_beam + h_circ + h_iso + poa.ground) / 1000.0;
            // Incidence angle modifier.
            let theta = incidence_deg(&sun, g.tilt_deg, g.azimuth_deg).to_radians();
            let iam_beam = if theta.cos() > 0.05 {
                (1.0 - a.iam_b0 * (1.0 / theta.cos() - 1.0)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let t_cell = temp + gpoa * 0.9 * (1.0 - 0.2) / g.u;
            // The chain from the shading on, for a share `lit` of the beam and circumsolar.
            let chain = |lit: f64| {
                let beam = h_beam * lit;
                // (The circumsolar part keeps the diffuse incidence modifier, as calibrated.)
                let sky = h_circ * lit + h_iso * d_view;
                let after_shading = p_stc * (beam + sky + poa.ground) / 1000.0;
                let geff = beam * iam_beam + (sky + poa.ground) * a.iam_diffuse;
                let after_iam = p_stc * geff / 1000.0;
                let g_soiled = geff * (1.0 - a.soiling_loss);
                let after_soiling = p_stc * g_soiled / 1000.0;
                let low_light = if g_soiled > 1.0 {
                    (1.0 + a.low_light_a * (g_soiled / 1000.0).ln()).max(0.0)
                } else {
                    0.0
                };
                let after_low_light = after_soiling * low_light;
                let after_temp = after_low_light * (1.0 + g.gamma * (t_cell - 25.0)).max(0.0);
                let after_mismatch = after_temp * (1.0 - a.mismatch_loss);
                [
                    after_shading,
                    after_iam,
                    after_soiling,
                    after_low_light,
                    after_temp,
                    after_mismatch,
                    g_soiled,
                ]
            };
            let [after_shading, after_iam, after_soiling, after_low_light, after_temp, after_mismatch, g_soiled] =
                chain(1.0 - s_beam);
            let sun_lit = h_beam + h_circ > 0.0 && s_beam > 0.0;
            hour_of_group[k] = GroupHour {
                p_module_w: after_mismatch / g.modules,
                voltage_factor: voltage_factor(g_soiled, t_cell, g.beta_voc),
                // A module in full sun / in the shade (for the partial shading).
                p_sun_w: if sun_lit {
                    chain(1.0)[5] / g.modules
                } else {
                    0.0
                },
                p_shade_w: if sun_lit {
                    chain(0.0)[5] / g.modules
                } else {
                    0.0
                },
            };
            b.nominal_kwh += nominal;
            b.horizon_kwh += nominal - after_horizon;
            b.shading_kwh += after_horizon - after_shading;
            b.iam_kwh += after_shading - after_iam;
            b.soiling_kwh += after_iam - after_soiling;
            b.low_light_kwh += after_soiling - after_low_light;
            b.temperature_kwh += after_low_light - after_temp;
            b.mismatch_kwh += after_temp - after_mismatch;
            *dc_by_inverter.entry(g.inverter).or_default() += after_mismatch;
        }
        // Electrical mismatch on each tracker: partial shading inside its strings, unequal
        // strings in parallel.
        let elevation = 90.0 - sun.zenith_deg;
        let bin = (elevation > 0.0).then(|| shade::bin_of(sun.azimuth_deg, elevation));
        for t in &trackers {
            let shaded = bin.and_then(|bin| {
                partial_shading_loss(t, bin, month, &groups, &hour_of_group, &mut shade)
            });
            if let Some(lost) = shaded.filter(|l| *l > 0.0) {
                b.wiring_mismatch_kwh += lost;
                *dc_by_inverter.entry(Some(t.inverter)).or_default() -= lost;
            }
            if t.counts.len() < 2 {
                continue;
            }
            points.clear();
            let mut power = 0.0;
            for s in &t.counts {
                let (mut p, mut vmpp, mut voc, mut isc_per_impp) = (0.0, 0.0, 0.0, 0.0);
                for &(k, n) in s {
                    let (g, gh) = (&groups[k], hour_of_group[k]);
                    p += n * gh.p_module_w;
                    vmpp += n * g.vmpp_v * gh.voltage_factor;
                    voc += n * g.voc_v * gh.voltage_factor;
                    isc_per_impp += n * gh.p_module_w * g.isc_per_impp;
                }
                if p > 0.0 && vmpp > 0.0 {
                    let impp = p / vmpp;
                    points.push(parallel::StringPoint {
                        vmpp_v: vmpp,
                        impp_a: impp,
                        voc_v: voc,
                        isc_a: impp * isc_per_impp / p,
                    });
                    power += p;
                }
            }
            let lost = power * parallel::loss_fraction(&points);
            if lost > 0.0 {
                b.wiring_mismatch_kwh += lost;
                *dc_by_inverter.entry(Some(t.inverter)).or_default() -= lost;
            }
        }
        for (inv, &before_cables) in &dc_by_inverter {
            let dc = before_cables * (1.0 - a.dc_cable_loss);
            b.dc_cable_kwh += before_cables - dc;
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
        &mut b.horizon_kwh,
        &mut b.shading_kwh,
        &mut b.iam_kwh,
        &mut b.soiling_kwh,
        &mut b.low_light_kwh,
        &mut b.temperature_kwh,
        &mut b.mismatch_kwh,
        &mut b.wiring_mismatch_kwh,
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
